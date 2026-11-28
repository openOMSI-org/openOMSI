//! Simulation clock and timetable sync that is independent of the graphical
//! application. Keep wall-clock and simulated time separate for accurate TPS.

use omsi_lan::admin::ServerAdmin;
use omsi_sim::world_clock::WorldClock;
use std::time::Instant;

/// Advance the simulation clock and optionally correct real-time synchronization.
pub fn sync_clock(
    clock: &mut f64,
    wall: &mut WorldClock,
    admin: &mut ServerAdmin,
    dt: f64,
    speed: f64,
    initial: f64,
    real_time_seconds: Option<f64>,
) {
    *clock += wall.step(Instant::now(), dt, speed);
    if let Some(real) = real_time_seconds {
        let have = (initial + *clock + admin.shift).rem_euclid(86_400.0);
        let offset = (real - have + 43_200.0).rem_euclid(86_400.0) - 43_200.0;
        if offset.abs() > 0.5 {
            admin.shift += offset;
        }
    }
}

/// Real-time correction is done by the headless host, not by each client.
pub fn correct_real_time(calendar: &mut NativeCalendar, clock: f64, admin: &mut ServerAdmin) {
    let current = calendar.at(clock + admin.shift).clone();
    if let Some(real) = omsi_career::real_time::clock_now(&current) {
        let drift = omsi_career::real_time::gap(&current, &real);
        if drift.abs() > 0.25 {
            admin.shift += drift;
        }
    }
}

pub fn follow_server_clock(day_time: f64, want: f64) -> (f64, Option<f64>) {
    let jump = want - day_time;
    (want, (jump.abs() >= 120.0).then_some(jump))
}

#[cfg(test)]
mod tests {
    use super::follow_server_clock;

    #[test]
    fn timetable_jump_threshold_is_symmetric_and_inclusive() {
        // A 119.999-second correction must not reset departures, but a
        // 120-second jump in either direction must rebase the timetable.
        assert_eq!(follow_server_clock(1_000.0, 1_119.999), (1_119.999, None));
        assert_eq!(follow_server_clock(1_000.0, 880.001), (880.001, None));
        assert_eq!(
            follow_server_clock(1_000.0, 1_120.0),
            (1_120.0, Some(120.0))
        );
        assert_eq!(follow_server_clock(1_000.0, 880.0), (880.0, Some(-120.0)));
    }

    #[test]
    fn the_traffic_follows_the_servers_clock_and_a_jump_restarts_the_timetable() {
        assert_eq!(follow_server_clock(54_000.0, 54_000.5), (54_000.5, None));
        assert_eq!(follow_server_clock(54_000.0, 54_119.0), (54_119.0, None));
        assert_eq!(
            follow_server_clock(54_000.0, 94_560.0),
            (94_560.0, Some(40_560.0))
        );
    }
}

const NATIVE_DAY_SECONDS: f64 = 86_400.0;

/// Convert the monotonic simulation timeline to the date and time of day
/// advertised by LAN. Timetables keep their continuous day_time separately.
#[derive(Debug, Clone)]
pub struct NativeCalendar {
    date: omsi_sim::SimClock,
    day_index: i64,
}

impl NativeCalendar {
    pub fn new(date: &str, time: &str) -> Self {
        Self {
            date: omsi_sim::SimClock::starting(Some(date), None, time),
            day_index: 0,
        }
    }

    pub fn snapshot(&self) -> omsi_sim::SimClock {
        self.date.clone()
    }

    pub fn at(&mut self, absolute_seconds: f64) -> &omsi_sim::SimClock {
        if !absolute_seconds.is_finite() {
            return &self.date;
        }
        let target = (absolute_seconds / NATIVE_DAY_SECONDS).floor() as i64;
        // Change days only at actual calendar boundaries, not every CPU tick.
        while self.day_index < target {
            self.date.next_day();
            self.day_index += 1;
        }
        while self.day_index > target {
            self.date.prev_day();
            self.day_index -= 1;
        }
        self.date.time = absolute_seconds.rem_euclid(NATIVE_DAY_SECONDS);
        &self.date
    }
}

#[cfg(test)]
mod native_calendar_tests {
    use super::NativeCalendar;

    #[test]
    fn midnight_changes_the_host_date() {
        let mut c = NativeCalendar::new("1989-12-31", "23:59");
        assert_eq!(c.at(86_399.5).date_code(), 19891231);
        assert_eq!(c.at(86_400.5).date_code(), 19900101);
        assert!((c.at(86_400.5).time - 0.5).abs() < 1e-9);
    }

    #[test]
    fn leap_day_and_backward_clock_shift_are_reversible() {
        let mut c = NativeCalendar::new("2024-02-28", "09:00");
        assert_eq!(c.at(172_800.0).date_code(), 20240301);
        assert_eq!(c.at(86_400.0).date_code(), 20240229);
        assert_eq!(c.at(-1.0).date_code(), 20240227);
        assert!((c.at(-1.0).time - 86_399.0).abs() < 1e-9);
        assert_eq!(c.at(0.0).date_code(), 20240228);
    }

    #[test]
    fn nonfinite_updates_keep_the_last_valid_date() {
        let mut c = NativeCalendar::new("2026-11-19", "20:00");
        assert_eq!(c.at(f64::NAN).date_code(), 20261119);
        assert_eq!(c.at(f64::INFINITY).date_code(), 20261119);
    }
}
