//! Per-mesh material properties (visibility, lightmaps, alpha, texture offsets) from the
//! script variables, resolved once into a plan.

use super::*;

/// Per-mesh dynamic display properties evaluated from script variables every frame.
#[derive(Debug, Clone)]
pub struct MeshProps {
    pub visible: bool,
    /// Per material slot `[matl_lightmap]` strength.
    pub slot_light: Vec<f32>,
    /// Per material slot: `[matl_change]` item active (its night map shows).
    pub slot_night: Vec<f32>,
    /// Alpha multiplier per material slot of the mesh.
    pub slot_alpha: Vec<f32>,
    /// `[texcoordtransX/Y]` texture offset per material slot.
    pub slot_uv: Vec<[f32; 2]>,
    /// Brightness of the `[interiorlight]`s listed in the mesh's `[illumination_interior]`.
    pub interior: f32,
}

impl Default for MeshProps {
    fn default() -> Self {
        Self {
            visible: true,
            slot_alpha: Vec::new(),
            slot_light: Vec::new(),
            slot_night: Vec::new(),
            slot_uv: Vec::new(),
            interior: 0.0,
        }
    }
}

/// Per-mesh `[visible]`, `[matl_alphascale]` and texture offsets from variables. The
/// vehicles use the same rules through `PropsPlan`, resolved once; this is the plain
/// statement of them (and what the plan is tested against).
pub fn compute_mesh_props(ty: &VehicleType, var: &dyn Fn(&str) -> Option<f32>) -> Vec<MeshProps> {
    ty.meshes
        .iter()
        .map(|vm| {
            let def = &ty.model.meshes[vm.def_index];
            let mut props = MeshProps {
                slot_alpha: vec![1.0; vm.materials.len().max(1)],
                slot_light: vec![1.0; vm.materials.len().max(1)],
                slot_night: vec![1.0; vm.materials.len().max(1)],
                slot_uv: vec![[0.0; 2]; vm.materials.len().max(1)],
                ..Default::default()
            };
            for (mi, m) in def.materials.iter().enumerate() {
                // [matl_change] tex idx var + [matl_item]: the variant (night map) is active
                // when the variable is set
                if let Some((_, _, v)) = m.change.as_ref().filter(|_| !m.item) {
                    // (its items follow it: item n shows at n, 1 <= n <= their number)
                    let items = def.materials[mi + 1..].iter().take_while(|d| d.item).count().max(1) as f32;
                    if let Some(slot) = override_slot(&vm.materials, m) {
                        // (the item as Omsi.exe picks it: the variable rounded is 1; an
                        // undeclared one is 0 - see scene.rs `change_picks_item`)
                        let x = v
                            .trim()
                            .parse::<f32>()
                            .ok()
                            .or_else(|| var(v))
                            .unwrap_or(0.0);
                        let n = if x.is_finite() { x.round_ties_even() } else { 0.0 };
                        props.slot_night[slot] = if n >= 1.0 && n <= items { 1.0 } else { 0.0 };
                    }
                }
                // several light maps: the slot is as bright as the brightest (the texture
                // is made of the ones switched on, see scene.rs `MultiLight`)
                if !m.lightmaps.is_empty() {
                    if let Some(slot) = override_slot(&vm.materials, m) {
                        props.slot_light[slot] = 0.0;
                    }
                }
                for (_, v) in &m.lightmaps {
                    if let Some(slot) = override_slot(&vm.materials, m) {
                        let x = v
                            .trim()
                            .parse::<f32>()
                            .ok()
                            .or_else(|| var(v))
                            // (a variable the bus does not have: always on, see below)
                            .unwrap_or(1.0);
                        props.slot_light[slot] = props.slot_light[slot].max(if x >= 0.5 { 1.0 } else { 0.0 });
                    }
                }
            }
            // Unknown visibility names leave the mesh shown. They must stay unresolved:
            // the same missing name in [matl_change] selects the base material.
            // (several conditions, a range, a negation or `||`: see `MeshDef::visible_any`)
            if let Some(shown) = def.visible_by(&var) {
                props.visible = shown;
            }
            // [illumination_interior] a b c d: the interior lights (by index) lighting this mesh
            let mut interior = 0.0f32;
            for idx in &def.illumination_interior {
                if let Some(il) = usize::try_from(*idx)
                    .ok()
                    .and_then(|i| ty.model.interior_lights.get(i))
                {
                    let b = il
                        .variable
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .or_else(|| var(&il.variable))
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    interior = interior.max(b * il.range.clamp(0.0, 1.0));
                }
            }
            props.interior = interior * 0.5;
            for m in &def.materials {
                if let Some(v) = &m.alphascale {
                    let x = v
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .or_else(|| var(v))
                        .unwrap_or(0.0);
                    if let Some(slot) = override_slot(&vm.materials, m) {
                        let boost = if v.trim().to_ascii_lowercase().starts_with("rain_window") {
                            1.8
                        } else {
                            1.0
                        };
                        props.slot_alpha[slot] = (x * boost).clamp(0.0, 1.0);
                    }
                }
                // [texcoordtransX/Y] scroll one material slot's texture (each band of a
                // roller blind has its own variable)
                if let Some(v) = &m.texcoord_trans_x {
                    if let (Some(x), Some(slot)) = (var(v), override_slot(&vm.materials, m)) {
                        props.slot_uv[slot][0] = x;
                    }
                }
                if let Some(v) = &m.texcoord_trans_y {
                    if let (Some(x), Some(slot)) = (var(v), override_slot(&vm.materials, m)) {
                        props.slot_uv[slot][1] = x;
                    }
                }
            }
            props
        })
        .collect()
}

/// Where a mesh property takes its value from: a number written in model.cfg, a
/// variable, or nothing (the property keeps its default).
#[derive(Debug, Clone, Copy)]
pub(super) enum PropSource {
    Const(f32),
    Var(usize),
    Missing,
}

impl PropSource {
    pub(super) fn value(self, vars: &[f32], default: f32) -> f32 {
        match self {
            PropSource::Const(c) => c,
            PropSource::Var(i) => vars.get(i).copied().unwrap_or(default),
            PropSource::Missing => default,
        }
    }
}

/// The property sources of one mesh, in the terms of `compute_mesh_props`.
#[derive(Debug, Clone, Default)]
pub(super) struct MeshPlan {
    pub(super) slots: usize,
    /// `[matl_change]` (missing: base material) and `[matl_lightmap]` (missing: on) per slot.
    pub(super) night: Vec<(usize, PropSource)>,
    pub(super) light: Vec<(usize, PropSource)>,
    /// `[visible]`: each condition of `MeshDef::visible_any` with its variable's index (None:
    /// an unresolved name, whose condition is ignored) - shown when all of any one group hold;
    /// empty: always.
    pub(super) visible: Vec<Vec<(Option<usize>, omsi_model::VisibleCond)>>,
    /// `[illumination_interior]`: (brightness, range).
    pub(super) interior: Vec<(PropSource, f32)>,
    /// `[alphascale]`, `[texcoordtransX/Y]` variables per slot; the third field of `alpha`
    /// boosts a raindrop-film layer (`Rain_Window_*_Wetness`) so it stays visible instead of
    /// reading as the texture's own faint alpha (its drops are only a few percent opaque,
    /// so a middling wetness value was nearly invisible against the glass behind it).
    /// An undeclared `[alphascale]` variable reads 0.
    pub(super) alpha: Vec<(usize, PropSource, f32)>,
    pub(super) uv_x: Vec<(usize, usize)>,
    pub(super) uv_y: Vec<(usize, usize)>,
}

/// `compute_mesh_props` resolved once per vehicle: every frame it looked each variable up
/// by name (a lower-cased copy of the name each time), matched material slots by texture
/// name and allocated four vectors per mesh - for every mesh of every AI car, which made
/// the mesh properties the costliest part of the traffic's frame.
#[derive(Debug, Clone, Default)]
pub(super) struct PropsPlan {
    pub(super) meshes: Vec<MeshPlan>,
    /// The dirt overlays (`[alphascale] Dirt_Norm` / `Dirt_Wiped`): (mesh, slot, whether
    /// `Dirt_Wiped`), and where `Dirt_Wiped` is.
    pub(super) dirt: Vec<(usize, usize, bool)>,
    pub(super) dirt_wiped: Option<usize>,
    /// What each `[light_enh_2]` of the vehicle's own model follows, in order: a number or
    /// a variable (one the vehicle does not have reads 0).
    pub(super) lights: Vec<PropSource>,
    /// Size of the variable table the plan was resolved against (engine variables such
    /// as `Dirt_Norm` join it later, and an `[alphascale]` on one must then find it).
    pub(super) vars_seen: usize,
    pub(super) built: bool,
}

impl PropsPlan {
    pub(super) fn refresh(&mut self, ty: &VehicleType, var_index: &HashMap<String, omsi_script::VarId>) {
        if self.built && self.vars_seen == var_index.len() {
            return;
        }
        let var = |v: &str| var_index.get(&v.to_ascii_lowercase()).map(|&i| i as usize);
        let source = |v: &str| match v.trim().parse::<f32>() {
            Ok(c) => PropSource::Const(c),
            Err(_) => var(v).map(PropSource::Var).unwrap_or(PropSource::Missing),
        };
        self.meshes = ty
            .meshes
            .iter()
            .map(|vm| {
                let def = &ty.model.meshes[vm.def_index];
                let mut plan = MeshPlan {
                    slots: vm.materials.len().max(1),
                    ..Default::default()
                };
                for m in &def.materials {
                    let Some(slot) = override_slot(&vm.materials, m) else {
                        continue;
                    };
                    if let Some((_, _, v)) = &m.change {
                        plan.night.push((slot, source(v)));
                    }
                    for (_, v) in &m.lightmaps {
                        plan.light.push((slot, source(v)));
                    }
                    if let Some(name) = m.alphascale.as_deref() {
                        let boost =
                            if name.trim().to_ascii_lowercase().starts_with("rain_window") {
                                1.8
                            } else {
                                1.0
                            };
                        plan.alpha.push((slot, source(name), boost));
                    }
                    if let Some(i) = m.texcoord_trans_x.as_deref().and_then(var) {
                        plan.uv_x.push((slot, i));
                    }
                    if let Some(i) = m.texcoord_trans_y.as_deref().and_then(var) {
                        plan.uv_y.push((slot, i));
                    }
                }
                plan.visible = def
                    .visible_groups()
                    .map(|g| g.iter().map(|c| (var(&c.variable), c.clone())).collect())
                    .collect();
                for idx in &def.illumination_interior {
                    if let Some(il) = usize::try_from(*idx)
                        .ok()
                        .and_then(|i| ty.model.interior_lights.get(i))
                    {
                        plan.interior
                            .push((source(&il.variable), il.range.clamp(0.0, 1.0)));
                    }
                }
                plan
            })
            .collect();
        self.dirt.clear();
        for (i, vm) in ty.meshes.iter().enumerate() {
            for m in &ty.model.meshes[vm.def_index].materials {
                let Some(name) = m.alphascale.as_deref().map(str::trim) else { continue };
                let wiped = if name.eq_ignore_ascii_case("dirt_norm") {
                    false
                } else if name.eq_ignore_ascii_case("dirt_wiped") {
                    true
                } else {
                    continue;
                };
                if let Some(slot) = override_slot(&vm.materials, m) {
                    self.dirt.push((i, slot, wiped));
                }
            }
        }
        self.dirt_wiped = var("dirt_wiped");
        self.lights = ty
            .model
            .meshes
            .iter()
            .flat_map(|md| &md.light_enh_2)
            .map(|l| {
                let n = l.variable.trim();
                match n.parse::<f32>() {
                    Ok(c) => PropSource::Const(c),
                    Err(_) => var(n).map(PropSource::Var).unwrap_or(PropSource::Missing),
                }
            })
            .collect();
        self.vars_seen = var_index.len();
        self.built = true;
    }

    /// The mesh properties for the current variables, written into `out` in place.
    pub(super) fn apply(&self, vars: &[f32], out: &mut Vec<MeshProps>) {
        out.resize_with(self.meshes.len(), MeshProps::default);
        for (plan, props) in self.meshes.iter().zip(out.iter_mut()) {
            let n = plan.slots;
            for (v, fill) in [
                (&mut props.slot_alpha, 1.0),
                (&mut props.slot_light, 1.0),
                (&mut props.slot_night, 1.0),
            ] {
                v.clear();
                v.resize(n, fill);
            }
            props.slot_uv.clear();
            props.slot_uv.resize(n, [0.0; 2]);
            props.visible = true;
            for &(slot, src) in &plan.night {
                let x = src.value(vars, 0.0);
                props.slot_night[slot] = if x.is_finite() && x.round_ties_even() == 1.0 { 1.0 } else { 0.0 };
            }
            for &(slot, _) in &plan.light {
                props.slot_light[slot] = 0.0;
            }
            // (a light map is on at its variable's 0.5 and off below - Omsi.exe skips the
            // texture stage of one whose variable reads under 0.5, 0x7fe51f - never half lit;
            // one whose variable the bus does not have - index -1 - is always on, 0x7fe4e7)
            for &(slot, src) in &plan.light {
                props.slot_light[slot] = props.slot_light[slot].max(if src.value(vars, 1.0) >= 0.5 { 1.0 } else { 0.0 });
            }
            if !plan.visible.is_empty() {
                props.visible = plan.visible.iter().any(|g| g.iter().all(|(i, c)| {
                    i.and_then(|i| vars.get(i).copied()).is_none_or(|x| c.holds(x))
                }));
            }
            let mut interior = 0.0f32;
            for &(src, range) in &plan.interior {
                interior = interior.max(src.value(vars, 0.0).clamp(0.0, 1.0) * range);
            }
            props.interior = interior * 0.5;
            for &(slot, src, boost) in &plan.alpha {
                let x = match src {
                    PropSource::Missing => 0.0,
                    src => src.value(vars, 1.0),
                };
                props.slot_alpha[slot] = (x * boost).clamp(0.0, 1.0);
            }
            for &(slot, i) in &plan.uv_x {
                props.slot_uv[slot][0] = vars.get(i).copied().unwrap_or(0.0);
            }
            for &(slot, i) in &plan.uv_y {
                props.slot_uv[slot][1] = vars.get(i).copied().unwrap_or(0.0);
            }
        }
    }
}
