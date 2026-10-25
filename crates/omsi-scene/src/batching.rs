//! Static spline batching and the GPU cache of meshes, textures and materials.
use super::*;
use crate::spline_layers::SplineLayer;

/// Short static spline segments with matching materials share a mesh within a 48 m cell.
/// Their coordinates, material order, terrain mapping and shadow flag stay intact; long segments retain
/// their own culling bounds. The original meshes remain in the staging/collision data.
pub(super) fn batch_static_splines(
    splines: Vec<(Arc<MeshData>, Arc<SplineType>, bool, DVec3, SplineLayer)>,
) -> Vec<(Arc<MeshData>, Arc<SplineType>, bool, DVec3, SplineLayer)> {
    if omsi_cfg::flags::OMSI_NO_SPLINE_BATCHING.is_set() {
        return splines;
    }
    let mut groups: Vec<(Vec<Arc<MeshData>>, Arc<SplineType>, bool, DVec3, SplineLayer)> = Vec::new();
    let mut cells = HashMap::new();
    let mut signatures = HashMap::new();
    let mut type_materials = HashMap::new();
    let material_batching = !omsi_cfg::flags::OMSI_NO_MATERIAL_SPLINE_BATCHING.is_set();
    for (mesh, ty, casts, sort_origin, level) in splines {
        let (lo, hi) = mesh.positions.iter().fold(
            (glam::Vec3::splat(f32::INFINITY), glam::Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), &p| (lo.min(p), hi.max(p)),
        );
        let centre = (lo + hi) * 0.5;
        // Blended segments retain their individual placement origins and draw order.
        let blended = mesh.ranges.iter().any(|r| ty.def.textures.get(r.2 as usize).is_some_and(|t| t.alpha >= 2));
        let short = !blended && centre.is_finite() && (hi - lo).length() <= 48.0;
        let group = if short {
            let slots: Vec<_> = mesh.ranges.iter().map(|r| r.2).collect();
            // UV generation is already complete. Only the textures of the remaining
            // ranges matter now; an unused terrain slot must not split identical curbs.
            // Keep the lookup directory and alpha mode exact so similarly named files
            // in different content packs cannot be combined.
            let material_type = if material_batching && slots.iter().all(|&s| (s as usize) < ty.def.textures.len()) {
                (true, *type_materials.entry((Arc::as_ptr(&ty) as usize, slots.clone())).or_insert_with(|| {
                    let signature = (ty.dir.clone(), slots.iter().map(|&s| {
                        let texture = &ty.def.textures[s as usize];
                        (s, texture.file.clone(), texture.alpha)
                    }).collect::<Vec<_>>());
                    let next = signatures.len();
                    *signatures.entry(signature).or_insert(next)
                }))
            } else {
                (false, Arc::as_ptr(&ty) as usize)
            };
            let key = (
                material_type,
                (centre.x / 48.0).floor() as i32,
                (centre.y / 48.0).floor() as i32,
                casts,
                mesh.one_sided,
                slots,
                // (a layer on another spline or on a surface object keeps its depth apart)
                level,
            );
            *cells.entry(key).or_insert_with(|| {
                let i = groups.len();
                groups.push((Vec::new(), ty.clone(), casts, sort_origin, level));
                i
            })
        } else {
            let i = groups.len();
            groups.push((Vec::new(), ty, casts, sort_origin, level));
            i
        };
        groups[group].0.push(mesh);
    }
    groups.into_iter().map(|(mut meshes, ty, casts, sort_origin, level)| {
        let mesh = if meshes.len() == 1 {
            meshes.pop().unwrap()
        } else {
            Arc::new(MeshData::merge_static(&meshes.iter().map(AsRef::as_ref).collect::<Vec<_>>()))
        };
        (mesh, ty, casts, sort_origin, level)
    }).collect()
}

/// These faces all use the tile's ground materials, irrespective of the source .sli.
/// Pool them before upload so grass widths and curb types can share a ground draw.
pub(super) fn batch_ground_splines(meshes: Vec<Arc<MeshData>>) -> Vec<Arc<MeshData>> {
    let mut groups: Vec<Vec<Arc<MeshData>>> = Vec::new();
    let mut cells = HashMap::new();
    for mesh in meshes {
        let (lo, hi) = mesh.positions.iter().fold(
            (glam::Vec3::splat(f32::INFINITY), glam::Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), &p| (lo.min(p), hi.max(p)),
        );
        let centre = (lo + hi) * 0.5;
        let group = if centre.is_finite() && (hi - lo).length() <= 48.0 {
            let key = (
                (centre.x / 48.0).floor() as i32,
                (centre.y / 48.0).floor() as i32,
                (centre.z / 48.0).floor() as i32,
                mesh.one_sided,
            );
            *cells.entry(key).or_insert_with(|| {
                let i = groups.len();
                groups.push(Vec::new());
                i
            })
        } else {
            let i = groups.len();
            groups.push(Vec::new());
            i
        };
        groups[group].push(mesh);
    }
    groups.into_iter().map(|mut meshes| {
        if meshes.len() == 1 {
            meshes.pop().unwrap()
        } else {
            Arc::new(MeshData::merge_static(&meshes.iter().map(AsRef::as_ref).collect::<Vec<_>>()))
        }
    }).collect()
}

/// The GPU side of the loaded tiles: shared resources with their users, and the freed ids
/// that new resources take over.
#[derive(Default)]
pub struct GpuCache {
    pub(super) textures: hashbrown::HashMap<PathBuf, TexEntry>,
    pub(super) misses: hashbrown::HashSet<String>,
    pub(super) types: HashMap<usize, TypeGpu>,
    pub(super) splines: HashMap<usize, SplineGpu>,
    pub(super) trees: HashMap<String, TreeGpu>,
    pub(super) ground: Option<GroundGpu>,
    pub(super) free_meshes: FreeList,
    pub(super) free_textures: FreeList,
    pub(super) free_materials: FreeList,
    /// Removed instances by material slot count.
    pub(super) free_instances: HashMap<usize, FreeList>,
    /// Textures decoded on the thread that draws (not decoded ahead on the loader): how
    /// many, and the seconds they took (`OMSI_PROFILE`).
    pub(super) sync_decodes: usize,
    pub(super) sync_decode_secs: f64,
    /// Textures read here as RGBA to spare the frame, to be compressed on a worker.
    pub(super) wants_upgrade: Vec<PathBuf>,
    /// Pictures read on the thread that draws are read the quick way (a window's frames;
    /// an offscreen load compresses them at once).
    pub(super) fast_loads: bool,
    /// Textures that lost mip levels to the budget and are near again: read again whole.
    pub(super) wants_restore: Vec<PathBuf>,
    /// Scenery sign texts (`[texttexture]`) by what they show: (texture, material, users).
    /// A street's name on twenty signs is one texture - every sign had its own, 270 MB
    /// around the Ahlheim main station.
    pub(super) text_textures: HashMap<String, (TextureId, MaterialId, usize)>,
}

impl GpuCache {
    pub(super) fn add_mesh(&mut self, renderer: &Renderer, scene: &mut Scene, data: &MeshData) -> MeshId {
        let id = renderer.add_mesh(scene, data);
        self.take_mesh_slot(renderer, scene, id)
    }

    pub(super) fn take_mesh_slot(&mut self, renderer: &Renderer, scene: &mut Scene, id: MeshId) -> MeshId {
        match self.free_meshes.pop() {
            Some(slot) => {
                let r = renderer.recycle_mesh(scene, id, slot);
                if r != slot {
                    self.free_meshes.push(slot);
                }
                r
            }
            None => id,
        }
    }

    pub(super) fn add_data(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        data: &TextureData,
    ) -> TextureId {
        let id = renderer.add_texture_data(scene, data);
        self.take_texture_slot(renderer, scene, id)
    }

    /// An RGBA picture of the scene's own (a sign's text), uploaded as it is.
    pub(super) fn add_image(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        img: &Image,
        mipmaps: bool,
    ) -> TextureId {
        let id = renderer.add_texture(scene, img, mipmaps);
        self.take_texture_slot(renderer, scene, id)
    }

    pub(super) fn add_blank(&mut self, renderer: &Renderer, scene: &mut Scene, width: u32, height: u32) -> TextureId {
        let id = renderer.add_blank_texture(scene, width, height);
        self.take_texture_slot(renderer, scene, id)
    }

    /// A transparent dynamic text texture with a full mip chain. Text textures are often
    /// viewed much smaller than their authored pixel size; without lower levels the sampler
    /// minifies level zero directly and thin glyph strokes break into unstable pixels.
    pub(super) fn add_blank_mips(&mut self, renderer: &Renderer, scene: &mut Scene, width: u32, height: u32) -> TextureId {
        let (width, height) = (width.max(1), height.max(1));
        self.add_image(
            renderer,
            scene,
            &Image {
                width,
                height,
                rgba: vec![0; (width * height * 4) as usize],
                has_alpha: true,
            },
            true,
        )
    }

    pub(super) fn take_texture_slot(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        id: TextureId,
    ) -> TextureId {
        match self.free_textures.pop() {
            Some(slot) => {
                let r = renderer.recycle_texture(scene, id, slot);
                if r != slot {
                    self.free_textures.push(slot);
                }
                r
            }
            None => id,
        }
    }

    pub(super) fn material(&mut self, renderer: &Renderer, scene: &mut Scene, id: MaterialId) -> MaterialId {
        match self.free_materials.pop() {
            Some(slot) => {
                let r = renderer.recycle_material(scene, id, slot);
                if r != slot {
                    self.free_materials.push(slot);
                }
                r
            }
            None => id,
        }
    }

    pub(super) fn instance(&mut self, renderer: &Renderer, scene: &mut Scene, id: usize) -> usize {
        let slots = renderer.instance_slots(scene, id);
        match self.free_instances.get_mut(&slots).and_then(|l| l.pop()) {
            Some(slot) => {
                let r = renderer.recycle_instance(scene, id, slot);
                if r != slot {
                    self.free_instances.entry(slots).or_default().push(slot);
                }
                r
            }
            None => id,
        }
    }

    /// A texture found by OMSI's rules, uploaded on first use; the caller becomes one of its
    /// users (and must `release_texture` the returned path).
    pub(super) fn texture(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        name: &str,
        dirs: &[PathBuf],
        images: &HashMap<PathBuf, Arc<TextureData>>,
    ) -> Option<(TextureId, PathBuf)> {
        let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
        let Some(path) = omsi_texture::find_texture(name, &dirs_ref) else {
            if !name.trim().is_empty() && self.misses.insert(name.to_string()) {
                log::warn!("Did not find texture file \"{name}\"!");
                if omsi_cfg::flags::OMSI_DEBUG_MISSING.is_set() {
                    log::info!("  texture {name} looked for in {:?}", dirs);
                }
            }
            return None;
        };
        if let Some(e) = self.textures.get_mut(&path) {
            e.users += 1;
            return Some((e.id, path));
        }
        let img = match images.get(&path) {
            Some(i) => i.clone(),
            None => {
                let t = std::time::Instant::now();
                let decoded = if self.fast_loads {
                    omsi_texture::gpu::load_gpu_fast(&path)
                } else {
                    omsi_texture::gpu::load_gpu(&path).map(|(t, _)| (t, false))
                };
                self.sync_decodes += 1;
                self.sync_decode_secs += t.elapsed().as_secs_f64();
                match decoded {
                    Ok((i, worth)) => {
                        if worth {
                            self.wants_upgrade.push(path.clone());
                            // (to be swapped for the compressed whole soon: meanwhile at half
                            // the size, a quarter of the memory - uploaded whole as RGBA, a
                            // map's first tiles took a gigabyte more than compressed, and
                            // cards with little memory ran out while the game was loading)
                            Arc::new(omsi_texture::gpu::halved_for_now(i))
                        } else {
                            Arc::new(i)
                        }
                    }
                    Err(e) => {
                        if self.misses.insert(path.to_string_lossy().into_owned()) {
                            log::warn!("{e}");
                        }
                        return None;
                    }
                }
            }
        };
        let id = self.add_data(renderer, scene, &img);
        attach_pbr(renderer, scene, &path, id);
        self.textures.insert(
            path.clone(),
            TexEntry {
                id,
                alpha: img.has_alpha,
                users: 1,
                texels: img.width as u64 * img.height as u64,
                bytes: renderer.texture_size_bytes(scene, id),
                format: img.format,
                dropped: 0,
            },
        );
        Some((id, path))
    }

    pub(super) fn has_alpha(&self, path: &Path) -> bool {
        self.textures.get(path).map(|e| e.alpha).unwrap_or(false)
    }

    /// Lazily make material overrides for one placed scenery object's active `[CTC]` and
    /// `[texchanges]` choices. The choices are cached by their complete per-group index
    /// vector so placements sharing a type and choices also share textures and materials.
    pub(super) fn dynamic_texture_variant(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        type_key: usize,
        selection: &[usize],
        root: &Path,
        images: &HashMap<PathBuf, Arc<TextureData>>,
    ) -> Option<Arc<Vec<Vec<Option<(MaterialId, MaterialId)>>>>> {
        // (shared, not copied: every scripted object with such textures asks for its rows
        // on every frame)
        if let Some(found) = self
            .types
            .get(&type_key)?
            .dynamic_texture_variants
            .get(selection)
        {
            return Some(found.clone());
        }
        let ot = self.types.get(&type_key)?.ot.clone();
        let mut replacements: HashMap<String, (String, PathBuf)> = HashMap::new();
        let mut affected_keys: HashMap<String, ()> = HashMap::new();
        for (group, &index) in ot.dynamic_textures.iter().zip(selection) {
            for choice in &group.choices {
                for (default, _, _) in choice {
                    affected_keys.insert(scenery_texture_key(default), ());
                }
            }
            if let Some(choice) = group.choices.get(index) {
                for (default, file, dir) in choice {
                    replacements.insert(scenery_texture_key(default), (file.clone(), dir.clone()));
                }
            }
        }

        let (base_meshes, base_variants) = {
            let ty = self.types.get(&type_key)?;
            (ty.meshes.clone(), ty.variants.clone())
        };
        let mut rows: Vec<Vec<Option<(MaterialId, MaterialId)>>> = base_meshes
            .iter()
            .map(|(_, materials)| vec![None; materials.len()])
            .collect();
        for (mesh_index, (_, o3d_materials, _)) in ot.meshes.iter().enumerate() {
            let Some((_, base_materials)) = base_meshes.get(mesh_index) else {
                continue;
            };
            for (slot, source) in o3d_materials.iter().enumerate() {
                let key = scenery_texture_key(&source.texture);
                if !affected_keys.contains_key(&key) {
                    continue;
                }
                let Some(&base) = base_materials.get(slot) else {
                    continue;
                };
                let item = base_variants
                    .iter()
                    .find(|v| v.0 == mesh_index && v.1 == slot)
                    .map(|v| v.3)
                    .unwrap_or(base);
                // Always retain a reset pair. An invalid index or a missing replacement
                // texture must restore the model material after a previously valid choice.
                rows[mesh_index][slot] = Some((base, item));
                let Some((file, scheme_dir)) = replacements.get(&key) else {
                    continue;
                };
                let mut dirs = ot.texture_dirs(root);
                dirs.insert(0, scheme_dir.clone());
                let Some((texture, path)) = self.texture(renderer, scene, file, &dirs, images)
                else {
                    continue;
                };
                let Some(base_ctc) = renderer.add_material_retextured(scene, base, Some(texture))
                else {
                    self.release_texture(renderer, scene, &path);
                    continue;
                };
                let item_ctc = if item == base {
                    base_ctc
                } else if let Some(mat) =
                    renderer.add_material_retextured(scene, item, Some(texture))
                {
                    mat
                } else {
                    base_ctc
                };
                let ty = self.types.get_mut(&type_key)?;
                ty.materials.push(base_ctc);
                if item_ctc != base_ctc {
                    ty.materials.push(item_ctc);
                }
                ty.textures.push(path);
                rows[mesh_index][slot] = Some((base_ctc, item_ctc));
            }
        }
        let rows = Arc::new(rows);
        self.types
            .get_mut(&type_key)?
            .dynamic_texture_variants
            .insert(selection.to_vec(), Arc::clone(&rows));
        Some(rows)
    }

    /// A `[matl_bumpmap]` height map (`omsi_texture::gpu::prepare_bump`), shared like
    /// [`GpuCache::texture`] under its own key (`bump_key`); read here, which the two
    /// stock objects with one can afford.
    pub(super) fn bump_texture(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        name: &str,
        dirs: &[PathBuf],
    ) -> Option<(TextureId, PathBuf)> {
        let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
        let path = omsi_texture::find_texture(name, &dirs_ref)?;
        let key = bump_key(&path);
        if let Some(e) = self.textures.get_mut(&key) {
            e.users += 1;
            return Some((e.id, key));
        }
        let t = std::time::Instant::now();
        let data = load_texture_key(&key, !self.fast_loads)?;
        self.sync_decodes += 1;
        self.sync_decode_secs += t.elapsed().as_secs_f64();
        let id = self.add_data(renderer, scene, &data);
        self.textures.insert(
            key.clone(),
            TexEntry {
                id,
                alpha: true,
                users: 1,
                texels: data.width as u64 * data.height as u64,
                bytes: renderer.texture_size_bytes(scene, id),
                format: data.format,
                dropped: 0,
            },
        );
        Some((id, key))
    }

    pub(super) fn release_texture(&mut self, renderer: &Renderer, scene: &mut Scene, path: &Path) {
        let Some(e) = self.textures.get_mut(path) else {
            return;
        };
        e.users = e.users.saturating_sub(1);
        if e.users == 0 {
            let id = e.id;
            self.textures.remove(path);
            renderer.free_texture(scene, id);
            self.free_textures.push(id);
        }
    }

    pub(super) fn free_material(&mut self, renderer: &Renderer, scene: &mut Scene, id: MaterialId) {
        renderer.free_material(scene, id);
        self.free_materials.push(id);
    }

    /// Give back everything a tile held.
    pub(super) fn release_tile(&mut self, renderer: &Renderer, scene: &mut Scene, tg: TileGpu) -> usize {
        let mut freed_types = 0;
        for i in tg.instances {
            renderer.remove_instance(scene, i);
            let slots = renderer.instance_slots(scene, i);
            self.free_instances.entry(slots).or_default().push(i);
        }
        for m in tg.materials {
            self.free_material(renderer, scene, m);
        }
        for m in tg.meshes {
            renderer.free_mesh(scene, m);
            self.free_meshes.push(m);
        }
        for t in tg.textures {
            renderer.free_texture(scene, t);
            self.free_textures.push(t);
        }
        for p in tg.shared_textures {
            self.release_texture(renderer, scene, &p);
        }
        for key in tg.texts {
            let gone = match self.text_textures.get_mut(&key) {
                Some(e) => {
                    e.2 = e.2.saturating_sub(1);
                    e.2 == 0
                }
                None => false,
            };
            if gone {
                let (tex, mat, _) = self.text_textures.remove(&key).unwrap();
                self.free_material(renderer, scene, mat);
                renderer.free_texture(scene, tex);
                self.free_textures.push(tex);
            }
        }
        for key in tg.types {
            let gone = match self.types.get_mut(&key) {
                Some(t) => {
                    t.users = t.users.saturating_sub(1);
                    t.users == 0
                }
                None => false,
            };
            if gone {
                let t = self.types.remove(&key).unwrap();
                let mut meshes: Vec<MeshId> = t.meshes.iter().map(|m| m.0).collect();
                meshes.extend(t.lods.iter().flat_map(|l| l.2.iter().map(|m| m.0)));
                meshes.extend(t.terrain_rest.iter().map(|r| r.1));
                for m in meshes {
                    renderer.free_mesh(scene, m);
                    self.free_meshes.push(m);
                }
                for m in t.materials {
                    self.free_material(renderer, scene, m);
                }
                for p in t.textures {
                    self.release_texture(renderer, scene, &p);
                }
                drop(t.ot);
                freed_types += 1;
            }
        }
        for key in tg.spline_types {
            let gone = match self.splines.get_mut(&key) {
                Some(s) => {
                    s.users = s.users.saturating_sub(1);
                    s.users == 0
                }
                None => false,
            };
            if gone {
                let s = self.splines.remove(&key).unwrap();
                for m in s.materials {
                    self.free_material(renderer, scene, m);
                }
                for p in s.textures {
                    self.release_texture(renderer, scene, &p);
                }
            }
        }
        for key in tg.trees {
            let gone = match self.trees.get_mut(&key) {
                Some(t) => {
                    t.users = t.users.saturating_sub(1);
                    t.users == 0
                }
                None => false,
            };
            if gone {
                let t = self.trees.remove(&key).unwrap();
                self.free_material(renderer, scene, t.material);
                if let Some(p) = t.texture {
                    self.release_texture(renderer, scene, &p);
                }
            }
        }
        freed_types
    }
}
