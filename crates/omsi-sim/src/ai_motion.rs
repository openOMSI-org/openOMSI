//! How the body of an AI vehicle follows the way its planner lays out
//! (`traffic::AiState::way_point`): a road vehicle is a bicycle model steered by pure
//! pursuit and sitting on its springs, a rail vehicle rides on its two outer axles, an
//! aircraft flies along its path and banks into its turns.
//!
//! Placing the vehicle *on* the lane every frame, turned to the lane's heading, is what made
//! the traffic look wrong: the heading jumped at every lane sample and joint (the steering
//! was worked out from those jumps), the body turned about its middle so that the rear
//! wheels slid sideways through every bend, and a lane change was a sideways glide with a
//! made-up yaw. Here the heading only ever changes by speed × tan(steering) / wheelbase,
//! the rear axle never moves sideways, and the steering itself is rate limited.

use crate::vehicle::VehicleInstance;
use glam::{DVec2, DVec3, Vec3};
use omsi_vehicle::Vehicle;
use std::f32::consts::FRAC_PI_2;

/// What kind of way the vehicle follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    Road,
    Rail,
    Air,
}

/// The line the vehicle turns about (`[rot_pnt_long]`, normally the rear axle) and the
/// distance from it to the front axle: (rotation point, wheelbase), both in metres along
/// the body.
pub fn rotation_point(def: &Vehicle) -> (f32, f32) {
    let front = def.axles.iter().map(|a| a.long).fold(f32::MIN, f32::max);
    let rear = def.axles.iter().map(|a| a.long).fold(f32::MAX, f32::min);
    if def.axles.is_empty() {
        return (-1.3, 2.6);
    }
    // a rotation point at (or in front of) the front axle is a file without one
    let rot = if def.axles.len() >= 2 && def.rot_pnt_long < front - 0.5 { def.rot_pnt_long } else { rear };
    (rot, (front - rot).max(0.8))
}

/// Clearance a vehicle pulling out keeps from the corner of what it goes round (m).
pub const PULL_OUT_CLEARANCE: f64 = 0.25;
/// Seconds a driver standing behind something turns the wheels before moving off round it
/// (the shortest reaction time of the AI drivers).
pub const PULL_OUT_WAIT: f32 = 0.4;
/// Acceleration (m/s²) of a driver edging out round something standing close ahead, until
/// the front is past its corner: pulling away briskly, a car covered too much ground while
/// the wheels were still turning and came within 0.2 m of the corner.
pub const PULL_OUT_ACCEL: f32 = 1.0;

/// Lengths (m) of the S-curve that takes a vehicle out round something standing `real`
/// metres ahead of its front bumper, gentlest first (`front`: origin to front bumper).
/// From a standstill a driver turns out steeply; rolling up to a parked car they move over
/// well before it.
pub fn pull_out_ramps(real: f32, front: f32, rolling: bool) -> Vec<f32> {
    let base = real + 0.5 * front;
    let (factors, max): (&[f32], f32) = if rolling { (&[1.0, 0.8, 0.6, 0.45], 20.0) } else { (&[1.5, 1.2, 1.0, 0.8, 0.6], 12.0) };
    let mut out: Vec<f32> = Vec::new();
    for f in factors {
        let r = (base * f).clamp(4.0, max);
        if !out.iter().any(|&o| (o - r).abs() < 0.3) {
            out.push(r);
        }
    }
    out
}

/// Sideways acceleration (m/s²) a driver allows moving back into the lane after passing.
pub const BACK_IN_LAT_ACCEL: f32 = 2.5;

/// Length (m) of the S-curve that takes a vehicle `offset` metres back into its lane at
/// `speed` (m/s) after passing: a smoothstep of length L over the offset D turns at most
/// 6·D·v²/L², so L = v·√(6·D/a) keeps it to `lat_accel` - about 2.8 s of travel for a lane
/// width. A fixed 8-14 m cut back 3.3 m in under a second at 40 km/h (4-5.6 m/s²).
pub fn back_in_ramp(offset: f32, speed: f32, lat_accel: f32) -> f32 {
    let geometric = (offset.abs() * 3.5).clamp(8.0, 14.0);
    let dynamic = speed.max(0.0) * (6.0 * offset.abs() / lat_accel.max(0.5)).sqrt();
    geometric.max(dynamic).min(60.0)
}

/// A straight way north from the origin that moves `side` metres to the left along an
/// S-curve (smoothstep) `ramp` metres long, as `AiState::way_point` lays out a pull-out.
pub fn straight_pull_out(side: f32, ramp: f32) -> impl Fn(f32) -> DVec3 {
    move |d: f32| {
        let t = (d / ramp.max(0.1)).clamp(0.0, 1.0);
        let lat = -side * t * t * (3.0 - 2.0 * t);
        DVec3::new(lat as f64, d as f64, 0.0)
    }
}

/// How far behind something standing in its lane a vehicle has to stop to get out round
/// it later from a standstill (m, from its front bumper to the other's body), with its own
/// wheelbase, steering lock and steering rate: the shortest gap from which one of the
/// `pull_out_ramps` takes it past the corner of a standing bus (`obstacle_half_width`,
/// in the middle of the lane, the oncoming lane `side` metres over) with
/// `PULL_OUT_CLEARANCE` to spare, and half a metre more for where it comes to rest.
/// `extent` is (origin to front bumper, to rear bumper, half width).
pub fn pull_out_room(def: &Vehicle, extent: (f32, f32, f32), obstacle_half_width: f64, side: f32) -> f32 {
    let (front, _, _) = extent;
    let bus_half_len = 6.0;
    let mut gap = 0.75f32;
    while gap <= 12.0 {
        let obstacle = crate::collision::Obb::vehicle(DVec2::new(0.0, (front + gap) as f64 + bus_half_len), 0.0, bus_half_len, bus_half_len, obstacle_half_width);
        for ramp in pull_out_ramps(gap, front, false) {
            let way = straight_pull_out(side, ramp);
            let mut body = AiBody::new(def, MotionKind::Road);
            body.place(&way, None, None, 0.0);
            if body.sweep_clearance(&way, 0.0, PULL_OUT_WAIT, PULL_OUT_ACCEL, 8.0, gap + 6.0, extent, &[obstacle]) >= PULL_OUT_CLEARANCE {
                return gap + 0.5;
            }
        }
        gap += 0.25;
    }
    12.5
}

/// Unit vector (east, north) of a heading in degrees.
fn dir(h: f64) -> DVec2 {
    let r = h.to_radians();
    DVec2::new(r.sin(), r.cos())
}

fn wrap(d: f64) -> f64 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

/// One wheel on its spring.
#[derive(Debug, Clone, Copy)]
struct Wheel {
    axle: usize,
    side: usize,
    /// Body frame: right and forward (m).
    lat: f32,
    long: f32,
    /// Spring (N/m) and damper (Ns/m) of this wheel.
    k: f32,
    c: f32,
}

/// How far over an AI car's wheel its ground is looked for (m): Omsi.exe casts the ray of its
/// ground query from 3 m over the point (0x7c40c8) and takes the highest face under that.
pub const AI_RAY_UP: f64 = 3.0;
/// A face this far under the wheel's lane is not this wheel's ground (m): the road under the
/// bridge the lane crosses, or the ground beyond the edge of the road, which the wheel hangs
/// over on its spring rather than drops onto (the lane stays its height there).
const AI_DROP: f64 = 0.3;

#[derive(Debug, Clone)]
pub struct AiBody {
    pub kind: MotionKind,
    rot_long: f32,
    wheelbase: f32,
    front_long: f32,
    rear_long: f32,
    /// Largest front wheel angle (deg) and how fast the driver turns towards it (deg/s).
    max_steer: f32,
    steer_rate: f32,
    wheels: Vec<Wheel>,
    axle_count: usize,
    mass: f32,
    cog: f32,
    /// Moments of inertia about the lateral (pitch) and longitudinal (roll) axes (kg m²).
    inertia: [f32; 2],
    /// Stiffness and damping of the body in heave (N/m), pitch and roll (Nm/rad).
    spring: [f32; 3],
    damper: [f32; 3],
    started: bool,
    /// World position of the rotation point (x east, y north).
    rear: DVec2,
    /// Heading (deg, clockwise from north).
    pub heading: f64,
    /// Front wheel angle of the bicycle model (deg, + = right), rate limited and smoothed.
    pub steer: f32,
    steer_cmd: f32,
    last_speed: f32,
    /// How fast the body's rotation point went along its heading in the last step (m/s):
    /// the planner's speed, less what held the body back, plus its catching up.
    pub moved: f32,
    /// How far ahead the driver looks along the way to steer, as a share of the usual
    /// (`drive`): 1, or what a corner caught on something is worked free with
    /// (`ai_traffic::scenery`).
    pub look_scale: f32,
    /// Longitudinal and lateral acceleration (m/s², forward / right positive).
    pub a_long: f32,
    pub a_lat: f32,
    yaw_rate: f32,
    /// The body on its springs: height of the origin, pitch and roll (rad), their rates.
    z: f64,
    vz: f32,
    pitch: f32,
    vpitch: f32,
    roll: f32,
    vroll: f32,
    /// The plane the wheels stand on last frame (height, pitch, roll) and its rates.
    ground: Option<(f64, f32, f32)>,
    ground_rate: [f32; 3],
    /// Pose for the vehicle: origin, pitch and bank (deg, + = nose up / right side down).
    pub position: DVec3,
    impact_offset: DVec2,
    impact_velocity: DVec2,
    pub pitch_deg: f32,
    pub bank_deg: f32,
    /// Per axle (left, right): how far the wheel hangs below its rest position against
    /// the body (m, the `Axle_Suspension_*` convention: negative = compressed).
    pub suspension: Vec<[f32; 2]>,
    /// Each wheel's contact height last frame (NaN until the first frame): where its next
    /// ground query starts from.
    contact_z: Vec<f64>,
}

impl AiBody {
    pub fn new(def: &Vehicle, kind: MotionKind) -> AiBody {
        let (rot_long, wheelbase) = rotation_point(def);
        let front_long = def.axles.iter().map(|a| a.long).fold(f32::MIN, f32::max);
        let rear_long = def.axles.iter().map(|a| a.long).fold(f32::MAX, f32::min);
        let (front_long, rear_long) = if def.axles.len() >= 2 { (front_long, rear_long) } else { (rot_long + wheelbase, rot_long) };
        // `[mass]` is in tonnes (a few files give kilograms), `[momentofintertia]` in the
        // same unit times m²: 1 t and 1.49 for a Golf, 10.9 t and 300 for an SD200
        let tonnes = def.mass < 100.0;
        let mass = (if tonnes { def.mass * 1000.0 } else { def.mass }).max(300.0);
        let unit = if tonnes { 1000.0 } else { 1.0 };
        let heavy = mass > 6000.0;
        // inv_min_turnradius = tan(max angle) / wheelbase
        let max_steer = if def.inv_min_turn_radius > 0.0 { (def.inv_min_turn_radius * wheelbase).atan().to_degrees().clamp(15.0, 55.0) } else { 35.0 };
        let mut wheels = Vec::new();
        for (i, a) in def.axles.iter().enumerate() {
            let half = (a.max_width * 0.5).max(0.4);
            for (side, lat) in [(0, -half), (1, half)] {
                // `achse_feder` / `achse_daempfer`: kN/m and kNs/m per side
                wheels.push(Wheel { axle: i, side, lat, long: a.long, k: a.spring.max(0.0) * 1000.0, c: a.damper.max(0.0) * 1000.0 });
            }
        }
        let n = wheels.len().max(1) as f32;
        if wheels.iter().any(|w| w.k <= 0.0) {
            // no springs in the file: 1.5 Hz with a third of critical damping
            let k_total = mass * (std::f32::consts::TAU * 1.5).powi(2);
            let c_total = 2.0 * 0.35 * (k_total * mass).sqrt();
            for w in wheels.iter_mut() {
                w.k = k_total / n;
                w.c = c_total / n;
            }
        }
        let mut spring = [0.0f32; 3];
        let mut damper = [0.0f32; 3];
        for w in &wheels {
            spring[0] += w.k;
            spring[1] += w.k * w.long * w.long;
            spring[2] += w.k * w.lat * w.lat;
            damper[0] += w.c;
            damper[1] += w.c * w.long * w.long;
            damper[2] += w.c * w.lat * w.lat;
        }
        let half_track = wheels.iter().map(|w| w.lat.abs()).fold(0.5, f32::max);
        // (pitch and roll: the first and the third value, as Omsi.exe reads them - see
        // `RigidBody::from_definition`)
        let mut inertia = [def.moment_of_inertia[0] * unit, def.moment_of_inertia[2] * unit];
        if inertia[0] <= 0.0 {
            inertia[0] = mass * (front_long * front_long + rear_long * rear_long) * 0.5;
        }
        if inertia[1] <= 0.0 {
            inertia[1] = mass * half_track * half_track * 1.2;
        }
        // keep every mode below ~6 Hz: the springs are integrated at a few hundred Hz at most
        for (i, j) in [(1usize, 0usize), (2, 1)] {
            let max_k = inertia[j] * 38.0f32.powi(2);
            if spring[i] > max_k {
                inertia[j] = spring[i] / 38.0f32.powi(2);
            }
        }
        // The heave as well: a tram's or a ship's `achse_feder` / `achse_daempfer` of 5000
        // (Mainz's GT6M, Schiff1) gave c/m of ~3300/s, the 5 ms steps of `settle` ran away
        // (vz inf, then NaN) and the car and its rear section went to NaN - which took the
        // player's bus with it on contact (#3161).
        spring[0] = spring[0].min(mass * 38.0f32.powi(2));
        // at least 0.8 of critical damping in every mode: a driven car settles after a
        // bump or a stop instead of swinging on (the files' dampers are tuned for the
        // player's physics, which damps through the tyres as well) - and at most twice
        // critical, which the steps still integrate
        let heave_c = 2.0 * (spring[0] * mass).sqrt();
        damper[0] = damper[0].clamp(0.8 * heave_c, 2.0 * heave_c);
        for (i, j) in [(1usize, 0usize), (2, 1)] {
            let crit = 2.0 * (spring[i] * inertia[j]).sqrt();
            damper[i] = damper[i].clamp(0.8 * crit, 2.0 * crit);
        }
        let axle_count = def.axles.len().max(1);
        AiBody {
            kind,
            rot_long,
            wheelbase,
            front_long,
            rear_long,
            max_steer,
            // (a bus driver turns his wheel briskly too: at 25 deg/s the AI buses came into every
            // junction turn late and ran over the kerb, half their turning time past the lane's edge)
            steer_rate: if heavy { 40.0 } else { 45.0 },
            wheels,
            axle_count,
            mass,
            cog: if def.cog_height > 0.0 { def.cog_height } else { 0.6 },
            inertia,
            spring,
            damper,
            started: false,
            rear: DVec2::ZERO,
            heading: 0.0,
            steer: 0.0,
            steer_cmd: 0.0,
            last_speed: 0.0,
            moved: 0.0,
            look_scale: 1.0,
            a_long: 0.0,
            a_lat: 0.0,
            yaw_rate: 0.0,
            z: 0.0,
            vz: 0.0,
            pitch: 0.0,
            vpitch: 0.0,
            roll: 0.0,
            vroll: 0.0,
            ground: None,
            ground_rate: [0.0; 3],
            position: DVec3::ZERO,
            impact_offset: DVec2::ZERO,
            impact_velocity: DVec2::ZERO,
            pitch_deg: 0.0,
            bank_deg: 0.0,
            suspension: vec![[0.0; 2]; axle_count],
            contact_z: Vec::new(),
        }
    }

    /// Where the line the body turns about is (world x east, y north) and how far it lies
    /// along the body from the vehicle's origin (m): the point of the body that follows the
    /// way's point `rotation_long()` metres along it.
    pub fn rotation_at(&self) -> DVec2 {
        self.rear
    }

    pub fn rotation_long(&self) -> f32 {
        self.rot_long
    }

    /// Put the body onto its way, standing still and straight. `way(d)` is the point `d`
    /// metres along the way from the vehicle's origin.
    pub fn place(&mut self, way: &dyn Fn(f32) -> DVec3, ground: Option<&dyn Fn(f64, f64) -> Option<f64>>, contact: Option<&dyn crate::rigid::Ground>, speed: f32) {
        self.started = false;
        self.ground = None;
        self.contact_z.clear();
        self.impact_offset = DVec2::ZERO;
        self.impact_velocity = DVec2::ZERO;
        self.last_speed = speed;
        self.step(0.0, speed, way, ground, contact);
    }

    /// Follow the way for `dt` seconds at `speed` (m/s, the planner's speed).
    /// `contact`: what the wheels stand on (the faces under a height, as the player's wheels
    /// ask it); without it the plain height sampler `ground`.
    pub fn step(&mut self, dt: f32, speed: f32, way: &dyn Fn(f32) -> DVec3, ground: Option<&dyn Fn(f64, f64) -> Option<f64>>, contact: Option<&dyn crate::rigid::Ground>) {
        if dt > 0.0 {
            let a = (speed - self.last_speed) / dt;
            self.a_long += (a - self.a_long) * (dt / 0.15).min(1.0);
        }
        self.last_speed = speed;
        match self.kind {
            MotionKind::Road => {
                self.drive(dt, speed, way);
                self.settle(dt, way, ground, contact);
            }
            MotionKind::Rail => self.ride(way),
            MotionKind::Air => self.fly(dt, speed, way),
        }
        self.settle_collision(dt);
    }

    /// Give a road vehicle a velocity impulse in world space. Its path-following driver
    /// recovers after the impact rather than teleporting the body back onto the lane.
    /// `closing_speed` is how fast the two came together along `direction`, `striker` the
    /// mass (kg) of the vehicle that hit this one, `mass` this one's: the change of speed
    /// is the impulse of the blow over this mass, (1 + e) m1 / (m1 + m2) v - the rigid
    /// model's restitution, given back only by a real blow (above 1 m/s).
    pub fn collision_impulse(
        &mut self,
        direction: DVec2,
        closing_speed: f32,
        striker: f32,
        mass: f32,
    ) {
        if self.kind != MotionKind::Road
            || !direction.is_finite()
            || !closing_speed.is_finite()
            || !striker.is_finite()
            || !mass.is_finite()
            || closing_speed <= 0.0
            || striker <= 0.0
            || mass <= 0.0
        {
            return;
        }
        let e = if closing_speed > 1.0 { crate::rigid::RESTITUTION } else { 0.0 };
        let recoil_speed = (1.0 + e) * striker / (striker + mass) * closing_speed;
        self.impact_velocity += direction.normalize_or_zero() * recoil_speed as f64;
        self.impact_velocity = self.impact_velocity.clamp_length_max(12.0);
    }

    fn settle_collision(&mut self, dt: f32) {
        if self.kind != MotionKind::Road || dt <= 0.0 {
            return;
        }
        let steps = (dt / 0.015).ceil().clamp(1.0, 20.0) as usize;
        let step = (dt / steps as f32).min(0.25) as f64;
        for _ in 0..steps {
            // A damped suspension-like response returns the displaced car to its planned
            // path without cancelling its initial backwards or sideways impulse.
            let acceleration = -self.impact_offset * 7.0 - self.impact_velocity * 5.0;
            self.impact_velocity += acceleration * step;
            self.impact_offset += self.impact_velocity * step;
        }
        self.impact_offset = self.impact_offset.clamp_length_max(4.0);
        self.impact_velocity = self.impact_velocity.clamp_length_max(12.0);
        self.position.x += self.impact_offset.x;
        self.position.y += self.impact_offset.y;
    }

    /// Bicycle model: the rotation point follows the way, the front wheels steer towards a
    /// point further along it (pure pursuit, looking further ahead the faster the car goes).
    fn drive(&mut self, dt: f32, speed: f32, way: &dyn Fn(f32) -> DVec3) {
        let target = way(self.rot_long).truncate();
        if !self.started || (target - self.rear).length() > 8.0 {
            // spawned, or put somewhere else: stand on the way, straight
            let ahead = way(self.rot_long + 2.0).truncate();
            let d = ahead - target;
            self.heading = if d.length() > 1e-3 { d.x.atan2(d.y).to_degrees() } else { self.heading };
            self.rear = target;
            self.steer = 0.0;
            self.steer_cmd = 0.0;
            self.started = true;
        }
        let fwd = dir(self.heading);
        let right = DVec2::new(fwd.y, -fwd.x);
        // the planner's position along the way leads; the body keeps up with it
        let along = (target - self.rear).dot(fwd) as f32;
        let v = if speed < 0.02 && along.abs() < 0.05 { 0.0 } else { (speed + 1.5 * along).clamp(0.0, speed + 3.0) };
        // (a long wheelbase looks less far ahead: 1.2 wheelbases and more, a bus aimed 8-10 m
        // down its way and cut every junction turn over the kerb)
        let (base, per_speed) = if self.wheelbase <= 3.0 { ((1.2 * self.wheelbase).max(3.5), 0.6) } else { (3.6 + 0.5 * (self.wheelbase - 3.0), 0.45) };
        let look = (base + per_speed * speed.min(20.0)) * self.look_scale.clamp(0.25, 2.0);
        let g = way(self.rot_long + look).truncate() - self.rear;
        let alpha = (g.dot(right) as f32).atan2(g.dot(fwd) as f32).clamp(-FRAC_PI_2, FRAC_PI_2);
        let reach = (g.length() as f32).max(1.0);
        // The way itself says how tight it bends here: a junction's turn laid tighter than
        // the model's `[inv_min_turnradius]` still has to be followed - capped at the
        // model's lock the car ran wide of it, through the kerb, the corner house and the
        // people on the pavement (#249; OMSI's AI keeps to its path). The lock the bend
        // needs, and a little more, is allowed, and the wheel may turn that much faster.
        let (a, b, c) = (way(self.rot_long).truncate(), way(self.rot_long + 0.5 * look).truncate(), way(self.rot_long + look).truncate());
        let (u, w) = (b - a, c - b);
        let turn = (u.x * w.y - u.y * w.x).atan2(u.dot(w)).abs() as f32;
        let bend_k = if look > 0.1 { 2.0 * turn / look } else { 0.0 };
        let need = (bend_k * self.wheelbase).atan().to_degrees() * 1.15;
        // (both switches are read once and kept: asked of the environment here, every car
        // every frame took the process's environment lock from each worker thread)
        let limit = if omsi_cfg::flags::OMSI_AI_MODEL_LOCK.is_set() { self.max_steer } else { self.max_steer.max(need.min(60.0)) };
        // OMSI_DEBUG_AI_WIDE: every tenth of a second a car stands over 1.5 m beside its way
        if omsi_cfg::flags::OMSI_DEBUG_AI_WIDE.is_set() {
            let off = ((target - self.rear).dot(right)).abs();
            if off > 1.5 {
                static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n % 50 == 0 {
                    log::info!("ai wide: {off:.1} m beside its way at ({:.0}, {:.0}), bend needs {need:.0} deg, lock {:.0} ({} so far)", self.rear.x, self.rear.y, self.max_steer, n + 1);
                }
            }
        }
        let want = (2.0 * self.wheelbase * alpha.sin() / reach).atan().to_degrees().clamp(-limit, limit);
        if dt > 0.0 {
            let rate = self.steer_rate * dt * if limit > self.max_steer { 1.5 } else { 1.0 };
            self.steer_cmd += (want - self.steer_cmd).clamp(-rate, rate);
            self.steer += (self.steer_cmd - self.steer) * (dt / 0.08).min(1.0);
        }
        self.moved = v;
        let k = self.steer.to_radians().tan() / self.wheelbase;
        let dpsi = (v * dt * k) as f64;
        let mid = dir(self.heading + dpsi.to_degrees() * 0.5);
        self.rear += mid * (v * dt) as f64;
        self.heading = (self.heading + dpsi.to_degrees()).rem_euclid(360.0);
        self.yaw_rate = if dt > 0.0 { (dpsi / dt as f64) as f32 } else { 0.0 };
        self.a_lat = v * v * k;
        let o = self.rear - dir(self.heading) * self.rot_long as f64;
        self.position.x = o.x;
        self.position.y = o.y;
    }

    /// The body on its springs over the ground under its wheels: the plane through the
    /// contact points is where the body wants to be, braking and cornering lean it off
    /// that plane, and the difference is how far each wheel moves in its arch.
    fn settle(&mut self, dt: f32, way: &dyn Fn(f32) -> DVec3, ground: Option<&dyn Fn(f64, f64) -> Option<f64>>, contact: Option<&dyn crate::rigid::Ground>) {
        let fwd = dir(self.heading);
        let right = DVec2::new(fwd.y, -fwd.x);
        let origin = self.position.truncate();
        // the way's own height at each axle: the road the map says is there (where nothing
        // is drawn under a wheel, and what tells another level from this one)
        let mut axle_z = vec![f64::NAN; self.axle_count];
        if self.contact_z.len() != self.wheels.len() {
            self.contact_z = vec![f64::NAN; self.wheels.len()];
        }
        let mut contacts: Vec<(f32, f32, f64)> = Vec::with_capacity(self.wheels.len());
        // per wheel: no face near its lane's height, and the face found deeper down
        let mut hung: Vec<bool> = Vec::with_capacity(self.wheels.len());
        let mut low: Vec<Option<f64>> = Vec::with_capacity(self.wheels.len());
        for (k, w) in self.wheels.iter().enumerate() {
            if axle_z[w.axle].is_nan() {
                axle_z[w.axle] = way(w.long).z;
            }
            let path_z = axle_z[w.axle];
            let p = origin + right * w.lat as f64 + fwd * w.long as f64;
            // What the wheel stands on, as Omsi.exe stands an AI car's wheels: its AI cars
            // are bodies on the same wheel physics as the player's bus (0x7d5124 ->
            // 0x7e2574), asking the ground under each wheel (0x7aec3c -> 0x7a0814): a ray
            // from 3 m over the wheel (0x7c40c8 adds 3 to z) down into the terrain, the
            // splines and the first mesh of the `[surface]` objects, and the highest face it
            // meets is the ground. Each wheel on its own face: a car on a cambered road, a
            // kerb or a junction plate tilted on a hill leans with it. (Kept to the lane's
            // height, the least of an axle's wheels and only a few centimetres off that,
            // every car stood up to 5 cm in the asphalt wherever the road was drawn higher
            // than its lane or one wheel stood lower than the other, and one whose lane ran
            // under the drawn road drove through it with only its roof showing.) The ray
            // starts over the wheel's last contact, or over the lane where it has none yet.
            // Without the world's faces, the plain height sampler.
            let last = self.contact_z[k];
            let from = if last.is_finite() && (last - path_z).abs() < AI_RAY_UP { last.max(path_z) } else { path_z };
            let (drawn, deeper) = match contact {
                Some(c) => {
                    let below = c.probe(p.x, p.y, from + AI_RAY_UP).below;
                    (below.filter(|g| *g >= path_z - AI_DROP), below.filter(|g| *g < path_z - AI_DROP && *g > path_z - AI_RAY_UP))
                }
                None => (ground.and_then(|g| g(p.x, p.y)).filter(|g| (*g - path_z).abs() < AI_RAY_UP), None),
            };
            low.push(deeper);
            contacts.push((w.lat, w.long, drawn.unwrap_or(path_z)));
            hung.push(drawn.is_none());
        }
        // A wheel hangs over a drop on its spring while the others carry the car; with no
        // wheel near its lane's height the car has nothing to hang from and comes down onto
        // what is drawn under it, as Omsi.exe's wheel physics drops it. (Kept at the lane's
        // height, a car whose junction's paths ran half a metre over the drawn road drove
        // through the junction in the air, #876.)
        if hung.iter().all(|h| *h) && low.iter().any(|l| l.is_some()) {
            for (c, l) in contacts.iter_mut().zip(&low) {
                if let Some(z) = l {
                    c.2 = *z;
                }
            }
        }
        for (k, c) in contacts.iter().enumerate() {
            self.contact_z[k] = c.2;
        }
        // least-squares plane h = a + b·long + c·lat (the wheels sit symmetrically, so the
        // two slopes separate)
        let n = contacts.len().max(1) as f64;
        let (ml, mt, mh) = contacts.iter().fold((0.0, 0.0, 0.0), |acc, c| (acc.0 + c.1 as f64 / n, acc.1 + c.0 as f64 / n, acc.2 + c.2 / n));
        let (mut sll, mut slh, mut stt, mut sth) = (0.0, 0.0, 0.0, 0.0);
        for c in &contacts {
            let (dl, dt_, dh) = (c.1 as f64 - ml, c.0 as f64 - mt, c.2 - mh);
            sll += dl * dl;
            slh += dl * dh;
            stt += dt_ * dt_;
            sth += dt_ * dh;
        }
        let b = if sll > 1e-6 { slh / sll } else { 0.0 };
        let c = if stt > 1e-6 { sth / stt } else { 0.0 };
        let zg = mh - b * ml - c * mt;
        // pitch up for a road rising ahead; bank positive = right side down (c < 0)
        let pg = b.atan() as f32;
        let rg = (-c).atan() as f32;
        match self.ground {
            Some((z0, p0, r0)) if (zg - self.z).abs() < 1.0 && dt > 0.0 => {
                let k = (dt / 0.1).min(1.0);
                let rates = [((zg - z0) / dt as f64) as f32, (pg - p0) / dt, (rg - r0) / dt];
                for (r, new) in self.ground_rate.iter_mut().zip(rates) {
                    *r += (new.clamp(-5.0, 5.0) - *r) * k;
                }
                let steps = 4;
                let h = dt / steps as f32;
                for _ in 0..steps {
                    let az = (self.spring[0] * (zg - self.z) as f32 + self.damper[0] * (self.ground_rate[0] - self.vz)) / self.mass;
                    let ap = (self.spring[1] * (pg - self.pitch) + self.damper[1] * (self.ground_rate[1] - self.vpitch) + self.mass * self.a_long * self.cog) / self.inertia[0];
                    let ar = (self.spring[2] * (rg - self.roll) + self.damper[2] * (self.ground_rate[2] - self.vroll) - self.mass * self.a_lat * self.cog) / self.inertia[1];
                    self.vz += az * h;
                    self.vpitch += ap * h;
                    self.vroll += ar * h;
                    self.z += (self.vz * h) as f64;
                    self.pitch += self.vpitch * h;
                    self.roll += self.vroll * h;
                }
                self.z = self.z.clamp(zg - 0.15, zg + 0.15);
                self.pitch = self.pitch.clamp(pg - 0.07, pg + 0.07);
                self.roll = self.roll.clamp(rg - 0.09, rg + 0.09);
                // (should the springs still run away, the car stands on the ground again -
                // the clamps above hid an inf velocity until it came out as NaN)
                if !([self.vz, self.vpitch, self.vroll, self.pitch, self.roll].iter().all(|x| x.is_finite()) && self.z.is_finite()) {
                    static ONCE: std::sync::Once = std::sync::Once::new();
                    ONCE.call_once(|| log::warn!("ai car springs ran away (no number); the car is put back on the ground"));
                    (self.z, self.pitch, self.roll) = (zg, pg, rg);
                    (self.vz, self.vpitch, self.vroll) = (0.0, 0.0, 0.0);
                }
            }
            _ => {
                // first frame, or the ground under the car changed completely
                self.z = zg;
                self.pitch = pg;
                self.roll = rg;
                self.vz = 0.0;
                self.vpitch = 0.0;
                self.vroll = 0.0;
                self.ground_rate = [0.0; 3];
            }
        }
        self.ground = Some((zg, pg, rg));
        self.position.z = self.z;
        self.pitch_deg = self.pitch.to_degrees();
        self.bank_deg = self.roll.to_degrees();
        let (tp, tr) = (self.pitch.tan(), self.roll.tan());
        for (w, c) in self.wheels.iter().zip(&contacts) {
            let body = self.z + (w.long * tp - w.lat * tr) as f64;
            if let Some(s) = self.suspension.get_mut(w.axle) {
                s[w.side] = ((body - c.2) as f32).clamp(-0.15, 0.15);
            }
        }
    }

    /// A rail vehicle: its outermost axles stand on the track, the body is the line
    /// between them.
    fn ride(&mut self, way: &dyn Fn(f32) -> DVec3) {
        let f = way(self.front_long);
        let r = way(self.rear_long);
        let d = f - r;
        let run = d.truncate().length();
        if run > 1e-3 {
            self.heading = d.x.atan2(d.y).to_degrees().rem_euclid(360.0);
            self.pitch_deg = (d.z / run).atan().to_degrees() as f32;
        }
        let span = (self.front_long - self.rear_long).max(0.1);
        self.position = r + d * ((0.0 - self.rear_long) / span) as f64;
        self.bank_deg = 0.0;
        self.steer = 0.0;
        self.started = true;
    }

    /// An aircraft: on its path, nose along it, banking into a turn as much as the turn
    /// asks for (tan bank = speed × turn rate / g).
    fn fly(&mut self, dt: f32, speed: f32, way: &dyn Fn(f32) -> DVec3) {
        let p = way(0.0);
        let d = way(15.0) - way(-15.0);
        let run = d.truncate().length();
        let (h_t, p_t) = if run > 1e-3 { (d.x.atan2(d.y).to_degrees(), (d.z / run).atan().to_degrees() as f32) } else { (self.heading, self.pitch_deg) };
        if !self.started || dt <= 0.0 {
            self.heading = h_t.rem_euclid(360.0);
            self.pitch_deg = p_t;
            self.bank_deg = 0.0;
            self.yaw_rate = 0.0;
            self.started = true;
        } else {
            let k = (dt / 0.6).min(1.0) as f64;
            let dh = wrap(h_t - self.heading) * k;
            self.heading = (self.heading + dh).rem_euclid(360.0);
            let rate = (dh.to_radians() / dt as f64) as f32;
            self.yaw_rate += (rate - self.yaw_rate) * (dt / 0.5).min(1.0);
            let bank = (speed * self.yaw_rate / 9.81).atan().to_degrees().clamp(-30.0, 30.0);
            let k = (dt / 0.8).min(1.0);
            self.bank_deg += (bank - self.bank_deg) * k;
            self.pitch_deg += (p_t - self.pitch_deg) * k;
        }
        self.position = p;
        self.steer = 0.0;
    }

    /// Drive a copy of this body along a way it has not taken yet and report how close it
    /// comes to `obstacles`: the smallest plan-view separation (m, negative = the bodies
    /// would touch) between the vehicle's box (`front`/`rear` from its origin, `half_width`)
    /// and the nearest of them while the vehicle covers `distance` metres. `way(d)` is the point `d`
    /// metres along the new way from where the vehicle is now (the same way `step` is given,
    /// e.g. with a pull-out round something standing ahead in it); the vehicle starts at
    /// `speed` (a standing one first turns its wheels for `wait` seconds, as a driver does
    /// before moving off) and speeds up at `accel` to at most `v_max`. The steering is the real one -
    /// its rate limit, its lock and the pure pursuit that cuts in on a steep S-bend - which
    /// is what decides whether a car standing a few metres behind a bus can get its front
    /// corner past the bus's.
    #[allow(clippy::too_many_arguments)]
    pub fn sweep_clearance(&self, way: &dyn Fn(f32) -> DVec3, speed: f32, wait: f32, accel: f32, v_max: f32, distance: f32, extent: (f32, f32, f32), obstacles: &[crate::collision::Obb]) -> f64 {
        let mut body = self.clone();
        let (front, rear, half_width) = extent;
        let dt = 1.0 / 20.0;
        let mut v = speed.max(0.0);
        let mut x = 0.0f32;
        let gap = |b: &AiBody| {
            let me = crate::collision::Obb::vehicle(b.position.truncate(), b.heading, front as f64, rear as f64, half_width as f64);
            obstacles.iter().map(|o| me.separation(o)).fold(f64::MAX, f64::min)
        };
        let mut best = gap(&body);
        if v < 0.05 {
            for _ in 0..(wait / dt).round() as usize {
                body.drive(dt, 0.0, way);
            }
        }
        // (at most a minute of simulated time: a car that cannot move is not getting past)
        for _ in 0..1200 {
            if x >= distance {
                break;
            }
            v = (v + accel.max(0.3) * dt).min(v_max.max(0.5));
            let ds = v * dt;
            x += ds;
            let at = x;
            body.drive(dt, v, &|d| way(at + d));
            best = best.min(gap(&body));
        }
        best
    }

    /// Drive a copy of this body along a way it has not taken yet, at `speed` (m/s, at least
    /// walking pace), and show `visit` each pose with the distance driven: every `step`
    /// metres for the first 20 m, twice that beyond, as far as `distance` or until `visit`
    /// says stop (true). The steering is the real one (`drive`: its rate, its lock, the
    /// pure pursuit cutting a bend), so the poses are where this body will be, not where
    /// its way is. `way(d)` is the point `d` metres along the new way from where the
    /// vehicle's planner is now.
    pub fn predict(&self, way: &dyn Fn(f32) -> DVec3, speed: f32, step: f32, distance: f32, visit: &mut dyn FnMut(f32, &AiBody) -> bool) {
        if self.kind != MotionKind::Road {
            return;
        }
        let mut body = self.clone();
        let v = speed.max(1.5);
        let mut x = 0.0f32;
        while x < distance {
            let ds = if x < 20.0 { step } else { 2.0 * step };
            x += ds;
            let at = x;
            body.drive(ds / v, v, &|d| way(at + d));
            if visit(x, &body) {
                return;
            }
        }
    }

    /// Take a step back: the pose before it (`before`), the wheels still turned as they
    /// were turned in it. A body whose step ran into something stands where it was; had the
    /// steering gone back with the pose, the next step turned the same corner into the same
    /// thing again.
    pub fn take_back(&mut self, before: AiBody) {
        let (steer, steer_cmd, look_scale) = (self.steer, self.steer_cmd, self.look_scale);
        *self = before;
        self.steer = steer;
        self.steer_cmd = steer_cmd;
        self.look_scale = look_scale;
        self.moved = 0.0;
    }

    /// Hand the pose to the vehicle: position and attitude, the accelerations its scripts
    /// read and each wheel's travel.
    pub fn apply(&self, v: &mut VehicleInstance) {
        // A road vehicle's body stands on the contact plane of its wheels; its static pose
        // over that plane (`[ai_deltaheight]` and the model's wheel geometry) comes on top,
        // once: the springs here only add their travel about it. Rail and air vehicles
        // follow their path's own height.
        let (lift, delta) = if self.kind == MotionKind::Road { v.ai_rest_offset() } else { (0.0, 0.0) };
        v.position = self.position + DVec3::new(0.0, 0.0, lift as f64);
        v.heading = self.heading;
        v.pitch = self.pitch_deg;
        v.bank = self.bank_deg;
        v.physics.accel = Vec3::new(self.a_lat, self.a_long, 0.0);
        for (axle, s) in v.physics.wheels.iter_mut().zip(&self.suspension) {
            axle[0].suspension = s[0] + delta;
            axle[1].suspension = s[1] + delta;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::vehicle::Axle;

    #[test]
    fn back_in_ramp_keeps_the_sideways_acceleration() {
        for (offset, kmh) in [(3.28f32, 40.0f32), (3.28, 44.0), (2.5, 34.0), (5.0, 50.0)] {
            let v = kmh / 3.6;
            let len = back_in_ramp(offset, v, BACK_IN_LAT_ACCEL);
            // the peak of the smoothstep's second derivative, at its ends
            let peak = 6.0 * offset * v * v / (len * len);
            assert!(peak <= BACK_IN_LAT_ACCEL + 1e-3, "{offset} m at {kmh} km/h: {len:.1} m, {peak:.2} m/s²");
            assert!(len / v >= 2.0, "{offset} m at {kmh} km/h: {len:.1} m is {:.2} s", len / v);
        }
        // creeping round something: the old geometric length
        assert_eq!(back_in_ramp(3.0, 1.0, BACK_IN_LAT_ACCEL), 10.5);
    }

    #[test]
    fn a_road_ai_vehicle_recoils_then_returns_to_its_path() {
        let mut body = AiBody::new(&golf(), MotionKind::Road);
        body.place(&|_| DVec3::ZERO, None, None, 0.0);
        let origin = body.position;
        body.collision_impulse(DVec2::new(0.0, -1.0), 5.0, 10_000.0, 1_000.0);
        body.step(0.1, 0.0, &|_| DVec3::ZERO, None, None);
        assert!(body.position.y < origin.y - 0.1, "the collision must move the car backwards");
        for _ in 0..80 {
            body.step(0.05, 0.0, &|_| DVec3::ZERO, None, None);
        }
        assert!((body.position.y - origin.y).abs() < 0.1, "the AI should settle back onto its path");
    }

    #[test]
    fn a_heavier_striker_gives_the_ai_a_stronger_recoil() {
        let recoil_speed = |striker| {
            let mut body = AiBody::new(&golf(), MotionKind::Road);
            body.place(&|_| DVec3::ZERO, None, None, 0.0);
            body.collision_impulse(DVec2::new(0.0, -1.0), 3.0, striker, 1_000.0);
            body.impact_velocity.length()
        };
        let light = recoil_speed(1_000.0);
        let heavy = recoil_speed(12_000.0);
        assert!(heavy > light * 1.5, "light striker {light:.3} m/s, heavy striker {heavy:.3} m/s");
        // momentum: a 12 t bus at 3 m/s gives a 1 t car at most (1 + e) 12/13 of it
        assert!((heavy - 1.2 * 12.0 / 13.0 * 3.0).abs() < 1e-3, "{heavy}");
    }

    /// A tram on OMSI's near-rigid springs (Mainz's GT6M: `achse_feder` and `achse_daempfer`
    /// 5000 on every axle) driving over a road with small bumps and braking: its springs
    /// stay numbers (they ran away within a minute and put the car at NaN, #3161).
    #[test]
    fn stiff_springs_stay_numbers() {
        let mut def = Vehicle { mass: 12.0, moment_of_inertia: [1760.0, 54.0, 1803.0], cog_height: 0.6, rot_pnt_long: 0.0, inv_min_turn_radius: 0.1, ..Default::default() };
        def.axles = [7.15, 4.65, -4.65, -7.15].iter().map(|&long| Axle { long, max_width: 1.606, spring: 5000.0, damper: 5000.0, wheel_diameter: 0.503, ..Default::default() }).collect();
        let mut body = AiBody::new(&def, MotionKind::Road);
        let bumpy = |_x: f64, y: f64| Some(10.0 + 0.02 * (y * 1.7).sin() + 0.01 * (y * 5.3).sin());
        body.place(&|d| DVec3::new(0.0, d as f64, 10.0), Some(&bumpy), None, 10.0);
        let dt = 0.02;
        let mut at = 0.0f64;
        for i in 0..3000 {
            let speed = if (i / 250) % 2 == 0 { 10.0 } else { 2.0 };
            at += speed as f64 * dt as f64;
            let s = at;
            body.step(dt, speed, &|d| DVec3::new(0.0, s + d as f64, 10.0), Some(&bumpy), None);
            assert!(body.position.is_finite() && body.vz.is_finite() && body.vpitch.is_finite(), "step {i}: {:?} vz {} vpitch {}", body.position, body.vz, body.vpitch);
        }
    }

    fn golf() -> Vehicle {
        let mut v = Vehicle { mass: 1.0, moment_of_inertia: [1.49, 0.40, 1.56], cog_height: 0.4, rot_pnt_long: -1.304, inv_min_turn_radius: 0.1904, ..Default::default() };
        v.axles = vec![Axle { long: 1.21, max_width: 1.606, spring: 40.0, damper: 2.0, wheel_diameter: 0.503, ..Default::default() }, Axle { long: -1.304, max_width: 1.606, spring: 50.0, damper: 2.0, wheel_diameter: 0.503, ..Default::default() }];
        v
    }

    /// A way: straight north for 50 m, then a right turn of radius `r`.
    fn bend(r: f64, s: f64) -> DVec3 {
        if s < 50.0 {
            DVec3::new(0.0, s, 10.0)
        } else {
            let a = (s - 50.0) / r;
            DVec3::new(r - r * a.cos(), 50.0 + r * a.sin(), 10.0)
        }
    }

    #[test]
    fn follows_a_bend_smoothly() {
        let def = golf();
        let mut body = AiBody::new(&def, MotionKind::Road);
        let speed = 6.0f32;
        let dt = 1.0 / 30.0;
        let mut s = 20.0f64;
        body.place(&|d| bend(12.0, s + d as f64), None, None, speed);
        let mut last_yaw: Option<f32> = None;
        let mut worst_jump = 0.0f32;
        let mut worst_off = 0.0f64;
        for _ in 0..(12.0 / dt) as usize {
            s += (speed * dt) as f64;
            let at = s;
            body.step(dt, speed, &|d| bend(12.0, at + d as f64), None, None);
            if let Some(y) = last_yaw {
                worst_jump = worst_jump.max((body.yaw_rate - y).abs().to_degrees());
            }
            last_yaw = Some(body.yaw_rate);
            // the rear axle stays on the way (a circle has no steady error; entering it costs a little)
            let rear = body.rear;
            let q = bend(12.0, at + def.rot_pnt_long as f64).truncate();
            worst_off = worst_off.max((rear - q).length());
        }
        // a quarter turn done at 6 m/s: the heading has turned right, the yaw rate never
        // jumps by more than a few degrees per second between frames, the car keeps to its lane
        assert!(body.heading > 60.0 && body.heading < 300.0, "heading {}", body.heading);
        assert!(worst_jump < 3.0, "yaw rate jumps by {worst_jump} deg/s in one frame");
        assert!(worst_off < 0.6, "rear axle {worst_off} m off the way");
    }

    #[test]
    fn brakes_nose_down_and_rolls_out_of_a_turn() {
        let def = golf();
        let mut body = AiBody::new(&def, MotionKind::Road);
        let flat = |_x: f64, _y: f64| Some(10.0);
        body.place(&|d| DVec3::new(0.0, d as f64, 10.0), Some(&flat), None, 10.0);
        let dt = 1.0 / 30.0;
        let mut speed = 10.0f32;
        let mut s = 0.0f64;
        let mut min_pitch = 0.0f32;
        for _ in 0..60 {
            speed = (speed - 3.0 * dt).max(0.0);
            s += (speed * dt) as f64;
            let at = s;
            body.step(dt, speed, &|d| DVec3::new(0.0, at + d as f64, 10.0), Some(&flat), None);
            min_pitch = min_pitch.min(body.pitch_deg);
        }
        assert!(min_pitch < -0.1, "braking pitch {min_pitch}");
        assert!((body.position.z - 10.0).abs() < 0.1);
        // the front wheels come up in their arches while the nose dives
        assert!(body.suspension[0][0] < 0.0 && body.suspension[1][0] > 0.0, "{:?}", body.suspension);
    }

    /// A two-axle lorry: 4.5 m between the axles, a turning circle of 11 m radius.
    fn lorry() -> Vehicle {
        let mut v = Vehicle { mass: 12.0, moment_of_inertia: [40.0, 10.0, 45.0], cog_height: 1.2, rot_pnt_long: -2.0, inv_min_turn_radius: 0.09, ..Default::default() };
        v.axles = vec![Axle { long: 2.5, max_width: 2.0, spring: 300.0, damper: 20.0, wheel_diameter: 1.0, ..Default::default() }, Axle { long: -2.0, max_width: 1.8, spring: 400.0, damper: 25.0, wheel_diameter: 1.0, ..Default::default() }];
        v
    }

    /// A standing bus `half_width` × 2 wide, `gap` metres ahead of a vehicle whose front is `front`
    /// metres ahead of its origin at (0, 0) facing north.
    fn bus_ahead(front: f32, gap: f32, half_width: f64) -> crate::collision::Obb {
        crate::collision::Obb::vehicle(DVec2::new(0.0, (front + gap) as f64 + 6.0), 0.0, 6.0, 6.0, half_width)
    }

    #[test]
    fn a_car_too_close_behind_a_bus_cannot_steer_round_it() {
        let def = golf();
        let extent = (2.1, 2.2, 0.85);
        let best = |gap: f32| {
            pull_out_ramps(gap, extent.0, false)
                .into_iter()
                .map(|ramp| {
                    let way = straight_pull_out(3.3, ramp);
                    let mut body = AiBody::new(&def, MotionKind::Road);
                    body.place(&way, None, None, 0.0);
                    body.sweep_clearance(&way, 0.0, PULL_OUT_WAIT, PULL_OUT_ACCEL, 8.0, gap + 6.0, extent, &[bus_ahead(extent.0, gap, 1.25)])
                })
                .fold(f64::MIN, f64::max)
        };
        // half a metre behind the bus no S-curve gets the front corner past (the lock
        // is 25°, the steering takes a moment to get there); six metres back it is easy
        let close = best(0.5);
        assert!(close < PULL_OUT_CLEARANCE, "clearance {close} from 0.5 m");
        let far = best(6.0);
        assert!(far >= PULL_OUT_CLEARANCE, "clearance {far} from 6 m");
        // and the swept body does leave the lane: after the pull-out it is 3.3 m over
        let way = straight_pull_out(3.3, 8.0);
        let mut body = AiBody::new(&def, MotionKind::Road);
        body.place(&way, None, None, 0.0);
        let mut x = 0.0f32;
        let dt = 1.0 / 20.0;
        for _ in 0..400 {
            x += 4.0 * dt;
            let at = x;
            body.step(dt, 4.0, &|d| way(at + d), None, None);
        }
        assert!((body.position.x + 3.3).abs() < 0.2, "ended at x {}", body.position.x);
    }

    #[test]
    fn room_to_pull_out_depends_on_the_steering() {
        let car = pull_out_room(&golf(), (2.1, 2.2, 0.85), 1.25, 3.3);
        let lorry = pull_out_room(&lorry(), (3.6, 3.4, 1.25), 1.25, 3.3);
        // a bus standing a little further over (or wider) needs more room
        let wide = pull_out_room(&golf(), (2.1, 2.2, 0.85), 1.6, 3.3);
        assert!(wide > car, "{wide} against {car}");
        assert!(car > 1.5 && car < 6.0, "car {car}");
        assert!(lorry > car + 0.5 && lorry < 12.5, "lorry {lorry}, car {car}");
        // what it found works: from that room less the half metre of slack one of the
        // curves clears, from a metre less none does
        let clears = |gap: f32| {
            pull_out_ramps(gap, 2.1, false).into_iter().any(|ramp| {
                let way = straight_pull_out(3.3, ramp);
                let mut body = AiBody::new(&golf(), MotionKind::Road);
                body.place(&way, None, None, 0.0);
                body.sweep_clearance(&way, 0.0, PULL_OUT_WAIT, PULL_OUT_ACCEL, 8.0, gap + 6.0, (2.1, 2.2, 0.85), &[bus_ahead(2.1, gap, 1.25)]) >= PULL_OUT_CLEARANCE
            })
        };
        assert!(clears(car - 0.5));
        assert!(!clears(car - 1.0));
    }

    #[test]
    fn pull_out_curves_run_gentle_to_steep() {
        let r = pull_out_ramps(3.0, 2.0, false);
        assert!(r.windows(2).all(|w| w[0] > w[1]), "{r:?}");
        assert!(r.iter().all(|&x| (4.0..=12.0).contains(&x)), "{r:?}");
        let rolling = pull_out_ramps(30.0, 2.0, true);
        assert_eq!(rolling[0], 20.0);
        assert!(rolling.iter().all(|&x| (4.0..=20.0).contains(&x)), "{rolling:?}");
    }

    /// Every wheel stands on the face drawn under it, as Omsi.exe's ground query stands an
    /// AI car's (the highest face under a ray from 3 m over the wheel): a road drawn 8 cm
    /// over the lane and cambered across it carries each wheel at its own height, and a lane
    /// laid 0.9 m under its road has the car on the road, not inside it.
    #[test]
    fn each_wheel_stands_on_the_face_under_it() {
        let def = golf();
        for (lift, camber) in [(0.08f64, 0.03f64), (0.9, 0.0), (0.08, -0.05)] {
            let road = move |x: f64, _y: f64| 10.0 + lift + camber * x;
            let ground = move |x: f64, y: f64, top: f64| {
                let z = road(x, y);
                if z <= top { crate::rigid::GroundProbe { below: Some(z), above: None } } else { crate::rigid::GroundProbe { below: Some(9.0), above: Some(z) } }
            };
            let way = |d: f32| DVec3::new(0.0, d as f64, 10.0);
            let mut body = AiBody::new(&def, MotionKind::Road);
            body.place(&way, None, Some(&ground), 0.0);
            let mut x = 0.0f32;
            for _ in 0..90 {
                x += 0.2;
                let at = x;
                body.step(1.0 / 30.0, 6.0, &|d| way(at + d), None, Some(&ground));
            }
            for (w, z) in body.wheels.iter().zip(&body.contact_z) {
                let p = body.position.truncate() + DVec2::new(w.lat as f64, w.long as f64);
                assert!((z - road(p.x, p.y)).abs() < 0.01, "lift {lift} camber {camber}: wheel at {z:.3}, road {:.3}", road(p.x, p.y));
            }
            assert!((body.position.z - (10.0 + lift)).abs() < 0.02, "lift {lift} camber {camber}: body at {:.3}", body.position.z);
            let lean = (-camber).atan().to_degrees() as f32;
            assert!((body.bank_deg - lean).abs() < 0.3, "camber {camber}: bank {} against {lean}", body.bank_deg);
        }
    }

    /// A lane laid half a metre over its drawn road (a junction's paths): with no wheel
    /// near the lane's height the car comes down onto the road instead of driving in the air;
    /// a road far below (under a bridge the lane crosses) still does not pull it down.
    #[test]
    fn a_car_over_a_lower_road_comes_down_onto_it() {
        let def = golf();
        for (drop, expect) in [(0.5f64, 9.5f64), (6.0, 10.0)] {
            let ground = move |_x: f64, _y: f64, top: f64| {
                let z = 10.0 - drop;
                crate::rigid::GroundProbe { below: (z <= top).then_some(z), above: None }
            };
            let way = |d: f32| DVec3::new(0.0, d as f64, 10.0);
            let mut body = AiBody::new(&def, MotionKind::Road);
            body.place(&way, None, Some(&ground), 0.0);
            let mut x = 0.0f32;
            for _ in 0..60 {
                x += 0.2;
                let at = x;
                body.step(1.0 / 30.0, 6.0, &|d| way(at + d), None, Some(&ground));
            }
            assert!((body.position.z - expect).abs() < 0.02, "drop {drop}: body at {:.3}", body.position.z);
        }
    }

    #[test]
    fn aircraft_keep_their_height() {
        let mut def = golf();
        def.axles.truncate(1);
        let mut body = AiBody::new(&def, MotionKind::Air);
        let way = |d: f32| DVec3::new(0.0, d as f64, 300.0 - d as f64 * 0.05);
        body.place(&way, None, None, 70.0);
        body.step(1.0 / 30.0, 70.0, &way, None, None);
        assert!((body.position.z - 300.0).abs() < 1e-6);
        assert!(body.pitch_deg < -2.0 && body.pitch_deg > -4.0, "pitch {}", body.pitch_deg);
    }
}
