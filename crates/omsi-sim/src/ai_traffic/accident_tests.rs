//! LAN play: a client's accident is kept by the host, whose traffic is the simulated one,
//! over a link that loses messages.

use super::accident::AccidentMsg;
use super::body_tests::{Fixture, add_car, car, traffic};
use super::*;
use crate::traffic::LaneBuilder;
use crate::vehicle::DynamicImpact;

/// A client, player 3 of the session.
const CLIENT: u32 = 3;
const DT: f32 = 0.05;

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

/// The host with a car on the road, and the client with its copy of it.
struct Session {
    host: TrafficSim,
    client: TrafficSim,
    car: u64,
    copy: u64,
    /// Messages sent each way, and those lost.
    sent: usize,
    lost: usize,
}

impl Session {
    fn new(f: &Fixture) -> Session {
        let mut host = road(f);
        let car = add_car(&mut host, f, 0, 50.0, 42, Some(8.0));
        for _ in 0..25 {
            host.advance(0.02, None);
        }
        let mut client = road(f);
        client.mirror = true;
        let copy = add_car(&mut client, f, 0, 50.0, 42, Some(0.0));
        Session {
            host,
            client,
            car,
            copy,
            sent: 0,
            lost: 0,
        }
    }

    fn hit(&mut self) {
        let at = car(&self.host, self.car).vehicle.position;
        self.client.player_impacts(vec![DynamicImpact {
            obstacle_id: -2 - self.copy as i64,
            point: at + DVec3::new(-1.0, 1.5, 0.5),
            push: DVec3::new(1.0, 0.0, 0.0),
            speed: 8.0,
            energy: 60_000.0,
            mass: 12_000.0,
        }]);
    }

    /// `secs` of the session, every message passing the link unless `lose` says it is lost
    /// (it is asked of each sending, with how many of its kind went before).
    fn run(&mut self, secs: f32, lose: &mut dyn FnMut(&AccidentMsg, usize) -> bool) {
        let mut kinds: HashMap<std::mem::Discriminant<AccidentMsg>, usize> = HashMap::new();
        let mut pass = |msg: &AccidentMsg, sent: &mut usize, lost: &mut usize| {
            let n = kinds.entry(std::mem::discriminant(msg)).or_default();
            *n += 1;
            *sent += 1;
            let gone = lose(msg, *n - 1);
            *lost += usize::from(gone);
            !gone
        };
        for _ in 0..(secs / DT).round() as usize {
            for mut msg in self.client.accident_to_host(DT) {
                if let AccidentMsg::Hit { hit, .. } = &mut msg {
                    assert_eq!(hit.obstacle_id, -2 - self.copy as i64);
                    hit.obstacle_id = -2 - self.car as i64;
                }
                if !pass(&msg, &mut self.sent, &mut self.lost) {
                    continue;
                }
                if let Some(answer) = self.host.accident_from_player(CLIENT, msg) {
                    if pass(&answer, &mut self.sent, &mut self.lost) {
                        self.client.accident_from_host(answer);
                    }
                }
            }
            for (player, msg) in self.host.accident_to_players(&[CLIENT], DT) {
                assert_eq!(player, CLIENT);
                if !pass(&msg, &mut self.sent, &mut self.lost) {
                    continue;
                }
                if let Some(answer) = self.client.accident_from_host(msg) {
                    if pass(&answer, &mut self.sent, &mut self.lost) {
                        self.host.accident_from_player(CLIENT, answer);
                    }
                }
            }
        }
    }

    fn loose(&self) -> bool {
        car(&self.host, self.car).body.is_loose()
    }
}

fn lossless(_: &AccidentMsg, _: usize) -> bool {
    false
}

#[test]
fn a_clients_accident_is_the_hosts_and_its_police_clear_it() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    s.hit();
    assert!(
        !car(&s.client, s.copy).body.is_loose(),
        "a client knocks no car loose itself"
    );
    s.run(0.2, &mut lossless);
    assert!(s.loose());
    assert!(s.host.accident_open_by(CLIENT));
    assert!(!s.host.accident_open(), "the host's own player has none");
    assert_eq!(s.host.take_accidents(), 0);
    assert!(s.client.accident_open());
    assert_eq!(s.client.take_accidents(), 1);

    assert_eq!(
        s.host.clear_accidents(),
        0,
        "the host's own call leaves a client's"
    );
    assert_eq!(s.client.clear_accidents(), 1);
    assert!(!s.client.accident_open());
    assert_eq!(s.client.clear_accidents(), 0, "called once");
    s.run(0.2, &mut lossless);
    assert!(!s.loose());
    assert!(!s.client.accident_open());
    assert_eq!(s.client.take_accidents(), 0);

    let sent = s.sent;
    s.run(5.0, &mut lossless);
    assert_eq!(s.sent, sent, "nothing is sent again once answered");
}

/// The first two sendings of every kind of message are lost - hits, calls, states and the
/// answers to each: everything arrives in the end, and nothing is done twice.
#[test]
fn lost_messages_are_sent_again_and_taken_once() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    let mut lose_first_two = |_: &AccidentMsg, n: usize| n < 2;
    s.hit();
    s.run(5.0, &mut lose_first_two);
    assert!(s.lost >= 6, "lost {} of {}", s.lost, s.sent);
    assert!(s.loose(), "the hit reached the host");
    assert!(s.client.accident_open());
    assert_eq!(s.client.take_accidents(), 1, "one accident, told once");

    let mut lose_first_two = |_: &AccidentMsg, n: usize| n < 2;
    s.client.clear_accidents();
    s.run(5.0, &mut lose_first_two);
    assert!(!s.loose(), "the call reached the host");
    assert!(!s.client.accident_open());

    // a second accident: the host keeps the client's call apart from the first
    s.hit();
    s.run(5.0, &mut lose_first_two);
    assert!(s.loose());
    assert!(s.client.accident_open());
    assert_eq!(s.client.take_accidents(), 1);
}

/// Every state the host sends is lost for a while: the client learns of the accident, and
/// that it left the scene, once the link carries again.
#[test]
fn a_client_that_leaves_the_scene_is_told_so_after_a_loss() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    s.hit();
    let mut states_lost = |m: &AccidentMsg, _: usize| matches!(m, AccidentMsg::State { .. });
    s.run(1.0, &mut states_lost);
    assert!(s.loose());
    assert!(!s.client.accident_open(), "not told yet");
    let i = s.host.cars.iter().position(|c| c.id == s.car).unwrap();
    s.host.leave_scene(i);
    assert_eq!(s.host.take_fled(), 0, "not the host's own player");
    s.run(1.0, &mut states_lost);
    assert_eq!(s.client.take_fled(), 0);

    s.run(1.0, &mut lossless);
    assert!(!s.client.accident_open());
    assert_eq!(s.client.take_accidents(), 1);
    assert_eq!(s.client.take_fled(), 1);
    s.run(2.0, &mut lossless);
    assert_eq!(s.client.take_fled(), 0, "told once");
}

/// A state that arrives after a newer one does not undo it.
#[test]
fn an_old_state_arriving_late_is_ignored() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    let newer = AccidentMsg::State {
        seq: 2,
        open: false,
        new: 1,
        fled: 1,
    };
    let older = AccidentMsg::State {
        seq: 1,
        open: true,
        new: 1,
        fled: 0,
    };
    assert_eq!(
        s.client.accident_from_host(newer),
        Some(AccidentMsg::StateSeen { seq: 2 })
    );
    assert_eq!(
        s.client.accident_from_host(older),
        Some(AccidentMsg::StateSeen { seq: 1 }),
        "answered, so that the host stops"
    );
    assert!(!s.client.accident_open());
    assert_eq!((s.client.take_accidents(), s.client.take_fled()), (1, 1));
}

fn hit_on(host: &TrafficSim, id: u64, seq: u32) -> AccidentMsg {
    let at = car(host, id).vehicle.position;
    AccidentMsg::Hit {
        seq,
        hit: DynamicImpact {
            obstacle_id: -2 - id as i64,
            point: at + DVec3::new(-1.0, 1.5, 0.5),
            push: DVec3::new(1.0, 0.0, 0.0),
            speed: 8.0,
            energy: 60_000.0,
            mass: 12_000.0,
        },
    }
}

/// The client hit a car (1), called the police (2) and hit another car (3), and the call
/// arrives last: it clears the first accident, not the second.
#[test]
fn a_late_call_clears_only_the_accidents_before_it() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    let other = add_car(&mut s.host, &f, 0, 150.0, 43, Some(8.0));
    let msgs = [
        hit_on(&s.host, s.car, 1),
        hit_on(&s.host, other, 3),
        AccidentMsg::Police { seq: 2 },
    ];
    for msg in msgs {
        s.host.accident_from_player(CLIENT, msg);
    }
    assert!(!s.loose(), "the call covers the first hit");
    assert!(car(&s.host, other).body.is_loose(), "not the one after it");
    assert!(s.host.accident_open_by(CLIENT));
}

/// The same with the car hit again after the call: it stays where the second hit left it.
#[test]
fn a_late_call_leaves_a_car_hit_again_after_it() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    let msgs = [
        hit_on(&s.host, s.car, 1),
        hit_on(&s.host, s.car, 3),
        AccidentMsg::Police { seq: 2 },
    ];
    for msg in msgs {
        s.host.accident_from_player(CLIENT, msg);
    }
    assert!(s.loose());
    assert!(s.host.accident_open_by(CLIENT));
    s.host
        .accident_from_player(CLIENT, AccidentMsg::Police { seq: 4 });
    assert!(!s.loose(), "a call after the second hit clears it");
}

/// The call (2) overtakes the hit it covers (1): the car is pushed but not left standing,
/// and the client is told of no accident; a hit after the call is one.
#[test]
fn a_hit_arriving_after_the_call_that_covers_it_leaves_no_accident() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    let late = hit_on(&s.host, s.car, 1);
    s.host
        .accident_from_player(CLIENT, AccidentMsg::Police { seq: 2 });
    assert_eq!(
        s.host.accident_from_player(CLIENT, late),
        Some(AccidentMsg::HitSeen { seq: 1 })
    );
    assert!(!s.loose());
    assert!(!s.host.accident_open_by(CLIENT));
    assert!(s.host.accident_to_players(&[CLIENT], DT).is_empty());
    let next = hit_on(&s.host, s.car, 3);
    s.host.accident_from_player(CLIENT, next);
    assert!(s.loose());
    assert_eq!(
        s.host.accident_to_players(&[CLIENT], DT),
        vec![(
            CLIENT,
            AccidentMsg::State {
                seq: 1,
                open: true,
                new: 1,
                fled: 0
            }
        )]
    );
}
