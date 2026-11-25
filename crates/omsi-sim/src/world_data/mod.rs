//! The map world the simulation reads: the ground of the loaded tiles, the lists made from
//! them (stops, waiting places, obstacles, parked cars' boxes, petrol stations, echoing
//! places), where every object of the map stands and the map-wide index's facts about the
//! stops. No GPU handle and nothing of the view in it.
//!
//! [`WorldData`] is a snapshot: immutable, shared as `Arc`, made of `Arc`'d parts. The loader
//! publishes a new one into a [`WorldCell`] whenever a tile's ground comes or goes and when
//! the tile lists are rebuilt (copying only the part it changes); a reader takes the current
//! one without a lock (`arc-swap`) and keeps it for as long as it needs a consistent world -
//! a vehicle's step, the people's tick. The game's scene (`omsi-scene::World`) holds the cell
//! and keeps its GPU side and its load-time caches to itself.

mod arrivals;
mod ground;
mod traits;

pub use arrivals::{TrafficArrival, TrafficArrivals};

pub use ground::{
    BURIED_FACE, GroundTiles, LiveGround, PAINT_LAYER, WALL_TOP_STEP, probe_tile, surface_flush,
    tile_key,
};

use crate::collision::{CollisionWorld, Obb};
use arc_swap::ArcSwap;
use glam::DVec3;
use hashbrown::{HashMap, HashSet};
use omsi_world::map_index::MapIndex;
use std::path::PathBuf;
use std::sync::Arc;

/// What the simulation knows of the loaded map, as one consistent snapshot.
#[derive(Clone, Default)]
pub struct WorldData {
    pub map: Arc<MapFacts>,
    pub ground: Arc<GroundTiles>,
    pub lists: Arc<TileLists>,
    pub objects: Arc<ObjectPlaces>,
    /// The map-wide index (every tile file read once), once it is built: the stops' strings.
    pub index: Option<Arc<MapIndex>>,
    /// The map's tiles whose file exists (the tile layout's), once it is known.
    pub tile_files: Option<Arc<HashSet<(i32, i32)>>>,
}

/// The map itself: its folders, its tiles, its AI lists and date.
#[derive(Clone, Default)]
pub struct MapFacts {
    /// The game's folder and the map's.
    pub root: PathBuf,
    pub map_dir: PathBuf,
    /// The tiles of `global.cfg` in the order the timetable's tile indices count them.
    pub raw_tiles: Vec<(i32, i32)>,
    /// global.cfg's `[map]` tiles with their files (relative to `map_dir`).
    pub tiles: Vec<((i32, i32), String)>,
    /// The map's `ailists.cfg`, its date (yyyymmdd) and the chrono folders in force on it.
    pub ailists: omsi_map::AiLists,
    pub date: i32,
    pub chrono_dirs: Vec<PathBuf>,
}

/// What the loaded tiles hold, gathered over all of them (`World::refresh_tile_lists`).
#[derive(Clone, Default)]
pub struct TileLists {
    /// Counts the rebuilds: whoever keeps what it derived from the stops, the waiting
    /// places or the ground looks again.
    pub generation: u64,
    /// Placed `[busstop]` objects: (map id, world position, heading, name).
    pub bus_stops: Vec<(i64, DVec3, f64, String)>,
    /// Where people wait at the stops: (object id, world position, heading in degrees, seat
    /// height - 0 for a standing place).
    pub waiting_places: Vec<(i64, DVec3, f64, f32)>,
    /// Obstacles for vehicle collisions.
    pub collision: Arc<CollisionWorld>,
    /// The boxes of the parked cars, which people walk round.
    pub parked_boxes: Arc<Vec<Obb>>,
    /// The boxes of the `[petrolstation]` objects (fuel and wash yards).
    pub petrol_stations: Vec<Obb>,
    /// Places that echo: the box, the reverberation time (s) and the distance (m) over which
    /// it fades in at the box's sides.
    pub reverb_zones: Vec<(Obb, f32, f32)>,
}

/// Where an object stands: world position (with terrain height) and rotation.
pub type Place = (DVec3, [f64; 3]);

/// Where the map's objects stand, by id: first as the map index read them (every object of
/// the map), then as placing their tiles put them (a later placing wins); and the objects
/// whose id is used on more than one tile, by (tile, id).
///
/// The placings are kept as layers over the index's list, one per placed tile (merged now
/// and then), so that publishing a tile's objects does not copy the whole map's.
#[derive(Clone, Default)]
pub struct ObjectPlaces {
    base: Arc<HashMap<i64, Place>>,
    layers: Vec<Arc<HashMap<i64, Place>>>,
    dups: Arc<HashMap<((i32, i32), i64), Place>>,
}

/// More placing layers than this are merged into one.
const PLACE_LAYERS: usize = 8;

impl ObjectPlaces {
    /// Where object `id` stands.
    pub fn get(&self, id: &i64) -> Option<&Place> {
        self.layers
            .iter()
            .rev()
            .find_map(|l| l.get(id))
            .or_else(|| self.base.get(id))
    }

    /// Every object once, where it stands now (in no particular order).
    pub fn iter(&self) -> impl Iterator<Item = (&i64, &Place)> + '_ {
        let mut seen: HashSet<i64> = HashSet::new();
        self.layers
            .iter()
            .rev()
            .chain(std::iter::once(&self.base))
            .flat_map(|l| l.iter())
            .filter(move |(id, _)| seen.insert(**id))
    }

    /// Object `id` of tile `tile`, when its id is used on more than one tile.
    pub fn dup(&self, tile: (i32, i32), id: i64) -> Option<&Place> {
        self.dups.get(&(tile, id))
    }

    /// The map index's objects and duplicates: those not known yet are added (an object
    /// already known keeps its place).
    pub fn add_index(
        &mut self,
        objects: impl IntoIterator<Item = (i64, Place)>,
        dups: impl IntoIterator<Item = (((i32, i32), i64), Place)>,
    ) {
        let base = Arc::make_mut(&mut self.base);
        for (id, p) in objects {
            base.entry(id).or_insert(p);
        }
        let mut dups = dups.into_iter().peekable();
        if dups.peek().is_some() {
            let d = Arc::make_mut(&mut self.dups);
            for (k, p) in dups {
                d.entry(k).or_insert(p);
            }
        }
    }

    /// The objects of tile `tile` as placed: each stands where it was put, and so does its
    /// entry among the duplicates (when it has one).
    pub fn place(&mut self, tile: (i32, i32), placed: Vec<(i64, Place)>) {
        if placed.is_empty() {
            return;
        }
        if placed
            .iter()
            .any(|(id, _)| self.dups.contains_key(&(tile, *id)))
        {
            let d = Arc::make_mut(&mut self.dups);
            for (id, p) in &placed {
                if let Some(e) = d.get_mut(&(tile, *id)) {
                    *e = *p;
                }
            }
        }
        self.layers.push(Arc::new(placed.into_iter().collect()));
        if self.layers.len() > PLACE_LAYERS {
            let mut merged: HashMap<i64, Place> =
                HashMap::with_capacity(self.layers.iter().map(|l| l.len()).sum());
            for l in &self.layers {
                merged.extend(l.iter().map(|(k, v)| (*k, *v)));
            }
            self.layers = vec![Arc::new(merged)];
        }
    }
}

impl WorldData {
    /// The map has tile `key` (listed in global.cfg, and its file exists).
    pub fn has_tile(&self, key: (i32, i32)) -> bool {
        match &self.tile_files {
            Some(t) => t.contains(&key),
            // (before the layout is known: the same question asked of the files)
            None => self.map.tiles.iter().any(|(k, f)| {
                *k == key && omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(&self.map.map_dir, f))
            }),
        }
    }

    /// How many passengers get off at stop object `id` (a stop without strings: the
    /// defaults' mean, 0.5).
    pub fn stop_exit_weight(&self, id: i64) -> f32 {
        self.index
            .as_ref()
            .and_then(|ix| ix.stop_weights.get(&id).copied())
            .unwrap_or(0.5)
    }

    /// Stop object `id`'s (pass_enter_max, pass_enter_min) (a stop without strings: 1 and 0).
    pub fn stop_enter(&self, id: i64) -> (f32, f32) {
        self.index
            .as_ref()
            .and_then(|ix| ix.stop_enter.get(&id).copied())
            .unwrap_or((1.0, 0.0))
    }

    /// The side stop object `id`'s platform lies on: 0 = right, 1 = the other, 2 = both; a
    /// stop the map says nothing about: 0.
    pub fn stop_side(&self, id: i64) -> f32 {
        self.index
            .as_ref()
            .and_then(|ix| ix.stop_side.get(&id).copied())
            .unwrap_or(0.0)
    }

    /// Stop object `id`'s length (30 m when the map says nothing).
    pub fn stop_length(&self, id: i64) -> f32 {
        self.index
            .as_ref()
            .and_then(|ix| ix.stop_length.get(&id).copied())
            .unwrap_or(30.0)
    }
}

/// The current [`WorldData`], replaced as a whole by the loader and read without a lock.
pub struct WorldCell {
    current: ArcSwap<WorldData>,
    /// Serialises the writers (the loader's threads and the frame's): a read-copy-publish
    /// must not lose another's change. Readers never take it.
    writer: parking_lot::Mutex<()>,
}

impl WorldCell {
    pub fn new(data: WorldData) -> WorldCell {
        WorldCell {
            current: ArcSwap::from_pointee(data),
            writer: parking_lot::Mutex::new(()),
        }
    }

    /// The current snapshot, to keep (a step's or a tick's world).
    pub fn snapshot(&self) -> Arc<WorldData> {
        self.current.load_full()
    }

    /// The current snapshot for one short look (cheaper than [`WorldCell::snapshot`]; do not
    /// hold it across a frame).
    pub fn peek(&self) -> arc_swap::Guard<Arc<WorldData>> {
        self.current.load()
    }

    /// Change the world: `f` gets a copy of the current snapshot (its parts still shared:
    /// `Arc::make_mut` copies the one it changes) and what it leaves is published.
    pub fn update<R>(&self, f: impl FnOnce(&mut WorldData) -> R) -> R {
        let _w = self.writer.lock();
        let mut d = WorldData::clone(&self.current.load());
        let r = f(&mut d);
        self.current.store(Arc::new(d));
        r
    }
}

impl Default for WorldCell {
    fn default() -> WorldCell {
        WorldCell::new(WorldData::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64) -> Place {
        (DVec3::new(x, 0.0, 0.0), [0.0; 3])
    }

    #[test]
    fn placed_objects_override_the_index_and_the_latest_placing_wins() {
        let mut o = ObjectPlaces::default();
        o.add_index(
            [(1, at(1.0)), (2, at(2.0))],
            [(((0, 0), 2), at(2.0)), (((1, 0), 2), at(20.0))],
        );
        o.place((1, 0), vec![(2, at(21.0)), (3, at(3.0))]);
        assert_eq!(o.get(&2), Some(&at(21.0)));
        assert_eq!(o.dup((1, 0), 2), Some(&at(21.0)));
        assert_eq!(o.dup((0, 0), 2), Some(&at(2.0)));
        // a rebuilt index adds what is new and keeps what is known
        o.add_index([(1, at(100.0)), (4, at(4.0))], []);
        assert_eq!(o.get(&1), Some(&at(1.0)));
        assert_eq!(o.get(&4), Some(&at(4.0)));
        for k in 0..(PLACE_LAYERS as i64 * 2) {
            o.place((k as i32, 0), vec![(2, at(30.0 + k as f64))]);
        }
        assert!(o.layers.len() <= PLACE_LAYERS);
        assert_eq!(o.get(&2), Some(&at(30.0 + (PLACE_LAYERS * 2 - 1) as f64)));
        assert_eq!(o.get(&3), Some(&at(3.0)));
        let mut all: Vec<(i64, f64)> = o.iter().map(|(k, p)| (*k, p.0.x)).collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            all,
            vec![
                (1, 1.0),
                (2, 30.0 + (PLACE_LAYERS * 2 - 1) as f64),
                (3, 3.0),
                (4, 4.0)
            ]
        );
    }

    #[test]
    fn a_snapshot_stays_as_it_was_taken() {
        let cell = WorldCell::default();
        let before = cell.snapshot();
        cell.update(|d| Arc::make_mut(&mut d.lists).generation = 7);
        assert_eq!(before.lists.generation, 0);
        assert_eq!(cell.snapshot().lists.generation, 7);
        assert_eq!(cell.peek().lists.generation, 7);
        // the parts it did not change are shared, not copied
        assert!(Arc::ptr_eq(&before.ground, &cell.snapshot().ground));
    }
}
