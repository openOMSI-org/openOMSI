//! The scene: its instances (what is drawn where, and how), and what the renderer keeps of
//! them on the GPU between frames.

use crate::*;

mod instances;
pub(crate) mod overlay;
mod upload;

#[derive(Clone, Copy, Default)]
pub(crate) struct InstanceBounds {
    pub(crate) centre: Vec3,
    pub(crate) radius: f32,
    pub(crate) scale: f32,
}

impl InstanceBounds {
    pub(crate) fn new(mesh: &GpuMesh, transform: Mat4) -> Self {
        let scale = transform_scale(transform);
        Self {
            centre: transform.transform_point3(mesh.bounds_center),
            radius: mesh.bounds_radius * scale,
            scale,
        }
    }
}

pub(crate) const CULL_BLOCK: usize = 128;
pub(crate) const CULL_BLOCK_REBUILDS: usize = 8;

pub(crate) fn transform_scale(transform: Mat4) -> f32 {
    transform.x_axis.truncate().length_squared()
        .max(transform.y_axis.truncate().length_squared())
        .max(transform.z_axis.truncate().length_squared()).sqrt()
}

/// The ordered world passes used by OMSI for ground and scenery geometry.
///
/// Keep these phases separate in the main pass: a later phase must be able to sit over an
/// earlier blended surface, while depth testing still lets nearer geometry win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RenderPhase {
    PreSurface = 0,
    Terrain = 1,
    Surface = 2,
    Spline = 3,
    OnSurface = 4,
    BeforeNormal = 5,
    #[default]
    Normal = 6,
    AfterNormal = 7,
    AfterVehicles = 8,
}

impl RenderPhase {
    pub(crate) const COUNT: usize = 9;
    pub(crate) const DRAW_ORDER: [Self; Self::COUNT] = [
        Self::PreSurface,
        Self::Terrain,
        Self::Surface,
        Self::Spline,
        Self::OnSurface,
        Self::BeforeNormal,
        Self::Normal,
        Self::AfterNormal,
        Self::AfterVehicles,
    ];
}

#[repr(C, align(64))]
pub struct Instance {
    pub mesh: MeshId,
    /// World position of the instance's local frame.
    pub origin: DVec3,
    /// Transformed local bounds, refreshed with the GPU instance data. The floating
    /// render origin is applied per view, so changing it cannot leave stale spheres.
    pub(crate) bounds: InstanceBounds,
    /// Screen-size range [min, max) in which this instance is drawn (`[LOD]` levels).
    pub lod: (f32, f32),
    /// The whole object this mesh belongs to (see `set_object_culling`): the radius of a
    /// sphere about `origin` that holds all of it (0 = the mesh is judged on its own sphere),
    /// its `[detail_factor]` and whether it is kept at any distance (`[noDistanceCheck]`).
    pub object_radius: f32,
    pub detail: f32,
    pub any_distance: bool,
    pub visible: bool,
    /// Seen only in the mirrors and other views drawn into textures, not in the window's
    /// picture: the driver at the wheel while the player looks from the driver's seat (the
    /// figure would fill the view, but the mirrors show him as OMSI does).
    pub mirror_only: bool,
    /// A vehicle's flat shadow blob (`[isshadow]`, a surface). It is drawn always, as OMSI
    /// draws it: it stood in for the sun shadow map only while that was off, and with the
    /// map on (the usual case, Enhanced always) no bus had anything under it - the sun's
    /// shadow falls beside the bus at any but a noon sun, while the blob is the sky light
    /// the body keeps off the road, which no shadow map and no screen-space AO supplies.
    pub blob: bool,
    /// Surface geometry classification (roads, markings, crossings), used for culling and
    /// weather/shading. `surface_bias` independently selects the vertex shader's depth pull.
    pub surface: bool,
    /// It may cast a sun shadow: every ordinary instance, no surface - a surface lies on
    /// the ground, and a caster in one plane with what it falls on paints dark patches into
    /// it - except a spline standing clear of the ground (a bridge deck, an elevated
    /// railway), which is raised with `set_casts_shadow`.
    pub casts_shadow: bool,
    /// The model marks the mesh `[shadow]`: one OMSI casts a shadow from (with the
    /// option `omsi_shadow_casters` only these do).
    pub omsi_caster: bool,
    /// Drawn only while the camera stands in this area of the ground (world x0, y0, x1, y1):
    /// a stand-in for far tiles, which OMSI has loaded only around its own tile (see
    /// `set_near_only`).
    pub near_only: Option<[f64; 4]>,
    /// Creation order, preserved when moved into a recycled GPU slot. Transparent
    /// layers of one model must keep this order independently of their slot ids.
    pub(crate) draw_order: u64,
    /// Transform relative to `origin` (rotation/scale plus a small translation).
    pub transform: Mat4,
    /// Material per mesh material slot.
    pub materials: Vec<MaterialId>,
    /// Dynamic parameters: per material slot alpha multiplier; per instance visibility and
    /// uv offset.
    pub slot_alpha: Vec<f32>,
    /// Per material slot `[matl_lightmap]` strength (script variable).
    pub slot_light: Vec<f32>,
    /// Per material slot: is the `[matl_item]` variant (night map) active (0/1).
    pub slot_night: Vec<f32>,
    /// `[texcoordtransX/Y]` offset per material slot: every band of the SD200's roller
    /// blind scrolls on its own, so one offset for the whole mesh is not enough.
    pub slot_uv: Vec<[f32; 2]>,
    /// Interior light brightness (0..1) added as warm ambient (`[interiorlight]`), for an
    /// instance without lamps of its own (a passenger standing in a lit bus).
    pub interior: f32,
    /// The `[interiorlight]` lamps that light this mesh (its `[illumination_interior]`): the
    /// first of its run of slots in `Scene::interior_lights` times `LAMP_CODE_STRIDE` plus
    /// how many; 0 = none, `interior` stands in.
    pub interior_lamps: u32,
    /// First entry of this instance in the per-draw storage buffers (set by `prepare`).
    pub(crate) base: u32,
    /// `[rendertype] presurface`: drawn before terrain, including blended materials whose
    /// transparent texels write depth to reveal excavations below the ground.
    pub presurface: bool,
    /// OMSI world-pass order. Most instances use `Normal`; road/surface assets are assigned
    /// their authored phase by the scene loader.
    pub render_phase: RenderPhase,
    /// Apply a planar view-space depth pull. Metric-lifted roads and ordered scenery phases skip
    /// it, so their placement does not change with the camera angle.
    pub surface_bias: bool,
    /// OMSI sorts blended spline pieces by their placement origin (horizontal distance), not
    /// the containing map tile's shared origin.
    pub blend_sort_origin: Option<DVec3>,
    /// A painted ground layer (`[groundtex]` through its brush mask): a surface that is the
    /// ground itself, so it does not get the roads' pull towards the camera (see vs_main).
    pub ground_layer: bool,
    /// A surface object (a crossing, markings) over the road splines. Legacy instances get a
    /// small view-space pull; ordered OMSI surfaces keep the flag for shading but skip it.
    pub decal: bool,
    /// Stable identity of a pole fixture hosting embedded virtual map lights.
    pub shadow_owner: Option<i64>,
    /// Part of a vehicle whose roof lies this high over its origin (model frame): what faces
    /// up under the roof (the floor, the seats) is out of the weather - no snow nor wet on
    /// it. (Only the vehicle the camera is in was spared, by its box; every other bus showed
    /// its saloon under snow through the windows.)
    pub roof: Option<f32>,
    /// Drawn with every slot in model order among the blended draws, as Omsi.exe draws a
    /// model: mesh after mesh, each material subset with its own states and depth write
    /// (0x7c32c4 -> 0x7fd6c4, DrawSubset), not its opaque parts first. Set on the models
    /// where it matters: a blended slot that writes depth before an opaque one (a body with
    /// `[matl_alpha] 2` listed before its interior hides the interior as in the original,
    /// instead of showing it through the paint's alpha).
    pub ordered: bool,
    /// The draw order's depth step of each material slot (see `draw_order_level`): above 0
    /// only for a slot lying on an earlier surface of the same object (`overlay`).
    pub(crate) slot_levels: Vec<u8>,
}

/// The rain the renderer draws round the camera (rain.wgsl): every drop is worked out on
/// the GPU from these.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rainfall {
    /// How hard it rains (0..1; 0 nothing falls).
    pub rate: f32,
    /// The median volume diameter of its drops (mm).
    pub drop_mm: f32,
    /// The eye's velocity (m/s, world): the drops' streaks are their way against it.
    pub eye_velocity: glam::Vec3,
    /// The wind the drops drift with (m/s, world).
    pub wind: glam::Vec3,
    /// The `rain_quality` setting: 2 high, 1 medium, 0 low.
    pub quality: u8,
}

pub struct Scene {
    pub meshes: Vec<GpuMesh>,
    pub textures: Vec<GpuTexture>,
    /// Texture slot read by procedural rain films.
    pub(crate) glass_slot: Option<TextureId>,
    pub materials: Vec<Material>,
    pub instances: Vec<Instance>,
    /// The trolleybus wire networks whose wires this scene already draws (by address: see
    /// `omsi_scene::trolley_view`), so that each bus does not add a copy of them.
    pub trolley_wire_drawn: Vec<usize>,
    /// World position everything is expressed relative to on the GPU (updated per frame).
    pub render_origin: DVec3,
    /// Point lights and coronas for the next frame (set by the app every frame).
    pub lights: Vec<PointLight>,
    /// The vehicles' `[interiorlight]` lamps, in slots each vehicle keeps
    /// (`Renderer::alloc_interior_lights`): they light only the meshes that name them.
    pub interior_lights: Vec<PointLight>,
    pub(crate) interior_free: Vec<(u32, u32)>,
    pub coronas: Vec<Corona>,
    /// The rain falling round the camera this frame (rain.wgsl; set by the app every frame).
    pub rainfall: Rainfall,
    /// Smoke particles for the next frame (set by the app every frame).
    pub smoke: Vec<SmokeParticle>,
    pub(crate) smoke_buf: Option<wgpu::Buffer>,
    pub(crate) smoke_count: u32,
    /// What `smoke_buf` holds (the bytes last written), as `corona_sent` for the coronas.
    pub(crate) smoke_sent: Vec<u8>,
    /// Runs of this frame's coronas by picture: (texture, first, count).
    /// (and whether the run belongs to the vehicle the camera is in, drawn after it)
    pub(crate) corona_runs: Vec<(u16, u32, u32, bool)>,
    pub(crate) model_buf: Option<GpuArray>,
    pub(crate) params_buf: Option<GpuArray>,
    pub(crate) light_buf: Option<GpuArray>,
    pub(crate) grid_buf: Option<GpuArray>,
    pub(crate) corona_buf: Option<wgpu::Buffer>,
    pub(crate) corona_count: u32,
    /// What `corona_buf` holds (the bytes last written): the window's picture and its
    /// mirrors send the same coronas in a frame, and it is written once.
    pub(crate) corona_sent: Vec<u8>,
    /// The frame's draw list (see `Batch`), shared by the shadow, prepass and main passes.
    pub(crate) draw_buf: Option<GpuArray>,
    pub(crate) camera_bind_group: Option<wgpu::BindGroup>,
    /// The same with slot 8 not the ground albedo picture, for the pass that draws into it
    /// (a texture is not read and drawn into at once).
    pub(crate) ground_camera_bind_group: Option<wgpu::BindGroup>,
    pub(crate) shadow_bind_group: Option<wgpu::BindGroup>,
    /// As `shadow_bind_group`, with the precipitation roof map's camera.
    pub(crate) precip_bind_group: Option<wgpu::BindGroup>,
    pub(crate) sky_bind_group: Option<wgpu::BindGroup>,
    /// HUD images drawn after the scene: (texture, rect in pixels x0,y0,x1,y1).
    pub overlays: Vec<(TextureId, [f32; 4])>,
    /// Overlay textures that hold premultiplied alpha (drawn by `omsi-ui`, e.g. the
    /// navigator) rather than straight alpha.
    pub premultiplied: std::collections::HashSet<TextureId>,
    /// Overlay textures drawn on their side (their u down the rectangle, v across it): the
    /// mirror panels of a glass whose mesh lays the picture so.
    pub transposed: std::collections::HashSet<TextureId>,
    /// Per overlay: the texture its bind group was made for, its rect buffer and the group
    /// (kept between frames; only the rect is rewritten).
    pub(crate) overlay_res: Vec<(TextureId, wgpu::Buffer, wgpu::BindGroup, [f32; 8])>,
    /// Structural change (render origin moved, buffers too small): everything is rebuilt.
    pub(crate) dirty: bool,
    /// How many instances (and per-draw entries) the buffers hold; instances added since
    /// are appended to the buffers instead of rebuilding them, as long as they fit.
    pub(crate) uploaded_instances: usize,
    pub(crate) next_instance_order: u64,
    /// The object whose meshes are being added, for `Instance::slot_levels`.
    pub(crate) object_run: overlay::ObjectRun,
    pub(crate) uploaded_entries: u32,
    /// Instances whose transform or parameters changed since the last `prepare`: only
    /// their entries are rewritten. Rebuilding the whole per-draw buffer for 17 000 objects
    /// because one bus moved was the biggest single CPU cost of a frame.
    pub(crate) changed: Vec<usize>,
    pub(crate) mesh_pages: Vec<MeshPage>,
    pub(crate) changed_mark: Vec<bool>,
    pub(crate) origin_moved: bool,
    /// ... and up or down: the vehicles' roof heights in their parameters are redone too.
    pub(crate) origin_moved_z: bool,
    pub(crate) cache_bounds: bool,
    pub(crate) bounds_meshes: Vec<bool>,
    /// The meshes ever reshaped (skinned), and the instances drawing one of them, so that a
    /// new pose rescans those instead of the whole scene; stale when an instance's mesh changes.
    pub(crate) bounds_known: Vec<bool>,
    pub(crate) bounds_users: Vec<usize>,
    pub(crate) bounds_users_stale: bool,
    /// A mesh slot was freed or taken by another mesh: its instances' bounds are redone once
    /// by a scan of the whole scene, without counting the slot as reshaped.
    pub(crate) bounds_rescan: bool,
    pub(crate) block_bounds: Vec<(DVec3, DVec3)>,
    pub(crate) block_dirty: Vec<bool>,
    pub(crate) block_cursor: usize,
    pub(crate) bounds_dirty: bool,
    /// What the per-draw buffers hold, kept on the CPU: changed entries are written here
    /// and uploaded as a few merged ranges. Every `write_buffer` makes a new staging buffer
    /// on the GPU, and one per changed vehicle or person was a hundred of them a frame.
    pub(crate) cpu_models: Vec<[[f32; 4]; 4]>,
    pub(crate) cpu_params: Vec<[f32; 4]>,
    /// The light grid and lights as last uploaded, so that unchanged ones are not sent again.
    pub(crate) last_grid: Vec<u32>,
    /// The room the next grid is made in: half a megabyte, made and let go twice a frame
    /// (the window's picture and a mirror's), went back to the system each time and came
    /// back as fresh pages to be faulted in. Unused (`u32::MAX`) but for `grid_filled`.
    pub(crate) grid_scratch: Vec<u32>,
    /// The cells with lamps in `grid_scratch`, and in `last_grid` (what the buffer holds).
    pub(crate) grid_filled: Vec<u32>,
    pub(crate) grid_sent: Vec<u32>,
    /// The lamps in each cell while a grid is made, and the cells to compare.
    pub(crate) grid_counts: Vec<u8>,
    /// How strongly each light of a grid cell lights the cell's middle (parallel to the
    /// grid): a full cell keeps its strongest (see `Renderer::prepare_lights`).
    pub(crate) grid_strength: Vec<f32>,
    pub(crate) grid_marks: Vec<bool>,
    /// The smoke's sprites, their order and the sorted list of the last `prepare_smoke`,
    /// kept for the next: every car's exhaust is thousands of sprites, and lists made anew
    /// each time grew by copying and went back to the system (see `grid_scratch`).
    pub(crate) smoke_scratch: (Vec<(f64, GpuCorona)>, Vec<(f64, u32)>, Vec<GpuCorona>),
    /// The world's lamps that have a shadow map and the tile of each (the places of the lamps
    /// in centimetres): they keep it against a lamp only a little stronger (`prepare_lights`).
    pub(crate) lamp_slots: WorldSlots,
    /// The same for the lamps of the player's saloon (`Cabin`).
    pub(crate) cabin_slots: CabinSlots,
    /// Where each of `interior_lights` hangs in the player's vehicle's frame, as last taken for
    /// its tile (`lamp_tiles`).
    pub(crate) cabin_places: Vec<Vec3>,
    pub(crate) last_lights: Vec<u8>,
    /// Material bind groups and uniform buffers made since the last `prepare`, by what they
    /// hold: materials made in one go with the same textures and values share them (a C2's
    /// 965 materials need about a tenth as many). Only for a frame, so that nothing keeps a
    /// freed or replaced texture alive.
    pub(crate) bind_groups: HashMap<BindKey, (wgpu::BindGroup, wgpu::Buffer)>,
    /// The number of each material look seen so far, by its key's hash (see `Material::look`).
    pub(crate) looks: hashbrown::HashMap<u64, u32>,
    /// The PBR maps of a diffuse texture (register them before making its materials).
    pub pbr_maps: HashMap<TextureId, PbrMaps>,
    /// Textures that are a season's snow pictures (`WinterSnow` folders): a material drawn
    /// with one shows its snow as the map made it, as OMSI 2 shows snow, and gets no snow
    /// laid over it (register them before making their materials).
    pub snow_textures: std::collections::HashSet<TextureId>,
}

/// `MaterialUniform::ambient`'s w: 1 for a material whose texture is a season's snow
/// picture (`Scene::snow_textures`).
pub(crate) fn snow_texture_flag(scene: &Scene, texture: Option<TextureId>) -> f32 {
    if texture.is_some_and(|t| scene.snow_textures.contains(&t)) {
        1.0
    } else {
        0.0
    }
}

impl Scene {
    /// The look number of a material bind group made of `key` (numbers start at 1; 0 is the
    /// shared look of the opaque depth-only draws).
    pub(crate) fn look(&mut self, key: &BindKey) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        self.intern_look(h.finish())
    }

    pub(crate) fn intern_look(&mut self, hash: u64) -> u32 {
        let next = self.looks.len() as u32 + 1;
        *self.looks.entry(hash).or_insert(next)
    }
}

impl Scene {
    /// Move the overlays from `start` on by `x` pixels across: a panel laid out at its own
    /// origin, put where it shows.
    pub fn shift_overlays(&mut self, start: usize, x: f32) {
        for (_, rect) in &mut self.overlays[start..] {
            rect[0] += x;
            rect[2] += x;
        }
    }

    /// Bytes of one texture on the GPU (0 for a freed slot or an unknown id).
    pub fn texture_bytes_of(&self, id: TextureId) -> u64 {
        self.textures.get(id).map(|t| t.bytes).unwrap_or(0)
    }

    /// The size of one texture.
    pub fn texture_size_of(&self, id: TextureId) -> Option<(u32, u32)> {
        self.textures.get(id).map(|t| t.size)
    }

    /// The GPU format of one texture, for statistics.
    pub fn texture_format_of(&self, id: TextureId) -> String {
        self.textures
            .get(id)
            .map(|t| format!("{:?}", t.texture.format()))
            .unwrap_or_default()
    }

    /// Bytes the meshes' pages take on the GPU, each page once, whatever share of it is in use.
    pub fn mesh_page_bytes(&self) -> u64 {
        let shared: u64 = self.mesh_pages.iter().map(|p| p.vertex_cap + p.index_cap).sum();
        // (a set: without base vertices every mesh has buffers of its own, thousands of them)
        let mut own: std::collections::HashSet<&wgpu::Buffer> = std::collections::HashSet::new();
        for m in self.meshes.iter().filter(|m| m.page == u32::MAX && !m.ranges.is_empty()) {
            if own.insert(&m.vertex_buf) {
                own.insert(&m.index_buf);
            }
        }
        shared + own.iter().map(|b| b.size()).sum::<u64>()
    }

    /// Bytes on the GPU: (textures, mesh buffers, per-draw and light buffers). Freed slots
    /// share one small placeholder, which is not counted.
    pub fn gpu_bytes(&self) -> (u64, u64, u64) {
        let tex = self.textures.iter().map(|t| t.bytes).sum();
        let mesh = self.mesh_page_bytes();
        let other = [&self.model_buf, &self.params_buf, &self.light_buf, &self.grid_buf, &self.draw_buf]
            .iter()
            .filter_map(|b| b.as_ref())
            .map(|b| b.gpu_bytes())
            .sum::<u64>()
            + self.corona_buf.as_ref().map_or(0, |b| b.size());
        (tex, mesh, other)
    }
}
