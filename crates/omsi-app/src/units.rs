//! The unit speeds show in (setting `speed_unit`): kilometres an hour, or miles an hour for
//! players in the United Kingdom - the navigator's speed and its speed limit sign, the
//! information bar, the phone's navigator and a speed camera's fine. The game itself keeps
//! km/h; only what the player reads changes.

use std::sync::atomic::{AtomicBool, Ordering};

static MPH: AtomicBool = AtomicBool::new(false);

/// Kilometres in a mile.
pub const KMH_PER_MPH: f32 = 1.609_344;

/// The setting taken (`speed_unit`: "kmh" or "mph").
pub fn set(setting: &str) {
    MPH.store(omsi_launcher_lib::speed_unit(setting) == "mph", Ordering::Relaxed);
}

/// Whether speeds show in miles an hour.
pub fn mph() -> bool {
    MPH.load(Ordering::Relaxed)
}

/// `kmh` in miles an hour when `mph`, else as it is.
pub fn speed_in(kmh: f32, mph: bool) -> f32 {
    if mph {
        kmh / KMH_PER_MPH
    } else {
        kmh
    }
}

/// A speed limit of `kmh` as its sign shows it: to the nearest 5 of the unit (a map's 50 km/h
/// is a 30 mph sign, its 30 km/h a 20, its 100 km/h a 60).
pub fn sign_in(kmh: f32, mph: bool) -> f32 {
    (speed_in(kmh, mph) / 5.0).round() * 5.0
}

/// `kmh` in the unit shown.
pub fn speed(kmh: f32) -> f32 {
    speed_in(kmh, mph())
}

/// A speed limit as its sign shows it, in the unit shown.
pub fn sign(kmh: f32) -> f32 {
    sign_in(kmh, mph())
}

/// The unit's short name: "mph", else `kmh` (the language's own way of writing km/h).
pub fn label(kmh: &str) -> &str {
    if mph() {
        "mph"
    } else {
        kmh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_and_signs_in_miles_an_hour() {
        assert!((speed_in(80.4672, true) - 50.0).abs() < 1e-3);
        assert_eq!(speed_in(50.0, false), 50.0);
        // the map's limits as British signs
        let signs: Vec<f32> = [30.0, 50.0, 80.0, 100.0, 120.0].iter().map(|v| sign_in(*v, true)).collect();
        assert_eq!(signs, [20.0, 30.0, 50.0, 60.0, 75.0]);
        assert_eq!(sign_in(48.0, false), 50.0);
    }
}
