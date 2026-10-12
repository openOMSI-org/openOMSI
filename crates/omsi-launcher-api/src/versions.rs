use crate::{err, Answer, Platform};
use omsi_launcher_lib as core;
use omsi_platform::recovery;
use omsi_platform::updater::{self, Listed, Status, Updater};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

struct Job {
    updater: Updater,
    version: String,
    keep: bool,
    ready: bool,
    started: std::time::Instant,
}

static LISTED: Mutex<Vec<Listed>> = Mutex::new(Vec::new());
static JOB: Mutex<Option<Job>> = Mutex::new(None);
static SWAPPED: AtomicBool = AtomicBool::new(false);

const CHOSEN: &str = "chosen-version";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn available() -> bool {
    !updater::STORE_BUILD && cfg!(not(any(target_os = "android", target_os = "ios")))
}

fn chosen_file() -> PathBuf {
    core::data_dir().join(CHOSEN)
}

fn chosen() -> Option<(String, bool)> {
    chosen_of(&std::fs::read_to_string(chosen_file()).ok()?)
}

fn chosen_of(text: &str) -> Option<(String, bool)> {
    let mut lines = text.lines().map(str::trim);
    let version = lines.next().filter(|v| !v.is_empty())?.to_string();
    Some((version, lines.next() != Some("0")))
}

fn after_choice(before: Option<(String, bool)>, checking: bool, version: &str, keep: bool) -> (bool, Option<String>) {
    let checked = before.map_or(checking, |(_, c)| c);
    if keep {
        (false, Some(format!("{version}\n{}\n", u8::from(checked))))
    } else {
        (checked, None)
    }
}

fn after_start(chosen: Option<(String, bool)>, current: &str, checking: bool) -> Option<bool> {
    chosen.filter(|(v, _)| v != current).map(|(_, c)| c || checking)
}

pub fn settle() {
    if SWAPPED.load(Ordering::Relaxed) {
        return;
    }
    let Ok(mut s) = core::get_settings() else { return };
    let Some(checking) = after_start(chosen(), updater::current_version(), s["update_check"].as_bool().unwrap_or(true)) else { return };
    s["update_check"] = json!(checking);
    if core::save_settings(&s).is_ok() {
        let _ = std::fs::remove_file(chosen_file());
    }
}

fn remember(version: &str, keep: bool) -> Result<(), String> {
    let mut s = core::get_settings().map_err(err)?;
    let (checking, mark) = after_choice(chosen(), s["update_check"].as_bool().unwrap_or(true), version, keep);
    if let Some(text) = &mark {
        recovery::write_atomic(&chosen_file(), text).map_err(err)?;
    }
    s["update_check"] = json!(checking);
    if let Err(e) = core::save_settings(&s) {
        if mark.is_some() {
            let _ = std::fs::remove_file(chosen_file());
        }
        return Err(err(e));
    }
    if mark.is_none() {
        let _ = std::fs::remove_file(chosen_file());
    }
    Ok(())
}

fn watch() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let mut job = lock(&JOB);
        let Some(j) = job.as_mut() else { return };
        match j.updater.status() {
            Status::Restarting(_) => {
                SWAPPED.store(true, Ordering::Relaxed);
                if let Err(e) = remember(&j.version, j.keep) {
                    core::log_to_file(&format!("launcher: the update checks could not be set for {}: {e}", j.version));
                }
                j.ready = true;
                return;
            }
            Status::Failed(_) => return,
            _ => {}
        }
    }
}

pub fn installing() -> bool {
    lock(&JOB).as_ref().is_some_and(|j| j.started.elapsed().as_secs() < 600 && (matches!(j.updater.status(), Status::Installing(_)) || (matches!(j.updater.status(), Status::Restarting(_)) && !j.ready)))
}

pub fn swapped() -> bool {
    SWAPPED.load(Ordering::Relaxed)
}

pub fn list() -> Answer {
    if !available() {
        return Err("openOMSI is installed by the store here".into());
    }
    let listed = updater::releases().map_err(err)?;
    let broken = recovery::listed_broken();
    let current = updater::current_version();
    let latest = listed.first().map(|l| l.release.version.clone());
    let rows: Vec<Value> = listed
        .iter()
        .map(|l| {
            let v = l.release.version.as_str();
            let broken = if recovery::is_bad(v) {
                Some("here")
            } else if broken.iter().any(|b| b == v) {
                Some("listed")
            } else {
                None
            };
            json!({
                "version": v,
                "date": l.published,
                "size": l.release.size,
                "page": l.release.page,
                "current": v == current,
                "latest": latest.as_deref() == Some(v),
                "broken": broken,
            })
        })
        .collect();
    *lock(&LISTED) = listed;
    Ok(json!({
        "current": current,
        "blocked": updater::install_place().err().map(err),
        "list": rows,
    }))
}

fn busy(s: &Status) -> bool {
    matches!(s, Status::Checking | Status::Downloading { .. } | Status::Installing(_) | Status::WaitingForInstaller(_) | Status::Restarting(_))
}

pub fn install(version: String) -> Answer {
    if !available() {
        return Err("openOMSI is installed by the store here".into());
    }
    let (release, latest) = {
        let listed = lock(&LISTED);
        let found = listed.iter().find(|l| l.release.version == version).ok_or("Open the list of versions again.")?;
        (found.release.clone(), listed.first().is_some_and(|l| l.release.version == version))
    };
    if version == updater::current_version() {
        return Err("This version is installed already.".into());
    }
    if recovery::is_bad(&version) {
        return Err("Did not start on this computer".into());
    }
    if core::list_instances().iter().any(|i| i.running) {
        return Err("Close the game first: openOMSI cannot be replaced while it runs.".into());
    }
    let mut job = lock(&JOB);
    if job.as_ref().is_some_and(|j| busy(&j.updater.status())) {
        return Err("Another version is being installed.".into());
    }
    let mut updater = Updater::default();
    updater.install(release);
    *job = Some(Job { updater, version, keep: !latest, ready: false, started: std::time::Instant::now() });
    drop(job);
    std::thread::spawn(watch);
    status()
}

pub fn status() -> Answer {
    let job = lock(&JOB);
    let Some(j) = job.as_ref() else { return Ok(json!({ "state": "idle" })) };
    let v = j.version.as_str();
    Ok(match j.updater.status() {
        Status::Downloading { done, total, .. } => json!({ "state": "downloading", "version": v, "done": done, "total": total }),
        Status::Restarting(_) if j.ready => json!({ "state": "installed", "version": v }),
        Status::Failed(e) => json!({ "state": "failed", "version": v, "error": reason(&e) }),
        Status::Installing(_) | Status::Restarting(_) => json!({ "state": "installing", "version": v }),
        _ => json!({ "state": "downloading", "version": v, "done": 0, "total": 0 }),
    })
}

fn reason(e: &str) -> &str {
    e.split_once(": ").filter(|(head, _)| head.starts_with("openOMSI was not updated to")).map_or(e, |(_, why)| why)
}

fn program(place: &updater::Place) -> PathBuf {
    let game = place.exe.with_file_name(if cfg!(windows) { "openomsi.exe" } else { "openomsi" });
    if game.is_file() {
        game
    } else {
        place.exe.clone()
    }
}

pub fn restart(p: &dyn Platform) -> Answer {
    let version = {
        let job = lock(&JOB);
        let j = job.as_ref().filter(|j| j.ready && matches!(j.updater.status(), Status::Restarting(_))).ok_or("No version is installed yet.")?;
        j.version.clone()
    };
    let place = updater::install_place().map_err(err)?;
    let exe = program(&place);
    std::process::Command::new(&exe).current_dir(&place.dir).env_remove("OMSI_LAUNCHER_INPUT").env_remove("OMSI_GUARD_STATE").env_remove("OMSI_SAFE_MODE").spawn().map_err(err)?;
    core::log_to_file(&format!("launcher: openOMSI {version} installed, {} starts", exe.display()));
    p.quit();
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chosen_version_is_read_back() {
        assert_eq!(chosen_of("0.2.33\n0\n"), Some(("0.2.33".into(), false)));
        assert_eq!(chosen_of("0.2.33\n1\n"), Some(("0.2.33".into(), true)));
        assert_eq!(chosen_of(" 0.2.33 "), Some(("0.2.33".into(), true)));
        assert_eq!(chosen_of("\n0\n"), None);
        assert_eq!(chosen_of(""), None);
    }

    #[test]
    fn an_older_version_keeps_itself_until_the_latest_is_chosen() {
        let (checking, mark) = after_choice(None, true, "0.2.33", true);
        assert_eq!((checking, mark.as_deref()), (false, Some("0.2.33\n1\n")));
        let first = chosen_of(mark.as_deref().unwrap());
        let (checking, mark) = after_choice(first.clone(), false, "0.2.34", true);
        assert_eq!((checking, mark.as_deref()), (false, Some("0.2.34\n1\n")));
        assert_eq!(after_choice(first, false, "0.2.38", false), (true, None));
        assert_eq!(after_choice(None, false, "0.2.38", false), (false, None));
        assert_eq!(after_choice(None, false, "0.2.33", true), (false, Some("0.2.33\n0\n".into())));
    }

    #[test]
    fn the_checks_come_back_once_another_version_runs() {
        assert_eq!(after_start(Some(("0.2.33".into(), true)), "0.2.33", false), None);
        assert_eq!(after_start(Some(("0.2.33".into(), true)), "0.2.38", false), Some(true));
        assert_eq!(after_start(Some(("0.2.33".into(), false)), "0.2.38", false), Some(false));
        assert_eq!(after_start(Some(("0.2.33".into(), false)), "0.2.38", true), Some(true));
        assert_eq!(after_start(None, "0.2.38", false), None);
    }

    #[test]
    fn a_failure_says_why_once() {
        assert_eq!(reason("openOMSI was not updated to 0.2.33: the download stopped at 5 of 9 bytes"), "the download stopped at 5 of 9 bytes");
        assert_eq!(reason("the folder x cannot be written"), "the folder x cannot be written");
    }
}
