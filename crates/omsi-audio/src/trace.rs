//! `OMSI_AUDIO_TRACE=<file>`: what the game asks of the mixer, written as text, one line per
//! request - every voice started (its file, gain, pitch, loop, place, range, filter), every
//! change of its parameters, every stop, the listener once a frame, the radio's effects, the
//! blocks a recording mixes. Two builds that should sound alike write the same trace for the
//! same run: the trace is taken here, where the game hands its sounds to the mixer, so it
//! means the same whatever moved around above it (see `scripts/audio-trace.sh`).
//!
//! Numbers are rounded (gain and pitch to 1/10000, places to the centimetre) and a voice's
//! parameters are written only when they change at that precision, so the trace stays small
//! and does not move with the last bit of a float. An offscreen recording (`OMSI_RECORD`)
//! steps the mixer with the simulation, so its trace is the same from run to run; a window's
//! mixer runs on the wall clock, and only what is played (not when) compares there.
//!
//! Off (the variable unset), every hook is one load of a `OnceLock` and nothing else.

use glam::Vec3;
use hashbrown::HashMap;
use parking_lot::Mutex;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

use crate::mixer::{Listener, VoiceId, VoiceParams};

struct Trace {
    out: std::io::BufWriter<std::fs::File>,
    /// The file of every clip read, by the address of its data (a clip's `Arc` keeps it).
    files: HashMap<usize, String>,
    /// The parameters last written for each voice, as written.
    last: HashMap<VoiceId, String>,
    frame: u64,
}

fn trace() -> Option<&'static Mutex<Trace>> {
    static TRACE: OnceLock<Option<Mutex<Trace>>> = OnceLock::new();
    TRACE
        .get_or_init(|| {
            let path = omsi_cfg::flags::OMSI_AUDIO_TRACE.os()?;
            match std::fs::File::create(path) {
                Ok(f) => {
                    let mut out = std::io::BufWriter::new(f);
                    let _ = writeln!(out, "# openomsi audio trace 1");
                    Some(Mutex::new(Trace {
                        out,
                        files: HashMap::new(),
                        last: HashMap::new(),
                        frame: 0,
                    }))
                }
                Err(e) => {
                    log::warn!("OMSI_AUDIO_TRACE: {}: {e}", Path::new(path).display());
                    None
                }
            }
        })
        .as_ref()
}

/// Whether a trace is being written.
pub fn on() -> bool {
    trace().is_some()
}

fn place(p: Option<Vec3>) -> String {
    match p {
        Some(p) => format!("{:.2},{:.2},{:.2}", p.x, p.y, p.z),
        None => "-".into(),
    }
}

fn params(p: &VoiceParams) -> String {
    format!(
        "g={:.4} p={:.4} loop={} at={} r={:.2} lp={:.0} imp={} dop={}",
        p.gain,
        p.pitch,
        p.looping as u8,
        place(p.position),
        p.range,
        p.lowpass_hz,
        p.important as u8,
        p.doppler as u8
    )
}

/// An engine was made (`device`, `silent`, `capture`).
pub(crate) fn engine(kind: &str) {
    if let Some(t) = trace() {
        let _ = writeln!(t.lock().out, "E {kind}");
    }
}

/// A clip was read from `path` (its data at `key`).
pub(crate) fn clip_read(key: usize, path: &Path) {
    if let Some(t) = trace() {
        t.lock()
            .files
            .insert(key, path.to_string_lossy().replace('\\', "/"));
    }
}

/// Voice `id` started on the clip at `key` (or a stream, `None`) from frame `start`.
pub(crate) fn play(id: VoiceId, key: Option<usize>, p: &VoiceParams, start: f64) {
    if let Some(t) = trace() {
        let mut t = t.lock();
        let file = match key {
            Some(k) => t.files.get(&k).cloned().unwrap_or_else(|| "?".into()),
            None => "stream".into(),
        };
        let ps = params(p);
        let _ = writeln!(t.out, "+ {id} {file} {ps} from={start:.0}");
        t.last.insert(id, ps);
    }
}

/// New parameters for voice `id` (written when they differ, rounded, from the last ones).
pub(crate) fn set(id: VoiceId, p: &VoiceParams) {
    if let Some(t) = trace() {
        let mut t = t.lock();
        let ps = params(p);
        if t.last.get(&id) != Some(&ps) {
            let _ = writeln!(t.out, "~ {id} {ps}");
            t.last.insert(id, ps);
        }
    }
}

/// Voice `id` stopped by the game.
pub(crate) fn stop(id: VoiceId) {
    if let Some(t) = trace() {
        let mut t = t.lock();
        let _ = writeln!(t.out, "- {id}");
        t.last.remove(&id);
    }
}

/// Voices the mixer let go (played to their end, or stopped before).
pub(crate) fn ended(ids: &[VoiceId]) {
    if let Some(t) = trace() {
        let mut t = t.lock();
        for id in ids {
            let _ = writeln!(t.out, "x {id}");
            t.last.remove(id);
        }
    }
}

/// The radio's effects on stream voice `id`.
pub(crate) fn radio_fx(id: VoiceId, p: &crate::radio_fx::RadioFxParams) {
    if let Some(t) = trace() {
        let _ = writeln!(t.lock().out, "fx {id} {p:?}");
    }
}

/// The listener, given once a frame: starts the frame's lines.
pub(crate) fn listener(l: &Listener) {
    if let Some(t) = trace() {
        let mut t = t.lock();
        t.frame += 1;
        let n = t.frame;
        let _ = writeln!(
            t.out,
            "L {n} at={} fw={:.3},{:.3},{:.3} m={:.4} rv={:.3},{:.3}",
            place(Some(l.position)),
            l.forward.x,
            l.forward.y,
            l.forward.z,
            l.master,
            l.reverb_time,
            l.reverb_mix
        );
        let _ = t.out.flush();
    }
}

/// A recording mixed `frames` frames.
pub(crate) fn mixed(frames: usize) {
    if let Some(t) = trace() {
        let mut t = t.lock();
        let _ = writeln!(t.out, "M {frames}");
        let _ = t.out.flush();
    }
}
