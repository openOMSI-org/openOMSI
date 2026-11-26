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
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    trace: wgpu::RenderPipeline,
    blur_x: wgpu::RenderPipeline,
    blur_y: wgpu::RenderPipeline,
    accumulate: wgpu::RenderPipeline,
    resolve: wgpu::RenderPipeline,
    glass: [wgpu::RenderPipeline; 2],
    vehicle: Vec<wgpu::RenderPipeline>,
    chassis: wgpu::RenderPipeline,
    vehicle_params: wgpu::Buffer,
    vehicle_bg: wgpu::BindGroup,
}

impl Pipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        scene_shader: &wgpu::ShaderModule,
        camera_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let texture = |binding, sample_type, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("puddle reflections"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Uniform>() as u64
                        ),
                    },
                    count: None,
                },
                texture(1, float, wgpu::TextureViewDimension::D2),
                texture(2, float, wgpu::TextureViewDimension::D2),
                texture(
                    3,
                    wgpu::TextureSampleType::Depth,
                    wgpu::TextureViewDimension::D2,
                ),
                texture(4, float, wgpu::TextureViewDimension::D2),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(6, float, wgpu::TextureViewDimension::Cube),
                texture(
                    7,
                    wgpu::TextureSampleType::Depth,
                    wgpu::TextureViewDimension::D2,
                ),
                texture(8, float, wgpu::TextureViewDimension::D2),
                texture(9, float, wgpu::TextureViewDimension::D2),
            ],
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("puddle reflection params"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("puddle reflections"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("puddle reflections"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: crate::pipeline_cache::get(device).as_ref(),
            })
        };
        let glass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("puddle glass depth"),
            bind_group_layouts: &[Some(camera_layout), Some(material_layout)],
            immediate_size: 0,
        });
        let glass = [false, true].map(|cull| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("puddle glass depth"),
                layout: Some(&glass_layout),
                vertex: wgpu::VertexState {
                    module: scene_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                    }],
                    compilation_options: Default::default(),
                },
                primitive: one_sided_primitive(cull),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: scene_shader,
                    entry_point: Some("fs_puddle_glass_depth"),
                    targets: &[],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: crate::pipeline_cache::get(device).as_ref(),
            })
        });
        let vehicle_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("puddle vehicle plane"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<VehicleUniform>() as u64
                    ),
                },
                count: None,
            }],
        });
        let vehicle_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("puddle vehicle plane"),
            size: std::mem::size_of::<VehicleUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vehicle_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("puddle vehicle plane"),
            layout: &vehicle_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: vehicle_params.as_entire_binding(),
            }],
        });
        let capture_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("puddle vehicle capture"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(material_layout),
                Some(&vehicle_layout),
            ],
            immediate_size: 0,
        });
        let mut vehicle = Vec::new();
        for kind in 0..PIPE_KINDS {
            for cull in [false, true] {
                for _surface in [false, true] {
                    let mut primitive = one_sided_primitive(cull);
                    // Mirroring reverses winding. Keep authored one-sided geometry.
                    primitive.front_face = wgpu::FrontFace::Ccw;
                    vehicle.push(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("puddle vehicle capture"),
                        layout: Some(&capture_layout),
                        vertex: wgpu::VertexState {
                            module: scene_shader,
                            entry_point: Some("vs_puddle_vehicle"),
                            buffers: &[wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<Vertex>() as u64,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                            }],
                            compilation_options: Default::default(),
                        },
                        primitive,
                        depth_stencil: Some(wgpu::DepthStencilState {
                            format: DEPTH_FORMAT,
                            depth_write_enabled: Some(kind != PIPE_BLEND_NO_WRITE),
                            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                            stencil: Default::default(),
                            bias: Default::default(),
                        }),
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState {
                            module: scene_shader,
                            entry_point: Some("fs_puddle_vehicle"),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: HDR_FORMAT,
                                blend: (kind == PIPE_BLEND || kind == PIPE_BLEND_NO_WRITE).then_some(wgpu::BlendState::ALPHA_BLENDING),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                            compilation_options: Default::default(),
                        }),
                        multiview_mask: None,
                        cache: crate::pipeline_cache::get(device).as_ref(),
                    }));
                }
            }
        }
        let chassis = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("puddle closed chassis"),
            layout: Some(&capture_layout),
            vertex: wgpu::VertexState {
                module: scene_shader,
                entry_point: Some("vs_puddle_chassis"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: scene_shader,
                entry_point: Some("fs_puddle_chassis"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: crate::pipeline_cache::get(device).as_ref(),
        });
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
            vehicle_bg,
            layout,
            params,
        }
    }
}

pub(super) struct Targets {
    size: (u32, u32),
    /// Made for the low quality (its quarter-size rays).
    low: bool,
    source_depth: wgpu::Texture,
    hit_depth: wgpu::Texture,
    hit_depth_view: wgpu::TextureView,
    vehicle_view: wgpu::TextureView,
    vehicle_depth: wgpu::TextureView,
    trace: wgpu::TextureView,
    blur: wgpu::TextureView,
    filtered: wgpu::TextureView,
    /// the reflections kept from frame to frame (`fs_accumulate`): this frame's is
    /// `history[current]`, the last frame's the other
    history: [wgpu::TextureView; 2],
    current: usize,
    last: Option<History>,
    /// frames drawn with these targets (moves the rays' steps on, see `fs_trace`)
    frame: u32,
    pub(super) view: wgpu::TextureView,
    trace_bg: wgpu::BindGroup,
    blur_x_bg: wgpu::BindGroup,
    blur_y_bg: wgpu::BindGroup,
    accumulate_bg: [wgpu::BindGroup; 2],
    resolve_bg: [wgpu::BindGroup; 2],
    pub(super) down_bg: wgpu::BindGroup,
    pub(super) tonemap_bg: [wgpu::BindGroup; 2],
    pub(super) classic_bg: wgpu::BindGroup,
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
        let hit_depth = r.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("puddle hit depth including glass"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let hit_depth_view = hit_depth.create_view(&Default::default());
        let target = |label, (width, height)| {
            r.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let trace = target("puddle reflection rays", size);
        let vehicle_view = target("puddle player vehicle", size);
        let vehicle_depth = r
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("puddle player vehicle depth"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let blur = target("puddle reflection horizontal filter", size);
        let filtered = target("puddle reflection filtered", size);
        let history = [
            target("puddle reflection history", size),
            target("puddle reflection history", size),
        ];
        let view = target("scene with puddle reflections", (w, h));
        let binding = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        let group = |ray_view, history_view| {
            r.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("puddle reflections"),
                layout: &pipelines.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: pipelines.params.as_entire_binding(),
                    },
                    binding(1, &hdr.view),
                    binding(2, &hdr.mask),
                    binding(3, &r.targets.ao.as_ref().unwrap().depth_view),
                    binding(4, ray_view),
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&r.tone.post_sampler),
                    },
                    binding(6, &r.tone.probe.as_ref().unwrap().view),
                    binding(7, &hit_depth_view),
                    binding(8, &vehicle_view),
                    binding(9, history_view),
                ],
            })
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
            r.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post with puddle reflections"),
                layout: &r.tone.post_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: r.tone.post_buf.as_entire_binding(),
                    },
                    binding(1, &view),
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&r.tone.post_sampler),
                    },
                    binding(3, base),
                    binding(4, adapt),
                    binding(5, screen_mask),
                ],
            })
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
            t.source_depth != *self.targets.ao.as_ref().unwrap().depth_view.texture()
                || t.low != low
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
        self.queue.write_buffer(
            &self.puddles.as_ref().unwrap().vehicle_params,
            0,
            bytemuck::bytes_of(&VehicleUniform { plane, parts }),
        );
        self.queue.write_buffer(
            &self.puddles.as_ref().unwrap().params,
            0,
            bytemuck::bytes_of(&u),
        );
        true
    }

    pub(super) fn encode_puddle_reflections(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        w: u32,
        h: u32,
        scene: &Scene,
        batches: &[Batch],
        draw_list: &[u32],
        lighting: &Lighting,
        camera: &Camera,
        queries: Option<&wgpu::QuerySet>,
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("puddle player vehicle"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.vehicle_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.vehicle_depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: pass_timer(queries, timed, "puddle vehicle"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
            pass.set_bind_group(2, &pipelines.vehicle_bg, &[]);
            if !omsi_cfg::flags::OMSI_NO_PUDDLE_VEHICLE.is_set() {
                if let Some(first) = vehicle_batches.first() {
                    pass.set_bind_group(
                        1,
                        &scene.materials[first.material as usize].bind_group,
                        &[],
                    );
                    pass.set_pipeline(&pipelines.chassis);
                    pass.draw(0..6, 0..origins.len() as u32);
                }
                encode_batches(&mut pass, scene, &vehicle_batches, |pipe| {
                    &pipelines.vehicle[pipe as usize]
                });
            }
        }
        let copy = |texture| wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        };
        encoder.copy_texture_to_texture(
            copy(&targets.source_depth),
            copy(&targets.hit_depth),
            wgpu::Extent3d {
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("puddle glass depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.hit_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: pass_timer(queries, timed, "puddle glass"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
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
            filter_timer
                .as_ref()
                .map(|t| wgpu::RenderPassTimestampWrites {
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
            filter_timer.map(|t| wgpu::RenderPassTimestampWrites {
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
#[path = "puddles_tests.rs"]
mod tests;
