//! GPU-free publication of bus stops and passenger waiting places from OMSI
//! map records. The CPU PeopleSim consumes WorldData::lists, not a scene.
//!
//! This deliberately processes a complete active tile set: partial calls
//! replace the previous stop set, just as the graphical tile streamer does.
//! Full attachment placement and terrain deformation are still required
//! before the native dedicated host can admit game clients.

use crate::world::HostWorld;
use glam::{DVec2, DVec3, Vec3};
use omsi_map::MapObject;
use omsi_scenery::SceneryObject;
use omsi_vehicle::PassengerCabin;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

type BusStop = (i64, DVec3, f64, String);
type WaitingPlace = (i64, DVec3, f64, f32);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StopCounts {
    pub loaded_tiles: usize,
    pub bus_stops: usize,
    pub waiting_places: usize,
}

/// Compute world-space stop and waiting-point locations with the same object
/// rotation and passenger cabin convention as the graphical tile placement.
/// A map attachment requires its resolved parent transform; callers exclude
/// those until that CPU placement machinery is available.
fn placed_stops(
    object: &MapObject,
    definition: &SceneryObject,
    world_pos: DVec3,
    cabin: Option<&PassengerCabin>,
) -> (Option<BusStop>, Vec<WaitingPlace>) {
    let heading = object.rot[0];
    let stop = definition.is_bus_stop.then(|| {
        (
            object.id,
            world_pos,
            heading,
            object.extra.first().cloned().unwrap_or_default(),
        )
    });
    let rotation = omsi_geometry::object_rotation(object.rot);
    let waiting = cabin
        .into_iter()
        .flat_map(|c| &c.pass_positions)
        .map(|p| {
            let point = world_pos + rotation.transform_vector3(Vec3::from(p.pos)).as_dvec3();
            (object.id, point, heading + p.rot as f64, p.height)
        })
        .collect();
    (stop, waiting)
}

/// MapIndex resolves terrain height and Chrono placement without borrowing
/// an object with the same numeric ID from another tile.
fn indexed_position(
    index: &omsi_world::map_index::MapIndex,
    tile: (i32, i32),
    id: i64,
) -> Option<DVec3> {
    index
        .duplicates
        .get(&(tile, id))
        .map(|(pos, _)| *pos)
        .or_else(|| {
            index
                .objects
                .get(&id)
                .and_then(|(owner, pos, _)| (*owner == tile).then_some(*pos))
        })
}

fn unique_keys(keys: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut seen = HashSet::with_capacity(keys.len());
    keys.iter()
        .copied()
        .filter(|key| seen.insert(*key))
        .collect()
}

impl HostWorld {
    /// Replace the passenger stop/waiting lists from the *complete* active
    /// CPU tile set, without touching collision/parking lists or the renderer.
    ///
    /// This is a distinct batch operation so lazy tile reads and an LRU cache
    /// do not continually copy whole-world stop lists into WorldCell.
    pub fn install_stop_lists(&mut self, keys: &[(i32, i32)]) -> StopCounts {
        let root = self.data.snapshot().map.root.clone();
        let mut counts = StopCounts::default();
        let mut stops = Vec::new();
        let mut waiting = Vec::new();
        let mut cabins: HashMap<PathBuf, Option<Arc<PassengerCabin>>> = HashMap::new();

        for key in unique_keys(keys) {
            let Some(tile) = self.load_tile(key) else {
                continue;
            };
            counts.loaded_tiles += 1;
            let origin = DVec2::new(
                key.0 as f64 * omsi_map::tile_size(),
                key.1 as f64 * omsi_map::tile_size(),
            );
            for object in &tile.objects {
                // An attached object has no standalone world pose: using its
                // raw local coordinates would create a phantom stop.
                if object.parent_id.is_some() || object.file.trim().is_empty() {
                    continue;
                }
                let Some(definition) = self.scenery(&root, &object.file) else {
                    continue;
                };
                if !definition.is_bus_stop && definition.passenger_cabin.is_none() {
                    continue;
                }
                let x = origin.x + object.pos[0];
                let y = origin.y + object.pos[1];
                if !x.is_finite() || !y.is_finite() {
                    continue;
                }
                let position = indexed_position(&self.index, key, object.id).unwrap_or_else(|| {
                    let ground = if definition.absolute_height() {
                        0.0
                    } else {
                        // Objects can stand over a neighbouring tile's terrain.
                        self.terrain_height(x, y).map(f64::from).unwrap_or(0.0)
                    };
                    DVec3::new(x, y, object.pos[2] + ground)
                });
                if !position.is_finite() {
                    continue;
                }

                let cabin = definition.passenger_cabin.as_ref().and_then(|rel| {
                    let base = definition.path.parent().unwrap_or(Path::new(""));
                    let path = omsi_cfg::resolve_path(base, rel);
                    cabins
                        .entry(path.clone())
                        .or_insert_with(|| match PassengerCabin::load(&path) {
                            Ok(c) => Some(Arc::new(c)),
                            Err(e) => {
                                log::debug!("headless waiting cabin {}: {e}", path.display());
                                None
                            }
                        })
                        .as_deref()
                });
                let (stop, places) = placed_stops(object, &definition, position, cabin);
                if let Some(stop) = stop {
                    stops.push(stop);
                }
                waiting.extend(places);
            }
        }
        counts.bus_stops = stops.len();
        counts.waiting_places = waiting.len();
        self.data.update(|data| {
            let lists = Arc::make_mut(&mut data.lists);
            if lists.bus_stops != stops || lists.waiting_places != waiting {
                lists.bus_stops = stops;
                lists.waiting_places = waiting;
                lists.generation = lists.generation.wrapping_add(1);
            }
        });
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::cabin::PassPos;

    #[test]
    fn stop_name_heading_and_rotated_waiting_place_match_scenery_placement() {
        let map_object = MapObject {
            id: 73,
            rot: [90.0, 12.0, -4.0],
            extra: vec!["Central Station".into()],
            ..Default::default()
        };
        let definition = SceneryObject {
            is_bus_stop: true,
            ..Default::default()
        };
        let cabin = PassengerCabin {
            pass_positions: vec![PassPos {
                pos: [1.0, 3.0, 0.5],
                rot: -15.0,
                height: 0.3,
                ..Default::default()
            }],
            ..Default::default()
        };
        let position = DVec3::new(100.0, 200.0, 15.0);
        let (stops, waiting) = placed_stops(&map_object, &definition, position, Some(&cabin));
        assert_eq!(stops, Some((73, position, 90.0, "Central Station".into())));
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].0, 73);
        assert_eq!(waiting[0].2, 75.0);
        assert_eq!(waiting[0].3, 0.3);
        let expected = position
            + omsi_geometry::object_rotation(map_object.rot)
                .transform_vector3(Vec3::from([1.0, 3.0, 0.5]))
                .as_dvec3();
        assert!((waiting[0].1 - expected).length() < 0.00001);
        assert!((waiting[0].1 - position).length() > 1.0);
    }

    #[test]
    fn unmarked_object_does_not_create_a_phantom_stop() {
        let (stop, places) = placed_stops(
            &MapObject::default(),
            &SceneryObject::default(),
            DVec3::ZERO,
            None,
        );
        assert!(stop.is_none());
        assert!(places.is_empty());
    }

    #[test]
    fn duplicate_object_ids_do_not_read_the_other_tiles_height() {
        let mut index = omsi_world::map_index::MapIndex::default();
        let a = DVec3::new(1.0, 2.0, 3.0);
        let b = DVec3::new(10.0, 20.0, 30.0);
        index.objects.insert(42, ((1, 2), a, [0.0; 3]));
        assert_eq!(indexed_position(&index, (1, 2), 42), Some(a));
        assert_eq!(indexed_position(&index, (3, 4), 42), None);
        index.duplicates.insert(((3, 4), 42), (b, [0.0; 3]));
        assert_eq!(indexed_position(&index, (3, 4), 42), Some(b));
    }

    #[test]
    fn overlapping_streaming_rings_do_not_duplicate_stop_tiles() {
        let keys = [(1, 2), (0, 0), (1, 2), (-1, 3), (0, 0)];
        assert_eq!(unique_keys(&keys), vec![(1, 2), (0, 0), (-1, 3)]);
    }
}
