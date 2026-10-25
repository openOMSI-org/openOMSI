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
fn next_stop_distance_is_exposed_only_for_its_stop_id() {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    assert_eq!(n.next_stop_distance(), None);
    n.next_dist = Some((10, 123.5));
    assert_eq!(n.next_stop_distance(), Some(123.5));
    assert_eq!(n.next_stop_distance_for(10), Some(123.5));
    assert_eq!(n.next_stop_distance_for(11), None);
    n.invalidate_next_stop_distance();
    assert_eq!(n.next_stop_distance(), None);
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

/// A navigator with the synthetic example of the docs (three lines: two buses and a tram) as its package.
fn with_example_lines() -> Navigator {
    let mut n = Navigator::new(true, 0.85, "bottom-left");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
    n.set_surface(omsi_map::package::Package::open(&dir));
    assert!(n.surface.is_some(), "the example is a package");
    n
}

#[test]
fn the_lines_panel_lists_the_lines_of_one_kind_of_vehicle_and_one_group() {
    let mut n = with_example_lines();
    // buses first: E1 and E2; the BRT tab keeps E2; the tram tab has T1
    assert_eq!(n.shown_lines(), vec![0, 1]);
    n.lines_click(LinesHit::Filter(omsi_map::package::GROUP_BRT));
    assert_eq!(n.shown_lines(), vec![1]);
    n.lines_click(LinesHit::Modal(omsi_map::package::MODAL_TRAM));
    assert_eq!(n.shown_lines(), vec![2]);
}

#[test]
fn a_line_is_ticked_and_unticked_and_a_double_click_opens_it() {
    let mut n = with_example_lines();
    n.lines_click(LinesHit::Toggle(0));
    assert_eq!(n.city.lines.selected, vec![0]);
    n.lines_click(LinesHit::Toggle(0));
    assert!(n.city.lines.selected.is_empty());
    // a click on a name ticks it; a second one within a moment opens its data
    n.time = 10.0;
    n.lines_click(LinesHit::Focus(1));
    assert_eq!((n.city.lines.selected.clone(), n.city.lines.focus), (vec![1], None));
    n.time = 10.2;
    n.lines_click(LinesHit::Focus(1));
    assert_eq!((n.city.lines.selected.clone(), n.city.lines.focus), (vec![1], Some(1)));
    // a line that does not exist is left alone
    n.lines_click(LinesHit::Toggle(99));
    assert_eq!(n.city.lines.selected, vec![1]);
}

#[test]
fn select_all_ticks_what_is_on_show_and_clears_it_the_second_time() {
    let mut n = with_example_lines();
    n.lines_click(LinesHit::All);
    assert_eq!(n.city.lines.selected, vec![0, 1]);
    n.lines_click(LinesHit::All);
    assert!(n.city.lines.selected.is_empty());
}

#[test]
fn the_route_of_the_line_in_focus_is_picked() {
    let mut n = with_example_lines();
    n.debug_lines("E1;1");
    assert_eq!((n.city.lines.open, n.city.lines.selected.clone(), n.city.lines.focus), (true, vec![0], Some(0)));
    assert_eq!(n.city.lines.route.get(&0), Some(&1));
    n.lines_click(LinesHit::RoutePick(0));
    assert_eq!(n.city.lines.route.get(&0), Some(&0));
    // an unknown number opens nothing
    let mut other = with_example_lines();
    other.debug_lines("nope");
    assert!(!other.city.lines.open);
}

#[test]
fn the_terrain_view_leaves_out_the_land_use_and_draws_the_contour_lines() {
    let n = with_example_lines();
    let sf = n.surface.clone().unwrap();
    assert!(sf.has_contours());
    let pal = style::palette(false);
    let (mut full, mut terrain) = (Painter::new(), Painter::new());
    surface::build_surface(&mut full, &sf, DVec2::ZERO, None, pal, false);
    let layers = surface::build_surface(&mut terrain, &sf, DVec2::ZERO, None, pal, true);
    assert!(!full.verts.is_empty() && !terrain.verts.is_empty());
    // (the land use is left out of the terrain view)
    assert!(terrain.verts.len() < full.verts.len());
    // the contour lines are a ribbon each; the strength changes the colour, not the lines
    let (mut strong, mut faint) = (Painter::new(), Painter::new());
    surface::build_contours(&mut strong, &layers, DVec2::ZERO, pal, 1.0);
    surface::build_contours(&mut faint, &layers, DVec2::ZERO, pal, 0.2);
    assert!(!strong.verts.is_empty());
    assert_eq!(strong.verts.len(), faint.verts.len());
    assert!(strong.verts.iter().zip(&faint.verts).any(|(a, b)| a.color != b.color));
}

#[test]
fn the_terrain_view_needs_a_button_a_package_with_relief_and_has_a_band_for_each_zoom() {
    let mut n = with_example_lines();
    assert!(n.contour_band().is_none(), "off until the button is pressed");
    n.show_contours = true;
    n.city.mpp = 2.0;
    assert_eq!(n.contour_band().map(|b| b.1), Some(0));
    n.city.mpp = 8.0;
    assert_eq!(n.contour_band().map(|b| b.1), Some(1));
    n.city.mpp = 30.0;
    assert_eq!(n.contour_band().map(|b| b.1), Some(2));
    // (the surface drawn off: no terrain view either)
    n.surface = None;
    assert!(n.contour_band().is_none());
}

#[test]
fn the_turned_view_has_the_chosen_direction_up_on_the_screen() {
    let (w, h) = (1000.0f32, 600.0f32);
    let top = |phi: f64| Navigator::unproject(&Navigator::city_cam(2.0, 0.0, phi, w, h), w, h, w * 0.5, 0.0).unwrap();
    let centre = |phi: f64| Navigator::unproject(&Navigator::city_cam(2.0, 0.0, phi, w, h), w, h, w * 0.5, h * 0.5).unwrap();
    // as drawn: north (+y) is up; the middle of the view is the point looked at, however the view is turned
    assert!(top(0.0).y > 100.0 && top(0.0).x.abs() < 1e-3);
    assert!(centre(1.234).length() < 1e-3);
    // turned a quarter clockwise: the direction a quarter clockwise from the map's up (east, +x) is up
    let east = top(std::f64::consts::FRAC_PI_2);
    assert!(east.x > 100.0 && east.y.abs() < 1e-2, "{east:?}");
    // half a turn: south is up; the same distance from the middle whichever way it is turned
    let south = top(std::f64::consts::PI);
    assert!(south.y < -100.0 && (south.length() - top(0.0).length()).abs() < 1e-2, "{south:?}");
}

#[test]
fn the_compass_changes_how_the_map_is_turned_in_turn() {
    let mut n = with_example_lines();
    n.city.open = true;
    n.city.rect = [0.0, 0.0, 800.0, 500.0];
    n.city.buttons.push((Rect::new(10.0, 10.0, 40.0, 40.0), 7));
    assert_eq!(n.orient, 0);
    for want in [1, 2, 0] {
        n.map_press(20.0, 20.0);
        n.map_release();
        assert_eq!(n.orient, want);
    }
}

#[test]
fn the_timetable_is_read_on_a_worker_and_the_frame_does_not_wait_for_it() {
    let mut n = with_example_lines();
    n.date = 19900101;
    // (asked for the first time: a worker starts, and what comes back is "not yet" or, if it was quick, the timetable)
    let first = n.line_times();
    assert!(first.is_some() || n.times_job.is_some());
    let began = std::time::Instant::now();
    let mut got = first;
    while got.is_none() && began.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(5));
        got = n.line_times();
    }
    assert!(got.is_some(), "the worker answers");
    // (kept: asked again, it is the same and no worker runs)
    assert!(n.times_job.is_none());
    assert!(std::sync::Arc::ptr_eq(&got.unwrap(), &n.line_times().unwrap()));
    // a new date is read again, on a worker
    n.date = 19910101;
    let again = n.line_times();
    assert!(again.is_some() || n.times_job.as_ref().is_some_and(|j| j.0 == 19910101));
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

/// A stop a plugin closed (`duty.set_closed_stops`) is still on the list, marked with the
/// way round, so it is drawn struck through.
#[test]
fn closed_stops_stay_on_the_list_marked() {
    use omsi_sim::timetable_run::{PlannedStop, PlannedTrip, PlayerDuty};
    let stop = |name: &str, arr: f64| PlannedStop { object_id: 0, name: name.into(), arr, dep: arr, position: None, dir: Default::default(), stops: true, closed: None };
    let trip = PlannedTrip { name: "5_1".into(), line: "5".into(), terminus: "Bahnhof".into(), departure: 0.0, end: 120.0, stops: vec![stop("Markt", 0.0), stop("Rathaus", 60.0), stop("Bahnhof", 120.0)] };
    let mut d = PlayerDuty::new("5".into(), "1".into(), vec![trip], 0, 0, true);
    d.set_closed_stops(&["rathaus".into()], "via Kirchweg");
    let (_, _, stops, _) = duty_parts(Some(&d));
    assert_eq!(stops.iter().map(|s| s.closed.as_deref()).collect::<Vec<_>>(), [None, Some("via Kirchweg"), None]);
}

#[test]
fn the_heights_of_the_contours_are_made_on_a_worker_and_only_with_the_terrain_on() {
    let mut n = with_example_lines();
    let sf = n.surface.clone().unwrap();
    let win = Rect::new(0.0, 0.0, 1000.0, 600.0);
    let at = |_q: DVec3| Vec2::new(500.0, 300.0);
    // terrain off: nothing is asked for, nothing is started
    n.city.mpp = 2.0;
    let mut ui = Painter::new();
    n.terrain_labels(&mut ui, &at, win, 1.0, &mut Vec::new());
    assert_eq!(n.marks_started, 0);
    assert!(sf.contour_marks_ready().is_none());
    // terrain on: the frame does not make them; a worker does, and the labels follow
    n.show_contours = true;
    // (and with the map's surface off there is no terrain view either: no lines are built, so no heights)
    n.show_surface = false;
    n.terrain_labels(&mut ui, &at, win, 1.0, &mut Vec::new());
    assert_eq!(n.marks_started, 0);
    n.show_surface = true;
    n.terrain_labels(&mut ui, &at, win, 1.0, &mut Vec::new());
    assert_eq!(n.marks_started, n.surface_version + 1);
    let began = std::time::Instant::now();
    while sf.contour_marks_ready().is_none() && began.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(sf.contour_marks_ready().is_some_and(|m| !m.is_empty()), "the worker made the marks");
    let before = ui.verts.len();
    n.terrain_labels(&mut ui, &at, win, 1.0, &mut Vec::new());
    assert!(ui.verts.len() > before, "the heights are written once they are there");
    // none over what the map has written already: with the whole window taken, no height is written
    let all = vec![Rect::new(-1.0e5, -1.0e5, 2.0e5, 2.0e5)];
    let before = ui.verts.len();
    n.terrain_labels(&mut ui, &at, win, 1.0, &mut all.clone());
    assert_eq!(ui.verts.len(), before, "the heights step aside for what is already there");
}

#[test]
fn opening_a_line_does_not_read_the_timetable_but_pressing_the_button_does() {
    let mut n = with_example_lines();
    n.date = 19900101;
    n.debug_lines("E1");
    assert!(n.city.lines.open && !n.city.lines.sheet);
    // (the button asks only whether it has been read)
    assert!(n.line_times_read().is_none());
    assert!(n.times_job.is_none(), "no worker for a line that is only opened");
    // the button: the sheet opens, and asking for it starts the worker
    n.lines_click(LinesHit::Sheet);
    assert!(n.city.lines.sheet);
    let first = n.line_times();
    assert!(first.is_some() || n.times_job.is_some());
}

#[test]
fn the_compass_keeps_clear_of_the_open_lines_panel() {
    let (w, h, s) = (1600.0f32, 900.0f32, 1.0f32);
    let r = 24.0 * s;
    // no panel: bottom right
    let free = orient::compass_centre(w, h, s, None);
    assert!((free.x - (w - 36.0 * s - r)).abs() < 1e-3);
    // the panel open (its left edge as the city map lays it out: the column at w - 52, 12 px of gap, 332 px wide)
    let left = (w - 52.0 * s) - 12.0 * s - 332.0 * s;
    let beside = orient::compass_centre(w, h, s, Some(left));
    assert!(beside.x + r <= left, "the disc ends before the panel begins: {} vs {left}", beside.x + r);
    assert_eq!(beside.y, free.y, "only moved sideways");
}

/// A plugin's panel in the whole frame puts the map where it says, kept inside the picture:
/// what reaches past its edges is cut off, and one below it is no map at all.
#[test]
fn a_frame_panel_s_map_stays_inside_the_navigator() {
    let m = |rect: [f32; 4]| FrameMap { rect, cards: false };
    assert_eq!(framed_map(m([10.0, 80.0, 340.0, 272.0]), 360.0, 500.0), (Rect::new(10.0, 80.0, 340.0, 272.0), false));
    assert_eq!(framed_map(m([10.0, 300.0, 400.0, 272.0]), 360.0, 500.0).0, Rect::new(10.0, 300.0, 350.0, 200.0));
    assert_eq!(framed_map(m([10.0, 600.0, 340.0, 272.0]), 360.0, 500.0).0.h, 0.0);
    // (no frame panel: the navigator lays itself out as ever)
    let n = Navigator::new(true, 0.85, "bottom-left");
    assert_eq!((n.frame_panel, n.framed, n.frame_card), (None, None, None));
}
