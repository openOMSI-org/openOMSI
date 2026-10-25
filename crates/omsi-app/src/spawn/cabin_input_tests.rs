use super::*;

fn player() -> Player {
    // A bounded control, like an authored door with physical end stops.
    let mut vehicle = crate::schedule::tests::script_test_vehicle(
        "{trigger:knob_drag}\n(L.L.value) (L.S.mouse_y) 100 / + 0 max 1 min (S.L.value)\n{end}\n\
         {trigger:knob_off}\n(L.L.releases) 1 + (S.L.releases)\n{end}\n",
        "value\nreleases\n",
        "",
    );
    let ty = Arc::make_mut(&mut vehicle.ty);
    ty.model.meshes.push(omsi_model::MeshDef {
        mouse_event: Some("knob".into()),
        animations: vec![omsi_model::Animation {
            variable: "value".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    ty.meshes.push(omsi_sim::vehicle::VehicleMesh {
        def_index: 0,
        data: Default::default(),
        file: Default::default(),
        materials: vec![],
        overrides: vec![],
        pivot: glam::Mat4::IDENTITY,
        viewpoint: 0,
        skin: vec![],
        keep_winding: false,
    });
    vehicle.mesh_props.resize_with(1, Default::default);
    vehicle.set_var("value", 0.4);
    let ty = vehicle.ty.clone();
    let render = scene::VehicleRender {
        window_wipers: None,
        instances: vec![],
        far: vec![],
        own_materials: vec![],
        set: None,
        variants: vec![],
        text_textures: vec![],
        script_textures: vec![],
        shared_script: false,
        displays_far: false,
        display_tick: 0,
        skinned: vec![],
        damaged: vec![],
        glass_cracks: vec![],
        hidden: false,
        interior_lamps: Default::default(),
        interior_blocks: Default::default(),
    };
    let mut player = new_player(vehicle, render, vec![], vec![], false, &ty);
    player.pressed_mesh = Some(0);
    player.press_info = (false, 0.0);
    player
}

#[test]
fn precise_release_retains_short_drag_and_fires_button_off_once() {
    let mut player = player();
    player.drag(0.0, 0.5);
    player.release_precise();
    assert!((player.vehicle.var("value").unwrap() - 0.405).abs() < 1e-6);
    assert_eq!(player.vehicle.var("releases"), Some(1.0));
    assert!(player.auto_drag.is_none());
    assert!(player.pressed_mesh.is_none());
    player.release_precise();
    assert_eq!(player.vehicle.var("releases"), Some(1.0));
}

#[test]
fn desktop_click_keeps_endpoint_fallback_and_precise_cancel_stops_it() {
    let mut player = player();
    player.release();
    assert!(player.auto_drag.is_some());
    player.vehicle.host.mouse = (0.0, 5.0);
    player.release_precise();
    assert!(player.auto_drag.is_none());
    assert_eq!(player.vehicle.host.mouse, (0.0, 0.0));
    assert_eq!(player.vehicle.var("releases"), Some(2.0));
}

#[test]
fn desktop_escape_finishes_pending_drag_and_keeps_endpoint_fallback() {
    use clap::Parser;
    let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
    app.player = Some(player());
    app.input.dragging = true;
    app.input.drag_delta = (0.0, 0.5);
    app.input.buttons_held = (true, false);
    app.open_game_menu();
    let player = app.player.as_ref().unwrap();
    assert!(player.pressed_mesh.is_none());
    assert!(player.auto_drag.is_some());
    assert!((player.vehicle.var("value").unwrap() - 0.405).abs() < 1e-6);
    assert_eq!(player.vehicle.var("releases"), Some(1.0));
    assert!(!app.input.dragging);
    assert_eq!(app.input.drag_delta, (0.0, 0.0));
    assert_eq!(app.input.buttons_held, (false, false));
    app.close_game_menu();
    assert!(!app.input.dragging);
}

fn automatically_moving_control() -> Player {
    let mut p = player();
    p.release();
    assert!(p.auto_drag.is_some());
    p
}

#[test]
fn desktop_escape_preserves_an_auto_drag_until_it_finishes() {
    use clap::Parser;
    let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
    app.player = Some(automatically_moving_control());
    app.open_game_menu();
    assert!(app.player.as_ref().unwrap().auto_drag.is_some());
    app.close_game_menu();
    let p = app.player.as_mut().unwrap();
    for _ in 0..250 {
        p.tick_auto_drag(0.01);
    }
    assert!(p.auto_drag.is_none());
    assert!(p.vehicle.var("value").unwrap() > 0.95);
}

#[test]
fn a_missed_desktop_click_keeps_auto_drag_but_a_vr_click_stops_it() {
    let mut p = automatically_moving_control();
    let origin = p.vehicle.position;
    assert!(p.click(origin, Vec3::Z, 0.0).is_none());
    assert!(p.auto_drag.is_some());
    assert!(p.click_precise(origin, Vec3::Z, 0.0).is_none());
    assert!(p.auto_drag.is_none());
    assert_eq!(p.vehicle.var("releases"), Some(2.0));
}

#[test]
fn wheel_over_another_switch_preserves_desktop_auto_drag_and_cancels_it_in_vr() {
    let mut p = automatically_moving_control();
    let ty = Arc::make_mut(&mut p.vehicle.ty);
    ty.model.meshes[0].mouse_event = Some("other_switch".into());
    ty.meshes[0].data.positions = vec![
        Vec3::new(-0.5, 1.0, -0.5),
        Vec3::new(0.5, 1.0, -0.5),
        Vec3::new(0.0, 1.0, 0.5),
    ];
    ty.meshes[0].data.indices = vec![0, 1, 2];
    ty.meshes[0].data.ranges = vec![(0, 3, 0)];
    let origin = p.vehicle.position;
    let direction = p.vehicle.body_rotation().transform_vector3(Vec3::Y);
    assert_eq!(p.pick(origin, direction, 0.0), Some(0));
    p.wheel(origin, direction, 0.0, 40.0);
    assert!(p.auto_drag.is_some());
    p.wheel_precise(origin, direction, 0.0, 40.0);
    assert!(p.auto_drag.is_none());
}

#[test]
fn vr_menu_release_keeps_a_short_drag_and_stops_automatic_motion() {
    let mut p = player();
    p.drag(0.0, 0.5);
    p.release_for_menu(true);
    assert!((p.vehicle.var("value").unwrap() - 0.405).abs() < 1e-6);
    assert!(p.auto_drag.is_none());
    assert!(p.pressed_mesh.is_none());
    let mut moving = automatically_moving_control();
    moving.release_for_menu(true);
    assert!(moving.auto_drag.is_none());
    assert_eq!(moving.vehicle.var("releases"), Some(2.0));
}
