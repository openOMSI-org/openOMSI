//! The coupled cars' own variables (`vars::CAR_VARS_MAGIC`) between real UDP sessions.
use super::*;

/// A client's train sends its rear cab's switch (and a string of it) on the cars' channel:
/// the host and the other client get them by the client's id and table, the host passing them
/// on, and none of them comes in as the leading vehicle's variables. Changed later, the
/// change arrives too, and a string acknowledged on this channel is not taken for another's.
#[test]
fn a_trains_rear_cab_variables_reach_the_others_on_their_own_channel() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |s| {
        s[1].connected && s[2].connected && s[0].peers.len() == 2
    });
    let mut floats = vec![0.0f32; 40];
    let strings = vec!["Depot".to_string(), "x".repeat(3000)];
    let mut got: [std::collections::HashMap<u16, f32>; 2] = Default::default();
    let mut texts: [std::collections::HashMap<u16, String>; 2] = Default::default();
    for step in 0..60 {
        floats[7] = if step < 30 { 1.0 } else { 0.0 };
        a.send_car_vars(0xCA75, &floats, &strings, 0.1);
        host.tick(0.1, &poses[0]);
        a.tick(0.1, &poses[1]);
        b.tick(0.1, &poses[2]);
        let a_id = a.my_id;
        for (k, session) in [&mut host, &mut b].into_iter().enumerate() {
            assert!(
                session.take_vars().iter().all(|v| v.id != a_id),
                "not the leading vehicle's"
            );
            for v in session.take_car_vars() {
                assert_eq!((v.id, v.table), (a_id, 0xCA75));
                got[k].extend(v.floats);
                texts[k].extend(v.strings);
            }
        }
        if step == 29 {
            for k in 0..2 {
                assert_eq!(got[k].get(&7), Some(&1.0), "the switch, at {k}");
                assert_eq!(got[k].len(), floats.len());
            }
        }
    }
    for k in 0..2 {
        assert_eq!(got[k].get(&7), Some(&0.0), "turned off again, at {k}");
        assert_eq!(texts[k].get(&0).map(String::as_str), Some("Depot"));
        assert_eq!(
            texts[k].get(&1),
            Some(&strings[1]),
            "a long string in fragments"
        );
    }
}
