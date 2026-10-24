//! The vehicles' view sync: what changed in a vehicle's simulation this frame, brought to
//! its renderer instances - its skinned meshes, its switched materials and free textures,
//! its interior lamps, its text and script textures - for the player's vehicle, the AI cars
//! and the coupled parts of both alike. The renders themselves are made in `scene`
//! (`World::add_vehicle*`); the vehicle's damage is in `vehicle_damage`.

use crate::scene::*;
use glam::{DVec3, Mat4};
use omsi_render::{Renderer, Scene};
use omsi_texture::Image;
use std::path::{Path, PathBuf};

/// Reshape the skinned meshes of a vehicle and its coupled parts whose bones moved (the
/// player's own, or an AI copy's - see `own_skinned_meshes`).
pub fn sync_skinned(
    renderer: &Renderer,
    scene: &mut Scene,
    vehicle: &mut omsi_sim::VehicleInstance,
    render: &mut VehicleRender,
    parts: &mut [VehicleRender],
) {
    for (i, id, last) in render.skinned.iter_mut() {
        let key = vehicle.skin_key(*i);
        if key == *last {
            continue;
        }
        if let Some((pos, nrm)) = vehicle.skinned(*i) {
            renderer.update_mesh(scene, *id, &pos, &nrm, &vehicle.ty.meshes[*i].data.uvs);
        }
        *last = key;
    }
    let n_vars = vehicle.state.vars.len();
    for (t, r) in vehicle.trailers.iter_mut().zip(parts.iter_mut()) {
        for (i, id, last) in r.skinned.iter_mut() {
            let key = t.skin_key(*i);
            if key == *last {
                continue;
            }
            if let Some((pos, nrm)) = t.skinned(*i, n_vars) {
                renderer.update_mesh(scene, *id, &pos, &nrm, &t.ty.meshes[*i].data.uvs);
            }
            *last = key;
        }
    }
}

/// Upload the text and script textures of a vehicle that changed since the last frame.
/// Switch the material of every slot a variable controls: `[matl_freetex]` loads the file a
/// string variable names, `[texchanges]` picks an entry of its master, `[matl_change]` picks
/// between the plain material and the `[matl_item]` variant.
pub fn sync_vehicle_materials(
    renderer: &Renderer,
    scene: &mut Scene,
    vehicle: &omsi_sim::VehicleInstance,
    render: &mut VehicleRender,
) {
    sync_materials(renderer, scene, vehicle, render);
}

/// A coupled part's switched materials and text textures, driven by the variables of the
/// vehicle it is coupled to (its scripts are shared with it).
pub fn sync_vehicle_part(
    renderer: &Renderer,
    scene: &mut Scene,
    main: &omsi_sim::VehicleInstance,
    part: &mut omsi_sim::vehicle::TrailerPart,
    render: &mut VehicleRender,
) {
    sync_interior_lamps(
        renderer,
        scene,
        &part.ty,
        part.position,
        part.body_rotation(),
        |li| interior_lamp_on(&part.ty, li, |n| main.var(n)),
        render,
    );
    sync_materials(renderer, scene, main, render);
    for i in part.update_text_textures(main) {
        if let (Some(Some(tex)), Some(img)) = (
            render.text_textures.get(i),
            part.text_textures[i].pending.take(),
        ) {
            let d = &part.text_textures[i].def;
            renderer.update_texture_mips(
                scene,
                *tex,
                &Image {
                    width: d.width.max(1) as u32,
                    height: d.height.max(1) as u32,
                    rgba: img,
                    has_alpha: true,
                },
            );
        }
    }
}

/// Resolve a vehicle `[matl_freetex]` name. OMSI add-ons often write paths such as
/// `..\\Texture\\mb_pmon\\warning.bmp`: if the normal lookup misses, retry the part
/// below the `Texture` component against the vehicle's texture search directories.
pub(crate) fn find_vehicle_freetex(name: &str, dirs: &[&Path]) -> Option<PathBuf> {
    if let Some(path) = omsi_texture::find_texture(name, dirs) {
        return Some(path);
    }
    let normalized = name.trim().replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
    let texture = parts.iter().position(|p| p.eq_ignore_ascii_case("Texture"))?;
    let rel = parts.get(texture + 1..)?.join("/");
    if rel.is_empty() {
        return None;
    }
    omsi_texture::find_texture(&rel, dirs)
}

/// Whether `key` is the cache key `sync_materials` makes of the free texture `name` under
/// the light maps `mask`: the name lower-cased (ASCII), then `#` and the mask if there is
/// one.
fn freetex_key_is(key: Option<&str>, name: &str, mask: Option<u32>) -> bool {
    let Some(key) = key.map(str::as_bytes) else {
        return false;
    };
    let name = name.as_bytes();
    let Some((head, rest)) = key.split_at_checked(name.len()) else {
        return false;
    };
    if !head.iter().zip(name).all(|(k, n)| *k == n.to_ascii_lowercase()) {
        return false;
    }
    match mask {
        None => rest.is_empty(),
        Some(m) => {
            let mut digits = [0u8; 10];
            let mut at = digits.len();
            let mut v = m;
            loop {
                at -= 1;
                digits[at] = b'0' + (v % 10) as u8;
                v /= 10;
                if v == 0 {
                    break;
                }
            }
            rest.first() == Some(&b'#') && rest[1..] == digits[at..]
        }
    }
}

/// What a vehicle's materials and lamps are switched by: its variables by name. The player's
/// vehicle is read in place; an AI car through its copy in the frame's snapshot ([`CarVars`]).
pub(crate) trait LookVars {
    fn var(&self, name: &str) -> Option<f32>;
    /// String variable `name`, trimmed ("" if there is none).
    fn str_var(&self, name: &str) -> &str;
}

impl LookVars for omsi_sim::VehicleInstance {
    fn var(&self, name: &str) -> Option<f32> {
        omsi_sim::VehicleInstance::var(self, name)
    }

    fn str_var(&self, name: &str) -> &str {
        self.ty.program.str_var(name).and_then(|i| self.state.str_vars.get(i as usize)).map_or("", |s| s.trim())
    }
}

/// An AI car's variables as the snapshot has them (`omsi_sim::view_events::CarLooks`), with
/// the string variables the view keeps of it (`CarRender::strs`).
pub(crate) struct CarVars<'a> {
    pub(crate) ty: &'a omsi_sim::VehicleType,
    pub(crate) index: &'a hashbrown::HashMap<String, omsi_script::VarId>,
    pub(crate) vars: &'a [f32],
    pub(crate) strs: &'a [String],
}

impl LookVars for CarVars<'_> {
    fn var(&self, name: &str) -> Option<f32> {
        omsi_script::compile::with_lower(name, |k| self.index.get(k).copied()).and_then(|i| self.vars.get(i as usize).copied())
    }

    fn str_var(&self, name: &str) -> &str {
        self.ty.program.str_var(name).and_then(|i| self.strs.get(i as usize)).map_or("", |s| s.trim())
    }
}

/// Whether `[interiorlight]` `li` of `ty` is on: OMSI enables the lamp when its variable is
/// 0.5 or more, there is no dimming.
pub(crate) fn interior_lamp_on(ty: &omsi_sim::VehicleType, li: usize, var: impl Fn(&str) -> Option<f32>) -> bool {
    let il = &ty.model.interior_lights[li];
    il.variable.trim().parse::<f32>().ok().or_else(|| var(&il.variable)).unwrap_or(0.0) >= 0.5
}

/// How many combinations of free pictures a slot keeps the materials of (see
/// `VariantSlot::free_built`).
const FREE_BUILT_KEEP: usize = 32;

pub(crate) fn sync_materials(
    renderer: &Renderer,
    scene: &mut Scene,
    vehicle: &impl LookVars,
    render: &mut VehicleRender,
) {
    for v in &mut render.variants {
        // the light maps switched on now (several `[matl_lightmap]`s, see `MultiLight`)
        let mask = v.lights.as_ref().map(|l| {
            let mut mask = 0u32;
            for (k, (_, var)) in l.maps.iter().enumerate() {
                let x = var.trim().parse::<f32>().ok().or_else(|| vehicle.var(var)).unwrap_or(0.0);
                // (on at 0.5, as each map's texture stage is, 0x7fe51f: a variable a script
                // dims through 0.1 lit the map at full)
                if x >= 0.5 {
                    mask |= 1 << k;
                }
            }
            mask
        });
        // A free texture under several light maps (an on-board unit's display dimmed by
        // two of them, #1650) is made with the maps switched on: the light maps' own
        // materials knew nothing of the free texture and put the slot's default texture
        // over the display, and a new picture came with the last map whatever its variable.
        // All free textures of the slot are laid into one material: an item records the
        // slot's list and its own after it, so a powered display whose item swaps the night
        // map keeps the picture the slot's string put on it, not the model's stand-in.
        let mut changed = !v.free.is_empty() && v.free_lights != Some(mask);
        for f in &mut v.free {
            // (the string read in place and compared with the name it had last time: the
            // name was copied, lower-cased and formatted for every free texture of every
            // vehicle each frame, though it changes only when a display does)
            let current = vehicle.str_var(&f.var);
            if freetex_key_is(f.current.as_deref(), current, None) {
                continue;
            }
            changed = true;
            let name = current.to_ascii_lowercase();
            if !f.pictures.contains_key(&name) {
                let dirs: Vec<&Path> = f.dirs.iter().map(|p| p.as_path()).collect();
                let found = if current.is_empty() {
                    None
                } else {
                    let resolved = find_vehicle_freetex(current, &dirs);
                    if resolved.is_none() {
                        log::warn!("vehicle [matl_freetex] '{}' = {:?}: texture not found", f.var, current);
                    }
                    resolved.and_then(|path| {
                        let mut shared = f.shared.lock();
                        // (a picture a display generator rewrites is reloaded in place,
                        // under the same id, see `FreeTextureReloads`)
                        let max_dimension = renderer.device.limits().max_texture_dimension_2d;
                        if let Some(e) = shared.get_mut(&path) {
                            e.1 += 1;
                            f.reloads.watch(path.clone(), e.0, &f.shared, max_dimension);
                            f.held.push(path);
                            return Some(e.0);
                        }
                        let (img, worth) = f.textures.get_gpu_fast(&path)?;
                        let id = renderer.add_texture_data(scene, &img);
                        if worth {
                            f.wants_upgrade.lock().push(path.clone());
                        }
                        attach_pbr(renderer, scene, &path, id);
                        shared.insert(path.clone(), (id, 1));
                        f.reloads.watch(path.clone(), id, &f.shared, max_dimension);
                        f.held.push(path);
                        Some(id)
                    })
                };
                f.pictures.insert(name.clone(), found);
            }
            f.current = Some(name);
        }
        if changed {
            v.free_lights = Some(mask);
            // (the pictures by name, and the light maps: what the materials are made of)
            let mut key = mask.map(|m| format!("#{m}")).unwrap_or_default();
            for f in &v.free {
                key.push('|');
                key.push_str(f.current.as_deref().unwrap_or(""));
            }
            if !v.free_built.contains_key(&key) {
                let mut spec = v.spec.clone();
                // the halves something is laid on; the others keep the slot's materials
                let mut halves = 0u32;
                for f in &v.free {
                    // (an empty string or a file not found leaves the slot its own texture
                    // from the mesh, with its addressing: a roller blind's idle "next" band
                    // then stays out of sight in its transparent border instead of covering
                    // the display as an untextured white plane)
                    if let Some(Some(tex)) = f.current.as_ref().and_then(|n| f.pictures.get(n)) {
                        spec = spec.with_freetex(f.key, *tex, f.diffuse, f.halves);
                    }
                    halves |= f.halves;
                }
                if let (Some(m), Some(l)) = (mask, v.lights.as_mut()) {
                    let tex = if m == 0 { None } else { l.composite(renderer, scene, m) };
                    spec.set_lightmap(tex);
                    halves = u32::MAX;
                }
                let built = spec.build_halves(renderer, scene, v.base_tex, halves, |_, m| m);
                v.free_built.insert(key.clone(), built);
            }
            let built = &v.free_built[&key];
            if let Some(m) = built.base {
                v.base = m;
            }
            if let Some(m) = built.item {
                v.item = m;
            }
            for (slot, m) in v.more.iter_mut().zip(&built.more) {
                if let Some(m) = m {
                    *slot = *m;
                }
            }
            // (a display that runs through many pictures - a clock, a ticket printer - would
            // keep the materials of every one while the vehicle lives: past a few dozen the
            // ones not on show go)
            if v.free_built.len() > FREE_BUILT_KEEP {
                let shown = |b: &omsi_scene::FreeBuilt| b.materials().any(|m| m == v.base || m == v.item || v.more.contains(&m));
                let gone: Vec<String> = v.free_built.iter().filter(|(k, b)| **k != key && !shown(b)).map(|(k, _)| k.clone()).collect();
                for k in gone {
                    if let Some(b) = v.free_built.remove(&k) {
                        for m in b.materials() {
                            renderer.free_material(scene, m);
                        }
                    }
                }
            }
        }
        if let (Some(l), Some(mask)) = (&mut v.lights, mask) {
            // (with a free texture, the free texture's materials above carry the maps)
            if mask != l.current && v.free.is_empty() {
                l.current = mask;
                let pair = if let Some(p) = l.cache.get(&mask) {
                    *p
                } else {
                    // none on: no light map at all (the set's own materials have the last
                    // map, lit whatever its variable said)
                    let tex = if mask == 0 { None } else { l.composite(renderer, scene, mask) };
                    let mut spec = v.spec.clone();
                    spec.set_lightmap(tex);
                    let p = spec.build(renderer, scene, v.base_tex);
                    l.cache.insert(mask, p);
                    p
                };
                v.base = pair.0;
                v.item = pair.1;
            }
        }
        if let Some(inst) = render.instances.get(v.mesh) {
            let m = v.material(|n| vehicle.var(n));
            if omsi_cfg::flags::OMSI_DEBUG_VARIANTS.var().is_some_and(|f| !f.is_empty() && v.var.to_ascii_lowercase().contains(&f.to_ascii_lowercase())) {
                log::info!("variant mesh {} slot {} var {} = {:?}: material {m} (base {}, item {})", v.mesh, v.slot, v.var, vehicle.var(&v.var), v.base, v.item);
            }
            renderer.set_material(scene, *inst, v.slot, m);
        }
    }
}

/// A vehicle's `[interiorlight]`s (`variable range r g b x y z`) as lamps for this frame:
/// points of light at their place in the vehicle, as strong as their variable (0..1) times
/// `range` (1 for a saloon lamp, 0.4 for a door lamp, 2 for the LiAZ's saloon rows - a door
/// lamp 0.4 m across could not reach the step 2 m below it, so it is no distance), each
/// lighting only the meshes that list it in
/// their `[illumination_interior]` - OMSI's four per mesh, or as many as a model lists
/// (up to `omsi_render::MAX_LAMPS_PER_MESH`), as OMSI switches those lights on
/// for just that mesh. Every set of lamps some mesh names gets a run of slots of its own
/// (the LiAZ 5292 has 32 lamps; only the first eight were drawn, and its saloon stayed dark).
/// The seats' sets (`PassPos::illumination`, the lamps that light a person sitting there)
/// get theirs too: see [`VehicleRender::seat_lamps`].
pub(crate) fn sync_interior_lamps(
    renderer: &Renderer,
    scene: &mut Scene,
    ty: &omsi_sim::VehicleType,
    position: DVec3,
    rotation: Mat4,
    on: impl Fn(usize) -> bool,
    render: &VehicleRender,
) {
    let n = ty.model.interior_lights.len();
    if n == 0 || render.instances.is_empty() {
        return;
    }
    let blocks = render.interior_blocks.get_or_init(|| {
        let mut blocks: Vec<(u32, Vec<usize>)> = Vec::new();
        let mut sets: Vec<(usize, Vec<usize>)> = Vec::new();
        for (i, _) in render.instances.iter().enumerate() {
            let Some(vm) = ty.meshes.get(i) else { continue };
            let set = lamp_set(&ty.model.meshes[vm.def_index].illumination_interior, n);
            if !set.is_empty() {
                sets.push((i, set));
            }
        }
        let mut distinct: Vec<Vec<usize>> = Vec::new();
        for (_, set) in &sets {
            if !distinct.contains(set) {
                distinct.push(set.clone());
            }
        }
        if let Some(cabin) = crate::driver::cabin_of(&ty.def) {
            for seat in cabin.driver_positions.iter().chain(&cabin.pass_positions) {
                let set = lamp_set(&seat.illumination, n);
                if !set.is_empty() && !distinct.contains(&set) {
                    distinct.push(set);
                }
            }
        }
        let total: u32 = distinct.iter().map(|d| d.len() as u32).sum();
        if omsi_cfg::flags::OMSI_DEBUG_INTERIOR.is_set() {
            log::info!("interior lamps of {}: {} lamps, {} instances, {} meshes lit by sets {:?}", ty.def.path.display(), n, render.instances.len(), sets.len(), distinct);
        }
        if total == 0 {
            return blocks;
        }
        let first = renderer.alloc_interior_lights(scene, total);
        render.interior_lamps.set(Some((first, total)));
        let mut at = first;
        for d in distinct {
            blocks.push((at, d.clone()));
            at += d.len() as u32;
        }
        for (i, set) in &sets {
            if let Some(b) = blocks.iter().find(|b| &b.1 == set) {
                renderer.set_interior_lamps(scene, render.instances[*i], b.0, set.len() as u32);
            }
        }
        blocks
    });
    for (first, set) in blocks {
        for (k, &li) in set.iter().enumerate() {
            let il = &ty.model.interior_lights[li];
            // on or off (`interior_lamp_on`)
            let on = on(li);
            let at = rotation.transform_vector3(glam::Vec3::from(il.pos));
            renderer.set_interior_light(
                scene,
                first + k as u32,
                omsi_render::PointLight {
                    position: position + at.as_dvec3(),
                    // OMSI's Direct3D light: a point light of the
                    // colour / 255, Range 100 m, attenuation 1 / (d² / range²) - full
                    // light at `range` metres, stronger closer in, a quarter at twice
                    radius: 100.0,
                    core: il.range.max(0.01),
                    color: [il.color[0] / 255.0, il.color[1] / 255.0, il.color[2] / 255.0],
                    intensity: if on { 1.0 } else { 0.0 },
                    ..Default::default()
                },
            );
        }
    }
}

pub fn sync_vehicle_textures(
    renderer: &Renderer,
    scene: &mut Scene,
    vehicle: &mut omsi_sim::VehicleInstance,
    render: &VehicleRender,
    budget: &mut usize,
) {
    vehicle.update_html_textures();
    sync_interior_lamps(
        renderer,
        scene,
        &vehicle.ty,
        vehicle.position,
        vehicle.body_rotation(),
        |li| interior_lamp_on(&vehicle.ty, li, |n| vehicle.var(n)),
        render,
    );
    for i in vehicle.update_text_textures() {
        if let (Some(Some(tex)), Some(img)) = (
            render.text_textures.get(i),
            vehicle.text_textures[i].pending.take(),
        ) {
            let d = &vehicle.text_textures[i].def;
            renderer.update_texture_mips(
                scene,
                *tex,
                &Image {
                    width: d.width.max(1) as u32,
                    height: d.height.max(1) as u32,
                    rgba: img,
                    has_alpha: true,
                },
            );
        }
    }
    let mut rebound = Vec::new();
    for (i, st) in vehicle.host.script_textures.iter_mut().enumerate() {
        // (far away what the scripts redraw goes up every half second: `displays_far`)
        if !render.displays_far {
            if let Some(Some(tex)) = render.script_textures.get(i) {
                if *budget == 0 {
                    continue;
                }
                let Some(rgba) = st.take_upload() else { continue };
                *budget = budget.saturating_sub(rgba.len());
                let img = Image {
                    width: st.width,
                    height: st.height,
                    rgba,
                    has_alpha: true,
                };
                if st.mipmaps {
                    if renderer.update_texture_mips(scene, *tex, &img) {
                        rebound.push(*tex);
                    }
                } else {
                    renderer.update_texture(scene, *tex, &img);
                }
            }
        }
    }
    renderer.rebind_textures(scene, &rebound);
}

#[cfg(test)]
mod freetex_key_tests {
    use super::freetex_key_is;

    /// The key as `sync_materials` makes it.
    fn key(name: &str, mask: Option<u32>) -> String {
        match mask {
            Some(m) => format!("{}#{m}", name.to_ascii_lowercase()),
            None => name.to_ascii_lowercase(),
        }
    }

    #[test]
    fn matches_the_key_made_of_the_same_name() {
        for name in ["", "Anz_Linie.BMP", "x#0", "Ä_Ziel", "a/b\\c"] {
            for mask in [None, Some(0), Some(3), Some(10), Some(u32::MAX)] {
                assert!(freetex_key_is(Some(&key(name, mask)), name, mask), "{name:?} {mask:?}");
            }
        }
    }

    #[test]
    fn differs_from_any_other_key() {
        let names = ["", "x", "X#0", "x#0", "xy", "y", "Ä", "ä"];
        let masks = [None, Some(0), Some(3), Some(30)];
        for a in names {
            for ma in masks {
                for b in names {
                    for mb in masks {
                        let same = key(a, ma) == key(b, mb);
                        assert_eq!(freetex_key_is(Some(&key(a, ma)), b, mb), same, "{a:?} {ma:?} / {b:?} {mb:?}");
                    }
                }
            }
        }
        assert!(!freetex_key_is(None, "", None));
    }
}

#[cfg(test)]
mod freetex_path_tests {
    use super::*;

    #[test]
    fn vehicle_freetex_retries_paths_below_texture_component() {
        let root = std::env::temp_dir().join("openomsi-freetex-path-test");
        let vehicle_texture = root.join("Vehicles/TestBus/Texture");
        let wanted = vehicle_texture.join("mb_pmon/alerta_FalhaCambio.bmp");
        std::fs::create_dir_all(wanted.parent().unwrap()).unwrap();
        std::fs::write(&wanted, b"x").unwrap();
        let dirs = [vehicle_texture.as_path()];
        let found = find_vehicle_freetex(
            r"..\Texture\mb_pmon\alerta_FalhaCambio.bmp",
            &dirs,
        );
        assert_eq!(found.as_deref(), Some(wanted.as_path()));
        let _ = std::fs::remove_dir_all(root);
    }
}
