use super::*;

#[test]
fn freetex_packets_relay_between_clients_but_cannot_spoof_an_owner() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |s| {
        find(s[2], 200.0).is_some() && find(s[1], 100.0).is_some()
    });
    assert!(a.connected && b.connected);
    let mut sync = scripttex::Sync::default();
    sync.subscribe(a.my_id, 42);
    let mut request = sync.tick(PROTOCOL as u8, b.my_id, 0.1).remove(0);
    request[0] = crate::freetex::MAGIC;
    b.send_freetex_packet(&request);
    pump(&mut [&mut host, &mut a, &mut b], &poses, 15, |_| false);
    assert_eq!(a.take_freetex_packets(), vec![request.clone()]);
    assert!(host.take_freetex_packets().is_empty());
    assert!(
        b.take_script_textures().is_empty(),
        "independent from script texture streams"
    );

    // Send a valid header claiming to be b from a's real socket: host must reject it.
    request[7..11].copy_from_slice(host.my_id.to_le_bytes().as_slice());
    a.socket.send_to(&request, addr).unwrap();
    pump(&mut [&mut host, &mut a, &mut b], &poses, 15, |_| false);
    assert!(host.take_freetex_packets().is_empty());
    // Nor may a client insert packets directly into another client's stream.
    request[7..11].copy_from_slice(b.my_id.to_le_bytes().as_slice());
    a.socket
        .send_to(
            &request,
            SocketAddr::from(([127, 0, 0, 1], b.local_addr().unwrap().port())),
        )
        .unwrap();
    pump(&mut [&mut host, &mut a, &mut b], &poses, 15, |_| false);
    assert!(b.take_freetex_packets().is_empty());
}
