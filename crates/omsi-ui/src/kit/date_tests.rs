use super::*;

#[test]
fn calendar_popup_stays_inside_narrow_and_right_edge_windows() {
    for width in [240.0, 400.0, 800.0] {
        let popup = DatePopup { id: 1, anchor: Rect::new(width - 120.0, 80.0, 110.0, 34.0), year: 2026, month: 11, current: (2026, 11, 3), picked: None, opened: 0.0 };
        let r = date_rect(&popup, Vec2::new(width, 600.0));
        assert!(r.x >= 10.0);
        assert!(r.right() <= width - 10.0);
    }
}

#[test]
fn calendar_arithmetic() {
    assert_eq!(weekday(2026, 9, 24), 3); // a Thursday
    assert_eq!(weekday(1989, 5, 30), 1); // a Tuesday
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(days_in_month(1900, 2), 28);
    assert_eq!(parse_date("1989-05-30"), (1989, 5, 30));
}
