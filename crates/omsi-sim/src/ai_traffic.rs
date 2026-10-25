//! AI road traffic, the simulation: vehicles from `ailists.cfg` moving on the map's path
//! network (`crate::traffic`), the traffic light programs of the junctions, the cars out of
//! range, and who may appear where the player looks.
//!
//! Nothing here knows of the GPU, the window or the loaded world: what the cars look like
//! and sound like, and taking the lanes and the vehicle types from the world, is omsi-app's
//! `traffic` (its `Traffic` is a `TrafficSim` with its pictures and sounds).

pub mod body_link;
pub mod admission;
pub mod berths;
pub mod blocks;
pub mod bus_service;
pub mod control;
pub mod deadlock;
pub mod fixed_step;
pub mod horn;
pub mod density;
pub mod dormant;
pub mod emergency;
pub mod junctions;
pub mod lateral;
pub mod light_paths;
pub mod lights;
pub mod mirror;
pub mod model;
pub mod obstacles;
pub mod foot_grid;
pub mod parked;
pub mod planning;
pub mod scenery;
pub mod setup;
pub mod stats;
pub mod tick;
pub mod viewer;
#[cfg(test)]
mod body_tests;
#[cfg(test)]
mod scenario_tests;
#[cfg(test)]
mod junction_tests;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod population_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod lateral_tests;
#[cfg(test)]
mod emergency_tests;
#[cfg(test)]
mod signal_tests;
#[cfg(test)]
mod request_tests;

use crate::ai_motion::{
    back_in_ramp, pull_out_ramps, AiBody, MotionKind, BACK_IN_LAT_ACCEL, PULL_OUT_ACCEL,
    PULL_OUT_CLEARANCE,
};
use crate::collision::Obb;
use crate::traffic::{
    arrival_time, AiState, Aspect, LaneKind, Lead, Network, TrafficLightController, MAX_BRAKE,
};
use crate::vehicle::AiFrame;
use crate::{VehicleInstance, VehicleType};
use bus_service::{BusService, Phase};
use glam::{DVec2, DVec3};
use hashbrown::HashMap;
use std::path::Path;
use std::sync::Arc;

use control::Removal;
use dormant::*;
use junctions::*;
use lateral::*;
use lights::*;
use model::*;
use obstacles::*;
use parked::*;
use tick::*;
use viewer::*;

/// How many of a type's paint schemes the AI uses: every scheme is a full upload of the
/// bus's textures the first time it appears, which used to cost a frame of 100-200 ms
/// each and a minute of stutter after loading Spandau.
pub const AI_SCHEMES: usize = 4;

pub struct TrafficSim {
    pub net: Network,
    /// Sum of the spawn weights of every street lane, updated only as tiles add lanes.
    pub street_weight: f64,
    /// Parked cars standing in or beside a lane: per lane, (distance along it, signed
    /// lateral offset of the car's centre, + = right). A car in the lane's middle is an
    /// obstacle to stop behind; one over the kerb side is passed with a swerve to the left.
    pub parked: HashMap<usize, Vec<(f32, f32)>>,
    /// Parked cars no lane has been found beside yet: the lane may come with a tile that is
    /// not loaded yet (a road spline often starts in the next tile). Most stand in car parks
    /// and stay here.
    pub parked_waiting: Vec<DVec3>,
    /// Tiles whose lanes the network has (lanes stay once they are in).
    pub lane_tiles: hashbrown::HashSet<(i32, i32)>,
    /// Counts the times tiles brought their lanes: whoever resolved something against the
    /// network and missed a part of it looks again when this changes.
    pub lanes_generation: u64,
    /// AI vehicle types with weight, the lane kind they run on (`[type]` 2 rail, 3 air)
    /// and their group in `groups`.
    pub types: Vec<(Arc<VehicleType>, f32, LaneKind, usize)>,
    /// The random traffic groups: name, `unsched_trafficdens.txt` factor and day curves.
    pub groups: Vec<omsi_map::ailists::UnschedGroup>,
    /// The map has an `unsched_trafficdens.txt` (else the global.cfg curve applies).
    pub group_curves: bool,
    /// Each group's place in `unsched_vehgroups.txt`, the number a path's `[rule]
    /// trafficdensity` names it by (None: the map has no such file, and every group drives
    /// wherever the lane's density lets traffic).
    pub group_uvg: Vec<Option<usize>>,
    /// The default density of every `unsched_vehgroups.txt` entry, in file order: 0 none, 1
    /// for the first entry its medium density, for any other that of the first entry, 2 of
    /// the second, and so on. It applies on the paths without a rule for the group.
    pub uvg_defaults: Arc<Vec<i32>>,
    pub cars: Vec<AiCar>,
    /// The random cars out of range (see `DormantCar`).
    pub dormant: Vec<DormantCar>,
    /// `time` when the dormant cars last moved on.
    pub dormant_time: f32,
    /// The street lanes' weights summed in lane order, for placing the map's dormant cars
    /// (`fill_map`), with the number of lanes they were made for.
    pub(crate) lane_weight_sums: (usize, Vec<f64>),
    pub rng: u64,
    /// Target number of cars around the camera.
    pub target: usize,
    /// Made only so that the light programs run (no traffic, no timetable): nobody is put
    /// on the roads - no aircraft, no parked car pulling out - while `target` is 0.
    pub lights_only: bool,
    pub spawn_radius: f64,
    /// Since when (sim time) fewer than half the cars asked for drive about the player: open
    /// country, where nothing within `NEAR_HIDE` is ever out of sight and a car coming along
    /// the road could never be woken (see `omsi-app`'s `wake_dormant`).
    pub starved_since: Option<f32>,
    pub time: f32,
    /// Where the camera is (the window sets it before `sync`): far cars show their script
    /// textures as stand-ins.
    pub camera: Option<DVec3>,
    pub lights: Vec<TrafficLightController>,
    pub controller_of_object: HashMap<i64, usize>,
    /// Coupled vehicle types by file.
    pub trailer_types: HashMap<std::path::PathBuf, Option<Arc<VehicleType>>>,
    pub coupled_chains: HashMap<(std::path::PathBuf, bool, bool), Vec<(Arc<VehicleType>, bool)>>,
    pub root: std::path::PathBuf,
    /// Car-frames spent waiting for a red light (statistics).
    pub held_at_red: usize,
    /// Who wants a timetable bus to stop (`Humans::stop_wishes`): the buses somebody
    /// aboard wants to get off, the stops where somebody waits. None without passengers:
    /// every bus then serves every stop.
    pub stop_wishes: Option<(hashbrown::HashSet<u64>, hashbrown::HashSet<i64>)>,
    /// How full each timetable bus is (`PeopleSim::bus_loads`), for the departure displays.
    pub bus_loads: hashbrown::HashMap<u64, f32>,
    /// Seconds the player's vehicle has been standing.
    pub player_still: f32,
    /// Time of day (seconds since midnight); light cycles and timetables run on it.
    pub day_time: f64,
    /// How fast the clock runs (the time speed): the timetable keeps to it.
    pub time_scale: f64,
    /// Day of the week (0 Monday … 6 Sunday) for the traffic density curves.
    pub weekday: i32,
    /// Street lights on → AI vehicles switch their lights on.
    pub night: bool,
    /// The light of the day, for the cars' `Envir_Brightness` (see `sync`).
    pub daylight: Option<crate::Daylight>,
    pub next_id: u64,
    /// The last car that started an overtake and when (for chase-camera debugging).
    pub last_overtaker: Option<(u64, f32)>,
    /// The first car that entered a turning lane and when (`--follow turn`).
    pub first_turner: Option<(u64, f32)>,
    /// The first car that stopped at a red light (`--follow red`), the first that gave way
    /// at a junction (`--follow yield`), the first that pulled out onto the other side of
    /// the road round an obstacle or squeezed past a bus at its stop (`--follow pass`).
    pub first_red: Option<(u64, f32)>,
    pub first_yield: Option<(u64, f32)>,
    pub first_passer: Option<(u64, f32)>,
    /// `[trafficdensity_road]` curve of the map: (hour, factor).
    pub density_curve: Vec<(f32, f32)>,
    /// The options' `[AIUnschedFactor]`: the share of the random traffic.
    pub unsched_factor: f32,
    /// The options' `[AIMaxCountScheduled]` (0 = no limit).
    pub max_scheduled: u32,
    /// The `ai_wait_timed_stops_only` setting (off by default): an early timetable bus waits
    /// for its departure only at the stops the timetable times itself
    /// (`bus_service::BusService::waits_here`). Off, it waits at every stop it serves, as in
    /// Omsi.exe, where the 20 s check (0x7d9bdc) runs against every station's time, the ones
    /// shared out by distance too (0x616afc -> 0x73b474).
    pub timed_waits_only: bool,
    /// `--no-timetable-buses`: the timetable runs for the player's duty, but puts no AI
    /// bus on the road (#1762).
    pub no_timetable_buses: bool,
    /// Where the player looks from (set every frame).
    pub viewer: Option<Viewer>,
    /// Buildings that hide what is behind them (the player's collision world).
    pub occluders: Option<Arc<crate::collision::CollisionWorld>>,
    /// Pedestrians on the footpaths: (lane, distance along it), for giving way at crossings
    /// and for the pedestrian lights' request buttons.
    pub walkers: Vec<(usize, f32)>,
    /// Everybody on foot on the ground: position, velocity and whether they are waiting
    /// at a stop (set every frame) - the cars stop for anybody in their way, not only on
    /// a crossing.
    pub people: Vec<(DVec3, DVec2, bool)>,
    /// No car has been placed yet: the first population may fill the view.
    pub initial: bool,
    /// Seconds of the last tick (the lamp scripts run in `sync`).
    pub last_dt: f32,
    /// Game time since the lamps' scripts last ran (see `sync`).
    pub lamp_dt: f32,
    /// `OMSI_TRACE_AI=<file.csv>`: every car's pose, steering and speed, every frame.
    pub trace: Option<std::io::BufWriter<std::fs::File>>,
    /// Cars already reported for a hard bend (`OMSI_DEBUG_TRAFFIC`).
    pub logged_hard: hashbrown::HashSet<u64>,
    /// `OMSI_DEBUG_LIGHTS=all|near|<controller>,…`: which programs log their changes, and
    /// the states they showed last.
    pub light_log: Option<String>,
    pub light_prev: Vec<Vec<i32>>,
    /// `OMSI_DEBUG_POPULATION`: log where cars appear and vanish relative to the view.
    pub debug_population: bool,
    /// Cars placed since the last look inside the view frustum (hidden behind something):
    /// (id, position). `OMSI_POPULATION_SHOTS` photographs them to check.
    pub framed_spawns: Vec<(u64, DVec3)>,
    /// The player's vehicle as of the last tick (nothing is put on the road on top of it).
    pub player: Option<PlayerBox>,
    /// The curvature of the player's path as of the last tick (1/m, positive to the right,
    /// `player_turn_of`): the cars look for the bus along its turn (`in_player_sweep`).
    pub player_turn: f64,
    /// The player's bus has right of way over the traffic (its script's `TrafficPriority`,
    /// OMSI: priority 1000 over the types' own): cars keep out of the way it is about
    /// to take for longer.
    pub player_priority: bool,
    /// The LAN players whose vehicles' scripts claim priority (`TrafficPriority`, as their
    /// variables come over the network): emergency vehicles like the player's (by session id,
    /// set before each `tick`).
    pub other_priority: hashbrown::HashSet<u32>,
    /// Emergency vehicles on an active drive, the cars making way for them and the junctions
    /// kept for them (`emergency`).
    pub emergency: emergency::Emergency,
    /// The player's indicators (0 off, 1 left, 2 right, 3 hazard; `lan::indicator`), set
    /// before each `tick`.
    pub player_blinker: u8,
    /// The player's tram or train on the rails, set before each `tick`: it asks the lights on
    /// the track ahead as an AI one does (they stayed red for the player).
    pub player_rail: Option<PlayerRail>,
    /// The map's signalling blocks (`signals.cfg`) and its configured points.
    pub blocks: blocks::BlockSys,
    pub switch_board: blocks::SwitchBoard,
    /// Seconds since the player's bus last showed the indicator towards the traffic (the
    /// lamps go dark half of the time).
    pub player_signal_age: f32,
    /// ... and for how long it has been indicating so (s).
    pub player_signalling: f32,
    /// The player's vehicle and the LAN players' as the junctions see them (`way_user_on`),
    /// as of this tick.
    pub way_users: Vec<WayUser>,
    /// The LAN players' vehicles (their session ids and boxes as for the player), set
    /// before each `tick`: the cars stop behind them and go round them as round the
    /// player's bus.
    pub others: Vec<(u32, PlayerBox)>,
    /// The indicators of `others` by id (0 off, 1 left, 2 right, 3 hazard; a rear section
    /// shows its towing vehicle's), set with them: a light path marked as a turn asks its
    /// light for whoever stands on it indicating that way (`light_paths`).
    pub other_blinkers: HashMap<u32, u8>,
    /// Where the last `tick` spent its time (s, OMSI_PROFILE): who is on which lane and the
    /// light programs, every car's plan, the bodies and scripts on the workers.
    pub tick_split: [f64; 3],
    /// `tick_split` summed over the steps of the last frame (`advance`).
    pub frame_split: [f64; 3],
    /// Seconds each of them has stood still.
    pub others_still: HashMap<u32, f32>,
    /// Per car: `AiCar::geo_block` of the frame before (who waits for whom by geometry).
    pub geo_prev: Vec<Option<u64>>,
    /// Car index by id (as of the start of the tick).
    pub index_of: HashMap<u64, usize>,
    /// The plans of a tick's cars and the order they plan in, kept for the next tick.
    pub frames_buf: Vec<Option<AiFrame>>,
    pub order_buf: Vec<usize>,
    /// `pull_out_room` by vehicle file.
    pub pull_out_rooms: HashMap<std::path::PathBuf, f32>,
    /// Timetable buses taken off the road because the tile under them was unloaded (their
    /// ids), for the timetable to put them back when the tiles come again.
    pub removed_scheduled: Vec<u64>,
    /// One-way lanes that have had their reverse twin added (`add_reverse_twins`).
    pub twinned: hashbrown::HashSet<usize>,
    /// Bodies besides the AI vehicles' that no timetable vehicle may be put into: the
    /// player's vehicle and the LAN players' (set before each `Schedule::tick`).
    pub keep_clear: Vec<crate::collision::Obb>,
    /// LAN play: this game draws the host's traffic instead of its own (`lan_world`).
    pub mirror: bool,
    /// Count only the cars within this distance of this point when filling up (the
    /// population around a LAN player, `populate_lan_centers`).
    pub count_near: Option<(DVec3, f64)>,
    /// LAN play: where the other players are (host): the traffic is kept around them too.
    pub lan_centers: Vec<DVec3>,
    /// LAN play: where the other players look from (host): what they could see, the
    /// population may not be seen doing either (see `TrafficSim::unseen`).
    pub lan_eyes: Vec<Viewer>,
    /// Cars the last `tick` took off the road (their ids): their sounds and pictures are
    /// for the game to let go (see omsi-app's `Traffic::tick`).
    pub retired: Vec<u64>,
    /// The cars that honked since the game last asked (`horn`): id and whether it is a heavy
    /// vehicle. The stock cars' sounds have nothing on `ev_AI_Horn`, so the game plays
    /// OMSI's own AI horn for them.
    pub honked: Vec<(u64, bool)>,
    /// The LAN players' vehicles as of this tick (`others` is taken for the tick's scene):
    /// the lane changes look at them (`players_on`).
    pub others_now: Vec<PlayerBox>,
    /// `OMSI_TRAFFIC_STATS`: the flow statistics of the run (see `stats`).
    pub stats: Option<Box<stats::TrafficStats>>,
    /// The scenery the traffic drives through: what a car stood for, or could not steer
    /// free of, for `scenery::PATIENCE` seconds (a path the map lays through an object).
    pub scenery_through: Vec<Obb>,
    /// The frame time not yet run as a step (s, see `fixed_step`).
    pub sim_accum: f64,
    /// Whether the scripts and animations of the cars run in this call's last step (the
    /// game sets it for the frame's last tick of the fixed simulation step; true by
    /// default). The bodies move in every step; the scripts, the animations and the sounds
    /// they trigger once a frame, as Omsi.exe runs them - run in every 20 ms step they cost
    /// up to four times as much as a frame had time for at low frame rates, and the frame
    /// rate fell the further the slower it already was (big maps, heavy AI lists).
    pub scripts_due: bool,
    /// The step running now runs the scripts (see `scripts_due`).
    pub step_scripts: bool,
    /// How far this frame's picture is from the step before the last to the last (0..1).
    pub frame_alpha: f32,
    /// Every car's parts as they stood before the last step, by id (see `fixed_step`).
    pub poses: fixed_step::StepPoses,
    /// The lane changes the cars' ways require and those begun this step (see
    /// `lateral::ChangeClaim`).
    pub change_claims: Vec<lateral::ChangeClaim>,
    /// Asks whether a box touches the scenery, for the cars' manoeuvres (`ghost_blocked`);
    /// None: the player's collision world (`occluders`) is asked.
    pub scenery_probe: Option<lateral::SceneryProbe>,
    /// The stops' berths: which timetable bus holds which, who queues for it (`berths`).
    pub berths: berths::Berths,
    /// Cars taken off the road, by `Removal` (diagnostics).
    pub removals: [u64; Removal::COUNT],
    /// The requests to put vehicles on the road, and their answers (`admission`).
    pub admission: admission::Admission,
}
