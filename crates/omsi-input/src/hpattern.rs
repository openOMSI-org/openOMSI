//! H-pattern gate actions, including vehicles exposing only the held (`_fest`) variant.

pub fn is_gate(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let Some(gate) = name.strip_prefix("kw_s_") else {
        return false;
    };
    let gate = gate.strip_suffix("_fest").unwrap_or(gate);
    gate == "r" || gate == "n" || gate.parse::<u32>().is_ok()
}

pub fn has_held_bindings(cfg: &[crate::controllers::DeviceCfg]) -> bool {
    cfg.iter().any(|d| {
        d.buttons
            .iter()
            .any(|(name, _)| is_gate(name) && name.to_ascii_lowercase().ends_with("_fest"))
    })
}

pub fn resolve(program: &omsi_script::Program, name: &str) -> Option<String> {
    if !is_gate(name) {
        return None;
    }
    if program.trigger(name).is_some() {
        return Some(name.to_string());
    }
    let lower = name.to_ascii_lowercase();
    let other = match lower.strip_suffix("_fest") {
        Some(base) => base.to_string(),
        None => format!("{lower}_fest"),
    };
    program.trigger(&other).map(|_| other)
}

/// Returns None for non-manual actions, so automatic gearboxes keep their own scripts.
/// Release the exact supported gate first; neutral is an extra action only when enabled.
pub fn action(
    vehicle: &mut omsi_sim::VehicleInstance,
    name: &str,
    pressed: bool,
    momentary: bool,
) -> Option<bool> {
    if !is_gate(name) || !vehicle.ty.program.manual_gearbox() {
        return None;
    }
    let name = resolve(&vehicle.ty.program, name)?;
    let done = vehicle.trigger(&if pressed {
        name.clone()
    } else {
        format!("{name}_off")
    });
    if pressed || !momentary {
        return Some(done || vehicle.ty.program.trigger(&name).is_some());
    }
    for neutral in ["kw_s_N", "kw_s_N_fest"] {
        if vehicle.ty.program.trigger(neutral).is_none() {
            continue;
        }
        // A neutral trigger can require the clutch just like a numbered gate. Operate it
        // briefly when automatic clutch is enabled; leave a physical clutch untouched.
        let clutch = vehicle.var("Clutch");
        let assist = vehicle.host.auto_clutch >= 0.5
            && !vehicle
                .ty
                .program
                .reads_sys(omsi_script::SysVar::AutoClutch);
        if assist {
            vehicle.set_var("Clutch", 1.0);
        }
        let selected = vehicle.trigger(neutral);
        if selected {
            vehicle.trigger(&format!("{neutral}_off"));
        }
        if assist {
            if let Some(clutch) = clutch {
                vehicle.set_var("Clutch", clutch);
            }
        }
        if selected {
            return Some(true);
        }
    }
    Some(done)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;

    pub fn vehicle(
        held: bool,
        gated_neutral: bool,
        explicit_off: bool,
    ) -> omsi_sim::VehicleInstance {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "omsi_hpattern_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let suffix = if held { "_fest" } else { "" };
        let mut script = String::new();
        for (gate, gear) in [("1", 1), ("2", 2), ("R", -1)] {
            script.push_str(&format!(
                "{{trigger:kw_s_{gate}{suffix}}}\n{gear} (S.L.gear)\n1 (S.L.gate_held)\n{{end}}\n"
            ));
            if explicit_off {
                script.push_str(&format!("{{trigger:kw_s_{gate}{suffix}_off}}\n0 (S.L.gate_held)\n1 (S.L.off_seen)\n{{end}}\n"));
            }
        }
        let neutral = if gated_neutral {
            "(L.L.Clutch) 1 =\n{if}\n0 (S.L.gear)\n{endif}"
        } else {
            "0 (S.L.gear)"
        };
        script.push_str(&format!("{{trigger:kw_s_N{suffix}}}\n{neutral}\n{{end}}\n"));
        std::fs::write(dir.join("gate.osc"), script).unwrap();
        std::fs::write(dir.join("vars.txt"), "gear\ngate_held\noff_seen\nClutch\n").unwrap();
        let program = omsi_script::compile(&omsi_script::CompileInput {
            scripts: vec![dir.join("gate.osc")],
            varlists: vec![dir.join("vars.txt")],
            ..Default::default()
        });
        assert!(program.errors.is_empty(), "{:?}", program.errors);
        assert!(program.manual_gearbox());
        let ty = Arc::new(omsi_sim::VehicleType {
            def: Default::default(),
            model: Default::default(),
            model_dir: dir.clone(),
            program: Arc::new(program),
            meshes: Vec::new(),
            lod_meshes: Vec::new(),
            paint_schemes: Vec::new(),
            texchanges: Vec::new(),
            wheel_meshes: Vec::new(),
            suspension_axles: Vec::new(),
            missing_packs: Vec::new(),
            mesh_bounds: Vec::new(),
            mesh_boxes: Vec::new(),
        });
        std::fs::remove_dir_all(dir).unwrap();
        let mut vehicle =
            omsi_sim::VehicleInstance::new(ty, omsi_sim::VehicleHost::new(Default::default()));
        vehicle.host.auto_clutch = 1.0;
        vehicle
    }

    #[test]
    fn gear_down_up_neutral_for_normal_and_held_bindings() {
        for held in [false, true] {
            for binding in ["kw_s_1", "kw_s_1_fest", "KW_S_1_FEST"] {
                for explicit_off in [false, true] {
                    let mut v = vehicle(held, false, explicit_off);
                    assert_eq!(action(&mut v, binding, true, true), Some(true));
                    assert_eq!(v.var("gear"), Some(1.0));
                    assert_eq!(action(&mut v, binding, false, true), Some(true));
                    assert_eq!(v.var("gear"), Some(0.0));
                    if explicit_off {
                        assert_eq!(v.var("gate_held"), Some(0.0));
                        assert_eq!(v.var("off_seen"), Some(1.0));
                    }
                }
            }
        }
    }

    #[test]
    fn disabled_option_does_not_force_neutral_but_still_fires_off() {
        let mut v = vehicle(true, false, true);
        action(&mut v, "kw_s_R_fest", true, false);
        action(&mut v, "kw_s_R_fest", false, false);
        assert_eq!(v.var("gear"), Some(-1.0));
        assert_eq!(v.var("gate_held"), Some(0.0));
    }

    #[test]
    fn neutral_assistance_respects_auto_clutch_and_restores_its_state() {
        for assisted in [false, true] {
            let mut v = vehicle(false, true, true);
            v.host.auto_clutch = if assisted { 1.0 } else { 0.0 };
            v.set_var("Clutch", 0.25);
            action(&mut v, "kw_s_2", true, true);
            action(&mut v, "kw_s_2", false, true);
            assert_eq!(v.var("gear"), Some(if assisted { 0.0 } else { 2.0 }));
            assert_eq!(v.var("Clutch"), Some(0.25));
        }
    }

    #[test]
    fn automatic_and_unrelated_actions_keep_their_existing_dispatch() {
        let mut v = vehicle(false, false, true);
        assert_eq!(action(&mut v, "horn", true, true), None);
        let ty = Arc::get_mut(&mut v.ty).unwrap();
        let gate = ty.program.trigger("kw_s_1").unwrap();
        Arc::make_mut(&mut ty.program)
            .triggers
            .insert("automatic_d".into(), gate);
        assert!(!v.ty.program.manual_gearbox());
        assert_eq!(action(&mut v, "kw_s_1", false, true), None);
    }

    #[test]
    fn gate_off_is_retained_when_the_bus_has_no_neutral_trigger() {
        let mut v = vehicle(true, false, true);
        let ty = Arc::get_mut(&mut v.ty).unwrap();
        Arc::make_mut(&mut ty.program)
            .triggers
            .remove("kw_s_n_fest");
        action(&mut v, "kw_s_1_fest", true, true);
        action(&mut v, "kw_s_1_fest", false, true);
        assert_eq!(v.var("gate_held"), Some(0.0));
        assert_eq!(v.var("off_seen"), Some(1.0));
    }

    #[test]
    fn recommendations_require_held_gate_bindings() {
        let mut cfg = crate::controllers::DeviceCfg::default();
        cfg.buttons = vec![("horn_fest".into(), "0".into())];
        assert!(!has_held_bindings(&[cfg.clone()]));
        cfg.buttons.push(("kw_s_2_fest".into(), "0".into()));
        assert!(has_held_bindings(&[cfg]));
    }
}
