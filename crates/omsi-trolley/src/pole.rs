//! A pole (boom): a rigid arm hinged on the roof that a spring presses up against the wire,
//! with the shoe at its tip that the wire runs in.
//!
//! The arm has two angles, its elevation `theta` over the horizontal and its swing `psi`
//! from straight back (positive to the right). On the wire the tip lies on the wire at the
//! boom's length from the hinge, which fixes both angles; what the wire does to the arm is
//! then worked out from how the wire moves past the shoe. Off the wire the arm swings under
//! its spring, its weight and the bus's own motion until a rope holds it or the shoe catches
//! a wire.
//!
//! Elevation is a real degree of freedom with a one-sided contact: the spring pushes the tip
//! up with a force that has to accelerate the arm's mass along every rise of the wire (a
//! hanger, a kink, the wire's own unevenness); when the wire rises faster than the arm can
//! follow, the shoe leaves it for a moment (and an arc strikes if current flows), and when
//! the wire lifts out of the shoe's groove the pole has come off. Sideways the arm cannot
//! stand off the wire's direction by more than the shoe's swivel allows, and a kink in the
//! wire's direction taken faster than the shoe can turn pulls the wire out.

use crate::def::PoleDef;
use crate::wire::{Contact, WireNet};
use glam::{DMat3, DVec3};

pub const G: f64 = 9.81;
const THETA_MAX: f64 = 80.0 * std::f64::consts::PI / 180.0;
/// How stiffly the spring's force falls as the arm rises (per radian).
const SPRING_FALL: f64 = 0.35;
/// Damping of the arm's turning (1/s, times its inertia).
const DAMPING: f64 = 1.2;
/// The sub-step the arms are integrated with (s).
pub const SUBSTEP: f64 = 1.0 / 240.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoleMode {
    /// On its hook, tied down.
    Down,
    /// Loose under its spring.
    Free,
    /// Held by its rope and guided by hand (or by the motor).
    Hand,
    /// Pulled down by its rope.
    Lowering,
    /// The shoe is on the wire.
    OnWire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DewireCause {
    /// The boom stood too far off the wire's direction.
    Lateral,
    /// A bend of the wire taken too fast.
    Kink,
    /// The wire lifted out of the shoe.
    Bounce,
    /// The wire is out of the boom's reach.
    OutOfReach,
    /// The wire ended under the shoe.
    EndOfWire,
    /// Pulled off by the rope.
    Rope,
}

impl DewireCause {
    pub fn describe(self) -> &'static str {
        match self {
            DewireCause::Lateral => "the bus stood too far to the side of the wire",
            DewireCause::Kink => "a bend of the wire taken too fast",
            DewireCause::Bounce => "the shoe jumped off the wire",
            DewireCause::OutOfReach => "the wire is out of the boom's reach",
            DewireCause::EndOfWire => "the wire ended",
            DewireCause::Rope => "pulled down by the rope",
        }
    }
}

/// What the rope is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rope {
    Idle,
    /// Pulled down.
    Pull,
    /// Held and guided at this wire point (or not aimed at any); `auto` is the motor's.
    Aim { goal: Option<Contact>, auto: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoleEventKind {
    Engaged,
    Dewired(DewireCause),
    /// The shoe left the wire for a moment.
    ContactOpened,
    /// The shoe came back after `open_s` seconds, closing at `impact` m/s.
    ContactClosed { open_s: f32, impact: f32 },
    Latched,
    Released,
    GapEntered,
    GapLeft,
    FrogPassed { switch: u32, branch: bool },
    Plate { switch: u32 },
}

#[derive(Debug, Clone, Copy)]
pub struct PoleEvent {
    pub pole: usize,
    pub kind: PoleEventKind,
    pub pos: DVec3,
    pub amps: f32,
}

/// The bus's motion over one step, for the arm to follow: poses at its start and end.
#[derive(Debug, Clone, Copy)]
pub struct PoseSpan {
    pub pos0: DVec3,
    pub rot0: DMat3,
    pub pos1: DVec3,
    pub rot1: DMat3,
    /// Acceleration of the roof in the bus frame (m/s^2: x right, y forward, z up).
    pub accel: glam::Vec3,
}

impl PoseSpan {
    pub fn still(pos: DVec3, rot: DMat3) -> PoseSpan {
        PoseSpan { pos0: pos, rot0: rot, pos1: pos, rot1: rot, accel: glam::Vec3::ZERO }
    }

    fn at(&self, t: f64) -> (DVec3, DMat3) {
        let q0 = glam::DQuat::from_mat3(&self.rot0);
        let q1 = glam::DQuat::from_mat3(&self.rot1);
        (self.pos0.lerp(self.pos1, t), DMat3::from_quat(q0.slerp(q1, t)))
    }
}

/// The player's or the motor's hand on the rope.
#[derive(Debug, Clone, Copy, Default)]
struct Hand {
    held: f64,
    phase: [f64; 4],
}

#[derive(Debug, Clone)]
pub struct Pole {
    pub def: PoleDef,
    pub mode: PoleMode,
    pub theta: f64,
    pub psi: f64,
    pub w_theta: f64,
    pub w_psi: f64,
    pub contact: Option<Contact>,
    dir: i8,
    /// The shoe touches the wire.
    pub closed: bool,
    /// How far under the wire the shoe is while the contact is open (m).
    pub gap: f64,
    pub tip: DVec3,
    prev_tip: DVec3,
    /// The wire at the shoe carries current and the shoe touches it.
    pub on_live: bool,
    pub section: usize,
    /// The force the shoe presses the wire with (N).
    pub force: f64,
    /// Speed the shoe slides along the wire at (m/s).
    pub slide: f64,
    theta_w_prev: f64,
    dtheta_w_prev: f64,
    have_prev: bool,
    prev_wire_pt: DVec3,
    open_time: f64,
    in_gap: bool,
    last_pos: Option<(u32, f64)>,
    hand: Hand,
    lower_auto: bool,
    retrieve_wait: f64,
    /// A motor's raising has let the pole go to spring on to its wire.
    auto_released: bool,
    /// A pole that has just come off does not take the wire again at once (s).
    engage_block: f64,
    rng: u64,
    clock: f64,
    /// Bad weather and age: the unevenness of the wire (m).
    pub roughness: f64,
    /// Where a hand holds the tip when no wire is near its aim: a point of the world the
    /// mouse's ray reaches at the pole's length (the rig sets it each step).
    pub free_aim: Option<DVec3>,
    /// A pole just let go of: the wire the spring is lifting it to, which swings the boom
    /// the last bit sideways (the hand's last nudge) while it rises.
    pub steer_to: Option<DVec3>,
}

pub fn rotate_tip_pub(theta: f64, psi: f64, len: f64) -> DVec3 {
    rotate_tip(theta, psi, len)
}

fn rotate_tip(theta: f64, psi: f64, len: f64) -> DVec3 {
    DVec3::new(psi.sin() * theta.cos(), -psi.cos() * theta.cos(), theta.sin()) * len
}

impl Pole {
    pub fn new(def: PoleDef, seed: u64) -> Pole {
        let theta = (def.rest_angle as f64).to_radians();
        Pole {
            def,
            mode: PoleMode::Down,
            theta,
            psi: 0.0,
            w_theta: 0.0,
            w_psi: 0.0,
            contact: None,
            dir: 1,
            closed: false,
            gap: 0.0,
            tip: DVec3::ZERO,
            prev_tip: DVec3::ZERO,
            on_live: false,
            section: 0,
            force: 0.0,
            slide: 0.0,
            theta_w_prev: 0.0,
            dtheta_w_prev: 0.0,
            have_prev: false,
            prev_wire_pt: DVec3::ZERO,
            open_time: 0.0,
            in_gap: false,
            last_pos: None,
            hand: Hand::default(),
            lower_auto: false,
            retrieve_wait: 0.0,
            auto_released: false,
            engage_block: 0.0,
            rng: seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1,
            clock: 0.0,
            roughness: 0.0012,
            free_aim: None,
            steer_to: None,
        }
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn len(&self) -> f64 {
        self.def.length as f64
    }

    fn inertia(&self) -> f64 {
        (self.def.mass as f64 / 3.0 + self.def.head_mass as f64) * self.len() * self.len()
    }

    fn rest(&self) -> f64 {
        (self.def.rest_angle as f64).to_radians()
    }

    /// The torque (N m) the spring, the weight and the roof's vertical acceleration `az` make
    /// on the arm at elevation `theta`.
    pub fn torque(&self, theta: f64, az: f64) -> f64 {
        let w_eq = (self.def.mass as f64 * 0.5 + self.def.head_mass as f64) * G;
        let work = (self.def.work_angle as f64).to_radians();
        let spring = (self.def.tip_force as f64 + w_eq) * (1.0 + SPRING_FALL * (work - theta));
        let inertial = (self.def.mass as f64 * 0.5 + self.def.head_mass as f64) * az;
        self.len() * theta.cos() * (spring - w_eq - inertial)
    }

    /// The tip as the angles put it.
    fn place(&mut self, base: DVec3, rot: &DMat3) {
        self.prev_tip = self.tip;
        self.tip = base + *rot * rotate_tip(self.theta, self.psi, self.len());
    }

    /// Elevation and swing of a world point seen from the hinge.
    fn angles_to(&self, base: DVec3, rot: &DMat3, p: DVec3) -> (f64, f64, f64) {
        let v = rot.transpose() * (p - base);
        let l = v.length().max(1e-6);
        ((v.z / l).clamp(-1.0, 1.0).asin(), v.x.atan2(-v.y), l)
    }

    pub fn is_up(&self) -> bool {
        !matches!(self.mode, PoleMode::Down)
    }

    pub fn state_code(&self) -> f32 {
        match self.mode {
            PoleMode::Down => 0.0,
            PoleMode::Lowering => 1.0,
            PoleMode::Hand => 2.0,
            PoleMode::Free => 3.0,
            PoleMode::OnWire => {
                if self.closed {
                    5.0
                } else {
                    4.0
                }
            }
        }
    }

    fn dewire(&mut self, cause: DewireCause, amps: f32, out: &mut Vec<PoleEvent>, pole: usize) {
        out.push(PoleEvent { pole, kind: PoleEventKind::Dewired(cause), pos: self.tip, amps: if self.closed { amps } else { 0.0 } });
        self.contact = None;
        self.closed = false;
        self.gap = 0.0;
        self.on_live = false;
        self.have_prev = false;
        self.last_pos = None;
        self.in_gap = false;
        self.force = 0.0;
        self.engage_block = 0.5;
        if cause == DewireCause::Rope {
            self.mode = PoleMode::Lowering;
            self.lower_auto = false;
        } else {
            self.mode = PoleMode::Free;
            self.w_theta = self.w_theta.max(0.5);
            self.retrieve_wait = 0.45;
        }
    }

    /// Advance the pole over a bus step `dt` with its motion `span`.
    pub fn step(&mut self, net: &WireNet, span: &PoseSpan, dt: f64, rope: Rope, amps: f32, pole: usize, out: &mut Vec<PoleEvent>) {
        let n = (dt / SUBSTEP).ceil().max(1.0) as usize;
        let h = dt / n as f64;
        for i in 0..n {
            let (pos, rot) = span.at((i as f64 + 1.0) / n as f64);
            let base = pos + rot * self.def.base.as_dvec3();
            self.clock += h;
            self.sub(net, base, &rot, span.accel, h, rope, amps, pole, out);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sub(&mut self, net: &WireNet, base: DVec3, rot: &DMat3, accel: glam::Vec3, dt: f64, rope: Rope, amps: f32, pole: usize, out: &mut Vec<PoleEvent>) {
        self.engage_block = (self.engage_block - dt).max(0.0);
        // the rope first
        if !matches!(rope, Rope::Aim { auto: true, .. }) {
            self.auto_released = false;
        }
        let rope = match rope {
            Rope::Aim { auto: true, .. } if self.auto_released && matches!(self.mode, PoleMode::Free | PoleMode::OnWire) => Rope::Idle,
            r => r,
        };
        match rope {
            Rope::Pull => match self.mode {
                PoleMode::OnWire => {
                    self.dewire(DewireCause::Rope, amps, out, pole);
                }
                PoleMode::Free | PoleMode::Hand => {
                    self.mode = PoleMode::Lowering;
                    self.lower_auto = false;
                    self.contact = None;
                }
                _ => {}
            },
            Rope::Aim { auto, .. } => match self.mode {
                // a hand (the mouse) takes the pole off its wire to put it somewhere else
                PoleMode::OnWire if !auto => {
                    self.dewire(DewireCause::Rope, amps, out, pole);
                    self.mode = PoleMode::Hand;
                    self.hand = Hand { held: 0.0, phase: [self.rand() * 6.28, self.rand() * 6.28, self.rand() * 6.28, self.rand() * 6.28] };
                }
                PoleMode::Down => {
                    self.mode = PoleMode::Hand;
                    self.hand = Hand { held: 0.0, phase: [self.rand() * 6.28, self.rand() * 6.28, self.rand() * 6.28, self.rand() * 6.28] };
                    out.push(PoleEvent { pole, kind: PoleEventKind::Released, pos: self.tip, amps: 0.0 });
                    let _ = auto;
                }
                PoleMode::Lowering | PoleMode::Free => {
                    self.mode = PoleMode::Hand;
                    self.hand = Hand { held: 0.0, phase: [self.rand() * 6.28, self.rand() * 6.28, self.rand() * 6.28, self.rand() * 6.28] };
                }
                _ => {}
            },
            Rope::Idle => match self.mode {
                PoleMode::Hand => {
                    self.mode = PoleMode::Free;
                }
                PoleMode::Lowering if !self.lower_auto => {
                    self.mode = PoleMode::Free;
                }
                _ => {}
            },
        }

        match self.mode {
            PoleMode::Down => {
                self.theta = self.rest();
                self.psi = 0.0;
                self.w_theta = 0.0;
                self.w_psi = 0.0;
                self.place(base, rot);
            }
            PoleMode::Lowering => {
                let rate = if self.lower_auto { (self.def.retriever.max(20.0) as f64).to_radians() } else { 35f64.to_radians() };
                self.theta -= rate * dt;
                self.psi -= self.psi.signum() * (0.9 * dt).min(self.psi.abs());
                self.w_theta = -rate;
                if self.theta <= self.rest() {
                    self.theta = self.rest();
                    if self.psi.abs() < 0.2 {
                        self.mode = PoleMode::Down;
                        self.w_theta = 0.0;
                        out.push(PoleEvent { pole, kind: PoleEventKind::Latched, pos: self.tip, amps: 0.0 });
                    }
                }
                self.place(base, rot);
            }
            PoleMode::Hand => {
                let goal = match rope {
                    Rope::Aim { goal, .. } => goal,
                    _ => None,
                };
                self.hand.held += dt;
                if matches!(rope, Rope::Aim { auto: true, .. }) && self.hand.held > 1.6 {
                    // the motor lets the boom go, as a hand would, to spring up on to the wire
                    self.auto_released = true;
                    self.mode = PoleMode::Free;
                    return;
                }
                let t = self.hand.held;
                let p = self.hand.phase;
                // the hand's unsteadiness grows with the arm's length
                let amp = if matches!(rope, Rope::Aim { auto: true, .. }) { 0.004 } else { 0.03 } / self.len();
                let jit = |a: f64, b: f64| amp * ((std::f64::consts::TAU * 0.9 * t + a).sin() + 0.6 * (std::f64::consts::TAU * 2.3 * t + b).sin());
                let fwd = *rot * DVec3::Y;
                let goal_pt = goal.and_then(|mut c| {
                    net.place_behind(&mut c, base, fwd, self.len());
                    net.solve_tip(&mut c, base, self.len()).ok().map(|_| net.wire_point(&c).0)
                });
                let (tg, pg) = match goal_pt {
                    Some(g) => {
                        let aim = g - DVec3::Z * 0.2;
                        let (th, ps, _) = self.angles_to(base, rot, aim);
                        (th, ps)
                    }
                    None => match self.free_aim {
                        // no wire near: the tip goes where the hand points
                        Some(p) => {
                            let (th, ps, _) = self.angles_to(base, rot, p);
                            (th, ps)
                        }
                        None => ((self.def.work_angle as f64 * 0.6).to_radians(), 0.0),
                    },
                };
                let manual = matches!(rope, Rope::Aim { auto: false, .. }) && self.free_aim.is_some();
                if manual {
                    // the mouse's hand: the tip goes where the cursor points, smoothly (a time
                    // constant of a twentieth of a second, at most three radians a second), with
                    // no tremor of its own
                    // (where the cursor points at a wire the tip is held a little under it, to be
                    // let go of and rise into it; elsewhere where the cursor points)
                    let target = goal_pt.map(|g| g - DVec3::Z * 0.35).or(self.free_aim).unwrap();
                    let (tm, pm, _) = self.angles_to(base, rot, target);
                    let (tm, pm) = (tm.clamp(self.rest(), THETA_MAX), pm.clamp(-1.7, 1.7));
                    let k = 1.0 - (-dt / 0.05).exp();
                    let step = 3.0 * dt;
                    let dth = ((tm - self.theta) * k).clamp(-step, step);
                    let dps = ((pm - self.psi) * k).clamp(-step, step);
                    self.theta += dth;
                    self.psi += dps;
                    self.w_theta = dth / dt;
                    self.w_psi = dps / dt;
                } else {
                    let tg = (tg + jit(p[0], p[1])).clamp(self.rest(), THETA_MAX);
                    let pg = (pg + jit(p[2], p[3])).clamp(-1.7, 1.7);
                    let up = 0.9 * dt;
                    let down = 1.3 * dt;
                    let d = tg - self.theta;
                    self.w_theta = d.clamp(-down, up) / dt;
                    self.theta += d.clamp(-down, up);
                    let dp = pg - self.psi;
                    self.w_psi = dp.clamp(-1.2 * dt, 1.2 * dt) / dt;
                    self.psi += dp.clamp(-1.2 * dt, 1.2 * dt);
                }
                self.place(base, rot);
                self.try_engage(net, base, rot, amps, pole, out);
            }
            PoleMode::Free => {
                let az = accel.z as f64;
                let i = self.inertia();
                let al = (self.torque(self.theta, az) - DAMPING * i * self.w_theta) / i;
                self.w_theta += al * dt;
                self.theta += self.w_theta * dt;
                if self.theta > THETA_MAX {
                    self.theta = THETA_MAX;
                    if self.w_theta > 0.0 {
                        self.w_theta *= -0.25;
                    }
                }
                if self.theta < self.rest() {
                    self.theta = self.rest();
                    self.w_theta = self.w_theta.max(0.0);
                }
                // swing: a free pendulum with no restoring force, only drag, pushed by the roof's
                // sideways acceleration
                let lh = (self.len() * self.theta.cos()).max(0.5);
                self.w_psi += (-accel.x as f64 / lh - 1.5 * self.w_psi) * dt;
                if let Some(w) = self.steer_to {
                    // (just let go: the boom is swung on to the wire above it as it rises)
                    let (_, ps, _) = self.angles_to(base, rot, w);
                    self.w_psi += (14.0 * (ps - self.psi) - 7.0 * self.w_psi) * dt;
                }
                self.psi += self.w_psi * dt;
                if self.psi.abs() > 1.7 {
                    self.psi = self.psi.signum() * 1.7;
                    self.w_psi = 0.0;
                }
                self.place(base, rot);
                if self.def.retriever > 0.0 {
                    self.retrieve_wait -= dt;
                    if self.retrieve_wait <= 0.0 && (self.theta > 55f64.to_radians() || self.retrieve_wait < -0.6) {
                        self.mode = PoleMode::Lowering;
                        self.lower_auto = true;
                    }
                }
                self.try_engage(net, base, rot, amps, pole, out);
            }
            PoleMode::OnWire => self.wire_sub(net, base, rot, accel, dt, amps, pole, out),
        }
    }

    /// Does the shoe, moving from where it was, take a wire into its groove?
    fn try_engage(&mut self, net: &WireNet, base: DVec3, rot: &DMat3, amps: f32, pole: usize, out: &mut Vec<PoleEvent>) {
        if net.is_empty() || self.engage_block > 0.0 {
            return;
        }
        let depth = self.def.groove_depth as f64;
        let funnel = self.def.funnel as f64;
        let mut best: Option<(Contact, f64)> = None;
        for (c, _) in net.candidates(self.tip, 0.6) {
            let (w, t) = net.wire_point(&c);
            let r = crate::wire::right_of(t);
            let rel = self.tip - w;
            let prel = self.prev_tip - w;
            let perp = rel - t * rel.dot(t);
            let pperp = prel - t * prel.dot(t);
            let lat = perp.dot(r);
            // the wire passes between the horns as the tip rises through it
            // (a wire the boom could not stay on is not taken)
            let h = DVec3::new(w.x - base.x, w.y - base.y, 0.0);
            let th = DVec3::new(t.x, t.y, 0.0);
            let aligned = h.length() < 0.3 || th.length() < 0.05 || h.normalize().dot(th.normalize()).abs().clamp(0.0, 1.0).acos() < (self.def.max_wire_angle as f64).to_radians() * 0.8;
            // (a shoe lifted by its spring crosses the wire's height in a step or two: the wire passed
            // between the horns counts as well as one the shoe is sitting in)
            let through = pperp.z <= 0.0 && perp.z > 0.02 && perp.z <= 0.3;
            if aligned && lat.abs() <= funnel && ((perp.z >= -depth && perp.z <= 0.02 && perp.z >= pperp.z - 1e-9) || through) {
                if best.is_none_or(|b| lat.abs() < b.1) {
                    best = Some((c, lat.abs()));
                }
            }
        }
        if let Some((mut c, _)) = best {
            if net.solve_tip(&mut c, base, self.len()).is_ok() {
                self.contact = Some(c);
                self.mode = PoleMode::OnWire;
                self.closed = true;
                self.gap = 0.0;
                self.have_prev = false;
                self.dir = 1;
                self.last_pos = Some((c.span, c.s));
                self.open_time = 0.0;
                self.in_gap = net.in_gap(&c);
                out.push(PoleEvent { pole, kind: PoleEventKind::Engaged, pos: self.tip, amps });
                let _ = rot;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn wire_sub(&mut self, net: &WireNet, base: DVec3, rot: &DMat3, accel: glam::Vec3, dt: f64, amps: f32, pole: usize, out: &mut Vec<PoleEvent>) {
        let Some(mut c) = self.contact else {
            self.mode = PoleMode::Free;
            return;
        };
        let old = c;
        let len = self.len();
        let info = match net.solve_tip(&mut c, base, len) {
            Ok(i) => i,
            Err(e) => {
                let cause = match e {
                    crate::wire::SolveErr::EndOfWire => DewireCause::EndOfWire,
                    crate::wire::SolveErr::OutOfReach => DewireCause::OutOfReach,
                };
                self.dewire(cause, amps, out, pole);
                return;
            }
        };
        self.contact = Some(c);
        let (mut w, t) = net.wire_point(&c);
        // the unevenness of the wire: waves along it, and the hard spots at the hangers
        let sg = c.s + c.span as f64 * 7.31;
        let hanger = {
            let k = (c.s / 24.0).round();
            let x = (c.s - k * 24.0) / 0.35;
            0.003 * (-x * x).exp()
        };
        w.z += self.roughness * ((std::f64::consts::TAU * sg / 2.6).sin() + 0.5 * (std::f64::consts::TAU * sg / 0.95 + 1.3).sin()) + hanger;
        let (th_w, ps_w, _) = self.angles_to(base, rot, w);
        // how fast the shoe slides along the wire
        let u = if self.have_prev { (w - self.prev_wire_pt).length() / dt } else { 0.0 };
        self.slide = u;
        // events at joints, the insulator and the plates before a frog
        for np in &info.nodes {
            if let Some(si) = net.nodes[np.node as usize].switch {
                out.push(PoleEvent { pole, kind: PoleEventKind::FrogPassed { switch: si, branch: net.switch_branch(si as usize) }, pos: w, amps });
            }
        }
        let gap_now = net.in_gap(&c);
        if gap_now != self.in_gap {
            out.push(PoleEvent { pole, kind: if gap_now { PoleEventKind::GapEntered } else { PoleEventKind::GapLeft }, pos: w, amps });
            self.in_gap = gap_now;
        }
        for (si, sw) in net.switches.iter().enumerate() {
            let (plate_span, plate_s, towards) = sw.plate;
            if plate_span != c.span {
                continue;
            }
            if let Some((ps, pss)) = self.last_pos {
                let crossed = if towards == 0 { pss > plate_s && c.s <= plate_s } else { pss < plate_s && c.s >= plate_s };
                if ps == c.span && crossed {
                    out.push(PoleEvent { pole, kind: PoleEventKind::Plate { switch: si as u32 }, pos: w, amps });
                }
            }
        }
        self.last_pos = Some((c.span, c.s));
        let _ = old;
        // the boom against the wire's direction, seen from above
        let h = DVec3::new(w.x - base.x, w.y - base.y, 0.0);
        let th = DVec3::new(t.x, t.y, 0.0);
        if h.length() > 0.3 && th.length() > 0.05 {
            let cosb = (h.normalize().dot(th.normalize())).abs().clamp(0.0, 1.0);
            if cosb.acos() > (self.def.max_wire_angle as f64).to_radians() {
                self.tip = w;
                self.dewire(DewireCause::Lateral, amps, out, pole);
                return;
            }
        }
        // a bend taken faster than the shoe can turn
        if info.kink > 1e-3 && self.have_prev {
            let dv = u * 2.0 * (info.kink * 0.5).sin();
            if dv > self.def.kink_speed as f64 {
                self.tip = w;
                self.dewire(DewireCause::Kink, amps, out, pole);
                return;
            }
        }
        // elevation: one-sided contact
        let dth_w = if self.have_prev { (th_w - self.theta_w_prev) / dt } else { 0.0 };
        let ddth_w = if self.have_prev { (dth_w - self.dtheta_w_prev) / dt } else { 0.0 };
        if !self.have_prev {
            self.theta = th_w;
            self.w_theta = 0.0;
            self.closed = true;
        }
        let i = self.inertia();
        let tau = self.torque(self.theta, accel.z as f64) - DAMPING * i * self.w_theta;
        self.w_theta += tau / i * dt;
        self.theta += self.w_theta * dt;
        // (the shoe and the wire give a fraction of a millimetre: a contact does not open at
        // the solver's noise)
        if self.theta >= th_w - 3e-5 {
            if !self.closed {
                let impact = ((self.w_theta - dth_w) * len * self.theta.cos()).max(0.0);
                out.push(PoleEvent { pole, kind: PoleEventKind::ContactClosed { open_s: self.open_time as f32, impact: impact as f32 }, pos: w, amps });
                self.closed = true;
            }
            self.theta = self.theta.max(th_w);
            self.theta = th_w;
            self.w_theta = self.w_theta.min(dth_w);
            self.gap = 0.0;
            self.open_time = 0.0;
            // what the shoe presses with: the spring's torque less what the arm's own
            // acceleration along the wire takes
            self.force = ((self.torque(th_w, accel.z as f64) - i * ddth_w) / (len * th_w.cos().max(0.1))).max(0.0);
        } else {
            if self.closed {
                out.push(PoleEvent { pole, kind: PoleEventKind::ContactOpened, pos: w, amps });
                self.closed = false;
            }
            self.open_time += dt;
            self.gap = len * self.theta.cos() * (th_w - self.theta);
            self.force = 0.0;
            // (a bump opens the contact for a moment; the shoe is only off once it stays out of
            // its groove, or leaves it far)
            let groove = self.def.groove_depth as f64;
            if self.gap > 3.0 * groove || (self.gap > groove && self.open_time > 0.15) {
                self.psi = ps_w;
                self.tip = w - DVec3::Z * self.gap;
                self.dewire(DewireCause::Bounce, amps, out, pole);
                return;
            }
        }
        self.psi = ps_w;
        self.w_psi = 0.0;
        self.theta_w_prev = th_w;
        self.dtheta_w_prev = dth_w;
        self.have_prev = true;
        self.prev_wire_pt = w;
        self.prev_tip = self.tip;
        self.tip = if self.closed { w } else { w - DVec3::Z * self.gap };
        self.section = net.section_of(&c);
        self.on_live = self.closed && net.is_live(&c);
    }

    /// The tip's world position for drawing, from the angles at a pose.
    pub fn tip_at(&self, pos: DVec3, rot: &DMat3) -> DVec3 {
        pos + *rot * (self.def.base.as_dvec3() + rotate_tip(self.theta, self.psi, self.len()))
    }

    pub fn base_at(&self, pos: DVec3, rot: &DMat3) -> DVec3 {
        pos + *rot * self.def.base.as_dvec3()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::WireNet;

    fn straight() -> WireNet {
        WireNet::parse("[trolley_section]\nid 1\nlive 1\n[trolley_span]\nsection 1\np -50 0 5.5\np 0 0 5.5\np 50 0 5.5\np 100 0 5.5\n")
    }

    fn pole() -> Pole {
        let def = PoleDef { side: 1, base: glam::Vec3::new(0.3, 0.0, 3.0), length: 6.5, ..Default::default() };
        let mut p = Pole::new(def, 7);
        p.roughness = 0.0;
        p
    }

    fn run(p: &mut Pole, net: &WireNet, pos: DVec3, secs: f64, rope: Rope, amps: f32, ev: &mut Vec<PoleEvent>) {
        // the bus stands: heading north (+y), the wire runs east-west: rotate so that the
        // bus's back is -x... the tests put the wire along the bus: heading east
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        let span = PoseSpan::still(pos, rot);
        let n = (secs * 60.0) as usize;
        for _ in 0..n {
            p.step(net, &span, 1.0 / 60.0, rope, amps, 0, ev);
        }
    }

    #[test]
    fn the_pole_lying_on_its_hook_stays() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        run(&mut p, &net, DVec3::new(20.0, 0.0, 0.0), 2.0, Rope::Idle, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::Down);
        assert!(ev.is_empty());
    }

    #[test]
    fn a_hand_aimed_pole_catches_the_wire() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        let pos = DVec3::new(20.0, 0.0, 0.0);
        // the bus heads east: its pole trails to the west; the wire straight above the axis
        let goal = Contact { span: 0, s: 65.0, side: 0 };
        run(&mut p, &net, pos, 4.0, Rope::Aim { goal: Some(goal), auto: false }, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::Hand);
        // let go: the spring lifts the shoe into the wire
        run(&mut p, &net, pos, 2.0, Rope::Idle, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::OnWire, "{:?}", ev);
        assert!(p.closed);
        assert!(ev.iter().any(|e| matches!(e.kind, PoleEventKind::Engaged)));
        // the shoe lies on the wire a boom's length from the hinge
        let base = pos + DVec3::new(0.0, -0.3, 3.0);
        assert!(((p.tip - base).length() - 6.5).abs() < 0.01);
    }

    #[test]
    fn a_hand_takes_the_pole_off_its_wire() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        let pos = DVec3::new(20.0, 0.0, 0.0);
        let goal = Contact { span: 0, s: 65.0, side: 0 };
        run(&mut p, &net, pos, 4.0, Rope::Aim { goal: Some(goal), auto: false }, 0.0, &mut ev);
        run(&mut p, &net, pos, 2.0, Rope::Idle, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::OnWire);
        // the mouse takes it: it leaves the wire and is in the hand
        run(&mut p, &net, pos, 0.5, Rope::Aim { goal: None, auto: false }, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::Hand, "{:?}", p.mode);
        assert!(!p.closed);
        assert!(ev.iter().any(|e| matches!(e.kind, PoleEventKind::Dewired(DewireCause::Rope))));
    }

    #[test]
    fn with_no_wire_near_the_tip_goes_where_the_hand_points() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        let pos = DVec3::new(20.0, 0.0, 0.0);
        // (the bus heads east, as in `run`) a point to the side and up, away from the wire
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        let base = p.base_at(pos, &rot);
        let aim = base + (rot * DVec3::new(2.5, -3.0, 4.0)).normalize() * p.def.length as f64;
        p.free_aim = Some(aim);
        run(&mut p, &net, pos, 6.0, Rope::Aim { goal: None, auto: false }, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::Hand);
        let tip = p.tip_at(pos, &rot);
        assert!((tip - aim).length() < 0.7, "{tip:?} vs {aim:?}");
    }

    #[test]
    fn a_pole_held_just_under_the_wire_rises_into_it_when_let_go() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        let pos = DVec3::new(20.0, 0.0, 0.0);
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        let base = p.base_at(pos, &rot);
        // the cursor points at a spot half a metre under the pole's wire (5.5 m up, 0.3 m to the
        // right of the axis), a few centimetres to the side of it, a boom's length off
        let aim = DVec3::new(base.x - 6.0, -0.3 + 0.08, 5.0);
        p.free_aim = Some(base + (aim - base).normalize() * p.def.length as f64);
        run(&mut p, &net, pos, 3.0, Rope::Aim { goal: None, auto: false }, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::Hand, "{:?}", p.mode);
        // (the hand follows without shaking: the angles hold still)
        let (th, ps) = (p.theta, p.psi);
        run(&mut p, &net, pos, 0.5, Rope::Aim { goal: None, auto: false }, 0.0, &mut ev);
        assert!((p.theta - th).abs() < 1e-3 && (p.psi - ps).abs() < 1e-3);
        // let go, the spring lifts it and the nudge swings it on to the wire
        p.free_aim = None;
        for _ in 0..180 {
            // (the rig points the nudge at the wire above the tip each step)
            p.steer_to = net.candidates(p.tip, 2.5).into_iter().filter(|(c, _)| net.wire_point(c).0.y < 0.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| net.wire_point(&x.0).0);
            run(&mut p, &net, pos, 1.0 / 60.0, Rope::Idle, 0.0, &mut ev);
        }
        assert_eq!(p.mode, PoleMode::OnWire, "{:?} {:?}", p.mode, ev.iter().map(|e| e.kind).collect::<Vec<_>>());
    }

    #[test]
    fn a_pole_on_the_wire_follows_a_moving_bus_and_presses_with_its_spring() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        let pos = DVec3::new(20.0, 0.0, 0.0);
        run(&mut p, &net, pos, 4.0, Rope::Aim { goal: Some(Contact { span: 0, s: 65.0, side: 0 }), auto: false }, 0.0, &mut ev);
        run(&mut p, &net, pos, 2.0, Rope::Idle, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::OnWire);
        // drive east at 8 m/s for 10 s
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        let mut x = 20.0;
        for _ in 0..600 {
            let span = PoseSpan { pos0: DVec3::new(x, 0.0, 0.0), rot0: rot, pos1: DVec3::new(x + 8.0 / 60.0, 0.0, 0.0), rot1: rot, accel: glam::Vec3::ZERO };
            x += 8.0 / 60.0;
            p.step(&net, &span, 1.0 / 60.0, Rope::Idle, 100.0, 0, &mut ev);
        }
        assert_eq!(p.mode, PoleMode::OnWire, "{:?}", ev.iter().map(|e| e.kind).collect::<Vec<_>>());
        // the shoe presses the wire with about the spring's rated force
        assert!((p.force - 120.0).abs() < 40.0, "force {}", p.force);
        assert!((p.slide - 8.0).abs() < 1.0);
    }

    #[test]
    fn drifting_sideways_pulls_the_pole_off() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        let pos = DVec3::new(20.0, 0.0, 0.0);
        run(&mut p, &net, pos, 4.0, Rope::Aim { goal: Some(Contact { span: 0, s: 65.0, side: 0 }), auto: false }, 0.0, &mut ev);
        run(&mut p, &net, pos, 2.0, Rope::Idle, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::OnWire);
        // the bus slides 5 m to the side: further than the boom can stand off the wire
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        for k in 0..200 {
            let y = (k as f64 * 0.03).min(5.0);
            let span = PoseSpan::still(DVec3::new(20.0, y, 0.0), rot);
            p.step(&net, &span, 1.0 / 60.0, Rope::Idle, 0.0, 0, &mut ev);
            if p.mode != PoleMode::OnWire {
                break;
            }
        }
        assert_ne!(p.mode, PoleMode::OnWire);
        assert!(ev.iter().any(|e| matches!(e.kind, PoleEventKind::Dewired(DewireCause::Lateral) | PoleEventKind::Dewired(DewireCause::OutOfReach))), "{:?}", ev.iter().map(|e| e.kind).collect::<Vec<_>>());
    }

    #[test]
    fn a_pulled_rope_lowers_the_pole_to_its_hook() {
        let net = straight();
        let mut p = pole();
        let mut ev = Vec::new();
        let pos = DVec3::new(20.0, 0.0, 0.0);
        run(&mut p, &net, pos, 4.0, Rope::Aim { goal: Some(Contact { span: 0, s: 65.0, side: 0 }), auto: false }, 0.0, &mut ev);
        run(&mut p, &net, pos, 2.0, Rope::Idle, 0.0, &mut ev);
        assert_eq!(p.mode, PoleMode::OnWire);
        run(&mut p, &net, pos, 3.0, Rope::Pull, 200.0, &mut ev);
        assert_eq!(p.mode, PoleMode::Down);
        let kinds: Vec<_> = ev.iter().map(|e| e.kind).collect();
        assert!(kinds.iter().any(|k| matches!(k, PoleEventKind::Dewired(DewireCause::Rope))));
        assert!(kinds.iter().any(|k| matches!(k, PoleEventKind::Latched)));
        // pulling it off under 200 A struck an arc
        assert!(ev.iter().any(|e| matches!(e.kind, PoleEventKind::Dewired(_)) && e.amps > 100.0));
    }

    #[test]
    fn a_sharp_kink_taken_fast_dewires_but_slowly_does_not() {
        // a wire bending 25 degrees to the left at x = 60
        let net = WireNet::parse("[trolley_section]\nid 1\nlive 1\n[trolley_span]\nsection 1\np -50 0 5.5\np 60 0 5.5\np 100 18.65 5.5\n");
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        for (speed, should_dewire) in [(11.0, true), (3.0, false)] {
            let mut p = pole();
            let mut ev = Vec::new();
            let mut x = 20.0;
            run(&mut p, &net, DVec3::new(x, 0.0, 0.0), 4.0, Rope::Aim { goal: Some(Contact { span: 0, s: 65.0, side: 0 }), auto: false }, 0.0, &mut ev);
            run(&mut p, &net, DVec3::new(x, 0.0, 0.0), 2.0, Rope::Idle, 0.0, &mut ev);
            assert_eq!(p.mode, PoleMode::OnWire);
            // drive straight on past the bend
            let steps = ((60.0 - x + 12.0) / speed * 60.0) as usize;
            for _ in 0..steps {
                let span = PoseSpan { pos0: DVec3::new(x, 0.0, 0.0), rot0: rot, pos1: DVec3::new(x + speed / 60.0, 0.0, 0.0), rot1: rot, accel: glam::Vec3::ZERO };
                x += speed / 60.0;
                p.step(&net, &span, 1.0 / 60.0, Rope::Idle, 0.0, 0, &mut ev);
            }
            let kinked = ev.iter().any(|e| matches!(e.kind, PoleEventKind::Dewired(DewireCause::Kink)));
            assert_eq!(kinked, should_dewire, "speed {speed}: {:?}", ev.iter().map(|e| e.kind).collect::<Vec<_>>());
        }
    }

    #[test]
    fn an_uneven_wire_opens_the_contact_at_speed_but_not_at_walking_pace() {
        let net = straight();
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        let mut results = Vec::new();
        for speed in [1.5f64, 13.0] {
            let mut p = pole();
            p.roughness = 0.006;
            let mut ev = Vec::new();
            let mut x = 0.0;
            run(&mut p, &net, DVec3::new(x, 0.0, 0.0), 4.0, Rope::Aim { goal: Some(Contact { span: 0, s: 45.0, side: 0 }), auto: false }, 0.0, &mut ev);
            run(&mut p, &net, DVec3::new(x, 0.0, 0.0), 2.0, Rope::Idle, 0.0, &mut ev);
            ev.clear();
            for _ in 0..300 {
                let span = PoseSpan { pos0: DVec3::new(x, 0.0, 0.0), rot0: rot, pos1: DVec3::new(x + speed / 60.0, 0.0, 0.0), rot1: rot, accel: glam::Vec3::ZERO };
                x += speed / 60.0;
                p.step(&net, &span, 1.0 / 60.0, Rope::Idle, 150.0, 0, &mut ev);
            }
            results.push(ev.iter().filter(|e| matches!(e.kind, PoleEventKind::ContactOpened)).count());
        }
        assert_eq!(results[0], 0, "{results:?}");
        assert!(results[1] > 0, "{results:?}");
    }
}

#[cfg(test)]
mod switch_tests {
    use super::*;

    /// A frog at the origin, the toe to the west, the straight way east and a branch leaving to
    /// the north-east; a bus heading east on the straight way with a pole on the wire behind it.
    fn run(branch: bool, secs: usize) -> (Pole, Vec<PoleEvent>) {
        let net = WireNet::parse(
            "[trolley_section]\nid 1\nlive 1\n[trolley_span]\nsection 1\np -50 0 5.5\np 0 0 5.5\n\
             [trolley_span]\nsection 1\np 0 0 5.5\np 25 0 5.5\np 50 0 5.5\n\
             [trolley_span]\nsection 1\np 0 0 5.5\np 10 3 5.5\np 20 8 5.5\np 30 14 5.5\np 40 21 5.5\n\
             [trolley_switch]\npos 0 0 5.5\nmode fixed\n",
        );
        net.set_switch(0, branch);
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        let def = PoleDef { side: 1, base: glam::Vec3::new(0.3, 0.0, 3.0), length: 6.5, ..Default::default() };
        let mut p = Pole::new(def, 7);
        p.roughness = 0.0;
        let mut ev = Vec::new();
        let pos = DVec3::new(-20.0, 0.0, 0.0);
        let goal = Contact { span: 0, s: 30.0, side: 0 };
        for _ in 0..240 {
            p.step(&net, &PoseSpan::still(pos, rot), 1.0 / 60.0, Rope::Aim { goal: Some(goal), auto: false }, 0.0, 0, &mut ev);
        }
        for _ in 0..120 {
            p.step(&net, &PoseSpan::still(pos, rot), 1.0 / 60.0, Rope::Idle, 0.0, 0, &mut ev);
        }
        assert_eq!(p.mode, PoleMode::OnWire);
        ev.clear();
        let mut x = -20.0;
        for _ in 0..secs * 60 {
            let span = PoseSpan { pos0: DVec3::new(x, 0.0, 0.0), rot0: rot, pos1: DVec3::new(x + 3.0 / 60.0, 0.0, 0.0), rot1: rot, accel: glam::Vec3::ZERO };
            x += 3.0 / 60.0;
            p.step(&net, &span, 1.0 / 60.0, Rope::Idle, 0.0, 0, &mut ev);
        }
        (p, ev)
    }

    #[test]
    fn a_shoe_goes_the_way_the_frog_is_set_without_chatter() {
        // straight: the shoe passes the frog once and stays on the wire the bus follows
        let (p, ev) = run(false, 16);
        let frogs = ev.iter().filter(|e| matches!(e.kind, PoleEventKind::FrogPassed { branch: false, .. })).count();
        assert_eq!(frogs, 1, "{:?}", ev.iter().map(|e| e.kind).collect::<Vec<_>>());
        assert_eq!(p.mode, PoleMode::OnWire);
        assert_eq!(p.contact.unwrap().span, 1);
        // the branch: the shoe takes it, once ...
        let (p, ev) = run(true, 10);
        let frogs = ev.iter().filter(|e| matches!(e.kind, PoleEventKind::FrogPassed { branch: true, .. })).count();
        assert_eq!(frogs, 1, "{:?}", ev.iter().map(|e| e.kind).collect::<Vec<_>>());
        assert_eq!(p.contact.unwrap().span, 2);
        // ... and the bus going straight on pulls the pole off the diverging wire
        let (p, ev) = run(true, 30);
        assert_ne!(p.mode, PoleMode::OnWire);
        assert!(ev.iter().any(|e| matches!(e.kind, PoleEventKind::Dewired(DewireCause::Lateral) | PoleEventKind::Dewired(DewireCause::OutOfReach))), "{:?}", ev.iter().map(|e| e.kind).collect::<Vec<_>>());
    }

    /// A toe cut into short spans (a map's junction by a crossover): the plate lies 9 m before
    /// the frog over the joint, and a shoe passing it is told so once; ends drawn up to a metre
    /// apart are still one frog.
    #[test]
    fn the_plate_is_found_over_a_joint_and_loose_ends_make_a_frog() {
        let net = WireNet::parse(
            "[trolley_section]\nid 1\nlive 1\n[trolley_span]\nsection 1\np -50 0 5.5\np -4 0 5.5\n\
             [trolley_span]\nsection 1\np -4 0 5.5\np 0.4 0.3 5.5\n\
             [trolley_span]\nsection 1\np -0.3 0.5 5.5\np 25 0 5.5\np 50 0 5.5\n\
             [trolley_span]\nsection 1\np 0.2 -0.6 5.5\np 10 3 5.5\np 20 8 5.5\np 30 14 5.5\np 40 21 5.5\n\
             [trolley_switch]\npos 0 0 5.5\nmode fixed\ntrigger 9\n",
        );
        assert_eq!(net.switches.len(), 1);
        let (span, s, towards) = net.switches[0].plate;
        assert_eq!((span, towards), (0, 1));
        // (46 m of toe span, 4.6 m of the span after it: the plate is 4.4 m from the first one's end)
        assert!((s - 41.6).abs() < 1.0, "{s}");
        let rot = DMat3::from_rotation_z(-std::f64::consts::FRAC_PI_2);
        let def = PoleDef { side: 1, base: glam::Vec3::new(0.3, 0.0, 3.0), length: 6.5, ..Default::default() };
        let mut p = Pole::new(def, 7);
        p.roughness = 0.0;
        let mut ev = Vec::new();
        let pos = DVec3::new(-20.0, 0.0, 0.0);
        let goal = Contact { span: 0, s: 30.0, side: 0 };
        for _ in 0..240 {
            p.step(&net, &PoseSpan::still(pos, rot), 1.0 / 60.0, Rope::Aim { goal: Some(goal), auto: false }, 0.0, 0, &mut ev);
        }
        for _ in 0..120 {
            p.step(&net, &PoseSpan::still(pos, rot), 1.0 / 60.0, Rope::Idle, 0.0, 0, &mut ev);
        }
        assert_eq!(p.mode, PoleMode::OnWire);
        ev.clear();
        let mut x = -20.0;
        for _ in 0..8 * 60 {
            let span = PoseSpan { pos0: DVec3::new(x, 0.0, 0.0), rot0: rot, pos1: DVec3::new(x + 3.0 / 60.0, 0.0, 0.0), rot1: rot, accel: glam::Vec3::ZERO };
            x += 3.0 / 60.0;
            p.step(&net, &span, 1.0 / 60.0, Rope::Idle, 0.0, 0, &mut ev);
        }
        let plates = ev.iter().filter(|e| matches!(e.kind, PoleEventKind::Plate { .. })).count();
        assert_eq!(plates, 1, "{:?}", ev.iter().map(|e| e.kind).collect::<Vec<_>>());
    }
}
