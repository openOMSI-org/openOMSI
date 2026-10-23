use super::*;
use anyhow::bail;
use crate::i18n::{tr, trf};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MapHofOffer {
    pub map: String,
    pub files: Vec<String>,
}

struct MapHofs {
    folder: String,
    map: PathBuf,
    sources: Vec<PathBuf>,
}

type Warning = (&'static str, Vec<(&'static str, String)>);
type Scan = (String, Vec<MapHofOffer>, Vec<Warning>);

fn last_scan() -> &'static std::sync::Mutex<Option<Scan>> {
    static SCAN: std::sync::Mutex<Option<Scan>> = std::sync::Mutex::new(None);
    &SCAN
}

fn writable_content() -> Option<(PathBuf, PathBuf)> {
    let content = content_dir()?;
    let original = root().ok()?;
    let canonical_content = std::fs::canonicalize(&content).unwrap_or_else(|_| content.clone());
    let canonical_original = std::fs::canonicalize(&original).unwrap_or_else(|_| original.clone());
    (canonical_content != canonical_original).then_some((content, original))
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

fn install_unit(map_rel: &str) -> String {
    let mut parts = map_rel.split('/');
    match (parts.next(), parts.next()) {
        (Some(top), Some(archive)) if top.eq_ignore_ascii_case(install::ARCHIVES) => format!("{top}/{archive}"),
        _ => map_rel.to_string(),
    }
}

fn sync_stamp() -> String {
    let roots = omsi_cfg::content_roots();
    let mut stamp = index::content_stamp(&roots, None);
    let hof_dirs: Vec<PathBuf> = roots.iter().map(|r| r.join("HOFs")).collect();
    stamp.push_str(&format!("-{:016x}", index::folder_stamp(&hof_dirs)));
    stamp
}

fn is_hof(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hof"))
}

fn hof_is_called(p: &Path, name: &str) -> bool {
    p.file_stem().is_some_and(|s| s.to_string_lossy().trim().eq_ignore_ascii_case(name)) || omsi_vehicle::Hof::read_name(p).is_some_and(|n| n.trim().eq_ignore_ascii_case(name))
}

fn depot_name(raw: &str) -> &str {
    let name = raw.trim();
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name);
    match name.len().checked_sub(4) {
        Some(cut) if name.get(cut..).is_some_and(|ext| ext.eq_ignore_ascii_case(".hof")) => &name[..cut],
        _ => name,
    }
}

fn map_depot_source(map: &Path, name: &str) -> Option<PathBuf> {
    omsi_cfg::mirrored_dirs(map)
        .iter()
        .flat_map(|d| omsi_cfg::vfs::read_dir_paths(d))
        .find(|p| is_hof(p) && hof_is_called(p, name))
        .or_else(|| omsi_vehicle::hof::depot_anywhere(name).map(|h| h.path))
}

fn copy_to_shared(content: &Path, file_name: &str, bytes: &[u8]) -> Result<Option<String>> {
    let dir = content_case_path(content, "HOFs");
    if !path_stays_in_content(content, &dir) {
        bail!(tr("the shared HOFs folder is outside the writable content folder"));
    }
    std::fs::create_dir_all(&dir)?;
    let dest = content_case_path(content, &format!("HOFs/{file_name}"));
    let mut output = match std::fs::OpenOptions::new().write(true).create_new(true).open(&dest) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if let Err(e) = std::io::Write::write_all(&mut output, bytes) {
        drop(output);
        let _ = std::fs::remove_file(&dest);
        return Err(e.into());
    }
    Ok(Some(relative(content, &dest)))
}

fn worded(w: &Warning, text: impl Fn(&str) -> String) -> String {
    w.1.iter().fold(text(w.0), |s, (k, v)| s.replace(&format!("{{{k}}}"), v))
}

fn scan(content: &Path, original: &Path, warnings: &mut Vec<Warning>) -> Vec<MapHofs> {
    let shared: Vec<PathBuf> = omsi_cfg::read_dir_merged("HOFs").into_iter().filter(|p| is_hof(p)).collect();
    let mut out = Vec::new();
    for (folder, dirs) in merged_folders("maps") {
        let Some(map) = dirs.into_iter().find(|d| in_content(d) && !(original.starts_with(content) && d.starts_with(original)) && omsi_cfg::vfs::is_file(&d.join("global.cfg"))) else { continue };
        let ailists_path = map.join("ailists.cfg");
        if !omsi_cfg::vfs::is_file(&ailists_path) {
            continue;
        }
        let ailists = match omsi_map::ailists::AiLists::load(&ailists_path) {
            Ok(ailists) => ailists,
            Err(e) => {
                warnings.push(("{folder}: couldn't read ailists.cfg: {e}", vec![("folder", folder.clone()), ("e", e.to_string())]));
                continue;
            }
        };
        let mut names: Vec<&str> = ailists.groups.iter().filter_map(|g| g.hof.as_deref()).map(depot_name).filter(|n| !n.is_empty()).collect();
        names.sort_by_key(|n| n.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let mut sources = Vec::new();
        for name in names {
            if shared.iter().any(|p| hof_is_called(p, name)) {
                continue;
            }
            match map_depot_source(&map, name) {
                Some(source) if source.file_name().is_some() => sources.push(source),
                _ => warnings.push(("{folder}: the HOF file '{name}.hof' it needs isn't installed.", vec![("folder", folder.clone()), ("name", name.to_string())])),
            }
        }
        if !sources.is_empty() {
            out.push(MapHofs { folder, map, sources });
        }
    }
    out
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

pub(super) fn offers() -> (Vec<MapHofOffer>, Vec<String>) {
    let Some((content, original)) = writable_content() else { return Default::default() };
    let mut last = last_scan().lock().unwrap_or_else(|e| e.into_inner());
    if install::jobs().iter().any(|j| j.finished.is_none()) {
        return last.as_ref().map(|(_, o, w)| (o.clone(), w.iter().map(|w| worded(w, tr)).collect())).unwrap_or_default();
    }
    let stamp = sync_stamp();
    if let Some((_, o, w)) = last.as_ref().filter(|(s, ..)| *s == stamp) {
        return (o.clone(), w.iter().map(|w| worded(w, tr)).collect());
    }
    omsi_cfg::content_changed();
    let mut warnings = Vec::new();
    let offers: Vec<MapHofOffer> = scan(&content, &original, &mut warnings).into_iter().map(|m| MapHofOffer { map: m.folder, files: m.sources.iter().map(|p| file_name(p)).collect() }).collect();
    warnings.sort();
    warnings.dedup();
    for w in &warnings {
        log_line(&format!("maps: HOF sync warning: {}", worded(w, str::to_string)));
    }
    let shown = warnings.iter().map(|w| worded(w, tr)).collect();
    *last = Some((stamp, offers.clone(), warnings));
    (offers, shown)
}

pub fn share_map_hofs(map: &str) -> Result<usize> {
    let Some((content, original)) = writable_content() else {
        bail!(tr("the writable content folder is the original OMSI folder; HOF files were not copied"));
    };
    if install::jobs().iter().any(|j| j.finished.is_none()) {
        bail!(tr("a mod is being installed; share the HOF files once it is done"));
    }
    let Some(found) = scan(&content, &original, &mut Vec::new()).into_iter().find(|m| m.folder.eq_ignore_ascii_case(map)) else { return Ok(0) };
    let mut added = Vec::new();
    let mut failures = Vec::new();
    for source in &found.sources {
        let name = file_name(source);
        match omsi_cfg::vfs::read(source).map_err(anyhow::Error::from).and_then(|bytes| copy_to_shared(&content, &name, &bytes)) {
            Ok(Some(rel)) => added.push(rel),
            Ok(None) => failures.push(trf("HOFs/{name} already holds another depot", &[("name", &name)])),
            Err(e) => failures.push(trf("couldn't share {name} with every bus: {e}", &[("name", &name), ("e", &e)])),
        }
    }
    if !added.is_empty() {
        mods::attach(&content, &install_unit(&relative(&content, &found.map)), &added);
        log_line(&format!("maps: shared {} HOF file(s) of {map} with every bus in {}", added.len(), content.join("HOFs").display()));
        omsi_cfg::content_changed();
    }
    *last_scan().lock().unwrap_or_else(|e| e.into_inner()) = None;
    if !failures.is_empty() {
        bail!(trf("{map}: shared {count}; {failures}", &[("map", &map), ("count", &added.len()), ("failures", &failures.join("; "))]));
    }
    Ok(added.len())
}

pub fn add_shared_hofs(files: &[PathBuf]) -> Result<usize> {
    let Some((content, _)) = writable_content() else {
        bail!(tr("the writable content folder is the original OMSI folder; HOF files were not copied"));
    };
    if files.is_empty() {
        bail!(tr("no HOF files were selected"));
    }
    let mut copied = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::new();
    for source in files {
        let Some(file_name) = source.file_name().map(|n| n.to_string_lossy().to_string()) else {
            failures.push(trf("{file} has no file name", &[("file", &source.display())]));
            continue;
        };
        if !is_hof(source) {
            failures.push(trf("{file_name} is not a .hof file", &[("file_name", &file_name)]));
            continue;
        }
        match std::fs::read(source).map_err(anyhow::Error::from).and_then(|bytes| copy_to_shared(&content, &file_name, &bytes)) {
            Ok(Some(_)) => copied += 1,
            Ok(None) => skipped += 1,
            Err(e) => failures.push(trf("couldn't add {file_name}: {e}", &[("file_name", &file_name), ("e", &e)])),
        }
    }
    if copied > 0 {
        omsi_cfg::content_changed();
    }
    if !failures.is_empty() {
        bail!(trf("added {copied} file(s), {skipped} already there; {failures}", &[("copied", &copied), ("skipped", &skipped), ("failures", &failures.join("; "))]));
    }
    Ok(copied)
}

fn content_case_path(base: &Path, rel: &str) -> PathBuf {
    let mut current = base.to_path_buf();
    for component in rel.split('/').filter(|c| !c.is_empty()) {
        let direct = current.join(component);
        if direct.exists() {
            current = direct;
            continue;
        }
        let found = std::fs::read_dir(&current).ok().and_then(|rd| rd.flatten().map(|e| e.file_name()).find(|n| n.to_string_lossy().eq_ignore_ascii_case(component)));
        current = found.map(|name| current.join(name)).unwrap_or(direct);
    }
    current
}

fn path_stays_in_content(content: &Path, path: &Path) -> bool {
    let base = std::fs::canonicalize(content).unwrap_or_else(|_| content.to_path_buf());
    let mut ancestor = path;
    while !ancestor.exists() {
        let Some(parent) = ancestor.parent() else { return false };
        ancestor = parent;
    }
    std::fs::canonicalize(ancestor).map(|p| p.starts_with(base)).unwrap_or(false)
}
