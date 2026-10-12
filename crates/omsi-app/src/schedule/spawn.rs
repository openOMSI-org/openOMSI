//! Putting one departure on the road.

use super::*;
use omsi_render::{Renderer, Scene};

impl Schedule {
    /// Put departure `i` on the road where its bus is at `day_time`: on the part of the route
    /// the loaded tiles have, which is carried on as more tiles come.
    ///
    /// With `onto`, the timetable bus of that index (the tour's bus, at the end of its
    /// previous trip) takes the trip on instead of a new one.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn_departure(
        &mut self,
        i: usize,
        world: &World,
        traffic: &mut Traffic,
        view: &mut TrafficView,
        renderer: &Renderer,
        scene: &mut Scene,
        day_time: f64,
        onto: Option<usize>,
    ) -> Placed {
        let profile = omsi_cfg::flags::OMSI_PROFILE.is_set();
        let t_spawn = std::time::Instant::now();
        let p = match self.sim.place_departure(i, world, &mut traffic.sim, day_time, onto) {
            Ok(p) => p,
            Err(placed) => return placed,
        };
        let t_route = t_spawn.elapsed();
        if let Some(ci) = onto {
            return self.take_trip_on(i, ci, p, world, traffic, view, renderer, scene, day_time);
        }
        self.spawn_new_bus(i, p, world, traffic, view, renderer, scene, day_time, profile, t_spawn, t_route)
    }

    /// The tour's bus `ci`, at the end of its previous trip, takes departure `i` on.
    #[allow(clippy::too_many_arguments)]
    fn take_trip_on(
        &mut self,
        i: usize,
        ci: usize,
        p: Placing,
        world: &World,
        traffic: &mut Traffic,
        view: &mut TrafficView,
        renderer: &Renderer,
        scene: &mut Scene,
        day_time: f64,
    ) -> Placed {
        let (trip_name, section) = (&p.trip_name, &p.section);
        // a train whose next trip runs the other way (its `[trainreverse]` is not how the
        // train stands) is turned round where it stands, as Omsi.exe turns it when the
        // trip begins (0x613a98): its last car leads, on the way back. (It drove off
        // along the siding instead - past the end of the track - and another train
        // appeared for the trip.)
        let reverse = self.sim.trip_of(i).train_reverse;
        if traffic.cars[ci].is_rail() && traffic.cars[ci].consist_reversed != reverse {
            let c = &traffic.cars[ci];
            let tail = c.vehicle.trailers.last().map(|t| t.position).unwrap_or(c.vehicle.position);
            let net = &traffic.net;
            let found = section
                .iter()
                .enumerate()
                .take(24)
                .filter_map(|(k, &l)| net.lanes[l].nearest_point(tail).map(|(s, d)| (k, l, s, d)))
                .min_by(|a, b| a.3.total_cmp(&b.3));
            match found {
                Some((k, l, s, d)) if d < 2.5 => {
                    traffic.turn_train(view, world, renderer, scene, ci, l, s, &section[..k], reverse);
                }
                _ => {
                    if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                        log::info!("trip {trip_name}: train {} would turn round, but its last car at ({:.1}, {:.1}) is not on the trip's way (nearest {:?}); its front at ({:.1}, {:.1}) on lane {}", c.id, tail.x, tail.y, found.map(|f| (f.0, f.1, f.2, f.3)), c.vehicle.position.x, c.vehicle.position.y, c.state.lane);
                        for &l in section.iter().take(4).chain(std::iter::once(&c.state.lane)) {
                            let ln = &net.lanes[l];
                            log::info!("  lane {l} {:?} len {:.1} ({:.1}, {:.1}) -> ({:.1}, {:.1}) next {:?} rev {}", ln.key, ln.length(), ln.start().x, ln.start().y, ln.end().x, ln.end().y, ln.next, ln.reversed);
                        }
                    }
                }
            }
        }
        self.sim.take_trip_on(i, ci, p, &mut traffic.sim, day_time)
    }

    /// A new bus for departure `i`, put on the road where the trip is now.
    #[allow(clippy::too_many_arguments)]
    fn spawn_new_bus(
        &mut self,
        i: usize,
        p: Placing,
        world: &World,
        traffic: &mut Traffic,
        view: &mut TrafficView,
        renderer: &Renderer,
        scene: &mut Scene,
        day_time: f64,
        profile: bool,
        t_spawn: std::time::Instant,
        t_route: std::time::Duration,
    ) -> Placed {
        let Placing { trip_name, stations, departure, leave, steps, station_steps, station_tiles, slots, leg, frac, at, start, end, section, start_index, mut s, mut stops, served } = p;
        let Some(Choice {
                     ty,
                     number,
                     hof,
                     scheme,
                     train,
                 }) = self.sim.choose(i, world)
        else {
            log::warn!(
                "trip {trip_name}: no vehicles for AI group '{}'",
                self.sim.departure_ai_group(i)
            );
            return Placed::Drop;
        };
        self.next_number += 1;
        let rail = traffic.net.lanes[section[start_index]].kind == omsi_sim::traffic::LaneKind::Rail;
        // every further car of the train with the cars of its unit, as Omsi.exe creates
        // each car of a `.zug` (the first has its own with `create_car`): the ones before it
        // (towards the front of the train), the car, the ones behind it
        let rest: Option<Vec<(Arc<VehicleType>, bool)>> = train.as_ref().map(|cars| {
            let mut rest = Vec::new();
            for (t, rev) in &cars[1..] {
                let mut front = traffic.coupled_chain(t, *rev, false);
                front.reverse();
                rest.extend(front);
                rest.push((t.clone(), *rev));
                rest.extend(traffic.coupled_chain(t, *rev, true));
            }
            rest
        });
        // a trip that runs the train turned round (`[trainreverse]`): its last car leads
        let turned: Option<Vec<(Arc<VehicleType>, bool)>> = (self.sim.trip_of(i).train_reverse
            && rail)
            .then(|| {
                let mut all = vec![(ty.clone(), false)];
                all.extend(traffic.trailer_chain(&ty));
                all.extend(rest.clone().unwrap_or_default());
                all.into_iter().rev().map(|(t, r)| (t, !r)).collect()
            });
        // where the one that leads comes to rest at a station
        let lead_ty = turned.as_ref().map(|t| t[0].0.clone()).unwrap_or_else(|| ty.clone());
        place_stops(&traffic.net, &section, 0, &mut stops, &lead_ty, rail);
        log::debug!("spawn trip {trip_name}: departure {:.2} min, now {:.2} min, leg {leg} at {:.0} %, step {at} of {}, start {s:.0} m into its lane", departure / 60.0, day_time / 60.0, frac * 100.0, steps.len());
        // the bus starts on its step's lane; the stops behind it are dropped
        let mut start_index = start_index;
        while start_index + 1 < section.len()
            && s > traffic.net.lanes[section[start_index]].length()
        {
            s -= traffic.net.lanes[section[start_index]].length();
            start_index += 1;
        }
        // a bus that would start a few metres short of its next stop stands at it (half a
        // metre short, so that it is served): starting before it, it had to pull over into
        // the stop - often a lane over - in less than its own length
        if let Some(&(ri, ss, _, _, _, _)) = stops
            .iter()
            .find(|st| st.0 > start_index || (st.0 == start_index && st.1 > s))
        {
            let mut d = ss - s;
            for k in start_index..ri {
                if !traffic.net.parallel(section[k], section[k + 1]) {
                    d += traffic.net.lanes[section[k]].length();
                }
            }
            if d < 25.0 {
                start_index = ri;
                s = (ss - 0.5).max(0.0);
            }
        }
        let at_pos = traffic.net.lanes[section[start_index]].at(s).0;
        // lanes stay in the network when their tile is unloaded: nothing is put on ground
        // that is not there (the departure waits for its tile)
        if !track_is_air(traffic, section[start_index]) && !world.has_ground(at_pos.x, at_pos.y) {
            log::debug!("trip {trip_name}: the bus would stand on an unloaded tile");
            return Placed::Wait;
        }
        // not into a vehicle that happens to be there (the player's bus at its stop, a car),
        // nor just in front of one driving up to that place: try again in a moment
        let at_heading = traffic.net.lanes[section[start_index]].at(s).1 as f64;
        if traffic.blocked(&ty, at_pos, at_heading) || !traffic.spawn_clear(&ty, at_pos, at_heading)
        {
            log::debug!("trip {trip_name}: a vehicle stands where the bus would appear");
            return Placed::Busy;
        }
        // A bus on its layover waits at the stand only when no other bus stands there or
        // pulls in; otherwise it comes at its departure time. (A stand shared by four lines
        // had five buses queueing in the road for a quarter of an hour, and every bus
        // serving the stop and every car behind them waiting as well.)
        if departure > day_time + 30.0
            && traffic
            .cars
            .iter()
            .any(|c| c.is_bus() && !c.gone && (c.vehicle.position - at_pos).length() < 50.0)
        {
            log::debug!(
                "trip {trip_name}: its stand is taken, the bus comes at its departure time"
            );
            self.sim.stand_taken(i);
            return Placed::Drop;
        }
        // nobody may see a bus appear (except while the map loads)
        // (the map-loading exemption holds only while the world is being built: a departure
        // of that moment that had to wait popped up in plain view minutes later)
        if !(self.sim.queued_at_startup(i) && traffic.loading_phase()) && !traffic.may_appear(world, at_pos) {
            log::debug!("trip {trip_name}: the bus would appear in view");
            return Placed::Busy;
        }
        self.sim.started(i);
        let stops: Vec<(usize, f32, f32, f64, i64, f32)> = stops
            .into_iter()
            .filter(|(ri, ss, _, _, _, _)| *ri > start_index || (*ri == start_index && *ss > s))
            .map(|(ri, ss, lat, t, id, side)| (ri - start_index, ss, lat, t, id, side))
            .collect();
        let route: Vec<usize> = section[start_index..].to_vec();
        // the trip's own line (" 5"), which is what the displays show; the timetable line's
        // name ("5 & 5N") only groups the tours
        let trip_line = self.sim.trip_of(i)
            .line
            .trim()
            .to_string();
        let line = if trip_line.is_empty() {
            self.sim.departure_line(i).to_string()
        } else {
            trip_line
        };
        let tour = self.sim.departure_tour(i).to_string();
        let terminus = self.sim.trip_of(i).terminus.clone();
        let Some(ci) = traffic.spawn_bus(
            view,
            world,
            renderer,
            scene,
            lead_ty.clone(),
            route,
            s,
            stops,
            number.clone(),
            hof.clone(),
            Some(scheme),
            self.sim.ai_timetable(i),
        ) else {
            return Placed::Drop;
        };
        self.sim.bus_runs(traffic.cars[ci].id, i);
        if let Some(t) = &turned {
            traffic.set_trailers(view, world, renderer, scene, ci, &t[1..]);
            traffic.cars[ci].consist_reversed = true;
        } else if let Some(rest) = &rest {
            traffic.attach_cars(view, world, renderer, scene, ci, rest);
        }
        if train.is_some() {
            log::info!("train: {}", std::iter::once(traffic.cars[ci].vehicle.ty.def.path.file_stem().unwrap_or_default().to_string_lossy().to_string()).chain(traffic.cars[ci].vehicle.trailers.iter().map(|t| format!("{}{}", t.ty.def.path.file_stem().unwrap_or_default().to_string_lossy(), if t.reversed { " (turned)" } else { "" }))).collect::<Vec<_>>().join(" + "));
            traffic.cars[ci].state.max_speed_kmh = 90.0;
            traffic.cars[ci].state.length = 20.0 * (1 + traffic.cars[ci].vehicle.trailers.len()) as f32;
        }
        let names = self.sim.trip_stop_names(self.sim.trip_index(i));
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let last_stop = trip_stations(&self.sim.trip_of(i)).last().copied();
        let (always, early, holds) = self.sim.special_stops(i);
        let car = &mut traffic.cars[ci];
        // on its layover only when it stands at its first stop now (the trip's first station
        // may lie on a part of the track that is not loaded): it waits there for its departure
        if let Some(b) = car.bus.as_mut() {
            b.layover = departure > day_time
                && b.stops.front().map(|st| st.ri == 0 && (st.s - s).abs() < 2.0).unwrap_or(false);
            b.route_open = end < slots.len();
            b.terminus = terminus.clone();
            b.last_stop = last_stop;
            b.always = always;
            b.serve_early = early;
            b.holds = holds;
        }
        // the bus scripts read the line/terminus for their displays
        if let Some(i) = ty.program.str_var("Linie") {
            car.vehicle.state.str_vars[i as usize] = line.clone();
        }
        set_ai_destination(&mut car.vehicle, hof.as_deref(), &line, &terminus, &names);
        log::info!("scheduled bus: line {line} tour {tour} trip {trip_name} {} #{:?} at {:.1} min, {} stops, at ({:.1}, {:.1}) heading {:.0}{}", ty.def.type_name, number.as_ref().map(|n| format!("{} plate {:?} paint {:?}", n.0, n.1, scheme.and_then(|i| ty.paint_schemes.get(i)).map(|p| p.name.as_str()))), day_time / 60.0, car.bus.as_ref().map(|b| b.stops.len()).unwrap_or(0), car.vehicle.position.x, car.vehicle.position.y, car.vehicle.heading, if end < slots.len() { format!(", route {} of {} steps so far", end - start, steps.len()) } else { String::new() });
        if end < slots.len() {
            self.sim.carry_later(car.id, steps, end, &stations, &leave, served, station_steps, station_tiles);
        }
        if profile {
            log::info!(
                "  spawn took {:.1} ms (route {:.1} ms), {} pending, {} waiting",
                t_spawn.elapsed().as_secs_f64() * 1000.0,
                t_route.as_secs_f64() * 1000.0,
                self.sim.pending(),
                self.sim.waiting_count()
            );
        }
        Placed::Spawned
    }
}
