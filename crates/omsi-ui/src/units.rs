//! The units the game's own texts give speeds, distances and temperatures in (the `units`
//! setting): metric, British (miles an hour, miles and yards, degrees Celsius) or American
//! (miles an hour, miles and feet, degrees Fahrenheit). The simulation keeps its metres and
//! kilometres an hour; only what is shown changes. The cockpit's own instruments are the
//! bus's and stay as its maker drew them.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    Metric,
    British,
    American,
}

const METRES_PER_MILE: f64 = 1609.344;
const METRES_PER_YARD: f64 = 0.9144;
const METRES_PER_FOOT: f64 = 0.3048;

static CURRENT: AtomicU8 = AtomicU8::new(0);

impl Units {
    /// The setting's value (`metric`, `uk`, `us`); anything else is metric.
    pub fn from_setting(v: &str) -> Units {
        match v.trim().to_ascii_lowercase().as_str() {
            "uk" | "british" | "imperial_uk" => Units::British,
            "us" | "american" | "imperial" => Units::American,
            _ => Units::Metric,
        }
    }

    /// The setting's value for these units.
    pub fn setting(self) -> &'static str {
        match self {
            Units::Metric => "metric",
            Units::British => "uk",
            Units::American => "us",
        }
    }

    /// The units the texts use now.
    pub fn now() -> Units {
        match CURRENT.load(Ordering::Relaxed) {
            1 => Units::British,
            2 => Units::American,
            _ => Units::Metric,
        }
    }

    /// Use these units from now on (the settings read or changed).
    pub fn set(self) {
        CURRENT.store(self as u8, Ordering::Relaxed);
    }

    /// A speed's number in these units and its unit.
    pub fn speed(self, kmh: f32) -> (f32, &'static str) {
        match self {
            Units::Metric => (kmh, "km/h"),
            _ => (kmh * 1000.0 / METRES_PER_MILE as f32, "mph"),
        }
    }

    /// A speed as text: `48 km/h`, `30 mph`.
    pub fn speed_text(self, kmh: f32) -> String {
        let (v, unit) = self.speed(kmh);
        format!("{v:.0} {}", crate::i18n::tr(unit))
    }

    /// A speed limit as a sign shows it: to the nearest five.
    pub fn speed_limit(self, kmh: f32) -> f32 {
        ((self.speed(kmh).0 / 5.0).round() * 5.0).max(5.0)
    }

    /// A distance on the road as a navigator says it: kilometres from one, else metres to
    /// ten; miles from a quarter (British) or a tenth (American), else yards to ten or feet
    /// to fifty.
    pub fn distance(self, m: f64) -> String {
        let m = m.max(0.0);
        match self {
            Units::Metric if m >= 1000.0 => format!("{:.1} {}", m / 1000.0, crate::i18n::tr("km")),
            Units::Metric => format!("{:.0} {}", ((m / 10.0).round() * 10.0).max(10.0), crate::i18n::tr("m")),
            Units::British if m >= METRES_PER_MILE / 4.0 => format!("{:.1} {}", m / METRES_PER_MILE, crate::i18n::tr("mi")),
            Units::British => format!("{:.0} {}", ((m / METRES_PER_YARD / 10.0).round() * 10.0).max(10.0), crate::i18n::tr("yd")),
            Units::American if m >= METRES_PER_MILE / 10.0 => format!("{:.1} {}", m / METRES_PER_MILE, crate::i18n::tr("mi")),
            Units::American => format!("{:.0} {}", ((m / METRES_PER_FOOT / 50.0).round() * 50.0).max(50.0), crate::i18n::tr("ft")),
        }
    }

    /// A long distance (driven, a mileometer): `12.3 km`, `7.6 mi`.
    pub fn long_distance(self, km: f64, decimals: usize) -> String {
        match self {
            Units::Metric => format!("{:.*} {}", decimals, km, crate::i18n::tr("km")),
            _ => format!("{:.*} {}", decimals, km * 1000.0 / METRES_PER_MILE, crate::i18n::tr("mi")),
        }
    }

    /// A temperature: `15 °C`, `59 °F`.
    pub fn temperature(self, celsius: f32) -> String {
        match self {
            Units::American => format!("{:.0} °F", celsius * 1.8 + 32.0),
            _ => format!("{celsius:.0} °C"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Units::*;
    use super::*;

    #[test]
    fn speeds_distances_and_temperatures_in_each_unit() {
        assert_eq!(Metric.speed_text(48.3), "48 km/h");
        assert_eq!(British.speed_text(48.3), "30 mph");
        assert_eq!(American.speed_limit(50.0), 30.0);
        assert_eq!(Metric.speed_limit(48.0), 50.0);
        assert_eq!(Metric.distance(1234.0), "1.2 km");
        assert_eq!(Metric.distance(234.0), "230 m");
        assert_eq!(British.distance(234.0), "260 yd");
        assert_eq!(British.distance(805.0), "0.5 mi");
        assert_eq!(American.distance(100.0), "350 ft");
        assert_eq!(American.distance(400.0), "0.2 mi");
        assert_eq!(British.long_distance(16.09344, 1), "10.0 mi");
        assert_eq!(British.temperature(15.0), "15 °C");
        assert_eq!(American.temperature(15.0), "59 °F");
    }

    #[test]
    fn the_setting_reads_back() {
        for u in [Metric, British, American] {
            assert_eq!(Units::from_setting(u.setting()), u);
        }
        assert_eq!(Units::from_setting("whatever"), Metric);
    }
}
