//! Other players' buses read off the frame. Reading a bus - its configuration, meshes and
//! scripts (`omsi_sim::VehicleType::load`) - and its paint's textures took the thread that
//! draws a good part of a second for a heavy coach: fifteen players coming one after the
//! other were fifteen frozen frames in a row. Here they are read on a thread of their own,
//! two at a time, the paint's textures and meshes put on the GPU there too (`lend_prefetcher`,
//! as the AI fleet's are), while a box of the bus's size stands in for it
//! (`mods::placeholder`); the bus takes the box's place once all of it is here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use omsi_sim::VehicleType;

/// What reads a bus's paint ahead (its textures and meshes, made on the GPU from a worker):
/// the app's renderer lends one (`lend_prefetcher`); without one (a server) nothing is.
pub type Prefetcher = Arc<dyn Fn(&VehicleType, Option<usize>) + Send + Sync>;

static PREFETCHER: Mutex<Option<Prefetcher>> = Mutex::new(None);

/// The renderer's reader of vehicle sets lent for this session: until the guard is dropped
/// (the session's world gone), when it is given back.
pub fn lend_prefetcher(p: Prefetcher) -> Lent {
    *PREFETCHER.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
    Lent(())
}

/// The reader of `lend_prefetcher`, lent while this lives.
pub struct Lent(());

impl Drop for Lent {
    fn drop(&mut self) {
        *PREFETCHER.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

pub fn has_prefetcher() -> bool {
    PREFETCHER.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// Buses read at once, at most (each holds its files, and a coach's paints are pictures of
/// tens of MB).
const AT_ONCE: usize = 2;
/// A bus no player drives any more is let go after this long.
const KEEP: Duration = Duration::from_secs(120);
/// One that could not be read is tried again after this long.
pub const RETRY: Duration = Duration::from_secs(60);

enum State {
    Reading,
    /// The bus, what reading it read (`mods::vehicles::audit`), and when it was last asked for.
    Ready(Arc<VehicleType>, Vec<PathBuf>, Instant),
    Failed(String, Instant),
}

/// By (bus file, paint).
static BUSES: Mutex<Option<HashMap<(PathBuf, String), State>>> = Mutex::new(None);
static READING: AtomicUsize = AtomicUsize::new(0);
/// Counts the buses that became ready: a box that waits for one is made again when it changes.
static READY: AtomicU64 = AtomicU64::new(0);

/// Where the bus `path` in `paint` stands.
pub enum Got {
    /// Read: the bus, and what reading it read.
    Ready(Arc<VehicleType>, Vec<PathBuf>),
    /// Being read (or waiting for its turn): a box stands for it meanwhile.
    Reading,
    Failed(String),
}

/// Is the bus `path` in `paint` being read now (on its thread; not waiting for its turn)?
pub fn being_read(path: &Path, paint: &str) -> bool {
    let key = (path.to_path_buf(), paint.to_ascii_lowercase());
    matches!(BUSES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&key)), Some(State::Reading))
}

/// The count of buses that became ready (see `READY`).
pub fn generation() -> u64 {
    READY.load(Ordering::Acquire)
}

/// The bus `path` (under the game's `root`) in `paint`: read already, or started now.
pub fn get(root: &Path, path: &Path, paint: &str) -> Got {
    let key = (path.to_path_buf(), paint.to_ascii_lowercase());
    let mut g = BUSES.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    // (the buses nobody drives any more go; one that failed is tried again)
    map.retain(|_, s| match s {
        State::Ready(ty, _, at) => Arc::strong_count(ty) > 1 || at.elapsed() < KEEP,
        State::Failed(_, at) => at.elapsed() < RETRY,
        State::Reading => true,
    });
    match map.get_mut(&key) {
        Some(State::Ready(ty, reads, at)) => {
            *at = Instant::now();
            return Got::Ready(ty.clone(), reads.clone());
        }
        Some(State::Reading) => return Got::Reading,
        Some(State::Failed(why, _)) => return Got::Failed(why.clone()),
        None => {}
    }
    // (its turn comes when one of the others is done)
    if READING.load(Ordering::Acquire) >= AT_ONCE {
        return Got::Reading;
    }
    READING.fetch_add(1, Ordering::AcqRel);
    map.insert(key.clone(), State::Reading);
    drop(g);
    let (root_buf, paint_s) = (root.to_path_buf(), paint.to_string());
    let spawned = std::thread::Builder::new().name("remote-bus".into()).spawn(move || {
        let state = read(&root_buf, &key.0, &paint_s);
        BUSES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, state);
        READING.fetch_sub(1, Ordering::AcqRel);
        READY.fetch_add(1, Ordering::AcqRel);
    });
    if let Err(e) = spawned {
        READING.fetch_sub(1, Ordering::AcqRel);
        BUSES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert((path.to_path_buf(), paint.to_ascii_lowercase()), State::Failed(format!("no thread: {e}"), Instant::now()));
    }
    Got::Reading
}

/// Read the bus and its paint (on the bus's own thread).
fn read(root: &Path, path: &Path, paint: &str) -> State {
    let t0 = Instant::now();
    // (what reading it reads: a fetched bus should read nothing but its own files)
    let audit = omsi_cfg::vfs::Audit::begin();
    // (read under a guard: a file that would stop the reader makes the bus a box that says
    // so, not the end of this game)
    let loaded = std::panic::catch_unwind(|| VehicleType::load(root, path)).map_err(|_| "its files could not be read".to_string()).and_then(|r| r.map_err(|e| e.to_string()));
    let reads = audit.end();
    let ty = match loaded {
        Ok(t) => Arc::new(t),
        Err(why) => return State::Failed(why, Instant::now()),
    };
    let t1 = Instant::now();
    let scheme = if paint.is_empty() { None } else { ty.paint_schemes.iter().position(|s| s.name.eq_ignore_ascii_case(paint)) };
    let prefetcher = PREFETCHER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(p) = prefetcher {
        // (a picture that would stop the reader leaves it to the frame's upload, as before)
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p(&ty, scheme)));
    }
    log::info!(
        "LAN: {} read off the frame in {:.0} ms, its paint in {:.0} ms",
        path.file_name().unwrap_or_default().to_string_lossy(),
        (t1 - t0).as_secs_f64() * 1000.0,
        t1.elapsed().as_secs_f64() * 1000.0
    );
    State::Ready(ty, reads, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bus_is_read_off_the_frame_and_then_handed_out() {
        // (no such bus: it is read on its thread all the same, and fails there)
        let root = std::env::temp_dir().join(format!("openomsi-remote-load-{}", std::process::id()));
        let path = root.join("Vehicles/None/none.bus");
        let before = generation();
        assert!(matches!(get(&root, &path, ""), Got::Reading));
        let t0 = Instant::now();
        while generation() == before && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(generation() > before, "the box is told the bus is done");
        assert!(matches!(get(&root, &path, ""), Got::Failed(_)), "a bus that cannot be read says why");
    }
}
