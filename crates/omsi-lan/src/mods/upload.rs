//! Players' buses for everybody: a player whose bus the server does not have sends the
//! server the files that bus uses, the server keeps them sealed and passes them on with its
//! own mods (see the parent module), and nobody in the session sees a stand-in for it.
//!
//! What goes: the bus's manifest (`identity`) - every file its description names, found as
//! the game finds them (`bus_files`): the variant and the paint driven, the folders its texts
//! point into (`..\..\Anzeigen`, shared sounds), the fonts of its displays; not the whole
//! pack. Its fingerprint goes along in INFO.
//!
//! Nobody has to take the fingerprint a player announces on trust, so the server passes it
//! on as it is: a game that draws the bus checks the list it gets against that fingerprint
//! and every file against the list (`vehicles::want`). A player who announced another bus's
//! fingerprint is drawn as that other bus - its own doing; no file goes unchecked.
//!
//! The server takes what it lacks, without asking anybody (`accept_uploads` of
//! `server.cfg`): it checks every file as a joining player checks the host's (`refuse_path`,
//! size, hash, no program) and keeps the bus by its content (`vehicles`): every version
//! of it, each under the fingerprint the server works out itself, still there after a
//! restart. What it has already - in any bus - is never sent again.
//!
//! The other players get the bus by its fingerprint when they draw its player
//! (`vehicles::want`): their own copy when it is the same, else fetched from the server.
//!
//! The player is told: a joining game sends its bus under the loading screen (`begin`,
//! `loading`: what is being done, and how far), so that the others see the real bus from
//! its first metre; what goes or comes later is a card over the HUD (`notices`).
//!
//! On the mods connection, after its greeting (encrypted as the rest):
//!
//! ```text
//! UPLOAD-PREFLIGHT                                     → OK | ERR what (older host)
//! OFFER3 <len>         + manifest and descriptor bytes → verified piece plan | ERR
//! OFFER <len>          + JSON VehicleManifest            → OK <len> + JSON [wanted indices] | ERR
//! PUT <index> <len>    + the bytes                      → OK | ERR
//! DONE                                                  → OK <files that came> | ERR
//! MANIFEST <fp>                                         → OK <len> + JSON VehicleManifest | ERR
//! BLOB <sha256>                                         → OK <len> + the bytes | ERR
//! ```

use super::identity::VehicleManifest;
use super::Entry;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};


/// A modern sender proves that every descriptor in its offer permits transfers before
/// the host requests assets. Order is the descriptor order in the validated manifest.
#[derive(serde::Serialize, serde::Deserialize)]
struct UploadPreflight {
    manifest: VehicleManifest,
    descriptors: Vec<Vec<u8>>,
}

pub(super) const MAX_DESCRIPTOR_BYTES: u64 = 4 << 20;

mod notices;
mod own_bus;
mod requests;
mod sending;
mod server;
#[cfg(test)]
mod tests;

// (each part's items, as reachable as they were when this was one file)
pub use notices::*;
pub use own_bus::*;
pub(in crate::mods) use requests::*;
pub use sending::*;
pub use server::*;
