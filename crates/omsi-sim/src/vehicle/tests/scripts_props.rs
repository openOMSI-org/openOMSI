//! Tests of the scripts' doors, the material property plan and the contact plane.

use super::*;

#[test]
fn script_speed_reports_tiny_resting_motion_as_stopped() {
    assert_eq!(script_speed(0.000251), 0.0);
    assert_eq!(script_speed(-0.000251), 0.0);
    assert_eq!(script_speed(0.02), 0.02);
}

#[test]
fn ai_scripts_receive_pedals_before_both_frame_variants() {
    for frame in ["frame", "frame_ai"] {
        let script = format!(
            "{{{frame}}}\n(L.L.Throttle) (S.L.engine_throttle_injection)\n(L.L.Brake) (S.L.brake_seen)\n(L.L.Clutch) (S.L.clutch_seen)\n{{end}}\n"
        );
        let mut v = crate::timetable_run::tests::script_test_vehicle(
            &script,
            "Throttle\nBrake\nClutch\nengine_throttle_injection\nbrake_seen\nclutch_seen\n",
            "",
        );
        for (throttle, brake, clutch) in [(0.75, 0.0, 0.0), (0.0, 0.6, 0.2), (0.0, 0.0, 0.0)] {
            v.set_controls(crate::Controls { throttle, brake, clutch, ..Default::default() });
            v.update_ai(0.02, &AiFrame { speed: 8.0, ..Default::default() });
            assert_eq!(v.var("engine_throttle_injection"), Some(throttle), "{frame}");
            assert_eq!(v.var("brake_seen"), Some(brake), "{frame}");
            assert_eq!(v.var("clutch_seen"), Some(clutch), "{frame}");
        }
        let inputs = [(v.ty.program.var("Throttle").unwrap(), 0.4)];
        let pinned = [(v.ty.program.var("engine_throttle_injection").unwrap(), 0.9)];
        v.update_ai_with(0.02, &AiFrame::default(), &inputs, &[]);
        assert_eq!(v.var("engine_throttle_injection"), Some(0.4));
        v.update_ai_with(0.02, &AiFrame::default(), &inputs, &pinned);
        assert_eq!(v.var("engine_throttle_injection"), Some(0.9));
    }
}

/// A shadow blob at the model's z = 0 is laid onto the plane through the wheels: 15 cm
/// up with the body sagging, and following a pitch; one axle gives a level plane.
#[test]
fn shadow_blob_lies_on_the_wheels_plane() {
    let wheels = [
        Vec3::new(-1.0, 3.2, 0.13),
        Vec3::new(1.0, 3.2, 0.13),
        Vec3::new(-1.0, -2.6, 0.17),
        Vec3::new(1.0, -2.6, 0.17),
    ];
    let p = fit_plane(&wheels);
    for w in wheels {
        assert!(
            (p[0] + p[1] * w.x + p[2] * w.y - w.z).abs() < 1e-4,
            "{p:?} at {w:?}"
        );
    }
    let m = onto_plane(p);
    for corner in [Vec3::new(1.4, 5.7, 0.0), Vec3::new(-1.4, -5.7, 0.0)] {
        let q = m.transform_point3(corner);
        assert!((q.truncate() - corner.truncate()).length() < 1e-5);
        assert!(
            (q.z - (p[0] + p[2] * corner.y) - SHADOW_LIFT).abs() < 1e-4,
            "{q:?}"
        );
    }
    let axle = fit_plane(&[Vec3::new(-1.0, -0.4, 0.2), Vec3::new(1.0, -0.4, 0.1)]);
    assert!(
        (axle[0] - 0.15).abs() < 1e-5 && axle[1] == 0.0 && axle[2] == 0.0,
        "{axle:?}"
    );

    // A rail vehicle sits level on the track and its shadow blob is not distorted by road ground.
    let rail_ty = coupling_test_type(Some(5.0));
    let rail_v = VehicleInstance::new(rail_ty.clone(), VehicleHost::new(Default::default()));
    assert_eq!(rail_v.contact_plane(), [0.0, 0.0, 0.0]);
    let rail_t = TrailerPart::new_ex(rail_ty.clone(), &rail_ty, false, false, &rail_ty.program, 0, 0);
    let m_local = rail_t.mesh_local_transform(0);
    assert!(!m_local.is_nan());
}

/// `Program::stores`/`reads` answer from bits made once; for the stock vehicles' real
/// scripts they say for every variable what a scan of the whole bytecode says.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn stores_and_reads_bits_match_the_scan_for_stock_scripts() {
    let root = original_root();
    for bus in [
        "Vehicles/MAN_NL_NG/MAN_GN92_main.bus",
        "Vehicles/MAN_NL_NG/MAN_EN92_main_KI.bus",
        "Vehicles/MAN_SD202/MAN_D92.bus",
        "Vehicles/MAN_SD200/MAN_SD80.bus",
        "Vehicles/VW_Golf_2/ai_vw_golf_2.bus",
    ] {
        let bus = root.join(bus);
        require_content(&[&bus]);
        let ty = VehicleType::load(&root, &bus).expect("vehicle");
        let p = &ty.program;
        assert!(p.var_names.len() > 10, "{}: no script", bus.display());
        let ops: Vec<&omsi_script::Op> = p.blocks.iter().flat_map(|b| &b.ops).collect();
        let (mut stored, mut read) = (0, 0);
        for v in 0..p.var_names.len() as omsi_script::VarId + 64 {
            let s = ops.iter().any(|op| matches!(op, omsi_script::Op::Store(x) if *x == v));
            let r = ops.iter().any(|op| matches!(op, omsi_script::Op::Load(x) if *x == v));
            assert_eq!(p.stores(v), s, "{}: stores {v}", bus.display());
            assert_eq!(p.reads(v), r, "{}: reads {v}", bus.display());
            stored += s as usize;
            read += r as usize;
        }
        assert!(stored > 0 && read > 0, "{}", bus.display());
    }
}

/// Volvo Wright's dashboard rear-close trigger falls back to its explicit external-close
/// path when the handbrake guard only produced the button sound.
#[test]
#[ignore = "needs OMSI_ROOT with Volvo_Wright_Family (not part of the stock install)"]
fn volvo_wright_rear_close_fallback_moves_a_partly_open_door() {
    let root = original_root();
    let bus = root.join("Vehicles/Volvo_Wright_Family/AVBWS1.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("AVBWS1"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    for (name, value) in [
        ("elec_real_main", 1.0),
        ("elec_busbar_main", 1.0),
        ("elec_busbar_avail", 1.0),
        ("cockpit_button_ignition", 1.0),
        ("bremse_feststell_sw", 0.0),
        ("cockpit_button_smallhb", 0.0),
        ("door_2", 0.5),
        ("door_3", 0.5),
        ("doorTarget_23", 1.0),
    ] {
        assert!(v.set_var(name, value), "missing {name}");
    }
    assert!(v.trigger("bus_dooraftclose"));
    assert_eq!(v.var("doorTarget_23"), Some(0.0));
}

#[test]
#[ignore = "needs OMSI_ROOT with Volvo_Wright_Family (not part of the stock install)"]
fn volvo_wright_rear_toggle_falls_back_to_external_close() {
    let root = original_root();
    let bus = root.join("Vehicles/Volvo_Wright_Family/AVBWS1.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("AVBWS1"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    for (name, value) in [
        ("elec_real_main", 1.0),
        ("elec_busbar_main", 1.0),
        ("elec_busbar_avail", 1.0),
        ("cockpit_button_ignition", 1.0),
        ("bremse_feststell_sw", 0.0),
        ("cockpit_button_smallhb", 0.0),
        ("door_2", 0.5),
        ("door_3", 0.5),
        ("doorTarget_23", 1.0),
    ] {
        assert!(v.set_var(name, value), "missing {name}");
    }
    assert!(v.trigger("bus_dooraft"));
    assert_eq!(v.var("doorTarget_23"), Some(0.0));
    assert_eq!(v.var("bdoor_embtn_cls"), Some(1.0));
}

#[test]
#[ignore = "needs OMSI_ROOT with Volvo_Wright_Family (not part of the stock install)"]
fn volvo_wright_rear_toggle_closes_open_leaves_even_with_zero_target() {
    let root = original_root();
    let bus = root.join("Vehicles/Volvo_Wright_Family/AVBWS1.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("AVBWS1"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    for (name, value) in [
        ("elec_real_main", 1.0),
        ("elec_busbar_main", 1.0),
        ("elec_busbar_avail", 1.0),
        ("cockpit_button_ignition", 1.0),
        ("bremse_feststell_sw", 0.0),
        ("cockpit_button_smallhb", 0.0),
        ("door_2", 0.5),
        ("door_3", 0.5),
        ("doorTarget_23", 0.0),
    ] {
        assert!(v.set_var(name, value), "missing {name}");
    }
    assert!(v.trigger("bus_dooraft"));
    assert_eq!(v.var("doorTarget_23"), Some(0.0));
}

#[test]
#[ignore = "needs OMSI_ROOT with Volvo_Wright_Family (not part of the stock install)"]
fn volvo_wright_rear_toggle_reopens_during_forced_close() {
    let root = original_root();
    let bus = root.join("Vehicles/Volvo_Wright_Family/AVBWS1.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("AVBWS1"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    for (name, value) in [
        ("elec_real_main", 1.0),
        ("elec_busbar_main", 1.0),
        ("elec_busbar_avail", 1.0),
        ("cockpit_button_ignition", 1.0),
        ("bremse_feststell_sw", 1.0),
        ("cockpit_button_smallhb", 0.0),
        ("door_2", 0.5),
        ("door_3", 0.5),
        ("doorTarget_23", 0.0),
        ("bdoor_embtn_cls", 1.0),
    ] {
        assert!(v.set_var(name, value), "missing {name}");
    }
    assert!(v.trigger("bus_dooraft"));
    assert_eq!(v.var("doorTarget_23"), Some(1.0));
    assert_eq!(v.var("bdoor_embtn_cls"), Some(0.0));
}

#[test]
#[ignore = "needs OMSI_ROOT with Volvo_Wright_Family (not part of the stock install)"]
fn volvo_wright_rear_toggle_keeps_close_target_through_frames() {
    let root = original_root();
    let bus = root.join("Vehicles/Volvo_Wright_Family/AVBWS1.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("AVBWS1"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    for (name, value) in [
        ("elec_real_main", 1.0),
        ("elec_busbar_main", 1.0),
        ("elec_busbar_avail", 1.0),
        ("cockpit_button_ignition", 1.0),
        ("bremse_feststell_sw", 0.0),
        ("cockpit_button_smallhb", 0.0),
        ("bremse_p_Tank04", 800000.0),
        ("door_2", 1.0),
        ("door_3", 1.0),
        ("doorTarget_23", 1.0),
        ("bdoor_sound_played", 1.0),
    ] {
        assert!(v.set_var(name, value), "missing {name}");
    }
    assert!(v.trigger("bus_dooraft"));
    v.update(1.0 / 30.0);
    assert_eq!(v.var("backdoor_buzzer"), Some(1.0));
    for _ in 1..120 {
        v.update(1.0 / 30.0);
    }
    assert_eq!(v.var("doorTarget_23"), Some(0.0));
    assert!(v.var("door_2").unwrap_or(1.0) < 0.1);
    assert!(v.var("door_3").unwrap_or(1.0) < 0.1);
    assert!(v.host.fired_triggers.iter().any(|name| name == "ev_doortriggerclose_2"));
}

#[test]
#[ignore = "needs OMSI_ROOT with Volvo_Wright_Family (not part of the stock install)"]
fn volvo_wright_family_rear_toggle_closes_every_bus_variant() {
    let root = original_root();
    let dir = root.join("Vehicles/Volvo_Wright_Family");
    require_content(&[&dir]);
    let mut buses: Vec<_> = std::fs::read_dir(&dir)
        .expect("Volvo Wright directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bus")))
        .collect();
    buses.sort();
    assert!(buses.len() >= 20, "unexpectedly few Volvo Wright buses: {}", buses.len());
    for bus in buses {
        let name = bus.file_name().unwrap().to_string_lossy().into_owned();
        let ty = Arc::new(VehicleType::load(&root, &bus).unwrap_or_else(|e| panic!("{name}: {e}")));
        let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
        for (var, value) in [
            ("elec_real_main", 1.0),
            ("elec_busbar_main", 1.0),
            ("elec_busbar_avail", 1.0),
            ("cockpit_button_ignition", 1.0),
            ("bremse_feststell_sw", 0.0),
            ("cockpit_button_smallhb", 0.0),
            ("door_2", 0.5),
            ("door_3", 0.5),
            ("doorTarget_23", 1.0),
        ] {
            if v.var(var).is_some() {
                assert!(v.set_var(var, value), "{name}: missing {var}");
            }
        }
        assert!(v.trigger("bus_dooraft"), "{name}: missing bus_dooraft");
        let target = v.var("doorTarget_23").unwrap_or(0.0);
        let request = v.var("door_back_close_request").unwrap_or(0.0);
        assert!(target == 0.0 || request > 0.0, "{name}: rear door stayed open (target={target}, request={request})");
    }
}

/// The wheel of a body without a rigid body stands on the road the drawn faces put
/// under it, not on the deck of a bridge over that road (or a canopy above it): the
/// plain sampler knows only x and y and gives the highest face there, which laid the
/// `[isshadow]` blob up on the deck. Where the faces put nothing near the wheel - and
/// where there is no face probe at all - the plain sampler still answers.
#[test]
fn a_wheels_ground_is_the_road_under_it_not_the_highest_face() {
    let (road, deck) = (12.0f64, 17.0f64);
    // the faces: the deck above the wheel, the road under it
    let faces = |_x: f64, _y: f64, top: f64| crate::rigid::GroundProbe {
        below: [road, deck].into_iter().filter(|h| *h <= top).fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b)))),
        above: None,
    };
    let faces: &dyn crate::rigid::Ground = &faces;
    // the plain sampler: the highest face at (x, y), which is the deck
    let plain = |_x: f64, _y: f64| Some(deck);
    let plain: &(dyn Fn(f64, f64) -> Option<f64> + Send + Sync) = &plain;
    let wheel = DVec3::new(100.0, 200.0, road);
    assert_eq!(wheel_ground(Some(faces), Some(plain), wheel), Some(road));
    // a wheel standing on the deck itself gets the deck
    assert_eq!(wheel_ground(Some(faces), Some(plain), DVec3::new(100.0, 200.0, deck)), Some(deck));
    // nothing drawn within a step of the wheel: the plain sampler, as before
    let empty = |_x: f64, _y: f64, _top: f64| crate::rigid::GroundProbe { below: None, above: Some(deck) };
    let empty: &dyn crate::rigid::Ground = &empty;
    assert_eq!(wheel_ground(Some(empty), Some(plain), wheel), Some(deck));
    // no face probe at all (a rail or air lane): the plain sampler
    assert_eq!(wheel_ground(None, Some(plain), wheel), Some(deck));
}

/// The resolved property plan gives what `compute_mesh_props` gives, for a stock bus
/// with its variables set to changing values, and after an engine variable joins.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn props_plan_matches_compute_mesh_props() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_EN92_main.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("EN92"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    let compare = |v: &VehicleInstance| {
        let mut plan = PropsPlan::default();
        plan.refresh(&v.ty, &v.var_index);
        let mut got = vec![MeshProps::default(); 3];
        plan.apply(&v.state.vars, &mut got);
        let want = compute_mesh_props(&v.ty, &|n| v.var(n));
        assert_eq!(got.len(), want.len());
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(
                (
                    g.visible,
                    &g.slot_alpha,
                    &g.slot_light,
                    &g.slot_night,
                    &g.slot_uv,
                    g.interior
                ),
                (
                    w.visible,
                    &w.slot_alpha,
                    &w.slot_light,
                    &w.slot_night,
                    &w.slot_uv,
                    w.interior
                ),
                "mesh {i} {}",
                v.ty.model.meshes[v.ty.meshes[i].def_index].file
            );
        }
    };
    for round in 0..4 {
        for (k, x) in v.state.vars.iter_mut().enumerate() {
            *x = ((k * 7 + round * 13) % 5) as f32 * 0.37 - 0.2;
        }
        compare(&v);
    }
    // Dirt_Norm is not declared by the scripts; the engine adds it later
    v.set_engine_var("Dirt_Norm", 0.63);
    compare(&v);
}

/// The plan's dirt overlays and light sources are what the names say, as the frame
/// looked them up before: every material by its `[alphascale]`, every lamp by number or
/// variable.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn props_plan_dirt_and_lights_match_the_names() {
    let root = original_root();
    for bus in ["Vehicles/MAN_NL_NG/MAN_EN92_main.bus", "Vehicles/MAN_SD202/MAN_D92.bus", "Vehicles/VW_Golf_2/ai_vw_golf_2.bus"] {
        let bus = root.join(bus);
        require_content(&[&bus]);
        let ty = Arc::new(VehicleType::load(&root, &bus).expect("bus"));
        let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
        for (k, x) in v.state.vars.iter_mut().enumerate() {
            *x = (k % 7) as f32 * 0.3;
        }
        let mut dirt = Vec::new();
        for (i, vm) in v.ty.meshes.iter().enumerate() {
            for m in &v.ty.model.meshes[vm.def_index].materials {
                let Some(name) = m.alphascale.as_deref().map(str::trim) else { continue };
                let wiped = match name.to_ascii_lowercase().as_str() {
                    "dirt_norm" => false,
                    "dirt_wiped" => true,
                    _ => continue,
                };
                if let Some(slot) = override_slot(&vm.materials, m) {
                    dirt.push((i, slot, wiped));
                }
            }
        }
        let lights: Vec<f32> = v
            .ty
            .model
            .meshes
            .iter()
            .flat_map(|md| &md.light_enh_2)
            .map(|l| l.variable.trim().parse::<f32>().ok().or_else(|| v.var(l.variable.trim())).unwrap_or(0.0))
            .collect();
        let mut plan = PropsPlan::default();
        plan.refresh(&v.ty, &v.var_index);
        assert_eq!(plan.dirt, dirt, "{}", bus.display());
        let planned: Vec<f32> = plan.lights.iter().map(|s| s.value(&v.state.vars, 0.0)).collect();
        assert_eq!(planned, lights, "{}", bus.display());
        assert_eq!(plan.dirt_wiped.map(|i| v.state.vars[i]), v.var("Dirt_Wiped"));
        assert!(!lights.is_empty(), "{}: no lamps", bus.display());
    }
}

#[test]
fn synthetic_material_plan_preserves_lightmap_thresholds_and_alpha_slots() {
    let mut ty = coupling_test_type(None);
    let ty = Arc::get_mut(&mut ty).unwrap();
    ty.model = Model::parse(&omsi_cfg::CfgFile::from_str("synthetic.cfg",
        include_str!("../../../../omsi-model/tests/fixtures/material-compatibility.cfg")));
    ty.meshes.push(VehicleMesh {
        def_index: 0, data: MeshData::default(), file: PathBuf::new(),
        materials: (0..2).map(|_| omsi_o3d::Material {
            texture: "display.dds".into(), ..Default::default()
        }).collect(),
        overrides: Vec::new(), pivot: Mat4::IDENTITY, viewpoint: 0,
        skin: Vec::new(), keep_winding: false,
    });
    let mut index: HashMap<String, omsi_script::VarId> = ["display_alpha", "bright", "dim", "display_mode"]
        .into_iter().enumerate().map(|(i, n)| (n.into(), i as omsi_script::VarId)).collect();
    let mut plan = PropsPlan::default();
    plan.refresh(ty, &index);
    let mut got = Vec::new();
    // Both lightmaps off, either on, both on, and the exact OMSI 0.5 threshold.
    for (bright, dim, expected) in [(0.0, 0.0, 0.0), (1.0, 0.0, 1.0),
        (0.0, 1.0, 1.0), (1.0, 1.0, 1.0), (0.49, 0.49, 0.0), (0.5, 0.0, 1.0)] {
        for alpha in [0.0, 0.5, 1.0] {
            for mode in [0.0, 1.0] {
                let vars = [alpha, bright, dim, mode];
                plan.apply(&vars, &mut got);
                let want = compute_mesh_props(ty, &|name| index.get(name).map(|&i| vars[i as usize]));
                assert_eq!(got[0].slot_light, vec![expected, 1.0]);
                assert_eq!(got[0].slot_alpha, vec![alpha, 1.0]);
                assert_eq!(got[0].slot_light, want[0].slot_light);
                assert_eq!(got[0].slot_alpha, want[0].slot_alpha);
            }
        }
    }
    // An undeclared lightmap variable stays on; rebuilding the existing plan
    // must not keep the obsolete resolved variable index.
    index.remove("dim");
    plan.refresh(ty, &index);
    plan.apply(&[0.5, 0.0, 0.0, 0.0], &mut got);
    assert_eq!(got[0].slot_light, vec![1.0, 1.0]);
}

#[test]
fn a_number_in_alphascale_is_the_scale_itself() {
    let cfg = "[mesh]\nwindows.o3d\n\
        [matl]\nfilm.png\n0\n[matl_alpha]\n2\n[alphascale]\n0.9\n\
        [matl]\nfrost.tga\n0\n[matl_alpha]\n2\n[alphascale]\n0.2\n\
        [matl]\npane.tga\n0\n[matl_alpha]\n2\n[alphascale]\nno_such_var\n\
        [matl]\ntint.tga\n0\n[matl_alpha]\n2\n[alphascale]\nenvir_brightness\n";
    let mut ty = coupling_test_type(None);
    let ty = Arc::get_mut(&mut ty).unwrap();
    ty.model = Model::parse(&omsi_cfg::CfgFile::from_str("glass.cfg", cfg));
    ty.meshes.push(VehicleMesh {
        def_index: 0,
        data: MeshData::default(),
        file: PathBuf::new(),
        materials: ["film.png", "frost.tga", "pane.tga", "tint.tga"]
            .into_iter()
            .map(|texture| omsi_o3d::Material {
                texture: texture.into(),
                ..Default::default()
            })
            .collect(),
        overrides: Vec::new(),
        pivot: Mat4::IDENTITY,
        viewpoint: 0,
        skin: Vec::new(),
        keep_winding: false,
    });
    let index: HashMap<String, omsi_script::VarId> =
        [("envir_brightness".to_string(), 0)].into_iter().collect();
    let mut plan = PropsPlan::default();
    plan.refresh(ty, &index);
    let mut got = Vec::new();
    for light in [0.0, 0.4, 1.0] {
        let vars = [light];
        plan.apply(&vars, &mut got);
        assert_eq!(
            got[0].slot_alpha,
            vec![0.9, 0.2, 0.0, light],
            "brightness {light}"
        );
        let want = compute_mesh_props(ty, &|name| index.get(name).map(|&i| vars[i as usize]));
        assert_eq!(
            got[0].slot_alpha, want[0].slot_alpha,
            "the plan and the direct way agree"
        );
    }
}

/// A `[matl_lightmap]` whose variable the bus does not have is always on (Omsi.exe's
/// index -1, 0x7fe4e7); one on a variable at 0.3 is off.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn a_lightmap_on_an_unknown_variable_is_on() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_EN92_main.bus");
    require_content(&[&bus]);
    let light = |name: Option<&str>, value: f32| {
        let mut ty = VehicleType::load(&root, &bus).expect("EN92");
        let (i, slot, var) = ty
            .meshes
            .iter()
            .enumerate()
            .find_map(|(i, vm)| {
                ty.model.meshes[vm.def_index].materials.iter().find_map(|m| {
                    let (_, v) = m.lightmaps.first()?;
                    Some((i, override_slot(&vm.materials, m)?, v.clone()))
                })
            })
            .expect("a lightmap");
        let di = ty.meshes[i].def_index;
        for m in ty.model.meshes[di].materials.iter_mut() {
            if let Some(l) = m.lightmaps.first_mut() {
                l.1 = name.unwrap_or(&var).to_string();
                m.lightmaps.truncate(1);
            }
        }
        let mut v = VehicleInstance::new(Arc::new(ty), VehicleHost::new(crate::SimClock::default()));
        for x in v.state.vars.iter_mut() {
            *x = value;
        }
        let mut plan = PropsPlan::default();
        plan.refresh(&v.ty, &v.var_index);
        let mut got = vec![MeshProps::default(); 3];
        plan.apply(&v.state.vars, &mut got);
        let want = compute_mesh_props(&v.ty, &|n| v.var(n));
        assert_eq!(got[i].slot_light[slot], want[i].slot_light[slot]);
        got[i].slot_light[slot]
    };
    assert_eq!(light(Some("no_such_var"), 0.3), 1.0);
    assert_eq!(light(None, 0.3), 0.0);
}

#[test]
fn a_control_cable_hands_what_the_scripts_write_to_what_they_read() {
    let base = crate::timetable_run::tests::script_test_vehicle(
        "{frame}\n0.6 (S.L.CCW_Throttle)\n(L.L.CCR_Throttle) (S.L.seen)\n{end}\n",
        "CCW_Throttle\nCCR_Throttle\nseen\n",
        "",
    );
    let cable = omsi_vehicle::vehicle::ControlCable { lines: ["C", "2", "CCR_Throttle", "CCW_Throttle", "E-Kupplung_Front"].map(String::from).to_vec() };
    let mut ty = (*base.ty).clone();
    ty.def.control_cable_front.push(cable.clone());
    ty.def.control_cable_back.push(cable);
    for (cabled, expected) in [(false, 0.0), (true, 0.6)] {
        let ty = if cabled { Arc::new(ty.clone()) } else { base.ty.clone() };
        let mut v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        v.update(1.0 / 30.0);
        assert_eq!(v.var("seen"), Some(0.0));
        v.update(1.0 / 30.0);
        assert_eq!(v.var("seen"), Some(expected), "cabled {cabled}");
    }
}

#[test]
fn road_vehicle_variables_are_known_without_the_program_folder_and_in_any_case() {
    let has = |v: &[String], n: &str| v.iter().any(|x| x == n);
    let dir = std::env::temp_dir().join(format!("omsi_builtin_vars_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bare")).unwrap();
    std::fs::create_dir_all(dir.join("upper/Program")).unwrap();
    std::fs::write(dir.join("upper/Program/VarList_RoadVehicle.txt"), "Throttle\r\nOwnVar\r\n").unwrap();
    std::fs::write(dir.join("t.osc"), "{frame}\n(L.L.Throttle) (S.L.Velocity)\n(L.L.cockpit_light_sw) (S.L.Spot_Select)\n{end}\n").unwrap();
    let read = builtin_vars(&dir.join("upper"));
    assert!(has(&read, "OwnVar") && has_program_varlist(&dir.join("upper")));
    assert!(!has_program_varlist(&dir.join("bare")));
    let compile = |declare_unknown| omsi_script::compile(&omsi_script::CompileInput { builtin_vars: builtin_vars(&dir.join("bare")), scripts: vec![dir.join("t.osc")], declare_unknown, ..Default::default() });
    let p = compile(true);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert!(["throttle", "velocity", "spot_select", "cockpit_light_sw"].iter().all(|n| p.var(n).is_some()));
    assert!(compile(false).errors.iter().any(|e| e.message.contains("cockpit_light_sw")));
    let _ = std::fs::remove_dir_all(&dir);
}
