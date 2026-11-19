//! The panel's tests, moved out of `panel.rs` as they were.

use super::*;
use omsi_sim::traffic::LaneBuilder;

#[test]
fn destination_card_keeps_speed_limit_indicator_data() {
    let mut nav = Navigator::new(true, 0.85, "bottom-left");
    nav.set_destination(DVec2::new(0.0, 50.0), None);

    let mut net = Network {
        lanes: vec![LaneBuilder::polyline(
            vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 100.0, 0.0)],
            LaneKind::Street,
            3.0,
        )],
        ..Default::default()
    };
    net.lanes[0].speed_limit_kmh = 50.0;
    net.build_grid();

    let f = NavFrame {
        traffic: None,
        touch: false,
        players: Vec::new(),
        bus: DVec3::new(0.0, 10.0, 0.0),
        heading: 0.0,
        speed_kmh: 30.0,
        outside_temp: 20.0,
        inside_temp: 22.0,
        line: None,
        terminus: None,
        stops: Vec::new(),
        delay: None,
        passengers: None,
        stop_requested: false,
        time: 0.0,
        weekday: 0,
        date: 20261113,
        language: "en",
        screen: (1280.0, 720.0),
        ui_scale: 1.0,
        follow_window: false,
        dt: 1.0 / 60.0,
        info_rect: None,
        touch_gap: None,
    };
    let words = Words {
        days: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        off_route: "Off route",
        rerouting: "Recalculating route",
        recalculated: "Route recalculated",
        no_duty: "Free drive",
        last_stop: "Final stop",
        on_time: "on time",
        closed: "Closed",
    };

    nav.draw_cards(
        &mut Painter::new(),
        &f,
        &words,
        Some(&net),
        Rect::new(0.0, 0.0, 360.0, 240.0),
        1.0,
        &style::palette(true),
    );

    assert_eq!(nav.speed_limit, Some(50.0));
}

/// A frame with the bus at the origin and these other players of the session about.
fn with_players(players: Vec<NavPlayer>) -> NavFrame<'static> {
    NavFrame {
        traffic: None,
        touch: false,
        players,
        bus: DVec3::ZERO,
        heading: 0.0,
        speed_kmh: 30.0,
        outside_temp: 20.0,
        inside_temp: 22.0,
        line: None,
        terminus: None,
        stops: Vec::new(),
        delay: None,
        passengers: None,
        stop_requested: false,
        time: 0.0,
        weekday: 0,
        date: 20261113,
        language: "en",
        screen: (1280.0, 720.0),
        ui_scale: 1.0,
        follow_window: false,
        dt: 1.0 / 60.0,
        info_rect: None,
        touch_gap: None,
    }
}

/// The layers a plugin's frame map gives (cars on, transit, the other players, the icons and
/// the stops off) are that map's alone: the player's settings are never written, a cockpit
/// screen beside it draws what they say, and once the plugin unloads its panel and its look
/// go and the map draws what the settings say again - also a setting the player changes after.
#[test]
fn unloading_the_plugin_gives_the_map_the_player_s_layers_back() {
    let mut nav = Navigator::new(true, 0.85, "bottom-left");
    (nav.show_ai, nav.show_cars, nav.show_surface) = (true, false, true);
    let settings = |n: &Navigator| (n.show_ai, n.show_cars, n.show_surface);
    let before = settings(&nav);
    let look = PanelLook::hud(&nav, 360.0);
    let screen = PanelLook::screen((800, 480), 288.0);
    // (what `draw` takes of the frame map for a picture of this look)
    let over = |n: &Navigator, look: &PanelLook| {
        n.framed
            .filter(|_| look.plugins)
            .flatten()
            .map(|(_, _, l)| l)
    };
    let own = nav.layers_shown(&look, over(&nav, &look));
    assert!(own.transit && !own.cars && own.players && own.icons && own.stops);
    nav.frame_panel = Some(FramePanel {
        height: 400.0,
        map: Some(FrameMap {
            rect: [0.0, 40.0, 360.0, 288.0],
            cards: true,
        }),
        background: None,
    });
    nav.frame_look = Some(FrameLook {
        layers: FrameLayers {
            cars: Some(true),
            transit: Some(false),
            players: Some(false),
            icons: Some(false),
            stops: Some(false),
            ..FrameLayers::default()
        },
    });
    nav.framed = nav.framed_now(nav.frame_panel, 360.0, 500.0);
    let framed = nav.layers_shown(&look, over(&nav, &look));
    assert!(framed.cars && !framed.transit && !framed.players && !framed.icons && !framed.stops);
    assert!(framed.jams && framed.history == own.history);
    assert_eq!(settings(&nav), before, "the settings are never written");
    assert_eq!(
        nav.layers_shown(&screen, over(&nav, &screen)),
        nav.layers_shown(&screen, None)
    );
    // the plugin unloads
    nav.frame_panel = None;
    nav.frame_look = None;
    nav.framed = nav.framed_now(nav.frame_panel, 360.0, 500.0);
    assert_eq!(nav.layers_shown(&look, over(&nav, &look)), own);
    nav.show_cars = true;
    assert!(nav.layers_shown(&look, over(&nav, &look)).cars);
}

/// The other players of a session (the frame's `players`, as a second client sends them) are
/// drawn on the map with their purple puck and name while the players layer is on - the
/// setting's, or a plugin's map turning it on - and not at all while a plugin's map turns it
/// off.
#[test]
fn the_players_layer_draws_the_other_players_or_leaves_them_out() {
    let mut nav = Navigator::new(true, 0.85, "bottom-left");
    let player = |x: f64, y: f64, name: &str| NavPlayer {
        position: DVec3::new(x, y, 0.0),
        heading: 90.0,
        name: name.into(),
    };
    let look = PanelLook::hud(&nav, 360.0);
    let map = Rect::new(0.0, 0.0, 360.0, 288.0);
    // (the map's view straight onto it: a metre the whole width, the origin in the middle)
    let mut drawn = |f: &NavFrame, players: Option<bool>| {
        let over = FrameLayers {
            players,
            ..FrameLayers::default()
        };
        let shown = nav.layers_shown(&look, Some(over));
        let mut ui = Painter::new();
        nav.draw_map_marks(
            &mut ui,
            f,
            map,
            Mat4::IDENTITY,
            [map.x, map.y, map.w, map.h],
            DVec2::ZERO,
            1.0,
            0.0,
            shown,
            Vec::new(),
            style::palette(true),
        );
        ui.verts.iter().filter(|v| v.color == PLAYER.0).count()
    };
    let one = with_players(vec![player(0.3, 0.2, "Anna")]);
    let two = with_players(vec![player(0.3, 0.2, "Anna"), player(-0.4, -0.3, "Ben")]);
    let alone = with_players(Vec::new());
    assert_eq!(drawn(&alone, None), 0);
    let (n1, n2) = (drawn(&one, None), drawn(&two, None));
    assert!(n1 > 0 && n2 > n1, "{n1} {n2}");
    assert_eq!(drawn(&two, Some(true)), n2);
    assert_eq!(drawn(&two, Some(false)), 0);
    assert_eq!(drawn(&one, Some(false)), 0);
}
