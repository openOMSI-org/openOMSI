//! Says in the log where a frame is stuck.
//!
//! Reports of a game that "stays at 90 % of the loading screen" came with logs that simply stop:
//! nothing said which part of the frame was running. The frame runner (`frame_stages::run`)
//! tells this module which stage it enters and when it is done; a thread looks once a second
//! and, when one stage has run for ten seconds, writes its name to the log (and again every
//! thirty seconds while it lasts). It changes nothing: a stage that is only slow ends by
//! itself and the log says how long it took.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

/// The stage now running, with when it began (None between frames).
static CURRENT: Mutex<Option<(&'static str, Instant)>> = Mutex::new(None);
/// Counts the stages entered: tells a stage that went on from a new one of the same name.
static ENTERED: AtomicU64 = AtomicU64::new(0);
static STARTED: Once = Once::new();
static SHUT: AtomicBool = AtomicBool::new(false);

const FIRST: Duration = Duration::from_secs(10);
const AGAIN: Duration = Duration::from_secs(30);

/// A frame stage begins.
pub(crate) fn enter(stage: &'static str) {
    STARTED.call_once(spawn);
    ENTERED.fetch_add(1, Ordering::Relaxed);
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some((stage, Instant::now()));
}

/// The frame is over (or ends early): nothing is running that could be stuck.
pub(crate) fn idle() {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Held by the frame runner: the frame's end, however it ends, goes through `Drop`.
pub(crate) struct Frame;

impl Drop for Frame {
    fn drop(&mut self) {
        idle();
    }
}

fn spawn() {
    let _ = std::thread::Builder::new().name("watchdog".into()).spawn(|| {
        // (the stage and its count last reported, and when)
        let mut reported: Option<(u64, Instant)> = None;
        while !SHUT.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            let now = *CURRENT.lock().unwrap_or_else(|e| e.into_inner());
            let id = ENTERED.load(Ordering::Relaxed);
            let Some((stage, since)) = now else {
                reported = None;
                continue;
            };
            let ran = since.elapsed();
            let due = match reported {
                Some((same, at)) if same == id => at.elapsed() >= AGAIN,
                _ => ran >= FIRST,
            };
            if due && ran >= FIRST {
                log::warn!("watchdog: the frame's stage '{stage}' has been running for {:.0} s (a long load, or stuck)", ran.as_secs_f32());
                reported = Some((id, Instant::now()));
            }
        }
    });
}
