//! What the start guard (`omsi-guard`) needs of the updater to heal a broken release on its
//! own: the version an update replaced, kept until the new one has started well once, and put
//! back (`rollback`); the versions that broke on this computer, never offered again
//! (`mark_bad`, `is_bad`); the new program tried before it replaces the old one
//! (`self_test`, used by `updater::install_archive`); the project's list of releases known to
//! be broken (`remotely_bad`); and an update installed at once, without a window
//! (`rescue_update`).
//!
//! Nothing here needs the graphics card or a window: the guard runs it in a process that has
//! neither.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::updater::{self, Place, Release};

/// The project's list of broken releases, read from the repository's main branch.
const HEALTH_URL: &str = "https://raw.githubusercontent.com/openOMSI-org/openOMSI/main/release/health.json";

/// Beside the installed program: the version the `*.old-update` files belong to.
pub(crate) const PREVIOUS: &str = ".openomsi-previous";
/// What a rollback sets aside (deleted with the `*.old-update` files once a start went well).
pub(crate) const BAD: &str = ".bad-update";

/// The guard's own folder in the data folder (the journal, the bad versions); the tests' own
/// folder in a test, so that they never mark anything in the player's.
pub fn guard_dir() -> PathBuf {
    #[cfg(test)]
    let d = std::env::temp_dir().join(format!("openomsi-guard-test-{}", std::process::id()));
    #[cfg(not(test))]
    let d = omsi_launcher_lib::data_dir().join("guard");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn bad_file() -> PathBuf {
    guard_dir().join("bad-versions")
}

/// The versions that failed to start on this computer (one per line).
pub fn bad_versions() -> Vec<String> {
    std::fs::read_to_string(bad_file()).map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()).unwrap_or_default()
}

/// Whether `version` failed to start here, or would not even run its self-test: it is not
/// offered again, only a newer release is.
pub fn is_bad(version: &str) -> bool {
    let v = version.trim().trim_start_matches(['v', 'V']);
    bad_versions().iter().any(|b| b == v)
}

/// Remember that `version` is broken on this computer.
pub fn mark_bad(version: &str) {
    let v = version.trim().trim_start_matches(['v', 'V']).to_string();
    if v.is_empty() || is_bad(&v) {
        return;
    }
    let mut list = bad_versions();
    list.push(v.clone());
    // (the newest few are enough: an older bad version is never newer than what is installed)
    let keep = list.len().saturating_sub(20);
    let _ = write_atomic(&bad_file(), &(list[keep..].join("\n") + "\n"));
    log::warn!("guard: version {v} is marked broken on this computer; it will not be installed again");
}

/// Write `text` to `path` through a temporary file and a rename, so a reader never sees half.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

// --- the program tried before it goes in -------------------------------------------------------

/// The new program did not pass its self-test: it was not installed.
#[derive(Debug)]
pub struct SelfTestFailed(pub String);

impl std::fmt::Display for SelfTestFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the new version does not start on this computer ({})", self.0)
    }
}

impl std::error::Error for SelfTestFailed {}

/// The program an unpacked release holds in `staging`: `openomsi(.exe)` at its top, or the
/// one inside its `.app` on macOS.
pub(crate) fn staged_program(staging: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "openomsi.exe" } else { "openomsi" };
    if staging.join(exe).is_file() {
        return Some(staging.join(exe));
    }
    std::fs::read_dir(staging).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))).map(|app| app.join("Contents").join("MacOS").join("openomsi")).filter(|p| p.is_file())
}

/// Start `program --self-test` and wait for it (at most `limit`): it must exit 0 and say
/// `self-test ok`. A program that cannot even be loaded - a library missing, a broken build -
/// fails here, before it has replaced anything.
pub fn self_test(program: &Path, limit: Duration) -> Result<(), SelfTestFailed> {
    let out = std::env::temp_dir().join(format!("openomsi-self-test-{}.txt", std::process::id()));
    let file = std::fs::File::create(&out).map_err(|e| SelfTestFailed(format!("no file for its answer: {e}")))?;
    let mut cmd = std::process::Command::new(program);
    cmd.arg("--self-test").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.stdout(file);
    // (its own folder, as a player's start: it looks for what lies beside it)
    if let Some(dir) = program.parent() {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().map_err(|e| SelfTestFailed(format!("it could not be started: {e}")))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&out);
                return Err(SelfTestFailed(format!("no answer within {} s", limit.as_secs())));
            }
            Err(e) => return Err(SelfTestFailed(format!("{e}"))),
        }
    };
    let said = std::fs::read_to_string(&out).unwrap_or_default();
    let _ = std::fs::remove_file(&out);
    if !status.success() {
        return Err(SelfTestFailed(format!("it ended with {status}")));
    }
    if !said.contains("self-test ok") {
        return Err(SelfTestFailed("it did not answer its self-test".to_string()));
    }
    Ok(())
}

// --- the version kept for going back -----------------------------------------------------------

/// The version the files set aside by the last update belong to, when they are still there.
pub fn previous_version(place: &Place) -> Option<String> {
    let v = std::fs::read_to_string(place.dir.join(PREVIOUS)).ok()?.trim().to_string();
    (!v.is_empty() && !asides(&place.dir).is_empty()).then_some(v)
}

/// The `*.old-update` files in `dir` by the name they were set aside from; of several
/// (`x.old-update`, `x.2.old-update`: an older one could not be deleted at the time) the one
/// with the highest number, the last set aside.
fn asides(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, u32, PathBuf)> = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".old-update") else { continue };
        let (base, n) = match stem.rsplit_once('.') {
            Some((head, n)) if !head.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => (head.to_string(), n.parse().unwrap_or(1)),
            _ => (stem.to_string(), 1),
        };
        match found.iter_mut().find(|f| f.0 == base) {
            Some(f) if n > f.1 => {
                f.1 = n;
                f.2 = e.path();
            }
            Some(_) => {}
            None => found.push((base, n, e.path())),
        }
    }
    found.into_iter().map(|(b, _, p)| (b, p)).collect()
}

fn aside_bad(p: &Path) -> std::io::Result<()> {
    let mut s = p.as_os_str().to_os_string();
    s.push(BAD);
    let bad = PathBuf::from(s);
    remove_any(&bad);
    // (Windows lets a running program be renamed, not deleted: the guard runs from it)
    std::fs::rename(p, &bad)
}

fn remove_any(p: &Path) {
    if p.is_dir() && !p.is_symlink() {
        let _ = std::fs::remove_dir_all(p);
    } else {
        let _ = std::fs::remove_file(p);
    }
}

/// Put back the version the last update replaced: every file it set aside returns, what that
/// update brought new goes aside, and the version gone back from is marked broken here.
/// Returns the version now installed.
pub fn rollback(place: &Place) -> anyhow::Result<String> {
    let Some(previous) = previous_version(place) else {
        anyhow::bail!("no earlier version is kept");
    };
    let from = updater::current_version().to_string();
    let back = asides(&place.dir);
    // what the update brought that was not there before
    let installed: Vec<String> = std::fs::read_to_string(place.dir.join(updater::MANIFEST)).map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && !l.contains(['/', '\\'])).collect()).unwrap_or_default();
    for name in &installed {
        let target = updater::target_of(name, place);
        let returns = back.iter().any(|(b, _)| place.dir.join(b) == target || b.eq_ignore_ascii_case(name));
        if !returns && (target.exists() || target.is_symlink()) {
            aside_bad(&target).map_err(|e| anyhow::anyhow!("{} could not be set aside ({e})", target.display()))?;
        }
    }
    for (base, old) in &back {
        let target = place.dir.join(base);
        if target.exists() || target.is_symlink() {
            aside_bad(&target).map_err(|e| anyhow::anyhow!("{} could not be set aside ({e})", target.display()))?;
        }
        std::fs::rename(old, &target).map_err(|e| anyhow::anyhow!("{} could not be put back ({e})", target.display()))?;
    }
    // (the manifest came back with the files when the earlier update wrote one; else none)
    if !back.iter().any(|(b, _)| b == updater::MANIFEST) {
        let _ = std::fs::remove_file(place.dir.join(updater::MANIFEST));
    }
    let _ = std::fs::remove_file(place.dir.join(PREVIOUS));
    mark_bad(&from);
    log::warn!("guard: went back from {from} to {previous} in {}", place.dir.display());
    Ok(previous)
}

// --- the project's list of broken releases -------------------------------------------------------

/// When the project lists `version` as broken (`release/health.json` on main: `{"bad":
/// ["0.2.29"], "message": "..."}`), its message (or an empty one). A short wait at most;
/// no answer (offline, GitHub down) is no verdict.
pub fn remotely_bad(version: &str) -> Option<String> {
    listed_bad(&health()?, version)
}

pub fn listed_broken() -> Vec<String> {
    health().map(|t| broken_of(&t)).unwrap_or_default()
}

fn health() -> Option<String> {
    let url = omsi_cfg::flags::OMSI_HEALTH_URL.var().map(str::to_string).unwrap_or_else(|| HEALTH_URL.to_string());
    if let Some(p) = url.strip_prefix("file://") {
        return std::fs::read_to_string(p).ok();
    }
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(4)).timeout_read(Duration::from_secs(4)).user_agent(&format!("openOMSI/{} (guard)", updater::current_version())).build();
    agent.get(&url).call().ok()?.into_string().ok()
}

fn broken_of(text: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    v["bad"].as_array().into_iter().flatten().filter_map(|b| b.as_str()).map(|b| b.trim().trim_start_matches(['v', 'V']).to_string()).filter(|b| !b.is_empty()).collect()
}

/// `version` in a `health.json`'s `bad` list: its message.
fn listed_bad(text: &str, version: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let want = version.trim().trim_start_matches(['v', 'V']);
    let listed = v["bad"].as_array()?.iter().filter_map(|b| b.as_str()).any(|b| b.trim().trim_start_matches(['v', 'V']) == want);
    listed.then(|| v["message"].as_str().unwrap_or("").to_string())
}

// --- an update installed at once ---------------------------------------------------------------

/// Look for a newer release and install it now, without a window (the guard's rescue): the
/// release it went to, None when there is none (or the update check is not reachable). A
/// release whose program fails its self-test is marked broken here and not installed.
pub fn rescue_update() -> anyhow::Result<Option<Release>> {
    let r = match updater::latest() {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(e) => {
            log::warn!("guard: no update check possible: {e:#}");
            return Ok(None);
        }
    };
    log::info!("guard: installing {} to replace a version that does not start", r.version);
    match updater::install_now(&r) {
        Ok(()) => Ok(Some(r)),
        Err(e) => {
            if e.downcast_ref::<SelfTestFailed>().is_some() {
                mark_bad(&r.version);
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!("openomsi-recovery-{name}-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_last_aside_of_a_file_is_the_one_with_the_highest_number() {
        let d = temp("asides");
        for f in ["openomsi.old-update", "openomsi.2.old-update", "LICENSE.old-update", "README.md"] {
            std::fs::write(d.join(f), f).unwrap();
        }
        let mut a = asides(&d);
        a.sort();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].0, "LICENSE");
        assert_eq!(a[1].0, "openomsi");
        assert!(a[1].1.ends_with("openomsi.2.old-update"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn going_back_puts_the_old_files_in_place_and_sets_the_new_ones_aside() {
        let d = temp("rollback");
        let place = Place { dir: d.clone(), bundle: None, exe: d.join("openomsi") };
        // as an update leaves it: the new program and a file it brought, the old program aside
        std::fs::write(d.join("openomsi"), "new").unwrap();
        std::fs::write(d.join("new-lib.dll"), "new lib").unwrap();
        std::fs::write(d.join("openomsi.old-update"), "old").unwrap();
        std::fs::write(d.join("gone.txt.old-update"), "a file the update took away").unwrap();
        std::fs::write(d.join(updater::MANIFEST), "openomsi\nnew-lib.dll\n").unwrap();
        std::fs::write(d.join(PREVIOUS), "0.2.29").unwrap();
        std::fs::write(d.join("Mods-of-the-player.txt"), "mine").unwrap();
        assert_eq!(previous_version(&place).as_deref(), Some("0.2.29"));
        let back = rollback(&place).unwrap();
        assert_eq!(back, "0.2.29");
        assert_eq!(std::fs::read_to_string(d.join("openomsi")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(d.join("gone.txt")).unwrap(), "a file the update took away");
        assert!(!d.join("new-lib.dll").exists());
        assert!(d.join("new-lib.dll.bad-update").exists());
        assert!(d.join("openomsi.bad-update").exists());
        assert!(!d.join(PREVIOUS).exists());
        assert_eq!(std::fs::read_to_string(d.join("Mods-of-the-player.txt")).unwrap(), "mine");
        assert!(previous_version(&place).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// An update installed the way the updater does it, and taken back: the old program and
    /// the file the update took away return, the file it brought goes, the list of what an
    /// update brought is the old one again, and the version gone back from is not offered.
    #[test]
    fn an_update_installed_and_taken_back_leaves_the_folder_as_it_was() {
        use std::io::Write;
        let d = temp("roundtrip");
        std::fs::write(d.join("openomsi"), "old game").unwrap();
        std::fs::write(d.join("retired.dll"), "old lib").unwrap();
        std::fs::write(d.join(updater::MANIFEST), "openomsi\nretired.dll\n").unwrap();
        std::fs::create_dir_all(d.join("Vehicles")).unwrap();
        std::fs::write(d.join("Vehicles/bus.bus"), "mod").unwrap();
        let zip_path = d.join("new.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            for (n, body) in [("openomsi", "new game"), ("added.dll", "new lib")] {
                z.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        let place = Place { dir: d.clone(), bundle: None, exe: d.join("openomsi") };
        updater::install_archive_with(&zip_path, &place, false).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("openomsi")).unwrap(), "new game");
        assert!(!d.join("retired.dll").exists());
        assert_eq!(previous_version(&place).as_deref(), Some(updater::current_version()));
        rollback(&place).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("openomsi")).unwrap(), "old game");
        assert_eq!(std::fs::read_to_string(d.join("retired.dll")).unwrap(), "old lib");
        assert!(!d.join("added.dll").exists());
        assert_eq!(std::fs::read_to_string(d.join(updater::MANIFEST)).unwrap(), "openomsi\nretired.dll\n");
        assert_eq!(std::fs::read_to_string(d.join("Vehicles/bus.bus")).unwrap(), "mod");
        assert!(is_bad(updater::current_version()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nothing_to_go_back_to_is_said() {
        let d = temp("nothing");
        let place = Place { dir: d.clone(), bundle: None, exe: d.join("openomsi") };
        std::fs::write(d.join("openomsi"), "only").unwrap();
        assert!(rollback(&place).is_err());
        assert_eq!(std::fs::read_to_string(d.join("openomsi")).unwrap(), "only");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_does_not_answer_its_self_test_fails_it() {
        use std::os::unix::fs::PermissionsExt;
        let d = temp("selftest");
        let script = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        assert!(self_test(&script("good", "echo 'openomsi 1.0 self-test ok'"), Duration::from_secs(10)).is_ok());
        assert!(self_test(&script("crash", "exit 3"), Duration::from_secs(10)).is_err());
        assert!(self_test(&script("silent", "exit 0"), Duration::from_secs(10)).is_err());
        let hang = self_test(&script("hang", "sleep 30"), Duration::from_millis(500));
        assert!(hang.unwrap_err().0.contains("no answer"));
        assert!(self_test(&d.join("missing"), Duration::from_secs(1)).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_projects_list_names_a_broken_release() {
        let text = r#"{"bad": ["0.2.29", "v0.2.31"], "message": "0.2.29 does not start on Windows"}"#;
        assert_eq!(listed_bad(text, "0.2.29").as_deref(), Some("0.2.29 does not start on Windows"));
        assert!(listed_bad(text, "v0.2.31").is_some());
        assert!(listed_bad(text, "0.2.30").is_none());
        assert!(listed_bad(r#"{"bad": []}"#, "0.2.29").is_none());
        assert!(listed_bad("not json", "0.2.29").is_none());
        assert_eq!(broken_of(text), ["0.2.29", "0.2.31"]);
        assert!(broken_of(r#"{"bad": [" ", 3]}"#).is_empty());
        assert!(broken_of("not json").is_empty());
    }
}
