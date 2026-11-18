//! The departures due each frame put on the road (which are due is omsi-sim's
//! `ScheduleSim`), tours handing their bus on, buses taken off.

use super::*;
use omsi_render::{Renderer, Scene};

/// Departures tried at most in one look for due ones (`Schedule::tick`) once the session
/// has started: a few per call, the rest in the next.
pub(crate) const RETRIES_PER_CALL: usize = 6;

impl Schedule {
    /// The clock was set to another time (by hand, the menu, Ctrl+Shift+Page Up/Down): every
    /// timetable bus goes, and at the next tick each trip under way at the new time is put
    /// out where its timetable has it then, as when the game starts - as Omsi.exe does when
    /// the time is changed. They stayed where they were, the whole timetable running hours
    /// early or late: buses queued at stops, waiting there for their time (#1607, #1455).
    /// (The renders of the buses taken off go with `view_sync::apply_traffic` after it.)
    pub fn restart(&mut self, traffic: &mut Traffic, day_time: f64) {
        if traffic.is_mirror() {
            return;
        }
        let gone: Vec<u64> = self.sim.car_ids();
        let mut n = 0;
        for id in gone {
            self.sim.forget_car(id);
            if traffic.remove_car(id) {
                n += 1;
            }
        }
        self.sim.restart_clock(day_time, n);
    }

    /// The timetable buses at the end of their trip: each takes its tour's next trip on
    /// where it stands, or goes.
    pub(super) fn tour_handover(
        &mut self,
        world: &World,
        traffic: &mut Traffic,
        day_time: f64,
    ) {
        let done: Vec<u64> = traffic.cars.iter().filter(|c| c.trip_done()).map(|c| c.id).collect();
        for id in done {
            let Some(ci) = traffic.cars.iter().position(|c| c.id == id) else { continue };
            if self.sim.tour_held(id, day_time) {
                continue;
            }
            let next = self.sim.next_of_car(id);
            let mut taken = false;
            if let Some(j) = next {
                if self.sim.may_take_on(j, day_time) {
                    match self.spawn_departure(j, world, traffic, day_time, Some(ci)) {
                        Placed::Spawned => {
                            taken = true;
                            self.sim.taken_on(j);
                        }
                        // (it stands where it set down and takes the trip on later)
                        Placed::Early => continue,
                        _ => {}
                    }
                }
                if !taken {
                    self.sim.not_taken_on(j);
                }
            }
            if !taken && next.is_none() {
                // the tour's last trip is over: Omsi takes the bus (and what is coupled to
                // it) off the road at once rather than letting it drive on
                traffic.remove_car(id);
                self.sim.forget_car(id);
                if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                    log::info!("scheduled bus {id}: the last trip of its tour is over: removed");
                }
            } else if !taken {
                traffic.release(ci);
                if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                    log::info!("scheduled bus {id}: trip over, no next trip of its tour to take on here: it drives off");
                }
            }
        }
        self.sim.requeue_orphans(&traffic.sim);
    }

    /// Spawn buses whose departure time has come (or passed within `window` seconds), and
    /// the layover buses of tours under way: [`Schedule::tick_due`], the vehicle
    /// sets read ahead (`fleet`), [`Schedule::tick_spawn`].
    ///
    /// The renders of the buses it puts on the road or takes off follow at once
    /// (`view_sync::apply_traffic`), before and after the vehicle sets go up, where these
    /// calls used to make them.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        world: &World,
        traffic: &mut Traffic,
        view: &mut TrafficView,
        renderer: &Renderer,
        scene: &mut Scene,
        day_time: f64,
        window: f64,
    ) {
        let Some(loading) = self.tick_due(world, traffic, day_time, window) else {
            return;
        };
        crate::view_sync::apply_traffic(traffic, view, world, renderer, scene);
        self.fleet(world, traffic, renderer, scene, day_time);
        self.tick_spawn(world, traffic, day_time, loading);
        crate::view_sync::apply_traffic(traffic, view, world, renderer, scene);
    }

    /// The timetable's tick up to the vehicle sets (see [`Schedule::tick`]), without the
    /// renderer: the buses of the player's tour taken off, the departures due queued.
    /// Whether the traffic is still loading; None for a LAN client, which draws the host's
    /// timetable buses.
    pub fn tick_due(&mut self, world: &World, traffic: &mut Traffic, day_time: f64, window: f64) -> Option<bool> {
        if traffic.is_mirror() {
            return None;
        }
        for id in self.sim.begin_tick(day_time) {
            if traffic.remove_car(id) {
                log::info!("timetable: bus {id} of the player's tour taken off the road");
            }
        }
        Some(self.sim.queue_due(&*world.data(), &mut traffic.sim, day_time, window))
    }

    /// The timetable's tick after the vehicle sets (see [`Schedule::tick`]), without the
    /// renderer: the tours' buses at the end of a trip take the next on or go, and a few of
    /// the departures due are put on the road (more while the traffic is `loading`).
    pub fn tick_spawn(&mut self, world: &World, traffic: &mut Traffic, day_time: f64, loading: bool) {
        self.tour_handover(world, traffic, day_time);
        // a handful per call: spawning a bus builds its meshes, and a whole rush hour at
        // once is a frame that lasts seconds; and only a few tries: a departure that has to
        // wait still looks for its route, and when the waiting ones all come back at once
        // (every half minute, `queue_due`) two dozen of them were a heavy frame every tenth
        // of a second until they were through (the rest are tried in the next calls). All of
        // them while the traffic is still loading: the trips under way at the start may be
        // put in plain view only then (`queued_at_startup`), and tried a few at a time some
        // came too late for it and never appeared.
        let (mut spawned, mut tried) = (0, 0);
        let tries = if loading || traffic.loading_phase() { 24 } else { RETRIES_PER_CALL };
        while spawned < if loading { 3 } else { 1 } && tried < tries {
            let Some(i) = self.sim.next_pending() else {
                break;
            };
            tried += 1;
            let placed = self.spawn_departure(i, world, traffic, day_time, None);
            if let Placed::Spawned = placed {
                spawned += 1;
            }
            self.sim.settle(i, &placed, day_time);
        }
    }
}
