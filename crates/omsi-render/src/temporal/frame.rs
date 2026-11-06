//! A frame with temporal upscaling: `begin_frame` draws the picture small into the upscaler's
//! input, `temporal_jitter` shifts it by a sub-pixel amount every frame, the depth prepass
//! writes each pixel's motion (`motion.wgsl`), and `temporal_present` runs the upscaler on
//! wgpu's queue, then the HUD at full size.

use super::{fallback_render_size, fidelityfx, streamline, FrameConstants, Inputs, TemporalQuality, TemporalUpscaler};
use crate::passes::FrameCtx;
use crate::pipelines::common::uniform_buffer;
use crate::*;

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

/// How much `upscale.wgsl` sharpens DLSS's result: DLSS no longer sharpens, FSR 3 does.
const DLSS_SHARPEN: f32 = 0.25;

pub(crate) struct TemporalPipelines {
    pub(crate) prepass: [wgpu::RenderPipeline; 6],
    pub(crate) late_motion: [wgpu::RenderPipeline; 6],
    pub(crate) sky: wgpu::RenderPipeline,
    pub(crate) motion_layout: wgpu::BindGroupLayout,
    pub(crate) glass_mask: [wgpu::RenderPipeline; 2],
    pub(crate) rain_mask: [wgpu::RenderPipeline; 2],
    pub(crate) screen_mask: [wgpu::RenderPipeline; 2],
}

pub(crate) struct TemporalTargets {
    render: (u32, u32),
    output: (u32, u32),
    color: wgpu::Texture,
    pub(crate) color_view: wgpu::TextureView,
    color_raw_view: wgpu::TextureView,
    pub(crate) color_bg: wgpu::BindGroup,
    motion: wgpu::Texture,
    pub(crate) motion_view: wgpu::TextureView,
    mask: wgpu::Texture,
    pub(crate) mask_view: wgpu::TextureView,
    output_tex: wgpu::Texture,
    output_view: wgpu::TextureView,
    output_bg: wgpu::BindGroup,
}

#[derive(Clone, Copy)]
struct TemporalPrev {
    camera: Camera,
    aspect: f32,
    origin: DVec3,
    render: (u32, u32),
    output: (u32, u32),
}

pub(crate) enum Runtime {
    Dlss(streamline::Runtime),
    Fsr3(fidelityfx::Upscaler),
}

pub(crate) struct TemporalState {
    runtime: Runtime,
    pub(crate) targets: Option<TemporalTargets>,
    /// The textures of the last few frames, held a little longer: our D3D12 list runs after
    /// wgpu's submit, and wgpu would otherwise free a texture it thinks nobody needs.
    keep: std::collections::VecDeque<Vec<wgpu::Texture>>,
    motion_buf: wgpu::Buffer,
    blit_buf: wgpu::Buffer,
    pub(crate) prev_models: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    prev_epoch: u64,
    prev_entries: u32,
    prev: Option<TemporalPrev>,
    frame: u64,
}

pub(crate) struct TemporalFrame {
    vp: Mat4,
    vp_prev_here: Mat4,
    jitter: [f32; 2],
    reset: bool,
    aspect: f32,
    dt: f32,
}

pub(super) fn halton(mut i: u32, base: u32) -> f32 {
    let (mut f, mut r) = (1.0f32, 0.0f32);
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

pub(super) fn sl_matrix(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

pub(super) fn jitter_phases(render_w: u32, full_w: u32) -> u32 {
    let ratio = (full_w as f32 / render_w.max(1) as f32).max(1.0);
    ((8.0 * ratio * ratio).round() as u32).clamp(8, 64)
}

pub(super) fn opaque(mut rgba: Vec<u8>) -> Vec<u8> {
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    rgba
}

pub(crate) fn window_fits(full: (u32, u32)) -> bool {
    full.0 >= 64 && full.1 >= 64
}

/// The textures' mip bias of an upscaled frame: log2(render width / window width), -0.79 at
/// Balanced. Without it the textures are sampled for the small picture and the cab goes soft.
pub(crate) fn temporal_mip_bias(render_w: u32, full_w: u32) -> f32 {
    (render_w.max(1) as f32 / full_w.max(1) as f32).log2().clamp(-3.0, 0.0)
}

impl Renderer {
    /// Whether a window of this size can be upscaled at all. A minimised one is 1x1: asked for
    /// that, NGX refuses (0xbad00005) and DLSS would be gone for the session. Its history does
    /// not outlive the gap.
    pub(crate) fn temporal_fits(&mut self, full: (u32, u32)) -> bool {
        let fits = window_fits(full);
        if !fits {
            self.temporal_forget();
        }
        fits
    }

    /// The window was drawn without the upscaler: its history and last picture are out of date.
    pub(crate) fn temporal_forget(&mut self) {
        if let Some(d) = self.temporal.as_mut() {
            d.prev = None;
        }
    }

    /// The running upscaler, its GPU work done, to go on in a renderer rebuilt on this device.
    pub(crate) fn temporal_runtime(&mut self) -> Option<Runtime> {
        let mut d = self.temporal.take()?;
        match &mut d.runtime {
            Runtime::Dlss(r) => r.wait(),
            Runtime::Fsr3(r) => r.wait(),
        }
        Some(d.runtime)
    }

    pub(crate) fn temporal_start(&mut self) -> bool {
        if self.temporal_failed {
            if self.temporal.take().is_some() {
                // (its last commands may still use the textures that go with it)
                let _ = crate::wait_gpu(&self.device, None);
            }
            return false;
        }
        if self.temporal.is_some() {
            return true;
        }
        let Some(asked) = self.options.temporal.filter(|_| self.temporal_pipes.is_some()) else { return false };
        let loaded = match (asked.upscaler, self.temporal_kept.take()) {
            (TemporalUpscaler::Dlss, Some(r @ Runtime::Dlss(_))) | (TemporalUpscaler::Fsr3, Some(r @ Runtime::Fsr3(_))) => Ok(r),
            (TemporalUpscaler::Dlss, _) => streamline::Runtime::load(&self.device).map(Runtime::Dlss),
            (TemporalUpscaler::Fsr3, _) => fidelityfx::Upscaler::load(&self.device).map(Runtime::Fsr3),
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

    /// The last upscaled picture, before the HUD: what a player's screenshot shows. None when none
    /// was upscaled. sRGB-encoded, without DLSS's light sharpening.
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

    pub(crate) fn temporal_render_size(&mut self, full: (u32, u32)) -> (u32, u32) {
        let quality = self.temporal_quality();
        match self.temporal.as_mut().map(|t| &mut t.runtime) {
            Some(Runtime::Dlss(r)) => r.render_size(quality, full),
            _ => fallback_render_size(quality, full),
        }
    }

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
        // The resource is plain UNORM, drawn through an sRGB view: cards that cast fully typed formats
        // would make an sRGB resource sRGB-typed, and FSR would read linear numbers, too dark.
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
        if let Some(old) = d.targets.take() {
            d.keep.push_back(vec![old.color, old.motion, old.mask, old.output_tex]);
        }
        d.prev = None;
        d.targets = Some(TemporalTargets { render, output, color, color_view, color_raw_view, color_bg, motion, motion_view, mask, mask_view, output_tex, output_view, output_bg });
    }

    pub(crate) fn temporal_jitter(&mut self, scene: &Scene, f: &FrameCtx, vp_plain: Mat4) -> Mat4 {
        let (width, height, full_w, full_h, camera, ro) = (f.width, f.height, f.full_w, f.target.texture().height(), f.camera, f.ro);
        let Some(pipes) = self.temporal_pipes.as_ref() else { return vp_plain };
        let Some(d) = self.temporal.as_mut() else { return vp_plain };
        d.frame += 1;
        let i = (d.frame % jitter_phases(width, full_w) as u64) as u32 + 1;
        let jitter = [halton(i, 2) - 0.5, halton(i, 3) - 0.5];
        let shift = Mat4::from_translation(Vec3::new(2.0 * jitter[0] / width as f32, -2.0 * jitter[1] / height as f32, 0.0));
        let prev = d.prev.filter(|p| p.render == (width, height) && p.output == (full_w, full_h));
        let jumped = prev.is_some_and(|p| (p.camera.position - camera.position).length() > 30.0 || p.camera.forward().dot(camera.forward()) < 0.8);
        let (vp_prev, vp_prev_here) = match prev {
            Some(p) => (p.camera.view_proj(p.aspect, p.origin), p.camera.view_proj(p.aspect, ro)),
            None => (vp_plain, vp_plain),
        };
        let origin_delta = prev.map_or(Vec3::ZERO, |p| (p.origin - ro).as_vec3());
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

    /// Last commands of the scene encoder in an upscaled frame: the model matrices copied for the
    /// next frame's motion, and two empty passes so that the upscalers find their textures in the
    /// states they are told (render target, depth write).
    pub(crate) fn temporal_end_of_scene(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene) {
        let Some(d) = self.temporal.as_mut() else { return };
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

    pub(crate) fn temporal_present(&mut self, scene: &Scene, f: &FrameCtx, frame: TemporalFrame) {
        let (target, camera, ro) = (f.target, f.camera, f.ro);
        let render = (f.width, f.height);
        let full = (f.full_w, target.texture().height());
        let overlays = f.overlays.len();
        let quality = self.temporal_quality();
        let (Some(d), Some(ao)) = (self.temporal.as_mut(), self.targets.ao.as_ref()) else { return };
        let Some(t) = d.targets.as_ref() else { return };
        let fov = camera.fov_deg.to_radians();
        let proj = Mat4::perspective_rh(fov, frame.aspect, camera.depth_far(), camera.near);
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
            far: camera.depth_far(),
            fov,
            aspect: frame.aspect,
            reset: frame.reset,
            frame_ms: frame.dt * 1000.0,
        };
        let inputs = Inputs { color: &t.color, depth: &ao.depth_tex, motion: &t.motion, mask: &t.mask, output: &t.output_tex, render_size: render, output_size: full, views: [&t.color_raw_view, &ao.depth_view, &t.motion_view, &t.mask_view, &t.output_view] };
        let clock = std::time::Instant::now();
        let result = match &mut d.runtime {
            Runtime::Dlss(r) => r.evaluate(&self.queue, quality, &inputs, &constants),
            Runtime::Fsr3(r) => r.upscale(&self.queue, &inputs, &constants),
        };
        let upscaled = clock.elapsed();
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
        if !ok {
            // (the plain picture's bind group reads this buffer, which no upscaled frame writes)
            self.queue.write_buffer(&self.scale.upscale_buf, 0, bytemuck::cast_slice(&[render.0 as f32, render.1 as f32, 0.0, 0.0]));
        }
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
        if self.profiling {
            let mut stats = self.stats.borrow_mut();
            *stats.entry("upscale").or_default() += upscaled.as_secs_f64();
            *stats.entry("upscale hud").or_default() += (clock.elapsed() - upscaled).as_secs_f64();
        }
        if !ok {
            self.temporal_failed = true;
        }
    }
}
