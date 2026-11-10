//! Planar reflections of the lit cabin in its windows. Unlike screen-space rays,
//! a mirrored geometry capture sees the seats, handrails and people behind the eye.
//! Two visible pane planes, at at most 640 x 360 each, bound the extra rendering.

use crate::*;
use crate::passes::{FrameCtx, kind_of};

const PLANES: usize = 2;
// Half strength at 3 m, fading smoothly away between 8 and 14 m of travel to the
// pane. This visibility envelope keeps distant saloon imagery from washing out
// the road; it supplements the actual lamp-to-surface lighting, not replaces it.
const DISTANCE_FADE: [f32; 4] = [3.0, 8.0, 14.0, 0.0];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Uniform {
    planes: [[f32; 4]; PLANES],
    screen: [f32; 4],
}

pub(crate) struct Plan {
    planes: Vec<glam::Vec4>,
    panes: Vec<Batch>,
    cabin: Vec<Batch>,
}

fn pane(mat: &Material) -> bool {
    mat.alpha == AlphaMode::Blend && mat.uniform.emissive[3] == 1.0
}

fn plane_error(a: glam::Vec4, b: glam::Vec4) -> f32 {
    let b = if a.truncate().dot(b.truncate()) < 0.0 { -b } else { b };
    (a.w - b.w).abs() + 4.0 * (1.0 - a.truncate().dot(b.truncate()).clamp(-1.0, 1.0))
}

/// Keep the largest visible surfaces, merging the two faces/layers of a pane.
/// Planes come from the authored triangles, so tilted glass and a rotated bus
/// reflect the cabin in the right direction rather than assuming a vertical wall.
fn planes(scene: &Scene, f: &FrameCtx, visible: &[(usize, f32, bool)], origin: DVec3) -> Vec<glam::Vec4> {
    let mut candidates = Vec::new();
    for &(i, _, _) in visible {
        let inst = &scene.instances[i];
        if !inst.visible || inst.roof.is_none() || inst.origin.distance_squared(origin) > 0.01 { continue; }
        let mesh = &scene.meshes[inst.mesh];
        let normal_matrix = inst.transform.inverse().transpose();
        for flat in &mesh.flats {
            let mat = &scene.materials[inst.materials.get(flat.slot as usize).copied().unwrap_or(0)];
            if !pane(mat) || (mat.uniform.glow[2] < 0.5 && inst.slot_alpha.get(flat.slot as usize).is_some_and(|a| *a < 1.0 / 512.0)) { continue; }
            let centre = (flat.lo + flat.hi) * 0.5;
            let point = centre - flat.normal * (flat.normal.dot(centre) - flat.offset);
            let point = (inst.origin - f.ro).as_vec3() + inst.transform.transform_point3(point);
            let mut n = normal_matrix.transform_vector3(flat.normal).normalize_or_zero();
            let to_eye = f.cam_rel - point;
            if n.dot(to_eye) < 0.0 { n = -n; }
            let distance = to_eye.length();
            if n.length_squared() < 0.5 || n.dot(to_eye) < 0.08 || distance > 8.0 { continue; }
            let radius = (flat.hi - flat.lo).length() * scene::transform_scale(inst.transform) * 0.5;
            if (-to_eye).dot(f.camera.forward()) + radius < 0.1 { continue; }
            // Projected area favours the windscreen when looking forwards, and the
            // side pane when turning to it. Very small instrument covers stay out.
            let extent = (flat.hi - flat.lo) * scene::transform_scale(inst.transform);
            let area = extent.x * extent.y + extent.x * extent.z + extent.y * extent.z;
            if area < 0.12 { continue; }
            let score = area * (n.dot(to_eye) / distance.max(0.1)) / distance.max(0.4).powi(2);
            candidates.push((score, n.extend(n.dot(point))));
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut selected = Vec::new();
    for (_, p) in candidates {
        if selected.iter().any(|&q| plane_error(p, q) < 0.12) { continue; }
        selected.push(p);
        if selected.len() == PLANES { break; }
    }
    selected
}

pub(crate) fn plan(scene: &Scene, f: &FrameCtx, visible: &[(usize, f32, bool)], list: &mut Vec<u32>) -> Option<Plan> {
    if !f.enhanced || f.lighting.reflection_strength <= 0.0 || !f.prepass_on || !(f.with_overlays || f.xr_view) { return None; }
    let own = f.lighting.inside.as_ref()?;
    if !point_in_vehicle_box(f.camera.position, own) { return None; }
    let planes = planes(scene, f, visible, own.0);
    if f.env.debug_draws { log::info!("cabin reflection planes: {planes:?}"); }
    if planes.is_empty() { return None; }
    let parts: Vec<_> = std::iter::once(own).chain(f.lighting.puddle_parts.iter()).take(4).collect();
    let bounds: Vec<_> = parts.iter().map(|p| passes::VehicleBox::new(p, f.ro)).collect();
    let mut pane_items = Vec::new();
    let mut solid_items = Vec::new();
    let mut blend_items = Vec::new();
    // Deliberately independent of main-view culling: the reflected cabin is behind
    // the camera, and the mirror-only driver must be visible in the reflection.
    for inst in &scene.instances {
        if !inst.visible || inst.blob || inst.surface { continue; }
        let own_part = inst.roof.is_some() && parts.iter().any(|p| inst.origin.distance_squared(p.0) < 0.01);
        let centre = (inst.origin - f.ro).as_vec3() + inst.bounds.centre;
        let human = inst.roof.is_none() && inst.bounds.radius < 2.0 && bounds.iter().any(|b| b.contains(centre, -0.05));
        if !own_part && !human { continue; }
        let radius = if inst.object_radius > 0.0 { inst.object_radius } else { inst.bounds.radius };
        let distance = if inst.object_radius > 0.0 { inst.origin.distance(f.camera.position) as f32 } else { centre.distance(f.cam_rel) };
        let size = scene::object_screen_size(radius, distance, f.camera.fov_deg.to_radians().max(1e-3));
        if size < inst.lod.0 || (inst.lod.1 < f32::MAX && size >= inst.lod.1) { continue; }
        for (ri, &(_, _, slot)) in scene.meshes[inst.mesh].ranges.iter().enumerate() {
            let material = inst.materials.get(slot as usize).copied().unwrap_or(0);
            let mat = &scene.materials[material];
            let alpha = inst.slot_alpha.get(slot as usize).copied().unwrap_or(1.0);
            let mut item = DrawItem { pipe: pipe_code(kind_of(mat.alpha), culls_back_faces(scene, inst), false), mesh: inst.mesh as u32, range: ri as u32, material: material as u32, look: mat.look, entry: inst.base + slot };
            if pane(mat) {
                if own_part && (alpha >= 1.0 / 512.0 || mat.uniform.glow[2] > 0.5) { pane_items.push(item); }
                continue;
            }
            // Never capture rain, dirt, or glass again: that would reflect the
            // environment photograph and opaque window films into the cabin.
            if mat.uniform.emissive[3] > 0.5 || (mat.alpha == AlphaMode::Blend && (mat.no_z_write || alpha < 1.0 / 512.0)) { continue; }
            if mat.alpha == AlphaMode::Blend {
                item.pipe = pipe_code(PIPE_BLEND, culls_back_faces(scene, inst), false);
                blend_items.push(item);
            } else {
                solid_items.push(item);
            }
        }
    }
    let (mut panes, mut cabin) = (Vec::new(), Vec::new());
    batch_items(scene, &mut pane_items, true, list, &mut panes);
    batch_items(scene, &mut solid_items, true, list, &mut cabin);
    batch_items(scene, &mut blend_items, false, list, &mut cabin);
    Some(Plan { planes, panes, cabin })
}

struct Capture {
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    plane: wgpu::Buffer,
    plane_bg: wgpu::BindGroup,
    params: wgpu::Buffer,
    picture_bg: wgpu::BindGroup,
}

pub(crate) struct Pipelines {
    layout: wgpu::BindGroupLayout,
    compose: [wgpu::RenderPipeline; 2],
    resolve: wgpu::RenderPipeline,
    overlay: Option<(wgpu::TextureView, wgpu::BindGroup)>,
    captures: Vec<Capture>,
    size: (u32, u32),
}

impl Pipelines {
    pub(crate) fn new(device: &wgpu::Device, shader: &wgpu::ShaderModule, camera: &wgpu::BindGroupLayout, material: &wgpu::BindGroupLayout, plane: &wgpu::BindGroupLayout) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cabin reflection picture"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniform>() as u64) }, count: None },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("cabin reflection"), bind_group_layouts: &[Some(camera), Some(material), Some(plane), Some(&layout)], immediate_size: 0 });
        let compose = [false, true].map(|cull| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cabin reflection on glass"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), buffers: &[wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2] }], compilation_options: Default::default() },
            primitive: one_sided_primitive(cull),
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH_FORMAT, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::GreaterEqual), stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_cabin_reflection"), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview_mask: None, cache: crate::pipeline_cache::get(device).as_ref(),
        }));
        let resolve_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("cabin reflection resolve"), bind_group_layouts: &[None, None, None, Some(&layout)], immediate_size: 0 });
        let resolve = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cabin reflection resolve"), layout: Some(&resolve_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_cabin_resolve"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_cabin_resolve"), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent::OVER }), write_mask: wgpu::ColorWrites::COLOR })], compilation_options: Default::default() }),
            multiview_mask: None, cache: crate::pipeline_cache::get(device).as_ref(),
        });
        Self { layout, compose, resolve, overlay: None, captures: Vec::new(), size: (0, 0) }
    }

    fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, plane_layout: &wgpu::BindGroupLayout, f: &FrameCtx, plan: &Plan) {
        let scale = (640.0 / f.width as f32).min(360.0 / f.height as f32).min(0.5);
        let size = ((f.width as f32 * scale).ceil().max(1.0) as u32, (f.height as f32 * scale).ceil().max(1.0) as u32);
        if self.size != size { self.captures.clear(); self.size = size; }
        self.captures.truncate(plan.planes.len());
        while self.captures.len() < plan.planes.len() {
            let texture = |format| device.create_texture(&wgpu::TextureDescriptor { label: Some("cabin reflection"), size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] }).create_view(&Default::default());
            let color = texture(HDR_FORMAT);
            let depth = texture(DEPTH_FORMAT);
            let buffer = |size| device.create_buffer(&wgpu::BufferDescriptor { label: Some("cabin reflection parameters"), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let plane = buffer(std::mem::size_of::<puddles::VehicleUniform>() as u64);
            let plane_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("cabin mirror plane"), layout: plane_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: plane.as_entire_binding() }] });
            let params = buffer(std::mem::size_of::<Uniform>() as u64);
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
            let picture_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("cabin picture"), layout: &self.layout, entries: &[wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&color) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) }, wgpu::BindGroupEntry { binding: 3, resource: params.as_entire_binding() }] });
            self.captures.push(Capture { color, depth, plane, plane_bg, params, picture_bg });
        }
        let mut planes = [[0.0; 4]; PLANES];
        for (i, p) in plan.planes.iter().enumerate() { planes[i] = p.to_array(); }
        for (i, c) in self.captures.iter().enumerate() {
            let mut u: puddles::VehicleUniform = bytemuck::Zeroable::zeroed();
            u.plane = planes[i];
            u.cabin_fade = DISTANCE_FADE;
            queue.write_buffer(&c.plane, 0, bytemuck::bytes_of(&u));
            queue.write_buffer(&c.params, 0, bytemuck::bytes_of(&Uniform { planes, screen: [f.width as f32, f.height as f32, i as f32, plan.planes.len() as f32] }));
        }
        if self.overlay.as_ref().is_none_or(|(v, _)| v.texture().width() != f.width || v.texture().height() != f.height) {
            let view = device.create_texture(&wgpu::TextureDescriptor { label: Some("cabin reflection on panes"), size: wgpu::Extent3d { width: f.width, height: f.height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: HDR_FORMAT, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] }).create_view(&Default::default());
            let sampler = device.create_sampler(&Default::default());
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("cabin reflection resolve"), layout: &self.layout, entries: &[wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) }, wgpu::BindGroupEntry { binding: 3, resource: self.captures[0].params.as_entire_binding() }] });
            self.overlay = Some((view, bg));
        }
    }
}

impl Renderer {
    pub(crate) fn upload_cabin_planes(&self, plan: Option<&Plan>) {
        if let Some(plan) = plan {
            let mut planes = [[0.0; 4]; PLANES];
            for (i, p) in plan.planes.iter().enumerate() { planes[i] = p.to_array(); }
            self.queue.write_buffer(&self.camera_buf, std::mem::offset_of!(CameraUniform, cabin_planes) as u64, bytemuck::cast_slice(&planes));
        }
    }

    pub(crate) fn encode_cabin_reflections(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, plan: Option<&Plan>, timers: &mut passes::PassTimers) {
        let Some(pipes) = self.cabin_reflections.as_mut() else { return };
        let Some(plan) = plan else {
            // A mirror is rendered before the main view; do not throw away the main
            // view's targets at every mirror. Release them when leaving the bus.
            if f.with_overlays || f.xr_view { pipes.captures.clear(); pipes.overlay = None; }
            return;
        };
        let Some(vehicle) = self.puddles.as_ref() else { return };
        let Some(ao) = self.targets.ao.as_ref() else { return };
        pipes.prepare(&self.device, &self.queue, &vehicle.vehicle[0].get_bind_group_layout(2), f, plan);
        for c in &pipes.captures {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cabin reflected geometry"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &c.color, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &c.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "cabin capture"), occlusion_query_set: None, multiview_mask: None,
            });
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
            pass.set_bind_group(2, &c.plane_bg, &[]);
            encode_batches(&mut pass, scene, &plan.cabin, |pipe| &vehicle.vehicle[pipe as usize]);
        }
        let (overlay, resolve_bg) = pipes.overlay.as_ref().unwrap();
        // Replace into one picture first: a pane with coincident front/back faces
        // or two authored layers must reflect the cabin only once.
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cabin reflected in glass"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: overlay, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &ao.depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "cabin glass"), occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
        for c in &pipes.captures {
            pass.set_bind_group(2, &c.plane_bg, &[]);
            pass.set_bind_group(3, &c.picture_bg, &[]);
            encode_batches(&mut pass, scene, &plan.panes, |pipe| &pipes.compose[(pipe % 4 / 2) as usize]);
        }
        drop(pass);
        let hdr = &self.targets.hdr_targets[&(f.width, f.height)];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cabin reflection resolve"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &hdr.view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "cabin resolve"), occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&pipes.resolve);
        pass.set_bind_group(3, resolve_bg, &[]);
        pass.draw(0..3, 0..1);
    }
}
