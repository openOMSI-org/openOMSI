//! What the game asks of the Swift app on an iPhone or an iPad: the functions the app
//! exports with `@_cdecl` (`apple/iOS/GameHost.swift`). UIKit's windows and scenes, the
//! motion sensor, the haptic engine, the clipboard and the browser are the app's; the game
//! only says when it needs them.

use std::ffi::{c_char, c_void, CString};
use std::sync::Arc;
use winit::window::Window;

unsafe extern "C" {
    /// The game's window (its view) goes onto the app's scene, in front of the launcher.
    fn openomsi_swift_attach_window(view: *mut c_void);
    /// The game's window leaves the scene (the launcher is in front again).
    fn openomsi_swift_detach_window(view: *mut c_void);
    /// A game is starting: the Swift launcher steps back (and stops its timers).
    fn openomsi_swift_game_started();
    fn openomsi_swift_tilt(on: i32);
    /// The latest turn of the device (-1 .. 1) into `out`; 0 while there is none yet.
    fn openomsi_swift_tilt_value(out: *mut f32) -> i32;
    fn openomsi_swift_haptic(ms: u32);
    fn openomsi_swift_set_clipboard(text: *const c_char) -> i32;
    fn openomsi_swift_is_phone() -> i32;
    /// The game window's safe area in pixels: left, top, right, bottom.
    fn openomsi_swift_safe_insets(out: *mut f32);
}

/// The UIView winit drew the window into.
fn view_of(window: &Window) -> Option<*mut c_void> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::UiKit(h) => Some(h.ui_view.as_ptr()),
        _ => None,
    }
}

pub(crate) fn attach_window(window: &Arc<Window>) {
    if let Some(v) = view_of(window) {
        // SAFETY: the view lives as long as the window, which outlives the call
        unsafe { openomsi_swift_attach_window(v) }
    }
}

pub(crate) fn detach_window(window: &Arc<Window>) {
    if let Some(v) = view_of(window) {
        // SAFETY: as in `attach_window`
        unsafe { openomsi_swift_detach_window(v) }
    }
}

pub(crate) fn game_started() {
    // SAFETY: a plain call into the app
    unsafe { openomsi_swift_game_started() }
}

pub(crate) fn set_tilt(on: bool) {
    // SAFETY: a plain call into the app
    unsafe { openomsi_swift_tilt(on as i32) }
}

pub(crate) fn tilt() -> Option<f32> {
    let mut v = 0.0f32;
    // SAFETY: `out` points at a live f32
    (unsafe { openomsi_swift_tilt_value(&mut v) } != 0).then_some(v)
}

pub(crate) fn buzz(ms: u32) {
    // SAFETY: a plain call into the app
    unsafe { openomsi_swift_haptic(ms) }
}

/// The text onto the system's clipboard (UIPasteboard).
pub(crate) fn set_clipboard(text: &str) -> bool {
    let Ok(t) = CString::new(text) else { return false };
    // SAFETY: a nul-terminated string the call only reads
    unsafe { openomsi_swift_set_clipboard(t.as_ptr()) != 0 }
}

pub(crate) fn safe_insets() -> (f32, f32, f32, f32) {
    let mut v = [0.0f32; 4];
    // SAFETY: four writable f32s
    unsafe { openomsi_swift_safe_insets(v.as_mut_ptr()) };
    (v[0], v[1], v[2], v[3])
}

pub(crate) fn is_phone() -> bool {
    unsafe { openomsi_swift_is_phone() != 0 }
}
