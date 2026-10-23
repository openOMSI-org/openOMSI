//! The game's side of voice chat: the talk keys into the voice engine (`omsi_voice`), its
//! packets onto the session (`omsi_net::voice_net`, which the host relays), and what is
//! heard placed in the world - a nearby voice where the speaker stands, quieter with the
//! distance and muffled through a bus's body, a radio voice everywhere at once.
//!
//! Nothing is recorded or stored. The microphone is opened by the engine only while a talk
//! key is held (or, in the open mode, while voice chat is on) and is closed shortly after.

use crate::settings::Settings;
use glam::DVec3;
use omsi_audio::{AudioEngine, VoiceId, VoiceParams};
use omsi_lan::game::LanGame;
use omsi_net::{LanSession, VoicePolicy};
use omsi_voice::mutes::MuteList;
use omsi_voice::{Channel, Frame, Mode, Status, Voice};
use std::collections::{HashMap, HashSet};

/// Seconds between asking the host what it allows, until it answers.
const ASK_EVERY: f32 = 3.0;
/// Heard through a bus's body (one of the two sits in a bus, the other outside it or in
/// another bus): this share of the loudness, and no highs.
const MUFFLED_GAIN: f32 = 0.45;
const MUFFLED_HZ: f32 = 900.0;
/// Where a speaker's voice comes from, above their feet (m), on foot.
const MOUTH: f64 = 1.6;

/// Where a player is, as the voice has it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Place {
    pub at: DVec3,
    /// The bus they are in (its driver's player id), if any.
    pub inside: Option<u32>,
}

/// The volumes of the settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Volumes {
    pub master: f32,
    pub nearby: f32,
    pub radio: f32,
}

impl Volumes {
    pub fn of(s: &Settings) -> Volumes {
        Volumes { master: s.voice_volume, nearby: s.voice_volume_nearby, radio: s.voice_volume_radio }
    }
}

/// Loudness and low-pass (Hz, 0: none) of a voice: `dist` metres away on the nearby channel
/// (the radio is the same everywhere), `muffled` when there is a bus's body between.
pub(crate) fn voice_sound(channel: Channel, dist: f32, range: f32, v: Volumes, muffled: bool) -> (f32, f32) {
    match channel {
        Channel::Radio => (v.master * v.radio, 0.0),
        Channel::Nearby => {
            let g = omsi_voice::dsp::proximity_gain(dist, range) * v.master * v.nearby;
            if muffled {
                (g * MUFFLED_GAIN, MUFFLED_HZ)
            } else {
                (g, 0.0)
            }
        }
    }
}

/// Where player `id` is: on foot where they walk, aboard a bus in it, driving at the wheel.
pub(crate) fn place_of(lan: &LanSession, game: &LanGame, my_bus: Option<DVec3>, id: u32) -> Option<Place> {
    let bus_of = |id: u32| -> Option<DVec3> {
        if id == lan.my_id {
            return my_bus;
        }
        game.remotes.get(&id).map(|r| r.vehicle().position).or_else(|| lan.peers().find(|p| p.pose.id == id).map(|p| DVec3::new(p.pose.x, p.pose.y, p.pose.z)))
    };
    let p = lan.peers().find(|p| p.pose.id == id)?;
    let pose = &p.pose;
    match pose.walker {
        Some(w) => match w.aboard {
            Some(a) => Some(Place { at: bus_of(a.owner).map(|b| b + DVec3::new(0.0, 0.0, 1.9)).unwrap_or(DVec3::new(w.x, w.y, w.z + MOUTH)), inside: Some(a.owner) }),
            None => Some(Place { at: DVec3::new(w.x, w.y, w.z + MOUTH), inside: None }),
        },
        None if pose.has_vehicle() => Some(Place { at: bus_of(pose.id).unwrap_or(DVec3::new(pose.x, pose.y, pose.z)) + DVec3::new(0.0, 0.0, 1.9), inside: Some(pose.id) }),
        None => None,
    }
}

/// The mute list beside the settings (`voice_mutes.txt`).
pub(crate) fn load_mutes() -> MuteList {
    match Settings::path().and_then(|p| p.parent().map(|d| d.join("voice_mutes.txt"))) {
        Some(p) => MuteList::load(p),
        None => MuteList::default(),
    }
}

pub(crate) struct VoiceChat {
    engine: Voice,
    /// What the session allows: the host's own, or what it told us.
    pub policy: VoicePolicy,
    policy_known: bool,
    /// The host knows we take voice (it answered our `voice?`): until then it is asked again.
    listening: bool,
    asked: bool,
    ask_in: f32,
    voices: HashMap<u32, VoiceId>,
    cue: Option<VoiceId>,
    /// An administrator silenced us.
    pub server_muted: bool,
    pub status: Status,
    /// Who is heard now: (player id, name, channel).
    pub talking: Vec<(u32, String, Channel)>,
}

impl VoiceChat {
    pub fn new(first_time: bool) -> VoiceChat {
        let engine = Voice::new();
        if first_time {
            // (the system asks for the microphone now, not on the first press of a key)
            engine.probe_microphone();
        }
        VoiceChat {
            engine,
            policy: VoicePolicy::default(),
            policy_known: false,
            listening: false,
            asked: false,
            ask_in: 0.0,
            voices: HashMap::new(),
            cue: None,
            server_muted: false,
            status: Status::default(),
            talking: Vec::new(),
        }
    }

    /// The host told what the session allows.
    pub fn set_policy(&mut self, p: VoicePolicy) {
        self.policy = p;
        self.policy_known = true;
        self.listening |= self.asked;
    }

    /// Whether the session carries voice chat on `channel`.
    pub fn allows(&self, channel: Channel) -> bool {
        self.policy.allows(channel.id())
    }

    pub fn transmitting(&self) -> Option<Channel> {
        self.status.transmitting
    }

    /// Whether `id` is heard now.
    pub fn speaks(&self, id: u32) -> Option<Channel> {
        self.talking.iter().find(|t| t.0 == id).map(|t| t.2)
    }

    /// Stop everything the voice chat plays (it is over: the session ended or it was switched
    /// off).
    pub fn stop(&mut self, audio: Option<&AudioEngine>) {
        if let Some(a) = audio {
            for (_, v) in self.voices.drain() {
                a.stop(v);
            }
            if let Some(c) = self.cue.take() {
                a.stop(c);
            }
        }
    }

    /// A line for the screen: what is wrong, or that the key is down.
    pub fn notices(&self) -> Vec<String> {
        let tr = |s: &str| omsi_ui::tr(s).into_owned();
        let mut v = Vec::new();
        if let Some(e) = &self.status.error {
            v.push(tr("Voice: the microphone cannot be used ({e})").replace("{e}", &tr(e)));
        }
        if self.server_muted {
            v.push(tr("Voice: an admin has silenced your voice chat"));
        }
        if !self.policy.enabled && self.policy_known {
            v.push(tr("Voice: this session has no voice chat"));
        }
        v
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        dt: f32,
        s: &Settings,
        lan: &mut LanSession,
        game: &LanGame,
        audio: Option<&AudioEngine>,
        listener: Option<DVec3>,
        my_bus: Option<DVec3>,
        my_inside: Option<u32>,
        held: (bool, bool),
        mutes: &MuteList,
    ) {
        let host = lan.role == omsi_net::Role::Host;
        if host {
            // (the host's choice; the range is its own setting)
            let mut p = lan.voice_policy();
            if (p.range - s.voice_range).abs() > 0.01 {
                p = p.with_range(s.voice_range);
                lan.set_voice_policy(p);
            }
            self.set_policy(p);
        } else if !self.listening && s.voice_enabled {
            self.ask_in -= dt;
            if self.ask_in <= 0.0 && lan.connected {
                self.ask_in = ASK_EVERY;
                self.asked = true;
                lan.command(1, "voice?");
            }
        }
        self.engine.set_settings(&omsi_voice::Settings {
            enabled: s.voice_enabled && lan.connected,
            // (an open microphone sends nothing where an administrator silenced us or the
            // session has no voice on the nearby channel)
            mode: if s.voice_mode == "open" && !self.server_muted && self.allows(Channel::Nearby) { Mode::Open } else { Mode::PushToTalk },
            open_channel: Channel::Nearby,
            mic_gain: s.voice_mic_gain,
            gate_db: s.voice_threshold_db,
            input_device: s.voice_input_device.clone(),
        });
        // the talk keys (radio wins when both are down)
        let want = if held.1 && self.allows(Channel::Radio) {
            Some(Channel::Radio)
        } else if held.0 && self.allows(Channel::Nearby) {
            Some(Channel::Nearby)
        } else {
            None
        };
        self.engine.set_transmit(want.filter(|_| !self.server_muted));
        // who is not to be heard
        let muted: HashSet<u32> = lan.peers().filter(|p| mutes.is_muted(&p.pose.name)).map(|p| p.pose.id).collect();
        self.engine.set_muted(muted, s.voice_all_off);
        // packets out and in
        for o in self.engine.take_outgoing() {
            lan.voice_send(o.channel.id(), o.end, o.seq, &o.payload);
        }
        for v in lan.take_voice() {
            if let Some(channel) = Channel::from_id(v.channel) {
                self.engine.feed(Frame { from: v.from, channel, seq: v.seq, end: v.end, payload: v.payload });
            }
        }
        self.status = self.engine.status();
        // the buffers the engine made, played through the game's audio
        for n in self.engine.take_new_streams() {
            let Some(a) = audio else { continue };
            if n.id == 0 {
                self.cue = Some(a.play_stream(n.stream, VoiceParams { gain: 0.0, doppler: false, ..Default::default() }));
            } else {
                if let Some(old) = self.voices.insert(n.id, a.play_stream(n.stream, VoiceParams { gain: 0.0, doppler: false, ..Default::default() })) {
                    a.stop(old);
                }
            }
        }
        // each speaker's loudness and place
        let vols = Volumes::of(s);
        let heard: HashMap<u32, Channel> = self.status.talkers.iter().map(|t| (t.id, t.channel)).collect();
        self.talking = self
            .status
            .talkers
            .iter()
            .map(|t| (t.id, lan.peers().find(|p| p.pose.id == t.id).map(|p| p.pose.name.clone()).unwrap_or_default(), t.channel))
            .collect();
        if let Some(a) = audio {
            self.voices.retain(|_, v| a.is_playing(*v));
            for (id, v) in &self.voices {
                let Some(&channel) = heard.get(id) else {
                    a.set_params(*v, VoiceParams { gain: 0.0, doppler: false, ..Default::default() });
                    continue;
                };
                let place = place_of(lan, game, my_bus, *id);
                let params = match (channel, place, listener) {
                    (Channel::Nearby, Some(p), Some(ear)) => {
                        let dist = (p.at - ear).length() as f32;
                        let (gain, lowpass_hz) = voice_sound(channel, dist, self.policy.range, vols, p.inside != my_inside);
                        // (the speaker's own place: panning follows it; the loudness is ours)
                        VoiceParams { gain, position: Some(p.at.as_vec3()), range: 1.0e9, doppler: false, lowpass_hz, ..Default::default() }
                    }
                    (Channel::Nearby, _, _) => VoiceParams { gain: 0.0, doppler: false, ..Default::default() },
                    (Channel::Radio, _, _) => {
                        let (gain, _) = voice_sound(channel, 0.0, self.policy.range, vols, false);
                        VoiceParams { gain, doppler: false, ..Default::default() }
                    }
                };
                a.set_params(*v, params);
            }
            if let Some(c) = self.cue {
                let (gain, _) = voice_sound(Channel::Radio, 0.0, self.policy.range, vols, false);
                a.set_params(c, VoiceParams { gain, doppler: false, ..Default::default() });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vols() -> Volumes {
        Volumes { master: 1.0, nearby: 1.0, radio: 1.0 }
    }

    #[test]
    fn a_nearby_voice_fades_with_distance_and_is_muffled_through_a_body() {
        let (near, _) = voice_sound(Channel::Nearby, 3.0, 75.0, vols(), false);
        let (far, _) = voice_sound(Channel::Nearby, 50.0, 75.0, vols(), false);
        let (out, _) = voice_sound(Channel::Nearby, 80.0, 75.0, vols(), false);
        assert!(near == 1.0 && far > 0.0 && far < near && out == 0.0);
        let (gain, hz) = voice_sound(Channel::Nearby, 3.0, 75.0, vols(), true);
        assert!(gain < near && hz > 0.0 && hz < 2000.0);
        // the volumes of the settings
        let (quiet, _) = voice_sound(Channel::Nearby, 3.0, 75.0, Volumes { master: 0.5, nearby: 0.5, radio: 1.0 }, false);
        assert_eq!(quiet, 0.25);
    }

    #[test]
    fn the_radio_is_as_loud_everywhere_and_never_muffled() {
        for d in [0.0, 100.0, 5000.0] {
            assert_eq!(voice_sound(Channel::Radio, d, 75.0, Volumes { master: 0.8, nearby: 0.1, radio: 0.5 }, true), (0.4, 0.0));
        }
    }
}
