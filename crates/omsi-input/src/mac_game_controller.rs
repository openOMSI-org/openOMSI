//! Xbox-type controllers on macOS. Apple's own driver takes the whole USB device over
//! (`ioreg`: `"UsbExclusiveOwner" = "XboxUSBDevice"`) and leaves only a pass-through HID
//! device whose report sits entirely on a vendor-defined usage page - gilrs, and the raw
//! HID elements `mac_hid` reads, find no axis or button on it at all. The GameController
//! framework has Apple's own knowledge of that report, and is the only way to read it.
//! DualSense and DualShock controllers already read fine through gilrs (`controllers.rs`'s
//! `same_hid_device`), so only the Xbox product category is read here, to not create a
//! second, conflicting device for a pad already working.
//!
//! The sticks' and triggers' `value()` read back a plausible number polled cold, straight
//! off the extended gamepad - but a different, wrong one on every poll, the pad sitting
//! untouched (a hardware or IOKit-adjacent timing quirk of this profile, not of ours -
//! `isPressed()` reads buttons fine the same way). The one working path is each element's
//! own `valueChangedHandler` (see `register_axis_handlers`): the values it hands over are
//! the real, current ones. So the handler is the only place axes are ever read; everywhere
//! else reads a cache it fills in.

use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_game_controller::{
    GCController, GCControllerButtonInput, GCControllerDirectionPad, GCDevice, GCExtendedGamepad,
    GCProductCategoryXboxOne,
};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

/// Buttons 0..12 as the extended gamepad profile names them, then the D-pad's four
/// directions as the other hats on this platform are numbered (`controllers::HAT_BUTTONS`).
const BUTTONS: usize = 16;

fn button_index(i: usize) -> usize {
    if i < 12 {
        i
    } else {
        crate::controllers::HAT_BUTTONS + (i - 12)
    }
}

fn read_buttons(pad: &GCExtendedGamepad, dpad: &GCControllerDirectionPad) -> [bool; BUTTONS] {
    unsafe {
        [
            pad.buttonA().isPressed(),
            pad.buttonB().isPressed(),
            pad.buttonX().isPressed(),
            pad.buttonY().isPressed(),
            pad.leftShoulder().isPressed(),
            pad.rightShoulder().isPressed(),
            pad.leftTrigger().isPressed(),
            pad.rightTrigger().isPressed(),
            pad.leftThumbstickButton().is_some_and(|b| b.isPressed()),
            pad.rightThumbstickButton().is_some_and(|b| b.isPressed()),
            pad.buttonMenu().isPressed(),
            pad.buttonOptions().is_some_and(|b| b.isPressed()),
            // the directions, as DirectInput's first hat (up, right, down, left)
            dpad.up().isPressed(),
            dpad.right().isPressed(),
            dpad.down().isPressed(),
            dpad.left().isPressed(),
        ]
    }
}

/// The sticks (0: left X, 1: left Y, 2: right X, 3: right Y) and triggers (4, 5), on -1..1
/// as every other device's axes are, in the generic axis assignment (Launcher → Controls).
/// Only meaningful called from within the profile's `valueChangedHandler` - see the module
/// doc comment.
fn read_axes(pad: &GCExtendedGamepad) -> [f32; 6] {
    unsafe {
        let (left, right) = (pad.leftThumbstick(), pad.rightThumbstick());
        [
            left.xAxis().value(),
            left.yAxis().value(),
            right.xAxis().value(),
            right.yAxis().value(),
            pad.leftTrigger().value() * 2.0 - 1.0,
            pad.rightTrigger().value() * 2.0 - 1.0,
        ]
    }
}

/// A handler on the whole profile only fires once for every element that changed together
/// in one report - a stick's x and y, say - but not more often than that: this pad's stick,
/// watched that way, read choppier than a DualSense's (a continuous HID poll, not a
/// handler). Each stick and trigger gets its own handler instead, firing on its own change
/// alone, which is what Apple hands to it anyway.
fn register_axis_handlers(gamepad: &GCExtendedGamepad, axes: &Arc<Mutex<[f32; 6]>>) {
    unsafe {
        let left = gamepad.leftThumbstick();
        let store = axes.clone();
        let handler = block2::RcBlock::new(
            move |_pad: NonNull<GCControllerDirectionPad>, x: f32, y: f32| {
                if let Ok(mut store) = store.lock() {
                    store[0] = x;
                    store[1] = y;
                }
            },
        );
        left.setValueChangedHandler(block2::RcBlock::as_ptr(&handler));

        let right = gamepad.rightThumbstick();
        let store = axes.clone();
        let handler = block2::RcBlock::new(
            move |_pad: NonNull<GCControllerDirectionPad>, x: f32, y: f32| {
                if let Ok(mut store) = store.lock() {
                    store[2] = x;
                    store[3] = y;
                }
            },
        );
        right.setValueChangedHandler(block2::RcBlock::as_ptr(&handler));

        let left_trigger = gamepad.leftTrigger();
        let store = axes.clone();
        let handler = block2::RcBlock::new(
            move |_button: NonNull<GCControllerButtonInput>, v: f32, _pressed: Bool| {
                if let Ok(mut store) = store.lock() {
                    store[4] = v * 2.0 - 1.0;
                }
            },
        );
        left_trigger.setValueChangedHandler(block2::RcBlock::as_ptr(&handler));

        let right_trigger = gamepad.rightTrigger();
        let store = axes.clone();
        let handler = block2::RcBlock::new(
            move |_button: NonNull<GCControllerButtonInput>, v: f32, _pressed: Bool| {
                if let Ok(mut store) = store.lock() {
                    store[5] = v * 2.0 - 1.0;
                }
            },
        );
        right_trigger.setValueChangedHandler(block2::RcBlock::as_ptr(&handler));
    }
}

struct Pad {
    controller: Retained<GCController>,
    gamepad: Retained<GCExtendedGamepad>,
    name: String,
    buttons: [bool; BUTTONS],
    /// Filled in by the profile's `valueChangedHandler` (see `read_axes`), not read here.
    axes: Arc<Mutex<[f32; 6]>>,
}

pub fn monitor_background(on: bool) {
    use objc2::ClassType;
    if GCController::class()
        .metaclass()
        .responds_to(objc2::sel!(setShouldMonitorBackgroundEvents:))
    {
        unsafe { GCController::setShouldMonitorBackgroundEvents(on) };
    }
}

pub struct GcPads {
    pads: Vec<Pad>,
    last_scan: Option<std::time::Instant>,
}

// (the controllers are only touched from the thread that polls the controllers; the
// handler's own `Arc<Mutex<_>>` is Send + Sync on its own)
unsafe impl Send for GcPads {}

impl GcPads {
    pub fn new() -> GcPads {
        GcPads {
            pads: Vec::new(),
            last_scan: None,
        }
    }

    /// Find the controllers again when the set of them changed (checked every two
    /// seconds, as `mac_hid` does).
    fn scan(&mut self) {
        if self
            .last_scan
            .is_some_and(|t| t.elapsed().as_secs_f32() < 2.0)
        {
            return;
        }
        self.last_scan = Some(std::time::Instant::now());
        let controllers = unsafe { GCController::controllers() };
        let mut pads = Vec::new();
        let mut xbox = 0;
        for controller in controllers.iter() {
            let category = unsafe { controller.productCategory() };
            let is_xbox = unsafe { GCProductCategoryXboxOne }.is_some_and(|c| *category == *c);
            log::info!(
                "GameController framework: {category} ({})",
                if is_xbox {
                    "read here"
                } else {
                    "left to gilrs"
                }
            );
            if !is_xbox {
                continue;
            }
            xbox += 1;
            // a controller already known keeps its exact pad, handlers and all: asking
            // the controller for its extended gamepad again, every rescan, handed back a
            // different object each time, and the handlers (registered on the first one)
            // stopped being told about it after a couple of seconds - read as the stick
            // turning jittery, not as it going quiet
            if let Some(i) = self.pads.iter().position(|p| *p.controller == *controller) {
                pads.push(self.pads.remove(i));
                continue;
            }
            let Some(gamepad) = (unsafe { controller.extendedGamepad() }) else {
                continue;
            };
            let name = if xbox == 1 {
                "Xbox Controller".to_string()
            } else {
                format!("Xbox Controller {xbox}")
            };
            let axes = Arc::new(Mutex::new(read_axes(&gamepad)));
            register_axis_handlers(&gamepad, &axes);
            pads.push(Pad {
                controller,
                gamepad,
                name,
                buttons: [false; BUTTONS],
                axes,
            });
        }
        self.pads = pads;
    }

    /// The buttons pressed (true) and let go since the last call, as `Devices::poll` wants
    /// them.
    pub fn poll(&mut self) -> Vec<(String, usize, bool)> {
        self.scan();
        let mut out = Vec::new();
        for pad in &mut self.pads {
            let dpad = unsafe { pad.gamepad.dpad() };
            let now = read_buttons(&pad.gamepad, &dpad);
            for i in 0..BUTTONS {
                if now[i] != pad.buttons[i] {
                    out.push((pad.name.clone(), button_index(i), now[i]));
                }
            }
            pad.buttons = now;
        }
        out
    }

    /// Every controller's name and axes, as the `valueChangedHandler` last filled them in.
    pub fn connected(&self) -> Vec<(String, Vec<(usize, f32)>)> {
        self.pads
            .iter()
            .map(|p| {
                let axes = p.axes.lock().map(|a| *a).unwrap_or_default();
                (p.name.clone(), axes.into_iter().enumerate().collect())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_dpad_direction_is_numbered_as_the_first_hat() {
        assert_eq!(super::button_index(0), 0);
        assert_eq!(super::button_index(11), 11);
        assert_eq!(super::button_index(12), crate::controllers::HAT_BUTTONS);
        assert_eq!(super::button_index(15), crate::controllers::HAT_BUTTONS + 3);
    }
}
