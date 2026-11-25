//! `--only fleet`: every vehicle loaded the way the game loads it, with a per-vehicle
//! report of what is missing or unsupported - the files it names (across all content
//! roots), script errors, callbacks and fonts the scripts ask for, textures and sounds,
//! its coupled parts - and whether it can be put into service by itself.

use omsi_cfg::resolve_path;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The sub-folders of `dir` (a folder or a folder inside a mounted archive).
fn sub_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = omsi_cfg::vfs::list_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, is_dir)| *is_dir)
        .map(|(n, _)| dir.join(n))
        .collect();
    v.sort();
    v
}

/// Whether `dir` holds a vehicle file of its own.
fn has_vehicle(dir: &Path) -> bool {
    omsi_cfg::vfs::read_dir_paths(dir).iter().any(|f| {
        f.extension()
            .map(|x| x.eq_ignore_ascii_case("bus") || x.eq_ignore_ascii_case("ovh"))
            .unwrap_or(false)
    })
}

/// The `Vehicles` folder of one content root: this root's own. `resolve_path` would answer
/// with the folder a content root of higher priority mirrors, so a scan of installation and
/// mod folder together read the mod twice and never saw the installation's own buses.
fn vehicles_dir(root: &Path) -> Option<PathBuf> {
    let d = root.join("Vehicles");
    if omsi_cfg::vfs::is_dir(&d) {
        return Some(d);
    }
    omsi_cfg::vfs::list_dir(root)?
        .into_iter()
        .find(|(n, is_dir)| *is_dir && n.to_string_lossy().eq_ignore_ascii_case("vehicles"))
        .map(|(n, _)| root.join(n))
}

/// Vehicle files (`.bus`/`.ovh`) under `Vehicles` of the given content folders (archives
/// mounted as content roots included), highest-priority root first. A vehicle folder name
/// that exists under more than one root (a mod repacking a stock bus under its own name) is
/// only the first root's, the way the game's own content layering would only ever load one
/// of them - otherwise a report taken with `--content <mods>` double-counted it.
pub fn vehicle_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen_folders: std::collections::HashSet<String> = std::collections::HashSet::new();
    for d in dirs {
        let Some(vehicles) = vehicles_dir(d) else {
            continue;
        };
        for folder in sub_dirs(&vehicles) {
            let name = folder
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if !seen_folders.insert(name) {
                continue;
            }
            for f in omsi_cfg::vfs::read_dir_paths(&folder) {
                if f.extension()
                    .map(|e| e.eq_ignore_ascii_case("bus") || e.eq_ignore_ascii_case("ovh"))
                    .unwrap_or(false)
                {
                    out.push(f);
                }
            }
        }
    }
    out.sort();
    out
}

/// The `[scripttexture]` groups that the visible meshes of a model show: as the texture of
/// a slot (`[useScriptTexture] n`) or as its transparency map (`[matl_transmap] \S:n`, how
/// every flip-dot matrix is drawn). Several colours can be layers of one physical display;
/// it is blank only when every one of its layers is blank.
fn shown_script_texture_groups(
    model: &omsi_model::Model,
    value: impl Fn(&str) -> Option<f32>,
) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for m in &model.meshes {
        // Matrix packs often declare a mesh per LED colour, each selecting a different
        // script texture. Only the mesh whose `[visible]` value matches is actually on
        // screen, so inspecting every colour produced false "blank display" reports.
        if let Some((var, wanted)) = &m.visible {
            if let Some(actual) = value(var) {
                if (actual - wanted).abs() >= 0.5 {
                    continue;
                }
            }
        }
        let mut group: Vec<usize> = Vec::new();
        for o in &m.materials {
            let transmap = o
                .transmap
                .as_deref()
                .unwrap_or("")
                .trim()
                .strip_prefix("\\S:")
                .and_then(|n| n.trim().parse::<i32>().ok());
            for i in [o.use_script_texture, transmap].into_iter().flatten() {
                if let Ok(i) = usize::try_from(i) {
                    if i < model.script_textures.len() && !group.contains(&i) {
                        group.push(i);
                    }
                }
            }
        }
        if !group.is_empty() && !groups.contains(&group) {
            groups.push(group);
        }
    }
    groups
}

/// The `[texttexture]`s a material of the model really shows (`[useTextTexture] n`).
fn shown_text_textures(model: &omsi_model::Model) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    for m in &model.meshes {
        for o in &m.materials {
            if let Some(i) = o.use_text_texture.and_then(|i| usize::try_from(i).ok()) {
                if i < model.text_textures.len() && !v.contains(&i) {
                    v.push(i);
                }
            }
        }
    }
    v
}

fn missing(label: &str, base: &Path, rel: &str, out: &mut Vec<String>) -> Option<PathBuf> {
    if rel.trim().is_empty() {
        return None;
    }
    let p = resolve_path(base, rel);
    if omsi_cfg::vfs::exists(&p) {
        Some(p)
    } else {
        out.push(format!("missing {label}: {rel}"));
        None
    }
}

/// One vehicle: the problems found, in the order a modder would fix them.
pub fn check_vehicle(root: &Path, path: &Path, run: bool) -> (Vec<String>, Vec<String>) {
    let mut problems: Vec<String> = Vec::new();
    let mut info: Vec<String> = Vec::new();
    let def = match omsi_vehicle::Vehicle::load(path) {
        Ok(v) => v,
        Err(e) => return (vec![format!("cannot read: {e}")], info),
    };
    let dir = def.dir().to_path_buf();
    if !def.unknown_keywords.is_empty() {
        problems.push(format!("unknown keywords {:?}", def.unknown_keywords));
    }
    // how OMSI would offer it
    if def.is_selectable() {
        info.push(format!(
            "listed as \"{} {}\"",
            def.manufacturer.trim(),
            def.type_name.trim()
        ));
    } else if def.is_rear_section() {
        let fronts = omsi_vehicle::vehicle::front_sections_of(path);
        match fronts.first() {
            Some(f) => info.push(format!(
                "rear section of {} (not listed; spawned with it)",
                f.file_name().unwrap_or_default().to_string_lossy()
            )),
            None => problems.push(
                "rear section that no vehicle couples ([couple_back]) - it can never be driven"
                    .into(),
            ),
        }
        if !def.script_share && def.scripts.scripts.is_empty() {
            problems.push("rear section without [scriptshare] and without scripts: its animations have no variables".into());
        }
    } else {
        info.push("not listed (no [friendlyname]: AI or helper vehicle)".into());
    }
    // files the .bus names
    for (label, rel) in [
        ("model", &def.model),
        ("sound", &def.sound),
        ("sound_ai", &def.sound_ai),
        ("paths", &def.paths),
        ("passengercabin", &def.passenger_cabin),
        ("number list", &def.number_file),
    ] {
        if let Some(rel) = rel {
            missing(label, &dir, rel, &mut problems);
        }
    }
    for (label, list) in [
        ("varlist", &def.scripts.varlists),
        ("stringvarlist", &def.scripts.stringvarlists),
        ("script", &def.scripts.scripts),
        ("constfile", &def.scripts.constfiles),
    ] {
        for p in list {
            if !omsi_cfg::vfs::exists(p) {
                problems.push(format!("missing {label}: {}", p.display()));
            }
        }
    }
    if let Some((f, _)) = &def.couple_back {
        if let Some(p) = missing("coupled part [couple_back]", &dir, f, &mut problems) {
            info.push(format!(
                "couples {}",
                p.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
    }
    // the vehicle as the game loads it
    let vt = match omsi_sim::VehicleType::load(root, path) {
        Ok(v) => Arc::new(v),
        Err(e) => {
            problems.push(format!("does not load: {e:#}"));
            return (problems, info);
        }
    };
    let mut seen_err = std::collections::HashSet::new();
    for e in &vt.program.errors {
        let s = e.to_string();
        if seen_err.insert(s.clone()) {
            problems.push(format!("script: {s}"));
        }
    }
    let provided = omsi_sim::host::PROVIDED_CALLBACKS;
    let unknown: Vec<String> = vt
        .program
        .callbacks_used()
        .into_iter()
        .filter(|c| !provided.contains(&c.to_ascii_lowercase().as_str()))
        .collect();
    if !unknown.is_empty() {
        problems.push(format!(
            "callbacks the engine does not provide (read 0): {unknown:?}"
        ));
    }
    let mut fonts = omsi_sim::texttex::FontLibrary::new(root);
    let no_decode = |_: &Path| -> Option<(u32, u32, Vec<u8>)> { Some((1, 1, vec![0; 4])) };
    let mut bad_fonts: Vec<String> = Vec::new();
    let mut sibling_fonts: Vec<String> = Vec::new();
    for f in vt.program.literal_arguments("GetFontIndex") {
        if fonts.has_exact(&f) || bad_fonts.contains(&f) || sibling_fonts.contains(&f) {
            continue;
        }
        if fonts.get(&f, &no_decode).is_some() {
            sibling_fonts.push(f);
        } else {
            bad_fonts.push(f);
        }
    }
    if !bad_fonts.is_empty() {
        problems.push(format!(
            "GetFontIndex asks for fonts that are in no Fonts folder: {bad_fonts:?}"
        ));
    }
    if !sibling_fonts.is_empty() {
        info.push(format!("GetFontIndex asks for fonts that are in no Fonts folder (drawn with another weight of the same family): {sibling_fonts:?}"));
    }
    // only the text textures a mesh shows ([useTextTexture] n): a pack's model files keep
    // unused ones whose fonts it never shipped
    let shown = shown_text_textures(&vt.model);
    let mut bad_tt: Vec<String> = Vec::new();
    let mut unused_tt: Vec<String> = Vec::new();
    for (i, t) in vt.model.text_textures.iter().enumerate() {
        if fonts.get(&t.font, &no_decode).is_some() {
            continue;
        }
        let list = if shown.contains(&i) {
            &mut bad_tt
        } else {
            &mut unused_tt
        };
        if !list.contains(&t.font) {
            list.push(t.font.clone());
        }
    }
    if !bad_tt.is_empty() {
        problems.push(format!(
            "[texttexture] fonts that are in no Fonts folder: {bad_tt:?}"
        ));
    }
    if !unused_tt.is_empty() {
        info.push(format!(
            "fonts of unused [texttexture]s missing (harmless): {unused_tt:?}"
        ));
    }
    // meshes and their textures
    let model_dir = vt.model_dir.clone();
    let lod0_end = vt
        .model
        .lods
        .get(1)
        .map(|l| l.first_mesh)
        .unwrap_or(vt.model.meshes.len());
    let mut bad_meshes = Vec::new();
    for md in &vt.model.meshes[vt.model.lods.first().map(|l| l.first_mesh).unwrap_or(0)..lod0_end] {
        let p = resolve_path(&model_dir, &md.file);
        if !omsi_cfg::vfs::is_file(&p) {
            bad_meshes.push(md.file.clone());
        } else if let Err(e) = omsi_o3d::load_mesh(&p) {
            bad_meshes.push(format!("{} ({e})", md.file));
        }
    }
    if !bad_meshes.is_empty() {
        problems.push(format!(
            "{} meshes missing or unreadable: {:?}",
            bad_meshes.len(),
            bad_meshes
        ));
    }
    let dirs = vt.texture_dirs(root);
    let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
    let mut bad_tex: Vec<String> = Vec::new();
    let check_tex = |name: &str, bad: &mut Vec<String>| {
        let name = name.trim();
        if name.is_empty()
            || name.starts_with("\\S:")
            || name.to_ascii_lowercase().starts_with("mirror")
        {
            return;
        }
        if omsi_texture::find_texture(name, &dirs_ref).is_none()
            && !bad.iter().any(|b| b.eq_ignore_ascii_case(name))
        {
            bad.push(name.to_string());
        }
    };
    for vm in &vt.meshes {
        let def = &vt.model.meshes[vm.def_index];
        for (slot, m) in vm.materials.iter().enumerate() {
            let ov: Vec<&omsi_model::MaterialDef> = def
                .materials
                .iter()
                .filter(|o| omsi_sim::vehicle::override_slot(&vm.materials, o) == Some(slot))
                .collect();
            let generated = ov.iter().any(|o| {
                o.use_text_texture.is_some()
                    || o.use_script_texture.is_some()
                    || o.freetex.is_some()
            }) || vt.texchange(&m.texture).is_some();
            if !generated {
                check_tex(&m.texture, &mut bad_tex);
            }
            for o in ov {
                for t in [
                    o.nightmap.clone(),
                    o.transmap.clone(),
                    o.lightmap.clone().map(|l| l.0),
                    o.envmap.clone().map(|e| e.0),
                    o.envmap_mask.clone(),
                    o.bumpmap.clone().map(|b| b.0),
                ]
                .into_iter()
                .flatten()
                {
                    check_tex(&t, &mut bad_tex);
                }
            }
        }
    }
    if !bad_tex.is_empty() {
        problems.push(format!(
            "{} textures not found: {:?}",
            bad_tex.len(),
            bad_tex
        ));
    }
    for c in &vt.model.ctc {
        if !omsi_cfg::vfs::is_dir(&resolve_path(&dir, &c.path)) {
            problems.push(format!("paint scheme folder [CTC] not found: {}", c.path));
        }
    }
    info.push(format!(
        "{} meshes, {} script blocks, {} variables, {} paint schemes",
        vt.meshes.len(),
        vt.program.blocks.len(),
        vt.program.var_names.len(),
        vt.paint_schemes.len()
    ));
    // sounds
    if let Some(rel) = &def.sound {
        let p = resolve_path(&dir, rel);
        if let Ok(cfg) = omsi_vehicle::SoundCfg::load(&p) {
            let sdir = p.parent().map(Path::to_path_buf).unwrap_or_default();
            // a blank file name is a real `[sound]` block, not a broken reference: it is
            // how a sound is deliberately silenced (the MB_C2 gearbox script's neutral
            // sound, several stock `[sound]` blocks) and `VehicleHost::sound_trigger_file`
            // already treats it as "play nothing" at run time - counting it here only
            // buried the genuinely missing files in noise
            let bad: Vec<String> = cfg
                .sounds
                .iter()
                .filter(|s| {
                    !s.file.trim().is_empty()
                        && s.file.trim().parse::<i32>().is_err()
                        && !omsi_cfg::vfs::is_file(&resolve_path(&sdir, &s.file))
                })
                .map(|s| s.file.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if !bad.is_empty() {
                problems.push(format!("{} sound files not found: {:?}", bad.len(), bad));
            }
        }
    }
    // only what a player can drive is started (a rear section runs on its front's scripts,
    // an AI-only vehicle is never put into service by hand)
    if run && def.is_selectable() && !def.is_rear_section() {
        run_vehicle(root, &vt, &mut problems, &mut info);
    }
    (problems, info)
}

/// Start the vehicle the way Shift+U does and run its scripts for a while: whether the
/// electrics and the engine come on, which displays get drawn, which callbacks were missed.
fn run_vehicle(
    root: &Path,
    vt: &Arc<omsi_sim::VehicleType>,
    problems: &mut Vec<String>,
    info: &mut Vec<String>,
) {
    let mut host = omsi_sim::VehicleHost::new(omsi_sim::SimClock::default());
    host.font_lib = Some(Arc::new(parking_lot::Mutex::new(
        omsi_sim::texttex::FontLibrary::new(root),
    )));
    // the depot file next to the vehicle, as a map without one would give it
    host.hof = omsi_vehicle::hof::depot_files(vt.def.dir())
        .first()
        .and_then(|h| omsi_vehicle::Hof::load(h).ok())
        .map(Arc::new);
    let mut v = omsi_sim::VehicleInstance::new(vt.clone(), host);
    {
        let lib = v.host.font_lib.clone().unwrap();
        v.init_text_textures(&mut lib.lock(), &|p| {
            omsi_texture::decode_file(p)
                .ok()
                .map(|i| (i.width, i.height, i.rgba))
        });
    }
    let bound: Vec<String> = omsi_content::KeyboardCfg::load(&root.join("Inputs/keyboard.cfg"))
        .map(|k| k.vehicles.iter().map(|b| b.action.clone()).collect())
        .unwrap_or_default();
    let mut s = omsi_sim::startup::StartUp::new(&v, &bound);
    let dt = 1.0 / 30.0;
    let mut typed = false;
    for _ in 0..(20.0 / dt) as usize {
        if s.running() {
            s.tick(&mut v, &bound, dt);
        } else if !typed {
            // the displays show something only once the IBIS has a trip
            typed = true;
            match type_first_trip(&mut v) {
                Some(t) => info.push(t),
                None => {
                    info.push("no IBIS trip could be typed (no depot file, or no IBIS keys)".into())
                }
            }
        }
        v.update(dt);
    }
    let power = omsi_sim::startup::power_on(&v);
    let engine = omsi_sim::startup::engine_running(&v);
    let what = if s.report.is_empty() {
        "nothing pressed".to_string()
    } else {
        s.report.join(", ")
    };
    if power && engine {
        info.push(format!("start-up: {what}"));
    } else {
        problems.push(format!(
            "start-up leaves the electrics {} and the engine {}: {what}",
            if power { "on" } else { "OFF" },
            if engine { "running" } else { "OFF" }
        ));
    }
    if power && engine {
        let fba = v
            .var("bremse_p_Brzyl_FBA")
            .or_else(|| v.var("spring_brake_pressure"));
        if let Some(p) = fba {
            if p < 6.0e5 {
                problems.push(format!(
                    "air pressure low after 20s start-up (spring brake {:.1} bar < 6.0 bar)",
                    p / 1e5
                ));
            }
        }
    }
    let _ = v.update_text_textures();
    // a work texture holds its picture in the colour channels with alpha 0 (the Krüger
    // matrix), the LED textures in alpha alone: any byte counts
    let drawn = v
        .host
        .script_textures
        .iter()
        .filter(|t| t.rgba.iter().any(|b| *b != 0))
        .count();
    let with_text = v
        .text_textures
        .iter()
        .filter(|t| {
            t.last_text
                .as_deref()
                .map(|x| !x.trim().is_empty())
                .unwrap_or(false)
        })
        .count();
    info.push(format!(
        "after 20 s: script textures drawn {drawn}/{}, text textures with text {with_text}/{}",
        v.host.script_textures.len(),
        v.text_textures.len()
    ));
    // the displays a mesh really shows: a destination display that stays blank once the
    // IBIS has its trip is what a driver notices first in a mod that does not work here
    let mut blank: Vec<String> = Vec::new();
    for group in shown_script_texture_groups(&vt.model, |name| v.var(name)) {
        if group.iter().all(|i| {
            v.host
                .script_textures
                .get(*i)
                .map(|t| t.rgba.iter().all(|b| *b == 0))
                .unwrap_or(true)
        }) {
            let ids = group
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            blank.push(format!("[scripttexture] {ids}"));
        }
    }
    for i in shown_text_textures(&vt.model) {
        let Some(t) = v.text_textures.get(i) else {
            continue;
        };
        if t.atlas.is_none() {
            blank.push(format!(
                "[texttexture] {i} (\"{}\", no font)",
                t.def.variable
            ));
        }
    }
    if !blank.is_empty() {
        problems.push(format!(
            "displays a mesh shows that stay blank with the IBIS set: {blank:?}"
        ));
    }
    if !v.host.unknown_callbacks().is_empty() {
        problems.push(format!(
            "called callbacks the engine does not provide: {:?}",
            v.host.unknown_callbacks()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_matrix_colour_is_not_checked_as_a_blank_display() {
        let mut model = omsi_model::Model::default();
        model.script_textures = vec![(16, 8), (16, 8)];
        let mut inactive = omsi_model::MeshDef::default();
        inactive.visible = Some(("matrix_colour".into(), 1.0));
        inactive.materials.push(omsi_model::MaterialDef {
            use_script_texture: Some(1),
            ..Default::default()
        });
        model.meshes.push(inactive);

        assert!(shown_script_texture_groups(&model, |_| Some(0.0)).is_empty());
        assert_eq!(
            shown_script_texture_groups(&model, |_| Some(1.0)),
            vec![vec![1]]
        );
    }
}

/// Type the depot file's first numbered IBIS trip the way a driver does (log in where the IBIS wants
/// its driver number, line/Kurs, route); what the IBIS then has.
fn type_first_trip(v: &mut omsi_sim::VehicleInstance) -> Option<String> {
    let hof = v.host.hof.clone()?;
    // a trip of a numbered line (depot files start with test and service entries)
    let trip = hof
        .info_trips
        .iter()
        .find(|t| !t.line.trim().is_empty() && t.line.trim().bytes().any(|b| !b.is_ascii_digit()))
        .or_else(|| {
            hof.info_trips.iter().find(|t| {
                !t.line.trim().is_empty() && t.line.trim().bytes().all(|b| b.is_ascii_digit())
            })
        })?;
    let press = |v: &mut omsi_sim::VehicleInstance, keys: &str| {
        for d in keys.chars() {
            v.trigger(&format!("IBIS_{d}"));
        }
        v.trigger("IBIS_eingabe");
    };
    if let Some(pin) = v.ty.program.constant("PIN").filter(|p| *p >= 0.0) {
        v.trigger("IBIS_setmode_linie_kurs");
        if v.var("IBIS_mode").map(|m| m.round() as i32) != Some(1) {
            press(v, &format!("{}", pin.round() as u32));
        }
    }
    let line = omsi_cfg::parse_i32(&trip.code).max(0) as u32;
    let route = trip
        .code
        .trim()
        .chars()
        .rev()
        .take(2)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if !v.trigger("IBIS_setmode_linie_kurs") {
        return None;
    }
    press(v, &format!("{line:05}"));
    v.trigger("IBIS_setmode_route");
    press(v, &format!("{:0>2}", route));
    Some(format!(
        "IBIS typed line {} route {route} ({}): terminus index {:?}, route index {:?}",
        trip.line.trim(),
        trip.name.trim(),
        v.var("IBIS_TerminusIndex"),
        v.var("IBIS_RouteIndex")
    ))
}

/// What a problem line is about, for the summary: the engine-side cause a mod author (or
/// this project) would have to remove, not the file it happened in.
fn cause_of(problem: &str) -> &'static str {
    let p = problem.to_ascii_lowercase();
    match () {
        _ if p.starts_with("cannot read") || p.starts_with("does not load") => {
            "the vehicle does not load at all"
        }
        _ if p.starts_with("unknown keywords") => "keywords the loaders do not know",
        _ if p.starts_with("script:") && p.contains("varinvalid") => {
            "script: variables no varlist declares"
        }
        _ if p.starts_with("script:") && p.contains("functioninvalid") => {
            "script: curves no constfile defines"
        }
        _ if p.starts_with("script:") && p.contains("constantinvalid") => {
            "script: constants no constfile defines"
        }
        _ if p.starts_with("script:") && p.contains("macroinvalid") => {
            "script: macros no script defines"
        }
        _ if p.starts_with("script:") => "script: other compiler errors",
        _ if p.starts_with("callbacks the engine does not provide") => {
            "callbacks the engine does not provide"
        }
        _ if p.starts_with("called callbacks") => {
            "callbacks the engine does not provide (called while driving)"
        }
        _ if p.starts_with("getfontindex") || p.starts_with("[texttexture] fonts") => {
            "fonts that are in no Fonts folder"
        }
        // almost always a stock train/tram `.ovh` shipped without the front section that
        // would couple it (trains couple through `Trains/`, not `[couple_back]`): counting
        // it with the real problems buried them under the size of the rolling-stock fleet
        _ if p.starts_with("rear section that no vehicle couples") => {
            "not driveable alone (no [couple_back] found for it - almost always unfinished stock rolling stock, not a mod problem)"
        }
        _ if p.starts_with("displays a mesh shows") => "displays that stay blank with the IBIS set",
        _ if p.contains("textures not found") => "textures not found",
        _ if p.contains("meshes missing") => "meshes missing or unreadable",
        _ if p.contains("sound files not found") => "sound files not found",
        _ if p.starts_with("start-up leaves") => "cannot be put into service (Shift+U)",
        _ if p.starts_with("missing") => "files the vehicle names are not there",
        _ => "other",
    }
}

/// The compatibility report: which causes keep third-party content from working and how
/// much of the fleet each of them touches, worst offenders first. Counting the causes is
/// what says where to look next - one mod with 40 script errors is a mod's own bug, a
/// cause that touches half the fleet is ours.
fn report_causes(results: &[(PathBuf, Vec<String>, Vec<String>)], dirs: &[PathBuf]) {
    let mut by_cause: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for (_, problems, _) in results {
        let mut seen: Vec<&str> = Vec::new();
        for p in problems {
            let c = cause_of(p);
            let e = by_cause.entry(c).or_default();
            e.0 += 1;
            if !seen.contains(&c) {
                seen.push(c);
                e.1 += 1;
            }
        }
    }
    let mut causes: Vec<(&str, (usize, usize))> = by_cause.into_iter().collect();
    causes.sort_by_key(|(_, (n, v))| (std::cmp::Reverse(*v), std::cmp::Reverse(*n)));
    if !causes.is_empty() {
        println!("  by cause (vehicles affected, problems):");
        for (c, (n, v)) in &causes {
            println!("    {v:4} vehicles, {n:5} problems - {c}");
        }
    }
    let mut worst: Vec<(&PathBuf, usize)> = results
        .iter()
        .map(|(f, p, _)| (f, p.len()))
        .filter(|(_, n)| *n > 0)
        .collect();
    worst.sort_by_key(|(f, n)| (std::cmp::Reverse(*n), f.to_string_lossy().to_string()));
    if !worst.is_empty() {
        println!("  worst offenders:");
        for (f, n) in worst.iter().take(10) {
            let rel = dirs
                .iter()
                .find_map(|d| f.strip_prefix(d).ok())
                .unwrap_or(f);
            println!("    {n:3} problems - {}", rel.display());
        }
    }
}

pub fn check_fleet(
    root: &Path,
    content: &[PathBuf],
    filter: Option<&str>,
    run: bool,
    verbose: bool,
) {
    // every content root the game itself would search, mods first: `--content` alone used to
    // replace the installation's own fleet instead of adding to it, so a compatibility report
    // taken with `--content <mods>` never saw the stock buses at all (48 vehicles, all of them
    // the mod's, however big the installation's own Vehicles folder was)
    let mut dirs: Vec<PathBuf> = content.to_vec();
    if !dirs.iter().any(|d| d == root) {
        dirs.push(root.to_path_buf());
    }
    let files: Vec<PathBuf> = vehicle_files(&dirs)
        .into_iter()
        .filter(|f| {
            filter
                .map(|x| {
                    f.to_string_lossy()
                        .to_ascii_lowercase()
                        .contains(&x.to_ascii_lowercase())
                })
                .unwrap_or(true)
        })
        .collect();
    let results: Vec<(PathBuf, Vec<String>, Vec<String>)> = files
        .par_iter()
        .map(|f| {
            let (p, i) = check_vehicle(root, f, run);
            (f.clone(), p, i)
        })
        .collect();
    let clean = results.iter().filter(|r| r.1.is_empty()).count();
    println!(
        "[fleet] {} vehicles in {:?}: {} without problems",
        results.len(),
        dirs,
        clean
    );
    report_causes(&results, &dirs);
    // vehicle folders without a vehicle file: a repaint, an advert or textures for a bus
    // that has to be installed (or is installed in another content root)
    for d in &dirs {
        for f in vehicles_dir(d).map(|v| sub_dirs(&v)).unwrap_or_default() {
            let name = f
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if filter
                .map(|x| !name.to_ascii_lowercase().contains(&x.to_ascii_lowercase()))
                .unwrap_or(false)
            {
                continue;
            }
            if has_vehicle(&f) {
                continue;
            }
            let elsewhere = omsi_cfg::content_dirs(&format!("Vehicles/{name}"))
                .iter()
                .any(|x| has_vehicle(x));
            println!(
                "  Vehicles/{name}/ - no vehicle file: {}",
                if elsewhere {
                    "textures/repaints for the bus of another content root"
                } else {
                    "a repaint or textures for a bus that is not installed (not offered for driving)"
                }
            );
        }
    }
    for (f, problems, info) in &results {
        let rel = dirs
            .iter()
            .find_map(|d| f.strip_prefix(d).ok())
            .unwrap_or(f);
        println!(
            "  {} - {}",
            rel.display(),
            if problems.is_empty() {
                "ok".to_string()
            } else {
                format!("{} problem(s)", problems.len())
            }
        );
        if verbose {
            for i in info {
                println!("      {i}");
            }
        }
        for p in problems.iter().take(if verbose { usize::MAX } else { 12 }) {
            println!("    ! {p}");
        }
    }
}
