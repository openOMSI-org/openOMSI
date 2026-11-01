//! Door candidates for handing the owned bus to another driver.

use glam::DVec3;

pub fn find_exit(
    mut doors: Vec<DVec3>,
    near: DVec3,
    mut check: impl FnMut(DVec3) -> Option<DVec3>,
) -> Option<DVec3> {
    // Handover leaves the driver outside directly; the walking door reach does not apply.
    doors.retain(|door| door.is_finite());
    doors.sort_by(|a, b| a.distance_squared(near).total_cmp(&b.distance_squared(near)));
    doors.into_iter().find_map(&mut check)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_door_of_a_long_bus_is_not_rejected_by_distance_from_origin() {
        let door = DVec3::new(1.8, 5.5, 0.0);
        assert_eq!(find_exit(vec![door], DVec3::ZERO, Some), Some(door));
    }

    #[test]
    fn another_door_is_tried_when_the_nearest_door_is_blocked() {
        let blocked = DVec3::new(1.8, 0.0, 0.0);
        let clear = DVec3::new(1.8, -5.0, 0.0);
        let mut checked = vec![];
        let result = find_exit(vec![clear, blocked], DVec3::ZERO, |door| {
            checked.push(door);
            (door == clear).then_some(door)
        });
        assert_eq!(result, Some(clear));
        assert_eq!(checked, vec![blocked, clear]);
    }

    #[test]
    fn rejected_ground_or_obstacles_never_produce_a_fallback_position() {
        assert_eq!(find_exit(vec![DVec3::X, -DVec3::X], DVec3::ZERO, |_| None), None);
        assert_eq!(find_exit(vec![], DVec3::ZERO, Some), None);
    }

    #[test]
    fn the_drivers_door_is_preferred_over_a_door_near_the_vehicle_origin() {
        let front = DVec3::new(1.8, 5.5, 0.0);
        let rear = DVec3::new(1.8, -0.5, 0.0);
        assert_eq!(find_exit(vec![rear, front], front, Some), Some(front));
    }

    #[test]
    fn a_rotated_translated_bus_can_use_the_door_on_either_side() {
        let origin = DVec3::new(893760.0, 4195478.0, 33.8);
        let near = origin + DVec3::new(5.5, -1.8, 0.0);
        let clear = origin + DVec3::new(5.5, 1.8, 0.0);
        assert_eq!(find_exit(vec![near, clear], near, |d| (d == clear).then_some(d)), Some(clear));
    }

    #[test]
    fn invalid_door_coordinates_are_never_used_as_an_exit() {
        assert_eq!(find_exit(vec![DVec3::NAN, DVec3::INFINITY], DVec3::ZERO, Some), None);
    }

}
