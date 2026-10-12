//! openOMSI's multiplayer on top of the transport (`omsi-net`): what of a session needs no
//! game world - the host's mods for the players who join (`mods`), the voice chat's link
//! to the GreenTeaSpeak plugin (`voice`), the WebSocket gateway, tunnel and status page
//! (`gateway`), what of a vehicle goes over the network beyond its pose (`sync`), where a
//! joining bus may stand (`spawn`), a client's tracks of the host's world (`mirror`), a
//! dedicated server's `server.cfg` and administration (`server_cfg`, `admin`) and the
//! session's status file for the launcher. The game's side (the remote buses drawn and
//! heard, the shared world, the chat on the HUD) stays in the app, which hands this crate
//! plain values.

pub mod admin;
pub mod gateway;
pub mod mirror;
pub mod mods;
pub mod remote;
pub mod server_cfg;
pub mod spawn;
pub mod sync;
pub mod voice;

use std::path::PathBuf;

/// `~/.openomsi`: the session's status file, the LAN mods' store, the voice chat's files.
pub fn data_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".openomsi"))
}

pub use omsi_net::LEFT_SERVER;

/// The status file of this game process.
pub fn status_path() -> Option<PathBuf> {
    let id = omsi_cfg::flags::OMSI_INSTANCE
        .var()
        .map(str::to_string)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .unwrap_or_else(|| std::process::id().to_string());
    Some(data_dir()?.join("lan").join(format!("{id}.json")))
}

/// Removes the status file when the game ends.
pub struct StatusFileGuard;

impl Drop for StatusFileGuard {
    fn drop(&mut self) {
        if let Some(p) = status_path() {
            let _ = std::fs::remove_file(p);
        }
    }
}
