//! What the bus radio's own sound does to a station on its way to the ear, each part off
//! by default: a cheap speaker (the band of a small cone - no deep bass, no air on top -,
//! nearly mono, and pushed a little into saturation), the hiss and crackle of a weak FM
//! signal, and the moments the signal is lost altogether (a bridge, a tall building in the
//! way) when the music sinks into noise for a second.
//!
//! A voice with all three at 0 carries no effect at all ([`RadioFx::new`] gives none): the
//! mixer then plays the stream exactly as it did without this module.

/// How much of each effect (0 = none, 1 = full).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RadioFxParams {
    pub speaker: f32,
    pub hiss: f32,
    pub dropouts: f32,
}

impl RadioFxParams {
    /// The amounts as the settings give them, each cut to 0..1 (a NaN is none).
    pub fn clamped(self) -> RadioFxParams {
        let c = |v: f32| if v > 0.0 { v.min(1.0) } else { 0.0 };
        RadioFxParams {
            speaker: c(self.speaker),
            hiss: c(self.hiss),
            dropouts: c(self.dropouts),
        }
    }

    pub fn is_off(&self) -> bool {
        let p = self.clamped();
        p.speaker == 0.0 && p.hiss == 0.0 && p.dropouts == 0.0
    }
}

/// A second-order section (the "audio EQ cookbook" filters), transposed direct form II.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    /// A Butterworth low-pass (Q = 1/sqrt 2) at `fc` Hz.
    pub fn low_pass(fc: f32, rate: f32) -> Biquad {
        let (cos, alpha) = Self::omega(fc, rate);
        let b1 = 1.0 - cos;
        Self::normalised(
            [b1 / 2.0, b1, b1 / 2.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    /// A Butterworth high-pass at `fc` Hz.
    pub fn high_pass(fc: f32, rate: f32) -> Biquad {
        let (cos, alpha) = Self::omega(fc, rate);
        let b1 = 1.0 + cos;
        Self::normalised(
            [b1 / 2.0, -b1, b1 / 2.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    fn omega(fc: f32, rate: f32) -> (f32, f32) {
        // (kept under the Nyquist frequency of the rate it runs at)
        let w = 2.0 * std::f32::consts::PI * fc.clamp(1.0, rate * 0.45) / rate.max(1.0);
        (w.cos(), w.sin() / std::f32::consts::SQRT_2)
    }

    fn normalised(b: [f32; 3], a: [f32; 3]) -> Biquad {
        Biquad {
            b: b.map(|x| x / a[0]),
            a: [a[1] / a[0], a[2] / a[0]],
            z: [0.0; 2],
        }
    }

    /// The same filter with new coefficients, its memory kept (no click when they change).
    fn retuned(self, to: Biquad) -> Biquad {
        Biquad { z: self.z, ..to }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// One stream's effects and their state.
pub struct RadioFx {
    params: RadioFxParams,
    rate: u32,
    /// The speaker's band, per channel: a high-pass and a low-pass.
    hp: [Biquad; 2],
    lp: [Biquad; 2],
    /// How hard the speaker is driven (1 = clean).
    drive: f32,
    rng: u64,
    /// The hiss's own high-pass (one pole): FM noise is bright.
    noise_hp: (f32, f32),
    /// A crackle under way: its level now and how quickly it dies.
    crackle: f32,
    crackle_decay: f32,
    /// The signal's strength as heard (1 = full), what it is heading for, and how many
    /// samples are left until the reception changes again.
    signal: f32,
    signal_target: f32,
    signal_left: u32,
}

impl RadioFx {
    /// The effects for `params` at `rate` Hz; None when all are off - nothing is done to
    /// the sound then, not even a multiplication by one.
    pub fn new(params: RadioFxParams, rate: u32) -> Option<RadioFx> {
        if params.is_off() {
            return None;
        }
        let mut fx = RadioFx {
            params: RadioFxParams::default(),
            rate: 0,
            hp: [Biquad::default(); 2],
            lp: [Biquad::default(); 2],
            drive: 1.0,
            rng: 0x9e37_79b9_7f4a_7c15,
            noise_hp: (0.0, 0.0),
            crackle: 0.0,
            crackle_decay: 0.0,
            signal: 1.0,
            signal_target: 1.0,
            signal_left: 0,
        };
        fx.set(params, rate);
        Some(fx)
    }

    /// New amounts or a new rate: the filters are tuned again, their memory kept.
    pub fn set(&mut self, params: RadioFxParams, rate: u32) {
        let params = params.clamped();
        if params == self.params && rate == self.rate {
            return;
        }
        self.params = params;
        self.rate = rate.max(1);
        let r = self.rate as f32;
        // the speaker: from the full band (20 Hz .. 20 kHz) to a small cone's 300 Hz .. 3.5 kHz
        let s = params.speaker;
        let hp = Biquad::high_pass(20.0 * 15f32.powf(s), r);
        let lp = Biquad::low_pass(20_000.0 * (3_500.0f32 / 20_000.0).powf(s), r);
        self.hp = self.hp.map(|b| b.retuned(hp));
        self.lp = self.lp.map(|b| b.retuned(lp));
        self.drive = 1.0 + 1.5 * s;
        self.crackle_decay = (-1.0 / (0.0015 * r)).exp();
    }

    pub fn params(&self) -> RadioFxParams {
        self.params
    }

    /// A random number in 0..1 (xorshift: the same noise every run, no allocation).
    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    /// One stereo frame through the effects.
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let p = self.params;
        let rate = self.rate as f32;
        let (mut l, mut r) = (l, r);
        if p.speaker > 0.0 {
            // one small speaker in the dashboard is close to mono
            let mid = (l + r) * 0.5;
            let keep = 1.0 - 0.8 * p.speaker;
            l = mid + (l - mid) * keep;
            r = mid + (r - mid) * keep;
            l = self.lp[0].process(self.hp[0].process(l));
            r = self.lp[1].process(self.hp[1].process(r));
            // (tanh(k x) / k: the same level for quiet passages, the peaks rounded off)
            let k = self.drive;
            l = (k * l).tanh() / k;
            r = (k * r).tanh() / k;
        }
        let mut noise_level = 0.03 * p.hiss;
        if p.dropouts > 0.0 {
            if self.signal_left == 0 {
                // in the clear for a while (12 s on average at full), then lost for a
                // moment of 0.15 .. 1.2 s
                if self.signal_target < 1.0 {
                    self.signal_target = 1.0;
                    let mean = 12.0 / p.dropouts;
                    self.signal_left =
                        (-(1.0 - self.random()).max(1e-6).ln() * mean * rate) as u32 + 1;
                } else {
                    self.signal_target = 0.04 + 0.2 * self.random();
                    self.signal_left = ((0.15 + 1.05 * self.random()) * rate) as u32;
                }
            }
            self.signal_left -= 1;
            // (the signal fades over some 20 ms, as a receiver's level control follows it)
            self.signal += (self.signal_target - self.signal) * (1.0 / (0.02 * rate)).min(1.0);
            l *= self.signal;
            r *= self.signal;
            // a receiver with no signal turns the noise up: the lost moments hiss
            noise_level += 0.06 * (1.0 - self.signal);
        }
        if noise_level > 0.0 {
            let white = self.random() * 2.0 - 1.0;
            // one pole at about 1.5 kHz, taken off: what is left is the bright part
            let a = (-2.0 * std::f32::consts::PI * 1_500.0 / rate).exp();
            self.noise_hp.1 = white * (1.0 - a) + self.noise_hp.1 * a;
            let mut n = (white - self.noise_hp.1) * noise_level;
            if p.hiss > 0.0 {
                // crackles, four a second at full
                if self.random() < 4.0 * p.hiss / rate {
                    let sign = if self.random() < 0.5 { -1.0 } else { 1.0 };
                    self.crackle = sign * (0.05 + 0.2 * self.random()) * p.hiss;
                }
                n += self.crackle;
                self.crackle *= self.crackle_decay;
            }
            l += n;
            r += n;
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// The level (RMS) a filter leaves of a sine at `hz`, after it settled.
    fn gain_of(f: &mut dyn FnMut(f32) -> f32, hz: f32) -> f32 {
        let n = RATE as usize;
        let mut sum = 0.0f64;
        let mut ref_sum = 0.0f64;
        for i in 0..n {
            let x = (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin();
            let y = f(x);
            if i > n / 2 {
                sum += (y * y) as f64;
                ref_sum += (x * x) as f64;
            }
        }
        (sum / ref_sum).sqrt() as f32
    }

    fn db(g: f32) -> f32 {
        20.0 * g.log10()
    }

    #[test]
    fn butterworth_sections_are_3_db_down_at_their_corner() {
        let mut lp = Biquad::low_pass(1_000.0, RATE as f32);
        assert!((db(gain_of(&mut |x| lp.process(x), 1_000.0)) + 3.01).abs() < 0.1);
        let mut lp = Biquad::low_pass(1_000.0, RATE as f32);
        assert!(
            db(gain_of(&mut |x| lp.process(x), 100.0)).abs() < 0.05,
            "the pass band is flat"
        );
        let mut lp = Biquad::low_pass(1_000.0, RATE as f32);
        assert!(
            db(gain_of(&mut |x| lp.process(x), 10_000.0)) < -38.0,
            "12 dB an octave: some 40 dB a decade"
        );
        let mut hp = Biquad::high_pass(300.0, RATE as f32);
        assert!((db(gain_of(&mut |x| hp.process(x), 300.0)) + 3.01).abs() < 0.1);
        let mut hp = Biquad::high_pass(300.0, RATE as f32);
        assert!(db(gain_of(&mut |x| hp.process(x), 30.0)) < -38.0);
    }

    #[test]
    fn nothing_is_made_for_effects_all_at_zero() {
        assert!(RadioFx::new(RadioFxParams::default(), RATE).is_none());
        assert!(
            RadioFx::new(
                RadioFxParams {
                    speaker: -1.0,
                    hiss: f32::NAN,
                    dropouts: 0.0
                },
                RATE
            )
            .is_none()
        );
        assert!(
            RadioFx::new(
                RadioFxParams {
                    hiss: 0.1,
                    ..Default::default()
                },
                RATE
            )
            .is_some()
        );
    }

    #[test]
    fn a_cheap_speaker_keeps_the_voice_band_and_loses_bass_and_treble() {
        let mut fx = RadioFx::new(
            RadioFxParams {
                speaker: 1.0,
                ..Default::default()
            },
            RATE,
        )
        .unwrap();
        // (quiet, so that the saturation stays out of it)
        let mut run = |hz: f32| {
            fx.hp = [Biquad::default(); 2].map(|b| b.retuned(fx.hp[0]));
            fx.lp = [Biquad::default(); 2].map(|b| b.retuned(fx.lp[0]));
            let mut f = |x: f32| fx.process(0.05 * x, 0.05 * x).0 / 0.05;
            db(gain_of(&mut f, hz))
        };
        let mid = run(1_000.0);
        assert!(mid.abs() < 1.0, "1 kHz: {mid} dB");
        assert!(run(60.0) < -15.0, "60 Hz");
        assert!(run(12_000.0) < -15.0, "12 kHz");
    }

    #[test]
    fn the_speaker_rounds_off_loud_peaks_only() {
        let mut fx = RadioFx::new(
            RadioFxParams {
                speaker: 1.0,
                ..Default::default()
            },
            RATE,
        )
        .unwrap();
        let mut peak = 0.0f32;
        for i in 0..RATE {
            let x = (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / RATE as f32).sin();
            peak = peak.max(fx.process(x, x).0.abs());
        }
        assert!(peak < 0.45 && peak > 0.3, "{peak}");
    }

    #[test]
    fn hiss_on_silence_is_quiet_and_bright() {
        let mut fx = RadioFx::new(
            RadioFxParams {
                hiss: 1.0,
                ..Default::default()
            },
            RATE,
        )
        .unwrap();
        let out: Vec<f32> = (0..RATE).map(|_| fx.process(0.0, 0.0).0).collect();
        let rms = (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms > 0.005 && rms < 0.06, "{rms}");
        assert!(out.iter().all(|x| x.abs() < 0.5));
        // bright: neighbouring samples hardly alike (white noise's correlation is 0, a
        // low rumble's near 1)
        let corr: f32 = out.windows(2).map(|w| w[0] * w[1]).sum::<f32>()
            / out.iter().map(|x| x * x).sum::<f32>();
        assert!(corr < 0.5, "{corr}");
    }

    #[test]
    fn dropouts_come_now_and_then_and_hiss_while_the_signal_is_gone() {
        let mut fx = RadioFx::new(
            RadioFxParams {
                dropouts: 1.0,
                ..Default::default()
            },
            RATE,
        )
        .unwrap();
        // a minute of a steady tone: most of it heard as it is, a few moments of it lost
        let (mut lost, mut clear) = (0usize, 0usize);
        for _ in 0..60 * RATE {
            let (_, _) = fx.process(0.5, 0.5);
            if fx.signal < 0.3 {
                lost += 1;
            } else if fx.signal > 0.99 {
                clear += 1;
            }
        }
        let n = (60 * RATE) as usize;
        assert!(lost > n / 200 && lost < n / 4, "{lost} of {n}");
        assert!(clear > n / 2, "{clear} of {n}");
    }

    #[test]
    fn new_amounts_keep_the_filters_running() {
        let mut fx = RadioFx::new(
            RadioFxParams {
                speaker: 0.5,
                ..Default::default()
            },
            RATE,
        )
        .unwrap();
        for _ in 0..100 {
            fx.process(0.3, 0.3);
        }
        let z = fx.hp[0].z;
        fx.set(
            RadioFxParams {
                speaker: 0.8,
                ..Default::default()
            },
            RATE,
        );
        assert_eq!(fx.hp[0].z, z);
        assert_eq!(fx.params().speaker, 0.8);
    }
}
