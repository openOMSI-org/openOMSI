//! Scenes for the sideways moves (`lateral`): lanes changed into one car at a time, no
//! swinging from lane to lane, a route change that waits instead of jumping, lanes that
//! close to cars, and passing on the other half of the road - when it may, and when it may
//! not.

use super::*;
use crate::ai_traffic::setup::RandomTypes;
use crate::traffic::{Lane, LaneBuilder};
use std::sync::atomic::{AtomicUsize, Ordering};

const DT: f32 = super::fixed_step::SIM_DT;

/// A vehicle type with no model and no scripts, in a folder of its own.
struct Fixture {
    dir: std::path::PathBuf,
    ty: Arc<VehicleType>,
}

impl Fixture {
    fn new() -> Fixture {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("omsi-traffic-lateral-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("car.bus"), "[model]\nmodel.cfg\n").unwrap();
        std::fs::write(dir.join("model.cfg"), "").unwrap();
        let ty = Arc::new(VehicleType::load_ai(&dir, &dir.join("car.bus")).unwrap());
        Fixture { dir, ty }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn traffic(f: &Fixture, mut net: Network) -> TrafficSim {
    net.build_grid();
    net.compute_reach();
    let random = RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
    TrafficSim::assemble(&f.dir, net, random, Vec::new(), HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0)
}

fn add_car(t: &mut TrafficSim, f: &Fixture, lane: usize, s: f32, seed: u64, speed: Option<f32>) -> u64 {
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    t.place_car(vehicle, LaneKind::Street, lane, s, f.ty.clone(), seed, None, None, speed, None)
}

fn street(x: f64, y: f64, heading: f64, length: f64) -> Lane {
    LaneBuilder::arc(DVec3::new(x, y, 0.0), heading, length, 0.0, 0.0, LaneKind::Street, 3.0)
}

/// A lane closed to cars: it goes on as a bus lane.
fn bus_lane(mut l: Lane) -> Lane {
    l.no_cars = true;
    l.rule_bus = true;
    l
}

fn net_of(lanes: Vec<Lane>, beside: &[(usize, usize)]) -> Network {
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    // (`left` lies to the left of `right`, both northbound)
    for &(left, right) in beside {
        net.lanes[left].right = Some(right);
        net.lanes[right].left = Some(left);
    }
    net
}

fn car(t: &TrafficSim, id: u64) -> Option<&AiCar> {
    t.cars.iter().find(|c| c.id == id)
}

fn car_mut(t: &mut TrafficSim, id: u64) -> &mut AiCar {
    t.cars.iter_mut().find(|c| c.id == id).unwrap()
}

fn set_route(t: &mut TrafficSim, id: u64, route: Vec<usize>, s: f32) {
    let TrafficSim { net, cars, .. } = t;
    cars.iter_mut().find(|c| c.id == id).unwrap().state.set_route(net, route, s);
}

/// Do two AI vehicles' bodies overlap (by more than `slack` m)?
fn any_overlap(t: &TrafficSim, slack: f64) -> Option<(u64, u64)> {
    let feet = t.footprints();
    for (a, fa) in feet.iter().enumerate() {
        for fb in feet.iter().skip(a + 1) {
            if fa.car != fb.car && fa.overlaps(fb, -slack) {
                return Some((t.cars[fa.car].id, t.cars[fb.car].id));
            }
        }
    }
    None
}

/// Does car `id`'s body overlap the parked car standing at `s` of `lane`, `lat` to the side?
fn hits_parked(t: &TrafficSim, id: u64, lane: usize, s: f32, lat: f32) -> bool {
    let c = car(t, id).unwrap();
    let (q, h) = t.net.lanes[lane].at(s);
    let hr = (h as f64).to_radians();
    let centre = q.truncate() + DVec2::new(hr.cos(), -hr.sin()) * lat as f64;
    let parked = Footprint::from_obb(usize::MAX, &Obb::vehicle(centre, h as f64, 2.3, 2.3, 0.9), 0.0);
    car_foot(c, 0.0).overlaps(&parked, -0.05)
}

/// Three lanes north side by side for 150 m (0 left, 1 middle, 2 right); after them the
/// middle goes on for 300 m, the outer two go on as bus lanes.
fn closing_outer_lanes() -> Network {
    let lanes = vec![
        street(-3.5, 0.0, 0.0, 150.0),
        street(0.0, 0.0, 0.0, 150.0),
        street(3.5, 0.0, 0.0, 150.0),
        bus_lane(street(-3.5, 150.0, 0.0, 300.0)),
        street(0.0, 150.0, 0.0, 2000.0),
        bus_lane(street(3.5, 150.0, 0.0, 300.0)),
    ];
    net_of(lanes, &[(0, 1), (1, 2)])
}

#[test]
fn two_cars_whose_lanes_close_move_into_the_middle_one_at_a_time() {
    // both outer lanes close; both cars have to move into the middle one at the same
    // place and the same moment. One goes first, the other after it, and neither drives
    // into a bus lane.
    let f = Fixture::new();
    let mut t = traffic(&f, closing_outer_lanes());
    let a = add_car(&mut t, &f, 0, 30.0, 0x101, Some(8.0));
    let b = add_car(&mut t, &f, 2, 30.0, 0x202, Some(8.0));
    let mut both_moving_over = 0;
    for _ in 0..(40.0 / DT) as usize {
        t.tick(DT, None);
        let into_middle: Vec<f32> = t.cars.iter().filter_map(|c| c.state.change.filter(|ch| ch.to == 1 || ch.to == 4).map(|ch| ch.s_to)).collect();
        if into_middle.len() == 2 && (into_middle[0] - into_middle[1]).abs() < 15.0 {
            both_moving_over += 1;
        }
        assert_eq!(any_overlap(&t, 0.1), None, "bodies overlap at t={:.2}", t.time);
        for c in &t.cars {
            assert!(c.state.lane != 3 && c.state.lane != 5, "car {} drove into the bus lane", c.id);
        }
    }
    assert_eq!(both_moving_over, 0, "two cars moved into the middle lane beside each other");
    for id in [a, b] {
        let c = car(&t, id).expect("still on the road");
        assert_eq!(c.state.lane, 4, "car {id} is on lane {} at s {:.1}", c.state.lane, c.state.s);
    }
}

#[test]
fn a_car_whose_lane_closes_with_no_way_on_leaves_instead_of_entering_it() {
    let f = Fixture::new();
    let net = net_of(vec![street(0.0, 0.0, 0.0, 120.0), bus_lane(street(0.0, 120.0, 0.0, 300.0))], &[]);
    let mut t = traffic(&f, net);
    let id = add_car(&mut t, &f, 0, 20.0, 0x303, Some(8.0));
    let mut gone = false;
    for _ in 0..(30.0 / DT) as usize {
        t.tick(DT, None);
        match car(&t, id) {
            Some(c) => {
                assert_eq!(c.state.lane, 0, "it drove into the bus lane");
                gone |= c.gone;
            }
            None => break,
        }
    }
    assert!(gone || car(&t, id).is_none(), "the car stays on the road for good");
}

#[test]
fn a_car_does_not_keep_right_into_a_lane_that_closes() {
    // the right lane is free, but it goes on as a bus lane after 120 m
    let f = Fixture::new();
    let lanes = vec![street(0.0, 0.0, 0.0, 120.0), street(3.5, 0.0, 0.0, 120.0), street(0.0, 120.0, 0.0, 600.0), bus_lane(street(3.5, 120.0, 0.0, 600.0))];
    let mut t = traffic(&f, net_of(lanes, &[(0, 1)]));
    let id = add_car(&mut t, &f, 0, 5.0, 0x404, Some(10.0));
    for _ in 0..(25.0 / DT) as usize {
        t.tick(DT, None);
        let c = car(&t, id).expect("on the road");
        assert!(c.state.lane != 1 && c.state.lane != 3 && c.state.change.is_none(), "it moved over to lane {} (change {:?})", c.state.lane, c.state.change.map(|c| c.to));
    }
    // (and with the right lane open all the way it does keep right)
    let lanes = vec![street(0.0, 0.0, 0.0, 120.0), street(3.5, 0.0, 0.0, 120.0), street(0.0, 120.0, 0.0, 600.0), street(3.5, 120.0, 0.0, 600.0)];
    let mut t = traffic(&f, net_of(lanes, &[(0, 1), (2, 3)]));
    let id = add_car(&mut t, &f, 0, 5.0, 0x404, Some(10.0));
    let mut kept_right = false;
    for _ in 0..(25.0 / DT) as usize {
        t.tick(DT, None);
        let c = car(&t, id).expect("on the road");
        kept_right |= c.state.lane == 1 || c.state.lane == 3;
    }
    assert!(kept_right, "with the lane open it keeps right");
}

#[test]
fn a_wish_to_change_lanes_holds_before_the_car_moves_over() {
    // a slow car ahead on the right lane, the left lane free: the overtake begins only once
    // the wish has held for a while
    let f = Fixture::new();
    let lanes = vec![street(-3.5, 0.0, 0.0, 1500.0), street(0.0, 0.0, 0.0, 1500.0)];
    let mut t = traffic(&f, net_of(lanes, &[(0, 1)]));
    let slow = add_car(&mut t, &f, 1, 60.0, 0x505, Some(4.0));
    let me = add_car(&mut t, &f, 1, 25.0, 0x606, Some(12.0));
    car_mut(&mut t, slow).state.max_speed_kmh = 15.0;
    car_mut(&mut t, me).state.change_cooldown = 0.0;
    let (mut wished, mut moved) = (None, None);
    for _ in 0..(20.0 / DT) as usize {
        t.tick(DT, None);
        let c = car(&t, me).unwrap();
        if wished.is_none() && c.sideways.wish.is_some() {
            wished = Some(t.time);
        }
        if moved.is_none() && c.state.change.is_some() {
            moved = Some(t.time);
        }
    }
    let (w, m) = (wished.expect("it wants to overtake"), moved.expect("it overtakes"));
    assert!(m - w >= lateral::WISH_HOLD - 0.05, "it moved over {:.2} s after it wished to", m - w);
}

#[test]
fn a_car_overtakes_on_a_road_built_of_short_pieces() {
    // two lanes north made of 40 m spline pieces: the move carries on across the joints,
    // so a car stuck behind a slow one closes up and goes past it (held to the end of each
    // piece, it never found the room)
    let f = Fixture::new();
    let mut lanes = Vec::new();
    for k in 0..40 {
        lanes.push(street(-3.5, k as f64 * 40.0, 0.0, 40.0));
        lanes.push(street(0.0, k as f64 * 40.0, 0.0, 40.0));
    }
    let pairs: Vec<(usize, usize)> = (0..40).map(|k| (2 * k, 2 * k + 1)).collect();
    let mut t = traffic(&f, net_of(lanes, &pairs));
    let slow = add_car(&mut t, &f, 3, 10.0, 0x515, Some(4.0));
    let me = add_car(&mut t, &f, 1, 10.0, 0x616, Some(10.0));
    car_mut(&mut t, slow).state.max_speed_kmh = 15.0;
    let mut past = false;
    for _ in 0..(40.0 / DT) as usize {
        t.tick(DT, None);
        let (a, b) = (car(&t, me).unwrap(), car(&t, slow).unwrap());
        past |= a.vehicle.position.y > b.vehicle.position.y + 10.0;
        assert_eq!(any_overlap(&t, 0.1), None, "bodies overlap at t={:.2}", t.time);
    }
    assert!(past, "it stayed behind the slow car");
}

#[test]
fn cars_on_a_two_lane_road_do_not_swing_from_lane_to_lane() {
    // a dozen cars of all speeds on a long two-lane road: no car changes back within
    // `NO_SWING_BACK` of a change, and no two cars begin a change into one lane near each
    // other in the same step
    let f = Fixture::new();
    let lanes = vec![street(-3.5, 0.0, 0.0, 3000.0), street(0.0, 0.0, 0.0, 3000.0)];
    let mut t = traffic(&f, net_of(lanes, &[(0, 1)]));
    let mut ids = Vec::new();
    for k in 0..12u64 {
        let lane = (k % 2) as usize;
        let id = add_car(&mut t, &f, lane, 20.0 + k as f32 * 35.0, 0x1000 + k * 0x3b1, Some(8.0));
        if k % 3 == 0 {
            car_mut(&mut t, id).state.max_speed_kmh = 28.0 + k as f32;
        }
        ids.push(id);
    }
    let mut last: HashMap<u64, (i32, f32)> = HashMap::new();
    let mut changes = 0;
    for _ in 0..(120.0 / DT) as usize {
        let before: HashMap<u64, bool> = t.cars.iter().map(|c| (c.id, c.state.change.is_some())).collect();
        t.tick(DT, None);
        let begun: Vec<(u64, usize, f32, i32)> = t.cars.iter().filter(|c| c.state.change.is_some() && !before.get(&c.id).copied().unwrap_or(true)).map(|c| { let ch = c.state.change.unwrap(); (c.id, ch.to, ch.s_to, ch.dir) }).collect();
        for (k, a) in begun.iter().enumerate() {
            for b in &begun[k + 1..] {
                assert!(a.1 != b.1 || (a.2 - b.2).abs() >= lateral::CHANGE_SPACING, "cars {} and {} began moving into lane {} together", a.0, b.0, a.1);
            }
            if let Some(&(dir, at)) = last.get(&a.0) {
                assert!(dir == a.3 || t.time - at >= lateral::NO_SWING_BACK - 0.05, "car {} swung back after {:.1} s", a.0, t.time - at);
            }
            last.insert(a.0, (a.3, t.time));
            changes += 1;
        }
        assert_eq!(any_overlap(&t, 0.1), None, "bodies overlap at t={:.2}", t.time);
    }
    assert!(changes > 3, "the cars hardly changed lanes ({changes})");
}

/// Lane 0 north (150 m, then 1 its way on), lane 2 beside it on the right (150 m, then 3).
fn route_fork() -> Network {
    let lanes = vec![street(0.0, 0.0, 0.0, 150.0), street(0.0, 150.0, 0.0, 300.0), street(3.5, 0.0, 0.0, 150.0), street(3.5, 150.0, 0.0, 300.0)];
    net_of(lanes, &[(0, 2)])
}

#[test]
fn a_route_change_into_a_full_lane_waits_and_never_jumps() {
    // the route moves from lane 0 over to lane 2 and on to 3; lane 2 is full of standing
    // cars. The car waits before the end of its lane, indicating; it neither cuts in nor
    // turns up on lane 3 without having been on lane 2.
    let f = Fixture::new();
    let mut t = traffic(&f, route_fork());
    let me = add_car(&mut t, &f, 0, 10.0, 0x707, Some(8.0));
    set_route(&mut t, me, vec![0, 2, 3], 10.0);
    let mut blockers = Vec::new();
    let mut s = 3.0;
    while s < 148.0 {
        blockers.push(add_car(&mut t, &f, 2, s, 0x800 + s as u64, Some(0.0)));
        s += 6.5;
    }
    let hold = |t: &mut TrafficSim, ids: &[u64]| {
        for c in t.cars.iter_mut().filter(|c| ids.contains(&c.id)) {
            c.state.speed = 0.0;
            c.state.max_speed_kmh = 0.0;
        }
    };
    for _ in 0..(40.0 / DT) as usize {
        hold(&mut t, &blockers);
        t.tick(DT, None);
        let c = car(&t, me).expect("the car is still on the road");
        assert_eq!(c.state.lane, 0, "it left its lane for {} at s {:.1}", c.state.lane, c.state.s);
    }
    let c = car(&t, me).unwrap();
    assert!(c.state.speed < 0.1 && c.state.s < 150.0 - c.state.front, "it waits short of the end (s {:.1}, {:.1} m/s)", c.state.s, c.state.speed);
    assert!(c.state.change.is_none() && c.state.lateral.abs() < 0.05);
    assert_eq!(c.state.blinker, 2, "it indicates right while it waits");
    // the lane clears: over it goes, through lane 2
    t.cars.retain(|c| !blockers.contains(&c.id));
    let mut via_2 = false;
    let mut prev = car(&t, me).unwrap().vehicle.position;
    for _ in 0..(30.0 / DT) as usize {
        t.tick(DT, None);
        let Some(c) = car(&t, me) else { break };
        via_2 |= c.state.lane == 2 || c.state.change.is_some_and(|ch| ch.to == 2);
        let p = c.vehicle.position;
        assert!((p - prev).truncate().length() < 1.0, "the body jumped {:.2} m", (p - prev).truncate().length());
        prev = p;
        if c.state.lane == 3 {
            break;
        }
    }
    let c = car(&t, me).expect("on the road");
    assert!(via_2 && c.state.lane == 3, "on lane {} (via lane 2: {via_2})", c.state.lane);
}

#[test]
fn a_required_change_comes_before_an_optional_way_round() {
    // three lanes; the middle one closes after 150 m, something stands in it ahead. The
    // car moves over to the lane on its own side (the change its way requires) rather
    // than pulling out round the obstacle to the left first.
    let f = Fixture::new();
    let lanes = vec![
        street(-3.5, 0.0, 0.0, 150.0),
        street(0.0, 0.0, 0.0, 150.0),
        street(3.5, 0.0, 0.0, 150.0),
        street(-3.5, 150.0, 0.0, 300.0),
        bus_lane(street(0.0, 150.0, 0.0, 300.0)),
        street(3.5, 150.0, 0.0, 300.0),
    ];
    let mut t = traffic(&f, net_of(lanes, &[(0, 1), (1, 2)]));
    t.parked.entry(1).or_default().push((60.0, 0.0));
    let me = add_car(&mut t, &f, 1, 20.0, 0x909, Some(0.0));
    let mut went = None;
    for _ in 0..(30.0 / DT) as usize {
        t.tick(DT, None);
        let c = car(&t, me).expect("on the road");
        if let Some(ch) = c.state.change {
            went.get_or_insert(ch.to);
        }
    }
    assert_eq!(went, Some(2), "it moved over to {went:?}");
}

/// A road with a lane each way: lane 0 north along x = 0, lane 1 south along x = `apart`
/// to the west - or along `oncoming` when given (the points of the southbound lane).
fn two_way(apart: f64, oncoming: Option<Vec<DVec3>>) -> Network {
    let north = street(0.0, 0.0, 0.0, 400.0);
    let south = match oncoming {
        Some(points) => LaneBuilder::polyline(points, LaneKind::Street, 3.0),
        None => street(-apart, 400.0, 180.0, 400.0),
    };
    net_of(vec![north, south], &[])
}

/// Car `me` drives up to a car parked in the middle of lane 0 at 100 m: did it go round
/// it on the other half of the road within `secs`? (It never touches it.)
fn passes_parked(t: &mut TrafficSim, me: u64, secs: f32) -> bool {
    for _ in 0..(secs / DT) as usize {
        t.tick(DT, None);
        let Some(c) = car(t, me) else { return true };
        assert!(!hits_parked(t, me, 0, 100.0, 0.0), "it ran into the parked car");
        if c.state.lane == 0 && c.state.s > 110.0 {
            return true;
        }
    }
    false
}

fn parked_scene(f: &Fixture, net: Network) -> (TrafficSim, u64) {
    let mut t = traffic(f, net);
    t.parked.entry(0).or_default().push((100.0, 0.0));
    let me = add_car(&mut t, f, 0, 40.0, 0xa0a, Some(6.0));
    (t, me)
}

#[test]
fn a_car_passes_on_the_oncoming_lane_of_its_own_carriageway_only() {
    let f = Fixture::new();
    let (mut t, me) = parked_scene(&f, two_way(3.5, None));
    assert!(passes_parked(&mut t, me, 30.0), "it does not go round the parked car");
    // the other carriageway of a divided road is no oncoming lane to pass on (#126)
    let (mut t, me) = parked_scene(&f, two_way(5.5, None));
    assert!(!passes_parked(&mut t, me, 30.0), "it passed on the other carriageway");
    assert!(car(&t, me).unwrap().passing.is_none());
}

#[test]
fn an_island_beginning_after_the_pull_out_forbids_the_pass() {
    // the oncoming lane runs beside at 3.5 m where the car stands, but an island takes it
    // 6 m away just past the parked car
    let f = Fixture::new();
    let points = vec![DVec3::new(-6.0, 400.0, 0.0), DVec3::new(-6.0, 106.0, 0.0), DVec3::new(-3.5, 96.0, 0.0), DVec3::new(-3.5, 0.0, 0.0)];
    let (mut t, me) = parked_scene(&f, two_way(3.5, Some(points)));
    assert!(!passes_parked(&mut t, me, 30.0), "it passed across the island");
    // an island the lanes do not show, a kerb in the scenery between the two lanes
    let (mut t, me) = parked_scene(&f, two_way(3.5, None));
    let island = Obb { center: DVec2::new(-1.75, 125.0), half: DVec2::new(0.4, 18.0), heading: 0.0, z0: 0.0, z1: 0.3, velocity: DVec2::ZERO, mass: 0.0, pole: None, id: -1 };
    t.scenery_probe = Some(Arc::new(move |b: &Obb| b.overlaps(&island)));
    assert!(!passes_parked(&mut t, me, 30.0), "it passed through the scenery");
}

#[test]
fn somebody_on_the_other_half_of_the_road_forbids_the_pass() {
    let f = Fixture::new();
    let (mut t, me) = parked_scene(&f, two_way(3.5, None));
    t.people = vec![(DVec3::new(-3.5, 106.0, 0.0), DVec2::ZERO, false)];
    for _ in 0..(30.0 / DT) as usize {
        t.tick(DT, None);
        assert!(car(&t, me).unwrap().passing.is_none(), "it pulled out towards somebody on foot");
    }
}

#[test]
fn a_wider_vehicle_does_not_start_a_pass_a_car_can_make() {
    // a car parked at the far kerb of the oncoming lane leaves room for a car out there,
    // not for a bus
    let f = Fixture::new();
    let wide = |t: &mut TrafficSim, me: u64| {
        let c = car_mut(t, me);
        c.half_width = 1.25;
    };
    let far_kerb = |t: &mut TrafficSim| t.parked.entry(1).or_default().push((400.0 - 106.0, 2.2));
    let (mut t, me) = parked_scene(&f, two_way(3.5, None));
    far_kerb(&mut t);
    assert!(passes_parked(&mut t, me, 30.0), "the car does not pass");
    let (mut t, me) = parked_scene(&f, two_way(3.5, None));
    wide(&mut t, me);
    assert!(passes_parked(&mut t, me, 30.0), "the wide one does not pass even with room");
    let (mut t, me) = parked_scene(&f, two_way(3.5, None));
    far_kerb(&mut t);
    wide(&mut t, me);
    assert!(!passes_parked(&mut t, me, 30.0), "the wide one passed the parked car out there");
}

#[test]
fn a_pass_is_given_up_when_somebody_comes_the_other_way() {
    let f = Fixture::new();
    let (mut t, me) = parked_scene(&f, two_way(3.5, None));
    let mut oncoming = None;
    let mut gave_up = false;
    for _ in 0..(40.0 / DT) as usize {
        t.tick(DT, None);
        let c = car(&t, me).expect("on the road");
        let y = c.vehicle.position.y as f32;
        if oncoming.is_none() && c.passing.is_some() {
            // the moment it pulls out, a car comes the other way, 55 m off
            oncoming = Some(add_car(&mut t, &f, 1, 400.0 - (y + 55.0), 0xb0b, Some(13.0)));
        }
        let c = car(&t, me).unwrap();
        gave_up |= c.passing.is_some_and(|p| p.aborted);
        assert!(!hits_parked(&t, me, 0, 100.0, 0.0), "it ran into the parked car");
        assert_eq!(any_overlap(&t, 0.05), None, "the two met at t={:.2}", t.time);
    }
    assert!(oncoming.is_some(), "it never pulled out");
    assert!(gave_up, "it did not give the pass up");
}

/// Two lanes north (0 right, 1 left, 400 m), with a timetable bus standing at its stop on
/// lane 0 at 200 m and two cars queued behind it.
#[test]
fn a_queue_behind_a_bus_at_its_stop_goes_round_it() {
    let f = Fixture::new();
    let lanes = vec![street(0.0, 0.0, 0.0, 400.0), street(-3.5, 0.0, 0.0, 400.0)];
    let mut t = traffic(&f, net_of(lanes, &[(1, 0)]));
    let bus = add_car(&mut t, &f, 0, 200.0, 0xc0c, Some(0.0));
    {
        let day = t.day_time;
        let c = car_mut(&mut t, bus);
        let mut service = bus_service::BusService::new(Vec::new());
        service.phase = Phase::Waiting;
        service.leave_at = day + 600.0;
        c.bus = Some(Box::new(service));
    }
    let queue: Vec<u64> = [188.0, 180.0, 172.0].iter().map(|&s| add_car(&mut t, &f, 0, s, 0xd00 + s as u64, Some(0.0))).collect();
    for _ in 0..(60.0 / DT) as usize {
        t.tick(DT, None);
        assert_eq!(any_overlap(&t, 0.1), None, "bodies overlap at t={:.2}", t.time);
    }
    // (those gone have driven off the end of the road)
    for c in queue.iter().filter_map(|&id| car(&t, id)) {
        assert!(c.state.lane == 1 || c.state.s > 210.0, "car {} still queues on lane {} at {:.1}", c.id, c.state.lane, c.state.s);
    }
}

#[test]
fn a_queue_at_a_red_light_is_no_stop_to_go_round() {
    let f = Fixture::new();
    let lanes = vec![street(0.0, 0.0, 0.0, 200.0), street(-3.5, 0.0, 0.0, 200.0), street(0.0, 200.0, 0.0, 300.0), street(-3.5, 200.0, 0.0, 300.0)];
    let mut net = net_of(lanes, &[(1, 0)]);
    net.lanes[2].traffic_light = Some((0, 0));
    let mut t = traffic(&f, net);
    t.lights.push(TrafficLightController::new(vec![vec![(0, 1000.0)]], 1000.0));
    let queue: Vec<u64> = [190.0, 182.0, 174.0, 166.0].iter().map(|&s| add_car(&mut t, &f, 0, s, 0xe00 + s as u64, Some(0.0))).collect();
    for _ in 0..(60.0 / DT) as usize {
        t.tick(DT, None);
    }
    for id in queue {
        let c = car(&t, id).expect("on the road");
        assert!(c.state.lane == 0 && c.state.change.is_none(), "car {id} left the queue at the red light for lane {}", c.state.lane);
    }
}

#[test]
fn a_bus_queueing_for_its_own_stop_does_not_go_round_the_queue() {
    // a road with a lane each way; ahead a bus serving the stop, a car behind it, and our
    // timetable bus behind that, its own next stop the same one
    let f = Fixture::new();
    let mut t = traffic(&f, two_way(3.5, None));
    let serving = add_car(&mut t, &f, 0, 200.0, 0xf0f, Some(0.0));
    {
        let day = t.day_time;
        let c = car_mut(&mut t, serving);
        let mut service = bus_service::BusService::new(Vec::new());
        service.phase = Phase::Waiting;
        service.leave_at = day + 600.0;
        c.bus = Some(Box::new(service));
    }
    add_car(&mut t, &f, 0, 190.0, 0xf10, Some(0.0));
    let me = add_car(&mut t, &f, 0, 160.0, 0xf11, Some(0.0));
    set_route(&mut t, me, vec![0], 160.0);
    {
        let day = t.day_time;
        let c = car_mut(&mut t, me);
        let stop = bus_service::Stop { ri: 0, s: 205.0, bay: 0.0, depart: day + 600.0, id: 1, side: 0.0 };
        c.bus = Some(Box::new(bus_service::BusService::new(vec![stop])));
    }
    for _ in 0..(40.0 / DT) as usize {
        t.tick(DT, None);
        let c = car(&t, me).expect("on the road");
        assert!(c.passing.is_none(), "the bus went round the queue for its own stop (s {:.1}, {:?})", c.state.s, c.bus.as_ref().map(|b| (b.phase, b.stops.len())));
    }
}

fn player_before_red(f: &Fixture, short: f64) -> (TrafficSim, u64, PlayerBox) {
    let lanes = vec![street(0.0, 0.0, 0.0, 200.0), street(-3.5, 400.0, 180.0, 200.0), street(0.0, 200.0, 0.0, 200.0), street(-3.5, 200.0, 180.0, 200.0)];
    let mut net = net_of(lanes, &[]);
    net.lanes[2].traffic_light = Some((0, 0));
    let mut t = traffic(f, net);
    t.lights.push(TrafficLightController::new(vec![vec![(0, 1000.0)]], 1000.0));
    let bus: PlayerBox = (DVec3::new(0.0, 200.0 - short - 6.0, 0.0), 0.0, 6.0, 1.25, 0.0);
    let me = add_car(&mut t, f, 0, 80.0, 0xa1a, Some(10.0));
    (t, me, bus)
}

#[test]
fn a_car_does_not_go_round_the_players_bus_waiting_at_a_red_light() {
    let f = Fixture::new();
    let (mut t, me, bus) = player_before_red(&f, 30.0);
    for _ in 0..(60.0 / DT) as usize {
        t.tick(DT, Some(bus));
        let c = car(&t, me).expect("on the road");
        assert!(c.passing.is_none() && c.state.lane == 0, "it went round the bus waiting at the red light (lane {}, s {:.1})", c.state.lane, c.state.s);
    }
    t.lights[0].lights[0] = vec![(6, 1000.0)];
    let mut went_round = None;
    for k in 0..(60.0 / DT) as usize {
        t.tick(DT, Some(bus));
        if went_round.is_none() && car(&t, me).expect("on the road").passing.is_some() {
            went_round = Some(k as f32 * DT);
        }
    }
    let after = went_round.expect("it does not go round the bus still standing at the green light");
    assert!(after > 9.0, "it went round the bus {after:.1} s after the light turned green");
}

#[test]
fn a_car_goes_round_the_players_bus_standing_well_before_a_red_light() {
    let f = Fixture::new();
    let (mut t, me, bus) = player_before_red(&f, 60.0);
    let mut went_round = false;
    for _ in 0..(60.0 / DT) as usize {
        t.tick(DT, Some(bus));
        let c = car(&t, me).expect("on the road");
        went_round |= c.passing.is_some();
        assert_eq!(c.state.lane, 0, "it drove through the red light");
    }
    assert!(went_round, "it does not go round the bus standing 60 m before the light");
}
