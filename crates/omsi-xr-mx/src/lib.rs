//! openBus in a PC VR headset: the OpenXR session on Windows (Direct3D 12 swapchains shared
//! with the renderer's mX device, the desktop mirror) and the cockpit navigator's placement
//! in the headset. Nothing here knows the game's `App`: the app opens a session, hands it the
//! scene to draw each frame and edits the placements.

pub mod navigator;
#[cfg(windows)]
pub mod openxr;
