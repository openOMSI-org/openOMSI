use omsi_trolley::WireNet;

/// A straight line along x with a branch at x = 50, its switch starting `way`.
fn net(way: &str) -> WireNet {
    WireNet::parse(&format!(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np 0 0 5.5\np 50 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 100 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 90 20 5.5\n\
         [trolley_switch]\npos 50 0 5.5\ninitial {way}\n"
    ))
}

#[test]
fn a_random_switch_is_straight_until_drawn_and_the_draw_leaves_fixed_ones_alone() {
    assert!(!net("random").switch_branch(0), "parsing alone is deterministic");
    for way in ["straight", "branch"] {
        let mut n = net(way);
        n.draw_random_switches();
        assert_eq!(n.switch_branch(0), way == "branch");
    }
}

#[test]
fn a_drawn_switch_starts_either_way_without_a_movement() {
    let draws: Vec<bool> = (0..64)
        .map(|_| {
            let mut n = net("random");
            n.draw_random_switches();
            assert!(n.take_switch_sounds().is_empty());
            assert_eq!(n.switch_branch(0), n.switches[0].initial_branch);
            n.switch_branch(0)
        })
        .collect();
    assert!(draws.contains(&true) && draws.contains(&false));
}
