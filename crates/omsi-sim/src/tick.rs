//! The simulation's fixed step: the traffic, the player's vehicle with its scripts, the
//! people, the duty and the clock move on in ticks of one length (1/120 s, [`RATE_HZ`]),
//! however long the frames are. The frame's time is collected and spent in whole ticks; what
//! is left over says how far the picture stands between the last two ticks (`Ticks::alpha`),
//! and the player's bus and the camera are drawn there (`BodyPose::between`).
//!
//! Two runs of one session at different frame rates step the same ticks with the same inputs,
//! which is what makes them comparable at all (and the bus's physics the same at 30 and at
//! 144 frames a second).

use crate::ai_traffic::fixed_step::advance_fixed_clock;
use glam::DVec3;

/// The simulation's ticks a second, in the window and the offscreen run alike.
pub const RATE_HZ: f64 = 120.0;
/// The window's frame is at most a tenth of a second (`timing.rs`): at most that much time
/// is made up in one frame; a longer stall is dropped, as the frame's own time was.
const MAX_FRAME_S: f64 = 0.1;

/// What a frame runs: `n` ticks, and where the picture stands between the last two of them
/// (0: at the second last, towards 1: at the last).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ticks {
    pub n: u32,
    pub alpha: f32,
}

/// The accumulator of the fixed step.
#[derive(Debug, Clone)]
pub struct FixedStep {
    rate: f64,
    dt: f32,
    accum: f64,
    ticks: u64,
    max_per_frame: u32,
}

impl FixedStep {
    /// The step at `rate` ticks a second (the game's is [`RATE_HZ`]; the tests take others).
    pub fn new(rate: f64) -> Self {
        FixedStep {
            rate,
            dt: (1.0 / rate) as f32,
            accum: 0.0,
            ticks: 0,
            max_per_frame: (MAX_FRAME_S * rate).ceil() as u32 + 1,
        }
    }

    /// Ticks a second.
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// One tick (s): the time every simulation stage is given.
    pub fn dt(&self) -> f32 {
        self.dt
    }

    /// Ticks run since the start.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// The simulation's time since the start (s): the ticks run, each `dt` long.
    pub fn sim_time(&self) -> f64 {
        self.ticks as f64 * self.dt as f64
    }

    /// A frame of `frame_dt` seconds went by: the whole ticks it completes (at most a tenth
    /// of a second's worth; the rest of a longer stall is dropped).
    pub fn advance(&mut self, frame_dt: f32) -> Ticks {
        let n = advance_fixed_clock(&mut self.accum, frame_dt, self.dt, self.max_per_frame, self.max_per_frame);
        self.ticks += n as u64;
        Ticks { n, alpha: self.alpha() }
    }

    /// Where the picture stands between the last two ticks.
    pub fn alpha(&self) -> f32 {
        ((self.accum / self.dt as f64) as f32).clamp(0.0, 1.0)
    }
}

/// A body's pose as the picture takes it: position, heading (degrees, clockwise from north),
/// pitch and bank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyPose {
    pub position: DVec3,
    pub heading: f64,
    pub pitch: f32,
    pub bank: f32,
}

/// A body that moved further than this in one tick was put somewhere (placed, reset, taken
/// over): it is drawn where it is, not swept there.
const JUMP_M: f64 = 10.0;

impl BodyPose {
    /// The pose `alpha` of the way from `prev` to `self` (the heading the short way round).
    pub fn between(prev: &BodyPose, cur: &BodyPose, alpha: f32) -> BodyPose {
        if (cur.position - prev.position).length() > JUMP_M {
            return *cur;
        }
        let a = alpha.clamp(0.0, 1.0);
        let dh = (cur.heading - prev.heading + 540.0).rem_euclid(360.0) - 180.0;
        BodyPose {
            position: prev.position.lerp(cur.position, a as f64),
            heading: cur.heading - dh * (1.0 - a as f64),
            pitch: prev.pitch + (cur.pitch - prev.pitch) * a,
            bank: prev.bank + (cur.bank - prev.bank) * a,
        }
    }
}

/// The poses of a vehicle as drawn: the body, then each trailer.
pub fn poses_of(v: &crate::VehicleInstance) -> Vec<BodyPose> {
    let mut out = Vec::with_capacity(1 + v.trailers.len());
    out.push(BodyPose { position: v.position, heading: v.heading, pitch: v.pitch, bank: v.bank });
    out.extend(v.trailers.iter().map(|t| BodyPose { position: t.position, heading: t.heading, pitch: t.pitch, bank: t.bank }));
    out
}

/// Puts a vehicle at `poses` (as [`poses_of`] gave them).
pub fn put(v: &mut crate::VehicleInstance, poses: &[BodyPose]) {
    if let Some(b) = poses.first() {
        (v.position, v.heading, v.pitch, v.bank) = (b.position, b.heading, b.pitch, b.bank);
    }
    for (t, p) in v.trailers.iter_mut().zip(poses.iter().skip(1)) {
        (t.position, t.heading, t.pitch, t.bank) = (p.position, p.heading, p.pitch, p.bank);
    }
}

/// The fixed step of a session as its frames take it: the accumulator, the player's vehicle
/// before the last tick (to draw it between), the last tick's daylight, and a line in the log
/// every ten seconds of the wall clock.
pub struct SimTicks {
    pub step: FixedStep,
    /// The tick being run (its end), or the last one run between the frame's ticks.
    now: u64,
    prev: Vec<BodyPose>,
    alpha: f32,
    daylight: Option<crate::Daylight>,
    frames: u32,
    since: Option<std::time::Instant>,
}

impl Default for SimTicks {
    /// The game's: [`RATE_HZ`] ticks a second.
    fn default() -> Self {
        Self::new(RATE_HZ)
    }
}

impl SimTicks {
    pub fn new(rate_hz: f64) -> Self {
        SimTicks { step: FixedStep::new(rate_hz), now: 0, prev: Vec::new(), alpha: 1.0, daylight: None, frames: 0, since: None }
    }

    /// The simulation's time (s) and its ticks since the start: at the end of the tick being
    /// run (the same at any frame rate), between ticks at the end of the last one.
    pub fn sim_time(&self) -> (f64, u64) {
        (self.now as f64 * self.step.dt() as f64, self.now)
    }

    /// The frame runs the `k`-th of its `n` ticks now (`n` as `FixedStep::advance` gave them).
    pub fn enter_tick(&mut self, k: u32, n: u32) {
        self.now = self.step.ticks() - n as u64 + k as u64 + 1;
    }

    /// The vehicle as it stands before the frame's last tick.
    pub fn remember(&mut self, v: Option<&crate::VehicleInstance>) {
        self.prev = v.map(poses_of).unwrap_or_default();
    }

    /// The vehicle's poses now and where the frame draws it (between its last two ticks), or
    /// none when there is nothing to blend.
    pub fn between(&self, v: &crate::VehicleInstance) -> Option<(Vec<BodyPose>, Vec<BodyPose>)> {
        let cur = poses_of(v);
        (!self.prev.is_empty() && cur.len() == self.prev.len()).then(|| {
            let at = self.prev.iter().zip(&cur).map(|(a, b)| BodyPose::between(a, b, self.alpha)).collect();
            (cur, at)
        })
    }

    pub fn to_drawn(&self, v: &mut crate::VehicleInstance) -> Option<(Vec<BodyPose>, Vec<BodyPose>)> {
        let (cur, at) = self.between(v)?;
        put(v, &at);
        Some((cur, at))
    }

    pub fn behind(&self) -> f32 {
        (1.0 - self.alpha) * self.step.dt()
    }

    /// The daylight the frame takes: its last tick's, else the one kept from before.
    pub fn daylight(&self) -> Option<crate::Daylight> {
        self.daylight
    }

    /// The frame's ticks are done: where the picture stands, the last daylight.
    pub fn frame_done(&mut self, ticks: Ticks, daylight: crate::Daylight) {
        self.alpha = ticks.alpha;
        self.daylight = Some(daylight);
        self.frames += 1;
        let since = *self.since.get_or_insert_with(std::time::Instant::now);
        let wall = since.elapsed().as_secs_f32();
        if wall >= 10.0 {
            let (t, n) = self.sim_time();
            log::debug!("fixed step: {n} ticks, {t:.2} s simulated; {} frames in the last {wall:.1} s", self.frames);
            self.frames = 0;
            self.since = Some(std::time::Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ticks of `secs` seconds in frames of the given lengths, repeated.
    fn ticks_of(frames: &[f32], secs: f32) -> u64 {
        let mut f = FixedStep::new(120.0);
        let (mut t, mut k) = (0.0f32, 0);
        while t < secs - 1e-4 {
            let dt = frames[k % frames.len()];
            f.advance(dt);
            t += dt;
            k += 1;
        }
        f.ticks()
    }

    #[test]
    fn the_ticks_do_not_depend_on_the_frame_rate() {
        // ten seconds at 30, 45, 60, 144 and 240 frames a second, and frames of uneven length:
        // 1200 ticks each (one either way where the frames' sum ends a hair before or after)
        for frames in [vec![1.0 / 30.0], vec![1.0 / 45.0], vec![1.0 / 60.0], vec![1.0 / 144.0], vec![1.0 / 240.0], vec![0.011, 0.023, 0.016, 0.0067]] {
            let n = ticks_of(&frames, 10.0);
            assert!((1199..=1201).contains(&n), "{frames:?}: {n} ticks");
        }
        // whole multiples come out exact
        assert_eq!(ticks_of(&[1.0 / 30.0], 10.0), 1200);
        assert_eq!(ticks_of(&[1.0 / 60.0], 10.0), 1200);
    }

    #[test]
    fn a_frame_of_a_tick_s_multiple_gives_that_many_ticks() {
        let mut f = FixedStep::new(120.0);
        for _ in 0..1000 {
            assert_eq!(f.advance(1.0 / 30.0).n, 4);
        }
    }

    #[test]
    fn alpha_is_where_the_frame_ends_between_two_ticks() {
        let mut f = FixedStep::new(100.0);
        let t = f.advance(0.025);
        assert_eq!(t.n, 2);
        assert!((t.alpha - 0.5).abs() < 1e-3, "{t:?}");
        let t = f.advance(0.004);
        assert_eq!(t.n, 0);
        assert!((t.alpha - 0.9).abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn a_stall_is_not_made_up_beyond_a_tenth_of_a_second() {
        let mut f = FixedStep::new(120.0);
        let t = f.advance(2.0);
        assert_eq!(t.n, 13);
        // (and nothing of it is owed afterwards)
        assert!(f.advance(1.0 / 120.0).n <= 2);
    }

    #[test]
    fn a_tick_s_time_is_its_own_whatever_frame_it_runs_in() {
        // the times the ticks run at, at 30 and at 144 frames a second: the same sequence
        let times = |fps: f32| {
            let mut s = SimTicks::new(120.0);
            let mut out = Vec::new();
            while out.len() < 600 {
                let t = s.step.advance(1.0 / fps);
                for k in 0..t.n {
                    s.enter_tick(k, t.n);
                    out.push(s.sim_time().1);
                }
            }
            out.truncate(600);
            out
        };
        assert_eq!(times(30.0), times(144.0));
        assert_eq!(times(30.0), (1..=600).collect::<Vec<u64>>());
    }

    #[test]
    fn a_pose_between_two_ticks() {
        let a = BodyPose { position: DVec3::new(0.0, 0.0, 0.0), heading: 350.0, pitch: 0.0, bank: 1.0 };
        let b = BodyPose { position: DVec3::new(1.0, 2.0, 0.0), heading: 10.0, pitch: 2.0, bank: 1.0 };
        let m = BodyPose::between(&a, &b, 0.5);
        assert!((m.position - DVec3::new(0.5, 1.0, 0.0)).length() < 1e-9);
        assert!((m.heading.rem_euclid(360.0) - 0.0).abs() < 1e-9 || (m.heading.rem_euclid(360.0) - 360.0).abs() < 1e-9, "{m:?}");
        assert_eq!(m.pitch, 1.0);
        assert_eq!(BodyPose::between(&a, &b, 1.0), b);
        // a jump is not swept
        let far = BodyPose { position: DVec3::new(100.0, 0.0, 0.0), ..b };
        assert_eq!(BodyPose::between(&a, &far, 0.3), far);
    }
}
