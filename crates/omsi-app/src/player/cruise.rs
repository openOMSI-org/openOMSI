//! Cruise control (Num0, #3976): holds the speed the bus had when it was switched on by
//! working the pedals the scripts read, as a driver's foot would. The brake (key or pedal)
//! switches it off, while a light touch of an analog one goes through; the throttle key or
//! pedal pressed further wins while it is held, and the speed is held again once it is let go.

/// The lowest speed it is switched on at (km/h).
pub(crate) const MIN_KMH: f32 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cruise {
    /// The speed held (km/h).
    pub(crate) kmh: f32,
    /// The controller's memory of how far off the speed has been (km/h · s).
    integral: f32,
}

impl Cruise {
    pub(crate) fn new(kmh: f32) -> Cruise {
        Cruise { kmh, integral: 0.0 }
    }

    /// The pedals (throttle, brake) that hold the speed: a PI controller. Below the speed
    /// it gives throttle; a little above it the bus only rolls (the engine brake and the
    /// retarder do their part); further above, downhill, it brakes gently.
    pub(crate) fn pedals(&mut self, kmh: f32, dt: f32) -> (f32, f32) {
        if !kmh.is_finite() || !dt.is_finite() || dt <= 0.0 {
            return (0.0, 0.0);
        }
        let err = self.kmh - kmh;
        self.integral = (self.integral + err * dt).clamp(-10.0, 30.0);
        let u = 0.08 * err + 0.03 * self.integral;
        if u >= 0.0 {
            (u.min(1.0), 0.0)
        } else if err < -1.5 {
            (0.0, (-(err + 1.5) * 0.05).clamp(0.0, 0.4))
        } else {
            (0.0, 0.0)
        }
    }

    /// While the driver's own throttle wins, the controller does not wind up.
    pub(crate) fn overridden(&mut self) {
        self.integral = self.integral.min(0.0);
    }
}

impl super::Player {
    /// Cruise control on at the speed driven now, or off (Num0). The text for the screen.
    pub(crate) fn toggle_cruise(&mut self) -> String {
        if self.cruise.take().is_some() {
            return "Cruise control off".into();
        }
        let kmh = self.vehicle.physics.velocity_kmh();
        if kmh < MIN_KMH {
            return format!("Cruise control works above {} km/h", MIN_KMH as i32);
        }
        let kmh = kmh.round();
        self.cruise = Some(Cruise::new(kmh));
        format!("Cruise control: {} km/h", kmh as i32)
    }

    /// The pedals with the cruise control's foot on them: the brake switches it off, a
    /// throttle pressed further wins while held.
    pub(super) fn cruise_pedals(&mut self, dt: f32, throttle: f32, brake: f32) -> (f32, f32) {
        let Some(c) = self.cruise.as_mut() else {
            return (throttle, brake);
        };
        let kmh = self.vehicle.physics.velocity_kmh();
        match c.with_driver(kmh, dt, throttle, brake, self.axes.brake_key) {
            Some(pedals) => pedals,
            None => {
                self.cruise = None;
                self.cruise_note = Some("Cruise control off".into());
                (throttle, brake)
            }
        }
    }
}

impl Cruise {
    /// The pedals with the driver's on top of the controller's, or None when the driver
    /// switched it off: a brake pressed past a pedal's resting noise (or the key), or the bus
    /// standing (reversing, or brought to a stand by something else). A lighter touch of an
    /// analog brake is the driver's still: it goes through, with no throttle under it, and
    /// the controller waits rather than winding up against it.
    fn with_driver(
        &mut self,
        kmh: f32,
        dt: f32,
        throttle: f32,
        brake: f32,
        brake_key: bool,
    ) -> Option<(f32, f32)> {
        if brake > 0.05 || brake_key || kmh < 1.0 {
            return None;
        }
        if brake > 0.0 {
            return Some((0.0, brake));
        }
        let (t, b) = self.pedals(kmh, dt);
        if throttle > t {
            self.overridden();
            return Some((throttle, 0.0));
        }
        Some((t, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bus of a crude kind: throttle accelerates, brake and drag slow it, a slope pulls.
    fn drive(target: f32, start: f32, slope: f32, secs: f32) -> f32 {
        let mut c = Cruise::new(target);
        let mut v = start;
        let dt = 1.0 / 60.0;
        for _ in 0..(secs / dt) as usize {
            let (t, b) = c.pedals(v, dt);
            assert!((0.0..=1.0).contains(&t) && (0.0..=0.4).contains(&b) && (t == 0.0 || b == 0.0));
            let a = t * 6.0 - b * 25.0 - v * 0.02 - slope;
            v = (v + a * dt).max(0.0);
        }
        v
    }

    #[test]
    fn it_holds_the_speed_on_the_flat_uphill_and_downhill() {
        for slope in [0.0, 1.5, -1.0] {
            let v = drive(50.0, 50.0, slope, 120.0);
            assert!((v - 50.0).abs() < 2.0, "slope {slope}: {v}");
        }
        // it catches up from below
        assert!((drive(50.0, 40.0, 0.0, 120.0) - 50.0).abs() < 2.0);
    }

    #[test]
    fn a_light_analog_brake_goes_through_and_a_firm_one_switches_it_off() {
        let mut c = Cruise::new(50.0);
        // well below the speed, so the controller alone would give throttle
        assert!(c.with_driver(40.0, 0.016, 0.0, 0.0, false).unwrap().0 > 0.0);
        assert_eq!(
            c.with_driver(40.0, 0.016, 0.0, 0.03, false),
            Some((0.0, 0.03))
        );
        assert_eq!(c.with_driver(40.0, 0.016, 0.0, 0.2, false), None);
        assert_eq!(c.with_driver(40.0, 0.016, 0.0, 0.0, true), None);
    }

    #[test]
    fn its_pedals_shift_an_automated_manual_up() {
        use omsi_input::auto_shift::auto_shift_gear;
        // the keys let go a little below the speed held: the driver's pedals alone keep second
        let mut c = Cruise::new(50.0);
        let (t, b) = c.with_driver(47.0, 0.016, 0.0, 0.0, false).unwrap();
        assert_eq!(auto_shift_gear(2, 5, 2.8, 0.0, 0.0, 0.0, 47.0), 2);
        assert_eq!(auto_shift_gear(2, 5, 2.8, 0.0, t, b, 47.0), 3);
    }

    #[test]
    fn nonsense_in_gives_no_pedals() {
        let mut c = Cruise::new(30.0);
        assert_eq!(c.pedals(f32::NAN, 0.016), (0.0, 0.0));
        assert_eq!(c.pedals(20.0, 0.0), (0.0, 0.0));
    }
}
