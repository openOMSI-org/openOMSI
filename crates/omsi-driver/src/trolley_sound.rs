//! The sounds of a trolleybus's gear: the hiss of the shoes on the wire (louder with the
//! current and the speed, with the rectifier's ripple in it), the crack of an arc, the thud of a
//! shoe on the wire and of a pole on its hook, the contactors, a frog's clatter. They are made
//! from noise and decaying tones when first wanted, not read from files, and played where they
//! happen.

use glam::Vec3;
use omsi_audio::{AudioEngine, Clip, VoiceId, VoiceParams};
use omsi_trolley::RigSound;
use std::sync::Arc;

const RATE: u32 = 44100;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32 / (1u32 << 23) as f32) - 1.0
    }
}

fn clip(samples: Vec<f32>) -> Arc<Clip> {
    Arc::new(Clip { sample_rate: RATE, channels: 1, samples: samples.into_iter().map(|s| (s.clamp(-1.0, 1.0) * 30000.0) as i16).collect() })
}

/// Sliding contact: noise tilted towards the highs, a loop of one second, with the six-pulse
/// rectifier's 300 Hz ripple riding on its level.
pub fn hiss() -> Arc<Clip> {
    let n = RATE as usize;
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let raw: Vec<f32> = (0..n).map(|_| r.next()).collect();
    let out: Vec<f32> = (0..n)
        .map(|i| {
            // (circular, so the loop closes without a click)
            let x = raw[i] - 0.82 * raw[(i + n - 1) % n];
            let ripple = 1.0 + 0.18 * (std::f32::consts::TAU * 300.0 * i as f32 / RATE as f32).sin();
            0.6 * x * ripple
        })
        .collect();
    clip(out)
}

/// An arc's crack: a burst of noise falling in a few milliseconds over a sharp tone.
pub fn snap() -> Arc<Clip> {
    let n = (RATE as f32 * 0.22) as usize;
    let mut r = Rng(0x1234_5678_9abc_def1);
    clip(
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let burst = r.next() * (-t * 90.0).exp();
                let tone = (std::f32::consts::TAU * (2600.0 - 5000.0 * t) * t).sin() * (-t * 140.0).exp() * 0.6;
                0.9 * (burst + tone)
            })
            .collect(),
    )
}

/// A hollow knock: a shoe dropping on a wire, a pole on its hook.
pub fn thud() -> Arc<Clip> {
    let n = (RATE as f32 * 0.3) as usize;
    let mut r = Rng(0x0bad_cafe_0bad_cafe);
    clip(
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.8 * (std::f32::consts::TAU * (95.0 - 40.0 * t) * t).sin() * (-t * 22.0).exp() + 0.25 * r.next() * (-t * 120.0).exp()
            })
            .collect(),
    )
}

/// A contactor closing: a click and a short low thump.
pub fn click() -> Arc<Clip> {
    let n = (RATE as f32 * 0.09) as usize;
    let mut r = Rng(0x5555_aaaa_5555_aaaa);
    clip(
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.7 * r.next() * (-t * 400.0).exp() + 0.5 * (std::f32::consts::TAU * 140.0 * t).sin() * (-t * 60.0).exp()
            })
            .collect(),
    )
}

/// A shoe crossing a frog: three quick knocks.
pub fn frog() -> Arc<Clip> {
    let n = (RATE as f32 * 0.25) as usize;
    let mut r = Rng(0x7777_1111_7777_1111);
    clip(
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let mut s = 0.0;
                for (k, at) in [0.0f32, 0.045, 0.1].iter().enumerate() {
                    let u = t - at;
                    if u >= 0.0 {
                        s += (0.8 - 0.2 * k as f32) * (r.next() * 0.6 + (std::f32::consts::TAU * 700.0 * u).sin() * 0.5) * (-u * 160.0).exp();
                    }
                }
                s
            })
            .collect(),
    )
}

/// The pole latching on its hook or the hook letting go: a metallic clack.
pub fn clack() -> Arc<Clip> {
    let n = (RATE as f32 * 0.16) as usize;
    clip(
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.6 * ((std::f32::consts::TAU * 880.0 * t).sin() + 0.7 * (std::f32::consts::TAU * 1530.0 * t).sin()) * (-t * 55.0).exp()
            })
            .collect(),
    )
}

#[derive(Default)]
pub struct TrolleySound {
    clips: Option<Clips>,
    hiss_voice: Option<VoiceId>,
}

struct Clips {
    hiss: Arc<Clip>,
    snap: Arc<Clip>,
    thud: Arc<Clip>,
    click: Arc<Clip>,
    frog: Arc<Clip>,
    clack: Arc<Clip>,
}

impl TrolleySound {
    /// One frame of the gear's sounds, heard from `listener` (the camera).
    pub fn update(&mut self, audio: &AudioEngine, v: &mut omsi_sim::VehicleInstance, sparks_at: &[Vec3], paused: bool) {
        let Some(g) = v.trolley.as_deref_mut() else { return };
        let c = self.clips.get_or_insert_with(|| Clips { hiss: hiss(), snap: snap(), thud: thud(), click: click(), frog: frog(), clack: clack() });
        let sounds = g.rig.take_sounds();
        let tip = g.rig.hiss_pos.as_vec3();
        let roof = v.position.as_vec3() + Vec3::new(0.0, 0.0, 3.0);
        if paused {
            return;
        }
        // the hiss: a loop kept running, its level following the current and the speed
        let level = g.rig.hiss;
        match self.hiss_voice {
            Some(id) if audio.is_playing(id) => {
                audio.set_params(id, VoiceParams { gain: level * 0.5, pitch: 0.85 + 0.35 * level, looping: true, position: Some(tip), doppler: false, range: 10.0, lowpass_hz: 0.0, important: false });
            }
            _ if level > 0.01 => {
                self.hiss_voice = Some(audio.play_from_random_place(c.hiss.clone(), VoiceParams { gain: level * 0.5, pitch: 1.0, looping: true, position: Some(tip), doppler: false, range: 10.0, lowpass_hz: 0.0, important: false }));
            }
            _ => {}
        }
        if level <= 0.01 {
            if let Some(id) = self.hiss_voice.take() {
                audio.stop(id);
            }
        }
        let one = |clip: &Arc<Clip>, gain: f32, at: Vec3, range: f32| {
            audio.play(clip.clone(), VoiceParams { gain, pitch: 1.0, looping: false, position: Some(at), doppler: false, range, lowpass_hz: 0.0, important: true });
        };
        let mut sp = sparks_at.iter().copied();
        for s in sounds {
            match s {
                RigSound::Click => one(&c.click, 0.5, roof - Vec3::Y * 2.0, 6.0),
                RigSound::Thud(l) => one(&c.thud, 0.3 + 0.5 * l, tip, 12.0),
                RigSound::Snap(l) => one(&c.snap, 0.3 + 0.7 * l, sp.next().unwrap_or(tip), 40.0),
                RigSound::Frog => one(&c.frog, 0.7, tip, 20.0),
                RigSound::Release | RigSound::Latch => one(&c.clack, 0.5, roof - Vec3::Y * 4.0, 10.0),
            }
        }
    }
}
