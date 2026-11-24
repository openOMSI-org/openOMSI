//! The rail drive backing into points (see `rail_drive`).

use super::*;

/// A train backing into points (its rear end leads: the NF6D driven from its rear cab)
/// takes the branch it is steered to there, and the track its cars follow behind it is
/// that branch - the cars took the straightest way whatever the points said, and the
/// leading car, choosing again by itself, came apart from them. Backing on, the vehicle
/// itself keeps to the trail its train lies on.
#[test]
fn a_train_backing_into_points_keeps_to_the_branch_its_rear_end_takes() {
    use omsi_sim::traffic::LaneBuilder;
    // lane 0 runs north from the points at (0, 0); lanes 1 and 2 come into them from the
    // south-west and the south-east
    let rail = |pts: Vec<DVec3>| LaneBuilder::polyline(pts, LaneKind::Rail, 1.435);
    let mut lanes = vec![
        rail(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 100.0, 0.0)]),
        rail(vec![
            DVec3::new(-30.0, -60.0, 0.0),
            DVec3::new(-5.0, -20.0, 0.0),
            DVec3::new(0.0, 0.0, 0.0),
        ]),
        rail(vec![
            DVec3::new(30.0, -60.0, 0.0),
            DVec3::new(5.0, -20.0, 0.0),
            DVec3::new(0.0, 0.0, 0.0),
        ]),
    ];
    lanes[1].next = vec![0];
    lanes[2].next = vec![0];
    let net = Network {
        lanes,
        prev: vec![vec![1, 2], vec![], vec![]],
        ..Default::default()
    };
    let mut branches = Vec::new();
    for blinker in [1u8, 2] {
        // the vehicle faces north 20 m up lane 0; its train reaches 25 m behind it
        let mut r = RailDrive {
            lane: 0,
            s: 20.0,
            along: true,
            trail: Trail::laid([], 25.0),
            u: 0.0,
            body: Default::default(),
            last_heading: None,
        };
        let here = net.lanes[0].at(20.0).0;
        r.trail.record(0.0, here, r.spot(), TRAIL);
        for _ in 0..60 {
            assert!(r.reverse(&net, None, -0.5, blinker));
        }
        // 30 m back: the vehicle stands on the branch, 10 m into it
        assert_ne!(r.lane, 0, "blinker {blinker}: backed into a branch");
        let behind: std::collections::HashSet<usize> = r
            .trail
            .spots()
            .filter(|e| e.1.0 != 0)
            .map(|e| e.1.0)
            .collect();
        assert_eq!(
            behind.len(),
            1,
            "blinker {blinker}: the train lies on one branch: {behind:?}"
        );
        assert!(
            behind.contains(&r.lane),
            "blinker {blinker}: the vehicle on the branch its train lies on"
        );
        branches.push(r.lane);
    }
    assert_ne!(
        branches[0], branches[1],
        "the indicator chooses the branch backing too: {branches:?}"
    );
}
