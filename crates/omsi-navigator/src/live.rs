//! Every bus, tram and train of the timetable on the maps as it runs: those on the road near
//! the bus are the traffic's own vehicles; the others - anywhere on the map, which the traffic
//! does not drive - are put where the timetable has them now, along the trip's own route
//! between the stop they left and the next one (standing at a stop while it is there), and
//! glide from one place to the next.

use super::*;
use omsi_sim::timetable_run::ScheduleSim;

/// A vehicle of the timetable the traffic does not drive, as the maps show it.
#[derive(Clone, Debug)]
pub struct LiveVehicle {
    pub pos: DVec3,
    pub heading: f64,
    pub line: String,
    pub rail: bool,
}

/// A trip's route as one line: its points, the distance along it to each, and to each stop.
pub(super) struct TripPath {
    pts: Vec<DVec3>,
    along: Vec<f64>,
    stops: Vec<f64>,
    rail: bool,
}

impl TripPath {
    fn at(&self, s: f64) -> (DVec3, f64) {
        let s = s.clamp(0.0, *self.along.last().unwrap_or(&0.0));
        let k = self.along.partition_point(|&a| a < s).clamp(1, self.pts.len() - 1);
        let (a, b) = (self.pts[k - 1], self.pts[k]);
        let span = (self.along[k] - self.along[k - 1]).max(1e-6);
        let p = a.lerp(b, ((s - self.along[k - 1]) / span).clamp(0.0, 1.0));
        let d = (b - a).truncate();
        (p, d.x.atan2(d.y).to_degrees().rem_euclid(360.0))
    }
}

/// The route of a trip with its stops placed on it (`stops`: their objects' places, in order).
fn trip_path(net: &Network, lanes: &[usize], stops: &[Option<DVec3>]) -> Option<TripPath> {
    let mut pts: Vec<DVec3> = Vec::new();
    for &l in lanes {
        let lane = net.lanes.get(l)?;
        // (a hole in the route - a piece the map does not have - would be crossed in a straight
        // line over the fields: such a trip is not shown at all)
        if let (Some(q), Some(first)) = (pts.last(), lane.points.first()) {
            if (*q - *first).truncate().length() > 25.0 {
                return None;
            }
        }
        for p in &lane.points {
            if pts.last().map(|q: &DVec3| (*q - *p).truncate().length() > 0.3).unwrap_or(true) {
                pts.push(*p);
            }
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let mut along = vec![0.0];
    for w in pts.windows(2) {
        along.push(along.last().unwrap() + (w[1] - w[0]).truncate().length());
    }
    // each stop where the route passes nearest it, never before the stop before it
    let mut out = Vec::with_capacity(stops.len());
    let mut from = 0usize;
    for st in stops {
        let Some(p) = st else {
            out.push(out.last().copied().unwrap_or(0.0));
            continue;
        };
        let mut best = (f64::MAX, from, 0.0);
        for k in from..pts.len() - 1 {
            let (a, b) = (pts[k].truncate(), pts[k + 1].truncate());
            let ab = b - a;
            let t = ((p.truncate() - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            let d = (a + ab * t - p.truncate()).length();
            if d < best.0 {
                best = (d, k, t);
            }
            // (well past it: the route has gone on)
            if best.0 < 30.0 && along[k] > along[best.1] + 600.0 {
                break;
            }
        }
        // (a stop the route does not pass: the route is not this trip's, or the map lacks it -
        // the vehicle would run where it never goes, as the airport bus out over the fields)
        if best.0 > 60.0 {
            return None;
        }
        from = best.1;
        out.push(along[best.1] + (along[best.1 + 1] - along[best.1]) * best.2);
    }
    // (and a route that knows none of its stops' places says nothing either)
    if stops.iter().all(|s| s.is_none()) {
        return None;
    }
    // (an aircraft's approach is no vehicle of the streets)
    if lanes.iter().any(|&l| net.lanes.get(l).is_some_and(|l| l.kind == LaneKind::Air)) {
        return None;
    }
    let rail = lanes.first().and_then(|&l| net.lanes.get(l)).is_some_and(|l| l.kind == LaneKind::Rail);
    Some(TripPath { pts, along, stops: out, rail })
}

impl Navigator {
    /// Where the timetable's vehicles are now (every half second; a few new trips' routes are
    /// worked out each time). `cars`: where the traffic's own buses are, with their lines.
    pub fn live_due(&self, dt: f32) -> bool {
        self.live_t - dt <= 0.0
    }

    pub fn update_live(&mut self, sch: &ScheduleSim, now: f64, dt: f32, cars: &[(DVec3, String)], driven: &dyn Fn(DVec3) -> bool) {
        // (eased towards the last places in between)
        let k = ease(dt, 0.6);
        for v in self.live_drawn.iter_mut() {
            if let Some(t) = self.live.iter().find(|t| t.line == v.line && (t.pos - v.pos).truncate().length() < 150.0) {
                v.pos += (t.pos - v.pos) * k;
                v.heading += angle_diff(v.heading, t.heading) * k;
            }
        }
        self.live_t -= dt;
        if self.live_t > 0.0 {
            return;
        }
        self.live_t = 0.5;
        let Some(net) = self.global.clone() else { return };
        // the departures on the road, looked for every few seconds (a day has thousands)
        self.live_list_t -= 0.5;
        if self.live_list_t <= 0.0 {
            self.live_list_t = 5.0;
            self.live_list = sch.upcoming(now, 30.0);
            let keep: hashbrown::HashSet<usize> = self.live_list.iter().copied().collect();
            self.live_tt.retain(|k, _| keep.contains(k));
        }
        let mut out = Vec::new();
        let mut made = 0;
        for i in self.live_list.clone() {
            let ti = sch.trip_index(i);
            let tt = self.live_tt.entry(i).or_insert_with(|| std::sync::Arc::new(sch.ai_timetable(i))).clone();
            let (Some(first), Some(last)) = (tt.stops.first(), tt.stops.last()) else { continue };
            // (on the road from a minute before its first departure until it arrives)
            if now < first.3 as f64 - 60.0 || now > last.2 as f64 + 30.0 {
                continue;
            }
            if !self.trip_paths.contains_key(&ti) {
                if made >= 3 {
                    continue;
                }
                made += 1;
                let lanes = sch.trip_route_in(&net, &sch.trip_of(i).name);
                let stops: Vec<Option<DVec3>> = tt.stops.iter().map(|s| self.stop_pos.get(&s.0).copied()).collect();
                self.trip_paths.insert(ti, trip_path(&net, &lanes, &stops).map(std::sync::Arc::new));
            }
            let Some(Some(path)) = self.trip_paths.get(&ti).cloned() else { continue };
            // the stretch it is on: standing at stop j until it leaves, then on to the next
            let mut s = path.stops.first().copied().unwrap_or(0.0);
            for (j, st) in tt.stops.iter().enumerate() {
                let (arr, dep) = (st.2 as f64, st.3 as f64);
                if now < arr {
                    // on the way from the stop before
                    if j > 0 {
                        let prev_dep = tt.stops[j - 1].3 as f64;
                        let t = ((now - prev_dep) / (arr - prev_dep).max(1.0)).clamp(0.0, 1.0);
                        // (pulling away and slowing down at either end)
                        let t = t * t * (3.0 - 2.0 * t);
                        let (a, b) = (path.stops[j - 1], path.stops[j]);
                        s = a + (b - a) * t;
                    }
                    break;
                }
                s = path.stops[j];
                if now < dep {
                    break;
                }
            }
            let (pos, heading) = path.at(s);
            // (the traffic drives this one itself)
            if driven(pos) || cars.iter().any(|(p, l)| (*p - pos).truncate().length() < if l.trim() == tt.line.trim() { 250.0 } else { 25.0 }) {
                continue;
            }
            out.push(LiveVehicle { pos, heading, line: tt.line.clone(), rail: path.rail });
        }
        // new ones appear where they are; the ones drawn take their new places gently
        let mut drawn = Vec::with_capacity(out.len());
        for v in &out {
            match self.live_drawn.iter().find(|d| d.line == v.line && (d.pos - v.pos).truncate().length() < 150.0) {
                Some(d) => drawn.push(d.clone()),
                None => drawn.push(v.clone()),
            }
        }
        self.live = out;
        self.live_drawn = drawn;
    }

    /// The timetable's vehicles the traffic does not drive, where they are drawn.
    pub fn live_vehicles(&self) -> &[LiveVehicle] {
        &self.live_drawn
    }
}
