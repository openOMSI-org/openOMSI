//! openOMSI's input devices: game controllers (gilrs, DirectInput on Windows, evdev on Linux,
//! the HID and GameController frameworks on macOS), their mappings (`gamectrler.cfg`),
//! force feedback, head tracking (TrackIR / FreeTrack), the keyboard's scan codes and the
//! names of the cockpit's controls. Nothing here knows the game's `App` or the renderer:
//! the application reads these devices each frame and applies what they say.

pub mod actions;
pub mod auto_shift;
pub mod auto_setup;
pub mod background;
pub mod controllers;
pub mod describe;
pub mod ffb_calibration;
pub mod gamepad_profile;
pub mod head_filter;
pub mod headtrack;
pub mod hpattern;
pub mod keys;
pub mod launcher_pads;
pub mod pedal;
pub mod stock_keys;
pub mod diagnostics;
pub mod tobii;

#[cfg(windows)]
mod dinput;
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
mod evdev_buttons;
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
mod evdev_ff;
#[cfg(target_os = "macos")]
mod mac_game_controller;
#[cfg(target_os = "macos")]
mod mac_hid;
