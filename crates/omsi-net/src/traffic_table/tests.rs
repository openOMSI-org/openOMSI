use super::client::{ClientTable, Event, SILENT_PURGE};
use super::ids::{REUSE_DELAY, TrafficId};
use super::loopback::{Link, Loopback};
use super::row::{Kind, Look, Meta, Row};
use super::table::TrafficTable;
use super::wire::{self, Anchor, Packet, RowUpdate, Spawn};

const CENTRE: [f64; 3] = [892_000.0, 4_196_000.0, 30.0];

fn look(n: usize) -> Look {
    Look { file: format!("Vehicles/Car{}/car{}.bus", n % 7, n % 7), scheme: Some((n % 5) as u8), paint: format!("paint {}", n % 5) }
}

/// Where car `i` is at time `t`: round a circle of `radius` m round `CENTRE` at 12 m/s.
fn truth(i: usize, t: f64, radius: f64) -> Row {
    let theta = i as f64 * 0.37 + 12.0 * t / radius;
    let pos = [CENTRE[0] + radius * theta.sin(), CENTRE[1] + radius * theta.cos(), CENTRE[2] + 0.01 * i as f64];
    Row::from_state(pos, 90.0 + theta.to_degrees(), 0.0, 0.0, 12.0, 3.0, if i % 4 == 0 { 2 } else { 0 }, false, true, 0)
}

fn write(table: &mut TrafficTable, cars: std::ops::Range<usize>, t: f64, radius: f64) {
    table.begin(t);
    for i in cars {
        table.upsert(i as u64, if i % 9 == 0 { Kind::Bus } else { Kind::Car }, || look(i), Meta::default, truth(i, t, radius)).unwrap();
    }
    table.finish();
}

fn run(l: &mut Loopback, table: &mut TrafficTable, cars: std::ops::Range<usize>, secs: f64, radius: f64) {
    let steps = (secs / 0.02) as usize;
    for _ in 0..steps {
        let t = l.now + 0.02;
        write(table, cars.clone(), t, radius);
        l.step(table, CENTRE, 0.02);
    }
}

fn error(l: &Loopback, table: &TrafficTable, ms: f64, radius: f64) -> (usize, f64) {
    let mut worst: f64 = 0.0;
    let mut n = 0;
    for slot in table.iter() {
        let Some(p) = l.client.sample(slot.id, ms) else { continue };
        let want = truth(slot.key as usize, ms / 1000.0, radius);
        let w = want.position();
        worst = worst.max(((p.pos[0] - w[0]).powi(2) + (p.pos[1] - w[1]).powi(2)).sqrt());
        n += 1;
    }
    (n, worst)
}

#[test]
fn a_client_follows_sixty_cars_over_a_bad_network() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::new(Link::new(0.2, 0.03, 0.06, 7), Link::new(0.2, 0.03, 0.06, 9));
    run(&mut l, &mut table, 0..60, 30.0, 200.0);
    assert_eq!(l.client.len(), 60);
    assert!(l.client.ids().all(|id| l.client.look(l.client.get(id).unwrap()).is_some()), "every row has its look");
    for _ in 0..3 {
        run(&mut l, &mut table, 0..60, 1.0, 200.0);
        // drawn a quarter second behind, as the game does
        let (n, worst) = error(&l, &table, l.now * 1000.0 - 250.0, 200.0);
        assert_eq!(n, 60);
        assert!(worst < 0.3, "{worst} m off");
    }
    let host = l.sync.stats;
    assert!(host.delta_rows > host.full_rows * 3, "mostly deltas: {} against {} whole", host.delta_rows, host.full_rows);
    // 60 cars moving, 10 times a second: well under the byte budget
    assert!((l.bytes_down as f64 / l.now) < 12_000.0, "{} B/s", l.bytes_down as f64 / l.now);
}

#[test]
fn a_vehicle_that_goes_is_removed_and_its_id_rests() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::perfect();
    run(&mut l, &mut table, 0..10, 2.0, 100.0);
    let mut events = l.client.take_events();
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Spawned(_))).count(), 10);
    let gone = table.id_of(3).unwrap();
    // car 3 stops being written
    for _ in 0..50 {
        let t = l.now + 0.02;
        table.begin(t);
        for i in (0..10).filter(|i| *i != 3) {
            table.upsert(i as u64, Kind::Car, || look(i), Meta::default, truth(i, t, 100.0)).unwrap();
        }
        table.finish();
        l.step(&table, CENTRE, 0.02);
    }
    events = l.client.take_events();
    assert_eq!(events, vec![Event::Removed(gone)]);
    assert!(l.client.get(gone).is_none() && l.client.len() == 9);
    // a new vehicle at once gets another slot, and after the rest a newer generation of it
    table.begin(l.now);
    let fresh = table.upsert(77, Kind::Car, || look(1), Meta::default, truth(0, l.now, 100.0)).unwrap();
    assert_ne!(fresh.slot(), gone.slot());
    table.finish();
    table.remove(77);
    let mut later = None;
    for _ in 0..10 {
        table.begin(l.now + REUSE_DELAY + 1.0);
        later = table.upsert(78, Kind::Car, || look(1), Meta::default, truth(0, l.now, 100.0));
        table.finish();
        l.now += REUSE_DELAY + 1.0;
        if later.is_some() {
            break;
        }
    }
    let later = later.unwrap();
    assert!(later.slot() == gone.slot() || later.slot() == fresh.slot());
    assert!(later != gone && later != fresh);
}

#[test]
fn vehicles_out_of_range_are_not_sent_and_come_back_when_near() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::perfect();
    // car 0 stands 300 m away, car 1 3 km
    let put = |table: &mut TrafficTable, t: f64, far: f64| {
        table.begin(t);
        let near = Row::from_state([CENTRE[0] + 300.0, CENTRE[1], CENTRE[2]], 0.0, 0.0, 0.0, 0.0, 0.0, 0, false, false, 0);
        let faraway = Row::from_state([CENTRE[0] + far, CENTRE[1], CENTRE[2]], 0.0, 0.0, 0.0, 0.0, 0.0, 0, false, false, 0);
        table.upsert(0, Kind::Car, || look(0), Meta::default, near).unwrap();
        table.upsert(1, Kind::Car, || look(1), Meta::default, faraway).unwrap();
        table.finish();
    };
    for k in 0..100 {
        put(&mut table, l.now + 0.02, 3000.0);
        l.step(&table, CENTRE, 0.02);
        let _ = k;
    }
    assert_eq!(l.client.len(), 1);
    // it drives up to 600 m (inside), then past 700 (out), and between the two radii it
    // stays whichever it was
    for (far, want) in [(600.0, 2), (680.0, 2), (720.0, 1), (680.0, 1), (600.0, 2)] {
        for _ in 0..100 {
            put(&mut table, l.now + 0.02, far);
            l.step(&table, CENTRE, 0.02);
        }
        assert_eq!(l.client.len(), want, "at {far} m");
    }
    // the same id came back
    assert!(l.client.get(table.id_of(1).unwrap()).is_some());
}

#[test]
fn standing_vehicles_cost_nothing_once_the_client_has_them() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::perfect();
    let stand = |table: &mut TrafficTable, t: f64| {
        table.begin(t);
        for i in 0..80 {
            let r = Row::from_state([CENTRE[0] + i as f64 * 3.0, CENTRE[1] + 20.0, CENTRE[2]], 90.0, 0.0, 0.0, 0.0, 0.0, 0, true, false, 0);
            table.upsert(i, Kind::Car, || look(i as usize), Meta::default, r).unwrap();
        }
        table.finish();
    };
    for _ in 0..100 {
        stand(&mut table, l.now + 0.02);
        l.step(&table, CENTRE, 0.02);
    }
    assert_eq!(l.client.len(), 80);
    let before = l.bytes_down;
    for _ in 0..250 {
        stand(&mut table, l.now + 0.02);
        l.step(&table, CENTRE, 0.02);
    }
    // five seconds of 80 standing cars: nothing but the sign of life no more often than every 8 s
    assert!(l.bytes_down - before < 80 * 12, "{} bytes", l.bytes_down - before);
}

#[test]
fn the_byte_budget_holds_and_the_near_are_served_first() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::perfect();
    l.sync.bytes_per_second = 10_000.0;
    // 400 cars on rings between 20 and 600 m from the viewer
    let ring = |i: usize| 20.0 + (i as f64 / 400.0) * 580.0;
    let put = |table: &mut TrafficTable, t: f64| {
        table.begin(t);
        for i in 0..400 {
            let r = ring(i);
            let theta = i as f64 + 12.0 * t / r;
            let pos = [CENTRE[0] + r * theta.sin(), CENTRE[1] + r * theta.cos(), CENTRE[2]];
            table.upsert(i as u64, Kind::Car, || look(i), Meta::default, Row::from_state(pos, 90.0 + theta.to_degrees(), 0.0, 0.0, 12.0, 0.0, 0, false, false, 0)).unwrap();
        }
        table.finish();
    };
    for _ in 0..500 {
        put(&mut table, l.now + 0.02);
        l.step(&table, CENTRE, 0.02);
    }
    let rate = l.bytes_down as f64 / l.now;
    assert!(rate < 10_000.0 * 1.2, "{rate} B/s over a 10000 B/s budget");
    assert!(l.sync.stats.deferred > 0, "the budget did not bind");
    assert_eq!(l.client.len(), 400, "everybody is known in the end");
    // the near ones are drawn right, the far ones less often updated (but not lost)
    let ms = l.now * 1000.0 - 100.0;
    let mut near = 0.0f64;
    for slot in table.iter().filter(|s| s.key < 60) {
        let p = l.client.sample(slot.id, ms).unwrap();
        let theta = slot.key as f64 + 12.0 * (ms / 1000.0) / ring(slot.key as usize);
        let r = ring(slot.key as usize);
        let w = [CENTRE[0] + r * theta.sin(), CENTRE[1] + r * theta.cos()];
        near = near.max(((p.pos[0] - w[0]).powi(2) + (p.pos[1] - w[1]).powi(2)).sqrt());
    }
    assert!(near < 1.5, "the nearest are {near} m off");
}

#[test]
fn a_vehicle_the_client_lost_track_of_is_told_again() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::perfect();
    run(&mut l, &mut table, 0..5, 2.0, 100.0);
    let id = table.id_of(2).unwrap();
    // the client's table loses a row by itself (a bug, a purge): its next row for it is
    // unknown, and the host starts the vehicle over
    l.client = ClientTable::default();
    run(&mut l, &mut table, 0..5, 3.0, 100.0);
    assert_eq!(l.client.len(), 5);
    assert!(l.client.get(id).is_some());
}

#[test]
fn a_started_over_host_makes_the_client_start_over_too() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::perfect();
    run(&mut l, &mut table, 0..5, 2.0, 100.0);
    l.client.take_events();
    l.sync.reset();
    run(&mut l, &mut table, 0..5, 2.0, 100.0);
    let e = l.client.take_events();
    assert_eq!(e.iter().filter(|e| matches!(e, Event::Removed(_))).count(), 5);
    assert_eq!(e.iter().filter(|e| matches!(e, Event::Spawned(_))).count(), 5);
    assert_eq!(l.client.len(), 5);
}

#[test]
fn a_late_copy_of_an_old_spawn_is_no_news() {
    let mut c = ClientTable::default();
    let id = TrafficId::new(5, 1);
    let row = Row::default();
    let spawn = |snap: u16| Packet { epoch: 1, seq: snap, snap, anchor: Anchor::default(), looks: vec![(0, look(0))], spawns: vec![Spawn { id, look: 0, kind: Kind::Car, row }], ..Default::default() };
    c.apply(spawn(10), 0.0);
    c.apply(Packet { epoch: 1, seq: 50, snap: 50, removes: vec![id], ..Default::default() }, 1.0);
    assert!(c.get(id).is_none());
    // the first spawn arrives again, late: nothing
    c.apply(spawn(10), 1.5);
    assert!(c.get(id).is_none(), "an old spawn brought a removed vehicle back");
    // a spawn made after the removal does
    c.apply(spawn(60), 2.0);
    assert!(c.get(id).is_some());
    // and a late copy of the removal does not take it away again
    c.apply(Packet { epoch: 1, seq: 51, snap: 50, removes: vec![id], ..Default::default() }, 2.1);
    assert!(c.get(id).is_some());
    assert_eq!(c.take_events(), vec![Event::Spawned(id), Event::Removed(id), Event::Spawned(id)]);
}

#[test]
fn deltas_are_against_what_the_client_acknowledged() {
    // the host's rows change every snapshot, the client's answers are lost: the host keeps
    // telling changes of the row it knows the client holds, and the client can follow
    let mut table = TrafficTable::default();
    let mut l = Loopback::new(Link::new(0.0, 0.0, 0.0, 1), Link::new(1.0, 0.0, 0.0, 2));
    run(&mut l, &mut table, 0..4, 3.0, 100.0);
    // (no ack came back: nothing is known to be held, every row goes whole)
    assert_eq!(l.sync.stats.delta_rows, 0);
    assert_eq!(l.client.len(), 4);
    let (n, worst) = error(&l, &table, l.now * 1000.0 - 150.0, 100.0);
    assert_eq!(n, 4);
    assert!(worst < 0.2, "{worst}");
}

#[test]
fn a_line_and_destination_arrive_with_the_bus_and_when_they_change() {
    let mut table = TrafficTable::default();
    let mut l = Loopback::new(Link::new(0.3, 0.02, 0.02, 3), Link::new(0.3, 0.02, 0.02, 4));
    table.begin(0.0);
    let id = table
        .upsert(1, Kind::Bus, || look(1), || Meta { line: "37".into(), destination: "#4".into(), number: "80.849".into(), ident: "EMT-1234".into(), hof: "Brigadeiro".into() }, truth(1, 0.0, 50.0))
        .unwrap();
    table.finish();
    for _ in 0..200 {
        write_with(&mut table, id, l.now + 0.02);
        l.step(&table, CENTRE, 0.02);
    }
    let r = l.client.get(id).unwrap();
    assert_eq!((r.meta.line.as_str(), r.meta.destination.as_str()), ("37", "#4"));
    assert_eq!((r.meta.number.as_str(), r.meta.ident.as_str(), r.meta.hof.as_str()), ("80.849", "EMT-1234", "Brigadeiro"), "the fleet number, plate and depot file come with it");
    assert_eq!(l.client.look(r).map(|x| x.file.as_str()), Some("Vehicles/Car1/car1.bus"));
    table.set_meta(id, &Meta { line: "37".into(), destination: "#9".into(), ..Default::default() });
    for _ in 0..200 {
        write_with(&mut table, id, l.now + 0.02);
        l.step(&table, CENTRE, 0.02);
    }
    assert_eq!(l.client.get(id).unwrap().meta.destination, "#9");
    assert!(l.client.take_events().contains(&Event::Meta(id)));
}

fn write_with(table: &mut TrafficTable, id: TrafficId, t: f64) {
    let key = table.get(id).unwrap().key;
    table.begin(t);
    table.upsert(key, Kind::Bus, || unreachable!(), Meta::default, truth(1, t, 50.0)).unwrap();
    table.finish();
}

#[test]
fn between_poses_a_vehicle_glides_and_past_them_it_is_guessed_on() {
    let mut c = ClientTable::default();
    let id = TrafficId::new(1, 1);
    let mk = |x: f64, y: f64, h: f64| Row::from_state([x, y, 10.0], h, 0.0, 0.0, 10.0, 0.0, 0, false, false, 0);
    // heading east at 10 m/s: 1 m every 100 ms
    let packet = |snap: u16, ms: u32, row: Row, spawn: bool| Packet {
        epoch: 1,
        seq: snap,
        snap,
        host_ms: ms,
        anchor: Anchor::at([1000.0, 2000.0, 10.0]),
        looks: if spawn { vec![(0, look(0))] } else { vec![] },
        spawns: if spawn { vec![Spawn { id, look: 0, kind: Kind::Car, row }] } else { vec![] },
        rows: if spawn { vec![] } else { vec![RowUpdate::Full { id, row }] },
        ..Default::default()
    };
    c.apply(packet(1, 1000, mk(1000.0, 2000.0, 90.0), true), 0.0);
    c.apply(packet(2, 1100, mk(1001.0, 2000.0, 90.0), false), 0.1);
    c.apply(packet(3, 1200, mk(1002.0, 2000.0, 90.0), false), 0.2);
    let mid = c.sample(id, 1150.0).unwrap();
    assert!((mid.pos[0] - 1001.5).abs() < 0.02 && !mid.extrapolated, "{:?}", mid.pos);
    let past = c.sample(id, 1300.0).unwrap();
    assert!((past.pos[0] - 1003.0).abs() < 0.05 && past.extrapolated, "{:?}", past.pos);
    // guessing stops after MAX_EXTRAPOLATE
    let far = c.sample(id, 3000.0).unwrap();
    assert!((far.pos[0] - 1006.0).abs() < 0.05, "{:?}", far.pos);
    // before the first pose: the first
    assert!((c.sample(id, 500.0).unwrap().pos[0] - 1000.0).abs() < 0.02);
    // a teleport is not glided over
    c.apply(packet(4, 1300, mk(1500.0, 2000.0, 90.0), false), 0.3);
    let t = c.sample(id, 1250.0).unwrap();
    assert!(t.teleported && (t.pos[0] - 1500.0).abs() < 0.02);
    // a turn: heading goes the short way round north
    let mut d = ClientTable::default();
    d.apply(packet(1, 1000, mk(1000.0, 2000.0, 350.0), true), 0.0);
    d.apply(packet(2, 1100, mk(1000.0, 2001.0, 10.0), false), 0.1);
    let h = d.sample(id, 1050.0).unwrap().heading;
    assert!(h > 359.0 || h < 1.0, "{h}");
    // a vehicle that stands is not guessed on
    let mut s = ClientTable::default();
    let still = Row::from_state([1000.0, 2000.0, 10.0], 0.0, 0.0, 0.0, 0.0, 0.0, 0, false, false, 0);
    s.apply(packet(1, 1000, still, true), 0.0);
    s.apply(packet(2, 1100, still, false), 0.1);
    assert_eq!(s.sample(id, 1400.0).unwrap().pos[0], 1000.0);
    // the flags switch past the middle of the two poses
    let mut f = ClientTable::default();
    let on = Row { flags: 4, ..still };
    f.apply(packet(1, 1000, still, true), 0.0);
    f.apply(packet(2, 1100, on, false), 0.1);
    assert!(!f.sample(id, 1040.0).unwrap().brake && f.sample(id, 1060.0).unwrap().brake);
}

#[test]
fn a_vehicle_nobody_mentions_for_long_is_let_go_while_the_host_speaks() {
    let mut c = ClientTable::default();
    let id = TrafficId::new(2, 1);
    let spawn = Packet { epoch: 1, seq: 1, snap: 1, looks: vec![(0, look(0))], spawns: vec![Spawn { id, look: 0, kind: Kind::Car, row: Row::default() }], ..Default::default() };
    c.apply(spawn, 0.0);
    c.tick(SILENT_PURGE + 1.0);
    assert!(c.get(id).is_some(), "the host said nothing at all: the session handles that");
    c.apply(Packet { epoch: 1, seq: 2, snap: 2, ..Default::default() }, SILENT_PURGE + 1.0);
    c.tick(SILENT_PURGE + 2.0);
    assert!(c.get(id).is_none());
}

#[test]
fn the_wire_survives_garbage() {
    let mut seed = 12345u64;
    let mut c = ClientTable::default();
    for n in 0..2000 {
        let len = 1 + (n % 90);
        let mut data: Vec<u8> = (0..len)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (seed >> 33) as u8
            })
            .collect();
        data[0] = wire::MAGIC;
        if data.len() > 1 {
            data[1] = crate::PROTOCOL as u8;
        }
        c.on_packet(&data, 0.0);
        let _ = wire::decode_ack(&data, crate::PROTOCOL as u8);
    }
}
