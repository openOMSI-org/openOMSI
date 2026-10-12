//! The window's frame (`WindowEvent::RedrawRequested`): its stages in their lists in `stages`
//! (once a frame, once per tick of the fixed simulation step, once a frame again: `ticked`),
//! taken by the runner of `frame_stages`, each a function of its own in the files beside this
//! one. What the offscreen run takes as well is in `steps`.

use super::*;
use crate::view_sync::{self, ViewSync};

mod controls;
mod drawn;
mod driving;
mod integrations;
mod interface;
mod people;
mod present;
mod road;
pub(crate) mod stages;
pub(crate) mod steps;
mod surroundings;
mod ticked;
mod timing;
mod view_camera;
mod view_keys;

/// The frame's time: when it started, the time since the last one, and that time as the
/// simulation steps it (at most a tenth of a second).
pub(super) struct FrameTime {
    pub(super) now: Instant,
    pub(super) raw_dt: f32,
    pub(super) dt: f32,
}

impl App {
    /// One frame of the window: its stages in their lists and ticks (see `ticked`).
    pub(super) fn frame(&mut self, event_loop: &ActiveEventLoop) {
        // (the window's stages do not fail)
        if let Err(e) = self.frame_ticked(event_loop) {
            log::error!("frame: {e:#}");
        }
    }
}

crate::app::stage_ctx!(
    /// The wheel surface stage's part of the game.
    WheelSurfaceCx { reads: [world], writes: [player] }
);

/// What the bus's scripts are told of the ground under its wheels.
pub(super) fn wheel_surface(cx: WheelSurfaceCx) {
    if let (Some(w), Some(p)) = (cx.world.world.as_deref(), cx.player.player.as_mut()) {
        crate::wheel_surface::tell_scripts(w, &mut p.vehicle);
    }
}

crate::app::stage_ctx!(
    /// The look trace's part of the game (its file is the run's own, as the logs are).
    TraceLookCx { reads: [input, view], writes: [perf] }
);

/// `OMSI_TRACE_LOOK=<csv>`: the frame's line (`frame`) with the cursor, the head's turn and the
/// camera drawn last.
pub(super) fn trace_look(cx: TraceLookCx) {
    let Some(path) = omsi_cfg::flags::OMSI_TRACE_LOOK.var() else { return };
    use std::io::Write;
    static FILE: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);
    let mut g = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = std::fs::File::create(path).ok();
        if let Some(f) = g.as_mut() {
            let _ = writeln!(f, "t,what,x,y,look_yaw,look_pitch,cam_yaw,cam_pitch,mouse_look,grab");
        }
    }
    let (input, look) = (&**cx.input, cx.view.cam.look);
    let (x, y) = input.cursor;
    if let (Some(f), Some(c)) = (g.as_mut(), cx.view.camera.as_ref()) {
        let _ = writeln!(f, "{:.4},frame,{x:.1},{y:.1},{:.3},{:.3},{:.3},{:.3},{},{:?}", cx.common.started.elapsed().as_secs_f64(), look.0, look.1, c.yaw, c.pitch, input.mouse_look as u8, input.mouse_grab.mode);
    }
}
