//! The ambience's recordings: a folder with `pack.json` and the takes it lists (built by
//! `tools/ambience/build_pack.py` from free recordings, credited in its `CREDITS.md`).

use std::path::{Path, PathBuf};

/// One cut of a recording.
#[derive(Debug, Clone)]
pub struct Take {
    pub path: PathBuf,
    /// Its loudness (LUFS: integrated for a bed, the loudest 3 s of an event).
    pub lufs: f32,
    /// The country it belongs to (`de`, `uk`), if any: a German platform's announcements are
    /// not heard on a British map.
    pub region: Option<String>,
}

#[derive(Debug, Default)]
pub struct Pack {
    /// Every take, and those of the map's country (and of none) - what is played.
    all: hashbrown::HashMap<String, Vec<Take>>,
    slots: hashbrown::HashMap<String, Vec<Take>>,
}

impl Pack {
    /// The pack in `dir`, if there is one.
    pub fn load(dir: &Path) -> Option<Pack> {
        let text = std::fs::read_to_string(dir.join("pack.json")).ok()?;
        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(j) => j,
            Err(e) => {
                log::warn!("ambience: {}: {e}", dir.join("pack.json").display());
                return None;
            }
        };
        let mut slots: hashbrown::HashMap<String, Vec<Take>> = hashbrown::HashMap::new();
        for t in json.get("takes").and_then(|t| t.as_array()).into_iter().flatten() {
            let (Some(slot), Some(file)) = (t.get("slot").and_then(|v| v.as_str()), t.get("file").and_then(|v| v.as_str())) else { continue };
            let num = |k: &str| t.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
            // (a one-shot is as loud as its loudest moment, a layer as its whole)
            let lufs = num("st_max_lufs").or(num("lufs")).unwrap_or(-27.0);
            let region = t.get("region").and_then(|v| v.as_str()).map(|s| s.to_ascii_lowercase());
            slots.entry(slot.to_string()).or_default().push(Take { path: dir.join(file), lufs, region });
        }
        let takes: usize = slots.values().map(|v| v.len()).sum();
        log::info!("ambience: {takes} recordings in {} slots from {}", slots.len(), dir.display());
        Some(Pack { all: slots.clone(), slots })
    }

    /// Where the pack is looked for: `OMSI_AMBIENCE_DIR`, then `ambience` beside the program
    /// (and in the macOS bundle's resources), then in the user's openOMSI folder.
    pub fn find() -> Option<Pack> {
        Pack::dirs().iter().find_map(|d| Pack::load(d))
    }

    fn dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(d) = omsi_cfg::flags::OMSI_AMBIENCE_DIR.var() {
            dirs.push(PathBuf::from(d));
        }
        if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            dirs.push(exe.join("ambience"));
            dirs.push(exe.join("../Resources/ambience"));
        }
        dirs.extend(Pack::user_dir());
        dirs
    }

    /// Where a downloaded pack goes: the user's openOMSI folder.
    pub fn user_dir() -> Option<PathBuf> {
        omsi_cfg::user_data::base_dir().map(|d| d.join("ambience"))
    }

    /// Fetch the recordings in the background when there are none yet (a computer, the
    /// ambience on): the release asset `ambience-1` of the project, checked and unpacked into
    /// the user's openOMSI folder. They play from the next drive on.
    pub fn fetch_if_missing() {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if cfg!(target_os = "android") || STARTED.swap(true, std::sync::atomic::Ordering::Relaxed) || Pack::find_dir().is_some() {
            return;
        }
        let Some(dir) = Pack::user_dir() else { return };
        let part = dir.with_extension("zip.part");
        // (another openOMSI process - the launcher, the game - is fetching it now)
        if std::fs::metadata(&part).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() < 120) {
            return;
        }
        std::thread::Builder::new()
            .name("ambience fetch".into())
            .spawn(move || match fetch(&dir, &part) {
                Ok(()) => log::info!("ambience: the recordings were downloaded into {}", dir.display()),
                Err(e) => {
                    let _ = std::fs::remove_file(&part);
                    log::warn!("ambience: the recordings could not be downloaded: {e:#}");
                }
            })
            .ok();
    }

    /// The folder of the pack that `find` would load.
    fn find_dir() -> Option<PathBuf> {
        Pack::dirs().into_iter().find(|d| d.join("pack.json").is_file())
    }

    /// Play the takes of this country (`uk` or `de`) and those of none.
    pub fn set_region(&mut self, region: &str) {
        self.slots = self
            .all
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().filter(|t| t.region.as_deref().is_none_or(|r| r == region)).cloned().collect::<Vec<_>>()))
            .filter(|(_, v)| !v.is_empty())
            .collect();
    }

    pub fn takes(&self, slot: &str) -> &[Take] {
        self.slots.get(slot).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn has(&self, slot: &str) -> bool {
        !self.takes(slot).is_empty()
    }
}

/// The recordings' archive (a pre-release of the project's own, see `tools/ambience`), its
/// SHA-256 and size.
const PACK_URL: &str = "https://github.com/openOMSI-org/openOMSI/releases/download/ambience-1/openOMSI-ambience-1.zip";
const PACK_SHA256: &str = "f0f90ed4108dcd28c492cc2d0ff4fd01c776cb12886556187280eb3d714cacc3";
const PACK_BYTES: u64 = 297_379_915;

/// Download the archive to `part`, check it, unpack it beside `dir` and put it in its place.
fn fetch(dir: &Path, part: &Path) -> anyhow::Result<()> {
    use sha2::Digest;
    use std::io::{Read, Write};
    if let Some(d) = part.parent() {
        std::fs::create_dir_all(d)?;
    }
    log::info!("ambience: downloading the recordings ({} MB) from {PACK_URL}", PACK_BYTES >> 20);
    let resp = omsi_platform::updater::agent().get(PACK_URL).call().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(part)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut last_note = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        done += n as u64;
        // (the file's time says another process that it is being fetched, see `fetch_if_missing`)
        if done - last_note > 32 << 20 {
            last_note = done;
            out.flush()?;
            log::info!("ambience: {} of {} MB", done >> 20, PACK_BYTES >> 20);
        }
    }
    out.flush()?;
    drop(out);
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    anyhow::ensure!(got == PACK_SHA256, "the download is damaged ({done} bytes, SHA-256 {got})");
    let tmp = dir.with_extension("unpacking");
    let _ = std::fs::remove_dir_all(&tmp);
    omsi_platform::updater::unpack(part, &tmp)?;
    anyhow::ensure!(tmp.join("pack.json").is_file(), "the archive holds no pack.json");
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&tmp, dir)?;
    let _ = std::fs::remove_file(part);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_pack_lists_its_takes_by_slot() {
        let dir = std::env::temp_dir().join(format!("omsi-ambience-pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pack.json"),
            r#"{"version":1,"takes":[{"slot":"rain.light","file":"rain/light/1_1.mp3","seconds":60.0,"lufs":-27.0},{"slot":"rain.light","file":"rain/light/2_1.mp3","seconds":45.5,"lufs":-26.0},{"slot":"bird.robin","file":"bird/robin/3_1.mp3","seconds":4.0,"lufs":-22.0,"st_max_lufs":-18.0},{"slot":"station.announce","file":"a.mp3","region":"de"},{"slot":"station.announce","file":"b.mp3","region":"uk"},{"slot":"station.announce","file":"c.mp3"}]}"#,
        )
        .unwrap();
        let p = super::Pack::load(&dir).unwrap();
        assert_eq!(p.takes("rain.light").len(), 2);
        assert_eq!(p.takes("rain.light")[1].lufs, -26.0);
        assert!(p.has("bird.robin") && !p.has("bird.gull"));
        assert!(p.takes("bird.robin")[0].path.ends_with("bird/robin/3_1.mp3"));
        assert_eq!(p.takes("bird.robin")[0].lufs, -18.0, "an event by its loudest moment");
        let mut p = p;
        p.set_region("uk");
        let names: Vec<_> = p.takes("station.announce").iter().map(|t| t.path.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["b.mp3", "c.mp3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every take of the pack at `OMSI_AMBIENCE_DIR` decodes with the game's own reader (run
    /// with `--ignored` where a pack is).
    #[test]
    #[ignore]
    fn every_take_of_the_pack_decodes() {
        let Some(dir) = omsi_cfg::flags::OMSI_AMBIENCE_DIR.var() else { return };
        let p = super::Pack::load(std::path::Path::new(dir)).expect("pack.json");
        let mut bad = Vec::new();
        let mut n = 0;
        for takes in p.all.values() {
            for t in takes {
                n += 1;
                match omsi_audio::mixer::read_clip(&t.path) {
                    Some(c) if c.frames() > 0 => {}
                    _ => bad.push(t.path.display().to_string()),
                }
            }
        }
        assert!(bad.is_empty(), "{} of {n} takes do not decode: {:?}", bad.len(), &bad[..bad.len().min(20)]);
    }
}
