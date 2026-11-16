//! A bus without a hazard switch only shows its lamps, dark half of the time: read at one
//! frame, hazards that are on looked off in every dark phase.

/// Longer than an indicator's dark phase (about half a second in the stock scripts).
pub(crate) const HOLD: f32 = 1.5;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct HazardWatch {
    since_both: Option<f32>,
}

impl HazardWatch {
    pub(crate) fn step(&mut self, dt: f32, both_lit: bool) {
        self.since_both = if both_lit {
            Some(0.0)
        } else {
            self.since_both
                .map(|t| t + dt.max(0.0))
                .filter(|t| *t <= HOLD)
        };
    }

    pub(crate) fn flashing(&self) -> bool {
        self.since_both.is_some()
    }
}

pub(crate) fn hazards_on(switch: Option<f32>, watch: &HazardWatch) -> bool {
    match switch {
        Some(s) => s > 0.5,
        None => watch.flashing(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hazards flashing 0.4 s lit, 0.4 s dark, at 50 frames a second.
    fn flash(watch: &mut HazardWatch, secs: f32, mut each: impl FnMut(bool, &HazardWatch)) {
        let frames = (secs / 0.02).round() as usize;
        for i in 0..frames {
            let lit = (i as f32 * 0.02) % 0.8 < 0.4;
            watch.step(0.02, lit);
            each(lit, &*watch);
        }
    }

    #[test]
    fn hazards_count_as_on_in_the_dark_phase_of_the_lamps() {
        let mut watch = HazardWatch::default();
        let mut dark_frames = 0;
        flash(&mut watch, 4.0, |lit, w| {
            dark_frames += usize::from(!lit);
            assert!(
                hazards_on(None, w),
                "a bus without the hazard switch, lamps lit {lit}"
            );
        });
        assert!(
            dark_frames > 50,
            "the dark phase was tested: {dark_frames} frames"
        );
    }

    #[test]
    fn hazards_switched_off_go_off_after_the_hold() {
        let mut watch = HazardWatch::default();
        flash(&mut watch, 2.0, |_, _| {});
        let mut dark = 0.0;
        while dark < HOLD + 0.1 {
            watch.step(0.02, false);
            dark += 0.02;
        }
        assert!(!hazards_on(None, &watch));
        assert!(!hazards_on(None, &HazardWatch::default()), "never lit");
    }

    #[test]
    fn the_scripts_hazard_switch_decides_where_there_is_one() {
        let mut watch = HazardWatch::default();
        flash(&mut watch, 1.0, |_, _| {});
        assert!(!hazards_on(Some(0.0), &watch), "switch off, lamps flashing");
        assert!(
            hazards_on(Some(1.0), &HazardWatch::default()),
            "switch on, lamps dark"
        );
    }
}
