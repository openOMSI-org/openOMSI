//! Input actions: the names from `Inputs/keyboard.cfg` and how they reach a vehicle.
//!
//! Vehicle actions are script triggers with the same name; on key release the trigger
//! `<name>_off` fires. A few actions are handled by the engine itself (throttle, brake,
//! clutch, steering, views).

/// Actions the engine handles instead of forwarding to the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineAction {
    Throttle,
    ThrottleAmplify,
    Brake,
    Clutch,
    SteeringLeft,
    SteeringRight,
    SteeringNeutral,
}

pub fn engine_action(name: &str) -> Option<EngineAction> {
    Some(match name.to_ascii_lowercase().as_str() {
        "throttle" => EngineAction::Throttle,
        "throttle_amplify" => EngineAction::ThrottleAmplify,
        "brake" => EngineAction::Brake,
        "clutch" => EngineAction::Clutch,
        "steering_left" => EngineAction::SteeringLeft,
        "steering_right" => EngineAction::SteeringRight,
        "steering_neutral" => EngineAction::SteeringNeutral,
        _ => return None,
    })
}

/// Keyboard-driven analogue inputs, integrated per frame like the original (keys ramp the
/// pedal/steering position instead of setting it).
#[derive(Debug, Clone, Default)]
pub struct KeyboardAxes {
    pub throttle_key: bool,
    pub amplify_key: bool,
    pub brake_key: bool,
    pub clutch_key: bool,
    pub left_key: bool,
    pub right_key: bool,
    pub neutral_key: bool,
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steering: f32,
    /// Road speed in km/h: the steering returns to centre by itself only while rolling.
    pub speed_kmh: f32,
    /// Rate of the steering wheel (fraction per second) while it swings back on its own.
    pub steer_vel: f32,
    /// "Steering linearity": the keys turn the wheel as Omsi.exe does (0x7e64c6): the
    /// curvature changes by 0.00005 per millisecond whatever the speed, so the wheel goes at
    /// one steady pace; `lock_curvature` (`[inv_min_turnradius]`) turns that into a share
    /// of the lock.
    pub linear: bool,
    /// "Old Steering": let go, the wheel stays where it is and is turned back by hand - OMSI
    /// without `[autoCenter]`.
    pub old_steering: bool,
    /// OMSI's `[redSteerSpd]` option (off by default, as in Omsi.exe): the keys turn the
    /// wheel, and `steering_neutral` and the self-centring bring it back, slower at speed -
    /// by min(1, 1.5 e^(-0.1 v)), v in m/s (key handler 0x7e614c, frame 0x7d5124): all of
    /// the pace up to 14.6 km/h, 0.37 of it at 50.
    pub red_steer_spd: bool,
    pub lock_curvature: f32,
    /// OMSI's held brake (the default): let go, the brake stays where the key left it until
    /// the throttle key is pressed - tap the brake and it keeps that pressure. Off, the brake
    /// comes off with its key, as in most games. (The throttle never stays: see `update`.)
    pub pedal_hold: bool,
    /// The steering is on its way back to the middle (Omsi.exe +0x5d6): set by the
    /// `steering_neutral` key, cleared by a steering key.
    pub centering: bool,
}

impl KeyboardAxes {
    pub fn set(&mut self, action: EngineAction, pressed: bool) {
        match action {
            EngineAction::Throttle => self.throttle_key = pressed,
            EngineAction::ThrottleAmplify => self.amplify_key = pressed,
            EngineAction::Brake => self.brake_key = pressed,
            EngineAction::Clutch => self.clutch_key = pressed,
            EngineAction::SteeringLeft => self.left_key = pressed,
            EngineAction::SteeringRight => self.right_key = pressed,
            EngineAction::SteeringNeutral => self.neutral_key = pressed,
        }
    }

    /// Let go of every key: the window losing focus (alt-tab, a click outside it, an OS
    /// dialog) never delivers the matching key-up, so without this a throttle or steering
    /// key held at that moment stayed "pressed" forever (and, with a modifier key stuck the
    /// same way, a later plain key press could be misread as held with that modifier).
    pub fn release_all(&mut self) {
        *self = KeyboardAxes {
            throttle: self.throttle,
            brake: self.brake,
            clutch: self.clutch,
            steering: self.steering,
            speed_kmh: self.speed_kmh,
            linear: self.linear,
            old_steering: self.old_steering,
            red_steer_spd: self.red_steer_spd,
            lock_curvature: self.lock_curvature,
            pedal_hold: self.pedal_hold,
            centering: self.centering,
            ..Default::default()
        };
    }

    /// Road speed (km/h), as a number: a speed that is none made the return's step NaN, and
    /// its clamp stopped the game (#1045).
    fn speed(&self) -> f32 {
        if self.speed_kmh.is_finite() { self.speed_kmh.abs() } else { 0.0 }
    }

    /// `[redSteerSpd]`: the share of the pace left at speed (1 with it off).
    fn red(&self) -> f32 {
        if self.red_steer_spd { (1.5 * (-0.1 * self.speed() / 3.6).exp()).min(1.0) } else { 1.0 }
    }

    /// The paces (share of the lock a second) a key turns the wheel at, and it comes back
    /// at by itself.
    fn paces(&self) -> (f32, f32) {
        let v = self.speed();
        let (rate, back) = if self.linear {
            // OMSI: 0.05 of curvature a second, from the middle to the lock in
            // `[inv_min_turnradius]` / 0.05 seconds (2 s for a bus with a 10 m radius); it
            // comes back (unless Old Steering) at the same pace, as `[autoCenter]` does
            let r = (0.05 / self.lock_curvature.max(0.01)).clamp(0.05, 5.0);
            (r, r)
        } else {
            let base = 0.8 / (1.0 + v / 45.0);
            let back = base * (0.25 + 0.75 * (v / 25.0).min(1.0));
            ((back * 1.1).min(0.5 / (1.0 + v / 60.0)), back)
        };
        // `[redSteerSpd]`: the key's pace, and OMSI's return (the linear one), less at speed
        let red = self.red();
        (rate * red, if self.linear { back * red } else { back })
    }

    /// Where a wheel let go at `steering` is `dt` later, as this bus's comes back by itself:
    /// at the return's pace, easing out over the last bit so that it settles instead of
    /// stopping dead in the middle; with Old Steering it stays where the hands left it. The
    /// keys' wheel, and the one on a phone's screen (#1090: that one sprang back to the
    /// middle in a third of a second whatever the speed).
    pub fn let_go(&self, steering: f32, dt: f32) -> f32 {
        if self.old_steering {
            return steering;
        }
        let ease = (steering.abs() / 0.08).clamp(0.3, 1.0);
        let step = self.paces().1 * ease * dt;
        let s = steering - steering.clamp(-step, step);
        // (a snap from farther out was a visible jolt of the wheel and the driver's hands)
        if s.abs() < 0.0003 {
            0.0
        } else {
            s
        }
    }

    pub fn update(&mut self, dt: f32) {
        // The pedals as Omsi.exe works them from the keys (key handler sub_7e614c, frame
        // sub_7d5124): the throttle key raises the throttle at 2 a second up to 0.85 - to
        // the floor only with throttle_amplify held - and takes the brake off at once; let
        // go, the throttle falls at 1 a second. The brake key raises the brake at 1 a second
        // and takes the throttle off; let go, the brake stays where it is until the throttle
        // key is pressed, or eases off at 0.5 a second while throttle_amplify is held.
        let top = if self.amplify_key { 1.0 } else { 0.85 };
        if self.throttle_key {
            self.brake = 0.0;
            self.throttle = (self.throttle + 2.0 * dt).min(top);
        } else {
            self.throttle = (self.throttle - dt).max(0.0);
        }
        if self.brake_key {
            self.throttle = 0.0;
            self.brake = (self.brake + dt).min(1.0);
        } else if !self.pedal_hold {
            self.brake = (self.brake - 3.0 * dt).max(0.0);
        } else if self.amplify_key {
            self.brake = (self.brake - 0.5 * dt).max(0.0);
        }
        // The clutch as Omsi.exe works it from a key (0x7e648f, 0x7d59c0): pressed, the
        // pedal is down at once; let go, it comes up at 0.7 a second - a foot letting the
        // clutch in, which is what makes a gear change on the keyboard smooth.
        if self.clutch_key {
            self.clutch = 1.0;
        } else {
            self.clutch = (self.clutch - 0.7 * dt).max(0.0);
        }
        // Steering (the paces: `paces`)
        let (rate, back) = self.paces();
        let red = self.red();
        // The centring key held with the steering key towards the middle brings the wheel
        // back at both paces together, as in OMSI - the quick way back out of a full lock
        // (#1652, #851): the steering key used to end the centring, so Num 5 held with
        // Num 4 did no more than Num 4 alone. It stops in the middle.
        let one_key = self.left_key != self.right_key;
        let towards_middle = one_key
            && ((self.left_key && self.steering > 0.0) || (self.right_key && self.steering < 0.0) || self.steering == 0.0);
        if self.neutral_key && towards_middle {
            let omsi = (0.05 / self.lock_curvature.max(0.01)).clamp(0.05, 5.0) * red;
            let own = if self.old_steering { 0.0 } else { back };
            let step = (rate + omsi.max(own)) * dt;
            self.steering -= self.steering.clamp(-step, step);
            self.steer_vel = 0.0;
            self.centering = true;
            return;
        }
        if self.neutral_key {
            self.centering = true;
        }
        if self.left_key || self.right_key {
            self.centering = false;
        }
        if self.left_key && self.right_key {
            // Omsi.exe keeps the wheel where it is while both steering keys are held.
            // Releasing either key immediately hands control to the direction still held.
            self.steer_vel = 0.0;
        } else if self.left_key || self.right_key {
            let dir = if self.left_key { -1.0 } else { 1.0 };
            let own = if self.old_steering { 0.0 } else { back };
            self.steering = if self.steering * dir < 0.0 {
                let s = self.steering + dir * rate.max(own) * dt;
                if s * dir > 0.0 { 0.0 } else { s }
            } else {
                (self.steering + dir * rate * dt).clamp(-1.0, 1.0)
            };
            self.steer_vel = 0.0;
        } else if self.centering {
            // steering_neutral as in Omsi.exe (sub_7d5124 at 0x7d55d6): the wheel goes back
            // to the middle at the pace the keys turn it - in OMSI both are 0.05 of curvature
            // a second (0x7e6a58, 0x7d8a00) - in a straight line, and stays there until a
            // steering key is pressed; it used to jump to the middle, a jerk of the whole bus
            // at speed. Here the wheel comes back faster on its own while rolling: at OMSI's
            // figure alone the centring key brought the wheel back slower than letting go (#851)
            let omsi = (0.05 / self.lock_curvature.max(0.01)).clamp(0.05, 5.0) * red;
            let own = if self.old_steering { 0.0 } else { back };
            let step = omsi.max(rate).max(own) * dt;
            self.steering -= self.steering.clamp(-step, step);
            self.steer_vel = 0.0;
        } else {
            // released: back by itself (Old Steering: it stays where the hands left it)
            self.steering = self.let_go(self.steering, dt);
            self.steer_vel = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_and_old_steering_as_omsi() {
        let mut a = KeyboardAxes { linear: true, old_steering: true, lock_curvature: 0.1, speed_kmh: 50.0, ..Default::default() };
        a.right_key = true;
        for _ in 0..100 {
            a.update(0.01);
        }
        // 0.05 1/m a second against a lock of 0.1 1/m: half the lock in a second, at any speed
        assert!((a.steering - 0.5).abs() < 1e-3, "{}", a.steering);
        a.right_key = false;
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.steering - 0.5).abs() < 1e-3, "old steering: it stays");
        a.old_steering = false;
        for _ in 0..50 {
            a.update(0.01);
        }
        assert!((a.steering - 0.25).abs() < 0.02, "it comes back at the same pace: {}", a.steering);
    }

    /// A wheel let go comes back as the castor pulls it - a fifth of the lock a second
    /// standing, about half rolling at 25 km/h - or stays with Old Steering, the keys' wheel
    /// and the one on a phone's screen alike (#1090: that one was back in the middle within a
    /// third of a second at any speed).
    #[test]
    fn a_wheel_let_go_comes_back_as_the_castor_pulls_it() {
        let after = |v: f32, old: bool, secs: f32| {
            let a = KeyboardAxes { lock_curvature: 0.1, speed_kmh: v, old_steering: old, ..Default::default() };
            let mut s = 1.0;
            for _ in 0..(secs * 100.0).round() as usize {
                s = a.let_go(s, 0.01);
            }
            s
        };
        assert!((after(0.0, false, 1.0) - 0.8).abs() < 1e-3, "{}", after(0.0, false, 1.0));
        assert!((after(25.0, false, 1.0) - (1.0 - 0.8 / (1.0 + 25.0 / 45.0))).abs() < 1e-3, "{}", after(25.0, false, 1.0));
        assert_eq!(after(25.0, false, 3.0), 0.0);
        assert_eq!(after(25.0, true, 3.0), 1.0);
        // (the keys' wheel let go goes the same way)
        let mut k = KeyboardAxes { lock_curvature: 0.1, speed_kmh: 25.0, steering: 1.0, ..Default::default() };
        for _ in 0..100 {
            k.update(0.01);
        }
        assert!((k.steering - after(25.0, false, 1.0)).abs() < 1e-5, "{}", k.steering);
    }

    /// A speed that is no number (a bus whose physics went NaN) leaves the wheel coming
    /// back as standing still; its clamp stopped the game (#1045).
    #[test]
    fn a_speed_that_is_no_number_does_not_stop_the_game() {
        let mut a = KeyboardAxes { lock_curvature: 0.1, speed_kmh: f32::NAN, steering: 0.5, ..Default::default() };
        for _ in 0..10 {
            a.update(0.01);
        }
        assert!(a.steering.is_finite() && a.steering < 0.5, "{}", a.steering);
    }

    /// `[redSteerSpd]` with the steady pace: Omsi.exe's keys at speed (0x7e614c).
    #[test]
    fn red_steer_spd_slows_the_keys_at_speed() {
        for (v, want) in [(50.0, 0.5 * 1.5 * (-0.1f32 * 50.0 / 3.6).exp()), (10.0, 0.5)] {
            let mut a = KeyboardAxes { linear: true, red_steer_spd: true, lock_curvature: 0.1, speed_kmh: v, ..Default::default() };
            a.right_key = true;
            for _ in 0..100 {
                a.update(0.01);
            }
            assert!((a.steering - want).abs() < 1e-3, "{v} km/h: {} against {want}", a.steering);
        }
        assert!((0.5 * 1.5 * (-0.1f32 * 50.0 / 3.6).exp() - 0.187).abs() < 1e-3);
    }

    #[test]
    fn the_keys_turn_the_wheel_slower_the_faster_the_bus_goes() {
        let turned = |v: f32| {
            let mut a = KeyboardAxes { lock_curvature: 0.1, speed_kmh: v, ..Default::default() };
            a.right_key = true;
            for _ in 0..10 {
                a.update(0.01);
            }
            a.steering
        };
        let mut last = turned(15.0);
        for v in [25.0, 40.0, 50.0, 80.0] {
            assert!(turned(v) < last, "{v} km/h: {} against {last}", turned(v));
            last = turned(v);
        }
        for v in [0.0, 5.0, 10.0, 11.6, 15.0] {
            assert!(turned(v) <= 0.042, "{v} km/h: {}", turned(v));
        }
        assert!((turned(50.0) - 0.05 / (1.0 + 50.0 / 60.0)).abs() < 1e-4, "{}", turned(50.0));
    }

    #[test]
    fn dynamic_steering_never_turns_the_keys_faster() {
        for v in [0.0, 10.0, 15.0, 25.0, 50.0, 80.0] {
            let turned = |red: bool| {
                let mut a = KeyboardAxes { red_steer_spd: red, lock_curvature: 0.1, speed_kmh: v, ..Default::default() };
                a.right_key = true;
                for _ in 0..10 {
                    a.update(0.01);
                }
                a.steering
            };
            assert!(turned(true) <= turned(false) + 1e-6, "{v} km/h: {} against {}", turned(true), turned(false));
        }
        let mut a = KeyboardAxes { red_steer_spd: true, lock_curvature: 0.1, speed_kmh: 50.0, ..Default::default() };
        a.right_key = true;
        a.update(0.1);
        assert!((a.steering - 0.05 / (1.0 + 50.0 / 60.0) * 1.5 * (-0.1f32 * 50.0 / 3.6).exp()).abs() < 1e-4, "{}", a.steering);
    }

    #[test]
    fn the_opposite_key_straightens_the_wheel_no_slower_than_letting_go() {
        for (v, old) in [(0.0, false), (10.0, false), (30.0, false), (50.0, false), (80.0, false), (50.0, true)] {
            let back = |key: bool| {
                let mut a = KeyboardAxes { old_steering: old, lock_curvature: 0.13, speed_kmh: v, steering: -0.5, ..Default::default() };
                a.right_key = key;
                for _ in 0..12 {
                    a.update(1.0 / 60.0);
                }
                a.steering
            };
            assert!(back(true) >= back(false) - 1e-6, "{v} km/h: the key left {}, letting go {}", back(true), back(false));
            assert!(back(true) > -0.5, "{v} km/h: {}", back(true));
        }
        let mut a = KeyboardAxes { lock_curvature: 0.13, speed_kmh: 50.0, steering: 0.01, ..Default::default() };
        a.left_key = true;
        a.update(0.1);
        assert_eq!(a.steering, 0.0);
        a.update(0.1);
        assert!(a.steering < 0.0, "{}", a.steering);
    }

    /// Omsi.exe's keyboard pedals: the brake stays until the throttle key, the throttle
    /// goes up to 0.85 (1 with throttle_amplify) and comes back by itself.
    #[test]
    fn pedals_as_omsi_works_them_from_the_keys() {
        let mut a = KeyboardAxes { pedal_hold: true, ..Default::default() };
        a.brake_key = true;
        for _ in 0..30 {
            a.update(0.01);
        }
        assert!((a.brake - 0.3).abs() < 1e-3, "1 a second: {}", a.brake);
        a.brake_key = false;
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.brake - 0.3).abs() < 1e-3, "the brake stays: {}", a.brake);
        a.throttle_key = true;
        a.update(0.01);
        assert_eq!(a.brake, 0.0);
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.throttle - 0.85).abs() < 1e-3, "up to 0.85: {}", a.throttle);
        a.amplify_key = true;
        for _ in 0..20 {
            a.update(0.01);
        }
        assert!((a.throttle - 1.0).abs() < 1e-3, "amplified: {}", a.throttle);
        a.amplify_key = false;
        a.throttle_key = false;
        for _ in 0..50 {
            a.update(0.01);
        }
        assert!((a.throttle - 0.5).abs() < 1e-3, "falls at 1 a second: {}", a.throttle);
        // the other way: the brake comes off with its key
        let mut b = KeyboardAxes { pedal_hold: false, brake: 0.6, ..Default::default() };
        for _ in 0..10 {
            b.update(0.01);
        }
        assert!((b.brake - 0.3).abs() < 1e-3, "{}", b.brake);
    }

    /// The centring key brings the wheel back in a straight line at OMSI's pace, not at once.
    #[test]
    fn steering_neutral_brings_the_wheel_back_steadily() {
        let mut a = KeyboardAxes { old_steering: true, lock_curvature: 0.1, steering: 0.8, ..Default::default() };
        a.neutral_key = true;
        a.update(0.1);
        a.neutral_key = false;
        assert!((a.steering - 0.75).abs() < 1e-4, "{}", a.steering);
        for _ in 0..10 {
            a.update(0.1);
        }
        assert!((a.steering - 0.25).abs() < 1e-3, "{}", a.steering);
        for _ in 0..10 {
            a.update(0.1);
        }
        assert_eq!(a.steering, 0.0);
        // a steering key ends it
        a.steering = 0.5;
        a.right_key = true;
        a.update(0.01);
        a.right_key = false;
        a.update(0.5);
        assert!(a.steering > 0.5, "old steering stays: {}", a.steering);
    }

    /// The centring key is the quick way back, as in OMSI; it used to bring the wheel back
    /// slower than letting go (#851).
    #[test]
    fn steering_neutral_is_never_slower_than_the_keys_or_letting_go() {
        let step = 1.0 / 60.0;
        for v in [0.0, 15.0, 30.0, 50.0, 80.0] {
            let turned = |neutral: bool| {
                let mut a = KeyboardAxes { lock_curvature: 0.13, speed_kmh: v, ..Default::default() };
                a.right_key = true;
                for _ in 0..60 {
                    a.update(step);
                }
                let from = a.steering;
                a.right_key = false;
                a.neutral_key = neutral;
                a.update(step);
                a.neutral_key = false;
                for _ in 0..29 {
                    a.update(step);
                }
                (from, a.steering)
            };
            let (from, centred) = turned(true);
            let (_, let_go) = turned(false);
            assert!(centred <= let_go + 1e-6, "{v} km/h: the centring key left {centred}, letting go {let_go}");
            // half a second back at least half of what a second of the key turned
            assert!(centred <= from * 0.5 + 1e-3, "{v} km/h: {from} turned in a second, {centred} left after half a second back");
        }
        // with OMSI's steady keys it stays OMSI's pace
        let mut a = KeyboardAxes { linear: true, lock_curvature: 0.1, speed_kmh: 50.0, steering: 0.8, ..Default::default() };
        a.neutral_key = true;
        a.update(0.1);
        assert!((a.steering - 0.75).abs() < 1e-4, "{}", a.steering);
        // with Old Steering the wheel does not come back by itself: OMSI's centring pace,
        // slowed at speed with Dynamic steering
        let mut a = KeyboardAxes { old_steering: true, red_steer_spd: true, lock_curvature: 0.13, speed_kmh: 50.0, steering: 0.8, ..Default::default() };
        a.right_key = true;
        a.update(0.1);
        let key_pace = (a.steering - 0.8) / 0.1;
        a.right_key = false;
        a.steering = 0.8;
        a.neutral_key = true;
        a.update(0.1);
        let back = (0.8 - a.steering) / 0.1;
        assert!(back >= key_pace, "{back} against the keys' {key_pace}");
        assert!((back - 0.05 / 0.13 * 1.5 * (-0.1f32 * 50.0 / 3.6).exp()).abs() < 1e-3, "{back}");
    }

    /// Num 5 held with the steering key towards the middle comes back faster than either
    /// alone, and stops in the middle (#1652).
    #[test]
    fn centring_with_the_key_towards_the_middle_is_faster() {
        let back = |left: bool, neutral: bool| {
            let mut a = KeyboardAxes { lock_curvature: 0.13, speed_kmh: 30.0, steering: 0.9, ..Default::default() };
            a.left_key = left;
            a.neutral_key = neutral;
            for _ in 0..6 {
                a.update(1.0 / 60.0);
            }
            0.9 - a.steering
        };
        let (both, key, centre) = (back(true, true), back(true, false), back(false, true));
        assert!(both > key * 1.3 && both > centre * 1.3, "both {both}, key {key}, centring {centre}");
        let mut a = KeyboardAxes { lock_curvature: 0.13, steering: 0.05, ..Default::default() };
        a.left_key = true;
        a.neutral_key = true;
        for _ in 0..120 {
            a.update(1.0 / 60.0);
        }
        assert_eq!(a.steering, 0.0);
    }

    /// Omsi.exe treats the two steering keys symmetrically: either one works alone,
    /// both together hold the current wheel position, and releasing one immediately lets
    /// the other continue steering.
    #[test]
    fn opposite_steering_keys_hold_then_resume_remaining_direction() {
        let mut a = KeyboardAxes {
            steering: 0.25,
            ..Default::default()
        };

        a.left_key = true;
        a.update(0.1);
        let after_left = a.steering;
        assert!(after_left < 0.25, "left alone must steer left: {after_left}");

        a.right_key = true;
        a.update(0.2);
        assert!(
            (a.steering - after_left).abs() < 1e-6,
            "both keys must hold the wheel: {} -> {}",
            after_left,
            a.steering
        );

        a.left_key = false;
        a.update(0.1);
        let after_right_resume = a.steering;
        assert!(
            after_right_resume > after_left,
            "releasing left while right remains held must steer right: {after_left} -> {after_right_resume}"
        );

        a.left_key = true;
        a.update(0.2);
        assert!(
            (a.steering - after_right_resume).abs() < 1e-6,
            "both keys must hold symmetrically: {} -> {}",
            after_right_resume,
            a.steering
        );

        a.right_key = false;
        a.update(0.1);
        assert!(
            a.steering < after_right_resume,
            "releasing right while left remains held must steer left: {after_right_resume} -> {}",
            a.steering
        );
    }

    #[test]
    fn the_clutch_goes_down_at_once_and_comes_up_slowly() {
        let mut a = KeyboardAxes::default();
        a.clutch_key = true;
        a.update(0.01);
        assert_eq!(a.clutch, 1.0);
        a.clutch_key = false;
        for _ in 0..100 {
            a.update(0.01);
        }
        assert!((a.clutch - 0.3).abs() < 1e-3, "{}", a.clutch);
    }

    /// Let go, the wheel settles in the middle without swinging through it; standing still
    /// it still comes back, just slowly.
    #[test]
    fn steering_returns_like_a_spring() {
        let step = 1.0 / 60.0;
        // standing: full lock takes a few seconds, not the old 1.25 s
        let mut s = KeyboardAxes::default();
        s.set(EngineAction::SteeringRight, true);
        let mut t_lock = None;
        for i in 1..=300 {
            s.update(step);
            if s.steering >= 1.0 && t_lock.is_none() {
                t_lock = Some(i as f32 * step);
            }
        }
        let t_lock = t_lock.expect("never reached full lock");
        assert!(
            (2.5..=5.0).contains(&t_lock),
            "middle to full lock standing took {t_lock} s"
        );
        // at 30 km/h a second of the key is under half a lock, and slower still at 80
        let mut k = KeyboardAxes {
            speed_kmh: 30.0,
            ..Default::default()
        };
        k.set(EngineAction::SteeringLeft, true);
        for _ in 0..60 {
            k.update(step);
        }
        assert!(
            k.steering < -0.3 && k.steering > -0.6,
            "held left for a second at 30 km/h: {}",
            k.steering
        );
        let mut fast = KeyboardAxes {
            speed_kmh: 80.0,
            ..Default::default()
        };
        fast.set(EngineAction::SteeringLeft, true);
        for _ in 0..60 {
            fast.update(step);
        }
        assert!(
            fast.steering > k.steering,
            "turned faster at 80 km/h ({}) than at 30 ({})",
            fast.steering,
            k.steering
        );
        k.set(EngineAction::SteeringLeft, false);
        let from = k.steering;
        let mut t_back = None;
        for i in 1..=300 {
            let before = k.steering;
            k.update(step);
            assert!(
                k.steering <= 0.0,
                "swung through the middle: {}",
                k.steering
            );
            assert!(
                k.steering >= before,
                "moved away from the middle: {} -> {}",
                before,
                k.steering
            );
            if k.steering == 0.0 && t_back.is_none() {
                t_back = Some(i as f32 * step);
            }
        }
        let t_back = t_back.expect("never settled");
        assert!(
            (0.7..=1.0).contains(&t_back),
            "back in {t_back} s from {from} (turned for 1 s)"
        );
        // standing still it still comes back, but slowly: well off centre after a second,
        // not stuck
        let mut s = KeyboardAxes {
            steering: -0.8,
            speed_kmh: 0.0,
            ..Default::default()
        };
        for _ in 0..60 {
            s.update(step);
        }
        assert!(
            s.steering < -0.3,
            "returned too fast standing still: {}",
            s.steering
        );
        assert!(
            s.steering > -0.78,
            "hardly came back standing still: {}",
            s.steering
        );
    }
}
