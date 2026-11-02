//! AI road traffic: vehicles from `ailists.cfg` moving on the map's path network, the
//! traffic light programs of the junctions, and the population of cars around the player.
//!
//! The simulation is omsi-sim's `ai_traffic` (`TrafficSim`, which knows nothing of the
//! GPU); here it gets the loaded world's lanes and vehicle types, the population around
//! the player, the cars' pictures (`crate::view_sync::traffic`) and their sounds
//! (`crate::view_sync::traffic_audio`).

mod control;
mod mirror;
mod parked;
mod population;
mod setup;
mod table;
mod trains;
#[cfg(test)]
mod locator_tests;

use anyhow::Result;
use glam::DVec3;
use hashbrown::HashMap;
use omsi_sim::ai_motion::{AiBody, MotionKind};
use omsi_sim::ai_traffic::bus_service::BusService;
use omsi_sim::ai_traffic::setup::RandomTypes;
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::traffic::{AiState, LaneKind, Network};
use omsi_sim::{VehicleInstance, VehicleType};
use std::path::Path;
use std::sync::Arc;

use omsi_sim::ai_traffic::density::*;
use omsi_sim::ai_traffic::dormant::*;
use omsi_sim::ai_traffic::lights::*;
use omsi_sim::ai_traffic::model::*;
use omsi_sim::ai_traffic::viewer::*;
use omsi_sim::view_events::TrafficEvent;

pub use omsi_sim::ai_traffic::AI_SCHEMES;
pub(crate) use omsi_sim::ai_traffic::model::{vehicle_bodies, AiCar, BusSetup, ParkPlan, PlayerBox};
pub(crate) use setup::warm_up;
pub(crate) use omsi_sim::ai_traffic::viewer::Viewer;

/// The traffic simulation with what the game makes of it: the cars' sounds (played by the
/// view sync, `crate::view_sync::traffic_audio`). Everything of
/// the simulation reads through it (`Deref` to `TrafficSim`). The cars' pictures are the
/// view sync's (`TrafficView`, in `view_sync::SimView`): the traffic's own calls that put a
/// car on the road or take it off write down what the pictures have to follow (`events`),
/// replayed by `view_sync::apply_traffic`.
pub struct Traffic {
    pub sim: TrafficSim,
    /// What the renders have to follow since the view last caught up, oldest first (see
    /// `TrafficEvent`).
    pub(crate) events: Vec<TrafficEvent>,
    /// Where the cars and their meshes are drawn, taken at each view sync (its buffers kept
    /// from one to the next; see `FrameSnapshot`).
    pub(crate) snapshot: omsi_sim::view_events::FrameSnapshot,
    /// What the snapshots sent the view of each car's looks (see `LookTracker`).
    pub(crate) looks: omsi_sim::view_events::LookTracker,
    /// What the cars' sounds read of them, taken at each audio update (see `SoundFrame`).
    pub(crate) sound_frame: omsi_sim::view_events::SoundFrame,
    /// `[sound_ai]` set of each car near the listener, by car id (see `audio`).
    pub(crate) sounds: HashMap<u64, omsi_audio::SoundSet>,
    /// Sound sets of despawned cars, stopped at the next audio update.
    pub(crate) orphan_sounds: Vec<omsi_audio::SoundSet>,
    /// The bus the player rides in on foot, heard from inside: its whole `[sound]` set as
    /// the player's own bus is heard from its cab (#1286).
    pub(crate) riding_sounds: Option<(u64, omsi_audio::SoundSet)>,
    /// `[sound_ai]` configurations by file.
    pub(crate) sound_cfgs: HashMap<std::path::PathBuf, Option<Arc<omsi_vehicle::SoundCfg>>>,
    /// Shared script environment for existing AI and for their next `{init}`.
    pub script_environment: Option<([f32; 22], glam::Vec3, f32)>,
    /// What each vehicle definition (by its file) is heard with, its paths resolved once:
    /// a car waiting for its clips looked them all up on the disk again every frame, and a
    /// car without a usable sound configuration did so as long as it was near.
    pub(crate) sound_plans: HashMap<std::path::PathBuf, Option<Arc<crate::view_sync::traffic_audio::SoundPlan>>>,
    /// The traffic light lamps updated in turns far off (see `view_sync::traffic`): the
    /// frames counted, and the seconds each far lamp's script is owed, by script.
    pub(crate) lamp_turn: u32,
    pub(crate) lamp_owed: HashMap<usize, f32>,
    /// Every car as a row under an id of its own, written after each step (`write_table`):
    /// what the LAN host tells its clients.
    pub(crate) table: omsi_net::traffic_table::TrafficTable,
    /// The content folder the table names vehicle files by.
    pub(crate) table_root: std::path::PathBuf,
    /// Whether the table is written: only a LAN host reads it (set each frame by the caller).
    pub(crate) table_on: bool,
    /// The traffic time the buses' line and destination were last written to the table.
    pub(crate) table_meta_t: f32,
}

impl std::ops::Deref for Traffic {
    type Target = TrafficSim;
    fn deref(&self) -> &TrafficSim {
        &self.sim
    }
}

impl std::ops::DerefMut for Traffic {
    fn deref_mut(&mut self) -> &mut TrafficSim {
        &mut self.sim
    }
}

impl Traffic {
    /// The simulation, with no car heard yet (and none drawn: its `TrafficView` starts
    /// afresh where it is put in place).
    fn with_sim(sim: TrafficSim, root: &Path) -> Traffic {
        Traffic {
            sim,
            events: Vec::new(),
            snapshot: Default::default(),
            looks: Default::default(),
            sound_frame: Default::default(),
            sounds: HashMap::new(),
            orphan_sounds: Vec::new(),
            riding_sounds: None,
            sound_cfgs: HashMap::new(),
            script_environment: None,
            sound_plans: HashMap::new(),
            lamp_turn: 0,
            lamp_owed: HashMap::new(),
            table: Default::default(),
            table_root: root.to_path_buf(),
            table_on: false,
            table_meta_t: f32::NEG_INFINITY,
        }
    }

    /// Advance all cars by a frame (see `TrafficSim::advance`: in the traffic's fixed steps),
    /// then [`Traffic::after_step`].
    /// `player`: (centre, heading in degrees, half length, half width, speed) of the
    /// player's vehicle.
    pub fn tick(&mut self, dt: f32, player: Option<PlayerBox>) {
        self.sim.advance(dt, player);
        self.after_step();
    }

    /// What follows the traffic's step (`TrafficSim::advance`, which may run elsewhere): the
    /// cars it took off the road go silent, and their pictures go back to the world at the
    /// next sync (`TrafficEvent::Retired`).
    pub fn after_step(&mut self) {
        for id in std::mem::take(&mut self.sim.retired) {
            self.orphan_sounds.extend(self.sounds.remove(&id));
            self.events.push(TrafficEvent::Retired { id });
        }
        self.write_table();
    }

    /// Refresh shared script inputs even while paused, and keep them for new vehicles.
    pub fn set_script_environment(&mut self, values: [f32; 22], wind: glam::Vec3, street: f32) {
        self.script_environment = Some((values, wind, street));
        if !self.sim.mirror {
            for car in &mut self.sim.cars {
                car.vehicle.host.apply_ai_globals(&values);
                car.vehicle.host.wind = wind;
                car.vehicle.host.street_cond = street;
            }
        }
    }

    /// Let go a car's sound set (it is stopped at the next audio update).
    pub(crate) fn drop_sounds(&mut self, id: u64) {
        self.orphan_sounds.extend(self.sounds.remove(&id));
    }
}
