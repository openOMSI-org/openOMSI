//! Cruise control (Num0, #3976): it works the pedals the bus's scripts read, as a driver's
//! foot would.

pub(crate) const MIN_KMH: f32 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cruise {
    pub(crate) kmh: f32,
    /// km/h · s
    integral: f32,
}

impl Cruise {
    pub(crate) fn new(kmh: f32) -> Cruise {
        Cruise { kmh, integral: 0.0 }
    }

    /// A little above the speed the bus only rolls: the engine brake and the retarder do
    /// their part.
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

    pub(crate) fn overridden(&mut self) {
        self.integral = self.integral.min(0.0);
    }
}

/// The template is translated first: a key with the number in it is in no table.
pub(crate) fn on_text(kmh: f32, tr: impl Fn(&str) -> String) -> String {
    let (key, n) = if kmh < MIN_KMH {
        ("Cruise control works above {} km/h", MIN_KMH)
    } else {
        ("Cruise control: {} km/h", kmh.round())
    };
    tr(key).replace("{}", &(n as i32).to_string())
}

impl super::Player {
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
    /// A touch of an analog brake within its resting noise is still the driver's: it goes
    /// through and the controller waits rather than winding up against it.
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
