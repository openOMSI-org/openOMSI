use super::*;

fn panel(src: &str) -> Result<Panel, String> {
    panel_from_source(src)
}

#[test]
fn colours() {
    assert_eq!(Rgba::parse("#F47F30"), Some(Rgba([0xF4, 0x7F, 0x30, 255])));
    assert_eq!(
        Rgba::parse("#14161acc"),
        Some(Rgba([0x14, 0x16, 0x1A, 0xCC]))
    );
    for bad in ["F47F30", "#F47F3", "#GGGGGG", "#F47F30C", "#ÄÄÄ"] {
        assert_eq!(Rgba::parse(bad), None, "{bad}");
    }
}

/// Version 4: boxes, stop lists, bars of parts, a badge's weight, tables with aligned
/// columns and cells of their own colour.
#[test]
fn boxes_stops_bars_of_parts_and_tables_are_read() {
    let p = panel(
        r##"{ children = {
            { type = "box", color = "#F58620", children = { { type = "text", text = "Next 08:10", id = "next" } } },
            { type = "stops", stops = { { name = "Markt", time = "08:05", expected = "08:09", expected_color = "#FF5F57", state = "now" }, { name = "Rathaus", time = "08:12", tag = "terminus" } } },
            { type = "bar", segments = { { value = 2, color = "#2A75F7" }, { value = 5 } } },
            { type = "table", aligns = { "left", "right" }, rows = { { "Zoo", { text = "+3:12", color = "#FF5F57" } }, { "Markt", "12:05" } } },
            { type = "table", rows = { { "Zoo", "12:01" } } },
            { type = "badge", text = "62", weight = "bold" },
        } }"##,
    )
    .unwrap();
    let Kind::Box { children, .. } = &p.children[0].kind else {
        panic!("{:?}", p.children[0])
    };
    assert_eq!(children.len(), 1);
    let Kind::Stops { stops, size } = &p.children[1].kind else {
        panic!("{:?}", p.children[1])
    };
    assert_eq!(
        (stops[0].state, stops[1].state, stops[1].tag.as_str(), *size),
        (StopState::Now, StopState::Ahead, "terminus", 13.0)
    );
    assert_eq!(stops[0].expected_color, Rgba::parse("#FF5F57"));
    let Kind::Bar { segments, .. } = &p.children[2].kind else {
        panic!("{:?}", p.children[2])
    };
    assert_eq!(
        segments,
        &[
            (2.0, Rgba([0x2A, 0x75, 0xF7, 255])),
            (5.0, Rgba([232, 160, 48, 255]))
        ]
    );
    let Kind::Table {
        rows, aligns, inks, ..
    } = &p.children[3].kind
    else {
        panic!("{:?}", p.children[3])
    };
    assert_eq!(
        rows,
        &[
            vec!["Zoo".to_string(), "+3:12".into()],
            vec!["Markt".into(), "12:05".into()]
        ]
    );
    assert_eq!(aligns, &[Align::Left, Align::Right]);
    assert_eq!(
        inks,
        &[vec![None, Rgba::parse("#FF5F57")], vec![None, None]]
    );
    // (a table without a coloured cell keeps no colours)
    let Kind::Table { inks, .. } = &p.children[4].kind else {
        panic!("{:?}", p.children[4])
    };
    assert!(inks.is_empty());
    assert!(matches!(
        p.children[5].kind,
        Kind::Badge {
            weight: Weight::Bold,
            ..
        }
    ));
    // an element in a box is found by its id, as one in a row
    let mut ui = UiState::default();
    ui.set(1, "p", p).unwrap();
    ui.update(
        1,
        "p",
        "next",
        &Value::map([("text", "Next 08:20".into())]),
        None,
    )
    .unwrap();
    assert!(
        matches!(&ui.get(1, "p").unwrap().panel.children[0].kind, Kind::Box { children, .. } if matches!(&children[0].kind, Kind::Text { text, .. } if text == "Next 08:20"))
    );
    // a table's alignments and a bar's parts change in place too, with its rows or value
    let mut p = panel(r##"{ children = { { type = "table", id = "t", rows = { { "a", "b" } } }, { type = "bar", id = "b", value = 0.5 } } }"##).unwrap();
    p.children[0].id = Some("t".into());
    ui.set(1, "q", p).unwrap();
    let cells = Value::List(vec![Value::List(vec!["x".into(), "y".into()])]);
    ui.update(
        1,
        "q",
        "t",
        &Value::map([
            ("aligns", Value::List(vec!["left".into(), "right".into()])),
            ("rows", cells),
        ]),
        None,
    )
    .unwrap();
    let parts = Value::List(vec![Value::map([("value", 1i64.into())])]);
    ui.update(
        1,
        "q",
        "b",
        &Value::map([("value", 0.25.into()), ("segments", parts)]),
        None,
    )
    .unwrap();
    let q = &ui.get(1, "q").unwrap().panel;
    assert!(
        matches!(&q.children[0].kind, Kind::Table { aligns, rows, .. } if aligns == &[Align::Left, Align::Right] && rows[0][1] == "y")
    );
    assert!(
        matches!(&q.children[1].kind, Kind::Bar { value, segments, .. } if *value == 0.25 && segments.len() == 1)
    );
    for (src, want) in [
        (
            r#"{ children = { { type = "table", aligns = { "middle" } } } }"#,
            "children[1].aligns: \"middle\" is no alignment",
        ),
        (
            r#"{ children = { { type = "stops", stops = { { state = "gone" } } } } }"#,
            "children[1].stops[1].state: \"gone\" is no state",
        ),
        (
            r#"{ children = { { type = "bar", segments = { 1 } } } }"#,
            "children[1].segments[1]: a part is a table",
        ),
    ] {
        let e = panel(src).unwrap_err();
        assert!(e.contains(want), "{src}: {e}");
    }
}

#[test]
fn a_panel_for_the_navigators_board() {
    let p = panel(
        r##"{ anchor = "navigator", children = { { type = "text", text = "Next: Markt" } } }"##,
    )
    .unwrap();
    assert_eq!(
        (p.anchor, p.anchor.factors()),
        (Anchor::Navigator, (0.0, 0.0))
    );
    // the game says where the board is, or that there is none
    let mut ui = UiState::default();
    assert_eq!(ui.dock(), None);
    let d = Dock {
        width: 360.0,
        room: 420.0,
        height: 96.0,
        owner: Some(7),
        frame: false,
    };
    ui.set_dock(Some(d));
    assert_eq!(ui.dock(), Some(d));
}

#[test]
fn a_panel_that_fills_the_navigators_frame_with_its_map() {
    let p = panel(
        r##"{ anchor = "navigator", frame = true, children = {
        { type = "text", text = "Line 5" },
        { type = "map", height = 180, cards = false },
        { type = "map" },
    } }"##,
    )
    .unwrap();
    assert!(p.frame && !Panel::default().frame);
    assert_eq!(
        p.children[1].kind,
        Kind::Map {
            height: 180.0,
            cards: false,
            colors: Vec::new()
        }
    );
    // (no height: the navigator's own map's; its cards over it)
    assert_eq!(
        p.children[2].kind,
        Kind::Map {
            height: 0.0,
            cards: true,
            colors: Vec::new()
        }
    );
    assert!(
        panel(r##"{ frame = "yes" }"##)
            .unwrap_err()
            .contains("frame")
    );
}

/// A frame panel with a map of these `colors`, read.
fn map_colors(colors: &str) -> Result<Vec<(String, Rgba)>, String> {
    let p = panel(&format!(
        "{{ anchor = \"navigator\", frame = true, children = {{ {{ type = \"map\", colors = {colors} }} }} }}"
    ))?;
    match &p.children[0].kind {
        Kind::Map { colors, .. } => Ok(colors.clone()),
        _ => unreachable!(),
    }
}

/// The map's colours (version 8) by the palette's names; none given: the palette's own.
#[test]
fn a_map_takes_colours_of_its_own() {
    let mut c =
        map_colors(r##"{ land = "#E8E4D8", road_edge = "#00000080", card_text = "#101010" }"##)
            .unwrap();
    c.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        c,
        [
            ("card_text".to_string(), Rgba([0x10, 0x10, 0x10, 255])),
            ("land".to_string(), Rgba([0xE8, 0xE4, 0xD8, 255])),
            ("road_edge".to_string(), Rgba([0, 0, 0, 0x80])),
        ]
    );
    assert_eq!(map_colors("{}"), Ok(Vec::new()));
    assert_eq!(map_colors("nil"), Ok(Vec::new()));
    // (every name of the list is a colour of the map)
    for name in MAP_COLORS {
        let c = map_colors(&format!(r##"{{ {name} = "#123456" }}"##)).unwrap();
        assert_eq!(c, vec![(name.to_string(), Rgba([0x12, 0x34, 0x56, 255]))]);
    }
}

#[test]
fn a_colour_of_the_wrong_kind_is_refused() {
    let e = map_colors(r##"{ land = "green" }"##).unwrap_err();
    assert!(e.contains("colors.land") && e.contains("#RRGGBB"), "{e}");
    let e = map_colors(r##"{ land = 3 }"##).unwrap_err();
    assert!(e.contains("colors.land"), "{e}");
    let e = map_colors(r##""#E8E4D8""##).unwrap_err();
    assert!(e.contains("children[1].colors"), "{e}");
    let e = map_colors(r##"{ "#E8E4D8" }"##).unwrap_err();
    assert!(e.contains("children[1].colors"), "{e}");
}

#[test]
fn an_unknown_colour_key_is_left_alone() {
    // (a plugin written for a later game still shows here; the labels' and contour lines'
    // colours, which the frame map does not draw, and the route's, which has a look of its
    // own, are no colours of this map either)
    let c = map_colors(r##"{ land = "#E8E4D8", sky = "#87CEEB" }"##).unwrap();
    assert_eq!(c, vec![("land".to_string(), Rgba([0xE8, 0xE4, 0xD8, 255]))]);
    for name in [
        "label",
        "label_halo",
        "contour",
        "route",
        "route_edge",
        "driven",
    ] {
        assert!(!MAP_COLORS.contains(&name));
        assert_eq!(
            map_colors(&format!(r##"{{ {name} = "#4A4A4F" }}"##)),
            Ok(Vec::new())
        );
    }
    // (an unknown name's value is not read: it may be of any kind)
    assert_eq!(map_colors("{ sky = 3 }"), Ok(Vec::new()));
}

#[test]
fn a_panel_is_read_with_its_defaults() {
    let p = panel(
        r##"{ anchor = "bottom_right", x = 16, width = 340, accent = "#F47F30", children = {
            { type = "text", text = "Linie 42", size = 18, weight = "bold" },
            { type = "row", align = "between", children = {
                { type = "icon", name = "schedule" },
                { type = "text", text = 12.5, grow = true },
                { type = "badge", text = "+2", color = "#2E7D32" },
            } },
            { type = "bar", value = 1.7 },
            { type = "text", text = "hidden", visible = false },
            { type = "sparkles", text = "from a later version" },
            { type = "button", id = "pause", text = "Pause", icon = "pause" },
        } }"##,
    )
    .unwrap();
    assert_eq!(p.anchor, Anchor::BottomRight);
    assert_eq!(
        (p.x, p.y, p.width, p.padding, p.gap, p.radius),
        (16.0, 0.0, 340.0, 12.0, 6.0, 12.0)
    );
    assert_eq!(p.accent, Some(Rgba([0xF4, 0x7F, 0x30, 255])));
    assert!(p.visible && !p.clickable && !p.grab && p.background.is_none());
    assert!(panel("{ grab = true }").unwrap().grab);
    // (a click from a grab starts no typing: the game asks which parts are fields)
    let mut ui = UiState::default();
    ui.set(1, "p", panel(r#"{ children = { { type = "input", id = "name" }, { type = "button", id = "ok", text = "OK" } } }"#).unwrap()).unwrap();
    assert!(
        ui.is_field(1, "p", "name") && !ui.is_field(1, "p", "ok") && !ui.is_field(2, "p", "name")
    );
    // (the hidden text and the unknown type are left out)
    assert_eq!(p.children.len(), 4);
    assert_eq!(
        p.children[0].kind,
        Kind::Text {
            text: "Linie 42".into(),
            size: 18.0,
            weight: Weight::Bold,
            align: Align::Left,
            wrap: true
        }
    );
    let Kind::Row {
        children,
        gap,
        align,
    } = &p.children[1].kind
    else {
        panic!("{:?}", p.children[1])
    };
    assert_eq!((*gap, *align, children.len()), (8.0, RowAlign::Between, 3));
    assert!(children[1].grow);
    assert_eq!(
        children[1].kind,
        Kind::Text {
            text: "12.5".into(),
            size: 14.0,
            weight: Weight::Regular,
            align: Align::Left,
            wrap: true
        }
    );
    // (held to its range)
    assert_eq!(
        p.children[2].kind,
        Kind::Bar {
            value: 1.0,
            height: 6.0,
            background: None,
            segments: Vec::new()
        }
    );
    assert!(p.children[3].takes_clicks() && !p.children[0].takes_clicks());
}

#[test]
fn mistakes_say_where_they_are() {
    let cases = [
        (r#"{ width = "wide" }"#, "width: a number is expected"),
        (
            r#"{ anchor = "middle" }"#,
            "anchor: \"middle\" is no anchor",
        ),
        (
            r#"{ background = "red" }"#,
            "background: \"red\" is no colour",
        ),
        (
            r#"{ children = { { type = "button", text = "Go" } } }"#,
            "children[1]: a button needs an id",
        ),
        (
            r#"{ children = { { type = "row", children = { { type = "text", size = {} } } } } }"#,
            "children[1].children[1].size: a number is expected",
        ),
        (
            r#"{ children = { { text = "no type" } } }"#,
            "children[1]: no type",
        ),
        (
            r#"{ children = { "text" } }"#,
            "children[1]: an element is a table",
        ),
        (
            r#"{ children = { { type = "text", text = string.rep("x", 501) } } }"#,
            "children[1].text: longer than 500 characters",
        ),
        (
            r#"{ children = { { type = "text", weight = "heavy" } } }"#,
            "is no weight",
        ),
        (
            r#"{ children = { { type = "slider", value = 2 } } }"#,
            "a slider needs an id",
        ),
        (
            r#"{ children = { { type = "chart", values = { 1, "x" } } } }"#,
            "children[1].values[2]: a number is expected",
        ),
    ];
    for (src, want) in cases {
        let e = panel(src).unwrap_err();
        assert!(e.contains(want), "{src}: {e}");
    }
    // too many elements, rows counted with what is in them
    let e = panel("(function() local c = {} for i = 1, 101 do c[i] = { type = 'row', children = { { type = 'space' } } } end return { children = c } end)()").unwrap_err();
    assert!(e.contains("at most 200 elements"), "{e}");
    // a row holding itself ends at the depth limit
    let e = panel("(function() local r = { type = 'row' } r.children = { r } return { children = { r } } end)()").unwrap_err();
    assert!(e.contains("deep"), "{e}");
}

#[test]
fn panels_belong_to_their_plugin() {
    let p = panel("{}").unwrap();
    let mut ui = UiState::default();
    for i in 0..MAX_PANELS {
        ui.set(1, &format!("p{i}"), p.clone()).unwrap();
    }
    assert!(
        ui.set(1, "one too many", p.clone())
            .unwrap_err()
            .contains("at most 16 panels")
    );
    // replacing one is no new panel; another plugin has its own sixteen
    ui.set(1, "p0", p.clone()).unwrap();
    ui.set(2, "p0", p.clone()).unwrap();
    assert!(ui.set(1, "", p.clone()).is_err());
    assert_eq!(ui.panels().len(), MAX_PANELS + 1);
    assert!(ui.remove(1, "p3") && !ui.remove(1, "p3"));
    ui.clear(1);
    assert_eq!(ui.panels().iter().map(|p| p.owner).collect::<Vec<_>>(), [2]);
}

#[test]
fn the_same_panel_again_is_no_change() {
    let mut ui = UiState::default();
    ui.set(
        1,
        "trip",
        panel(r#"{ children = { { type = "text", text = "a" } } }"#).unwrap(),
    )
    .unwrap();
    let first = ui.panels()[0].revision;
    ui.set(
        1,
        "trip",
        panel(r#"{ children = { { type = "text", text = "a" } } }"#).unwrap(),
    )
    .unwrap();
    assert_eq!(ui.panels()[0].revision, first);
    ui.set(
        1,
        "trip",
        panel(r#"{ children = { { type = "text", text = "b" } } }"#).unwrap(),
    )
    .unwrap();
    assert_ne!(ui.panels()[0].revision, first);
}

#[test]
fn toasts_go_by_themselves_and_make_room() {
    let mut ui = UiState::default();
    for i in 0..MAX_TOASTS + 2 {
        ui.toast(1, parse_toast(&Value::Str(format!("t{i}")), None).unwrap());
    }
    let opts = Value::map([
        ("seconds", 1i64.into()),
        ("title", "Pay".into()),
        ("icon", "payments".into()),
        ("color", "#2E7D32".into()),
    ]);
    let spec = parse_toast(&Value::Int(42), Some(&opts)).unwrap();
    assert_eq!(
        spec,
        ToastSpec {
            text: "42".into(),
            title: Some("Pay".into()),
            icon: Some("payments".into()),
            color: Some(Rgba([0x2E, 0x7D, 0x32, 255])),
            seconds: 1.0
        }
    );
    ui.toast(2, spec);
    let bad = Value::map([("seconds", "long".into())]);
    assert_eq!(
        parse_toast(&Value::Int(1), Some(&bad)).unwrap_err(),
        "opts.seconds: a number is expected"
    );
    assert_eq!(ui.toasts().len(), MAX_TOASTS + 1);
    assert_eq!(ui.toasts()[0].text, "t2");
    ui.tick(2.0);
    assert!(ui.toasts().iter().all(|t| t.owner == 1));
    ui.tick(3.5);
    assert!(ui.toasts().is_empty());
}

#[test]
fn focus_clicks_and_owners() {
    let mut ui = UiState::default();
    ui.set(
        7,
        "menu",
        Panel {
            anchor: Anchor::Center,
            width: 200.0,
            clickable: true,
            ..Panel::default()
        },
    )
    .unwrap();
    assert!(ui.set_focus(7, true) && ui.focused());
    ui.release_focus();
    assert!(!ui.focused());
    // hidden under the game's menu, the panels let the mouse go and cannot have it
    assert!(ui.set_focus(7, true));
    ui.set_shown(false);
    assert!(!ui.focused() && !ui.set_focus(7, true));
    ui.set_shown(true);
    assert!(ui.set_focus(7, true));
    ui.click(7, "menu", Some("ok"));
    ui.click(8, "x", None);
    assert_eq!(
        ui.take_clicks(7),
        [Click {
            owner: 7,
            panel: "menu".into(),
            element: Some("ok".into())
        }]
    );
    assert!(ui.take_clicks(7).is_empty());
    // another plugin stops: the mouse stays; the one that asked for it stops: it goes
    ui.remove_owner(8);
    assert!(ui.focused());
    ui.click(8, "x", None);
    ui.remove_owner(7);
    assert!(ui.panels().is_empty() && !ui.focused());
    assert_eq!(ui.take_clicks(8).len(), 1);
}

#[test]
fn controls_change_and_tell_their_plugin() {
    let mut ui = UiState::default();
    let p = panel(
        r#"{ children = {
            { type = "checkbox", id = "rain", text = "Rain" },
            { type = "row", children = { { type = "slider", id = "vol", min = 0, max = 10, step = 1, value = 3 } } },
            { type = "input", id = "name", max = 5 },
            { type = "tabs", id = "tab", tabs = { "A", "B", "C" } },
            { type = "chart", values = { 1, 2, 3 }, style = "bars" },
            { type = "table", columns = { "Stop", "Time" }, rows = { { "Zoo", "12:01" } } },
        } }"#,
    )
    .unwrap();
    ui.set(1, "p", p).unwrap();
    ui.click(1, "p", Some("rain"));
    ui.set_control(1, "p", "vol", 0.72);
    ui.set_control(1, "p", "tab", 0.5);
    ui.click(1, "p", Some("name"));
    assert_eq!(ui.typing().map(|t| t.element.as_str()), Some("name"));
    ui.type_text(Some("Hello world"));
    ui.type_text(None);
    ui.stop_typing(true);
    let changes: Vec<(String, Value)> = ui
        .take_changes(1)
        .into_iter()
        .map(|c| (c.element, c.value))
        .collect();
    assert_eq!(
        changes,
        [
            ("rain".to_string(), Value::Bool(true)),
            ("vol".into(), Value::Num(7.0)),
            ("tab".into(), Value::Int(2)),
            ("name".into(), Value::Str("Hell".into()))
        ]
    );
    // (Enter in the field is a click on it; the checkbox's click was its change)
    assert_eq!(ui.take_clicks(1).len(), 1);
    assert_eq!(ui.value(1, "p", "vol"), Some(Value::Num(7.0)));
    ui.update(
        1,
        "p",
        "rain",
        &Value::map([("checked", false.into())]),
        None,
    )
    .unwrap();
    assert_eq!(ui.value(1, "p", "rain"), Some(Value::Bool(false)));
    assert!(ui.update(1, "p", "nothing", &Value::map([]), None).is_err());
    ui.drag(1, "p", 5.0, -2.0);
    assert_eq!(ui.get(1, "p").unwrap().moved, (5.0, -2.0));
}
