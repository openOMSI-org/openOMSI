//! The quickest way through the network from where a vehicle is: back onto a route still to
//! drive, or onto any of a set of lanes (the navigator's way back, a duty's way to its first
//! stop). Times, not lengths: each lane at about its speed, turns and turning round cost.

use super::{LaneKind, Network};
use crate::crowd::angle_diff;
use glam::{DVec2, DVec3};
use hashbrown::HashMap;

/// Seconds to drive a lane at about the speed its limit allows in a town (a bus never reaches
/// more than 60 or so, and does not crawl either).
pub fn lane_time(l: &super::Lane) -> f32 {
    let v = (l.speed_limit_kmh.clamp(15.0, 60.0) / 3.6) * 0.85;
    l.length().max(0.5) / v
}

/// What a turn from lane `a` into lane `b` costs (s): straight on nothing, a turn a few
/// seconds (a left turn across the oncoming traffic a little more), turning round a lot.
fn turn_cost(net: &Network, a: &super::Lane, b: &super::Lane) -> f32 {
    let d = super::wrap_deg(b.start_heading() - a.end_heading()) + super::wrap_deg(b.end_heading() - b.start_heading());
    if d.abs() > 150.0 {
        60.0
    } else if d.abs() > 35.0 {
        // (across the oncoming traffic: left on the right-hand side of the road)
        if (d < 0.0) != net.left_hand { 9.0 } else { 6.0 }
    } else {
        0.0
    }
}

/// A way from where the bus is onto `ahead` (the route still to drive): the lanes to drive up
/// to the route, and the index in `ahead` of the route lane it reaches (the route goes on from
/// there). It is the quickest way to the end of `ahead`, not to the nearest bit of the route:
/// every route lane reached is weighed with the route still to drive from it, so the way back
/// never leads to a place behind on the route only to drive it all again, and never skips
/// ahead past part of it either. Times, not lengths: each lane at its speed, turns and turning
/// round cost, a change to the next lane of the same road (`left`/`right`) is allowed.
/// `max_cost` is in metres (as far as the search goes, at about 8 m/s).
pub fn way_back(net: &Network, bus: DVec3, heading: f64, ahead: &[usize], max_cost: f32) -> Option<(Vec<usize>, usize)> {
    way_search(net, bus, heading, ahead, max_cost, true)
}

/// The quickest way onto any of `targets` (lanes in no order: the lanes at a stop).
pub fn way_to_any(net: &Network, bus: DVec3, heading: f64, targets: &[usize], max_cost: f32) -> Option<(Vec<usize>, usize)> {
    way_search(net, bus, heading, targets, max_cost, false)
}

fn way_search(net: &Network, bus: DVec3, heading: f64, ahead: &[usize], max_cost: f32, along: bool) -> Option<(Vec<usize>, usize)> {
    use std::cmp::Ordering;
    use std::collections::BinaryHeap;
    // the lane under the bus going its way; else (a depot, a car park, the grass) the
    // nearest street lane that does not point back at the bus, else any near
    let cands: Vec<(usize, f64, f64, f32)> = lanes_near(net, bus.truncate(), 90.0)
        .into_iter()
        .filter_map(|i| {
            let l = net.lanes.get(i)?;
            if l.kind != LaneKind::Street {
                return None;
            }
            let (s, d) = l.nearest_point(bus)?;
            let (_, h) = l.at(s);
            Some((i, d, angle_diff(heading, h as f64).abs(), s))
        })
        .collect();
    let pick = |max_d: f64, max_a: f64| cands.iter().filter(|c| c.1 < max_d && c.2 < max_a).min_by(|a, b| (a.1 + a.2 * 0.1).total_cmp(&(b.1 + b.2 * 0.1))).copied();
    let start = pick(14.0, 70.0).or_else(|| pick(80.0, 100.0)).or_else(|| pick(80.0, 181.0));
    if omsi_cfg::flags::OMSI_DEBUG_NAV.is_set() {
        log::info!("navigator: way search from {:?} ({} street lanes within 90 m) to {} route lanes", start.map(|s| s.0), cands.len(), ahead.len());
    }
    let (start, _, _, s0) = start?;
    let window = &ahead[..ahead.len().min(400)];
    // the route still to drive from each lane of it (s)
    let mut to_go = vec![0.0f32; window.len() + 1];
    for k in (0..window.len()).rev().filter(|_| along) {
        to_go[k] = to_go[k + 1] + net.lanes.get(window[k]).map(lane_time).unwrap_or(0.0);
    }
    let mut targets: HashMap<usize, usize> = HashMap::new();
    for (k, &l) in window.iter().enumerate() {
        targets.entry(l).or_insert(k);
    }
    #[derive(PartialEq)]
    struct Node(f32, usize);
    impl Eq for Node {}
    impl Ord for Node {
        fn cmp(&self, o: &Self) -> Ordering {
            o.0.total_cmp(&self.0)
        }
    }
    impl PartialOrd for Node {
        fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
            Some(self.cmp(o))
        }
    }
    // cost to reach the start of a lane (s); a lane changed into is entered half way
    let mut dist: HashMap<usize, f32> = HashMap::new();
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut lateral: hashbrown::HashSet<usize> = hashbrown::HashSet::new();
    let mut heap = BinaryHeap::new();
    let limit = max_cost / 8.0;
    dist.insert(start, 0.0);
    heap.push(Node(0.0, start));
    let mut best: Option<(f32, usize, usize)> = None;
    while let Some(Node(cost, lane)) = heap.pop() {
        if cost > dist.get(&lane).copied().unwrap_or(f32::INFINITY) {
            continue;
        }
        // (nothing reached later can beat what was found: everything left costs more)
        if best.is_some_and(|b| cost >= b.0) || cost > limit {
            break;
        }
        let Some(l) = net.lanes.get(lane) else { continue };
        if let Some(&k) = targets.get(&lane) {
            // (the start lane is on the route only ahead of the bus: its remaining part)
            let total = cost + to_go[k] - if lane == start && along { s0 / l.length().max(0.5) * lane_time(l) } else { 0.0 };
            if best.map(|b| total < b.0).unwrap_or(true) {
                best = Some((total, lane, k));
            }
        }
        let through = if lane == start {
            (l.length() - s0).max(0.0) / l.length().max(0.5) * lane_time(l)
        } else if lateral.contains(&lane) {
            lane_time(l) * 0.5
        } else {
            lane_time(l)
        };
        for &n in &l.next {
            let Some(nl) = net.lanes.get(n) else { continue };
            if nl.kind != LaneKind::Street {
                continue;
            }
            let c = cost + through + turn_cost(net, l, nl);
            if c < dist.get(&n).copied().unwrap_or(f32::INFINITY) {
                dist.insert(n, c);
                parent.insert(n, lane);
                lateral.remove(&n);
                heap.push(Node(c, n));
            }
        }
        // the next lane of the same road, the same way: changed into on the way
        for n in [l.left, l.right].into_iter().flatten() {
            let Some(nl) = net.lanes.get(n) else { continue };
            if nl.kind != LaneKind::Street || angle_diff(nl.start_heading() as f64, l.start_heading() as f64).abs() > 30.0 {
                continue;
            }
            let c = cost + through * 0.5 + 4.0;
            if c < dist.get(&n).copied().unwrap_or(f32::INFINITY) {
                dist.insert(n, c);
                parent.insert(n, lane);
                lateral.insert(n);
                heap.push(Node(c, n));
            }
        }
    }
    let (_, reached, k) = best?;
    let mut path = vec![reached];
    let mut c = reached;
    let mut seen = hashbrown::HashSet::new();
    seen.insert(c);
    while let Some(&p) = parent.get(&c) {
        if !seen.insert(p) {
            return None;
        }
        path.push(p);
        c = p;
    }
    path.reverse();
    // (the route lane reached is where the route goes on)
    path.pop();
    // a lane left for the one beside it is not driven: the one beside it is
    let mut out: Vec<usize> = Vec::with_capacity(path.len());
    for (i, &l) in path.iter().enumerate() {
        let next = path.get(i + 1).copied().unwrap_or(reached);
        let beside = net.lanes.get(l).map(|x| x.left == Some(next) || x.right == Some(next)).unwrap_or(false);
        if !beside {
            out.push(l);
        }
    }
    Some((out, k))
}

/// Lanes with a point within `radius` of `c` (by the network's 50 m grid).
pub fn lanes_near(net: &Network, c: DVec2, radius: f64) -> Vec<usize> {
    let mut seen = hashbrown::HashSet::new();
    let (cx, cy) = Network::grid_cell(c.extend(0.0));
    let r = (radius / 50.0).ceil() as i32;
    if net.grid.is_empty() {
        return (0..net.lanes.len()).filter(|&i| net.lanes[i].points.iter().any(|q| (q.truncate() - c).length() < radius)).collect();
    }
    for gx in cx - r..=cx + r {
        for gy in cy - r..=cy + r {
            if let Some(v) = net.grid.get(&(gx, gy)) {
                seen.extend(v.iter().copied());
            }
        }
    }
    let mut v: Vec<usize> = seen.into_iter().collect();
    v.sort_unstable();
    v
}
