//! The driver of the player's bus: a person on the bus's `[drivpos]` with both hands on the
//! steering wheel, turning it as the wheel turns - seen from outside, from the passengers'
//! places and in the mirrors, left out of the driver's own view (the cab view shows him
//! only in the mirrors, as OMSI does).
//!
//! OMSI keeps driver figures of its own among the people (`Humans/*/..._driver.hum`, which
//! the passenger crowd leaves out). The wheel is the mesh the model turns with
//! `Axle_Steering_*` by a large factor (the LiAZ's -1680, the MANs' 1450); its turning axis
//! is its `[newanim]` origin frame, its rim the farthest ring of its vertices round that
//! axis and its centre the middle of that ring on the axis (the origin is often the foot of
//! the column: the Urbino's lies 12 cm under the hub, and the hands held the air under and
//! past the rim). Both hands hold the rim at ten to two, closed round it in fists whose
//! wrists continue the forearms (the hand turned onto a fixed frame on the rim bent the
//! wrists sharply), and turn with it, the wheel's angle read from its own animation
//! variable. Turned out of its reach a hand lets go and takes the rim again further back
//! while the other holds on, as drivers shuffle a bus's wheel through their hands; held
//! still, the wheel gets the hands back at their rest. (Before, the hands stopped at the
//! end of a small range and the rim slid on through them: the wheel seemed to turn by
//! itself under hands frozen in the air.)
//!
//! Manual buses also get their gear lever worked by the driver (`find_shifter`). The lever is
//! the mesh near the seat that a gear/shift/"Antrieb" variable animates (or, failing that, one whose
//! file name says so; `OMSI_DRIVER_SHIFTER=<part of a variable or file name>` forces it); its
//! knob is the far end of the mesh from its turning axis, and the hand that works it is the
//! one on the lever's side of the seat, so left- and right-hand-drive buses alike get the
//! right one. When the lever moves (or the clutch goes down) that hand lets go of the rim,
//! reaches over, rides the knob through the shift (a short push of its own when the model's
//! lever does not move) and goes back to the rim, the other hand keeping the wheel meanwhile.
//! With the bus stopped the hand waits on the knob and the other one holds the wheel; it goes
//! back to the rim when the bus moves off, or when the wheel is turned too far for one hand.
//! A bus with no lever (an automatic's selector is buttons, too small to be taken for one) gets
//! both hands on the wheel, always; `OMSI_DRIVER_SHIFTER=off` does that for any bus.
//!
//! The seat is slid up until the hands reach the wheel and then a little further, until the
//! elbows are bent as a driver's are at a wheel (an arm stretched out straight meant the seat
//! was too far back: the hands had just reached the rim). A hand turned out of its reach lets go
//! a moment before it gets there, the sooner the faster the wheel turns; with the other hand at
//! the gear lever the lone hand pushes the wheel round as far as it can and then takes it again
//! further back (palming it), rather than letting the rim slide through it.

use glam::{DVec3, Mat4, Vec3};
use omsi_render::{AlphaMode, MeshId, Renderer, Scene};
use omsi_sim::human::{
    Activity, HumanType, Pose, PoseInput, curl_hands, grip_centres, hand_slot, skin_from,
};
use std::sync::Arc;

mod car;
mod controls;
mod rig;
mod steering;
pub use car::{DriverCar, SnapshotBus};

use rig::*;
use steering::{Hand, Targets, Wheel, WheelAngleTracker};

pub struct DriverFigure {
    ty: Arc<HumanType>,
    /// The figure's meshes with the fingers closed round the rim (`curl_hands`).
    curled: Vec<(Vec<Vec3>, Vec<Vec3>)>,
    /// Wrist to knuckles (m).
    knuckles: f32,
    /// Where the closed fingers hold their bar, rest frame (see `grip_centres`), and how far
    /// the wrists' targets are moved (person frame) so that the bar they hold is the rim:
    /// the hand does not always turn as far as the grip asks (its bend at the wrist is
    /// limited), and the fingers closed round the air beside the rim.
    grip_rest: [Option<Vec3>; 2],
    grip_fix: [Vec3; 2],
    /// The radius the fingers are closed round (the rim's tube and the fingers' half
    /// thickness) that `curled` and `grip_rest` are made for.
    grip_radius: f32,
    pose: Pose,
    meshes: Vec<(MeshId, usize)>,
    skins: Vec<(Vec<Vec3>, Vec<Vec3>)>,
    /// Hip point, floor point in front of the seat, heading (model frame).
    hip: Vec3,
    floor: Vec3,
    heading: f32,
    /// The seat's four `[interiorlight]`s (see `PassPos::illumination`): the lamps that
    /// light the figure, as Omsi.exe lights a seated person (0x62f7b8).
    lamps: [i32; 4],
    wheel: Option<Wheel>,
    wheel_tracker: WheelAngleTracker,
    /// Extra forward lean that brings the hands to the rim (degrees, see SLIDE_MAX), and
    /// what it is with the hands at their places.
    lean: f32,
    base_lean: f32,
    /// Whether hands/arms should remain visible in cab (first-person) view behind settings.
    pub show_hands_in_cab: bool,
    shown: bool,
    /// Posed at least once (the first pose is settled, not eased in from standing).
    settled: bool,
    /// How far the seat is slid forward so that the hands reach the rim (m): a figure of
    /// the stock size on the LiAZ's `[drivpos]` sat 0.7 m behind its wheel with its arms
    /// stretched out in the air.
    slide: f32,
    /// Where each hand holds the rim, or is on its way to (see `steer_hands`).
    hands: [Hand; 2],
    hands_placed: bool,
    /// The wheel's angle seen from the seat (degrees, clockwise) now and a frame ago, and
    /// how long it has been held still (s).
    theta: f32,
    last_theta: f32,
    still: f32,
    /// How fast the wheel turns (degrees/s, smoothed).
    rate: f32,
    /// The elbows as last posed (model frame): the forearms the hands continue.
    elbows: Option<[Vec3; 2]>,
    /// How each hand lies now (wrist to knuckles, palm; model frame), eased.
    frames: [Option<(Vec3, Vec3)>; 2],
    /// How far each fist is rolled round the rim (degrees), eased.
    rolls: [Option<f32>; 2],
    /// Per mesh and vertex, the hand it belongs to (0 left, 1 right, -1 none), and a
    /// scratch copy of a mesh with a hand opening.
    hand_of: Vec<Vec<i8>>,
    /// Per mesh and vertex, the arm (upper arm and forearm) it belongs to (0 left, 1 right,
    /// -1 none): used to fold each upper arm onto its own elbow in the cab view.
    arm_of: Vec<Vec<i8>>,
    blend: (Vec<Vec3>, Vec<Vec3>),
    /// The gear lever, when the bus's model has one near the seat, and what the hand
    /// working it is doing.
    shifter: Option<Shifter>,
    shift: ShiftState,
    /// Parking-brake lever and how far its hand has reached (0 on the wheel, 1 holding it).
    handbrake: Option<Handbrake>,
    handbrake_w: f32,
    handbrake_hold: f32,
    handbrake_last_applied: Option<bool>,
    door_buttons: Vec<DoorButtonControl>,
    /// Which numbered button the last door command selected (0 = all / release).
    door_button_index: usize,
    /// Trigger fired by the selected door command; lets a mouse press target its exact mesh.
    door_button_trigger: Option<String>,
    /// Mesh hit by a mouse press; several physical pins can share the same trigger.
    door_button_mesh: Option<usize>,
    /// How far the hand has reached from the wheel to the door controls.
    door_button_w: f32,
    door_button_hold: f32,
}

/// The upper-arm and forearm bone slots of each side (0 left, 1 right) in `Posed::bones`,
/// numbered as in `omsi_sim::human` (thighs 0-1, shins 2-3, upper arms 4-5, forearms 6-7).
const UPPER_ARM_SLOT: [usize; 2] = [4, 5];
const FORE_ARM_SLOT: [usize; 2] = [6, 7];

/// Where the hands rest on the rim, from the top, clockwise seen by the driver (degrees):
/// a little above the sides, ten to two as bus drivers hold a flat wheel.
const REST: [f32; 2] = [-70.0, 70.0];
/// Where each hand can hold the rim (degrees from the top): the hands turn with the wheel
/// within it; a hand turned out of it lets go and takes the rim again further back (the
/// other hand holding on meanwhile), as a driver shuffles the wheel through his hands. (The
/// figure used to keep its hands still and let the rim slide through them past a small
/// range: the wheel seemed to turn by itself under hands frozen in the air.)
const RANGE: [(f32, f32); 2] = [(-150.0, -20.0), (20.0, 150.0)];
/// A hand lets go this far (degrees) past its range at most before the rim slides.
const SLIP: f32 = 25.0;
/// How far a fist rolls round the rim (degrees, see `hand_targets`) and the time constant
/// (s) it rolls with.
const ROLL: (f32, f32) = (-30.0, 120.0);
/// The roll where the forearm says nothing about it.
const ROLL_PLAIN: f32 = 30.0;
const ROLL_EASE: f32 = 0.12;
/// How fast the wrists' targets are moved so that the fists hold the rim (time constant, s)
/// and the most they move for it in a frame (m).
const FIX_EASE: f32 = 0.25;
const FIX_STEP: f32 = 0.002;
/// Time constant (s) a hand turns into the frame its hold asks for.
const FRAME_EASE: f32 = 0.07;
/// How far a hand that lets go reaches back (degrees short of the far end of its range).
const REGRIP_BACK: f32 = 40.0;
/// Lifted off the rim while moving to its new hold (m).
const LIFT: f32 = 0.06;
/// The wheel held still this long (s): the hands go back to their rest one after the other.
const SETTLE_AFTER: f32 = 0.6;
/// The rim does not run exactly across the fist where the forearm comes along it: the hand
/// holds it diagonally (the rim from the base of the forefinger to the heel of the hand)
/// with up to this angle (degrees) between the rim and the knuckles, the wrist straight.
const DIAGONAL: f32 = 40.0;
/// A hip over the seat point stands this far in front of it (the feet under the knees).
const SEAT_FRONT: f32 = 0.34;
/// Radius the fingers close round: the rim's own (measured, see `find_wheel`; this one when
/// there is no wheel) and the fingers' half thickness.
const GRIP_RADIUS: f32 = 0.026;
const FINGER_HALF: f32 = 0.009;
/// The most the seat is slid forward to bring the hands to the wheel (m); what is still
/// missing is made up by leaning forward. Slid further, the driver sat in front of his seat
/// (the interior mirror showed the seat empty).
const SLIDE_MAX: f32 = 0.10;
/// A driver at a wheel does not hold his arms out straight: the seat is slid up (and then the
/// body leaned forward by at most `LEAN_COMFORT` degrees) until the wrist is this fraction of
/// the arm's length from the shoulder (1 is an arm stretched out, 0.9 an elbow bent about 130
/// degrees); `UPPER_ARM` is the shoulder-to-elbow length (m) of the stock figure.
const ARM_RATIO: f32 = 0.98;
const LEAN_COMFORT: f32 = 2.0;
const UPPER_ARM: f32 = 0.30;
/// A hand lets go this many seconds ahead of the moment it would leave its range (at most
/// `LEAD_MAX` degrees ahead), and a lone hand (the other at the gear lever) goes this far
/// (degrees) past its range before it lets go.
const REGRIP_LEAD: f32 = 0.08;
const LEAD_MAX: f32 = 14.0;
const ONE_HAND_OVER: f32 = 15.0;
/// What a gear lever measures (m, the diagonal of its parts' box): less is a button or a
/// switch (an automatic's selector), more a panel.

/// Time (s) the hand takes to reach the gear lever from the rim, and to come back.
const REACH_TIME: f32 = 0.22;
/// ... and when a gear is being engaged: the hand is on its way the moment the lever moves.
const REACH_FAST: f32 = 0.12;
const BACK_TIME: f32 = 0.25;
/// The hand stays on the knob this long (s) after the lever (or the clutch) last moved.
const HOLD_AFTER: f32 = 0.08;
/// Length (s) and reach (m) of the push the hand makes when the model's lever does not move
/// by itself, and the travel (m) of the lever from which that push fades out.
const STROKE_TIME: f32 = 0.45;
const STROKE: f32 = 0.035;
const STROKE_FADE: f32 = 0.025;
/// Speeds (m/s) below which the bus counts as stopped (for `STOP_AFTER` s) and above which
/// it counts as moving again.
const STOP_SPEED: f32 = 0.25;
const GO_SPEED: f32 = 0.8;
const STOP_AFTER: f32 = 0.6;
/// The lever's knob moves faster than this (m/s): a gear is being engaged.
const LEVER_MOVING: f32 = 0.015;
/// A shift waits at most this long (s) for the other hand to finish a regrip.
const SHIFT_WAIT: f32 = 2.0;
/// The lever must be within this reach (m) of the driver's shoulder.
const LEVER_REACH: f32 = 1.1;
/// Variables that may carry the gear (watched for changes when the lever has no variable of
/// its own) and the clutch pedal (the hand starts for the lever as the pedal goes down).
const GEAR_VARS: &[&str] = &["Gear", "Gearbox_Gear", "Gear_Selected", "Gang", "Antrieb"];
const CLUTCH_VARS: &[&str] = &["Clutch", "Clutch_Pedal", "Kupplung"];

/// The gear lever of a manual bus, as found in its model (`find_shifter`).
struct Shifter {
    /// The lever mesh (index into the vehicle's meshes).
    mesh: usize,
    /// Variables watched for gear changes: the lever's own and any gear variable the
    /// vehicle has.
    vars: Vec<String>,
    /// The clutch pedal's variable, if the vehicle has one.
    clutch: Option<String>,
    /// Where the hand holds the knob, and the lever's axis (foot to knob), at rest, model
    /// frame.
    grab: Vec3,
    axis: Vec3,
    /// The hand that works it (0 left, 1 right): the one on the lever's side of the seat.
    hand: usize,
}

/// The parking-brake lever and the hand that reaches for it while the brake is applied.
struct Handbrake {
    mesh: usize,
    grab: Vec3,
    axis: Vec3,
    hand: usize,
    variable: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ShiftPhase {
    /// The hand is on the rim.
    Away,
    /// On its way to the knob.
    Reach,
    /// On the knob.
    Hold,
    /// On its way back to the rim.
    Back,
}

struct ShiftState {
    phase: ShiftPhase,
    /// 0 hand on the rim .. 1 hand on the knob (linear, eased where it is used).
    w: f32,
    /// How long (s) the hand takes over the way to the knob this time.
    reach: f32,
    /// The bus has stood still long enough (hysteresis on its speed).
    stopped: bool,
    still_for: f32,
    /// Time (s) the stopped bus's hand stays off the lever after the wheel was turned too
    /// far for one hand.
    cool: f32,
    last_pos: Option<glam::DVec3>,
    last_knob: Option<Vec3>,
    last_vars: Vec<f32>,
    last_clutch: f32,
    was_active: bool,
    /// Time (s) since the lever or the clutch last moved.
    idle: f32,
    /// A shift is under way and the hand has not started for the lever yet.
    pending: bool,
    waiting: f32,
    /// Progress (0..1) of the driver's own push on the knob; 1 when there is none.
    stroke: f32,
    /// Where the knob was when the shift began and how far it has moved since.
    event_from: Vec3,
    event_travel: f32,
}

impl Default for ShiftState {
    fn default() -> Self {
        ShiftState {
            phase: ShiftPhase::Away,
            w: 0.0,
            reach: REACH_TIME,
            stopped: false,
            still_for: 0.0,
            cool: 0.0,
            last_pos: None,
            last_knob: None,
            last_vars: Vec::new(),
            last_clutch: 0.0,
            was_active: false,
            idle: 10.0,
            pending: false,
            waiting: 0.0,
            stroke: 1.0,
            event_from: Vec3::ZERO,
            event_travel: 0.0,
        }
    }
}

/// Ease in and out with no jolt at either end (smootherstep).
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

impl DriverFigure {
    /// The driver for the player's bus, when it has a `[drivpos]` and a driver figure is
    /// installed.
    /// `pick` chooses among the map's drivers (`drivers.txt`): 0 for the player, a vehicle's
    /// own number for the traffic.
    pub fn new(
        world: &crate::World,
        renderer: &Renderer,
        scene: &mut Scene,
        v: &impl DriverCar,
        pick: u64,
    ) -> Option<DriverFigure> {
        let ty = driver_type(world, pick)?;
        Self::new_with(world, renderer, scene, v, ty)
    }

    /// The driver as another game has them (LAN): the figure it names by its `.hum` file
    /// (`hum`: relative to the installation, as `omsi_net::human_path` checked the name that
    /// came over the network), else one of the map's as `new` picks.
    pub fn new_named(
        world: &crate::World,
        renderer: &Renderer,
        scene: &mut Scene,
        v: &impl DriverCar,
        hum: Option<&str>,
        pick: u64,
    ) -> Option<DriverFigure> {
        let named = hum
            .map(|rel| omsi_cfg::resolve_path(&world.root, rel))
            .filter(|p| omsi_cfg::vfs::exists(p))
            .and_then(|p| cached_type(&p));
        let ty = named.or_else(|| driver_type(world, pick))?;
        Self::new_with(world, renderer, scene, v, ty)
    }

    fn new_with(
        world: &crate::World,
        renderer: &Renderer,
        scene: &mut Scene,
        v: &impl DriverCar,
        ty: Arc<HumanType>,
    ) -> Option<DriverFigure> {
        let seat = seat_of(v)?;
        let mut meshes = Vec::new();
        let dirs = ty.texture_dirs(&world.root);
        for hm in &ty.meshes {
            let mut mats = Vec::new();
            for (k, m) in hm.materials.iter().enumerate() {
                let look: Vec<&std::path::Path> = dirs.iter().map(|p| p.as_path()).collect();
                let tex = omsi_texture::find_texture(&m.texture, &look).and_then(|path| {
                    let t = world
                        .textures
                        .get_gpu_fast(&path)
                        .map(|(img, _)| renderer.add_texture_data(scene, &img));
                    world.textures.release(&path);
                    t
                });
                let alpha = match hm.alpha.get(k).copied().unwrap_or(0) {
                    1 => AlphaMode::Test,
                    2 => AlphaMode::Blend,
                    _ => AlphaMode::Opaque,
                };
                mats.push(renderer.add_material(scene, tex, alpha, [1.0; 4], false));
            }
            let id = renderer.add_mesh(scene, &hm.data);
            let inst = renderer.add_instance(scene, id, v.position(), Mat4::IDENTITY, mats);
            // (hidden until `update` has posed and placed it: drawn as loaded, the figure
            // stood in the file's T-pose at the bus's origin, in the middle of the aisle)
            renderer.set_params(scene, inst, &[], false, &[]);
            meshes.push((id, inst));
        }
        let curled = curl_hands(&ty, GRIP_RADIUS);
        let knuckles = (ty.joints.finger - ty.joints.hand)
            .length()
            .clamp(0.12, 0.3)
            * 0.58;
        let hand_of = ty
            .meshes
            .iter()
            .map(|m| {
                m.skin
                    .iter()
                    .map(|inf| {
                        (0..2)
                            .find(|&side| {
                                (0..inf.n as usize).any(|j| {
                                    inf.slot[j] as usize == hand_slot(side) && inf.weight[j] > 0.5
                                })
                            })
                            .map(|side| side as i8)
                            .unwrap_or(-1)
                    })
                    .collect()
            })
            .collect();
        let arm_of: Vec<Vec<i8>> = ty
            .meshes
            .iter()
            .map(|m| {
                m.skin
                    .iter()
                    .map(|inf| {
                        (0..2)
                            .find(|&side| {
                                let w: f32 = (0..inf.n.max(1) as usize)
                                    .filter(|&j| {
                                        let s = inf.slot[j] as usize;
                                        s == UPPER_ARM_SLOT[side] || s == FORE_ARM_SLOT[side]
                                    })
                                    .map(|j| if inf.n <= 1 { 1.0 } else { inf.weight[j] })
                                    .sum();
                                w > 0.5
                            })
                            .map(|side| side as i8)
                            .unwrap_or(-1)
                    })
                    .collect()
            })
            .collect();
        let grip_rest = grip_centres(&ty, GRIP_RADIUS);
        let mut f = DriverFigure {
            ty,
            curled,
            knuckles,
            grip_rest,
            grip_fix: [Vec3::ZERO; 2],
            grip_radius: GRIP_RADIUS,
            pose: Pose::new(0x5eed_d71e),
            meshes,
            skins: Vec::new(),
            hip: Vec3::ZERO,
            floor: Vec3::ZERO,
            heading: 0.0,
            lamps: [-1; 4],
            wheel: None,
            wheel_tracker: WheelAngleTracker::default(),
            lean: 0.0,
            base_lean: 0.0,
            show_hands_in_cab: false,
            shown: false,
            settled: false,
            slide: 0.0,
            hands: [Hand::default(); 2],
            hands_placed: false,
            theta: 0.0,
            last_theta: 0.0,
            still: 0.0,
            rate: 0.0,
            elbows: None,
            frames: [None; 2],
            rolls: [None; 2],
            hand_of,
            arm_of,
            blend: Default::default(),
            shifter: None,
            shift: ShiftState::default(),
            handbrake: None,
            handbrake_w: 0.0,
            handbrake_hold: 0.0,
            handbrake_last_applied: None,
            door_buttons: Vec::new(),
            door_button_index: 0,
            door_button_trigger: None,
            door_button_mesh: None,
            door_button_w: 0.0,
            door_button_hold: 0.0,
        };
        f.seat_in(v, seat);
        Some(f)
    }

    /// Put the figure into (another) vehicle's driver's seat: `false` when it has none.
    /// The traffic keeps a few figures and moves them from bus to bus.
    pub fn attach(&mut self, v: &impl DriverCar) -> bool {
        match seat_of(v) {
            Some(seat) => {
                self.seat_in(v, seat);
                true
            }
            None => false,
        }
    }

    /// Start a short reach to press a door control, then bring the hand back to the wheel.
    /// Called for player-operated door buttons, never for doors moved by the bus script.
    pub fn door_button_pressed(
        &mut self,
        index: usize,
        trigger: Option<&str>,
        mesh: Option<usize>,
    ) {
        self.door_button_index = index;
        self.door_button_trigger = trigger.map(str::to_ascii_lowercase);
        self.door_button_mesh = mesh;
        self.door_button_hold = 0.35;
    }

    /// The physical key group for a trigger attached to one of the bus's door controls.
    /// This also recognizes a model event that writes the same door leaf as the key trigger.
    pub fn door_button_group_for_trigger(&self, trigger: &str) -> Option<usize> {
        self.door_buttons
            .iter()
            .find(|button| button.event.as_deref().is_some_and(|event| event.eq_ignore_ascii_case(trigger)))
            .map(|button| button.group.unwrap_or(0))
    }

    /// The physical key group for a door-control mesh, even if its event name is shared.
    pub fn door_button_group_for_mesh(&self, mesh: usize) -> Option<usize> {
        self.door_buttons
            .iter()
            .find(|button| button.mesh == mesh)
            .map(|button| button.group.unwrap_or(0))
    }

    fn seat_in(&mut self, v: &impl DriverCar, seat: omsi_vehicle::cabin::PassPos) {
        let hip = Vec3::from(seat.pos);
        let r = seat.rot.to_radians();
        self.hip = hip;
        self.floor = Vec3::new(
            hip.x + r.sin() * SEAT_FRONT,
            hip.y + r.cos() * SEAT_FRONT,
            hip.z - seat.height.max(0.3),
        );
        self.heading = seat.rot;
        self.lamps = seat.illumination;
        self.wheel = find_wheel(v, hip);
        self.shifter = find_shifter(v, hip, seat.rot);
        self.handbrake = find_handbrake(v, hip, seat.rot).map(|h| Handbrake {
            mesh: h.mesh,
            grab: h.grab,
            axis: h.axis,
            hand: h.hand,
            variable: h.variable,
        });
        self.door_buttons = find_door_buttons(v, hip, seat.rot);
        self.handbrake_w = 0.0;
        self.handbrake_hold = 0.0;
        self.handbrake_last_applied = self
            .handbrake
            .as_ref()
            .and_then(|hb| v.var(&hb.variable).map(|value| value > 0.5));
        self.door_button_w = 0.0;
        self.door_button_hold = 0.0;
        self.door_button_trigger = None;
        self.door_button_mesh = None;
        self.shift = ShiftState::default();
        let r = self
            .wheel
            .as_ref()
            .map(|w| w.tube + FINGER_HALF)
            .unwrap_or(GRIP_RADIUS);
        if (r - self.grip_radius).abs() > 1e-4 {
            self.curled = curl_hands(&self.ty, r);
            self.grip_rest = grip_centres(&self.ty, r);
            self.grip_radius = r;
        }
        self.wheel_tracker = WheelAngleTracker::default();
        self.grip_fix = [Vec3::ZERO; 2];
        self.pose = Pose::new(0x5eed_d71e);
        self.settled = false;
        self.slide = 0.0;
        self.lean = 0.0;
        self.base_lean = 0.0;
        self.hands_placed = false;
        self.elbows = None;
        self.frames = [None; 2];
        self.rolls = [None; 2];
        if let Some(sh) = &self.shifter {
            log::debug!(
                "driver: gear lever (mesh {}, vars {:?}) worked with the {} hand",
                sh.mesh,
                sh.vars,
                if sh.hand == 0 { "left" } else { "right" }
            );
        }
        if let Some(hb) = &self.handbrake {
            log::debug!(
                "driver: parking-brake lever (mesh {}, variable {}) worked with the {} hand",
                hb.mesh,
                hb.variable,
                if hb.hand == 0 { "left" } else { "right" }
            );
        }
        log::debug!(
            "driver: {} on [drivpos] ({:.2}, {:.2}, {:.2}){}",
            self.ty
                .def
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            hip.x,
            hip.y,
            hip.z,
            self.wheel
                .as_ref()
                .map(|w| format!(
                    ", hands on the wheel (rim {:.2} m at ({:.2}, {:.2}, {:.2}))",
                    w.radius, w.centre.x, w.centre.y, w.centre.z
                ))
                .unwrap_or_else(|| ", no steering wheel found: hands on the lap".into())
        );
    }

    /// Hide the figure (kept for another bus).
    pub fn hide(&mut self, renderer: &Renderer, scene: &mut Scene) {
        for (_, inst) in &self.meshes {
            renderer.set_params(scene, *inst, &[], false, &[]);
        }
        self.shown = false;
    }

    pub fn follow(&self, renderer: &Renderer, scene: &mut Scene, position: DVec3, body: Mat4) {
        if !self.shown || !self.settled {
            return;
        }
        let h = self.heading.to_radians();
        let floor = self.floor + Vec3::new(h.sin(), h.cos(), 0.0) * self.slide;
        let at = position + body.transform_point3(floor).as_dvec3();
        let xf = body * Mat4::from_rotation_z(-h);
        for (_, inst) in &self.meshes {
            renderer.set_transform(scene, *inst, at, xf);
        }
    }

    /// Turn the hands with the wheel, pose, skin and place the figure; `show` false hides it,
    /// `mirror_only` keeps it out of the window's picture but in the mirrors (the driver's
    /// own view: OMSI shows the driver in the mirrors while one looks from his seat).
    pub fn update(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        v: &impl DriverCar,
        render: &crate::VehicleRender,
        dt: f32,
        show: bool,
        mirror_only: bool,
    ) {
        if show != self.shown {
            for (_, inst) in &self.meshes {
                renderer.set_params(scene, *inst, &[], show, &[]);
            }
            self.shown = show;
        }

        // (in the cab with the hands shown: the figure is drawn in the window's picture too,
        // with everything but the hands folded away, see the skinning below)
        let force_visible_in_cab = mirror_only && self.show_hands_in_cab;
        for (_, inst) in &self.meshes {
            renderer.set_mirror_only(
                scene,
                *inst,
                if force_visible_in_cab {
                    false
                } else {
                    mirror_only
                },
            );
        }

        if !show {
            return;
        }
        if self.settled {
            self.update_handbrake(v, dt);
            self.update_shift(v, dt);
            self.update_door_button(dt);
        }
        if let Some(theta) = self.wheel_angle(v, dt) {
            self.steer_hands(theta, if self.settled { dt } else { 0.0 });
        }
        // the person's own frame: feet at `floor` (slid forward by `slide`), facing `heading`
        let h = self.heading.to_radians();
        let fwd = Vec3::new(h.sin(), h.cos(), 0.0);
        if !self.settled {
            self.settle_at_wheel(v, fwd);
            return self.update(renderer, scene, v, render, dt, show, mirror_only);
        }
        let targets = self.hand_targets(v, dt);
        if let (Some(t), true) = (&targets, omsi_cfg::flags::OMSI_DEBUG_DRIVER.is_set()) {
            log::info!(
                "HANDT {dt:.4} {:?} {:?} {:?} {:?} seen {:.1} {}",
                t.grips[0].to_array(),
                t.frames[0].0.to_array(),
                t.frames[0].1.to_array(),
                t.grips[1].to_array(),
                self.hands[0].seen(self.theta).0,
                self.hands[0].mv.is_some()
            );
        }
        let input = self.pose_input(targets.as_ref(), fwd);
        let floor = self.floor + fwd * self.slide;
        self.pose.advance(&self.ty.rig, &input, dt);
        let posed = self.pose.bones(&self.ty.rig);
        if posed.ok {
            self.keep_elbows(&posed.elbow);
        }
        if omsi_cfg::flags::OMSI_DEBUG_DRIVER.is_set() {
            log::info!(
                "HANDP {dt:.4} {:?} {:?} {:?} lean {:.2}",
                posed.wrist[0].to_array(),
                posed.elbow[0].to_array(),
                self.grip_fix[0].to_array(),
                self.lean
            );
        }
        if let (Some(t), true) = (&targets, posed.ok) {
            // (drawn this frame as posed; the next frame holds the rim) - a hand on its way
            // to a new hold keeps the correction it had
            let tubes = t.tubes.map(|q| self.to_person(q));
            let holding = [0, 1].map(|k| self.hands[k].mv.is_none());
            let off =
                self.correct_grips(&posed.bones, tubes, 1.0 - (-dt / FIX_EASE).exp(), holding);
            if omsi_cfg::flags::OMSI_DEBUG_DRIVER.is_set() {
                log::info!(
                    "driver: grip off the rim {off:.3} m, fix {:?}",
                    self.grip_fix
                );
            }
        }
        // a hold the arms do not quite reach (the top of a tilted wheel is further off than
        // its sides): lean towards it, and back again when the hands are nearer
        if let (Some(g), true) = (input.grips, posed.ok && dt > 0.0) {
            let miss = (0..2)
                .map(|k| (posed.wrist[k] - g[k]).length())
                .fold(0.0f32, f32::max);
            if miss > 0.015 {
                self.lean = (self.lean + (miss / 0.011) * dt * 4.0)
                    .min(34.0)
                    .min(self.base_lean + 6.0);
            } else if miss < 0.006 {
                self.lean = (self.lean - 4.0 * dt).max(self.base_lean);
            }
        }
        if omsi_cfg::flags::OMSI_DEBUG_DRIVER.is_set() {
            if let Some(g) = input.grips {
                log::info!(
                    "driver: wheel {:.0} deg (sign {}), hands at {:.0} {:.0}{}{}, lean {:.1}, miss {:.3} {:.3}",
                    self.theta,
                    self.wheel_tracker.sign(),
                    self.hands[0].seen(self.theta).0,
                    self.hands[1].seen(self.theta).0,
                    if self.hands[0].mv.is_some() {
                        " (left regrips)"
                    } else {
                        ""
                    },
                    if self.hands[1].mv.is_some() {
                        " (right regrips)"
                    } else {
                        ""
                    },
                    self.lean,
                    (posed.wrist[0] - g[0]).length(),
                    (posed.wrist[1] - g[1]).length()
                );
            }
        }
        if !posed.ok && !self.skins.is_empty() {
            return;
        }
        self.skins
            .resize_with(self.ty.meshes.len(), Default::default);
        let open = [0, 1].map(|k| {
            self.hands[k]
                .open()
                .max(self.shift_open(k))
                .max(self.handbrake_open(k))
                .max(self.door_button_open(k))
        });
        for (k, m) in self.ty.meshes.iter().enumerate() {
            let (pos, nrm) = &mut self.skins[k];
            if open.iter().any(|&o| o > 0.01) {
                // a hand on its way to a new hold opens its fingers
                let blend = &mut self.blend;
                blend.0.clone_from(&self.curled[k].0);
                blend.1.clone_from(&self.curled[k].1);
                for (i, side) in self.hand_of[k].iter().enumerate() {
                    if *side >= 0 && open[*side as usize] > 0.01 {
                        let o = open[*side as usize];
                        blend.0[i] = blend.0[i].lerp(m.data.positions[i], o);
                        blend.1[i] = blend.1[i].lerp(m.data.normals[i], o).normalize_or_zero();
                    }
                }
                skin_from(m, blend, &posed.bones, pos, nrm);
            } else {
                skin_from(m, &self.curled[k], &posed.bones, pos, nrm);
            }

            // Keep the hands and forearms in the cab. Collapse each upper arm onto its
            // posed elbow so the joining triangles close there instead of stretching back
            // towards the shoulder. Other views use the complete, freshly skinned figure.
            if mirror_only && self.show_hands_in_cab {
                let (hand_of, arm_of) = (&self.hand_of[k], &self.arm_of[k]);
                let count = pos.len().min(hand_of.len()).min(arm_of.len());
                for i in 0..count {
                    if hand_of[i] >= 0 {
                        continue;
                    }
                    let inf = &m.skin[i];
                    if arm_of[i] >= 0 {
                        let side = arm_of[i] as usize;
                        let upper: f32 = (0..inf.n.max(1) as usize)
                            .filter(|&j| inf.slot[j] as usize == UPPER_ARM_SLOT[side])
                            .map(|j| if inf.n <= 1 { 1.0 } else { inf.weight[j] })
                            .sum();
                        if upper > 0.5 {
                            pos[i] = posed.elbow[side];
                        }
                    } else {
                        let here = pos[i];
                        pos[i] = posed
                            .elbow
                            .iter()
                            .copied()
                            .min_by(|a, b| {
                                a.distance_squared(here)
                                    .total_cmp(&b.distance_squared(here))
                            })
                            .unwrap_or(here);
                    }
                }
            }

            renderer.update_mesh(scene, self.meshes[k].0, pos, nrm, &m.data.uvs);
        }
        let body = v.body_rotation();
        let at = v.position() + body.transform_point3(floor).as_dvec3();
        let xf = body * Mat4::from_rotation_z(-h);
        // lit by the seat's four lamps as Omsi.exe lights a seated person (0x62f7b8 enables
        // them as Direct3D lights for the figure): the bus meshes' point lights, coloured,
        // falling off with distance and only on the side facing them. (A flat warm term of
        // the lamps' sum lit the figure evenly on every side, glowing in the dark cab.)
        let (first, count) = render
            .seat_lamps(v.ty().model.interior_lights.len(), &self.lamps)
            .unwrap_or((0, 0));
        for (_, inst) in &self.meshes {
            renderer.set_transform(scene, *inst, at, xf);
            renderer.set_interior(scene, *inst, 0.0);
            renderer.set_interior_lamps(scene, *inst, first, count);
        }
    }

    fn settle_at_wheel(&mut self, v: &impl DriverCar, fwd: Vec3) {
        // sat down already when first seen (an offscreen picture is one frame), the
        // seat slid up until the hands reach the rim
        // how much of the lean is for the elbows' sake, not the reach
        let mut comfort_extra = 0.0f32;
        for round in 0..28 {
            let mut p = Pose::new(0x5eed_d71e);
            let targets = self.hand_targets(v, 0.0);
            let try_input = self.pose_input(targets.as_ref(), fwd);
            for _ in 0..90 {
                p.advance(&self.ty.rig, &try_input, 1.0 / 30.0);
            }
            let posed = p.bones(&self.ty.rig);
            let miss = match (try_input.grips, posed.ok) {
                (Some(g), true) => (0..2)
                    .map(|k| (posed.wrist[k] - g[k]).length())
                    .fold(0.0f32, f32::max),
                _ => 0.0,
            };
            // An arm stretched out straight to the rim says the seat is too far back (the
            // hands had just reached it): how far the wrists are from a driver's bent arms.
            let excess = if posed.ok {
                self.arm_excess(&posed.elbow, &posed.wrist)
            } else {
                0.0
            };
            if omsi_cfg::flags::OMSI_DEBUG_DRIVER.is_set() {
                log::info!(
                    "driver settle {round}: excess {excess:.3} slide {:.2} grips {:?} wrists {:?} elbows {:?} neck {:?} hip {:?}",
                    self.slide,
                    try_input.grips,
                    posed.wrist,
                    posed.elbow,
                    posed.neck,
                    posed.hip
                );
            }
            // the hand's frame follows the forearm: pose again until both settle
            let moved = if posed.ok {
                self.keep_elbows(&posed.elbow)
            } else {
                0.0
            };
            let off = match (&targets, posed.ok) {
                (Some(t), true) => {
                    let tubes = t.tubes.map(|q| self.to_person(q));
                    self.correct_grips(&posed.bones, tubes, 1.0, [true; 2])
                }
                _ => 0.0,
            };
            let comfort_left = self.slide < SLIDE_MAX || comfort_extra < LEAN_COMFORT - 0.01;
            let still_posing = miss < 0.02 && off < 0.01 && moved < 0.01;
            if (still_posing && (excess < 0.02 || !comfort_left))
                || (self.slide >= SLIDE_MAX && self.lean >= 30.0)
                || round == 27
            {
                self.pose = p;
                break;
            }
            if miss >= 0.02 {
                if self.slide < SLIDE_MAX {
                    self.slide = (self.slide + miss * 0.8).min(SLIDE_MAX);
                } else {
                    // about 1.1 cm of reach per degree of lean for a seated adult
                    self.lean = (self.lean + (miss / 0.011).max(2.0)).min(30.0);
                }
            } else if excess >= 0.02 && comfort_left {
                // reached, but with the arms straighter than a driver holds them: the seat
                // closer to the wheel first, then a little forward from the hips
                if self.slide < SLIDE_MAX {
                    self.slide = (self.slide + excess * 0.8).min(SLIDE_MAX);
                } else {
                    let add = (excess / 0.011)
                        .clamp(1.0, 3.0)
                        .min(LEAN_COMFORT - comfort_extra);
                    self.lean += add;
                    comfort_extra += add;
                }
            }
            // (else only the fingers' hold or the forearm moved: pose again)
        }
        self.settled = true;
        self.base_lean = self.lean;
        log::debug!(
            "driver: seat slid {:.2} m forward and {:.0} deg of lean to reach the wheel ({:.0} of them for the elbows)",
            self.slide,
            self.lean,
            comfort_extra
        );
    }
}

impl DriverFigure {
    /// The pose input of the driver at the wheel (the grips in the person's frame).
    fn pose_input(&self, targets: Option<&Targets>, fwd: Vec3) -> PoseInput<'static> {
        let h = self.heading.to_radians();
        let turn_person = move |d: Vec3| {
            Vec3::new(
                d.x * h.cos() - d.y * h.sin(),
                d.x * h.sin() + d.y * h.cos(),
                d.z,
            )
        };
        let floor = self.floor + fwd * self.slide;
        let hip = self.hip + fwd * self.slide;
        PoseInput {
            activity: Activity::Sit,
            origin: floor.as_dvec3(),
            heading: self.heading as f64,
            frame: 1,
            seat: Some(self.to_person(hip)),
            look: Some(self.to_person(hip + fwd * 20.0 + Vec3::Z * 0.4)),
            grips: targets.map(|t| [0, 1].map(|k| self.to_person(t.grips[k]) + self.grip_fix[k])),
            grip_frames: targets.map(|t| t.frames.map(|(d, p)| (turn_person(d), turn_person(p)))),
            grip_lean: self.lean,
            ..Default::default()
        }
    }

    /// A model-frame point in the person's frame (feet at the slid floor point, facing the
    /// seat's heading), and back.
    fn to_person(&self, q: Vec3) -> Vec3 {
        let h = self.heading.to_radians();
        let d = q - (self.floor + Vec3::new(h.sin(), h.cos(), 0.0) * self.slide);
        Vec3::new(
            d.x * h.cos() - d.y * h.sin(),
            d.x * h.sin() + d.y * h.cos(),
            d.z,
        )
    }

    fn from_person(&self, d: Vec3) -> Vec3 {
        let h = self.heading.to_radians();
        let floor = self.floor + Vec3::new(h.sin(), h.cos(), 0.0) * self.slide;
        floor
            + Vec3::new(
                d.x * h.cos() + d.y * h.sin(),
                -d.x * h.sin() + d.y * h.cos(),
                d.z,
            )
    }

    /// Keep the posed elbows (person frame) for the next hands' frames; how far they moved.
    fn keep_elbows(&mut self, posed: &[Vec3; 2]) -> f32 {
        let now = posed.map(|e| self.from_person(e));
        let moved = self
            .elbows
            .map(|e| (0..2).map(|k| (e[k] - now[k]).length()).fold(0.0, f32::max))
            .unwrap_or(1.0);
        self.elbows = Some(now);
        moved
    }

    /// Move the wrists' targets by `gain` of what separates the bar the posed fingers hold
    /// from the rim (`tubes`, person frame); the largest distance left.
    fn correct_grips(
        &mut self,
        bones: &[glam::Affine3A],
        tubes: [Vec3; 2],
        gain: f32,
        which: [bool; 2],
    ) -> f32 {
        let mut worst = 0.0f32;
        for k in (0..2).filter(|&k| which[k]) {
            let Some(rest) = self.grip_rest[k] else {
                continue;
            };
            let Some(b) = bones.get(hand_slot(k)) else {
                continue;
            };
            let held = Vec3::from(b.transform_point3a(rest.into()));
            let err = tubes[k] - held;
            if !err.is_finite() {
                continue;
            }
            worst = worst.max(err.length());
            // (a few millimetres a frame at most: a larger step read as the hand jumping)
            let step = err * gain;
            let step = if gain < 1.0 {
                step.clamp_length_max(FIX_STEP)
            } else {
                step
            };
            let fix = self.grip_fix[k] + step;
            self.grip_fix[k] = fix.clamp_length_max(0.15);
        }
        worst
    }
}

fn frame_quat(d: Vec3, p: Vec3) -> glam::Quat {
    glam::Quat::from_mat3(&glam::Mat3::from_cols(d, p, d.cross(p))).normalize()
}

/// The vehicle's first `[drivpos]`.

fn seat_of(v: &impl DriverCar) -> Option<omsi_vehicle::cabin::PassPos> {
    cabin_of(&v.ty().def)?.driver_positions.first().cloned()
}

/// A vehicle type's passenger cabin, read once per type.
pub fn cabin_of(def: &omsi_vehicle::Vehicle) -> Option<Arc<omsi_vehicle::PassengerCabin>> {
    static CABINS: std::sync::Mutex<
        Option<
            std::collections::HashMap<
                std::path::PathBuf,
                Option<Arc<omsi_vehicle::PassengerCabin>>,
            >,
        >,
    > = std::sync::Mutex::new(None);
    let mut cabins = CABINS.lock().unwrap_or_else(|e| e.into_inner());
    cabins
        .get_or_insert_with(Default::default)
        .entry(def.path.clone())
        .or_insert_with(|| {
            let rel = def.passenger_cabin.as_ref()?;
            omsi_vehicle::PassengerCabin::load(&omsi_cfg::resolve_path(def.dir(), rel))
                .map_err(|e| log::warn!("driver: {e}"))
                .ok()
                .map(Arc::new)
        })
        .clone()
}

/// `a` turned towards `b` by `max` radians at most (both unit vectors).
fn turn_towards(a: Vec3, b: Vec3, max: f32) -> Vec3 {
    let angle = a.angle_between(b);
    if angle <= max || !angle.is_finite() {
        return b;
    }
    let axis = a.cross(b).normalize_or_zero();
    if axis == Vec3::ZERO {
        return a;
    }
    glam::Quat::from_axis_angle(axis, max) * a
}

impl DriverFigure {
    /// Allow photo shutter samples to move the driver with the bus.
    pub fn instances(&self) -> impl Iterator<Item = usize> + '_ {
        self.meshes.iter().map(|m| m.1)
    }

    /// The figure at the wheel (the player's own when getting up).
    pub fn human_type(&self) -> Arc<HumanType> {
        self.ty.clone()
    }
}

/// The driver figure: one of the map's `drivers.txt` (the human files OMSI draws at the
/// wheel of its buses - Spandau and Grundorf name `humans\\axyz\\man01.hum`; OMSI
/// reads the list with the map, the original), chosen by `pick`; without the list OMSI's own
/// driver figure `Humans/*/DBC_man04_driver.hum`. Each file is read once.
/// The figure `DriverFigure::new` gives a driver with this `pick` (0: the player): also the
/// player's figure on foot, so that getting out of the bus or having none changes no clothes.
pub fn driver_type(world: &crate::World, pick: u64) -> Option<Arc<HumanType>> {
    let listed: Vec<std::path::PathBuf> =
        omsi_map::ailists::load_list(&world.map_dir.join("drivers.txt"))
            .iter()
            .map(|l| omsi_cfg::resolve_path(&world.root, l))
            .collect();
    let path = if listed.is_empty() {
        let mut found: Vec<std::path::PathBuf> = Vec::new();
        for r in omsi_cfg::content_dirs("Humans") {
            for (group, is_dir) in omsi_cfg::vfs::list_dir(&r).unwrap_or_default() {
                if !is_dir {
                    continue;
                }
                let d = r.join(&group);
                for (n, _) in omsi_cfg::vfs::list_dir(&d).unwrap_or_default() {
                    let lower = n.to_string_lossy().to_ascii_lowercase();
                    if lower.ends_with(".hum") && lower.contains("driver") {
                        found.push(d.join(&n));
                    }
                }
            }
        }
        found.sort_by_key(|p| {
            let n = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            (!n.starts_with("dbc_man04"), n)
        });
        found.into_iter().next()?
    } else {
        listed[(pick % listed.len() as u64) as usize].clone()
    };
    cached_type(&path)
}

/// A driver figure's type, read once per file.
pub fn cached_type(path: &std::path::Path) -> Option<Arc<HumanType>> {
    static TYPES: std::sync::Mutex<
        Option<std::collections::HashMap<std::path::PathBuf, Option<Arc<HumanType>>>>,
    > = std::sync::Mutex::new(None);
    let path = path.to_path_buf();
    let mut types = TYPES.lock().unwrap_or_else(|e| e.into_inner());
    types
        .get_or_insert_with(Default::default)
        .entry(path.clone())
        .or_insert_with(|| {
            HumanType::load(&path)
                .map_err(|e| log::warn!("driver {}: {e:#}", path.display()))
                .ok()
                .map(Arc::new)
        })
        .clone()
}
