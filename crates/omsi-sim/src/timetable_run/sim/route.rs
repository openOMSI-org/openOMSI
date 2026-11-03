//! A trip's route on the lanes of the network: its steps, the slots the loaded tiles have for
//! them, and its stops placed on it.

use super::*;

impl ScheduleSim {
    /// The steps of a trip's route: from the trip's own track when it has one (trains,
    /// ferries, planes), else from the station links between its stops. The flag says it is
    /// a track.
    ///
    /// The track is the one the trip's `[trip]` block names on its first line (Novi Sad's
    /// trip "1 Klisa-Liman I" runs track "1_Klisa-Liman1"; the stock trains name tracks of
    /// their own name), else the one named like the trip.
    pub(super) fn steps_of(&self, track_name: &str, stations: &[i64]) -> (Vec<Step>, bool) {
        let track_name = self
            .data
            .trip(track_name)
            .map(|t| t.display_name.trim())
            .filter(|n| !n.is_empty())
            .unwrap_or(track_name);
        let key = |id: f64, path: f64, tile_index: f64| {
            self.tile_coords
                .get(tile_index as usize)
                .map(|&tile| LaneKey {
                    tile,
                    id: id as i64,
                    path: path as u16,
                })
        };
        let mut steps: Vec<Step> = Vec::new();
        if let Some(track) = self.data.tracks.iter().find(|t| {
            t.path
                .file_stem()
                .map(|s| s.to_string_lossy().eq_ignore_ascii_case(track_name))
                .unwrap_or(false)
        }) {
            steps.extend(
                track
                    .entries
                    .iter()
                    .filter(|e| e.values.len() >= 5)
                    .map(|e| Step {
                        key: key(e.values[0], e.values[1], e.values[2]),
                        leg: 0,
                        length: e.values[4],
                    }),
            );
            return (steps, true);
        }
        for (leg, w) in stations.windows(2).enumerate() {
            match self
                .data
                .stn_links
                .iter()
                .find(|l| l.from_id == w[0] && l.to_id == w[1])
            {
                Some(link) => {
                    for e in &link.entries {
                        let k = key(e.values[0], e.values[1], e.values[2]);
                        // consecutive links repeat the shared lane
                        if steps.last().map(|s| s.key == k).unwrap_or(false) {
                            continue;
                        }
                        steps.push(Step {
                            key: k,
                            leg,
                            length: e.values[3],
                        });
                    }
                }
                None => log::debug!("trip {track_name}: no station link {} -> {}", w[0], w[1]),
            }
        }
        (steps, false)
    }

    /// The steps as the loaded network has them, each lane's direction chosen so that it
    /// follows the lane before it (`prev` for the first) and leads into the one after it.
    /// Taking whichever direction came first - as the first step used to, with nothing
    /// before it - sent the route (and the navigator) the wrong way along a two-way street.
    pub(super) fn slots(
        &self,
        world: &dyn TimetableWorld,
        traffic: &TrafficSim,
        steps: &[Step],
        prev: Option<usize>,
    ) -> Vec<Slot> {
        let net = &traffic.net;
        // every step's candidate lanes (both directions of a two-way path)
        let cands: Vec<Result<&Vec<usize>, Slot>> = steps
            .iter()
            .map(|st| {
                let Some(key) = st.key else {
                    return Err(Slot::Absent);
                };
                match net.by_key.get(&key) {
                    Some(c) if !c.is_empty() => Ok(c),
                    _ if !traffic.lane_tiles.contains(&key.tile) && world.has_tile(key.tile) => {
                        Err(Slot::Waiting)
                    }
                    _ => Err(Slot::Absent),
                }
            })
            .collect();
        let mut out = Vec::with_capacity(steps.len());
        let mut last = prev;
        for (i, c) in cands.iter().enumerate() {
            let c = match c {
                Ok(c) => *c,
                Err(slot) => {
                    if *slot == Slot::Waiting {
                        last = None;
                    }
                    out.push(*slot);
                    continue;
                }
            };
            // the next lanes the route has (not across a gap)
            let next = cands[i + 1..].iter().find_map(|x| match x {
                Ok(n) => Some(Some(*n)),
                Err(Slot::Waiting) => Some(None),
                Err(_) => None,
            });
            let next = next.flatten();
            let score = |l: usize| -> f64 {
                let mut s = 0.0;
                if let Some(prev) = last {
                    s += (net.lanes[l].start() - net.lanes[prev].end()).length();
                }
                if let Some(next) = next {
                    let end = net.lanes[l].end();
                    s += next
                        .iter()
                        .map(|&n| (net.lanes[n].start() - end).length())
                        .fold(f64::MAX, f64::min);
                }
                s
            };
            let best = c
                .iter()
                .copied()
                .min_by(|a, b| score(*a).total_cmp(&score(*b)))
                .unwrap();
            out.push(Slot::Lane(best));
            last = Some(best);
        }
        skip_detours(net, &mut out);
        if omsi_cfg::flags::OMSI_DEBUG_ROUTES.is_set() {
            // where consecutive lanes of the route do not join (a gap, or a change within the
            // same spline, which is a lane change)
            let lanes: Vec<usize> = out
                .iter()
                .filter_map(|s| {
                    if let Slot::Lane(l) = s {
                        Some(*l)
                    } else {
                        None
                    }
                })
                .collect();
            for (k, w) in lanes.windows(2).enumerate() {
                let (a, b) = (&net.lanes[w[0]], &net.lanes[w[1]]);
                let gap = (b.start() - a.end()).truncate().length();
                if gap > 2.0
                    || a.key.map(|k| (k.tile, k.id, k.path))
                    == b.key.map(|k| (k.tile, k.id, k.path))
                {
                    log::info!("route: step {k}: lane {} {:?} rev {} -> lane {} {:?} rev {}: gap {gap:.1} m, linked {}, lane change {}", w[0], a.key, a.reversed, w[1], b.key, b.reversed, a.next.contains(&w[1]), net.parallel(w[0], w[1]));
                }
            }
        }
        let waiting = out.iter().filter(|s| **s == Slot::Waiting).count();
        let absent = out.iter().filter(|s| **s == Slot::Absent).count();
        if absent > 0 || omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
            log::debug!("route: {} of {} steps on loaded lanes, {waiting} on tiles still to come, {absent} not in the map", out.len() - waiting - absent, out.len());
        }
        out
    }

    /// The lanes a trip runs on (for the navigator): its track, else the station links
    /// between its stops, as far as the tiles have brought them - and whether that is all.
    pub fn trip_route(
        &self,
        world: &dyn TimetableWorld,
        traffic: &TrafficSim,
        trip_name: &str,
    ) -> (Vec<usize>, bool) {
        let Some(trip) = self
            .data
            .trips
            .iter()
            .find(|x| x.is_named(trip_name))
        else {
            return (Vec::new(), true);
        };
        let slots = self.slots(
            world,
            traffic,
            &self.steps_of(trip_name, &trip_stations(trip)).0,
            None,
        );
        let complete = !slots.contains(&Slot::Waiting);
        (
            slots
                .into_iter()
                .filter_map(|s| if let Slot::Lane(l) = s { Some(l) } else { None })
                .collect(),
            complete,
        )
    }

    /// The lanes a trip runs on in `net` - the navigator's network of the whole map, which
    /// has every tile's lanes whether loaded or not - chosen as `slots` chooses them (of a
    /// two-way path the direction that joins the lanes before and after).
    pub fn trip_route_in(&self, net: &crate::traffic::Network, trip_name: &str) -> Vec<usize> {
        let Some(trip) = self.data.trips.iter().find(|x| x.is_named(trip_name)) else {
            return Vec::new();
        };
        let (steps, _) = self.steps_of(trip_name, &trip_stations(trip));
        let cands: Vec<Option<&Vec<usize>>> = steps.iter().map(|st| st.key.and_then(|k| net.by_key.get(&k)).filter(|c| !c.is_empty())).collect();
        let mut out: Vec<Slot> = Vec::with_capacity(steps.len());
        let mut last: Option<usize> = None;
        for (i, c) in cands.iter().enumerate() {
            let Some(c) = c else {
                out.push(Slot::Absent);
                continue;
            };
            let next = cands[i + 1..].iter().find_map(|x| *x);
            let score = |l: usize| -> f64 {
                let mut s = 0.0;
                if let Some(prev) = last {
                    s += (net.lanes[l].start() - net.lanes[prev].end()).length();
                }
                if let Some(next) = next {
                    let end = net.lanes[l].end();
                    s += next.iter().map(|&n| (net.lanes[n].start() - end).length()).fold(f64::MAX, f64::min);
                }
                s
            };
            let best = c.iter().copied().min_by(|a, b| score(*a).total_cmp(&score(*b))).unwrap();
            out.push(Slot::Lane(best));
            last = Some(best);
        }
        skip_detours(net, &mut out);
        out.into_iter().filter_map(|s| if let Slot::Lane(l) = s { Some(l) } else { None }).collect()
    }

    /// `OMSI_CHECK_TRIPS=1`: build the route of every trip on the loaded lanes and say where
    /// consecutive lanes do not join - a gap the bus would jump, or a lane taken the wrong
    /// way round (its end, not its start, lies where the lane before ends), which sends a
    /// bus into the oncoming traffic. Only trips whose route is wholly loaded are judged.
    pub fn check_routes(&self, world: &dyn TimetableWorld, traffic: &mut TrafficSim) {
        for trip in &self.data.trips {
            let (steps, _) = self.steps_of(&trip.name, &trip_stations(trip));
            add_twins(traffic, &steps);
            let lanes: Vec<usize> = self
                .slots(world, traffic, &steps, None)
                .iter()
                .filter_map(|s| if let Slot::Lane(l) = s { Some(*l) } else { None })
                .collect();
            add_connectors(traffic, &lanes);
        }
        let traffic = &*traffic;
        let net = &traffic.net;
        let (mut trips, mut joints, mut linked, mut changes, mut gaps, mut wrong, mut partial) =
            (0, 0, 0, 0, 0, 0, 0);
        let mut bad_length = 0;
        for trip in &self.data.trips {
            let stations = trip_stations(trip);
            let (steps, _) = self.steps_of(&trip.name, &stations);
            let slots = self.slots(world, traffic, &steps, None);
            if slots.contains(&Slot::Waiting) || steps.is_empty() {
                if partial < 5 {
                    let k = slots.iter().position(|s| *s == Slot::Waiting).unwrap_or(0);
                    log::info!(
                        "check trips: {}: {} steps, {} on tiles not loaded, first {:?} (tile loaded {}, in the map {})",
                        trip.name,
                        steps.len(),
                        slots.iter().filter(|s| **s == Slot::Waiting).count(),
                        steps.get(k).and_then(|s| s.key),
                        steps.get(k).and_then(|s| s.key).map(|key| traffic.lane_tiles.contains(&key.tile)).unwrap_or(false),
                        steps.get(k).and_then(|s| s.key).map(|key| world.has_tile(key.tile)).unwrap_or(false),
                    );
                }
                partial += 1;
                continue;
            }
            trips += 1;
            // `OMSI_CHECK_TRIPS=<trip name>`: every step of that trip
            if omsi_cfg::flags::OMSI_CHECK_TRIPS.var().map(|v| v.eq_ignore_ascii_case(&trip.name)).unwrap_or(false) {
                for (k, (st, sl)) in steps.iter().zip(&slots).enumerate() {
                    let cands: Vec<String> = st
                        .key
                        .and_then(|key| net.by_key.get(&key))
                        .map(|c| {
                            c.iter()
                                .map(|&l| {
                                    let x = &net.lanes[l];
                                    format!("{l}{} ({:.1},{:.1})->({:.1},{:.1}) {:.1} m", if x.reversed { "r" } else { "" }, x.start().x, x.start().y, x.end().x, x.end().y, x.length())
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    log::info!("check trips: {} step {k} leg {} {:?} ({:.1} m): {:?} of {:?}", trip.name, st.leg, st.key, st.length, sl, cands);
                }
            }
            // the lane a step names should be as long as the file says its path is
            for (st, sl) in steps.iter().zip(&slots) {
                if let Slot::Lane(l) = sl {
                    let len = net.lanes[*l].length() as f64;
                    if st.length > 0.5 && (len - st.length).abs() > 1.0 + 0.05 * st.length {
                        if bad_length < 12 {
                            log::info!(
                                "check trips: {}: lane {} {:?} is {len:.1} m long, the file says {:.1} m",
                                trip.name, l, net.lanes[*l].key, st.length
                            );
                        }
                        bad_length += 1;
                    }
                }
            }
            let lanes: Vec<usize> = slots
                .iter()
                .filter_map(|s| if let Slot::Lane(l) = s { Some(*l) } else { None })
                .collect();
            let lanes = bridge_gaps(net, &lanes).0;
            let mut shown = 0;
            for (k, w) in lanes.windows(2).enumerate() {
                let (a, b) = (&net.lanes[w[0]], &net.lanes[w[1]]);
                joints += 1;
                if a.next.contains(&w[1]) {
                    linked += 1;
                    continue;
                }
                if net.parallel(w[0], w[1]) {
                    changes += 1;
                    continue;
                }
                let gap = (b.start() - a.end()).truncate().length();
                let backwards = (b.end() - a.end()).truncate().length();
                let is_wrong = backwards + 1.0 < gap && backwards < 3.0;
                if is_wrong {
                    wrong += 1;
                } else if gap > 2.0 {
                    gaps += 1;
                } else {
                    linked += 1;
                    continue;
                }
                if shown < 6 {
                    shown += 1;
                    log::info!(
                        "check trips: {}: step {k}: lane {} {:?} rev {} -> {} {:?} rev {}: {} (gap {gap:.1} m, to its end {backwards:.1} m) at ({:.0}, {:.0})",
                        trip.name, w[0], a.key, a.reversed, w[1], b.key, b.reversed,
                        if is_wrong { "WRONG WAY" } else { "gap" }, a.end().x, a.end().y
                    );
                }
            }
        }
        log::info!("check trips: {trips} trips on loaded lanes ({partial} not wholly loaded): {joints} joints, {linked} joined, {changes} lane changes, {gaps} gaps, {wrong} taken the wrong way round; {bad_length} lanes not as long as the file says");
    }
}
