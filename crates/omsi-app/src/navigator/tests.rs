#[test]
fn centre_panel_map_click_and_drag_use_window_coordinates() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.origin_x = 1920.0;
    n.panel_rect = [1930.0, 600.0, 2290.0, 890.0];
    n.panel_room = [1240.0, 610.0];
    assert!(!n.over_panel(100.0, 700.0));
    assert!(n.over_panel(2020.0, 700.0));
    n.panel_press(2020.0, 700.0);
    n.panel_move(2630.0, 405.0);
    assert_eq!(n.at, Some([0.5, 0.5]));
    n.city.open = true;
    n.city.rect = [2100.0, 100.0, 3700.0, 900.0];
    n.city.center = DVec2::new(100.0, 200.0);
    n.city.mpp = 2.0;
    assert_eq!(n.map_point(2900.0, 500.0), Some(n.city.center));
    assert_eq!(n.map_point(1000.0, 500.0), None);
}
/// A press and a small wobble is a click (the city map), a longer move drags the
/// navigator, and the place reads back from the setting it is saved as (#940).
#[test]
fn the_navigator_is_dragged_by_the_mouse() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    assert!(n.at.is_none());
    n.panel_rect = [10.0, 600.0, 370.0, 890.0];
    n.panel_room = [1240.0, 610.0];
    n.panel_press(100.0, 700.0);
    assert!(!n.panel_move(103.0, 702.0));
    assert_eq!(n.panel_release(), Some(false));
    n.panel_press(100.0, 700.0);
    assert!(n.panel_move(100.0 + 610.0, 700.0 - 295.0));
    assert_eq!(n.panel_release(), Some(true));
    let at = n.at.unwrap();
    assert!((at[0] - 0.5).abs() < 1e-3 && (at[1] - 0.5).abs() < 1e-3, "{at:?}");
    let back = omsi_launcher_lib::nav_rect_parts(&n.take_rect().unwrap()).0.unwrap();
    assert!((back[0] - 0.5).abs() < 1e-3 && (back[1] - 0.5).abs() < 1e-3);
    assert_eq!(n.take_rect(), None, "handed over once");
    // (dragged past the window's edge: held inside it)
    n.panel_press(100.0, 700.0);
    n.panel_move(5000.0, -5000.0);
    assert_eq!(n.at, Some([1.0, 0.0]));
    assert_eq!(placed_at("bottom-right"), None);
}

/// With the on-screen controls the navigator stands in the top middle, under the
/// information bar when that is on - and where a finger dragged it, once it has been
/// (#1138), as the mouse places it on a computer; the bar does not move a navigator in a
/// corner it does not reach (#1164).
#[test]
fn the_navigator_keeps_where_it_was_dragged_on_a_phone_and_clear_of_the_information_bar() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let (screen, size) = ((1280.0, 720.0), (300.0, 250.0));
    assert_eq!(n.panel_origin(screen, size, true, None, None), (490.0, 13.0));
    assert_eq!(n.panel_origin(screen, size, true, Some([311.0, 20.0, 898.0, 69.0]), None), (490.0, 76.0));
    assert_eq!(n.panel_origin(screen, size, false, Some([311.0, 20.0, 898.0, 69.0]), None), (13.0, 457.0));
    n.corner = "top-left".into();
    assert_eq!(n.panel_origin(screen, size, false, Some([330.0, 20.0, 950.0, 69.0]), None), (13.0, 13.0));
    assert_eq!(n.panel_origin(screen, size, false, Some([300.0, 20.0, 980.0, 69.0]), None), (13.0, 76.0));
    // a finger's drag: there, on the phone as on the computer
    n.panel_rect = [490.0, 13.0, 790.0, 263.0];
    n.panel_press(600.0, 100.0);
    assert!(n.panel_move(600.0 - 490.0, 100.0 + 457.0));
    assert_eq!(n.panel_release(), Some(true));
    assert_eq!(n.panel_origin(screen, size, true, None, None), (0.0, 470.0));
    assert_eq!(n.panel_origin(screen, size, false, None, None), (0.0, 470.0));
}

use super::*;
use omsi_sim::traffic::Lane;

/// Over the duty sheet beside the city map the wheel scrolls its list and a press does
/// not drag the map; its button in the header puts it away.
#[test]
fn the_duty_sheet_takes_the_wheel_and_the_press() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.toggle_map();
    n.city.rect = [100.0, 50.0, 1100.0, 850.0];
    n.city.sheet = Rect::new(12.0, 56.0, 340.0, 700.0);
    n.city.sheet_scale = 1.0;
    n.city.sheet_scroll = 100.0;
    let mpp = n.city.mpp;
    n.map_wheel(1.0, 200.0, 300.0);
    assert_eq!((n.city.sheet_scroll, n.city.mpp), (52.0, mpp));
    n.map_wheel(5.0, 200.0, 300.0);
    assert_eq!(n.city.sheet_scroll, 0.0);
    n.map_press(200.0, 300.0);
    assert!(n.city.drag.is_none() && n.city.open);
    // (dragged with a finger, as a list on a phone)
    n.map_move(200.0, 260.0);
    assert_eq!((n.city.sheet_scroll, n.city.center), (40.0, DVec2::ZERO));
    n.map_release();
    n.map_press(800.0, 300.0);
    assert!(n.city.drag.is_some());
    n.map_release();
    n.city.buttons = vec![(Rect::new(900.0, 7.0, 30.0, 30.0), 4)];
    n.city.shown = Left::Duty;
    n.map_press(1015.0, 72.0);
    assert!(n.city.shown == Left::Nothing && n.city.drag.is_none());
    assert_eq!(left_page(n.city.left, n.city.stage, true), Left::Nothing, "put away");
}

/// The left column shows the sign-on page until the duty is signed, then the duty
/// sheet; a button chooses for as long as the stage is the same, a second press on it
/// puts the column away.
#[test]
fn the_left_column_follows_the_signing_on() {
    assert_eq!(left_page(None, Stage::SignOn, true), Left::SignOn);
    // (no duty to sign for, not signed on: the map - the page would cover all of it)
    assert_eq!(left_page(None, Stage::SignOn, false), Left::Nothing);
    assert_eq!(left_page(None, Stage::DutyMenu, false), Left::SignOn);
    assert_eq!(left_page(None, Stage::DutyOrder, true), Left::SignOn);
    assert_eq!(left_page(None, Stage::OnDuty, true), Left::Duty);
    assert_eq!(left_page(None, Stage::OnDuty, false), Left::Nothing, "a free drive: the whole map");
    let mut c = CityMap { stage: Stage::SignOn, shown: Left::SignOn, ..CityMap::default() };
    c.choose(Left::Duty);
    assert_eq!(left_page(c.left, Stage::SignOn, true), Left::Duty);
    // (no duty: no sheet)
    assert_eq!(left_page(c.left, Stage::SignOn, false), Left::Nothing);
    // signed on (here or on a phone): the choice was for the stage before
    assert_eq!(left_page(c.left, Stage::DutyOrder, true), Left::SignOn);
    c.shown = Left::SignOn;
    c.choose(Left::SignOn);
    assert_eq!((c.shown, left_page(c.left, Stage::SignOn, true)), (Left::Nothing, Left::Nothing));
    c.choose(Left::SignOn);
    assert_eq!(c.shown, Left::SignOn);
}

/// On the sign-on page a press works its key, as the keyboard's digits do while it asks
/// for the number; a press beside the page's scrolling part does nothing.
#[test]
fn the_sign_on_page_takes_presses_and_keys() {
    use crate::nav_signon::Action;
    use winit::keyboard::KeyCode;
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.toggle_map();
    n.city.rect = [100.0, 50.0, 1100.0, 850.0];
    n.city.sheet = Rect::new(12.0, 56.0, 340.0, 700.0);
    n.city.shown = Left::SignOn;
    n.city.phone_view = Rect::new(12.0, 122.0, 340.0, 626.0);
    n.city.phone_hits = vec![(Rect::new(28.0, 200.0, 98.0, 48.0), Action::Digit(4)), (Rect::new(28.0, 90.0, 98.0, 48.0), Action::Digit(9))];
    n.map_press(100.0 + 60.0, 50.0 + 220.0);
    assert_eq!(n.city.phone.typed, "4");
    assert!(n.city.drag.is_none());
    // (above the scrolling part: the key scrolled out of view under the head)
    n.map_press(100.0 + 60.0, 50.0 + 100.0);
    assert_eq!(n.city.phone.typed, "4");
    assert!(n.page_key(KeyCode::Digit8));
    assert!(n.page_key(KeyCode::Numpad2));
    assert!(n.page_key(KeyCode::Backspace));
    assert_eq!(n.city.phone.typed, "48");
    assert!(!n.page_key(KeyCode::KeyW), "not the keypad's");
    n.city.stage = Stage::OnDuty;
    assert!(!n.page_key(KeyCode::Digit1), "nothing to type at work");
    n.city.open = false;
    n.city.stage = Stage::SignOn;
    assert!(!n.page_key(KeyCode::Digit1), "the map is shut");
}

/// The small navigator as the sign-on page (while a duty waits to be signed for): a press
/// on its keypad's key works it - no click that opens the city map, no drag - and the
/// digit keys work it while it asks for the number or the code; elsewhere on it a press is
/// the navigator's as ever.
#[test]
fn the_small_navigator_signs_on_too() {
    use crate::nav_signon::{self as signon, Action, Room};
    use winit::keyboard::KeyCode;
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let fonts = Fonts::hanken();
    let st = crate::companion::CompanionState { personnel_number: "482913".into(), personnel_code: "5821".into(), ..Default::default() };
    assert!(signon::waiting(&st));
    let page = signon::page_in(&st, &n.city.phone, None, 0.0, Room::Panel, None);
    let fit = signon::fit_panel(page, 360.0, 1.0, 900.0, &fonts);
    let height = fit.height;
    let hits = signon::panel_hits(&fit, 0.0);
    let four = hits.iter().find(|h| h.1 == Action::Digit(4)).unwrap().0.center();
    n.signing = Some(PanelPage { fit, hits, stage: Stage::SignOn });
    n.panel_rect = [10.0, 300.0, 370.0, 300.0 + height];
    n.panel_room = [1240.0, 600.0];
    // a press on the 4: typed, and nothing else
    n.panel_press(10.0 + four.x, 300.0 + four.y);
    assert_eq!(n.city.phone.typed, "4");
    assert_eq!(n.panel_release(), None, "no click, no drag");
    assert!(!n.city.open);
    // the keys, as on the city map
    assert!(n.keypad_shown());
    assert!(n.page_key(KeyCode::Digit8) && n.page_key(KeyCode::Numpad2));
    assert_eq!(n.city.phone.typed, "482");
    assert!(!n.page_key(KeyCode::KeyW));
    // the head is no key: a click there opens the city map, as on the map
    n.panel_press(30.0, 310.0);
    assert_eq!(n.panel_release(), Some(false));
    // the duty order on it: no keypad for the keys
    n.signing.as_mut().unwrap().stage = Stage::DutyOrder;
    assert!(!n.page_key(KeyCode::Digit1));
    // the navigator off: none
    n.signing.as_mut().unwrap().stage = Stage::SignOn;
    n.enabled = false;
    assert!(!n.keypad_shown());
}

/// The small navigator's page is laid out every frame, the city map open or not: while the
/// map is open its page is the one scrolled, further than the small one's goes, and the
/// small one leaves that alone; shut, the small one keeps it in its own range again.
#[test]
fn the_small_page_leaves_the_city_maps_scroll_alone() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let st = crate::companion::CompanionState { personnel_number: "482913".into(), personnel_code: "5821".into(), ..Default::default() };
    n.qr = (qr_key(&st), 0.0, None);
    n.toggle_map();
    n.city.phone.scroll = 300.0;
    // (a tall window: the small page has nothing to scroll)
    let page = n.panel_page(&st, None, 0.0, 360.0, 1.0, 2000.0, false);
    assert_eq!(crate::nav_signon::panel_scroll_max(&page.fit), 0.0);
    assert_eq!(n.city.phone.scroll, 300.0, "the city map's page keeps its scroll");
    n.toggle_map();
    n.panel_page(&st, None, 0.0, 360.0, 1.0, 2000.0, false);
    assert_eq!(n.city.phone.scroll, 0.0);
}

/// Ctrl + the wheel over the small navigator or the city map sizes the navigator by the
/// setting's steps (a touchpad's parts of a notch add up), within its range, as do the
/// city map's - and +; the size is handed over once to be kept. Without Ctrl the wheel
/// over the small navigator is not the navigator's, and on the city map it zooms.
#[test]
fn ctrl_and_the_wheel_size_the_navigator() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.panel_rect = [10.0, 600.0, 370.0, 890.0];
    assert!(!n.wheel(1.0, 100.0, 700.0, false, true), "the wheel alone is the cab's");
    assert!(!n.wheel(1.0, 900.0, 700.0, true, true), "not over the navigator");
    assert!(!n.wheel(1.0, 100.0, 700.0, true, false), "not in VR's cab");
    assert!(n.wheel(2.0, 100.0, 700.0, true, true));
    assert!((n.size - 1.1).abs() < 1e-4, "{}", n.size);
    assert_eq!(n.take_resized().map(|v| (v * 100.0).round()), Some(110.0));
    assert_eq!(n.take_resized(), None, "handed over once");
    // a touchpad: parts of a notch
    for _ in 0..3 {
        n.wheel(-0.4, 100.0, 700.0, true, true);
    }
    assert!((n.size - 1.05).abs() < 1e-4, "{}", n.size);
    for _ in 0..40 {
        n.wheel(-1.0, 100.0, 700.0, true, true);
    }
    assert!((n.size - 0.6).abs() < 1e-4, "{}", n.size);
    // the city map: the wheel zooms, Ctrl + the wheel sizes; its - and +
    n.toggle_map();
    n.city.rect = [100.0, 50.0, 1100.0, 850.0];
    let mpp = n.city.mpp;
    n.take_resized();
    assert!(n.wheel(1.0, 800.0, 400.0, false, true) && n.city.mpp < mpp && n.take_resized().is_none());
    assert!(n.wheel(1.0, 800.0, 400.0, true, true) && (n.size - 0.65).abs() < 1e-4);
    n.city.buttons = vec![(Rect::new(500.0, 7.0, 30.0, 30.0), 7), (Rect::new(600.0, 7.0, 30.0, 30.0), 8)];
    n.map_press(100.0 + 615.0, 50.0 + 20.0);
    n.map_press(100.0 + 615.0, 50.0 + 20.0);
    assert!((n.size - 0.75).abs() < 1e-4 && n.city.drag.is_none());
    n.map_press(100.0 + 515.0, 50.0 + 20.0);
    assert!((n.size - 0.7).abs() < 1e-4);
    assert_eq!(n.take_resized().map(|v| (v * 100.0).round()), Some(70.0));
    assert_eq!(size_text(1.25), "125 %");
}

/// A frame for painting: the duty's line, no traffic, no duty board.
fn painting_frame() -> NavFrame<'static> {
    NavFrame {
        traffic: None,
        players: Vec::new(),
        bus: DVec3::ZERO,
        heading: 0.0,
        speed_kmh: 0.0,
        outside_temp: 15.0,
        inside_temp: 15.0,
        line: Some("307".into()),
        terminus: Some("Markgraf-Berthold-Platz".into()),
        stops: Vec::new(),
        delay: None,
        duty: None,
        passengers: None,
        stop_requested: false,
        time: 19.0 * 3600.0 + 5.0 * 60.0,
        weekday: 3,
        language: "NLD",
        screen: (1920.0, 1080.0),
        ui_scale: 1.0,
        follow_window: true,
        dt: 1.0 / 60.0,
        info_rect: None,
        touch_gap: None,
    }
}

/// The city map's whole view as the sign-on page, painted at the navigator's sizes, in a
/// wide window and a narrow one: everything within the window, the cards across it; the
/// header's buttons - closing, the map, the size's - and + - apart from each other and in
/// it; the page's keys where a press finds them.
#[test]
fn the_city_map_paints_the_sign_on_page_within_its_window() {
    use crate::nav_signon::tests::{hans, listening, made_up_qr};
    let st = listening(hans());
    let f = painting_frame();
    for (w, h) in [(1536.0f32, 886.0f32), (760.0, 620.0)] {
        for size in [0.6, 1.0, 2.0] {
            let mut n = Navigator::new(true, 0.85, "bottom-left");
            n.size = size;
            n.qr = (qr_key(&st), 0.0, Some(made_up_qr(29)));
            let s = (h / 760.0).clamp(0.95, 2.0) * size;
            n.atlas.begin_frame();
            let (bg, list, ui, body) = n.paint_city_signon(&f, &st, None, w, h, s);
            assert!(body.y > 0.0 && body.bottom() <= h + 0.01);
            for v in bg.verts.iter().chain(&ui.verts) {
                assert!(v.pos[0] >= -0.01 && v.pos[0] <= w + 0.01 && v.pos[1] >= -0.01 && v.pos[1] <= h + 0.01, "{v:?} at {size} in {w}x{h}");
            }
            assert!(list.verts.iter().all(|v| v.pos[0] >= 0.0 && v.pos[0] <= w), "the cards across the window");
            let mut ids: Vec<u8> = n.city.buttons.iter().map(|b| b.1).collect();
            ids.sort_unstable();
            assert_eq!(ids, [3, 6, 7, 8]);
            for (i, (a, _)) in n.city.buttons.iter().enumerate() {
                assert!(a.x >= 0.0 && a.right() <= w && a.bottom() <= body.y + 0.01);
                for (b, _) in &n.city.buttons[i + 1..] {
                    assert!(a.right() <= b.x + 0.01 || b.right() <= a.x + 0.01, "{a:?} on {b:?}");
                }
            }
            let five = n.city.phone_hits.iter().find(|h| h.1 == crate::nav_signon::Action::Digit(5)).map(|h| h.0);
            assert!(five.is_some_and(|r| body.contains(r.center()) || size > 1.5), "{five:?} at {size}");
        }
    }
}

/// Pictures of the navigator as the sign-on page, as the game paints it (with its header
/// and its size): the city map's whole view at 100 % and 130 %, and the small navigator,
/// in Dutch: `OMSI_NAV_SIGNON_PREVIEW=<folder> cargo test -p omsi-app --lib navigator --
/// --ignored`.
#[test]
#[ignore]
fn preview_sign_on_pictures() {
    use crate::nav_signon::tests::{hans, listening, made_up_qr};
    let Ok(dir) = std::env::var("OMSI_NAV_SIGNON_PREVIEW") else { return };
    crate::ui_language("NLD");
    let st = listening(hans());
    let f = painting_frame();
    let raster = crate::nav_duty::tests::raster;
    let all = Rect::new(0.0, 0.0, 1e5, 1e5);
    for (name, size) in [("city-signon", 1.0f32), ("city-signon-130", 1.3)] {
        let mut n = Navigator::new(true, 0.85, "bottom-left");
        n.atlas = Atlas::new(2048);
        n.size = size;
        n.qr = (qr_key(&st), 0.0, Some(made_up_qr(29)));
        n.city.phone.typed = "48".into();
        // (just made larger: the size lit in the header)
        n.size_at = 0.0;
        let (w, h) = (1536.0, 886.0);
        let s = (h / 760.0f32).clamp(0.95, 2.0) * size;
        n.atlas.begin_frame();
        let (bg, list, ui, body) = n.paint_city_signon(&f, &st, None, w, h, s);
        // (the game behind it: a grey)
        let mut img = image::RgbaImage::from_pixel(w as u32, h as u32, image::Rgba([70, 84, 96, 255]));
        raster(&bg.verts, &n.atlas, &mut img, all);
        raster(&list.verts, &n.atlas, &mut img, body);
        raster(&ui.verts, &n.atlas, &mut img, all);
        img.save(format!("{dir}/{name}.png")).unwrap();
    }
    // the small navigator: as the frame lays it out at a 1080p window
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.atlas = Atlas::new(2048);
    n.qr = (qr_key(&st), 0.0, Some(made_up_qr(29)));
    let pw = (1080.0f32 * 0.33).round();
    // (and in a short window, its page scrolled a little: the bar says so)
    for (name, room, scroll) in [("panel-live", 1080.0 - 2.0 * 19.0, 0.0), ("panel-short", 330.0, 40.0)] {
        n.city.phone.scroll = scroll;
        let page = n.panel_page(&st, None, 0.0, pw, pw / 360.0, room, false);
        let ph = page.fit.height.round();
        n.signing = Some(page);
        n.size_at = if scroll > 0.0 { f32::MIN } else { 0.0 };
        n.atlas.begin_frame();
        let (bg, ui, list, view) = n.paint_signon(pw, ph).unwrap();
        let mut img = image::RgbaImage::from_pixel(pw as u32 + 40, ph as u32 + 40, image::Rgba([70, 84, 96, 255]));
        let shift = |p: Painter| p.verts.into_iter().map(|mut v| { v.pos[0] += 20.0; v.pos[1] += 20.0; v }).collect::<Vec<_>>();
        raster(&shift(bg), &n.atlas, &mut img, all);
        raster(&shift(list), &n.atlas, &mut img, Rect::new(view.x + 20.0, view.y + 20.0, view.w, view.h));
        raster(&shift(ui), &n.atlas, &mut img, all);
        img.save(format!("{dir}/{name}.png")).unwrap();
    }
}

/// While the duty waits to be signed for the city map is the sign-on page: no map to drag
/// or to point at under it; its map button puts it away for the map (the badge brings it
/// back), until the stage changes.
#[test]
fn the_city_map_is_the_sign_on_page_until_the_duty_is_signed() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.toggle_map();
    n.city.rect = [100.0, 50.0, 1100.0, 850.0];
    n.city.stage = Stage::SignOn;
    n.city.shown = left_page(n.city.left, Stage::SignOn, true);
    assert!(n.full_view());
    n.city.sheet = Rect::new(0.0, 44.0, 1000.0, 756.0);
    n.city.buttons = vec![(Rect::new(950.0, 7.0, 30.0, 30.0), 3), (Rect::new(912.0, 7.0, 30.0, 30.0), 6)];
    assert_eq!(n.map_point(600.0, 400.0), None, "no map to put the bus on");
    // (the header beside its buttons: no drag of a map that is not there)
    n.map_press(100.0 + 400.0, 50.0 + 20.0);
    assert!(n.city.drag.is_none() && n.city.open);
    // the map button: the map, for as long as the stage is this one
    n.map_press(100.0 + 925.0, 50.0 + 20.0);
    assert_eq!(left_page(n.city.left, Stage::SignOn, true), Left::Nothing);
    assert_eq!(left_page(n.city.left, Stage::DutyOrder, true), Left::SignOn, "signed on: the duty order on the page");
    n.city.shown = Left::Nothing;
    assert!(!n.full_view());
    assert!(n.map_point(600.0, 400.0).is_some());
    // at work the page is the column again
    n.city.shown = Left::SignOn;
    n.city.stage = Stage::OnDuty;
    assert!(!n.full_view());
}

#[test]
fn stop_request_icon_reserves_space_only_while_active() {
    let mut atlas = Atlas::new(256);
    for scale in [0.75, 1.0, 2.0] {
        // Both the next-stop row and the free-drive row leave room for the symbol.
        for height in [20.0, 46.0] {
            let row = Rect::new(11.0 * scale, 300.0 * scale, 300.0 * scale, height * scale);
            for requested in [false, true, false] {
                let mut ui = Painter::new();
                let text = stop_request_icon(&mut ui, &mut atlas, requested, row, scale);
                assert_eq!((text.x, text.y, text.h), (row.x, row.y, row.h));
                if requested {
                    assert!(!ui.verts.is_empty(), "the stop sign must render");
                    assert!(ui.verts.iter().all(|v| v.color == STOP_REQUEST.0));
                    assert!(ui.verts.iter().all(|v| v.pos[0] > text.right() && v.pos[0] <= row.right()));
                    assert!((text.w - (row.w - 38.0 * scale)).abs() < 0.01);
                } else {
                    assert!(ui.verts.is_empty());
                    assert_eq!(text.w, row.w);
                }
            }
        }
    }
}

fn straight(a: (f64, f64), b: (f64, f64)) -> Lane {
    omsi_sim::traffic::LaneBuilder::polyline(vec![DVec3::new(a.0, a.1, 0.0), DVec3::new(b.0, b.1, 0.0)], LaneKind::Street, 3.0)
}

/// A square block: the route goes north then east; the bus turned west by mistake at
/// the first corner and must be led round the block back onto the route.
#[test]
fn a_way_back_joins_the_route_ahead() {
    let lanes = vec![
        straight((0.0, 0.0), (0.0, 100.0)),     // 0 north (route)
        straight((0.0, 100.0), (100.0, 100.0)), // 1 east (route)
        straight((100.0, 100.0), (200.0, 100.0)), // 2 east (route)
        straight((0.0, 100.0), (-100.0, 100.0)), // 3 west (the mistake)
        straight((-100.0, 100.0), (-100.0, 200.0)), // 4 north
        straight((-100.0, 200.0), (100.0, 200.0)),  // 5 east
        straight((100.0, 200.0), (100.0, 100.0)),   // 6 south, back to the route's corner
    ];
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    // (square corners: joined by hand, `link` wants a junction's curves)
    for (a, n) in [(0, vec![1, 3]), (1, vec![2]), (3, vec![4]), (4, vec![5]), (5, vec![6]), (6, vec![2])] {
        net.lanes[a].next = n;
    }
    let route = [0usize, 1, 2];
    let (path, join) = way_back(&net, DVec3::new(-40.0, 100.0, 0.0), 270.0, &route[1..], 6000.0).expect("a way");
    assert_eq!(path.first(), Some(&3));
    assert!(path.contains(&6), "{path:?}");
    // round the block back to the route's second corner: it goes on east on lane 2
    assert_eq!(route[1..][join], 2, "{path:?} join {join}");
    assert!(heading_vec(90.0).x > 0.99);
}

#[test]
fn projection_puts_the_look_at_point_in_the_middle() {
    let view = Mat4::look_at_rh(Vec3::new(0.0, -50.0, 80.0), Vec3::ZERO, Vec3::Z);
    let l = Layer::world(view, 0.7, [0.0, 0.0, 400.0, 300.0], [0.0; 4], 0.0, 1.0);
    let p = project(l.view_proj, [0.0, 0.0, 400.0, 300.0], Vec3::ZERO).unwrap();
    assert!((p - Vec2::new(200.0, 150.0)).length() < 0.5, "{p}");
    let ahead = project(l.view_proj, [0.0, 0.0, 400.0, 300.0], Vec3::new(0.0, 30.0, 0.0)).unwrap();
    assert!(ahead.y < 150.0, "ahead is up the picture: {ahead}");
}

#[test]
fn editor_only_lanes_do_not_become_gps_roads() {
    let visible = straight((0.0, 0.0), (100.0, 0.0));
    let mut hidden = straight((0.0, 30.0), (100.0, 30.0));
    hidden.invisible = true;
    let net = Network { lanes: vec![visible.clone(), hidden], ..Default::default() };
    let drawn = visible_road_lanes(&net);
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].1.points, visible.points);
}

#[test]
fn separate_asphalt_meshes_corroborate_invisible_traffic_splines() {
    let mut on_road = straight((0.0, 0.0), (100.0, 0.0));
    on_road.invisible = true;
    let mut helper = straight((0.0, 25.0), (100.0, 25.0));
    helper.invisible = true;
    let mut bridge = straight((0.0, 0.0), (100.0, 0.0));
    bridge.points.iter_mut().for_each(|p| p.z = 8.0);
    bridge.invisible = true;
    let mut net = Network { lanes: vec![on_road, helper, bridge], ..Default::default() };
    let surface = (vec![DVec3::ZERO, DVec3::new(100.0, 0.0, 0.0)], 8.0);
    confirm_road_surfaces(&mut net, &[surface.clone()]);
    assert!(!net.lanes[0].invisible);
    assert!(net.lanes[1].invisible);
    assert!(net.lanes[2].invisible, "asphalt below a bridge must not corroborate its paths");
    assert_eq!(road_geometry(&net).len(), 1, "surface evidence must not draw a second road");
}

#[test]
fn adjacent_spline_lanes_form_carriageways_without_filling_the_median() {
    let mut lanes = Vec::new();
    for (path, offset) in [-10.0f32, -7.0, 7.0, 10.0].into_iter().enumerate() {
        let mut lane = straight((offset as f64, 0.0), (offset as f64, 100.0));
        lane.source = 1;
        lane.key = Some(LaneKey { tile: (0, 0), id: 42, path: path as u16 });
        lane.offset = offset;
        if offset < 0.0 { lane.points.reverse(); lane.reversed = true; }
        lanes.push(lane);
    }
    let net = Network { lanes, ..Default::default() };
    let roads = road_geometry(&net);
    assert_eq!(roads.len(), 2);
    assert_eq!(roads[0].width, 6.0);
    assert_eq!(roads[1].width, 6.0);
    assert_eq!(roads[0].points, vec![DVec3::new(-8.5, 0.0, 0.0), DVec3::new(-8.5, 100.0, 0.0)]);
    assert_eq!(roads[1].points[0].x, 8.5);
    // Display geometry must not replace or move the graph used by routing.
    assert_eq!(net.lanes.len(), 4);
    assert_eq!(net.lanes[0].points[0].y, 100.0);
}

/// A two-lane street of paths narrower than its lanes (2.5 m paths, 4 m apart - Rheinhausen
/// and Bad Hügelsdorf have hundreds) is one carriageway as wide as its two lanes, not two
/// strips with the gap between their edges left out.
#[test]
fn narrow_paths_on_a_lanes_pitch_are_one_carriageway() {
    for (pitch, width) in [(4.0f32, 8.0f32), (3.5, 7.0)] {
        let mut lanes = Vec::new();
        for (path, offset) in [-pitch * 0.5, pitch * 0.5].into_iter().enumerate() {
            let mut lane = omsi_sim::traffic::LaneBuilder::polyline(vec![DVec3::new(offset as f64, 0.0, 0.0), DVec3::new(offset as f64, 100.0, 0.0)], LaneKind::Street, 2.5);
            lane.source = 1;
            lane.key = Some(LaneKey { tile: (0, 0), id: 7, path: path as u16 });
            lane.offset = offset;
            lanes.push(lane);
        }
        let roads = road_geometry(&Network { lanes, ..Default::default() });
        assert_eq!(roads.len(), 1, "a pitch of {pitch} m");
        assert!((roads[0].width - width).abs() < 1e-4, "{} for a pitch of {pitch} m", roads[0].width);
        assert!(roads[0].points[0].x.abs() < 1e-6);
    }
}

#[test]
fn opposite_directions_draw_once_even_when_reverse_is_first() {
    let mut a = straight((0.0, 0.0), (0.0, 100.0));
    a.key = Some(LaneKey { tile: (0, 0), id: 42, path: 0 });
    let mut b = a.clone();
    b.points.reverse();
    b.reversed = true;
    let net = Network { lanes: vec![b, a], ..Default::default() };
    assert_eq!(visible_road_lanes(&net).len(), 1);
}

#[test]
fn junction_helpers_between_roads_remain_connected() {
    let mut lanes = vec![straight((0.0, 0.0), (0.0, 40.0)), straight((0.0, 40.0), (0.0, 50.0)), straight((0.0, 50.0), (0.0, 60.0)), straight((0.0, 60.0), (0.0, 100.0)), straight((30.0, 0.0), (30.0, 100.0))];
    lanes[0].next = vec![1]; lanes[1].next = vec![2]; lanes[2].next = vec![3];
    for i in [1, 2, 4] { lanes[i].invisible = true; }
    let mut net = Network { lanes, ..Default::default() };
    confirm_road_surfaces(&mut net, &[]);
    assert!(!net.lanes[1].invisible && !net.lanes[2].invisible);
    assert!(net.lanes[4].invisible);
    assert_eq!(road_geometry(&net).len(), 4);
}

#[test]
fn placement_gaps_are_bridged_only_where_the_graph_connects() {
    let mut lanes = vec![straight((0.0, 0.0), (0.0, 40.0)), straight((0.0, 41.0), (0.0, 80.0)), straight((2.0, 41.0), (2.0, 80.0))];
    lanes[0].next = vec![1];
    let roads = road_geometry(&Network { lanes, ..Default::default() });
    assert_eq!(roads.len(), 4);
    assert_eq!(roads[3].points, vec![DVec3::new(0.0, 40.0, 0.0), DVec3::new(0.0, 41.0, 0.0)]);
}

#[test]
fn paved_areas_without_driving_paths_do_not_draw_streets() {
    let mut net = Network::default();
    confirm_road_surfaces(&mut net, &[(vec![DVec3::ZERO, DVec3::new(100.0, 0.0, 0.0)], 20.0)]);
    assert!(road_geometry(&net).is_empty());
}

#[test]
fn stacked_paths_on_one_spline_are_not_merged() {
    let mut a = straight((0.0, 0.0), (0.0, 100.0));
    a.source = 1;
    a.key = Some(LaneKey { tile: (0, 0), id: 42, path: 0 });
    let mut b = a.clone();
    b.key.as_mut().unwrap().path = 1;
    b.points.iter_mut().for_each(|p| p.z = 8.0);
    let roads = road_geometry(&Network { lanes: vec![a, b], ..Default::default() });
    assert_eq!(roads.len(), 2);
    assert_eq!(roads[0].points[0].z, 0.0);
    assert_eq!(roads[1].points[0].z, 8.0);
}

// --- the player's own pins (`nav_pins`) --------------------------------------------------

use crate::nav_pins::tests::{find, grid};
use crate::nav_pins::{CardHit, Note, Op, Pin, Role};

/// A frame with the bus at `bus` heading `heading`, no duty.
fn at(bus: (f64, f64), heading: f64) -> NavFrame<'static> {
    NavFrame { bus: DVec3::new(bus.0, bus.1, 0.0), heading, line: None, terminus: None, ..painting_frame() }
}

fn on_grid(n: &mut Navigator, blocks: i32) -> std::sync::Arc<Network> {
    let net = std::sync::Arc::new(grid(blocks));
    n.global = Some(net.clone());
    n.global_version = 1;
    net
}

/// A free drive: the pins make the route (the destination set first, a via before it, a
/// click far from any road no pin); the free drive's `clear_route` leaves it; the via is
/// passed and dropped, the destination reached, and a moment later route and pins are gone.
#[test]
fn a_free_drive_goes_through_the_pins_to_the_destination() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let net = on_grid(&mut n, 3);
    n.pins.ops = vec![Op::Add(DVec2::new(398.5, 300.0)), Op::Add(DVec2::new(100.0, 198.5)), Op::Add(DVec2::new(100.0, 100.0))];
    n.follow(&at((1.5, 20.0), 0.0));
    assert_eq!(n.pins.note.map(|x| x.0), Some(Note::NoRoad), "the middle of a block");
    assert_eq!(n.pins.list.len(), 2);
    assert_eq!((n.pins.role(0), n.pins.role(1)), (Role::Via(1), Role::Destination));
    assert_eq!(n.route.key, DEST_KEY);
    assert!(n.route.on_route && n.route.joined && n.route.lanes.len() > 3, "{:?}", n.route.lanes);
    assert!(n.route.end.is_some());
    n.pin_distances(&net);
    let d: Vec<f64> = n.pins.dist.iter().map(|d| d.unwrap()).collect();
    assert!(d[0] < d[1] && (d[0] - (180.0 + 100.0 + 1.5)).abs() < 5.0, "{d:?}");
    // the card and the header say so
    let card = n.pin_card(&at((1.5, 20.0), 0.0));
    assert!(!card.diversion && card.rows.len() == 2 && card.total.is_some_and(|t| t.0 == d[1]));
    assert!(n.pins_head().is_some_and(|h| h.0.starts_with(&n.pins.list[1].name)));
    // (a free drive's frame clears the route of no trip: not this one)
    n.clear_route();
    assert_eq!(n.route.key, DEST_KEY);
    // past the via: dropped, said
    n.follow(&at((100.0, 198.5), 90.0));
    assert_eq!(n.pins.note.map(|x| x.0), Some(Note::Reached(1)));
    assert_eq!(n.pins.list.len(), 1);
    assert!(n.route.on_route);
    // at the destination (on the other side of its street): arrived; a moment later all gone
    n.follow(&at((401.5, 290.0), 0.0));
    assert_eq!(n.pins.note.map(|x| x.0), Some(Note::Arrived));
    assert!(n.pins.arrived.is_some() && !n.route.lanes.is_empty());
    n.follow(&NavFrame { dt: crate::nav_pins::ARRIVED_FOR + 0.1, ..at((401.5, 295.0), 0.0) });
    assert!(n.pins.list.is_empty() && n.route.lanes.is_empty() && n.route.key.is_empty());
    // removing the last pin ends the drive too
    n.pins.ops = vec![Op::Add(DVec2::new(398.5, 300.0))];
    n.follow(&at((1.5, 20.0), 0.0));
    assert_eq!(n.route.key, DEST_KEY);
    n.pins.ops = vec![Op::Remove(0)];
    n.follow(&at((1.5, 20.0), 0.0));
    assert!(n.route.lanes.is_empty() && n.route.key.is_empty());
}

/// A destination no road leads to: said once, no route (and no "recalculated"), tried
/// again now and then without saying it again; the pin stays for the player to move.
#[test]
fn a_destination_no_way_leads_to_is_said_once() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let mut net = grid(2);
    net.lanes.push(crate::nav_pins::tests::lane((5000.0, 0.0), (5100.0, 0.0)));
    net.build_grid();
    n.global = Some(std::sync::Arc::new(net));
    n.global_version = 1;
    n.pins.ops = vec![Op::Add(DVec2::new(5050.0, 0.0))];
    let f = at((1.5, 20.0), 0.0);
    n.follow(&f);
    assert_eq!(n.pins.note.map(|x| x.0), Some(Note::NoWay));
    assert!(n.route.lanes.is_empty() && n.pins.list.len() == 1 && n.pins.marks == vec![None]);
    n.pins.note = None;
    for _ in 0..((REROUTE_EVERY * 2.0 / f.dt) as usize + 10) {
        n.follow(&f);
    }
    assert_eq!(n.pins.note, None, "not said again");
    assert!(n.route.lanes.is_empty() && n.route.note == 0.0);
    n.clear_route();
    assert_eq!(n.pins.list.len(), 1);
}

/// Off the pins' route the navigator plans again from the bus through the pins ahead.
#[test]
fn off_the_pins_route_it_is_planned_again_from_the_bus() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    on_grid(&mut n, 3);
    n.pins.ops = vec![Op::Add(DVec2::new(598.5, 500.0))];
    let f = at((1.5, 20.0), 0.0);
    n.follow(&f);
    let first = n.route.lanes.clone();
    // the bus went the wrong way: east along the map's southern edge
    let wrong = at((100.0, -1.5), 90.0);
    for _ in 0..(((OFF_ROUTE_AFTER + REROUTE_EVERY) / f.dt) as usize + 10) {
        n.follow(&wrong);
    }
    assert_ne!(n.route.lanes, first);
    assert!(n.route.on_route && n.route.note > 0.0, "recalculated");
    assert_eq!(n.pins.list.len(), 1, "the destination stays");
    assert_eq!(n.pins.marks.len(), 1);
}

/// On a duty the pins are a diversion: the trip's route key stays (the timetable is not
/// asked again), the route goes through the via and back onto the trip's; the timetable's
/// route given again leaves it; "Clear diversion" gives the trip's route back with the bus
/// on it; and a diversion ends with its trip.
#[test]
fn a_diversion_is_the_navigations_alone_and_ends_with_its_trip() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let net = on_grid(&mut n, 3);
    let route: Vec<usize> = (0..3).map(|j| find(&net, (1.5, j as f64 * 200.0), (1.5, (j + 1) as f64 * 200.0))).collect();
    let g = n.global_version + (1 << 40);
    n.set_route("0/trip", route.clone(), true, g);
    let f = at((1.5, 50.0), 0.0);
    n.follow(&f);
    assert!(n.route.on_route);
    n.pins.ops.push(Op::Add(DVec2::new(201.5, 300.0)));
    n.follow(&f);
    assert!(n.pins.diverts("0/trip"));
    assert_eq!(n.pins.role(0), Role::Via(1));
    assert_eq!(n.route.key, "0/trip");
    assert!(!n.wants_route("0/trip", 0), "the trip's route is not asked for again");
    let join = n.pins.join.expect("back onto the trip's route");
    assert_eq!(&n.route.lanes[join..], &route[2..]);
    assert_eq!(n.route.end, None);
    assert!(n.pin_card(&f).diversion);
    assert!(n.pins_head().is_some_and(|h| h.1 == crate::nav_duty::NOW));
    let diverted = n.route.lanes.clone();
    n.set_route("0/trip", route.clone(), true, g);
    assert_eq!(n.route.lanes, diverted, "the timetable's route again: the diversion stays");
    // cleared: the trip's route, the bus found on it
    n.pins.ops.push(Op::Clear);
    n.follow(&f);
    assert!(n.pins.list.is_empty() && !n.pins.diversion());
    assert_eq!((&n.route.lanes, n.route.progress, n.route.on_route), (&route, 0, true));
    // the via reached: on along the trip's route from where it rejoins
    n.pins.ops.push(Op::Add(DVec2::new(201.5, 300.0)));
    n.follow(&f);
    n.follow(&at((201.5, 295.0), 0.0));
    assert_eq!(n.pins.note.map(|x| x.0), Some(Note::Reached(1)));
    assert!(!n.pins.diversion() && n.route.lanes.ends_with(&route[2..]));
    // a diversion ends with its trip: the next trip, or none
    n.set_route("0/trip", route.clone(), true, g + 1);
    n.pins.ops.push(Op::Add(DVec2::new(201.5, 300.0)));
    n.follow(&f);
    assert!(n.pins.diverts("0/trip"));
    n.set_route("1/next", route.clone(), true, g);
    assert!(n.pins.list.is_empty() && n.route.lanes == route);
    n.pins.ops.push(Op::Add(DVec2::new(201.5, 300.0)));
    n.follow(&f);
    assert!(n.pins.diverts("1/next"));
    n.clear_route();
    assert!(n.pins.list.is_empty() && n.route.lanes.is_empty());
}

/// The navigation map coming during a diversion takes the route with it: the timetable's
/// route given again is the trip's route meanwhile (kept when no way leads through the
/// pins on the new network), and the way through them follows.
#[test]
fn a_diversion_has_the_trips_route_when_the_map_comes() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let net = on_grid(&mut n, 3);
    let route: Vec<usize> = (0..3).map(|j| find(&net, (1.5, j as f64 * 200.0), (1.5, (j + 1) as f64 * 200.0))).collect();
    let g = n.global_version + (1 << 40);
    n.set_route("0/trip", route.clone(), true, g);
    let f = at((1.5, 50.0), 0.0);
    n.follow(&f);
    n.pins.ops.push(Op::Add(DVec2::new(201.5, 300.0)));
    n.follow(&f);
    assert!(n.pins.diverts("0/trip"));
    // (as `frame` does when the map's network is there)
    n.route = Route { version: n.route.version + 1, ..Route::default() };
    n.pins.rest.clear();
    n.pins.marks.clear();
    n.pins.join = None;
    n.pins.dirty = true;
    assert!(n.wants_route("0/trip", 0));
    n.set_route("0/trip", route.clone(), true, g);
    assert_eq!(n.route.lanes, route, "the trip's route meanwhile");
    assert_eq!(n.pins.rest, route);
    assert!(n.pins.diverts("0/trip"));
    n.follow(&f);
    assert!(n.pins.join.is_some() && n.route.lanes != route, "through the via again");
}

/// The city map's pin tool: a click sets a pin (a drag of the map, a click on the header
/// none); a right-click sets one without the tool and takes the one under it away; a pin
/// is dragged by as far as the cursor went; the card's buttons work and it takes no drag
/// or wheel.
#[test]
fn pins_are_set_moved_and_taken_away_on_the_city_map() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.toggle_map();
    n.city.rect = [100.0, 50.0, 1100.0, 850.0];
    n.city.center = DVec2::ZERO;
    n.city.mpp = 1.0;
    n.city.sheet_scale = 1.0;
    n.city.buttons = vec![(Rect::new(900.0, 7.0, 30.0, 30.0), 9)];
    // the window's middle is the map's centre
    n.map_press(600.0, 450.0);
    n.map_release();
    assert!(n.pins.ops.is_empty() && !n.city.pin_tool, "no tool: a drag");
    n.map_press(100.0 + 915.0, 50.0 + 20.0);
    assert!(n.city.pin_tool);
    n.map_press(600.0, 450.0);
    n.map_release();
    assert_eq!(n.pins.ops, vec![Op::Add(DVec2::ZERO)]);
    n.pins.ops.clear();
    n.map_press(600.0, 450.0);
    n.map_move(640.0, 450.0);
    n.map_release();
    assert!(n.pins.ops.is_empty(), "a drag of the map");
    n.city.center = DVec2::ZERO;
    n.map_press(400.0, 60.0);
    n.map_release();
    assert!(n.pins.ops.is_empty(), "the header");
    // a right-click, without the tool
    n.city.pin_tool = false;
    n.map_right_press(700.0, 450.0);
    assert_eq!(n.pins.ops, vec![Op::Add(DVec2::new(100.0, 0.0))]);
    n.pins.ops.clear();
    n.map_right_press(400.0, 60.0);
    assert!(n.pins.ops.is_empty(), "the header");
    n.pins.list = vec![Pin { at: DVec2::new(100.0, 0.0), name: "Markt".into(), n: 1 }];
    n.map_right_press(702.0, 452.0);
    assert_eq!(n.pins.ops, vec![Op::Remove(0)]);
    n.pins.ops.clear();
    // dragged by its flag's head: moved as far as the cursor went
    let head = crate::nav_pins::destination_head(Vec2::new(600.0, 400.0), crate::nav_pins::DEST_R);
    n.map_press(100.0 + head.x, 50.0 + head.y);
    assert!(n.city.drag.is_none() && n.city.pin_drag.is_some());
    n.map_move(100.0 + head.x + 50.0, 50.0 + head.y - 20.0);
    n.map_release();
    assert_eq!(n.pins.ops, vec![Op::Move(0, DVec2::new(150.0, 20.0), DVec2::new(100.0, 0.0))]);
    assert_eq!(n.pins.list[0].at, DVec2::new(150.0, 20.0), "where it was let go, until snapped");
    n.pins.ops.clear();
    // a press on a pin without moving: nothing
    n.map_press(100.0 + 650.0, 50.0 + 380.0);
    n.map_release();
    assert!(n.pins.ops.is_empty());
    // the card
    n.city.card = Rect::new(12.0, 56.0, 340.0, 200.0);
    n.city.card_hits = vec![(Rect::new(300.0, 120.0, 26.0, 26.0), CardHit::Remove(0)), (Rect::new(270.0, 120.0, 26.0, 26.0), CardHit::Up(1)), (Rect::new(26.0, 210.0, 300.0, 32.0), CardHit::Clear)];
    n.map_press(100.0 + 310.0, 50.0 + 130.0);
    n.map_press(100.0 + 280.0, 50.0 + 130.0);
    n.map_press(100.0 + 50.0, 50.0 + 220.0);
    n.map_press(100.0 + 50.0, 50.0 + 70.0);
    assert_eq!(n.pins.ops, vec![Op::Remove(0), Op::Shift(1, true), Op::Clear]);
    assert!(n.city.drag.is_none());
    let mpp = n.city.mpp;
    n.map_wheel(2.0, 100.0 + 50.0, 50.0 + 70.0);
    assert_eq!(n.city.mpp, mpp);
    // the map shut: the tool too
    n.city.pin_tool = true;
    n.toggle_map();
    assert!(!n.city.pin_tool);
}

/// The small navigator's bottom bar says where the pins lead - the destination with how
/// far, how long and when there; a diversion and its via beside the next stop - within it.
#[test]
fn the_bottom_bar_says_where_the_pins_lead() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    on_grid(&mut n, 3);
    n.pins.ops = vec![Op::Add(DVec2::new(398.5, 300.0)), Op::Add(DVec2::new(100.0, 198.5))];
    let f = at((1.5, 20.0), 0.0);
    n.follow(&f);
    n.pin_distances(n.global.clone().unwrap().as_ref());
    for s in [0.9f32, 1.0, 2.0] {
        let bottom = Rect::new(0.0, 200.0, 360.0 * s, 46.0 * s);
        let mut ui = Painter::new();
        n.paint_next(&mut ui, &f, bottom, s, &[]);
        // (the bar, the flag, the name, the way there and the via: a quad each)
        assert_eq!(ui.len(), 5 * 6);
        // (a letter's sprite has a little room round it)
        let e = 2.0 * s;
        for v in &ui.verts {
            assert!(v.pos[0] >= -e && v.pos[0] <= bottom.right() + e && v.pos[1] >= bottom.y - e && v.pos[1] <= bottom.bottom() + e, "{v:?} at {s}");
        }
    }
}

/// The map's world vertices (relative to `anchor`) as the city map's flat view shows them:
/// the pixels of a window `w` x `h` centred on `center`, `mpp` metres a pixel.
fn flatten(verts: &[omsi_ui::Vertex], anchor: DVec2, center: DVec2, mpp: f64, w: f32, h: f32) -> Vec<omsi_ui::Vertex> {
    verts
        .iter()
        .map(|v| {
            let p = DVec2::new(v.pos[0] as f64, v.pos[1] as f64) + anchor - center;
            let pos = [(w as f64 * 0.5 + p.x / mpp) as f32, (h as f64 * 0.5 - p.y / mpp) as f32, 0.0];
            omsi_ui::Vertex { pos, ext: [v.ext[0], -v.ext[1]], width: [(v.width[0] / mpp as f32).max(v.width[1]), 0.0], mode: [0.0, v.mode[1]], ..*v }
        })
        .collect()
}

/// Pictures of the pins as the game paints them, in Dutch:
/// `OMSI_NAV_PINS_PREVIEW=<folder> cargo test -p omsi-app --lib navigator -- --ignored`.
/// The city map on a free drive (the pin tool on, a via and a destination, the card in the
/// left column, a pin being dragged), on a duty with a diversion (the duty sheet in the
/// column, the card at the right edge, a note), and the small navigator's bottom bar.
#[test]
#[ignore]
fn preview_pin_pictures() {
    let Ok(dir) = std::env::var("OMSI_NAV_PINS_PREVIEW") else { return };
    crate::ui_language("NLD");
    let raster = crate::nav_duty::tests::raster;
    let all = Rect::new(0.0, 0.0, 1e5, 1e5);
    let (w, h) = (1536.0f32, 886.0f32);
    let s = (h / 760.0f32).clamp(0.95, 2.0);
    let signs = vec![(DVec3::new(10.0, 300.0, 0.0), 90.0, "Hauptstraße".to_string()), (DVec3::new(300.0, 190.0, 0.0), 0.0, "Kirchweg".to_string()), (DVec3::new(410.0, 500.0, 0.0), 90.0, "Parkallee".to_string()), (DVec3::new(500.0, 410.0, 0.0), 0.0, "Schulstraße".to_string())];
    let picture = |n: &mut Navigator, f: &NavFrame, duty: Option<&crate::nav_duty::DutyState>, left: Left, name: &str| {
        // (the roads and the route made again: the game keeps them on the GPU)
        n.city.roads = None;
        n.city.route.0 = (u64::MAX, 0, 0, 0);
        n.atlas.begin_frame();
        let companion = crate::nav_signon::tests::listening(crate::nav_signon::tests::hans());
        let p = n.paint_city(f, &companion, duty, left, w, h, s);
        let mut img = image::RgbaImage::from_pixel(w as u32, h as u32, image::Rgba([70, 84, 96, 255]));
        let (c, mpp) = (n.city.center, n.city.mpp);
        raster(&p.bg.verts, &n.atlas, &mut img, all);
        raster(&flatten(&p.roads.unwrap_or_default(), p.anchor, c, mpp, w, h), &n.atlas, &mut img, all);
        raster(&flatten(&p.route.unwrap_or_default(), p.anchor, c, mpp, w, h), &n.atlas, &mut img, all);
        raster(&p.ui.verts, &n.atlas, &mut img, all);
        raster(&p.list.verts, &n.atlas, &mut img, Rect::new(p.list_clip[0], p.list_clip[1], p.list_clip[2] - p.list_clip[0], p.list_clip[3] - p.list_clip[1]));
        img.save(format!("{dir}/{name}.png")).unwrap();
    };
    // a free drive
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.atlas = Atlas::new(2048);
    let net = on_grid(&mut n, 3);
    n.streets = Some(std::sync::Arc::new(build_streets(&net, &signs)));
    n.stop_names = Some(vec![(7, "Rathaus".to_string())]);
    n.stop_pos = std::sync::Arc::new([(7, DVec3::new(412.0, 310.0, 0.0))].into_iter().collect());
    n.toggle_map();
    n.city.rect = [0.0, 0.0, w, h];
    n.city.mpp = 0.85;
    n.city.follow = false;
    n.city.center = DVec2::new(122.0, 290.0);
    n.city.pin_tool = true;
    n.speed_avg = 9.0;
    let f = NavFrame { speed_kmh: 32.0, ..at((1.5, 60.0), 0.0) };
    n.pins.ops = vec![Op::Add(DVec2::new(398.5, 300.0)), Op::Add(DVec2::new(100.0, 198.5)), Op::Add(DVec2::new(300.0, 401.5))];
    n.follow(&f);
    n.pin_distances(&net);
    picture(&mut n, &f, None, Left::Nothing, "pins-free");
    // the same, a via being dragged and a note
    n.city.pin_drag = Some(PinDrag { k: 1, from: Vec2::new(500.0, 400.0), at: Vec2::new(560.0, 360.0), moved: true });
    n.pins.say(Note::NoRoad);
    picture(&mut n, &f, None, Left::Nothing, "pins-free-drag");
    n.city.pin_drag = None;
    // the small navigator's bottom bar: on the way, a via reached, arrived; a diversion
    let bars = |n: &mut Navigator, f: &NavFrame, img: &mut image::RgbaImage, y: f32| {
        n.atlas.begin_frame();
        let sb = 1.6f32;
        let mut ui = Painter::new();
        n.paint_next(&mut ui, f, Rect::new(20.0, y, 360.0 * sb, 46.0 * sb), sb, &[]);
        raster(&ui.verts, &n.atlas, img, all);
    };
    let mut img = image::RgbaImage::from_pixel(620, 4 * 90 + 20, image::Rgba([70, 84, 96, 255]));
    n.pins.note = None;
    bars(&mut n, &f, &mut img, 20.0);
    n.pins.say(Note::Reached(1));
    bars(&mut n, &f, &mut img, 110.0);
    n.pins.note = None;
    n.pins.arrived = Some(3.0);
    n.pins.say(Note::Arrived);
    bars(&mut n, &f, &mut img, 200.0);
    // on a duty, diverted
    let mut d = Navigator::new(true, 0.85, "bottom-left");
    d.atlas = Atlas::new(2048);
    let net = on_grid(&mut d, 3);
    d.streets = Some(std::sync::Arc::new(build_streets(&net, &signs)));
    let route: Vec<usize> = (0..3).map(|j| find(&net, (1.5, j as f64 * 200.0), (1.5, (j + 1) as f64 * 200.0))).collect();
    d.set_route("0/trip", route, true, d.global_version + (1 << 40));
    let stop = |name: &str, y: f64, min: f64| NavStop { object_id: 0, position: DVec3::new(4.0, y, 0.0), name: name.into(), arrival: 11.0 * 3600.0 + min * 60.0 };
    let f = NavFrame { line: Some("35".into()), terminus: Some("Diakonissenkrankenhaus".into()), stops: vec![stop("Kirchweg", 300.0, 32.0), stop("Rathaus", 450.0, 35.0), stop("Schule", 590.0, 38.0)], delay: Some(40.0), time: 11.0 * 3600.0 + 30.0 * 60.0, ..at((1.5, 60.0), 0.0) };
    d.follow(&f);
    d.pins.ops = vec![Op::Add(DVec2::new(201.5, 300.0)), Op::Add(DVec2::new(398.5, 450.0))];
    d.follow(&f);
    d.pin_distances(&net);
    d.toggle_map();
    d.city.rect = [0.0, 0.0, w, h];
    d.city.mpp = 0.85;
    d.city.follow = false;
    d.city.center = DVec2::new(285.0, 290.0);
    let (trips, tours) = crate::nav_duty::tests::duty();
    let state = crate::nav_duty::tests::state(&trips, &tours, 0, 3, false, 40.0);
    picture(&mut d, &f, Some(&state), Left::Duty, "pins-diversion");
    bars(&mut d, &f, &mut img, 290.0);
    img.save(format!("{dir}/pins-bar.png")).unwrap();
}

/// The map's world vertices as the GPU's shader places them (`ui.wgsl`): through the
/// layer's view, at least their pixel width at their depth - as pixels, for `raster`.
fn on_screen(verts: &[omsi_ui::Vertex], l: &Layer) -> Vec<omsi_ui::Vertex> {
    let px = |p: Vec3| {
        let c = l.view_proj * p.extend(1.0);
        // (the GPU clips at the near plane, a metre from the eye: no triangle across it)
        (c.w > 0.999).then(|| Vec2::new(l.viewport[0] + (c.x / c.w * 0.5 + 0.5) * l.viewport[2], l.viewport[1] + (0.5 - c.y / c.w * 0.5) * l.viewport[3]))
    };
    let mut out = Vec::with_capacity(verts.len());
    for t in verts.chunks_exact(3) {
        let pts: Vec<Option<Vec2>> = t
            .iter()
            .map(|v| {
                let p = Vec3::from_array(v.pos);
                let c = l.view_proj * p.extend(1.0);
                let mpp = c.w.max(0.01) * l.px_scale;
                let w = v.width[0].max(v.width[1] * mpp);
                px(p + Vec3::new(v.ext[0] * w, v.ext[1] * w, 0.0))
            })
            .collect();
        if pts.iter().all(|p| p.is_some()) {
            for (v, p) in t.iter().zip(pts) {
                let p = p.unwrap();
                out.push(omsi_ui::Vertex { pos: [p.x, p.y, 0.0], ext: [0.0, 0.0], width: [0.0, 0.0], mode: [0.0, v.mode[1]], ..*v });
            }
        }
    }
    out
}

/// Some traffic round the bus on the grid's street (x 1.5, heading north): buses of each
/// kind with their lines (one of the duty's own), an articulated one, a tram and cars.
fn traffic_round() -> Vec<MapVehicle> {
    let v = |x: f64, y: f64, heading: f64, len: f32, width: f32, color: Color, line: Option<&str>| MapVehicle { at: DVec3::new(x, y, 0.0), heading, len, width, color, bus: color != DOT, line: line.map(str::to_string) };
    vec![
        v(1.5, 112.0, 0.0, 12.0, 2.55, BUS, Some("314")),
        v(-1.5, 150.0, 180.0, 18.0, 2.55, BUS, Some("35")),
        v(60.0, 198.5, 90.0, 12.0, 2.55, TROLLEY, Some("1")),
        v(150.0, 201.5, 270.0, 30.0, 2.4, TRAM, Some("8")),
        v(1.5, 88.0, 0.0, 4.4, 1.8, DOT, None),
        v(-1.5, 128.0, 180.0, 4.6, 1.85, DOT, None),
        v(-1.5, 236.0, 180.0, 4.2, 1.75, DOT, None),
        v(28.0, 201.5, 270.0, 4.4, 1.8, DOT, None),
        v(105.0, 198.5, 90.0, 4.8, 1.9, DOT, None),
        v(1.5, 262.0, 30.0, 4.4, 1.8, DOT, None),
    ]
}

/// Pictures of the small navigator as the game paints it - in its own size, and dragged to
/// a narrow tall shape, a wide short one, a square, the smallest and a huge one, each with
/// the duty board and without - with the other vehicles as shapes, and far out as dots; and
/// the vehicles' shapes at the city map's zooms. In Dutch:
/// `OMSI_NAV_PANEL_PREVIEW=<folder> cargo test -p omsi-app --lib navigator -- --ignored`.
#[test]
#[ignore]
fn preview_panel_shapes() {
    use crate::nav_panel as np;
    let Ok(dir) = std::env::var("OMSI_NAV_PANEL_PREVIEW") else { return };
    crate::ui_language("NLD");
    let raster = crate::nav_duty::tests::raster;
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.atlas = Atlas::new(2048);
    let net = on_grid(&mut n, 3);
    let signs = vec![(DVec3::new(10.0, 300.0, 0.0), 90.0, "Hauptstraße".to_string()), (DVec3::new(300.0, 190.0, 0.0), 0.0, "Kirchweg".to_string())];
    n.streets = Some(std::sync::Arc::new(build_streets(&net, &signs)));
    let route: Vec<usize> = (0..3).map(|j| find(&net, (1.5, j as f64 * 200.0), (1.5, (j + 1) as f64 * 200.0))).collect();
    n.set_route("0/trip", route, true, n.global_version + (1 << 40));
    let stop = |name: &str, y: f64, min: f64| NavStop { object_id: 0, position: DVec3::new(4.0, y, 0.0), name: name.into(), arrival: 7.0 * 3600.0 + min * 60.0 };
    let f = NavFrame {
        line: Some("314".into()),
        terminus: Some("Großweier".into()),
        stops: vec![stop("Stadttheater", 300.0, 52.0), stop("Altstadtring", 450.0, 53.0), stop("Alter Bahnhof - Altstadtforum", 590.0, 55.0)],
        delay: Some(20.0),
        speed_kmh: 34.0,
        time: 7.0 * 3600.0 + 51.0 * 60.0,
        ..at((1.5, 60.0), 0.0)
    };
    n.follow(&f);
    n.follow(&f);
    n.speed_avg = 9.0;
    n.next_dist = Some(160.0);
    // (as the camera stands at 34 km/h)
    n.zoom = 110.0 + 34.0 * 2.2;
    let (trips, tours) = crate::nav_duty::tests::duty();
    let duty = crate::nav_duty::tests::state(&trips, &tours, 0, 3, false, 20.0);
    let vehicles = traffic_round();
    let unit = 1.0;
    // (the panel painted as the game does, flattened on the CPU over a cab-grey ground)
    let paint = |n: &mut Navigator, img: &mut image::RgbaImage, x0: f32, y0: f32, w: f32, h: f32, board_on: bool, auto: bool| {
        n.roads = None;
        n.route_mesh.0 = u64::MAX;
        let lay = if auto {
            let s = w / np::WIDTH;
            let (rows, bh) = if board_on { crate::nav_duty::board_fitting(Some(&duty), s, 1e4) } else { (Vec::new(), 0.0) };
            let ph = ((w * 0.62).round() + (np::HEAD * s).round() + (np::NEXT * s).round() + bh).round();
            (np::layout(w, ph, s, board_on.then_some(bh), false, true), rows)
        } else {
            let (s, beside) = np::scale(w, h, unit, board_on);
            let (rows, bh) = if board_on { crate::nav_duty::board_within(Some(&duty), s, np::board_room(h, s, beside)) } else { (Vec::new(), 0.0) };
            (np::layout(w, h, s, (!rows.is_empty()).then_some(bh), beside, true), rows)
        };
        let (lay, rows) = lay;
        let h = if auto { lay.next.bottom() + lay.board.map(|b| b.h).unwrap_or(0.0) } else { h };
        n.schedule = board_on;
        let p = n.paint_panel((w, h), &lay, &f, &rows, &vehicles);
        let mut panel = image::RgbaImage::from_pixel(w as u32, h as u32, image::Rgba([70, 84, 96, 255]));
        let all = Rect::new(0.0, 0.0, w, h);
        raster(&p.bg.verts, &n.atlas, &mut panel, all);
        let map = lay.map;
        for world in [p.roads.unwrap_or_default(), p.traffic.verts, p.route.unwrap_or_default(), p.vehicles.verts] {
            raster(&on_screen(&world, &p.map_layer), &n.atlas, &mut panel, map);
        }
        raster(&p.ui.verts, &n.atlas, &mut panel, all);
        image::imageops::overlay(img, &panel, x0 as i64, y0 as i64);
        h
    };
    let shapes: [(&str, f32, f32); 6] = [("own-size", 356.0, 0.0), ("narrow-tall", 280.0, 900.0), ("wide-short", 1100.0, 330.0), ("square", 520.0, 520.0), ("smallest", 220.0, 150.0), ("huge", 1000.0, 1000.0)];
    for (name, w, h) in shapes {
        let auto = h == 0.0;
        let tall = if auto { 900.0 } else { h };
        let mut img = image::RgbaImage::from_pixel((2.0 * w + 60.0) as u32, (tall + 40.0) as u32, image::Rgba([40, 46, 54, 255]));
        for (k, board_on) in [true, false].into_iter().enumerate() {
            paint(&mut n, &mut img, 20.0 + k as f32 * (w + 20.0), 20.0, w, h, board_on, auto);
        }
        img.save(format!("{dir}/panel-{name}.png")).unwrap();
    }
    // far out: the vehicles as dots with their tips
    n.zoom = 900.0;
    let mut img = image::RgbaImage::from_pixel(560, 560, image::Rgba([40, 46, 54, 255]));
    paint(&mut n, &mut img, 20.0, 20.0, 520.0, 520.0, false, false);
    img.save(format!("{dir}/panel-far.png")).unwrap();
    // the shapes themselves at the city map's zooms (north up, metres a pixel), each in a
    // row of its own: 70 px apart, as they head
    let (cw, ch) = (760.0f32, 150.0f32);
    let mpps = [0.08f64, 0.2, 0.6, 1.6, 4.0];
    let mut img = image::RgbaImage::from_pixel(cw as u32, (ch * mpps.len() as f32) as u32, image::Rgba([12, 16, 26, 255]));
    for (k, mpp) in mpps.into_iter().enumerate() {
        let (hw, hh) = (cw as f64 * 0.5 * mpp, ch as f64 * 0.5 * mpp);
        let proj = Mat4::orthographic_rh(-hw as f32, hw as f32, -hh as f32, hh as f32, -1000.0, 1000.0);
        let row = Rect::new(0.0, k as f32 * ch, cw, ch);
        let layer = Layer { view_proj: proj, viewport: [row.x, row.y, row.w, row.h], clip: [0.0; 4], radius: 0.0, opacity: 1.0, px_scale: mpp as f32 };
        let row_of: Vec<MapVehicle> = vehicles.iter().enumerate().map(|(j, v)| MapVehicle { at: DVec3::new((-(cw as f64) * 0.5 + 50.0 + j as f64 * 72.0) * mpp, 0.0, 0.0), ..v.clone() }).collect();
        let mut p = Painter::new();
        paint_vehicles(&mut p, &row_of, &|q: DVec3| Vec3::new(q.x as f32, q.y as f32, 0.0), mpp as f32, 1.2);
        raster(&on_screen(&p.verts, &layer), &n.atlas, &mut img, row);
    }
    img.save(format!("{dir}/vehicles.png")).unwrap();
}

/// The board's handle shows and hides the board and is kept; an edge of the panel sizes
/// it (the place following when the top or left edge goes), a corner both ways, held to
/// the smallest size and the window; the place and size are kept as `nav_rect` and read
/// back; Ctrl + the wheel leaves a sized panel's size; the reset forgets it all.
#[test]
fn the_navigator_is_sized_by_its_edges_and_its_board_switched() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.window = [1920.0, 1080.0];
    n.unit = 1.0;
    n.panel_rect = [20.0, 500.0, 380.0, 1060.0];
    n.panel_room = [1560.0, 520.0];
    n.layout = Some(crate::nav_panel::layout(360.0, 560.0, 1.0, Some(240.0), false, true));
    // the handle: the board away and back, kept each time; no click, no drag
    let hd = n.layout.unwrap().handle.unwrap();
    let (hx, hy) = (20.0 + hd.center().x, 500.0 + hd.center().y);
    assert_eq!(n.hover(hx, hy), Some(1));
    n.schedule = true;
    n.panel_press(hx, hy);
    assert_eq!(n.panel_release(), None);
    assert!(!n.schedule && !n.board);
    assert_eq!(n.take_board(), Some(false));
    n.panel_press(hx, hy);
    assert!(n.schedule && n.board && n.take_board() == Some(true) && n.take_board().is_none());
    // the right edge: wider, the place where it was
    assert_eq!(n.hover(378.0, 800.0), Some(5));
    n.panel_press(378.0, 800.0);
    assert!(n.panel_move(478.0, 800.0));
    let c = n.custom.unwrap();
    assert!((c[0] * 1080.0 - 460.0).abs() < 0.5 && (c[1] * 1080.0 - 560.0).abs() < 0.5, "{c:?}");
    assert_eq!(n.panel_release(), Some(true));
    let kept = n.take_rect().unwrap();
    let (at, size) = omsi_launcher_lib::nav_rect_parts(&kept);
    assert!(size.is_some_and(|z| (z[0] * 1080.0 - 460.0).abs() < 0.5));
    let p = crate::nav_panel::place([at.unwrap()[0] as f32, at.unwrap()[1] as f32], 460.0, 560.0, [1920.0, 1080.0]);
    assert!((p.x - 20.0).abs() < 0.6 && (p.y - 500.0).abs() < 0.6, "{p}");
    // the top-left corner, far past the smallest size: held there, the bottom right staying
    n.panel_rect = [20.0, 500.0, 480.0, 1060.0];
    n.panel_press(22.0, 502.0);
    n.panel_move(2000.0, 2000.0);
    let c = n.custom.unwrap();
    assert!((c[0] * 1080.0 - 220.0).abs() < 0.5 && (c[1] * 1080.0 - 150.0).abs() < 0.5, "{c:?}");
    n.panel_release();
    // read back as the game starts
    let mut m = Navigator::new(true, 0.85, "bottom-left");
    m.set_rect(&n.take_rect().unwrap());
    assert!(m.custom.is_some() && m.at.is_some());
    // Ctrl + the wheel: a sized panel keeps its size (its texts follow the size)
    let before = n.custom.unwrap();
    n.resize_by(2);
    assert!(n.custom == Some(before) && n.take_rect().is_none() && n.take_resized().is_some());
    // the middle of it still moves it, the cockpit's display is only moved
    assert_eq!(n.hover(300.0, 700.0), Some(0));
    n.cockpit_display = true;
    assert_eq!(n.hover(378.0, 800.0), Some(0));
    n.cockpit_display = false;
    n.reset_panel();
    assert!(n.custom.is_none() && n.at.is_none() && n.board && n.size == 1.0);
    assert_eq!(n.rect_setting(), "");
}

#[test]
fn crowded_stop_markers_and_labels_do_not_overlap() {
    let p = Vec2::new(200.0, 200.0);
    let markers = spaced_markers([(0, p), (1, p + Vec2::X * 5.0), (2, p + Vec2::X * 35.0)], 20.0);
    assert_eq!(markers.iter().map(|m| m.0).collect::<Vec<_>>(), vec![0, 2]);
    let win = Rect::new(0.0, 0.0, 600.0, 400.0);
    let first = stop_label_rect(p, 150.0, 1.0, win, &[]).unwrap();
    let second = stop_label_rect(p + Vec2::X * 35.0, 150.0, 1.0, win, &[first]).unwrap();
    assert!(!rects_overlap(&first, &second));
    let edge = stop_label_rect(Vec2::new(590.0, 200.0), 150.0, 1.0, win, &[]).unwrap();
    assert!(edge.right() < 600.0);
    assert!(stop_label_rect(p, 150.0, 1.0, win, &[win]).is_none());
}
