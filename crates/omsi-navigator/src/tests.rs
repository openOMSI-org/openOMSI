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
    // (flat from above at this zoom: the middle is the centre, 100 px right is 200 m east)
    let mid = n.map_point(2900.0, 500.0).expect("on the map");
    assert!((mid - n.city.center).length() < 0.05, "{mid:?}");
    let east = n.map_point(3000.0, 500.0).expect("on the map") - n.city.center;
    assert!((east.x - 200.0).abs() < 0.5 && east.y.abs() < 0.5, "{east:?}");
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
    let back = placed_at(&n.placement()).unwrap();
    assert!((back[0] - 0.5).abs() < 1e-3 && (back[1] - 0.5).abs() < 1e-3);
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
use omsi_sim::traffic::LaneKey;
use omsi_sim::traffic::Lane;

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

/// The bus went along a road beside the route. A short cut back leads onto the route
/// behind (it would drive that part of the route again); going on a little leads onto it
/// further on. The way back is the quickest to the end of the route, not to its nearest lane.
#[test]
fn a_way_back_does_not_lead_back_along_the_route() {
    let lanes = vec![
        straight((0.0, 0.0), (100.0, 0.0)),    // 0 route
        straight((100.0, 0.0), (200.0, 0.0)),  // 1 route
        straight((200.0, 0.0), (300.0, 0.0)),  // 2 route
        straight((300.0, 0.0), (400.0, 0.0)),  // 3 route
        straight((100.0, 50.0), (200.0, 50.0)), // 4 the road beside, the bus on it
        straight((200.0, 50.0), (300.0, 50.0)), // 5
        straight((300.0, 50.0), (300.0, 0.0)),  // 6 down onto lane 3
        straight((200.0, 50.0), (100.0, 0.0)),  // 7 a short cut back onto lane 1
    ];
    let mut net = Network { lanes, ..Default::default() };
    for (a, n) in [(0, vec![1]), (1, vec![2]), (2, vec![3]), (4, vec![5, 7]), (5, vec![6]), (6, vec![3]), (7, vec![1])] {
        net.lanes[a].next = n;
    }
    net.build_grid();
    let route = [1usize, 2, 3];
    let (path, join) = way_back(&net, DVec3::new(150.0, 50.0, 0.0), 90.0, &route, 6000.0).expect("a way");
    assert_eq!(route[join], 3, "{path:?} join {join}");
    assert!(!path.contains(&7), "{path:?}");
}

/// A turn off the left lane of a two-lane road: the bus in the right lane changes over rather
/// than being sent round the block (lanes beside each other are joined by `left`/`right`).
#[test]
fn a_way_may_change_lanes() {
    let lanes = vec![
        straight((0.0, 0.0), (0.0, 100.0)),    // 0 right lane, the bus
        straight((-3.5, 0.0), (-3.5, 100.0)),  // 1 left lane
        straight((0.0, 100.0), (0.0, 200.0)),  // 2 straight on from the right lane
        straight((-3.5, 100.0), (-100.0, 100.0)), // 3 left turn from the left lane
        straight((-100.0, 100.0), (-200.0, 100.0)), // 4 the route
    ];
    let mut net = Network { lanes, ..Default::default() };
    for (a, n) in [(0, vec![2]), (1, vec![3]), (3, vec![4])] {
        net.lanes[a].next = n;
    }
    net.lanes[0].left = Some(1);
    net.lanes[1].right = Some(0);
    net.build_grid();
    let (path, join) = way_back(&net, DVec3::new(0.0, 20.0, 0.0), 0.0, &[4], 6000.0).expect("a way");
    assert_eq!(join, 0);
    assert_eq!(path, vec![1, 3], "the bus's own lane is left for the one beside it");
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


#[test]
fn redraw_rate_holds_steady_at_its_edges() {
    let run = |fps: &[f32]| fps.iter().fold(1, |n, f| draw_every(n, 1.0 / f));
    assert_eq!(run(&[60.0]), 1);
    assert_eq!(run(&[74.0, 76.5, 74.5, 77.0]), 1);
    assert_eq!(run(&[144.0]), 2);
    assert_eq!(run(&[144.0, 148.0, 152.0, 147.0]), 2);
    assert_eq!(run(&[240.0]), 4);
    assert_eq!(run(&[25.0]), 2);
    assert_eq!(run(&[25.0, 31.0, 29.5, 31.5]), 2);
    assert_eq!(run(&[31.0, 29.0, 31.0]), 1);
    assert_eq!(run(&[25.0, 40.0]), 1);
}

#[test]
fn the_map_zooms_in_for_a_turn_and_out_on_the_straight() {
    assert_eq!(zoom_target(0.0, None, 0.0), 150.0);
    assert_eq!(zoom_target(80.0, None, 0.0), 320.0);
    assert_eq!(zoom_target(-20.0, None, 0.0), zoom_target(20.0, None, 0.0));
    assert_eq!(zoom_target(80.0, Some(1000.0), 0.0), 320.0);
    assert!(zoom_target(70.0, Some(100.0), 0.0) < 220.0);
    assert_eq!(zoom_target(70.0, Some(0.0), 0.0), 150.0);
    assert!(zoom_target(50.0, None, 30.0) < zoom_target(50.0, None, 5.0));
    assert!(zoom_target(50.0, None, -30.0) >= 150.0);
}
