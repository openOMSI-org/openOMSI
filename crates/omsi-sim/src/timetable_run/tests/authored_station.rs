use super::*;
use crate::traffic::{LaneBuilder, LaneKind, Network};
use glam::DVec3;

/// A stop box beside a curve, on either side of the path, keeps the repeated visit its
/// timetable names and its place along the curve.
#[test]
fn authored_platform_boxes_on_curves_keep_the_repeated_visit() {
    for radius in [-40.0, 40.0] {
        let net = Network {
            lanes: vec![LaneBuilder::arc(
                DVec3::ZERO,
                0.0,
                60.0,
                radius,
                0.0,
                LaneKind::Street,
                3.0,
            )],
            ..Default::default()
        };
        let (point, heading) = net.lanes[0].at(30.0);
        let h = (heading as f64).to_radians();
        let right = DVec3::new(h.cos(), -h.sin(), 0.0);
        for offset in [-4.0, 4.0] {
            for side in [0.0, 1.0, 2.0] {
                let (ri, s, lat) = project_stop(
                    &net,
                    &[0, 0],
                    point + right * offset,
                    Some(25.0),
                    0,
                    side,
                    StopRoute::Track(1),
                )
                .unwrap();
                assert_eq!(ri, 1, "the authored repeated visit is retained");
                assert!(
                    (s - 30.0).abs() < 0.5,
                    "projection keeps the position along the curve: {s}"
                );
                assert!(lat.abs() > 3.5, "the platform box lies off the path");
            }
        }
    }
}

#[test]
fn paired_boxes_keep_their_authored_visit_even_across_the_platform_side() {
    let net = Network {
        lanes: vec![
            LaneBuilder::polyline(
                vec![DVec3::ZERO, DVec3::new(0.0, 100.0, 0.0)],
                LaneKind::Street,
                3.0,
            ),
            LaneBuilder::polyline(
                vec![DVec3::new(-6.0, 100.0, 0.0), DVec3::new(-6.0, 0.0, 0.0)],
                LaneKind::Street,
                3.0,
            ),
        ],
        ..Default::default()
    };
    let unload = DVec3::new(1.0, 50.0, 0.0);
    let board = DVec3::new(-1.2, 52.0, 1.0);
    // Recife's type-1 BRT trips name one entry for both boxes, although one
    // box stands across the nominal platform side. A geometric search selects
    // the return visit for that box and advances the search past the boarding visit.
    assert_eq!(
        project_stop(
            &net,
            &[0, 1],
            unload,
            Some(25.0),
            0,
            1.0,
            StopRoute::Nearest
        )
        .unwrap()
        .0,
        1
    );
    let first = project_stop(
        &net,
        &[0, 1],
        unload,
        Some(25.0),
        0,
        1.0,
        StopRoute::Track(0),
    )
    .unwrap();
    let second = project_stop(
        &net,
        &[0, 1],
        board,
        Some(25.0),
        first.0,
        1.0,
        StopRoute::Track(0),
    )
    .unwrap();
    assert_eq!((first.0, second.0), (0, 0));
    assert!((first.2 - 1.0).abs() < 0.01 && (second.2 + 1.2).abs() < 0.01);
    assert!(second.1 > first.1);
    // A repeated lane is a later visit, even with an identical geometric position.
    assert_eq!(
        project_stop(&net, &[0, 1, 0], board, None, 0, 1.0, StopRoute::Track(2))
            .unwrap()
            .0,
        2
    );
    assert!(project_stop(&net, &[0], board, None, 0, 1.0, StopRoute::Outside).is_none());
    assert!(
        project_stop(
            &net,
            &[0],
            DVec3::new(40.0, 50.0, 0.0),
            Some(25.0),
            0,
            1.0,
            StopRoute::Track(0)
        )
        .is_none()
    );
}

#[test]
fn streamed_track_stations_account_for_absent_steps_and_connectors() {
    let slots = [Slot::Lane(7), Slot::Absent, Slot::Lane(9), Slot::Waiting];
    assert_eq!(
        station_route(Some(12), 10, &slots, &[0, 3]),
        StopRoute::Track(3)
    );
    assert_eq!(
        station_route(Some(10), 10, &slots, &[0, 3]),
        StopRoute::Track(0)
    );
    for step in [9, 11, 13, 14] {
        assert_eq!(
            station_route(Some(step), 10, &slots, &[0, 3]),
            StopRoute::Outside
        );
    }
    assert_eq!(station_route(None, 10, &slots, &[0, 3]), StopRoute::Nearest);
}

#[test]
fn only_valid_type_one_track_stations_select_authored_entries() {
    let mut trip = omsi_timetable::Trip {
        stations_legacy: vec![
            vec!["42".into(), "70".into()],
            vec!["43".into(), "bad".into()],
            vec!["44".into(), "99".into()],
        ],
        ..Default::default()
    };
    assert_eq!(
        trip_station_steps(&trip, true, 80),
        vec![Some(70), None, None]
    );
    assert_eq!(trip_station_steps(&trip, false, 80), vec![None; 3]);
    trip.stations = vec![42, 43];
    assert_eq!(trip_station_steps(&trip, true, 80), vec![None; 2]);
}
