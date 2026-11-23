use crate::pipelines::common::*;
use crate::*;

pub(crate) const SIGHT_LAMPS: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SightUniform {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    lamps: [[f32; 4]; SIGHT_LAMPS],
}

pub(crate) struct LampSight {
    pipeline: mx::RenderPipeline,
    uniform: mx::Buffer,
    view: mx::TextureView,
    readback: mx::Buffer,
    /// The submission that carries the copy to `readback`, once submitted.
    submission: Option<mx::SubmissionIndex>,
    waiting: Option<Vec<DVec3>>,
    frames_waiting: u32,
    pub(crate) candidates: Vec<DVec3>,
    seen: Vec<(DVec3, f32)>,
}

impl LampSight {
    pub(crate) fn new(device: &mx::Device, leave_out_enhanced: bool) -> Option<LampSight> {
        if leave_out_enhanced
            || basic_pipelines()
            || gl_backend()
            || omsi_cfg::flags::OMSI_NO_LAMP_SIGHT.is_set()
        {
            return None;
        }
        optional_pipeline(device, "the lamp sight", || LampSight::build(device))
    }

    fn build(device: &mx::Device) -> LampSight {
        let module = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("lamp sight"),
                source: &*(crate::shaders::hot_wgsl(
                    include_str!("../lamp_sight.wgsl"),
                    "lamp_sight.wgsl",
                )),
            },
        );
        let targets = [target(mx::TextureFormat::Rgba8Unorm, None)];
        let pipeline =
            no_vertex_pipeline("lamp sight", &module, "fs_sight", &targets).create(device);
        let texture = device
            .create_texture(&mx::TextureDescriptor {
                label: Some("lamp sight"),
                size: mx::Extent3d {
                    width: SIGHT_LAMPS as u32,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: mx::TextureFormat::Rgba8Unorm,
                usage: mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
            .expect("mX: a texture");
        let readback = device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("lamp sight readback"),
                size: SIGHT_LAMPS as u64 * 4,
                usage: mx::BufferUsages::COPY_DST | mx::BufferUsages::MAP_READ,
            })
            .expect("mX: a buffer");
        LampSight {
            pipeline,
            uniform: uniform_buffer(
                device,
                "lamp sight",
                std::mem::size_of::<SightUniform>() as u64,
            ),
            view: texture
                .create_view(&Default::default())
                .expect("mX: a texture view"),
            readback,
            submission: None,
            waiting: None,
            frames_waiting: 0,
            candidates: Vec::new(),
            seen: Vec::new(),
        }
    }

    pub(crate) fn seen(&self, at: DVec3) -> f32 {
        self.seen
            .iter()
            .find(|(p, _)| p.distance_squared(at) < 1.0)
            .map_or(1.0, |s| s.1)
    }

    pub(crate) fn idle(&self) -> bool {
        self.waiting.is_none()
    }

    /// The frame's commands went in `index`.
    pub(crate) fn submitted(&mut self, index: mx::SubmissionIndex) {
        if self.waiting.is_some() && self.submission.is_none() {
            self.submission = Some(index);
        }
    }

    pub(crate) fn take(&mut self, device: &mx::Device, device_lost: bool) {
        if device_lost {
            self.waiting = None;
            self.submission = None;
            self.frames_waiting = 0;
            return;
        }
        let Some(lamps) = self.waiting.as_ref() else {
            return;
        };
        self.frames_waiting += 1;
        if self.frames_waiting > 240 {
            self.seen.clear();
        }
        if self.submission.is_none_or(|i| !device.is_done(i)) {
            return;
        }
        let read = self
            .readback
            .map_read_now(0..SIGHT_LAMPS as u64 * 4, |bytes| {
                lamps
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (*p, bytes[i * 4] as f32 / 255.0))
                    .collect::<Vec<_>>()
            });
        if let Ok(seen) = read {
            self.seen = seen;
        }
        self.submission = None;
        self.waiting = None;
        self.frames_waiting = 0;
    }

    pub(crate) fn encode(
        &mut self,
        device: &mx::Device,
        queue: &mx::Queue,
        encoder: &mut mx::CommandEncoder,
        depth: &mx::TextureView,
        view_proj: Mat4,
        ro: DVec3,
        eye: Vec3,
    ) {
        if self.waiting.is_some() || self.candidates.is_empty() {
            return;
        }
        let mut u = SightUniform {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            eye: eye.extend(1.0).to_array(),
            lamps: [[0.0; 4]; SIGHT_LAMPS],
        };
        for (slot, p) in u.lamps.iter_mut().zip(&self.candidates) {
            *slot = (*p - ro).as_vec3().extend(1.0).to_array();
        }
        queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u))
            .expect("mX: a queue write");
        let bg = device.create_bind_group(&mx::BindGroupDescriptor {
            label: Some("lamp sight"),
            entries: &[
                mx::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                mx::BindGroupEntry {
                    binding: 1,
                    resource: mx::BindingResource::TextureView(depth),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("lamp sight"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view: &self.view,
                    resolve_target: None,
                    ops: mx::Operations {
                        load: mx::LoadOp::Clear(mx::Color::WHITE),
                        store: mx::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            mx::TexelCopyTextureInfo {
                texture: &self.view.texture(),
                mip_level: 0,
                origin: mx::Origin3d::ZERO,
                aspect: mx::TextureAspect::All,
            },
            mx::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIGHT_LAMPS as u32 * 4),
                    rows_per_image: Some(1),
                },
            },
            mx::Extent3d {
                width: SIGHT_LAMPS as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.waiting = Some(self.candidates.clone());
    }
}

pub(crate) fn brightest_glare(
    lights: &[PointLight],
    ro: DVec3,
    cam_rel: Vec3,
    forward: Vec3,
    view_proj: Mat4,
) -> Vec<DVec3> {
    let mut sources: Vec<(DVec3, f32)> = glare_sources(lights, ro, cam_rel, forward)
        .into_iter()
        .filter(|(at, _)| {
            let clip = view_proj * (*at - ro).as_vec3().extend(1.0);
            clip.w > 0.0 && clip.x.abs() <= clip.w && clip.y.abs() <= clip.w
        })
        .collect();
    if sources.len() > SIGHT_LAMPS {
        sources.select_nth_unstable_by(SIGHT_LAMPS, |a, b| b.1.total_cmp(&a.1));
        sources.truncate(SIGHT_LAMPS);
    }
    sources.into_iter().map(|s| s.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discards_a_completed_readback_after_device_loss() {
        let instance = mx::Instance::with_backends(mx::Backends::NOOP);
        let adapter = instance.default_adapter().unwrap();
        let (device, _) = adapter
            .request_device(&mx::DeviceDescriptor::default())
            .unwrap();
        let mut sight = LampSight::build(&device);
        sight.waiting = Some(vec![DVec3::ZERO]);
        sight.submission = Some(mx::SubmissionIndex(1));

        sight.take(&device, true);

        assert!(sight.waiting.is_none());
    }
}
