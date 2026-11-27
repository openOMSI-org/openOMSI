// Isolated LAN admission and state-order regression tests.
use super::*;

#[test]
fn a_client_takes_a_limited_number_of_players() {
    let host = raw();
    let mut c =
        LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    host.send_to(welcome_msg(2).as_bytes(), to_client(&c))
        .unwrap();
    for id in 3..(3 + 3 * MAX_PEERS as u32) {
        host.send_to(&state_of(id, id as f64), to_client(&c))
            .unwrap();
    }
    for _ in 0..20 {
        c.tick(0.01, &pose(0.0));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(c.connected);
    assert_eq!(c.peer_count(), MAX_PEERS);
}

#[test]
fn old_states_do_not_overtake_new_ones() {
    let host = raw();
    let mut c =
        LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    host.send_to(welcome_msg(2).as_bytes(), to_client(&c))
        .unwrap();
    let mut p = pose(10.0);
    p.id = 1;
    host.send_to(&wire::encode_state(&p, PROTOCOL as u8, 500), to_client(&c))
        .unwrap();
    p.x = 5.0;
    host.send_to(&wire::encode_state(&p, PROTOCOL as u8, 499), to_client(&c))
        .unwrap();
    for _ in 0..10 {
        c.tick(0.01, &pose(0.0));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(c.peers().next().map(|p| p.pose.x), Some(10.0));
}

#[test]
fn a_full_session_turns_the_next_player_away() {
    let mut host = LanSession::host(28170, "host", world("m"), true).unwrap();
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, host.local_addr().unwrap().port()));
    let hello = format!("HELLO|{PROTOCOL}|-|p|Vehicles/x.bus|m|1989-05-30|32400||");
    let players: Vec<UdpSocket> = (0..MAX_PEERS).map(|_| raw()).collect();
    let mut buf = [0u8; 512];
    for (i, p) in players.iter().enumerate() {
        p.send_to(hello.as_bytes(), at).unwrap();
        host.tick(0.01, &pose(0.0));
        // everybody gets in only once the ones before them have loaded (a state arrived)
        let mut id = None;
        for _ in 0..20 {
            match p.recv_from(&mut buf) {
                Ok((n, _)) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    if let Some(rest) = text.strip_prefix(&format!("WELCOME|{PROTOCOL}|")) {
                        id = rest.split('|').next().and_then(|s| s.parse::<u32>().ok());
                        break;
                    }
                }
                Err(_) => {
                    host.tick(0.01, &pose(0.0));
                }
            }
        }
        let id = id.unwrap_or_else(|| panic!("player {i} welcomed"));
        p.send_to(&state_of(id, i as f64), at).unwrap();
        host.tick(0.01, &pose(0.0));
    }
    assert_eq!(host.peer_count(), MAX_PEERS);
    let late = raw();
    let mut answer = String::new();
    for _ in 0..40 {
        late.send_to(hello.as_bytes(), at).unwrap();
        host.tick(0.01, &pose(0.0));
        if let Ok((n, _)) = late.recv_from(&mut buf) {
            answer = String::from_utf8_lossy(&buf[..n]).to_string();
            if answer.starts_with("REJECT") {
                break;
            }
        }
    }
    assert!(
        answer.starts_with("REJECT|") && answer.contains("full"),
        "{answer}"
    );
    assert_eq!(host.peer_count(), MAX_PEERS);
}
