use super::*;

#[test]
fn hosted_session_enforces_session_buses_on_host_own_bus() {
    use clap::Parser;
    let mut app = crate::assemble_app(Args::parse_from(["openomsi"]), Default::default());
    let host = omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap();
    app.net.lan = Some(host);
    app.args.session_buses = Some("Vehicles/MAN_SD200/MAN_SD77.bus".into());

    // Disallowed host bus is cleared
    app.args.bus = Some("Vehicles/Forbidden/Forbidden.bus".into());
    app.tick_lan(0.016);
    assert_eq!(app.args.bus, None, "disallowed host bus is cleared");
    assert!(app.player.is_none());
    assert!(
        app.service_msg
            .as_ref()
            .unwrap()
            .0
            .contains("only these buses can be driven here")
    );

    // Allowed host bus is kept
    app.args.bus = Some("Vehicles/MAN_SD200/MAN_SD77.bus".into());
    app.tick_lan(0.016);
    assert_eq!(
        app.args.bus.as_deref(),
        Some("Vehicles/MAN_SD200/MAN_SD77.bus"),
        "allowed host bus is kept"
    );

    // In-game placement of disallowed bus is rejected
    app.place_vehicle("Vehicles/Forbidden/Forbidden.bus", None, None);
    assert!(app.menus.pending_placement.is_none());
    assert!(
        app.service_msg
            .as_ref()
            .unwrap()
            .0
            .contains("only these buses can be driven here")
    );

    // Changing vehicle after startup to a disallowed bus cleans up and puts host on foot
    let mut vehicle = crate::schedule::tests::script_test_vehicle("", "", "");
    std::sync::Arc::make_mut(&mut vehicle.ty).def.path =
        std::path::PathBuf::from("Vehicles/Forbidden/Forbidden.bus");
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
        own_set: None,
    };
    app.player = Some(crate::spawn::new_player(
        vehicle,
        render,
        vec![],
        vec![],
        false,
        &ty,
    ));
    app.tick_lan(0.016);
    assert!(app.player.is_none(), "disallowed driven vehicle is removed");
    assert_eq!(app.view, "foot", "host is placed on foot");
    assert!(
        app.service_msg
            .as_ref()
            .unwrap()
            .0
            .contains("only these buses can be driven here")
    );
}
