//! Healing a version that does not start here: a newer release, else the version before it,
//! else safe mode, else telling the player - each step said in a message box.

use crate::dialog::{Message, show};
use crate::journal::{Journal, Kind};
use crate::process::alive;
use omsi_platform::recovery;
use omsi_platform::updater;

/// Where the healing runs: in the program at its start (before any window), or in the
/// watcher after the launcher it watched failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    AtStart,
    AfterFailure,
}

/// Why this start should be healed before it opens anything, if it should: this version
/// failed here before (and its watcher could not heal it), or the project lists it as broken.
pub fn due_at_start(version: &str) -> Option<String> {
    // (this is the safe mode's own start: what failed before is known, let it try)
    if std::env::var_os("OMSI_SAFE_MODE").is_some() {
        return None;
    }
    let j = Journal::load();
    if j.gave_up.iter().any(|v| v == version) {
        return None;
    }
    if let Some((bad, why)) = &j.remote_bad {
        if bad == version {
            return Some(format!("the project lists {version} as broken ({why})"));
        }
    }
    if j.broken(Kind::Launcher, version, alive) {
        return Some(format!("the last starts of {version} failed here"));
    }
    // (a game session that crashes before its first frame may be one map or bus: only two
    // in a row, of a version none of whose sessions ever ran here, count)
    let (failures, ever) = j.record(Kind::Game, version, alive);
    if failures >= 2 && !ever {
        return Some(format!(
            "the last game sessions of {version} failed to start here"
        ));
    }
    None
}

/// The program to start again: the installed one (its new or old file after the step), else
/// this very file.
fn program() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    if let Ok(place) = updater::install_place() {
        return Some((place.exe, place.dir));
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    Some((exe, dir))
}

/// Start the program again (the launcher), in safe mode or not.
fn relaunch(safe: bool) -> bool {
    let Some((exe, dir)) = program() else {
        return false;
    };
    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(&dir).env_remove("OMSI_GUARD_STATE");
    if safe {
        cmd.env("OMSI_SAFE_MODE", "1");
    } else {
        cmd.env_remove("OMSI_SAFE_MODE");
    }
    match cmd.spawn() {
        Ok(_) => {
            crate::note(&format!(
                "started {} again{}",
                exe.display(),
                if safe { " in safe mode" } else { "" }
            ));
            true
        }
        Err(e) => {
            crate::note(&format!("could not start {} again: {e}", exe.display()));
            false
        }
    }
}

/// Heal this version: true when another start took this one's place (the caller ends).
pub fn run(reason: &str, mode: Mode) -> bool {
    let version = updater::current_version().to_string();
    let journal = Journal::load();
    let listed = journal
        .remote_bad
        .as_ref()
        .filter(|(v, _)| *v == version)
        .map(|(_, why)| why.clone());
    crate::note(&format!("healing {version} ({mode:?}): {reason}"));
    // 1. a newer release (its program passes its self-test, or it is not installed); the
    // version left behind is broken here, and never gone back to
    match recovery::rescue_update() {
        Ok(Some(r)) => {
            recovery::mark_bad(&version);
            show(&match &listed {
                Some(why) => Message::ReplacedBroken {
                    from: version.clone(),
                    to: r.version.clone(),
                    why: why.clone(),
                },
                None => Message::Updated {
                    from: version.clone(),
                    to: r.version.clone(),
                },
            });
            crate::journal::update(|j| j.remote_bad = None);
            return relaunch(false);
        }
        Ok(None) => crate::note("no newer release"),
        Err(e) => crate::note(&format!("the newer release could not be installed: {e:#}")),
    }
    // 2. the version before the last update, kept for this
    if let Ok(place) = updater::install_place() {
        if recovery::previous_version(&place).is_some_and(|prev| !recovery::is_bad(&prev)) {
            match recovery::rollback(&place) {
                Ok(prev) => {
                    show(&match &listed {
                        Some(why) => Message::ReplacedBroken {
                            from: version.clone(),
                            to: prev.clone(),
                            why: why.clone(),
                        },
                        None => Message::WentBack {
                            from: version.clone(),
                            to: prev.clone(),
                        },
                    });
                    crate::journal::update(|j| j.remote_bad = None);
                    return relaunch(false);
                }
                Err(e) => crate::note(&format!("could not go back: {e:#}")),
            }
        }
    }
    // a listed version that starts: nothing better to go to yet (asked again next time)
    if listed.is_some() {
        crate::note("listed as broken, but no newer or earlier version is at hand");
        return false;
    }
    // 3. once more in safe mode
    if !journal.safe_tried.contains(&version) {
        crate::journal::update(|j| j.safe_tried.push(version.clone()));
        show(&Message::SafeMode {
            version: version.clone(),
        });
        return relaunch(true);
    }
    // 4. nothing helped: say what to do (once per version; the next start just tries)
    crate::journal::update(|j| j.gave_up.push(version.clone()));
    let log = recovery::guard_dir()
        .join("guard.log")
        .display()
        .to_string();
    show(&Message::GiveUp { version, log });
    false
}
