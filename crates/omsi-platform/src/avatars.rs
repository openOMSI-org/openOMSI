//! The passenger avatars ("Character models: High quality"): the Microsoft Rocketbox avatars
//! (MIT) as `tools/avatars/build_pack.py` packs them - a folder with `pack.json`, one folder
//! per avatar with its FBX and textures, the licence and the credits. They are drawn on this
//! computer only, in place of the people of OMSI's content; nothing about them is sent to
//! anybody, and a LAN session's other players see their own people.

use std::path::{Path, PathBuf};

/// The avatars' archive (a pre-release of the project's own), its SHA-256 and size.
const PACK_URL: &str = "https://github.com/openOMSI-org/openOMSI/releases/download/avatars-2/openOMSI-avatars-2.zip";
const PACK_SHA256: &str = "fb6d40d34f51f1860b6b57934341f9bd032c987ceef8b4caf2aae63b8f21823d";
const PACK_BYTES: u64 = 326_209_348;

/// One avatar of the pack.
#[derive(Debug, Clone)]
pub struct Avatar {
    pub id: String,
    pub file: PathBuf,
    pub female: bool,
    pub child: bool,
}

#[derive(Debug, Clone)]
pub struct ClipFile {
    pub file: PathBuf,
    pub kind: String,
    pub female: bool,
}

const PACK_VERSION: u64 = 2;

fn version(dir: &Path) -> u64 {
    std::fs::read_to_string(dir.join("pack.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|j| j.get("version").and_then(|v| v.as_u64())).unwrap_or(0)
}

pub fn clips(dir: &Path) -> Vec<ClipFile> {
    let Some(json) = std::fs::read_to_string(dir.join("pack.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return Vec::new() };
    json.get("clips")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let file = dir.join(c.get("file")?.as_str()?);
            file.is_file().then(|| ClipFile { file, kind: c.get("kind").and_then(|k| k.as_str()).unwrap_or("").to_string(), female: c.get("female").and_then(|v| v.as_bool()).unwrap_or(false) })
        })
        .collect()
}

/// Where the pack is looked for: `OMSI_AVATARS_DIR`, then `avatars` beside the program (and
/// in the macOS bundle's resources), then in the user's openOMSI folder.
fn dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = omsi_cfg::flags::OMSI_AVATARS_DIR.var() {
        dirs.push(PathBuf::from(d));
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        dirs.push(exe.join("avatars"));
        dirs.push(exe.join("../Resources/avatars"));
    }
    dirs.extend(user_dir());
    dirs
}

/// Where a downloaded pack goes.
pub fn user_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| PathBuf::from(h).join(".openomsi").join("avatars"))
}

/// The folder of the pack, if there is one.
pub fn find_dir() -> Option<PathBuf> {
    if let Some(d) = omsi_cfg::flags::OMSI_AVATARS_DIR.var() {
        let d = PathBuf::from(d);
        return d.join("pack.json").is_file().then_some(d);
    }
    if let Some(d) = user_dir() {
        if let Err(e) = restore_backup(&d) {
            log::warn!("avatars: the backup pack could not be put back into {}: {e}", d.display());
        }
    }
    best(dirs())
}

// (an upgrade stopped between moving the pack aside and promoting the new one)
fn restore_backup(dir: &Path) -> std::io::Result<()> {
    let old = dir.with_extension("old");
    if !dir.exists() && old.join("pack.json").is_file() {
        std::fs::rename(&old, dir)?;
    }
    Ok(())
}

fn best(dirs: Vec<PathBuf>) -> Option<PathBuf> {
    dirs.into_iter().filter(|d| d.join("pack.json").is_file()).fold(None, |b: Option<PathBuf>, d| match b {
        Some(b) if version(&b) >= version(&d) => Some(b),
        _ => Some(d),
    })
}

fn needs_fetch(found: Option<&Path>) -> bool {
    omsi_cfg::flags::OMSI_AVATARS_DIR.var().is_none() && found.is_none_or(|d| version(d) < PACK_VERSION)
}

/// The avatars of the pack in `dir`.
pub fn load(dir: &Path) -> Vec<Avatar> {
    let Ok(text) = std::fs::read_to_string(dir.join("pack.json")) else { return Vec::new() };
    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(j) => j,
        Err(e) => {
            log::warn!("avatars: {}: {e}", dir.join("pack.json").display());
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for a in json.get("avatars").and_then(|a| a.as_array()).into_iter().flatten() {
        let (Some(id), Some(file)) = (a.get("id").and_then(|v| v.as_str()), a.get("file").and_then(|v| v.as_str())) else { continue };
        let file = dir.join(file);
        if !file.is_file() {
            continue;
        }
        let flag = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        out.push(Avatar { id: id.to_string(), file, female: flag("female"), child: flag("child") });
    }
    out
}

/// Whether the pack is being downloaded now (by this process).
pub fn fetching() -> bool {
    FETCHING.load(std::sync::atomic::Ordering::Relaxed)
}

static FETCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Fetch the pack in the background when there is none yet: the release asset `avatars-2`
/// of the project, checked and unpacked into the user's openOMSI folder. The avatars are
/// drawn from the next drive on.
pub fn fetch_if_missing() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if cfg!(target_os = "android") || !needs_fetch(find_dir().as_deref()) || STARTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(dir) = user_dir() else { return };
    let part = dir.with_extension("zip.part");
    // (another openOMSI process - the launcher, the game - is fetching it now)
    if std::fs::metadata(&part).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() < 120) {
        return;
    }
    FETCHING.store(true, std::sync::atomic::Ordering::Relaxed);
    std::thread::Builder::new()
        .name("avatars fetch".into())
        .spawn(move || {
            match fetch(&dir, &part) {
                Ok(()) => log::info!("avatars: the passenger avatars were downloaded into {}", dir.display()),
                Err(e) => {
                    let _ = std::fs::remove_file(&part);
                    log::warn!("avatars: the passenger avatars could not be downloaded: {e:#}");
                }
            }
            FETCHING.store(false, std::sync::atomic::Ordering::Relaxed);
        })
        .ok();
}

/// Download the archive to `part`, check it, unpack it beside `dir` and put it in its place.
fn fetch(dir: &Path, part: &Path) -> anyhow::Result<()> {
    use sha2::Digest;
    use std::io::{Read, Write};
    if let Some(d) = part.parent() {
        std::fs::create_dir_all(d)?;
    }
    log::info!("avatars: downloading the passenger avatars ({} MB) from {PACK_URL}", PACK_BYTES >> 20);
    let resp = crate::updater::agent().get(PACK_URL).call().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(part)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last_note) = (0u64, 0u64);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        done += n as u64;
        // (the file's time tells another process that it is being fetched)
        if done - last_note > 32 << 20 {
            last_note = done;
            out.flush()?;
            log::info!("avatars: {} of {} MB", done >> 20, PACK_BYTES >> 20);
        }
    }
    out.flush()?;
    drop(out);
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    anyhow::ensure!(got == PACK_SHA256, "the download is damaged ({done} bytes, SHA-256 {got})");
    let tmp = dir.with_extension("unpacking");
    let _ = std::fs::remove_dir_all(&tmp);
    crate::updater::unpack(part, &tmp)?;
    anyhow::ensure!(tmp.join("pack.json").is_file(), "the archive holds no pack.json");
    promote(&tmp, dir)?;
    let _ = std::fs::remove_file(part);
    Ok(())
}

fn promote(new: &Path, dir: &Path) -> anyhow::Result<()> {
    restore_backup(dir)?;
    let old = dir.with_extension("old");
    let _ = std::fs::remove_dir_all(&old);
    if dir.exists() {
        std::fs::rename(dir, &old)?;
    }
    if let Err(e) = std::fs::rename(new, dir) {
        if old.exists() {
            std::fs::rename(&old, dir)?;
        }
        return Err(e.into());
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The published pack downloads, checks and unpacks (network; run with `--ignored`).
    #[test]
    #[ignore]
    fn the_published_pack_is_fetched() {
        let dir = std::env::temp_dir().join(format!("omsi-avatars-fetch-{}", std::process::id()));
        let part = dir.with_extension("zip.part");
        super::fetch(&dir, &part).unwrap();
        let a = super::load(&dir);
        assert_eq!(a.len(), 91);
        assert_eq!(super::clips(&dir).len(), 34);
        assert_eq!(super::version(&dir), super::PACK_VERSION);
        assert!(!part.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pack_lists_its_clips_and_version() {
        let dir = std::env::temp_dir().join(format!("omsi-avatars-clips-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("clips")).unwrap();
        std::fs::write(dir.join("clips/w.fbx"), b"x").unwrap();
        std::fs::write(dir.join("pack.json"), r#"{"version":2,"avatars":[],"clips":[{"id":"w","file":"clips/w.fbx","kind":"walk","female":true},{"id":"g","file":"clips/g.fbx","kind":"sit"}]}"#).unwrap();
        let c = super::clips(&dir);
        assert_eq!(c.len(), 1);
        assert!(c[0].female && c[0].kind == "walk");
        assert_eq!(super::version(&dir), super::PACK_VERSION);
        let old = dir.with_extension("v1");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("pack.json"), r#"{"version":1,"avatars":[]}"#).unwrap();
        assert_eq!(super::best(vec![old.clone(), dir.clone()]), Some(dir.clone()), "the newer pack wins wherever it lies");
        if omsi_cfg::flags::OMSI_AVATARS_DIR.var().is_none() {
            assert!(super::needs_fetch(Some(&old)) && super::needs_fetch(None) && !super::needs_fetch(Some(&dir)));
        }
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&old);
    }

    #[test]
    fn a_failed_upgrade_keeps_the_installed_pack() {
        let dir = std::env::temp_dir().join(format!("omsi-avatars-promote-{}", std::process::id()));
        let new = dir.with_extension("unpacking");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pack.json"), r#"{"version":1,"avatars":[]}"#).unwrap();
        let _ = std::fs::remove_dir_all(&new);
        assert!(super::promote(&new, &dir).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("pack.json")).unwrap(), r#"{"version":1,"avatars":[]}"#);
        assert!(!dir.with_extension("old").exists());
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("pack.json"), r#"{"version":2,"avatars":[]}"#).unwrap();
        super::promote(&new, &dir).unwrap();
        assert_eq!(super::version(&dir), 2);
        assert!(!new.exists() && !dir.with_extension("old").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_interrupted_upgrade_restores_the_backup() {
        let dir = std::env::temp_dir().join(format!("omsi-avatars-interrupted-{}", std::process::id()));
        let (new, old) = (dir.with_extension("unpacking"), dir.with_extension("old"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&new);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("pack.json"), r#"{"version":1,"avatars":[]}"#).unwrap();
        assert!(super::promote(&new, &dir).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("pack.json")).unwrap(), r#"{"version":1,"avatars":[]}"#);
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_backup_left_alone_is_used_offline() {
        let dir = std::env::temp_dir().join(format!("omsi-avatars-offline-{}", std::process::id()));
        let old = dir.with_extension("old");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("a.fbx"), b"x").unwrap();
        std::fs::write(old.join("pack.json"), r#"{"version":1,"avatars":[{"id":"a","file":"a.fbx"}]}"#).unwrap();
        super::restore_backup(&dir).unwrap();
        assert_eq!(super::best(vec![dir.clone()]), Some(dir.clone()));
        assert_eq!(super::load(&dir).len(), 1);
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pack_lists_its_avatars() {
        let dir = std::env::temp_dir().join(format!("omsi-avatars-pack-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("A")).unwrap();
        std::fs::write(dir.join("A/A.fbx"), b"x").unwrap();
        std::fs::write(dir.join("pack.json"), r#"{"version":1,"avatars":[{"id":"A","file":"A/A.fbx","female":true},{"id":"B","file":"B/B.fbx"}]}"#).unwrap();
        let a = super::load(&dir);
        assert_eq!(a.len(), 1, "a listed avatar without its file is left out");
        assert!(a[0].female && !a[0].child);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
