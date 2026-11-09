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
    p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase()
}

pub fn same(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

pub fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    let name = name.to_lowercase();
    vfs::list_dir(dir)?.into_iter().find(|(n, _)| n.to_string_lossy().to_lowercase() == name).map(|(n, _)| dir.join(n))
}

pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
    vfs::list_dir(dir).unwrap_or_default().into_iter().filter(|(_, d)| *d).map(|(n, _)| dir.join(n)).collect()
}

fn text_files(dir: &Path, exts: &[&str], depth: u32, out: &mut Vec<PathBuf>) {
    for (name, is_dir) in vfs::list_dir(dir).unwrap_or_default() {
        let p = dir.join(&name);
        if is_dir {
            if depth < 8 {
                text_files(&p, exts, depth + 1, out);
            }
        } else if Path::new(&name).extension().is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x))) {
            out.push(p);
        }
    }
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

fn scan(dir: &Path, exts: &[&str], pick: fn(&str) -> Vec<String>) -> Vec<String> {
    let mut files = Vec::new();
    text_files(dir, exts, 0, &mut files);
    files.sort();
    let (k, s) = (format!("{}|{}", exts.join(","), key(dir)), stamp(&files));
    if let Some((_, _, u)) = SEEN.lock().unwrap().iter().find(|(d, st, _)| *d == k && *st == s) {
        return u.clone();
    }
    let mut set = BTreeSet::new();
    for f in &files {
        let Ok(text) = vfs::read_text(f) else { continue };
        set.extend(omsi_cfg::split_lines(&text).iter().flat_map(|l| pick(l)));
    }
    let u: Vec<String> = set.into_iter().collect();
    let mut seen = SEEN.lock().unwrap();
    seen.retain(|(d, _, _)| *d != k);
    seen.push((k, s, u.clone()));
    u
}

pub fn uses(map_dir: &Path) -> Vec<String> {
    scan(map_dir, &["map", "cfg", "txt"], |l| ref_unit(l).into_iter().collect())
}

fn up_links(line: &str) -> Vec<String> {
    if !line.contains("..") {
        return Vec::new();
    }
    line.to_lowercase().split(['/', '\\']).map(str::trim).filter(|c| !c.is_empty() && *c != "..").map(str::to_string).collect()
}

fn linked_from(content: &Path, skip: &HashSet<String>, names: &HashSet<String>) -> HashSet<String> {
    let mut found = HashSet::new();
    for top in SHARED {
        let Some(top_dir) = child(content, top) else { continue };
        for pack in subdirs(&top_dir) {
            let unit = format!("{top}/{}", pack.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
            if !skip.contains(&unit) {
                found.extend(scan(&pack, &["sco", "sli", "cfg", "txt"], up_links).into_iter().filter(|c| names.contains(c)));
            }
        }
    }
    found
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
    let used: HashSet<String> = s.others.iter().flat_map(|d| uses(d)).collect();
    let owned_units: HashSet<String> = s.owned.iter().filter_map(|p| unit_of(p)).collect();
    let foreign_units: HashSet<String> = s.foreign.iter().filter_map(|p| unit_of(p)).collect();
    let elsewhere: Vec<&PathBuf> = roots.iter().filter(|r| !same(r, content)).collect();
    let in_root = |root: &Path, unit: &str| unit.split_once('/').and_then(|(t, n)| child(root, t).and_then(|d| child(&d, n))).filter(|p| vfs::is_dir(p));
    let mut cands: Vec<(String, PathBuf)> = Vec::new();
    if full && !s.blind && elsewhere.iter().any(|r| !r.starts_with(content)) {
        for u in s.mine.iter().flat_map(|d| uses(d)).collect::<BTreeSet<_>>() {
            if used.contains(&u) || owned_units.contains(&u) || foreign_units.contains(&u) || elsewhere.iter().any(|r| in_root(r, &u).is_some()) {
                continue;
            }
            if let Some(dir) = in_root(content, &u).filter(|p| p.is_dir()) {
                cands.push((u, dir));
            }
        }
    }
    let mut stay: HashSet<String> = owned_units.iter().filter(|u| s.blind || used.contains(*u)).cloned().collect();
    loop {
        let going: HashSet<String> = cands.iter().map(|(u, _)| u).chain(&owned_units).filter(|u| !stay.contains(*u)).cloned().collect();
        let names: HashSet<String> = going.iter().map(|u| name_of(u).to_string()).collect();
        if names.is_empty() {
            break;
        }
        let linked = linked_from(content, &going, &names);
        let newly: Vec<String> = going.into_iter().filter(|u| linked.contains(name_of(u))).collect();
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
