//! CPU-only traffic-light programs for the native dedicated host.
//!
//! A signal controller is keyed by its placed crossing object, not by a GPU
//! scene instance. The same production TrafficLightController interprets
//! traffic_light, stop and jump phases. Parenting scripted signal lamps,
//! dynamic crossings and rail interlocks still require validation.

use crate::world::HostWorld;
use hashbrown::HashMap;
use omsi_scenery::SceneryObject;
use omsi_sim::traffic::{Lane, Network, TrafficLightController};
use std::collections::BTreeSet;

#[derive(Default)]
pub struct SignalNetwork {
    pub network: Network,
    pub controllers: Vec<TrafficLightController>,
    /// Production TrafficSim uses these object IDs for cycle state and LAN.
    pub controller_of_object: HashMap<i64, usize>,
    pub duplicate_ids: usize,
    pub missing_definitions: usize,
    pub attached_objects: usize,
}

impl SignalNetwork {
    pub fn partial(&self) -> bool {
        self.duplicate_ids > 0 || self.missing_definitions > 0 || self.attached_objects > 0
    }
}

fn program_enabled(def: &SceneryObject, map_has_signals: bool) -> bool {
    !def.traffic_lights.is_empty() && (def.is_traffic_light || map_has_signals)
}

fn controller(def: &SceneryObject) -> TrafficLightController {
    let phases = def
        .traffic_lights
        .iter()
        .map(|light| {
            (
                light.phases.iter().map(|p| (p.state, p.duration)).collect(),
                light.approach_dist,
            )
        })
        .collect();
    TrafficLightController::from_program(
        phases,
        def.traffic_lights_group,
        &def.traffic_light_stop,
        &def.traffic_light_jump,
    )
}

type MapSignal = ((i32, i32), i64);
type LaneBindings = HashMap<MapSignal, (usize, Vec<i32>)>;

fn bind_lanes(
    lanes: &mut [Lane],
    bindings: &LaneBindings,
    lights: &[TrafficLightController],
) -> usize {
    let mut count = 0;
    for lane in lanes {
        // Source 2 means a placed scenery object's [path], not a spline or
        // generated through-road. IDs may repeat between these source types.
        if lane.source != 2 {
            continue;
        }
        let Some(key) = lane.key else { continue };
        let Some((controller, path_lights)) = bindings.get(&(key.tile, key.id)) else {
            continue;
        };
        let Some(program) = lights.get(*controller) else {
            // A partially loaded map may not have published all its crossing
            // programs yet; never panic during a network update.
            continue;
        };
        let Some(light) = path_lights
            .get(key.path as usize)
            .copied()
            .filter(|light| *light >= 0 && (*light as usize) < program.lights.len())
        else {
            continue;
        };
        lane.traffic_light = Some((*controller, light as usize));
        count += 1;
    }
    count
}

impl HostWorld {
    /// Build the same CPU road network plus crossing light programs, in stable
    /// tile/object order. No renderer and no local driver bus are instantiated.
    /// This does not yet solve scripted parent lamps, rail signaling or
    /// timetable signal dependencies, so it is not a playable-server gate.
    pub fn network_with_traffic_lights(&mut self, keys: &[(i32, i32)]) -> SignalNetwork {
        let mut output = SignalNetwork {
            network: self.network_for_tiles(keys),
            ..Default::default()
        };
        let mut binding = LaneBindings::new();
        let root = self.data.snapshot().map.root.clone();

        for key in keys.iter().copied().collect::<BTreeSet<_>>() {
            let Some(tile) = self.load_tile(key) else {
                continue;
            };
            for obj in &tile.objects {
                if obj.parent_id.is_some() {
                    output.attached_objects += 1;
                    continue;
                }
                if obj.file.trim().is_empty() {
                    continue;
                }
                let Some(def) = self.scenery(&root, &obj.file) else {
                    output.missing_definitions += 1;
                    continue;
                };
                if !program_enabled(&def, self.index.traffic_light_parents.contains(&obj.id)) {
                    continue;
                }
                if output.controller_of_object.contains_key(&obj.id) {
                    // Legacy controllers identify objects by their numeric ID.
                    // Never let a duplicate silently control another tile.
                    output.duplicate_ids += 1;
                    continue;
                }
                let slot = output.controllers.len();
                output.controllers.push(controller(&def));
                output.controller_of_object.insert(obj.id, slot);
                binding.insert((key, obj.id), (slot, def.path_traffic_light.clone()));
            }
        }
        let count = bind_lanes(&mut output.network.lanes, &binding, &output.controllers);
        log::info!(
            "CPU signal programs: {} crossings, {} signal-controlled lanes, {} duplicate IDs",
            output.controllers.len(),
            count,
            output.duplicate_ids
        );
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use omsi_sim::traffic::{LaneBuilder, LaneKey, LaneKind};

    #[test]
    fn only_object_paths_bind_to_their_own_controller() {
        let mut a = LaneBuilder::polyline(
            vec![DVec3::ZERO, DVec3::new(0.0, 12.0, 0.0)],
            LaneKind::Street,
            3.0,
        );
        a.key = Some(LaneKey {
            tile: (0, 0),
            id: 77,
            path: 0,
        });
        a.source = 2;
        let mut other = a.clone();
        other.source = 1;
        let mut another_tile = a.clone();
        another_tile.key = Some(LaneKey {
            tile: (1, 0),
            id: 77,
            path: 0,
        });
        let mut bindings = LaneBindings::new();
        bindings.insert(((0, 0), 77), (0, vec![0]));
        let lights = vec![TrafficLightController::new(vec![vec![(0, 5.0)]], 5.0)];
        let mut lanes = vec![a, other, another_tile];
        assert_eq!(bind_lanes(&mut lanes, &bindings, &lights), 1);
        assert_eq!(lanes[0].traffic_light, Some((0, 0)));
        assert_eq!(lanes[1].traffic_light, None);
        assert_eq!(lanes[2].traffic_light, None);
    }

    #[test]
    fn invalid_signal_index_cannot_be_attached_to_a_lane() {
        let mut a = LaneBuilder::polyline(vec![DVec3::ZERO, DVec3::Y], LaneKind::Street, 3.0);
        a.source = 2;
        a.key = Some(LaneKey {
            tile: (0, 0),
            id: 1,
            path: 0,
        });
        let mut bindings = LaneBindings::new();
        bindings.insert(((0, 0), 1), (0, vec![100]));
        let mut lanes = vec![a];
        assert_eq!(
            bind_lanes(
                &mut lanes,
                &bindings,
                &[TrafficLightController::new(vec![vec![(0, 5.0)]], 5.0)]
            ),
            0
        );
        assert!(lanes[0].traffic_light.is_none());
    }
}
