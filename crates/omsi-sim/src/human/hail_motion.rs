//! Local hailing wrist motion. Timing is animation tuning, not a national average.
// Adapted from Mayconrib808/OpenOmsi-BBS; copyright (c) 2026 Mayconrib808.
// Original MIT notice: docs/optional-passengers/MAYCON-LICENSE.txt.

#[derive(Default)]
pub struct HailWrist {
    elapsed: f32,
}

impl HailWrist {
    /// Advance with pose time, never the HH:MM:SS clock that admins can change.
    /// A short group of wrist flexions is followed by a held request. The arm
    /// stays extended; it must not flap along with the hand.
    pub fn advance(&mut self, active: bool, dt: f32, person: u32) -> f32 {
        if !active {
            self.elapsed = 0.0;
            return 0.0;
        }
        if dt.is_finite() && dt > 0.0 {
            self.elapsed += dt.min(0.25);
        }
        let tempo = 0.92 + (person % 9) as f32 * 0.02;
        let stroke = 0.46 / tempo;
        let pause = 0.45 + (person % 5) as f32 * 0.035;
        let time = (self.elapsed - 0.24).max(0.0);
        let phase = time.rem_euclid(stroke * 3.0 + pause);
        if phase >= stroke * 3.0 {
            return 0.0;
        }
        let turn = (phase / stroke).fract() * std::f32::consts::TAU;
        // About 17 degrees down and back, with zero velocity at each end.
        0.30 * (0.5 - 0.5 * turn.cos())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hailing_wrist_is_visible_brisk_and_has_holds_instead_of_a_continuous_wave() {
        let mut wrist = HailWrist::default();
        let mut peaks = 0;
        let mut above = false;
        let mut held_frames = 0;
        for frame in 0..360 {
            let angle = wrist.advance(true, 1.0 / 120.0, 4);
            let high = angle > 0.26;
            if high && !above {
                peaks += 1;
            }
            above = high;
            if frame > 30 && angle.abs() < 1e-6 {
                held_frames += 1;
            }
            assert!((0.0..=0.30001).contains(&angle));
        }
        assert!(
            peaks >= 5,
            "wrist only made {peaks} flexions in three seconds"
        );
        assert!(
            held_frames > 25,
            "the wrist never held still between groups"
        );
    }

    #[test]
    fn wrist_timing_survives_frame_rates_pauses_and_new_requests() {
        let mut values = Vec::new();
        for hz in [30, 60, 120] {
            let mut w = HailWrist::default();
            let mut angle = 0.0;
            for _ in 0..hz {
                angle = w.advance(true, 1.0 / hz as f32, 3);
            }
            assert_eq!(w.advance(true, 0.0, 3), angle);
            assert_eq!(w.advance(true, f32::NAN, 3), angle);
            values.push(angle);
            assert_eq!(w.advance(false, 1.0, 3), 0.0);
            assert_eq!(w.advance(true, 1.0 / 60.0, 3), 0.0);
        }
        assert!(values.iter().all(|v| (*v - values[0]).abs() < 1e-4));
    }
}
