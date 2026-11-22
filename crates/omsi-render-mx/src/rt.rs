//! Enhanced+: ray tracing with the device's hardware ray queries.
//!
//! Every solid and alpha-tested mesh within `RT_RANGE` of the camera is put into an
//! acceleration structure each frame (a bottom-level structure per mesh and set of drawn
//! material slots, kept while it is used; a top-level one with the instances, built anew).
//! The window's picture then traces, per pixel of the depth prepass, the sun's shadow and
//! the sky's occlusion (rt.wgsl); the main pass takes them in place of the shadow map and
//! the screen-space AO where they are there. What a hit looks like - for the reflections -
//! comes from a record per geometry: its material's colour and textures, which the hit reads
//! from small copies of them at its triangle's texture coordinates (rt_tex.rs), or their mean
//! colours until those are made, and its corners' normals, by which it is lit.
//!
//! After the main pass the reflections are traced (rt_reflect.wgsl): a ray per pixel along a
//! direction of the surface's GGX lobe, new every frame, and denoised over the frames and the
//! pixels round it (rt_denoise.wgsl) before the composite adds them, weighted by the main
//! pass's Fresnel and lobe integral, in place of the probe's reflection.
//!
//! OMSI_NO_RT=1 opens no ray queries (Enhanced+ draws as Enhanced), OMSI_NO_RT_FRAME=1 keeps
//! them but traces nothing, OMSI_NO_RT_GRADE=1 leaves out Enhanced+'s grade, and
//! OMSI_RT_REFL_HALF=1 traces the reflections at half size. OMSI_DEBUG_RT=1 logs the
//! structures now and then; its numbers show: 3 cut-out meshes as solid, 4 cut-out meshes
//! as clear, 7 the rays unfiltered, 8 / 9 / 10 the reflections' rays and weights (OMSI_DEBUG_ENHANCED=1 and 2 show the traced shadow and occlusion).

use super::*;

/// How far from the camera meshes are traced (m): beyond it the shadow map holds.
pub(super) const RT_RANGE: f32 = 420.0;
/// How far the ambient occlusion looks (m).
const AO_RADIUS: f32 = 2.2;
/// Bottom-level structures built (or rebuilt) per frame at most: a map tile streamed in
/// brings hundreds of meshes; they come into the shadows over a few frames instead of a hitch.
const BLAS_BUDGET: usize = 320;
/// Texture means worked out per frame at most.
pub(super) const AVG_BUDGET: usize = 128;
/// Frames a pixel's rays are accumulated over at most.
const HISTORY: f32 = 24.0;
const NO_TEX: u32 = u32::MAX;

#[derive(Clone, PartialEq, Eq, Hash)]
struct BlasKey {
    vb: mx::Buffer,
    ib: mx::Buffer,
    /// The mesh (its `generation`) and where it lies in its page's buffers.
    generation: u64,
    first_vertex: u32,
    first_index: u32,
    vertex_count: u32,
    /// The mesh's ranges traced as solid, and as alpha-tested (a bit per range).
    solid: u64,
    cut: u64,
    /// ... and as see-through panes (the sun's shadow only: they let part of it through)
    glass: u64,
}

/// A `BlasKey` made of borrowed buffers, to look one up without cloning them (hashed the
/// same: the same fields in the same order).
#[derive(Hash)]
struct BlasKeyRef<'a> {
    vb: &'a mx::Buffer,
    ib: &'a mx::Buffer,
    generation: u64,
    first_vertex: u32,
    first_index: u32,
    vertex_count: u32,
    solid: u64,
    cut: u64,
    glass: u64,
}

impl hashbrown::Equivalent<BlasKey> for BlasKeyRef<'_> {
    fn equivalent(&self, k: &BlasKey) -> bool {
        *self.vb == k.vb
            && *self.ib == k.ib
            && (
                self.generation,
                self.first_vertex,
                self.first_index,
                self.vertex_count,
                self.solid,
                self.cut,
                self.glass,
            ) == (
                k.generation,
                k.first_vertex,
                k.first_index,
                k.vertex_count,
                k.solid,
                k.cut,
                k.glass,
            )
    }
}

impl BlasKeyRef<'_> {
    fn to_key(&self) -> BlasKey {
        BlasKey {
            vb: self.vb.clone(),
            ib: self.ib.clone(),
            generation: self.generation,
            first_vertex: self.first_vertex,
            first_index: self.first_index,
            vertex_count: self.vertex_count,
            solid: self.solid,
            cut: self.cut,
            glass: self.glass,
        }
    }
}

/// What a kept bind group was made with: `RayTracer::bind_gen` and the textures it names.
type RtBindKey = (u64, Vec<mx::TextureView>);

/// A top-level instance as last built: its structure, transform, record base and mask.
type TlasItem = (mx::Blas, [f32; 12], u32, u8);

struct BlasEntry {
    blas: mx::Blas,
    sizes: Vec<mx::BlasTriangleGeometrySizeDescriptor>,
    /// (first index, index count) of each geometry, in the order of `ranges`.
    spans: Vec<(u32, u32)>,
    mesh: MeshId,
    built: bool,
    used: u64,
}

/// What a traced geometry looks like (rt.wgsl `Record`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Record {
    tex: u32,
    trans: u32,
    flags: u32,
    uvs: u32,
    color: [f32; 4],
    uvt: [f32; 4],
}

/// The size of the shaders' `Record` (for the shader test).
#[cfg(test)]
pub(super) const RECORD_SIZE: usize = std::mem::size_of::<Record>();

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    prev_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    sun: [f32; 4],
    size: [f32; 4],
    proj: [f32; 4],
    temporal: [f32; 4],
    vehicle_now: [f32; 4],
    vehicle_prev: [f32; 4],
    vehicle_box: [f32; 4],
    vehicle_centre: [f32; 4],
    sky: [f32; 4],
    sun_light: [f32; 4],
}

/// The traced lighting's textures for one picture size (all rgba16float: r the ambient
/// occlusion, g the view depth it was traced at, b the sun's visibility (< 0: not traced),
/// a the frames accumulated).
pub(super) struct Targets {
    pub(super) size: (u32, u32),
    raw: mx::TextureView,
    tmp: mx::TextureView,
    /// What the main pass reads (camera bind group binding 8 in place of the SSAO).
    pub(super) out: mx::TextureView,
}

pub(super) struct RayTracer {
    blas: hashbrown::HashMap<BlasKey, BlasEntry>,
    /// Meshes whose vertices changed (skinned people): their structures are built again.
    dirty: std::cell::RefCell<std::collections::HashSet<MeshId>>,
    tlas: mx::Tlas,
    tlas_cap: u32,
    tlas_used: usize,
    /// The instances the top level was last built with (`tlas_built`): a frame with the same
    /// ones, and no structure built again, keeps it as it is.
    tlas_last: Vec<TlasItem>,
    tlas_built: bool,
    /// What the records buffer holds.
    records_sent: Vec<Record>,
    /// Bumped whenever a buffer or the top level the bind groups name is made anew; the
    /// lighting's and the reflections' bind groups are kept while it and their textures stay.
    bind_gen: u64,
    trace_groups: Option<(RtBindKey, [mx::BindGroup; 2])>,
    refl_groups: Option<(RtBindKey, [mx::BindGroup; 2])>,
    records: Vec<Record>,
    records_buf: mx::Buffer,
    tex_avg: mx::Buffer,
    tex_gen: Vec<u64>,
    /// The textures' small copies and the meshes' texture coordinates, for the reflections.
    tex: rt_tex::TexStore,
    avg_stride: u64,
    avg_pipeline: mx::ComputePipeline,
    trace: mx::ComputePipeline,
    temporal: mx::ComputePipeline,
    params: mx::Buffer,
    pub(super) targets: Option<Targets>,
    frame: u64,
    /// Last frame's view-projection, render origin and player vehicle (origin, heading deg).
    prev: Option<(Mat4, DVec3, Option<(DVec3, f64)>)>,
    /// Which structures to build this frame.
    to_build: Vec<BlasKey>,
    pub(super) instances: usize,
    /// The reflections: their layout, the rays, their history and the composite.
    reflect: mx::ComputePipeline,
    composite: mx::RenderPipeline,
    /// The reflections' rays (one per `refl_div()` x `refl_div()` pixels) and their denoiser's
    /// textures.
    refl: Option<((u32, u32), ReflTargets)>,
    /// 1 x 1 storage textures for the outputs a pass leaves unused (one per binding).
    spare: [mx::TextureView; 3],
    /// The reflections' denoiser (rt_denoise.wgsl): its passes, the a-trous steps (a uniform
    /// per pass, `step_stride` apart) and its bind groups.
    temporal_refl: mx::ComputePipeline,
    atrous: mx::ComputePipeline,
    steps: mx::Buffer,
    step_stride: u64,
    denoise_groups: Option<(RtBindKey, [mx::BindGroup; 8])>,
}

/// The reflections' textures at the rays' size (all rgba16float).
struct ReflTargets {
    /// This frame's rays: rgb what each brings back, a how far it went.
    raw: mx::TextureView,
    /// The surface each left: its distance, roughness, normal (rt_reflect.wgsl `t_meta_out`).
    meta: mx::TextureView,
    /// The history (rgb, frames in it) and its moments (the luminance's two, the view depth it
    /// was taken at, the variance), this frame's and last frame's by turns.
    hist: [mx::TextureView; 2],
    moments: [mx::TextureView; 2],
    /// The a-trous passes' two textures by turns; the last pass leaves the result in `a`.
    a: mx::TextureView,
    b: mx::TextureView,
}

/// One reflection ray per this many pixels each way (OMSI_RT_REFL_HALF=1: per 2 x 2).
fn refl_div() -> u32 {
    if omsi_cfg::flags::OMSI_RT_REFL_HALF.is_set() {
        2
    } else {
        1
    }
}

/// The traced lighting's shader: the shared ray tracing and its own.
pub(super) fn lighting_source() -> String {
    [
        crate::shaders::hot_wgsl(include_str!("rt_common.wgsl"), "rt_common.wgsl"),
        crate::shaders::hot_wgsl(include_str!("rt.wgsl"), "rt.wgsl"),
    ]
    .join("\n")
}

/// The size of the shaders' `RtParams` (for the shader test).
#[cfg(test)]
pub(super) const PARAMS_SIZE: usize = std::mem::size_of::<Params>();

/// The reflections' denoiser's shader: the shared ray tracing's parameters and its own.
pub(super) fn denoise_source() -> String {
    [
        crate::shaders::hot_wgsl(include_str!("rt_common.wgsl"), "rt_common.wgsl"),
        crate::shaders::hot_wgsl(include_str!("rt_denoise.wgsl"), "rt_denoise.wgsl"),
    ]
    .join("\n")
}

/// The reflections' shader: the shared ray tracing, the enhanced uniform's layout, its own.
pub(super) fn reflect_source() -> String {
    let common =
        crate::shaders::hot_wgsl(include_str!("enhanced_common.wgsl"), "enhanced_common.wgsl");
    let enh = &common[..common
        .find("@group(0) @binding(11)")
        .expect("Enhanced struct")];
    [
        crate::shaders::hot_wgsl(include_str!("rt_common.wgsl"), "rt_common.wgsl"),
        enh,
        crate::shaders::hot_wgsl(include_str!("rt_reflect.wgsl"), "rt_reflect.wgsl"),
    ]
    .join("\n")
}

impl RayTracer {
    pub(super) fn new(device: &mx::Device) -> RayTracer {
        let module = device
            .create_shader_module(&mx::ShaderModuleDescriptor {
                label: Some("ray tracing"),
                source: &*(lighting_source()),
            })
            .expect("mX: a shader");
        let pipeline = |entry: &str, step: Option<f64>| {
            let constants: Vec<(&str, f64)> = step.map(|s| vec![("STEP", s)]).unwrap_or_default();
            device
                .create_compute_pipeline(&mx::ComputePipelineDescriptor {
                    label: Some(entry),
                    module: &module,
                    entry_point: Some(entry),
                    constants: &constants,
                })
                .expect("mX: a compute pipeline")
        };
        let trace = pipeline("cs_trace", None);
        let temporal = pipeline("cs_denoise", None);
        let avg_stride = (device.limits().min_storage_buffer_offset_alignment as u64).max(256);
        let avg_module = device
            .create_shader_module(&mx::ShaderModuleDescriptor {
                label: Some("texture means"),
                source: crate::shaders::hot_wgsl(include_str!("rt_avg.wgsl"), "rt_avg.wgsl"),
            })
            .expect("mX: a shader");
        let avg_pipeline = device
            .create_compute_pipeline(&mx::ComputePipelineDescriptor {
                label: Some("texture means"),
                module: &avg_module,
                entry_point: Some("cs_tex_avg"),
                constants: &[],
            })
            .expect("mX: a compute pipeline");
        let refl_module = device
            .create_shader_module(&mx::ShaderModuleDescriptor {
                label: Some("ray-traced reflections"),
                source: &*(reflect_source()),
            })
            .expect("mX: a shader");
        let div = [("REFL_DIV", refl_div() as f64)];
        let refl_compute = |entry: &str| {
            device
                .create_compute_pipeline(&mx::ComputePipelineDescriptor {
                    label: Some(entry),
                    module: &refl_module,
                    entry_point: Some(entry),
                    constants: &div,
                })
                .expect("mX: a compute pipeline")
        };
        let reflect = refl_compute("cs_reflect");
        let add = mx::BlendComponent {
            src_factor: mx::BlendFactor::One,
            dst_factor: mx::BlendFactor::One,
            operation: mx::BlendOperation::Add,
        };
        let composite = device
            .create_render_pipeline(&mx::RenderPipelineDescriptor {
                label: Some("reflections composite"),
                vertex: mx::VertexState {
                    constants: &[],
                    module: &refl_module,
                    entry_point: Some("vs_full"),
                    buffers: &[],
                },
                primitive: mx::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(mx::FragmentState {
                    module: &refl_module,
                    entry_point: Some("fs_composite"),
                    targets: &[Some(mx::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: Some(mx::BlendState {
                            color: add,
                            alpha: add,
                        }),
                        write_mask: mx::ColorWrites::COLOR,
                    })],
                    constants: &div,
                }),
            })
            .expect("mX: a render pipeline");
        let denoise_module = device
            .create_shader_module(&mx::ShaderModuleDescriptor {
                label: Some("rt reflections denoiser"),
                source: &*(denoise_source()),
            })
            .expect("mX: a shader");
        let denoise_pass = |entry: &str| {
            device
                .create_compute_pipeline(&mx::ComputePipelineDescriptor {
                    label: Some(entry),
                    module: &denoise_module,
                    entry_point: Some(entry),
                    constants: &div,
                })
                .expect("mX: a compute pipeline")
        };
        let temporal_refl = denoise_pass("cs_temporal");
        let atrous = denoise_pass("cs_atrous");
        // (written with the parameters every frame, `prepare_ray_tracing`)
        let step_stride = (device.limits().min_uniform_buffer_offset_alignment as u64).max(256);
        let steps = device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("rt reflections denoiser steps"),
                size: 3 * step_stride,
                usage: mx::BufferUsages::UNIFORM | mx::BufferUsages::COPY_DST,
            })
            .expect("mX: a buffer");
        let spare = || {
            device
                .create_texture(&mx::TextureDescriptor {
                    label: Some("rt spare"),
                    size: mx::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format: mx::TextureFormat::Rgba16Float,
                    usage: mx::TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                })
                .expect("mX: a texture")
                .create_view(&Default::default())
                .expect("mX: a texture view")
        };
        let tlas_cap = 8192;
        RayTracer {
            blas: hashbrown::HashMap::new(),
            dirty: Default::default(),
            tlas: Self::make_tlas(device, tlas_cap),
            tlas_cap,
            tlas_used: 0,
            tlas_last: Vec::new(),
            tlas_built: false,
            records_sent: Vec::new(),
            bind_gen: 0,
            trace_groups: None,
            refl_groups: None,
            records: Vec::new(),
            records_buf: Self::make_records(device, 1 << 16),
            tex_avg: Self::make_avg(device, 4096, avg_stride),
            tex_gen: Vec::new(),
            tex: rt_tex::TexStore::new(device),
            avg_stride,
            avg_pipeline,
            trace,
            temporal,
            params: device
                .create_buffer(&mx::BufferDescriptor {
                    label: Some("ray tracing params"),
                    size: std::mem::size_of::<Params>() as u64,
                    usage: mx::BufferUsages::UNIFORM | mx::BufferUsages::COPY_DST,
                })
                .expect("mX: a buffer"),
            targets: None,
            frame: 0,
            prev: None,
            to_build: Vec::new(),
            instances: 0,
            reflect,
            composite,
            refl: None,
            spare: [spare(), spare(), spare()],
            temporal_refl,
            atrous,
            steps,
            step_stride,
            denoise_groups: None,
        }
    }

    fn make_tlas(device: &mx::Device, cap: u32) -> mx::Tlas {
        device
            .create_tlas(&mx::CreateTlasDescriptor {
                label: Some("ray tracing instances"),
                max_instances: cap,
                flags: mx::AccelerationStructureFlags::PREFER_FAST_BUILD,
                update_mode: mx::AccelerationStructureUpdateMode::Build,
            })
            .expect("mX: an acceleration structure")
    }

    fn make_records(device: &mx::Device, n: u64) -> mx::Buffer {
        device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("ray tracing records"),
                size: n * std::mem::size_of::<Record>() as u64,
                usage: mx::BufferUsages::STORAGE | mx::BufferUsages::COPY_DST,
            })
            .expect("mX: a buffer")
    }

    fn make_avg(device: &mx::Device, n: u64, stride: u64) -> mx::Buffer {
        device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("texture means"),
                size: n * stride,
                usage: mx::BufferUsages::STORAGE
                    | mx::BufferUsages::COPY_DST
                    | mx::BufferUsages::COPY_SRC,
            })
            .expect("mX: a buffer")
    }

    /// The vertices of these meshes changed: their structures are built again.
    pub(super) fn meshes_changed(&self, ids: impl IntoIterator<Item = MeshId>) {
        self.dirty.borrow_mut().extend(ids);
    }

    /// The traced lighting's textures for this size; true when they were made anew (the
    /// camera bind group must then point at the new one).
    pub(super) fn ensure_targets(&mut self, device: &mx::Device, w: u32, h: u32) -> bool {
        if self.targets.as_ref().is_some_and(|t| t.size == (w, h)) {
            return false;
        }
        let mk = |label: &str| {
            device
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
                    format: mx::TextureFormat::Rgba16Float,
                    usage: mx::TextureUsages::STORAGE_BINDING
                        | mx::TextureUsages::TEXTURE_BINDING
                        | mx::TextureUsages::COPY_SRC
                        | mx::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
                .expect("mX: a texture")
                .create_view(&Default::default())
                .expect("mX: a texture view")
        };
        self.targets = Some(Targets {
            size: (w, h),
            raw: mk("rt rays"),
            tmp: mk("rt filter"),
            out: mk("rt lighting"),
        });
        self.prev = None;
        true
    }
}

impl Renderer {
    /// Which of an instance's mesh ranges are traced, solid or cut out (a bit per range), as
    /// its materials draw them; blended ones (glass, decals, dirt) are not.
    fn traced_ranges(scene: &Scene, inst: &Instance) -> (u64, u64, u64) {
        let m = &scene.meshes[inst.mesh];
        let (mut solid, mut cut, mut glass) = (0u64, 0u64, 0u64);
        for (ri, (_, count, slot)) in m.ranges.iter().enumerate().take(64) {
            if *count < 3 {
                continue;
            }
            let Some(mat) = inst
                .materials
                .get(*slot as usize)
                .and_then(|&id| scene.materials.get(id))
            else {
                continue;
            };
            if mat.no_z_check
                || inst
                    .slot_alpha
                    .get(*slot as usize)
                    .is_some_and(|a| *a < 0.01)
            {
                continue;
            }
            match mat.alpha {
                AlphaMode::Opaque => solid |= 1 << ri,
                AlphaMode::Test => cut |= 1 << ri,
                // a body whose paint is cut by its transmap writes depth: cut out as well
                AlphaMode::Blend if mat.transmap.is_some() && !mat.no_z_write => cut |= 1 << ri,
                // a pane (any other blended layer): the shadow rays' tint
                AlphaMode::Blend => glass |= 1 << ri,
            }
        }
        (solid, cut, glass)
    }

    /// The frame's ray-tracing input: the instances near the camera with their structures
    /// (made for meshes met for the first time) and the records of their geometries, and the
    /// shaders' parameters. False when there is nothing to trace with.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_ray_tracing(
        &mut self,
        scene: &Scene,
        camera: &Camera,
        lighting: &Lighting,
        view_proj: Mat4,
        proj: Mat4,
        w: u32,
        h: u32,
        dt: f32,
    ) -> bool {
        let Some(rt) = self.rt.as_mut() else {
            return false;
        };
        rt.frame += 1;
        let frame = rt.frame;
        let ro = scene.render_origin;
        let cam = (camera.position - ro).as_vec3();
        let lod_fov = camera.fov_deg.to_radians().max(1e-3);
        // structures of meshes whose vertices changed are built again
        if !rt.dirty.borrow().is_empty() {
            let dirty = std::mem::take(&mut *rt.dirty.borrow_mut());
            for e in rt.blas.values_mut() {
                if dirty.contains(&e.mesh) {
                    e.built = false;
                }
            }
        }
        rt.to_build.clear();
        rt.records.clear();
        let mut chosen: Vec<TlasItem> = Vec::new();
        // The blocks of instances wholly out of reach, passed by as the culling does (a
        // block's sphere holds each of its instances' spheres; a metre to spare for the
        // rounding): the whole map's instances were measured one by one every frame.
        let far: Vec<bool> = Self::cull_blocks(scene)
            .map(|b| {
                b.iter()
                    .map(|&(c, r)| (c - cam).length() - r > RT_RANGE + 1.0)
                    .collect()
            })
            .unwrap_or_default();
        for (k, inst) in scene.instances.iter().enumerate() {
            if far.get(k / CULL_BLOCK).copied().unwrap_or(false) {
                continue;
            }
            // (a decal - a surface object lying on the road - is no geometry for the rays,
            // but what such an object raises and casts a shadow from is: the Spandau depot's
            // halls, one object with its yard, lost their shadow close up in Enhanced+, #1783)
            if !inst.visible || inst.blob || (inst.decal && !inst.casts_shadow) || inst.ground_layer
            {
                continue;
            }
            let m = &scene.meshes[inst.mesh];
            if m.ranges.is_empty() || m.bounds_radius <= 0.0 {
                continue;
            }
            if let Some([x0, y0, x1, y1]) = inst.near_only {
                if camera.position.x < x0
                    || camera.position.x > x1
                    || camera.position.y < y0
                    || camera.position.y > y1
                {
                    continue;
                }
            }
            let (c, r) = Self::bounding_sphere(scene, inst);
            let d = (c - cam).length();
            if d - r > RT_RANGE {
                continue;
            }
            // the level of detail the picture shows
            if inst.lod.0 > 0.0 || inst.lod.1 < f32::MAX {
                let scale = Self::instance_scale(scene, inst);
                let radius = if inst.object_radius > 0.0 {
                    inst.object_radius
                } else {
                    m.bounds_radius
                } * scale;
                let distance = ((inst.origin - ro).as_vec3() - cam).length();
                let size = object_screen_size(radius, distance, lod_fov);
                if size < inst.lod.0 || (inst.lod.1 < f32::MAX && size >= inst.lod.1) {
                    continue;
                }
            }
            let (solid, cut, glass) = Self::traced_ranges(scene, inst);
            if solid | cut | glass == 0 {
                continue;
            }
            let caster =
                inst.casts_shadow && !(self.options.omsi_shadow_casters && !inst.omsi_caster);
            let seen = !inst.mirror_only;
            let t = Mat4::from_translation((inst.origin - ro).as_vec3()) * inst.transform;
            let c = t.to_cols_array();
            // (3 x 4, rows first)
            let rows = [
                c[0], c[4], c[8], c[12], c[1], c[5], c[9], c[13], c[2], c[6], c[10], c[14],
            ];
            // the solid ranges and the cut-out ones as two instances: the shadow rays take the
            // first hit of the solid ones alone, the others stop at the nearest of either
            for (bits, kind) in [(solid, 0), (cut, 1), (glass, 2)] {
                let is_cut = kind == 1;
                let mask = match kind {
                    0 => (caster as u8) | ((seen as u8) << 1),
                    1 => (seen as u8) << 2,
                    // (the panes also stop the reflections' rays: a car beside the bus mirrors
                    // its windows, not the seats behind them in the sun and the open sky)
                    _ => ((caster as u8) << 3) | ((seen as u8) << 4),
                };
                if bits == 0 || mask == 0 {
                    continue;
                }
                let key = BlasKeyRef {
                    vb: &m.vertex_buf,
                    ib: &m.index_buf,
                    generation: m.generation,
                    first_vertex: m.base_vertex.max(0) as u32,
                    first_index: m.first_index,
                    vertex_count: (m.vertex_bytes / std::mem::size_of::<Vertex>() as u64) as u32,
                    solid: if kind == 0 { bits } else { 0 },
                    cut: if kind == 1 { bits } else { 0 },
                    glass: if kind == 2 { bits } else { 0 },
                };
                let entry = match rt.blas.get_mut(&key) {
                    Some(e) => e,
                    None => {
                        if rt.to_build.len() >= BLAS_BUDGET {
                            continue;
                        }
                        let vertex_count = key.vertex_count;
                        let mut sizes = Vec::new();
                        let mut spans = Vec::new();
                        for (ri, (first, count, _)) in m.ranges.iter().enumerate().take(64) {
                            if bits & (1 << ri) == 0 {
                                continue;
                            }
                            let count = count / 3 * 3;
                            sizes.push(mx::BlasTriangleGeometrySizeDescriptor {
                                vertex_format: mx::VertexFormat::Float32x3,
                                vertex_count,
                                index_format: Some(mx::IndexFormat::Uint32),
                                index_count: Some(count),
                                flags: mx::AccelerationStructureGeometryFlags::OPAQUE,
                            });
                            spans.push((*first, count));
                        }
                        let blas = self
                            .device
                            .create_blas(
                                &mx::CreateBlasDescriptor {
                                    label: None,
                                    flags: mx::AccelerationStructureFlags::PREFER_FAST_TRACE,
                                    update_mode: mx::AccelerationStructureUpdateMode::Build,
                                },
                                &sizes,
                            )
                            .expect("mX: an acceleration structure");
                        rt.blas.entry(key.to_key()).or_insert(BlasEntry {
                            blas,
                            sizes,
                            spans,
                            mesh: inst.mesh,
                            built: false,
                            used: frame,
                        })
                    }
                };
                entry.used = frame;
                if !entry.built {
                    if rt.to_build.len() >= BLAS_BUDGET {
                        continue;
                    }
                    entry.built = true;
                    rt.to_build.push(key.to_key());
                }
                // the geometries' records, in the structure's order
                let base = rt.records.len() as u32;
                if base >= (1 << 24) - 64 {
                    break;
                }
                let uvs = rt.tex.mesh_uvs(&self.device, inst.mesh, m, frame);
                for (ri, (first, _, slot)) in m.ranges.iter().enumerate().take(64) {
                    if bits & (1 << ri) == 0 {
                        continue;
                    }
                    let Some(mat) = inst
                        .materials
                        .get(*slot as usize)
                        .and_then(|&id| scene.materials.get(id))
                    else {
                        crate::passes::batch::warn_invalid_material(
                            inst.mesh,
                            *slot,
                            inst.materials.get(*slot as usize).copied().unwrap_or(0),
                        );
                        continue;
                    };
                    let tex = mat.texture.map_or(NO_TEX, |t| t as u32);
                    let (trans, from_trans) = match mat.transmap {
                        Some((t, true)) => (t as u32, true),
                        _ => (NO_TEX, false),
                    };
                    let mut flags = 0u32;
                    if is_cut {
                        flags |= 1;
                    }
                    if mat.unlit {
                        flags |= 2;
                    }
                    if from_trans {
                        flags |= 4;
                    }
                    flags |= rt_tex::address_mode(mat.address) << 3;
                    // (a [matl_transmap] whose alpha is not taken: the slot is opaque, see the
                    // main pass)
                    if mat.uniform.params[2] > 0.5 && mat.uniform.params[3] < 0.5 {
                        flags |= 32;
                    }
                    if kind == 2 {
                        flags |= 64;
                    }
                    // (a range's triangles are the mesh's from its first on: a range that
                    // starts within a triangle has none)
                    let uvs = uvs
                        .filter(|_| first % 3 == 0)
                        .map_or(rt_tex::NO_UV, |b| b + first / 3);
                    let off = inst
                        .slot_uv
                        .get(*slot as usize)
                        .copied()
                        .unwrap_or([0.0; 2]);
                    let repeat = if mat.uniform.extra[0] > 0.5 {
                        mat.uniform.extra[2]
                    } else {
                        1.0
                    };
                    rt.records.push(Record {
                        tex,
                        trans,
                        flags,
                        uvs,
                        color: mat.color,
                        uvt: [repeat, off[0], off[1], 0.0],
                    });
                }
                chosen.push((entry.blas.clone(), rows, base, mask));
            }
        }
        // the top level: grown when the instances outnumber it
        if chosen.len() as u32 > rt.tlas_cap {
            rt.tlas_cap = (chosen.len() as u32).next_power_of_two();
            rt.tlas = RayTracer::make_tlas(&self.device, rt.tlas_cap);
            rt.tlas_used = 0;
            rt.tlas_built = false;
            rt.bind_gen += 1;
        }
        // (the same instances where they were, and no structure built again: the top level
        // is kept - a bus standing at a stop with the traffic parked round it)
        let same = |a: &TlasItem, b: &TlasItem| {
            a.0 == b.0 && a.1.map(f32::to_bits) == b.1.map(f32::to_bits) && a.2 == b.2 && a.3 == b.3
        };
        let unchanged = rt.tlas_built
            && rt.to_build.is_empty()
            && chosen.len() == rt.tlas_last.len()
            && chosen.iter().zip(&rt.tlas_last).all(|(a, b)| same(a, b));
        if !unchanged {
            for (i, (blas, rows, base, mask)) in chosen.iter().enumerate() {
                rt.tlas[i] = Some(mx::TlasInstance::new(blas.clone(), *rows, *base, *mask));
            }
            for i in chosen.len()..rt.tlas_used {
                rt.tlas[i] = None;
            }
            rt.tlas_used = chosen.len();
            rt.tlas_built = false;
        }
        rt.instances = chosen.len();
        rt.tlas_last = chosen;
        if rt.tex.grown() {
            rt.bind_gen += 1;
        }
        // the records, in a buffer grown as needed; only from the first that changed to the last
        let bytes: &[u8] = bytemuck::cast_slice(&rt.records);
        if bytes.len() as u64 > rt.records_buf.size() {
            rt.records_buf = RayTracer::make_records(
                &self.device,
                (rt.records.len() as u64).next_power_of_two(),
            );
            rt.records_sent.clear();
            rt.bind_gen += 1;
        }
        let differs = |k: &usize| {
            rt.records_sent
                .get(*k)
                .is_none_or(|r| bytemuck::bytes_of(r) != bytemuck::bytes_of(&rt.records[*k]))
        };
        if let Some(first) = (0..rt.records.len()).find(differs) {
            let last = (first..rt.records.len())
                .rev()
                .find(differs)
                .unwrap_or(first);
            let unit = std::mem::size_of::<Record>();
            self.queue
                .write_buffer(
                    &rt.records_buf,
                    (first * unit) as u64,
                    &bytes[first * unit..(last + 1) * unit],
                )
                .expect("mX: a queue write");
        }
        rt.records_sent.clone_from(&rt.records);
        // structures nobody used for a while are let go
        if frame % 240 == 0 {
            rt.blas.retain(|_, e| frame - e.used < 1200);
            rt.tex.retain(frame);
        }
        if omsi_cfg::flags::OMSI_DEBUG_RT.is_set() && frame % 30 == 1 {
            log::info!(
                "ray tracing: {} instances, {} geometries, {} structures ({} built this frame)",
                rt.instances,
                rt.records.len(),
                rt.blas.len(),
                rt.to_build.len()
            );
        }
        // --- the parameters
        let sun = lighting.sun_dir.normalize_or_zero();
        let sun_on = lighting.casts_sun_shadows();
        let vehicle = lighting.inside.map(|(o, h, _)| (o, h));
        let (prev_vp, history) = match rt.prev {
            Some((vp, prev_ro, _)) => (
                vp * Mat4::from_translation((ro - prev_ro).as_vec3()),
                dt < 3.0,
            ),
            None => (view_proj, false),
        };
        let vrel = |o: DVec3| (o - ro).as_vec3();
        let (vehicle_now, vehicle_prev, vehicle_box, vehicle_centre) =
            match (lighting.inside, rt.prev.and_then(|p| p.2)) {
                (Some((o, hdg, bb)), Some((po, ph))) => (
                    vrel(o).extend((hdg as f32).to_radians()).to_array(),
                    vrel(po).extend((ph as f32).to_radians()).to_array(),
                    [bb[0] * 0.5, bb[1] * 0.5, bb[2] * 0.5, 1.0],
                    [bb[3], bb[4], bb[5], 0.0],
                ),
                _ => ([0.0; 4], [0.0; 4], [0.0; 4], [0.0; 4]),
            };
        let debug = omsi_cfg::flags::OMSI_DEBUG_RT.parse::<f32>().unwrap_or(0.0);
        // The sun's disc as the shadow rays see it (tangent of its radius): its own 0.27
        // degrees, so that a shadow is as sharp at its caster's foot and as soft away from it
        // as the real one; a veil of high cloud spreads the sun into an aureole a few degrees
        // wide, and the shadows widen with it (as the shadow map's, `sun_shadow_soft`).
        let veil_spread = 1.0 - (-lighting.veil.clamp(0.0, 3.0) / sun.z.max(0.03)).exp();
        let cone = SUN_RADIUS.tan() * (1.0 + 3.0 * veil_spread);
        let p = Params {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            prev_view_proj: prev_vp.to_cols_array_2d(),
            eye: cam.extend((frame % 4096) as f32).to_array(),
            sun: sun.extend(if sun_on { cone } else { 0.0 }).to_array(),
            size: [
                w as f32,
                h as f32,
                1.0 / w.max(1) as f32,
                1.0 / h.max(1) as f32,
            ],
            proj: [proj.z_axis.z, proj.w_axis.z, RT_RANGE, AO_RADIUS],
            temporal: [history as u8 as f32, 0.06, debug, HISTORY],
            vehicle_now,
            vehicle_prev,
            vehicle_box,
            vehicle_centre,
            sky: [0.0; 4],
            sun_light: [0.0; 4],
        };
        self.queue
            .write_buffer(&rt.params, 0, bytemuck::bytes_of(&p))
            .expect("mX: a queue write");
        // the reflections' a-trous passes' steps, 1, 2 and 4 rays apart (the first takes the
        // variance from the moments; the temporal pass reads the first and ignores it)
        let mut steps = vec![0u8; 3 * rt.step_stride as usize];
        for (k, step) in [
            [1.0f32, 1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0, 0.0],
        ]
        .iter()
        .enumerate()
        {
            let at = k * rt.step_stride as usize;
            steps[at..at + 16].copy_from_slice(bytemuck::cast_slice(step));
        }
        self.queue
            .write_buffer(&rt.steps, 0, &steps)
            .expect("mX: a queue write");
        rt.prev = Some((view_proj, ro, vehicle));
        true
    }

    /// The mean colours of the textures the records name that have none yet (or were replaced).
    fn encode_texture_means(&mut self, encoder: &mut mx::CommandEncoder, scene: &Scene) {
        let Some(rt) = self.rt.as_mut() else { return };
        if scene.textures.len() > rt.tex_gen.len() {
            rt.tex_gen.resize(scene.textures.len(), 0);
        }
        let need = (scene.textures.len() as u64) * rt.avg_stride;
        if need > rt.tex_avg.size() {
            // (grown: the means made so far are copied over)
            let bigger = RayTracer::make_avg(
                &self.device,
                (scene.textures.len() as u64).next_power_of_two(),
                rt.avg_stride,
            );
            encoder.copy_buffer_to_buffer(&rt.tex_avg, 0, &bigger, 0, rt.tex_avg.size());
            rt.tex_avg = bigger;
            rt.bind_gen += 1;
        }
        let mut wanted: Vec<usize> = Vec::new();
        for r in &rt.records {
            for t in [r.tex, r.trans] {
                if t == NO_TEX {
                    continue;
                }
                let t = t as usize;
                if t < scene.textures.len()
                    && rt.tex_gen[t] != scene.textures[t].generation
                    && !wanted.contains(&t)
                {
                    wanted.push(t);
                    if wanted.len() >= AVG_BUDGET {
                        break;
                    }
                }
            }
            if wanted.len() >= AVG_BUDGET {
                break;
            }
        }
        if wanted.is_empty() {
            return;
        }
        if rt.tex.ensure_atlas(&self.device, encoder, &wanted) {
            rt.bind_gen += 1;
        }
        rt.tex.write_tiles(&self.queue, &wanted);
        let groups: Vec<mx::BindGroup> = wanted
            .iter()
            .enumerate()
            .map(|(k, &t)| {
                let [atlas, tile] = rt.tex.avg_bindings(k);
                self.device.create_bind_group(&mx::BindGroupDescriptor {
                    label: Some("texture mean"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: mx::BindingResource::TextureView(&scene.textures[t].view),
                        },
                        mx::BindGroupEntry {
                            binding: 1,
                            resource: mx::BindingResource::Sampler(&self.tone.lin_sampler),
                        },
                        mx::BindGroupEntry {
                            binding: 2,
                            resource: mx::BindingResource::Buffer(mx::BufferBinding {
                                buffer: &rt.tex_avg,
                                offset: t as u64 * rt.avg_stride,
                                size: Some(32),
                            }),
                        },
                        atlas,
                        tile,
                    ],
                })
            })
            .collect();
        {
            let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
                label: Some("texture means"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&rt.avg_pipeline);
            for bg in &groups {
                pass.set_bind_group(0, bg);
                pass.dispatch_workgroups(1, 1, 1);
            }
        }
        for &t in &wanted {
            rt.tex_gen[t] = scene.textures[t].generation;
        }
    }

    /// Build this frame's structures and trace the lighting into `Targets::out`, after the
    /// depth prepass.
    pub(super) fn encode_ray_tracing(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        queries: Option<&mx::QuerySet>,
        timed: &mut Vec<&'static str>,
    ) {
        self.encode_texture_means(encoder, scene);
        let Some(rt) = self.rt.as_mut() else { return };
        rt.tex.encode_uvs(&self.device, &self.queue, encoder);
        let Some(depth) = self.targets.ao.as_ref().map(|a| &a.depth_view) else {
            return;
        };
        let Some(t) = rt.targets.as_ref() else { return };
        if !rt.to_build.is_empty() || !rt.tlas_built {
            let entries: Vec<mx::BlasBuildEntry> = rt
                .to_build
                .iter()
                .map(|k| {
                    let e = &rt.blas[k];
                    mx::BlasBuildEntry {
                        blas: &e.blas,
                        geometry: e
                            .sizes
                            .iter()
                            .zip(&e.spans)
                            .map(|(size, (first, _))| mx::BlasTriangleGeometry {
                                size,
                                vertex_buffer: &k.vb,
                                first_vertex: k.first_vertex,
                                vertex_stride: std::mem::size_of::<Vertex>() as u64,
                                index_buffer: Some(&k.ib),
                                first_index: Some(k.first_index + *first),
                            })
                            .collect(),
                    }
                })
                .collect();
            encoder.build_acceleration_structures(entries.iter(), std::iter::once(&rt.tlas));
            rt.tlas_built = true;
        }
        let group = |inp: &mx::TextureView, raw: &mx::TextureView, out: &mx::TextureView| {
            self.device.create_bind_group(&mx::BindGroupDescriptor {
                label: Some("ray tracing"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: rt.params.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::AccelerationStructure(&rt.tlas),
                    },
                    mx::BindGroupEntry {
                        binding: 2,
                        resource: mx::BindingResource::TextureView(depth),
                    },
                    mx::BindGroupEntry {
                        binding: 3,
                        resource: rt.records_buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 4,
                        resource: rt.tex_avg.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 5,
                        resource: mx::BindingResource::TextureView(inp),
                    },
                    mx::BindGroupEntry {
                        binding: 6,
                        resource: mx::BindingResource::TextureView(raw),
                    },
                    mx::BindGroupEntry {
                        binding: 7,
                        resource: mx::BindingResource::TextureView(out),
                    },
                    mx::BindGroupEntry {
                        binding: 8,
                        resource: mx::BindingResource::Sampler(&self.tone.lin_sampler),
                    },
                ],
            })
        };
        // (each pass's input never is its output)
        let key: RtBindKey = (
            rt.bind_gen,
            vec![depth.clone(), t.tmp.clone(), t.raw.clone(), t.out.clone()],
        );
        if rt.trace_groups.as_ref().is_none_or(|g| g.0 != key) {
            rt.trace_groups = Some((
                key,
                [group(&t.tmp, &t.tmp, &t.raw), group(&t.raw, &t.tmp, &t.out)],
            ));
        }
        let [trace_bg, denoise_bg] = &rt.trace_groups.as_ref().expect("made above").1;
        let (gx, gy) = (t.size.0.div_ceil(8), t.size.1.div_ceil(8));
        for (label, pipe, bg) in [
            ("rt trace", &rt.trace, trace_bg),
            ("rt denoise", &rt.temporal, denoise_bg),
        ] {
            let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
                label: Some(label),
                timestamp_writes: pass_timer(queries, timed, label).map(|t| {
                    mx::PassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, bg);
            pass.dispatch_workgroups(gx, gy, 1);
        }
    }
}

impl Renderer {
    /// The ray-traced reflections of the window's picture, added to `HdrTargets::view` after
    /// the main pass (and before the rain on the glass, which looks through it).
    pub(super) fn encode_rt_reflections(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        w: u32,
        h: u32,
        queries: Option<&mx::QuerySet>,
        timed: &mut Vec<&'static str>,
    ) {
        let (Some(rt), Some(hdr), Some(ao), Some(probe)) = (
            self.rt.as_mut(),
            self.targets.hdr_targets.get(&(w, h)),
            self.targets.ao.as_ref(),
            self.tone.probe.as_ref(),
        ) else {
            return;
        };
        let Some(gbuf) = hdr.gbuf.as_ref() else {
            return;
        };
        let half = (w.div_ceil(refl_div()), h.div_ceil(refl_div()));
        if rt.refl.as_ref().is_none_or(|r| r.0 != half) {
            let mk = |label: &str| {
                self.device
                    .create_texture(&mx::TextureDescriptor {
                        label: Some(label),
                        size: mx::Extent3d {
                            width: half.0,
                            height: half.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: mx::TextureDimension::D2,
                        format: mx::TextureFormat::Rgba16Float,
                        usage: mx::TextureUsages::STORAGE_BINDING
                            | mx::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .expect("mX: a texture")
                    .create_view(&Default::default())
                    .expect("mX: a texture view")
            };
            rt.refl = Some((
                half,
                ReflTargets {
                    raw: mk("rt reflection rays"),
                    meta: mk("rt reflection surfaces"),
                    hist: [mk("rt reflection history"), mk("rt reflection history")],
                    moments: [mk("rt reflection moments"), mk("rt reflection moments")],
                    a: mk("rt reflection filter"),
                    b: mk("rt reflection filter"),
                },
            ));
        }
        let (_, rtg) = rt.refl.as_ref().unwrap();
        let raw = &rtg.raw;
        let dummy = &self.black_texture.view;
        let group = |out: &mx::TextureView,
                     scene: &mx::TextureView,
                     h12: &mx::TextureView,
                     meta_out: &mx::TextureView,
                     meta_in: &mx::TextureView| {
            self.device.create_bind_group(&mx::BindGroupDescriptor {
                label: Some("ray-traced reflections"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: rt.params.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::AccelerationStructure(&rt.tlas),
                    },
                    mx::BindGroupEntry {
                        binding: 2,
                        resource: mx::BindingResource::TextureView(&ao.depth_view),
                    },
                    mx::BindGroupEntry {
                        binding: 3,
                        resource: rt.records_buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 4,
                        resource: rt.tex_avg.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 5,
                        resource: mx::BindingResource::TextureView(&gbuf[0].1),
                    },
                    mx::BindGroupEntry {
                        binding: 6,
                        resource: mx::BindingResource::TextureView(&gbuf[1].1),
                    },
                    mx::BindGroupEntry {
                        binding: 7,
                        resource: mx::BindingResource::TextureView(out),
                    },
                    mx::BindGroupEntry {
                        binding: 8,
                        resource: mx::BindingResource::Sampler(&self.tone.lin_sampler),
                    },
                    mx::BindGroupEntry {
                        binding: 9,
                        resource: mx::BindingResource::TextureView(scene),
                    },
                    mx::BindGroupEntry {
                        binding: 10,
                        resource: mx::BindingResource::TextureView(&probe.view),
                    },
                    mx::BindGroupEntry {
                        binding: 11,
                        resource: self.tone.enh_buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 12,
                        resource: mx::BindingResource::TextureView(h12),
                    },
                    mx::BindGroupEntry {
                        binding: 13,
                        resource: rt.tex.uvs.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 14,
                        resource: mx::BindingResource::TextureView(&rt.tex.atlas_view),
                    },
                    mx::BindGroupEntry {
                        binding: 15,
                        resource: mx::BindingResource::TextureView(meta_out),
                    },
                    mx::BindGroupEntry {
                        binding: 16,
                        resource: mx::BindingResource::TextureView(meta_in),
                    },
                ],
            })
        };
        // (no pass's output is among its inputs)
        let key: RtBindKey = (
            rt.bind_gen,
            vec![
                ao.depth_view.clone(),
                gbuf[0].1.clone(),
                gbuf[1].1.clone(),
                raw.clone(),
                rtg.meta.clone(),
                rtg.a.clone(),
                hdr.view.clone(),
                probe.view.clone(),
                dummy.clone(),
            ],
        );
        if rt.refl_groups.as_ref().is_none_or(|g| g.0 != key) {
            rt.refl_groups = Some((
                key,
                [
                    group(raw, &hdr.view, dummy, &rtg.meta, dummy),
                    group(&rt.spare[0], dummy, &rtg.a, &rt.spare[1], &rtg.meta),
                ],
            ));
        }
        let [trace_bg, composite_bg] = &rt.refl_groups.as_ref().expect("made above").1;
        // The denoiser's passes (rt_denoise.wgsl): the temporal one by this frame's parity
        // (last frame's history in, this frame's out), then the three a-trous ones, each with
        // its step's uniform.
        let dgroup = |cur: &mx::TextureView,
                      prev: &mx::TextureView,
                      prev_m: &mx::TextureView,
                      out: &mx::TextureView,
                      out_m: &mx::TextureView,
                      step: u64| {
            let view = |binding, v| mx::BindGroupEntry {
                binding,
                resource: mx::BindingResource::TextureView(v),
            };
            self.device.create_bind_group(&mx::BindGroupDescriptor {
                label: Some("rt reflections denoiser"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: rt.params.as_entire_binding(),
                    },
                    view(5, &rtg.meta),
                    view(6, cur),
                    view(7, prev),
                    view(8, prev_m),
                    view(9, out),
                    view(10, out_m),
                    mx::BindGroupEntry {
                        binding: 11,
                        resource: mx::BindingResource::Buffer(mx::BufferBinding {
                            buffer: &rt.steps,
                            offset: step * rt.step_stride,
                            size: Some(16),
                        }),
                    },
                ],
            })
        };
        let dkey: RtBindKey = (
            rt.bind_gen,
            vec![
                raw.clone(),
                rtg.meta.clone(),
                rtg.hist[0].clone(),
                rtg.moments[0].clone(),
                rtg.a.clone(),
            ],
        );
        if rt.denoise_groups.as_ref().is_none_or(|g| g.0 != dkey) {
            let (h, m, sp) = (&rtg.hist, &rtg.moments, &rt.spare[2]);
            rt.denoise_groups = Some((
                dkey,
                [
                    dgroup(raw, &h[1], &m[1], &h[0], &m[0], 0),
                    dgroup(raw, &h[0], &m[0], &h[1], &m[1], 0),
                    dgroup(&h[0], raw, &m[0], &rtg.a, sp, 0),
                    dgroup(&h[1], raw, &m[1], &rtg.a, sp, 0),
                    dgroup(&rtg.a, raw, &m[0], &rtg.b, sp, 1),
                    dgroup(&rtg.b, raw, &m[0], &rtg.a, sp, 2),
                    dgroup(&rtg.a, raw, &m[1], &rtg.b, sp, 1),
                    dgroup(&rtg.b, raw, &m[1], &rtg.a, sp, 2),
                ],
            ));
        }
        let dg = &rt.denoise_groups.as_ref().expect("made above").1;
        let parity = (rt.frame % 2) as usize;
        {
            let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
                label: Some("rt reflections"),
                timestamp_writes: pass_timer(queries, timed, "rt reflections").map(|t| {
                    mx::PassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            let (gx, gy) = (half.0.div_ceil(8), half.1.div_ceil(8));
            pass.set_pipeline(&rt.reflect);
            pass.set_bind_group(0, trace_bg);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
                label: Some("rt reflections denoiser"),
                timestamp_writes: pass_timer(queries, timed, "rt reflections denoise").map(|t| {
                    mx::PassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            let (gx, gy) = (half.0.div_ceil(8), half.1.div_ceil(8));
            pass.set_pipeline(&rt.temporal_refl);
            pass.set_bind_group(0, &dg[parity]);
            pass.dispatch_workgroups(gx, gy, 1);
            pass.set_pipeline(&rt.atrous);
            for bg in [&dg[2 + parity], &dg[4 + 2 * parity], &dg[5 + 2 * parity]] {
                pass.set_bind_group(0, bg);
                pass.dispatch_workgroups(gx, gy, 1);
            }
        }
        let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
            label: Some("rt reflections composite"),
            color_attachments: &[Some(mx::RenderPassColorAttachment {
                view: &hdr.view,
                resolve_target: None,
                ops: mx::Operations {
                    load: mx::LoadOp::Load,
                    store: mx::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: pass_timer(queries, timed, "rt composite"),
            occlusion_query_set: None,
        });
        pass.set_pipeline(&rt.composite);
        pass.set_bind_group(0, composite_bg);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod spv_dump {
    /// `SPV_DUMP=<dir> cargo test -p omsi-render spv_dump -- --ignored`: the ray tracing's
    /// shaders as wgpu's Vulkan backend writes them, for spirv-val.
    #[test]
    #[ignore]
    fn spv_dump() {
        let dir = std::path::PathBuf::from(std::env::var("SPV_DUMP").expect("SPV_DUMP"));
        let modules = [
            ("lighting", super::lighting_source()),
            ("reflect", super::reflect_source()),
            (
                "scene_plus",
                super::super::scene_shader_source(false).replace("//RT ", ""),
            ),
        ];
        for (name, src) in &modules {
            let module = naga::front::wgsl::parse_str(&crate::shaders::with_heap(src)).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
            let (module, info) = naga::back::pipeline_constants::process_overrides(
                &module,
                &info,
                None,
                &Default::default(),
            )
            .unwrap();
            let opts = naga::back::spv::Options {
                lang_version: (1, 5),
                flags: naga::back::spv::WriterFlags::empty(),
                force_loop_bounding: true,
                ray_query_initialization_tracking: true,
                ..Default::default()
            };
            for entry in &module.entry_points {
                let pipeline = naga::back::spv::PipelineOptions {
                    shader_stage: entry.stage,
                    entry_point: entry.name.clone(),
                };
                let words =
                    naga::back::spv::write_vec(&module, &info, &opts, Some(&pipeline)).unwrap();
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                std::fs::write(dir.join(format!("{name}_{}.spv", entry.name)), bytes).unwrap();
            }
            // and the HLSL wgpu's Direct3D 12 backend hands DXC (shader model 6.5)
            let hopts = naga::back::hlsl::Options {
                shader_model: naga::back::hlsl::ShaderModel::V6_5,
                ..Default::default()
            };
            let mut out = String::new();
            let popts = Default::default();
            let mut w = naga::back::hlsl::Writer::new(&mut out, &hopts, &popts);
            match w.write(&module, &info, None) {
                Ok(_) => std::fs::write(dir.join(format!("{name}.hlsl")), &out).unwrap(),
                Err(e) => {
                    std::fs::write(dir.join(format!("{name}.hlsl.err")), format!("{e:?}")).unwrap()
                }
            }
        }
    }
}
