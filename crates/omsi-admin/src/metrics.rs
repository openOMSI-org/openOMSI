//! Wall-clock dedicated-server ticks per second (TPS).
//!
//! Count completed simulation ticks rather than frames rendered, admin HTTP
//! requests, or nominal simulation time. A slow or overloaded host must report
//! a lower TPS; it must never claim the target rate just because its clock is
//! advanced in fixed-size steps.

use std::time::{Duration, Instant};

/// One wall-clock window; no GPU or platform timing APIs are involved.
const WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct TickMeter {
    since: Instant,
    ticks: u64,
    last: Option<f64>,
    target: f64,
}

impl TickMeter {
    pub fn new(target: f64) -> Self {
        Self {
            since: Instant::now(),
            ticks: 0,
            last: None,
            target: if target.is_finite() && target > 0.0 {
                target
            } else {
                60.0
            },
        }
    }

    /// Called after each full simulation/network step. No smoothing hides stalls.
    pub fn completed(&mut self, at: Instant) {
        self.ticks = self.ticks.saturating_add(1);
        let elapsed = at.saturating_duration_since(self.since);
        if elapsed >= WINDOW {
            self.last = Some(self.ticks as f64 / elapsed.as_secs_f64());
            self.ticks = 0;
            self.since = at;
        }
    }

    /// None until the first measurement window has elapsed.
    pub fn tps(&self) -> Option<f64> {
        self.last
    }

    pub fn target(&self) -> f64 {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_rate_follows_completed_ticks_not_nominal_dt() {
        let mut meter = TickMeter::new(60.0);
        let start = meter.since;
        assert_eq!(meter.tps(), None);
        for i in 1..=20 {
            meter.completed(start + Duration::from_millis(i * 50));
        }
        assert_eq!(meter.tps(), Some(20.0));
        assert_eq!(meter.target(), 60.0);
        for i in 1..=40 {
            meter.completed(start + Duration::from_millis(1000 + i * 25));
        }
        assert_eq!(meter.tps(), Some(40.0));
    }

    #[test]
    fn stalls_are_not_hidden_by_an_average_over_simulated_time() {
        let mut meter = TickMeter::new(60.0);
        let start = meter.since;
        for i in 1..=60 {
            meter.completed(start + Duration::from_millis(i * 20));
        }
        assert_eq!(meter.tps(), Some(50.0));
        // Ten additional ticks completed between 1.0 and 1.2 seconds,
        // followed by a two-second stall. Count ALL eleven completions
        // since the previous reporting window: 11 / 2.2 = 5 TPS.
        meter.completed(start + Duration::from_millis(3200));
        assert_eq!(meter.tps(), Some(5.0));
        // With no intermediate ticks, an isolated tick following a
        // two-second stall must report 0.5 TPS, not the nominal 60.
        meter.completed(start + Duration::from_millis(5200));
        assert_eq!(meter.tps(), Some(0.5));
    }

    #[test]
    fn invalid_target_is_safe() {
        assert_eq!(TickMeter::new(f64::NAN).target(), 60.0);
        assert_eq!(TickMeter::new(0.0).target(), 60.0);
    }
}
