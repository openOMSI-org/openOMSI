use crate::{content_dir, data_dir, root};
use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The situation the game left on `map` last (`laststn.osn` in the map's folder: the
/// content folder's copy first, then OMSI 2's own), if there is one.
pub fn last_situation(map: &str) -> Option<PathBuf> {
    let dir = Path::new(&map.replace('\\', "/")).parent()?.to_path_buf();
    content_dir()
        .map(|c| c.join(&dir).join("laststn.osn"))
        .into_iter()
        .chain(root().ok().map(|r| r.join(&dir).join("laststn.osn")))
        .find(|p| p.is_file())
}

/// A situation saved on a map to continue from: the file, its `[name]`, when it was written
/// (seconds since 1970).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SavedSituation {
    pub file: PathBuf,
    pub name: String,
    pub saved: u64,
    pub slot: bool,
}

/// What can be continued on `map` (#341): the last situation, then the save slots the game
/// writes into `Saves` of the map's folder in the content folder and the quicksave made on
/// that map, the newest first.
pub fn saved_situations(map: &str) -> Vec<SavedSituation> {
    let mut out: Vec<SavedSituation> = last_situation(map).map(|f| SavedSituation { saved: modified_secs(&f), file: f, name: "Last situation".into(), slot: false }).into_iter().collect();
    if let Some(dir) = Path::new(&map.replace('\\', "/")).parent() {
        let c = content_dir();
        // (and where the game saves when the content folder takes no file, #1673)
        let fallback = data_dir().join("content");
        let bases: Vec<PathBuf> = c.iter().cloned().chain((c.as_deref() != Some(fallback.as_path())).then_some(fallback)).collect();
        for base in &bases {
            out.extend(save_slots(&base.join(dir).join("Saves")));
        }
        // "Save the situation" (Ctrl+S) writes the game's one quicksave, for every map: it
        // is offered on the map it was made on. Left out, it was saved where nothing
        // offered it again and only a slot seemed to save (#1746).
        for base in &bases {
            let f = base.join("Situations").join("quicksave.osn");
            if f.is_file() && situation_map(&f).is_some_and(|m| same_map(&m, map)) {
                out.push(SavedSituation { saved: modified_secs(&f), file: f, name: "Quicksave".into(), slot: false });
            }
        }
        let last = usize::from(out.first().is_some_and(|f| f.name == "Last situation"));
        out[last..].sort_by(|a, b| b.saved.cmp(&a.saved));
    }
    out
}

/// Delete a saved situation of `map` (#2556): only one `saved_situations` lists for it - a
/// slot, the quicksave made on it, its last situation - with the script textures saved
/// beside it. Whether it was there to delete.
pub fn delete_situation(map: &str, file: &Path) -> Result<bool> {
    if listed(map, file).is_none() {
        return Ok(false);
    }
    std::fs::remove_file(file)?;
    let mut tex = file.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    tex.push(".scripttex");
    let _ = std::fs::remove_dir_all(file.with_file_name(tex));
    Ok(true)
}

pub fn rename_situation(map: &str, file: &Path, name: &str) -> Result<bool> {
    let name = name.trim();
    if name.is_empty() || name.chars().any(char::is_control) || !listed(map, file).is_some_and(|s| s.slot) {
        return Ok(false);
    }
    write_name(file, name)?;
    Ok(true)
}

fn listed(map: &str, file: &Path) -> Option<SavedSituation> {
    if !Path::new(&map.replace('\\', "/")).components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)) {
        return None;
    }
    saved_situations(map).into_iter().find(|s| s.file == file)
}

fn write_name(file: &Path, name: &str) -> Result<()> {
    let bytes = std::fs::read(file)?;
    let saved = std::fs::metadata(file)?.modified()?;
    let text = omsi_cfg::decode_text(&bytes);
    let mut lines = omsi_cfg::split_lines(&text);
    match lines.iter().position(|l| l.trim().eq_ignore_ascii_case("[name]")) {
        Some(i) if i + 1 < lines.len() => lines[i + 1] = name.to_string(),
        Some(_) => lines.push(name.to_string()),
        None => lines.splice(0..0, ["[name]".to_string(), name.to_string(), String::new()]).for_each(drop),
    }
    let ended = text.ends_with(['\r', '\n']);
    let mut text = lines.join("\r\n");
    if ended {
        text.push_str("\r\n");
    }
    // (Windows-1252 cannot hold every name: such a file comes back in UTF-16, as the game writes it)
    let out: Vec<u8> = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        [0xEF, 0xBB, 0xBF].into_iter().chain(text.bytes()).collect()
    } else {
        [0xFF, 0xFE].into_iter().chain(text.encode_utf16().flat_map(u16::to_le_bytes)).collect()
    };
    let tmp = file.with_extension("osn.renaming");
    // (the lists sort and date the saves by this time: a rename is not a newer save)
    let written = std::fs::write(&tmp, out).and_then(|_| std::fs::File::options().write(true).open(&tmp)?.set_modified(saved)).and_then(|_| std::fs::rename(&tmp, file));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(written?)
}

/// The `[map]` a situation file was saved on (its head alone is read).
fn situation_map(f: &Path) -> Option<String> {
    let text = std::fs::read(f).map(|b| omsi_cfg::decode_text(&b[..b.len().min(8192) & !1])).ok()?;
    let mut lines = text.lines().map(str::trim);
    lines.by_ref().find(|l| l.eq_ignore_ascii_case("[map]"))?;
    lines.next().map(str::to_string).filter(|m| !m.is_empty())
}

/// Whether two map paths name the same `global.cfg`: from their `maps/` on (a map from the
/// content folder is saved with its whole path), slashes and case aside.
fn same_map(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let s = s.trim().replace('\\', "/").to_ascii_lowercase();
        match s.rfind("maps/") {
            Some(i) => s[i..].to_string(),
            None => s,
        }
    };
    norm(a) == norm(b)
}

fn modified_secs(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

/// The situations in a map's `Saves` folder, the newest first, by their `[name]`.
fn save_slots(dir: &Path) -> Vec<SavedSituation> {
    let mut slots: Vec<SavedSituation> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("osn")))
        .map(|f| {
            // (the `[name]` line alone: the lists ask again every few seconds, and a whole
            // situation holds every variable of every vehicle; the game writes them in
            // UTF-16, as OMSI does)
            let text = std::fs::read(&f).map(|b| omsi_cfg::decode_text(&b[..b.len().min(8192) & !1])).unwrap_or_default();
            let mut lines = text.lines().map(str::trim);
            let name = lines.by_ref().find(|l| l.eq_ignore_ascii_case("[name]")).and_then(|_| lines.next()).map(str::to_string).filter(|n| !n.is_empty()).unwrap_or_else(|| f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
            SavedSituation { saved: modified_secs(&f), file: f, name, slot: true }
        })
        .collect();
    slots.sort_by(|a, b| b.saved.cmp(&a.saved).then_with(|| b.name.cmp(&a.name)));
    slots
}

#[cfg(test)]
mod save_slot_tests {
    use super::*;

    #[test]
    fn a_quicksave_belongs_to_the_map_it_names() {
        assert!(same_map("maps\\Grundorf\\global.cfg", "maps/Grundorf/global.cfg"));
        assert!(same_map("C:\\Users\\x\\openOMSI\\content\\maps\\Grundorf\\global.cfg", "maps/grundorf/global.cfg"));
        assert!(!same_map("maps\\Berlin-Spandau\\global.cfg", "maps/Grundorf/global.cfg"));
        let dir = std::env::temp_dir().join(format!("omsi_quicksave_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("quicksave.osn");
        let utf16 = |s: &str| -> Vec<u8> { [0xFF, 0xFE].into_iter().chain(s.encode_utf16().flat_map(|u| u.to_le_bytes())).collect() };
        std::fs::write(&f, utf16("[name]\r\nQuicksave\r\n\r\n[map]\r\nmaps\\Grundorf\\global.cfg\r\n")).unwrap();
        assert_eq!(situation_map(&f).as_deref(), Some("maps\\Grundorf\\global.cfg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_slots_of_a_map_are_listed_by_their_names() {
        let dir = std::env::temp_dir().join(format!("omsi_slots_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // as the game writes them: UTF-16 with its mark
        let utf16 = |t: &str| [0xFFu8, 0xFE].into_iter().chain(t.encode_utf16().flat_map(|u| u.to_le_bytes())).collect::<Vec<u8>>();
        std::fs::write(dir.join("Slot 1.osn"), utf16("\r\n[name]\r\nSlot 1: SD202, 09:00\r\n[description]\r\nx\r\n")).unwrap();
        std::fs::write(dir.join("Slot 2.osn"), utf16("[name]\r\nSlot 2: NG272, 10:30\r\n")).unwrap();
        std::fs::write(dir.join("notes.txt"), "not a situation").unwrap();
        let mut names: Vec<String> = save_slots(&dir).into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(names, ["Slot 1: SD202, 09:00", "Slot 2: NG272, 10:30"]);
        assert!(save_slots(&dir.join("none")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_renamed_slot_keeps_the_rest_of_its_situation() {
        let dir = std::env::temp_dir().join(format!("omsi_rename_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let utf16 = |t: &str| [0xFFu8, 0xFE].into_iter().chain(t.encode_utf16().flat_map(|u| u.to_le_bytes())).collect::<Vec<u8>>();
        let body = "\r\n[map]\r\nmaps\\Grundorf\\global.cfg\r\n[name]\r\nx\r\n";
        std::fs::write(dir.join("Slot 1.osn"), utf16(&format!("[name]\r\nSlot 1: SD202, 09:00{body}"))).unwrap();
        std::fs::write(dir.join("Slot 2.osn"), b"[time]\r\n8\r\n").unwrap();
        std::fs::write(dir.join("Slot 3.osn"), b"[name]\rold\r[time]\r8\r").unwrap();
        write_name(&dir.join("Slot 3.osn"), "Morning").unwrap();
        assert_eq!(omsi_cfg::decode_text(&std::fs::read(dir.join("Slot 3.osn")).unwrap()), "[name]\r\nMorning\r\n[time]\r\n8\r\n");
        std::fs::remove_file(dir.join("Slot 3.osn")).unwrap();
        let before = std::fs::metadata(dir.join("Slot 1.osn")).unwrap().modified().unwrap();
        write_name(&dir.join("Slot 1.osn"), "Before the depot").unwrap();
        assert_eq!(std::fs::metadata(dir.join("Slot 1.osn")).unwrap().modified().unwrap(), before);
        write_name(&dir.join("Slot 2.osn"), "Перед депо").unwrap();
        assert_eq!(std::fs::read(dir.join("Slot 1.osn")).unwrap(), utf16(&format!("[name]\r\nBefore the depot{body}")));
        assert_eq!(omsi_cfg::decode_text(&std::fs::read(dir.join("Slot 2.osn")).unwrap()), "[name]\r\nПеред депо\r\n\r\n[time]\r\n8\r\n");
        let mut names: Vec<String> = save_slots(&dir).into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(names, ["Before the depot", "Перед депо"]);
        assert!(!rename_situation("maps/Grundorf/global.cfg", &dir.join("Slot 1.osn"), "elsewhere").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
