//! Drawing a frame: the entry points (window, triple screen, headset eyes, images), the
//! fallbacks when a device refuses a feature, and the post pass with its exposure metering.

use crate::*;

/// One single-sampled full-screen pass.
pub(crate) fn post_pass(
    encoder: &mut mx::CommandEncoder,
    view: &mx::TextureView,
    timer: Option<mx::PassTimestampWrites<'_>>,
    pipeline: &mx::RenderPipeline,
    bg: &HeapGroup,
) {
    let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
        label: Some("post"),
        color_attachments: &[Some(mx::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: mx::Operations {
                load: mx::LoadOp::Clear(mx::Color::BLACK),
                store: mx::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: timer,
        occlusion_query_set: None,
    });
    pass.set_pipeline(pipeline);
    bg.set(&mut pass);
    pass.draw(0..3, 0..1);
}

/// `OMSI_DEBUG_EXPOSURE=1`: the enhanced path's exposure as it adapts - the light model's
/// pre-exposure, the metered picture and the correction the tone mapping applies, logged
/// about four times a second (the metered value lives on the GPU and is read back).
pub(crate) struct ExposureLog {
    pub(crate) buf: mx::Buffer,
    pub(crate) device: mx::Device,
    /// The submission of the frame whose metering is being read back (see `submitted`).
    pub(crate) submission: Option<mx::SubmissionIndex>,
    pub(crate) waiting: bool,
    pub(crate) frame: u64,
    pub(crate) started: std::time::Instant,
    /// the pre-exposure (log2) and the metering settings of the frame being read back
    pub(crate) pending: (f32, [f32; 6]),
    /// The tone mapping's metering correction last read back (EV). Self-lit surfaces (a
    /// display's text, a script texture) are drawn this much brighter or darker in advance,
    /// so that they come out at their own brightness whatever the metering does to the
    /// rest of the picture: a destination display was darkened with a sunlit street and
    /// hardly readable by day.
    pub(crate) ev: f32,
    /// `OMSI_DEBUG_EXPOSURE`: log what is read.
    pub(crate) log: bool,
}

impl ExposureLog {
    pub(crate) fn new(device: &mx::Device) -> Option<ExposureLog> {
        let buf = device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("exposure readback"),
                size: 256,
                usage: mx::BufferUsages::COPY_DST | mx::BufferUsages::MAP_READ,
            })
            .expect("mX: a buffer");
        Some(ExposureLog {
            buf,
            device: device.clone(),
            submission: None,
            waiting: false,
            frame: 0,
            started: std::time::Instant::now(),
            pending: (0.0, [0.0; 6]),
            ev: 0.0,
            log: omsi_cfg::flags::OMSI_DEBUG_EXPOSURE.is_set(),
        })
    }

    pub(crate) fn sample(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        adapted: &mx::TextureView,
        pre_log2: f32,
        meter: [f32; 6],
    ) {
        self.frame += 1;
        let _ = self.device.poll();
        if self.waiting && self.submission.is_some_and(|i| self.device.is_done(i)) {
            let _ = self.buf.map_read_now(0..8, |view| {
                let bits = u16::from_le_bytes([view[0], view[1]]);
                let metered = half_to_f32(bits);
                let (pre, m) = self.pending;
                let gap = m[1] - metered;
                let ev = (gap * if gap > 0.0 { shortfall_adaptation(pre, m[3], m[0]).1 } else { m[0] }).clamp(-m[2], m[3]) + m[4];
                if ev.is_finite() {
                    self.ev = ev;
                }
                if self.log { log::info!("exposure t={:.2}s: light model {:+.2} EV, metered picture log2 {:+.2}, correction {:+.2} EV, total {:+.2} EV", self.started.elapsed().as_secs_f32(), pre, metered, ev, pre + ev); }
            });
            self.submission = None;
            self.waiting = false;
        }
        if self.waiting || self.frame % 8 != 0 {
            return;
        }
        encoder.copy_texture_to_buffer(
            mx::TexelCopyTextureInfo {
                texture: &adapted.texture(),
                mip_level: 0,
                origin: mx::Origin3d::ZERO,
                aspect: mx::TextureAspect::All,
            },
            mx::TexelCopyBufferInfo {
                buffer: &self.buf,
                layout: mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            mx::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.pending = (pre_log2, meter);
        // read once the frame's submission is done (see `submitted`)
        self.waiting = true;
        self.submission = None;
    }

    /// The frame whose commands copied the metering was submitted as `index`.
    pub(crate) fn submitted(&mut self, index: mx::SubmissionIndex) {
        if self.waiting && self.submission.is_none() {
            self.submission = Some(index);
        }
    }
}

pub(crate) fn half_to_f32(b: u16) -> f32 {
    let sign = if b & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((b >> 10) & 0x1f) as i32;
    let m = (b & 0x3ff) as f32;
    match e {
        0 => sign * m / 1024.0 * 2f32.powi(-14),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + m / 1024.0) * 2f32.powi(e - 15),
    }
}

impl Renderer {
    /// Whether the street-scale light (the wide occlusion and the bounced light) is on: the
    /// setting's, or as the chip allows (`RenderOptions::street_light`).
    pub(crate) fn street_light(&self) -> bool {
        self.options.street_light.unwrap_or(true)
    }

    /// Whether the lamps' shadow atlas holds the cube maps' tiles (made when the game started with
    /// *Light has shadows: Every light*); without them that setting takes the street lamps' level
    /// until the next start.
    pub fn lamp_cube_maps(&self) -> bool {
        self.shadows.lamp_atlas.has_cubes()
    }

    /// The lightning strikes since the last call, as (seconds until the thunder is heard,
    /// distance in m, direction): the sound plays the thunder of what the picture showed.
    pub fn take_strikes(&mut self) -> Vec<(f32, f32, [f32; 2])> {
        std::mem::take(&mut self.tone.lightning.strikes)
            .into_iter()
            .map(|s| (s.delay, s.distance, s.dir))
            .collect()
    }

    /// Change the dynamic parameters of an instance. `slot_alpha` entries beyond the mesh's
    /// slot count are ignored; missing ones keep 1.0.
    pub fn set_params(
        &self,
        scene: &mut Scene,
        instance: usize,
        slot_alpha: &[f32],
        visible: bool,
        slot_uv: &[[f32; 2]],
    ) {
        let i = &mut scene.instances[instance];
        let mut changed = i.visible != visible;
        for (k, a) in i.slot_alpha.iter_mut().enumerate() {
            let requested = slot_alpha.get(k).copied().unwrap_or(1.0);
            let v = i
                .materials
                .get(k)
                .and_then(|id| scene.materials.get(*id))
                .map_or(requested, |m| {
                    Self::clamp_slot_alpha(requested, m.alpha, m.transmap_declared())
                });
            changed |= *a != v;
            *a = v;
        }
        for (k, u) in i.slot_uv.iter_mut().enumerate() {
            let v = slot_uv.get(k).copied().unwrap_or([0.0; 2]);
            changed |= *u != v;
            *u = v;
        }
        i.visible = visible;
        if changed {
            Self::mark_changed(scene, instance);
        }
    }

    /// One step lighter on the graphics card, for a picture the card cannot keep up with at
    /// the smallest render scale: SSAO off. What was switched off, or None when it is off.
    pub fn lighten(&mut self) -> Option<&'static str> {
        if self.options.ssao {
            self.options.ssao = false;
            return Some("ambient occlusion (SSAO) off");
        }
        None
    }

    /// `Self::build` again with `options`: on ANGLE on the compiler thread of its own (see
    /// `angle::compile`: on this thread's stack the D3D compiler overflowed it).
    pub(crate) fn rebuilt(&self, options: RenderOptions) -> Renderer {
        let (device, queue, name, format) = (
            self.device.clone(),
            self.queue.clone(),
            self.adapter_name.clone(),
            self.format,
        );
        let build = move || Self::build(device, queue, name, format, options);
        if cfg!(windows) && gl_backend() && self.adapter_name.contains("ANGLE") {
            angle::compile(&self.device, build)
        } else {
            build()
        }
    }

    /// Rebuild without the ray tracing after a GPU error while it was on (see `rt_error`).
    pub(crate) fn fall_back_without_ray_tracing(&mut self, scene: &mut Scene) {
        log::error!(
            "ray tracing failed on {}; Enhanced+ draws as Enhanced from now on",
            self.adapter_name
        );
        let options = RenderOptions {
            ray_tracing: false,
            ..self.options
        };
        RT_BUFFERS.store(false, std::sync::atomic::Ordering::Relaxed);
        self.rebuild_in_place(scene, options);
    }

    /// The upscaler could not start or stopped working: back to the player's anti-aliasing.
    /// With the card out of memory (the upscaler's own textures could not be made) the
    /// player's multisampling would only take more of it - 8x MSAA after DLSS gave up, and
    /// the shadows went down to 512 - so the picture is drawn with FXAA instead, and the
    /// upscaler, kept, is tried once more when the texture budget has lowered the textures.
    pub(crate) fn fall_back_without_upscaler(&mut self, scene: &mut Scene) {
        let memory_short = crate::pipelines::errors::out_of_memory_within(
            crate::pipelines::errors::OUT_OF_MEMORY_SPELL,
        );
        if memory_short && !self.upscaler_retried && self.options.temporal.is_some() {
            log::warn!(
                "the upscaler failed with the graphics card out of memory: drawing without it, with FXAA, until it is tried once more in {} s",
                crate::pipelines::errors::OUT_OF_MEMORY_SPELL.as_secs()
            );
            self.upscaler_retry = Some((std::time::Instant::now(), self.options));
            self.rebuild_with(scene, Self::without_upscaler(self.options, true));
            return;
        }
        let options = Self::without_upscaler(self.options, memory_short);
        if memory_short {
            log::warn!(
                "drawing without the upscaler from now on, with FXAA (the graphics card is out of memory)"
            );
        } else {
            log::warn!(
                "drawing without the upscaler from now on, with {}x MSAA",
                options.msaa
            );
        }
        self.upscaler_retry = None;
        self.rebuild_with(scene, options);
    }

    /// `options` without the upscaler: the player's multisampling back, or, with the card out
    /// of memory, none and FXAA.
    pub(crate) fn without_upscaler(options: RenderOptions, memory_short: bool) -> RenderOptions {
        if memory_short {
            RenderOptions {
                temporal: None,
                msaa: 1,
                fxaa: true,
                ..options
            }
        } else {
            RenderOptions {
                temporal: None,
                msaa: options.msaa_fallback.max(1),
                msaa_fallback: 0,
                ..options
            }
        }
    }

    /// The upscaler again, once, after it failed with the card out of memory (see
    /// `fall_back_without_upscaler`): with the options it was drawn with.
    pub(crate) fn retry_upscaler(&mut self, scene: &mut Scene) {
        let Some((_, was)) = self.upscaler_retry.take() else {
            return;
        };
        self.upscaler_retried = true;
        log::info!("the upscaler is tried once more, now that the textures were lowered");
        self.rebuild_with(
            scene,
            RenderOptions {
                temporal: was.temporal,
                msaa: was.msaa,
                msaa_fallback: was.msaa_fallback,
                fxaa: was.fxaa,
                ..self.options
            },
        );
    }

    /// Rebuild the pipelines and targets without multisampling after a GPU error with it.
    /// The scene's materials stay valid (the device hands out the same bind group layout
    /// for identical entries); its per-draw buffers and camera bind group are made anew,
    /// because the camera buffer belongs to the renderer.
    pub(crate) fn fall_back_to_single_sample(&mut self, scene: &mut Scene) {
        log::error!(
            "{}x MSAA failed on {}; drawing without multisampling from now on",
            self.options.msaa,
            self.adapter_name
        );
        update_ceiling(|ceiling| {
            if let Some(c) = ceiling.as_mut() {
                c.msaa = 1;
            }
        });
        let options = RenderOptions {
            msaa: 1,
            ..self.options
        };
        self.rebuild_in_place(scene, options);
    }

    pub fn rebuild_with(&mut self, scene: &mut Scene, options: RenderOptions) -> bool {
        let options = match ceiling() {
            Some(c) => RenderOptions {
                msaa: c.msaa_for(options.msaa),
                ssao: options.ssao && c.ssao,
                shadow_size: options.shadow_size.clamp(512, c.shadow_size),
                puddle_reflections: options.puddle_reflections && c.puddle_reflections,
                ..options
            },
            None => options,
        };
        // (an upscaler takes the place of the MSAA, whatever the settings say; the player's stays
        // for when it gives up)
        let options = match options.temporal {
            Some(_) => RenderOptions {
                msaa: 1,
                msaa_fallback: if options.msaa > 1 {
                    options.msaa
                } else {
                    options.msaa_fallback
                },
                ..options
            },
            None => options,
        };
        let options = if options.ray_tracing
            && options.msaa > 4
            && !omsi_cfg::flags::OMSI_FULL_GPU.is_set()
        {
            RenderOptions { msaa: 4, ..options }
        } else {
            options
        };
        if options == self.options {
            return false;
        }
        log::info!(
            "graphics rebuilt: {}x MSAA, shadow map {}, SSAO {}",
            options.msaa,
            options.shadow_size,
            options.ssao
        );
        self.rebuild_in_place(scene, options);
        true
    }

    fn rebuild_in_place(&mut self, scene: &mut Scene, options: RenderOptions) {
        // (the same device: the upscaler goes on, and Streamline on Vulkan could not start again;
        // without one in the new options it is dropped here, with its context and textures)
        // (frame generation reads the depth, the motion and the pictures before the HUD at the
        // window's present, after wgpu is done with them: its work is waited for before they go)
        if let Some(g) = &self.frame_generation {
            g.wait();
        }
        let upscaler = self
            .temporal_runtime()
            .or_else(|| self.temporal_kept.take())
            .filter(|_| options.temporal.is_some() || self.upscaler_retry.is_some());
        // Release render targets before building their replacements. Keeping the old
        // targets alive while the new renderer is built can turn a recoverable OOM into
        // another allocation failure.
        self.targets.depth = None;
        self.pane_depth = None;
        self.targets.msaa_targets.clear();
        self.targets.ao = None;
        self.targets.hdr_targets.clear();
        self.targets.scale_targets.clear();
        self.targets.triple_targets = None;
        self.targets.glass_picture = None;
        self.targets.glass_mips = None;
        self.targets.target_use.clear();
        let (mesh_pages, frame_generation) = (self.mesh_pages, self.frame_generation.take());
        let (upscaler_retry, upscaler_retried) = (self.upscaler_retry, self.upscaler_retried);
        *self = Renderer {
            mesh_pages,
            temporal_kept: upscaler,
            frame_generation,
            upscaler_retry,
            upscaler_retried,
            ..self.rebuilt(options)
        };
        scene.dirty = true;
        scene.model_buf = None;
        scene.params_buf = None;
        scene.camera_heaps = None;
        self.remake_sky_bind_group(scene);
    }

    /// Render the scene into `target` (which must have the renderer's format).
    pub fn render(
        &mut self,
        scene: &mut Scene,
        target: &mx::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
    ) {
        // (the triple screen's panels are not kept once it is turned off)
        if self.targets.triple_targets.take().is_some() {
            self.history.triple_culling = Default::default();
        }
        self.render_inner(
            scene, target, width, height, camera, lighting, true, None, None, false,
        );
    }

    /// `render` for the window's own picture, the only one temporal upscaling draws: a photo or
    /// an off-screen picture would rebuild its targets and lose its history.
    pub fn render_window(
        &mut self,
        scene: &mut Scene,
        target: &mx::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
    ) {
        self.temporal_window = true;
        self.render(scene, target, width, height, camera, lighting);
        self.temporal_window = false;
    }

    /// Three independently culled and shaded physical panels, composited into a
    /// spanning window. HUD and menu remain in window pixels and are drawn once.
    pub fn render_triple(
        &mut self,
        scene: &mut Scene,
        target: &mx::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
        rig: &TripleScreen,
    ) {
        if width < 3 || height == 0 || self.device_lost().is_some() {
            return;
        }
        self.temporal_forget();
        let views = rig.views(camera, width, height);
        // all three panels have the same size: they share the size-keyed targets
        let pw = panel_width(width);
        if self.targets.triple_targets.as_ref().map(|t| t.0) != Some((width, height)) {
            let targets = views
                .iter()
                .map(|v| {
                    let w = pw;
                    let texture = self
                        .device
                        .create_texture(&mx::TextureDescriptor {
                            label: Some("triple screen panel"),
                            size: mx::Extent3d {
                                width: w,
                                height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: mx::TextureDimension::D2,
                            format: self.format,
                            usage: mx::TextureUsages::RENDER_ATTACHMENT
                                | mx::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        })
                        .expect("mX: a texture");
                    let view = texture
                        .create_view(&Default::default())
                        .expect("mX: a texture view");
                    // (z: where the panel starts in the window, for `fs_panel`)
                    let params = [w as f32, height as f32, v.viewport[0] as f32, 0.0];
                    let buf = buffer_init(
                        &self.device,
                        &self.queue,
                        Some("triple screen composite"),
                        bytemuck::cast_slice(&params),
                        mx::BufferUsages::UNIFORM | mx::BufferUsages::STORAGE,
                    );
                    let group = HeapGroup::of(
                        picture_layout(),
                        0,
                        &mx::BindGroupDescriptor {
                            label: Some("triple screen composite"),
                            entries: &[
                                mx::BindGroupEntry {
                                    binding: 0,
                                    resource: buf.as_entire_binding(),
                                },
                                mx::BindGroupEntry {
                                    binding: 1,
                                    resource: mx::BindingResource::TextureView(&view),
                                },
                                mx::BindGroupEntry {
                                    binding: 2,
                                    resource: mx::BindingResource::Sampler(&self.sky.sky_sampler),
                                },
                            ],
                        },
                    );
                    (view, group)
                })
                .collect();
            self.targets.triple_targets = Some(((width, height), targets));
        }
        // the HUD is drawn once over the whole window, after the panels: they draw none
        // (its rect buffers are kept aside, not rebuilt each frame)
        let overlays = std::mem::take(&mut scene.overlays);
        let overlay_res = std::mem::take(&mut scene.overlay_res);
        let env_heading = self.env_heading.replace(Some(camera.yaw));
        // Centre meters exposure and updates shared lighting once. All panels
        // then use that same exposure and sun shadow atlas.
        for (turn, i) in [1, 0, 2].into_iter().enumerate() {
            let view = self.targets.triple_targets.as_ref().unwrap().1[i].0.clone();
            self.swap_triple_culling(i);
            self.render_inner(
                scene,
                &view,
                pw,
                height,
                &views[i].camera,
                lighting,
                true,
                None,
                Some(views[i].projection),
                turn != 0,
            );
            self.swap_triple_culling(i);
        }
        self.env_heading.set(env_heading);
        scene.overlays = overlays;
        scene.overlay_res = overlay_res;
        self.prepare_overlays(scene, width, height);
        let mut encoder = self
            .device
            .create_command_encoder(Some("triple screen composite"));
        {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("triple screen composite"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: mx::Operations {
                        load: mx::LoadOp::Clear(mx::Color::BLACK),
                        store: mx::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.scale.panel_pipeline);
            for (i, v) in views.iter().enumerate() {
                // (the right panel loses its last pixel or two at the window's edge)
                let [x, y, w, h] = v.viewport;
                pass.set_scissor_rect(x, y, w.min(width - x), h);
                self.targets.triple_targets.as_ref().unwrap().1[i]
                    .1
                    .set(&mut pass);
                pass.draw(0..3, 0..1);
            }
            pass.set_scissor_rect(0, 0, width, height);
            pass.set_pipeline(&self.overlay_pipeline_1x);
            for (_, _, group, _) in &scene.overlay_res {
                group.set(&mut pass);
                pass.draw(0..6, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    pub(crate) fn swap_triple_culling(&mut self, panel: usize) {
        // Visibility and LOD hysteresis belong to one projection. A side panel
        // must not replace the previous centre view's history each frame.
        let history = &mut self.history.triple_culling[panel];
        std::mem::swap(self.history.cull_drawn.get_mut(), &mut history.drawn);
        std::mem::swap(self.history.object_sizes.get_mut(), &mut history.sizes);
        std::mem::swap(
            self.history.object_sizes_scratch.get_mut(),
            &mut history.scratch,
        );
    }

    /// Render one OpenXR view using the headset's asymmetric projection matrix.
    /// The matrix uses the same reversed depth range as the desktop camera. The
    /// second eye reuses the first eye's shadow atlas when both share an origin,
    /// while each eye keeps its own visibility and LOD history between frames.
    pub fn render_xr_eye(
        &mut self,
        scene: &mut Scene,
        target: &mx::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
        projection: Mat4,
        second_eye: bool,
    ) {
        self.swap_xr_culling(second_eye as usize);
        self.render_inner(
            scene,
            target,
            width,
            height,
            camera,
            lighting,
            false,
            None,
            Some(projection),
            second_eye,
        );
        self.swap_xr_culling(second_eye as usize);
    }

    fn swap_xr_culling(&mut self, eye: usize) {
        // As with triple-screen panels, an eye must use its own previous view,
        // and leave the desktop's history intact when returning from the draw.
        let history = &mut self.history.xr_culling[eye];
        std::mem::swap(self.history.cull_drawn.get_mut(), &mut history.drawn);
        std::mem::swap(self.history.object_sizes.get_mut(), &mut history.sizes);
        std::mem::swap(
            self.history.object_sizes_scratch.get_mut(),
            &mut history.scratch,
        );
    }

    /// Composite the shared game menu and a world-positioned pointer into each eye.
    /// Eye-specific positions make the menu fuse into one virtual panel.
    pub fn render_xr_ui(
        &self,
        scene: &Scene,
        eyes: &[mx::TextureView; 2],
        desktop_size: (u32, u32),
        eye_size: (u32, u32),
        menu_range: std::ops::Range<usize>,
        menu_transforms: [Mat4; 2],
        cursor_overlay: Option<usize>,
        tooltip_overlay: Option<usize>,
        cursor_transforms: [Option<Mat4>; 2],
        navigator: Option<(TextureId, [Mat4; 2])>,
    ) {
        let Some(menu) = scene.overlays.get(menu_range) else {
            return;
        };
        if menu.is_empty() && cursor_transforms.iter().all(Option::is_none) && navigator.is_none() {
            return;
        }
        let (w, h) = (desktop_size.0.max(1) as f32, desktop_size.1.max(1) as f32);
        let (eye_w, eye_h) = (eye_size.0.max(1) as f32, eye_size.1.max(1) as f32);
        let prepare = |id: &TextureId, quad: [Vec4; 4]| {
            let texture = scene.textures.get(*id)?;
            let mut uniform = [0.0f32; 20];
            for (index, corner) in quad.iter().enumerate() {
                uniform[index * 4..index * 4 + 4].copy_from_slice(&corner.to_array());
            }
            uniform[16] = scene.premultiplied.contains(id) as u8 as f32;
            let buffer = buffer_init(
                &self.device,
                &self.queue,
                Some("OpenXR menu rectangle"),
                bytemuck::cast_slice(&uniform),
                mx::BufferUsages::UNIFORM | mx::BufferUsages::STORAGE,
            );
            let group = HeapGroup::of(
                picture_layout(),
                0,
                &mx::BindGroupDescriptor {
                    label: Some("OpenXR menu rectangle"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 1,
                            resource: mx::BindingResource::TextureView(&texture.view),
                        },
                        mx::BindGroupEntry {
                            binding: 2,
                            resource: mx::BindingResource::Sampler(&self.sky.sky_sampler),
                        },
                    ],
                },
            );
            Some((buffer, group))
        };
        let mut prepared = [Vec::new(), Vec::new()];
        for eye in 0..2 {
            if let Some((id, transforms)) = navigator.as_ref() {
                let quad = [
                    Vec4::new(-1.0, 1.0, 0.0, 1.0),
                    Vec4::new(1.0, 1.0, 0.0, 1.0),
                    Vec4::new(1.0, -1.0, 0.0, 1.0),
                    Vec4::new(-1.0, -1.0, 0.0, 1.0),
                ]
                .map(|p| transforms[eye] * p);
                if let Some(item) = prepare(id, quad) {
                    prepared[eye].push(item);
                }
            }
            for (index, (id, rect)) in menu.iter().enumerate() {
                // The first rectangle dims the view. Every other rectangle is
                // projected from the same menu plane in world space.
                let transform = if index == 0
                    && rect[0] <= 0.0
                    && rect[1] <= 0.0
                    && rect[2] >= w
                    && rect[3] >= h
                {
                    Mat4::IDENTITY
                } else {
                    menu_transforms[eye]
                };
                let x0 = rect[0] / w * 2.0 - 1.0;
                let y0 = 1.0 - rect[1] / h * 2.0;
                let x1 = rect[2] / w * 2.0 - 1.0;
                let y1 = 1.0 - rect[3] / h * 2.0;
                let quad = [
                    transform * Vec4::new(x0, y0, 0.0, 1.0),
                    transform * Vec4::new(x1, y0, 0.0, 1.0),
                    transform * Vec4::new(x1, y1, 0.0, 1.0),
                    transform * Vec4::new(x0, y1, 0.0, 1.0),
                ];
                if let Some(item) = prepare(id, quad) {
                    prepared[eye].push(item);
                }
            }
            if let (Some(index), Some(transform)) = (cursor_overlay, cursor_transforms[eye]) {
                if let Some((id, rect)) = scene.overlays.get(index) {
                    let half_x = (rect[2] - rect[0]) / eye_w;
                    let half_y = (rect[3] - rect[1]) / eye_h;
                    let quad = [
                        transform * Vec4::new(-half_x, half_y, 0.0, 1.0),
                        transform * Vec4::new(half_x, half_y, 0.0, 1.0),
                        transform * Vec4::new(half_x, -half_y, 0.0, 1.0),
                        transform * Vec4::new(-half_x, -half_y, 0.0, 1.0),
                    ];
                    if let Some(item) = prepare(id, quad) {
                        prepared[eye].push(item);
                    }
                }
                if let Some((id, rect)) = tooltip_overlay.and_then(|i| scene.overlays.get(i)) {
                    let pointer_width = scene
                        .overlays
                        .get(index)
                        .map(|(_, r)| r[2] - r[0])
                        .unwrap_or(24.0);
                    let left = (pointer_width * 0.5 + 8.0) * 2.0 / eye_w;
                    let right = left + (rect[2] - rect[0]) * 2.0 / eye_w;
                    let bottom = -(rect[3] - rect[1]) * 2.0 / eye_h;
                    let quad = [
                        transform * Vec4::new(left, 0.0, 0.0, 1.0),
                        transform * Vec4::new(right, 0.0, 0.0, 1.0),
                        transform * Vec4::new(right, bottom, 0.0, 1.0),
                        transform * Vec4::new(left, bottom, 0.0, 1.0),
                    ];
                    if let Some(item) = prepare(id, quad) {
                        prepared[eye].push(item);
                    }
                }
            }
        }
        let mut encoder = self.device.create_command_encoder(Some("OpenXR menu"));
        for (eye, target) in eyes.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("OpenXR menu"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: mx::Operations {
                        load: mx::LoadOp::Load,
                        store: mx::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.xr_ui_pipeline);
            for (_, group) in &prepared[eye] {
                group.set(&mut pass);
                pass.draw(0..6, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    pub(crate) fn prepare_overlays(&self, scene: &mut Scene, full_w: u32, full_h: u32) {
        let overlays = &scene.overlays;
        // the HUD's rect buffers and bind groups live on between frames: making them
        // anew for every overlay of every frame was a steady stream of GPU allocations
        scene.overlay_res.truncate(overlays.len());
        for (k, (tex, r)) in overlays.iter().copied().enumerate() {
            let r = snap_rect(r);
            let ndc = [
                r[0] / full_w as f32 * 2.0 - 1.0,
                1.0 - r[1] / full_h as f32 * 2.0,
                r[2] / full_w as f32 * 2.0 - 1.0,
                1.0 - r[3] / full_h as f32 * 2.0,
                scene.premultiplied.contains(&tex) as u8 as f32,
                scene.transposed.contains(&tex) as u8 as f32,
                0.0,
                0.0,
            ];
            if let Some((_, buf, _, last)) = scene.overlay_res.get_mut(k).filter(|o| o.0 == tex) {
                if *last != ndc {
                    self.queue
                        .write_buffer(buf, 0, bytemuck::cast_slice(&ndc))
                        .expect("mX: a queue write");
                    *last = ndc;
                }
                continue;
            }
            let buf = buffer_init(
                &self.device,
                &self.queue,
                Some("overlay rect"),
                bytemuck::cast_slice(&ndc),
                mx::BufferUsages::UNIFORM | mx::BufferUsages::STORAGE | mx::BufferUsages::COPY_DST,
            );
            let bg = HeapGroup::of(
                picture_layout(),
                0,
                &mx::BindGroupDescriptor {
                    label: Some("overlay"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: buf.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 1,
                            resource: mx::BindingResource::TextureView(&scene.textures[tex].view),
                        },
                        mx::BindGroupEntry {
                            binding: 2,
                            resource: mx::BindingResource::Sampler(&self.sky.sky_sampler),
                        },
                    ],
                },
            );
            if k < scene.overlay_res.len() {
                scene.overlay_res[k] = (tex, buf, bg, ndc);
            } else {
                scene.overlay_res.push((tex, buf, bg, ndc));
            }
        }
    }

    /// One frame; a resource mX could not make for it (the card out of memory, the device
    /// lost) leaves that frame out and goes to the error report (wgpu made an invalid object
    /// and reported it, the frame drawing nothing where it was used).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_inner(
        &mut self,
        scene: &mut Scene,
        target: &mx::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
        with_overlays: bool,
        exclude_texture: Option<TextureId>,
        projection: Option<Mat4>,
        second_eye: bool,
    ) {
        let drawn = gpu_attempt(|| {
            self.render_frame(
                scene,
                target,
                width,
                height,
                camera,
                lighting,
                with_overlays,
                exclude_texture,
                projection,
                second_eye,
            )
        });
        if let Err(text) = drawn {
            (self.errors.report)(&refused_error(text));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame(
        &mut self,
        scene: &mut Scene,
        target: &mx::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
        with_overlays: bool,
        exclude_texture: Option<TextureId>,
        projection: Option<Mat4>,
        second_eye: bool,
    ) {
        // the frame's switches from the environment, each read once
        let env = FrameEnv::read();
        if !self.frame_hooks(scene, &env, with_overlays) {
            return;
        }
        // OMSI_GPU_TIMERS: GPU time per pass, one frame at a time
        self.collect_gpu_timers();
        let timer_view = view_index(with_overlays, projection.is_some(), second_eye);
        let mut pictures = self.pictures.get();
        pictures[timer_view] += 1;
        self.pictures.set(pictures);
        let mut timers = PassTimers {
            view: timer_view,
            set: self.gpu_timers[timer_view]
                .as_ref()
                .filter(|t| !t.waiting && !t.unresolved)
                .map(|t| t.set.clone()),
            timed: Vec::new(),
        };
        // OMSI_PROFILE: time per stage, the mirrors apart from the window's picture
        let mut clock = StageClock::new(with_overlays);
        let args = FrameArgs {
            target,
            width,
            height,
            camera,
            lighting,
            with_overlays,
            exclude_texture,
            projection,
            second_eye,
        };
        let f = self.begin_frame(scene, env, args, &mut clock);
        let sh = self.plan_shadow_cache(scene, &f);
        let (vp_mat, cu, probe_redraw) = self.frame_uniforms(scene, &f, &sh, &mut clock);
        // --- what every pass draws, as batches over one draw list (see `Batch`): the shadow
        // casters of each cascade, the depth prepass and the main pass. The list is built
        // and uploaded before any pass is encoded.
        clock.stage(self, "setup", "mirror.setup");
        let mut shadow_list: Vec<u32> = Vec::new();
        let mut shadow_batches = self.plan_shadow_casters(scene, &f, &sh, &mut shadow_list);
        clock.stage(self, "shadow items", "mirror.shadow items");
        let visible = self.cull_view(scene, &f, &mut clock);
        let mut plan = self.batch_draws(scene, &f, &visible, Vec::new());
        if plan.ground.active() {
            // (the frame's uniform was written with the flag off: the base is lit from the ground picture)
            let flag = 1.0f32;
            self.queue
                .write_buffer(
                    &self.camera_buf,
                    (std::mem::offset_of!(CameraUniform, lod) + 4) as u64,
                    bytemuck::bytes_of(&flag),
                )
                .expect("mX: a queue write");
        }
        // (the shadow casters' entries after the passes' own: the main pass's batches keep
        // their places in the list while only the shadows change, and its bundles are kept)
        let at = plan.list.len() as u32;
        for b in shadow_batches.iter_mut().flatten() {
            b.instances = b.instances.start + at..b.instances.end + at;
        }
        plan.list.extend_from_slice(&shadow_list);
        self.count_draws(
            scene,
            with_overlays,
            &visible,
            &plan,
            &shadow_batches,
            timer_view,
        );
        clock.stage(self, "items", "mirror.items");
        plan.split_rain(scene, f.glass_on);
        self.upload_draw_list(scene, &plan.list);
        clock.stage(self, "upload", "mirror.upload");
        let main_bundles = self.record_main_bundles(scene, &f, &plan);
        clock.stage(self, "bundles", "mirror.bundles");
        // three command buffers, finished side by side (see below): the shadow maps, the
        // depth prepass with the ambient occlusion, and the picture itself
        let mut shadow_encoder = self.device.create_command_encoder(Some("shadow maps"));
        self.flush_pending_meshes(scene, &mut shadow_encoder);
        let mut enc = Encoders {
            shadow: shadow_encoder,
            prepass: self.device.create_command_encoder(Some("depth prepass")),
            picture: self.device.create_command_encoder(Some("picture")),
        };
        self.encode_shadow_maps(&mut enc.shadow, scene, &sh, &shadow_batches, &mut timers);
        self.encode_lamp_shadows(&mut enc.shadow, scene, &f, &shadow_batches, &mut timers);
        self.encode_precip_map(&mut enc.shadow, scene, &sh, &shadow_batches, &mut timers);
        let prepass_batches = if self.shares_prepass_depth(&f, &plan) {
            plan.prefill_batches()
        } else {
            &plan.prepass_batches
        };
        self.encode_depth_prepass(&mut enc.prepass, scene, &f, prepass_batches, &mut timers);
        if plan.ground.active() {
            self.encode_ground(&mut enc.prepass, scene, &plan.ground, &mut timers);
        }
        if f.enhanced && f.lead_view && f.prepass_on && self.device_lost().is_none() {
            if let (Some(s), Some(ao)) = (self.tone.lamp_sight.as_mut(), self.targets.ao.as_ref()) {
                s.encode(
                    &self.device,
                    &self.queue,
                    &mut enc.prepass,
                    &ao.depth_view,
                    vp_mat,
                    f.ro,
                    f.cam_rel,
                );
            }
        }
        if f.rt_frame {
            self.encode_ray_tracing(
                &mut enc.prepass,
                scene,
                timers.set.as_ref(),
                &mut timers.timed,
            );
        }
        self.encode_sky_cube(&mut enc.prepass, scene, &f, probe_redraw, &mut timers);
        self.encode_clouds(&mut enc.prepass, scene, &f, vp_mat, &mut timers);
        self.encode_probe(&mut enc.prepass, scene, probe_redraw, &mut timers);
        let mut main_parts = self.encode_main_pass(
            &mut enc.picture,
            scene,
            &f,
            &plan,
            &main_bundles,
            &mut timers,
        );
        // Turning the recorded passes into Metal commands is the costliest CPU step of a
        // frame (wgpu checks every draw): the shadow maps, the prepass, the main pass and
        // the passes after it are finished side by side on helper threads and this one.
        let big =
            shadow_batches.iter().map(|b| b.len()).sum::<usize>() + plan.prepass_batches.len() > 64
                || !main_parts.is_empty()
                || (!cfg!(any(target_os = "macos", target_os = "ios")) && main_bundles.len() >= 2);
        if big && !f.env.no_post_split {
            let rest = self
                .device
                .create_command_encoder(Some("after the main pass"));
            main_parts.push(std::mem::replace(&mut enc.picture, rest));
        }
        let puddles_on =
            self.encode_scene_reflections(&mut enc.picture, scene, &f, &plan, &cu, &mut timers);
        self.encode_fog_lamps(&mut enc.picture, scene, &f, vp_mat, puddles_on, &mut timers);
        self.encode_temporal_late_motion(&mut enc.picture, scene, &f, &plan);
        self.encode_temporal_mask(&mut enc.picture, scene, &f, &plan);
        self.encode_rain_film(&mut enc.picture, scene, &f, &plan.rain_batches, puddles_on);
        self.encode_classic_present(&mut enc.picture, scene, &f, puddles_on);
        if f.enhanced {
            self.encode_post(&mut enc.picture, scene, &f, puddles_on, &mut timers);
        }
        self.encode_upscale(&mut enc.picture, scene, &f, &mut timers);
        clock.stage(self, "encode", "mirror.encode");
        self.finish_and_submit(enc, main_parts, big, with_overlays, &mut clock, timers);
        // the picture is on the queue now: the upscaler brings it up into the window, the HUD after
        if let Some(frame) = self.temporal_pending.take().filter(|_| f.temporal_frame) {
            self.temporal_present(scene, &f, frame);
        }
    }

    /// Render off-screen and return RGBA8 pixels.
    pub fn render_to_image(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
    ) -> Result<Vec<u8>> {
        self.render_image(scene, width, height, camera, lighting, None)
    }

    /// A picture as `render_to_image`, and the distance along the view (m) of what each of
    /// its pixels shows, from the depth prepass (None where the renderer has none).
    pub fn render_to_image_with_depth(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
    ) -> Result<(Vec<u8>, Option<Vec<f32>>)> {
        // (OpenGL ES reads no depth back; mX's GL reads it on desktop GL only)
        let copies = !gl_backend();
        self.depth_readback = copies;
        let px = self.render_image(scene, width, height, camera, lighting, None);
        self.depth_readback = false;
        let px = px?;
        let tex = self
            .targets
            .ao
            .as_ref()
            .filter(|a| copies && a.size == (width, height))
            .map(|a| a.depth_tex.clone());
        let depth = match tex {
            Some(t) => {
                // (a backend that cannot copy depth back: the picture without it)
                match self.read_texture(&t, mx::TextureAspect::DepthOnly) {
                    Ok(raw) => {
                        // reversed depth (1 at the near plane, 0 at the far one) of
                        // `perspective_rh(fov, aspect, far, near)` back to the distance along
                        // the view
                        let (near, far) = (camera.near as f64, camera.depth_far() as f64);
                        let r = near / (far - near);
                        Some(
                            raw.chunks_exact(4)
                                .take((width * height) as usize)
                                .map(|b| {
                                    let d = f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64;
                                    (r * far / (d + r)).min(far) as f32
                                })
                                .collect(),
                        )
                    }
                    Err(e) => {
                        log::warn!("the depth could not be read back: {e}");
                        None
                    }
                }
            }
            None => None,
        };
        Ok((px, depth))
    }

    /// Capture all three physical screen projections, including the shared HUD.
    pub fn render_triple_to_image(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
        rig: &TripleScreen,
    ) -> Result<Vec<u8>> {
        self.render_image(scene, width, height, camera, lighting, Some(rig))
    }

    pub(crate) fn render_image(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        camera: &Camera,
        lighting: &Lighting,
        rig: Option<&TripleScreen>,
    ) -> Result<Vec<u8>> {
        let tex = self
            .device
            .create_texture(&mx::TextureDescriptor {
                label: Some("offscreen"),
                size: mx::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: self.format,
                usage: mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
            .expect("mX: a texture");
        let view = tex
            .create_view(&Default::default())
            .expect("mX: a texture view");
        // a picture on its own: the enhanced exposure is where the light puts it at once
        self.instant_exposure = true;
        let max_scene = self.options.max_scene.take();
        if let Some(rig) = rig {
            self.render_triple(scene, &view, width, height, camera, lighting, rig);
        } else {
            self.render(scene, &view, width, height, camera, lighting);
        }
        self.options.max_scene = max_scene;
        self.instant_exposure = false;
        let mut out = self.read_texture(&tex, mx::TextureAspect::All)?;
        // BGRA surfaces → swap
        if matches!(
            self.format,
            mx::TextureFormat::Bgra8Unorm | mx::TextureFormat::Bgra8UnormSrgb
        ) {
            for px in out.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        Ok(out)
    }

    pub(crate) fn read_texture(
        &self,
        texture: &mx::Texture,
        aspect: mx::TextureAspect,
    ) -> Result<Vec<u8>> {
        let (width, height) = (texture.width(), texture.height());
        let row_bytes = width
            * texture
                .format()
                .block_copy_size()
                .context("unsupported readback format")?;
        let bpr = row_bytes.div_ceil(256) * 256;
        let buf = self
            .device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("readback"),
                size: (bpr * height) as u64,
                usage: mx::BufferUsages::COPY_DST | mx::BufferUsages::MAP_READ,
            })
            .expect("mX: a buffer");
        let mut enc = self.device.create_command_encoder(None);
        enc.copy_texture_to_buffer(
            mx::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: mx::Origin3d::ZERO,
                aspect,
            },
            mx::TexelCopyBufferInfo {
                buffer: &buf,
                layout: mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(height),
                },
            },
            mx::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let idx = self.queue.submit([enc.finish()]);
        wait_gpu(&self.device, Some(idx)).map_err(|e| anyhow!("waiting for the GPU: {e}"))?;
        let out = buf
            .map_read_now(0..buf.size(), |data| {
                let mut out = Vec::with_capacity((row_bytes * height) as usize);
                for row in 0..height {
                    let start = (row * bpr) as usize;
                    out.extend_from_slice(&data[start..start + row_bytes as usize]);
                }
                out
            })
            .map_err(|e| anyhow!("reading back: {e}"))?;
        Ok(out)
    }
}

/// The enhanced picture's post pass and exposure: pipelines, metering, the eye's adaptation, the lighting uniform and probe.
pub(crate) struct ToneMapping {
    pub(crate) post: PostPipelines,
    pub(crate) post_buf: mx::Buffer,
    pub(crate) post_sampler: mx::Sampler,
    /// The metered mean log luminance (1x1), and the adapted value in two textures drawn
    /// into by turns (`adapt_front` is the current one), with their bind groups.
    pub(crate) meter_view: mx::TextureView,
    pub(crate) adapt_views: [mx::TextureView; 2],
    pub(crate) adapt_bg: [HeapGroup; 2],
    pub(crate) adapt_front: usize,
    /// OMSI_DEBUG_EXPOSURE: the adapted metering read back now and then, and logged.
    pub(crate) exposure_log: Option<ExposureLog>,
    pub(crate) lamp_sight: Option<LampSight>,
    /// Enhanced lighting: its uniform, the sky table, the sampler both use, the reflection
    /// probe, the sky last computed and the exposure as it follows it.
    pub(crate) enh_buf: mx::Buffer,
    pub(crate) lin_sampler: mx::Sampler,
    pub(crate) probe: Option<Probe>,
    /// The lamps' and headlights' light on what the camera looks at (`view_lamp_light`),
    /// which the eye adapts to by night.
    pub(crate) view_lamps: Option<f32>,
    /// The pre-exposure (natural log) as it follows the sky's.
    pub(crate) exposure: Option<f32>,
    /// The enhanced lighting uniform as last written (`prepare_enhanced`), for the passes
    /// that decide on the CPU whether they have anything to draw (`shafts_seen`).
    pub(crate) enh_last: crate::uniforms::EnhancedUniform,
    /// The thunderstorm's flashes (`Lightning`) and the strikes the sound has not yet taken.
    pub(crate) lightning: crate::lighting::Lightning,
    /// This frame's flash (stepped for every pipeline, drawn by the Enhanced one).
    pub(crate) flash: crate::lighting::Flash,
    /// The last frame of the window's picture the bounced light read back: its view and
    /// projection, render origin, size and where it looked (see `bounce_history`).
    pub(crate) bounce_prev: Option<(Mat4, DVec3, (u32, u32), Vec3, Vec3)>,
    /// From a view-space point to the last frame's clip space, and whether that picture can
    /// be read (0 or 1), for the AO pass.
    pub(crate) bounce_reproj: ([[f32; 4]; 4], f32),
}

#[cfg(test)]
mod upscaler_fallback_tests {
    use super::*;

    fn upscaled() -> RenderOptions {
        RenderOptions {
            msaa: 1,
            msaa_fallback: 8,
            fxaa: false,
            temporal: Some(Temporal {
                upscaler: TemporalUpscaler::Dlss,
                quality: TemporalQuality::Balanced,
            }),
            ..Default::default()
        }
    }

    /// The upscaler gone with memory to spare: the player's multisampling, as set.
    #[test]
    fn without_the_upscaler_the_player_s_msaa_comes_back() {
        let o = Renderer::without_upscaler(upscaled(), false);
        assert_eq!(
            (o.temporal, o.msaa, o.msaa_fallback, o.fxaa),
            (None, 8, 0, false)
        );
    }

    /// The upscaler gone with the card out of memory: no multisampling (8x took more of the
    /// memory that was short), FXAA instead, and the player's MSAA kept for later.
    #[test]
    fn without_the_upscaler_and_memory_fxaa_and_no_msaa() {
        let o = Renderer::without_upscaler(upscaled(), true);
        assert_eq!(
            (o.temporal, o.msaa, o.msaa_fallback, o.fxaa),
            (None, 1, 8, true)
        );
    }
}
