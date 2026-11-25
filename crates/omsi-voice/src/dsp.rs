//! The sound shaping of voice chat: the microphone's level and gate, the radio's band,
//! compression and squelch, and how loudly a nearby speaker is heard at a distance.

use crate::codec::RATE;
use omsi_audio::radio_fx::Biquad;

/// Linear gain of `db` decibels.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Level of a block in dB full scale (-120 for silence).
pub fn rms_db(x: &[f32]) -> f32 {
    if x.is_empty() {
        return -120.0;
    }
    let ms = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
    (10.0 * ms.max(1e-12).log10()).max(-120.0)
}

/// The microphone gain (a multiplier, 0 to 4), with a soft limit so a loud voice never
/// clips the encoder.
pub fn mic_gain(frame: &mut [f32], gain: f32) {
    let g = if gain.is_finite() {
        gain.clamp(0.0, 4.0)
    } else {
        1.0
    };
    for s in frame {
        let v = *s * g;
        // (linear up to 0.8, then eased towards 1)
        *s = if v.abs() <= 0.8 {
            v
        } else {
            v.signum() * (0.8 + 0.2 * ((v.abs() - 0.8) / 0.2).tanh())
        };
    }
}

/// How loud a nearby speaker is heard `dist` metres away when they carry `range` metres:
/// full within 5 m, then falling smoothly to nothing at the range.
pub fn proximity_gain(dist: f32, range: f32) -> f32 {
    if !dist.is_finite() || !range.is_finite() || range <= 0.0 {
        return 0.0;
    }
    const NEAR: f32 = 5.0;
    if dist <= NEAR {
        return 1.0;
    }
    if dist >= range || range <= NEAR {
        return 0.0;
    }
    ((range - dist) / (range - NEAR)).powf(1.6)
}

/// The gate of the open-microphone mode: opens above a level, stays open a while after it,
/// so the ends of words are not cut. (Push-to-talk needs none.)
pub struct Gate {
    open_db: f32,
    close_db: f32,
    hold: u32,
    left: u32,
    pub open: bool,
}

impl Gate {
    /// `threshold_db`: the level a voice must reach to open the gate.
    pub fn new(threshold_db: f32) -> Gate {
        let t = threshold_db.clamp(-80.0, -5.0);
        Gate {
            open_db: t,
            close_db: t - 4.0,
            hold: 15,
            left: 0,
            open: false,
        }
    }

    pub fn set_threshold(&mut self, threshold_db: f32) {
        let t = threshold_db.clamp(-80.0, -5.0);
        self.open_db = t;
        self.close_db = t - 4.0;
    }

    /// Whether the frame is let through.
    pub fn process(&mut self, frame: &[f32]) -> bool {
        let level = rms_db(frame);
        if level >= self.open_db || self.open && level >= self.close_db {
            self.left = self.hold;
            self.open = true;
        } else if self.left > 0 {
            self.left -= 1;
        } else {
            self.open = false;
        }
        self.open
    }
}

/// The radio's voice: the band of a small speaker (300 Hz to 3.4 kHz), compressed so a quiet
/// speaker is as present as a loud one, a little saturation, and a faint carrier hiss while
/// the key is down.
pub struct RadioVoice {
    hp: [Biquad; 2],
    lp: [Biquad; 2],
    env: f32,
    rng: u32,
}

impl Default for RadioVoice {
    fn default() -> Self {
        let r = RATE as f32;
        RadioVoice {
            hp: [Biquad::high_pass(300.0, r); 2],
            lp: [Biquad::low_pass(3400.0, r); 2],
            env: 0.0,
            rng: 0x1234_5678,
        }
    }
}

impl RadioVoice {
    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    pub fn process(&mut self, frame: &mut [f32]) {
        for s in frame {
            let mut v = *s;
            for b in &mut self.hp {
                v = b.process(v);
            }
            for b in &mut self.lp {
                v = b.process(v);
            }
            // a feed-forward compressor, 4:1 above -24 dB, quick attack and slow release
            let a = v.abs();
            let coef = if a > self.env { 0.2 } else { 0.0008 };
            self.env += (a - self.env) * coef;
            let over_db = (20.0 * self.env.max(1e-5).log10() + 24.0).max(0.0);
            let reduction = db_to_gain(-over_db * 0.75);
            v *= reduction * 2.2;
            // (a little saturation: the cone is driven)
            v = (v * 1.4).tanh() * 0.8;
            *s = v + self.noise() * 0.004;
        }
    }
}

/// The two clicks of a radio: when somebody keys it (a short tick) and when they let go (a
/// burst of noise that dies away), as samples at `RATE`.
pub fn squelch(open: bool) -> Vec<f32> {
    let r = RATE as f32;
    let n = (r * if open { 0.03 } else { 0.07 }) as usize;
    let mut hp = Biquad::high_pass(1200.0, r);
    let mut lp = Biquad::low_pass(4500.0, r);
    let mut rng = if open { 0xA5A5_1234u32 } else { 0x5A5A_4321 };
    (0..n)
        .map(|i| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let t = i as f32 / n as f32;
            let env = if open {
                (1.0 - t).powi(2)
            } else {
                (1.0 - t).powi(3)
            };
            // (the opening one has a faint tone in it, like a roger beep's start)
            let tone = if open {
                (2.0 * std::f32::consts::PI * 1800.0 * i as f32 / r).sin() * 0.5
            } else {
                0.0
            };
            lp.process(hp.process(noise)) * env * 0.5 + tone * env * 0.15
        })
        .collect()
}

/// A tone of `freq` Hz for tests and the synthetic microphone: frame `frame` (960 samples).
pub fn tone_frame(freq: f32, frame: usize, amp: f32) -> Vec<f32> {
    let n = crate::codec::FRAME;
    (0..n)
        .map(|i| {
            amp * (2.0 * std::f32::consts::PI * freq * ((frame * n + i) as f32 / RATE as f32)).sin()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearby_voice_fades_with_distance_to_the_range() {
        assert_eq!(proximity_gain(0.0, 75.0), 1.0);
        assert_eq!(proximity_gain(5.0, 75.0), 1.0);
        assert_eq!(proximity_gain(75.0, 75.0), 0.0);
        assert_eq!(proximity_gain(200.0, 75.0), 0.0);
        let (a, b, c) = (
            proximity_gain(20.0, 75.0),
            proximity_gain(40.0, 75.0),
            proximity_gain(60.0, 75.0),
        );
        assert!(1.0 > a && a > b && b > c && c > 0.0, "{a} {b} {c}");
        // nonsense gives silence, not a blast
        assert_eq!(proximity_gain(f32::NAN, 75.0), 0.0);
        assert_eq!(proximity_gain(10.0, f32::NAN), 0.0);
        assert_eq!(proximity_gain(10.0, -1.0), 0.0);
        // a longer range reaches further
        assert!(proximity_gain(100.0, 150.0) > proximity_gain(100.0, 120.0));
    }

    #[test]
    fn the_gate_opens_on_a_voice_and_holds_the_ends_of_words() {
        let mut g = Gate::new(-40.0);
        let quiet = vec![0.001f32; 960];
        let loud = tone_frame(300.0, 0, 0.3);
        assert!(!g.process(&quiet));
        assert!(g.process(&loud));
        // held for a while after the voice stops
        let held = (0..30).take_while(|_| g.process(&quiet)).count();
        assert!((14..=16).contains(&held), "held {held} frames");
        assert!(!g.process(&quiet));
    }

    #[test]
    fn the_mic_gain_never_clips() {
        let mut f = vec![0.9f32, -0.9, 0.5, 3.0];
        mic_gain(&mut f, 4.0);
        assert!(f.iter().all(|v| v.abs() <= 1.0), "{f:?}");
        let mut f = vec![0.5f32];
        mic_gain(&mut f, 1.0);
        assert_eq!(f, vec![0.5]);
        mic_gain(&mut f, f32::NAN);
        assert_eq!(f, vec![0.5]);
    }

    /// Energy of a block at `freq`.
    fn at(x: &[f32], freq: f32) -> f32 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, s) in x.iter().enumerate() {
            let a = 2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32;
            re += s * a.cos();
            im += s * a.sin();
        }
        (re * re + im * im).sqrt() / x.len() as f32 * 2.0
    }

    #[test]
    fn the_radio_takes_the_deep_and_the_high_and_evens_out_the_level() {
        let run = |freq: f32, amp: f32| {
            let mut r = RadioVoice::default();
            let mut out = Vec::new();
            for f in 0..20 {
                let mut x = tone_frame(freq, f, amp);
                r.process(&mut x);
                out.extend(x);
            }
            at(&out[out.len() - 4800..], freq)
        };
        let mid = run(1000.0, 0.3);
        assert!(run(60.0, 0.3) < mid * 0.1, "no deep bass");
        assert!(run(10_000.0, 0.3) < mid * 0.1, "no air");
        // 20 dB between a quiet and a loud speaker becomes well under 12 dB
        let ratio = run(1000.0, 0.5) / run(1000.0, 0.05);
        assert!(ratio < 4.0 && ratio > 1.0, "ratio {ratio}");
    }

    #[test]
    fn the_squelch_clicks_are_short_and_not_loud() {
        for open in [true, false] {
            let c = squelch(open);
            assert!(c.len() > 1000 && c.len() < 5000);
            let peak = c.iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.02 && peak < 0.6, "peak {peak}");
        }
    }
}
