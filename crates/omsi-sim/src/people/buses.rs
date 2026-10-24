//! The buses the people use this frame: their doors and the scripts' variables, the next
//! stop, and the vehicles the player placed or other players drive.

use super::*;

impl PeopleSim {
    /// Whether the bus script reports `name`: it writes it (`PAX_*` are engine variables every
    /// vehicle has, so stock scripts set them without a varlist entry) or declares it.
    pub fn script_reports(v: &VehicleInstance, name: &str) -> bool {
        v.has_script_var(name) || v.ty.program.var(name).is_some_and(|id| v.ty.program.stores(id))
    }

    /// Whether entry or exit `i` (`kind` "Entry" or "Exit") has variables of its own: one of
    /// Omsi.exe's eight, or one past them whose `PAX_<kind><i>_Open` the script reports (#719).
    /// The others open with the eighth, as in Omsi.exe (0x62d31e reads the open state of door
    /// min(i, 7)), and ask through its `_Req` ([`PeopleSim::write_door_requests`]).
    pub fn own_pax_door(v: &VehicleInstance, kind: &str, i: usize) -> bool {
        i < OMSI_PAX_DOORS || (i < crate::vehicle::PAX_DOORS && v.pax_reports_open(kind, i))
    }

    /// The passengers' requests into the bus's `PAX_Entry<n>_Req` / `PAX_Exit<n>_Req`, and
    /// who stands in its doorways into their `_Busy` (#720): an entry or exit without
    /// variables of its own asks through the eighth's ([`PeopleSim::own_pax_door`]), where
    /// Omsi.exe drops them (its request arrays have eight slots). Past the eighth they were
    /// written to no variable at all.
    pub fn write_door_requests(v: &mut VehicleInstance, doors: &DoorWants) {
        for (kind, flags, what) in [
            ("Entry", &doors.entry_req, "Req"),
            ("Exit", &doors.exit_req, "Req"),
            ("Entry", &doors.entry_busy, "Busy"),
            ("Exit", &doors.exit_busy, "Busy"),
        ] {
            let mut slots = vec![false; flags.len().min(crate::vehicle::PAX_DOORS)];
            for (i, r) in flags.iter().enumerate() {
                let slot = if Self::own_pax_door(v, kind, i) { i } else { OMSI_PAX_DOORS - 1 };
                if let Some(s) = slots.get_mut(slot) {
                    *s |= *r;
                }
            }
            for (i, r) in slots.iter().enumerate() {
                v.set_pax_var(kind, i, what, if *r { 1.0 } else { 0.0 });
            }
        }
        for (name, on) in &doors.places {
            v.set_var(name, if *on { 1.0 } else { 0.0 });
        }
    }

    /// Whether the bus says when its doors are open: any `PAX_Entry<i>_Open` /
    /// `PAX_Exit<i>_Open` its script reports, or any `door_<i>` / `door<i>` animation of an
    /// entry or exit, including buses whose front door is not an entry. Only an AI bus that
    /// says nothing of any door is taken to open them all while it boards.
    pub(super) fn reports_doors(v: &VehicleInstance, n_entry: usize, n_exit: usize) -> bool {
        let base = if n_entry <= 1 { 1 } else { 2 };
        [("Entry", n_entry, 0), ("Exit", n_exit, base)].into_iter().any(|(kind, count, offset)| {
            (0..count).any(|i| {
                v.pax_reports_open(kind, i) || v.door_anim(offset + i).is_some()
            })
        })
    }

    /// `PAX_Entry<i>_Open` / `PAX_Exit<i>_Open` as the bus script reports them. A bus whose
    /// script never sets them (or only sets some of them) falls back to its physical `door_<i>`
    /// or `door<i>` animations; an entry or exit past the eighth without variables of its own
    /// is open with the eighth ([`PeopleSim::own_pax_door`]).
    pub fn doors_open(v: &VehicleInstance, n_entry: usize, n_exit: usize) -> (Vec<bool>, Vec<bool>) {
        let door_val = |k: usize| -> bool { v.door_anim(k).unwrap_or(0.0) > 0.5 };
        // Exits in standard OMSI city buses (2 or more front door leaves) begin at door_2 (middle door),
        // while coaches with a single front door leaf begin at door_1. Exits must not be offset by
        // n_entry, because buses with all doors configured as entries (e.g. 3-door buses with 6 entries)
        // still place middle-door exits at door_2/3 and rear-door exits at door_4/5.
        let exit_door_base = if n_entry <= 1 { 1 } else { 2 };
        let states = |kind: &str, n: usize, door: &dyn Fn(usize) -> bool| -> Vec<bool> {
            let mut out: Vec<bool> = Vec::with_capacity(n);
            for i in 0..n {
                let open = if !Self::own_pax_door(v, kind, i) {
                    out[OMSI_PAX_DOORS - 1]
                } else if v.pax_reports_open(kind, i) {
                    v.pax_open(kind, i).unwrap_or(0.0) > 0.5
                } else {
                    door(i)
                };
                out.push(open);
            }
            out
        };
        let entry = states("Entry", n_entry, &door_val);
        // Each exit follows its own leaf's animation (an articulated bus's rear doors are
        // door_8, door_9, ...); one past door_7 the bus has no animation for opens with door_7.
        let has_door = |k: usize| v.door_anim(k).is_some();
        let exit = states("Exit", n_exit, &|i| {
            let k = exit_door_base + i;
            door_val(if k > 7 && !has_door(k) { 7 } else { k })
        });
        (entry, exit)
    }

    /// The stop the player's duty is due at next: (stop object, its timetable name, where it
    /// is when the timetable knows it).
    pub fn set_player_next_stop(&mut self, stop: Option<(i64, &str, Option<DVec3>)>) {
        // (called once a frame: worked out again only when the duty moves on to another stop)
        if let (Some(s), Some(cur)) = (stop, self.player_next_stop.as_ref()) {
            if cur.id == s.0 {
                return;
            }
        }
        self.player_next_stop = stop
            .and_then(|stop| self.request_stop(stop.0, Some(stop.1), stop.2));
    }

    pub fn request_stop(
        &self,
        id: i64,
        name: Option<&str>,
        position: Option<DVec3>,
    ) -> Option<RequestStop> {
        let loaded = self.stops.get(&id);
        let name = name
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .or_else(|| loaded.map(|stop| stop.name.clone()))
            .unwrap_or_else(|| id.to_string());
        let alias = loaded
            .map(|stop| {
                if name.trim() == stop.name.trim() {
                    stop.alias.clone()
                } else {
                    stop.name.clone()
                }
            })
            .unwrap_or_default();
        Some(RequestStop {
            id,
            name,
            alias,
            pos: position.or_else(|| loaded.map(|stop| stop.pos))?,
        })
    }

    pub fn vehicle_next_stop(&self, vehicle: &VehicleInstance) -> Option<RequestStop> {
        let tt = usize::try_from(vehicle.host.tt_busstop_index).ok().and_then(|index| {
            vehicle.host.tt_stop_ids.get(index).map(|id| (*id, vehicle.host.tt_stops.get(index).map(|stop| stop.0.as_str())))
        });
        self.next_stop_at(tt, &vehicle.str_var("act_busstop"), vehicle.position, vehicle.heading)
    }

    /// The stop a bus at `position`, heading `heading`, is due at: its timetable's (`tt`: the
    /// stop's object and name), else the nearest stop of the name its scripts give
    /// (`act_busstop`), one facing its way first.
    pub fn next_stop_at(&self, tt: Option<(i64, Option<&str>)>, act_busstop: &str, position: DVec3, heading: f64) -> Option<RequestStop> {
        if let Some((id, name)) = tt {
            if let Some(stop) = self.request_stop(id, name, None) {
                return Some(stop);
            }
        }
        let name = act_busstop;
        if name.trim().is_empty() {
            return None;
        }
        let (id, _) = self
            .stops
            .iter()
            .filter(|(_, stop)| stop.is_named(name))
            .min_by(|(_, a), (_, b)| {
                let back =
                    |stop: &PaxStop| crowd::angle_diff(heading, stop.heading).abs() > 100.0;
                back(a).cmp(&back(b)).then_with(|| {
                    (a.pos - position)
                        .length_squared()
                        .total_cmp(&(b.pos - position).length_squared())
                })
            })?;
        self.request_stop(*id, Some(name), None)
    }

    /// The buses passengers deal with this frame.
    pub fn gather_buses(
        &mut self,
        world: &dyn World,
        bus: Option<&PlayerBusView>,
        traffic: Option<&TrafficSim>,
    ) -> Vec<BusNow> {
        let mut out = Vec::new();
        let stops: Vec<(i64, DVec3, f64)> =
            world.bus_stops().iter().map(|s| (s.0, s.1, s.2)).collect();
        // The stop a bus serves: the nearest in reach - but one facing the way the bus goes
        // before one facing the other way. The two stops of a street often lie within
        // reach of each other, and the people of the stop across the road then walked over
        // the carriageway, through the traffic, to a bus that was not theirs.
        let serving = |pos: DVec3, heading: f64, reach: f64| -> Option<i64> {
            stops
                .iter()
                .filter(|s| (s.1 - pos).length() < reach)
                .min_by(|a, c| {
                    let back = |s: &(i64, DVec3, f64)| crowd::angle_diff(heading, s.2).abs() > 100.0;
                    back(a)
                        .cmp(&back(c))
                        .then((a.1 - pos).length().total_cmp(&(c.1 - pos).length()))
                })
                .map(|s| s.0)
        };
        let bb_from = |bb: Option<[f32; 6]>| {
            let bb = bb.unwrap_or([2.5, 11.0, 3.0, 0.0, 0.0, 1.5]);
            (
                DVec2::new(bb[0] as f64 * 0.5, bb[1] as f64 * 0.5),
                DVec2::new(bb[3] as f64, bb[4] as f64),
            )
        };
        let bb_of = |v: &VehicleInstance| bb_from(v.ty.def.bounding_box);
        if let (Some(b), Some(cabin), Some(cv)) = (bus, self.player_cabin.clone(), bus.and_then(|b| b.cabin.as_ref())) {
            let speed = b.speed_kmh as f64 / 3.6;
            let (entry_open, exit_open) = (cv.entry_open.clone(), cv.exit_open.clone());
            let (half, centre) = bb_from(b.bounding_box);
            let trailers = cv.trailers.clone();
            let terminus = match &b.terminus {
                Some(shown) => shown.clone(),
                // (a bus whose scripts keep no destination index: on a duty, its trip's
                // terminus - not for one showing an `[addterminus_allexit]` sign, which
                // empties the bus whatever the duty)
                None => self.duty.as_ref().map(|(trip, _, _)| trip.terminus.clone()),
            };
            // On a duty the people its trip takes where they are going get on. In free drive
            // the bus takes whom its destination display takes, as Omsi.exe's buses do
            // (sub_61c33c): those whose line record lists the terminus shown, and those
            // without one. (Nobody got on in free drive since 0.2.0, #1627: the bus stood at
            // Grundorf's and Spandau's stops with its line set and its doors open.)
            let takes = match &self.duty {
                Some((trip, next, done)) => Takes::Duty { trip: trip.clone(), next: *next, done: *done },
                None => Takes::Terminus,
            };
            out.push(BusNow {
                terminus,
                takes,
                id: BusId::Player,
                next_stop: self
                    .player_next_stop
                    .clone()
                    .or_else(|| self.next_stop_at(b.tt_stop.as_ref().map(|(id, n)| (*id, n.as_deref())), &b.act_busstop, b.position, b.heading)),
                walk_open: None,
                places_off: cv.places_off.clone(),
                served: None,
                route: b.route.clone().map(Arc::new),
                cabin,
                pos: b.position,
                rot: b.rot,
                heading: b.heading,
                speed,
                entry_open,
                exit_open,
                interior: b.interior,
                air: b.air,
                half,
                centre,
                accel: DVec2::ZERO,
                trailers,
            });
        }
        if let Some(t) = traffic {
            let riding: HashSet<u64> = self
                .people
                .iter()
                .filter_map(|p| match p.state.bus() {
                    Some(BusId::Ai(id)) => Some(id),
                    _ => None,
                })
                .collect();
            let mut visits = HashMap::new();
            for c in t.cars.iter().filter(|c| c.is_bus() || c.vehicle.ty.def.passenger_cabin.is_some()) {
                let from_eye = self.eye.map(|e| (c.vehicle.position - e.pos).length()).unwrap_or(f64::MAX);
                // (the timetable buses near every player: the people waiting around the other
                // LAN players board them too)
                if from_eye > 400.0 && self.far_from_players(c.vehicle.position, 400.0) && !riding.contains(&c.id) {
                    continue;
                }
                let Some(cabin) = self.cabin_for(&c.vehicle) else {
                    continue;
                };
                let speed = c.state.speed as f64;
                let boarding = c.at_station() && speed.abs() < 0.3;
                // The timetable identifies the platform exactly. The closest stop object
                // can belong to the other side of the road or a BRT platform across the
                // median, and must not replace the service's actual stop (#3123).
                let served = if boarding {
                    c.bus.as_ref().and_then(|b| b.stops.front()).map(|s| s.id)
                } else {
                    None
                };
                // Only buses without an authoritative scheduled stop use nearby geometry.
                let stop = served.or_else(|| {
                    boarding.then(|| serving(c.vehicle.position, c.vehicle.heading, 18.0)).flatten()
                });
                let since = match stop {
                    Some(s) => {
                        let v = match self.ai_visits.get(&c.id) {
                            Some(&(vs, t0)) if vs == s => (vs, t0),
                            _ => (s, self.time),
                        };
                        visits.insert(c.id, v);
                        self.time - v.1
                    }
                    None => 0.0,
                };
                let open = stop.is_some();
                let (mut entry_open, mut exit_open) = (
                    vec![false; cabin.entries.len()],
                    vec![false; cabin.exits.len()],
                );
                if open {
                    if Self::reports_doors(&c.vehicle, cabin.entries.len(), cabin.exits.len()) {
                        let (e, x) =
                            Self::doors_open(&c.vehicle, cabin.entries.len(), cabin.exits.len());
                        entry_open = e;
                        exit_open = x;
                    } else if since > 2.5 {
                        // the script does not say: the doors are open while the bus boards
                        entry_open
                            .iter_mut()
                            .chain(exit_open.iter_mut())
                            .for_each(|o| *o = true);
                    }
                }
                let walk_open = (!c.is_bus()).then(|| Self::doors_open(&c.vehicle, cabin.entries.len(), cabin.exits.len()));
                self.seats
                    .entry(BusId::Ai(c.id))
                    .or_insert_with(|| vec![false; cabin.seats.len()]);
                let (half, centre) = bb_of(&c.vehicle);
                let trailers = part_frames(&c.vehicle, &cabin);
                out.push(BusNow {
                    terminus: c.bus.as_ref().map(|b| b.terminus.trim().to_string()).filter(|t| !t.is_empty()),
                    takes: if c.is_bus() { Takes::Terminus } else { Takes::Nobody },
                    id: BusId::Ai(c.id),
                    next_stop: c
                        .bus
                        .as_ref()
                        .and_then(|bus| bus.stops.front())
                        .and_then(|stop| {
                            let position = c
                                .state
                                .route
                                .get(stop.ri)
                                .and_then(|lane| t.net.lanes.get(*lane))
                                .map(|lane| lane.at(stop.s).0);
                            self.request_stop(stop.id, None, position)
                        }),
                    walk_open,
                    places_off: places_off(&c.vehicle, &cabin),
                    served,
                    route: None,
                    cabin,
                    pos: c.vehicle.position,
                    rot: c.vehicle.body_rotation(),
                    heading: c.vehicle.heading,
                    speed,
                    entry_open,
                    exit_open,
                    interior: c.vehicle.interior_light(),
                    air: CabinAir::of(&c.vehicle),
                    half,
                    centre,
                    accel: DVec2::ZERO,
                    trailers,
                });
            }
            self.ai_visits = visits;
            let alive: HashSet<u64> = t.cars.iter().map(|c| c.id).chain(self.remote_now.iter().chain(self.placed_now.iter()).filter_map(|b| match b.id {
                BusId::Ai(id) => Some(id),
                BusId::Player => None,
            })).collect();
            self.seats.retain(|k, _| match k {
                BusId::Ai(id) => alive.contains(id),
                BusId::Player => true,
            });
        }
        for b in self.remote_now.iter().chain(self.placed_now.iter()) {
            self.seats.entry(b.id).or_insert_with(|| vec![false; b.cabin.seats.len()]);
            out.push(b.clone());
        }
        out
    }

    /// The vehicles standing in the world that the player placed and does not drive now:
    /// (their `Player::uid`, the vehicle). Their riders stay aboard; nobody new boards them.
    pub fn set_placed_buses<'a>(&mut self, buses: impl Iterator<Item = (u64, &'a VehicleInstance)>) {
        let mut out = Vec::new();
        for (uid, v) in buses {
            if let Some(b) = self.parked_bus(BusId::Ai(placed_bus_id(uid)), v) {
                out.push(b);
            }
        }
        self.placed_now = out;
    }

    /// A bus people may be in but do not board here (another player's, one the player left).
    pub fn parked_bus(&mut self, id: BusId, v: &VehicleInstance) -> Option<BusNow> {
        let cabin = self.cabin_for(v)?;
        let bb = v.ty.def.bounding_box.unwrap_or([2.5, 11.0, 3.0, 0.0, 0.0, 1.5]);
        let trailers = part_frames(v, &cabin);
        let walk_open = Self::doors_open(v, cabin.entries.len(), cabin.exits.len());
        Some(BusNow {
            terminus: None,
            takes: Takes::Nobody,
            places_off: Vec::new(),
            served: None,
            route: None,
            id,
            next_stop: None,
            entry_open: vec![false; cabin.entries.len()],
            exit_open: vec![false; cabin.exits.len()],
            walk_open: Some(walk_open),
            cabin,
            pos: v.position,
            rot: v.body_rotation(),
            heading: v.heading,
            speed: v.physics.velocity_kmh() as f64 / 3.6,
            interior: v.interior_light(),
            air: CabinAir::of(v),
            half: DVec2::new(bb[0] as f64 * 0.5, bb[1] as f64 * 0.5),
            centre: DVec2::new(bb[3] as f64, bb[4] as f64),
            accel: DVec2::ZERO,
            trailers,
        })
    }

    /// The player now drives vehicle `new_uid` and left `old_uid`: whoever rode in the one
    /// left stays in it (it is one of the placed vehicles now), whoever rode in the one
    /// taken over is the player's bus's, and the player's cabin is the new vehicle's own.
    /// (Riders followed the player into the next bus, and people boarding it took the old
    /// bus's seats - places in the air round a minibus's bonnet.)
    pub fn player_bus_swapped(&mut self, old_uid: u64, new_uid: u64, new_vehicle: &mut VehicleInstance) {
        let old = BusId::Ai(placed_bus_id(old_uid));
        let new = BusId::Ai(placed_bus_id(new_uid));
        // (through a free id: Player -> old, new -> Player)
        let tmp = BusId::Ai(u64::MAX);
        self.remap_bus(BusId::Player, tmp);
        self.remap_bus(new, BusId::Player);
        self.remap_bus(tmp, old);
        let kept = self.seats.remove(&BusId::Player);
        self.player_cabin = None;
        self.served_stop = None;
        self.set_cabin(new_vehicle);
        if let (Some(k), Some(now)) = (kept, self.seats.get_mut(&BusId::Player)) {
            if k.len() == now.len() {
                *now = k;
            }
        }
    }

    /// Bus `bus` is gone (the player removed it): whoever was in it stands where they were,
    /// on the ground, and walks off.
    pub fn evict(&mut self, bus: BusId, world: &dyn World) {
        let _ = world;
        for i in (0..self.people.len()).rev() {
            let p = &self.people[i];
            let theirs = matches!(p.place, Place::Bus(b, _) if b == bus) || matches!(&p.state, State::Pax(x) if x.bus == Some(bus) || x.inside == Some(bus));
            if theirs {
                self.release(i);
                let p = self.people.swap_remove(i);
                self.retire(&p);
            }
        }
        self.seats.remove(&bus);
        self.bus_motion.remove(&bus);
        if bus == BusId::Player {
            self.player_cabin = None;
            self.served_stop = None;
        }
    }

    /// Everyone and everything that belongs to bus `from` belongs to `to` now.
    pub fn remap_bus(&mut self, from: BusId, to: BusId) {
        let fix = |b: &mut BusId| {
            if *b == from {
                *b = to;
            }
        };
        for p in &mut self.people {
            if let Place::Bus(b, _) = &mut p.place {
                fix(b);
            }
            if let State::Pax(x) = &mut p.state {
                if let Some(b) = x.bus.as_mut() {
                    fix(b);
                }
                if let Some(b) = x.inside.as_mut() {
                    fix(b);
                }
            }
        }
        if let Some(v) = self.seats.remove(&from) {
            self.seats.insert(to, v);
        }
        if let Some(v) = self.bus_motion.remove(&from) {
            self.bus_motion.insert(to, v);
        }
    }

    /// LAN play: the other players' buses this frame, by player id (their riders are drawn
    /// in them, see `remote_bus_id`).
    pub fn set_remote_buses<'a>(&mut self, buses: impl Iterator<Item = (u32, &'a VehicleInstance)>) {
        let mut out = Vec::new();
        for (player, v) in buses {
            let Some(cabin) = self.cabin_for(v) else { continue };
            let bb = v.ty.def.bounding_box.unwrap_or([2.5, 11.0, 3.0, 0.0, 0.0, 1.5]);
            let trailers = part_frames(v, &cabin);
            // (their doors as their game has them: a walker gets in only where one is open;
            // the passengers here never board it - that bus's own game boards them)
            let walk_open = Self::doors_open(v, cabin.entries.len(), cabin.exits.len());
            out.push(BusNow {
                terminus: None,
                takes: Takes::Nobody,
                places_off: Vec::new(),
                served: None,
                route: None,
                id: BusId::Ai(remote_bus_id(player)),
                next_stop: None,
                entry_open: vec![false; cabin.entries.len()],
                exit_open: vec![false; cabin.exits.len()],
                walk_open: Some(walk_open),
                cabin,
                pos: v.position,
                rot: v.body_rotation(),
                heading: v.heading,
                speed: v.physics.velocity_kmh() as f64 / 3.6,
                interior: v.interior_light(),
                air: CabinAir::of(v),
                half: DVec2::new(bb[0] as f64 * 0.5, bb[1] as f64 * 0.5),
                centre: DVec2::new(bb[3] as f64, bb[4] as f64),
                accel: DVec2::ZERO,
                trailers,
            });
        }
        self.remote_now = out;
    }

    /// Is `bus` among the buses people can be in this frame (an AI bus, or another player's)?
    pub fn knows_bus(&self, bus: u64) -> bool {
        self.remote_now.iter().any(|b| b.id == BusId::Ai(bus)) || self.seats.contains_key(&BusId::Ai(bus))
    }
}
