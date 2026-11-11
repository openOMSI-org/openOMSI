use super::*;

/// A bus standing free at `x`, under the parked number `k`.
fn parked(k: u32, x: f64) -> Pose {
    Pose { id: FIRST_PARKED_ID + k, name: String::new(), speed_kmh: 0.0, content: format!("v2-{}", "ab".repeat(32)), ..pose(x) }
}

/// The host (no bus of its own) and a client in a bus, until `until` or `rounds` are over.
fn run(host: &mut LanSession, c: &mut LanSession, rounds: usize, until: impl Fn(&LanSession) -> bool) -> Vec<u32> {
    let mut gone = Vec::new();
    for _ in 0..rounds {
        host.tick(0.06, &Pose::default());
        host.tell_parked(0.06);
        gone.extend(c.tick(0.06, &pose(7.0)));
        if until(c) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    gone
}

#[test]
fn a_player_sees_the_buses_parked_in_the_hosts_world_until_one_is_taken() {
    let mut host = LanSession::host(28610, "server", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    host.set_parked(vec![parked(0, 300.0), parked(1, 400.0), pose(500.0)]);
    let mut c = LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
    // (a player who joins later hears them as well: they are told again and again)
    run(&mut host, &mut c, 200, |c| find(c, 300.0).is_some() && find(c, 400.0).is_some());
    let bus = find(&c, 300.0).expect("the parked bus");
    assert_eq!((bus.pose.id, bus.pose.has_vehicle()), (FIRST_PARKED_ID, true));
    assert_eq!(bus.pose.flags, FLAG_VEHICLE, "it stands with everything off");
    assert!(bus.pose.content.starts_with("v2-") && bus.pose.bus == "Vehicles/x.bus");
    assert!(is_parked(bus.pose.id) && !is_parked(c.my_id));
    // (a pose with a player's number is no parked bus)
    assert!(find(&c, 500.0).is_none());
    // a player takes the first: it is gone from the others' games at once, the other stays
    host.set_parked(vec![parked(1, 400.0)]);
    let gone = run(&mut host, &mut c, 200, |c| find(c, 300.0).is_none());
    assert_eq!(gone, vec![FIRST_PARKED_ID]);
    assert!(find(&c, 400.0).is_some());
}

#[test]
fn a_parked_bus_is_not_forgotten_while_it_stands() {
    let mut host = LanSession::host(28611, "server", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    host.set_parked(vec![parked(0, 300.0)]);
    let mut c = LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
    c.timeout = Duration::from_millis(1500);
    let t0 = Instant::now();
    let gone = run(&mut host, &mut c, 100_000, |_| t0.elapsed() > Duration::from_millis(4000));
    assert!(gone.is_empty() && find(&c, 300.0).is_some(), "{gone:?}");
}

#[test]
fn a_joining_player_is_told_the_parked_buses_where_its_bus_would_stand() {
    let mut host = LanSession::host(28612, "server", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    host.set_parked(vec![parked(0, 100.0), parked(1, 5000.0)]);
    let mut c = LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
    run(&mut host, &mut c, 80, |c| c.connected);
    c.request_near(Footprint { x: 101.0, y: 2.0, z: 3.0, heading: 90.0, length: 12.0, width: 2.5 });
    run(&mut host, &mut c, 80, |c| c.near.is_some());
    let near = c.near.as_ref().expect("near");
    assert_eq!(near.len(), 1, "{near:?}");
    assert!((near[0].x - 100.0).abs() < 1.0, "{near:?}");
}

#[test]
fn no_player_gets_a_parked_buss_number() {
    let mut host = LanSession::host(28613, "server", world("m"), true).unwrap();
    host.next_id = FIRST_PARKED_ID - 1;
    assert_eq!(host.fresh_id(), FIRST_PARKED_ID - 1);
    assert_eq!(host.fresh_id(), 2);
}

#[test]
fn a_player_is_told_the_parked_buses_round_it_not_those_across_the_map() {
    let mut host = LanSession::host(28614, "server", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    // more of them than a game keeps players, most of them far away
    let mut buses = vec![parked(0, 300.0)];
    buses.extend((1..120).map(|k| parked(k, 5000.0 + k as f64 * 20.0)));
    host.set_parked(buses);
    let mut c = LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
    run(&mut host, &mut c, 200, |c| find(c, 300.0).is_some());
    // (some frames more: nothing else comes)
    run(&mut host, &mut c, 60, |_| false);
    let told: Vec<u32> = c.peers().map(|p| p.pose.id).filter(|id| is_parked(*id)).collect();
    assert_eq!(told, vec![FIRST_PARKED_ID]);
}

#[test]
fn parked_buses_never_take_a_players_place_in_a_game() {
    let host = raw();
    let mut c = LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    host.send_to(welcome_msg(4).as_bytes(), to_client(&c)).unwrap();
    c.tick(0.1, &pose(0.0));
    let tell = |c: &mut LanSession, id: u32| {
        host.send_to(&state_of(id, id as f64), to_client(c)).unwrap();
        c.tick(0.05, &pose(0.0));
    };
    // more parked buses than a game keeps beside the players' places: the rest is left out
    for k in 0..40 {
        tell(&mut c, FIRST_PARKED_ID + k);
    }
    let parked = |c: &LanSession| c.peers().filter(|p| is_parked(p.pose.id)).count();
    assert_eq!(parked(&c), 30);
    // players fill the session: when it is full, each new one takes a parked bus's place
    for id in 100..160 {
        tell(&mut c, id);
    }
    assert_eq!(c.peers().filter(|p| !is_parked(p.pose.id)).count(), 60, "every player is there");
    assert_eq!(c.peers().count(), MAX_PEERS);
    assert_eq!(parked(&c), 4);
    // and no parked bus comes back while the players need the room
    tell(&mut c, FIRST_PARKED_ID + 50);
    assert_eq!(parked(&c), 4);
}

#[test]
fn parked_buses_are_not_counted_among_the_other_players() {
    let mut host = LanSession::host(28615, "server", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    host.set_parked(vec![parked(0, 300.0), parked(1, 400.0)]);
    let mut c = LanSession::join(&port.to_string(), "c", world("m"), Duration::from_secs(1)).unwrap();
    run(&mut host, &mut c, 200, |c| find(c, 300.0).is_some() && find(c, 400.0).is_some());
    // (the server itself, and no other player: the two buses standing there are nobody)
    assert_eq!(c.peers().count() - c.peer_count(), 2);
    assert_eq!(c.peer_count(), c.peers().filter(|p| !is_parked(p.pose.id)).count());
}
