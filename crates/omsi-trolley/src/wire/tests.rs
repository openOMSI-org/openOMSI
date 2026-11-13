//! The overhead wire network: its tests.
use super::*;

/// A straight line along x with a branch leaving to the left at x = 50.
pub(crate) fn y_net() -> WireNet {
    WireNet::parse(
        "[trolley_net]\nvoltage 600\n\
         [trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np 0 0 5.5\np 25 0 5.5\np 50 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 75 0 5.5\np 100 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 70 6 5.5\np 90 20 5.5\n\
         [trolley_switch]\npos 50 0 5.5\nmode power\ntrigger 8\nsense 10\n",
    )
}

/// A 0.8 m piece beside the frog (shorter than the joining distance) is closed over, not
/// joined to itself: the frog still has its three ends and the straight way runs through.
#[test]
fn a_stub_shorter_than_the_join_distance_does_not_break_the_frog() {
    let n = WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np 0 0 5.5\np 25 0 5.5\np 50 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 50.8 0 5.5\n\
         [trolley_span]\nsection 1\np 50.8 0 5.5\np 75 0 5.5\np 100 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 70 6 5.5\np 90 20 5.5\n\
         [trolley_switch]\npos 50 0 5.5\nmode fixed\n",
    );
    assert_eq!(n.spans.len(), 3, "the stub is dropped");
    assert_eq!(n.switches.len(), 1);
    assert!(n.spans.iter().all(|s| s.len() > 1.0));
}

/// A piece longer than twice the joining distance whose ends curl close together keeps its
/// two ends apart: they are two nodes, and the neighbours at the first end meet there.
#[test]
fn a_curled_piece_does_not_join_its_own_two_ends() {
    let n = WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np 0 0 5.5\np 20 0 5.5\n\
         [trolley_span]\nsection 1\np 20 0 5.5\np 22 1.5 5.5\np 22.5 4 5.5\np 21 5 5.5\np 20.6 0.8 5.5\n\
         [trolley_span]\nsection 1\np 20.6 0.8 5.5\np 40 0 5.5\n",
    );
    let curl = &n.spans[1];
    assert_ne!(curl.ends[0], curl.ends[1], "both ends in one node");
    assert!(curl.len() > 4.0);
}

#[test]
fn spans_join_into_nodes_and_a_switch() {
    let n = y_net();
    assert_eq!(n.spans.len(), 3);
    assert_eq!(n.switches.len(), 1);
    let sw = &n.switches[0];
    assert_eq!(sw.toe.0, 0);
    assert_eq!(sw.straight.0, 1);
    assert_eq!(sw.branch.0, 2);
    // the branch leaves to the left (north) of a vehicle heading east
    assert!(!sw.branch_right);
}

#[test]
fn a_shoe_takes_the_branch_the_switch_is_set_to() {
    let n = y_net();
    let mut dir = 1i8;
    let mut c = Contact { span: 0, s: 40.0, side: 0 };
    let mut info = WalkInfo::default();
    n.walk(&mut c, &mut dir, 20.0, &mut info).unwrap();
    assert_eq!(c.span, 1);
    assert!((c.s - 10.0).abs() < 1e-9);
    n.set_switch(0, true);
    let mut c = Contact { span: 0, s: 40.0, side: 0 };
    let mut dir = 1i8;
    n.walk(&mut c, &mut dir, 20.0, &mut WalkInfo::default()).unwrap();
    assert_eq!(c.span, 2);
    // the branch turns away from the main line: the walk saw a kink
    let mut info = WalkInfo::default();
    let mut c = Contact { span: 0, s: 40.0, side: 0 };
    n.walk(&mut c, &mut 1i8.clone(), 20.0, &mut info).unwrap();
    assert!(info.kink > 0.2, "{}", info.kink);
}

#[test]
fn the_end_of_a_wire_is_a_dead_end() {
    let n = y_net();
    let mut c = Contact { span: 1, s: 20.0, side: 0 };
    let mut dir = 1i8;
    assert!(n.walk(&mut c, &mut dir, 100.0, &mut WalkInfo::default()).is_err());
}

#[test]
fn trailing_through_the_frog_goes_to_the_toe_whatever_the_position() {
    let n = y_net();
    for branch in [false, true] {
        n.set_switch(0, branch);
        let mut c = Contact { span: 2, s: 5.0, side: 0 };
        let mut dir = -1i8;
        n.walk(&mut c, &mut dir, 10.0, &mut WalkInfo::default()).unwrap();
        assert_eq!(c.span, 0);
        assert_eq!(dir, -1);
    }
}

#[test]
fn solving_the_tip_puts_it_a_boom_length_from_the_hinge() {
    let n = y_net();
    let base = DVec3::new(30.0, 0.0, 3.0);
    let mut c = Contact { span: 0, s: 20.0, side: 0 };
    n.solve_tip(&mut c, base, 6.5).unwrap();
    let (p, _) = n.wire_point(&c);
    assert!(((p - base).length() - 6.5).abs() < 1e-3);
    // the pole trails behind the hinge: the contact is behind (west of) it
    assert!(p.x < base.x);
}

#[test]
fn a_wire_too_far_cannot_be_reached() {
    let n = y_net();
    let base = DVec3::new(30.0, 0.0, -10.0);
    let mut c = Contact { span: 0, s: 20.0, side: 0 };
    assert!(n.solve_tip(&mut c, base, 6.5).is_err());
}

#[test]
fn dead_sections_and_insulator_gaps_carry_no_current() {
    let n = WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n[trolley_section]\nid 2\nlive 0\n\
         [trolley_span]\nsection 1\np 0 0 5\np 20 0 5\n[trolley_span]\nsection 2\np 20 0 5\np 40 0 5\n",
    );
    assert_eq!(n.spans.len(), 2);
    assert!(n.is_live(&Contact { span: 0, s: 10.0, side: 0 }));
    assert!(!n.is_live(&Contact { span: 1, s: 10.0, side: 0 }));
    // the stretch around the insulator is dead even on the live side
    assert!(!n.is_live(&Contact { span: 0, s: 19.8, side: 0 }));
    assert!(n.is_live(&Contact { span: 0, s: 19.0, side: 0 }));
}

#[test]
fn power_sensed_switch_follows_the_current() {
    let n = y_net();
    let amps = |current: f32| SwitchRequest { current, ..Default::default() };
    assert!(n.trigger_switch(0, &amps(80.0)));
    assert!(n.switch_branch(0));
    // held while the vehicle passes
    assert!(!n.trigger_switch(0, &amps(0.0)));
    assert!(n.switch_branch(0));
    n.end_step(3.0);
    assert!(n.trigger_switch(0, &amps(0.0)));
    assert!(!n.switch_branch(0));
}

pub(super) fn radio_net(extra: &str) -> WireNet {
    let text = format!(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np 0 0 5.5\np 25 0 5.5\np 50 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 75 0 5.5\np 100 0 5.5\n\
         [trolley_span]\nsection 1\np 50 0 5.5\np 70 6 5.5\np 90 20 5.5\n\
         [trolley_switch]\npos 50 0 5.5\nmode radio\ntrigger 8\n{extra}"
    );
    WireNet::parse(&text)
}

#[test]
fn radio_always_uses_range_and_never_accepts_a_contact_plate() {
    let request = SwitchRequest { radio: -1, ..Default::default() };
    let net = radio_net("");
    assert_eq!(net.switches[0].radio_range, 150.0);
    assert!(!net.trigger_switch(0, &request));
    assert!(!net.switch_branch(0));
    assert_eq!(net.receive_radio(DVec3::new(50.0, 0.0, 5.5), &request), 1);
    let mut ranged = radio_net("range 150\n");
    assert_eq!(ranged.switches[0].radio_range, 150.0);
    for mode in [SwitchMode::Power, SwitchMode::Indicator, SwitchMode::Fixed] {
        ranged.switches[0].mode = mode;
        assert_eq!(ranged.receive_radio(DVec3::new(50.0, 0.0, 5.5), &request), 0);
    }
    assert_eq!(radio_net("range -10\n").switches[0].radio_range, 1.0);
    assert_eq!(radio_net("range 0\n").switches[0].radio_range, 1.0);
    assert_eq!(radio_net("range 5000\n").switches[0].radio_range, 1000.0);
}

#[test]
fn radio_receivers_use_horizontal_range_and_filter_channels() {
    let net = radio_net("range 20\nchannel 3\n");
    let request = SwitchRequest { radio: -1, channel: 3, ..Default::default() };
    assert_eq!(net.receive_radio(DVec3::new(50.0, 20.01, 5.5), &request), 0);
    assert_eq!(net.receive_radio(DVec3::new(50.0, 0.0, 5.5), &SwitchRequest { channel: 2, ..request }), 0);
    assert!(!net.switch_branch(0));
    assert_eq!(net.receive_radio(DVec3::new(50.0, 20.0, 100.0), &request), 1);
    assert!(net.switch_branch(0));
}

#[test]
fn radio_receivers_change_immediately_and_keep_unknown_route_positions() {
    let net = radio_net("range 20\nroute 1 left\nroute 2 right\n");
    let pos = DVec3::new(50.0, 0.0, 5.5);
    let left = SwitchRequest { code: 1, ..Default::default() };
    let right = SwitchRequest { code: 2, ..Default::default() };
    assert_eq!(net.receive_radio(pos, &left), 1);
    assert_eq!(net.receive_radio(pos, &right), 1);
    assert_eq!(net.switch_display(0), Some((2.0, 1.0, 0.0)));
    assert_eq!(net.receive_radio(pos, &left), 1);
    for _ in 0..30 {
        assert_eq!(net.receive_radio(pos, &left), 0);
        net.end_step(0.1);
    }
    assert_eq!(net.receive_radio(pos, &SwitchRequest { code: 999, ..Default::default() }), 0);
    assert_eq!(net.receive_radio(pos, &right), 1);
    assert!(!net.switch_branch(0));
    assert_eq!(net.receive_radio(pos, &left), 1);
}

#[test]
fn a_radio_switch_takes_the_cabs_request_and_keeps_it() {
    let n = radio_net("");
    assert_eq!(n.switches[0].mode, SwitchMode::Radio);
    assert!(!n.switches[0].branch_right, "the branch leaves to the left");
    let radio = |radio: i8| SwitchRequest { radio, ..Default::default() };
    // power and indicator do not set it
    assert_eq!(n.receive_radio(DVec3::ZERO, &SwitchRequest { current: 300.0, blinker: -1, ..Default::default() }), 0);
    assert_eq!(n.receive_radio(DVec3::ZERO, &radio(-1)), 1);
    assert!(n.switch_branch(0));
    n.end_step(3.0);
    // no request: the next vehicle finds it as it was left
    assert_eq!(n.receive_radio(DVec3::ZERO, &radio(0)), 0);
    assert!(n.switch_branch(0));
    assert_eq!(n.receive_radio(DVec3::ZERO, &radio(1)), 1);
    assert!(!n.switch_branch(0));
}

#[test]
fn displays_bind_by_name_before_distance_and_have_a_limited_fallback() {
    let net = radio_net("name Junction A\n");
    assert_eq!(net.switch_for_display(DVec3::ZERO, &[" junction a ".into()]), Some(0));
    assert_eq!(net.switch_for_display(DVec3::new(50.0, 10.0, 20.0), &[]), Some(0));
    assert_eq!(net.switch_for_display(DVec3::new(50.0, 40.0, 5.5), &[]), None);
    assert_eq!(net.switch_display(99), None);
}

#[test]
fn displays_show_actual_direction_and_time_since_a_change() {
    let mut net = radio_net("");
    assert_eq!(net.switch_display(0), Some((2.0, 1.0, 1.0e6)));
    assert_eq!(net.receive_radio(DVec3::ZERO, &SwitchRequest { radio: -1, ..Default::default() }), 1);
    assert_eq!(net.switch_display(0), Some((1.0, 0.0, 0.0)));
    net.end_step(1.0);
    assert_eq!(net.switch_display(0), Some((1.0, 0.0, 0.0)));
    net.step_switch_displays(1.0);
    assert_eq!(net.receive_radio(DVec3::ZERO, &SwitchRequest { radio: -1, ..Default::default() }), 0);
    assert_eq!(net.switch_display(0), Some((1.0, 0.0, 1.0)));
    net.set_switch(0, true);
    assert_eq!(net.switch_display(0), Some((1.0, 0.0, 1.0)));
    net.set_switch(0, false);
    assert_eq!(net.switch_display(0), Some((2.0, 1.0, 0.0)));
    net.switches[0].branch_right = true;
    assert_eq!(net.switch_display(0), Some((1.0, 1.0, 0.0)));
    net.set_switch(0, true);
    assert_eq!(net.switch_display(0), Some((2.0, 0.0, 0.0)));
}

#[test]
fn route_codes_set_a_radio_switch_and_the_cabs_request_overrides_them() {
    let n = radio_net("route 158 branch\nroute 160 straight\nroute 7 right\n");
    assert_eq!(n.switches[0].routes, vec![(158, SwitchWay::Branch), (160, SwitchWay::Straight), (7, SwitchWay::Right)]);
    let code = |code: u32| SwitchRequest { code, ..Default::default() };
    assert_eq!(n.receive_radio(DVec3::ZERO, &code(158)), 1);
    assert!(n.switch_branch(0));
    n.end_step(3.0);
    assert_eq!(n.receive_radio(DVec3::ZERO, &code(160)), 1);
    assert!(!n.switch_branch(0));
    n.end_step(3.0);
    // a code it does not know leaves it alone; right is the straight way here
    assert_eq!(n.receive_radio(DVec3::ZERO, &code(999)), 0);
    assert_eq!(n.receive_radio(DVec3::ZERO, &code(7)), 0);
    n.end_step(3.0);
    assert_eq!(n.receive_radio(DVec3::ZERO, &SwitchRequest { radio: -1, code: 160, ..Default::default() }), 1);
    assert!(n.switch_branch(0));
}

#[test]
fn the_map_chooses_only_the_overhead_lines_sounds() {
    let n = WireNet::parse("[trolley_sounds]\nhiss off\nsnap off\nfrog sounds/frog clatter.wav\nclick off\n[trolley_span]\np 0 0 5\np 50 0 5\n");
    assert_eq!(n.sounds, WireSounds { frog: SoundChoice::File("sounds/frog clatter.wav".into()), switch: SoundChoice::Off });
    assert_eq!(n.spans.len(), 1);
}

#[test]
fn switch_sound_is_map_defined_and_only_queued_when_the_frog_moves() {
    let net = radio_net("channel 1\n[trolley_sounds]\nswitch sounds/switch.wav\n");
    assert_eq!(net.sounds.switch, SoundChoice::File("sounds/switch.wav".into()));
    assert!(net.take_switch_sounds().is_empty());
    let request = SwitchRequest { radio: -1, channel: 1, ..Default::default() };
    assert_eq!(net.receive_radio(DVec3::ZERO, &request), 1);
    assert_eq!(net.take_switch_sounds(), vec![net.nodes[net.switches[0].node as usize].pos]);
    assert_eq!(net.receive_radio(DVec3::ZERO, &request), 0);
    assert_eq!(net.receive_radio(DVec3::ZERO, &SwitchRequest { radio: 1, channel: 2, ..Default::default() }), 0);
    assert!(net.take_switch_sounds().is_empty());
    net.set_switch(0, false);
    net.set_switch(0, false);
    assert_eq!(net.take_switch_sounds().len(), 1);
    for index in 0..300 {
        net.set_switch(0, index % 2 == 0);
    }
    assert_eq!(net.take_switch_sounds().len(), 256);
}

#[test]
fn a_radio_switch_on_a_channel_hears_only_that_channel() {
    let n = radio_net("channel 3\n");
    assert_eq!(n.switches[0].channel, Some(3));
    assert_eq!(n.receive_radio(DVec3::ZERO, &SwitchRequest { radio: -1, channel: 1, ..Default::default() }), 0);
    assert_eq!(n.receive_radio(DVec3::ZERO, &SwitchRequest { radio: -1, channel: 3, ..Default::default() }), 1);
}

#[test]
fn voltage_sags_with_the_current_drawn() {
    let n = y_net();
    let v0 = n.section_volts(0, 0.0);
    let v1 = n.section_volts(0, 300.0);
    assert!(v1 < v0 && v0 <= 600.0);
    assert!((v0 - v1 - 300.0 * 0.05).abs() < 1e-3);
}

/// Two wires that cross without joining: an overhead crossing. Where two spans meet at a
/// node they are joined, and that is not a crossing.
#[test]
fn two_wires_that_cross_are_a_crossing() {
    // a wire along x, and another crossing it at x = 30
    let net = WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np -40 0 5.5\np 40 0 5.5\n\
         [trolley_span]\nsection 1\np 30 -40 5.5\np 30 40 5.5\n",
    );
    assert_eq!(net.crossings.len(), 4, "{:?}", net.crossings);
    // Each conductor on one line crosses both conductors on the perpendicular line.
    assert_eq!(net.crossings.iter().filter(|c| c.a == 0 && c.side_a == 0).count(), 2);
    assert_eq!(net.crossings.iter().filter(|c| c.a == 0 && c.side_a == 1).count(), 2);
    assert_eq!(net.crossings_of(0).len(), 4);
    assert_eq!(net.crossings_of(1).len(), 4);
}

/// The physical wires cross at a shared centerline point, not just when the centerlines cross
/// in the middle of segments.
#[test]
fn two_wires_crossing_at_a_shared_node_are_a_crossing() {
    let net = WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np -40 0 5.5\np 0 0 5.5\np 40 0 5.5\n\
         [trolley_span]\nsection 1\np 0 -40 5.5\np 0 0 5.5\np 0 40 5.5\n",
    );
    assert_eq!(net.crossings.len(), 4, "{:?}", net.crossings);
}

/// A wire that passes above another is not a crossing: it flies over it.
#[test]
fn a_wire_above_another_is_not_a_crossing() {
    let net = WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np -40 0 5.5\np 40 0 5.5\n\
         [trolley_span]\nsection 1\np 30 -40 7.5\np 30 40 7.5\n",
    );
    assert!(net.crossings.is_empty(), "{:?}", net.crossings);
}

/// Physical conductor crossings are found when their spans also share a separate node.
#[test]
fn a_crossing_where_the_spans_meet_elsewhere_is_a_crossing() {
    let net = WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n\
         [trolley_span]\nsection 1\np -40 0 5.5\np 40 0 5.5\np 40 20 5.5\n\
         [trolley_span]\nsection 1\np 40 20 5.5\np 0 20 5.5\np 0 -40 5.5\n",
    );
    assert_eq!(net.crossings.iter().filter(|c| c.pos.x.abs() < 1.0 && c.pos.y.abs() < 1.0).count(), 4, "{:?}", net.crossings);
}
