//! Opt-in, bounded timings profiler for a dedicated host.
//!
//! Records the authoritative simulation's stage execution times and tick latency
//! only while enabled. No sampling thread, OS process inspection, client
//! identity, path, IP address or script-content capture is involved.

use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_STAGE_NAMES: usize = 24;
const MAX_TICK_SAMPLES: usize = 1024;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct StageTiming {
    pub name: String,
    pub calls: u64,
    pub avg_ms: f64,
    pub max_ms: f64,
    pub total_ms: f64,
    /// Fraction of all measured stage execution time, not process CPU usage.
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct ProfileReport {
    pub enabled: bool,
    pub seconds: f64,
    pub ticks: u64,
    pub avg_tick_ms: f64,
    pub p95_tick_ms: f64,
    pub p99_tick_ms: f64,
    pub max_tick_ms: f64,
    /// Most expensive stages first; bounded to the fixed server stage set.
    pub stages: Vec<StageTiming>,
}

#[derive(Debug)]
struct Stage {
    name: &'static str,
    calls: u64,
    total: Duration,
    max: Duration,
}

/// Server-loop owned, no locks or collection while off.
#[derive(Debug)]
pub struct Profiler {
    enabled: bool,
    started: Instant,
    stopped: Duration,
    ticks: u64,
    tick_samples: VecDeque<f64>,
    stages: Vec<Stage>,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            enabled: false,
            started: Instant::now(),
            stopped: Duration::ZERO,
            ticks: 0,
            tick_samples: VecDeque::new(),
            stages: Vec::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Start a fresh recording; explicit reset prevents mixing sessions.
    pub fn start(&mut self) {
        self.stages.clear();
        self.tick_samples.clear();
        self.ticks = 0;
        self.stopped = Duration::ZERO;
        self.started = Instant::now();
        self.enabled = true;
    }

    /// Stop timing while retaining the last report for the console and admin UI.
    pub fn stop(&mut self) {
        if self.enabled {
            self.stopped = self.started.elapsed();
            self.enabled = false;
        }
    }

    pub fn reset(&mut self) {
        let enabled = self.enabled;
        self.start();
        if !enabled {
            self.stop();
        }
    }

    pub fn stage(&mut self, name: &'static str, elapsed: Duration) {
        if !self.enabled {
            return;
        }
        if let Some(s) = self.stages.iter_mut().find(|s| s.name == name) {
            s.calls = s.calls.saturating_add(1);
            s.total = s.total.saturating_add(elapsed);
            s.max = s.max.max(elapsed);
        } else if self.stages.len() < MAX_STAGE_NAMES {
            self.stages.push(Stage {
                name,
                calls: 1,
                total: elapsed,
                max: elapsed,
            });
        }
    }

    pub fn tick(&mut self, elapsed: Duration) {
        if !self.enabled {
            return;
        }
        self.ticks = self.ticks.saturating_add(1);
        if self.tick_samples.len() == MAX_TICK_SAMPLES {
            self.tick_samples.pop_front();
        }
        self.tick_samples.push_back(elapsed.as_secs_f64() * 1000.0);
    }

    pub fn report(&self) -> ProfileReport {
        let mut times: Vec<f64> = self.tick_samples.iter().copied().collect();
        times.sort_by(f64::total_cmp);
        let percentile = |percent: f64| {
            if times.is_empty() {
                0.0
            } else {
                times[((times.len() - 1) as f64 * percent).ceil() as usize]
            }
        };
        let total = self
            .stages
            .iter()
            .map(|s| s.total.as_secs_f64())
            .sum::<f64>();
        let mut stages: Vec<_> = self
            .stages
            .iter()
            .map(|s| {
                let seconds = s.total.as_secs_f64();
                StageTiming {
                    name: s.name.to_string(),
                    calls: s.calls,
                    avg_ms: if s.calls == 0 {
                        0.0
                    } else {
                        seconds * 1000.0 / s.calls as f64
                    },
                    max_ms: s.max.as_secs_f64() * 1000.0,
                    total_ms: seconds * 1000.0,
                    share_pct: if total == 0.0 {
                        0.0
                    } else {
                        100.0 * seconds / total
                    },
                }
            })
            .collect();
        stages.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
        ProfileReport {
            enabled: self.enabled,
            seconds: (if self.enabled {
                self.started.elapsed()
            } else {
                self.stopped
            })
            .as_secs_f64(),
            ticks: self.ticks,
            avg_tick_ms: if times.is_empty() {
                0.0
            } else {
                times.iter().sum::<f64>() / times.len() as f64
            },
            p95_tick_ms: percentile(0.95),
            p99_tick_ms: percentile(0.99),
            max_tick_ms: times.last().copied().unwrap_or_default(),
            stages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiler_stays_off_until_started_and_keeps_stopped_results() {
        let mut profiler = Profiler::new();
        profiler.stage("traffic", Duration::from_millis(4));
        profiler.tick(Duration::from_millis(20));
        assert_eq!(profiler.report().ticks, 0);
        profiler.start();
        profiler.stage("traffic", Duration::from_millis(4));
        profiler.stage("traffic", Duration::from_millis(2));
        profiler.stage("lan", Duration::from_millis(1));
        profiler.tick(Duration::from_millis(20));
        profiler.tick(Duration::from_millis(18));
        let running = profiler.report();
        assert_eq!(running.ticks, 2);
        assert_eq!(running.stages[0].name, "traffic");
        assert_eq!(running.stages[0].calls, 2);
        assert!((running.stages[0].avg_ms - 3.0).abs() < 0.01);
        assert!(running.p95_tick_ms >= 20.0);
        profiler.stop();
        assert!(!profiler.enabled());
        profiler.tick(Duration::from_secs(1));
        assert_eq!(profiler.report().ticks, 2);
        profiler.reset();
        assert!(!profiler.enabled());
        assert_eq!(profiler.report().ticks, 0);
    }

    #[test]
    fn caps_latency_samples_and_stage_names() {
        let mut profiler = Profiler::new();
        profiler.start();
        for _ in 0..(MAX_TICK_SAMPLES + 5) {
            profiler.tick(Duration::from_millis(12));
        }
        assert_eq!(profiler.tick_samples.len(), MAX_TICK_SAMPLES);
    }
}
