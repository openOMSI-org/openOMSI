use super::*;

#[test]
fn pause_list_title_subtitle_and_body_do_not_overlap() {
    let area = Rect::new(20.0, 30.0, 420.0, 600.0);
    let (title, subtitle, body_top) = list_header_layout(area);
    assert!(title.x >= area.x + 36.0);
    assert!(title.bottom() <= subtitle.y);
    assert!(subtitle.bottom() <= body_top);
}

#[test]
fn player_count_badge_follows_the_title_without_overflowing() {
    let badge = count_badge_rect(120.0, 28.0, 100.0, 400.0, 10.0).unwrap();
    assert_eq!(badge, Rect::new(232.0, 19.0, 28.0, 22.0));
    assert!(count_badge_rect(250.0, 28.0, 100.0, 370.0, 10.0).is_none());
}

#[test]
fn a_label_that_opens_more_loses_its_dots() {
    assert_eq!(plain("Options..."), ("Options", true));
    assert_eq!(plain("Resume"), ("Resume", false));
    assert_eq!(plain("Fleet number..."), ("Fleet number", true));
}

#[test]
fn a_settings_row_is_taken_apart() {
    let r = parse_row("Clouds\u{1f}s\u{1f}on\u{1f}Draws the clouds\u{1f}0.5");
    assert_eq!(
        (r.name, r.kind, r.value, r.desc, r.frac),
        ("Clouds", "s", "on", "Draws the clouds", Some(0.5))
    );
    let r = parse_row("Plain");
    assert_eq!((r.name, r.kind, r.value), ("Plain", "a", ""));
}

#[test]
fn destinations_are_codes_and_names() {
    assert_eq!(
        code_and_name(" 12  Hauptbahnhof"),
        Some(("12", "Hauptbahnhof"))
    );
    assert_eq!(code_and_name("Line 5  (3 tours)"), None);
}

#[test]
fn a_dropdown_opens_upwards_where_there_is_no_room_below() {
    let size = Vec2::new(1440.0, 880.0);
    let low = dropdown_rect(Rect::new(500.0, 800.0, 300.0, 34.0), 6, size);
    assert!(low.bottom() <= 800.0, "{low:?}");
    let high = dropdown_rect(Rect::new(500.0, 100.0, 300.0, 34.0), 6, size);
    assert!(high.y >= 134.0, "{high:?}");
}
