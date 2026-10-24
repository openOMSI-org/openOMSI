//! The chosen map as the game's own city map has it - every spline's and every object's
//! `[path]`, linked to its neighbours, grouped into carriageways with their real widths, where
//! its entry points stand and where its objects are - and the road pieces the chosen trip
//! drives over it, as data for the launchers that draw it themselves (the desktop launcher's
//! page through `export`, the Swift one through `overview`).
//!
//! It reads the map the way the navigator does (`scene::navigation_map_of`, then
//! `navigator::city_roads`, through `Host::city_map`), so what a launcher shows cannot drift
//! from what the game draws on its own map. What it leaves out is what choosing a duty does
//! not need: no meshes, no textures, no traffic, no timetable beyond the chosen trip.

use glam::{DVec2, Vec3};
use hashbrown::HashMap;
use omsi_sim::traffic::Network;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the map should show. Changing `map` reads the tiles again; changing `trip` only walks
/// the timetable; changing `entry` only moves the ring.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Look {
    /// The map as the launcher names it (`maps/<name>/global.cfg`), and where that is.
    pub map: String,
    pub global: PathBuf,
    /// The date the duty starts on (the chrono folders change the map and the timetable).
    pub date: String,
    /// The trip whose route is drawn (empty: the map alone).
    pub trip: String,
    /// The entry point the player picked, -1 = automatic (nothing is ringed).
    pub entry: i32,
}

/// An entry point as the map draws it: where its marker stands, its name, and its own place
/// in `global.cfg`'s list. That place - not its place among the drawn markers - is what the
/// choice and the game count (`--entry` is the position in that list), so an entry point
/// whose position cannot be resolved leaves a gap that does not shift every marker after it
/// onto the wrong spawn.
struct Entry {
    /// Where the marker stands, in world metres.
    at: DVec2,
    /// Its name, as `global.cfg` writes it.
    name: String,
    /// Its position in `global.entry_points`.
    index: usize,
}

/// A map as the map needs it: the roads the game's own city map draws, the lanes they were
/// grouped from (the chosen trip's route runs on those), where its entry points stand and
/// where its objects do - all in world metres.
#[derive(Default)]
struct Roads {
    /// Every carriageway the map has, as the game groups them: points in world metres.
    roads: Vec<crate::host::MapRoad>,
    /// The linked network behind them (a trip's road pieces are lanes of it).
    net: Arc<Network>,
    /// (tile x, tile y, spline id, path) → the lane the timetable names.
    lanes: HashMap<(i32, i32, i64, u16), usize>,
    /// (tile x, tile y, spline id) → every lane of that spline (for a piece whose exact path
    /// is not there any more).
    splines: HashMap<(i32, i32, i64), Vec<usize>>,
    /// The world point the drawn lines are measured from: an f32 cannot hold a map's own
    /// millions of metres to the centimetre, and roads are drawn to the centimetre.
    origin: DVec2,
    /// The world rectangle the roads cover (the 1 - 99 % box: see `read_map`).
    lo: DVec2,
    hi: DVec2,
    /// The entry points that could be placed, in `global.cfg`'s order - each carrying its own
    /// place in that list (see `Entry`).
    entries: Vec<Entry>,
    /// Every placed object's world place: the trip's stops are found here.
    objects: HashMap<i64, DVec2>,
    /// The map's own layers (ground, water, paved surfaces, houses), when the game has them.
    layers: Option<Arc<omsi_map::layers::Layers>>,
}

impl Roads {
    /// The lanes a trip's road piece runs on: the very `[path]` the timetable names, else -
    /// a map whose timetable still names a spline it no longer has - every lane of that
    /// spline, so a hole in the map's own data is not a hole in the picture.
    fn routes_of(&self, piece: &omsi_launcher_lib::RoadPiece) -> Vec<usize> {
        if let Some(&i) = self.lanes.get(&(piece.tile_x, piece.tile_y, piece.spline, piece.path)) {
            return vec![i];
        }
        self.splines.get(&(piece.tile_x, piece.tile_y, piece.spline)).cloned().unwrap_or_default()
    }

    /// The trip's route as lines in world metres: its pieces' lanes in order, a spline the
    /// map lost reduced to the one lane that carries on from the piece before (not every lane
    /// of it, the opposite carriageway included), and the lanes the timetable lists side by
    /// side drawn as one with a smooth change over (`route_line`).
    fn route_lines(&self, route: &[omsi_launcher_lib::RoadPiece]) -> Vec<Vec<DVec2>> {
        let mut lanes: Vec<Vec<DVec2>> = Vec::new();
        for piece in route {
            let mut found = self.routes_of(piece);
            if found.len() > 1 {
                let from = lanes.last().and_then(|l| l.last()).copied();
                found.sort_by(|&a, &b| {
                    let d = |i: usize| match (from, self.net.lanes.get(i).and_then(|l| l.points.first())) {
                        (Some(f), Some(p)) => f.distance(p.truncate()),
                        _ => 0.0,
                    };
                    d(a).total_cmp(&d(b))
                });
                found.truncate(1);
            }
            for i in found {
                if let Some(l) = self.net.lanes.get(i) {
                    lanes.push(l.points.iter().map(|q| q.truncate()).collect());
                }
            }
        }
        crate::route_line::route_lines(&lanes)
    }
}

/// A polyline with the points the eye cannot tell apart taken out (the game's own map
/// simplification, in metres: 12 cm there, half a pixel here).
fn simplify(pts: &[Vec3], tol: f32) -> Vec<Vec3> {
    crate::host::host().simplify(pts, tol)
}

/// The map for another front end (the desktop launcher draws it itself): the roads the
/// game's city map draws, thinned to `tolerance` metres, the entry points with their own place
/// in `global.cfg`'s list, the chosen trip's route as the lanes the game drives and where its
/// stops stand - points in metres from `origin`, rounded to the decimetre, as flat
/// `[x0, y0, x1, y1, ...]` lists. `colors` are the game's city map colours (casing, road, main
/// road, route) as CSS.
pub fn export(map: &str, global: &Path, date: &str, trip: &str, tolerance: f32, surface: bool) -> serde_json::Value {
    export_with(map, global, date, trip, tolerance, surface, Detail::Map)
}

/// How much of a map `export_with` sends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Detail {
    /// The roads, the route, the stops of the trip and the entry points.
    Map,
    /// That, and every stop of the map's timetable with its place (the timetable editor's map).
    Editor,
    /// The trip's route and stops alone, measured from the same origin as the map's own, for a
    /// picture that already has the roads (the editor asks for a new route at every click).
    Route,
}

/// `export` with a choice of how much is sent: `Editor` adds every stop of the map's timetable
/// with its place, `"busstops": [{"id", "name", "at"}]`.
pub fn export_with(map: &str, global: &Path, date: &str, trip: &str, tolerance: f32, surface: bool, detail: Detail) -> serde_json::Value {
    use serde_json::json;
    let look = Look { map: map.to_string(), global: global.to_path_buf(), date: date.to_string(), trip: trip.to_string(), entry: -1 };
    let roads = roads_of(&look);
    let path = trip_of(&look);
    let o = roads.origin;
    let flat = |pts: &[DVec2]| -> Vec<f32> {
        let v: Vec<Vec3> = pts.iter().map(|q| Vec3::new((q.x - o.x) as f32, (q.y - o.y) as f32, 0.0)).collect();
        simplify(&v, tolerance).iter().flat_map(|q| [(q.x * 10.0).round() / 10.0, (q.y * 10.0).round() / 10.0]).collect()
    };
    let list: Vec<serde_json::Value> = if detail == Detail::Route { Vec::new() } else { roads.roads.iter().map(|r| json!({ "w": (r.width * 10.0).round() / 10.0, "main": r.main, "p": flat(&r.points.iter().map(|q| q.truncate()).collect::<Vec<_>>()) })).collect() };
    let route: Vec<Vec<f32>> = roads.route_lines(&path.route).iter().map(|l| flat(l)).collect();
    let rel = |q: DVec2| [((q.x - o.x) * 10.0).round() / 10.0, ((q.y - o.y) * 10.0).round() / 10.0];
    let stops: Vec<serde_json::Value> = path.stops.iter().enumerate().filter_map(|(k, id)| roads.objects.get(id).map(|q| json!({ "k": k, "at": rel(*q) }))).collect();
    let entries: Vec<serde_json::Value> = if detail == Detail::Route { Vec::new() } else { roads.entries.iter().map(|e| json!({ "index": e.index, "name": e.name, "at": rel(e.at) })).collect() };
    let css = |c: [f32; 4]| format!("rgba({},{},{},{:.3})", (c[0] * 255.0).round(), (c[1] * 255.0).round(), (c[2] * 255.0).round(), c[3]);
    let [casing, road, main, red] = crate::host::host().city_map_colors();
    // (a stop's name is the first of its busstop records; several records name one object)
    let busstops: Vec<serde_json::Value> = if detail == Detail::Editor {
        let data = omsi_timetable::TimetableData::load(global.parent().unwrap_or(Path::new("")));
        let mut seen = std::collections::HashSet::new();
        data.bus_stops.iter().filter(|b| seen.insert(b.object_id)).filter_map(|b| roads.objects.get(&b.object_id).map(|q| json!({ "id": b.object_id, "name": b.name.trim(), "at": rel(*q) }))).collect()
    } else {
        Vec::new()
    };
    json!({
        "origin": [o.x, o.y],
        "lo": rel(roads.lo), "hi": rel(roads.hi),
        "roads": list, "route": route, "stops": stops, "entries": entries, "busstops": busstops,
        "colors": [css(casing), css(road), css(main), css(red)],
        // (the package's credit always: the page shows it over the game's ground too, which
        // asks for no surface)
        "credit": global.parent().and_then(omsi_map::package::Package::open).map(|p| p.credit),
        "package": if surface && detail != Detail::Route { package_view(global, o, roads.lo, roads.hi, tolerance) } else { serde_json::Value::Null },
        "layers": if surface && detail != Detail::Route { layers_view(roads.layers.as_deref(), o) } else { serde_json::Value::Null },
    })
}

/// How far past the roads' 1 - 99 % box the package's surface is read: the page can be dragged
/// to the far corners, and a river or a shore runs on beyond the last road.
const PACKAGE_MARGIN: f64 = 2500.0;

/// The map's package (`Maps/<Map>/openomsi-realmap`, docs/MAP_PACKAGE.md) as the page draws it
/// under the roads: the filling triangles by class and the rivers, rail and borders, in metres
/// from `origin` like the roads, and the manifest's dark colours as CSS. `null` for a map
/// without a package, so the picture is what it always was.
fn package_view(global: &Path, origin: DVec2, lo: DVec2, hi: DVec2, tolerance: f32) -> serde_json::Value {
    use omsi_map::package::{self, Package};
    use serde_json::json;
    let Some(map_dir) = global.parent() else { return serde_json::Value::Null };
    let Some(pkg) = Package::open(map_dir) else { return serde_json::Value::Null };
    let m = DVec2::splat(PACKAGE_MARGIN);
    let (a, b) = (lo - m, hi + m);
    let layers = pkg.layers(Some(((a.x, a.y), (b.x, b.y))));
    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    let at = |q: (f64, f64)| [r1(q.0 - origin.x), r1(q.1 - origin.y)];
    let fills: Vec<Vec<f64>> = layers.fills.iter().map(|list| list.iter().flat_map(|t| t.iter().flat_map(|q| at(*q))).collect()).collect();
    let lines: Vec<serde_json::Value> = layers
        .lines
        .iter()
        .filter_map(|l| {
            let v: Vec<Vec3> = l.points.iter().map(|q| Vec3::new((q.0 - origin.x) as f32, (q.1 - origin.y) as f32, 0.0)).collect();
            let p: Vec<f64> = simplify(&v, tolerance).iter().flat_map(|q| [r1(q.x as f64), r1(q.y as f64)]).collect();
            (p.len() >= 4).then(|| json!({ "c": l.class, "w": l.width, "p": p }))
        })
        .collect();
    let hex = |c: package::Rgb| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let d = pkg.dark;
    log::info!("map package \"{}\": {} chunks, {} triangles, {} lines near the roads", pkg.name, pkg.chunks(), fills.iter().map(|f| f.len() / 6).sum::<usize>(), lines.len());
    json!({
        "name": pkg.name,
        "credit": pkg.credit,
        "fills": fills,
        "lines": lines,
        "colors": {
            "land": hex(d.land), "water": hex(d.water), "river": hex(d.river), "green": hex(d.green), "grass": hex(d.grass),
            "forest": hex(d.forest), "sport": hex(d.sport), "farm": hex(d.farm), "built": hex(d.built),
            "rail": hex(d.rail), "rail_edge": hex(d.rail_edge), "border": hex(d.border),
        },
    })
}

/// The route of the chosen trip (nothing when no trip is chosen or the map has a timetable
/// that does not know it).
fn trip_of(look: &Look) -> omsi_launcher_lib::TripPath {
    if look.trip.trim().is_empty() {
        return Default::default();
    }
    // a trip being made in the timetable editor: `@stops:<id>,<id>,...`, the stops it calls at
    if let Some(ids) = look.trip.trim().strip_prefix(STOPS_PREFIX) {
        let ids: Vec<i64> = ids.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        return omsi_launcher_lib::chain_path(&look.map, &look.date, &ids).unwrap_or_default();
    }
    match omsi_launcher_lib::trip_path(&look.map, &look.date, &look.trip) {
        Ok(t) => t,
        Err(e) => {
            log::debug!("trip path for {}: {e}", look.trip);
            Default::default()
        }
    }
}

/// What a trip's name starts with when it is a list of stops (see `trip_of`).
pub const STOPS_PREFIX: &str = "@stops:";

/// The map as data, for a launcher that draws it itself (the Swift one, see `apple_bridge`):
/// the same roads, route, stops and entry points this map draws, in metres from the map's
/// middle (`origin`, world metres), x east and y north, thinned to a tenth of a metre's
/// rounding and `TOLERANCE` metres of simplification. A map's tiles are read once and kept
/// for the next call on the same map and date (another trip only walks the timetable).
pub fn overview(look: Look) -> serde_json::Value {
    use serde_json::json;
    const TOLERANCE: f32 = 1.2;
    let roads = roads_of(&look);
    let trip = trip_of(&look);
    let o = roads.origin;
    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    let flat = |pts: &[DVec2]| -> Vec<f64> {
        let v: Vec<Vec3> = pts.iter().map(|q| Vec3::new((q.x - o.x) as f32, (q.y - o.y) as f32, 0.0)).collect();
        simplify(&v, TOLERANCE).iter().flat_map(|p| [r1(p.x as f64), r1(p.y as f64)]).collect()
    };
    let roads_json: Vec<serde_json::Value> = roads.roads.iter().map(|r| json!({ "w": r.width, "main": r.main, "p": flat(&r.points.iter().map(|q| q.truncate()).collect::<Vec<_>>()) })).filter(|v| v["p"].as_array().is_some_and(|a| a.len() >= 4)).collect();
    let route: Vec<Vec<f64>> = roads.route_lines(&trip.route).iter().map(|l| flat(l)).filter(|p| p.len() >= 4).collect();
    let stops: Vec<serde_json::Value> = trip.stops.iter().enumerate().filter_map(|(i, id)| roads.objects.get(id).map(|q| json!({ "x": r1(q.x - o.x), "y": r1(q.y - o.y), "i": i }))).collect();
    let entries: Vec<serde_json::Value> = roads.entries.iter().map(|e| json!({ "x": r1(e.at.x - o.x), "y": r1(e.at.y - o.y), "name": e.name, "index": e.index })).collect();
    json!({
        "map": look.map,
        "trip": look.trip,
        "origin": [o.x, o.y],
        "lo": [r1(roads.lo.x - o.x), r1(roads.lo.y - o.y)],
        "hi": [r1(roads.hi.x - o.x), r1(roads.hi.y - o.y)],
        "roads": roads_json,
        "route": route,
        "stops": stops,
        "entries": entries,
        "lanes": roads.net.lanes.len(),
        "layers": layers_view(roads.layers.as_deref(), o),
    })
}

/// The map's network and layers for the launcher's map (`omsi_navigator::MapGround`), and the
/// world point the page measures from (`export`'s `origin`).
pub fn map_network(map: &str, global: &Path, date: &str) -> (DVec2, Arc<Network>, Option<Arc<omsi_map::layers::Layers>>) {
    let look = Look { map: map.to_string(), global: global.to_path_buf(), date: date.to_string(), trip: String::new(), entry: -1 };
    let roads = roads_of(&look);
    (roads.origin, roads.net.clone(), roads.layers.clone())
}

/// The map's data (`read_map`) for `look`'s map and date, read once and kept for the next
/// call on the same map and date: the route map and the ground the game builds for the
/// launcher's map (`map_network`) share it.
fn roads_of(look: &Look) -> Arc<Roads> {
    static CACHE: std::sync::Mutex<Option<(PathBuf, String, Arc<Roads>)>> = std::sync::Mutex::new(None);
    let cached = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|(g, d, _)| *g == look.global && *d == look.date).map(|c| c.2.clone());
    if let Some(roads) = cached {
        return roads;
    }
    let roads = Arc::new(read_map(look));
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((look.global.clone(), look.date.clone(), roads.clone()));
    roads
}

/// The map's own data, read the way the game reads it for its city map: every tile's paths
/// (splines and the crossings' objects), linked, grouped into carriageways with their real
/// widths. `look.global` is the map's `global.cfg`; the tiles stand beside it and the assets
/// the map names under the installation it belongs to.
fn read_map(look: &Look) -> Roads {
    let Ok(mut global) = omsi_map::GlobalCfg::load(&look.global) else { return Roads::default() };
    // (a map from OMSI 1 lists no entry points: they are its tiles' entry objects)
    if global.entry_points.is_empty() {
        global.add_object_entry_points();
    }
    // everything that turns a tile index and a place in the tile into world metres needs
    // this first (`[worldcoordinates]` maps use another grid)
    omsi_map::configure_grid(&global);
    let Some(map_dir) = look.global.parent().map(Path::to_path_buf) else { return Roads::default() };
    let chrono = omsi_map::date_code(&look.date).map(|c| omsi_map::active_chrono_dirs(&map_dir, c)).unwrap_or_default();
    let size = omsi_map::tile_size();
    // the installation the map belongs to: what its `.sli` and `.sco` files are named from.
    // A map is a folder under a content root's `maps` (that is where the launcher's list came
    // from), so the root that holds it is the one its assets are named from; a map outside
    // every root falls back to the folder two steps up, as `<root>/maps/<name>`.
    let root = omsi_cfg::content_roots()
        .into_iter()
        .find(|r| map_dir.starts_with(r))
        .or_else(|| map_dir.parent().and_then(|p| p.parent()).map(Path::to_path_buf))
        .unwrap_or_else(|| map_dir.clone());
    let tiles: Vec<(i32, i32, PathBuf)> = global
        .tiles
        .iter()
        .map(|t| (t.x, t.y, omsi_cfg::resolve_path(&map_dir, &t.file)))
        .filter(|t| omsi_cfg::vfs::is_file(&t.2))
        .collect();
    let t0 = std::time::Instant::now();
    let crate::host::CityMap { roads, net, places } = crate::host::host().city_map(&root, &tiles, &chrono);
    // the lanes the timetable can name, and the lanes of every spline behind them
    let mut by_path = HashMap::new();
    let mut by_spline: HashMap<(i32, i32, i64), Vec<usize>> = HashMap::new();
    for (i, l) in net.lanes.iter().enumerate() {
        let Some(key) = l.key else { continue };
        by_path.insert((key.tile.0, key.tile.1, key.id, key.path), i);
        by_spline.entry((key.tile.0, key.tile.1, key.id)).or_default().push(i);
    }
    // the entry points: on the tile the record names, else at their object (`World::
    // entry_point_place` reads them the same way)
    let mut entries = Vec::new();
    for (index, ep) in global.entry_points.iter().enumerate() {
        let p = usize::try_from(ep.group)
            .ok()
            .and_then(|i| global.raw_tiles.get(i))
            .map(|t| DVec2::new(t.0 as f64 * size + ep.pos[0], t.1 as f64 * size + ep.pos[1]))
            .or_else(|| places.get(&ep.object_id).map(|q| q.truncate()));
        // the marker carries where it stands in `global.cfg`'s list, so one left out here
        // (no tile, no object) does not shift the ones after it
        if let Some(p) = p {
            entries.push(Entry { at: p, name: ep.name.clone(), index });
        }
    }
    let objects: HashMap<i64, DVec2> = places.iter().map(|(k, v)| (*k, v.truncate())).collect();
    // where the roads are, and where the picture is measured from
    let (mut min, mut max) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for r in &roads {
        for q in &r.points {
            min = min.min(q.truncate());
            max = max.max(q.truncate());
        }
    }
    if min.x == f64::MAX {
        min = DVec2::ZERO;
        max = DVec2::ZERO;
    }
    let origin = (min + max) * 0.5;
    // Where the roads really are: one spline left in a far corner (an editor's marker, a
    // piece of the next town) would shrink everything else to a dot, so the map opens on the
    // 1 - 99 % box of what the roads cover instead of the extremes (the far ones are still
    // there to be dragged to).
    let mut xs: Vec<f32> = roads.iter().flat_map(|r| r.points.iter()).map(|q| q.x as f32).collect();
    let mut ys: Vec<f32> = roads.iter().flat_map(|r| r.points.iter()).map(|q| q.y as f32).collect();
    xs.sort_by(f32::total_cmp);
    ys.sort_by(f32::total_cmp);
    let (lo, hi) = if xs.is_empty() {
        (min, max)
    } else {
        let at = |v: &Vec<f32>, t: f64| v[((v.len() - 1) as f64 * t) as usize] as f64;
        (DVec2::new(at(&xs, 0.01), at(&ys, 0.01)), DVec2::new(at(&xs, 0.99), at(&ys, 0.99)))
    };
    log::info!(
        "map: {} roads from {} lanes in {} tiles, {} objects, {} entry points, read in {:.2} s; roads cover ({:.0}, {:.0}) - ({:.0}, {:.0}), opened at the 1 - 99 % box ({:.0}, {:.0}) - ({:.0}, {:.0})",
        roads.len(),
        net.lanes.len(),
        tiles.len(),
        objects.len(),
        entries.len(),
        t0.elapsed().as_secs_f64(),
        min.x, min.y, max.x, max.y,
        lo.x, lo.y, hi.x, hi.y
    );
    let layers = crate::host::host().map_layers(&root, &map_dir, &tiles, &chrono).map(Arc::new);
    Roads { roads, net: Arc::new(net), lanes: by_path, splines: by_spline, origin, lo, hi, entries, objects, layers }
}

/// The map's layers as the page draws them under the roads, in metres from `origin` rounded to
/// the decimetre: the ground by class, the paved surfaces by class, the houses' outlines with
/// their heights, the railways and tram tracks. `null` when the game has none for the map.
fn layers_view(l: Option<&omsi_map::layers::Layers>, origin: DVec2) -> serde_json::Value {
    use serde_json::json;
    let Some(l) = l else { return serde_json::Value::Null };
    let shift = (l.origin - origin).as_vec2();
    let r1 = |v: f32| ((v as f64) * 10.0).round() / 10.0;
    let tris = |t: &[glam::Vec2]| -> Vec<f64> {
        t.chunks_exact(3)
            // (slivers under a few square decimetres are nothing at a launcher's zoom)
            .filter(|c| (c[1] - c[0]).perp_dot(c[2] - c[0]).abs() > 0.1)
            .flat_map(|c| c.iter().flat_map(|q| [r1(q.x + shift.x), r1(q.y + shift.y)]).collect::<Vec<_>>())
            .collect()
    };
    let land: Vec<Vec<f64>> = (0..omsi_map::layers::LAND_CLASSES).map(|c| tris(l.land_of(c))).collect();
    let paved: Vec<Vec<f64>> = (0..omsi_map::layers::PAVE_GREEN).map(|c| tris(l.paved_of(c))).collect();
    let houses: Vec<serde_json::Value> = l.buildings.iter().map(|b| json!({ "h": r1(b.height), "p": b.outline.iter().flat_map(|q| [r1(q.x + shift.x), r1(q.y + shift.y)]).collect::<Vec<_>>() })).collect();
    let lines: Vec<serde_json::Value> = l.lines.iter().filter(|x| x.class < omsi_map::layers::LINE_ROAD_EDGE).map(|x| json!({ "c": x.class, "w": r1(x.width), "p": x.points.iter().flat_map(|q| [r1(q.x + shift.x), r1(q.y + shift.y)]).collect::<Vec<_>>() })).collect();
    let tiles: Vec<[f64; 4]> = l.tiles.iter().map(|t| [r1(t[0] + shift.x), r1(t[1] + shift.y), r1(t[2] + shift.x), r1(t[3] + shift.y)]).collect();
    json!({ "tiles": tiles, "land": land, "paved": paved, "houses": houses, "lines": lines })
}

#[cfg(test)]
mod package_tests {
    use super::*;

    /// With a real map that carries a package (`OMSI_TEST_PACKAGE_MAP` = the path of its `global.cfg`): the
    /// export holds the surface, in metres from the origin, and says how big it is.
    #[test]
    #[ignore = "needs a map with a package: OMSI_TEST_PACKAGE_MAP=<path of its global.cfg>"]
    fn the_package_of_a_real_map_is_exported() {
        let Some(global) = std::env::var_os("OMSI_TEST_PACKAGE_MAP").map(PathBuf::from) else { return };
        let cfg = omsi_map::GlobalCfg::load(&global).expect("global.cfg");
        omsi_map::configure_grid(&cfg);
        let big = 1.0e12;
        let v = package_view(&global, DVec2::ZERO, DVec2::splat(-big), DVec2::splat(big), 1.2);
        assert!(!v.is_null(), "no package found beside {}", global.display());
        let triangles: usize = v["fills"].as_array().unwrap().iter().map(|f| f.as_array().unwrap().len() / 6).sum();
        let lines = v["lines"].as_array().unwrap().len();
        println!("package \"{}\": {triangles} triangles, {lines} lines, {} bytes of JSON", v["name"], v.to_string().len());
        assert!(triangles > 0);
    }

    #[test]
    fn a_map_without_a_package_exports_null() {
        let dir = std::env::temp_dir().join("omsi-mapview-no-package");
        let _ = std::fs::create_dir_all(&dir);
        let v = package_view(&dir.join("global.cfg"), DVec2::ZERO, DVec2::splat(-10.0), DVec2::splat(10.0), 1.2);
        assert!(v.is_null());
    }
}
