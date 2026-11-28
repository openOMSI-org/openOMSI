//! LAN play (see `lan_world`): a host keeps people around every player, tells the clients
//! where they are and hands the waiting ones over to a client's bus; a client draws the
//! host's people instead of its own and simulates only those who board its bus.

use super::*;

/// Where one of the host's people is this frame, as a client draws them.
#[derive(Debug, Clone, Copy)]
pub struct MirrorPose {
    pub pos: DVec3,
    pub heading: f64,
    pub vel: DVec2,
    pub activity: Activity,
    /// Aboard a timetable bus: (its id, the point of its frame, heading in its frame, the
    /// seat or standing place, if known).
    pub aboard: Option<(u64, Vec3, f64, Option<usize>)>,
    /// Waiting at a stop: (the stop object, the waiting place).
    pub waiting: Option<(i64, usize)>,
}

/// A server-owned waiting person held until the player's physical boarding
/// confirmation. Keeping the original person avoids inventing a new human
/// model, passenger ID, route or destination when a claim is cancelled.
pub struct HandedPerson {
    pub owner: u32,
    pub person: Person,
    pub stop: Option<i64>,
    pub spot: Option<usize>,
}

/// The bus id (`BusId::Ai`) another LAN player's bus has among the buses here: far above
/// the traffic's car ids.
pub fn remote_bus_id(player: u32) -> u64 {
    (1 << 40) | player as u64
}

/// The bus id (`BusId::Ai`) of a vehicle the player placed (`Player::uid`).
pub fn placed_bus_id(uid: u64) -> u64 {
    (2 << 40) | uid
}

/// The player whose bus `remote_bus_id` gave this id (None for a traffic bus).
pub fn remote_bus_player(bus: u64) -> Option<u32> {
    (bus >> 40 == 1).then_some((bus & 0xFFFF_FFFF) as u32)
}

/// One of the host's people as it tells the clients.
pub struct LanPerson {
    pub id: u32,
    pub ty: Arc<HumanType>,
    pub pos: DVec3,
    pub heading: f64,
    pub speed: f64,
    pub activity: Activity,
    pub aboard: Option<(u64, Vec3, f64, Option<usize>)>,
    pub waiting: Option<(i64, usize)>,
}

impl PeopleSim {
    /// Is person `id` (one drawn for another game) here?
    pub fn has_mirror(&self, id: u32) -> bool {
        self.people.iter().any(|p| p.id == id && p.remote)
    }

    /// The riders of our own bus, for the other LAN players to see: (id, type, place in the
    /// bus frame, heading there, seat, activity).
    pub fn lan_riders(&self) -> Vec<LanPerson> {
        self.people
            .iter()
            .filter(|p| p.puppet.is_none() && !p.remote)
            .filter_map(|p| match p.place {
                Place::Bus(BusId::Player, l) => Some(LanPerson {
                    id: p.id,
                    ty: p.ty.clone(),
                    pos: p.position,
                    heading: p.heading,
                    speed: 0.0,
                    activity: p.activity,
                    aboard: Some((
                        0,
                        l,
                        p.lheading,
                        match &p.state {
                            State::Pax(x) if x.task == Task::SittingInBus => x.seat,
                            _ => None,
                        },
                    )),
                    waiting: None,
                }),
                _ => None,
            })
            .collect()
    }

    /// The places people are kept around: ours (the player's bus, else the camera) - not on
    /// a dedicated server, where nobody plays there (`players_only`) - and every other LAN
    /// player's.
    pub fn anchors(&self) -> impl Iterator<Item = DVec3> + '_ {
        (!self.players_only)
            .then_some(self.center)
            .into_iter()
            .chain(self.lan_centers.iter().copied())
    }

    /// Is `p` further than `r` from us and from every other LAN player?
    pub fn far_from_players(&self, p: DVec3, r: f64) -> bool {
        self.anchors().all(|c| (p - c).length() > r)
    }

    /// Density used when filling pedestrian places. Never inflate a normal
    /// graphical session, only the population owned by a headless host.
    pub fn focused_density(&self) -> f32 {
        let bonus = if self.players_only {
            self.lan_population_boost.clamp(1.0, 3.0)
        } else {
            1.0
        };
        (self.density.max(0.0) * bonus).clamp(0.0, 3.0)
    }

    /// The stops and pavements around the other players of a LAN session (host).
    pub fn populate_lan_centers(&mut self, world: &dyn World, net: &Network) {
        if self.lan_centers.is_empty() {
            return;
        }
        let mine = self.center;
        // Round-robin the starting player each second so the first peer in
        // the LAN table cannot monopolize the bounded population pool.
        let mut centers = self.lan_centers.clone();
        let offset = (self.time.max(0.0) as usize) % centers.len();
        centers.rotate_left(offset);
        for c in centers {
            // (near us they are there already - unless nobody plays here)
            if !self.players_only && (c - mine).length() < 150.0 {
                continue;
            }
            self.populate_with(world, Some(net), c);
            self.populate_on_foot(world, net, 1.0);
        }
        self.center = mine;
    }

    /// Everybody within `radius` of `near` the clients may see (host): on foot, waiting at
    /// a stop, or aboard a timetable bus - not the riders of our own bus, which the others
    /// see from outside only.
    pub fn lan_people(&self, near: DVec3, radius: f64) -> Vec<LanPerson> {
        self.lan_people_selected(near, radius, 0..self.people.len())
    }

    /// A caller with an XY spatial broad-phase can pass candidate indices.
    /// The same exact 3D check, identity and original list order apply.
    /// Legacy graphical LAN still calls lan_people without a spatial index.
    pub fn lan_people_subset(&self, near: DVec3, radius: f64, indices: &[usize]) -> Vec<LanPerson> {
        self.lan_people_selected(near, radius, indices.iter().copied())
    }

    fn lan_people_selected(
        &self,
        near: DVec3,
        radius: f64,
        indices: impl Iterator<Item = usize>,
    ) -> Vec<LanPerson> {
        let r2 = radius * radius;
        indices
            .filter_map(|i| self.people.get(i))
            .filter(|p| p.puppet.is_none() && !p.remote)
            .filter(|p| (p.position - near).length_squared() < r2)
            .filter_map(|p| {
                let aboard = match p.place {
                    Place::Bus(BusId::Player, _) => return None,
                    Place::Bus(BusId::Ai(bus), l) => Some((
                        bus,
                        l,
                        p.lheading,
                        match &p.state {
                            State::Pax(x) if x.task == Task::SittingInBus => x.seat,
                            _ => None,
                        },
                    )),
                    Place::Ground => None,
                };
                let waiting = match &p.state {
                    State::Pax(x) if aboard.is_none() && x.task == Task::WaitingForBus => {
                        x.stop.zip(x.spot)
                    }
                    _ => None,
                };
                Some(LanPerson {
                    id: p.id,
                    ty: p.ty.clone(),
                    pos: p.position,
                    heading: p.heading,
                    speed: if aboard.is_some() {
                        0.0
                    } else {
                        p.vel.length()
                    },
                    activity: p.activity,
                    aboard,
                    waiting,
                })
            })
            .collect()
    }

    /// Hand over waiting people while retaining their complete identity and
    /// original stop in a native host escrow until BOARD or RETURN arrives.
    /// Only the headless host retains these snapshots; graphical hosts may
    /// continue using the legacy hand_over function below.
    pub fn hand_over_saved(&mut self, player: u32, ids: &[u32]) -> Vec<HandedPerson> {
        let mut out = Vec::new();
        for id in ids {
            let Some(i) = self.people.iter().position(|p| p.id == *id) else {
                continue;
            };
            if !matches!(&self.people[i].state, State::Pax(x) if x.task == Task::WaitingForBus)
                || self.people[i].remote
            {
                continue;
            }
            let (stop, spot) = self.pax(i).map_or((None, None), |p| (p.stop, p.spot));
            let Some(known_stop) = stop.filter(|stop| self.stops.contains_key(stop)) else {
                // No safe location to restore this person if the client
                // later abandons its bus. Refuse a non-restorable GRANT.
                continue;
            };
            self.handed.push((known_stop, remote_bus_id(player)));
            self.release(i);
            let person = self.people.swap_remove(i);
            self.retire(&person);
            out.push(HandedPerson {
                owner: player,
                person,
                stop,
                spot,
            });
        }
        out
    }

    /// LAN play (host): the stops player `player`'s bus has stood at and now left: more than
    /// 60 m behind it, as Omsi.exe counts a bus past a station (0x7d5124). Each is told once.
    pub fn remote_stops_left(&mut self, player: u32) -> Vec<i64> {
        let id = remote_bus_id(player);
        let Some(bus) = self.remote_now.iter().find(|b| b.id == BusId::Ai(id)) else {
            self.remote_stood.retain(|s| s.0 != id);
            return Vec::new();
        };
        let mut left = Vec::new();
        for (&stop, s) in &self.stops {
            if (bus.pos - s.pos).length() < 60.0 {
                if bus.speed.abs() < 0.5 {
                    self.remote_stood.insert((id, stop));
                }
            } else if self.remote_stood.remove(&(id, stop)) {
                left.push(stop);
            }
        }
        left.sort_unstable();
        left
    }

    /// The original graphical host does not hold a native escrow.
    pub fn hand_over(&mut self, player: u32, ids: &[u32]) -> Vec<u32> {
        self.hand_over_saved(player, ids)
            .into_iter()
            .map(|saved| saved.person.id)
            .collect()
    }

    /// Restore a cancelled but never boarded person at the original platform.
    /// Do not reuse the occupied waiting spot if another person took it while
    /// the claimant was away. ID and human type remain unchanged.
    pub fn restore_unboarded(&mut self, mut saved: HandedPerson) -> bool {
        if self.people.iter().any(|p| p.id == saved.person.id) {
            return false;
        }
        let Some(stop) = saved.stop.filter(|stop| self.stops.contains_key(stop)) else {
            // A streamed-out stop cannot safely receive a pedestrian; a
            // future full streaming unload must retain a suspended queue.
            return false;
        };
        let original_available = saved.spot.filter(|&spot| {
            self.stops
                .get(&stop)
                .and_then(|s| s.taken.get(spot))
                .is_some_and(|taken| !*taken)
        });
        let spot = if let Some(index) = original_available {
            if let Some(taken) = self
                .stops
                .get_mut(&stop)
                .and_then(|s| s.taken.get_mut(index))
            {
                *taken = true;
            }
            Some(index)
        } else {
            self.take_spot(stop)
        };
        if let Some(pax) = match &mut saved.person.state {
            State::Pax(pax) => Some(pax),
            _ => None,
        } {
            pax.task = Task::WaitingForBus;
            pax.stop = Some(stop);
            pax.spot = spot;
            pax.bus = None;
            pax.inside = None;
            if let Some(place) = spot.and_then(|index| self.stops[&stop].spots.get(index)) {
                pax.pos = place.pos;
                pax.yaw = place.face.to_radians();
                saved.person.position = place.pos;
                saved.person.heading = place.face;
            }
        }
        saved.person.place = Place::Ground;
        if let Some(index) = self
            .handed
            .iter()
            .rposition(|(s, bus)| *s == stop && *bus == remote_bus_id(saved.owner))
        {
            self.handed.swap_remove(index);
        }
        self.bodies.ops.push(BodyOp::Spawn {
            id: saved.person.id,
            ty: saved.person.ty.clone(),
            variant: saved.person.variant,
            position: saved.person.position,
        });
        self.people.push(saved.person);
        true
    }

    /// Draw the host's people from now on (`on`), or simulate our own again. Everybody who
    /// is not getting on, riding or getting off our bus goes (the host's come instead; the
    /// host's copies cannot walk on by themselves).
    pub fn set_mirror(&mut self, on: bool) {
        if self.mirror == on {
            return;
        }
        self.mirror = on;
        let keep = |p: &Person| !p.remote && p.state.bus() == Some(BusId::Player);
        let mut i = 0;
        while i < self.people.len() {
            if keep(&self.people[i]) || self.people[i].puppet.is_some() {
                i += 1;
                continue;
            }
            self.release(i);
            let p = self.people.swap_remove(i);
            self.retire(&p);
        }
        // our own people from now on are numbered far above the host's (those riding with
        // us already too)
        if on {
            self.next_id = self.next_id.max(1 << 30);
            for p in self.people.iter_mut().filter(|p| p.puppet.is_none()) {
                self.bodies.ops.push(BodyOp::Renamed {
                    from: p.id,
                    to: self.next_id,
                });
                p.id = self.next_id;
                self.next_id += 1;
            }
        }
        for s in self.stops.values_mut() {
            for t in s.taken.iter_mut() {
                *t = false;
            }
        }
        self.claims_out.clear();
        self.claimed.clear();
        self.mirror_wait.clear();
    }

    /// Drop the previous host's people and outstanding claims on reconnection.
    pub fn reset_mirrored_people(&mut self) {
        let mut i = 0;
        while i < self.people.len() {
            if self.people[i].remote {
                let person = self.people.swap_remove(i);
                self.retire(&person);
            } else {
                i += 1;
            }
        }
        self.claims_out.clear();
        self.claimed.clear();
        self.mirror_wait.clear();
    }

    /// The human type of a file relative to a content root (`Humans/…/x.hum`).
    pub fn type_by_file(&self, file: &str) -> Option<usize> {
        let want = file.replace('\\', "/").to_ascii_lowercase();
        self.types.iter().position(|t| {
            t.def
                .path
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
                .ends_with(&want)
        })
    }

    /// The file of a human type relative to its content root (`Humans/…/x.hum`).
    pub fn type_file(ty: &HumanType) -> String {
        let p = ty.def.path.to_string_lossy().replace('\\', "/");
        match p.to_ascii_lowercase().rfind("/humans/") {
            Some(k) => p[k + 1..].to_string(),
            None => p,
        }
    }

    /// One of the host's people appears here (client).
    pub fn mirror_add(&mut self, world: &dyn World, id: u32, ty: usize, pose: &MirrorPose) -> bool {
        if self.people.iter().any(|p| p.id == id) {
            return false;
        }
        let state = State::Idle;
        let Some(i) = self.spawn_as(world, pose.pos, pose.heading, state, Some(ty)) else {
            return false;
        };
        self.next_id -= 1;
        let p = &mut self.people[i];
        self.bodies.renamed(p.id, id);
        p.id = id;
        p.anim = OmsiAnim::default();
        p.remote = true;
        self.mirror_set(id, pose);
        true
    }

    /// Where one of the host's people is this frame (client).
    pub fn mirror_set(&mut self, id: u32, pose: &MirrorPose) {
        let Some(p) = self.people.iter_mut().find(|p| p.id == id && p.remote) else {
            return;
        };
        p.position = pose.pos;
        p.heading = pose.heading;
        p.vel = pose.vel;
        p.activity = pose.activity;
        match pose.waiting {
            Some(w) => {
                self.mirror_wait.insert(id, w);
            }
            None => {
                self.mirror_wait.remove(&id);
            }
        }
        match pose.aboard {
            Some((bus, local, lheading, seat)) => {
                p.place = Place::Bus(BusId::Ai(bus), local);
                p.lheading = lheading;
                p.vel = DVec2::ZERO;
                p.remote_seat = if pose.activity == Activity::Sit {
                    seat
                } else {
                    None
                };
            }
            None => {
                p.place = Place::Ground;
                p.remote_seat = None;
            }
        }
    }

    /// One of the host's people has gone (client).
    pub fn mirror_remove(&mut self, id: u32) {
        if let Some(i) = self.people.iter().position(|p| p.id == id && p.remote) {
            let p = self.people.swap_remove(i);
            self.retire(&p);
        }
        self.claimed.remove(&id);
        self.mirror_wait.remove(&id);
    }

    /// A remote person this frame: they stand where the host put them.
    pub fn mirror_want(&mut self, i: usize, _buses: &[BusNow]) -> Want {
        Want::stand(None, self.people[i].activity)
    }

    /// The type of one of our people (host).
    pub fn lan_people_by_id(&self, id: u32) -> Option<Arc<HumanType>> {
        self.people
            .iter()
            .find(|p| p.id == id && !p.remote)
            .map(|p| p.ty.clone())
    }

    /// Where the host's people are drawn (client; `OMSI_LAN_TRACE`).
    pub fn mirror_positions(&self) -> Vec<(u32, DVec3)> {
        self.people
            .iter()
            .filter(|p| p.remote && p.place == Place::Ground)
            .map(|p| (p.id, p.position))
            .collect()
    }

    /// Waiting people to ask the host for (client).
    pub fn take_claims(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.claims_out)
    }

    /// The host handed this waiting person over to our bus (client). (The people of a LAN
    /// game stay the host's: nothing is taken over.)
    pub fn grant(&mut self, id: u32) -> bool {
        self.claimed.remove(&id);
        let Some((stop, spot)) = self.mirror_wait.remove(&id) else {
            return false;
        };
        let Some(i) = self.people.iter().position(|p| p.id == id && p.remote) else {
            return false;
        };
        if !self.stops.contains_key(&stop) {
            return false;
        }
        // ours from now on: waiting at that place, for the bus that stands there (the first
        // listed, as Omsi.exe takes it without a line record)
        let sp = self.stops[&stop].spots.get(spot).cloned();
        let seatheight = self.people[i].ty.def.seat_height;
        let walk = 1.1 + (self.rand_f() as f32 * 2.0 - 1.0) * 0.2;
        let mut pax = Pax::new(walk, self.rand_f());
        pax.task = Task::WaitingForBus;
        pax.stop = Some(stop);
        // what a waiting person of ours has (sub_626044 and task 6): a destination drawn
        // from the stop (both sides load the same map) and a distance to ride without one;
        // with neither they would get off again at once (#813)
        let (dest, line) = self.draw_dest(stop);
        pax.dest = dest;
        pax.line = line;
        pax.ride_km = self.rand_f() as f32 * 19.0 + 1.0;
        pax.pos = self.people[i].position;
        pax.yaw = self.people[i].heading.to_radians();
        if let Some(sp) = sp {
            pax.spot = Some(spot);
            if let Some(t) = self.stops.get_mut(&stop).unwrap().taken.get_mut(spot) {
                *t = true;
            }
            if sp.height != 0.0 {
                pax.seat_h = sp.height;
                pax.pos = sp.pos - DVec3::Z * seatheight as f64;
                pax.pax_state = 2.0;
            }
            pax.yaw = sp.face.to_radians();
        }
        let p = &mut self.people[i];
        p.remote = false;
        p.state = State::Pax(Box::new(pax));
        true
    }

    /// The host's people waiting at the stop our bus is listed at (client): ask for them.
    pub fn claim_waiting(&mut self) {
        if !self.mirror {
            return;
        }
        let now = self.time;
        self.claimed.retain(|_, t| now - *t < 10.0);
        let at: Vec<i64> = self
            .stops
            .iter()
            .filter(|(_, s)| s.buses.iter().any(|b| b.0 == BusId::Player))
            .map(|(id, _)| *id)
            .collect();
        if at.is_empty() {
            return;
        }
        for (id, (stop, _)) in &self.mirror_wait {
            if at.contains(stop) && !self.claimed.contains_key(id) {
                self.claimed.insert(*id, now);
                self.claims_out.push(*id);
            }
        }
    }
}

#[cfg(test)]
mod native_focus_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn headless_population_has_no_phantom_host_player() {
        let mut people = PeopleSim::new(Path::new("."), 16);
        people.center = DVec3::ZERO;
        people.players_only = true;
        people.lan_centers = vec![DVec3::new(5_000.0, 0.0, 0.0)];
        let centers: Vec<_> = people.anchors().collect();
        assert_eq!(centers, people.lan_centers);
        assert!(people.far_from_players(DVec3::ZERO, 450.0));
        assert!(!people.far_from_players(centers[0], 450.0));
    }

    #[test]
    fn headless_focus_only_boosts_remote_player_population() {
        let mut people = PeopleSim::new(Path::new("."), 16);
        people.density = 1.0;
        people.lan_population_boost = 1.75;
        assert_eq!(people.focused_density(), 1.0);
        people.players_only = true;
        assert_eq!(people.focused_density(), 1.75);
        people.density = 2.5;
        assert_eq!(people.focused_density(), 3.0);
        people.density = 0.0;
        assert_eq!(people.focused_density(), 0.0);
    }
}
