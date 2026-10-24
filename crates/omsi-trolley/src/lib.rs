//! Trolleybuses: the overhead wire, the poles that reach it, the traction behind them and the
//! arcs where contact breaks. Pure state and physics; the game draws and sounds what this
//! says (`omsi-sim` runs it on a vehicle, `omsi-scene` shows it).

pub mod def;
pub mod panto;
pub mod pole;
pub mod rig;
pub mod traction;
pub mod wire;

pub use def::*;
pub use panto::*;
pub use pole::*;
pub use rig::*;
pub use traction::*;
pub use wire::*;
