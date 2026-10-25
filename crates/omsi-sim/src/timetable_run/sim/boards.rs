//! The departure boards at the stops.

use super::*;

/// One timetable bus on the road, for outside tools (`telemetry` in `omsi-app`).
pub struct AiBusRow {
    pub id: u64,
    pub line: String,
    pub tour: String,
    pub trip: String,
    pub terminus: String,
    pub depart: f64,
    pub next_stop_id: Option<i64>,
    pub at_stop: bool,
    pub trip_done: bool,
    pub delay_s: f64,
    pub x: f64,
    pub y: f64,
    pub number: String,
}

impl ScheduleSim {
    /// The buses due at one bus stop (map object id) within the next two hours, unsorted, as
    /// (expected arrival, line, terminus, time it stands at the stop), all in seconds of the day:
    /// the timetable buses with the delay they run with, and the player's.
    pub(super) fn stop_list(
        &self,
        stop: i64,
        now: f64,
        on_road: &HashMap<usize, OnRoad>,
        duty: Option<&PlayerDuty>,
    ) -> Vec<(f64, String, String, f64, StopExtra)> {
        let mut list: Vec<(f64, String, String, f64, StopExtra)> = Vec::new();
        // the last time each line leaves this stop today (for "last bus")
        let mut last_of_line: HashMap<String, f64> = HashMap::new();
        for &(trip, k) in self.visits.get(&stop).map(|v| v.as_slice()).unwrap_or(&[]) {
            for &i in &self.trip_departures[trip] {
                let d = &self.departures[i];
                let tt = &self.times[d.trip][d.profile];
                if self.runs(i) && tt.stops[k] {
                    let leave = d.time + tt.stations[k].1;
                    let e = last_of_line.entry(self.display_line(i)).or_insert(f64::MIN);
                    *e = e.max(leave);
                }
            }
        }
        for &(trip, k) in self.visits.get(&stop).map(|v| v.as_slice()).unwrap_or(&[]) {
            for &i in &self.trip_departures[trip] {
                if !self.runs(i) {
                    continue;
                }
                let d = &self.departures[i];
                let tt = &self.times[d.trip][d.profile];
                if !tt.stops[k] {
                    continue;
                }
                let (arrive, leave) = (d.time + tt.stations[k].0, d.time + tt.stations[k].1);
                if arrive > now + BOARD_AHEAD || leave < now - BOARD_AHEAD {
                    continue;
                }
                if self.is_player_tour(i) {
                    continue;
                }
                let mut extra = StopExtra::default();
                let expected = match on_road.get(&i) {
                    Some(r) => match r.next {
                        // the stations before the bus's next stop are behind it
                        Some(next) if leave < next - 0.5 => continue,
                        None => continue,
                        // standing at this stop
                        Some(next) if r.dwelling && (leave - next).abs() < 0.5 => {
                            extra.stops_away = Some(0);
                            extra.delay = Some(r.late);
                            extra.load = r.load;
                            now
                        }
                        Some(next) => {
                            // its next stop's place in the trip: the stops from there up to
                            // this one are still to come (the one it stands at is not)
                            if let Some(j) = tt.stations.iter().position(|s| (d.time + s.1 - next).abs() < 0.5) {
                                let from = if r.dwelling { j + 1 } else { j };
                                extra.stops_away = Some((from..=k).filter(|&x| tt.stops.get(x).copied().unwrap_or(false)).count() as u32);
                            }
                            extra.delay = Some(r.late);
                            extra.load = r.load;
                            // on its way: at least as late as it left its last stop, and
                            // later still once its next stop is overdue
                            let next_arrive = tt
                                .stations
                                .iter()
                                .find(|s| (d.time + s.1 - next).abs() < 0.5)
                                .map(|s| d.time + s.0)
                                .unwrap_or(next);
                            let late = if r.dwelling {
                                r.late
                            } else {
                                r.late.max(now - next_arrive)
                            };
                            (arrive + late).max(now)
                        }
                    },
                    // not on the road (still to come, or where no tiles are loaded): on time
                    None if leave < now => continue,
                    None => arrive.max(now),
                };
                // Departure boards use the timetable destination; HOF strings carry
                // vehicle-specific padding, multi-line text and texture names.
                let terminus = &self.data.trips[d.trip].terminus;
                let line = self.display_line(i);
                extra.last_trip = last_of_line.get(&line).is_some_and(|&l| leave >= l - 0.5);
                list.push((expected, line, terminus.clone(), (leave - arrive).max(0.0), extra));
            }
        }
        // the player's bus
        if let Some(duty) = duty {
            // (a duty's times are its own day's: a night tour's 25:05 is 1:05 on the clock. The
            // board is worked out in the duty's time and its arrivals handed back in the
            // clock's - compared with the clock's 0:50, the player's next trip looked 24
            // hours away and the bus was missing from every board after midnight)
            let duty_ahead = duty.duty_time(now) - now;
            let now = now + duty_ahead;
            // a trip not begun leaves on time at the earliest (the bus waits at its
            // first stop), one under way arrives as early or late as it runs
            let delay = duty.delay(now);
            let lateness = if duty.left_late().is_some() || duty.at_stop() {
                delay
            } else {
                delay.max(0.0)
            };
            for (ti, trip) in duty.trips.iter().enumerate().skip(duty.trip_index) {
                if trip.departure > now + BOARD_AHEAD {
                    break;
                }
                for (k, s) in trip
                    .stops
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.object_id == stop && s.stops)
                {
                    let current = ti == duty.trip_index;
                    let expected = if current
                        && (k < duty.next_stop || duty.trip_done() && k + 1 < trip.stops.len())
                    {
                        continue;
                    } else if current && k == duty.next_stop && duty.at_stop() {
                        now
                    } else if current {
                        (s.arr + lateness).max(now)
                    } else {
                        (s.arr + delay.max(0.0)).max(now)
                    };
                    let mut extra = StopExtra::default();
                    if current {
                        let from = if duty.at_stop { duty.next_stop + 1 } else { duty.next_stop };
                        let away = (from..=k).filter(|&x| trip.stops.get(x).is_some_and(|s| s.stops)).count() as u32;
                        extra.stops_away = Some(if duty.at_stop && k == duty.next_stop { 0 } else { away });
                        extra.delay = Some(delay);
                    }
                    list.push((
                        expected - duty_ahead,
                        trip.line.trim().to_string(),
                        trip.terminus.clone(),
                        (s.dep - s.arr).max(0.0),
                        extra,
                    ));
                }
            }
        }
        list
    }

    /// Whether the departure boards were made less than a second before `now` (omsi-app's
    /// `Schedule::update_boards` makes them at most once a second).
    pub fn boards_fresh(&self, now: f64) -> bool {
        (now - self.boards_made).abs() < 1.0
    }

    /// The departure boards of the stops `wanted` (by map object id): the buses due at each
    /// stop in the next two hours, soonest first - the timetable buses with the delay they
    /// run with, and the player's - as (line, terminus, expected arrival); and the
    /// departures the pages asked for by the stop names `wanted_names`, as (line,
    /// destination, timestamp). The time they are made is kept (`boards_fresh`).
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub fn make_boards(
        &mut self,
        traffic: Option<&TrafficSim>,
        wanted: Vec<i64>,
        wanted_names: Vec<String>,
        duty: Option<&PlayerDuty>,
        clock: &crate::SimClock,
    ) -> (HashMap<i64, Vec<(String, String, f64)>>, std::collections::HashMap<String, Vec<crate::vehicle_api::Departure>>) {
        let now = clock.time;
        self.boards_made = now;
        // the timetable buses on the road: departure -> where they are in their trip (a
        // train runs its track without stops of its own and is taken as on time)
        let mut on_road: HashMap<usize, OnRoad> = HashMap::new();
        if let Some(t) = traffic {
            for car in &t.cars {
                let Some(&i) = self.car_departure.get(&car.id) else {
                    continue;
                };
                if trip_stations(&self.data.trips[self.departures[i].trip]).is_empty() {
                    continue;
                }
                let next = car.bus.as_ref().and_then(|b| b.stops.front()).map(|s| s.depart).or_else(|| {
                    self.running.iter().find(|r| r.car == car.id).and_then(|r| {
                        r.stations
                            .iter()
                            .zip(&r.served)
                            .find(|(_, s)| !**s)
                            .map(|(st, _)| st.1)
                    })
                })
                // (the bus's times are on the traffic's clock, which counts on past 24:00)
                .map(|t| t - self.day_base);
                on_road.insert(
                    i,
                    OnRoad {
                        next,
                        dwelling: car.at_stop(),
                        late: car.bus.as_ref().map(|b| b.delay).unwrap_or(0.0).max(0.0),
                        load: t.bus_loads.get(&car.id).copied(),
                    },
                );
            }
        }
        let mut made = HashMap::new();
        for stop in wanted {
            let mut list = self.stop_list(stop, now, &on_road, duty);
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
            list.truncate(8);
            if omsi_cfg::flags::OMSI_DEBUG_BOARDS.is_set() {
                log::info!(
                    "board of stop {stop} at {:.0} s: {:?}",
                    now,
                    list.iter()
                        .map(|(t, l, d, _, _)| format!("{l} {d} in {:.1} min", (t - now) / 60.0))
                        .collect::<Vec<_>>()
                );
            }
            made.insert(stop, list.into_iter().map(|(t, l, d, _, _)| (l, d, t)).collect());
        }
        // the departures the pages asked for by stop name (`omsi.getDepartures`): the next two
        // hours, at most 20, as (line, destination, timestamp)
        let mut departures = std::collections::HashMap::new();
        for key in wanted_names {
            let mut ids: Vec<i64> = self
                .data
                .bus_stops
                .iter()
                .filter(|b| b.name.trim().eq_ignore_ascii_case(&key))
                .map(|b| b.object_id)
                .collect();
            // (a map without Busstops.cfg - the older TTData, NCCR's - names its stops only
            // in the trip files: look there too)
            if ids.is_empty() {
                for trip in &self.data.trips {
                    for (i, &id) in trip_stations(trip).iter().enumerate() {
                        if self.station_name(trip, i, id).trim().eq_ignore_ascii_case(&key) {
                            ids.push(id);
                        }
                    }
                }
            }
            ids.sort_unstable();
            ids.dedup();
            if omsi_cfg::flags::OMSI_DEBUG_BOARDS.is_set() {
                log::info!("page departures of '{key}': stop objects {ids:?}");
            }
            let mut list: Vec<(f64, String, String, StopExtra)> = Vec::new();
            for id in ids {
                for (expected, line, terminus, dwell, extra) in self.stop_list(id, now, &on_road, duty) {
                    let leaves = expected + dwell;
                    if leaves <= now + BOARD_AHEAD {
                        list.push((leaves, line, terminus, extra));
                    }
                }
            }
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
            list.truncate(MAX_PAGE_DEPARTURES);
            if omsi_cfg::flags::OMSI_DEBUG_BOARDS.is_set() {
                log::info!("page departures of '{key}': {} departures", list.len());
            }
            departures.insert(
                key,
                list.into_iter()
                    .map(|(t, l, d, x)| crate::vehicle_api::Departure {
                        line: l,
                        destination: d,
                        time: crate::vehicle_api::timestamp(clock, t),
                        stops_away: x.stops_away,
                        load: x.load,
                        delay: x.delay,
                        last_trip: x.last_trip,
                    })
                    .collect(),
            );
        }
        (made, departures)
    }

    /// The line a departure's displays show: its trip's own (" 5"), else the timetable
    /// line's name.
    /// The name of station `i` (object `id`) of `trip`, as the map's stop calls it.
    pub(super) fn station_name(&self, trip: &omsi_timetable::Trip, i: usize, id: i64) -> String {
        self.data
            .bus_stops
            .iter()
            .find(|b| b.object_id == id)
            .map(|b| b.name.clone())
            .filter(|n| !n.trim().is_empty())
            // (the trip file names its stations too: a stop whose object is not
            // among the map's known stops - Novi Sad's, on tiles not loaded yet -
            // had no name on the navigator, the HUD and in the log)
            .or_else(|| {
                trip.stations
                    .is_empty()
                    .then(|| {
                        trip.stations_legacy
                            .get(i)
                            .and_then(|r| r.get(2))
                            .map(|n| n.trim().to_string())
                    })
                    .flatten()
            })
            .unwrap_or_default()
    }

    /// The station names of trip `ti`, for picking its route in the depot file.
    pub fn trip_stop_names(&self, ti: usize) -> Vec<String> {
        let trip = &self.data.trips[ti];
        trip_stations(trip)
            .iter()
            .enumerate()
            .map(|(i, id)| self.station_name(trip, i, *id))
            .collect()
    }

    /// The timetable buses on the road (those a departure drives), for outside tools.
    pub fn ai_bus_rows(&self, traffic: &TrafficSim) -> Vec<AiBusRow> {
        let mut out = Vec::new();
        for car in &traffic.cars {
            let Some(&i) = self.car_departure.get(&car.id) else { continue };
            let Some(b) = car.bus.as_ref() else { continue };
            let d = &self.departures[i];
            let trip = &self.data.trips[d.trip];
            let pos = car.vehicle.position;
            out.push(AiBusRow {
                id: car.id,
                line: self.display_line(i),
                tour: d.tour.to_string(),
                trip: trip.name.to_string(),
                terminus: b.terminus.trim().to_string(),
                depart: d.time,
                next_stop_id: b.stops.front().map(|s| s.id),
                at_stop: car.at_stop(),
                trip_done: car.trip_done(),
                delay_s: b.delay,
                x: pos.x,
                y: pos.y,
                number: car.vehicle.str_var("number").trim().to_string(),
            });
        }
        out
    }

    /// The line an AI bus and the departure boards show for departure `i`: its trip's own,
    /// blank when the trip has none, as Omsi.exe shows it (Gerolstein's service trips: a
    /// blank rear matrix and line column). The timetable line's name stood in for it once,
    /// and the service trips read "Diensttafeln".
    pub fn display_line(&self, i: usize) -> String {
        self.data.trips[self.departures[i].trip].line.trim().to_string()
    }
}

/// Where a timetable bus on the road is in its trip, for the departure boards.
pub(super) struct OnRoad {
    /// Timetable departure time of its next stop (None: it has served its last).
    next: Option<f64>,
    /// Standing at that stop.
    dwelling: bool,
    /// How late it left its last stop (s).
    late: f64,
    /// How full it is (0..1), when its passengers are simulated.
    load: Option<f32>,
}

/// What a departure display adds to a bus's time (`omsi.getDepartures`).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct StopExtra {
    /// Stops still to call at up to this one, this one counted (0: standing here).
    stops_away: Option<u32>,
    load: Option<f32>,
    /// How late it runs (s), on the road.
    delay: Option<f64>,
    /// The last departure of its line at this stop today.
    last_trip: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyWorld {
        path: PathBuf,
        ai: omsi_map::AiLists,
        positions: crate::world_data::ObjectPlaces,
    }

    impl TimetableWorld for EmptyWorld {
        fn root(&self) -> &Path { &self.path }
        fn map_dir(&self) -> &Path { &self.path }
        fn chrono_dirs(&self) -> Vec<PathBuf> { Vec::new() }
        fn ailists(&self) -> &omsi_map::AiLists { &self.ai }
        fn raw_tiles(&self) -> Vec<(i32, i32)> { Vec::new() }
        fn has_tile(&self, _: (i32, i32)) -> bool { false }
        fn date(&self) -> i32 { 20221010 }
        fn object_positions(&self) -> &crate::world_data::ObjectPlaces { &self.positions }
        fn stop_side(&self, _: i64) -> f32 { 0.0 }
    }

    fn schedule() -> ScheduleSim {
        let world = EmptyWorld {
            path: std::env::temp_dir().join(format!("omsi_empty_board_world_{}", std::process::id())),
            ai: Default::default(),
            positions: Default::default(),
        };
        ScheduleSim::new(&world.path, &world, &crate::SimClock::default())
    }

    #[test]
    fn same_number_tours_use_the_variant_operating_today() {
        let mut sim = schedule();
        let world = EmptyWorld {
            path: PathBuf::new(),
            ai: Default::default(),
            positions: Default::default(),
        };
        sim.data.trips = ["MWF", "TuTh"]
            .into_iter()
            .map(|name| omsi_timetable::Trip {
                name: name.into(),
                line: "5".into(),
                stations: vec![42],
                ..Default::default()
            })
            .collect();
        sim.times = vec![
            vec![TripTimes {
                stations: vec![(0.0, 0.0)],
                stops: vec![true],
                duration: 600.0,
                ..Default::default()
            }];
            2
        ];
        sim.data.lines = vec![omsi_timetable::Line {
            name: "5".into(),
            tours: [("MWF", 789), ("TuTh", 778)]
                .into_iter()
                .map(|(trip, mask)| omsi_timetable::Tour {
                    number: "1".into(),
                    extra: mask.to_string(),
                    trips: vec![omsi_timetable::TourTrip {
                        trip: trip.into(),
                        profile: 0,
                        departure: 360.0,
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }];
        for (day, expected) in [(1, "MWF"), (2, "TuTh")] {
            sim.day_bits = (day, 256);
            let duty = sim.player_duty(&world, "5", "1", 0.0, None, true).unwrap();
            assert_eq!(duty.trips[0].name, expected);
            assert_eq!(duty.tour, "1");
        }
        // The stop preview follows the same variant as the launched duty.
        sim.data.lines[0].tours[1].trips[0].departure = 420.0;
        assert_eq!(sim.tour_stops("5", "1")[0].3, 420.0 * 60.0);
    }

    /// A trip without a line of its own shows none, as in Omsi.exe: Gerolstein's service
    /// trips in `Diensttafeln.ttl` read "Diensttafeln" on the boards and the AI buses.
    #[test]
    fn a_trip_without_a_line_shows_none() {
        let mut sim = schedule();
        sim.data.trips.push(omsi_timetable::Trip { terminus: "Betriebsfahrt".into(), stations: vec![42], ..Default::default() });
        sim.departures.push(Departure {
            time: 100.0, trip: 0, profile: 0, line: "Diensttafeln".into(), ai_group: "depot".into(),
            tour: "101 Mo-Do".into(), mask: -1, spawned: false, yesterday: false,
        });
        sim.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0)], stops: vec![true], ..Default::default() }]];
        sim.visits.insert(42, vec![(0, 0)]);
        sim.trip_departures = vec![vec![0]];
        assert_eq!(sim.display_line(0), "");
        let clock = crate::SimClock { time: 0.0, ..Default::default() };
        let (boards, _) = sim.make_boards(None, vec![42], vec!["markt".into()], None, &clock);
        assert_eq!(boards[&42].iter().map(|r| (r.0.as_str(), r.1.as_str())).collect::<Vec<_>>(), [("", "Betriebsfahrt")]);
    }

    #[test]
    fn departure_boards_keep_timetable_text_for_ai_players_and_pages() {
        let mut sim = schedule();
        let destination = "U Marktstraße";
        sim.data.trips.push(omsi_timetable::Trip {
            terminus: destination.into(), line: "7".into(), stations: vec![42], ..Default::default()
        });
        sim.departures.push(Departure {
            time: 100.0, trip: 0, profile: 0, line: "7".into(), ai_group: "depot".into(),
            tour: "1".into(), mask: -1, spawned: false, yesterday: false,
        });
        sim.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0)], stops: vec![true], ..Default::default() }]];
        sim.visits.insert(42, vec![(0, 0)]);
        sim.trip_departures = vec![vec![0]];
        sim.data.bus_stops.push(omsi_timetable::BusStopEntry { name: "Markt".into(), object_id: 42, ..Default::default() });
        let hof = omsi_vehicle::Hof::parse(&omsi_cfg::CfgFile::from_str("test.hof", concat!(
            "stringcount_terminus\n3\n[addterminus]\n10\nU Marktstraße\n",
            "     U Marktstraße   über Bahnhof\n\nMATRIX\\target.bmp\n",
        )));
        let bus = crate::timetable_run::tests::ibis_test_vehicle();
        sim.depots.insert("depot".into(), vec![(bus.ty.clone(), Vec::new(), Some(Arc::new(hof)))]);
        let mut trip = crate::timetable_run::tests::planned(200.0, &[(0.0, 200.0, 200.0)]);
        trip.line = "8".into();
        trip.terminus = destination.into();
        trip.stops[0].object_id = 42;
        let duty = PlayerDuty::new("8".into(), "2".into(), vec![trip], 0, 0, true);
        let clock = crate::SimClock { time: 0.0, ..Default::default() };
        let (boards, pages) = sim.make_boards(None, vec![42], vec!["markt".into()], Some(&duty), &clock);
        assert_eq!(boards[&42].iter().map(|r| (r.0.as_str(), r.1.as_str())).collect::<Vec<_>>(), [("7", destination), ("8", destination)]);
        assert_eq!(pages["markt"].iter().map(|r| r.destination.as_str()).collect::<Vec<_>>(), [destination, destination]);
        assert_eq!(sim.depots["depot"][0].2.as_ref().unwrap().termini[0].strings[0], "     U Marktstraße   über Bahnhof");
    }

    /// The player's night tour after midnight: its 25:05 trip is on the board at 0:50 and
    /// shows 1:05 there; at 21:00 the same trip is hours off and on no board.
    #[test]
    fn the_players_night_trip_shows_on_the_board_in_clock_time() {
        let sim = schedule();
        let mut trip = crate::timetable_run::tests::planned(90_300.0, &[(0.0, 90_300.0, 90_300.0)]);
        trip.line = "N7".into();
        trip.stops[0].object_id = 9;
        let duty = PlayerDuty::new("N7".into(), "3".into(), vec![trip], 0, 0, true);
        let after_midnight = sim.stop_list(9, 3_000.0, &HashMap::new(), Some(&duty));
        assert_eq!(after_midnight.iter().map(|r| (r.0.round(), r.1.as_str())).collect::<Vec<_>>(), [(3_900.0, "N7")]);
        assert!(sim.stop_list(9, 75_600.0, &HashMap::new(), Some(&duty)).is_empty());
    }

    /// A night tour of today (23:00, 24:30) and the same tour's 24:30 as yesterday's, at
    /// 00:30 (#2166).
    fn night_tour(sim: &mut ScheduleSim) {
        sim.data.trips.push(omsi_timetable::Trip { terminus: "Bahnhof".into(), line: "N1".into(), stations: vec![42], ..Default::default() });
        sim.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0)], stops: vec![true], duration: 600.0, ..Default::default() }]];
        let mask = sim.day_bits.0 | sim.day_bits.1;
        let d = |time: f64, yesterday: bool| Departure {
            time, trip: 0, profile: 0, line: "N1".into(), ai_group: "depot".into(), tour: "1".into(), mask, spawned: false, yesterday,
        };
        sim.departures = vec![d(1800.0, true), d(82_800.0, false), d(88_200.0, false)];
        sim.night_twin = super::load::night_twins(&sim.departures);
        sim.tour_next = vec![None, Some(2), None];
        sim.tour_prev = vec![None, None, Some(1)];
    }

    #[test]
    fn a_night_trip_after_midnight_runs_as_yesterdays_tour() {
        let mut sim = schedule();
        night_tour(&mut sim);
        assert_eq!(sim.night_twin, [Some(2), None, Some(0)]);
        // today the tour runs from 23:00 into the night; this morning's 00:30 was yesterday's
        assert!(sim.runs(1) && sim.runs(2));
        assert!(!sim.runs(0), "yesterday's tour does not run on yesterday's day mask");
        // midnight: the day moves on, and the 24:30 comes at 00:30 as yesterday's
        sim.roll_day(DAY + 10.0);
        assert!(sim.runs(0), "after midnight the night trip is yesterday's tour's");
        assert!(!sim.departures[0].spawned);
        let tod = 1800.0 + 1.0;
        assert!(sim.departures[0].time <= tod && sim.departures[0].time > tod - 5.0, "due at 00:30");
        assert_eq!(sim.dep_time(0), DAY + 1800.0);
    }

    #[test]
    fn the_tour_bus_on_a_night_trip_at_midnight_keeps_it_as_yesterdays() {
        let mut sim = schedule();
        night_tour(&mut sim);
        // 23:55: the tour's bus will take the 24:30 on after its 23:00 trip
        sim.departures[2].spawned = true;
        sim.awaiting.insert(2);
        sim.car_departure.insert(7, 1);
        sim.roll_day(DAY + 10.0);
        assert!(sim.awaiting.contains(&0) && !sim.awaiting.contains(&2));
        assert!(sim.departures[0].spawned, "no second bus for the trip its tour's bus takes on");
        assert!(!sim.departures[2].spawned, "tonight's 24:30 runs again");
        assert_eq!(sim.dep_time(0), DAY + 1800.0, "the same moment as before midnight");
    }

    fn no_traffic() -> crate::ai_traffic::TrafficSim {
        crate::ai_traffic::TrafficSim::assemble(
            Path::new(""),
            Default::default(),
            crate::ai_traffic::setup::RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() },
            Vec::new(),
            Default::default(),
            (Vec::new(), Vec::new()),
            Vec::new(),
            (1.0, 0),
            0,
        )
    }

    fn no_world() -> EmptyWorld {
        EmptyWorld { path: std::env::temp_dir(), ai: Default::default(), positions: Default::default() }
    }

    /// One trip of two stations, 10 minutes long, and departures (time, tour, previous of
    /// the tour) all running today and not yet out.
    fn tours(sim: &mut ScheduleSim, deps: &[(f64, &str, Option<usize>)]) {
        sim.data.trips.push(omsi_timetable::Trip { terminus: "Bahnhof".into(), line: "1".into(), stations: vec![42, 43], ..Default::default() });
        sim.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0), (600.0, 600.0)], stops: vec![true, true], duration: 600.0, ..Default::default() }]];
        let mask = sim.day_bits.0 | sim.day_bits.1;
        sim.departures = deps
            .iter()
            .map(|&(time, tour, _)| Departure {
                time, trip: 0, profile: 0, line: "1".into(), ai_group: "depot".into(), tour: tour.into(), mask, spawned: false, yesterday: false,
            })
            .collect();
        sim.tour_prev = deps.iter().map(|d| d.2).collect();
        sim.tour_next = vec![None; deps.len()];
        for (i, d) in deps.iter().enumerate() {
            if let Some(k) = d.2 {
                sim.tour_next[k] = Some(i);
            }
        }
    }

    /// At the start every trip still under way comes out, however long ago it left (a bus
    /// that stands on a detector from 20:00 to 04:00, the map opened at 23:30); once the
    /// map is running, only the departures of the moment do, and a trip already over never.
    #[test]
    fn the_trips_under_way_at_the_start_all_come_out() {
        let mut sim = schedule();
        let (world, mut traffic) = (no_world(), no_traffic());
        tours(&mut sim, &[(72_000.0, "1", None), (82_000.0, "2", None), (84_300.0, "3", None)]);
        sim.times[0][0].duration = 8.0 * 3600.0;
        sim.times.push(vec![TripTimes { stations: vec![(0.0, 0.0), (60.0, 60.0)], stops: vec![true, true], duration: 60.0, ..Default::default() }]);
        sim.departures[1].trip = 1;
        sim.queue_due(&world, &mut traffic, 84_600.0, 20.0 * 60.0);
        assert_eq!(sim.pending.iter().copied().collect::<Vec<_>>(), [0, 2], "20:00's long trip and 23:25's, not 22:46's one-minute trip, over long ago");
        let mut later = schedule();
        tours(&mut later, &[(72_000.0, "1", None)]);
        later.times[0][0].duration = 8.0 * 3600.0;
        later.queue_due(&world, &mut traffic, 84_600.0, 2.5);
        assert!(later.pending.is_empty(), "a running map does not put out a trip begun hours ago");
    }

    /// Opened in the small hours, the map gets yesterday's trip still on its long way since
    /// 21:40 as well as the ones of the last minutes.
    #[test]
    fn a_trip_from_last_evening_comes_out_at_a_start_after_midnight() {
        let mut sim = schedule();
        let (world, mut traffic) = (no_world(), no_traffic());
        tours(&mut sim, &[(78_000.0 - 86_400.0, "4", None), (6_900.0, "5", None)]);
        sim.times[0][0].duration = 5.0 * 3600.0;
        sim.departures[0].yesterday = true;
        // `tours` uses today's calendar mask, but this departure belongs to yesterday.
        sim.departures[0].mask = sim.yesterday_bits.0 | sim.yesterday_bits.1;
        assert!(sim.runs(0), "the fixture's night tour must run on yesterday's calendar");
        sim.queue_due(&world, &mut traffic, 7_200.0, 20.0 * 60.0);
        assert_eq!(sim.pending.iter().copied().collect::<Vec<_>>(), [0, 1]);
    }

    /// A tour's bus is out only while its tour runs: the second trip of a tour whose bus is
    /// missing stands at its first stop well before it leaves (#2593), the first trip of a
    /// tour comes at its departure time and not before (#2595).
    #[test]
    fn only_a_tour_under_way_has_its_bus_before_a_departure() {
        let mut sim = schedule();
        let (world, mut traffic) = (no_world(), no_traffic());
        // tour 1 at 10:00 and 10:35 (its first trip over at 10:10), tour 2 from 10:30
        tours(&mut sim, &[(36_000.0, "1", None), (37_800.0, "2", None), (38_100.0, "1", Some(0))]);
        sim.departures[0].spawned = true;
        // 10:25: tour 1's bus stands for its 10:35 trip, tour 2 has no bus yet
        sim.queue_due(&world, &mut traffic, 36_000.0 + 1500.0, 1.0);
        assert_eq!(sim.pending.iter().copied().collect::<Vec<_>>(), [2]);
        // 10:30: tour 2 leaves now
        sim.queue_due(&world, &mut traffic, 37_800.0 + 0.5, 1.0);
        assert_eq!(sim.pending.iter().copied().collect::<Vec<_>>(), [2, 1]);
    }

    /// A previous trip of the tour that does not run today is no trip before this one: the
    /// trip is its tour's first and comes at its time. And after a break longer than a tour's
    /// bus waits out, no bus stands there early either (the one that drove has gone).
    #[test]
    fn no_early_bus_after_a_trip_of_another_day_or_a_long_break() {
        let mut sim = schedule();
        let (world, mut traffic) = (no_world(), no_traffic());
        // 10:00 (another day's), 10:30 after it; 11:00, then 11:41 after a 31-minute break
        tours(&mut sim, &[(36_000.0, "1", None), (37_800.0, "1", Some(0)), (39_600.0, "2", None), (42_060.0, "2", Some(2))]);
        sim.departures[0].mask = 0;
        sim.departures[2].spawned = true;
        sim.queue_due(&world, &mut traffic, 37_800.0 - 600.0, 1.0);
        assert!(sim.pending.is_empty(), "{:?}", sim.pending);
        sim.queue_due(&world, &mut traffic, 42_060.0 - 300.0, 1.0);
        assert!(sim.pending.is_empty(), "{:?}", sim.pending);
        // a 29-minute break: it stands there
        sim.departures[3].time = 41_940.0;
        sim.queue_due(&world, &mut traffic, 41_940.0 - 300.0, 1.0);
        assert_eq!(sim.pending.iter().copied().collect::<Vec<_>>(), [3]);
    }

    /// A layover bus whose stand was taken tries again in a while, and when it comes at its
    /// departure time it is no longer kept as one: at midnight the trip is made ready to run
    /// again (it had stopped running after the first midnight).
    #[test]
    fn a_stand_taken_departure_retries_and_runs_again_the_next_day() {
        let mut sim = schedule();
        let (world, mut traffic) = (no_world(), no_traffic());
        tours(&mut sim, &[(36_000.0, "1", None), (37_800.0, "1", Some(0))]);
        sim.departures[0].spawned = true;
        let t = 37_800.0 - 900.0;
        sim.queue_due(&world, &mut traffic, t, 1.0);
        assert_eq!(sim.next_pending(), Some(1));
        sim.stand_taken(1, t);
        sim.queue_due(&world, &mut traffic, t + 5.0, 1.0);
        assert!(sim.pending.is_empty(), "not again at once");
        sim.queue_due(&world, &mut traffic, t + 30.0, 1.0);
        assert_eq!(sim.next_pending(), Some(1), "tried again");
        sim.stand_taken(1, t + 30.0);
        // at its departure time it comes, and is no longer a layover departure
        sim.queue_due(&world, &mut traffic, 37_800.0 + 0.5, 1.0);
        assert_eq!(sim.next_pending(), Some(1));
        assert!(sim.later_layover.is_empty());
        let mut next = sim.date_clock.clone();
        next.day_of_year += 1;
        sim.set_day(&next);
        assert!(!sim.departures[1].spawned, "made ready to run again");
    }

    /// A trip that runs in from the map's edge to its first stop is not put out early on a
    /// layover at the start of its track: it comes at its departure time.
    #[test]
    fn a_trip_from_the_edge_is_not_put_out_early() {
        let mut sim = schedule();
        let (world, mut traffic) = (no_world(), no_traffic());
        // a tour at 14:00 and 14:20: its second trip reaches its first stop 4 minutes in
        tours(&mut sim, &[(50_400.0, "7", None), (51_600.0, "7", Some(0))]);
        sim.times[0][0].stations[0] = (240.0, 270.0);
        sim.departures[0].spawned = true;
        sim.queue_due(&world, &mut traffic, 51_600.0 - 420.0, 1.0);
        assert!(sim.pending.is_empty(), "put out early: {:?}", sim.pending);
        sim.queue_due(&world, &mut traffic, 51_600.0 + 0.5, 1.0);
        assert_eq!(sim.pending.iter().copied().collect::<Vec<_>>(), [1]);
    }

}
