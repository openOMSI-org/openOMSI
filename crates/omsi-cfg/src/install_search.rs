//! Finding an installation of the original OMSI 2 without being told where it is.
//!
//! Tried in order: the places the caller names (a configured folder, $OMSI_ROOT, the folder
//! remembered from the last run), folders beside the program and the working directory,
//! every Steam library (Steam's `libraryfolders.vdf` on Windows, macOS and Linux), the usual
//! Windows install folders on every drive, Wine/CrossOver/Whisky bottles, and finally a shallow
//! look through the user's own folders (Desktop, Documents, Downloads, Games ...). Only a
//! complete installation counts ([`crate::missing_original_essentials`] is empty).

use std::path::{Path, PathBuf};

pub const NAMES: &[&str] = &[
    "OMSI 2",
    "OMSI 2 Original",
    "Omsi 2",
    "OMSI2",
    "omsi2",
    "OMSI 2 Steam Edition",
    "OMSI",
];

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn is_install(p: &Path) -> bool {
    p.is_dir() && crate::missing_original_essentials(p).is_empty()
}

/// Steam library folders named in a `libraryfolders.vdf` (`"path"  "D:\\SteamLibrary"`).
fn steam_libraries(vdf: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(vdf) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('"').filter(|s| !s.trim().is_empty()).collect();
        if parts.len() >= 2 && parts[0].eq_ignore_ascii_case("path") {
            out.push(PathBuf::from(parts[1].replace("\\\\", "\\")));
        }
    }
    out
}

/// Every Steam installation's own folder this machine may have.
fn steam_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = home() {
        v.push(h.join("Library/Application Support/Steam"));
        v.push(h.join(".steam/steam"));
        v.push(h.join(".local/share/Steam"));
        v.push(h.join(".var/app/com.valvesoftware.Steam/data/Steam"));
    }
    for drive in windows_drives() {
        v.push(drive.join("Program Files (x86)/Steam"));
        v.push(drive.join("Program Files/Steam"));
        v.push(drive.join("Steam"));
        v.push(drive.join("SteamLibrary"));
        v.push(drive.join("Games/Steam"));
    }
    v
}

fn windows_drives() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    (b'C'..=b'Z')
        .map(|c| PathBuf::from(format!("{}:/", c as char)))
        .filter(|p| p.exists())
        .collect()
}

/// Wine prefixes (plain Wine, CrossOver, Whisky, Heroic ...) whose `drive_c` may hold OMSI.
fn wine_drives() -> Vec<PathBuf> {
    let Some(h) = home() else { return Vec::new() };
    let mut v = vec![h.join(".wine/drive_c")];
    for bottles in [
        h.join("Library/Application Support/CrossOver/Bottles"),
        h.join("Library/Containers/com.isaacmarovitz.Whisky/Bottles"),
        h.join("Games/Heroic/Prefixes"),
        h.join(".local/share/bottles/bottles"),
    ] {
        if let Ok(rd) = std::fs::read_dir(&bottles) {
            for e in rd.flatten() {
                v.push(e.path().join("drive_c"));
            }
        }
    }
    v
}

/// Candidate folders, most likely first (without the caller's own).
pub fn candidates() -> Vec<PathBuf> {
    let mut tried: Vec<PathBuf> = Vec::new();
    let mut bases: Vec<PathBuf> = Vec::new();
    // a phone: the shared storage, where a copy of the game is put by cable or file manager
    // (openOMSI's own folder first, where the app asks for it)
    if cfg!(target_os = "android") {
        for s in ["/storage/emulated/0", "/sdcard"] {
            let s = PathBuf::from(s);
            bases.push(s.join("openOMSI"));
            bases.push(s.join("Download"));
            bases.push(s.join("Games"));
            bases.push(s);
        }
    }
    // an iPhone or an iPad: the app's own Documents (the content folder, which the Files app
    // shows as "openOMSI"), where a copy of the game is put by the Files app or the Finder
    if cfg!(target_os = "ios") {
        if let Some(c) = crate::flags::OMSI_CONTENT.live_os() {
            bases.push(PathBuf::from(c));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        bases.extend(exe.ancestors().skip(1).take(8).map(|p| p.to_path_buf()));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().take(5).map(|p| p.to_path_buf()));
    }
    for b in &bases {
        tried.push(b.clone());
        for n in NAMES {
            tried.push(b.join(n));
        }
    }
    let mut steams = steam_roots();
    for w in wine_drives() {
        steams.push(w.join("Program Files (x86)/Steam"));
        steams.push(w.join("Program Files/Steam"));
    }
    for s in &steams {
        let mut libs = vec![s.clone()];
        libs.extend(steam_libraries(&s.join("steamapps/libraryfolders.vdf")));
        libs.extend(steam_libraries(&s.join("config/libraryfolders.vdf")));
        for l in libs {
            tried.push(l.join("steamapps/common/OMSI 2"));
        }
    }
    for d in windows_drives().into_iter().chain(wine_drives()) {
        for sub in [
            "Program Files (x86)",
            "Program Files",
            "Games",
            "Spiele",
            "",
        ] {
            for n in NAMES {
                tried.push(d.join(sub).join(n));
            }
            tried.push(d.join(sub).join("aerosoft").join("OMSI 2"));
        }
    }
    if let Some(h) = home() {
        for sub in [
            "",
            "Desktop",
            "Documents",
            "Downloads",
            "Games",
            "Spiele",
            "Applications",
            "Library/Application Support",
        ] {
            for n in NAMES {
                tried.push(h.join(sub).join(n));
            }
        }
    }
    tried.push(PathBuf::from("/Applications/OMSI 2"));
    tried
}

/// A shallow search (two levels below the user's usual folders) for any folder that is an
/// installation, whatever it is called.
fn shallow_scan() -> Option<PathBuf> {
    let h = home()?;
    let mut dirs = vec![h.clone()];
    for sub in [
        "Desktop",
        "Documents",
        "Downloads",
        "Games",
        "Spiele",
        "Applications",
    ] {
        dirs.push(h.join(sub));
    }
    dirs.extend(windows_drives());
    let mut seen = 0usize;
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir()
                || p.file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(true)
            {
                continue;
            }
            seen += 1;
            if seen > 4000 {
                return None;
            }
            if p.join("Omsi.exe").is_file() && is_install(&p) {
                return Some(p);
            }
            if let Ok(rd2) = std::fs::read_dir(&p) {
                for e2 in rd2.flatten().take(200) {
                    let q = e2.path();
                    if q.join("Omsi.exe").is_file() && is_install(&q) {
                        return Some(q);
                    }
                }
            }
        }
    }
    None
}

/// The folders a path the player gave may mean: what was typed or pasted (Windows' "Copy as
/// path" puts quotes round it), the folder of a file named instead of its folder (`Omsi.exe`,
/// or openOMSI's own program when openOMSI was unpacked into the OMSI folder), the folders
/// above a subfolder chosen by mistake (`maps`, `Vehicles\MAN_SD200`), and an OMSI folder
/// inside the one chosen.
pub fn root_guesses(given: &Path) -> Vec<PathBuf> {
    let s = given.to_string_lossy();
    let s = s.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    if s.is_empty() {
        return Vec::new();
    }
    let p = PathBuf::from(s);
    let dir = if p.is_file() {
        p.parent().map(Path::to_path_buf).unwrap_or(p.clone())
    } else {
        p.clone()
    };
    let mut v = vec![dir.clone()];
    v.extend(dir.ancestors().skip(1).take(3).map(Path::to_path_buf));
    for n in NAMES {
        v.push(dir.join(n));
    }
    v
}

/// The first complete installation among `first` (the caller's own guesses, in order) and
/// then every usual place.
pub fn find_original_install(first: &[PathBuf]) -> Option<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    // (the usual places only when the given ones hold none: making that list looks at every
    // drive letter, and a network drive that is not connected keeps each look waiting)
    first
        .iter()
        .flat_map(|p| root_guesses(p))
        .chain(std::iter::once_with(candidates).flatten())
        .filter(|p| !p.as_os_str().is_empty() && seen.insert(p.clone()))
        .find(|p| is_install(p))
        .or_else(shallow_scan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_or_a_quoted_path_means_its_folder() {
        let dir = std::env::temp_dir().join("openomsi-root-guess");
        let _ = std::fs::create_dir_all(dir.join("maps"));
        let exe = dir.join("openomsi.exe");
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(root_guesses(&exe)[0], dir);
        let quoted = PathBuf::from(format!("\"{}\" ", dir.display()));
        assert_eq!(root_guesses(&quoted)[0], dir);
        // a subfolder: the folder above is tried next
        assert_eq!(root_guesses(&dir.join("maps"))[1], dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn openomsi_unpacked_into_the_omsi_folder_keeps_its_content_apart() {
        let dir = std::env::temp_dir().join("openomsi-in-omsi");
        let _ = std::fs::create_dir_all(dir.join("maps"));
        std::fs::write(dir.join("Omsi.exe"), b"").unwrap();
        // a marker an older openOMSI left there does not make it "not the game"
        std::fs::write(dir.join(crate::CONTENT_MARKER), b"").unwrap();
        assert_eq!(crate::content_folder_of(&dir), dir.join("openOMSI"));
        assert!(
            !crate::missing_original_essentials(&dir)
                .iter()
                .any(|m| m.contains("content folder"))
        );
        let other = std::env::temp_dir().join("openomsi-plain");
        let _ = std::fs::create_dir_all(&other);
        assert_eq!(crate::content_folder_of(&other), other);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
