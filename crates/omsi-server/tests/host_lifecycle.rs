//! Headless host lifecycle regressions that exercise real UDP admission and leave.
//! This is a transport-only smoke test, not full bus-physics validation.

use omsi_net::{LanSession, Pose, WorldInfo};
use omsi_server::transport::HostTransport;
use std::net::SocketAddr;
use std::time::Duration;

fn world() -> WorldInfo {
    WorldInfo {
        map: "Maps/Grundorf/global.cfg".into(),
        date: "1989-05-30".into(),
        time: 9.0 * 3600.0,
        weather: "Weather/Sommer.owt".into(),
        ..Default::default()
    }
}

fn driver(name: &str, x: f64) -> Pose {
    Pose {
        bus: format!("Vehicles/{name}.bus"),
        flags: omsi_net::FLAG_VEHICLE,
        name: name.into(),
        x,
        ..Default::default()
    }
}

fn address(host: &HostTransport) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], host.session().local_addr().unwrap().port()))
}

#[test]
fn a_departed_client_releases_its_slot_and_its_bus_does_not_become_a_ghost() {
    let original = world();
    let mut host = HostTransport::bind(0, "headless", original.clone(), 2).unwrap();
    host.ready();
    let endpoint = address(&host);
    let mut a = LanSession::join_addr(vec![endpoint], None, "alpha", original.clone()).unwrap();
    let mut b = LanSession::join_addr(vec![endpoint], None, "beta", original.clone()).unwrap();
    let pa = driver("alpha", 101.0);
    let pb = driver("beta", 202.0);

    let mut joined = false;
    for _ in 0..240 {
        host.tick(0.06);
        a.tick(0.06, &pa);
        b.tick(0.06, &pb);
        if a.connected
            && b.connected
            && host.session().peer_count() == 2
            && a.peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "beta")
        {
            joined = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    assert!(joined, "initial clients did not enter the host");

    // BYE is part of the same production wire protocol. The next driver must
    // not inherit beta's old bus or exceed the configured player limit.
    b.leave();
    drop(b);
    let mut cleaned = false;
    for _ in 0..120 {
        host.tick(0.06);
        a.tick(0.06, &pa);
        if host.session().peer_count() == 1 && !a.peers().any(|p| p.pose.name == "beta") {
            cleaned = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    assert!(cleaned, "disconnected peer or bus remains in the host");
    let mut c = LanSession::join_addr(vec![endpoint], None, "charlie", original).unwrap();
    let pc = driver("charlie", 303.0);
    let mut rejoined = false;
    for _ in 0..240 {
        host.tick(0.06);
        a.tick(0.06, &pa);
        c.tick(0.06, &pc);
        if c.connected
            && host.session().peer_count() == 2
            && a.peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "charlie")
            && c.peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "alpha")
        {
            rejoined = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    assert!(rejoined, "freed host slot did not admit replacement client");
    assert!(!a.peers().any(|p| p.pose.name == "beta"));
}

#[test]
fn joining_player_receives_the_latest_authoritative_clock_and_weather() {
    let original = world();
    let mut host = HostTransport::bind(0, "headless", original.clone(), 3).unwrap();
    host.session_mut().set_clock("1990-01-05", 43_210.0);
    host.session_mut().set_weather("Weather/Herbst.owt");
    host.ready();

    let mut client = LanSession::join_addr(vec![address(&host)], None, "alpha", original).unwrap();
    let pose = driver("alpha", 20.0);
    for _ in 0..200 {
        host.tick(0.06);
        client.tick(0.06, &pose);
        if client.connected {
            break;
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    let welcome = client
        .welcome
        .as_ref()
        .expect("authoritative world welcome");
    assert_eq!(welcome.world.map, "Maps/Grundorf/global.cfg");
    assert_eq!(welcome.world.date, "1990-01-05");
    assert_eq!(welcome.world.weather, "Weather/Herbst.owt");
    assert!((welcome.world.time - 43_210.0).abs() < 1e-6);
    assert_eq!(host.session().peer_count(), 1);
}
