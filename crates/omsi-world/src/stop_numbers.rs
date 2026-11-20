//! How a bus stop's passenger numbers are read (#3154).

/// How a bus stop's `pass_enter_min` / `pass_enter_max` strings are read (#3154).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopNumbers {
    /// Rounded as Omsi.exe reads them: `0.7` is `1`, and a stop the map gives `0.7` keeps one
    /// person waiting until the density drops below `0.5`. What OMSI 2 does, and what a game
    /// does unless the player asks otherwise.
    #[default]
    Omsi,
    /// Kept as the map writes them, so the passenger density multiplies first and the rounding
    /// comes last: at a density of `0.6` a stop given `0.7` has nobody waiting, and at `1` one
    /// person - a range that can change with the hour instead of a step at `0.5`.
    Exact,
}

impl StopNumbers {
    pub(crate) fn round(self, v: f32) -> f32 {
        match self {
            StopNumbers::Omsi => v.round(),
            StopNumbers::Exact => v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_index::*;

    /// A stop's passenger numbers as the map writes them, kept for the density to multiply
    /// (#3154): 0.7 is 0.7, not 1, and the exit weight follows the same reading.
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
    }
}
