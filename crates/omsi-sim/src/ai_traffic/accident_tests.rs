//! LAN play: a client's accident is kept by the host, whose traffic is the simulated one,
//! over a link that loses messages.

use super::accident::{AccidentMsg, HAZARD_HOLD, HazardWatch, police_may_be_called};
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
        done: 0,
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

/// Every hit is lost for 20 s: it is sent again all that time, and reaches the host once
/// the link carries again.
#[test]
fn a_hit_lost_for_long_reaches_the_host_in_the_end() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    s.hit();
    let mut hits_lost = |m: &AccidentMsg, _: usize| matches!(m, AccidentMsg::Hit { .. });
    s.run(20.0, &mut hits_lost);
    assert!(!s.loose());
    assert!(s.lost >= 39, "sent again all along: {} lost", s.lost);
    s.run(1.0, &mut lossless);
    assert!(s.loose(), "the hit reached the host");
    assert!(s.client.accident_open());
    assert_eq!(s.client.take_accidents(), 1);
}

/// A hit on a car the host no longer has is answered all the same, and not sent again.
#[test]
fn a_hit_on_a_car_gone_on_the_host_is_answered() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    let i = s.host.cars.iter().position(|c| c.id == s.car).unwrap();
    s.host.take_off(i, control::Removal::Finished);
    s.client.player_impacts(vec![DynamicImpact {
        obstacle_id: -2 - s.copy as i64,
        point: DVec3::ZERO,
        push: DVec3::new(1.0, 0.0, 0.0),
        speed: 8.0,
        energy: 60_000.0,
        mass: 12_000.0,
    }]);
    s.run(1.0, &mut lossless);
    assert_eq!(s.sent, 2, "the hit and its answer");
    s.run(5.0, &mut lossless);
    assert_eq!(s.sent, 2, "not sent again");
    assert!(!s.host.accident_open_by(CLIENT));
    assert!(!s.client.accident_open());
}

/// What a client owed a host it no longer plays with is not sent to the next one.
#[test]
fn a_new_session_starts_with_nothing_owed() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    s.hit();
    s.client.accident_session_changed();
    assert!(s.client.accident_to_host(DT).is_empty());
    assert!(!s.client.accident_open());
}

/// Every message goes out as a LAN command and reads back as itself.
#[test]
fn the_messages_read_back_from_their_commands() {
    let hit = DynamicImpact {
        obstacle_id: -2 - 65537,
        point: DVec3::new(892391.5, 4196807.25, 33.5),
        push: DVec3::new(0.6, -0.8, 0.0),
        speed: 8.5,
        energy: 60_000.0,
        mass: 12_000.0,
    };
    for msg in [
        AccidentMsg::Hit {
            seq: 7,
            done: 5,
            hit,
        },
        AccidentMsg::HitSeen { seq: 7 },
        AccidentMsg::Police { seq: 8 },
        AccidentMsg::PoliceSeen { seq: 8 },
        AccidentMsg::State {
            seq: 9,
            open: true,
            new: 2,
            fled: 1,
        },
        AccidentMsg::StateSeen { seq: 9 },
    ] {
        let text = msg.command().unwrap();
        assert!(text.len() < 160, "{text}");
        assert_eq!(AccidentMsg::parse(&text, true), Some(msg), "{text}");
    }
    let nameless = DynamicImpact {
        obstacle_id: -1,
        ..hit
    };
    assert_eq!(
        AccidentMsg::Hit {
            seq: 1,
            done: 0,
            hit: nameless
        }
        .command(),
        None
    );
    for bad in [
        "crash 1 0 1 0 0 0 1 0 0 nan 1 1",
        "crash 1 0 1 inf 0 0 1 0 0 1 1 1",
        "crash 1 1 0 0 0 1 0 0 1 1 1",
        "crash 1 0 0",
        "police",
        "police x",
        "dance 1",
    ] {
        assert_eq!(AccidentMsg::parse(bad, true), None, "{bad}");
    }
    for only_host in ["accident 9 1 1 0", "crash-seen 7", "police-seen 8"] {
        assert!(AccidentMsg::parse(only_host, true).is_some());
        assert_eq!(AccidentMsg::parse(only_host, false), None, "{only_host}");
    }
    assert!(AccidentMsg::parse("police 8", false).is_some());
}

/// Hazards flashing 0.4 s lit, 0.4 s dark, at 50 frames a second.
fn flash(watch: &mut HazardWatch, secs: f32, mut each: impl FnMut(bool, &HazardWatch)) {
    for i in 0..(secs / 0.02).round() as usize {
        let lit = (i as f32 * 0.02) % 0.8 < 0.4;
        watch.step(0.02, lit);
        each(lit, &*watch);
    }
}

#[test]
fn hazards_count_as_on_in_the_dark_phase_of_the_lamps() {
    let mut watch = HazardWatch::default();
    let mut dark = 0;
    flash(&mut watch, 4.0, |lit, w| {
        dark += usize::from(!lit);
        assert!(
            w.on(None),
            "a bus without the hazard switch, lamps lit {lit}"
        );
        assert!(police_may_be_called(None, 0.0, w));
        assert!(!police_may_be_called(None, 5.0, w), "moving");
    });
    assert!(dark > 50, "the dark phase was tested: {dark} frames");
}

#[test]
fn hazards_switched_off_go_off_after_the_hold() {
    let mut watch = HazardWatch::default();
    flash(&mut watch, 2.0, |_, _| {});
    let mut t = 0.0;
    while t < HAZARD_HOLD + 0.1 {
        watch.step(0.02, false);
        t += 0.02;
    }
    assert!(!watch.on(None));
    assert!(!HazardWatch::default().on(None), "never lit");
}

#[test]
fn the_scripts_hazard_switch_decides_where_there_is_one() {
    let mut watch = HazardWatch::default();
    flash(&mut watch, 1.0, |_, _| {});
    assert!(!watch.on(Some(0.0)), "switch off, lamps flashing");
    assert!(
        HazardWatch::default().on(Some(1.0)),
        "switch on, lamps dark"
    );
}

/// The host keeps the number of every hit the client may still send, however many others
/// come after it: a hit whose answer is lost while hundreds of later ones are answered, and
/// a copy of it delayed past the client's next word, are carried out once.
#[test]
fn a_hit_answered_late_is_carried_out_once_after_hundreds_of_others() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    let first = hit_on(&s.host, s.car, 1);
    assert_eq!(
        s.host.accident_from_player(CLIENT, first),
        Some(AccidentMsg::HitSeen { seq: 1 })
    );
    assert!(s.loose());
    let rest = |s: &mut Session| {
        for _ in 0..200 {
            s.host.advance(DT, None);
        }
        car(&s.host, s.car).vehicle.position
    };
    let at = rest(&mut s);
    // the answer to hit 1 is lost: the client sends the others with "none answered"
    for seq in 2..=600 {
        let mut msg = hit_on(&s.host, s.car, seq);
        if let AccidentMsg::Hit { hit, .. } = &mut msg {
            hit.obstacle_id = -2 - 999_999;
        }
        assert_eq!(
            s.host.accident_from_player(CLIENT, msg),
            Some(AccidentMsg::HitSeen { seq })
        );
    }
    assert_eq!(
        s.host.accident_from_player(CLIENT, first),
        Some(AccidentMsg::HitSeen { seq: 1 }),
        "the copy is answered"
    );
    assert_eq!(rest(&mut s), at, "and not carried out again");

    // all answered now; a copy of hit 1 sent before that arrives after the client said so
    let mut next = hit_on(&s.host, s.car, 601);
    if let AccidentMsg::Hit { done, .. } = &mut next {
        *done = 600;
    }
    s.host.accident_from_player(CLIENT, next);
    let pushed = rest(&mut s);
    assert!(pushed.distance(at) > 0.01, "a new hit is carried out");
    s.host.accident_from_player(CLIENT, first);
    assert_eq!(rest(&mut s), pushed, "the late copy is not");
}

/// A session gives a departed player's id to the next one: the newcomer starts with its own
/// numbers from 1, and the host takes none of its hits for the old player's, while the old
/// player's accident ends with it.
#[test]
fn a_player_given_a_departed_players_id_starts_afresh() {
    let f = Fixture::new();
    let mut s = Session::new(&f);
    s.host.accident_to_players(&[CLIENT], DT);
    let mut old = hit_on(&s.host, s.car, 50);
    if let AccidentMsg::Hit { done, .. } = &mut old {
        *done = 49;
    }
    s.host.accident_from_player(CLIENT, old);
    assert!(s.loose());
    s.host
        .accident_from_player(CLIENT, AccidentMsg::Police { seq: 60 });
    assert!(!s.loose());
    s.host
        .accident_from_player(CLIENT, hit_on(&s.host, s.car, 70));
    assert!(s.loose() && s.host.accident_open_by(CLIENT));

    s.host.accident_to_players(&[], DT);
    assert!(!s.loose(), "the accident ends with the player who left");
    assert!(!s.host.accident_open_by(CLIENT));

    s.host.accident_to_players(&[CLIENT], DT);
    assert_eq!(
        s.host
            .accident_from_player(CLIENT, hit_on(&s.host, s.car, 1)),
        Some(AccidentMsg::HitSeen { seq: 1 })
    );
    assert!(s.loose(), "the newcomer's first hit is carried out");
    assert!(s.host.accident_open_by(CLIENT));
    s.host
        .accident_from_player(CLIENT, AccidentMsg::Police { seq: 2 });
    assert!(!s.loose(), "and its first call clears it");
}
