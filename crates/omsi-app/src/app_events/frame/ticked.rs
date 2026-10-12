//! The window's frame with the fixed simulation step (`omsi_sim::tick`, 120 ticks a second):
//! what the player does and the frame's housekeeping run once a frame (`FRAME_BEFORE`); the
//! simulation - the traffic, the player's bus with its scripts, the people, the duty, the
//! clock and the weather - runs once per tick, each tick as long as the next (`FRAME_TICK`);
//! then the view of it, the sounds, the LAN, the interface and the picture once a frame again
//! (`FRAME_AFTER`), with the player's bus and the camera drawn between the last two ticks.

use super::stages::{WindowFrame, FRAME_AFTER, FRAME_BEFORE, FRAME_TICK};
use super::*;
use crate::frame_stages::Flow;

impl App {
    /// One frame of the window (see the module's notes).
    pub(super) fn frame_ticked(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let mut frame = WindowFrame::new(self, event_loop);
        if crate::frame_stages::run(&mut frame, FRAME_BEFORE)? == Flow::Stop {
            return Ok(());
        }
        let frame_dt = frame.frame_dt();
        let ft = &mut frame.app().fixed_tick;
        let tick_dt = ft.step.dt();
        let ticks = ft.step.advance(frame_dt);
        for k in 0..ticks.n {
            let app = frame.app();
            app.fixed_tick.enter_tick(k, ticks.n);
            if k + 1 == ticks.n {
                app.fixed_tick.remember(app.player.as_ref().map(|p| &p.vehicle));
            }
            frame.set_tick_dt(Some(tick_dt));
            let flow = crate::frame_stages::run(&mut frame, FRAME_TICK);
            frame.set_tick_dt(None);
            flow?;
        }
        // the daylight of the last tick (of the frame before, when this one had none)
        let daylight = match frame.daylight() {
            Some(d) => d,
            None => {
                let app = frame.app();
                app.fixed_tick.daylight().unwrap_or_else(|| omsi_sim::Daylight::compute(&app.clock, app.session.envir.as_ref()))
            }
        };
        frame.set_daylight(daylight);
        frame.app().fixed_tick.frame_done(ticks, daylight);
        crate::frame_stages::run(&mut frame, FRAME_AFTER)?;
        Ok(())
    }

    /// The player's bus as the frame shows it (`FRAME_AFTER`): between its last two ticks,
    /// into the scene and under the camera, then back where the simulation has it.
    pub(super) fn frame_player_view(&mut self, dt: f32) {
        // a walker stepping into or out of the bus hears its cab by degrees
        let walker = self.foot_sound_amount(dt);
        omsi_audio::soundset::set_cab_amount(walker);
        if self.player.is_none() || self.renderer.is_none() || self.scene.is_none() {
            return;
        }
        let between = match self.player.as_ref() {
            Some(p) if !self.paused => self.fixed_tick.between(&p.vehicle),
            _ => None,
        };
        if let (Some((_, at)), Some(p)) = (between.as_ref(), self.player.as_mut()) {
            omsi_sim::tick::put(&mut p.vehicle, at);
        }
        super::driving::player_to_scene(super::driving::PlayerToSceneCx::of(&mut self.parts()), dt);
        self.frame_view_camera(dt);
        if let (Some((cur, _)), Some(p)) = (between.as_ref(), self.player.as_mut()) {
            omsi_sim::tick::put(&mut p.vehicle, cur);
        }
        self.frame_player_hover();
    }
}
