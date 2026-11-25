//! The mixer's side: the voices as they play, owned by whoever mixes the next block (the
//! device's callback, or the game itself for a recording and an engine without a device).
//! It takes the game's commands from a lock-free ring, mixes, and hands what is to be freed
//! back through another: in the callback nothing waits for a lock and nothing is allocated
//! or freed (a block mixed late is a click, and the allocator may hold a lock).

use super::{Clip, Listener, VoiceId, VoiceParams, distance_gain, pan_gains};
use crate::spsc;
use glam::Vec3;
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering, fence};

pub(super) struct Voice {
    pub(super) id: VoiceId,
    pub(super) clip: Arc<Clip>,
    /// A voice fed while it plays (internet radio) instead of from `clip`.
    pub(super) stream: Option<Arc<crate::stream::StreamBuf>>,
    /// The mixer's end of `stream` (none when another voice took it).
    tap: Option<crate::stream::StreamInner>,
    pub(super) params: VoiceParams,
    pub(super) pos: f64,
    pub(super) finished: bool,
    /// Smoothed gain to avoid clicks.
    pub(super) cur_gain: f32,
    /// One-pole low-pass filter state, left/right.
    pub(super) lp: [f32; 2],
    /// The low-pass was just switched on: it starts from the sound as it is (from the
    /// state it was left in a while ago, a voice coming into the cab clicked).
    pub(super) lp_fresh: bool,
    /// The bus radio's effects on a stream (see `radio_fx`); None plays it as it is.
    pub(super) fx: Option<Box<crate::radio_fx::RadioFx>>,
    /// The Doppler shift: the distance to the listener when the position last came, when,
    /// and the (smoothed) pitch factor it gives.
    pub(super) doppler: (f32, Option<std::time::Instant>, f32),
}

impl Voice {
    /// A voice on `clip` (or on `stream`), playing from frame `start`.
    pub(super) fn new(
        id: VoiceId,
        clip: Arc<Clip>,
        stream: Option<Arc<crate::stream::StreamBuf>>,
        params: VoiceParams,
        start: f64,
    ) -> Voice {
        let tap = stream.as_ref().and_then(|s| s.take_tap());
        Voice {
            id,
            clip,
            stream,
            tap,
            params,
            pos: start,
            finished: false,
            cur_gain: 0.0,
            lp: [0.0, 0.0],
            lp_fresh: false,
            fx: None,
            doppler: (0.0, None, 1.0),
        }
    }
}

/// The voice list, made on the game's side with room for `cap` voices: the mixer moves
/// into a bigger one before it would have to grow its own.
pub(super) struct Room {
    pub(super) voices: Vec<Voice>,
}

impl Room {
    pub(super) fn with_capacity(cap: usize) -> Room {
        Room {
            voices: Vec::with_capacity(cap),
        }
    }
}

/// What the game asks of the mixer, taken in at the start of its next block, in the order
/// given.
#[allow(clippy::large_enum_variant)]
pub(super) enum Cmd {
    Play(Voice),
    /// New parameters, with the time they were given (for the Doppler shift) and their
    /// number in the game's sequence (see `Shared::taken`).
    Params(VoiceId, VoiceParams, std::time::Instant, u64),
    Stop(VoiceId),
    /// The bus radio's effects on a stream voice, with new ones made ready in case it has none.
    RadioFx(
        VoiceId,
        crate::radio_fx::RadioFxParams,
        Option<Box<crate::radio_fx::RadioFx>>,
    ),
    /// Room for more voices (see [`Room`]).
    Grow(Room),
}

/// What the mixer hands back to the game to be let go of there (the effects and lists are
/// only dropped: never read).
#[allow(clippy::large_enum_variant, dead_code)]
pub(super) enum Back {
    /// A voice that ended (played out, or stopped): the game takes it out of its list.
    Ended(Voice),
    Fx(Box<crate::radio_fx::RadioFx>),
    Room(Room),
}

/// The listener as the game gave it last, for the mixer to read at the start of a block
/// without a lock: a sequence lock over its numbers (one writer, the game; the reader
/// reads again in the rare case the numbers changed while it read).
pub(super) struct ListenerCell {
    seq: AtomicU32,
    v: [AtomicU32; 13],
}

impl ListenerCell {
    fn new(l: &Listener) -> ListenerCell {
        let c = ListenerCell {
            seq: AtomicU32::new(0),
            v: Default::default(),
        };
        c.store(l);
        c
    }

    /// (one writer at a time: the game, under its lock)
    pub(super) fn store(&self, l: &Listener) {
        let s = self.seq.load(Ordering::Relaxed);
        self.seq.store(s.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        let n = [
            l.position.x,
            l.position.y,
            l.position.z,
            l.forward.x,
            l.forward.y,
            l.forward.z,
            l.right.x,
            l.right.y,
            l.right.z,
            l.master,
            l.reverb_time,
            l.reverb_mix,
            0.0,
        ];
        for (a, x) in self.v.iter().zip(n) {
            a.store(x.to_bits(), Ordering::Relaxed);
        }
        self.seq.store(s.wrapping_add(2), Ordering::Release);
    }

    pub(super) fn load(&self) -> Listener {
        loop {
            let s1 = self.seq.load(Ordering::Acquire);
            let n: [f32; 13] =
                std::array::from_fn(|i| f32::from_bits(self.v[i].load(Ordering::Relaxed)));
            fence(Ordering::Acquire);
            if s1 & 1 == 0 && self.seq.load(Ordering::Relaxed) == s1 {
                return Listener {
                    position: Vec3::new(n[0], n[1], n[2]),
                    forward: Vec3::new(n[3], n[4], n[5]),
                    right: Vec3::new(n[6], n[7], n[8]),
                    master: n[9],
                    reverb_time: n[10],
                    reverb_mix: n[11],
                };
            }
            std::hint::spin_loop();
        }
    }
}

/// What the game's side and the mixer's both read: plain numbers, no locks.
pub(super) struct Shared {
    /// The output device's rate and channels: those of the device played on now (the stream
    /// is opened again on another device when the system's output changes).
    pub(super) sample_rate: AtomicU32,
    pub(super) channels: AtomicUsize,
    /// `OMSI_MUTE`: everything is mixed as usual (voices play and end), nothing is heard -
    /// for test runs on a machine somebody is working at.
    muted: bool,
    /// A recording run's clock (see `AudioEngine::capture`): microseconds of mixed sound,
    /// counted from `epoch`, stand in for the wall clock - the Doppler shift is worked out
    /// from how far a source moved between two parameter updates, and a run that renders
    /// slower than real time would otherwise hear every car standing still.
    pub(super) capture_us: Option<AtomicU64>,
    epoch: std::time::Instant,
    pub(super) listener: ListenerCell,
    /// The sequence number of the last parameters the mixer took in: the game's
    /// `voice_state` answers with the parameters a voice plays with, not those on their way.
    pub(super) taken: AtomicU64,
    /// OMSI's `sound_doppler` and `[sound_stereo]` (see `AudioEngine::set_doppler`,
    /// `set_stereo`).
    pub(super) doppler: AtomicBool,
    pub(super) stereo: AtomicU32,
}

impl Shared {
    pub(super) fn new(rate: u32, channels: usize, muted: bool, capture: bool) -> Shared {
        Shared {
            sample_rate: AtomicU32::new(rate),
            channels: AtomicUsize::new(channels),
            muted,
            capture_us: capture.then(|| AtomicU64::new(0)),
            epoch: std::time::Instant::now(),
            listener: ListenerCell::new(&Listener::default()),
            taken: AtomicU64::new(0),
            doppler: AtomicBool::new(true),
            stereo: AtomicU32::new(10),
        }
    }

    /// The time parameters are stamped with: the wall clock, or a recording's own.
    pub(super) fn now(&self) -> std::time::Instant {
        match &self.capture_us {
            Some(us) => self.epoch + std::time::Duration::from_micros(us.load(Ordering::Relaxed)),
            None => std::time::Instant::now(),
        }
    }
}

/// The mixer, shared by the stream's callback and the engine: whoever mixes holds it, and
/// the callback only ever *tries* to (a block it could not have is silence; it never
/// waits). With a device, only the callback mixes; the game holds it only for a recording
/// or an engine without a device, where no callback runs.
pub(super) struct MixerCell {
    busy: AtomicBool,
    mixer: UnsafeCell<Mixer>,
}

// (the mixer is reached only through `try_with`/`with`, by one thread at a time)
unsafe impl Sync for MixerCell {}

struct Held<'a>(&'a AtomicBool);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl MixerCell {
    pub(super) fn new(m: Mixer) -> MixerCell {
        MixerCell {
            busy: AtomicBool::new(false),
            mixer: UnsafeCell::new(m),
        }
    }

    /// `f` on the mixer, unless somebody else holds it.
    pub(super) fn try_with<R>(&self, f: impl FnOnce(&mut Mixer) -> R) -> Option<R> {
        if self.busy.swap(true, Ordering::Acquire) {
            return None;
        }
        let _held = Held(&self.busy);
        Some(f(unsafe { &mut *self.mixer.get() }))
    }

    /// `f` on the mixer, waiting for it (the game's side only, never the callback).
    pub(super) fn with<R>(&self, f: impl FnOnce(&mut Mixer) -> R) -> R {
        while self.busy.swap(true, Ordering::Acquire) {
            std::thread::yield_now();
        }
        let _held = Held(&self.busy);
        f(unsafe { &mut *self.mixer.get() })
    }
}

pub(super) struct Mixer {
    /// The voices as the mixer plays them, by rising id (each is put in by the queue, where
    /// its id is drawn).
    pub(super) voices: Vec<Voice>,
    /// The echo of an underpass, fed from the mix.
    pub(super) reverb: Reverb,
    /// The master limiter's gain now (1 = none).
    limiter: f32,
    cmds: spsc::Consumer<Cmd>,
    back: spsc::Producer<Back>,
    pub(super) shared: Arc<Shared>,
}

impl Mixer {
    /// A mixer with room for `room` voices, taking its commands from `cmds` and handing
    /// back through `back`; its reverb ready for `rate` and `channels`.
    pub(super) fn new(
        shared: Arc<Shared>,
        cmds: spsc::Consumer<Cmd>,
        back: spsc::Producer<Back>,
        room: usize,
    ) -> Mixer {
        let rate = shared.sample_rate.load(Ordering::Relaxed);
        let ch = shared.channels.load(Ordering::Relaxed);
        let Room { voices } = Room::with_capacity(room);
        Mixer {
            voices,
            reverb: Reverb::ready(rate, ch),
            limiter: 1.0,
            cmds,
            back,
            shared,
        }
    }

    fn give_back(&mut self, b: Back) {
        // (the game takes them out every call; a full ring means a game that stopped
        // asking anything - then this one is let go of here)
        let _ = self.back.push(b);
    }

    /// A reverb made ready for a new device's format (by the thread that opened it) put in
    /// place of one made for another; the old one goes back into `spare`.
    pub(super) fn adopt_reverb(&mut self, spare: &mut Reverb) {
        let rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1);
        let ch = self.shared.channels.load(Ordering::Relaxed).max(1);
        if !self.reverb.fits(rate, ch) && spare.fits(rate, ch) {
            std::mem::swap(&mut self.reverb, spare);
        }
    }

    /// The game's commands, in the order given, applied to the voices.
    pub(super) fn take_in(&mut self, listener: Vec3) {
        // (by rising id: a binary search; a voice put in some other way is still found)
        fn find(voices: &[Voice], id: VoiceId) -> Option<usize> {
            voices
                .binary_search_by_key(&id, |v| v.id)
                .ok()
                .or_else(|| voices.iter().position(|v| v.id == id))
        }
        let mut taken = None;
        while let Some(cmd) = self.cmds.pop() {
            match cmd {
                Cmd::Play(v) => self.voices.push(v),
                Cmd::Params(id, params, at, seq) => {
                    if let Some(k) = find(&self.voices, id) {
                        apply_params(
                            &mut self.voices[k],
                            params,
                            at,
                            listener,
                            self.shared.doppler.load(Ordering::Relaxed),
                        );
                    }
                    taken = Some(seq);
                }
                Cmd::Stop(id) => {
                    if let Some(k) = find(&self.voices, id) {
                        self.voices[k].finished = true;
                    }
                }
                Cmd::RadioFx(id, params, fresh) => {
                    let rate = self.shared.sample_rate.load(Ordering::Relaxed);
                    let mut unused = fresh;
                    if let Some(k) = find(&self.voices, id) {
                        let v = &mut self.voices[k];
                        match (v.fx.as_mut(), params.is_off()) {
                            (_, true) => {
                                if let Some(old) = v.fx.take() {
                                    self.give_back(Back::Fx(old));
                                }
                            }
                            (Some(fx), false) => fx.set(params, rate),
                            (None, false) => v.fx = unused.take(),
                        }
                    }
                    if let Some(fx) = unused {
                        self.give_back(Back::Fx(fx));
                    }
                }
                Cmd::Grow(mut room) => {
                    room.voices.append(&mut self.voices);
                    std::mem::swap(&mut self.voices, &mut room.voices);
                    self.give_back(Back::Room(room));
                }
            }
        }
        if let Some(seq) = taken {
            self.shared.taken.store(seq, Ordering::Release);
        }
    }

    pub(super) fn render(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        let listener = self.shared.listener.load();
        self.take_in(listener.position);
        let ch = self.shared.channels.load(Ordering::Relaxed).max(1);
        let frames = out.len() / ch;
        let rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1);
        // (every voice is mixed: Omsi.exe cuts whole sound packs for `[sound_maxcount]`,
        // before they play - see `crate::packs`)
        let stereo = self.shared.stereo.load(Ordering::Relaxed) as f32;
        for v in self.voices.iter_mut() {
            if v.finished {
                continue;
            }
            mix_voice(v, out, ch, frames, rate, &listener, stereo);
        }
        if self.voices.iter().any(|v| v.finished) {
            // (the ended voices go back to the game, which lets go of their clips; while
            // its ring is full they stay, finished, for a later block)
            let mut room = self.back.slots();
            let tracing = crate::trace::on();
            let ended = self.voices.extract_if(.., |v| {
                let out = v.finished && room > 0;
                room -= out as usize;
                out
            });
            for v in ended {
                if tracing {
                    crate::trace::ended(&[v.id]);
                }
                let _ = self.back.push(Back::Ended(v));
            }
        }
        // (a NaN that got into the mix would stay in the reverb's lines for good and reach
        // the device as a burst: dropped here)
        for s in out.iter_mut() {
            if !s.is_finite() {
                *s = 0.0;
            }
        }
        if listener.reverb_mix > 0.001 && listener.reverb_time > 0.05 {
            self.reverb.process(
                out,
                ch,
                rate,
                listener.reverb_time.min(3.0),
                listener.reverb_mix.min(1.0),
            );
        }
        // The master limiter: a busy street sums past full scale, and cut off hard there the
        // sound crackled and squeaked. Loud moments are turned down (at once) and back up
        // (over half a second), and what still peaks is rounded off, not cut.
        {
            let g = &mut self.limiter;
            let frames = (out.len() / ch.max(1)).max(1);
            let release = (-1.0 / (0.5 * rate as f32)).exp();
            for f in 0..frames {
                let peak = (0..ch)
                    .map(|c| out[f * ch + c].abs())
                    .fold(0.0f32, f32::max);
                let want = if peak * *g > 0.9 { 0.9 / peak } else { 1.0 };
                *g = if want < *g {
                    want
                } else {
                    want + (*g - want) * release
                };
                for c in 0..ch {
                    out[f * ch + c] = soft_clip(out[f * ch + c] * *g);
                }
            }
        }
        let muted = self.shared.muted;
        for s in out.iter_mut() {
            *s = if muted { 0.0 } else { s.clamp(-1.0, 1.0) };
        }
    }
}

/// Voice `v` mixed into `out` (`frames` frames of `ch` channels at `rate`).
fn mix_voice(
    v: &mut Voice,
    out: &mut [f32],
    ch: usize,
    frames: usize,
    rate: u32,
    listener: &Listener,
    stereo: f32,
) {
    let dev_rate = rate as f64;
    // spatialisation
    let (mut left, mut right) = (1.0f32, 1.0f32);
    let mut spatial_gain = 1.0f32;
    if let Some(p) = v.params.position {
        let d = p - listener.position;
        spatial_gain = distance_gain(v.params.range, d.length());
        (left, right) = pan_gains(listener, d, stereo);
    }
    let target_gain = (v.params.gain * spatial_gain * listener.master).max(0.0);
    if let Some(sb) = v.stream.clone() {
        let lp_alpha = if v.params.lowpass_hz > 0.0 {
            1.0 - (-2.0 * std::f32::consts::PI * v.params.lowpass_hz / dev_rate as f32).exp()
        } else {
            1.0
        };
        let Some(buf) = v.tap.as_mut() else {
            v.finished = sb.is_closed();
            return;
        };
        buf.sync(&sb);
        if buf.closed {
            v.finished = true;
            return;
        }
        let step = buf.rate as f64 / dev_rate;
        if let Some(fx) = v.fx.as_mut() {
            fx.set(fx.params(), rate);
        }
        for f in 0..frames {
            v.cur_gain += (target_gain - v.cur_gain) * 0.005;
            // silence while the buffer fills (at the start, after a stall)
            let Some((a, b)) = buf.pair() else { break };
            let t = v.pos as f32;
            let (mut l, mut r) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            if v.params.lowpass_hz > 0.0 {
                if std::mem::take(&mut v.lp_fresh) {
                    v.lp = [l, r];
                }
                v.lp[0] += (l - v.lp[0]) * lp_alpha;
                v.lp[1] += (r - v.lp[1]) * lp_alpha;
                (l, r) = (v.lp[0], v.lp[1]);
            }
            if let Some(fx) = v.fx.as_mut() {
                (l, r) = fx.process(l, r);
            }
            let g = v.cur_gain;
            out[f * ch] += l * g * left;
            if ch > 1 {
                out[f * ch + 1] += r * g * right;
            }
            v.pos += step;
            while v.pos >= 1.0 {
                v.pos -= 1.0;
                buf.advance();
            }
        }
        buf.publish(&sb);
        return;
    }
    let clip = &v.clip;
    let cch = clip.channels as usize;
    let nframes = clip.frames();
    if nframes == 0 {
        v.finished = true;
        return;
    }
    let step = (v.params.pitch * v.doppler.2).max(0.01) as f64 * clip.sample_rate as f64 / dev_rate;
    // one-pole low-pass: alpha such that the filter's -3dB point sits at `lowpass_hz`
    let lp_alpha = if v.params.lowpass_hz > 0.0 {
        1.0 - (-2.0 * std::f32::consts::PI * v.params.lowpass_hz / dev_rate as f32).exp()
    } else {
        1.0
    };
    for f in 0..frames {
        // smooth gain over ~5 ms
        v.cur_gain += (target_gain - v.cur_gain) * 0.005;
        let mut i0 = v.pos as usize;
        if i0 >= nframes {
            if !v.params.looping {
                v.finished = true;
                break;
            }
            // wrap and mix this output frame from the loop's start (skipping it left
            // a silent frame at every turn of the loop: a click on every engine loop)
            v.pos %= nframes as f64;
            i0 = (v.pos as usize).min(nframes - 1);
        }
        let i1 = if i0 + 1 < nframes {
            i0 + 1
        } else if v.params.looping {
            0
        } else {
            i0
        };
        let t = (v.pos - i0 as f64) as f32;
        let sample = |c: usize| {
            let a = clip.samples[i0 * cch + c.min(cch - 1)] as f32 / 32768.0;
            let b = clip.samples[i1 * cch + c.min(cch - 1)] as f32 / 32768.0;
            a + (b - a) * t
        };
        let (l, r) = if cch >= 2 {
            (sample(0), sample(1))
        } else {
            let m = sample(0);
            (m, m)
        };
        let (l, r) = if v.params.lowpass_hz > 0.0 {
            if std::mem::take(&mut v.lp_fresh) {
                v.lp = [l, r];
            }
            v.lp[0] += (l - v.lp[0]) * lp_alpha;
            v.lp[1] += (r - v.lp[1]) * lp_alpha;
            (v.lp[0], v.lp[1])
        } else {
            (l, r)
        };
        let g = v.cur_gain;
        out[f * ch] += l * g * left;
        if ch > 1 {
            out[f * ch + 1] += r * g * right;
        }
        v.pos += step;
    }
}

/// Parameters for voice `v`, given at `now` with the listener at `listener`: the Doppler
/// shift from how fast the distance to the listener changes (the bus's own sounds move with
/// the listener and keep their pitch), when `doppler` (OMSI's `sound_doppler`) is on.
pub(super) fn apply_params(
    v: &mut Voice,
    params: VoiceParams,
    now: std::time::Instant,
    listener: Vec3,
    doppler: bool,
) {
    if let (Some(p), true) = (
        params.position.filter(|p| p.is_finite()),
        params.doppler && doppler,
    ) {
        let dist = (p - listener).length();
        let (last, at, factor) = v.doppler;
        let mut f = factor;
        if let Some(at) = at {
            let dt = now.saturating_duration_since(at).as_secs_f32();
            if (0.004..0.5).contains(&dt) {
                let radial = ((dist - last) / dt).clamp(-60.0, 60.0);
                let target = 343.0 / (343.0 + radial);
                f += (target - f) * (dt / 0.25).min(1.0);
            }
        }
        v.doppler = (dist, Some(now), f);
    } else {
        v.doppler = (0.0, None, 1.0);
    }
    if params.lowpass_hz > 0.0 && !(v.params.lowpass_hz > 0.0) {
        v.lp_fresh = true;
    }
    v.params = params;
}

/// A small Schroeder reverb (four combs in parallel, two all-passes in series, per channel)
/// for the echo under a bridge: the combs' feedback gives the reverberation time.
#[derive(Default)]
pub(super) struct Reverb {
    lines: Vec<Vec<(Vec<f32>, usize, f32)>>,
    allpass: Vec<Vec<(Vec<f32>, usize)>>,
    rate: u32,
}

impl Reverb {
    const COMBS: [f32; 4] = [0.0297, 0.0371, 0.0411, 0.0437];
    const ALLPASS: [f32; 2] = [0.005, 0.0017];

    /// A reverb with its delay lines made for `rate` and `ch` channels (silent).
    pub(super) fn ready(rate: u32, ch: usize) -> Reverb {
        Reverb {
            lines: (0..ch)
                .map(|c| {
                    Self::COMBS
                        .iter()
                        .map(|d| {
                            (
                                vec![0.0; ((d + c as f32 * 0.0011) * rate as f32) as usize + 1],
                                0,
                                0.0,
                            )
                        })
                        .collect()
                })
                .collect(),
            allpass: (0..ch)
                .map(|_| {
                    Self::ALLPASS
                        .iter()
                        .map(|d| (vec![0.0; (d * rate as f32) as usize + 1], 0))
                        .collect()
                })
                .collect(),
            rate,
        }
    }

    fn fits(&self, rate: u32, ch: usize) -> bool {
        self.rate == rate && self.lines.len() == ch
    }

    pub(super) fn process(&mut self, out: &mut [f32], ch: usize, rate: u32, rt60: f32, mix: f32) {
        if !self.fits(rate, ch) {
            // (made ready with the stream normally: see `adopt_reverb`)
            *self = Reverb::ready(rate, ch);
        }
        let frames = out.len() / ch;
        // feedback so that a comb decays by 60 dB in rt60 seconds
        let gains: [f32; 4] = Self::COMBS.map(|d| 10f32.powf(-3.0 * d / rt60));
        for c in 0..ch {
            let combs = &mut self.lines[c];
            let aps = &mut self.allpass[c];
            for f in 0..frames {
                let x = out[f * ch + c];
                let mut y = 0.0;
                for (k, (buf, i, lp)) in combs.iter_mut().enumerate() {
                    let d = buf[*i];
                    *lp = d * 0.8 + *lp * 0.2;
                    buf[*i] = x + *lp * gains[k];
                    *i = (*i + 1) % buf.len();
                    y += d;
                }
                y *= 0.25;
                for (buf, i) in aps.iter_mut() {
                    let d = buf[*i];
                    let v = y + d * 0.5;
                    buf[*i] = v;
                    *i = (*i + 1) % buf.len();
                    y = d - v * 0.5;
                }
                out[f * ch + c] = x + y * mix;
            }
        }
    }
}

/// Past 0.9 a sample is bent smoothly towards 1 instead of being cut off there.
pub(super) fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.9 {
        x
    } else {
        x.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh())
    }
}
