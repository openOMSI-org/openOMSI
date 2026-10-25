//! Scheduled AI buses: the map's timetable lines (`TTData`) put buses on their tracks at the
//! tour departure times; they follow the track lanes and stop at the trip's stations.
//!
//! The timetable itself is omsi-sim's `timetable_run` (`ScheduleSim`: the departures and
//! which are due, the tours' vehicles, the routes on the lanes, the departure boards; the
//! trip times, the IBIS, the player's duty), which does not touch the GPU. `Schedule` holds
//! it (`Deref` to `ScheduleSim`) with what the GPU needs: `fleet` reads and uploads the
//! vehicles ahead, `dispatch` and `spawn` put the buses on the road and take them off.

mod dispatch;
mod fleet;
mod spawn;
#[cfg(test)]
pub(crate) mod tests;

use crate::scene::World;
use crate::traffic::Traffic;
use crate::view_sync::traffic::TrafficView;
use hashbrown::{HashMap, HashSet};
use omsi_sim::VehicleType;
use std::path::Path;
use std::sync::Arc;

use omsi_sim::timetable_run::*;

pub use omsi_sim::timetable_run::{PlannedStop, PlannedTrip, PlayerDuty};
// (the tests of `schedule_paper` name it by this path)
#[cfg_attr(not(test), allow(unused_imports))]
pub use omsi_sim::timetable_run::StopDir;
pub(crate) use omsi_sim::timetable_run::{
    has_roller_blind, hhmm, ibis_stop_index, player_ibis, set_ai_destination,
    set_ai_destination_at, set_player_destination_at, set_player_destination_directly,
    shown_destination, turn_roller_blind, BlindPick,
};

/// The timetable (`ScheduleSim`, which everything of the timetable reads through `Deref`)
/// with the vehicle sets it reads and uploads ahead for the GPU.
pub struct Schedule {
    pub sim: ScheduleSim,
    next_number: usize,
    /// Vehicle sets being read ahead on the workers, with the type to upload them with, and
    /// those read and waiting for their upload.
    fleet_reading: HashMap<crate::scene::VehicleKey, Arc<VehicleType>>,
    fleet_ready: Arc<parking_lot::Mutex<Vec<crate::scene::VehicleKey>>>,
    /// Time of day of the last look at the next departures' vehicles.
    fleet_check: f64,
    /// Vehicle sets a departure needed that were not on the GPU: read ahead before the
    /// departure is tried again (see `Schedule::sets_ready`), not built on the frame it
    /// is put on the road.
    fleet_wanted: Vec<(Arc<VehicleType>, Option<usize>)>,
    /// Seconds until the next look for due departures (see `frame_traffic`).
    pub(crate) tick_t: f32,
    /// The tracks of the map's transport routes, being found on a worker once the map's whole
    /// network is read.
    transit_rx: Option<std::sync::mpsc::Receiver<TransitSolved>>,
}

/// What the worker found on the whole map's network: the trips' tracks and the blocks' lanes.
pub(crate) struct TransitSolved {
    trips: Vec<(String, Result<omsi_sim::timetable_run::transit::Routed, String>)>,
    blocks: Vec<(String, Result<omsi_sim::ai_traffic::blocks::BlockDef, String>)>,
}

impl std::ops::Deref for Schedule {
    type Target = ScheduleSim;
    fn deref(&self) -> &ScheduleSim {
        &self.sim
    }
}

impl std::ops::DerefMut for Schedule {
    fn deref_mut(&mut self) -> &mut ScheduleSim {
        &mut self.sim
    }
}

impl Schedule {
    /// `clock` gives the date: tours carry a validity mask (bits 0-6 Monday…Sunday, 7 public
    /// holiday, 8 school days, 9 school holidays) that selects which run today.
    pub fn new(root: &Path, world: &World, clock: &omsi_sim::SimClock) -> Schedule {
        Schedule {
            sim: ScheduleSim::new(root, &*world.data(), clock),
            next_number: 0,
            fleet_reading: HashMap::new(),
            fleet_ready: Default::default(),
            fleet_check: f64::NEG_INFINITY,
            fleet_wanted: Vec::new(),
            // (a second after the population's, never in the same frame)
            tick_t: 1.0,
            transit_rx: None,
        }
    }

    /// Find the tracks of the trips made from the map's transport routes and the lanes of its
    /// signalling blocks: the whole map's network is read on a worker (the navigator reads the
    /// same), the trips and blocks routed over it, and the result taken by `poll_transit`. The
    /// trips' departures wait until then.
    pub fn start_transit(&mut self, world: Arc<World>) {
        let has_blocks = world.transit.signals.blocks.iter().any(|b| b.from.is_some() && b.to.is_some());
        if self.transit_rx.is_some() || !(self.sim.transit_pending() || has_blocks) {
            return;
        }
        let wanted = self.sim.transit_wanted();
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("transport routes".into()).spawn(move || {
            let m = world.navigation_map();
            let mut net = omsi_sim::traffic::Network { lanes: m.lanes, ..Default::default() };
            net.link(1.5);
            let places: hashbrown::HashMap<i64, glam::DVec3> = m.places.into_iter().collect();
            let trips = omsi_sim::timetable_run::transit::route_all(&net, &places, &wanted);
            let blocks = omsi_sim::timetable_run::transit::route_blocks(&net, &world.transit.signals);
            let _ = tx.send(TransitSolved { trips, blocks });
        });
        if spawned.is_ok() {
            self.transit_rx = Some(rx);
        }
    }

    /// Install what the worker found, if it has finished: the trips' tracks go into the
    /// timetable, the blocks are returned for the traffic.
    pub fn poll_transit(&mut self) -> Option<Vec<omsi_sim::ai_traffic::blocks::BlockDef>> {
        let rx = self.transit_rx.as_ref()?;
        match rx.try_recv() {
            Ok(done) => {
                self.sim.install_transit(done.trips);
                self.transit_rx = None;
                let mut defs = Vec::new();
                for (id, r) in done.blocks {
                    match r {
                        Ok(d) => defs.push(d),
                        Err(why) => log::warn!("signal block {id}: {why}"),
                    }
                }
                if !defs.is_empty() {
                    log::info!("signal blocks: {} found on the track", defs.len());
                }
                Some(defs)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(_) => {
                // (the worker died: give the trips up rather than hold their departures for ever)
                let all: Vec<_> = self.sim.transit_wanted().into_iter().map(|t| (t.trip, Err("the router stopped".to_string()))).collect();
                self.sim.install_transit(all);
                self.transit_rx = None;
                None
            }
        }
    }

    /// The lanes a trip runs on (for the navigator): its track, else the station links
    /// between its stops, as far as the tiles have brought them - and whether that is all.
    pub fn trip_route(&self, world: &World, traffic: &Traffic, trip_name: &str) -> (Vec<usize>, bool) {
        self.sim.trip_route(&*world.data(), &traffic.sim, trip_name)
    }

    /// `OMSI_CHECK_TRIPS=1`: the routes of every trip on the loaded lanes checked (see
    /// `ScheduleSim::check_routes`).
    pub fn check_routes(&self, world: &World, traffic: &mut Traffic) {
        self.sim.check_routes(&*world.data(), &mut traffic.sim)
    }

    /// Assign the player a tour of a line (see `ScheduleSim::player_duty`).
    pub fn player_duty(
        &mut self,
        world: &World,
        line: &str,
        tour: &str,
        now: f64,
        trip: Option<&str>,
        whole_tour: bool,
    ) -> Result<PlayerDuty, String> {
        self.sim.player_duty(&*world.data(), line, tour, now, trip, whole_tour)
    }

    /// Make the departure boards of the stops whose displays are near
    /// (`World::timetable_boards`), and hand the scenery the time of day. The boards are
    /// made at most once a second: the buses due at each stop in the next two hours,
    /// soonest first - the timetable buses with the delay they run with, and the player's.
    pub fn update_boards(
        &mut self,
        world: &World,
        traffic: Option<&Traffic>,
        duty: Option<&PlayerDuty>,
        clock: &omsi_sim::SimClock,
    ) {
        let now = clock.time;
        let mut boards = world.timetable_boards.lock();
        boards.clock = Some(clock.clone());
        // (a stop a page asks for anew is made at once, not up to a second later: the boards of
        // the map's own displays may just have been made without it)
        let new_name = boards.wanted_names.iter().any(|k| !boards.departures.contains_key(k));
        if (self.sim.boards_fresh(now) && !new_name) || (boards.wanted.is_empty() && boards.wanted_names.is_empty()) {
            return;
        }
        let (wanted, wanted_names) = (boards.wanted.clone(), boards.wanted_names.clone());
        let (by_stop, departures) =
            self.sim.make_boards(traffic.map(|t| &t.sim), wanted, wanted_names, duty, clock);
        boards.by_stop = by_stop;
        boards.departures = departures;
        boards.departures_gen = boards.departures_gen.wrapping_add(1);
    }
}

impl Schedule {
    /// The timetable buses on the road, for outside tools (`telemetry`): line, tour, trip file,
    /// next stop (map object), standing at it, delay, position and fleet number of each.
    pub fn telemetry_ai(&self, traffic: Option<&crate::traffic::Traffic>) -> serde_json::Value {
        let Some(t) = traffic else { return serde_json::Value::Array(Vec::new()) };
        let list = self.sim.ai_bus_rows(&t.sim);
        serde_json::Value::Array(list.into_iter().map(|b| serde_json::json!({
            "id": b.id,
            "line": b.line,
            "tour": b.tour,
            "trip": b.trip,
            "terminus": b.terminus,
            "depart": b.depart,
            "next_stop_id": b.next_stop_id,
            "at_stop": b.at_stop,
            "trip_done": b.trip_done,
            "delay_s": b.delay_s,
            "x": b.x, "y": b.y,
            "number": b.number,
        })).collect())
    }
}
