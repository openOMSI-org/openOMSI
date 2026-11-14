use crate::pipelines::common::*;
use crate::*;
use std::sync::atomic::{AtomicU8, Ordering};

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
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    state: Arc<AtomicU8>,
    waiting: Option<Vec<DVec3>>,
    frames_waiting: u32,
    pub(crate) candidates: Vec<DVec3>,
    seen: Vec<(DVec3, f32)>,
}

impl LampSight {
    pub(crate) fn new(device: &wgpu::Device, leave_out_enhanced: bool) -> Option<LampSight> {
        if leave_out_enhanced || basic_pipelines() || gl_backend() || omsi_cfg::flags::OMSI_NO_LAMP_SIGHT.is_set() {
            return None;
        }
        optional_pipeline(device, "the lamp sight", || LampSight::build(device))
    }

    fn build(device: &wgpu::Device) -> LampSight {
        let module = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("lamp sight"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../lamp_sight.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lamp sight"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture_entry(1, wgpu::TextureSampleType::Depth, wgpu::TextureViewDimension::D2),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lamp sight"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let targets = [target(wgpu::TextureFormat::Rgba8Unorm, None)];
        let pipeline = no_vertex_pipeline("lamp sight", &pl, &module, "fs_sight", &targets).create(device);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lamp sight"),
            size: wgpu::Extent3d { width: SIGHT_LAMPS as u32, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lamp sight readback"),
            size: SIGHT_LAMPS as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        LampSight {
            pipeline,
            layout,
            uniform: uniform_buffer(device, "lamp sight", std::mem::size_of::<SightUniform>() as u64),
            view: texture.create_view(&Default::default()),
            readback,
            state: Arc::new(AtomicU8::new(0)),
            waiting: None,
            frames_waiting: 0,
            candidates: Vec::new(),
            seen: Vec::new(),
        }
    }

    pub(crate) fn seen(&self, at: DVec3) -> f32 {
        self.seen.iter().find(|(p, _)| p.distance_squared(at) < 1.0).map_or(1.0, |s| s.1)
    }

    pub(crate) fn idle(&self) -> bool {
        self.waiting.is_none()
    }

    pub(crate) fn take(&mut self, device_lost: bool) {
        if device_lost {
            self.waiting = None;
            self.frames_waiting = 0;
            return;
        }
        let Some(lamps) = self.waiting.as_ref() else { return };
        self.frames_waiting += 1;
        if self.frames_waiting > 240 {
            self.seen.clear();
        }
        match self.state.swap(0, Ordering::Relaxed) {
            1 => {
                {
                    let bytes = self.readback.slice(..).get_mapped_range();
                    self.seen = lamps.iter().enumerate().map(|(i, p)| (*p, bytes[i * 4] as f32 / 255.0)).collect();
                }
                self.readback.unmap();
                self.waiting = None;
                self.frames_waiting = 0;
            }
            2 => {
                self.waiting = None;
                self.frames_waiting = 0;
            }
            _ => {}
        }
    }

    pub(crate) fn encode(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, depth: &wgpu::TextureView, view_proj: Mat4, ro: DVec3, eye: Vec3) {
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
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lamp sight"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lamp sight"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: self.view.texture(), mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIGHT_LAMPS as u32 * 4), rows_per_image: Some(1) },
            },
            wgpu::Extent3d { width: SIGHT_LAMPS as u32, height: 1, depth_or_array_layers: 1 },
        );
        let state = self.state.clone();
        encoder.map_buffer_on_submit(&self.readback, wgpu::MapMode::Read, .., move |r| {
            state.store(if r.is_ok() { 1 } else { 2 }, Ordering::Relaxed)
        });
        self.waiting = Some(self.candidates.clone());
    }
}

pub(crate) fn brightest_glare(lights: &[PointLight], ro: DVec3, cam_rel: Vec3, forward: Vec3, view_proj: Mat4) -> Vec<DVec3> {
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
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let mut sight = LampSight::build(&device);
        sight.readback.destroy();
        sight.waiting = Some(vec![DVec3::ZERO]);
        sight.state.store(1, Ordering::Relaxed);

        sight.take(true);

        assert!(sight.waiting.is_none());
    }
}
