//! Right of way at junctions: what a car crosses on its way, who comes there, and
//! who goes first (the player's bus and the LAN players' vehicles included).

use super::*;

use super::deadlock::Hold;

/// Between two of equal rank (no sign, no turn across the other) the one that gets to the
/// meeting place this many seconds later than the other gives way to it (s); closer than
/// that neither does, and their claims decide.
pub const FIRST_COME_MARGIN: f32 = 0.5;

/// Seconds a driver has stood first at a side road's line before a slow queue on the road
/// it gives way to lets it in (see `lets_in`).
pub const COURTESY_WAIT: f32 = 20.0;
/// A driver on the road with the right of way lets a long waiter in only while slower than
/// this (m/s): in a queue, not in flowing traffic.
pub const COURTESY_SPEED: f32 = 6.0;

/// A driver at `v` m/s, `room` m before a meeting place, who would let a car that has
/// waited long at its line go first there: slow, and able to stop before it braking
/// comfortably (`decel`). Both sides judge it from the same speed and distance, so the
/// one waiting goes only where the other really holds back. This takes the place of the
/// claim a long waiter used to put in after 25 s, which the cars not yet decided had to
/// stop for whatever their speed: what lets a side road's car into a queue that never
/// leaves a gap (Spandau) is now the queue's drivers' courtesy (StVO §11(3) lets the
/// driver with the right of way give it up), and nobody is asked to brake hard for it.
pub fn lets_in(v: f32, decel: f32, room: f32) -> bool {
    v < COURTESY_SPEED && room > v * v / (2.0 * decel.max(0.5)) + 1.0
}

/// Of two decided cars, does the other one (its arrival in s, its room to the meeting
/// place in m, its speed, its id) go first before this one? The one that gets there first,
/// a tie to the lower number - but of two close in time, the one that can no longer stop
/// firmly goes before one that still can: on a tie broken by number, a car on the main road
/// at 50 km/h a dozen metres off stopped in an emergency for one that rolled out at walking
/// pace.
pub fn goes_first((t_j, theirs, v_o, id_o): (f32, f32, f32, u64), (t_mine, my_room, v, me): (f32, f32, f32, u64)) -> bool {
    let (i_must_go, they_must_go) = (!stops_before(v, my_room), !stops_before(v_o, theirs));
    if i_must_go != they_must_go && (t_j - t_mine).abs() <= 1.0 {
        return they_must_go;
    }
    t_j < t_mine - 0.3 || ((t_j - t_mine).abs() <= 0.3 && id_o < me)
}

/// A car that rolled into the junction without a claim of its own (on this meeting place's
/// lane either: the others give way to one), gives way here and can still stop before the
/// meeting place (at `v` m/s, `my_room` m off) does not pull out in front of one with the
/// right of way (`o`, `theirs` m off, there in `t_j` s while this one is clear in `t_clear`)
/// that is too close to stop for it without braking hard. Decided only by being inside, a
/// car turning out of a side road drove into the crossing ahead of a main road's car at
/// 50 km/h a few seconds off, which had to stop in an emergency and ran into it (Spandau).
/// It waits until that one is by, not for a gap in the stream - and not for one that gives
/// way itself (to this car, perhaps: the two waited for each other at Grundorf).
pub fn too_close_to_pull_out(v: f32, my_room: f32, o: &AiState, o_yielding: bool, theirs: f32, t_j: f32, t_clear: f32) -> bool {
    let they_cannot_stop = theirs < o.speed * o.speed / (2.0 * o.decel.max(1.0)) + o.speed + 2.0;
    my_room > v * v / MAX_BRAKE && o.speed > 3.0 && !o_yielding && they_cannot_stop && t_j < t_clear + 1.0
}

/// Distance (m) a driver at `v` m/s needs to stop with firm braking: what decides whether
/// a driver who gives way by a sign (a lower `[rule] priority`, an entry to a ring) can
/// still stop before the meeting place - an early claim of his does not take the right of
/// way from the traffic he gives way to while he can (both sides judge it alike).
pub fn firm_stop(v: f32) -> f32 {
    // (4 m/s²: firm, but short of what counts as an emergency stop)
    v * v / (2.0 * MAX_BRAKE * 0.5) + 0.3 * v + 1.0
}

/// Can a driver at `v` m/s, `room` m before a meeting place, still stop before it? One who
/// stands short of it always can. `firm_stop` keeps a metre in hand, and a car waiting at
/// its line (60 cm off) counted as one that could not stop any more: its early claim took
/// the right of way from the traffic it gives way to, so the cars going round a roundabout
/// stopped for each car standing at an entry (#2257), and of two cars close in time it was
/// the one standing at its line that went first.
pub fn stops_before(v: f32, room: f32) -> bool {
    room > firm_stop(v) || (v < STANDING && room > 0.3)
}

/// Below this speed (m/s) a driver counts as standing for `stops_before`.
const STANDING: f32 = 0.3;

/// The junction lanes each car has claimed this step (by car index) and the room on the
/// exits promised to the cars admitted: (car, metres, odometer reading at the exit) per
/// exit lane. A car admitted into a
/// junction is promised the room it needs on its exit, so that two are not let in for the
/// same space (15 m behind a standing queue take one 10 m bus, not two cars and the bus).
#[derive(Debug, Default)]
pub struct ClaimBook {
    pub lanes: HashMap<usize, Vec<usize>>,
    pub exits: HashMap<usize, Vec<(usize, f32, f32)>>,
}

impl ClaimBook {
    /// Car `i` claims `lanes`.
    pub fn claim(&mut self, i: usize, lanes: &[usize]) {
        for &l in lanes {
            let list = self.lanes.entry(l).or_default();
            if !list.contains(&i) {
                list.push(i);
            }
        }
    }

    /// Car `i` lets its claims of `lanes` and its room on any exit go.
    pub fn release(&mut self, i: usize, lanes: &[usize]) {
        for l in lanes {
            if let Some(list) = self.lanes.get_mut(l) {
                list.retain(|&c| c != i);
            }
        }
        for list in self.exits.values_mut() {
            list.retain(|e| e.0 != i);
        }
    }

    /// Car `i` is promised `need` metres on `exit`.
    pub fn promise(&mut self, i: usize, exit: usize, need: f32, at: f32) {
        for list in self.exits.values_mut() {
            list.retain(|e| e.0 != i);
        }
        self.exits.entry(exit).or_default().push((i, need, at));
    }

    /// What car `i` (`c`) holds as the step begins: its claims, those its tail still
    /// keeps, the room on its exit while it is not on it yet.
    pub fn add_car(&mut self, i: usize, c: &AiCar) {
        for &l in c.reserved.iter().chain(c.tail_claims.iter().map(|t| &t.0)) {
            let list = self.lanes.entry(l).or_default();
            if !list.contains(&i) {
                list.push(i);
            }
        }
        // (only a car still holding its claims is going there: one queued or waiting at a
        // line far back promised nothing)
        if let Some((exit, need, at)) = c.exit_claim.filter(|e| e.0 != c.state.lane && !c.reserved.is_empty()) {
            self.exits.entry(exit).or_default().push((i, need, at));
        }
    }

    /// The room on `exit` promised to cars other than `i` that get there first: closer to
    /// it than `before` metres (`odometer` of a car by index). A claim from further back
    /// takes nobody's room who is there before it.
    pub fn promised(&self, i: usize, exit: usize, before: f32, odometer: impl Fn(usize) -> f32) -> f32 {
        self.exits.get(&exit).map(|l| l.iter().filter(|e| e.0 != i && e.2 - odometer(e.0) < before).map(|e| e.1).sum()).unwrap_or(0.0)
    }
}

/// How far car `c`'s body reaches back from its origin (m): its own rear, or the rear of
/// its last section (an articulated bus, a car with a trailer) as the body stands now.
pub fn body_tail(c: &AiCar) -> f32 {
    let v = &c.vehicle;
    v.trailers
        .iter()
        .filter_map(|t| {
            let bb = t.ty.def.bounding_box?;
            Some((t.position - v.position).truncate().length() as f32 + bb[1] * 0.5 - bb[4])
        })
        .fold(c.state.rear, f32::max)
}

/// Car `i` (`c`) where its body still covers junction lanes it has claimed further back
/// than the lane it came from (the rear section of an articulated bus), for `by_lane`.
pub fn tail_bodies(net: &Network, i: usize, c: &AiCar, by_lane: &mut HashMap<usize, Vec<(usize, f32, f32, bool)>>) {
    for &(l, left) in &c.tail_claims {
        if Some(l) != c.state.prev_lane && l < net.lanes.len() {
            by_lane.entry(l).or_default().push((i, net.lanes[l].length() + c.state.odometer - left, c.state.lateral, false));
        }
    }
}

/// What `Traffic::weigh_crossings` found at a junction: somebody physically in the way
/// (`hard`), only the rules in the way (`ruled`), the cars this one waits for that wait
/// themselves (`soft`), why (for the debug output), where to stop, and whether anybody is
/// near the crossing lanes at all (`contested`).
pub struct Weighing {
    hard: bool,
    ruled: bool,
    soft: Vec<usize>,
    why: Vec<String>,
    stop_at: Option<f32>,
    /// The first vehicle (by id) found in the way or with the right of way.
    by: Option<u64>,
    /// A driver who has waited long at a side road's line goes, and this car lets him in.
    courtesy: bool,
    /// Whom it waits for and why (the waits-for graph).
    holds: Vec<(u64, Hold)>,
    /// Somebody on a footpath holds it.
    people: bool,
    /// Every vehicle that holds it comes the other way (`oncoming_meet`: from its origin to
    /// the nearest place where it meets them).
    only_oncoming: bool,
    oncoming_meet: Option<f32>,
    /// From the car's origin to the nearest meeting place where a vehicle holds it.
    nearest: Option<f32>,
    /// It gives up an early claim to traffic it gives way to by a sign.
    sign_yield: bool,
    /// It goes because a slow queue lets it in (`lets_in`).
    let_in: bool,
}

/// Where a vehicle meets a crossing lane on its way: its lane in the sequence, the distance
/// from its origin to that lane's start.
#[derive(Debug, Clone)]
pub struct Junction {
    /// (lane, distance from the car's origin to its start) of the junction's lanes on the
    /// car's way; the first is where it has to wait.
    pub lanes: Vec<(usize, f32)>,
    /// The lane after the junction and the distance to its start.
    pub exit: Option<(usize, f32)>,
    /// The car is already on one of the junction's lanes.
    pub inside: bool,
}

/// A vehicle the AI does not drive (the player's bus, a LAN player's) as the right of way
/// sees it: Omsi.exe keeps the player's vehicle on the paths like any AI vehicle, so the
/// cars give way to it by the same rules; here it is put onto the lanes where it is.
#[derive(Debug, Clone)]
pub struct WayUser {
    /// The lane it is on and those it may take next: (lane, distance from its centre to
    /// the lane's start - negative for the lane it is on).
    pub lanes: Vec<(usize, f32)>,
    /// Speed along its way (m/s), half its length (m), how long it has stood (s).
    pub speed: f32,
    pub half_len: f32,
    pub still: f32,
    /// Its script claims priority (`TrafficPriority`).
    pub prio: bool,
}

/// Seconds until a vehicle `dist` metres from a point gets its front there, from speed `v`
/// with acceleration `a`.
pub fn time_to(dist: f32, v: f32, a: f32) -> f32 {
    if dist <= 0.0 {
        return 0.0;
    }
    let a = a.max(0.3);
    // v t + a t² / 2 = dist
    (-v + (v * v + 2.0 * a * dist).sqrt()) / a
}

pub fn crossing_arrival(st: &AiState, distance: f32, claimed: bool, waits_short: bool, stalled: bool) -> f32 {
    if distance <= 0.3 {
        return 0.0;
    }
    if claimed {
        return time_to(distance, st.speed, st.accel)
            + if st.speed < 0.1 { st.reaction } else { 0.0 };
    }
    if waits_short {
        return f32::MAX;
    }
    // A queue cannot accelerate freely. Keep its actual movement in the prediction:
    // ignoring a crawling car altogether would let another drive into its path.
    if stalled {
        return if st.speed > 0.0 { distance / st.speed } else { f32::MAX };
    }
    if st.speed > 0.5 {
        distance / st.speed
    } else {
        time_to(distance, 0.0, st.accel) + st.reaction
    }
}

/// Metres of room beyond what it needs that a car waiting for a full exit wants before it
/// goes (see `TrafficSim::junction_stop`).
pub const EXIT_HYSTERESIS: f32 = 3.0;

/// Comfortable braking of a vehicle on a junction's exit, for where it will have stopped
/// (m/s², see `queued_exit_vehicle`).
pub const EXIT_BRAKE: f32 = 2.0;

/// The nearest vehicle in the part of the chosen exit that must be clear for this car:
/// (the room left before its rear, its speed, its lane).
///
/// `way` distances are measured from the car's present lane origin; `rear` is measured
/// from the occupied lane's start. Combining both lets a queue be found across short,
/// consecutive path objects instead of only on the first lane after a junction.
/// A vehicle that is still moving counts where it would have stopped braking gently now
/// (its speed² / 2 `EXIT_BRAKE` further on): a queue crawling along the exit leaves no
/// more room than one standing there. Counted only once it stood (below 1.5 m/s), a queue
/// creeping on at walking pace let car after car into a junction they could not leave;
/// the queue stopped and they stood in the crossing traffic's way, which in its turn
/// queued back into the junctions before - on Spandau's big junctions the rings of cars
/// waiting on each other never cleared.
pub fn queued_exit_vehicle(
    way: &[(usize, f32)],
    exit: (usize, f32),
    need: f32,
    occupied: impl IntoIterator<Item = (usize, f32, f32)>,
) -> Option<(f32, f32, usize)> {
    let start = way
        .iter()
        .position(|&(lane, distance)| lane == exit.0 && (distance - exit.1).abs() < 0.01)?;
    occupied
        .into_iter()
        .filter_map(|(lane, rear, speed)| {
            let offset = way[start..]
                .iter()
                .take_while(|&&(_, distance)| distance - exit.1 < need)
                .find(|&&(candidate, _)| candidate == lane)
                .map(|&(_, distance)| distance - exit.1)?;
            let space = offset + rear;
            let will = space + speed.max(0.0).powi(2) / (2.0 * EXIT_BRAKE);
            (will < need).then_some((will, space, speed, lane))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, space, speed, lane)| (space, speed, lane))
}

/// A vehicle the AI does not drive, put onto the lanes for the right of way: the street
/// lane it drives along (within 3.5 m, its heading within 45 degrees) and the lanes it
/// may take on from there, as far as it gets in about six seconds. Where the way forks,
/// an indicator picks the turns that way (when the fork has one); without one every
/// branch counts - the cars cannot know where the bus is going, and the autopilot's
/// left turn without an indicator ran into a car that had taken the bus for going
/// straight on. None off the lanes or reversing.
pub fn way_user_on(net: &Network, b: &PlayerBox, blinker: u8, still: f32, prio: bool) -> Option<WayUser> {
    let (pos, heading, half_len, _, speed) = *b;
    if speed < -0.3 {
        return None;
    }
    let (lane, s, _) = net.lane_along(pos, heading, LaneKind::Street, 3.5, 45.0)?;
    let reach = (speed.max(0.0) * 6.0 + 40.0).min(LOOK_AHEAD_MAX);
    let mut lanes = vec![(lane, -s)];
    let mut open = vec![(lane, net.lanes[lane].length() - s)];
    while let Some((l, to_start)) = open.pop() {
        if to_start > reach || lanes.len() > 48 {
            continue;
        }
        let next: Vec<usize> = net.lanes[l]
            .next
            .iter()
            .copied()
            .filter(|&n| net.lanes[n].kind == LaneKind::Street && !lanes.iter().any(|o| o.0 == n))
            .collect();
        let chosen: Vec<usize> = match blinker {
            1 | 2 if next.len() > 1 && next.iter().any(|&n| net.lanes[n].turn == blinker as i32) => {
                next.into_iter().filter(|&n| net.lanes[n].turn == blinker as i32).collect()
            }
            _ => next,
        };
        for n in chosen {
            lanes.push((n, to_start));
            open.push((n, to_start + net.lanes[n].length()));
        }
    }
    Some(WayUser { lanes, speed: speed.max(0.0), half_len, still, prio })
}

/// The player's bus (or a LAN player's) coming to the joint where the car's (`me`) lane runs
/// into the one the bus is on - a side road's right turn into the main road, two lanes
/// becoming one - as `obstacle_ahead` sorts out two AI cars there: whoever gets to the
/// joint first goes first (a near tie to the bus), and the car keeps behind the bus as
/// if it were ahead in its own lane. Before, the car only saw the bus once its box was in
/// the car's way, and pulled out in front of it.
pub fn merging_lead(net: &Network, me: &AiState, users: &[WayUser]) -> Option<Lead> {
    if users.is_empty() || me.change.is_some() {
        return None;
    }
    let mut best: Option<Lead> = None;
    let mut before = net.lanes[me.lane].length() - me.s;
    let mut from = me.lane;
    for next in me.upcoming().take(2) {
        if before > LOOK_AHEAD {
            break;
        }
        if before - me.front < 0.5 {
            // at the joint already
            before += net.lanes[next].length();
            from = next;
            continue;
        }
        for &f in net.prev.get(next).map(|v| v.as_slice()).unwrap_or(&[]) {
            if f == from || net.crossings[from].iter().any(|c| c.other == f && c.merge) {
                continue;
            }
            for u in users {
                let (Some(&(_, df)), true) = (u.lanes.iter().find(|x| x.0 == f), u.lanes.iter().any(|x| x.0 == next)) else { continue };
                // its centre to the joint
                let to_joint = df + net.lanes[f].length();
                let theirs = to_joint - u.half_len;
                if to_joint < -u.half_len || (u.speed < 0.5 && u.still > 2.0) {
                    continue; // past it (then it is ahead in the lane), or standing
                }
                let t_me = (before - me.front).max(0.0) / me.speed.max(1.0);
                let t_them = if u.speed > 0.5 { theirs.max(0.0) / u.speed } else { time_to(theirs, 0.0, 1.0) + 1.0 };
                if t_them > t_me + 0.4 {
                    continue;
                }
                // behind it at the joint; while it is not past, wait at the joint itself
                let d = before - to_joint - u.half_len;
                let l = if d >= 0.0 {
                    Lead { gap: d - me.front, speed: u.speed, acc: 0.0 }
                } else {
                    Lead { gap: (before - 1.0 - me.front).max(0.0), speed: 0.0, acc: 0.0 }
                };
                if best.map(|b| l.gap < b.gap).unwrap_or(true) {
                    best = Some(l);
                }
            }
        }
        before += net.lanes[next].length();
        from = next;
    }
    best
}

/// What a vehicle the AI does not drive means for a car at a meeting place of its junction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    /// Nothing: through already, far off, standing, or the car goes first.
    Free,
    /// It is in the meeting place, or in the junction and there first: wait (a car that
    /// cannot stop any more still goes).
    Hard,
    /// It has the right of way and comes within the gap the driver would take.
    Ruled,
}

/// `Traffic::junction_stop` for one vehicle the AI does not drive (`u`, its lane's start
/// `dm` m from its centre) at the meeting place `c` of the car's junction lane, `point` m
/// from the car's origin: the rules the cars keep among themselves. `inside`: the car is
/// in the junction already; `committed`: it has claimed it; `me_prio`: it has priority by
/// its script; `must_yield`: its lane gives way to the vehicle's; `patience` shrinks the
/// gap it wants after a long wait. Also when the vehicle arrives (s) and when the car
/// would be through (s).
#[allow(clippy::too_many_arguments)]
pub fn way_user_verdict(st: &AiState, u: &WayUser, dm: f32, c: &crate::traffic::Crossing, point: f32, inside: bool, committed: bool, me_prio: bool, (must_yield, equals): (bool, bool), patience: f32) -> (Verdict, f32, f32) {
    let (v, a_me) = (st.speed, st.accel);
    // (its centre to the meeting point, and its front to the meeting place)
    let dj = dm + c.other_at;
    let t_clear = time_to(point + c.after + st.rear + 0.3, v, a_me) + if v < 0.5 { st.reaction } else { 0.0 };
    if dj + c.other_after < -u.half_len - 0.3 {
        return (Verdict::Free, f32::MAX, t_clear); // through
    }
    // (a meeting place far into the junction is weighed once the car is in it)
    if !inside && point - c.before - st.front > 25.0 {
        return (Verdict::Free, f32::MAX, t_clear);
    }
    let theirs = dj - c.other_before - u.half_len;
    let t_mine = time_to(point - c.before - st.front, v, a_me) + if v < 0.1 { st.reaction } else { 0.0 };
    if dm <= 0.0 && theirs <= 0.3 {
        // on that lane and in the meeting place: unless this car is further in already
        // (only where it is: a bus at its stop line is not on any of the ways it may take
        // yet, and counted as in all of them it held the cross traffic it waited for)
        let mine_in = st.front - (point - c.before);
        let ahead = mine_in > 0.0 && mine_in > -theirs + 0.3;
        return (if ahead { Verdict::Free } else { Verdict::Hard }, 0.0, t_clear);
    }
    // standing (at a stop, at its own line, letting this car go): not coming
    if u.speed < 0.5 && u.still > 2.0 {
        return (Verdict::Free, f32::MAX, t_clear);
    }
    let t_j = if u.speed > 0.5 { theirs.max(0.0) / u.speed } else { time_to(theirs, u.speed, 1.0) + 1.0 };
    if dm <= 0.0 && u.speed > 0.5 {
        // in the junction already and moving: the first there goes first
        let first = !(committed || inside) || t_j < t_mine - 0.3;
        let verdict = if first && t_j < t_clear + 1.0 { Verdict::Hard } else { Verdict::Free };
        return (verdict, t_j, t_clear);
    }
    let later = equals && t_j + FIRST_COME_MARGIN < t_mine;
    if inside || committed || (me_prio && !u.prio) || (!must_yield && !later && !(u.prio && !me_prio)) {
        return (Verdict::Free, t_j, t_clear);
    }
    // the gap a driver takes in the main road's traffic (as for the cars)
    let verdict = if t_j < (st.accept_gap + 2.0).max(t_clear + 1.0) * patience { Verdict::Ruled } else { Verdict::Free };
    (verdict, t_j, t_clear)
}

impl TrafficSim {
    /// The lanes of a car's way with their distance from its origin: the current lane (at
    /// minus `s`) and the plan, up to `within` metres.
    pub fn way_lanes(&self, st: &AiState, within: f32) -> Vec<(usize, f32)> {
        // (the plan is walked as far as `within` reaches: it was copied whole first, for
        // each of the several times a car asks every tick)
        let mut out = Vec::with_capacity(8);
        let (first, from, rest): (usize, f32, &mut dyn Iterator<Item = usize>) = match st.change {
            // over on the new lane: its distances count from the same place
            Some(c) => (c.to, c.s_to, &mut st.change_plan.iter().copied()),
            None => (st.lane, st.s, &mut st.upcoming()),
        };
        out.push((first, -from));
        let mut d = self.net.lanes[first].length() - from;
        for l in rest {
            if d > within {
                break;
            }
            out.push((l, d));
            d += self.net.lanes[l].length();
        }
        out
    }

    /// The junction on car `i`'s way within `within` metres: its lanes that cross or meet
    /// others (or a footpath), and the lane after it.
    pub fn junction_ahead(&self, way: &[(usize, f32)]) -> Option<Junction> {
        let has = |l: usize| !self.net.crossings[l].is_empty() || !self.net.walks[l].is_empty();
        let object = |l: usize| {
            self.net.lanes[l]
                .key
                .filter(|_| self.net.lanes[l].source == 2)
                .map(|k| (k.tile, k.id))
        };
        let mut j: Option<Junction> = None;
        for (k, &(l, d)) in way.iter().enumerate() {
            match j.as_mut() {
                None => {
                    if has(l) {
                        j = Some(Junction {
                            lanes: vec![(l, d)],
                            exit: None,
                            inside: k == 0,
                        });
                    }
                }
                Some(jn) => {
                    if object(l).is_some() && object(l) == object(jn.lanes[0].0) {
                        jn.lanes.push((l, d));
                    } else {
                        jn.exit = Some((l, d));
                        break;
                    }
                }
            }
        }
        j
    }

    /// Right of way at the junction ahead of car `i`: where it has to wait (distance from
    /// its origin), or None when it may go - in which case it claims the junction's lanes
    /// and is promised the room it needs on the exit. It gives way to anyone already in the
    /// junction on a crossing path, to anyone who has claimed a crossing path and arrives
    /// before it could be through, to traffic with the right of way that is close enough in
    /// time (its `accept_gap`), to pedestrians on a crossing, and it does not drive into a
    /// junction it could not leave (no room on the exit, or the queue ahead standing where
    /// the car would stop behind it in a meeting place of its way). A driver who has decided
    /// to go keeps to it (the claim stands, until its tail has left the junction's lanes)
    /// unless someone is actually in the way: weighing the gap again every step made two
    /// cars take turns at stopping and going, a hard brake every other step. Only a driver
    /// who gives way by a sign and can still stop gives up an early claim. `way` is the
    /// car's own way: where its own path crosses itself nothing is to be given way to.
    #[allow(clippy::too_many_arguments)]
    pub fn junction_stop(
        &mut self,
        i: usize,
        jn: &Junction,
        way: &[(usize, f32)],
        lead: Option<Lead>,
        on_lane: &HashMap<usize, Vec<(usize, f32, f32, bool)>>,
        coming: &HashMap<usize, Vec<(usize, f32)>>,
        book: &mut ClaimBook,
        walkers: &HashMap<usize, Vec<f32>>,
    ) -> Option<f32> {
        self.cars[i].holds.clear();
        let car = &self.cars[i];
        let st = &car.state;
        let v = st.speed;
        let entry = jn.lanes[0].1;
        let decide = (v * v / (2.0 * st.decel) + 12.0).clamp(20.0, 70.0);
        if !jn.inside && entry - st.front > decide {
            // too far to decide; a claim made just inside that distance stands (slowing
            // down moves the line, and letting go and claiming again in turns made the
            // cross traffic stop and go with it)
            if entry - st.front > decide + 20.0 {
                let old = std::mem::take(&mut self.cars[i].reserved);
                book.release(i, &old);
                self.cars[i].courtesy = false;
            }
            // (no room on the exit is kept for a car that has not got to deciding)
            self.cars[i].exit_claim = None;
            book.release(i, &[]);
            self.cars[i].exit_wait = false;
            return None;
        }
        // queued behind someone who is not through the junction yet: no claim
        let queued = !jn.inside
            && lead
                .map(|l| l.speed < 1.0 && l.gap < entry - st.front + 3.0)
                .unwrap_or(false);
        // decided already (a claim from the steps before), or past the point where it could
        // still stop without an emergency brake
        let committed = car.reserved.contains(&jn.lanes[0].0);
        // a junction kept for an emergency vehicle: wait at the entry (unless too close to
        // stop there)
        if let Some(owner) = self.emergency_entry_hold(i, jn, committed) {
            let room = entry - st.front - 0.6;
            if v <= 1.0 || room >= v * v / (2.0 * MAX_BRAKE * 0.7) {
                let old = std::mem::take(&mut self.cars[i].reserved);
                book.release(i, &old);
                let c = &mut self.cars[i];
                c.exit_claim = None;
                c.courtesy = false;
                c.exit_wait = false;
                c.yield_to = Some(owner);
                c.holds = vec![(owner, Hold::Claim)];
                return Some(entry);
            }
        }
        // (a stop line is kept 0.6 m off)
        let line_room = entry - st.front - 0.6;
        let explain = omsi_cfg::flags::OMSI_DEBUG_JUNCTION.is_set() || omsi_cfg::flags::OMSI_DEBUG_STUCK.is_set();
        let stop_at = if jn.inside { None } else { Some(entry) };
        // A driver who has waited long accepts a shorter gap (the critical gap shrinks with
        // the wait, by up to a third after forty seconds): a bus that needed twelve seconds
        // of a busy main road stood at the mouth of its side road for minutes.
        let wait = self.cars[i].state.yield_time;
        let patience = 1.0 - (wait / 40.0).min(1.0) / 3.0;
        // (a driver let in keeps counting on it while he holds his claim)
        let long_waiter = (!jn.inside && !queued && wait > COURTESY_WAIT) || (self.cars[i].courtesy && committed);
        let w = self.weigh_crossings(i, jn, way, on_lane, coming, &book.lanes, walkers, committed, patience, long_waiter, explain, stop_at);
        let Weighing { hard, mut ruled, mut soft, mut why, stop_at, by, mut courtesy, mut holds, people, only_oncoming, oncoming_meet, let_in, nearest, sign_yield } = w;
        // A left turn across the oncoming traffic at a signal pulls into the junction up to
        // where it meets that traffic and waits there, as drivers do, rather than at the
        // line: waiting at the line, one car got across a cycle (on the yellow) and the turn
        // lane's queue grew for good. Only one at a time, only when nothing but oncoming
        // traffic holds it, the meeting place at least 3 m past the line; without a signal
        // the car waits at the line.
        let held = hard || ruled || !soft.is_empty();
        let pull_in = (held && !jn.inside && only_oncoming && !people && !queued)
            .then_some(oncoming_meet)
            .flatten()
            .filter(|&m| self.pulls_in(jn, m, lead, st.front));
        // Pulling in, or giving up an early claim by a sign too close to its line to stop
        // there comfortably, what it can still stop for is the meeting place where it is
        // held, not the line: it stops before that place then (both sides judged it could).
        let meet_room = nearest.map(|n| n - st.front - 0.3);
        let line_late = sign_yield && v > 1.0 && line_room < v * v / (2.0 * st.decel);
        let room = match pull_in {
            Some(m) => m - st.front - 0.3,
            None if line_late => meet_room.map_or(line_room, |m| m.max(line_room)),
            None => line_room,
        };
        // (a car creeping up to its line can always stop: at the line the room is nothing,
        // and the car standing there used to count as one that could not stop any more)
        let cannot_stop = !jn.inside && v > 1.0 && room < v * v / (2.0 * MAX_BRAKE * 0.7);
        let cannot_stop_gently = !jn.inside && v > 1.0 && room < v * v / (2.0 * st.decel * 1.5);
        // (what is only courtesy - room for the car on the exit, a driver who has waited
        // long - is given only braking no harder than the driver likes)
        let cannot_stop_comfortably = !jn.inside && v > 1.0 && room < v * v / (2.0 * st.decel);
        // (nor, a queue on the exit that only will stand, to a driver who has decided and is
        // rolling off his line: stopped again for it, the car crept off and stood by turns,
        // a jolt every few seconds)
        let started = committed && v > 0.3 && line_room < 3.0;
        // the circle of cars that only give way to each other this one has been chosen to
        // end (`deadlock`): the right of way does not hold it (bodies, people, the lights
        // and a full exit still do)
        // (and a driver who let somebody in that way takes his right of way back)
        if self.cars[i].yield_pass > self.time {
            ruled = false;
            soft.clear();
            courtesy = false;
            holds.retain(|h| !matches!(h.1, Hold::Rule | Hold::Courtesy));
        }
        let need = st.front + self.cars[i].tail + st.min_gap + if self.cars[i].exit_wait { EXIT_HYSTERESIS } else { 0.0 };
        let (exit_full, exit_car) = if jn.inside { (false, None) } else { self.exit_room(i, jn, way, lead, on_lane, book, need, committed, started || cannot_stop_comfortably, &mut why) };
        if let Some(id) = exit_car {
            holds.push((id, Hold::Exit));
        }
        let mut blocked = (hard && !cannot_stop)
            || ((ruled || exit_full || !soft.is_empty()) && !cannot_stop_gently)
            || (courtesy && !cannot_stop_comfortably && !started);
        if !hard && !ruled && !exit_full && !soft.is_empty() && wait > 2.5 + st.reaction {
            // everybody is waiting for somebody: the longest waiter goes
            let wins = soft.iter().all(|&j| {
                let o = &self.cars[j];
                (o.yielding || o.state.speed < 0.3)
                    && (wait > o.state.yield_time + 0.05
                        || ((wait - o.state.yield_time).abs() <= 0.05 && self.cars[i].id < o.id))
            });
            if wins {
                blocked = false;
                if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                    log::info!("t={:.1}: car {} ends a wait of {wait:.1} s at a junction ({} waiting on it)", self.time, self.cars[i].id, soft.len());
                }
            }
        }
        let mut stop_at = stop_at;
        if let (false, true, Some(n)) = (jn.inside, line_late, nearest) {
            if n - 0.3 > entry {
                // (a footpath before the meeting place still holds it there)
                stop_at = Some(match stop_at {
                    Some(x) if x > entry + 0.01 => x.min(n - 0.3),
                    _ => n - 0.3,
                });
            }
        }
        if let Some(meet) = pull_in.filter(|_| blocked && !exit_full) {
            stop_at = Some(meet - 0.3);
            if explain {
                why.push(format!("pulls in to {:.1} m to turn across the oncoming traffic", meet - entry));
            }
        }
        let lanes: Vec<usize> = jn.lanes.iter().map(|x| x.0).collect();
        // (and every ten seconds of a long wait)
        let long_wait =
            wait > 15.0 && (wait / 10.0).floor() != ((wait - self.last_dt) / 10.0).floor();
        if explain && (blocked != self.cars[i].yielding || (blocked && long_wait)) {
            log::info!("t={:.2}: car {} at {:.1} m/s {} the junction {:?} (entry {:.1} m, inside {}): hard {hard}, waiting for {:?}, claims {:?} {:?}", self.time, self.cars[i].id, v, if blocked { "waits at" } else { "goes into" }, lanes, entry - st.front, jn.inside, soft.iter().map(|&j| self.cars[j].id).collect::<Vec<_>>(), lanes.iter().map(|l| book.lanes.get(l).map(|r| r.iter().map(|&j| self.cars[j].id).collect::<Vec<_>>())).collect::<Vec<_>>(), why);
        }
        if explain {
            self.cars[i].junction_why = if blocked { format!("{why:?} soft {:?}", soft.iter().map(|&j| self.cars[j].id).collect::<Vec<_>>()) } else { String::new() };
        }
        let yield_to = if blocked {
            by.or(exit_car).or_else(|| soft.first().map(|&j| self.cars[j].id))
        } else {
            None
        };
        self.cars[i].yield_to = yield_to;
        self.cars[i].exit_wait = blocked && exit_full;
        if blocked {
            for &j in &soft {
                holds.push((self.cars[j].id, Hold::Rule));
            }
            self.cars[i].holds = holds;
        }
        // Waiting at the line, a driver keeps his claim only for a body that is going
        // through (it will be gone, and he goes then): for the rules, a full exit or
        // somebody on foot he lets it go, and those who have not decided yet go first.
        let keeps = committed && hard && !ruled && !exit_full && !people && soft.is_empty() && !courtesy;
        if blocked && !jn.inside {
            if !keeps {
                let old = std::mem::take(&mut self.cars[i].reserved);
                book.release(i, &old);
                self.cars[i].exit_claim = None;
                self.cars[i].courtesy = false;
            }
            return stop_at;
        }
        if blocked {
            return stop_at;
        }
        if queued && !jn.inside {
            let old = std::mem::take(&mut self.cars[i].reserved);
            book.release(i, &old);
            self.cars[i].exit_claim = None;
            self.cars[i].courtesy = false;
            return None;
        }
        // claim the way through, and the room on the exit
        book.claim(i, &lanes);
        let car = &mut self.cars[i];
        for l in lanes {
            if !car.reserved.contains(&l) {
                car.reserved.push(l);
            }
        }
        if let Some(exit) = jn.exit.filter(|_| !jn.inside || car.exit_claim.is_some()) {
            let at = car.state.odometer + exit.1;
            book.promise(i, exit.0, need, at);
            car.exit_claim = Some((exit.0, need, at));
        }
        car.courtesy = let_in || (car.courtesy && committed);
        None
    }

    /// Is there room for car `i` beyond junction `jn`? None in the way: (false, None).
    /// Otherwise (true, the vehicle it waits for). The exit must take the whole car (`need`)
    /// besides what has been promised to the cars let in before it, up to the first
    /// vehicle standing or crawling there; and with the traffic ahead on its way standing,
    /// the car must not stop behind it with its body in a meeting place of its way through
    /// ("don't block the box", StVO §11(1)). `late`: the car has started off
    /// its line or could not stop comfortably any more - a queue that only will stand does
    /// not stop it then.
    #[allow(clippy::too_many_arguments)]
    fn exit_room(&self, i: usize, jn: &Junction, way: &[(usize, f32)], lead: Option<Lead>, on_lane: &HashMap<usize, Vec<(usize, f32, f32, bool)>>, book: &ClaimBook, need: f32, committed: bool, late: bool, why: &mut Vec<String>) -> (bool, Option<u64>) {
        let st = &self.cars[i].state;
        let entry = jn.lanes[0].1;
        let explain = omsi_cfg::flags::OMSI_DEBUG_JUNCTION.is_set() || omsi_cfg::flags::OMSI_DEBUG_STUCK.is_set();
        if let Some(exit) = jn.exit {
            // (a car holding its claim keeps the room promised to it: only the cars let in
            // before a newcomer count against him)
            let promised = if committed { 0.0 } else { book.promised(i, exit.0, exit.1, |j| self.cars.get(j).map_or(0.0, |c| c.state.odometer)) };
            let need_all = need + promised;
            // A map often builds the road immediately beyond a crossing from several short
            // path objects. Looking only at the first exit lane then calls the exit empty
            // while a queue stands on the next 2 m piece, and a car enters the box with
            // nowhere to put its body. Follow this car's chosen way until there is enough
            // clear road for all of it.
            let all_cars = &self.cars;
            let occupied = on_lane.iter().flat_map(|(&lane, cars)| {
                cars.iter().filter_map(move |&(j, s, _, passing)| {
                    // (one standing aside for this emergency vehicle is no queue to it)
                    (!passing && j != i && !self.aside_for(j, i)).then_some((lane, s - all_cars[j].tail, all_cars[j].state.speed))
                })
            });
            if let Some((space, speed, lane)) = queued_exit_vehicle(way, exit, need_all, occupied) {
                // (a car within its distance to decide looks across the junction however
                // long it is: with the exit taken only within 40 m of the car, a big
                // junction's exit came into view when the car could no longer stop for it,
                // and it stood in the middle behind the queue)
                // (a queue that stands near the junction, at any braking a driver calls
                // gentle; one that is about to stand, only if the car can stop for it
                // comfortably)
                let standing = speed < 1.5 && space < need_all && exit.1 < 40.0;
                let will_stand = space + speed.powi(2) / (2.0 * EXIT_BRAKE) < need_all && exit.1 - entry < 60.0;
                if standing || (will_stand && !late) {
                    // (the last car of that queue on that lane)
                    let last = on_lane
                        .get(&lane)
                        .and_then(|v| v.iter().filter(|e| !e.3 && e.0 != i).min_by(|a, b| (a.1 - self.cars[a.0].tail).total_cmp(&(b.1 - self.cars[b.0].tail))))
                        .map(|e| self.cars[e.0].id);
                    if explain {
                        why.push(format!("exit {} full on lane {lane} ({space:.1} m, {promised:.1} m promised)", exit.0));
                    }
                    return (true, last);
                }
            }
        }
        // don't block the box: the queue ahead stands beyond the line and the car would
        // stop behind it in a meeting place of its way through
        if late {
            return (false, None);
        }
        let Some(l) = lead.filter(|l| l.speed < 1.5 && l.acc < 0.5) else { return (false, None) };
        let lead_rear = st.front + l.gap;
        if lead_rear < entry + 0.5 {
            return (false, None); // queued before the line: that is `queued`
        }
        // Where its body would stand behind that car (from its rear to its front), and the
        // meeting places of its way through that body would cover. It is kept out of those
        // only: the rule used to be that its rear had to be clear of the last meeting place
        // of its whole way, and on a big crossing object (a divided road, crosswalks inside
        // it, the turns joining the way out at the object's far end) that place is 50 m in.
        // Behind a car standing anywhere inside - pulled in to turn, at a light further in,
        // queued from the road out - every car then stopped at the object's edge and left
        // the junction empty (#2592), and the queue it formed held the junctions behind it
        // (#2373). Omsi.exe has no such rule at all: its cars drive in up to the first path
        // somebody else has reserved.
        let front_at_stop = lead_rear - st.min_gap;
        let rear_at_stop = front_at_stop - st.front - self.cars[i].tail;
        let covered = jn
            .lanes
            .iter()
            .flat_map(|&(lane, dl)| {
                self.net.crossings[lane]
                    .iter()
                    .filter(|c| !way.iter().any(|w| w.0 == c.other))
                    .map(move |c| (dl + c.at - c.before, dl + c.at + c.after))
            })
            .filter(|&(from, to)| from < front_at_stop && to > rear_at_stop)
            .map(|(_, to)| to)
            .fold(f32::MIN, f32::max);
        if covered > f32::MIN {
            if explain {
                why.push(format!("the queue ahead stands {:.1} m into the junction, behind it the car would stand in a meeting place (to {:.1} m)", lead_rear - entry, covered - entry));
            }
            return (true, self.cars[i].lead_info.map(|x| x.0));
        }
        (false, None)
    }

    /// Does a car waiting at the line for the oncoming traffic only pull into junction `jn`
    /// up to the place `meet` (from its origin) where it meets that traffic? Only a turn
    /// across the oncoming traffic at a signal, with the meeting place at least 3 m past the
    /// line, and nobody ahead of it in the junction already (one at a time).
    fn pulls_in(&self, jn: &Junction, meet: f32, lead: Option<Lead>, front: f32) -> bool {
        let entry = jn.lanes[0].1;
        let across = if self.net.left_hand { 2 } else { 1 };
        let signalled = jn.lanes.iter().any(|&(l, _)| self.net.lanes[l].traffic_light.is_some());
        let turning = jn.lanes.iter().any(|&(l, _)| self.net.lanes[l].turn == across);
        let alone = lead.is_none_or(|l| front + l.gap > meet + 2.0);
        signalled && turning && alone && meet - entry >= 3.0
    }

    /// Car `i` drives off the lanes it has claimed: the claims of those its tail is still on
    /// are kept until it has left them (an articulated bus's rear section holds the
    /// junction until it is out), the others go. The room promised on an exit no longer on
    /// its way goes too.
    pub fn keep_tail_claims(&mut self, i: usize, way: &[(usize, f32)]) {
        let car = &mut self.cars[i];
        let on_way = |l: usize| way.iter().any(|w| w.0 == l);
        let (odo, s, prev) = (car.state.odometer, car.state.s, car.state.prev_lane);
        for &l in &car.reserved {
            if !on_way(l) && prev == Some(l) && !car.tail_claims.iter().any(|t| t.0 == l) {
                car.tail_claims.push((l, odo - s));
            }
        }
        car.reserved.retain(|&l| on_way(l));
        let tail = car.tail;
        car.tail_claims.retain(|&(l, left)| !on_way(l) && odo - left < tail + 0.3);
        if car.exit_claim.is_some_and(|(e, _, _)| !way.iter().skip(1).any(|w| w.0 == e)) {
            car.exit_claim = None;
        }
        if car.reserved.is_empty() {
            car.courtesy = false;
        }
    }

    /// Somebody on a footpath crossing junction lane `l` (`dl` m from the car's origin)
    /// within 30 m of the car's front: the car stops 2.5 m before the crossing.
    fn people_crossing(&self, l: usize, dl: f32, front: f32, walkers: &HashMap<usize, Vec<f32>>, why: Option<&mut Vec<String>>, stop_at: &mut Option<f32>) -> bool {
        let mut found = false;
        let mut why = why;
        for &(w, at, w_at) in &self.net.walks[l] {
            let point = dl + at;
            if point < front - 1.0 {
                continue;
            }
            if walkers.get(&w).is_some_and(|ps| ps.iter().any(|&p| (p - w_at).abs() < 3.0)) && point - front < 30.0 {
                found = true;
                if let Some(why) = why.as_deref_mut() {
                    why.push(format!("someone on the crossing of lane {l} and footpath {w}"));
                }
                let before = point - 2.5;
                *stop_at = Some(stop_at.unwrap_or(before).min(before));
            }
        }
        found
    }

    /// The two crossing lanes' directions, priority and signs.
    fn crossing_rank(&self, l: usize, m: usize) -> (bool, bool, bool, bool) {
        let oncoming = crate::traffic::wrap_deg(self.net.lanes[m].start_heading() - self.net.lanes[l].start_heading()).abs() > 135.0;
        let must_yield = self.net.must_yield(l, m);
        // between equals (no sign, no turn across the other) whoever gets there first
        // goes first (Omsi.exe: the path reserved first): the later one gives way
        // as to the right of way (see `FIRST_COME_MARGIN`)
        let equals = !must_yield && !self.net.must_yield(m, l);
        // signs decide between the two (a priority road, a ring) - and so does the
        // turn across the oncoming traffic, which gives way to it whoever decided
        // first (Omsi.exe FUN_00716970: a vehicle turning left yields to one that does
        // not, whatever was reserved). Decided by being first, a turn claimed from far
        // off held the oncoming queue at its green, half the cars of that approach
        // went through (#2431).
        let across = if self.net.left_hand { 2 } else { 1 };
        let signed = self.net.signed(l, m) || (oncoming && (self.net.lanes[l].turn == across) != (self.net.lanes[m].turn == across));
        (oncoming, must_yield, equals, signed)
    }

    /// `junction_stop`'s look at everybody on or coming to the lanes that cross car `i`'s
    /// way through junction `jn`, and at the people on its crossings.
    #[allow(clippy::too_many_arguments)]
    fn weigh_crossings(
        &self,
        i: usize,
        jn: &Junction,
        way: &[(usize, f32)],
        on_lane: &HashMap<usize, Vec<(usize, f32, f32, bool)>>,
        coming: &HashMap<usize, Vec<(usize, f32)>>,
        reservations: &HashMap<usize, Vec<usize>>,
        walkers: &HashMap<usize, Vec<f32>>,
        committed: bool,
        patience: f32,
        long_waiter: bool,
        explain: bool,
        mut stop_at: Option<f32>,
    ) -> Weighing {
        let car = &self.cars[i];
        let st = &car.state;
        let v = st.speed;
        let a_me = st.accel;
        let mut hard = false;
        // a matter of the rules (right of way, a full exit), not of someone in the way
        let mut ruled = false;
        let mut soft: Vec<usize> = Vec::new();
        let mut why: Vec<String> = Vec::new();
        let mut holds: Vec<(u64, Hold)> = Vec::new();
        let me_id = car.id;
        // (Omsi.exe: a vehicle whose script sets `TrafficPriority` takes a junction ahead of
        // any reservation but one of a vehicle already on the path, FUN_00716970; here an
        // emergency vehicle on an active drive, `emergency`)
        let prio = |c: &AiCar| self.emergency.is_siren(c.id);
        let me_prio = prio(car);
        // (on the movement kept for it, it waits only for who is in the junction or has
        // decided to go: not for those that stand for it)
        let reserved = me_prio && self.emergency.holds_movement(me_id, jn.lanes[0].0);
        let mut by: Option<u64> = None;
        let mut courtesy = false;
        let mut people = false;
        let mut let_in = false;
        let mut only_oncoming = true;
        let mut oncoming_meet: Option<f32> = None;
        let mut nearest: Option<f32> = None;
        let mut sign_yield = false;
        for &(l, dl) in &jn.lanes {
            for c in &self.net.crossings[l] {
                let point = dl + c.at;
                let m = c.other;
                if way.iter().any(|w| w.0 == m) {
                    continue; // its own way
                }
                // Where the bodies meet (`Crossing::before`/`after`): two paths that cross at a
                // shallow angle, or two turns bending towards each other, bring the cars
                // together metres before their centre lines cross (two turning cars used to
                // touch while the one that gave way still rolled towards "its" point). Paths
                // that run into one another (a merge) are ordered at the joint itself.
                if point + c.after < -car.tail - 0.3 {
                    continue; // passed already
                }
                let (oncoming, must_yield, equals, signed) = self.crossing_rank(l, m);
                // this car's front to the meeting place
                let my_room = point - c.before - st.front;
                // A driver who gives way by a sign and can still stop has not taken the way
                // from the traffic he gives way to by deciding early: his claim stands
                // against the others, not against them.
                let me_yields_by_sign = signed && must_yield && !me_prio && stops_before(v, my_room);
                // standing in the junction before a meeting place where it gives way (pulled
                // in to turn across the oncoming traffic, held up in a queue): it gives way
                // there as at a line
                let waiting_inside = jn.inside && v < 0.5 && my_room > 0.0 && must_yield;
                // (rolled over its line to stop before the meeting place, it is not decided)
                let me_decided = (committed && !me_yields_by_sign) || (jn.inside && !waiting_inside && !me_yields_by_sign);
                // (judged alike claimed or not: a car too fast for its line but able to stop
                // before the meeting place claimed and gave up its claim by turns)
                let sign_hold = me_yields_by_sign;
                let mut hold = |hard_: bool, o: u64, kind: Hold, holds: &mut Vec<(u64, Hold)>, stop_at: &mut Option<f32>| {
                    holds.push((o, kind));
                    sign_yield |= sign_hold;
                    nearest = Some(nearest.unwrap_or(f32::MAX).min(point - c.before));
                    if !oncoming {
                        only_oncoming = false;
                    } else {
                        oncoming_meet = Some(oncoming_meet.unwrap_or(f32::MAX).min(point - c.before));
                    }
                    if jn.inside && (hard_ || waiting_inside) {
                        *stop_at = Some(stop_at.unwrap_or(f32::MAX).min(point - c.before));
                    }
                };
                for (j, dj, is_on) in vehicles_toward(on_lane, coming, m, c.other_at) {
                    if j == i {
                        continue;
                    }
                    let o = &self.cars[j];
                    // it waits behind this car's body: it will not come before this one moves
                    if self.geo_prev.get(j).copied().flatten() == Some(me_id) {
                        continue;
                    }
                    if dj + c.other_after < -o.tail - 0.3 {
                        continue; // it is through
                    }
                    let t_clear = time_to(point + c.after + car.tail + 0.3, v, a_me)
                        + if v < 0.5 { st.reaction } else { 0.0 };
                    // when this car's front gets to the meeting place
                    let t_mine = time_to(my_room, v, a_me) + if v < 0.1 { st.reaction } else { 0.0 };
                    // A meeting place far into the junction's way (the far side of a
                    // roundabout its path runs round to) is not weighed at the line: the car
                    // goes in behind the traffic already on its way and gives way there if
                    // it has to (inside the junction every meeting place counts). Weighed
                    // at the line, an entry waited for a gap of nine seconds on a ring that
                    // never had one, and the queue behind it stood for minutes.
                    if !jn.inside && my_room > 25.0 {
                        continue;
                    }
                    // A stalled car neither claims an imminent arrival nor accelerates
                    // freely in the arrival prediction, even without a reservation.
                    // (queued: right behind a car that stands - not at its own line)
                    let queued = o.lead_info.is_some_and(|(lid, gap)| gap < 8.0 && self.car_by_id(lid).is_some_and(|x| x.state.speed < 1.0));
                    let stalled = (o.stopped > 4.0 && o.state.speed < 0.1) || o.crawl >= 8.0 || (o.state.speed < 1.5 && queued);
                    let theirs = dj - c.other_before - o.state.front;
                    // A car a red light holds short of the meeting place is not coming, claim or
                    // no claim. A car inside a crossing object (on the stretch before a light
                    // further in, or between two of its lights) still weighs its junction and
                    // claims its way through it while it waits at red; counted as coming, that
                    // claim held the cross traffic at its green until the waiting car had
                    // stood for a few seconds, and again after every metre it crept on.
                    let held_short = o.light_hold && o.light_at.is_some_and(|at| at <= dj - c.other_before + 0.5);
                    // (one going into a slow queue that lets it in keeps its claim standing)
                    let claimed = reservations
                        .get(&m)
                        .map(|r| r.contains(&j))
                        .unwrap_or(false)
                        && (!stalled || o.courtesy)
                        && !held_short;
                    if reserved && !is_on && !claimed {
                        continue;
                    }
                    // it waits for someone else before this meeting place (a car that gives
                    // way further on still rolls through here on its way to its line)
                    let waits_short = o.light_hold
                        || (o.yielding
                            && !claimed
                            && o.wait_at
                                .map(|w| w - 0.6 <= dj - c.other_before)
                                .unwrap_or(false));
                    // (it arrives in `t_j` seconds)
                    let t_j = crossing_arrival(&o.state, theirs, claimed, waits_short, stalled);
                    if is_on && theirs <= 0.3 {
                        // in the meeting place right now: unless this car is further in
                        // already (then it is the other one that has to wait)
                        let mine_in = -my_room;
                        let theirs_in = -theirs;
                        let ahead = mine_in > 0.0
                            && (mine_in > theirs_in + 0.3
                                || ((mine_in - theirs_in).abs() <= 0.3 && me_id > o.id));
                        if !ahead {
                            hard = true;
                            by = by.or(Some(o.id));
                            hold(true, o.id, Hold::Body, &mut holds, &mut stop_at);
                            if explain {
                                why.push(format!("car {} in the crossing of lanes {l}/{m}", o.id));
                            }
                        }
                        continue;
                    }
                    let o_prio = prio(o);
                    // (see `too_close_to_pull_out`)
                    if jn.inside && !committed && !car.reserved.contains(&l) && !waiting_inside && (must_yield || equals) && !me_prio && !waits_short && too_close_to_pull_out(v, my_room, &o.state, o.yielding, theirs, t_j, t_clear) {
                        hard = true;
                        by = by.or(Some(o.id));
                        hold(true, o.id, Hold::Rule, &mut holds, &mut stop_at);
                        if explain {
                            why.push(format!("car {} with the right of way arrives at {l}/{m} in {t_j:.1} s, too close to stop for this one", o.id));
                        }
                        continue;
                    }
                    // A driver going into a slow queue that lets him in, and the driver of
                    // that queue letting him: both judge it by `lets_in` of the queue's car.
                    // (between equals as well: the one who waits gave way by coming later)
                    let ring_entry = self.net.lanes[l].ring != self.net.lanes[m].ring;
                    let other_lets_in = long_waiter && !ring_entry && (must_yield || equals) && !o_prio && lets_in(o.state.speed, o.state.decel, theirs);
                    if other_lets_in {
                        let_in = true;
                        continue;
                    }
                    let waiter = (o.courtesy && claimed)
                        || (o.yielding && o.state.yield_time > COURTESY_WAIT && !queued && o.wait_at.is_some_and(|w| w - o.state.front < 3.0));
                    if waiter && !ring_entry && !must_yield && (self.net.must_yield(m, l) || equals) && !me_prio && lets_in(v, st.decel, my_room) {
                        courtesy = true;
                        by = by.or(Some(o.id));
                        hold(false, o.id, Hold::Courtesy, &mut holds, &mut stop_at);
                        if jn.inside {
                            stop_at = Some(stop_at.unwrap_or(f32::MAX).min(point - c.before));
                        }
                        if explain {
                            why.push(format!("lets car {} in from its line at {l}/{m} (it has waited {:.0} s)", o.id, o.state.yield_time));
                        }
                        continue;
                    }
                    // An early claim of a driver who gives way to this one by a sign does not
                    // take the right of way while he can still stop (he judges it alike).
                    // (with a margin: judged from its last step, a car right at the limit has
                    // gone on by now, and both went)
                    let they_yield_by_sign = signed && !must_yield && self.net.must_yield(m, l) && !o_prio && (theirs > firm_stop(o.state.speed) + 0.5 * o.state.speed + 2.0 || (o.state.speed < STANDING && stops_before(o.state.speed, theirs)));
                    if claimed || (is_on && o.state.speed > 0.5 && !waits_short) {
                        if they_yield_by_sign {
                            continue;
                        }
                        // Both have decided (or this one is in the junction already): the one
                        // that gets there first goes first, a tie goes to the lower number.
                        // Otherwise two cars standing at their lines, each with a claim,
                        // waited for each other for good.
                        let first = !me_decided || goes_first((t_j, theirs, o.state.speed, o.id), (t_mine, my_room, v, me_id));
                        if first && t_j < t_clear * if me_decided { 1.0 } else { patience } + 1.0 {
                            hard = true;
                            by = by.or(Some(o.id));
                            hold(true, o.id, if claimed && (!is_on || o.state.speed < 0.3) { Hold::Claim } else { Hold::Body }, &mut holds, &mut stop_at);
                            if explain {
                                why.push(format!("car {} ({}) arrives at {l}/{m} in {t_j:.1} s, this one in {t_mine:.1} s, clear in {t_clear:.1} s [its v {:.2} stood {:.1} crawl {:.1} yielding {} lead {:?} lane {} theirs {:.1}]", o.id, if claimed { "claimed" } else { "on it" }, o.state.speed, o.stopped, o.crawl, o.yielding, o.lead_info, o.state.lane, theirs));
                            }
                        }
                        continue;
                    }
                    // a vehicle with priority goes before one without, whatever the lanes say;
                    // one without gives way to it
                    let later = equals && t_j + FIRST_COME_MARGIN < t_mine;
                    if (me_decided && !waiting_inside) || (me_prio && !o_prio) || (!must_yield && !later && !(o_prio && !me_prio)) {
                        continue;
                    }
                    // An entry to a ring gives way to the ring's traffic standing close before
                    // the meeting place as well: it does not squeeze in ahead of a ring that
                    // queues (one standing farther back is a stalled queue it may go past),
                    // and its long wait does not outrank a ring car briefly stopped.
                    let ring_queue = self.net.lanes[m].ring && !self.net.lanes[l].ring && o.state.speed < 0.3 && theirs < 10.0;
                    // the gap a driver takes in the main road's traffic: the critical gap of
                    // 5 to 7.5 s (by driver), and time enough to be through
                    // (where the two run into one lane the other only has to be able to
                    // follow this car in: it need not be clear of the joint first, it must be
                    // there well ahead of the other - the gap a driver takes to turn into a
                    // road's traffic, not the one to cross it)
                    let gap = if c.merge { st.accept_gap.max(t_mine + 3.0) } else { (st.accept_gap + 2.0).max(t_clear + 1.0) };
                    if ring_queue || t_j < gap * patience {
                        if !ring_queue && (t_j == f32::MAX || o.state.speed < 0.3) {
                            soft.push(j);
                            hold(false, o.id, Hold::Rule, &mut Vec::new(), &mut stop_at);
                        } else {
                            ruled = true;
                            by = by.or(Some(o.id));
                            hold(false, o.id, Hold::Rule, &mut holds, &mut stop_at);
                            if explain {
                                why.push(format!("car {} has the right of way at {l}/{m}, arrives in {t_j:.1} s, clear in {t_clear:.1} s{}", o.id, if ring_queue { " (the ring's queue)" } else { "" }));
                            }
                        }
                    }
                }
                // The player's bus (and a LAN player's) on that lane or coming to it, by the
                // same rules: before, the cars saw it only as a box in their way a second and
                // a half ahead, and pulled out of a side road in front of a bus coming along
                // the main road, or turned across it.
                for u in &self.way_users {
                    let Some(&(_, dm)) = u.lanes.iter().find(|x| x.0 == m) else { continue };
                    let (verdict, t_j, t_clear) = way_user_verdict(st, u, dm, c, point, jn.inside && !waiting_inside, me_decided, me_prio, (must_yield, equals), patience);
                    // (in the way, or only having the right of way)
                    let by_rule = match verdict {
                        Verdict::Free => continue,
                        Verdict::Hard => false,
                        Verdict::Ruled => true,
                    };
                    *(if by_rule { &mut ruled } else { &mut hard }) = true;
                    by = by.or(Some(super::stats::WAITS_ON_PLAYER));
                    only_oncoming = false;
                    if (by_rule && waiting_inside) || (!by_rule && jn.inside) {
                        stop_at = Some(stop_at.unwrap_or(f32::MAX).min(point - c.before));
                    }
                    if explain {
                        why.push(if by_rule {
                            format!("the player's vehicle has the right of way at {l}/{m}, arrives in {t_j:.1} s, clear in {t_clear:.1} s")
                        } else {
                            format!("the player's vehicle in or at the crossing of lanes {l}/{m} (arrives in {t_j:.1} s, this one is clear in {t_clear:.1} s)")
                        });
                    }
                }
            }
            // people on a zebra or a signalled crossing
            if self.people_crossing(l, dl, st.front, walkers, explain.then_some(&mut why), &mut stop_at) {
                hard = true;
                people = true;
            }
        }
        Weighing { hard, ruled, soft, why, stop_at, by, courtesy, holds, people, only_oncoming, oncoming_meet, let_in, nearest, sign_yield }
    }

    // (the car ahead by the step's index, not a search of every car for every
    // car at every meeting place; one it does not know is looked for as before)
    fn car_by_id(&self, id: u64) -> Option<&AiCar> {
        self.index_of.get(&id).and_then(|&k| self.cars.get(k)).filter(|x| x.id == id).or_else(|| self.cars.iter().find(|x| x.id == id))
    }
}

// vehicles on the other lane, or coming to it
fn vehicles_toward<'a>(
    on_lane: &'a HashMap<usize, Vec<(usize, f32, f32, bool)>>,
    coming: &'a HashMap<usize, Vec<(usize, f32)>>,
    lane: usize,
    other_at: f32,
) -> impl Iterator<Item = (usize, f32, bool)> + 'a {
    let on = on_lane
        .get(&lane)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|e| !e.3)
        .map(move |&(j, sj, _, _)| (j, other_at - sj, true));
    let near = coming
        .get(&lane)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .map(move |&(j, dj)| (j, dj + other_at, false));
    on.chain(near)
}
