//! Cruise control (Num0, #3976): holds the speed the bus had when it was switched on by
//! working the pedals the scripts read, as a driver's foot would. The brake (key or pedal)
//! switches it off; the throttle key or pedal pressed further wins while it is held, and the
//! speed is held again once it is let go.

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
    fn nonsense_in_gives_no_pedals() {
        let mut c = Cruise::new(30.0);
        assert_eq!(c.pedals(f32::NAN, 0.016), (0.0, 0.0));
        assert_eq!(c.pedals(20.0, 0.0), (0.0, 0.0));
    }
}
