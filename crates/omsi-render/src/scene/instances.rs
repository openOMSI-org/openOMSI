//! Adding scene instances and changing them: transforms, flags, slots, lights, LOD ranges,
//! and the bounds the culling works with.

use crate::*;

impl Renderer {
    pub fn new_scene(&self) -> Scene {
        Scene {
            meshes: Vec::new(),
            textures: Vec::new(),
            glass_slot: None,
            materials: Vec::new(),
            trolley_wire_drawn: Vec::new(),
            instances: Vec::new(),
            render_origin: DVec3::ZERO,
            lights: Vec::new(),
            interior_lights: Vec::new(),
            interior_free: Vec::new(),
            coronas: Vec::new(),
            model_buf: None,
            params_buf: None,
            light_buf: None,
            grid_buf: None,
            corona_buf: None,
            corona_count: 0,
            corona_sent: Vec::new(),
            smoke: Vec::new(),
            smoke_buf: None,
            smoke_count: 0,
            rainfall: Default::default(),
            smoke_sent: Vec::new(),
            corona_runs: Vec::new(),
            draw_buf: None,
            camera_bind_group: None,
            shadow_bind_group: None,
            precip_bind_group: None,
            sky_bind_group: None,
            overlays: Vec::new(),
            premultiplied: Default::default(),
            transposed: Default::default(),
            overlay_res: Vec::new(),
            dirty: true,
            changed: Vec::new(),
            mesh_pages: Vec::new(),
            changed_mark: Vec::new(),
            origin_moved: false,
            origin_moved_z: false,
            cache_bounds: !omsi_cfg::flags::OMSI_NO_BOUNDS_CACHE.is_set(),
            bounds_meshes: Vec::new(),
            bounds_known: Vec::new(),
            bounds_users: Vec::new(),
            bounds_users_stale: true,
            bounds_rescan: false,
            bounds_dirty: false,
            block_bounds: Vec::new(),
            block_dirty: Vec::new(),
            block_cursor: 0,
            uploaded_instances: 0,
            next_instance_order: 0,
            object_run: Default::default(),
            uploaded_entries: 0,
            cpu_models: Vec::new(),
            cpu_params: Vec::new(),
            last_grid: Vec::new(),
            grid_scratch: Vec::new(),
            grid_filled: Vec::new(),
            grid_sent: Vec::new(),
            grid_counts: Vec::new(),
            grid_strength: Vec::new(),
            grid_marks: Vec::new(),
            smoke_scratch: Default::default(),
            lamp_slots: Default::default(),
            cabin_slots: Default::default(),
            cabin_places: Vec::new(),
            last_lights: Vec::new(),
            bind_groups: HashMap::new(),
            looks: hashbrown::HashMap::new(),
            pbr_maps: HashMap::new(),
            snow_textures: Default::default(),
        }
    }

    /// A model begins: the meshes added from now on, one after another from one place, are
    /// one object, and a surface of it lying on an earlier one of it in the same plane is
    /// drawn over that one, as Omsi.exe draws a model mesh by mesh (`scene::overlay`). Call it
    /// before each scenery object's and each vehicle's meshes; meshes added without it take
    /// no such step.
    pub fn begin_object(&self, scene: &mut Scene) {
        scene.object_run.begin(None);
    }

    /// `begin_object` for a model whose meshes come in the same order and place every time
    /// (a vehicle set, named by `model`): its steps are worked out for the first copy only.
    pub fn begin_shared_object(&self, scene: &mut Scene, model: impl std::hash::Hash) {
        use std::hash::{BuildHasher, BuildHasherDefault};
        let key = BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default().hash_one(model);
        scene.object_run.begin(Some(key));
    }

    pub fn add_instance(
        &self,
        scene: &mut Scene,
        mesh: MeshId,
        origin: DVec3,
        transform: Mat4,
        materials: Vec<MaterialId>,
    ) -> usize {
        let slots = scene.meshes[mesh]
            .ranges
            .iter()
            .map(|r| r.2)
            .max()
            .map(|m| m as usize + 1)
            .unwrap_or(1);
        let draw_order = scene.next_instance_order;
        scene.next_instance_order += 1;
        let key = crate::frame_plan::origin_key(origin);
        let slot_levels = scene.object_run.levels(key, draw_order, mesh, transform, &scene.meshes[mesh].flats, slots);
        scene.instances.push(Instance {
            draw_order,
            slot_levels,
            mesh,
            transform,
            origin,
            materials,
            slot_alpha: vec![1.0; slots],
            slot_light: vec![1.0; slots],
            slot_night: vec![1.0; slots],
            visible: true,
            slot_uv: vec![[0.0; 2]; slots],
            interior: 0.0,
            interior_lamps: 0,
            base: 0,
            bounds: InstanceBounds::default(),
            surface: false,
            presurface: false,
            render_phase: RenderPhase::Normal,
            surface_bias: false,
            blend_sort_origin: None,
            lod: (0.0, f32::MAX),
            blob: false,
            ground_layer: false,
            decal: false,
            object_radius: 0.0,
            detail: 1.0,
            any_distance: false,
            near_only: None,
            mirror_only: false,
            omsi_caster: false,
            ordered: false,
            casts_shadow: true,
            shadow_owner: None,
            roof: None,
        });
        if scene.bounds_known.get(mesh).copied().unwrap_or(false) {
            scene.bounds_users.push(scene.instances.len() - 1);
        }
        scene.instances.len() - 1
    }

    pub fn add_surface_instance(
        &self,
        scene: &mut Scene,
        mesh: MeshId,
        origin: DVec3,
        transform: Mat4,
        materials: Vec<MaterialId>,
    ) -> usize {
        let slots = scene.meshes[mesh]
            .ranges
            .iter()
            .map(|r| r.2)
            .max()
            .map(|m| m as usize + 1)
            .unwrap_or(1);
        let draw_order = scene.next_instance_order;
        scene.next_instance_order += 1;
        let key = crate::frame_plan::origin_key(origin);
        let slot_levels = scene.object_run.levels(key, draw_order, mesh, transform, &scene.meshes[mesh].flats, slots);
        scene.instances.push(Instance {
            draw_order,
            slot_levels,
            mesh,
            transform,
            origin,
            materials,
            slot_alpha: vec![1.0; slots],
            slot_light: vec![1.0; slots],
            slot_night: vec![1.0; slots],
            visible: true,
            slot_uv: vec![[0.0; 2]; slots],
            interior: 0.0,
            interior_lamps: 0,
            base: 0,
            bounds: InstanceBounds::default(),
            surface: true,
            presurface: false,
            render_phase: RenderPhase::Normal,
            surface_bias: true,
            blend_sort_origin: None,
            lod: (0.0, f32::MAX),
            blob: false,
            ground_layer: false,
            decal: false,
            object_radius: 0.0,
            detail: 1.0,
            any_distance: false,
            near_only: None,
            mirror_only: false,
            omsi_caster: false,
            ordered: false,
            casts_shadow: false,
            shadow_owner: None,
            roof: None,
        });
        if scene.bounds_known.get(mesh).copied().unwrap_or(false) {
            scene.bounds_users.push(scene.instances.len() - 1);
        }
        scene.instances.len() - 1
    }

    /// A vehicle's shadow blob: a surface instance (see `Instance::blob`).
    pub fn add_shadow_blob_instance(
        &self,
        scene: &mut Scene,
        mesh: MeshId,
        origin: DVec3,
        transform: Mat4,
        materials: Vec<MaterialId>,
    ) -> usize {
        let i = self.add_surface_instance(scene, mesh, origin, transform, materials);
        scene.instances[i].blob = true;
        i
    }

    /// Note that instance `i` needs its per-draw entries rewritten.
    pub(crate) fn mark_changed(scene: &mut Scene, i: usize) {
        if scene.dirty || i >= scene.uploaded_instances {
            return;
        }
        if scene.changed_mark.len() < scene.instances.len() {
            scene.changed_mark.resize(scene.instances.len(), false);
        }
        if !scene.changed_mark[i] {
            scene.changed_mark[i] = true;
            scene.changed.push(i);
        }
    }

    /// `n` consecutive slots for a vehicle's `[interiorlight]` lamps; give them back with
    /// [`Renderer::free_interior_lights`].
    pub fn alloc_interior_lights(&self, scene: &mut Scene, n: u32) -> u32 {
        if let Some(k) = scene.interior_free.iter().position(|f| f.1 >= n) {
            let (first, len) = scene.interior_free[k];
            if len == n {
                scene.interior_free.remove(k);
            } else {
                scene.interior_free[k] = (first + n, len - n);
            }
            return first;
        }
        let first = scene.interior_lights.len() as u32;
        scene
            .interior_lights
            .extend((0..n).map(|_| PointLight { intensity: 0.0, ..Default::default() }));
        first
    }

    pub fn free_interior_lights(&self, scene: &mut Scene, first: u32, n: u32) {
        for l in scene
            .interior_lights
            .iter_mut()
            .skip(first as usize)
            .take(n as usize)
        {
            l.intensity = 0.0;
        }
        scene.interior_free.push((first, n));
    }

    pub fn set_interior_light(&self, scene: &mut Scene, slot: u32, light: PointLight) {
        if let Some(l) = scene.interior_lights.get_mut(slot as usize) {
            *l = light;
        }
    }

    /// Which lamps light an instance: `count` (up to `MAX_LAMPS_PER_MESH`) slots from `first` (see
    /// `Instance::interior_lamps`).
    pub fn set_interior_lamps(&self, scene: &mut Scene, instance: usize, first: u32, count: u32) {
        let code = if count == 0 { 0 } else { first * LAMP_CODE_STRIDE + count.min(MAX_LAMPS_PER_MESH) };
        let i = &mut scene.instances[instance];
        if i.interior_lamps != code {
            i.interior_lamps = code;
            Self::mark_changed(scene, instance);
        }
    }

    /// Interior light brightness of an instance (warm ambient inside vehicles).
    /// Whether the model marks the instance's mesh `[shadow]` (see [`Instance::omsi_caster`]).
    pub fn set_omsi_caster(&self, scene: &mut Scene, instance: usize, on: bool) {
        if let Some(i) = scene.instances.get_mut(instance) {
            i.omsi_caster = on;
        }
    }

    /// Draw an instance with all its slots in model order (see [`Instance::ordered`]).
    pub fn set_ordered(&self, scene: &mut Scene, instance: usize, on: bool) {
        if let Some(i) = scene.instances.get_mut(instance) {
            i.ordered = on;
        }
    }

    /// Let an instance cast a sun shadow or not (see [`Instance::casts_shadow`]).
    pub fn set_casts_shadow(&self, scene: &mut Scene, instance: usize, on: bool) {
        if let Some(i) = scene.instances.get_mut(instance) {
            i.casts_shadow = on;
        }
    }

    /// The depth step of a spline laid on an earlier spline of its tile at the same height
    /// (every slot alike, at most `MAX_LEVEL`; see `draw_order_level`).
    pub fn set_spline_layer(&self, scene: &mut Scene, instance: usize, level: u8) {
        if let Some(i) = scene.instances.get_mut(instance) {
            let level = level.min(crate::scene::overlay::MAX_LEVEL);
            if i.slot_levels.iter().any(|l| *l != level) {
                i.slot_levels.iter_mut().for_each(|l| *l = level);
                Self::mark_changed(scene, instance);
            }
        }
    }

    /// The vehicle roof height of an instance (see [`Instance::roof`]).
    pub fn set_roof(&self, scene: &mut Scene, instance: usize, roof: Option<f32>) {
        if let Some(i) = scene.instances.get_mut(instance) {
            if i.roof != roof {
                i.roof = roof;
                Self::mark_changed(scene, instance);
            }
        }
    }

    /// Show an instance only in the mirrors (see [`Instance::mirror_only`]).
    pub fn set_mirror_only(&self, scene: &mut Scene, instance: usize, on: bool) {
        if let Some(i) = scene.instances.get_mut(instance) {
            i.mirror_only = on;
        }
    }

    pub fn set_interior(&self, scene: &mut Scene, instance: usize, interior: f32) {
        let i = &mut scene.instances[instance];
        if (i.interior - interior).abs() > 1e-4 {
            i.interior = interior;
            Self::mark_changed(scene, instance);
        }
    }

    /// Per-slot lightmap strengths (`[matl_lightmap]` variables).
    pub fn set_slot_light(&self, scene: &mut Scene, instance: usize, light: &[f32]) {
        let i = &mut scene.instances[instance];
        let mut changed = false;
        for (k, l) in i.slot_light.iter_mut().enumerate() {
            let v = light.get(k).copied().unwrap_or(1.0);
            changed |= *l != v;
            *l = v;
        }
        if changed {
            Self::mark_changed(scene, instance);
        }
    }

    /// Per-slot `[matl_item]` activity (night maps of `[matl_change]` variants).
    pub fn set_slot_night(&self, scene: &mut Scene, instance: usize, night: &[f32]) {
        let i = &mut scene.instances[instance];
        let mut changed = false;
        for (k, l) in i.slot_night.iter_mut().enumerate() {
            let v = night.get(k).copied().unwrap_or(1.0);
            changed |= *l != v;
            *l = v;
        }
        if changed {
            Self::mark_changed(scene, instance);
        }
    }

    /// Restrict an instance to a screen-size range (LOD levels).
    pub fn set_lod_range(&self, scene: &mut Scene, instance: usize, min: f32, max: f32) {
        scene.instances[instance].lod = (min, max);
    }

    /// Judge this mesh as part of a whole object, the way the original does: the object is
    /// drawn or left out as one - by its screen size against `performance_minObjSize` times
    /// its `[detail_factor]`, and beyond `performance_maxObjDist` unless it says
    /// `[noDistanceCheck]` - and its `[LOD]` level is chosen by the object's size, not by
    /// each mesh's own. `radius` (m, in the instance's own scale) holds the whole object
    /// about the instance's origin. Judged mesh by mesh, the small parts of a bus (mirrors,
    /// wipers, lamps) went missing at a distance while its body was still there, and the
    /// meshes of one LOD level switched at different distances.
    pub fn set_object_culling(
        &self,
        scene: &mut Scene,
        instance: usize,
        radius: f32,
        detail: f32,
        any_distance: bool,
    ) {
        let i = &mut scene.instances[instance];
        i.object_radius = radius.max(0.0);
        i.detail = if detail > 0.0 { detail } else { 1.0 };
        i.any_distance = any_distance;
    }

    /// Draw an instance only while the camera stands in `area` (world x0, y0, x1, y1 on the
    /// ground; None: wherever it is loaded).
    pub fn set_near_only(&self, scene: &mut Scene, instance: usize, area: Option<[f64; 4]>) {
        scene.instances[instance].near_only = area;
    }

    /// Change an instance transform (re-uploaded on the next `prepare`).
    pub fn set_transform(
        &self,
        scene: &mut Scene,
        instance: usize,
        origin: DVec3,
        transform: Mat4,
    ) {
        let i = &mut scene.instances[instance];
        if i.transform != transform || i.origin != origin {
            i.transform = transform;
            i.origin = origin;
            Self::mark_changed(scene, instance);
        }
    }

    /// Choose the render origin. Everything is re-uploaded when it moves.
    pub fn set_render_origin(&self, scene: &mut Scene, origin: DVec3) {
        if scene.render_origin != origin {
            if scene.model_buf.is_none() {
                scene.dirty = true;
            } else {
                // (up or down as well: only the translations and the vehicles' roof heights
                // depend on it - a hill climbed past the origin's reach rebuilt everything)
                scene.origin_moved_z |= origin.z != scene.render_origin.z;
                scene.origin_moved = true;
            }
            scene.render_origin = origin;
        }
    }

    /// Bounding sphere of an instance relative to the render origin: the mesh's sphere
    /// through the instance transform (the model matrix is that transform moved by the
    /// origin, so no matrix product is needed), the radius scaled by the largest axis.
    pub(crate) fn bounding_sphere(scene: &Scene, i: &Instance) -> (Vec3, f32) {
        let b = if scene.cache_bounds { i.bounds } else { InstanceBounds::new(&scene.meshes[i.mesh], i.transform) };
        (b.centre + (i.origin - scene.render_origin).as_vec3(), b.radius)
    }

    pub(crate) fn instance_scale(scene: &Scene, i: &Instance) -> f32 {
        if scene.cache_bounds { i.bounds.scale } else { transform_scale(i.transform) }
    }

    pub(crate) fn mesh_bounds_changed(scene: &mut Scene, mesh: MeshId) {
        scene.bounds_meshes.resize(scene.meshes.len(), false);
        scene.bounds_meshes[mesh] = true;
        scene.bounds_dirty = true;
        scene.bounds_known.resize(scene.meshes.len(), false);
        if !scene.bounds_known[mesh] {
            scene.bounds_known[mesh] = true;
            scene.bounds_users_stale = true;
        }
    }

    /// Slot `mesh` holds another mesh now (freed, or recycled): no longer a reshaped one.
    pub(crate) fn mesh_slot_replaced(scene: &mut Scene, mesh: MeshId) {
        scene.bounds_meshes.resize(scene.meshes.len(), false);
        scene.bounds_meshes[mesh] = true;
        scene.bounds_dirty = true;
        scene.bounds_rescan = true;
        if scene.bounds_known.get(mesh).copied().unwrap_or(false) {
            scene.bounds_known[mesh] = false;
            scene.bounds_users_stale = true;
        }
    }

    pub(crate) fn prepare_bounds(scene: &mut Scene) {
        if !scene.cache_bounds { return; }
        let blocks = scene.instances.len().div_ceil(CULL_BLOCK);
        let all = scene.dirty || scene.block_bounds.len() != blocks;
        scene.block_bounds.resize(blocks, (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)));
        scene.block_dirty.resize(blocks, false);
        let grow = |bb: &mut (DVec3, DVec3), i: &Instance| {
            let c = i.origin + i.bounds.centre.as_dvec3();
            let r = i.bounds.radius as f64;
            bb.0 = bb.0.min(c - r);
            bb.1 = bb.1.max(c + r);
        };
        if scene.dirty {
            for i in &mut scene.instances {
                i.bounds = InstanceBounds::new(&scene.meshes[i.mesh], i.transform);
            }
        } else {
            for k in scene.uploaded_instances..scene.instances.len() {
                let i = &mut scene.instances[k];
                i.bounds = InstanceBounds::new(&scene.meshes[i.mesh], i.transform);
                grow(&mut scene.block_bounds[k / CULL_BLOCK], i);
            }
            for &idx in &scene.changed {
                if let Some(i) = scene.instances.get_mut(idx) {
                    i.bounds = InstanceBounds::new(&scene.meshes[i.mesh], i.transform);
                    grow(&mut scene.block_bounds[idx / CULL_BLOCK], i);
                    scene.block_dirty[idx / CULL_BLOCK] = true;
                }
            }
            if scene.bounds_dirty {
                // Skinning can alter a shared mesh without changing any model matrix.
                // Scan once per pose update, rather than once per shadow/mirror view.
                if scene.bounds_users_stale {
                    let known = &scene.bounds_known;
                    scene.bounds_users = (0..scene.instances.len()).filter(|&k| known.get(scene.instances[k].mesh).copied().unwrap_or(false)).collect();
                    scene.bounds_users_stale = false;
                }
                let all: Vec<usize>;
                let scan: &[usize] = if std::mem::take(&mut scene.bounds_rescan) {
                    all = (0..scene.instances.len()).collect();
                    &all
                } else {
                    &scene.bounds_users
                };
                for &k in scan {
                    let Some(i) = scene.instances.get_mut(k) else { continue };
                    if scene.bounds_meshes.get(i.mesh).copied().unwrap_or(false) {
                        i.bounds = InstanceBounds::new(&scene.meshes[i.mesh], i.transform);
                        grow(&mut scene.block_bounds[k / CULL_BLOCK], i);
                        scene.block_dirty[k / CULL_BLOCK] = true;
                    }
                }
            }
        }
        if scene.bounds_dirty {
            scene.bounds_meshes.fill(false);
            scene.bounds_dirty = false;
        }
        let rebuild = |scene: &mut Scene, b: usize| {
            let mut bb = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
            for i in &scene.instances[b * CULL_BLOCK..((b + 1) * CULL_BLOCK).min(scene.instances.len())] {
                grow(&mut bb, i);
            }
            scene.block_bounds[b] = bb;
            scene.block_dirty[b] = false;
        };
        if all {
            for b in 0..blocks {
                rebuild(scene, b);
            }
            return;
        }
        let mut left = CULL_BLOCK_REBUILDS;
        for _ in 0..blocks {
            if left == 0 {
                break;
            }
            scene.block_cursor = (scene.block_cursor + 1) % blocks.max(1);
            if scene.block_dirty[scene.block_cursor] {
                rebuild(scene, scene.block_cursor);
                left -= 1;
            }
        }
    }

    pub(crate) fn cull_blocks(scene: &Scene) -> Option<Vec<(Vec3, f32)>> {
        (scene.cache_bounds && scene.block_bounds.len() == scene.instances.len().div_ceil(CULL_BLOCK)).then(|| {
            scene
                .block_bounds
                .iter()
                .map(|&(lo, hi)| {
                    if lo.x > hi.x {
                        (Vec3::ZERO, 0.0)
                    } else {
                        (((lo + hi) * 0.5 - scene.render_origin).as_vec3(), ((hi - lo).length() * 0.5) as f32)
                    }
                })
                .collect()
        })
    }

    /// A dynamic alpha value is never allowed to fade an opaque body panel; only
    /// blended materials follow the script value. An alpha-tested slot is cut out by its
    /// texture or transmap alone: the Thüringer Wald buses put `[alphascale]
    /// Envir_Brightness` on their transmapped body and roof (`[matl_alpha] 1`), which is 0
    /// at night, and scaled by it the whole roof went at dusk - with alpha to coverage
    /// under MSAA the colour pass drew none of its samples - while in OMSI it stays.
    /// Except a blended slot with a declared `[matl_transmap]`: Omsi.exe's transmap stage
    /// (0x7ffeb7) replaces the diffuse alpha that `[alphascale]` scaled (0x7feb8f).
    pub fn clamp_slot_alpha(alpha: f32, material_alpha: AlphaMode, transmap_declared: bool) -> f32 {
        match material_alpha {
            AlphaMode::Opaque | AlphaMode::Test => 1.0,
            AlphaMode::Blend if transmap_declared => 1.0,
            AlphaMode::Blend => alpha,
        }
    }
}
