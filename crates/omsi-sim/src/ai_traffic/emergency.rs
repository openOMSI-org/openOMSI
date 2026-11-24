//! Emergency vehicles on an active drive, and the traffic making way for them.
//!
//! A vehicle is on an active drive while its script sets `TrafficPriority` above a half
//! (Omsi.exe reads it so every tick, 0x7d531b) and it is no timetable vehicle - a timetable
//! bus's scripts use the variable for something else. The player's vehicle and a LAN
//! player's are emergency vehicles by the same variable of theirs.
//!
//! Omsi.exe has the cars in front of one drive on as if they had priority themselves,
//! through red lights and over the limit; nobody pulls over. Here the drivers do what the
//! road traffic rules ask of them instead:
//! - only the vehicles on the emergency vehicle's actual way ahead react - its lane and the
//!   parallel lanes of the same carriageway, within `horizon` of its front - not a road that
//!   only runs nearby; a car is let go once the emergency vehicle has passed it;
//! - they make way (`emergency_way`, through the one owner of sideways moves, `lateral`): on a
//!   road of several lanes the innermost lane moves inward and the others outward, which
//!   opens a corridor between the innermost lane and the one beside it; on a road of one lane
//!   the car moves to the kerb, within its lane. The whole move is checked first (bodies,
//!   people, scenery); the car rolls at walking pace while it moves over and then waits. A
//!   car in a junction or on a ring drives on and clears the way;
//! - the emergency vehicle takes the corridor once the bodies have made room (`corridor`),
//!   and goes round a car standing aside for it on a road of one lane over the other half of
//!   the road, waiting for the oncoming traffic like anybody else (`planning::plan_pass`);
//! - a junction it comes to (within `RESERVE_REACH`, or one it is in) is kept for it: the
//!   lanes of the junction's object and those crossing them (`Reservation`). Whoever has
//!   not claimed a way through it yet waits at the entry - but for the vehicles ahead of it
//!   on its way, which lead it through (and cross with it), and the traffic on a ring. It
//!   may cross against a red light on the movement kept for it, at `CRAWL` at most; it still
//!   waits for whoever is in the junction already or has decided to go, for people on the
//!   crossing and for room on its exit, however long that takes. The junction is kept until
//!   its rear (its last section's) is out, or until it turns away, goes or the network
//!   changes.
//! - its speed: the limit times `EMERGENCY_LIMIT_FACTOR`, at most `EMERGENCY_LIMIT_PLUS`
//!   km/h over it, within its own top speed (`AiState::priority_drive`).
//!
//! Everything is worked out once at the start of a step from where everybody stands then
//! (`emergency_begin_step`), so neither the order the cars plan in nor the frame rate
//! changes who makes way.

use super::*;
use crate::traffic::{EMERGENCY_LIMIT_FACTOR, EMERGENCY_LIMIT_PLUS};

/// The cars ahead react within the emergency vehicle's speed times this many seconds ...
pub const HORIZON_TIME: f32 = 6.0;
/// ... plus this many metres, ...
pub const HORIZON_EXTRA: f32 = 20.0;
/// ... at least and at most this far from its front (m).
pub const HORIZON_MIN: f32 = 30.0;
pub const HORIZON_MAX: f32 = 100.0;
/// A car is let go once the emergency vehicle's front is this far past its own (m), or its
/// rear is past it if that is further.
pub const PASSED: f32 = 12.0;
/// A junction is kept for an emergency vehicle from this far before its entry (m) ...
pub const RESERVE_REACH: f32 = 45.0;
/// ... and kept while it is no further away than this (slowing down for something moves it
/// back a little).
pub const RESERVE_KEEP: f32 = 65.0;
/// A junction is kept only for an emergency vehicle that comes: one that has stood this
/// long before its entry (s) lets it go - at a red light, at a stop, in a queue - and gets it
/// again once it moves (#2431). Kept for one standing, a junction stood still at every green
/// for as long as a siren - or a player's bus whose script sets `TrafficPriority` - waited
/// within `RESERVE_KEEP` of it (Omsi.exe keeps no junction for anybody).
/// An AI one standing at its line (held by the junction itself: somebody in it, people on
/// its crossing, a full exit) keeps it until `STUCK_RELEASE_ALL`.
pub const STANDING_LETS_GO: f32 = 5.0;
/// An emergency vehicle whose front is this close to a junction's entry (m) stands at its line.
pub const AT_LINE: f32 = 3.0;
/// Against a stop aspect the emergency vehicle crosses at this speed at most (m/s).
pub const CRAWL: f32 = 5.0;
/// A car moving over for an emergency vehicle rolls at this speed at most (m/s).
pub const WALK: f32 = 1.0;
/// `TrafficPriorityWarningNeeded` is set for a vehicle this close ahead (m, FUN_007168a0).
pub const WARN_GAP: f32 = 50.0;
/// An emergency vehicle that has stood this long lets go the cars waiting for it that it
/// cannot get past (s): they drive on, and it follows. Nobody waits for one that will not
/// come by. (3 s and 8 s let a car making way drive on while the other lane's traffic still
/// came, and the siren never got past it - kept at 8 and 30 until the release is taken back
/// once the way is clear.)
pub const STUCK_RELEASE: f32 = 8.0;
/// ... and after this long every car waiting for it.
pub const STUCK_RELEASE_ALL: f32 = 30.0;
/// The id a LAN player's emergency vehicle goes by (`u64::MAX - 1 - its session id`); the
/// player's own is `stats::WAITS_ON_PLAYER`.
pub fn lan_siren_id(session: u32) -> u64 {
    u64::MAX - 1 - session as u64
}

/// Is car `c` an emergency vehicle on an active drive (see the module)?
pub fn active(c: &AiCar) -> bool {
    !c.is_bus() && c.vehicle.var("TrafficPriority").is_some_and(|v| v > 0.5)
}

/// How far ahead of an emergency vehicle at `speed` m/s the cars react (m from its front).
pub fn horizon(speed: f32) -> f32 {
    (speed.max(0.0) * HORIZON_TIME + HORIZON_EXTRA).clamp(HORIZON_MIN, HORIZON_MAX)
}

/// The speed an emergency vehicle drives at under the lane's posted `limit` (km/h) on an
/// active drive, with its own top speed `max` (km/h).
pub fn bonus_limit(limit: f32, max: f32) -> f32 {
    (limit * EMERGENCY_LIMIT_FACTOR)
        .min(limit + EMERGENCY_LIMIT_PLUS)
        .min(max)
}

/// An emergency vehicle on an active drive as this step sees it.
#[derive(Debug, Clone)]
pub struct Siren {
    pub id: u64,
    /// Its index among the cars (None: the player's vehicle or a LAN player's).
    pub car: Option<usize>,
    /// Its way ahead: (lane, from its front to the lane's start - negative for the lane it
    /// is on). The player's covers every branch it may take.
    pub way: Vec<(usize, f32)>,
    pub speed: f32,
    pub stopped: f32,
    /// The AI drives it (the player's vehicle and a LAN player's stand where their drivers
    /// stop them: for a light, at a stop).
    pub ai: bool,
    /// Where across its lane it will be (m, + right), and the corridor it can take there.
    pub lateral: f32,
    pub corridor: Option<f32>,
    pub half_width: f32,
    /// From its front to its rear, the last section's included (m).
    pub length: f32,
    pub pos: DVec3,
    /// How far its body reaches behind its origin (m), and its odometer (the player's: 0).
    pub tail: f32,
    pub odometer: f32,
}

impl Siren {
    /// Has it stood before a junction `entry` m off (from its front) long enough to let it go
    /// (`STANDING_LETS_GO`)? An AI one standing at the line waits for the junction itself
    /// (it may cross the red light there): it lets go only after `STUCK_RELEASE_ALL`.
    pub fn lets_go(&self, entry: f32) -> bool {
        let at_line = self.ai && entry < AT_LINE;
        self.stopped > STANDING_LETS_GO && (!at_line || self.stopped > STUCK_RELEASE_ALL)
    }
}

/// What a car is asked this step: to make way for siren `from`, whose front is `gap` m
/// behind its rear (negative alongside). `side`: 0 on the siren's own way, -1 / 1 on the
/// lanes to its left / right; `adjacent`: right beside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Request {
    pub from: u64,
    pub gap: f32,
    pub side: i8,
    pub adjacent: bool,
}

/// A car making way: for whom, to where across its lane, the S-curve it moves along, and
/// where it stands once over (odometer). `drive_on`: the emergency vehicle could not get
/// past it standing there, so it keeps going at its new place across the lane. `vetoed`:
/// the way over was not clear when it looked (`time`): it looks again a second later.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MakeWay {
    pub from: u64,
    pub target: f32,
    pub ramp: (f32, f32, f32, f32),
    pub hold: Option<f32>,
    pub drive_on: bool,
    pub vetoed: bool,
    pub time: f32,
}

/// A junction kept for an emergency vehicle (see the module).
#[derive(Debug, Clone)]
pub struct Reservation {
    pub owner: u64,
    pub object: ((i32, i32), i64),
    /// The lanes kept (sorted): the object's, and those crossing them.
    pub group: Vec<usize>,
    /// The owner's way through it.
    pub movement: Vec<usize>,
    /// The lanes a car must not drive into while it is kept (sorted): the owner's way
    /// through and the lanes that cross or join it. A car whose way through the junction
    /// meets none of them (a right turn on the far side) goes on.
    pub blocks: Vec<usize>,
    /// It crosses against a stop aspect: at `CRAWL` until the reservation ends.
    pub crawl: bool,
    /// Its body has been on the lanes kept, and since where it is off them (odometer, or for
    /// the player's vehicle where it stood).
    pub entered: bool,
    pub left_at: Option<f32>,
    pub left_pos: Option<DVec3>,
    /// The vehicles ahead of it on its way, which lead it through (ids).
    pub leaders: Vec<u64>,
}

#[derive(Debug, Default)]
pub struct Emergency {
    /// This step's emergency vehicles, by id.
    pub sirens: Vec<Siren>,
    /// Who makes way for whom this step (by car id).
    pub requests: HashMap<u64, Request>,
    pub reservations: Vec<Reservation>,
    /// The emergency vehicles that cross against a stop aspect this step (ids).
    pub ran_red: Vec<u64>,
    /// (car, siren): let go because the siren stood behind it (`STUCK_RELEASE`), until the
    /// siren has passed it.
    released: Vec<(u64, u64)>,
    /// The lanes of each junction object, for `lanes_generation` `generation`.
    objects: HashMap<((i32, i32), i64), Vec<usize>>,
    generation: u64,
}

impl Emergency {
    pub fn is_siren(&self, id: u64) -> bool {
        self.sirens.binary_search_by_key(&id, |s| s.id).is_ok()
    }

    pub fn siren(&self, id: u64) -> Option<&Siren> {
        self.sirens
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|k| &self.sirens[k])
    }

    /// Does `owner` hold the junction whose first lane on its way is `lane`?
    pub fn holds_movement(&self, owner: u64, lane: usize) -> bool {
        self.reservations
            .iter()
            .any(|r| r.owner == owner && r.movement.contains(&lane))
    }

    /// Does vehicle `id` cross against a red light under a reservation now (the emergency
    /// vehicle, or one leading it through)?
    pub fn crawling(&self, id: u64) -> bool {
        self.reservations
            .iter()
            .any(|r| r.crawl && (r.owner == id || r.leaders.contains(&id)))
    }
}

/// Do two sorted lists share an element?
fn meet(a: &[usize], b: &[usize]) -> bool {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => return true,
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    false
}

impl TrafficSim {
    /// This step's emergency vehicles, who makes way for whom, and the junctions kept: from
    /// where everybody stands as the step begins.
    pub fn emergency_begin_step(&mut self, ts: &TickScene) {
        let mut em = std::mem::take(&mut self.emergency);
        if em.generation != self.lanes_generation {
            // (the lanes were renumbered or added to: what was kept by lane is stale)
            em.reservations.clear();
            em.objects.clear();
            em.generation = self.lanes_generation;
        }
        em.ran_red.clear();
        em.sirens = self.sirens(ts);
        if em.sirens.is_empty() {
            em.requests.clear();
            em.reservations.clear();
            em.released.clear();
            self.emergency = em;
            return;
        }
        self.keep_reservations(&mut em);
        self.grant_reservations(&mut em);
        self.find_leaders(&mut em, &ts.by_lane);
        self.courtesy_requests(&mut em, &ts.by_lane);
        self.emergency = em;
    }

    /// Every emergency vehicle on an active drive (sorted by id).
    fn sirens(&self, ts: &TickScene) -> Vec<Siren> {
        let mut out = Vec::new();
        for (k, c) in self.cars.iter().enumerate() {
            if self.net.lanes[c.state.lane].kind != LaneKind::Street || !active(c) {
                continue;
            }
            let st = &c.state;
            let reach = horizon(st.speed).max(RESERVE_KEEP) + st.front + 30.0;
            let way = self
                .way_lanes(st, reach)
                .into_iter()
                .map(|(l, d)| (l, d - st.front))
                .collect();
            out.push(Siren {
                id: c.id,
                car: Some(k),
                way,
                speed: st.speed,
                stopped: c.stopped,
                ai: true,
                lateral: st.lateral_target,
                corridor: self.corridor_offset(st.lane),
                half_width: c.half_width,
                length: st.front + c.tail,
                pos: c.vehicle.position,
                tail: c.tail,
                odometer: st.odometer,
            });
        }
        let mut user = |id: u64, b: &PlayerBox, blinker: u8, still: f32| {
            if let Some(u) = way_user_on(&self.net, b, blinker, still, true) {
                out.push(Siren {
                    id,
                    car: None,
                    way: u.lanes.iter().map(|&(l, d)| (l, d - u.half_len)).collect(),
                    speed: u.speed,
                    stopped: still,
                    ai: false,
                    lateral: 0.0,
                    corridor: None,
                    half_width: b.3,
                    length: 2.0 * u.half_len,
                    pos: b.0,
                    tail: u.half_len,
                    odometer: 0.0,
                });
            }
        };
        if self.player_priority {
            if let Some(p) = ts.player.as_ref() {
                user(
                    super::stats::WAITS_ON_PLAYER,
                    p,
                    self.player_blinker,
                    ts.player_standing,
                );
            }
        }
        for (id, b) in ts
            .others
            .iter()
            .filter(|(id, _)| *id < 0xFFF0_0000 && self.other_priority.contains(id))
        {
            let still = self.others_still.get(id).copied().unwrap_or(0.0);
            user(
                lan_siren_id(*id),
                b,
                self.other_blinkers.get(id).copied().unwrap_or(0),
                still,
            );
        }
        out.sort_by_key(|s| s.id);
        out
    }

    /// The corridor on a road of several lanes from lane `l`: the offset (m, + right) of the
    /// line between the innermost lane and the one beside it, from `l`'s middle, when `l` is
    /// one of the two.
    pub fn corridor_offset(&self, l: usize) -> Option<f32> {
        let lane = &self.net.lanes[l];
        // (the inner side is the left, or the right on a left-hand-traffic map)
        let (inner, outer, sign) = if self.net.left_hand {
            (lane.right, lane.left, 1.0)
        } else {
            (lane.left, lane.right, -1.0)
        };
        let half = |o: usize| {
            let d = (self.net.lanes[o].offset - lane.offset).abs();
            0.5 * if d > 1.5 && d < 6.0 {
                d
            } else {
                0.5 * (lane.width.max(3.0) + self.net.lanes[o].width.max(3.0))
            }
        };
        match (inner, outer) {
            (None, Some(o)) => Some(-sign * half(o)),
            (Some(n), _) if self.net.lanes[n].kind == LaneKind::Street => {
                let nn = if self.net.left_hand {
                    self.net.lanes[n].right
                } else {
                    self.net.lanes[n].left
                };
                nn.is_none().then(|| sign * half(n))
            }
            _ => None,
        }
    }

    /// The lanes of the carriageway beside `l` (same direction), with their side (-1 left,
    /// 1 right) and whether they are right beside it.
    fn carriageway(&self, l: usize) -> Vec<(usize, i8, bool)> {
        let mut out = vec![(l, 0, true)];
        for side in [-1i8, 1] {
            let mut at = l;
            for k in 0..8 {
                let lane = &self.net.lanes[at];
                let Some(n) = (if side < 0 { lane.left } else { lane.right }) else {
                    break;
                };
                if out.iter().any(|e| e.0 == n) {
                    break;
                }
                out.push((n, side, k == 0));
                at = n;
            }
        }
        out
    }

    /// Who makes way for whom (see the module): the nearest emergency vehicle by gap, a tie
    /// to the lower id.
    fn courtesy_requests(&self, em: &mut Emergency, by_lane: &ByLane) {
        let mut req: HashMap<u64, Request> = HashMap::new();
        for s in &em.sirens {
            let h = horizon(s.speed);
            let mut seen: Vec<usize> = Vec::new();
            for &(l, d) in &s.way {
                if d > h || l >= self.net.lanes.len() {
                    continue;
                }
                for (m, side, adjacent) in self.carriageway(l) {
                    if seen.contains(&m) {
                        continue;
                    }
                    seen.push(m);
                    for &(j, sj, _, out) in by_lane.get(&m).map(|v| v.as_slice()).unwrap_or(&[]) {
                        let c = &self.cars[j];
                        // (the cars leading it through a junction kept for it drive on)
                        if out
                            || c.state.lane != m
                            || Some(j) == s.car
                            || c.is_rail()
                            || em.is_siren(c.id)
                            || em.reservations.iter().any(|r| r.leaders.contains(&c.id))
                        {
                            continue;
                        }
                        let along = d + if side == 0 {
                            sj
                        } else {
                            self.net.beside_s(m, l, sj)
                        };
                        let gap = along - c.state.rear;
                        // (alongside or ahead of its rear; one it is passing until it is by)
                        let was = c.make_way.is_some_and(|w| w.from == s.id);
                        let by = if was {
                            -(PASSED.max(s.length + 2.0))
                        } else {
                            -s.length
                        };
                        if gap > h || along + c.state.front < by {
                            continue;
                        }
                        let r = Request {
                            from: s.id,
                            gap,
                            side,
                            adjacent,
                        };
                        match req.get(&c.id) {
                            Some(o)
                                if o.gap.abs() < gap.abs()
                                    || (o.gap.abs() == gap.abs() && o.from < s.id) => {}
                            _ => {
                                req.insert(c.id, r);
                            }
                        }
                    }
                }
            }
        }
        // a siren that has stood for long lets go those it cannot get past (all of them
        // after longer): they drive on rather than wait for one that does not come
        let mut released: Vec<(u64, u64)> = em
            .released
            .iter()
            .copied()
            .filter(|p| req.get(&p.0).is_some_and(|r| r.from == p.1))
            .collect();
        for s in em.sirens.iter().filter(|s| s.stopped > STUCK_RELEASE) {
            let mut ids: Vec<u64> = req
                .iter()
                .filter(|(_, r)| r.from == s.id)
                .map(|(&id, _)| id)
                .collect();
            ids.sort_unstable();
            for id in ids {
                let Some(&k) = self.index_of.get(&id) else {
                    continue;
                };
                let c = &self.cars[k];
                let target = c
                    .make_way
                    .filter(|w| w.from == s.id)
                    .map_or(c.state.lateral, |w| w.target);
                let blocks = c.make_way.is_none_or(|w| w.drive_on)
                    || !self.passable(c.state.lane, c.state.s, target, c.half_width, &req[&id], s);
                if (blocks || s.stopped > STUCK_RELEASE_ALL) && !released.contains(&(id, s.id)) {
                    released.push((id, s.id));
                }
            }
        }
        for p in &released {
            if req.get(&p.0).is_some_and(|r| r.from == p.1) {
                req.remove(&p.0);
            }
        }
        released.sort_unstable();
        em.released = released;
        em.requests = req;
    }

    /// Can siren `s` get past a car (`half_width`) standing on `lane` at `at`, `target` m
    /// across it (where it moves to)? By the lateral room the planners judge a lead by
    /// (`obstacle_from`), or, on a road of one lane, over the other half of the road.
    fn passable(
        &self,
        lane_i: usize,
        at: f32,
        target: f32,
        half_width: f32,
        r: &Request,
        s: &Siren,
    ) -> bool {
        let lane = &self.net.lanes[lane_i];
        let need = half_width + s.half_width + 0.3;
        if lane.left.is_none() && lane.right.is_none() {
            // (a road of one lane: round it on the other half, `plan_pass` - not in a
            // junction, nor right after one: there it drives on, out of the way)
            let clear_behind = at > 30.0
                || self.net.prev.get(lane_i).is_none_or(|p| {
                    p.iter()
                        .all(|&b| self.net.crossings[b].is_empty() && self.net.walks[b].is_empty())
                });
            return clear_behind
                && self.net.opposite(lane_i, at).is_some_and(|(o, _, side)| {
                    side >= 2.3 && !super::planning::divided_carriageway(&self.net, lane_i, o, side)
                });
        }
        let e_lat = s.corridor.unwrap_or(s.lateral);
        match r.side {
            0 => (target - e_lat).abs() >= need,
            side if r.adjacent && e_lat * side as f32 > 0.5 => {
                // (it takes the corridor towards this lane: the room is half the lanes'
                // spacing and what this car moved away from it)
                e_lat.abs() + target.abs() >= need
            }
            _ => true,
        }
    }

    /// Keep or let go the junctions kept (see `Reservation`).
    fn keep_reservations(&self, em: &mut Emergency) {
        let sirens = std::mem::take(&mut em.sirens);
        em.reservations.retain_mut(|r| {
            let Some(s) = sirens.iter().find(|s| s.id == r.owner) else {
                return false;
            };
            let on = self.body_on(s, &r.group);
            if on {
                r.entered = true;
                if s.car
                    .is_some_and(|k| r.group.binary_search(&self.cars[k].state.lane).is_ok())
                {
                    r.left_at = None;
                    r.left_pos = None;
                }
            }
            if r.entered {
                // its origin off the lanes kept: until its rear (its last section's) is out
                let off = match s.car {
                    Some(k) => r.group.binary_search(&self.cars[k].state.lane).is_err(),
                    None => s
                        .way
                        .first()
                        .is_none_or(|w| r.group.binary_search(&w.0).is_err()),
                };
                if !off {
                    return true;
                }
                let gone = match s.car {
                    Some(_) => s.odometer - *r.left_at.get_or_insert(s.odometer) > s.tail + 0.5,
                    None => {
                        (s.pos - *r.left_pos.get_or_insert(s.pos))
                            .truncate()
                            .length() as f32
                            > s.tail + 1.0
                    }
                };
                return on || !gone;
            }
            // not in it yet: kept while it still comes (it may have turned away, or stand
            // before it - see `STANDING_LETS_GO`)
            let entry = s
                .way
                .iter()
                .find(|w| r.movement.contains(&w.0))
                .map(|w| w.1);
            entry.is_some_and(|e| e <= RESERVE_KEEP && !s.lets_go(e))
        });
        em.sirens = sirens;
    }

    /// The vehicles ahead of each owner of a kept junction on its way, up to the junction's
    /// far side: they lead it through (they are not held at the entry, and cross against
    /// the light with it).
    fn find_leaders(&self, em: &mut Emergency, by_lane: &ByLane) {
        for r in em.reservations.iter_mut() {
            let Some(s) = em.sirens.iter().find(|s| s.id == r.owner) else {
                continue;
            };
            let through = s
                .way
                .iter()
                .filter(|w| r.movement.contains(&w.0))
                .map(|w| w.1 + self.net.lanes[w.0].length())
                .fold(0.0f32, f32::max);
            let mut leaders = Vec::new();
            for &(l, d) in &s.way {
                if d > through {
                    continue;
                }
                for &(j, sj, _, out) in by_lane.get(&l).map(|v| v.as_slice()).unwrap_or(&[]) {
                    let c = &self.cars[j];
                    if !out
                        && Some(j) != s.car
                        && c.state.lane == l
                        && d + sj - c.state.rear > -0.5
                        && d + sj < through
                    {
                        leaders.push(c.id);
                    }
                }
            }
            leaders.sort_unstable();
            leaders.dedup();
            r.leaders = leaders;
        }
    }

    /// Is siren `s`'s body on any of `group`'s lanes?
    fn body_on(&self, s: &Siren, group: &[usize]) -> bool {
        let has = |l: usize| group.binary_search(&l).is_ok();
        match s.car {
            Some(k) => {
                let c = &self.cars[k];
                has(c.state.lane)
                    || c.state
                        .prev_lane
                        .is_some_and(|p| has(p) && c.state.s < c.tail + 1.0)
                    || c.tail_claims.iter().any(|t| has(t.0))
            }
            None => s.way.first().is_some_and(|w| has(w.0)),
        }
    }

    /// The lanes of junction object `key` (built once per network generation).
    fn object_lanes<'a>(&self, em: &'a mut Emergency, key: ((i32, i32), i64)) -> &'a [usize] {
        if em.objects.is_empty() {
            for (i, l) in self.net.lanes.iter().enumerate() {
                if let Some(k) = l.key.filter(|_| l.source == 2) {
                    em.objects.entry((k.tile, k.id)).or_default().push(i);
                }
            }
        }
        em.objects.get(&key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// New reservations: of the sirens within `RESERVE_REACH` of a junction (or in one), the
    /// one already in it first, then the lower id; none over lanes another one keeps.
    fn grant_reservations(&self, em: &mut Emergency) {
        let mut asks: Vec<(bool, u64, Reservation)> = Vec::new();
        let sirens = std::mem::take(&mut em.sirens);
        for s in &sirens {
            let Some(jn) = self.junction_ahead(&s.way) else {
                continue;
            };
            let entry = jn.lanes[0].1;
            if !jn.inside && (entry > RESERVE_REACH || s.lets_go(entry)) {
                continue;
            }
            let l0 = jn.lanes[0].0;
            let Some(k) = self.net.lanes[l0]
                .key
                .filter(|_| self.net.lanes[l0].source == 2)
            else {
                continue;
            };
            let object = (k.tile, k.id);
            if em
                .reservations
                .iter()
                .any(|r| r.owner == s.id && r.object == object)
            {
                continue;
            }
            let mut group: Vec<usize> = self.object_lanes(em, object).to_vec();
            let crossing: Vec<usize> = group
                .iter()
                .flat_map(|&l| self.net.crossings[l].iter().map(|c| c.other))
                .collect();
            group.extend(crossing);
            group.extend(jn.lanes.iter().map(|x| x.0));
            group.sort_unstable();
            group.dedup();
            let movement: Vec<usize> = jn.lanes.iter().map(|x| x.0).collect();
            let mut blocks: Vec<usize> = movement
                .iter()
                .flat_map(|&l| {
                    std::iter::once(l).chain(self.net.crossings[l].iter().map(|c| c.other))
                })
                .collect();
            blocks.sort_unstable();
            blocks.dedup();
            asks.push((
                jn.inside,
                s.id,
                Reservation {
                    owner: s.id,
                    object,
                    group,
                    movement,
                    blocks,
                    crawl: false,
                    entered: false,
                    left_at: None,
                    left_pos: None,
                    leaders: Vec::new(),
                },
            ));
        }
        em.sirens = sirens;
        asks.sort_by_key(|a| (!a.0, a.1));
        for (_, _, r) in asks {
            if em
                .reservations
                .iter()
                .any(|o| o.owner != r.owner && meet(&o.group, &r.group))
            {
                continue;
            }
            if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                log::info!(
                    "t={:.1}: junction {:?} kept for emergency vehicle {}",
                    self.time,
                    r.object,
                    r.owner
                );
            }
            em.reservations.push(r);
        }
    }

    /// Car `i` at junction `jn`: does a junction kept for an emergency vehicle hold it at its
    /// entry? Only where its way meets the emergency vehicle's (`Reservation::blocks`); not
    /// when it is in the junction already or has claimed its way through, leads the
    /// emergency vehicle through, or drives on a ring. The owner's id.
    pub fn emergency_entry_hold(&self, i: usize, jn: &Junction, committed: bool) -> Option<u64> {
        if self.emergency.reservations.is_empty() || jn.inside || committed {
            return None;
        }
        let c = &self.cars[i];
        if self.net.lanes[c.state.lane].ring {
            return None;
        }
        self.emergency
            .reservations
            .iter()
            .find(|r| {
                r.owner != c.id
                    && !r.leaders.contains(&c.id)
                    && jn
                        .lanes
                        .iter()
                        .any(|x| r.blocks.binary_search(&x.0).is_ok())
            })
            .map(|r| r.owner)
    }

    /// Car `j` stands aside for car `i` (an emergency vehicle) where it can get past: it is
    /// no queue on its exit.
    pub fn aside_for(&self, j: usize, i: usize) -> bool {
        self.cars[j]
            .make_way
            .is_some_and(|w| w.from == self.cars[i].id && !w.drive_on && w.hold.is_some())
    }

    /// The light ahead of car `i` (`light`, distance from its origin to the line): an
    /// emergency vehicle crosses against it on the movement kept for it, and so do the
    /// vehicles leading it through (crawling from then on). None when it goes.
    pub fn emergency_light(
        &mut self,
        i: usize,
        way: &[(usize, f32)],
        light: Option<f32>,
    ) -> Option<f32> {
        let d = light?;
        let id = self.cars[i].id;
        if self.emergency.reservations.is_empty() {
            return light;
        }
        let Some(&(lane, _)) = way.iter().find(|w| (w.1 - d).abs() < 0.01) else {
            return light;
        };
        let Some(r) = self.emergency.reservations.iter_mut().find(|r| {
            (r.owner == id && r.movement.contains(&lane))
                || (r.leaders.contains(&id) && r.group.binary_search(&lane).is_ok())
        }) else {
            return light;
        };
        if !r.crawl && omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
            log::info!(
                "t={:.1}: car {id} crosses against the light of lane {lane} for emergency vehicle {}",
                self.time,
                r.owner
            );
        }
        r.crawl = true;
        if r.owner == id {
            self.emergency.ran_red.push(id);
        }
        None
    }

    /// Car `i`'s speed for this step as an emergency vehicle (its bonus; crawling through a
    /// red light kept for it) or making way for one (walking pace while it moves over).
    pub fn emergency_speed(&mut self, i: usize, way: &[(usize, f32)]) {
        let id = self.cars[i].id;
        let siren = !self.emergency.sirens.is_empty() && self.emergency.is_siren(id);
        let car = &self.cars[i];
        let st = &car.state;
        let mut cap = car
            .make_way
            .filter(|w| !w.drive_on && (st.lateral - w.target).abs() > 0.05)
            .map(|_| WALK);
        // (the owner of a junction crossed against the light, and those leading it through,
        // from the speed that braking gently brings down to the crawl at the line)
        if let Some(r) = self
            .emergency
            .reservations
            .iter()
            .find(|r| r.crawl && (r.owner == id || r.leaders.contains(&id)))
        {
            let to_line = way
                .iter()
                .find(|w| r.group.binary_search(&w.0).is_ok())
                .map(|w| w.1 - st.front);
            if r.owner == id || to_line.is_some() {
                // (aimed half a metre a second under it: the speed follows the cap with a lag)
                let aim = CRAWL - 0.5;
                cap = Some(match to_line {
                    Some(d) if d > 0.0 => (aim * aim + 2.0 * 0.7 * st.decel * d).sqrt(),
                    _ => aim,
                });
            }
        }
        let st = &mut self.cars[i].state;
        st.priority_drive = siren;
        st.speed_cap = cap;
    }

    /// `TrafficPriorityWarningNeeded` for car `i`: an emergency vehicle that crosses against
    /// a red light now, or has a vehicle in its way within `WARN_GAP` (FUN_007168a0 clears it
    /// at every look ahead and sets it so).
    pub fn priority_warning(&self, i: usize, lead: Option<Lead>) -> bool {
        let id = self.cars[i].id;
        self.emergency.is_siren(id)
            && (self.emergency.ran_red.contains(&id) || lead.is_some_and(|l| l.gap < WARN_GAP))
    }

    /// Car `i` makes way for the emergency vehicle that asks it to (see the module): its
    /// place across the lane is set here. None when it is not asked (or drives on in a
    /// junction or on a ring); else where it stops (from its origin), None while it rolls.
    pub fn emergency_way(
        &mut self,
        i: usize,
        ts: &TickScene,
        sc: &super::lateral::SideScene,
    ) -> Option<Option<f32>> {
        let id = self.cars[i].id;
        let Some(req) = self.emergency.requests.get(&id).copied() else {
            self.cars[i].make_way = None;
            return None;
        };
        let car = &self.cars[i];
        let st = &car.state;
        let lane = &self.net.lanes[st.lane];
        // (a timetable bus's place across the lane is its service's)
        if lane.ring
            || !self.net.crossings[st.lane].is_empty()
            || car.is_bus()
            || lane.kind != LaneKind::Street
        {
            self.cars[i].make_way = None;
            return None;
        }
        let before = car.make_way.filter(|w| w.from == req.from);
        let mut w = match before.filter(|w| !(w.vetoed && self.time - w.time > 1.0)) {
            Some(w) => w,
            None => {
                let mut w = self.begin_make_way(i, req, ts, sc);
                // (still not clear: it stays where it stopped)
                if w.vetoed {
                    w.hold = before.and_then(|b| b.hold);
                }
                w
            }
        };
        let car = &mut self.cars[i];
        let st = &mut car.state;
        st.lateral_target = w.target;
        st.lateral_ramp = w.ramp;
        let over = (st.lateral - w.target).abs() <= 0.05;
        let mut stop = None;
        if over && !w.drive_on {
            let odo = st.odometer;
            let at = *w
                .hold
                .get_or_insert(odo + st.speed * st.speed / (2.0 * st.decel.max(1.0)) + 0.3);
            // (a stop point is kept 0.6 m off)
            stop = Some(st.front + (at - odo).max(0.0) + 0.6);
        }
        car.make_way = Some(w);
        Some(stop)
    }

    /// Where car `i` moves to for the siren of `req`, along which S-curve, and whether it
    /// waits there; the whole way over is checked first (a move that is not clear is not
    /// made: the car stays where it is across its lane).
    fn begin_make_way(
        &self,
        i: usize,
        req: Request,
        ts: &TickScene,
        sc: &super::lateral::SideScene,
    ) -> MakeWay {
        let car = &self.cars[i];
        let st = &car.state;
        let lane = &self.net.lanes[st.lane];
        let kerb = if self.net.left_hand { -1.0 } else { 1.0 };
        let inner = if self.net.left_hand {
            lane.right.is_none()
        } else {
            lane.left.is_none()
        };
        let several = lane.left.is_some() || lane.right.is_some();
        let dir = if several && inner { -kerb } else { kerb };
        // (within the lane: a path counts at least 3 m wide)
        let room = (lane.width.max(3.0) * 0.5 - car.half_width - 0.1).max(0.0);
        let mut target = dir * room;
        // over the S-curve of twice the speed (at least 2 m, and short of the car ahead)
        let ahead = sc.lead.map_or(50.0, |l| l.0.gap) - 0.5;
        let len = (2.0 * st.speed).max(2.0).min(ahead);
        let odo = st.odometer;
        let mut vetoed = false;
        if len < 1.0 || (target - st.lateral).abs() < 0.05 {
            vetoed = (target - st.lateral).abs() >= 0.05;
            target = st.lateral;
        } else {
            let mut hyp = st.clone();
            hyp.lateral_target = target;
            hyp.lateral_ramp = (st.lateral, target, odo, len);
            // (the cars ahead in its lane it keeps behind anyway)
            let skip: Vec<usize> = ts
                .by_lane
                .get(&st.lane)
                .into_iter()
                .flatten()
                .filter(|e| !e.3 && e.0 != i && e.1 > st.s)
                .map(|e| e.0)
                .collect();
            let parked = self.parked_along(i, len + st.front + 2.0);
            let net = &self.net;
            if let Some(what) = self.ghost_blocked(
                i,
                &|d| hyp.way_point(net, d),
                len + 1.0,
                &ts.feet,
                &skip,
                &parked,
            ) {
                if ts.debug {
                    log::info!(
                        "t={:.1}: car {} does not move aside for emergency vehicle {}: {what}",
                        self.time,
                        car.id,
                        req.from
                    );
                }
                target = st.lateral;
                vetoed = true;
            }
        }
        let ramp = if (target - st.lateral).abs() < 0.05 {
            (target, target, odo, 1.0)
        } else {
            (st.lateral, target, odo, len)
        };
        let drive_on = self
            .emergency
            .siren(req.from)
            .is_none_or(|s| !self.passable(st.lane, st.s, target, car.half_width, &req, s));
        let w = MakeWay {
            from: req.from,
            target,
            ramp,
            hold: None,
            drive_on,
            vetoed,
            time: self.time,
        };
        if ts.debug {
            log::info!(
                "t={:.1}: car {} makes way for emergency vehicle {} ({:.1} m behind): to {target:+.2} m{}",
                self.time,
                car.id,
                req.from,
                req.gap,
                if w.drive_on { ", driving on" } else { "" }
            );
        }
        w
    }

    /// The parked cars beside car `i`'s way within `len` m of its origin, as boxes.
    fn parked_along(&self, i: usize, len: f32) -> Vec<Obb> {
        let st = &self.cars[i].state;
        let mut out = Vec::new();
        for (l, d) in self.way_lanes(st, len) {
            for &(s, lat) in self.parked.get(&l).map(|v| v.as_slice()).unwrap_or(&[]) {
                if d + s < -6.0 || d + s > len + 6.0 {
                    continue;
                }
                let (q, h) = self.net.lanes[l].at(s);
                let hr = (h as f64).to_radians();
                let centre = q.truncate() + DVec2::new(hr.cos(), -hr.sin()) * lat as f64;
                out.push(Obb::vehicle(centre, h as f64, 2.3, 2.3, 0.9));
            }
        }
        out
    }

    /// An emergency vehicle (car `i`) on a road of several lanes takes the corridor once a
    /// car ahead of it has moved aside for it, if the way over is clear. True when it does.
    pub fn emergency_corridor(&mut self, i: usize, ts: &TickScene) -> bool {
        let id = self.cars[i].id;
        let Some(s) = self.emergency.siren(id) else {
            return false;
        };
        let Some(off) = s.corridor else { return false };
        let made_room = self.emergency.requests.iter().any(|(&cid, r)| {
            r.from == id
                && r.gap > -1.0
                && self
                    .index_of
                    .get(&cid)
                    .is_some_and(|&k| self.aside_for(k, i))
        });
        let st = &self.cars[i].state;
        if !made_room {
            return false;
        }
        if (st.lateral_target - off).abs() > 0.05 {
            let len = ((off - st.lateral).abs() * 8.0).clamp(8.0, 30.0);
            let mut hyp = st.clone();
            hyp.lateral_target = off;
            hyp.lateral_ramp = (st.lateral, off, st.odometer, len);
            let net = &self.net;
            if self
                .ghost_blocked(i, &|d| hyp.way_point(net, d), len, &ts.feet, &[], &[])
                .is_some()
            {
                return false;
            }
        }
        self.cars[i].state.lateral_target = off;
        true
    }

    /// The length of road the cars standing aside for car `i` from car `j` on fill (m), for
    /// going round them as a whole.
    pub fn aside_queue(&self, j: usize, i: usize) -> f32 {
        let mut k = j;
        let mut len = self.cars[j].state.front + self.cars[j].state.rear;
        for _ in 0..8 {
            let Some((id, gap)) = self.cars[k].lead_info else {
                break;
            };
            let Some(&n) = self.index_of.get(&id) else {
                break;
            };
            if gap > 15.0
                || self.cars[n]
                    .make_way
                    .is_none_or(|w| w.from != self.cars[i].id)
            {
                break;
            }
            len += gap.max(0.0) + self.cars[n].state.front + self.cars[n].state.rear;
            k = n;
        }
        len
    }
}
