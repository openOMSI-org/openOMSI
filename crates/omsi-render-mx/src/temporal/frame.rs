//! A frame with temporal upscaling: `begin_frame` draws the picture small into the upscaler's
//! input, `temporal_jitter` shifts it by a sub-pixel amount every frame, the depth prepass
//! writes each pixel's motion (`motion.wgsl`), and `temporal_present` runs the upscaler on
//! mX's queue (`mx-upscale`), then the HUD at full size.

use super::{FrameConstants, Inputs, TemporalQuality, fallback_render_size, upscale_options};
use crate::passes::FrameCtx;
use crate::pipelines::common::uniform_buffer;
use crate::*;

pub(crate) const TEMPORAL_MOTION_FORMAT: mx::TextureFormat = mx::TextureFormat::Rg16Float;
/// The window-sized result: a storage texture, and those can't be sRGB. It only ever holds the
/// sRGB-encoded numbers of the 8 bit picture the upscaler was given.
const TEMPORAL_OUTPUT_FORMAT: mx::TextureFormat = mx::TextureFormat::Rgba8Unorm;
/// Where glass, its reflections and the rain on it lie (see `vs_mask`). Not R8: Streamline's
/// Vulkan layer cannot read VK_FORMAT_R8_UNORM back (`getFormat`), and logged four errors a frame.
pub(crate) const TEMPORAL_MASK_FORMAT: mx::TextureFormat = mx::TextureFormat::R16Float;

/// An object moved further than this in one frame (m) gets no motion of its own. The engine hands
/// a freed entry to the next object loaded (`recycle_instance`), so for one frame the "last
/// matrix" can be some other house half the map away: the history pulled from there would flash
/// bits of picture over the scene. 8 m is about 290 km/h at 10 fps.
const MAX_FRAME_JUMP: f32 = 8.0;

/// How much `upscale.wgsl` sharpens DLSS's result unless the settings say otherwise
/// (`RenderOptions::dlss_sharpness`): DLSS no longer sharpens itself, FSR 3 does.
pub const DLSS_SHARPEN: f32 = 0.25;

/// How much `upscale.wgsl` sharpens MetalFX's result, which MetalFX never sharpens itself. Chosen
/// on St-Servan's pictures with `METALFX_MIP_BIAS`: 0.6 brought the gauges' numbers and the bark
/// closest to the picture drawn at the window's size, but the foliage shimmered more while the
/// view turned; no value up to 0.6 drew halos around the lamp posts against the sky.
const METALFX_SHARPEN: f32 = 0.4;

/// MetalFX's textures one mip level sharper than the render scale alone gives, as Apple advises
/// for its temporal scaler (log2(render / output) - 1, WWDC22 "Boost performance with MetalFX
/// Upscaling"): -1.58 at Quality. Without it the result was softer than the picture drawn at
/// the window's size (bark, the gauges' numbers, grass).
const METALFX_MIP_BIAS: f32 = -1.0;

/// The motion vectors' uniform and the last frame's model places in the heap: the motion
/// pipelines read them at the scene shader's `lights` and `grid` places, which they do not
/// otherwise read (temporal/motion.wgsl; `frame_from_heap`).
pub(crate) struct MotionHeap {
    views: [mx::BufferView; 2],
}

impl MotionHeap {
    /// The frame's places with the motion's in `lights` and `grid`.
    pub(crate) fn heap(&self, frame: &FrameHeap) -> FrameHeap {
        FrameHeap {
            lights: self.views[0].index(),
            grid: self.views[1].index(),
            ..*frame
        }
    }
}

pub(crate) struct TemporalPipelines {
    pub(crate) prepass: [mx::RenderPipeline; 6],
    pub(crate) late_motion: [mx::RenderPipeline; 6],
    pub(crate) sky: mx::RenderPipeline,
    pub(crate) glass_mask: [mx::RenderPipeline; 2],
    pub(crate) rain_mask: [mx::RenderPipeline; 2],
    pub(crate) screen_mask: [mx::RenderPipeline; 2],
}

pub(crate) struct TemporalTargets {
    render: (u32, u32),
    output: (u32, u32),
    color: mx::Texture,
    pub(crate) color_view: mx::TextureView,
    pub(crate) color_bg: HeapGroup,
    motion: mx::Texture,
    pub(crate) motion_view: mx::TextureView,
    mask: mx::Texture,
    pub(crate) mask_view: mx::TextureView,
    output_tex: mx::Texture,
    output_bg: HeapGroup,
}

#[derive(Clone, Copy)]
struct TemporalPrev {
    camera: Camera,
    aspect: f32,
    origin: DVec3,
    render: (u32, u32),
    output: (u32, u32),
}

/// The vendor's upscaler (`mx-upscale`).
pub(crate) type Runtime = mx_upscale::Upscaler;

pub(crate) struct TemporalState {
    runtime: Runtime,
    pub(crate) targets: Option<TemporalTargets>,
    /// The textures of the last few frames, held a little longer: our D3D12 list runs after
    /// wgpu's submit, and wgpu would otherwise free a texture it thinks nobody needs.
    keep: std::collections::VecDeque<Vec<mx::Texture>>,
    motion_buf: mx::Buffer,
    blit_buf: mx::Buffer,
    pub(crate) prev_models: Option<(mx::Buffer, MotionHeap)>,
    prev_epoch: u64,
    prev_entries: u32,
    prev: Option<TemporalPrev>,
    frame: u64,
    /// For frame generation: the window's picture before the HUD, two in turn (this frame's and
    /// the last one's), and the frame the newest was drawn in.
    hudless: Vec<(mx::Texture, mx::TextureView)>,
    hudless_frame: Option<u64>,
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
/// Balanced, plus the upscaler's own `extra`. Without it the textures are sampled for the small
/// picture and the cab goes soft.
pub(crate) fn temporal_mip_bias(render_w: u32, full_w: u32, extra: f32) -> f32 {
    ((render_w.max(1) as f32 / full_w.max(1) as f32).log2() + extra).clamp(-3.0, 0.0)
}

/// A picture of the window's size and format for frame generation: a plain UNORM resource (the
/// sRGB-encoded numbers, as the upscaler's input), drawn through the renderer's sRGB view.
fn hudless_target(
    device: &mx::Device,
    (width, height): (u32, u32),
    format: mx::TextureFormat,
) -> (mx::Texture, mx::TextureView) {
    let texture = device
        .create_texture(&mx::TextureDescriptor {
            label: Some("frame generation: the picture before the HUD"),
            size: mx::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: mx::TextureDimension::D2,
            format: format.remove_srgb_suffix(),
            usage: mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::TEXTURE_BINDING,
            view_formats: &[format],
        })
        .expect("mX: a texture");
    let view = texture
        .create_view(&mx::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        })
        .expect("mX: a texture view");
    (texture, view)
}

impl Renderer {
    /// This upscaled frame's mip bias (`temporal_mip_bias`): MetalFX's one level sharper.
    pub(crate) fn temporal_textures_bias(&self, render_w: u32, full_w: u32) -> f32 {
        let extra = match self.temporal.as_ref().map(|d| &d.runtime) {
            Some(r) if r.method() == mx_upscale::Method::MetalFxTemporal => METALFX_MIP_BIAS,
            _ => 0.0,
        };
        temporal_mip_bias(render_w, full_w, extra)
    }

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
        d.runtime.wait();
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
        let Some(asked) = self
            .options
            .temporal
            .filter(|_| self.temporal_pipes.is_some())
        else {
            return false;
        };
        let method = asked.upscaler.method();
        let loaded = match self.temporal_kept.take() {
            Some(r) if r.method() == method => Ok(r),
            _ => Runtime::new(&self.device, method, &upscale_options()),
        };
        match loaded {
            Ok(runtime) => {
                self.temporal = Some(TemporalState {
                    runtime,
                    targets: None,
                    keep: Default::default(),
                    motion_buf: uniform_buffer(
                        &self.device,
                        "temporal motion",
                        std::mem::size_of::<MotionUniform>() as u64,
                    ),
                    blit_buf: uniform_buffer(&self.device, "temporal blit", 16),
                    prev_models: None,
                    prev_epoch: u64::MAX,
                    prev_entries: 0,
                    prev: None,
                    frame: 0,
                    hudless: Vec::new(),
                    hudless_frame: None,
                });
                log::info!(
                    "{} upscaling ({}) running on {}",
                    asked.upscaler.as_str(),
                    asked.quality.as_str(),
                    self.adapter_name
                );
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
        Some(
            self.read_texture(&t.output_tex, mx::TextureAspect::All)
                .map(opaque),
        )
    }

    fn temporal_quality(&self) -> TemporalQuality {
        self.options
            .temporal
            .map_or_else(Default::default, |t| t.quality)
    }

    pub(crate) fn temporal_render_size(&mut self, full: (u32, u32)) -> (u32, u32) {
        let quality = self.temporal_quality();
        match self.temporal.as_mut().map(|t| &mut t.runtime) {
            Some(r) => r.render_size(quality.mx(), full),
            None => fallback_render_size(quality, full),
        }
    }

    pub(crate) fn temporal_targets(&mut self, render: (u32, u32), output: (u32, u32)) {
        let Some(blit_buf) = self
            .temporal
            .as_ref()
            .filter(|d| {
                !d.targets
                    .as_ref()
                    .is_some_and(|t| t.render == render && t.output == output)
            })
            .map(|d| d.blit_buf.clone())
        else {
            return;
        };
        let tex = |label: &str,
                   (w, h): (u32, u32),
                   format: mx::TextureFormat,
                   usage: mx::TextureUsages| {
            self.device
                .create_texture(&mx::TextureDescriptor {
                    label: Some(label),
                    size: mx::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .expect("mX: a texture")
        };
        let targets = mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::TEXTURE_BINDING;
        // The resource is plain UNORM, drawn through an sRGB view: cards that cast fully typed formats
        // would make an sRGB resource sRGB-typed, and FSR would read linear numbers, too dark.
        let color = self
            .device
            .create_texture(&mx::TextureDescriptor {
                label: Some("temporal scene"),
                size: mx::Extent3d {
                    width: render.0,
                    height: render.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: self.format.remove_srgb_suffix(),
                usage: targets | mx::TextureUsages::COPY_SRC,
                view_formats: &[self.format],
            })
            .expect("mX: a texture");
        let motion = tex(
            "temporal motion vectors",
            render,
            TEMPORAL_MOTION_FORMAT,
            targets,
        );
        let mask = tex(
            "temporal transparency mask",
            render,
            TEMPORAL_MASK_FORMAT,
            targets,
        );
        let output_tex = tex(
            "temporal output",
            output,
            TEMPORAL_OUTPUT_FORMAT,
            targets | mx::TextureUsages::STORAGE_BINDING | mx::TextureUsages::COPY_SRC,
        );
        let color_view = color
            .create_view(&mx::TextureViewDescriptor {
                format: Some(self.format),
                ..Default::default()
            })
            .expect("mX: a texture view");
        let motion_view = motion
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let mask_view = mask
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let output_view = output_tex
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let color_bg = self.picture_group(&color_view);
        let output_bg = HeapGroup::of(
            picture_layout(),
            0,
            &mx::BindGroupDescriptor {
                label: Some("temporal output"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: blit_buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::TextureView(&output_view),
                    },
                    mx::BindGroupEntry {
                        binding: 2,
                        resource: mx::BindingResource::Sampler(&self.tone.post_sampler),
                    },
                ],
            },
        );
        let d = self.temporal.as_mut().expect("temporal state");
        if let Some(old) = d.targets.take() {
            d.keep
                .push_back(vec![old.color, old.motion, old.mask, old.output_tex]);
        }
        d.prev = None;
        d.targets = Some(TemporalTargets {
            render,
            output,
            color,
            color_view,
            color_bg,
            motion,
            motion_view,
            mask,
            mask_view,
            output_tex,
            output_bg,
        });
    }

    pub(crate) fn temporal_jitter(&mut self, scene: &Scene, f: &FrameCtx, vp_plain: Mat4) -> Mat4 {
        let (width, height, full_w, full_h, camera, ro) = (
            f.width,
            f.height,
            f.full_w,
            f.target.texture().height(),
            f.camera,
            f.ro,
        );
        if self.temporal_pipes.is_none() {
            return vp_plain;
        }
        let Some(d) = self.temporal.as_mut() else {
            return vp_plain;
        };
        d.frame += 1;
        let i = (d.frame % jitter_phases(width, full_w) as u64) as u32 + 1;
        let jitter = [halton(i, 2) - 0.5, halton(i, 3) - 0.5];
        let shift = Mat4::from_translation(Vec3::new(
            2.0 * jitter[0] / width as f32,
            -2.0 * jitter[1] / height as f32,
            0.0,
        ));
        let prev = d
            .prev
            .filter(|p| p.render == (width, height) && p.output == (full_w, full_h));
        let jumped = prev.is_some_and(|p| {
            (p.camera.position - camera.position).length() > 30.0
                || p.camera.forward().dot(camera.forward()) < 0.8
        });
        let (vp_prev, vp_prev_here) = match prev {
            Some(p) => (
                p.camera.view_proj(p.aspect, p.origin),
                p.camera.view_proj(p.aspect, ro),
            ),
            None => (vp_plain, vp_plain),
        };
        let origin_delta = prev.map_or(Vec3::ZERO, |p| (p.origin - ro).as_vec3());
        if let Some(GpuArray::Buffer(models)) = scene.model_buf.as_ref() {
            if d.prev_models
                .as_ref()
                .is_none_or(|(b, _)| b.size() < models.size())
            {
                let buf = self
                    .device
                    .create_buffer(&mx::BufferDescriptor {
                        label: Some("temporal last models"),
                        size: models.size(),
                        usage: mx::BufferUsages::STORAGE | mx::BufferUsages::COPY_DST,
                    })
                    .expect("mX: a buffer");
                let views = [
                    crate::pipelines::common::heap_view(&d.motion_buf),
                    crate::pipelines::common::heap_view(&buf),
                ];
                d.prev_models = Some((buf, MotionHeap { views }));
                d.prev_epoch = u64::MAX;
            }
        }
        let models_ok =
            prev.is_some() && d.prev_epoch == scene.model_epoch && d.prev_models.is_some();
        let mu = MotionUniform {
            vp: vp_plain.to_cols_array_2d(),
            vp_prev: vp_prev.to_cols_array_2d(),
            vp_prev_here: vp_prev_here.to_cols_array_2d(),
            vp_inv: vp_plain.inverse().to_cols_array_2d(),
            params: [
                width as f32,
                height as f32,
                if models_ok { 1.0 } else { 0.0 },
                d.prev_entries as f32,
            ],
            origin_delta: [
                origin_delta.x,
                origin_delta.y,
                origin_delta.z,
                MAX_FRAME_JUMP,
            ],
        };
        self.queue
            .write_buffer(&d.motion_buf, 0, bytemuck::bytes_of(&mu))
            .expect("mX: a queue write");
        self.temporal_pending = Some(TemporalFrame {
            vp: vp_plain,
            vp_prev_here,
            jitter,
            reset: prev.is_none() || jumped,
            aspect: f.aspect,
            dt: f.dt,
        });
        shift * vp_plain
    }

    /// Last commands of the scene encoder in an upscaled frame: the model matrices copied for the
    /// next frame's motion.
    pub(crate) fn temporal_end_of_scene(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        scene: &Scene,
    ) {
        let Some(d) = self.temporal.as_mut() else {
            return;
        };
        if let (Some(GpuArray::Buffer(models)), Some((prev, _))) =
            (scene.model_buf.as_ref(), d.prev_models.as_ref())
        {
            let bytes = (scene.uploaded_entries as u64 * 64)
                .min(models.size())
                .min(prev.size());
            if bytes > 0 {
                encoder.copy_buffer_to_buffer(models, 0, prev, 0, bytes);
            }
            d.prev_epoch = scene.model_epoch;
            d.prev_entries = (bytes / 64) as u32;
        }
        // (the upscalers find the inputs in the states mX leaves every texture in between its
        // commands: no pass is needed to put them in one, as wgpu's last-use states needed)
    }

    pub(crate) fn temporal_present(&mut self, scene: &Scene, f: &FrameCtx, frame: TemporalFrame) {
        let (target, camera, ro) = (f.target, f.camera, f.ro);
        let render = (f.width, f.height);
        let full = (f.full_w, target.texture().height());
        let overlays = f.overlays.len();
        let quality = self.temporal_quality();
        let (Some(d), Some(ao)) = (self.temporal.as_mut(), self.targets.ao.as_ref()) else {
            return;
        };
        let Some(t) = d.targets.as_ref() else { return };
        let fov = camera.fov_deg.to_radians();
        let proj = Mat4::perspective_rh(fov, frame.aspect, camera.depth_far(), camera.near);
        let view = Mat4::look_to_rh(
            (camera.position - ro).as_vec3(),
            camera.forward(),
            camera.up(),
        );
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
            world_to_view: sl_matrix(view),
            view_to_world: sl_matrix(view.inverse()),
            ..Default::default()
        };
        let inputs = Inputs {
            color: &t.color,
            depth: Some(&ao.depth_tex),
            motion: Some(&t.motion),
            mask: Some(&t.mask),
            output: &t.output_tex,
            render_size: render,
            output_size: full,
            guides: None,
        };
        let clock = std::time::Instant::now();
        let result = d
            .runtime
            .upscale(&self.queue, quality.mx(), &inputs, &constants);
        let upscaled = clock.elapsed();
        let ok = match result {
            Ok(()) => true,
            Err(e) => {
                log::error!(
                    "upscaling stopped working ({e:#}); the picture is drawn without it from now on"
                );
                false
            }
        };
        d.keep.push_back(vec![
            t.color.clone(),
            ao.depth_tex.clone(),
            t.motion.clone(),
            t.mask.clone(),
            t.output_tex.clone(),
        ]);
        while d.keep.len() > 4 {
            d.keep.pop_front();
        }
        d.prev = Some(TemporalPrev {
            camera: *camera,
            aspect: frame.aspect,
            origin: ro,
            render,
            output: full,
        });
        // (`upscale.wgsl`'s third mode: the upscalers return the sRGB-encoded numbers they were
        // given, and writing them to the sRGB window unconverted would encode them twice)
        let sharpen = match d.runtime.method() {
            mx_upscale::Method::Dlss | mx_upscale::Method::DlssRayReconstruction => {
                self.options.dlss_sharpness.clamp(0.0, 1.0)
            }
            mx_upscale::Method::MetalFxTemporal | mx_upscale::Method::MetalFxSpatial => {
                METALFX_SHARPEN
            }
            mx_upscale::Method::Fsr3 | mx_upscale::Method::Xess => 0.0,
        };
        self.queue
            .write_buffer(
                &d.blit_buf,
                0,
                bytemuck::cast_slice(&[full.0 as f32, full.1 as f32, sharpen, 2.0]),
            )
            .expect("mX: a queue write");
        let bg = if ok { &t.output_bg } else { &t.color_bg };
        if !ok {
            // (the plain picture's bind group reads this buffer, which no upscaled frame writes)
            self.queue
                .write_buffer(
                    &self.scale.upscale_buf,
                    0,
                    bytemuck::cast_slice(&[render.0 as f32, render.1 as f32, 0.0, 0.0]),
                )
                .expect("mX: a queue write");
        }
        let mut encoder = self.device.create_command_encoder(Some("temporal hud"));
        // Frame generation (which takes the scaler's depth and motion): the same picture as the
        // window's below the HUD, to the bit - it tells the interface apart by what differs
        // between the two.
        let generation = self.frame_generation.clone().filter(|g| ok && g.working());
        let hudless = generation.as_ref().map(|_| {
            if d.hudless
                .first()
                .is_none_or(|(h, _)| (h.width(), h.height()) != full)
            {
                d.hudless = (0..2)
                    .map(|_| hudless_target(&self.device, full, self.format))
                    .collect();
                d.hudless_frame = None;
            }
            let last = d
                .hudless_frame
                .filter(|at| at + 1 == d.frame)
                .map(|_| d.hudless[((d.frame + 1) % 2) as usize].0.clone());
            d.hudless_frame = Some(d.frame);
            (d.hudless[(d.frame % 2) as usize].clone(), last)
        });
        if let Some(((_, view), _)) = hudless.as_ref() {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("frame generation hudless"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view,
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
            pass.set_pipeline(&self.scale.upscale_pipeline);
            bg.set(&mut pass);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("temporal hud"),
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
            pass.set_pipeline(&self.scale.upscale_pipeline);
            bg.set(&mut pass);
            pass.draw(0..3, 0..1);
            if overlays > 0 {
                pass.set_pipeline(&self.overlay_pipeline_1x);
                for k in 0..overlays {
                    if let Some((_, _, bg, _)) = scene.overlay_res.get(k) {
                        bg.set(&mut pass);
                        pass.draw(0..6, 0..1);
                    }
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        if let (Some(g), Some(((picture, _), last_picture))) = (generation, hudless) {
            let mut inputs = mx_upscale::FrameInputs::new(
                picture,
                ao.depth_tex.clone(),
                t.motion.clone(),
                render,
                full,
                constants,
            )
            .with_upscaler(Some(&d.runtime));
            inputs.last_hudless = last_picture.filter(|_| g.wants_last_picture());
            g.prepare(&self.queue, inputs);
        }
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
