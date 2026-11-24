use super::*;

#[test]
fn travelled_counts_only_the_movement_made_up_to_a_buffer() {
    use omsi_sim::traffic::LaneBuilder;
    let lane = LaneBuilder::polyline(
        vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 100.0, 0.0)],
        LaneKind::Rail,
        1.435,
    );
    let net = Network {
        lanes: vec![lane],
        ..Default::default()
    };
    let len = net.lanes[0].length();
    let at = |s: f32, along: bool| RailDrive {
        lane: 0,
        s,
        along,
        trail: Default::default(),
        u: 0.0,
        body: Default::default(),
        last_heading: None,
    };

    let mut r = at(len - 2.0, true);
    assert!(!r.step(&net, None, 5.0, 0));
    assert_eq!(r.s, len);
    assert!((r.travelled() - 2.0).abs() < 1e-3, "{}", r.travelled());
    for _ in 0..10 {
        assert!(!r.step(&net, None, 5.0, 0));
    }
    assert_eq!(r.s, len);
    assert!((r.travelled() - 2.0).abs() < 1e-3, "{}", r.travelled());
    assert!(r.step(&net, None, -10.0, 0));
    assert!((r.travelled() + 8.0).abs() < 1e-3, "{}", r.travelled());

    let mut r = at(1.0, false);
    assert!(!r.step(&net, None, 3.0, 0));
    assert_eq!(r.s, 0.0);
    assert!((r.travelled() - 1.0).abs() < 1e-3, "{}", r.travelled());
    assert!(!r.step(&net, None, 3.0, 0));
    assert!((r.travelled() - 1.0).abs() < 1e-3, "{}", r.travelled());
}
