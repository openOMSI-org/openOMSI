//! Routing to a point on a directed road, including the partial first and last lanes.

use super::*;
use omsi_sim::traffic::way::{lane_time, lanes_near, way_to_any};
use std::collections::BinaryHeap;

pub(crate) struct DestinationPath {
    pub lanes: Vec<usize>,
    pub start_s: f32,
    pub end_s: f32,
    pub end: DVec3,
}

#[derive(PartialEq)]
struct Node(f32, usize);
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.0.total_cmp(&self.0)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) fn route_to_destination(
    net: &Network,
    bus: DVec3,
    heading: f64,
    at: DVec2,
) -> Option<DestinationPath> {
    if !at.is_finite() || !bus.is_finite() || !heading.is_finite() {
        return None;
    }
    let starts: Vec<_> = lanes_near(net, bus.truncate(), 90.0)
        .into_iter()
        .filter(|&i| net.lanes[i].kind == LaneKind::Street)
        .collect();
    // Use the navigator's existing heading-aware selection of the road under the bus.
    let (_, k) = way_to_any(net, bus, heading, &starts, 30_000.0)?;
    let start = starts[k];
    let (start_s, _) = net.lanes[start].nearest_point(bus)?;
    let mut goals: HashMap<usize, (f32, f64)> = lanes_near(net, at, 250.0)
        .into_iter()
        .filter_map(|i| {
            let l = &net.lanes[i];
            if l.kind != LaneKind::Street {
                return None;
            }
            let (s, _) = l.nearest_point(at.extend(l.start().z))?;
            let d = (l.at(s).0.truncate() - at).length();
            (d <= 250.0).then_some((i, (s, d)))
        })
        .collect();
    let nearest = goals.values().map(|g| g.1).min_by(f64::total_cmp)?;
    // Opposite carriageways may both serve a pin. Do not "reach" an isolated road by
    // silently picking an unrelated street hundreds of metres from the destination.
    goals.retain(|_, g| g.1 <= nearest + 8.0);
    // A cheaper arrival later on a lane cannot dominate an earlier arrival: only the
    // latter may reach a pin behind it. Keep the non-dominated (cost, entry) labels.
    struct Label {
        lane: usize,
        s: f32,
        cost: f32,
        parent: Option<usize>,
        active: bool,
    }
    let mut labels = vec![Label {
        lane: start,
        s: start_s,
        cost: 0.0,
        parent: None,
        active: true,
    }];
    let mut by_lane: HashMap<usize, Vec<usize>> = HashMap::new();
    by_lane.insert(start, vec![0]);
    let mut heap = BinaryHeap::new();
    heap.push(Node(0.0, 0));
    let mut best: Option<(f32, usize, f32)> = None;
    while let Some(Node(cost, state)) = heap.pop() {
        if !labels[state].active {
            continue;
        }
        if best.is_some_and(|b| cost > b.0) {
            break;
        }
        let i = labels[state].lane;
        let lane = &net.lanes[i];
        let from = labels[state].s;
        let time_per_m = lane_time(lane) / lane.length().max(0.5);
        if let Some(&(s, _)) = goals.get(&i) {
            if s + 0.1 >= from {
                let total = cost + (s - from).max(0.0) * time_per_m;
                if best.is_none_or(|b| total < b.0) {
                    best = Some((total, state, s));
                }
            }
        }
        let mut relax = |j: usize, cost: f32, s: f32| {
            if j >= net.lanes.len() || net.lanes[j].kind != LaneKind::Street {
                return;
            }
            let known = by_lane.entry(j).or_default();
            if known
                .iter()
                .any(|&k| labels[k].active && labels[k].cost <= cost && labels[k].s <= s + 0.05)
            {
                return;
            }
            for &k in known.iter() {
                if cost <= labels[k].cost && s <= labels[k].s + 0.05 {
                    labels[k].active = false;
                }
            }
            let next = labels.len();
            labels.push(Label {
                lane: j,
                s,
                cost,
                parent: Some(state),
                active: true,
            });
            known.push(next);
            heap.push(Node(cost, next));
        };
        let through = (lane.length() - from).max(0.0) * time_per_m;
        for &j in &lane.next {
            relax(j, cost + through, 0.0);
        }
        for j in [lane.left, lane.right].into_iter().flatten() {
            let Some(other) = net.lanes.get(j) else {
                continue;
            };
            let p = lane.at(from).0;
            let Some((s, _)) = other.nearest_point(p) else {
                continue;
            };
            if angle_diff(lane.at(from).1 as f64, other.at(s).1 as f64).abs() < 30.0 {
                relax(j, cost + 4.0, s);
            }
        }
    }
    let (_, mut state, end_s) = best?;
    let end_lane = labels[state].lane;
    let mut lanes = Vec::new();
    loop {
        lanes.push(labels[state].lane);
        match labels[state].parent {
            Some(p) => state = p,
            None => break,
        }
    }
    lanes.reverse();
    let keep: Vec<_> = lanes
        .iter()
        .enumerate()
        .filter_map(|(k, &i)| {
            let beside = lanes
                .get(k + 1)
                .is_some_and(|j| net.lanes[i].left == Some(*j) || net.lanes[i].right == Some(*j));
            (!beside).then_some(i)
        })
        .collect();
    Some(DestinationPath {
        start_s: keep
            .first()
            .and_then(|&i| net.lanes[i].nearest_point(bus))
            .map(|p| p.0)
            .unwrap_or(start_s),
        lanes: keep,
        end_s,
        end: net.lanes[end_lane].at(end_s).0,
    })
}
