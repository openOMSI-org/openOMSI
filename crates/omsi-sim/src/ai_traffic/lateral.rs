//! Where a car goes across the road: the one owner of its lateral target. Every sideways
//! wish of a car - the lane its route moves over to, a turn lane, its lane closing ahead to
//! its kind of vehicle, overtaking and keeping to its side, going round something standing
//! (on the lane beside or on the other half of the road), swerving round a car at the kerb,
//! parking and pulling out of a space - is weighed here once a step, in one order, and only
//! what is decided here moves the car sideways. Before, half a dozen planners each set the
//! target at their own point of the step, and whichever ran last won: a car could start an
//! overtake in the step its route wanted the other lane, or two cars moved into the same gap
//! of a lane in the same step, each unaware of the other.
//!
//! The order, most urgent first: what is under way goes on (a lane change runs its course,
//! a pass round something standing is finished or given up, a parking or a pull-out out of
//! a space goes on); a timetable bus's place at its stop is its service's
//! (`bus_service`); making way for an emergency vehicle (`emergency::emergency_way`); the
//! change the car's way requires; an emergency vehicle's corridor (`emergency_corridor`); a
//! bus queueing for its own stop stays where it is; a change of the driver's own choosing;
//! going round something that stands in the way; and else the middle of the lane, or a
//! swerve round a car at the kerb.

use super::*;
use super::planning::lane_open_to;

/// Seconds a driver has to want a lane change of their own choosing before acting on it:
/// conditions that flicker (a leader just under the speed that is worth overtaking) do not
/// make the car twitch.
pub const WISH_HOLD: f32 = 0.6;
/// A change of the driver's own choosing back to the side of their last change is not
/// started within this many seconds of it (no swinging from lane to lane).
pub const NO_SWING_BACK: f32 = 8.0;
/// Of the cars moving over into one lane, at most one starts within this distance of
/// another in a step (m): the planners read the lanes as they were at the start of the step,
/// so two that started together each took the gap for their own.
pub const CHANGE_SPACING: f32 = 40.0;
/// How far ahead a car looks for its way closing to it (m).
pub const CLOSING_LOOK: f32 = 150.0;
/// Within this distance of a junction a car moves into the lane for its turn (m).
pub const TURN_LANE_LOOK: f32 = 150.0;
/// A car waiting for a gap to make the change its way requires stands this far short of
/// where it must have moved over (m): a move from a standstill is 12 m long
/// (`AiState::start_change`), and one that is not over where the car's own way ends
/// finishes with a jump sideways.
pub const CHANGE_ROOM: f32 = 13.0;
/// Seconds a car whose route moves it over a lane waits for its gap before it moves over
/// even where the two lanes no longer run side by side.
pub const LATE_CHANGE_AFTER: f32 = 12.0;

/// Is the box (with its heights) in a scenery object's way? See `TrafficSim::scenery_probe`.
pub type SceneryProbe = Arc<dyn Fn(&Obb) -> bool + Send + Sync>;

/// What a car remembers of its sideways moves from step to step.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sideways {
    /// The lane the driver would like to move over to of their own choosing, and since when
    /// (`TrafficSim::time`).
    pub wish: Option<(usize, f32)>,
    /// The side (1 left, 2 right) of the last lane change it began, and when.
    pub last: Option<(i32, f32)>,
    /// No new look at a change of its own choosing before this time: one whose way was not
    /// clear is not tried again every step.
    pub retry: f32,
    /// Seconds the indicator has been on for the change its way requires.
    pub signalled: f32,
    /// The lane it is waiting for a gap in for that change, and when it last waited
    /// (`TrafficSim::time`): the cars of that lane that stand behind it for good are told
    /// to drive on (`TrafficSim::break_lead_pairs`).
    pub waiting: Option<(usize, f32)>,
    /// What decided its lateral target this step (for the debug output).
    pub why: &'static str,
}

/// A lane change a car means to make or has begun this step, for sorting out who moves
/// into a lane first (`TrafficSim::change_claims`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChangeClaim {
    pub lane: usize,
    /// Where along that lane.
    pub s: f32,
    /// The car's way requires it (else it is of the driver's choosing or optional).
    pub required: bool,
    pub id: u64,
    /// Begun this step (else only wanted).
    pub started: bool,
}

/// Why a car's way requires a lane change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// A timetable route moves over to the lane beside.
    Route,
    /// The lane closes ahead to this kind of vehicle (it goes on as a bus lane, `[rule]
    /// no_cars`, or into lanes of no traffic).
    Closing,
    /// The turn the car takes at the junction ahead has a lane of its own.
    TurnLane,
}

/// The change a car's way requires: the lane (and side, 1 left 2 right) to move to - None
/// when there is no open way on at all - and, for a lane that closes, how far it is from
/// the car's origin to where it closes.
#[derive(Debug, Clone, Copy)]
pub struct Required {
    pub need: Need,
    pub to: Option<(usize, i32)>,
    pub end: f32,
}

/// How a lane change is begun (`AiState::start_change` and its variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Choice,
    Required(Need),
    /// Pulling out from behind something standing: a short, steep move.
    Bypass,
}

/// What the owner of the lateral target is told of car's situation this step.
pub struct SideScene {
    pub lead: LeadOf,
    /// The length of what stands ahead (a queue at a stop counts whole).
    pub obstacle_len: f32,
    pub standing: bool,
    pub parked_ahead: bool,
    pub at_stop: bool,
    pub player: Option<PlayerBox>,
    pub parked_box: Option<Obb>,
    /// The offset in its lane that keeps the car clear of a car at the kerb or of a bus
    /// standing half in its bay.
    pub kerb_swerve: Option<f32>,
}

/// What the lateral decision means for the car's speed: where it waits for a gap to make
/// the change its way requires, where it stops beside its parking space (distances from
/// its origin to where its front stops).
#[derive(Debug, Clone, Copy, Default)]
pub struct SideOut {
    pub merge_wait: Option<f32>,
    pub park: Option<f32>,
    /// Where it waits, moved aside for an emergency vehicle.
    pub make_way: Option<f32>,
}

impl TrafficSim {
    /// The changes the cars' ways require, before anybody plans this step: a car whose way
    /// requires a change goes before a car that only wants one, wherever the two stand in
    /// the order of planning.
    pub fn collect_change_claims(&mut self) {
        let mut claims = Vec::new();
        for i in 0..self.cars.len() {
            if let Some(Required { to: Some((to, _)), .. }) = self.required_change(i) {
                let st = &self.cars[i].state;
                let s = self.net.beside_s(st.lane, to, st.s);
                claims.push(ChangeClaim { lane: to, s, required: true, id: self.cars[i].id, started: false });
            }
        }
        claims.sort_by_key(|c| c.id);
        self.change_claims = claims;
    }

    /// May car `id` begin a change into `lane` at `s` as far as the other cars' changes of
    /// this step go? Not within `CHANGE_SPACING` of one that has begun into it this step,
    /// and one of its own choosing not near a change another car's way requires there.
    /// (Between two required changes the order of planning - the cars' ids - decides.)
    pub fn claim_free(&self, lane: usize, s: f32, id: u64, required: bool) -> bool {
        !self.change_claims.iter().any(|c| {
            c.id != id
                && c.lane == lane
                && (c.s - s).abs() < CHANGE_SPACING
                && (c.started || (c.required && !required))
        })
    }

    /// Car `i`'s sideways decision for this step (see the module's notes for the order),
    /// and what it means for its speed.
    pub fn steer_sideways(&mut self, i: usize, ts: &TickScene, sc: &SideScene) -> SideOut {
        let mut out = SideOut::default();
        // what is under way goes on
        if self.cars[i].park.is_some() {
            out.park = self.park_step(i);
        }
        self.pull_out_step(i, ts);
        self.guard_pass(i, &ts.by_lane);
        self.finish_pass(i);
        let car = &self.cars[i];
        let mut why = if car.state.change.is_some() {
            "change"
        } else if car.passing.is_some() {
            "pass"
        } else if car.park.is_some() {
            "park"
        } else if car.pull_out > 0.0 {
            "pull_out"
        } else {
            ""
        };
        let settled = why.is_empty();
        // (the way of an emergency vehicle comes before anything the car wants itself)
        if settled {
            if let Some(stop) = self.emergency_way(i, ts, sc) {
                why = "emergency";
                out.make_way = stop;
            }
        } else {
            self.cars[i].make_way = None;
        }
        // the change the car's way requires
        let required = if why.is_empty() { self.required_change(i) } else { None };
        match required {
            Some(r) => {
                out.merge_wait = self.make_required_change(i, r, ts);
                why = match r.need {
                    Need::Route => "route",
                    Need::Closing => "closing",
                    Need::TurnLane => "turn_lane",
                };
            }
            None => self.cars[i].sideways.signalled = 0.0,
        }
        if why.is_empty() {
            if self.emergency_corridor(i, ts) {
                why = "corridor";
            } else if self.queues_for_own_stop(i, sc.lead, sc.obstacle_len) {
                why = "own_stop";
            } else if self.discretionary_change(i, ts) {
                why = "choice";
            } else {
                // going round what stands in the way: the lane beside, else the other
                // half of the road
                self.plan_bypass(i, sc.lead, sc.standing, ts);
                if self.cars[i].state.change.is_some() {
                    why = "bypass";
                } else {
                    let way_now = self.way_lanes(&self.cars[i].state, 120.0);
                    self.plan_pass(i, sc, &way_now, ts);
                    if self.cars[i].passing.is_some() {
                        why = "pass";
                    }
                }
            }
        }
        // (aside for an emergency vehicle, or in its corridor, the place across the lane is
        // that move's)
        if why != "emergency" && why != "corridor" {
            self.set_in_lane(i, sc.kerb_swerve, ts.debug);
        }
        self.cars[i].sideways.why = if why.is_empty() { "lane" } else { why };
        out
    }

    /// A timetable bus queueing for its own stop (what stands ahead stands at or before
    /// it): it goes round nothing, it waits its turn.
    pub fn queues_for_own_stop(&self, i: usize, lead: LeadOf, obstacle_len: f32) -> bool {
        let car = &self.cars[i];
        let st = &car.state;
        let Some((l, who)) = lead else { return false };
        let real = l.gap + if who == Some(usize::MAX) { PLAYER_BOX_MARGIN } else { 0.0 };
        car.next_stop()
            .is_some_and(|(ri, ss)| ri >= st.route_index && st.route_distance(&self.net, ri, ss) < real + obstacle_len + st.front + 15.0)
    }

    /// The change car `i`'s way requires, if any (see `Required`).
    pub fn required_change(&self, i: usize) -> Option<Required> {
        let car = &self.cars[i];
        let st = &car.state;
        if st.change.is_some() || car.passing.is_some() || car.park.is_some() {
            return None;
        }
        if !st.route.is_empty() {
            if car.next_stop().is_some_and(|(ri, s)| ri == st.route_index && self.net.lanes[st.lane].length() - s >= 8.0) {
                return None;
            }
            let (to, dir) = st.route_change_due(&self.net)?;
            return Some(Required { need: Need::Route, to: Some((to, dir)), end: f32::MAX });
        }
        if self.net.lanes[st.lane].kind != LaneKind::Street {
            return None;
        }
        if let Some(end) = self.closing_ahead(i) {
            return Some(Required { need: Need::Closing, to: self.open_neighbour(i, end), end });
        }
        self.turn_lane_wanted(i).map(|to| Required { need: Need::TurnLane, to: Some(to), end: f32::MAX })
    }

    /// Where car `i`'s way closes to it within `CLOSING_LOOK`: the distance from its origin
    /// to the end of the last lane it may drive, where every way on is closed to its kind
    /// of vehicle (a lane that goes on as a bus lane). The end of the network is no such
    /// place: the cars leave the map there.
    pub fn closing_ahead(&self, i: usize) -> Option<f32> {
        let st = &self.cars[i].state;
        let mut d = self.net.lanes[st.lane].length() - st.s;
        let mut last = st.lane;
        for l in st.upcoming() {
            if d > CLOSING_LOOK {
                return None;
            }
            d += self.net.lanes[l].length();
            last = l;
        }
        if d > CLOSING_LOOK {
            return None;
        }
        let lane = &self.net.lanes[last];
        (!lane.next.is_empty() && lane.next.iter().all(|&n| !lane_open_to(&self.net.lanes[n], st))).then_some(d)
    }

    /// The lane beside car `i` that stays open to it well past `end` (where its own way
    /// closes, from its origin): the lane on its own side of the road first, the other one
    /// if that goes clearly further. None: no open way on.
    fn open_neighbour(&self, i: usize, end: f32) -> Option<(usize, i32)> {
        let st = &self.cars[i].state;
        let lane = &self.net.lanes[st.lane];
        let sides = if self.net.left_hand { [(lane.left, 1), (lane.right, 2)] } else { [(lane.right, 2), (lane.left, 1)] };
        let need = end + 30.0;
        let mut best: Option<(usize, i32, f32)> = None;
        for (side, dir) in sides {
            let Some(l) = side.filter(|&l| self.open_to(i, l)) else { continue };
            let s_l = self.net.beside_s(st.lane, l, st.s);
            let run = self.open_run(st, l, s_l, need + 50.0);
            if run >= need && best.is_none_or(|b| run > b.2 + 1.0) {
                best = Some((l, dir, run));
            }
        }
        best.map(|b| (b.0, b.1))
    }

    /// How far a vehicle of `st`'s kind can drive on from distance `s` of `lane` along the
    /// lanes open to it (the longest way, m), counted up to `limit`. The end of the network
    /// counts as open road (the cars leave the map there); a lane whose every way on is
    /// closed to the vehicle ends it.
    pub fn open_run(&self, st: &AiState, lane: usize, s: f32, limit: f32) -> f32 {
        fn walk(t: &TrafficSim, st: &AiState, lane: usize, run: f32, limit: f32, depth: u32) -> f32 {
            let l = &t.net.lanes[lane];
            if run >= limit || depth >= 8 || l.next.is_empty() {
                return limit.max(run);
            }
            let mut best = run;
            for &n in &l.next {
                if lane_open_to(&t.net.lanes[n], st) {
                    best = best.max(walk(t, st, n, run + t.net.lanes[n].length(), limit, depth + 1));
                    if best >= limit {
                        break;
                    }
                }
            }
            best
        }
        let Some(l) = self.net.lanes.get(lane) else { return 0.0 };
        walk(self, st, lane, l.length() - s, limit, 0).min(limit)
    }

    /// The lane beside car `i` for the turn it takes at the junction ahead (within
    /// `TURN_LANE_LOOK`), when it is open to it and leads to a way out with that turn.
    fn turn_lane_wanted(&self, i: usize) -> Option<(usize, i32)> {
        let st = &self.cars[i].state;
        if st.speed < 4.0 || st.change_cooldown > 0.0 {
            return None;
        }
        let lane = &self.net.lanes[st.lane];
        let left = lane.length() - st.s;
        // (the indicator, the move and a little margin must fit into what is left of it)
        if left >= TURN_LANE_LOOK || left < (st.speed * 5.5 + 10.0).max(40.0) {
            return None;
        }
        let turn = st.planned_next.map(|n| self.net.lanes[n].turn).unwrap_or(0);
        let side = match turn {
            1 => lane.left,
            2 => lane.right,
            _ => None,
        }?;
        (self.open_to(i, side) && self.net.lanes[side].next.iter().any(|&n| self.net.lanes[n].turn == turn)).then_some((side, turn))
    }

    /// The change car `i`'s way requires: begun as soon as the lane beside lets it in;
    /// until then the indicator is on and the car waits short of where it must have moved
    /// over (returned). It never cuts in and never jumps over: with no open way on at all
    /// it leaves the road where its way ends rather than drive into a lane closed to it.
    fn make_required_change(&mut self, i: usize, r: Required, ts: &TickScene) -> Option<f32> {
        let Some((to, dir)) = r.to else {
            let car = &mut self.cars[i];
            if !car.gone && !car.is_bus() {
                car.gone = true;
                if ts.debug {
                    log::info!("t={:.1}: car {} finds its lane closing to it ahead and no open way on: it leaves", self.time, car.id);
                }
            }
            return None;
        };
        let st = &self.cars[i].state;
        // (a route's change is made where the two lanes still run side by side; a bus that
        // waited for its gap until they parted - or came to the change too late to wait for
        // it - stood at the end of its lane for good, the street behind it with it: after
        // `LATE_CHANGE_AFTER` s of waiting it moves over wherever it is, finishing the move
        // with a jump sideways at the end of its lane, as the module notes say)
        let possible = r.need != Need::Route
            || self.net.route_change_locally_possible(st.lane, to, st.s)
            || self.cars[i].sideways.signalled > LATE_CHANGE_AFTER;
        if possible && self.try_change(i, to, dir, ChangeKind::Required(r.need), ts, None) {
            if ts.debug {
                log::info!("t={:.1}: car {} moves over to lane {to} ({:?})", self.time, self.cars[i].id, r.need);
            }
            self.cars[i].sideways.signalled = 0.0;
            return None;
        }
        if r.need == Need::TurnLane {
            // (the turn can be taken from the lane the car is on as well)
            return None;
        }
        // where it waits: short of where the lanes part or its way closes, with room for
        // the move (a route's is shortened to fit, `try_change`, down to 6 m)
        let st = &self.cars[i].state;
        let wait = match r.need {
            Need::Route => self.net.route_change_wait_distance(st.lane, to, st.s) - 6.0,
            _ => r.end - CHANGE_ROOM,
        };
        let car = &mut self.cars[i];
        car.state.signal = dir;
        car.state.signal_time = 1.0;
        car.sideways.signalled += ts.dt;
        car.sideways.waiting = Some((to, self.time));
        if ts.debug && [30.0, 90.0, 240.0].iter().any(|t| (car.sideways.signalled - t).abs() < ts.dt * 0.5) {
            let (id, s) = (car.id, car.state.s);
            let s_to = self.net.beside_s(self.cars[i].state.lane, to, s);
            let near: Vec<String> = ts
                .by_lane
                .get(&to)
                .into_iter()
                .flatten()
                .filter(|e| (e.1 - s_to).abs() < 45.0)
                .map(|e| format!("car {} {:+.1} m v {:.1}{}", self.cars[e.0].id, e.1 - s_to, self.cars[e.0].state.speed, if e.3 { " foreign" } else { "" }))
                .collect();
            log::info!(
                "t={:.1}: car {id} has waited {:.0} s on lane {} (s {s:.1}) to move over to lane {to} ({:?}): may merge {}, claim free {}, cars there: {:?}",
                self.time,
                self.cars[i].sideways.signalled,
                self.cars[i].state.lane,
                r.need,
                self.can_merge(i, to, s_to, &ts.by_lane),
                self.claim_free(to, s_to, id, true),
                near
            );
        }
        Some(wait.max(0.0))
    }

    /// Begin car `i`'s change into lane `to` (side `dir`) if nothing speaks against it:
    /// the other changes of this step (`claim_free`), the gaps on that lane (`can_merge`)
    /// and the whole way over against what stands there, the people on foot and the scenery
    /// (`ghost_blocked`; `skip`: what it pulls out round). True when it has begun.
    pub fn try_change(&mut self, i: usize, to: usize, dir: i32, kind: ChangeKind, ts: &TickScene, skip: Option<usize>) -> bool {
        let car = &self.cars[i];
        let st = &car.state;
        let id = car.id;
        let s_to = self.net.beside_s(st.lane, to, st.s);
        let required = matches!(kind, ChangeKind::Required(_));
        if !self.claim_free(to, s_to, id, required) || !self.can_merge(i, to, s_to, &ts.by_lane) {
            return false;
        }
        let mut hyp = st.clone();
        match kind {
            ChangeKind::Required(Need::Route) => {
                hyp.start_route_change(&self.net, to, dir);
                // (the indicator goes on before the car moves, unless it has been on while
                // the driver waited for the gap; the move is over before the lanes part)
                let left = (self.net.lanes[hyp.lane].length() - hyp.s).max(0.0);
                let wait = (crate::traffic::SIGNAL_BEFORE_CHANGE - car.sideways.signalled).max(0.0);
                if let Some(c) = hyp.change.as_mut() {
                    c.wait = wait;
                    c.length = c.length.min((left - wait * hyp.speed) * 0.7).max(6.0);
                }
            }
            ChangeKind::Required(Need::Closing) => {
                // (over before the way closes: there the move could not carry on)
                hyp.start_change(&self.net, to, dir);
                let left = self.closing_ahead(i).unwrap_or(f32::MAX);
                if let Some(c) = hyp.change.as_mut() {
                    c.length = c.length.min((left - c.wait * hyp.speed) * 0.85).max(8.0);
                }
            }
            ChangeKind::Required(Need::TurnLane) => {
                hyp.turn_wish = dir;
                hyp.start_change(&self.net, to, dir);
            }
            ChangeKind::Bypass => hyp.start_bypass(&self.net, to, dir),
            _ => hyp.start_change(&self.net, to, dir),
        }
        let Some(c) = hyp.change else { return false };
        let len = c.wait * hyp.speed.max(2.0) + c.length + st.front + 2.0;
        // (a rail vehicle's way over is the track's)
        let blocked = if car.is_rail() {
            None
        } else {
            // (what stands ahead in its own lane it keeps behind anyway, the way it follows
            // it: the trajectory reaches it only if the car does not slow down for it)
            let mut skip: Vec<usize> = skip.into_iter().collect();
            let own = std::iter::once((st.lane, -st.s)).chain(st.upcoming().scan(self.net.lanes[st.lane].length() - st.s, |d, l| {
                let at = *d;
                *d += self.net.lanes[l].length();
                Some((l, at))
            }));
            for (l, dl) in own.take_while(|e| e.1 < len) {
                skip.extend(ts.by_lane.get(&l).into_iter().flatten().filter(|e| !e.3 && e.0 != i && dl + e.1 > 0.0).map(|e| e.0));
            }
            let net = &self.net;
            let way = |d: f32| hyp.way_point(net, d);
            self.ghost_blocked(i, &way, len, &ts.feet, &skip, &[])
        };
        if let Some(what) = blocked {
            if ts.debug {
                log::info!("t={:.1}: car {id} does not move over to lane {to}: {what} on the way", self.time);
            }
            self.cars[i].sideways.retry = self.time + 0.5;
            return false;
        }
        let car = &mut self.cars[i];
        car.state = hyp;
        car.sideways.last = Some((dir, self.time));
        car.sideways.wish = None;
        self.change_claims.push(ChangeClaim { lane: to, s: s_to, required, id, started: true });
        true
    }

    /// A lane change of the driver's own choosing: past a slow car ahead into a free lane on
    /// the overtaking side, else back to the lane on its own side of the road when that is
    /// free and stays open. The wish has to hold for `WISH_HOLD`, a change back is not made
    /// within `NO_SWING_BACK` of the last, and a lane that closes ahead to the car is not
    /// moved into. True when a change has begun.
    fn discretionary_change(&mut self, i: usize, ts: &TickScene) -> bool {
        let want = self.discretionary_wish(i, &ts.by_lane);
        let now = self.time;
        let car = &mut self.cars[i];
        let Some((to, dir, overtake)) = want else {
            car.sideways.wish = None;
            return false;
        };
        match car.sideways.wish {
            Some((w, _)) if w == to => {}
            _ => {
                car.sideways.wish = Some((to, now));
                return false;
            }
        }
        let since = car.sideways.wish.map(|w| w.1).unwrap_or(now);
        let swing_back = car.sideways.last.is_some_and(|(d, t)| d != dir && now - t < NO_SWING_BACK);
        if now - since < WISH_HOLD - 1e-3 || swing_back || now < car.sideways.retry {
            return false;
        }
        if !self.try_change(i, to, dir, ChangeKind::Choice, ts, None) {
            return false;
        }
        if overtake && self.last_overtaker.is_none() {
            self.last_overtaker = Some((self.cars[i].id, now));
        }
        if ts.debug {
            let c = &self.cars[i];
            log::info!("t={now:.1}: car {} {}: lane {} -> {to} at ({:.1}, {:.1})", c.id, if overtake { "overtakes" } else { "keeps to its side" }, c.state.lane, c.vehicle.position.x, c.vehicle.position.y);
        }
        true
    }

    /// The lane change car `i` would like to make of its own choosing now: (lane, side,
    /// whether it is an overtake). On a road of several lanes only, at speed, not shortly
    /// before the end of the lane, not out of a turn lane before its junction.
    fn discretionary_wish(&self, i: usize, by_lane: &ByLane) -> Option<(usize, i32, bool)> {
        let car = &self.cars[i];
        let st = &car.state;
        if st.change.is_some() || !st.route.is_empty() || st.change_cooldown > 0.0 || st.speed < 4.0 || car.passing.is_some() {
            return None;
        }
        let lane = &self.net.lanes[st.lane];
        // the indicator, the move and a little margin must fit in before the next junction
        // (a move not over at a joint of the road carries on across it, `AiState::drive`:
        // held to the end of the spline piece, the wish that has to hold for a moment first
        // came too late on the short pieces of Spandau's six-lane roads)
        let need = (st.speed * 5.5 + 10.0).max(40.0);
        if self.open_road_ahead(st, need) < need {
            return None;
        }
        let turn = st.planned_next.map(|n| self.net.lanes[n].turn).unwrap_or(0);
        if lane.length() - st.s < TURN_LANE_LOOK && turn != 0 {
            return None;
        }
        let limit = (lane.speed_limit_kmh * st.desire).min(st.max_speed_kmh) / 3.6;
        // (on a left-hand-traffic map overtaking is on the right and the keeping to the left)
        let (pass_side, keep_side, pass_dir, keep_dir) = if self.net.left_hand { (lane.right, lane.left, 2, 1) } else { (lane.left, lane.right, 1, 2) };
        if let Some(side) = pass_side.filter(|&l| self.open_to(i, l)) {
            let plan: Vec<usize> = st.upcoming().collect();
            if let Some((d, v, _)) = self.obstacle_from(i, st.lane, st.s, Some(&plan), 45.0, by_lane) {
                let s_side = self.net.beside_s(st.lane, side, st.s);
                let room = (5.5 * st.speed + 10.0).max(40.0);
                if v < limit * 0.7
                    && v < st.speed + 1.0
                    && d > 8.0
                    && self.lane_clear(i, side, s_side, 20.0, 50.0, by_lane)
                    && self.open_run(st, side, s_side, room) >= room
                {
                    return Some((side, pass_dir, true));
                }
            }
        }
        // keep to its side when that lane is free and goes on (not into a parking lane: the
        // outer lanes of Spandau's six-lane roads carry `[rule] trafficdensity 0` and the
        // parked cars; not into a lane that closes ahead to the car, #125)
        let side = keep_side.filter(|&l| self.open_to(i, l))?;
        let s_side = self.net.beside_s(st.lane, side, st.s);
        (self.lane_clear(i, side, s_side, 30.0, 70.0, by_lane)
            && self.net.lanes[side].length() - s_side > 40.0
            && self.open_run(st, side, s_side, CLOSING_LOOK) >= CLOSING_LOOK)
            .then_some((side, keep_dir, false))
    }

    /// How far car state `st` can drive on along its way before the next junction (or the
    /// end of what it has planned), counted up to `limit` (m from its origin).
    fn open_road_ahead(&self, st: &AiState, limit: f32) -> f32 {
        let mut d = self.net.lanes[st.lane].length() - st.s;
        if !self.net.crossings[st.lane].is_empty() {
            return 0.0;
        }
        for l in st.upcoming() {
            if d >= limit || !self.net.crossings[l].is_empty() {
                break;
            }
            d += self.net.lanes[l].length();
        }
        d.min(limit)
    }

    /// A pass on the other half of the road: given up before it got out (held there for
    /// long), cleared once the car stands behind what it gave up going round, back into the
    /// lane once past.
    fn finish_pass(&mut self, i: usize) {
        let car = &mut self.cars[i];
        let Some(mut p) = car.passing else { return };
        if !p.aborted && car.stopped > 8.0 && car.state.odometer < p.until && car.state.lateral.abs() < p.side * 0.5 {
            // held before it got out: back in behind the obstacle rather than wait half in
            // the oncoming lane
            p.until = car.state.odometer;
            p.aborted = true;
            p.hold = car.state.odometer;
            car.passing = Some(p);
        }
        if p.aborted {
            // standing behind what it gave up going round: free to look again (from where
            // it stands, half out or not)
            if car.state.speed < 0.1 && (car.state.odometer >= p.hold - 0.3 || car.stopped > 1.0) {
                car.passing = None;
            }
        } else if car.state.odometer >= p.until {
            car.state.lateral_target = 0.0;
            let odo = car.state.odometer;
            let from = car.state.lateral;
            if car.state.lateral_ramp.1.abs() > 1e-3 {
                // back in gently, as a driver does once past: over the S-curve planned for
                // the speed it has here and the room ahead
                car.state.lateral_ramp = (from, 0.0, odo, p.back);
            }
            if car.state.lateral.abs() < 0.05 {
                car.passing = None;
            }
        }
    }

    /// A car that was parked at the kerb stands indicating for its moment (`pull_out`),
    /// then pulls out once the traffic coming up behind it in its lane can let it in.
    fn pull_out_step(&mut self, i: usize, ts: &TickScene) {
        let car = &self.cars[i];
        if car.pull_out <= 0.0 {
            return;
        }
        let left = car.pull_out - ts.dt;
        let st = &car.state;
        let clear = left > 0.0 || self.traffic_lets_in(i, st.lane, st.s, &ts.by_lane);
        let car = &mut self.cars[i];
        car.pull_out = if clear { left.max(0.0) } else { ts.dt };
        if !clear {
            car.state.signal = 1;
            car.state.signal_time = car.state.signal_time.max(0.5);
        }
    }

    /// Parking: stop beside the space, move over into it along the last metres and stand
    /// there. Returns where it stops (distance from its origin to its front).
    fn park_step(&mut self, i: usize) -> Option<f32> {
        let way = self.way_lanes(&self.cars[i].state, 200.0);
        let car = &mut self.cars[i];
        let mut plan = car.park?;
        let st = &mut car.state;
        let mut stop = None;
        match way.iter().find(|w| w.0 == plan.lane) {
            None if st.lane != plan.lane => {
                // (its way went elsewhere after all)
                car.park = None;
            }
            found => {
                let dl = found.map(|w| w.1).unwrap_or(-st.s);
                let d = dl + plan.s; // origin to the space's middle
                if d < 80.0 {
                    stop = Some(d + st.front + 0.3);
                    st.signal = 2;
                    st.signal_time = st.signal_time.max(1.0);
                }
                // over into the space along the last metres, the move ending where the car
                // stops (the lateral place follows the distance driven: started too late,
                // the car stood half out of the space)
                if car.passing.is_none() {
                    let len = (plan.lat.abs() * 6.0).clamp(10.0, 22.0);
                    if !plan.ramped && d < len + 1.0 && d > 2.0 {
                        plan.ramped = true;
                        st.lateral_target = plan.lat;
                        st.lateral_ramp = (st.lateral, plan.lat, st.odometer, (d - 0.6).max(4.0));
                    } else if plan.ramped {
                        st.lateral_target = plan.lat;
                    } else {
                        st.lateral_target = 0.0;
                    }
                }
                if d.abs() < 1.5 && st.speed < 0.2 && (st.lateral - plan.lat).abs() < 0.3 {
                    plan.done = true;
                } else if d < -4.0 {
                    car.park = None;
                    st.lateral_target = 0.0;
                    stop = None;
                }
                if car.park.is_some() {
                    car.park = Some(plan);
                }
            }
        }
        stop
    }

    /// The place across its lane of a car that is not out passing, parking or at the
    /// service of a timetable: round a car parked at the kerb (or a bus half in its bay),
    /// else the middle. (A timetable bus's place is its service's, `bus_service`.)
    fn set_in_lane(&mut self, i: usize, kerb_swerve: Option<f32>, debug: bool) {
        let car = &mut self.cars[i];
        if car.is_bus() || car.passing.is_some() || car.park.is_some() {
            return;
        }
        let target = kerb_swerve.unwrap_or(0.0);
        if debug && (target - car.state.lateral_target).abs() > 0.3 {
            log::info!("t={:.2}: car {} swerves to {target:+.2} (was {:+.2}, now at {:+.2})", self.time, car.id, car.state.lateral_target, car.state.lateral);
        }
        car.state.lateral_target = target;
    }
}
