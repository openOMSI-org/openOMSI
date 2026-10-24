//! Loading the timetable, the day it runs and which tours run on it.

use super::*;

impl ScheduleSim {
    /// `clock` gives the date: tours carry a validity mask (bits 0-6 Monday…Sunday, 7 public
    /// holiday, 8 school holidays, 9 school days) that selects which run today.
    pub fn new(root: &Path, world: &dyn TimetableWorld, clock: &crate::SimClock) -> ScheduleSim {
        let chrono_dirs = world.chrono_dirs();
        let deactivated = omsi_map::chrono_deactivated_lines(&chrono_dirs);
        let data = TimetableData::load_with_chrono(world.map_dir(), &chrono_dirs, &deactivated);
        for e in &data.errors {
            log::warn!("timetable: {e}");
        }
        let calendar =
            omsi_map::Calendar::load(&world.map_dir().join("Holidays.txt")).unwrap_or_default();
        let car_use = omsi_timetable::CarUse::load_dir(world.map_dir());
        // station link lengths, the first link of a pair counts (as the routes take it)
        let mut links: HashMap<(i64, i64), f64> = HashMap::new();
        for l in &data.stn_links {
            links.entry((l.from_id, l.to_id)).or_insert(l.length);
        }
        let link_length = |a: i64, b: i64| links.get(&(a, b)).copied();
        let times: Vec<Vec<TripTimes>> = data
            .trips
            .iter()
            .map(|t| {
                let stations = trip_stations(t);
                if t.profiles.is_empty() {
                    vec![TripTimes::new(&stations, None, &link_length)]
                } else {
                    t.profiles
                        .iter()
                        .map(|p| TripTimes::new(&stations, Some(p), &link_length))
                        .collect()
                }
            })
            .collect();
        let mut visits: HashMap<i64, Vec<(usize, usize)>> = HashMap::new();
        for (ti, t) in data.trips.iter().enumerate() {
            for (k, sid) in trip_stations(t).iter().enumerate() {
                visits.entry(*sid).or_default().push((ti, k));
            }
        }
        let date = clock.date_code();
        let weekday = clock.weekday();
        let holiday = calendar.is_holiday(date);
        let school_holiday = calendar.in_holiday_range(date);
        let (day_bit, school_bit) = day_bits(&calendar, clock);
        let mut departures = Vec::new();
        let mut skipped_tours = 0;
        for line in &data.lines {
            for tour in &line.tours {
                let mask = tour.extra.trim().parse::<i32>().unwrap_or(1023);
                if mask & day_bit == 0 || mask & school_bit == 0 {
                    skipped_tours += 1;
                }
                for t in &tour.trips {
                    if let Some(ti) = data
                        .trips
                        .iter()
                        .position(|x| x.name.eq_ignore_ascii_case(&t.trip))
                    {
                        let profile = usize::try_from(t.profile)
                            .unwrap_or(0)
                            .min(times[ti].len() - 1);
                        let d = Departure {
                            time: t.departure as f64 * 60.0,
                            trip: ti,
                            profile,
                            line: line.name.clone(),
                            ai_group: tour.ai_group.clone(),
                            tour: tour.number.clone(),
                            mask,
                            spawned: false,
                            yesterday: false,
                        };
                        // (a trip after 24:00 runs in the early hours of the next day as
                        // well: the night tour that began the evening before, #2166 - and so
                        // does one that leaves before midnight and is still on its way after
                        // it, for a map opened in the small hours)
                        if reaches_tomorrow(d.time, times[ti][profile].duration) {
                            departures.push(Departure { time: d.time - DAY, yesterday: true, line: d.line.clone(), ai_group: d.ai_group.clone(), tour: d.tour.clone(), ..d });
                        }
                        departures.push(d);
                    }
                }
            }
        }
        departures.sort_by(|a, b| a.time.total_cmp(&b.time));
        let night_twin = night_twins(&departures);
        let mut trip_departures = vec![Vec::new(); data.trips.len()];
        for (i, d) in departures.iter().enumerate() {
            trip_departures[d.trip].push(i);
        }
        // the previous departure of each tour, looked up once: the layover check used to
        // walk all 3600 Spandau departures for each of them, every two seconds - 1.5 fps
        let mut tour_prev = vec![None; departures.len()];
        let mut last_of: HashMap<(String, String), usize> = HashMap::new();
        for (i, d) in departures.iter().enumerate() {
            // (yesterday's night trips are a tour of their own: tonight's run on from the
            // evening's trips)
            let key = (d.line.clone(), if d.yesterday { format!("{}\u{1}", d.tour) } else { d.tour.clone() });
            tour_prev[i] = last_of.get(&key).copied();
            last_of.insert(key, i);
        }
        let mut tour_next = vec![None; departures.len()];
        for (i, p) in tour_prev.iter().enumerate() {
            if let Some(p) = p {
                tour_next[*p] = Some(i);
            }
        }
        let mut depots = HashMap::new();
        let mut warned: HashSet<String> = HashSet::new();
        let mut hof_cache: HashMap<(PathBuf, String), Option<Arc<omsi_vehicle::Hof>>> =
            HashMap::new();
        // a bus file several depots (or typgroups) list is one type: loaded once
        let mut loaded: HashMap<PathBuf, Option<Arc<VehicleType>>> = HashMap::new();
        let mut load = |path: &std::path::Path| -> Result<Arc<VehicleType>, String> {
            loaded
                .entry(path.to_path_buf())
                .or_insert_with(|| {
                    VehicleType::load_ai(root, path)
                        .map(Arc::new)
                        .map_err(|e| log::debug!("{}: {e}", path.display()))
                        .ok()
                })
                .clone()
                .ok_or_else(|| "could not be loaded".to_string())
        };
        {
            let lists = world.ailists();
            for g in lists.groups.iter().filter(|g| g.is_depot) {
                let mut vehicles = Vec::new();
                for tg in &g.typgroups {
                    let path = omsi_cfg::resolve_path(root, &tg.file);
                    match load(&path) {
                        Ok(t) => {
                            let hof = g
                                .hof
                                .as_ref()
                                .and_then(|h| depot_file(&mut hof_cache, t.def.dir(), h));
                            vehicles.push((t, tg.entries.clone(), hof));
                        }
                        Err(e) => {
                            if !warned.contains(&tg.file) {
                                log::warn!("depot vehicle {}: {e}", tg.file);
                                warned.insert(tg.file.clone());
                            }
                        }
                    }
                }
                depots.insert(g.name.to_ascii_lowercase(), vehicles);
            }
        }
        // train groups: [aigroup_2] entries pointing at .zug files
        let mut trains: HashMap<String, Vec<TrainCars>> = HashMap::new();
        {
            let lists = world.ailists();
            for g in lists.groups.iter().filter(|g| !g.is_depot) {
                for v in &g.vehicles {
                    if !v.file.to_ascii_lowercase().ends_with(".zug") {
                        continue;
                    }
                    let path = omsi_cfg::resolve_path(root, &v.file);
                    let Ok(train) = omsi_vehicle::vehicle::Train::load(&path) else {
                        continue;
                    };
                    let mut cars = Vec::new();
                    let mut pick = 0usize;
                    for (file, rev) in &train.cars {
                        // an entry is a vehicle file, or the name of a vehicle pool group
                        let p = omsi_cfg::resolve_path(root, file);
                        let path = if omsi_cfg::vfs::is_file(&p) {
                            Some(p)
                        } else {
                            lists
                                .groups
                                .iter()
                                .find(|g| {
                                    g.name.eq_ignore_ascii_case(file.trim())
                                        && !g.vehicles.is_empty()
                                })
                                .map(|g| {
                                    pick += 1;
                                    omsi_cfg::resolve_path(
                                        root,
                                        &g.vehicles[pick % g.vehicles.len()].file,
                                    )
                                })
                        };
                        let Some(path) = path else {
                            log::warn!("train car {file}: no such file or group");
                            continue;
                        };
                        match load(&path) {
                            Ok(t) => cars.push((t, *rev)),
                            Err(e) => log::warn!("train car {}: {e}", file),
                        }
                    }
                    if !cars.is_empty() {
                        trains
                            .entry(g.name.to_ascii_lowercase())
                            .or_default()
                            .push(cars);
                    }
                }
            }
        }
        let tile_coords = world.raw_tiles();
        if omsi_cfg::flags::OMSI_PROFILE.is_set() {
            let mut seen: HashSet<*const VehicleType> = HashSet::new();
            let mut bytes = 0usize;
            for t in depots
                .values()
                .flatten()
                .map(|v| &v.0)
                .chain(trains.values().flatten().flatten().map(|c| &c.0))
            {
                if seen.insert(Arc::as_ptr(t)) {
                    bytes += t.mesh_bytes();
                }
            }
            log::info!(
                "timetable: {} vehicle types loaded, {:.0} MB of meshes on the CPU",
                seen.len(),
                bytes as f64 / 1e6
            );
        }
        let today = departures.iter().filter(|d| d.mask & day_bit != 0 && d.mask & school_bit != 0).count();
        log::info!("timetable: {} lines, {} trips, {} tracks, {} departures today (weekday {weekday}, holiday {holiday}, school holidays {school_holiday}; {skipped_tours} tours not today), {} depot groups", data.lines.len(), data.trips.len(), data.tracks.len(), today, depots.len());
        let mut s = ScheduleSim {
            data,
            departures,
            depots,
            tile_coords,
            trains,
            pools: HashMap::new(),
            pool_hofs: HashMap::new(),
            pending: Default::default(),
            tour_prev,
            waiting: Vec::new(),
            running: Vec::new(),
            seen_generation: 0,
            last_retry: f64::NEG_INFINITY,
            car_departure: HashMap::new(),
            retry_at: HashMap::new(),
            times,
            visits,
            trip_departures,
            boards_made: f64::NEG_INFINITY,
            player_tour: None,
            player_departure: None,
            purge_player_tour: false,
            lan_tours: HashSet::new(),
            last_tod: 0.0,
            deactivated,
            startup: Default::default(),
            later_layover: Default::default(),
            tour_next,
            awaiting: Default::default(),
            restarted: false,
            calendar,
            day: date,
            day_bits: (day_bit, school_bit),
            yesterday_bits: (0, 0),
            night_twin,
            next_day_bit: 1 << ((clock.weekday() + 1) % 7),
            day_base: 0.0,
            date_clock: clock.clone(),
            car_use,
            tour_vehicle: HashMap::new(),
            used_numbers: HashSet::new(),
            vehicle_seed: omsi_script::session_seed().map(|s| omsi_script::mix_seed(s ^ 0x746f_7572_5f62_7573)).unwrap_or(0),
        };
        s.yesterday_bits = s.yesterday_bits();
        s.assign_car_use();
        s
    }

    /// The day's vehicles of the tours the map's `car_use` names (the original, run
    /// at load and when the date changes): for every record in force whose line runs,
    /// first its `[number_tour]` pairs (a fleet number for a tour), then for each other
    /// tour of the line, with the probability `[types_prefered]` gives (1 for
    /// `[onlytypes]`), a fleet number of one of the listed types from the tour's depot
    /// group that no tour has yet. Other tours draw theirs when they are first due.
    pub(super) fn assign_car_use(&mut self) {
        self.tour_vehicle.clear();
        self.used_numbers.clear();
        let date = self.day;
        let mut n_fixed = 0;
        let mut n_typed = 0;
        for cu in self.car_use.iter().filter(|c| c.valid_on(date)) {
            let Some(line) = self.data.lines.iter().find(|l| l.name.trim().eq_ignore_ascii_case(cu.line.trim())) else { continue };
            // [number_tour]: this tour runs with this vehicle
            for (number, tour) in &cu.number_tour {
                let Some(t) = line.tours.iter().find(|t| t.number.trim().eq_ignore_ascii_case(tour.trim())) else { continue };
                let group = t.ai_group.to_ascii_lowercase();
                let key = tour_key_of(&group, &line.name, &t.number);
                if self.tour_vehicle.contains_key(&key) || self.used_numbers.contains(&(group.clone(), number.trim().to_string())) {
                    continue;
                }
                let found = self.depots.get(&group).and_then(|v| {
                    v.iter().enumerate().find_map(|(k, (_, nums, _))| nums.iter().position(|e| e.number.trim() == number.trim()).map(|j| (k, j)))
                });
                if let Some(kj) = found {
                    self.tour_vehicle.insert(key, kj);
                    self.used_numbers.insert((group, number.trim().to_string()));
                    n_fixed += 1;
                }
            }
            // [onlytypes] / [types_prefered]: the other tours from those types
            let Some((factor, types)) = cu.types() else { continue };
            let types: Vec<String> = types.iter().map(|t| norm_vehicle_path(t)).collect();
            for t in &line.tours {
                let group = t.ai_group.to_ascii_lowercase();
                let key = tour_key_of(&group, &line.name, &t.number);
                if self.tour_vehicle.contains_key(&key) {
                    continue;
                }
                let h = mix(key ^ date as u64);
                if (h >> 11) as f64 / (1u64 << 53) as f64 > factor as f64 {
                    continue;
                }
                let Some(vehicles) = self.depots.get(&group) else { continue };
                let candidates: Vec<(usize, usize)> = vehicles
                    .iter()
                    .enumerate()
                    .filter(|(_, (ty, _, _))| {
                        let p = norm_vehicle_path(&ty.def.path.to_string_lossy());
                        types.iter().any(|w| p.ends_with(w.as_str()))
                    })
                    .flat_map(|(k, (_, nums, _))| (0..nums.len()).map(move |j| (k, j)))
                    .filter(|(k, j)| !self.used_numbers.contains(&(group.clone(), vehicles[*k].1[*j].number.trim().to_string())))
                    .collect();
                if candidates.is_empty() {
                    continue;
                }
                let (k, j) = candidates[(mix(h) % candidates.len() as u64) as usize];
                if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                    log::info!("car_use: line {} tour {} -> {} #{}", line.name, t.number, vehicles[k].0.def.path.display(), vehicles[k].1[j].number);
                }
                self.used_numbers.insert((group.clone(), vehicles[k].1[j].number.trim().to_string()));
                self.tour_vehicle.insert(key, (k, j));
                n_typed += 1;
            }
        }
        if n_fixed + n_typed > 0 {
            log::info!("timetable: car_use gives {n_fixed} tours their fleet number and {n_typed} a vehicle of their line's types");
        }
    }

    /// When departure `i` leaves on the traffic's clock.
    pub(super) fn dep_time(&self, i: usize) -> f64 {
        self.day_base + self.departures[i].time
    }

    /// Move the calendar without rebuilding every intervening day's timetable.
    /// A large date jump needs only the final day's mask and fleet assignment.
    pub(super) fn shift_calendar_days(&mut self, days: i64) {
        if days == 0 { return; }
        self.day_base += days as f64 * DAY;
        step_calendar(&mut self.date_clock, days);
        let c = self.date_clock.clone();
        self.set_day(&c);
    }

    /// Past midnight on the traffic's clock: the next day's timetable.
    pub(super) fn roll_day(&mut self, day_time: f64) {
        if !day_time.is_finite() { return; }
        let days = ((day_time - self.day_base) / DAY).floor() as i64;
        if days > 0 {
            self.shift_calendar_days(days);
        }
    }

    /// Whether a tour is offered on the current day (the lists of lines and tours): its day
    /// mask has the day (and school day or holiday), or - for a night tour with trips after
    /// 24:00 - the next day's weekday, as the night belongs to both, or the day before's
    /// (its trips after midnight run today, #1576).
    pub fn tour_available(&self, tour: &omsi_timetable::Tour) -> bool {
        let m = tour.extra.trim().parse::<i32>().unwrap_or(1023);
        if m & self.day_bits.0 != 0 && m & self.day_bits.1 != 0 {
            return true;
        }
        let night = tour.trips.iter().any(|t| t.departure >= 24.0 * 60.0);
        night && ((m & self.next_day_bit != 0 && m & self.day_bits.1 != 0) || self.tour_runs_on(tour, self.yesterday_bits()))
    }

    /// Whether a tour's day mask has these day bits (`day_bits`).
    pub(super) fn tour_runs_on(&self, tour: &omsi_timetable::Tour, bits: (i32, i32)) -> bool {
        let m = tour.extra.trim().parse::<i32>().unwrap_or(1023);
        m & bits.0 != 0 && m & bits.1 != 0
    }

    /// The day bits of the day before the current one.
    pub(super) fn yesterday_bits(&self) -> (i32, i32) {
        let mut c = self.date_clock.clone();
        if c.day_of_year > 1 {
            c.day_of_year -= 1;
        } else {
            c.year -= 1;
            c.day_of_year = crate::clock::days_in_year(c.year);
        }
        day_bits(&self.calendar, &c)
    }

    /// Whether departure `i`'s tour runs on the current day.
    pub(super) fn runs(&self, i: usize) -> bool {
        let d = &self.departures[i];
        let bits = if d.yesterday { self.yesterday_bits } else { self.day_bits };
        d.mask & bits.0 != 0 && d.mask & bits.1 != 0
    }

    /// At midnight: tonight's night trips (24:10, ...) whose tour's bus is on them already
    /// or about to take them on are from now on yesterday's (00:10): the same moment, and
    /// the tour's bus goes on with the next of them. Left as tonight's, the bus found no
    /// trip of its tour to take on after midnight, and yesterday's came with a bus of its own.
    fn night_trips_to_yesterday(&mut self) {
        let to: HashMap<usize, usize> = (0..self.departures.len())
            .filter(|&i| !self.departures[i].yesterday)
            .filter_map(|i| Some((i, self.night_twin.get(i).copied().flatten()?)))
            .collect();
        if to.is_empty() {
            return;
        }
        let map = |i: &mut usize| {
            if let Some(&j) = to.get(i) {
                *i = j;
            }
        };
        for v in self.car_departure.values_mut() {
            map(v);
        }
        self.pending.iter_mut().for_each(map);
        self.waiting.iter_mut().for_each(map);
        let moved = |set: &mut std::collections::HashSet<usize>| {
            *set = set.iter().map(|i| to.get(i).copied().unwrap_or(*i)).collect();
        };
        moved(&mut self.awaiting);
        self.later_layover = self.later_layover.iter().map(|(i, t)| (to.get(i).copied().unwrap_or(*i), *t)).collect();
        moved(&mut self.startup);
        self.retry_at = self.retry_at.iter().map(|(i, t)| (to.get(i).copied().unwrap_or(*i), *t)).collect();
        for (&from, &j) in &to {
            if self.departures[from].spawned {
                self.departures[j].spawned = true;
            }
        }
    }

    /// Move the timetable on to the clock's date when it has changed (midnight): the day's
    /// tours are chosen anew and every departure may run again. Before, the departures were
    /// made once for the start day and stayed spawned, so after the first midnight only the
    /// early-morning trips before the start time came, and on the old day's tours.
    /// Departures still queued or under way keep their state; the player's tour stays taken.
    pub(super) fn set_day(&mut self, clock: &crate::SimClock) {
        let date = clock.date_code();
        if date == self.day {
            return;
        }
        let forward = date > self.day;
        self.day = date;
        self.day_bits = day_bits(&self.calendar, clock);
        self.yesterday_bits = self.yesterday_bits();
        self.next_day_bit = 1 << ((clock.weekday() + 1) % 7);
        // (only when the clock runs on over midnight: set back, every bus went already)
        if forward {
            self.night_trips_to_yesterday();
        }
        let busy: HashSet<usize> = self
            .pending
            .iter()
            .chain(self.waiting.iter())
            .chain(self.awaiting.iter())
            .chain(self.later_layover.keys())
            .chain(self.car_departure.values())
            .copied()
            .collect();
        let mut n = 0;
        for i in 0..self.departures.len() {
            if busy.contains(&i) {
                continue;
            }
            let mine = self.is_player_tour(i);
            let d = &mut self.departures[i];
            if d.spawned && !mine {
                d.spawned = false;
                n += 1;
            }
        }
        self.startup.clear();
        self.boards_made = f64::NEG_INFINITY;
        self.assign_car_use();
        let today = (0..self.departures.len()).filter(|&i| self.runs(i)).count();
        log::info!("timetable: a new day ({date}): {today} departures today, {n} made ready to run again");
    }

    /// When departure `i`'s bus is at its trip's stations.
    /// The stations departure `i` serves whoever wants them or not (`[profile_otherstopping]`
    /// 1 or 4), those it serves when it would be early (3), and the ones whose time the map
    /// wrote itself (the bus waits there for its departure), by object id.
    pub fn special_stops(&self, i: usize) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
        let stations = trip_stations(&self.data.trips[self.departures[i].trip]);
        let times = self.times_of(i);
        let kinds = &times.kinds;
        let of = |want: &[u8]| -> Vec<i64> {
            stations.iter().zip(kinds).filter(|(_, k)| want.contains(k)).map(|(id, _)| *id).collect()
        };
        let holds: Vec<i64> = stations
            .iter()
            .zip(&times.holds)
            .filter(|(_, h)| **h)
            .map(|(id, _)| *id)
            .collect();
        (of(&[1, 4]), of(&[3]), holds)
    }

    pub(super) fn times_of(&self, i: usize) -> &TripTimes {
        let d = &self.departures[i];
        &self.times[d.trip][d.profile]
    }

    /// A 64-bit hash of a departure's tour (its group, line and tour number).
    pub(super) fn tour_key(&self, i: usize) -> u64 {
        let d = &self.departures[i];
        tour_key_of(&d.ai_group.to_ascii_lowercase(), &d.line, &d.tour)
    }
}

/// Per departure, its night twin (`ScheduleSim::night_twin`): the trip after 24:00 of a tour
/// and the same trip of yesterday's tour, a day earlier.
pub(super) fn night_twins(departures: &[Departure]) -> Vec<Option<usize>> {
    let key = |d: &Departure, time: f64| (d.line.clone(), d.tour.clone(), d.trip, d.profile, (time * 10.0).round() as i64);
    let tonight: HashMap<_, usize> = departures
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.yesterday && d.time >= DAY)
        .map(|(i, d)| (key(d, d.time), i))
        .collect();
    let mut twin = vec![None; departures.len()];
    for (i, d) in departures.iter().enumerate().filter(|(_, d)| d.yesterday) {
        if let Some(&j) = tonight.get(&key(d, d.time + DAY)) {
            twin[i] = Some(j);
            twin[j] = Some(i);
        }
    }
    twin
}

/// Whether a departure at `time` (seconds of its day) of a trip `duration` long is on the
/// road after that day's midnight: then it is also yesterday's departure in today's early
/// hours.
fn reaches_tomorrow(time: f64, duration: f64) -> bool {
    time >= DAY || time + duration > DAY
}

#[cfg(test)]
mod midnight_tests {
    use super::reaches_tomorrow;

    #[test]
    fn trips_on_the_road_after_midnight_are_also_yesterdays() {
        // 25:10, and 21:40 for five hours, run after midnight; 22:00 for an hour does not
        assert!(reaches_tomorrow(90_600.0, 1_800.0));
        assert!(reaches_tomorrow(78_000.0, 18_000.0));
        assert!(!reaches_tomorrow(79_200.0, 3_600.0));
    }
}

/// Advance just the calendar date. The time-of-day on this copy isn't used for
/// timetable scheduling; processing intermediate days would repeatedly reload
/// assignments and can stall the main thread on a large date jump.
fn step_calendar(clock: &mut crate::SimClock, days: i64) {
    if days >= 0 {
        for _ in 0..days { clock.next_day(); }
    } else {
        for _ in 0..days.unsigned_abs() { clock.prev_day(); }
    }
}

#[cfg(test)]
mod clock_jump_tests {
    use super::step_calendar;
    use crate::SimClock;

    #[test]
    fn large_backward_jump_handles_leap_year() {
        let mut clock = SimClock::default();
        clock.set_date(2026, 10, 18);
        step_calendar(&mut clock, -594);
        assert_eq!(clock.date_code(), 20250303);
    }

    #[test]
    fn days_forward_and_back_are_inverse() {
        let mut clock = SimClock::default();
        clock.set_date(2024, 2, 29);
        step_calendar(&mut clock, 400);
        step_calendar(&mut clock, -400);
        assert_eq!(clock.date_code(), 20240229);
    }
}
