//! The points key from a single car: the points beyond its end, not those under it.

use super::*;

/// Points 40 m along the track, and points again 20 m on along each of their branches.
fn two_sets_of_points() -> Network {
    use omsi_sim::traffic::LaneBuilder;
    let rail = |pts: Vec<DVec3>| LaneBuilder::polyline(pts, LaneKind::Rail, 1.435);
    let mut lanes = vec![
        rail(vec![DVec3::new(0.0, -40.0, 0.0), DVec3::new(0.0, 0.0, 0.0)]),
        rail(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(-5.0, 20.0, 0.0)]),
        rail(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(5.0, 20.0, 0.0)]),
    ];
    for (x, from) in [(-5.0, 1), (5.0, 2)] {
        let at = lanes.len();
        lanes[from].next = vec![at, at + 1];
        for dx in [-8.0, 8.0] {
            lanes.push(rail(vec![
                DVec3::new(x, 20.0, 0.0),
                DVec3::new(x + dx, 50.0, 0.0),
            ]));
        }
    }
    lanes[0].next = vec![1, 2];
    let net_prev = vec![vec![], vec![0], vec![0], vec![1], vec![1], vec![2], vec![2]];
    Network {
        lanes,
        prev: net_prev,
        ..Default::default()
    }
}

/// A single-car tram standing across points, 3 m from its middle to them and its body
/// 7.5 m either way: J sets the points ahead of its front end, not those under it, and
/// says how far they are from that end. Turned round on the track and driven from its rear
/// end, the same from that end. With no reach of its own it found the points under it.
#[test]
fn a_single_car_straddling_points_sets_the_ones_beyond_its_end() {
    let net = two_sets_of_points();
    let branch_len = net.lanes[1].length();
    let on = |along: bool| RailDrive {
        lane: 0,
        s: 37.0,
        along,
        trail: Default::default(),
        u: 0.0,
        body: Default::default(),
        last_heading: None,
    };
    let def = omsi_vehicle::Vehicle {
        coupling_front: Some(omsi_vehicle::vehicle::Coupling {
            pos: [0.0, 7.5, 0.3],
        }),
        coupling_back: Some(omsi_vehicle::vehicle::Coupling {
            pos: [0.0, -7.5, 0.3],
        }),
        ..Default::default()
    };
    let ends = omsi_sim::vehicle::body_ends(&def, false);
    assert_eq!(ends, (7.5, 7.5));
    for (along, from_rear) in [(true, false), (false, true)] {
        let (dist, branches) =
            switch_ahead(&on(along), &net, None, from_rear, ends).expect("the next points");
        assert!(
            branches.iter().all(|b| b.0 >= 3),
            "the points beyond its end, not those under it: {branches:?}"
        );
        assert!(
            (dist - (branch_len - 4.5)).abs() < 1e-3,
            "from its end: {dist} (along {along})"
        );
        let (under, _) = switch_ahead(&on(along), &net, None, from_rear, (0.0, 0.0)).unwrap();
        assert!((under - 3.0).abs() < 1e-3, "from its middle: {under}");
    }
}
