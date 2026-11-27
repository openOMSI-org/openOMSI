//! Interest-based CPU tile selection for the experimental headless host.
//!
//! Prefer the tiles under connected players, then nearby neighborhoods,
//! without dropping the initial map entrypoint area. This is a bounded
//! selection policy; unloading world entities and AI remains separate work.

use omsi_map::global::GlobalCfg;
use std::collections::BTreeSet;

pub const STREAM_TILE_LIMIT: usize = 192;
/// Until entity-aware unloading can retire old lane indices, bound total
/// populated tiles instead of accumulating the entire map indefinitely.
pub const MAX_POPULATED_TILES: usize = 512;
pub const STREAM_NEIGHBOR_RADIUS: i32 = 2;

/// Request only unloaded tiles, keeping the player's prioritized order and
/// the global retained-memory budget. A stopped worker must never silently
/// receive more lanes than the CPU world can retain.
pub fn populated_new_tiles(
    desired: &[(i32, i32)],
    loaded: &BTreeSet<(i32, i32)>,
    limit: usize,
) -> Vec<(i32, i32)> {
    let budget = limit.saturating_sub(loaded.len());
    let mut selected = BTreeSet::new();
    desired
        .iter()
        .copied()
        .filter(|key| !loaded.contains(key) && selected.insert(*key))
        .take(budget)
        .collect()
}

pub fn interest_tiles(
    global: &GlobalCfg,
    initial: &[(i32, i32)],
    player_positions: &[(f64, f64)],
    limit: usize,
) -> Vec<(i32, i32)> {
    if limit == 0 {
        return Vec::new();
    }
    let known: BTreeSet<_> = global.tiles.iter().map(|tile| (tile.x, tile.y)).collect();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut add = |key: (i32, i32)| {
        if selected.len() < limit && known.contains(&key) && seen.insert(key) {
            selected.push(key);
        }
    };
    let centers: Vec<_> = player_positions
        .iter()
        .filter(|(x, y)| x.is_finite() && y.is_finite() && x.abs() <= 1.0e8 && y.abs() <= 1.0e8)
        .map(|&(x, y)| omsi_map::world_to_tile_local(x, y).0)
        .filter(|key| known.contains(key))
        .collect();

    // Never let a distant player lose their own tile to the initial 64.
    for &center in &centers {
        add(center);
    }
    // The original spawn neighborhood stays present for late joiners.
    for &key in initial {
        add(key);
    }
    // Expand one ring at a time across all players: a crowded server must
    // not allocate the entire tile budget to its first connected player.
    for radius in 1..=STREAM_NEIGHBOR_RADIUS {
        for &(cx, cy) in &centers {
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    if let (Some(x), Some(y)) = (cx.checked_add(dx), cy.checked_add(dy)) {
                        add((x, y));
                    }
                }
            }
        }
    }
    selected
}

/// Completed CPU map snapshot for the free-roam host. Loading is never
/// performed on the network tick thread.
pub struct StreamUpdate {
    pub keys: Vec<(i32, i32)>,
    pub signals: crate::signals::SignalNetwork,
    pub surfaces: crate::surfaces::SurfaceCounts,
    pub collisions: crate::collision::CollisionCounts,
}

/// Swap the completed lane/signal snapshot on the simulation thread without
/// restarting the LAN session or resetting the other connected drivers.
pub fn apply_free_roam_update(
    sim: &mut crate::simulation::CpuSimulation,
    root: &std::path::Path,
    density: &[(f32, f32)],
    update: StreamUpdate,
) -> Vec<(i32, i32)> {
    let old_clocks: Vec<_> = sim
        .traffic
        .controller_of_object
        .iter()
        .filter_map(|(object, slot)| {
            sim.traffic
                .lights
                .get(*slot)
                .map(|ctl| (*object, ctl.time, ctl.held))
        })
        .collect();
    let random = omsi_sim::ai_traffic::setup::RandomTypes {
        types: Vec::new(),
        groups: Vec::new(),
        group_curves: false,
        group_uvg: Vec::new(),
        uvg_defaults: Vec::new(),
    };
    let mut next_traffic = crate::traffic::assemble(
        root,
        update.signals.network,
        random,
        density.to_vec(),
        update.keys.clone(),
        0,
    );
    next_traffic.lights = update.signals.controllers;
    next_traffic.controller_of_object = update.signals.controller_of_object;
    next_traffic.day_time = sim.traffic.day_time;
    next_traffic.time_scale = sim.traffic.time_scale;
    for (object, time, held) in old_clocks {
        next_traffic.set_light_state(object, time, held);
    }
    sim.traffic = next_traffic;
    log::info!(
        "native tile stream: {} active tiles, {} road lanes, {} wheel meshes, {} boxes and {} collision meshes, {} pending surfaces",
        update.keys.len(),
        sim.traffic.net.lanes.len(),
        update.surfaces.wheel_meshes,
        update.collisions.static_boxes,
        update.collisions.static_meshes,
        update.surfaces.pending_scenery_surfaces
    );
    update.keys
}

/// Append CPU map tiles without renumbering lanes already referenced by AI
/// routes, scheduled buses, pedestrian paths or light programs. Unlike the
/// free-roam relay, a populated region never replaces the entire network.
pub fn apply_populated_update(
    sim: &mut crate::simulation::CpuSimulation,
    update: StreamUpdate,
) -> Vec<(i32, i32)> {
    let mut signals = update.signals;
    let mut controllers: Vec<_> = signals.controllers.drain(..).map(Some).collect();
    let mut slots = vec![None; controllers.len()];
    let mut definitions: Vec<_> = signals.controller_of_object.into_iter().collect();
    definitions.sort_by_key(|(object, local)| (*local, *object));

    for (object, local) in definitions {
        if local >= controllers.len() {
            continue;
        }
        let global = if let Some(&existing) = sim.traffic.controller_of_object.get(&object) {
            existing
        } else {
            let Some(controller) = controllers[local].take() else {
                continue;
            };
            let index = sim.traffic.lights.len();
            sim.traffic
                .light_prev
                .push(vec![-100; controller.lights.len()]);
            sim.traffic.lights.push(controller);
            sim.traffic.controller_of_object.insert(object, index);
            index
        };
        slots[local] = Some(global);
    }
    for lane in &mut signals.network.lanes {
        if let Some((controller, light)) = lane.traffic_light {
            lane.traffic_light = slots
                .get(controller)
                .and_then(|slot| *slot)
                .map(|global| (global, light));
        }
    }
    let range = sim
        .traffic
        .net
        .extend(signals.network.lanes, omsi_sim::traffic::LINK_TOLERANCE);
    sim.traffic.set_spawn_densities(range.clone());
    sim.traffic.street_weight += sim.traffic.net.lanes[range.clone()]
        .iter()
        .filter_map(omsi_sim::ai_traffic::dormant::street_lane_weight)
        .sum::<f64>();
    for &key in &update.keys {
        sim.traffic.lane_tiles.insert(key);
    }
    sim.traffic.lanes_generation = sim.traffic.lanes_generation.wrapping_add(1);
    log::info!(
        "native populated tile stream: {} more tiles, {} added lanes ({} total), {} live AI vehicles and {} pedestrians; existing routes retained",
        update.keys.len(),
        range.len(),
        sim.traffic.net.lanes.len(),
        sim.traffic.cars.len(),
        sim.people.people.len()
    );
    update.keys
}

pub struct StreamWorker {
    requests: std::sync::mpsc::Sender<Vec<(i32, i32)>>,
    results: std::sync::mpsc::Receiver<Option<StreamUpdate>>,
    worker: std::thread::JoinHandle<()>,
}

impl StreamWorker {
    /// Own the mutable CPU map on a separate thread. Shared WorldCell data is
    /// published via immutable snapshots and read by the running simulation.
    /// The restricted relay has no server-owned AI, so old entities cannot
    /// retain lane or surface references during a region replacement.
    pub fn start(world: crate::world::HostWorld) -> std::io::Result<Self> {
        Self::start_mode(world, false, Vec::new())
    }

    /// The renderer-free AI host may add new regions but must never prune
    /// ground or replace the lane network out from beneath running scripts.
    pub fn start_populated(
        world: crate::world::HostWorld,
        initial: Vec<(i32, i32)>,
    ) -> std::io::Result<Self> {
        Self::start_mode(world, true, initial)
    }

    fn start_mode(
        mut world: crate::world::HostWorld,
        populated: bool,
        initial: Vec<(i32, i32)>,
    ) -> std::io::Result<Self> {
        let (requests, pending) = std::sync::mpsc::channel::<Vec<(i32, i32)>>();
        let (complete, results) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("omsi-cpu-tile-stream".into())
            .spawn(move || {
                let mut retained: BTreeSet<_> = initial.into_iter().collect();
                while let Ok(requested) = pending.recv() {
                    // In simulated mode the thread only parses new tiles.
                    // Older map regions stay attached to live AI route indices.
                    let keys: Vec<_> = if populated {
                        requested
                            .into_iter()
                            .filter(|key| retained.insert(*key))
                            .collect()
                    } else {
                        requested
                    };
                    if keys.is_empty() {
                        if complete.send(None).is_err() {
                            break;
                        }
                        continue;
                    }
                    let signals = world.network_with_traffic_lights(&keys);
                    if signals.network.lanes.is_empty() && !populated {
                        log::warn!("native tile stream: no road lanes in the selected region");
                        if complete.send(None).is_err() {
                            break;
                        }
                        continue;
                    }
                    let surfaces = world.install_spline_ground_surfaces(&keys);
                    let collisions = if populated {
                        // Both functions REPLACE a complete map-wide list.
                        // Rebuild from the cumulative keys, never from just
                        // the incremental request, or old waiting places and
                        // collision obstacles would disappear mid-session.
                        let all: Vec<_> = retained.iter().copied().collect();
                        world.install_stop_lists(&all);
                        let all_lanes = world.network_for_tiles(&all);
                        world.install_static_box_collisions(&all, &all_lanes.lanes)
                    } else {
                        world.install_stop_lists(&keys);
                        let collision =
                            world.install_static_box_collisions(&keys, &signals.network.lanes);
                        world.prune_streamed_ground(&keys);
                        collision
                    };
                    if complete
                        .send(Some(StreamUpdate {
                            keys,
                            signals,
                            surfaces,
                            collisions,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests,
            results,
            worker,
        })
    }

    pub fn request(&self, keys: Vec<(i32, i32)>) -> bool {
        self.requests.send(keys).is_ok()
    }

    /// Ok(None) means the worker rejected the requested region; Empty means
    /// still processing; Disconnected is fatal to future map streaming.
    pub fn poll(&self) -> Result<Option<StreamUpdate>, std::sync::mpsc::TryRecvError> {
        self.results.try_recv()
    }

    pub fn stop(self) {
        let Self {
            requests,
            results: _,
            worker,
        } = self;
        drop(requests);
        if worker.join().is_err() {
            log::warn!("native tile stream worker panicked while shutting down");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_map::global::MapTileRef;

    fn fixture() -> GlobalCfg {
        let mut global = GlobalCfg::default();
        for x in 0..20 {
            for y in 0..20 {
                global.tiles.push(MapTileRef {
                    x,
                    y,
                    ..Default::default()
                });
            }
        }
        global
    }

    #[test]
    fn populated_tiles_stop_at_the_retained_memory_budget() {
        let current = BTreeSet::from([(0, 0), (1, 0), (2, 0)]);
        let desired = [(0, 0), (5, 0), (5, 0), (6, 0), (7, 0), (8, 0)];
        assert_eq!(
            populated_new_tiles(&desired, &current, 5),
            vec![(5, 0), (6, 0)],
        );
        assert!(populated_new_tiles(&desired, &current, 2).is_empty());
        assert!(populated_new_tiles(&desired, &current, 3).is_empty());
        assert_eq!(
            populated_new_tiles(&desired, &current, MAX_POPULATED_TILES).len(),
            4
        );
    }

    #[test]
    fn worker_builds_a_distant_drivable_cpu_region_without_a_renderer() {
        use std::fs;
        use std::time::{Duration, Instant};
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("omsi-stream-{}-{stamp}", std::process::id()));
        let map_dir = root.join("Maps/Fixture");
        fs::create_dir_all(&map_dir).unwrap();
        fs::create_dir_all(root.join("Splines")).unwrap();
        fs::write(
            map_dir.join("global.cfg"),
            "[name]\nStreaming Fixture\n[map]\n0\n0\ntile_0_0.map\n[map]\n20\n0\ntile_20_0.map\n",
        )
        .unwrap();
        let tile = "[version]\n10\n[spline]\n0\nSplines/smoke.sli\n101\n-1\n10\n0\n10\n0\n40\n0\n0\n0\n0\n0\n0\n";
        fs::write(map_dir.join("tile_0_0.map"), tile).unwrap();
        fs::write(map_dir.join("tile_20_0.map"), tile).unwrap();
        fs::write(
            root.join("Splines/smoke.sli"),
            "[length]\n40\n[texture]\nroad.bmp\n[profile]\n0\n[profilepnt]\n-1.5\n0\n0\n1\n[profilepnt]\n1.5\n0\n1\n1\n[path]\n0\n0\n0\n3\n0\n",
        )
        .unwrap();
        omsi_cfg::add_content_root(root.clone());
        let world =
            crate::world::HostWorld::open(&root, &map_dir.join("global.cfg"), 19890530).unwrap();
        let worker = StreamWorker::start(world).unwrap();
        assert!(worker.request(vec![(20, 0)]));
        let until = Instant::now() + Duration::from_secs(30);
        let mut streamed = false;
        while Instant::now() < until {
            match worker.poll() {
                Ok(Some(update)) => {
                    assert_eq!(update.keys, vec![(20, 0)]);
                    assert!(
                        !update.signals.network.lanes.is_empty(),
                        "remote tile must have CPU road lanes"
                    );
                    assert_eq!(update.surfaces.target_tiles, 1);
                    streamed = true;
                    break;
                }
                Ok(None) => panic!("stream worker rejected a valid road tile"),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("stream worker disconnected: {error}"),
            }
        }
        worker.stop();
        omsi_cfg::remove_content_root(&root);
        fs::remove_dir_all(&root).unwrap();
        assert!(
            streamed,
            "worker did not return the distant CPU map snapshot"
        );
    }

    #[test]
    fn remote_driver_gets_their_tile_and_neighbors_not_just_spawn_area() {
        let global = fixture();
        let edge = omsi_map::tile_size();
        let selected = interest_tiles(
            &global,
            &[(0, 0), (1, 0)],
            &[(15.5 * edge, 15.5 * edge)],
            40,
        );
        assert_eq!(&selected[..3], &[(15, 15), (0, 0), (1, 0)]);
        assert!(selected.contains(&(14, 15)));
        assert!(selected.contains(&(16, 16)));
        assert_eq!(
            selected.len(),
            selected.iter().copied().collect::<BTreeSet<_>>().len()
        );
    }

    #[test]
    fn preserve_all_player_centers_before_the_small_tile_budget() {
        let global = fixture();
        let edge = omsi_map::tile_size();
        let selected = interest_tiles(
            &global,
            &[(0, 0), (1, 0)],
            &[(2.5 * edge, 2.5 * edge), (17.5 * edge, 17.5 * edge)],
            2,
        );
        assert_eq!(selected, vec![(2, 2), (17, 17)]);
    }

    #[test]
    fn invalid_and_outside_coordinates_never_expand_the_world() {
        let global = fixture();
        let selected = interest_tiles(
            &global,
            &[(0, 0), (1, 0)],
            &[(f64::NAN, 0.0), (f64::INFINITY, 0.0), (1.0e9, 0.0)],
            STREAM_TILE_LIMIT,
        );
        assert_eq!(selected, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn valid_clients_fairly_share_one_ring_when_the_budget_is_tight() {
        let global = fixture();
        let edge = omsi_map::tile_size();
        let selected = interest_tiles(
            &global,
            &[],
            &[(5.5 * edge, 5.5 * edge), (14.5 * edge, 14.5 * edge)],
            6,
        );
        assert_eq!(&selected[..2], &[(5, 5), (14, 14)]);
        assert!(
            selected
                .iter()
                .any(|&(x, y)| x.abs_diff(5) == 1 || y.abs_diff(5) == 1)
        );
    }
}
