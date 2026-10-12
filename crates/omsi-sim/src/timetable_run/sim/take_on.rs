//! A tour's bus taking its next trip on where it stands: the route from there onto the
//! trip, its stops, its line and destination. omsi-app's `schedule` turns a train round
//! first (that moves its renders); the rest is the timetable's.

use super::*;

impl ScheduleSim {
    /// The tour's bus `ci` (in `traffic`), at the end of its previous trip, takes departure `i`
    /// on, placed by `p` (`ScheduleSim::place_departure`).
    pub fn take_trip_on(&mut self, i: usize, ci: usize, p: Placing, traffic: &mut TrafficSim, day_time: f64) -> Placed {
        let Placing { trip_name, stations, departure, leave, steps, station_steps, station_tiles, slots, end, section, mut stops, served, .. } = p;
        let (ty, rail) = (traffic.cars[ci].vehicle.ty.clone(), traffic.cars[ci].is_rail());
        place_stops(&traffic.net, &section, 0, &mut stops, &ty, rail);
        // the tour's bus that has just finished its trip takes this one on from where
        // it stands: the section itself when it stands on it, else the shortest way
        // from its lane onto one of the section's first lanes (round a terminal loop)
        let (lane0, s0) = (traffic.cars[ci].state.lane, traffic.cars[ci].state.s);
        let net = &traffic.net;
        let (prefix, from) = match tour_entry(&section, lane0) {
            Some(r) => (Vec::new(), r),
            None => {
                let mut best: Option<(f32, Vec<usize>, usize)> = None;
                for t in 0..section.len().min(TOUR_ENTRY_LANES) {
                    if let Some(p) = net.shortest_path(lane0, section[t]) {
                        let len = p[..p.len() - 1].iter().map(|&l| net.lanes[l].length()).sum::<f32>() - s0;
                        if len < 400.0 && best.as_ref().map(|b| len < b.0).unwrap_or(true) {
                            best = Some((len, p, t));
                        }
                    }
                }
                match best {
                    Some((_, p, t)) => (p[..p.len() - 1].to_vec(), t),
                    None => {
                        log::debug!("trip {trip_name}: the tour's bus has no way from where it stands");
                        if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                            let ln = &net.lanes[lane0];
                            log::info!("trip {trip_name}: tour bus on lane {lane0} {:?} at s {s0:.1} of {:.1}, ({:.1}, {:.1}) -> ({:.1}, {:.1}), next {:?}", ln.key, ln.length(), ln.start().x, ln.start().y, ln.end().x, ln.end().y, ln.next);
                            for &l in section.iter().take(4) {
                                let ln = &net.lanes[l];
                                log::info!("  trip lane {l} {:?} len {:.1} ({:.1}, {:.1}) -> ({:.1}, {:.1}) prev? next {:?}", ln.key, ln.length(), ln.start().x, ln.start().y, ln.end().x, ln.end().y, ln.next);
                            }
                        }
                        return Placed::Drop;
                    }
                }
            }
        };
        let route: Vec<usize> = prefix.iter().copied().chain(section[from..].iter().copied()).collect();
        let shift = prefix.len() as isize - from as isize;
        // the stops from the bus on; one just behind it on its lane is where it stands
        let stops: Vec<(usize, f32, f32, f64, i64, f32)> = stops
            .into_iter()
            .filter(|st| st.0 >= from)
            .filter_map(|(ri, ss, lat, t, id, side)| {
                let nri = (ri as isize + shift) as usize;
                if nri == 0 && ss <= s0 + 0.3 {
                    (s0 - ss < 25.0).then_some((0, s0 + 0.3, 0.0, t, id, side))
                } else {
                    Some((nri, ss, lat, t, id, side))
                }
            })
            .collect();
        let layover = departure > day_time;
        let n_stops = stops.len();
        traffic.reroute(ci, route, s0, stops, layover);
        let line = self.display_line(i);
        let terminus = self.trip_of(i).terminus.clone();
        let names = self.trip_stop_names(self.trip_index(i));
        let last_stop = trip_stations(&self.trip_of(i)).last().copied();
        let (always, early, holds) = self.special_stops(i);
        let car = &mut traffic.cars[ci];
        if let Some(k) = car.vehicle.ty.program.str_var("Linie") {
            car.vehicle.state.str_vars[k as usize] = line.clone();
        }
        let hof = car.vehicle.host.hof.clone();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let timetable = self.ai_timetable(i);
        timetable.install(&mut car.vehicle.host, car.bus.as_ref().and_then(|b| b.stops.front()));
        car.vehicle.set_var("schedule_active", 1.0);
        if let Some(b) = car.bus.as_mut() {
            b.delay = 0.0;
        }
        set_ai_destination(&mut car.vehicle, hof.as_deref(), &line, &terminus, &names);
        if let Some(b) = car.bus.as_mut() {
            b.route_open = end < slots.len();
            b.terminus = terminus.clone();
            b.last_stop = last_stop;
            b.always = always;
            b.serve_early = early;
            b.holds = holds;
        }
        let id = car.id;
        self.bus_runs(id, i);
        self.forget_route(id);
        if end < slots.len() {
            self.carry_later(id, steps, end, &stations, &leave, served, station_steps, station_tiles);
        }
        log::info!(
            "scheduled bus {id}: line {line} tour {} goes on with trip {trip_name} to {} at {:.1} min (leaves {:.1} min), {n_stops} stops{}",
            self.departure_tour(i),
            terminus.trim(),
            day_time / 60.0,
            departure / 60.0,
            if prefix.is_empty() { String::new() } else { format!(", {} lanes to its first stop", prefix.len()) }
        );
        Placed::Spawned
    }
}
