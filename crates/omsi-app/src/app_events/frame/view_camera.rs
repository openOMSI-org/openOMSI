//! The view from the player's bus in the window's frame: the seat, head tracking, the
//! zoom, the eased return (F1) and the smooth switch between the cab's cameras.

use super::*;

impl App {
    /// The camera of the bus's views (the field of view only for the free camera and on
    /// foot).
    pub(super) fn frame_view_camera(&mut self, dt: f32) {
        if self.view != "free" && self.view != "foot" {
            let Some(p) = self.player.as_mut() else { return };
            // (the key made only when the view changed: `swap_view_look` does nothing else)
            if !crate::input_script::look_key_is(&self.cam.look_view, &self.view, Some(p.cam_choice)) {
                let key = crate::input_script::look_key_of(&self.view, Some(p.cam_choice));
                let left = self.cam.look_view.clone();
                crate::input_script::swap_view_look(&mut self.cam.look, &mut self.cam.view_looks, &mut self.cam.look_view, &key);
                crate::app_impl::swap_cam_zoom(&mut self.cam.view_zoom, &mut self.cam.cam_zooms, &left, &key, &self.view);
            }
            if let Some(cam) = self.camera.as_ref() {
                let cam = *cam;
                // (the seat kept for this bus, when one is: see `bus_seats`)
                let bus = crate::game_lists::seat_key(p);
                if bus != self.session.seat_bus {
                    if let Some((seat, pitch)) = crate::settings::bus_seats::of(&bus) {
                        self.settings.seat = seat;
                        self.settings.seat_pitch_deg = pitch;
                    } else {
                        let saved = crate::settings::Settings::load();
                        self.settings.seat = saved.seat;
                        self.settings.seat_pitch_deg = saved.seat_pitch_deg;
                    }
                    // (the view editor's turn and field of view kept for it, `views.cfg`)
                    let def = &p.vehicle.ty.def;
                    let std = def.cameras_driver.get(def.camera_std % def.cameras_driver.len().max(1));
                    (self.cam.view_yaw, self.cam.view_fov) = std.map_or((0.0, None), |c| crate::game_panels::view_editor::stored(&bus, c));
                    self.session.seat_bus = bus;
                }
                p.seat = glam::Vec3::from_array(self.settings.seat);
                // head tracking: the head's turn on top of the look, its movement
                // on top of the seat (opentrack: x left, y up, z back, in cm; the
                // eye moved by HeadPose::seat_offset)
                // (a port that cannot be had is tried again now and then, the
                // setting stays on: turning it off here undid the switch in the
                // menu at once)
                if self.settings.head_tracking && self.input.headtrack.is_none() && self.input.headtrack_failed.is_none_or(|t| t.elapsed().as_secs_f32() > 5.0) {
                    let hwnd = self.window.as_ref().and_then(|window| crate::controllers::window_handle(window));
                    let tobii = crate::tobii::Options { edges: self.settings.tobii_edges, ..Default::default() };
                    self.input.headtrack = crate::headtrack::HeadTracker::start_with(self.settings.head_tracking_port, &self.settings.head_tracking_source, tobii, hwnd);
                    self.input.headtrack_failed = self.input.headtrack.is_none().then(std::time::Instant::now);
                }
                if let Some(h) = self.input.headtrack.as_ref() {
                    h.set_tobii(crate::tobii::Options { edges: self.settings.tobii_edges, ..Default::default() });
                }
                if !self.settings.head_tracking {
                    self.input.headtrack_scale_last = None;
                    self.input.headtrack_scale_bias = [0.0; 6];
                    self.input.headtrack_invert_last = None;
                }
                // (the tracker's latest pose made calm frame by frame - glided between the
                // tracker's poses, a Tobii tracker's trembling filtered out, see
                // `crate::head_filter`; with head tracking switched off or the tracker lost the
                // view glides home - then the sensitivities and inversions)
                let sample = self.input.headtrack.as_ref().and_then(|h| h.sample()).filter(|_| self.settings.head_tracking);
                let calm = crate::head_filter::Params::tobii(self.settings.head_tracking_smoothing, self.settings.head_tracking_dead_zone);
                let tracked = self.input.head_filter.step(sample.as_ref(), std::time::Instant::now(), calm).filter(|_| matches!(self.view.as_str(), "driver" | "pax"));
                #[cfg(windows)]
                let vr_on = self.xr.vr.is_some();
                #[cfg(not(windows))]
                let vr_on = false;
                // Camera smoothing uses frame time, not the head physics' clamped step.
                // Physical head tracking controls the view without an added automatic turn.
                p.steer_look = if vr_on || tracked.is_some() { 0.0 } else {
                    crate::player::steering_view_yaw(p.steer_look, p.vehicle.physics.controls.steering, dt,
                                                     self.settings.steer_look && crate::player::steering_follows(&self.view, p.cam_choice), self.settings.steer_look_angle, self.settings.steer_look_response)
                };
                let mut tracked_rot = None;
                if let Some(t) = tracked {
                    let t = self.head_track_adjust(t);
                    tracked_rot = Some(t.rot);
                    if let Some(p) = self.player.as_mut() {
                        p.seat += t.seat_offset();
                    }
                }
                // (the outside view's field of view starts from the plain 60
                // degrees every frame: taken from the last frame's camera, the
                // zoom was applied on top of itself and ran off to its narrowest
                // or widest at once)
                let prev_cam = cam;
                let base = omsi_render::Camera { fov_deg: 60.0, ..cam };
                // Where the view is drawn: the way the mouse (or the stick, or the
                // arrow keys) turned the head is eased in, so the picture glides to
                // the angle asked for instead of jumping to it (off by default); the
                // seat's head pitch goes on top of it.
                let look = crate::input_script::ease_look(
                    &mut self.cam.look_smooth,
                    self.cam.look,
                    dt,
                    self.settings.look_smoothing_ms,
                );
                let head_look = crate::player::driver_head_look(
                    look,
                    &self.view,
                    self.settings.seat_pitch_deg,
                    vr_on,
                );
                // the view editor's turn and field of view: the standard driver's camera's
                let own_view = self.view == "driver" && self.player.as_ref().is_some_and(|p| p.cam_choice.0 == 0) && !vr_on;
                let head_look = if own_view { (head_look.0 + self.cam.view_yaw, head_look.1) } else { head_look };
                let view_fov = self.cam.view_fov.filter(|_| own_view);
                // what turns the bus's own camera into the picture: the head's turn,
                // the field of view setting and the zoom (for the camera left in a
                // switch as well as for the one taken)
                let fov_setting = self.settings.fov;
                // Eased Space return (F1): look + zoom glide home on the
                // same ease-out as the viewpoint switch instead of
                // teleporting - ahead of the zoom read below, so the
                // frame draws this frame's zoom, not the last one's.
                // The glide belongs to the camera it started from:
                // a switch mid-glide finalizes that camera straight
                // ahead instead of saving a partial angle, and
                // leaving the view drops it the same way.
                self.f1_return(dt);
                let Some(p) = self.player.as_mut() else { return };
                let zoom = self.cam.view_zoom.get(&self.view).copied();
                // The sway's own turn of the view: the driver's view only, for it is
                // his head (and nothing at all while the sway is off or driven by a
                // head tracker - `head_idle` is still then).
                let idle_rot = (self.view == "driver" && !p.head_idle.is_still()).then(|| [p.head_idle.yaw, p.head_idle.pitch, p.head_idle.roll]);
                let finish = move |c: &mut omsi_render::Camera| {
                    if let Some(r) = tracked_rot {
                        c.yaw += r[0].clamp(-170.0, 170.0);
                        c.pitch = (c.pitch + r[1].clamp(-80.0, 80.0)).clamp(-89.0, 89.0);
                        c.roll += r[2].clamp(-60.0, 60.0);
                    }
                    if let Some(r) = idle_rot {
                        c.yaw += r[0];
                        c.pitch = (c.pitch + r[1]).clamp(-89.0, 89.0);
                        c.roll += r[2];
                    }
                    // Settings → Field of view (0: the bus's own cameras); the view editor's for
                    // this bus before it
                    if let Some(f) = view_fov {
                        c.fov_deg = f.min(120.0);
                    } else if fov_setting >= 20.0 {
                        c.fov_deg = fov_setting.min(120.0);
                    }
                    if let Some(z) = zoom {
                        c.fov_deg = (c.fov_deg * z).clamp(8.0, 120.0);
                    }
                };
                let mut cam = p.camera_look(&self.view, &base, head_look, self.cam.orbit);
                finish(&mut cam);
                // Smooth cockpit camera switch (arrow keys): the camera left comes closer to the one
                // taken in the bus's own frame each frame, as in Omsi.exe (`cam_approach`); the bus's
                // motion and the head go on top afterwards, so nothing of the last frame's picture is
                // needed.
                let mut cam = self.blend_cab_camera(cam, prev_cam, head_look, zoom, &finish, dt);
                let Some(p) = self.player.as_mut() else { return };
                if self.view == "outside" && self.settings.camera_collision {
                    if let Some(w) = self.world.as_ref() {
                        cam = p.camera_clipped(cam, w, self.cam.orbit, dt);
                    }
                } else {
                    p.arm.reset();
                }
                self.camera = Some(cam);
            }
        } else if let Some(cam) = self.camera.as_mut() {
            // the free camera and the view on foot follow the setting too (they
            // stayed at 60 degrees whatever it said)
            let base = if self.settings.fov >= 20.0 { self.settings.fov.min(120.0) } else { 60.0 };
            cam.fov_deg = (base * self.cam.view_zoom.get(&self.view).copied().unwrap_or(1.0)).clamp(8.0, 120.0);
        }
    }

    /// Head tracking's pose with the six sensitivity sliders and the inversions applied.
    fn head_track_adjust(&mut self, mut t: crate::headtrack::HeadPose) -> crate::headtrack::HeadPose {
        // TrackIR/NPClient reports an absolute pose. Apply the six
        // user-facing sensitivity controls, but when a sensitivity
        // slider changes while the head is stationary, compensate the
        // already displayed output so the camera does not jump.
        // Inversion is deliberately NOT compensated: it only changes
        // direction, and switching it back restores the original pose.
        let mut scales = [0.0_f32; 6];
        scales[0] = (self.settings.head_tracking_x_sens / 100.0).clamp(0.0, 1.0).powi(2);
        scales[1] = (self.settings.head_tracking_y_sens / 100.0).clamp(0.0, 1.0).powi(2);
        scales[2] = (self.settings.head_tracking_z_sens / 100.0).clamp(0.0, 1.0).powi(2);
        scales[3] = (self.settings.head_tracking_yaw_sens / 100.0).clamp(0.0, 1.0).powi(2);
        scales[4] = (self.settings.head_tracking_pitch_sens / 100.0).clamp(0.0, 1.0).powi(2);
        scales[5] = (self.settings.head_tracking_roll_sens / 100.0).clamp(0.0, 1.0).powi(2);

        let legacy = ["yaw", "pitch", "roll"];
        let invert = [
            self.settings.head_tracking_invert_x,
            self.settings.head_tracking_invert_y,
            self.settings.head_tracking_invert_z,
            self.settings.head_tracking_invert_yaw || self.settings.head_tracking_invert.contains(legacy[0]),
            self.settings.head_tracking_invert_pitch || self.settings.head_tracking_invert.contains(legacy[1]),
            self.settings.head_tracking_invert_roll || self.settings.head_tracking_invert.contains(legacy[2]),
        ];
        for k in 0..6 {
            if invert[k] { scales[k] = -scales[k]; }
        }

        let raw = [t.pos[0], t.pos[1], t.pos[2], t.rot[0], t.rot[1], t.rot[2]];
        let invert_changed = self.input.headtrack_invert_last.is_some_and(|previous| previous != invert);
        if invert_changed {
            self.input.headtrack_scale_bias = [0.0; 6];
        } else if let Some(previous) = self.input.headtrack_scale_last {
            let sensitivity_changed = (0..6).any(|k| (previous[k].abs() - scales[k].abs()).abs() > f32::EPSILON);
            if sensitivity_changed {
                for k in 0..6 {
                    let old_output = raw[k] * previous[k] + self.input.headtrack_scale_bias[k];
                    self.input.headtrack_scale_bias[k] = old_output - raw[k] * scales[k];
                }
            }
        } else {
            self.input.headtrack_scale_bias = [0.0; 6];
        }
        self.input.headtrack_scale_last = Some(scales);
        self.input.headtrack_invert_last = Some(invert);

        let adjusted = [
            raw[0] * scales[0] + self.input.headtrack_scale_bias[0],
            raw[1] * scales[1] + self.input.headtrack_scale_bias[1],
            raw[2] * scales[2] + self.input.headtrack_scale_bias[2],
            raw[3] * scales[3] + self.input.headtrack_scale_bias[3],
            raw[4] * scales[4] + self.input.headtrack_scale_bias[4],
            raw[5] * scales[5] + self.input.headtrack_scale_bias[5],
        ];
        t.pos = [adjusted[0], adjusted[1], adjusted[2]];
        t.rot = [adjusted[3], adjusted[4], adjusted[5]];
        t
    }

    /// The eased return of the look and the zoom (F1), for the camera it started from.
    fn f1_return(&mut self, dt: f32) {
        let Some(cam_choice) = self.player.as_ref().map(|p| p.cam_choice) else { return };
        let cur = crate::input_script::look_key_of(&self.view, Some(cam_choice));
        let mine = matches!(&self.cam.f1_reset, Some((.., k)) if *k == cur);
        if self.view == "driver" && mine {
            if let Some((look_from, zoom_from, t, _)) = self.cam.f1_reset.clone() {
                let (look, zoom, done) =
                    crate::input_script::reset_blend(look_from, zoom_from, t + crate::cam_approach::step(dt));
                if done {
                    self.cam.look = (0.0, 0.0);
                    self.cam.view_zoom.remove(&self.view);
                    self.cam.f1_reset = None;
                } else {
                    self.cam.look = look;
                    self.cam.view_zoom.insert(self.view.clone(), zoom);
                    self.cam.f1_reset = Some((look_from, zoom_from, t + crate::cam_approach::step(dt), cur));
                }
            }
        } else if let Some((.., key)) = self.cam.f1_reset.take() {
            // camera changed mid-glide, or F1 left: the return
            // is done for the camera it started from - store
            // it straight ahead, never a partial angle.
            self.cam.view_looks.insert(key, (0.0, 0.0));
            self.cam.view_zoom.remove("driver");
        }
    }

    /// The glide between the cab's cameras: `cam`, the camera taken, mixed with the one left
    /// (`prev_cam` the last frame's).
    fn blend_cab_camera(
        &mut self,
        cam: Camera,
        prev_cam: Camera,
        head_look: (f32, f32),
        zoom: Option<f32>,
        finish: &dyn Fn(&mut Camera),
        dt: f32,
    ) -> Camera {
        let mut cam = cam;
        let Some(p) = self.player.as_ref() else { return cam };
        let inside_view = self.view == "driver";
        let entering = std::mem::take(&mut self.cam.cam_blend.entering);
        let left = self
            .cam.cam_blend
            .key
            .as_ref()
            .is_some_and(|k| k.0 == self.view && k.1 .0 != p.cam_choice.0);
        let target = if inside_view { p.driver_local(head_look) } else { None };
        let mut started = false;
        if let Some(to) = target.as_ref() {
            if (entering || left) && self.settings.driverview_smooth {
                let from = if entering {
                    // (what `driver_world` adds to every frame - the head and the seat - is
                    // taken off the walker's eyes, and the zoom `finish` applies again off
                    // its field of view: the first frame then is the walker's picture)
                    let mut f = p.local_of_world(&prev_cam);
                    let head = p.head_offset();
                    f.pos[0] -= head.x + p.seat.x;
                    f.pos[1] -= head.y + p.seat.y;
                    f.pos[2] -= head.z + p.seat.z;
                    if let Some(z) = zoom.filter(|z| *z > 0.0) {
                        f.fov /= z;
                    }
                    Some(f)
                } else {
                    self.cam.cam_blend.shown.clone()
                };
                if let Some(from) = from {
                    let d = glam::Vec3::from_array(from.pos) - glam::Vec3::from_array(to.pos);
                    // (a far jump is another bus, not another camera of this one)
                    if d.length() < 25.0 {
                        self.cam.cam_blend.from = Some(from);
                        started = true;
                    }
                }
            }
        }
        self.cam.cam_blend.key = Some((self.view.clone(), p.cam_choice));
        let mut shown = target.clone();
        let from_now = self.cam.cam_blend.from.clone();
        match (target.as_ref(), from_now.as_ref()) {
            (Some(to), Some(from)) => {
                // (the frame that starts the glide does not count)
                let now = if started { from.clone() } else { crate::cam_approach::approach(from, to, dt) };
                let mut here = p.driver_world(&now);
                finish(&mut here);
                if crate::cam_approach::settled(&now, to) {
                    // (the hand-over to the plain camera: what is left of the way, and what
                    // the two ways of making the camera might still differ in, is eased out)
                    self.cam.cam_blend.carry = Some(crate::app::CamCarry::between(&here, &cam));
                    self.cam.cam_blend.from = None;
                } else {
                    cam = here;
                    self.cam.cam_blend.from = Some(now.clone());
                    shown = Some(now);
                }
            }
            _ => self.cam.cam_blend.from = None,
        }
        self.cam.cam_blend.shown = shown;
        if started || !inside_view {
            self.cam.cam_blend.carry = None;
        }
        // the zoom glides with the camera: from the one drawn on the camera left to the one
        // the camera taken keeps, on the same curve (it jumped at the first frame, #2306)
        let zoom_now = zoom.unwrap_or(1.0);
        if started && !entering {
            self.cam.cam_blend.zoom_from = self.cam.cam_blend.zoom_drawn;
        } else if !inside_view {
            self.cam.cam_blend.zoom_from = None;
        }
        let drawn = match self.cam.cam_blend.zoom_from {
            Some(z) => {
                let z = if started { z } else { zoom_glide(z, zoom_now, dt) };
                self.cam.cam_blend.zoom_from = ((z - zoom_now).abs() > 1e-3).then_some(z);
                cam.fov_deg = (cam.fov_deg * z / zoom_now.max(0.01)).clamp(8.0, 120.0);
                z
            }
            None => zoom_now,
        };
        self.cam.cam_blend.zoom_drawn = Some(drawn);
        if let Some(c) = self.cam.cam_blend.carry.as_mut() {
            c.apply(&mut cam);
            if !c.decay(dt) {
                self.cam.cam_blend.carry = None;
            }
        }
        cam
    }
}

/// The zoom one frame of `dt` closer to `to` on the cab camera glide's curve (`cam_approach`).
fn zoom_glide(from: f32, to: f32, dt: f32) -> f32 {
    to - (to - from) * crate::cam_approach::left_after(dt)
}

#[cfg(test)]
mod zoom_glide_tests {
    /// The zoom of the camera left glides to the camera taken's within the glide's time,
    /// instead of jumping at its first frame (#2306).
    #[test]
    fn the_zoom_glides_to_the_camera_taken() {
        let mut z = 0.5;
        let mut prev = z;
        for _ in 0..60 {
            z = super::zoom_glide(z, 1.0, 1.0 / 60.0);
            assert!(z > prev && z <= 1.0);
            prev = z;
        }
        // a second: as good as there (e^-5 of the way left)
        assert!((1.0 - z) < 0.01, "{z}");
    }
}
