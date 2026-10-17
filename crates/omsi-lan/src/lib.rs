//! openOMSI's multiplayer on top of the transport (`omsi-net`): what of a session needs no
//! game world - the host's mods for the players who join (`mods`), the voice chat's link
//! to the GreenTeaSpeak plugin (`voice`), the WebSocket gateway, tunnel and status page
//! (`gateway`), what of a vehicle goes over the network beyond its pose (`sync`), where a
//! joining bus may stand (`spawn`), a client's tracks of the host's world (`mirror`), a
//! dedicated server's `server.cfg` and administration (`server_cfg`, `admin`) and the
//! session's status file for the launcher. The session as the game plays it (`game`: our
//! bus sent, the other players' vehicles run, the host's world and clock followed, the
//! chat) takes the game's choices as a `config::LanConfig`, the frame's as a
//! `game::LanInput`, and asks the rest of the game through `host::LanHost`; how the others
//! are drawn and heard, and the shared world's mirror, stay in the app.

pub mod admin;
pub mod commands;
pub mod config;
pub mod game;
pub mod gateway;
pub mod host;
pub mod mirror;
pub mod map_pack;
pub mod mods;
pub mod passenger;
pub mod remote;
pub mod remote_load;
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

pub mod animation_sync;
pub mod variable_schema;

pub mod clock;
pub mod script_texture;
