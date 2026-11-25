//! Playing the recordings: a layer goes from take to take of its slot with a crossfade (no
//! loop point is ever heard, and the takes of a slot - a minute each, from several
//! recordings - follow each other in no fixed order); a one-shot plays once where it is put.

use super::pack::{Pack, Take};
use glam::DVec3;
use omsi_audio::{AudioEngine, VoiceId, VoiceParams};

/// Seconds two takes of a layer overlap.
const CROSSFADE: f64 = 4.0;
/// Seconds a layer takes to follow its target loudness (a place changes as one walks).
const GLIDE: f32 = 1.5;

/// Where a layer or a one-shot is heard from.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Placing {
    /// `None`: around the listener, not from anywhere in particular.
    pub at: Option<DVec3>,
    /// Full loudness up to this distance, 1/d beyond (see `omsi_audio::distance_gain`).
    pub range: f32,
    /// A low-pass in Hz (0: none): through the bodywork, from far away.
    pub lowpass: f32,
}

struct Playing {
    id: VoiceId,
    started: f64,
    seconds: f64,
    /// Its gain against the slot's level (its loudness made up to the slot's).
    trim: f32,
}

/// One slot played as a continuous layer.
pub struct Layer {
    pub slot: &'static str,
    /// The loudness it is played at now and the one it moves to (linear, 0 … ).
    gain: f32,
    pub target: f32,
    pub placing: Placing,
    playing: Vec<Playing>,
    last_take: Option<usize>,
    waiting: Option<usize>,
}

impl Layer {
    pub fn new(slot: &'static str) -> Layer {
        Layer {
            slot,
            gain: 0.0,
            target: 0.0,
            placing: Placing::default(),
            playing: Vec::new(),
            last_take: None,
            waiting: None,
        }
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn is_silent(&self) -> bool {
        self.playing.is_empty() && self.gain <= 1.0e-4
    }

    /// One frame: `now` is the ambience's own clock (s), `level` the slot's loudness in
    /// LUFS at a target of 1, `rng` a fresh random number 0 … 1.
    pub fn tick(
        &mut self,
        engine: &AudioEngine,
        pack: &Pack,
        now: f64,
        dt: f32,
        level: f32,
        rng: f32,
    ) {
        let k = 1.0 - (-dt / GLIDE).exp();
        self.gain += (self.target - self.gain) * k;
        if self.target <= 1.0e-4 && self.gain < 2.0e-4 {
            self.gain = 0.0;
        }
        let takes = pack.takes(self.slot);
        if takes.is_empty() {
            return;
        }
        // the takes that ended (or whose fade-out is through) go
        self.playing.retain(|p| {
            let over = now - p.started >= p.seconds;
            if over {
                engine.stop(p.id);
            }
            !over
        });
        if self.gain <= 0.0 {
            for p in self.playing.drain(..) {
                engine.stop(p.id);
            }
            self.waiting = None;
            return;
        }
        // the next take: when nothing plays, or the last one is into its crossfade
        let due = self
            .playing
            .last()
            .is_none_or(|p| now - p.started >= p.seconds - CROSSFADE);
        if due {
            let i = *self
                .waiting
                .get_or_insert_with(|| pick(takes.len(), self.last_take, rng));
            let take = &takes[i];
            if engine.clips_ready(std::slice::from_ref(&take.path)) {
                self.waiting = None;
                if let Some(clip) = engine.load_clip(&take.path) {
                    let seconds = (clip.frames() as f64 / clip.sample_rate.max(1) as f64).max(1.0);
                    let trim = trim(take, level);
                    let id = engine.play(clip, self.params(0.0));
                    self.playing.push(Playing {
                        id,
                        started: now,
                        seconds,
                        trim,
                    });
                    self.last_take = Some(i);
                }
            }
        }
        for p in &self.playing {
            let t = now - p.started;
            let fade = CROSSFADE.min(p.seconds / 3.0);
            // (equal power: two takes crossing keep the layer's loudness)
            let up = (t / fade).clamp(0.0, 1.0);
            let down = ((p.seconds - t) / fade).clamp(0.0, 1.0);
            let env = ((up * std::f64::consts::FRAC_PI_2).sin()
                * (down * std::f64::consts::FRAC_PI_2).sin()) as f32;
            engine.set_params(p.id, self.params(self.gain * p.trim * env));
        }
    }

    fn params(&self, gain: f32) -> VoiceParams {
        VoiceParams {
            gain,
            pitch: 1.0,
            looping: false,
            position: self.placing.at.map(|p| p.as_vec3()),
            doppler: false,
            range: if self.placing.at.is_some() {
                self.placing.range.max(0.1)
            } else {
                1.0
            },
            lowpass_hz: self.placing.lowpass,
            important: false,
        }
    }

    pub fn stop(&mut self, engine: &AudioEngine) {
        for p in self.playing.drain(..) {
            engine.stop(p.id);
        }
        self.gain = 0.0;
        self.target = 0.0;
    }
}

/// The gain that brings a take from its own loudness to `level` LUFS.
pub fn trim(take: &Take, level: f32) -> f32 {
    10f32.powf((level - take.lufs) / 20.0).min(8.0)
}

/// A take other than the last one, where there is a choice.
pub fn pick(n: usize, last: Option<usize>, rng: f32) -> usize {
    if n <= 1 {
        return 0;
    }
    let mut i = ((rng * n as f32) as usize).min(n - 1);
    if Some(i) == last {
        i = (i + 1) % n;
    }
    i
}

/// Play one take of `slot` once, at `level` LUFS (times `gain`), where `placing` says. The
/// clip is read in the background the first time: `false` until it is there.
pub fn one_shot(
    engine: &AudioEngine,
    pack: &Pack,
    slot: &str,
    level: f32,
    gain: f32,
    placing: Placing,
    rng: f32,
    last: &mut hashbrown::HashMap<String, usize>,
) -> bool {
    let takes = pack.takes(slot);
    if takes.is_empty() {
        return true;
    }
    let i = pick(takes.len(), last.get(slot).copied(), rng);
    let take = &takes[i];
    if !engine.clips_ready(std::slice::from_ref(&take.path)) {
        return false;
    }
    let Some(clip) = engine.load_clip(&take.path) else {
        return true;
    };
    last.insert(slot.to_string(), i);
    engine.play(
        clip,
        VoiceParams {
            gain: gain * trim(take, level),
            pitch: 1.0,
            looping: false,
            position: placing.at.map(|p| p.as_vec3()),
            doppler: false,
            range: placing.range.max(0.1),
            lowpass_hz: placing.lowpass,
            important: false,
        },
    );
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_take_is_not_played_twice_in_a_row() {
        for k in 0..100 {
            let r = k as f32 / 100.0;
            assert_ne!(super::pick(3, Some(1), r), 1);
            assert!(super::pick(3, None, r) < 3);
        }
        assert_eq!(super::pick(1, Some(0), 0.5), 0);
    }

    #[test]
    fn a_quiet_take_is_brought_up_to_its_slot() {
        let t = super::Take {
            path: Default::default(),
            lufs: -33.0,
            region: None,
        };
        assert!((super::trim(&t, -27.0) - 2.0).abs() < 0.01);
    }
}
