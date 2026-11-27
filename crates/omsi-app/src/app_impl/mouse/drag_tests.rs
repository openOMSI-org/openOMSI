use super::drag_heading;
use glam::DVec2;

#[test]
fn a_ctrl_drag_gives_the_heading_and_a_click_none() {
    let from = DVec2::new(10.0, 10.0);
    assert_eq!(
        drag_heading(
            [100.0, 100.0],
            [105.0, 104.0],
            from,
            Some(DVec2::new(30.0, 10.0))
        ),
        None,
        "under 12 px: a click"
    );
    assert_eq!(
        drag_heading(
            [100.0, 100.0],
            [140.0, 100.0],
            from,
            Some(DVec2::new(30.0, 10.0))
        ),
        Some(90.0),
        "east"
    );
    assert_eq!(
        drag_heading(
            [100.0, 100.0],
            [100.0, 60.0],
            from,
            Some(DVec2::new(10.0, 40.0))
        ),
        Some(0.0),
        "north"
    );
    assert_eq!(
        drag_heading([100.0, 100.0], [140.0, 100.0], from, None),
        None,
        "let go off the map"
    );
}
