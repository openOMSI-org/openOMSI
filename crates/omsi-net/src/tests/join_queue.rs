//! Join-admission regression tests, including burst loads and untrusted sources.

use super::*;

#[test]
fn one_ip_cannot_fill_the_whole_waiting_queue() {
    let mut host = LanSession::host(0, "host", world("m"), false).unwrap();
    // Seed queued entries so admission stays in FIFO mode without creating peers.
    for port in 10000..10032 {
        host.waiting_joins.push_back((SocketAddr::from(([127, 0, 0, 1], port)), Instant::now()));
    }
    assert!(host.defer_new_join(SocketAddr::from(([127, 0, 0, 1], 11000)), true));
    assert_eq!(host.waiting_joins.len(), 32);
    assert!(host.defer_new_join(SocketAddr::from(([127, 0, 0, 2], 11000)), true));
    assert_eq!(host.waiting_joins.len(), 33);
}

#[test]
fn legacy_client_is_told_to_retry_instead_of_silently_ignoring_wait() {
    let mut host = LanSession::host(0, "host", world("m"), false).unwrap();
    host.waiting_joins.push_back((SocketAddr::from(([127, 0, 0, 2], 10000)), Instant::now()));
    let client = raw();
    assert!(host.defer_new_join(SocketAddr::from((Ipv4Addr::LOCALHOST, client.local_addr().unwrap().port())), false));
    let mut buf = [0; 1024];
    let (n, _) = client.recv_from(&mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).contains("try again in a minute"));
}

#[test]
fn configured_player_capacity_limits_new_remote_peers() {
    let mut host = LanSession::host(0, "dedicated", world("m"), false).unwrap();
    host.set_peer_limit(1);
    assert_eq!(host.max_peers, 1);
    let at = to_client(&host);
    let hello = format!("HELLO|{PROTOCOL}|-|p|Vehicles/x.bus|m|1989-05-30|32400|||||||wait-v1");
    let first = raw();
    first.send_to(hello.as_bytes(), at).unwrap();
    host.tick(0.02, &pose(0.0));
    assert_eq!(host.peer_count(), 1);

    let second = raw();
    second.send_to(hello.as_bytes(), at).unwrap();
    host.tick(0.02, &pose(0.0));
    assert_eq!(host.peer_count(), 1, "the configured limit rejects new joins");

    host.set_peer_limit(100);
    assert_eq!(host.max_peers, MAX_PEERS);
    host.set_peer_limit(0);
    assert_eq!(host.max_peers, 1);
}

#[test]
fn too_many_loading_players_wait() {
    let mut host = LanSession::host(28180, "host", world("m"), true).unwrap();
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, host.local_addr().unwrap().port()));
    let hello = format!("HELLO|{PROTOCOL}|-|p|Vehicles/x.bus|m|1989-05-30|32400|||||||wait-v1");
    let players: Vec<UdpSocket> = (0..MAX_JOINING + 1).map(|_| raw()).collect();
    for p in &players {
        p.send_to(hello.as_bytes(), at).unwrap();
    }
    for _ in 0..10 {
        host.tick(0.01, &pose(0.0));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.peer_count(), MAX_JOINING);
    let mut buf = [0u8; 512];
    let mut answers = Vec::new();
    while let Ok((n, _)) = players[MAX_JOINING].recv_from(&mut buf) {
        answers.push(String::from_utf8_lossy(&buf[..n]).to_string());
    }
    assert!(
        answers
            .iter()
            .any(|a| a.starts_with(&format!("WAIT|{PROTOCOL}|2"))),
        "{answers:?}"
    );
}

#[test]
fn twenty_simultaneous_hellos_eventually_admit_every_player() {
    let mut host = LanSession::host(0, "dedicated", world("m"), false).unwrap();
    host.set_peer_limit(20);
    let at = to_client(&host);
    let hello = format!("HELLO|{PROTOCOL}|-|p|Vehicles/x.bus|m|1989-05-30|32400|||||||wait-v1");
    let clients: Vec<UdpSocket> = (0..20).map(|_| raw()).collect();
    let mut welcomed = [false; 20];
    let mut saw_wait = false;
    let mut buf = [0u8; 2048];

    // Send the first batch together rather than pacing clients one by one.
    for client in &clients {
        client.send_to(hello.as_bytes(), at).unwrap();
    }
    for _ in 0..150 {
        host.tick(0.02, &pose(0.0));
        for (i, client) in clients.iter().enumerate() {
            while let Ok((n, _)) = client.recv_from(&mut buf) {
                let message = String::from_utf8_lossy(&buf[..n]);
                if message.starts_with("WAIT|") {
                    saw_wait = true;
                }
                if let Some(id) = message
                    .strip_prefix(&format!("WELCOME|{PROTOCOL}|"))
                    .and_then(|rest| rest.split('|').next())
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    welcomed[i] = true;
                    client.send_to(&state_of(id, i as f64), at).unwrap();
                }
            }
            if !welcomed[i] {
                client.send_to(hello.as_bytes(), at).unwrap();
            }
        }
        if welcomed.iter().all(|done| *done) && host.peer_count() == 20 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(saw_wait, "the loading limit must temporarily defer a burst");
    assert!(welcomed.iter().all(|done| *done), "welcome state: {welcomed:?}");
    assert_eq!(host.peer_count(), 20);
}

#[test]
fn burst_waiters_are_admitted_in_first_in_first_out_order() {
    let mut host = LanSession::host(0, "host", world("m"), false).unwrap();
    host.set_peer_limit(20);
    for id in 100..100 + MAX_JOINING as u32 {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, id as u16 + 30000));
        host.peers.insert(id, Peer::new(Some(addr), Pose::default()));
    }
    let a = SocketAddr::from((Ipv4Addr::LOCALHOST, 42001));
    let b = SocketAddr::from((Ipv4Addr::LOCALHOST, 42002));
    let hello = format!("HELLO|{PROTOCOL}|-|queued|Vehicles/x.bus|m|1989-05-30|32400|||||||wait-v1");
    let fields: Vec<&str> = hello.split('|').collect();
    host.on_hello(&fields, a);
    host.on_hello(&fields, b);
    assert_eq!(host.waiting_joins.len(), 2);
    assert_eq!(host.waiting_joins.front().unwrap().0, a);

    // One incumbent finishes loading. B cannot skip the earlier waiter A.
    host.peers.get_mut(&100).unwrap().has_pose = true;
    host.on_hello(&fields, b);
    assert_eq!(host.peer_count(), MAX_JOINING);
    assert_eq!(host.waiting_joins.front().unwrap().0, a);
    host.on_hello(&fields, a);
    assert_eq!(host.peer_count(), MAX_JOINING + 1);
    assert_eq!(host.waiting_joins.front().unwrap().0, b);

    host.peers.get_mut(&101).unwrap().has_pose = true;
    host.on_hello(&fields, b);
    assert_eq!(host.peer_count(), MAX_JOINING + 2);
    assert!(host.waiting_joins.is_empty());
}

#[test]
fn unrecognized_source_cannot_inject_wait_reject_or_welcome() {
    let intended = raw();
    let from = intended.local_addr().unwrap();
    let attacker = raw();
    let mut client =
        LanSession::join(&from.to_string(), "player", world("m"), Duration::from_secs(10)).unwrap();
    let target = to_client(&client);
    for text in [
        format!("WAIT|{PROTOCOL}|2"),
        format!("REJECT|{PROTOCOL}|permanent denial"),
        format!("WELCOME|{PROTOCOL}|2|AAAAAAAAAAAA|attacker|m|1989-05-30|32400||||"),
    ] {
        attacker.send_to(text.as_bytes(), target).unwrap();
        client.tick(0.01, &pose(0.0));
    }
    assert!(!client.waiting_for_slot);
    assert!(!client.connected);
    assert!(client.rejected.is_none());
    assert!(client.turned_away.is_none());
}

#[test]
fn a_busy_host_keeps_the_client_waiting_without_permanent_rejection() {
    let host = raw();
    let at = host.local_addr().unwrap();
    let mut client =
        LanSession::join(&at.to_string(), "waiting", world("m"), Duration::from_secs(10)).unwrap();
    let from = to_client(&client);
    host.send_to(format!("WAIT|{PROTOCOL}|2").as_bytes(), from).unwrap();
    client.tick(0.02, &pose(0.0));
    assert!(client.waiting_for_slot);
    assert!(client.rejected.is_none());
    assert!(client.turned_away.is_none());
    assert!(!client.connected);

    // A different socket cannot put the client in the wait state.
    let other = raw();
    let mut fresh =
        LanSession::join(&at.to_string(), "fresh", world("m"), Duration::from_secs(10)).unwrap();
    other.send_to(format!("WAIT|{PROTOCOL}|2").as_bytes(), to_client(&fresh)).unwrap();
    fresh.tick(0.02, &pose(0.0));
    assert!(!fresh.waiting_for_slot);
}

