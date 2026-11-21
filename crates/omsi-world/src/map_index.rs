//! Placement of the map's attached objects and the map-wide index.
//!
//! * `[attachObj]` hangs an object on one of the `[new_attachment]` points of its parent
//!   (traffic lights on a whip beam, line plates on a bus stop sign, the next house of a
//!   terrace) - none of them was placed before, which is most of what "objects missing" and
//!   "a sign floating in the air" were on the stock maps.
//! * `[splineAttachement]` puts a row of objects along a spline (street lamps, parking
//!   bays, catenary masts) and the splines after it in its tile;
//!   `[splineAttachement_repeater]` continues the row where the chain enters another tile,
//!   and the position there depends on the length of the chain from its start - which may
//!   lie in other tiles. [`MapIndex`] knows every spline of the map for that walk, and every
//!   object for entry points and bus stops of tiles that are not loaded.
//! * The game's tile streamer (omsi-app's `tiles::Streamer`) loads the tiles around the
//!   camera and the player's bus with what is here.

use glam::{DVec2, DVec3, Mat4, Vec3};
use hashbrown::{HashMap, HashSet};
use omsi_geometry::SplineCurve;
use omsi_map::{MapSpline, SplineAttachment, Tile, tile_size};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// A spline of the map as far as a repeater's walk along its chain needs it.
#[derive(Debug, Clone, Copy)]
pub struct IndexedSpline {
    pub length: f64,
    /// The chain distance stored in the last `[spline]` field (tile version 11+).
    /// `None` for older tiles, where that field is not present.
    pub map_chain_offset: Option<f64>,
    pub prev: i64,
    pub next: i64,
}

impl IndexedSpline {
    fn from_map(spline: &MapSpline) -> Self {
        Self {
            length: spline.length,
            map_chain_offset: spline.map_chain_offset,
            prev: spline.prev_id,
            next: spline.next_id,
        }
    }
}

/// What the map holds outside the loaded tiles: its splines (lengths, chain offsets and links),
/// the spline every `[splineAttachement]` row starts on, and where every object stands.
#[derive(Default)]
pub struct MapIndex {
    pub splines: HashMap<i64, IndexedSpline>,
    /// (tile index in global.cfg, attachment id) → the `[splineAttachement]` rows: (spline id,
    /// where the row's start lies on that spline - its own start distance less the spline's
    /// chain offset, negative when the row begins before the spline, the distance between two
    /// objects). The index's own check of the chain model against the object counts the editor
    /// writes into the repeaters reads it.
    pub masters: HashMap<(usize, i64), (i64, f64, f64)>,
    pub repeater_first: HashMap<(i64, i64), usize>,
    /// Object id → (tile, world position with the ground under it, rotation).
    pub objects: HashMap<i64, ((i32, i32), DVec3, [f64; 3])>,
    /// The objects whose id another tile uses as well, by (tile, id): a map joined from two
    /// maps (two towns you cannot drive between) repeats ids, and an entry point named by
    /// its id was looked for in the other town.
    pub duplicates: HashMap<((i32, i32), i64), (DVec3, [f64; 3])>,
    /// Every object and spline file the map names (as written, lower case), with the number
    /// of records naming it and one tile that does.
    pub files: HashMap<String, (usize, (i32, i32))>,
    /// Crossing ids whose light program runs: placed `[trafficlight]` objects name them, or
    /// a child of any kind names one of their lights (`names_traffic_light`), including
    /// children on other tiles.
    pub traffic_light_parents: HashSet<i64>,
    /// Tile → the world rectangle (x0, y0, x1, y1) its tile square and its splines (with
    /// room for their width) cover.
    pub covers: HashMap<(i32, i32), [f64; 4]>,
    pub tiles_read: usize,
    pub tiles_failed: usize,
    /// Object id → how many passengers get off at it, as Omsi.exe weighs a `[busstop]`'s
    /// strings (see [`stop_exit_weight`]); only objects that carry strings.
    pub stop_weights: HashMap<i64, f32>,
    /// Object id → its `[busstop]`'s (pass_enter_max, pass_enter_min) (see
    /// [`stop_enter`]); only objects that carry strings.
    pub stop_enter: HashMap<i64, (f32, f32)>,
    /// Object id → the side the stop's platform lies on (see [`stop_side`]): 0 = right of
    /// the way the map lays the road (the side a bus stopping in its own lane keeps its
    /// doors on), 1 = the other. Only objects that carry strings.
    pub stop_side: HashMap<i64, f32>,
    /// Object id → the stop's length (string 4, 30 m when not given; Omsi.exe's +0x7c,
    /// sub_620058): how far along it the waiting places and a standing bus may be.
    pub stop_length: HashMap<i64, f32>,
}

pub use crate::stop_numbers::StopNumbers;

/// How many passengers get off at a bus stop, as Omsi.exe reads the stop object's strings
/// when it sets the station up (0x620058): `pass_enter_max` (string 1, else 1) and
/// `pass_enter_min` (string 2, else 0) rounded, and `pass_exit` (string 3) rounded - or,
/// without it, the mean of the two - never below 0. A boarding passenger draws where to get
/// off among the stops ahead by these numbers (0x61baa8), so a stop with twice the number
/// takes twice the riders; the stock maps put 10 on every stop.
pub fn stop_exit_weight(strings: &[String]) -> f32 {
    stop_exit_weight_with(strings, StopNumbers::Omsi)
}

/// [`stop_exit_weight`] read the way `numbers` says (#3154).
pub fn stop_exit_weight_with(strings: &[String], numbers: StopNumbers) -> f32 {
    let (max, min) = stop_enter_with(strings, numbers);
    stop_num(strings, 3, numbers)
        .unwrap_or((min + max) / 2.0)
        .max(0.0)
}

fn stop_num(strings: &[String], i: usize, numbers: StopNumbers) -> Option<f32> {
    let v = strings
        .get(i)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|s| s.replace(',', ".").parse::<f64>().ok())
        .filter(|v| v.is_finite())?;
    Some(numbers.round(v))
}

/// A bus stop's `pass_enter_max` (string 1, else 1) and `pass_enter_min` (string 2, else
/// 0), rounded, as Omsi.exe sets the station up (0x620058): how many people wait there.
pub fn stop_enter(strings: &[String]) -> (f32, f32) {
    stop_enter_with(strings, StopNumbers::Omsi)
}

/// [`stop_enter`] read the way `numbers` says (#3154): `Omsi` rounds each string, `Exact`
/// leaves it for whoever multiplies it by the passenger density.
pub fn stop_enter_with(strings: &[String], numbers: StopNumbers) -> (f32, f32) {
    (
        stop_num(strings, 1, numbers).unwrap_or(1.0),
        stop_num(strings, 2, numbers).unwrap_or(0.0),
    )
}

/// The side a bus stop's platform lies on, as Omsi.exe reads it off the stop object's
/// *timetable data* strings: string 5 (0-based, after name and the entering/exiting
/// numbers), 0 = the right of the way the map lays the road down, 1 = the other side.
///
/// AiList vehicles that carry doors on both sides (Urumqi61's `[AI]YoungMan*`) read it as
/// `AI_Scheduled_AtStation_Side` and open only the platform's doors; without it a left-hand
/// platform is served through the traffic, and the door lamps on that side stay dark.
///
/// Anything else (an empty string, rubbish) means the map says nothing, which OMSI takes
/// as the right-hand side: 0. Stops whose strings are not timetable data at all (Spandau's
/// `bss1\*.jpg` entry-point signs carry flag 7 too) land on the same default. A value past
/// 1 (OMSI's door scripts test `= 1`, so their other branch covers everything else) is kept
/// as it stands, up to the two a script that knows the sides can tell apart.
pub fn stop_length(strings: &[String]) -> f32 {
    strings
        .get(4)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|s| s.replace(',', ".").parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v as f32)
        .unwrap_or(30.0)
}

pub fn stop_side(strings: &[String]) -> f32 {
    strings
        .get(5)
        .map(|s| s.trim())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 2.0) as f32)
        .unwrap_or(0.0)
}

/// Does a child of a crossing name one of its lights? OMSI's `RefreshAmpelParenting`
/// (0x77d460) switches a crossing's light program on for every object whose `[varparent]`
/// is that crossing and whose first string is a light index (`StrToInt` >= 0) - whatever
/// kind of object it is: it never asks for `[trafficlight]`. A mod's lamp without that
/// keyword (a copy of a lamp with its own script) still runs its junction, and the lamp
/// shows that light.
pub fn names_traffic_light(strings: &[String]) -> bool {
    strings
        .first()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .is_some_and(|i| i >= 0)
}

fn traffic_light_parents(tile: &Tile, mut is_signal: impl FnMut(&str) -> bool) -> HashSet<i64> {
    light_children(tile)
        .filter_map(|(file, _, parent)| is_signal(file).then_some(parent))
        .collect()
}

/// Every child of another object in a tile: (file, strings, parent id).
fn light_children(tile: &Tile) -> impl Iterator<Item = (&str, &[String], i64)> {
    tile.objects
        .iter()
        .chain(&tile.attach_objects)
        .filter_map(|o| {
            o.var_parent
                .or(o.parent_id)
                .map(|parent| (o.file.as_str(), o.extra.as_slice(), parent))
        })
        .chain(tile.spline_attachments.iter().filter_map(|a| {
            a.var_parent
                .map(|parent| (a.file.as_str(), a.strings.as_slice(), parent))
        }))
}

/// The parents a tile's children name a light of (`names_traffic_light`), whatever the
/// children are: whether the parent is a crossing with a light program is settled once
/// the whole map is read (it may stand on another tile).
fn light_naming_parents(tile: &Tile) -> HashSet<i64> {
    light_children(tile)
        .filter_map(|(_, strings, parent)| names_traffic_light(strings).then_some(parent))
        .collect()
}

type RowParts = (
    Vec<((usize, i64), i64, f64, f64, IndexedSpline)>,
    Vec<(i64, i64, usize, f64, f64)>,
    Vec<((i32, i32), MapSpline, SplineAttachment)>,
    Vec<omsi_map::MapObject>,
);
type LightParts = (HashSet<i64>, Vec<(i64, u32)>, Vec<String>);
type SignalTypes = parking_lot::Mutex<HashMap<String, std::sync::Arc<std::sync::OnceLock<bool>>>>;

fn read_tile_part(
    gi: usize,
    tx: i32,
    ty: i32,
    path: &Path,
    chrono_dirs: &[PathBuf],
    root: &Path,
    numbers: StopNumbers,
    signal_types: &SignalTypes,
) -> Option<(MapIndex, RowParts, LightParts)> {
    let tile = read_tile(path, chrono_dirs)?;
    let mut part = MapIndex::default();
    part.traffic_light_parents = traffic_light_parents(&tile, |file| {
        let key = file.replace('/', "\\").to_ascii_lowercase();
        let cell = signal_types.lock().entry(key).or_default().clone();
        *cell.get_or_init(|| {
            let path = omsi_cfg::resolve_path(root, file);
            match omsi_scenery::SceneryObject::load(&path) {
                Ok(sco) => sco.is_traffic_light,
                Err(e) => {
                    log::warn!("reading traffic light type: {e}");
                    false
                }
            }
        })
    });
    let lights: LightParts = {
        let named = light_naming_parents(&tile);
        let mut names: Vec<String> = Vec::new();
        let mut by_name: HashMap<&str, u32> = HashMap::new();
        let mut ids = Vec::with_capacity(tile.objects.len());
        for o in &tile.objects {
            let k = *by_name.entry(o.file.as_str()).or_insert_with(|| {
                names.push(o.file.clone());
                names.len() as u32 - 1
            });
            ids.push((o.id, k));
        }
        (named, ids, names)
    };
    let mut rows: RowParts = Default::default();
    for s in tile.splines.iter().filter(|s| !s.deleted) {
        part.splines.insert(s.id, IndexedSpline::from_map(s));
    }
    for a in &tile.spline_attachments {
        let Some(s) = tile.splines.get(a.spline_index.max(0) as usize) else {
            continue;
        };
        match a.repeater {
            None => {
                rows.0.push((
                    (gi, a.id),
                    s.id,
                    a.offset[2],
                    a.interval,
                    IndexedSpline::from_map(s),
                ));
                if !s.deleted {
                    rows.2.push((
                        (tx, ty),
                        s.clone(),
                        SplineAttachment {
                            file: a.file.clone(),
                            id: a.id,
                            offset: a.offset,
                            rot: a.rot,
                            interval: a.interval,
                            range: a.range,
                            tilt: a.tilt,
                            ..Default::default()
                        },
                    ));
                }
            }
            Some((_master_tile, first)) => {
                rows.1.push((a.id, s.id, first, a.offset[2], a.interval))
            }
        }
    }
    let terrain = omsi_map::Terrain::load(&terrain_file(&tile, path)).ok();
    let origin = DVec2::new(tx as f64 * tile_size(), ty as f64 * tile_size());
    part.covers.insert((tx, ty), spline_cover(&tile, origin));
    let mut name = |f: &str| {
        if f.trim().is_empty() {
            return;
        }
        let e = part
            .files
            .entry(f.trim().replace('/', "\\").to_ascii_lowercase())
            .or_insert((0, (tx, ty)));
        e.0 += 1;
    };
    for f in tile
        .objects
        .iter()
        .map(|o| &o.file)
        .chain(tile.attach_objects.iter().map(|o| &o.file))
        .chain(tile.spline_attachments.iter().map(|a| &a.file))
        .chain(tile.splines.iter().filter(|s| !s.deleted).map(|s| &s.file))
    {
        name(f);
    }
    for o in &tile.objects {
        let ground = terrain
            .as_ref()
            .map(|t| {
                t.sample(
                    o.pos[0].clamp(0.0, tile_size()) as f32,
                    o.pos[1].clamp(0.0, tile_size()) as f32,
                ) as f64
            })
            .unwrap_or(0.0);
        part.objects.insert(
            o.id,
            (
                (tx, ty),
                DVec3::new(origin.x + o.pos[0], origin.y + o.pos[1], o.pos[2] + ground),
                o.rot,
            ),
        );
        if o.extra.len() >= 2 {
            part.stop_weights
                .insert(o.id, stop_exit_weight_with(&o.extra, numbers));
            part.stop_enter
                .insert(o.id, stop_enter_with(&o.extra, numbers));
            part.stop_side.insert(o.id, stop_side(&o.extra));
            part.stop_length.insert(o.id, stop_length(&o.extra));
        }
    }
    for a in tile
        .spline_attachments
        .iter()
        .filter(|a| a.repeater.is_none() && a.strings.len() >= 2)
    {
        part.stop_weights
            .insert(a.id, stop_exit_weight_with(&a.strings, numbers));
        part.stop_enter
            .insert(a.id, stop_enter_with(&a.strings, numbers));
        part.stop_side.insert(a.id, stop_side(&a.strings));
        part.stop_length.insert(a.id, stop_length(&a.strings));
    }
    rows.3 = tile.attach_objects;
    part.tiles_read = 1;
    Some((part, rows, lights))
}

impl MapIndex {
    /// Read every tile file of the map (with the active chrono patches), without meshes.
    /// Signal types are read only to identify installed traffic lights. The tiles are read
    /// on the worker pool; a tile that cannot be read is logged and left out. `tiles` are
    /// (index in global.cfg's `[map]` list, x, y, file): repeaters and timetable tracks name
    /// a tile by that index, which a missing tile file must not shift.
    pub fn build(
        tiles: &[(usize, i32, i32, PathBuf)],
        chrono_dirs: &[PathBuf],
        root: &Path,
    ) -> MapIndex {
        MapIndex::build_with(tiles, chrono_dirs, root, StopNumbers::Omsi)
    }

    /// [`MapIndex::build`], with the stops' passenger numbers read the way `numbers` says
    /// (#3154).
    pub fn build_with(
        tiles: &[(usize, i32, i32, PathBuf)],
        chrono_dirs: &[PathBuf],
        root: &Path,
        numbers: StopNumbers,
    ) -> MapIndex {
        let t0 = std::time::Instant::now();
        let signal_types: SignalTypes = parking_lot::Mutex::new(HashMap::new());
        let parts: Vec<Option<(MapIndex, RowParts, LightParts)>> = tiles
            .par_iter()
            .map(|&(gi, tx, ty, ref path)| {
                read_tile_part(gi, tx, ty, path, chrono_dirs, root, numbers, &signal_types)
            })
            .collect();
        let mut index = MapIndex::default();
        let mut rows: RowParts = Default::default();
        // A stop's distance counts from the chain's start, often beyond the length of
        // its own segment. All predecessors must be known before indexing row poses,
        // including those on other tiles in maps without an authored chain offset.
        let mut parts = parts;
        for (part, _, _) in parts.iter_mut().flatten() {
            index.splines.extend(std::mem::take(&mut part.splines));
        }
        parts.par_iter_mut().flatten().for_each(|(part, rows, _)| {
            for (tile, spline, attachment) in std::mem::take(&mut rows.2) {
                let origin = DVec2::new(tile.0 as f64 * tile_size(), tile.1 as f64 * tile_size());
                let Some(first) = row_start(&attachment, &spline, Some(&index)).and_then(|start| {
                    place_on(&attachment, &spline, origin, Some(&index), start)
                        .into_iter()
                        .next()
                }) else {
                    continue;
                };
                part.objects.entry(attachment.id).or_insert((
                    tile,
                    first.pose.pos,
                    [first.pose.heading(), 0.0, 0.0],
                ));
            }
            // an object hung on another (`[attachObj]`: the stops of Ahlheim and many
            // other maps hang on their shelters): at its parent's place - a few metres
            // off at most, and the placed object gives the exact place once its tile is
            // loaded. They were in no index at all: a duty's stop beyond the loaded tiles
            // had no place, never showed on the map and was never reached (#1014, #975).
            for o in std::mem::take(&mut rows.3) {
                let Some(parent) = o.parent_id.and_then(|id| part.objects.get(&id).copied()) else {
                    continue;
                };
                part.objects.entry(o.id).or_insert((
                    parent.0,
                    parent.1,
                    [parent.2[0] + o.rot[0], 0.0, 0.0],
                ));
                if o.extra.len() >= 2 {
                    part.stop_weights
                        .insert(o.id, stop_exit_weight_with(&o.extra, numbers));
                    part.stop_enter
                        .insert(o.id, stop_enter_with(&o.extra, numbers));
                    part.stop_side.insert(o.id, stop_side(&o.extra));
                    part.stop_length.insert(o.id, stop_length(&o.extra));
                }
            }
        });
        let named: HashSet<i64> = parts
            .iter()
            .flatten()
            .flat_map(|p| p.2.0.iter().copied())
            .collect();
        let mut programs: HashMap<String, bool> = HashMap::new();
        for (_, ids, names) in parts.iter().flatten().map(|p| &p.2) {
            for &(id, k) in ids {
                if !named.contains(&id) || index.traffic_light_parents.contains(&id) {
                    continue;
                }
                let file = &names[k as usize];
                let key = file.replace('/', "\\").to_ascii_lowercase();
                let has = *programs.entry(key).or_insert_with(|| {
                    omsi_scenery::SceneryObject::load(&omsi_cfg::resolve_path(root, file))
                        .map(|sco| !sco.traffic_lights.is_empty())
                        .unwrap_or(false)
                });
                if has {
                    index.traffic_light_parents.insert(id);
                }
            }
        }
        for p in parts {
            match p {
                Some((p, (r, q, _, _), _)) => {
                    for (id, v) in p.objects {
                        if let Some(prev) = index.objects.get(&id).filter(|prev| prev.0 != v.0) {
                            index.duplicates.insert((prev.0, id), (prev.1, prev.2));
                            index.duplicates.insert((v.0, id), (v.1, v.2));
                        }
                        index.objects.insert(id, v);
                    }
                    index.covers.extend(p.covers);
                    index.stop_weights.extend(p.stop_weights);
                    index.stop_enter.extend(p.stop_enter);
                    index.stop_side.extend(p.stop_side);
                    index.stop_length.extend(p.stop_length);
                    index.traffic_light_parents.extend(p.traffic_light_parents);
                    for (f, (n, t)) in p.files {
                        index.files.entry(f).or_insert((0, t)).0 += n;
                    }
                    index.tiles_read += 1;
                    rows.0.extend(r);
                    rows.1.extend(q);
                }
                None => index.tiles_failed += 1,
            }
        }
        for (key, spline, d, interval, local) in rows.0 {
            let offset = chain_offset_from(&index, spline, local);
            index.masters.insert(key, (spline, d - offset, interval));
        }
        // how well the chain model matches the editor: a repeater names the first object of
        // the row that lies on its spline, and every record lays the row out from its own copy
        // of it (see [`row_start`])
        let (mut checked, mut agree) = (0usize, 0usize);
        for (id, spline, first, d, interval) in &rows.1 {
            index.repeater_first.insert((*id, *spline), *first);
            if *interval <= 0.0 {
                continue;
            }
            let Some(local) = index.splines.get(spline) else {
                continue;
            };
            let d0 = d - chain_offset_from(&index, *spline, *local);
            let ours = first_after(d0, 0.0, *interval).map(|(_, j)| j).unwrap_or(0);
            index
                .repeater_first
                .insert((*id, *spline), ours.min(*first));
            checked += 1;
            agree += (ours == *first) as usize;
            if ours != *first && omsi_cfg::flags::OMSI_DEBUG_REPEATERS.is_set() {
                log::info!(
                    "spline attachment {id}: the map counts object {first} first on spline {spline}, its own copy says {ours} (the row starts {d0:.2} m into the spline, objects every {interval} m)"
                );
            }
        }
        log::info!(
            "map index: {} tiles read ({} unreadable), {} splines, {} objects, {} attachment rows ({} repeaters, {agree} of the {checked} on a known chain start where the map says), {} object and spline files in {:.2} s",
            index.tiles_read,
            index.tiles_failed,
            index.splines.len(),
            index.objects.len(),
            index.masters.len(),
            rows.1.len(),
            index.files.len(),
            t0.elapsed().as_secs_f64()
        );
        index
    }

    /// The files the map names that this installation (with its content folder and archives)
    /// does not have, grouped by the add-on folder they belong to (the folder under
    /// `Sceneryobjects` / `Splines`): (folder, [(file, records, a tile)]).
    pub fn missing_files(&self, root: &Path) -> Vec<(String, Vec<(String, usize, (i32, i32))>)> {
        let named: Vec<(&String, &(usize, (i32, i32)))> = self.files.iter().collect();
        let mut missing: Vec<(String, usize, (i32, i32))> = named
            .into_par_iter()
            .filter(|(f, _)| !omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(root, f)))
            .map(|(f, (n, t))| (f.clone(), *n, *t))
            .collect();
        missing.sort();
        let mut groups: Vec<(String, Vec<(String, usize, (i32, i32))>)> = Vec::new();
        for m in missing {
            let folder = m.0.split('\\').take(2).collect::<Vec<_>>().join("\\");
            match groups.last_mut() {
                Some((g, list)) if *g == folder => list.push(m),
                _ => groups.push((folder, vec![m])),
            }
        }
        groups
    }
}

/// Room for a spline's width on either side of its centre line.
const SPLINE_HALF_WIDTH: f64 = 50.0;

/// The world rectangle a tile's square and its splines cover. A damaged length is bounded,
/// so one bad record cannot make a tile a neighbour of the whole map.
fn spline_cover(tile: &Tile, origin: DVec2) -> [f64; 4] {
    let ts = tile_size();
    let mut c = [origin.x, origin.y, origin.x + ts, origin.y + ts];
    for s in tile
        .splines
        .iter()
        .filter(|s| !s.deleted && !s.file.trim().is_empty())
    {
        let curve = SplineCurve::from_map(s, origin);
        let len = if s.length.is_finite() {
            s.length.clamp(0.0, 5000.0)
        } else {
            0.0
        };
        let steps = (len / 10.0).ceil().max(1.0) as usize;
        for i in 0..=steps {
            let p = curve.point_at(len * i as f64 / steps as f64);
            if !(p.x.is_finite() && p.y.is_finite()) {
                continue;
            }
            c = [
                c[0].min(p.x - SPLINE_HALF_WIDTH),
                c[1].min(p.y - SPLINE_HALF_WIDTH),
                c[2].max(p.x + SPLINE_HALF_WIDTH),
                c[3].max(p.y + SPLINE_HALF_WIDTH),
            ];
        }
    }
    c
}

/// A file that belongs to a tile (`.terrain`, …): beside the tile file, or - for a tile the
/// object editor saved as a copy in the content folder, which has only the tile itself -
/// in the same map folder under the other content roots.
pub fn tile_companion(path: &Path, ext: &str) -> PathBuf {
    // a copy in a content root before the installation's (the editor's ground, a mod's)
    if let (Some(dir), Some(name)) = (path.parent(), path.file_name()) {
        let p = omsi_cfg::resolve_path(dir, &format!("{}{}", name.to_string_lossy(), ext));
        if omsi_cfg::vfs::exists(&p) {
            return p;
        }
    }
    let direct = PathBuf::from(format!("{}{}", path.display(), ext));
    if omsi_cfg::vfs::exists(&direct) {
        return direct;
    }
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return direct;
    };
    let name = format!("{}{}", name.to_string_lossy(), ext);
    omsi_cfg::mirrored_dirs(dir)
        .into_iter()
        .map(|d| omsi_cfg::resolve_path(&d, &name))
        .find(|p| omsi_cfg::vfs::exists(p))
        .unwrap_or(direct)
}

/// A tile file with the chrono folders active on the sim date applied; None (logged) when
/// the file cannot be read.
pub fn read_tile(path: &Path, chrono_dirs: &[PathBuf]) -> Option<Tile> {
    let mut tile = match Tile::load(path) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("{e}");
            return None;
        }
    };
    chain_linked_splines(&mut tile);
    if let Some(name) = path.file_name() {
        for c in chrono_dirs {
            let p = omsi_cfg::resolve_path(c, &name.to_string_lossy());
            if !omsi_cfg::vfs::is_file(&p) {
                continue;
            }
            match Tile::load(&p) {
                Ok(mut patch) => {
                    // (a chrono patch of an old map links its splines as the tile does)
                    chain_linked_splines(&mut patch);
                    // (a chrono event that reshapes the ground - a cutting, a tunnel's
                    // portal, a lake let down for a new road - saves the tile's terrain and
                    // water with its patch: read from the map's own files, the old ground
                    // filled the new tunnel and the old water stood over the new road,
                    // #923, #925)
                    if patch.has_terrain && omsi_cfg::vfs::is_file(&tile_companion(&p, ".terrain"))
                    {
                        tile.terrain_from = Some(p.clone());
                    }
                    if patch.has_water && omsi_cfg::vfs::is_file(&tile_companion(&p, ".water")) {
                        tile.water_from = Some(p.clone());
                        tile.has_water = true;
                    }
                    let unmatched = tile.apply_chrono(&patch);
                    if unmatched > 0 {
                        log::debug!(
                            "chrono {}: {unmatched} selections name nothing in the tile",
                            p.display()
                        );
                    }
                }
                Err(e) => log::warn!("{e}"),
            }
        }
        // (after the chrono patches: they are written in the tile's own measure)
        if let Some((_, ty)) = omsi_map::tile_index_of(&name.to_string_lossy()) {
            tile.fit_to_world_grid(ty);
        }
    }
    Some(tile)
}

/// Where the linked splines of a tile older than version 11 lie. Such a tile links a spline
/// to the one written before it (`[spline]`'s fourth line: 0, or -1 for none) and leaves its
/// place out: x, height, y and heading are all 0, and the spline goes on from the end of the
/// one before, at its height and in its direction. Read where they are written, every linked
/// spline of a map from OMSI 1 lay in the south-west corner of its tile, heading north -
/// Great Grundorf 2 is built that way (3326 of its 4558 splines), and most of its roads were
/// missing from the map and from the navigator. Newer tiles name both neighbours and keep
/// every spline's own place.
fn chain_linked_splines(tile: &mut Tile) {
    for i in 1..tile.splines.len() {
        let (before, after) = tile.splines.split_at_mut(i);
        let (prev, s) = (&before[i - 1], &mut after[0]);
        if !s.linked {
            continue;
        }
        let c = SplineCurve::from_map(prev, DVec2::ZERO);
        let end = c.end_point();
        s.pos = [end.x, end.y, end.z];
        s.heading = c.heading_at(c.length).rem_euclid(360.0);
    }
}

/// The `.terrain` file of a tile read by [`read_tile`] from `path`: an active chrono
/// patch's own where it brings one (see `Tile::terrain_from`).
pub fn terrain_file(tile: &Tile, path: &Path) -> PathBuf {
    tile_companion(tile.terrain_from.as_deref().unwrap_or(path), ".terrain")
}

/// The `.water` file of a tile read by [`read_tile`] from `path` (see `Tile::water_from`).
pub fn water_file(tile: &Tile, path: &Path) -> PathBuf {
    tile_companion(tile.water_from.as_deref().unwrap_or(path), ".water")
}

/// The transform of `[new_attachment]` point `a` in its parent's frame (x right, y forward,
/// z up). The operations act on the attached object in the order they are listed, like the
/// original's Direct3D matrices (`attach_rot_z 180` then `attach_trans` turns a traffic
/// light round and then hangs it at the end of the beam), and the angles have the sign
/// convention of the model's origin rotations.
pub fn attachment_matrix(a: &omsi_scenery::sco::Attachment) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    for (op, v) in &a.ops {
        let step = match (op.as_str(), v.as_slice()) {
            ("attach_trans", [x, y, z, ..]) => Mat4::from_translation(Vec3::new(*x, *y, *z)),
            ("attach_rot_x", [r, ..]) => Mat4::from_rotation_x(-r.to_radians()),
            ("attach_rot_y", [r, ..]) => Mat4::from_rotation_y(-r.to_radians()),
            ("attach_rot_z", [r, ..]) => Mat4::from_rotation_z(-r.to_radians()),
            _ => continue,
        };
        m = step * m;
    }
    m
}

/// A world pose: position, and the rotation of the object's frame.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    pub pos: DVec3,
    pub rot: Mat4,
}

impl Pose {
    /// The pose of an object hanging on attachment point `attach` of this pose, turned by
    /// its own heading/pitch/bank `own`, as Omsi.exe puts it there (0x79d4c4..0x79d689): the
    /// point's place turned with the parent, and for the turn the D3DX quaternion product
    /// point x own x parent - the point's rotation, then the object's own (bank, pitch,
    /// heading), then the parent's, all taken as rotations of the world axes. For the
    /// usual turns about the vertical this is the plain hierarchy; with a tilt in more than
    /// one of them it is what the original shows.
    pub fn attached(&self, attach: &Mat4, own: [f64; 3]) -> Pose {
        let local = attach.transform_point3(Vec3::ZERO);
        let pos = self.pos + self.rot.transform_vector3(local).as_dvec3();
        let (_, r, _) = attach.to_scale_rotation_translation();
        let (_, parent, _) = self.rot.to_scale_rotation_translation();
        let rot = Mat4::from_quat(
            (omsi_geometry::object_rotation_ypr(own)
                .to_scale_rotation_translation()
                .1
                * r
                * parent)
                .normalize(),
        );
        Pose { pos, rot }
    }

    /// Heading (degrees, clockwise from north) of the pose's forward axis.
    pub fn heading(&self) -> f64 {
        let f = self.rot.transform_vector3(Vec3::Y);
        (f.x as f64)
            .atan2(f.y as f64)
            .to_degrees()
            .rem_euclid(360.0)
    }
}

/// One object of a spline attachment row.
#[derive(Debug, Clone, Copy)]
pub struct RowObject {
    /// Index of the object in its row.
    pub index: usize,
    pub pose: Pose,
}

/// How far the start of spline `id` lies from the start of its chain. OMSI stores this in the
/// last `[spline]` field in tile version 11 and newer; that value is authoritative because
/// `prev`/`next` links can be stale. Older tiles do not store it, so reconstruct it by walking
/// the links (flipping direction where two splines meet end to end).
#[cfg(test)]
pub fn chain_offset(index: &MapIndex, id: i64) -> f64 {
    index
        .splines
        .get(&id)
        .map(|s| chain_offset_from(index, id, *s))
        .unwrap_or(0.0)
}

/// Start with this record, not a foreign spline sharing its id. After the first step
/// the legacy chain follows the same predecessor links and loop guards as before.
fn chain_offset_from(index: &MapIndex, id: i64, mut spline: IndexedSpline) -> f64 {
    if let Some(offset) = spline.map_chain_offset {
        return offset;
    }
    let mut cur = id;
    let mut forward = true;
    let mut acc = 0.0;
    let mut seen: Vec<i64> = vec![id];
    loop {
        let prev_id = if forward { spline.prev } else { spline.next };
        let Some(p) = index.splines.get(&prev_id) else {
            break;
        };
        if seen.contains(&prev_id) || seen.len() > 500 {
            break;
        }
        if p.next == cur {
            forward = true;
        } else if p.prev == cur {
            forward = false;
        }
        acc += p.length;
        seen.push(prev_id);
        cur = prev_id;
        spline = *p;
    }
    acc
}

/// How far past the end of a chain an object still stands at its end: the editor's sums of
/// spline lengths drift by a little (Spandau has buffer stops 0.1 and 0.5 m past theirs).
const CHAIN_END_TOLERANCE: f64 = 1.0;
const JOINT_TOLERANCE: f64 = 0.01;
const ROW_END_DRIFT: f64 = 0.25;

/// Where a row record's objects begin on its own spline.
#[derive(Debug, Clone, Copy)]
struct RowStart {
    /// Distance of the first object from the start of the spline (the objects before the
    /// start are skipped).
    s: f64,
    /// Index of that object in the row.
    j: usize,
    /// The spline runs against the row.
    backwards: bool,
    /// The row's start and this spline's start, as distances along the chain from the start
    /// of the master's spline.
    d0: f64,
    acc: f64,
}

/// The first object of a row starting at `d0` that lies after `acc` along the chain (one
/// standing right on a joint belongs to the spline before): (its distance past `acc`, its
/// index).
fn first_after(d0: f64, acc: f64, interval: f64) -> Option<(f64, usize)> {
    if interval > 0.0 {
        let j = ((acc + JOINT_TOLERANCE - d0) / interval).ceil().max(0.0) as usize;
        Some((d0 + j as f64 * interval - acc, j))
    } else if d0 > acc + JOINT_TOLERANCE {
        Some((d0 - acc, 0))
    } else {
        None
    }
}

/// Where the objects of row record `att` begin on `spline`, its own spline.
///
/// Every record of a row - the `[splineAttachement]` of the spline the row was attached to and
/// every `[splineAttachement_repeater]` that carries it on where the chain enters another tile
/// - lays the row out itself: object `j` lies `d + j * interval` along the chain from the
/// chain's start while `j * interval <= range`, the record's own copy of those numbers being
/// the distance the editor last wrote (docs/FORMATS.md). A repeater's copy is what the map's
/// own object counts agree with - it is the same row, so a tile can be drawn without the tile
/// that holds the record the row was attached on, and where a mod map's copies have drifted
/// apart (Grundorf Island 4's bridges carry interval 60 and a start that has moved, where the
/// record on the bridge's first spline says 55) the row goes where its own record says, as
/// `Omsi.exe` places it. The master's tile index the repeater stores is not consulted: it
/// names a tile of `global.cfg`'s `[map]` list that no longer holds the master when a tile was
/// added or removed after the repeater was written (32 of Berlin-Spandau's 213 repeaters).
fn row_start(
    att: &SplineAttachment,
    spline: &MapSpline,
    index: Option<&MapIndex>,
) -> Option<RowStart> {
    // The start distance counts from the start of the chain:
    // Separate towns in a merged map can reuse spline ids. The record's authored
    // offset or legacy links belong to this record; a global id may name another.
    let offset = spline
        .map_chain_offset
        .or_else(|| {
            index.map(|ix| chain_offset_from(ix, spline.id, IndexedSpline::from_map(spline)))
        })
        .unwrap_or(0.0);
    let d0 = att.offset[2] - offset;
    Some(RowStart {
        s: d0,
        j: 0,
        backwards: false,
        d0,
        acc: 0.0,
    })
}

/// The distance of a spline from the start of its own tile's chain: the `prev` links of the
/// same tile's splines, a tile's chains being cut where the spline before lies in another
/// tile. (Not the file's stored offset: Omsi.exe's records before their own spline are
/// recognised by it with the chains as the tile reads them.)
fn tile_chain_offset(splines: &[MapSpline], si: usize) -> f64 {
    let mut cur = si;
    let mut total = 0.0;
    let mut seen: Vec<i64> = vec![splines[si].id];
    loop {
        let prev = splines[cur].prev_id;
        if prev == 0 || seen.contains(&prev) || seen.len() > 500 {
            break;
        }
        let Some(pi) = splines.iter().position(|s| s.id == prev) else {
            break;
        };
        seen.push(prev);
        total += splines[pi].length.max(0.0);
        cur = pi;
    }
    total
}

/// Whether Omsi.exe refuses this record: its first object would lie before the record's own
/// spline (start distance smaller than the spline's place in its chain). Verified against
/// Omsi.exe on Berlin-Spandau, 2026-10-11: `[splineAttachement]` records laid out with start
/// -5.5 (lateral 0) and start -1.5 (lateral -6) are not drawn by it, and neither is a start
/// of +5 on a spline 27 m into its chain, while start +37.5 with lateral 0 is.
pub fn row_starts_before_its_spline(att: &SplineAttachment, splines: &[MapSpline]) -> bool {
    let si = att.spline_index.max(0) as usize;
    if si >= splines.len() || splines[si].deleted {
        return false;
    }
    att.offset[2] - tile_chain_offset(splines, si) < -1e-6
}

/// The objects of row record `att` on `spline` from `start` on, while `j * interval <= range`.
fn place_on(
    att: &SplineAttachment,
    spline: &MapSpline,
    origin: DVec2,
    index: Option<&MapIndex>,
    start: RowStart,
) -> Vec<RowObject> {
    place_upto(att, spline, origin, index, start, 0)
}

fn place_upto(
    att: &SplineAttachment,
    spline: &MapSpline,
    origin: DVec2,
    index: Option<&MapIndex>,
    start: RowStart,
    upto: usize,
) -> Vec<RowObject> {
    let curve = SplineCurve {
        half_cant_width: omsi_geometry::half_cant_width_of(&spline.file),
        ..SplineCurve::from_map(spline, origin)
    };
    let len = spline.length.max(0.0);
    // the record's own copy of the row's spacing and reach (see [`row_start`])
    let interval = att.interval.max(0.0);
    let range = att.range.max(0.0);
    let (x, h) = (att.offset[0], att.offset[1]);
    let RowStart {
        mut s,
        mut j,
        backwards,
        ..
    } = start;
    // where the chain ends with this spline (in the row's direction), an object a little
    // past the end stands at the end
    let far = if backwards {
        spline.prev_id
    } else {
        spline.next_id
    };
    let chain_ends = index
        .map(|ix| !ix.splines.contains_key(&far))
        .unwrap_or(far == 0);
    // (only an object on its own - a buffer stop - stands at the end from a little past it:
    // a row's next post due 0.75 m past a dead end was put at the end, in the carriageway
    // of Thüringer Wald's road to Haselbach, #1693)
    let single = interval <= 0.0 || range < interval;
    let last_j = if interval > 0.0 {
        ((range + JOINT_TOLERANCE) / interval).floor() as usize
    } else {
        0
    };
    let end_of = |j: usize| {
        len + if chain_ends && single {
            CHAIN_END_TOLERANCE
        } else if chain_ends && j == last_j {
            ROW_END_DRIFT
        } else {
            JOINT_TOLERANCE
        }
    };
    let from = if att.repeater.is_some() {
        JOINT_TOLERANCE
    } else {
        -1e-6
    };
    let mut out = Vec::new();
    loop {
        if interval > 0.0 && j as f64 * interval > range + JOINT_TOLERANCE {
            break;
        }
        if s > end_of(j) && j >= upto {
            break;
        }
        if s >= from {
            let along = s.clamp(0.0, len);
            // a spline run against the row: the row's distance counts from its far end and
            // the row's right is the spline's left
            let (u, side, turn) = if backwards {
                (len - along, -x, 180.0)
            } else {
                (along, x, 0.0)
            };
            let pos = curve.offset_point(u, side, h);
            let pitch = if att.tilt {
                curve.slope_at(u).atan().to_degrees()
            } else {
                0.0
            };
            // The cant lifts only within the spline type's half cant width.
            let bank = if att.tilt && side.abs() < curve.half_cant_width {
                (curve.cant_at(u) / 100.0).atan().to_degrees()
            } else {
                0.0
            };
            let mut own = omsi_geometry::map_rotation(att.rot);
            own[0] += turn;
            // Tangential objects turn in the spline's inclined frame. Adding the
            // spline's pitch/bank to the object's Euler angles instead tilts a
            // sideways railing across the road, and a reversed object downhill.
            // The half turn of a backwards chain belongs to that same local frame.
            // The row's own matrix uses bank, then pitch, then heading (Omsi.exe
            // 0x79dd20..0x79de02), unlike a plain map object's pitch-first matrix.
            // Two quarter turns otherwise laid retaining blocks across the street.
            let rot = omsi_geometry::object_rotation([curve.heading_at(u), pitch, bank])
                * omsi_geometry::object_rotation_ypr(own);
            out.push(RowObject {
                index: j,
                pose: Pose { pos, rot },
            });
        }
        if interval <= 0.0 {
            break;
        }
        s += interval;
        j += 1;
        if out.len() > 4096 {
            log::warn!(
                "spline attachment {} ({}): more than 4096 objects on one spline, row cut short",
                att.id,
                att.file
            );
            break;
        }
    }
    out
}

/// The objects of row record `att` of a tile whose splines are `splines`, whose curves start
/// at `origin` (the tile origin). Object `j` of a row lies `d + j * interval` along the chain
/// from the chain's start (see [`chain_offset`]) while `j * interval <= range`: on the
/// record's own spline and on the splines after it in the chain as long as they are in the
/// same tile - where the
/// chain enters another tile, a repeater there carries the row on (every one of the 169
/// repeaters of Berlin-Spandau with objects lies on the first spline of a new tile, and 413
/// later splines in the record's own tile have none). (index into `splines`, object).
pub fn tile_row_objects(
    att: &SplineAttachment,
    splines: &[MapSpline],
    origin: DVec2,
    index: Option<&MapIndex>,
) -> Vec<(usize, RowObject)> {
    let si = att.spline_index.max(0) as usize;
    let Some(own) = splines.get(si).filter(|s| !s.deleted) else {
        return Vec::new();
    };
    let Some(start) = row_start(att, own, index) else {
        return Vec::new();
    };
    let interval = att.interval.max(0.0);
    let last = start.d0
        + if interval > 0.0 {
            ((att.range.max(0.0) + JOINT_TOLERANCE) / interval).floor() * interval
        } else {
            0.0
        };
    let mut out: Vec<(usize, RowObject)> = place_on(att, own, origin, index, start)
        .into_iter()
        .map(|o| (si, o))
        .collect();
    let (mut cur, mut acc, mut backwards) = (si, start.acc, start.backwards);
    let mut cur_start = start;
    let mut seen = vec![si];
    loop {
        acc += splines[cur].length.max(0.0);
        if last <= acc + 1e-6 {
            break;
        }
        let c = &splines[cur];
        let next_id = if backwards { c.prev_id } else { c.next_id };
        let Some(ni) = splines.iter().position(|s| s.id == next_id && !s.deleted) else {
            let drawn = out.iter().map(|(_, o)| o.index + 1).max().unwrap_or(0);
            let last_j =
                ((att.range.max(0.0) + JOINT_TOLERANCE) / interval.max(1e-9)).floor() as usize;
            let past_end =
                cur_start.s + (drawn as f64 - cur_start.j as f64) * interval - c.length.max(0.0);
            let room = match index.map(|ix| ix.repeater_first.get(&(att.id, next_id))) {
                Some(Some(&k)) if k > drawn => CHAIN_END_TOLERANCE,
                Some(None) if drawn == last_j => ROW_END_DRIFT,
                _ => f64::NEG_INFINITY,
            };
            if interval > 0.0 && past_end <= room {
                let gap = place_upto(att, c, origin, index, cur_start, drawn + 1);
                out.extend(
                    gap.into_iter()
                        .filter(|o| o.index == drawn)
                        .map(|o| (cur, o)),
                );
            }
            break;
        };
        if seen.contains(&ni) || seen.len() > 500 {
            break;
        }
        seen.push(ni);
        let next = &splines[ni];
        if next.prev_id == c.id {
            backwards = false;
        } else if next.next_id == c.id {
            backwards = true;
        }
        let Some((s, j)) = first_after(start.d0, acc, interval) else {
            break;
        };
        cur_start = RowStart {
            s,
            j,
            backwards,
            d0: start.d0,
            acc,
        };
        out.extend(
            place_on(att, next, origin, index, cur_start)
                .into_iter()
                .map(|o| (ni, o)),
        );
        cur = ni;
    }
    out
}

#[cfg(test)]
#[path = "map_index_tests.rs"]
mod tests;
