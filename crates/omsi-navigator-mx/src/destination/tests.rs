use super::*;
use crate::destination::{route_to_destination, DestinationStatus, EntryMarker};
use omsi_sim::traffic::{Lane, LaneBuilder};

fn road(a: (f64, f64), b: (f64, f64)) -> Lane {
    LaneBuilder::polyline(
        vec![DVec3::new(a.0, a.1, 0.0), DVec3::new(b.0, b.1, 0.0)],
        LaneKind::Street,
        3.0,
    )
}

fn network() -> Network {
    let mut n = Network {
        lanes: vec![
            road((0.0, 0.0), (0.0, 100.0)),
            road((0.0, 100.0), (0.0, 200.0)),
        ],
        ..Default::default()
    };
    n.lanes[0].next = vec![1];
    n.build_grid();
    n
}

#[test]
fn destination_path_ends_at_the_selected_point_on_the_road() {
    let n = network();
    let p =
        route_to_destination(&n, DVec3::new(0.0, 10.0, 0.0), 0.0, DVec2::new(0.0, 150.0)).unwrap();
    assert_eq!(p.lanes, vec![0, 1]);
    assert!((p.end_s - 50.0).abs() < 0.1);
    assert!((p.end - DVec3::new(0.0, 150.0, 0.0)).length() < 0.1);
}

#[test]
fn destination_off_road_keeps_a_reachable_road_endpoint() {
    let p = route_to_destination(
        &network(),
        DVec3::new(0.0, 10.0, 0.0),
        0.0,
        DVec2::new(30.0, 150.0),
    )
    .unwrap();
    assert!((p.end.x).abs() < 0.1);
    assert!((p.end.y - 150.0).abs() < 0.1);
}

#[test]
fn destination_behind_on_a_one_way_road_is_not_a_forward_route() {
    let mut n = network();
    n.lanes.truncate(1);
    n.lanes[0].next.clear();
    n.build_grid();
    assert!(
        route_to_destination(&n, DVec3::new(0.0, 80.0, 0.0), 0.0, DVec2::new(0.0, 20.0)).is_none()
    );
}

#[test]
fn destination_behind_can_be_reached_by_a_directed_loop() {
    let mut n = Network {
        lanes: vec![
            road((0.0, 0.0), (0.0, 100.0)),
            road((0.0, 100.0), (100.0, 100.0)),
            road((100.0, 100.0), (100.0, 0.0)),
            road((100.0, 0.0), (0.0, 0.0)),
        ],
        ..Default::default()
    };
    for i in 0..4 {
        n.lanes[i].next = vec![(i + 1) % 4];
    }
    n.build_grid();
    let p =
        route_to_destination(&n, DVec3::new(0.0, 80.0, 0.0), 0.0, DVec2::new(0.0, 20.0)).unwrap();
    assert_eq!(p.lanes, vec![0, 1, 2, 3, 0]);
}

#[test]
fn destination_disconnected_roads_do_not_make_a_fake_route() {
    let mut n = network();
    n.lanes[0].next.clear();
    assert!(
        route_to_destination(&n, DVec3::new(0.0, 10.0, 0.0), 0.0, DVec2::new(0.0, 190.0)).is_none()
    );
}

#[test]
fn destination_cancelling_restores_the_latest_duty_route() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.set_route("first", vec![1, 2], true, 1);
    n.set_destination(DVec2::new(20.0, 30.0), None);
    assert!(n.route.lanes.is_empty());
    assert!(!n.wants_route("first", 1));
    n.set_route("second", vec![4, 5], true, 2);
    assert!(n.route.lanes.is_empty());
    n.cancel_destination();
    assert_eq!(n.route.key, "second");
    assert_eq!(n.route.lanes, vec![4, 5]);
    assert!(n.destination.is_none());
}

#[test]
fn destination_survives_free_drive_clear_route_and_replacement() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.set_destination(DVec2::new(20.0, 30.0), None);
    n.clear_route();
    n.set_destination(DVec2::new(40.0, 50.0), Some("Depot".into()));
    assert_eq!(n.destination.as_ref().unwrap().at, DVec2::new(40.0, 50.0));
    n.cancel_destination();
    assert!(n.route.key.is_empty());
    assert!(n.duty_route.is_none());
}

fn map() -> Navigator {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    n.city.open = true;
    n.city.rect = [100.0, 100.0, 900.0, 700.0];
    n.city.center = DVec2::new(1000.0, 1000.0);
    n.city.mpp = 2.0;
    n
}

#[test]
fn destination_map_click_is_free_and_drag_does_not_set_a_pin() {
    let mut n = map();
    n.map_press(500.0, 400.0);
    assert!(n.map_release());
    n.map_destination(500.0, 400.0);
    assert!((n.destination.as_ref().unwrap().at - DVec2::splat(1000.0)).length() < 0.1);
    n.cancel_destination();
    n.map_press(500.0, 400.0);
    n.map_move(550.0, 400.0);
    assert!(!n.map_release());
    assert!(n.destination.is_none());
}

#[test]
fn destination_entry_selection_uses_screen_hit_tolerance() {
    let mut n = map();
    n.entries.push(EntryMarker {
        at: DVec2::splat(1000.0),
        name: "Depot".into(),
    });
    n.map_destination(505.0, 400.0);
    let d = n.destination.as_ref().unwrap();
    assert_eq!(d.at, DVec2::splat(1000.0));
    assert_eq!(d.name, "Depot");
    n.map_destination(560.0, 400.0);
    assert_eq!(n.destination.as_ref().unwrap().at, DVec2::splat(1000.0));
    assert_ne!(n.waypoints[0].at, DVec2::splat(1000.0));
}

#[test]
fn destination_invalid_coordinates_are_ignored() {
    let mut n = map();
    n.set_destination(DVec2::new(f64::NAN, 1.0), None);
    assert!(n.destination.is_none());
    n.set_destination(DVec2::ONE, None);
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Pending
    );
}

fn frame(bus: DVec3) -> NavFrame<'static> {
    NavFrame {
        traffic: None,
        touch: false,
        players: vec![],
        bus,
        heading: 0.0,
        speed_kmh: 0.0,
        outside_temp: 20.0,
        inside_temp: 20.0,
        line: None,
        terminus: None,
        stops: vec![],
        delay: None,
        passengers: None,
        stop_requested: false,
        time: 0.0,
        weekday: 0,
        date: 20261020,
        language: "ENG",
        screen: (1280.0, 720.0),
        ui_scale: 1.0,
        follow_window: false,
        dt: 0.1,
        info_rect: None,
        touch_gap: None,
    }
}

#[test]
fn destination_progress_and_arrival_use_the_road_endpoint() {
    let mut n = map();
    n.global = Some(std::sync::Arc::new(network()));
    n.set_destination(DVec2::new(30.0, 150.0), None);
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Navigating
    );
    assert_eq!(n.route.lanes, vec![0, 1]);
    n.follow_destination(&frame(DVec3::new(0.0, 110.0, 0.0)));
    assert_eq!(n.route.progress, 1);
    assert!(
        (n.destination_distance(n.global.as_deref().unwrap())
            .unwrap()
            - 40.0)
            .abs()
            < 0.1
    );
    n.follow_destination(&frame(DVec3::new(0.0, 148.0, 0.0)));
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Arrived
    );
    assert!(n.route.lanes.is_empty());
    assert_eq!(n.destination.as_ref().unwrap().at, DVec2::new(30.0, 150.0));
}

#[test]
fn destination_unreachable_clears_the_previous_path() {
    let mut n = map();
    n.global = Some(std::sync::Arc::new(network()));
    n.set_destination(DVec2::new(0.0, 150.0), None);
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    let mut moved = frame(DVec3::new(900.0, 900.0, 0.0));
    moved.dt = 6.0;
    n.follow_destination(&moved);
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Unreachable
    );
    assert!(n.route.lanes.is_empty());
}

#[test]
fn destination_network_changes_keep_the_pin_and_rebuild_the_path() {
    let mut n = map();
    n.global = Some(std::sync::Arc::new(network()));
    n.set_destination(DVec2::new(0.0, 150.0), Some("Depot".into()));
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    n.global_version += 1;
    n.reset_navigation_network();
    assert_eq!(n.destination.as_ref().unwrap().name, "Depot");
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Navigating
    );
    assert_eq!(n.route.lanes, vec![0, 1]);
}

#[test]
fn destination_buttons_do_not_set_or_replace_the_pin() {
    let mut n = map();
    let button = Rect::new(380.0, 280.0, 50.0, 50.0);
    n.city.buttons.push((button, 9));
    n.map_press(500.0, 400.0);
    assert!(!n.map_release());
    n.map_destination(500.0, 400.0);
    assert!(n.destination.is_none());
    n.set_destination(DVec2::ONE, None);
    n.city.buttons = vec![(button, 8)];
    n.map_press(500.0, 400.0);
    assert!(!n.map_release());
    assert!(n.destination.is_none());
}

#[test]
fn destination_route_drawing_clips_the_last_lane_only_when_requested() {
    use mapmesh::roads::{build_route, build_route_to, RouteStyle};
    let n = network();
    let style = RouteStyle {
        extra_m: 0.0,
        min_px: 4.0,
        arrows: None,
        max_len: f32::MAX,
        near: None,
    };
    let mut full = Painter::new();
    let mut compatible = Painter::new();
    let mut clipped = Painter::new();
    let jam = HashMap::new();
    build_route(
        &mut full,
        &n,
        &[0, 1],
        DVec2::ZERO,
        10.0,
        &jam,
        &style,
        0,
        &style::LIGHT,
    );
    build_route_to(
        &mut compatible,
        &n,
        &[0, 1],
        DVec2::ZERO,
        10.0,
        &jam,
        &style,
        0,
        &style::LIGHT,
        None,
    );
    build_route_to(
        &mut clipped,
        &n,
        &[0, 1],
        DVec2::ZERO,
        10.0,
        &jam,
        &style,
        0,
        &style::LIGHT,
        Some(50.0),
    );
    assert_eq!(full.verts.len(), compatible.verts.len());
    for (a, b) in full.verts.iter().zip(&compatible.verts) {
        assert_eq!(a.pos, b.pos);
    }
    assert!(clipped.verts.iter().all(|v| v.pos[1] <= 151.0));
    assert!(full.verts.iter().any(|v| v.pos[1] >= 199.0));
}

#[test]
fn destination_loop_with_a_parallel_lane_keeps_the_earlier_entry() {
    let mut n = Network {
        lanes: vec![
            road((0.0, 0.0), (0.0, 100.0)),
            road((3.0, 0.0), (3.0, 100.0)),
            LaneBuilder::polyline(
                vec![
                    DVec3::new(0.0, 100.0, 0.0),
                    DVec3::new(100.0, 100.0, 0.0),
                    DVec3::new(100.0, 0.0, 0.0),
                    DVec3::ZERO,
                ],
                LaneKind::Street,
                3.0,
            ),
        ],
        ..Default::default()
    };
    n.lanes[0].right = Some(1);
    n.lanes[1].left = Some(0);
    n.lanes[0].next = vec![2];
    n.lanes[1].next = vec![2];
    n.lanes[2].next = vec![0];
    n.build_grid();
    let path =
        route_to_destination(&n, DVec3::new(0.0, 80.0, 0.0), 0.0, DVec2::new(0.0, 20.0)).unwrap();
    assert_eq!(path.lanes, vec![0, 2, 0]);
}

#[test]
fn destination_short_loop_progress_does_not_return_to_the_first_occurrence() {
    let mut net = Network {
        lanes: vec![
            road((0.0, 0.0), (0.0, 100.0)),
            LaneBuilder::polyline(
                vec![
                    DVec3::new(0.0, 100.0, 0.0),
                    DVec3::new(100.0, 100.0, 0.0),
                    DVec3::new(100.0, 0.0, 0.0),
                    DVec3::ZERO,
                ],
                LaneKind::Street,
                3.0,
            ),
        ],
        ..Default::default()
    };
    net.lanes[0].next = vec![1];
    net.lanes[1].next = vec![0];
    net.build_grid();
    let mut n = map();
    n.global = Some(std::sync::Arc::new(net));
    n.set_destination(DVec2::new(0.0, 30.0), None);
    n.follow_destination(&frame(DVec3::new(0.0, 80.0, 0.0)));
    assert_eq!(n.route.lanes, vec![0, 1, 0]);
    let mut halfway = frame(DVec3::new(100.0, 50.0, 0.0));
    halfway.heading = 180.0;
    n.follow_destination(&halfway);
    assert_eq!(n.route.progress, 1);
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    assert_eq!(n.route.progress, 2);
    n.follow_destination(&frame(DVec3::new(0.0, 29.0, 0.0)));
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Arrived
    );
}

#[test]
fn destination_entry_name_label_is_clickable_over_its_whole_rectangle() {
    let mut n = map();
    n.entries.push(EntryMarker {
        at: DVec2::splat(1000.0),
        name: "Airport Maintenance Area".into(),
    });
    let mut painter = Painter::new();
    n.draw_destination_markers(
        &mut painter,
        &|_| Some(Vec2::new(400.0, 300.0)),
        Rect::new(0.0, 0.0, 800.0, 600.0),
        1.0,
        true,
        &style::LIGHT,
    );
    let rect = n.city.entry_hits[0].0;
    assert!(rect.w > 40.0);
    n.map_destination(100.0 + rect.center().x, 100.0 + rect.center().y);
    let d = n.destination.as_ref().unwrap();
    assert_eq!(d.at, DVec2::splat(1000.0));
    assert_eq!(d.name, "Airport Maintenance Area");
}

#[test]
fn destination_past_the_endpoint_replans_instead_of_sticking_at_zero_distance() {
    let mut net = Network {
        lanes: vec![road((0.0, 0.0), (0.0, 1000.0))],
        ..Default::default()
    };
    net.build_grid();
    let mut n = map();
    n.global = Some(std::sync::Arc::new(net));
    n.set_destination(DVec2::new(0.0, 100.0), None);
    n.follow_destination(&frame(DVec3::new(0.0, 10.0, 0.0)));
    let mut beyond = frame(DVec3::new(0.0, 200.0, 0.0));
    beyond.dt = 6.0;
    n.follow_destination(&beyond);
    assert_eq!(
        n.destination.as_ref().unwrap().status,
        DestinationStatus::Unreachable
    );
    assert!(n.route.lanes.is_empty());
}

#[test]
fn destination_drawing_keeps_the_target_lane_when_the_neighbour_has_a_different_origin() {
    use mapmesh::roads::{build_route_to, RouteStyle};
    let mut net = network();
    net.lanes.push(road((3.0, 120.0), (3.0, 220.0)));
    net.lanes[1].right = Some(2);
    net.lanes[2].left = Some(1);
    let style = RouteStyle {
        extra_m: 0.0,
        min_px: 4.0,
        arrows: None,
        max_len: f32::MAX,
        near: None,
    };
    let mut p = Painter::new();
    build_route_to(
        &mut p,
        &net,
        &[0, 1],
        DVec2::ZERO,
        10.0,
        &HashMap::new(),
        &style,
        0,
        &style::LIGHT,
        Some(50.0),
    );
    assert!(!p.verts.is_empty());
    assert!(p.verts.iter().all(|v| v.pos[1] <= 151.0));
}

#[path = "waypoints_tests.rs"]
mod waypoints;
