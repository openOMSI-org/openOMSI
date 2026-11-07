//! The trolley gear of one vehicle: its poles, its traction, and what they do to each other
//! and to the wire: the circuit through the poles, the current drawn, the switches its shoes
//! set, and the arcs where the current is broken.

use crate::def::TrolleyDef;
use crate::pole::{rotate_tip_pub, DewireCause, Pole, PoleEvent, PoleEventKind, PoleMode, PoseSpan, Rope, G};
use crate::traction::{Traction, TractionIn, TractionOut};
use crate::wire::{ArcRule, Contact, SwitchRequest, WireNet};
use glam::{DMat3, DVec3, Vec3};

/// Inductance of the motor circuit and its choke (H): the energy `L I^2 / 2` of the current
/// is what an arc has to burn when it is broken.
const LOOP_HENRY: f32 = 0.012;
/// Voltage of an arc between metal and metal (V).
const ARC_VOLTS: f32 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RopeCmd {
    Idle,
    Pull,
    /// Raise and guide: along the player's line of sight (origin, direction) to the wire it
    /// meets, or by itself (a motor) to the wire the boom reaches.
    Raise { ray: Option<(DVec3, DVec3)>, auto: bool },
    /// Held by the rope at this elevation and swing (radians, the bus's frame), with no help to
    /// the wire: let go (`Idle`), it rises on its spring from where it was held.
    Hold { theta: f64, psi: f64 },
    /// Let go of a pole held by hand: it is lifted by its spring, and swung on to the nearest wire
    /// above its tip as it rises.
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparkKind {
    /// The shoe lifted off a wire carrying current.
    Break,
    /// It came back.
    Make,
    /// A pole pulled off, or thrown off, under current.
    Dewire,
    /// An insulator crossed under current.
    Insulator,
    /// A frog crossed under current.
    Frog,
}

/// An arc to show: where, how much energy it burns (J) and the current through it.
#[derive(Debug, Clone, Copy)]
pub struct Spark {
    pub pos: DVec3,
    pub energy: f32,
    pub amps: f32,
    pub kind: SparkKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RigSound {
    /// A contactor of the controller.
    Click,
    /// Pole `pole`'s shoe falling back on the wire after it bounced off it (loudness 0..1).
    ShoeHit { pole: usize, loud: f32 },
    /// It came off the wire (by the rope, or thrown off).
    OffWire { pole: usize },
    /// An arc's crack (loudness 0..1).
    Snap(f32),
    /// Pole `pole`'s shoe through a frog.
    Frog { pole: usize },
    /// Its shoe over a section insulator.
    Insulator { pole: usize },
    /// Thrown off the wire (a bounce, a kink, the bus too far aside), not pulled off by the rope.
    Dewire { pole: usize },
    /// The loose boom brought up short by its rope at `max_angle` (loudness 0..1).
    RopeStop { pole: usize, loud: f32 },
    /// Its hook lets it go.
    Release { pole: usize },
    /// Pole `pole` (from 0) brought down on to its hook, which catches it.
    Latch { pole: usize },
    /// Its shoe put on the wire.
    OnWire { pole: usize },
}

pub struct RigInput<'a> {
    pub net: Option<&'a WireNet>,
    pub dt: f32,
    pub pos: DVec3,
    pub rot: DMat3,
    /// The roof's acceleration in the bus frame.
    pub accel: Vec3,
    /// Speed along the heading (m/s).
    pub speed: f32,
    pub wheel_radius: f32,
    pub pedal: f32,
    pub brake: f32,
    pub dir: f32,
    /// -1 left indicator, +1 right, 0 none.
    pub blinker: i8,
    /// For radio switches: the cab's request (-1 left, +1 right, 2 toggle, 0 none), the route code
    /// (0 none) and the channel sent on.
    pub radio: i8,
    pub route_code: u32,
    pub channel: u32,
    /// One per pole.
    pub ropes: &'a [RopeCmd],
    /// A modelled drive supplies the force; without one the script's torque is only gated.
    pub drive: bool,
    /// Without a modelled drive: the driving force the script asks of the wheels (N).
    pub script_force: f32,
}

pub struct TrolleyRig {
    pub def: TrolleyDef,
    pub poles: Vec<Pole>,
    pub traction: Option<Traction>,
    pub out: TractionOut,
    last: Option<(DVec3, DMat3)>,
    /// Voltage at the vehicle and the current it draws (A, negative: given back).
    pub volts: f32,
    pub amps: f32,
    /// Both poles are on live wires of opposite polarity.
    pub circuit: bool,
    /// What a scripted drive's torque is scaled by: 1 with a good circuit, 0 without.
    pub script_gate: f32,
    pub sparks: Vec<Spark>,
    pub sounds: Vec<RigSound>,
    /// How loud the wire hisses at the shoes (0..1) and where, and how hard an arc is
    /// burning now (0..1).
    pub hiss: f32,
    pub hiss_pos: DVec3,
    pub arc: f32,
    /// The picture of the last step's events, for a HUD line.
    pub last_event: Option<(PoleEventKind, usize)>,
    /// Short messages for the driver about what happened to the poles (taken by the game).
    pub notes: Vec<String>,
    /// Every event of the poles goes into the notes (for logs).
    pub verbose: bool,
    events: Vec<PoleEvent>,
    radio_previous: Option<(i8, u32)>,
    /// Per pole, how hard (0..1) its last hit was (on the wire, back on it, against its rope).
    pub hit_level: Vec<f32>,
    /// Where the vehicle was last step while sending a route code.
    route_from: Option<DVec3>,
    rng: u64,
}

/// The point a ray reaches at `len` metres from `base` (the farthest, where it crosses that
/// sphere twice; the nearest approach pushed out to `len` when it misses): where a hand
/// pointing along the ray holds a pole's tip.
fn free_aim(o: DVec3, d: DVec3, base: DVec3, len: f64) -> DVec3 {
    let d = d.normalize_or_zero();
    let oc = o - base;
    let b = d.dot(oc);
    let c = oc.length_squared() - len * len;
    let disc = b * b - c;
    let p = if disc >= 0.0 { o + d * (-b + disc.sqrt()) } else { o + d * (-b).max(0.0) };
    base + (p - base).normalize_or_zero() * len
}

impl TrolleyRig {
    pub fn new(def: &TrolleyDef, seed: u64) -> TrolleyRig {
        let poles = def.poles.iter().enumerate().map(|(i, p)| Pole::new(p.clone(), seed.wrapping_add(i as u64 * 977))).collect();
        let traction = def.drive.as_ref().map(|d| Traction::new(d, def.battery.clone()));
        TrolleyRig {
            def: def.clone(),
            poles,
            traction,
            out: TractionOut::default(),
            last: None,
            volts: 0.0,
            amps: 0.0,
            circuit: false,
            script_gate: 0.0,
            sparks: Vec::new(),
            sounds: Vec::new(),
            hiss: 0.0,
            hiss_pos: DVec3::ZERO,
            arc: 0.0,
            last_event: None,
            notes: Vec::new(),
            verbose: false,
            events: Vec::new(),
            radio_previous: None,
            hit_level: vec![0.0; def.poles.len()],
            route_from: None,
            rng: seed.wrapping_add(0x5bd1_e995).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Whether a crossing with `amps` arcs under the map's `rule` (no draw at chance 1).
    fn strikes(&mut self, rule: ArcRule, amps: f32) -> bool {
        amps > rule.min_amps && (rule.chance >= 1.0 || self.rand() < rule.chance)
    }

    /// The wire side a pole of `side` takes where the span's tangent is `t`, for a vehicle
    /// heading `fwd`: the right pole takes the wire on the right of the way it runs.
    fn wanted_side(side: i8, t: DVec3, fwd: DVec3) -> u8 {
        let same = t.dot(fwd) >= 0.0;
        if (side > 0) == same {
            0
        } else {
            1
        }
    }

    fn goal_for(&self, k: usize, cmd: RopeCmd, net: &WireNet, pos: DVec3, rot: &DMat3) -> Option<Contact> {
        let RopeCmd::Raise { ray, auto } = cmd else { return None };
        let pole = &self.poles[k];
        let base = pole.base_at(pos, rot);
        let fwd = *rot * DVec3::Y;
        let side = pole.def.side;
        let reach = pole.def.length as f64;
        if let Some((o, d)) = ray {
            let hit = net.aim_wire(o, d, base, reach, &|t| Some(Self::wanted_side(side, t, fwd)), 2.2);
            if hit.is_some() {
                return hit.map(|h| h.0);
            }
        }
        // (a pole the motor has in hand or has just let go is carried to the wire nearest its own
        // tip; one not raised yet, or a hand's whose look found no wire, to the wire above its
        // working angle)
        let held = auto && matches!(pole.mode, PoleMode::Hand | PoleMode::Free) && pole.tip.length_squared() > 1.0;
        let (tip, radius) = if held { (pole.tip, 4.5) } else { (base + *rot * rotate_tip_pub((pole.def.work_angle as f64).to_radians(), 0.0, reach), 3.5) };
        net.candidates(tip, radius)
            .into_iter()
            .filter(|(c, _)| {
                let (_, t) = net.wire_point(c);
                c.side == Self::wanted_side(side, t, fwd)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|x| x.0)
    }

    /// Advance one step.
    pub fn step(&mut self, inp: &RigInput) -> &TractionOut {
        let dt = inp.dt.max(1e-4) as f64;
        let (p0, r0) = self.last.unwrap_or((inp.pos, inp.rot));
        let span = PoseSpan { pos0: p0, rot0: r0, pos1: inp.pos, rot1: inp.rot, accel: inp.accel };
        self.last = Some((inp.pos, inp.rot));
        self.events.clear();
        static EMPTY: std::sync::OnceLock<WireNet> = std::sync::OnceLock::new();
        let net = inp.net.unwrap_or_else(|| EMPTY.get_or_init(|| WireNet::parse("")));
        let request = SwitchRequest { current: self.amps - self.out.charge_amps, blinker: inp.blinker, radio: inp.radio, code: inp.route_code, channel: inp.channel };
        // (a release sends nothing: it would hand a toggled or chosen switch back to the route)
        let transmit = self.radio_previous.is_some_and(|(radio, code)| match inp.radio {
            2 => radio != 2,
            0 => code != inp.route_code,
            direction => (radio, code) != (direction, inp.route_code),
        });
        if transmit {
            net.receive_radio(inp.pos, &request);
        }
        self.radio_previous = Some((inp.radio, inp.route_code));
        // (a route code sets each switch as the bus arrives, as a VETRA-like system does)
        if inp.route_code != 0 {
            if let Some(from) = self.route_from {
                net.receive_route_on_entry(from, inp.pos, inp.route_code, inp.channel);
            }
            self.route_from = Some(inp.pos);
        } else {
            self.route_from = None;
        }
        let amps_before = self.amps;
        for k in 0..self.poles.len() {
            let cmd = inp.ropes.get(k).copied().unwrap_or(RopeCmd::Idle);
            let rope = match cmd {
                RopeCmd::Idle | RopeCmd::Release => Rope::Idle,
                RopeCmd::Pull => Rope::Pull,
                RopeCmd::Hold { theta, psi } => Rope::Hold { theta, psi },
                RopeCmd::Raise { auto, .. } => Rope::Aim { goal: self.goal_for(k, cmd, net, inp.pos, &inp.rot), auto },
            };
            let rope = match (rope, self.poles[k].gear_order()) {
                (Rope::Idle, Some(at)) => Rope::Gear { at, goal: self.goal_for(k, RopeCmd::Raise { ray: None, auto: true }, net, inp.pos, &inp.rot) },
                (Rope::Idle, None) => Rope::Idle,
                (r, _) => {
                    self.poles[k].gear_cancel();
                    r
                }
            };
            let mut ev = std::mem::take(&mut self.events);
            self.poles[k].steer_to = match cmd {
                RopeCmd::Release => {
                    let (tip, side, fwd) = (self.poles[k].tip, self.poles[k].def.side, inp.rot * DVec3::Y);
                    net.candidates(tip, 2.5)
                        .into_iter()
                        .filter(|(c, _)| c.side == Self::wanted_side(side, net.wire_point(c).1, fwd))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|x| net.wire_point(&x.0).0)
                }
                _ => None,
            };
            self.poles[k].free_aim = match cmd {
                RopeCmd::Raise { ray: Some((o, d)), auto: false } => {
                    let base = self.poles[k].base_at(inp.pos, &inp.rot);
                    Some(free_aim(o, d, base, self.poles[k].def.length as f64))
                }
                _ => None,
            };
            self.poles[k].roughness = net.roughness as f64;
            self.poles[k].step(net, &span, dt, rope, amps_before, k, &mut ev);
            self.events = ev;
        }
        self.handle_events(net, inp, amps_before);
        // the circuit: one pole on each wire, both fed
        let mut plus: Option<&Pole> = None;
        let mut minus: Option<&Pole> = None;
        for p in &self.poles {
            if p.mode == PoleMode::OnWire && p.closed && p.on_live {
                if let Some(c) = p.contact {
                    if c.side == 0 {
                        plus = plus.or(Some(p));
                    } else {
                        minus = minus.or(Some(p));
                    }
                }
            }
        }
        self.circuit = plus.is_some() && minus.is_some();
        let nominal = net.nominal_volts.max(1.0);
        let section = plus.map(|p| p.section);
        self.volts = match section {
            Some(s) if self.circuit => net.section_volts(s, 0.0),
            _ => 0.0,
        };
        let accept = section.filter(|_| self.circuit).map_or(0.0, |s| net.regen_capacity_kw(s));
        if let Some(tr) = self.traction.as_mut() {
            let o = tr.step(&TractionIn {
                dt: inp.dt,
                volts: self.volts,
                nominal,
                pedal: inp.pedal,
                brake: inp.brake,
                dir: inp.dir,
                speed: inp.speed,
                wheel_radius: inp.wheel_radius,
                regen_accept_kw: accept,
                aux_kw: self.def.aux_kw,
            });
            self.out = o;
            self.amps = if self.circuit { o.line_amps } else { 0.0 };
            for _ in 0..o.clicks.min(3) {
                self.sounds.push(RigSound::Click);
            }
        } else {
            self.out = TractionOut::default();
            // a script's drive: the line current its force and speed ask for, with the start-up
            // current a rheostat controller draws while it holds the motors back
            let chopper = self.def.legacy.as_ref().is_some_and(|l| l.chopper);
            let power_w = inp.script_force.max(0.0) * inp.speed.abs().max(0.0) / 0.82 + self.def.aux_kw * 1000.0;
            let start = if chopper { 0.0 } else { (inp.script_force * 0.011).min(260.0) * (1.0 - (inp.speed.abs() / 4.0).min(1.0)) };
            let a = if self.circuit && self.volts > 50.0 { power_w / self.volts + start } else { 0.0 };
            self.amps = a;
        }
        self.script_gate = if self.circuit && self.volts >= 0.55 * nominal { (self.volts / nominal).min(1.0) } else { 0.0 };
        if let Some(s) = section.filter(|_| self.circuit) {
            net.add_current(s, self.amps);
        }
        // hiss and arcs
        let mut hiss = 0.0f32;
        let mut pos = DVec3::ZERO;
        let mut n = 0.0;
        let mut arc = 0.0f32;
        for p in &self.poles {
            if p.mode == PoleMode::OnWire {
                let load = 0.12 + 0.88 * (self.amps.abs() / 220.0).min(1.0);
                let moving = (p.slide as f32 / 6.0).min(1.0);
                let h = if p.closed { load * moving } else { 0.0 };
                hiss += h;
                pos += p.tip;
                n += 1.0;
                if !p.closed && self.amps.abs() > 3.0 {
                    arc = arc.max((self.amps.abs() / 250.0).min(1.0));
                }
            }
        }
        self.hiss = (hiss / self.poles.len().max(1) as f32).min(1.0);
        if n > 0.0 {
            self.hiss_pos = pos / n;
        }
        self.arc = arc;
        net.end_step(inp.dt);
        &self.out
    }

    fn handle_events(&mut self, net: &WireNet, inp: &RigInput, amps: f32) {
        let events = std::mem::take(&mut self.events);
        let ind = |a: f32| 0.5 * LOOP_HENRY * a * a;
        for e in &events {
            let a = e.amps.abs();
            let live = a > 3.0;
            self.last_event = Some((e.kind, e.pole));
            if self.verbose {
                self.notes.push(format!("{:?} pole {} at ({:.1}, {:.1}, {:.2}) with {:.0} A", e.kind, e.pole + 1, e.pos.x, e.pos.y, e.pos.z, e.amps));
            }
            match e.kind {
                PoleEventKind::Engaged => {
                    // (how fast the shoe came up into the wire, 2 m/s and more: 1)
                    let p = &self.poles[e.pole];
                    self.hit_level[e.pole] = (p.w_theta.abs() * p.def.length as f64 / 2.0).clamp(0.2, 1.0) as f32;
                    self.sounds.push(RigSound::OnWire { pole: e.pole });
                    self.notes.push(format!("Pole {} is on the wire", e.pole + 1));
                }
                PoleEventKind::Latched => {
                    self.sounds.push(RigSound::Latch { pole: e.pole });
                    self.notes.push(format!("Pole {} is down on its hook", e.pole + 1));
                }
                PoleEventKind::Released => self.sounds.push(RigSound::Release { pole: e.pole }),
                PoleEventKind::ContactOpened => {
                    if live {
                        self.spark(e.pos, 0.5 * ind(a), a, SparkKind::Break);
                    }
                }
                PoleEventKind::ContactClosed { open_s, impact } => {
                    if live {
                        self.spark(e.pos, a * ARC_VOLTS * open_s.max(0.001), a, SparkKind::Make);
                    }
                    if impact > 0.5 {
                        let loud = (impact / 3.0).min(1.0);
                        self.hit_level[e.pole] = loud;
                        self.sounds.push(RigSound::ShoeHit { pole: e.pole, loud });
                    }
                }
                PoleEventKind::Dewired(cause) => {
                    self.notes.push(format!("Pole {} came off the wire: {}", e.pole + 1, cause.describe()));
                    if live {
                        self.spark(e.pos, ind(a) + a * 100.0 * 0.02, a, SparkKind::Dewire);
                    }
                    self.sounds.push(if cause == DewireCause::Rope { RigSound::OffWire { pole: e.pole } } else { RigSound::Dewire { pole: e.pole } });
                }
                PoleEventKind::RopeTaut { speed } => {
                    let loud = (speed / 2.0).min(1.0);
                    self.hit_level[e.pole] = loud;
                    self.sounds.push(RigSound::RopeStop { pole: e.pole, loud });
                }
                PoleEventKind::GapEntered => {
                    self.sounds.push(RigSound::Insulator { pole: e.pole });
                    if self.strikes(net.insulator_arc, a) {
                        self.spark(e.pos, net.insulator_arc.scale * ind(a), a, SparkKind::Insulator);
                    }
                }
                PoleEventKind::GapLeft => {
                    if self.strikes(net.insulator_arc, a) {
                        self.spark(e.pos, net.insulator_arc.scale * a * 60.0 * 0.002, a, SparkKind::Insulator);
                    }
                }
                PoleEventKind::FrogPassed { .. } => {
                    self.sounds.push(RigSound::Frog { pole: e.pole });
                    if self.strikes(net.frog_arc, a) {
                        self.spark(e.pos, net.frog_arc.scale * (0.15 * ind(a) + 0.5 * a), a, SparkKind::Frog);
                    }
                }
                PoleEventKind::Plate { switch } => {
                    let definition = &net.switches[switch as usize];
                    if definition.mode == crate::wire::SwitchMode::Radio {
                        continue;
                    }
                    let request = SwitchRequest { current: amps - self.out.charge_amps, blinker: inp.blinker, radio: inp.radio, code: inp.route_code, channel: inp.channel };
                    net.trigger_switch(switch as usize, &request);
                }
            }
        }
        self.events = events;
    }

    fn spark(&mut self, pos: DVec3, energy: f32, amps: f32, kind: SparkKind) {
        if energy < 0.05 {
            return;
        }
        self.sparks.push(Spark { pos, energy, amps, kind });
        let loud = (energy.sqrt() / 18.0).min(1.0);
        if loud > 0.05 {
            self.sounds.push(RigSound::Snap(loud));
        }
    }

    pub fn take_sparks(&mut self) -> Vec<Spark> {
        std::mem::take(&mut self.sparks)
    }

    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// Where pole `k`'s hinge is on the roof, in the world, as of the last step.
    pub fn hinge_at(&self, k: usize) -> Option<DVec3> {
        let (pos, rot) = self.last?;
        Some(self.poles.get(k)?.base_at(pos, &rot))
    }

    pub fn take_sounds(&mut self) -> Vec<RigSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Both poles are on the wire.
    pub fn all_on_wire(&self) -> bool {
        !self.poles.is_empty() && self.poles.iter().all(|p| p.mode == PoleMode::OnWire)
    }

    /// The gravity-compensated weight of the gear, for tests.
    pub fn pole_weight_n(&self) -> f32 {
        self.poles.iter().map(|p| (p.def.mass + p.def.head_mass) * G as f32).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::{DriveKind, PoleDef, RheostatDef};

    fn def() -> TrolleyDef {
        let pole = |side: i8| PoleDef { side, base: Vec3::new(0.3 * side as f32, -1.0, 3.0), length: 6.5, ..Default::default() };
        TrolleyDef { poles: vec![pole(-1), pole(1)], drive: Some(DriveKind::Rheostat(RheostatDef::default())), aux_kw: 3.0, ..Default::default() }
    }

    fn rot() -> DMat3 {
        DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2)
    }

    struct Sim {
        rig: TrolleyRig,
        x: f64,
        v: f32,
        forces: Vec<f32>,
    }

    impl Sim {
        fn new() -> Sim {
            Sim { rig: TrolleyRig::new(&def(), 3), x: 30.0, v: 0.0, forces: Vec::new() }
        }

        fn step(&mut self, net: &WireNet, pedal: f32, brake: f32, ropes: &[RopeCmd]) {
            let dt = 1.0 / 60.0;
            let o = self.rig.step(&RigInput {
                net: Some(net),
                dt,
                pos: DVec3::new(self.x, 0.0, 0.0),
                rot: rot(),
                accel: Vec3::ZERO,
                speed: self.v,
                wheel_radius: 0.5,
                pedal,
                brake,
                dir: 1.0,
                blinker: 0,
                radio: 0,
                route_code: 0,
                channel: 0,
                ropes,
                drive: true,
                script_force: 0.0,
            });
            let (f, mb) = (o.wheel_force, o.mech_brake);
            // a 16 t bus with rolling resistance 1 %
            let roll = if self.v > 0.01 { 0.01 * 16_000.0 * 9.81 } else { 0.0 };
            let a = (f - roll - mb.min(60_000.0) - o.electric_brake) / 16_000.0;
            self.v = (self.v + a * dt as f32).max(0.0);
            self.x += self.v as f64 * dt as f64;
            self.forces.push(f);
        }

        fn raise_both(&mut self, net: &WireNet) {
            for _ in 0..(60 * 6) {
                self.step(net, 0.0, 0.0, &[RopeCmd::Raise { ray: None, auto: true }; 2]);
            }
            for _ in 0..120 {
                self.step(net, 0.0, 0.0, &[RopeCmd::Idle; 2]);
            }
        }
    }

    fn line() -> WireNet {
        WireNet::parse("[trolley_section]\nid 1\nlive 1\n[trolley_span]\nsection 1\np -50 0 5.5\np 100 0 5.5\np 250 0 5.5\n")
    }

    /// A radio switch at (50, 0) on channel 1, range 20, its branch leaving to the north-east.
    fn radio_y(extra: &str) -> WireNet {
        WireNet::parse(&format!(
            "[trolley_span]\np 0 0 5.5\np 50 0 5.5\n[trolley_span]\np 50 0 5.5\np 100 0 5.5\n[trolley_span]\np 50 0 5.5\np 90 20 5.5\n\
             [trolley_switch]\npos 50 0 5.5\nmode radio\nrange 20\nchannel 1\n{extra}"
        ))
    }

    /// A bus standing at `pos` with its poles down, sending nothing on channel 1.
    fn standing(net: &WireNet, pos: DVec3) -> RigInput<'_> {
        RigInput {
            net: Some(net), dt: 1.0 / 60.0, pos, rot: rot(), accel: Vec3::ZERO, speed: 0.0,
            wheel_radius: 0.5, pedal: 0.0, brake: 0.0, dir: 1.0, blinker: 0, radio: 0,
            route_code: 0, channel: 1, ropes: &[RopeCmd::Idle; 2], drive: false, script_force: 0.0,
        }
    }

    #[test]
    fn radio_requests_reach_a_receiver_without_poles_on_the_wire() {
        let net = radio_y("");
        let mut rig = TrolleyRig::new(&def(), 3);
        let mut input = RigInput { radio: -1, ..standing(&net, DVec3::ZERO) };
        rig.step(&input);
        assert!(!net.switch_branch(0));
        input.pos = DVec3::new(50.0, 10.0, 0.0);
        rig.step(&input);
        assert!(!net.switch_branch(0));
        input.radio = 0;
        rig.step(&input);
        input.radio = -1;
        rig.step(&input);
        assert!(net.switch_branch(0));
        assert!(!rig.circuit);
        assert!(rig.poles.iter().all(|pole| pole.mode == PoleMode::Down));
        input.radio = 1;
        rig.step(&input);
        assert!(!net.switch_branch(0));
        assert_eq!(net.switch_display(0).map(|display| display.0), Some(2.0));
        assert_eq!(net.receive_radio(input.pos, &SwitchRequest { radio: -1, channel: 1, ..Default::default() }), 1);
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.pos = DVec3::ZERO;
        rig.step(&input);
        assert!(net.switch_branch(0));
        net.end_step(86400.0);
        assert!(net.switch_branch(0));
        input.pos = DVec3::new(50.0, 10.0, 0.0);
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.channel = 2;
        rig.step(&input);
        input.channel = 1;
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.radio = 0;
        rig.step(&input);
        input.radio = 1;
        rig.step(&input);
        assert!(!net.switch_branch(0));
    }

    #[test]
    fn radio_route_codes_are_sent_on_entry_but_not_on_a_channel_change() {
        let net = radio_y("route 158 left\nroute 159 right\n");
        let mut rig = TrolleyRig::new(&def(), 3);
        let mut input = RigInput { route_code: 158, ..standing(&net, DVec3::new(50.0, 0.0, 0.0)) };
        rig.step(&input);
        assert!(!net.switch_branch(0));
        input.route_code = 159;
        rig.step(&input);
        input.route_code = 158;
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.pos = DVec3::ZERO;
        input.route_code = 159;
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.pos = DVec3::new(50.0, 0.0, 0.0);
        rig.step(&input);
        assert!(!net.switch_branch(0));
        net.set_switch(0, true);
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.pos = DVec3::ZERO;
        input.route_code = 0;
        rig.step(&input);
        input.pos = DVec3::new(50.0, 0.0, 0.0);
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.route_code = 159;
        rig.step(&input);
        net.set_switch(0, false);
        input.channel = 2;
        input.route_code = 158;
        rig.step(&input);
        input.channel = 1;
        rig.step(&input);
        assert!(!net.switch_branch(0));
        input.route_code = 159;
        rig.step(&input);
        input.route_code = 158;
        rig.step(&input);
        assert!(net.switch_branch(0));
    }

    #[test]
    fn releasing_a_toggle_or_a_direction_does_not_hand_the_switch_back_to_the_route() {
        let net = radio_y("route 158 left\n");
        let mut rig = TrolleyRig::new(&def(), 3);
        let mut input = RigInput { route_code: 158, ..standing(&net, DVec3::new(50.0, 0.0, 0.0)) };
        rig.step(&input);
        net.set_switch(0, true);
        for (press, after) in [(2, false), (2, true), (1, false), (-1, true)] {
            input.radio = press;
            rig.step(&input);
            assert_eq!(net.switch_branch(0), after, "press {press}");
            input.radio = 0;
            rig.step(&input);
            assert_eq!(net.switch_branch(0), after, "release of {press}");
        }
        net.set_switch(0, false);
        input.route_code = 159;
        rig.step(&input);
        input.route_code = 158;
        rig.step(&input);
        assert!(net.switch_branch(0));
    }

    #[test]
    fn radio_toggle_requires_a_new_press_not_range_entry_or_channel_change() {
        let net = radio_y("");
        let mut rig = TrolleyRig::new(&def(), 3);
        let mut input = RigInput { radio: 2, ..standing(&net, DVec3::new(50.0, 0.0, 0.0)) };
        rig.step(&input);
        assert!(!net.switch_branch(0));
        input.radio = 0;
        rig.step(&input);
        input.radio = 2;
        rig.step(&input);
        assert!(net.switch_branch(0));
        rig.step(&input);
        assert!(net.switch_branch(0));
        net.set_switch(0, false);
        rig.step(&input);
        assert!(!net.switch_branch(0));
        input.radio = 0;
        rig.step(&input);
        input.radio = 2;
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.pos = DVec3::ZERO;
        input.radio = 0;
        rig.step(&input);
        input.radio = 2;
        rig.step(&input);
        input.pos = DVec3::new(50.0, 0.0, 0.0);
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.channel = 2;
        input.radio = 0;
        rig.step(&input);
        input.radio = 2;
        rig.step(&input);
        input.channel = 1;
        input.route_code = 158;
        rig.step(&input);
        assert!(net.switch_branch(0));
        input.radio = 0;
        rig.step(&input);
        input.radio = 2;
        rig.step(&input);
        assert!(!net.switch_branch(0));
        assert!(rig.poles.iter().all(|pole| pole.mode == PoleMode::Down));
    }

    #[test]
    fn frog_and_insulator_arcs_follow_the_net_settings() {
        let arcs = |settings: &str, kind: PoleEventKind, amps: f32| {
            let net = WireNet::parse(&format!("[trolley_net]\n{settings}\n[trolley_span]\np 0 0 5.5\np 50 0 5.5\n"));
            let mut rig = TrolleyRig::new(&def(), 7);
            let input = standing(&net, DVec3::ZERO);
            let (mut count, mut energy) = (0, 0.0f32);
            for _ in 0..1000 {
                rig.events.clear();
                rig.events.push(PoleEvent { pole: 0, kind, pos: DVec3::ZERO, amps });
                rig.handle_events(&net, &input, amps);
                let frog = matches!(kind, PoleEventKind::FrogPassed { .. });
                for spark in rig.take_sparks() {
                    assert_eq!(spark.kind, if frog { SparkKind::Frog } else { SparkKind::Insulator });
                    count += 1;
                    energy = spark.energy;
                }
                assert_eq!(rig.sounds.drain(..).any(|sound| matches!(sound, RigSound::Frog { .. })), frog);
            }
            (count, energy)
        };
        let frogs = |settings: &str, amps: f32| arcs(settings, PoleEventKind::FrogPassed { switch: 0, branch: false }, amps);
        let entries = |settings: &str, amps: f32| arcs(settings, PoleEventKind::GapEntered, amps);
        let exits = |settings: &str, amps: f32| arcs(settings, PoleEventKind::GapLeft, amps);
        let (count, full) = frogs("", 100.0);
        assert_eq!(count, 1000);
        assert!((full - 59.0).abs() < 1e-3, "{full}");
        assert_eq!(frogs("", 2.0).0, 0);
        assert_eq!(frogs("frog_arc_chance 0", 100.0).0, 0);
        assert_eq!(frogs("frog_arc_chance 5", 100.0).0, 1000);
        let (some, _) = frogs("frog_arc_chance 0.25", 100.0);
        assert!((150..350).contains(&some), "{some}");
        let (count, half) = frogs("frog_arc_scale 0.5", 100.0);
        assert_eq!(count, 1000);
        assert!((half - full / 2.0).abs() < 1e-3, "{half}");
        assert_eq!(frogs("frog_arc_min_amps 150", 100.0).0, 0);
        assert_eq!(frogs("frog_arc_min_amps 50", 100.0).0, 1000);
        assert_eq!(frogs("insulator_arc_chance 0", 100.0).0, 1000);
        let (count, entry) = entries("", 100.0);
        assert_eq!(count, 1000);
        assert!((entry - 60.0).abs() < 1e-3, "{entry}");
        assert!((exits("", 100.0).1 - 12.0).abs() < 1e-3);
        assert_eq!(entries("", 2.0).0, 0);
        assert_eq!(entries("frog_arc_chance 0", 100.0).0, 1000);
        assert_eq!(entries("insulator_arc_chance 0", 100.0).0, 0);
        assert_eq!(exits("insulator_arc_chance 0", 100.0).0, 0);
        let (some, _) = entries("insulator_arc_chance 0.25", 100.0);
        assert!((150..350).contains(&some), "{some}");
        assert!((entries("insulator_arc_scale 0.5", 100.0).1 - 30.0).abs() < 1e-3);
        assert_eq!(entries("insulator_arc_min_amps 20", 13.0).0, 0);
        assert_eq!(exits("insulator_arc_min_amps 20", 25.0).0, 1000);
    }

    #[test]
    fn a_script_s_gear_raises_both_poles_and_each_hits_the_wire_on_its_own() {
        let net = line();
        let mut s = Sim::new();
        let mut heard = Vec::new();
        for i in 0..(60 * 8) {
            for p in s.rig.poles.iter_mut() {
                p.gear = Some((i as f64 / 240.0).min(1.0));
            }
            s.step(&net, 0.0, 0.0, &[RopeCmd::Idle; 2]);
            heard.extend(s.rig.take_sounds());
        }
        assert!(s.rig.all_on_wire());
        let hits: Vec<usize> = heard.iter().filter_map(|h| match h { RigSound::OnWire { pole } => Some(*pole), _ => None }).collect();
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert_ne!(hits[0], hits[1]);
    }

    #[test]
    fn shoes_keep_contact_through_joints_and_frogs_under_power() {
        let frog = "[trolley_span]\np -50 0 5.5\np 100 0 5.5\n[trolley_span]\np 100 0 5.5\np 400 0 5.5\n[trolley_span]\np 100 0 5.5\np 300 80 5.5\n";
        let joint = "[trolley_span]\np -50 0 5.5\np 100 0 5.5\n[trolley_span]\np 100 0 5.5\np 400 0 5.5\n";
        for (wire, roughness) in [(frog, "0"), (frog, "0.0012"), (joint, "0"), (joint, "0.0012")] {
            let net = WireNet::parse(&format!("[trolley_net]\nroughness {roughness}\nfrog_arc_chance 0\n{wire}[trolley_switch]\npos 100 0 5.5\nmode fixed\n"));
            let mut s = Sim::new();
            s.raise_both(&net);
            let mut sparks = Vec::new();
            for _ in 0..(60 * 20) {
                s.step(&net, if s.v < 8.0 { 1.0 } else { 0.4 }, 0.0, &[RopeCmd::Idle; 2]);
                sparks.extend(s.rig.take_sparks().into_iter().map(|spark| (spark.kind, spark.pos.x.round())));
            }
            assert!(s.x > 150.0 && s.rig.all_on_wire() && s.rig.amps > 20.0);
            assert!(sparks.is_empty(), "roughness {roughness}: {sparks:?}");
        }
    }

    #[test]
    fn a_bus_without_poles_up_has_no_power() {
        let net = line();
        let mut s = Sim::new();
        for _ in 0..120 {
            s.step(&net, 1.0, 0.0, &[RopeCmd::Idle; 2]);
        }
        assert!(!s.rig.circuit);
        assert_eq!(s.v, 0.0);
    }

    #[test]
    fn raised_poles_close_the_circuit_and_the_bus_accelerates() {
        let net = line();
        let mut s = Sim::new();
        s.raise_both(&net);
        assert!(s.rig.all_on_wire(), "{:?}", s.rig.poles.iter().map(|p| p.mode).collect::<Vec<_>>());
        assert!(s.rig.circuit);
        for _ in 0..(60 * 10) {
            s.step(&net, 1.0, 0.0, &[RopeCmd::Idle; 2]);
        }
        assert!(s.v > 5.0, "{}", s.v);
        assert!(s.rig.volts > 500.0 && s.rig.volts <= 600.0);
        assert!(s.rig.amps > 30.0);
    }

    #[test]
    fn a_dead_section_stops_the_current_and_the_bus_rolls_down() {
        // live up to x = 100, dead after it
        let net = WireNet::parse(
            "[trolley_section]\nid 1\nlive 1\n[trolley_section]\nid 2\nlive 0\n[trolley_span]\nsection 1\np -50 0 5.5\np 100 0 5.5\n[trolley_span]\nsection 2\np 100 0 5.5\np 400 0 5.5\n",
        );
        let mut s = Sim::new();
        s.raise_both(&net);
        assert!(s.rig.circuit);
        let mut sparks = 0;
        let mut was_live = false;
        let mut lost_at = None;
        for k in 0..(60 * 40) {
            s.step(&net, 1.0, 0.0, &[RopeCmd::Idle; 2]);
            sparks += s.rig.take_sparks().len();
            if s.rig.circuit {
                was_live = true;
            } else if was_live && lost_at.is_none() {
                lost_at = Some((k, s.x, s.v));
            }
        }
        let (_, x_lost, v_lost) = lost_at.expect("the circuit broke");
        assert!(x_lost > 100.0 && x_lost < 112.0, "x {x_lost}");
        assert!(s.rig.out.wheel_force == 0.0);
        // it rolled on after losing the current, slowing under rolling resistance
        assert!(s.v < v_lost);
        assert!(s.x > x_lost + 10.0);
        // the break under load struck an arc
        assert!(sparks > 0);
    }

    #[test]
    fn coasting_through_the_insulator_strikes_no_arc() {
        let net = WireNet::parse(
            "[trolley_section]\nid 1\nlive 1\n[trolley_section]\nid 2\nlive 1\n[trolley_span]\nsection 1\np -50 0 5.5\np 100 0 5.5\n[trolley_span]\nsection 2\np 100 0 5.5\np 400 0 5.5\n",
        );
        let mut s = Sim::new();
        s.raise_both(&net);
        let mut loud = 0.0f32;
        for _ in 0..(60 * 14) {
            // power until 60 m, then coast through the gap
            let pedal = if s.x < 80.0 { 1.0 } else { 0.0 };
            s.step(&net, pedal, 0.0, &[RopeCmd::Idle; 2]);
            for sp in s.rig.take_sparks() {
                loud = loud.max(sp.energy);
            }
        }
        assert!(s.x > 105.0);
        assert!(loud < 1.0, "{loud}");
        // power on through the gap is an arc
        let mut s = Sim::new();
        s.raise_both(&net);
        let mut loud = 0.0f32;
        for _ in 0..(60 * 14) {
            s.step(&net, 1.0, 0.0, &[RopeCmd::Idle; 2]);
            for sp in s.rig.take_sparks() {
                loud = loud.max(sp.energy);
            }
        }
        assert!(loud > 30.0, "{loud}");
    }

    #[test]
    fn a_pulled_rope_takes_a_pole_off_and_the_circuit_goes() {
        let net = line();
        let mut s = Sim::new();
        s.raise_both(&net);
        for _ in 0..120 {
            s.step(&net, 0.0, 0.0, &[RopeCmd::Pull, RopeCmd::Idle]);
        }
        assert_eq!(s.rig.poles[0].mode, PoleMode::Down);
        assert!(!s.rig.circuit);
    }
}
