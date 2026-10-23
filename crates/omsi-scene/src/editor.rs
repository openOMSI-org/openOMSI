//! The object editor: a small part of what OMSI's map editor does, inside the game. The
//! scenery objects a tile places itself (its `[object]` records) can be picked, moved,
//! turned and deleted where they stand, and the tiles changed are written as copies into
//! the content folder - which the game reads before the installation - never into the
//! original map. New objects are made as copies of one that is there (C), and a copy
//! takes the shape of any other object of its folder (V): each is a new `[object]` record
//! after its model's. The ground is shaped with a brush where the view points (raised,
//! lowered, flattened); a tile's ground is written as its `.map.terrain` copy. Splines are
//! not part of it (the timetable is the launcher's Timetable page).
//!
//! Keys while it is on (Ctrl+Shift+E, or the game menu):
//! Enter picks the object nearest the middle of the view, Tab the next nearest;
//! I / K / J / L move it forward, back, left and right as the camera faces, U / O down and
//! up, N / M turn it (half a metre and five degrees a press, a tenth with Shift);
//! Delete takes it away (again: back), Backspace undoes all its edits, C copies it (the
//! copy is then the one edited), V gives a copy the next object of its folder, Ctrl+S
//! saves the changed tiles and Escape leaves the editor. Page Up / Page Down raise and
//! lower the ground under the middle of the view (a quarter metre, a twentieth with
//! Shift), F flattens it to the height at the middle, [ and ] make the brush smaller and
//! larger.

use crate::{ObjectEdit, World};
use glam::{DVec3, Vec3};
use hashbrown::HashMap;
use std::path::{Path, PathBuf};

/// A new object: a copy of `template` (a map object), moved and turned from it, perhaps of
/// another type of the template's folder.
pub struct Added {
    pub template: i64,
    pub tile: (i32, i32),
    pub id: i64,
    /// Its `.sco` (the template's, or one of its folder: `V`).
    pub sco: PathBuf,
    pub base: DVec3,
    pub base_heading: f64,
    pub moved: DVec3,
    pub turned: f64,
    pub deleted: bool,
    gpu: Option<crate::TileGpu>,
}

#[derive(Default)]
pub struct Editor {
    pub selected: Option<i64>,
    /// The copies made in this session, and the one being edited (it takes the keys).
    pub added: Vec<Added>,
    pub editing_added: Option<usize>,
    /// The tile of every object edited, by map id (its tile may be unloaded when saving).
    tiles: HashMap<i64, (i32, i32)>,
    /// The candidates of the last pick, nearest first (Tab walks them).
    candidates: Vec<i64>,
    next: usize,
    /// The ground brush's radius (m; 0: the default).
    brush: f64,
}

/// The ground brush's radius when none is set, and its limits (m).
const BRUSH: (f64, f64, f64) = (6.0, 1.0, 60.0);

/// What a key does in the editor.
pub enum Action {
    Copy,
    Variant,
    Pick,
    NextPick,
    Move(DVec3),
    Turn(f64),
    Delete,
    Undo,
    Save,
    Leave,
    /// Raise (or lower) the ground under the view by metres.
    Ground(f64),
    Flatten,
    /// Make the brush larger (or smaller) by this factor.
    Brush(f64),
}

impl Editor {
    /// The objects in front of the camera, those nearest the middle of the view first.
    pub fn pick(&mut self, world: &World, eye: DVec3, forward: Vec3) -> Option<i64> {
        self.editing_added = None;
        let f = forward.as_dvec3().normalize_or_zero();
        let objects = world.edit_objects.lock();
        let edits = world.object_edits.lock();
        let mut scored: Vec<(f64, i64)> = objects
            .iter()
            .filter_map(|(id, o)| {
                let e = edits.get(id).copied().unwrap_or_default();
                if e.deleted {
                    return None;
                }
                let d = o.pos + e.moved - eye;
                let along = d.dot(f);
                if !(1.0..150.0).contains(&along) {
                    return None;
                }
                // off the view's axis, as an angle (a near post beside the middle before a
                // far house right in it)
                let off = (d - f * along).length() / along;
                (off < 0.35).then_some((off + along * 0.0015, *id))
            })
            .collect();
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.candidates = scored.into_iter().map(|s| s.1).take(12).collect();
        self.next = 1;
        self.selected = self.candidates.first().copied();
        self.selected
    }

    pub fn next_pick(&mut self) -> Option<i64> {
        if self.candidates.is_empty() {
            return None;
        }
        let id = self.candidates[self.next % self.candidates.len()];
        self.next += 1;
        self.selected = Some(id);
        self.selected
    }

    /// The object being edited (a map object or a copy) put where the mouse drags it: its
    /// foot on the ground at `ground`, its turn kept.
    pub fn drag_to(&mut self, world: &World, renderer: &omsi_render::Renderer, scene: &mut omsi_render::Scene, ground: DVec3) -> Option<String> {
        if let Some(k) = self.editing_added {
            let a = self.added.get_mut(k)?;
            a.moved = ground - a.base;
            self.place_added(k, world, renderer, scene);
            return Some(self.describe(world));
        }
        let id = self.selected?;
        let (tile, pos) = world.edit_objects.lock().get(&id).map(|o| (o.tile, o.pos))?;
        let mut e = world.object_edits.lock().get(&id).copied().unwrap_or_default();
        e.moved = ground - pos;
        self.tiles.insert(id, tile);
        world.apply_object_edit(renderer, scene, id, e);
        Some(self.describe(world))
    }

    /// What the other players' games need to show the object being edited as it is now
    /// (`all`: every object edited or added this session): LAN commands, see
    /// `App::editor_broadcast`.
    pub fn sync_lines(&self, world: &World, root: &Path, all: bool) -> Vec<String> {
        let mut out = Vec::new();
        let edits = world.object_edits.lock();
        let line = |id: i64, e: &ObjectEdit| format!("objedit {id} {:.3} {:.3} {:.3} {:.2} {}", e.moved.x, e.moved.y, e.moved.z, e.turned, e.deleted as u8);
        if all {
            for (id, e) in edits.iter() {
                out.push(line(*id, e));
            }
        } else if let (None, Some(id)) = (self.editing_added, self.selected) {
            if let Some(e) = edits.get(&id) {
                out.push(line(id, e));
            }
        }
        let added_line = |a: &Added| {
            let rel = a.sco.strip_prefix(root).unwrap_or(&a.sco).to_string_lossy().replace('\\', "/");
            let p = a.base + a.moved;
            format!("objadd {} {:.2} {:.2} {:.2} {:.1} {} {rel}", a.id, p.x, p.y, p.z, a.base_heading + a.turned, a.deleted as u8)
        };
        if all {
            out.extend(self.added.iter().map(added_line));
        } else if let Some(a) = self.editing_added.and_then(|k| self.added.get(k)) {
            out.push(added_line(a));
        }
        out
    }

    /// Change the selected object; returns what to say.
    pub fn apply(&mut self, world: &World, renderer: &omsi_render::Renderer, scene: &mut omsi_render::Scene, action: &Action) -> Option<String> {
        match action {
            Action::Copy => return self.copy(world, renderer, scene),
            Action::Variant => return self.variant(world, renderer, scene),
            _ => {}
        }
        if let Some(k) = self.editing_added {
            let a = self.added.get_mut(k)?;
            match action {
                Action::Move(d) => a.moved += *d,
                Action::Turn(t) => a.turned += *t,
                Action::Delete => a.deleted = !a.deleted,
                Action::Undo => {
                    a.moved = DVec3::ZERO;
                    a.turned = 0.0;
                    a.deleted = false;
                }
                _ => return None,
            }
            self.place_added(k, world, renderer, scene);
            return Some(self.describe(world));
        }
        let id = self.selected?;
        let tile = world.edit_objects.lock().get(&id).map(|o| o.tile)?;
        let mut e = world.object_edits.lock().get(&id).copied().unwrap_or_default();
        match action {
            Action::Move(d) => e.moved += *d,
            Action::Turn(t) => e.turned += *t,
            Action::Delete => e.deleted = !e.deleted,
            Action::Undo => e = ObjectEdit::default(),
            _ => return None,
        }
        self.tiles.insert(id, tile);
        world.apply_object_edit(renderer, scene, id, e);
        Some(self.describe(world))
    }

    /// A copy of the selected object beside it, which the keys then move.
    fn copy(&mut self, world: &World, renderer: &omsi_render::Renderer, scene: &mut omsi_render::Scene) -> Option<String> {
        // a copy of a copy is a copy of its template
        let (template, sco, base, heading, tile) = match self.editing_added.and_then(|k| self.added.get(k)) {
            Some(a) => (a.template, a.sco.clone(), a.base + a.moved, a.base_heading + a.turned, a.tile),
            None => {
                let id = self.selected?;
                let objects = world.edit_objects.lock();
                let o = objects.get(&id)?;
                let e = world.object_edits.lock().get(&id).copied().unwrap_or_default();
                let f = o.xf.transform_vector3(Vec3::Y);
                let h = (f.x as f64).atan2(f.y as f64).to_degrees() + e.turned;
                (id, o.sco.clone(), o.pos + e.moved, h, o.tile)
            }
        };
        let max_id = world.edit_objects.lock().keys().copied().chain(self.added.iter().map(|a| a.id)).max().unwrap_or(0);
        let right = DVec3::new(heading.to_radians().cos(), -heading.to_radians().sin(), 0.0);
        self.added.push(Added { template, tile, id: max_id + 1, sco, base: base + right * 2.0, base_heading: heading, moved: DVec3::ZERO, turned: 0.0, deleted: false, gpu: None });
        let k = self.added.len() - 1;
        self.editing_added = Some(k);
        self.tiles.insert(template, tile);
        self.place_added(k, world, renderer, scene);
        Some(self.describe(world))
    }

    /// The copy being edited takes the next object type of its folder.
    fn variant(&mut self, world: &World, renderer: &omsi_render::Renderer, scene: &mut omsi_render::Scene) -> Option<String> {
        let k = self.editing_added?;
        let cur = self.added[k].sco.clone();
        let dir = cur.parent()?;
        let mut all: Vec<PathBuf> = omsi_cfg::vfs::list_dir(dir)?.into_iter().map(|(n, _)| dir.join(n)).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sco"))).collect();
        all.sort();
        let i = all.iter().position(|p| p == &cur).map(|i| (i + 1) % all.len()).unwrap_or(0);
        self.added[k].sco = all.get(i)?.clone();
        self.place_added(k, world, renderer, scene);
        Some(self.describe(world))
    }

    /// Draw copy `k` where it is now.
    fn place_added(&mut self, k: usize, world: &World, renderer: &omsi_render::Renderer, scene: &mut omsi_render::Scene) {
        let a = &mut self.added[k];
        if let Some(g) = a.gpu.take() {
            world.remove_helper_object(renderer, scene, g);
        }
        if !a.deleted {
            a.gpu = world.add_helper_object(renderer, scene, &a.sco.to_string_lossy(), a.base + a.moved, a.base_heading + a.turned, &[]);
        }
    }

    /// The selected object and what has been done to it.
    pub fn describe(&self, world: &World) -> String {
        if let Some(a) = self.editing_added.and_then(|k| self.added.get(k)) {
            let name = a.sco.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            return if a.deleted { format!("New object {name}: taken away (Delete brings it back)") } else { format!("New object {} {name}: V for the next of its folder, C copies it again", a.id) };
        }
        let Some(id) = self.selected else { return "Object editor: Enter picks the object in the middle of the view".into() };
        let name = world
            .edit_objects
            .lock()
            .get(&id)
            .and_then(|o| o.sco.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default();
        let e = world.object_edits.lock().get(&id).copied().unwrap_or_default();
        if e.deleted {
            format!("Object {id} {name}: deleted (Delete brings it back)")
        } else if e == ObjectEdit::default() {
            omsi_ui::i18n::said(omsi_ui::tr("Object {id} {name}").replace("{id}", &id.to_string()).replace("{name}", &name), format!("Object {id} {name}"))
        } else {
            format!("Object {id} {name}: moved {:+.2} / {:+.2} / {:+.2} m, turned {:+.1}°", e.moved.x, e.moved.y, e.moved.z, e.turned)
        }
    }

    /// Where the middle of the view meets the ground (within 400 m).
    pub fn aim(world: &World, eye: DVec3, forward: Vec3) -> Option<DVec3> {
        let f = forward.as_dvec3().normalize_or_zero();
        let mut t = 0.5;
        while t < 400.0 {
            let p = eye + f * t;
            if world.ground_terrain(p.x, p.y).is_some_and(|g| p.z <= g) {
                return Some(p);
            }
            t += if t < 50.0 { 0.25 } else { 1.0 };
        }
        None
    }

    /// The brush's radius now.
    pub fn brush(&self) -> f64 {
        if self.brush > 0.0 { self.brush } else { BRUSH.0 }
    }

    /// Shape the ground about `at` (raise by `Ground`'s metres, or flatten to the height at
    /// `at`); `Brush` resizes the brush. Returns what to say and the tiles to read again.
    pub fn ground(&mut self, world: &World, at: Option<DVec3>, action: &Action) -> (String, Vec<(i32, i32)>) {
        if let Action::Brush(f) = action {
            self.brush = (self.brush() * f).clamp(BRUSH.1, BRUSH.2);
            return (format!("Ground brush: {:.1} m", self.brush), Vec::new());
        }
        let Some(at) = at else { return ("Point the view at the ground".into(), Vec::new()) };
        let r = self.brush();
        let size = omsi_map::tile_size();
        let target = world.ground_terrain(at.x, at.y).unwrap_or(at.z);
        let mut changed = Vec::new();
        let mut edits = world.terrain_edits.lock();
        let (tx0, tx1) = (((at.x - r) / size).floor() as i32, ((at.x + r) / size).floor() as i32);
        let (ty0, ty1) = (((at.y - r) / size).floor() as i32, ((at.y + r) / size).floor() as i32);
        for tx in tx0..=tx1 {
            for ty in ty0..=ty1 {
                if !edits.contains_key(&(tx, ty)) {
                    let Some(src) = world.tile_source(tx, ty) else { continue };
                    let t = omsi_map::Terrain::load(&crate::tile_companion(&src, ".terrain")).unwrap_or_else(|_| omsi_map::Terrain::flat());
                    edits.insert((tx, ty), t);
                }
                let t = edits.get_mut(&(tx, ty)).unwrap();
                if shape(t, (tx as f64 * size, ty as f64 * size), at, r, action, target) {
                    changed.push((tx, ty));
                }
            }
        }
        let what = match action {
            Action::Ground(d) if *d > 0.0 => format!("Ground raised {:.2} m", d),
            Action::Ground(d) => format!("Ground lowered {:.2} m", -d),
            _ => format!("Ground flattened to {:.2} m", target),
        };
        (format!("{what} (brush {:.1} m, {} tile(s)) - Ctrl+S saves", r, changed.len()), changed)
    }

    /// Write every tile with edits as a copy under `content` (the map's own folder there),
    /// from the file the game reads it from. Returns the files written.
    pub fn save(&self, world: &World, map_rel: &str, content: &Path, original: &Path) -> Result<Vec<PathBuf>, String> {
        let edits = world.object_edits.lock().clone();
        let mut by_tile: HashMap<(i32, i32), HashMap<i64, ObjectEdit>> = HashMap::new();
        for (id, e) in edits {
            let Some(tile) = self.tiles.get(&id) else { continue };
            by_tile.entry(*tile).or_default().insert(id, e);
        }
        let mut copies_by_tile: HashMap<(i32, i32), Vec<NewRecord>> = HashMap::new();
        for a in self.added.iter().filter(|a| !a.deleted) {
            let file = a.sco.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let offset = a.base + a.moved - world.edit_objects.lock().get(&a.template).map(|o| o.pos).unwrap_or(a.base);
            copies_by_tile.entry(a.tile).or_default().push(NewRecord { template: a.template, id: a.id, file, moved: offset, turned: a.base_heading + a.turned - template_heading(world, a.template).unwrap_or(a.base_heading) });
            by_tile.entry(a.tile).or_default();
        }
        let map_dir = Path::new(map_rel).parent().unwrap_or(Path::new(""));
        let mut written = Vec::new();
        for ((tx, ty), edits) in by_tile {
            let src = world.tile_source(tx, ty).ok_or_else(|| omsi_ui::tr("tile ({tx}, {ty}) is not in the map").replace("{tx}", &tx.to_string()).replace("{ty}", &ty.to_string()))?;
            // (a LAN host's map is sealed: never written out in plain form, edited or not)
            if omsi_cfg::vfs::is_sealed(&src) {
                return Err(omsi_ui::tr("{file} is the LAN host's map: not written").replace("{file}", &src.display().to_string()));
            }
            let name = src.file_name().ok_or_else(|| omsi_ui::tr("tile without a name").into_owned())?.to_owned();
            let out = content.join(map_dir).join(&name);
            // never into the installation itself
            if let (Ok(o), Ok(r)) = (out.parent().map(|p| p.to_path_buf()).unwrap_or_default().canonicalize().or_else(|_| Ok::<_, std::io::Error>(out.clone())), original.canonicalize()) {
                if o.starts_with(&r) {
                    return Err(omsi_ui::tr("{file} lies in the original installation: not written").replace("{file}", &out.display().to_string()));
                }
            }
            let bytes = omsi_cfg::vfs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
            let (text, enc) = decode(&bytes);
            let (new_text, n) = rewrite_tile(&text, &edits);
            let (new_text, c) = add_copies(&new_text, copies_by_tile.get(&(tx, ty)).map(|v| v.as_slice()).unwrap_or(&[]));
            let n = n + c;
            if n == 0 {
                return Err(omsi_ui::tr("the objects edited were not found in {file}").replace("{file}", &src.display().to_string()));
            }
            std::fs::create_dir_all(out.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
            let data = encode(&new_text, enc);
            std::fs::write(&out, data).map_err(|e| format!("{}: {e}", out.display()))?;
            log::info!("object editor: {} objects of tile ({tx}, {ty}) changed, written to {}", n, out.display());
            written.push(out);
        }
        // the ground the brush shaped: each tile's .map.terrain
        let terrains = world.terrain_edits.lock().clone();
        for ((tx, ty), t) in terrains {
            let src = world.tile_source(tx, ty).ok_or_else(|| omsi_ui::tr("tile ({tx}, {ty}) is not in the map").replace("{tx}", &tx.to_string()).replace("{ty}", &ty.to_string()))?;
            // (a LAN host's map is sealed: never written out in plain form, edited or not)
            if omsi_cfg::vfs::is_sealed(&src) {
                return Err(omsi_ui::tr("{file} is the LAN host's map: not written").replace("{file}", &src.display().to_string()));
            }
            let name = format!("{}.terrain", src.file_name().ok_or_else(|| omsi_ui::tr("tile without a name").into_owned())?.to_string_lossy());
            let out = content.join(map_dir).join(&name);
            if let (Some(dir), Ok(r)) = (out.parent(), original.canonicalize()) {
                if dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()).starts_with(&r) {
                    return Err(omsi_ui::tr("{file} lies in the original installation: not written").replace("{file}", &out.display().to_string()));
                }
            }
            std::fs::create_dir_all(out.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
            std::fs::write(&out, t.to_bytes()).map_err(|e| format!("{}: {e}", out.display()))?;
            log::info!("map editor: the ground of tile ({tx}, {ty}) written to {}", out.display());
            written.push(out);
        }
        Ok(written)
    }
}

/// Shape one tile's ground (its origin `o`) about `at` within `r`: raise by `Ground`'s
/// metres, or bring toward `target` (`Flatten`), weighted smoothly to nothing at the rim.
/// True when a point moved.
fn shape(t: &mut omsi_map::Terrain, o: (f64, f64), at: DVec3, r: f64, action: &Action, target: f64) -> bool {
    let n = t.samples();
    let step = omsi_map::tile_size() / t.cells as f64;
    let mut any = false;
    for iy in 0..n {
        for ix in 0..n {
            let (x, y) = (o.0 + ix as f64 * step, o.1 + iy as f64 * step);
            let d = ((x - at.x).powi(2) + (y - at.y).powi(2)).sqrt();
            if d >= r {
                continue;
            }
            let w = (1.0 - (d / r).powi(2)).powi(2);
            let h = &mut t.heights[iy * n + ix];
            let new = match action {
                Action::Ground(m) => *h as f64 + m * w,
                _ => *h as f64 + (target - *h as f64) * w.min(1.0),
            };
            if (new - *h as f64).abs() > 1e-5 {
                *h = new as f32;
                any = true;
            }
        }
    }
    any
}

/// The heading of map object `id` as it stands now (its edits included).
fn template_heading(world: &World, id: i64) -> Option<f64> {
    let objects = world.edit_objects.lock();
    let o = objects.get(&id)?;
    let e = world.object_edits.lock().get(&id).copied().unwrap_or_default();
    let f = o.xf.transform_vector3(Vec3::Y);
    Some((f.x as f64).atan2(f.y as f64).to_degrees() + e.turned)
}

/// A new object for the tile file: a copy of `template`'s record with its own id, another
/// file name of the same folder (or the same), moved and turned from the template.
pub struct NewRecord {
    pub template: i64,
    pub id: i64,
    pub file: String,
    pub moved: DVec3,
    pub turned: f64,
}

/// The tile file with `copies` added, each record after its template's. Returns the text
/// and how many were added.
pub fn add_copies(text: &str, copies: &[NewRecord]) -> (String, usize) {
    if copies.is_empty() {
        return (text.to_string(), 0);
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let body = |l: &str| l.trim_end_matches(['\r', '\n']).to_string();
    let ending = |l: &str| l[l.trim_end_matches(['\r', '\n']).len()..].to_string();
    let mut out = String::with_capacity(text.len() + copies.len() * 200);
    let mut added = 0;
    let mut i = 0;
    while i < lines.len() {
        let is_object = body(lines[i]).trim().eq_ignore_ascii_case("[object]");
        let id = lines.get(i + 3).and_then(|l| body(l).trim().parse::<i64>().ok());
        let mine: Vec<&NewRecord> = if is_object { copies.iter().filter(|c| Some(c.template) == id).collect() } else { Vec::new() };
        if mine.is_empty() {
            out.push_str(lines[i]);
            i += 1;
            continue;
        }
        // the template's record, up to the next keyword
        let start = i;
        i += 1;
        while i < lines.len() && !body(lines[i]).trim_start().starts_with('[') {
            i += 1;
        }
        let record = &lines[start..i];
        for l in record {
            out.push_str(l);
        }
        let eol = ending(record[0]);
        // (a blank line between records, as the editor writes them)
        if !record.last().map(|l| body(l).trim().is_empty()).unwrap_or(false) {
            out.push_str(&eol);
        }
        for c in mine {
            for (k, l) in record.iter().enumerate() {
                let b = body(l);
                let new = match k {
                    // the file: the copy's name in the template's folder
                    2 => match b.rfind(['\\', '/']) {
                        Some(p) => format!("{}{}", &b[..=p], c.file),
                        None => c.file.clone(),
                    },
                    3 => c.id.to_string(),
                    4..=7 => match b.trim().parse::<f64>() {
                        Ok(v) => num(v + [c.moved.x, c.moved.y, c.moved.z, c.turned][k - 4]),
                        Err(_) => b.clone(),
                    },
                    _ => b.clone(),
                };
                out.push_str(&new);
                out.push_str(&ending(l));
            }
            if !record.last().map(|l| body(l).trim().is_empty()).unwrap_or(false) {
                out.push_str(&eol);
            }
            added += 1;
        }
    }
    (out, added)
}

/// How a tile file is written: OMSI's editor saves UTF-16 (little endian, with its byte
/// order mark); hand-made ones are ASCII or Latin-1.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Encoding {
    Utf8,
    Latin1,
    Utf16Le,
}

fn decode(bytes: &[u8]) -> (String, Encoding) {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return (String::from_utf16_lossy(&units), Encoding::Utf16Le);
    }
    match String::from_utf8(bytes.to_vec()) {
        Ok(t) => (t, Encoding::Utf8),
        Err(_) => (bytes.iter().map(|&b| b as char).collect(), Encoding::Latin1),
    }
}

fn encode(text: &str, enc: Encoding) -> Vec<u8> {
    match enc {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Latin1 => text.chars().map(|c| c as u32 as u8).collect(),
        Encoding::Utf16Le => [0xFF, 0xFE].into_iter().chain(text.encode_utf16().flat_map(|u| u.to_le_bytes())).collect(),
    }
}

/// A number as a tile file writes it.
fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// The tile file with the edits applied to its `[object]` records (by map id): a moved or
/// turned object gets its position (x, y, height over the ground) and heading changed, a
/// deleted one loses its record. Everything else stays as it was, line endings included.
/// Returns the text and how many records changed.
pub fn rewrite_tile(text: &str, edits: &HashMap<i64, ObjectEdit>) -> (String, usize) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let body = |l: &str| l.trim_end_matches(['\r', '\n']).to_string();
    let ending = |l: &str| l[l.trim_end_matches(['\r', '\n']).len()..].to_string();
    let mut out = String::with_capacity(text.len());
    let mut changed = 0;
    let mut i = 0;
    while i < lines.len() {
        let is_object = body(lines[i]).trim().eq_ignore_ascii_case("[object]");
        let id = lines.get(i + 3).and_then(|l| body(l).trim().parse::<i64>().ok());
        let edit = if is_object { id.and_then(|id| edits.get(&id)) } else { None };
        let Some(e) = edit else {
            out.push_str(lines[i]);
            i += 1;
            continue;
        };
        changed += 1;
        if e.deleted {
            // the record up to the next keyword
            i += 1;
            while i < lines.len() && !body(lines[i]).trim_start().starts_with('[') {
                i += 1;
            }
            continue;
        }
        // [object], 0, file, id, x, y, z, heading, …
        for k in 0..4 {
            out.push_str(lines[i + k]);
        }
        let deltas = [e.moved.x, e.moved.y, e.moved.z, e.turned];
        for (k, d) in deltas.iter().enumerate() {
            let Some(l) = lines.get(i + 4 + k) else { break };
            match body(l).trim().parse::<f64>() {
                Ok(v) if *d != 0.0 => {
                    out.push_str(&num(v + d));
                    out.push_str(&ending(l));
                }
                _ => out.push_str(l),
            }
        }
        i += 8;
    }
    (out, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brush_raises_the_middle_most_and_not_its_rim() {
        let mut t = omsi_map::Terrain::flat();
        let size = omsi_map::tile_size();
        let at = DVec3::new(size * 0.5, size * 0.5, 0.0);
        assert!(shape(&mut t, (0.0, 0.0), at, 12.0, &Action::Ground(1.0), 0.0));
        assert!((t.sample(at.x as f32, at.y as f32) - 1.0).abs() < 1e-4);
        assert_eq!(t.sample(at.x as f32 + 18.0, at.y as f32), 0.0);
        // flattened back to 0 where the weight is full
        shape(&mut t, (0.0, 0.0), at, 12.0, &Action::Flatten, 0.0);
        assert!(t.sample(at.x as f32, at.y as f32).abs() < 1e-4);
        assert_eq!(omsi_map::Terrain::parse(&t.to_bytes()).unwrap(), t);
    }

    const TILE: &str = "[version]\r\n4\r\n\r\n[object]\r\n0\r\nSceneryobjects\\a.sco\r\n7\r\n5\r\n6\r\n0.25\r\n90\r\n0\r\n0\r\n0\r\n\r\nObject Nr. 1\r\n[object]\r\n0\r\nSceneryobjects\\b.sco\r\n8\r\n1\r\n2\r\n0\r\n0\r\n0\r\n0\r\n2\r\nHalt\r\nx\r\n\r\n[spline]\r\n0\r\n";

    #[test]
    fn a_moved_object_changes_its_lines_and_a_deleted_one_goes() {
        let mut edits = HashMap::new();
        edits.insert(7, ObjectEdit { moved: DVec3::new(1.5, -1.0, 0.0), turned: 12.5, deleted: false });
        edits.insert(8, ObjectEdit { deleted: true, ..Default::default() });
        let (out, n) = rewrite_tile(TILE, &edits);
        assert_eq!(n, 2);
        assert!(out.contains("[object]\r\n0\r\nSceneryobjects\\a.sco\r\n7\r\n6.5\r\n5\r\n0.25\r\n102.5\r\n0\r\n"), "{out:?}");
        assert!(!out.contains("b.sco") && !out.contains("Halt"), "{out:?}");
        assert!(out.ends_with("[spline]\r\n0\r\n"));
        // nothing to do: the same text
        assert_eq!(rewrite_tile(TILE, &HashMap::new()), (TILE.to_string(), 0));
    }

    #[test]
    fn a_utf16_tile_is_written_back_as_utf16() {
        let bytes = encode(TILE, Encoding::Utf16Le);
        assert_eq!(&bytes[..4], &[0xFF, 0xFE, b'[', 0]);
        let (text, enc) = decode(&bytes);
        assert_eq!((text.as_str(), enc), (TILE, Encoding::Utf16Le));
    }
}

#[cfg(test)]
mod copy_tests {
    #[test]
    fn a_copy_follows_its_template_with_its_own_id() {
        let text = "[object]\r\n0\r\nSceneryobjects\\A\\post.sco\r\n17\r\n10\r\n20\r\n0\r\n90\r\n\r\n[object]\r\n0\r\nx.sco\r\n18\r\n1\r\n1\r\n0\r\n0\r\n";
        let (out, n) = super::add_copies(text, &[super::NewRecord { template: 17, id: 99, file: "lamp.sco".into(), moved: glam::DVec3::new(2.0, 0.0, 0.0), turned: 10.0 }]);
        assert_eq!(n, 1);
        assert!(out.contains("Sceneryobjects\\A\\lamp.sco\r\n99\r\n12\r\n20\r\n0\r\n100\r\n"), "{out}");
        assert!(out.contains("[object]\r\n0\r\nx.sco\r\n18"));
    }
}
