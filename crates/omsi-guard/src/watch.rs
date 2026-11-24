//! The watcher: a second start of the program without a window (`--guard-watch <pid>
//! <state file>`), beside a launcher until it has drawn its first frames.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::journal::{End, Journal, Kind};
use crate::process::{alive, kill};

/// Start the watcher of the launcher `pid` (its state in `state`).
pub fn spawn(pid: u32, state: &Path) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--guard-watch")
        .arg(pid.to_string())
        .arg(state)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // (no console of its own, and none shared: CREATE_NO_WINDOW | DETACHED_PROCESS)
        cmd.creation_flags(0x0800_0000 | 0x0000_0008);
    }
    match cmd.spawn() {
        Ok(c) => crate::note(&format!("watching the launcher {pid} (watcher {})", c.id())),
        Err(e) => crate::note(&format!("no watcher for the launcher {pid}: {e}")),
    }
}

fn read(state: &Path) -> String {
    std::fs::read_to_string(state)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// What a state file says about how the start went: None while it runs.
fn verdict(s: &str) -> Option<Option<End>> {
    match s {
        "healthy" | "exited-ok" => Some(None),
        "exited-error" => Some(Some(End::Error)),
        _ => None,
    }
}

/// The watcher's whole life: wait for the launcher's first frames, its end or the deadline;
/// heal after a failed start.
pub fn run(args: &[String]) -> i32 {
    let (Some(pid), Some(state)) = (
        args.first().and_then(|p| p.parse::<u32>().ok()),
        args.get(1),
    ) else {
        return 2;
    };
    let state = Path::new(state);
    let version = omsi_platform::VERSION.to_string();
    let deadline = Duration::from_secs_f64(
        omsi_cfg::flags::OMSI_GUARD_DEADLINE
            .var()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|s| *s > 0.0)
            .unwrap_or(120.0),
    );
    // the project's list of broken releases, asked meanwhile (never holding anything up)
    let listed = {
        let version = version.clone();
        std::thread::spawn(move || omsi_platform::recovery::remotely_bad(&version))
    };
    let started = Instant::now();
    let outcome = loop {
        if let Some(v) = verdict(&read(state)) {
            break v;
        }
        if !alive(pid) {
            // (it may have said how it went just before it ended)
            break verdict(&read(state)).unwrap_or(Some(End::Crash));
        }
        if started.elapsed() > deadline {
            kill(pid);
            break Some(End::Hang);
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let _ = std::fs::remove_file(state);
    let Some(end) = outcome else {
        // a good start: the list's answer, if it comes soon, is kept for the next start
        let until = Instant::now() + Duration::from_secs(6);
        while !listed.is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
        let verdict = if listed.is_finished() {
            listed.join().ok().flatten()
        } else {
            None
        };
        match verdict {
            // (the way back stays: the next start takes it)
            Some(why) => {
                crate::note(&format!("the project lists {version} as broken: {why}"));
                crate::journal::update(|j| j.remote_bad = Some((version.clone(), why)));
            }
            // a version that runs needs no way back any more
            None => {
                #[cfg(not(target_os = "android"))]
                omsi_platform::updater::cleanup_now();
            }
        }
        return 0;
    };
    crate::journal::update(|j| j.end(pid, end));
    crate::note(&format!(
        "the launcher {pid} of {version} failed to start ({end:?}) after {:.1} s",
        started.elapsed().as_secs_f64()
    ));
    let j = Journal::load();
    if j.gave_up.contains(&version) {
        return 0;
    }
    if j.broken(Kind::Launcher, version.as_str(), alive) {
        crate::rescue::run(
            &format!(
                "the launcher {}",
                match end {
                    End::Hang => "hung before its first frames",
                    End::Error => "ended on an error before its first frames",
                    _ => "crashed before its first frames",
                }
            ),
            crate::rescue::Mode::AfterFailure,
        );
    } else {
        crate::note("one failed start of a version that ran here before: not healed yet");
    }
    0
}
