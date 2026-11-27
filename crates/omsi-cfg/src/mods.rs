use std::path::{Path, PathBuf};

use crate::add_content_root_before;

/// The `Mods` subfolders of the content folder that the launcher keeps for its own
/// bookkeeping (an installed pack's notes, the inbox, packs held back, the switched-off
/// ones): they are not mods to mount.
const MODS_OWN: [&str; 6] = [
    "installed",
    "waiting",
    "plugins-not-enabled",
    "uninstalled",
    "disabled",
    ".installed-records",
];

/// The mod folders of `<content>/Mods`, in name order, each the root a mod is used from:
/// a pack dropped there as it came (`Mods/MyPack/Vehicles/...`) is not merged into the
/// content folder's tree. A pack that was unzipped with its own folder inside
/// (`Mods/MyPack/MyPack/Vehicles/...`) is used from that inner folder, unless the inner
/// folder is one the game itself has at the top (`Mods/MyPack/Vehicles/...`).
pub fn mod_dirs(content: &Path) -> Vec<PathBuf> {
    let mods = content.join("Mods");
    let Ok(rd) = std::fs::read_dir(&mods) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !p.is_dir()
            || name.starts_with('.')
            || MODS_OWN.iter().any(|o| name.eq_ignore_ascii_case(o))
        {
            continue;
        }
        // an unpacked pack that kept its own folder: use the folder inside
        let inner: Vec<PathBuf> = std::fs::read_dir(&p)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        match (inner.len(), inner.first()) {
            (1, Some(one)) if one.is_dir() => {
                let iname = one
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if crate::CONTENT_FOLDERS
                    .iter()
                    .any(|t| iname.eq_ignore_ascii_case(t))
                {
                    out.push(p);
                } else {
                    out.push(one.clone());
                }
            }
            _ => out.push(p),
        }
    }
    out.sort();
    out
}

/// Mount every mod folder of `<content>/Mods` as a content root of its own (#2135): a pack
/// lies where it was put, and is searched before the content folder, so its `Vehicles/...`
/// or `maps/...` is found as if it were installed - without copying it into the content
/// folder's tree, and deleting the folder takes the mod out again. Returns the roots added.
pub fn mount_mod_dirs(content: &Path) -> Vec<PathBuf> {
    let dirs = mod_dirs(content);
    for d in &dirs {
        add_content_root_before(d.clone(), content);
    }
    dirs
}
