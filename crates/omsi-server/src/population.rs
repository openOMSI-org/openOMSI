//! Bounded AI vehicle population for the renderer-free experimental host.
//! Real vehicles run their native omsi-sim physics and script programs; no
//! graphical assets or fake client-side AI packets are generated.

use glam::DVec3;
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::traffic::LaneKind;
use omsi_sim::world_data::WorldCell;
use omsi_sim::{SimClock, VehicleHost, VehicleInstance, VehicleType};
use std::sync::Arc;

pub const MAX_EXPERIMENTAL_TRAFFIC: usize = 512;
const MAX_SPAWNS_PER_SECOND: usize = 4;
const RECYCLE_DISTANCE: f64 = 900.0;
const SPAWN_INTERVAL: f32 = 1.0;
const SPAWN_RADIUS: f64 = 390.0;
const MIN_PLAYER_DISTANCE: f64 = 100.0;
const MIN_CAR_GAP: f64 = 24.0;

/// Scheduled line buses have a separate budget; they must not consume random
/// AI slots or make cars disappear when a timetable starts a new departure.
pub fn random_slots(target: usize, active_unscheduled: usize) -> usize {
    target
        .min(MAX_EXPERIMENTAL_TRAFFIC)
        .saturating_sub(active_unscheduled)
        .min(MAX_SPAWNS_PER_SECOND)
}

pub struct CpuTrafficPopulation {
    elapsed: f32,
    state: u64,
}

impl Default for CpuTrafficPopulation {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            state: 0xD1B5_4A32_0713_1EEB,
        }
    }
}

impl CpuTrafficPopulation {
    /// Only ordinary cars follow remote player concentration. Timetable
    /// buses retain their routes regardless of where players are.
    pub fn recycle_distant(traffic: &mut TrafficSim, viewers: &[DVec3]) -> usize {
        use omsi_sim::ai_traffic::control::Removal;
        let mut removed = 0;
        for index in (0..traffic.cars.len()).rev() {
            let car = &traffic.cars[index];
            if car.is_bus() {
                continue;
            }
            let far = viewers
                .iter()
                .all(|at| (car.vehicle.position - *at).truncate().length() > RECYCLE_DISTANCE);
            if far {
                let car = traffic.take_off(index, Removal::Finished);
                traffic.retired.push(car.id);
                removed += 1;
            }
        }
        removed
    }

    fn random(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Take a bounded number of candidates per interval and avoid spawning
    /// in sight of a driver or on a collision-occupied road. Only use locally
    /// installed, map-approved AI types already loaded by traffic::random_types.
    pub fn tick(
        &mut self,
        traffic: &mut TrafficSim,
        data: &Arc<WorldCell>,
        viewers: &[DVec3],
        target: usize,
        clock: &SimClock,
        dt: f32,
    ) -> usize {
        if target == 0 || viewers.is_empty() || traffic.types.is_empty() {
            return 0;
        }
        self.elapsed += dt;
        if self.elapsed < SPAWN_INTERVAL {
            return 0;
        }
        self.elapsed %= SPAWN_INTERVAL;
        let budget = random_slots(
            target,
            traffic.cars.iter().filter(|car| !car.is_bus()).count(),
        );
        let mut placed = 0;
        for _ in 0..budget {
            let near = viewers[(self.random() as usize) % viewers.len()];
            let candidates: Vec<usize> = traffic
                .net
                .lanes_starting_near(near, SPAWN_RADIUS)
                .into_iter()
                .filter(|&i| {
                    let lane = &traffic.net.lanes[i];
                    lane.kind == LaneKind::Street
                        && lane.length() > 35.0
                        && !lane.no_cars
                        && lane.density > 0.0
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let lane_id = candidates[(self.random() as usize) % candidates.len()];
            let lane = &traffic.net.lanes[lane_id];
            let distance = (lane.length() - 12.0).max(1.0);
            let along = 6.0 + ((self.random() % 10_000) as f32 / 10_000.0) * distance;
            let (position, _) = lane.at(along);
            if viewers
                .iter()
                .any(|at| (position - *at).truncate().length() < MIN_PLAYER_DISTANCE)
                || traffic
                    .cars
                    .iter()
                    .any(|car| (position - car.vehicle.position).truncate().length() < MIN_CAR_GAP)
            {
                continue;
            }
            let valid_ground = {
                let d = data.peek();
                let (key, _) = omsi_map::world_to_tile_local(position.x, position.y);
                d.ground.terrains.contains_key(&key) || d.ground.surfaces.contains_key(&key)
            };
            if !valid_ground {
                continue;
            }
            let eligible: Vec<_> = traffic
                .types
                .iter()
                .filter(|entry| entry.2 == LaneKind::Street && entry.1.is_finite() && entry.1 > 0.0)
                .map(|entry| Arc::clone(&entry.0))
                .collect();
            if eligible.is_empty() {
                break;
            }
            let ty: Arc<VehicleType> = eligible[(self.random() as usize) % eligible.len()].clone();
            let seed = self.random();
            let scheme = traffic.ai_scheme(&ty, seed >> 8);
            // Random traffic scripts use the host's actual date/time.
            // SimClock::default() would silently start every AI car in 1989
            // at 09:00, even on an evening Chrono timetable.
            let mut host = VehicleHost::new(clock.clone());
            host.paint_scheme = Some(scheme);
            let mut vehicle = VehicleInstance::new(ty.clone(), host);
            attach_cpu_ground(&mut vehicle, data);
            vehicle.apply_paint_vars(scheme);
            traffic.place_car(
                vehicle,
                LaneKind::Street,
                lane_id,
                along,
                ty,
                seed,
                scheme,
                None,
                None,
                None,
            );
            placed += 1;
        }
        placed
    }
}

/// Resolve a smooth drivable surface, or the visible ground only where the
/// terrain is NOT cut out. A tunnel's [terrainhole] is open air below its road
/// mesh, not an invisible mound for AI vehicle suspension and placement.
fn cpu_ground_height(ground: &omsi_sim::world_data::GroundTiles, x: f64, y: f64) -> Option<f64> {
    let (key, (lx, ly)) = omsi_map::world_to_tile_local(x, y);
    let (lx, ly) = (lx as f32, ly as f32);
    let surface = ground.surfaces.get(&key);
    if let Some(road) = surface.and_then(|s| s.sample_road_smooth(lx, ly)) {
        return Some(f64::from(road));
    }
    let terrain = ground.terrains.get(&key)?;
    let height = terrain.sample(lx, ly);
    if surface.is_some_and(|s| s.cut_at(lx, ly, height, omsi_sim::world_data::surface_flush())) {
        return None;
    }
    Some(f64::from(height))
}

/// Attach the same road/terrain and exact wheel-contact samplers used by
/// the game, but via shared immutable CPU map snapshots rather than a Scene.
pub fn attach_cpu_ground(vehicle: &mut VehicleInstance, data: &Arc<WorldCell>) {
    let ground = Arc::clone(data);
    vehicle.ground = Some(Arc::new(move |x, y| {
        let d = ground.peek();
        cpu_ground_height(&d.ground, x, y)
    }));
    let ground = Arc::clone(data);
    vehicle.contact = Some(
        Arc::new(move |x, y, top| ground.peek().ground.drive_probe(x, y, top))
            as Arc<dyn omsi_sim::rigid::Ground>,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_sim::ai_traffic::setup::RandomTypes;
    use omsi_sim::traffic::Network;
    use omsi_sim::world_data::WorldData;

    #[test]
    fn cpu_ai_ground_never_uses_terrain_inside_an_explicit_tunnel_cutout() {
        use glam::{Mat4, Vec3};
        use omsi_geometry::{MeshData, TileSurface};
        use omsi_map::Terrain;
        use omsi_sim::world_data::GroundTiles;

        let hole = MeshData {
            positions: vec![
                Vec3::new(4.0, 4.0, 0.0),
                Vec3::new(16.0, 4.0, 0.0),
                Vec3::new(16.0, 16.0, 0.0),
                Vec3::new(4.0, 16.0, 0.0),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        };
        let mut raster = TileSurface::new(128);
        raster.rasterize_hole(&hole, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
        raster.finish();
        let mut ground = GroundTiles::default();
        ground.terrains.insert((0, 0), Arc::new(Terrain::flat()));
        ground.surfaces.insert((0, 0), Arc::new(raster));

        assert_eq!(cpu_ground_height(&ground, 10.0, 10.0), None);
        assert_eq!(cpu_ground_height(&ground, 30.0, 30.0), Some(0.0));
    }

    #[test]
    fn cannot_create_vehicles_without_connected_players_or_ai_types() {
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut traffic = crate::traffic::assemble(
            std::path::Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            10,
        );
        let data = Arc::new(WorldCell::new(WorldData::default()));
        let mut population = CpuTrafficPopulation::default();
        let clock = SimClock::default();
        assert_eq!(
            population.tick(&mut traffic, &data, &[], 10, &clock, 1.0),
            0
        );
        assert_eq!(
            population.tick(&mut traffic, &data, &[DVec3::ZERO], 10, &clock, 1.0),
            0
        );
        assert!(traffic.cars.is_empty());
    }

    #[test]
    fn ordinary_traffic_max_and_spawn_rate_are_bounded() {
        assert_eq!(random_slots(64, 64), 0);
        assert_eq!(random_slots(128, 0), MAX_SPAWNS_PER_SECOND);
        assert_eq!(random_slots(128, 120), MAX_SPAWNS_PER_SECOND);
    }

    #[test]
    fn scheduled_buses_do_not_consume_random_ai_capacity() {
        // Scheduled buses are counted separately from ordinary cars.
        // Population growth is capped at four ordinary spawns per second.
        assert_eq!(random_slots(8, 0), MAX_SPAWNS_PER_SECOND);
        assert_eq!(random_slots(8, 6), 2);
        assert_eq!(random_slots(8, 7), 1);
        assert_eq!(random_slots(8, 8), 0);
        assert_eq!(random_slots(0, 0), 0);
        assert_eq!(random_slots(usize::MAX, 507), MAX_SPAWNS_PER_SECOND);
        assert_eq!(random_slots(usize::MAX, 511), 1);
        assert_eq!(random_slots(usize::MAX, 512), 0);
        assert_eq!(random_slots(usize::MAX, 600), 0);
    }

    #[test]
    fn newly_spawned_ai_uses_the_live_calendar_after_midnight() {
        let mut calendar = crate::clock::NativeCalendar::new("2024-02-28", "23:59");
        let now = calendar.at(2.0 * 86_400.0 + 78_000.0).clone();
        let host = VehicleHost::new(now);
        assert_eq!(host.clock.date_code(), 20240301);
        assert_eq!(host.clock.time, 78_000.0);
    }
}
