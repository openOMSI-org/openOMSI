//! The voice chat's engine: what goes from the microphone to the network (gain, gate, Opus)
//! and from the network to the speakers (jitter buffer, Opus, radio or distance shaping).
//!
//! `Core` is the engine without threads or devices: frames and samples go in, packets and
//! sound come out, with the time given. `Voice` runs a `Core` on its own thread and is what
//! the game talks to; it opens the microphone only while it is needed and never records.

use crate::capture::Capture;
use crate::codec::{Decoder, Encoder, FRAME, FRAME_SECS, RATE};
use crate::dsp::{self, Gate, RadioVoice};
use crate::jitter::{Jitter, Playout};
use omsi_audio::stream::StreamBuf;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Players heard at once, at most (each has a decoder and a buffer).
pub const MAX_SPEAKERS: usize = 32;
/// Frames waiting for the voice thread (a game that stalls drops the oldest).
const INBOX: usize = 512;
/// A speaker silent this long is forgotten (their buffer closed).
const IDLE_FORGET: Duration = Duration::from_secs(20);
/// The microphone stays open this long after the key is let go, so the next press does not
/// wait for the device to open again. (Nothing is sent or kept meanwhile.)
const MIC_GRACE: Duration = Duration::from_millis(1500);
/// A speaker's buffer is not fed beyond this much (s) (an audio device that stopped); the ring
/// (`voice_buffer`) holds more than this plus one frame, so a push never waits.
const MAX_BUFFERED: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Heard by those near the speaker, quieter with the distance.
    Nearby,
    /// Heard by the whole session, with a radio's sound.
    Radio,
}

impl Channel {
    pub fn id(self) -> u8 {
        match self {
            Channel::Nearby => 0,
            Channel::Radio => 1,
        }
    }

    pub fn from_id(id: u8) -> Option<Channel> {
        match id {
            0 => Some(Channel::Nearby),
            1 => Some(Channel::Radio),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The microphone is open (and sent) only while a talk key is held.
    PushToTalk,
    /// The microphone is open all the time while voice chat is on; what is loud enough is sent
    /// on `Settings::open_channel`. A talk key sends regardless of the level.
    Open,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub mode: Mode,
    pub open_channel: Channel,
    /// Multiplier on the microphone (1 = as it is).
    pub mic_gain: f32,
    /// The open microphone's gate (dB full scale).
    pub gate_db: f32,
    /// The microphone to use (empty: the system's).
    pub input_device: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: false,
            mode: Mode::PushToTalk,
            open_channel: Channel::Nearby,
            mic_gain: 1.0,
            gate_db: -45.0,
            input_device: String::new(),
        }
    }
}

/// A packet to send.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub channel: Channel,
    pub end: bool,
    pub seq: u16,
    pub payload: Vec<u8>,
}

/// A packet that came in.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub from: u32,
    pub channel: Channel,
    pub seq: u16,
    pub end: bool,
    pub payload: Vec<u8>,
}

/// The buffer a new speaker is heard from, for the game to play through its audio engine.
pub struct NewStream {
    /// The speaker's player id; 0 is our own radio's clicks.
    pub id: u32,
    pub stream: Arc<StreamBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Talker {
    pub id: u32,
    pub channel: Channel,
    /// Level of what is heard (dB full scale).
    pub level_db: f32,
}

struct Speaker {
    dec: Decoder,
    jit: Jitter,
    stream: Arc<StreamBuf>,
    radio: RadioVoice,
    channel: Channel,
    active: bool,
    ending: bool,
    due: Instant,
    last_rx: Instant,
    level_db: f32,
}

pub struct Core {
    pub settings: Settings,
    enc: Encoder,
    seq: u16,
    pcm: Vec<f32>,
    gate: Gate,
    sending: Option<Channel>,
    pub mic_level_db: f32,
    speakers: HashMap<u32, Speaker>,
    pub muted: HashSet<u32>,
    pub all_off: bool,
    pub out: Vec<Outgoing>,
    pub new_streams: Vec<NewStream>,
    cue: Arc<StreamBuf>,
}

fn voice_buffer() -> Arc<StreamBuf> {
    let b = StreamBuf::with_buffering(1 << 14, 0.03, 0.02, 0.12);
    // (the rate is set once, so the mixer's resampling starts right)
    b.push(RATE, std::iter::empty());
    Arc::new(b)
}

fn feed(stream: &StreamBuf, samples: &[f32]) {
    if stream.buffered() < MAX_BUFFERED {
        stream.push(RATE, samples.iter().map(|s| [*s, *s]));
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        // (the streams end, and the game's voices that play them with them)
        self.cue.close();
        for (_, sp) in self.speakers.drain() {
            sp.stream.close();
        }
    }
}

impl Core {
    pub fn new() -> Option<Core> {
        let cue = voice_buffer();
        Some(Core {
            settings: Settings::default(),
            enc: Encoder::new()?,
            seq: 0,
            pcm: Vec::new(),
            gate: Gate::new(Settings::default().gate_db),
            sending: None,
            mic_level_db: -120.0,
            speakers: HashMap::new(),
            muted: HashSet::new(),
            all_off: false,
            out: Vec::new(),
            new_streams: vec![NewStream {
                id: 0,
                stream: cue.clone(),
            }],
            cue,
        })
    }

    pub fn apply(&mut self, s: &Settings) {
        if self.settings.gate_db != s.gate_db {
            self.gate.set_threshold(s.gate_db);
        }
        self.settings = s.clone();
    }

    pub fn sending(&self) -> Option<Channel> {
        self.sending
    }

    /// Whether the microphone is wanted at all (a key held, or the open microphone).
    pub fn wants_mic(&self, transmit: Option<Channel>) -> bool {
        self.settings.enabled && (transmit.is_some() || self.settings.mode == Mode::Open)
    }

    fn end_transmission(&mut self) {
        if let Some(ch) = self.sending.take() {
            self.out.push(Outgoing {
                channel: ch,
                end: true,
                seq: self.seq,
                payload: Vec::new(),
            });
            self.seq = self.seq.wrapping_add(1);
            if ch == Channel::Radio {
                feed(&self.cue, &dsp::squelch(false));
            }
        }
    }

    /// Microphone samples (mono, 48 kHz) while `transmit` says which key is held. Packets
    /// for every full 20 ms are made.
    pub fn mic(&mut self, samples: &[f32], transmit: Option<Channel>) {
        if !self.settings.enabled {
            self.pcm.clear();
            self.end_transmission();
            return;
        }
        self.pcm.extend_from_slice(samples);
        // (what has piled up beyond a quarter second is stale)
        if self.pcm.len() > RATE as usize / 4 {
            let cut = self.pcm.len() - RATE as usize / 4;
            self.pcm.drain(..cut);
        }
        while self.pcm.len() >= FRAME {
            let mut frame: Vec<f32> = self.pcm.drain(..FRAME).collect();
            dsp::mic_gain(&mut frame, self.settings.mic_gain);
            let level = dsp::rms_db(&frame);
            self.mic_level_db = if level > self.mic_level_db {
                level
            } else {
                self.mic_level_db * 0.9 + level * 0.1
            };
            let channel = match (transmit, self.settings.mode) {
                (Some(c), _) => Some(c),
                (None, Mode::Open) if self.gate.process(&frame) => Some(self.settings.open_channel),
                _ => None,
            };
            match channel {
                Some(ch) => {
                    // (changing channels mid-word ends the first transmission)
                    if self.sending.is_some_and(|s| s != ch) {
                        self.end_transmission();
                    }
                    if let Some(payload) = self.enc.encode(&frame) {
                        if self.sending.is_none() && ch == Channel::Radio {
                            feed(&self.cue, &dsp::squelch(true));
                        }
                        self.sending = Some(ch);
                        self.out.push(Outgoing {
                            channel: ch,
                            end: false,
                            seq: self.seq,
                            payload,
                        });
                        self.seq = self.seq.wrapping_add(1);
                    }
                }
                None => self.end_transmission(),
            }
        }
        if transmit.is_none() && self.settings.mode == Mode::PushToTalk {
            self.pcm.clear();
            self.end_transmission();
        }
    }

    /// A packet from the network.
    pub fn receive(&mut self, now: Instant, f: Frame) {
        if self.all_off || self.muted.contains(&f.from) || !self.settings.enabled || f.from == 0 {
            return;
        }
        if !self.speakers.contains_key(&f.from) {
            if self.speakers.len() >= MAX_SPEAKERS || (f.end && f.payload.is_empty()) {
                return;
            }
            let (Some(dec), stream) = (Decoder::new(), voice_buffer()) else {
                return;
            };
            self.new_streams.push(NewStream {
                id: f.from,
                stream: stream.clone(),
            });
            self.speakers.insert(
                f.from,
                Speaker {
                    dec,
                    jit: Jitter::new(),
                    stream,
                    radio: RadioVoice::default(),
                    channel: f.channel,
                    active: false,
                    ending: false,
                    due: now,
                    last_rx: now,
                    level_db: -120.0,
                },
            );
        }
        let sp = self.speakers.get_mut(&f.from).unwrap();
        sp.last_rx = now;
        if !sp.active && !sp.ending {
            sp.channel = f.channel;
        }
        if !f.payload.is_empty() {
            sp.channel = f.channel;
            sp.ending = false;
            sp.jit.push(f.seq, f.payload);
        }
        if f.end {
            sp.ending = true;
        }
    }

    /// Decode what is due and hand it to the speakers' buffers (call every few ms).
    pub fn play(&mut self, now: Instant) {
        let step = Duration::from_secs_f32(FRAME_SECS);
        let (muted, all_off) = (self.muted.clone(), self.all_off);
        let mut gone = Vec::new();
        for (id, sp) in self.speakers.iter_mut() {
            if all_off || muted.contains(id) || !self.settings.enabled {
                gone.push(*id);
                continue;
            }
            if sp.active && sp.ending && sp.jit.is_empty() {
                Self::finish(sp);
                continue;
            }
            if !sp.active {
                sp.due = now;
            } else if now.saturating_duration_since(sp.due) > Duration::from_millis(100) {
                sp.due = now;
            }
            while now >= sp.due {
                let samples = match sp.jit.pop() {
                    Playout::Wait => break,
                    Playout::Frame(p) => sp.dec.decode(&p).unwrap_or_else(|| sp.dec.conceal()),
                    Playout::Recover(next) => {
                        sp.dec.recover(&next).unwrap_or_else(|| sp.dec.conceal())
                    }
                    Playout::Conceal => sp.dec.conceal(),
                    Playout::Dead => {
                        Self::finish(sp);
                        break;
                    }
                };
                if !sp.active {
                    sp.active = true;
                    sp.due = now;
                    if sp.channel == Channel::Radio {
                        feed(&sp.stream, &dsp::squelch(true));
                    }
                }
                let mut samples = samples.to_vec();
                if sp.channel == Channel::Radio {
                    sp.radio.process(&mut samples);
                }
                let level = dsp::rms_db(&samples);
                sp.level_db = if level > sp.level_db {
                    level
                } else {
                    sp.level_db * 0.8 + level * 0.2
                };
                feed(&sp.stream, &samples);
                sp.due += step;
                if sp.ending && sp.jit.is_empty() {
                    Self::finish(sp);
                    break;
                }
            }
        }
        for id in gone {
            if let Some(sp) = self.speakers.remove(&id) {
                sp.stream.close();
            }
        }
        // the forgotten
        let idle: Vec<u32> = self
            .speakers
            .iter()
            .filter(|(_, s)| !s.active && now.saturating_duration_since(s.last_rx) > IDLE_FORGET)
            .map(|(i, _)| *i)
            .collect();
        for id in idle {
            if let Some(sp) = self.speakers.remove(&id) {
                sp.stream.close();
            }
        }
    }

    fn finish(sp: &mut Speaker) {
        if sp.channel == Channel::Radio {
            feed(&sp.stream, &dsp::squelch(false));
        }
        sp.active = false;
        sp.ending = false;
        sp.jit.reset();
        sp.level_db = -120.0;
    }

    /// Who is heard now.
    pub fn talkers(&self) -> Vec<Talker> {
        let mut v: Vec<Talker> = self
            .speakers
            .iter()
            .filter(|(_, s)| s.active)
            .map(|(id, s)| Talker {
                id: *id,
                channel: s.channel,
                level_db: s.level_db,
            })
            .collect();
        v.sort_by_key(|t| t.id);
        v
    }

    pub fn speaker_count(&self) -> usize {
        self.speakers.len()
    }

    /// Close every buffer (voice chat is over).
    pub fn close_all(&mut self) {
        for (_, sp) in self.speakers.drain() {
            sp.stream.close();
        }
        self.end_transmission();
        self.pcm.clear();
    }
}

/// What the game can see of the engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub mic_open: bool,
    /// Why the microphone could not be used (none: no problem).
    pub error: Option<String>,
    pub mic_level_db: f32,
    pub transmitting: Option<Channel>,
    pub talkers: Vec<Talker>,
    pub device: String,
}

#[derive(Default)]
struct Inbox {
    settings: Settings,
    transmit: Option<Channel>,
    muted: HashSet<u32>,
    all_off: bool,
    probe: bool,
    incoming: VecDeque<Frame>,
    outgoing: Vec<Outgoing>,
    new_streams: Vec<NewStream>,
    status: Status,
    /// Frames the engine took in / sent so far (for the logs and tests).
    received: u64,
    sent: u64,
}

struct Shared {
    inbox: Mutex<Inbox>,
    stop: AtomicBool,
}

/// The engine on its own thread.
pub struct Voice {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

enum Mic {
    Device(Capture),
    /// A synthetic source for tests (`OMSI_VOICE_TEST_TONE`): a tone at this pitch, made at the
    /// pace of real time.
    Tone {
        freq: f32,
        made: u64,
        started: Instant,
    },
}

/// `OMSI_VOICE_TEST_TONE=1` (or a pitch in Hz): the microphone is a tone, the device is never
/// opened. For tests of the whole chain without a person.
pub fn test_tone() -> Option<f32> {
    let v = omsi_cfg::flags::OMSI_VOICE_TEST_TONE.live_var().ok()?;
    let v = v.trim();
    match v.parse::<f32>() {
        Ok(f) if f >= 50.0 => Some(f.min(4000.0)),
        Ok(f) if f > 0.0 => Some(440.0),
        _ => None,
    }
}

impl Voice {
    pub fn new() -> Voice {
        let shared = Arc::new(Shared {
            inbox: Mutex::new(Inbox::default()),
            stop: AtomicBool::new(false),
        });
        let s = shared.clone();
        let thread = std::thread::Builder::new()
            .name("voice".into())
            .spawn(move || worker(s))
            .ok();
        Voice { shared, thread }
    }

    pub fn set_settings(&self, s: &Settings) {
        let mut i = self.shared.inbox.lock();
        if i.settings != *s {
            i.settings = s.clone();
        }
    }

    /// Which talk key is held (none: neither).
    pub fn set_transmit(&self, c: Option<Channel>) {
        self.shared.inbox.lock().transmit = c;
    }

    pub fn set_muted(&self, muted: HashSet<u32>, all_off: bool) {
        let mut i = self.shared.inbox.lock();
        i.muted = muted;
        i.all_off = all_off;
    }

    /// Open the microphone for a moment so the system asks for permission now (when voice
    /// chat is first switched on) rather than on the first press of the key.
    pub fn probe_microphone(&self) {
        self.shared.inbox.lock().probe = true;
    }

    pub fn feed(&self, f: Frame) {
        let mut i = self.shared.inbox.lock();
        if i.incoming.len() >= INBOX {
            i.incoming.pop_front();
        }
        i.incoming.push_back(f);
    }

    pub fn take_outgoing(&self) -> Vec<Outgoing> {
        std::mem::take(&mut self.shared.inbox.lock().outgoing)
    }

    pub fn take_new_streams(&self) -> Vec<NewStream> {
        std::mem::take(&mut self.shared.inbox.lock().new_streams)
    }

    pub fn status(&self) -> Status {
        self.shared.inbox.lock().status.clone()
    }

    /// Frames received and sent so far.
    pub fn counters(&self) -> (u64, u64) {
        let i = self.shared.inbox.lock();
        (i.received, i.sent)
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn worker(shared: Arc<Shared>) {
    let Some(mut core) = Core::new() else {
        log::warn!("voice: the Opus encoder could not be made: no voice chat");
        return;
    };
    let tone = test_tone();
    let mut mic: Option<Mic> = None;
    let mut mic_error: Option<String> = None;
    let mut mic_wanted_until = Instant::now();
    let mut mic_device = String::new();
    // (the device the settings asked for when it was opened: a card that is gone gives the
    // default one, whose own name is not the setting - that is no reason to open it again)
    let mut mic_asked = String::new();
    let mut probe_until: Option<Instant> = None;
    let mut buf: Vec<f32> = Vec::new();
    let mut last_error_try = Instant::now() - Duration::from_secs(10);
    while !shared.stop.load(Ordering::Acquire) {
        let now = Instant::now();
        // what the game says
        let (settings, transmit, frames, probe) = {
            let mut i = shared.inbox.lock();
            core.muted = i.muted.clone();
            core.all_off = i.all_off;
            let frames: Vec<Frame> = i.incoming.drain(..).collect();
            i.received += frames.len() as u64;
            (
                i.settings.clone(),
                i.transmit,
                frames,
                std::mem::take(&mut i.probe),
            )
        };
        core.apply(&settings);
        if probe {
            probe_until = Some(now + Duration::from_millis(600));
        }
        let probing = probe_until.is_some_and(|t| now < t);
        if !settings.enabled && !probing {
            if mic.is_some() {
                mic = None;
            }
            mic_error = None;
            mic_wanted_until = now;
            core.close_all();
            std::thread::sleep(Duration::from_millis(50));
            shared.inbox.lock().status = Status::default();
            continue;
        }
        // the microphone: open while wanted (a key held, the open mode) and a moment after
        let wanted = core.wants_mic(transmit);
        if wanted {
            mic_wanted_until = now + MIC_GRACE;
        }
        let keep = (settings.enabled && (wanted || now < mic_wanted_until)) || probing;
        if keep && mic.is_none() && now.duration_since(last_error_try) > Duration::from_secs(3) {
            match tone {
                Some(freq) => {
                    mic = Some(Mic::Tone {
                        freq,
                        made: 0,
                        started: now,
                    })
                }
                None => match Capture::open(&settings.input_device) {
                    Ok(c) => {
                        mic_device = c.device.clone();
                        mic_asked = settings.input_device.clone();
                        mic_error = None;
                        mic = Some(Mic::Device(c));
                    }
                    Err(e) => {
                        log::warn!("voice: {e}");
                        mic_error = Some(e);
                        last_error_try = now;
                    }
                },
            }
        } else if !keep && mic.is_some() {
            mic = None;
            log::info!("voice: microphone closed");
        }
        // an input device changed in the settings
        if let (Some(Mic::Device(_)), true) = (
            &mic,
            tone.is_none()
                && !settings.input_device.is_empty()
                && settings.input_device != mic_asked,
        ) {
            mic = None;
        }
        buf.clear();
        match &mut mic {
            Some(Mic::Device(c)) => c.take(&mut buf),
            Some(Mic::Tone {
                freq,
                made,
                started,
            }) => {
                let due = (started.elapsed().as_secs_f64() * RATE as f64) as u64;
                while *made < due {
                    let n = (due - *made).min(FRAME as u64);
                    buf.extend((0..n).map(|k| {
                        0.3 * (2.0
                            * std::f32::consts::PI
                            * *freq
                            * ((*made + k) as f32 / RATE as f32))
                            .sin()
                    }));
                    *made += n;
                }
            }
            None => {}
        }
        core.mic(&buf, transmit);
        for f in frames {
            core.receive(now, f);
        }
        core.play(now);
        {
            let mut i = shared.inbox.lock();
            i.sent += core.out.iter().filter(|o| !o.end).count() as u64;
            i.outgoing.append(&mut core.out);
            i.new_streams.append(&mut core.new_streams);
            if i.outgoing.len() > 512 {
                let cut = i.outgoing.len() - 512;
                i.outgoing.drain(..cut);
            }
            i.status = Status {
                mic_open: mic.is_some(),
                error: mic_error.clone(),
                mic_level_db: if mic.is_some() {
                    core.mic_level_db
                } else {
                    -120.0
                },
                transmitting: core.sending(),
                talkers: core.talkers(),
                device: if matches!(mic, Some(Mic::Tone { .. })) {
                    "test tone".into()
                } else {
                    mic_device.clone()
                },
            };
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

#[cfg(test)]
mod tests;
