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

/// A frame for the route's tests: the bus at `bus`, nothing else on.
fn frame_at(bus: DVec3) -> NavFrame<'static> {
    NavFrame {
        traffic: None,
        touch: false,
        players: Vec::new(),
        bus,
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

/// A navigator on a route of two straight lanes northwards, the bus 10 m along the second,
/// and the network they are in.
fn on_a_route() -> (Navigator, Network) {
    let lane = |a: f64, b: f64| {
        LaneBuilder::polyline(
            vec![DVec3::new(0.0, a, 0.0), DVec3::new(0.0, b, 0.0)],
            LaneKind::Street,
            3.0,
        )
    };
    let mut net = Network {
        lanes: vec![lane(0.0, 200.0), lane(200.0, 400.0)],
        ..Default::default()
    };
    net.lanes[0].next = vec![1];
    let mut nav = Navigator::new(true, 0.85, "bottom-left");
    nav.set_route("trip", vec![0, 1], true, 1);
    nav.route.progress = 1;
    nav.route.s = 10.0;
    (nav, net)
}

/// The colours of the route's mesh in cache `slot`.
fn mesh_colours(nav: &Navigator, slot: usize) -> Vec<[f32; 4]> {
    let (_, verts, _) = nav.route_mesh_history[slot].as_ref().expect("a route mesh");
    verts.iter().map(|v| v.color).collect()
}

/// Without a look of its own (every picture but a frame map that has one) the route is
/// drawn as it always was.
#[test]
fn a_route_style_from_none_is_todays() {
    let style = route_style(None, DVec2::new(1.0, 2.0), 700.0);
    assert_eq!((style.extra_m, style.min_px), (-0.6, 5.0));
    assert_eq!(style.arrows, Some((32.0, 900.0)));
    assert_eq!(style.max_len, 12_000.0);
    assert_eq!(style.near, Some((DVec2::new(1.0, 2.0), 700.0)));
    let pal = *style::palette(true);
    let same = route_palette(&pal, None);
    assert_eq!((same.route, same.route_edge), (pal.route, pal.route_edge));
    assert!(ghost_shown(None) && jams_shown(None));
    assert_eq!((route_slot(false, None), route_slot(true, None)), (0, 1));
    // (a look that says nothing is today's too)
    let none = FrameRoute::default();
    assert_eq!(route_style(Some(&none), DVec2::ZERO, 1.0).min_px, 5.0);
    assert!(ghost_shown(Some(&none)) && jams_shown(Some(&none)));
    // (and one that does: its width, no arrows, no ghost)
    let own = FrameRoute {
        width: Some(9.0),
        arrows: Some(false),
        ghost: Some(false),
        ..FrameRoute::default()
    };
    let style = route_style(Some(&own), DVec2::ZERO, 1.0);
    assert_eq!((style.min_px, style.arrows), (9.0, None));
    assert!(!ghost_shown(Some(&own)));
    assert_eq!(
        (route_slot(false, Some(own)), route_slot(true, Some(own))),
        (2, 3)
    );
}

/// The route's mesh is kept while its look stays the same, and built once again when the
/// look changes; it then has the look's colours.
#[test]
fn the_route_mesh_key_changes_with_its_look() {
    let (mut nav, net) = on_a_route();
    let f = frame_at(DVec3::new(0.0, 210.0, 0.0));
    let pal = *style::palette(true);
    let on = (DVec2::ZERO, 3, 800.0);
    let green = FrameRoute {
        color: Some(Color::hex(0x22aa44)),
        ..FrameRoute::default()
    };
    let slot = nav.route_mesh(&net, &f, (false, Some(green)), on, false, &pal);
    nav.route_mesh(&net, &f, (false, Some(green)), on, false, &pal);
    assert_eq!(nav.route_mesh_generation[slot], 1, "the same look: kept");
    assert!(mesh_colours(&nav, slot).contains(&Color::hex(0x22aa44).0));
    assert!(!mesh_colours(&nav, slot).contains(&pal.route.0));
    let red = FrameRoute {
        color: Some(Color::hex(0xcc2222)),
        edge: Some(Color::hex(0x661111)),
        ..green
    };
    assert_eq!(
        nav.route_mesh(&net, &f, (false, Some(red)), on, false, &pal),
        slot
    );
    nav.route_mesh(&net, &f, (false, Some(red)), on, false, &pal);
    assert_eq!(
        nav.route_mesh_generation[slot], 2,
        "another look: once again"
    );
    let colours = mesh_colours(&nav, slot);
    assert!(colours.contains(&Color::hex(0xcc2222).0) && colours.contains(&Color::hex(0x661111).0));
}

/// Pictures drawn in turn - the frame map with a look of its own, a cockpit screen with
/// today's - keep each other's route: each mesh is built once, the screen's in today's look.
#[test]
fn pictures_in_turn_keep_each_other_s_route() {
    let (mut nav, net) = on_a_route();
    let f = frame_at(DVec3::new(0.0, 210.0, 0.0));
    let pal = *style::palette(true);
    let on = (DVec2::ZERO, 3, 800.0);
    let look = FrameRoute {
        color: Some(Color::hex(0x22aa44)),
        width: Some(12.0),
        ..FrameRoute::default()
    };
    let (mut frame, mut screen) = (0, 0);
    for _ in 0..3 {
        frame = nav.route_mesh(&net, &f, (false, Some(look)), on, false, &pal);
        screen = nav.route_mesh(&net, &f, (false, None), on, false, &pal);
    }
    assert_ne!(frame, screen);
    assert_eq!(nav.route_mesh_generation[frame], 1);
    assert_eq!(nav.route_mesh_generation[screen], 1);
    assert!(mesh_colours(&nav, screen).contains(&pal.route.0));
    assert!(!mesh_colours(&nav, screen).contains(&Color::hex(0x22aa44).0));
}

/// Jams colour the route where they are; with `jams = false` it keeps its own colour all
/// along.
#[test]
fn jams_off_keeps_the_route_colour() {
    let (mut nav, net) = on_a_route();
    nav.route_jam.insert(1, 1.0);
    let f = frame_at(DVec3::new(0.0, 210.0, 0.0));
    let pal = *style::palette(true);
    let on = (DVec2::ZERO, 3, 800.0);
    let jammed = nav.route_mesh(&net, &f, (false, None), on, false, &pal);
    assert!(mesh_colours(&nav, jammed).contains(&style::TRAFFIC[3].0));
    let calm = FrameRoute {
        jams: Some(false),
        ..FrameRoute::default()
    };
    let slot = nav.route_mesh(&net, &f, (false, Some(calm)), on, false, &pal);
    let colours = mesh_colours(&nav, slot);
    assert!(colours.contains(&pal.route.0), "the route's own colour");
    assert!(style::TRAFFIC[1..].iter().all(|t| !colours.contains(&t.0)));
}

/// The part of the route already driven shows only when a look gives it a colour (the
/// navigator never drew it), and then in that colour.
#[test]
fn the_part_driven_shows_in_its_colour_when_given() {
    let (mut nav, net) = on_a_route();
    let f = frame_at(DVec3::new(0.0, 210.0, 0.0));
    let pal = *style::palette(true);
    let on = (DVec2::ZERO, 3, 800.0);
    let today = nav.route_mesh(&net, &f, (false, None), on, false, &pal);
    assert!(!mesh_colours(&nav, today).contains(&pal.driven.0));
    let grey = Color::hex(0x808080);
    let look = FrameRoute {
        driven: Some(grey),
        ..FrameRoute::default()
    };
    let slot = nav.route_mesh(&net, &f, (false, Some(look)), on, false, &pal);
    assert!(mesh_colours(&nav, slot).contains(&grey.0));
}
