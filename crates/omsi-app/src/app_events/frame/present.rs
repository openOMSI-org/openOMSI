//! The picture in the window's frame: its lighting, the mirrors, the headset, the window's
//! picture and its presentation, the frame rate's limit and the session's statistics.

use super::*;

crate::app::stage_ctx!(
    /// The lighting stage's part of the game.
    LightingCx { reads: [net, world, clock], writes: [weather, player] }
);

/// The lighting the frame is drawn with.
pub(super) fn lighting(cx: LightingCx, dt: f32, daylight: omsi_sim::Daylight) -> omsi_render::Lighting {
    if let Some(w) = cx.weather.weather.as_ref() {
        *cx.weather.wetness = road_wetness(precip_of(w).1, dt as f64, *cx.weather.wetness);
    }
    let inside = match cx.net.inside_remote.and_then(|id| cx.net.remotes.remotes.get(&id)) {
        // (in another player's bus: its box is the one the camera is in)
        Some(rv) => Some(rv.vehicle()),
        None => cx.player.player.as_ref().map(|p| &p.vehicle),
    };
    steps::picture_lighting(
        &daylight,
        cx.weather.weather.as_ref(),
        *cx.weather.cloud_drift,
        *cx.weather.wetness,
        cx.weather.rain.snow_cover(),
        cx.world.world.as_deref(),
        inside,
        cx.player.player.as_ref().map(|p| &p.vehicle),
        cx.weather.cabin_air.appearance(),
        cx.common.settings,
        cx.clock.clock.run_time as f32,
        &*cx.weather.sky,
    )
}

impl App {
    /// The frame drawn and shown (or, with the window hidden, the simulation kept at a
    /// display's pace), the frame rate limited, the session's statistics.
    pub(super) fn frame_render(
        &mut self,
        event_loop: &ActiveEventLoop,
        time: &FrameTime,
        lighting: &omsi_render::Lighting,
        vr_nav_display: Option<crate::vr_navigator::Display>,
    ) {
        let mut finish = false;
        let mut reconfigure = false;
        let shot = self.perf.shot.take();
        if let Some(s) = self.gfx.surface.as_ref() {
            let (w, h) = (s.config.width, s.config.height);
            self.touch_prepare(w, h);
        }
        if self.gfx.surface.is_some()
            && self.renderer.is_some()
            && self.scene.is_some()
            && self.camera.is_some()
            && self.window.is_some()
        {
            if let Some(shot) = shot {
                let file = shot.0.to_string_lossy().into_owned();
                self.frame_shot(shot, lighting);
                if self.integrations.plugins.as_ref().is_some_and(|p| !p.is_empty()) {
                    crate::plugins::queue_event(&mut self.integrations.plugin_events, "screenshot", vec![omsi_plugin::InfoValue::Text(file)]);
                }
            }
            // the photo mode's pictures (the window shows the developed photo)
            if self.photo.is_some() {
                self.photo_render(lighting);
            }
            let (frame, view, shown_nothing) = self.frame_acquire(&mut reconfigure);
            let presented = frame.is_some();
            match view {
                Some(view) => self.frame_draw(time.raw_dt, lighting, vr_nav_display, frame, view),
                None => self.frame_hidden(time.now),
            }
            self.frame_pace(time.now, shown_nothing, presented);
            // (what the sim thread stepped meanwhile is back before the frame is counted or
            // the session finished, see `sim_thread`)
            self.join_sim_thread();
            finish = self.frame_count(event_loop);
            if let Some(win) = self.window.as_ref() {
                win.request_redraw();
            }
        }
        if reconfigure {
            // the drawable went away under us (display change, lost surface)
            if let (Some(s), Some(r), Some(win)) = (
                self.gfx.surface.as_mut(),
                self.renderer.as_ref(),
                self.window.as_ref(),
            ) {
                let size = crate::platform::surface_size(win);
                s.resize(r, size.width, size.height);
            }
        }
        if finish {
            self.finish_session();
        }
    }

    /// `shot <file>` from the input script.
    fn frame_shot(&mut self, shot: (PathBuf, bool), lighting: &omsi_render::Lighting) {
        let (Some(s), Some(r), Some(scene), Some(cam)) = (
            self.gfx.surface.as_ref(),
            self.renderer.as_mut(),
            self.scene.as_mut(),
            self.camera.as_ref(),
        ) else {
            return;
        };
        // `shot <file>` from the input script: the scene the window is showing,
        // from its camera and lighting, into a PNG - the only way to look at
        // what an automated window run draws (also when the window is hidden,
        // so it does not depend on a frame being acquired)
        let (path, include_touch) = shot;
        // the photo mode: its developed photo (stretched to the window) under its panel
        let developed = self.photo.as_ref().filter(|ph| ph.render.showing_photo()).and_then(|ph| ph.shown()).map(|(w, h, px)| crate::photo::stretch(w, h, px, s.config.width, s.config.height));
        // a player's screenshot is the world without the interface over it, as OMSI's
        // (#1265); the input script's pictures keep it for the visual tests
        let hud = (!include_touch).then(|| std::mem::take(&mut scene.overlays));
        let picture = developed.map(Ok).unwrap_or_else(|| r.render_to_image(
            scene,
            s.config.width,
            s.config.height,
            cam,
            lighting,
        ));
        if let Some(hud) = hud {
            scene.overlays = hud;
        }
        match picture {
            Ok(mut px) => match {
                // (with the on-screen controls, when there are)
                if let Some(over) = self.shell.picture_for_shot(r, s.config.width, s.config.height) {
                    crate::touch::composite(&mut px, &over);
                }
                if include_touch {
                    if let Some(over) = self.input.touch.picture(r, s.config.width, s.config.height) {
                        crate::touch::composite(&mut px, &over);
                    }
                }
                image::save_buffer(
                    &path,
                    &px,
                    s.config.width,
                    s.config.height,
                    image::ColorType::Rgba8,
                ) } {
                Ok(()) => {
                    log::info!("input script: window picture written to {}", path.display());
                    if !include_touch {
                        crate::platform::to_gallery(&path);
                        if cfg!(target_os = "android") {
                            self.service_msg = Some((omsi_ui::tr("Screenshot saved to the gallery").into_owned(), 4.0));
                        }
                    }
                }
                Err(e) => log::warn!(
                    "input script: {} could not be written: {e}",
                    path.display()
                ),
            },
            Err(e) => log::warn!(
                "input script: the window picture could not be rendered: {e}"
            ),
        }
    }

    /// The window's picture to draw into (none: the window is hidden; `reconfigure`: the
    /// drawable went away), and whether nothing is shown this frame.
    fn frame_acquire(&mut self, reconfigure: &mut bool) -> (Option<wgpu::SurfaceTexture>, Option<wgpu::TextureView>, bool) {
        let (Some(s), Some(r)) = (self.gfx.surface.as_ref(), self.renderer.as_ref()) else {
            return (None, None, true);
        };
        // A window that is hidden (another app covers it, another Space) gets
        // no frames on macOS. OMSI_RENDER_OCCLUDED=1 draws them into a texture
        // of the window's size anyway and waits for the GPU as a present would,
        // so frame times can be measured with the window out of sight.
        let __t = Instant::now();
        // OMSI_HIDE_WINDOW=from,to: treat the window as hidden between these
        // seconds of the session (the frame is acquired and dropped unshown), to
        // check the hidden-window path without covering the window by hand
        let hide_test = omsi_cfg::flags::OMSI_HIDE_WINDOW.var().and_then(|v| {
            let mut it = v.split(',').filter_map(|x| x.trim().parse::<f32>().ok());
            Some((it.next()?, it.next()?))
        });
        let hidden_now = hide_test
            .map(|(a, b)| (a..b).contains(&self.started.elapsed().as_secs_f32()))
            .unwrap_or(false);
        let acquired = match s.acquire() {
            wgpu::CurrentSurfaceTexture::Success(_)
            | wgpu::CurrentSurfaceTexture::Suboptimal(_)
            if hidden_now =>
                {
                    wgpu::CurrentSurfaceTexture::Occluded
                }
            other => other,
        };
        let (frame, stand_in) = match acquired {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (Some(frame), None),
            wgpu::CurrentSurfaceTexture::Occluded
            if omsi_cfg::flags::OMSI_RENDER_OCCLUDED.is_set() || omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set() =>
                {
                    let (w, h) = (s.config.width, s.config.height);
                    if self
                        .gfx.stand_in
                        .as_ref()
                        .map(|t| (t.width(), t.height()) != (w, h))
                        .unwrap_or(true)
                    {
                        self.gfx.stand_in =
                            Some(r.device.create_texture(&wgpu::TextureDescriptor {
                                label: Some("hidden window"),
                                size: wgpu::Extent3d {
                                    width: w,
                                    height: h,
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: r.format(),
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                view_formats: &[],
                            }));
                    }
                    (
                        None,
                        self.gfx.stand_in
                            .as_ref()
                            .map(|t| t.create_view(&Default::default())),
                    )
                }
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost => {
                *reconfigure = true;
                (None, None)
            }
            _ => (None, None),
        };
        self.perf.profile.add(crate::profile_keys::ProfKey::Acquire, __t.elapsed().as_secs_f64());
        if frame.is_none() {
            self.gfx.hidden_frames += 1;
        }
        let shown_nothing = frame.is_none() && stand_in.is_none();
        let view = frame
            .as_ref()
            .map(|f| s.view(&r.device, f))
            .or(stand_in);
        (frame, view, shown_nothing)
    }

    /// The mirrors, the headset's picture, the mirror panels, the window's picture and the
    /// on-screen controls, and the picture shown.
    fn frame_draw(
        &mut self,
        raw_dt: f32,
        lighting: &omsi_render::Lighting,
        vr_nav_display: Option<crate::vr_navigator::Display>,
        frame: Option<wgpu::SurfaceTexture>,
        view: wgpu::TextureView,
    ) {
        #[cfg(not(windows))]
        let _ = vr_nav_display;
        // (the photo mode draws the live picture while its camera moves, the photo once it
        // stands - as its panel decided when it was drawn this frame: asked again here, after
        // the photo's pictures were taken, the photo had become ready meanwhile, the scene was
        // left out and the panel had no photo in it - the window flashed black)
        let photo = self.photo.is_some() && self.shell.opaque;
        if !photo {
            self.frame_mirrors(raw_dt, lighting);
        }
        let (Some(s), Some(r), Some(scene), Some(cam), Some(win)) = (
            self.gfx.surface.as_ref(),
            self.renderer.as_mut(),
            self.scene.as_mut(),
            self.camera.as_ref(),
            self.window.as_ref(),
        ) else {
            return;
        };
        let __t = Instant::now();
        #[cfg(windows)]
        let mut mirrored = false;
        #[cfg(not(windows))]
        let mirrored = false;
        #[cfg(windows)]
        if let Some(vr) = self.xr.vr.as_mut() {
            let menu_range = self.ui.as_ref().map(|u| u.menu_overlay_range.clone()).unwrap_or(0..0);
            let cursor_overlay = self.ui.as_ref().and_then(|u| u.vr_cursor_overlay).filter(|_| self.xr.vr_nav_edit.is_none());
            let tooltip_overlay = self.ui.as_ref().and_then(|u| u.vr_tooltip_overlay).filter(|_| self.xr.vr_nav_edit.is_none());
            match vr.render(
                r,
                scene,
                cam,
                &lighting,
                &view,
                (s.config.width, s.config.height),
                menu_range,
                cursor_overlay,
                tooltip_overlay,
                self.input.cursor,
                self.player.as_ref().map(|p| (p.vehicle.position, p.vehicle.body_rotation())),
                vr_nav_display.filter(|d| d.placement.enabled).and_then(|d| {
                    self.menus.navigator.as_ref().and_then(|n| n.panel_overlay).map(|index| (index, d))
                }),
                self.player.as_ref().map(|p| p.uid),
                self.settings.vr_head_smoothing_ms,
                !self.input.mouse_drive,
                self.xr.vr_zoom_active,
            ) {
                Ok(visible) => mirrored = visible,
                Err(e) => {
                    log::error!("OpenXR rendering stopped: {e:#}");
                    self.xr.vr = None;
                }
            }
        }
        if photo {
            // (the developed photo covers the window: the screens draw it)
        } else if !mirrored
            && self.settings.triple.enabled
            && !self.settings.vr_requested()
        {
            let rig = self.settings.triple.zoomed(
                s.config.width,
                s.config.height,
                self.cam.view_zoom.get(&self.view).copied().unwrap_or(1.0),
            );
            r.render_triple(
                scene,
                &view,
                s.config.width,
                s.config.height,
                cam,
                lighting,
                &rig,
            );
        } else if !mirrored {
            r.render(
                scene,
                &view,
                s.config.width,
                s.config.height,
                cam,
                lighting,
            );
        }
        // the pause menu and the photo mode's panel
        self.shell.render(r, &view, s.config.width, s.config.height);
        // the on-screen controls over the picture (a phone)
        self.input.touch.render(r, &view, s.config.width, s.config.height);
        self.perf.profile.add(crate::profile_keys::ProfKey::Render, __t.elapsed().as_secs_f64());
        if omsi_cfg::flags::OMSI_PROFILE_GPU.is_set() {
            // wait for the GPU here, so that its time shows as a stage of its own
            let __t = Instant::now();
            let _ = omsi_render::wait_gpu(&r.device, None);
            self.perf.profile.add(crate::profile_keys::ProfKey::Gpu, __t.elapsed().as_secs_f64());
        }
        let __t = Instant::now();
        match frame {
            Some(frame) => {
                // (without V-sync max_fps paces the frames: waiting for the compositor's frame callback cost a missed refresh each slow frame)
                if self.settings.vsync {
                    win.pre_present_notify();
                }
                s.present(&r.device, &r.queue, frame);
            }
            None => {
                let _ = omsi_render::wait_gpu(&r.device, None);
            }
        }
        self.perf.profile.add(crate::profile_keys::ProfKey::Present, __t.elapsed().as_secs_f64());
    }

    /// The bus's mirrors redrawn, in turn, within their budget.
    fn frame_mirrors(&mut self, raw_dt: f32, lighting: &omsi_render::Lighting) {
        let (Some(s), Some(r), Some(scene), Some(cam)) = (
            self.gfx.surface.as_ref(),
            self.renderer.as_mut(),
            self.scene.as_mut(),
            self.camera.as_ref(),
        ) else {
            return;
        };
        let __t = Instant::now();
        // One mirror a turn, in turn, at most MIRROR_RATE pictures a second in
        // all: a mirror costs half the main picture's CPU time, and at 140 fps
        // five mirrors were each redrawn 28 times a second, a small picture
        // that nobody can tell from 15.
        // Every mirror at least MIRROR_MIN_HZ, though: with eight of them (the
        // Procity) at 25 fps each was redrawn three times a second, and the
        // street jerked past in them - up to two a frame then (each costs a
        // few milliseconds of the frame).
        if let (Some(p), Some(w)) = (self.player.as_ref(), self.world.as_ref()) {
            steps::mirror_hud_sync(&mut self.gfx.mirror_hud, w, p, self.settings.mirror_hud);
        }
        r.options.mirror_enhanced = self.settings.mirror_quality == "enhanced";
        #[cfg(windows)]
        let vr_active = self.xr.vr.is_some();
        #[cfg(not(windows))]
        let vr_active = false;
        let mirrors_shown = self.settings.mirror_size != 0
            && self.settings.mirror_refresh != "off"
            && (self.cam.in_cab || self.player.as_ref().is_some_and(|p| (p.vehicle.position - cam.position).length() < 12.0));
        self.gfx.sim_view.people.mirror_hz = if vr_active {
            None
        } else if !mirrors_shown {
            Some(0.0)
        } else if self.settings.mirror_refresh == "every" || (self.settings.mirror_hud_every_frame && self.gfx.mirror_hud.active()) {
            None
        } else if self.settings.mirror_refresh == "full" {
            Some(MIRROR_MAX_HZ_FULL)
        } else {
            Some(MIRROR_MAX_HZ_ECO)
        };
        // With `mirror_hud_every_frame` the mirror panels in the cab (Ctrl+M) are drawn every
        // frame, whatever the refresh setting, seen in the glass or not: a driving aid on the
        // screen, not a small picture far off. The other mirrors take their turns below without
        // them. (In VR the panels are not drawn.)
        let mut panel_cams = std::mem::take(&mut self.gfx.mirror_panel_cams);
        if self.settings.mirror_size != 0 && !vr_active && self.cam.in_cab && self.gfx.mirror_hud.active() {
            self.gfx.mirror_hud.cams_into(&mut panel_cams);
        } else {
            panel_cams.clear();
        }
        let every_frame = self.settings.mirror_hud_every_frame;
        let (skip, on_screen): (&[usize], &[usize]) = if every_frame { (&panel_cams, &[]) } else { (&[], &panel_cams) };
        if every_frame && !panel_cams.is_empty() {
            if let (Some(w), Some(p)) = (self.world.as_ref(), self.player.as_ref()) {
                let view = (*cam, s.config.width as f32 / s.config.height.max(1) as f32);
                render_mirrors(r, scene, w, p, lighting, MirrorPick::Only(&panel_cams), Some(view), &[]);
            }
        }
        if self.settings.mirror_size == 0 {
            self.gfx.mirror_budget = 0.0;
            self.gfx.mirrors_seen = 0;
        } else if self.settings.mirror_refresh == "every" && !vr_active {
            // every mirror in the picture every frame: the street goes by in them as smoothly
            // as through the windscreen (in turns, each picture was a step behind, and the
            // steps grew with the speed - the mirrors seemed to shake)
            self.gfx.mirror_budget = 0.0;
            let mirror_view = if self.settings.triple.enabled {
                None
            } else {
                Some((*cam, s.config.width as f32 / s.config.height.max(1) as f32))
            };
            let near = self.player.as_ref().is_some_and(|p| (p.vehicle.position - cam.position).length() < 12.0);
            if let (Some(w), Some(p)) = (self.world.as_ref(), self.player.as_ref()) {
                if self.cam.in_cab || near {
                    self.gfx.mirrors_seen = render_mirrors(r, scene, w, p, lighting, MirrorPick::All(skip), mirror_view, on_screen);
                }
            }
        } else if self.settings.mirror_refresh == "off" {
            self.gfx.mirror_budget = 0.0;
            if let (Some(w), Some(p)) = (self.world.as_ref(), self.player.as_ref()) {
                let since = match &self.gfx.frozen_mirrors {
                    Some(m) if m.bus == p.uid => m.since,
                    _ => -1.0,
                };
                let next = since.max(0.0) + raw_dt.min(0.1);
                // (and while the driver turns a mirror, so it can be aimed)
                if since < 0.0 || (since < MIRROR_FREEZE_REDRAW && next >= MIRROR_FREEZE_REDRAW) || p.mirrors_dirty {
                    self.gfx.mirrors_seen = render_mirrors(r, scene, w, p, lighting, MirrorPick::All(skip), None, on_screen);
                }
                self.gfx.frozen_mirrors = Some(FrozenMirrors { bus: p.uid, since: next });
            }
        } else {
            let mirrors = self.player.as_ref().map(|p| p.vehicle.ty.def.cameras_reflexion.len()).unwrap_or(0);
            let rate = {
                if vr_active {
                    // Preserve the user's total redraw budget. A negative
                    // value explicitly requests every mirror each frame.
                    omsi_cfg::flags::OMSI_OPENXR_MIRROR_RATE.parse::<f32>()
                        .filter(|rate| rate.is_finite() && *rate >= -1.0)
                        .unwrap_or(self.settings.vr_mirror_rate)
                } else {
                    let max_hz = if self.settings.mirror_refresh == "full" { MIRROR_MAX_HZ_FULL } else { MIRROR_MAX_HZ_ECO };
                    MIRROR_RATE.max(mirrors as f32 * MIRROR_MIN_HZ).min(max_hz * self.gfx.mirrors_seen.max(1) as f32)
                }
            };
            // The desktop camera does not follow the headset. Culling by
            // its frustum can leave a mirror visible in VR uninitialised
            // (black). Refresh all bus mirrors in VR, still taking turns
            // within the configured budget; keep desktop visibility culling.
            // (Panels taking their turns keep their mirrors in the turns, in the picture or
            // not. Panels drawn every frame above are left out of the turns instead.)
            let mirror_view = if vr_active || self.settings.triple.enabled {
                None
            } else {
                Some((*cam, s.config.width as f32 / s.config.height.max(1) as f32))
            };
            // (in the cab, and from outside too while the bus is near: its
            // mirrors are seen from the pavement and stood frozen)
            let near = self.player.as_ref().zip(self.camera.as_ref()).is_some_and(|(p, c)| (p.vehicle.position - c.position).length() < 12.0);
            let draw_limit = if vr_active {
                if self.cam.in_cab || near {
                    vr_mirror_updates(&mut self.gfx.mirror_budget, raw_dt, rate, mirrors)
                } else {
                    self.gfx.mirror_budget = 0.0;
                    0
                }
            } else {
                let min_rate = self.gfx.mirrors_seen.max(1) as f32 * MIRROR_MIN_HZ;
                mirror_updates(&mut self.gfx.mirror_budget, &mut self.gfx.mirror_dt, raw_dt, rate, min_rate, self.gfx.mirrors_seen.clamp(1, 2))
            };
            let mut drawn = 0;
            if vr_active && draw_limit > 0 && draw_limit == mirrors {
                // Prepare the cameras and textures only once when all
                // mirrors are due, including the Every frame mode.
                if let (Some(w), Some(p)) = (self.world.as_ref(), self.player.as_ref()) {
                    self.gfx.mirror_turn = self.gfx.mirror_turn.wrapping_add(draw_limit);
                    self.gfx.mirrors_seen = render_mirrors(r, scene, w, p, lighting, MirrorPick::All(skip), mirror_view, on_screen);
                    drawn = draw_limit;
                }
            }
            while (self.cam.in_cab || near) && drawn < draw_limit {
                let (Some(w), Some(p)) = (self.world.as_ref(), self.player.as_ref()) else { break };
                drawn += 1;
                self.gfx.mirror_turn = self.gfx.mirror_turn.wrapping_add(1);
                self.gfx.mirrors_seen = render_mirrors(
                    r,
                    scene,
                    w,
                    p,
                    lighting,
                    MirrorPick::Turn(self.gfx.mirror_turn, skip),
                    mirror_view,
                    on_screen,
                );
            }
        }
        self.gfx.mirror_panel_cams = panel_cams;
        self.perf.profile.add(crate::profile_keys::ProfKey::Mirrors, __t.elapsed().as_secs_f64());
    }

    /// Nothing to draw into: what was uploaded let go, the simulation at a display's pace.
    fn frame_hidden(&mut self, now: Instant) {
        let Some(r) = self.renderer.as_ref() else { return };
        // Nothing to draw into (a hidden window): the simulation goes on at
        // a display's pace instead of spinning a core a thousand times a
        // second. What it uploaded (traffic instances, streamed tiles,
        // people, the navigator) waits in wgpu's staging buffers until the
        // next submit, so submit nothing to let them go: without it a hidden
        // window on Ahlheim grew by 100 MB a second (5.6 GB after 55 s).
        let __t = Instant::now();
        r.queue.submit(std::iter::empty::<wgpu::CommandBuffer>());
        let _ = r.device.poll(wgpu::PollType::Poll);
        self.perf.profile.add(crate::profile_keys::ProfKey::Present, __t.elapsed().as_secs_f64());
        if let Some(rest) =
            std::time::Duration::from_millis(16).checked_sub(now.elapsed())
        {
            std::thread::sleep(rest);
        }
    }

    /// The frame rate's limit: the rest of the frame's time slept. `presented`: the frame
    /// went to the window's surface (with V-sync its present already waits for the screen).
    fn frame_pace(&mut self, now: Instant, shown_nothing: bool, presented: bool) {
        // max_fps (the original's [maxFPS]; OMSI_MAX_FPS for a test): the rest of
        // the frame's time is slept, not spun, so a limit gives the CPU back
        // (and keeps a laptop cool enough not to slow itself down)
        let max_fps = if self.optimiser_holds_frame_rate() { 1000 } else { omsi_cfg::flags::OMSI_MAX_FPS.parse::<u32>().unwrap_or(self.settings.max_fps) };
        let asked = max_fps;
        // 0 = the screen's refresh rate: frames the screen never shows only heat the
        // machine (with V-sync off and no limit an M4 drew 300 frames a second in the
        // depot and ran hot); 1000 and more = no limit at all
        let max_fps = if max_fps == 0 {
            // (asked of the system once a second, not every frame: it is a round trip to
            // the window server, and a window only moves to another screen now and then)
            static RATE: parking_lot::Mutex<Option<(Instant, u32)>> = parking_lot::Mutex::new(None);
            let mut rate = RATE.lock();
            match *rate {
                Some((at, r)) if now.duration_since(at) < std::time::Duration::from_secs(1) => r,
                _ => {
                    let r = self.window.as_ref().and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).map(|mhz| (mhz as f64 / 1000.0).round() as u32).filter(|r| *r >= 30).unwrap_or(120);
                    *rate = Some((now, r));
                    r
                }
            }
        } else if max_fps >= 1000 {
            0
        } else {
            max_fps
        };
        // a frame not shown (the window minimised or out of sight): 30 frames a
        // second keep the simulation and the sound going; more is only heat
        // (OMSI_RENDER_OCCLUDED, which draws them anyway, keeps its pace)
        let max_fps = if shown_nothing { if max_fps == 0 { 30 } else { max_fps.min(30) } } else { max_fps };
        #[cfg(windows)]
        let vr_active = self.xr.vr.is_some();
        #[cfg(not(windows))]
        let vr_active = false;
        // With V-sync and no limit of the player's own (0 = the screen's rate) the present
        // already holds the frames to the screen's refresh. Sleeping a refresh from each
        // frame's start on top of it drifted against the screen's vblank: now and then a
        // frame slept past one and waited for the next, two refreshes for one frame.
        let vsync_paces = presented
            && asked == 0
            && self.gfx.surface.as_ref().is_some_and(|s| matches!(s.config.present_mode, wgpu::PresentMode::Fifo | wgpu::PresentMode::FifoRelaxed | wgpu::PresentMode::AutoVsync));
        if max_fps > 0 && !vr_active && !vsync_paces {
            let __t = Instant::now();
            pace_until(now, std::time::Duration::from_secs_f64(1.0 / max_fps as f64));
            self.perf.profile.add(crate::profile_keys::ProfKey::Limiter, __t.elapsed().as_secs_f64());
        }
    }

    /// The frames counted, `--exit-after` with its statistics (true: the session ends), and
    /// the window's title once a second.
    fn frame_count(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let (Some(_), Some(r), Some(cam), Some(win)) = (
            self.gfx.surface.as_ref(),
            self.renderer.as_mut(),
            self.camera.as_ref(),
            self.window.as_ref(),
        ) else {
            return false;
        };
        let mut finish = false;
        self.perf.frames += 1;
        // (a second of play drawn: this session started well, see omsi-guard)
        if self.perf.frames == 60 {
            omsi_guard::healthy(omsi_guard::journal::Kind::Game);
        }
        let profiling = omsi_cfg::flags::OMSI_PROFILE.is_set();
        // (the warm-up: 15 s of play, not of the process - a big map's loading took most of
        // 15 s, and the summary then held the first heavy frames of play; marked whether or not
        // the CPU time can be read)
        let playing = *self.perf.play_started.get_or_insert_with(Instant::now);
        if profiling
            && self.perf.profile_mark.is_none()
            && playing.elapsed().as_secs_f32() > 15.0
        {
            self.perf.cpu_mark =
                process_cpu_seconds().map(|c| (c, Instant::now(), self.perf.total_frames));
            self.perf.thread_cpu_mark = crate::startup::thread_cpu_seconds();
            self.perf.instructions_mark = crate::startup::process_instructions();
            self.perf.profile_mark = Some(crate::perf_report::ProfileMark::take(&self.perf.profile, r));
        }
        if let (Some(limit), false) = (self.args.exit_after, self.exiting) {
            if self.started.elapsed().as_secs_f32() > limit {
                self.exiting = true;
                log::info!("exit after {limit} s: {} frames total ({} with the window hidden{}), {:.1} fps average, {} frames over 50 ms, worst {:.0} ms", self.perf.total_frames, self.gfx.hidden_frames, if omsi_cfg::flags::OMSI_RENDER_OCCLUDED.is_set() || omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set() { ", drawn off-screen" } else { ", not drawn" }, self.perf.total_frames as f32 / self.started.elapsed().as_secs_f32(), self.perf.spikes, self.perf.worst_ms);
                if let (Some(st), Some(w)) =
                    (self.gfx.streamer.as_ref(), self.world.as_ref())
                {
                    log::info!("tile streaming: {} tiles loaded now, {} loaded and {} unloaded in all, {:.1} s preparing on the worker, slowest upload {:.0} ms, streaming over 16 ms in {} frames (worst {:.0} ms); {} objects + {} trees, {} rows, {} attached ({} without parent), {} unresolved", w.loaded_tiles().len(), st.loaded_total, st.unloaded_total, st.prepare_secs, st.worst_upload_ms, st.slow_frames, st.worst_frame_ms, st.stats.objects, st.stats.trees, st.stats.rows, st.stats.attached, st.stats.unattached, st.stats.failed_objects);
                    st.stats.log_ground();
                }
                finish = true;
                crate::platform::exit(event_loop);
            }
        }
        self.perf.total_frames += 1;
        if self.perf.fps_t.elapsed().as_secs_f32() >= 1.0 {
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                let secs = self.perf.fps_t.elapsed().as_secs_f32();
                log::info!("profile interval: {:.1} fps over {secs:.2} s", self.perf.frames as f32 / secs);
            }
            let speed = self
                .player
                .as_ref()
                .map(|p| format!(" - {:.0} km/h", p.vehicle.physics.velocity_kmh()))
                .unwrap_or_default();
            self.perf.fps = self.perf.frames as f32;
            let title = format!(
                "openOMSI - {} fps{speed} - {:.0},{:.0},{:.0} yaw {:.0}",
                self.perf.frames,
                cam.position.x,
                cam.position.y,
                cam.position.z,
                cam.yaw.rem_euclid(360.0)
            );
            // (a new title is a synchronous round trip to the window server: only when it
            // says something new - standing still at a steady rate it does not)
            static SHOWN: parking_lot::Mutex<String> = parking_lot::Mutex::new(String::new());
            let mut shown = SHOWN.lock();
            if *shown != title {
                win.set_title(&title);
                *shown = title;
            }
            self.perf.frames = 0;
            self.perf.fps_t = Instant::now();
        }
        if finish {
            self.log_profile_summary();
        }
        finish
    }

    /// OMSI_PROFILE's summary of the session: the stages per frame, the GPU passes, counts and
    /// percentiles. Written once, by whichever way out comes first - `--exit-after`, or the
    /// player closing the game (`exiting`), so a tester can play as usual and quit instead of
    /// guessing a duration beforehand.
    pub(crate) fn log_profile_summary(&mut self) {
        if !omsi_cfg::flags::OMSI_PROFILE.is_set() || self.perf.profile_written {
            return;
        }
        let (Some(s), Some(r)) = (self.gfx.surface.as_ref(), self.renderer.as_ref()) else {
            return;
        };
        self.perf.profile_written = true;
        let n = self.perf.total_frames.max(1) as f64;
        for (k, v) in &self.perf.profile {
            log::info!("profile {k:10}: {:.1} ms/frame", v / n * 1000.0);
        }
        if let Some(h) = self.session.humans.as_ref() {
            log::info!(
                "profile people: {} ({})",
                h.people.len(),
                h.summary(&self.gfx.sim_view.people)
            );
        }
        for (k, v) in r.stats.borrow().iter() {
            log::info!(
                "profile render.{k:10}: {:.2} ms/frame",
                v / n * 1000.0
            );
        }
        for (k, v) in r.counts.borrow().iter() {
            log::info!("profile count {k}: {:.0} a frame", v / n);
        }
        if let Some(memory) = r.allocator_summary() {
            log::info!("profile graphics memory: {memory}");
        }
        for (pass, ms, frames) in r.gpu_pass_times() {
            log::info!("profile gpu pass {pass:12}: {ms:.2} ms ({frames} frames measured)");
        }
        if let (Some((c0, t0, f0)), Some(c1)) =
            (self.perf.cpu_mark, process_cpu_seconds())
        {
            let frames = self.perf.total_frames.saturating_sub(f0).max(1) as f64;
            let main = self.perf.thread_cpu_mark.zip(crate::startup::thread_cpu_seconds()).map(|(m0, m1)| format!(", {:.2} ms CPU of the frame's own thread", (m1 - m0) / frames * 1000.0)).unwrap_or_default();
            log::info!("profile: since 15 s {:.1} ms wall and {:.1} ms CPU (all threads) per frame{main}, {:.1} cores busy", t0.elapsed().as_secs_f64() / frames * 1000.0, (c1 - c0) / frames * 1000.0, (c1 - c0) / t0.elapsed().as_secs_f64().max(1e-3));
        }
        let (sw, sh) = r.scene_size(s.config.width, s.config.height);
        log::info!(
            "profile: window {}x{}, scene drawn at {sw}x{sh}, {}x MSAA",
            s.config.width,
            s.config.height,
            r.options.msaa
        );
        let frames = crate::perf_report::frame_summary(&self.perf.frame_times);
        log::info!("{}", crate::perf_report::log_line(&frames));
        if let Some(path) = omsi_cfg::flags::OMSI_PROFILE_JSON.var() {
            let report = crate::perf_report::report(crate::perf_report::Run { perf: &self.perf, settings: &self.settings, args: &self.args, lan: self.net.lan.is_some() }, r, (s.config.width, s.config.height), (sw, sh), frames);
            match serde_json::to_vec_pretty(&report).map_err(anyhow::Error::from).and_then(|b| Ok(std::fs::write(path, b)?)) {
                Ok(()) => log::info!("profile: the summary is in {path}"),
                Err(e) => log::warn!("profile: the summary could not be written to {path}: {e}"),
            }
        }
    }
}

/// Waits for the next frame's deadline (see [`next_deadline`]).
fn pace_until(frame_start: Instant, period: std::time::Duration) {
    use std::time::Duration;
    static NEXT: parking_lot::Mutex<Option<(Instant, Duration)>> = parking_lot::Mutex::new(None);
    static LATE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1_000_000);
    let mut next = NEXT.lock();
    let deadline = next_deadline(*next, frame_start, Instant::now(), period);
    *next = Some((deadline, period));
    drop(next);
    let late = LATE_NS.load(std::sync::atomic::Ordering::Relaxed);
    let margin = Duration::from_nanos(late.min(1_000_000));
    if let Some(rest) = deadline.checked_duration_since(Instant::now()) {
        if rest > margin {
            let wake = Instant::now() + (rest - margin);
            std::thread::sleep(rest - margin);
            let seen = Instant::now().saturating_duration_since(wake).as_nanos() as u64;
            LATE_NS.store((seen * 3 / 2).max(late - late / 8), std::sync::atomic::Ordering::Relaxed);
        }
    }
    while Instant::now() < deadline {
        std::thread::yield_now();
    }
}

/// When the frame that started at `frame_start` (and ends `now`) gives way to the next, the
/// last deadline `last` (with its period). The deadlines lie a period apart on one clock
/// (`next += period`), not a period after each frame's start: a frame's own jitter then
/// does not shift all the frames after it, and the pace stays even. A little late - a
/// quarter period - is made up in the next frame; later than that a new count starts from
/// now: making up a whole frame's delay drew the next frame at once, two frames shown
/// close together. A new limit starts its count from the frame's start.
fn next_deadline(last: Option<(Instant, std::time::Duration)>, frame_start: Instant, now: Instant, period: std::time::Duration) -> Instant {
    match last {
        Some((at, p)) if p == period => {
            let at = at + period;
            if at + period / 4 < now { now } else { at }
        }
        _ => frame_start + period,
    }
}

#[cfg(test)]
mod pace_tests {
    use super::next_deadline;
    use std::time::{Duration, Instant};

    /// Frames of uneven length keep to deadlines a period apart; a little late is made up,
    /// a long frame starts the count anew; a new limit starts from the frame's start.
    #[test]
    fn deadlines_keep_one_clock() {
        let ms = |x: u64| Duration::from_millis(x);
        let (t0, p) = (Instant::now(), ms(10));
        let d1 = next_deadline(None, t0, t0 + ms(3), p);
        assert_eq!(d1, t0 + ms(10));
        // a frame of 7 ms or of 2 ms: the next deadline is still 10 ms on
        assert_eq!(next_deadline(Some((d1, p)), d1, d1 + ms(7), p), t0 + ms(20));
        assert_eq!(next_deadline(Some((d1, p)), d1, d1 + ms(2), p), t0 + ms(20));
        // 2 ms late: kept (the next frame makes it up)
        assert_eq!(next_deadline(Some((d1, p)), d1, d1 + ms(12), p), t0 + ms(20));
        // 5 ms late: a new count from now
        assert_eq!(next_deadline(Some((d1, p)), d1, d1 + ms(15), p), d1 + ms(15));
        // another limit
        assert_eq!(next_deadline(Some((d1, p)), d1, d1 + ms(1), ms(20)), d1 + ms(20));
    }
}
