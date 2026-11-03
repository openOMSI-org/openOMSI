//! A joining game's WebSocket breaks and is made again: the host must know the player at
//! the gateway's new socket at once, not time it out first.
use super::*;
use crate::{LanSession, Pose, WorldInfo};
use std::time::Instant;

/// A TCP relay between the game's bridge and the gateway whose connections can be cut.
fn breakable_relay(to: SocketAddr) -> (SocketAddr, Arc<Mutex<Vec<TcpStream>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let open: Arc<Mutex<Vec<TcpStream>>> = Arc::default();
    let o = open.clone();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let Ok(t) = TcpStream::connect(to) else { continue };
            o.lock().unwrap().extend([s.try_clone().unwrap(), t.try_clone().unwrap()]);
            for (mut a, mut b) in [(s.try_clone().unwrap(), t.try_clone().unwrap()), (t, s)] {
                std::thread::spawn(move || { let _ = std::io::copy(&mut a, &mut b); let _ = b.shutdown(std::net::Shutdown::Both); });
            }
        }
    });
    (addr, open)
}

#[test]
fn a_player_whose_websocket_is_made_again_is_known_at_its_new_socket_at_once() {
    let mut host = LanSession::host(0, "host", WorldInfo::default(), true).unwrap();
    let session = host.local_addr().unwrap();
    let gw = WsGateway::start("127.0.0.1:0".parse().unwrap(), SocketAddr::from(([127, 0, 0, 1], session.port())), ServerInfo::default()).unwrap();
    let (relay, open) = breakable_relay(gw.addr);
    let bridge = WsClient::connect(&format!("ws://{relay}/ws")).unwrap();
    let mut c = LanSession::join(&bridge.local.to_string(), "c", WorldInfo::default(), Duration::from_millis(10)).unwrap();
    c.heartbeat = 0.05;
    let both = |host: &mut LanSession, c: &mut LanSession| {
        c.tick(0.05, &Pose::default());
        host.tick(0.05, &Pose::default());
        std::thread::sleep(Duration::from_millis(10));
    };
    let t0 = Instant::now();
    while !host.peers.get(&c.my_id).is_some_and(|p| p.settled) && t0.elapsed() < Duration::from_secs(10) {
        both(&mut host, &mut c);
    }
    let id = c.my_id;
    let before = host.peers.get(&id).and_then(|p| p.addr).expect("joined through the gateway");
    for s in open.lock().unwrap().drain(..) {
        let _ = s.shutdown(std::net::Shutdown::Both);
    }
    let t0 = Instant::now();
    while bridge.reconnects() == 0 && t0.elapsed() < Duration::from_secs(10) {
        both(&mut host, &mut c);
    }
    assert_eq!(bridge.reconnects(), 1, "the bridge made the WebSocket again");
    // (what the game does when `ws_came_back`)
    c.rehello();
    let t0 = Instant::now();
    while host.peers.get(&id).and_then(|p| p.addr) == Some(before) && t0.elapsed() < Duration::from_secs(5) {
        both(&mut host, &mut c);
    }
    let after = host.peers.get(&id).and_then(|p| p.addr);
    assert!(after.is_some() && after != Some(before), "still at {before:?} {:.1} s after the WebSocket came back", t0.elapsed().as_secs_f32());
}
