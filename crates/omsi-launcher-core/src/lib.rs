//! The launcher's data side: what the window asks for (maps, buses, lines, tours, the
//! roadbook, weather, profiles, settings) and how a duty is turned into a command line for
//! the game. Everything here is plain functions over the OMSI content crates; the window
//! (the game binary's `launcher` module, drawn with wgpu) calls them directly, and
//! `cli()` exposes the same functions to a terminal (`omsi-launcher --cli ...`).
//!
//! `install` runs mod installs as background jobs, `index` caches the content lists and
//! tells the page when they changed, `instances` keeps track of the games started.

pub mod crash;
pub mod depot;
pub mod index;
pub mod install;
pub mod instances;
pub mod mods;
pub mod optimise;
mod hof_sync;
mod file_dialog;

pub use file_dialog::{pick_file, pick_folder, pick_hofs, pick_mod};
pub use hof_sync::add_shared_hofs;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------
// configuration: where the game and the OMSI content are

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// The OMSI 2 folder (maps, Vehicles, ...).
    pub root: String,
    /// The `omsi` game binary.
    pub game: String,
    /// The current profile name.
    pub profile: String,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

pub fn data_dir() -> PathBuf {
    // (made once: asked for dozens of times a second - the launcher's frames, its polls - and
    // made each time, two system calls for a folder that was there)
    static MADE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
    let d = home().join(".openomsi");
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if made.as_ref() != Some(&d) && std::fs::create_dir_all(&d).is_ok() {
        *made = Some(d.clone());
    }
    d
}

fn config_path() -> PathBuf {
    data_dir().join("launcher.json")
}

/// A complete installation of the original OMSI 2 (the same test the game makes: it refuses
/// to start on anything else).
fn is_omsi_root(p: &Path) -> bool {
    omsi_cfg::missing_original_essentials(p).is_empty()
}

/// Where the game binary is: the configured path, next to the launcher, or the
/// development build in the source tree.
fn find_game(configured: &str) -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    // the game that came with this launcher first: a path remembered from an older
    // installation (`target/release/omsi` of the days before the rename) kept starting an
    // old build after every update - the new pause menu "was not there" on macOS
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join(if cfg!(windows) { "openomsi.exe" } else { "openomsi" }));
        }
    }
    if !configured.trim().is_empty() {
        let c = PathBuf::from(configured.trim());
        // (only a game of today's name: the old `omsi` binary is not taken any more)
        let stem = c.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if stem == "openomsi" {
            cands.push(c);
        }
    }
    if let Some(p) = std::env::var_os("OPENOMSI_BIN") {
        cands.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        for a in exe.ancestors().skip(1).take(7) {
            cands.push(a.join("openomsi"));
            cands.push(a.join("openomsi.exe"));
            cands.push(a.join("target").join("release").join("openomsi"));
            cands.push(a.join("target").join("release").join("openomsi.exe"));
            cands.push(a.join("Resources").join("openomsi"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for a in cwd.ancestors().take(4) {
            cands.push(a.join("target").join("release").join("openomsi"));
        }
    }
    cands.push(data_dir().join("openomsi"));
    cands.into_iter().find(|p| p.is_file())
}

/// The last search for the OMSI folder: (the places given first, what was found).
static FOUND: std::sync::Mutex<Option<(Vec<PathBuf>, Option<PathBuf>)>> = std::sync::Mutex::new(None);

/// The OMSI folder: configured, remembered by the game, or found in any usual place
/// (beside the program, Steam libraries, Wine bottles, the user's folders).
fn find_root(configured: &str) -> Option<PathBuf> {
    let mut first: Vec<PathBuf> = Vec::new();
    if !configured.trim().is_empty() {
        first.push(PathBuf::from(configured.trim()));
    }
    if let Some(p) = omsi_cfg::flags::OMSI_ROOT.live_os() {
        first.push(PathBuf::from(p));
    }
    if let Ok(t) = std::fs::read_to_string(home().join(".openomsi-root")) {
        first.push(PathBuf::from(t.trim()));
    }
    // searching the disk costs a moment: once per process is enough (until the settings
    // are saved again, see `save_config`)
    let mut g = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, v)) = g.as_ref() {
        if *k == first && v.as_ref().map(|p| is_omsi_root(p)).unwrap_or(true) {
            return v.clone();
        }
    }
    let r = omsi_cfg::find_original_install(&first);
    if let Some(p) = &r {
        // the game finds it the same way next time
        let _ = std::fs::write(home().join(".openomsi-root"), p.to_string_lossy().as_bytes());
    }
    *g = Some((first, r.clone()));
    r
}

pub fn load_config() -> Config {
    let mut c: Config = std::fs::read_to_string(config_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if let Some(r) = find_root(&c.root) {
        c.root = r.to_string_lossy().to_string();
    }
    if let Some(g) = find_game(&c.game) {
        c.game = g.to_string_lossy().to_string();
    }
    if c.profile.trim().is_empty() {
        // a first start takes the driver OMSI 2 had last ([last_driver] of its options.cfg)
        c.profile = Path::new(&c.root)
            .is_dir()
            .then(|| omsi_options(Path::new(&c.root)))
            .flatten()
            .and_then(|o| o.last_driver)
            .unwrap_or_else(|| "Driver".into());
    }
    c
}

/// What the player's own OMSI 2 remembers in its `options.cfg`: the settings in the
/// launcher's keys, the map and the driver played last.
pub struct OmsiOptions {
    pub settings: Value,
    /// `maps/<name>/global.cfg`, as the launcher names maps.
    pub last_map: Option<String>,
    /// The personnel file's name without `.odr`.
    pub last_driver: Option<String>,
}

/// Read the original's `options.cfg` (never written: the game keeps its own settings).
pub fn omsi_options(root: &Path) -> Option<OmsiOptions> {
    let o = omsi_content::options::Options::load(&root.join("options.cfg")).ok()?;
    let mut v = json!({});
    let num = |k: &str| o.str(k).and_then(|x| x.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite());
    // (not on a phone: the PC's OMSI caps at 30, and a phone played at 30 frames)
    if let Some(x) = num("maxfps").filter(|_| !cfg!(any(target_os = "android", target_os = "ios"))) {
        v["max_fps"] = json!(x.max(0.0) as i64);
    }
    if let Some(x) = num("performance_minobjsize") {
        v["min_obj_size"] = json!(x.clamp(0.0, 0.2));
    }
    if let Some(x) = num("performance_maxobjdist") {
        v["max_obj_dist"] = json!((x.round() as i64).max(0).to_string());
    }
    if let Some(af) = o.values.get("texfilter").and_then(|x| x.get(1)).and_then(|x| x.trim().parse::<i64>().ok()) {
        v["anisotropy"] = json!(af.clamp(1, 16));
    }
    // (`texmemlimit`, OMSI's texture budget, is not taken: openOMSI's video memory bounds
    // everything on the card, and the stock 401 would leave it 401 MB)
    if let Some(x) = num("performance_refltexsize") {
        v["mirror_size"] = json!(1i64 << (x as i64).clamp(6, 11));
    }
    if let Some(x) = o.str("performance_realreflexions") {
        v["mirror_refresh"] = json!(mirror_refresh(x));
    }
    if let Some(x) = num("sound_vol_master") {
        v["volume"] = json!(x.clamp(0.0, 1.0));
    }
    if let Some(d) = o.str("sound_doppler") {
        v["doppler"] = json!(!d.trim().eq_ignore_ascii_case("off"));
    }
    if let Some(l) = o.str("language").filter(|l| !l.trim().is_empty()) {
        v["language"] = json!(language_code(l));
    }
    if let Some(x) = num("wear_lifespan") {
        v["maintenance"] = json!((x as i64).clamp(0, 4));
    }
    if let Some(x) = num("aiunschedfactor") {
        v["ai_unsched_factor"] = json!((x as i64).clamp(0, 300));
    }
    if let Some(x) = num("aimaxcountscheduled") {
        v["ai_max_scheduled"] = json!((x as i64).max(0));
    }
    // (the second line of [AIMaxCountRandom]: the people Omsi.exe makes)
    if let Some(x) = o.values.get("aimaxcountrandom").and_then(|x| x.get(1)).and_then(|x| x.trim().parse::<i64>().ok()) {
        v["ai_max_humans"] = json!(x.max(1));
    }
    if let Some(x) = num("aimaxcountparked") {
        v["ai_max_parked"] = json!((x as i64).max(0));
    }
    if let Some(x) = num("aipassfactor") {
        v["pax_density"] = json!((x / 100.0).clamp(0.0, 3.0));
    }
    // flags: present or not
    v["head_movement"] = json!(o.flag("driverview_moving"));
    v["driverview_smooth"] = json!(o.flag("driverview_smooth"));
    v["collision_vehicles"] = json!(!o.flag("no_collision_vehtoveh"));
    v["collision_objects"] = json!(!o.flag("no_collision"));
    v["driver"] = json!(o.flag("see_own_driver"));
    if let Some(x) = num("ticketselling") {
        v["boarding"] = json!(if x > 0.5 { "pay" } else { "auto" });
    }
    let file_stem = |p: &str| Path::new(&p.replace('\\', "/")).file_stem().map(|s| s.to_string_lossy().to_string());
    Some(OmsiOptions {
        settings: v,
        last_map: o.str("last_map").map(|m| m.trim().replace('\\', "/")).filter(|m| !m.is_empty()),
        last_driver: o.str("last_driver").and_then(file_stem).filter(|d| !d.is_empty()),
    })
}

pub fn save_config(c: &Config) -> Result<()> {
    std::fs::write(config_path(), serde_json::to_vec_pretty(c)?)?;
    // the folder is looked for again: a folder that was not a complete OMSI 2 when it was
    // first chosen (still being copied, a part missing) and is now stayed "not found" until
    // the launcher was restarted, however often it was chosen and saved again
    *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(())
}

/// The OMSI 2 folder the launcher works with (configured or found).
pub fn root_dir() -> Result<PathBuf> {
    root()
}

/// The game program a duty is started with (see `find_game`).
pub fn game_binary() -> Option<PathBuf> {
    find_game(&load_config().game)
}

/// Where a content path (`maps/Grundorf/picture.jpg`) is: the content folder and the
/// archives first, then the OMSI 2 folder.
pub fn content_path(rel: &str) -> Result<PathBuf> {
    resolve_content(rel)
}

/// The buses the map's ailists.cfg gives a tour's AI group on `date` (#1886: to drive a
/// model the AI drives on it): of a depot group each vehicle type with a bus that runs
/// that day - the file, the first such bus's repaint and fleet number -, else the group's
/// vehicles, the most likely first. (file, paint, number)
pub fn tour_buses(map: &str, group: &str, date: &str) -> Vec<(String, String, String)> {
    let want = group.trim();
    let (Ok(dir), Some(day)) = (resolve_content(map), omsi_map::ailists::date_code(date)) else { return Vec::new() };
    let Some(dir) = dir.parent().filter(|_| !want.is_empty()) else { return Vec::new() };
    let lists = omsi_map::ailists::ailists_with_chrono(dir, &omsi_map::ailists::active_chrono_dirs(dir, day));
    let Some(g) = lists.groups.iter().find(|g| g.name.trim().eq_ignore_ascii_case(want)) else { return Vec::new() };
    let file = |f: &str| f.trim().replace('\\', "/");
    let mut depot: Vec<(String, String, String)> = g
        .typgroups
        .iter()
        .filter_map(|t| t.entries.iter().find(|e| e.from.is_none_or(|f| f <= day) && e.to.is_none_or(|to| day <= to)).map(|e| (file(&t.file), e.paint.trim().to_string(), e.number.trim().to_string())))
        .collect();
    // (a type listed in several groups of the depot: once)
    let mut seen = std::collections::HashSet::new();
    depot.retain(|(f, _, _)| seen.insert(f.to_ascii_lowercase()));
    if !depot.is_empty() {
        return depot;
    }
    let mut v: Vec<_> = g.vehicles.iter().collect();
    v.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    v.into_iter().map(|v| (file(&v.file), String::new(), v.number.clone().unwrap_or_default().trim().to_string())).collect()
}

/// The depot file a bus uses on `map` on `date`: the map's own buses' of that day (chrono
/// scenarios move it - Berlin's 1994 depot has 137, 1986's 92), one of the bus's carrying
/// the line driven, its own of that name or named like the map, else the first (#896).
pub fn default_hof(map: Option<&MapInfo>, bus: Option<&VehicleInfo>, date: &str, line: Option<&str>) -> String {
    let on_date = map.and_then(|m| {
        let dir = resolve_content(&m.file).ok()?;
        omsi_map::ailists::depot_hof_on(dir.parent()?, omsi_map::ailists::date_code(date)?)
    });
    let want = on_date.or_else(|| map.map(|m| m.hof.clone())).unwrap_or_default();
    let Some(v) = bus else { return want };
    let names: Vec<&str> = v.hofs.iter().map(|h| h.as_str()).collect();
    let like = |hints: &[&str]| omsi_vehicle::hof::closest_name(&names, hints).map(|i| v.hofs[i].clone());
    let map_hints: Vec<String> = map.map(|m| vec![m.name.clone(), m.friendly.clone(), m.file.trim_end_matches("/global.cfg").rsplit('/').next().unwrap_or("").to_string()]).unwrap_or_default();
    let map_hints: Vec<&str> = map_hints.iter().map(|h| h.as_str()).collect();
    let with_line = match (line, root()) {
        (Some(line), Ok(r)) => depot_names_with_line(&r, &v.file, line),
        _ => Vec::new(),
    };
    // (not over the map's own depot borrowed from elsewhere when it carries the line: a bus
    // without it took a depot of its folder that had a line of the same number - the map's was
    // no longer given to a bus that lacks it, #1837)
    let map_depot_has_line = match (line, want.is_empty()) {
        (Some(line), false) => depot_has_line(&want, line) == Some(true),
        _ => false,
    };
    if !with_line.is_empty() && !map_depot_has_line && !with_line.iter().any(|h| h.eq_ignore_ascii_case(&want)) {
        let names: Vec<&str> = with_line.iter().map(|h| h.as_str()).collect();
        let hints: Vec<&str> = std::iter::once(want.as_str()).chain(map_hints.iter().copied()).collect();
        let pick = omsi_vehicle::hof::closest_name(&names, &hints).unwrap_or(0);
        return with_line[pick].clone();
    }
    v.hofs
        .iter()
        .find(|h| h.eq_ignore_ascii_case(&want))
        .cloned()
        .or_else(|| like(&[want.as_str()]))
        .or(Some(want.clone()).filter(|w| !w.is_empty()))
        .or_else(|| like(&map_hints))
        .or_else(|| v.hofs.first().cloned())
        .unwrap_or_default()
}

fn root() -> Result<PathBuf> {
    let c = load_config();
    let r = PathBuf::from(&c.root);
    if c.root.trim().is_empty() {
        return Err(anyhow!("no OMSI 2 folder configured (set it under Setup)"));
    }
    let missing = omsi_cfg::missing_original_essentials(&r);
    if !missing.is_empty() {
        return Err(anyhow!(
            "{} is not a complete OMSI 2 installation (missing: {}); choose the original game's folder under Setup",
            r.display(),
            missing.join(", ")
        ));
    }
    Ok(r)
}

/// The game's own content folder: the folder of the game binary, laid out like OMSI 2
/// (Vehicles, maps, Sceneryobjects ...). Installed mods live here; the game searches it
/// before the original installation.
pub fn content_dir() -> Option<PathBuf> {
    // the same rules as the game: $OMSI_CONTENT, else the folder of the game binary (beside
    // the bundle when the binary sits inside a macOS .app)
    let dir = match omsi_cfg::flags::OMSI_CONTENT.live_os() {
        Some(d) => PathBuf::from(d),
        None => {
            let c = load_config_raw();
            let game = find_game(&c.game)?;
            let dir = game.parent()?.to_path_buf();
            let beside = if dir.ends_with("Contents/MacOS") { dir.parent()?.parent()?.parent()?.to_path_buf() } else { dir };
            let cand = omsi_cfg::content_folder_of(&beside);
            if !omsi_cfg::is_programs_folder(&cand) && (cand.exists() || std::fs::create_dir_all(&cand).is_ok()) && omsi_cfg::is_writable(&cand) {
                cand
            } else {
                data_dir().join("content")
            }
        }
    };
    let _ = omsi_cfg::ensure_content_layout(&dir);
    register_roots(&dir);
    Some(dir)
}

/// Tell the OMSI readers about the content folder, the archives used in place in its
/// `Archives` folder and the OMSI folder (in that order, as the game has them), so a path
/// inside one is also looked for in the others (a repaint's `.cti`, a map in an archive).
fn register_roots(content: &Path) {
    static DONE: std::sync::Once = std::sync::Once::new();
    let content = content.to_path_buf();
    DONE.call_once(|| {
        omsi_cfg::add_content_root(content.clone());
        mount_archives(&content);
    });
    // the OMSI folder - also one chosen under Setup while the launcher runs (it was only
    // looked for once, at the start, and a folder set later was never a root)
    if let Some(r) = find_root(&load_config_raw().root) {
        omsi_cfg::add_content_root(r);
    }
}

/// Mount the archives in `<content>/Archives` that are not mounted yet (an install may
/// have put one there, or the user did), each as a content root in front of the OMSI
/// folder. Archives that went away stay mounted until the launcher restarts.
fn mount_archives(content: &Path) {
    let dir = content.join(install::ARCHIVES);
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut zips: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().map(|e| e.eq_ignore_ascii_case("zip")).unwrap_or(false)).collect();
    zips.sort();
    let mounted: Vec<PathBuf> = omsi_cfg::vfs::mounts().iter().map(|m| m.path().to_path_buf()).collect();
    for z in zips.into_iter().filter(|z| !mounted.contains(z)) {
        mount_archive(&z);
    }
}

/// Mount one archive of the content folder for the lists.
pub(crate) fn mount_archive(zip: &Path) {
    match omsi_cfg::vfs::mount_zip(zip) {
        Ok(m) => match find_root(&load_config_raw().root) {
            Some(r) => omsi_cfg::add_content_root_before(m, &r),
            None => omsi_cfg::add_content_root(m),
        },
        Err(e) => {
            log_to_file(&format!("archive {}: {e}", zip.display()));
        }
    }
}

/// The archives of the content folder that are mounted, in name order.
fn archive_roots(content: &Path) -> Vec<PathBuf> {
    let dir = content.join(install::ARCHIVES);
    let mut v: Vec<PathBuf> = omsi_cfg::vfs::mounts().iter().map(|m| m.path().to_path_buf()).filter(|p| p.starts_with(&dir) && p.exists()).collect();
    v.sort();
    v
}

fn load_config_raw() -> Config {
    std::fs::read_to_string(config_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// `rel` (a path as the game takes it, `Vehicles/Foo/foo.bus`) in the content folder if it
/// is there, else in the OMSI folder.
fn resolve_content(rel: &str) -> Result<PathBuf> {
    let root = root()?;
    for b in bases() {
        if b == root {
            continue;
        }
        let p = omsi_cfg::resolve_path(&b, rel);
        if omsi_cfg::vfs::exists(&p) {
            return Ok(p);
        }
    }
    Ok(omsi_cfg::resolve_path(&root, rel))
}

/// The folders the lists are made of: the content folder first, then the archives used in
/// place (mounted as folders), then the OMSI folder.
fn bases() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(c) = content_dir() {
        mount_archives(&c);
        v.push(c.clone());
        v.extend(archive_roots(&c));
    }
    if let Ok(r) = root() {
        v.push(r);
    }
    v
}

/// Entries of `rel` (e.g. "Vehicles") across the content folder and the OMSI 2 folder;
/// a name in the content folder hides the same name in the installation.
fn merged_entries(rel: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let dirs: Vec<PathBuf> = bases().iter().map(|b| b.join(rel)).collect();
    for d in dirs {
        for (name, _) in omsi_cfg::vfs::list_dir(&d).unwrap_or_default() {
            if seen.insert(name.to_string_lossy().to_ascii_lowercase()) {
                out.push(d.join(name));
            }
        }
    }
    out.sort();
    out
}

/// Folders of `rel` by name across the content folder and the OMSI folder: each name with
/// all its copies, the content folder's first (a mod may add files to a stock folder).
fn merged_folders(rel: &str) -> Vec<(String, Vec<PathBuf>)> {
    let mut out: Vec<(String, Vec<PathBuf>)> = Vec::new();
    let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for base in bases() {
        let dir = base.join(rel);
        let Some(list) = omsi_cfg::vfs::list_dir(&dir) else { continue };
        let mut names: Vec<PathBuf> = list.into_iter().filter(|(_, is_dir)| *is_dir).map(|(n, _)| dir.join(n)).collect();
        names.sort();
        for p in names {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            // (by an index: thousands of folders compared each with all before took long)
            match at.get(&name.to_ascii_lowercase()) {
                Some(&i) => out[i].1.push(p),
                None => {
                    at.insert(name.to_ascii_lowercase(), out.len());
                    out.push((name, vec![p]));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    out
}

/// Is `p` in the content folder (a mod) rather than the OMSI folder?
/// (An archive used in place lies in the content folder's `Archives` too.)
fn in_content(p: &Path) -> bool {
    content_dir().map(|c| p.starts_with(c)).unwrap_or(false)
}

// ---------------------------------------------------------------------------------------
// mods: sorting a mod's folders into the content folder, the way it would be copied into
// an OMSI 2 installation by hand

#[derive(Serialize, Clone, Debug)]
pub struct ModsStatus {
    pub content_dir: String,
    /// (folder, number of entries)
    pub folders: Vec<(String, usize)>,
    pub inbox: String,
    /// What lies in the inbox now.
    pub inbox_items: Vec<String>,
    /// Packs waiting in Mods/waiting for the bus they belong to.
    pub waiting: Vec<String>,
    /// Archives used in place (in `<content>/Archives`): (name, size in bytes).
    pub archives: Vec<(String, u64)>,
    /// Free space on the content folder's disk (bytes).
    pub free_bytes: u64,
    /// What an earlier, interrupted install left and was removed now.
    pub cleaned: Vec<String>,
    pub jobs: Vec<install::Progress>,
    pub hof_warnings: Vec<String>,
    /// Every mod, one by one (see `mods`).
    pub installed: Vec<mods::Mod>,
}

/// Start installing the mod at `src` (a folder, .zip, .7z or .rar) into the content folder in the
/// background; `mode` is "auto" (unpack, or use a .zip in place when it does not fit; .7z
/// and .rar are always unpacked),
/// "extract" or "inplace".
pub fn start_install(src: &Path, mode: &str) -> Result<install::Progress> {
    let content = content_dir().ok_or_else(|| anyhow!("no game binary configured, so no content folder"))?;
    if !src.exists() {
        return Err(anyhow!("{} does not exist", src.display()));
    }
    let from_inbox = src.starts_with(content.join("Mods"));
    let job = install::start(content, root().ok(), src.to_path_buf(), install::InstallMode::parse(mode), from_inbox);
    Ok(job.snapshot())
}

/// How big the mod at `src` is unpacked, what is free, and whether it can be used in place.
pub fn inspect_mod(src: &Path) -> Result<install::SourceInfo> {
    let content = content_dir().ok_or_else(|| anyhow!("no game binary configured, so no content folder"))?;
    install::inspect(&content, root().ok().as_deref(), src)
}

/// Install the mod at `src` and wait for it (the terminal), printing the progress.
pub fn install_mod_blocking(src: &Path, mode: &str, cancel_after_ms: Option<u64>) -> Result<install::Progress> {
    let content = content_dir().ok_or_else(|| anyhow!("no game binary configured, so no content folder"))?;
    Ok(install::run_blocking(content, root().ok(), src.to_path_buf(), install::InstallMode::parse(mode), cancel_after_ms.map(std::time::Duration::from_millis), true))
}

fn inbox_entries(content: &Path) -> Vec<PathBuf> {
    let inbox = content.join("Mods");
    let Ok(rd) = std::fs::read_dir(&inbox) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            !(name.starts_with('.') || name.eq_ignore_ascii_case("installed") || name.eq_ignore_ascii_case(install::WAITING) || name.eq_ignore_ascii_case(install::PLUGINS_HELD) || name.eq_ignore_ascii_case(install::UNINSTALLED) || name.eq_ignore_ascii_case(mods::DISABLED) || name.eq_ignore_ascii_case("README.txt"))
                && (p.is_dir() || p.extension().map(|x| ["zip", "7z", "rar"].iter().any(|ext| x.eq_ignore_ascii_case(ext))).unwrap_or(false))
        })
        .collect();
    v.sort();
    v
}

/// (size, newest time, files) of a file or a folder tree.
fn tree_signature(p: &Path) -> (u64, u64, u64) {
    let Ok(md) = std::fs::symlink_metadata(p) else { return (0, 0, 0) };
    let t = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos() as u64).unwrap_or(0);
    if !md.is_dir() {
        return (md.len(), t, 1);
    }
    let mut acc = (0, t, 0);
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let (s, m, n) = tree_signature(&e.path());
            acc = (acc.0 + s, acc.1.max(m), acc.2 + n);
        }
    }
    acc
}

/// The inbox watcher: something dropped into Mods/ is installed once it has stopped
/// growing (the same size and time on two looks at least two seconds apart); packs in
/// Mods/waiting are installed once their bus is. Returns the sources of the jobs started.
fn watch_inbox(content: &Path) -> Vec<String> {
    struct Seen {
        sig: (u64, u64, u64),
        at: std::time::Instant,
        started: bool,
        /// When the signature was last read (a big folder whose install failed is looked
        /// at again only now and then).
        checked: std::time::Instant,
    }
    static SEEN: std::sync::Mutex<Option<std::collections::HashMap<PathBuf, Seen>>> = std::sync::Mutex::new(None);
    // a mod deleted from Mods/installed is taken out of the lists (#819)
    let gone = install::uninstall_removed(content);
    for g in &gone {
        log_line(&format!("mods: {g} was deleted from Mods/installed - uninstalled, its folders are in Mods/{}/{g}", install::UNINSTALLED));
    }
    if !gone.is_empty() {
        omsi_cfg::content_changed();
    }
    let mut started = Vec::new();
    let mut guard = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let seen = guard.get_or_insert_with(Default::default);
    let waiting_dir = content.join("Mods").join(install::WAITING);
    let items = inbox_entries(content);
    seen.retain(|p, _| items.contains(p) || (p.starts_with(&waiting_dir) && p.exists()));
    for p in items {
        if install::is_busy(&p) {
            continue;
        }
        let now = std::time::Instant::now();
        if seen.get(&p).map(|s| s.started && now.duration_since(s.checked) < std::time::Duration::from_secs(30)).unwrap_or(false) {
            continue;
        }
        let sig = tree_signature(&p);
        if let Some(s) = seen.get_mut(&p) {
            s.checked = now;
        }
        match seen.get_mut(&p) {
            Some(s) if s.sig == sig => {
                if !s.started && now.duration_since(s.at) >= std::time::Duration::from_secs(2) {
                    s.started = true;
                    install::start(content.to_path_buf(), root().ok(), p.clone(), install::InstallMode::Auto, true);
                    started.push(p.to_string_lossy().to_string());
                }
            }
            // new, still growing (a copy in progress), or changed after a failed try
            _ => {
                seen.insert(p.clone(), Seen { sig, at: now, started: false, checked: now });
            }
        }
    }
    let busy = install::jobs().iter().any(|j| j.finished.is_none());
    if !busy {
        for p in install::waiting_ready(content, root().ok().as_deref()) {
            let sig = tree_signature(&p);
            if seen.get(&p).map(|s| s.started && s.sig == sig).unwrap_or(false) {
                continue;
            }
            seen.insert(p.clone(), Seen { sig, at: std::time::Instant::now(), started: true, checked: std::time::Instant::now() });
            install::start(content.to_path_buf(), root().ok(), p.clone(), install::InstallMode::Extract, true);
            started.push(p.to_string_lossy().to_string());
        }
    }
    started
}

/// Install everything in the inbox (and the waiting packs whose bus is there) now, and
/// wait (the terminal's `mods`).
pub fn install_inbox_blocking() -> Vec<install::Progress> {
    let Some(content) = content_dir() else { return Vec::new() };
    let mut out = Vec::new();
    for p in inbox_entries(&content).into_iter().chain(install::waiting_ready(&content, root().ok().as_deref())) {
        let job = install::start(content.clone(), root().ok(), p, install::InstallMode::Auto, true);
        while job.snapshot().finished.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        out.push(job.snapshot());
    }
    out
}

pub fn mods_status() -> Result<ModsStatus> {
    let content = content_dir().ok_or_else(|| anyhow!("no game binary configured, so no content folder"))?;
    let cleaned = install::cleanup_stale(&data_dir(), Some(&content));
    hof_sync::sync_map_hofs();
    let hof_warnings = hof_sync::warnings();
    let folders = omsi_cfg::CONTENT_FOLDERS
        .iter()
        .map(|f| {
            let n = std::fs::read_dir(content.join(f)).map(|rd| rd.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).count()).unwrap_or(0);
            (f.to_string(), n)
        })
        .collect();
    let names = |v: Vec<PathBuf>| v.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect::<Vec<_>>();
    let waiting: Vec<PathBuf> = std::fs::read_dir(content.join("Mods").join(install::WAITING)).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    let mut archives: Vec<(String, u64)> = std::fs::read_dir(content.join(install::ARCHIVES))
        .map(|rd| rd.flatten().filter(|e| e.path().extension().map(|x| x.eq_ignore_ascii_case("zip")).unwrap_or(false)).map(|e| (e.file_name().to_string_lossy().to_string(), e.metadata().map(|m| m.len()).unwrap_or(0))).collect())
        .unwrap_or_default();
    archives.sort();
    Ok(ModsStatus {
        content_dir: content.to_string_lossy().to_string(),
        folders,
        inbox: content.join("Mods").to_string_lossy().to_string(),
        inbox_items: names(inbox_entries(&content)),
        waiting: names(waiting),
        archives,
        free_bytes: install::free_space(&content).unwrap_or(0),
        cleaned,
        jobs: install::jobs(),
        hof_warnings,
        installed: mods::list(&content),
    })
}

/// Switch the mod `id` of the content folder off or on (see `mods::set_enabled`); its name.
pub fn mod_set_enabled(id: &str, on: bool) -> Result<String> {
    let content = content_dir().ok_or_else(|| anyhow!("no game binary configured, so no content folder"))?;
    mods::set_enabled(&content, id, on)
}

/// Delete the mod `id` of the content folder (see `mods::remove`); its name.
pub fn mod_remove(id: &str) -> Result<String> {
    let content = content_dir().ok_or_else(|| anyhow!("no game binary configured, so no content folder"))?;
    mods::remove(&content, id)
}

/// What the page asks every few seconds: whether the content changed (then it asks for the
/// lists again), the install jobs and the running games. It also starts the inbox installs.
#[derive(Serialize, Clone, Debug)]
pub struct Poll {
    pub stamp: String,
    pub jobs: Vec<install::Progress>,
    pub instances: Vec<instances::Instance>,
    /// Inbox items whose install started with this poll.
    pub started: Vec<String>,
}

pub fn poll() -> Result<Poll> {
    let content = content_dir();
    let started = content.as_deref().map(watch_inbox).unwrap_or_default();
    let stamp = index::content_stamp(&bases(), content.as_ref().map(|c| c.join("Mods")).as_deref());
    // (only the games still running: an ended one stays listed a while for its log, not as running)
    let running = instances::list().into_iter().filter(|i| i.running).collect();
    Ok(Poll { stamp, jobs: install::jobs(), instances: running, started })
}

// ---------------------------------------------------------------------------------------
// content lists

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MapInfo {
    pub name: String,
    pub friendly: String,
    pub file: String,
    pub description: String,
    pub entry_points: Vec<EntryInfo>,
    /// The depot file (`.hof` name) the map's own buses use, from ailists.cfg.
    pub hof: String,
    /// Installed as a mod (in the content folder).
    #[serde(default)]
    pub installed: bool,
    /// The years the map's `[years]` (global.cfg) allows, first and last, when it sets any;
    /// a date outside them is moved in ([`fit_date_to_years`]).
    #[serde(default)]
    pub year_span: Option<(i32, i32)>,
}

/// The span of years a map's `[years]` allows: none for `0 0`; no first year when the first
/// is 0, no last when the last is 0 or before the first.
fn year_span_of(g: &omsi_map::GlobalCfg) -> Option<(i32, i32)> {
    match g.years {
        (first, last) if first <= 0 && last <= 0 => None,
        (first, last) => {
            let from = if first > 0 { first } else { i32::MIN };
            let to = if last > 0 && last >= from { last } else { i32::MAX };
            Some((from, to))
        }
    }
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// `date` (YYYY-MM-DD) fitted to a map's year span, as OMSI 2 starts a map on a day it
/// allows so that its AI lists, chrono and timetables fit: the same day in the nearest
/// allowed year (29 February becomes the 28th where that year has none). A date inside the
/// span, or one that is no date, is kept.
pub fn fit_date_to_years(date: &str, span: Option<(i32, i32)>) -> String {
    let mut fields = date.trim().splitn(3, '-').map(|f| f.trim().parse::<i32>());
    let (Some((from, to)), Some(Ok(year)), Some(Ok(month)), Some(Ok(day))) = (span, fields.next(), fields.next(), fields.next()) else {
        return date.to_string();
    };
    if (from..=to).contains(&year) {
        return date.to_string();
    }
    let year = year.clamp(from, to);
    let day = if (month, day) == (2, 29) && !is_leap_year(year) { 28 } else { day };
    format!("{year:04}-{month:02}-{day:02}")
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EntryInfo {
    pub index: i32,
    pub name: String,
}

pub fn list_maps() -> Result<Vec<MapInfo>> {
    root()?;
    hof_sync::sync_map_hofs();
    let lang = content_language();
    let mut out = Vec::new();
    let mut keys = Vec::new();
    for (folder, dirs) in merged_folders("maps") {
        // a map is one folder: the first copy that has a global.cfg
        let Some(d) = dirs.into_iter().find(|d| omsi_cfg::vfs::is_file(&d.join("global.cfg"))) else { continue };
        let key = format!("map|{lang}|{}", d.display());
        let mut stamped = vec![d.clone(), d.join("global.cfg"), d.join("ailists.cfg")];
        stamped.extend(dsc_candidates(&d.join("global.cfg"), lang));
        let stamp = index::files_stamp(&stamped);
        keys.push(key.clone());
        let info: Option<MapInfo> = index::cached(&key, stamp, || read_map(&d, &folder, lang));
        out.extend(info);
    }
    index::save("map|", Some(&keys));
    if out.is_empty() {
        log_empty("maps", "global.cfg");
    }
    Ok(out)
}

/// Say in launcher.log where a list that came out empty was looked for: each folder of
/// `rel` and how many entries it has (a player whose lists stay empty sends the log).
fn log_empty(rel: &str, what: &str) {
    let places: Vec<String> = bases()
        .iter()
        .map(|b| {
            let d = b.join(rel);
            match omsi_cfg::vfs::list_dir(&d) {
                Some(l) => format!("{} ({} entries)", d.display(), l.len()),
                None => format!("{} (cannot be read: {})", d.display(), std::fs::read_dir(&d).err().map(|e| e.to_string()).unwrap_or_else(|| "not a folder".into())),
            }
        })
        .collect();
    log_line(&format!("{rel}: nothing with a {what} found in {}", if places.is_empty() { "no folder (no OMSI 2 folder and no content folder)".to_string() } else { places.join(", ") }));
}

/// The map in `d` and the files its entry is read from beyond the stamped ones (the tiles,
/// when their entry objects are its entry points).
fn read_map(d: &Path, folder: &str, lang: &str) -> (Option<MapInfo>, Vec<PathBuf>) {
    let mut g = match omsi_map::GlobalCfg::load(&d.join("global.cfg")) {
        Ok(g) => g,
        Err(e) => {
            log_line(&format!("maps: {} cannot be read ({e:#}) - not listed", d.join("global.cfg").display()));
            return (None, Vec::new());
        }
    };
    // A map from OMSI 1 lists no entry points in global.cfg: the game takes them from the
    // entry objects of its tiles (`GlobalCfg::add_object_entry_points`). Only then are the
    // tiles read here - the list of maps stays quick.
    let mut deps = Vec::new();
    if g.entry_points.is_empty() {
        g.add_object_entry_points();
        deps = g.tiles.iter().map(|t| omsi_cfg::resolve_path(g.dir(), &t.file)).collect();
    }
    // `global_ENG.dsc` (the language of the settings) names and describes the map
    let dsc = find_dsc(&d.join("global.cfg"), lang);
    let friendly = dsc.as_ref().and_then(|x| x.name.first().cloned()).unwrap_or_else(|| g.friendly_name.trim().to_string());
    let description = dsc.map(|x| x.description).filter(|t| !t.is_empty()).unwrap_or_else(|| g.description.trim().to_string());
    let mut entries: Vec<EntryInfo> = g.entry_points.iter().map(|e| EntryInfo { index: e.index, name: e.name.trim().to_string() }).collect();
    // the game's --entry is the position in the list, not the index field; several
    // entries often share a name (one per stop position), so number them
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let total: std::collections::HashMap<String, usize> = entries.iter().fold(std::collections::HashMap::new(), |mut m, e| {
        *m.entry(e.name.clone()).or_default() += 1;
        m
    });
    for (i, e) in entries.iter_mut().enumerate() {
        e.index = i as i32;
        let n = seen.entry(e.name.clone()).or_default();
        *n += 1;
        if total.get(&e.name).copied().unwrap_or(0) > 1 {
            e.name = format!("{} ({})", e.name, n);
        }
    }
    // (the depot groups' first: a plain car group's name line is no depot)
    let hof = omsi_map::ailists::AiLists::load(&d.join("ailists.cfg")).ok().and_then(|l| l.groups.iter().filter(|g| g.is_depot).chain(l.groups.iter()).find_map(|g| g.hof.clone())).unwrap_or_default();
    (Some(MapInfo { name: if g.name.trim().is_empty() { folder.to_string() } else { g.name.trim().to_string() }, friendly, file: format!("maps/{folder}/global.cfg"), description, entry_points: entries, hof, installed: in_content(d), year_span: year_span_of(&g) }), deps)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VehicleInfo {
    pub name: String,
    pub manufacturer: String,
    pub type_name: String,
    pub file: String,
    pub folder: String,
    pub description: String,
    /// The original third [friendlyname] line, shown for the bus's standard paint.
    #[serde(default)]
    pub default_paint: String,
    pub paints: Vec<String>,
    pub hofs: Vec<String>,
    /// Installed as a mod (the bus file is in the content folder).
    #[serde(default)]
    pub installed: bool,
    /// Vehicle packs this bus borrows parts from that are not installed (the Ahlheim
    /// Citaro's dashboard, steering wheel and ticket machine come from `Urbino_II`): it
    /// drives, but with holes in the cockpit, as it would in OMSI.
    #[serde(default)]
    pub missing_packs: Vec<String>,
    /// The fleet numbers of the bus's `[number]` list with the plate each comes with
    /// (Omsi.exe's number combo in the vehicle dialog; empty: the bus has no list).
    #[serde(default)]
    pub numbers: Vec<(String, String)>,
}

/// Names in older vehicle packs use underscores as spaces.
pub fn display_bus_name(name: &str) -> String {
    name.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The displayed vehicle type, using its file name when [friendlyname] leaves it empty.
pub fn vehicle_type_label(type_name: &str, path: &Path) -> String {
    if type_name.trim().is_empty() {
        display_bus_name(&path.file_stem().unwrap_or_default().to_string_lossy())
    } else {
        display_bus_name(type_name)
    }
}

/// The vehicle packs whose parts a model file names and that are installed nowhere.
fn missing_packs_of(model: &Path) -> Vec<String> {
    let Ok(text) = omsi_cfg::vfs::read(model) else { return Vec::new() };
    let text = omsi_cfg::codepage::decode(&text);
    let dir = model.parent().unwrap_or(Path::new("."));
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if !l.starts_with("..") {
            continue;
        }
        let p = omsi_cfg::resolve_path(dir, l);
        // (the game also finds a part from the model's parent folders - `<vehicle>/model`
        // and the vehicle folder for a cfg in `model/Configuration Files`: mesh_path)
        let found = |d: &Path| omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(d, l));
        if omsi_cfg::vfs::is_file(&p) || dir.ancestors().skip(1).take(2).any(found) {
            continue;
        }
        if let Some(pack) = omsi_cfg::missing_vehicle_pack(&p) {
            if !out.contains(&pack) {
                out.push(pack);
            }
        }
    }
    out
}

/// `[item]` names of every `.cti` in the model's `[CTC]` folders (in every content root):
/// the paint schemes, and the folders they were read from.
fn paint_schemes(vehicle: &omsi_vehicle::Vehicle) -> (Vec<String>, Vec<PathBuf>) {
    let mut names: Vec<String> = Vec::new();
    let mut dirs_read: Vec<PathBuf> = Vec::new();
    let Some(model_rel) = vehicle.model.as_ref() else { return (names, dirs_read) };
    let model_path = omsi_cfg::resolve_path(vehicle.dir(), model_rel);
    let Ok(model) = omsi_model::Model::load(&model_path) else { return (names, dirs_read) };
    for c in &model.ctc {
        let dir = omsi_cfg::resolve_path(vehicle.dir(), &c.path);
        let mut files: Vec<PathBuf> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut copies = omsi_cfg::mirrored_dirs(&dir);
        if !copies.contains(&dir) {
            copies.push(dir.clone());
        }
        for d in copies {
            dirs_read.push(d.clone());
            let Some(list) = omsi_cfg::vfs::list_dir(&d) else { continue };
            let mut here: Vec<PathBuf> = list.into_iter().map(|(n, _)| d.join(n)).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("cti")).unwrap_or(false)).collect();
            here.sort();
            for f in here {
                if seen.insert(f.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase()) {
                    files.push(f);
                }
            }
        }
        for f in files {
            let Ok(cfg) = omsi_cfg::CfgFile::read(&f) else { continue };
            let mut r = cfg.reader();
            while let Some(k) = r.next_keyword() {
                if k == "item" {
                    let name = r.str().trim().to_string();
                    if !name.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                        names.push(name);
                    }
                }
            }
        }
    }
    dirs_read.sort();
    dirs_read.dedup();
    (names, dirs_read)
}

/// The `[name]`s of the depot files in the top-level `HOFs/` folder of every content root
/// (shared by all vehicles; a higher-priority root's file hides the same file name lower down).
fn shared_depot_names() -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for base in bases() {
        let dir = base.join("HOFs");
        let mut list = omsi_cfg::vfs::list_dir(&dir).unwrap_or_default();
        list.sort();
        for (n, is_dir) in list {
            let file = n.to_string_lossy().to_ascii_lowercase();
            if is_dir || !file.ends_with(".hof") || !seen.insert(file) {
                continue;
            }
            if let Some(name) = omsi_vehicle::Hof::read_name(&dir.join(&n)) {
                names.push(name.trim().to_string());
            }
        }
    }
    names
}

/// One line into ~/.openomsi/launcher.log.
fn log_line(line: &str) {
    use std::io::Write;
    let p = data_dir().join("launcher.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "{now} {line}");
    }
}

pub fn list_vehicles() -> Result<Vec<VehicleInfo>> {
    list_vehicles_progress(|_, _, _| {})
}

/// The depot files (by `[name]`) beside bus `bus` (relative to `root`) whose IBIS trips carry
/// line `line`, the fullest first, read once per bus and line.
pub fn depot_names_with_line(root: &Path, bus: &str, line: &str) -> Vec<String> {
    type Cache = std::collections::HashMap<(PathBuf, String, String), Vec<String>>;
    static CACHE: std::sync::LazyLock<std::sync::Mutex<Cache>> = std::sync::LazyLock::new(Default::default);
    let key = (root.to_path_buf(), bus.to_string(), line.to_string());
    if let Some(v) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return v.clone();
    }
    let file = omsi_cfg::resolve_path(root, bus);
    let found: Vec<String> = file
        .parent()
        .map(|dir| {
            let mut hofs: Vec<omsi_vehicle::Hof> = omsi_vehicle::hof::depot_files(dir)
                .into_iter()
                .filter_map(|p| omsi_vehicle::Hof::load(&p).ok())
                .filter(|h| h.has_line(line))
                .collect();
            hofs.sort_by_key(|h| std::cmp::Reverse(h.info_trips.len()));
            hofs.into_iter().map(|h| h.name.trim().to_string()).collect()
        })
        .unwrap_or_default();
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).insert(key, found.clone());
    found
}

/// Whether the depot file called `name`, wherever it is installed (the bus's own folder,
/// another bus's, the shared `HOFs/`), carries line `line`; None when there is no such file.
/// Read once per name and line.
pub fn depot_has_line(name: &str, line: &str) -> Option<bool> {
    type Cache = std::collections::HashMap<(String, String), Option<bool>>;
    static CACHE: std::sync::LazyLock<std::sync::Mutex<Cache>> = std::sync::LazyLock::new(Default::default);
    let key = (name.trim().to_ascii_lowercase(), line.to_string());
    if let Some(v) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return *v;
    }
    let found = omsi_vehicle::hof::depot_anywhere(name).map(|h| h.has_line(line));
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).insert(key, found);
    found
}

/// The buses, as `list_vehicles`, with `progress` told after every few folders what they
/// held, how many folders are done and how many there are: a big installation's first
/// reading (thousands of vehicle folders, nothing in the cache yet) takes minutes, and the
/// page showed nothing at all until the last folder was read.
pub fn list_vehicles_progress(progress: impl Fn(&[VehicleInfo], usize, usize)) -> Result<Vec<VehicleInfo>> {
    use rayon::prelude::*;
    root()?;
    let lang = content_language();
    let folders = merged_folders("Vehicles");
    let shared_hofs = shared_depot_names();
    let keys: Vec<String> = folders.iter().map(|(_, dirs)| format!("bus4|{lang}|{}", dirs.iter().map(|d| d.to_string_lossy()).collect::<Vec<_>>().join("|"))).collect();
    let read = |(folder, dirs): &(String, Vec<PathBuf>), key: &String| -> Vec<VehicleInfo> {
        // the stamp covers every copy of the folder and their direct entries (Model/,
        // Texture/ ...); the paint folders the entry read are its dependencies
        let mut stamped: Vec<PathBuf> = dirs.clone();
        for d in dirs {
            if let Some(list) = omsi_cfg::vfs::list_dir(d) {
                let mut subs: Vec<PathBuf> = list.into_iter().filter(|(_, is_dir)| *is_dir).map(|(n, _)| d.join(n)).collect();
                subs.sort();
                stamped.extend(subs);
            }
        }
        index::cached(key, index::folder_stamp(&stamped), || read_vehicle_folder(folder, dirs, lang))
    };
    // folders side by side (the files of one wait for the disk while another's are parsed),
    // a handful of threads so that a hard disk is not sent seeking all over
    let pool = rayon::ThreadPoolBuilder::new().num_threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8)).build().ok();
    let mut out = Vec::new();
    let total = folders.len();
    let mut done = 0;
    let mut saved = std::time::Instant::now();
    for (chunk, chunk_keys) in folders.chunks(32).zip(keys.chunks(32)) {
        let lists: Vec<Vec<VehicleInfo>> = match &pool {
            Some(pool) => pool.install(|| chunk.par_iter().zip(chunk_keys.par_iter()).map(|(f, k)| read(f, k)).collect()),
            None => chunk.iter().zip(chunk_keys.iter()).map(|(f, k)| read(f, k)).collect(),
        };
        let mut batch: Vec<VehicleInfo> = lists.into_iter().flatten().collect();
        // the shared depot files after each bus's own (as the game offers them,
        // `omsi_vehicle::hof::depot_files`; a bus's own of the same name wins) - after the
        // cache, whose stamps cover only the vehicle folders
        for v in batch.iter_mut() {
            for name in &shared_hofs {
                if !v.hofs.iter().any(|h| h.eq_ignore_ascii_case(name)) {
                    v.hofs.push(name.clone());
                }
            }
        }
        done += chunk.len();
        // what was read is kept every few seconds: a first reading left half-way (the
        // launcher closed) starts from there the next time
        if saved.elapsed().as_secs() >= 10 {
            index::save("bus4|", None);
            saved = std::time::Instant::now();
        }
        progress(&batch, done, total);
        out.extend(batch);
    }
    index::save("bus4|", Some(&keys));
    if out.is_empty() {
        log_empty("Vehicles", ".bus file");
    }
    Ok(out)
}

/// The buses of one vehicle folder (all its copies; a bus file in the content folder hides
/// the one of the same name in the OMSI folder), and the folders read besides its own.
fn read_vehicle_folder(folder: &str, dirs: &[PathBuf], lang: &str) -> (Vec<VehicleInfo>, Vec<PathBuf>) {
    // Every file of the folder and of the folders in it, however deep: OMSI's bus list looks
    // for `*.bus` and `*.ovh` under Vehicles to any depth (Omsi.exe 0x67bdf9, the search
    // with depth 255) - `Vehicles\Pack\Variant\x.bus` was not listed here (#137). A file of
    // the content folder hides the one at the same place in the installation.
    fn walk(d: &Path, rel: &str, depth: u32, out: &mut Vec<(String, PathBuf)>) {
        let mut list = omsi_cfg::vfs::list_dir(d).unwrap_or_default();
        list.sort();
        for (n, is_dir) in list {
            let name = n.to_string_lossy().to_string();
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if is_dir {
                if depth > 0 && !name.starts_with('.') {
                    walk(&d.join(&n), &r, depth - 1, out);
                }
            } else {
                out.push((r, d.join(&n)));
            }
        }
    }
    let mut files: Vec<PathBuf> = Vec::new();
    let mut rel_of: std::collections::HashMap<PathBuf, String> = std::collections::HashMap::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for d in dirs {
        let mut here = Vec::new();
        walk(d, "", 8, &mut here);
        for (r, f) in here {
            if seen.insert(r.to_ascii_lowercase()) {
                rel_of.insert(f.clone(), r);
                files.push(f);
            }
        }
    }
    let mut out = Vec::new();
    let mut deps = Vec::new();
    if !files.iter().any(|f| f.extension().map(|e| e.eq_ignore_ascii_case("bus") || e.eq_ignore_ascii_case("ovh")).unwrap_or(false)) {
        // textures, or a repaint for a bus that is not installed: nothing to drive
        log_line(&format!("vehicles: Vehicles/{folder} has no .bus file (a repaint or textures for a bus that is not installed?) - not listed"));
        return (out, deps);
    }
    // the depot files beside each bus (a bus in a folder of the pack: those of its folder,
    // else those of the pack's own)
    let hof_dir = |f: &PathBuf| rel_of.get(f).map(|r| r.rsplit_once('/').map(|(d, _)| d.to_ascii_lowercase()).unwrap_or_default()).unwrap_or_default();
    let mut hofs_in: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for f in files.iter().filter(|f| f.extension().map(|e| e.eq_ignore_ascii_case("hof")).unwrap_or(false)) {
        // (the name alone: UK depot files carry megabytes of trips)
        if let Some(name) = omsi_vehicle::Hof::read_name(f) {
            hofs_in.entry(hof_dir(f)).or_default().push(name.trim().to_string());
        }
    }
    // OMSI offers what has a [friendlyname]: never the rear section of an articulated bus
    // (its front brings it along), an AI-only variant or a car
    for (f, v) in omsi_vehicle::vehicle::offered_vehicles(&files) {
        let f = &f;
        let stem = f.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();
        // a vehicle file whose model is not there would load as nothing
        let model = v.model.as_ref().map(|m| omsi_cfg::resolve_path(v.dir(), m));
        if !model.as_ref().map(|m| omsi_cfg::vfs::is_file(m)).unwrap_or(false) {
            log_line(&format!("vehicles: {} - its model {} is missing, not listed", f.display(), model.map(|m| m.display().to_string()).unwrap_or_else(|| "(none)".into())));
            continue;
        }
        let rel = format!("Vehicles/{}/{}", folder, rel_of.get(f).cloned().unwrap_or_else(|| f.file_name().unwrap().to_string_lossy().to_string()));
        let hofs = hofs_in.get(&hof_dir(f)).or_else(|| hofs_in.get("")).cloned().unwrap_or_default();
        let name = format!("{} {}", v.manufacturer.trim(), v.type_name.trim()).trim().to_string();
        let (paints, paint_dirs) = paint_schemes(&v);
        deps.extend(paint_dirs);
        // `<bus>_ENG.dsc` beside the bus file (the language of the settings) describes it
        let description = find_dsc(f, lang).map(|x| x.description).filter(|t| !t.is_empty()).unwrap_or_else(|| v.description.trim().to_string());
        let missing_packs = model.as_deref().map(missing_packs_of).unwrap_or_default();
        if !missing_packs.is_empty() {
            log_line(&format!("vehicles: {} borrows parts from packs that are not installed: {}", f.display(), missing_packs.join(", ")));
        }
        out.push(VehicleInfo { name: if name.is_empty() { stem.clone() } else { name }, manufacturer: v.manufacturer.trim().to_string(), type_name: v.type_name.trim().to_string(), file: rel, folder: folder.to_string(), description, default_paint: v.default_paint.trim().to_string(), paints, hofs, installed: in_content(f), missing_packs, numbers: v.numbers_with_plates().to_vec() });
    }
    deps.sort();
    deps.dedup();
    // the folders themselves are stamped by the caller
    deps.retain(|d| !dirs.contains(d));
    (out, deps)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WeatherInfo {
    pub name: String,
    pub file: String,
    pub description: String,
    pub fog_m: f32,
    pub temp: f32,
    pub clouds: String,
    pub precip: String,
    pub snow: bool,
    #[serde(default)]
    pub installed: bool,
}

pub use omsi_content::weather_choice::metar_airports;
mod airports;
pub use airports::nearest_airport;
mod custom_weather;
pub use custom_weather::{custom_weather, CustomWeather};

pub fn list_weather() -> Result<Vec<WeatherInfo>> {
    root()?;
    let mut out = Vec::new();
    let mut keys = Vec::new();
    let lang = content_language();
    let mut files: Vec<PathBuf> = merged_entries("Weather").into_iter().filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("owt")).unwrap_or(false)).collect();
    files.sort();
    for f in files {
        // the description in the settings' language lives in `<name>_ENG.dsc`
        let mut stamped = vec![f.clone()];
        stamped.extend(dsc_candidates(&f, lang));
        let key = format!("wx2|{lang}|{}", f.display());
        keys.push(key.clone());
        let info: Option<WeatherInfo> = index::cached(&key, index::files_stamp(&stamped), || (read_weather(&f, lang), Vec::new()));
        out.extend(info);
    }
    index::save("wx2|", Some(&keys));
    Ok(out)
}

/// A weather's name as the player reads it: the `[name]` of its description in the
/// interface's language - the English one where OMSI has none in that language (the
/// interface's tables translate it further) - else its file's name. OMSI's own angle brackets
/// ("<Best Weather>", which sort it first in its list) are left out.
pub fn weather_name(f: &Path) -> String {
    weather_name_in(f, content_language())
}

/// OMSI 2's own weathers by their English names (its `_ENG.dsc`), which the interface's
/// tables translate for the languages OMSI has no descriptions in; listed here so the
/// translation check knows them as texts the program shows.
pub const STOCK_WEATHER_NAMES: [&str; 9] = ["Best Weather", "Ground Fog", "Twilight Fog", "Iciness", "Rainy", "Summerlike", "Heavy Snowfall", "Storm", "Freezing Wetness"];

fn weather_name_in(f: &Path, lang: &str) -> String {
    let stem = f.file_stem().map(|s| s.to_string_lossy().trim_start_matches('#').to_string()).unwrap_or_default();
    find_dsc(f, lang)
        .and_then(|d| d.name.first().map(|n| n.trim().trim_start_matches('<').trim_end_matches('>').trim().to_string()))
        .filter(|n| !n.is_empty())
        .unwrap_or(stem)
}

fn read_weather(f: &Path, lang: &str) -> Option<WeatherInfo> {
    let w = omsi_content::weather::Weather::load(f).ok()?;
    // (the `[name]` of the file is not part of the description: "Ground Fog Heavy ground fog ...")
    let description = find_dsc(f, lang).map(|x| x.description.replace('\n', " ")).filter(|t| !t.is_empty()).unwrap_or_else(|| w.description.replace('\n', " "));
    let precip = match w.precip.first().copied().unwrap_or(0.0) as i32 {
        1 => format!("rain {:.0}%", w.precip.get(1).copied().unwrap_or(0.0) / 255.0 * 100.0),
        2 => format!("snow {:.0}%", w.precip.get(1).copied().unwrap_or(0.0) / 255.0 * 100.0),
        _ => "dry".into(),
    };
    Some(WeatherInfo { name: weather_name_in(f, lang), file: format!("Weather/{}", f.file_name().unwrap().to_string_lossy()), description, fog_m: w.fog.0, temp: w.temp.0, clouds: if w.clouds.0.trim().starts_with("-1") { "clear".into() } else { w.clouds.0.trim().to_string() }, precip, snow: w.snow, installed: in_content(f) })
}

/// The content language of the settings (`language=`, English by default): which
/// `<file>_<LANG>.dsc` names and describes maps, buses and weathers.
fn content_language() -> &'static str {
    let text = std::fs::read_to_string(data_dir().join("settings.cfg")).ok();
    language_code(settings_from_text(text.as_deref())["language"].as_str().unwrap_or("ENG"))
}

/// The description files OMSI reads for `file` (`global.cfg` -> `global_ENG.dsc`), in the
/// order they are tried: the language itself, then English for a language that has none
/// (German is the files' own language, so German falls back to the file itself).
fn dsc_candidates(file: &Path, lang: &str) -> Vec<PathBuf> {
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut langs = vec![lang];
    if lang != "DEU" && lang != "ENG" {
        langs.push("ENG");
    }
    langs.into_iter().map(|l| file.with_file_name(format!("{stem}_{l}.dsc"))).collect()
}

/// A `.dsc` file: the `[name]` / `[friendlyname]` lines and the `[description]` text.
struct Dsc {
    name: Vec<String>,
    description: String,
}

fn parse_dsc(text: &str) -> Dsc {
    let mut name = Vec::new();
    let mut desc: Vec<&str> = Vec::new();
    let mut section = "";
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section = if t.eq_ignore_ascii_case("[name]") || t.eq_ignore_ascii_case("[friendlyname]") {
                "name"
            } else if t.eq_ignore_ascii_case("[description]") {
                "description"
            } else {
                ""
            };
            continue;
        }
        match section {
            "name" if !t.is_empty() => name.push(t.to_string()),
            "name" => section = "",
            "description" => desc.push(line.trim_end()),
            _ => {}
        }
    }
    Dsc { name, description: desc.join("\n").trim().to_string() }
}

fn find_dsc(file: &Path, lang: &str) -> Option<Dsc> {
    dsc_candidates(file, lang).iter().find_map(|p| omsi_cfg::vfs::read(p).ok()).map(|b| parse_dsc(&encoding_latin1(&b)))
}

/// OMSI's text files are Latin-1, except that some description files were saved as
/// UTF-16 with a byte-order mark.
fn encoding_latin1(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&u);
    }
    if b.len() >= 2 && b[0] == 0xFE && b[1] == 0xFF {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&u);
    }
    b.iter().map(|&c| c as char).collect()
}

// ---------------------------------------------------------------------------------------
// timetable: lines, tours, trips and the roadbook

#[derive(Serialize, Clone, Debug)]
pub struct StopInfo {
    pub name: String,
    pub arr: f64,
    pub dep: f64,
}

/// One trip of a tour, as OMSI's timetable dialog lists it: when it leaves, from where to
/// where, on which line, and the trip's (route's) own name.
#[derive(Serialize, Clone, Debug)]
pub struct TripInfo {
    /// The trip file's name ("4 Liman-ZS"): the name the map gives the route.
    pub name: String,
    /// Its place in the tour, 1 = the first (what `--trip` takes, as does its departure).
    pub index: usize,
    /// The line its displays show (a depot run has none).
    pub line: String,
    /// First and last stop.
    pub from: String,
    pub terminus: String,
    pub departure: f64,
    pub arrival: f64,
    pub stops: Vec<StopInfo>,
    pub km: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct TourInfo {
    pub number: String,
    pub ai_group: String,
    pub first: f64,
    pub last: f64,
    /// The days it runs, in words ("Mon-Fri", "Sat", "daily", ...), from its validity mask.
    pub days: String,
    /// It runs on the date asked for (the game's timetable has only these tours that day).
    pub runs: bool,
    /// The first date from the one asked for on which it runs (`YYYY-MM-DD`): the game's
    /// timetable dialog lists only the tours of the chosen day, so a
    /// tour of another day is picked by moving the date to it.
    pub next_run: Option<String>,
    pub trips: Vec<TripInfo>,
}

#[derive(Serialize, Clone, Debug)]
pub struct LineInfo {
    pub name: String,
    pub user_allowed: bool,
    /// The line's ends: the first and the last stop of its longest trip (the trips' own
    /// termini where no trip has two stops).
    pub termini: Vec<String>,
    pub tours: Vec<TourInfo>,
}

/// The date the game starts on without `--date` (its clock's default, day 150 of 1989), and
/// the launcher's own default date.
pub const DEFAULT_DATE: &str = "1989-05-30";

/// A trip as a map picture needs it: the map's own road pieces the trip drives, in order,
/// and its stops.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TripPath {
    /// The road pieces it drives, in order (the shared piece of two links appears once).
    pub route: Vec<RoadPiece>,
    /// The stop objects the trip calls at, in order.
    pub stops: Vec<i64>,
}

/// One spline of the map a trip drives over: its id in its tile, which `[path]` of it the
/// trip uses, and the tile's place in `global.cfg`'s `[map]` list. It is the game's own
/// `LaneKey` (`schedule::steps_of` builds the same three fields), so the launcher's map
/// picture can find the very lane the game would drive the trip on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoadPiece {
    pub tile_x: i32,
    pub tile_y: i32,
    pub spline: i64,
    /// The `[path]` of that spline's `.sli` the trip drives (the game's own lane).
    pub path: u16,
}

/// The route of trip `trip` of the map `map` on `date`: its road pieces and its stops.
/// The trip's own track (`.ttr`) when it names one - trains, ferries, and the type-1 trips
/// of mod maps - else the station links between its stops, as the game walks them.
pub fn trip_path(map: &str, date: &str, trip: &str) -> Result<TripPath> {
    let map_dir = resolve_content(map)?.parent().map(|p| p.to_path_buf()).context("map folder")?;
    let date = if date.trim().is_empty() { DEFAULT_DATE } else { date.trim() };
    let code = omsi_map::date_code(date).with_context(|| format!("'{date}' is not a date (YYYY-MM-DD)"))?;
    let chrono = omsi_map::active_chrono_dirs(&map_dir, code);
    let off = omsi_map::chrono_deactivated_lines(&chrono);
    let data = omsi_timetable::TimetableData::load_with_chrono(&map_dir, &chrono, &off);
    let mut out = TripPath::default();
    let Some(t) = data.trips.iter().find(|x| x.name.eq_ignore_ascii_case(trip.trim())) else { return Ok(out) };
    // a type-1 trip's stations are its own `[station]` records (all of Novi Sad)
    out.stops = if t.stations.is_empty() {
        t.stations_legacy.iter().filter_map(|r| r.first()?.trim().parse::<i64>().ok()).collect()
    } else {
        t.stations.clone()
    };
    // the tile of a path step is the index of its entry in `[map]`, as the game reads it
    let tiles = omsi_map::GlobalCfg::load(&map_dir.join("global.cfg")).map(|g| g.raw_tiles).unwrap_or_default();
    let at = |tile_index: f64| tiles.get(tile_index as usize).copied();
    let track_name = if t.display_name.trim().is_empty() { t.name.trim() } else { t.display_name.trim() };
    if let Some(track) = data.tracks.iter().find(|x| x.path.file_stem().map(|s| s.to_string_lossy().eq_ignore_ascii_case(track_name)).unwrap_or(false)) {
        for e in &track.entries {
            if e.values.len() < 5 {
                continue;
            }
            if let Some(tile) = at(e.values[2]) {
                out.route.push(RoadPiece { tile_x: tile.0, tile_y: tile.1, spline: e.values[0] as i64, path: e.values[1] as u16 });
            }
        }
        return Ok(out);
    }
    for w in out.stops.windows(2) {
        let Some(link) = data.stn_links.iter().find(|l| l.from_id == w[0] && l.to_id == w[1]) else { continue };
        for e in &link.entries {
            if e.values.len() < 4 {
                continue;
            }
            let Some(tile) = at(e.values[2]) else { continue };
            let p = RoadPiece { tile_x: tile.0, tile_y: tile.1, spline: e.values[0] as i64, path: e.values[1] as u16 };
            // consecutive links repeat the piece they share
            if out.route.last() != Some(&p) {
                out.route.push(p);
            }
        }
    }
    Ok(out)
}

/// The lines of a map's timetable on `date` (`YYYY-MM-DD`, the game's default when empty):
/// the chrono folders active that day add their lines and take theirs off, as the game does
/// - Spandau's 1991 timetable change replaces line "5 & 5N" and sixteen others.
pub fn list_lines(map: &str, date: &str) -> Result<Vec<LineInfo>> {
    let map_dir = resolve_content(map)?.parent().map(|p| p.to_path_buf()).context("map folder")?;
    lines_on(&map_dir, date)
}

fn lines_on(map_dir: &Path, date: &str) -> Result<Vec<LineInfo>> {
    let date = if date.trim().is_empty() { DEFAULT_DATE } else { date.trim() };
    let code = omsi_map::date_code(date).with_context(|| format!("'{date}' is not a date (YYYY-MM-DD)"))?;
    let chrono = omsi_map::active_chrono_dirs(map_dir, code);
    let off = omsi_map::chrono_deactivated_lines(&chrono);
    let data = omsi_timetable::TimetableData::load_with_chrono(map_dir, &chrono, &off);
    // which tours run that day: the tour's mask as the game reads it (bits 0-6 Monday to
    // Sunday, 7 a public holiday, 8 school days, 9 school holidays: the original, #1883)
    let calendar = omsi_map::Calendar::load(&map_dir.join("Holidays.txt")).unwrap_or_default();
    let day_bit = if calendar.is_holiday(code) { 1 << 7 } else { 1 << weekday(code) };
    let school_bit = if calendar.in_holiday_range(code) { 1 << 9 } else { 1 << 8 };
    let mut out = Vec::new();
    for l in &data.lines {
        let mut termini: Vec<String> = Vec::new();
        // the line's two ends: the first and last stop of its longest trip (the termini of
        // every short working and depot run, listed one after another, read as all its stops)
        let mut ends: Option<(usize, String, String)> = None;
        let mut tours = Vec::new();
        for t in &l.tours {
            let mask = t.extra.trim().parse::<i32>().unwrap_or(1023);
            let runs_on = |c: i32| {
                let day = if calendar.is_holiday(c) { 1 << 7 } else { 1 << weekday(c) };
                let school = if calendar.in_holiday_range(c) { 1 << 9 } else { 1 << 8 };
                mask & day != 0 && mask & school != 0
            };
            // (and yesterday's night tour: its trips past 24:00 run today, #1576)
            let night = t.trips.iter().any(|tt| tt.departure >= 24.0 * 60.0);
            let runs = (mask & day_bit != 0 && mask & school_bit != 0) || (night && runs_on(day_before(code)));
            let next_run = (0..400).map(|k| add_days(code, k)).find(|c| runs_on(*c)).map(|c| format!("{:04}-{:02}-{:02}", c / 10000, c / 100 % 100, c % 100));
            let mut trips = Vec::new();
            for tt in &t.trips {
                let Some(trip) = data.trips.iter().find(|x| x.name.eq_ignore_ascii_case(&tt.trip)) else { continue };
                let departure = tt.departure as f64 * 60.0;
                let duration = trip.profiles.get(tt.profile.max(0) as usize).or(trip.profiles.first()).map(|p| p.factor as f64 * 60.0).filter(|d| *d > 0.0).unwrap_or(600.0);
                // its stations: [station_typ2] objects, or the [station] records of a type-1
                // trip (all of Novi Sad), which carry their stop's name themselves
                let legacy: Vec<(i64, String)> = trip
                    .stations_legacy
                    .iter()
                    .filter_map(|r| Some((r.first()?.trim().parse::<i64>().ok()?, r.get(2).map(|n| n.trim().to_string()).unwrap_or_default())))
                    .collect();
                let stations: Vec<i64> = if trip.stations.is_empty() { legacy.iter().map(|x| x.0).collect() } else { trip.stations.clone() };
                let mut lens = Vec::new();
                for w in stations.windows(2) {
                    lens.push(data.stn_links.iter().find(|k| k.from_id == w[0] && k.to_id == w[1]).map(|k| k.length.max(1.0)).unwrap_or(500.0));
                }
                let total: f64 = lens.iter().sum::<f64>().max(1.0);
                let mut acc = 0.0;
                let mut stops = Vec::new();
                for (i, id) in stations.iter().enumerate() {
                    let t_at = departure + duration * acc / total;
                    let name = data
                        .bus_stops
                        .iter()
                        .find(|b| b.object_id == *id)
                        .map(|b| b.name.trim().to_string())
                        .or_else(|| legacy.iter().find(|x| x.0 == *id).map(|x| x.1.clone()).filter(|n| !n.is_empty()))
                        .unwrap_or_else(|| format!("stop {id}"));
                    stops.push(StopInfo { name, arr: t_at, dep: if i == 0 { departure } else { t_at } });
                    if i < lens.len() {
                        acc += lens[i];
                    }
                }
                if !trip.terminus.trim().is_empty() && !termini.iter().any(|x| x == trip.terminus.trim()) {
                    termini.push(trip.terminus.trim().to_string());
                }
                let from = stops.first().map(|s| s.name.clone()).unwrap_or_default();
                if stops.len() >= 2 && ends.as_ref().is_none_or(|e| stops.len() > e.0) {
                    ends = Some((stops.len(), from.clone(), stops.last().map(|s| s.name.clone()).unwrap_or_default()));
                }
                let index = trips.len() + 1;
                trips.push(TripInfo { name: trip.name.clone(), index, line: trip.line.trim().to_string(), from, terminus: trip.terminus.trim().to_string(), departure, arrival: departure + duration, stops, km: total / 1000.0 });
            }
            // (a tour with no trip the timetable knows cannot be driven: not offered)
            if trips.is_empty() {
                continue;
            }
            let first = trips.first().map(|t| t.departure).unwrap_or(0.0);
            let last = trips.last().map(|t| t.arrival).unwrap_or(0.0);
            tours.push(TourInfo { number: t.number.clone(), ai_group: t.ai_group.clone(), first, last, days: days_of(mask), runs, next_run, trips });
        }
        tours.sort_by(|a, b| a.first.total_cmp(&b.first));
        if let Some((_, first, last)) = ends {
            termini = if first == last { vec![first] } else { vec![first, last] };
        }
        out.push(LineInfo { name: l.name.clone(), user_allowed: l.user_allowed, termini, tours });
    }
    out.sort_by(|a, b| natural_key(&a.name).cmp(&natural_key(&b.name)));
    Ok(out)
}

mod date_codes;
use date_codes::{add_days, day_before, days_of, weekday};

fn natural_key(s: &str) -> (u64, String) {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    (digits.parse().unwrap_or(u64::MAX), s.to_ascii_lowercase())
}

/// What the driver types into the IBIS for a line, from the bus's depot file: the line
/// code, and for each destination the route code and the terminus code.
#[derive(Serialize, Clone, Debug)]
pub struct IbisInfo {
    pub hof: String,
    pub line_code: String,
    pub routes: Vec<IbisRoute>,
}

#[derive(Serialize, Clone, Debug)]
pub struct IbisRoute {
    pub code: String,
    pub route: String,
    pub name: String,
    pub terminus_code: i32,
    pub terminus: String,
}

pub fn ibis_info(bus: &str, hof_name: &str, line: &str) -> Result<IbisInfo> {
    let bus_path = resolve_content(bus)?;
    let dir = bus_path.parent().context("bus folder")?;
    // the bus's own depot file of that name (every content root's copy of its folder), else
    // the one another bus brings (the game borrows it the same way), else the bus's first
    let hof = omsi_vehicle::hof::depot_in(dir, hof_name)
        .or_else(|| omsi_vehicle::hof::depot_anywhere(hof_name))
        .or_else(|| omsi_vehicle::hof::depot_files(dir).iter().find_map(|f| omsi_vehicle::Hof::load(f).ok()))
        .context("no depot file next to the bus")?;
    let hof = &hof;
    let line_digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    let mut routes = Vec::new();
    for t in &hof.info_trips {
        let matches = t.line.trim().eq_ignore_ascii_case(line.trim()) || (!line_digits.is_empty() && t.code.trim_start_matches('0').starts_with(line_digits.trim_start_matches('0')) && t.code.len() >= line_digits.len());
        if !matches {
            continue;
        }
        let code = omsi_cfg::parse_i32(&t.route);
        let terminus = hof.termini.iter().find(|x| x.code == code).and_then(|x| x.strings.first().cloned()).unwrap_or_default();
        routes.push(IbisRoute { code: t.code.clone(), route: t.route.clone(), name: t.name.clone(), terminus_code: code, terminus });
    }
    Ok(IbisInfo { hof: hof.name.clone(), line_code: line_digits, routes })
}

// ---------------------------------------------------------------------------------------
// profiles: the driver's personnel file plus the sessions the game writes

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Session {
    pub time: u64,
    pub driver: String,
    pub map: String,
    pub bus: String,
    pub line: Option<String>,
    pub tour: Option<String>,
    pub seconds: f64,
    pub metres: f64,
    pub stops: i32,
    pub early: i32,
    pub late: i32,
    pub tickets: i32,
    pub cash: f64,
    pub crashes: i32,
    pub hurt: i32,
    pub jolts: i32,
    pub driving: f64,
    pub comfort: f64,
    pub ticketing: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub file: String,
    pub hours: f64,
    pub km: f64,
    pub xp: i64,
    pub level: i64,
    pub next_level_xp: i64,
    pub stops: i32,
    pub early: i32,
    pub late: i32,
    pub tickets: f64,
    pub cash: f64,
    pub crashes: i32,
    pub hurt: i32,
    pub rating_driving: f64,
    pub rating_comfort: f64,
    pub rating_tickets: f64,
    pub sessions: Vec<Session>,
    pub exists: bool,
}

fn sessions() -> Vec<Session> {
    let dir = data_dir().join("sessions");
    let mut out: Vec<Session> = std::fs::read_dir(&dir).map(|rd| rd.flatten().filter_map(|e| std::fs::read_to_string(e.path()).ok()).filter_map(|t| serde_json::from_str::<Session>(&t).ok()).collect()).unwrap_or_default();
    out.sort_by(|a, b| b.time.cmp(&a.time));
    out
}

/// Experience: a point per hundred metres, five per stop served on time, two per ticket,
/// minus twenty per crash and fifty per pedestrian; the level grows with the square root.
fn xp_of(s: &Session) -> i64 {
    let on_time = (s.stops - s.early - s.late).max(0) as i64;
    let xp = (s.metres / 100.0) as i64 + on_time * 5 + s.tickets as i64 * 2 + (s.seconds / 60.0) as i64 - s.crashes as i64 * 20 - s.hurt as i64 * 50 - s.jolts as i64;
    xp.max(0)
}

fn level_of(xp: i64) -> (i64, i64) {
    let level = ((xp as f64 / 250.0).sqrt().floor() as i64) + 1;
    let next = (level * level) as i64 * 250;
    (level, next)
}

/// Where a driver's personnel file is written: the content folder's `Drivers` (the
/// original installation is only ever read).
fn driver_write_path(root: &Path, name: &str) -> PathBuf {
    content_dir().unwrap_or_else(|| root.to_path_buf()).join("Drivers").join(format!("{name}.odr"))
}

/// Where a driver's personnel file is read: the content folder's copy once the game has
/// written one, else the original installation's (the stock `OMSI-Fan.odr`).
fn driver_read_path(root: &Path, name: &str) -> PathBuf {
    let own = driver_write_path(root, name);
    if own.exists() {
        own
    } else {
        root.join("Drivers").join(format!("{name}.odr"))
    }
}

pub fn list_profiles() -> Result<Vec<String>> {
    let root = root()?;
    let odr_names = |dir: PathBuf| -> Vec<String> { std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("odr")).unwrap_or(false)).filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string())).collect()).unwrap_or_default() };
    let mut names = odr_names(root.join("Drivers"));
    if let Some(c) = content_dir() {
        names.extend(odr_names(c.join("Drivers")));
    }
    for s in sessions() {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&s.driver)) {
            names.push(s.driver.clone());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn get_profile(name: &str) -> Result<Profile> {
    let root = root()?;
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("a profile needs a name"));
    }
    let file = driver_read_path(&root, name);
    let driver = omsi_content::driver::Driver::load(&file).ok();
    let mine: Vec<Session> = sessions().into_iter().filter(|s| s.driver.eq_ignore_ascii_case(name)).collect();
    let xp: i64 = mine.iter().map(xp_of).sum();
    let (level, next) = level_of(xp);
    let hours = mine.iter().map(|s| s.seconds).sum::<f64>() / 3600.0;
    let (km, stops, early, late, tickets, cash, crashes, hurt, rating) = match &driver {
        Some(d) => (d.hektom / 10.0, d.bus_stops[0], d.bus_stops[1], d.bus_stops[2], d.tickets[0], d.tickets[1], d.crashes[0], d.crashes[1], [d.driving_percent(), d.comfort_percent().unwrap_or(100.0), d.ticket_percent().unwrap_or(100.0)]),
        None => {
            let km = mine.iter().map(|s| s.metres).sum::<f64>() / 1000.0;
            let n = mine.len().max(1) as f64;
            (km, mine.iter().map(|s| s.stops).sum(), mine.iter().map(|s| s.early).sum(), mine.iter().map(|s| s.late).sum(), mine.iter().map(|s| s.tickets as f64).sum(), mine.iter().map(|s| s.cash).sum(), mine.iter().map(|s| s.crashes).sum(), mine.iter().map(|s| s.hurt).sum(), [mine.iter().map(|s| s.driving).sum::<f64>() / n, mine.iter().map(|s| s.comfort).sum::<f64>() / n, mine.iter().map(|s| s.ticketing).sum::<f64>() / n])
        }
    };
    Ok(Profile { name: name.to_string(), file: format!("Drivers/{name}.odr"), hours, km, xp, level, next_level_xp: next, stops, early, late, tickets, cash, crashes, hurt, rating_driving: rating[0], rating_comfort: rating[1], rating_tickets: rating[2], sessions: mine.into_iter().take(40).collect(), exists: driver.is_some() })
}

/// Create the personnel file for a new driver (OMSI's own `.odr` format), so that the game
/// can add every run to it.
pub fn create_profile(name: &str, sex: &str) -> Result<Profile> {
    let root = root()?;
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) {
        return Err(anyhow!("the name must be a plain file name"));
    }
    let file = driver_write_path(&root, name);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if !file.exists() && !driver_read_path(&root, name).exists() {
        let d = omsi_content::driver::Driver { path: file.clone(), name: name.to_string(), sex: if sex.trim().is_empty() { "M".into() } else { sex.trim().to_string() }, ..Default::default() };
        d.save(&file)?;
    }
    let mut c = load_config();
    c.profile = name.to_string();
    save_config(&c)?;
    get_profile(name)
}

/// Remove only the history belonging to this driver. Unreadable or malformed records
/// have no trustworthy owner and are kept, as are records belonging to other drivers.
fn delete_profile_sessions(dir: &Path, name: &str) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(session) = std::fs::read_to_string(entry.path())
            .ok()
            .and_then(|text| serde_json::from_str::<Session>(&text).ok())
        else {
            continue;
        };
        if session.driver.eq_ignore_ascii_case(name) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Delete the personnel file and its run history. A driver who exists only in the
/// original installation is left there: the original is never written.
pub fn delete_profile(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) {
        return Err(anyhow!("the name must be a plain file name"));
    }
    let root = root()?;
    let file = driver_write_path(&root, name);
    if file.exists() {
        std::fs::remove_file(&file)?;
    } else if root.join("Drivers").join(format!("{}.odr", name.trim())).exists() {
        return Err(anyhow!("'{}' belongs to the original OMSI 2 installation, which is not changed", name.trim()));
    }
    delete_profile_sessions(&data_dir().join("sessions"), name)
}

#[cfg(test)]
mod profile_cleanup_tests;

// ---------------------------------------------------------------------------------------
// key bindings: the newer of ~/.openomsi/Inputs/keyboard.cfg and the content folder's once
// the page has saved one (the game reads that first), else the original installation's,
// which is never written

pub fn adopt_own_inputs() {
    let game = load_config_raw().game;
    omsi_cfg::adopt_own_inputs(Some(Path::new(game.trim())).filter(|p| !p.as_os_str().is_empty()));
}

fn keyboard_cfg_write_path() -> Result<PathBuf> {
    omsi_cfg::own_input("keyboard.cfg").ok_or_else(|| anyhow!("no home folder to keep the key bindings in"))
}

fn keyboard_cfg_read_path() -> Result<PathBuf> {
    match omsi_cfg::input_in_use("keyboard.cfg") {
        Some(p) => Ok(p),
        None => Ok(omsi_cfg::original_keyboard_cfg(&root()?)),
    }
}

fn binding_to_json(b: &omsi_content::input::KeyBinding) -> Value {
    json!({ "action": b.action, "scan_code": b.scan_code, "modifier": b.modifier })
}

fn binding_from_json(v: &Value) -> Option<omsi_content::input::KeyBinding> {
    Some(omsi_content::input::KeyBinding {
        action: v.get("action")?.as_str()?.to_string(),
        scan_code: v.get("scan_code")?.as_i64()? as i32,
        modifier: v.get("modifier").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
    })
}

pub fn get_keybindings() -> Result<Value> {
    let path = keyboard_cfg_read_path()?;
    let k = omsi_content::input::KeyboardCfg::load(&path)?.with_game_defaults().with_vr_defaults();
    Ok(json!({ "game": k.game.iter().map(binding_to_json).collect::<Vec<_>>(), "vehicles": k.vehicles.iter().map(binding_to_json).collect::<Vec<_>>() }))
}

/// Replace the bindings with the page's list. Written to a temp file and read back through
/// the same loader the game uses before it replaces the real file, so a page bug never
/// leaves the player with a `keyboard.cfg` the game itself cannot parse.
pub fn save_keybindings(v: &Value) -> Result<()> {
    let list = |k: &str| -> Vec<omsi_content::input::KeyBinding> { v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(binding_from_json).collect()).unwrap_or_default() };
    let k = omsi_content::input::KeyboardCfg { game: list("game"), vehicles: list("vehicles") };
    let path = keyboard_cfg_write_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("cfg.tmp");
    k.save(&tmp)?;
    omsi_content::input::KeyboardCfg::load(&tmp)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// settings (the game's ~/.openomsi/settings.cfg)

pub fn get_settings() -> Result<Value> {
    let text = std::fs::read_to_string(data_dir().join("settings.cfg")).ok();
    let mut v = settings_from_text(text.as_deref());
    // no settings of our own yet: start from what the player set in OMSI 2
    if text.is_none() {
        if let Some(o) = root().ok().and_then(|r| omsi_options(&r)) {
            if let (Some(dst), Some(src)) = (v.as_object_mut(), o.settings.as_object()) {
                for (k, x) in src {
                    dst.insert(k.clone(), x.clone());
                }
            }
            v["imported_from_omsi"] = json!(true);
        }
    }
    Ok(v)
}

/// Other spellings the game reads for a key the launcher writes: they are read here too and
/// dropped when the file is written again (the game takes the last line of them, so a kept
/// `lang=` would undo the page's `language=`).
// (`navigator_opacity`: the opacity was the navigator's before it was the whole interface's)
const SETTING_ALIASES: &[(&str, &str)] = &[("af", "anisotropy"), ("ambient_occlusion", "ssao"), ("fractal", "detail_textures"), ("lang", "language"), ("texmemlimit", "texture_memory"), ("navigator_opacity", "ui_opacity"), ("gear_buttons_hold", "momentary_gears")];

/// Keys of before that are dropped when the file is written again: `texture_memory=` (and
/// OMSI's `texmemlimit=`, its alias) was the textures' budget, which `video_memory=` - all
/// the game holds on the card - replaced; kept, a 401 or a 1000 meant for the textures would
/// be taken for something else by whoever reads it next.
const RETIRED_SETTINGS: &[&str] = &["texture_memory"];

/// A window size as the settings keep it: "WxH" in pixels (each 320..16384), else "auto".
pub fn resolution_text(v: &str) -> String {
    let v = v.trim().to_ascii_lowercase().replace(' ', "").replace(['*', '×'], "x");
    match v.split_once('x').map(|(w, h)| (w.trim().parse::<u32>(), h.trim().parse::<u32>())) {
        Some((Ok(w), Ok(h))) if (320..=16384).contains(&w) && (240..=16384).contains(&h) => format!("{w}x{h}"),
        _ => "auto".into(),
    }
}

fn setting_key(k: &str) -> String {
    let k = k.trim().to_ascii_lowercase();
    SETTING_ALIASES.iter().find(|(alias, _)| *alias == k).map(|(_, key)| key.to_string()).unwrap_or(k)
}

// (the language table lives in omsi-cfg, below the input crate that needs it too)
pub use omsi_cfg::lang::{language_code, language_iso, LANGUAGES};

/// `vanilla` (as OMSI 2), `vanilla_plus`, `enhanced` or `enhanced_plus`, from the ways a file
/// may spell them (as the game's `settings::graphics_mode`).
pub fn graphics_mode(v: &str) -> &'static str {
    match v.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
        "enhanced_plus" | "enhanced+" | "enhancedplus" | "enhanced_+" | "2" => "enhanced_plus",
        "enhanced" | "1" => "enhanced",
        "vanilla" | "classic" | "original" | "omsi" | "omsi2" | "omsi_2" => "vanilla",
        _ => "vanilla_plus",
    }
}

/// The Graphics settings' quality presets - Low, Medium, High and Ultra - for the graphics
/// mode chosen (`graphics_mode`), shared by the launcher and the game's options. A preset
/// tunes that mode and keeps it: it never switches Enhanced or ray tracing on or off.
/// Vanilla's leave out what OMSI 2 does not draw; High is the defaults of Vanilla+; the
/// enhanced modes take fewer MSAA samples and a smaller render scale below High, and
/// Enhanced+ smaller shadow maps and mirrors too. The texture memory is left as it is.
pub fn graphics_presets_for(mode: &str) -> [(&'static str, Value); 4] {
    let mode = graphics_mode(mode);
    let classic = mode == "vanilla";
    let traced = mode == "enhanced_plus";
    let enhanced = traced || mode == "enhanced";
    // per level: Low, Medium, High, Ultra
    let (msaa, scale) = match mode {
        "enhanced" => ([1, 1, 2, 4], ["0.67", "0.85", "auto", "auto"]),
        "enhanced_plus" => ([1, 1, 1, 2], ["0.5", "0.67", "0.85", "auto"]),
        _ => ([1, 2, 4, 4], ["0.75", "auto", "auto", "auto"]),
    };
    let shadow = if traced { [1024, 1024, 2048, 2048] } else { [1024, 1024, 2048, 4096] };
    let mirror = if traced { [128, 128, 256, 512] } else { [128, 256, 256, 512] };
    let refresh = if traced { ["eco", "eco", "eco", "full"] } else { ["eco", "eco", "full", "full"] };
    let anisotropy = [2, 4, 8, 8];
    let view = ["600", "900", "auto", "2000"];
    let obj_size = [0.03, 0.02, 0.013, 0.005];
    let obj_dist = ["500", "750", "auto", "1500"];
    let rain = ["low", "medium", "high", "high"];
    std::array::from_fn(|i| {
        let low = i == 0;
        let mut preset = json!({
            "graphics": mode, "msaa": msaa[i], "anisotropy": anisotropy[i], "shadow_size": shadow[i],
            "clouds": !(enhanced && low), "rain_quality": rain[i], "windy_trees": !low,
            "view_distance": view[i], "min_obj_size": obj_size[i], "max_obj_dist": obj_dist[i],
            "mirror_size": mirror[i], "mirror_refresh": refresh[i], "render_scale": scale[i],
        });
        if !classic {
            preset["detail_textures"] = json!(!low);
        }
        if !classic && !traced {
            preset["ssao"] = json!(i >= 2);
            preset["shadows"] = json!(!low);
        }
        if !traced {
            preset["reflections"] = json!(1.0);
        }
        (["Low", "Medium", "High", "Ultra"][i], preset)
    })
}

/// The wet roads' reflections: `off` or `low`, else `high` (as the game reads its `puddles`).
pub fn puddles(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() { "off" | "0" | "none" => "off", "low" => "low", _ => "high" }
}

/// The enhanced clouds' quality: `low`, else `high` (as the game's `settings::cloud_quality`).
fn cloud_quality(x: &str) -> &'static str {
    if x.trim().eq_ignore_ascii_case("low") { "low" } else { "high" }
}

/// The rain's quality: `medium` or `low`, else `high` (as the game's `settings::rain_quality`).
fn rain_quality(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "medium" => "medium",
        "low" => "low",
        _ => "high",
    }
}

/// How often the mirrors are drawn: `off`, `eco` or `full`, also from OMSI's
/// `performance_realreflexions` (none, economy, full).
fn mirror_refresh(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "off" | "none" => "off",
        "eco" | "economy" => "eco",
        "every" | "every_frame" => "every",
        _ => "full",
    }
}

/// How the mirrors are shaded in the Enhanced pictures: `standard` (the plain light) or
/// `enhanced` (as the window's picture).
fn mirror_quality(x: &str) -> &'static str {
    if x.trim().eq_ignore_ascii_case("enhanced") {
        "enhanced"
    } else {
        "standard"
    }
}

pub fn perf_overlay_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "compact" | "1" | "true" | "on" => "compact",
        "detailed" | "2" => "detailed",
        _ => "off",
    }
}

/// The page's view of a `settings.cfg` text (None: no file yet, the game's defaults).
pub fn settings_from_text(text: Option<&str>) -> Value {
    settings_from_text_on(text, cfg!(any(target_os = "android", target_os = "ios")))
}

pub fn settings_from_text_on(text: Option<&str>, phone: bool) -> Value {
    let mut v = json!({ "msaa": 4, "anisotropy": 8, "ssao": true, "shadows": true, "shadow_size": 2048, "navigator": true, "ui_opacity": 0.85, "navigator_corner": "bottom-left", "boarding": "auto", "detail_textures": true, "exact_fare": true, "enhanced": false, "graphics": "vanilla_plus", "fullscreen": false, "vsync": true, "volume": 0.6, "drive_keys": "simple", "render_scale": "auto", "view_distance": "auto", "language": "ENG", "video_memory": 0, "texture_compression": true, "gpu_texture_compression": true, "chat": true, "tooltips": true, "name_tags": true, "show_fps": false, "clouds": true, "cloud_quality": "high", "rain_quality": "high", "pax_density": 1.0, "vol_ai": 1.0, "vol_scenery": 1.0, "ambient": true, "vol_ambient": 0.8, "mirror_size": 256, "doppler": true, "driver": true, "max_fps": 0, "min_obj_size": 0.013, "max_obj_dist": "auto" });
    v["puddles"] = json!("high"); // (apart: the literal above is at the json! macro's recursion limit)
    if phone {
        for (k, d) in [("msaa", json!(2)), ("anisotropy", json!(4)), ("ssao", json!(false)), ("shadow_size", json!(1024)), ("mirror_size", json!(128)), ("max_fps", json!(60)), ("max_obj_dist", json!("900")), ("pax_density", json!(0.7)), ("navigator_corner", json!("top-center"))] {
            v[k] = d;
        }
    }
    v["touch_door_mode"] = json!("standard");
    v["touch_pedal_length"] = json!(1.0);
    v["touch_pedal_deadzone"] = json!(0.15);
    // (the game's menus switch these: saved through this page's writer, unknown they were dropped)
    for k in ["full_tank", "trip_summary"] { v[k] = json!(true); }
    v["triple_screen"] = json!(false);
    v["triple_span"] = json!(true);
    v["triple_hud_center"] = json!(true);
    v["triple_fov_deg"] = json!(0.0);
    v["triple_width_mm"] = json!(600);
    v["triple_distance_mm"] = json!(650);
    v["triple_bezel_mm"] = json!(0);
    v["triple_left_angle_deg"] = json!(45);
    v["triple_right_angle_deg"] = json!(45);
    v["triple_eye_height_mm"] = json!(0);
    v["vr"] = json!(false);
    v["vr_scale"] = json!(0.65);
    v["vr_head_smoothing_ms"] = json!(0);
    v["vr_mirror_rate"] = json!(16);
    v["mirror_refresh"] = json!("full");
    v["mirror_quality"] = json!("standard");
    v["vr_desktop_mirror"] = json!(true);
    v["discord_status"] = json!(true);
    // iPhone, iPad, Mac: the cross-platform launcher (this one, drawn by the game) instead of the
    // system's own (SwiftUI); elsewhere there is only this one
    v["cross_platform_launcher"] = json!(false);
    v["discord_app_id"] = json!("");
    // positional voice through GreenTeaSpeak's openOMSI plugin in multiplayer
    v["voice_chat"] = json!(true);
    // the launcher gives the graphics card up while a game runs (off: it stays drawn)
    v["launcher_rest"] = json!(true);
    // the window's size in pixels, "auto" to fit the screen (#904)
    v["resolution"] = json!("auto");
    // the game's information bar along the top, as the last session left it (#1164)
    v["info_bar"] = json!(false);
    v["perf_overlay"] = json!("off");
    // Human posing; the original OMSI behavior remains the default.
    v["passenger_animation"] = json!("original");
    v["photo_mode"] = json!("simple");
    // the people as drawn: the content's own, or the Rocketbox avatars (this computer only)
    v["character_models"] = json!("original");
    v["units"] = json!("metric");
    // the trees' foliage bends and sways in the weather's wind
    v["windy_trees"] = json!(true);
    v["sim_thread"] = json!(false); // (switched in the game's Display menu, saved through this writer)
    v["mirror_hud_every_frame"] = json!(false);
    // an early timetable bus waits only at the stops the timetable times (off: at every
    // stop, as in OMSI; #1773)
    v["ai_wait_timed_stops_only"] = json!(false);
    v["pax_bad_driving"] = json!(true);
    // OMSI's own options
    for (k, d) in [("maintenance", json!(0)), ("ai_unsched_factor", json!(100)), ("standing_chance", json!(5)), ("ai_max_scheduled", json!(0)), ("ai_max_parked", json!(0)), ("ai_max_humans", json!(200)), ("use_real_time", json!(false)), ("use_real_date", json!(false)), ("use_real_year", json!(false)), ("collision_vehicles", json!(true)), ("collision_objects", json!(true)), ("collision_pedestrians", json!(true)), ("head_movement", json!(true)), ("driverview_smooth", json!(true)), ("hands_in_cab", json!(false)), ("alt_view", json!(true)), ("precision_zoom", json!(false))] {
        v[k] = d;
    }
    // openOMSI's own: what passengers say, OMSI's route arrows, getting up from the seat
    for (k, d) in [("pax_voices", json!("all")), ("nav_arrows", json!(false)), ("nav_ai", json!(true)), ("nav_surface", json!(true)), ("nav_light", json!(false)), ("nav_3d", json!(true)), ("nav_widget", json!(true)), ("nav_size", json!(1.0)), ("get_up", json!(false)), ("time_speed", json!("1")), ("time_sync", json!(false)), ("metar_sync", json!(false)), ("metar_station", json!("")), ("machine_translation", json!(false)), ("shadow_casters", json!("all")), ("shadow_blobs", json!(true)), ("reflections", json!(1.0)), ("mouse_sens", json!(1.0)), ("graphics_api", json!("auto")), ("ctrl_off", json!("")), ("steering_linear", json!(false)), ("old_steering", json!(false)), ("red_steer_spd", json!(false)), ("ff_invert", json!(false)), ("ff_enabled", json!(true)), ("brake_hold", json!(true)), ("auto_clutch", json!(true)), ("momentary_gears", json!(false)), ("auto_shift", json!(false)), ("wheel_range", json!(900.0)), ("wheel_lock", json!(0.0)), ("fov", json!(0.0)), ("camera_collision", json!(true)), ("right_stick_look", json!(true)), ("steer_look", json!(false)), ("pedal_throttle", json!(1.0)), ("pedal_brake", json!(1.0)), ("seat_x", json!(0.0)), ("seat_y", json!(0.0)), ("seat_z", json!(0.0)), ("seat_pitch_deg", json!(0.0)), ("head_tracking", json!(false)), ("head_tracking_yaw_sens", json!(100.0)), ("head_tracking_pitch_sens", json!(100.0)), ("head_tracking_roll_sens", json!(100.0)), ("head_tracking_x_sens", json!(100.0)), ("head_tracking_y_sens", json!(100.0)), ("head_tracking_z_sens", json!(100.0)), ("head_tracking_invert_yaw", json!(false)), ("head_tracking_invert_pitch", json!(false)), ("head_tracking_invert_roll", json!(false)), ("head_tracking_invert_x", json!(false)), ("head_tracking_invert_y", json!(false)), ("head_tracking_invert_z", json!(false)), ("led_glow", json!(6)), ("led_mips", json!(1.3)), ("night_brightness", json!(0.0)), ("ui_scale", json!(1.0)), ("ui_scale_window", json!(true)), ("chat_size", json!(1.0)), ("notes", json!(true)), ("mouse_steering", json!(false)), ("mouse_right_off", json!(false)), ("mouse_smooth", json!(true)), ("mouse_hold", json!(true)), ("mouse_pedals", json!(true)), ("key_pedal_speed", json!(1.0)), ("blinker_cancel", json!(true)), ("ff_road_vib", json!(1.0)), ("ff_engine_vib", json!(1.0)), ("ff_fade", json!(0.28))] {
        v[k] = d;
    }
    v["steer_look_angle"] = json!(30.0);
    v["look_sens"] = json!(1.0);
    v["look_smoothing_ms"] = json!(0.0);
    v["pad_steer_smooth"] = json!(120.0);
    v["pad_steer_linear"] = json!(false);
    v["ctrl_background"] = json!(false);
    v["pad_steer_speed"] = json!(2.0);
    v["pad_deadzone"] = json!(0.08);
    v["pad_type"] = json!("auto");
    v["pad_buttons"] = json!(true);
    v["arrows_switch_cams"] = json!(true);
    v["steer_look_response"] = json!(0.25);
    v["head_idle"] = json!(0.0);
    v["head_idle_pace"] = json!(1.0);
    // updates from the GitHub releases: looked for when the launcher starts (and during a
    // session, said over the navigator), installed after asking (or at once); and whether
    // the game is counted on the website's "playing now"
    for (k, d) in [("update_check", json!(true)), ("update_auto", json!(false)), ("update_notify", json!(true)), ("update_channel", json!("stable")), ("presence", json!(true))] {
        v[k] = d;
    }
    let Some(t) = text else { return v };
    let mut version = 0;
    let mut graphics: Option<&str> = None;
    for line in t.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, val)) = line.split_once('=') else { continue };
        let (k, val) = (setting_key(k), val.trim());
        let b = |x: &str| matches!(x.to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes");
        match k.as_str() {
            "anisotropy" => v[&k] = json!(val.parse::<i64>().unwrap_or(8).clamp(1, 16)),
            "msaa" | "shadow_size" => v[&k] = json!(val.parse::<i64>().unwrap_or(0)),
            "ui_opacity" | "volume" | "vol_ai" | "vol_scenery" | "vol_ambient" | "min_obj_size" | "radio_speaker" | "radio_hiss" | "radio_dropouts" => v[&k] = json!(val.parse::<f64>().unwrap_or(0.0)),
            "pax_density" => v[&k] = json!(val.trim_end_matches('%').parse::<f64>().map(|x| if x > 5.0 { x / 100.0 } else { x }).unwrap_or(1.0)),
            "triple_fov_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| if x < 20.0 { 0.0 } else { x.min(120.0) }).unwrap_or(0.0)),
            "triple_width_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(200.0, 2000.0)).unwrap_or(600.0)),
            "triple_distance_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(200.0, 3000.0)).unwrap_or(650.0)),
            "triple_bezel_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 100.0)).unwrap_or(0.0)),
            "triple_left_angle_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 90.0)).unwrap_or(45.0)),
            "triple_right_angle_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 90.0)).unwrap_or(45.0)),
            "triple_eye_height_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(-500.0, 500.0)).unwrap_or(0.0)),
            "vr_scale" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.5, 1.0)).unwrap_or(0.65)),
            "vr_head_smoothing_ms" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 30.0) as i64).unwrap_or(0)),
            "vr_mirror_rate" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(-1.0, 360.0) as i64).unwrap_or(16)),
            "mirror_size" => v[&k] = json!(val.parse::<i64>().map(|x| if x == 0 { 0 } else { x.clamp(64, 2048) }).unwrap_or(256)),
            "mirror_refresh" => v[&k] = json!(mirror_refresh(val)),
            "mirror_quality" => v[&k] = json!(mirror_quality(val)),
            "cloud_quality" => v[&k] = json!(cloud_quality(val)),
            "rain_quality" => v[&k] = json!(rain_quality(val)),
            "puddles" => v[&k] = json!(puddles(val)),
            "gpu_texture_compression" => v[&k] = json!(!matches!(val.trim().to_ascii_lowercase().as_str(), "0" | "off" | "no" | "false" | "disabled")),
            "max_fps" => v[&k] = json!(val.parse::<f64>().map(|x| x as i64).unwrap_or(0)),
            "max_obj_dist" => v[&k] = if val.eq_ignore_ascii_case("auto") { json!("auto") } else { json!(val.parse::<f64>().map(|m| (m.round() as i64).to_string()).unwrap_or_else(|_| "auto".into())) },
            "ssao" | "shadows" | "shadow_blobs" | "navigator" | "enhanced" | "triple_screen" | "triple_span" | "triple_hud_center" | "vr" | "vr_desktop_mirror" | "fullscreen" | "vsync" | "exact_fare" | "detail_textures" | "texture_compression" | "chat" | "tooltips" | "name_tags" | "show_fps" | "clouds" | "doppler" | "driver" | "use_real_time" | "use_real_date" | "use_real_year" | "collision_vehicles" | "collision_objects" | "collision_pedestrians" | "head_movement" | "driverview_smooth" | "hands_in_cab" | "alt_view" | "precision_zoom" => v[&k] = json!(b(val)),
            "maintenance" | "ai_unsched_factor" | "ai_max_scheduled" | "standing_chance" => v[&k] = json!(val.trim_end_matches('%').parse::<f64>().map(|x| x.max(0.0) as i64).unwrap_or(0)),
            // (-1: no parked cars at all, #864)
            "ai_max_parked" => v[&k] = json!(val.parse::<f64>().map(|x| x.max(-1.0) as i64).unwrap_or(0)),
            "ai_max_humans" => v[&k] = json!(val.parse::<f64>().map(|x| x.max(1.0) as i64).unwrap_or(200)),
            "drive_keys" | "navigator_corner" | "boarding" | "render_scale" | "pax_voices" => v[&k] = json!(val),
            "touch_door_mode" => v[&k] = json!(if val.eq_ignore_ascii_case("direct") { "direct" } else { "standard" }),
            "update_channel" => v[&k] = json!(if val.eq_ignore_ascii_case("beta") { "beta" } else { "stable" }),
            "photo_mode" => v[&k] = json!(if val.eq_ignore_ascii_case("advanced") { "advanced" } else { "simple" }),
            "passenger_animation" => v[&k] = json!(if val.eq_ignore_ascii_case("enhanced") { "enhanced" } else { "original" }),
            "character_models" => v[&k] = json!(if val.eq_ignore_ascii_case("improved") { "improved" } else { "original" }),
            "units" => v[&k] = json!(units_setting(val)),
            "ctrl_off" => v[&k] = json!(val),
            "metar_station" => v[&k] = json!(val.chars().filter(|c| c.is_ascii_alphabetic()).take(4).collect::<String>().to_ascii_uppercase()),
            "discord_app_id" => v[&k] = json!(val),
            "resolution" | "window_size" => v["resolution"] = json!(resolution_text(val)),
            "graphics_api" => v[&k] = json!(match val.to_ascii_lowercase().as_str() { "vulkan" => "vulkan", "dx12" => "dx12", "gl" => "gl", "angle" => "angle", _ => "auto" }),
            "shadow_casters" => v[&k] = json!(if val.eq_ignore_ascii_case("omsi") { "omsi" } else { "all" }),
            "ctrl_deadzone" => v[&k] = json!(val.parse::<f64>().unwrap_or(0.0).clamp(0.0, 0.3)),
            "mouse_sens" => v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.1, 3.0)),
            "key_pedal_speed" => v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.25, 2.0)),
            "touch_pedal_length" => v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.8, 1.6)),
            "touch_pedal_deadzone" => v[&k] = json!(val.parse::<f64>().unwrap_or(0.15).clamp(0.0, 0.4)),
            "mouse_pedal_strength" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(1.0)
                        .clamp(0.25, 4.0)
                )
            }
            "ui_scale" | "nav_size" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(1.0).clamp(if k == "ui_scale" { 0.5 } else { 0.6 }, 2.0)),
            "chat_size" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(1.0).clamp(0.5, 3.0)),
            "wheel_range" => v[&k] = json!(val.parse::<f64>().unwrap_or(900.0).clamp(90.0, 2880.0)),
            "wheel_lock" => v[&k] = json!(val.parse::<f64>().map(|x| if x < 45.0 { 0.0 } else { x.min(2880.0) }).unwrap_or(0.0)),
            "fov" => v[&k] = json!(val.parse::<f64>().map(|x| if x < 20.0 { 0.0 } else { x.min(120.0) }).unwrap_or(0.0)),
            "pad_steer_speed" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.8, 5.0)).unwrap_or(2.0)),
            "pad_deadzone" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 0.4)).unwrap_or(0.08)),
            "pad_type" => v[&k] = json!(match val.trim().to_ascii_lowercase().as_str() { "xbox" => "xbox", "ps4" => "ps4", "ps5" => "ps5", _ => "auto" }),
            "camera_collision" | "right_stick_look" | "ctrl_background" | "pad_steer_linear" | "pad_buttons" | "arrows_switch_cams" | "steer_look" | "head_tracking" | "head_tracking_invert_yaw" | "head_tracking_invert_pitch" | "head_tracking_invert_roll" | "head_tracking_invert_x" | "head_tracking_invert_y" | "head_tracking_invert_z" | "discord_status" | "voice_chat" | "launcher_rest" | "cross_platform_launcher" => v[&k] = json!(b(val)),
            // (how much of the mip chain an LED panel is held at, 0..4; a file from before
            // it was a number says 1 or 0)
            "led_mips" => v[&k] = json!(val.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 4.0)).unwrap_or(1.3)),
            "led_glow" => v[&k] = json!(val.parse::<i64>().map(|x| x.clamp(0, 15)).unwrap_or(6)),
            "night_brightness" => v[&k] = json!(val.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 3.0)).unwrap_or(0.0)),
            "reflections" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map_or_else(|| if b(val) { 1.0 } else { 0.0 }, |x| x.clamp(0.0, 1.0))),
            "look_sens" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.1, 2.0)).unwrap_or(1.0)),
            "look_smoothing_ms" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 200.0)).unwrap_or(0.0)),
            "pad_steer_smooth" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 300.0)).unwrap_or(120.0)),
            "steer_look_angle" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 60.0)).unwrap_or(30.0)),
            "steer_look_response" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.05, 1.0)).unwrap_or(0.25)),
            "head_idle" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 1.0)).unwrap_or(0.0)),
            "head_idle_pace" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.5, 2.0)).unwrap_or(1.0)),
            "pedal_throttle" | "pedal_brake" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(0.25, 4.0)).unwrap_or(1.0)),
            "ff_road_vib" | "ff_engine_vib" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(0.0, 4.0)).unwrap_or(1.0)),
            "ff_fade" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(0.0, 1.5)).unwrap_or(0.28)),
            "seat_x" | "seat_y" | "seat_z" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(-1.5, 1.5)).unwrap_or(0.0)),
            "seat_pitch_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(0.0).clamp(-45.0, 45.0)),
            "head_tracking_yaw_sens" | "head_tracking_pitch_sens" | "head_tracking_roll_sens" | "head_tracking_x_sens" | "head_tracking_y_sens" | "head_tracking_z_sens" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 100.0)).unwrap_or(100.0)),
            "full_tank" | "trip_summary" => v[&k] = json!(b(val)),
            "nav_arrows" | "nav_ai" | "nav_surface" | "nav_light" | "nav_3d" | "nav_widget" | "get_up" | "time_sync" | "metar_sync" | "ui_scale_window" | "notes" | "machine_translation" | "update_check" | "update_auto" | "update_notify" | "presence" | "steering_linear" | "old_steering" | "red_steer_spd" | "ff_invert" | "ff_enabled" | "brake_hold" | "auto_clutch" | "momentary_gears" | "auto_shift" | "mouse_steering" | "mouse_right_off" | "mouse_smooth" | "mouse_hold" | "mouse_pedals" | "blinker_cancel" => v[&k] = json!(b(val)),
            "info_bar" => v[&k] = json!(b(val)),
            "perf_overlay" => v[&k] = json!(perf_overlay_mode(val)),
            "sim_thread" => v[&k] = json!(b(val)),
            "windy_trees" | "mirror_hud_every_frame" | "ai_wait_timed_stops_only" | "pax_bad_driving" | "ambient" => v[&k] = json!(b(val)),
            "time_speed" => v[&k] = json!(val.trim_start_matches(['x', 'X']).parse::<f64>().map(|x| x.clamp(1.0, 30.0)).map(|x| if x.fract() == 0.0 { format!("{}", x as i64) } else { x.to_string() }).unwrap_or_else(|_| "1".into())),
            "language" => v[&k] = json!(language_code(val)),
            "graphics" | "renderer" => graphics = Some(graphics_mode(val)),
            // whole metres, as the page's select has them ("1500"); anything else (auto) is
            // the game's default
            "view_distance" => {
                v[&k] = match val.parse::<f64>() {
                    Ok(m) if m > 0.0 => json!((m.round() as i64).to_string()),
                    _ => json!("auto"),
                }
            }
            // MB, 0 = automatic (a fraction written by hand is cut, as the game does)
            "video_memory" => {
                if let Ok(mb) = val.parse::<f64>() {
                    v[&k] = json!(mb.max(0.0) as i64);
                }
            }
            "version" => version = val.parse::<i64>().unwrap_or(0),
            _ => {}
        }
    }
    // a file without `graphics` (older builds) says only `enhanced`; its vanilla renderer is
    // what is now Vanilla+
    let g = graphics.unwrap_or(if v["enhanced"] == json!(true) { "enhanced" } else { "vanilla_plus" });
    v["graphics"] = json!(g);
    v["enhanced"] = json!(g == "enhanced" || g == "enhanced_plus");
    // before version 2 the launcher wrote its old default `boarding=pay` for everybody
    // (passengers then waited at the cash desk for the driver): the game reads that as auto
    if version < 2 && v["boarding"] == "pay" {
        v["boarding"] = json!("auto");
    }
    if version == 3 && v["passenger_animation"] == "enhanced" {
        v["passenger_animation"] = json!("original");
    }
    v
}

/// OMSI's tutorials (`Tutorials/menu_<n>_<LANG>.html`): per lesson its title and what
/// it teaches, in the settings' language.
pub fn tutorials() -> Vec<(usize, String, String)> {
    let Ok(r) = root() else { return Vec::new() };
    let lang = content_language();
    let mut out = Vec::new();
    for n in 1..=4usize {
        let p = [lang, "ENG", "DEU"].iter().map(|l| r.join("Tutorials").join(format!("menu_{n}_{l}.html"))).find(|p| p.is_file());
        let Some(p) = p else { continue };
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let html = omsi_cfg::codepage::decode(&bytes);
        let body = html.split_once("</style>").map(|x| x.1).unwrap_or(&html);
        let mut text = String::new();
        let mut tag = false;
        for c in body.replace("</p>", "\n").replace("<br>", "\n").replace("</h2>", "\n").replace("<li>", "\n• ").chars() {
            match c {
                '<' => tag = true,
                '>' => tag = false,
                _ if !tag => text.push(c),
                _ => {}
            }
        }
        let text = decode_html_entities(&text);
        let mut lines = text.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty());
        let title = lines.next().unwrap_or_default();
        let rest: Vec<String> = lines.collect();
        out.push((n, title, rest.join("\n")));
    }
    out
}

pub use omsi_content::tutorial::decode_html_entities;

/// OMSI's option presets (`option_presets/*.oop`): their names and what they say, in the
/// settings' own keys (maxFPS, performance_minObjSize/maxObjDist, texFilter, texmemlimit,
/// performance_reflTexSize).
pub fn option_presets() -> Vec<(String, Value)> {
    let Ok(r) = root() else { return Vec::new() };
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir(r.join("option_presets")) else { return out };
    let mut files: Vec<PathBuf> = dir.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("oop")).unwrap_or(false)).collect();
    files.sort();
    for f in files {
        let Ok(o) = omsi_content::options::Options::load(&f) else { continue };
        let name = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let mut v = json!({});
        if !cfg!(any(target_os = "android", target_os = "ios")) {
            v["max_fps"] = json!(o.i32("maxfps", 0).max(0));
        }
        v["min_obj_size"] = json!(o.f32("performance_minobjsize", 0.013) as f64);
        v["max_obj_dist"] = json!((o.f32("performance_maxobjdist", 900.0).round() as i64).to_string());
        if let Some(af) = o.values.get("texfilter").and_then(|x| x.get(1)).and_then(|x| x.parse::<i64>().ok()) {
            v["anisotropy"] = json!(af.clamp(1, 16));
        }
        let refl = o.i32("performance_refltexsize", 8);
        v["mirror_size"] = json!(1i64 << refl.clamp(6, 11));
        if let Some(x) = o.str("performance_realreflexions") {
            v["mirror_refresh"] = json!(mirror_refresh(x));
        }
        out.push((name, v));
    }
    out
}

pub fn save_settings(v: &Value) -> Result<()> {
    let p = data_dir().join("settings.cfg");
    let old = std::fs::read_to_string(&p).ok();
    std::fs::write(p, settings_to_text(v, old.as_deref()))?;
    Ok(())
}

/// The settings a graphics profile holds: what the Graphics tab shows, except the machine's
/// own (fullscreen, graphics API).
pub const GRAPHICS_PROFILE_KEYS: [&str; 31] = [
    "graphics", "msaa", "render_scale", "anisotropy", "shadow_size", "ssao", "shadows", "shadow_casters", "detail_textures", "night_brightness", "led_glow", "led_mips", "shadow_blobs", "reflections", "clouds", "cloud_quality", "rain_quality", "puddles", "windy_trees",
    "vsync", "max_fps", "view_distance", "max_obj_dist", "min_obj_size", "mirror_size", "mirror_refresh", "mirror_quality", "mirror_hud_every_frame", "video_memory", "texture_compression", "gpu_texture_compression",
];

fn graphics_profiles_path() -> PathBuf {
    data_dir().join("graphics_profiles.json")
}

/// The saved graphics profiles by name (`~/.openomsi/graphics_profiles.json`).
pub fn graphics_profiles() -> std::collections::BTreeMap<String, Value> {
    std::fs::read_to_string(graphics_profiles_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Keep the graphics of `settings` as profile `name` (an existing one of that name is
/// replaced). Returns the name as kept.
pub fn save_graphics_profile(name: &str, settings: &Value) -> Result<String> {
    let name: String = name.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(40).collect();
    if name.is_empty() {
        return Err(anyhow!("Give the profile a name."));
    }
    let mut profile = serde_json::Map::new();
    for k in GRAPHICS_PROFILE_KEYS {
        if let Some(x) = settings.get(k) {
            profile.insert(k.to_string(), x.clone());
        }
    }
    let mut all = graphics_profiles();
    all.insert(name.clone(), Value::Object(profile));
    std::fs::write(graphics_profiles_path(), serde_json::to_string_pretty(&all)?)?;
    Ok(name)
}

/// Remove profile `name`.
pub fn delete_graphics_profile(name: &str) -> Result<()> {
    let mut all = graphics_profiles();
    all.remove(name);
    std::fs::write(graphics_profiles_path(), serde_json::to_string_pretty(&all)?)?;
    Ok(())
}

/// Put a profile's values into the page's `settings` (only the keys a profile may hold).
pub fn apply_graphics_profile(profile: &Value, settings: &mut Value) {
    for k in GRAPHICS_PROFILE_KEYS {
        if let Some(x) = profile.get(k) {
            settings[k] = x.clone();
        }
    }
}

/// The `settings.cfg` text for the page's values `v`; the lines of the `old` file that the
/// page does not manage (keys of newer games, hand-written switches) are kept.
/// The `units` setting's value: `metric`, `uk` or `us` (the game's `units` module).
fn units_setting(v: &str) -> &'static str {
    match v.trim().to_ascii_lowercase().as_str() {
        "uk" | "british" => "uk",
        "us" | "american" | "imperial" => "us",
        _ => "metric",
    }
}

pub fn settings_to_text(v: &Value, old: Option<&str>) -> String {
    let b = |k: &str, d: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(d) as u8;
    // a number, also as the string a select gives
    let n = |k: &str, d: i64| v.get(k).and_then(|x| x.as_i64().or_else(|| x.as_f64().or_else(|| x.as_str().and_then(|s| s.trim().parse::<f64>().ok())).map(|f| f as i64))).unwrap_or(d);
    let f = |k: &str, d: f64| v.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
    let text = format!(
        "# openOMSI settings (written by the launcher)\nversion=4\nmsaa={}\nanisotropy={}\nssao={}\nshadows={}\nshadow_size={}\nnavigator={}\nui_opacity={}\nnavigator_corner={}\nboarding={}\ndetail_textures={}\nexact_fare={}\nenhanced={}\ngraphics={}\nfullscreen={}\nvsync={}\nvolume={}\ndrive_keys={}\nrender_scale={}\nview_distance={}\nlanguage={}\nvideo_memory={}\ntexture_compression={}\nchat={}\ntooltips={}\nname_tags={}\nshow_fps={}\nclouds={}\npax_density={}\nvol_ai={}\nvol_scenery={}\nmirror_size={}\ndoppler={}\ndriver={}\nmax_fps={}\nmin_obj_size={}\nmax_obj_dist={}\n",
        n("msaa", 4),
        n("anisotropy", 8),
        b("ssao", true),
        b("shadows", true),
        n("shadow_size", 2048),
        b("navigator", true),
        f("ui_opacity", 0.85),
        v.get("navigator_corner").and_then(|x| x.as_str()).unwrap_or("bottom-left"),
        v.get("boarding").and_then(|x| x.as_str()).unwrap_or("auto"),
        b("detail_textures", true),
        b("exact_fare", true),
        matches!(graphics_mode(v.get("graphics").and_then(|x| x.as_str()).unwrap_or("vanilla_plus")), "enhanced" | "enhanced_plus") as u8,
        graphics_mode(v.get("graphics").and_then(|x| x.as_str()).unwrap_or("vanilla_plus")),
        b("fullscreen", false),
        b("vsync", true),
        f("volume", 0.6),
        v.get("drive_keys").and_then(|x| x.as_str()).unwrap_or("simple"),
        // "auto" or a fraction, as a string (the page's select) or a number
        match v.get("render_scale") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(Value::Number(x)) => x.to_string(),
            _ => "auto".to_string(),
        },
        // metres, or "auto" (the game's 1200 m)
        match v.get("view_distance") {
            Some(Value::String(s)) if s.trim().parse::<f64>().map(|m| m > 0.0).unwrap_or(false) => s.trim().to_string(),
            Some(Value::Number(x)) if x.as_f64().map(|m| m > 0.0).unwrap_or(false) => x.to_string(),
            _ => "auto".to_string(),
        },
        language_code(v.get("language").and_then(|x| x.as_str()).unwrap_or("ENG")),
        n("video_memory", 0).max(0),
        b("texture_compression", true),
        b("chat", true),
        b("tooltips", true),
        b("name_tags", true),
        b("show_fps", false),
        b("clouds", true),
        f("pax_density", 1.0),
        f("vol_ai", 1.0),
        f("vol_scenery", 1.0),
        match n("mirror_size", 256) { 0 => 0, x => x.clamp(64, 2048) },
        b("doppler", true),
        b("driver", true),
        n("max_fps", 0).max(0),
        f("min_obj_size", 0.013),
        match v.get("max_obj_dist") {
            Some(Value::String(s)) if s.trim().parse::<f64>().is_ok() => s.trim().to_string(),
            Some(Value::Number(x)) => x.to_string(),
            _ => "auto".to_string(),
        },
    );
    // OMSI's own options (options.cfg): maintenance ([wear_lifespan]), the AI counts and
    // the share of random traffic, the real clock and calendar, collisions, head movement
    let text = format!(
        "{text}maintenance={}\nai_unsched_factor={}\nai_max_scheduled={}\nai_max_parked={}\nai_max_humans={}\nuse_real_time={}\nuse_real_date={}\nuse_real_year={}\ncollision_vehicles={}\ncollision_objects={}\ncollision_pedestrians={}\nhead_movement={}\ndriverview_smooth={}\nhands_in_cab={}\nalt_view={}\nprecision_zoom={}\n",
        n("maintenance", 0).clamp(0, 4),
        n("ai_unsched_factor", 100).clamp(0, 300),
        n("ai_max_scheduled", 0).max(0),
        n("ai_max_parked", 0).max(-1),
        n("ai_max_humans", 200).max(1),
        b("use_real_time", false),
        b("use_real_date", false),
        b("use_real_year", false),
        b("collision_vehicles", true),
        b("collision_objects", true),
        b("collision_pedestrians", true),
        b("head_movement", true),
        b("driverview_smooth", true),
        b("hands_in_cab", false),
        b("alt_view", true),
        b("precision_zoom", false),
    );
    let text = format!(
        "{text}pax_voices={}\nnav_arrows={}\nnav_ai={}\nnav_surface={}\nnav_light={}\nnav_3d={}\nnav_widget={}\nnav_size={}\nget_up={}\ntime_speed={}\nmachine_translation={}\nshadow_casters={}\nshadow_blobs={}\nctrl_deadzone={}\nupdate_check={}\nupdate_auto={}\nupdate_notify={}\npresence={}\nreflections={}\nmouse_sens={}\nmouse_pedal_strength={}\ngraphics_api={}\nctrl_off={}\nsteering_linear={}\nold_steering={}\nred_steer_spd={}\nff_invert={}\nwheel_range={}\nwheel_lock={}\nfov={}\ncamera_collision={}\npedal_throttle={}\npedal_brake={}\nseat_x={}\nseat_y={}\nseat_z={}\nseat_pitch_deg={}\nsteer_look={}\nhead_tracking={}\nhead_tracking_yaw_sens={}\nhead_tracking_pitch_sens={}\nhead_tracking_roll_sens={}\nhead_tracking_x_sens={}\nhead_tracking_y_sens={}\nhead_tracking_z_sens={}\nhead_tracking_invert_yaw={}\nhead_tracking_invert_pitch={}\nhead_tracking_invert_roll={}\nhead_tracking_invert_x={}\nhead_tracking_invert_y={}\nhead_tracking_invert_z={}\nff_enabled={}\nbrake_hold={}\nauto_clutch={}\nmomentary_gears={}\nauto_shift={}\nled_glow={}\nled_mips={}\nnight_brightness={}\nui_scale={}\nui_scale_window={}\nchat_size={}\nnotes={}\nmouse_steering={}\nmouse_right_off={}\nmouse_smooth={}\nmouse_hold={}\nblinker_cancel={}\nff_road_vib={}\nff_engine_vib={}\nff_fade={}\n",
        match v.get("pax_voices").and_then(|x| x.as_str()).unwrap_or("all") {
            "tickets" => "tickets",
            "off" => "off",
            _ => "all",
        },
        b("nav_arrows", false),
        b("nav_ai", true), b("nav_surface", true), b("nav_light", false), b("nav_3d", true), b("nav_widget", true), f("nav_size", 1.0).clamp(0.6, 2.0),
        b("get_up", false),
        match v.get("time_speed") {
            Some(Value::String(x)) => x.trim().parse::<f64>().map(|x| x.clamp(1.0, 30.0)).unwrap_or(1.0),
            Some(Value::Number(x)) => x.as_f64().unwrap_or(1.0).clamp(1.0, 30.0),
            _ => 1.0,
        },
        b("machine_translation", false),
        if v.get("shadow_casters").and_then(|x| x.as_str()) == Some("omsi") { "omsi" } else { "all" },
        b("shadow_blobs", true),
        f("ctrl_deadzone", 0.0).clamp(0.0, 0.3),
        b("update_check", true),
        b("update_auto", false),
        b("update_notify", true),
        b("presence", true),
        v.get("reflections").and_then(|x| x.as_f64().or_else(|| x.as_bool().map(|on| if on { 1.0 } else { 0.0 }))).unwrap_or(1.0).clamp(0.0, 1.0),
        f("mouse_sens", 1.0).clamp(0.1, 3.0),
        f("mouse_pedal_strength", 1.0).clamp(0.25, 4.0),
        match v.get("graphics_api").and_then(|x| x.as_str()).unwrap_or("auto") {
            "vulkan" => "vulkan",
            "dx12" => "dx12",
            "gl" => "gl",
            "angle" => "angle",
            _ => "auto",
        },
        v.get("ctrl_off").and_then(|x| x.as_str()).unwrap_or("").replace(['\n', '\r'], " "),
        b("steering_linear", false),
        b("old_steering", false),
        b("red_steer_spd", false),
        b("ff_invert", false),
        f("wheel_range", 900.0).clamp(90.0, 2880.0),
        f("wheel_lock", 0.0).clamp(0.0, 2880.0),
        f("fov", 0.0).clamp(0.0, 120.0),
        b("camera_collision", true),
        f("pedal_throttle", 1.0).clamp(0.25, 4.0),
        f("pedal_brake", 1.0).clamp(0.25, 4.0),
        f("seat_x", 0.0).clamp(-1.5, 1.5),
        f("seat_y", 0.0).clamp(-1.5, 1.5),
        f("seat_z", 0.0).clamp(-1.5, 1.5),
        f("seat_pitch_deg", 0.0).clamp(-45.0, 45.0),
        b("steer_look", false),
        b("head_tracking", false),
        f("head_tracking_yaw_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_pitch_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_roll_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_x_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_y_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_z_sens", 100.0).clamp(0.0, 100.0),
        b("head_tracking_invert_yaw", false),
        b("head_tracking_invert_pitch", false),
        b("head_tracking_invert_roll", false),
        b("head_tracking_invert_x", false),
        b("head_tracking_invert_y", false),
        b("head_tracking_invert_z", false),
        b("ff_enabled", true),
        b("brake_hold", true),
        b("auto_clutch", true),
        b("momentary_gears", false),
        b("auto_shift", false),
        n("led_glow", 6).clamp(0, 15),
        f("led_mips", 1.3).clamp(0.0, 4.0),
        f("night_brightness", 0.0).clamp(0.0, 3.0),
        f("ui_scale", 1.0).clamp(0.5, 2.0),
        b("ui_scale_window", true),
        f("chat_size", 1.0).clamp(0.5, 3.0),
        b("notes", true),
        b("mouse_steering", false),
        b("mouse_right_off", false),
        b("mouse_smooth", true),
        b("mouse_hold", true),
        b("blinker_cancel", true),
        f("ff_road_vib", 1.0).clamp(0.0, 4.0),
        f("ff_engine_vib", 1.0).clamp(0.0, 4.0),
        f("ff_fade", 0.28).clamp(0.0, 1.5),
    );
    let vr_scale = v.get("vr_scale").and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).filter(|x| x.is_finite()).unwrap_or(0.65).clamp(0.5, 1.0);
    let vr_head_smoothing_ms = v.get("vr_head_smoothing_ms").and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).filter(|x| x.is_finite()).unwrap_or(0.0).clamp(0.0, 30.0);
    let vr_mirror_rate = v.get("vr_mirror_rate").and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).filter(|x| x.is_finite()).unwrap_or(16.0).clamp(-1.0, 360.0);
    let text = format!("{text}vr={}\nvr_scale={vr_scale}\nvr_head_smoothing_ms={vr_head_smoothing_ms}\nvr_mirror_rate={vr_mirror_rate}\nvr_desktop_mirror={}\ndiscord_status={}\nvoice_chat={}\nlauncher_rest={}\ncross_platform_launcher={}\n", b("vr", false), b("vr_desktop_mirror", true), b("discord_status", true), b("voice_chat", true), b("launcher_rest", true), b("cross_platform_launcher", false));
    // what the page does not manage (keys of newer games, hand-written ones) stays as it
    // was in the file; other spellings of the keys just written go
    let mut text = text;
    text.push_str(&format!("right_stick_look={}\n", b("right_stick_look", true)));
    text.push_str(&format!("pad_steer_linear={}\n", b("pad_steer_linear", false)));
    text.push_str(&format!("ctrl_background={}\n", b("ctrl_background", false)));
    text.push_str(&format!("pad_steer_speed={}\npad_deadzone={}\npad_buttons={}\n", f("pad_steer_speed", 2.0).clamp(0.8, 5.0), f("pad_deadzone", 0.08).clamp(0.0, 0.4), b("pad_buttons", true)));
    text.push_str(&format!("pad_type={}\n", match v.get("pad_type").and_then(|x| x.as_str()).unwrap_or("auto") { t @ ("xbox" | "ps4" | "ps5") => t, _ => "auto" }));
    text.push_str(&format!("touch_door_mode={}\n", if v.get("touch_door_mode").and_then(|x| x.as_str()) == Some("direct") { "direct" } else { "standard" }));
    text.push_str(&format!("update_channel={}\n", if v.get("update_channel").and_then(|x| x.as_str()) == Some("beta") { "beta" } else { "stable" }));
    text.push_str(&format!("arrows_switch_cams={}\n", b("arrows_switch_cams", true)));
    text.push_str(&format!("mouse_pedals={}\n", b("mouse_pedals", true)));
    text.push_str(&format!("key_pedal_speed={}\n", f("key_pedal_speed", 1.0).clamp(0.25, 2.0)));
    text.push_str(&format!("touch_pedal_length={}\ntouch_pedal_deadzone={}\n", f("touch_pedal_length", 1.0).clamp(0.8, 1.6), f("touch_pedal_deadzone", 0.15).clamp(0.0, 0.4)));
    text.push_str(&format!("resolution={}\n", resolution_text(v.get("resolution").and_then(|x| x.as_str()).unwrap_or("auto"))));
    text.push_str(&format!("gpu_texture_compression={}\n", b("gpu_texture_compression", true)));
    text.push_str(&format!("cloud_quality={}\n", cloud_quality(v.get("cloud_quality").and_then(|x| x.as_str()).unwrap_or("high"))));
    text.push_str(&format!("rain_quality={}\n", rain_quality(v.get("rain_quality").and_then(|x| x.as_str()).unwrap_or("high"))));
    text.push_str(&format!("puddles={}\n", puddles(v.get("puddles").and_then(|x| x.as_str()).unwrap_or("high"))));
    text.push_str(&format!("mirror_refresh={}\n", mirror_refresh(v.get("mirror_refresh").and_then(|x| x.as_str()).unwrap_or("full"))));
    text.push_str(&format!("mirror_quality={}\n", mirror_quality(v.get("mirror_quality").and_then(|x| x.as_str()).unwrap_or("standard"))));
    text.push_str(&format!("look_sens={}\nlook_smoothing_ms={}\nsteer_look_angle={}\nsteer_look_response={}\nhead_idle={}\nhead_idle_pace={}\ntime_sync={}\nmetar_sync={}\nmetar_station={}\n", f("look_sens", 1.0).clamp(0.1, 2.0), f("look_smoothing_ms", 0.0).clamp(0.0, 200.0), f("steer_look_angle", 30.0).clamp(0.0, 60.0), f("steer_look_response", 0.25).clamp(0.05, 1.0), f("head_idle", 0.0).clamp(0.0, 1.0), f("head_idle_pace", 1.0).clamp(0.5, 2.0), b("time_sync", false), b("metar_sync", false), v.get("metar_station").and_then(|x| x.as_str()).unwrap_or("").chars().filter(|c| c.is_ascii_alphabetic()).take(4).collect::<String>().to_ascii_uppercase()));
    text.push_str(&format!("pad_steer_smooth={}\n", f("pad_steer_smooth", 120.0).clamp(0.0, 300.0)));
    let triple_fov = f("triple_fov_deg", 0.0);
    text.push_str(&format!("triple_hud_center={}\ntriple_fov_deg={}\n", b("triple_hud_center", true), if triple_fov < 20.0 { 0.0 } else { triple_fov.min(120.0) }));
    text.push_str(&format!("triple_screen={}\ntriple_span={}\n", b("triple_screen", false), b("triple_span", true)));
    text.push_str(&format!("triple_width_mm={}\ntriple_distance_mm={}\ntriple_bezel_mm={}\n", f("triple_width_mm", 600.0).clamp(200.0, 2000.0), f("triple_distance_mm", 650.0).clamp(200.0, 3000.0), f("triple_bezel_mm", 0.0).clamp(0.0, 100.0)));
    text.push_str(&format!("info_bar={}\nperf_overlay={}\n", b("info_bar", false), perf_overlay_mode(&v.get("perf_overlay").map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string)).unwrap_or_default())));
    text.push_str(&format!("windy_trees={}\n", b("windy_trees", true)));
    text.push_str(&format!("sim_thread={}\n", b("sim_thread", false)));
    text.push_str(&format!("mirror_hud_every_frame={}\n", b("mirror_hud_every_frame", false)));
    text.push_str(&format!("standing_chance={}\n", n("standing_chance", 5).clamp(0, 100)));
    text.push_str(&format!("ambient={}\nvol_ambient={}\n", b("ambient", true), f("vol_ambient", 0.8).clamp(0.0, 1.0)));
    text.push_str(&format!("radio_speaker={}\nradio_hiss={}\nradio_dropouts={}\n", f("radio_speaker", 0.0).clamp(0.0, 1.0), f("radio_hiss", 0.0).clamp(0.0, 1.0), f("radio_dropouts", 0.0).clamp(0.0, 1.0)));
    text.push_str(&format!("ai_wait_timed_stops_only={}\n", b("ai_wait_timed_stops_only", false)));
    text.push_str(&format!("trip_summary={}\nfull_tank={}\n", b("trip_summary", true), b("full_tank", true)));
    text.push_str(&format!("pax_bad_driving={}\n", b("pax_bad_driving", true)));
    text.push_str(&format!("triple_left_angle_deg={}\ntriple_right_angle_deg={}\ntriple_eye_height_mm={}\n", f("triple_left_angle_deg", 45.0).clamp(0.0, 90.0), f("triple_right_angle_deg", 45.0).clamp(0.0, 90.0), f("triple_eye_height_mm", 0.0).clamp(-500.0, 500.0)));
    text.push_str(&format!("photo_mode={}\n", if v.get("photo_mode").and_then(|x| x.as_str()) == Some("advanced") { "advanced" } else { "simple" }));
    text.push_str(&format!("passenger_animation={}\n", if v.get("passenger_animation").and_then(|x| x.as_str()) == Some("enhanced") { "enhanced" } else { "original" }));
    text.push_str(&format!("character_models={}\n", if v.get("character_models").and_then(|x| x.as_str()) == Some("improved") { "improved" } else { "original" }));
    text.push_str(&format!("units={}\n", units_setting(v.get("units").and_then(|x| x.as_str()).unwrap_or("metric"))));
    let written: Vec<String> = text.lines().filter_map(|l| l.split_once('=')).map(|(k, _)| k.trim().to_ascii_lowercase()).collect();
    for line in old.unwrap_or("").lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        match t.split_once('=') {
            Some((k, _)) if !written.contains(&setting_key(k)) && !RETIRED_SETTINGS.contains(&setting_key(k).as_str()) => {
                text.push_str(t);
                text.push('\n');
            }
            _ => {}
        }
    }
    text
}

// ---------------------------------------------------------------------------------------
// the 3D preview and launching the game

/// Ask the game to write the bus as glTF (cached by bus and paint) and return the path.
pub fn bus_preview(bus: &str, paint: &str) -> Result<String> {
    let c = load_config();
    let game = find_game(&c.game).context("the game binary was not found (set it under Setup)")?;
    let root = root()?;
    let key: String = format!("{bus}|{paint}").chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' }).collect();
    let out = data_dir().join("cache").join(format!("{key}.glb"));
    let bus_path = resolve_content(bus)?;
    // a bus inside an archive used in place changes with the archive
    let bus_path = omsi_cfg::vfs::archive_of(&bus_path).unwrap_or(bus_path);
    let newest_input = [&bus_path, &game].iter().filter_map(|p| p.metadata().and_then(|m| m.modified()).ok()).max();
    let fresh = out.metadata().and_then(|m| m.modified()).ok().zip(newest_input).map(|(o, b)| o >= b).unwrap_or(false);
    if !fresh {
        let mut cmd = std::process::Command::new(&game);
        cmd.arg("--root").arg(&root).arg("--bus").arg(bus).arg("--export-glb").arg(&out);
        if !paint.trim().is_empty() {
            cmd.arg("--paint").arg(paint.trim());
        }
        let status = cmd.env("RUST_LOG", "warn").status().context("running the game for the preview")?;
        if !status.success() || !out.exists() {
            return Err(anyhow!("the game could not export {bus}"));
        }
    }
    Ok(out.to_string_lossy().to_string())
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Duty {
    pub map: String,
    pub bus: String,
    pub paint: Option<String>,
    /// The number plate (registration) the player typed: it wins over the plate the bus's
    /// `[number]` list or the map's `registrations.txt` gives it (empty: as the content says).
    #[serde(default)]
    pub plate: Option<String>,
    /// The fleet number picked from the bus's `[number]` list (none: its first).
    #[serde(default)]
    pub number: Option<String>,
    pub hof: Option<String>,
    pub entry: Option<i32>,
    pub line: Option<String>,
    pub tour: Option<String>,
    /// The trip of the tour to start with: its departure (HH:MM) or its place in the tour.
    #[serde(default)]
    pub trip: Option<String>,
    #[serde(default)]
    pub whole_tour: bool,
    /// HH:MM
    pub time: String,
    /// YYYY-MM-DD
    pub date: Option<String>,
    pub weather: Option<String>,
    pub traffic: Option<u32>,
    pub passengers: Option<bool>,
    pub schedule: Option<bool>,
    pub autostart: Option<bool>,
    /// Start as a pedestrian beside the bus (which is then left out): a bus is placed or
    /// taken over from the game menu later.
    #[serde(default)]
    pub on_foot: Option<bool>,
    pub profile: Option<String>,
    /// LAN play: "host", or "join:<session code | ip[:port] | port | empty = search>".
    pub lan: Option<String>,
    /// The name the other LAN players see (default: the profile).
    pub lan_name: Option<String>,
    /// Discovered web gateway to try if the initial UDP join gets no answer (Auto only).
    pub lan_ws_fallback: Option<String>,
    /// Season override: spring / summer / autumn / winter, with a phase `-early` / `-late`
    /// (`autumn-late`; none = the middle; empty = by date).
    pub season: Option<String>,
    /// One of OMSI's tutorials (1..4): its own situation, nothing else of the duty.
    #[serde(default)]
    pub tutorial: Option<usize>,
    /// A situation file to continue (the map's `laststn.osn`): nothing else of the duty.
    #[serde(default)]
    pub situation: Option<String>,
    /// The depot bus driven: it starts with the wear it was left with (see [`depot`]).
    #[serde(default)]
    pub depot: Option<String>,
}

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
}

/// What can be continued on `map` (#341): the last situation, then the save slots the game
/// writes into `Saves` of the map's folder in the content folder and the quicksave made on
/// that map, the newest first.
pub fn saved_situations(map: &str) -> Vec<SavedSituation> {
    let mut out: Vec<SavedSituation> = last_situation(map).map(|f| SavedSituation { saved: modified_secs(&f), file: f, name: "Last situation".into() }).into_iter().collect();
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
                out.push(SavedSituation { saved: modified_secs(&f), file: f, name: "Quicksave".into() });
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
    if !Path::new(&map.replace('\\', "/")).components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)) || !saved_situations(map).iter().any(|s| s.file == file) {
        return Ok(false);
    }
    std::fs::remove_file(file)?;
    let mut tex = file.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    tex.push(".scripttex");
    let _ = std::fs::remove_dir_all(file.with_file_name(tex));
    Ok(true)
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
            SavedSituation { saved: modified_secs(&f), file: f, name }
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
}

/// The command line a duty becomes.
pub fn duty_args(d: &Duty) -> Result<Vec<String>> {
    let root = root()?;
    duty_args_for_root(d, &root)
}

// The installation is validated by duty_args; argument tests supply their own path.
fn duty_args_for_root(d: &Duty, root: &Path) -> Result<Vec<String>> {
    if let Some(t) = d.tutorial {
        return Ok(vec!["--root".into(), root.to_string_lossy().to_string(), "--no-menu".into(), "--tutorial".into(), t.to_string()]);
    }
    if let Some(sit) = d.situation.as_deref().filter(|s| !s.trim().is_empty()) {
        let mut a = vec!["--root".into(), root.to_string_lossy().to_string(), "--no-menu".into(), "--situation".into(), sit.to_string()];
        if let Some(p) = d.profile.as_deref().filter(|p| !p.trim().is_empty()) {
            a.extend(["--driver".into(), format!("Drivers/{}.odr", p.trim())]);
        }
        // the traffic and the people as for any other start: a situation file keeps the
        // vehicles the player placed, not how busy the streets are (OMSI takes that from its
        // options), and without these a continued session had the timetable buses alone -
        // no cars, nobody at the stops (#136)
        a.extend(["--traffic".into(), d.traffic.unwrap_or(30).to_string()]);
        if d.passengers.unwrap_or(true) {
            a.push("--passengers".into());
        }
        return Ok(a);
    }
    let mut a: Vec<String> = vec!["--root".into(), root.to_string_lossy().to_string(), "--no-menu".into(), "--map".into(), d.map.clone(), "--bus".into(), d.bus.clone(), "--time".into(), if d.time.trim().is_empty() { "09:00".into() } else { d.time.trim().to_string() }];
    if let Some(p) = d.paint.as_deref().filter(|p| !p.trim().is_empty()) {
        a.extend(["--paint".into(), p.trim().to_string()]);
    }
    if let Some(pl) = d.plate.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        a.extend(["--plate".into(), pl.to_string()]);
    }
    if let Some(n) = d.number.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        a.extend(["--number".into(), n.to_string()]);
    }
    if let Some(id) = d.depot.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        a.extend(["--depot".into(), id.to_string()]);
    }
    // (a vehicle file taken for a depot from a broken ailists.cfg by older launchers is none)
    if let Some(h) = d.hof.as_deref().filter(|h| !h.trim().is_empty() && !h.to_ascii_lowercase().contains(".bus") && !h.to_ascii_lowercase().contains(".ovh")) {
        a.extend(["--hof".into(), h.trim().to_string()]);
    }
    // the entry point's place in the map's list; -1: the one nearest to the duty's first
    // stop (a free drive takes the list's first then)
    match d.entry {
        Some(e) if e < 0 => {
            if d.line.as_deref().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                a.push("--auto-entry".into());
            }
        }
        Some(e) => a.extend(["--entry".into(), e.to_string()]),
        None => {}
    }
    // OMSI's [useActTime] / [useActDate] / [useActYear]: the machine's clock and calendar
    // instead of the duty's (the year only on its own switch - a map's timetable is for
    // its years)
    let st = get_settings().unwrap_or_default();
    let on = |k: &str| st.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let now = local_now();
    if let (true, Some((_, _, _, h, m))) = (on("use_real_time"), now) {
        if let Some(i) = a.iter().position(|x| x == "--time") {
            a[i + 1] = format!("{h:02}:{m:02}");
        }
        // (a duty picked for later starts now, not ten minutes before its first trip)
        a.push("--keep-time".into());
    }
    let mut date = d.date.as_deref().map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    if let (true, Some((y, mo, dd, _, _))) = (on("use_real_date"), now) {
        let year = if on("use_real_year") { y } else { date.as_deref().and_then(|x| x.split('-').next()?.parse::<i32>().ok()).unwrap_or(y) };
        date = Some(format!("{year:04}-{mo:02}-{dd:02}"));
    }
    if let Some(dt) = date {
        // (a date outside the map's [years] starts on a day inside them, as OMSI 2 does: the
        // machine's calendar or an old saved duty would load a map without its buses)
        let file = root.join(&d.map);
        let file = if file.is_file() { file } else { resolve_content(&d.map).unwrap_or(file) };
        let span = omsi_map::GlobalCfg::load(&file).ok().and_then(|g| year_span_of(&g));
        a.extend(["--date".into(), fit_date_to_years(&dt, span)]);
    }
    if let Some(w) = d.weather.as_deref().filter(|x| !x.trim().is_empty()) {
        a.extend(["--weather".into(), w.trim().to_string()]);
    }
    a.extend(["--traffic".into(), d.traffic.unwrap_or(30).to_string()]);
    if d.passengers.unwrap_or(true) {
        a.push("--passengers".into());
    }
    // (a duty needs the timetable even with the timetable buses switched off: the game
    // then runs it for the player alone, #1762)
    let schedule = d.schedule.unwrap_or(true) || d.line.is_some();
    if schedule {
        a.push("--schedule".into());
        if d.schedule == Some(false) {
            a.push("--no-timetable-buses".into());
        }
    }
    if let (Some(l), true) = (d.line.as_deref().filter(|x| !x.trim().is_empty()), schedule) {
        a.extend(["--line".into(), l.trim().to_string()]);
        if let Some(t) = d.tour.as_deref().filter(|x| !x.trim().is_empty()) {
            a.extend(["--tour".into(), t.trim().to_string()]);
            if let Some(tr) = d.trip.as_deref().filter(|x| !x.trim().is_empty()) {
                a.extend(["--trip".into(), tr.trim().to_string()]);
                if d.whole_tour {
                    a.push("--whole-tour".into());
                }
            }
        }
    }
    if d.autostart.unwrap_or(false) {
        a.push("--autostart".into());
    }
    if d.on_foot.unwrap_or(false) {
        a.push("--on-foot".into());
    }
    let profile = d.profile.clone().filter(|p| !p.trim().is_empty()).unwrap_or_else(|| load_config().profile);
    if let Some(season) = d.season.as_deref().map(str::trim).filter(|x| !x.is_empty() && !x.eq_ignore_ascii_case("auto")) {
        a.extend(["--season".into(), season.to_ascii_lowercase()]);
    }
    if let Some(lan) = d.lan.as_deref().map(str::trim).filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("off")) {
        if lan.eq_ignore_ascii_case("host") {
            // 0: the default port, or the next free one when a session runs here already
            a.extend(["--lan-host".into(), "0".into()]);
        } else if let Some(target) = lan.strip_prefix("join:") {
            let target = target.trim();
            omsi_net::describe_join(target).map_err(|e| anyhow!("LAN join: {e}"))?;
            a.extend(["--lan-join".into(), if target.is_empty() { "auto".into() } else { target.to_string() }]);
            if let Some(url) = d.lan_ws_fallback.as_deref().filter(|url| !url.is_empty()) { a.extend(["--lan-ws-fallback".into(), url.to_string()]); }
        } else {
            return Err(anyhow!("LAN play: '{lan}' is neither host nor join:<code or address>"));
        }
        let name = d.lan_name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| profile.clone());
        if !name.trim().is_empty() {
            a.extend(["--lan-name".into(), name.trim().to_string()]);
        }
    }
    if !profile.trim().is_empty() {
        a.extend(["--driver".into(), format!("Drivers/{}.odr", profile.trim())]);
    }
    Ok(a)
}

#[derive(Serialize, Clone, Debug)]
pub struct Launched {
    pub pid: u32,
    pub log: String,
    pub command: String,
    /// Games that were running already (and keep running).
    pub others: usize,
}

/// Start a game for the duty. Any number may run at once; each writes its own log.
pub fn launch(d: &Duty) -> Result<Launched> {
    if IN_PROCESS_GAMES {
        let args = duty_args(d)?;
        let command = args.join(" ");
        log_to_file(&format!("game in this process: {command}"));
        *IN_PROCESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(args);
        return Ok(Launched { pid: std::process::id(), log: data_dir().join("game.log").to_string_lossy().to_string(), command, others: 0 });
    }
    let c = load_config();
    let game = find_game(&c.game).context("the game binary was not found (set it under Setup)")?;
    let args = duty_args(d)?;
    let profile = d.profile.clone().filter(|p| !p.trim().is_empty()).unwrap_or(c.profile);
    let s = instances::start(&game, &args, d, &profile)?;
    Ok(Launched { pid: s.pid, log: s.log.to_string_lossy().to_string(), command: s.command, others: s.others })
}

/// What a LAN join field means (or what is wrong with it), and the sessions hosted here.
pub fn check_join(text: &str) -> Value {
    match omsi_net::describe_join(text) {
        Ok(d) => json!({ "ok": true, "text": d, "local": instances::local_hosts() }),
        Err(e) => json!({ "ok": false, "text": e, "local": instances::local_hosts() }),
    }
}

// ---------------------------------------------------------------------------------------
// small services for the window

/// A line into ~/.openomsi/launcher.log.
pub fn log_to_file(line: &str) {
    use std::io::Write;
    let p = data_dir().join("launcher.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "{now} {line}");
    }
}

/// What interrupted installs left behind (and the old unzip folder): cleaned up once when
/// the launcher opens.
pub fn cleanup() {
    for line in install::cleanup_stale(&data_dir(), content_dir().as_deref()) {
        log_to_file(&format!("cleanup: {line}"));
    }
}

/// A phone runs one program: the launcher hands the game's command line over here and the
/// same process plays it in the same window (see the app's `android.rs`) instead of starting
/// another process.
static IN_PROCESS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

/// The command line of a game the launcher asked for (taken once).
pub fn take_in_process_launch() -> Option<Vec<String>> {
    IN_PROCESS.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Whether games run inside the launcher's own process (a phone, an iPhone, an iPad: an app
/// there may not start programs).
pub const IN_PROCESS_GAMES: bool = cfg!(any(target_os = "android", target_os = "ios"));

pub use instances::{list as list_instances, log_tail, stop as stop_instance, Instance};

/// Terminal access to the same functions: `--cli lines '{"map":"maps/Grundorf/global.cfg"}'`.
pub fn cli(cmd: &str, arg: &str) -> Result<Value> {
    let a: Value = serde_json::from_str(arg).unwrap_or(json!({}));
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Ok(match cmd {
        "config" => serde_json::to_value(load_config())?,
        "maps" => serde_json::to_value(list_maps()?)?,
        "vehicles" => serde_json::to_value(list_vehicles()?)?,
        "weather" => serde_json::to_value(list_weather()?)?,
        "lines" => serde_json::to_value(list_lines(&s("map"), &s("date"))?)?,
        "ibis" => serde_json::to_value(ibis_info(&s("bus"), &s("hof"), &s("line"))?)?,
        "profiles" => serde_json::to_value(list_profiles()?)?,
        "profile" => serde_json::to_value(get_profile(&s("name"))?)?,
        "mods" => {
            // the inbox is installed right away here (there is no page to follow it)
            let done = install_inbox_blocking();
            let mut v = serde_json::to_value(mods_status()?)?;
            v["installed_now"] = serde_json::to_value(done)?;
            v
        }
        "install" => {
            let cancel = a.get("cancel_after_ms").and_then(|v| v.as_u64());
            let mode = s("mode");
            let p = install_mod_blocking(Path::new(&s("path")), if mode.is_empty() { "auto" } else { mode.as_str() }, cancel)?;
            if p.state == "failed" {
                return Err(anyhow!("{}", p.message));
            }
            serde_json::to_value(p)?
        }
        "modinfo" => serde_json::to_value(inspect_mod(Path::new(&s("path")))?)?,
        "stamp" => json!(poll()?.stamp),
        "poll" => {
            // {"watch": seconds}: keep polling like the page does (the inbox watcher needs
            // two looks), then wait for the installs it started
            let watch = a.get("watch").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let t0 = std::time::Instant::now();
            let mut stamps = vec![poll()?.stamp];
            let mut started = Vec::new();
            while t0.elapsed().as_secs_f64() < watch || install::jobs().iter().any(|j| j.finished.is_none()) {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                let p = poll()?;
                started.extend(p.started);
                if stamps.last() != Some(&p.stamp) {
                    stamps.push(p.stamp);
                }
            }
            let mut v = serde_json::to_value(poll()?)?;
            v["started"] = json!(started);
            v["stamps"] = json!(stamps);
            v
        }
        "instances" => serde_json::to_value(instances::list())?,
        "stop" => {
            let by_itself = instances::stop(a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32)?;
            json!({ "stopped": true, "ended_by_itself": by_itself })
        }
        "log" => json!(instances::log_tail(a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32, a.get("lines").and_then(|v| v.as_u64()).unwrap_or(40) as usize)?),
        "join" => check_join(&s("text")),
        "settings" => get_settings()?,
        // what the page's Save does: `--cli save_settings '{"view_distance":"1500"}'` (the
        // other values from the file as it is)
        "save_settings" => {
            let mut v = get_settings()?;
            if let (Some(v), Some(changes)) = (v.as_object_mut(), a.as_object()) {
                v.extend(changes.clone());
            }
            save_settings(&v)?;
            get_settings()?
        }
        "keybindings" => get_keybindings()?,
        // `--cli save_keybindings '{"game":[...],"vehicles":[...]}'`: the whole list, as
        // `keybindings` returns it - a partial update reads the current file first
        "save_keybindings" => {
            save_keybindings(&a)?;
            get_keybindings()?
        }
        "preview" => json!(bus_preview(&s("bus"), &s("paint"))?),
        "args" => json!(duty_args(&serde_json::from_value(a.clone())?)?),
        "launch" => serde_json::to_value(launch(&serde_json::from_value(a.clone())?)?)?,
        _ => return Err(anyhow!("unknown command {cmd}")),
    })
}


#[cfg(test)]
mod tests;

/// The machine's local date and time: (year, month, day, hour, minute).
#[cfg(unix)]
pub fn local_now() -> Option<(i32, i32, i32, i32, i32)> {
    // SAFETY: time and localtime_r only write the struct handed to them
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some((tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min))
    }
}

#[cfg(windows)]
pub fn local_now() -> Option<(i32, i32, i32, i32, i32)> {
    // SAFETY: GetLocalTime only fills the struct handed to it
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    Some((t.wYear as i32, t.wMonth as i32, t.wDay as i32, t.wHour as i32, t.wMinute as i32))
}

#[cfg(not(any(unix, windows)))]
pub fn local_now() -> Option<(i32, i32, i32, i32, i32)> {
    None
}

#[cfg(test)]
mod omsi_options_tests;

#[cfg(test)]
mod right_stick_look_tests;
