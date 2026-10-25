//! What a view draws: the batches of draw items sorted by pipeline and material, the
//! pipeline kinds, and the render bundles recorded from them.

use crate::*;

pub(crate) fn origin_key(origin: DVec3) -> [u64; 3] {
    // DVec3 equality treats -0 and +0 alike. Keep that property in the hash key.
    origin.to_array().map(|v| if v == 0.0 { 0 } else { v.to_bits() })
}

/// For each distinct origin, the object's nearest blended mesh distance, rather than
/// the single point all its meshes share (a long vehicle's origin can be far from its
/// window nearest the camera). Full coordinates and the existing float equality apply.
pub(crate) fn nearest_by_origin(items: impl IntoIterator<Item = (DVec3, f32)>) -> HashMap<[u64; 3], f32> {
    let mut out: HashMap<[u64; 3], f32> = HashMap::new();
    for (origin, d) in items {
        // NaN origins never compared equal in the old lookup either.
        if origin.is_nan() {
            continue;
        }
        out.entry(origin_key(origin)).and_modify(|best| *best = best.min(d)).or_insert(d);
    }
    out
}

/// One draw of a mesh range with a material for one per-draw entry, before batching.
/// `pipe` picks the pipeline within a pass (and orders the batches).
#[derive(Clone, Copy)]
pub(crate) struct DrawItem {
    pub(crate) pipe: u8,
    pub(crate) mesh: u32,
    pub(crate) range: u32,
    pub(crate) material: u32,
    /// The material's look: draws of materials that look alike are batched together.
    pub(crate) look: u32,
    pub(crate) entry: u32,
}

/// Draws of the same mesh range with materials that look alike and the same pipeline, made as one instanced
/// draw: `instances` indexes the frame's draw list, which holds each instance's per-draw
/// entry (the vertex shader looks it up). Thousands of single draws were the biggest CPU
/// cost of a frame - wgpu validates and records every one - and trees, lamps, fences,
/// people and the AI cars' shared meshes collapse into a few hundred batches.
#[derive(Clone, PartialEq)]
pub(crate) struct Batch {
    pub(crate) pipe: u8,
    pub(crate) mesh: u32,
    pub(crate) first: u32,
    pub(crate) count: u32,
    pub(crate) material: u32,
    pub(crate) instances: std::ops::Range<u32>,
}

/// Turn draw items into batches, appending their entries to `list`. `sort`: the order does
/// not matter (depth-tested opaque and alpha-tested draws), so draws alike are gathered;
/// otherwise only neighbours are merged (the blended pass keeps its far-to-near order).
pub(crate) fn batch_items(
    scene: &Scene,
    items: &mut [DrawItem],
    sort: bool,
    list: &mut Vec<u32>,
    out: &mut Vec<Batch>,
) {
    // a batch keeps the material of the one before it when they look alike: no rebinding
    let mut last = (u32::MAX, 0);
    let mut push = |d: &DrawItem, start: u32, list: &Vec<u32>| {
        if last.0 != d.look {
            last = (d.look, d.material);
        }
        let (first, count, _) = scene.meshes[d.mesh as usize].ranges[d.range as usize];
        out.push(Batch {
            pipe: d.pipe,
            mesh: d.mesh,
            first,
            count,
            material: last.1,
            instances: start..list.len() as u32,
        });
    };
    if sort {
        let bits = |n: u32| u32::BITS - n.leading_zeros();
        let (mut look, mut mesh, mut range) = (0, 0, 0);
        for d in items.iter() {
            (look, mesh, range) = (look | d.look, mesh | d.mesh, range | d.range);
        }
        let mesh_shift = bits(range);
        let look_shift = mesh_shift + bits(mesh);
        let pipe_shift = look_shift + bits(look);
        if pipe_shift + 8 <= 64 {
            // one number a draw sorts twice as fast as comparing the four fields
            let mut keys: Vec<(u64, u32, u32)> = items
                .iter()
                .map(|d| {
                    let key = (d.pipe as u64) << pipe_shift
                        | (d.look as u64) << look_shift
                        | (d.mesh as u64) << mesh_shift
                        | d.range as u64;
                    (key, d.entry, d.material)
                })
                .collect();
            keys.sort_unstable_by_key(|k| k.0);
            let field = |key: u64, from: u32, to: u32| ((key >> from) & ((1 << (to - from)) - 1)) as u32;
            let mut k = 0;
            while k < keys.len() {
                let (key, _, material) = keys[k];
                let start = list.len() as u32;
                while k < keys.len() && keys[k].0 == key {
                    list.push(keys[k].1);
                    k += 1;
                }
                let d = DrawItem {
                    pipe: (key >> pipe_shift) as u8,
                    mesh: field(key, mesh_shift, look_shift),
                    range: field(key, 0, mesh_shift),
                    material,
                    look: field(key, look_shift, pipe_shift),
                    entry: 0,
                };
                push(&d, start, list);
            }
            return;
        }
        items.sort_unstable_by_key(|d| (d.pipe, d.look, d.mesh, d.range));
    }
    let mut k = 0;
    while k < items.len() {
        let d = items[k];
        let start = list.len() as u32;
        while k < items.len() && (items[k].pipe, items[k].mesh, items[k].range, items[k].look) == (d.pipe, d.mesh, d.range, d.look) {
            list.push(items[k].entry);
            k += 1;
        }
        push(&d, start, list);
    }
}

/// The material and look a depth-only draw (shadow map, depth prepass) is batched with: an
/// opaque surface writes its depth whatever its texture, so all of them share the first
/// material and batch across materials; an alpha-tested one needs its own texture for the cut-out.
pub(crate) fn depth_only_material(kind: u8, material: MaterialId, look: u32) -> (u32, u32) {
    if kind == 0 {
        (0, 0)
    } else {
        (material as u32, look)
    }
}

/// Record batches into a pass or a bundle, setting pipeline, buffers and material only
/// when they change.
pub(crate) fn encode_batches<'a, E: wgpu::util::RenderEncoder<'a>>(
    pass: &mut E,
    scene: &'a Scene,
    batches: &[Batch],
    pipeline: impl Fn(u8) -> &'a wgpu::RenderPipeline,
) {
    encode_batches_filtered(pass, scene, batches, |_| true, pipeline);
}

/// Record the batches accepted by `include`, setting pipeline, buffers and material only
/// when they change.
pub(crate) fn encode_batches_filtered<'a, E: wgpu::util::RenderEncoder<'a>>(
    pass: &mut E,
    scene: &'a Scene,
    batches: &[Batch],
    include: impl Fn(&Batch) -> bool,
    pipeline: impl Fn(u8) -> &'a wgpu::RenderPipeline,
) {
    let (mut pipe, mut mesh, mut material) = (u8::MAX, u32::MAX, u32::MAX);
    let mut page: Option<(&wgpu::Buffer, &wgpu::Buffer)> = None;
    let (mut base_vertex, mut first_index) = (0i32, 0u32);
    for b in batches {
        if !include(b) {
            continue;
        }
        if b.pipe != pipe {
            pass.set_pipeline(pipeline(b.pipe));
            pipe = b.pipe;
        }
        if b.mesh != mesh {
            let m = &scene.meshes[b.mesh as usize];
            if page.is_none_or(|(v, i)| *v != m.vertex_buf || *i != m.index_buf) {
                pass.set_vertex_buffer(0, m.vertex_buf.slice(..));
                pass.set_index_buffer(m.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                page = Some((&m.vertex_buf, &m.index_buf));
            }
            (base_vertex, first_index) = (m.base_vertex, m.first_index);
            mesh = b.mesh;
        }
        if b.material != material {
            pass.set_bind_group(
                1,
                Some(&scene.materials[b.material as usize].bind_group),
                &[],
            );
            material = b.material;
        }
        pass.draw_indexed(first_index + b.first..first_index + b.first + b.count, base_vertex, b.instances.clone());
    }
}

/// The state changes `encode_batches` makes for `chunks` (each recorded on its own, as the
/// main pass's bundles are): (pipeline switches, material bind groups set). OMSI_PROFILE.
pub(crate) fn state_changes(chunks: &[&[Batch]]) -> (usize, usize) {
    let (mut pipes, mut binds) = (0, 0);
    for chunk in chunks {
        let (mut pipe, mut material) = (u8::MAX, u32::MAX);
        for b in chunk.iter() {
            pipes += (b.pipe != pipe) as usize;
            binds += (b.material != material) as usize;
            (pipe, material) = (b.pipe, b.material);
        }
    }
    (pipes, binds)
}

/// What `state_changes` would count if each run of opaque and alpha-tested batches (as
/// `batch_items` sorts them: by pipeline, then look) were ordered by material first, the
/// cull and bias variants of a kind after it - an estimate for OMSI_PROFILE of what such an
/// order would save in bind groups and cost in pipeline switches. A run ends where the
/// pipeline code goes down (the next sorted call) or a blended batch comes.
pub(crate) fn state_changes_by_material(batches: &[Batch]) -> (usize, usize) {
    let mut reordered: Vec<Batch> = Vec::with_capacity(batches.len());
    let mut k = 0;
    while k < batches.len() {
        let sortable = |b: &Batch| b.pipe / 4 <= PIPE_ALPHA_TEST;
        if !sortable(&batches[k]) {
            reordered.push(batches[k].clone());
            k += 1;
            continue;
        }
        let start = k;
        k += 1;
        while k < batches.len() && sortable(&batches[k]) && batches[k].pipe >= batches[k - 1].pipe {
            k += 1;
        }
        let mut run = batches[start..k].to_vec();
        run.sort_by_key(|b| (b.pipe / 4, b.material, b.pipe));
        reordered.extend(run);
    }
    state_changes(&[&reordered])
}

/// Main pass pipeline kinds (`pipe_code`): opaque, alpha-tested, blended, blended
/// without depth write, depth-only surface coverage, and painted terrain without depth write.
pub(crate) const PIPE_OPAQUE: u8 = 0;
pub(crate) const PIPE_ALPHA_TEST: u8 = 1;
pub(crate) const PIPE_BLEND: u8 = 2;
pub(crate) const PIPE_BLEND_NO_WRITE: u8 = 3;
pub(crate) const PIPE_SURFACE_DEPTH: u8 = 4;
pub(crate) const PIPE_TERRAIN_PAINT: u8 = 5;
pub(crate) const PIPE_KINDS: u8 = 6;

pub(crate) fn effective_render_phase(instance: &Instance) -> RenderPhase {
    if instance.presurface {
        RenderPhase::PreSurface
    } else {
        instance.render_phase
    }
}

pub(crate) fn world_surface_phase(phase: RenderPhase) -> bool {
    matches!(phase, RenderPhase::PreSurface | RenderPhase::Surface | RenderPhase::Spline | RenderPhase::OnSurface)
}

/// Only diffuse-alpha ground coverage is committed after the surface phases. Painted
/// terrain masks, ordinary glass, and explicit no-Z-check materials keep their semantics.
pub(crate) fn surface_depth_coverage(phase: RenderPhase, alpha: AlphaMode, transmap: bool, no_z_check: bool) -> bool {
    world_surface_phase(phase) && alpha == AlphaMode::Blend && !transmap && !no_z_check
}
/// Which depth-prepass variant a material contributes to. A blended material normally has
/// no prepass because its fragments are see-through; a transmap is the useful exception:
/// fully opaque texels are usually the vehicle body while lower-alpha texels are its windows.
/// Presurfaces also contribute fully transparent blended texels, which seal the ground.
/// Materials explicitly marked no-Z-write/no-Z-check remain excluded, just as they are from
/// the ordinary prepass.
pub(crate) fn depth_prepass_kind(kind: u8, material: &Material, presurface: bool) -> Option<u8> {
    if material.no_z_check {
        return None;
    }
    if kind < PIPE_BLEND {
        Some(kind)
    } else if presurface && !material.no_z_write {
        // Alpha blending preserves colour, but even fully transparent cover texels
        // occlude later ground. AO must see the same cover as the colour pass.
        Some(PIPE_OPAQUE)
    } else if kind == PIPE_BLEND && material.transmap.is_some() && !material.no_z_write {
        Some(2)
    } else {
        None
    }
}

pub(crate) fn instance_depth_bias(instance: &Instance, material: &Material) -> bool {
    // Surfaces already receive a planar view-space pull in vs_main. Adding a
    // slope-scaled raster bias lets roads cover raised floors at grazing angles.
    // Keep raster bias for explicit material overrides only.
    material.z_bias > 0 || (material.no_z_check && !instance.surface)
}

pub(crate) fn surface_instance_code(
    blob: bool,
    ground_layer: bool,
    decal: bool,
    surface: bool,
    surface_bias: bool,
    paint: bool,
) -> f32 {
    if blob {
        2.0
    } else if paint && surface && surface_bias && !decal {
        // a road's painted lines (`RenderPhase::BeforeNormal` spline paint): pulled a little
        // more than the surface objects, so that a crossing laid over the road - drawn after
        // them, and losing the depth test to the paint a millimetre over the road in
        // Omsi.exe - does not cover the lines it overlaps (#1694)
        1.4
    } else if ground_layer {
        0.75
    } else if decal {
        if surface_bias { 1.25 } else { 0.9 }
    } else if surface {
        if surface_bias { 1.0 } else { 0.9 }
    } else {
        0.0
    }
}

/// OMSI's spline blend sort is horizontal in the x/z ground plane; Rust's vertical axis is z.
pub(crate) fn horizontal_sort_distance(origin: DVec3, render_origin: DVec3, camera_relative: Vec3) -> f32 {
    let p = (origin - render_origin).as_vec3() - camera_relative;
    glam::Vec2::new(p.x, p.y).length()
}

/// A main-pass draw's pipeline: the kind, whether back faces are culled, and whether the
/// raster depth bias applies (`[matl_Zbias]` and no-depth-check decals). The opaque
/// and alpha-tested draws are batched in this order (kinds 0 and 1 first).
pub(crate) fn pipe_code(kind: u8, cull: bool, surface: bool) -> u8 {
    debug_assert!(kind < PIPE_KINDS);
    kind * 4 + (cull as u8) * 2 + surface as u8
}

/// The pipeline of a main-pass batch (`DrawItem::pipe`).
pub(crate) fn main_pipeline(pp: &PassPipelines, pipe: u8) -> &wgpu::RenderPipeline {
    &pp.pipelines[pipe as usize]
}

/// Rasterizer state of a scene pipeline. The content meshes keep Direct3D's winding: the
/// visible side of a triangle is the one it shows clockwise on the screen (the side its
/// normals face). Turning y and z round for the right-handed world (`mesh_from_o3d`)
/// mirrors the mesh, but the right-handed camera mirrors the picture back, so the visible
/// side still arrives clockwise here (`o3d_front_faces_arrive_clockwise`). D3D culls the
/// other side by default, and models rely on it: the SD202's front flap click spot is an
/// inside-out shell over the right headlight, invisible from outside, and windows are
/// modelled twice, once per side, with different glass.
pub(crate) fn one_sided_primitive(cull: bool) -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        cull_mode: cull.then_some(wgpu::Face::Back),
        front_face: wgpu::FrontFace::Cw,
        ..Default::default()
    }
}

/// Are the back faces of this instance culled? Content meshes only, and not when the
/// instance is mirrored (a negative determinant turns the winding round).
/// `OMSI_NO_CULL=1` draws every mesh from both sides, for comparison.
pub(crate) fn culls_back_faces(scene: &Scene, inst: &Instance) -> bool {
    static NO_CULL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    scene.meshes[inst.mesh].one_sided
        && !*NO_CULL.get_or_init(|| omsi_cfg::flags::OMSI_NO_CULL.is_set())
        && glam::Mat3::from_mat4(inst.transform).determinant() > 0.0
}

/// How many bundles `record_bundles` makes of `n` batches (each of the same share).
pub(crate) fn bundle_parts(pool: Option<&rayon::ThreadPool>, n: usize) -> usize {
    let max_parts = pool.map(|p| (p.current_num_threads() + 1).clamp(4, 8)).unwrap_or(4);
    let parts = (n / 250).clamp(1, max_parts);
    if parts == 1 { 1 } else { n.div_ceil(n.div_ceil(parts)) }
}

/// The batches each bundle of `record_bundles` is made of, in order.
pub(crate) fn bundle_chunks<'b>(pool: Option<&rayon::ThreadPool>, batches: &'b [Batch]) -> Vec<&'b [Batch]> {
    let parts = bundle_parts(pool, batches.len());
    if parts == 1 {
        return vec![batches];
    }
    batches.chunks(batches.len().div_ceil(parts)).collect()
}

/// The main pass's batches as render bundles, one per chunk of `bundle_chunks`, recorded on
/// up to four threads. Checking every draw is the costliest CPU step of a frame; a bundle is
/// checked when it is made, and the pass that runs it only replays it. The bundles keep the
/// batches' order (the blended ones are far to near). A chunk with a bundle in `reuse` (the
/// same batches recorded before) is not recorded again; None where a part could not be
/// recorded.
pub(crate) fn record_bundles(
    device: &wgpu::Device,
    pool: Option<&rayon::ThreadPool>,
    scene: &Scene,
    chunks: &[&[Batch]],
    reuse: &[Option<wgpu::RenderBundle>],
    pp: &PassPipelines,
    camera: &wgpu::BindGroup,
    format: wgpu::TextureFormat,
    samples: u32,
) -> Vec<Option<wgpu::RenderBundle>> {
    let color_formats = [Some(format), Some(MASK_FORMAT), Some(GBUF_FORMAT), Some(AUX_FORMAT)];
    let record = |chunk: &[Batch]| -> wgpu::RenderBundle {
        let mut bundle =
            device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                label: Some("main pass part"),
                color_formats: &color_formats[..if format != HDR_FORMAT { 1 } else if rt_gbuf() { 4 } else { 2 }],
                depth_stencil: Some(wgpu::RenderBundleDepthStencil {
                    format: DEPTH_FORMAT,
                    depth_read_only: false,
                    stencil_read_only: true,
                }),
                sample_count: samples,
                multiview: None,
            });
        bundle.set_bind_group(0, camera, &[]);
        encode_batches(&mut bundle, scene, chunk, |pipe| main_pipeline(pp, pipe));
        bundle.finish(&wgpu::RenderBundleDescriptor {
            label: Some("main pass part"),
        })
    };
    // a bundle wgpu refuses (a buffer it names could not be made: the card ran out of
    // memory) panics in `finish`: that part of the picture is left out for the frame and
    // the game goes on - it ended the game on Windows right after an "Out of memory"
    let record = |chunk: &[Batch]| -> Option<wgpu::RenderBundle> {
        let r = catch(|| record(chunk)).ok_or(());
        match r {
            Ok(b) => Some(b),
            Err(_) => {
                static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log::error!("a part of the picture could not be recorded (the graphics card is out of memory?); left out");
                }
                None
            }
        }
    };
    let part = |k: usize| reuse.get(k).cloned().flatten().or_else(|| record(chunks[k]));
    if chunks.len() == 1 {
        return vec![part(0)];
    }
    run_parts(pool, chunks.len(), part)
}

/// The sun's and the lamps' shadow maps, their pipelines, and the cascades as last drawn.
pub(crate) struct ShadowRes {
    pub(crate) shadow_view: wgpu::TextureView,
    pub(crate) shadow_view_far: wgpu::TextureView,
    pub(crate) shadow_sampler: wgpu::Sampler,
    pub(crate) shadow_layout: wgpu::BindGroupLayout,
    /// [near opaque, near alpha-tested, far opaque, far alpha-tested, close opaque, close
    /// alpha-tested], then the lamps' (6 + kind; not with `basic_pipelines`)
    pub(crate) shadow_pipelines: Vec<wgpu::RenderPipeline>,
    /// Where the lamps' tiles lie under the far cascade (the shaders read it from the
    /// camera uniform).
    pub(crate) lamp_atlas: LampAtlas,
    /// The lamps' matrices as the window's last frame drew them: the mirrors look the tiles
    /// up with them (see `Renderer::prepare_camera`).
    pub(crate) lamp_matrices: std::cell::RefCell<Vec<Mat4>>,
    /// The far shadow cascade as last drawn: its light matrix, frames since, the render
    /// origin and the sun it was drawn for.
    pub(crate) shadow_far_cache: std::cell::Cell<(Mat4, u32, DVec3, Vec3)>,
    /// The same for the near cascade, drawn every other frame (see `render_inner`), with the
    /// window frames counted instead of its age (the frame's `ShadowPhase`).
    pub(crate) shadow_near_cache: std::cell::Cell<(Mat4, u32, DVec3, Vec3)>,
    /// The close cascade as the window last drew it: its light matrix, the render origin and
    /// the sun it was drawn for. The mirrors look the window's three cascades up from where
    /// they are, without a shadow pass of their own.
    pub(crate) shadow_close_cache: std::cell::Cell<(Mat4, DVec3, Vec3)>,
    /// Shadow atlas matrices from the first OpenXR eye, reused by the second eye.
    pub(crate) xr_shadow_cache: std::cell::Cell<Option<(DVec3, Vec3, Mat4, Mat4, Mat4)>>,
    /// Depth-only pipeline that fills its viewport with the far depth: clears the close
    /// cascade's part of the atlas when the near part is kept from the frame before.
    pub(crate) shadow_clear_pipeline: wgpu::RenderPipeline,
    /// What the street lamps' shadow tiles hold, kept while nothing in them changes.
    pub(crate) lamp_tiles: std::cell::Cell<passes::LampTiles>,
    /// Which parts of the player's vehicle stand still in it (the saloon's lamp tiles hold those).
    pub(crate) cabin_motion: std::cell::RefCell<passes::CabinMotion>,
    /// What stands over the ground round the camera, for the rain and the snow.
    pub(crate) precip: passes::PrecipMap,
}

/// What the views drew in their last frames, which the next frame's culling and bundles go by.
pub(crate) struct ViewHistory {
    pub(crate) triple_culling: [ViewCulling; 3],
    pub(crate) xr_culling: [ViewCulling; 2],
    /// `OMSI_DEBUG_FLICKER`: which near instances in view were drawn last frame.
    pub(crate) flicker: std::cell::RefCell<HashMap<usize, bool>>,
    /// The window's main-pass bundles as last recorded, with what they were made of (see
    /// `record_main_bundles`).
    pub(crate) main_bundles: std::cell::RefCell<Option<passes::batch::MainBundles>>,
    /// The same for each mirror (by its picture's texture), the latest last.
    pub(crate) mirror_bundles: std::cell::RefCell<Vec<(TextureId, passes::batch::MainBundles)>>,
    /// The window's last main pass: bundles recorded anew and bundles kept from the frame
    /// before (the tests check the reuse by it).
    pub(crate) bundle_stats: std::cell::Cell<(usize, usize)>,
    /// Instances the main view drew last frame (a bit each): they keep being drawn a little
    /// past the size and distance limits, so that an object right at a limit does not pop
    /// in and out as the camera sways.
    pub(crate) cull_drawn: std::cell::RefCell<Vec<u64>>,
    /// The screen size each object (origin and radius) was judged by in the main view's last
    /// frame: all its meshes and LOD levels take the same one, so exactly one level of an
    /// object is drawn and it does not flip between levels with the view's jitter.
    pub(crate) object_sizes: std::cell::RefCell<hashbrown::HashMap<[u64; 4], f32>>,
    /// Cleared and reused as the next main view's object-size history.
    pub(crate) object_sizes_scratch: std::cell::RefCell<hashbrown::HashMap<[u64; 4], f32>>,
}

#[derive(Default)]
pub(crate) struct ViewCulling {
    pub(crate) drawn: Vec<u64>,
    pub(crate) sizes: hashbrown::HashMap<[u64; 4], f32>,
    pub(crate) scratch: hashbrown::HashMap<[u64; 4], f32>,
}
