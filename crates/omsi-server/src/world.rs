//! CPU-only map loading for the dedicated host.
//!
//! Read global.cfg, Chrono lists and the whole-map index. Neither the world
//! metadata nor its collision/traffic snapshot owns a wgpu device or a scene
//! graph. The 3D renderer is not involved in this operation.

use anyhow::{Context, Result};
use glam::{DVec2, DVec3};
use omsi_geometry::{MeshData, SplineCurve};
use omsi_map::global::GlobalCfg;
use omsi_map::{Terrain, tile::Tile, tile_size};
use omsi_scenery::{SceneryObject, Spline};
use omsi_sim::traffic::{Lane, Network};
use omsi_sim::world_data::{MapFacts, WorldCell, WorldData};
use omsi_world::map_index::MapIndex;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// CPU data that stays alive for the lifetime of a dedicated map session.
pub struct HostWorld {
    pub global: GlobalCfg,
    pub map_dir: PathBuf,
    pub data: Arc<WorldCell>,
    pub index: Arc<MapIndex>,
    /// Bounded, CPU-only tile data; no GPU buffers or material textures.
    tiles: HostTileCache,
    tile_paths: HashMap<(i32, i32), PathBuf>,
    chrono_dirs: Vec<PathBuf>,
    spline_defs: HashMap<String, Option<Arc<Spline>>>,
    object_defs: HashMap<String, Option<Arc<SceneryObject>>>,
    crossing_fields: HashMap<PathBuf, Result<Option<Arc<MeshData>>, ()>>,
}

/// LRU cache for immutable OMSI tile records. A map can have tens of thousands
/// of tiles; we must not retain every .map and terrain file in RAM.
struct HostTileCache {
    limit: usize,
    order: VecDeque<(i32, i32)>,
    loaded: HashMap<(i32, i32), (Arc<Tile>, Arc<Terrain>)>,
}

impl HostTileCache {
    fn new(limit: usize) -> Self {
        Self {
            limit: limit.clamp(1, 512),
            order: VecDeque::new(),
            loaded: HashMap::new(),
        }
    }

    fn get(&mut self, key: (i32, i32)) -> Option<(Arc<Tile>, Arc<Terrain>)> {
        let pair = self.loaded.get(&key)?.clone();
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
        Some(pair)
    }

    fn insert(
        &mut self,
        key: (i32, i32),
        tile: Arc<Tile>,
        ground: Arc<Terrain>,
    ) -> Option<(i32, i32)> {
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
        self.loaded.insert(key, (tile, ground));
        if self.loaded.len() > self.limit {
            let oldest = self.order.pop_front()?;
            self.loaded.remove(&oldest);
            return Some(oldest);
        }
        None
    }
}

/// Locate a map entry point using the same rules as the graphical World:
/// resolve duplicate object IDs on the record's own tile first, then the
/// indexed object, and finally global.cfg's recorded position and heading.
pub(crate) fn resolve_entry_point(
    global: &GlobalCfg,
    index: &MapIndex,
    ep: &omsi_map::global::EntryPoint,
) -> Option<(DVec3, [f64; 3])> {
    let tile = usize::try_from(ep.group)
        .ok()
        .and_then(|i| global.raw_tiles.get(i))
        .copied();
    if let Some(key) = tile {
        if let Some(place) = index.duplicates.get(&(key, ep.object_id)) {
            return Some(*place);
        }
    }
    let found = index
        .objects
        .get(&ep.object_id)
        .map(|(_, pos, rot)| (*pos, *rot));
    let recorded = tile
        .filter(|_| ep.pos.iter().chain(ep.quat.iter()).all(|v| v.is_finite()))
        .map(|(tx, ty)| {
            let heading = (2.0 * ep.quat[1].atan2(ep.quat[3]))
                .to_degrees()
                .rem_euclid(360.0);
            (
                DVec3::new(
                    f64::from(tx) * tile_size() + ep.pos[0],
                    f64::from(ty) * tile_size() + ep.pos[1],
                    ep.pos[2],
                ),
                [heading, 0.0, 0.0],
            )
        });
    match (found, recorded) {
        (Some(f), Some(r)) if (f.0.truncate() - r.0.truncate()).length() > 50.0 => {
            log::info!(
                "entry point {} \"{}\": object {} is on another tile: using the record's position",
                ep.index,
                ep.name,
                ep.object_id
            );
            Some(r)
        }
        (Some(f), _) => Some(f),
        (None, r) => r,
    }
}

impl HostWorld {
    /// `date` is the YYYYMMDD date used for OMSI Chrono selections.
    ///
    /// This phase loads the **map's CPU index and metadata**, not GPU meshes.
    /// Road/collision tile streaming and its sim adapters must be installed by
    /// the actual host engine before accepting players.
    pub fn open(root: &Path, global_path: &Path, date: i32) -> Result<Self> {
        let mut global = GlobalCfg::load(global_path)
            .with_context(|| format!("reading map {}", global_path.display()))?;
        omsi_map::configure_grid(&global);
        if global.entry_points.is_empty() {
            let added = global.add_object_entry_points();
            log::info!("entry points: {added} from the tiles' entry objects");
        } else {
            log::info!(
                "entry points: {} listed in global.cfg",
                global.entry_points.len()
            );
        }
        let map_dir = global.dir().to_path_buf();
        let chrono_dirs = omsi_map::active_chrono_dirs(&map_dir, date);
        let ailists = omsi_map::ailists::ailists_with_chrono(&map_dir, &chrono_dirs);
        let files: Vec<(usize, i32, i32, PathBuf)> = global
            .tiles
            .iter()
            .map(|t| (t.index, t.x, t.y, omsi_cfg::resolve_path(&map_dir, &t.file)))
            .collect();

        // CPU indexing is parallel; the resulting data structures are immutable
        // and shared, with no textures/mesh buffers to upload or retain.
        let tile_paths = files
            .iter()
            .map(|(_, x, y, p)| ((*x, *y), p.clone()))
            .collect();
        let index = Arc::new(MapIndex::build(&files, &chrono_dirs, &|file: &str| {
            omsi_cfg::resolve_path(root, file)
        }));
        let mut data = WorldData {
            map: Arc::new(MapFacts {
                root: root.to_path_buf(),
                map_dir: map_dir.clone(),
                raw_tiles: global.raw_tiles.clone(),
                tiles: global
                    .tiles
                    .iter()
                    .map(|t| ((t.x, t.y), t.file.clone()))
                    .collect(),
                ailists,
                date,
                chrono_dirs: chrono_dirs.clone(),
            }),
            tile_files: Some(Arc::new(
                files
                    .iter()
                    .filter(|(_, _, _, path)| omsi_cfg::vfs::is_file(path))
                    .map(|(_, x, y, _)| (*x, *y))
                    .collect(),
            )),
            ..Default::default()
        };
        let objects = Arc::make_mut(&mut data.objects);
        objects.add_index(
            index
                .objects
                .iter()
                .map(|(id, (_tile, pos, rot))| (*id, (*pos, *rot))),
            index
                .duplicates
                .iter()
                .map(|(key, (pos, rot))| (*key, (*pos, *rot))),
        );
        data.index = Some(Arc::clone(&index));

        log::info!(
            "dedicated CPU world: {} tiles indexed, {} unreadable, {} placed objects (no renderer allocated)",
            index.tiles_read,
            index.tiles_failed,
            index.objects.len()
        );
        Ok(Self {
            global,
            map_dir,
            data: Arc::new(WorldCell::new(data)),
            index,
            tiles: HostTileCache::new(64),
            tile_paths,
            chrono_dirs,
            spline_defs: HashMap::new(),
            object_defs: HashMap::new(),
            crossing_fields: HashMap::new(),
        })
    }

    /// Read one tile including Chrono patches, and publish its terrain into the
    /// simulation's ground store. CPU map/terrain parsing is lazy; repeated
    /// queries use a bounded LRU and do not retain any rendering assets.
    pub fn load_tile(&mut self, key: (i32, i32)) -> Option<Arc<Tile>> {
        if let Some((tile, _ground)) = self.tiles.get(key) {
            return Some(tile);
        }
        let path = self.tile_paths.get(&key)?;
        // read_tile already applies Chrono patches and world-grid scaling.
        // Never fit a second time: geographic tiles would shift their paths.
        let tile = omsi_world::map_index::read_tile(path, &self.chrono_dirs)?;
        let source = tile.terrain_from.as_ref().unwrap_or(&tile.path);
        let ground_path = omsi_world::map_index::tile_companion(source, ".terrain");
        let ground = Arc::new(if tile.has_terrain {
            match Terrain::load(&ground_path) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!(
                        "dedicated CPU terrain {}: {e}; using flat fallback",
                        ground_path.display()
                    );
                    Terrain::flat()
                }
            }
        } else {
            Terrain::flat()
        });
        let tile = Arc::new(tile);
        self.tiles
            .insert(key, Arc::clone(&tile), Arc::clone(&ground));
        // Evicting a decoded .map cache entry MUST NOT unload live physics
        // terrain from beneath scheduled traffic or a player. Ground snapshots
        // are retained until a separate, entity-aware unloading policy exists.
        self.data.update(|data| {
            Arc::make_mut(&mut data.ground).terrains.insert(key, ground);
        });
        Some(tile)
    }

    /// Sample CPU terrain below a person or vehicle, even without a render
    /// device or window. Road collision overlays will be added by the physical
    /// tile loader, not silently approximated by the terrain.
    pub fn terrain_height(&mut self, x: f64, y: f64) -> Option<f32> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (key, (local_x, local_y)) = omsi_map::world_to_tile_local(x, y);
        self.load_tile(key)?;
        let terrain = self.tiles.loaded.get(&key).map(|(_, t)| t)?;
        Some(terrain.sample(local_x as f32, local_y as f32))
    }

    /// Load a spline definition at most once per active cache window.
    pub(crate) fn spline(&mut self, root: &Path, name: &str) -> Option<Arc<Spline>> {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        if let Some(v) = self.spline_defs.get(&key) {
            return v.clone();
        }
        let path = omsi_cfg::resolve_path(root, name);
        let val = match Spline::load(&path) {
            Ok(v) => Some(Arc::new(v)),
            Err(e) => {
                log::debug!("CPU spline {}: {e}", path.display());
                None
            }
        };
        if self.spline_defs.len() >= 256 {
            self.spline_defs.clear();
        }
        self.spline_defs.insert(key, val.clone());
        val
    }

    /// Read only .sco path/script metadata; no 3D meshes or textures.
    pub(crate) fn scenery(&mut self, root: &Path, name: &str) -> Option<Arc<SceneryObject>> {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        if let Some(v) = self.object_defs.get(&key) {
            return v.clone();
        }
        let path = omsi_cfg::resolve_path(root, name);
        let val = match SceneryObject::load(&path) {
            Ok(v) => Some(Arc::new(v)),
            Err(e) => {
                log::debug!("CPU scenery {}: {e}", path.display());
                None
            }
        };
        if self.object_defs.len() >= 256 {
            self.object_defs.clear();
        }
        self.object_defs.insert(key, val.clone());
        val
    }

    /// Cache the authored height field; every crossing of this type reuses it.
    /// A field is optional, but a declared field that cannot be loaded must
    /// never produce a pretend flat crossing in the AI path network.
    fn crossing_field(&mut self, def: &SceneryObject) -> Result<Option<Arc<MeshData>>, ()> {
        if let Some(field) = self.crossing_fields.get(&def.path) {
            return field.clone();
        }
        let field = crate::scenery_ground::crossing_field(def);
        if self.crossing_fields.len() >= 256 {
            self.crossing_fields.clear();
        }
        self.crossing_fields.insert(def.path.clone(), field.clone());
        field
    }

    /// Build AI/pedestrian road lanes from the same .sli/.sco definitions and map
    /// rules as the game, but without allocating any render/model resources.
    /// Junction signal controllers and collision meshes are attached later.
    pub fn lanes_for_tile(&mut self, key: (i32, i32)) -> Option<Vec<Lane>> {
        let tile = self.load_tile(key)?;
        let root = self.data.snapshot().map.root.clone();
        let terrain = self.tiles.loaded.get(&key)?.1.clone();
        let origin = DVec2::new(key.0 as f64 * tile_size(), key.1 as f64 * tile_size());
        let mut lanes = Vec::new();
        for spline in &tile.splines {
            if spline.deleted || spline.file.trim().is_empty() {
                continue;
            }
            let Some(def) = self.spline(&root, &spline.file) else {
                continue;
            };
            if def.paths.is_empty() {
                continue;
            }
            let curve = SplineCurve::from_map(spline, origin);
            let mut made = crate::lanes::spline_lanes(&def, spline, &curve, key);
            for lane in &mut made {
                lane.invisible = def.only_editor;
            }
            lanes.extend(made);
        }
        for object in &tile.objects {
            if object.file.trim().is_empty() {
                continue;
            }
            let Some(def) = self.scenery(&root, &object.file) else {
                continue;
            };
            if def.paths.is_empty() {
                continue;
            }
            // Tile::fit_to_world_grid scales authored placement, but the
            // original terrain raster remains in the tile's native coordinates.
            let (scale_x, scale_y) = omsi_map::world_tile_scale(key.1);
            let lx = (object.pos[0] / scale_x).clamp(0.0, omsi_map::TILE_SIZE) as f32;
            let ly = (object.pos[1] / scale_y)
                .clamp(0.0, omsi_map::WORLD_TILE_SIZE.max(tile_size())) as f32;
            let height = if def.absolute_height() {
                0.0
            } else {
                terrain.sample(lx, ly) as f64
            };
            let place = DVec3::new(
                origin.x + object.pos[0],
                origin.y + object.pos[1],
                object.pos[2] + height,
            );
            let Ok(field) = self.crossing_field(&def) else {
                log::warn!(
                    "dedicated AI: skipped crossing paths with unavailable deformation {}",
                    def.path.display()
                );
                continue;
            };
            let mut made = crate::lanes::object_lanes(
                &def,
                place,
                [object.rot[0], 0.0, 0.0],
                None,
                key,
                object.id,
                &object.rules,
            );
            crate::lanes::place_object_paths(&mut made, place, object.rot, field.as_deref());
            lanes.extend(made);
        }
        Some(lanes)
    }

    /// Link the nearby CPU lanes into the very same traffic network type used
    /// by the graphical client. Callers control the active tile set.
    pub fn network_for_tiles(&mut self, keys: &[(i32, i32)]) -> Network {
        let mut network = Network::default();
        for &key in keys {
            if let Some(lanes) = self.lanes_for_tile(key) {
                network.extend(lanes, omsi_sim::traffic::LINK_TOLERANCE);
            }
        }
        network
    }

    /// The restricted relay has no autonomous AI vehicles or pedestrians.
    /// Retain terrain in the active region and its immediate neighborhood,
    /// but release previous road rasters after the clients move away.
    /// Do not use this with populated authoritative simulation worlds.
    pub fn prune_streamed_ground(&mut self, active: &[(i32, i32)]) {
        use std::collections::BTreeSet;
        let visible: BTreeSet<_> = active.iter().copied().collect();
        let mut terrain = visible.clone();
        for &(x, y) in &visible {
            for dx in -1_i32..=1 {
                for dy in -1_i32..=1 {
                    if let (Some(x), Some(y)) = (x.checked_add(dx), y.checked_add(dy)) {
                        terrain.insert((x, y));
                    }
                }
            }
        }
        self.data.update(|data| {
            let ground = Arc::make_mut(&mut data.ground);
            ground.surfaces.retain(|key, _| visible.contains(key));
            ground.terrains.retain(|key, _| terrain.contains(key));
        });
    }

    pub fn loaded_tiles(&self) -> usize {
        self.tiles.loaded.len()
    }
}

/// The map of `server.cfg` is installed (in the original folder or the content folder), or
/// what to do: the maps there are.
pub fn check_map(root: &std::path::Path, map: &str) -> Result<(), String> {
    let path = omsi_cfg::resolve_path(root, map);
    if omsi_cfg::vfs::exists(&path) {
        return Ok(());
    }
    Err(format!(
        "the map {} (server.cfg's map, or --server-map) is not installed ({} not found). Installed maps: {}",
        map,
        path.display(),
        Some(omsi_lan::server_cfg::installed_maps().join(", "))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| "none".into())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoint_resolution_matches_graphical_world_for_duplicate_ids() {
        let mut global = GlobalCfg::default();
        global.raw_tiles = vec![(0, 0), (10, 5)];
        let entry = omsi_map::global::EntryPoint {
            group: 1,
            object_id: 77,
            pos: [5.0, 10.0, 3.0],
            quat: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        let mut index = MapIndex::default();
        index
            .objects
            .insert(77, ((0, 0), DVec3::new(1.0, 2.0, 3.0), [0.0, 0.0, 0.0]));
        let expected = (DVec3::new(11.0, 22.0, 33.0), [90.0, 0.0, 0.0]);
        index.duplicates.insert(((10, 5), 77), expected);
        assert_eq!(resolve_entry_point(&global, &index, &entry), Some(expected));
    }

    #[test]
    fn entrypoint_resolution_uses_global_record_when_object_is_missing() {
        let mut global = GlobalCfg::default();
        global.raw_tiles = vec![(3, 4)];
        let half = std::f64::consts::FRAC_PI_4;
        let entry = omsi_map::global::EntryPoint {
            group: 0,
            object_id: 98,
            pos: [6.5, 225.5, 0.2],
            quat: [0.0, half.sin(), 0.0, half.cos()],
            ..Default::default()
        };
        let (position, heading) =
            resolve_entry_point(&global, &MapIndex::default(), &entry).unwrap();
        assert!((position.x - (3.0 * tile_size() + 6.5)).abs() < 1.0e-6);
        assert!((position.y - (4.0 * tile_size() + 225.5)).abs() < 1.0e-6);
        assert_eq!(position.z, 0.2);
        assert!((heading[0] - 90.0).abs() < 1.0e-6);
    }

    #[test]
    fn cpu_tile_cache_stays_bounded_and_recent_tiles_survive() {
        let mut cache = HostTileCache::new(2);
        let tile = Arc::new(Tile::default());
        let ground = Arc::new(Terrain::flat());
        assert_eq!(
            cache.insert((0, 0), Arc::clone(&tile), Arc::clone(&ground)),
            None
        );
        assert_eq!(
            cache.insert((1, 0), Arc::clone(&tile), Arc::clone(&ground)),
            None
        );
        assert!(cache.get((0, 0)).is_some());
        assert_eq!(cache.insert((2, 0), tile, ground), Some((1, 0)));
        assert!(cache.get((1, 0)).is_none());
        assert!(cache.get((0, 0)).is_some());
        assert_eq!(cache.loaded.len(), 2);
        assert_eq!(cache.order.len(), 2);
    }

    #[test]
    fn cpu_tile_cache_restricts_untrusted_capacity() {
        assert_eq!(HostTileCache::new(0).limit, 1);
        assert_eq!(HostTileCache::new(usize::MAX).limit, 512);
    }
}
