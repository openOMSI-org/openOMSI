use super::*;

pub(super) struct Wheel {
    /// The steering wheel mesh (index into the vehicle's meshes).
    pub(super) mesh: usize,
    /// Its `[newanim]` (index among the mesh's), variable and factor: the wheel's angle in
    /// degrees is their product (the factor as the bus has it now, see
    /// `DriverCar::anim_factor`: the player's own may be scaled by `cab_wheel_rotation`).
    pub(super) anim: usize,
    pub(super) var: String,
    pub(super) factor: f32,
    /// The turning axis, pointing at the driver.
    pub(super) axis: Vec3,
    /// Centre, axis (towards the driver) and the rim's up and right in the wheel's plane,
    /// at rest, in the model frame; the rim radius the hands hold.
    pub(super) centre: Vec3,
    pub(super) up: Vec3,
    pub(super) right: Vec3,
    pub(super) radius: f32,
    /// The rim's own thickness (its tube's radius, m).
    pub(super) tube: f32,
}

/// A hand on the rim: the point it holds, as an angle on the wheel (the angle seen from the
/// seat less the wheel's), or its way to a new hold.
#[derive(Clone, Copy, Default)]
pub(super) struct Hand {
    pub(super) on_rim: f32,
    pub(super) mv: Option<Regrip>,
}

/// A hand let go of the rim and moving to hold it again: from and to angles seen from the
/// seat (degrees), progress 0..1 and duration (s).
#[derive(Clone, Copy)]
pub(super) struct Regrip {
    from: f32,
    to: f32,
    t: f32,
    dur: f32,
    /// How fast the hand moves along the rim as it lets go and as it takes hold (degrees
    /// over the whole move): it leaves the rim moving with it and meets it the same way,
    /// not stopping dead in one frame.
    v0: f32,
    v1: f32,
}

impl Regrip {
    fn new(from: f32, to: f32, dur: f32, rate: f32) -> Regrip {
        let v = (rate * dur).clamp(-60.0, 60.0);
        Regrip {
            from,
            to,
            t: 0.0,
            dur,
            v0: v,
            v1: v,
        }
    }
}

impl Hand {
    /// The angle the hand is at, seen from the seat, and how far it is lifted off the rim
    /// (0..1).
    pub(super) fn seen(&self, theta: f32) -> (f32, f32) {
        match self.mv {
            Some(m) => {
                // a Hermite curve: from and to, leaving and arriving with the rim's speed
                let t = m.t.clamp(0.0, 1.0);
                let (t2, t3) = (t * t, t * t * t);
                let a = m.from * (2.0 * t3 - 3.0 * t2 + 1.0)
                    + m.v0 * (t3 - 2.0 * t2 + t)
                    + m.to * (3.0 * t2 - 2.0 * t3)
                    + m.v1 * (t3 - t2);
                (a, (t * std::f32::consts::PI).sin().powi(2))
            }
            None => (self.on_rim + theta, 0.0),
        }
    }

    /// How far its fingers are open (0 closed round the rim).
    pub(super) fn open(&self) -> f32 {
        self.mv
            .map(|m| (m.t.clamp(0.0, 1.0) * std::f32::consts::PI).sin().powi(2) * 0.45)
            .unwrap_or(0.0)
    }
}

/// Where the hands go this frame (model frame): the wrists, how each hand lies (wrist to
/// knuckles, the way the palm faces) and the points of the rim they hold.
pub(super) struct Targets {
    pub(super) grips: [Vec3; 2],
    pub(super) frames: [(Vec3, Vec3); 2],
    pub(super) tubes: [Vec3; 2],
}

const WHEEL_RATE_LIMIT: f32 = 1800.0;
const WHEEL_DELTA_BASE: f32 = 45.0;
const WHEEL_MESH_TOLERANCE: f32 = 35.0;

#[derive(Default)]
pub(super) struct WheelAngleTracker {
    sign: f32,
    angle: Option<f32>,
    last_variable: Option<f32>,
    last_mesh: Option<f32>,
}

impl WheelAngleTracker {
    pub(super) fn sign(&self) -> f32 {
        self.sign
    }

    fn update(&mut self, variable: Option<f32>, mesh: f32, dt: f32) -> f32 {
        let variable = variable.filter(|a| a.is_finite());
        if self.sign == 0.0 {
            if let Some(value) = variable {
                if mesh.abs() > 10.0 && mesh.abs() < 170.0 {
                    self.sign = if wrap(value - mesh).abs() <= wrap(-value - mesh).abs() {
                        1.0
                    } else {
                        -1.0
                    };
                }
            }
        }

        let Some(mut angle) = self.angle else {
            self.angle = Some(mesh);
            self.last_variable = variable;
            self.last_mesh = Some(mesh);
            return mesh;
        };

        let mesh_delta = self.last_mesh.map(|last| wrap(mesh - last)).unwrap_or(0.0);
        let variable_delta = variable
            .zip(self.last_variable)
            .filter(|_| self.sign != 0.0)
            .map(|(now, last)| (now - last) * self.sign);
        let max_delta = WHEEL_DELTA_BASE + WHEEL_RATE_LIMIT * dt.max(0.0).min(1.0);
        let variable_delta = variable_delta.filter(|delta| {
            delta.is_finite()
                && delta.abs() <= max_delta
                && wrap(*delta - mesh_delta).abs() <= WHEEL_MESH_TOLERANCE
        });

        angle += variable_delta.unwrap_or(mesh_delta);
        self.angle = Some(angle);
        self.last_variable = variable;
        self.last_mesh = Some(mesh);
        angle
    }
}

fn wrap(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

impl DriverFigure {
    /// The wheel's frame as it stands now (model frame): centre, axis towards the driver, up
    /// and right in its plane. An adjustable column (the SD202's) tilts the whole wheel, and
    /// holds reckoned in the resting frame put the fists beside the tilted rim.
    pub(super) fn wheel_frame(w: &Wheel, v: &impl DriverCar) -> (Mat4, Vec3, Vec3, Vec3, Vec3) {
        let turn = v.mesh_transform(w.mesh).unwrap_or(Mat4::IDENTITY);
        let centre = turn.transform_point3(w.centre);
        let axis = turn.transform_vector3(w.axis).normalize_or(w.axis);
        let mut up = (Vec3::Z - axis * axis.dot(Vec3::Z)).normalize_or_zero();
        if up.length_squared() < 0.5 {
            up = w.up;
        }
        let right = up.cross(axis).normalize_or(w.right);
        let right = if right.dot(w.right) < 0.0 {
            -right
        } else {
            right
        };
        (turn, centre, axis, up, right)
    }

    /// How far the wheel is turned, seen from the driver (degrees clockwise): from its
    /// variable (all the turns of a lock to lock), its sign found from the mesh as turned now
    /// (within one turn).
    pub(super) fn wheel_angle(&mut self, v: &impl DriverCar, dt: f32) -> Option<f32> {
        let w = self.wheel.as_ref()?;
        let (turn, _, _, up, right) = Self::wheel_frame(w, v);
        let up_now = turn.transform_vector3(w.up).normalize_or(w.up);
        let mesh_angle = up_now.dot(right).atan2(up_now.dot(up)).to_degrees();
        let factor = v.anim_factor(w.mesh, w.anim).unwrap_or(w.factor);
        let variable = v.var(&w.var).map(|value| value * factor);
        self.theta = self.wheel_tracker.update(variable, mesh_angle, dt);
        Some(self.theta)
    }

    /// Move the hands with the wheel turned to `theta`: each turns with the rim while it
    /// stays within its range; one turned out of it lets go and takes the rim again further
    /// back while the other holds on (the rim slides through a hand only while the other is
    /// off it); held still a while, the wheel gets its hands back at their rest.
    pub(super) fn steer_hands(&mut self, theta: f32, dt: f32) {
        // OMSI_DRIVER_HANDS=<left>,<right>: both hands held at these angles (grip close-ups)
        if let Some(a) = omsi_cfg::flags::OMSI_DRIVER_HANDS.var().and_then(|s| {
            let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 2).then(|| [v[0], v[1]])
        }) {
            self.hands = [0, 1].map(|k| Hand {
                on_rim: a[k] - theta,
                mv: None,
            });
            self.hands_placed = true;
            return;
        }
        if !self.hands_placed {
            for k in 0..2 {
                self.hands[k] = Hand {
                    on_rim: REST[k] - theta,
                    mv: None,
                };
            }
            self.hands_placed = true;
            self.last_theta = theta;
            self.still = 0.0;
            return;
        }
        let turned = theta - self.last_theta;
        self.last_theta = theta;
        if dt > 0.0 {
            self.rate += (turned / dt - self.rate) * (dt / 0.15).min(1.0);
        }
        if turned.abs() < 12.0 * dt.max(1e-3) {
            self.still += dt;
        } else {
            self.still = 0.0;
        }
        for k in 0..2 {
            if let Some(m) = &mut self.hands[k].mv {
                m.t = (m.t + dt / m.dur).clamp(0.0, 1.0);
                if m.t >= 1.0 {
                    self.hands[k] = Hand {
                        on_rim: m.to - theta,
                        mv: None,
                    };
                }
            }
        }
        let inside = |k: usize, a: f32| a >= RANGE[k].0 && a <= RANGE[k].1;
        // The hand that is off at the gear lever holds nothing: it takes no part in the
        // shuffling, and the other one must not let go meanwhile.
        let away = self.away();
        // With a shift waiting to start, no hand sets out on a new regrip (the one that
        // stays on the rim would be busy just when the other is to leave it).
        let freeze = self.shift.pending;
        // the hand furthest out of its range goes first
        let mut order = [0usize, 1];
        let out_by = |h: &Hand, k: usize| {
            let a = h.on_rim + theta;
            (RANGE[k].0 - a).max(a - RANGE[k].1)
        };
        if out_by(&self.hands[1], 1) > out_by(&self.hands[0], 0) {
            order = [1, 0];
        }
        // A driver lets go a moment before the hand gets to the end of its reach, not after it:
        // the faster the wheel turns, the sooner.
        let lead = (self.rate * REGRIP_LEAD).clamp(-LEAD_MAX, LEAD_MAX);
        for k in order {
            if self.hands[k].mv.is_some() || away[k] {
                continue;
            }
            let a = self.hands[k].on_rim + theta;
            let ahead = a + lead;
            if inside(k, a) && inside(k, ahead) {
                continue;
            }
            let (lo, hi) = RANGE[k];
            // With the other hand at the gear lever this one is the wheel's only: it pushes
            // the wheel round as far as it goes and only then takes it again further back.
            let lone = away[1 - k];
            // Always keep both hands on wheel unless shifting (rare and brief)
            let can_regrip = self.hands[1 - k].mv.is_none()
                && !freeze
                && (!lone || a > hi + ONE_HAND_OVER || a < lo - ONE_HAND_OVER);
            // (the other hand keeps hold meanwhile, sliding if it has to)
            if can_regrip {
                // back against the turn, towards the other end of the range
                let up = if a > hi {
                    true
                } else if a < lo {
                    false
                } else {
                    ahead > hi
                };
                let to = if up {
                    lo + REGRIP_BACK
                } else {
                    hi - REGRIP_BACK
                };
                let to = if (to - REST[k]).abs() > 90.0 {
                    REST[k]
                } else {
                    to
                };
                // (from where the hand is now: a hand that slid is already past the range)
                let from = a;
                // an unhurried reach back, only a little quicker the faster the wheel turns
                // (at a tenth of a second a hand flew back to its hold); a lone hand is
                // quicker, the wheel has none else
                let dur = (0.38 + (to - from).abs() / 350.0) / (1.0 + self.rate.abs() / 2000.0);
                let dur = dur.max(0.32);
                self.hands[k] = Hand {
                    on_rim: self.hands[k].on_rim,
                    mv: Some(Regrip::new(from, to, dur, self.rate)),
                };
            } else {
                // the rim slides through the hand past its range: the hand goes on with it
                // less and less over SLIP degrees (stopped dead at a limit, it jumped)
                let (lo, hi) = RANGE[k];
                let before = self.hands[k].on_rim + theta - turned;
                let over = (lo - before).max(before - hi).max(0.0);
                let outward = (before > hi && turned > 0.0) || (before < lo && turned < 0.0);
                let follow = if outward {
                    1.0 - smooth(over / SLIP)
                } else {
                    1.0
                };
                let now = (before + turned * follow).clamp(lo - SLIP, hi + SLIP);
                self.hands[k].on_rim = now - theta;
            }
        }
        if self.still > SETTLE_AFTER
            && self.hands.iter().all(|h| h.mv.is_none())
            && !away.iter().any(|&a| a)
        {
            let far = |k: usize| (self.hands[k].on_rim + theta - REST[k]).abs();
            let k = if far(0) >= far(1) { 0 } else { 1 };
            if far(k) > 22.0 {
                let from = self.hands[k].on_rim + theta;
                self.hands[k].mv = Some(Regrip::new(
                    from,
                    REST[k],
                    0.45 + (REST[k] - from).abs() / 300.0,
                    0.0,
                ));
                self.still = 0.0;
            }
        }
    }

    /// Where the wrists go and how the hands lie: each fist closed round the rim at the angle
    /// its hand is at, the wrist continuing the forearm; a hand on its way to a new hold
    /// lifted off the rim towards the driver.
    /// `dt` eases each hand's frame towards the one its hold asks for (0 takes it at once).
    pub(super) fn hand_targets(&mut self, v: &impl DriverCar, dt: f32) -> Option<Targets> {
        let w = self.wheel.as_ref()?;
        let (_, centre, axis, up, right) = Self::wheel_frame(w, v);
        let h = self.heading.to_radians();
        let fwd = Vec3::new(h.sin(), h.cos(), 0.0);
        let mut t = Targets {
            grips: [Vec3::ZERO; 2],
            frames: [(Vec3::ZERO, Vec3::ZERO); 2],
            tubes: [Vec3::ZERO; 2],
        };
        let lever = self.lever_target(v, fwd);
        let handbrake = self.handbrake_target(v, fwd);
        let door_button = self.door_button_target(v, fwd);
        for k in 0..2 {
            let (seen, lift) = self.hands[k].seen(self.theta);
            let a = seen.to_radians();
            let radial = (up * a.cos() + right * a.sin()).normalize_or(up);
            let along = (right * a.cos() - up * a.sin()).normalize_or(right);
            let tube = centre + radial * w.radius + (axis * 0.85 + radial * 0.3) * (LIFT * lift);
            // the forearm: from the elbow as last posed, else from about where it will be
            let elbow = self.elbows.map(|e| e[k]).unwrap_or(
                self.hip + Vec3::Z * 0.2 - fwd * 0.05 + (tube - centre).with_z(0.0) * 0.5,
            );
            let fore = (tube - elbow).normalize_or(fwd);
            // How far the fist is rolled round the rim (0: the fingers out over the rim's
            // outer edge, the palm on it from the driver's side; 90: the knuckles turned away
            // from the driver, the palm towards the wheel's middle, as round an upright
            // wheel's sides): as near the forearm's line as it goes, within what a wrist does.
            // Where the forearm runs along the rim (the sides of a flat wheel) its line says
            // nothing about the roll and the fist keeps to the plain grip over the outer edge
            // (taken from the line alone, the palm flipped over there from frame to frame).
            let (e1, e2) = (radial, -axis);
            let proj = fore - along * along.dot(fore);
            let want = proj
                .dot(e2)
                .atan2(proj.dot(e1))
                .to_degrees()
                .clamp(ROLL.0, ROLL.1);
            let want = ROLL_PLAIN + (want - ROLL_PLAIN) * smooth((proj.length() - 0.2) / 0.4);
            let roll = match self.rolls[k] {
                Some(r) if dt > 0.0 => r + (want - r) * (1.0 - (-dt / ROLL_EASE).exp()),
                _ => want,
            };
            self.rolls[k] = Some(roll);
            let (sr, cr) = roll.to_radians().sin_cos();
            let across = (e1 * cr + e2 * sr).normalize_or(e1);
            // the rim held diagonally, the knuckles turned towards the forearm's line
            let dir = turn_towards(across, fore, DIAGONAL.to_radians());
            // a right hand's thumb lies towards the top of the wheel on its right side, a
            // left hand's likewise on its left: the fingers close round the rim the way that
            // puts it there (the other way round was a mirrored hand, the thumb pointing down
            // the rim and the fingers held in over the top)
            let palm = dir.cross(along).normalize_or(-axis);
            let palm = (palm - dir * dir.dot(palm)).normalize_or(palm);
            // The hand that works the gear lever leaves the rim for the knob (a little
            // lifted on the way) and turns into the hold the knob asks for.
            let mut tube = tube;
            let (mut dir, mut palm) = (dir, palm);
            if let Some((lh, e, knob, ldir, lpalm)) = lever {
                if lh == k {
                    tube = tube.lerp(knob, e) + Vec3::Z * (0.04 * (e * std::f32::consts::PI).sin());
                    let q = frame_quat(dir, palm)
                        .slerp(frame_quat(ldir, lpalm), e)
                        .normalize();
                    dir = q * Vec3::X;
                    palm = q * Vec3::Y;
                }
            }
            if let Some((lh, e, knob, ldir, lpalm)) = handbrake {
                if lh == k {
                    tube = tube.lerp(knob, e) + Vec3::Z * (0.04 * (e * std::f32::consts::PI).sin());
                    let q = frame_quat(dir, palm)
                        .slerp(frame_quat(ldir, lpalm), e)
                        .normalize();
                    dir = q * Vec3::X;
                    palm = q * Vec3::Y;
                }
            }
            if let Some((lh, e, button, ldir, lpalm)) = door_button {
                let brake_busy = self
                    .handbrake
                    .as_ref()
                    .is_some_and(|hb| hb.hand == k && self.handbrake_w > 0.0);
                let gear_busy = self
                    .shifter
                    .as_ref()
                    .is_some_and(|sh| sh.hand == k && self.shift.w > 0.0);
                if lh == k && !brake_busy && !gear_busy {
                    tube =
                        tube.lerp(button, e) + Vec3::Z * (0.045 * (e * std::f32::consts::PI).sin());
                    let q = frame_quat(dir, palm)
                        .slerp(frame_quat(ldir, lpalm), e)
                        .normalize();
                    dir = q * Vec3::X;
                    palm = q * Vec3::Y;
                }
            }
            // the hand turns into its new frame over a moment, not in one frame
            let (dir, palm) = match self.frames[k] {
                Some((d0, p0)) if dt > 0.0 => {
                    let q0 = glam::Quat::from_mat3(&glam::Mat3::from_cols(d0, p0, d0.cross(p0)))
                        .normalize();
                    let q1 =
                        glam::Quat::from_mat3(&glam::Mat3::from_cols(dir, palm, dir.cross(palm)))
                            .normalize();
                    let q = q0.slerp(q1, 1.0 - (-dt / FRAME_EASE).exp()).normalize();
                    (q * Vec3::X, q * Vec3::Y)
                }
                _ => (dir, palm),
            };
            self.frames[k] = Some((dir, palm));
            let knuckle = tube - palm * self.grip_radius;
            t.grips[k] = knuckle - dir * self.knuckles;
            t.frames[k] = (dir, palm);
            t.tubes[k] = tube;
        }
        Some(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_angle_unwraps_mesh_rotation() {
        let mut tracker = WheelAngleTracker::default();
        assert_eq!(tracker.update(Some(0.0), 0.0, 0.0), 0.0);
        assert!((tracker.update(Some(170.0), 170.0, 0.1) - 170.0).abs() < 0.01);
        assert!((tracker.update(Some(190.0), -170.0, 0.1) - 190.0).abs() < 0.01);
    }

    #[test]
    fn wheel_angle_rejects_a_variable_spike_then_recovers() {
        let mut tracker = WheelAngleTracker::default();
        tracker.update(Some(0.0), 0.0, 0.0);
        assert!((tracker.update(Some(20.0), 20.0, 0.1) - 20.0).abs() < 0.01);
        assert!((tracker.update(Some(1000.0), 30.0, 0.1) - 30.0).abs() < 0.01);
        assert!((tracker.update(Some(1010.0), 40.0, 0.1) - 40.0).abs() < 0.01);
    }

    #[test]
    fn wheel_angle_bridges_missing_variable_without_a_reacquisition_jump() {
        let mut tracker = WheelAngleTracker::default();
        tracker.update(Some(0.0), 0.0, 0.0);
        tracker.update(Some(10.0), 10.0, 0.1);
        assert!((tracker.update(None, 20.0, 0.1) - 20.0).abs() < 0.01);
        assert!((tracker.update(Some(20.0), 20.0, 0.1) - 20.0).abs() < 0.01);
        assert!((tracker.update(Some(30.0), 30.0, 0.1) - 30.0).abs() < 0.01);
    }

    #[test]
    fn wheel_angle_calibrates_negative_variable_direction() {
        let mut tracker = WheelAngleTracker::default();
        tracker.update(Some(0.0), 0.0, 0.0);
        assert!((tracker.update(Some(35.0), -35.0, 0.1) + 35.0).abs() < 0.01);
        assert_eq!(tracker.sign(), -1.0);
        assert!((tracker.update(Some(45.0), -45.0, 0.1) + 45.0).abs() < 0.01);
    }

    #[test]
    fn regrip_curve_starts_and_ends_on_the_requested_holds() {
        let mut move_hand = Regrip::new(-20.0, 55.0, 0.5, 20.0);
        let hand = |mv| Hand {
            on_rim: 0.0,
            mv: Some(mv),
        };
        assert!((hand(move_hand).seen(0.0).0 + 20.0).abs() < 0.01);
        move_hand.t = 0.5;
        let (middle, lift) = hand(move_hand).seen(0.0);
        assert!((middle - 17.5).abs() < 0.01);
        assert!((lift - 1.0).abs() < 0.01);
        assert!((hand(move_hand).open() - 0.45).abs() < 0.01);
        move_hand.t = 1.0;
        let (end, lift) = hand(move_hand).seen(0.0);
        assert!((end - 55.0).abs() < 0.01);
        assert!(lift.abs() < 0.01);
    }
}
