use super::*;

#[test]
fn adding_waypoints_preserves_the_active_route_and_duty() {
    let mut n = map();
    n.set_route("duty", vec![8, 9], true, 1);
    n.add_destination(DVec2::new(0.0, 50.0), None);
    n.route.lanes = vec![0];
    n.add_destination(DVec2::new(0.0, 150.0), Some("Depot".into()));
    assert_eq!(n.destination.as_ref().unwrap().at.y, 50.0);
    assert_eq!(n.waypoints.len(), 1);
    assert_eq!(n.route.lanes, vec![0]);
    n.cancel_destination();
    assert!(n.waypoints.is_empty());
    assert_eq!(n.route.lanes, vec![8, 9]);
}

#[test]
fn removing_points_preserves_order_and_replans_the_next_leg() {
    let mut n = map();
    for y in [50.0, 100.0, 150.0] {
        n.add_destination(DVec2::new(0.0, y), None);
    }
    n.remove_destination(1);
    assert_eq!(n.waypoints.len(), 1);
    assert_eq!(n.waypoints[0].at.y, 150.0);
    n.remove_destination(0);
    assert_eq!(n.destination.as_ref().unwrap().at.y, 150.0);
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Pending
    );
    n.remove_destination(0);
    assert!(n.destination.is_none());
}

#[test]
fn arrival_advances_then_finishes_and_restores_duty() {
    let mut n = map();
    n.global = Some(std::sync::Arc::new(network()));
    n.set_route("duty", vec![8], true, 1);
    n.add_destination(DVec2::new(0.0, 50.0), None);
    n.add_destination(DVec2::new(0.0, 150.0), None);
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    n.follow_destination(&frame(DVec3::new(0.0, 49.0, 0.0)));
    assert_eq!(n.destination.as_ref().unwrap().at.y, 150.0);
    n.follow_destination(&frame(DVec3::new(0.0, 149.0, 0.0)));
    n.follow_destination(&frame(DVec3::new(0.0, 149.0, 0.0)));
    assert!(n.destination.is_none());
    assert_eq!(n.route.key, "duty");
}

#[test]
fn later_legs_follow_directed_roads_and_rebuild_after_removal() {
    let mut n = map();
    n.global = Some(std::sync::Arc::new(network()));
    for y in [50.0, 100.0, 150.0] {
        n.add_destination(DVec2::new(0.0, y), None);
    }
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    assert!(n.waypoints.iter().all(|d| d.path.is_some()));
    n.remove_destination(1);
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    assert!((n.waypoints[0].path.as_ref().unwrap().start_s - 50.0).abs() < 0.1);
    n.reset_navigation_network();
    assert!(n.waypoints[0].path.is_none());
}

#[test]
fn unreachable_waypoint_is_kept_and_does_not_skip_ahead() {
    let mut n = map();
    n.global = Some(std::sync::Arc::new(network()));
    n.add_destination(DVec2::new(0.0, -500.0), None);
    n.add_destination(DVec2::new(0.0, 150.0), None);
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Unreachable
    );
    assert_eq!(n.waypoints.len(), 1);
    assert!(n.waypoints[0].path.is_none());
}

#[test]
fn invalid_or_duplicate_waypoints_are_ignored() {
    let mut n = map();
    n.add_destination(DVec2::ONE, None);
    n.add_destination(DVec2::new(f64::NAN, 0.0), None);
    n.add_destination(DVec2::ONE, None);
    n.remove_destination(20);
    assert!(n.waypoints.is_empty());
    assert!(n.destination.is_some());
}

#[test]
fn clicking_a_pin_does_not_append_a_duplicate() {
    let mut n = map();
    n.add_destination(DVec2::splat(1000.0), None);
    n.map_destination(500.0, 400.0);
    assert!(n.waypoints.is_empty());
}

#[test]
fn hover_cross_removes_only_its_pin_without_starting_a_map_click() {
    let mut n = map();
    n.add_destination(DVec2::splat(1000.0), None);
    n.add_destination(DVec2::new(1100.0, 1000.0), None);
    n.map_move(500.0, 400.0);
    let mut painter = Painter::new();
    n.draw_destination_markers(
        &mut painter,
        &|p| n_screen(p),
        Rect::new(0.0, 0.0, 800.0, 600.0),
        1.0,
        true,
        &style::LIGHT,
    );
    assert_eq!(n.city.waypoint_remove.len(), 1);
    let cross = n.city.waypoint_remove[0].0;
    n.map_move(100.0 + cross.center().x, 100.0 + cross.center().y);
    n.draw_destination_markers(
        &mut painter,
        &|p| n_screen(p),
        Rect::new(0.0, 0.0, 800.0, 600.0),
        1.0,
        true,
        &style::LIGHT,
    );
    assert_eq!(n.city.waypoint_remove.len(), 1);
    n.map_press(100.0 + cross.center().x, 100.0 + cross.center().y);
    assert!(!n.map_release());
    assert_eq!(n.destination.as_ref().unwrap().at.x, 1100.0);
    assert!(n.waypoints.is_empty());
}

fn n_screen(p: DVec2) -> Option<Vec2> {
    Some(Vec2::new(400.0 + (p.x - 1000.0) as f32 * 0.5, 300.0))
}

#[test]
fn overlapping_pins_are_drawn_before_clickable_crosses() {
    let mut n = map();
    n.add_destination(DVec2::new(400.0, 300.0), None);
    n.add_destination(DVec2::new(417.0, 283.0), None);
    let area = Rect::new(0.0, 0.0, 800.0, 600.0);
    let screen = |p: DVec2| Some(p.as_vec2());
    let mut pins = Painter::new();
    n.draw_destination_markers(&mut pins, &screen, area, 1.0, true, &style::LIGHT);
    n.map_move(517.0, 361.0);
    let mut hovered = Painter::new();
    n.draw_destination_markers(&mut hovered, &screen, area, 1.0, true, &style::LIGHT);
    // Every pin remains below every clickable cross, including nearby later pins.
    assert_eq!(&hovered.verts[..pins.verts.len()], pins.verts.as_slice());
    let cross = n
        .city
        .waypoint_remove
        .iter()
        .find(|(_, i)| *i == 0)
        .unwrap()
        .0;
    assert!(cross.contains(Vec2::new(417.0, 261.0)));
    n.map_press(517.0, 361.0);
    assert!(!n.map_release());
    assert_eq!(n.destination.as_ref().unwrap().at, DVec2::new(417.0, 283.0));
}

#[test]
fn map_pin_tip_is_anchored_and_the_head_remains_clickable_at_each_scale() {
    for scale in [0.5, 1.0, 2.0] {
        let mut n = map();
        n.add_destination(DVec2::splat(1000.0), None);
        let mut painter = Painter::new();
        n.draw_destination_markers(
            &mut painter,
            &n_screen,
            Rect::new(0.0, 0.0, 800.0, 600.0),
            scale,
            true,
            &style::LIGHT,
        );
        let hit = n.city.waypoint_hits[0].0;
        let tip = Vec2::new(400.0, 300.0);
        let head = tip - Vec2::new(0.0, 22.0 * scale);
        assert!(hit.contains(tip));
        assert!(hit.contains(head));
        assert!(painter.verts.iter().any(|v| v.pos[..2] == tip.to_array()));
        assert!(painter.verts.iter().all(|v| v.pos[1] <= tip.y));
        n.map_move(100.0 + head.x, 100.0 + head.y);
        n.draw_destination_markers(
            &mut painter,
            &n_screen,
            Rect::new(0.0, 0.0, 800.0, 600.0),
            scale,
            true,
            &style::LIGHT,
        );
        let cross = n.city.waypoint_remove[0].0;
        assert!(cross.center().y < head.y);
        n.map_press(100.0 + head.x, 100.0 + head.y);
        assert!(!n.map_release());
        assert!(n.destination.is_some());
    }
}
