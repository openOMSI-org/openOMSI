//! What the player is told about bus files going either way: the loading screen's line
//! and the cards over the HUD.

use super::*;

/// What the player is told about the bus files going either way: one line, how far
/// (bytes), and whether it is still going on.
struct Shown {
    seq: u64,
    text: String,
    done: u64,
    total: u64,
    busy: bool,
}

/// The line about the bus files going either way, and the card shown for it last. The game
/// has one (`notices`, behind this module's functions).
pub(in crate::mods) struct Notices {
    shown: Mutex<Option<Shown>>,
    /// Numbers each new line (from 1: 0 is the card before any was shown).
    seq: std::sync::atomic::AtomicU64,
    /// (number, text) of the line shown as a card last.
    last: Mutex<(u64, String)>,
}

impl Default for Notices {
    fn default() -> Notices {
        Notices {
            shown: Mutex::new(None),
            seq: std::sync::atomic::AtomicU64::new(1),
            last: Mutex::new((0, String::new())),
        }
    }
}

static NOTICES: std::sync::LazyLock<Notices> = std::sync::LazyLock::new(Notices::default);

/// The game's own `Notices`.
pub(in crate::mods) fn notices() -> &'static Notices {
    &NOTICES
}

pub(in crate::mods) fn show(text: impl Into<String>, done: u64, total: u64, busy: bool) {
    notices().show(text, done, total, busy);
}

pub(in crate::mods) fn mb(b: u64) -> String {
    format!(
        "{:.0} MB",
        (b as f64 / 1e6).max(if b > 0 { 1.0 } else { 0.0 })
    )
}

/// While the start waits for our bus to be sent: the loading screen's line and how far.
pub fn loading() -> Option<(String, f32)> {
    let started = sender().waiting_since()?;
    if started.elapsed() > START_WAIT {
        return None;
    }
    notices().line()
}

/// How long a card over the HUD stays (s): while what it tells goes on (each new line of it
/// takes the card's place), and once it is done.
const CARD_GOING_ON: f32 = 600.0;
const CARD_DONE: f32 = 8.0;

/// Once a frame: the line about our bus or the others' for a card over the HUD, when there
/// is a new one - (the text of the card before, to take down; the new text; how long the
/// card stays, s: while it goes on, its text following the progress). None while the line
/// is the one shown already, or while the loading screen says it.
pub fn notice() -> Option<(String, String, f32)> {
    notices()
        .next_card(loading().is_some())
        .map(|(before, text, busy)| (before, text, if busy { CARD_GOING_ON } else { CARD_DONE }))
}

impl Notices {
    pub(in crate::mods) fn show(&self, text: impl Into<String>, done: u64, total: u64, busy: bool) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        *self.shown.lock().unwrap_or_else(|e| e.into_inner()) = Some(Shown {
            seq,
            text: text.into(),
            done,
            total,
            busy,
        });
    }

    /// No line any more (a host that takes no uploads: nothing to tell about it).
    pub(super) fn clear(&self) {
        *self.shown.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The line, and how far it has come (0..1).
    pub(super) fn line(&self) -> Option<(String, f32)> {
        let s = self.shown.lock().unwrap_or_else(|e| e.into_inner());
        let s = s.as_ref()?;
        let p = if s.total > 0 {
            s.done as f32 / s.total as f32
        } else {
            0.0
        };
        Some((s.text.clone(), p))
    }

    /// The line for a card, when it is a new one (see `notice`); `loading`: the loading
    /// screen says it now.
    pub(super) fn next_card(&self, loading: bool) -> Option<(String, String, bool)> {
        let (seq, text, busy) = self
            .shown
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| (s.seq, s.text.clone(), s.busy))?;
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        // (under the loading screen the screen says it)
        if last.0 == seq || (busy && loading) {
            return None;
        }
        let before = std::mem::replace(&mut last.1, text.clone());
        last.0 = seq;
        Some((before, text, busy))
    }

    /// How far the line has come: (bytes done, bytes in all).
    #[cfg(test)]
    pub(super) fn progress(&self) -> Option<(u64, u64)> {
        self.shown
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| (s.done, s.total))
    }
}
