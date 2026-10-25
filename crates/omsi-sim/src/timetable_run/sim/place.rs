//! Where a due departure's bus is: the step of its route it is on now, and the part of the
//! route around it that the loaded tiles have, with the trip's stops placed on it.

use super::*;

impl ScheduleSim {
    /// Where departure `i`'s bus is put on the road at `day_time`: the part of its route
    /// the loaded tiles have, with its stops placed on it - or why it is not put on the road
    /// now. With `onto` (the timetable bus of that index takes the trip on), the trip starts
    /// at its beginning.
    pub fn place_departure(
        &mut self,
        i: usize,
        world: &dyn TimetableWorld,
        traffic: &mut TrafficSim,
        day_time: f64,
        onto: Option<usize>,
    ) -> Result<Placing, Placed> {
        let on_route = self.locate_departure(i, world, traffic, day_time, onto)?;
        Ok(Self::route_section(world, traffic, on_route))
    }

    /// Where on its route departure `i` is at `day_time`: the trip's steps, the slots the loaded
    /// tiles have for them, and the step the bus is on - or why it is not put on the road now.
    fn locate_departure(
        &mut self,
        i: usize,
        world: &dyn TimetableWorld,
        traffic: &mut TrafficSim,
        day_time: f64,
        onto: Option<usize>,
    ) -> Result<OnRoute, Placed> {
        let trip = &self.data.trips[self.departures[i].trip];
        let trip_name = trip.name.clone();
        // a trip of a transport route has no track until the map's network has been read
        if self.transit_unrouted.contains(&trip_name.to_ascii_lowercase()) {
            return Err(Placed::Wait);
        }
        // (the [station] records of a type-1 trip as well: Novi Sad's buses have no others,
        // and without them they drove past every stop)
        let stations = trip_stations(trip);
        // the timetable's times at the stations (see `TripTimes`)
        let departure = self.dep_time(i);
        let tt = self.times_of(i).clone();
        let duration = tt.duration;
        if day_time - departure > duration && onto.is_none() {
            return Err(Placed::Drop); // already arrived
        }
        let arrive: Vec<f64> = tt.stations.iter().map(|s| departure + s.0).collect();
        let leave: Vec<f64> = tt.stations.iter().map(|s| departure + s.1).collect();
        let (steps, track) = self.steps_of(&trip_name, &stations);
        let station_steps = trip_station_steps(trip, track, steps.len());
        add_twins(traffic, &steps);
        let slots = self.slots(world, traffic, &steps, None);
        if !slots.iter().any(|s| matches!(s, Slot::Lane(_))) && !slots.contains(&Slot::Waiting) {
            log::debug!("trip {trip_name}: no route");
            return Err(Placed::Drop);
        }
        // the leg the bus is on now, and how far along it (a layover bus stands at the start)
        let legs = if track {
            1
        } else {
            stations.len().saturating_sub(1).max(1)
        };
        let leg_time = |k: usize| {
            if track {
                (departure, departure + duration)
            } else {
                (
                    leave[k],
                    arrive.get(k + 1).copied().unwrap_or(departure + duration),
                )
            }
        };
        // (the tour's own bus taking the trip on starts it at its first stop, however late)
        let leg = (0..legs)
            .rev()
            .find(|&k| leg_time(k).0 <= day_time)
            .filter(|_| onto.is_none())
            .unwrap_or(0);
        let (t0, t1) = leg_time(leg);
        let frac = if day_time <= departure || onto.is_some() {
            0.0
        } else {
            ((day_time - t0) / (t1 - t0).max(1e-3)).clamp(0.0, 1.0)
        };
        // lengths of the steps: a step still to come counts as long as an average one
        let net = &traffic.net;
        let known: Vec<f64> = slots
            .iter()
            .filter_map(|s| {
                if let Slot::Lane(l) = s {
                    Some(net.lanes[*l].length() as f64)
                } else {
                    None
                }
            })
            .collect();
        let average = if known.is_empty() {
            40.0
        } else {
            known.iter().sum::<f64>() / known.len() as f64
        };
        let est: Vec<f64> = slots
            .iter()
            .map(|slot| match slot {
                Slot::Lane(l) => net.lanes[*l].length() as f64,
                Slot::Waiting => average,
                Slot::Absent => 0.0,
            })
            .collect();
        let Some((at, offset)) = step_at(&steps, &slots, &est, leg, frac) else {
            return Err(Placed::Drop);
        };
        if slots[at] == Slot::Waiting {
            // when it reaches the next loaded step, at the pace of its leg
            let in_leg = |k: usize| track || steps[k].leg == leg;
            let leg_len: f64 = (0..steps.len())
                .filter(|&k| in_leg(k))
                .map(|k| est[k])
                .sum();
            let rate = (t1 - t0).max(0.0) / leg_len.max(1e-3);
            let base = day_time.max(departure);
            let retry = match (at + 1..slots.len()).find(|&k| matches!(slots[k], Slot::Lane(_))) {
                Some(k) if in_leg(k) => {
                    let ahead =
                        (est[at] - offset).max(0.0) + (at + 1..k).map(|j| est[j]).sum::<f64>();
                    base + ahead * rate
                }
                // on a later leg (or nowhere): look again when this leg is over
                _ => t1.max(base),
            };
            log::debug!(
                "trip {trip_name}: the bus is on a tile that is not loaded (again at {:.2} min)",
                retry / 60.0
            );
            self.retry_at.insert(i, retry);
            return Err(Placed::Wait);
        }
        Ok(OnRoute { trip_name, stations, departure, tt, leave, steps, track, station_steps, slots, leg, frac, at, offset })
    }

    /// The part of the route around the bus that the network has, with the trip's stops
    /// placed on it.
    fn route_section(world: &dyn TimetableWorld, traffic: &mut TrafficSim, r: OnRoute) -> Placing {
        let OnRoute { trip_name, stations, departure, tt, leave, steps, track, station_steps, slots, leg, frac, at, offset } = r;
        // the part of the route around the bus that the network has
        let (start, end) = section_around(&slots, at);
        let whole = start == 0 && end == slots.len();
        let lane_of = |s: &Slot| {
            if let Slot::Lane(l) = s {
                Some(*l)
            } else {
                None
            }
        };
        let section: Vec<usize> = slots[start..end].iter().filter_map(lane_of).collect();
        let start_index = slots[start..at].iter().filter_map(lane_of).count();
        add_connectors(traffic, &section);
        let net = &traffic.net;
        let (section, index) = bridge_gaps(net, &section);
        let start_index = index[start_index.min(index.len() - 1)];
        let s = offset.min(net.lanes[section[start_index]].length() as f64) as f32;
        // stations → stop points on that part of the route
        let reach = if whole { None } else { Some(STOP_REACH) };
        let mut served = vec![false; stations.len()];
        let mut stops = Vec::new();
        let mut from = 0;
        for (si, sid) in stations.iter().enumerate() {
            // a station the trip runs through is none of its stops
            if !tt.stops[si] {
                served[si] = true;
                continue;
            }
            // the stations of the legs behind the bus are passed
            if !track && (si < leg || (si == leg && frac > 0.0)) {
                served[si] = true;
            }
            let found = world.object_positions().get(sid).copied();
            // An authored entry behind the spawn position is already passed.
            if station_steps[si].is_some_and(|entry| entry < at) {
                served[si] = true;
                continue;
            }
            match found {
                Some((pos, _)) => match project_stop(
                    net,
                    &section,
                    pos,
                    reach,
                    from,
                    world.stop_side(*sid),
                    station_route(station_steps[si], start, &slots[start..end], &index),
                ) {
                    Some((ri, ss, lat)) => {
                        from = ri;
                        served[si] = true;
                        stops.push((ri, ss, bay_offset(lat), leave[si], *sid, world.stop_side(*sid)));
                    }
                    None => log::debug!("station {sid}: not near the route"),
                },
                None => log::debug!("station {sid}: object not in the map"),
            }
        }
        stops.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
            let len: f32 = section.iter().map(|&l| net.lanes[l].length()).sum();
            log::info!("trip {trip_name}: {} stations {:?}, route {} of {} steps ({} lanes, {len:.0} m) from step {start}, bus on step {at} (leg {leg}, {:.0} %), stops {:?}", stations.len(), stations, end - start, steps.len(), section.len(), frac * 100.0, stops);
        }
        Placing { trip_name, stations, departure, leave, steps, station_steps, slots, leg, frac, at, start, end, section, start_index, s, stops, served }
    }
}

/// Where on its route a departure's bus is now (`ScheduleSim::locate_departure`).
struct OnRoute {
    trip_name: String,
    stations: Vec<i64>,
    departure: f64,
    tt: TripTimes,
    leave: Vec<f64>,
    steps: Vec<Step>,
    track: bool,
    station_steps: Vec<Option<usize>>,
    slots: Vec<Slot>,
    leg: usize,
    frac: f64,
    at: usize,
    offset: f64,
}

/// The part of the route a departure's bus is put on, with its stops (`ScheduleSim::route_section`).
pub struct Placing {
    pub trip_name: String,
    pub stations: Vec<i64>,
    pub departure: f64,
    pub leave: Vec<f64>,
    pub steps: Vec<Step>,
    pub station_steps: Vec<Option<usize>>,
    pub slots: Vec<Slot>,
    pub leg: usize,
    pub frac: f64,
    pub at: usize,
    pub start: usize,
    pub end: usize,
    pub section: Vec<usize>,
    pub start_index: usize,
    pub s: f32,
    pub stops: Vec<(usize, f32, f32, f64, i64, f32)>,
    pub served: Vec<bool>,
}
