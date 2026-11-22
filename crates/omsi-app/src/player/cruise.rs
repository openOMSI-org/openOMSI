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
        if err >= 0.0 && u >= 0.0 {
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

/// The text for switching it on at `kmh`, or for refusing below [`MIN_KMH`]: the template is
/// translated by `tr` first, as a key with the number in it is in no table.
pub(crate) fn on_text(kmh: f32, tr: impl Fn(&str) -> String) -> String {
    let (key, n) = if kmh < MIN_KMH {
        ("Cruise control works above {} km/h", MIN_KMH)
    } else {
        ("Cruise control: {} km/h", kmh.round())
    };
    tr(key).replace("{}", &(n as i32).to_string())
}

impl super::Player {
    /// Cruise control on at the speed driven now, or off (Num0). The text for the screen.
    pub(crate) fn toggle_cruise(&mut self) -> String {
        if self.cruise.take().is_some() {
            return omsi_ui::tr("Cruise control off").into_owned();
        }
        let kmh = self.vehicle.physics.velocity_kmh();
        if kmh >= MIN_KMH {
            self.cruise = Some(Cruise::new(kmh.round()));
        }
        on_text(kmh, |key| omsi_ui::tr(key).into_owned())
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
#[path = "cruise_tests.rs"]
mod tests;
