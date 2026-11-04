//! The sounds of a trolleybus's gear: the hiss of the shoes on the wire (louder with the
//! current and the speed, with the rectifier's ripple in it), the crack of an arc, the thud of a
//! shoe on the wire and of a pole on its hook, the contactors, a frog's clatter. They are made
//! from noise and decaying tones when first wanted, not read from files, and played where they
//! happen. A vehicle plays its own instead where its `sound.cfg` has an entry on the sound's
//! trigger (`ev_trolleyhitup_1`...) or reads `Trolley_Hiss`; the map's `trolley.txt` can switch the
//! overhead line's own (frog, switch) off or give a file for them (see `docs/TROLLEYBUS.md`, Sounds).

use glam::Vec3;
use crate::soundset::{cab_lowpass_hz, outside_vol};
use crate::{AudioEngine, Clip, SoundSet, VoiceId, VoiceParams};
use omsi_trolley::{PoleMode, RigSound, SoundChoice, TrolleyRig, WireNet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

const RATE: u32 = 44100;
/// The hiss at its loudest (full current at speed), outside the bus.
const HISS_GAIN: f32 = 0.035;
const HISS_NEAR: f32 = 1.0;
const HISS_FAR: f32 = 5.0;

/// The share of a sound on the roof that comes through a shut cab.
const ROOF_SHUT: f32 = 0.5;
/// The share of a switch's clunk out on the line that comes through a shut cab.
const SWITCH_SHUT: f32 = 0.2;

/// How an outside sound is heard from the player's cab, with `open` its `Snd_OutsideVol` and
/// `shut` the share that comes through with all shut: (gain factor, low-pass Hz), the less
/// through the duller, clear as doors and windows open. Fixed when the sound starts.
fn cab(inside: bool, open: f32, shut: f32) -> (f32, f32) {
    let open = if open.is_finite() { open.clamp(0.0, 1.0) } else { 0.0 };
    let base = 0.8 * shut;
    if inside { (shut + (1.0 - shut) * open, cab_lowpass_hz(base + (1.0 - base) * open)) } else { (1.0, 0.0) }
}

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
fn hiss() -> Arc<Clip> {
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
fn snap() -> Arc<Clip> {
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
fn thud() -> Arc<Clip> {
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
fn click() -> Arc<Clip> {
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
fn frog() -> Arc<Clip> {
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
fn clack() -> Arc<Clip> {
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
    hiss_voices: Vec<Option<VoiceId>>,
}

#[derive(Default)]
pub struct SwitchSound {
    network: Weak<WireNet>,
    clip: Option<Arc<Clip>>,
}

impl SwitchSound {
    /// Drain each actual switch movement once and play the map's sound at the frog; `inside`:
    /// the listener is in the player's cab.
    pub fn update(&mut self, audio: &AudioEngine, net: &Arc<WireNet>, inside: bool, paused: bool) -> usize {
        let events = net.take_switch_sounds();
        if paused || events.is_empty() {
            return 0;
        }
        let network = Arc::downgrade(net);
        if !self.network.ptr_eq(&network) {
            self.clip = pick(audio, &net.sounds.switch, net.dir.as_deref(), clack);
            self.network = network;
        }
        let Some(clip) = &self.clip else { return 0 };
        let count = events.len();
        // (a switch drive's clunk carries across a junction, and the driver listens for it)
        let (through, lowpass_hz) = cab(inside, outside_vol(), SWITCH_SHUT);
        for position in events {
            audio.play(clip.clone(), VoiceParams {
                gain: 0.6 * through, pitch: 1.0, looping: false, position: Some(position.as_vec3()),
                doppler: false, range: 15.0, lowpass_hz, important: true,
            });
        }
        count
    }
}

fn hiss_params(level: f32, inside: bool, contact: Vec3, listener: Vec3) -> VoiceParams {
    let distance = contact.distance(listener);
    let gain = if inside || !distance.is_finite() || !level.is_finite() {
        0.0
    } else {
        let fade = ((HISS_FAR - distance) / (HISS_FAR - HISS_NEAR)).clamp(0.0, 1.0);
        level.clamp(0.0, 1.0) * HISS_GAIN * fade * fade
    };
    VoiceParams {
        gain, pitch: 0.85 + 0.35 * level, looping: true, position: Some(contact),
        doppler: false, range: HISS_NEAR, lowpass_hz: 0.0, important: false,
    }
}

/// Each sound's clip; `None` where it is switched off.
struct Clips {
    hiss: Option<Arc<Clip>>,
    snap: Option<Arc<Clip>>,
    thud: Option<Arc<Clip>>,
    click: Option<Arc<Clip>>,
    frog: Option<Arc<Clip>>,
    clack: Option<Arc<Clip>>,
}

/// The sound triggers a vehicle's `sound.cfg` can play a sound of the gear on, the first one it
/// has. Named as OMSI 2's doors' (`ev_doortriggeropen_0`, `ev_doorhitclose_0`): pole k's own
/// (`_<k>`, from 1 as `Trolley_Pole<k>`), else any pole's.
pub fn triggers_of(s: RigSound) -> Vec<String> {
    let pole = |name: &str, pole: usize| vec![format!("ev_trolley{name}_{}", pole + 1), format!("ev_trolley{name}")];
    match s {
        RigSound::Click => vec!["ev_trolleycontactor".into()],
        RigSound::Snap(_) => vec!["ev_trolleyarc".into()],
        RigSound::Frog { pole: k } => pole("frog", k),
        RigSound::Insulator { pole: k } => pole("insulator", k),
        RigSound::Dewire { pole: k } => pole("dewire", k),
        RigSound::RopeStop { pole: k, .. } => pole("hitmax", k),
        RigSound::Release { pole: k, .. } => pole("hookopen", k),
        RigSound::OnWire { pole: k, .. } => pole("hitup", k),
        RigSound::ShoeHit { pole: k, .. } => pole("wirehit", k),
        RigSound::OffWire { pole: k, .. } => pole("wireoff", k),
        RigSound::Latch { pole: k, .. } => pole("hitdown", k),
    }
}

/// The clip a choice makes: the game's own, none, or the file (beside the file that named it);
/// a file that cannot be read leaves the game's own.
fn pick(audio: &AudioEngine, choice: &SoundChoice, dir: Option<&Path>, own: fn() -> Arc<Clip>) -> Option<Arc<Clip>> {
    match choice {
        SoundChoice::Builtin => Some(own()),
        SoundChoice::Off => None,
        SoundChoice::File(f) => {
            let path = dir.map_or_else(|| PathBuf::from(f), |d| omsi_cfg::resolve_path(d, f));
            audio.load_clip(&path).or_else(|| {
                log::warn!("trolley sound {}: not read, the game's own plays", path.display());
                Some(own())
            })
        }
    }
}

impl TrolleySound {
    /// One frame of the gear's sounds; `bus` is where the vehicle stands, `net` the wire it runs
    /// on (its `[trolley_sounds]` choose the frog's clip). A sound the vehicle's own
    /// `sound_set` has an entry for is fired into `fired` (its sound triggers) instead.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, audio: &AudioEngine, rig: &mut TrolleyRig, net: Option<&WireNet>, sound_set: Option<&SoundSet>, fired: &mut Vec<String>, bus: Vec3, sparks_at: &[Vec3], inside: bool, paused: bool) {
        let c = self.clips.get_or_insert_with(|| {
            Clips {
                hiss: Some(hiss()),
                snap: Some(snap()),
                frog: net.map_or_else(|| Some(frog()), |n| pick(audio, &n.sounds.frog, n.dir.as_deref(), frog)),
                thud: Some(thud()),
                click: Some(click()),
                clack: Some(clack()),
            }
        });
        let sounds = rig.take_sounds();
        let tip = rig.hiss_pos.as_vec3();
        let hinge = |k: usize| rig.hinge_at(k).map(|h| h.as_vec3());
        let (sum, n) = (0..rig.poles.len()).filter_map(hinge).fold((Vec3::ZERO, 0), |(s, n), h| (s + h, n + 1));
        let roof = if n == 0 { bus + Vec3::new(0.0, 0.0, 3.0) } else { sum / n as f32 };
        let shoe = |k: usize| rig.poles.get(k).map_or(tip, |p| p.tip.as_vec3());
        if paused {
            return;
        }
        // the hiss: a loop kept running, its level following the current and the speed
        let own_hiss = sound_set.is_some_and(|s| s.reads_var("Trolley_Hiss"));
        while self.hiss_voices.len() > rig.poles.len() {
            if let Some(id) = self.hiss_voices.pop().flatten() {
                audio.stop(id);
            }
        }
        self.hiss_voices.resize(rig.poles.len(), None);
        let listener = audio.listener_position();
        for (voice, pole) in self.hiss_voices.iter_mut().zip(&rig.poles) {
            let level = if c.hiss.is_some() && !own_hiss && pole.mode == PoleMode::OnWire && pole.closed { rig.hiss } else { 0.0 };
            let params = hiss_params(level, inside, pole.tip.as_vec3(), listener);
            if level <= 0.01 || params.gain <= 0.0 {
                if let Some(id) = voice.take() {
                    audio.stop(id);
                }
                continue;
            }
            match *voice {
                Some(id) if audio.is_playing(id) => audio.set_params(id, params),
                _ => *voice = Some(audio.play_from_random_place(c.hiss.clone().unwrap(), params)),
            }
        }
        let open = outside_vol();
        let one = |clip: &Option<Arc<Clip>>, gain: f32, at: Vec3, range: f32, heard: (f32, f32)| {
            let Some(clip) = clip else { return };
            audio.play(clip.clone(), VoiceParams { gain: gain * heard.0, pitch: 1.0, looping: false, position: Some(at), doppler: false, range, lowpass_hz: heard.1, important: true });
        };
        let mut sp = sparks_at.iter().copied();
        for s in sounds {
            // (an articulated bus's poles are on its rear section, whose own sound config plays them)
            let has = |set: &SoundSet, t: &str| set.has_trigger(t) || set.parts.iter().any(|(_, p)| p.has_trigger(t));
            if let Some(trigger) = triggers_of(s).into_iter().find(|t| sound_set.is_some_and(|set| has(set, t))) {
                // (an un-numbered hit of both poles in the same frame: one sound)
                if !fired.contains(&trigger) {
                    fired.push(trigger);
                }
                continue;
            }
            match s {
                RigSound::Click => one(&c.click, 0.5, roof - Vec3::Z * 1.5, 4.0, (1.0, 0.0)),
                RigSound::ShoeHit { pole, loud } => one(&c.thud, 0.3 + 0.5 * loud, shoe(pole), 6.0, cab(inside, open, ROOF_SHUT)),
                RigSound::OffWire { pole, .. } => one(&c.thud, 0.55, shoe(pole), 6.0, cab(inside, open, ROOF_SHUT)),
                RigSound::Snap(l) => one(&c.snap, 0.3 + 0.7 * l, sp.next().unwrap_or(tip), 10.0, cab(inside, open, ROOF_SHUT)),
                RigSound::Frog { pole } => one(&c.frog, 0.7, shoe(pole), 8.0, cab(inside, open, ROOF_SHUT)),
                // (a shoe over an insulator: only the vehicle's own sound, the arc has its own)
                RigSound::Insulator { .. } => {}
                RigSound::Dewire { pole } => one(&c.thud, 0.6, shoe(pole), 6.0, cab(inside, open, ROOF_SHUT)),
                RigSound::RopeStop { pole, loud } => one(&c.thud, 0.2 + 0.4 * loud, shoe(pole), 6.0, cab(inside, open, ROOF_SHUT)),
                RigSound::Release { pole, .. } => one(&c.clack, 0.5, hinge(pole).unwrap_or(roof), 6.0, cab(inside, open, ROOF_SHUT)),
                RigSound::Latch { pole, .. } => one(&c.clack, 0.5, hinge(pole).unwrap_or(roof), 6.0, cab(inside, open, ROOF_SHUT)),
                RigSound::OnWire { pole, .. } => one(&c.thud, 0.65, shoe(pole), 6.0, cab(inside, open, ROOF_SHUT)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pole_has_its_own_triggers_numbered_as_its_variables() {
        assert_eq!(triggers_of(RigSound::Latch { pole: 1 }), ["ev_trolleyhitdown_2", "ev_trolleyhitdown"]);
        assert_eq!(triggers_of(RigSound::OnWire { pole: 0 }), ["ev_trolleyhitup_1", "ev_trolleyhitup"]);
        assert_eq!(triggers_of(RigSound::Release { pole: 0 }), ["ev_trolleyhookopen_1", "ev_trolleyhookopen"]);
        assert_eq!(triggers_of(RigSound::Snap(1.0)), ["ev_trolleyarc"]);
        assert_eq!(triggers_of(RigSound::Click), ["ev_trolleycontactor"]);
    }

    #[test]
    fn hiss_is_local_to_contact_and_silent_in_the_cab() {
        let contact = Vec3::new(20.0, 30.0, 5.0);
        let near = hiss_params(1.0, false, contact, contact + Vec3::X);
        let middle = hiss_params(1.0, false, contact, contact + Vec3::X * 3.0);
        assert_eq!(near.position, Some(contact));
        assert_eq!(near.range, 1.0);
        assert!((near.gain - 0.035).abs() < 1e-6);
        assert!(middle.gain > 0.0 && middle.gain < near.gain);
        for distance in [5.0, 10.0, 100.0] {
            assert_eq!(hiss_params(1.0, false, contact, contact + Vec3::X * distance).gain, 0.0);
        }
        assert_eq!(hiss_params(1.0, true, contact, contact).gain, 0.0);
        assert_eq!(hiss_params(0.0, false, contact, contact).gain, 0.0);
        assert_eq!(hiss_params(1.0, false, contact, Vec3::splat(f32::NAN)).gain, 0.0);
    }

    #[test]
    fn the_cab_halves_and_dulls_roof_sounds_until_it_is_opened() {
        assert_eq!(cab(false, 0.0, ROOF_SHUT), (1.0, 0.0));
        let (gain, lowpass) = cab(true, 0.0, ROOF_SHUT);
        assert!((gain - 0.5).abs() < 1e-6 && lowpass > 2000.0 && lowpass < 3000.0, "{gain} {lowpass}");
        let (half_open, half_lowpass) = cab(true, 0.5, ROOF_SHUT);
        assert!(half_open > gain && half_lowpass > lowpass);
        assert_eq!(cab(true, 1.0, ROOF_SHUT), (1.0, 0.0));
        assert_eq!(cab(true, f32::NAN, ROOF_SHUT), cab(true, 0.0, ROOF_SHUT));
        let (switch, switch_lowpass) = cab(true, 0.0, SWITCH_SHUT);
        assert!((switch - 0.2).abs() < 1e-6 && switch_lowpass > 700.0 && switch_lowpass < 1200.0, "{switch} {switch_lowpass}");
        assert_eq!(cab(true, 1.0, SWITCH_SHUT), (1.0, 0.0));
    }

    #[test]
    fn hiss_voices_follow_individual_contacts_and_stop_out_of_range() {
        let audio = AudioEngine::capture(44100);
        let mut sound = TrolleySound::default();
        let definition = omsi_trolley::TrolleyDef {
            poles: vec![omsi_trolley::PoleDef::default(); 2], ..Default::default()
        };
        let mut rig = TrolleyRig::new(&definition, 1);
        for (index, pole) in rig.poles.iter_mut().enumerate() {
            pole.mode = PoleMode::OnWire;
            pole.closed = true;
            pole.tip = glam::DVec3::new(index as f64 * 10.0, 0.0, 5.0);
        }
        rig.hiss = 1.0;
        audio.set_listener(crate::Listener { position: Vec3::new(0.0, 0.0, 5.0), ..Default::default() });
        sound.update(&audio, &mut rig, None, None, &mut Vec::new(), Vec3::ZERO, &[], false, false);
        assert!(sound.hiss_voices[0].is_some());
        assert!(sound.hiss_voices[1].is_none());
        audio.render_capture(64);
        let (params, _) = audio.voice_state(sound.hiss_voices[0].unwrap()).unwrap();
        assert_eq!(params.position, Some(Vec3::new(0.0, 0.0, 5.0)));
        audio.set_listener(crate::Listener { position: Vec3::new(50.0, 0.0, 5.0), ..Default::default() });
        sound.update(&audio, &mut rig, None, None, &mut Vec::new(), Vec3::ZERO, &[], false, false);
        assert!(sound.hiss_voices.iter().all(Option::is_none));
    }

    fn network(sound: &str) -> Arc<WireNet> {
        Arc::new(WireNet::parse(&format!(
            "[trolley_span]\np 0 0 5\np 50 0 5\n[trolley_span]\np 50 0 5\np 100 0 5\n\
             [trolley_span]\np 50 0 5\np 90 20 5\n[trolley_switch]\npos 50 0 5\nmode radio\n\
             [trolley_sounds]\nswitch {sound}\n"
        )))
    }

    #[test]
    fn switch_audio_plays_each_movement_once_and_respects_off() {
        let audio = AudioEngine::capture(44100);
        let mut sound = SwitchSound::default();
        let net = network("on");
        assert_eq!(sound.update(&audio, &net, false, false), 0);
        net.set_switch(0, true);
        assert_eq!(sound.update(&audio, &net, false, false), 1);
        assert_eq!(sound.update(&audio, &net, false, false), 0);
        net.set_switch(0, false);
        assert_eq!(sound.update(&audio, &net, false, true), 0);
        assert_eq!(sound.update(&audio, &net, false, false), 0);
        let off = network("off");
        off.set_switch(0, true);
        assert_eq!(sound.update(&audio, &off, false, false), 0);
        assert!(off.take_switch_sounds().is_empty());
    }
}
