use super::game_info::stop_info;
use super::*;

#[test]
fn disabled_session_accepts_cli_flag_and_returns_no_plugins() {
    use clap::Parser;
    let normal = crate::cli::Args::try_parse_from(["openomsi"]).unwrap();
    assert!(!normal.no_plugins);
    let disabled = crate::cli::Args::try_parse_from(["openomsi", "--no-plugins"]).unwrap();
    assert!(disabled.no_plugins);
    assert!(load(disabled.no_plugins).is_empty());
}

fn snapshot(info: Vec<(&'static str, InfoValue)>) -> Io<'static> {
    Io::snapshot(info)
}

/// A ticket's price or a jolt's acceleration reaches Lua as the number it reads as.
#[test]
fn f32_values_reach_lua_as_they_read() {
    assert_eq!(num_f32(2.1), InfoValue::Num(2.1));
    assert_eq!(num_f32(-5.4), InfoValue::Num(-5.4));
    assert_eq!(num_f32(38.0), InfoValue::Num(38.0));
}

#[test]
fn game_info_names_require_prefix_and_ignore_ascii_case() {
    let io = snapshot(vec![("heading", InfoValue::Num(93.0))]);
    assert_eq!(io.game_number("OPENOMSI_Heading"), Some(93.0));
    for name in [
        "heading",
        "openomsi",
        "openomsi_",
        "openomsi_unknown",
        "openomsí_heading",
        "💡💡💡heading",
    ] {
        assert_eq!(io.game_value(name), None, "{name}");
    }
}

#[test]
fn game_info_callbacks_keep_number_boolean_and_text_types() {
    let io = snapshot(vec![
        ("speed", InfoValue::Num(42.5)),
        ("paused", InfoValue::Bool(true)),
        ("on_foot", InfoValue::Bool(false)),
        ("destination", InfoValue::Text("Žďár nad Sázavou".into())),
        ("map_path", InfoValue::Text(String::new())),
    ]);
    assert_eq!(io.game_number("openomsi_speed"), Some(42.5));
    assert_eq!(io.game_number("openomsi_paused"), Some(1.0));
    assert_eq!(io.game_number("openomsi_on_foot"), Some(0.0));
    assert_eq!(
        io.game_string("openomsi_destination").as_deref(),
        Some("Žďár nad Sázavou")
    );
    assert_eq!(io.game_string("openomsi_map_path"), Some(String::new()));
    assert_eq!(io.game_number("openomsi_destination"), None);
    assert_eq!(io.game_string("openomsi_speed"), None);
    assert_eq!(io.game_string("openomsi_paused"), None);
}

#[test]
fn game_info_numbers_reject_non_finite_values_and_float_overflow() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        let io = snapshot(vec![("speed", InfoValue::Num(value))]);
        assert_eq!(io.game_number("openomsi_speed"), None);
    }
}

#[test]
fn legacy_vehicle_callbacks_still_require_a_player_vehicle() {
    let mut io = snapshot(vec![
        ("clock", InfoValue::Num(32400.0)),
        (
            "map_path",
            InfoValue::Text("maps/example/global.cfg".into()),
        ),
    ]);
    assert_eq!(io.var("openomsi_clock"), None);
    assert_eq!(io.string("openomsi_map_path"), None);
}

#[test]
fn plugins_next_stop_skips_pass_through_station() {
    let stop = |id: i64, name: &str, stops: bool| crate::schedule::PlannedStop {
        object_id: id,
        name: name.into(),
        arr: 0.0,
        dep: 0.0,
        position: None,
        dir: Default::default(),
        stops,
        closed: None,
    };
    let mut closed = stop(12, "Closed stop", true);
    closed.closed = Some("plugin diversion".into());
    let stops = vec![
        stop(11, "Due stop", true),
        stop(12, "Pass-through", false),
        closed,
        stop(13, "Next calling stop", true),
    ];

    let next = omsi_sim::timetable_run::next_calling_stop(&stops, 1);
    assert_eq!(next, 3);

    let mut info = Vec::new();
    stop_info(&mut info, &stops, next, false, None);
    let get = |key: &str| {
        info.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
    };
    assert_eq!(
        get("next_stop"),
        Some(InfoValue::Text("Next calling stop".into()))
    );
    assert_eq!(get("next_stop_number"), Some(InfoValue::Num(4.0)));
    assert_eq!(get("next_stop_id"), Some(InfoValue::Num(13.0)));
    assert_eq!(get("at_stop"), Some(InfoValue::Bool(false)));

    let depot_stops = vec![
        stop(21, "Depot start", false),
        stop(22, "Depot next", false),
    ];
    let depot_next = omsi_sim::timetable_run::next_calling_stop(&depot_stops, 0);
    assert_eq!(depot_next, 0);

    let mut depot_info = Vec::new();
    stop_info(&mut depot_info, &depot_stops, depot_next, false, None);
    let depot_get = |key: &str| {
        depot_info
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
    };

    assert_eq!(
        depot_get("next_stop"),
        Some(InfoValue::Text("Depot start".into()))
    );
    assert_eq!(depot_get("next_stop_number"), Some(InfoValue::Num(1.0)));
    assert_eq!(depot_get("at_stop"), Some(InfoValue::Bool(false)));
}

/// The next stop and the one before it, by object ID, with their distances: the stop
/// before skips a passing station, and a stop with no known place has no distance.
#[test]
fn info_names_the_stops_on_either_side_of_the_bus() {
    let stop = |id: i64, name: &str, x: f64, stops: bool| crate::schedule::PlannedStop {
        object_id: id,
        name: name.into(),
        arr: 0.0,
        dep: 0.0,
        position: (x >= 0.0).then_some(glam::DVec3::new(x, 0.0, 5.0)),
        dir: Default::default(),
        stops,
        closed: None,
    };
    let stops = vec![
        stop(11, "A", 0.0, true),
        stop(12, "B", 100.0, false),
        stop(13, "C", -1.0, true),
    ];
    let mut v = Vec::new();
    stop_info(
        &mut v,
        &stops,
        2,
        true,
        Some(glam::DVec3::new(30.0, 40.0, 0.0)),
    );
    let get = |k: &str| v.iter().find(|(key, _)| *key == k).map(|(_, x)| x.clone());
    assert_eq!(get("next_stop"), Some(InfoValue::Text("C".into())));
    assert_eq!(get("next_stop_id"), Some(InfoValue::Num(13.0)));
    assert_eq!(get("at_stop"), Some(InfoValue::Bool(true)));
    assert_eq!(get("next_stop_distance"), None);
    assert_eq!(get("previous_stop"), Some(InfoValue::Text("A".into())));
    assert_eq!(get("previous_stop_id"), Some(InfoValue::Num(11.0)));
    assert_eq!(get("previous_stop_distance"), Some(InfoValue::Num(50.0)));
    // at the first stop there is none before it
    let mut v = Vec::new();
    stop_info(&mut v, &stops, 0, false, None);
    assert!(
        v.iter()
            .all(|(k, _)| !k.starts_with("previous_stop") && *k != "next_stop_distance")
    );
}
