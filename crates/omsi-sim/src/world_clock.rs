//! A dedicated server's world clock against the wall: the physics take fixed steps of
//! 1/60 s, and when the machine cannot keep up the steps fall behind the wall (the pace,
//! `RealTime`, gives up a lag over a quarter of a second rather than run steps back to back).
//! The world clock - the day, the timetable, what the players are told - moved by the steps
//! fell behind the day with them, silently and for good: a server at half its steps had half
//! a day's timetable in a day. It follows the wall instead, at most twice as fast as the wall
//! runs while it takes back what it lost: a server that keeps up runs exactly as before, a
//! slow one keeps the day (its physics still take their fixed steps, slower), and the log
//! says how far behind it was.

use std::time::{Duration, Instant};

/// How much faster than its speed the clock may run while it takes back what it lost (1:
/// twice as fast).
const CATCH_UP: f64 = 1.0;
/// Behind by more than this, it is told as a stall.
const SKIP: f64 = 60.0;
/// Behind by less than this, it is not a lag (a step's jitter).
const SLACK: f64 = 0.05;
/// How often a lag is told in the log.
const TELL_EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
pub struct WorldClock {
    /// The wall at the last step.
    last: Option<Instant>,
    /// The world seconds the wall has given since the start (each wall second at the speed
    /// of that moment) less those the clock has been moved on: how far it is behind.
    behind: f64,
    told: Option<Instant>,
    /// The worst lag since the last telling (for the log).
    worst: f64,
}

impl WorldClock {
    /// The world seconds to move the clock on for a step of `dt` at `speed`, the wall now at
    /// `now`.
    pub fn step(&mut self, now: Instant, dt: f64, speed: f64) -> f64 {
        let speed = speed.max(0.0);
        let wall = self.last.map(|l| now.saturating_duration_since(l).as_secs_f64()).unwrap_or(dt);
        self.last = Some(now);
        self.behind += (wall - dt) * speed;
        let mut take = dt * speed;
        if self.behind > SKIP {
            log::warn!("server clock: {:.0} s behind the wall after a stall (the machine asleep or stopped); the world clock goes on at the wall's time", self.behind);
        }
        if self.behind > SLACK {
            // (at most twice the wall's pace: no faster than the players can follow)
            let extra = self.behind.min(((1.0 + CATCH_UP) * wall * speed - take).max(0.0));
            take += extra;
            self.behind -= extra;
        } else if self.behind < -SLACK {
            // (ahead: steps came faster than the wall; slower until it is not)
            let less = (-self.behind - SLACK).min(take);
            take -= less;
            self.behind += less;
        }
        self.worst = self.worst.max(self.behind);
        if self.worst > 1.0 && self.told.is_none_or(|t| now.saturating_duration_since(t) >= TELL_EVERY) {
            log::warn!("server clock: up to {:.1} s behind the wall lately (the machine cannot keep up with its steps: its people and traffic move slower than the clock)", self.worst);
            self.told = Some(now);
            self.worst = 0.0;
        }
        take
    }

    /// How far the world clock is behind the wall now (s).
    #[cfg(test)]
    pub(crate) fn behind(&self) -> f64 {
        self.behind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    /// A server whose steps come every `wall_per_step` seconds of the wall, for `secs`: the
    /// world seconds its clock moved.
    fn run(wall_per_step: f64, secs: f64, speed: f64) -> (f64, WorldClock) {
        let mut c = WorldClock::default();
        let t0 = Instant::now();
        let (mut world, mut wall) = (0.0, 0.0);
        while wall < secs {
            wall += wall_per_step;
            world += c.step(t0 + Duration::from_secs_f64(wall), DT, speed);
        }
        (world, c)
    }

    /// At 60, 30, 15, 10 and 5 steps a second of the wall (a server that keeps up, and ones
    /// two to twelve times too slow), a minute of the wall is a minute of the world: never
    /// more than a step's length behind, never ahead.
    #[test]
    fn a_minute_of_the_wall_is_a_minute_of_the_world_at_any_pace() {
        for fps in [60.0, 30.0, 15.0, 10.0, 5.0] {
            let (world, c) = run(1.0 / fps, 60.0, 1.0);
            let lag = 60.0 - world;
            assert!((-0.06..=0.25).contains(&lag), "{fps} steps a second: {world:.3} world seconds in 60");
            assert!(c.behind() < 0.25);
        }
    }

    /// Steps at the wall's own pace move the clock by their own length, as before.
    #[test]
    fn a_server_that_keeps_up_runs_as_before() {
        let mut c = WorldClock::default();
        let t0 = Instant::now();
        for i in 1..=600 {
            let take = c.step(t0 + Duration::from_secs_f64(i as f64 * DT), DT, 1.0);
            assert!((take - DT).abs() < 1e-9, "step {i}: {take}");
        }
    }

    /// The clock never runs more than twice as fast as the wall: a lag of slow steps is taken
    /// back over the next ones, not in a jump.
    #[test]
    fn a_lag_is_taken_back_no_faster_than_twice_the_wall() {
        let mut c = WorldClock::default();
        let t0 = Instant::now();
        let mut wall = 0.0;
        c.step(t0, DT, 1.0);
        // steps of four times their length for a second (a load), then the wall's pace
        let mut world = 0.0;
        for i in 0..200 {
            let step = if i < 15 { 4.0 * DT } else { DT };
            wall += step;
            let take = c.step(t0 + Duration::from_secs_f64(wall), DT, 1.0);
            assert!(take <= 2.0 * step + 1e-9, "step {i}: {take} in {step}");
            world += take;
        }
        assert!((wall - world).abs() < SLACK + 1e-9, "caught up: wall {wall:.3}, world {world:.3}");
    }

    /// A stall moves the clock by the wall's time once the steps go on; a clock run at a
    /// speed of the admins' keeps that speed against the wall; a stopped one stays stopped.
    #[test]
    fn a_stall_is_the_walls_time_and_the_speed_is_kept() {
        let mut c = WorldClock::default();
        let t0 = Instant::now();
        c.step(t0, DT, 1.0);
        let take = c.step(t0 + Duration::from_secs(120), DT, 1.0);
        assert!((take - 120.0).abs() < 1e-6, "{take}");
        assert!(c.behind().abs() < 1e-9);
        let (world, _) = run(1.0 / 10.0, 60.0, 2.0);
        assert!((119.5..=120.1).contains(&world), "{world}");
        let (world, _) = run(1.0 / 10.0, 60.0, 0.0);
        assert_eq!(world, 0.0);
    }
}
