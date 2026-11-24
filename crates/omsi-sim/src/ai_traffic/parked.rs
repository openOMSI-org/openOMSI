//! Parked cars: beside which lanes they stand (see omsi-app's `traffic::parked` for the
//! cars parking in a free space and parked cars pulling out).

use super::*;

impl TrafficSim {
    /// Put parked cars onto the lanes they stand in or beside: (distance along the lane,
    /// signed lateral offset, + = right). `cars` are the ones the tiles placed since the last
    /// call; the cars no lane was found for before are tried again where the lanes `added`
    /// just came in.
    pub fn sort_parked(&mut self, cars: Vec<(DVec3, f64)>, added: std::ops::Range<usize>) {
        let mut todo: Vec<DVec3> = Vec::new();
        if !added.is_empty() && !self.parked_waiting.is_empty() {
            let cells: hashbrown::HashSet<(i32, i32)> = added
                .clone()
                .flat_map(|i| Network::lane_cells(&self.net.lanes[i]))
                .collect();
            let near = |p: &DVec3| {
                let (cx, cy) = Network::grid_cell(*p);
                (-1..=1).any(|dx| (-1..=1).any(|dy| cells.contains(&(cx + dx, cy + dy))))
            };
            let (retry, keep): (Vec<DVec3>, Vec<DVec3>) = std::mem::take(&mut self.parked_waiting)
                .into_iter()
                .partition(|p| near(p));
            self.parked_waiting = keep;
            todo = retry;
        }
        let new_cars = cars.len();
        todo.extend(cars.into_iter().map(|(p, _heading)| p));
        if todo.is_empty() {
            return;
        }
        let mut on_lanes = 0usize;
        for p in todo {
            // a car beside a lane is within a few metres of it: the lanes of the cells
            // around it are enough, and a car in a car park finds none
            let beside = self
                .net
                .nearest_lane_near(p, LaneKind::Street)
                .filter(|(l, _, d)| *d <= (self.net.lanes[*l].width * 0.5).max(1.5) as f64 + 1.6);
            let Some((l, s, _)) = beside else {
                self.parked_waiting.push(p);
                continue;
            };
            let (q, h) = self.net.lanes[l].at(s);
            let hr = (h as f64).to_radians();
            let right = DVec3::new(hr.cos(), -hr.sin(), 0.0);
            let lat = (p - q).dot(right) as f32;
            if lat.abs() < 0.9 && omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                let lane = &self.net.lanes[l];
                log::info!(
                    "parked car at ({:.1}, {:.1}) stands in lane {l} ({} {:?}, width {:.1}, heading {:.0} there, s {s:.1} of {:.1}, {lat:+.2} m to the side)",
                    p.x,
                    p.y,
                    lane.name,
                    lane.key,
                    lane.width,
                    h,
                    lane.length()
                );
            }
            self.parked.entry(l).or_default().push((s, lat));
            on_lanes += 1;
        }
        if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
            log::info!(
                "traffic: {new_cars} parked cars placed, {on_lanes} more stand in or beside a lane ({} in all, {} not beside one)",
                self.parked.values().map(|v| v.len()).sum::<usize>(),
                self.parked_waiting.len()
            );
        }
    }
}

/// A parked car uses the same approximate body as the driving obstacle check:
/// half length 2.3 m, half width 0.9 m, and 0.15 m lateral clearance. A car well
/// off the target lane must not prevent a lane change on a narrow street.
pub fn parked_lane_clear(
    parked: &[(f32, f32)],
    s: f32,
    back: f32,
    ahead: f32,
    half_width: f32,
) -> bool {
    !parked.iter().any(|&(at, lat)| {
        lat.abs() < half_width + 0.9 + 0.15 && at + 2.3 >= s - back && at - 2.3 <= s + ahead
    })
}
