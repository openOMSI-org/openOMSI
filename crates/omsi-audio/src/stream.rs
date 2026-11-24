//! A buffer of sound that is filled while it plays: the decoded internet radio. A reader
//! thread pushes stereo frames at the stream's own rate; the mixer takes them at the
//! device rate (see `AudioEngine::play_stream`). The frames go through a lock-free ring:
//! the mixer never waits for the reader (a reader holding a lock while it decoded was a
//! block mixed late), and the reader waits only when the ring is full.

use crate::spsc;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Frames the ring holds: the reader keeps some 8 seconds ahead (`radio::AHEAD`), at
/// 48 kHz well inside.
const FRAMES: usize = 1 << 19;

pub struct StreamBuf {
    /// The reader's end of the ring.
    feed: Mutex<spsc::Producer<[f32; 2]>>,
    /// The mixer's end, until the voice that plays the stream takes it.
    tap: Mutex<Option<StreamInner>>,
    /// Frames per second of what is in the buffer.
    rate: AtomicU32,
    /// Frames ever pushed, ever taken out by the mixer, and the count of pushed frames
    /// below which the mixer drops them unplayed (a new rate starts the buffer over).
    pushed: AtomicU64,
    taken: AtomicU64,
    flush_to: AtomicU64,
    /// The mixer plays it (it does not wait for the buffer to fill).
    playing: AtomicBool,
    closed: AtomicBool,
    /// What the reader says about the stream: the song playing, or why it is silent.
    status: Mutex<String>,
    /// What the station says of itself: its name (`icy-name`) and the title playing now
    /// (`StreamTitle`), each empty while it says nothing.
    meta: Mutex<(String, String)>,
}

/// The mixer's end of a stream: owned by the voice that plays it.
pub struct StreamInner {
    frames: spsc::Consumer<[f32; 2]>,
    /// Frames per second of what is in the buffer.
    pub rate: u32,
    /// Playing (else filling up: at the start and after the network fell behind).
    playing: bool,
    /// How much is gathered before playing starts again after a stall, in seconds: a
    /// connection that stalled once tends to stall again, so each stall asks for more.
    prebuffer: f32,
    /// How much more each stall asks for, and the most it ever asks for (seconds).
    step: f32,
    limit: f32,
    /// The voice ends (the radio was switched off or to another station).
    pub closed: bool,
    /// Frames taken out, and the flush mark last acted on (see `StreamBuf::flush_to`).
    taken: u64,
    flushed: u64,
}

impl StreamInner {
    /// The reader's news: closed, a new rate (the frames before it dropped).
    pub(crate) fn sync(&mut self, buf: &StreamBuf) {
        self.closed = buf.closed.load(Ordering::Acquire);
        let flush = buf.flush_to.load(Ordering::Acquire);
        if flush != self.flushed {
            self.flushed = flush;
            self.playing = false;
            while self.taken < flush && self.frames.pop().is_some() {
                self.taken += 1;
            }
        }
        self.rate = buf.rate.load(Ordering::Acquire);
    }

    /// What the mixer did, for the reader and the game to see.
    pub(crate) fn publish(&self, buf: &StreamBuf) {
        buf.taken.store(self.taken, Ordering::Release);
        buf.playing.store(self.playing, Ordering::Relaxed);
    }

    /// The frame playing now and the next one, while there are both.
    pub(crate) fn pair(&mut self) -> Option<([f32; 2], [f32; 2])> {
        if !self.playing {
            if (self.frames.len() as f32) < self.rate as f32 * self.prebuffer {
                return None;
            }
            self.playing = true;
        }
        match (self.frames.get(0).copied(), self.frames.get(1).copied()) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => {
                self.playing = false;
                self.prebuffer = (self.prebuffer + self.step).min(self.limit);
                None
            }
        }
    }

    pub(crate) fn advance(&mut self) {
        if self.frames.pop().is_some() {
            self.taken += 1;
        }
    }
}

impl Default for StreamBuf {
    fn default() -> Self {
        Self::with_buffering(FRAMES, 1.5, 1.0, 6.0)
    }
}

impl StreamBuf {
    /// A buffer for live voice: a ring of `capacity` frames, playing starts once `prebuffer`
    /// seconds are gathered, and a stall asks for `step` seconds more (never above `limit`) -
    /// the radio's defaults (1.5 s, 1 s, 6 s) are far too slow for a conversation.
    pub fn with_buffering(capacity: usize, prebuffer: f32, step: f32, limit: f32) -> Self {
        let (tx, rx) = spsc::channel(capacity);
        StreamBuf {
            feed: Mutex::new(tx),
            tap: Mutex::new(Some(StreamInner {
                frames: rx,
                rate: 44100,
                playing: false,
                prebuffer,
                step,
                limit,
                closed: false,
                taken: 0,
                flushed: 0,
            })),
            rate: AtomicU32::new(44100),
            pushed: AtomicU64::new(0),
            taken: AtomicU64::new(0),
            flush_to: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            status: Mutex::new(String::new()),
            meta: Mutex::new((String::new(), String::new())),
        }
    }

    /// The mixer's end, for the voice that plays it (once).
    pub(crate) fn take_tap(&self) -> Option<StreamInner> {
        self.tap.lock().take()
    }

    /// Add decoded frames at `rate`. A new rate (another station, a stream that changed
    /// format) starts the buffer over. Waits while the buffer is full (unless it is closed).
    pub fn push(&self, rate: u32, frames: impl IntoIterator<Item = [f32; 2]>) {
        let mut tx = self.feed.lock();
        if self.rate.load(Ordering::Relaxed) != rate {
            self.rate.store(rate, Ordering::Release);
            self.flush_to
                .store(self.pushed.load(Ordering::Relaxed), Ordering::Release);
        }
        for f in frames {
            let mut f = f;
            while let Err(back) = tx.push(f) {
                if self.is_closed() {
                    return;
                }
                f = back;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            self.pushed.fetch_add(1, Ordering::Release);
        }
    }

    /// Seconds of sound waiting to be played.
    pub fn buffered(&self) -> f32 {
        if self.is_closed() {
            return 0.0;
        }
        let from = self
            .taken
            .load(Ordering::Acquire)
            .max(self.flush_to.load(Ordering::Acquire));
        self.pushed.load(Ordering::Acquire).saturating_sub(from) as f32
            / self.rate.load(Ordering::Relaxed).max(1) as f32
    }

    /// Whether the mixer is playing it (not waiting for the buffer to fill).
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub fn set_status(&self, s: impl Into<String>) {
        *self.status.lock() = s.into();
    }

    pub fn status(&self) -> String {
        self.status.lock().clone()
    }

    /// The station's name and the title playing (see `meta`).
    pub fn set_meta(&self, name: &str, title: &str) {
        let mut m = self.meta.lock();
        if m.0 != name || m.1 != title {
            *m = (name.to_string(), title.to_string());
        }
    }

    /// The station's name as it gives it (empty when it gives none).
    pub fn station_name(&self) -> String {
        self.meta.lock().0.clone()
    }

    /// The title playing now as the station gives it (empty when it gives none).
    pub fn title(&self) -> String {
        self.meta.lock().1.clone()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new rate drops what was buffered at the old one; the mixer's end sees it.
    #[test]
    fn a_new_rate_starts_the_buffer_over() {
        let b = StreamBuf::default();
        b.push(44_100, (0..1000).map(|i| [i as f32, 0.0]));
        assert!((b.buffered() - 1000.0 / 44_100.0).abs() < 1e-6);
        b.push(48_000, (0..10).map(|i| [100.0 + i as f32, 0.0]));
        assert!((b.buffered() - 10.0 / 48_000.0).abs() < 1e-6);
        let mut tap = b.take_tap().unwrap();
        assert!(b.take_tap().is_none());
        tap.sync(&b);
        tap.prebuffer = 0.0;
        assert_eq!(
            (tap.rate, tap.pair().map(|p| p.0[0])),
            (48_000, Some(100.0))
        );
        tap.advance();
        tap.publish(&b);
        assert!((b.buffered() - 9.0 / 48_000.0).abs() < 1e-6 && b.is_playing());
        b.close();
        tap.sync(&b);
        assert!(tap.closed && b.buffered() == 0.0);
    }
}
