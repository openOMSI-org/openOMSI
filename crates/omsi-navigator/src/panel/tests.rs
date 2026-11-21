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
