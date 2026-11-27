use super::part_in_reach;
use glam::DVec3;

/// Standing by the rear section of an articulated bus, 17 m behind the front part's
/// origin: out of the front part's reach, in the rear one's.
#[test]
fn the_rear_section_is_reached_by_its_own_box() {
    let front = [2.5, 11.0, 3.0, 0.0, -3.0, 1.5];
    let rear = [2.5, 7.0, 3.0, 0.0, -3.5, 1.5];
    let eye = DVec3::new(2.0, -17.0, 1.7);
    assert!(!part_in_reach(eye, DVec3::ZERO, 0.0, Some(front)));
    assert!(part_in_reach(
        eye,
        DVec3::new(0.0, -12.0, 0.0),
        0.0,
        Some(rear)
    ));
    // (the box's centre turns with the part: heading 180, the rear is ahead)
    assert!(part_in_reach(
        DVec3::new(-2.0, 17.0, 1.7),
        DVec3::new(0.0, 12.0, 0.0),
        180.0,
        Some(rear)
    ));
}
