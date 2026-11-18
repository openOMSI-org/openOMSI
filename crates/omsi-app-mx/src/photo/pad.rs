//! A gamepad in the photo mode, laid out as the racing games' photo modes have it: the left
//! stick moves, the right one looks, the triggers go down and up, the shoulder buttons roll,
//! the d-pad zooms; A takes the photo, Y hides the panel, B leaves, X sets the roll level.

use gilrs::{Axis, Button};
use glam::Vec3;

use super::Request;

#[derive(Default)]
pub(crate) struct PadState {
    /// Right, forward, up (-1 .. 1).
    pub moving: Vec3,
    /// Right, down.
    pub look: [f32; 2],
    pub roll: f32,
    pub zoom: f32,
    /// The buttons held last frame (south, east, north, west).
    held: [bool; 4],
    /// The pad the camera listens to, and when that was last settled (see `read`).
    pad: Option<gilrs::GamepadId>,
    chosen: Option<std::time::Instant>,
}

fn dead(x: f32) -> f32 {
    let d = 0.15;
    if x.abs() < d { 0.0 } else { (x - d * x.signum()) / (1.0 - d) }
}

impl PadState {
    pub(crate) fn moves(&self) -> Vec3 {
        self.moving
    }

    /// The gamepad's sticks and buttons this frame (a wheel is none); the buttons pressed now as
    /// requests.
    pub(crate) fn read(&mut self, ctl: &crate::controllers::Controllers) -> Vec<Request> {
        let mut out = Vec::new();
        let Some(g) = ctl.devices.gilrs.as_ref() else { return out };
        // A gamepad only - not a wheel, pedals or a joystick, nor anything that steers the bus,
        // whatever layout the system gives it: the devices' own sorting (`Connected::gamepad`)
        // and the controller setup say which. Settled every two seconds (listing the devices
        // is not for every frame), so a pad plugged in meanwhile is found.
        let now = std::time::Instant::now();
        if self.chosen.is_none_or(|t| now.duration_since(t).as_secs_f32() > 2.0) {
            self.chosen = Some(now);
            let devices = ctl.connected();
            self.pad = g
                .gamepads()
                .find(|(_, p)| {
                    p.is_connected()
                        && p.mapping_source() != gilrs::MappingSource::None
                        && !ctl.steers(p.name())
                        && devices.iter().any(|c| c.gamepad && crate::controllers::names_match(&c.name, p.name()))
                })
                .map(|(id, _)| id);
        }
        let Some(pad) = self.pad.and_then(|id| g.connected_gamepad(id)) else {
            (self.moving, self.look, self.roll, self.zoom, self.held) = (Vec3::ZERO, [0.0; 2], 0.0, 0.0, [false; 4]);
            return out;
        };
        let trig = |b: Button| pad.button_data(b).map(|d| d.value()).unwrap_or(0.0);
        self.moving = Vec3::new(dead(pad.value(Axis::LeftStickX)), dead(pad.value(Axis::LeftStickY)), trig(Button::RightTrigger2) - trig(Button::LeftTrigger2));
        self.look = [dead(pad.value(Axis::RightStickX)) * 1.6, -dead(pad.value(Axis::RightStickY)) * 1.6];
        self.roll = pad.is_pressed(Button::RightTrigger) as i32 as f32 - pad.is_pressed(Button::LeftTrigger) as i32 as f32;
        self.zoom = pad.is_pressed(Button::DPadUp) as i32 as f32 - pad.is_pressed(Button::DPadDown) as i32 as f32;
        let now = [pad.is_pressed(Button::South), pad.is_pressed(Button::East), pad.is_pressed(Button::North), pad.is_pressed(Button::West)];
        for (k, req) in [Request::Take, Request::Exit, Request::HideUi, Request::Reset].into_iter().enumerate() {
            if now[k] && !self.held[k] {
                // (X: the horizon level again, not every setting back)
                out.push(if k == 3 { Request::Level } else { req });
            }
        }
        self.held = now;
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_sticks_have_a_dead_zone_and_reach_full() {
        assert_eq!(super::dead(0.1), 0.0);
        assert!((super::dead(1.0) - 1.0).abs() < 1e-6);
        assert!((super::dead(-1.0) + 1.0).abs() < 1e-6);
    }
}
