//! The window's volumetric clouds (clouds_ss.wgsl): marched per pixel at a quarter of the
//! picture's size, accumulated over the frames into a picture at half its size with the
//! last frame's reprojected and clamped, and looked up by the dome where the sky shows.

use crate::pipelines::common::*;
use crate::*;

/// sky_enhanced.wgsl `CloudView`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CloudView {
    pub(crate) inv_vp: [[f32; 4]; 4],
    pub(crate) vp: [[f32; 4]; 4],
    pub(crate) prev_vp: [[f32; 4]; 4],
    pub(crate) moved: [f32; 4],
    pub(crate) size: [f32; 4],
    pub(crate) full: [f32; 4],
    pub(crate) frame: [f32; 4],
    pub(crate) params: [f32; 4],
}

/// The picture's pixels a side of a traced pixel; the accumulated picture has two pixels a
/// side to every traced one (each the centre of one of the four quarters the trace's rays
/// take in turn).
pub(crate) const TRACE_DIV: u32 = 4;
/// Rounds of a picture drawn on its own (no history): traced and averaged in one frame.
pub(crate) const STILL_ROUNDS: u32 = 8;
const TRACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const TRACE_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

/// The targets for one picture size.
pub(crate) struct CloudTargets {
    size: (u32, u32),
    trace: wgpu::TextureView,
    trace_depth: wgpu::TextureView,
    display_tex: wgpu::Texture,
    display: wgpu::TextureView,
    hist_tex: wgpu::Texture,
    /// The resolve's bind groups, per round (`CloudPass::rounds`).
    resolve_bgs: Vec<wgpu::BindGroup>,
}

/// What the last traced frame was drawn with (its history is found again from it).
struct Prev {
    vp: Mat4,
    cam: DVec3,
    drift: glam::Vec2,
    scale: f32,
    size: (u32, u32),
}

/// The passes, their fixed resources and the targets of the window's size.
pub(crate) struct CloudPass {
    trace_pipeline: wgpu::RenderPipeline,
    resolve_pipeline: wgpu::RenderPipeline,
    resolve_layout: wgpu::BindGroupLayout,
    depth_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Per round (`probe.p` in the shaders): the window's frame (0), then a picture's own
    /// `STILL_ROUNDS`: x the round, y the history's share (< 0: the usual).
    rounds: Vec<wgpu::Buffer>,
    /// The trace's bind groups (the probe's layout), per round.
    trace_bgs: Vec<wgpu::BindGroup>,
    /// A depth texel for a frame without the prepass's depth (nothing is skipped).
    no_depth: wgpu::TextureView,
    /// What the camera group reads before there are clouds: no cloud anywhere.
    placeholder: wgpu::TextureView,
    targets: Option<CloudTargets>,
    frame: u32,
    prev: Option<Prev>,
}

impl CloudPass {
    /// `trace_group` makes a bind group of the probe's layout reading the round buffer it
    /// is given; `probe_layout` is that layout and `sky` the sky's (groups 0 and 1).
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader: &wgpu::ShaderModule,
        probe_layout: &wgpu::BindGroupLayout,
        sky_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        trace_group: &dyn Fn(&wgpu::Buffer) -> wgpu::BindGroup,
    ) -> CloudPass {
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud trace depth"),
            entries: &[texture_entry(
                0,
                wgpu::TextureSampleType::Depth,
                wgpu::TextureViewDimension::D2,
            )],
        });
        let unfiltered = wgpu::TextureSampleType::Float { filterable: false };
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud resolve"),
            entries: &[
                sampler_entry(13),
                uniform_entry(15, wgpu::ShaderStages::VERTEX_FRAGMENT),
                float_texture_entry(23),
                texture_entry(24, unfiltered, wgpu::TextureViewDimension::D2),
                float_texture_entry(25),
            ],
        });
        let trace_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cloud trace"),
            bind_group_layouts: &[Some(probe_layout), Some(sky_layout), Some(&depth_layout)],
            immediate_size: 0,
        });
        let resolve_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cloud resolve"),
            bind_group_layouts: &[Some(&resolve_layout), Some(sky_layout)],
            immediate_size: 0,
        });
        let trace_pipeline = no_vertex_pipeline(
            "cloud trace",
            &trace_pl,
            shader,
            "fs_enhanced_cloud_trace",
            &[target(TRACE_FORMAT, None), target(TRACE_DEPTH_FORMAT, None)],
        )
        .vs("vs_probe")
        .create(device);
        let resolve_pipeline = no_vertex_pipeline(
            "cloud resolve",
            &resolve_pl,
            shader,
            "fs_enhanced_cloud_resolve",
            &[target(TRACE_FORMAT, None)],
        )
        .vs("vs_probe")
        .create(device);
        let rounds: Vec<wgpu::Buffer> = std::iter::once([0.0, -1.0, 0.0, 0.0])
            .chain((0..STILL_ROUNDS).map(|k| [k as f32, k as f32 / (k as f32 + 1.0), 0.0, 0.0]))
            .map(|p: [f32; 4]| {
                buffer_init(
                    device,
                    queue,
                    Some("cloud round"),
                    bytemuck::cast_slice(&p),
                    wgpu::BufferUsages::UNIFORM,
                )
            })
            .collect();
        let trace_bgs = rounds.iter().map(trace_group).collect();
        let texel = |label: &str, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let no_depth = texel(
            "no cloud depth",
            DEPTH_FORMAT,
            wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let placeholder_tex = texel(
            "no clouds",
            TRACE_FORMAT,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        // (no light, everything let through: half floats 0, 0, 0, 1)
        let clear: [u16; 4] = [0, 0, 0, 0x3c00];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &placeholder_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&clear),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        CloudPass {
            trace_pipeline,
            resolve_pipeline,
            resolve_layout,
            depth_layout,
            sampler: sampler.clone(),
            rounds,
            trace_bgs,
            no_depth,
            placeholder: placeholder_tex.create_view(&Default::default()),
            targets: None,
            frame: 0,
            prev: None,
        }
    }

    /// The accumulated clouds the dome reads (the camera group's binding 17).
    pub(crate) fn display_view(&self) -> &wgpu::TextureView {
        self.targets
            .as_ref()
            .map_or(&self.placeholder, |t| &t.display)
    }

    /// The targets for a picture of `size`; true when they were made anew (the camera group
    /// must be made again to read them).
    pub(crate) fn ensure(&mut self, device: &wgpu::Device, size: (u32, u32)) -> bool {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return false;
        }
        let make = |label: &str,
                    w: u32,
                    h: u32,
                    format: wgpu::TextureFormat,
                    usage: wgpu::TextureUsages| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        use wgpu::TextureUsages as U;
        let (tw, th) = (size.0.div_ceil(TRACE_DIV), size.1.div_ceil(TRACE_DIV));
        let (rw, rh) = (2 * tw, 2 * th);
        let trace = make(
            "cloud trace",
            tw,
            th,
            TRACE_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let trace_depth = make(
            "cloud trace depth",
            tw,
            th,
            TRACE_DEPTH_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let display_tex = make(
            "clouds",
            rw,
            rh,
            TRACE_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_SRC,
        );
        let hist_tex = make(
            "cloud history",
            rw,
            rh,
            TRACE_FORMAT,
            U::TEXTURE_BINDING | U::COPY_DST,
        );
        let hist = hist_tex.create_view(&Default::default());
        let resolve_bgs = self
            .rounds
            .iter()
            .map(|round| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("cloud resolve"),
                    layout: &self.resolve_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 13,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 15,
                            resource: round.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 23,
                            resource: wgpu::BindingResource::TextureView(&trace),
                        },
                        wgpu::BindGroupEntry {
                            binding: 24,
                            resource: wgpu::BindingResource::TextureView(&trace_depth),
                        },
                        wgpu::BindGroupEntry {
                            binding: 25,
                            resource: wgpu::BindingResource::TextureView(&hist),
                        },
                    ],
                })
            })
            .collect();
        self.targets = Some(CloudTargets {
            size,
            trace,
            trace_depth,
            display: display_tex.create_view(&Default::default()),
            display_tex,
            hist_tex,
            resolve_bgs,
        });
        self.prev = None;
        true
    }
}

/// What the window's frame traces its clouds with.
pub(crate) struct CloudFrame<'a> {
    /// The frame's view-projection and the camera relative to the render origin and in the
    /// world.
    pub vp: Mat4,
    pub cam_rel: Vec3,
    pub cam: DVec3,
    /// How far the wind has carried the clouds (m), the sky table's scale, the march's
    /// steps, the camera's vertical field of view (rad), the frame time.
    pub drift: glam::Vec2,
    pub scale: f32,
    pub steps: u32,
    pub fov: f32,
    pub dt: f32,
    /// A picture on its own: all its rounds now, no history.
    pub still: bool,
    /// The prepass's depth of the picture (pixels the geometry covers are not marched).
    pub depth: Option<&'a wgpu::TextureView>,
}

impl CloudPass {
    /// Trace and accumulate this frame's clouds (writes the view uniform the dome reads).
    pub(crate) fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        enc: &mut wgpu::CommandEncoder,
        sky_bg: &wgpu::BindGroup,
        view_buf: &wgpu::Buffer,
        fr: &CloudFrame,
        timers: &mut PassTimers,
    ) {
        let Some(t) = self.targets.as_ref() else {
            queue.write_buffer(view_buf, 0, bytemuck::bytes_of(&CloudView::default()));
            return;
        };
        let size = t.size;
        let (tw, th) = (size.0.div_ceil(TRACE_DIV), size.1.div_ceil(TRACE_DIV));
        let (rw, rh) = (2 * tw, 2 * th);
        // the view without its translation: the clouds are looked up by direction
        let vp = fr.vp * Mat4::from_translation(fr.cam_rel);
        let history = self.prev.as_ref().filter(|p| {
            !fr.still && p.size == size && fr.dt < 0.5 && (fr.cam - p.cam).length() < 500.0
        });
        let (prev_vp, moved, weight, ratio) = match history {
            Some(p) => {
                let wind = fr.drift - p.drift;
                (
                    p.vp,
                    (fr.cam - p.cam).as_vec3() - Vec3::new(wind.x, wind.y, 0.0),
                    1.0,
                    p.scale / fr.scale.max(1e-12),
                )
            }
            None => (vp, Vec3::ZERO, 0.0, 1.0),
        };
        self.frame = (self.frame + 1) % 65536;
        let u = CloudView {
            inv_vp: vp.inverse().to_cols_array_2d(),
            vp: vp.to_cols_array_2d(),
            prev_vp: prev_vp.to_cols_array_2d(),
            moved: moved.extend(weight).to_array(),
            size: [tw as f32, th as f32, rw as f32, rh as f32],
            full: [size.0 as f32, size.1 as f32, TRACE_DIV as f32, 0.0],
            frame: [self.frame as f32, 0.0, 0.0, fr.steps as f32],
            params: [1.0, ratio, 2.0 * (fr.fov * 0.5).tan() / th as f32, 0.0],
        };
        queue.write_buffer(view_buf, 0, bytemuck::bytes_of(&u));
        let depth_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud trace depth"),
            layout: &self.depth_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(fr.depth.unwrap_or(&self.no_depth)),
            }],
        });
        let rounds: Vec<usize> = if fr.still {
            (1..=STILL_ROUNDS as usize).collect()
        } else if history.is_none() {
            // Seed all four subpixel phases on the first frame after a reset.
            // A single first-frame trace left blocks that the history kept reusing.
            (1..=4).collect()
        } else {
            vec![0]
        };
        for (n, round) in rounds.into_iter().enumerate() {
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cloud trace"),
                    color_attachments: &[
                        attach(
                            &t.trace,
                            wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            },
                        ),
                        attach(
                            &t.trace_depth,
                            wgpu::Color {
                                r: 1e6,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            },
                        ),
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes: if n == 0 {
                        pass_timer(timers.set.as_ref(), &mut timers.timed, "cloud trace")
                    } else {
                        None
                    },
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.trace_pipeline);
                pass.set_bind_group(0, &self.trace_bgs[round], &[]);
                pass.set_bind_group(1, sky_bg, &[]);
                pass.set_bind_group(2, &depth_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cloud resolve"),
                    color_attachments: &[attach(&t.display, wgpu::Color::BLACK)],
                    depth_stencil_attachment: None,
                    timestamp_writes: if n == 0 {
                        pass_timer(timers.set.as_ref(), &mut timers.timed, "cloud resolve")
                    } else {
                        None
                    },
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.resolve_pipeline);
                pass.set_bind_group(0, &t.resolve_bgs[round], &[]);
                pass.set_bind_group(1, sky_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            enc.copy_texture_to_texture(
                t.display_tex.as_image_copy(),
                t.hist_tex.as_image_copy(),
                wgpu::Extent3d {
                    width: rw,
                    height: rh,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.prev = Some(Prev {
            vp,
            cam: fr.cam,
            drift: fr.drift,
            scale: fr.scale,
            size,
        });
    }
}

fn attach(
    view: &wgpu::TextureView,
    clear: wgpu::Color,
) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(clear),
            store: wgpu::StoreOp::Store,
        },
    })
}

impl CloudPass {
    /// Lets the history go: the next traced frame starts afresh.
    pub(crate) fn forget(&mut self) {
        self.prev = None;
    }
}

impl Renderer {
    /// The window's clouds at its picture's size; true when the camera group must be made
    /// again to read new targets.
    pub(crate) fn ensure_cloud_targets(&mut self, width: u32, height: u32) -> bool {
        match self.tone.probe.as_mut() {
            Some(p) => p.clouds.ensure(&self.device, (width, height)),
            None => false,
        }
    }

    /// The window's clouds for this frame (its lead view only: a mirror and another view
    /// look them up as they are, or take the sky cube).
    pub(crate) fn encode_clouds(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &Scene,
        f: &crate::passes::FrameCtx,
        vp: Mat4,
        timers: &mut PassTimers,
    ) {
        if !(f.enhanced && f.lead_view) {
            return;
        }
        let Some(probe) = self.tone.probe.as_mut() else {
            return;
        };
        // (a clear sky, or the clouds turned off: no trace - its quarter-resolution march
        // ran every frame whatever the sky held - and no history to come back to)
        if f.lighting.cloud_density <= 0.001 {
            probe.clouds.forget();
            self.queue.write_buffer(
                &self.sky.cloud_view_buf,
                0,
                bytemuck::bytes_of(&CloudView::default()),
            );
            return;
        }
        let Some(sky_bg) = scene.sky_bind_group.as_ref() else {
            self.queue.write_buffer(
                &self.sky.cloud_view_buf,
                0,
                bytemuck::bytes_of(&CloudView::default()),
            );
            return;
        };
        let depth = self
            .targets
            .ao
            .as_ref()
            .filter(|a| f.prepass_on && a.size == (f.width, f.height))
            .map(|a| &a.depth_view);
        let lighting = f.lighting;
        let fr = CloudFrame {
            vp,
            cam_rel: f.cam_rel,
            cam: f.ro + f.cam_rel.as_dvec3(),
            drift: glam::Vec2::new(lighting.cloud_offset[0], lighting.cloud_offset[1]) * 2500.0,
            scale: self.tone.enh_last.ground[3],
            steps: if lighting.low_clouds {
                CLOUD_STEPS_LOW
            } else {
                CLOUD_STEPS
            },
            fov: f.camera.fov_deg.to_radians(),
            dt: f.dt,
            still: self.instant_exposure,
            depth,
        };
        probe.clouds.encode(
            &self.device,
            &self.queue,
            enc,
            sky_bg,
            &self.sky.cloud_view_buf,
            &fr,
            timers,
        );
    }
}
