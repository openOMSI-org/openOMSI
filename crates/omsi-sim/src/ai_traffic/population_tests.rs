//! The population headless: cars going out of range and coming back, waking without
//! landing on anybody.

use super::admission::{decide_batch, Budgets, Class, Denial, Retry, Verdict};
use super::service_tests::{add_car, road, Fixture};
use super::*;

/// A car that goes out of range drives on as a dormant car and comes back as itself: its
/// id, type, seed and paint, where it has got to meanwhile.
#[test]
fn a_car_keeps_its_identity_across_streaming() {
    let f = Fixture::new();
    let mut t = road(&f);
    let id = add_car(&mut t, &f, 5, 100.0, 8.0);
    for _ in 0..100 {
        t.tick(fixed_step::SIM_DT, None);
    }
    let (seed, scheme, s0) = {
        let c = t.cars.iter().find(|c| c.id == id).unwrap();
        (c.seed, c.scheme, c.state.s)
    };
    let i = t.cars.iter().position(|c| c.id == id).unwrap();
    assert_eq!(t.put_to_sleep(i), id);
    assert!(t.cars.is_empty());
    assert_eq!(t.removals[Removal::Dormant as usize], 1);
    t.time += 10.0;
    t.advance_dormant();
    let reqs = t.wake_requests(&[DVec3::new(0.0, 300.0, 0.0)], 400.0);
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].s > s0 + 15.0, "it drove on while asleep: {} -> {}", s0, reqs[0].s);
    let mut b = Budgets::default();
    b.set(Class::Dormant, Some(5));
    assert_eq!(decide_batch(&reqs, &mut b, |r| if t.spawn_gap(r.pos) { Verdict::Admit } else { Verdict::Retry(Retry::NoGap) }), vec![Verdict::Admit]);
    let d = t.unregister_dormant(id).unwrap();
    let ty = d.ty.clone();
    let woken = t.wake(d, VehicleInstance::new(ty.clone(), crate::VehicleHost::new(crate::SimClock::default())));
    let c = t.cars.iter().find(|c| c.id == woken).unwrap();
    assert_eq!((c.id, c.seed, c.scheme), (id, seed, scheme));
    assert!(Arc::ptr_eq(&c.vehicle.ty, &ty));
    assert!((c.state.s - reqs[0].s).abs() < 0.5);
}

/// Waking never puts a car onto another: not onto one on the road, not onto one woken in
/// the same pass; a car whose ground is not loaded stays asleep. They wake in the order
/// they went to sleep.
#[test]
fn waking_never_creates_an_overlap() {
    let f = Fixture::new();
    let mut t = road(&f);
    add_car(&mut t, &f, 1, 200.0, 0.0);
    let sleeper = |id: u64, s: f32| DormantCar { id, ty: f.ty.clone(), kind: LaneKind::Street, lane: 0, s, speed: 5.0, seed: id, scheme: None, walk: id | 1 };
    t.dormant = vec![sleeper(10, 205.0), sleeper(11, 400.0), sleeper(12, 404.0), sleeper(13, 700.0), sleeper(14, 900.0)];
    let reqs = t.wake_requests(&[DVec3::new(0.0, 500.0, 0.0)], 1000.0);
    assert_eq!(reqs.iter().map(|r| r.dormant.unwrap()).collect::<Vec<_>>(), vec![10, 11, 12, 13, 14]);
    let mut b = Budgets::default();
    b.set(Class::Dormant, Some(10));
    let verdicts = decide_batch(&reqs, &mut b, |r| {
        if r.dormant == Some(13) {
            Verdict::Deny(Denial::NoGround)
        } else if t.spawn_gap(r.pos) {
            Verdict::Admit
        } else {
            Verdict::Retry(Retry::NoGap)
        }
    });
    assert_eq!(verdicts, vec![Verdict::Retry(Retry::NoGap), Verdict::Admit, Verdict::Retry(Retry::NoGap), Verdict::Deny(Denial::NoGround), Verdict::Admit]);
    for (r, v) in reqs.iter().zip(&verdicts) {
        if *v == Verdict::Admit {
            let d = t.unregister_dormant(r.dormant.unwrap()).unwrap();
            let ty = d.ty.clone();
            t.wake(d, VehicleInstance::new(ty, crate::VehicleHost::new(crate::SimClock::default())));
        }
    }
    assert_eq!(t.dormant.iter().map(|d| d.id).collect::<Vec<_>>(), vec![10, 12, 13]);
    for (k, a) in t.cars.iter().enumerate() {
        for b in &t.cars[k + 1..] {
            assert!((a.vehicle.position - b.vehicle.position).length() >= admission::SPAWN_GAP, "{} and {}", a.id, b.id);
        }
    }
}

/// A LAN client decides nothing about who is on the road: the host does.
#[test]
fn a_lan_client_admits_nobody() {
    let f = Fixture::new();
    let mut t = road(&f);
    t.mirror = true;
    t.admission.submit(0.0, Class::Street, (0, 50.0), (DVec3::new(0.0, 50.0, 0.0), 0.0), None).unwrap();
    let mut b = Budgets::default();
    b.set(Class::Street, Some(10));
    let host = !t.is_mirror();
    assert!(t.admission.pass(0.0, host, &mut b, |_| Verdict::Admit).is_empty());
    assert_eq!(t.admission.denied[Denial::NotHost as usize], 1);
}

/// `[AIMaxCountScheduled]`: the timetable's vehicles have their own budget.
#[test]
fn the_timetable_has_a_budget_of_its_own() {
    let f = Fixture::new();
    let mut t = road(&f);
    assert_eq!(t.scheduled_room(), None);
    t.max_scheduled = 2;
    add_car(&mut t, &f, 1, 100.0, 0.0);
    assert_eq!(t.scheduled_room(), Some(2), "random cars do not count");
    t.cars[0].state.route = vec![0];
    assert_eq!(t.scheduled_room(), Some(1));
    t.no_timetable_buses = true;
    assert_eq!(t.scheduled_room(), Some(0));
}

/// A lane's spawn density weighs each group's rule there by how much traffic the map makes
/// of that group: a lane open only to a rare group stays nearly empty.
#[test]
fn a_lane_open_only_to_a_rare_group_spawns_little() {
    use super::scenario_tests::{street, Fixture};
    use super::setup::RandomTypes;
    let f = Fixture::new();
    let lane = |x: f64, rules: Vec<(u16, f32)>| {
        let mut l = street(DVec3::new(x, 0.0, 0.0), 0.0, 100.0);
        l.group_density = rules;
        l
    };
    // the delivery vans' rule of 2.5 only, cars at their default beside a tow truck rule of
    // 4, tow trucks only (4 at a fortieth of the cars' traffic, so 0.1), and cars at 0.8
    // with the vans at their 2.5 (at half the cars' traffic: 1.25); then one without rules
    let lanes = vec![
        lane(0.0, vec![(0, 0.0), (1, 2.5)]),
        lane(10.0, vec![(2, 4.0)]),
        lane(20.0, vec![(0, 0.0), (2, 4.0)]),
        lane(30.0, vec![(0, 0.8), (1, 2.5)]),
        lane(40.0, Vec::new()),
    ];
    let mut net = Network { lanes, ..Default::default() };
    net.build_grid();
    net.compute_reach();
    let group = |name: &str, factor: f32| omsi_map::ailists::UnschedGroup { name: name.into(), factor, densities: Vec::new() };
    let random = RandomTypes {
        types: (0..3).map(|g| (f.ty.clone(), 1.0, LaneKind::Street, g)).collect(),
        groups: vec![group("cars", 2.0), group("vans", 1.0), group("tow", 0.05)],
        group_curves: true,
        group_uvg: vec![Some(0), Some(1), Some(2)],
        uvg_defaults: vec![1, 0, 0],
    };
    let t = TrafficSim::assemble(&f.dir, net, random, Vec::new(), HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0);
    let d: Vec<f32> = t.net.lanes.iter().map(|l| l.density).collect();
    assert!((d[0] - 1.25).abs() < 1e-4, "{d:?}");
    assert!((d[1] - 1.0).abs() < 1e-4, "{d:?}");
    assert!((d[2] - 0.1).abs() < 1e-4, "{d:?}");
    assert!((d[3] - 1.25).abs() < 1e-4, "{d:?}");
    assert!((d[4] - 1.0).abs() < 1e-4, "{d:?}");
    // a very rare group keeps its lane a little above 0
    let mut l = lane(0.0, vec![(0, 0.0), (2, 4.0)]);
    l.density = 4.0;
    let mut t = t;
    t.groups[2].factor = 0.0004;
    assert_eq!(t.spawn_density(&l), Some(super::density::SPAWN_DENSITY_FLOOR));
}

#[test]
fn the_ai_paints_are_drawn_from_every_scheme_of_the_type() {
    let mut seen = std::collections::HashSet::new();
    for h in 0..200u64 {
        let set = super::spread_schemes(11, h.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        assert_eq!(set.len(), super::AI_SCHEMES);
        assert!(set.iter().all(|&i| i < 11));
        assert_eq!(set.iter().collect::<std::collections::HashSet<_>>().len(), set.len());
        seen.extend(set);
    }
    assert_eq!(seen.len(), 11);
    assert_eq!(super::spread_schemes(3, 7).len(), 3);
    assert!(super::spread_schemes(0, 7).is_empty());
    let salted = |salt: u64| super::spread_schemes(11, omsi_script::mix_seed(salt ^ 0xcbf2_9ce4_8422_2325));
    assert_eq!(salted(1), salted(1), "the same salt gives the same four");
    assert!((2..20).any(|s| salted(s) != salted(1)), "another salt gives others");
    assert_eq!(super::spread_schemes(11, 0x1_0000_0003), vec![7, 6, 5, 4], "the same on 32 and 64 bits: the hash is reduced as a u64");
}
