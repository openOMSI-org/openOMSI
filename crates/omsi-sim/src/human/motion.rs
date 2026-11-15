//! How a person moves from frame to frame: the gait with its planted and swinging feet,
//! sitting, reaching, looking round, and landing on the floor below.

use super::*;

impl Pose {
    pub fn new(seed: u32) -> Pose {
        let mut p = Pose {
            rng: seed.wrapping_mul(0x9E37_79B9) | 1,
            init: false,
            frame: 0,
            origin: DVec3::ZERO,
            heading: 0.0,
            phase: 0.0,
            gait: false,
            walk: 0.0,
            speed: 0.0,
            accel: 0.0,
            turn: 0.0,
            vel: DVec2::ZERO,
            feet: [Foot::new(), Foot::new()],
            fidget: [Vec2::ZERO; 2],
            fidget_t: 0.0,
            sit: 0.0,
            seat: Vec3::ZERO,
            seat_pelvis: None,
            reach: 0.0,
            reach_at: Vec3::new(0.3, 0.5, 1.1),
            grip: 0.0,
            grip_at: [Vec3::new(-0.2, 0.4, 1.0), Vec3::new(0.2, 0.4, 1.0)],
            grip_frame: None,
            grip_extra: 0.0,
            hold: 0.0,
            head: Vec2::ZERO,
            glance: Vec2::ZERO,
            glance_t: 0.0,
            shift: 0.0,
            shift_to: 0.0,
            shift_t: 0.0,
            breath: 0.0,
            lean: Vec2::ZERO,
            lean_v: Vec2::ZERO,
            drop: 0.0,
            body_floor: 0.0,
            style: [0.0; 4],
            pace: 1.0,
            idle_from: Idle::Hang,
            idle_to: Idle::Hang,
            idle_mix: 1.0,
            idle_t: 0.0,
            catch_ups: 0,
            shuffles: 0,
            landed: false,
        };
        for k in 0..4 {
            p.style[k] = p.rand() * 2.0 - 1.0;
        }
        p.breath = p.rand() * TAU;
        p.glance_t = p.rand() * 3.0;
        p.shift_t = p.rand() * 5.0;
        p.pace = 0.88 + 0.24 * p.rand();
        p.idle_to = Idle::pick(p.rand());
        p.idle_from = p.idle_to;
        p.idle_t = 12.0 + p.rand() * 28.0;
        p.fidget_t = 8.0 + p.rand() * 10.0;
        p
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// 0 standing … 1 seated.
    pub fn sit_amount(&self) -> f32 {
        self.sit
    }

    /// Sitting down or getting up is under way: the person should not walk off yet.
    pub fn settling(&self) -> bool {
        self.sit > 0.02 && self.sit < 0.98
    }

    /// The state in a line, for `OMSI_DEBUG_POSE`.
    pub fn describe(&self) -> String {
        let foot = |f: &Foot| {
            if f.planted {
                format!("down at ({:.2}, {:.2}, {:.2})", f.pos.x, f.pos.y, f.pos.z)
            } else {
                format!(
                    "in the air {:.0}%{}",
                    f.t * 100.0,
                    if f.walk { " (walk)" } else { "" }
                )
            }
        };
        format!(
            "origin ({:.2}, {:.2}, {:.2}) heading {:.0} turning {:.0}/s frame {} speed {:.2} gait {} walk {:.2} phase {:.2} sit {:.2} reach {:.2} hold {:.2} head ({:.0}, {:.0}) floor {:.2} drop {:.3} catch-ups {} shuffles {}; left {}; right {}",
            self.origin.x, self.origin.y, self.origin.z, self.heading, self.turn, self.frame, self.speed, self.gait, self.walk, self.phase, self.sit, self.reach, self.hold, self.head.x, self.head.y, self.body_floor, self.drop, self.catch_ups, self.shuffles, foot(&self.feet[0]), foot(&self.feet[1])
        )
    }

    /// Whether foot `side` (0 left, 1 right) is on the floor.
    pub fn planted(&self, side: usize) -> bool {
        self.feet[side.min(1)].planted
    }

    /// A foot was put down during the last [`Pose::advance`]: one footfall, for the step
    /// sound. (The gait plants a foot only while walking, so standing still is silent.)
    pub fn landed(&self) -> bool {
        self.landed
    }

    /// Nothing quick is going on (standing or sitting still: breathing, shifting, glancing),
    /// so a lower update rate will not show.
    pub fn calm(&self) -> bool {
        !self.gait
            && self.walk < 0.05
            && self.feet.iter().all(|f| f.planted)
            && (self.sit == 0.0 || self.sit == 1.0)
            && (self.reach == 0.0 || self.reach == 1.0)
            && (self.hold == 0.0 || self.hold == 1.0)
    }

    /// Quick steps taken so far to catch up with a foot left behind.
    pub fn catch_ups(&self) -> u32 {
        self.catch_ups
    }

    pub(super) fn to_local(&self, p: DVec3) -> Vec3 {
        let d = p - self.origin;
        let h = self.heading.to_radians();
        let (s, c) = (h.sin(), h.cos());
        // inverse of local → frame (x' = x cos + y sin, y' = -x sin + y cos)
        Vec3::new(
            (d.x * c - d.y * s) as f32,
            (d.x * s + d.y * c) as f32,
            d.z as f32,
        )
    }

    fn place(origin: DVec3, heading: f64, l: Vec3) -> DVec3 {
        let h = heading.to_radians();
        let (s, c) = (h.sin(), h.cos());
        let (x, y) = (l.x as f64, l.y as f64);
        origin + DVec3::new(x * c + y * s, -x * s + y * c, l.z as f64)
    }

    pub(super) fn sample_floor(input: &PoseInput, at: DVec2, fallback: f64) -> f64 {
        match input.floor {
            Some(f) => f(at)
                .filter(|z| (z - fallback).abs() < 1.2)
                .unwrap_or(fallback),
            None => fallback,
        }
    }

    /// Where a foot's resting place is in the floor frame for the body at `origin`/`heading`.
    pub(super) fn rest_target(
        &self,
        rig: &Rig,
        side: usize,
        origin: DVec3,
        heading: f64,
        walking: bool,
    ) -> (DVec3, f64) {
        let r = rig.rest_foot(side);
        let (x, y) = if walking {
            let stride = self.speed / (rig.cadence(self.speed) * self.pace * 0.5);
            let beta = stance_fraction(self.speed);
            (r.x * 0.62, r.y + 0.4 * beta * stride)
        } else {
            (
                r.x + SIDE[side] * 0.035 * self.style[2] + self.fidget[side].x,
                r.y + self.fidget[side].y,
            )
        };
        let toe_out = SIDE[side] as f64 * if walking { 5.0 } else { 9.0 };
        (
            Self::place(origin, heading, Vec3::new(x, y, 0.0)),
            heading + toe_out,
        )
    }

    fn reset_feet(&mut self, rig: &Rig, input: &PoseInput) {
        for side in 0..2 {
            let (p, yaw) = self.rest_target(rig, side, input.origin, input.heading, false);
            let f = &mut self.feet[side];
            *f = Foot::new();
            f.pos = DVec3::new(p.x, p.y, Self::sample_floor(input, p.truncate(), input.origin.z));
            f.yaw = yaw;
        }
        self.body_floor = 0.0;
    }

    /// Ankle of a planted foot pitched by `pitch` degrees (toes up positive), floor frame.
    pub(super) fn stance_ankle(rig: &Rig, f: &Foot, pitch: f32) -> DVec3 {
        let h = f.yaw.to_radians();
        let (s, c) = (h.sin(), h.cos());
        let dir = DVec3::new(s, c, 0.0);
        let p = pitch.to_radians();
        let ah = rig.ankle_h as f64;
        // pivot on the heel when the toes are up, on the toe joint when the heel is up (the
        // toes stay flat)
        let (pivot, ph) = if p > 0.0 {
            (rig.heel as f64, 0.0)
        } else {
            (rig.ball as f64, rig.ball_h as f64)
        };
        let (sp, cp) = (p.sin() as f64, p.cos() as f64);
        // the ankle seen from the pivot, turned by the pitch
        let (a, u) = (-pivot, ah - ph);
        let along = a * cp - u * sp;
        let up = a * sp + u * cp;
        f.pos + dir * (pivot + along) + DVec3::Z * (ph + up)
    }

    /// Advance the animation by `dt` seconds.
    pub fn advance(&mut self, rig: &Rig, input: &PoseInput, dt: f32) {
        // nothing sensible to follow: keep the last state
        if !input.origin.is_finite() || !input.heading.is_finite() || !dt.is_finite() {
            return;
        }
        let mut clean = *input;
        if !clean.velocity.is_finite() {
            clean.velocity = DVec2::ZERO;
        }
        if !clean.sway.is_finite() {
            clean.sway = Vec2::ZERO;
        }
        clean.seat = clean.seat.filter(|v| v.is_finite());
        clean.look = clean.look.filter(|v| v.is_finite());
        clean.reach = clean.reach.filter(|v| v.is_finite());
        clean.grips = clean.grips.filter(|g| g[0].is_finite() && g[1].is_finite());
        if !clean.hold.is_finite() {
            clean.hold = 0.0;
        }
        let input = &clean;
        let dt = dt.clamp(0.0, 0.25);
        let floor_changed = self.init && ((input.origin.z - self.origin.z).abs() > 1e-6 || input.frame != self.frame);
        self.landed = false;
        // a new floor frame (boarding, getting off): keep the feet where they are
        if self.init && input.frame != self.frame {
            self.follow_floor_frame(input);
        }
        // first frame, or a jump no step could follow: stand where they are
        let jumped = self.init
            && ((input.origin - self.origin).truncate().length() > 1.5
                || (input.origin.z - self.origin.z).abs() > 1.0);
        if !self.init || jumped {
            self.init = true;
            self.frame = input.frame;
            self.origin = input.origin;
            self.heading = input.heading;
            self.vel = input.velocity;
            self.reset_feet(rig, input);
            if input.activity == Activity::Sit && input.seat.is_some() {
                self.sit = 1.0;
            }
            self.gait = false;
        }
        let turned = angle_diff(self.heading, input.heading) as f32;
        if dt > 1e-5 {
            self.turn += (turned / dt - self.turn) * ease_k(dt, 0.15);
            let v_now = input.velocity.length() as f32;
            let fwd = {
                let h = input.heading.to_radians();
                DVec2::new(h.sin(), h.cos())
            };
            let a = ((input.velocity - self.vel).dot(fwd) as f32) / dt;
            self.accel += (a.clamp(-4.0, 4.0) - self.accel) * ease_k(dt, 0.25);
            self.speed += (v_now - self.speed) * ease_k(dt, 0.12);
        }
        self.vel = input.velocity;
        self.origin = input.origin;
        self.heading = input.heading;
        if floor_changed {
            self.refresh_support(input);
        }
        let v_in = input.velocity.length() as f32;

        // sitting down and getting up
        let wants_sit = input.activity == Activity::Sit && input.seat.is_some();
        if let Some(s) = input.seat {
            self.seat = s;
            self.seat_pelvis = input.seat_pelvis.filter(|p| p.is_finite());
        }
        if wants_sit {
            self.sit = approach(self.sit, 1.0, dt / 1.3);
        } else {
            self.sit = approach(self.sit, 0.0, dt / 1.1);
        }
        let seated = self.sit > 0.0;

        self.ease_hands(input, seated, dt);

        self.idle_motion(rig, input, dt);

        // the gait: on while moving, off when stopped or sitting
        let moving = if self.gait {
            self.speed > 0.06 || v_in > 0.08
        } else {
            self.speed > 0.12 || v_in > 0.2
        };
        let gait_now = moving && !seated;
        let walk_to = if gait_now {
            (self.speed / 1.1).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.walk += (walk_to - self.walk) * ease_k(dt, 0.2);
        let speed = self.speed.max(0.08);
        let cadence = rig.cadence(speed) * self.pace * 0.5;
        let beta = stance_fraction(speed);
        if gait_now && !self.gait {
            // start with the foot that is further back (or the one already in the air)
            let back = |f: &Foot| {
                if f.planted {
                    self.to_local(f.pos).y
                } else {
                    f32::MIN
                }
            };
            let first = if back(&self.feet[0]) <= back(&self.feet[1]) {
                0
            } else {
                1
            };
            self.phase = (beta - 0.001 + if first == 1 { 0.5 } else { 0.0 }) % 1.0;
        }
        if !gait_now && self.gait {
            // stopping: a foot in the air lands beside the other one, in the time its
            // swing had left
            for f in self.feet.iter_mut() {
                if !f.planted && f.walk {
                    f.walk = false;
                    f.dur = (1.0 - beta) / cadence;
                }
            }
        }
        self.gait = gait_now;
        if self.gait {
            self.catch_up(rig, beta);
        }
        let prev_phase = self.phase;
        if self.gait {
            self.phase = (self.phase + dt * cadence) % 1.0;
        }
        for side in 0..2 {
            let offset = if side == 0 { 0.0 } else { 0.5 };
            let u0 = (prev_phase + offset) % 1.0;
            let u1 = (self.phase + offset) % 1.0;
            let (origin, heading) = (self.origin, self.heading);
            if !self.gait {
                if !self.feet[side].planted {
                    self.fly(rig, input, side, dt, origin, heading);
                }
                continue;
            }
            // lift off at the end of the stance (or a little late, when the other foot
            // was still in the air), from the push-off pitch
            if self.feet[side].planted
                && u1 >= beta
                && u1 < beta + 0.3
                && (self.feet[1 - side].planted || run_factor(speed) > 0.5)
            {
                let from_pitch = if self.feet[side].walk {
                    toe_pitch(beta, beta, self.walk)
                } else {
                    0.0
                };
                let f = &mut self.feet[side];
                f.from_ankle = Self::stance_ankle(rig, f, from_pitch);
                f.from_yaw = f.yaw;
                f.from_pitch = from_pitch;
                f.from_floor = f.pos.z;
                f.planted = false;
                f.walk = true;
                // the swing starts at its beginning even when the lift-off came late (the
                // other foot still in the air): begun at the phase's own progress, up to
                // 0.3 of the way, the foot jumped 15-22 cm towards its landing in a frame
                f.t0 = ((u1 - beta) / (1.0 - beta)).clamp(0.0, 0.9);
                f.t = 0.0;
                f.to_sampled = DVec2::splat(f64::MAX);
                f.land_z = f.pos.z;
                f.land_wait = 0.0;
                f.lift = (0.035 + 0.035 * (speed / 1.3).min(1.3) + 0.11 * run_factor(speed)) * rig.scale;
            }
            if self.feet[side].planted {
                continue;
            }
            if !self.feet[side].walk {
                // a shuffle step still in the air when the walk began
                self.fly(rig, input, side, dt, origin, heading);
                continue;
            }
            if u1 < beta || u1 < u0 {
                self.land(input, side);
                continue;
            }
            // on its way: to where the body will be when it lands
            let tp = ((u1 - beta) / (1.0 - beta)).clamp(0.0, 1.0);
            let remain = (1.0 - tp) * (1.0 - beta) / cadence;
            let t0 = self.feet[side].t0;
            let t = ((tp - t0) / (1.0 - t0).max(0.1)).clamp(0.0, 1.0);
            let pred_origin = origin + (self.vel * remain as f64).extend(0.0);
            // (the turn to come, but not more than a step's worth: at a quick turn the whole
            // remaining swing's turn rate put the landing half a metre to the side, and the
            // legs were flung out sideways)
            let pred_heading = heading + (self.turn * remain).clamp(-25.0, 25.0) as f64;
            let (to, yaw) = self.rest_target(rig, side, pred_origin, pred_heading, true);
            let f = &mut self.feet[side];
            f.to = to;
            f.to_yaw = yaw;
            f.t = f.t.max(t);
        }
        // standing: step when a foot is left too far from where it belongs
        if !self.gait && !seated && self.feet.iter().all(|f| f.planted) {
            self.step_to_rest(rig);
        }
        // last resort: a planted foot the legs cannot reach (pushed away while the other
        // one was in the air) shuffles along the floor until they can
        if !seated {
            self.shuffle_unreachable(rig, beta);
        }
        self.follow_feet_floor(dt);
        // the pelvis may rise again only gradually
        self.drop = approach(self.drop, 0.0, dt * 0.4);
        self.swing_floor(input, dt);
    }

    /// A new floor frame (boarding, getting off): the feet stay where they are in the world.
    fn follow_floor_frame(&mut self, input: &PoseInput) {
        let dyaw = angle_diff(self.heading, input.heading);
        for k in 0..2 {
            let f = self.feet[k];
            let (lp, la, lt) = (
                self.to_local(f.pos),
                self.to_local(f.from_ankle),
                self.to_local(f.to),
            );
            let floor = f.to_floor - self.origin.z + input.origin.z;
            let from_floor = f.from_floor - self.origin.z + input.origin.z;
            let land_z = f.land_z - self.origin.z + input.origin.z;
            let g = &mut self.feet[k];
            g.pos = Self::place(input.origin, input.heading, lp);
            g.from_ankle = Self::place(input.origin, input.heading, la);
            g.to = Self::place(input.origin, input.heading, lt);
            g.to_floor = floor;
            g.from_floor = from_floor;
            g.land_z = land_z;
            g.yaw += dyaw;
            g.from_yaw += dyaw;
            g.to_yaw += dyaw;
            g.to_sampled = DVec2::splat(f64::MAX);
        }
        self.frame = input.frame;
        self.origin = input.origin;
        self.heading = input.heading;
        self.vel = input.velocity;
    }

    /// The hands on their way to the cash desk or a grip, and back.
    fn ease_hands(&mut self, input: &PoseInput, seated: bool, dt: f32) {
        // reaching for the cash desk
        match input.reach {
            Some(r) if !seated => {
                // the hand moves on to a new target, it does not jump there
                if self.reach <= 0.0 {
                    self.reach_at = r;
                } else {
                    self.reach_at += (r - self.reach_at) * ease_k(dt, 0.22);
                }
                self.reach = approach(self.reach, 1.0, dt / 0.55);
            }
            _ => self.reach = approach(self.reach, 0.0, dt / 0.6),
        }
        match input.grips {
            Some(g) => {
                self.grip_at = g;
                self.grip_frame = input.grip_frames;
                self.grip_extra += (input.grip_lean - self.grip_extra) * ease_k(dt, 0.3);
                self.grip = approach(self.grip, 1.0, dt / 0.8);
            }
            None => self.grip = approach(self.grip, 0.0, dt / 0.8),
        }
        let hold_to = if seated {
            0.0
        } else {
            input.hold.clamp(0.0, 1.0)
        };
        self.hold = approach(
            self.hold,
            hold_to,
            dt / if hold_to > self.hold { 0.7 } else { 1.3 },
        );
    }

    /// What a body does whatever its feet do: the balance against the bus, breathing, weight
    /// shifts, fidgeting, glances and the head turning to what it looks at.
    fn idle_motion(&mut self, rig: &Rig, input: &PoseInput, dt: f32) {
        // balance against the bus: a damped spring pulled by the floor's acceleration
        let pull = -input.sway.clamp(Vec2::splat(-4.0), Vec2::splat(4.0));
        let k = 38.0;
        let damp = 7.5;
        let acc = (pull - self.lean) * k - self.lean_v * damp;
        self.lean_v += acc * dt;
        self.lean += self.lean_v * dt;

        // the way of standing: kept a while, then another, changed over a second (only while
        // standing: walking, the arms swing)
        self.idle_mix = approach(self.idle_mix, 1.0, dt / 1.1);
        if self.walk < 0.2 && self.sit <= 0.0 {
            self.idle_t -= dt;
            if self.idle_t <= 0.0 && self.idle_mix >= 1.0 {
                self.idle_t = 12.0 + self.rand() * 28.0;
                let next = Idle::pick(self.rand());
                if next != self.idle_to {
                    self.idle_from = self.idle_to;
                    self.idle_to = next;
                    self.idle_mix = 0.0;
                }
            }
        }
        // breathing, weight shifts, fidgeting, glances
        self.breath = (self.breath + dt * TAU / (3.6 + 0.6 * self.style[0])) % (TAU * 64.0);
        self.shift_t -= dt;
        if self.shift_t <= 0.0 {
            self.shift_t = 4.0 + self.rand() * 7.0;
            self.shift_to = (self.rand() * 2.0 - 1.0) * 0.03 * rig.scale;
        }
        self.shift += (self.shift_to - self.shift) * ease_k(dt, 1.2);
        self.fidget_t -= dt;
        if self.fidget_t <= 0.0 {
            self.fidget_t = 9.0 + self.rand() * 14.0;
            let side = (self.rand() * 2.0) as usize % 2;
            self.fidget[side] =
                Vec2::new((self.rand() - 0.5) * 0.05, (self.rand() - 0.5) * 0.12) * rig.scale;
        }
        self.glance_t -= dt;
        if self.glance_t <= 0.0 {
            let walking = self.walk > 0.3;
            self.glance_t = if walking {
                2.0 + self.rand() * 4.0
            } else {
                1.5 + self.rand() * 5.0
            };
            self.glance = if self.rand() < 0.35 {
                Vec2::ZERO
            } else if walking {
                Vec2::new((self.rand() - 0.5) * 50.0, -4.0 - self.rand() * 8.0)
            } else {
                Vec2::new((self.rand() - 0.5) * 110.0, (self.rand() - 0.6) * 18.0)
            };
        }
        let look_to = match input.look {
            Some(p) => {
                let from = rig.neck + Vec3::Z * 0.08;
                let d = p - from;
                let yaw = d.x.atan2(d.y).to_degrees();
                let pitch = d.z.atan2(d.truncate().length()).to_degrees();
                if yaw.abs() > 125.0 {
                    Vec2::new(0.0, pitch.clamp(-35.0, 25.0))
                } else {
                    Vec2::new(yaw.clamp(-80.0, 80.0), pitch.clamp(-38.0, 28.0))
                }
            }
            // (on the phone: looking down at it, glancing up now and then)
            None if self.idle_to == Idle::Phone && self.walk < 0.2 && self.sit <= 0.0 && self.glance.x.abs() < 40.0 => Vec2::new(self.glance.x * 0.15, -32.0),
            None if self.walk > 0.3 => Vec2::new(self.glance.x.clamp(-25.0, 25.0), self.glance.y),
            None => self.glance,
        };
        let step = 220.0 * dt;
        let k_head = ease_k(dt, 0.14);
        let want = self.head + (look_to - self.head) * k_head;
        self.head.x = approach(self.head.x, want.x, step);
        self.head.y = approach(self.head.y, want.y, step);
    }

    /// Walking: a foot left too far behind takes its own step now (a quick step when stranded on another floor; see the comment inside).
    fn catch_up(&mut self, rig: &Rig, beta: f32) {
        // a foot left too far behind (the body was pushed on or sped up faster than the
        // stride): a quick step catches up rather than the hips sinking to reach it. A
        // foot left on another floor (a stair the body has gone down from, the bus floor
        // after stepping off) goes first: it lifts the body's floor, so the other foot
        // could never reach down and stepped again and again while this one was dragged
        // along a flight up.
        if self.feet.iter().all(|f| f.planted) {
            let reach = (rig.thigh + rig.shin) * 0.997;
            let mut worst: Option<(usize, f32)> = None;
            for side in 0..2 {
                let f = &self.feet[side];
                let u = (self.phase + if side == 0 { 0.0 } else { 0.5 }) % 1.0;
                if f.walk && u >= beta - 0.04 {
                    // lifts now anyway
                    continue;
                }
                let pitch = if f.walk {
                    stance_pitch(u, beta, self.walk)
                } else {
                    0.0
                };
                let a = self.to_local(Self::stance_ankle(rig, f, pitch));
                let hip = rig.hip[side] + Vec3::Z * self.body_floor;
                let horiz = (a - hip).truncate().length();
                let stranded = (f.pos.z - self.origin.z).abs() as f32;
                let need = if stranded > STRANDED {
                    1.0 + stranded
                } else {
                    (hip.z - a.z) - (reach * reach - horiz * horiz).max(0.0).sqrt()
                };
                if need > CATCH_UP && worst.map(|w| need > w.1).unwrap_or(true) {
                    worst = Some((side, need));
                }
            }
            if let Some((side, need)) = worst {
                if need <= 1.0 {
                    self.phase = (beta + if side == 0 { 0.0 } else { 0.5 }) % 1.0;
                    self.catch_ups += 1;
                    return;
                }
                let f = &mut self.feet[side];
                f.from_ankle = Self::stance_ankle(rig, f, 0.0);
                f.from_yaw = f.yaw;
                f.from_pitch = 0.0;
                f.from_floor = f.pos.z;
                f.planted = false;
                f.walk = false;
                f.t = 0.0;
                f.dur = 0.26;
                f.lift = 0.05 * rig.scale;
                f.to_sampled = DVec2::splat(f64::MAX);
                f.land_z = f.pos.z;
                f.land_wait = 0.0;
                self.catch_ups += 1;
            }
        }
    }

    /// Standing: a step for the foot furthest from where it belongs.
    fn step_to_rest(&mut self, rig: &Rig) {
        let mut worst: Option<(usize, f32)> = None;
        for side in 0..2 {
            let (rp, ryaw) = self.rest_target(rig, side, self.origin, self.heading, false);
            let off = (self.feet[side].pos - rp).truncate().length() as f32;
            let yaw_off = angle_diff(self.feet[side].yaw, ryaw).abs() as f32;
            let reach = self.to_local(self.feet[side].pos).truncate().length();
            let stranded = (self.feet[side].pos.z - self.origin.z).abs() as f32;
            let bad = (off / 0.13)
                .max(yaw_off / 12.0)
                .max(reach / (0.6 * rig.leg()))
                .max(stranded / STRANDED);
            if bad > 1.0 && worst.map(|w| bad > w.1).unwrap_or(true) {
                worst = Some((side, bad));
            }
        }
        if let Some((side, bad)) = worst {
            let f = &mut self.feet[side];
            f.from_ankle = Self::stance_ankle(rig, f, 0.0);
            f.from_yaw = f.yaw;
            f.from_pitch = 0.0;
            f.from_floor = f.pos.z;
            f.planted = false;
            f.walk = false;
            f.t = 0.0;
            f.dur = if bad > 2.5 { 0.28 } else { 0.38 };
            f.lift = 0.045 * rig.scale;
            f.to_sampled = DVec2::splat(f64::MAX);
            f.land_z = f.pos.z;
            f.land_wait = 0.0;
        }
    }

    /// A planted foot the legs cannot reach slides along the floor until they can.
    fn shuffle_unreachable(&mut self, rig: &Rig, beta: f32) {
        let reach = (rig.thigh + rig.shin) * 0.997;
        for side in 0..2 {
            let f = self.feet[side];
            if !f.planted {
                continue;
            }
            let u = (self.phase + if side == 0 { 0.0 } else { 0.5 }) % 1.0;
            let pitch = if f.walk && self.gait && u < beta + 0.3 {
                stance_pitch(u.min(beta), beta, self.walk)
            } else {
                0.0
            };
            let a = self.to_local(Self::stance_ankle(rig, &f, pitch));
            let hip = rig.hip[side] + Vec3::Z * self.body_floor;
            let off = (a - hip).truncate();
            let down = hip.z - a.z - MAX_DROP;
            let horiz_max = (reach * reach - down * down).max(0.0).sqrt();
            if off.length() > horiz_max + 0.01 && off.length() > 1e-3 {
                let pull = off.normalize() * (off.length() - horiz_max);
                let h = self.heading.to_radians();
                let (sn, cs) = (h.sin(), h.cos());
                let (px, py) = (pull.x as f64, pull.y as f64);
                self.feet[side].pos -= DVec3::new(px * cs + py * sn, -px * sn + py * cs, 0.0);
                self.shuffles += 1;
            }
        }
    }

    /// The floor under the body follows the planted feet.
    fn follow_feet_floor(&mut self, dt: f32) {
        // the floor under the body follows the planted feet
        let mut floor_sum = 0.0;
        let mut floor_n = 0.0;
        for f in &self.feet {
            let z = if f.planted {
                f.pos.z
            } else {
                f.land_z.max(f.pos.z.min(f.land_z))
            };
            floor_sum += (z - self.origin.z) as f32;
            floor_n += 1.0;
        }
        let floor_to = if floor_n > 0.0 {
            floor_sum / floor_n
        } else {
            0.0
        };
        let floor_to = floor_to.clamp(-0.6, 0.6);
        self.body_floor += (floor_to - self.body_floor) * ease_k(dt, 0.18);
    }

    fn fly(
        &mut self,
        rig: &Rig,
        input: &PoseInput,
        side: usize,
        dt: f32,
        origin: DVec3,
        heading: f64,
    ) {
        // to beside the other foot, or in the walk to where the body will be
        let remain = ((1.0 - self.feet[side].t) * self.feet[side].dur) as f64;
        let ahead = if self.gait { remain } else { 0.15 };
        let (to, yaw) = self.rest_target(
            rig,
            side,
            origin + (self.vel * ahead).extend(0.0),
            heading + (self.turn * 0.25) as f64,
            self.gait,
        );
        let f = &mut self.feet[side];
        f.to = to;
        f.to_yaw = yaw;
        f.t = (f.t + dt / f.dur.max(0.05)).min(1.0);
        if f.t >= 1.0 {
            // a foot whose landing height is still on its way stays up a moment longer
            // (put down at once it would drop or jump the rest)
            if (f.to_floor - f.land_z).abs() > 0.01 && f.land_wait < 0.2 {
                f.land_wait += dt;
                return;
            }
            self.land(input, side);
        }
    }

    fn land(&mut self, input: &PoseInput, side: usize) {
        let fallback = self.origin.z;
        let f = &mut self.feet[side];
        let floor = if (f.to.truncate() - f.to_sampled).length() < 0.05 {
            f.to_floor
        } else {
            Self::sample_floor(input, f.to.truncate(), fallback)
        };
        // (within a centimetre of where the swing brought it, or it gave up waiting)
        let floor = if (floor - f.land_z).abs() < 0.05 { floor } else { f.land_z + (floor - f.land_z).clamp(-0.05, 0.05) };
        f.pos = DVec3::new(f.to.x, f.to.y, floor);
        f.yaw = f.to_yaw;
        f.planted = true;
        f.t = 0.0;
        self.landed = true;
    }

    /// Update the landing floor height of the swinging feet.
    fn swing_floor(&mut self, input: &PoseInput, dt: f32) {
        let fallback = self.origin.z;
        for f in self.feet.iter_mut() {
            if !f.planted && (f.to.truncate() - f.to_sampled).length() > 0.05 {
                f.to_floor = Self::sample_floor(input, f.to.truncate(), fallback);
                f.to_sampled = f.to.truncate();
            }
            if !f.planted {
                let gap = f.to_floor - f.land_z;
                let step = (gap * ease_k(dt, 0.05) as f64).clamp(-1.8 * dt as f64, 1.8 * dt as f64);
                f.land_z = if gap.abs() < 0.002 { f.to_floor } else { f.land_z + step };
            }
        }
    }
}
