use super::*;

#[test]
fn hosted_session_enforces_session_buses_on_host_own_bus() {
    use clap::Parser;
    let mut app = crate::assemble_app(Args::parse_from(["openomsi"]), Default::default());
    // assemble_app loads the interface language from the saved settings and other tests
    // switch it through the shared i18n state. Pin this test to English afterwards and
    // also check the source recorded for the shown text, so neither can translate the
    // message under us.
    let _language = omsi_ui::i18n::LanguageGuard::capture();
    omsi_ui::i18n::set_language("en");
    let session_bus_error =
        |msg: &str| omsi_ui::i18n::english(msg).contains("only these buses can be driven here");
    let host = omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap();
    app.net.lan = Some(host);
    app.args.session_buses = Some("Vehicles/MAN_SD200/MAN_SD77.bus".into());

    // Disallowed host bus is cleared
    app.args.bus = Some("Vehicles/Forbidden/Forbidden.bus".into());
    app.tick_lan(0.016);
    assert_eq!(app.args.bus, None, "disallowed host bus is cleared");
    assert!(app.player.is_none());
    assert!(session_bus_error(&app.service_msg.as_ref().unwrap().0));

    // Allowed host bus is kept
    app.args.bus = Some("Vehicles/MAN_SD200/MAN_SD77.bus".into());
    app.tick_lan(0.016);
    assert_eq!(
        app.args.bus.as_deref(),
        Some("Vehicles/MAN_SD200/MAN_SD77.bus"),
        "allowed host bus is kept"
    );

    // A fresh rejection must come from placement, not from the previous tick.
    app.service_msg = None;
    app.place_vehicle("Vehicles/Forbidden/Forbidden.bus", None, None);
    assert!(app.menus.pending_placement.is_none());
    assert!(session_bus_error(&app.service_msg.as_ref().unwrap().0));

    // Changing vehicle after startup to a disallowed bus cleans up and puts host on foot
    let mut vehicle = crate::schedule::tests::script_test_vehicle("", "", "");
    std::sync::Arc::make_mut(&mut vehicle.ty).def.path =
        std::path::PathBuf::from("Vehicles/Forbidden/Forbidden.bus");
    app.player = Some(crate::spawn::test_player(vehicle, vec![]));
    app.tick_lan(0.016);
    assert!(app.player.is_none(), "disallowed driven vehicle is removed");
    assert_eq!(app.view, "foot", "host is placed on foot");
    assert!(session_bus_error(&app.service_msg.as_ref().unwrap().0));
}
