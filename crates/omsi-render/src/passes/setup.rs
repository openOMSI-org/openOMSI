//! The start of a frame: the test hooks and fallbacks, the frame's modes and targets, and
//! the camera's uniforms.

use super::*;

const AO_PREPASS_REACH: f32 = 250.0;

impl Renderer {
    /// The test hooks and the fallbacks after a GPU error. False: the device is gone and
    /// nothing is drawn.
    pub(crate) fn frame_hooks(&mut self, scene: &mut Scene, env: &FrameEnv, with_overlays: bool) -> bool {
        #[cfg(not(feature = "test-hooks"))]
        let _ = (env, with_overlays);
        // test hook for a lost device (a driver reset): its resources are taken away and
        // the session has to end in order
        #[cfg(feature = "test-hooks")]
        if env.fake_gpu_error == Some("lost")
            && with_overlays
            && self.started.elapsed().as_secs_f32() > 3.0
            && self.device_lost().is_none()
        {
            log::error!("the graphics device was lost (test): OMSI_FAKE_GPU_ERROR=lost");
            self.device.destroy();
            *self.errors.device_lost.lock().unwrap_or_else(|e| e.into_inner()) = Some("test".into());
        }
        // the device is gone: nothing can be drawn, and the readbacks (the exposure meter)
        // would find their buffers taken away - "Error in Buffer::get_mapped_range:
        // Validation Error" ended the game instead of the session ending in order
        if self.errors.device_lost.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return false;
        }
        #[cfg(feature = "test-hooks")]
        if env.fake_gpu_error == Some("frame")
            && self.options.msaa > 1
            && self.started.elapsed().as_secs_f32() > 3.0
        {
            // test hook for the fallback below
            let _ = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("invalid"),
                size: wgpu::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 3,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
        }
        if self.errors.rt_error.load(std::sync::atomic::Ordering::Relaxed) {
            self.fall_back_without_ray_tracing(scene);
        } else if self.errors.gpu_error.load(std::sync::atomic::Ordering::Relaxed) {
            self.fall_back_to_single_sample(scene);
        }
        true
    }

    /// The frame's modes (render scale, Enhanced, the reflections, the prepass, ray
    /// tracing) and the targets they need, the lights, coronas and smoke prepared.
    pub(crate) fn begin_frame<'a>(&mut self, scene: &mut Scene, env: FrameEnv, a: FrameArgs<'a>, clock: &mut StageClock) -> FrameCtx<'a> {
        let FrameArgs { target, width, height, camera, lighting, with_overlays, exclude_texture, projection, second_eye } = a;
        // render origin: the camera position rounded to 100 m, kept while the camera stays near it
        if (camera.position - scene.render_origin).abs().max_element() > 200.0 {
            self.set_render_origin(scene, (camera.position / 100.0).floor() * 100.0);
        }
        let ro = scene.render_origin;
        // The window's 3D picture may be drawn smaller and scaled up to it (render scale):
        // from here on `width` and `height` are the size of the picture, `full_*` the
        // window's (the HUD is drawn at that size). Mirrors keep their own size.
        let (full_w, full_h) = (width, height);
        let (width, height) = if with_overlays {
            self.scene_size(full_w, full_h)
        } else if let Some(s) = self.xr_fsr_scale().filter(|_| projection.is_some()) {
            // (a VR eye with FSR 1: drawn smaller, scaled up to the headset's image)
            (((full_w as f32 * s).round() as u32).max(1), ((full_h as f32 * s).round() as u32).max(1))
        } else {
            (full_w, full_h)
        };
        // FXAA on the plain graphics too, where there is no multisampling to smooth the
        // edges (the Enhanced path has it in its post passes): the picture is drawn into a
        // texture of the window's size and smoothed on its way to the window, the HUD after
        let vanilla_fxaa = with_overlays
            && (width, height) == (full_w, full_h)
            && self.options.fxaa
            && self.options.msaa <= 1
            && !(lighting.enhanced && self.hdr_pass.is_some() && !env.no_enhanced)
            && !env.no_fxaa;
        // Rain films are drawn after copying the clean current-frame scene. Classic
        // graphics need a sampleable scene target too, even without puddles or scaling.
        let enhanced_view = lighting.enhanced && self.hdr_pass.is_some() && !env.no_enhanced;
        let glass_on = with_overlays
            && scene.glass_slot.is_some()
            && (lighting.rain > 0.001 || lighting.wetness > 0.02)
            && !env.no_glass_picture;
        let scaled = (width, height) != (full_w, full_h) || vanilla_fxaa || (glass_on && !enhanced_view);
        let scene_target: Option<(wgpu::TextureView, wgpu::BindGroup)> = if scaled {
            Some(self.scale_target(width, height))
        } else {
            None
        };
        let aspect = self.texture_aspect.unwrap_or(width as f32 / height.max(1) as f32);
        let cam_rel = (camera.position - ro).as_vec3();
        // The mirrors are drawn with the plain shading even in Enhanced: a mirror has no depth
        // prepass (sized for the window), so every layer of its picture runs the enhanced shader
        // - 12.7 ms of GPU time for one 256-pixel mirror against 5.8 ms for the whole window on a
        // weak card; plainly shaded it is 0.6 ms, and a mirror's small picture shows no
        // difference worth that - unless the player asks for it (`mirror_quality=enhanced`,
        // `RenderOptions::mirror_enhanced`) or `OMSI_MIRROR_ENHANCED=1`.
        // The headset's eyes are the real picture as much as the window is (#784: VR showed
        // the plain graphics with Enhanced on); the first eye is the one that moves the
        // exposure, the sky cube and the frame clock on, as the window does without VR.
        let xr_view = projection.is_some();
        let eye_color = enhanced_view && !with_overlays && xr_view && self.hdr_eye_pass.is_some();
        let lead_view = (with_overlays || xr_view) && !second_eye;
        let enhanced_frame = lighting.enhanced && self.hdr_pass.is_some() && !env.no_enhanced && (with_overlays || xr_view || env.mirror_enhanced || self.options.mirror_enhanced);
        // the mirrors are drawn by the same path as the window (their picture graded with
        // the window's exposure, see the post passes)
        let enhanced = enhanced_frame;
        // Rain reflections belong to all graphics modes. Classic shading uses the same
        // scene/mask targets, then presents their linear colour without Enhanced grading.
        let puddles_wanted = with_overlays
            && self.puddles.is_some()
            && self.options.reflections
            && lighting.puddles > 0
            && lighting.wetness * (1.0 - lighting.snow.clamp(0.0, 1.0)) > 0.05
            && scene.materials.iter().any(|m| m.uniform.params2[2] > 0.0)
            && debug_view() == 0.0
            && !env.no_puddle_reflections;
        let reflection_frame = !enhanced && puddles_wanted && self.reflection_pass.is_some();
        let masked_frame = enhanced || reflection_frame;
        let eye = LightEye {
            cam_rel,
            view_proj: projection.is_none().then(|| camera.view_proj(aspect, ro)),
            vehicle: lighting.inside.as_ref().map(|b| (b, lighting.inside_rotation)),
        };
        let (grid, lamp_shadows) = self.prepare_lights(scene, &eye, enhanced_frame, lighting.lamp_shadows && with_overlays && projection.is_none() && self.shadows.shadow_pipelines.len() > 6);
        self.prepare_coronas(scene, lighting.night, lighting.inside.as_ref().filter(|v| point_in_vehicle_box(camera.position, v)));
        // (the mirrors draw no smoke, see `encode_main_late`)
        if exclude_texture.is_none() {
            self.prepare_smoke(scene, camera.position);
        }
        // ambient occlusion only for the real picture, not for the mirrors
        // Enhanced+: the window's picture traces its sun shadow and ambient occlusion (not the
        // mirrors, a headset's eyes or a triple screen's panels: they keep the shadow map)
        let rt_frame = self.rt.is_some() && enhanced && with_overlays && projection.is_none() && !env.no_rt_frame;
        let ao_on = with_overlays && self.rt.is_none() && self.options.ssao && self.ssao_pipeline.is_some() && !env.no_ao;
        // the enhanced path's shading is costly: the depth prepass keeps it to the visible
        // surface (without multisampling, see `share_depth`)
        let depth_wanted = puddles_wanted || glass_on || (enhanced && (with_overlays || xr_view)) || (self.depth_readback && projection.is_none());
        let prepass_on = ao_on || depth_wanted;
        // the terrain's paint laid into one albedo picture first: the window's enhanced
        // picture, single-sampled, with the camera group's slot 8 free of the AO
        let ground_ok = with_overlays && enhanced && projection.is_none() && !second_eye && !ao_on && !rt_frame && prepass_on && self.options.msaa <= 1
            && exclude_texture.is_none() && !env.no_ground_pass && self.ground_eligible();
        let prepass_reach = if depth_wanted || env.no_ao_reach { f32::MAX } else { AO_PREPASS_REACH };
        // (only the bind group is made again, by `upload_draw_list`: dropping the instance
        // buffers for it wrote every instance's entries again at each render-scale step)
        if prepass_on && self.ensure_ao(width, height, crate::lighting::bounce_wanted(ao_on, enhanced_frame, self.street_light())) {
            // a new AO texture: the camera bind group must point at it
            scene.camera_bind_group = None;
            self.targets.hdr_targets.clear();
        }
        // (the ground albedo picture only while that pass can run)
        if prepass_on && self.ensure_ground(width, height, self.ground_eligible()) {
            scene.camera_bind_group = None;
        }
        // (slot 8 holds the AO's picture or the ground albedo's: the group is made again when that changes)
        if prepass_on && self.ground_bound.get() != self.ground_should_bind() {
            scene.camera_bind_group = None;
        }
        if rt_frame && self.rt.as_mut().is_some_and(|rt| rt.ensure_targets(&self.device, width, height)) {
            // the camera bind group must point at the new lighting texture
            scene.camera_bind_group = None;
        }
        if masked_frame {
            self.hdr_targets(width, height, rt_gbuf() && !eye_color);
        }
        if glass_on {
            self.prepare_glass_behind(scene, width, height, if masked_frame { HDR_FORMAT } else { self.format });
        }
        let dt = {
            let now = std::time::Instant::now();
            let dt = self
                .last_frame
                .map(|t| (now - t).as_secs_f32())
                .unwrap_or(0.0);
            if lead_view {
                self.last_frame = Some(now);
            }
            dt
        };
        clock.stage(self, "setup", "mirror.setup");
        self.prepare(scene);
        clock.stage(self, "prepare", "mirror.prepare");
        // overlay uniforms/bind groups (rects in pixels → NDC)
        let overlays: Vec<(TextureId, [f32; 4])> = if with_overlays {
            scene.overlays.clone()
        } else {
            Vec::new()
        };
        if with_overlays {
            self.prepare_overlays(scene, full_w, full_h);
        }
        FrameCtx {
            env,
            target,
            camera,
            lighting,
            with_overlays,
            exclude_texture,
            projection,
            second_eye,
            ro,
            full_w,
            width,
            height,
            vanilla_fxaa,
            glass_on,
            scaled,
            scene_target,
            aspect,
            cam_rel,
            xr_view,
            eye_color,
            lead_view,
            enhanced_frame,
            enhanced,
            puddles_wanted,
            reflection_frame,
            masked_frame,
            grid,
            lamp_shadows,
            rt_frame,
            ao_on,
            ground_ok,
            prepass_on,
            prepass_reach,
            dt,
            overlays,
        }
    }

    /// The camera's uniforms (and the light maps' place), the ray tracing's preparation,
    /// the lamps' light on the view and the sky. Returns the view-projection matrix, the
    /// uniform and whether the reflection probe is redrawn this frame.
    pub(crate) fn frame_uniforms(&mut self, scene: &Scene, f: &FrameCtx, sh: &ShadowPlan, clock: &mut StageClock) -> (Mat4, CameraUniform, bool) {
        let (camera, lighting, ro, cam_rel, aspect) = (f.camera, f.lighting, f.ro, f.cam_rel, f.aspect);
        let (enhanced, lead_view, rt_frame, ao_on, glass_on, shadows) = (f.enhanced, f.lead_view, f.rt_frame, f.ao_on, f.glass_on, sh.shadows);
        let lamp_shadows = &f.lamp_shadows;
        // The lamps' matrices: the window's frame draws the tiles and keeps what it drew them
        // with; a mirror's picture looks the same tiles up, as it looks the sun's cascades up.
        let lamp_mats: Vec<Mat4> = if f.with_overlays && f.projection.is_none() {
            let m: Vec<Mat4> = (0..LAMP_SHADOWS).map(|k| lamp_shadows.iter().find(|l| l.slot == k).map_or(Mat4::ZERO, |l| l.view_proj())).collect();
            self.shadows.lamp_matrices.borrow_mut().clone_from(&m);
            m
        } else {
            self.shadows.lamp_matrices.borrow().clone()
        };
        let vp_mat = f
            .projection
            .map(|p| p * Mat4::look_to_rh((camera.position - ro).as_vec3(), camera.forward(), camera.up()))
            .unwrap_or_else(|| camera.view_proj(aspect, ro));
        // the last frame's picture for the bounced light, found again under this frame's view
        if lead_view {
            let size = (f.width, f.height);
            self.tone.bounce_reproj = if crate::lighting::bounce_wanted(ao_on, enhanced, self.street_light()) {
                let view = Mat4::look_to_rh(cam_rel, camera.forward(), camera.up());
                crate::lighting::bounce_history(self.tone.bounce_prev, view, ro, size, cam_rel, camera.forward(), f.dt)
            } else {
                (Mat4::ZERO.to_cols_array_2d(), 0.0)
            };
            self.tone.bounce_prev = crate::lighting::bounce_wanted(ao_on, enhanced, self.street_light()).then_some((vp_mat, ro, size, cam_rel, camera.forward()));
        }
        // where the tile light maps lie, relative to the render origin
        {
            let (lx, ly, side) = self.lights.lm_place.get();
            let v: [f32; 4] = [(lx - ro.x) as f32, (ly - ro.y) as f32, side as f32, if side > 0.0 { 1.0 } else { 0.0 }];
            self.queue.write_buffer(&self.lights.lm_uniform, 0, bytemuck::cast_slice(&v));
        }
        {
            let v = &lighting.vanilla_sky;
            let u = VanillaSkyUniform {
                cloud: [v.cloud_height, v.cloud_size, v.cloud_offset[0], v.cloud_offset[1]],
                haze: [v.fog_range, v.visibility, if v.overcast { 1.0 } else { 0.0 }, ro.z as f32],
            };
            self.queue.write_buffer(&self.sky.vanilla_sky_buf, 0, bytemuck::bytes_of(&u));
        }
        let cu = CameraUniform {
            post: [
                if enhanced { 1.0 } else { 0.0 },
                lighting.animation_time.unwrap_or_else(|| self.started.elapsed().as_secs_f32()),
                // (z: the roads are kept clear of the snow, `Lighting::roads_clear`)
                if lighting.roads_clear { 1.0 } else { 0.0 },
                // (the sun's height on the screen is read by no shader any more: the close
                // shadow map's share of its half of the atlas)
                self.options.shadow_size.min(SHADOW_CLOSE_MAX) as f32 / self.options.shadow_size.max(1) as f32,
            ],
            view_proj: vp_mat.to_cols_array_2d(),
            cam_pos: cam_rel.extend(lighting.reflection_strength.clamp(0.0, 1.0)).to_array(),
            // (modulo the shaders' PATTERN_PERIOD, 1000 m: the patterns repeat with it, and
            // the whole map coordinate has no precision left for them in 32 bits)
            // (zw: the origin modulo CLOUD_ORIGIN_PERIOD for the sky's clouds, which are
            // drawn over ground points: taken relative to the floating origin, the whole
            // cloud field jumped by the origin's step each time it moved on)
            world_origin: [
                ro.x.rem_euclid(1000.0) as f32,
                ro.y.rem_euclid(1000.0) as f32,
                ro.x.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32,
                ro.y.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32,
            ],
            sun_dir: lighting
                .sun_dir
                .normalize()
                .extend(lighting.sun_intensity)
                .to_array(),
            ambient: lighting
                .ambient
                .extend(lighting.snow.clamp(0.0, 1.0))
                .to_array(),
            fog: lighting.fog_color.extend(lighting.fog_density).to_array(),
            sun_color: lighting.sun_color.extend(lighting.night_maps.unwrap_or(lighting.night)).to_array(),
            sky_color: lighting
                .secondary
                .extend(if lighting.classic && !enhanced { 1.0 } else { 0.0 })
                .to_array(),
            light_grid: f.grid,
            sky: [
                lighting.sun_azimuth,
                lighting.sky_weights[0],
                lighting.sky_weights[1],
                lighting.sky_weights[2],
            ],
            clouds: [
                lighting.cloud_density,
                lighting.cloud_offset[0],
                lighting.cloud_offset[1],
                // (2: the traced lighting, full size, see `ao_at`)
                if rt_frame { 2.0 } else if ao_on { 1.0 } else { 0.0 },
            ],
            // (w: the heading the sphere maps are laid out by in the headset, see
            // `set_env_heading`; flagged by cam_up.w)
            cam_right: camera.right().extend(self.env_heading.get().map(|h| h.to_radians()).unwrap_or(0.0)).to_array(),
            cam_up: camera
                .right()
                .cross(camera.forward())
                .normalize_or_zero()
                .extend(if self.env_heading.get().is_some() { 1.0 } else { 0.0 })
                .to_array(),
            light_view_proj: sh.light_view_proj.to_cols_array_2d(),
            light_view_proj_far: sh.light_view_proj_far.to_cols_array_2d(),
            shadow: [
                if shadows { 1.0 } else { 0.0 },
                1.0 / self.options.shadow_size as f32,
                SHADOW_RANGE,
                lighting.wetness.clamp(0.0, 1.0),
            ],
            inside_a: match lighting.inside {
                Some((o, h, _)) => {
                    let r = (o - ro).as_vec3();
                    [r.x, r.y, r.z, (h as f32).to_radians().sin()]
                }
                None => [0.0; 4],
            },
            inside_b: match lighting.inside {
                Some((_, h, bb)) => [
                    (h as f32).to_radians().cos(),
                    bb[0] * 0.5,
                    bb[1] * 0.5,
                    bb[2] * 0.5,
                ],
                None => [1.0, 0.0, 0.0, 0.0],
            },
            inside_c: match lighting.inside {
                Some(v @ (_, _, bb)) => {
                    let block = !f.env.no_own_vehicle_lamp_block && point_in_vehicle_box(camera.position, &v);
                    [bb[3], bb[4], bb[5], if block { 2.0 } else { 1.0 }]
                }
                None => [0.0; 4],
            },
            flags: [
                if lighting.detail { 1.0 } else { 0.0 },
                if enhanced { 1.0 } else { 0.0 },
                // (below zero: the rain films have the clean current picture to look through,
                // see `rain_behind`; above zero is an old branch never taken)
                if glass_on { -1.0 } else { 0.0 },
                if shadows { SHADOW_RANGE_CLOSE } else { 0.0 },
            ],
            light_view_proj_close: sh.light_view_proj_close.to_cols_array_2d(),
            wind: [lighting.glass_wind.x, lighting.glass_wind.y, lighting.glass_wind.z, 1.0],
            lamp_view_proj: std::array::from_fn(|k| lamp_mats[k].to_cols_array_2d()),
            lamp_atlas: self.shadows.lamp_atlas.uniform(),
            tree_wind: {
                // the gusts drift with the wind: summed over the frames (the wind changes
                // with the weather), in f64 and modulo the shaders' pattern period
                let now = self.started.elapsed().as_secs_f64();
                let (drift, last) = self.tree_gust_drift.get();
                let drift = (drift + lighting.wind.truncate().as_dvec2() * (now - last).clamp(0.0, 1.0)).rem_euclid(glam::DVec2::splat(1000.0));
                self.tree_gust_drift.set((drift, now));
                let wind = if lighting.windy_trees { lighting.wind.truncate() } else { glam::Vec2::ZERO };
                [wind.x, wind.y, drift.x as f32, drift.y as f32]
            },
            inside2_a: match lighting.puddle_parts.first() {
                Some((o, h, _)) => {
                    let r = (*o - ro).as_vec3();
                    [r.x, r.y, r.z, (*h as f32).to_radians().sin()]
                }
                None => [0.0; 4],
            },
            inside2_b: match lighting.puddle_parts.first() {
                Some((_, h, bb)) => [(*h as f32).to_radians().cos(), bb[0] * 0.5, bb[1] * 0.5, bb[2] * 0.5],
                None => [1.0, 0.0, 0.0, 0.0],
            },
            inside2_c: match lighting.puddle_parts.first() {
                Some((_, _, bb)) if lighting.inside.is_some() => [bb[3], bb[4], bb[5], 1.0],
                _ => [0.0; 4],
            },
            cookies: self.lights.cookies.fields(),
            cabin_lamps: {
                let mut at = [-1.0f32; 16];
                for (k, l) in lamp_shadows.iter().filter(|l| l.cabin.is_some()).take(16).enumerate() {
                    at[k] = l.index as f32;
                }
                std::array::from_fn(|r| std::array::from_fn(|c| at[r * 4 + c]))
            },
            lod: [self.fsr_mip_bias(f), 0.0, 0.0, 0.0],
            spot: classic_spot(scene, ro),
        };
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cu));
        if let Some(m) = sh.precip {
            self.write_precip_camera(&cu, m);
        }
        if rt_frame {
            let proj = Mat4::perspective_rh(camera.fov_deg.to_radians(), aspect, camera.far, camera.near);
            self.prepare_ray_tracing(scene, camera, lighting, vp_mat, proj, f.width, f.height, f.dt);
            clock.stage(self, "ray tracing", "mirror.ray tracing");
        }
        // (a mirror takes the window's light - its own call would move the exposure on -
        // unless it comes before the window's first frame)
        if enhanced && lead_view {
            let ground = lighting.fog_base.or(lighting.inside.map(|v| v.0.z)).map(|z| (z - scene.render_origin.z) as f32);
            if let Some(s) = self.tone.lamp_sight.as_mut() {
                s.take();
            }
            let sight = self.tone.lamp_sight.as_ref();
            let seen = |at: DVec3| sight.map_or(1.0, |s| s.seen(at));
            self.tone.view_lamps = Some(view_lamp_light(&scene.lights, scene.render_origin, cam_rel, camera, aspect, ground, &seen));
            if let Some(s) = self.tone.lamp_sight.as_mut().filter(|s| s.idle()) {
                s.candidates = brightest_glare(&scene.lights, scene.render_origin, cam_rel, camera.forward(), vp_mat);
            }
            if f.env.debug_view_lamps {
                log::info!("view lamps: {:.6}", self.tone.view_lamps.unwrap_or(0.0));
            }
        }
        // the night sky's glow from the lamps round the camera (the window's view leads), in
        // steps of a tenth: the sky is recomputed for a new value, not for every metre driven
        if enhanced && lead_view {
            let raw = lamp_sky_glow(scene, cam_rel);
            let target = raw.clamp(0.03, 1.5);
            let step = (target.ln() * 10.0).round() / 10.0;
            let changed = self.sky.city_glow.is_none_or(|g| (g.ln() - step).abs() > 0.15);
            if changed {
                self.sky.city_glow = Some(step.exp());
            }
            if f.env.debug_sky && changed {
                log::info!("sky glow from the lamps: {raw:.3} (taken {:.3})", step.exp());
            }
        }
        // the lightning goes on for every pipeline (the sound follows it); only the Enhanced one draws it
        if lead_view {
            self.tone.lightning.seed_once(lighting.day_seed);
            let storm = crate::lighting::forced_storm().map_or(lighting.storm, |f| f.0).clamp(0.0, 1.0);
            self.tone.flash = self.tone.lightning.step(storm, f.dt);
        }
        let probe_redraw = enhanced
            && (lead_view || self.sky.sky_state.is_none())
            && self.prepare_enhanced(lighting, cam_rel, ro, f.dt);
        (vp_mat, cu, probe_redraw)
    }
}

/// The camera uniform's `spot`: the player's `[spotlight]`, the one classic spot of kind 1
/// (the scene gives the player's alone, as Omsi.exe lights the road by it alone, 0x708acc);
/// none while it is off or in a frame without a player's vehicle.
fn classic_spot(scene: &Scene, ro: DVec3) -> [[f32; 4]; 4] {
    let spot = scene.lights.iter().find_map(|l| {
        let [range, setback, kind] = l.d3d_spot?;
        (kind > 0.5 && l.mode == crate::LightMode::Vanilla && l.intensity > 0.0).then_some((l, range, setback))
    });
    let Some((l, range, setback)) = spot else { return [[0.0; 4]; 4] };
    let at = (l.position - ro).as_vec3();
    let d = l.direction.normalize_or_zero();
    let c = l.color;
    [
        [at.x, at.y, at.z, range],
        [d.x, d.y, d.z, l.cone[1]],
        [c[0] * l.intensity, c[1] * l.intensity, c[2] * l.intensity, l.cone[0]],
        [setback, 0.0, 0.0, 0.0],
    ]
}
