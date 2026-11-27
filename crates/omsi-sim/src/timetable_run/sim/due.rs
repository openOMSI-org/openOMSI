//! The trips due at the stops about now, for the people turning up there (Omsi.exe's station
//! manager, 0x61a068), and where the tours' buses are, which takes a trip off a stop once its
//! bus has passed there.

use super::*;

impl ScheduleSim {
    /// [`ScheduleSim::due_trips`] with no bus known to have passed anywhere.
    pub fn due_destinations(&self, day_time: f64) -> DueTrips {
        self.due_trips(day_time, None, None)
    }

    /// Per bus stop, the trips due there from [`PAX_LATE_WAIT`] ago to [`PAX_SPAWN_AHEAD`]
    /// ahead (today's departures): their calls there, whose [`pax_share`]s say how many of
    /// the stop's people turn up as long as their bus has not passed it (0x61a068), and the
    /// destinations the trips not passed yet go on to, which the people turning up there now
    /// draw theirs from. Omsi.exe
    /// spawns people for a trip up to a quarter of an hour before it is due at their stop
    /// (pionsix's tests, #1436); the destinations of every trip of the map, whatever its hour
    /// or day, had people waiting at 2 a.m. for the six o'clock bus and at a school's stop
    /// for hours before its run (#1415). Which buses they then take is still the stop's line
    /// records (`stop_targets`), from every trip. Where the buses are comes from the
    /// timetable buses on the road (`traffic`) and the player's `duty` ([`Self::tour_buses`]).
    pub fn due_trips(
        &self,
        day_time: f64,
        traffic: Option<&TrafficSim>,
        duty: Option<&PlayerDuty>,
    ) -> DueTrips {
        self.due_with(day_time, self.tour_buses(traffic, duty))
    }

    pub(super) fn due_with(&self, day_time: f64, buses: TourBuses) -> DueTrips {
        let names = self.stop_names();
        let name_of = |id: i64| names.get(&id).cloned().unwrap_or_else(|| id.to_string());
        let tod = day_time - self.day_base;
        let longest = self
            .times
            .iter()
            .flatten()
            .map(|t| t.duration)
            .fold(0.0, f64::max);
        let from = self
            .departures
            .partition_point(|d| d.time < tod - longest - PAX_LATE_WAIT);
        let mut out = DueTrips::default();
        for i in from..self.departures.len() {
            let d = &self.departures[i];
            if d.time > tod + PAX_SPAWN_AHEAD {
                break;
            }
            if !self.runs(i) {
                continue;
            }
            let tt = self.times_of(i);
            let tour = self.bus_key(i);
            let stations = trip_stations(&self.data.trips[d.trip]);
            for (k, sid) in stations.iter().enumerate() {
                let due = d.time + tt.stations.get(k).map(|s| s.0).unwrap_or(0.0);
                if !pax_turn_up_for(due, tod) || !tt.stops.get(k).copied().unwrap_or(true) {
                    continue;
                }
                let here = name_of(*sid);
                let goes_on: HashSet<String> = stations
                    .iter()
                    .enumerate()
                    .skip(k + 1)
                    .filter(|(j, _)| tt.stops.get(*j).copied().unwrap_or(true))
                    .map(|(_, to)| name_of(*to))
                    .filter(|to| *to != here)
                    .collect();
                let visit = DueVisit {
                    tour,
                    departure: i,
                    station: k,
                    share: pax_share(due - tod),
                };
                // (a trip that ends here takes nobody on. One whose bus has been here already
                // brings nobody any more, 0x61a068: its call is kept, for `share_of` to weigh
                // against where the buses are as they move on, but not its destinations)
                let set = out.dests.entry(*sid).or_default();
                if goes_on.is_empty() {
                    continue;
                }
                if !buses.passed(&visit) {
                    set.extend(goes_on);
                }
                out.visits.entry(*sid).or_default().push(visit);
            }
        }
        out.buses = buses;
        out
    }

    /// Where the tours' buses are now ([`TourBuses`]): the timetable buses on the road
    /// (`traffic`), each on the departure it runs, and the player's bus on the trip of the
    /// `duty` (its next stop is the one it makes for, or stands at). Cheap enough for every
    /// frame: Omsi.exe asks the tours' vehicles anew each frame a stop is in range.
    pub fn tour_buses(&self, traffic: Option<&TrafficSim>, duty: Option<&PlayerDuty>) -> TourBuses {
        let mut out = TourBuses::default();
        for car in traffic.map(|t| t.cars.as_slice()).unwrap_or(&[]) {
            let Some(&i) = self.car_departure.get(&car.id) else {
                continue;
            };
            let Some(bus) = car.bus.as_ref() else {
                continue;
            };
            // the departure time of the stop it makes for (or stands at); none left: the
            // trip is behind it (a bus at its end, or let go)
            let next = if car.gone || bus.trip_done() {
                None
            } else {
                bus.stops.front().map(|s| s.depart).or_else(|| {
                    self.running.iter().find(|r| r.car == car.id).and_then(|r| {
                        r.stations
                            .iter()
                            .zip(&r.served)
                            .find(|(_, s)| !**s)
                            .map(|(st, _)| st.1)
                    })
                })
            };
            out.insert(self.bus_key(i), i, self.station_left_at(i, next));
        }
        if let Some(duty) = duty {
            self.duty_bus(duty, &mut out);
        }
        for (&tour, &(departure, next)) in &self.lan_progress.0 {
            out.insert(tour, departure, next);
        }
        out
    }

    /// LAN play (host): the bus of the player driving tour `tour` of line `line` has stood at
    /// stop `stop` and left it. Its trip - of the calls of that tour at the stop about now,
    /// the one due nearest - asks for nobody there or before any more, as an AI bus's does
    /// ([`TourBuses`]). Without this the host knew nothing of how far a player had got, and
    /// the stops they had served filled again for [`PAX_LATE_WAIT`].
    pub fn lan_bus_left(&mut self, line: &str, tour: &str, stop: i64) {
        let tod = self.last_tod;
        let mut best: Option<(f64, usize, usize)> = None;
        for (i, d) in self.departures.iter().enumerate() {
            if !d.line.eq_ignore_ascii_case(line)
                || !d.tour.eq_ignore_ascii_case(tour)
                || !self.runs(i)
            {
                continue;
            }
            let tt = self.times_of(i);
            for (k, sid) in trip_stations(&self.data.trips[d.trip]).iter().enumerate() {
                let due = d.time + tt.stations.get(k).map(|s| s.0).unwrap_or(0.0);
                let off = (due - tod).abs();
                if *sid == stop && pax_turn_up_for(due, tod) && best.is_none_or(|b| off < b.0) {
                    best = Some((off, i, k));
                }
            }
        }
        if let Some((_, i, k)) = best {
            let key = self.bus_key(i);
            self.lan_progress.insert(key, i, k + 1);
        }
    }

    /// Which bus drives departure `i` ([`DueVisit::tour`]): its tour's (`tour_key`), the
    /// player's own when a player drives it. A trip picked from a tour is the player's while
    /// the tour's other trips are the AI's: the AI's bus on a later trip of the tour did not
    /// pass the stops the player has still to come to.
    pub(super) fn bus_key(&self, i: usize) -> u64 {
        let key = self.tour_key(i);
        if self.is_player_tour(i) { !key } else { key }
    }

    /// The index of the station of departure `i` its bus leaves at `leave` (the traffic's
    /// clock): the stations before it are behind the bus. None: all of them are.
    pub(super) fn station_left_at(&self, i: usize, leave: Option<f64>) -> usize {
        let stations = &self.times_of(i).stations;
        let Some(leave) = leave else {
            return stations.len();
        };
        let start = self.dep_time(i);
        stations
            .iter()
            .position(|s| start + s.1 >= leave - 0.5)
            .unwrap_or(stations.len())
    }

    /// The player's bus, on the departure of its tour that its duty's trip is (of a night
    /// trip, tonight's 24:30 or yesterday's tour's 00:30, whichever is nearer the timetable's
    /// time), or of a tour standing in for it on a day it does not run (`player_twins`).
    fn duty_bus(&self, duty: &PlayerDuty, out: &mut TourBuses) {
        let at = duty.trip().departure;
        let off = |i: usize| (self.departures[i].time - self.last_tod).abs();
        let mut own: Option<usize> = None;
        for t in [at - DAY, at, at + DAY] {
            let lo = self.departures.partition_point(|d| d.time < t - 120.0);
            let hi = self.departures.partition_point(|d| d.time <= t + 120.0);
            for i in lo..hi {
                let d = &self.departures[i];
                if self.player_twins.contains(&i) {
                    out.insert(self.bus_key(i), i, duty.next_stop);
                } else if d.line.eq_ignore_ascii_case(&duty.line)
                    && d.tour.eq_ignore_ascii_case(&duty.tour)
                    && own.is_none_or(|o| off(i) < off(o))
                {
                    own = Some(i);
                }
            }
        }
        if let Some(i) = own {
            out.insert(self.bus_key(i), i, duty.next_stop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetable_run::sim::boards::tests::{schedule, tours};

    /// Tour "1": the 10:00 and the 11:00 of a trip from stop 42 to 43 and on to 44, ten
    /// minutes a leg; tour "2": its 10:05.
    fn three(sim: &mut ScheduleSim) {
        tours(
            sim,
            &[
                (36_000.0, "1", None),
                (36_300.0, "2", None),
                (39_600.0, "1", Some(0)),
            ],
        );
        sim.data.trips[0].stations = vec![42, 43, 44];
        let tt = &mut sim.times[0][0];
        tt.stations = vec![(0.0, 0.0), (600.0, 600.0), (1200.0, 1200.0)];
        tt.stops = vec![true; 3];
        tt.duration = 1200.0;
    }

    /// Omsi.exe 0x61a068: a trip whose bus has left a stop behind (its tour entry +0x668 past
    /// the trip's, or the same with its next station +0x680 past the stop) brings nobody
    /// there any more; a trip whose bus is not on the road keeps its 40 late minutes.
    #[test]
    fn a_trip_brings_nobody_to_a_stop_its_bus_has_left() {
        let mut sim = schedule();
        three(&mut sim);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // 10:12: the 10:00 is due at 43 two minutes ago, the 10:05 in three minutes
        let now = sim.day_base + 36_720.0;
        let due = sim.due_with(now, TourBuses::default());
        assert!(near(due.share_of(43), 1.0));
        assert_eq!(due.visits[&43].len(), 2);
        // the bus of the 10:00 has left 43 for 44: only the 10:05 asks for people at 43
        let mut buses = TourBuses::default();
        buses.insert(sim.tour_key(0), 0, 2);
        let due = sim.due_with(now, buses);
        assert!(
            near(due.share_of(43), 1.0),
            "the 10:05 alone asks for all of them"
        );
        // (the 10:05's bus is not on the road: it keeps its call at 42, as in Omsi.exe)
        assert!(near(due.share_of(42), 1.0));
        // ... standing at 43 the bus still has it ahead
        let mut standing = TourBuses::default();
        standing.insert(sim.tour_key(0), 0, 1);
        assert!(!standing.passed(&due.visits[&43][0]));
        // 10:30: the 10:00 is 20 minutes late at 44, and its bus has gone on to the 11:00
        let now = sim.day_base + 37_800.0;
        let due = sim.due_with(now, TourBuses::default());
        assert!(
            near(due.share_of(43), 1.0),
            "the 10:05 is 15 minutes late at 43"
        );
        let mut buses = TourBuses::default();
        buses.insert(sim.tour_key(2), 2, 0);
        buses.insert(sim.tour_key(1), 1, 2);
        let due = sim.due_with(now, buses);
        assert_eq!(due.share_of(43), 0.0);
        assert!(
            due.get(&43).is_some_and(|d| d.is_empty()),
            "nor any destination"
        );
    }

    /// The buses move on while the stop's trips are worked out once a minute: the share of
    /// a stop follows them (`DueTrips::buses`, every frame).
    #[test]
    fn a_stops_share_drops_the_moment_the_bus_leaves() {
        let mut sim = schedule();
        three(&mut sim);
        let now = sim.day_base + 36_720.0;
        let mut due = sim.due_with(now, TourBuses::default());
        due.visits
            .get_mut(&43)
            .unwrap()
            .retain(|v| v.departure == 0);
        assert_eq!(due.share_of(43), 1.0);
        due.buses.insert(sim.tour_key(0), 0, 2);
        assert_eq!(due.share_of(43), 0.0);
        // a bus of the tour let go further back does not undo it
        due.buses.insert(sim.tour_key(0), 0, 0);
        assert_eq!(due.share_of(43), 0.0);
    }

    /// An AI bus's next stop (its departure time there) gives the stations behind it.
    #[test]
    fn the_stations_before_a_bus_s_next_stop_are_behind_it() {
        let mut sim = schedule();
        three(&mut sim);
        let start = sim.dep_time(0);
        assert_eq!(sim.station_left_at(0, Some(start)), 0);
        assert_eq!(sim.station_left_at(0, Some(start + 600.0)), 1);
        assert_eq!(sim.station_left_at(0, Some(start + 1200.0)), 2);
        assert_eq!(sim.station_left_at(0, None), 3);
    }

    /// A LAN player's bus tells the host nothing of its duty's next stop: having stood at a
    /// stop and left it, its trip asks for nobody there any more.
    #[test]
    fn a_lan_players_bus_passes_the_stops_it_leaves() {
        let mut sim = schedule();
        three(&mut sim);
        sim.set_lan_tours([("1".to_string(), "1".to_string())].into_iter().collect());
        // 10:12, two minutes after the 10:00 was due at 43
        sim.last_tod = 36_720.0;
        let call = DueVisit {
            tour: sim.bus_key(0),
            departure: 0,
            station: 1,
            share: 1.0,
        };
        assert!(!sim.tour_buses(None, None).passed(&call));
        sim.lan_bus_left("1", "1", 43);
        let buses = sim.tour_buses(None, None);
        assert!(buses.passed(&call));
        assert!(
            !buses.passed(&DueVisit { station: 2, ..call }),
            "the stop ahead still asks"
        );
        // given up: nothing is kept of it
        sim.set_lan_tours(Default::default());
        assert!(sim.tour_buses(None, None).0.is_empty());
    }

    /// The player's bus is where its duty is: its trip's departure, its next stop.
    #[test]
    fn the_players_bus_is_on_its_duty_s_trip() {
        let mut sim = schedule();
        three(&mut sim);
        sim.reserve_tour("1", "1");
        let stop = |id: i64, at: f64| PlannedStop {
            object_id: id,
            name: id.to_string(),
            arr: at,
            dep: at,
            position: None,
            dir: StopDir::default(),
            stops: true,
            closed: None,
        };
        let trip = |dep: f64| PlannedTrip {
            name: "T".into(),
            line: "1".into(),
            terminus: "Bahnhof".into(),
            departure: dep,
            end: dep + 1200.0,
            stops: vec![stop(42, dep), stop(43, dep + 600.0), stop(44, dep + 1200.0)],
        };
        let mut duty = PlayerDuty::new(
            "1".into(),
            "1".into(),
            vec![trip(36_000.0), trip(39_600.0)],
            1,
            0,
            false,
        );
        duty.next_stop = 1;
        let buses = sim.tour_buses(None, Some(&duty));
        assert_eq!(buses.0.get(&sim.bus_key(2)), Some(&(2, 1)));
        assert_eq!(buses.0.len(), 1, "tour 2 is the AI's");
        let call = |departure, station| DueVisit {
            tour: sim.bus_key(0),
            departure,
            station,
            share: 1.0,
        };
        assert!(buses.passed(&call(0, 2)), "the trip before it is behind it");
        assert!(buses.passed(&call(2, 0)));
        assert!(!buses.passed(&call(2, 1)), "the stop it makes for is not");
        // (an AI bus of the tour is another bus)
        assert_ne!(sim.bus_key(0), sim.tour_key(0));
    }
}
