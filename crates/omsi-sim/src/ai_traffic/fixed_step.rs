//! The traffic's fixed step: every decision and every body moves on in steps of `SIM_DT`,
//! whatever the frame rate. With one step per frame, how a car decided depended on the frame
//! rate (a car at 144 frames a second looked at the junction again before one at 30 had
//! moved), and two runs of the same session at different frame rates drove differently.
//! The frame's time is collected in `sim_accum` and spent in whole steps; what is left over
//! says how far the picture is between the last two steps (`frame_alpha`), and the cars are
//! drawn there (`show_between`), so that they glide at any frame rate.

use super::*;

/// One step of the traffic (s).
pub const SIM_DT: f32 = 0.02;
/// At most this many steps of time are kept to be made up: after a long stall (a hitch, a
/// breakpoint) the traffic does not try to make up for all of it, the rest is dropped.
pub const MAX_SIM_STEPS: u32 = 8;
/// At most this many steps in one frame; what is left of `MAX_SIM_STEPS` is made up in the
/// next frames. Eight steps at once after a hitch made the frame after it a hitch of its
/// own; four keep the traffic on time at any frame rate above 12.5 a second (below that it
/// runs that much slower, as it did below 6.25 before).
pub const STEPS_PER_FRAME: u32 = 4;
/// A part that moved further than this between two steps was put somewhere (placed, taken
/// over, recovered): it is drawn where it is, not swept there.
const JUMP: f64 = 10.0;

/// Add a frame of `frame_dt` seconds to `accum` and take out the whole steps of `dt` it
/// holds: how many to run now, at most `per_frame`. Steps beyond those wait in `accum` for
/// the next frames, up to `max_steps` of them; the time beyond that is dropped. The steps
/// depend only on the time that went by, not on how it was split into frames (as long as
/// no frame needs more than `per_frame`, or the frames after it make up for it).
pub fn advance_fixed_clock(accum: &mut f64, frame_dt: f32, dt: f32, per_frame: u32, max_steps: u32) -> u32 {
    if !(frame_dt > 0.0) || !(dt > 0.0) {
        return 0;
    }
    let dt = dt as f64;
    *accum += frame_dt as f64;
    // (a hair under a whole step counts as one: 0.02 s summed fifty times is not quite 1 s)
    let held = ((*accum + 1e-9) / dt).floor().max(0.0) as u64;
    if held > max_steps as u64 {
        // more than `max_steps` behind: the rest of the debt is dropped, its fraction kept
        *accum = max_steps as f64 * dt + (*accum - held as f64 * dt).max(0.0);
    }
    let n = held.min(max_steps as u64).min(per_frame as u64);
    *accum = (*accum - n as f64 * dt).max(0.0);
    n as u32
}

/// Where one part of a car stood after a step: position, heading, pitch and bank.
pub type PartPose = (DVec3, f64, f32, f32);

impl TrafficSim {
    /// Move the traffic on by a frame of `frame_dt` seconds: as many fixed steps as the time
    /// collected holds (see `advance_fixed_clock`). Returns how many ran. A LAN client's
    /// traffic is the host's and only follows it, frame by frame.
    pub fn advance(&mut self, frame_dt: f32, player: Option<PlayerBox>) -> u32 {
        if self.mirror {
            self.tick(frame_dt, player);
            self.frame_split = self.tick_split;
            self.frame_alpha = 1.0;
            return 1;
        }
        let steps = advance_fixed_clock(&mut self.sim_accum, frame_dt, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS);
        let mut split = [0.0; 3];
        for k in 0..steps {
            if k + 2 >= steps {
                self.remember_poses();
            }
            self.tick(SIM_DT, player);
            for (a, b) in split.iter_mut().zip(self.tick_split) {
                *a += b;
            }
        }
        self.frame_split = split;
        self.frame_alpha = ((self.sim_accum / SIM_DT as f64) as f32).clamp(0.0, 1.0);
        steps
    }

    /// Every car's parts as they stand before a step, to draw them between it and the next.
    /// (Into one flat list kept from step to step: a list per car made and freed fifty
    /// times a second, for every car, was allocator traffic for nothing.)
    fn remember_poses(&mut self) {
        let p = &mut self.poses;
        std::mem::swap(&mut p.before, &mut p.earlier);
        std::mem::swap(&mut p.before_at, &mut p.earlier_at);
        p.before.clear();
        p.before_at.clear();
        for c in &self.cars {
            let v = &c.vehicle;
            p.before_at.insert(c.id, (p.before.len() as u32, 1 + v.trailers.len() as u32));
            p.before.push((v.position, v.heading, v.pitch, v.bank));
            p.before.extend(v.trailers.iter().map(|t| (t.position, t.heading, t.pitch, t.bank)));
        }
    }

    /// Put every car where the picture of this frame shows it, between the last two steps,
    /// keeping where they really stand for `show_actual` to put them back after drawing.
    /// Nothing moves when the frame falls on a step.
    pub fn show_between(&mut self) {
        self.show_between_at(0.0);
    }

    pub fn show_between_at(&mut self, behind: f32) {
        let a = self.frame_alpha - behind.clamp(0.0, SIM_DT) / SIM_DT;
        let p = &mut self.poses;
        p.shown.clear();
        p.actual.clear();
        if a >= 1.0 || self.mirror {
            return;
        }
        for (i, c) in self.cars.iter_mut().enumerate() {
            let n = 1 + c.vehicle.trailers.len();
            let Some(b) = span(&p.before_at, c.id, n) else { continue };
            let before = &p.before[b..b + n];
            let v = &mut c.vehicle;
            if (v.position - before[0].0).length() > JUMP {
                continue;
            }
            let earlier = span(&p.earlier_at, c.id, n)
                .map(|e| &p.earlier[e..e + n])
                .filter(|e| (before[0].0 - e[0].0).length() <= JUMP);
            let k = p.actual.len();
            p.shown.push((i, k as u32));
            p.actual.push((v.position, v.heading, v.pitch, v.bank));
            p.actual.extend(v.trailers.iter().map(|t| (t.position, t.heading, t.pitch, t.bank)));
            let now = &p.actual[k..k + n];
            let (from, to, w) = shown(earlier, before, now, a);
            (v.position, v.heading, v.pitch, v.bank) = between(from[0], to[0], w);
            for (j, t) in v.trailers.iter_mut().enumerate() {
                (t.position, t.heading, t.pitch, t.bank) = between(from[j + 1], to[j + 1], w);
            }
        }
    }

    /// Back where they really stand after the picture was drawn (see `show_between`).
    pub fn show_actual(&mut self) {
        let p = &mut self.poses;
        for &(i, at) in &p.shown {
            let Some(c) = self.cars.get_mut(i) else { continue };
            let v = &mut c.vehicle;
            let saved = &p.actual[at as usize..];
            if let Some(&q) = saved.first() {
                (v.position, v.heading, v.pitch, v.bank) = q;
            }
            for (t, &q) in v.trailers.iter_mut().zip(saved.iter().skip(1)) {
                (t.position, t.heading, t.pitch, t.bank) = q;
            }
        }
        p.shown.clear();
        p.actual.clear();
    }
}

/// The cars' poses kept for drawing between two steps (see `TrafficSim::advance`), in flat
/// lists reused from step to step and frame to frame.
#[derive(Default)]
pub struct StepPoses {
    /// Every car's parts as they stood before the last step ...
    pub before: Vec<PartPose>,
    /// ... where each car's begin in `before` and how many there are, by id.
    pub before_at: HashMap<u64, (u32, u32)>,
    earlier: Vec<PartPose>,
    earlier_at: HashMap<u64, (u32, u32)>,
    /// The cars `show_between` moved (index) and where their real poses begin in `actual`.
    shown: Vec<(usize, u32)>,
    actual: Vec<PartPose>,
}

fn shown<'a>(earlier: Option<&'a [PartPose]>, before: &'a [PartPose], now: &'a [PartPose], a: f32) -> (&'a [PartPose], &'a [PartPose], f32) {
    match earlier.filter(|_| a < 0.0) {
        Some(e) => (e, before, 1.0 + a),
        None => (before, now, a.max(0.0)),
    }
}

fn span(at: &HashMap<u64, (u32, u32)>, id: u64, n: usize) -> Option<usize> {
    at.get(&id).filter(|&&(_, m)| m as usize == n).map(|&(b, _)| b as usize)
}

/// `a` of the way from `from` to `to`, the angles the short way round.
fn between(from: PartPose, to: PartPose, a: f32) -> PartPose {
    let turn = (to.1 - from.1 + 540.0).rem_euclid(360.0) - 180.0;
    (
        from.0.lerp(to.0, a as f64),
        from.1 + turn * a as f64,
        from.2 + (to.2 - from.2) * a,
        from.3 + (to.3 - from.3) * a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_before_the_last_step_is_drawn_between_the_two_before() {
        let at = |y: f64| [(DVec3::new(0.0, y, 0.0), 0.0, 0.0, 0.0)];
        let (earlier, before, now) = (at(0.0), at(1.0), at(2.0));
        let y = |a: f32| {
            let (from, to, w) = shown(Some(&earlier), &before, &now, a);
            between(from[0], to[0], w).0.y
        };
        assert!((y(0.25) - 1.25).abs() < 1e-9);
        assert!((y(-0.25) - 0.75).abs() < 1e-9);
        assert!((y(-1e-6) - y(0.0)).abs() < 1e-5, "no jump where the step is");
        let (from, to, w) = shown(None, &before, &now, -0.25);
        assert!((between(from[0], to[0], w).0.y - 1.0).abs() < 1e-9, "without the step before: where it stood before the last");
    }

    /// The same seconds give the same steps whether they come as 15, 30, 60 or 144 frames
    /// a second.
    #[test]
    fn the_steps_depend_on_the_time_not_the_frame_rate() {
        for fps in [12.5f32, 15.0, 30.0, 60.0, 144.0, 240.0] {
            let mut accum = 0.0;
            let frames = (10.0 * fps).round() as u32;
            let steps: u32 = (0..frames)
                .map(|_| advance_fixed_clock(&mut accum, 1.0 / fps, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS))
                .sum();
            assert_eq!(steps, 500, "{fps} fps");
        }
    }

    #[test]
    fn a_frame_runs_at_most_four_steps_and_the_next_ones_make_up_for_it() {
        let mut accum = 0.0;
        // a hitch of 0.14 s: seven steps, four now and three in the next frame
        assert_eq!(advance_fixed_clock(&mut accum, 0.14, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS), 4);
        assert_eq!(advance_fixed_clock(&mut accum, 0.005, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS), 3);
        assert_eq!(advance_fixed_clock(&mut accum, 0.005, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS), 0);
    }

    #[test]
    fn a_long_stall_keeps_at_most_the_cap_and_drops_the_rest() {
        let mut accum = 0.0;
        let mut steps = advance_fixed_clock(&mut accum, 1.0, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS);
        assert_eq!(steps, STEPS_PER_FRAME);
        assert!(accum < MAX_SIM_STEPS as f64 * SIM_DT as f64);
        for _ in 0..10 {
            steps += advance_fixed_clock(&mut accum, 1e-4, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS);
        }
        assert_eq!(steps, MAX_SIM_STEPS);
        // and the next frame is ordinary again
        assert_eq!(advance_fixed_clock(&mut accum, 0.021, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS), 1);
    }

    #[test]
    fn nothing_runs_for_no_time() {
        let mut accum = 0.0;
        assert_eq!(advance_fixed_clock(&mut accum, 0.0, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS), 0);
        assert_eq!(advance_fixed_clock(&mut accum, f32::NAN, SIM_DT, STEPS_PER_FRAME, MAX_SIM_STEPS), 0);
        assert_eq!(accum, 0.0);
    }

    #[test]
    fn a_heading_is_turned_the_short_way_round() {
        let p = |h: f64| (DVec3::ZERO, h, 0.0, 0.0);
        let mid = between(p(350.0), p(10.0), 0.5);
        assert!((mid.1.rem_euclid(360.0) - 0.0).abs() < 1e-9 || (mid.1 - 360.0).abs() < 1e-9);
        let q = between(p(10.0), p(350.0), 0.25);
        assert!((q.1 - 5.0).abs() < 1e-9);
    }
}
