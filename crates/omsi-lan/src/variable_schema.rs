use omsi_net::var_schema::Schema;
use omsi_script::Program;
use omsi_sim::VehicleInstance;

fn reference(out: &mut Vec<String>, program: &Program, name: &str, string: bool) {
    let name = name.trim().to_ascii_lowercase();
    if name.is_empty() || (!string && name.parse::<f32>().is_ok()) {
        return;
    }
    let resolved = if string {
        program.str_var(&name).is_some()
    } else {
        program.var(&name).is_some()
    };
    out.push(format!(
        "{}:{name}",
        if resolved {
            if string { "s" } else { "f" }
        } else {
            "u"
        }
    ));
}

fn model_references(out: &mut Vec<String>, program: &Program, model: &omsi_model::Model) {
    let mut float = |name: &str| reference(out, program, name, false);
    for mesh in &model.meshes {
        if let Some((name, _)) = &mesh.visible {
            float(name);
        }
        for anim in &mesh.animations {
            float(&anim.variable);
        }
        for light in &mesh.light_enh {
            float(&light.variable);
        }
        for light in &mesh.light_enh_2 {
            float(&light.variable);
        }
        for mat in &mesh.materials {
            if let Some((_, _, name)) = &mat.change {
                float(name);
            }
            if let Some((_, name)) = &mat.lightmap {
                float(name);
            }
            if let Some(e) = &mat.emission {
                for name in [&e.variable, &e.kelvin_variable].into_iter().flatten() {
                    float(name);
                }
            }
            for (_, name) in &mat.lightmaps {
                float(name);
            }
            for name in [
                &mat.alphascale,
                &mat.texcoord_trans_x,
                &mat.texcoord_trans_y,
            ]
            .into_iter()
            .flatten()
            {
                float(name);
            }
        }
    }
    for light in &model.interior_lights {
        float(&light.variable);
    }
    for light in &model.spotlights_2 {
        float(&light.variable);
    }
    for light in &model.spotlight_cookies {
        float(&light.variable);
        for name in [&light.yaw, &light.pitch].into_iter().flatten() {
            float(name);
        }
    }
    for ctc in &model.ctc {
        float(&ctc.variable);
    }
    for (name, _) in &model.set_vars {
        float(name);
    }
    for item in &model.items {
        for (name, _) in &item.set_vars {
            float(name);
        }
    }
    for ps in model.particle_systems() {
        for range in [
            &ps.velocity,
            &ps.freq,
            &ps.life,
            &ps.brake,
            &ps.gravity,
            &ps.size_start,
            &ps.size_grow,
            &ps.alpha_initial,
            &ps.alpha_final,
        ]
        .into_iter()
        .chain(ps.rgb.iter())
        .chain(ps.burst.iter())
        {
            for value in [&range.0, &range.1] {
                if let omsi_model::PsValue::Var(name) = value {
                    float(name);
                }
            }
        }
    }
    for texture in &model.text_textures {
        reference(out, program, &texture.variable, true);
    }
    for mesh in &model.meshes {
        for mat in &mesh.materials {
            if let Some((_, name)) = &mat.freetex {
                reference(out, program, name, true);
            }
        }
    }
}

fn sound_references(out: &mut Vec<String>, program: &Program, sound: &omsi_vehicle::SoundCfg) {
    for entry in &sound.sounds {
        reference(out, program, &entry.pitch_variable, false);
        for curve in &entry.vol_curves {
            reference(out, program, &curve.variable, false);
        }
        for condition in &entry.conditions {
            reference(out, program, &condition.variable, false);
        }
    }
}

pub fn for_vehicle(vehicle: &VehicleInstance) -> Schema {
    let program = &vehicle.ty.program;
    let table = crate::sync::var_table(program);
    let mut schema = Schema {
        floats: table
            .floats
            .iter()
            .map(|id| program.var_names[*id as usize].to_ascii_lowercase())
            .collect(),
        strings: table
            .strings
            .iter()
            .map(|id| program.str_var_names[*id as usize].to_ascii_lowercase())
            .collect(),
        ..Default::default()
    };
    for ty in std::iter::once(&vehicle.ty).chain(vehicle.trailers.iter().map(|t| &t.ty)) {
        model_references(&mut schema.model, program, &ty.model);
        for change in &ty.texchanges {
            reference(&mut schema.model, program, &change.variable, false);
        }
        // Include the full interior and AI sound configurations, without pose filtering/caps.
        for rel in [&ty.def.sound, &ty.def.sound_ai].into_iter().flatten() {
            let path = omsi_cfg::resolve_path(ty.def.dir(), rel);
            match omsi_vehicle::SoundCfg::load(&path) {
                Ok(sound) => sound_references(&mut schema.sound, program, &sound),
                Err(e) => {
                    log::warn!("LAN variable compatibility: {}: {e}", path.display());
                    schema.sound.push("u:unreadable_sound_configuration".into());
                }
            }
        }
    }
    schema.model.sort_unstable();
    schema.model.dedup();
    schema.sound.sort_unstable();
    schema.sound.dedup();
    schema
}

pub fn mapping(
    program: &Program,
    local: &Schema,
    remote: &Schema,
) -> Option<crate::sync::VarTable> {
    if !local.compatible(remote) {
        return None;
    }
    Some(crate::sync::VarTable {
        hash: remote.table_hash(),
        floats: remote
            .floats
            .iter()
            .map(|name| program.var(name).unwrap_or(u32::MAX))
            .collect(),
        strings: remote
            .strings
            .iter()
            .map(|name| program.str_var(name).unwrap_or(u32::MAX))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_map_all_common_variables_and_strings_by_name() {
        let mut local = Program::default();
        for name in ["local_only", "private_shared", "door"] {
            local.declare_var(name);
        }
        local.declare_str_var("display");
        local.declare_str_var("private_string");
        let local_schema = Schema {
            floats: vec!["door".into(), "local_only".into(), "private_shared".into()],
            strings: vec!["display".into(), "private_string".into()],
            model: vec!["f:door".into(), "s:display".into()],
            sound: vec!["f:door".into()],
        };
        let mut remote = local_schema.clone();
        remote.floats = vec!["door".into(), "private_shared".into(), "remote_only".into()];
        remote.strings.push("remote_string".into());
        let map = mapping(&local, &local_schema, &remote).unwrap();
        assert_ne!(local_schema.table_hash(), map.hash);
        assert_eq!(
            map.floats,
            vec![
                local.var("door").unwrap(),
                local.var("private_shared").unwrap(),
                u32::MAX
            ]
        );
        assert_eq!(
            map.strings,
            vec![
                local.str_var("display").unwrap(),
                local.str_var("private_string").unwrap(),
                u32::MAX
            ]
        );
        remote.model.push("f:remote_only".into());
        assert!(mapping(&local, &local_schema, &remote).is_none());
        remote = local_schema.clone();
        remote.sound.clear();
        assert!(mapping(&local, &local_schema, &remote).is_none());
    }

    #[test]
    fn model_and_sound_references_include_engine_strings_and_uncapped_values() {
        let mut program = Program::default();
        for name in [
            "door_0",
            "rain_window",
            "pitch",
            "condition",
            "volume",
            "cookie_light",
            "cookie_yaw",
            "cookie_pitch",
        ] {
            program.declare_var(name);
        }
        program.declare_str_var("display");
        let mut model = omsi_model::Model::default();
        model.meshes.push(omsi_model::MeshDef {
            visible: Some(("DOOR_0".into(), 1.0)),
            animations: vec![omsi_model::Animation {
                variable: "door_0".into(),
                ..Default::default()
            }],
            materials: vec![omsi_model::MaterialDef {
                alphascale: Some("rain_window".into()),
                freetex: Some(("x.bmp".into(), "display".into())),
                ..Default::default()
            }],
            ..Default::default()
        });
        model.spotlight_cookies.push(omsi_model::SpotlightCookie {
            variable: "COOKIE_LIGHT".into(),
            yaw: Some("cookie_yaw".into()),
            pitch: Some("cookie_pitch".into()),
            ..Default::default()
        });
        let mut refs = Vec::new();
        model_references(&mut refs, &program, &model);
        assert!(refs.contains(&"f:door_0".into()));
        assert!(refs.contains(&"f:rain_window".into()));
        assert!(refs.contains(&"s:display".into()));
        for name in ["cookie_light", "cookie_yaw", "cookie_pitch"] {
            assert!(refs.contains(&format!("f:{name}")));
        }
        reference(&mut refs, &program, "missing", false);
        assert!(refs.contains(&"u:missing".into()));
        let sound = omsi_vehicle::SoundCfg {
            sounds: vec![omsi_vehicle::SoundEntry {
                pitch_variable: "pitch".into(),
                vol_curves: vec![omsi_vehicle::VolCurve {
                    variable: "volume".into(),
                    ..Default::default()
                }],
                conditions: vec![omsi_vehicle::sound::Condition {
                    variable: "condition".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut refs = Vec::new();
        sound_references(&mut refs, &program, &sound);
        assert_eq!(refs, vec!["f:pitch", "f:volume", "f:condition"]);
    }
}
