//! A tour's bus taking its next trip on where it stands: the route from there onto the
//! trip, its stops, its line and destination. omsi-app's `schedule` turns a train round
//! first (that moves its renders); the rest is the timetable's.

use super::*;

impl ScheduleSim {
    /// The tour's bus `ci` (in `traffic`), at the end of its previous trip, takes departure `i`
    /// on, placed by `p` (`ScheduleSim::place_departure`).
    pub fn take_trip_on(&mut self, i: usize, ci: usize, p: Placing, traffic: &mut TrafficSim, day_time: f64) -> Placed {
        let Placing { trip_name, stations, departure, leave, steps, station_steps, slots, end, section, mut stops, served, .. } = p;
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
        // a bus whose next trip begins at a stop further on (a terminus that sets down and
        // takes on at separate stops) stands where it set down until it is time to drive up,
        // as Omsi.exe's buses do: driving up at once, it held the first stop for its whole
        // layover and the buses after it queued in the road (#3142)
        if let Some(&(ri, ss, ..)) = stops.first().filter(|_| !rail) {
            let drive = route[..ri].iter().map(|&l| net.lanes[l].length()).sum::<f32>() - s0 + ss;
            let leave_at = departure - (drive / DRIVE_UP_SPEED) as f64 - DRIVE_UP_AHEAD;
            if drive > STOP_REACH as f32 && day_time < leave_at {
                self.tour_hold.insert(traffic.cars[ci].id, leave_at);
                return Placed::Early;
            }
        }
        self.tour_hold.remove(&traffic.cars[ci].id);
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
            self.carry_later(id, steps, end, &stations, &leave, served, station_steps);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_traffic::bus_service::{AiTimetable, Phase};
    use crate::ai_traffic::service_tests::{road, Fixture};
    use crate::ai_traffic::model::BusSetup;
    use crate::timetable_run::sim::boards::tests::{no_world, schedule, tours};
    use crate::traffic::{LaneBuilder, LaneKind, Network};

    /// Tour 1 at 10:00 (its bus out) and 10:30; its bus has set down 500 m before the
    /// 10:30 trip's first stop.
    const DEPART: f64 = 37_800.0;
    const SET_DOWN: f32 = 100.0;
    /// The way up (500 m at `DRIVE_UP_SPEED`) and `DRIVE_UP_AHEAD`.
    const LEAVE_AT: f64 = DEPART - 100.0 - DRIVE_UP_AHEAD;

    fn tour_sim() -> ScheduleSim {
        let mut sim = schedule();
        tours(&mut sim, &[(36_000.0, "1", None), (DEPART, "1", Some(0))]);
        sim.departures[0].spawned = true;
        sim
    }

    /// The tour's bus at the end of its 10:00 trip, standing at `s` on lane 0.
    fn done_bus(sim: &mut ScheduleSim, t: &mut TrafficSim, f: &Fixture, kind: LaneKind, s: f32) -> (u64, usize) {
        let setup = BusSetup {
            route: vec![0],
            stops: Vec::new(),
            number: None,
            hof: None,
            timetable: AiTimetable { line: String::new(), terminus: String::new(), stops: Vec::new() },
        };
        let vehicle = crate::VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        let id = t.place_car(vehicle, kind, 0, s, f.ty.clone(), 7, None, Some(7), Some(0.0), Some(setup));
        let ci = t.cars.iter().position(|c| c.id == id).unwrap();
        t.cars[ci].bus.as_mut().unwrap().phase = Phase::TripDone;
        sim.bus_runs(id, 0);
        (id, ci)
    }

    /// The 10:30 trip on lane 0, its first stop at `first`, its last at 1400 m.
    fn placing(first: f32) -> Placing {
        Placing {
            trip_name: "1".into(),
            stations: vec![42, 43],
            departure: DEPART,
            leave: vec![DEPART, DEPART + 600.0],
            steps: vec![Step { key: None, leg: 0, length: 1500.0 }],
            station_steps: vec![Some(0), Some(0)],
            slots: vec![Slot::Lane(0)],
            leg: 0,
            frac: 0.0,
            at: 0,
            start: 0,
            end: 1,
            section: vec![0],
            start_index: 0,
            s: first,
            stops: vec![(0, first, 0.0, DEPART, 42, 0.0), (0, 1400.0, 0.0, DEPART + 600.0, 43, 0.0)],
            served: vec![false, false],
        }
    }

    /// The bus stands where it set down until it is time to drive up, no other bus comes out
    /// for the trip meanwhile, and then it takes the trip on, once.
    #[test]
    fn a_tour_bus_waits_where_it_set_down_then_takes_its_trip_once() {
        let f = Fixture::new();
        let (world, mut traffic, mut sim) = (no_world(), road(&f), tour_sim());
        let (id, ci) = done_bus(&mut sim, &mut traffic, &f, LaneKind::Street, SET_DOWN);
        let t = DEPART - 900.0;
        assert!(matches!(sim.take_trip_on(1, ci, placing(SET_DOWN + 500.0), &mut traffic, t), Placed::Early));
        assert!(sim.tour_held(id, t) && sim.tour_held(id, LEAVE_AT - 1.0) && !sim.tour_held(id, LEAVE_AT));
        assert!(traffic.cars[ci].trip_done(), "it stands where it set down");
        assert_eq!(sim.next_of_car(id), Some(1), "still the tour's bus for the trip");
        sim.queue_due(&world, &mut traffic, LEAVE_AT - 60.0, 1.0);
        assert!(sim.pending.is_empty(), "no second bus for the trip: {:?}", sim.pending);

        assert!(matches!(sim.take_trip_on(1, ci, placing(SET_DOWN + 500.0), &mut traffic, LEAVE_AT), Placed::Spawned));
        sim.taken_on(1);
        assert!(sim.tour_hold.is_empty());
        assert!(!traffic.cars[ci].trip_done(), "on its way up");
        sim.queue_due(&world, &mut traffic, DEPART + 0.5, 1.0);
        assert!(sim.pending.is_empty(), "the trip is not put out again: {:?}", sim.pending);
    }

    /// A bus that sets down after its time to drive up goes on at once.
    #[test]
    fn a_late_tour_bus_drives_up_at_once() {
        let f = Fixture::new();
        let (mut traffic, mut sim) = (road(&f), tour_sim());
        let (id, ci) = done_bus(&mut sim, &mut traffic, &f, LaneKind::Street, SET_DOWN);
        assert!(matches!(sim.take_trip_on(1, ci, placing(SET_DOWN + 500.0), &mut traffic, LEAVE_AT + 30.0), Placed::Spawned));
        assert!(!sim.tour_held(id, LEAVE_AT + 30.0));
    }

    /// A first stop where the bus stands takes it on at once, as before.
    #[test]
    fn a_tour_bus_at_its_first_stop_takes_its_trip_on_at_once() {
        let f = Fixture::new();
        let (mut traffic, mut sim) = (road(&f), tour_sim());
        let (id, ci) = done_bus(&mut sim, &mut traffic, &f, LaneKind::Street, SET_DOWN);
        assert!(matches!(sim.take_trip_on(1, ci, placing(SET_DOWN + 10.0), &mut traffic, DEPART - 900.0), Placed::Spawned));
        assert!(sim.tour_hold.is_empty() && !sim.tour_held(id, DEPART - 900.0));
    }

    /// A train is not held.
    #[test]
    fn a_train_takes_its_trip_on_at_once() {
        let f = Fixture::new();
        let mut traffic = road(&f);
        let mut net = Network { lanes: vec![LaneBuilder::arc(glam::DVec3::ZERO, 0.0, 1500.0, 0.0, 0.0, LaneKind::Rail, 3.0)], ..Default::default() };
        net.link(1.5);
        net.build_grid();
        net.compute_reach();
        traffic.net = net;
        let mut sim = tour_sim();
        let (id, ci) = done_bus(&mut sim, &mut traffic, &f, LaneKind::Rail, SET_DOWN);
        assert!(traffic.cars[ci].is_rail());
        assert!(matches!(sim.take_trip_on(1, ci, placing(SET_DOWN + 500.0), &mut traffic, DEPART - 900.0), Placed::Spawned));
        assert!(!sim.tour_held(id, DEPART - 900.0));
    }

    /// The hold goes with the bus and with a change of the clock.
    #[test]
    fn the_hold_goes_with_the_bus_and_the_clock() {
        let f = Fixture::new();
        let (mut traffic, mut sim) = (road(&f), tour_sim());
        let (id, ci) = done_bus(&mut sim, &mut traffic, &f, LaneKind::Street, SET_DOWN);
        let t = DEPART - 900.0;
        assert!(matches!(sim.take_trip_on(1, ci, placing(SET_DOWN + 500.0), &mut traffic, t), Placed::Early));
        sim.forget_car(id);
        assert!(!sim.tour_held(id, t));

        sim.bus_runs(id, 0);
        assert!(matches!(sim.take_trip_on(1, ci, placing(SET_DOWN + 500.0), &mut traffic, t), Placed::Early));
        sim.restart_clock(t + 3600.0, 1);
        assert!(!sim.tour_held(id, t));
    }
}
