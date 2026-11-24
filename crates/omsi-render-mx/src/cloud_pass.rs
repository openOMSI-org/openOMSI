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
const TRACE_FORMAT: mx::TextureFormat = mx::TextureFormat::Rgba16Float;
const TRACE_DEPTH_FORMAT: mx::TextureFormat = mx::TextureFormat::R32Float;

/// The targets for one picture size.
pub(crate) struct CloudTargets {
    size: (u32, u32),
    trace: mx::TextureView,
    trace_depth: mx::TextureView,
    display_tex: mx::Texture,
    display: mx::TextureView,
    hist_tex: mx::Texture,
    /// The resolve's bind groups, per round (`CloudPass::rounds`).
    resolve_bgs: Vec<HeapGroup>,
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
    trace_pipeline: mx::RenderPipeline,
    resolve_pipeline: mx::RenderPipeline,
    sampler: mx::Sampler,
    /// Per round (`probe.p` in the shaders): the window's frame (0), then a picture's own
    /// `STILL_ROUNDS`: x the round, y the history's share (< 0: the usual).
    rounds: Vec<mx::Buffer>,
    /// The trace's bind groups (the probe's layout), per round.
    trace_bgs: Vec<HeapGroup>,
    /// the sky module's places in the heap (`SkyBase::layout`)
    layout: crate::shaders::HeapLayout,
    /// A depth texel for a frame without the prepass's depth (nothing is skipped).
    no_depth: mx::TextureView,
    /// What the camera group reads before there are clouds: no cloud anywhere.
    placeholder: mx::TextureView,
    targets: Option<CloudTargets>,
    frame: u32,
    prev: Option<Prev>,
}

impl CloudPass {
    /// `trace_group` makes a bind group of the probe's kind (group 0 of the trace) reading
    /// the round buffer it is given.
    pub(crate) fn new(
        device: &mx::Device,
        queue: &mx::Queue,
        shader: &mx::ShaderModule,
        layout: &crate::shaders::HeapLayout,
        sampler: &mx::Sampler,
        trace_group: &dyn Fn(&mx::Buffer) -> HeapGroup,
    ) -> CloudPass {
        let trace_pipeline = no_vertex_pipeline(
            "cloud trace",
            shader,
            "fs_enhanced_cloud_trace",
            &[target(TRACE_FORMAT, None), target(TRACE_DEPTH_FORMAT, None)],
        )
        .vs("vs_probe")
        .create(device);
        let resolve_pipeline = no_vertex_pipeline(
            "cloud resolve",
            shader,
            "fs_enhanced_cloud_resolve",
            &[target(TRACE_FORMAT, None)],
        )
        .vs("vs_probe")
        .create(device);
        let rounds: Vec<mx::Buffer> = std::iter::once([0.0, -1.0, 0.0, 0.0])
            .chain((0..STILL_ROUNDS).map(|k| [k as f32, k as f32 / (k as f32 + 1.0), 0.0, 0.0]))
            .map(|p: [f32; 4]| {
                buffer_init(
                    device,
                    queue,
                    Some("cloud round"),
                    bytemuck::cast_slice(&p),
                    mx::BufferUsages::UNIFORM | mx::BufferUsages::STORAGE,
                )
            })
            .collect();
        let trace_bgs = rounds.iter().map(trace_group).collect();
        let texel = |label: &str, format: mx::TextureFormat, usage: mx::TextureUsages| {
            device
                .create_texture(&mx::TextureDescriptor {
                    label: Some(label),
                    size: mx::Extent3d {
                        width: 1,
                        height: 1,
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
        let no_depth = texel(
            "no cloud depth",
            DEPTH_FORMAT,
            mx::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default())
        .expect("mX: a texture view");
        let placeholder_tex = texel(
            "no clouds",
            TRACE_FORMAT,
            mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
        );
        // (no light, everything let through: half floats 0, 0, 0, 1)
        let clear: [u16; 4] = [0, 0, 0, 0x3c00];
        queue
            .write_texture(
                mx::TexelCopyTextureInfo {
                    texture: &placeholder_tex,
                    mip_level: 0,
                    origin: mx::Origin3d::ZERO,
                    aspect: mx::TextureAspect::All,
                },
                bytemuck::cast_slice(&clear),
                mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(8),
                    rows_per_image: Some(1),
                },
                mx::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            )
            .expect("mX: a queue write");
        CloudPass {
            trace_pipeline,
            resolve_pipeline,
            sampler: sampler.clone(),
            rounds,
            trace_bgs,
            layout: layout.clone(),
            no_depth,
            placeholder: placeholder_tex
                .create_view(&Default::default())
                .expect("mX: a texture view"),
            targets: None,
            frame: 0,
            prev: None,
        }
    }

    /// The accumulated clouds the dome reads (the camera group's binding 17).
    pub(crate) fn display_view(&self) -> &mx::TextureView {
        self.targets
            .as_ref()
            .map_or(&self.placeholder, |t| &t.display)
    }

    /// The targets for a picture of `size`; true when they were made anew (the camera group
    /// must be made again to read them).
    pub(crate) fn ensure(&mut self, device: &mx::Device, size: (u32, u32)) -> bool {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return false;
        }
        let make =
            |label: &str, w: u32, h: u32, format: mx::TextureFormat, usage: mx::TextureUsages| {
                device
                    .create_texture(&mx::TextureDescriptor {
                        label: Some(label),
                        size: mx::Extent3d {
                            width: w.max(1),
                            height: h.max(1),
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
        use mx::TextureUsages as U;
        let (tw, th) = (size.0.div_ceil(TRACE_DIV), size.1.div_ceil(TRACE_DIV));
        let (rw, rh) = (2 * tw, 2 * th);
        let trace = make(
            "cloud trace",
            tw,
            th,
            TRACE_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        )
        .create_view(&Default::default())
        .expect("mX: a texture view");
        let trace_depth = make(
            "cloud trace depth",
            tw,
            th,
            TRACE_DEPTH_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        )
        .create_view(&Default::default())
        .expect("mX: a texture view");
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
        let hist = hist_tex
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let resolve_bgs = self
            .rounds
            .iter()
            .map(|round| {
                HeapGroup::of(
                    &self.layout,
                    0,
                    &mx::BindGroupDescriptor {
                        label: Some("cloud resolve"),
                        entries: &[
                            mx::BindGroupEntry {
                                binding: 13,
                                resource: mx::BindingResource::Sampler(&self.sampler),
                            },
                            mx::BindGroupEntry {
                                binding: 15,
                                resource: round.as_entire_binding(),
                            },
                            mx::BindGroupEntry {
                                binding: 23,
                                resource: mx::BindingResource::TextureView(&trace),
                            },
                            mx::BindGroupEntry {
                                binding: 24,
                                resource: mx::BindingResource::TextureView(&trace_depth),
                            },
                            mx::BindGroupEntry {
                                binding: 25,
                                resource: mx::BindingResource::TextureView(&hist),
                            },
                        ],
                    },
                )
            })
            .collect();
        self.targets = Some(CloudTargets {
            size,
            trace,
            trace_depth,
            display: display_tex
                .create_view(&Default::default())
                .expect("mX: a texture view"),
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
    pub depth: Option<&'a mx::TextureView>,
}

impl CloudPass {
    /// Trace and accumulate this frame's clouds (writes the view uniform the dome reads).
    pub(crate) fn encode(
        &mut self,
        _device: &mx::Device,
        queue: &mx::Queue,
        enc: &mut mx::CommandEncoder,
        sky_bg: &HeapGroup,
        view_buf: &mx::Buffer,
        fr: &CloudFrame,
        timers: &mut PassTimers,
    ) {
        let Some(t) = self.targets.as_ref() else {
            queue
                .write_buffer(view_buf, 0, bytemuck::bytes_of(&CloudView::default()))
                .expect("mX: a queue write");
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
        queue
            .write_buffer(view_buf, 0, bytemuck::bytes_of(&u))
            .expect("mX: a queue write");
        let depth_bg = HeapGroup::of(
            &self.layout,
            2,
            &mx::BindGroupDescriptor {
                label: Some("cloud trace depth"),
                entries: &[mx::BindGroupEntry {
                    binding: 0,
                    resource: mx::BindingResource::TextureView(fr.depth.unwrap_or(&self.no_depth)),
                }],
            },
        );
        let rounds: Vec<usize> = if fr.still {
            (1..=STILL_ROUNDS as usize).collect()
        } else {
            vec![0]
        };
        for (n, round) in rounds.into_iter().enumerate() {
            {
                let mut pass = enc.begin_render_pass(&mx::RenderPassDescriptor {
                    label: Some("cloud trace"),
                    color_attachments: &[
                        attach(
                            &t.trace,
                            mx::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            },
                        ),
                        attach(
                            &t.trace_depth,
                            mx::Color {
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
                });
                pass.set_pipeline(&self.trace_pipeline);
                self.trace_bgs[round].set(&mut pass);
                sky_bg.set(&mut pass);
                depth_bg.set(&mut pass);
                pass.draw(0..3, 0..1);
            }
            {
                let mut pass = enc.begin_render_pass(&mx::RenderPassDescriptor {
                    label: Some("cloud resolve"),
                    color_attachments: &[attach(&t.display, mx::Color::BLACK)],
                    depth_stencil_attachment: None,
                    timestamp_writes: if n == 0 {
                        pass_timer(timers.set.as_ref(), &mut timers.timed, "cloud resolve")
                    } else {
                        None
                    },
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.resolve_pipeline);
                t.resolve_bgs[round].set(&mut pass);
                sky_bg.set(&mut pass);
                pass.draw(0..3, 0..1);
            }
            enc.copy_texture_to_texture(
                t.display_tex.as_image_copy(),
                t.hist_tex.as_image_copy(),
                mx::Extent3d {
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

fn attach(view: &mx::TextureView, clear: mx::Color) -> Option<mx::RenderPassColorAttachment<'_>> {
    Some(mx::RenderPassColorAttachment {
        view,
        resolve_target: None,
        ops: mx::Operations {
            load: mx::LoadOp::Clear(clear),
            store: mx::StoreOp::Store,
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
        enc: &mut mx::CommandEncoder,
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
            self.queue
                .write_buffer(
                    &self.sky.cloud_view_buf,
                    0,
                    bytemuck::bytes_of(&CloudView::default()),
                )
                .expect("mX: a queue write");
            return;
        }
        let Some(sky_bg) = scene.sky_bind_group.as_ref() else {
            self.queue
                .write_buffer(
                    &self.sky.cloud_view_buf,
                    0,
                    bytemuck::bytes_of(&CloudView::default()),
                )
                .expect("mX: a queue write");
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
