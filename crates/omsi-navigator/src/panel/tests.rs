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

/// Without marks the stops' pins are the colours they always had; a closed stop is one of the
/// others, and the stops behind the bus are grey.
#[test]
fn no_marks_are_todays_pins() {
    for closed in [false, true] {
        assert_eq!(
            stop_pin_color(PinKind::Next, closed, None),
            style::LIGHT.route
        );
        assert_eq!(
            stop_pin_color(PinKind::Last, closed, None),
            Color::hex(0xe5483d)
        );
        assert_eq!(
            stop_pin_color(PinKind::Other, closed, None),
            Color::hex(0x5d6b80)
        );
        assert_eq!(
            stop_pin_color(PinKind::Done, closed, None),
            Color::hex(0x808080)
        );
    }
    // (marks that name nothing change nothing either)
    let none = FrameMarks::default();
    for kind in [PinKind::Next, PinKind::Last, PinKind::Other, PinKind::Done] {
        assert_eq!(
            stop_pin_color(kind, true, Some(&none)),
            stop_pin_color(kind, true, None)
        );
    }
}

#[test]
fn a_closed_stop_gets_its_colour_only_when_asked() {
    let pink = Color::hex(0xff00aa);
    let teal = Color::hex(0x00aaaa);
    let asked = FrameMarks {
        closed_stop: Some(pink),
        stop: Some(teal),
        ..Default::default()
    };
    // (a closed stop has the colour of its own, an open one that of its kind)
    assert_eq!(stop_pin_color(PinKind::Other, true, Some(&asked)), pink);
    assert_eq!(stop_pin_color(PinKind::Other, false, Some(&asked)), teal);
    // (not asked: the closed stop is one of the others as before)
    let plain = FrameMarks {
        stop: Some(teal),
        ..Default::default()
    };
    assert_eq!(stop_pin_color(PinKind::Other, true, Some(&plain)), teal);
    // (the ones behind the bus stay grey)
    assert_eq!(
        stop_pin_color(PinKind::Done, true, Some(&asked)),
        Color::hex(0x808080)
    );
    // (the next and the last stop have a colour each)
    let ends = FrameMarks {
        next_stop: Some(pink),
        last_stop: Some(teal),
        ..Default::default()
    };
    assert_eq!(stop_pin_color(PinKind::Next, false, Some(&ends)), pink);
    assert_eq!(stop_pin_color(PinKind::Last, false, Some(&ends)), teal);
    assert_eq!(
        stop_pin_color(PinKind::Other, false, Some(&ends)),
        Color::hex(0x5d6b80)
    );
}

/// The bus is the navigator's red-orange puck of 10 unless the marks say otherwise: no marks,
/// or none for the bus, keep it.
#[test]
fn the_bus_puck_is_todays_without_marks() {
    assert_eq!(bus_mark(None), (style::LIGHT.route, 10.0));
    assert_eq!(
        bus_mark(Some(&FrameMarks::default())),
        (style::LIGHT.route, 10.0)
    );

    let m = FrameMarks {
        bus: Some(Color::hex(0x00aa00)),
        bus_size: Some(16.0),
        ..Default::default()
    };
    assert_eq!(bus_mark(Some(&m)), (Color::hex(0x00aa00), 16.0));
}
