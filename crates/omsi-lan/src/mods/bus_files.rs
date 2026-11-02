//! Every file a bus may read, found the way the game finds them - what goes into its
//! manifest (see `identity`). Read from the bus's own description, not from what one run of
//! the game happened to read: a run sees summer and day, and a bus put together from its
//! reads lacked its night maps and winter textures for whoever saw it at night (finding
//! every dependency by tracing a run cannot be complete - Guo & Engler, USENIX ATC 2011).
//! What a run did read is added on top (`upload::files_of`), for whatever this misses.
//!
//! When in doubt a file goes in: one too many costs a few kilobytes, one too few is a bus
//! that is not the same.

use omsi_sim::VehicleType;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// How many coupled parts one vehicle may have behind it (as the game's spawn allows).
const MAX_COUPLED_PARTS: usize = 32;

/// The season folders a texture may have a copy in (`omsi_texture::season_chain`).
const SEASONS: &[&str] = &["wintersnowfall", "wintersnow", "winter", "fall", "spring"];

/// The files of `ty` in paint `scheme` (None: its own look), with the depot file `hof` its
/// displays use. Absolute paths, each an existing file.
pub(crate) fn of_type(root: &Path, ty: &VehicleType, scheme: Option<usize>, hof: Option<&Path>) -> Vec<PathBuf> {
    of_parts(root, ty, &[], scheme, hof)
}

/// `of_type` for a vehicle already loaded with its rear sections (`parts`, front to back):
/// those are taken as they are instead of being read from the disk again - an articulated
/// bus's trailer was loaded a second time for its list.
pub(crate) fn of_parts(root: &Path, ty: &VehicleType, parts: &[&VehicleType], scheme: Option<usize>, hof: Option<&Path>) -> Vec<PathBuf> {
    let _listing = Listing::start();
    let mut out = BTreeSet::new();
    add_type(root, ty, parts, scheme, &mut out, 0, false);
    if let Some(h) = hof {
        add(&mut out, h.to_path_buf());
    }
    out.into_iter().collect()
}

type Folder = Option<Vec<(std::ffi::OsString, bool)>>;

thread_local! {
    /// The folders read during one listing (`Listing`): each texture's `variants` read its
    /// whole folder and the season folders beside it, so a bus of ten paints read the same
    /// big texture folders hundreds of times (a fleet check spent up to 98 s on one bus).
    static DIRS: std::cell::RefCell<Option<std::collections::HashMap<PathBuf, Folder>>> = const { std::cell::RefCell::new(None) };
}

/// Keeps the folders read until the outermost one is dropped: only within one listing, so a
/// later listing sees the files added meanwhile.
struct Listing(bool);

impl Listing {
    fn start() -> Listing {
        Listing(DIRS.with(|d| {
            let mut d = d.borrow_mut();
            let first = d.is_none();
            d.get_or_insert_with(Default::default);
            first
        }))
    }
}

impl Drop for Listing {
    fn drop(&mut self) {
        if self.0 {
            DIRS.with(|d| *d.borrow_mut() = None);
        }
    }
}

/// `vfs::list_dir`, read once per folder within a `Listing`.
fn list_dir(dir: &Path) -> Folder {
    let kept = DIRS.with(|d| d.borrow().as_ref().map(|m| m.get(dir).cloned()));
    match kept {
        Some(Some(hit)) => hit,
        Some(None) => {
            let read = omsi_cfg::vfs::list_dir(dir);
            DIRS.with(|d| {
                if let Some(m) = d.borrow_mut().as_mut() {
                    m.insert(dir.to_path_buf(), read.clone());
                }
            });
            read
        }
        None => omsi_cfg::vfs::list_dir(dir),
    }
}

fn add(out: &mut BTreeSet<PathBuf>, p: PathBuf) {
    if omsi_cfg::vfs::is_file(&p) {
        out.insert(p);
    }
}

/// `of_type` of each of `schemes`, together: the same files as their union, but what no
/// paint changes (scripts, meshes, sounds and their folders, the fonts - every `.oft` read)
/// is gathered once, and for each other paint only its textures. The server's fleet check
/// went through all of a bus once per paint: 40 to 90 s for a bus of ten.
pub(crate) fn of_type_paints(root: &Path, ty: &VehicleType, schemes: &[Option<usize>]) -> Vec<PathBuf> {
    let _listing = Listing::start();
    let mut out = BTreeSet::new();
    for (i, scheme) in schemes.iter().enumerate() {
        add_type(root, ty, &[], *scheme, &mut out, 0, i > 0);
    }
    out.into_iter().collect()
}

/// (`textures_only`: the paint's textures, of it and its rear sections - the rest was taken)
fn add_type(root: &Path, ty: &VehicleType, parts: &[&VehicleType], scheme: Option<usize>, out: &mut BTreeSet<PathBuf>, depth: usize, textures_only: bool) {
    let def = &ty.def;
    if textures_only {
        add_textures(root, ty, scheme, out);
        if depth < MAX_COUPLED_PARTS {
            if let Some(p) = def.couple_back_path() {
                match VehicleType::load(root, &p) {
                    Ok(part) => add_type(root, &part, &[], scheme.filter(|i| *i < part.paint_schemes.len()), out, depth + 1, true),
                    Err(e) => log::warn!("bus files: {}: {e}", p.display()),
                }
            }
        }
        return;
    }
    let here = def.dir();
    add(out, def.path.clone());
    let s = &def.scripts;
    for p in s.varlists.iter().chain(&s.stringvarlists).chain(&s.scripts).chain(&s.constfiles) {
        add(out, p.clone());
    }
    add(out, ty.model.path.clone());
    for vm in &ty.meshes {
        add(out, vm.file.clone());
    }
    for rel in [&def.paths, &def.passenger_cabin, &def.number_file].into_iter().flatten() {
        add(out, omsi_cfg::resolve_path(here, rel));
    }
    if let Some((list, _, _)) = &def.registration_list {
        add(out, omsi_cfg::resolve_path(here, list));
    }
    for rel in [&def.sound, &def.sound_ai].into_iter().flatten() {
        add_sounds(&omsi_cfg::resolve_path(here, rel), out);
    }
    add_textures(root, ty, scheme, out);
    add_fonts(root, ty.model.text_textures.iter().map(|t| t.font.as_str()), out);
    // the rear sections of an articulated bus, each a vehicle of its own
    if depth < MAX_COUPLED_PARTS {
        if let Some(p) = def.couple_back_path().filter(|p| !out.contains(p)) {
            // (the section the vehicle has loaded already, when it is that file)
            match parts.split_first().filter(|(part, _)| part.def.path == p) {
                Some((part, rest)) => add_type(root, part, rest, scheme.filter(|i| *i < part.paint_schemes.len()), out, depth + 1, false),
                None => match VehicleType::load(root, &p) {
                    Ok(part) => add_type(root, &part, &[], scheme.filter(|i| *i < part.paint_schemes.len()), out, depth + 1, false),
                    Err(e) => log::warn!("bus files: {}: {e}", p.display()),
                },
            }
        }
    }
}

/// A sound config, its clips, and every other sound beside them (a script names one by
/// itself, `[sound] N`: which is known only when it plays).
fn add_sounds(cfg: &Path, out: &mut BTreeSet<PathBuf>) {
    let Ok(c) = omsi_vehicle::SoundCfg::load(cfg) else { return };
    add(out, cfg.to_path_buf());
    let dir = cfg.parent().unwrap_or(Path::new(""));
    // (its fixed clips, as the mixer finds them; `[sound] N` names none)
    let clips: Vec<PathBuf> = c.sounds.iter().filter(|def| def.file.trim().parse::<i32>().is_err()).map(|def| omsi_cfg::resolve_path(dir, &def.file)).collect();
    let mut folders: BTreeSet<PathBuf> = BTreeSet::new();
    for p in clips {
        if let Some(f) = p.parent() {
            folders.insert(f.to_path_buf());
        }
        add(out, p);
    }
    folders.insert(dir.to_path_buf());
    for f in folders {
        for (name, is_dir) in list_dir(&f).unwrap_or_default() {
            let n = name.to_string_lossy().to_ascii_lowercase();
            if !is_dir && (n.ends_with(".wav") || n.ends_with(".ogg")) {
                add(out, f.join(&name));
            }
        }
    }
}

/// Every texture the meshes name in that paint - the material's own and each of its maps -
/// found as the renderer finds them, and the files that go with each (`variants`).
fn add_textures(root: &Path, ty: &VehicleType, scheme: Option<usize>, out: &mut BTreeSet<PathBuf>) {
    let mut dirs = ty.texture_dirs(root);
    let subst = match scheme {
        Some(i) => {
            let (m, d) = ty.scheme_substitutions(i, omsi_texture::find_texture);
            if let Some(d) = d {
                // the scheme's own description (`.cti`): which of them it is
                for (name, is_dir) in list_dir(&d).unwrap_or_default() {
                    if !is_dir && name.to_string_lossy().to_ascii_lowercase().ends_with(".cti") {
                        add(out, d.join(&name));
                    }
                }
                dirs.insert(0, d);
            }
            m
        }
        None => {
            let m = ty.default_substitutions(root);
            // (its values are full paths)
            for p in m.values() {
                add(out, PathBuf::from(p));
            }
            m
        }
    };
    let subst = |name: &str| subst.get(&name.trim().to_ascii_lowercase()).cloned().unwrap_or_else(|| name.trim().to_string());
    let mut wanted: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for vm in &ty.meshes {
        for m in &vm.materials {
            match ty.texchange(&m.texture) {
                Some(master) => {
                    let mut d = vec![master.dir.clone()];
                    d.extend(dirs.iter().cloned());
                    for e in &master.entries {
                        wanted.push((subst(e), d.clone()));
                    }
                }
                None => wanted.push((subst(&m.texture), dirs.clone())),
            }
        }
        for o in &vm.overrides {
            let mut names: Vec<String> = vec![o.texture.clone()];
            names.extend(o.nightmap.clone());
            names.extend(o.transmap.clone());
            names.extend(o.envmap_mask.clone());
            names.extend(o.raindropmap.clone());
            names.extend(o.envmap.clone().map(|e| e.0));
            names.extend(o.bumpmap.clone().map(|b| b.0));
            names.extend(o.lightmap.clone().map(|l| l.0));
            names.extend(o.lightmaps.iter().map(|l| l.0.clone()));
            if let Some((a, b)) = &o.freetex {
                names.push(a.clone());
                names.push(b.clone());
            }
            for n in names {
                wanted.push((subst(&n), dirs.clone()));
            }
        }
    }
    for (name, d) in wanted {
        // (a script's or text's texture, `\S:1`, is no file; nor is a word that names none)
        if name.is_empty() || name.starts_with('\\') || !name.contains('.') {
            continue;
        }
        let refs: Vec<&Path> = d.iter().map(|p| p.as_path()).collect();
        if let Some(found) = omsi_texture::find_texture(&name, &refs) {
            for p in variants(&found) {
                add(out, p);
            }
            let pbr = omsi_texture::pbr::find(&found);
            for p in pbr.normals.into_iter().map(|n| n.0).chain(pbr.rough).chain(pbr.metal).chain(pbr.ao).chain(pbr.packed.map(|p| p.0)) {
                add(out, p);
            }
        }
    }
}

/// `found` and every file that goes with it: the same name with another ending in its
/// folder (`bus.dds.cfg`, `bus_nn.png`, `bus-night.dds`) and in the season folders beside it
/// (`Winter\bus.dds`) - whatever the game would look for at night, in winter or in another
/// graphics mode.
pub(crate) fn variants(found: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (found.parent(), found.file_stem()) else { return vec![found.to_path_buf()] };
    let stem = stem.to_string_lossy().to_ascii_lowercase();
    let mut folders = vec![dir.to_path_buf()];
    for (name, is_dir) in list_dir(dir).unwrap_or_default() {
        if is_dir && SEASONS.contains(&name.to_string_lossy().to_ascii_lowercase().as_str()) {
            folders.push(dir.join(&name));
        }
    }
    let mut out = vec![found.to_path_buf()];
    for f in folders {
        for (name, is_dir) in list_dir(&f).unwrap_or_default() {
            let n = name.to_string_lossy().to_ascii_lowercase();
            let goes = n.strip_prefix(&stem).and_then(|rest| rest.chars().next()).is_some_and(|c| matches!(c, '.' | '_' | '-'));
            if !is_dir && goes {
                out.push(f.join(&name));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The `.oft` files of the content folders that define one of `fonts`, with their bitmaps.
pub(crate) fn add_fonts<'a>(root: &Path, fonts: impl Iterator<Item = &'a str>, out: &mut BTreeSet<PathBuf>) {
    let wanted: BTreeSet<String> = fonts.map(|f| f.trim().to_lowercase()).filter(|f| !f.is_empty()).collect();
    if wanted.is_empty() {
        return;
    }
    // every content root's own `Fonts` (a path through `resolve_path` is the one of the root
    // that holds that folder first: the content folder's empty `Fonts` stood for all of them,
    // and no bus took a font along - its signs stayed blank for the others)
    let mut dirs = omsi_cfg::mirrored_dirs(&omsi_cfg::resolve_path(root, "Fonts"));
    let installation = root.join("Fonts");
    if !dirs.contains(&installation) && omsi_cfg::vfs::is_dir(&installation) {
        dirs.push(installation);
    }
    let before = out.len();
    let mut seen: Vec<String> = Vec::new();
    // (.oft, its text, its bitmaps) of every file naming a wanted font, by folder
    let mut found: Vec<(PathBuf, String, Vec<PathBuf>)> = Vec::new();
    for dir in &dirs {
        let dir = dir.clone();
        let list = list_dir(&dir);
        let (mut ofts, mut unread) = (0, 0);
        for (name, is_dir) in list.clone().unwrap_or_default() {
            if is_dir || !name.to_string_lossy().to_ascii_lowercase().ends_with(".oft") {
                continue;
            }
            ofts += 1;
            let path = dir.join(&name);
            let Some(text) = oft_text(&path) else {
                unread += 1;
                continue;
            };
            let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
            let mut bitmaps = Vec::new();
            for (i, l) in lines.iter().enumerate() {
                if l.eq_ignore_ascii_case("[newfont]") && lines.get(i + 1).is_some_and(|n| wanted.contains(&n.to_lowercase())) {
                    bitmaps.extend(lines.iter().skip(i + 2).take(2).map(|b| omsi_cfg::resolve_path(&dir, b)));
                }
            }
            if !bitmaps.is_empty() {
                found.push((path, text, bitmaps));
            }
        }
        seen.push(format!("{} ({} entries, {ofts} .oft, {unread} unreadable)", dir.display(), list.map(|l| l.len()).unwrap_or(0)));
    }
    for (path, _, bitmaps) in without_copies(found) {
        add(out, path);
        for b in bitmaps {
            add(out, b);
        }
    }
    // (a display's font that is nowhere: its sign stays blank for the others too)
    if out.len() == before {
        log::warn!("bus files: no font of {wanted:?} in {seen:?}: its signs stay blank for the other players");
    }
}

/// The text of an `.oft`, read once per process while it stays the same file (its size and
/// time): every bus of a fleet check read every `.oft` of every Fonts folder again.
fn oft_text(path: &Path) -> Option<String> {
    type Texts = std::collections::HashMap<PathBuf, (u64, Option<std::time::SystemTime>, String)>;
    static TEXTS: std::sync::Mutex<Option<Texts>> = std::sync::Mutex::new(None);
    let Ok(meta) = std::fs::metadata(path) else { return omsi_cfg::vfs::read_text(path).ok() };
    let stamp = (meta.len(), meta.modified().ok());
    let mut texts = TEXTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((len, time, text)) = texts.get_or_insert_with(Default::default).get(path) {
        if (*len, *time) == stamp {
            return Some(text.clone());
        }
    }
    drop(texts);
    let text = omsi_cfg::vfs::read_text(path).ok()?;
    TEXTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(path.to_path_buf(), (stamp.0, stamp.1, text.clone()));
    Some(text)
}

/// `found` without the copies of a file beside it: an `.oft` with the very text of another in
/// its folder (Windows' `LCD_7-Segment (2).oft` next to `LCD_7-Segment.oft`) draws the same
/// letters, yet it made the bus another version for every player and server without the copy.
/// Of the same text, the shortest name is kept (then the first by name).
pub(super) fn without_copies(found: Vec<(PathBuf, String, Vec<PathBuf>)>) -> Vec<(PathBuf, String, Vec<PathBuf>)> {
    let rank = |p: &Path| {
        let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        (n.chars().count(), n)
    };
    let mut kept: Vec<(PathBuf, String, Vec<PathBuf>)> = Vec::new();
    for f in found {
        match kept.iter().position(|k| k.0.parent() == f.0.parent() && k.1 == f.1) {
            Some(i) if rank(&f.0) < rank(&kept[i].0) => kept[i] = f,
            Some(_) => {}
            None => kept.push(f),
        }
    }
    kept
}

/// `path` from the root it lies in - the content folder, a mounted archive or the original
/// installation, whichever holds it most closely: the path every machine knows it by.
pub(crate) fn relative(root: &Path, path: &Path) -> Option<String> {
    let mut roots = omsi_cfg::content_roots();
    roots.push(root.to_path_buf());
    let r = roots.iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.components().count())?;
    let rest: Vec<String> = path.strip_prefix(r).ok()?.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    (!rest.is_empty()).then(|| rest.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-busfiles-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_folder_is_read_once_within_a_listing_and_afresh_after_it() {
        let d = temp("listing");
        std::fs::write(d.join("a.dds"), "a").unwrap();
        let names = |l: Folder| l.unwrap_or_default().len();
        {
            let _outer = Listing::start();
            assert_eq!(names(list_dir(&d)), 1);
            std::fs::write(d.join("b.dds"), "b").unwrap();
            {
                let _inner = Listing::start();
                assert_eq!(names(list_dir(&d)), 1, "a listing within one keeps what it read");
            }
            assert_eq!(names(list_dir(&d)), 1, "the inner one's end does not end the outer");
        }
        assert_eq!(names(list_dir(&d)), 2, "after the listing the folder is read again");
        let oft = d.join("f.oft");
        std::fs::write(&oft, "[newfont]\nA\n").unwrap();
        assert_eq!(oft_text(&oft).as_deref(), Some("[newfont]\nA\n"));
        std::fs::write(&oft, "[newfont]\nBB\n").unwrap();
        assert_eq!(oft_text(&oft).as_deref(), Some("[newfont]\nBB\n"), "a changed file is read again");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_texture_brings_its_night_winter_and_side_files_and_nothing_else() {
        let d = temp("variants");
        let tex = d.join("Texture");
        for f in ["Texture/body.dds", "Texture/body.dds.cfg", "Texture/body_nn.png", "Texture/body-night.dds", "Texture/Winter/body.dds", "Texture/bodywork.dds", "Texture/other.dds", "Texture/Logos/body.dds"] {
            let p = f.split('/').fold(d.clone(), |p, c| p.join(c));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        let got: Vec<String> = variants(&tex.join("body.dds")).iter().map(|p| p.strip_prefix(&d).unwrap().to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(got, vec!["Texture/Winter/body.dds", "Texture/body-night.dds", "Texture/body.dds", "Texture/body.dds.cfg", "Texture/body_nn.png"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn fonts_in_use_come_with_their_bitmaps() {
        let d = temp("fonts");
        std::fs::create_dir_all(d.join("Fonts")).unwrap();
        std::fs::write(d.join("Fonts/matrix.oft"), "[newfont]\nMatrix 16\nmatrix16.bmp\nmatrix16_a.bmp\n\n[newfont]\nOther\nother.bmp\nother_a.bmp\n").unwrap();
        std::fs::write(d.join("Fonts/unused.oft"), "[newfont]\nUnused\nu.bmp\nu_a.bmp\n").unwrap();
        for f in ["matrix16.bmp", "matrix16_a.bmp", "other.bmp", "u.bmp"] {
            std::fs::write(d.join("Fonts").join(f), b"x").unwrap();
        }
        let mut out = BTreeSet::new();
        add_fonts(&d, ["matrix 16"].into_iter(), &mut out);
        let got: Vec<String> = out.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(got, vec!["matrix.oft", "matrix16.bmp", "matrix16_a.bmp"]);
        // a copy of it Windows made beside it changes nothing
        std::fs::copy(d.join("Fonts/matrix.oft"), d.join("Fonts/matrix (2).oft")).unwrap();
        let mut again = BTreeSet::new();
        add_fonts(&d, ["Matrix 16"].into_iter(), &mut again);
        let names: Vec<String> = again.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["matrix.oft", "matrix16.bmp", "matrix16_a.bmp"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_path_is_named_from_its_closest_root() {
        let root = PathBuf::from("/omsi");
        assert_eq!(relative(&root, Path::new("/omsi/Vehicles/B/b.bus")).as_deref(), Some("Vehicles/B/b.bus"));
        assert_eq!(relative(&root, Path::new("/elsewhere/x.dds")), None);
    }

    /// A stock bus of an OMSI 2 installation (`OMSI_ROOT`), when there is one: its files
    /// include its night maps, the sounds beside its clips and its paint's description.
    #[test]
    fn a_stock_bus_is_complete() {
        let Some(root) = std::env::var_os("OMSI_ROOT").map(PathBuf::from) else { return };
        // as the game has it: its content folder first, laid out with an empty `Fonts`
        let content = temp("stock-content");
        omsi_cfg::ensure_content_layout(&content).unwrap();
        omsi_cfg::add_content_root_first(content.clone());
        omsi_cfg::add_content_root(root.clone());
        let bus = omsi_cfg::resolve_path(&root, "Vehicles/MAN_SD200/MAN_SD77.bus");
        let Ok(ty) = VehicleType::load(&root, &bus) else { return };
        let files = of_type(&root, &ty, (!ty.paint_schemes.is_empty()).then_some(0), None);
        let names: Vec<String> = files.iter().filter_map(|p| relative(&root, p)).map(|r| r.to_lowercase()).collect();
        assert!(names.iter().any(|n| n.ends_with(".bus")), "{names:?}");
        assert!(names.iter().any(|n| n.ends_with(".o3d")));
        assert!(names.iter().any(|n| n.ends_with(".osc")));
        assert!(names.iter().any(|n| n.ends_with(".wav")));
        assert!(names.iter().any(|n| n.ends_with(".dds") || n.ends_with(".bmp") || n.ends_with(".tga") || n.ends_with(".jpg")));
        // the fonts its displays write with: without them another player's copy shows blank signs
        let fonts: Vec<&String> = names.iter().filter(|n| n.starts_with("fonts/")).collect();
        eprintln!("{} files for MAN_SD77, fonts {:?} of {:?}", names.len(), fonts, ty.model.text_textures.iter().map(|t| &t.font).collect::<Vec<_>>());
        assert!(ty.model.text_textures.is_empty() || fonts.iter().any(|n| n.ends_with(".oft")), "no font of its displays: {fonts:?}");
    }

    /// Every paint at once is the union of each paint on its own (`OMSI_ROOT`, a stock bus).
    #[test]
    fn all_paints_at_once_are_each_paint_together() {
        let Some(root) = std::env::var_os("OMSI_ROOT").map(PathBuf::from) else { return };
        let bus = omsi_cfg::resolve_path(&root, "Vehicles/MAN_SD200/MAN_SD77.bus");
        let Ok(ty) = VehicleType::load(&root, &bus) else { return };
        let schemes: Vec<Option<usize>> = std::iter::once(None).chain((0..ty.paint_schemes.len()).map(Some)).collect();
        let union: BTreeSet<PathBuf> = schemes.iter().flat_map(|s| of_type(&root, &ty, *s, None)).collect();
        assert_eq!(of_type_paints(&root, &ty, &schemes), union.into_iter().collect::<Vec<_>>());
    }
}

