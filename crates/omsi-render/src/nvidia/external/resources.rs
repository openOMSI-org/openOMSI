use super::*;
use crate::{Camera, Renderer, Scene, passes::FrameCtx};
use glam::{DVec3, Mat4};

pub(super) struct Resources {
    size: (u32, u32),
    pub(super) scene: wgpu::Texture,
    output: wgpu::Texture,
    original_bg: wgpu::BindGroup,
    output_bg: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    staging: [wgpu::Buffer; 4],
    pitch: [u32; 4],
}
impl Resources {
    fn new(device: &wgpu::Device, size: (u32, u32), format: wgpu::TextureFormat) -> Self {
        let extent = wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        };
        let texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let scene = texture("neural HUD-free LDR", format);
        let output = texture("accepted neural LDR", wgpu::TextureFormat::Rgba8UnormSrgb);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("neural composition"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let bg = |texture: &wgpu::Texture| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("neural composition"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                }],
            })
        };
        let original_bg = bg(&scene);
        let output_bg = bg(&output);
        let shader = crate::pipelines::common::compile_shader(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("neural composition"),
                source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
            },
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("neural composition"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = crate::pipelines::common::no_vertex_pipeline(
            "neural composition",
            &pipeline_layout,
            &shader,
            "fs_main",
            &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        )
        .create(device);
        let pitch =
            std::array::from_fn(|i| (size.0 * if i == 3 { 1 } else { 4 }).div_ceil(256) * 256);
        let staging = std::array::from_fn(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("neural cached staging"),
                size: u64::from(pitch[i]) * u64::from(size.1),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        Self {
            size,
            scene,
            output,
            original_bg,
            output_bg,
            pipeline,
            staging,
            pitch,
        }
    }
    fn read(&self, renderer: &Renderer) -> Result<[Vec<u8>; 4]> {
        let temporal = renderer
            .temporal_buffers()
            .context("temporal resources unavailable")?;
        ensure!(
            temporal.size == self.size,
            "prototype requires render scale 100%; temporal and color extents differ"
        );
        let sources = [
            (&self.scene, wgpu::TextureAspect::All),
            (temporal.motion, wgpu::TextureAspect::All),
            (temporal.depth, wgpu::TextureAspect::DepthOnly),
            (temporal.validity, wgpu::TextureAspect::All),
        ];
        let mut enc = renderer.device.create_command_encoder(&Default::default());
        for (i, (texture, aspect)) in sources.into_iter().enumerate() {
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.staging[i],
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.pitch[i]),
                        rows_per_image: Some(self.size.1),
                    },
                },
                self.scene.size(),
            );
        }
        let submission = renderer.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        for buffer in &self.staging {
            let tx = tx.clone();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
        }
        let result = (|| {
            crate::wait_gpu(&renderer.device, Some(submission))
                .map_err(|e| anyhow::anyhow!("staging poll: {e:?}"))?;
            for _ in 0..4 {
                rx.recv()?
                    .map_err(|e| anyhow::anyhow!("staging map: {e:?}"))?;
            }
            Ok(std::array::from_fn(|i| {
                let mapped = self.staging[i].slice(..).get_mapped_range();
                let width = self.size.0 as usize * if i == 3 { 1 } else { 4 };
                let mut bytes = Vec::with_capacity(width * self.size.1 as usize);
                for row in mapped.chunks_exact(self.pitch[i] as usize) {
                    bytes.extend_from_slice(&row[..width]);
                }
                bytes
            }))
        })();
        for buffer in &self.staging {
            buffer.unmap();
        }
        result
    }
}

pub(super) fn payload(
    mut planes: [Vec<u8>; 4],
    format: wgpu::TextureFormat,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let pixels = planes[0].len() / 4;
    ensure!(
        planes[0].len() == pixels * 4
            && planes[1].len() == pixels * 4
            && planes[2].len() == pixels * 4
            && planes[3].len() == pixels,
        "plane size mismatch"
    );
    if format == wgpu::TextureFormat::Bgra8UnormSrgb {
        for p in planes[0].chunks_exact_mut(4) {
            p.swap(0, 2);
        }
    }
    let mut packed = Vec::with_capacity(pixels * 17);
    packed.extend_from_slice(&planes[0]);
    for i in 0..pixels {
        let m = &planes[1][i * 4..i * 4 + 4];
        let x = crate::render::half_to_f32(u16::from_le_bytes([m[0], m[1]]));
        let y = crate::render::half_to_f32(u16::from_le_bytes([m[2], m[3]]));
        let z = f32::from_le_bytes(planes[2][i * 4..i * 4 + 4].try_into().unwrap());
        if !x.is_finite() || !y.is_finite() || !z.is_finite() || !(0.0..=1.0).contains(&z) {
            planes[3][i] = 0;
        }
        packed.extend_from_slice(&if x.is_finite() { x } else { 0. }.to_le_bytes());
        packed.extend_from_slice(&if y.is_finite() { y } else { 0. }.to_le_bytes());
    }
    for z in planes[2].chunks_exact(4) {
        let d = f32::from_le_bytes(z.try_into().unwrap());
        packed.extend_from_slice(&if d.is_finite() { d.clamp(0., 1.) } else { 0. }.to_le_bytes());
    }
    packed.extend_from_slice(&planes[3]);
    Ok((
        packed,
        std::mem::take(&mut planes[0]),
        std::mem::take(&mut planes[3]),
    ))
}

pub(super) fn camera_constants(
    camera: &Camera,
    ro: DVec3,
    vp: Mat4,
    previous: Mat4,
    size: (u32, u32),
) -> [f32; 80] {
    let aspect = size.0 as f32 / size.1 as f32;
    let projection =
        Mat4::perspective_rh(camera.fov_deg.to_radians(), aspect, camera.far, camera.near);
    let reprojection = previous * vp.inverse();
    let view_to_world = Mat4::look_to_rh(
        (camera.position - ro).as_vec3(),
        camera.forward(),
        camera.up(),
    )
    .inverse();
    let mut values = [0.; 80];
    for (i, m) in [
        projection,
        projection.inverse(),
        reprojection,
        reprojection.inverse(),
    ]
    .into_iter()
    .enumerate()
    {
        // SL uses row vectors and row-major storage: the rows of m.transpose()
        // are the columns of glam's column-vector matrix m.
        values[i * 16..i * 16 + 16].copy_from_slice(&m.to_cols_array());
    }
    for (i, v) in [
        (camera.position - ro).as_vec3(),
        view_to_world.y_axis.truncate(),
        view_to_world.x_axis.truncate(),
        // Streamline's camera-to-world basis uses the view's +Z axis. In this
        // RH projection it points opposite the engine's look direction.
        view_to_world.z_axis.truncate(),
    ]
    .into_iter()
    .enumerate()
    {
        values[64 + i * 3..67 + i * 3].copy_from_slice(&v.to_array());
    }
    values[76..].copy_from_slice(&[camera.near, camera.far, camera.fov_deg.to_radians(), aspect]);
    values
}

impl Renderer {
    /// Only the main desktop view can own this adapter. Off-screen inputs are cached.
    pub(crate) fn external_neural_target(
        &mut self,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Option<wgpu::TextureView> {
        self.neural_external.environment();
        if !self.neural_external.enabled() {
            return None;
        }
        if !matches!(
            self.device.adapter_info().backend,
            wgpu::Backend::Dx12 | wgpu::Backend::Vulkan
        ) {
            self.neural_external
                .fail("the staging prototype requires DX12 or Vulkan temporal buffers");
            self.neural_external.diagnostics.state = State::UnsupportedGraphicsAPI;
            return None;
        }
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
        ) || size.0 == 0
            || size.1 == 0
            || u64::from(size.0) * u64::from(size.1) > MAX_PIXELS
        {
            self.neural_external.fail("unsupported prototype extent/color format (requires sRGB RGBA8/BGRA8, at most 3840x2160)");
            return None;
        }
        let rebuild = self
            .neural_external
            .resources
            .as_ref()
            .is_none_or(|r| r.size != size || r.scene.format() != format);
        if rebuild {
            self.neural_external.resources =
                crate::gpu::optional_pipeline(&self.device, "external neural staging", || {
                    Resources::new(&self.device, size, format)
                });
            self.neural_external.generation = self.neural_external.generation.saturating_add(1);
            if self.neural_external.resources.is_none() {
                self.neural_external
                    .fail("optional staging allocation failed");
                return None;
            }
        }
        Some(
            self.neural_external
                .resources
                .as_ref()?
                .scene
                .create_view(&Default::default()),
        )
    }

    pub(crate) fn external_neural_finish(
        &mut self,
        scene: &Scene,
        f: &FrameCtx,
        target: &wgpu::TextureView,
        overlays: &[(crate::TextureId, [f32; 4])],
        vp: Mat4,
    ) {
        let Some(resources) = self.neural_external.resources.take() else {
            return;
        };
        let result = (|| -> Result<(Vec<u8>, usize)> {
            ensure!(self.device_lost().is_none(), "device lost");
            ensure!(
                self.options.temporal_debug == crate::nvidia::TemporalDebug::Off,
                "disable temporal debug before inference"
            );
            let planes = resources.read(self)?;
            let (packed, original, validity) = payload(planes, resources.scene.format())?;
            let (previous, valid) = self
                .temporal_reprojection()
                .context("temporal reprojection unavailable")?;
            let header = Header {
                kind: REQUEST,
                session: 0,
                frame: 0,
                generation: self.neural_external.generation,
                width: resources.size.0,
                height: resources.size.1,
                flags: 1 | 4 | if valid { 0 } else { 2 },
                payload: packed.len() as u64,
                stages: 0,
                result: 0,
                camera: camera_constants(f.camera, f.ro, vp, previous, resources.size),
            };
            if let Some(capture) = &mut self.neural_external.capture {
                let complete = capture.write(header, &packed)?;
                self.neural_external.diagnostics.captures += 1;
                if complete {
                    self.neural_external.capture = None;
                    self.neural_external.diagnostics.state = State::CaptureComplete;
                }
                return Ok((original, 0));
            }
            let mut output = self
                .neural_external
                .session
                .as_mut()
                .context("host disconnected")?
                .exchange(header, &packed, &mut self.neural_external.diagnostics)?;
            let mut used = 0;
            for ((p, old), valid) in output
                .chunks_exact_mut(4)
                .zip(original.chunks_exact(4))
                .zip(validity)
            {
                if valid < 255 {
                    p.copy_from_slice(old);
                } else {
                    used += 1;
                    p[3] = old[3];
                }
            }
            Ok((output, used))
        })();
        let mut composed = false;
        if let Ok((bytes, used)) = &result {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &resources.output,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(resources.size.0 * 4),
                    rows_per_image: Some(resources.size.1),
                },
                resources.output.size(),
            );
            composed = *used > 0;
            if !composed
                && !matches!(
                    self.neural_external.diagnostics.state,
                    State::Capturing | State::CaptureComplete
                )
            {
                self.neural_external.diagnostics.state = State::ReadyForTransport;
            }
        } else if let Err(error) = &result {
            self.neural_external.fail(error);
        }
        if self.device_lost().is_none() {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("neural output or original fallback, then HUD"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
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
                pass.set_pipeline(&resources.pipeline);
                pass.set_bind_group(
                    0,
                    if composed {
                        &resources.output_bg
                    } else {
                        &resources.original_bg
                    },
                    &[],
                );
                pass.draw(0..3, 0..1);
                pass.set_pipeline(&self.overlay_pipeline_1x);
                for (i, _) in overlays.iter().enumerate() {
                    if let Some((_, _, bg, _)) = scene.overlay_res.get(i) {
                        pass.set_bind_group(0, bg, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
            }
            let submission = self.queue.submit([encoder.finish()]);
            let finished = crate::wait_gpu(&self.device, Some(submission));
            if let Err(error) = &finished {
                self.neural_external
                    .fail(format!("composition GPU completion: {error:?}"));
            }
            if composed && finished.is_ok() {
                self.neural_external.diagnostics.compositions += 1;
                self.neural_external.diagnostics.state = State::Active;
                log::info!(
                    "neural external: evaluation={}, composition={} (GPU completed, presentation not certified)",
                    self.neural_external.diagnostics.evaluations,
                    self.neural_external.diagnostics.compositions
                );
            }
        }
        // Cache only while attached. Failure releases optional allocations after the
        // fallback submission has completed; normal rendering resumes next frame.
        self.neural_external.resources = self.neural_external.enabled().then_some(resources);
    }
}
