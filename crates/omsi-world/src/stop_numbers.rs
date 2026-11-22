//! Reading of bus stop passenger counts (#3154).

/// Reading mode for stop passenger entry and exit counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopNumbers {
    /// Rounded before scaling, matching OMSI 2's integer truncation behavior.
    #[default]
    Omsi,
    /// Preserved as floats so passenger density scales the counts before rounding.
    Exact,
}

impl StopNumbers {
    pub(crate) fn round(self, v: f64) -> f32 {
        match self {
            StopNumbers::Omsi => v.round() as f32,
            StopNumbers::Exact => v as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_index::*;

    #[test]
    fn the_maps_own_passenger_numbers_can_be_kept_exactly() {
        let s = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let stop = s(&["Klinikum", "0.7", "0.6"]);
        assert_eq!(
            stop_enter(&stop),
            (1.0, 1.0),
            "OMSI's own reading rounds both up"
        );
        assert_eq!(stop_enter_with(&stop, StopNumbers::Exact), (0.7, 0.6));
        let mean = |(max, min): (f32, f32)| (max + min) / 2.0;
        assert_eq!((0.6 * mean(stop_enter(&stop))).round(), 1.0);
        assert_eq!(
            (0.6 * mean(stop_enter_with(&stop, StopNumbers::Exact))).round(),
            0.0
        );
        assert_eq!(
            (1.0 * mean(stop_enter_with(&stop, StopNumbers::Exact))).round(),
            1.0
        );
        assert_eq!(stop_exit_weight(&stop), 1.0);
        assert!((stop_exit_weight_with(&stop, StopNumbers::Exact) - 0.65).abs() < 1e-6);
        let plain = s(&["A", "6", "2"]);
        assert_eq!(
            stop_enter(&plain),
            stop_enter_with(&plain, StopNumbers::Exact)
        );
        assert_eq!(
            stop_enter_with(&s(&["A", "0,7"]), StopNumbers::Exact),
            (0.7, 0.0)
        );
        assert_eq!(
            stop_enter_with(&s(&["A", "", ""]), StopNumbers::Exact),
            (1.0, 0.0)
        );
        let border = s(&["Border", "0.49999999", "0.0"]);
        assert_eq!(
            stop_enter(&border),
            (0.0, 0.0),
            "0.49999999 rounds to zero in OMSI mode"
        );
    }
}
