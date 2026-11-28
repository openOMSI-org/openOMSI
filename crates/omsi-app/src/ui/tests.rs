#[test]
fn cockpit_tooltips_use_complete_catalogue_labels_and_mouse_suffixes() {
    let names = omsi_input::describe::ControlNames::from_table(
        "ENG",
        &[
            ("parking_brake_toggle", "Parking Brake On/Off"),
            ("bus_doorfront0", "Front Door, 1st Wing Open/Close"),
        ],
    );
    crate::with_test_language("pt", || {
        for (event, expected) in [
            (
                "parking_brake_mouse",
                "Freio de estacionamento: ligar/desligar",
            ),
            (
                "parking_brake_2",
                "Freio de estacionamento: ligar/desligar 2",
            ),
            (
                "bus_doorfront0_external",
                "Porta dianteira, primeira folha: abrir/fechar (externo)",
            ),
            (
                "cp_heizregler_temp",
                "Controle de temperatura do aquecimento",
            ),
        ] {
            assert_eq!(tooltip_label(&names.control(event)), expected, "{event}");
        }
        // A numbered full label takes precedence over the mouse suffix fallback.
        assert_eq!(
            tooltip_label("Ticket Printer: Select Ticket 2"),
            "Impressora de bilhetes: selecionar bilhete 2"
        );
        for event in ["ALMEX_Funktion", "Zahltisch_Wechsler_0_05"] {
            let label = names.control(event);
            assert_eq!(tooltip_label(&label), label, "untranslated mod: {event}");
        }
    });
    crate::with_test_language("ENG", || {
        for event in [
            "parking_brake_mouse",
            "parking_brake_2",
            "bus_doorfront0_external",
        ] {
            let label = names.control(event);
            assert_eq!(tooltip_label(&label), label);
        }
    });
}

#[test]
fn cockpit_mouse_suffixes_use_the_selected_catalogue_language() {
    // An existing Spanish text also works as a mod's control label.
    let names = omsi_input::describe::ControlNames::from_table("ENG", &[("mod_toggle", "on")]);
    crate::with_test_language("es", || {
        assert_eq!(tooltip_label(&names.control("mod_2")), "activado 2");
    });
}

/// The steering cross: white arms with a dark edge, open in the middle, nothing in the
/// corners - the same in all four directions.
#[test]
fn the_steering_cross_is_a_white_cross_with_a_dark_edge() {
    let n = 34;
    let px = super::steer_cross_pixels(n);
    let at = |x: usize, y: usize| &px[(y * n + x) * 4..(y * n + x) * 4 + 4];
    let c = n / 2;
    assert_eq!(at(c, c)[3], 0, "the middle is open");
    assert_eq!(at(0, 0)[3], 0, "the corners are empty");
    for (x, y) in [(c, 6), (c, n - 7), (6, c), (n - 7, c)] {
        assert_eq!(at(x, y), [255, 255, 255, 255], "arm at {x},{y}");
    }
    assert_eq!(at(c + 2, 6)[..3], [0, 0, 0], "the arm's edge is dark");
}

use super::*;

#[test]
fn a_server_notice_is_read_and_fades() {
    let (id, n) = Notice::parse("a1b2 5 warn Service 15/5: departure at 06:07 | depot").unwrap();
    assert_eq!(id, "a1b2");
    assert_eq!(
        (n.kind, n.text.as_str(), n.total, n.left),
        (
            NoticeKind::Warn,
            "Service 15/5: departure at 06:07 | depot",
            5.0,
            5.0
        )
    );
    // seconds held to 2 .. 30, an unknown kind is information
    assert_eq!(Notice::parse("x 0.5 info hi").unwrap().1.total, 2.0);
    assert_eq!(
        Notice::parse("x 600 whatever hi").unwrap().1,
        Notice {
            kind: NoticeKind::Info,
            text: "hi".into(),
            left: 30.0,
            total: 30.0
        }
    );
    // no text, no seconds, nothing: not a notice
    for bad in [
        "",
        "x",
        "x 5",
        "x 5 info",
        "x 5 info   ",
        "x five info hi",
        "x NaN info hi",
    ] {
        assert!(Notice::parse(bad).is_none(), "{bad}");
    }
    // fades in, stays, fades out (eased)
    let mut n = Notice::parse("x 5 alert hi").unwrap().1;
    assert_eq!(n.alpha(), 0.0);
    n.left = 4.0;
    assert_eq!(n.alpha(), 1.0);
    n.left = 0.2;
    assert!((n.alpha() - 0.5).abs() < 1e-6);
    assert_eq!(
        Notice::parse("x 5 action hi").unwrap().1.kind,
        NoticeKind::Action
    );
    // four at most: the oldest fade out to make room
    let mut list = Vec::new();
    for k in 0..6 {
        push_notice(
            &mut list,
            Notice::parse(&format!("{k} 5 info n{k}")).unwrap().1,
        );
    }
    let live: Vec<&str> = list
        .iter()
        .filter(|n| n.left > NOTICE_OUT)
        .map(|n| n.text.as_str())
        .collect();
    assert_eq!(live, ["n2", "n3", "n4", "n5"]);
    // the same text again stands once, for its time again, without fading in anew
    let mut list = vec![Notice::new(NoticeKind::Action, "Doors are open")];
    list[0].left = 2.0;
    push_notice(&mut list, Notice::new(NoticeKind::Action, "Doors are open"));
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].left, list[0].alpha()), (7.0, 1.0));
}

/// What the game tells the driver becomes a card when it starts, not every frame, and
/// again once it was gone for a while.
#[test]
fn a_note_becomes_a_notification_once_when_it_starts() {
    let mut w = NoteWatch::default();
    let mut list = Vec::new();
    let brake = |v: f32| {
        vec![(
            note_key(&format!("Air pressure is low ({v:.1} bar)")),
            format!("Air pressure is low ({v:.1} bar)"),
            NoticeKind::Action,
        )]
    };
    assert_eq!(
        note_key("Parking brake is on  (. releases it)"),
        "Parking brake is on"
    );
    for k in 0..100 {
        // (the pressure in the text changes, the condition is the same)
        w.frame(0.016, &brake(4.0 + k as f32 * 0.01), &mut list);
    }
    assert_eq!(list.len(), 1);
    // gone a moment (the throttle let go) and back: the same reason
    w.frame(1.0, &[], &mut list);
    w.frame(0.016, &brake(5.0), &mut list);
    assert_eq!(list.len(), 1);
    // gone for longer, and back: shown again
    w.frame(NoteWatch::REARM + 0.5, &[], &mut list);
    list.clear();
    w.frame(0.016, &brake(5.0), &mut list);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].kind, NoticeKind::Action);
}

/// The loading bar goes along the expected time evenly, never stands still or goes back,
/// takes the real count when that is ahead, runs to its end in about a third of a second
/// once the world is there, and then fades.
#[test]
fn the_loading_bar_follows_the_expected_time_and_fades() {
    assert_eq!(time_progress(0.0), 0.0);
    assert!((time_progress(1.0) - 0.92).abs() < 1e-6);
    let mut prev = 0.0;
    for k in 1..=60 {
        let p = time_progress(k as f32 * 0.05);
        assert!(p > prev && p < 0.98, "{k}: {p}");
        prev = p;
    }
    // half the time: about half the bar
    assert!((time_progress(0.5) - 0.53).abs() < 0.03);
    // a load of 4 s expected, the count standing at none: the bar goes on evenly
    let mut l = Loading {
        expected: 4.0,
        ..Default::default()
    };
    let mut shown = Vec::new();
    for _ in 0..240 {
        l.step(0.0, 1.0 / 60.0);
        shown.push(l.shown());
    }
    assert!(shown.windows(2).all(|w| w[1] >= w[0]));
    assert!(shown[119] > 0.4 && shown[119] < 0.65, "{}", shown[119]);
    assert!(shown[239] > 0.8 && shown[239] <= 0.92, "{}", shown[239]);
    // the count ahead of the time: the bar follows the count
    l.step(0.97, 1.0);
    assert!(l.shown() > 0.92);
    // the world is there: the rest in about 0.35 s, then a moment, then the fade
    l.finish();
    let mut t = 0.0;
    while l.shown() < 1.0 {
        l.step(1.0, 1.0 / 60.0);
        t += 1.0 / 60.0;
    }
    assert!(t < 0.4 + 1.0 / 60.0, "{t}");
    assert_eq!(l.over_session(), Some(1.0));
    for _ in 0..60 {
        l.step(1.0, 1.0 / 60.0);
    }
    assert_eq!(l.over_session(), None);
    // a map that needed no loading screen: nothing fades
    let mut quiet = Loading::default();
    quiet.finish();
    assert_eq!(quiet.over_session(), None);
    // the last three loads, averaged
    assert_eq!(expected_of(&serde_json::json!([2.0, 4.0, 6.0])), Some(4.0));
    assert_eq!(expected_of(&serde_json::json!([])), None);
}

#[test]
fn centre_hud_moves_menu_controls_and_chat_hitboxes_together() {
    let mut ui = Ui::new().unwrap();
    ui.menu_rects.push([20.0, 100.0, 900.0, 140.0]);
    ui.menu_ctl.push(Some([600.0, 110.0, 860.0, 130.0]));
    ui.menu_side.push([30.0, 100.0, 200.0, 140.0]);
    ui.dd_rects.push([650.0, 150.0, 850.0, 180.0]);
    ui.menu_arrows.push(Some([600.0, 640.0, 820.0]));
    ui.chat.rect = [10.0, 800.0, 400.0, 1000.0];
    ui.dd_scroll = Some(([850.0, 150.0, 860.0, 300.0], [844.0, 160.0, 864.0, 200.0]));
    ui.menu_pane_scroll = Some((
        [880.0, 150.0, 890.0, 700.0],
        [874.0, 160.0, 894.0, 220.0],
        40,
        10,
    ));
    ui.shift_hitboxes(1920.0);
    let track = ui.menu_ctl[0].unwrap();
    assert_eq!((2650.0 - track[0]) / (track[2] - track[0]), 0.5);
    assert!(ui.chat.contains(2020.0, 900.0));
    assert!(!ui.chat.contains(100.0, 900.0));
    assert_eq!(ui.dd_rects[0], [2570.0, 150.0, 2770.0, 180.0]);
    assert_eq!(ui.menu_arrows[0], Some([2520.0, 2560.0, 2740.0]));
    assert_eq!(ui.dd_scroll.unwrap().0[0], 2770.0);
    assert_eq!(ui.menu_pane_scroll.unwrap().1[0], 2794.0);
    ui.shift_hitboxes(-1920.0);
    assert_eq!(ui.menu_ctl[0], Some([600.0, 110.0, 860.0, 130.0]));
}

#[test]
fn a_long_chat_line_goes_on_in_rows() {
    // (10 px a character)
    let m = |t: &str| t.chars().count() as f32 * 10.0;
    // fits: one row, as it was
    assert_eq!(wrap_rows("Anton: hello", 200.0, 20.0, &m), ["Anton: hello"]);
    assert_eq!(wrap_rows("", 200.0, 20.0, &m), [""]);
    // between words; the rows after the first are 20 px narrower
    let rows = wrap_rows(
        "Admin (private): take tour 13/1 at 04:47 from the depot",
        200.0,
        20.0,
        &m,
    );
    assert_eq!(
        rows,
        [
            "Admin (private):",
            "take tour 13/1 at",
            "04:47 from the",
            "depot"
        ]
    );
    assert!(rows[0].chars().count() <= 20 && rows[1..].iter().all(|r| r.chars().count() <= 18));
    // nothing lost: the words come back in order
    assert_eq!(
        rows.join(" "),
        "Admin (private): take tour 13/1 at 04:47 from the depot"
    );
    // a word longer than a row is cut inside
    let rows = wrap_rows("x: https://example.org/a/very/long/link", 100.0, 20.0, &m);
    assert_eq!(
        rows,
        ["x:", "https://", "example.", "org/a/ve", "ry/long/", "link"]
    );
    assert_eq!(rows.concat(), "x:https://example.org/a/very/long/link");
    // a box narrower than a character still ends (a character a row)
    assert_eq!(wrap_rows("abc", 5.0, 0.0, &m), ["a", "b", "c"]);
}

#[test]
fn vr_settings_sidebar_keeps_back_clear() {
    for scale in [0.5, 1.0, 2.0] {
        for height in [220.0, 440.0] {
            let pages_top = (72.0 + 6.0) * scale;
            let back_top = (height - PAD - 6.0 - 38.0) * scale;
            for pages in [7, 8, 10] {
                let step =
                    vr_settings_sidebar_step(back_top - 6.0 * scale - pages_top, pages, scale);
                let last_bottom =
                    pages_top + (pages - 1) as f32 * step + (step - 4.0 * scale).max(1.0);
                assert!(last_bottom < back_top);
            }
        }
    }
    assert_eq!(vr_settings_sidebar_step(1000.0, 8, 1.0), 42.0);
}

#[test]
fn text_renders_with_an_outline() {
    let f = FontVec::try_from_vec(INTER.to_vec()).unwrap();
    let img = render_text(&f, "Savva: hi", 20.0, [255, 255, 255, 220]);
    assert!(img.width > 40 && img.height > 14);
    // white text and dark outline pixels are both there
    let px: Vec<&[u8]> = img.rgba.chunks(4).collect();
    assert!(px.iter().any(|p| p[3] > 200 && p[0] > 200));
    assert!(px.iter().any(|p| p[3] > 100 && p[0] < 40));
}

/// The Esc menu in Chinese, Korean and Thai: the system's fonts, not boxes.
#[test]
fn scripts_inter_lacks_come_from_the_system() {
    let f = FontVec::try_from_vec(INTER.to_vec()).unwrap();
    for t in ["继续", "繼續", "계속", "ดำเนินการต่อ"] {
        if t.chars()
            .next()
            .and_then(omsi_ui::text::fallback_font)
            .is_none()
        {
            continue;
        }
        for c in t.chars() {
            let g = font_for(&f, c);
            assert!(!std::ptr::eq(g, &f) && g.glyph_id(c).0 != 0, "{t}: {c}");
        }
        let img = render_text(&f, t, 20.0, [255, 255, 255, 220]);
        let ink = img
            .rgba
            .chunks(4)
            .filter(|p| p[3] > 128 && p[0] > 128)
            .count();
        assert!(ink > 30, "{t}: {ink}");
    }
}

/// Laid out for 1080p: a lower window as it always was, a taller one in proportion.
#[test]
fn the_interface_grows_with_tall_windows() {
    assert_eq!(size_factor(900.0, 1.0, 1.0, true), 1.0);
    assert_eq!(size_factor(1080.0, 1.0, 1.0, true), 1.0);
    assert_eq!(size_factor(2160.0, 1.0, 1.0, true), 2.0);
    assert_eq!(size_factor(4320.0, 1.0, 1.0, true), 2.0);
    // (4K at 150 % display scaling is 1440 logical lines: with the screen's scale the
    // same size as at 100 %)
    assert!((1.5 * size_factor(2160.0, 1.5, 1.0, true) - 2.0).abs() < 1e-5);
    assert_eq!(size_factor(1080.0, 1.0, 1.5, true), 1.5);
    assert_eq!(size_factor(2160.0, 1.0, 0.5, true), 1.0);
    // (switched off: the size alone)
    assert_eq!(size_factor(2160.0, 1.0, 1.0, false), 1.0);
    assert_eq!(size_factor(2160.0, 1.0, 1.25, false), 1.25);
}

/// A thinned panel's light texts get an outline; dark ones - the highlighted menu line's,
/// on the solid accent - do not (it came out as a smear round them, like a shadow).
#[test]
fn panel_texts_get_an_outline_only_where_it_helps() {
    // (full panels: as asked)
    assert_eq!(outline_for([235, 235, 235, 0], 1.0), 0);
    // (thinned: the light text gets one, the dark text none)
    assert!(outline_for([235, 235, 235, 0], 0.47) > 100);
    assert!(outline_for([140, 140, 140, 0], 0.47) > 100);
    assert_eq!(outline_for([20, 20, 20, 0], 0.47), 0);
    // (texts with an outline of their own keep it)
    assert_eq!(outline_for([255, 255, 255, 235], 0.47), 235);
}

#[test]
fn the_game_menu_grows_with_the_interface_size_as_far_as_it_fits() {
    // (it ignored the setting: 1.5 drew the menu as 1.0 while the HUD grew)
    assert_eq!(super::menu_scale_for(1.0, 1.0, 1280.0, 720.0, false), 1.0);
    let s = super::menu_scale_for(1.0, 1.5, 1280.0, 720.0, false);
    assert!(
        s > 1.1 && s < 1.5 && 880.0 * s + 24.0 <= 1280.0 && 600.0 * s <= 720.0 * 0.94 + 0.01,
        "{s}"
    );
    assert_eq!(super::menu_scale_for(1.0, 1.5, 3840.0, 2160.0, false), 1.5);
    // never smaller than it always was, and the headset keeps its own size
    assert_eq!(super::menu_scale_for(1.0, 1.5, 800.0, 500.0, false), 1.0);
    assert_eq!(super::menu_scale_for(1.0, 1.5, 1920.0, 1080.0, true), 1.0);
}

#[test]
fn destinations_split_into_code_and_name() {
    assert_eq!(
        super::code_and_name(" 13  BETRIEBSFAHRT"),
        Some(("13", "BETRIEBSFAHRT"))
    );
    assert_eq!(
        super::code_and_name("1001  Blanko.tga"),
        Some(("1001", "Blanko.tga"))
    );
    assert_eq!(super::code_and_name("Route number: 5..."), None);
    assert_eq!(super::code_and_name("Line 76  (1 tours)"), None);
}

#[test]
fn chat_filter_stars_out_swearing() {
    assert_ne!(
        filter_chat("you are a fucking idiot"),
        "you are a fucking idiot"
    );
    assert_eq!(
        filter_chat("next stop Rathaus Spandau"),
        "next stop Rathaus Spandau"
    );
}
