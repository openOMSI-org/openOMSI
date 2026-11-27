#[test]
fn a_cursor_way_of_78_75_px_turns_by_the_field_of_view() {
    assert!((78.75 * super::look_deg_per_px(60.0) - 60.0).abs() < 1e-4);
}
