//! The start guard: a release that breaks the start must not strand the player.
//!
//! A release once broke the start - a crash before the window, a launcher hanging in the
//! background - and the updater, which lives in the launcher, never ran: every player had to
//! fetch a fixed build by hand. The guard runs before anything that could break and needs
//! neither the graphics card nor a window:
//!
//! * **A watcher beside every launcher start** (`start`): the program's very first act is to
//!   start itself once more as a small process without a window (`--guard-watch`), which
//!   watches the launcher until it has drawn its first frames (`healthy`). A launcher that
//!   ends on a crash or an error before that, or has not got there after
//!   `OMSI_GUARD_DEADLINE` seconds (it is then ended), failed its start. After a good start
//!   the watcher is gone; a crash later (in a drive) is no broken release.
//! * **The start journal** (`journal`): every start, whether it got to its first frames, and
//!   how it ended - also the game sessions the launcher starts, and starts no watcher saw
//!   (a phone, a watcher that could not run): a start that never got there and whose process
//!   is gone failed.
//! * **Healing** (`rescue`), when a version fails here: a newer release is installed at once
//!   (its program tried with `--self-test` before it replaces anything); else the version
//!   before the last update, kept until the new one has started well once, is put back; else
//!   one more start in safe mode (the basic render pipelines, no plugins); else the player
//!   is told what to do. Every step is said in a message box that needs no graphics card.
//!   The version that failed is never installed again here, only a newer one.
//! * **The project's list of broken releases** (`release/health.json` on GitHub): a version
//!   listed there is healed at the next start even when it starts well (subtle damage).
//!
//! A phone has no second process: there the journal, the update and the safe mode apply at
//! the next start (going back is the system's business, not ours).
//!
//! The guard keeps its own log, `guard/guard.log` in the data folder, since it works where the
//! game's log is not set up yet (or never will be).

pub mod dialog;
pub mod journal;
pub mod launcher_link;
mod process;
pub mod rescue;
mod watch;

use std::path::PathBuf;

use journal::{End, Kind};

/// Keep the newer half of a UTF-8 log without slicing through a character.
fn log_tail(text: &str) -> &str {
    let mut start = text.len() / 2;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// The guard's own log (append, small): what it saw and did.
pub(crate) fn note(text: &str) {
    use std::io::Write;
    let path = omsi_platform::recovery::guard_dir().join("guard.log");
    // (kept short: half of it goes once it passes 256 kB)
    if std::fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        if let Ok(t) = std::fs::read_to_string(&path) {
            let _ = std::fs::write(&path, log_tail(&t));
        }
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{secs} pid {}] {text}", std::process::id());
    }
}

/// What the program does first of all, in `main`: answers `--self-test` (the updater tries a
/// new program with it before installing it) and runs the watcher (`--guard-watch`). Some
/// exit code when the process has nothing more to do.
pub fn early() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--self-test") => Some(self_test()),
        Some("--guard-watch") => Some(watch::run(&args[1..])),
        _ => None,
    }
}

/// `--self-test`: the program could be loaded and run (its libraries found, its own folder
/// readable) - the one breakage a watcher in the same program cannot catch, a build that
/// does not even start, shows here before the update replaces anything.
fn self_test() -> i32 {
    let version = omsi_platform::VERSION;
    let ok = !version.is_empty() && std::env::current_exe().ok().and_then(|e| e.parent().map(|d| std::fs::read_dir(d).is_ok())).unwrap_or(false);
    if ok {
        println!("openomsi {version} build {} self-test ok", omsi_platform::BUILD);
        0
    } else {
        println!("openomsi {version}: self-test failed (the program's own folder cannot be read)");
        1
    }
}

/// Safe mode (set by the guard after failed starts, `OMSI_SAFE_MODE`): the switches that make a
/// start as light as it gets. Called in `main` before any flag is read (the flags take the
/// environment once).
pub fn apply_safe_mode() {
    if std::env::var_os("OMSI_SAFE_MODE").is_none() {
        return;
    }
    for flag in ["OMSI_BASIC_PIPELINES", "OMSI_NO_PLUGINS"] {
        if std::env::var_os(flag).is_none() {
            // SAFETY of the environment: the first thing in `main`, before any other thread
            unsafe { std::env::set_var(flag, "1") };
        }
    }
}

/// Whether this program file is a development build (a cargo `target` folder), which the guard
/// leaves alone unless `OMSI_GUARD_FORCE` is set: it cannot update itself anyway.
fn dev_build() -> bool {
    std::env::current_exe().map(|e| omsi_platform::updater::is_dev_build(&e)).unwrap_or(false)
}

/// The variable that hands the start's state file to the process it watches (and only to it:
/// it names that process).
const STATE_VAR: &str = "OMSI_GUARD_STATE";

/// This start's state file, when this process is a start the guard keeps a record of.
fn own_state() -> Option<PathBuf> {
    let v = std::env::var(STATE_VAR).ok()?;
    let (pid, path) = v.split_once(':')?;
    (pid.parse::<u32>().ok()? == std::process::id()).then(|| PathBuf::from(path))
}

/// A start of `kind` begins: the journal notes it, and a launcher start gets its watcher - or,
/// when this version already failed here (or the project lists it as broken), is healed right
/// now, before any window. Call it as early as possible (for the launcher: first thing in
/// `main`); `false` when this process should end (another one was started in its place).
pub fn start(kind: Kind) -> bool {
    if omsi_cfg::flags::OMSI_NO_GUARD.is_set() || (dev_build() && !omsi_cfg::flags::OMSI_GUARD_FORCE.is_set()) {
        return true;
    }
    // (a start the guard already noted: the same process after macOS' restart with its
    // allocator settings, which keeps the pid and the environment)
    if own_state().is_some() {
        return true;
    }
    let version = omsi_platform::VERSION;
    if kind == Kind::Launcher {
        if let Some(reason) = rescue::due_at_start(version) {
            note(&format!("start of {version}: {reason}"));
            if rescue::run(&reason, rescue::Mode::AtStart) {
                return false;
            }
        }
    }
    let pid = std::process::id();
    let state = omsi_platform::recovery::guard_dir().join(format!("start-{pid}.state"));
    let _ = std::fs::write(&state, "begun");
    journal::update(|j| j.begin(kind, version, omsi_platform::BUILD, pid));
    // SAFETY of the environment: called first thing in `main`, before any other thread
    unsafe { std::env::set_var(STATE_VAR, format!("{pid}:{}", state.display())) };
    if kind == Kind::Launcher && cfg!(not(target_os = "android")) {
        watch::spawn(pid, &state);
    }
    true
}

/// This start got to its first frames: the watcher may go, the journal says so, and the files
/// the last update set aside (kept to go back to) are no longer needed.
pub fn healthy(kind: Kind) {
    static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ONCE.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if let Some(state) = own_state() {
        let _ = std::fs::write(&state, "healthy");
        journal::update(|j| j.mark_healthy(std::process::id()));
        note(&format!("{kind:?} of {} started well", omsi_platform::VERSION));
    }
    // a version that runs needs no way back any more: the watcher drops it once it knows the
    // project does not list this version as broken; without a watcher it goes now
    #[cfg(not(target_os = "android"))]
    if kind == Kind::Launcher && own_state().is_none() {
        omsi_platform::updater::cleanup_after_update();
    }
}

/// The process ends on its own (`ok`: without an error): the watcher knows it was no crash.
pub fn exited(ok: bool) {
    if let Some(state) = own_state() {
        let healthy = std::fs::read_to_string(&state).map(|s| s.trim() == "healthy").unwrap_or(false);
        let _ = std::fs::write(&state, if ok { "exited-ok" } else { "exited-error" });
        if !healthy {
            journal::update(|j| j.end(std::process::id(), if ok { End::Clean } else { End::Error }));
        } else {
            journal::update(|j| j.end(std::process::id(), End::Clean));
        }
    }
}

/// A test start that breaks on purpose (`OMSI_GUARD_TEST_BREAK` and a file `guard-test-break`
/// beside the program saying how: `panic`, `exit` or `hang`), before its first frames - for
/// scripts/guard-e2e.sh. The file travels with a release archive and goes back with it.
pub fn test_break() {
    if !omsi_cfg::flags::OMSI_GUARD_TEST_BREAK.is_set() {
        return;
    }
    let Some(dir) = omsi_platform::updater::install_place().ok().map(|p| p.dir).or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from))) else { return };
    let how = std::fs::read_to_string(dir.join("guard-test-break")).unwrap_or_default();
    match how.trim() {
        "panic" => panic!("guard test: this start breaks on purpose"),
        "exit" => std::process::exit(3),
        "hang" => loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        },
        _ => {}
    }
}

#[cfg(test)]
mod utf8_log_tests {
    use super::log_tail;

    #[test]
    fn trims_at_a_valid_utf8_boundary() {
        assert_eq!(log_tail("abcdef"), "def");
        assert_eq!(log_tail("a€bb"), "bb");
        assert_eq!(log_tail(""), "");
    }
}
