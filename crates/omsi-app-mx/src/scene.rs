//! The scene built from the map and its vehicles is omsi-scene's; here it is at its old
//! paths, with the vehicles' per-frame view sync (`view_sync`) beside it as before.

pub(crate) use omsi_scene::*;
// (the vehicles' per-frame view sync lives in `view_sync`; at its old paths here)
pub(crate) use crate::view_sync::vehicle::*;
pub(crate) use crate::view_sync::vehicle_damage::*;
