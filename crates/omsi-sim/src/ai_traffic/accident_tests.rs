//! LAN play: a client's accident is kept by the host, whose traffic is the simulated one.

use super::body_tests::{Fixture, add_car, car, traffic};
use super::*;
use crate::traffic::LaneBuilder;
use crate::vehicle::DynamicImpact;

/// A client, player 3 of the session.
const CLIENT: u32 = 3;

fn road(f: &Fixture) -> TrafficSim {
    let lane = LaneBuilder::arc(DVec3::ZERO, 0.0, 1000.0, 0.0, 0.0, LaneKind::Street, 3.0);
    traffic(
        f,
        Network {
            lanes: vec![lane],
            ..Default::default()
        },
    )
}

fn hit(car: u64, at: DVec3) -> DynamicImpact {
    DynamicImpact {
        obstacle_id: -2 - car as i64,
        point: at + DVec3::new(-1.0, 1.5, 0.5),
        push: DVec3::new(1.0, 0.0, 0.0),
        speed: 8.0,
        energy: 60_000.0,
        mass: 12_000.0,
    }
}

/// The host with a car on the road, and that car knocked loose by the client's hit as the
/// client sent it.
fn host_hit_by_client(f: &Fixture) -> (TrafficSim, u64) {
    let mut host = road(f);
    let id = add_car(&mut host, f, 0, 50.0, 42, Some(8.0));
    for _ in 0..25 {
        host.advance(0.02, None);
    }
    let at = car(&host, id).vehicle.position;
    let mut client = road(f);
    client.mirror = true;
    let copy = add_car(&mut client, f, 0, 50.0, 42, Some(0.0));
    client.player_impacts(vec![hit(copy, at)]);
    assert!(
        !car(&client, copy).body.is_loose(),
        "a client knocks no car loose itself"
    );
    assert!(!client.accident_open());
    let (hits, police) = client.take_for_host();
    assert_eq!((hits.len(), police), (1, false), "the hit goes to the host");
    let mut sent = hits[0];
    sent.obstacle_id = -2 - id as i64;
    host.player_impacts_by(vec![sent], CLIENT);
    (host, id)
}

#[test]
fn a_clients_accident_is_the_hosts_and_its_police_clear_it() {
    let f = Fixture::new();
    let (mut host, id) = host_hit_by_client(&f);
    assert!(car(&host, id).body.is_loose());
    assert!(host.accident_open_by(CLIENT));
    assert!(
        !host.accident_open(),
        "the host's own player has no accident"
    );
    assert_eq!(host.take_accidents(), 0);
    assert_eq!(host.accident_note(CLIENT), Some((true, 1, 0)));
    assert_eq!(host.accident_note(CLIENT), None, "told once");

    let mut client = road(&f);
    client.mirror = true;
    client.accident_from_host(true, 1, 0);
    assert!(client.accident_open());
    assert_eq!(client.take_accidents(), 1);

    assert_eq!(client.clear_accidents(), 1);
    assert!(!client.accident_open());
    let (hits, police) = client.take_for_host();
    assert!(hits.is_empty() && police, "the call goes to the host");
    assert_eq!(client.clear_accidents(), 0, "called once");

    assert_eq!(
        host.clear_accidents(),
        0,
        "the host's own call leaves a client's accident"
    );
    assert!(car(&host, id).body.is_loose());
    assert_eq!(host.clear_accidents_by(CLIENT), 1);
    assert!(!car(&host, id).body.is_loose());
    assert_eq!(host.accident_note(CLIENT), Some((false, 0, 0)));
}

#[test]
fn a_client_that_leaves_the_scene_is_told_so() {
    let f = Fixture::new();
    let (mut host, id) = host_hit_by_client(&f);
    assert_eq!(host.accident_note(CLIENT), Some((true, 1, 0)));
    let i = host.cars.iter().position(|c| c.id == id).unwrap();
    host.leave_scene(i);
    assert_eq!(host.take_fled(), 0, "not the host's own player");
    assert_eq!(host.accident_note(CLIENT), Some((false, 0, 1)));

    let mut client = road(&f);
    client.mirror = true;
    client.accident_from_host(false, 0, 1);
    assert!(!client.accident_open());
    assert_eq!(client.take_fled(), 1);
    assert_eq!(client.take_fled(), 0);
}
