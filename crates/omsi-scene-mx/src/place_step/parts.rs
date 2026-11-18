//! The pieces of an object's placement: meshes, HTML pages, LODs, water and the step log.
use super::*;

/// An object's own meshes (a crossing warped onto the ground has its own) and the meshes of
/// its `[terrainmapping]` slots: (level, mesh index, mesh).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn object_meshes(
    renderer: &Renderer,
    scene: &mut Scene,
    gpu: &mut GpuCache,
    p: &Prepared,
    tg: &mut TileGpu,
    terrain_mapping: bool,
    obj: &ObjectCx,
    (warped, type_meshes, terrain_slots): (Option<&Vec<MeshData>>, &[(MeshId, Vec<MaterialId>)], &[(usize, usize, usize)]),
    type_lods: &mut [(f32, f32, Vec<(MeshId, Vec<MaterialId>)>)],
) -> (Vec<(MeshId, Vec<MaterialId>)>, Vec<(usize, usize, MeshId)>) {
    let ObjectCx { ot, pos, xf, tkey, .. } = *obj;
    // a crossing warped onto the ground has meshes of its own
    let own_meshes: Option<Vec<(MeshId, Vec<MaterialId>)>> = warped.as_ref().map(|ms| {
        ms.iter()
            .zip(type_meshes.iter())
            .map(|(m, (_, mats))| {
                let id = gpu.add_mesh(renderer, scene, m);
                scene.meshes[id].source = Some(ot.sco.path.display().to_string());
                tg.meshes.push(id);
                (id, mats.clone())
            })
            .collect()
    });
    let mut mesh_list: Vec<(MeshId, Vec<MaterialId>)> =
        own_meshes.unwrap_or_else(|| type_meshes.to_vec());
    // [terrainmapping] slots: drawn with the uncut base ground, from a mesh
    // of this placement's own (see split_terrain_mapped); (level, mesh, id)
    let mut ground_meshes: Vec<(usize, usize, MeshId)> = Vec::new();
    if terrain_mapping {
        let mut parts: Vec<(usize, usize)> =
            terrain_slots.iter().map(|t| (t.0, t.1)).collect();
        parts.dedup();
        for (level, mi) in parts {
            let slots: Vec<usize> = terrain_slots
                .iter()
                .filter(|t| (t.0, t.1) == (level, mi))
                .map(|t| t.2)
                .collect();
            let src = if level == 0 {
                warped.as_ref().and_then(|w| w.get(mi)).or(ot.meshes.get(mi).map(|m| &m.0))
            } else {
                ot.lower_lods.get(level - 1).and_then(|l| l.1.get(mi)).map(|m| &m.0)
            };
            let Some(src) = src else { continue };
            let ground = terrain_ground(src, &slots, pos, xf, p.origin);
            if ground.is_empty() {
                continue;
            }
            // (a crossing warped onto the ground has a mesh of its own; the
            // rest of every other object is the same for all its placements)
            let rest_id = if level == 0 && warped.is_some() {
                let id = gpu.add_mesh(renderer, scene, &terrain_rest(src, &slots));
                scene.meshes[id].source = Some(ot.sco.path.display().to_string());
                tg.meshes.push(id);
                id
            } else if let Some(&(_, id)) = gpu.types[&tkey].terrain_rest.iter().find(|r| r.0 == (level, mi)) {
                id
            } else {
                let id = gpu.add_mesh(renderer, scene, &terrain_rest(src, &slots));
                scene.meshes[id].source = Some(ot.sco.path.display().to_string());
                if let Some(t) = gpu.types.get_mut(&tkey) {
                    t.terrain_rest.push(((level, mi), id));
                }
                id
            };
            let slot = if level == 0 {
                mesh_list.get_mut(mi)
            } else {
                type_lods.get_mut(level - 1).and_then(|l| l.2.get_mut(mi))
            };
            if let Some(slot) = slot {
                slot.0 = rest_id;
            }
            let ground_id = gpu.add_mesh(renderer, scene, &ground);
            scene.meshes[ground_id].source = Some(ot.sco.path.display().to_string());
            tg.meshes.push(ground_id);
            ground_meshes.push((level, mi, ground_id));
        }
    }
    (mesh_list, ground_meshes)
}

/// `[htmltexture]` + `[useHtmlTexture]` on one mesh of an object.
#[allow(clippy::too_many_arguments)]
pub(super) fn object_html_pages(
    renderer: &Renderer,
    scene: &mut Scene,
    gpu: &mut GpuCache,
    tg: &mut TileGpu,
    ot: &ObjectType,
    (mi, inst): (usize, usize),
    html_pages: &mut Vec<(usize, TextureId)>,
    html_mats: &mut HashMap<usize, MaterialId>,
) {
    if let Some((_, o3d_mats, overrides)) = ot.meshes.get(mi) {
        for o in overrides.iter().filter(|o| !o.item) {
            let Some(page) = o.use_script_texture.map(|n| n.max(0) as usize) else { continue };
            let Some(def) = ot.model.html_page(page) else { continue };
            let Some(slot) = omsi_sim::vehicle::override_slot(o3d_mats, o) else { continue };
            let mat = match html_mats.get(&page) {
                Some(m) => *m,
                None => {
                    let (w, h) = (def.width.max(1) as u32, def.height.max(1) as u32);
                    let tex = gpu.add_image(
                        renderer,
                        scene,
                        // (black until the page first draws: a page far away starts later)
                        &Image { width: w, height: h, rgba: [0, 0, 0, 255].repeat((w * h) as usize), has_alpha: true },
                        false,
                    );
                    // A page declared `lit` (the word after its path, see
                    // `HtmlTextureDef::lit`) is a surface: shaded like every other material -
                    // the sun, the street's lamps, the fog, the eye's own metering - so a
                    // sign face dims with the night it stands in. Left out, the page is a
                    // display, drawn at its own brightness: an outdoor screen.
                    let mat = renderer.add_material(scene, Some(tex), text_alpha(o3d_mats, slot, overrides), [1.0; 4], !def.lit);
                    let mat = gpu.material(renderer, scene, mat);
                    tg.textures.push(tex);
                    tg.materials.push(mat);
                    html_pages.push((page, tex));
                    html_mats.insert(page, mat);
                    mat
                }
            };
            renderer.set_material(scene, inst, slot, mat);
        }
    }
}

/// The instances of an object's lower levels of detail, and the ground of its
/// `[terrainmapping]` slots on every level (the first level's go with its own instances).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn place_object_lods(
    renderer: &Renderer,
    scene: &mut Scene,
    gpu: &mut GpuCache,
    tg: &mut TileGpu,
    pl: &Placing,
    obj: &ObjectCx,
    ground_meshes: &[(usize, usize, MeshId)],
    type_lods: &[(f32, f32, Vec<(MeshId, Vec<MaterialId>)>)],
    all_instances: &mut Vec<usize>,
) -> Vec<usize> {
    let ObjectCx { ot, pos, xf, lamp, lod0_lo, lod0_max, surface, render_phase, has_lower, .. } = *obj;
    let mut lod_instances = Vec::new();
    let lod_drawn = has_lower && !surface && lamp.is_none() && ot.program.is_none();
    for &(level, _, ground_id) in ground_meshes {
        // the first level without lower ones is drawn at any size
        let range = if level == 0 {
            lod_drawn.then_some((lod0_lo, lod0_max))
        } else if lod_drawn {
            type_lods.get(level - 1).map(|l| (l.0, l.1))
        } else {
            continue;
        };
        // Keep the first ground texture on the object even where the map
        // author painted asphalt or another layer on the terrain below it.
        if let Some(mat) = pl.terrain_mapping_mat {
            let inst = if surface {
                instance!(gpu, renderer, scene, tg; renderer.add_surface_instance(scene, ground_id, pos, xf, vec![mat]))
            } else {
                instance!(gpu, renderer, scene, tg; renderer.add_instance(scene, ground_id, pos, xf, vec![mat]))
            };
            if let Some(x) = scene.instances.get_mut(inst) {
                x.decal = surface;
                x.render_phase = render_phase;
            }
            if let Some((lo, hi)) = range {
                renderer.set_lod_range(scene, inst, lo, hi);
            }
            if level == 0 {
                all_instances.push(inst);
            } else {
                lod_instances.push(inst);
            }
        }
    }
    if lod_drawn {
        for (min_size, max_size, meshes) in type_lods {
            for (mesh_id, mats) in meshes {
                let inst = instance!(gpu, renderer, scene, tg; renderer.add_instance(
                    scene,
                    *mesh_id,
                    pos,
                    xf,
                    mats.clone()
                ));
                renderer.set_lod_range(scene, inst, *min_size, *max_size);
                if let Some(x) = scene.instances.get_mut(inst).filter(|_| ot.paint) {
                    x.decal = true;
                    x.surface_bias = true;
                }
                lod_instances.push(inst);
            }
        }
    }
    lod_instances
}

/// The water of a tile (`p.water`).
pub(super) fn place_water(renderer: &Renderer, scene: &mut Scene, gpu: &mut GpuCache, p: &Prepared, tg: &mut TileGpu, water_mat: MaterialId) {
    // the tile's water surface: one quad at the four corner heights, drawn over the
    // riverbed. OMSI keeps it in `tile.map.water`, one height per corner.
    if let Some(h) = p.water {
        let wm = water_quad(h, tile_size() as f32);
        let wid = gpu.add_mesh(renderer, scene, &wm);
        tg.meshes.push(wid);
        if omsi_cfg::flags::OMSI_DEBUG_SURFACES.is_set() {
            log::info!("tile ({}, {}): water at {:.1}..{:.1} m, centred ({:.0}, {:.0})", p.tx, p.ty, h.iter().cloned().fold(f32::MAX, f32::min), h.iter().cloned().fold(f32::MIN, f32::max), p.origin.x + tile_size() / 2.0, p.origin.y + tile_size() / 2.0);
        }
        // an ordinary instance, not a surface: the surface depth bias would let a
        // tile-wide water quad win the depth test against the banks and flood the
        // whole tile when seen at a shallow angle
        let _ = instance!(gpu, renderer, scene, tg; renderer.add_instance(
            scene,
            wid,
            p.origin,
            Mat4::IDENTITY,
            vec![water_mat]
        ));
    }
}

/// A tile's water quad at its four corner heights (corners 0 (0, 0), 1 (t, 0), 2 (0, t),
/// 3 (t, t)). It is split along the diagonal from corner 0 to corner 3, as Omsi.exe's
/// index buffer (0x7abf2c: 0 3 1, 0 2 3; here wound the other way round for our
/// counter-clockwise front faces). On a sloping water surface the other diagonal gives
/// the middle another height: Tettau's water rose into the road.
fn water_quad(h: [f32; 4], t: f32) -> MeshData {
    let mut wm = MeshData::default();
    for (i, (x, y)) in [(0.0, 0.0), (t, 0.0), (0.0, t), (t, t)]
        .into_iter()
        .enumerate()
    {
        wm.positions.push(glam::Vec3::new(x, y, h[i]));
        wm.normals.push(glam::Vec3::Z);
        wm.uvs.push(glam::Vec2::new(x / 40.0, y / 40.0));
    }
    wm.indices.extend_from_slice(&[0, 1, 3, 0, 3, 2]);
    wm.ranges.push((0, 6, 0));
    wm
}

/// OMSI_PROFILE: what one [`World::place_step`] took.
pub(super) fn log_place_step(
    gpu: &GpuCache,
    pl: &Placing,
    key: (i32, i32),
    decodes_before: (usize, f64),
    t_start: std::time::Instant,
    lock_wait: f64,
    done: bool,
) {
    if omsi_cfg::flags::OMSI_PROFILE.is_set() {
        let took = t_start.elapsed().as_secs_f64();
        let decodes = (
            gpu.sync_decodes - decodes_before.0,
            gpu.sync_decode_secs - decodes_before.1,
        );
        if took + lock_wait > 0.02 || decodes.0 > 0 {
            log::info!(
                "place tile ({}, {}): {:.1} ms this step (+{:.1} ms waiting for the GPU cache), {} textures decoded here in {:.1} ms; so far ground {:.1} ms, {} splines {:.1} ms, {} trees {:.1} ms, {} objects {:.1} ms{}",
                key.0,
                key.1,
                took * 1000.0,
                lock_wait * 1000.0,
                decodes.0,
                decodes.1 * 1000.0,
                pl.secs[0] * 1000.0,
                pl.splines,
                pl.secs[1] * 1000.0,
                pl.trees,
                pl.secs[2] * 1000.0,
                pl.objects,
                pl.secs[3] * 1000.0,
                if done { ", done" } else { "" }
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::water_quad;

    #[test]
    fn water_quad_splits_from_corner_0_to_corner_3() {
        // a water surface sloping on one diagonal only: corners 1 and 2 low, 0 and 3 high
        let q = water_quad([2.0, 0.0, 0.0, 2.0], 300.0);
        let tris: Vec<[u32; 3]> = q.indices.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
        assert_eq!(tris.len(), 2);
        // both triangles share the edge 0-3, neither uses the edge 1-2
        for t in &tris {
            assert!(t.contains(&0) && t.contains(&3), "{t:?}");
        }
        // the quad's middle lies on the 0-3 edge: at the high corners' height (2 m), not
        // at the low corners' 0 m the other diagonal gave
        let mid = (q.positions[0] + q.positions[3]) / 2.0;
        assert_eq!(mid.z, 2.0);
        // and both face up (counter-clockwise seen from above)
        for t in &tris {
            let [a, b, c] = t.map(|i| q.positions[i as usize]);
            assert!((b - a).cross(c - a).z > 0.0, "{t:?} faces down");
        }
    }
}
