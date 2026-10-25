//! Rigid-body vehicle dynamics after Omsi.exe's own (0x7e2574; OMSI uses ODE only for the
//! objects a crash knocks over): a six-degree-of-freedom body with mass,
//! `[momentofintertia]` and `[schwerpunkt]`, wheels on spring/damper suspensions
//! (`achse_feder`, `achse_daempfer`, `achse_maxforce`, measured where the tyres stand
//! between `achse_minwidth` and `achse_maxwidth`, pushing from `achse_maxwidth`), every axle
//! steered towards the turning centre on `[rot_pnt_long]` with `[inv_min_turnradius]` as the
//! full lock, OMSI's grip across the tyres (the bus follows its wheels until the bend asks
//! more than the road's grip, then slides at mu x load), drive torque and brake forces from
//! the scripts, pitch and roll damped as OMSI damps them, gravity, ground contact per wheel
//! and impulse responses against obstacles. `examples/handling.rs` measures what a `.bus`
//! file makes of a bus.
//!
//! Ride height. A `.bus` file has no height for its axles: OMSI hangs each wheel so that
//! its spring is *unloaded* when the tyre stands on the model's ground plane (z = 0), and
//! the body then sinks by what the load compresses the springs - mass share over
//! `achse_feder` times the scripts' `Axle_Springfactor`. That sag is the `ai_deltaheight`
//! the same files give for their AI copies (-0.10 m for the SD202 and the NL202, whose
//! springs take 22-28 kN at 240-280 kN/m; -0.15 m for the F90 lorry, 55 kN at 370 kN/m),
//! and it is what `Axle_Suspension` reports: the model lifts the wheel into its arch by the
//! compression. Every vehicle therefore sits at its own height, set by its own data - the
//! NL202's level control even pumps its bellows until the compression reads 0.105 m.

use crate::collision::Obb;
use glam::{DVec2, DVec3, Quat, Vec2, Vec3};
use omsi_vehicle::Vehicle;

mod body;
pub mod realism;
mod tyre;
#[cfg(test)]
mod tests;

use tyre::{track_walls, tread_step, tyre_line};

/// How far a wheel may hang below its unloaded position (m).
pub const DROOP: f32 = 0.15;
/// Compression at which the bump stop takes over (m). A deflated air suspension rests on it.
pub const BUMP: f32 = 0.24;
/// The wheel between the road and the strut: its share of the corner's mass (axle, hub,
/// brake, tyre - about an eighth on a bus), and the tyre's vertical stiffness (N/m) and
/// damping (N s/m). Taken as massless, the wheel stood wherever the road put it: a bump
/// shoved it into the arch within a substep, the strut's damper kicked the body into the
/// air, and the wheel then dropped to its full droop at once - the bus hopped over a
/// manhole cover with its wheels flicking in and out of the body.
const UNSPRUNG: f32 = 0.12;
const TYRE_K: f32 = 900_000.0;
const TYRE_C: f32 = 3_000.0;
/// The frame OMSI's per-frame damping of the body's pitch and roll is measured in (see
/// `RigidBus::step`): a thirtieth of a second, the rate its options.cfg caps OMSI at.
const OMSI_FRAME: f32 = 1.0 / 30.0;

/// The suspension as Omsi.exe has it (see `step_slice`); `OMSI_TYRE_SUSPENSION=1` gives
/// the old one with a wheel mass, a tyre and bump stops (A/B).
fn omsi_suspension() -> bool {
    !omsi_cfg::flags::OMSI_TYRE_SUSPENSION.is_set()
}

/// What is left of the body's pitch and roll rate after `dt` seconds of OMSI's damping.
fn body_damping(body_freq: f32, dt: f32) -> f32 {
    let x0 = 1.5 * body_freq * OMSI_FRAME;
    if x0 <= 1e-6 || dt <= 0.0 {
        return 1.0;
    }
    (x0.sin() / x0).clamp(0.1, 1.0).powf(dt / OMSI_FRAME)
}

/// OMSI has no tyre between the road and the spring: `achse_feder` alone carries the body
/// (Omsi.exe 0x7e4960..0x7e4c40, force = feder x penetration - daempfer x speed). The
/// tyre here is at least this many times as stiff as the spring, so that the two in series
/// give the body 94 % of the file's spring rate instead of the 79 % a fixed 900 kN/m tyre
/// left a bus with 240 kN/m springs - that softness was a good part of its sway.
const TYRE_OVER_SPRING: f32 = 15.0;
/// Tyre samples along the envelope's length (see [`tyre_line`]).
const TYRE_SAMPLES: usize = 13;
/// Samples taken: the lattice they stand on covers the envelope with one more to spare, and
/// two more beyond each end tell the edges near the ends from the grade.
const LATTICE: usize = TYRE_SAMPLES + 5;
/// How far the tyre reaches for the ground ahead and behind, in radii. A pneumatic tyre
/// wraps round an edge and climbs it over about twice the length a rigid disc would: a
/// 15 cm kerb then costs about half the wheel load in push, not all of it, and a lowered
/// kerb of 3 cm is a nudge instead of the wall a rigid disc makes of it.
const ENVELOPE: f32 = 2.0;
/// How far (m) the rise between two samples may depart from the rises beside it before it
/// is an edge rather than the road's own grade or curve.
const EDGE: f64 = 0.01;
/// Halvings that place an edge between two samples (to a 64th of their spacing) and a face
/// the tyre cannot climb (to a 128th of the bracket).
const EDGE_HALVINGS: usize = 6;
const FACE_HALVINGS: usize = 7;
/// The highest step a tyre climbs, in radii over the ground it stands on; anything higher
/// is a wall. A step of the full radius would only give way to an infinite push (and the
/// hub and 5 cm, climbed before, took a bus over a 0.52 m step from 10 km/h). Up to 0.8 the
/// steepest slope the envelope meets (2.4) is still one the push takes in full (see
/// `step`), so a climb costs exactly the lift; and it keeps the stock maps' 30 cm gaps
/// between a side road and its junction something a bus drives over.
pub const CLIMB: f32 = 0.8;
/// Half the width of a tyre, in radii (a 275 mm bus tyre on a 0.47 m radius).
const HALF_WIDTH: f32 = 0.3;
/// How far beside the tyre (in half widths from the hub) a face is looked for.
const SIDE_REACH: f32 = 2.0;
/// How far (m) either side of a face's first point its direction is measured.
const FACE_SPAN: f32 = 0.2;
/// A face this far above the axle next to the tyre is a wall, not something far overhead.
const WALL_HEIGHT: f64 = 1.5;
/// Stiffness (N/m) and damping (N s/m) of a tyre pressed into a wall it cannot climb.
const WALL_K: f32 = 1.5e6;
const WALL_C: f32 = 6.0e4;
/// Share of the closing speed an obstacle gives back, and the sliding friction along it.
pub(crate) const RESTITUTION: f32 = 0.2;
const SCRAPE_FRICTION: f32 = 0.4;
/// Below this closing speed (m/s) a touch is a push, not a crash.
pub const CRASH_SPEED: f32 = 0.4;

/// The ground a wheel finds at one point: the highest face at or below the probe's top and
/// the lowest face above it (world heights).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GroundProbe {
    pub below: Option<f64>,
    pub above: Option<f64>,
}

/// What the wheels of a vehicle stand on.
pub trait Ground: Send + Sync {
    /// The faces at world (x, y): the highest at or below `top` and the lowest above it.
    fn probe(&self, x: f64, y: f64, top: f64) -> GroundProbe;

    /// A prober for one step of one vehicle - many points close together. It may keep what
    /// it looked up (the tiles under the vehicle) until it is dropped.
    fn session(&self) -> Box<dyn Fn(f64, f64, f64) -> GroundProbe + '_> {
        Box::new(move |x, y, top| self.probe(x, y, top))
    }
}

impl<F: Fn(f64, f64, f64) -> GroundProbe + Send + Sync> Ground for F {
    fn probe(&self, x: f64, y: f64, top: f64) -> GroundProbe {
        self(x, y, top)
    }
}

/// A face a tyre cannot climb, kept while the tyre is up against it: the samples that found
/// it are lost as soon as the hub is over it, and a wheel pushed that far went on through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallFace {
    /// A point of the face (world, ground plane) and its normal, towards the tyre.
    pub point: DVec2,
    pub normal: Vec2,
}

impl WallFace {
    /// How far the hub stands off the face (negative once it is past it).
    fn distance(&self, hub: DVec3) -> f32 {
        (hub.truncate() - self.point).dot(self.normal.as_dvec2()) as f32
    }

    /// How far a tyre of radius `r` rolling along `fwd_h` reaches from its hub towards the
    /// face: its radius straight ahead, half its width to the side.
    fn reach(&self, fwd_h: Vec3, r: f32) -> f32 {
        let n = self.normal.extend(0.0);
        let right_h = Vec3::new(fwd_h.y, -fwd_h.x, 0.0);
        r * n.dot(fwd_h).abs() + HALF_WIDTH * r * n.dot(right_h).abs()
    }
}

#[derive(Debug, Clone)]
pub struct RigidWheel {
    /// The wheel's own turning (rad/s) and its state as OMSI keeps it:
    /// gripping (turning with the ground), slipping (spinning or skidding: the tyre passes on
    /// the sliding friction and the wheel turns by its own torques), locked (a brake that
    /// stopped it). `inertia_inv`: the axle's `achse_inertia_inv` (0.002 when not given).
    pub spin: f32,
    pub slipping: bool,
    pub locked: bool,
    pub inertia_inv: f32,
    /// Hub in the body frame (x right, y forward, z up, relative to the model origin) with
    /// the spring unloaded: the model's wheel centre when `Axle_Suspension` reads zero.
    pub attach: Vec3,
    /// How much farther out than the hub the strut's force acts (m, signed like `attach.x`).
    /// OMSI measures the spring's travel where the tyres stand - halfway between
    /// `achse_minwidth` and `achse_maxwidth`, (max + min) / 4 from the middle - but turns the
    /// body with the force at `achse_maxwidth` / 2 (Omsi.exe 0x7e47b3 and 0x7e4bad).
    pub lever: f32,
    pub radius: f32,
    pub driven: bool,
    pub steered: bool,
    /// The wheel's steering angle this step (rad, positive to the right): every axle points
    /// at the turning centre on the `[rot_pnt_long]` line, atan((long - rot_pnt_long) x
    /// curvature) (Omsi.exe 0x7e3060), so an axle behind that line steers the other way.
    pub steer: f32,
    /// The tyre's vertical stiffness and damping (N/m, N s/m).
    pub tyre_k: f32,
    pub tyre_c: f32,
    /// Spring rate (N/m), damper rate (N s/m) and the load the strut is built for (N).
    pub spring: f32,
    pub damper: f32,
    pub max_force: f32,
    /// `Axle_Springfactor_*`: air suspensions scale the spring with the bellows pressure.
    pub spring_factor: f32,
    /// The kind of strut (`[suspension]`); a coil spring unless the vehicle says otherwise.
    pub strut: Strut,
    /// Static load at rest (N), from the mass split over the axles.
    pub rest_load: f32,
    /// Current compression (m, positive = the wheel pushed up into the body) and its rate.
    pub compression: f32,
    pub compression_rate: f32,
    /// The compression at which the tyre would just touch the ground, last substep (None
    /// in the air or right after [`RigidBody::place`]).
    pub touch: Option<f32>,
    pub on_ground: bool,
    pub rotation_deg: f32,
    pub rpm: f32,
    /// Load on the tyre this step (N).
    pub load: f32,
    /// World height of the ground the tyre stands on this step (the surface under the hub
    /// or the edge it is climbing).
    pub ground_z: f64,
    /// `ground_z` came from the ground (not yet after [`RigidBody::place`]): steps up to
    /// [`CLIMB`] radii over it are ground, higher ones walls.
    pub ground_seen: bool,
    /// Faces the tyre is up against.
    pub walls: Vec<WallFace>,
    /// The `[surface]` id of what the tyre rolls on (`Axle_SurfaceID_`), the factor that
    /// surface puts on the road's weather grip, and how much the road's unevenness lifts the
    /// ground under the tyre at this moment (m; see [`realism::roughness`]).
    pub surface: u8,
    /// The surface in force, the one before it and how far (0..1, a metre) the road has
    /// changed from the one to the other.
    pub surface_now: u8,
    pub surface_was: u8,
    pub surface_mix: f32,
    pub mu_scale: f32,
    pub rough: f32,
    /// Horizontal push (N) of a step the tyre climbs or a wall it stands against.
    pub step_force: f32,
    /// How far the contact patch lags behind the wheel across (m, positive to the right): the
    /// tyre's lateral deflection, which the slip-angle model turns into a side force.
    pub lat_defl: f32,
}

fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// The kind of strut of a wheel and what it remembers (see `omsi_strut`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strut {
    pub kind: omsi_vehicle::vehicle::SuspensionKind,
    /// Dry friction of leaf springs (N).
    pub friction: f32,
    /// The extra stiffness at 10 cm of compression, as a fraction of the spring's rate.
    pub progression: f32,
    /// The levelling valve's time constant (s).
    pub level_time: f32,
    /// What the levelling valve has added to the bellows' force (N).
    pub preload: f32,
}

impl Default for Strut {
    fn default() -> Strut {
        Strut { kind: omsi_vehicle::vehicle::SuspensionKind::Coil, friction: 0.0, progression: 0.0, level_time: 4.0, preload: 0.0 }
    }
}

impl Strut {
    pub fn from_block(s: Option<omsi_vehicle::vehicle::Suspension>) -> Strut {
        match s {
            Some(s) => Strut { kind: s.kind, friction: s.friction, progression: s.progression, level_time: s.level_time, preload: 0.0 },
            None => Strut::default(),
        }
    }

    /// The spring's force at `t` metres of compression, rate `k` N/m. A coil spring is linear;
    /// the other kinds stiffen by `progression` per 10 cm.
    pub fn spring(&self, k: f32, t: f32) -> f32 {
        use omsi_vehicle::vehicle::SuspensionKind::Coil;
        if self.kind == Coil {
            return k * t;
        }
        k * t * (1.0 + self.progression * (t / 0.10).max(0.0)) + self.preload
    }

    /// The damping force at an upward speed `v_up` (m/s): the damper, and for leaf springs
    /// the friction of the leaves, which is there at any speed and fades to nothing at rest.
    pub fn damping(&self, damper: f32, v_up: f32) -> f32 {
        use omsi_vehicle::vehicle::SuspensionKind::Leaf;
        let mut f = damper * v_up;
        if self.kind == Leaf {
            f += self.friction * (v_up / 0.03).tanh();
        }
        f
    }

    /// The levelling valve of air bellows: it lets air in or out until the body stands where
    /// the empty bus stood (`design` m of compression), at a pace of one rate-times-error per
    /// `level_time` seconds, so that bumps pass it by and a change of load does not.
    pub fn level(&mut self, k: f32, t: f32, design: f32, dt: f32, load: f32) {
        if self.kind != omsi_vehicle::vehicle::SuspensionKind::Air {
            return;
        }
        self.preload += k * (t - design) / self.level_time.max(0.1) * dt;
        // (the bellows cannot hold more than a fraction more than the load, nor suck)
        self.preload = self.preload.clamp(-0.6 * load, 1.5 * load);
    }
}

impl RigidWheel {
    /// One substep of the wheel's turning:
    /// a gripping wheel turns with the ground and starts to slip when what it has to pass
    /// on (`f_long`, before the friction circle cut it) is more than the road takes; a
    /// slipping one turns by its drive, its brake and the sliding friction, locks when the
    /// brake stops it, and grips again when its speed crosses the ground's.
    fn advance_spin(&mut self, c: &Contact, f_long_wanted: f32, friction: f32, h: f32) {
        let r = self.radius.max(0.05);
        let ground = c.v_long / r;
        let mu_n = friction * c.grip;
        // (`OMSI_NO_WHEEL_SLIP=1`: every wheel grips, as before the wheels had a turning of
        // their own - for comparison)
        let no_slip = omsi_cfg::flags::OMSI_NO_WHEEL_SLIP.is_set();
        if no_slip {
            self.slipping = false;
            self.locked = false;
        }
        if !self.slipping {
            let wanted = if c.v_long.abs() > STANDING { c.drive - c.brake * c.v_long.signum() } else { f_long_wanted };
            if wanted.abs() > mu_n * 1.02 && mu_n > 0.0 && !no_slip {
                self.slipping = true;
                self.locked = false;
                self.spin = ground;
            } else {
                self.spin = ground;
            }
        }
        if self.slipping {
            let slide_before = self.spin * r - c.v_long;
            if self.locked {
                // it turns again when the road's pull and the drive beat the brake
                if (sign(-c.v_long) * mu_n - c.drive).abs() > c.brake {
                    self.locked = false;
                } else {
                    self.spin = 0.0;
                }
            }
            if !self.locked {
                let before = self.spin;
                self.spin += h * self.inertia_inv * (c.drive - sign(self.spin) * c.brake - sign(slide_before) * mu_n);
                if before != 0.0 && sign(before) != sign(self.spin) && c.brake > 0.0 {
                    self.spin = 0.0;
                    self.locked = true;
                }
            }
            let slide_after = self.spin * r - c.v_long;
            // gripping again: the wheel's speed crossed the ground's (or both stand)
            // (a slide that only now begins - zero before - has crossed nothing)
            if sign(slide_after) * sign(slide_before) < 0.0 || (c.v_long.abs() < STANDING && self.spin.abs() * r < STANDING && c.drive.abs() <= c.brake.max(mu_n)) {
                self.slipping = false;
                self.locked = false;
                self.spin = ground;
            }
        }
        self.rpm = self.spin * 60.0 / std::f32::consts::TAU;
        self.rotation_deg = (self.rotation_deg + self.spin.to_degrees() * h).rem_euclid(360.0);
    }

    /// Compression the static load gives with the current spring factor.
    pub fn rest_compression(&self) -> f32 {
        self.rest_load / (self.spring * self.spring_factor.max(0.05))
    }
}

/// The static load on each wheel of each axle (N): the weight shared so that the axle loads
/// balance the centre of gravity (a least-squares split when there are more than two axles).
pub fn wheel_rest_loads(def: &Vehicle) -> Vec<f32> {
    let mass = if def.mass < 100.0 { def.mass * 1000.0 } else { def.mass }.max(500.0);
    let cog_y = def.cog.map(|c| c[1]).unwrap_or(0.0);
    let g = 9.81;
    let n = (def.axles.len() * 2).max(1) as f32;
    let sy: f32 = def.axles.iter().map(|a| 2.0 * a.long).sum();
    let syy: f32 = def.axles.iter().map(|a| 2.0 * a.long * a.long).sum();
    let det = n * syy - sy * sy;
    let (a, b) = if det.abs() > 1e-6 {
        let rhs1 = mass * g;
        let rhs2 = mass * g * cog_y;
        ((rhs1 * syy - sy * rhs2) / det, (n * rhs2 - sy * rhs1) / det)
    } else {
        (mass * g / n, 0.0)
    };
    def.axles.iter().map(|x| (a + b * x.long).max(mass * g / n * 0.2)).collect()
}

/// A wheel on the ground in one substep, before its tyre forces are settled.
struct Contact {
    /// Contact point relative to the centre of gravity, tyre forward and right (world).
    r: Vec3,
    fwd: Vec3,
    right: Vec3,
    /// Suspension, step and wall forces (world).
    base: Vec3,
    drive: f32,
    /// What the brake (and the rolling resistance) can hold.
    brake: f32,
    grip: f32,
    /// Friction coefficient of this tyre on this road.
    mu: f32,
    v_long: f32,
    /// Takes side forces (a coupled part's joint does not).
    lateral: bool,
    /// The wheel it is (None: a coupled part's joint).
    wheel: Option<usize>,
}

/// `achse_inertia_inv` where a `[newachse]` does not say (OMSI LoadFromFile: 0.002).
pub const DEFAULT_INERTIA_INV: f32 = 0.002;

/// Below this speed (m/s) at the contact a tyre stands and its brake holds statically.
const STANDING: f32 = 0.15;

/// One hit of the body against an obstacle.
#[derive(Debug, Clone, Copy)]
pub struct Impact {
    /// Where it hit, in the body frame relative to the model origin (x right, y forward, z up).
    pub point: Vec3,
    /// Closing speed along the contact normal (m/s).
    pub speed: f32,
    /// Kinetic energy the impact took away (J).
    pub energy: f32,
    /// Index into the obstacle list the body was tested against; for a wheel stopped by a
    /// face (`RigidBody::wheel_impacts`), the wheel's.
    pub obstacle: usize,
    /// A `[crashmode_pole]` obstacle that broke off.
    pub broke: bool,
    /// Direction the obstacle was pushed (world, ground plane).
    pub push: Vec3,
}

/// A part towed behind the body (the rear section of an articulated bus). The parts follow
/// kinematically, so what acts on them along their length arrives at the joint: the drive
/// of a driven axle - the O530G/GL Facelift and the stock GN92 are pushers, whose front
/// sections roll free -, the brakes and rolling resistance, the weight on a grade and the
/// mass. The body takes the part of it along its own axis (cosine of the articulation
/// angle); the side part is what a pusher's joint damping and the rear section's tyres
/// hold. Handed on whole, it turned a pusher standing against its brakes round on the spot
/// and drove every bend into a jack-knife: the kinematic rear section has no tyres of its
/// own to resist it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoupledPart {
    /// The coupling on this body (body frame, relative to the model origin).
    pub point: Vec3,
    /// The direction of travel of the part hung on that coupling (world).
    pub dir: Vec3,
    /// Mass (kg).
    pub mass: f32,
    /// Driven wheels, their radius (m) and the static load on them (N): they pass on no
    /// more than friction times that load.
    pub driven_wheels: usize,
    pub radius: f32,
    pub driven_load: f32,
    /// What its brakes and its rolling resistance hold (N), and the static load on all its
    /// wheels (N).
    pub brake: f32,
    pub load: f32,
    /// The weight it hangs on the coupling (N), pressing the body down there.
    pub joint_load: f32,
}

#[derive(Debug, Clone)]
pub struct RigidBody {
    pub mass: f32,
    /// Body-frame inertia (diagonal, kg m²).
    pub inertia: Vec3,
    /// Centre of gravity relative to the model origin (m).
    pub cog: Vec3,
    /// World position of the centre of gravity.
    pub position: DVec3,
    pub orientation: Quat,
    pub velocity: Vec3,
    /// Angular velocity in the body frame (rad/s).
    pub omega: Vec3,
    pub wheels: Vec<RigidWheel>,
    /// Axle index of each wheel (two per axle, left then right).
    pub wheel_axle: Vec<usize>,
    pub steer_deg: f32,
    /// The curvature the steering asks for this step (1/m, positive to the right).
    pub kappa: f32,
    pub max_steer_deg: f32,
    /// `[rot_pnt_long]` and `[inv_min_turnradius]`: the line the bus turns about and the
    /// curvature of the full lock.
    pub rot_pnt_long: f32,
    pub inv_min_turn_radius: f32,
    /// sqrt(sum of the springs / mass) (1/s): OMSI damps the body's pitch and roll by
    /// sin(x)/x a frame with x = 1.5 of it times the frame (Omsi.exe 0x7e4f55).
    pub body_freq: f32,
    /// OMSI's two states of the tyres across (Omsi.exe `+0x1d8`): holding - the bus follows
    /// its wheels however quickly they are turned, as long as the bend asks no more than
    /// the grip of all its tyres and no wheel spins or locks - or sliding, each tyre giving
    /// at most its own grip until the sideways speed at every tyre is under 0.1 m/s again.
    pub holding: bool,
    pub rolling_resistance: f32,
    /// Body-frame acceleration of the last step (m/s²), for `A_Trans_*`.
    pub accel_body: Vec3,
    pub friction: f32,
    /// Wheels a face stopped during the last step, one entry per wheel.
    pub wheel_impacts: Vec<Impact>,
    /// The parts coupled behind (see [`CoupledPart`]); `drive_torque` is shared by their
    /// driven wheels and the body's own.
    pub coupled: Vec<CoupledPart>,
    /// Obstacles (by id) the body was put down inside: left alone until it is out of them
    /// (a bus spawned in a shelter's or a depot hall's box was held there for good - every
    /// move towards the nearest side pushed it back in). `None` until the first collision
    /// check after `place`.
    pub spawned_inside: Option<Vec<i64>>,
    /// The faces a wheel cannot climb stop it (a platform's edge, a kerb too high for it).
    /// Off with the collisions with objects: the wheels then keep to the ground under them,
    /// as OMSI's do - its wheels know no faces - and a step nobody sees (a height profile
    /// over the drawn road, a helper object's collision box) is no invisible wall.
    pub wheel_walls: bool,
    /// How the air meets the vehicle (see [`realism::Aero`]), its density (kg/m³) and the
    /// wind (world, m/s).
    pub aero: realism::Aero,
    pub air_density: f32,
    pub wind: Vec3,
    /// How wet the road is, 0 dry .. 1 wet .. 2 under snow (the host's `street_cond`).
    pub wetness: f32,
    /// Distance driven along the road (m, signed): where on the road's unevenness the
    /// body's origin is. An axle at `y` metres from the origin is at `road_s + y`.
    pub road_s: f64,
    /// The road model of the "Realistic road physics" setting: slip-angle tyres, grip and
    /// roughness by surface, wind. Off, the body is OMSI 2's (follows its wheels until the grip
    /// is passed, one grip, no unevenness, its old air drag).
    pub realistic: bool,
}

static REALISTIC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether new bodies take the realistic road model (the setting; `OMSI_REALISTIC_ROAD=1` too).
pub fn set_realistic_default(on: bool) {
    REALISTIC.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn realistic_default() -> bool {
    REALISTIC.load(std::sync::atomic::Ordering::Relaxed) || omsi_cfg::flags::OMSI_REALISTIC_ROAD.is_set()
}
