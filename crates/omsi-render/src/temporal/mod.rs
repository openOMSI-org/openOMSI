//! Engine temporal diagnostics. This module does not execute NVIDIA inference.

mod history;
use crate::nvidia::TemporalDebug;
use crate::pipelines::common::{compile_pipeline, compile_shader};
use crate::*;
pub(crate) use history::History;
use history::MotionUniform;

pub(crate) fn motion_source() -> String {
    // Reuse the engine's exact vertex displacement at the PREVIOUS frame's wind/time.
    // Leave the scene module used by every existing backend byte-for-byte unchanged.
    let vertices = include_str!("../shaders/scene/03_vertex.wgsl");
    let start = vertices.find("fn tree_sway(").expect("tree sway function");
    let end = vertices[start..]
        .find("// Where a scene vertex")
        .expect("end of tree sway")
        + start;
    let previous = vertices[start..end]
        .replace("fn tree_sway(", "fn previous_tree_sway(")
        .replace("camera.tree_wind", "temporal.tree_wind")
        .replace("camera.post.y", "temporal.time.x")
        .replace("world_pattern_xy(foot)", "(foot.xy + temporal.origin.xy)");
    let scene = scene_shader_for(false, ArrayPath::Storage)
        .replace("\r\n", "\n")
        .replace("@vertex\nfn vs_depth(", "fn depth_for_motion(");
    format!("{}\n{}\n{}", scene, previous, include_str!("motion.wgsl"))
}

struct Targets {
    size: (u32, u32),
    motion: wgpu::Texture,
    motion_view: wgpu::TextureView,
    valid: wgpu::Texture,
    valid_view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    debug_bg: wgpu::BindGroup,
}

pub(crate) struct MotionPass {
    #[cfg(feature = "neural-external")]
    reprojection: Option<(Mat4, bool)>,
    pipelines: [wgpu::RenderPipeline; 6],
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    previous: wgpu::Buffer,
    previous_positions: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    debug_layout: wgpu::BindGroupLayout,
    debug_uniform: wgpu::Buffer,
    debug_pipeline: wgpu::RenderPipeline,
    targets: Option<Targets>,
    matrices: Vec<[[f32; 4]; 4]>,
    positions: Vec<[f32; 4]>,
    pose_offsets: HashMap<MeshId, u32>,
    batches: Vec<Batch>,
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn buffer(device: &wgpu::Device, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("temporal diagnostics"),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl MotionPass {
    fn new(r: &Renderer) -> Self {
        let device = &r.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motion history"),
            entries: &[
                uniform_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("motion vectors"),
            bind_group_layouts: &[
                Some(&r.camera_layout),
                Some(&r.material_layout),
                Some(&layout),
            ],
            immediate_size: 0,
        });
        let shader = compile_shader(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("motion vectors"),
                source: wgpu::ShaderSource::Wgsl(motion_source().into()),
            },
        );
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        let targets = [
            Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rg16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
            Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
        ];
        let pipelines = std::array::from_fn(|i| {
            compile_pipeline(
                device,
                &wgpu::RenderPipelineDescriptor {
                    label: Some("motion vectors"),
                    layout: Some(&pl),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_motion"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &attributes,
                        }],
                        compilation_options: Default::default(),
                    },
                    primitive: wgpu::PrimitiveState {
                        cull_mode: (i % 2 == 1).then_some(wgpu::Face::Back),
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
                        module: &shader,
                        entry_point: Some(
                            ["fs_motion_opaque", "fs_motion_cutout", "fs_motion_transmap"][i / 2],
                        ),
                        targets: &targets,
                        compilation_options: Default::default(),
                    }),
                    multiview_mask: None,
                    cache: crate::pipeline_cache::get(device).as_ref(),
                },
            )
        });
        let uniform = buffer(
            device,
            std::mem::size_of::<MotionUniform>() as u64,
            wgpu::BufferUsages::UNIFORM,
        );
        let previous = buffer(device, 64, wgpu::BufferUsages::STORAGE);
        let previous_positions = buffer(device, 16, wgpu::BufferUsages::STORAGE);
        let bind_group =
            Self::history_bind_group(device, &layout, &uniform, &previous, &previous_positions);
        let textures = [1, 2, 3].map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: if binding == 3 {
                    wgpu::TextureSampleType::Depth
                } else {
                    wgpu::TextureSampleType::Float { filterable: false }
                },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        let debug_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("temporal visualisation"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                textures[0],
                textures[1],
                textures[2],
            ],
        });
        let debug_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("temporal visualisation"),
            bind_group_layouts: &[Some(&debug_layout)],
            immediate_size: 0,
        });
        let shader = compile_shader(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("temporal visualisation"),
                source: wgpu::ShaderSource::Wgsl(include_str!("debug.wgsl").into()),
            },
        );
        let debug_pipeline = compile_pipeline(
            device,
            &wgpu::RenderPipelineDescriptor {
                label: Some("temporal visualisation"),
                layout: Some(&debug_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_debug"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_debug"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: r.format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: crate::pipeline_cache::get(device).as_ref(),
            },
        );
        Self {
            #[cfg(feature = "neural-external")]
            reprojection: None,
            pipelines,
            layout,
            uniform,
            previous,
            previous_positions,
            bind_group,
            debug_layout,
            debug_uniform: buffer(device, 16, wgpu::BufferUsages::UNIFORM),
            debug_pipeline,
            targets: None,
            matrices: Vec::new(),
            positions: Vec::new(),
            pose_offsets: HashMap::new(),
            batches: Vec::new(),
        }
    }

    fn history_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        previous: &wgpu::Buffer,
        positions: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("motion history"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: previous.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: positions.as_entire_binding(),
                },
            ],
        })
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.size == (width, height))
        {
            return;
        }
        let texture = |format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("temporal diagnostic target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let motion = texture(wgpu::TextureFormat::Rg16Float);
        let valid = texture(wgpu::TextureFormat::R8Unorm);
        let depth = texture(DEPTH_FORMAT);
        let (motion_view, valid_view, depth_view) = (
            motion.create_view(&Default::default()),
            valid.create_view(&Default::default()),
            depth.create_view(&Default::default()),
        );
        let debug_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("temporal visualisation"),
            layout: &self.debug_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.debug_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&motion_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&valid_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
            ],
        });
        self.targets = Some(Targets {
            size: (width, height),
            motion,
            motion_view,
            valid,
            valid_view,
            depth,
            depth_view,
            debug_bg,
        });
    }
}

impl Renderer {
    pub(crate) fn temporal_requested(&self) -> bool {
        if self.options.temporal_debug != TemporalDebug::Off {
            return true;
        }
        #[cfg(feature = "neural-external")]
        {
            return self.external_neural_requested();
        }
        #[cfg(not(feature = "neural-external"))]
        false
    }

    #[cfg(feature = "neural-external")]
    pub(crate) fn temporal_reprojection(&self) -> Option<(Mat4, bool)> {
        self.temporal.as_ref()?.reprojection
    }

    pub fn temporal_buffers(&self) -> Option<nvidia::TemporalBuffers<'_>> {
        if !self.temporal_requested() || self.device_lost().is_some() {
            return None;
        }
        let t = self.temporal.as_ref()?.targets.as_ref()?;
        Some(nvidia::TemporalBuffers {
            motion: &t.motion,
            validity: &t.valid,
            depth: &t.depth,
            size: t.size,
        })
    }

    pub(crate) fn encode_temporal(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &mut Scene,
        f: &passes::FrameCtx,
        vp: Mat4,
        cu: &CameraUniform,
        plan: &passes::batch::DrawPlan,
        timers: &mut passes::PassTimers,
    ) -> bool {
        if !self.temporal_requested() {
            self.temporal = None;
            scene.temporal.reset();
            return false;
        }
        // A viewport key must be added before supporting triple screens/VR. Mirrors and
        // previews must never overwrite the desktop view's histories or textures.
        if !f.with_overlays || f.projection.is_some() || f.width == 0 || f.height == 0 {
            return false;
        }
        if array_path() != ArrayPath::Storage
            || !matches!(
                self.device.adapter_info().backend,
                wgpu::Backend::Dx12 | wgpu::Backend::Vulkan | wgpu::Backend::Metal
            )
        {
            return false;
        }
        if !vp.is_finite() || !vp.inverse().is_finite() {
            scene.temporal.reset();
            return false;
        }
        if self.temporal.is_none() {
            scene.temporal.restart_view();
            self.temporal =
                crate::gpu::optional_pipeline(&self.device, "temporal diagnostics", || {
                    MotionPass::new(self)
                });
            if self.temporal.is_none() {
                self.options.temporal_debug = TemporalDebug::Off;
                return false;
            }
            log::info!(
                "temporal diagnostics: previous-current pixels, Y down, no NVIDIA inference"
            );
        }
        let motion = self.temporal.as_mut().unwrap();
        let mut history = std::mem::take(&mut scene.temporal);
        let size = [f.width, f.height, f.full_w, f.target.texture().height()];
        let uniform = history.uniform(f, vp, cu, size, self.instant_exposure);
        #[cfg(feature = "neural-external")]
        {
            motion.reprojection = Some((
                Mat4::from_cols_array_2d(&uniform.previous_vp),
                uniform.size[2] > 0.5,
            ));
        }
        motion.matrices.clear();
        motion.positions.clear();
        motion.pose_offsets.clear();
        motion.matrices.resize(
            scene.uploaded_entries.max(1) as usize,
            Mat4::IDENTITY.to_cols_array_2d(),
        );
        for (k, i) in scene.instances.iter().enumerate() {
            let mesh = &scene.meshes[i.mesh];
            let mut m = history.model(k, i, f.ro, mesh);
            m[1][3] = -1.0;
            m[2][3] = mesh.base_vertex as f32;
            if let Some(pose) = history.previous_pose(i.mesh, mesh.generation) {
                // A topology/count change needs fresh history, never an out-of-range read.
                if pose.positions.len() as u64 * std::mem::size_of::<Vertex>() as u64
                    == mesh.vertex_bytes
                {
                    let at = *motion.pose_offsets.entry(i.mesh).or_insert_with(|| {
                        let at = motion.positions.len() as u32;
                        motion.positions.extend_from_slice(&pose.positions);
                        at
                    });
                    m[1][3] = at as f32;
                } else {
                    m[0][3] = 0.0;
                }
            }
            let first = i.base as usize;
            if let Some(entries) = motion.matrices.get_mut(first..first + i.slot_alpha.len()) {
                entries.fill(m);
            }
        }
        if motion.positions.is_empty() {
            motion.positions.push([0.0; 4]);
        }
        let bytes = bytemuck::cast_slice(&motion.matrices);
        let positions = bytemuck::cast_slice(&motion.positions);
        let limit = self.device.limits().max_storage_buffer_binding_size as u64;
        if bytes.len() as u64 > limit
            || positions.len() as u64 > limit
            || motion.positions.len() >= 1 << 24
        {
            scene.temporal = history;
            self.options.temporal_debug = TemporalDebug::Off;
            self.temporal = None;
            log::warn!("temporal diagnostics disabled: history exceeds GPU storage binding limit");
            return false;
        }
        let needs_growth = motion.previous.size() < bytes.len() as u64
            || motion.previous_positions.size() < positions.len() as u64;
        let grown = !needs_growth
            || crate::gpu::optional_pipeline(&self.device, "temporal history buffers", || {
                let mut changed = false;
                if motion.previous.size() < bytes.len() as u64 {
                    motion.previous = buffer(
                        &self.device,
                        (bytes.len() as u64).div_ceil(256) * 256,
                        wgpu::BufferUsages::STORAGE,
                    );
                    changed = true;
                }
                if motion.previous_positions.size() < positions.len() as u64 {
                    motion.previous_positions = buffer(
                        &self.device,
                        (positions.len() as u64).div_ceil(256) * 256,
                        wgpu::BufferUsages::STORAGE,
                    );
                    changed = true;
                }
                if changed {
                    motion.bind_group = MotionPass::history_bind_group(
                        &self.device,
                        &motion.layout,
                        &motion.uniform,
                        &motion.previous,
                        &motion.previous_positions,
                    );
                }
            })
            .is_some();
        if !grown {
            self.options.temporal_debug = TemporalDebug::Off;
            self.temporal = None;
            scene.temporal.reset();
            return false;
        }
        self.queue
            .write_buffer(&motion.uniform, 0, bytemuck::bytes_of(&uniform));
        self.queue.write_buffer(&motion.previous, 0, bytes);
        self.queue
            .write_buffer(&motion.previous_positions, 0, positions);
        // Scope optional allocation failures too; the normal picture remains usable.
        let needs_resize = motion
            .targets
            .as_ref()
            .is_none_or(|t| t.size != (f.width, f.height));
        let resized = !needs_resize
            || crate::gpu::optional_pipeline(&self.device, "temporal targets", || {
                motion.resize(&self.device, f.width, f.height)
            })
            .is_some();
        if !resized {
            self.options.temporal_debug = TemporalDebug::Off;
            self.temporal = None;
            scene.temporal.reset();
            return false;
        }
        motion.batches.clear();
        motion.batches.extend(
            plan.main_batches
                .iter()
                .chain(&plan.cab_batches)
                .cloned()
                .map(|mut batch| {
                    let kind = batch.pipe / 4;
                    batch.pipe = if kind < 2 {
                        kind * 2 + (batch.pipe & 1)
                    } else {
                        4 + (batch.pipe & 1)
                    };
                    batch
                }),
        );
        let t = motion.targets.as_ref().unwrap();
        let color = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("motion vectors"),
                color_attachments: &[color(&t.motion_view), color(&t.valid_view)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: pass_timer(
                    timers.set.as_ref(),
                    &mut timers.timed,
                    "motion vectors",
                ),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
            pass.set_bind_group(2, &motion.bind_group, &[]);
            // Full visible draw list without AO's distance cutoff; preserve batching
            // across materials/meshes. Partial transparency writes invalid history.
            encode_batches(&mut pass, scene, &motion.batches, |p| {
                &motion.pipelines[p as usize]
            });
        }
        history.commit(scene, f, vp, cu, size);
        scene.temporal = history;
        true
    }

    pub(crate) fn encode_temporal_debug(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        f: &passes::FrameCtx,
    ) {
        let Some(motion) = &self.temporal else { return };
        let Some(t) = &motion.targets else { return };
        let mode = match self.options.temporal_debug {
            TemporalDebug::Off => return,
            TemporalDebug::Motion => 1.0,
            TemporalDebug::Depth => 2.0,
            TemporalDebug::Validity => 3.0,
        };
        self.queue.write_buffer(
            &motion.debug_uniform,
            0,
            bytemuck::cast_slice(&[
                f.full_w as f32,
                f.target.texture().height() as f32,
                mode,
                0.0,
            ]),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("temporal visualisation"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: f.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&motion.debug_pipeline);
        pass.set_bind_group(0, &t.debug_bg, &[]);
        pass.draw(0..3, 0..1);
        // Preserve the engine's HUD after replacing the 3D picture with diagnostics.
        pass.set_pipeline(&self.overlay_pipeline_1x);
        for (k, _) in f.overlays.iter().enumerate() {
            if let Some((_, _, bg, _)) = scene.overlay_res.get(k) {
                pass.set_bind_group(0, bg, &[]);
                pass.draw(0..6, 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests;
