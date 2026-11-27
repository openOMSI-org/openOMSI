//! A small software mixer on top of cpal: voices with per-sample linear resampling (pitch),
//! gain, looping and Omsi.exe's 3D model (inverse-distance attenuation beyond a `[3d]`
//! sound's range, a pan of a few dB - see [`pan_gains`]).
//!
//! The game and the mixer share no lock: the game's calls (any thread, `AudioEngine` is
//! `Send + Sync`) put commands into a lock-free ring and keep their own list of the voices
//! playing; the mixer (`render`, on the device's callback) owns the voices, takes the
//! commands in at the start of each block and hands the voices that ended back through a
//! second ring, so that their clips are let go of on the game's side (see `render`). The
//! device's stream lives on a thread of its own (`device`).

mod device;
mod render;

use glam::Vec3;
use hashbrown::HashMap;
use parking_lot::Mutex;
use render::{Back, Cmd, Mixer, MixerCell, Room, Shared, Voice};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub struct Clip {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved 16-bit samples (see `wav::WavData`).
    pub samples: Vec<i16>,
}

impl Clip {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }
}

pub type VoiceId = u64;

#[derive(Debug, Clone, Copy)]
pub struct VoiceParams {
    pub gain: f32,
    /// Playback speed relative to the clip's own rate.
    pub pitch: f32,
    pub looping: bool,
    /// World position; `None` = non-spatial.
    pub position: Option<Vec3>,
    /// Whether motion relative to the listener changes playback pitch.
    pub doppler: bool,
    /// Distance of full volume for spatial voices.
    pub range: f32,
    /// A one-pole low-pass cutoff in Hz, or 0.0 for none: a sound heard through the
    /// bodywork from the cabin loses its edge, not just some volume (a straight gain cut
    /// still reads as "the same sound, turned down" rather than "coming from outside").
    pub lowpass_hz: f32,
    /// OMSI's `[important]` (kept for the traces: the mixer cuts no voice).
    pub important: bool,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            position: None,
            doppler: true,
            range: 5.0,
            lowpass_hz: 0.0,
            important: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub master: f32,
    /// Echo of the place the listener is in (`[triggerbox_setreverb]`: under a bridge):
    /// its reverberation time in seconds, and how much of it is heard (0..1).
    pub reverb_time: f32,
    pub reverb_mix: f32,
    /// The output is a 5.1 system and `[3d]` sounds are to be placed on its five speakers
    /// (#2037, the options' switch): off, everything is panned across two channels as before.
    pub surround: bool,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::Y,
            right: Vec3::X,
            master: 1.0,
            reverb_time: 0.0,
            reverb_mix: 0.0,
            surround: false,
        }
    }
}

/// Commands the ring between the game and the mixer holds; more wait on the game's side.
const RING: usize = 8192;
/// Voices the mixer has room for at first (more are made room for by the game, see `Room`).
const FIRST_ROOM: usize = 512;

/// A voice as the game knows it: the parameters the mixer plays it with, and the newest
/// ones on their way there, with their sequence number.
struct Live {
    params: VoiceParams,
    pending: Option<(u64, VoiceParams)>,
}

impl Live {
    /// The parameters on their way, once the mixer took them in.
    fn settle(&mut self, taken: u64) {
        if let Some((seq, p)) = self.pending {
            if seq <= taken {
                self.params = p;
                self.pending = None;
            }
        }
    }
}

/// The game's side of the engine: what the game's calls touch, never the mixer.
struct GameSide {
    cmds: crate::spsc::Producer<Cmd>,
    /// Commands the ring had no room for (a device that stopped calling back), in order.
    overflow: VecDeque<Cmd>,
    back: crate::spsc::Consumer<Back>,
    /// The voices playing: what the game asks (whether a voice still plays, how loud it is -
    /// thousands of times a frame) without asking the mixer. Started and stopped voices go
    /// in and out at once; those that ended by themselves when the mixer hands them back.
    live: HashMap<VoiceId, Live>,
    next_id: VoiceId,
    /// The last parameters' sequence number (see `Shared::taken`).
    seq: u64,
    /// Voices handed to the mixer and not handed back, and the room it has for them.
    in_mixer: usize,
    room: usize,
    listener: Listener,
}

impl GameSide {
    /// What the mixer handed back, let go of: voices that ended leave the list.
    fn take_back(&mut self) {
        while let Some(b) = self.back.pop() {
            if let Back::Ended(v) = b {
                self.live.remove(&v.id);
                self.in_mixer = self.in_mixer.saturating_sub(1);
            }
        }
    }

    /// Voice `id` as the mixer plays it now (see `Shared::taken`).
    fn current(&mut self, shared: &Shared, id: VoiceId) -> Option<VoiceParams> {
        // (the number first: the voices that ended before the mixer took it are back then)
        let taken = shared.taken.load(Ordering::Acquire);
        self.take_back();
        let l = self.live.get_mut(&id)?;
        l.settle(taken);
        Some(l.params)
    }

    /// The commands waiting on the game's side, into the ring while it has room.
    fn flush(&mut self) {
        while let Some(c) = self.overflow.pop_front() {
            if let Err(c) = self.cmds.push(c) {
                self.overflow.push_front(c);
                break;
            }
        }
    }

    fn push(&mut self, cmd: Cmd) {
        self.flush();
        let cmd = if self.overflow.is_empty() {
            match self.cmds.push(cmd) {
                Ok(()) => return,
                Err(c) => c,
            }
        } else {
            cmd
        };
        // (a game paused while the device stopped calling back must not pile up parameters;
        // new voices and stops are kept)
        if self.overflow.len() > 4096 {
            let live = &mut self.live;
            self.overflow.retain(|c| match c {
                Cmd::Params(id, _, _, seq) => {
                    if let Some(l) = live.get_mut(id) {
                        if l.pending.is_some_and(|p| p.0 == *seq) {
                            l.pending = None;
                        }
                    }
                    false
                }
                _ => true,
            });
        }
        self.overflow.push_back(cmd);
    }

    /// Room in the mixer for one more voice, made before it is needed.
    fn make_room(&mut self) {
        if self.in_mixer + 1 > self.room {
            self.room *= 2;
            self.push(Cmd::Grow(Room::with_capacity(self.room)));
        }
        self.in_mixer += 1;
    }
}

/// Owns the output stream. Dropping it stops playback.
pub struct AudioEngine {
    shared: Arc<Shared>,
    mixer: Arc<MixerCell>,
    game: Mutex<GameSide>,
    /// The device's thread (none without a device) and what it plays on.
    device: Mutex<DeviceState>,
    /// Set when the stream fails (its device went away) or the system's default output
    /// changed (the device thread looks every two seconds).
    reopen: Arc<AtomicBool>,
    /// Clips by file; `None` for a file that is missing or unreadable (not tried again). With
    /// when each was last asked for (see `trim_clips`).
    clips: Arc<Mutex<HashMap<PathBuf, (Option<Arc<Clip>>, std::time::Instant)>>>,
    last_trim: Mutex<std::time::Instant>,
    /// Files a background reader is working on (see `clips_ready`).
    loading: Arc<Mutex<hashbrown::HashSet<PathBuf>>>,
    pub enabled: bool,
    /// Mixed on demand, for a recording (see [`AudioEngine::capture`]).
    capture: bool,
    /// Mixed never (see [`AudioEngine::silent`]): the voices are kept as they are told.
    silent: bool,
    /// The sound sets updated, sorted once a frame for `[sound_maxcount]` (see `packs`).
    packs: Mutex<crate::packs::Packs>,
}

struct DeviceState {
    thread: Option<device::DeviceThread>,
    /// The name of the device played on (see `follow_device`).
    name: String,
    /// Whether a stream plays on it.
    playing: bool,
    /// When the stream was last opened (at most one new stream a second).
    opened: std::time::Instant,
}

// (the simulation may run on a thread of its own: it takes the engine along)
const _: () = {
    const fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<AudioEngine>()
};

fn muted() -> bool {
    omsi_cfg::flags::OMSI_MUTE.is_set()
}

/// The sound formats tried for a file that is missing under its own name, best first.
pub const SOUND_EXTENSIONS: [&str; 9] = [
    "wav", "ogg", "flac", "mp3", "m4a", "aac", "aiff", "aif", "caf",
];

/// Read and decode a clip (any thread).
pub fn read_clip(path: &Path) -> Option<Arc<Clip>> {
    let bytes = match omsi_cfg::vfs::read(path) {
        Ok(b) => b,
        // a sound a mod converted to another format and left the name in its sound.cfg
        // (`horn.wav` shipped as `horn.ogg`): the same name in a format read here
        Err(_) => SOUND_EXTENSIONS
            .iter()
            .filter(|e| path.extension().is_none_or(|x| !x.eq_ignore_ascii_case(e)))
            .find_map(|e| omsi_cfg::vfs::read(&path.with_extension(e)).ok())?,
    };
    // (by content: a `.wav` that is an MP3 is common)
    let clip = match crate::wav::parse_any(&bytes) {
        Ok(w) => Some(Arc::new(Clip {
            sample_rate: w.sample_rate,
            channels: w.channels,
            samples: w.samples,
        })),
        Err(e) => {
            log::warn!("{}: {e}", path.display());
            None
        }
    };
    if let Some(c) = &clip {
        crate::trace::clip_read(Arc::as_ptr(c) as usize, path);
    }
    clip
}

impl AudioEngine {
    /// An engine around a mixer at `rate`/`channels`, no device opened yet.
    fn with_mixer(
        rate: u32,
        channels: usize,
        muted: bool,
        capture: bool,
        silent: bool,
    ) -> AudioEngine {
        let shared = Arc::new(Shared::new(rate, channels, muted, capture));
        let (cmd_tx, cmd_rx) = crate::spsc::channel(RING);
        let (back_tx, back_rx) = crate::spsc::channel(RING / 2);
        let mixer = Arc::new(MixerCell::new(Mixer::new(
            shared.clone(),
            cmd_rx,
            back_tx,
            FIRST_ROOM,
        )));
        AudioEngine {
            game: Mutex::new(GameSide {
                cmds: cmd_tx,
                overflow: VecDeque::new(),
                back: back_rx,
                live: HashMap::new(),
                next_id: 1,
                seq: 0,
                in_mixer: 0,
                room: FIRST_ROOM,
                listener: Listener::default(),
            }),
            shared,
            mixer,
            device: Mutex::new(DeviceState {
                thread: None,
                name: String::new(),
                playing: false,
                opened: std::time::Instant::now(),
            }),
            reopen: Arc::new(AtomicBool::new(false)),
            clips: Default::default(),
            last_trim: Mutex::new(std::time::Instant::now()),
            loading: Default::default(),
            enabled: true,
            capture,
            silent,
            packs: Default::default(),
        }
    }

    /// An engine without a device: clips load and voices are kept, nothing is mixed or
    /// heard - for tools that look at sound configurations.
    pub fn silent() -> AudioEngine {
        crate::trace::engine("silent");
        AudioEngine::with_mixer(48_000, 2, true, false, true)
    }

    /// An engine that plays on no device and is mixed on demand ([`AudioEngine::render_capture`]):
    /// a recording run writes the sound of the same simulated time as its pictures, however
    /// long a picture takes. Clips are read at once (a background read would leave the
    /// first seconds silent), and the clock of the parameters is the mixed sound's own.
    pub fn capture(rate: u32) -> AudioEngine {
        crate::trace::engine("capture");
        AudioEngine::with_mixer(rate.max(8000), 2, false, true, false)
    }

    /// A capture engine's next `frames` frames of the mix (stereo, interleaved), with the
    /// parameters given since the last call. Nothing on an engine playing on a device.
    pub fn render_capture(&self, frames: usize) -> Vec<f32> {
        let Some(us) = self.shared.capture_us.as_ref() else {
            return Vec::new();
        };
        let mut out = vec![0.0f32; frames * 2];
        crate::trace::mixed(frames);
        let mut g = self.game.lock();
        // (more commands than the ring holds: the first ones taken in before the block, as
        // the block takes in the rest)
        while !g.overflow.is_empty() {
            g.flush();
            let at = g.listener.position;
            self.mixer.with(|m| m.take_in(at));
        }
        self.mixer.with(|m| m.render(&mut out));
        g.take_back();
        drop(g);
        let rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1) as u64;
        us.fetch_add(frames as u64 * 1_000_000 / rate, Ordering::Relaxed);
        out
    }

    /// The time the engine goes by: the wall clock, or a recording's own (the mixed sound's,
    /// see [`AudioEngine::capture`]) - a sound's fade-in by the seconds it has played
    /// (`[volcurve] -1`) is heard in a recording as it is in the game, however long the
    /// pictures take to draw.
    pub fn clock(&self) -> std::time::Instant {
        self.shared.now()
    }

    /// The device's (or the capture's) sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.shared.sample_rate.load(Ordering::Relaxed)
    }

    /// Open the default output device. Returns a silent engine if none is available.
    pub fn new() -> AudioEngine {
        crate::trace::engine("device");
        let mut engine = AudioEngine::with_mixer(48_000, 2, muted(), false, false);
        let thread = device::DeviceThread::start(
            engine.shared.clone(),
            engine.mixer.clone(),
            engine.reopen.clone(),
        );
        let name = thread.as_ref().and_then(|t| t.open());
        engine.enabled = name.is_some();
        *engine.device.get_mut() = DeviceState {
            thread,
            playing: name.is_some(),
            name: name.unwrap_or_default(),
            opened: std::time::Instant::now(),
        };
        engine
    }

    /// Follow the system's output: when its default device changed (headphones plugged in,
    /// a Bluetooth headset connected) or the one played on went away, the stream is opened
    /// again on the default device - the sound had stayed on the speakers, or stopped for
    /// good when the headset was disconnected. Cheap; called every frame.
    pub fn follow_device(&self) {
        if !self.enabled {
            return;
        }
        let mut d = self.device.lock();
        if d.opened.elapsed().as_secs_f32() < 1.0 || !self.reopen.swap(false, Ordering::Relaxed) {
            return;
        }
        d.opened = std::time::Instant::now();
        let before = d.name.clone();
        match d.thread.as_ref().and_then(|t| t.open()) {
            Some(now) => {
                if now != before {
                    log::info!("audio: output moved from {before} to {now}");
                }
                d.name = now;
                d.playing = true;
            }
            // (no device right now: tried again when the watcher sees one)
            None => {
                d.name.clear();
                d.playing = false;
            }
        }
    }

    /// A clip from the cache, read now if it is not there.
    pub fn load_clip(&self, path: &Path) -> Option<Arc<Clip>> {
        if let Some(c) = self.clips.lock().get_mut(path) {
            c.1 = std::time::Instant::now();
            return c.0.clone();
        }
        let clip = read_clip(path);
        self.clips.lock().insert(
            path.to_path_buf(),
            (clip.clone(), std::time::Instant::now()),
        );
        clip
    }

    /// Let go of the clips nobody holds (no sound set, no voice) and nobody asked for in
    /// `unused`, once every ten seconds at most: the sounds of every vehicle that ever came
    /// into earshot stayed (140 MB around the Ahlheim main station). They are read again in
    /// the background when a vehicle comes back. Returns the bytes let go.
    pub fn trim_clips(&self, unused: std::time::Duration) -> usize {
        {
            let mut last = self.last_trim.lock();
            if last.elapsed().as_secs_f32() < 10.0 {
                return 0;
            }
            *last = std::time::Instant::now();
        }
        let mut freed = 0usize;
        self.clips.lock().retain(|_, (clip, used)| {
            let idle = used.elapsed() >= unused
                // Failed lookups have no owners; expire those negative cache entries
                // too, or changing/missing sound paths accumulate for the whole session.
                && clip
                    .as_ref()
                    .map(|c| Arc::strong_count(c) == 1)
                    .unwrap_or(true);
            if idle {
                freed += clip.as_ref().map(|c| c.samples.len() * 2).unwrap_or(0);
            }
            !idle
        });
        freed
    }

    /// Whether all of `paths` are in the cache (or known to be missing). The ones that are
    /// not are read on a background thread, started on the first call: a vehicle coming
    /// into earshot for the first time had its sounds read in the frame, which on a map
    /// read from an archive was a stall of up to 400 ms.
    pub fn clips_ready(&self, paths: &[PathBuf]) -> bool {
        if !self.enabled {
            return true;
        }
        if self.capture {
            for p in paths {
                self.load_clip(p);
            }
            return true;
        }
        let missing: Vec<PathBuf> = {
            let mut clips = self.clips.lock();
            let now = std::time::Instant::now();
            paths
                .iter()
                .filter(|p| match clips.get_mut(*p) {
                    Some(c) => {
                        c.1 = now;
                        false
                    }
                    None => true,
                })
                .cloned()
                .collect()
        };
        if missing.is_empty() {
            return true;
        }
        let todo: Vec<PathBuf> = {
            let mut loading = self.loading.lock();
            missing
                .into_iter()
                .filter(|p| loading.insert(p.clone()))
                .collect()
        };
        if !todo.is_empty() {
            let (clips, loading) = (self.clips.clone(), self.loading.clone());
            let back = todo.clone();
            let spawned = std::thread::Builder::new()
                .name("sound loader".into())
                .spawn(move || {
                    for p in todo {
                        let c = read_clip(&p);
                        clips
                            .lock()
                            .insert(p.clone(), (c, std::time::Instant::now()));
                        loading.lock().remove(&p);
                    }
                });
            if spawned.is_err() {
                // no thread: read them here
                for p in back {
                    self.load_clip(&p);
                    self.loading.lock().remove(&p);
                }
                return true;
            }
        }
        false
    }

    pub fn play(&self, clip: Arc<Clip>, params: VoiceParams) -> VoiceId {
        self.play_at(clip, params, 0.0)
    }

    /// Start a loop from a random place in its clip, as Omsi.exe starts a looping buffer
    /// (`SetCurrentPosition(Random(size))`, 0x750c0c): two buses of a type standing side by
    /// side do not drone in phase.
    pub fn play_from_random_place(&self, clip: Arc<Clip>, params: VoiceParams) -> VoiceId {
        static SEED: AtomicU64 = AtomicU64::new(0x2545_f491_4f6c_dd1d);
        let mut x = SEED.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        SEED.store(x, Ordering::Relaxed);
        let start = (x % clip.frames().max(1) as u64) as f64;
        self.play_at(clip, params, start)
    }

    fn play_at(&self, clip: Arc<Clip>, params: VoiceParams, start: f64) -> VoiceId {
        let trace_clip = Arc::as_ptr(&clip) as usize;
        self.start_voice(Some(trace_clip), params, start, |id| {
            Voice::new(id, clip, None, params, start)
        })
    }

    /// A new voice given its id (drawn in the order the commands go to the mixer, so that
    /// its list stays in the order of the ids) and handed to the mixer; playing from now on.
    fn start_voice(
        &self,
        trace_clip: Option<usize>,
        params: VoiceParams,
        start: f64,
        voice: impl FnOnce(VoiceId) -> Voice,
    ) -> VoiceId {
        let mut g = self.game.lock();
        let id = g.next_id;
        g.next_id += 1;
        crate::trace::play(id, trace_clip, &params, start);
        g.live.insert(
            id,
            Live {
                params,
                pending: None,
            },
        );
        g.make_room();
        self.send(&mut g, Cmd::Play(voice(id)));
        id
    }

    /// A command for the mixer: queued for its next block, or (with no mixer taking them
    /// in: an engine without a device) taken in at once.
    fn send(&self, g: &mut GameSide, cmd: Cmd) {
        g.push(cmd);
        if self.direct() {
            let at = g.listener.position;
            self.mixer.with(|m| m.take_in(at));
        }
    }

    /// No mixer takes the commands in: an engine without a device (see [`AudioEngine::silent`]).
    fn direct(&self) -> bool {
        !self.enabled || self.silent
    }

    /// Play what `stream` is fed with (see `stream::StreamBuf`); the voice ends when the
    /// stream is closed.
    pub fn play_stream(
        &self,
        stream: Arc<crate::stream::StreamBuf>,
        params: VoiceParams,
    ) -> VoiceId {
        let clip = Arc::new(Clip {
            sample_rate: 44100,
            channels: 2,
            samples: Vec::new(),
        });
        self.start_voice(None, params, 0.0, |id| {
            Voice::new(id, clip, Some(stream), params, 0.0)
        })
    }

    /// The bus radio's effects on stream voice `id` (see `radio_fx`): made, retuned, or
    /// done away with when all of them are 0.
    pub fn set_radio_fx(&self, id: VoiceId, params: crate::radio_fx::RadioFxParams) {
        crate::trace::radio_fx(id, &params);
        let rate = self.shared.sample_rate.load(Ordering::Relaxed);
        // (made here, should the voice have none yet: the mixer's thread allocates nothing)
        let fresh = if params.is_off() {
            None
        } else {
            crate::radio_fx::RadioFx::new(params, rate).map(Box::new)
        };
        let mut g = self.game.lock();
        self.send(&mut g, Cmd::RadioFx(id, params, fresh));
    }

    /// New parameters for a voice: queued for the mixer's next block (a few milliseconds),
    /// so the game never waits for a block being mixed. Given twice before that block, the
    /// later ones win.
    pub fn set_params(&self, id: VoiceId, params: VoiceParams) {
        crate::trace::set(id, &params);
        let at = self.shared.now();
        let mut g = self.game.lock();
        g.seq += 1;
        let seq = g.seq;
        let taken = self.shared.taken.load(Ordering::Acquire);
        if let Some(l) = g.live.get_mut(&id) {
            l.settle(taken);
            l.pending = Some((seq, params));
        }
        self.send(&mut g, Cmd::Params(id, params, at, seq));
    }

    /// What a voice plays with now, and how loud it arrives at the listener (gain after
    /// distance), while it plays.
    /// (from the game's own list: never waits for a block being mixed)
    pub fn voice_state(&self, id: VoiceId) -> Option<(VoiceParams, f32)> {
        let mut g = self.game.lock();
        let params = g.current(&self.shared, id)?;
        let listener = g.listener.position;
        let spatial = params
            .position
            .map(|p| distance_gain(params.range, (p - listener).length()))
            .unwrap_or(1.0);
        Some((params, params.gain * spatial))
    }

    /// (from the game's own list: never waits for a block being mixed)
    pub fn is_playing(&self, id: VoiceId) -> bool {
        let mut g = self.game.lock();
        g.take_back();
        g.live.contains_key(&id)
    }

    pub fn stop(&self, id: VoiceId) {
        crate::trace::stop(id);
        let mut g = self.game.lock();
        g.take_back();
        if g.live.remove(&id).is_some() {
            self.send(&mut g, Cmd::Stop(id));
        }
    }

    pub fn listener_position(&self) -> Vec3 {
        self.game.lock().listener.position
    }

    pub fn set_listener(&self, l: Listener) {
        crate::trace::listener(&l);
        let mut g = self.game.lock();
        g.listener = l;
        self.shared.listener.store(&l);
        // (once a frame: what waits on the game's side goes on, what came back is let go of)
        g.flush();
        g.take_back();
        drop(g);
        // (and the sound packs of the frame are sorted for the next: `[sound_maxcount]`)
        self.packs.lock().end_frame(crate::packs::sound_maxcount());
    }

    /// A sound set (a pack, see `packs`) is updated: whether it may be heard, as the packs
    /// were sorted at the end of the last frame.
    pub(crate) fn pack_heard(&self, p: crate::packs::Pack) -> bool {
        self.packs.lock().heard(p)
    }

    /// Whether the sound set (pack) `id` was too far at the end of the last frame.
    pub(crate) fn pack_too_far(&self, id: u64) -> bool {
        self.packs.lock().is_too_far(id)
    }

    /// OMSI's `sound_doppler`: a sound coming closer is higher, one going away lower (on
    /// unless told otherwise).
    pub fn set_doppler(&self, on: bool) {
        self.shared.doppler.store(on, Ordering::Relaxed);
    }

    /// OMSI's `[sound_stereo]` (0 to 20, 10 when options.cfg has none): how far a `[3d]`
    /// sound is panned (see [`pan_gains`]).
    pub fn set_stereo(&self, stereo: u32) {
        self.shared.stereo.store(stereo, Ordering::Relaxed);
    }

    /// The voices playing.
    pub fn voice_count(&self) -> usize {
        let mut g = self.game.lock();
        g.take_back();
        g.live.len()
    }
}

#[cfg(test)]
mod tests;
/// How loud a sound `dist` metres away arrives, with `range` its `[3d]` reference distance:
/// Omsi.exe multiplies the volume by `min(range / distance, 1)` itself (`TSound` update
/// 0x7509fe) - full up to the range, then 1/d (6 dB per doubling); a range of 0 is silent
/// anywhere but at the very spot. A sound at no real place (a vehicle whose position has
/// gone to NaN or infinity for a frame) is not heard: `min` passes over a NaN, and such a
/// sound was played as if at the listener's ear - an AI car's engine at full volume, a
/// loud burst while the car flickered out of sight (#2416).
pub fn distance_gain(range: f32, dist: f32) -> f32 {
    if !dist.is_finite() {
        return 0.0;
    }
    if dist <= 0.0 {
        return 1.0;
    }
    (range.max(0.0) / dist).min(1.0)
}

/// The channel gains of a sound in direction `d` from the listener, as Omsi.exe pans a
/// `[3d]` sound (0x7508ef): `s` is the side component of the horizontal direction (1 =
/// straight right), the buffer's pan `50 * [sound_stereo] * s^3` hundredths of a dB, and
/// DirectSound lowers the far channel by that much and leaves the near one alone - at the
/// default stereo setting a sound straight to the right is 5 dB down on the left. Ours gave
/// the near channel 1.2 and the far one 0, a centred sound 0.85 on both. `stereo`: OMSI's
/// `[sound_stereo]` (see [`AudioEngine::set_stereo`]).
pub fn pan_gains(listener: &Listener, d: Vec3, stereo: f32) -> (f32, f32) {
    if stereo <= 0.0 {
        return (1.0, 1.0);
    }
    let right = listener.right.normalize_or_zero();
    let up = right.cross(listener.forward).normalize_or_zero();
    let s = (d - up * d.dot(up))
        .normalize_or_zero()
        .dot(right)
        .clamp(-1.0, 1.0);
    let pan = (50.0 * stereo * s * s * s).clamp(-10_000.0, 10_000.0);
    let far = 10f32.powf(-pan.abs() / 2000.0);
    if pan > 0.0 { (far, 1.0) } else { (1.0, far) }
}

/// Where a 5.1 layout's speakers stand, in degrees from straight ahead, clockwise (the angle
/// [`surround_gains`] measures a source at): centre, front left, front right, back left,
/// back right. The sub-woofer is not here: it takes the bass of the whole mix, not a source.
const SURROUND_SPEAKERS: [(usize, f32); 5] =
    [(2, 0.0), (0, -30.0), (1, 30.0), (4, -110.0), (5, 110.0)];

/// The gains of a `[3d]` sound `d` away from the listener on a 5.1 system, in the order front
/// left, front right, centre, low frequency, back left, back right (#2037).
///
/// The sound goes to the speakers its direction points at, and only those: a door opening at
/// the front right of the bus is heard on the front right speaker, a rear door on the back
/// right one, the engine behind the cab on the back pair. A speaker 60° or more away from the
/// source is silent, the ones nearer share the sound with equal power (so a source exactly
/// towards one speaker has the whole of it there), and the sub-woofer never gets any of it -
/// bass management is the receiver's business, and a source placed in the sub is heard under
/// the seats.
///
/// `stereo` is not used: OMSI's `[sound_stereo]` narrows the stereo picture, which a surround
/// system does not need (it has a speaker where the sound is).
pub fn surround_gains(listener: &Listener, d: Vec3) -> [f32; 6] {
    let right = listener.right.normalize_or_zero();
    let forward = listener.forward.normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    // (the direction in the listener's horizontal plane: height picks no speaker)
    let flat = (d - up * d.dot(up)).normalize_or_zero();
    let angle = flat.dot(right).atan2(flat.dot(forward)).to_degrees();
    let mut gains = [0.0f32; 6];
    for (speaker, at) in SURROUND_SPEAKERS {
        // (the angle between the two, the short way round: -180..180)
        let mut off = (angle - at).abs() % 360.0;
        if off > 180.0 {
            off = 360.0 - off;
        }
        gains[speaker] = (1.0 - off / 60.0).clamp(0.0, 1.0);
    }
    // equal power: the squares add up to one, so a sound between two speakers is as loud as
    // it is on one of them
    let power: f32 = gains.iter().map(|g| g * g).sum();
    if power > 0.0 {
        let norm = power.sqrt();
        for g in gains.iter_mut() {
            *g /= norm;
        }
    } else {
        // (nowhere on the ring - straight above, say: the centre it is)
        gains[2] = 1.0;
    }
    gains
}

#[cfg(test)]
mod distance_tests {
    #[test]
    fn inverse_distance_beyond_the_reference() {
        assert_eq!(super::distance_gain(2.0, 1.0), 1.0);
        assert!((super::distance_gain(2.0, 4.0) - 0.5).abs() < 1e-6);
        assert!((super::distance_gain(1.0, 10.0) - 0.1).abs() < 1e-6);
        assert_eq!(super::distance_gain(0.0, 3.0), 0.0, "a range of 0");
        assert_eq!(
            super::distance_gain(5.0, f32::NAN),
            0.0,
            "nowhere: not heard"
        );
        assert_eq!(super::distance_gain(5.0, f32::INFINITY), 0.0);
    }

    #[test]
    fn a_surround_source_is_heard_from_the_speakers_it_faces() {
        use glam::Vec3;
        let l = super::Listener {
            forward: Vec3::Y,
            right: Vec3::X,
            ..Default::default()
        };
        let at = |deg: f32| {
            let r = deg.to_radians();
            super::surround_gains(&l, Vec3::new(r.sin() * 5.0, r.cos() * 5.0, 0.0))
        };
        // (the front right door, 30° to the driver's right: the front right speaker, the
        // middle of the front taken along by the centre - and nothing on the left or behind)
        let g = at(30.0);
        assert!(g[1] > 0.8 && g[2] > 0.0 && g[2] < g[1], "{g:?}");
        assert!(g[0] < 1e-4 && g[4] < 1e-4 && g[5] < 1e-4);
        // (a rear door 110° behind on the right: the back right one, alone)
        let g = at(110.0);
        assert!((g[5] - 1.0).abs() < 1e-4, "{g:?}");
        assert!(g[0] < 1e-4 && g[1] < 1e-4 && g[2] < 1e-4 && g[4] < 1e-4);
        // (the engine straight ahead: the centre speaker, and never the sub-woofer)
        let g = at(0.0);
        assert!(
            (g[2] - 0.8).abs() < 0.1 && g[2] > g[0] && g[2] > g[1],
            "{g:?}"
        );
        assert_eq!(g[3], 0.0, "the sub gets nothing");
        // (between two speakers the sound is shared, at the same power as on one of them)
        let g = at(20.0);
        assert!(g[1] > 0.5 && g[2] > 0.5, "{g:?}");
        assert!((g[0] * g[0] + g[1] * g[1] + g[2] * g[2] - 1.0).abs() < 1e-4);
        // (behind on the left: the back pair, that side first)
        let g = at(-140.0);
        assert!(
            g[4] > g[5] && g[4] > 0.5 && g[0] < 1e-4 && g[1] < 1e-4,
            "{g:?}"
        );
        // (right above the listener there is no side to go to: the front, centre and all)
        let g = super::surround_gains(&l, Vec3::new(0.0, 0.0, 2.0));
        assert!(
            g[2] > 0.8 && g[2] > g[0] && g[0] == g[1] && g[4] < 1e-4 && g[5] < 1e-4,
            "{g:?}"
        );
        // (the same direction 20 m away and higher: the same speakers - the distance and the
        // height are the mixer's business)
        assert_eq!(at(-90.0), {
            let r = (-90.0f32).to_radians();
            super::surround_gains(&l, Vec3::new(r.sin() * 20.0, r.cos() * 20.0, 3.0))
        });
    }

    #[test]
    fn a_sound_to_the_right_is_5_db_down_on_the_left() {
        use glam::Vec3;
        let l = super::Listener {
            forward: Vec3::Y,
            right: Vec3::X,
            ..Default::default()
        };
        let (left, right) = super::pan_gains(&l, Vec3::new(3.0, 0.0, 1.0), 10.0);
        assert!(
            (left - 10f32.powf(-0.25)).abs() < 1e-4 && right == 1.0,
            "{left} {right}"
        );
        let (left, right) = super::pan_gains(&l, Vec3::new(0.0, 5.0, 0.0), 10.0);
        assert_eq!((left, right), (1.0, 1.0), "ahead: both at full");
        let (left, right) = super::pan_gains(&l, Vec3::new(-1.0, 1.0, 0.0), 10.0);
        assert!(left == 1.0 && right < 1.0 && right > 0.8, "{right}");
    }
}

#[cfg(test)]
mod limiter_tests {
    use super::render::soft_clip;

    #[test]
    fn soft_clip_is_smooth_and_bounded() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert!(soft_clip(3.0) <= 1.0 && soft_clip(-3.0) >= -1.0);
        assert!(soft_clip(0.95) > 0.9 && soft_clip(0.95) < 0.95);
    }
}

#[cfg(test)]
mod clip_cache_eviction_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn expired_missing_sound_files_leave_the_cache() {
        let engine = AudioEngine::capture(48_000);
        let missing = PathBuf::from("nonexistent-audio-for-trim-test.wav");
        engine.clips.lock().insert(
            missing.clone(),
            (None, Instant::now() - Duration::from_secs(90)),
        );
        *engine.last_trim.lock() = Instant::now() - Duration::from_secs(11);
        assert_eq!(engine.trim_clips(Duration::from_secs(60)), 0);
        assert!(!engine.clips.lock().contains_key(&missing));
    }

    #[test]
    fn referenced_sound_remains_in_cache_when_old() {
        let engine = AudioEngine::capture(48_000);
        let path = PathBuf::from("sound-with-an-active-owner.wav");
        let clip = Arc::new(Clip {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![0; 48],
        });
        engine.clips.lock().insert(
            path.clone(),
            (Some(clip.clone()), Instant::now() - Duration::from_secs(90)),
        );
        *engine.last_trim.lock() = Instant::now() - Duration::from_secs(11);
        assert_eq!(engine.trim_clips(Duration::from_secs(60)), 0);
        assert!(engine.clips.lock().contains_key(&path));
    }
}
