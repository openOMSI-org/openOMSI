//! Signalled crossroads of several lanes run headless: the movements that have green
//! get through, whatever the cross traffic waiting at its red does.

use super::scenario_tests::{add_car, street, Fixture};
use super::*;
use crate::ai_traffic::setup::RandomTypes;
use crate::traffic::{LaneBuilder, LaneKey, TrafficLightController};

const HALF: f64 = 14.0;
const LANES: [f64; 2] = [1.75, 5.25];

/// How the crossing object is built.
#[derive(Clone, Copy, Default)]
struct Build {
    /// Each lane's light sits on a short stop-line path (2 m) of the object, the movements
    /// through the box after it are paths of their own without a light.
    stub: bool,
    /// The north-south road has a higher `[rule] priority`.
    main_priority: bool,
}

fn fwd(h: f64) -> DVec3 {
    let r = h.to_radians();
    DVec3::new(r.sin(), r.cos(), 0.0)
}

fn right(h: f64) -> DVec3 {
    let r = h.to_radians();
    DVec3::new(r.cos(), -r.sin(), 0.0)
}

/// A crossroads of two roads with two lanes each way, one crossing object (id 7): from
/// the inner lane straight on and left, from the outer lane straight on and right. Light
/// 0 of program 0 holds the north-south approaches, light 1 the east-west ones. Returns
/// the network and, per approach (north-, east-, south-, westbound), its two lanes in.
fn crossroads(b: Build) -> (Network, Vec<[usize; 2]>) {
    let mut lanes: Vec<crate::traffic::Lane> = Vec::new();
    let mut path_no = 0u16;
    let mut ins = Vec::new();
    let mut obj = |mut l: crate::traffic::Lane, light: Option<usize>, prio: bool, lanes: &mut Vec<crate::traffic::Lane>| {
        l.source = 2;
        l.key = Some(LaneKey { tile: (0, 0), id: 7, path: path_no });
        path_no += 1;
        l.traffic_light = light.map(|li| (0, li));
        if prio {
            l.priority = crate::traffic::DEFAULT_PRIORITY + 1.0;
        }
        lanes.push(l);
    };
    for d in 0..4 {
        let h = d as f64 * 90.0;
        let light = d % 2;
        let prio = b.main_priority && light == 0;
        let mut pair = [0usize; 2];
        for (k, &off) in LANES.iter().enumerate() {
            let line = -HALF * fwd(h) + off * right(h);
            pair[k] = lanes.len();
            lanes.push(street(line - 150.0 * fwd(h), h, 150.0));
            // out of the box
            lanes.push(street(HALF * fwd(h) + off * right(h), h, 150.0));
            let start = if b.stub {
                obj(street(line, h, 2.0), Some(light), prio, &mut lanes);
                line + 2.0 * fwd(h)
            } else {
                line
            };
            let lit = (!b.stub).then_some(light);
            let pre = if b.stub { 2.0 } else { 0.0 };
            obj(street(start, h, 2.0 * HALF - pre), lit, prio, &mut lanes);
            if k == 0 {
                let r = HALF + off;
                let mut l = LaneBuilder::arc(start, h, (r - pre) * std::f64::consts::FRAC_PI_2, -(r - pre), 0.0, LaneKind::Street, 3.0);
                l.turn = 1;
                obj(l, lit, prio, &mut lanes);
            } else {
                let r = HALF - off;
                let mut l = LaneBuilder::arc(start, h, (r - pre).max(1.0) * std::f64::consts::FRAC_PI_2, (r - pre).max(1.0), 0.0, LaneKind::Street, 3.0);
                l.turn = 2;
                obj(l, lit, prio, &mut lanes);
            }
        }
        ins.push(pair);
    }
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    (net, ins)
}

fn traffic(f: &Fixture, mut net: Network) -> TrafficSim {
    net.build_grid();
    net.compute_reach();
    let random = RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
    let ctl = TrafficLightController::new(vec![vec![(6, 1000.0)], vec![(0, 1000.0)]], 0.0);
    TrafficSim::assemble(&f.dir, net, random, vec![ctl], HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0)
}

/// Cars on every lane in, `n` per lane 10 m apart up to the line; north-south green. After
/// `secs`: how many of the cars from the green approaches and from the red ones have left
/// the lanes they came in on.
fn run(b: Build, n: usize, secs: f32) -> (usize, usize, usize) {
    let f = Fixture::new();
    let (net, ins) = crossroads(b);
    let mut t = traffic(&f, net);
    let mut seed = 0x400;
    let mut cars = Vec::new();
    for (d, pair) in ins.iter().enumerate() {
        for &lane in pair {
            for k in 0..n {
                seed += 0x13;
                let id = add_car(&mut t, &f, lane, 140.0 - 10.0 * k as f32, seed, Some(0.0));
                cars.push((id, lane, d % 2 == 0));
            }
        }
    }
    for _ in 0..(secs / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
    }
    let left = |green: bool| cars.iter().filter(|(id, lane, g)| *g == green && t.cars.iter().find(|c| c.id == *id).is_none_or(|c| c.state.lane != *lane)).count();
    (left(true), left(false), cars.len() / 2)
}

/// The crossroads with a light cycle (north-south green for `green` s, then all red 3 s,
/// east-west green, all red) and a car put on every lane in whenever its start is free.
/// Returns how many cars came through from each approach (north-, east-, south-, westbound).
fn cycle(b: Build, green: f32, secs: f32) -> [usize; 4] {
    let f = Fixture::new();
    let (net, ins) = crossroads(b);
    let mut t = traffic(&f, net);
    let period = 2.0 * (green + 3.0);
    t.lights[0] = TrafficLightController::new(
        vec![vec![(6, green), (0, period - green)], vec![(0, green + 3.0), (6, green), (0, 3.0)]],
        period,
    );
    let mut seed = 0x400;
    let mut through = [0usize; 4];
    let mut came: Vec<(u64, usize, usize)> = Vec::new();
    for _ in 0..(secs / super::fixed_step::SIM_DT) as usize {
        for (d, pair) in ins.iter().enumerate() {
            for &lane in pair {
                if !t.cars.iter().any(|c| c.state.lane == lane && c.state.s < 15.0) {
                    seed += 0x13;
                    let id = add_car(&mut t, &f, lane, 2.0, seed, Some(8.0));
                    came.push((id, lane, d));
                }
            }
        }
        t.tick(super::fixed_step::SIM_DT, None);
        came.retain(|&(id, lane, d)| {
            let gone = t.cars.iter().find(|c| c.id == id).is_none_or(|c| c.state.lane != lane);
            if gone {
                through[d] += 1;
            }
            !gone
        });
    }
    through
}

#[test]
fn green_movements_of_a_multi_lane_crossroads_get_through() {
    for b in [Build::default(), Build { stub: true, ..Default::default() }, Build { main_priority: true, ..Default::default() }, Build { stub: true, main_priority: true }] {
        let (green, red, each) = run(b, 3, 40.0);
        assert_eq!(red, 0, "a car went from a red approach");
        assert_eq!(green, each, "cars from the green approaches stood");
    }
}

#[test]
fn every_approach_of_a_signalled_crossroads_flows() {
    for b in [Build::default(), Build { stub: true, ..Default::default() }, Build { main_priority: true, ..Default::default() }, Build { stub: true, main_priority: true }] {
        let through = cycle(b, 20.0, 300.0);
        for (d, n) in through.iter().enumerate() {
            // (two lanes, 20 s of green a 46 s cycle: a starved approach gets a handful)
            assert!(*n >= 20, "approach {d}: only {n} cars through in 300 s");
        }
    }
}

/// Spandau's `Einm_CharlCh_Polizei`: the main road stays green at 34 s until the side road
/// asks (`[traffic_light_stop] 1 34.0 1`), then the cycle runs on.
#[test]
fn a_held_program_runs_on_when_the_side_road_asks() {
    let mut c = TrafficLightController::from_program(
        vec![
            (vec![(3, 2.0), (6, 51.0), (9, 3.0), (0, 0.0)], None),
            (vec![(0, 63.0), (3, 2.0), (6, 11.0), (9, 3.0), (0, 0.0)], None),
        ],
        Some(82.0),
        &[[1.0, 34.0, 1.0]],
        &[],
    );
    c.start(18.04);
    for _ in 0..1000 {
        c.request.fill(false);
        c.advance(0.02);
    }
    assert!((c.time - (34.0 - 1e-5)).abs() < 1e-5, "held at {}", c.time);
    for _ in 0..1000 {
        c.request.fill(false);
        c.request[1] = true;
        c.advance(0.02);
    }
    assert!(c.time > 50.0, "still at {}", c.time);
}
