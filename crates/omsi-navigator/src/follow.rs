//! Where the bus is along the route and the traffic around it: following the route (and
//! the way back when it was left), the cars drawn, the congestion tint of the roads and the
//! turn ahead. Moved out of `lib.rs` as they were.

use super::*;

impl Navigator {
    /// Where the bus is along the route, and a way back when it left it.
    pub(super) fn follow(&mut self, f: &NavFrame) {
        let global = self.global.clone();
        let Some(net) = global.as_deref().or(f.traffic.map(|t| &t.net)) else { return };
        let r = &mut self.route;
        if r.lanes.is_empty() {
            // no route (yet): the way to the next stop, looked for now and then
            let Some(stop) = f.stops.first() else { return };
            r.retry_in -= f.dt;
            if r.retry_in > 0.0 {
                return;
            }
            r.retry_in = REROUTE_EVERY * 2.0;
            // (the lanes that pass the stop on its side of the road: a way that came to the
            // stop on the far side turned round in front of it)
            let targets: Vec<usize> = lanes_near(net, stop.position.truncate(), 40.0)
                .into_iter()
                .filter(|&i| net.lanes[i].kind == LaneKind::Street && passes_on_its_side(net, i, stop.position, 20.0))
                .collect();
            if let Some((mut path, k)) = way_to_any(net, f.bus, f.heading, &targets, 30_000.0) {
                path.push(targets[k]);
                log::info!("navigator: no route of the trip here yet; {} lanes to the next stop '{}'", path.len(), stop.name.trim());
                r.lanes = path;
                r.progress = 0;
                r.s = 0.0;
                r.version += 1;
                r.provisional = true;
            }
            return;
        }
        r.note = (r.note - f.dt).max(0.0);
        // where the next stop is on the route: the way to the route must not skip it
        let stop_at = f.stops.first().and_then(|st| stop_on_route(net, &r.lanes[..r.lanes.len().min(r.progress + 1500)], r.progress, st.position, |_, _| true)).map(|(k, _)| k);
        // the nearest route lane near the bus that runs its way, a little back to a lot ahead
        // (before the bus first reached the route: only the stretch up to the next stop)
        let (from, to) = match (r.joined, stop_at) {
            (false, Some(k)) if r.approach => (r.progress.saturating_sub(3), (k + 2).min(r.lanes.len())),
            (false, Some(k)) => (k.saturating_sub(40).max(r.progress), (k + 2).min(r.lanes.len())),
            (true, Some(k)) => (r.progress.saturating_sub(3), (r.progress + 60).min(k.max(r.progress) + 2).min(r.lanes.len())),
            _ => (r.progress.saturating_sub(3), (r.progress + 60).min(r.lanes.len())),
        };
        let mut best: Option<(usize, f32, f64)> = None;
        for k in from..to {
            let Some(l) = net.lanes.get(r.lanes[k]) else { continue };
            let Some((s, d)) = l.nearest_point(f.bus) else { continue };
            // (a wide road, a stop bay, a lane change or a turn in progress: the bus is still
            // on its route well beyond the lane's middle; at 11 m and 75 deg it counted as off
            // and the navigator recalculated for ever)
            if d > 16.0 {
                continue;
            }
            let (_, h) = l.at(s);
            if angle_diff(f.heading, h as f64).abs() > 100.0 {
                continue;
            }
            // (ahead of where the bus was preferred: a route crossing itself)
            let score = d + if k < r.progress { 4.0 } else { 0.0 } + (k.saturating_sub(r.progress) as f64) * 0.05;
            if best.map(|b| score < b.2).unwrap_or(true) {
                best = Some((k, s, score));
            }
        }
        match best {
            Some((k, s, _)) => {
                if k != r.progress {
                    r.version += 1;
                }
                r.progress = k;
                r.s = s;
                r.on_route = true;
                r.joined = true;
                r.off_for = 0.0;
            }
            None => {
                r.on_route = false;
                r.off_for += f.dt;
            }
        }
        // the way to the route: at once before the bus first reached it (from the depot or
        // wherever it starts), after a moment off it once driving it
        if r.on_route || (r.joined && r.off_for < OFF_ROUTE_AFTER) {
            return;
        }
        r.retry_in -= f.dt;
        if r.retry_in > 0.0 {
            return;
        }
        r.retry_in = REROUTE_EVERY;
        let base = r.progress.min(r.lanes.len() - 1);
        // targets: the route from where the bus left it up to the next stop (joining it
        // later would skip that stop), or on for 120 lanes when no stop is ahead
        let (lo, hi) = match stop_at {
            Some(k) => (k.saturating_sub(150).max(base), k + 1),
            None => (base, (base + 120).min(r.lanes.len())),
        };
        // (from the depot to the first stop may be a long way; back onto the route is not)
        let way = way_back(net, f.bus, f.heading, &r.lanes[lo..hi], if r.joined { 6000.0 } else { 30_000.0 });
        if way.is_none() && omsi_cfg::flags::OMSI_DEBUG_NAV.is_set() {
            let info: Vec<_> = r.lanes[lo..hi].iter().map(|&l| (l, net.lanes[l].kind, net.lanes[l].name.clone(), net.lanes.iter().filter(|x| x.next.contains(&l)).count(), net.lanes[l].start())).collect();
            log::info!("navigator: off the route for {:.1} s and no way back found; targets {info:?}", r.off_for);
        }
        // the route's own lanes before a stop may be cut off from the road network (the
        // map links nothing into the start of line 31's trip at Maulbeerallee): then a
        // road past the stop running the route's way, else the route just after the stop
        let max = if r.joined { 6000.0 } else { 30_000.0 };
        let way = way.map(|(p, j)| (p, lo + j)).or_else(|| {
            let k = stop_at?;
            let st = f.stops.first()?;
            let (_, h) = net.lanes[r.lanes[k]].nearest_point(st.position).map(|(s, _)| net.lanes[r.lanes[k]].at(s))?;
            let near: Vec<usize> = lanes_near(net, st.position.truncate(), 40.0)
                .into_iter()
                .filter(|&i| {
                    let l = &net.lanes[i];
                    l.kind == LaneKind::Street
                        && l.nearest_point(st.position).map(|(s, d)| d < 20.0 && angle_diff(l.at(s).1 as f64, h as f64).abs() < 60.0).unwrap_or(false)
                })
                .collect();
            let (mut path, j) = way_to_any(net, f.bus, f.heading, &near, max)?;
            path.push(near[j]);
            log::info!("navigator: the route before stop '{}' cannot be reached; led onto the road past it", st.name.trim());
            Some((path, k + 1))
        })
            .or_else(|| {
                let k = stop_at?;
                let hi = (k + 120).min(r.lanes.len());
                (k + 1 < hi).then_some(())?;
                way_back(net, f.bus, f.heading, &r.lanes[k + 1..hi], max).map(|(p, j)| (p, k + 1 + j))
            });
        if let Some((path, join)) = way {
            let rest = r.lanes[join.min(r.lanes.len())..].to_vec();
            log::info!(
                "navigator: {} of {} lanes joins the route {} lanes on{}",
                if r.joined { "a way back" } else { "the way to the route" },
                path.len(),
                join,
                stop_at.map(|k| format!(" (the next stop is on route lane {k})")).unwrap_or_default()
            );
            let mut lanes = path;
            lanes.extend(rest);
            // (drawn from where the bus is on its first lane, not from that lane's start)
            r.s = lanes.first().and_then(|&l| net.lanes.get(l)).and_then(|l| l.nearest_point(f.bus)).map(|p| p.0).unwrap_or(0.0);
            r.lanes = lanes;
            r.progress = 0;
            r.version += 1;
            if r.joined {
                r.note = 4.0;
                r.on_route = true;
                r.off_for = 0.0;
            } else {
                r.approach = true;
            }
        }
    }

    /// Where each vehicle of the traffic is drawn: eased towards where it is, so that one the
    /// traffic steps seldom (far from the bus) glides instead of jumping; a long way (a car put
    /// back on the road elsewhere) is taken at once.
    pub(super) fn update_drawn_cars(&mut self, f: &NavFrame) {
        let Some(t) = f.traffic else {
            self.drawn_cars.clear();
            return;
        };
        let k = ease(f.dt, 0.12);
        let stamp = self.time;
        for c in t.cars.iter().filter(|c| !c.gone) {
            let now = c.vehicle.position;
            let e = self.drawn_cars.entry(c.id).or_insert((now, stamp));
            if (e.0 - now).truncate().length() > 40.0 {
                e.0 = now;
            } else {
                e.0 += (now - e.0) * k;
            }
            e.1 = stamp;
        }
        self.drawn_cars.retain(|_, v| v.1 == stamp);
    }

    /// Where a vehicle of the traffic is drawn (see `update_drawn_cars`).
    pub(super) fn smoothed_position(&self, c: &omsi_sim::ai_traffic::model::AiCar) -> DVec3 {
        self.drawn_cars.get(&c.id).map(|v| v.0).unwrap_or(c.vehicle.position)
    }

    /// How congested the lanes near the bus are, from how fast the traffic on them flows
    /// against how fast it could (0 free … 1 standing), smoothed over seconds:
    ///
    /// * a car waiting at a red light, a bus at its stop, a car parked or pulling out is no
    ///   evidence of a jam - only cars held for longer than a light's cycle count as standing;
    /// * one car is no evidence either way: a lane needs two moving or standing in it;
    /// * a full lane that moves is free (the old count of cars made busy streets amber and
    ///   two cars at a light red).
    ///
    /// The route's own lanes take the levels of the traffic's lanes they are (by their key,
    /// else by place and heading: junction paths have no key, and were never coloured).
    pub(super) fn update_congestion(&mut self, f: &NavFrame) {
        self.congestion_t -= f.dt;
        if self.congestion_t > 0.0 {
            return;
        }
        let step = 0.25f32;
        self.congestion_t = step;
        let Some(t) = f.traffic else { return };
        // per lane: the sum of the cars' speed against the lane's free speed, how many, and
        // how many stand for long
        let mut by_lane: HashMap<usize, (f32, u32, u32)> = HashMap::new();
        for c in &t.cars {
            if c.gone || (c.vehicle.position - f.bus).truncate().length() > 1500.0 {
                continue;
            }
            // (a bus at its stop, or a car standing as part of the furniture)
            if c.bus.as_ref().is_some_and(|b| !matches!(b.phase, omsi_sim::ai_traffic::bus_service::Phase::Running)) {
                continue;
            }
            let Some(l) = t.net.lanes.get(c.state.lane) else { continue };
            let free = (l.speed_limit_kmh.clamp(20.0, 60.0) / 3.6) * 0.8;
            let v = c.state.speed.max(0.0);
            // (waiting at a light up to a cycle is how traffic flows there)
            let ratio = if v < 0.8 && c.stopped < 35.0 { 0.75 } else { (v / free).min(1.0) };
            let e = by_lane.entry(c.state.lane).or_insert((0.0, 0, 0));
            e.0 += ratio;
            e.1 += 1;
            if c.stopped > 45.0 {
                e.2 += 1;
            }
        }
        let k = ease(step, 6.0) as f32;
        // (no cars: back to free, slowly)
        for v in self.congestion.values_mut() {
            *v -= *v * k * 0.5;
        }
        for (lane, (sum, n, standing)) in by_lane {
            if n < 2 && standing == 0 {
                continue;
            }
            let flow = sum / n as f32;
            let mut score = (1.0 - flow).clamp(0.0, 1.0);
            if standing >= 2 {
                score = score.max(0.9);
            }
            let v = self.congestion.entry(lane).or_insert(0.0);
            *v += (score - *v) * k;
        }
        self.congestion.retain(|_, v| *v > 0.03);
        // the same for the route ahead, in the route's own network
        let global = self.global.clone();
        if t.lanes_generation != self.lane_map_gen.0 || self.global_version != self.lane_map_gen.1 {
            self.lane_map.clear();
            self.lane_map_gen = (t.lanes_generation, self.global_version);
        }
        let jam = match global.as_deref() {
            Some(g) => {
                let mut out = HashMap::new();
                for (&l, &v) in &self.congestion {
                    let targets = self.lane_map.entry(l).or_insert_with(|| same_paths(g, &t.net, l));
                    for &gi in targets.iter() {
                        out.insert(gi, v);
                    }
                }
                out
            }
            None => self.congestion.clone(),
        };
        let net = global.as_deref().unwrap_or(&t.net);
        let r = &self.route;
        let mut route_jam = HashMap::new();
        let mut cost = 0.0;
        for &l in r.lanes.iter().skip(r.progress).take(600) {
            let Some(&c) = jam.get(&l) else { continue };
            route_jam.insert(l, c);
            // the time the slow stretches ahead cost
            if let Some(lane) = net.lanes.get(l) {
                if level(c) >= 2 {
                    let v_free = lane.speed_limit_kmh.clamp(20.0, 60.0) / 3.6;
                    let v = (v_free * (1.0 - c)).max(1.0);
                    cost += lane.length() / v - lane.length() / v_free;
                }
            }
        }
        self.jam_cost = cost;
        let changed = route_jam.len() != self.route_jam.len() || route_jam.iter().any(|(l, c)| self.route_jam.get(l).map(|o| level(*o) != level(*c)).unwrap_or(true));
        self.route_jam = route_jam;
        if changed {
            self.jam_version += 1;
        }
    }

    /// The first real turn on the route within 1.5 km: a lane whose heading changes by more
    /// than 35 degrees from its start to its end (a junction's curve), or a sharp bend
    /// between two lanes. Gentle curves of a road are not turns.
    pub(super) fn turn_ahead(&self, net: &Network) -> Option<(i32, f32, f64, Option<String>)> {
        let r = &self.route;
        if !r.on_route {
            return None;
        }
        let mut acc = -(r.s as f64);
        let mut prev_end: Option<f32> = None;
        for (j, &l) in r.lanes.iter().enumerate().skip(r.progress) {
            let lane = net.lanes.get(l)?;
            let len = if j + 1 == r.lanes.len() { self.destination.as_ref().and_then(|d| d.path.as_ref().map(|p| p.end_s)).unwrap_or(lane.length()) } else { lane.length() };
            if acc > 1500.0 {
                break;
            }
            let (h0, h1) = (lane.start_heading(), lane.at(len).1);
            let mut d = omsi_sim::traffic::wrap_deg(h1 - h0);
            if let Some(pe) = prev_end {
                d += omsi_sim::traffic::wrap_deg(h0 - pe);
            }
            // (a short junction lane turns within a few metres; a long road curve does not count)
            if d.abs() > 35.0 && (len < 60.0 || d.abs() > 70.0) && acc + len as f64 > 0.0 {
                let dir = if d.abs() > 150.0 { 2 } else if d > 0.0 { 1 } else { -1 };
                // the street it turns into: the first named lane after the turn
                let street = r.lanes.iter().skip(j + 1).take(4).chain(std::iter::once(&l)).find_map(|&x| self.street_of(x)).map(str::to_string);
                return Some((dir, d.abs(), acc.max(0.0), street));
            }
            prev_end = Some(h1);
            acc += len as f64;
        }
        None
    }

    /// The street a lane of the route's network belongs to (the map's network only).
    pub(super) fn street_of(&self, lane: usize) -> Option<&str> {
        self.global.as_ref()?;
        let st = self.streets.as_deref()?;
        let i = *st.of_lane.get(lane)?;
        st.names.get(i as usize).map(String::as_str)
    }

    /// Distance along the route to `stop` (m).
    pub(super) fn route_distance(&self, net: &Network, stop: DVec3) -> Option<f64> {
        if self.destination.is_some() { return None; }
        let r = &self.route;
        if r.progress >= r.lanes.len() {
            return None;
        }
        let mut acc = vec![-(r.s as f64)];
        for &l in r.lanes.iter().skip(r.progress) {
            let next = acc[acc.len() - 1] + net.lanes.get(l)?.length() as f64;
            if next > 30_000.0 {
                break;
            }
            acc.push(next);
        }
        let ahead = |k: usize| acc[k - r.progress];
        let (k, s) = stop_on_route(net, &r.lanes[..r.progress + acc.len().min(r.lanes.len() - r.progress)], r.progress, stop, |k, s| ahead(k) + s as f64 >= -5.0)?;
        Some((ahead(k) + s as f64).max(0.0))
    }
}
