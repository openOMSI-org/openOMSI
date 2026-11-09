use omsi_cfg::vfs;
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SHARED: [&str; 2] = ["sceneryobjects", "splines"];

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub kept: Vec<String>,
    pub freed: Vec<String>,
    pub bytes: u64,
}

pub fn unit_of(rel: &str) -> Option<String> {
    let norm = rel.trim().replace('\\', "/").to_lowercase();
    let mut parts = norm.split('/').filter(|s| !s.is_empty() && *s != ".");
    let top = parts.next()?;
    let name = parts.next()?;
    SHARED.contains(&top).then(|| format!("{top}/{name}"))
}

fn ref_unit(line: &str) -> Option<String> {
    let l = line.trim().trim_start_matches(['.', '\\', '/']);
    let lower = l.to_ascii_lowercase();
    if lower.len() < 9 || !(lower.ends_with(".sco") || lower.ends_with(".sli")) {
        return None;
    }
    unit_of(l)
}

fn key(p: &Path) -> String {
    let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_string()
}

pub fn same(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

fn listing(dir: &Path, lost: &mut bool) -> Vec<(std::ffi::OsString, bool)> {
    vfs::list_dir(dir).unwrap_or_else(|| {
        *lost |= vfs::exists(dir) || std::fs::symlink_metadata(dir).is_ok();
        Vec::new()
    })
}

pub fn child(dir: &Path, name: &str, lost: &mut bool) -> Option<PathBuf> {
    let name = name.to_lowercase();
    listing(dir, lost).into_iter().find(|(n, _)| n.to_string_lossy().to_lowercase() == name).map(|(n, _)| dir.join(n))
}

pub fn subdirs(dir: &Path, lost: &mut bool) -> Vec<PathBuf> {
    listing(dir, lost).into_iter().filter(|(_, d)| *d).map(|(n, _)| dir.join(n)).collect()
}

fn text_files(dir: &Path, exts: &[&str], depth: u32, out: &mut Vec<PathBuf>) -> bool {
    let Some(list) = vfs::list_dir(dir) else { return false };
    let mut whole = true;
    for (name, is_dir) in list {
        let p = dir.join(&name);
        if is_dir {
            whole &= depth < 16 && text_files(&p, exts, depth + 1, out);
        } else if Path::new(&name).extension().is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x))) {
            out.push(p);
        }
    }
    whole
}

fn stamp(files: &[PathBuf]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in files {
        f.hash(&mut h);
        vfs::file_size(f).hash(&mut h);
        std::fs::metadata(f).and_then(|m| m.modified()).ok().hash(&mut h);
    }
    h.finish()
}

static SEEN: Mutex<Vec<(String, u64, Vec<String>)>> = Mutex::new(Vec::new());

pub fn forget() {
    SEEN.lock().unwrap().clear();
}

fn scan(dir: &Path, exts: &[&str], pick: fn(&str) -> Vec<String>) -> Option<Vec<String>> {
    let mut files = Vec::new();
    if !text_files(dir, exts, 0, &mut files) {
        return None;
    }
    files.sort();
    let (k, s) = (format!("{}|{}", exts.join(","), key(dir)), stamp(&files));
    if let Some((_, _, u)) = SEEN.lock().unwrap().iter().find(|(d, st, _)| *d == k && *st == s) {
        return Some(u.clone());
    }
    let mut set = BTreeSet::new();
    for f in &files {
        let text = vfs::read_text(f).ok()?;
        set.extend(omsi_cfg::split_lines(&text).iter().flat_map(|l| pick(l)));
    }
    let u: Vec<String> = set.into_iter().collect();
    let mut seen = SEEN.lock().unwrap();
    seen.retain(|(d, _, _)| *d != k);
    seen.push((k, s, u.clone()));
    Some(u)
}

pub fn uses(map_dir: &Path) -> Option<Vec<String>> {
    scan(map_dir, &["map", "cfg", "txt"], |l| ref_unit(l).into_iter().collect())
}

fn up_links(line: &str) -> Vec<String> {
    if !line.contains("..") {
        return Vec::new();
    }
    line.to_lowercase().split(['/', '\\']).map(str::trim).filter(|c| !c.is_empty() && *c != "..").map(str::to_string).collect()
}

fn pack_links(content: &Path) -> Option<Vec<(String, Vec<String>)>> {
    let mut lost = false;
    let mut bases = vec![content.to_path_buf()];
    bases.extend(subdirs(&content.join("Mods").join("disabled"), &mut lost));
    let mut out = Vec::new();
    for base in bases {
        for top in SHARED {
            let Some(top_dir) = child(&base, top, &mut lost) else { continue };
            for pack in subdirs(&top_dir, &mut lost) {
                let unit = format!("{top}/{}", pack.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
                out.push((unit, scan(&pack, &["sco", "sli", "cfg", "txt"], up_links)?));
            }
        }
    }
    (!lost).then_some(out)
}

pub struct Scope<'a> {
    pub mine: Vec<PathBuf>,
    pub others: Vec<PathBuf>,
    pub owned: &'a [String],
    pub foreign: Vec<String>,
    pub blind: bool,
    pub maps: bool,
}

fn name_of(unit: &str) -> &str {
    unit.split_once('/').map(|(_, n)| n).unwrap_or(unit)
}

pub fn plan(content: &Path, roots: &[PathBuf], s: &Scope, full: bool) -> Plan {
    if !s.maps {
        return Plan::default();
    }
    let mut blind = s.blind;
    let mut used: HashSet<String> = HashSet::new();
    for d in &s.others {
        match uses(d) {
            Some(u) => used.extend(u),
            None => blind = true,
        }
    }
    let owned_units: HashSet<String> = s.owned.iter().filter_map(|p| unit_of(p)).collect();
    let foreign_units: HashSet<String> = s.foreign.iter().filter_map(|p| unit_of(p)).collect();
    let elsewhere: Vec<&PathBuf> = roots.iter().filter(|r| !same(r, content)).collect();
    let in_root = |root: &Path, unit: &str, lost: &mut bool| unit.split_once('/').and_then(|(t, n)| child(root, t, lost).and_then(|d| child(&d, n, lost))).filter(|p| vfs::is_dir(p));
    let mut cands: Vec<(String, PathBuf)> = Vec::new();
    if full && !blind && elsewhere.iter().any(|r| !r.starts_with(content)) {
        for u in s.mine.iter().flat_map(|d| uses(d).unwrap_or_default()).collect::<BTreeSet<_>>() {
            if used.contains(&u) || owned_units.contains(&u) || foreign_units.contains(&u) || elsewhere.iter().any(|r| {
                let mut lost = false;
                in_root(r, &u, &mut lost).is_some() || lost
            }) {
                continue;
            }
            if let Some(dir) = in_root(content, &u, &mut false).filter(|p| p.is_dir()) {
                cands.push((u, dir));
            }
        }
    }
    let links = if cands.is_empty() && owned_units.iter().all(|u| blind || used.contains(u)) { Some(Vec::new()) } else { pack_links(content) };
    let mut stay: HashSet<String> = owned_units.iter().filter(|u| blind || links.is_none() || used.contains(*u)).cloned().collect();
    if links.is_none() {
        cands.clear();
    }
    let links = links.unwrap_or_default();
    loop {
        let going: HashSet<String> = cands.iter().map(|(u, _)| u).chain(&owned_units).filter(|u| !stay.contains(*u)).cloned().collect();
        let names: HashSet<&str> = going.iter().map(|u| name_of(u)).collect();
        let linked: HashSet<&str> = links.iter().filter(|(u, _)| !going.contains(u)).flat_map(|(_, l)| l.iter().map(String::as_str)).filter(|c| names.contains(c)).collect();
        let newly: Vec<String> = going.iter().filter(|u| linked.contains(name_of(u))).cloned().collect();
        if newly.is_empty() {
            break;
        }
        stay.extend(newly);
    }
    let kept = s.owned.iter().filter(|p| unit_of(p).is_some_and(|u| stay.contains(&u))).cloned().collect();
    let freed = cands
        .iter()
        .filter(|(u, _)| !stay.contains(u))
        .filter_map(|(_, d)| d.strip_prefix(content).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .collect();
    Plan { kept, freed, bytes: 0 }
}
