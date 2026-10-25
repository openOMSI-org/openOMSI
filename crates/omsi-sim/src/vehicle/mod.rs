//! A road vehicle: its type (definition + compiled scripts + model) and instances.

use crate::particles::ParticleSet;
use crate::anim::{pivot_from_mesh, MeshAnimator};
use crate::host::VehicleHost;
use crate::physics::{Controls, VehiclePhysics};
use anyhow::{Context, Result};
use glam::{DVec3, Mat4, Quat, Vec3, Vec4};
use hashbrown::HashMap;
use omsi_geometry::{mesh_from_o3d, MeshData};
use omsi_model::{MaterialDef, Model};
use omsi_script::{compile, CompileInput, Program, State, Vm};
use omsi_vehicle::Vehicle;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod damage;
mod engine_vars;
mod pax_vars;
use engine_vars::Ev;
pub use pax_vars::PaxVars;
mod reload;
pub use damage::{BrokenGlass, DynamicImpact, VehicleDent};
mod car_run;
mod contact;
mod coupling;
mod drive;
mod instance;
mod pose;
mod program_ids;
mod props;
mod radio;
mod panto;
mod rail;
mod trailer;
mod trolley;
mod types;
mod update;
mod vars;
mod visuals;
pub(crate) mod weather;

pub use contact::*;
pub use coupling::*;
pub use car_run::CarRun;
#[cfg(test)]
use drive::*;
pub use pose::*;
pub use rail::RailEnv;
pub use props::*;
pub use trailer::*;
pub use panto::{Catenary, PantoGear};
pub use trolley::TrolleyGear;
pub use types::*;
use update::*;
pub use weather::*;

#[cfg(test)]
mod tests;

/// Entries and exits of a vehicle with variables of their own (`PAX_Entry<n>_Open` …
/// `PAX_Exit<n>_Req`): Omsi.exe's eight, and eight more for buses with more doors than
/// that (#719). A cabin's entries and exits past the eighth that the scripts give no
/// variables of their own open with the eighth, as in Omsi.exe, and ask through the eighth's
/// `_Req` (openOMSI's choice: Omsi.exe's request arrays have eight slots and lose them).
pub const PAX_DOORS: usize = 16;

/// Motion state handed to `VehicleInstance::update_ai`.
#[derive(Debug, Clone, Copy, Default)]
pub struct AiFrame {
    /// m/s
    pub speed: f32,
    /// Distance travelled so far (m), drives the wheel rotation.
    pub odometer: f32,
    /// Front wheel angle of the bicycle model through the `[rot_pnt_long]` line (deg,
    /// positive = right); each wheel's own angle follows from it.
    pub steer_deg: f32,
    /// 0 none, 1 left, 2 right, 3 both (hazard warning).
    pub blinker: i32,
    pub brake: bool,
    pub lights: bool,
    /// `AI_Scheduled_AtStation` as OMSI hands it to the script: 1 = standing at a stop, open
    /// the doors; -1 = time to go, close them (the script answers with 0 once they are
    /// shut); 0 = not at a stop (a script still closing its doors is told -1 until it
    /// answers, see `VehicleInstance::station_released`).
    pub at_station: i32,
    /// `AI_Scheduled_AtStation_Side`: which side's doors a bus standing at its stop opens -
    /// 0 = the side the map lays its road on, 1 = the other, 2 = both. AiList vehicles whose
    /// model has doors on both sides read it (Urumqi61's `[AI]YoungMan*`: the BRT platforms
    /// lie left, the ordinary stops right) and a script without the variable opens the right
    /// side, which is OMSI's default too. 0 when the vehicle is not at a stop.
    pub at_station_side: f32,
    /// `TrafficPriorityWarningNeeded`: a vehicle with right of way (`TrafficPriority`) has
    /// something in its way that is to be warned - the stock ambulance's script sounds its
    /// siren for the next 30 m on it.
    pub priority_warning: bool,
    /// `AI_Engine` -1: a timetable bus standing at a stop for its departure with time to
    /// spare switches its engine off (the stock AI scripts: engine off, parking brake on),
    /// and on again 20 s before it leaves (Omsi.exe 0x7d9128).
    pub engine_off: bool,
}

pub struct VehicleInstance {
    /// Latest electronic destination selection waiting for the current animation.
    pub(crate) pending_destination: Option<crate::timetable_run::PendingDestination>,
    /// `A_Trans_*` taken over OMSI's frames (see [`OmsiFrames`]).
    a_trans: OmsiFrames,
    pub ty: Arc<VehicleType>,
    pub state: State,
    pub vm: Vm,
    pub host: VehicleHost,
    /// World position of the vehicle origin and heading (degrees, clockwise from north).
    pub position: DVec3,
    pub heading: f64,
    pub pitch: f32,
    /// The driver's seat's travel and speed, and the body's turning last frame (see
    /// `update_driver_seat`).
    seat: (f32, f32, Vec3),
    pub bank: f32,
    pub animators: Vec<MeshAnimator>,
    /// Current mesh-space transforms, parallel to `ty.meshes`.
    pub mesh_transforms: Vec<Mat4>,
    pub mesh_props: Vec<MeshProps>,
    pub physics: VehiclePhysics,
    /// `[texttexture]` states, parallel to `ty.model.text_textures`.
    pub text_textures: Vec<crate::texttex::TextTextureState>,
    pub html_textures: Vec<crate::htmltex::HtmlTexture>,
    /// Ground height query (world x, y → z), set by the world.
    /// Ground height sampler (shared: the script host probes the same one).
    pub ground: Option<std::sync::Arc<dyn Fn(f64, f64) -> Option<f64> + Send + Sync>>,
    /// What the wheels stand on: the highest face at or below a height and the lowest one
    /// above it. Without it the wheels stand on `ground`.
    pub contact: Option<std::sync::Arc<dyn crate::rigid::Ground>>,
    /// Vehicles coupled behind this one.
    pub trailers: Vec<TrailerPart>,
    /// The vehicle's `[smoke]` particle systems (see `crate::particles`).
    pub particles: ParticleSet,
    /// How far each `[light_enh_2]` of the model (in order) has come on: its fading variable
    /// followed with the light's `timeconst` (63 % of the way in that time when switched on,
    /// down to 27 % when switched off, as the stock files document it).
    pub light_fade: Vec<f32>,
    /// The meshes' transforms in the modelled pose, for `[smoothskin]` (made when needed).
    skin_rest: Vec<Mat4>,
    /// What `[smoothskin]` needs of each skinned mesh, worked out once (see [`SkinPlan`]).
    skin_plans: SkinPlans,
    /// Static obstacles; the vehicle's `[boundingbox]` is kept out of them.
    pub collision: Option<Arc<crate::collision::CollisionWorld>>,
    /// Moving obstacles (AI vehicles) for this frame, set by the app.
    pub dynamic_boxes: Vec<crate::collision::Obb>,
    /// Contacts with moving vehicles, handed back to traffic after the player's physics step.
    pub dynamic_impacts: Vec<DynamicImpact>,
    /// New dents waiting for their per-instance render mesh to be updated.
    damage_dents: Vec<VehicleDent>,
    broken_glass: Vec<BrokenGlass>,
    glass_broken: Vec<bool>,
    /// A collision happened this frame: the `{trigger:collision}` block runs after physics.
    collided: bool,
    /// Energy of the last crash (J), for whoever wants to report it; cleared by the reader.
    pub last_crash: f32,
    /// Faces the wheels cannot climb stop the vehicle (see `RigidBody::wheel_walls`); the
    /// player's bus follows the setting for collisions with objects.
    pub wheel_walls: bool,
    /// What the radio plays, cut to what a text display shows (see `show_radio_text`):
    /// `None` leaves the scripts' own texts alone, an empty text is a radio that is off.
    pub radio_text: Option<String>,
    /// The frequency the station is on where the bus is (`94.6 MHz`), where it is known.
    pub radio_frequency: Option<String>,
    /// Crashes so far and the energy of the latest (J), kept for logs and the HUD.
    pub crashes: u32,
    pub last_impact: f32,
    /// Map ids of the `[crashmode_pole]` posts this vehicle has knocked over, and those
    /// knocked this frame with the direction they fall in (the scene lays them down).
    pub knocked: Vec<i64>,
    pub knocked_now: Vec<(i64, DVec3)>,
    /// Moving obstacles (AI cars, by their collision id) in contact last frame: one crash per
    /// contact, not one per frame of it.
    touching: Vec<i64>,
    /// Per axle, the static spring compression and how far the tyre radius exceeds the
    /// unloaded hub height (m): the simple model has no springs, but sits as low.
    rest_sag: Vec<(f32, f32)>,
    /// How far an AI copy has to stand higher for its modelled tyres to touch the road.
    ai_lift: f32,
    /// How dirty the body is (0..1). OMSI reports it to the model as `Dirt_Norm`, where
    /// it scales the alpha of the dirt overlay; the bus wash clears it.
    pub dirt: f32,
    /// Rigid-body dynamics (player vehicles); None = simple kinematic model.
    pub rigid: Option<crate::rigid::RigidBody>,
    /// The tyre meshes that are seated on their physical hub (see `seat_wheels`): mesh,
    /// wheel of the rigid body, the mesh's turning centre at rest (model frame). Built on
    /// first use.
    wheel_seats: Option<Vec<(usize, usize, Vec3)>>,
    /// The AI odometer at the last `update_ai` (the wheels roll the difference).
    ai_odometer: f32,
    /// Kilometres driven this session (the odometer's `kmcounter_*`).
    driven_km: f64,
    /// The odometer's start was set from `[kmcounter_init]` (see `update_engine_vars`).
    km_started: bool,
    /// The cabin air as the engine keeps it (°C, g/m³), and the value last written, so that
    /// a bus whose own scripts heat or cool the cabin keeps what they write.
    cabin_air: Option<(f32, f32, f32)>,
    /// An AI vehicle nobody can see (out of the view and away from the mirrors) leaves its
    /// mesh animations and material values as they are: the traffic clears this while it is
    /// out of sight, and the time it missed is made up at once when it comes back. The body,
    /// the coupled parts and the scripts still run every frame.
    pub ai_visuals: bool,
    /// Seconds between the runs of an AI vehicle's script while it is out of sight and far
    /// off (0 = every frame): its animations are not drawn, so the script's results wait
    /// (`TrafficSim::set_visibility`).
    pub ai_script_period: f32,
    ai_visuals_missed: f32,
    /// The coupled parts were moved on in the traffic's step already (`step_ai_trailers`):
    /// the frame's scripts and animations leave them be.
    trailers_stepped: bool,
    /// Variable names (lower case) to their slots; shared with the view's snapshot
    /// (`crate::view_events::CarLooks`), copied only when the vehicle gains a variable.
    var_index: std::sync::Arc<HashMap<String, omsi_script::VarId>>,
    model_only_vars: Vec<String>,
    /// Where each mesh's material properties come from, resolved against `var_index`
    /// (rebuilt when the vehicle gains engine variables).
    props_plan: PropsPlan,
    // cached variable ids for the physics interface
    v_velocity: Option<omsi_script::VarId>,
    v_velocity_ground: Option<omsi_script::VarId>,
    v_n_wheel: Option<omsi_script::VarId>,
    v_m_wheel: Option<omsi_script::VarId>,
    v_throttle: Option<omsi_script::VarId>,
    v_brake: Option<omsi_script::VarId>,
    /// `Brakeforce`: the scripts' brake force for the whole vehicle (N), shared out over its
    /// wheels besides each wheel's own `Axle_Brakeforce_*`.
    v_brakeforce: Option<omsi_script::VarId>,
    v_clutch: Option<omsi_script::VarId>,
    /// `PAX_Entry<n>_Req` and `PAX_Exit<n>_Req` ([`PAX_DOORS`]), and their `_Busy`: set by
    /// the passengers every frame and cleared after the scripts' frame (see
    /// `clear_pax_requests`).
    v_pax_req: Vec<omsi_script::VarId>,
    v_cables: Vec<(omsi_script::VarId, omsi_script::VarId)>,
    v_accel: [Option<omsi_script::VarId>; 3],
    v_wheels: Vec<[[Option<omsi_script::VarId>; 5]; 2]>,
    v_springfactor: Vec<[Option<omsi_script::VarId>; 2]>,
    v_surface: Vec<[Option<omsi_script::VarId>; 2]>,
    /// The passengers' door variables, looked up on first use (see `pax_vars`); made
    /// again when the vehicle gains a variable.
    pax_vars: std::sync::OnceLock<PaxVars>,
    /// The engine's own variables (see `engine_vars`), the same way.
    engine_ids: std::sync::OnceLock<engine_vars::EngineVarIds>,
    /// The program's own variables the engine uses every frame (see `program_ids`).
    program_ids: std::sync::OnceLock<program_ids::ProgramIds>,
    /// A trolleybus's poles and traction (see `trolley`); None for every other vehicle.
    pub trolley: Option<Box<TrolleyGear>>,
    /// A rail vehicle's pantograph against the wire (see `panto`); None without one.
    panto: Option<Box<PantoGear>>,
    /// The track a rail vehicle runs on (see `rail`); None for a road vehicle.
    rail_env: Option<RailEnv>,
    /// The vehicle's scripts have driven it (else pedals move it, see `rail`).
    rail_scripted: bool,
    rail_out: Option<crate::rail::Outcome>,
    rail_log: f32,
    /// The brake system and the couplings of a rail vehicle whose file describes them.
    rail_brake_sys: Option<crate::rail::brake::BrakeSystem>,
    rail_chain: Option<crate::rail::chain::Chain>,
    /// The train broke in two at a coupling: the brakes are applied.
    rail_broken: bool,
}
