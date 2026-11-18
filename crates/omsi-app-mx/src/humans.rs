//! People on foot: passengers and pedestrians as agents with a goal.
//!
//! The simulation is omsi-sim's `people` (`PeopleSim`, which knows nothing of the GPU);
//! here it gets the loaded world, the traffic and the player's duty, and the people's
//! pictures (`crate::view_sync::people`): their meshes, posing and skinning, and the coins
//! on the cash desk.

use crate::ambience;
use crate::scene::World;
use crate::traffic::Traffic;
use glam::DVec3;
use omsi_render::Camera;
use omsi_sim::human::HumanType;
use omsi_sim::people::pax::{StopPlan, TripPlan};
use omsi_sim::people::{DutyTrip, PeopleSim};
use omsi_sim::VehicleInstance;
use std::path::Path;
use std::sync::Arc;

// The renderer's side of the people.
use crate::view_sync::people::PeopleView;

// The module's API, at the paths it always had (some only returned, never named outside).
#[allow(unused_imports)]
pub use omsi_sim::people::{AvatarCmd, SeatSpot};
#[allow(unused_imports)]
pub use omsi_sim::people::{placed_bus_id, remote_bus_id, remote_bus_player, LanPerson, MirrorPose};
pub use omsi_sim::people::{BusId, DoorWants};
#[allow(unused_imports)]
pub use omsi_sim::people::Person;
#[allow(unused_imports)]
pub use omsi_sim::people::VoiceLine;

/// The map's traffic keeps left (its stops are on the left): see the doors of `Cabin`.

/// The people as the game drives them. Everything of the simulation reads through it
/// (`Deref` to `PeopleSim`). Their pictures are the view sync's (`PeopleView`, in
/// `view_sync::SimView`): the calls that make or remove people write down what the
/// pictures have to follow (`PeopleSim::bodies`), and the caller shows it right after the
/// call with `view_sync::apply_people`.
pub struct Humans {
    pub sim: PeopleSim,
}

impl std::ops::Deref for Humans {
    type Target = PeopleSim;
    fn deref(&self) -> &PeopleSim {
        &self.sim
    }
}

impl std::ops::DerefMut for Humans {
    fn deref_mut(&mut self) -> &mut PeopleSim {
        &mut self.sim
    }
}

/// Where the player looks from (`omsi_sim::people::Eye`), made from the camera.
pub struct Eye;

impl Eye {
    pub fn of(cam: &Camera, aspect: f32) -> omsi_sim::people::Eye {
        omsi_sim::people::Eye::looking(cam.position, cam.forward().as_dvec3(), cam.fov_deg as f64, aspect)
    }
}

impl Humans {
    /// The people, with `view` started afresh for them.
    pub fn new(root: &Path, view: &mut PeopleView) -> Humans {
        let configured_people = crate::settings::Settings::load().ai_max_humans as usize;
        let sim = PeopleSim::new(root, configured_people);
        *view = PeopleView::new();
        Humans { sim }
    }

    /// Advance everybody (see `PeopleSim::tick_inner`). `bus`: the player's bus as read this frame;
    /// `traffic`: the timetable buses, the traffic lights and the cars pedestrians wait for.
    /// Returns true when a passenger took the printed ticket (the caller resets
    /// `GivenTicket`). What the step did is shown and the step closed by
    /// `steps::tick_humans` (`view_sync::apply_people`, then [`Humans::tick_done`]).
    pub fn tick(&mut self, dt: f32, world: &World, bus: Option<&omsi_sim::people::PlayerBusView>, traffic: Option<&Traffic>) -> bool {
        self.sim.tick_stages.clear();
        self.sim.tick_inner(dt, &*world.data(), bus, traffic.map(|t| &t.sim))
    }

    /// The end of a step begun `started` with [`Humans::tick`], whose people who came and
    /// went were shown in `bodies_ms` (see `PeopleSim::tick_done`).
    pub fn tick_done(&mut self, dt: f32, world: &World, started: std::time::Instant, bodies_ms: f64) {
        self.sim.tick_stages.push(("bodies", bodies_ms));
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        self.sim.tick_done(dt, &*world.data(), ms);
    }

    /// Put people at the bus stops near `center` (see `PeopleSim::populate`).
    pub fn populate(&mut self, world: &World, center: DVec3) {
        self.sim.populate(&*world.data(), center);
    }

    /// Put avatar `key` where `cmd` says (see `PeopleSim::avatar`).
    pub fn avatar(&mut self, key: u32, world: &World, cmd: AvatarCmd, kind: u64) {
        self.sim.avatar(key, &*world.data(), cmd, kind);
    }

    /// One of the host's people appears here (client; see `PeopleSim::mirror_add`).
    pub fn mirror_add(&mut self, world: &World, id: u32, ty: usize, pose: &MirrorPose) -> bool {
        self.sim.mirror_add(&*world.data(), id, ty, pose)
    }

    /// `--riders n` (see `PeopleSim::seed_riders`).
    pub fn seed_riders(&mut self, n: usize, bus: &VehicleInstance, world: &World) {
        self.sim.seed_riders(n, bus, &*world.data());
    }

    /// OMSI's `change_take`: the driver takes back the coins lying on the change tray.
    pub fn take_change_tray(&mut self) {
        if let Some(m) = self.sim.money.as_mut() {
            // Collect the driver's change.
            m.clear(true);
            // Collect the passenger's payment.
            m.clear(false);
        }
    }

    /// Bus `bus` is gone (the player removed it; see `PeopleSim::evict`).
    pub fn evict(&mut self, bus: BusId, world: &World) {
        self.sim.evict(bus, &*world.data());
    }

    /// The stop the player's duty is due at next (see `PeopleSim::set_player_next_stop`).
    pub fn set_player_next_stop(&mut self, stop: Option<&crate::schedule::PlannedStop>) {
        self.sim.set_player_next_stop(stop.map(|s| (s.object_id, s.name.as_str(), s.position)));
    }

    /// The player's duty this frame; None in free drive, where the bus takes whom its terminus
    /// shown takes, as a timetable bus. (Set after `stop_names`: the trip's stops are named by it.)
    pub fn set_duty(&mut self, duty: Option<&crate::schedule::PlayerDuty>) {
        let Some(d) = duty else {
            self.sim.duty = None;
            return;
        };
        // the trip the IBIS is given: on a works trip from the depot the next one with a line
        let (t, next) = d.trip_for_ibis();
        let done = std::ptr::eq(t, d.trip()) && d.trip_done();
        let trip = match self.sim.duty.take() {
            Some((trip, ..)) if trip.name == t.name && trip.departure == t.departure => trip,
            _ => Arc::new(DutyTrip::of(&trip_plan(t), self.sim.stop_names.as_ref())),
        };
        self.sim.duty = Some((trip, next, done));
    }

    /// The footsteps taken since the last call, for the environment sounds (see
    /// `PeopleSim::take_footfalls`).
    pub fn take_footfalls(&mut self) -> Vec<ambience::Footfall> {
        self.sim
            .take_footfalls()
            .into_iter()
            .map(|f| ambience::Footfall { position: f.position, inside: f.inside, own_bus: f.own_bus, pack: f.pack })
            .collect()
    }

    /// Count of people per state, for logs, and the time posing them took (`view`).
    pub fn summary(&self, view: &PeopleView) -> String {
        let mut out = self.sim.summary();
        let (frames, posed, ms, up) = view.pose_stats;
        if frames > 0 {
            out.push_str(&format!(
                "; posing {:.2} ms a frame ({:.1} people, {:.2} ms of it uploading and placing)",
                ms / frames as f64,
                posed as f64 / frames as f64,
                up / frames as f64
            ));
        }
        out
    }

    /// The file of a human type relative to its content root (`Humans/…/x.hum`).
    pub fn type_file(ty: &HumanType) -> String {
        PeopleSim::type_file(ty)
    }

    /// Hand a bus's scripts what the passengers want of its doors and places (see
    /// `PeopleSim::write_door_requests`).
    pub(crate) fn write_door_requests(b: &mut VehicleInstance, doors: &DoorWants) {
        PeopleSim::write_door_requests(b, doors)
    }
}

/// A trip of the timetable as the passengers need it.
fn trip_plan(trip: &crate::schedule::PlannedTrip) -> TripPlan {
    TripPlan {
        name: trip.name.clone(),
        line: trip.line.clone(),
        terminus: trip.terminus.clone(),
        departure: trip.departure,
        stops: trip.stops.iter().map(|s| StopPlan { object_id: s.object_id, name: s.name.clone(), stops: s.stops }).collect(),
    }
}
