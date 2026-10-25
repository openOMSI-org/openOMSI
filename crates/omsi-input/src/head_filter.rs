//! Head tracking's pose made calm for the camera, frame by frame.
//! A Tobii eye tracker gives the head some 30 times a second, with noise in it: drawn as it
//! came, the view stood still for a few frames and then jumped, and the noise - made 2.5 times
//! larger and more by the sensitivities - shook it. As opentrack and Tobii's own Extended View
//! (what Euro Truck Simulator 2's eye tracking settings are made of) do it:
//!
//! - between two of the tracker's poses the view glides: the pose aimed at goes from where the
//!   last glide was to the new pose in the time between the two (a pose behind the head);
//! - an adaptive filter follows that, per axis and in steps of 4 ms: a small difference (the
//!   tracker's trembling) is followed hardly at all, a large one (a real turn) at once; while
//!   the head really moves - told by how far the pose aimed at gets from its own slow average,
//!   which the trembling does not reach - the view keeps up, and a moment after, so it neither
//!   lags nor creeps after a turn; a dead zone round where the view is leaves out movements
//!   smaller than it, wherever the head looks (not only straight ahead);
//! - with no pose (the tracker lost the face, the middle being taken, head tracking switched
//!   off) the view glides back straight ahead instead of jumping there; not followed for a
//!   while (another view) it starts afresh where the head is.
//!
//! The player sets the smoothing and the dead zone (`Params::tobii`); opentrack's poses, which
//! opentrack filters itself, are only glided (`Params::OPENTRACK`). All in the head's own terms
//! (degrees, cm), before the tracker's turn is made larger (`Sample::gain`).

use std::time::{Duration, Instant};

use crate::headtrack::HeadPose;

/// A pose as a tracker gave it, for `HeadFilter`.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// The head's pose from the middle, in its own terms (opentrack's: degrees, cm).
    pub pose: HeadPose,
    /// The turn made larger after the filter (yaw, pitch, roll).
    pub gain: [f32; 3],
    /// Yaw added after the gain (degrees: a look at the screen's edge).
    pub edge: f32,
    /// Which pose it is (one more with every pose of its source), when it came, and the time
    /// since the one before it (s).
    pub seq: u64,
    pub at: Instant,
    pub interval: f32,
    /// From a Tobii eye tracker (the player's smoothing), else from opentrack.
    pub tobii: bool,
}

/// How strongly the filter works, for the rotation (degrees) and the position (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// A difference this large (beyond the dead zone) is followed at full speed, smaller ones
    /// less and less (0: all at full speed).
    knee: [f32; 2],
    /// A difference within this is left out.
    dead: [f32; 2],
    /// The time the view takes to come most of the way at full speed (s).
    tau: [f32; 2],
}

impl Params {
    /// A Tobii eye tracker's: `smoothing` 0..1 (the player's; at 0.5 the knee is opentrack
    /// Accela's 1.5 degrees and 1 cm), `dead_zone` in degrees (and as many cm). Position is
    /// smoothed more: its noise moves the eye, and the dashboard and the mirrors swim with it.
    pub fn tobii(smoothing: f32, dead_zone: f32) -> Params {
        let s = smoothing.clamp(0.0, 1.0);
        let d = dead_zone.max(0.0);
        Params { knee: [3.0 * s, 2.0 * s], dead: [d, d], tau: [0.02 + 0.06 * s, 0.03 + 0.09 * s] }
    }

    /// opentrack's poses, which opentrack filters itself (its Accela): only glided.
    pub const OPENTRACK: Params = Params { knee: [0.0; 2], dead: [0.0; 2], tau: [0.02; 2] };
}

/// The channels: yaw, pitch, roll (degrees), x, y, z (cm), and the edge's yaw (degrees).
const N: usize = 7;
/// The filter's step at most (s): opentrack's 250 Hz.
const STEP: f32 = 0.004;
/// The pose aimed at, eased this slowly (s): a head that really moves gets away from it by
/// more than the trembling does.
const SLOW: f32 = 0.3;
/// While the head really moves the view keeps up, taking this long to (s) - not at once: a
/// switch in one step made a steady turn surge three times a second - and lets go in about
/// this long after it stopped (s), so it does not creep after it.
const RISE: f32 = 0.04;
const HOLD: f32 = 0.3;
/// With no pose the view goes back straight ahead in about this long (s): Tobii's 5 a second.
const HOME: f32 = 0.2;
/// Not stepped for this long (s; another view, no bus) the filter starts afresh, where the
/// head is now.
const STALE: f32 = 0.25;

/// A glide from one pose to the next: from, to, when it began and how long it takes (s).
#[derive(Clone, Copy)]
struct Glide {
    from: [f32; N],
    to: [f32; N],
    t0: Instant,
    span: f32,
}

impl Glide {
    fn at(&self, t: Instant) -> [f32; N] {
        let f = (t.saturating_duration_since(self.t0).as_secs_f32() / self.span).clamp(0.0, 1.0);
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * f)
    }
}

/// The filter's state (the camera's).
#[derive(Default)]
pub struct HeadFilter {
    out: [f32; N],
    hold: [f32; N],
    /// The pose aimed at, eased slowly (`SLOW`).
    slow: [f32; N],
    /// The glide to the latest pose, and the one before it (followed until the latest came:
    /// a frame that sees a new pose has steps from before it).
    glide: Option<Glide>,
    before: Option<Glide>,
    /// The pose the glide goes to (its source, its number) and its gain.
    key: Option<(bool, u64)>,
    gain: [f32; 3],
    /// The last frame's time.
    last: Option<Instant>,
    /// A pose is followed, or the view is on its way home.
    live: bool,
    /// The next pose is taken as it is (the filter started afresh).
    snap: bool,
}

impl HeadFilter {
    /// The pose for the frame at `now` from the tracker's latest (`s`; none: there is none),
    /// with `tobii` for a Tobii tracker's poses. None once the view is straight ahead again.
    pub fn step(&mut self, s: Option<&Sample>, now: Instant, tobii: Params) -> Option<HeadPose> {
        let gap = self.last.map(|t| now.saturating_duration_since(t).as_secs_f32());
        // (not stepped for a while - another view, no bus: what it had is old, the head is
        // where the tracker says now)
        if gap.is_some_and(|g| g > STALE) {
            *self = HeadFilter { snap: true, ..HeadFilter::default() };
        }
        self.last = Some(now);
        let dt = gap.unwrap_or(0.0).min(0.1);
        let Some(s) = s else {
            self.key = None;
            self.glide = None;
            self.before = None;
            if !self.live {
                return None;
            }
            // (home: no dead zone, nothing held)
            let k = (-dt / HOME).exp();
            for v in self.out.iter_mut() {
                *v *= k;
            }
            self.slow = self.out;
            self.hold = [0.0; N];
            if self.out.iter().all(|v| v.abs() < 0.05) {
                *self = HeadFilter { last: self.last, ..HeadFilter::default() };
                return None;
            }
            return Some(self.pose());
        };
        let p = if s.tobii { tobii } else { Params::OPENTRACK };
        let v = [s.pose.rot[0], s.pose.rot[1], s.pose.rot[2], s.pose.pos[0], s.pose.pos[1], s.pose.pos[2], s.edge];
        if self.key != Some((s.tobii, s.seq)) {
            // (a new pose: the glide goes on from where the last one is at the time it came -
            // after none, from where the view is)
            let mut from = self.glide.map_or(self.out, |g| g.at(s.at));
            let mut before = self.glide;
            // (another source - opentrack's poses and a Tobii tracker's in "auto" - turns by
            // another gain: what is followed is put in its terms, so the view does not jump)
            if self.live {
                for i in 0..3 {
                    if s.gain[i] != 0.0 && s.gain[i] != self.gain[i] {
                        let r = self.gain[i] / s.gain[i];
                        self.out[i] *= r;
                        self.slow[i] *= r;
                        from[i] *= r;
                        before = None;
                    }
                }
            }
            let mut to = v;
            for i in 0..3 {
                from[i] = wrap(from[i]);
                to[i] = from[i] + wrap(v[i] - from[i]);
            }
            if self.snap {
                (from, to, self.out, self.slow, before) = (v, v, v, v, None);
                self.snap = false;
            }
            self.before = before;
            self.glide = Some(Glide { from, to, t0: s.at, span: s.interval.clamp(0.004, 0.1) });
            self.key = Some((s.tobii, s.seq));
            self.gain = s.gain;
            self.live = true;
        }
        let n = (dt / STEP).ceil().max(1.0) as u32;
        let h = dt / n as f32;
        let (rise, fall) = ((-h / RISE).exp(), (-h / HOLD).exp());
        let ease = 1.0 - (-h / SLOW).exp();
        for j in 1..=n {
            let t = now.checked_sub(Duration::from_secs_f32((n - j) as f32 * h)).unwrap_or(now);
            let target = self.target(t);
            for i in 0..N {
                let (knee, dead, tau) = match i {
                    0..=2 => (p.knee[0], p.dead[0], p.tau[0]),
                    3..=5 => (p.knee[1], p.dead[1], p.tau[1]),
                    _ => (0.0, 0.0, 0.02),
                };
                let diff = |a: f32, b: f32| if i < 3 { wrap(a - b) } else { a - b };
                // (a real movement: the pose aimed at gets away from its slow average - told by
                // the head alone, not by the view, which made it swing)
                self.slow[i] += diff(target[i], self.slow[i]) * ease;
                self.hold[i] = if diff(target[i], self.slow[i]).abs() >= knee + dead { 1.0 - (1.0 - self.hold[i]) * rise } else { self.hold[i] * fall };
                let d = diff(target[i], self.out[i]);
                // (the dead zone round the view as it is: no jump at its edge)
                let m = (d.abs() - dead).max(0.0);
                // (a soft gate: the trembling hardly moves the view, a turn moves it at once)
                let g = if knee > 0.0 {
                    let f = (m / knee).powi(2);
                    f / (1.0 + f)
                } else {
                    1.0
                };
                let k = g.max(self.hold[i]);
                self.out[i] += d.signum() * m * (1.0 - (-k * h / tau).exp());
            }
            for i in 0..3 {
                self.out[i] = wrap(self.out[i]);
                self.slow[i] = wrap(self.slow[i]);
            }
        }
        Some(self.pose())
    }

    /// Where the glide is at `t` (before the latest pose came: where the one before it was).
    fn target(&self, t: Instant) -> [f32; N] {
        match (self.glide, self.before) {
            (Some(g), Some(b)) if t < g.t0 => b.at(t),
            (Some(g), _) => g.at(t),
            (None, _) => self.out,
        }
    }

    /// The filtered pose, the tracker's turn made larger and the edge's yaw on top.
    fn pose(&self) -> HeadPose {
        let o = &self.out;
        HeadPose { pos: [o[3], o[4], o[5]], rot: [wrap(o[0] * self.gain[0] + o[6]), o[1] * self.gain[1], o[2] * self.gain[2]] }
    }
}

/// An angle the short way round (degrees, -180..180).
fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noise as a tracker's: normal, from a fixed seed (xorshift, Box-Muller).
    struct Noise(u64);

    impl Noise {
        fn uniform(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 11) as f64 / (1u64 << 53) as f64) as f32
        }

        fn normal(&mut self, sigma: f32) -> f32 {
            let (a, b) = (self.uniform().max(1e-7), self.uniform());
            (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos() * sigma
        }
    }

    /// A frame of `run`: its time, the view's yaw, pitch and x, and the head's yaw then.
    #[derive(Clone, Copy, Debug)]
    struct Frame {
        t: f32,
        yaw: f32,
        pitch: f32,
        x: f32,
        head: f32,
    }

    /// A tracker at `hz` giving `head(t)` (yaw, pitch degrees; x cm) with noise `sigma`
    /// (degrees, and half of it in cm) and the gain `gain`, read by frames at `fps` for `secs`
    /// seconds through a filter with `p`.
    #[allow(clippy::too_many_arguments)]
    fn run_g(head: &dyn Fn(f32) -> (f32, f32, f32), sigma: f32, hz: f32, fps: f32, secs: f32, p: Params, tobii: bool, gain: [f32; 3]) -> Vec<Frame> {
        let start = Instant::now();
        let mut noise = Noise(0x9e37_79b9_7f4a_7c15);
        let mut f = HeadFilter::default();
        let mut out = Vec::new();
        let mut seq = 0u64;
        let mut sample: Option<Sample> = None;
        let frames = (secs * fps).round() as usize;
        for k in 0..=frames {
            let t = k as f32 / fps;
            // (the tracker's poses that came by this frame: the last of them is read)
            while (seq as f32) / hz <= t + 1e-6 {
                let ts = seq as f32 / hz;
                let (y, pi, x) = head(ts);
                let pose = HeadPose { pos: [x + noise.normal(sigma * 0.5), 0.0, 0.0], rot: [y + noise.normal(sigma), pi + noise.normal(sigma), 0.0] };
                sample = Some(Sample { pose, gain, edge: 0.0, seq, at: start + Duration::from_secs_f32(ts), interval: 1.0 / hz, tobii });
                seq += 1;
            }
            let r = f.step(sample.as_ref(), start + Duration::from_secs_f32(t), p).unwrap_or_default();
            out.push(Frame { t, yaw: r.rot[0], pitch: r.rot[1], x: r.pos[0], head: head(t).0 });
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn run(head: &dyn Fn(f32) -> (f32, f32, f32), sigma: f32, hz: f32, fps: f32, secs: f32, p: Params, tobii: bool) -> Vec<Frame> {
        run_g(head, sigma, hz, fps, secs, p, tobii, [1.0; 3])
    }

    /// The frame at (or just after) `t`.
    fn at(r: &[Frame], t: f32) -> Frame {
        *r.iter().find(|x| x.t >= t - 1e-4).unwrap()
    }

    fn std(v: &[f32]) -> f32 {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    }

    fn spread(v: &[f32]) -> f32 {
        v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
    }

    /// The settings' defaults: smoothing 50 %, dead zone 0.1 degrees.
    fn calm() -> Params {
        Params::tobii(0.5, 0.1)
    }

    /// A head held still (straight ahead and turned 15 degrees, where the old dead zone did
    /// not reach) with the tracker trembling 0.3 degrees: the view stays still.
    #[test]
    fn a_still_head_gives_a_still_view() {
        for at in [0.0, 15.0] {
            let r = run(&|_| (at, 0.0, 0.0), 0.3, 30.0, 144.0, 10.0, calm(), true);
            let rest: Vec<_> = r.iter().filter(|x| x.t >= 1.0).collect();
            let yaw: Vec<f32> = rest.iter().map(|x| x.yaw).collect();
            let pitch: Vec<f32> = rest.iter().map(|x| x.pitch).collect();
            let pos: Vec<f32> = rest.iter().map(|x| x.x).collect();
            let step = yaw.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
            assert!(std(&yaw) <= 0.06 && std(&pitch) <= 0.06, "at {at}: std yaw {} pitch {}", std(&yaw), std(&pitch));
            assert!(spread(&yaw) <= 0.3 && step <= 0.05, "at {at}: spread {}, step {step}", spread(&yaw));
            assert!(std(&pos) <= 0.03, "at {at}: std x {}", std(&pos));
            assert!((yaw.iter().sum::<f32>() / yaw.len() as f32 - at).abs() < 0.5);
        }
    }

    /// The tracker's gain comes after the filter: the same trembling at a Tobii tracker's gain
    /// is the same view made larger, frame by frame (made larger first, the trembling it lets
    /// through was twice as much).
    #[test]
    fn the_gain_comes_after() {
        let head = |_| (15.0, 3.0, 0.0);
        let a = run_g(&head, 0.3, 30.0, 144.0, 4.0, calm(), true, [1.0; 3]);
        let b = run_g(&head, 0.3, 30.0, 144.0, 4.0, calm(), true, [2.5, 2.0, 1.0]);
        for (x, y) in a.iter().zip(&b) {
            assert!((y.yaw - 2.5 * x.yaw).abs() <= 1e-3 && (y.pitch - 2.0 * x.pitch).abs() <= 1e-3, "{x:?} {y:?}");
        }
        // (and the edge's yaw on top)
        let start = Instant::now();
        let mut f = HeadFilter::default();
        let s = Sample { pose: HeadPose { pos: [0.0; 3], rot: [10.0, 4.0, 0.0] }, gain: [2.5, 2.0, 1.0], edge: 5.0, seq: 1, at: start, interval: 0.033, tobii: true };
        let mut x = HeadPose::default();
        for k in 0..300 {
            x = f.step(Some(&s), start + Duration::from_millis(4 * k), Params::tobii(0.5, 0.0)).unwrap();
        }
        assert!((x.rot[0] - 30.0).abs() < 0.05 && (x.rot[1] - 8.0).abs() < 0.05, "{x:?}");
    }

    /// The head turned 20 degrees at once: the view follows within a fifth of a second of
    /// the pose's coming, never past it.
    #[test]
    fn a_fast_turn_is_followed() {
        for fps in [60.0, 144.0] {
            let r = run(&|t| (if t >= 1.0 { 20.0 } else { 0.0 }, 0.0, 0.0), 0.0, 30.0, fps, 2.0, calm(), true);
            assert!(at(&r, 1.2).yaw >= 18.0, "{fps} fps: {} after 0.2 s", at(&r, 1.2).yaw);
            assert!(at(&r, 1.35).yaw >= 19.6, "{fps} fps: {} after 0.35 s", at(&r, 1.35).yaw);
            assert!(r.iter().all(|x| x.yaw <= 20.0 + 1e-3));
        }
    }

    /// A steady turn: the view turns as steadily, frame by frame - no frame standing still
    /// and the next jumping, as with the tracker's 30 poses a second drawn as they came.
    #[test]
    fn a_turn_glides_between_samples() {
        let fps = 144.0;
        let r = run(&|t| (30.0 * t, 0.0, 0.0), 0.0, 30.0, fps, 2.0, calm(), true);
        for w in r.windows(2).filter(|w| w[0].t >= 0.3) {
            let speed = (w[1].yaw - w[0].yaw) * fps;
            assert!((27.0..=33.0).contains(&speed), "{speed} degrees a second at {}", w[0].t);
        }
        let r = run(&|t| (60.0 * t, 0.0, 0.0), 0.0, 30.0, fps, 1.5, calm(), true);
        for x in r.iter().filter(|x| x.t >= 0.5) {
            assert!(x.head - x.yaw <= 6.0, "{} behind at {}", x.head - x.yaw, x.t);
        }
    }

    /// Slow and steady turns, the kind a look round the cab is made of, at every smoothing:
    /// the view turns as steadily (the keeping up switched on in one step made it surge
    /// three times a second between 10 and 25 degrees a second).
    #[test]
    fn a_slow_turn_does_not_surge() {
        let fps = 144.0;
        for smoothing in [0.25, 0.5, 1.0] {
            for v in [5.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0] {
                let r = run(&|t| (v * t, 0.0, 0.0), 0.0, 30.0, fps, 3.0, Params::tobii(smoothing, 0.1), true);
                for w in r.windows(2).filter(|w| w[0].t >= 1.5) {
                    let speed = (w[1].yaw - w[0].yaw) * fps;
                    assert!((0.85 * v..=1.15 * v).contains(&speed), "smoothing {smoothing}, {v} degrees a second: {speed} at {}", w[0].t);
                }
            }
        }
    }

    /// A turn that stops, fast or slow: the view settles where the head stopped, without
    /// creeping after it for seconds.
    #[test]
    fn the_view_settles_after_a_turn() {
        let r = run(&|t| (60.0 * t.min(0.6), 0.0, 0.0), 0.0, 30.0, 144.0, 1.5, calm(), true);
        assert!((36.0 - at(&r, 0.85).yaw).abs() <= 0.1 + 0.3, "{} at 0.25 s after the stop", at(&r, 0.85).yaw);
        for v in [8.0, 10.0, 15.0] {
            let r = run(&|t| (v * t.min(2.0), 0.0, 0.0), 0.0, 30.0, 144.0, 4.0, calm(), true);
            let end = 2.0 * v;
            assert!((end - at(&r, 2.5).yaw).abs() <= 0.1 + 0.05, "{v}: {} at 0.5 s after the stop", at(&r, 2.5).yaw);
            assert!((at(&r, 4.0).yaw - at(&r, 2.5).yaw).abs() <= 0.02, "{v}: still creeping");
        }
    }

    /// The dead zone: a movement within it leaves the view where it is - straight ahead and
    /// turned 15 degrees - and a larger one moves it that much less.
    #[test]
    fn the_dead_zone_leaves_small_movements_out() {
        let p = Params::tobii(0.5, 0.5);
        let r = run(&|t| (if t >= 1.0 { 0.4 } else { 0.0 }, 0.0, 0.0), 0.0, 30.0, 144.0, 5.0, p, true);
        assert!(r.last().unwrap().yaw.abs() <= 1e-4, "{}", r.last().unwrap().yaw);
        let r = run(&|t| (if t >= 1.0 { 5.0 } else { 0.0 }, 0.0, 0.0), 0.0, 30.0, 144.0, 5.0, p, true);
        assert!((r.last().unwrap().yaw - 4.5).abs() <= 0.05, "{}", r.last().unwrap().yaw);
        // (turned 15 degrees, the head going 0.4 to and fro: the view stands)
        let r = run(&|t| (15.0 - if t >= 2.0 && (t * 3.0) as i32 % 2 == 0 { 0.4 } else { 0.0 }, 0.0, 0.0), 0.0, 30.0, 144.0, 6.0, p, true);
        let late: Vec<f32> = r.iter().filter(|x| x.t >= 1.9).map(|x| x.yaw).collect();
        assert!(spread(&late) <= 1e-3, "{}", spread(&late));
    }

    /// The tracker lost: the view glides home from where it was (never faster than Tobii's 5
    /// a second, and not at once) and lets go; the tracker back: the view glides there again
    /// rather than jumping.
    #[test]
    fn lost_tracking_glides_home() {
        let start = Instant::now();
        let p = calm();
        let mut f = HeadFilter::default();
        let held = |seq: u64, yaw: f32, at: f32| Sample { pose: HeadPose { pos: [0.0; 3], rot: [yaw, 0.0, 0.0] }, gain: [1.0; 3], edge: 0.0, seq, at: start + Duration::from_secs_f32(at), interval: 1.0 / 30.0, tobii: true };
        let fps = 144.0;
        let mut t = 0.0;
        let mut k = 0;
        let mut tracked = 0.0;
        while t < 1.0 {
            tracked = f.step(Some(&held(k / 5, 20.0, (k / 5) as f32 / 30.0)), start + Duration::from_secs_f32(t), p).unwrap().rot[0];
            k += 1;
            t = k as f32 / fps;
        }
        let mut was = f.step(None, start + Duration::from_secs_f32(t), p).unwrap().rot[0];
        assert!(was <= tracked && tracked - was <= 1.5 * 20.0 / fps / HOME, "{tracked} -> {was}");
        let mut gone = None;
        // (stepped every frame, as the camera steps it, until the tracker is back)
        let back = t + 1.6;
        for j in 1..(1.6 * fps) as usize {
            let now = t + j as f32 / fps;
            match f.step(None, start + Duration::from_secs_f32(now), p) {
                Some(x) => {
                    assert!(gone.is_none(), "back from home at {now}");
                    let most = was * (1.0 - (-1.0 / fps / HOME).exp()) + 1e-3;
                    assert!(was - x.rot[0] <= most && x.rot[0] <= was, "{} -> {}", was, x.rot[0]);
                    was = x.rot[0];
                }
                None => {
                    gone = gone.or(Some(now - t));
                }
            }
        }
        assert!(gone.is_some_and(|g| (1.0..=1.5).contains(&g)), "{gone:?}");
        // (back, at 10 degrees: no frame closes more than a sixth of the gap)
        let mut was = 0.0f32;
        for j in 0..(0.5 * fps) as usize {
            let now = back + j as f32 / fps;
            let s = held(100 + (j / 5) as u64, 10.0, back + (j / 5) as f32 / 30.0);
            let x = f.step(Some(&s), start + Duration::from_secs_f32(now), p).map_or(0.0, |x| x.rot[0]);
            assert!(x - was <= 0.15 * 10.0, "{was} -> {x}");
            was = x;
        }
        assert!(was > 9.0, "{was}");
    }

    /// The frame rate does not change how the view follows: a turn seen at 30, 60, 120 and 240
    /// frames a second is where it is at the same moments, from the first frames after it on.
    #[test]
    fn the_same_at_every_frame_rate() {
        let runs: Vec<Vec<Frame>> = [30.0, 60.0, 120.0, 240.0]
            .iter()
            .map(|&fps| run(&|t| (if t >= 1.0 { 20.0 } else { 0.0 }, 0.0, 0.0), 0.0, 30.0, fps, 1.5, calm(), true))
            .collect();
        for (t, most) in [(1.0 + 1.0 / 30.0, 0.5), (1.0 + 2.0 / 30.0, 0.5), (1.1, 0.5), (1.2, 0.2)] {
            let v: Vec<f32> = runs.iter().map(|r| at(r, t).yaw).collect();
            assert!(spread(&v) <= most, "at {t}: {v:?}");
        }
    }

    /// opentrack's poses (filtered by opentrack, some 250 a second) are only glided: they are
    /// followed closely, whatever the smoothing.
    #[test]
    fn opentrack_is_only_glided() {
        let r = run(&|t| (60.0 * t, 0.0, 0.0), 0.0, 250.0, 144.0, 1.0, Params::tobii(1.0, 1.0), false);
        for x in r.iter().filter(|x| x.t >= 0.3) {
            assert!(x.head - x.yaw <= 1.5, "{} behind", x.head - x.yaw);
        }
        let r = run(&|t| (if t >= 0.5 { 1.0 } else { 0.0 }, 0.0, 0.0), 0.0, 250.0, 144.0, 1.0, Params::tobii(1.0, 1.0), false);
        assert!(at(&r, 0.56).yaw >= 0.9, "{} after 60 ms", at(&r, 0.56).yaw);
    }

    /// opentrack's yaw past straight behind (170 on to -170): the view turns on the short way,
    /// not 340 degrees back.
    #[test]
    fn the_yaw_turns_on_past_behind() {
        let r = run(&|t| (wrap(150.0 + 40.0 * t), 0.0, 0.0), 0.0, 250.0, 144.0, 1.5, Params::OPENTRACK, false);
        for w in r.windows(2).filter(|w| w[0].t >= 0.2) {
            assert!(wrap(w[1].yaw - w[0].yaw).abs() <= 1.0, "{} -> {} at {}", w[0].yaw, w[1].yaw, w[0].t);
            assert!(w[1].yaw.abs() <= 180.0);
        }
        assert!((wrap(r.last().unwrap().yaw - 210.0)).abs() <= 1.5, "{}", r.last().unwrap().yaw);
    }

    /// Every rotation channel crosses the boundary along the shortest arc, both ways.
    #[test]
    fn all_rotations_take_the_short_arc() {
        for axis in 0..3 {
            for sign in [-1.0, 1.0] {
                let start = Instant::now();
                let mut filter = HeadFilter::default();
                let mut previous: Option<f32> = None;
                for frame in 0..216u64 {
                    let t = frame as f32 / 144.0;
                    let mut rot = [0.0; 3];
                    rot[axis] = wrap(sign * (150.0 + 40.0 * t));
                    let sample = Sample { pose: HeadPose { rot, pos: [0.0; 3] }, gain: [1.0; 3], edge: 0.0, seq: frame, at: start + Duration::from_secs_f32(t), interval: 1.0 / 144.0, tobii: false };
                    let out = filter.step(Some(&sample), sample.at, Params::OPENTRACK).unwrap().rot[axis];
                    if let Some(was) = previous.filter(|_| t >= 0.3) {
                        assert!(wrap(out - was).abs() < 1.0, "axis {axis}: {was} -> {out}");
                    }
                    previous = Some(out);
                }
                assert!(wrap(previous.unwrap() - sign * 210.0).abs() < 2.0, "axis {axis}: {previous:?}");
            }
        }
    }

    /// In "auto", opentrack's poses giving way to a Tobii tracker's (and back) for the same
    /// head: the view goes on where it was, though the two turn by another gain.
    #[test]
    fn a_change_of_source_does_not_jump() {
        let start = Instant::now();
        let mut f = HeadFilter::default();
        let ot = |seq, at: f32| Sample { pose: HeadPose { pos: [0.0; 3], rot: [20.0, 0.0, 0.0] }, gain: [1.0; 3], edge: 0.0, seq, at: start + Duration::from_secs_f32(at), interval: 0.004, tobii: false };
        let tobii = |seq, at: f32| Sample { pose: HeadPose { pos: [0.0; 3], rot: [8.0, 0.0, 0.0] }, gain: [2.5, 2.0, 1.0], edge: 0.0, seq, at: start + Duration::from_secs_f32(at), interval: 1.0 / 30.0, tobii: true };
        let mut was: Option<f32> = None;
        for k in 0..432u64 {
            let t = k as f32 / 144.0;
            let s = if (1.0..2.0).contains(&t) { tobii(k / 5, t) } else { ot(k, t) };
            let x = f.step(Some(&s), start + Duration::from_secs_f32(t), calm()).unwrap().rot[0];
            if let Some(w) = was.filter(|_| t > 0.5) {
                assert!((x - w).abs() <= 0.5, "{w} -> {x} at {t}");
            }
            was = Some(x);
        }
    }

    /// Not stepped for a while (another view, no bus): the view starts where the head is now,
    /// not from where it was then.
    #[test]
    fn a_stale_filter_starts_afresh() {
        let start = Instant::now();
        let mut f = HeadFilter::default();
        let held = |seq, yaw, at: f32| Sample { pose: HeadPose { pos: [0.0; 3], rot: [yaw, 0.0, 0.0] }, gain: [2.5, 2.0, 1.0], edge: 0.0, seq, at: start + Duration::from_secs_f32(at), interval: 1.0 / 30.0, tobii: true };
        for k in 0..144u64 {
            f.step(Some(&held(k / 5, 20.0, (k / 5) as f32 / 30.0)), start + Duration::from_secs_f32(k as f32 / 144.0), calm());
        }
        let x = f.step(Some(&held(500, 0.0, 2.0)), start + Duration::from_secs_f32(2.0), calm()).unwrap();
        assert!(x.rot[0].abs() <= 1e-3, "{x:?}");
        // (and with no pose then: nothing)
        let mut g = HeadFilter::default();
        g.step(Some(&held(1, 20.0, 0.0)), start, calm());
        assert!(g.step(None, start + Duration::from_secs(2), calm()).is_none());
    }
}
