//! Determine motor polarity from two bounded, opposite pulses on the raw steering axis.

pub const PULSE_FORCE: f32 = 0.20;
pub const MAX_PULSE_FORCE: f32 = 0.50;
pub const PULSE_MS: u32 = 250;

enum Phase {
    Settle {
        origin: Option<f32>,
        stable_since: f32,
        first: Option<f32>,
    },
    Pulse {
        origin: f32,
        since: f32,
        first: Option<f32>,
        minimum: f32,
        maximum: f32,
    },
}

pub struct Calibration {
    strength: f32,
    phase: Phase,
    last_frame: f32,
    pub result: Option<Result<bool, &'static str>>,
}

impl Calibration {
    pub fn new(strength: f32) -> Self {
        Self {
            strength: strength.clamp(PULSE_FORCE, MAX_PULSE_FORCE),
            phase: Phase::Settle {
                origin: None,
                stable_since: 0.0,
                first: None,
            },
            last_frame: 0.0,
            result: None,
        }
    }

    pub fn fail(&mut self, message: &'static str) {
        self.result = Some(Err(message));
    }

    /// Returns the force of a new pulse. Time is measured from the user's Start click.
    pub fn update(&mut self, now: f32, position: Option<f32>) -> Option<f32> {
        if self.result.is_some() {
            return None;
        }
        if now - self.last_frame > 0.5 && self.last_frame > 0.0 {
            self.fail("The test was interrupted. Please try again.");
            return None;
        }
        self.last_frame = now;
        let Some(x) = position.filter(|x| x.is_finite()) else {
            if now > 2.0 || !matches!(self.phase, Phase::Settle { origin: None, .. }) {
                self.fail("The wheel is unavailable. Reconnect it and try again.");
            }
            return None;
        };
        if x.abs() > 0.8 {
            self.fail("Move the wheel away from its end stops and try again.");
            return None;
        }
        if now > 5.0 {
            self.fail("The wheel did not settle. Let go and try again.");
            return None;
        }
        match &mut self.phase {
            Phase::Settle {
                origin,
                stable_since,
                first,
            } => {
                if origin.is_none_or(|o| (x - o).abs() > 0.003) {
                    *origin = Some(x);
                    *stable_since = now;
                }
                if now - *stable_since >= 0.4 {
                    let sign = if first.is_some() { -1.0 } else { 1.0 };
                    log::info!(
                        "FFB calibration: pulse {sign:+.0}, raw origin {x:.5}, force {:.3}, duration {PULSE_MS} ms",
                        sign * self.strength
                    );
                    self.phase = Phase::Pulse {
                        origin: x,
                        since: now,
                        first: *first,
                        minimum: 0.0,
                        maximum: 0.0,
                    };
                    return Some(sign * self.strength);
                }
            }
            Phase::Pulse {
                origin,
                since,
                first,
                minimum,
                maximum,
            } => {
                let delta = x - *origin;
                *minimum = minimum.min(delta);
                *maximum = maximum.max(delta);
                if delta.abs() > 0.10 {
                    self.fail("The wheel moved too far. Test stopped.");
                } else if now - *since >= PULSE_MS as f32 / 1000.0 {
                    log::info!(
                        "FFB calibration: pulse response min {minimum:.5}, max {maximum:.5}, final {delta:.5}, elapsed {:.3} s",
                        now - *since
                    );
                    // The wheel may already be returning when a frame arrives after the
                    // hardware-timed pulse ends. Use its observed excursion, not just that last frame.
                    let positive = *maximum >= 0.006;
                    let negative = *minimum <= -0.006;
                    let movement = if positive && !negative {
                        *maximum
                    } else {
                        *minimum
                    };
                    if !positive && !negative {
                        self.fail("The wheel did not move enough. Retry or choose the direction manually.");
                    } else if positive && negative {
                        self.fail(
                            "The result is inconclusive. Retry or choose the direction manually.",
                        );
                    } else if let Some(first) = *first {
                        if first * movement >= 0.0 {
                            self.fail("The result is inconclusive. Retry or choose the direction manually.");
                        } else {
                            self.result = Some(Ok(first < 0.0));
                        }
                    } else {
                        self.phase = Phase::Settle {
                            origin: Some(x),
                            stable_since: now,
                            first: Some(movement),
                        };
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(first: f32, second: f32) -> Option<Result<bool, &'static str>> {
        let mut c = Calibration::new(PULSE_FORCE);
        assert_eq!(c.update(0.0, Some(0.0)), None);
        assert_eq!(c.update(0.41, Some(0.0)), Some(PULSE_FORCE));
        c.update(0.67, Some(first));
        if c.result.is_some() {
            return c.result;
        }
        assert_eq!(c.update(1.08, Some(first)), Some(-PULSE_FORCE));
        c.update(1.34, Some(first + second));
        c.result
    }

    #[test]
    fn both_polarities_are_detected() {
        assert_eq!(response(0.025, -0.025), Some(Ok(false)));
        assert_eq!(response(-0.025, 0.025), Some(Ok(true)));
    }

    #[test]
    fn selected_strength_is_used_and_bounded() {
        for (requested, expected) in [(0.0, PULSE_FORCE), (0.35, 0.35), (1.0, MAX_PULSE_FORCE)] {
            let mut c = Calibration::new(requested);
            c.update(0.0, Some(0.0));
            assert_eq!(c.update(0.41, Some(0.0)), Some(expected));
            c.update(0.67, Some(0.025));
            assert_eq!(c.update(1.08, Some(0.025)), Some(-expected));
        }
    }

    #[test]
    fn movement_during_the_pulse_is_kept_when_the_wheel_returns_before_the_last_sample() {
        for direction in [1.0, -1.0] {
            let mut c = Calibration::new(PULSE_FORCE);
            c.update(0.0, Some(0.0));
            assert_eq!(c.update(0.41, Some(0.0)), Some(PULSE_FORCE));
            c.update(0.55, Some(0.025 * direction));
            c.update(0.68, Some(0.0));
            assert_eq!(c.update(1.09, Some(0.0)), Some(-PULSE_FORCE));
            c.update(1.23, Some(-0.025 * direction));
            c.update(1.36, Some(0.0));
            assert_eq!(c.result, Some(Ok(direction < 0.0)));
        }
    }

    #[test]
    fn a_pulse_with_significant_motion_in_both_directions_is_rejected() {
        let mut c = Calibration::new(PULSE_FORCE);
        c.update(0.0, Some(0.0));
        c.update(0.41, Some(0.0));
        c.update(0.5, Some(0.02));
        c.update(0.6, Some(-0.02));
        c.update(0.68, Some(0.0));
        assert!(c.result.unwrap().is_err());
    }

    #[test]
    fn inconclusive_motion_is_rejected() {
        assert!(response(0.0, 0.0).unwrap().is_err());
        assert!(response(0.025, 0.025).unwrap().is_err());
        assert!(response(0.025, -0.001).unwrap().is_err());
    }

    #[test]
    fn excessive_motion_and_stalled_frames_abort() {
        assert!(response(0.11, -0.11).unwrap().is_err());
        let mut c = Calibration::new(PULSE_FORCE);
        c.update(0.1, Some(0.0));
        c.update(0.8, Some(0.0));
        assert!(c.result.unwrap().is_err());
    }

    #[test]
    fn missing_axis_noise_and_end_stops_do_not_start_a_pulse() {
        let mut missing = Calibration::new(PULSE_FORCE);
        for i in 0..25 {
            assert_eq!(missing.update(i as f32 / 10.0, None), None);
        }
        assert!(missing.result.unwrap().is_err());
        let mut noisy = Calibration::new(PULSE_FORCE);
        for i in 0..60 {
            assert_eq!(
                noisy.update(i as f32 / 10.0, Some(if i % 2 == 0 { 0.01 } else { -0.01 })),
                None
            );
        }
        assert!(noisy.result.unwrap().is_err());
        let mut end_stop = Calibration::new(PULSE_FORCE);
        assert_eq!(end_stop.update(0.0, Some(0.9)), None);
        assert!(end_stop.result.unwrap().is_err());
    }

    #[test]
    fn disconnect_during_a_pulse_aborts_without_another_pulse() {
        let mut c = Calibration::new(PULSE_FORCE);
        c.update(0.0, Some(0.0));
        assert_eq!(c.update(0.41, Some(0.0)), Some(PULSE_FORCE));
        assert_eq!(c.update(0.5, None), None);
        assert!(c.result.unwrap().is_err());
        assert_eq!(c.update(0.6, Some(0.0)), None);
    }
}
