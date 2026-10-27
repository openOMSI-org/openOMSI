//! The force feedback model of a wheel: what the rim is told each frame. The steering part
//! (parking drag, self-centring, the steering box's damping) and over it the vibrations -
//! the wheels' jolts, the scripts' shaking and the tremble of the tarmac and the engine
//! (`Micro`), every one of them eased away rather than cut off when it ends.

use super::Controllers;

/// What the force feedback is made of this frame (OMSI's `FF_*` variables of the bus and
/// its speed).
#[derive(Debug, Clone, Copy, Default)]
pub struct FfInput {
    /// Driving (from the driver's seat): the forces are on; else the wheel is let go.
    pub on: bool,
    pub kmh: f32,
    /// Sideways acceleration in the bus frame (m/s², right positive).
    pub lateral_accel: f32,
    /// Short jolt from wheel suspension travel or an impact, 0..1.
    pub wheel_bump: f32,
    /// Time since the latest wheel jolt, for a repeatable initial kick.
    pub wheel_bump_age: f32,
    /// `FF_Vib_Amp` 0..1 and `FF_Vib_Period` (hundredths of a second) of the scripts.
    pub vib_amp: f32,
    pub vib_period: f32,
    /// `StreetCond` of the road under the wheels: 0 dry, 1 wet, 2 covered in snow.
    pub street_cond: f32,
    /// The engine's speed (rpm; 0 while it is not running).
    pub engine_rpm: f32,
    /// How hard the engine is working (0..1).
    pub engine_load: f32,
    /// The trembling of the tarmac and the engine, worked out here from the four above
    /// (see `Micro`); the caller leaves it at 0.
    pub micro: f32,
    pub dt: f32,
}

/// How long (s) a jolt's or a script's vibration eases away once it stops, unless the
/// settings say otherwise (`ff_fade`).
pub(super) const FF_FADE: f32 = 0.28;

/// What is left of a vibration `t` seconds after its source stopped: all of it while it is
/// still coming, then easing away over `fade` seconds. A force that falls to zero in a
/// single step is a jolt of its own - the rim feels it and the wheel's motor clunks - so
/// everything that shakes the wheel once eases out rather than letting go. `fade` 0 is the
/// old behaviour of stopping where it stands.
fn fade_gain(t: f32, fade: f32) -> f32 {
    if fade <= 0.0 {
        return 1.0;
    }
    if t >= fade {
        return 0.0;
    }
    // a smoothstep, so the fade neither starts nor ends with a step of its own
    let k = 1.0 - t / fade;
    k * k * (3.0 - 2.0 * k)
}

/// A script's shaking (`FF_Vib_Amp` and `FF_Vib_Period`) and the little state its fade
/// needs. The scripts write both afresh every frame, so once they stop there is nothing
/// left to ease away unless the last of what they asked for is kept here.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ScriptVib {
    /// The last amplitude the scripts reached the wheel with.
    pub(super) amp: f32,
    /// The period that came with it: the scripts stop writing that too, and a rattle eased
    /// away at the wrong period would not be the one that stopped.
    pub(super) period: f32,
    /// How long ago (s) they last reached the wheel.
    pub(super) t: f32,
}

impl ScriptVib {
    /// What the wheel is told this frame: the retained amplitude eased on the settings'
    /// fade since the scripts went quiet, and the period to shake it at.
    ///
    /// The fade cannot be a gain applied to the incoming amplitude, because that amplitude
    /// is already zero on the frame the scripts stop and any gain leaves zero at zero - the
    /// wheel then drops the shake in a single step, which is the jolt the fade is here to
    /// avoid. It shows on the DirectInput path, where a zero amplitude stops the periodic
    /// effect outright, and on the fallback, whose shake is worked out of the same
    /// amplitude. The fade at zero stops the shake where it stands, as it always did.
    pub(super) fn step(&mut self, on: bool, incoming: f32, period: f32, fade: f32, dt: f32) -> (f32, f32) {
        let incoming = if on { incoming.clamp(0.0, 1.0) } else { 0.0 };
        if incoming > 0.004 {
            self.amp = incoming;
            if period > 0.0 {
                self.period = period;
            }
            self.t = 0.0;
        } else if on && fade > 0.01 {
            self.t += dt.max(0.0);
        } else {
            self.amp = 0.0;
            self.t = 0.0;
        }
        (self.amp * fade_gain(self.t, fade), self.period)
    }
}

/// The trembling that never stops while the bus runs: the grain of the tarmac under the
/// wheels, and the engine's own buzz coming up through the frame.
///
/// Both are small - a tenth of the wheel's lock at most, and a wheel with a real motor
/// spends most of its travel on parking resistance anyway - and both are fast enough to be
/// felt, which is why a wheel feels them as a buzz and not as a push. A constant-force
/// wheel can only be told one force at a time; a force that changes every few milliseconds
/// comes out of the motor as a tremble rather than as a movement of the wheel, which is
/// what the road is. A slow one is felt as nothing at all: the rim goes where it is told and
/// the hands go with it, so the rate matters more here than the size. And a force that lines
/// up with itself is felt as something solid being carried while one that does not is felt
/// as a loose surface, so most of the road is sines and only a little of it is noise.
///
/// It keeps a little state of its own: the distance the bus has rolled (the grain is read
/// along the road, so a bus standing still feels none of it and the same stretch of tarmac
/// shakes the same way twice) and the engine's angle (so its buzz is at the engine's own
/// rate whatever gear the bus is in, and a script's rpm that twitches does not reach the
/// wheel as a rattle).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Micro {
    /// The distance the bus has rolled (m).
    pub(super) roll: f32,
    /// The engine's speed, with the twitches of the script that writes it taken out (rpm).
    pub(super) rpm: f32,
    /// Where each part of the engine's buzz stands (rad). Each is kept on its own because a
    /// phase reduced on its own rate is exact for that rate, and a phase shared between them
    /// is not: wrap the crank angle into a single turn, then take half or a quarter of it,
    /// and you land on a different half turn every time instead of carrying on.
    pub(super) crank: f32,
    pub(super) firing: f32,
    pub(super) mass: f32,
}

impl Micro {
    /// How much the wheel trembles this frame, of its full lock (-1..1). `road` and
    /// `engine` are the settings' strengths (0 = neither); the rest is what the bus is
    /// doing - `kmh`, the road's `street_cond` (0 dry, 1 wet, 2 snow), the engine's rpm and
    /// how hard it is working.
    pub(super) fn sample(&mut self, dt: f32, kmh: f32, street_cond: f32, rpm: f32, load: f32, road: f32, engine: f32) -> f32 {
        let dt = dt.clamp(0.0, 0.1);
        // The grain of the road: nothing at all at a standstill, all of it by 30 km/h, and
        // a wet or snowy road hums under the tyres more than dry asphalt does.
        let v = kmh.abs() / 3.6;
        let drive = (v / 8.5).min(1.0);
        let rough = 0.6 + 0.25 * street_cond.clamp(0.0, 2.0);
        self.roll += v * dt;
// The road, read along the distance: mostly its ride - the shapes the whole bus is
        // being carried over - and only a seventh its grain. A wheel is told one force at a
        // time, and the difference the driver feels between a bus that feels heavy and one
        // whose wheel rattles is whether that force lines up with itself. Noise never does:
        // read along the road it gives every part of the spectrum a share, and a signal with
        // no shape to it is felt as a loose surface however fast it is felt. A ride made of
        // sines whose lengths do not fit together lines up and never repeats over a stretch
        // of road, and that is what reads as one solid weight being carried. The grain is
        // left in under it, small, because it is what says the road is made of something and
        // not polished. The three lengths are six metres, two and a quarter and nine
        // tenths: the deep ones are the mass, the last is what the tyre feels, and none of
        // them is so short that it comes back as a beat of its own before a wheel cannot be
        // told about it (a hundred times a second, so the 0.1 m grain is let go above 40 Hz,
        // where the 0.9 m one carries the surface on its own).
        let fine = 1.0 - (v / 0.4 / 40.0).clamp(0.0, 1.0);
        let shape = ride(self.roll / 6.0) * 0.34 + ride(self.roll / 2.2 + 5.3) * 0.26 + ride(self.roll / 0.9 + 11.9) * 0.4;
        let surface = grain(self.roll / 0.25) * 0.55 + grain(self.roll / 0.1 + 31.7) * 0.45 * fine;
        let tarmac = (shape * 0.85 + surface * 0.15) * drive * rough * 0.25 * road;
        // The engine's buzz, each part on the rate it really runs at: the crankshaft
        // turning, the firing pulses above it, and a low rumble under both standing for the
        // mass the frame works against. A wheel is told its force once a frame, so nothing
        // above about 28 Hz can be carried at all and each part is let go as it reaches for
        // it; above the revs where that has happened the crank and the pulses are both gone
        // and what is left is the low rumble.
        let turn = std::f32::consts::TAU * dt;
        self.rpm += (rpm.clamp(0.0, 4500.0) - self.rpm) * (dt / 0.08).min(1.0);
        let spin = |p: &mut f32, per_turn: f32| *p = (*p + self.rpm / per_turn * turn).rem_euclid(std::f32::consts::TAU);
        // a wheel is told its force once a frame, so nothing above about 28 Hz can be
        // carried at all and each part is let go over the last stretch before that
        let room = |f: f32| 1.0 - ((f - 28.0) / 7.0).clamp(0.0, 1.0);
        spin(&mut self.crank, 60.0);
        spin(&mut self.firing, 30.0);
        spin(&mut self.mass, 240.0);
        let running = ((self.rpm - 150.0) / 400.0).clamp(0.0, 1.0);
        // an engine working against its mounts twists harder than one idling
        let mount = 0.7 + 0.6 * load.clamp(0.0, 1.0);
        let lump = 0.6 + 0.4 * (self.rpm / 2400.0).clamp(0.0, 1.0);
        let hum = running * mount * lump
            * (self.crank.sin() * room(self.rpm / 60.0) * 0.03
                + self.firing.sin() * room(self.rpm / 30.0) * 0.02
                + self.mass.sin() * (self.rpm / 2400.0).min(1.0) * 0.055)
            * engine;
        tarmac + hum
    }
}

/// The ride of the road under the wheels: smooth, always moving, and never the same twice.
///
/// Where `grain` reads the surface, this reads the shape. It is sines whose lengths do not
/// fit into one another, so it never repeats itself over a stretch of road and never jumps
/// either. Its sum is 1 at the very most, so it can be mixed by eye.
fn ride(x: f32) -> f32 {
    let t = std::f32::consts::TAU * x;
    t.sin() * 0.55 + (t * 1.7 + 1.3).sin() * 0.3 + (t * 2.9 + 4.2).sin() * 0.15
}

/// Value noise along one line, -1..1. The grain of a road surface is not a sine wave, and
/// a wheel handed the same sine wave every few metres reads it as a whine from the gearbox
/// rather than as tarmac.
fn grain(x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let s = f * f * (3.0 - 2.0 * f); // the lattice's points, eased into one another
    let a = lattice(i as i32);
    let b = lattice(i as i32 + 1);
    (a + (b - a) * s) * 2.0 - 1.0
}

/// One point of the grain's lattice, hashed from its place in whole metres (so a long
/// drive does not walk the pattern off).
fn lattice(i: i32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x27d4_eb2d) ^ 0x9e37_79b1;
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    (h >> 8) as f32 / 16_777_216.0
}

/// Front-wheel contact reaches the steering linkage directly; rear-wheel contact
/// reaches it through the bus body at a lower strength.
impl Controllers {
    /// The wheels' jolt this frame (None: not driving, the travel is settled anew on return).
    pub fn wheel_bump(&mut self, body: Option<&omsi_sim::rigid::RigidBody>, kmh: f32, dt: f32) -> f32 {
        let Some(body) = body else {
            self.settled.clear();
            return 0.0;
        };
        let travel: Vec<f32> = body.wheels.iter().map(|w| w.compression).collect();
        let ripples = settle(&mut self.settled, &travel, dt);
        body.wheels.iter().zip(ripples).enumerate().map(|(i, (w, ripple))| {
            let impact_speed = body.wheel_impacts.iter().filter(|hit| hit.obstacle == i).map(|hit| hit.speed).fold(0.0, f32::max);
            bump_strength(ripple, impact_speed, kmh) * if w.steered { 1.0 } else { 0.55 }
        }).fold(0.0, f32::max)
    }
}

/// How far each wheel's travel leaves where it settled (a 1-2 cm road seam is no jolt).
fn settle(settled: &mut Vec<f32>, travel: &[f32], dt: f32) -> Vec<f32> {
    if settled.len() != travel.len() {
        *settled = travel.to_vec();
    }
    let k = 1.0 - (-dt.max(0.0) / 0.1).exp();
    travel.iter().zip(settled.iter_mut()).map(|(t, s)| {
        let ripple = t - *s;
        *s += ripple * k;
        ripple
    }).collect()
}

fn bump_strength(ripple: f32, impact_speed: f32, kmh: f32) -> f32 {
    let suspension = ((ripple.abs() - 0.012) / 0.09).clamp(0.0, 1.0);
    let impact = ((impact_speed - 0.12) / 1.1).clamp(0.0, 1.0);
    suspension.max(impact) * (kmh.abs() / 4.0).clamp(0.0, 1.0)
}

/// How much of the full force `FF_Vib_Amp` 1 shakes the wheel with.
pub(super) const VIB_SHARE: f32 = 0.25;

/// Per-wheel force feedback tuning, `[openOMSI.FFTuning]` in gamectrler.cfg (OMSI 2 ignores it).
///
/// Wheels differ more than the bus does: a direct-drive base passes on the smallest force
/// as it is, while a belt- or gear-driven one swallows the first few per cent of its range
/// in friction, and a light rim swings past the centre where a heavy one settles. So the
/// parts of the steering force can be weighted per wheel, a wheel that loses small forces
/// can have them lifted past its dead zone, and a wheel that buzzes on a frame-rate force
/// can have the steering part smoothed. The vibrations are not touched: they are
/// `[FFScale]`'s second value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FfTune {
    /// Self-centring (the tyres' aligning torque and the turn's lateral pull), 0..2.
    pub centring: f32,
    /// The steering box's damping, the quick return's included, 0..3.
    pub damping: f32,
    /// The tyres' scrub when the wheel is turned, which is what a parked bus is made of, 0..2.
    pub parking: f32,
    /// 0..0.2: lifts small steering forces above a belt or gear wheel's dead zone.
    pub min_force: f32,
    /// Seconds 0..0.05: a one-pole low-pass on the steering part only.
    pub smoothing: f32,
}

impl Default for FfTune {
    fn default() -> Self {
        Self { centring: 1.0, damping: 1.0, parking: 1.0, min_force: 0.0, smoothing: 0.0 }
    }
}

impl FfTune {
    /// Each value's range, in the order of the file's lines.
    pub const RANGES: [(f32, f32); 5] = [(0.0, 2.0), (0.0, 3.0), (0.0, 2.0), (0.0, 0.2), (0.0, 0.05)];

    /// The values in the order the file keeps them.
    pub fn values(&self) -> [f32; 5] {
        [self.centring, self.damping, self.parking, self.min_force, self.smoothing]
    }

    /// From the file's order, each value brought into its range (a value that is no number
    /// at all, or missing, is the default).
    pub fn from_values(values: &[f32]) -> FfTune {
        let d = FfTune::default().values();
        let v: [f32; 5] = std::array::from_fn(|i| {
            let (lo, hi) = FfTune::RANGES[i];
            // (+ 0.0: a -0 is written "-0.000", which is no default to the file)
            values.get(i).copied().filter(|v| !v.is_nan()).map_or(d[i], |v| v.clamp(lo, hi) + 0.0)
        });
        FfTune { centring: v[0], damping: v[1], parking: v[2], min_force: v[3], smoothing: v[4] }
    }

    /// The same tuning with every value in its range.
    pub fn clamped(&self) -> FfTune {
        FfTune::from_values(&self.values())
    }
}

/// The force on a wheel as the bus feels it, with the stock tuning and no smoothing (the
/// tests' way into `tuned_wheel_force`).
#[cfg(test)]
pub(super) fn wheel_force(f: &FfInput, x: f32, x0: f32, t: &mut f32, k_springs: f32, k_effects: f32) -> f32 {
    tuned_wheel_force(f, x, x0, t, &mut 0.0, k_springs, k_effects, &FfTune::default())
}

/// The force on a wheel standing at `x` (-1 full left .. 1), `x0` the frame before: -1..1.
/// Tyre scrub resists turning the wheel at a standstill and falls away once the bus rolls.
/// Self-aligning torque then returns the wheel to centre, with a softer response near full
/// lock and feedback from the bus's lateral acceleration. On top of that come front-wheel
/// jolts, the scripts' shaking and the tremble of the tarmac and the engine.
///
/// `tune` weights the parts of the steering force for this wheel; `smoothed` is the
/// steering force its low-pass last handed on, kept by the caller beside `t`. With the
/// default tuning the force is the very same as it was before the tuning existed: every
/// weight is a multiplication by one, which changes no bit of a float.
#[allow(clippy::too_many_arguments)]
pub(super) fn tuned_wheel_force(f: &FfInput, x: f32, x0: f32, t: &mut f32, smoothed: &mut f32, k_springs: f32, k_effects: f32, tune: &FfTune) -> f32 {
    let dt = f.dt.max(1e-3);
    let v = f.kmh.abs();
    let x = x.clamp(-1.0, 1.0);
    // A parked bus has no rolling self-aligning torque. Ignore tiny physics
    // speed fluctuations, then smoothly restore the usual forces by 5 km/h.
    let rolling = ((v - 0.5) / 4.5).clamp(0.0, 1.0);
    let rolling = rolling * rolling * (3.0 - 2.0 * rolling);
    // At road speed, power steering gives the driver a firmer sense of direction.
    // Keep parking and town-speed forces familiar while separating 70 km/h from 10 km/h.
    let road_speed = ((v - 20.0) / 50.0).clamp(0.0, 1.0);
    // The tyres' aligning torque is their side force times the pneumatic trail, and the side
    // force grows with the speed through the turn: crawling round a corner it is the caster
    // alone that brings the wheel back, slowly - the bus driver lets the wheel slide back
    // through the hands. Firmer only as the speed builds (#2103: a strong pull at town speed
    // snapped the wheel back as if the tyres had no grip).
    let spring_strength = (0.12 + 0.38 * v / (v + 20.0)) * (1.0 + 0.5 * road_speed);
    let moving_steering_gain = 1.0 + 0.18 * v / (v + 8.0);
    // More return torque at small and medium angles, where wheel friction can
    // otherwise stop the return. Compensate wheel friction near the centre with
    // a smooth extra torque, tapered away at larger angles. It crosses zero
    // continuously so there is no fixed kick when the wheel passes the centre.
    let centre_return = 0.025 * x / (x * x + 0.004 * 0.004).sqrt() / (1.0 + (x / 0.12).powi(4));
    // The tyre cannot self-align the steering while the bus is stationary. Preserve
    // parking drag against wheel movement, and bring the spring back as it rolls.
    let spring = -(spring_strength * x / (0.5 + 1.15 * x.abs()) + centre_return) * rolling;
    let road_align = -(f.lateral_accel / 9.81).clamp(-0.45, 0.45) * 0.25 * (v / 5.0).clamp(0.0, 1.0) * rolling;
    // Assisted steering should not demand ever more hand force near full lock.
    let lock_assist = 1.0 / (1.0 + 0.55 * x * x);
    let turning_speed = ((x - x0) / dt).clamp(-4.0, 4.0);
    // Without a steering-column torque sensor, motion away from the centre is our
    // indication that the driver is actively turning. Assist that motion, but keep
    // the full self-aligning torque when the wheel is held or let go.
    let turning_out = (x * turning_speed * 2.0).clamp(0.0, 1.0);
    let assist = 1.0 - (0.4 - 0.16 * (v / 80.0).min(1.0)) * turning_out;
    let parking_drag = 0.018 + 0.12 / (1.0 + (v / 6.0).powi(2));
    // Once rolling, reduce drag on return so it does not cancel the centring
    // force. At a standstill, resist motion equally in either direction.
    let returning = x * turning_speed < 0.0;
    let drag = -turning_speed * parking_drag * if returning { 1.0 - 0.8 * rolling } else { 1.0 };
    // Keep some damping on return as well as when turning out. Reducing all
    // resistance on return lets a quick wheel overshoot and oscillate around
    // the centre, especially when force commands arrive at a low frame rate.
    // This only resists motion: it adds no holding force or parked centring.
    // A wheel let go comes back through the steering box's hydraulic damping: more of it
    // on the way back, so that it settles at the centre instead of swinging past (#2103).
    // (only on a wheel coming back quickly, fading out across the centre: the last slow
    // degrees of a return are left to the centring, which a wheel's own friction would
    // otherwise stop short)
    let back = ((0.03 - x * turning_speed.signum()) / 0.06).clamp(0.0, 1.0);
    let fast_back = 0.35 * (turning_speed.abs() - 0.15).max(0.0) * turning_speed.signum() * back * back * (3.0 - 2.0 * back);
    let damping = -(0.03 * turning_speed + fast_back) * rolling;
    *t += dt;
    let period = (f.vib_period * 0.01).max(0.02);
    let shake = f.vib_amp.clamp(0.0, 1.0) * VIB_SHARE * (std::f32::consts::TAU * *t / period).sin();
    // Preserve small road details while softening kerb-sized peaks. One short
    // kick and rebound feels less like a continuously shaking wheel mount.
    let bump = f.wheel_bump.clamp(0.0, 1.0).sqrt() * 0.46 * (std::f32::consts::TAU * f.wheel_bump_age * 6.5).cos();
    let tune = tune.clamped();
    let mut steering = ((spring + road_align) * tune.centring * lock_assist * assist + drag * tune.parking) * moving_steering_gain + damping * tune.damping;
    // A belt- or gear-driven wheel loses the first few per cent of its force in its own
    // friction, so the last degrees of a return and the light pull of a gentle curve never
    // reach the hands. Lift every force past that by `min_force`, eased in over the first
    // 2 % so that a wheel held still at the centre is told nothing and does not chatter.
    if tune.min_force > 0.0 {
        let k = (steering.abs() / 0.02).clamp(0.0, 1.0);
        steering += tune.min_force * steering.signum() * k * k * (3.0 - 2.0 * k);
    }
    // A wheel that buzzes on a force changing at the frame rate is handed the steering part
    // through a low-pass of its own; the vibrations stay sharp, since they are the point.
    steering = if tune.smoothing > 0.0 {
        *smoothed + (steering - *smoothed) * (1.0 - (-dt / tune.smoothing).exp())
    } else {
        steering
    };
    *smoothed = steering;
    let micro = f.micro.clamp(-1.0, 1.0);
    (steering * k_springs.clamp(0.0, 2.0) + (shake + bump + micro) * k_effects.clamp(0.0, 2.0)).clamp(-1.0, 1.0)
}

#[cfg(test)]
#[path = "ffb_tests.rs"]
mod ffb_tests;
