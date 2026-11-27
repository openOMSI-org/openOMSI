//! Experimental renderer-free timetable dispatcher. It reuses the game's
//! ScheduleSim calendar, Chrono, trip routing and AI bus service callbacks.
//! Rail consists and complex fleet rendering are intentionally excluded.

use crate::population::attach_cpu_ground;
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::ai_traffic::bus_service::Stop;
use omsi_sim::ai_traffic::model::BusSetup;
use omsi_sim::timetable_run::{Placed, Placing, ScheduleSim};
use omsi_sim::traffic::LaneKind;
use omsi_sim::world_data::WorldCell;
use omsi_sim::{SimClock, VehicleHost, VehicleInstance};
use std::path::Path;
use std::sync::Arc;

/// Conservative default for servers without an explicit AI timetable budget.
pub const MAX_EXPERIMENTAL_SCHEDULED: usize = 12;
/// Bound explicitly configured native timetable fleets without silently
/// overriding server.cfg values such as ai_max_scheduled=60.
pub const MAX_CONFIGURED_SCHEDULED: usize = 64;

/// The legacy server.cfg defines 0 as "no configured limit". A native
/// experimental host defaults to 12 for bounded CPU testing, but honours
/// positive configured limits up to 64. Use the same helper on boot and reload.
pub fn scheduled_budget(setting: Option<u32>) -> usize {
    setting
        .filter(|&count| count > 0)
        .map(|count| (count as usize).min(MAX_CONFIGURED_SCHEDULED))
        .unwrap_or(MAX_EXPERIMENTAL_SCHEDULED)
}

pub struct CpuTimetable {
    pub sim: ScheduleSim,
    elapsed: f32,
    starting: bool,
    max: usize,
}

impl CpuTimetable {
    pub fn new(root: &Path, world: &Arc<WorldCell>, clock: &SimClock, limit: usize) -> Self {
        let snapshot = world.snapshot();
        let sim = ScheduleSim::new(root, &*snapshot, clock);
        log::info!(
            "native timetable: {} trips, {} lines, {} scheduled vehicles maximum",
            sim.data.trips.len(),
            sim.data.lines.len(),
            limit.min(MAX_CONFIGURED_SCHEDULED)
        );
        Self {
            sim,
            elapsed: 0.0,
            starting: true,
            max: limit.min(MAX_CONFIGURED_SCHEDULED),
        }
    }

    /// Once per simulated second, queue departures and place at most one
    /// genuinely routable scheduled bus on the CPU map.
    /// Applies only to future departures. Existing buses may finish their trip
    /// rather than being despawned in front of clients on a config reload.
    pub fn set_limit(&mut self, max: usize) {
        self.max = max.min(MAX_CONFIGURED_SCHEDULED);
    }

    pub fn tick(
        &mut self,
        root: &Path,
        world: &Arc<WorldCell>,
        traffic: &mut TrafficSim,
        clock: &SimClock,
        dt: f32,
    ) {
        self.elapsed += dt;
        if self.elapsed < 1.0 {
            return;
        }
        self.elapsed %= 1.0;
        let snapshot = world.snapshot();
        let now = traffic.day_time;
        for id in self.sim.begin_tick(now) {
            if let Some(ci) = traffic.cars.iter().position(|c| c.id == id) {
                traffic.release(ci);
            }
        }
        self.sim.queue_due(
            &*snapshot,
            traffic,
            now,
            if self.starting { 20.0 * 60.0 } else { 30.0 },
        );
        self.starting = false;
        self.handover(&*snapshot, traffic, now);
        let mut tried = 0;
        while self.sim.car_ids().len() < self.max && tried < 4 {
            let Some(i) = self.sim.next_pending() else {
                break;
            };
            tried += 1;
            let result = self.spawn(root, world, traffic, clock, i, now);
            self.sim.settle(i, &result, now);
            if matches!(result, Placed::Spawned) {
                break;
            }
        }
    }

    fn handover(
        &mut self,
        world: &dyn omsi_sim::timetable_run::TimetableWorld,
        traffic: &mut TrafficSim,
        now: f64,
    ) {
        let done: Vec<u64> = traffic
            .cars
            .iter()
            .filter(|c| c.trip_done())
            .map(|c| c.id)
            .collect();
        for id in done {
            let Some(ci) = traffic.cars.iter().position(|car| car.id == id) else {
                continue;
            };
            if self.sim.tour_held(id, now) {
                continue;
            }
            let next = self.sim.next_of_car(id);
            let mut taken = false;
            if let Some(i) = next {
                if self.sim.may_take_on(i, now) {
                    let placed = match self.sim.place_departure(i, world, traffic, now, Some(ci)) {
                        Ok(route) => self.sim.take_trip_on(i, ci, route, traffic, now),
                        Err(placed) => placed,
                    };
                    if matches!(placed, Placed::Spawned) {
                        self.sim.taken_on(i);
                        taken = true;
                    } else if matches!(placed, Placed::Early) {
                        continue;
                    }
                }
                if !taken {
                    self.sim.not_taken_on(i);
                }
            }
            if !taken {
                traffic.release(ci);
                self.sim.forget_car(id);
            }
        }
        self.sim.requeue_orphans(traffic);
    }

    fn spawn(
        &mut self,
        _root: &Path,
        world: &Arc<WorldCell>,
        traffic: &mut TrafficSim,
        clock: &SimClock,
        i: usize,
        now: f64,
    ) -> Placed {
        let snapshot = world.snapshot();
        let p: Placing = match self.sim.place_departure(i, &*snapshot, traffic, now, None) {
            Ok(route) => route,
            Err(status) => return status,
        };
        // Restrict this first native dispatcher to single road buses, not
        // trains, coupled consists or aircraft whose route spans unloaded tiles.
        let Some(choice) = self.sim.choose(i, &*snapshot) else {
            return Placed::Drop;
        };
        if choice.train.is_some() || p.section.is_empty() {
            return Placed::Wait;
        }
        let lane_id = p.section[p.start_index.min(p.section.len() - 1)];
        if traffic.net.lanes[lane_id].kind != LaneKind::Street {
            return Placed::Wait;
        }
        let s = p.s.clamp(0.0, traffic.net.lanes[lane_id].length());
        let at = traffic.net.lanes[lane_id].at(s).0;
        let (key, _) = omsi_map::world_to_tile_local(at.x, at.y);
        if !snapshot.ground.terrains.contains_key(&key)
            && !snapshot.ground.surfaces.contains_key(&key)
        {
            return Placed::Wait;
        }
        if traffic
            .cars
            .iter()
            .any(|car| (car.vehicle.position - at).truncate().length() < 16.0)
        {
            return Placed::Busy;
        }
        let scheme = choice.scheme;
        let timetable = self.sim.ai_timetable(i);
        let mut stops: Vec<Stop> = p
            .stops
            .iter()
            .filter(|stop| stop.0 >= p.start_index)
            .map(|&(ri, d, bay, time, id, side)| {
                Stop::from_tuple((ri - p.start_index, d, bay, time, id, side))
            })
            .collect();
        stops.sort_by(|a, b| a.ri.cmp(&b.ri).then(a.s.total_cmp(&b.s)));
        let mut host = VehicleHost::new(clock.clone());
        host.hof = choice.hof.clone();
        host.paint_scheme = Some(scheme);
        timetable.install(&mut host, stops.first());
        let mut vehicle = VehicleInstance::new(choice.ty.clone(), host);
        if let Some((number, reg)) = &choice.number {
            for (name, text) in [("number", number), ("ident", reg)] {
                if let Some(slot) = choice.ty.program.str_var(name) {
                    vehicle.state.str_vars[slot as usize] = text.clone();
                }
            }
        }
        attach_cpu_ground(&mut vehicle, world);
        vehicle.apply_paint_vars(scheme);
        let setup = BusSetup {
            route: p.section[p.start_index..].to_vec(),
            stops,
            number: choice.number,
            hof: choice.hof,
            timetable,
        };
        let id = traffic.place_car(
            vehicle,
            LaneKind::Street,
            lane_id,
            s,
            choice.ty,
            i as u64 + 0xE995_D7A1,
            scheme,
            None,
            None,
            Some(setup),
        );
        self.sim.bus_runs(id, i);
        self.sim.started(i);
        log::info!(
            "native timetable: scheduled road bus {id} for {}",
            p.trip_name
        );
        Placed::Spawned
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn legacy_zero_and_missing_limits_use_the_experimental_safety_cap() {
        assert_eq!(scheduled_budget(None), MAX_EXPERIMENTAL_SCHEDULED);
        assert_eq!(scheduled_budget(Some(0)), MAX_EXPERIMENTAL_SCHEDULED);
        assert_eq!(scheduled_budget(Some(5)), 5);
        assert_eq!(scheduled_budget(Some(12)), 12);
        assert_eq!(scheduled_budget(Some(60)), 60);
        assert_eq!(scheduled_budget(Some(64)), MAX_CONFIGURED_SCHEDULED);
        assert_eq!(scheduled_budget(Some(10_000)), MAX_CONFIGURED_SCHEDULED);
    }
}
