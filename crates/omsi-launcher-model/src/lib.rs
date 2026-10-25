//! The launcher's model, without a window: what a launcher knows and has chosen (`state`),
//! the background work that fills it, and the chosen map as data (`mapview`). The launchers
//! that draw it are elsewhere: the desktop launcher (`launcher/`, a Tauri window over
//! `omsi-launcher-core`) and the Swift one on Apple's systems, which works on this very
//! `state::State` through the game's `apple_bridge`.
//!
//! What only the game program has (the interface's language, machine translation, the city
//! map read the way the navigator reads it) the model asks through `host::Host`, which the
//! program installs before it makes one.

pub mod host;
pub mod mapview;
mod route_line;
mod season;
pub mod state;
pub use state::crash_of;
