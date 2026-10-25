//! `openomsi --check <file or folder> [--root <folder>] [--deep]`: any piece of content
//! loaded exactly as the game loads it - a vehicle (.bus/.ovh, with the parts coupled
//! behind it), a scenery object (.sco), a spline (.sli), a human (.hum), a weather (.owt),
//! a mesh (.o3d/.x), a whole map (its global.cfg or folder) or a mod (a folder or .zip with
//! any of these in it) - with every warning and error the loaders say caught instead of
//! scrolled past in a terminal, and the textures, meshes and files they name looked for in
//! the folders the game looks in. `--check-vehicle <file>` is the old name of the same.
//! The findings go to stdout as one JSON line, for the launcher's "Model test" (Troubleshooting).
//! No window, no graphics device: a modder sees what is wrong with their work in seconds.

use rayon::prelude::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Everything logged while the content loads, with the part (file) being loaded on that
/// thread at the time.
struct Capture(Mutex<Vec<(log::Level, String, String, String)>>);

thread_local! {
    /// The part this thread loads now: the log lines it says belong to it.
    static PART: RefCell<String> = const { RefCell::new(String::new()) };
}

fn set_part(part: &str) {
    PART.with(|p| *p.borrow_mut() = part.to_string());
}

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            let part = PART.with(|p| p.borrow().clone());
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push((r.level(), r.target().to_string(), r.args().to_string(), part));
        }
    }
    fn flush(&self) {}
}

static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

/// The findings of one run: said by the checks themselves, or logged by the loaders.
#[derive(Default)]
struct Report {
    findings: Mutex<Vec<Value>>,
}

impl Report {
    fn add(&self, level: &str, what: &str, text: String, hint: Option<String>, part: &str) {
        let mut f = json!({"level": level, "what": what, "text": text, "part": part});
        if let Some(h) = hint {
            f["hint"] = json!(h);
        }
        self.findings.lock().unwrap_or_else(|e| e.into_inner()).push(f);
    }
    fn warn(&self, what: &str, text: String, hint: Option<String>, part: &str) {
        self.add("warn", what, text, hint, part);
    }
    fn error(&self, what: &str, text: String, part: &str) {
        self.add("error", what, text, None, part);
    }
}

/// What the parts add up to.
#[derive(Default)]
struct Counts {
    meshes: usize,
    materials: usize,
    textures: usize,
    missing: usize,
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
}

/// A path as the player knows it: from the content folder it lies in.
fn short(p: &Path, root: &Path) -> String {
    for r in omsi_cfg::content_roots().iter().chain(std::iter::once(&root.to_path_buf())) {
        if let Ok(rel) = p.strip_prefix(r) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    p.display().to_string()
}

/// The textures of one vehicle part's meshes looked for in the folders the game looks in,
/// a finding for each that is not there. (As the scene decides it: a slot whose picture is
/// made at run time - a text texture, a script texture, a [matl_freetex], a texture-change
/// master - and a null texture are not files that should be there.)
fn check_textures(ty: &omsi_sim::VehicleType, root: &Path, part: &str, report: &Report, counts: &mut Counts) {
    let dirs = ty.texture_dirs(root);
    let dir_refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    let mut seen = HashSet::new();
    counts.meshes += ty.meshes.len();
    for vm in &ty.meshes {
        let file = vm.file.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        for (slot, m) in vm.materials.iter().enumerate() {
            counts.materials += 1;
            let t = m.texture.trim();
            if t.is_empty() || crate::is_null_texture(t) || !seen.insert(t.to_ascii_lowercase()) {
                continue;
            }
            let ov: Vec<&omsi_model::MaterialDef> = vm.overrides.iter().filter(|o| omsi_sim::vehicle::override_slot(&vm.materials, o) == Some(slot)).collect();
            let generated = ov.iter().any(|o| o.use_text_texture.is_some() || o.use_script_texture.is_some() || o.freetex.is_some()) || ty.texchange(t).is_some();
            if generated {
                continue;
            }
            if omsi_texture::find_texture(t, &dir_refs).is_none() {
                counts.missing += 1;
                report.warn("texture", format!("the texture {t} was not found (mesh {file})"), Some(format!("looked in {} and it is not there", dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", "))), part);
            }
        }
    }
    counts.textures += seen.len();
}

/// A vehicle and the parts coupled behind it (a trailer, an articulated rear section):
/// loaded by the game with the bus, so checked with it - their meshes and textures are the
/// usual place of a warning the bus alone does not show.
fn check_vehicle(root: &Path, bus: &Path, report: &Report) -> (bool, Value) {
    let mut parts: Vec<String> = Vec::new();
    let mut counts = Counts::default();
    let part = name_of(bus);
    set_part(&part);
    match omsi_sim::VehicleType::load(root, bus) {
        Ok(ty) => {
            check_textures(&ty, root, &part, report, &mut counts);
            parts.push(part);
            let name = format!("{} {}", ty.def.manufacturer.trim(), ty.def.type_name.trim()).trim().to_string();
            let (schemes, vars) = (ty.paint_schemes.len(), ty.program.var_names().len());
            let mut lead = std::sync::Arc::new(ty);
            let mut lead_rev = false;
            let mut seen = vec![lead.def.path.clone()];
            for _ in 0..omsi_vehicle::vehicle::MAX_COUPLED_PARTS {
                let Some((path, reversed)) = omsi_vehicle::vehicle::next_coupled(&lead.def, lead_rev, true) else { break };
                if omsi_vehicle::vehicle::chain_has_part(&seen, &path) {
                    break;
                }
                let part = name_of(&path);
                set_part(&part);
                match omsi_sim::VehicleType::load(root, &path) {
                    Ok(t) => {
                        check_textures(&t, root, &part, report, &mut counts);
                        parts.push(part);
                        seen.push(path);
                        lead = std::sync::Arc::new(t);
                        lead_rev = reversed;
                    }
                    Err(e) => {
                        report.error("load", format!("the vehicle coupled behind {} cannot load: {e:#}", name_of(&lead.def.path)), &part);
                        break;
                    }
                }
            }
            let summary = json!({
                "kind": "vehicle",
                "name": name,
                "parts": parts,
                "meshes": counts.meshes,
                "materials": counts.materials,
                "textures": counts.textures,
                "textures_missing": counts.missing,
                "paint_schemes": schemes,
                "variables": vars,
            });
            (true, summary)
        }
        Err(e) => {
            report.error("load", format!("{e:#}"), &part);
            (false, json!({"kind": "vehicle"}))
        }
    }
}

/// The mesh file a model's `[mesh]` names, where the game finds it (`model\` beside the
/// model.cfg first, then beside it).
fn mesh_path(model_dir: &Path, file: &str) -> PathBuf {
    let p = omsi_cfg::resolve_path(&omsi_cfg::resolve_path(model_dir, "model"), file);
    if omsi_cfg::vfs::is_file(&p) {
        p
    } else {
        omsi_cfg::resolve_path(model_dir, file)
    }
}

/// A model's meshes (every detail level) loaded and their textures looked for in `dirs`.
fn check_model(model: &omsi_model::Model, model_dir: &Path, dirs: &[PathBuf], part: &str, report: &Report, counts: &mut Counts) {
    let dir_refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    // the slots whose picture the game makes: not files that should be there
    let generated: HashSet<String> = model
        .meshes
        .iter()
        .flat_map(|m| m.materials.iter())
        .filter(|o| o.use_text_texture.is_some() || o.use_script_texture.is_some() || o.freetex.is_some())
        .map(|o| o.texture.trim().to_ascii_lowercase())
        .chain(model.texchanges.iter().map(|t| t.trim().to_ascii_lowercase()))
        .chain(model.ctc_textures.iter().map(|(t, _)| t.trim().to_ascii_lowercase()))
        .collect();
    let mut seen = HashSet::new();
    for md in &model.meshes {
        let path = mesh_path(model_dir, &md.file);
        if !omsi_cfg::vfs::is_file(&path) {
            report.add("error", "mesh", format!("the mesh {} was not found", md.file.trim()), Some(format!("looked in {} and it is not there", model_dir.display())), part);
            continue;
        }
        match omsi_o3d::load_mesh(&path) {
            Ok(m) => {
                counts.meshes += 1;
                for mat in &m.materials {
                    counts.materials += 1;
                    let t = mat.texture.trim();
                    if t.is_empty() || crate::is_null_texture(t) || generated.contains(&t.to_ascii_lowercase()) || !seen.insert(t.to_ascii_lowercase()) {
                        continue;
                    }
                    if omsi_texture::find_texture(t, &dir_refs).is_none() {
                        counts.missing += 1;
                        report.warn("texture", format!("the texture {t} was not found (mesh {})", md.file.trim()), Some(format!("looked in {} and it is not there", dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", "))), part);
                    }
                }
            }
            Err(e) => report.error("mesh", format!("the mesh {} cannot be read: {e}", md.file.trim()), part),
        }
    }
    counts.textures += seen.len();
}

/// The script files a scenery object names: there, and compiled as the game compiles them.
fn check_scripts(root: &Path, sco: &omsi_scenery::SceneryObject, part: &str, report: &Report) -> usize {
    let s = &sco.scripts;
    for f in s.varlists.iter().chain(&s.stringvarlists).chain(&s.scripts).chain(&s.constfiles) {
        if !omsi_cfg::vfs::is_file(f) {
            report.error("script", format!("the script {} was not found", short(f, root)), part);
        }
    }
    if s.scripts.is_empty() && s.varlists.is_empty() && s.stringvarlists.is_empty() {
        return 0;
    }
    // (the compiler's own errors come through the log as warnings)
    omsi_sim::scenery::compile_scenery(root, s).var_names().len()
}

/// A scenery object: the .sco, its model, meshes, textures, scripts and sound file.
fn check_object(root: &Path, path: &Path, report: &Report) -> (bool, Value) {
    let part = name_of(path);
    set_part(&part);
    let sco = match omsi_scenery::SceneryObject::load(path) {
        Ok(s) => s,
        Err(e) => {
            report.error("load", format!("{e}"), &part);
            return (false, json!({"kind": "object"}));
        }
    };
    for (k, n) in &sco.unknown_keywords {
        report.add("info", "keyword", format!("the keyword [{k}] is not known (line {n})"), None, &part);
    }
    let sco_dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let (model, model_dir) = match &sco.model_file {
        Some(m) => {
            let mp = omsi_cfg::resolve_path(&sco_dir, m);
            match omsi_model::Model::load(&mp) {
                Ok(model) => (model, mp.parent().unwrap_or(Path::new("")).to_path_buf()),
                Err(e) => {
                    report.error("model", format!("the model {m} cannot be read: {e}"), &part);
                    return (false, json!({"kind": "object", "name": sco.friendly_name}));
                }
            }
        }
        None => (sco.model.clone(), sco_dir.clone()),
    };
    let mut dirs = crate::texture_dirs(root, &model_dir);
    if sco.model_file.is_some() {
        dirs.insert(0, omsi_cfg::resolve_path(&sco_dir, "texture"));
    }
    let mut counts = Counts::default();
    check_model(&model, &model_dir, &dirs, &part, report, &mut counts);
    let vars = check_scripts(root, &sco, &part, report);
    for snd in sco.sound.iter().chain(&sco.sound_ai) {
        let p = omsi_cfg::resolve_path(&sco_dir, snd);
        if !omsi_cfg::vfs::is_file(&p) {
            report.warn("sound", format!("the sound {snd} was not found"), None, &part);
        }
    }
    (
        true,
        json!({
            "kind": "object",
            "name": sco.friendly_name,
            "meshes": counts.meshes,
            "materials": counts.materials,
            "textures": counts.textures,
            "textures_missing": counts.missing,
            "variables": vars,
            "paths": sco.paths.len(),
        }),
    )
}

/// A spline: the .sli and the textures of its profiles.
fn check_spline(root: &Path, path: &Path, report: &Report) -> (bool, Value) {
    let part = name_of(path);
    set_part(&part);
    let def = match omsi_scenery::Spline::load(path) {
        Ok(d) => d,
        Err(e) => {
            report.error("load", format!("{e}"), &part);
            return (false, json!({"kind": "spline"}));
        }
    };
    for (k, n) in &def.unknown_keywords {
        report.add("info", "keyword", format!("the keyword [{k}] is not known (line {n})"), None, &part);
    }
    let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let dirs = crate::texture_dirs(root, &dir);
    let dir_refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    let mut missing = 0;
    for t in &def.textures {
        let name = t.file.trim();
        if name.is_empty() || crate::is_null_texture(name) {
            continue;
        }
        if omsi_texture::find_texture(name, &dir_refs).is_none() {
            missing += 1;
            report.warn("texture", format!("the texture {name} was not found"), Some(format!("looked in {} and it is not there", dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", "))), &part);
        }
    }
    if def.profiles.is_empty() && def.paths.is_empty() && !def.only_editor {
        report.warn("spline", "no [profile] and no [path]: the spline draws nothing and carries nothing".into(), None, &part);
    }
    (
        true,
        json!({"kind": "spline", "textures": def.textures.len(), "textures_missing": missing, "profiles": def.profiles.len(), "paths": def.paths.len(), "length": def.length}),
    )
}

/// A human: the .hum and the model it names.
fn check_human(root: &Path, path: &Path, report: &Report) -> (bool, Value) {
    let part = name_of(path);
    set_part(&part);
    let h = match omsi_content::human::Human::load(path) {
        Ok(h) => h,
        Err(e) => {
            report.error("load", format!("{e}"), &part);
            return (false, json!({"kind": "human"}));
        }
    };
    let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let mut counts = Counts::default();
    if h.model.trim().is_empty() {
        report.error("model", "no [model] given".into(), &part);
    } else {
        let mp = omsi_cfg::resolve_path(&dir, &h.model);
        match omsi_model::Model::load(&mp) {
            Ok(model) => {
                let model_dir = mp.parent().unwrap_or(Path::new("")).to_path_buf();
                let dirs = crate::texture_dirs(root, &model_dir);
                check_model(&model, &model_dir, &dirs, &part, report, &mut counts);
            }
            Err(e) => report.error("model", format!("the model {} cannot be read: {e}", h.model.trim()), &part),
        }
    }
    (true, json!({"kind": "human", "meshes": counts.meshes, "textures": counts.textures, "textures_missing": counts.missing, "height": h.height}))
}

/// A weather file.
fn check_weather(path: &Path, report: &Report) -> (bool, Value) {
    let part = name_of(path);
    set_part(&part);
    match omsi_content::weather::Weather::load(path) {
        Ok(_) => (true, json!({"kind": "weather"})),
        Err(e) => {
            report.error("load", format!("{e}"), &part);
            (false, json!({"kind": "weather"}))
        }
    }
}

/// A single mesh file.
fn check_mesh(path: &Path, report: &Report) -> (bool, Value) {
    let part = name_of(path);
    set_part(&part);
    match omsi_o3d::load_mesh(path) {
        Ok(m) => (true, json!({"kind": "mesh", "materials": m.materials.len(), "vertices": m.vertices.len()})),
        Err(e) => {
            report.error("mesh", format!("{e}"), &part);
            (false, json!({"kind": "mesh"}))
        }
    }
}

/// A map: global.cfg, every tile it lists, every object and spline the tiles place (there,
/// and readable; with `deep` also every model, mesh and texture of them), the ground
/// textures, the AI lists and the timetable.
fn check_map(root: &Path, global_path: &Path, deep: bool, report: &Report) -> (bool, Value) {
    let map_dir = global_path.parent().unwrap_or(Path::new("")).to_path_buf();
    let part = format!("{} (map)", name_of(&map_dir));
    set_part(&part);
    let global = match omsi_map::global::GlobalCfg::load(global_path) {
        Ok(g) => g,
        Err(e) => {
            report.error("load", format!("{e}"), &part);
            return (false, json!({"kind": "map"}));
        }
    };
    omsi_map::configure_grid(&global);
    // the ground textures
    for gt in &global.ground_textures {
        for t in [&gt.texture, &gt.detail_texture] {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            if omsi_texture::find_texture(t, &[root, &map_dir]).is_none() {
                report.warn("ground texture", format!("the texture {t} was not found"), None, &part);
            }
        }
    }
    // the tiles
    let mut tile_paths = Vec::new();
    let mut missing_tiles = 0;
    for t in &global.tiles {
        let p = omsi_cfg::resolve_path(&map_dir, &t.file);
        if omsi_cfg::vfs::is_file(&p) {
            tile_paths.push((t.x, t.y, p));
        } else {
            missing_tiles += 1;
            if missing_tiles <= 50 {
                report.warn("tile", format!("the tile file {} was not found (tile {},{})", t.file.trim(), t.x, t.y), None, &part);
            }
        }
    }
    if missing_tiles > 50 {
        report.warn("tile", format!("and {} more tile files were not found", missing_tiles - 50), None, &part);
    }
    // what the tiles place: (file lower-case) -> (as written, kind, first tile, uses)
    let used: Mutex<BTreeMap<String, (String, &'static str, String, usize)>> = Mutex::new(BTreeMap::new());
    tile_paths.par_iter().for_each(|(x, y, p)| {
        set_part(&format!("tile {x},{y}"));
        let tile = match omsi_map::tile::Tile::load(p) {
            Ok(t) => t,
            Err(e) => {
                report.error("tile", format!("the tile {} cannot be read: {e}", name_of(p)), &part);
                return;
            }
        };
        let mut u = used.lock().unwrap_or_else(|e| e.into_inner());
        let mut note = |file: &str, kind: &'static str| {
            let f = file.trim();
            if f.is_empty() {
                return;
            }
            let e = u.entry(f.to_ascii_lowercase().replace('\\', "/")).or_insert_with(|| (f.to_string(), kind, format!("{x},{y}"), 0));
            e.3 += 1;
        };
        for o in tile.objects.iter().chain(&tile.attach_objects) {
            note(&o.file, "object");
        }
        for s in &tile.splines {
            note(&s.file, "spline");
        }
    });
    let used = used.into_inner().unwrap_or_else(|e| e.into_inner());
    // missing files, by the add-on folder they come with
    let mut by_addon: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut present: Vec<(PathBuf, &'static str)> = Vec::new();
    for (written, kind, tile, uses) in used.values() {
        let p = omsi_cfg::resolve_path(root, written);
        if omsi_cfg::vfs::is_file(&p) {
            present.push((p, kind));
        } else {
            let addon = written.replace('\\', "/").split('/').take(2).collect::<Vec<_>>().join("/");
            by_addon.entry(addon).or_default().push(format!("{kind} {written} (first on tile {tile}, used {uses} times)"));
        }
    }
    let missing_files: usize = by_addon.values().map(Vec::len).sum();
    for (addon, list) in &by_addon {
        report.warn(
            "not installed",
            format!("not installed: {} files the map uses from {addon}", list.len()),
            Some(list.iter().take(30).cloned().collect::<Vec<_>>().join("; ") + if list.len() > 30 { "; ..." } else { "" }),
            &part,
        );
    }
    // what is there: read (and with `deep` loaded whole) as the game would
    let broken = std::sync::atomic::AtomicUsize::new(0);
    present.par_iter().for_each(|(p, kind)| {
        let ok = match (*kind, deep) {
            ("object", true) => check_object(root, p, report).0,
            ("spline", true) => check_spline(root, p, report).0,
            ("object", false) => {
                set_part(&name_of(p));
                omsi_scenery::SceneryObject::load(p).map_err(|e| report.error("load", format!("{e}"), &name_of(p))).is_ok()
            }
            _ => {
                set_part(&name_of(p));
                omsi_scenery::Spline::load(p).map_err(|e| report.error("load", format!("{e}"), &name_of(p))).is_ok()
            }
        };
        if !ok {
            broken.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
    set_part(&part);
    // the timetable and the AI lists, as the game reads them
    let tt = omsi_cfg::resolve_path(&map_dir, "TTData");
    let timetable = omsi_cfg::vfs::is_dir(&tt);
    let ailists = omsi_cfg::resolve_path(&map_dir, "ailists.cfg");
    if !omsi_cfg::vfs::is_file(&ailists) {
        report.add("info", "ailists", "no ailists.cfg: no AI traffic on this map".into(), None, &part);
    } else {
        if let Ok(lists) = omsi_map::ailists::AiLists::load(&ailists) {
            let mut missing = BTreeMap::new();
            let files = lists.groups.iter().flat_map(|g| g.vehicles.iter().map(|v| &v.file).chain(g.typgroups.iter().map(|t| &t.file)));
            for v in files {
                let v = v.trim();
                if !v.is_empty() && !omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(root, v)) {
                    *missing.entry(v.to_string()).or_insert(0) += 1;
                }
            }
            if !missing.is_empty() {
                report.warn(
                    "ailists",
                    format!("not installed: {} AI vehicles in ailists.cfg", missing.len()),
                    Some(missing.keys().take(30).cloned().collect::<Vec<_>>().join("; ")),
                    &part,
                );
            }
        }
    }
    if global.entry_points.is_empty() {
        report.warn("entry points", "the map has no entry point: a drive cannot start on it".into(), None, &part);
    }
    let objects = used.values().filter(|v| v.1 == "object").count();
    (
        true,
        json!({
            "kind": "map",
            "name": if global.friendly_name.trim().is_empty() { global.name.clone() } else { global.friendly_name.clone() },
            "tiles": global.tiles.len(),
            "tiles_missing": missing_tiles,
            "objects": objects,
            "splines": used.len() - objects,
            "files_missing": missing_files,
            "files_broken": broken.into_inner(),
            "entry_points": global.entry_points.len(),
            "timetable": timetable,
            "deep": deep,
        }),
    )
}

/// What `path` is, by its name (or for a folder, by what is in it).
fn kind_of(path: &Path) -> &'static str {
    if omsi_cfg::vfs::is_dir(path) {
        return if omsi_cfg::vfs::is_file(&path.join("global.cfg")) { "map" } else { "folder" };
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "bus" | "ovh" => "vehicle",
        "sco" => "object",
        "sli" => "spline",
        "hum" => "human",
        "owt" => "weather",
        "o3d" | "x" => "mesh",
        "zip" => "folder",
        _ if name_of(path).eq_ignore_ascii_case("global.cfg") => "map",
        _ => "unknown",
    }
}

/// Everything in a folder (a mod, an add-on, a zip): each vehicle, object, spline, human,
/// weather and map in it, checked in turn. (Meshes on their own are checked through the
/// models that name them.)
fn check_folder(root: &Path, dir: &Path, deep: bool, report: &Report) -> (bool, Value) {
    let mut items: Vec<(PathBuf, &'static str)> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for p in omsi_cfg::vfs::read_dir_paths(&d) {
            if omsi_cfg::vfs::is_dir(&p) {
                if omsi_cfg::vfs::is_file(&p.join("global.cfg")) {
                    items.push((p.join("global.cfg"), "map"));
                } else {
                    stack.push(p);
                }
                continue;
            }
            match kind_of(&p) {
                k @ ("vehicle" | "object" | "spline" | "human" | "weather") => items.push((p, k)),
                _ => {}
            }
        }
    }
    items.sort();
    let total = items.len();
    const MAX_ITEMS: usize = 3000;
    if total > MAX_ITEMS {
        report.warn("folder", format!("only the first {MAX_ITEMS} of {total} files are checked"), None, &name_of(dir));
        items.truncate(MAX_ITEMS);
    }
    let results: Vec<Value> = items
        .par_iter()
        .map(|(p, k)| {
            let (ok, summary) = check_one(root, p, k, deep, report);
            json!({"file": short(p, root), "kind": k, "ok": ok, "summary": summary})
        })
        .collect();
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, k) in &items {
        *by_kind.entry(k).or_default() += 1;
    }
    if total == 0 {
        report.warn("folder", "no vehicle, scenery object, spline, human, weather or map found in it".into(), None, &name_of(dir));
    }
    (true, json!({"kind": "folder", "name": name_of(dir), "items": results, "counts": by_kind}))
}

fn check_one(root: &Path, path: &Path, kind: &str, deep: bool, report: &Report) -> (bool, Value) {
    match kind {
        "vehicle" => check_vehicle(root, path, report),
        "object" => check_object(root, path, report),
        "spline" => check_spline(root, path, report),
        "human" => check_human(root, path, report),
        "weather" => check_weather(path, report),
        "mesh" => check_mesh(path, report),
        "map" => check_map(root, &if omsi_cfg::vfs::is_dir(path) { path.join("global.cfg") } else { path.to_path_buf() }, deep, report),
        "folder" => check_folder(root, path, deep, report),
        _ if !omsi_cfg::vfs::is_file(path) && !omsi_cfg::vfs::is_dir(path) => {
            report.error("arguments", format!("the file {} was not found", path.display()), &name_of(path));
            (false, json!({"kind": "unknown"}))
        }
        _ => {
            report.error("arguments", format!("the game does not read {} (it reads a .bus, .ovh, .sco, .sli, .hum, .owt, .o3d, a map or a folder)", name_of(path)), &name_of(path));
            (false, json!({"kind": "unknown"}))
        }
    }
}

/// `args`: what follows `--check` on the command line; `find_root`: where the game finds
/// OMSI when `--root` does not say.
pub fn run(args: &[String], find_root: fn() -> Option<PathBuf>) -> anyhow::Result<()> {
    let _ = log::set_logger(&CAPTURE);
    log::set_max_level(log::LevelFilter::Info);
    let mut files: Vec<PathBuf> = Vec::new();
    let mut root: Option<PathBuf> = None;
    let mut deep = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = it.next().map(PathBuf::from),
            "--deep" => deep = true,
            _ => files.push(PathBuf::from(a)),
        }
    }
    if files.is_empty() {
        println!("{}", json!({"ok": false, "findings": [{"level": "error", "what": "arguments", "text": "nothing to check given"}]}));
        return Ok(());
    }
    let root = root.or_else(find_root).unwrap_or_else(|| PathBuf::from("."));
    // the content roots the game reads: its own folder (mods), the archives, the installation
    if let Some(c) = omsi_cfg::content_dir() {
        if omsi_cfg::vfs::is_dir(&c) {
            omsi_cfg::add_content_root(c.clone());
        }
        omsi_cfg::vfs::mount_dir_zips(&c.join("Archives"));
    }
    omsi_cfg::add_content_root(root.clone());
    // (a mod's paths are given relative to the content folder, which is searched first)
    let resolve = |f: &PathBuf| if f.is_absolute() { f.clone() } else { omsi_cfg::resolve_path(&root, &f.to_string_lossy()) };
    let mut targets: Vec<PathBuf> = files.iter().map(resolve).collect();
    // a zip (a mod not installed yet, an archive read in place): read where it lies
    for target in targets.iter_mut() {
        if target.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) && omsi_cfg::vfs::is_file(target) {
            match omsi_cfg::vfs::add_content_zip(target) {
                Ok(m) => *target = m,
                Err(e) => {
                    println!("{}", json!({"ok": false, "file": target.display().to_string(), "errors": 1, "findings": [{"level": "error", "what": "zip", "text": format!("{e}")}]}));
                    return Ok(());
                }
            }
        }
    }
    let started = std::time::Instant::now();
    let report = Report::default();
    let (kind, ok, summary) = if targets.len() == 1 {
        let kind = kind_of(&targets[0]);
        let (ok, summary) = check_one(&root, &targets[0], kind, deep, &report);
        (kind, ok, summary)
    } else {
        // several (the folders of one mod): each in turn, as the items of a folder
        // (a folder's pieces are listed as its own: a mod is the buses, objects and maps in it)
        let items: Vec<Value> = targets
            .iter()
            .flat_map(|p| {
                let k = kind_of(p);
                let (ok, summary) = check_one(&root, p, k, deep, &report);
                match summary.get("items").and_then(Value::as_array) {
                    Some(inner) if k == "folder" => inner.clone(),
                    _ => vec![json!({"file": short(p, &root), "kind": k, "ok": ok, "summary": summary})],
                }
            })
            .collect();
        let ok = items.iter().all(|i| i["ok"] == true);
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for i in &items {
            *counts.entry(i["kind"].as_str().unwrap_or("").to_string()).or_default() += 1;
        }
        ("folder", ok, json!({"kind": "folder", "items": items, "counts": counts}))
    };
    let target = targets[0].clone();
    // the loaders' own warnings and errors, on the part they were loading
    let log = CAPTURE.0.lock().unwrap_or_else(|e| e.into_inner());
    let mut findings = report.findings.into_inner().unwrap_or_else(|e| e.into_inner());
    for (level, target, text, part) in log.iter() {
        if *level <= log::Level::Warn {
            findings.push(json!({"level": level.to_string().to_lowercase(), "what": target.rsplit("::").next().unwrap_or(target), "text": text, "part": part}));
        }
    }
    let errors = findings.iter().filter(|f| f["level"] == "error").count();
    let warnings = findings.iter().filter(|f| f["level"] == "warn").count();
    println!(
        "{}",
        json!({
            "ok": ok && errors == 0,
            "kind": kind,
            "file": target.display().to_string(),
            "root": root.display().to_string(),
            "seconds": started.elapsed().as_secs_f32(),
            "summary": summary,
            "errors": errors + usize::from(!ok && errors == 0),
            "warnings": warnings,
            "log_lines": log.len(),
            "findings": findings,
        })
    );
    Ok(())
}
