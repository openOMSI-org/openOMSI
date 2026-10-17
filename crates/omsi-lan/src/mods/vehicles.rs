//! Buses by their content (see `identity`): every version of every bus players sent, side by
//! side. A file is kept once, sealed in the store by its SHA-256 (`store`), however many
//! buses use it; a bus is its manifest, kept by its fingerprint in `vehicles/<fp>.json` beside
//! the store. Two players with two versions of the same mod are two manifests, and each is
//! served and mounted as itself - nothing is kept by its path, so no version takes the
//! place of another (what Git, Nix and an OCI registry do with their objects).
//!
//! A bus is mounted on its own (`mount`): its files under `lan-vehicles/<fp>`, at the paths of
//! its manifest, a sealed mount that is no content root - nothing of another bus or of the
//! machine's own mods is seen through it.

use super::identity::{self, VehicleManifest};
use super::pieces;
use super::store::Store;
use super::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

struct Vehicles {
    store: Store,
    /// `vehicles/` beside the store: one `<fp>.json` per bus.
    dir: PathBuf,
    /// By fingerprint, in the order they came (the oldest first).
    manifests: Vec<(String, VehicleManifest)>,
    /// The size of each kept file by its SHA-256 (what a blob is asked for by).
    sizes: HashMap<String, u64>,
    mounted: HashSet<String>,
    /// Files refused for what they are (`quarantine`), by their content and kind
    /// (`held_key`): why. A bus is kept and drawn without them (`withhold`).
    withheld: HashMap<String, String>,
    /// By fingerprint: the files a kept bus is without (`<fp>.withheld.json` beside its list).
    withheld_of: HashMap<String, Vec<Withheld>>,
}

/// A file a bus is kept and drawn without: its path in the bus's list, and why it was refused
/// (`quarantine`). One bad font costs a bus its destination sign, not the whole bus; the file
/// itself reaches nobody.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Withheld {
    pub path: String,
    pub why: String,
}

/// The files a host's list says its bus is without (the `withheld` beside the list in the
/// answer to `MANIFEST`; a host before them says none).
#[derive(Default, serde::Deserialize)]
struct HeldBack {
    #[serde(default)]
    withheld: Vec<Withheld>,
}

/// The buses kept here, and what goes with keeping them: the files served last, the buses on
/// their way, the hashes of this machine's own files, the files to pack at their smallest
/// and the files the uploads under way hold. The game has one (`buses`, behind this module's
/// functions); a test of the store alone makes its own.
pub(in crate::mods) struct Buses {
    /// None before the store is opened (`open`).
    kept: Mutex<Option<Vehicles>>,
    /// The files served last, unsealed (`served`): a file asked for piece by piece is opened
    /// once, not once a piece. At most `SERVED_BYTES` of them.
    served_last: Mutex<VecDeque<(String, Arc<Vec<u8>>)>>,
    /// One file is opened at a time to be served (`served`).
    opening: Mutex<()>,
    /// One file at a time is put together from its pieces (`finish`).
    finishing: Mutex<()>,
    /// A bus has been kept (`commit`): a count, and the waiters on it.
    arrived: (Mutex<u64>, Condvar),
    /// The fingerprints the session's players announce now (`announce`); None before the first.
    announced: Mutex<Option<HashSet<String>>>,
    /// One offer at a time is compared with this machine's own files (`adopt_local`).
    adopting: Mutex<()>,
    /// Hashes of this machine's own files (`local_sha`).
    local_shas: Mutex<LocalShas>,
    /// The files to pack at their smallest (`recompress_later`).
    recompressing: Mutex<recompress::Recompressing>,
    /// How long a bus nobody has used is kept (`keep_for_days`).
    keep: Mutex<std::time::Duration>,
    /// Files an upload under way holds (by SHA-256, how many uploads): a collection passes
    /// them by (`lease`).
    leases: Mutex<HashMap<String, u32>>,
}

impl Default for Buses {
    fn default() -> Self {
        Buses {
            kept: Mutex::default(),
            served_last: Mutex::default(),
            opening: Mutex::default(),
            finishing: Mutex::default(),
            arrived: Default::default(),
            announced: Mutex::default(),
            adopting: Mutex::default(),
            local_shas: Mutex::default(),
            recompressing: Mutex::default(),
            keep: Mutex::new(KEEP),
            leases: Mutex::default(),
        }
    }
}

static BUSES: std::sync::LazyLock<Buses> = std::sync::LazyLock::new(Buses::default);

/// The game's own `Buses`.
pub(in crate::mods) fn buses() -> &'static Buses {
    &BUSES
}

impl Buses {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Vehicles>> {
        self.kept.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Where the buses are mounted (no folder on the disk).
fn mounts() -> Option<PathBuf> {
    super::sandbox_base().map(|b| b.with_file_name("lan-vehicles"))
}

mod kept;
mod local_files;
mod recompress;
mod room;
mod viewer;

// (each part's items, as reachable as they were when this was one file)
pub(crate) use kept::*;
pub(in crate::mods) use local_files::*;
pub(in crate::mods) use recompress::*;
pub use room::*;
pub(crate) use viewer::*;
pub use viewer::SharedFonts;

#[cfg(test)]
pub(super) fn open_for_test(dir: &Path) {
    buses().open_in(dir).unwrap();
}

/// The tests that open the game's own store (`buses`, one per process) take turns; a test of
/// the store alone makes its own `Buses` and takes no turn.
#[cfg(test)]
pub(super) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests;

/// A stock bus of an OMSI 2 installation (`OMSI_ROOT`), kept and mounted as a fetched bus is:
/// loading it reads nothing but its own files, its textures are found among them, and its
/// displays' fonts too.
#[cfg(test)]
mod mounted_stock_bus;
