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
use omsi_sim::VehicleInstance;
use omsi_sim::human::HumanType;
use omsi_sim::people::pax::{StopPlan, TripPlan};
use omsi_sim::people::{DutyTrip, PeopleSim};
use std::path::Path;
use std::sync::Arc;

// The renderer's side of the people.
use crate::view_sync::people::PeopleView;

// The module's API, at the paths it always had (some only returned, never named outside).
#[allow(unused_imports)]
pub use omsi_sim::people::Person;
#[allow(unused_imports)]
pub use omsi_sim::people::VoiceLine;
#[allow(unused_imports)]
pub use omsi_sim::people::{AvatarCmd, SeatSpot};
pub use omsi_sim::people::{BusId, DoorWants};
#[allow(unused_imports)]
pub use omsi_sim::people::{
    LanPerson, MirrorPose, placed_bus_id, remote_bus_id, remote_bus_player,
};

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

impl Humans {
    /// The passenger settings, handed over the same way wherever the people are made (single
    /// player, LAN, online room, offscreen) and when the options change in the game.
    /// `transfers` says whether connections are allowed in this session at all.
    pub(crate) fn apply_settings(&mut self, s: &crate::settings::Settings, transfers: bool) {
        use omsi_sim::people::{entry_doors::EntryDoors, free_drive::FreeDriveFill};
        let enhanced = s.passenger_animation == "enhanced";
        self.exact_fare = s.exact_fare;
        self.transfer_percent = if transfers && s.passenger_connections {
            s.passenger_transfer_percent
        } else {
            0.0
        };
        self.visual_hailing = s.passenger_hailing && enhanced;
        self.visual_full_bus_reaction = s.passenger_full_bus_reaction && enhanced;
        self.bad_driving = s.pax_bad_driving;
        self.bad_driving_alight = s.pax_bad_driving_alight;
        self.ambient_talk = s.passenger_ambient_talk;
        self.entry_doors = EntryDoors::from_setting(&s.pax_entry_doors);
        self.no_ticket = s.pax_no_ticket;
        self.late_tolerance = (s.pax_late_tolerance_min as f64 * 60.0).max(0.0);
        self.free_drive_fill = FreeDriveFill::from_setting(&s.pax_free_drive);
        self.board_when_stopped = s.pax_board_when_stopped;
        self.boarding = s.boarding.clone();
        self.rear_door_boarding = s.rear_door_boarding;
        self.stand_chance = s.standing_chance;
        self.short_rides_stand = s.pax_short_rides_stand;
        self.voices = match s.pax_voices.as_str() {
            "off" => 2,
            "tickets" => 1,
            _ => 0,
        };
    }
}

/// Where the player looks from (`omsi_sim::people::Eye`), made from the camera.
pub struct Eye;

impl Eye {
    pub fn of(cam: &Camera, aspect: f32) -> omsi_sim::people::Eye {
        omsi_sim::people::Eye::looking(
            cam.position,
            cam.forward().as_dvec3(),
            cam.fov_deg as f64,
            aspect,
        )
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
    pub fn tick(
        &mut self,
        dt: f32,
        world: &World,
        bus: Option<&omsi_sim::people::PlayerBusView>,
        traffic: Option<&Traffic>,
    ) -> bool {
        self.sim.tick_stages.clear();
        self.sim
            .tick_inner(dt, &*world.data(), bus, traffic.map(|t| &t.sim))
    }

    /// The end of a step begun `started` with [`Humans::tick`], whose people who came and
    /// went were shown in `bodies_ms` (see `PeopleSim::tick_done`).
    pub fn tick_done(
        &mut self,
        dt: f32,
        world: &World,
        started: std::time::Instant,
        bodies_ms: f64,
    ) {
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
        self.sim
            .set_player_next_stop(stop.map(|s| (s.object_id, s.name.as_str(), s.position)));
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
            .map(|f| ambience::Footfall {
                position: f.position,
                inside: f.inside,
                own_bus: f.own_bus,
                pack: f.pack,
            })
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
        stops: trip
            .stops
            .iter()
            .map(|s| StopPlan {
                object_id: s.object_id,
                name: s.name.clone(),
                stops: s.stops,
            })
            .collect(),
    }
}
