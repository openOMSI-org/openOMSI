use super::*;

#[test]
fn the_host_checks_and_limits_what_it_relays() {
    let mut host = LanSession::host(27890, "host", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let mut a =
        LanSession::join(&port.to_string(), "a", world("m"), Duration::from_secs(1)).unwrap();
    let mut b =
        LanSession::join(&port.to_string(), "b", world("m"), Duration::from_secs(1)).unwrap();
    let poses = [pose(1.0), pose(2.0), pose(3.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 80, |s| {
        s[1].connected && s[2].connected && s[2].peer_count() == 2
    });
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    // somebody who never joined, or who pretends to be A, is not relayed
    let stranger = raw();
    stranger.send_to(&state_of(a.my_id, 777.0), at).unwrap();
    stranger.send_to(&state_of(99, 778.0), at).unwrap();
    stranger
        .send_to(
            format!("INFO|{}|mallory|Vehicles/evil.bus|||||11|2.5|0|0", a.my_id).as_bytes(),
            at,
        )
        .unwrap();
    // B sends a state under A's id
    b.send(&state_of(a.my_id, 779.0), at);
    // A floods: a thousand states in no time (each newer than the one before)
    let before = b.received;
    for i in 0..1000u16 {
        let mut p = pose(1000.0 + i as f64);
        p.id = a.my_id;
        a.send(
            &wire::encode_state(&p, PROTOCOL as u8, a.seq.wrapping_add(1 + i)),
            at,
        );
    }
    for _ in 0..5 {
        host.tick(0.001, &poses[0]);
    }
    std::thread::sleep(Duration::from_millis(30));
    for _ in 0..3 {
        b.tick(0.001, &poses[2]);
    }
    let relayed = (b.received - before) / state_of(2, 1.0).len() as u64;
    assert!(relayed <= 60, "{relayed} of a thousand states relayed");
    assert!(
        b.peers().all(|p| (p.pose.x - 777.0).abs() > 0.5
            && (p.pose.x - 778.0).abs() > 0.5
            && (p.pose.x - 779.0).abs() > 0.5),
        "no spoofed state"
    );
    assert!(b.peers().all(|p| p.pose.name != "mallory"));
    assert!(host.peers().any(|p| p.dropped > 0));
}

#[test]
fn idle_buses_send_less() {
    let host = raw();
    let mut c =
        LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    host.send_to(welcome_msg(4).as_bytes(), to_client(&c))
        .unwrap();
    // (the simulated seconds pass quickly: the silent host must not time out meanwhile)
    host.set_nonblocking(true).unwrap();
    let still = pose(5.0);
    let mut buf = [0u8; 2048];
    let mut count = |c: &mut LanSession, frames: usize, moving: bool| -> usize {
        let mut states = 0;
        let mut p = still.clone();
        for i in 0..frames {
            if moving {
                p.x += 0.1 * i as f64;
            }
            c.tick(0.01, &p);
            while let Ok((n, _)) = host.recv_from(&mut buf) {
                if buf[0] == wire::STATE_MAGIC && n > 0 {
                    states += 1;
                }
            }
        }
        states
    };
    // two simulated seconds standing still: the first second at full rate, then five a second
    let standing = count(&mut c, 200, false);
    assert!(c.connected);
    // two simulated seconds driving
    let driving = count(&mut c, 200, true);
    assert!(
        (34..=46).contains(&driving),
        "{driving} states while driving"
    );
    assert!(standing < 30, "{standing} states while standing");
}

#[test]
fn a_player_without_a_bus_sends_a_heartbeat() {
    let host = raw();
    let mut c = LanSession::join_addr(
        vec![host.local_addr().unwrap()],
        None,
        "observer",
        world("m"),
    )
    .unwrap();
    host.send_to(welcome_msg(4).as_bytes(), to_client(&c))
        .unwrap();
    let none = Pose::default();
    let mut buf = [0u8; 1024];
    let mut beats = 0;
    // four seconds of frames without a bus
    for _ in 0..40 {
        c.tick(0.1, &none);
        while let Ok((n, _)) = host.recv_from(&mut buf) {
            if let Some((4, _, p)) = wire::decode_state(&buf[..n], PROTOCOL as u8) {
                assert_eq!(p.flags & FLAG_VEHICLE, 0);
                beats += 1;
            }
        }
    }
    assert!(c.connected);
    assert!(
        (3..=5).contains(&beats),
        "about one empty state a second, got {beats}"
    );
}
