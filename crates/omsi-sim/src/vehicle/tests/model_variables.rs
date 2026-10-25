use super::*;

fn diagnostic_vehicle(reverse: bool) -> VehicleInstance {
    VehicleInstance::new(
        Arc::new(diagnostic_type(reverse)),
        VehicleHost::new(Default::default()),
    )
}

fn diagnostic_type(reverse: bool) -> VehicleType {
    let mut declared = vec!["unwritten", "selection", "initialised"];
    if reverse {
        declared.reverse();
    }
    let seed = crate::timetable_run::tests::script_test_vehicle(
        "{init}\n1 (S.L.selection) (S.L.initialised)\n{end}\n",
        &declared.join("\n"),
        "",
    );
    let mut blocks = vec!["[mesh]\nfixed.o3d\n".to_string()];
    for value in 0..=2 {
        for (id, variable) in [
            ("unique", format!("missing_{value}")),
            ("shared", "missing_shared".to_string()),
            ("declared", "unwritten".to_string()),
        ] {
            blocks.push(format!(
                "[mesh]\n{id}{value}.o3d\n[visible]\n{variable}\n{value}\n"
            ));
        }
        let material = |id: &str, visible: &str, variable: &str| {
            format!(
            "[mesh]\n{id}{value}.o3d\n{visible}[matl_change]\nlamp.png\n0\n{variable}\n[matl_item]\n[matl_nightmap]\nlamp_on.png\n"
        )
        };
        let variable = format!("missing_material_{value}");
        blocks.push(material(
            "combined",
            &format!("[visible]\n{variable}\n{value}\n"),
            &variable,
        ));
        blocks.push(material("monitor", "", &variable));
        blocks.push(material(
            "declared_material",
            &format!("[visible]\nunwritten\n{value}\n"),
            "unwritten",
        ));
    }
    blocks.push("[mesh]\nmaterial_only.o3d\n[matl_change]\nlamp.png\n0\nmissing_material_only\n[matl_item]\n[matl_nightmap]\nlamp_on.png\n".to_string());
    blocks.push("[mesh]\ninitialised.o3d\n[visible]\ninitialised\n1\n[matl_change]\nlamp.png\n0\ninitialised\n[matl_item]\n[matl_nightmap]\nlamp_on.png\n".to_string());
    for value in [1, 4, 0] {
        blocks.push(format!(
            "[mesh]\nselector{value}.o3d\n[visible]\nselection\n{value}\n"
        ));
    }
    if reverse {
        blocks.reverse();
    }
    let mut ty = (*seed.ty).clone();
    ty.model = Model::parse(&omsi_cfg::CfgFile::from_str(
        "diagnostic.cfg",
        &blocks.join("\n"),
    ));
    ty.meshes = (0..ty.model.meshes.len())
        .map(|def_index| VehicleMesh {
            def_index,
            data: MeshData::default(),
            file: PathBuf::new(),
            materials: vec![omsi_o3d::Material {
                texture: "lamp.png".into(),
                ..Default::default()
            }],
            overrides: ty.model.meshes[def_index].materials.clone(),
            pivot: Mat4::IDENTITY,
            viewpoint: 0,
            skin: Vec::new(),
            keep_winding: false,
        })
        .collect();
    ty
}

/// Original OMSI 2 runtime diagnostic: distinct and shared missing names ignore
/// comparisons 0/1/2; declared, unwritten names compare zero. Reversing model and
/// varlist order does not change those results. No original vehicle assets needed.
#[test]
fn missing_visibility_names_are_distinct_from_declared_zero() {
    for reverse in [false, true] {
        let mut v = diagnostic_vehicle(reverse);
        v.update_visuals(0.0);
        assert_eq!(v.var("unwritten"), Some(0.0));
        assert_eq!(v.var("initialised"), Some(1.0));
        for name in ["missing_0", "missing_1", "missing_2", "missing_shared"] {
            assert_eq!(v.var(name), None, "model must not register {name}");
        }
        for (i, mesh) in v.ty.model.meshes.iter().enumerate() {
            let expected = if mesh.file.starts_with("declared") {
                mesh.file.ends_with("0.o3d")
            } else if mesh.file.starts_with("selector") {
                mesh.file == "selector1.o3d"
            } else {
                true
            };
            assert_eq!(v.mesh_props[i].visible, expected, "{}", mesh.file);
        }
        let reference = compute_mesh_props(&v.ty, &|name| v.var(name));
        for (actual, expected) in v.mesh_props.iter().zip(reference) {
            assert_eq!(actual.visible, expected.visible);
            assert_eq!(actual.slot_night, expected.slot_night);
        }
    }
}

#[test]
fn a_missing_visibility_name_does_not_activate_its_material_variant() {
    let mut v = diagnostic_vehicle(false);
    v.update_visuals(0.0);
    for (i, mesh) in v.ty.model.meshes.iter().enumerate() {
        if mesh.file.starts_with("combined")
            || mesh.file.starts_with("monitor")
            || mesh.file == "material_only.o3d"
        {
            assert!(v.mesh_props[i].visible, "{}", mesh.file);
            assert_eq!(v.mesh_props[i].slot_night, [0.0], "{}", mesh.file);
            let variable = &mesh.materials[0].change.as_ref().unwrap().2;
            assert_eq!(v.var(variable), None);
        }
        if mesh.file == "initialised.o3d" {
            assert!(v.mesh_props[i].visible);
            assert_eq!(v.mesh_props[i].slot_night, [1.0]);
        }
    }
}

#[test]
fn scripted_selector_transitions_leave_missing_materials_at_base() {
    let mut v = diagnostic_vehicle(false);
    for selection in [1.0, 4.0, 1.0, 0.0, 1.0, 4.0, 1.0, 0.0] {
        assert!(v.set_var("SELECTION", selection));
        for _ in 0..3 {
            v.update_visuals(1.0 / 30.0);
            let active: Vec<_> =
                v.ty.model
                    .meshes
                    .iter()
                    .enumerate()
                    .filter(|(i, m)| m.file.starts_with("selector") && v.mesh_props[*i].visible)
                    .map(|(_, m)| m.file.as_str())
                    .collect();
            assert_eq!(active, [format!("selector{selection}.o3d")]);
            for (i, mesh) in v.ty.model.meshes.iter().enumerate() {
                if mesh.file.starts_with("combined") {
                    assert!(v.mesh_props[i].visible);
                    assert_eq!(v.mesh_props[i].slot_night, [0.0]);
                }
            }
        }
    }
}

#[test]
fn an_engine_variable_registered_later_rebinds_visibility_and_materials() {
    let mut v = diagnostic_vehicle(false);
    v.update_visuals(0.0);
    v.set_engine_var("MISSING_MATERIAL_1", 0.0);
    v.update_visuals(0.0);
    let combined =
        v.ty.model
            .meshes
            .iter()
            .position(|m| m.file == "combined1.o3d")
            .unwrap();
    let monitor =
        v.ty.model
            .meshes
            .iter()
            .position(|m| m.file == "monitor1.o3d")
            .unwrap();
    assert!(!v.mesh_props[combined].visible);
    assert!(v.mesh_props[monitor].visible);
    assert_eq!(v.mesh_props[monitor].slot_night, [0.0]);
    assert!(v.set_var("missing_material_1", 1.0));
    v.update_visuals(0.0);
    assert!(v.mesh_props[combined].visible);
    assert_eq!(v.mesh_props[monitor].slot_night, [1.0]);
}

#[test]
fn a_setvar_on_a_model_only_name_switches_its_meshes() {
    let shown = |v: &VehicleInstance| -> Vec<String> {
        v.ty.model
            .meshes
            .iter()
            .enumerate()
            .filter(|(i, m)| m.file.starts_with("shared") && v.mesh_props[*i].visible)
            .map(|(_, m)| m.file.clone())
            .collect()
    };
    let mut v = diagnostic_vehicle(false);
    assert!(v.set_var("MISSING_SHARED", 2.0));
    v.update_visuals(0.0);
    assert_eq!(shown(&v), ["shared2.o3d"]);
    assert!(!v.set_var("not_in_the_model", 1.0));

    let mut ty = diagnostic_type(false);
    ty.paint_schemes = vec![PaintScheme {
        set_vars: vec![("Missing_Shared".into(), 1.0)],
        ..Default::default()
    }];
    let mut host = VehicleHost::new(Default::default());
    host.paint_scheme = Some(Some(0));
    let mut v = VehicleInstance::new(Arc::new(ty), host);
    v.update_visuals(0.0);
    assert_eq!(shown(&v), ["shared1.o3d"]);
}

/// A scheme's `[setvar]` wins over a script that writes the same variable every frame: the
/// Kajosoft Jelcz's `cockpit_frame` sets `v_drzwi` from its constant (0) and its repaints
/// choose the glazed doors with `[setvar] v_drzwi 1`, which Omsi.exe draws (#3183). The
/// scripts still see the scheme's value when they start, as before.
#[test]
fn a_schemes_setvar_is_what_is_drawn_after_the_scripts() {
    let dir = std::env::temp_dir().join(format!("omsi_scheme_after_scripts_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("cockpit.osc");
    std::fs::write(&script, "{frame}\n(L.L.v_drzwi) (S.L.seen)\n0 (S.L.v_drzwi)\n{end}\n").unwrap();
    let vars = dir.join("vars.txt");
    std::fs::write(&vars, "v_drzwi\nseen\n").unwrap();
    let program = omsi_script::compile(&omsi_script::CompileInput { scripts: vec![script], varlists: vec![vars], ..Default::default() });
    std::fs::remove_dir_all(&dir).ok();
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    let mut ty = diagnostic_type(false);
    ty.program = Arc::new(program);
    ty.paint_schemes = vec![PaintScheme { set_vars: vec![("v_drzwi".into(), 1.0)], ..Default::default() }];
    let mut host = VehicleHost::new(Default::default());
    host.paint_scheme = Some(Some(0));
    let mut v = VehicleInstance::new(Arc::new(ty), host);
    v.update(1.0 / 30.0);
    assert_eq!(v.var("v_drzwi"), Some(1.0), "drawn: the scheme's value");
    assert_eq!(v.var("seen"), Some(1.0), "the scripts started with the scheme's value");
    // a switch pressed: the scripts run alone, and the scheme's value is kept again
    v.update_scripts_only(0.0);
    assert_eq!(v.var("v_drzwi"), Some(1.0));
}
