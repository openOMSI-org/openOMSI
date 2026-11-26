//! The material slots of a scenery lamp or scripted object that its variables switch,
//! fade, scroll or light (`[alphascale]`, `[matl_lightmap]`, `[texcoordtransX/Y]`,
//! `[matl_emissive]`).

use super::*;

/// The material slots of a lamp's mesh switched by its variables: `[alphascale] var`
/// fades a slot (a lens shown by its alpha rather than by a `[visible]` mesh, as the
/// Korean maps' signals do, #826) and `[matl_lightmap] tex var` lights it.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct LampSlots {
    pub count: usize,
    pub alpha: Vec<(usize, String)>,
    pub light: Vec<(usize, String)>,
    pub uv: Vec<(usize, Option<String>, Option<String>)>,
    /// The slots whose `[matl_emissive]` the object's script drives (see [`EmitSlot`]).
    pub emit: Vec<EmitSlot>,
}

/// A slot whose emission (`[matl_emissive]` and companions) the object's variables drive, as
/// a vehicle's (`omsi_sim::vehicle::emission`): its level (0 off .. 1 full; no variable:
/// full), its temperature's variable, a filament's full temperature, and the fade up and
/// down (seconds to 63 %).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct EmitSlot {
    pub slot: usize,
    pub level: Option<String>,
    pub kelvin: Option<String>,
    pub filament: Option<f32>,
    pub fade: [f32; 2],
}

impl EmitSlot {
    /// The level the variables ask for now (0..1; no variable, or one the object does not
    /// have: full).
    pub fn target(&self, value: &dyn Fn(&str) -> Option<f32>) -> f32 {
        let target = self
            .level
            .as_deref()
            .map_or(1.0, |v| value(v).unwrap_or(1.0));
        if target.is_finite() {
            target.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The level `dt` seconds on from `level` towards [`EmitSlot::target`].
    pub fn step(&self, level: f32, dt: f32, value: &dyn Fn(&str) -> Option<f32>) -> f32 {
        let target = self.target(value);
        let tau = if target > level {
            self.fade[0]
        } else {
            self.fade[1]
        };
        if tau > 0.0 {
            level + (target - level) * (1.0 - (-dt.max(0.0) / tau).exp())
        } else {
            target
        }
    }

    /// The slot's light value for the renderer at `level` (`omsi_model::slot_value`).
    pub fn value(&self, level: f32, value: &dyn Fn(&str) -> Option<f32>) -> f32 {
        let kelvin = match (&self.kelvin, self.filament) {
            (Some(v), _) => Some(value(v).unwrap_or(6500.0)),
            (None, Some(full)) => Some(omsi_model::filament_kelvin(full, level)),
            (None, None) => None,
        };
        omsi_model::slot_value(level, kelvin)
    }
}

impl LampSlots {
    /// The `[alphascale]` and `[matl_lightmap]` variables of a mesh's material slots.
    pub fn of_mesh(
        o3d_mats: &[omsi_o3d::Material],
        overrides: &[MaterialDef],
        count: usize,
    ) -> LampSlots {
        let mut l = LampSlots {
            count,
            ..Default::default()
        };
        for o in overrides.iter().filter(|o| !o.item) {
            let Some(slot) = omsi_sim::vehicle::override_slot(o3d_mats, o) else {
                continue;
            };
            if let Some(v) = o.alphascale.as_ref().filter(|v| !v.trim().is_empty()) {
                l.alpha.push((slot, v.trim().to_string()));
            }
            if let Some((_, v)) = &o.lightmap {
                l.light.push((slot, v.trim().to_string()));
            }
            let trans = |v: &Option<String>| {
                v.as_ref()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            if o.texcoord_trans_x.is_some() || o.texcoord_trans_y.is_some() {
                l.uv.push((slot, trans(&o.texcoord_trans_x), trans(&o.texcoord_trans_y)));
            }
        }
        // (a slot with a light map keeps its light value for that, as on a vehicle)
        for o in overrides.iter().filter(|o| !o.item) {
            let Some(e) = o.emission.as_ref().filter(|e| e.is_driven()) else {
                continue;
            };
            let Some(slot) = omsi_sim::vehicle::override_slot(o3d_mats, o) else {
                continue;
            };
            let lit = overrides.iter().any(|m| {
                !m.lightmaps.is_empty()
                    && omsi_sim::vehicle::override_slot(o3d_mats, m) == Some(slot)
            });
            if lit || l.emit.iter().any(|x| x.slot == slot) {
                continue;
            }
            let name = |v: &Option<String>| {
                v.as_ref()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            l.emit.push(EmitSlot {
                slot,
                level: name(&e.variable),
                kelvin: name(&e.kelvin_variable),
                filament: e.kelvin.filter(|_| e.filament),
                fade: e.fade,
            });
        }
        l
    }

    /// The slots' light values with the driven emissive slots at `levels` (parallel to
    /// `emit`), the others at 1 (full, as a slot nothing drives).
    pub fn emit_values(&self, levels: &[f32], value: &dyn Fn(&str) -> Option<f32>) -> Vec<f32> {
        let mut light = vec![1.0; self.count.max(1)];
        for (e, &level) in self.emit.iter().zip(levels) {
            if let Some(l) = light.get_mut(e.slot) {
                *l = e.value(level, value);
            }
        }
        light
    }

    pub fn uv_values(&self, value: &dyn Fn(&str) -> Option<f32>) -> Vec<[f32; 2]> {
        let mut uv = vec![
            [0.0; 2];
            if self.uv.is_empty() {
                0
            } else {
                self.count.max(1)
            }
        ];
        for (slot, x, y) in &self.uv {
            if let Some(u) = uv.get_mut(*slot) {
                if let Some(x) = x {
                    u[0] = value(x).unwrap_or(0.0);
                }
                if let Some(y) = y {
                    u[1] = value(y).unwrap_or(0.0);
                }
            }
        }
        uv
    }

    pub fn is_empty(&self) -> bool {
        self.alpha.is_empty() && self.light.is_empty()
    }

    /// The slots' alpha and light-map switch from the lamp's variables (`value`: `None`
    /// for a variable the lamp does not have). Alpha: the variable's value, a slot without
    /// one opaque as authored. Light map: on from 0.5; a variable the lamp does not have
    /// (or none at all) leaves it on, as Omsi.exe's stage does with an unregistered one.
    pub fn values(&self, value: &dyn Fn(&str) -> Option<f32>) -> (Vec<f32>, Vec<f32>) {
        let mut alpha = vec![1.0; self.count.max(1)];
        let mut light = vec![1.0; self.count.max(1)];
        for (slot, v) in &self.alpha {
            if let (Some(a), Some(x)) = (alpha.get_mut(*slot), value(v)) {
                *a = x.clamp(0.0, 1.0);
            }
        }
        for (slot, v) in &self.light {
            if let (Some(l), Some(x)) = (
                light.get_mut(*slot),
                (!v.is_empty()).then(|| value(v)).flatten(),
            ) {
                *l = if x >= 0.5 { 1.0 } else { 0.0 };
            }
        }
        (alpha, light)
    }
}
