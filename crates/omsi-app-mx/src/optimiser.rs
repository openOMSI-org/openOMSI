use omsi_launcher_lib::optimise::{self, Action, Apply, Machine, Outcome, Step, Tuner};

const PROGRESS_S: f32 = 4.0;

#[derive(Default)]
pub(crate) struct Optimiser {
    tuner: Option<Tuner>,
    looked_for_armed: bool,
    driving: bool,
    shown: f32,
}

impl Optimiser {
    pub(crate) fn running(&self) -> bool {
        self.tuner.is_some()
    }
}

pub(crate) fn recover_cut_short() {
    match optimise::recover() {
        Ok(true) => log::info!("optimising: the last run was cut short, its settings are back as they were before it"),
        Ok(false) => {}
        Err(e) => log::warn!("optimising: the settings of a run cut short were not put back ({e:#})"),
    }
}

impl crate::App {
    pub(crate) fn can_optimise(&self) -> bool {
        !self.vr_active() && omsi_cfg::flags::OMSI_SAFE_GPU.parse::<u32>().filter(|n| *n > 0).is_none()
    }

    pub(crate) fn start_optimiser(&mut self, asked: u32) {
        if self.perf.optimiser.running() || !self.can_optimise() {
            return;
        }
        let Some(mut settings) = crate::game_lists::saved_settings() else { return };
        if let Err(e) = optimise::begin(&settings) {
            log::warn!("optimising: the settings could not be saved first, nothing changed ({e:#})");
            self.service_msg = Some(("The settings could not be saved: nothing was changed".into(), 5.0));
            return;
        }
        settings["graphics"] = serde_json::json!(crate::game_lists::session_mode(self));
        let refresh = self.window.as_ref().and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).map(|mhz| (mhz as f64 / 1000.0).round() as u32);
        let target = optimise::target_for(asked, refresh);
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        let machine = match omsi_render::ceiling() {
            Some(c) => Machine { cores, msaa_counts: [1, 2, 4, 8].into_iter().filter(|&n| c.msaa_for(n) == n).fold(0, |m, n| m | n), ssao: c.ssao },
            None => Machine { cores, ..Default::default() },
        };
        log::info!("optimising for this computer: {target} fps aimed at (screen {refresh:?} Hz, {machine:?})");
        if let Some(r) = self.renderer.as_ref() {
            r.set_dynamic_scale(1.0);
        }
        self.perf.optimiser.tuner = Some(Tuner::new(settings, target, machine));
        self.perf.optimiser.shown = 0.0;
        self.service_msg = Some(("Optimising for your PC: drive on as usual, it takes about a minute".into(), 6.0));
    }

    pub(crate) fn optimiser_frame(&mut self, raw_dt: f32, wait: f32) {
        let driving = !self.paused && self.player.is_some() && self.world.is_some() && self.menus.menu.is_none() && self.menus.game_menu.is_none() && self.menus.list_kind.is_none();
        self.perf.optimiser.driving = driving;
        if driving && !self.perf.optimiser.looked_for_armed {
            self.perf.optimiser.looked_for_armed = true;
            if let Some(target) = optimise::armed().filter(|_| self.can_optimise()) {
                self.start_optimiser(target);
            }
        }
        if !driving {
            return;
        }
        let Some(tuner) = self.perf.optimiser.tuner.as_mut() else { return };
        let action = tuner.frame(raw_dt, wait);
        let progress = (tuner.measuring() || tuner.steps() > 0).then(|| tuner.last_fps()).flatten();
        if let (false, Some(r)) = (action == Action::Wait, tuner.last_reading()) {
            log::info!("optimising: {:.0} fps, {:.0} fps for the slowest tenth of the frames, {:.0} % of the time waiting for the graphics card", r.fps, r.low_fps, r.gpu_share * 100.0);
        }
        match action {
            Action::Wait => {}
            Action::Apply(steps) => self.apply_optimiser_steps(&steps),
            Action::Finish(steps, outcome) => {
                self.apply_optimiser_steps(&steps);
                self.finish_optimiser(outcome);
                return;
            }
        }
        self.perf.optimiser.shown += raw_dt;
        if let Some(fps) = progress.filter(|_| self.perf.optimiser.shown >= PROGRESS_S) {
            self.perf.optimiser.shown = 0.0;
            self.service_msg = Some((format!("Optimising for your PC: {} fps", fps.round() as u32), PROGRESS_S + 0.5));
        }
    }

    pub(crate) fn optimiser_holds_frame_rate(&self) -> bool {
        self.perf.optimiser.running() && self.perf.optimiser.driving
    }

    fn apply_optimiser_steps(&mut self, steps: &[Step]) {
        if steps.is_empty() {
            return;
        }
        for st in steps {
            log::info!("optimising: {} = {}{}", st.key, st.to, if st.apply == Apply::NextStart { " (from the next start)" } else { "" });
        }
        crate::game_lists::store_with(self, |v| {
            for st in steps {
                v[st.key] = st.to.clone();
            }
        });
        self.picture_to_settings(steps.iter().any(|s| s.apply == Apply::Rebuild));
    }

    pub(crate) fn picture_to_settings(&mut self, rebuild: bool) {
        let o = self.settings.render_options();
        let Some(r) = self.renderer.as_mut() else { return };
        r.options.render_scale = o.render_scale;
        r.options.max_obj_dist = o.max_obj_dist;
        r.options.min_obj_size = o.min_obj_size;
        r.options.ssao = o.ssao && omsi_render::ceiling().is_none_or(|c| c.ssao);
        r.options.street_light = o.street_light;
        if let (true, Some(scene)) = (rebuild, self.scene.as_mut()) {
            r.rebuild_with(scene, omsi_render::RenderOptions { msaa: o.msaa, shadow_size: o.shadow_size, ..r.options });
        }
    }

    fn finish_optimiser(&mut self, outcome: Outcome) {
        self.perf.optimiser.tuner = None;
        optimise::end();
        optimise::disarm();
        let (before, after) = (outcome.before_fps.round() as u32, outcome.after_fps.round() as u32);
        log::info!("optimised for this computer: {before} -> {after} fps ({} fps aimed at, {} changes{})", outcome.target, outcome.changes.len(), if outcome.restart { ", more from the next start" } else { "" });
        if let Err(e) = optimise::save_result(&outcome) {
            log::warn!("optimising: the result was not saved ({e:#})");
        }
        let graphics_changed = outcome.changes.iter().any(|c| c.key != "max_fps");
        let msg = if !graphics_changed && outcome.reached {
            format!("Your PC already runs at {} fps: nothing to change", after)
        } else if !outcome.reached {
            format!("Optimised as far as it goes: {} → {} fps (aim: {} fps)", before, after, outcome.target)
        } else if outcome.restart {
            format!("Optimised for your PC: {} → {} fps, more at the next start", before, after)
        } else {
            format!("Optimised for your PC: {} → {} fps", before, after)
        };
        self.service_msg = Some((msg, 8.0));
    }

    pub(crate) fn abort_optimiser(&mut self) {
        if self.perf.optimiser.tuner.take().is_some() {
            recover_cut_short();
            self.perf.optimiser = Optimiser { looked_for_armed: true, ..Default::default() };
        }
    }

    pub(crate) fn restore_before_optimising(&mut self) {
        self.abort_optimiser();
        let mut restored = false;
        crate::game_lists::store_with(self, |v| restored = optimise::restore_into(v));
        if restored {
            optimise::forget();
            self.picture_to_settings(true);
            log::info!("optimising: the settings from before are back");
            self.service_msg = Some(("Your previous settings are back".into(), 5.0));
        }
    }
}
