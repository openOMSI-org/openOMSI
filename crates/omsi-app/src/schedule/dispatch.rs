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
    pub fn restart(&mut self, world: &World, traffic: &mut Traffic, view: &mut TrafficView, renderer: &Renderer, scene: &mut Scene, day_time: f64) {
        if traffic.is_mirror() {
            return;
        }
        let gone: Vec<u64> = self.sim.car_ids();
        let mut n = 0;
        for id in gone {
            self.sim.forget_car(id);
            if traffic.remove_car(view, world, renderer, scene, id) {
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
        view: &mut TrafficView,
        renderer: &Renderer,
        scene: &mut Scene,
        day_time: f64,
    ) {
        let done: Vec<u64> = traffic.cars.iter().filter(|c| c.trip_done()).map(|c| c.id).collect();
        for id in done {
            let Some(ci) = traffic.cars.iter().position(|c| c.id == id) else { continue };
            // (a trip of the player's bus company is reported to it: `company_live`)
            self.sim.car_trip_done(id, traffic.cars[ci].bus.as_ref().map(|b| b.delay).unwrap_or(0.0));
            let next = self.sim.next_of_car(id);
            let mut taken = false;
            if let Some(j) = next {
                if self.sim.may_take_on(j, day_time) {
                    if let Placed::Spawned =
                        self.spawn_departure(j, world, traffic, view, renderer, scene, day_time, Some(ci))
                    {
                        taken = true;
                        self.sim.taken_on(j);
                    }
                }
                if !taken {
                    self.sim.not_taken_on(j);
                }
            }
            if !taken && next.is_none() {
                // the tour's last trip is over: Omsi takes the bus (and what is coupled to
                // it) off the road at once rather than letting it drive on
                traffic.remove_car(view, world, renderer, scene, id);
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
    /// the layover buses of the next quarter of an hour.
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
        // (a LAN client draws the host's timetable buses)
        if traffic.is_mirror() {
            return;
        }
        for id in self.sim.begin_tick(day_time) {
            if traffic.remove_car(view, world, renderer, scene, id) {
                log::info!("timetable: bus {id} of the player's tour taken off the road");
            }
        }
        let loading = self.sim.queue_due(world, &mut traffic.sim, day_time, window);
        self.fleet(world, traffic, renderer, scene, day_time);
        self.tour_handover(world, traffic, view, renderer, scene, day_time);
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
            let placed = self.spawn_departure(i, world, traffic, view, renderer, scene, day_time, None);
            if let Placed::Spawned = placed {
                spawned += 1;
            }
            self.sim.settle(i, &placed, day_time);
        }
    }
}
