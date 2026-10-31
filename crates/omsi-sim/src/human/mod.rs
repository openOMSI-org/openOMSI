//! Humans (`.hum`): skinned models posed procedurally from the joint positions of the
//! `[links]` block, like the original's `mc_human`. Bones are addressed by the engine ids
//! given by `[setbone]` (-2 … -14): thighs, shins, upper arms, forearms, hip, torso, head, hands.
//!
//! OMSI ships no animation clips, so every pose is made here. Every stock and add-on model
//! is stored in a T-pose (arms straight out at shoulder height) with the feet part of the
//! shin bones; the rig is measured from the `[links]` joints and the mesh itself (where the
//! soles, heels, toes and ankles are), and the feet get a bone of their own split off the
//! shins so that they can stay flat on the floor, strike with the heel and push off with
//! the toes.
//!
//! [`Pose`] is the per-person animation state, advanced every frame by [`Pose::advance`]
//! from a [`PoseInput`] (where the person stands, how fast they move, whether they sit,
//! pay, brace or look at something); [`Pose::bones`] turns it into bone transforms and
//! [`skin`] deforms a mesh with them. Walking is a speed-driven gait with planted feet: a
//! foot on the floor stays where it was put (in the frame of the floor it stands on - the
//! ground or a bus) and the legs reach it by two-bone IK, so a stride always matches the
//! ground speed and nothing slides; the swinging foot flies to where the body will be when
//! it lands. The same stepping carries standing people through turns and shuffles, and the
//! arms reach the cash desk by IK as well.

use anyhow::{Context, Result};
use glam::{Affine3A, DVec2, DVec3, Mat3A, Quat, Vec2, Vec3, Vec3A};
use omsi_content::Human;
use omsi_geometry::{mesh_from_o3d, MeshData};
use omsi_model::Model;
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};

mod support;

#[cfg(test)]
mod contact_tests;

pub mod avatar;
mod bones;
mod gesture;
mod hail_look;
mod hail_motion;
pub use gesture::accessory_hail_arm;
pub use hail_look::{hail_look_target, HailLook};
pub use hail_motion::HailWrist;
pub mod face;
mod motion;
mod rig;
mod skin;
#[cfg(test)]
mod tests;

pub use rig::{HumanMesh, HumanType, Influence, Joints, Rig};
pub use skin::{curl_hands, grip_centres, hand_slot, skin, skin_from, slots_from_omsi};
#[cfg(test)]
use rig::split_feet;

/// Engine bone ids of `[setbone]`.
pub const BONE_OS_L: i32 = -2;
pub const BONE_OS_R: i32 = -3;
pub const BONE_US_L: i32 = -4;
pub const BONE_US_R: i32 = -5;
pub const BONE_OA_L: i32 = -6;
pub const BONE_OA_R: i32 = -7;
pub const BONE_UA_L: i32 = -8;
pub const BONE_UA_R: i32 = -9;
pub const BONE_HIP: i32 = -10;
pub const BONE_MAIN: i32 = -11;
pub const BONE_HEAD: i32 = -12;
pub const BONE_HAND_L: i32 = -13;
pub const BONE_HAND_R: i32 = -14;

/// Bone transform slots: the thirteen engine bones (id -2 … -14 → slot 0 … 12), the two
/// feet split off the shins and the toes split off the feet.
pub const SLOTS: usize = 17;
const THIGH: [usize; 2] = [0, 1];
const SHIN: [usize; 2] = [2, 3];
const UPPER: [usize; 2] = [4, 5];
const FORE: [usize; 2] = [6, 7];
const HIP: usize = 8;
const MAIN: usize = 9;
const HEAD: usize = 10;
const HAND: [usize; 2] = [11, 12];
const FOOT: [usize; 2] = [13, 14];
const TOE: [usize; 2] = [15, 16];
/// Index 0 is the left side, 1 the right; this is the sign of x on that side.
const SIDE: [f32; 2] = [-1.0, 1.0];

/// The transform slot of an engine bone id.
pub fn slot_of(id: i32) -> Option<usize> {
    (BONE_HAND_R..=BONE_OS_L)
        .contains(&id)
        .then(|| (-id - 2) as usize)
}

fn bone_id_by_name(name: &str) -> Option<i32> {
    match name.trim().to_ascii_lowercase().replace('_', "").as_str() {
        "osl" => Some(BONE_OS_L),
        "osr" => Some(BONE_OS_R),
        "usl" => Some(BONE_US_L),
        "usr" => Some(BONE_US_R),
        "oal" => Some(BONE_OA_L),
        "oar" => Some(BONE_OA_R),
        "ual" => Some(BONE_UA_L),
        "uar" => Some(BONE_UA_R),
        "hip" => Some(BONE_HIP),
        "main" => Some(BONE_MAIN),
        "head" => Some(BONE_HEAD),
        "handl" => Some(BONE_HAND_L),
        "handr" => Some(BONE_HAND_R),
        _ => None,
    }
}

/// What a human is doing; drives the pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Activity {
    Stand,
    Walk,
    Sit,
    /// Standing at the cash desk with the right hand held out.
    Pay,
}

/// What the animation needs to know about a person this frame. Points and directions are
/// in the person's model frame (x right, y forward, z up, origin at the feet) unless they
/// say otherwise.
#[derive(Clone, Copy)]
pub struct PoseInput<'a> {
    pub activity: Activity,
    /// Where the person stands and which way they face, in the frame of the floor they are
    /// on (the world, or a bus): position of the feet, heading in degrees (OMSI's, 0 = +y,
    /// clockwise).
    pub origin: DVec3,
    pub heading: f64,
    /// Which floor frame `origin` is in (0 the ground, else a bus); a change keeps the feet
    /// where they are and re-expresses them in the new frame.
    pub frame: u64,
    /// Ground velocity in that frame (m/s).
    pub velocity: DVec2,
    /// While sitting: the seat point (`[passpos]`, the hip) in the model frame.
    pub seat: Option<Vec3>,
    /// While sitting: where the pelvis itself goes (model frame), when the caller knows it
    /// better than the seat point does - a passenger takes it from the original seated pose,
    /// which sits on every kind of seat (a bench at a stop has no person standing in front
    /// of its hip point, as `seat` takes for granted).
    pub seat_pelvis: Option<Vec3>,
    /// Something to look at.
    pub look: Option<Vec3>,
    /// Where the right hand reaches (the cash desk).
    pub reach: Option<Vec3>,
    /// Where both hands hold on (left, right): the driver's hands on the steering wheel.
    /// Followed as given every frame, no easing (the caller moves them).
    pub grips: Option<[Vec3; 2]>,
    /// How each gripping hand lies (left, right): the direction from the wrist to the
    /// knuckles and the direction the palm faces, model frame. Without it the hand goes on
    /// straight from the forearm.
    pub grip_frames: Option<[(Vec3, Vec3); 2]>,
    /// Extra forward lean towards the grips (degrees): a driver whose wheel is far off leans
    /// to it from the seat instead of leaving it.
    pub grip_lean: f32,
    /// Steadied against the bus's sway, standing in it (0 … 1).
    pub hold: f32,
    /// Acceleration of the floor under the person (a moving bus), model frame, m/s².
    pub sway: Vec2,
    /// Floor height at a point of the floor frame, when the floor is not flat.
    pub floor: Option<&'a dyn Fn(DVec2) -> Option<f64>>,
}

impl Default for PoseInput<'_> {
    fn default() -> Self {
        PoseInput {
            activity: Activity::Stand,
            origin: DVec3::ZERO,
            heading: 0.0,
            frame: 0,
            velocity: DVec2::ZERO,
            seat: None,
            seat_pelvis: None,
            look: None,
            reach: None,
            grips: None,
            grip_frames: None,
            grip_lean: 0.0,
            hold: 0.0,
            sway: Vec2::ZERO,
            floor: None,
        }
    }
}

/// One foot: planted on the floor (frame coordinates of the floor point under the ankle
/// and the foot's heading), or in the air on its way to the next place.
#[derive(Debug, Clone, Copy)]
struct Foot {
    planted: bool,
    /// Floor point under the ankle, and heading (degrees), in the floor frame.
    pos: DVec3,
    yaw: f64,
    /// Swing: where it left (ankle position and pitch at lift-off) and where it goes.
    from_ankle: DVec3,
    from_yaw: f64,
    from_pitch: f32,
    from_floor: f64,
    to: DVec3,
    to_yaw: f64,
    /// Floor height sampled at `to` (and where that was).
    to_floor: f64,
    to_sampled: DVec2,
    /// The landing height the swing aims at: follows `to_floor` quickly but never in a
    /// jump (the target crossing the edge of a step changed it by 30-40 cm mid-swing and
    /// the foot snapped up with it), and how long a landing has waited for it.
    land_z: f64,
    land_wait: f32,
    /// Swing progress 0 … 1 and length (s) of a step not driven by the gait.
    t: f32,
    /// A walking step: the gait's swing progress when the foot left (the swing's own
    /// progress runs from there to the landing).
    t0: f32,
    dur: f32,
    /// This step belongs to the walk (heel strike and push-off) rather than a shuffle.
    walk: bool,
    lift: f32,
}

impl Foot {
    fn new() -> Foot {
        Foot {
            planted: true,
            pos: DVec3::ZERO,
            yaw: 0.0,
            from_ankle: DVec3::ZERO,
            t0: 0.0,
            from_yaw: 0.0,
            from_pitch: 0.0,
            from_floor: 0.0,
            to: DVec3::ZERO,
            to_yaw: 0.0,
            to_floor: 0.0,
            to_sampled: DVec2::splat(f64::MAX),
            land_z: 0.0,
            land_wait: 0.0,
            t: 0.0,
            dur: 0.4,
            walk: false,
            lift: 0.05,
        }
    }
}

/// A person's animation state.
#[derive(Debug, Clone)]
pub struct Pose {
    rng: u32,
    init: bool,
    frame: u64,
    origin: DVec3,
    heading: f64,
    /// Gait cycle (0 … 1, the left heel strikes at 0) and whether the gait drives the feet.
    phase: f32,
    gait: bool,
    /// Walking weight for the upper body (0 … 1), smoothed ground speed (m/s), forward
    /// acceleration (m/s²) and turning rate (deg/s).
    walk: f32,
    speed: f32,
    accel: f32,
    turn: f32,
    vel: DVec2,
    feet: [Foot; 2],
    /// Standing offsets of the feet (a little different each time somebody settles).
    fidget: [Vec2; 2],
    fidget_t: f32,
    /// 0 standing … 1 seated, and the seat.
    sit: f32,
    seat: Vec3,
    seat_pelvis: Option<Vec3>,
    reach: f32,
    reach_at: Vec3,
    /// Both hands on a steering wheel: how far (0 … 1) and where.
    grip: f32,
    grip_at: [Vec3; 2],
    /// How the gripping hands lie (see `PoseInput::grip_frames`).
    grip_frame: Option<[(Vec3, Vec3); 2]>,
    /// Extra lean towards the wheel (degrees, eased).
    grip_extra: f32,
    hold: f32,
    /// Head direction (degrees: right, up) and where a glance goes and for how long.
    head: Vec2,
    glance: Vec2,
    glance_t: f32,
    /// Weight shift: where the pelvis leans (m) and until when.
    shift: f32,
    shift_to: f32,
    shift_t: f32,
    breath: f32,
    /// Balance against the floor's acceleration: lean (m/s² equivalent) and its rate.
    lean: Vec2,
    lean_v: Vec2,
    /// Pelvis height offset kept from the last frame (rises are smoothed).
    drop: f32,
    /// Floor under the body relative to the origin (smoothed), from the planted feet.
    body_floor: f32,
    /// Per-person style: arm hang, head tilt, stance width.
    style: [f32; 4],
    /// How the person stands, waiting (see `Idle`): the way before and the way now, how far
    /// the change between them has come (0 … 1) and how long the way now is kept.
    /// The person's own pace: steps a little quicker and shorter, or slower and longer
    /// (1 the type's own), so that no two walk alike.
    pace: f32,
    idle_from: Idle,
    idle_to: Idle,
    idle_mix: f32,
    idle_t: f32,
    /// Steps taken to catch up with a foot left behind, and frames a foot had to be pulled
    /// in (for the tests).
    catch_ups: u32,
    shuffles: u32,
    /// A foot was put down in the last [`Pose::advance`] - one footstep sound.
    landed: bool,
}

/// Ways of standing while waiting, one a person keeps for a while and then changes: no two
/// people at a stop stand alike, and nobody stands the same way all the time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Idle {
    /// The arms hanging.
    Hang,
    /// The hands clasped in front.
    Clasped,
    /// The hands behind the back.
    Behind,
    /// The arms crossed over the chest.
    Crossed,
    /// One hand on the hip.
    HandOnHip,
    /// Looking at a phone held in the right hand.
    Phone,
}

impl Idle {
    /// A way of standing for the random number `r` (0 … 1), as often as people stand so.
    fn pick(r: f32) -> Idle {
        match r {
            r if r < 0.34 => Idle::Hang,
            r if r < 0.52 => Idle::Clasped,
            r if r < 0.64 => Idle::Behind,
            r if r < 0.74 => Idle::Crossed,
            r if r < 0.88 => Idle::HandOnHip,
            _ => Idle::Phone,
        }
    }
}

/// Bone transforms and the joint positions they put the limbs at (model frame).
#[derive(Debug, Clone)]
pub struct Posed {
    pub bones: [Affine3A; SLOTS],
    pub hip: [Vec3; 2],
    pub knee: [Vec3; 2],
    pub ankle: [Vec3; 2],
    pub elbow: [Vec3; 2],
    pub wrist: [Vec3; 2],
    /// Lowest point of each sole (heel, ball or toe).
    pub sole: [f32; 2],
    /// Knee flexion and ankle flexion (degrees, toes up positive) per side.
    pub knee_flex: [f32; 2],
    pub ankle_flex: [f32; 2],
    /// How far each ankle stayed from where it should be (unreachable targets).
    pub leg_miss: [f32; 2],
    pub neck: Vec3,
    /// All transforms are finite (else `bones` is the rest pose and the caller should keep
    /// the last good mesh).
    pub ok: bool,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Move `x` towards `to` by at most `step`.
fn approach(x: f32, to: f32, step: f32) -> f32 {
    if (to - x).abs() <= step {
        to
    } else {
        x + step * (to - x).signum()
    }
}

/// Exponential smoothing factor for time constant `tau`.
fn ease_k(dt: f32, tau: f32) -> f32 {
    1.0 - (-dt / tau.max(1e-3)).exp()
}

fn angle_diff(a: f64, b: f64) -> f64 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

/// Rotation about z turning +y towards +x by `deg` (OMSI's heading sense).
fn yaw_quat(deg: f32) -> Quat {
    Quat::from_rotation_z(-deg.to_radians())
}

/// A frame from a bone axis and a second direction (made perpendicular to it).
fn basis(axis: Vec3, front: Vec3) -> Mat3A {
    let a = axis.normalize_or(Vec3::Z);
    let mut f = front - a * a.dot(front);
    if f.length_squared() < 1e-8 {
        f = a.any_orthonormal_vector();
    }
    let f = f.normalize();
    Mat3A::from_cols(a.into(), f.into(), a.cross(f).into())
}

/// The rotation taking the rest frame (axis0, front0) onto (axis1, front1).
fn bone_rot(axis0: Vec3, front0: Vec3, axis1: Vec3, front1: Vec3) -> Mat3A {
    basis(axis1, front1) * basis(axis0, front0).transpose()
}

/// A rigid bone transform: rotation `r` about the rest joint `rest`, which moves to `at`.
fn joint_xf(rest: Vec3, at: Vec3, r: Mat3A) -> Affine3A {
    Affine3A::from_mat3_translation(r.into(), at - Vec3::from(r * Vec3A::from(rest)))
}

fn about(pivot: Vec3, q: Quat) -> Affine3A {
    Affine3A::from_translation(pivot) * Affine3A::from_quat(q) * Affine3A::from_translation(-pivot)
}

/// Two-bone IK: from `root` with bone lengths `l1`, `l2` to `target`, bending towards
/// `pole`. Returns the middle joint, the end joint actually reached, and the bend-plane
/// normal (the hinge).
fn two_bone(root: Vec3, l1: f32, l2: f32, target: Vec3, pole: Vec3) -> (Vec3, Vec3, Vec3) {
    let d = target - root;
    let len = d.length();
    let dir = if len > 1e-5 { d / len } else { -Vec3::Z };
    let reach = (l1 + l2) * 0.9995;
    // (a bone next to nothing puts the floor over the reach: clamp would panic)
    let dist = len.clamp(((l1 - l2).abs() + 1e-3).min(reach), reach);
    let mut n = dir.cross(pole);
    if n.length_squared() < 1e-8 {
        n = dir.cross(Vec3::Y);
        if n.length_squared() < 1e-8 {
            n = Vec3::X;
        }
    }
    let n = n.normalize();
    let cos1 = ((l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)).clamp(-1.0, 1.0);
    let a1 = cos1.acos();
    let mid = root + Quat::from_axis_angle(n, a1) * dir * l1;
    let end = root + dir * dist;
    (mid, end, n)
}

/// How far (m) the hips would have to sink to reach a planted foot before it steps up.
const CATCH_UP: f32 = 0.13;

/// The most the hips sink to reach a planted foot (m).
const MAX_DROP: f32 = 0.12;

/// A planted foot this far (m) above or below the floor the body stands on is on another
/// floor and steps over first.
const STRANDED: f32 = 0.5;

/// Share of a gait cycle a foot is on the floor.
fn stance_fraction(speed: f32) -> f32 {
    let walk = (0.64 - 0.05 * (speed - 0.8)).clamp(0.57, 0.7);
    // running: the foot is down for well under half the stride, with a flight between
    walk + (0.38 - walk) * run_factor(speed)
}

/// How much of a run the gait is (0 walking … 1 running), by the speed (m/s).
pub fn run_factor(speed: f32) -> f32 {
    smoothstep(2.0, 3.4, speed)
}

/// Foot pitch while planted during the walk (degrees, toes up positive): the heel strikes
/// with the toes up and rolls down, the foot lies flat, then the heel rises until the toes
/// push off.
fn stance_pitch(u: f32, beta: f32, walk: f32) -> f32 {
    let heel = heel_pitch(walk) * (1.0 - smoothstep(0.0, 0.11, u));
    heel + toe_pitch(u, beta, walk)
}

/// Foot pitch at heel strike (degrees, toes up).
fn heel_pitch(walk: f32) -> f32 {
    16.0 * walk.clamp(0.3, 1.0)
}

fn toe_pitch(u: f32, beta: f32, walk: f32) -> f32 {
    let from = beta - 0.28;
    let t = ((u - from) / (beta - from)).clamp(0.0, 1.0);
    -48.0 * walk.clamp(0.25, 1.0) * t * t
}

/// Limit a rotation to `max` radians.
fn limit_quat(q: Quat, max: f32) -> Quat {
    let q = if q.w < 0.0 { -q } else { q };
    let (axis, angle) = q.to_axis_angle();
    if angle.abs() > max && axis.is_finite() {
        Quat::from_axis_angle(axis, max * angle.signum())
    } else {
        q
    }
}
