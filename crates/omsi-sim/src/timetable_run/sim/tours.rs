//! Tours: their stops for the menus, the tours LAN players drive, and the player's duty taken
//! from one.

use super::*;

impl ScheduleSim {
    /// The stops of a tour in the order it drives them, over all its trips: (trip number in
    /// the duty, stop number in the trip, name, departure there in seconds). Passing stations
    /// are left out.
    pub fn tour_stops(&self, line: &str, tour: &str) -> Vec<(usize, usize, String, f64)> {
        let Some(l) = self.data.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)) else { return Vec::new() };
        let Some(t) = l.tours.iter().find(|t| t.number.eq_ignore_ascii_case(tour)) else { return Vec::new() };
        let mut out = Vec::new();
        let mut k = 0;
        for tt in &t.trips {
            let Some(ti) = self.data.trips.iter().position(|x| x.name.eq_ignore_ascii_case(&tt.trip)) else { continue };
            let trip = &self.data.trips[ti];
            let departure = tt.departure as f64 * 60.0;
            let times = &self.times[ti][usize::try_from(tt.profile).unwrap_or(0).min(self.times[ti].len() - 1)];
            for (i, id) in trip_stations(trip).iter().enumerate() {
                if !times.stops.get(i).copied().unwrap_or(true) {
                    continue;
                }
                let name = self
                    .data
                    .bus_stops
                    .iter()
                    .find(|b| b.object_id == *id)
                    .map(|b| b.name.clone())
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| trip.stations.is_empty().then(|| trip.stations_legacy.get(i).and_then(|r| r.get(2)).map(|n| n.trim().to_string())).flatten())
                    .unwrap_or_else(|| format!("{}", i + 1));
                out.push((k, i, name, departure + times.stations[i].1));
            }
            k += 1;
        }
        out
    }

    /// How many trips a tour has that the timetable knows (the trips `tour_stops` numbers).
    pub fn tour_trip_count(&self, line: &str, tour: &str) -> usize {
        let mut n = 0;
        let mut last = None;
        for s in self.tour_stops(line, tour) {
            if last != Some(s.0) {
                n += 1;
                last = Some(s.0);
            }
        }
        n
    }

    /// The position (in order of departure, as `tour_trip_stops` counts) of the trip of a
    /// tour that is under way or next to leave at `now` (seconds of the day).
    pub fn tour_trip_now(&self, line: &str, tour: &str, now: f64) -> usize {
        let all = self.tour_stops(line, tour);
        let mut order: Vec<(usize, f64, f64)> = Vec::new();
        for s in &all {
            match order.iter_mut().find(|o| o.0 == s.0) {
                Some(o) => o.2 = o.2.max(s.3),
                None => order.push((s.0, s.3, s.3)),
            }
        }
        order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        order.iter().position(|o| o.2 >= now - 60.0).unwrap_or(0)
    }

    /// All the stops of the trip in position `pos` of the tour's trips in order of the time
    /// they leave. The trip number in the entries is the one `tour_stops` gives it.
    pub fn tour_trip_stops(&self, line: &str, tour: &str, pos: usize) -> Vec<(usize, usize, String, f64)> {
        let all = self.tour_stops(line, tour);
        let mut order: Vec<(usize, f64)> = Vec::new();
        for s in &all {
            if !order.iter().any(|o| o.0 == s.0) {
                order.push((s.0, s.3));
            }
        }
        order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        let Some(&(k, _)) = order.get(pos) else { return Vec::new() };
        all.into_iter().filter(|s| s.0 == k).collect()
    }

    /// The stops of the trip of a tour that is under way or next to start at `now` (seconds
    /// of the day): the first trip with a stop still to come (a minute of grace), and only
    /// that trip's stops from there on. A tour with nothing left today lists its first trip
    /// whole. Same entries and numbering as `tour_stops`.
    pub fn tour_stops_from(&self, line: &str, tour: &str, now: f64) -> Vec<(usize, usize, String, f64)> {
        let all = self.tour_stops(line, tour);
        let limit = now - 60.0;
        let Some(k) = all.iter().find(|s| s.3 >= limit).map(|s| s.0).or_else(|| all.first().map(|s| s.0)) else { return Vec::new() };
        let trip: Vec<_> = all.into_iter().filter(|s| s.0 == k).collect();
        let rest: Vec<_> = trip.iter().filter(|s| s.3 >= limit).cloned().collect();
        if rest.is_empty() { trip } else { rest }
    }

    /// The player drives this tour (line and tour as `player_duty` names them): OMSI leaves
    /// it to the player, so no AI bus runs it as well - one of line 76 tour 1 appeared
    /// 5 m beside the player's own bus at the Bauernhof, where the passengers queued.
    /// Returns how many of today's departures that takes from the AI.
    pub fn reserve_tour(&mut self, line: &str, tour: &str) -> usize {
        self.player_tour = Some((line.to_string(), tour.to_string()));
        self.player_trips.clear();
        let n = self.leave_to_player();
        log::info!("timetable: {n} departures of line {line} tour {tour} left to the player");
        n
    }

    /// The player drives these trips (line, tour, departure in s of the day) of a duty made
    /// of several tours: the AI runs the rest of each tour - its bus drives off at the
    /// terminus where the player takes the tour over, and a bus of its own comes for the
    /// trips after.
    pub fn reserve_trips(&mut self, trips: Vec<(String, String, f64)>) -> usize {
        self.player_tour = None;
        self.player_departure = None;
        self.player_trips = trips;
        let n = self.leave_to_player();
        log::info!("timetable: {n} departures of the player's duty left to the player");
        n
    }

    /// Takes the departures the player drives from the AI (their buses already on the road
    /// go at the next tick). Returns how many.
    fn leave_to_player(&mut self) -> usize {
        let mine: Vec<bool> = (0..self.departures.len())
            .map(|i| self.is_player_tour(i))
            .collect();
        let mut n = 0;
        for (d, m) in self.departures.iter_mut().zip(&mine) {
            if *m {
                n += 1;
                d.spawned = true;
            }
        }
        self.pending.retain(|i| !mine[*i]);
        self.waiting.retain(|i| !mine[*i]);
        self.retry_at.retain(|i, _| !mine[*i]);
        self.purge_player_tour = true;
        n
    }

    /// LAN play (host): the tours the other players drive now. A tour taken leaves the
    /// timetable like the player's own (its bus on the road goes); one given up (the player
    /// left, or took another duty) runs again from its next departure.
    pub fn set_lan_tours(&mut self, tours: HashSet<(String, String)>) {
        if tours == self.lan_tours {
            return;
        }
        let added: Vec<(String, String)> = tours.difference(&self.lan_tours).cloned().collect();
        let removed: Vec<(String, String)> = self.lan_tours.difference(&tours).cloned().collect();
        self.lan_tours = tours;
        let of = |d: &Departure, t: &(String, String)| d.line.eq_ignore_ascii_case(&t.0) && d.tour.eq_ignore_ascii_case(&t.1);
        for t in &removed {
            let later: Vec<usize> = (0..self.departures.len())
                .filter(|&i| of(&self.departures[i], t) && self.departures[i].time > self.last_tod && !self.is_player_tour(i))
                .collect();
            for &i in &later {
                self.departures[i].spawned = false;
            }
            log::info!("timetable: line {} tour {} is no LAN player's any more: its {} later departures run again", t.0, t.1, later.len());
        }
        if !added.is_empty() {
            let mine: Vec<bool> = (0..self.departures.len()).map(|i| self.is_player_tour(i)).collect();
            for (d, m) in self.departures.iter_mut().zip(&mine) {
                if *m {
                    d.spawned = true;
                }
            }
            self.pending.retain(|i| !mine[*i]);
            self.waiting.retain(|i| !mine[*i]);
            self.retry_at.retain(|i, _| !mine[*i]);
            self.purge_player_tour = true;
            for t in &added {
                log::info!("timetable: line {} tour {} left to a LAN player", t.0, t.1);
            }
        }
    }

    /// Does a player drive departure `i` (we, or another LAN player)?
    pub(super) fn is_player_tour(&self, i: usize) -> bool {
        if let Some(d) = self.departures.get(i) {
            if self.lan_tours.iter().any(|t| d.line.eq_ignore_ascii_case(&t.0) && d.tour.eq_ignore_ascii_case(&t.1)) {
                return true;
            }
            if self.player_trips.iter().any(|(line, tour, time)| d.line.eq_ignore_ascii_case(line) && d.tour.eq_ignore_ascii_case(tour) && (d.time - time).abs() < 30.0) {
                return true;
            }
        }
        match (&self.player_tour, self.departures.get(i)) {
            (Some((line, tour)), Some(d)) => {
                d.line.eq_ignore_ascii_case(line)
                    && d.tour.eq_ignore_ascii_case(tour)
                    && self.player_departure.map(|t| (d.time - t).abs() < 30.0).unwrap_or(true)
            }
            _ => false,
        }
    }

    /// Assign the player a tour of a line (names as in the .ttl / `[newtour]`), starting
    /// with the trip that fits the time of day `now`: the one under way, else the next to
    /// leave - as OMSI does when a time is picked for a tour. (The duty used to start with
    /// the tour's first trip whatever the time: Spandau's "Mo-Fr 3" of line 5 at 15:05 began
    /// with the 14:44 depot run, and the IBIS was typed for it.) The AI no longer drives the
    /// tour: its trips are the player's ([`ScheduleSim::reserve_tour`]).
    /// When the line or its tour cannot be driven, the error says why in words a driver can
    /// act on (a line the date's chrono takes off names the chrono and the day it starts);
    /// the callers used to drop that silently and the game started without a duty.
    ///
    /// `trip` is the trip to start with as the player picked it (OMSI's timetable dialog
    /// has line, tour and trip): its departure time "HH:MM" - the first trip leaving then or
    /// later - or its number in the tour (1 = first). The duty goes on with the tour's next
    /// trips from there, as OMSI's does.
    pub fn player_duty(
        &mut self,
        world: &dyn TimetableWorld,
        line: &str,
        tour: &str,
        now: f64,
        trip: Option<&str>,
        whole_tour: bool,
    ) -> Result<PlayerDuty, String> {
        let date = |code: i32| {
            format!(
                "{:04}-{:02}-{:02}",
                code / 10000,
                code / 100 % 100,
                code % 100
            )
        };
        let Some(l) = self
            .data
            .lines
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(line))
        else {
            let running = self.data.lines.len();
            if let Some((name, dir)) = self
                .deactivated
                .iter()
                .find(|(l, _)| l.trim().eq_ignore_ascii_case(line.trim()))
            {
                let from = Some(omsi_cfg::resolve_path(dir, "Chrono.cfg"))
                    .and_then(|p| omsi_cfg::CfgFile::read(&p).ok())
                    .map(|f| omsi_map::ailists::parse_chrono_cfg(&f).start_date)
                    .filter(|d| *d > 0);
                return Err(format!(
                    "line {name} does not run on {}: the timetable change '{}'{} takes it off. {running} other lines run that day: pick one of them, or an earlier date",
                    date(world.date()),
                    dir.file_name().unwrap_or_default().to_string_lossy(),
                    from.map(|d| format!(" of {}", date(d))).unwrap_or_default()
                ));
            }
            return Err(format!("line {line} is not in the timetable of this map on {}: {running} lines run that day", date(world.date())));
        };
        let t = match l.tours.iter().find(|t| t.number.eq_ignore_ascii_case(tour)) {
            Some(t) => t,
            None => {
                let Some(first) = l.tours.first() else {
                    return Err(format!("line {} has no tours", l.name));
                };
                if !tour.is_empty() {
                    log::warn!(
                        "line {} has no tour '{tour}': driving its tour {}",
                        l.name,
                        first.number
                    );
                }
                first
            }
        };
        let mut trips = self.planned_trips(world, t);
        if trips.is_empty() {
            return Err(format!(
                "line {} tour {} has no trip the timetable knows ({} listed)",
                l.name,
                t.number,
                t.trips.len()
            ));
        }
        // Yesterday's night tour, still under way after midnight (its trips past 24:00 are
        // tonight's): its times a day earlier, so the duty goes on with the trip running
        // now. After a restart at 0:45 the night lines begun the day before could not be
        // taken (#1576).
        if !self.tour_runs_on(t, self.day_bits) && self.tour_runs_on(t, self.yesterday_bits()) && trips.iter().any(|p| p.departure >= DAY) && trips.last().is_some_and(|p| p.end - DAY > now) {
            for p in &mut trips {
                p.departure -= DAY;
                p.end -= DAY;
                for s in &mut p.stops {
                    s.arr -= DAY;
                    s.dep -= DAY;
                }
            }
            log::info!("player duty: line {} tour {} is yesterday's night tour, under way past midnight", l.name, t.number);
        }
        let (line_name, tour_name) = (l.name.clone(), t.number.clone());
        let trip_index = match trip.map(str::trim).filter(|t| !t.is_empty()) {
            Some(pick) => chosen_trip(&trips, pick).ok_or_else(|| {
                format!(
                    "line {line_name} tour {tour_name} has no trip '{pick}': its {} trips leave {}",
                    trips.len(),
                    trips.iter().map(|t| hhmm(t.departure)).collect::<Vec<_>>().join(", ")
                )
            })?,
            None => starting_trip(&trips, now),
        };
        let hm = hhmm;
        // A picked trip is the duty, one way to its terminus, as a trip chosen in OMSI is;
        // the rest of the tour only with `--whole-tour`.
        let (trips, trip_index, first_trip) = if trip.is_some() && !whole_tour {
            (vec![trips[trip_index].clone()], 0, trip_index)
        } else {
            (trips, trip_index, 0)
        };
        // the AI leaves the player what the player drives: the tour, or just the one trip
        self.player_departure = (trips.len() == 1 && trip.is_some() && !whole_tour).then(|| trips[0].departure);
        let reserved = self.reserve_tour(&line_name, &tour_name);
        let current = &trips[trip_index];
        log::info!(
            "player duty: line {line_name} tour {tour_name}: {} trips from {} ({} of them not driven by the AI); at {} trip {} {} {} (to {}), then {}",
            trips.len(),
            hm(trips[0].departure),
            reserved,
            hm(now),
            trip_index + 1,
            current.name,
            // (a trip after midnight picked in the evening leaves tonight)
            if current.departure < now && now - current.departure < DAY / 2.0 { format!("under way since {}", hm(current.departure)) } else { format!("leaves at {}", hm(current.departure)) },
            current.terminus,
            trips.get(trip_index + 1).map(|t| format!("{} at {}", t.name, hm(t.departure))).unwrap_or_else(|| "the end of the duty".into())
        );
        Ok(PlayerDuty::new(
            line_name,
            tour_name,
            trips,
            trip_index,
            first_trip,
            trip.map(|t| !t.trim().is_empty()).unwrap_or(false),
        ))
    }

    /// The trips of a tour as the player drives them, with their stops and times; a trip the
    /// timetable does not know is left out (so a trip's place here is its place in OMSI's
    /// dialog, which leaves them out too).
    fn planned_trips(&self, world: &dyn TimetableWorld, t: &omsi_timetable::Tour) -> Vec<PlannedTrip> {
        let mut trips = Vec::new();
        for tt in &t.trips {
            let Some(ti) = self
                .data
                .trips
                .iter()
                .position(|x| x.name.eq_ignore_ascii_case(&tt.trip))
            else {
                continue;
            };
            trips.push(self.planned_trip(world, ti, tt.profile, tt.departure as f64 * 60.0));
        }
        trips
    }

    /// Trip `ti` of the timetable as the player drives it, leaving at `departure` (seconds of
    /// the day) with the times of its profile `profile`: its stops, their places and times.
    /// A stop is the object of the tile its station names (`trip_station_tiles`), where the
    /// map repeats its ids across tiles.
    fn planned_trip(&self, world: &dyn TimetableWorld, ti: usize, profile: i32, departure: f64) -> PlannedTrip {
        let trip = &self.data.trips[ti];
        let times = &self.times[ti][usize::try_from(profile)
            .unwrap_or(0)
            .min(self.times[ti].len() - 1)];
        let tiles = trip_station_tiles(trip, &self.data.bus_stops);
        let stops = trip_stations(trip)
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let name = self.station_name(trip, i, *id);
                let position = world.object_of_tile(tiles.get(i).copied().flatten(), *id).map(|p| p.0);
                let (arr, dep) = times.stations[i];
                PlannedStop {
                    object_id: *id,
                    name,
                    arr: departure + arr,
                    dep: departure + dep,
                    position,
                    dir: StopDir::default(),
                    stops: times.stops[i],
                }
            })
            .collect();
        let mut planned = PlannedTrip {
            name: trip.name.clone(),
            line: trip.line.clone(),
            terminus: trip.terminus.clone(),
            departure,
            end: departure + times.duration,
            stops,
        };
        planned.set_dirs();
        planned
    }

    /// A free drive along one route of a line (`--free-line`, the launcher's free drive with
    /// a line the player chose): the timetable's trip `trip` (its file's name, the route as
    /// the map names it) as a duty of that one trip, leaving at `now` - the player chose
    /// where to drive, not when - so the navigator draws its way and the IBIS can be typed
    /// for it. Nothing is booked: the AI keeps every tour (the timetable's own bus on that
    /// route still drives), and the duty is `free`, so nobody keeps its time. `line` is the
    /// timetable's line it was chosen under.
    pub fn free_line_duty(&self, world: &dyn TimetableWorld, line: &str, trip: &str, now: f64) -> Result<PlayerDuty, String> {
        let Some(ti) = self.data.trips.iter().position(|x| x.name.trim().eq_ignore_ascii_case(trip.trim())) else {
            return Err(format!("the timetable of this map has no route '{}'", trip.trim()));
        };
        if self.times.get(ti).is_none_or(|t| t.is_empty()) {
            return Err(format!("route '{}' has no times", trip.trim()));
        }
        let planned = self.planned_trip(world, ti, 0, now);
        if planned.stops.is_empty() {
            return Err(format!("route '{}' has no stops", trip.trim()));
        }
        let line = if line.trim().is_empty() { planned.line.trim().to_string() } else { line.trim().to_string() };
        log::info!(
            "free drive along line {line}: route {} from {} to {}, {} stops, nothing booked",
            planned.name,
            planned.stops[0].name.trim(),
            planned.terminus.trim(),
            planned.stops.len()
        );
        let mut d = PlayerDuty::new(line, String::new(), vec![planned], 0, 0, true);
        d.free = true;
        Ok(d)
    }

    /// A duty of trips from several tours, as the launcher puts one together (omsi-app's
    /// `--duty-leg`, parsed there: see `omsi_launcher_lib::compose`): each leg is (line, tour,
    /// first, trips), so many trips of the tour from its trip `first` (1 = the first) on.
    /// Where one leg ends the next leaves, a few minutes later: the bus stays the player's
    /// and takes the next tour over there, as a driver whose shift goes on with another
    /// line. The AI keeps the rest of those tours.
    pub fn player_plan(&mut self, world: &dyn TimetableWorld, legs: &[(String, String, usize, usize)]) -> Result<PlayerDuty, String> {
        let mut trips: Vec<PlannedTrip> = Vec::new();
        let mut tours: Vec<(String, String, usize)> = Vec::new();
        for (line, tour, first, count) in legs {
            let (first, count) = (*first, *count);
            let Some(l) = self.data.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)) else {
                return Err(format!("line {line} is not in the timetable of this map on this day"));
            };
            let Some(t) = l.tours.iter().find(|t| t.number.eq_ignore_ascii_case(tour)) else {
                return Err(format!("line {} has no tour {tour}", l.name));
            };
            let all = self.planned_trips(world, t);
            if first == 0 || first > all.len() {
                return Err(format!("line {} tour {} has no trip {first}: it has {}", l.name, t.number, all.len()));
            }
            for (k, p) in all.into_iter().enumerate().skip(first - 1).take(count.max(1)) {
                tours.push((l.name.clone(), t.number.clone(), k));
                trips.push(p);
            }
        }
        let Some(first) = trips.first() else {
            return Err("the duty has no trips".into());
        };
        log::info!(
            "player duty of {} trips in {} parts: from {} {} at {} to {} at {}",
            trips.len(),
            legs.len(),
            tours[0].0,
            first.name,
            hhmm(first.departure),
            trips[trips.len() - 1].terminus,
            hhmm(trips[trips.len() - 1].end)
        );
        self.reserve_trips(trips.iter().zip(&tours).map(|(p, (l, t, _))| (l.clone(), t.clone(), p.departure)).collect());
        // (the duty is the player's from its first trip, however late the bus is for it)
        let mut d = PlayerDuty::new(tours[0].0.clone(), tours[0].1.clone(), trips, 0, 0, true);
        d.tours = tours;
        Ok(d)
    }
}
