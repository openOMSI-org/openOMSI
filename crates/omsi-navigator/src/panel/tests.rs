//! The panel's tests, moved out of `panel.rs` as they were.

use super::*;
use omsi_sim::traffic::LaneBuilder;

fn a_frame() -> NavFrame<'static> {
    NavFrame {
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
    }
}

fn some_words() -> Words {
    Words {
        days: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        off_route: "Off route",
        rerouting: "Recalculating route",
        recalculated: "Route recalculated",
        no_duty: "Free drive",
        last_stop: "Final stop",
        on_time: "on time",
        closed: "Closed",
    }
}

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

    let (f, words) = (a_frame(), some_words());

    nav.draw_cards(
        &mut Painter::new(),
        &f,
        &words,
        Some(&net),
        Rect::new(0.0, 0.0, 360.0, 240.0),
        [true; 4],
        1.0,
        &style::palette(true),
    );

    assert_eq!(nav.speed_limit, Some(50.0));
}

/// How many vertices the cards over the map are drawn with, `show` (the next turn, the clock,
/// the speed, the next stop) of them on.
fn drawn(show: [bool; 4]) -> u32 {
    let mut nav = Navigator::new(true, 0.85, "bottom-left");
    nav.street_here = Some("Main Street".into());
    let mut ui = Painter::new();
    nav.draw_cards(
        &mut ui,
        &a_frame(),
        &some_words(),
        None,
        Rect::new(0.0, 0.0, 360.0, 240.0),
        show,
        1.0,
        &style::palette(true),
    );
    ui.len()
}

#[test]
fn only_the_cards_shown_are_drawn() {
    let all = drawn([true; 4]);
    assert!(all > 0);
    for i in 0..4 {
        let mut show = [true; 4];
        show[i] = false;
        assert!(drawn(show) < all, "card {i} is left out");
    }
    assert_eq!(drawn([false; 4]), 0);
}

#[test]
fn cards_false_still_hides_them_all() {
    assert_eq!(shown_cards(false, [true; 4]), [false; 4]);
    assert_eq!(
        shown_cards(true, [true, false, true, false]),
        [true, false, true, false]
    );
    assert_eq!(shown_cards(true, [true; 4]), [true; 4]);
}
