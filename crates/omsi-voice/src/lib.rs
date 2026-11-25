//! Voice chat for multiplayer: the microphone, Opus, a jitter buffer, and the radio and
//! distance shaping of what is heard. The network side is `omsi_net::voice_net` (a relay
//! that never decodes); the game's side is the app's `voice_tick`.

pub mod capture;
pub mod codec;
pub mod dsp;
pub mod engine;
pub mod jitter;
pub mod mutes;

pub use engine::{
    Channel, Core, Frame, Mode, NewStream, Outgoing, Settings, Status, Talker, Voice,
};
