//! The weight of the passengers on the bus's physics (#4014): `passenger_mass` turns it on
//! (off by default: the bus weighs what its file says, whoever rides), `passenger_weight_kg`
//! is a passenger's weight where the `.hum` gives none (or everybody's, with
//! `passenger_weight_fixed`), and `passenger_weight_spread` how much one person's varies
//! (percent either way).

use super::Settings;
use omsi_sim::people::PassengerWeight;

/// A passenger's weight where nothing else says (kg).
pub(crate) const DEFAULT_WEIGHT_KG: f32 = 75.0;

impl Settings {
    /// How the people aboard weigh on the bus (see `omsi_sim::people::PassengerWeight`).
    pub fn passenger_weight(&self) -> PassengerWeight {
        if !self.passenger_mass {
            return PassengerWeight::Off;
        }
        let kg = self.passenger_weight_kg;
        let spread = self.passenger_weight_spread / 100.0;
        if self.passenger_weight_fixed {
            PassengerWeight::Fixed { kg, spread }
        } else {
            PassengerWeight::Files { kg, spread }
        }
    }

    /// Read one of the keys; false for another key.
    pub(crate) fn read_passenger_mass(&mut self, k: &str, v: &str) -> bool {
        let on = |v: &str| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes");
        let num = |v: &str| v.trim().parse::<f32>().ok().filter(|x| x.is_finite());
        match k {
            "passenger_mass" => self.passenger_mass = on(v),
            "passenger_weight_fixed" => self.passenger_weight_fixed = on(v),
            "passenger_weight_kg" => {
                self.passenger_weight_kg = num(v).unwrap_or(DEFAULT_WEIGHT_KG).clamp(20.0, 200.0)
            }
            "passenger_weight_spread" => {
                self.passenger_weight_spread = num(v).unwrap_or(0.0).clamp(0.0, 50.0)
            }
            _ => return false,
        }
        true
    }

    /// The keys as `settings.cfg` keeps them.
    pub(crate) fn write_passenger_mass(&self, text: &mut String) {
        text.push_str(&format!(
            "passenger_mass={}\npassenger_weight_kg={}\npassenger_weight_fixed={}\npassenger_weight_spread={}\n",
            self.passenger_mass as u8,
            self.passenger_weight_kg,
            self.passenger_weight_fixed as u8,
            self.passenger_weight_spread
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_and_kept_when_written() {
        let s = Settings::default();
        assert!(!s.passenger_mass);
        assert_eq!(s.passenger_weight(), PassengerWeight::Off);
        assert!(!Settings::from_text("version=1\n").passenger_mass);
        let s = Settings::from_text(
            "passenger_mass=1\npassenger_weight_kg=80\npassenger_weight_spread=10\n",
        );
        assert_eq!(
            s.passenger_weight(),
            PassengerWeight::Files {
                kg: 80.0,
                spread: 0.1
            }
        );
        let mut text = String::new();
        s.write_passenger_mass(&mut text);
        let again = Settings::from_text(&text);
        assert_eq!(again.passenger_weight(), s.passenger_weight());
        let fixed = Settings::from_text(
            "passenger_mass=1\npassenger_weight_fixed=1\npassenger_weight_kg=500\n",
        );
        assert_eq!(
            fixed.passenger_weight(),
            PassengerWeight::Fixed {
                kg: 200.0,
                spread: 0.0
            }
        );
    }
}
