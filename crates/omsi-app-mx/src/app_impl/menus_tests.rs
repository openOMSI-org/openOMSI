use super::*;
use clap::Parser;

#[test]
fn websocket_server_menu_offers_the_players_tour_controls() {
    for target in [
        "https://localhost",
        "http://localhost",
        "wss://localhost/ws",
        "ws://localhost/ws",
    ] {
        let args = Args::parse_from(["openomsi", "--lan-join", target]);
        let menu = game_menu_for(&args);
        assert!(menu.contains(&("duty", "Line and tour...")), "{target}");
        assert!(menu.contains(&("endduty", "End the tour")), "{target}");
        assert!(menu.contains(&("quit", "Leave the server")), "{target}");
        for local_only in ["save", "saveslot", "load", "skipstop"] {
            assert!(
                !menu.iter().any(|(id, _)| *id == local_only),
                "{target}: {local_only}"
            );
        }
    }
}

#[test]
fn solo_and_udp_menus_keep_their_existing_actions() {
    for extra in [vec![], vec!["--lan-join", "127.0.0.1:27015"]] {
        let args = Args::parse_from(std::iter::once("openomsi").chain(extra));
        let menu = game_menu_for(&args);
        assert_eq!(menu, &GAME_MENU);
        assert!(menu.contains(&("duty", "Line and tour...")));
        assert!(menu.contains(&("endduty", "End the tour")));
        assert!(menu.contains(&("quit", "End the session")));
    }
}

#[test]
fn opening_pause_settings_enters_the_searchable_settings_page() {
    let args = Args::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, crate::settings::Settings::default());
    app.menus.game_menu = Some(0);

    app.switch_pause_category(crate::shell::pause::navigation::Category::Settings);

    assert_eq!(
        app.shell.pause_category,
        crate::shell::pause::navigation::Category::Settings
    );
    assert_eq!(
        app.menus.list_kind,
        Some(crate::game_lists::ListKind::Options(0))
    );
    assert_eq!(
        app.menus
            .game_menu
            .and_then(|index| app.game_menu_items().get(index).map(|line| line.0)),
        Some("options")
    );
}

#[test]
fn opening_vehicle_and_world_category_enters_the_combined_settings_page() {
    let args = Args::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, crate::settings::Settings::default());
    app.menus.game_menu = Some(0);

    app.switch_pause_category(crate::shell::pause::navigation::Category::World);

    assert_eq!(
        app.shell.pause_category,
        crate::shell::pause::navigation::Category::World
    );
    assert_eq!(
        app.menus.list_kind,
        Some(crate::game_lists::ListKind::World(0))
    );
    assert_eq!(
        app.menus
            .game_menu
            .and_then(|index| app.game_menu_items().get(index).map(|line| line.0)),
        Some("world")
    );
}

#[test]
fn vehicle_and_world_entries_open_the_same_combined_settings_list() {
    for id in ["vehicle", "world"] {
        let args = Args::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, crate::settings::Settings::default());
        assert!(app.game_menu_action(id));
        assert_eq!(
            app.menus.list_kind,
            Some(crate::game_lists::ListKind::World(0))
        );
    }
}

#[test]
fn vehicle_selection_back_returns_to_combined_vehicle_and_world_settings() {
    let args = Args::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, crate::settings::Settings::default());
    app.menus.game_menu = Some(0);
    app.open_list(crate::game_lists::ListKind::PlaceMaker);
    let back = app
        .menus
        .admin_list
        .as_ref()
        .and_then(|rows| rows.iter().position(|row| row.1 == "back"))
        .expect("vehicle selection has a back row");

    app.chooser_pick(back);

    assert_eq!(
        app.menus.list_kind,
        Some(crate::game_lists::ListKind::World(0))
    );
}

#[test]
fn livery_selection_back_returns_to_combined_vehicle_and_world_settings() {
    let args = Args::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, crate::settings::Settings::default());
    app.menus.game_menu = Some(0);
    app.open_list(crate::game_lists::ListKind::PlaceLivery(
        "missing-bus".into(),
    ));
    let back = app
        .menus
        .admin_list
        .as_ref()
        .and_then(|rows| rows.iter().position(|row| row.1 == "back"))
        .expect("livery selection has a back row");

    app.chooser_pick(back);

    assert_eq!(
        app.menus.list_kind,
        Some(crate::game_lists::ListKind::World(0))
    );
}

#[test]
fn hof_selection_back_returns_to_combined_vehicle_and_world_settings() {
    let args = Args::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, crate::settings::Settings::default());
    app.menus.game_menu = Some(0);
    app.open_list(crate::game_lists::ListKind::PlaceHof(
        "missing-bus".into(),
        "paint".into(),
    ));
    let back = app
        .menus
        .admin_list
        .as_ref()
        .and_then(|rows| rows.iter().position(|row| row.1 == "back"))
        .expect("HOF selection has a back row");

    app.chooser_pick(back);

    assert_eq!(
        app.menus.list_kind,
        Some(crate::game_lists::ListKind::World(0))
    );
}

#[test]
fn vehicle_world_subpages_all_return_to_combined_settings() {
    let args = Args::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, crate::settings::Settings::default());
    let subpages = [
        crate::game_lists::ListKind::Drivers,
        crate::game_lists::ListKind::Numbers,
        crate::game_lists::ListKind::Liveries,
        crate::game_lists::ListKind::SwitchTo,
        crate::game_lists::ListKind::Destinations,
        crate::game_lists::ListKind::RouteNumbers,
        crate::game_lists::ListKind::Hofs,
        crate::game_lists::ListKind::Spots,
        crate::game_lists::ListKind::PlaceMaker,
        crate::game_lists::ListKind::PlaceType("maker".into()),
        crate::game_lists::ListKind::PlaceLivery("bus".into()),
        crate::game_lists::ListKind::PlaceHof("bus".into(), "paint".into()),
    ];

    for kind in subpages {
        assert_eq!(
            crate::game_lists::run(&mut app, &kind, "back"),
            Some(crate::game_lists::ListKind::World(0)),
            "back from {kind:?} should reopen combined vehicle and world settings"
        );
    }
}
