//! A pantograph on a contact wire: the head's travel, the wire that stops it, the force it
//! presses with, the contact it loses where the wire is uneven, and the arc that burns when a
//! current is broken.
//!
//! The model the game draws is only an animation of one script variable (0 down, 1 raised);
//! what this adds is the part Omsi.exe never had: a raised pantograph rises *until it meets
//! the wire* and no further, presses it with its static force, and has current only while the
//! head lies on a live wire.
//!
//! * The **shape** is the head's height over the vehicle's origin at each travel of the
//!   animation, measured on the model itself (the vehicle works it out from the pantograph's
//!   meshes and their animations).
//! * While the script's variable still rises (or falls) the head is **driven**: it goes where
//!   the animation takes it, but never through the wire.
//! * Once raised the head is **free**: a mass on the pantograph's spring, which pushes it
//!   up with the static force (the pantograph's characteristic is nearly flat, 60 to 90 N),
//!   against the wire, which gives like a spring (an elasticity of 0.3 to 0.5 mm per N) and
//!   is not smooth: its hangers and masts, its wear and its ice make it wave under the head.
//!   Where the wire rises faster than the spring can lift the head, the head leaves it.
//! * A broken current burns an arc: `L I^2 / 2` of the motor circuit's inductance, or the
//!   voltage across a few milliseconds of arc when the line is higher.

use crate::rig::{Spark, SparkKind};
use glam::DVec3;

/// What the pantograph presses the wire with (N), the reduced mass of its head and frame
/// (kg) and the damping of its frame (N s/m).
const STATIC_FORCE: f32 = 70.0;
const HEAD_MASS: f32 = 9.0;
const HEAD_DAMPING: f32 = 45.0;
/// The wire's stiffness under the head (N/m): 1 / (0.4 mm per N) at the middle of a span.
const WIRE_STIFFNESS: f32 = 2500.0;
/// ... and its damping (N s/m), so that the head and the wire do not ring.
const WIRE_DAMPING: f32 = 25.0;
/// The animation counts as fully raised from here: the pantograph's own spring takes over.
const RAISED_FROM: f32 = 0.985;
/// Inductance of the motor circuit (H) and the arc's voltage (V), as the poles' (see `rig`).
const LOOP_HENRY: f32 = 0.012;
/// A contact open for less than this (s) is bridged by the arc and the drive's input filter:
/// the current does not stop, and the vehicle keeps its power.
const RIDE_THROUGH: f32 = 0.06;
/// Largest step of the head's integration (s).
const SUB: f32 = 0.002;
/// How fast the mechanism lifts or lowers the head while the animation drives it (m/s).
const DRIVE_SPEED: f32 = 1.5;

/// The head's height over the vehicle's origin (m) at `n` equal steps of the animation's
/// travel, 0 (down) to 1 (raised), and where the head is over the roof.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PantoShape {
    pub heights: Vec<f32>,
    /// The head's place on the roof (model frame: x right, y forward, metres) at full travel.
    pub head: [f32; 2],
    /// Half the length of the head's sliding strip across the track (m).
    pub half_width: f32,
}

impl PantoShape {
    /// A shape needs a travel worth the name: a pantograph that moves less than this is a
    /// mesh that was taken for one.
    pub fn is_usable(&self) -> bool {
        self.heights.len() >= 2 && self.top() - self.bottom() > 0.2
    }

    pub fn bottom(&self) -> f32 {
        self.heights.first().copied().unwrap_or(0.0)
    }

    /// The highest the head goes.
    pub fn top(&self) -> f32 {
        self.heights.iter().copied().fold(f32::MIN, f32::max)
    }

    /// The head's height at travel `t`, between the samples.
    pub fn height_at(&self, t: f32) -> f32 {
        let n = self.heights.len();
        if n == 0 {
            return 0.0;
        }
        let f = t.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (f.floor() as usize).min(n - 1);
        let j = (i + 1).min(n - 1);
        self.heights[i] + (self.heights[j] - self.heights[i]) * (f - i as f32)
    }

    /// The least travel at which the head reaches height `h`: `None` where it never does.
    pub fn travel_for(&self, h: f32) -> Option<f32> {
        let n = self.heights.len();
        if n < 2 {
            return None;
        }
        if h <= self.heights[0] {
            return Some(0.0);
        }
        for i in 1..n {
            let (a, b) = (self.heights[i - 1], self.heights[i]);
            if b >= h {
                let k = if b > a { (h - a) / (b - a) } else { 1.0 };
                return Some(((i - 1) as f32 + k) / (n - 1) as f32);
            }
        }
        None
    }
}

/// The wire above the head, as the track says: heights are those over the vehicle's origin
/// along its own up axis (the vehicle works that out from the wire's place in the world).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WireAbove {
    /// The wire's height over the origin plane (m), without its unevenness.
    pub height: f32,
    /// How far the wire lies to the right of the head's middle (m).
    pub lateral: f32,
    /// The section is fed.
    pub live: bool,
    /// How hard the wire is at this place, 0 to 1: a crossing, a switch's frog, a section
    /// insulator, a hard point at a mast.
    pub hard: f32,
    /// How uneven the wire is, 1 for a wire in good order, 2 and more for a worn or iced one.
    pub roughness: f32,
}

impl Default for WireAbove {
    fn default() -> Self {
        WireAbove { height: 5.5, lateral: 0.0, live: true, hard: 0.0, roughness: 1.0 }
    }
}

pub struct PantoIn<'a> {
    pub dt: f32,
    pub shape: &'a PantoShape,
    /// The script's variable, 0 to 1: what the pantograph is told to be.
    pub cmd: f32,
    /// The wire above the head, or `None` where the map has none (the pantograph then goes
    /// to its stop).
    pub wire: Option<WireAbove>,
    /// Distance the vehicle has run along the track (m): the wire's unevenness is laid along
    /// it, so that a standing pantograph meets the same wire and a running one the wire as
    /// it passes.
    pub run: f64,
    pub speed: f32,
    /// Current drawn through the pantograph (A) and the line's voltage (V).
    pub amps: f32,
    pub volts: f32,
    /// Where the head is in the world (spark light and place), for the arcs.
    pub head_at: DVec3,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PantoOut {
    /// The animation's variable to show: the commanded one, held at the wire.
    pub shown: f32,
    /// The head lies on the wire (the circuit is made).
    pub contact: bool,
    /// The vehicle has current: the contact, or an arc and the filter bridging a short break.
    pub powered: bool,
    /// What the head presses the wire with (N).
    pub force: f32,
    /// The head's height over the origin (m).
    pub height: f32,
    /// How hard an arc burns now, 0 to 1.
    pub arc: f32,
}

#[derive(Debug, Clone)]
pub struct Pantograph {
    x: f32,
    v: f32,
    started: bool,
    free: bool,
    contact: bool,
    open_for: f32,
    force: f32,
    arc: f32,
    amps_seen: f32,
    sparks: Vec<Spark>,
    rng: u64,
    /// Contact losses so far (for tests and logs).
    pub losses: u32,
    phase: [f64; 3],
    /// The last output.
    pub out: PantoOut,
}

impl Pantograph {
    pub fn new(seed: u64) -> Pantograph {
        let mut p = Pantograph {
            x: 0.0,
            v: 0.0,
            started: false,
            free: false,
            contact: false,
            open_for: 0.0,
            force: 0.0,
            arc: 0.0,
            amps_seen: 0.0,
            sparks: Vec::new(),
            rng: seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1,
            losses: 0,
            phase: [0.0; 3],
            out: PantoOut::default(),
        };
        for k in 0..3 {
            p.phase[k] = p.rand() as f64 * std::f64::consts::TAU;
        }
        p
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        ((self.rng >> 40) as f32) / (1u32 << 24) as f32
    }

    /// The wire's unevenness at `run` metres along the track (m, positive up): the droppers
    /// every 8 m, the span between masts (40 m), and the roughness of wear, scaled by the
    /// wire's own `roughness` and a hard point's bump. A wire of good order is a few
    /// millimetres out of true; the pantograph follows that without losing it up to 60 km/h.
    pub fn wire_bumps(&self, run: f64, wire: &WireAbove) -> f32 {
        let q = wire.roughness.clamp(0.0, 6.0);
        let tau = std::f64::consts::TAU;
        let dropper = (run * tau / 8.0 + self.phase[0]).sin() as f32 * 0.0025;
        let span = (run * tau / 40.0 + self.phase[1]).sin() as f32 * 0.005;
        let wear = ((run * tau / 2.3 + self.phase[2]).sin() * (run * tau / 5.9).cos()) as f32 * 0.0035;
        (dropper + span + wear) * q + wire.hard.clamp(0.0, 1.0) * 0.016
    }

    /// One step.
    pub fn step(&mut self, i: &PantoIn) -> PantoOut {
        let shape = i.shape;
        if !self.started {
            // (a pantograph begins lowered)
            self.started = true;
            self.x = shape.bottom();
        }
        let top = shape.top();
        let cmd = i.cmd.clamp(0.0, 1.0);
        let dt = i.dt.clamp(0.0, 0.1);
        let kinematic = cmd < RAISED_FROM;
        // the wire the head can meet: in reach, under its strip, and fed or not
        let wire = i.wire.filter(|w| w.height <= top + 0.02 && w.lateral.abs() <= shape.half_width.max(0.2));
        let was_contact = self.contact;
        let mut remaining = dt;
        let mut contact_acc = 0.0;
        let mut steps = 0.0;
        let mut force_acc = 0.0;
        let mut run = i.run;
        while remaining > 1e-6 {
            let h = remaining.min(SUB);
            remaining -= h;
            run += (i.speed * h) as f64;
            let w = wire.map(|w| w.height + self.wire_bumps(run, &w));
            if kinematic {
                // driven by the animation: where it takes the head, but not through the wire
                let free_at = shape.height_at(cmd);
                // (and pressing it down by what its static force bends it)
                let (target, hit) = match w {
                    Some(w) if free_at >= w => (free_at.min(w + STATIC_FORCE / WIRE_STIFFNESS), true),
                    _ => (free_at, false),
                };
                // (the mechanism has a speed: a pantograph does not jump)
                let x = self.x + (target - self.x).clamp(-DRIVE_SPEED * h, DRIVE_SPEED * h);
                self.v = (x - self.x) / h.max(1e-4);
                self.x = x;
                self.free = false;
                self.contact = hit && w.is_some_and(|w| x >= w);
                force_acc += if self.contact { STATIC_FORCE } else { 0.0 };
            } else {
                if !self.free {
                    // the animation is done: the pantograph's spring takes over from where the
                    // animation left the head
                    self.free = true;
                    self.v = 0.0;
                }
                // the spring lifts the head with the static force until the stop; the wire
                // pushes it down like a spring if the head is above it
                let lift = if self.x < top { STATIC_FORCE } else { 0.0 };
                let (mut wire_f, mut touching) = (0.0, false);
                if let Some(w) = w {
                    let pen = self.x - w;
                    if pen > 0.0 {
                        wire_f = (WIRE_STIFFNESS * pen + WIRE_DAMPING * self.v.max(0.0)).max(0.0);
                        touching = true;
                    }
                }
                let a = (lift - wire_f - HEAD_DAMPING * self.v) / HEAD_MASS;
                self.v += a * h;
                self.x += self.v * h;
                if self.x >= top {
                    self.x = top;
                    self.v = self.v.min(0.0);
                }
                let bottom = shape.bottom();
                if self.x < bottom {
                    self.x = bottom;
                    self.v = self.v.max(0.0);
                }
                self.contact = touching;
                force_acc += wire_f;
            }
            contact_acc += if self.contact { 1.0 } else { 0.0 };
            steps += 1.0;
        }
        let contact_share = if steps > 0.0 { contact_acc / steps } else { 0.0 };
        self.force = if steps > 0.0 { force_acc / steps } else { self.force };
        // (the step's contact is made if it was made for most of it: a step of a frame is many
        // integration steps, and a break of a few milliseconds is a break all the same)
        let contact = wire.is_some() && (contact_share > 0.5 || (self.contact && contact_share > 0.0));
        self.contact = contact;
        let live = i.wire.is_none_or(|w| w.live);
        let amps = i.amps.abs();
        self.amps_seen = amps;
        // the circuit: the contact, or an arc and the filter bridging a short break
        if contact {
            self.open_for = 0.0;
        } else {
            self.open_for += dt;
        }
        let bridged = !contact && self.open_for < RIDE_THROUGH && self.out.powered;
        let powered = if i.wire.is_none() { !kinematic } else { live && (contact || bridged) };
        // arcs: the current broken, made again, or a little of it all the time
        let at = i.head_at;
        let burn = |amps: f32, volts: f32| -> f32 {
            let l = 0.5 * LOOP_HENRY * amps * amps;
            let v = 0.5 * volts.max(0.0) * amps * 0.004;
            l.max(v)
        };
        if was_contact && !contact && amps > 15.0 && wire.is_some() {
            self.losses += 1;
            let e = burn(amps, i.volts);
            self.push_spark(Spark { pos: at, energy: e, amps, kind: SparkKind::Break });
            self.arc = (amps / 300.0).clamp(0.15, 1.0);
        } else if !was_contact && contact && amps > 15.0 {
            let e = 0.15 * burn(amps, i.volts) + 1.0;
            self.push_spark(Spark { pos: at, energy: e, amps, kind: SparkKind::Make });
        } else if was_contact && !contact && wire.is_some() {
            self.losses += 1;
        }
        if contact && amps > 40.0 && i.speed > 2.0 && dt > 0.0 {
            // the strip skates over the wire under load: now and then it throws a spark, more
            // with the current, the speed and a wire that is worn
            let rough = wire.map_or(1.0, |w| w.roughness.max(0.2) + 2.0 * w.hard);
            let rate = (amps / 200.0).min(3.0) * (i.speed / 10.0).min(2.5) * rough * 0.35;
            if self.rand() < rate * dt {
                let e = 1.5 + 6.0 * self.rand() * (amps / 250.0).min(2.0);
                self.push_spark(Spark { pos: at, energy: e, amps, kind: SparkKind::Make });
            }
        }
        if contact || !(self.open_for < RIDE_THROUGH) {
            self.arc = (self.arc - dt / 0.12).max(0.0);
        }
        let shown = if self.x >= top - 1e-4 { 1.0 } else { shape.travel_for(self.x).unwrap_or(1.0).min(1.0) };
        self.out = PantoOut { shown, contact, powered, force: self.force, height: self.x, arc: self.arc };
        self.out
    }

    fn push_spark(&mut self, s: Spark) {
        if self.sparks.len() < 48 {
            self.sparks.push(s);
        }
    }

    /// The arcs struck since the last call.
    pub fn take_sparks(&mut self) -> Vec<Spark> {
        std::mem::take(&mut self.sparks)
    }

    /// Back to a lowered pantograph (the vehicle was put somewhere else).
    pub fn reset(&mut self, bottom: f32) {
        self.started = true;
        self.x = bottom;
        self.v = 0.0;
        self.free = false;
        self.contact = false;
        self.open_for = 0.0;
        self.arc = 0.0;
        self.sparks.clear();
        self.out = PantoOut::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pantograph that lifts its head from 3.4 m (folded) to 5.9 m (full travel) in a
    /// slightly curved law, as a scissor arm does.
    fn shape() -> PantoShape {
        PantoShape { heights: (0..=32).map(|i| 3.4 + 2.5 * ((i as f32 / 32.0).powf(0.9))).collect(), head: [0.0, -1.0], half_width: 0.55 }
    }

    /// The script's variable rises 0 -> 1 over three seconds and the pantograph is up.
    fn raise(p: &mut Pantograph, wire: Option<WireAbove>) -> PantoOut {
        let s = shape();
        let mut out = PantoOut::default();
        for k in 0..450 {
            let cmd = (k as f32 * 0.01 / 3.0).min(1.0);
            out = p.step(&PantoIn { dt: 0.01, shape: &s, cmd, wire, run: 0.0, speed: 0.0, amps: 0.0, volts: 600.0, head_at: DVec3::ZERO });
        }
        p.losses = 0;
        out
    }

    fn run(p: &mut Pantograph, wire: Option<WireAbove>, cmd: f32, secs: f32, speed: f32, amps: f32) -> PantoOut {
        let s = shape();
        let mut out = PantoOut::default();
        let mut run = 0.0f64;
        for _ in 0..(secs / 0.01) as usize {
            out = p.step(&PantoIn { dt: 0.01, shape: &s, cmd, wire, run, speed, amps, volts: 600.0, head_at: DVec3::ZERO });
            run += (speed * 0.01) as f64;
        }
        out
    }

    #[test]
    fn the_shape_is_read_both_ways() {
        let s = shape();
        assert!(s.is_usable());
        assert!((s.height_at(0.0) - 3.4).abs() < 1e-4 && (s.height_at(1.0) - 5.9).abs() < 1e-3);
        let t = s.travel_for(4.7).unwrap();
        assert!((s.height_at(t) - 4.7).abs() < 0.01);
        assert!(s.travel_for(6.5).is_none());
        assert_eq!(s.travel_for(3.0), Some(0.0));
    }

    #[test]
    fn a_raised_pantograph_stops_at_the_wire_and_does_not_grow_beyond_it() {
        let mut p = Pantograph::new(1);
        let wire = WireAbove { height: 4.7, ..Default::default() };
        // the script's variable rises 0 -> 1 over three seconds
        let s = shape();
        let mut worst: f32 = 0.0;
        let mut out = PantoOut::default();
        for k in 0..400 {
            let cmd = (k as f32 * 0.01 / 3.0).min(1.0);
            out = p.step(&PantoIn { dt: 0.01, shape: &s, cmd, wire: Some(wire), run: 0.0, speed: 0.0, amps: 0.0, volts: 600.0, head_at: DVec3::ZERO });
            worst = worst.max(out.height);
        }
        assert!(out.contact && out.powered, "{out:?}");
        // it presses the wire down by its static force over the wire's stiffness: 2.8 cm
        assert!(worst < 4.7 + 0.06, "the head went up to {worst} m through a wire at 4.7 m");
        assert!(out.height > 4.7, "{}", out.height);
        // what the model shows is the travel at which the head is there, not the full travel
        assert!(out.shown < 0.8 && out.shown > 0.4, "{}", out.shown);
        assert!((out.force - STATIC_FORCE).abs() < 8.0, "{}", out.force);
    }

    #[test]
    fn a_wire_out_of_reach_leaves_the_pantograph_at_its_stop_without_current() {
        let mut p = Pantograph::new(2);
        let out = run(&mut p, Some(WireAbove { height: 6.4, ..Default::default() }), 1.0, 4.0, 0.0, 0.0);
        assert!(!out.contact && !out.powered);
        assert_eq!(out.shown, 1.0);
        assert!((out.height - 5.9).abs() < 1e-3);
    }

    #[test]
    fn a_lowered_pantograph_has_no_current_and_one_under_a_dead_wire_neither() {
        let mut p = Pantograph::new(3);
        let wire = WireAbove { height: 4.7, ..Default::default() };
        assert!(!run(&mut p, Some(wire), 0.0, 1.0, 0.0, 0.0).powered);
        let out = run(&mut p, Some(WireAbove { live: false, ..wire }), 1.0, 4.0, 0.0, 0.0);
        assert!(out.contact && !out.powered, "on a dead wire: contact but no current: {out:?}");
        let out = run(&mut p, Some(WireAbove { live: true, ..wire }), 1.0, 1.0, 0.0, 0.0);
        assert!(out.powered);
    }

    #[test]
    fn without_a_wire_in_the_map_the_pantograph_goes_up_to_its_stop_and_keeps_its_power() {
        let mut p = Pantograph::new(4);
        let out = run(&mut p, None, 1.0, 4.0, 5.0, 0.0);
        assert!(out.powered && !out.contact);
        assert_eq!(out.shown, 1.0);
        let down = run(&mut p, None, 0.0, 3.0, 5.0, 0.0);
        assert!(!down.powered, "lowered it has no power wherever it is");
    }

    #[test]
    fn lowering_the_pantograph_under_current_strikes_an_arc() {
        let mut p = Pantograph::new(5);
        let wire = WireAbove { height: 4.7, ..Default::default() };
        raise(&mut p, Some(wire));
        p.take_sparks();
        run(&mut p, Some(wire), 0.3, 0.5, 0.0, 250.0);
        let sparks = p.take_sparks();
        assert!(sparks.iter().any(|s| s.kind == SparkKind::Break && s.energy > 100.0), "{sparks:?}");
        // the same without current is quiet
        let mut q = Pantograph::new(5);
        raise(&mut q, Some(wire));
        q.take_sparks();
        run(&mut q, Some(wire), 0.3, 0.5, 0.0, 0.0);
        assert!(q.take_sparks().iter().all(|s| s.kind != SparkKind::Break));
    }

    #[test]
    fn a_rough_wire_at_speed_loses_the_contact_and_a_good_one_at_walking_pace_does_not() {
        let mut worn = Pantograph::new(6);
        let bad = WireAbove { height: 4.7, roughness: 3.5, ..Default::default() };
        raise(&mut worn, Some(bad));
        run(&mut worn, Some(bad), 1.0, 40.0, 20.0, 200.0);
        assert!(worn.losses > 0, "a worn wire at 72 km/h under 200 A must lose the contact now and then");
        let mut good = Pantograph::new(6);
        let ok = WireAbove { height: 4.7, roughness: 1.0, ..Default::default() };
        raise(&mut good, Some(ok));
        run(&mut good, Some(ok), 1.0, 40.0, 3.0, 200.0);
        assert_eq!(good.losses, 0, "a wire in order at 11 km/h is never lost");
    }

    #[test]
    fn a_short_loss_of_contact_does_not_cut_the_power() {
        let mut p = Pantograph::new(7);
        let wire = WireAbove { height: 4.7, roughness: 4.0, hard: 0.0, ..Default::default() };
        raise(&mut p, Some(wire));
        let s = shape();
        let mut dark = 0;
        let mut run_s = 0.0f64;
        for _ in 0..4000 {
            let out = p.step(&PantoIn { dt: 0.01, shape: &s, cmd: 1.0, wire: Some(wire), run: run_s, speed: 22.0, amps: 150.0, volts: 600.0, head_at: DVec3::ZERO });
            run_s += 0.22;
            if !out.powered {
                dark += 1;
            }
        }
        assert!(p.losses > 0);
        assert!(dark < 80, "the power went off for {dark} of 4000 steps");
    }

    #[test]
    fn the_wire_beside_the_strip_is_not_touched() {
        let mut p = Pantograph::new(8);
        let out = run(&mut p, Some(WireAbove { height: 4.7, lateral: 1.4, ..Default::default() }), 1.0, 4.0, 0.0, 0.0);
        assert!(!out.contact && !out.powered);
    }
}
