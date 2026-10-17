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
        log::info!("timetable: {n} departures of line {line} tour {tour} left to the player");
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
        }
        match (&self.player_tour, self.departures.get(i)) {
            (Some((line, tour)), Some(d)) => {
                d.line.eq_ignore_ascii_case(line)
                    && d.tour.eq_ignore_ascii_case(tour)
                    && self.player_departure.map(|t| {
                        // (yesterday's night trips a day earlier: 00:30 is the trip of 24:30)
                        let dt = (d.time - t).rem_euclid(DAY);
                        dt < 30.0 || dt > DAY - 30.0
                    }).unwrap_or(true)
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
            let trip = &self.data.trips[ti];
            let departure = tt.departure as f64 * 60.0;
            let times = &self.times[ti][usize::try_from(tt.profile)
                .unwrap_or(0)
                .min(self.times[ti].len() - 1)];
            let stops = trip_stations(trip)
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let name = self.station_name(trip, i, *id);
                    let position = world.object_positions().get(id).map(|p| p.0);
                    let (arr, dep) = times.stations[i];
                    PlannedStop {
                        object_id: *id,
                        name,
                        arr: departure + arr,
                        dep: departure + dep,
                        position,
                        dir: StopDir::default(),
                        stops: times.stops[i],
                        closed: None,
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
            trips.push(planned);
        }
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
        // taken (#1576). A tour that runs every night is yesterday's too while today's has not
        // begun: at 03:25 the 22:00 tour's trip of 27:30 is the one at 03:30, not tomorrow's
        // (#2166).
        if (!self.tour_runs_on(t, self.day_bits) || trips[0].departure > now) && self.tour_runs_on(t, self.yesterday_bits()) && trips.iter().any(|p| p.departure >= DAY) && trips.last().is_some_and(|p| p.end - DAY > now) {
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
        let unnumbered_public = self.unnumbered_public(&trips);
        let mut duty = PlayerDuty::new(
            line_name,
            tour_name,
            trips,
            trip_index,
            first_trip,
            trip.map(|t| !t.trim().is_empty()).unwrap_or(false),
        );
        duty.unnumbered_public = unnumbered_public;
        Ok(duty)
    }

    /// The trips of `trips` without a line number that one of the map's depot files runs as
    /// public trips ([`ibis::serves_unnumbered`]).
    fn unnumbered_public(&self, trips: &[PlannedTrip]) -> Vec<String> {
        let mut hofs: Vec<&Arc<omsi_vehicle::Hof>> = Vec::new();
        for h in self.depots.values().flatten().filter_map(|v| v.2.as_ref()) {
            if !hofs.iter().any(|x| Arc::ptr_eq(x, h)) {
                hofs.push(h);
            }
        }
        let mut out: Vec<String> = Vec::new();
        for t in trips.iter().filter(|t| t.line.trim().is_empty()) {
            let stops: Vec<&str> = t.stops.iter().map(|s| s.name.as_str()).collect();
            if !out.contains(&t.name) && hofs.iter().any(|h| ibis::serves_unnumbered(h, &t.terminus, &stops)) {
                log::info!("player duty: trip {} has no line number, but the depot file runs it as a public trip (line 000)", t.name);
                out.push(t.name.clone());
            }
        }
        out
    }
}
