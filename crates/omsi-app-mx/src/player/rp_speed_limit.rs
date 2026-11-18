/// Smooth RP road-speed governor. Keeps throttle untouched below the approach band,
/// cuts propulsion at the configured speed, and adds a small downhill brake only
/// above the limit. Never changes reverse or adds an abrupt velocity clamp.
pub(super) fn controls(kmh: f64, limit: u32, throttle: f32, brake: f32) -> (f32, f32) {
    if limit == 0 || kmh <= 0.0 || !kmh.is_finite() {
        return (throttle, brake);
    }
    let limit = limit as f64;
    let power = ((limit - kmh) / 5.0).clamp(0.0, 1.0) as f32;
    let downhill = ((kmh - limit - 0.5) / 15.0).clamp(0.0, 0.30) as f32;
    (throttle * power, brake.max(downhill))
}

#[cfg(test)]
mod rp_governor_tests {
    use super::controls as govern;

    #[test]
    fn disabled_and_reverse_are_bit_for_bit_unchanged() {
        assert_eq!(govern(125.0, 0, 0.7, 0.2), (0.7, 0.2));
        assert_eq!(govern(-85.0, 60, 0.7, 0.2), (0.7, 0.2));
    }

    #[test]
    fn normal_road_speeds_are_untouched_and_overspeed_is_gradual() {
        assert_eq!(govern(50.0, 60, 1.0, 0.0), (1.0, 0.0));
        let approaching = govern(57.5, 60, 1.0, 0.0);
        assert!((approaching.0 - 0.5).abs() < 0.001 && approaching.1 == 0.0);
        assert_eq!(govern(60.0, 60, 1.0, 0.0), (0.0, 0.0));
        let downhill = govern(65.0, 60, 1.0, 0.0);
        assert_eq!(downhill.0, 0.0);
        assert!(downhill.1 > 0.0 && downhill.1 <= 0.30);
    }
}

