use super::*;

fn app() -> crate::App {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    crate::assemble_app(args, Default::default())
}

#[test]
fn the_tiles_have_icons_and_omsis_first_page() {
    for (id, _, icon) in TILES {
        let pixels = omsi_ui::icons::rasterize(icon, 48);
        assert!(
            pixels.is_some_and(|pixels| pixels.iter().any(|alpha| *alpha > 0)),
            "{id}: no drawable icon {icon}"
        );
    }
    let ids: std::collections::HashSet<_> = TILES.iter().map(|t| t.0).collect();
    assert_eq!(ids.len(), TILES.len());
    assert_eq!(pages(), 2);
    assert_eq!(page_tiles(0).len(), 15);
    assert!(omsi_ui::icons::svg("chevron_right").is_some());
}

#[test]
fn every_menu_line_tile_is_a_line_of_the_game_menu() {
    for id in MENU_LINES {
        assert!(TILES.iter().any(|t| t.0 == id), "{id}");
    }
    let all: Vec<&str> = crate::app_impl::GAME_MENU
        .iter()
        .map(|l| l.0)
        .chain(["tobus", "players", "admin"])
        .collect();
    for id in MENU_LINES {
        assert!(all.contains(&id), "{id} is no game menu line");
    }
}

#[test]
fn after_a_tile_clicked_the_arrows_drive_the_bus_again() {
    use winit::keyboard::KeyCode as K;
    let mut a = app();
    a.menus.game_panels.quick = Some(QuickMenu::default());
    assert!(
        key(&mut a, K::ArrowUp),
        "worked from the keys, the menu has the arrows"
    );
    a.menus.game_panels.quick = Some(QuickMenu {
        mouse_kept: true,
        ..Default::default()
    });
    for k in [
        K::ArrowUp,
        K::ArrowDown,
        K::ArrowLeft,
        K::ArrowRight,
        K::Enter,
        K::Tab,
        K::PageUp,
    ] {
        assert!(!key(&mut a, k), "{k:?} goes to the game");
    }
}

#[test]
fn the_arrows_walk_the_grid_and_turn_the_page() {
    let mut q = QuickMenu::default();
    q.step(1, 0);
    assert_eq!((q.page, q.focus), (0, 1));
    q.step(0, 1);
    assert_eq!((q.page, q.focus), (0, 6));
    q.step(0, 1);
    q.step(0, 1);
    assert_eq!(q.focus, 11, "no row under the last");
    q.focus = 14;
    q.step(1, 0);
    // the second page has 14 tiles: its third row ends at 13
    assert_eq!((q.page, q.focus), (1, 10));
    q.step(-1, 0);
    assert_eq!((q.page, q.focus), (0, 14));
    q.focus = 0;
    q.step(-1, 0);
    assert_eq!((q.page, q.focus), (1, 4));
    q.turn(1);
    assert_eq!(q.page, 0);
}

#[test]
fn a_still_cursor_leaves_the_keys_choice() {
    let mut q = QuickMenu {
        cursor: Some((5.0, 5.0)),
        ..Default::default()
    };
    q.hover("remove", (5.0, 5.0));
    assert_eq!(q.focus, 0);
    q.hover("remove", (6.0, 5.0));
    assert_eq!(q.focus, 2);
    q.step(1, 0);
    q.hover("remove", (6.0, 5.0));
    assert_eq!(q.focus, 3);
}

#[test]
fn the_panel_shows_the_chosen_tiles_name_and_the_lit_and_greyed_ones() {
    let q = QuickMenu {
        focus: 7,
        ..Default::default()
    };
    let p = q.panel_of(
        &TileState {
            off: vec!["map"],
            lit: vec!["arrows"],
        },
        Palette::OPENOMSI,
    );
    let tiles: Vec<&Element> = p
        .children
        .iter()
        .take(3)
        .flat_map(|r| match &r.kind {
            Kind::Row { children, .. } => children.iter().collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect();
    assert_eq!(tiles.len(), 15);
    let of = |id: &str| tiles.iter().find(|t| t.id.as_deref() == Some(id)).unwrap();
    assert!(matches!(of("repair").kind, Kind::Tile { focus: true, .. }));
    assert!(matches!(of("map").kind, Kind::Tile { off: true, .. }));
    assert!(!of("map").takes_clicks());
    assert_eq!(of("arrows").color, Some(LIT));
    let Kind::Row { children: foot, .. } = &p.children[3].kind else {
        panic!()
    };
    assert!(matches!(&foot[0].kind, Kind::Text { text, .. } if text == "Repair"));
    assert!(foot.iter().any(|e| e.id.as_deref() == Some("next")));
}

#[test]
fn keys_open_and_start_tiles() {
    use winit::keyboard::KeyCode as K;
    let mut app = app();
    assert!(!key(&mut app, K::ArrowRight), "no menu, no key");
    toggle(&mut app);
    // to "Time" (row 3, column 2) and Enter: the world window's Time page
    for k in [K::ArrowDown, K::ArrowDown, K::ArrowRight] {
        assert!(key(&mut app, k));
    }
    assert_eq!(
        app.menus
            .game_panels
            .quick
            .as_ref()
            .and_then(|q| q.chosen()),
        Some("time")
    );
    assert!(key(&mut app, K::Enter));
    assert!(app.menus.game_panels.quick.is_none());
    assert!(app.menus.chooser.is_some());
    app.close_list();
    app.close_game_menu();
}

#[test]
fn game_panels_follow_the_light_theme_setting() {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, Default::default());
    app.settings.ui_theme = "light".into();
    toggle(&mut app);
    app.frame_game_panels(0.0, [0.0, 0.0, 1280.0, 720.0], false);

    let panel = app
        .menus
        .game_panels
        .ui
        .panels()
        .iter()
        .find(|p| p.id == "quick")
        .unwrap();
    let background = panel.panel.background.unwrap();
    assert!(
        background.0[0] > 230 && background.0[1] > 230 && background.0[2] > 230,
        "{background:?}"
    );
}

#[test]
fn a_tile_that_opens_a_list_opens_the_game_menu_under_it() {
    let mut app = app();
    for tile in ["time", "weather", "stats", "controls", "options", "camera"] {
        toggle(&mut app);
        click(&mut app, tile);
        assert!(app.menus.chooser.is_some(), "{tile}: no list");
        assert!(
            app.menus.game_menu.is_some(),
            "{tile}: the list gets no input"
        );
        app.close_list();
        app.close_game_menu();
    }
}

#[test]
fn a_tile_done_in_place_keeps_the_menu_open_on_its_page() {
    let mut app = app();
    toggle(&mut app);
    click(&mut app, "next");
    click(&mut app, "arrows");
    assert!(
        app.menus
            .game_panels
            .quick
            .as_ref()
            .is_some_and(|q| q.page == 1),
        "the menu stays, as in OMSI 2"
    );
    assert!(app.settings.nav_dynamic_arrows);
    assert!(!app.settings.nav_map_arrows);
    click(&mut app, "time");
    assert!(app.menus.game_panels.quick.is_none(), "a list takes over");
}

#[test]
fn the_page_arrow_turns_the_page_and_keeps_the_menu() {
    let mut app = app();
    toggle(&mut app);
    click(&mut app, "next");
    assert_eq!(
        app.menus.game_panels.quick.as_ref().map(|q| q.page),
        Some(1)
    );
    click(&mut app, "next");
    assert_eq!(
        app.menus.game_panels.quick.as_ref().map(|q| q.page),
        Some(0)
    );
}

#[test]
fn a_tile_leaves_the_pause_as_it_was() {
    let mut app = app();
    for (paused, stale) in [(true, false), (false, true)] {
        for tile in ["reset", "remove", "repair"] {
            app.paused = paused;
            app.menus.menu_prev_pause = stale;
            toggle(&mut app);
            click(&mut app, tile);
            assert_eq!(app.paused, paused, "{tile}");
        }
    }
}
