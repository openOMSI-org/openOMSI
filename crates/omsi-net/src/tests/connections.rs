use super::*;
use crate::receive_budget::{receive_budget_reached, RECEIVE_PACKET_LIMIT, RECEIVE_TIME_BUDGET};

#[test]
fn players_without_a_bus_stay_in_the_session() {
    let mut host = LanSession::host(28190, "host", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let mut c =
        LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
    for s in [&mut host, &mut c] {
        s.timeout = Duration::from_millis(1000);
        s.heartbeat = 0.2;
    }
    let none = Pose::default();
    let t0 = Instant::now();
    let mut last = Instant::now();
    let mut id = None;
    while t0.elapsed() < Duration::from_millis(3500) {
        let dt = last.elapsed().as_secs_f32();
        last = Instant::now();
        host.tick(dt, &none);
        let gone = c.tick(dt, &none);
        if c.connected {
            assert_eq!(
                *id.get_or_insert(c.my_id),
                c.my_id,
                "the client was never dropped and let in again"
            );
        }
        if t0.elapsed() > Duration::from_millis(500) {
            assert!(
                c.connected,
                "the client stays connected at {:?}",
                t0.elapsed()
            );
            assert_eq!(
                host.peer_count(),
                1,
                "the host keeps the client at {:?}",
                t0.elapsed()
            );
            assert!(gone.is_empty(), "nobody left: {gone:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // the observer's empty state (at 0,0) is not a vehicle a new player must keep clear of
    assert!(host.peers().all(|p| !p.pose.has_vehicle()));
    let mut d =
        LanSession::join(&port.to_string(), "d", world("m"), Duration::from_secs(1)).unwrap();
    pump(
        &mut [&mut host, &mut c, &mut d],
        &[none.clone(), none.clone(), none.clone()],
        80,
        |s| s[2].connected,
    );
    d.request_near(Footprint {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        heading: 0.0,
        length: 12.0,
        width: 2.5,
    });
    pump(
        &mut [&mut host, &mut c, &mut d],
        &[none.clone(), none.clone(), none.clone()],
        80,
        |s| s[2].near.is_some(),
    );
    assert!(d.near.as_ref().expect("near").is_empty());
}

#[test]
fn a_loading_player_is_kept() {
    let mut host = LanSession::host(27880, "host", world("m"), true).unwrap();
    host.timeout = Duration::from_millis(200);
    host.load_timeout = Duration::from_millis(900);
    let port = host.local_addr().unwrap().port();
    let mut c =
        LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
    pump(&mut [&mut host, &mut c], &[pose(0.0), pose(1.0)], 40, |s| {
        s[1].connected
    });
    // the client loads its map: no frames for longer than the plain timeout
    let quiet = Instant::now();
    while quiet.elapsed() < Duration::from_millis(500) {
        host.tick(0.02, &pose(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(host.peer_count(), 1, "kept while loading");
    while quiet.elapsed() < Duration::from_millis(1200) {
        host.tick(0.02, &pose(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(host.peer_count(), 0, "a player who never comes is dropped");
    assert!(host
        .take_events()
        .iter()
        .any(|e| matches!(e, LanEvent::Notice(n) if n == "c lost the connection")));
}

#[test]
fn discovery_finds_a_host() {
    let mut host = LanSession::host(DEFAULT_PORT + 5, "finder", world("m"), false).unwrap();
    let h = std::thread::spawn(move || {
        for _ in 0..40 {
            host.tick(0.05, &pose(0.0));
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    let found =
        LanSession::discover(DEFAULT_PORT, "c", world("m"), Duration::from_secs(2)).unwrap();
    h.join().unwrap();
    assert!(found.is_some());
}

#[test]
fn a_join_nobody_answers_gives_up_with_a_message() {
    // an address where nothing answers (TEST-NET-1: never routed)
    let mut c = LanSession::join("192.0.2.1:27015", "c", world("m"), Duration::from_millis(10))
        .unwrap();
    c.join_timeout = Duration::from_millis(400);
    let t0 = Instant::now();
    while c.rejected.is_none() && t0.elapsed() < Duration::from_secs(3) {
        c.tick(0.05, &Pose::default());
        std::thread::sleep(Duration::from_millis(20));
    }
    let why = c.rejected.clone().expect("gave up");
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
    assert!(why.contains("no answer from 192.0.2.1:27015"), "{why}");
    assert!(why.contains("firewall"), "{why}");
    assert!(!c.connected);
    // and it stops saying hello
    let sent = c.sent();
    for _ in 0..30 {
        c.tick(0.1, &Pose::default());
    }
    assert_eq!(c.sent(), sent);
}

#[test]
fn a_code_with_a_dead_address_still_joins_by_the_live_one() {
    let mut host = LanSession::host(28200, "host", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    // the first address leads nowhere (a VPN that is down), the second is the host
    let code = SessionCode {
        protocol: PROTOCOL as u8,
        ips: vec![Ipv4Addr::new(192, 0, 2, 1), Ipv4Addr::LOCALHOST],
        port,
        session: host.session,
    };
    let mut c = LanSession::join(&code.encode(), "c", world("m"), Duration::from_millis(10))
        .unwrap();
    assert_eq!(c.candidates.len(), 2);
    assert_eq!(c.host, None);
    pump(&mut [&mut host, &mut c], &[pose(1.0), pose(2.0)], 100, |s| {
        find(s[0], 2.0).is_some() && find(s[1], 1.0).is_some()
    });
    assert!(c.connected);
    assert_eq!(c.host, Some(SocketAddr::from((Ipv4Addr::LOCALHOST, port))));
    assert_eq!(host.peer_count(), 1);
}

/// The id a raw socket's hello is welcomed with.
fn hello_id(s: &UdpSocket, host: &mut LanSession, name: &str, nonce: u64) -> Option<u32> {
    let port = host.local_addr().unwrap().port();
    let msg = format!("HELLO|{PROTOCOL}|-|{name}||m|1989-05-30|32400|||{nonce:016x}");
    let mut buf = [0u8; 1500];
    for _ in 0..40 {
        let _ = s.send_to(msg.as_bytes(), ("127.0.0.1", port));
        host.tick(0.02, &pose(0.0));
        while let Ok((n, _)) = s.recv_from(&mut buf) {
            let t = String::from_utf8_lossy(&buf[..n]).to_string();
            if let Some(rest) = t.strip_prefix("WELCOME|") {
                return rest.split('|').nth(1).and_then(|x| x.parse().ok());
            }
        }
    }
    None
}

#[test]
fn a_returning_player_is_known_by_its_nonce_not_its_name() {
    let mut host = LanSession::host(27870, "host", world("m"), true).unwrap();
    host.timeout = Duration::from_millis(100);
    host.load_timeout = Duration::from_millis(100);
    let first = hello_id(&raw(), &mut host, "alice", 0x1234).expect("welcomed");
    // alice's game goes quiet and is dropped
    let quiet = Instant::now();
    while host.peer_count() > 0 && quiet.elapsed() < Duration::from_secs(3) {
        host.tick(0.02, &pose(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(host.peer_count(), 0);
    // somebody else calling themselves alice gets a number of their own
    let other = hello_id(&raw(), &mut host, "alice", 0x9999).expect("welcomed");
    assert_ne!(other, first, "a name alone takes nobody's place");
    // alice herself (her nonce) comes back to her number
    let back = hello_id(&raw(), &mut host, "alice", 0x1234).expect("welcomed");
    assert_eq!(back, first);
}

/// Ticks a client (a second at a time, so that the slow hello comes round) and a host until
/// the client is connected.
// Every test that hosts on a fixed port has its own, ten apart: `LanSession::host` with
// `try_next` moves on to the next free one of `PORT_RANGE` (10), and two tests whose ports
// fell in one range could end up talking to each other's host when run in parallel.
fn until_connected(c: &mut LanSession, host: &mut LanSession) -> bool {
    let t0 = Instant::now();
    while !c.connected && t0.elapsed() < Duration::from_secs(3) {
        c.tick(1.0, &Pose::default());
        host.tick(0.05, &pose(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    c.connected
}

#[test]
fn a_client_that_gave_up_comes_back_when_the_host_does() {
    let port = 28210;
    // nobody hosts yet: the client gives up
    let mut c = LanSession::join(&format!("127.0.0.1:{port}"), "c", world("m"), Duration::from_millis(10))
        .unwrap();
    c.join_timeout = Duration::from_millis(300);
    let t0 = Instant::now();
    while c.rejected.is_none() && t0.elapsed() < Duration::from_secs(3) {
        c.tick(0.05, &Pose::default());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(c.rejected.is_some() && !c.connected);
    // the host starts: the client's slow hello finds it and the give-up is taken back
    let mut host = LanSession::host(port, "host", world("m"), false).unwrap();
    assert!(until_connected(&mut c, &mut host));
    assert!(c.rejected.is_none());
    assert_eq!(c.welcomes, 1);
}

#[test]
fn reconnect_tries_again_after_the_host_sent_us_away() {
    let mut host = LanSession::host(28220, "host", world("m"), false).unwrap();
    let mut c = LanSession::join("127.0.0.1:28220", "c", world("m"), Duration::from_millis(10)).unwrap();
    assert!(until_connected(&mut c, &mut host));
    // the host sends us away: we stay out, with its reason
    host.kick(c.my_id, "test", false);
    // (until the word arrives: a fixed number of rounds was too few on a busy machine)
    let t0 = Instant::now();
    while (c.connected || c.rejected.is_none()) && t0.elapsed() < Duration::from_secs(3) {
        c.tick(1.0, &Pose::default());
        host.tick(0.05, &pose(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!c.connected);
    assert!(c.rejected.as_deref().is_some_and(|r| r.contains("test")));
    // the host's own word: the game ends on it (a lost connection is no such word)
    assert_eq!(c.turned_away.as_deref(), Some("test"));
    assert!(c.reconnect());
    assert!(c.rejected.is_none() && c.turned_away.is_none());
    assert!(until_connected(&mut c, &mut host));
    // a host has nothing to reconnect to
    assert!(!host.reconnect());
}

#[test]
fn a_banned_player_hears_why_at_the_door() {
    let mut host = LanSession::host(28230, "host", world("m"), false).unwrap();
    let mut c = LanSession::join("127.0.0.1:28230", "c", world("m"), Duration::from_millis(10)).unwrap();
    assert!(until_connected(&mut c, &mut host));
    host.kick(c.my_id, "Banni : conduite dangereuse", true);
    // (until the word arrives: a fixed number of rounds was too few on a busy machine)
    let t0 = Instant::now();
    while c.turned_away.is_none() && t0.elapsed() < Duration::from_secs(3) {
        c.tick(1.0, &Pose::default());
        host.tick(0.05, &pose(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(c.turned_away.as_deref(), Some("Banni : conduite dangereuse"));
    // back again: turned away at the door, with the same reason
    assert!(c.reconnect());
    let t0 = Instant::now();
    while c.turned_away.is_none() && t0.elapsed() < Duration::from_secs(3) {
        c.tick(1.0, &Pose::default());
        host.tick(0.05, &pose(0.0));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!c.connected);
    assert!(c.turned_away.as_deref().is_some_and(|r| r.contains("Banni : conduite dangereuse")), "{:?}", c.turned_away);
}

#[test]
fn a_season_with_its_phase_crosses_the_wire() {
    for season in ["autumn-late", "spring-early", "summer-late", "winter"] {
        let mut w = world("maps/Grundorf/global.cfg");
        w.season = season.into();
        let line = w.fields();
        let parts: Vec<&str> = line.split('|').collect();
        assert_eq!(WorldInfo::from_fields(&parts, 0).season, season);
    }
}

#[test]
fn repeated_passenger_requests_reach_the_vehicle_owner_and_ai_host() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |sessions|
        find(sessions[2], 200.0).is_some() && find(sessions[0], 300.0).is_some());
    assert!(a.connected && b.connected);
    let player_request = "pax_request player 0 entry 2";
    let ai_request = "pax_request ai 7 exit 1";
    for _ in 0..2 {
        a.command(b.my_id, player_request);
        a.command(host.my_id, ai_request);
        host.tick(0.2, &poses[0]);
        b.tick(0.2, &poses[2]);
        assert!(host.take_commands().contains(&(a.my_id, ai_request.to_owned())));
        assert!(b.take_commands().contains(&(a.my_id, player_request.to_owned())));
        // Discarding the first received state does not lose the active request:
        // the next refresh still carries the whole target and door index.
    }
    let false_source: SocketAddr = "127.0.0.1:29001".parse().unwrap();
    let message = format!("CMD|{}|{}|{player_request}", a.my_id, b.my_id);
    host.on_command(&message.split('|').collect::<Vec<_>>(), false_source);
    b.tick(0.2, &poses[2]);
    assert!(b.take_commands().is_empty(), "another address cannot request a door for this player");
}

#[test]
fn sound_events_relay_between_clients_and_host_ai_without_echo_or_spoofing() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port().to_string();
    let mut a = LanSession::join(&port, "a", world("m"), Duration::from_secs(1)).unwrap();
    let mut b = LanSession::join(&port, "b", world("m"), Duration::from_secs(1)).unwrap();
    let poses = [pose(1.0), pose(2.0), pose(3.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 80, |s| s.iter().all(|s| s.peers.len() == 2));
    assert_eq!(a.peers.len(), 2);
    let tl = sounds::Event { ai: 0, bus: "Vehicles/x.bus".into(), trigger: "door".into(), file: None, values: vec![("doorSpeed".into(), 5.0)] };
    let tf = sounds::Event { trigger: "announcement".into(), file: Some("..\\Announcements\\árvíztűrő.wav".into()), values: Vec::new(), ..tl.clone() };
    a.publish_sound(tl.clone()); a.publish_sound(tf.clone());
    let ai = sounds::Event { ai: 77, trigger: "horn".into(), ..tl.clone() };
    host.publish_sound(ai.clone());
    // A client cannot originate AI events.
    b.publish_sound(sounds::Event { ai: 88, ..tl.clone() });
    pump(&mut [&mut host, &mut a, &mut b], &poses, 40, |_| false);
    let hs = host.take_sounds(); let as_ = a.take_sounds(); let bs = b.take_sounds();
    assert_eq!(hs.iter().map(|s| &s.event).collect::<Vec<_>>(), vec![&tl, &tf]);
    assert!(hs.iter().all(|s| s.owner == a.my_id));
    assert_eq!(as_.len(), 1); assert_eq!(as_[0].event, ai);
    assert_eq!(bs.iter().filter(|s| s.owner == a.my_id).map(|s| &s.event).collect::<Vec<_>>(), vec![&tl, &tf]);
    assert!(bs.iter().any(|s| s.owner == 1 && s.event == ai)); assert_eq!(bs.len(), 3);
    // Authentication uses the socket address as well as the claimed player ID.
    let mut fake = sounds::Sync::default(); fake.publish(&[host.my_id], tl);
    for packet in fake.tick(PROTOCOL as u8, b.my_id, 0.0) {
        host.on_sound(&packet, host.peers[&a.my_id].addr.unwrap());
    }
    assert!(host.take_sounds().is_empty());
    pump(&mut [&mut host, &mut a, &mut b], &poses, 10, |_| false);
    assert!(host.take_sounds().is_empty()); assert!(a.take_sounds().is_empty()); assert!(b.take_sounds().is_empty());
}

#[test]
fn restarting_host_replaces_retained_versions_and_rejects_old_session_packets() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut client = LanSession::join_addr(vec![addr], None, "client", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0)];
    pump(&mut [&mut host, &mut client], &poses, 100, |s| s[1].connected && s[1].peers.contains_key(&1));
    let old_secret = client.rebind_secret.unwrap();
    let old_welcome = format!("WELCOME|{PROTOCOL}|{}|{}|host|{}|2|{}|{:016x}", client.my_id,
        session_hex(host.session), host.world.fields(), admission::secret_hex(&old_secret), client.nonce);
    let sound = |trigger: &str| sounds::Event { ai: 0, bus: "Vehicles/x.bus".into(),
        trigger: trigger.into(), file: None, values: vec![] };
    host.publish_sound(sound("old"));
    let old_sound = host.sounds.tick(PROTOCOL as u8, 1, 0.2).remove(0);
    pump(&mut [&mut host, &mut client], &poses, 40, |_| false);
    assert_eq!(client.take_sounds().len(), 1);
    // Seed versions beyond what a fresh process will initially send.
    for n in 0..10 {
        host.send_vars(99, &[], &[], &[0], &[format!("old {n}")], 0.2);
        host.send_var_schema(99, &format!("old schema {n}"), 0.2);
        client.tick(0.1, &poses[1]);
    }
    let mut old_tx = vars::VarSender::default();
    let old_packet = old_tx.tick(PROTOCOL as u8, 1, 99, &[], &[], &[0], &["stale".into()], 0.2).remove(0);
    drop(host);
    // Keep peer 1: deliberately do not tick long enough for retention to expire.
    let mut host = LanSession::host(addr.port(), "host", world("m"), false).unwrap();
    client.connected = false;
    client.hello_acc = 10.0;
    pump(&mut [&mut host, &mut client], &poses, 150, |s| s[1].connected && s[1].peers.contains_key(&1) && s[1].rebind_secret != Some(old_secret));
    assert!(client.connected);
    assert_ne!(client.rebind_secret, Some(old_secret));
    client.on_welcome(&old_welcome.split('|').collect::<Vec<_>>(), addr);
    assert_ne!(client.rebind_secret, Some(old_secret), "delayed welcome must not restore the old session");
    client.take_vars(); client.take_var_schemas();
    let mut stale = vec![0xEA]; stale.extend_from_slice(&old_secret); stale.extend_from_slice(&old_packet);
    host.socket.send_to(&stale, to_client(&client)).unwrap();
    let mut stale_sound = vec![0xEA]; stale_sound.extend_from_slice(&old_secret); stale_sound.extend_from_slice(&old_sound);
    host.socket.send_to(&stale_sound, to_client(&client)).unwrap();
    // The same obsolete connection credential is rejected in the reverse direction.
    client.socket.send_to(&stale, addr).unwrap();
    host.publish_sound(sound("fresh"));
    host.send_vars(99, &[], &[], &[0], &["fresh".into()], 0.2);
    host.send_var_schema(99, "fresh schema", 0.2);
    client.tick(0.1, &poses[1]); host.tick(0.1, &poses[0]);
    assert!(client.take_vars().iter().any(|v| v.strings == vec![(0, "fresh".into())]));
    assert!(client.take_var_schemas().iter().any(|v| v.strings == vec![(0, "fresh schema".into())]));
    assert!(host.take_vars().is_empty());
    pump(&mut [&mut host, &mut client], &poses, 30, |_| false);
    let sounds = client.take_sounds();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].event.trigger, "fresh");
}

#[test]
fn restarting_client_on_the_same_socket_invalidates_host_receive_state() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut client = LanSession::join_addr(vec![addr], None, "client", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0)];
    pump(&mut [&mut host, &mut client], &poses, 100, |s| s[1].connected && s[0].peers.values().any(|p| p.has_pose));
    let id = client.my_id;
    let old_nonce = client.nonce;
    let old_secret = client.rebind_secret.unwrap();
    for n in 0..5 {
        client.send_vars(99, &[], &[], &[0], &[format!("old {n}")], 0.2);
        client.send_var_schema(99, &format!("schema {n}"), 0.2);
        host.tick(0.1, &poses[0]);
    }
    host.take_vars(); host.take_var_schemas();
    client.nonce = random_session_id();
    client.rebind_secret = None;
    client.connected = false;
    client.var_sender = vars::VarSender::default();
    client.schema_sender = vars::VarSender::default();
    client.hello_acc = 10.0;
    pump(&mut [&mut host, &mut client], &poses, 100, |s| s[1].connected && s[1].rebind_secret != Some(old_secret));
    assert_eq!(client.my_id, id);
    assert_ne!(client.rebind_secret, Some(old_secret));
    // A late old HELLO must not change the credential back or reset the new stream.
    let hello = format!("HELLO|{PROTOCOL}|-|client||{}|{old_nonce:016X}", client.world.fields());
    host.on_hello(&hello.split('|').collect::<Vec<_>>(), to_client(&client));
    assert_eq!(host.peers[&id].nonce, Some(client.nonce));
    client.send_vars(99, &[], &[], &[0], &["new".into()], 0.2);
    client.send_var_schema(99, "new schema", 0.2);
    host.tick(0.1, &poses[0]);
    assert!(host.take_vars().iter().any(|v| v.strings == vec![(0, "new".into())]));
    assert!(host.take_var_schemas().iter().any(|v| v.strings == vec![(0, "new schema".into())]));
}

#[test]
fn each_host_owner_gets_its_own_identifier_of_the_machine() {
    let machine = "0123456789abcdef0123456789abcdef";
    let mut ids = Vec::new();
    for (first_port, realm) in [(28310, "aa".repeat(16)), (28320, "bb".repeat(16))] {
        let mut host = LanSession::host(first_port, "host", world("m"), true).unwrap();
        host.set_ban_realm(&realm);
        let port = host.local_addr().unwrap().port();
        let mut c = LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
        c.hardware_base = Some(machine.to_string());
        let none = Pose::default();
        let t0 = Instant::now();
        while !c.connected && t0.elapsed() < Duration::from_secs(4) {
            host.tick(0.05, &none);
            c.tick(0.05, &none);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(c.connected);
        let id = host.peer_hardware_id(c.my_id).unwrap().to_string();
        assert_eq!(id, host_hardware_id(machine, &realm));
        ids.push(id);
    }
    assert_ne!(ids[0], ids[1]);
    assert!(!ids.iter().any(|i| i == machine));
}

#[test]
fn receive_budget_bounds_each_tick() {
    assert!(!receive_budget_reached(0, Duration::ZERO));
    assert!(!receive_budget_reached(RECEIVE_PACKET_LIMIT - 1, Duration::ZERO));
    assert!(receive_budget_reached(RECEIVE_PACKET_LIMIT, Duration::ZERO));
    assert!(receive_budget_reached(0, RECEIVE_TIME_BUDGET));
}

#[test]
fn rate_limit_diagnostics_preserve_packet_categories() {
    let mut host = LanSession::host(0, "host", WorldInfo::default(), false).unwrap();
    let from = SocketAddr::from(([127, 0, 0, 1], 30000));
    host.peers.insert(2, Peer::new(Some(from), Pose::default()));
    let seen = host.peers[&2].last_seen;

    // Rejected flood packets must not renew liveness.
    assert!(host.checked_peer(2, from, (0.0, 0.0), PeerTraffic::Sounds).is_none());
    assert!(host.checked_peer(2, from, (0.0, 0.0), PeerTraffic::Variables).is_none());
    let peer = &host.peers[&2];
    assert_eq!(peer.dropped, 2);
    assert_eq!(peer.drop_counts[PeerTraffic::Sounds as usize], 1);
    assert_eq!(peer.drop_counts[PeerTraffic::Variables as usize], 1);
    assert_eq!(peer.drop_counts[PeerTraffic::State as usize], 0);
    assert_eq!(peer.last_seen, seen);
}
