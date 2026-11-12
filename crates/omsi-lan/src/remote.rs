//! Another player's states as they came, and the state drawn: a little in the past, between
//! two of their states (or carried on from the last one for a moment), on a clock that
//! follows theirs smoothly (`PlayClock`).

use crate::mirror::{PlayClock, lerp_angle};
use omsi_net::Pose;
use std::time::Instant;

/// Their states by their clock (s), oldest first, and how far our clock (`lan_now`) is ahead
/// of theirs as the quickest state showed it: the bus is drawn where their states put it a
/// little in the past, between two of them - never pulled towards the newest as it happened
/// to arrive (network jitter made it jump a few centimetres at every state, and whoever
/// stood in it shook).
#[derive(Default)]
pub struct PoseTrack {
    samples: std::collections::VecDeque<(f64, Pose)>,
    offset: Option<f64>,
    /// The moment of theirs drawn (see `PlayClock`).
    play: PlayClock,
    /// Timestamp on the owner clock currently used for pose and animation playback.
    pub draw_time: Option<f64>,
    /// Additional buffer required by animation samples sent less often than poses.
    pub min_delay: f64,
    /// Changes when the owner clock restarts, invalidating dependent animation samples.
    pub generation: u64,
    /// A parked bus's scripts: the seconds they have run, and the frames since they last did.
    still: (f32, u32),
}

/// A server's parked bus stands with everything off and nobody in it: once its scripts have
/// run this long (s) they run every so many frames only - nothing of it moves, and two dozen
/// of them run every frame took the frame as long as drawing the world.
const SETTLED: f32 = 5.0;
const STILL_EVERY: u32 = 30;

/// Seconds on our clock (for comparing with the others' `sent_ms`).
pub fn lan_now() -> f64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// How far in the past the others' buses are drawn (s): two states at 20 a second, and
/// room for one late one.
pub const INTERP_DELAY: f64 = 0.12;

impl PoseTrack {
    /// Are the scripts of bus `id` left alone this frame (see `SETTLED`)?
    pub fn stands_still(&mut self, id: u32, dt: f32) -> bool {
        if !omsi_net::is_parked(id) {
            return false;
        }
        if self.still.0 < SETTLED {
            self.still.0 += dt;
            return false;
        }
        self.still.1 = (self.still.1 + 1) % STILL_EVERY;
        self.still.1 != 0
    }

    /// Take in the states that came (with when they arrived).
    pub fn take(&mut self, history: &std::collections::VecDeque<(Instant, Pose)>) {
        let now_i = Instant::now();
        let now = lan_now();
        for (at, p) in history {
            if p.sent_ms == 0 {
                continue;
            }
            let sent = p.sent_ms as f64 / 1000.0;
            if self.samples.back().map(|b| sent <= b.0).unwrap_or(false) {
                // (their game started again: its clock began anew)
                if self
                    .samples
                    .back()
                    .map(|b| b.0 - sent > 30.0)
                    .unwrap_or(false)
                {
                    self.generation = self.generation.wrapping_add(1);
                    self.samples.clear();
                    self.offset = None;
                    self.play = Default::default();
                } else {
                    continue;
                }
            }
            let arrived = now - now_i.saturating_duration_since(*at).as_secs_f64();
            let o = arrived - sent;
            // the quickest way over the network sets the offset; it may creep up slowly
            // (clocks drift, the way changes)
            self.offset = Some(match self.offset {
                Some(off) if o >= off => off + (o - off) * 0.01,
                _ => o,
            });
            self.samples.push_back((sent, p.clone()));
            while self.samples.len() > 40 {
                self.samples.pop_front();
            }
        }
    }

    /// Their state as it was `INTERP_DELAY` ago: between the two states around that moment,
    /// or carried on from the last one along its way for a short while. None without
    /// stamped states (an older game).
    pub fn interpolated(&mut self) -> Option<Pose> {
        self.draw_time = None;
        // (`OMSI_NO_INTERP=1`: the old way, for comparing)
        static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *OFF.get_or_init(|| omsi_cfg::flags::OMSI_NO_INTERP.is_set()) {
            return None;
        }
        let off = self.offset?;
        let (last_t, last) = self.samples.back()?;
        // as far back as two of their recent states apart: a bus that stood still is sent
        // five times a second, and 0.12 s behind the newest state lay past it - the doors,
        // the wheels and the walker waited for each state and then jumped to it
        let n = self.samples.len();
        let gap = if n >= 2 {
            self.samples
                .iter()
                .skip(n.saturating_sub(4))
                .zip(self.samples.iter().skip(n.saturating_sub(4) + 1))
                .map(|(a, b)| b.0 - a.0)
                .fold(0.0f64, f64::max)
        } else {
            0.05
        };
        // (the moment drawn follows that smoothly: set anew each frame, it went back a
        // few hundredths of a second whenever one of their frames had taken long, and on
        // again when that one was no longer among the last four - the bus jumped)
        let now = lan_now();
        let t = self.play.step(
            now,
            now - off
                - (gap * 2.0 + 0.02).clamp(
                    INTERP_DELAY.max(self.min_delay),
                    0.45f64.max(self.min_delay),
                ),
            0.5,
        );
        self.draw_time = Some(t);
        let k = self.samples.iter().rposition(|(st, _)| *st <= t);
        let Some(k) = k else {
            return self.samples.front().map(|x| x.1.clone());
        };
        if k + 1 >= self.samples.len() {
            // past the newest: along its heading at its speed, for at most a third of a second
            let ahead = (t - last_t).clamp(0.0, 0.3);
            let mut p = last.clone();
            let h = (p.heading as f64).to_radians();
            let d = (p.speed_kmh as f64 / 3.6) * ahead;
            p.x += h.sin() * d;
            p.y += h.cos() * d;
            for r in p.rear.iter_mut() {
                r.x += h.sin() * d;
                r.y += h.cos() * d;
            }
            // the walker on foot goes on its own way (held back, it stood still and jumped
            // on with every state, and the planted feet were torn along behind)
            if let Some(w) = p
                .walker
                .as_mut()
                .filter(|w| w.aboard.is_none() && !w.seated)
            {
                let c = if w.course.is_finite() {
                    w.course
                } else {
                    w.heading
                } as f64;
                let wd = w.speed as f64 * ahead;
                w.x += c.to_radians().sin() * wd;
                w.y += c.to_radians().cos() * wd;
            }
            return Some(p);
        }
        let (ta, a) = &self.samples[k];
        let (tb, b) = &self.samples[k + 1];
        let f = if tb > ta {
            ((t - ta) / (tb - ta)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        // a jump (placed elsewhere): no gliding across it
        if ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt() > 40.0 {
            return Some(if f < 0.5 { a.clone() } else { b.clone() });
        }
        let l = |x: f64, y: f64| x + (y - x) * f;
        let lf = |x: f32, y: f32| x + (y - x) * f as f32;
        let mut p = b.clone();
        p.x = l(a.x, b.x);
        p.y = l(a.y, b.y);
        p.z = l(a.z, b.z);
        p.heading = lerp_angle(a.heading as f64, b.heading as f64, f) as f32;
        p.pitch = lf(a.pitch, b.pitch);
        p.bank = lf(a.bank, b.bank);
        p.speed_kmh = lf(a.speed_kmh, b.speed_kmh);
        p.steer_deg = lf(a.steer_deg, b.steer_deg);
        p.rpm = lf(a.rpm, b.rpm);
        let mix = |x: &[f32], y: &[f32]| -> Vec<f32> {
            if x.len() == y.len() {
                x.iter()
                    .zip(y)
                    .map(|(u, v)| u + (v - u) * f as f32)
                    .collect()
            } else {
                y.to_vec()
            }
        };
        p.doors = mix(&a.doors, &b.doors);
        p.suspension = mix(&a.suspension, &b.suspension);
        p.values = mix(&a.values, &b.values);
        if a.rear.len() == b.rear.len() {
            for (r, (ra, rb)) in p.rear.iter_mut().zip(a.rear.iter().zip(&b.rear)) {
                r.x = l(ra.x, rb.x);
                r.y = l(ra.y, rb.y);
                r.z = l(ra.z, rb.z);
                r.heading = lerp_angle(ra.heading as f64, rb.heading as f64, f) as f32;
            }
        }
        if let (Some(wa), Some(wb)) = (a.walker, p.walker.as_mut()) {
            wb.x = l(wa.x, wb.x);
            wb.y = l(wa.y, wb.y);
            wb.z = l(wa.z, wb.z);
            wb.heading = lerp_angle(wa.heading as f64, wb.heading as f64, f) as f32;
            if wa.course.is_finite() && wb.course.is_finite() {
                wb.course = lerp_angle(wa.course as f64, wb.course as f64, f) as f32;
            }
            wb.speed = lf(wa.speed, wb.speed);
            if let (Some(aa), Some(ab)) = (wa.aboard, wb.aboard.as_mut()) {
                if aa.owner == ab.owner {
                    for i in 0..3 {
                        ab.local[i] = lf(aa.local[i], ab.local[i]);
                    }
                }
            }
        }
        Some(p)
    }
    /// Whether their clock against ours is known yet (a stamped state came).
    pub fn has_offset(&self) -> bool {
        self.offset.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parked_bus_runs_its_scripts_until_they_have_settled_and_now_and_then_after() {
        let mut t = PoseTrack::default();
        assert!((0..100).all(|_| !t.stands_still(7, 0.25)), "a player's bus runs them every frame");
        let bus = omsi_net::FIRST_PARKED_ID;
        assert_eq!((0..20).filter(|_| !t.stands_still(bus, 0.25)).count(), 20, "five seconds, every frame");
        assert_eq!((0..300).filter(|_| !t.stands_still(bus, 0.25)).count(), 10, "then one frame in thirty");
    }

    #[test]
    fn animations_share_the_pose_playback_timestamp_and_required_buffer() {
        let mut track = PoseTrack {
            min_delay: 0.42,
            ..Default::default()
        };
        let at = Instant::now();
        let history = [
            (
                at,
                Pose {
                    sent_ms: 1000,
                    ..Default::default()
                },
            ),
            (
                at,
                Pose {
                    sent_ms: 1100,
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect();
        track.take(&history);
        assert!(track.interpolated().is_some());
        let t = track.draw_time.expect("pose playback timestamp");
        assert!((0.65..0.8).contains(&t), "buffered owner timestamp: {t}");
    }

    #[test]
    fn a_restarted_owner_clock_invalidates_dependent_animation_samples() {
        let mut track = PoseTrack::default();
        let at = Instant::now();
        track.take(
            &[(
                at,
                Pose {
                    sent_ms: 40000,
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        );
        assert_eq!(track.generation, 0);
        track.take(
            &[(
                at,
                Pose {
                    sent_ms: 100,
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        );
        assert_eq!(track.generation, 1);
        assert_eq!(track.samples.len(), 1);
        assert_eq!(track.samples[0].1.sent_ms, 100);
    }
}
