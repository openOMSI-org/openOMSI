//! The sun's shadow cascades of a frame: which are redrawn (and with which light
//! matrices), and what each of them draws.

use super::*;
use super::shadow::{lamp_item_hash, LampSigs};

/// The shadow cascades of a frame (see `Renderer::plan_shadow_cache`).
pub(crate) struct ShadowPlan {
    pub moon_shadows: bool,
    pub shadows: bool,
    pub draw_shadows: bool,
    pub redraw_near: bool,
    pub redraw_far: bool,
    pub light_view_proj: Mat4,
    pub light_view_proj_far: Mat4,
    pub light_view_proj_close: Mat4,
    /// The precipitation's roof map this frame (`passes::precip_map`): the window's own
    /// frame while it rains or snows.
    pub precip: Option<Mat4>,
}

/// The light matrix of a cascade: an orthographic box `range` metres either side of the
/// camera, looking along `sun`, drawn into a map of `texels` a side. The box is a fixed size
/// whatever the view (no shimmer from a box that grows and shrinks as the camera turns), and
/// its centre moves in whole texels of its own map, so that a still caster stays on the same
/// texels while the camera moves. (The close cascade's map is smaller than the shadow setting
/// above 2048, `SHADOW_CLOSE_MAX`: snapped to the setting's texels, it had moved by half or a
/// quarter of its own texel, and every shadow edge round the bus trembled as the camera moved.)
pub(crate) fn cascade_matrix(sun: Vec3, cam_rel: Vec3, range: f32, texels: u32) -> Mat4 {
    let texel = range * 2.0 / texels.max(1) as f32;
    let up = if sun.z.abs() > 0.95 { Vec3::Y } else { Vec3::Z };
    let view0 = Mat4::look_at_rh(sun * 900.0, Vec3::ZERO, up);
    let ls = view0.transform_point3(cam_rel);
    let snapped = glam::Vec2::new((ls.x / texel).round() * texel, (ls.y / texel).round() * texel);
    // (the snapped centre as a translation in light space, the eye 900 m from it towards the
    // sun: built as a new look-at instead, its dot products with a 900 m eye rounded the
    // offset off the texel grid)
    let view = Mat4::from_translation(Vec3::new(-snapped.x, -snapped.y, -900.0 - ls.z)) * view0;
    let proj = Mat4::orthographic_rh(-range, range, -range, range, 1.0, 2200.0);
    proj * view
}

/// A window frame's place in the cycle of the shadow cascades' redraws (the frames counted
/// in `ShadowRes::shadow_near_cache`): the near cascade on even frames, the far one on
/// every other odd frame, so that every frame draws one of them at most - with the sky
/// cube's slice a frame (`SkySched`), the periodic passes spread evenly instead of piling
/// up on one frame of four.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShadowPhase(pub u32);

impl ShadowPhase {
    pub(crate) fn near_due(self) -> bool {
        self.0 % 2 == 0
    }
    pub(crate) fn far_due(self) -> bool {
        self.0 % 4 == 1
    }
}

impl Renderer {
    /// The light matrices of the cascades, the near and far ones kept from an earlier
    /// frame while they still fit (and the second XR eye taking the first one's).
    pub(crate) fn plan_shadow_cache(&self, scene: &Scene, f: &FrameCtx) -> ShadowPlan {
        let (lighting, cam_rel, projection, second_eye) = (f.lighting, f.cam_rel, f.projection, f.second_eye);
        let enhanced_frame = f.enhanced_frame;
        // sun shadow map: an orthographic box around the camera, looking along the sun (by
        // night along the moon, see `Lighting::casts_moon_shadows`)
        let moon_shadows = enhanced_frame && lighting.casts_moon_shadows();
        let sun = if moon_shadows { lighting.moon_dir.normalize_or_zero() } else { lighting.sun_dir.normalize_or_zero() };
        let casts = lighting.casts_sun_shadows() || moon_shadows;
        // A mirror's picture (a texture, no overlays) has no shadow pass: it looks the window's
        // cascades up as they were last drawn, with their own matrices. They are the sun's
        // for this origin, or the mirror has no shadows (the first frames, a moved origin).
        if !f.with_overlays && projection.is_none() && f.exclude_texture.is_some() && (self.options.mirror_shadows || self.options.mirror_enhanced) && casts {
            let (near, _, near_origin, near_sun) = self.shadows.shadow_near_cache.get();
            let (far, _, far_origin, far_sun) = self.shadows.shadow_far_cache.get();
            let (close, close_origin, close_sun) = self.shadows.shadow_close_cache.get();
            let same = |o: DVec3, s: Vec3| o == scene.render_origin && s.dot(sun) > 0.99999;
            let kept = near != Mat4::IDENTITY && far != Mat4::IDENTITY && close != Mat4::IDENTITY && same(near_origin, near_sun) && same(far_origin, far_sun) && same(close_origin, close_sun);
            return ShadowPlan {
                precip: None,
                moon_shadows,
                shadows: kept,
                draw_shadows: false,
                redraw_near: false,
                redraw_far: false,
                light_view_proj: near,
                light_view_proj_far: far,
                light_view_proj_close: close,
            };
        }
        let shadows = (f.with_overlays || projection.is_some()) && casts;
        let shared_xr_shadows = if second_eye && shadows {
            self.shadows.xr_shadow_cache
                .get()
                .filter(|(origin, previous_sun, _, _, _)| *origin == scene.render_origin && *previous_sun == sun)
        } else {
            None
        };
        let draw_shadows = shadows && shared_xr_shadows.is_none();
        let light_matrix = |range: f32, texels: u32| cascade_matrix(sun, cam_rel, range, texels);
        // The near cascade (140 m, 4096 texels at the top setting: the costliest shadow
        // pass, a third of it the trees' leaf cards) is drawn every other frame and kept
        // for the next, with the light matrix it was drawn with; the close one - the bus
        // and everything within 30 m - every frame. Redrawn at once when the camera has
        // jumped, the sun has moved or the render origin has (its matrix is relative to it).
        // Which frames is fixed by the frame's phase (see `ShadowPhase`), not by the
        // cascades' ages: counted by age, a redraw out of turn moved the near cascade onto
        // the far one's frames, and the two costliest shadow passes fell on one frame.
        let near_wanted = light_matrix(SHADOW_RANGE, self.options.shadow_size);
        let (near_m, frames, near_origin, near_sun) = self.shadows.shadow_near_cache.get();
        let phase = ShadowPhase(frames);
        let near_jumped = (near_m.project_point3(cam_rel) - near_wanted.project_point3(cam_rel)).length() > 0.03;
        let redraw_near = draw_shadows
            && (phase.near_due()
                || near_jumped
                || near_m == Mat4::IDENTITY
                || near_origin != scene.render_origin
                || near_sun.dot(sun) < 0.99999
                || f.env.shadow_near_every_frame);
        let light_view_proj = if let Some((_, _, near, _, _)) = shared_xr_shadows {
            near
        } else if !shadows {
            near_wanted
        } else if redraw_near {
            self.shadows.shadow_near_cache.set((near_wanted, frames.wrapping_add(1), scene.render_origin, sun));
            near_wanted
        } else {
            self.shadows.shadow_near_cache.set((near_m, frames.wrapping_add(1), near_origin, near_sun));
            near_m
        };
        let light_view_proj_close = shared_xr_shadows
            .map(|(_, _, _, _, close)| close)
            .unwrap_or_else(|| light_matrix(SHADOW_RANGE_CLOSE, self.options.shadow_size.min(SHADOW_CLOSE_MAX)));
        // The far cascade (700 m, metre-sized texels) is drawn every 4th frame (one of the
        // frames that keep the near cascade, see `ShadowPhase`), or at once
        // when the camera has left the middle of the one drawn, the sun has moved or the
        // render origin has jumped (its matrix is relative to that). Drawn every frame it
        // was 0.6 ms of GPU time for a picture that hardly changes; a car in it is a few
        // texels, and a tile streamed in waits three frames at most for its shadow.
        let far_wanted = light_matrix(SHADOW_RANGE_FAR, self.options.shadow_size);
        let (far_m, far_age, far_origin, far_sun) = self.shadows.shadow_far_cache.get();
        let far_moved = (far_m.project_point3(cam_rel) - far_wanted.project_point3(cam_rel)).length() > 0.12;
        let redraw_far = draw_shadows
            && (phase.far_due()
                || far_age >= 3
                || far_moved
                || far_m == Mat4::IDENTITY
                || far_origin != scene.render_origin
                || far_sun.dot(sun) < 0.99999
                || f.env.shadow_far_every_frame);
        if redraw_far && f.env.debug_shadow_far {
            log::info!("far shadow redrawn: age {far_age} moved {far_moved} origin {} sun {:.6}", far_origin != scene.render_origin, far_sun.dot(sun));
        }
        // The desktop view and the first XR eye maintain this cache; mirrors and
        // the second XR eye leave it alone.
        let light_view_proj_far = if let Some((_, _, _, far, _)) = shared_xr_shadows {
            far
        } else if !shadows {
            far_wanted
        } else if redraw_far {
            self.shadows.shadow_far_cache.set((far_wanted, 0, scene.render_origin, sun));
            far_wanted
        } else {
            self.shadows.shadow_far_cache.set((far_m, far_age + 1, far_origin, far_sun));
            far_m
        };
        if shadows && f.with_overlays && projection.is_none() {
            self.shadows.shadow_close_cache.set((light_view_proj_close, scene.render_origin, sun));
        }
        if projection.is_some() && !second_eye && shadows {
            self.shadows.xr_shadow_cache.set(Some((
                scene.render_origin,
                sun,
                light_view_proj,
                light_view_proj_far,
                light_view_proj_close,
            )));
        }
        let raining = scene.rainfall.rate > 0.001 && !f.env.no_rain && self.main_pass(f.enhanced, f.reflection_frame).rain_fall_pipeline.is_some();
        let snowing = lighting.snowfall > 0.01 && !f.env.no_snowfall && self.main_pass(f.enhanced, f.reflection_frame).snow_pipeline.is_some();
        let precip = ((raining || snowing) && f.with_overlays && projection.is_none() && f.exclude_texture.is_none() && !f.env.no_precip_roof).then(|| precip_matrix(cam_rel));
        ShadowPlan {
            precip,
            moon_shadows,
            shadows,
            draw_shadows,
            redraw_near,
            redraw_far,
            light_view_proj,
            light_view_proj_far,
            light_view_proj_close,
        }
    }

    /// The shadow casters of each cascade (near, far, close) and of the street lamps'
    /// maps, as batches over the frame's draw list.
    pub(crate) fn plan_shadow_casters(&self, scene: &Scene, f: &FrameCtx, sh: &ShadowPlan, list: &mut Vec<u32>) -> [Vec<Batch>; SHADOW_LISTS] {
        let (camera, cam_rel, rt_frame) = (f.camera, f.cam_rel, f.rt_frame);
        let (moon_shadows, draw_shadows, redraw_near, redraw_far) = (sh.moon_shadows, sh.draw_shadows, sh.redraw_near, sh.redraw_far);
        let (light_view_proj, light_view_proj_far, light_view_proj_close) = (sh.light_view_proj, sh.light_view_proj_far, sh.light_view_proj_close);
        let lamp_shadows = &f.lamp_shadows;
        let debug_draws = f.env.debug_draws;
        // near, far, close
        // (3..: each street lamp's map, every caster within that lamp's reach under its head;
        // a source embedded in a pole's fixture leaves its own fixture out, see `PointLight::shadow_owner`)
        let mut shadow_batches: [Vec<Batch>; SHADOW_LISTS] = std::array::from_fn(|_| Vec::new());
        let lamp_reaches = |l: &LampShadow, c: Vec3, r: f32| l.reaches(c, r);
        // (a sphere round every lamp's reach, tried first: most of the instances in a lit block
        // are out of all the forty-four maps' reach at once)
        let lamp_union = LampShadow::union_sphere(lamp_shadows);
        let lamp_reach = |c: Vec3, r: f32| lamp_union.is_none_or(|(uc, ur)| (c - uc).length() < ur + r) && lamp_shadows.iter().any(|l| lamp_reaches(l, c, r));
        // (what stands still in the player's vehicle, for its saloon lamps' tiles)
        if let Some(cabin) = lamp_shadows.iter().find_map(|l| l.cabin.as_ref()) {
            self.shadows.cabin_motion.borrow_mut().update(scene, cabin);
        }
        let motion = self.shadows.cabin_motion.borrow();
        let still: &[bool] = &motion.still;
        // an instance's screen size as the camera pass measures it for the LOD choice
        let lod_fov = camera.fov_deg.to_radians().max(1e-3);
        let lod_size = |inst: &Instance| -> f32 {
            let scale = Self::instance_scale(scene, inst);
            let radius = if inst.object_radius > 0.0 { inst.object_radius } else { scene.meshes[inst.mesh].bounds_radius } * scale;
            let d = ((inst.origin - scene.render_origin).as_vec3() - cam_rel).length();
            if d <= radius { f32::MAX } else { 2.0 * radius / (d.max(0.01) * lod_fov) }
        };
        let active = [draw_shadows && redraw_near, draw_shadows && redraw_far, draw_shadows];
        let boxes = [(SHADOW_RANGE, light_view_proj, 0.4f32), (SHADOW_RANGE_FAR, light_view_proj_far, 6.0), (SHADOW_RANGE_CLOSE, light_view_proj_close, 0.1)];
        // (the precipitation's roof map: what stands over the ground round the camera)
        let precip_box = sh.precip;
        let dbg_shadow = f.env.debug_shadow;
        let dbg_r: f32 = f
            .env
            .debug_shadow_r
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(3.0);
        // (a copy: the casters are gathered on the worker pool, the renderer is not shared)
        let omsi_shadow_casters = self.options.omsi_shadow_casters;
        // (the lists are filled in place, block after block of the instances: a fresh list
        // per block was a few hundred small allocations a frame and every item copied twice)
        // (what each street lamp's tile draws, summed up: the tile is kept while it stays the
        // same, see `LampTiles`; bending foliage changes every frame)
        let windy = f.lighting.windy_trees;
        let casters = |span: std::ops::Range<usize>, out: &mut [Vec<DrawItem>; SHADOW_LISTS], ranges: &mut Vec<(u8, u32, u32, usize, u32)>, sig: &mut LampSigs| {
            let first = span.start;
            for (at, inst) in scene.instances[span].iter().enumerate() {
                // (the roof map takes the ground and the roads as well, which cast no shadow:
                // the splashes land on them, see rain.wgsl `splash`)
                let caster = inst.casts_shadow && !(omsi_shadow_casters && !inst.omsi_caster);
                if !inst.visible || (!caster && precip_box.is_none()) {
                    continue;
                }
                // a stand-in for far tiles (`set_near_only`) left out of the picture casts no
                // shadow either: London's bridge lamps, hidden as part of a backdrop, still
                // threw their shadows on the deck (#1545)
                if let Some([x0, y0, x1, y1]) = inst.near_only {
                    let c = camera.position;
                    if c.x < x0 || c.x > x1 || c.y < y0 || c.y > y1 {
                        continue;
                    }
                }
                let m = &scene.meshes[inst.mesh];
                if m.ranges.is_empty() {
                    continue;
                }
                let (c, r) = Self::bounding_sphere(scene, inst);
                if inst.lod.0 > 0.0 || inst.lod.1 < f32::MAX {
                    let size = lod_size(inst);
                    if size < inst.lod.0 || (inst.lod.1 < f32::MAX && size >= inst.lod.1) {
                        if dbg_shadow && r >= dbg_r {
                            log::info!("shadow: mesh r={r:.1} at {:?} is another LOD than the one shown", inst.origin);
                        }
                        continue;
                    }
                }
                // Which maps the instance falls into, before its materials are looked at: on
                // most frames only the close cascade (30 m round the camera) is drawn, and
                // most of a city's instances lie outside it - looking up the materials of
                // every one of them first was most of this planning's time.
                let lamp_hit = caster && !lamp_shadows.is_empty() && !(m.bounds_radius > 0.0 && m.bounds_radius < 0.1) && lamp_reach(c, r);
                let mut hits = [false; 3];
                for (cascade, &(range, lvp, min_radius)) in boxes.iter().enumerate() {
                    if !active[cascade] {
                        continue;
                    }
                    if m.bounds_radius > 0.0 && m.bounds_radius < min_radius {
                        if dbg_shadow && cascade == 0 && m.bounds_radius >= dbg_r {
                            log::info!("shadow: mesh r={:.1} skipped (ranges {})", m.bounds_radius, m.ranges.len());
                        }
                        continue;
                    }
                    let lc = lvp.project_point3(c);
                    let rr = r / range;
                    if lc.x.abs() > 1.0 + rr || lc.y.abs() > 1.0 + rr {
                        if dbg_shadow && cascade == 0 && r >= dbg_r {
                            log::info!("shadow: mesh r={r:.1} outside the light box at ({:.2}, {:.2})", lc.x, lc.y);
                        }
                        continue;
                    }
                    hits[cascade] = caster;
                }
                // (a pole or a sign is no roof: only what is a fair size)
                let roof_hit = precip_box.is_some_and(|pm| {
                    let lc = pm.project_point3(c);
                    let rr = r / PRECIP_RANGE;
                    !(m.bounds_radius > 0.0 && m.bounds_radius < 0.25) && lc.x.abs() <= 1.0 + rr && lc.y.abs() <= 1.0 + rr
                });
                if !lamp_hit && !roof_hit && hits == [false; 3] {
                    continue;
                }
                ranges.clear();
                for (ri, (_, _, slot)) in m.ranges.iter().enumerate() {
                    let mat_id = inst.materials.get(*slot as usize).copied().unwrap_or(0);
                    let mat = &scene.materials[mat_id];
                    let mut kind = kind_of(mat.alpha);
                    let cut_body = kind == PIPE_BLEND && mat.transmap.is_some() && !mat.no_z_write;
                    if (kind == PIPE_BLEND && !cut_body) || mat.no_z_check {
                        if dbg_shadow && r >= dbg_r {
                            log::info!("shadow: mesh r={r:.1} at {:?} slot {slot} is blended, never a caster", inst.origin);
                        }
                        continue;
                    }
                    if cut_body {
                        kind = PIPE_ALPHA_TEST;
                    }
                    // (Enhanced+: what is solid casts its shadow by the traced rays; the
                    // close and near maps keep the cut-out leaves and fences, whose texels
                    // the rays cannot see)
                    if rt_frame && kind == PIPE_OPAQUE && !moon_shadows {
                        kind = PIPE_KINDS;
                    }
                    ranges.push((kind, ri as u32, *slot, mat_id, mat.look));
                }
                if lamp_hit {
                    // (each lamp's map takes the casters within its own reach: one list
                    // drawn into every map cost each lamp the others' casters as well)
                    for light in lamp_shadows.iter() {
                        let k = light.slot;
                        if (light.owner.is_some() && light.owner == inst.shadow_owner) || !lamp_reaches(light, c, r) {
                            continue;
                        }
                        // (the parts of the vehicle whose headlamps these are cast nothing: the
                        // lamps stand in them)
                        if light.hosts && inst.roof.is_some() && (c - light.position).length() < r + 0.4 {
                            continue;
                        }
                        // (a saloon lamp's tile holds what stands still in the vehicle, see `CabinMotion`)
                        if light.cabin.is_some() && !still.get(first + at).copied().unwrap_or(false) {
                            continue;
                        }
                        for &(kind, ri, slot, mat_id, look) in ranges.iter() {
                            let kind = if kind == PIPE_KINDS { PIPE_OPAQUE } else { kind };
                            let (material, look) = depth_only_material(kind, mat_id, look);
                            out[3 + k].push(DrawItem { pipe: kind, mesh: inst.mesh as u32, range: ri, material, look, entry: (inst.base + slot) | ((k as u32) << LAMP_ENTRY_BITS) });
                            let alpha = inst.slot_alpha.get(slot as usize).copied().unwrap_or(1.0);
                            sig.add(k, lamp_item_hash(inst, m.pose, kind, ri, material, look, alpha, light.cabin.as_ref().map(|c| (&c.to_local, scene.render_origin))));
                            if windy && scene.materials[mat_id].uniform.sway[0] >= 0.5 {
                                sig.volatile |= 1u64 << k;
                            }
                        }
                    }
                }
                if roof_hit {
                    for &(kind, ri, slot, mat_id, look) in ranges.iter() {
                        let kind = if kind == PIPE_KINDS { PIPE_OPAQUE } else { kind };
                        let (material, look) = depth_only_material(kind, mat_id, look);
                        out[PRECIP_LIST].push(DrawItem { pipe: kind, mesh: inst.mesh as u32, range: ri, material, look, entry: inst.base + slot });
                    }
                }
                for cascade in 0..3 {
                    if !hits[cascade] {
                        continue;
                    }
                    if dbg_shadow && cascade == 0 && r >= dbg_r {
                        log::info!("shadow: caster r={r:.1} at {:?} slots {:?}", inst.origin, ranges.iter().map(|x| x.0).collect::<Vec<_>>());
                    }
                    for &(kind, ri, slot, mat_id, look) in ranges.iter() {
                        let kind = if kind == PIPE_KINDS {
                            if cascade != 1 {
                                continue;
                            }
                            PIPE_OPAQUE
                        } else {
                            kind
                        };
                        // (after the remap: a traced-opaque one is drawn as the opaque it is)
                        let (material, look) = depth_only_material(kind, mat_id, look);
                        out[cascade].push(DrawItem {
                            pipe: kind,
                            mesh: inst.mesh as u32,
                            range: ri,
                            material,
                            look,
                            entry: inst.base + slot,
                        });
                    }
                }
            }
        };
        if active.iter().any(|a| *a) || !lamp_shadows.is_empty() || precip_box.is_some() {
            let n = scene.instances.len();
            let parts = instance_parts(self.encoding_pool.as_ref(), n);
            let chunk = n.div_ceil(parts).div_ceil(CULL_BLOCK) * CULL_BLOCK;
            let blocks = Self::cull_blocks(scene);
            let lit = |b: usize| {
                blocks.as_ref().is_none_or(|bl| {
                    let (c, r) = bl[b];
                    lamp_reach(c, r) || precip_box.is_some_and(|pm| {
                        let lc = pm.project_point3(c);
                        let rr = r / PRECIP_RANGE;
                        lc.x.abs() <= 1.0 + rr && lc.y.abs() <= 1.0 + rr
                    }) || boxes.iter().enumerate().any(|(k, &(range, lvp, _))| {
                        let lc = lvp.project_point3(c);
                        let rr = r / range;
                        active[k] && lc.x.abs() <= 1.0 + rr && lc.y.abs() <= 1.0 + rr
                    })
                })
            };
            let mut found: [Vec<DrawItem>; SHADOW_LISTS] = std::array::from_fn(|_| Vec::new());
            let mut sigs = LampSigs::default();
            for (part, sig) in run_parts(self.encoding_pool.as_ref(), parts, |p| {
                let mut out: [Vec<DrawItem>; SHADOW_LISTS] = std::array::from_fn(|_| Vec::new());
                let mut sig = LampSigs::default();
                let mut ranges = Vec::new();
                let end = ((p + 1) * chunk).min(n);
                let mut b = p * chunk;
                while b < end {
                    let next = (b + CULL_BLOCK).min(end);
                    if lit(b / CULL_BLOCK) {
                        casters(b..next, &mut out, &mut ranges, &mut sig);
                    }
                    b = next;
                }
                (out, sig)
            }) {
                for (a, b) in found.iter_mut().zip(part) {
                    a.extend(b);
                }
                sigs.merge(&sig);
            }
            if !lamp_shadows.is_empty() {
                let mut tiles = self.shadows.lamp_tiles.get();
                tiles.frame = sigs;
                self.shadows.lamp_tiles.set(tiles);
            }
            for cascade in 0..SHADOW_LISTS {
                if cascade < 3 && !active[cascade] {
                    continue;
                }
                if debug_draws {
                    log::info!("shadow cascade {cascade}: {} draws", found[cascade].len());
                }
                batch_items(scene, &mut found[cascade], true, list, &mut shadow_batches[cascade]);
            }
        }
        if sh.shadows && debug_draws {
            log::info!(
                "shadow passes: {} batches",
                shadow_batches.iter().map(|b| b.len()).sum::<usize>()
            );
        }
        shadow_batches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The near cascade every other frame, the far one every fourth, never together.
    #[test]
    fn cascades_take_turns() {
        let (mut near, mut far) = (0, 0);
        for n in 0..400 {
            let p = ShadowPhase(n);
            assert!(!(p.near_due() && p.far_due()), "frame {n}");
            near += p.near_due() as u32;
            far += p.far_due() as u32;
        }
        assert_eq!((near, far), (200, 100));
    }

    /// A still point keeps its place among the map's texels while the camera moves by
    /// fractions of a texel and turns: the cascade moves by whole texels only.
    #[test]
    fn cascades_move_by_whole_texels() {
        let sun = Vec3::new(0.4, -0.5, 0.75).normalize();
        let point = Vec3::new(3.3, -7.1, 0.4);
        for (range, texels) in [(SHADOW_RANGE_CLOSE, 2048u32), (SHADOW_RANGE, 4096), (SHADOW_RANGE_FAR, 2048)] {
            let frac = |cam: Vec3| {
                let p = cascade_matrix(sun, cam, range, texels).project_point3(point);
                let t = (glam::Vec2::new(p.x, p.y) * 0.5 + 0.5) * texels as f32;
                t - t.round()
            };
            let f0 = frac(Vec3::ZERO);
            for k in 1..40 {
                let cam = Vec3::new(0.013 * k as f32, -0.021 * k as f32, 0.005 * k as f32);
                assert!((frac(cam) - f0).abs().max_element() < 0.02, "range {range}: {:?} vs {:?}", frac(cam), f0);
            }
        }
    }
}
