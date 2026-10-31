use omsi_net::{LanSession, WorldInfo};
use std::net::{TcpListener, UdpSocket};

fn host(port: u16, try_next: bool) -> std::io::Result<LanSession> {
    LanSession::host(port, "port-test", WorldInfo::default(), try_next)
}

fn udp_conflict(address: &str) -> UdpSocket {
    // A UDP-assigned port may be excluded for TCP on Windows. Keep that unrelated
    // restriction out of tests that specifically exercise another UDP listener.
    for _ in 0..32 {
        let tcp = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        if let Ok(udp) = UdpSocket::bind((address, tcp.local_addr().unwrap().port())) {
            return udp;
        }
    }
    panic!("no port available for both transports");
}

#[test]
fn host_rejects_a_tcp_only_conflict() {
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = occupied.local_addr().unwrap().port();
    assert!(
        host(port, false).is_err(),
        "TCP is occupied even though UDP is free"
    );
}

#[test]
fn host_skips_a_tcp_only_conflict() {
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = occupied.local_addr().unwrap().port();
    let session = host(port, true).unwrap();
    assert_ne!(session.local_addr().unwrap().port(), port);
}

#[test]
fn host_reserves_tcp_until_the_session_ends() {
    let session = host(0, false).unwrap();
    let port = session.local_addr().unwrap().port();
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_err(),
        "another service must not take the reserved TCP port"
    );
    drop(session);
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "ending the session releases its reservation"
    );
}

#[test]
fn host_releases_udp_when_tcp_cannot_be_reserved() {
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = occupied.local_addr().unwrap().port();
    let session = host(port, false);
    assert!(session.is_err());
    assert!(UdpSocket::bind(("0.0.0.0", port)).is_ok());
}

#[test]
fn host_skips_a_udp_only_conflict() {
    let occupied = udp_conflict("0.0.0.0");
    let port = occupied.local_addr().unwrap().port();
    let session = host(port, true).unwrap();
    assert_ne!(session.local_addr().unwrap().port(), port);
    assert_eq!(
        session.code().unwrap().port,
        session.local_addr().unwrap().port()
    );
}

#[test]
fn reserved_listener_is_transferred_without_rebinding() {
    let mut session = host(0, false).unwrap();
    let port = session.local_addr().unwrap().port();
    let listener = session.take_mods_listener().unwrap();
    assert!(session.take_mods_listener().is_none());
    drop(session);
    assert!(TcpListener::bind(("127.0.0.1", port)).is_err());
    let probe = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (_, peer) = listener.accept().unwrap();
    assert_eq!(peer, probe.local_addr().unwrap());
}

#[test]
fn host_fails_when_the_candidate_range_is_occupied() {
    for _ in 0..32 {
        let first = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = first.local_addr().unwrap().port();
        if port.checked_add(omsi_net::PORT_RANGE).is_none() {
            continue;
        }
        let mut occupied = vec![first];
        for offset in 1..omsi_net::PORT_RANGE {
            if let Ok(listener) = TcpListener::bind(("127.0.0.1", port + offset)) {
                occupied.push(listener);
            } else {
                break;
            }
        }
        if occupied.len() == usize::from(omsi_net::PORT_RANGE) {
            assert!(
                host(port, true).is_err(),
                "do not announce a partially usable session"
            );
            assert!(UdpSocket::bind(("0.0.0.0", port)).is_ok());
            return;
        }
    }
    panic!("could not reserve a test range");
}

#[test]
fn highest_port_is_checked_without_wrapping() {
    let _occupied = TcpListener::bind(("127.0.0.1", u16::MAX)).ok();
    assert!(host(u16::MAX, true).is_err());
}

#[test]
fn simultaneous_hosts_keep_distinct_reserved_ports() {
    let first = host(0, false).unwrap();
    let first_port = first.local_addr().unwrap().port();
    let second = host(first_port, true).unwrap();
    let second_port = second.local_addr().unwrap().port();
    assert_ne!(first_port, second_port);
    assert!(TcpListener::bind(("127.0.0.1", first_port)).is_err());
    assert!(TcpListener::bind(("127.0.0.1", second_port)).is_err());
}

#[test]
fn explicitly_disabled_transfers_keep_udp_only_hosting() {
    const CHILD: &str = "PORT_RESERVATION_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = occupied.local_addr().unwrap().port();
        let mut session = host(port, false).unwrap();
        assert_eq!(session.local_addr().unwrap().port(), port);
        assert!(session.take_mods_listener().is_none());
        return;
    }
    // Isolate the startup-only flag from the rest of this test process.
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "explicitly_disabled_transfers_keep_udp_only_hosting",
        ])
        .env(CHILD, "1")
        .env("OMSI_NO_LAN_MODS", "1")
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn host_rejects_a_udp_loopback_conflict() {
    let occupied = udp_conflict("127.0.0.1");
    let port = occupied.local_addr().unwrap().port();
    assert!(
        host(port, false).is_err(),
        "a wildcard UDP bind must not hide another loopback owner"
    );
}

#[test]
fn host_releases_tcp_when_udp_cannot_be_reserved() {
    let occupied = udp_conflict("0.0.0.0");
    let port = occupied.local_addr().unwrap().port();
    assert!(host(port, false).is_err());
    assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
}
