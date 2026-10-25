//! What the depth prepass and the main pass draw, as batches over the frame's draw list
//! (see `Batch`), in the authored world phases and far to near for the blended draws.

use super::*;

fn sort_blended_draws(keyed: &mut [(u8, f32, usize)], draw_order: impl Fn(usize) -> u64) {
    keyed.sort_unstable_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.total_cmp(&a.1))
            .then_with(|| draw_order(a.2).cmp(&draw_order(b.2)))
            .then(a.2.cmp(&b.2))
    });
}

/// The batches of the camera's passes (the shadow casters' are planned apart, see
/// `plan_shadow_casters`).
pub(crate) struct DrawPlan {
    /// the frame's draw list: each batch's per-draw entries (the shadow casters' first)
    pub list: Vec<u32>,
    pub main_batches: Vec<Batch>,
    /// the blended (and, drawn in model order, all) slots of the vehicle the camera is in
    pub cab_batches: Vec<Batch>,
    pub prepass_batches: Vec<Batch>,
    pub prefill: Option<Vec<Batch>>,
    pub msaa_safe_batches: Vec<Batch>,
    pub presurface_batch_end: usize,
    /// the rain films on the panes, drawn over the finished picture (see `glass_on`)
    pub rain_batches: Vec<Batch>,
    /// opaque/alpha-tested and blended draws of the main pass
    pub main_draws: [usize; 2],
    pub has_presurface: bool,
    /// the terrain's paint laid into one albedo picture before the main pass (see `plan_ground`)
    pub ground: GroundPlan,
}

impl DrawPlan {
    pub(crate) fn prefill_batches(&self) -> &[Batch] {
        self.prefill.as_deref().unwrap_or(&self.prepass_batches)
    }

    /// Depth prefilling is shared by bundle planning and pass encoding so neither can
    /// omit the excavation prefix unless it will actually be drawn separately.
    pub(crate) fn msaa_prepass(&self, renderer: &Renderer, f: &FrameCtx) -> bool {
        // Excavation covers have already been drawn when prefilling begins. They
        // must not opt an otherwise opaque Apple view out of native surface removal.
        let remaining = if self.has_presurface { &self.main_batches[self.presurface_batch_end..] } else { &self.main_batches };
        let needs = !cfg!(target_vendor = "apple") || remaining.iter().chain(&self.cab_batches)
            .any(|b| matches!(b.pipe / 4, PIPE_ALPHA_TEST | PIPE_BLEND | PIPE_BLEND_NO_WRITE));
        let single = renderer.options.msaa <= 1;
        renderer.prepass_msaa_pipelines.is_some()
            && f.enhanced && (f.with_overlays || f.xr_view) && (!single || (self.has_presurface && cfg!(target_vendor = "apple")))
            && (!self.has_presurface || self.msaa_safe_batches.iter().any(|b| single || b.pipe / 2 != PIPE_ALPHA_TEST))
            && f.prepass_on && needs && !f.env.no_msaa_prepass
    }

    pub(crate) fn presurface_prefill(&self, renderer: &Renderer, f: &FrameCtx) -> bool {
        self.has_presurface && self.msaa_prepass(renderer, f)
    }

    /// the rain films out of the main pass: they are drawn after copying the clean picture
    pub(crate) fn split_rain(&mut self, scene: &Scene, glass_on: bool) {
        if glass_on {
            let is_rain = |b: &Batch| scene.materials[b.material as usize].uniform.emissive[3] > 1.5;
            self.presurface_batch_end = self.main_batches[..self.presurface_batch_end].iter().filter(|b| !is_rain(b)).count();
            let (rain, main): (Vec<_>, Vec<_>) = std::mem::take(&mut self.main_batches).into_iter().partition(is_rain);
            self.rain_batches = rain;
            self.main_batches = main;
        }
    }
}

// the depth prepass: opaque and alpha-tested, single-sampled
fn prepass_items(pool: Option<&rayon::ThreadPool>, scene: &Scene, visible: &[(usize, f32, bool)], deferred: bool, reach: f32) -> (Vec<u32>, Vec<Batch>, Option<Vec<Batch>>, Vec<Batch>) {
    let mut items: Vec<DrawItem> = Vec::new();
    let mut kept: Vec<DrawItem> = Vec::new();
    let mut any_cut = false;
    let mut safe_items = Vec::new();
    let first_panes = first_depth_panes(scene, visible);
    for &(i, z, _) in visible {
        let inst = &scene.instances[i];
        let far = z > reach && !inst.surface && !inst.presurface && z - Renderer::bounding_sphere(scene, inst).1 > reach;
        let mut cut = inst.ordered && first_panes.get(&origin_key(inst.origin)).is_some_and(|&o| inst.draw_order > o);
        let cull = culls_back_faces(scene, inst);
        for (ri, (_, _, slot)) in scene.meshes[inst.mesh].ranges.iter().enumerate() {
            let mat_id = inst.materials.get(*slot as usize).copied().unwrap_or(0);
            let mat = &scene.materials[mat_id];
            let kind = kind_of(mat.alpha);
            // A model drawn in its own order: an opaque part that a depth-writing pane
            // before it hides (a scroll box's line under its invisible cover) put its depth
            // here, kept the wall behind it from being drawn, and then failed against the
            // pane itself - a hole of the clear colour (#2059). So its parts after the first
            // such pane stay out of the prepass; the ones before it go in as any model's.
            // (0.2.46 left the whole model out: a bus with its glass listed early lost its
            // body from the depth that the ambient occlusion and the sun's glare read - no
            // contact shadow under it, the glare and the passengers through its roof.)
            if inst.ordered && blend_writes_depth(mat) {
                cut = true;
            }
            // Ground blends compose before they may occlude later scenery.
            // The transmap body prepass is for ordinary meshes, not these
            // authored surface layers (including their terrain brush masks).
            if kind == PIPE_BLEND && world_surface_phase(effective_render_phase(inst)) {
                continue;
            }
            if far && kind < PIPE_BLEND {
                continue;
            }
            if let Some(pre_kind) = depth_prepass_kind(kind, mat, inst.presurface) {
                // plain/alpha-tested materials use their ordinary depth pass;
                // blended transmaps use the opaque-pixels-only pass.
                let (material, look) = depth_only_material(pre_kind, mat_id, mat.look);
                let item = DrawItem {
                    pipe: pre_kind * 2 + cull as u8,
                    mesh: inst.mesh as u32,
                    range: ri as u32,
                    material,
                    look,
                    entry: inst.base + *slot,
                };
                // Ground must finish composing before its coverage can occlude later layers.
                if !cut && deferred && effective_render_phase(inst) as u8 >= RenderPhase::BeforeNormal as u8 {
                    safe_items.push(item);
                }
                if cut {
                    any_cut = true;
                } else {
                    kept.push(item);
                }
                items.push(item);
            }
        }
    }
    // (the three lists sorted side by side, then joined in this order: the same list as one after another)
    let sort = |items: &mut Vec<DrawItem>| {
        let (mut list, mut batches) = (Vec::new(), Vec::new());
        batch_items(scene, items, true, &mut list, &mut batches);
        (list, batches)
    };
    let sort = &sort;
    let (mut all, mut kept_sorted, mut safe_sorted) = ((Vec::new(), Vec::new()), None, (Vec::new(), Vec::new()));
    in_scope(pool, |s| {
        if any_cut {
            let (k, out) = (&mut kept, &mut kept_sorted);
            s.spawn(move |_| *out = Some(sort(k)));
        }
        let (k, out) = (&mut safe_items, &mut safe_sorted);
        s.spawn(move |_| *out = sort(k));
        all = sort(&mut items);
    });
    let (mut list, batches) = all;
    let mut append = |(l, mut b): (Vec<u32>, Vec<Batch>)| {
        let at = list.len() as u32;
        for x in &mut b {
            x.instances = x.instances.start + at..x.instances.end + at;
        }
        list.extend(l);
        b
    };
    let prefill = kept_sorted.map(&mut append);
    let safe_batches = append(safe_sorted);
    (list, batches, prefill, safe_batches)
}

fn blend_writes_depth(mat: &Material) -> bool {
    mat.alpha == AlphaMode::Blend && (!mat.no_z_write || mat.writes_depth) && !mat.no_z_check
}

fn first_depth_panes(scene: &Scene, visible: &[(usize, f32, bool)]) -> HashMap<[u64; 3], u64> {
    let mut first: HashMap<[u64; 3], u64> = HashMap::new();
    for &(i, _, _) in visible {
        let inst = &scene.instances[i];
        if !inst.ordered {
            continue;
        }
        let pane = scene.meshes[inst.mesh].ranges.iter().any(|(_, _, slot)| {
            blend_writes_depth(&scene.materials[inst.materials.get(*slot as usize).copied().unwrap_or(0)])
        });
        if pane {
            first.entry(origin_key(inst.origin)).and_modify(|o| *o = (*o).min(inst.draw_order)).or_insert(inst.draw_order);
        }
    }
    first
}

/// The surfaces' covered ground pixels committed to depth, before the normal phase.
fn surface_depth_items(scene: &Scene, visible: &[(usize, f32, bool)], exclude_texture: Option<TextureId>, items: &mut Vec<DrawItem>) {
    for &(i, _, _) in visible {
        let inst = &scene.instances[i];
        for (ri, (_, _, slot)) in scene.meshes[inst.mesh].ranges.iter().enumerate() {
            let mat_id = inst.materials.get(*slot as usize).copied().unwrap_or(0);
            let mat = &scene.materials[mat_id];
            if !surface_depth_coverage(effective_render_phase(inst), mat.alpha, mat.transmap.is_some(), mat.no_z_check)
                || exclude_texture.is_some_and(|t| mat.uses_texture(t))
            {
                continue;
            }
            items.push(DrawItem {
                pipe: pipe_code(PIPE_SURFACE_DEPTH, culls_back_faces(scene, inst), instance_depth_bias(inst, mat)),
                mesh: inst.mesh as u32,
                range: ri as u32,
                material: mat_id as u32,
                look: mat.look,
                entry: inst.base + *slot,
            });
        }
    }
}

/// A phase's opaque and cut-out draws into `items`; the instances with blended slots
/// (or drawn in model order) into `blended`.
fn opaque_items(scene: &Scene, visible: &[(usize, f32, bool)], exclude_texture: Option<TextureId>, items: &mut Vec<DrawItem>, blended: &mut Vec<usize>) {
    for &(i, _, _) in visible {
        let inst = &scene.instances[i];
        if inst.ordered {
            blended.push(i);
            continue;
        }
        let mut has_blend = false;
        let cull = culls_back_faces(scene, inst);
        for (ri, (_, _, slot)) in scene.meshes[inst.mesh].ranges.iter().enumerate() {
            let mat_id = inst.materials.get(*slot as usize).copied().unwrap_or(0);
            let mat = &scene.materials[mat_id];
            let kind = kind_of(mat.alpha);
            if kind == PIPE_BLEND || mat.no_z_check {
                has_blend = true;
                continue;
            }
            // a render target cannot be sampled while being drawn into (mirror glass, or a reflection map of it)
            if exclude_texture.is_some_and(|t| mat.uses_texture(t)) {
                continue;
            }
            items.push(DrawItem {
                pipe: pipe_code(
                    kind,
                    cull,
                    instance_depth_bias(inst, mat),
                ),
                mesh: inst.mesh as u32,
                range: ri as u32,
                material: mat_id as u32,
                look: mat.look,
                entry: inst.base + *slot,
            });
        }
        if has_blend {
            blended.push(i);
        }

    }
}

/// `opaque_items` over blocks of a long phase on the pool, joined in order: the same lists.
fn opaque_items_parts(pool: Option<&rayon::ThreadPool>, scene: &Scene, visible: &[(usize, f32, bool)], exclude_texture: Option<TextureId>, items: &mut Vec<DrawItem>, blended: &mut Vec<usize>) {
    let parts = match pool {
        Some(p) if visible.len() >= 4096 => (visible.len() / 1024).min(p.current_num_threads() * 2),
        _ => 1,
    };
    if parts <= 1 {
        return opaque_items(scene, visible, exclude_texture, items, blended);
    }
    let chunk = visible.len().div_ceil(parts);
    for (i, b) in run_parts(pool, parts, |k| {
        let (mut i, mut b) = (Vec::new(), Vec::new());
        opaque_items(scene, &visible[(k * chunk).min(visible.len())..((k + 1) * chunk).min(visible.len())], exclude_texture, &mut i, &mut b);
        (i, b)
    }) {
        items.extend(i);
        blended.extend(b);
    }
}

/// A phase's blended draws, in the order of `blended_order`, into `items`; the vehicle
/// the camera is in into `cab_items`.
fn blended_items(scene: &Scene, keyed: Vec<(u8, f32, usize)>, exclude_texture: Option<TextureId>, items: &mut Vec<DrawItem>, cab_items: &mut Vec<DrawItem>) {
    for (rank, _, i) in keyed {
        let inst = &scene.instances[i];
        let cull = culls_back_faces(scene, inst);
        for (ri, (_, _, slot)) in scene.meshes[inst.mesh].ranges.iter().enumerate() {
            let mat_id = inst.materials.get(*slot as usize).copied().unwrap_or(0);
            let mat = &scene.materials[mat_id];
            if (mat.alpha != AlphaMode::Blend && !mat.no_z_check && !inst.ordered)
                || exclude_texture.is_some_and(|t| mat.uses_texture(t))
            {
                continue;
            }
            // A layer its script has faded out (`[alphascale]` at 0: the rain film
            // on a dry day, the dirt on a clean bus) shows nothing, as in the
            // original, whose blend takes it out whole; drawn anyway it ran the full
            // shading over the whole windscreen for nothing - a bus's cab view had
            // three or four such screen-sized layers.
            if mat.alpha == AlphaMode::Blend && inst.slot_alpha.get(*slot as usize).is_some_and(|a| *a < 1.0 / 512.0) {
                continue;
            }
            // Ground blends use the C++ handler's no-write composition;
            // their opaque coverage is committed after OnSurface. Other
            // materials retain [matl_noZwrite] (glass, rain, dirt) semantics.
            // [matl_noZcheck] marks a decal that must win over the surface it lies
            // on (a bus's shadow blob, the digits on a counter): the original draws
            // it without a depth test right after that surface, in model order. With
            // everything opaque drawn first here, no test at all would put it over
            // the whole bus (the steering wheel in front of the counter, the body over
            // the shadow), so it is drawn with the surfaces' depth bias instead: on
            // top of its base, behind whatever really stands in front of it.
            let kind = if mat.alpha != AlphaMode::Blend && !mat.no_z_check {
                // (a model drawn in order: its opaque and cut-out slots too)
                kind_of(mat.alpha)
            } else if (mat.no_z_write && !mat.writes_depth) || mat.no_z_check || (world_surface_phase(inst.render_phase) && !inst.presurface) {
                PIPE_BLEND_NO_WRITE
            } else {
                PIPE_BLEND
            };
            // Only tile painting gets the zero-coverage fast path. Other
            // no-write blends (splines, glass, decals) keep their shader.
            let kind = if kind == PIPE_BLEND_NO_WRITE
                && mat.alpha == AlphaMode::Blend
                && inst.ground_layer
                && !inst.presurface
                && mat.uniform.extra[0] > 0.5
                && mat.transmap.is_some_and(|(_, alpha)| alpha)
            {
                PIPE_TERRAIN_PAINT
            } else {
                kind
            };
            let item = DrawItem {
                pipe: pipe_code(
                    kind,
                    cull,
                    instance_depth_bias(inst, mat),
                ),
                mesh: inst.mesh as u32,
                range: ri as u32,
                material: mat_id as u32,
                look: mat.look,
                entry: inst.base + *slot,
            };
            // the vehicle the camera is in is drawn after everything else
            if rank == 2 {
                cab_items.push(item);
            } else {
                items.push(item);
            }
        }
    }
}

impl Renderer {
    /// The batches of the depth prepass and of the main pass, appended to the draw list.
    pub(crate) fn batch_draws(&self, scene: &Scene, f: &FrameCtx, visible: &[(usize, f32, bool)], mut draw_list: Vec<u32>) -> DrawPlan {
        let list = &mut draw_list;
        let (exclude_texture, prepass_on) = (f.exclude_texture, f.prepass_on);
        let mut items: Vec<DrawItem> = Vec::new();
        let has_presurface = visible.iter().any(|&(i, _, _)| scene.instances[i].presurface);
        let mut prepass_batches: Vec<Batch> = Vec::new();
        let mut prefill = None;
        let mut msaa_safe_batches = Vec::new();
        let deferred = has_presurface && f.enhanced;
        let pool = self.encoding_pool.as_ref();
        let prepass_job = || { prepass_items(pool, scene, visible, deferred, f.prepass_reach) };
        // The main pass follows the authored world phases. Each phase keeps its
        // opaque/cutout draws followed by its blended draws, far to near. Transparent
        // ground layers never write depth while composing: an alpha-zero junction
        // texel otherwise blocks a later opaque grass spline and exposes the sky
        // wherever that spline's prepass already rejected the terrain underneath.
        let mut main_batches: Vec<Batch> = Vec::new();
        let mut presurface_batch_end = 0;
        let mut terrain_blends = 0..0;
        // the blended (and, drawn in model order, all) slots of the vehicle the camera is in
        let mut cab_batches: Vec<Batch> = Vec::new();
        let mut cab_items: Vec<DrawItem> = Vec::new();
        let mut main_draws = [0usize; 2];
        // Keep mesh/material order here: an excavation's floor is drawn before its
        // invisible cover writes depth. Sorting its blended cover after the terrain
        // leaves the terrain's colour in place even though the cover writes depth.
        // The prepass's batches are planned beside the main pass's on a worker - or by this
        // thread itself when it is done with the main pass first and no worker has taken
        // them up yet: it never waits for a job no one started (a worker the system had not
        // got round to held up the whole frame).
        let prepass_found = std::sync::Mutex::new(None);
        let prepass_taken = std::sync::atomic::AtomicBool::new(!prepass_on);
        let take_prepass = || {
            if !prepass_taken.swap(true, std::sync::atomic::Ordering::AcqRel) {
                let found = prepass_job();
                *prepass_found.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
            }
        };
        in_scope(pool, |scope| {
            if prepass_on {
                let take = &take_prepass;
                scope.spawn(move |_| take());
            }
            let mut by_phase: [Vec<(usize, f32, bool)>; RenderPhase::COUNT] =
                std::array::from_fn(|_| Vec::new());
            for &entry in visible {
                let phase = effective_render_phase(&scene.instances[entry.0]);
                by_phase[phase as usize].push(entry);
            }
            // OMSI draws each authored world phase as its own opaque/cutout pass followed
            // by that phase's blended draws. Keeping the phase boundary here lets later
            // surface markings compose over spline blends without changing the depth test.
            // The blended draws of the scenery that comes after the map's own objects
            // (`[rendertype]` 3 and 4) are sorted with the normal phase's, far to near,
            // after the opaque draws of all three: a vehicle's panes write depth (unless
            // its model says `[matl_noZwrite]`), so a bus shelter of `[rendertype] 4`
            // drawn after the bus failed the depth test behind its windows - and with the
            // depth prefilled, the houses behind the shelter were already gone, so the
            // windows showed the sky's colour in its shape, white (Aachen, Grundorf: #2852).
            let late_blend = |p: RenderPhase| matches!(p, RenderPhase::Normal | RenderPhase::AfterNormal);
            let mut late_visible: Vec<(usize, f32, bool)> = Vec::new();
            let mut late_blended: Vec<usize> = Vec::new();
            for phase in RenderPhase::DRAW_ORDER {
                if phase == RenderPhase::Terrain { presurface_batch_end = main_batches.len(); }
                // Finish surface composition before committing the fully covered ground
                // pixels to depth. Doing this in the global prepass (or while blending
                // each spline) would reject authored road overlaps and on-surface rails.
                // Later scenery must still be occluded by the solid part of the road:
                // otherwise cutout bushes below a bridge repaint its asphalt.
                if phase == RenderPhase::BeforeNormal {
                    items.clear();
                    for v in &by_phase[..RenderPhase::BeforeNormal as usize] {
                        surface_depth_items(scene, v, exclude_texture, &mut items);
                    }
                    batch_items(scene, &mut items, true, list, &mut main_batches);
                }
                let visible = &by_phase[phase as usize];
                items.clear();
                let mut blended: Vec<usize> = Vec::new();
                opaque_items_parts(pool, scene, visible, exclude_texture, &mut items, &mut blended);
                main_draws[0] += items.len();
                batch_items(scene, &mut items, true, list, &mut main_batches);
                let blends_from = main_batches.len();
                if late_blend(phase) {
                    late_visible.extend_from_slice(visible);
                    late_blended.append(&mut blended);
                    continue;
                }
                if phase == RenderPhase::AfterVehicles {
                    late_visible.extend_from_slice(visible);
                    late_blended.append(&mut blended);
                    blended = std::mem::take(&mut late_blended);
                }
                let visible = if phase == RenderPhase::AfterVehicles { &late_visible[..] } else { &visible[..] };
                let keyed = self.blended_order(scene, f, visible, &blended);
                items.clear();
                blended_items(scene, keyed, exclude_texture, &mut items, &mut cab_items);
                main_draws[1] += items.len();
                batch_items(scene, &mut items, false, list, &mut main_batches);
                if phase == RenderPhase::Terrain {
                    terrain_blends = blends_from..main_batches.len();
                }
            }
            // Omsi.exe draws the vehicle the camera sits in last of all, with the view mask
            // of its inside (0x6f1520 -> 0x6f0430), after every phase of the map, the other
            // vehicles, the particles and the lamps' flares (0x6f0400/0x6f0418): its glass,
            // whose depth is written unless the model says `[matl_noZwrite]`, then lies over
            // all of that. Its items wait here and are drawn after the coronas and the smoke
            // (see `cab_batches` in the main pass).
            main_draws[1] += cab_items.len();
            batch_items(scene, &mut cab_items, false, list, &mut cab_batches);
            take_prepass();
        });
        if let Some((pre_list, mut pre_batches, mut pre_prefill, mut safe_batches)) = prepass_found.into_inner().unwrap_or_else(|e| e.into_inner()) {
            let offset = list.len() as u32;
            for b in pre_batches.iter_mut().chain(pre_prefill.iter_mut().flatten()).chain(&mut safe_batches) {
                b.instances = b.instances.start + offset..b.instances.end + offset;
            }
            list.extend(pre_list);
            prepass_batches = pre_batches;
            prefill = pre_prefill;
            msaa_safe_batches = safe_batches;
        }
        // OMSI_SKIP_PIPE=3,1: leave pipeline kinds out of the main pass (0 opaque, 1 alpha
        // tested, 2 blended, 3 blended without depth writes, 4 surface depth, 5 terrain
        // paint; 3 includes its specialized terrain variant) - with
        // OMSI_GPU_TIMERS_RAW, what each kind costs the GPU
        if let Some(skip) = f.env.skip_pipe {
            let skip: Vec<u8> = skip.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            let include = |b: &Batch| {
                let kind = b.pipe / 4;
                !skip.contains(&kind)
                    && !(kind == PIPE_TERRAIN_PAINT && skip.contains(&PIPE_BLEND_NO_WRITE))
            };
            presurface_batch_end = main_batches[..presurface_batch_end].iter().filter(|b| include(b)).count();
            main_batches.retain(include);
            cab_batches.retain(include);
        }
        if f.env.debug_draws {
            log::info!("  main pass: {} opaque/alpha-tested and {} blended draws in {} batches; prepass {} batches; draw list {} entries", main_draws[0], main_draws[1], main_batches.len(), prepass_batches.len(), list.len());
        }
        // (the terrain's paint into one picture: not with an excavation, nor with draws left out
        // by a debug switch, which would move the places `terrain_blends` keeps)
        let ground = if has_presurface || f.env.skip_pipe.is_some() { GroundPlan::default() } else { self.plan_ground(scene, f, &mut main_batches, terrain_blends) };
        DrawPlan { list: draw_list, main_batches, cab_batches, prepass_batches, prefill, msaa_safe_batches, presurface_batch_end, rain_batches: Vec::new(), main_draws, has_presurface, ground }
    }

    /// The order a phase's blended draws are drawn in: (rank, distance, instance), far
    /// to near within a rank.
    fn blended_order(&self, scene: &Scene, f: &FrameCtx, visible: &[(usize, f32, bool)], blended: &[usize]) -> Vec<(u8, f32, usize)> {
        let (camera, lighting, ro, cam_rel) = (f.camera, f.lighting, f.ro, f.cam_rel);
        // Blended draws: objects far to near by the distance of their nearest blended
        // mesh (see `near_by_origin` below - not the single local origin all of an
        // object's meshes share), and within an object in creation order - the
        // model.cfg mesh order, which is what the original relies on (windows are
        // listed last).
        //
        // An object the camera is inside of (the bus seen from the driver's
        // seat) comes after everything outside it, and the player's own vehicle
        // last of all. By its origin alone the bus - whose origin is 4.6 m
        // behind the driver's eye on the NL202 - sorted as farther away than a
        // car right beside the driver's window, so the car was drawn after the
        // bus's window layers (rain film, dirt, door glass), which write depth:
        // its blended body failed the depth test and only the opaque wheels
        // were left, dark behind the tinted glass, exactly while the car was
        // half out of the picture.
        let mut holders: Vec<DVec3> = Vec::new();
        for &(i, _, inside) in visible {
            let inst = &scene.instances[i];
            if inside && !inst.surface && !holders.contains(&inst.origin) {
                holders.push(inst.origin);
            }
        }
        let player = lighting
            .inside
            .filter(|v| point_in_vehicle_box(camera.position, v))
            .map(|v| v.0);
        // An object's *nearest* blended mesh to the camera, not the single point its
        // meshes all share (`inst.origin`): a long vehicle's own origin can sit well
        // behind (or ahead of) its nearest window, so ranking the whole object by that
        // one point against a much smaller nearby object - a car passing level with the
        // middle of a stopped bus - picked the wrong order even outside the "camera is
        // inside" case above (the bus's origin, metres behind the window nearest the
        // car, sorted as farther away than the car itself, so the car was drawn last and
        // painted over the window instead of being hidden behind the body between the
        // windows). Every blended mesh of the object is a candidate; the closest one's
        // distance, less its own bounding radius, stands for the whole object.
        //
        // Scope, checked systematically while chasing a report of a car showing through
        // a stopped bus's body from outside (never reproduced, before or after this
        // commit): this order only ever decides how mutually-*blended* draws composite
        // where they overlap on screen (a car's own window glass in front of a bus's
        // window + interior, say) - it cannot be why an opaque wall would fail to hide
        // something behind it. Every pipeline the main pass uses, opaque or blended,
        // keeps depth *testing* on (`GreaterEqual`, see the pipeline table above); only
        // depth *writing* differs. Opaque batches are always recorded before blended ones
        // in the same pass (`main_draws[0]` first), so by the time any blended draw runs,
        // the depth buffer already holds every opaque surface in front of it, blend order
        // or not. Dumping the EN92's and the O530 Facelift's per-material alpha mode
        // (`OMSI_ONLY_MESH`) found every body panel `AlphaMode::Opaque`, as OMSI requires
        // (diffuse alpha is a reflection mask, not transparency, unless `[matl_alpha]` 1
        // or 2 says otherwise); an A/B render (this commit vs its parent, same seed, a
        // parked car centred behind a stopped EN92's midsection) came back pixel-identical
        // at the car/bus silhouette - the only measured difference was in the bus's own
        // overlapping window/dirt/interior layers, which is exactly this sort's stated
        // job. If the reported artefact is real, its cause is still open and elsewhere.
        let near_by_origin = if self.blend_by_origin {
            HashMap::new()
        } else {
            nearest_by_origin(blended.iter().filter_map(|&i| {
                let inst = &scene.instances[i];
                if inst.surface {
                    return None;
                }
                let (c, r) = Self::bounding_sphere(scene, inst);
                Some((inst.origin, (c - cam_rel).length() - r))
            }))
        };
        let mut keyed: Vec<(u8, f32, usize)> = blended
            .iter()
            .map(|&i| {
                let inst = &scene.instances[i];
                // Surfaces are ground and go by distance alone: a tile's painted ground
                // shares its origin with the terrain the camera is always inside of, and
                // ranked with it, it was drawn after everything blended near it - over the
                // shadow blobs of the buses standing on it. A blob belongs to the ground
                // under its vehicle too, drawn before the vehicle's glass.
                let rank = if self.blend_by_origin || inst.surface {
                    0
                } else if player == Some(inst.origin) {
                    2
                } else if holders.contains(&inst.origin) {
                    1
                } else {
                    0
                };
                // A vehicle's shadow blob goes after all the ground: it writes no depth, so
                // every road piece nearer than its origin, drawn after it by distance,
                // painted the road back over it (OMSI draws it over the road it lies on).
                let dist = if inst.blob {
                    -1.0
                } else if let Some(sort_origin) = inst.blend_sort_origin {
                    // Spline surfaces use the C++ handler's placement-origin distance
                    // in the horizontal plane (Rust's world axes are x/y horizontal,
                    // z vertical).
                horizontal_sort_distance(sort_origin, ro, cam_rel)
                } else if self.blend_by_origin || inst.surface {
                    ((inst.origin - ro).as_vec3() - cam_rel).length()
                } else {
                    near_by_origin
                        .get(&origin_key(inst.origin))
                        .copied()
                        .unwrap_or(0.0)
                };
                (rank, dist, i)
            })
            .collect();
        // (a total order even where a distance is NaN - an instance at a NaN position:
        // partial_cmp's "equal" for it broke the sort's order, and since Rust 1.81 the
        // sort panics on that, which ended the game)
        // AI meshes reuse slots from separate pools by material-slot count. Their ids
        // can put display glass between two text rows; keep the model's creation order.
        sort_blended_draws(&mut keyed, |i| scene.instances[i].draw_order);
        keyed
    }

    /// OMSI_PROFILE: the frame's counts, and every ten seconds what each asset costs.
    pub(crate) fn count_draws(&mut self, scene: &Scene, with_overlays: bool, visible: usize, plan: &DrawPlan, shadow_batches: &[Vec<Batch>; SHADOW_LISTS]) {
        let DrawPlan { main_batches, prepass_batches, main_draws, .. } = plan;
        if with_overlays {
            self.frame_draws.0 += main_batches.len() + prepass_batches.len() + shadow_batches.iter().map(Vec::len).sum::<usize>();
            self.frame_draws.1 += visible;
        }
        if self.profiling && !with_overlays {
            let mut c = self.counts.borrow_mut();
            *c.entry("mirror pictures").or_default() += 1.0;
            *c.entry("mirror visible instances").or_default() += visible as f64;
        }
        if self.profiling && with_overlays {
            let mut c = self.counts.borrow_mut();
            *c.entry("scene instances").or_default() += scene.instances.len() as f64;
            *c.entry("visible instances").or_default() += visible as f64;
            *c.entry("bounds users").or_default() += scene.bounds_users.len() as f64;
            *c.entry("main draws").or_default() += (main_draws[0] + main_draws[1]) as f64;
            *c.entry("opaque draws").or_default() += main_draws[0] as f64;
            *c.entry("blended draws").or_default() += main_draws[1] as f64;
            *c.entry("main batches").or_default() += main_batches.len() as f64;
            *c.entry("presurface views").or_default() += plan.has_presurface as u8 as f64;
            *c.entry("terrain paint batches").or_default() += main_batches.iter().filter(|b| b.pipe / 4 == PIPE_TERRAIN_PAINT).count() as f64;
            // the state the main pass's bundles set (each part starts afresh), and what an
            // order by material within the opaque runs would set instead
            let (pipes, binds) = state_changes(&bundle_chunks(self.encoding_pool.as_ref(), main_batches));
            *c.entry("main pipeline switches").or_default() += pipes as f64;
            *c.entry("main material binds").or_default() += binds as f64;
            let (pipes_m, binds_m) = state_changes_by_material(main_batches);
            *c.entry("main pipeline switches by material").or_default() += pipes_m as f64;
            *c.entry("main material binds by material").or_default() += binds_m as f64;
            *c.entry("prepass batches").or_default() += prepass_batches.len() as f64;
            *c.entry("shadow batches").or_default() +=
                (shadow_batches[0].len() + shadow_batches[1].len()) as f64;
            // triangles each pass draws (thousands), the geometry the GPU goes through
            let tris = |bs: &[Batch]| bs.iter().map(|b| b.count as f64 / 3.0 * b.instances.len() as f64).sum::<f64>() / 1000.0;
            *c.entry("ktris main").or_default() += tris(main_batches);
            *c.entry("ktris prepass").or_default() += tris(prepass_batches);
            *c.entry("ktris shadow near").or_default() += tris(&shadow_batches[0]);
            *c.entry("ktris shadow far").or_default() += tris(&shadow_batches[1]);
            *c.entry("ktris shadow close").or_default() += tris(&shadow_batches[2]);
        }
        if self.profiling && with_overlays && self.draw_audit_at.elapsed().as_secs() >= 10 {
            self.draw_audit_at = std::time::Instant::now();
            // (batches, draws, triangles) per asset: what the CPU encodes and what the GPU
            // goes through
            let mut assets: HashMap<&str, (usize, usize, u64)> = HashMap::new();
            for b in main_batches {
                let source = scene.meshes[b.mesh as usize].source.as_deref().unwrap_or("procedural / vehicle");
                let cost = assets.entry(source).or_default();
                cost.0 += 1;
                cost.1 += b.instances.len();
                cost.2 += b.count as u64 / 3 * b.instances.len() as u64;
            }
            let mut assets: Vec<_> = assets.into_iter().collect();
            assets.sort_unstable_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
            for (source, (batches, draws, tris)) in assets.iter().take(12) {
                log::info!("draw audit: {batches} batches, {draws} draws, {tris} triangles: {source}");
            }
            assets.sort_unstable_by(|a, b| b.1.2.cmp(&a.1.2).then(a.0.cmp(b.0)));
            for (source, (batches, draws, tris)) in assets.iter().take(12) {
                log::info!("triangle audit: {tris} triangles in {draws} draws ({batches} batches): {source}");
            }
        }
    }

    /// The main pass's batches recorded as render bundles on several threads (none with
    /// OMSI_NO_BUNDLES).
    pub(crate) fn record_main_bundles(&self, scene: &Scene, f: &FrameCtx, plan: &DrawPlan) -> Vec<wgpu::RenderBundle> {
        // OMSI_NO_BUNDLES=1 records the main pass directly, for comparison. (Splitting the
        // pass in two to finish the halves side by side was tried as well: the second half
        // has to load the first one's targets back into the GPU's tile memory, which cost
        // more GPU time than it saved on the CPU.)
        if !f.env.no_bundles {
            let pp = self.main_pass(f.enhanced, f.reflection_frame, f.eye_color);
            let format = if f.masked_frame { HDR_FORMAT } else { self.format };
            let batches = if plan.presurface_prefill(self, f) { &plan.main_batches[plan.presurface_batch_end..] } else { &plan.main_batches };
            let camera = scene.camera_bind_group.as_ref().expect("camera bind group");
            // The window's bundles of the last frame are drawn again where this frame's
            // batches are the same and name the same buffers and bind groups: a bundle holds
            // only which draws to make, and what the draws read (the instances' places, the
            // draw list) is in buffers written every frame. A view standing still - the bus
            // at a stop - recorded the same thousand draws anew every frame. This goes by
            // part: when only the last parts changed (the blended draws, re-sorted as the
            // traffic moves, come last), the parts before them are kept.
            // The mirrors' the same, each its own (by its picture's texture): two or three
            // mirror pictures a frame recorded some 1.5 ms of bundles each.
            let kept = f.with_overlays && f.projection.is_none() && !f.second_eye;
            let mirror = (!f.with_overlays && !f.second_eye).then_some(f.exclude_texture).flatten();
            let mode = (f.enhanced, f.reflection_frame, format, self.options.msaa, rt_gbuf() && !f.eye_color,
            );
            let chunks = bundle_chunks(self.encoding_pool.as_ref(), batches);
            let mut reuse: Vec<Option<wgpu::RenderBundle>> = vec![None; chunks.len()];
            // (what a part binds is kept once its batches repeat: while driving they change every frame)
            let mut repeated = vec![false; chunks.len()];
            let mirrors = self.history.mirror_bundles.borrow();
            let window = kept.then(|| self.history.main_bundles.borrow());
            let last = match &window {
                Some(w) => w.as_ref(),
                None => mirror.and_then(|t| mirrors.iter().find(|m| m.0 == t).map(|m| &m.1)),
            };
            {
                if let Some(c) = last {
                    if c.mode == mode && c.camera == *camera {
                        for (k, chunk) in chunks.iter().enumerate() {
                            if let Some((_, r, bundle)) = c.parts.get(k).filter(|(b, ..)| *b == **chunk) {
                                repeated[k] = true;
                                if r.as_ref().is_some_and(|r| same_resources(scene, chunk, r)) {
                                    reuse[k] = Some(bundle.clone());
                                }
                            }
                        }
                        if reuse.iter().all(Option::is_some) && c.parts.len() == chunks.len() {
                            if kept {
                                self.history.bundle_stats.set((0, reuse.len()));
                            }
                            return reuse.into_iter().flatten().collect();
                        }
                    }
                }
            }
            let kept_parts = reuse.iter().filter(|b| b.is_some()).count();
            if f.with_overlays {
                self.history.bundle_stats.set((chunks.len() - kept_parts, kept_parts));
            }
            let bundles = record_bundles(&self.device, self.encoding_pool.as_ref(), scene, &chunks, &reuse, pp, camera, format, self.options.msaa,
                rt_gbuf() && !f.eye_color,
            );
            // (not when a part could not be recorded: it would be left out frame after frame;
            // and a mirror's or an eye's picture leaves the window's alone - it cleared them,
            // so with the mirrors on the window never kept a bundle)
            drop((window, mirrors));
            let whole = bundles.iter().all(Option::is_some);
            let made = || MainBundles {
                mode,
                camera: camera.clone(),
                parts: chunks
                    .iter()
                    .zip(&bundles)
                    .zip(&repeated)
                    .map(|((chunk, b), &again)| (chunk.to_vec(), again.then(|| batch_resources(scene, chunk)), b.clone().expect("whole")))
                    .collect(),
            };
            if kept {
                *self.history.main_bundles.borrow_mut() = whole.then(made);
            } else if let Some(t) = mirror {
                let mut mirrors = self.history.mirror_bundles.borrow_mut();
                mirrors.retain(|m| m.0 != t);
                if whole {
                    // (a few: a bus has some mirrors, and a texture of one gone stays no longer
                    // than it takes the others to come round)
                    if mirrors.len() >= 8 {
                        mirrors.remove(0);
                    }
                    mirrors.push((t, made()));
                }
            }
            bundles.into_iter().flatten().collect()
        } else {
            Vec::new()
        }
    }
}

/// The window's main-pass bundles as last recorded and what they were recorded from.
pub(crate) struct MainBundles {
    mode: (bool, bool, wgpu::TextureFormat, u32, bool),
    camera: wgpu::BindGroup,
    /// each bundle with the batches it was recorded from and what they bound (once they repeat)
    parts: Vec<(Vec<Batch>, Option<Vec<BatchResources>>, wgpu::RenderBundle)>,
}

/// What a recorded batch binds: its mesh's buffers and place in them, and its material's
/// bind group (a mesh or material slot can be given to another since).
#[derive(PartialEq)]
struct BatchResources(wgpu::Buffer, wgpu::Buffer, i32, u32, wgpu::BindGroup);

fn batch_resource<'s>(scene: &'s Scene, b: &Batch) -> (&'s wgpu::Buffer, &'s wgpu::Buffer, i32, u32, &'s wgpu::BindGroup) {
    let m = &scene.meshes[b.mesh as usize];
    (&m.vertex_buf, &m.index_buf, m.base_vertex, m.first_index, &scene.materials[b.material as usize].bind_group)
}

fn batch_resources(scene: &Scene, batches: &[Batch]) -> Vec<BatchResources> {
    batches
        .iter()
        .map(|b| {
            let (v, i, base, first, bg) = batch_resource(scene, b);
            BatchResources(v.clone(), i.clone(), base, first, bg.clone())
        })
        .collect()
}

fn same_resources(scene: &Scene, batches: &[Batch], kept: &[BatchResources]) -> bool {
    batches.len() == kept.len()
        && batches.iter().zip(kept).all(|(b, k)| {
            let (v, i, base, first, bg) = batch_resource(scene, b);
            *v == k.0 && *i == k.1 && base == k.2 && first == k.3 && *bg == k.4
        })
}

#[cfg(test)]
mod tests {
    use super::sort_blended_draws;
    use crate::{AlphaMode, MeshData, RenderOptions, Renderer};
    use glam::{DVec3, Mat4, Vec3};

    #[test]
    fn state_changes_count_what_the_encoder_sets() {
        use crate::frame_plan::{state_changes, state_changes_by_material, Batch};
        let b = |pipe: u8, material: u32| Batch { pipe, mesh: 0, first: 0, count: 3, material, instances: 0..1 };
        // opaque culled (pipe 2) and not culled (pipe 0) of two looks, then a blended one
        let batches = [b(0, 1), b(0, 2), b(2, 1), b(2, 2), b(8, 1)];
        assert_eq!(state_changes(&[&batches]), (3, 5));
        // two parts: each sets its own state
        assert_eq!(state_changes(&[&batches[..3], &batches[3..]]), (4, 5));
        // by material: 1 1 2 2 then blended 1 - fewer binds, more pipeline switches
        assert_eq!(state_changes_by_material(&batches), (5, 3));
    }

    #[test]
    fn recycled_display_layers_keep_model_order_after_distance_and_camera_rank() {
        // Glass (three material slots) reused id 1; the text rows (one slot each)
        // reused ids 0 and 2. Slot order would tint only the upper row.
        let order = [11, 10, 12, 13, 14];
        let mut draws = vec![(0, 5.0, 2), (2, 20.0, 4), (0, 5.0, 0), (0, 9.0, 3), (0, 5.0, 1)];
        sort_blended_draws(&mut draws, |i| order[i]);
        assert_eq!(draws.iter().map(|d| d.2).collect::<Vec<_>>(), vec![3, 1, 0, 2, 4]);
    }

    #[test]
    #[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
    fn recycled_instances_sort_by_new_model_creation_order() {
        let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let renderer = pollster::block_on(Renderer::new_with(
            &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
        )).expect("test renderer");
        let mut scene = renderer.new_scene();
        let material = renderer.add_material(&mut scene, None, AlphaMode::Blend, [1.0; 4], true);
        let data = MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3], uvs: vec![glam::Vec2::ZERO; 3],
            indices: vec![0, 1, 2], ranges: vec![(0, 3, 0)], ..Default::default()
        };
        let text = renderer.add_mesh(&mut scene, &data);
        let glass = renderer.add_mesh(&mut scene, &MeshData {
            ranges: vec![(0, 3, 0), (0, 3, 1), (0, 3, 2)], ..data
        });
        let upper = renderer.add_instance(&mut scene, text, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
        let pane = renderer.add_instance(&mut scene, glass, DVec3::ZERO, Mat4::IDENTITY, vec![material; 3]);
        let lower = renderer.add_instance(&mut scene, text, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
        // A new bus creates its glass first, then its two rows, in different slot pools.
        for (mesh, slots, into) in [(glass, 3, pane), (text, 1, upper), (text, 1, lower)] {
            renderer.remove_instance(&mut scene, into);
            let new = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material; slots]);
            assert_eq!(renderer.recycle_instance(&mut scene, new, into), into);
        }
        let mut draws = vec![(0, 5.0, upper), (0, 5.0, pane), (0, 5.0, lower)];
        sort_blended_draws(&mut draws, |i| scene.instances[i].draw_order);
        assert_eq!(draws.iter().map(|d| d.2).collect::<Vec<_>>(), vec![pane, upper, lower]);
    }

    /// The lists planned on the pool are the ones planned in a row: blocks joined in order.
    #[test]
    #[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
    fn draw_lists_on_the_pool_are_the_same() {
        let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let renderer = pollster::block_on(Renderer::new_with(
            &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
        )).expect("test renderer");
        let mut scene = renderer.new_scene();
        let materials = [AlphaMode::Opaque, AlphaMode::Test, AlphaMode::Blend].map(|a| renderer.add_material(&mut scene, None, a, [1.0; 4], true));
        let data = MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3], uvs: vec![glam::Vec2::ZERO; 3],
            indices: vec![0, 1, 2], ranges: vec![(0, 3, 0), (0, 3, 1)], ..Default::default()
        };
        let meshes: Vec<_> = (0..7).map(|_| renderer.add_mesh(&mut scene, &data)).collect();
        let visible: Vec<(usize, f32, bool)> = (0..5000)
            .map(|k| {
                let m = vec![materials[k % 3], materials[(k / 3) % 3]];
                (renderer.add_instance(&mut scene, meshes[k % 7], DVec3::new(k as f64, 0.0, 0.0), Mat4::IDENTITY, m), k as f32, false)
            })
            .collect();
        let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
        let key = |d: &super::DrawItem| (d.pipe, d.mesh, d.range, d.material, d.look, d.entry);
        let (mut a, mut ab, mut b, mut bb) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        super::opaque_items(&scene, &visible, None, &mut a, &mut ab);
        super::opaque_items_parts(Some(&pool), &scene, &visible, None, &mut b, &mut bb);
        assert_eq!((a.iter().map(key).collect::<Vec<_>>(), ab), (b.iter().map(key).collect::<Vec<_>>(), bb));
        assert!(super::prepass_items(None, &scene, &visible, true, 1e9) == super::prepass_items(Some(&pool), &scene, &visible, true, 1e9));
    }
}
