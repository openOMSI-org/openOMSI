//! The lower levels of detail of a vehicle (`[LOD]` of its model): their meshes on the
//! graphics card with plain materials, and their instances, drawn in place of the full model
//! while the vehicle is small on the screen (the AI traffic's, see `add_vehicle_shared_lod`).

use super::*;

/// Where the keys of the lower levels' meshes start in a vehicle's mesh cache (the full model's
/// meshes are keyed by their index).
pub(super) const FAR_MESH_KEY: usize = 1 << 20;

/// The size band each level of detail is drawn in, by the object's screen size: OMSI picks the
/// first level, in the order the model lists them, whose least size the object reaches, else
/// the last one whatever its own (Omsi.exe 0x5ef860, as the scenery's levels, see
/// `type_gpu_lods`): a level is drawn from its least size (the last from 0) up to the least of
/// the sizes listed before it. `mins` are the least sizes of the model's levels, `present` the
/// levels (ascending) that have meshes on the card: a level whose meshes would not load is
/// left out, and the bands of the others still meet - the vehicle never has a size at which
/// nothing is drawn.
pub(super) fn lod_bands_of(mins: &[f32], present: &[usize]) -> Vec<(usize, (f32, f32))> {
    let kept: Vec<f32> = present.iter().map(|&l| mins.get(l).copied().unwrap_or(0.0)).collect();
    (0..kept.len())
        .map(|i| {
            let lo = if i + 1 == kept.len() { 0.0 } else { kept[i] };
            (present[i], (lo, kept[..i].iter().copied().fold(f32::MAX, f32::min)))
        })
        .collect()
}

pub(super) fn lod_bands(model: &omsi_model::Model, present: &[usize]) -> Vec<(usize, (f32, f32))> {
    let mins: Vec<f32> = model.lods.iter().map(|l| l.min_size).collect();
    lod_bands_of(&mins, present)
}

/// Whether the lower levels may stand in for the full model of `vt`: the plain meshes of a
/// lower level have no switched texture, interior lamp, display, animation or night map of
/// their own, so a model that has lit windows, a destination display or a switched look (a bus,
/// a tram) keeps its full meshes, and so does one whose lower level is itself lit (a lit window
/// must not go dark at the size the level takes over). A car's own lamp glows are coronas, which
/// the full model's night maps only add to: the car's lower level does without them.
pub(crate) fn lods_misfit(vt: &omsi_sim::VehicleType) -> Option<&'static str> {
    let unswitched = |o: &omsi_model::MaterialDef| o.change.is_none() && !o.item;
    let unlit = |o: &omsi_model::MaterialDef| o.nightmap.is_none() && o.lightmap.is_none() && o.lightmaps.is_empty();
    // (a number plate's text texture is no display: nobody reads it from this far)
    if !vt.model.interior_lights.is_empty() {
        Some("interior lamps")
    } else if !vt.model.script_textures.is_empty() {
        Some("script textures (displays)")
    } else if !vt.texchanges.is_empty() {
        Some("switched textures")
    } else if !vt.meshes.iter().all(|m| m.overrides.iter().all(unswitched)) {
        Some("switched materials")
    } else if !vt.lod_meshes.iter().all(|m| m.overrides.iter().all(|o| unswitched(o) && unlit(o))) {
        Some("a lit or switched lower level")
    } else if !vt.lod_meshes.iter().all(|m| {
        let d = &vt.model.meshes[m.def_index];
        d.animations.is_empty() && d.anim_parent.is_none() && d.visible.is_none() && d.visible_any.is_empty()
    }) {
        Some("an animated or conditional lower level")
    } else {
        None
    }
}

/// The levels with meshes: the full model's and those of `far`.
pub(super) fn levels_present(far: &[FarMesh]) -> Vec<usize> {
    let mut levels: Vec<usize> = std::iter::once(0).chain(far.iter().map(|f| f.level)).collect();
    levels.sort_unstable();
    levels.dedup();
    levels
}

impl World {
    /// The lower levels' meshes of `vt` and their plain materials, for a vehicle set being
    /// uploaded (`skip`: a debugging filter keeps only some meshes: none of these).
    pub(super) fn upload_far(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        up: &mut VehicleUpload,
        mesh_ids: &mut HashMap<(PathBuf, usize), (MeshId, usize)>,
        mesh_keys: &mut Vec<(PathBuf, usize)>,
        skip: bool,
    ) -> Vec<FarMesh> {
        let vt = up.vt;
        let mut far = Vec::new();
        // (the player's own bus never draws a lower level, nor does a model they would dim)
        if skip || up.player {
            return far;
        }
        if let Some(why) = lods_misfit(vt).filter(|_| !vt.lod_meshes.is_empty()) {
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                log::info!("  vehicle set {}: its lower levels of detail are not used ({why})", vt.def.path.file_name().unwrap_or_default().to_string_lossy());
            }
            return far;
        }
        for (k, vm) in vt.lod_meshes.iter().enumerate() {
            let def = &vt.model.meshes[vm.def_index];
            // (a mesh drawn only from some views: an AI car is seen from outside, as the full
            // model's - `view_events`)
            if def.viewpoint != 0 && def.viewpoint & 4 == 0 {
                continue;
            }
            let mats: Vec<MaterialId> = vm
                .materials
                .iter()
                .enumerate()
                .map(|(slot, m)| self.far_material(renderer, scene, up, vm, slot, m))
                .collect();
            let key = (vt.def.path.clone(), FAR_MESH_KEY + k);
            let id = match mesh_ids.get_mut(&key) {
                Some(e) => {
                    if !mesh_keys.contains(&key) {
                        e.1 += 1;
                        mesh_keys.push(key);
                    }
                    e.0
                }
                None => {
                    let id = self.gpu.lock().add_mesh(renderer, scene, &vm.data);
                    scene.meshes[id].source = Some(vt.def.path.display().to_string());
                    mesh_ids.insert(key.clone(), (id, 1));
                    mesh_keys.push(key);
                    id
                }
            };
            far.push(FarMesh { level: def.lod, mesh: id, mats, casts: def.shadow });
        }
        if omsi_cfg::flags::OMSI_PROFILE.is_set() && !vt.lod_meshes.is_empty() {
            log::info!("  vehicle set {}: {} meshes of {} lower levels of detail ({:?})", vt.def.path.file_name().unwrap_or_default().to_string_lossy(), far.len(), vt.model.lods.len().saturating_sub(1), lod_bands(&vt.model, &levels_present(&far)));
        }
        far
    }

    /// The instances of a vehicle's lower levels of detail, and the size bands of the full
    /// model's (`instances`): as Omsi.exe picks a level by the vehicle's size on the screen,
    /// the full model for a big one and the lower levels' meshes (plain, still, standing
    /// where the body does) for a small one - a bus far down the street was every one of its
    /// 200 meshes, tens of thousands of triangles, for a handful of pixels. None are made
    /// unless `on` (a vehicle whose pose the caller gives them too).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn far_instances(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        vt: &omsi_sim::VehicleType,
        set: &VehicleSet,
        instances: &[usize],
        on: bool,
        gpu: &mut GpuCache,
    ) -> Vec<usize> {
        let present = levels_present(&set.far);
        let mut bands = lod_bands(&vt.model, &present);
        // (OMSI_FORCE_LOD=1, to look at the lower levels: they are drawn at any size)
        if omsi_cfg::flags::OMSI_FORCE_LOD.is_set() && bands.len() > 1 {
            bands = bands.iter().enumerate().map(|(k, b)| (b.0, if k == 0 { (f32::MAX * 0.5, f32::MAX) } else { (0.0, f32::MAX) })).collect();
        }
        let mut far = Vec::new();
        if on && !set.far.is_empty() && bands.len() > 1 {
            for &inst in instances {
                if scene.instances.get(inst).is_some_and(|x| !x.blob) {
                    renderer.set_lod_range(scene, inst, bands[0].1 .0, bands[0].1 .1);
                }
            }
            renderer.begin_shared_object(scene, (&vt.def.path, "far"));
            for f in &set.far {
                let (lo, hi) = bands.iter().find(|b| b.0 == f.level).map_or((0.0, f32::MAX), |b| b.1);
                let i = renderer.add_instance(scene, f.mesh, DVec3::ZERO, Mat4::IDENTITY, f.mats.clone());
                renderer.set_omsi_caster(scene, i, f.casts);
                renderer.set_roof(scene, i, vt.def.bounding_box.map(|b| b[5] + b[2] * 0.5));
                renderer.set_lod_range(scene, i, lo, hi);
                far.push(gpu.instance(renderer, scene, i));
            }
        }
        far
    }

    /// The plain material of one slot of a lower level of detail's mesh: its texture (in the
    /// paint scheme's pictures) and its alpha mode, nothing the scripts switch.
    pub(super) fn far_material(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        up: &mut VehicleUpload,
        vm: &omsi_sim::vehicle::VehicleMesh,
        slot: usize,
        m: &omsi_o3d::Material,
    ) -> MaterialId {
        let name = up
            .subst
            .get(&m.texture.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| m.texture.clone());
        let tex = if is_null_texture(&m.texture) || name.trim().is_empty() || name.starts_with("\\S:") {
            None
        } else {
            self.vehicle_texture(renderer, scene, &mut up.tex_ids, &mut up.held, name.trim(), up.dirs_ref)
        };
        let mat = renderer.add_material_night(scene, tex, material_alpha(&vm.materials, slot, &vm.overrides), [1.0; 4], false, None, None);
        let mat = self.gpu.lock().material(renderer, scene, mat);
        up.materials.push(mat);
        mat
    }

}

#[cfg(test)]
mod tests {
    use super::lod_bands_of;

    /// A level whose meshes did not load is left out and the others' bands still meet.
    #[test]
    fn a_missing_middle_level_leaves_no_gap() {
        // three levels, 0.1 / 0.05 / 0 - the middle one has no meshes
        let bands = lod_bands_of(&[0.1, 0.05, 0.0], &[0, 2]);
        assert_eq!(bands, vec![(0, (0.1, f32::MAX)), (2, (0.0, 0.1))]);
        // with all three: 0.1.., 0.05..0.1, 0..0.05
        let all = lod_bands_of(&[0.1, 0.05, 0.0], &[0, 1, 2]);
        assert_eq!(all, vec![(0, (0.1, f32::MAX)), (1, (0.05, 0.1)), (2, (0.0, 0.05))]);
        // a last level left out: the one before it takes the sizes down to 0
        assert_eq!(lod_bands_of(&[0.1, 0.05, 0.0], &[0, 1]), vec![(0, (0.1, f32::MAX)), (1, (0.0, 0.1))]);
    }

    /// Every size is drawn by exactly one level of those present.
    #[test]
    fn every_size_has_a_level() {
        for present in [vec![0, 2], vec![0, 1], vec![0, 1, 2], vec![0]] {
            let bands = lod_bands_of(&[0.1, 0.05, 0.0], &present);
            for size in [0.0f32, 0.01, 0.049, 0.05, 0.07, 0.099, 0.1, 0.5, 1.0e9] {
                let hits = bands.iter().filter(|b| size >= b.1 .0 && size < b.1 .1).count();
                assert!(hits >= 1, "{present:?}: size {size} drawn by none");
            }
        }
    }
}
