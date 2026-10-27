//! The timetable ownership changes with the existing bus, never a second departure.

use super::*;

/// An opaque reservation snapshot preserves single-trip versus whole-tour selection.
#[derive(Clone, PartialEq)]
pub struct DutyReservation {
    tour: (String, String),
    departure: Option<f64>,
}

impl DutyReservation {
    pub(super) fn owns(&self, d: &Departure) -> bool {
        d.line.eq_ignore_ascii_case(&self.tour.0)
            && d.tour.eq_ignore_ascii_case(&self.tour.1)
            && self.departure.is_none_or(|t| {
                let dt = (d.time - t).rem_euclid(DAY);
                dt < 30.0 || dt > DAY - 30.0
            })
    }
}

impl ScheduleSim {
    /// Retained buses still own their departures, including after a clock reset.
    /// The caller supplies the live collection so deleted vehicles release their tours.
    pub fn set_retained_reservations(&mut self, reservations: Vec<DutyReservation>) {
        if self.retained_reservations == reservations {
            return;
        }
        let before: Vec<bool> = (0..self.departures.len())
            .map(|i| self.is_player_tour(i))
            .collect();
        self.retained_reservations = reservations;
        let mine: Vec<bool> = (0..self.departures.len())
            .map(|i| self.is_player_tour(i))
            .collect();
        for (i, d) in self.departures.iter_mut().enumerate() {
            if mine[i] {
                d.spawned = true;
            } else if before[i] && d.time > self.last_tod {
                d.spawned = false;
            }
        }
        self.pending.retain(|i| !mine[*i]);
        self.waiting.retain(|i| !mine[*i]);
        self.retry_at.retain(|i, _| !mine[*i]);
        self.purge_player_tour = true;
    }

    pub fn duty_reservation(&self) -> Option<DutyReservation> {
        self.player_tour.clone().map(|tour| DutyReservation {
            tour,
            departure: self.player_departure,
        })
    }

    pub fn restore_duty_reservation(&mut self, reservation: Option<DutyReservation>) {
        self.player_tour = None;
        self.player_departure = None;
        self.purge_player_tour = false;
        if let Some(r) = reservation {
            self.player_departure = r.departure;
            self.reserve_tour(&r.tour.0, &r.tour.1);
        }
    }

    /// Locate the exact service before the caller moves a vehicle or releases its driver.
    pub fn handover_departure(&self, duty: &PlayerDuty) -> Option<usize> {
        let trip = duty.trips.get(duty.trip_index)?;
        self.departures
            .iter()
            .enumerate()
            .find(|(i, d)| {
                d.line.eq_ignore_ascii_case(&duty.line)
                    && d.tour.eq_ignore_ascii_case(&duty.tour)
                    && self.trip_of(*i).name.eq_ignore_ascii_case(&trip.name)
                    && (self.dep_time(*i) - trip.departure).abs() < 1.0
            })
            .map(|(i, _)| i)
    }

    /// Resume future departures of the player's tour, with its current bus already present.
    pub fn handover_to_ai(&mut self, car: u64, departure: usize) {
        let line = self.departures[departure].line.clone();
        let tour = self.departures[departure].tour.clone();
        let time = self.departures[departure].time;
        let mine: Vec<bool> = (0..self.departures.len())
            .map(|i| self.is_player_tour(i))
            .collect();
        self.player_tour = None;
        self.player_departure = None;
        self.purge_player_tour = false;
        for (i, d) in self.departures.iter_mut().enumerate() {
            if mine[i]
                && !self.retained_reservations.iter().any(|r| r.owns(d))
                && d.line.eq_ignore_ascii_case(&line)
                && d.tour.eq_ignore_ascii_case(&tour)
                && d.time >= time
            {
                d.spawned = i == departure;
            }
        }
        self.pending.retain(|i| *i != departure);
        self.waiting.retain(|i| *i != departure);
        self.retry_at.remove(&departure);
        self.car_departure.insert(car, departure);
    }

    /// Build the player's duty for the very departure the selected car is running.
    pub fn take_car_duty(
        &mut self,
        world: &dyn TimetableWorld,
        car: u64,
        now: f64,
    ) -> Result<Option<PlayerDuty>, String> {
        let Some(&i) = self.car_departure.get(&car) else {
            return Ok(None);
        };
        let d = &self.departures[i];
        let (line, tour) = (d.line.clone(), d.tour.clone());
        let expected = self.dep_time(i);
        let name = self.trip_of(i).name.clone();
        let plan = self.tour_plan(world, &line, &tour)?;
        let mut trips: Vec<_> = plan.trips.into_iter().map(|t| t.trip).collect();
        trips.sort_by(|a, b| a.departure.total_cmp(&b.departure));
        let selected = trips
            .iter()
            .position(|t| {
                t.name.eq_ignore_ascii_case(&name)
                    && ((t.departure - expected + DAY / 2.0).rem_euclid(DAY) - DAY / 2.0).abs()
                        < 1.0
            })
            .ok_or("The selected AI departure no longer matches its tour")?;
        let shift = expected - trips[selected].departure;
        for trip in &mut trips {
            trip.departure += shift;
            trip.end += shift;
            for stop in &mut trip.stops {
                stop.arr += shift;
                stop.dep += shift;
            }
        }
        let mut duty = PlayerDuty::new(line.clone(), tour.clone(), trips, selected, 0, true);
        duty.unnumbered_public = self.unnumbered_public(&duty.trips);
        self.player_departure = None;
        self.reserve_tour(&line, &tour);
        let _ = now;
        self.forget_car(car);
        self.forget_route(car);
        Ok(Some(duty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        path: PathBuf,
        ai: omsi_map::AiLists,
        positions: crate::world_data::ObjectPlaces,
    }
    impl TimetableWorld for World {
        fn root(&self) -> &Path {
            &self.path
        }
        fn map_dir(&self) -> &Path {
            &self.path
        }
        fn chrono_dirs(&self) -> Vec<PathBuf> {
            vec![]
        }
        fn ailists(&self) -> &omsi_map::AiLists {
            &self.ai
        }
        fn raw_tiles(&self) -> Vec<(i32, i32)> {
            vec![]
        }
        fn has_tile(&self, _: (i32, i32)) -> bool {
            false
        }
        fn date(&self) -> i32 {
            20221010
        }
        fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
            &self.positions
        }
        fn stop_side(&self, _: i64) -> f32 {
            0.0
        }
    }

    fn fixture() -> (World, ScheduleSim) {
        let world = World {
            path: std::env::temp_dir().join("openomsi-handover-no-assets"),
            ai: Default::default(),
            positions: Default::default(),
        };
        let mut s = ScheduleSim::new(&world.path, &world, &crate::SimClock::default());
        s.data.trips.push(omsi_timetable::Trip {
            name: "trip".into(),
            line: "L".into(),
            stations: vec![10, 20],
            ..Default::default()
        });
        s.times = vec![vec![TripTimes {
            stations: vec![(0.0, 0.0), (60.0, 60.0)],
            stops: vec![true, true],
            duration: 60.0,
            ..Default::default()
        }]];
        s.data.lines.push(omsi_timetable::Line {
            name: "L".into(),
            tours: vec![omsi_timetable::Tour {
                number: "T".into(),
                extra: "1023".into(),
                trips: [60.0, 120.0, 180.0]
                    .into_iter()
                    .map(|departure| omsi_timetable::TourTrip {
                        trip: "trip".into(),
                        departure,
                        profile: 0,
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        });
        s.departures = [3600.0, 7200.0, 10800.0]
            .into_iter()
            .map(|time| Departure {
                time,
                trip: 0,
                profile: 0,
                line: "L".into(),
                ai_group: String::new(),
                tour: "T".into(),
                mask: 1023,
                spawned: true,
                yesterday: false,
            })
            .collect();
        s.tour_next = vec![Some(1), Some(2), None];
        s.tour_prev = vec![None, Some(0), Some(1)];
        (world, s)
    }

    #[test]
    fn retained_duties_stay_reserved_across_clock_resets_and_are_released_when_removed() {
        let (_, mut s) = fixture();
        s.reserve_tour("L", "T");
        let original = s.duty_reservation().unwrap();
        s.reserve_tour("other", "tour");
        s.set_retained_reservations(vec![original]);
        s.restart_clock(7300.0, 0);
        assert!((0..3).all(|i| s.is_player_tour(i) && s.departures[i].spawned));
        assert!(s.begin_tick(7300.0).is_empty());
        assert!(s.pending.is_empty());
        s.set_retained_reservations(vec![]);
        assert!((0..3).all(|i| !s.is_player_tour(i)));
        assert!(!s.departures[2].spawned);
    }

    #[test]
    fn returning_an_earlier_ai_trip_keeps_a_retained_later_trip_reserved() {
        let (world, mut s) = fixture();
        s.player_departure = Some(10800.0);
        s.reserve_tour("L", "T");
        let original = s.duty_reservation().unwrap();
        s.car_departure.insert(77, 1);
        s.take_car_duty(&world, 77, 7250.0).unwrap();
        s.set_retained_reservations(vec![original.clone()]);
        s.handover_to_ai(88, 1);
        s.set_retained_reservations(vec![original]);
        assert!(s.is_player_tour(2));
        assert!(s.departures[2].spawned);
        assert_eq!(s.car_departure.get(&88), Some(&1));
        assert!(!s.may_take_on(2, 10800.0));
    }

    #[test]
    fn retained_single_trip_does_not_reserve_the_rest_of_an_ai_tour() {
        let (_, mut s) = fixture();
        s.player_departure = Some(7200.0);
        s.reserve_tour("L", "T");
        let original = s.duty_reservation().unwrap();
        s.restore_duty_reservation(None);
        s.set_retained_reservations(vec![original]);
        s.restart_clock(7300.0, 0);
        assert!(!s.is_player_tour(0));
        assert!(s.is_player_tour(1));
        assert!(!s.is_player_tour(2));
        assert!(!s.departures[2].spawned);
    }

    #[test]
    fn restoring_a_parked_duty_preserves_single_departure_selection() {
        let (_, mut s) = fixture();
        s.player_departure = Some(7200.0);
        s.reserve_tour("L", "T");
        let saved = s.duty_reservation();
        s.player_departure = None;
        s.reserve_tour("other", "tour");
        s.restore_duty_reservation(saved);
        assert!(!s.is_player_tour(0));
        assert!(s.is_player_tour(1));
        assert!(!s.is_player_tour(2));
        s.restore_duty_reservation(None);
        assert!(s.player_tour.is_none());
        assert!(s.player_departure.is_none());
        assert!(!s.purge_player_tour);
    }

    #[test]
    fn restoring_a_parked_whole_tour_keeps_all_its_departures() {
        let (_, mut s) = fixture();
        s.reserve_tour("L", "T");
        let saved = s.duty_reservation();
        s.restore_duty_reservation(None);
        s.restore_duty_reservation(saved);
        assert!((0..3).all(|i| s.is_player_tour(i)));
    }

    #[test]
    fn ai_takeover_keeps_the_exact_departure_across_midnight() {
        let (world, mut s) = fixture();
        s.day_base = DAY;
        s.car_departure.insert(77, 1);
        let duty = s.take_car_duty(&world, 77, DAY + 7250.0).unwrap().unwrap();
        assert_eq!(duty.trip_index, 1);
        assert_eq!(duty.trip().departure, DAY + 7200.0);
        assert_eq!(duty.trip().stops[1].dep, DAY + 7260.0);
        assert!(s.car_departure.get(&77).is_none());
        assert!(s.is_player_tour(1));
        assert_eq!(s.handover_departure(&duty), Some(1));
        s.handover_to_ai(88, 1);
        assert!(!s.is_player_tour(1));
        assert_eq!(s.car_departure.get(&88), Some(&1));
        assert!(s.departures[1].spawned);
        assert!(!s.departures[2].spawned);
        assert_eq!(s.next_of_car(88), Some(2));
        assert!(s.begin_tick(DAY + 7250.0).is_empty());
    }

    #[test]
    fn handover_distinguishes_yesterday_and_today_with_the_same_trip() {
        let (world, mut s) = fixture();
        s.car_departure.insert(77, 1);
        let mut duty = s.take_car_duty(&world, 77, 7250.0).unwrap().unwrap();
        s.departures.insert(
            0,
            Departure {
                time: 7200.0 - DAY,
                trip: 0,
                profile: 0,
                line: "L".into(),
                ai_group: String::new(),
                tour: "T".into(),
                mask: 1023,
                spawned: true,
                yesterday: true,
            },
        );
        assert_eq!(s.handover_departure(&duty), Some(2));
        duty.trips[duty.trip_index].departure -= DAY;
        assert_eq!(s.handover_departure(&duty), Some(0));
    }

    #[test]
    fn releasing_a_single_trip_does_not_respawn_another_ais_departure() {
        let (_, mut s) = fixture();
        s.player_departure = Some(7200.0);
        s.reserve_tour("L", "T");
        s.car_departure.insert(99, 2);
        s.handover_to_ai(88, 1);
        assert!(s.departures[2].spawned);
        assert_eq!(s.car_departure.get(&99), Some(&2));
    }

    #[test]
    fn a_stale_ai_tour_does_not_change_timetable_ownership() {
        let (world, mut s) = fixture();
        s.car_departure.insert(77, 1);
        s.data.lines.clear();
        assert!(s.take_car_duty(&world, 77, 7250.0).is_err());
        assert!(s.player_tour.is_none());
        assert_eq!(s.car_departure.get(&77), Some(&1));
    }
}
