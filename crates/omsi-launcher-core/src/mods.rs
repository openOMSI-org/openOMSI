//! The mods in the content folder, one by one: what each install put there (its own folders,
//! or the archive it is read from), switched off and on, and removed.
//!
//! Every install the launcher makes is noted in `Mods/.mods.json` with the folders it made
//! (a bus's own folder, a map's) or the archive used in place. What lies in the content folder
//! without such a note - put there by hand, or installed before the notes - is listed as well,
//! one entry per bus, map, other folder and archive, so that anything can be switched off or
//! removed by itself. Switched off, a mod's folders move to `Mods/disabled/<id>` (the game
//! and the lists no longer see them) and back again when it is switched on; removed, they are
//! deleted. The original OMSI 2 folder is never touched: only the content folder holds mods.

use crate::i18n::{tr, trf};
use crate::install::{self, ARCHIVES};
use crate::map_uses::{self, Plan};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The notes of the installs, in `<content>/Mods`.
pub const REGISTRY: &str = ".mods.json";
/// Where a switched-off mod's folders wait, in `<content>/Mods`.
pub const DISABLED: &str = "disabled";

/// What a mod is, by what it holds first.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bus,
    Map,
    Archive,
    #[default]
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mod {
    /// Its name in `Mods/disabled` and in the notes (a file name).
    pub id: String,
    /// What it is called: the folder or archive it was installed from.
    pub name: String,
    /// What it owns, content-relative with '/': `Vehicles/MAN_SD202`, `Archives/x.zip`.
    pub paths: Vec<String>,
    pub enabled: bool,
    /// When it was installed (seconds since 1970; 0: not known).
    #[serde(default)]
    pub installed: u64,
    /// Bytes on the disk (worked out when listed).
    #[serde(skip_deserializing, default)]
    pub bytes: u64,
    #[serde(skip_deserializing, default)]
    pub kind: Kind,
    /// Noted by an install (else found in the content folder without a note).
    #[serde(skip_deserializing, default)]
    pub noted: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Registry {
    mods: Vec<Mod>,
}

fn registry_file(content: &Path) -> PathBuf {
    content.join("Mods").join(REGISTRY)
}

fn load(content: &Path) -> Registry {
    std::fs::read(registry_file(content)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(content: &Path, r: &Registry) -> anyhow::Result<()> {
    let f = registry_file(content);
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = f.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(r)?)?;
    std::fs::rename(&tmp, &f)?;
    Ok(())
}

/// A file name of `name`, not yet taken by another mod.
fn new_id(r: &Registry, name: &str) -> String {
    let base: String = name.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect::<String>().trim().trim_matches('.').to_string();
    let base = if base.is_empty() { "mod".to_string() } else { base };
    let mut id = base.clone();
    let mut k = 2;
    while r.mods.iter().any(|m| m.id.eq_ignore_ascii_case(&id)) {
        id = format!("{base} ({k})");
        k += 1;
    }
    id
}

fn kind_of(paths: &[String]) -> Kind {
    let top = |p: &String| p.split('/').next().unwrap_or("").to_ascii_lowercase();
    if paths.iter().any(|p| top(p) == ARCHIVES.to_ascii_lowercase()) {
        Kind::Archive
    } else if paths.iter().any(|p| top(p) == "maps") {
        Kind::Map
    } else if paths.iter().any(|p| top(p) == "vehicles") {
        Kind::Bus
    } else {
        Kind::Other
    }
}

/// Note an install: `name` put `paths` (the folders it made, or its archive) into the content
/// folder. An earlier note of the same name gains them (installed again).
pub fn record(content: &Path, name: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let mut r = load(content);
    match r.mods.iter_mut().find(|m| m.name.eq_ignore_ascii_case(name)) {
        Some(m) => {
            for p in paths {
                if !m.paths.iter().any(|x| x.eq_ignore_ascii_case(p)) {
                    m.paths.push(p.clone());
                }
            }
            m.enabled = true;
            m.installed = install::now_secs();
        }
        None => {
            let id = new_id(&r, name);
            r.mods.push(Mod { id, name: name.to_string(), paths: paths.to_vec(), enabled: true, installed: install::now_secs(), bytes: 0, kind: Kind::Other, noted: true });
        }
    }
    if let Err(e) = save(content, &r) {
        crate::log_to_file(&format!("mods: the note of {name} was not written: {e:#}"));
    }
}

pub fn attach(content: &Path, owner: &str, paths: &[String]) {
    let top = owner.split('/').next().unwrap_or("");
    if paths.is_empty() || !(top.eq_ignore_ascii_case("maps") || top.eq_ignore_ascii_case(ARCHIVES)) {
        return;
    }
    let mut r = load(content);
    import_records(content, &mut r);
    let owned = |p: &String| owner.eq_ignore_ascii_case(p) || owner.to_ascii_lowercase().starts_with(&format!("{}/", p.to_ascii_lowercase()));
    let k = match r.mods.iter().position(|m| m.enabled && m.paths.iter().any(owned)).or_else(|| r.mods.iter().position(|m| m.paths.iter().any(owned))) {
        Some(k) => k,
        None => {
            let name = owner.rsplit('/').next().unwrap_or(owner).to_string();
            let id = new_id(&r, &name);
            r.mods.push(Mod { id, name, paths: vec![owner.to_string()], enabled: true, installed: 0, bytes: 0, kind: Kind::Other, noted: true });
            r.mods.len() - 1
        }
    };
    for p in paths {
        if !r.mods[k].paths.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            r.mods[k].paths.push(p.clone());
        }
    }
    if let Err(e) = save(content, &r) {
        crate::log_to_file(&format!("mods: the note of {owner} was not written: {e:#}"));
    }
}

fn size_of(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| size_of(&e.path())).sum()).unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

fn disabled_dir(content: &Path, id: &str) -> PathBuf {
    content.join("Mods").join(DISABLED).join(id)
}

/// Every mod of the content folder: the noted ones (switched on or off), then what lies there
/// without a note, one entry per bus, map, other folder and archive.
pub fn list(content: &Path) -> Vec<Mod> {
    let mut r = load(content);
    import_records(content, &mut r);
    // (a note whose folders are gone everywhere - deleted by hand - goes too)
    let before = r.mods.len();
    r.mods.retain(|m| m.paths.iter().any(|p| content.join(p).exists() || disabled_dir(content, &m.id).join(p).exists()));
    if r.mods.len() != before {
        let _ = save(content, &r);
    }
    let mut out = Vec::new();
    for m in &r.mods {
        let mut m = m.clone();
        let at = |p: &String| if m.enabled { content.join(p) } else { disabled_dir(content, &m.id).join(p) };
        m.bytes = m.paths.iter().map(|p| size_of(&at(p))).sum();
        m.kind = kind_of(&m.paths);
        m.noted = true;
        if let Some(folder) = m.name.strip_suffix(" (loose files)") {
            m.name = trf("{folder} (loose files)", &[("folder", &folder)]);
        }
        out.push(m);
    }
    let claimed: Vec<String> = r.mods.iter().flat_map(|m| m.paths.iter().map(|p| p.to_ascii_lowercase())).collect();
    let is_claimed = |rel: &str| {
        let rel = rel.to_ascii_lowercase();
        claimed.iter().any(|c| *c == rel || rel.starts_with(&format!("{c}/")) || c.starts_with(&format!("{rel}/")))
    };
    let mut found = Vec::new();
    for folder in omsi_cfg::CONTENT_FOLDERS {
        let Ok(rd) = std::fs::read_dir(content.join(folder)) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !e.path().is_dir() {
                continue;
            }
            let rel = format!("{folder}/{name}");
            if is_claimed(&rel) {
                continue;
            }
            found.push((name, rel));
        }
    }
    // (files lying loose in a folder all mods share - fonts, textures - one entry per folder:
    // listed by folders alone they were not in the list at all)
    for folder in omsi_cfg::CONTENT_FOLDERS {
        let files = loose_files(content, folder, &is_claimed);
        if !files.is_empty() {
            let paths: Vec<String> = files.iter().map(|f| format!("{folder}/{f}")).collect();
            let bytes = paths.iter().map(|p| size_of(&content.join(p))).sum();
            out.push(Mod { id: format!("{LOOSE}{folder}"), name: trf("{folder} (loose files)", &[("folder", &folder)]), kind: kind_of(&paths), paths, bytes, enabled: true, installed: 0, noted: false });
        }
    }
    if let Ok(rd) = std::fs::read_dir(content.join(ARCHIVES)) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let rel = format!("{ARCHIVES}/{name}");
            if !name.starts_with('.') && e.path().is_file() && !is_claimed(&rel) {
                found.push((name, rel));
            }
        }
    }
    for (name, rel) in found {
        let paths = vec![rel.clone()];
        out.push(Mod { id: format!("found:{rel}"), name, bytes: size_of(&content.join(&rel)), kind: kind_of(&paths), paths, enabled: true, installed: 0, noted: false });
    }
    out
}

/// The id of the entry that stands for a folder's loose files (`list`).
const LOOSE: &str = "found-loose:";

/// The files of `folder` (not its subfolders) no note claims.
fn loose_files(content: &Path, folder: &str, is_claimed: &dyn Fn(&str) -> bool) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(content.join(folder)) else { return Vec::new() };
    let mut out: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.') && !is_claimed(&format!("{folder}/{n}")))
        .collect();
    out.sort_by_key(|n| n.to_ascii_lowercase());
    out
}

/// The notes of the inbox installs made before `Mods/.mods.json` (`Mods/.installed-records`)
/// taken over.
fn import_records(content: &Path, r: &mut Registry) {
    let dir = content.join("Mods").join(install::RECORDS);
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut changed = false;
    for e in rd.flatten() {
        let Some(name) = e.file_name().to_string_lossy().strip_suffix(".txt").map(str::to_string) else { continue };
        if r.mods.iter().any(|m| m.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let paths: Vec<String> = std::fs::read_to_string(e.path()).unwrap_or_default().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && content.join(l).exists()).collect();
        if paths.is_empty() {
            continue;
        }
        let id = new_id(r, &name);
        r.mods.push(Mod { id, name, paths, enabled: true, installed: 0, bytes: 0, kind: Kind::Other, noted: true });
        changed = true;
    }
    if changed {
        let _ = save(content, r);
    }
}

/// The note of `id` (a mod found without one gets one now).
fn take(content: &Path, r: &mut Registry, id: &str) -> anyhow::Result<usize> {
    if let Some(k) = r.mods.iter().position(|m| m.id == id) {
        return Ok(k);
    }
    if let Some(folder) = id.strip_prefix(LOOSE) {
        // (a folder's loose files: noted together, as the list shows them)
        let claimed: Vec<String> = r.mods.iter().flat_map(|m| m.paths.iter().map(|p| p.to_ascii_lowercase())).collect();
        let is_claimed = |rel: &str| claimed.iter().any(|c| c.eq_ignore_ascii_case(rel));
        let paths: Vec<String> = loose_files(content, folder, &is_claimed).into_iter().map(|f| format!("{folder}/{f}")).collect();
        if paths.is_empty() || !omsi_cfg::CONTENT_FOLDERS.iter().any(|f| f.eq_ignore_ascii_case(folder)) {
            anyhow::bail!(trf("no loose files in {folder}", &[("folder", &folder)]));
        }
        let new = new_id(r, &format!("{folder} (loose files)"));
        r.mods.push(Mod { id: new, name: format!("{folder} (loose files)"), paths, enabled: true, installed: 0, bytes: 0, kind: Kind::Other, noted: true });
        return Ok(r.mods.len() - 1);
    }
    let rel = id.strip_prefix("found:").ok_or_else(|| anyhow::anyhow!("no mod {id}"))?;
    if install::check_installed_rel(rel).is_err() || !content.join(rel).exists() {
        anyhow::bail!(trf("{rel} is not in the content folder", &[("rel", &rel)]));
    }
    let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
    let new = new_id(r, &name);
    r.mods.push(Mod { id: new, name, paths: vec![rel.to_string()], enabled: true, installed: 0, bytes: 0, kind: Kind::Other, noted: true });
    Ok(r.mods.len() - 1)
}

/// Whether an install runs (it may be moving folders this would move too). (The tests of the
/// installer run their installs beside these, in other folders.)
fn installing() -> bool {
    !cfg!(test) && install::jobs().iter().any(|j| j.finished.is_none())
}

fn move_path(from: &Path, to: &Path) -> anyhow::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d)?;
    }
    if to.exists() {
        anyhow::bail!(trf("{path} is in the way", &[("path", &to.display())]));
    }
    std::fs::rename(from, to).map_err(|e| anyhow::anyhow!("moving {} to {}: {e}", from.display(), to.display()))
}

/// The archives among `paths` no longer read by the lists (switched off or removed).
fn unmount(content: &Path, paths: &[String]) {
    for p in paths.iter().filter(|p| p.to_ascii_lowercase().starts_with(&format!("{}/", ARCHIVES.to_ascii_lowercase()))) {
        let zip = content.join(p);
        for root in omsi_cfg::content_roots().into_iter().filter(|r| r.starts_with(&zip)) {
            omsi_cfg::remove_content_root(&root);
        }
    }
}

/// Switch the mod `id` off (its folders into `Mods/disabled/<id>`) or on (back again).
pub fn set_enabled(content: &Path, id: &str, on: bool) -> anyhow::Result<String> {
    set_enabled_in(content, id, on, &omsi_cfg::content_roots())
}

fn set_enabled_in(content: &Path, id: &str, on: bool, roots: &[PathBuf]) -> anyhow::Result<String> {
    if installing() {
        anyhow::bail!(tr("an install is under way: wait for it to end"));
    }
    let mut r = load(content);
    let k = take(content, &mut r, id)?;
    let m = r.mods[k].clone();
    if m.enabled == on {
        return Ok(m.name);
    }
    let kept = if on { Vec::new() } else { plan_in(content, &r, k, roots, false).kept };
    let off = disabled_dir(content, &m.id);
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    if !on {
        unmount(content, &m.paths);
    }
    let mut shared_elsewhere = Vec::new();
    for p in m.paths.iter().filter(|p| !kept.contains(p)) {
        let (from, to) = if on { (off.join(p), content.join(p)) } else { (content.join(p), off.join(p)) };
        if on && p.to_ascii_lowercase().starts_with("hofs/") && to.exists() {
            shared_elsewhere.push(p.clone());
            continue;
        }
        if let Err(e) = move_path(&from, &to) {
            // (what was moved goes back: a mod is either on or off, never half)
            for (a, b) in moved.into_iter().rev() {
                let _ = std::fs::rename(&b, &a);
            }
            return Err(e);
        }
        moved.push((from, to));
    }
    if on {
        for p in m.paths.iter().filter(|p| p.to_ascii_lowercase().starts_with(&format!("{}/", ARCHIVES.to_ascii_lowercase()))) {
            crate::mount_archive(&content.join(p));
        }
        let _ = std::fs::remove_dir_all(&off);
    }
    r.mods[k].enabled = on;
    r.mods[k].paths.retain(|p| !shared_elsewhere.contains(p));
    save(content, &r)?;
    omsi_cfg::content_changed();
    Ok(m.name)
}

/// Delete the mod `id`: its folders (wherever they are, on or off), its note, and what its
/// inbox install left in `Mods/installed`.
pub fn remove(content: &Path, id: &str) -> anyhow::Result<String> {
    remove_in(content, id, &omsi_cfg::content_roots())
}

pub fn remove_preview(content: &Path, id: &str) -> anyhow::Result<Plan> {
    let mut r = load(content);
    let k = take(content, &mut r, id)?;
    Ok(plan_in(content, &r, k, &omsi_cfg::content_roots(), true))
}

fn plan_in(content: &Path, r: &Registry, k: usize, roots: &[PathBuf], full: bool) -> Plan {
    let m = &r.mods[k];
    let off = disabled_dir(content, &m.id);
    // (a switched-off map is the copy under Mods/disabled: a map of the same name in the content
    // folder then is another one, installed while it was off, and counts among the others)
    let at = |p: &String| if !m.enabled && off.join(p).symlink_metadata().is_ok() { vec![off.join(p)] } else { vec![content.join(p), off.join(p)] };
    let mut lost = false;
    let mut mine: Vec<PathBuf> = Vec::new();
    for p in &m.paths {
        let lower = p.to_ascii_lowercase();
        if lower.starts_with("maps/") {
            mine.extend(at(p).into_iter().filter(|d| d.is_dir()));
        } else if lower.starts_with(&format!("{}/", ARCHIVES.to_ascii_lowercase())) {
            let zip = content.join(p);
            for root in roots.iter().filter(|x| x.starts_with(&zip)) {
                mine.extend(map_uses::child(root, "maps", &mut false).map(|d| map_uses::subdirs(&d, &mut false)).unwrap_or_default());
            }
        }
    }
    let mut parents: Vec<PathBuf> = roots.iter().chain([&content.to_path_buf()]).filter_map(|x| map_uses::child(x, "maps", &mut lost)).collect();
    for d in map_uses::subdirs(&content.join("Mods").join(DISABLED), &mut lost) {
        parents.extend(map_uses::child(&d, "maps", &mut lost));
    }
    let mut others: Vec<PathBuf> = Vec::new();
    for d in parents.iter().flat_map(|p| map_uses::subdirs(p, &mut lost)).collect::<Vec<_>>() {
        if !mine.iter().chain(&others).any(|x| map_uses::same(x, &d)) {
            others.push(d);
        }
    }
    let foreign: Vec<String> = r.mods.iter().enumerate().filter(|(i, _)| *i != k).flat_map(|(_, x)| x.paths.clone()).collect();
    let archive = |p: &String| p.to_ascii_lowercase().starts_with(&format!("{}/", ARCHIVES.to_ascii_lowercase()));
    let unread = std::fs::read_dir(content.join(ARCHIVES)).map(|rd| rd.flatten().any(|e| e.path().is_file() && e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")) && !roots.iter().any(|x| x.starts_with(e.path())))).unwrap_or_else(|_| content.join(ARCHIVES).symlink_metadata().is_ok());
    let blind = lost || unread || r.mods.iter().enumerate().any(|(i, x)| i != k && !x.enabled && x.paths.iter().any(archive));
    let maps = !mine.is_empty() || matches!(kind_of(&m.paths), Kind::Map | Kind::Archive);
    let scope = map_uses::Scope { mine, others, owned: &m.paths, foreign, blind, maps };
    let mut plan = map_uses::plan(content, roots, &scope, full);
    plan.bytes = plan.freed.iter().map(|p| size_of(&content.join(p))).sum();
    plan
}

fn remove_in(content: &Path, id: &str, roots: &[PathBuf]) -> anyhow::Result<String> {
    if installing() {
        anyhow::bail!(tr("an install is under way: wait for it to end"));
    }
    let mut r = load(content);
    let k = take(content, &mut r, id)?;
    map_uses::forget();
    let plan = plan_in(content, &r, k, roots, true);
    let m = r.mods.remove(k);
    let off = disabled_dir(content, &m.id);
    fn here(p: &Path) -> bool {
        p.symlink_metadata().is_ok()
    }
    // (the kept folders of a switched-off map go back before anything is deleted; one whose
    // place in the content folder is taken - another mod's folder of the same name, installed
    // while the map was off - stops the delete before anything moves, so neither copy is lost)
    let back: Vec<(PathBuf, PathBuf)> = plan.kept.iter().map(|p| (off.join(p), content.join(p))).filter(|(from, _)| here(from)).collect();
    if let Some((_, to)) = back.iter().find(|(_, to)| here(to)) {
        anyhow::bail!(trf("{path} is in the way", &[("path", &to.display())]));
    }
    for (from, to) in &back {
        move_path(from, to)?;
    }
    unmount(content, &m.paths);
    for p in m.paths.iter().filter(|p| !plan.kept.contains(p)) {
        install::check_installed_rel(p)?;
        // (a switched-off mod's folder is the one under Mods/disabled: a folder of the same
        // name in the content folder then is another mod's, installed while it was off)
        let places = if !m.enabled && here(&off.join(p)) { vec![off.join(p)] } else { vec![content.join(p), off.join(p)] };
        for at in places {
            if at.is_dir() {
                std::fs::remove_dir_all(&at).map_err(|e| anyhow::anyhow!(trf("deleting {path}: {e}", &[("path", &at.display()), ("e", &e)])))?;
            } else if at.exists() {
                std::fs::remove_file(&at).map_err(|e| anyhow::anyhow!(trf("deleting {path}: {e}", &[("path", &at.display()), ("e", &e)])))?;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&off);
    // (an inbox install's leftovers: gone with it, so it is not taken for a new install)
    let installed = content.join("Mods").join("installed").join(&m.name);
    let _ = std::fs::remove_dir_all(&installed);
    let _ = std::fs::remove_file(&installed);
    let _ = std::fs::remove_file(content.join("Mods").join(install::RECORDS).join(format!("{}.txt", m.name)));
    save(content, &r)?;
    for p in plan.freed.iter().filter(|p| install::check_installed_rel(p).is_ok()) {
        if let Err(e) = std::fs::remove_dir_all(content.join(p)) {
            crate::log_to_file(&format!("mods: {p}, used by {} alone, was not deleted: {e}", m.name));
        }
    }
    omsi_cfg::content_changed();
    Ok(m.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("openomsi_mods_{}_{n}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(d.join("Vehicles/Big Bus")).unwrap();
        std::fs::write(d.join("Vehicles/Big Bus/big.bus"), b"[name]").unwrap();
        std::fs::create_dir_all(d.join("maps/Hill Town")).unwrap();
        std::fs::write(d.join("maps/Hill Town/global.cfg"), b"x").unwrap();
        std::fs::create_dir_all(d.join("Vehicles/Hand Made")).unwrap();
        d
    }

    /// Fonts lying loose in Fonts (a mod's, put there by hand) are one entry, switched off
    /// and on together; a noted mod's own font is not among them.
    #[test]
    fn loose_fonts_are_listed_and_switched_together() {
        let c = content();
        std::fs::create_dir_all(c.join("Fonts")).unwrap();
        std::fs::write(c.join("Fonts/a.oft"), b"x").unwrap();
        std::fs::write(c.join("Fonts/b.bmp"), b"x").unwrap();
        std::fs::write(c.join("Fonts/mine.oft"), b"x").unwrap();
        record(&c, "Bus with font", &["Vehicles/Big Bus".into(), "Fonts/mine.oft".into()]);
        let l = list(&c);
        let loose = l.iter().find(|m| m.id == "found-loose:Fonts").expect("the loose fonts");
        assert_eq!(loose.paths, ["Fonts/a.oft", "Fonts/b.bmp"]);
        set_enabled(&c, "found-loose:Fonts", false).unwrap();
        assert!(!c.join("Fonts/a.oft").exists() && !c.join("Fonts/b.bmp").exists() && c.join("Fonts/mine.oft").exists());
        let off = list(&c).into_iter().find(|m| m.name == "Fonts (loose files)").unwrap();
        set_enabled(&c, &off.id, true).unwrap();
        assert!(c.join("Fonts/a.oft").exists());
        std::fs::remove_dir_all(c).unwrap();
    }

    #[test]
    fn an_install_is_noted_and_the_rest_is_found_one_by_one() {
        let c = content();
        record(&c, "BigBus_v2.zip", &["Vehicles/Big Bus".into()]);
        let l = list(&c);
        let noted: Vec<_> = l.iter().filter(|m| m.noted).collect();
        assert_eq!(noted.len(), 1);
        assert_eq!(noted[0].name, "BigBus_v2.zip");
        assert_eq!(noted[0].kind, Kind::Bus);
        assert!(noted[0].bytes > 0);
        let found: Vec<&str> = l.iter().filter(|m| !m.noted).map(|m| m.paths[0].as_str()).collect();
        assert!(found.contains(&"maps/Hill Town") && found.contains(&"Vehicles/Hand Made"));
        assert!(!found.contains(&"Vehicles/Big Bus"));
        std::fs::remove_dir_all(c).unwrap();
    }

    #[test]
    fn a_mod_switched_off_leaves_the_content_folder_and_comes_back() {
        let c = content();
        record(&c, "Big Bus", &["Vehicles/Big Bus".into()]);
        let id = list(&c).into_iter().find(|m| m.noted).unwrap().id;
        set_enabled(&c, &id, false).unwrap();
        assert!(!c.join("Vehicles/Big Bus").exists());
        assert!(c.join("Mods/disabled").join(&id).join("Vehicles/Big Bus/big.bus").exists());
        let m = list(&c).into_iter().find(|m| m.id == id).unwrap();
        assert!(!m.enabled && m.bytes > 0);
        set_enabled(&c, &id, true).unwrap();
        assert!(c.join("Vehicles/Big Bus/big.bus").exists());
        assert!(!c.join("Mods/disabled").join(&id).exists());
        std::fs::remove_dir_all(c).unwrap();
    }

    /// A switched-off mod waits in `Mods/disabled`: the Mods inbox, which installs what is
    /// put into `Mods`, must not take it for a new mod (it put it straight back).
    #[test]
    fn a_switched_off_mod_is_not_installed_again_from_the_inbox() {
        let c = content();
        let id = list(&c).into_iter().find(|m| m.paths == ["Vehicles/Big Bus"]).unwrap().id;
        set_enabled(&c, &id, false).unwrap();
        assert!(c.join("Mods").join(DISABLED).is_dir());
        assert!(crate::inbox_entries(&c).is_empty(), "{:?}", crate::inbox_entries(&c));
        std::fs::remove_dir_all(c).unwrap();
    }

    #[test]
    fn a_found_folder_is_switched_off_and_removed_by_itself() {
        let c = content();
        let found = list(&c).into_iter().find(|m| m.paths == ["maps/Hill Town"]).unwrap();
        set_enabled(&c, &found.id, false).unwrap();
        assert!(!c.join("maps/Hill Town").exists());
        let noted = list(&c).into_iter().find(|m| m.name == "Hill Town").unwrap();
        assert!(noted.noted && !noted.enabled);
        remove(&c, &noted.id).unwrap();
        assert!(!c.join("Mods/disabled").join(&noted.id).exists());
        assert!(list(&c).iter().all(|m| m.name != "Hill Town"));
        // the others are untouched
        assert!(c.join("Vehicles/Big Bus/big.bus").exists());
        std::fs::remove_dir_all(c).unwrap();
    }

    #[test]
    fn a_maps_shared_depot_goes_with_the_map() {
        let c = content();
        std::fs::create_dir_all(c.join("HOFs")).unwrap();
        std::fs::write(c.join("HOFs/Hill Town.hof"), b"[name]").unwrap();
        std::fs::write(c.join("HOFs/Other.hof"), b"[name]").unwrap();
        attach(&c, "maps/Hill Town", &["HOFs/Hill Town.hof".into()]);
        let map = list(&c).into_iter().find(|m| m.name == "Hill Town").unwrap();
        assert!(map.noted && map.kind == Kind::Map);
        assert_eq!(map.paths, ["maps/Hill Town", "HOFs/Hill Town.hof"]);
        remove(&c, &map.id).unwrap();
        assert!(!c.join("HOFs/Hill Town.hof").exists() && c.join("HOFs/Other.hof").exists());

        record(&c, "Town pack.zip", &["Archives/Town pack.zip".into()]);
        std::fs::create_dir_all(c.join("Archives")).unwrap();
        std::fs::write(c.join("Archives/Town pack.zip"), b"x").unwrap();
        attach(&c, "Archives/Town pack.zip/maps/Town", &["HOFs/Other.hof".into()]);
        let pack = list(&c).into_iter().find(|m| m.name == "Town pack.zip").unwrap();
        assert_eq!(pack.paths, ["Archives/Town pack.zip", "HOFs/Other.hof"]);
        assert!(list(&c).iter().all(|m| m.name != "Town"));
        std::fs::remove_dir_all(c).unwrap();
    }

    #[test]
    fn a_depot_another_map_shares_meanwhile_does_not_block_switching_on() {
        let c = content();
        std::fs::create_dir_all(c.join("maps/Valley")).unwrap();
        std::fs::create_dir_all(c.join("HOFs")).unwrap();
        std::fs::write(c.join("HOFs/Depot.hof"), b"[name]").unwrap();
        attach(&c, "maps/Hill Town", &["HOFs/Depot.hof".into()]);
        let hill = list(&c).into_iter().find(|m| m.name == "Hill Town").unwrap();
        set_enabled(&c, &hill.id, false).unwrap();
        std::fs::write(c.join("HOFs/Depot.hof"), b"[name]").unwrap();
        attach(&c, "maps/Valley", &["HOFs/Depot.hof".into()]);
        set_enabled(&c, &hill.id, true).unwrap();
        let l = list(&c);
        assert_eq!(l.iter().find(|m| m.name == "Hill Town").unwrap().paths, ["maps/Hill Town"]);
        assert_eq!(l.iter().find(|m| m.name == "Valley").unwrap().paths, ["maps/Valley", "HOFs/Depot.hof"]);
        assert!(c.join("maps/Hill Town/global.cfg").exists() && c.join("HOFs/Depot.hof").exists());
        std::fs::remove_dir_all(c).unwrap();
    }

    #[test]
    fn a_depot_of_an_unnoted_archive_map_goes_with_the_archive() {
        let c = content();
        std::fs::create_dir_all(c.join("Archives")).unwrap();
        std::fs::write(c.join("Archives/Town.zip"), b"x").unwrap();
        std::fs::create_dir_all(c.join("HOFs")).unwrap();
        std::fs::write(c.join("HOFs/Town.hof"), b"[name]").unwrap();
        attach(&c, "Archives/Town.zip", &["HOFs/Town.hof".into()]);
        attach(&c, "Vehicles/Big Bus", &["HOFs/Town.hof".into()]);
        let l = list(&c);
        let town = l.iter().find(|m| m.name == "Town.zip").unwrap();
        assert!(town.noted && town.kind == Kind::Archive);
        assert_eq!(town.paths, ["Archives/Town.zip", "HOFs/Town.hof"]);
        assert!(l.iter().all(|m| m.name != "Big Bus" || !m.noted));
        remove(&c, &town.id).unwrap();
        assert!(!c.join("Archives/Town.zip").exists() && !c.join("HOFs/Town.hof").exists());
        std::fs::remove_dir_all(c).unwrap();
    }

    fn put(at: &Path, text: &str) {
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }

    fn two_maps() -> (PathBuf, PathBuf, String) {
        let c = content();
        let omsi = c.with_extension("omsi");
        put(&omsi.join("maps/Stock/tile_0_0.map"), "[object]\n0\nSceneryobjects\\Generic\\x.sco\n");
        put(&omsi.join("Sceneryobjects/Generic/x.sco"), "");
        put(&c.join("maps/Hill Town/tile_0_0.map"), "[object]\n0\nSceneryobjects\\PackA\\a.sco\n1\n\n[object]\n0\nsceneryobjects\\SHARED\\s.sco\n\n[spline]\n0\nSplines\\RoadA\\r.sli\n\n[object]\n0\nSceneryobjects\\Generic\\x.sco\n\n[object]\n0\nSceneryobjects\\Linked\\l.sco\n");
        put(&c.join("maps/Valley/Chrono/Summer/tile_0_0.map"), "[spline]\n0\nSplines\\ChronoB\\c.sli\n");
        put(&c.join("maps/Valley/parklist_p.txt"), "Sceneryobjects\\Shared\\t.sco\n");
        for d in ["Sceneryobjects/PackA/model", "Sceneryobjects/Shared", "Sceneryobjects/Generic", "Sceneryobjects/Linked", "Splines/RoadA", "Splines/ChronoB"] {
            put(&c.join(d).join("f.txt"), "1234");
        }
        put(&c.join("Sceneryobjects/Other/o.sco"), "[mesh]\n..\\..\\Linked\\model\\m.o3d\n");
        record(&c, "Hill Town", &["maps/Hill Town".into(), "Sceneryobjects/Shared".into()]);
        let id = list(&c).into_iter().find(|m| m.name == "Hill Town").unwrap().id;
        (c, omsi, id)
    }

    #[test]
    fn a_deleted_map_takes_the_objects_and_splines_no_other_map_uses() {
        let (c, omsi, id) = two_maps();
        let roots = [c.clone(), omsi.clone()];
        let mut r = load(&c);
        let k = take(&c, &mut r, &id).unwrap();
        let plan = plan_in(&c, &r, k, &roots, true);
        assert_eq!(plan.kept, ["Sceneryobjects/Shared"]);
        assert_eq!(plan.freed, ["Sceneryobjects/PackA", "Splines/RoadA"]);
        assert_eq!(plan.bytes, 8);
        remove_in(&c, &id, &roots).unwrap();
        for gone in ["maps/Hill Town", "Sceneryobjects/PackA", "Splines/RoadA"] {
            assert!(!c.join(gone).exists(), "{gone}");
        }
        for kept in ["Sceneryobjects/Shared", "Sceneryobjects/Generic", "Sceneryobjects/Linked", "Sceneryobjects/Other", "Splines/ChronoB", "maps/Valley"] {
            assert!(c.join(kept).exists(), "{kept}");
        }
        assert!(list(&c).iter().any(|m| m.paths == ["Sceneryobjects/Shared"] && !m.noted));
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[test]
    fn without_the_omsi_folder_only_the_shared_objects_are_kept() {
        let (c, omsi, id) = two_maps();
        remove_in(&c, &id, &[c.clone()]).unwrap();
        assert!(!c.join("maps/Hill Town").exists() && c.join("Sceneryobjects/Shared").exists() && c.join("Sceneryobjects/PackA").exists());
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[test]
    fn a_map_switched_off_leaves_the_objects_another_map_uses() {
        let (c, omsi, id) = two_maps();
        let roots = [c.clone(), omsi.clone()];
        set_enabled_in(&c, &id, false, &roots).unwrap();
        assert!(!c.join("maps/Hill Town").exists() && c.join("Sceneryobjects/Shared/f.txt").exists());
        std::fs::remove_dir_all(c.join("maps/Valley")).unwrap();
        set_enabled_in(&c, &id, true, &roots).unwrap();
        assert!(c.join("maps/Hill Town/tile_0_0.map").exists() && c.join("Sceneryobjects/Shared/f.txt").exists());
        remove_in(&c, &id, &roots).unwrap();
        assert!(!c.join("Sceneryobjects/Shared").exists() && !c.join("Splines/RoadA").exists() && c.join("Splines/ChronoB").exists());
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[test]
    fn an_object_pack_a_map_uses_is_still_removed_when_asked() {
        let (c, omsi, _) = two_maps();
        let roots = [c.clone(), omsi.clone()];
        set_enabled_in(&c, "found:Sceneryobjects/Linked", false, &roots).unwrap();
        assert!(!c.join("Sceneryobjects/Linked").exists());
        let pack = list(&c).into_iter().find(|m| m.paths == ["Sceneryobjects/Linked"] && !m.enabled).unwrap();
        remove_in(&c, &pack.id, &roots).unwrap();
        assert!(!c.join("Mods/disabled").join(&pack.id).exists() && !c.join("Sceneryobjects/Linked").exists());
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[test]
    fn objects_reached_through_another_pack_or_an_unread_archive_stay() {
        let (c, omsi, id) = two_maps();
        let roots = [c.clone(), omsi.clone()];
        put(&c.join("maps/Valley/tile_1_0.map"), "[object]\r0\r.\\Sceneryobjects\\ValPack\\v.sco\r");
        put(&c.join("Sceneryobjects/ValPack/v.sco"), "[mesh]\r\n..\\..\\PackA\\model\\a.o3d\r\n");
        record(&c, "Hill Town", &["Splines/RoadA".into()]);
        let mut r = load(&c);
        let k = take(&c, &mut r, &id).unwrap();
        let plan = plan_in(&c, &r, k, &roots, true);
        assert_eq!(plan.kept, ["Sceneryobjects/Shared"]);
        assert!(plan.freed.is_empty(), "{:?}", plan.freed);
        record(&c, "Town.zip", &["Archives/Town.zip".into()]);
        put(&c.join("Archives/Town.zip"), "x");
        let town = list(&c).into_iter().find(|m| m.name == "Town.zip").unwrap();
        set_enabled_in(&c, &town.id, false, &roots).unwrap();
        std::fs::remove_dir_all(c.join("Sceneryobjects/ValPack")).unwrap();
        let mut r = load(&c);
        let k = take(&c, &mut r, &id).unwrap();
        let plan = plan_in(&c, &r, k, &roots, true);
        assert!(plan.freed.is_empty() && plan.kept == ["Sceneryobjects/Shared", "Splines/RoadA"], "{plan:?}");
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[test]
    fn what_a_kept_pack_reaches_stays_and_nothing_owned_goes_when_a_map_cannot_be_read() {
        let (c, omsi, id) = two_maps();
        let roots = [c.clone(), omsi.clone()];
        record(&c, "Hill Town", &["Sceneryobjects/Base".into()]);
        put(&c.join("Sceneryobjects/Base/b.sco"), "[mesh]\n..\\..\\PackA\\x.o3d\n");
        put(&c.join("Sceneryobjects/Other/o2.sco"), "[mesh]\n..\\..\\Base\\y.o3d\n");
        let mut r = load(&c);
        let k = take(&c, &mut r, &id).unwrap();
        let plan = plan_in(&c, &r, k, &roots, true);
        assert_eq!(plan.kept, ["Sceneryobjects/Shared", "Sceneryobjects/Base"]);
        assert_eq!(plan.freed, ["Splines/RoadA"]);
        put(&c.join("Archives/Off.zip"), "x");
        let plan = plan_in(&c, &r, k, &roots, true);
        assert!(plan.freed.is_empty() && plan.kept == ["Sceneryobjects/Shared", "Sceneryobjects/Base"], "{plan:?}");
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[test]
    fn a_map_or_pack_not_read_to_the_end_keeps_everything() {
        let (c, omsi, id) = two_maps();
        let roots = [c.clone(), omsi.clone()];
        put(&c.join("Mods/disabled/Old/Sceneryobjects/Old/o.sco"), "[mesh]\n..\\..\\PackA\\a.o3d\n");
        let mut r = load(&c);
        let k = take(&c, &mut r, &id).unwrap();
        assert_eq!(plan_in(&c, &r, k, &roots, true).freed, ["Splines/RoadA"]);
        std::fs::remove_dir_all(c.join("Mods/disabled/Old")).unwrap();
        let deep: PathBuf = (0..20).fold(c.join("maps/Valley"), |p, i| p.join(i.to_string()));
        put(&deep.join("x.map"), "");
        let plan = plan_in(&c, &r, k, &roots, true);
        assert!(plan.freed.is_empty() && plan.kept == ["Sceneryobjects/Shared"], "{plan:?}");
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_switched_off_map_deleted_while_a_map_cannot_be_listed_puts_its_kept_objects_back() {
        let (c, omsi, id) = two_maps();
        std::fs::remove_file(c.join("maps/Valley/parklist_p.txt")).unwrap();
        let roots = [c.clone(), omsi.clone()];
        set_enabled_in(&c, &id, false, &roots).unwrap();
        let id = list(&c).into_iter().find(|m| m.name == "Hill Town").unwrap().id;
        assert!(!c.join("Sceneryobjects/Shared").exists() && disabled_dir(&c, &id).join("Sceneryobjects/Shared").exists());
        std::os::unix::fs::symlink(c.join("Archives"), c.join("Archives")).unwrap();
        remove_in(&c, &id, &roots).unwrap();
        assert!(c.join("Sceneryobjects/Shared/f.txt").exists() && !disabled_dir(&c, &id).exists() && !c.join("maps/Hill Town").exists());
        assert!(c.join("Sceneryobjects/PackA").exists());
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_kept_folder_whose_place_is_taken_stops_the_delete_and_both_copies_stay() {
        let (c, omsi, id) = two_maps();
        std::fs::remove_file(c.join("maps/Valley/parklist_p.txt")).unwrap();
        let roots = [c.clone(), omsi.clone()];
        set_enabled_in(&c, &id, false, &roots).unwrap();
        let id = list(&c).into_iter().find(|m| m.name == "Hill Town").unwrap().id;
        let off = disabled_dir(&c, &id);
        assert!(off.join("Sceneryobjects/Shared/f.txt").exists() && !c.join("Sceneryobjects/Shared").exists());
        // (another mod's folder of the same name, with other files, installed while the map is off)
        put(&c.join("Sceneryobjects/Shared/new.txt"), "other");
        std::os::unix::fs::symlink(c.join("Archives"), c.join("Archives")).unwrap();
        assert!(remove_in(&c, &id, &roots).is_err());
        assert_eq!(std::fs::read_to_string(off.join("Sceneryobjects/Shared/f.txt")).unwrap(), "1234");
        assert_eq!(std::fs::read_to_string(c.join("Sceneryobjects/Shared/new.txt")).unwrap(), "other");
        assert!(!c.join("Sceneryobjects/Shared/f.txt").exists() && off.join("maps/Hill Town/tile_0_0.map").exists());
        assert!(load(&c).mods.iter().any(|m| m.id == id && !m.enabled));
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[test]
    fn deleting_a_switched_off_map_leaves_a_folder_of_the_same_name_installed_meanwhile() {
        let (c, omsi, id) = two_maps();
        std::fs::remove_file(c.join("maps/Valley/parklist_p.txt")).unwrap();
        let roots = [c.clone(), omsi.clone()];
        set_enabled_in(&c, &id, false, &roots).unwrap();
        let id = list(&c).into_iter().find(|m| m.name == "Hill Town").unwrap().id;
        put(&c.join("Sceneryobjects/Shared/new.txt"), "other");
        put(&c.join("maps/Hill Town/global.cfg"), "another map of the same name");
        remove_in(&c, &id, &roots).unwrap();
        assert!(!disabled_dir(&c, &id).exists());
        assert_eq!(std::fs::read_to_string(c.join("Sceneryobjects/Shared/new.txt")).unwrap(), "other");
        assert_eq!(std::fs::read_to_string(c.join("maps/Hill Town/global.cfg")).unwrap(), "another map of the same name");
        std::fs::remove_dir_all(c).unwrap();
        std::fs::remove_dir_all(omsi).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_map_collection_that_cannot_be_listed_keeps_every_object_folder() {
        let (c, omsi, id) = two_maps();
        let more = c.with_extension("more");
        std::fs::create_dir_all(&more).unwrap();
        std::os::unix::fs::symlink(more.join("maps"), more.join("maps")).unwrap();
        let mut r = load(&c);
        let k = take(&c, &mut r, &id).unwrap();
        let roots = [c.clone(), omsi.clone(), more.clone()];
        let plan = plan_in(&c, &r, k, &roots, true);
        assert!(plan.freed.is_empty() && plan.kept == ["Sceneryobjects/Shared"], "{plan:?}");
        std::fs::remove_file(more.join("maps")).unwrap();
        let gone = c.join("Mods/disabled/Gone/maps");
        std::fs::create_dir_all(gone.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&gone, &gone).unwrap();
        assert!(plan_in(&c, &r, k, &roots, true).freed.is_empty());
        std::fs::remove_dir_all(gone.parent().unwrap()).unwrap();
        assert_eq!(plan_in(&c, &r, k, &roots, true).freed, ["Sceneryobjects/PackA", "Splines/RoadA"]);
        let packs = c.join("Mods/disabled/Old/Sceneryobjects");
        std::fs::create_dir_all(packs.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&packs, &packs).unwrap();
        assert!(plan_in(&c, &r, k, &roots, true).freed.is_empty());
        std::fs::remove_dir_all(packs.parent().unwrap()).unwrap();
        let tile = c.join("maps/Hill Town/tile_0_0.map");
        put(&tile, &(std::fs::read_to_string(&tile).unwrap() + "\n[object]\n0\nSceneryobjects\\Twin\\t.sco\n"));
        put(&c.join("Sceneryobjects/Twin/f.txt"), "1234");
        assert_eq!(plan_in(&c, &r, k, &roots, true).freed, ["Sceneryobjects/PackA", "Sceneryobjects/Twin", "Splines/RoadA"]);
        std::fs::rename(omsi.join("Sceneryobjects"), omsi.join("Objects")).unwrap();
        std::os::unix::fs::symlink(omsi.join("Sceneryobjects"), omsi.join("Sceneryobjects")).unwrap();
        assert_eq!(plan_in(&c, &r, k, &roots, true).freed, ["Splines/RoadA"]);
        std::fs::remove_file(omsi.join("Sceneryobjects")).unwrap();
        std::os::unix::fs::symlink(c.join("Archives"), c.join("Archives")).unwrap();
        assert!(plan_in(&c, &r, k, &roots, true).freed.is_empty());
        std::fs::remove_file(c.join("Archives")).unwrap();
        for d in [c, omsi, more] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }
}
