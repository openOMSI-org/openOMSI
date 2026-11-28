//! Emissive slots the scripts drive (openOMSI `[matl_emissive]`, #2926): each frame the
//! slot's level (its variable, 0 off .. 1 full), faded as `[matl_emissive_fade]` says, and
//! its temperature (`[matl_emissive_kelvin]`'s variable, or a filament's that follows the
//! level), handed to the renderer in the slot's light value (`omsi_model::slot_value`).
//! A slot with a `[matl_lightmap]` keeps that value for its light map, and its emission
//! shines as the material says, undriven.

use super::props::{PropSource, PropsPlan};
use super::*;

/// One driven emissive slot.
#[derive(Debug, Clone, Copy)]
pub(super) struct EmitSlot {
    pub(super) mesh: usize,
    pub(super) slot: usize,
    /// The level's source (missing: full).
    level: PropSource,
    /// The temperature's source (K), where a variable gives it.
    kelvin: Option<PropSource>,
    /// A filament's temperature at full power (K).
    filament: Option<f32>,
    /// Seconds to 63 % of a new level, rising and falling.
    fade: [f32; 2],
}

/// The driven emissive slots of a vehicle type, their variables found by `var`.
pub(super) fn plan(ty: &VehicleType, var: &dyn Fn(&str) -> Option<usize>) -> Vec<EmitSlot> {
    let source = |v: &str| match v.trim().parse::<f32>() {
        Ok(c) => PropSource::Const(c),
        Err(_) => var(v).map(PropSource::Var).unwrap_or(PropSource::Missing),
    };
    let mut out = Vec::new();
    for (mesh, vm) in ty.meshes.iter().enumerate() {
        let def = &ty.model.meshes[vm.def_index];
        for m in def.materials.iter().filter(|m| !m.item) {
            let Some(e) = m.emission.as_ref().filter(|e| e.is_driven()) else {
                continue;
            };
            let Some(slot) = override_slot(&vm.materials, m) else {
                continue;
            };
            let lit = def
                .materials
                .iter()
                .any(|o| !o.lightmaps.is_empty() && override_slot(&vm.materials, o) == Some(slot));
            if lit {
                continue;
            }
            out.push(EmitSlot {
                mesh,
                slot,
                level: e.variable.as_deref().map_or(PropSource::Missing, source),
                kelvin: e.kelvin_variable.as_deref().map(source),
                filament: e.kelvin.filter(|_| e.filament),
                fade: e.fade,
            });
        }
    }
    out
}

impl PropsPlan {
    /// The driven emissive slots' values for this frame, `dt` on from the last, written over
    /// the slots' light values in `out` (after `apply`).
    pub(super) fn apply_emission(&mut self, dt: f32, vars: &[f32], out: &mut [MeshProps]) {
        if self.emit_level.len() != self.emit.len() {
            // (a new plan: each slot starts where its variable stands, no fade from dark)
            self.emit_level = self
                .emit
                .iter()
                .map(|e| e.level.value(vars, 1.0).clamp(0.0, 1.0))
                .collect();
        }
        for (e, level) in self.emit.iter().zip(self.emit_level.iter_mut()) {
            let target = e.level.value(vars, 1.0);
            let target = if target.is_finite() {
                target.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let tau = if target > *level {
                e.fade[0]
            } else {
                e.fade[1]
            };
            *level = if tau > 0.0 && dt > 0.0 {
                *level + (target - *level) * (1.0 - (-dt / tau).exp())
            } else if tau > 0.0 {
                *level
            } else {
                target
            };
            let kelvin = match (e.kelvin, e.filament) {
                (Some(src), _) => Some(src.value(vars, 6500.0)),
                (None, Some(full)) => Some(omsi_model::filament_kelvin(full, *level)),
                (None, None) => None,
            };
            if let Some(v) = out
                .get_mut(e.mesh)
                .and_then(|p| p.slot_light.get_mut(e.slot))
            {
                *v = omsi_model::slot_value(*level, kelvin);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A driven filament: full at once where the plan starts, then off with a fade - the level
    /// falls as `[matl_emissive_fade]` says and the colour warms with it.
    #[test]
    fn a_filament_fades_and_warms() {
        let mut plan = PropsPlan {
            emit: vec![EmitSlot {
                mesh: 0,
                slot: 0,
                level: PropSource::Var(0),
                kelvin: None,
                filament: Some(3200.0),
                fade: [0.5, 0.5],
            }],
            ..Default::default()
        };
        let mut out = vec![MeshProps {
            slot_light: vec![1.0],
            ..Default::default()
        }];
        let decode = |v: f32| ((-v).floor(), ((-v).fract() / 0.999).min(1.0));
        plan.apply_emission(1.0 / 60.0, &[1.0], &mut out);
        let (k, level) = decode(out[0].slot_light[0]);
        assert_eq!(k, 3200.0);
        assert!(level > 0.99, "{level}");
        // switched off: half a second later at 1/e of full, and warmer
        for _ in 0..30 {
            plan.apply_emission(1.0 / 60.0, &[0.0], &mut out);
        }
        let (k, level) = decode(out[0].slot_light[0]);
        assert!((level - (-1.0f32).exp()).abs() < 0.02, "{level}");
        assert!(k < 3200.0 && k > 2000.0, "{k}");
        // a level without a temperature is the value itself
        plan.emit[0].filament = None;
        plan.apply_emission(0.0, &[0.0], &mut out);
        assert!(out[0].slot_light[0] >= 0.0 && out[0].slot_light[0] < 0.5);
    }
}
