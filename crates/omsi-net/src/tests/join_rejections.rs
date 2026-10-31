use super::*;

fn portless_client() -> LanSession {
    LanSession::join("127.0.0.1", "client", world("m"), Duration::from_secs(10)).unwrap()
}

fn deliver_rejection(client: &mut LanSession, sender: &UdpSocket, reason: &str) {
    let target = SocketAddr::from((Ipv4Addr::LOCALHOST, client.local_addr().unwrap().port()));
    sender
        .send_to(format!("REJECT|{PROTOCOL}|{reason}").as_bytes(), target)
        .unwrap();
    let received = client.received;
    let deadline = Instant::now() + Duration::from_secs(1);
    while client.received == received && Instant::now() < deadline {
        client.receive(&mut Vec::new());
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        client.received > received,
        "the rejection datagram was received"
    );
}

#[test]
fn ten_ports_on_one_machine_report_rejections_immediately() {
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    for reason in [
        "session full",
        "different LAN protocol",
        "banned",
        "sent you away",
    ] {
        let mut client = portless_client();
        // Use an OS-assigned sender port instead of occupying a live game's port.
        client.candidates[0] = sender.local_addr().unwrap();
        assert_eq!(client.candidates.len(), usize::from(PORT_RANGE));
        assert!(client.host.is_none());
        if reason == "sent you away" {
            client.welcomes = 1;
        }
        deliver_rejection(&mut client, &sender, reason);
        assert_eq!(client.rejected.as_deref(), Some(reason));
        assert_eq!(client.turned_away.as_deref(), Some(reason));
        assert!(client.other_reject.is_none());
        assert!(!client.connected && !client.timed_out);
        assert!(client.take_events().iter().any(|event| matches!(
            event, LanEvent::Notice(message) if message == &format!("the host turned us away: {reason}")
        )));
    }
}

#[test]
fn different_machines_keep_trying_after_one_rejects() {
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let from = sender.local_addr().unwrap();
    let mut client = LanSession::join_addr(
        vec![from, SocketAddr::from(([192, 0, 2, 1], from.port()))],
        None,
        "client",
        world("m"),
    )
    .unwrap();
    deliver_rejection(&mut client, &sender, "session full");
    assert!(client.host.is_none());
    assert!(client.rejected.is_none() && client.turned_away.is_none());
    assert!(
        client
            .other_reject
            .as_deref()
            .is_some_and(|reason| reason.contains("session full"))
    );
    assert!(!client.timed_out);
}

#[test]
fn explicit_port_rejections_remain_final() {
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut client = LanSession::join(
        &sender.local_addr().unwrap().to_string(),
        "client",
        world("m"),
        Duration::from_secs(10),
    )
    .unwrap();
    deliver_rejection(&mut client, &sender, "different LAN protocol");
    assert_eq!(client.rejected.as_deref(), Some("different LAN protocol"));
    assert_eq!(client.turned_away, client.rejected);
    assert!(client.other_reject.is_none());
}

#[test]
fn an_established_host_rejection_is_final_even_with_other_machine_candidates() {
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let from = sender.local_addr().unwrap();
    let mut client = LanSession::join_addr(
        vec![from, SocketAddr::from(([192, 0, 2, 1], from.port()))],
        None,
        "client",
        world("m"),
    )
    .unwrap();
    client.host = Some(from);
    client.welcomes = 1;
    deliver_rejection(&mut client, &sender, "sent you away");
    assert_eq!(client.turned_away.as_deref(), Some("sent you away"));
    assert_eq!(client.rejected, client.turned_away);
    assert!(client.other_reject.is_none());
}

#[test]
fn one_machine_without_a_session_reports_the_entire_port_range() {
    let mut client = portless_client();
    client.refused = 2;
    let hint = client.no_answer(Duration::from_secs(10));
    assert!(hint.contains("the address 127.0.0.1 answers"), "{hint}");
    assert!(
        hint.contains("no session runs on ports 27015-27024"),
        "{hint}"
    );
    client.refused = 0;
    assert!(
        client
            .no_answer(Duration::from_secs(10))
            .contains("UDP on ports 27015-27024")
    );
}

#[test]
fn explicit_port_and_multi_machine_timeout_hints_keep_their_meaning() {
    let mut client = LanSession::join_addr(
        vec![SocketAddr::from(([127, 0, 0, 1], 27019))],
        None,
        "client",
        world("m"),
    )
    .unwrap();
    client.refused = 2;
    assert!(
        client
            .no_answer(Duration::from_secs(10))
            .contains("no session runs on port 27019 there")
    );
    client
        .candidates
        .push(SocketAddr::from(([192, 0, 2, 1], 27019)));
    assert!(
        !client
            .no_answer(Duration::from_secs(10))
            .contains("but no session runs")
    );
}

#[test]
fn sparse_ports_are_not_described_as_an_entire_range() {
    let mut client = portless_client();
    client.candidates = vec![
        SocketAddr::from(([127, 0, 0, 1], 27024)),
        SocketAddr::from(([127, 0, 0, 1], 27015)),
        SocketAddr::from(([127, 0, 0, 1], 27015)),
    ];
    client.refused = 2;
    assert!(
        client
            .no_answer(Duration::from_secs(10))
            .contains("no session runs on ports 27015, 27024 there")
    );
}

#[test]
fn no_candidates_do_not_claim_that_one_machine_answered() {
    let mut client = portless_client();
    client.candidates.clear();
    client.refused = 2;
    let hint = client.no_answer(Duration::from_secs(10));
    assert!(!hint.contains("but no session runs"));
    assert!(hint.contains("UDP on port 27015"));
}
