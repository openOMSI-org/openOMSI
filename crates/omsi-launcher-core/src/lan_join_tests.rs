#[test]
fn auto_join_passes_the_discovered_gateway_to_the_game() {
    let duty = Duty {
        map: "maps/x/global.cfg".into(),
        bus: "Vehicles/x.bus".into(),
        time: "09:00".into(),
        lan: Some("join:play.example.org:27015".into()),
        lan_ws_fallback: Some("https://gateway.example.org".into()),
        ..Default::default()
    };
    let args = duty_args_for_root(&duty, Path::new("test-omsi")).unwrap();
    assert!(
        args.windows(2)
            .any(|w| w == ["--lan-join", "play.example.org:27015"])
    );
    assert!(
        args.windows(2)
            .any(|w| w == ["--lan-ws-fallback", "https://gateway.example.org"])
    );
    let forced = duty_args_for_root(
        &Duty {
            lan_ws_fallback: None,
            ..duty
        },
        Path::new("test-omsi"),
    )
    .unwrap();
    assert!(!forced.iter().any(|arg| arg == "--lan-ws-fallback"));
}

/// A hosted session's buses reach the game, and a joined one's do not: a player who joins
/// drives its own session's buses, not its own list (#1464).
#[test]
fn a_hosted_session_passes_its_buses_to_the_game() {
    let host = Duty {
        map: "maps/x/global.cfg".into(),
        bus: "Vehicles/x.bus".into(),
        time: "09:00".into(),
        lan: Some("host".into()),
        session_buses: Some("  MAN_SD200; Vehicles/MAN_NL202/MAN_NL202.bus  ".into()),
        ..Default::default()
    };
    let args = duty_args_for_root(&host, Path::new("test-omsi")).unwrap();
    let at = args
        .iter()
        .position(|a| a == "--session-buses")
        .expect("the list");
    assert_eq!(args[at + 1], "MAN_SD200; Vehicles/MAN_NL202/MAN_NL202.bus");
    // a blank list is no list: every bus, as before
    let blank = duty_args_for_root(
        &Duty {
            session_buses: Some("   ".into()),
            ..host.clone()
        },
        Path::new("test-omsi"),
    )
    .unwrap();
    assert!(!blank.iter().any(|a| a == "--session-buses"));
    // a duty that joins says nothing about the session's buses
    let joining = Duty {
        lan: Some("join:play.example.org:27015".into()),
        ..host
    };
    let args = duty_args_for_root(&joining, Path::new("test-omsi")).unwrap();
    assert!(!args.iter().any(|a| a == "--session-buses"));
}
