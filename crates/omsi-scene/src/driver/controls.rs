use super::*;

impl DriverFigure {
    /// Which hands are off at the gear lever (and so hold nothing).
    /// How far the wrists are (m, the worst of the two) from where a driver's would be: the
    /// shoulder-to-wrist distance over what an arm bent as a driver holds it allows
    /// (`ARM_RATIO` of its length). The shoulders are taken from the hip and the lean; the
    /// elbows and wrists are as posed (person frame).
    pub(super) fn arm_excess(&self, elbow: &[Vec3], wrist: &[Vec3]) -> f32 {
        let h = self.heading.to_radians();
        let fwd = Vec3::new(h.sin(), h.cos(), 0.0);
        let hip = self.to_person(self.hip + fwd * self.slide);
        let l = self.lean.to_radians();
        let up = Vec3::new(0.0, l.sin(), l.cos());
        let mut worst = 0.0f32;
        for k in 0..2 {
            if k >= elbow.len() || k >= wrist.len() {
                continue;
            }
            let side = if k == 1 { 1.0 } else { -1.0 };
            let shoulder = hip + up * 0.55 + Vec3::new(0.14 * side, 0.0, 0.0);
            let arm = UPPER_ARM + (wrist[k] - elbow[k]).length();
            let excess = (wrist[k] - shoulder).length() - ARM_RATIO * arm;
            if excess.is_finite() {
                worst = worst.max(excess);
            }
        }
        worst
    }

    pub(super) fn away(&self) -> [bool; 2] {
        let mut a = [false; 2];
        if let Some(sh) = &self.shifter {
            a[sh.hand] = self.shift.w > 0.0;
        }
        if let Some(hb) = &self.handbrake {
            a[hb.hand] |= self.handbrake_w > 0.0;
        }
        if let Some(button) = self.door_button_control() {
            a[button.hand] |= self.door_button_w > 0.0;
        } else {
            a[0] |= self.door_button_w > 0.0;
        }
        a
    }

    /// The reaching hand opens slightly while leaving and returning to press a door button.
    pub(super) fn door_button_open(&self, k: usize) -> f32 {
        let hand = self
            .door_button_control()
            .map(|button| button.hand)
            .unwrap_or(0);
        if k == hand && self.door_button_w > 0.0 && self.door_button_w < 1.0 {
            0.35 * (smooth(self.door_button_w) * std::f32::consts::PI)
                .sin()
                .powi(2)
        } else {
            0.0
        }
    }

    pub(super) fn door_button_control(&self) -> Option<&DoorButtonControl> {
        if let Some(mesh) = self.door_button_mesh {
            if let Some(button) = self.door_buttons.iter().find(|button| button.mesh == mesh) {
                return Some(button);
            }
        }
        if let Some(trigger) = self.door_button_trigger.as_deref() {
            if let Some(button) = self.door_buttons.iter().find(|button| {
                button
                    .event
                    .as_deref()
                    .is_some_and(|event| event.eq_ignore_ascii_case(trigger))
            }) {
                return Some(button);
            }
        }
        if self.door_button_index > 0 {
            return self
                .door_buttons
                .iter()
                .find(|button| button.group == Some(self.door_button_index))
                .or_else(|| {
                    self.door_buttons
                        .iter()
                        .find(|button| button.group.is_none())
                });
        }
        self.door_buttons.first()
    }

    pub(super) fn update_door_button(&mut self, dt: f32) {
        if !(dt > 0.0) {
            return;
        }
        let dt = dt.min(0.1);
        if self.door_button_hold > 0.0 {
            self.door_button_hold = (self.door_button_hold - dt).max(0.0);
            self.door_button_w = (self.door_button_w + dt / REACH_TIME).min(1.0);
        } else {
            self.door_button_w = (self.door_button_w - dt / BACK_TIME).max(0.0);
        }
    }

    /// How far the fingers of hand `k` are open on their way to the knob and back (0 closed).
    pub(super) fn shift_open(&self, k: usize) -> f32 {
        match &self.shifter {
            Some(sh) if sh.hand == k && self.shift.w > 0.0 && self.shift.w < 1.0 => {
                0.45 * (smooth(self.shift.w) * std::f32::consts::PI).sin().powi(2)
            }
            _ => 0.0,
        }
    }

    /// How far the hand opens on its way to or from the parking-brake handle.
    pub(super) fn handbrake_open(&self, k: usize) -> f32 {
        match &self.handbrake {
            Some(hb) if hb.hand == k && self.handbrake_w > 0.0 && self.handbrake_w < 1.0 => {
                0.45 * (smooth(self.handbrake_w) * std::f32::consts::PI)
                    .sin()
                    .powi(2)
            }
            _ => 0.0,
        }
    }

    /// Animate one pull or release when the parking-brake state changes during driving.
    /// The state at seat-in is the baseline, so a brake that starts applied does not pull
    /// the hand away from the wheel. A gear change gets priority if both controls use a hand.
    pub(super) fn update_handbrake(&mut self, v: &impl DriverCar, dt: f32) {
        let Some(hb) = self.handbrake.as_ref() else {
            return;
        };
        if !(dt > 0.0) {
            return;
        }
        let Some(value) = v.var(&hb.variable) else {
            return;
        };
        let applied = value > 0.5;
        let changed = self
            .handbrake_last_applied
            .replace(applied)
            .is_some_and(|last| last != applied);
        let gear_hand_busy = self
            .shifter
            .as_ref()
            .is_some_and(|sh| sh.hand == hb.hand && self.shift.w > 0.0);
        if changed {
            self.handbrake_hold = REACH_TIME + 0.22;
        }
        if self.handbrake_hold > 0.0 && !gear_hand_busy {
            self.handbrake_hold = (self.handbrake_hold - dt).max(0.0);
            self.handbrake_w = (self.handbrake_w + dt / REACH_TIME).min(1.0);
        } else if self.handbrake_hold <= 0.0 {
            self.handbrake_w = (self.handbrake_w - dt / BACK_TIME).max(0.0);
        }
    }

    /// Watches the lever, the clutch and the bus's speed and moves the shifting hand between
    /// the rim and the knob: `Away` -> `Reach` -> `Hold` -> `Back` -> `Away`.
    pub(super) fn update_shift(&mut self, v: &impl DriverCar, dt: f32) {
        let Some(sh) = self.shifter.as_ref() else {
            return;
        };
        if !(dt > 0.0) {
            return;
        }
        let dt = dt.min(0.1);
        let (hand, grab) = (sh.hand, sh.grab);
        let turn = v.mesh_transform(sh.mesh).unwrap_or(Mat4::IDENTITY);
        let knob = turn.transform_point3(grab);
        let vars: Vec<f32> = sh.vars.iter().map(|n| v.var(n).unwrap_or(0.0)).collect();
        let clutch = sh.clutch.as_ref().and_then(|n| v.var(n)).unwrap_or(0.0);

        // The hand left on the rim: is the wheel being turned too much for one hand?
        let other = 1 - hand;
        let a = self.hands[other].on_rim + self.theta;
        let out = (RANGE[other].0 - a).max(a - RANGE[other].1);
        let busy = self.rate.abs() > 70.0 || out > 5.0;
        let calm = self.rate.abs() < 60.0 && out <= 0.0;
        let brake_hand_free =
            self.handbrake_w <= 0.0 || self.handbrake.as_ref().is_none_or(|hb| hb.hand != hand);
        let hands_free =
            self.hands[0].mv.is_none() && self.hands[1].mv.is_none() && brake_hand_free;
        // A gear being engaged comes before the wheel: the hand that is to work the lever
        // drops whatever it was doing, only the hand that stays on the rim must not be in
        // the middle of a regrip (or the wheel would be left with no hand at all).
        let other_free = self.hands[other].mv.is_none()
            && !(self.handbrake_w > 0.0
                && self.handbrake.as_ref().is_some_and(|hb| hb.hand == other));

        let st = &mut self.shift;

        // Speed from the position: stopped for a while / moving again (with hysteresis).
        let speed = st
            .last_pos
            .map(|p| ((v.position() - p).length() / dt as f64) as f32)
            .unwrap_or(0.0);
        st.last_pos = Some(v.position());
        if speed < STOP_SPEED {
            st.still_for += dt;
        } else if speed > GO_SPEED {
            st.still_for = 0.0;
            st.stopped = false;
        }
        if st.still_for > STOP_AFTER {
            st.stopped = true;
        }
        st.cool = (st.cool - dt).max(0.0);

        // A gear is being engaged: the knob moves, a watched variable changes.
        let knob_speed = st
            .last_knob
            .map(|k| (knob - k).length() / dt)
            .unwrap_or(0.0);
        st.last_knob = Some(knob);
        let var_moved = st.last_vars.len() == vars.len()
            && st
                .last_vars
                .iter()
                .zip(&vars)
                .any(|(a, b)| (a - b).abs() > 1e-3);
        st.last_vars = vars;
        let clutch_edge = clutch > 0.5 && st.last_clutch <= 0.5;
        st.last_clutch = clutch;
        let active = knob_speed > LEVER_MOVING || var_moved;
        if active || clutch_edge {
            st.idle = 0.0;
            if matches!(st.phase, ShiftPhase::Away | ShiftPhase::Back) {
                st.pending = true;
            }
        } else {
            st.idle += dt;
        }

        // The driver's own push on the knob, for levers that do not move by themselves: it
        // starts with the shift, runs once the hand is on the knob, and fades out by itself
        // as the model's lever moves (the hand follows that instead).
        if active && !st.was_active && st.stroke >= 1.0 {
            st.stroke = 0.0;
            st.event_from = knob;
            st.event_travel = 0.0;
        }
        st.was_active = active;
        if st.stroke < 1.0 {
            st.event_travel = st.event_travel.max((knob - st.event_from).length());
            if st.phase == ShiftPhase::Hold {
                st.stroke += dt / STROKE_TIME;
            }
        }

        let want = (st.stopped && st.cool <= 0.0 && calm) || st.pending;
        // Set when the hand sets out for the knob this frame: whether for a shift.
        let mut start: Option<bool> = None;
        match st.phase {
            ShiftPhase::Away => {
                if want {
                    let go = brake_hand_free && if st.pending { other_free } else { hands_free };
                    if go {
                        start = Some(st.pending);
                        st.phase = ShiftPhase::Reach;
                        st.pending = false;
                        st.waiting = 0.0;
                    } else {
                        st.waiting += dt;
                        if st.waiting > SHIFT_WAIT {
                            st.pending = false;
                            st.waiting = 0.0;
                        }
                    }
                } else {
                    st.waiting = 0.0;
                }
            }
            ShiftPhase::Reach => {
                st.w = (st.w + dt / st.reach).min(1.0);
                if st.w >= 1.0 {
                    st.phase = ShiftPhase::Hold;
                } else if busy && st.idle > 0.8 {
                    st.phase = ShiftPhase::Back;
                    st.cool = 1.5;
                }
            }
            ShiftPhase::Hold => {
                st.w = 1.0;
                // Very brief hold: hand on knob for minimal time, then back to wheel
                if busy && st.idle > 0.8 {
                    st.phase = ShiftPhase::Back;
                    st.cool = 1.5;
                } else if !st.stopped && st.idle > HOLD_AFTER && st.stroke >= 1.0 {
                    st.phase = ShiftPhase::Back;
                }
            }
            ShiftPhase::Back => {
                if want {
                    start = Some(st.pending);
                    st.phase = ShiftPhase::Reach;
                    st.pending = false;
                } else {
                    st.w = (st.w - dt / BACK_TIME).max(0.0);
                    if st.w <= 0.0 {
                        st.phase = ShiftPhase::Away;
                        st.stroke = 1.0;
                    }
                }
            }
        }
        if let Some(event) = start {
            st.reach = if event { REACH_FAST } else { REACH_TIME };
            // (off the rim from this very frame: no regrip may start for this hand now)
            st.w = st.w.max(0.001);
            // A regrip under way is dropped where the hand is: it goes to the lever from
            // there, the wheel's shuffling not holding it back.
            if self.hands[hand].mv.is_some() {
                let (at, _) = self.hands[hand].seen(self.theta);
                self.hands[hand] = Hand {
                    on_rim: at - self.theta,
                    mv: None,
                };
            }
        }
    }

    /// Where the shifting hand is headed: its side, how far it has got (0..1, eased), the
    /// knob now, and the hand's frame there (wrist to knuckles, palm), or `None` with the
    /// hand on the rim.
    pub(super) fn lever_target(
        &self,
        v: &impl DriverCar,
        fwd: Vec3,
    ) -> Option<(usize, f32, Vec3, Vec3, Vec3)> {
        let sh = self.shifter.as_ref()?;
        let st = &self.shift;
        if st.w <= 0.0 {
            return None;
        }
        let turn = v.mesh_transform(sh.mesh).unwrap_or(Mat4::IDENTITY);
        let mut knob = turn.transform_point3(sh.grab);
        let axis = turn.transform_vector3(sh.axis).normalize_or(sh.axis);
        if st.stroke < 1.0 {
            let amp = 1.0 - smooth(st.event_travel / STROKE_FADE);
            knob += fwd * (STROKE * amp * (st.stroke.clamp(0.0, 1.0) * std::f32::consts::PI).sin());
        }
        let h = self.heading.to_radians();
        let right = Vec3::new(h.cos(), -h.sin(), 0.0);
        let side = if sh.hand == 1 { 1.0 } else { -1.0 };
        let shoulder = self.hip + fwd * self.slide + Vec3::Z * 0.55 + right * (0.14 * side);
        // The palm lies on the knob's end; the fingers point the way the arm comes from.
        let palm = -axis;
        let reach = knob - shoulder;
        let mut dir = reach - palm * reach.dot(palm);
        if dir.length_squared() < 1e-4 {
            dir = fwd - palm * fwd.dot(palm);
        }
        let dir = dir.normalize_or(fwd);
        Some((sh.hand, smooth(st.w), knob, dir, palm))
    }

    /// Where the hand holding the parking brake is headed and the frame of its grip.
    pub(super) fn handbrake_target(
        &self,
        v: &impl DriverCar,
        fwd: Vec3,
    ) -> Option<(usize, f32, Vec3, Vec3, Vec3)> {
        let hb = self.handbrake.as_ref()?;
        if self.handbrake_w <= 0.0 {
            return None;
        }
        let turn = v.mesh_transform(hb.mesh).unwrap_or(Mat4::IDENTITY);
        let lift = 0.10 * (smooth(self.handbrake_w) * std::f32::consts::PI).sin();
        let knob = turn.transform_point3(hb.grab) + Vec3::Z * lift;
        let axis = turn.transform_vector3(hb.axis).normalize_or(hb.axis);
        let h = self.heading.to_radians();
        let right = Vec3::new(h.cos(), -h.sin(), 0.0);
        let side = if hb.hand == 1 { 1.0 } else { -1.0 };
        let shoulder = self.hip + fwd * self.slide + Vec3::Z * 0.55 + right * (0.14 * side);
        let palm = -axis;
        let reach = knob - shoulder;
        let mut dir = reach - palm * reach.dot(palm);
        if dir.length_squared() < 1e-4 {
            dir = fwd - palm * fwd.dot(palm);
        }
        Some((
            hb.hand,
            smooth(self.handbrake_w),
            knob,
            dir.normalize_or(fwd),
            palm,
        ))
    }

    /// The door-button panel is normally just left of the steering wheel. Keep this target
    /// relative to the wheel so the gesture follows buses with a different seat position.
    pub(super) fn door_button_target(
        &self,
        v: &impl DriverCar,
        fwd: Vec3,
    ) -> Option<(usize, f32, Vec3, Vec3, Vec3)> {
        let wheel = self.wheel.as_ref()?;
        if self.door_button_w <= 0.0 {
            return None;
        }
        let (_, centre, axis, up, right) = Self::wheel_frame(wheel, v);
        let control = self.door_button_control();
        let hand = control.map(|button| button.hand).unwrap_or(0);
        let button = control
            .map(|button| {
                v.mesh_transform(button.mesh)
                    .unwrap_or(Mat4::IDENTITY)
                    .transform_point3(button.point)
            })
            .unwrap_or_else(|| {
                let side = if hand == 1 { 1.0 } else { -1.0 };
                centre + right * (side * 0.34) - up * 0.06 + axis * 0.12
            });
        let h = self.heading.to_radians();
        let shoulder = self.hip
            + fwd * self.slide
            + Vec3::Z * 0.55
            + Vec3::new(h.cos(), -h.sin(), 0.0) * if hand == 1 { 0.14 } else { -0.14 };
        let palm = -axis;
        let reach = button - shoulder;
        let mut dir = reach - palm * reach.dot(palm);
        if dir.length_squared() < 1e-4 {
            dir = fwd - palm * fwd.dot(palm);
        }
        Some((
            hand,
            smooth(self.door_button_w),
            button,
            dir.normalize_or(fwd),
            palm,
        ))
    }
}
