//! How a frame goes with temporal upscaling on.
//!
//! 1. `begin_frame` makes the picture as small as the upscaler wants and draws it into its input
//!    texture instead of the window. FXAA and the render scale stay out of the way.
//! 2. `temporal_jitter` shifts the projection by a different sub-pixel amount every frame, so
//!    after a few frames the upscaler has seen every pixel from lots of points.
//! 3. The depth prepass also writes how far each pixel moved since the last frame
//!    (`pipelines/temporal.rs`, `motion.wgsl`): the camera's motion plus every object's own, from
//!    its model matrix of the frame before.
//! 4. After the scene is submitted `temporal_present` runs the upscaler on wgpu's queue, draws
//!    the result to the window and then the HUD on top at full size.

use super::{fallback_render_size, fidelityfx, streamline, FrameConstants, Inputs, TemporalQuality, TemporalUpscaler};
use crate::passes::FrameCtx;
use crate::pipelines::common::uniform_buffer;
use crate::*;

/// Motion texture: per pixel, how many pixels back its point was one frame ago. Half floats, as
/// NVIDIA's and AMD's own samples use: still finer than a tenth of a pixel up to 100 pixels of
/// motion, and the prepass writes it for every surface it draws.
pub(crate) const TEMPORAL_MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// The window-sized result: a storage texture, and those can't be sRGB. It only ever holds the
/// sRGB-encoded numbers of the 8 bit picture the upscaler was given.
const TEMPORAL_OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Where glass, its reflections and the rain on it lie (see `vs_mask`). Not R8: Streamline's
/// Vulkan layer cannot read VK_FORMAT_R8_UNORM back (`getFormat`), and logged four errors a frame.
pub(crate) const TEMPORAL_MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

/// An object moved further than this in one frame (m) gets no motion of its own. The engine hands
/// a freed entry to the next object loaded (`recycle_instance`), so for one frame the "last
/// matrix" can be some other house half the map away: the history pulled from there would flash
/// bits of picture over the scene. 8 m is about 290 km/h at 10 fps.
const MAX_FRAME_JUMP: f32 = 8.0;

/// How much `upscale.wgsl` sharpens DLSS's result (contrast adaptive, 0..1). DLSS stopped
/// sharpening years ago and its picture of the cab comes out soft next to MSAA; FSR 3 sharpens on
/// its own.
const DLSS_SHARPEN: f32 = 0.25;

/// The prepass pipelines that write motion: the six kinds of the ordinary prepass (plain, cut-out,
/// transmap; each two-sided or culled), and one for the sky behind everything.
pub(crate) struct TemporalPipelines {
    pub(crate) prepass: [wgpu::RenderPipeline; 6],
    /// The same without writing depth, for the parts the prefill left out (`cut_batches`).
    pub(crate) late_motion: [wgpu::RenderPipeline; 6],
    pub(crate) sky: wgpu::RenderPipeline,
    pub(crate) motion_layout: wgpu::BindGroupLayout,
    /// The transparency mask's: the panes (by their opacity) and the rain films, two-sided and
    /// culled.
    pub(crate) glass_mask: [wgpu::RenderPipeline; 2],
    pub(crate) rain_mask: [wgpu::RenderPipeline; 2],
    /// The bus's screens (`MaterialExtra::screen`), any kind of draw.
    pub(crate) screen_mask: [wgpu::RenderPipeline; 2],
}

/// The upscaler's textures for one picture size and window size.
pub(crate) struct TemporalTargets {
    render: (u32, u32),
    output: (u32, u32),
    /// The small picture the upscaler reads; `color_bg` stretches it into the window instead
    /// should the upscaler fail.
    color: wgpu::Texture,
    pub(crate) color_view: wgpu::TextureView,
    /// The same as its plain UNORM self: the sRGB-encoded numbers as they are (Streamline on
    /// Vulkan reads the picture through it).
    color_raw_view: wgpu::TextureView,
    pub(crate) color_bg: wgpu::BindGroup,
    motion: wgpu::Texture,
    pub(crate) motion_view: wgpu::TextureView,
    mask: wgpu::Texture,
    pub(crate) mask_view: wgpu::TextureView,
    /// What the upscaler writes, and the bind group that copies it to the window.
    output_tex: wgpu::Texture,
    output_view: wgpu::TextureView,
    output_bg: wgpu::BindGroup,
}

/// The previous frame: its camera (for the motion and the upscaler's previous-clip matrices) and
/// what it was drawn with, which must match for its history to count.
#[derive(Clone, Copy)]
struct TemporalPrev {
    camera: Camera,
    aspect: f32,
    origin: DVec3,
    render: (u32, u32),
    output: (u32, u32),
}

/// The upscaler the settings chose, once it is running.
enum Runtime {
    Dlss(streamline::Runtime),
    Fsr3(fidelityfx::Upscaler),
}

/// Everything temporal upscaling keeps while it is on.
pub(crate) struct TemporalState {
    runtime: Runtime,
    pub(crate) targets: Option<TemporalTargets>,
    /// The textures of the last few frames, held a little longer: our D3D12 list runs after
    /// wgpu's submit, and wgpu would otherwise free a texture it thinks nobody needs.
    keep: std::collections::VecDeque<Vec<wgpu::Texture>>,
    motion_buf: wgpu::Buffer,
    /// The copy's parameters (`upscale.wgsl`, its third mode).
    blit_buf: wgpu::Buffer,
    /// A copy of the model matrices as the last frame had them, and the bind group that gives it
    /// to the motion shader together with `motion_buf`.
    pub(crate) prev_models: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    /// `Scene::model_epoch` and the number of entries when that copy was made: the old matrices
    /// are only used while the entries still sit where they sat.
    prev_epoch: u64,
    prev_entries: u32,
    prev: Option<TemporalPrev>,
    frame: u64,
}

/// What the camera work of a frame leaves for `temporal_present`.
pub(crate) struct TemporalFrame {
    /// The view-projection without the jitter, and the last frame's taken from this frame's
    /// render origin.
    vp: Mat4,
    vp_prev_here: Mat4,
    /// This frame's jitter in pixels.
    jitter: [f32; 2],
    reset: bool,
    aspect: f32,
    /// Since the last frame, in seconds.
    dt: f32,
}

/// Element `i` (1-based) of the van der Corput / Halton sequence in `base`, in 0..1.
pub(super) fn halton(mut i: u32, base: u32) -> f32 {
    let (mut f, mut r) = (1.0f32, 0.0f32);
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

/// A matrix laid out for the upscalers: glam stores columns, Streamline reads rows of row-vector
/// matrices, and the two are the same numbers.
pub(super) fn sl_matrix(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

/// How many jitter steps before the sequence starts over: more the more the picture is scaled up
/// (8 at native, up to 64), as NVIDIA's guide suggests.
pub(super) fn jitter_phases(render_w: u32, full_w: u32) -> u32 {
    let ratio = (full_w as f32 / render_w.max(1) as f32).max(1.0);
    ((8.0 * ratio * ratio).round() as u32).clamp(8, 64)
}

/// The upscalers' alpha is not the picture's: a screenshot's pixels made opaque.
pub(super) fn opaque(mut rgba: Vec<u8>) -> Vec<u8> {
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    rgba
}

/// The textures' mip bias of an upscaled frame: log2(render width / window width), as NVIDIA's
/// guide and AMD's docs give it (-0.79 at Balanced). Without it the textures are sampled for the
/// small picture and the cab goes soft.
pub(crate) fn temporal_mip_bias(render_w: u32, full_w: u32) -> f32 {
    (render_w.max(1) as f32 / full_w.max(1) as f32).log2().clamp(-3.0, 0.0)
}

impl Renderer {
    /// True when this window frame is upscaled. The first frame that wants it starts the
    /// upscaler; if that fails the reason is logged once and it stays off.
    pub(crate) fn temporal_start(&mut self) -> bool {
        if self.temporal_failed {
            // (let go of its resources and textures: a runtime left here failed every frame)
            self.temporal = None;
            return false;
        }
        if self.temporal.is_some() {
            return true;
        }
        let Some(asked) = self.options.temporal.filter(|_| self.temporal_pipes.is_some()) else { return false };
        let loaded = match asked.upscaler {
            TemporalUpscaler::Dlss => streamline::Runtime::load(&self.device).map(Runtime::Dlss),
            TemporalUpscaler::Fsr3 => fidelityfx::Upscaler::load(&self.device).map(Runtime::Fsr3),
        };
        match loaded {
            Ok(runtime) => {
                self.temporal = Some(TemporalState {
                    runtime,
                    targets: None,
                    keep: Default::default(),
                    motion_buf: uniform_buffer(&self.device, "temporal motion", std::mem::size_of::<MotionUniform>() as u64),
                    blit_buf: uniform_buffer(&self.device, "temporal blit", 16),
                    prev_models: None,
                    prev_epoch: u64::MAX,
                    prev_entries: 0,
                    prev: None,
                    frame: 0,
                });
                log::info!("{} upscaling ({}) running on {}", asked.upscaler.as_str(), asked.quality.as_str(), self.adapter_name);
                true
            }
            Err(e) => {
                log::warn!("{} upscaling is off: {e:#}", asked.upscaler.as_str());
                self.temporal_failed = true;
                false
            }
        }
    }

    /// The last upscaled picture of a window of this size, as RGBA8, before the HUD went over it:
    /// what a player's screenshot should show (`render_to_image` draws the scene again, without
    /// the upscaler). None when no frame has been upscaled, so that it is drawn after all. It
    /// holds the sRGB-encoded numbers, as a picture file does, without DLSS's light sharpening.
    pub fn temporal_picture(&self, width: u32, height: u32) -> Option<anyhow::Result<Vec<u8>>> {
        if self.temporal_failed {
            return None;
        }
        let d = self.temporal.as_ref().filter(|d| d.prev.is_some())?;
        let t = d.targets.as_ref().filter(|t| t.output == (width, height))?;
        Some(self.read_texture(&t.output_tex, wgpu::TextureAspect::All).map(opaque))
    }

    fn temporal_quality(&self) -> TemporalQuality {
        self.options.temporal.map_or_else(Default::default, |t| t.quality)
    }

    /// The picture size for a window of `full` pixels at the chosen quality.
    pub(crate) fn temporal_render_size(&mut self, full: (u32, u32)) -> (u32, u32) {
        let quality = self.temporal_quality();
        match self.temporal.as_mut().map(|t| &mut t.runtime) {
            Some(Runtime::Dlss(r)) => r.render_size(quality, full),
            _ => fallback_render_size(quality, full),
        }
    }

    /// Makes the upscaler's textures for these sizes if they are not the current ones.
    pub(crate) fn temporal_targets(&mut self, render: (u32, u32), output: (u32, u32)) {
        let Some(blit_buf) = self.temporal.as_ref().filter(|d| !d.targets.as_ref().is_some_and(|t| t.render == render && t.output == output)).map(|d| d.blit_buf.clone()) else {
            return;
        };
        let tex = |label: &str, (w, h): (u32, u32), format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let targets = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        // The resource itself is plain UNORM, and the scene draws (and the fallback samples)
        // through an sRGB view of it: on cards that cast fully typed formats wgpu would make an
        // sRGB resource sRGB-typed, FSR would read linear numbers from it and the picture would
        // come out much too dark. This way anyone reading the resource as it is gets the
        // sRGB-encoded bytes, which is what both upscalers are told they get.
        // (COPY_SRC as on the render scale's target: the rain on the glass copies the picture)
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("temporal scene"),
            size: wgpu::Extent3d { width: render.0, height: render.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format.remove_srgb_suffix(),
            usage: targets | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[self.format],
        });
        let motion = tex("temporal motion vectors", render, TEMPORAL_MOTION_FORMAT, targets);
        let mask = tex("temporal transparency mask", render, TEMPORAL_MASK_FORMAT, targets);
        // (COPY_SRC: a player's screenshot is read from it, `temporal_picture`)
        let output_tex = tex("temporal output", output, TEMPORAL_OUTPUT_FORMAT, targets | wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC);
        let color_view = color.create_view(&wgpu::TextureViewDescriptor { format: Some(self.format), ..Default::default() });
        let motion_view = motion.create_view(&Default::default());
        let mask_view = mask.create_view(&Default::default());
        let output_view = output_tex.create_view(&Default::default());
        let color_raw_view = color.create_view(&Default::default());
        let color_bg = self.picture_group(&color_view);
        let output_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("temporal output"),
            layout: &self.scale.upscale_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: blit_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&output_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.tone.post_sampler) },
            ],
        });
        let d = self.temporal.as_mut().expect("temporal state");
        // (the old set may still be in the upscaler's last list, so it is held a few frames; its
        // history does not fit the new size)
        if let Some(old) = d.targets.take() {
            d.keep.push_back(vec![old.color, old.motion, old.mask, old.output_tex]);
        }
        d.prev = None;
        d.targets = Some(TemporalTargets { render, output, color, color_view, color_raw_view, color_bg, motion, motion_view, mask, mask_view, output_tex, output_view, output_bg });
    }

    /// The camera side of an upscaled frame. Returns the view-projection shifted by this frame's
    /// jitter, writes the motion shader's uniform, and leaves what `temporal_present` needs in
    /// `temporal_pending`. The motion is worked out from the unjittered matrices.
    pub(crate) fn temporal_jitter(&mut self, scene: &Scene, f: &FrameCtx, vp_plain: Mat4) -> Mat4 {
        let (width, height, full_w, full_h, camera, ro) = (f.width, f.height, f.full_w, f.target.texture().height(), f.camera, f.ro);
        let Some(pipes) = self.temporal_pipes.as_ref() else { return vp_plain };
        let Some(d) = self.temporal.as_mut() else { return vp_plain };
        d.frame += 1;
        let i = (d.frame % jitter_phases(width, full_w) as u64) as u32 + 1;
        let jitter = [halton(i, 2) - 0.5, halton(i, 3) - 0.5];
        // (a shift of the clip space by that many pixels; y grows downwards on the screen)
        let shift = Mat4::from_translation(Vec3::new(2.0 * jitter[0] / width as f32, -2.0 * jitter[1] / height as f32, 0.0));
        // the last frame counts as history only if it was drawn the same way
        let prev = d.prev.filter(|p| p.render == (width, height) && p.output == (full_w, full_h));
        // (a camera cut, or a turn too quick for any history to help)
        let jumped = prev.is_some_and(|p| (p.camera.position - camera.position).length() > 30.0 || p.camera.forward().dot(camera.forward()) < 0.8);
        let (vp_prev, vp_prev_here) = match prev {
            Some(p) => (p.camera.view_proj(p.aspect, p.origin), p.camera.view_proj(p.aspect, ro)),
            None => (vp_plain, vp_plain),
        };
        let origin_delta = prev.map_or(Vec3::ZERO, |p| (p.origin - ro).as_vec3());
        // the copy of last frame's model matrices: grown with the scene's buffer, filled at the
        // end of every frame (`temporal_end_of_scene`)
        if let Some(GpuArray::Buffer(models)) = scene.model_buf.as_ref() {
            if d.prev_models.as_ref().is_none_or(|(b, _)| b.size() < models.size()) {
                let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("temporal last models"),
                    size: models.size(),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("temporal motion"),
                    layout: &pipes.motion_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: d.motion_buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: buf.as_entire_binding() },
                    ],
                });
                d.prev_models = Some((buf, bg));
                d.prev_epoch = u64::MAX;
            }
        }
        let models_ok = prev.is_some() && d.prev_epoch == scene.model_epoch && d.prev_models.is_some();
        let mu = MotionUniform {
            vp: vp_plain.to_cols_array_2d(),
            vp_prev: vp_prev.to_cols_array_2d(),
            vp_prev_here: vp_prev_here.to_cols_array_2d(),
            vp_inv: vp_plain.inverse().to_cols_array_2d(),
            params: [width as f32, height as f32, if models_ok { 1.0 } else { 0.0 }, d.prev_entries as f32],
            origin_delta: [origin_delta.x, origin_delta.y, origin_delta.z, MAX_FRAME_JUMP],
        };
        self.queue.write_buffer(&d.motion_buf, 0, bytemuck::bytes_of(&mu));
        self.temporal_pending = Some(TemporalFrame { vp: vp_plain, vp_prev_here, jitter, reset: prev.is_none() || jumped, aspect: f.aspect, dt: f.dt });
        shift * vp_plain
    }

    /// Last commands of the scene encoder in an upscaled frame.
    ///
    /// First the model matrices get copied for next frame's motion. Then two empty render passes
    /// touch the upscaler's textures: wgpu leaves a texture in whatever state it was last used
    /// in, and the upscalers are told "render target" (color, motion, mask, output) and "depth
    /// write" (depth), so these passes make sure that is what the last use was.
    pub(crate) fn temporal_end_of_scene(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene) {
        let Some(d) = self.temporal.as_mut() else { return };
        // (this frame's prepass has already read the old copy: it was recorded earlier)
        if let (Some(GpuArray::Buffer(models)), Some((prev, _))) = (scene.model_buf.as_ref(), d.prev_models.as_ref()) {
            let bytes = (scene.uploaded_entries as u64 * 64).min(models.size()).min(prev.size());
            if bytes > 0 {
                encoder.copy_buffer_to_buffer(models, 0, prev, 0, bytes);
            }
            d.prev_epoch = scene.model_epoch;
            d.prev_entries = (bytes / 64) as u32;
        }
        let (Some(t), Some(ao)) = (d.targets.as_ref(), self.targets.ao.as_ref()) else { return };
        let keep = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })
        };
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("temporal inputs"),
            color_attachments: &[keep(&t.color_view), keep(&t.motion_view), keep(&t.mask_view)],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &ao.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("temporal output"),
            color_attachments: &[keep(&t.output_view)],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
    }

    /// Runs the upscaler once the scene is submitted, then puts its result and the HUD in the
    /// window. If it fails this frame shows the small picture stretched, as the render scale
    /// would, and it is not tried again for the rest of the session.
    pub(crate) fn temporal_present(&mut self, scene: &Scene, f: &FrameCtx, frame: TemporalFrame) {
        let (target, camera, ro) = (f.target, f.camera, f.ro);
        let render = (f.width, f.height);
        let full = (f.full_w, target.texture().height());
        let overlays = f.overlays.len();
        let quality = self.temporal_quality();
        let (Some(d), Some(ao)) = (self.temporal.as_mut(), self.targets.ao.as_ref()) else { return };
        let Some(t) = d.targets.as_ref() else { return };
        let fov = camera.fov_deg.to_radians();
        let proj = Mat4::perspective_rh(fov, frame.aspect, camera.far, camera.near);
        let constants = FrameConstants {
            view_to_clip: sl_matrix(proj),
            clip_to_view: sl_matrix(proj.inverse()),
            clip_to_prev_clip: sl_matrix(frame.vp_prev_here * frame.vp.inverse()),
            prev_clip_to_clip: sl_matrix(frame.vp * frame.vp_prev_here.inverse()),
            jitter: frame.jitter,
            camera_pos: (camera.position - ro).as_vec3().to_array(),
            camera_up: camera.up().to_array(),
            camera_right: camera.right().to_array(),
            camera_fwd: camera.forward().to_array(),
            near: camera.near,
            far: camera.far,
            fov,
            aspect: frame.aspect,
            reset: frame.reset,
            frame_ms: frame.dt * 1000.0,
        };
        let inputs = Inputs { color: &t.color, depth: &ao.depth_tex, motion: &t.motion, mask: &t.mask, output: &t.output_tex, render_size: render, output_size: full, views: [&t.color_raw_view, &ao.depth_view, &t.motion_view, &t.mask_view, &t.output_view] };
        let result = match &mut d.runtime {
            Runtime::Dlss(r) => r.evaluate(&self.queue, quality, &inputs, &constants),
            Runtime::Fsr3(r) => r.upscale(&self.queue, &inputs, &constants),
        };
        let ok = match result {
            Ok(()) => true,
            Err(e) => {
                log::error!("upscaling stopped working ({e:#}); the picture is drawn without it from now on");
                false
            }
        };
        d.keep.push_back(vec![t.color.clone(), ao.depth_tex.clone(), t.motion.clone(), t.mask.clone(), t.output_tex.clone()]);
        while d.keep.len() > 4 {
            d.keep.pop_front();
        }
        d.prev = Some(TemporalPrev { camera: *camera, aspect: frame.aspect, origin: ro, render, output: full });
        // (`upscale.wgsl`'s third mode: the upscalers return the sRGB-encoded numbers they were
        // given, and writing them to the sRGB window unconverted would encode them twice)
        let sharpen = if matches!(d.runtime, Runtime::Dlss(_)) { DLSS_SHARPEN } else { 0.0 };
        self.queue.write_buffer(&d.blit_buf, 0, bytemuck::cast_slice(&[full.0 as f32, full.1 as f32, sharpen, 2.0]));
        let bg = if ok { &t.output_bg } else { &t.color_bg };
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("temporal hud") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("temporal hud"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.scale.upscale_pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1);
            if overlays > 0 {
                pass.set_pipeline(&self.overlay_pipeline_1x);
                for k in 0..overlays {
                    if let Some((_, _, bg, _)) = scene.overlay_res.get(k) {
                        pass.set_bind_group(0, bg, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        if !ok {
            self.temporal_failed = true;
        }
    }
}
