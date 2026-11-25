//! Bounded screen-space puddles, with a small planar capture of the player's vehicle
//! for its complete geometry. Both use at most 518400 pixels (high), 129600 (low).

use super::*;

const MAX_TRACE_PIXELS: f32 = 960.0 * 540.0;
/// The low quality's ray budget: a quarter of the high one's, at a quarter of the picture.
const MAX_TRACE_PIXELS_LOW: f32 = 480.0 * 270.0;

/// What a puddle ray may do (`Lighting::puddles`): how far from the eye a wet pixel is
/// traced (m) and how many screen-space steps a ray takes at most. Low keeps the puddles
/// near the bus.
fn reach(low: bool) -> [f32; 4] {
    if low {
        [40.0, 24.0, 0.0, 0.0]
    } else {
        [100.0, 48.0, 0.0, 0.0]
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VehicleBox {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct VehicleUniform {
    plane: [f32; 4],
    parts: [VehicleBox; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Uniform {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    eye_time: [f32; 4],
    origin_rain: [f32; 4],
    sky: [f32; 4],
    projection_trace: [f32; 4],
    vehicle_plane: [f32; 4],
    vehicle_info: [f32; 4],
    vehicle_parts: [VehicleBox; 4],
    /// x: the farthest wet pixel traced (m), y: the most steps a ray takes (see `reach`).
    quality: [f32; 4],
    /// The last frame's camera, moved to this frame's render origin (`History`).
    prev_view_proj: [[f32; 4]; 4],
    prev_inv_view_proj: [[f32; 4]; 4],
    /// x: 1 when the last frame's reflections may be carried on, y: how much of them is kept,
    /// z: the frame's number (0..1024), which moves the rays' steps on (`fs_trace`).
    history: [f32; 4],
}

/// How much of the last frames' reflections a puddle keeps each frame (`fs_accumulate`). A
/// ray finds or misses a thin pole, a branch or a gap in a tree's leaves anew every frame, as
/// its steps fall on the screen; alone, every frame's reflection was a little different, and
/// on the move they shimmered. Kept over some frames (and held near what this frame's rays see
/// round the pixel), they hold still.
const HISTORY_KEEP: f32 = 0.9;

/// How far the eye may move from one frame to the next for the last frame's reflections to
/// be carried on (m): further is a cut to another view.
const HISTORY_JUMP: f64 = 5.0;

/// The last frame a puddle picture was drawn for: its camera and render origin.
#[derive(Clone, Copy)]
struct History {
    view_proj: Mat4,
    origin: DVec3,
    eye: DVec3,
}

/// puddle_reflection.wgsl and its parts, its groups read from the heap.
pub(super) static PUDDLES: HeapModule = HeapModule::new(shader_source);

pub(super) fn shader_source() -> String {
    [
        crate::shaders::hot_wgsl(include_str!("colour.wgsl"), "colour.wgsl"),
        crate::shaders::hot_wgsl(include_str!("puddle_common.wgsl"), "puddle_common.wgsl"),
        crate::shaders::hot_wgsl(
            include_str!("puddle_reflection.wgsl"),
            "puddle_reflection.wgsl",
        ),
    ]
    .join("\n")
}

pub(super) struct Pipelines {
    params: mx::Buffer,
    trace: mx::RenderPipeline,
    blur_x: mx::RenderPipeline,
    blur_y: mx::RenderPipeline,
    accumulate: mx::RenderPipeline,
    resolve: mx::RenderPipeline,
    glass: [mx::RenderPipeline; 2],
    vehicle: Vec<mx::RenderPipeline>,
    chassis: mx::RenderPipeline,
    vehicle_params: mx::Buffer,
    vehicle_view: mx::BufferView,
}

impl Pipelines {
    pub(super) fn new(device: &mx::Device, scene_shader: &mx::ShaderModule) -> Self {
        let params = device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("puddle reflection params"),
                size: std::mem::size_of::<Uniform>() as u64,
                usage: mx::BufferUsages::UNIFORM
                    | mx::BufferUsages::STORAGE
                    | mx::BufferUsages::COPY_DST,
            })
            .expect("mX: a buffer");
        let shader = device
            .create_shader_module(&mx::ShaderModuleDescriptor {
                label: Some("puddle reflections"),
                source: &PUDDLES.wgsl(),
            })
            .expect("mX: a shader");
        let pipeline = |entry| {
            device
                .create_render_pipeline(&mx::RenderPipelineDescriptor {
                    label: Some(entry),
                    vertex: mx::VertexState {
                        constants: &[],
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                    },
                    primitive: mx::PrimitiveState {
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(mx::FragmentState {
                        constants: &[],
                        module: &shader,
                        entry_point: Some(entry),
                        targets: &[Some(mx::ColorTargetState {
                            format: HDR_FORMAT,
                            blend: None,
                            write_mask: mx::ColorWrites::ALL,
                        })],
                    }),
                })
                .expect("mX: a render pipeline")
        };
        let glass = [false, true].map(|cull| {
            device.create_render_pipeline(&mx::RenderPipelineDescriptor {
                label: Some("puddle glass depth"),
                vertex: mx::VertexState { constants: &[],
                    module: scene_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[mx::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: mx::VertexStepMode::Vertex,
                        attributes: &mx::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                    }],
                },
                primitive: one_sided_primitive(cull),
                depth_stencil: Some(mx::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: mx::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(mx::FragmentState { constants: &[],
                    module: scene_shader,
                    entry_point: Some("fs_puddle_glass_depth"),
                    targets: &[],
                }),
            }).expect("mX: a render pipeline")
        });
        let vehicle_params = device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("puddle vehicle plane"),
                size: std::mem::size_of::<VehicleUniform>() as u64,
                usage: mx::BufferUsages::UNIFORM
                    | mx::BufferUsages::STORAGE
                    | mx::BufferUsages::COPY_DST,
            })
            .expect("mX: a buffer");
        // (read by the scene shader through the heap, at the frame's `ground`: `FrameHeap`)
        let vehicle_view = crate::pipelines::common::heap_view(&vehicle_params);
        let mut vehicle = Vec::new();
        for kind in 0..PIPE_KINDS {
            for cull in [false, true] {
                for _surface in [false, true] {
                    let mut primitive = one_sided_primitive(cull);
                    // Mirroring reverses winding. Keep authored one-sided geometry.
                    primitive.front_face = mx::FrontFace::Ccw;
                    vehicle.push(device.create_render_pipeline(&mx::RenderPipelineDescriptor {
                        label: Some("puddle vehicle capture"),
                        vertex: mx::VertexState { constants: &[],
                            module: scene_shader,
                            entry_point: Some("vs_puddle_vehicle"),
                            buffers: &[mx::VertexBufferLayout {
                                array_stride: std::mem::size_of::<Vertex>() as u64,
                                step_mode: mx::VertexStepMode::Vertex,
                                attributes: &mx::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                            }],
                        },
                        primitive,
                        depth_stencil: Some(mx::DepthStencilState {
                            format: DEPTH_FORMAT,
                            depth_write_enabled: kind != PIPE_BLEND_NO_WRITE,
                            depth_compare: mx::CompareFunction::GreaterEqual,
                            stencil: Default::default(),
                            bias: Default::default(),
                        }),
                        multisample: Default::default(),
                        fragment: Some(mx::FragmentState { constants: &[],
                            module: scene_shader,
                            entry_point: Some("fs_puddle_vehicle"),
                            targets: &[Some(mx::ColorTargetState {
                                format: HDR_FORMAT,
                                blend: (kind == PIPE_BLEND || kind == PIPE_BLEND_NO_WRITE).then_some(mx::BlendState::ALPHA_BLENDING),
                                write_mask: mx::ColorWrites::ALL,
                            })],
                        }),
                    }).expect("mX: a render pipeline"));
                }
            }
        }
        let chassis = device
            .create_render_pipeline(&mx::RenderPipelineDescriptor {
                label: Some("puddle closed chassis"),
                vertex: mx::VertexState {
                    constants: &[],
                    module: scene_shader,
                    entry_point: Some("vs_puddle_chassis"),
                    buffers: &[],
                },
                primitive: mx::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(mx::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: mx::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(mx::FragmentState {
                    constants: &[],
                    module: scene_shader,
                    entry_point: Some("fs_puddle_chassis"),
                    targets: &[Some(mx::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: None,
                        write_mask: mx::ColorWrites::ALL,
                    })],
                }),
            })
            .expect("mX: a render pipeline");
        Self {
            trace: pipeline("fs_trace"),
            blur_x: pipeline("fs_blur_x"),
            blur_y: pipeline("fs_blur_y"),
            accumulate: pipeline("fs_accumulate"),
            resolve: pipeline("fs_resolve"),
            glass,
            vehicle,
            chassis,
            vehicle_params,
            vehicle_view,
            params,
        }
    }
}

pub(super) struct Targets {
    size: (u32, u32),
    /// Made for the low quality (its quarter-size rays).
    low: bool,
    source_depth: mx::Texture,
    hit_depth: mx::Texture,
    hit_depth_view: mx::TextureView,
    vehicle_view: mx::TextureView,
    vehicle_depth: mx::TextureView,
    trace: mx::TextureView,
    blur: mx::TextureView,
    filtered: mx::TextureView,
    /// the reflections kept from frame to frame (`fs_accumulate`): this frame's is
    /// `history[current]`, the last frame's the other
    history: [mx::TextureView; 2],
    current: usize,
    last: Option<History>,
    /// frames drawn with these targets (moves the rays' steps on, see `fs_trace`)
    frame: u32,
    pub(super) view: mx::TextureView,
    trace_bg: HeapGroup,
    blur_x_bg: HeapGroup,
    blur_y_bg: HeapGroup,
    accumulate_bg: [HeapGroup; 2],
    resolve_bg: [HeapGroup; 2],
    pub(super) down_bg: HeapGroup,
    pub(super) tonemap_bg: [HeapGroup; 2],
    pub(super) classic_bg: HeapGroup,
}

/// How many picture pixels one ray stands for, each way (2 at half size; 4 on Low, or on
/// High past the budget): the wet pixel looked for among them (see `probe_taps`).
fn footprint(w: u32, h: u32, low: bool) -> f32 {
    let (tw, th) = trace_size(w, h, low);
    (w as f32 / tw as f32)
        .max(h as f32 / th as f32)
        .ceil()
        .clamp(1.0, 8.0)
}

fn trace_size(w: u32, h: u32, low: bool) -> (u32, u32) {
    let (half, budget) = if low {
        (0.25_f32, MAX_TRACE_PIXELS_LOW)
    } else {
        (0.5, MAX_TRACE_PIXELS)
    };
    let scale = half.min((budget / (w.max(1) as f32 * h.max(1) as f32)).sqrt());
    (
        ((w as f32 * scale).floor() as u32).max(1),
        ((h as f32 * scale).floor() as u32).max(1),
    )
}

impl Targets {
    fn new(r: &Renderer, hdr: &HdrTargets, w: u32, h: u32, low: bool) -> Self {
        let pipelines = r.puddles.as_ref().unwrap();
        let size = trace_size(w, h, low);
        let source_depth = r.targets.ao.as_ref().unwrap().depth_view.texture().clone();
        let hit_depth = r
            .device
            .create_texture(&mx::TextureDescriptor {
                label: Some("puddle hit depth including glass"),
                size: mx::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: mx::TextureUsages::COPY_DST
                    | mx::TextureUsages::COPY_SRC
                    | mx::TextureUsages::RENDER_ATTACHMENT
                    | mx::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .expect("mX: a texture");
        let hit_depth_view = hit_depth
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let target = |label, (width, height)| {
            r.device
                .create_texture(&mx::TextureDescriptor {
                    label: Some(label),
                    size: mx::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format: HDR_FORMAT,
                    usage: mx::TextureUsages::RENDER_ATTACHMENT
                        | mx::TextureUsages::TEXTURE_BINDING
                        | mx::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
                .expect("mX: a texture")
                .create_view(&Default::default())
                .expect("mX: a texture view")
        };
        let trace = target("puddle reflection rays", size);
        let vehicle_view = target("puddle player vehicle", size);
        let vehicle_depth = r
            .device
            .create_texture(&mx::TextureDescriptor {
                label: Some("puddle player vehicle depth"),
                size: mx::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: mx::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .expect("mX: a texture")
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let blur = target("puddle reflection horizontal filter", size);
        let filtered = target("puddle reflection filtered", size);
        let history = [
            target("puddle reflection history", size),
            target("puddle reflection history", size),
        ];
        let view = target("scene with puddle reflections", (w, h));
        let binding = |binding, view| mx::BindGroupEntry {
            binding,
            resource: mx::BindingResource::TextureView(view),
        };
        let group = |ray_view, history_view| {
            HeapGroup::of(
                PUDDLES.layout(),
                0,
                &mx::BindGroupDescriptor {
                    label: Some("puddle reflections"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: pipelines.params.as_entire_binding(),
                        },
                        binding(1, &hdr.view),
                        binding(2, &hdr.mask),
                        binding(3, &r.targets.ao.as_ref().unwrap().depth_view),
                        binding(4, ray_view),
                        mx::BindGroupEntry {
                            binding: 5,
                            resource: mx::BindingResource::Sampler(&r.tone.post_sampler),
                        },
                        binding(6, &r.tone.probe.as_ref().unwrap().view),
                        binding(7, &hit_depth_view),
                        binding(8, &vehicle_view),
                        binding(9, history_view),
                    ],
                },
            )
        };
        // The trace target must never appear in its own bind group, including unused slots.
        let none = &r.black_texture.view;
        let trace_bg = group(none, none);
        let blur_x_bg = group(&trace, none);
        let blur_y_bg = group(&blur, none);
        // (frame i writes history[i] from the filtered rays and the other history)
        let accumulate_bg = [group(&filtered, &history[1]), group(&filtered, &history[0])];
        let resolve_bg = [group(&history[0], none), group(&history[1], none)];
        let post_group = |base, adapt, screen_mask| {
            HeapGroup::of(
                crate::pipelines::post::POST.layout(),
                0,
                &mx::BindGroupDescriptor {
                    label: Some("post with puddle reflections"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: r.tone.post_buf.as_entire_binding(),
                        },
                        binding(1, &view),
                        mx::BindGroupEntry {
                            binding: 2,
                            resource: mx::BindingResource::Sampler(&r.tone.post_sampler),
                        },
                        binding(3, base),
                        binding(4, adapt),
                        binding(5, screen_mask),
                    ],
                },
            )
        };
        let down_bg = post_group(&hdr.mask, &r.white_texture.view, &r.black_texture.view);
        let tonemap_bg = [
            post_group(&hdr.up[0], &r.tone.adapt_views[0], &hdr.mask),
            post_group(&hdr.up[0], &r.tone.adapt_views[1], &hdr.mask),
        ];
        let classic_bg = r.picture_group(&view);
        Self {
            size,
            low,
            source_depth,
            hit_depth,
            hit_depth_view,
            vehicle_view,
            vehicle_depth,
            trace,
            blur,
            filtered,
            history,
            current: 0,
            last: None,
            frame: 0,
            view,
            trace_bg,
            blur_x_bg,
            blur_y_bg,
            accumulate_bg,
            resolve_bg,
            down_bg,
            tonemap_bg,
            classic_bg,
        }
    }
}

impl Renderer {
    pub(super) fn prepare_puddle_reflections(
        &mut self,
        w: u32,
        h: u32,
        camera: &Camera,
        aspect: f32,
        projection: Option<Mat4>,
        cu: &CameraUniform,
        lighting: &Lighting,
        ro: DVec3,
    ) -> bool {
        if self.puddles.is_none()
            || self.targets.ao.is_none()
            || self.tone.probe.is_none()
            || (cu.post[0] > 0.5 && self.sky.sky_state.is_none())
        {
            return false;
        }
        let Some(hdr) = self.targets.hdr_targets.get(&(w, h)) else {
            return false;
        };
        // A previously used size can outlive the AO depth after a resize/another view.
        // Rebind the current receiver texture instead of tracing against an old frame.
        let low = lighting.puddles == 1;
        if hdr.puddles.as_ref().is_none_or(|t| {
            t.source_depth != self.targets.ao.as_ref().unwrap().depth_view.texture() || t.low != low
        }) {
            let targets = Targets::new(self, hdr, w, h, low);
            log::info!(
                "puddle reflections ({}): {}x{} rays, at most {} steps within {} m; resolve {w}x{h}",
                if low { "low" } else { "high" },
                targets.size.0,
                targets.size.1,
                reach(low)[1],
                reach(low)[0]
            );
            self.targets.hdr_targets.get_mut(&(w, h)).unwrap().puddles = Some(targets);
        }
        let vp = Mat4::from_cols_array_2d(&cu.view_proj);
        let proj = projection.unwrap_or_else(|| {
            Mat4::perspective_rh(
                camera.fov_deg.to_radians(),
                aspect,
                camera.depth_far(),
                camera.near,
            )
        });
        let enhanced = cu.post[0] > 0.5;
        let surround = if enhanced {
            let st = self.sky.sky_state.as_ref().unwrap();
            [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
                .into_iter()
                .map(|n| atmosphere::sh_irradiance(&st.sh, n))
                .sum::<Vec3>()
                * (0.25 / (6.0 * std::f32::consts::PI))
        } else {
            // Exactly the sky sheen replaced in shade_vanilla; no Enhanced exposure.
            lighting.secondary * 0.5 + lighting.sun_color * lighting.sun_intensity * 0.35
        };
        let pre = if enhanced {
            self.tone.exposure.unwrap_or(0.0).exp()
        } else {
            1.0
        };
        // (low: no capture of the player's vehicle - the screen-space rays still find it)
        let origins = if low {
            Vec::new()
        } else {
            vehicle_origins(lighting, camera)
        };
        let mut parts = [<VehicleBox as bytemuck::Zeroable>::zeroed(); 4];
        for (part, (o, h, bb)) in parts
            .iter_mut()
            .zip(lighting.inside.iter().chain(lighting.puddle_parts.iter()))
        {
            let origin = (*o - ro).as_vec3();
            let heading = (*h as f32).to_radians();
            *part = VehicleBox {
                a: [origin.x, origin.y, origin.z, heading.sin()],
                b: [heading.cos(), bb[0] * 0.5, bb[1] * 0.5, bb[2] * 0.5],
                c: [bb[3], bb[4], bb[5], 1.0],
            };
        }
        let normal = lighting.puddle_normal.normalize_or_zero();
        let plane_normal = if normal.z > 0.3 { normal } else { Vec3::Z };
        let plane_point = lighting.inside.map_or(DVec3::ZERO, |(o, _, _)| {
            DVec3::new(o.x, o.y, lighting.puddle_ground.unwrap_or(o.z))
        });
        let plane = reflection_plane(plane_point, plane_normal, ro).to_array();
        let mut u = Uniform {
            view_proj: cu.view_proj,
            inv_view_proj: vp.inverse().to_cols_array_2d(),
            eye_time: [cu.cam_pos[0], cu.cam_pos[1], cu.cam_pos[2], cu.post[1]],
            origin_rain: [
                cu.world_origin[0],
                cu.world_origin[1],
                lighting.rain.clamp(0.0, 1.0) * (1.0 - lighting.snow.clamp(0.0, 1.0)),
                PROBE_MIPS as f32,
            ],
            sky: (surround * pre)
                .extend(self.tone.probe.as_ref().unwrap().scale * pre)
                .to_array(),
            projection_trace: [
                proj.z_axis.z,
                proj.w_axis.z,
                omsi_cfg::flags::OMSI_DEBUG_PUDDLES.parse().unwrap_or(0.0),
                omsi_cfg::flags::OMSI_PUDDLE_THICKNESS
                    .parse()
                    .unwrap_or(0.12),
            ],
            vehicle_plane: plane,
            vehicle_info: [
                origins.len() as f32,
                w as f32,
                h as f32,
                if enhanced {
                    1.0
                } else if lighting.classic {
                    0.0
                } else {
                    -1.0
                },
            ],
            vehicle_parts: parts,
            quality: {
                let mut q = reach(low);
                q[2] = footprint(w, h, low);
                q
            },
            prev_view_proj: vp.to_cols_array_2d(),
            prev_inv_view_proj: vp.inverse().to_cols_array_2d(),
            history: [0.0; 4],
        };
        {
            // The last frame's camera, as it sees a point given against this frame's origin.
            let targets = self.targets.hdr_targets.get_mut(&(w, h)).unwrap();
            let targets = targets.puddles.as_mut().unwrap();
            let eye = ro
                + DVec3::new(
                    cu.cam_pos[0] as f64,
                    cu.cam_pos[1] as f64,
                    cu.cam_pos[2] as f64,
                );
            if let Some(last) = targets.last.filter(|l| l.eye.distance(eye) < HISTORY_JUMP) {
                let prev = last.view_proj * Mat4::from_translation((ro - last.origin).as_vec3());
                u.prev_view_proj = prev.to_cols_array_2d();
                u.prev_inv_view_proj = prev.inverse().to_cols_array_2d();
                u.history = [1.0, HISTORY_KEEP, 0.0, 0.0];
            }
            targets.last = Some(History {
                view_proj: vp,
                origin: ro,
                eye,
            });
            targets.current ^= 1;
            targets.frame = (targets.frame + 1) % 1024;
            u.history[2] = targets.frame as f32;
        }
        self.queue
            .write_buffer(
                &self.puddles.as_ref().unwrap().vehicle_params,
                0,
                bytemuck::bytes_of(&VehicleUniform { plane, parts }),
            )
            .expect("mX: a queue write");
        self.queue
            .write_buffer(
                &self.puddles.as_ref().unwrap().params,
                0,
                bytemuck::bytes_of(&u),
            )
            .expect("mX: a queue write");
        true
    }

    pub(super) fn encode_puddle_reflections(
        &self,
        encoder: &mut mx::CommandEncoder,
        w: u32,
        h: u32,
        scene: &Scene,
        batches: &[Batch],
        draw_list: &[u32],
        lighting: &Lighting,
        camera: &Camera,
        queries: Option<&mx::QuerySet>,
        timed: &mut Vec<&'static str>,
    ) {
        let pipelines = self.puddles.as_ref().unwrap();
        let targets = self.targets.hdr_targets[&(w, h)].puddles.as_ref().unwrap();
        // Select only this vehicle's existing draw entries. AI instances can share meshes
        // and batches with it; selecting a mesh alone would reflect all of them at one height.
        // (low: the capture is only cleared, see `prepare_puddle_reflections`)
        let origins = if lighting.puddles == 1 {
            Vec::new()
        } else {
            vehicle_origins(lighting, camera)
        };
        let vehicle_batches = player_vehicle_batches(scene, batches, draw_list, &origins);
        {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("puddle player vehicle"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view: &targets.vehicle_view,
                    resolve_target: None,
                    ops: mx::Operations {
                        load: mx::LoadOp::Clear(mx::Color::TRANSPARENT),
                        store: mx::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(mx::RenderPassDepthStencilAttachment {
                    view: &targets.vehicle_depth,
                    depth_ops: Some(mx::Operations {
                        load: mx::LoadOp::Clear(0.0),
                        store: mx::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: pass_timer(queries, timed, "puddle vehicle"),
                occlusion_query_set: None,
            });
            // (the vehicle plane's uniform at the frame's `ground` place: this pass reads no
            // ground picture)
            let heap = FrameHeap {
                ground: pipelines.vehicle_view.index(),
                ..scene.frame_heap
            };
            if !omsi_cfg::flags::OMSI_NO_PUDDLE_VEHICLE.is_set() {
                if let Some(first) = vehicle_batches.first() {
                    let binding = &scene.materials[first.material as usize].binding;
                    let slots = scene.material_table.immediates(binding);
                    pass.set_immediates(0, bytemuck::cast_slice(&slots));
                    pass.set_immediates(FrameHeap::OFFSET, bytemuck::bytes_of(&heap));
                    pass.set_pipeline(&pipelines.chassis);
                    pass.draw(0..6, 0..origins.len() as u32);
                }
                encode_batches_in(
                    &mut pass,
                    scene,
                    &heap,
                    &vehicle_batches,
                    |_| true,
                    |b| b.pipe as u16,
                    |pipe| &pipelines.vehicle[pipe as usize],
                );
            }
        }
        let copy = |texture| mx::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: mx::Origin3d::ZERO,
            aspect: mx::TextureAspect::DepthOnly,
        };
        encoder.copy_texture_to_texture(
            copy(&targets.source_depth),
            copy(&targets.hit_depth),
            mx::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        if !omsi_cfg::flags::OMSI_NO_PUDDLE_GLASS_DEPTH.is_set()
            && batches
                .iter()
                .any(|b| reflection_glass(&scene.materials[b.material as usize].uniform))
        {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("puddle glass depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(mx::RenderPassDepthStencilAttachment {
                    view: &targets.hit_depth_view,
                    depth_ops: Some(mx::Operations {
                        load: mx::LoadOp::Load,
                        store: mx::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: pass_timer(queries, timed, "puddle glass"),
                occlusion_query_set: None,
            });
            encode_batches_filtered(
                &mut pass,
                scene,
                batches,
                |b| reflection_glass(&scene.materials[b.material as usize].uniform),
                |pipe| &pipelines.glass[(pipe % 4 / 2) as usize],
            );
        }
        post_pass(
            encoder,
            &targets.trace,
            pass_timer(queries, timed, "puddle rays"),
            &pipelines.trace,
            &targets.trace_bg,
        );
        let filter_timer = pass_timer(queries, timed, "puddle filter");
        post_pass(
            encoder,
            &targets.blur,
            filter_timer.as_ref().map(|t| mx::PassTimestampWrites {
                query_set: t.query_set,
                beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                end_of_pass_write_index: None,
            }),
            &pipelines.blur_x,
            &targets.blur_x_bg,
        );
        post_pass(
            encoder,
            &targets.filtered,
            filter_timer.map(|t| mx::PassTimestampWrites {
                query_set: t.query_set,
                beginning_of_pass_write_index: None,
                end_of_pass_write_index: t.end_of_pass_write_index,
            }),
            &pipelines.blur_y,
            &targets.blur_y_bg,
        );
        post_pass(
            encoder,
            &targets.history[targets.current],
            pass_timer(queries, timed, "puddle history"),
            &pipelines.accumulate,
            &targets.accumulate_bg[targets.current],
        );
        post_pass(
            encoder,
            &targets.view,
            pass_timer(queries, timed, "puddle resolve"),
            &pipelines.resolve,
            &targets.resolve_bg[targets.current],
        );
    }
}

fn reflection_plane(point: DVec3, normal: Vec3, origin: DVec3) -> glam::Vec4 {
    normal.extend(normal.dot((point - origin).as_vec3()))
}

fn vehicle_origins(lighting: &Lighting, camera: &Camera) -> Vec<DVec3> {
    if lighting.puddle_ground.is_none()
        || omsi_cfg::flags::OMSI_NO_PUDDLE_VEHICLE.is_set()
        || !lighting
            .inside
            .is_some_and(|(o, _, _)| o.distance(camera.position) < 60.0)
    {
        return Vec::new();
    }
    lighting
        .inside
        .iter()
        .chain(lighting.puddle_parts.iter())
        .take(4)
        .map(|(o, _, _)| *o)
        .collect()
}

fn player_vehicle_batches(
    scene: &Scene,
    batches: &[Batch],
    list: &[u32],
    origins: &[DVec3],
) -> Vec<Batch> {
    // (no vehicle to reflect - dry ground under it, or the camera away from it: nothing to
    // look for among the whole scene's instances and every entry the frame draws)
    if origins.is_empty() {
        return Vec::new();
    }
    let entries: std::collections::HashSet<u32> = scene
        .instances
        .iter()
        .filter(|i| {
            i.visible
                && i.roof.is_some()
                && !i.blob
                && !i.surface
                && origins.iter().any(|o| i.origin.distance_squared(*o) < 0.01)
        })
        .flat_map(|i| i.base..i.base + i.materials.len() as u32)
        .collect();
    selected_entry_batches(batches, list, &entries)
}

fn selected_entry_batches(
    batches: &[Batch],
    list: &[u32],
    entries: &std::collections::HashSet<u32>,
) -> Vec<Batch> {
    let mut out = Vec::new();
    // (the few vehicles' entries lie close together: the rest of the frame's thousands are
    // told apart by two comparisons, not looked up)
    let lo = entries.iter().copied().min().unwrap_or(u32::MAX);
    let hi = entries.iter().copied().max().unwrap_or(0);
    for b in batches {
        let mut start = None;
        for index in b.instances.clone() {
            let e = list[index as usize];
            if e >= lo && e <= hi && entries.contains(&e) {
                start.get_or_insert(index);
            } else if let Some(first) = start.take() {
                out.push(Batch {
                    pipe: b.pipe,
                    mesh: b.mesh,
                    first: b.first,
                    count: b.count,
                    material: b.material,
                    instances: first..index,
                    lean: b.lean,
                });
            }
        }
        if let Some(first) = start {
            out.push(Batch {
                pipe: b.pipe,
                mesh: b.mesh,
                first: b.first,
                count: b.count,
                material: b.material,
                instances: first..b.instances.end,
                lean: b.lean,
            });
        }
    }
    out
}

// Match the enhanced shader's glass classification, excluding rain films, decals and
// unlit screens. Their colour is already composited over the actual window surface.
fn reflection_glass(u: &MaterialUniform) -> bool {
    u.extra[0] < 0.5
        && u.params[0] > 1.5
        && u.params[1] < 0.5
        && (u.bump[2] > 0.5 || u.emissive[3] > 0.5)
        && u.bump[3] < 0.5
        && u.emissive[3] < 1.5
        && (u.params2[1] > 0.0 || u.params[2] > 0.5 || u.emissive[3] > 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_preserves_clearance_at_any_height_grade_and_floating_origin() {
        for height in [-125.0, 0.09, 12.0, 501.0] {
            for normal in [Vec3::Z, Vec3::new(0.1, -0.04, 1.0).normalize()] {
                let ground = DVec3::new(-2165.0, 736.0, height);
                let origin = DVec3::new(-2200.0, 700.0, (height / 100.0).floor() * 100.0);
                let plane = reflection_plane(ground, normal, origin);
                let point = (ground - origin).as_vec3() + normal * 0.4;
                let reflected = point - 2.0 * normal * (normal.dot(point) - plane.w);
                assert!((normal.dot(point) - plane.w - 0.4).abs() < 1e-4);
                assert!((normal.dot(reflected) - plane.w + 0.4).abs() < 1e-4);
                assert!(((point + reflected) * 0.5 - (ground - origin).as_vec3()).length() < 1e-4);
            }
        }
    }

    #[test]
    #[ignore = "requires a graphics adapter; checks transparent window reflection depth"]
    fn glass_hit_depth_preserves_receiver_depth_across_resizes() {
        let mut r = test_renderer(4);
        let mut scene = r.new_scene();
        let back = test_material(
            &r,
            &mut scene,
            [1.0; 4],
            false,
            1.0,
            MaterialExtra::default(),
        );
        let pane = test_material(
            &r,
            &mut scene,
            [0.5, 0.5, 0.5, 0.15],
            false,
            0.0,
            MaterialExtra {
                glass: true,
                ..Default::default()
            },
        );
        for (y, half, material) in [(10.0, 10.0, back), (5.0, 2.0, pane)] {
            test_quad(
                &r,
                &mut scene,
                [
                    [-half, y, -half],
                    [half, y, -half],
                    [half, y, half],
                    [-half, y, half],
                ],
                -Vec3::Y,
                material,
            );
        }
        let camera = Camera {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 90.0,
            near: 0.1,
            far: 100.0,
        };
        let mut lighting = Lighting {
            enhanced: true,
            wetness: 1.0,
            shadows: false,
            ..Default::default()
        };
        fn depth_at_centre(r: &Renderer, texture: &mx::Texture, size: u32) -> f32 {
            let bytes = r
                .read_texture(texture, mx::TextureAspect::DepthOnly)
                .unwrap();
            let offset = ((size / 2 * size + size / 2) * 4) as usize;
            f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
        }
        for inside in [false, true] {
            lighting.inside =
                inside.then_some((DVec3::ZERO, 0.0, [12.0, 12.0, 12.0, 0.0, 0.0, 0.0]));
            for size in [64, 48, 64] {
                r.render_to_image(&mut scene, size, size, &camera, &lighting)
                    .unwrap();
                let targets = r.targets.hdr_targets[&(size, size)]
                    .puddles
                    .as_ref()
                    .unwrap();
                assert_eq!(
                    targets.source_depth,
                    r.targets.ao.as_ref().unwrap().depth_view.texture()
                );
                // Read both textures below through an immutable renderer borrow.
                let receiver = depth_at_centre(&r, &targets.source_depth, size);
                let hit = depth_at_centre(&r, &targets.hit_depth, size);
                let reversed = |z: f32| {
                    let (n, f) = (camera.near, camera.depth_far());
                    n * (f - z) / (z * (f - n))
                };
                assert!(
                    (receiver - reversed(10.0)).abs() < 1e-5,
                    "opaque receiver: {receiver}"
                );
                let expected_hit = if inside { receiver } else { reversed(5.0) };
                assert!((hit - expected_hit).abs() < 1e-5, "glass hit: {hit}");
            }
        }
    }

    fn test_renderer(msaa: u32) -> Renderer {
        let instance = mx::Instance::new();
        pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(mx::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa,
                ssao: false,
                fxaa: false,
                shadow_size: 1024,
                render_scale: 1.0,
                ..Default::default()
            },
        ))
        .expect("test renderer")
    }

    fn test_material(
        r: &Renderer,
        scene: &mut Scene,
        colour: [f32; 4],
        unlit: bool,
        moisture: f32,
        extra: MaterialExtra,
    ) -> MaterialId {
        let alpha = if extra.glass || extra.rain_film {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };
        r.add_material_inner(
            scene, None, alpha, colour, unlit, None, None, None, None, None, [0.0; 3], moisture,
            extra,
        )
    }

    fn test_quad(
        r: &Renderer,
        scene: &mut Scene,
        positions: [[f32; 3]; 4],
        normal: Vec3,
        material: MaterialId,
    ) -> usize {
        let mesh = r.add_mesh(
            scene,
            &MeshData {
                positions: positions.map(Vec3::from).to_vec(),
                normals: vec![normal; 4],
                uvs: vec![
                    glam::Vec2::ZERO,
                    glam::Vec2::X,
                    glam::Vec2::ONE,
                    glam::Vec2::Y,
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                ranges: vec![(0, 6, 0)],
                one_sided: false,
            },
        );
        r.add_instance(scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material])
    }

    fn refraction_source(r: &Renderer, enhanced: bool) -> Vec<f32> {
        let texture = r.targets.glass_picture.as_ref().unwrap().texture();
        let bytes = r.read_texture(&texture, mx::TextureAspect::All).unwrap();
        if texture.format() != HDR_FORMAT {
            return bytes.into_iter().map(|v| v as f32 / 255.0).collect();
        }
        let pre = if enhanced {
            r.tone.exposure.unwrap().exp()
        } else {
            1.0
        };
        bytes
            .chunks_exact(2)
            .enumerate()
            .map(|(i, b)| {
                half_to_f32(u16::from_le_bytes(b.try_into().unwrap()))
                    / if i % 4 == 3 { 1.0 } else { pre }
            })
            .collect()
    }

    fn image_difference(a: &[u8], b: &[u8]) -> u64 {
        a.iter().zip(b).map(|(a, b)| a.abs_diff(*b) as u64).sum()
    }

    #[test]
    #[ignore = "requires a graphics adapter; renders wet roads through bus glass"]
    fn wet_scene_reflections_survive_bus_glass_in_every_graphics_mode() {
        for msaa in [1, 4] {
            let mut r = test_renderer(msaa);
            let mut scene = r.new_scene();
            let road = test_material(
                &r,
                &mut scene,
                [0.25, 0.25, 0.25, 1.0],
                false,
                1.0,
                MaterialExtra::default(),
            );
            let red = test_material(
                &r,
                &mut scene,
                [1.0, 0.01, 0.01, 1.0],
                true,
                0.0,
                MaterialExtra::default(),
            );
            let pane = test_material(
                &r,
                &mut scene,
                [0.25, 0.3, 0.35, 0.15],
                false,
                0.0,
                MaterialExtra {
                    glass: true,
                    ..Default::default()
                },
            );
            let film = test_material(
                &r,
                &mut scene,
                [1.0; 4],
                false,
                0.0,
                MaterialExtra {
                    rain_film: true,
                    no_z_write: true,
                    ..Default::default()
                },
            );
            test_quad(
                &r,
                &mut scene,
                [
                    [-20.0, 0.0, 0.0],
                    [20.0, 0.0, 0.0],
                    [20.0, 30.0, 0.0],
                    [-20.0, 30.0, 0.0],
                ],
                Vec3::Z,
                road,
            );
            test_quad(
                &r,
                &mut scene,
                [
                    [-3.0, 10.0, 0.0],
                    [3.0, 10.0, 0.0],
                    [3.0, 10.0, 5.0],
                    [-3.0, 10.0, 5.0],
                ],
                -Vec3::Y,
                red,
            );
            let window = test_quad(
                &r,
                &mut scene,
                [
                    [-3.0, 1.0, -1.0],
                    [3.0, 1.0, -1.0],
                    [3.0, 1.0, 6.0],
                    [-3.0, 1.0, 6.0],
                ],
                -Vec3::Y,
                pane,
            );
            let rain = test_quad(
                &r,
                &mut scene,
                [
                    [-3.0, 0.99, -1.0],
                    [3.0, 0.99, -1.0],
                    [3.0, 0.99, 6.0],
                    [-3.0, 0.99, 6.0],
                ],
                -Vec3::Y,
                film,
            );
            let camera = Camera {
                position: DVec3::new(0.0, 0.0, 2.0),
                yaw: 0.0,
                pitch: -10.0,
                roll: 0.0,
                fov_deg: 70.0,
                near: 0.1,
                far: 100.0,
            };
            for (enhanced, classic) in [(false, true), (false, false), (true, false)] {
                let lighting = Lighting {
                    enhanced,
                    classic,
                    wetness: 1.0,
                    rain: 0.8,
                    shadows: false,
                    inside: Some((camera.position, 0.0, [4.0, 4.0, 4.0, 0.0, 0.0, 0.0])),
                    ..Default::default()
                };
                let mut reflection = Vec::new();
                for glass in 0..3 {
                    r.set_params(&mut scene, window, &[], glass > 0, &[]);
                    r.set_params(&mut scene, rain, &[], glass == 2, &[]);
                    r.options.reflections = false;
                    let without = r
                        .render_to_image(&mut scene, 128, 128, &camera, &lighting)
                        .unwrap();
                    r.options.reflections = true;
                    let with = r
                        .render_to_image(&mut scene, 128, 128, &camera, &lighting)
                        .unwrap();
                    // Only the road: the red wall is above this region.
                    let red_change: i64 = with[128 * 70 * 4..]
                        .chunks_exact(4)
                        .zip(without[128 * 70 * 4..].chunks_exact(4))
                        .map(|(a, b)| (a[0] as i64 - a[1] as i64) - (b[0] as i64 - b[1] as i64))
                        .sum();
                    assert!(
                        red_change > 1000,
                        "missing reflection: {red_change}; MSAA={msaa}, enhanced={enhanced}, classic={classic}, glass={glass}"
                    );
                    reflection.push(red_change);
                }
                assert!(
                    reflection[1..].iter().all(|v| *v > reflection[0] / 4),
                    "glass hid the reflection: {reflection:?}"
                );

                // The road stays still while drops move. Settle the sky after changing weather.
                let steady = Lighting {
                    rain: 0.0,
                    ..lighting
                };
                r.set_params(&mut scene, window, &[], true, &[]);
                r.set_params(&mut scene, rain, &[0.0], true, &[]);
                for _ in 0..3 {
                    r.render_to_image(&mut scene, 128, 128, &camera, &steady)
                        .unwrap();
                }
                let clean = r
                    .render_to_image(&mut scene, 128, 128, &camera, &steady)
                    .unwrap();
                let behind = refraction_source(&r, enhanced);
                r.set_params(&mut scene, rain, &[1.0], true, &[]);
                for _ in 0..4 {
                    let wet = r
                        .render_to_image(&mut scene, 128, 128, &camera, &steady)
                        .unwrap();
                    assert!(
                        image_difference(&wet, &clean) > 300,
                        "rain film was not exercised"
                    );
                    let current = refraction_source(&r, enhanced);
                    let drift = current
                        .iter()
                        .zip(&behind)
                        .map(|(a, b)| (a - b).abs())
                        .sum::<f32>()
                        / current.len() as f32;
                    assert!(
                        drift < 0.002,
                        "rain/wiper feedback: {drift}; MSAA={msaa}, enhanced={enhanced}, classic={classic}"
                    );
                }
                r.set_params(&mut scene, rain, &[0.0], true, &[]);
                let wiped = r
                    .render_to_image(&mut scene, 128, 128, &camera, &steady)
                    .unwrap();
                assert!(
                    image_difference(&wiped, &clean) < 128 * 128,
                    "wiper left old film colour"
                );

                if classic {
                    r.set_params(&mut scene, window, &[], false, &[]);
                    let fog = Lighting {
                        fog_color: Vec3::splat(0.5),
                        fog_density: 20.0,
                        inside: None,
                        ..steady
                    };
                    let picture = r
                        .render_to_image(&mut scene, 128, 128, &camera, &fog)
                        .unwrap();
                    let pixel = &picture[(80 * 128 + 64) * 4..][..3];
                    assert!(
                        pixel.iter().all(|v| (126..=129).contains(v)),
                        "double-encoded Vanilla fog: {pixel:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn reflection_depth_uses_panes_not_rain_decals_or_terrain() {
        let mut pane = <MaterialUniform as bytemuck::Zeroable>::zeroed();
        pane.params[0] = 2.0;
        pane.bump[2] = 1.0;
        pane.emissive[3] = 1.0;
        assert!(reflection_glass(&pane));
        for (field, value) in [(0, 2.0), (1, 1.0), (2, 1.0), (3, 1.0)] {
            let mut other = pane;
            match field {
                0 => other.emissive[3] = value,
                1 => other.bump[3] = value,
                2 => other.extra[0] = value,
                _ => other.params[1] = value,
            }
            assert!(!reflection_glass(&other));
        }
        pane.emissive[3] = 0.0;
        assert!(!reflection_glass(&pane));
        pane.params2[1] = 0.4;
        assert!(reflection_glass(&pane));
        pane.bump[2] = 0.0;
        assert!(!reflection_glass(&pane));
    }

    #[test]
    fn ray_budget_stays_bounded_at_large_and_odd_sizes() {
        for (w, h) in [
            (1, 1),
            (1279, 719),
            (1920, 1080),
            (3840, 2160),
            (7680, 4320),
            (2160, 3840),
        ] {
            let (tw, th) = trace_size(w, h, false);
            assert!(tw >= 1 && th >= 1 && tw <= w && th <= h);
            assert!(tw * th <= MAX_TRACE_PIXELS as u32);
            if w >= 2 && h >= 2 {
                assert!(tw <= w / 2 && th <= h / 2);
            }
            // the low quality: at most a quarter of the picture each way and of the budget
            let (lw, lh) = trace_size(w, h, true);
            assert!(lw >= 1 && lh >= 1 && lw <= tw && lh <= th);
            assert!(lw * lh <= MAX_TRACE_PIXELS_LOW as u32);
            if w >= 4 && h >= 4 {
                assert!(lw <= w / 4 && lh <= h / 4);
            }
        }
    }

    /// Where `fs_trace` looks for a wet pixel around a ray's centre, one way (the shader's
    /// sum, in i32): at most 3 looks, spread over the `span` pixels the ray stands for.
    fn probe_taps(span: i32) -> Vec<i32> {
        let span = span.clamp(1, 8);
        let taps = span.min(3);
        (0..taps)
            .map(|x| ((2 * x + 1) * span) / (2 * taps) - span / 2)
            .collect()
    }

    /// The looks for a wet pixel: the same 2x2 at half size as before, at most 3x3 spread
    /// over the pixels otherwise - so the looks per frame do not grow with the picture, and
    /// Low stays below High.
    #[test]
    fn the_wet_probe_is_bounded_and_spread() {
        assert_eq!(probe_taps(2), [-1, 0]);
        assert_eq!(probe_taps(4), [-2, 0, 1]);
        assert_eq!(probe_taps(8), [-3, 0, 2]);
        for span in 1..=8 {
            let t = probe_taps(span);
            assert!(
                t.len() <= 3 && t.iter().all(|&o| o >= -(span / 2) && o < span - span / 2),
                "{span}: {t:?}"
            );
        }
        for (w, h) in [(1920, 1080), (2560, 1440), (3840, 2160)] {
            let looks = |low: bool| {
                let (tw, th) = trace_size(w, h, low);
                let n = probe_taps(footprint(w, h, low) as i32).len() as u64;
                tw as u64 * th as u64 * n * n
            };
            assert!(
                looks(true) < looks(false),
                "{w}x{h}: low {} high {}",
                looks(true),
                looks(false)
            );
            assert!(looks(false) <= MAX_TRACE_PIXELS as u64 * 9);
        }
    }

    /// The pixels a ray stands for: 2 each way at half size, 4 on Low - and more past the
    /// budget.
    #[test]
    fn a_ray_looks_at_all_its_pixels() {
        assert_eq!(footprint(1920, 1080, false), 2.0);
        assert_eq!(footprint(1920, 1080, true), 4.0);
        assert_eq!(footprint(3840, 2160, false), 4.0);
        assert_eq!(footprint(3840, 2160, true), 8.0);
        for (w, h) in [(1, 1), (1279, 719), (2160, 3840), (7680, 4320)] {
            for low in [false, true] {
                let (tw, th) = trace_size(w, h, low);
                let f = footprint(w, h, low);
                assert!(f >= 1.0 && f <= 8.0);
                // (the pixels looked at cover the picture, up to the 8x8 cap)
                assert!(
                    (tw as f32 * f >= w as f32 && th as f32 * f >= h as f32) || f == 8.0,
                    "{w}x{h} {low}"
                );
            }
        }
    }
}
#[test]
fn vehicle_capture_splits_shared_batches_without_drawing_ai_entries() {
    let batch = Batch {
        pipe: 7,
        mesh: 3,
        first: 12,
        count: 24,
        material: 4,
        instances: 0..6,
        lean: Lean::No,
    };
    let own = [11, 12, 13, 14].into_iter().collect();
    let selected = selected_entry_batches(&[batch], &[11, 12, 32, 13, 45, 14], &own);
    assert_eq!(
        selected
            .iter()
            .map(|b| b.instances.clone())
            .collect::<Vec<_>>(),
        vec![0..2, 3..4, 5..6]
    );
    assert!(
        selected
            .iter()
            .all(|b| (b.pipe, b.mesh, b.first, b.count, b.material) == (7, 3, 12, 24, 4))
    );
}
