//! The departures due each frame (which trip leaves when, which wait for their tiles or for
//! their tour's bus), tours handing their bus on, trips carried on as tiles load.
//! omsi-app's `schedule` puts the buses on the road and takes them off.

use super::*;

/// A tour's bus takes its next trip on only from one of the trip's first lanes (or a short
/// way onto one of them): Omsi.exe starts every trip at its beginning.
pub const TOUR_ENTRY_LANES: usize = 4;

/// Where on the next trip's route `section` the tour's bus standing on `lane` takes it on:
/// only on one of its first lanes. A tour repeating the same trip leaves its bus on the
/// route's last lane, and taken on there the trip was over at once - the bus stood at the
/// last stop for good, and every bus behind it queued (#976).
pub fn tour_entry(section: &[usize], lane: usize) -> Option<usize> {
    section.iter().take(TOUR_ENTRY_LANES).position(|&l| l == lane)
}

impl ScheduleSim {
    /// The clock was set to another time (by hand, the menu, Ctrl+Shift+Page Up/Down): every
    /// timetable bus goes, and at the next tick each trip under way at the new time is put
    /// out where its timetable has it then, as when the game starts - as Omsi.exe does when
    /// the time is changed. They stayed where they were, the whole timetable running hours
    /// early or late: buses queued at stops, waiting there for their time (#1607, #1455).
    /// omsi-app's `Schedule::restart` has taken the buses off (`removed` of them): the
    /// departures start afresh at `day_time`.
    pub fn restart_clock(&mut self, day_time: f64, removed: usize) {
        let n = removed;
        self.running.clear();
        self.pending.clear();
        self.waiting.clear();
        self.awaiting.clear();
        self.retry_at.clear();
        self.startup.clear();
        self.later_layover.clear();
        // Skip directly to the requested date, rather than rebuilding the timetable
        // once for every intervening day (hundreds of reassignments on year jumps).
        if day_time.is_finite() && day_time < self.day_base {
            let days_back = ((self.day_base - day_time) / DAY).ceil() as i64;
            self.shift_calendar_days(-days_back);
        }
        self.roll_day(day_time);
        for i in 0..self.departures.len() {
            if !self.is_player_tour(i) {
                self.departures[i].spawned = false;
            }
        }
        self.last_tod = day_time - self.day_base;
        self.restarted = true;
        log::info!("timetable: the clock was set to {}: {n} timetable buses taken off, the trips under way put out again", hhmm(day_time - self.day_base));
    }

    /// The timetable buses on the road (by car id).
    pub fn car_ids(&self) -> Vec<u64> {
        self.car_departure.keys().copied().collect()
    }

    /// Car `id` runs no departure any more.
    pub fn forget_car(&mut self, id: u64) {
        self.car_departure.remove(&id);
    }

    /// The timetable bus on the road that runs departure `k` (not one that has been let go).
    fn tour_bus(&self, k: usize, traffic: &TrafficSim) -> Option<usize> {
        traffic
            .cars
            .iter()
            .position(|c| c.is_bus() && !c.gone && self.car_departure.get(&c.id) == Some(&k))
    }

    /// The next departure of the tour whose bus `id` is.
    pub fn next_of_car(&self, id: u64) -> Option<usize> {
        self.car_departure.get(&id).and_then(|&k| self.tour_next[k])
    }

    /// Whether the tour's bus, at the end of its trip at `day_time`, takes departure `j` on:
    /// the trip is still open, the AI's, and leaves within the layover.
    pub fn may_take_on(&self, j: usize, day_time: f64) -> bool {
        let d = &self.departures[j];
        let open = self.awaiting.contains(&j) || (!d.spawned && self.runs(j));
        open && !self.is_player_tour(j) && self.day_base + d.time - day_time < TOUR_LAYOVER_MAX
    }

    /// The tour's bus took departure `j` on.
    pub fn taken_on(&mut self, j: usize) {
        self.departures[j].spawned = true;
        self.awaiting.remove(&j);
        self.pending.retain(|x| *x != j);
        self.waiting.retain(|x| *x != j);
        self.retry_at.remove(&j);
        self.later_layover.remove(&j);
    }

    /// The tour's bus did not take departure `j` on.
    pub fn not_taken_on(&mut self, j: usize) {
        // its bus is not coming: the trip gets a bus of its own
        if self.awaiting.remove(&j) {
            self.pending.push_back(j);
        }
    }

    /// The departures awaiting their tour's bus whose bus is no longer on the road.
    pub fn requeue_orphans(&mut self, traffic: &TrafficSim) {
        // a trip whose tour's bus has gone off the road meanwhile
        if !self.awaiting.is_empty() {
            let orphans: Vec<usize> = self
                .awaiting
                .iter()
                .copied()
                .filter(|&j| self.tour_prev[j].and_then(|k| self.tour_bus(k, traffic)).is_none())
                .collect();
            for j in orphans {
                self.awaiting.remove(&j);
                self.pending.push_back(j);
            }
        }
    }

    /// How many due departures are still waiting to be put on the road.
    /// Omsi.exe's station targets (0x61cb18): per bus stop, the stops the timetable's trips
    /// go on to from there, each with the termini of the trips that do. A passenger waiting
    /// at the stop wants one of these targets and boards a bus whose terminus is among its
    /// termini (0x61c33c); the names compare exactly.
    /// Per bus stop, the destinations of the trips due there within the next
    /// [`PAX_SPAWN_AHEAD`] (today's departures, the stations they stop at after it): what
    /// the people turning up there now draw their destination from. Omsi.exe spawns
    /// people for a trip up to a quarter of an hour before it is due at their stop (pionsix's
    /// tests, #1436); the destinations of every trip of the map, whatever its hour or day,
    /// had people waiting at 2 a.m. for the six o'clock bus and at a school's stop for hours
    /// before its run (#1415). Which buses they then take is still the stop's line records
    /// (`stop_targets`), from every trip.
    pub fn due_destinations(&self, day_time: f64) -> HashMap<i64, HashSet<String>> {
        let names = self.stop_names();
        let name_of = |id: i64| names.get(&id).cloned().unwrap_or_else(|| id.to_string());
        let tod = day_time - self.day_base;
        let longest = self.times.iter().flatten().map(|t| t.duration).fold(0.0, f64::max);
        let from = self.departures.partition_point(|d| d.time < tod - longest - 60.0);
        let mut out: HashMap<i64, HashSet<String>> = HashMap::new();
        for i in from..self.departures.len() {
            let d = &self.departures[i];
            if d.time > tod + PAX_SPAWN_AHEAD {
                break;
            }
            if !self.runs(i) {
                continue;
            }
            let tt = self.times_of(i);
            let stations = trip_stations(&self.data.trips[d.trip]);
            for (k, sid) in stations.iter().enumerate() {
                let due = d.time + tt.stations.get(k).map(|s| s.0).unwrap_or(0.0);
                if !(tod - 60.0..=tod + PAX_SPAWN_AHEAD).contains(&due) || !tt.stops.get(k).copied().unwrap_or(true) {
                    continue;
                }
                let here = name_of(*sid);
                let set = out.entry(*sid).or_default();
                for (j, to) in stations.iter().enumerate().skip(k + 1) {
                    if !tt.stops.get(j).copied().unwrap_or(true) {
                        continue;
                    }
                    let to = name_of(*to);
                    if to != here {
                        set.insert(to);
                    }
                }
            }
        }
        out
    }

    pub fn stop_targets(&self) -> HashMap<i64, Vec<(String, HashSet<String>)>> {
        let names = self.stop_names();
        let name_of = |id: i64| names.get(&id).cloned().unwrap_or_else(|| id.to_string());
        station_targets(self.data.trips.iter().map(|t| (trip_stations(t), t.terminus.trim().to_string())), name_of)
    }

    /// Per bus stop object, the stop objects the trips leaving it go on to, by their
    /// timetable name: the very stations whose passengers' exit weight Omsi.exe gives a
    /// destination (0x61a068: the trip's station record), not just any object of that name.
    pub fn stop_target_objects(&self) -> HashMap<i64, HashMap<String, Vec<i64>>> {
        let names = self.stop_names();
        let name_of = |id: i64| names.get(&id).cloned().unwrap_or_else(|| id.to_string());
        let mut out: HashMap<i64, HashMap<String, Vec<i64>>> = HashMap::new();
        for t in &self.data.trips {
            let stations = trip_stations(t);
            for (k, from) in stations.iter().enumerate() {
                let targets = out.entry(*from).or_default();
                for to in &stations[k + 1..] {
                    let ids = targets.entry(name_of(*to)).or_default();
                    if !ids.contains(to) {
                        ids.push(*to);
                    }
                }
            }
        }
        out
    }

    /// The name each bus stop object has in the timetable (`Busstops.cfg`, the first entry
    /// of an object id): what [`Schedule::stop_targets`] calls it. The map object's own
    /// label can read otherwise (renamed in the editor, another code page than the tiles').
    pub fn stop_names(&self) -> HashMap<i64, String> {
        let mut names = HashMap::new();
        for b in &self.data.bus_stops {
            names.entry(b.object_id).or_insert_with(|| b.name.trim().to_string());
        }
        names
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// How many due departures wait for their tiles.
    pub fn waiting_count(&self) -> usize {
        self.waiting.len()
    }

    /// The start of a tick at `day_time` (omsi-app's `Schedule::tick`): the day moved on, and
    /// the buses of a tour a player took over that are to go off the road (their departures
    /// forgotten here; the app takes them off).
    pub fn begin_tick(&mut self, day_time: f64) -> Vec<u64> {
        self.roll_day(day_time);
        // the time of day of today's timetable
        let tod = day_time - self.day_base;
        self.last_tod = tod;
        if std::mem::take(&mut self.purge_player_tour) {
            let gone: Vec<u64> = self
                .car_departure
                .iter()
                .filter(|(_, i)| self.is_player_tour(**i))
                .map(|(id, _)| *id)
                .collect();
            for &id in &gone {
                self.car_departure.remove(&id);
                self.running.retain(|r| r.car != id);
            }
            return gone;
        }
        Vec::new()
    }

    /// The departures due at `day_time` (or passed within `window` seconds) and the layover
    /// buses of the next quarter of an hour queued, those whose bus left the loaded tiles
    /// waiting for them, the waiting ones queued again when tiles brought lanes or their
    /// vehicle reaches loaded ones, and the running routes carried on. Returns whether the
    /// map is loading (a wide window), when a few more buses are put out at once.
    pub fn queue_due(&mut self, world: &dyn TimetableWorld, traffic: &mut TrafficSim, day_time: f64, window: f64) -> bool {
        let tod = day_time - self.day_base;
        let window = if std::mem::take(&mut self.restarted) { window.max(20.0 * 60.0) } else { window };
        let loading = window > 60.0;
        // At the start (and after the clock was set) every trip still under way is put out,
        // however long ago it left - yesterday's night trips before 24:00 among them, as
        // Omsi.exe's loader places each tour's bus on its last trip begun (0x73a700): only
        // the last 20 minutes' trips came, and a map opened in the evening never got the
        // bus that stands on a light's detector from 20:00 to 04:00 (Städtedreieck's
        // Bergerplatz, so its evening and night programs never ran).
        let under_way = |d: &Departure| loading && self.times.get(d.trip).and_then(|t| t.get(d.profile)).is_some_and(|t| d.time + t.duration > tod);
        let due: Vec<usize> = self
            .departures
            .iter()
            .enumerate()
            .filter(|(i, d)| !d.spawned && d.time <= tod && (d.time > tod - window || under_way(d)) && self.runs(*i))
            .map(|(i, _)| i)
            .collect();
        for i in due {
            self.departures[i].spawned = true;
            // (a layover departure whose stand was taken comes now: kept in that set it
            // counted as busy at midnight, was never made ready again and stopped running)
            self.later_layover.remove(&i);
            // the tour's bus is still on its way here: it takes the trip on when it arrives
            if self.tour_prev[i].and_then(|k| self.tour_bus(k, traffic)).is_some() {
                self.awaiting.insert(i);
                continue;
            }
            self.pending.push_back(i);
            if loading {
                self.startup.insert(i);
            }
        }
        // Buses on their layover: a tour under way has its bus. A trip whose tour's previous
        // trip is over, with no bus of the tour on the road (the map just loaded, the bus
        // left the loaded tiles), stands at its first stop with the doors shut until its
        // departure, whether other lines stop there or not - for as long as a bus
        // that drove the previous trip would wait there (TOUR_LAYOVER_MAX). The first trip
        // of a tour has no bus before it leaves: Omsi.exe 0x73bc00 (called with no lead by
        // the loader 0x709274 and the thread 0x6fdef4) puts a tour's bus out only from its
        // first trip's departure to its last trip's end, and 0x73a700 places it on the last
        // trip that has begun. Buses appeared a quarter of an hour early at their first
        // stop (#2595), and at a stop other lines use only a minute before leaving (#2593).
        let mut early: Vec<usize> = Vec::new();
        // departures are sorted by time: only the ones within a layover from now
        let start = self.departures.partition_point(|d| d.time <= tod);
        for i in start..self.departures.len() {
            let d = &self.departures[i];
            if d.time > tod + TOUR_LAYOVER_MAX {
                break;
            }
            if d.spawned || self.later_layover.get(&i).is_some_and(|&t| day_time < t) || !self.runs(i) {
                continue;
            }
            // (the previous trip of the tour only when it runs today: a tour's trips on other
            // days are no trip before this one, which is then its tour's first)
            let Some(k) = self.tour_prev[i].filter(|&k| self.runs(k)) else {
                continue;
            };
            // only a trip with stops has a first stop to wait at: a flight (TXL.ttl, every
            // ten minutes) would take off early
            if !trip_has_stations(&self.data.trips[d.trip]) {
                continue;
            }
            // nor one that does not begin at a stop: one that comes in from the map's edge
            // first stood at the start of its track early, in the way of the trains coming
            // in (X10 Berlin's S7 trains, edge to edge through Zoologischer Garten, put out
            // a quarter of an hour early)
            if !self.times_of(i).begins_at_stop() {
                continue;
            }
            let prev_end = self.dep_time(k) + self.times_of(k).duration;
            // only a break the bus that drove the previous trip waits out (`may_take_on`):
            // after a longer one that bus has gone, and an identical one stood there again
            if self.dep_time(i) - prev_end >= TOUR_LAYOVER_MAX {
                continue;
            }
            let prev_running = prev_end >= day_time || self.tour_bus(k, traffic).is_some();
            if !prev_running {
                early.push(i);
            }
        }
        for i in early {
            self.departures[i].spawned = true;
            self.later_layover.remove(&i);
            self.pending.push_back(i);
            if loading {
                self.startup.insert(i);
            }
        }
        // buses whose ground was unloaded under them wait for it to come back
        for id in std::mem::take(&mut traffic.removed_scheduled) {
            if let Some(i) = self.car_departure.remove(&id) {
                log::debug!("departure {i}: its bus left the loaded tiles, waiting for them");
                self.waiting.push(i);
            }
        }
        if self.car_departure.len() > 64 + traffic.cars.len() * 2 {
            let alive: std::collections::HashSet<u64> = traffic.cars.iter().map(|c| c.id).collect();
            self.car_departure.retain(|id, _| alive.contains(id));
        }
        // tiles brought lanes, or half a minute went by: the waiting departures may be on
        // loaded ground now, and the routes that stopped short may go on
        let grew = traffic.lanes_generation != self.seen_generation;
        if grew || day_time - self.last_retry >= 30.0 || day_time < self.last_retry {
            self.last_retry = day_time;
            for i in std::mem::take(&mut self.waiting) {
                if !self.pending.contains(&i) {
                    self.pending.push_back(i);
                }
            }
            self.retry_at.clear();
        } else if !self.retry_at.is_empty() {
            // the ones whose vehicle has just reached the loaded part of its way
            let due: Vec<usize> = self
                .waiting
                .iter()
                .copied()
                .filter(|i| {
                    self.retry_at
                        .get(i)
                        .map(|t| *t <= day_time)
                        .unwrap_or(false)
                })
                .collect();
            if !due.is_empty() {
                self.waiting.retain(|i| !due.contains(i));
                for i in due {
                    self.retry_at.remove(&i);
                    if !self.pending.contains(&i) {
                        // ahead of the others: it is due now
                        self.pending.push_front(i);
                    }
                }
            }
        }
        if grew {
            self.seen_generation = traffic.lanes_generation;
            self.carry_on(world, traffic);
        }
        loading
    }

    /// The next due departure to put on the road.
    pub fn next_pending(&mut self) -> Option<usize> {
        self.pending.pop_front()
    }

    /// What became of due departure `i` at `day_time` (taken from `next_pending`).
    pub fn settle(&mut self, i: usize, placed: &Placed, day_time: f64) {
        match placed {
            Placed::Spawned => {}
            Placed::Wait => self.waiting.push(i),
            Placed::Busy => {
                // a vehicle stands where the bus would appear (the player's bus may stand
                // there for its whole layover): again in a few seconds, without keeping
                // the traffic on its quick spawning pace meanwhile
                self.retry_at.insert(i, day_time + 3.0);
                self.waiting.push(i);
            }
            Placed::Drop => {}
        }
    }

    /// Carry the routes of the running trips on over the lanes the network gained.
    fn carry_on(&mut self, world: &dyn TimetableWorld, traffic: &mut TrafficSim) {
        let mut keep = Vec::new();
        for mut run in std::mem::take(&mut self.running) {
            let Some(ci) = traffic.cars.iter().position(|c| c.id == run.car) else {
                continue;
            };
            let last = traffic.cars[ci].state.route.last().copied();
            add_twins(traffic, &run.steps[run.next.saturating_sub(1)..]);
            let slots = self.slots(world, traffic, &run.steps[run.next..], last);
            let n = slots
                .iter()
                .position(|s| *s == Slot::Waiting)
                .unwrap_or(slots.len());
            let lanes: Vec<usize> = slots[..n]
                .iter()
                .filter_map(|s| {
                    if let Slot::Lane(l) = s {
                        Some(*l)
                    } else {
                        None
                    }
                })
                .collect();
            // (bridged from the end of what the bus has)
            let (lanes, index) = match last {
                Some(l) if !lanes.is_empty() => {
                    let with: Vec<usize> = std::iter::once(l).chain(lanes.iter().copied()).collect();
                    add_connectors(traffic, &with);
                    let (b, ix) = bridge_gaps(&traffic.net, &with);
                    (b[1..].to_vec(), ix[1..].iter().map(|k| k.saturating_sub(1)).collect::<Vec<_>>())
                }
                _ => {
                    add_connectors(traffic, &lanes);
                    bridge_gaps(&traffic.net, &lanes)
                }
            };
            if !lanes.is_empty() {
                let base = traffic.cars[ci].state.route.len();
                let mut stops = Vec::new();
                let mut from = 0;
                for (si, (sid, t_dep)) in run.stations.iter().enumerate() {
                    if run.served[si] {
                        continue;
                    }
                    let Some((pos, _)) = world.object_positions().get(sid).copied() else {
                        continue;
                    };
                    if let Some((ri, ss, lat)) = project_stop(
                        &traffic.net,
                        &lanes,
                        pos,
                        Some(STOP_REACH),
                        from,
                        world.stop_side(*sid),
                        station_route(run.station_steps[si], run.next, &slots[..n], &index),
                    ) {
                        from = ri;
                        stops.push((base + ri, ss, bay_offset(lat), *t_dep, *sid, world.stop_side(*sid)));
                        run.served[si] = true;
                    }
                }
                let (ty, rail) = (traffic.cars[ci].vehicle.ty.clone(), traffic.cars[ci].is_rail());
                place_stops(&traffic.net, &lanes, base, &mut stops, &ty, rail);
                stops.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
                log::debug!(
                    "scheduled bus {}: route carried on by {} lanes, {} more stops",
                    run.car,
                    lanes.len(),
                    stops.len()
                );
                let car = &mut traffic.cars[ci];
                car.state.route.extend(lanes);
                if let Some(b) = car.bus.as_mut() {
                    b.stops.extend(stops.into_iter().map(crate::ai_traffic::bus_service::Stop::from_tuple));
                }
                // (it may have stood waiting at the end of what it had)
                car.state.planned_next = None;
                car.state.plan_next(&traffic.net);
            }
            run.next += n;
            if run.next < run.steps.len() {
                keep.push(run);
            } else {
                if let Some(b) = traffic.cars[ci].bus.as_mut() {
                    b.route_open = false;
                }
            }
        }
        self.running = keep;
    }

    pub fn ai_timetable(&self, i: usize) -> crate::ai_traffic::bus_service::AiTimetable {
        let trip = &self.data.trips[self.departures[i].trip];
        let ids = trip_stations(trip);
        let names = self.trip_stop_names(self.departures[i].trip);
        let times = &self.times_of(i).stations;
        let departure = self.dep_time(i);
        crate::ai_traffic::bus_service::AiTimetable {
            line: self.display_line(i),
            terminus: trip.terminus.clone(),
            stops: ids
                .into_iter()
                .zip(names)
                .zip(times)
                .map(|((id, name), &(arr, dep))| {
                    (id, name, (departure + arr) as f32, (departure + dep) as f32)
                })
                .collect(),
        }
    }

    /// The trip departure `i` runs.
    pub fn trip_of(&self, i: usize) -> &omsi_timetable::Trip {
        &self.data.trips[self.departures[i].trip]
    }

    /// The index in `data.trips` of the trip departure `i` runs.
    pub fn trip_index(&self, i: usize) -> usize {
        self.departures[i].trip
    }

    /// The timetable line (`[newtour]`'s), tour and AI group of departure `i`.
    pub fn departure_line(&self, i: usize) -> &str {
        &self.departures[i].line
    }

    pub fn departure_tour(&self, i: usize) -> &str {
        &self.departures[i].tour
    }

    pub fn departure_ai_group(&self, i: usize) -> &str {
        &self.departures[i].ai_group
    }

    /// Car `car` runs departure `i`.
    pub fn bus_runs(&mut self, car: u64, i: usize) {
        self.car_departure.insert(car, i);
    }

    /// Car `car`'s route is carried on no more.
    pub fn forget_route(&mut self, car: u64) {
        self.running.retain(|r| r.car != car);
    }

    /// Car `car`'s route stops short at step `next` of `steps`: it is carried on as the tiles
    /// come, with the trip's `stations` (left at `leave`) not `served` yet.
    #[allow(clippy::too_many_arguments)]
    pub fn carry_later(&mut self, car: u64, steps: Vec<Step>, next: usize, stations: &[i64], leave: &[f64], served: Vec<bool>, station_steps: Vec<Option<usize>>) {
        self.running.push(RunningTrip {
            car,
            steps,
            next,
            stations: stations.iter().copied().zip(leave.iter().copied()).collect(),
            served,
            station_steps,
        });
    }

    /// Layover departure `i` found its stand taken at `day_time`: it tries again in a while,
    /// and comes at its departure time at the latest. Tried only once, a busy terminus had
    /// its buses only at their departure times (#2593).
    pub fn stand_taken(&mut self, i: usize, day_time: f64) {
        self.departures[i].spawned = false;
        self.startup.remove(&i);
        self.later_layover.insert(i, day_time + STAND_RETRY);
    }

    /// Departure `i` was queued while the map loads (its bus may appear in view).
    pub fn queued_at_startup(&self, i: usize) -> bool {
        self.startup.contains(&i)
    }

    /// Departure `i`'s bus is out.
    pub fn started(&mut self, i: usize) {
        self.startup.remove(&i);
    }
}
