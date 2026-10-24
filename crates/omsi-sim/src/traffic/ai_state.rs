//! An AI road vehicle's driving: the lanes it plans ahead, lane and route changes, the speed
//! it wants (curves, leads, lights, stops) and the command it gives its body.

use super::*;

/// How far ahead (m) a car decides which way it goes: far enough to signal a turn in good
/// time and for the steering to look beyond the junction.
const PLAN_AHEAD: f32 = 90.0;
/// At most this many lanes are planned ahead (junction pieces can be a few metres long).
const PLAN_LANES: usize = 10;
/// Linked lanes may end up to 1.5 m apart; over this many metres on either side of a joint
/// the way is bent so that it has no step (half the gap on each side).
const JOINT_BLEND: f32 = 6.0;
/// Seconds of signalling before a lane change begins to move sideways.
pub const SIGNAL_BEFORE_CHANGE: f32 = 1.2;

/// One AI vehicle moving on the network: where it is along its lanes and which way it is
/// going. The body that follows this way is `ai_motion::AiBody`.
#[derive(Debug, Clone)]
pub struct AiState {
    pub traffic_pool: Option<(usize, std::sync::Arc<Vec<i32>>)>,
    /// The vehicle's `[ai_veh_type]` (0 car, 1 taxi, 2 bus, 3 truck; -1 a timetable bus):
    /// which lanes it may take, see [`Lane::allows`].
    pub veh_type: i32,
    pub lane: usize,
    pub s: f32,
    pub speed: f32,
    pub max_speed_kmh: f32,
    pub accel: f32,
    pub decel: f32,
    pub length: f32,
    pub rng: u64,
    pub blinker: i32,
    pub braking: bool,
    /// Seconds the brake lights have been on (they stay on at least `BRAKE_LIGHT_MIN`).
    pub brake_lamp: f32,
    /// Distance travelled, for wheel animation.
    pub odometer: f32,
    /// Lane chosen for after the current one.
    pub planned_next: Option<usize>,
    /// The lanes after `planned_next`, decided `PLAN_AHEAD` metres in advance.
    pub ahead: Vec<usize>,
    /// During a lane change: the way on from the lane the car is moving over to, chosen
    /// when the change begins, so that the steering and the speed for the bends already
    /// know it.
    pub change_plan: Vec<usize>,
    /// The lane the car came from (the way behind it, for the rear axle).
    pub prev_lane: Option<usize>,
    /// Seconds spent waiting to enter a crossing (deadlock breaker).
    pub yield_time: f32,
    /// Fixed lane sequence (timetable track); empty = wander randomly.
    pub route: Vec<usize>,
    /// Position in `route` of the current lane.
    pub route_index: usize,
    /// Lane change in progress.
    pub change: Option<LaneChange>,
    /// Seconds until the next lane change may start.
    pub change_cooldown: f32,
    /// Lateral offset from the lane (m, positive = right): bus bays, swerving round a
    /// parked car. It moves towards `lateral_target` along an S-curve over the distance
    /// driven (`lateral_ramp`), so that the car is straight again when it gets there and
    /// nothing moves while it stands.
    pub lateral: f32,
    pub lateral_target: f32,
    /// The S-curve in progress: (from, to, odometer at its start, its length in m).
    pub lateral_ramp: (f32, f32, f32, f32),
    /// A turn the car has taken a turn lane for (1 left, 2 right): the way out of the
    /// junction is chosen to match.
    pub turn_wish: i32,
    /// Indicator the driver sets for a manoeuvre of its own (pulling away from a stop),
    /// held while `signal_time` runs.
    pub signal: i32,
    pub signal_time: f32,
    /// Sideways acceleration the driver accepts in a bend (m/s²): the speed through a
    /// curve of radius r is at most sqrt(this × r).
    pub lat_accel: f32,
    /// The driver (`TPathInfo`'s rowdy_factor & co): how fast they like to go relative to
    /// the limit, the time gap they keep to the car ahead (s), the distance they stop
    /// behind it (m), the gap in the cross traffic they accept at a junction (s) and how
    /// long they take to move off when the way clears (s). `accel` is how hard they pull
    /// away and `decel` how hard they like to brake.
    pub desire: f32,
    pub headway: f32,
    pub min_gap: f32,
    pub accept_gap: f32,
    pub reaction: f32,
    /// From the vehicle's origin (the lane position `s`) to its front and rear bumper (m).
    pub front: f32,
    pub rear: f32,
    /// Standing still and held there (by a car ahead, a light, a stop): moving off again
    /// starts after `reaction`; `start_timer` counts it down.
    pub held: bool,
    pub start_timer: f32,
    /// Acceleration of the last step (m/s²).
    pub acc: f32,
    /// At most this much acceleration for now (m/s²): edging out round something standing
    /// close ahead.
    pub accel_cap: Option<f32>,
    /// The hardest braking of ordinary driving (m/s²): a light that turned yellow late, a
    /// car pulling in close ahead. Needing more than this is an emergency (`command`).
    pub ordinary_brake: f32,
    /// The hardest the vehicle brakes at all (m/s²): the ceiling of the emergency channel.
    pub emergency_brake: f32,
    /// How fast the ordinary command may grow (m/s³): a driver eases off the brake and
    /// onto the throttle again, rather than letting the brake go in one step.
    pub jerk: f32,
    /// The acceleration commanded in the last step (m/s²): where the jerk limit starts from.
    pub cmd: f32,
    /// The last step's command came from the collision-prevention channel (braking harder
    /// than `ordinary_brake` was needed), and that step began such a run (an emergency is
    /// counted once, however many steps it lasts).
    pub emergency: bool,
    pub emergency_began: bool,
    /// The body as the last step left it, seen from the way (see `BodyLink`).
    pub body: BodyLink,
    /// A small move of the way within the lane (m, + = right) that the body asks for to
    /// keep a corner off something beside it (`ai_traffic::scenery`): the S-curve in
    /// progress as `lateral_ramp`, (from, to, odometer at its start, its length in m). It
    /// bends only the way the body steers along, not the lateral place the planners see.
    pub nudge_ramp: (f32, f32, f32, f32),
    /// An emergency vehicle on an active drive (`ai_traffic::emergency`): it may go over the
    /// limit (`EMERGENCY_LIMIT_FACTOR`, at most `EMERGENCY_LIMIT_PLUS` km/h more), within its
    /// own top speed.
    pub priority_drive: bool,
    /// At most this speed for now (m/s): rolling aside at walking pace for an emergency
    /// vehicle, or one crossing against a red light on the junction kept for it.
    pub speed_cap: Option<f32>,
    /// A rail vehicle's traction as power over mass (W/kg): its acceleration falls with speed as
    /// `P / (m v)` once the starting effort is no longer what limits it. None for a road vehicle.
    pub rail_power: Option<f32>,
    pub bend: BendCache,
}

#[derive(Debug, Clone, Default)]
pub struct BendCache {
    lanes: Vec<usize>,
    cur: usize,
    offset: f32,
    lat: f32,
    next: f32,
    samples: std::collections::VecDeque<(f32, Option<f32>)>,
}

/// An emergency vehicle on an active drive takes the speed limit times this ...
pub const EMERGENCY_LIMIT_FACTOR: f32 = 1.5;
/// ... but at most this many km/h over it.
pub const EMERGENCY_LIMIT_PLUS: f32 = 30.0;

/// How the body stood after the last step, seen from the planner's way: how far the planner
/// is ahead of the body's own place on that way (m, negative behind it), how fast the body
/// went (m/s), and whether the body was found on the way at all (see
/// `ai_traffic::body_link`). Until a body has stepped nothing is known and nothing holds
/// the planner.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BodyLink {
    pub known: bool,
    pub on_way: bool,
    pub lead: f32,
    pub speed: f32,
}

/// The planner may be this far ahead of its body along the way (m) before it waits for it:
/// a body held back (by a wall it would not drive into, by something it hit) is not left
/// behind by its own plan. Run on further, the plan was a lane ahead of the car within
/// seconds, and everything the car decided was decided for a place it was not at.
pub const BODY_MAX_LEAD: f32 = 2.0;
/// Within this lead (m) the planner and the body are where they should be, and the plan
/// keeps its own speed (the body catches up with it within a step or two).
pub const BODY_SLACK: f32 = 0.5;

/// A driver's behaviour envelope, with its units: how hard they like to speed up and brake,
/// how hard ordinary driving may brake, the ceiling of an emergency stop, how fast they
/// change their command, the time gap and the standstill gap they keep, how long they take
/// to move off. Each car's comes from its seed (`ai_traffic::model::personality`) and
/// stays with the car.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    /// m/s²
    pub comfort_accel: f32,
    /// m/s²
    pub comfort_brake: f32,
    /// m/s²
    pub ordinary_brake: f32,
    /// m/s²
    pub emergency_brake: f32,
    /// m/s³
    pub jerk: f32,
    /// s
    pub headway: f32,
    /// m
    pub standstill_gap: f32,
    /// s
    pub reaction: f32,
}

/// What a car keeps its distance to: the gap from its front bumper to the thing (m), how
/// fast that is moving along the car's way (m/s) and how it is speeding up (m/s²).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Lead {
    pub gap: f32,
    pub speed: f32,
    pub acc: f32,
}

impl Lead {
    /// The nearer (more constraining) of two.
    pub fn min(a: Option<Lead>, b: Option<Lead>) -> Option<Lead> {
        match (a, b) {
            (Some(x), Some(y)) => Some(if y.gap < x.gap { y } else { x }),
            (x, None) => x,
            (None, y) => y,
        }
    }
}

/// Seconds a vehicle needs to cover `dist` metres from speed `v`, speeding up at `a` to at
/// most `v_max` (the earliest it can be there when nothing holds it back).
pub fn arrival_time(dist: f32, v: f32, a: f32, v_max: f32) -> f32 {
    if dist <= 0.0 {
        return 0.0;
    }
    let (v, a) = (v.max(0.0), a.max(0.05));
    let v_max = v_max.max(v).max(0.1);
    // speeding up to v_max takes (v_max - v) / a seconds and that much road
    let t_up = (v_max - v) / a;
    let d_up = (v + v_max) * 0.5 * t_up;
    if dist <= d_up {
        (-v + (v * v + 2.0 * a * dist).sqrt()) / a
    } else {
        t_up + (dist - d_up) / v_max
    }
}

/// How far along its S-curve (smoothstep, 0..1) a car moving back from the lane `side`
/// metres to the left has to be before its middle is `clear` metres from that lane's
/// middle (out of the way of the traffic there).
pub fn ramp_progress_for(side: f32, clear: f32) -> f32 {
    if clear <= 0.0 {
        return 0.0;
    }
    if side <= clear {
        return 1.0;
    }
    // the offset left is side × (1 − smoothstep(t)); it is clear once smoothstep(t) ≥ clear / side
    let want = clear / side;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..20 {
        let mid = 0.5 * (lo + hi);
        if smooth01(mid) < want {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Hardest braking of an AI driver (m/s²): an emergency stop.
pub const MAX_BRAKE: f32 = 8.0;
/// The hardest braking of ordinary driving (m/s²), before a car's own seed varies it.
pub const ORDINARY_BRAKE: f32 = 5.0;
/// How fast a driver eases the ordinary command off the brake and onto the throttle (m/s³).
pub const JERK: f32 = 8.0;
/// A car over its speed limit (a lower one has begun, or the profile down to it) sheds
/// this much of the excess a second (1/s): the free road's term alone fell to nothing as the
/// speed neared the limit, and a car crept down to the new limit for a hundred metres.
const LIMIT_GAIN: f32 = 2.0;
/// Standing this close behind a standing car (m, beyond the gap it keeps to it), a car stays
/// where it is: the model's pull towards the exact gap moved it on by a few centimetres at a
/// time whenever the one ahead rolled on a little, the brake lights going on and off.
const STANDSTILL_SLACK: f32 = 0.75;
/// Out of an emergency once the braking needed is this much (m/s²) under the ordinary
/// maximum again: right at the boundary the channel went on and off every step.
const EMERGENCY_HYSTERESIS: f32 = 0.5;
/// Gap a car leaves before a stop line or a stop point (m).
const STOP_LINE_GAP: f32 = 0.6;
/// The brake lights go on when the driver brakes harder than this (m/s²) and off again only
/// once he brakes less than `BRAKE_LIGHT_OFF`, after `BRAKE_LIGHT_MIN` seconds at least: easing
/// off a little is the throttle let go, not the brake. Lit at every -0.6 the command crossed,
/// the lamps flashed several times a minute on every car.
const BRAKE_LIGHT_ON: f32 = -1.0;
const BRAKE_LIGHT_OFF: f32 = -0.4;
const BRAKE_LIGHT_MIN: f32 = 0.6;
/// How hard a driver slows for a bend ahead (m/s²): early and gently. At 2 m/s² the car ran
/// up to the bend's speed profile and then braked at once, a jolt before every turn.
const BEND_DECEL: f32 = 1.2;
/// Braking gentler than this (m/s²) is put on at `BRAKE_JERK`, not in one step.
const GENTLE_BRAKE: f32 = 2.5;
const BRAKE_JERK: f32 = 5.0;

/// A lane change: the car moves over from its lane to `to` along `length` metres of road
/// (by distance, not by time: a car that has to stop halfway stands still, and so does its
/// way).
#[derive(Debug, Clone, Copy)]
pub struct LaneChange {
    pub to: usize,
    /// Progress 0..1.
    pub t: f32,
    pub length: f32,
    /// Position along `to`.
    pub s_to: f32,
    /// 1 left, 2 right.
    pub dir: i32,
    /// Seconds of indicating left before the car starts to move over.
    pub wait: f32,
    /// Pulling out from behind something standing: the car moves off sideways from a
    /// standstill, and what it leaves behind in its lane no longer holds it.
    pub bypass: bool,
}

/// Smoothstep 0..1.
pub(super) fn smooth01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A short sequence of lanes the way runs along, with the lane the car is on at `cur`.
struct LaneSeq {
    lanes: [usize; PLAN_LANES + 2],
    n: usize,
    cur: usize,
}

impl LaneSeq {
    /// Gap from the end of `lanes[i]` to the start of `lanes[i + 1]`, if they are linked
    /// (a gap of a few metres means they are not one road, and nothing is bent).
    fn gap(&self, net: &Network, i: usize) -> Option<DVec3> {
        let (a, b) = (&net.lanes[self.lanes[i]], &net.lanes[self.lanes[i + 1]]);
        let g = b.start() - a.end();
        (g.length() < 3.0).then_some(g)
    }

    /// Point and heading `u` metres into `lanes[i]` (beyond the ends of the first and last
    /// lane: straight on), with the joints on both sides smoothed.
    fn at(&self, net: &Network, i: usize, u: f32) -> (DVec3, f32) {
        let l = &net.lanes[self.lanes[i]];
        let (mut p, h) = l.at_ext(u);
        let w = |x: f32| 1.0 - smooth01(x / JOINT_BLEND);
        if i > 0 {
            if let Some(g) = self.gap(net, i - 1) {
                p -= g * (0.5 * w(u.max(0.0))) as f64;
            }
        }
        if i + 1 < self.n {
            if let Some(g) = self.gap(net, i) {
                p += g * (0.5 * w((l.length() - u).max(0.0))) as f64;
            }
        }
        (p, h)
    }

    /// Lane index in `lanes` and distance along it, `d` metres from distance `s` of the
    /// current lane.
    fn locate(&self, net: &Network, s: f32, d: f32) -> (usize, f32) {
        let mut i = self.cur;
        let mut u = s + d;
        while u < 0.0 && i > 0 {
            i -= 1;
            u += net.lanes[self.lanes[i]].length();
        }
        while i + 1 < self.n && u > net.lanes[self.lanes[i]].length() {
            u -= net.lanes[self.lanes[i]].length();
            i += 1;
        }
        (i, u)
    }

    /// Point and heading `d` metres from distance `s` of the current lane.
    fn point(&self, net: &Network, s: f32, d: f32) -> (DVec3, f32) {
        let (i, u) = self.locate(net, s, d);
        self.at(net, i, u)
    }

    fn heading(&self, net: &Network, s: f32, d: f32) -> f32 {
        let (i, u) = self.locate(net, s, d);
        net.lanes[self.lanes[i]].heading_ext(u)
    }
}

/// How many lanes deep a car looks for a way on that leads only into lanes closed to it.
const TRAP_DEPTH: u32 = 3;

impl AiState {
    pub fn new(lane: usize, s: f32, seed: u64) -> AiState {
        AiState { traffic_pool: None, veh_type: 0, lane, s, speed: 0.0, max_speed_kmh: 50.0, accel: 1.2, decel: 3.0, length: 5.0, rng: seed | 1, blinker: 0, braking: false, brake_lamp: 0.0, odometer: 0.0, planned_next: None, ahead: Vec::new(), change_plan: Vec::new(), prev_lane: None, yield_time: 0.0, route: Vec::new(), route_index: 0, change: None, change_cooldown: 5.0, lateral: 0.0, lateral_target: 0.0, lateral_ramp: (0.0, 0.0, 0.0, 1.0), turn_wish: 0, signal: 0, signal_time: 0.0, lat_accel: 2.8, desire: 1.0, headway: 1.4, min_gap: 2.0, accept_gap: 4.0, reaction: 0.7, front: 2.5, rear: 2.5, held: false, start_timer: 0.0, acc: 0.0, accel_cap: None, ordinary_brake: ORDINARY_BRAKE, emergency_brake: MAX_BRAKE, jerk: JERK, cmd: 0.0, emergency: false, emergency_began: false, body: BodyLink::default(), nudge_ramp: (0.0, 0.0, 0.0, 1.0), priority_drive: false, speed_cap: None, rail_power: None, bend: BendCache::default() }
    }

    fn rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// The lanes ahead, nearest first: `planned_next`, then the rest of the plan.
    pub fn upcoming(&self) -> impl Iterator<Item = usize> + '_ {
        self.planned_next.into_iter().chain(self.ahead.iter().copied())
    }

    /// A random way on from the end of `lane`. Lanes the map closes to this vehicle ([rule]
    /// no_cars, bus, trucks: `Lane::allows`) and lanes whose traffic density is zero are not driven
    /// into - filtering them only at spawn still let cars turn into a pedestrian street or a
    /// depot yard from next door. A car that has taken a turn lane takes the turn.
    pub(super) fn choose_after(&mut self, net: &Network, lane: usize) -> Option<usize> {
        let l = &net.lanes[lane];
        // (a car of a traffic pool - the trucks of a map that keeps them to its port roads -
        // takes only the ways its pool may go, as it was put on one. A forbidden exit
        // remains forbidden even if every exit is closed; normal end-of-route handling
        // must deal with that, rather than driving into a pedestrian or restricted lane.)
        let pool = self.traffic_pool.as_ref();
        let veh_type = self.veh_type;
        // may it drive from lane `from` into lane `n`?
        let open = |pooled: bool, from: usize, n: usize| -> bool {
            let (fl, nl) = (&net.lanes[from], &net.lanes[n]);
            let d = match pool.filter(|_| pooled) {
                Some((p, defaults)) => nl.pool_density(defaults, *p),
                None => nl.density,
            };
            nl.allows(veh_type) && d > 0.0
                // Reject the short return only when the graph links both ways.
                // Keep the graph itself intact: reach and upstream route choices
                // retain their existing behavior.
                && (!nl.next.contains(&from) || nl.continues_from(fl, LINK_TOLERANCE))
        };
        let open_to = |pooled: bool| -> Vec<usize> { l.next.iter().copied().filter(|&n| open(pooled, lane, n)).collect() };
        let pooled = self.traffic_pool.is_some().then(|| open_to(true));
        let weighted = pooled.is_some();
        let open_now = pooled.unwrap_or_else(|| open_to(false));
        let mut choices = open_now;
        // not into a lane that leads on only into lanes closed to it (a junction whose every
        // exit has density 0 for it): the car came to the end of that lane with nowhere to
        // go and vanished there in the middle of the road (#2466). The end of the network
        // itself (no lane on at all) is the map's edge, where Omsi.exe takes its cars away.
        // (looked at a few lanes deep: the trap is often a junction's paths after a short
        // piece of road)
        fn trapped(net: &Network, open: &dyn Fn(usize, usize) -> bool, n: usize, depth: u32) -> bool {
            let nl = &net.lanes[n];
            !nl.next.is_empty() && depth > 0 && nl.next.iter().all(|&m| !open(n, m) || trapped(net, open, m, depth - 1))
                || (!nl.next.is_empty() && depth == 0 && !nl.next.iter().any(|&m| open(n, m)))
        }
        let open_for = |from: usize, n: usize| open(weighted, from, n);
        let way_on: Vec<usize> = choices.iter().copied().filter(|&n| !trapped(net, &open_for, n, TRAP_DEPTH)).collect();
        if !way_on.is_empty() {
            choices = way_on;
        }
        // and a way that goes on rather than into the end of the network, where there is
        // the choice (the map's edge is where OMSI takes its cars away; a village like
        // Grundorf had a queue of twenty growing at the end of its one outbound road)
        if l.kind == LaneKind::Street && net.reach.len() == net.lanes.len() {
            let through: Vec<usize> = choices.iter().copied().filter(|&n| net.reach[n] >= DEAD_END).collect();
            if !through.is_empty() {
                choices = through;
            }
        }
        // Nothing open on but lanes of density 0 (a junction whose paths the map gives no
        // traffic of their own, as Berlin-Spandau's): Omsi.exe's `[rule] trafficdensity` is
        // where and how often it puts cars on the road, not where they may drive, so a car
        // drives on into what its vehicle type may use rather than end in the middle of the
        // road (it vanished there in plain view at 40 km/h, #2466). Lanes closed to it
        // (`no_cars`, bus and truck rules, Omsi.exe 0x71d714) stay closed, and so does a
        // lane the map closes to the car's pool by a rule of its own (a group's
        // `trafficdensity` other than the first, which is the path's density itself).
        if choices.is_empty() && l.kind == LaneKind::Street {
            let pool_closed = |nl: &Lane| pool.is_some_and(|(p, _)| *p != 0 && nl.group_density.iter().any(|&(g, v)| g as usize == *p && v <= 0.0));
            choices = l
                .next
                .iter()
                .copied()
                .filter(|&n| {
                    let nl = &net.lanes[n];
                    nl.kind == LaneKind::Street && nl.allows(veh_type) && !pool_closed(nl) && (!nl.next.contains(&lane) || nl.continues_from(l, LINK_TOLERANCE))
                })
                .collect();
        }
        if self.turn_wish != 0 {
            let wished: Vec<usize> = choices.iter().copied().filter(|&n| net.lanes[n].turn == self.turn_wish).collect();
            if !wished.is_empty() {
                choices = wished;
                self.turn_wish = 0;
            }
        }
        if choices.is_empty() {
            None
        } else {
            if let Some((pool, defaults)) = self.traffic_pool.clone().filter(|_| weighted) {
                let total: f32 = choices.iter().map(|&n| net.lanes[n].pool_density(&defaults, pool)).sum();
                let mut pick = (self.rand() >> 32) as f32 / (u32::MAX as f32 + 1.0) * total;
                for &n in &choices {
                    let weight = net.lanes[n].pool_density(&defaults, pool);
                    if pick < weight { return Some(n); }
                    pick -= weight;
                }
                choices.last().copied()
            } else { Some(choices[(self.rand() % choices.len() as u64) as usize]) }
        }
    }

    /// Decide the way on: the lane after the current one and enough lanes after it to
    /// cover `PLAN_AHEAD` metres (a timetable route simply is that plan).
    pub fn plan_next(&mut self, net: &Network) {
        if !self.route.is_empty() {
            // a route step onto a lane beside this one is a lane change, not the way on
            let mut k = self.route_index + 1;
            if self.route.get(k).map(|&b| net.parallel(self.lane, b)).unwrap_or(false) {
                k += 1;
            }
            self.planned_next = self.route.get(k).copied();
            self.ahead = self.route.iter().skip(k + 1).take(PLAN_LANES).copied().collect();
            return;
        }
        if self.planned_next.is_none() {
            self.ahead.clear();
        }
        let planned: f32 = self.upcoming().map(|l| net.lanes[l].length()).sum();
        let rest = net.lanes[self.lane].length() - self.s;
        if self.planned_next.is_some() && (rest + planned >= PLAN_AHEAD || self.ahead.len() >= PLAN_LANES) {
            return;
        }
        let mut plan: Vec<usize> = self.upcoming().collect();
        self.extend_plan(net, self.lane, rest, &mut plan);
        self.planned_next = plan.first().copied();
        self.ahead = plan.into_iter().skip(1).collect();
    }

    /// Choose lanes after `from` (which has `rest` metres left) until `plan` covers
    /// `PLAN_AHEAD` metres.
    fn extend_plan(&mut self, net: &Network, from: usize, rest: f32, plan: &mut Vec<usize>) {
        let mut dist = rest + plan.iter().map(|&l| net.lanes[l].length()).sum::<f32>();
        while dist < PLAN_AHEAD && plan.len() <= PLAN_LANES {
            let last = plan.last().copied().unwrap_or(from);
            match self.choose_after(net, last) {
                Some(n) => {
                    dist += net.lanes[n].length();
                    plan.push(n);
                }
                None => break,
            }
        }
    }

    /// Put the car on a fixed route starting at its first lane.
    pub fn set_route(&mut self, net: &Network, route: Vec<usize>, s: f32) {
        self.route = route;
        self.route_index = 0;
        if let Some(&l) = self.route.first() {
            self.lane = l;
            self.s = s;
        }
        self.prev_lane = None;
        self.plan_next(net);
    }

    /// Begin a lane change onto the neighbour `to` (`dir` 1 left, 2 right): the indicator
    /// goes on at once, the car moves over after a moment.
    pub fn start_change(&mut self, net: &Network, to: usize, dir: i32) {
        let Some(l) = net.lanes.get(self.lane) else { return };
        let Some(t) = net.lanes.get(to) else { return };
        // two to three and a half seconds at the speed the car has now, at least 12 m
        let duration = (3.5 - self.speed * 0.05).clamp(2.0, 3.5);
        let length = (self.speed * duration).max(12.0);
        // (the same share of a lane that starts beside this one, else the point beside the car)
        let s_to = net.beside_s(self.lane, to, self.s.min(l.length()));
        let rest = t.length() - s_to;
        self.change = Some(LaneChange { to, t: 0.0, length, s_to, dir, wait: SIGNAL_BEFORE_CHANGE, bypass: false });
        self.blinker = dir;
        let mut plan = Vec::new();
        if self.route.is_empty() {
            self.extend_plan(net, to, rest, &mut plan);
        } else {
            plan.extend(self.route.iter().skip(self.route_index + 2).take(PLAN_LANES).copied());
        }
        self.change_plan = plan;
    }

    /// How far the car still has to drive to distance `ss` of route lane `ri` (a lane it
    /// changes over from counts as the lane beside it).
    pub fn route_distance(&self, net: &Network, ri: usize, ss: f32) -> f32 {
        let (mut k, mut d) = (self.route_index, -self.s);
        if let Some(c) = self.change {
            if self.route.get(k + 1) == Some(&c.to) {
                k += 1;
                d = -c.s_to;
            }
        }
        while k < ri && k + 1 < self.route.len() {
            if !net.parallel(self.route[k], self.route[k + 1]) {
                d += net.lanes[self.route[k]].length();
            } else {
                // a lane beside that starts earlier is further along at the same place
                d -= net.beside_delta(self.route[k], self.route[k + 1]);
            }
            k += 1;
        }
        d + ss
    }

    /// A timetable route that moves over to the lane beside the current one next: that lane
    /// and the side it lies on (1 left, 2 right). The driver starts the move as soon as the
    /// lane is free (`start_route_change`).
    pub fn route_change_due(&self, net: &Network) -> Option<(usize, i32)> {
        if self.route.is_empty() || self.change.is_some() {
            return None;
        }
        let &b = self.route.get(self.route_index + 1)?;
        if !net.parallel(self.lane, b) {
            return None;
        }
        // a lane beside that begins further on: not before the car is level with its start
        let delta = net.beside_delta(self.lane, b);
        if delta < 0.0 && self.s < -delta + 1.0 {
            return None;
        }
        let (la, lb) = (&net.lanes[self.lane], &net.lanes[b]);
        // (compared where the car is: a lane beside may start well before or after this one)
        let (pa, ha) = la.at(self.s.min(la.length()));
        let pb = lb.at(net.beside_s(self.lane, b, self.s)).0;
        let h = (ha as f64).to_radians();
        let side = (pb - pa).truncate().dot(DVec2::new(h.cos(), -h.sin()));
        Some((b, if side > 0.0 || (side.abs() < 0.5 && lb.offset > la.offset) { 2 } else { 1 }))
    }

    /// Pull out into `to` from behind a standing obstacle: a short, steep move.
    pub fn start_bypass(&mut self, net: &Network, to: usize, dir: i32) {
        self.start_change(net, to, dir);
        if let Some(c) = self.change.as_mut() {
            c.length = 8.0;
            c.bypass = true;
        }
    }

    /// Start the lane change a route asks for: at once (the indicator has been on while the
    /// driver waited for a gap) and quick enough to be done well before a fork's branches part.
    pub fn start_route_change(&mut self, net: &Network, to: usize, dir: i32) {
        self.start_change(net, to, dir);
        let left = (net.lanes[self.lane].length() - self.s).max(0.0);
        if let Some(c) = self.change.as_mut() {
            c.wait = 0.0;
            c.length = c.length.min(left * 0.7).max(6.0);
        }
    }

    /// The lane sequence the car is driving along: where it came from, where it is, the plan.
    fn seq(&self) -> LaneSeq {
        let mut q = LaneSeq { lanes: [0; PLAN_LANES + 2], n: 0, cur: 0 };
        if let Some(p) = self.prev_lane {
            q.lanes[0] = p;
            q.n = 1;
        }
        q.cur = q.n;
        q.lanes[q.n] = self.lane;
        q.n += 1;
        for l in self.upcoming() {
            if q.n == q.lanes.len() {
                break;
            }
            q.lanes[q.n] = l;
            q.n += 1;
        }
        q
    }

    /// The lane sequence of the lane the car is changing to, with the way on it chose.
    fn change_seq(&self, to: usize) -> LaneSeq {
        let mut q = LaneSeq { lanes: [0; PLAN_LANES + 2], n: 1, cur: 0 };
        q.lanes[0] = to;
        for &l in &self.change_plan {
            if q.n == q.lanes.len() {
                break;
            }
            q.lanes[q.n] = l;
            q.n += 1;
        }
        q
    }

    /// The point of the car's way `d` metres ahead of it (behind for negative `d`): along
    /// its lanes with the joints smoothed, over to the new lane during a lane change, and
    /// out to the side where it pulls into a bay or round a parked car. The steering
    /// follows this curve; it has no steps, so neither does the car.
    pub fn way_point(&self, net: &Network, d: f32) -> DVec3 {
        let (mut p, mut h) = self.seq().point(net, self.s, d);
        if let Some(c) = self.change {
            if c.to < net.lanes.len() {
                let (pt, ht) = self.change_seq(c.to).point(net, c.s_to, d);
                // how far the change will have got by the time the car is `d` metres on
                let tau = (d - c.wait * self.speed.max(2.0)) / c.length;
                let k = smooth01(c.t + tau);
                p = p.lerp(pt, k as f64);
                h += wrap_deg(ht - h) * k;
            }
        }
        let lat = self.lateral_at(self.odometer + d) + self.nudge_at(self.odometer + d);
        if lat.abs() > 1e-3 {
            let hr = (h as f64).to_radians();
            p += DVec3::new(hr.cos(), -hr.sin(), 0.0) * lat as f64;
        }
        p
    }

    /// How fast the car may be going now so that it can take every bend of the next stretch
    /// of its way at `lat_accel`, braking gently (`BEND_DECEL`) into the tight ones. Without this
    /// the cars swept round a junction at 40 km/h - well over a g. The bend is the lane's
    /// own curvature or the turn of the way over 6 m, whichever is sharper: lanes are linked
    /// with up to 40° between them, and such a kink is a bend too.
    pub fn curve_speed(&self, net: &Network) -> f32 {
        let mut best = self.curve_speed_on(net, &self.seq(), self.s);
        if let Some(c) = self.change {
            if c.to < net.lanes.len() {
                best = best.min(self.curve_speed_on(net, &self.change_seq(c.to), c.s_to));
            }
        }
        best
    }

    fn bend_reach(&self) -> f32 {
        (self.speed * self.speed / (2.0 * BEND_DECEL) + 12.0).min(130.0)
    }

    fn bend_lat(&self) -> f32 {
        // (an emergency vehicle on its drive takes the bends harder)
        self.lat_accel * if self.priority_drive { 1.5 } else { 1.0 }
    }

    fn bend_at(net: &Network, q: &LaneSeq, s: f32, d: f32, lat: f32) -> Option<f32> {
        let (i, u) = q.locate(net, s, d);
        let turn = wrap_deg(q.heading(net, s, d + 3.0) - q.heading(net, s, d - 3.0)).abs().to_radians() / 6.0;
        let k = net.lanes[q.lanes[i]].curvature_at(u).abs().max(turn);
        (k > 1e-4).then(|| (lat / k).sqrt().max(2.5))
    }

    pub fn curve_speed_cached(&mut self, net: &Network) -> f32 {
        if self.change.is_some() || omsi_cfg::flags::OMSI_NO_BEND_CACHE.is_set() {
            return self.curve_speed(net);
        }
        let q = self.seq();
        let (s, lat, reach) = (self.s, self.bend_lat(), self.bend_reach());
        let offset = s - self.odometer;
        let c = &mut self.bend;
        if c.cur != q.cur || c.lanes[..] != q.lanes[..q.n] || c.lat != lat || (c.offset - offset).abs() > 0.05 {
            c.lanes.clear();
            c.lanes.extend_from_slice(&q.lanes[..q.n]);
            c.cur = q.cur;
            c.lat = lat;
            c.offset = offset;
            c.samples.clear();
            c.next = s + 2.5 - self.odometer.rem_euclid(2.5);
        }
        while c.samples.front().is_some_and(|x| x.0 <= s) {
            c.samples.pop_front();
        }
        while c.next <= s {
            c.next += 2.5;
        }
        while c.next - s <= reach {
            c.samples.push_back((c.next, Self::bend_at(net, &q, 0.0, c.next, lat)));
            c.next += 2.5;
        }
        let mut best = Self::bend_at(net, &q, s, 0.0, lat).unwrap_or(f32::MAX);
        for &(r, v) in &c.samples {
            let d = r - s;
            if d > reach {
                break;
            }
            if let Some(v) = v {
                best = best.min((v * v + 2.0 * BEND_DECEL * (d - 2.5).max(0.0)).sqrt());
            }
        }
        best
    }

    fn curve_speed_on(&self, net: &Network, q: &LaneSeq, s: f32) -> f32 {
        let reach = self.bend_reach();
        let lat = self.bend_lat();
        let mut best = f32::MAX;
        // The samples lie at fixed places of the road (every 2.5 m of the odometer), not at
        // fixed distances ahead of the car: moving with the car, the sample that caught a
        // bend jumped 2.5 m nearer every 2.5 m, the allowed speed fell in steps of 4 m²/s²
        // and the car braked hard, let go and braked hard again with nothing in front of it.
        let first = 2.5 - self.odometer.rem_euclid(2.5);
        let mut d = 0.0f32;
        let mut next = first;
        while d <= reach {
            if let Some(v) = Self::bend_at(net, q, s, d, lat) {
                // (a bend may begin anywhere up to a sample's spacing before the sample
                // that finds it: the speed is taken from there, or the car met the start of
                // a tight turn half a metre after its profile had allowed 1 m/s more)
                best = best.min((v * v + 2.0 * BEND_DECEL * (d - 2.5).max(0.0)).sqrt());
            }
            d = next;
            next += 2.5;
        }
        best
    }

    /// The sideways offset of the way `d` metres ahead of the car (m, + = right).
    pub fn lateral_ahead(&self, d: f32) -> f32 {
        self.lateral_at(self.odometer + d)
    }

    /// The sideways offset of the way when the odometer reads `x`.
    fn lateral_at(&self, x: f32) -> f32 {
        let (from, to, x0, len) = self.lateral_ramp;
        from + (to - from) * smooth01((x - x0) / len.max(0.1))
    }

    /// The body's nudge within the lane when the odometer reads `x` (see `nudge_ramp`).
    pub fn nudge_at(&self, x: f32) -> f32 {
        let (from, to, x0, len) = self.nudge_ramp;
        from + (to - from) * smooth01((x - x0) / len.max(0.1))
    }

    /// Nudge the way `to` metres to the side (+ = right) over the next `len` metres, from
    /// where the nudge is now.
    pub fn nudge_to(&mut self, to: f32, len: f32) {
        if (self.nudge_ramp.1 - to).abs() < 1e-3 {
            return;
        }
        let from = self.nudge_at(self.odometer);
        self.nudge_ramp = (from, to, self.odometer, len.max(1.0));
    }

    /// The first turn (1 left, 2 right) on the way within `within` metres, with the
    /// distance to the start of the turning lane (0 while on it).
    pub fn turn_ahead(&self, net: &Network, within: f32) -> Option<(i32, f32)> {
        let here = &net.lanes[self.lane];
        if here.turn != 0 {
            return Some((here.turn, 0.0));
        }
        let mut dist = here.length() - self.s;
        for l in self.upcoming() {
            if dist > within {
                break;
            }
            let lane = &net.lanes[l];
            if lane.turn != 0 {
                return Some((lane.turn, dist));
            }
            dist += lane.length();
        }
        None
    }

    /// Set the indicator for what the car is doing or about to do: a lane change, its own
    /// manoeuvre (pulling away from a stop), moving sideways into a bay or out round a
    /// parked car and back, and a turn at the junction ahead - from about four seconds
    /// before it (at least 25 m, at most 60 m) until the turn is done.
    pub fn update_blinker(&mut self, net: &Network) {
        self.blinker = if let Some(c) = self.change {
            c.dir
        } else if self.signal != 0 && self.signal_time > 0.0 {
            self.signal
        } else if (self.lateral_target - self.lateral).abs() > 0.3 {
            if self.lateral_target > self.lateral { 2 } else { 1 }
        } else {
            let within = (self.speed * 4.0).clamp(25.0, 60.0);
            self.turn_ahead(net, within).map(|t| t.0).unwrap_or(0)
        };
    }

    /// Advance along the network. `obstacle` = distance from the car's origin to the rear of
    /// a standing vehicle ahead (m), `stop_at` = distance from its origin to a stop line
    /// (m). False at a dead end (the car is taken off the road).
    pub fn advance(&mut self, net: &Network, dt: f32, obstacle: Option<f32>, stop_at: Option<f32>) -> bool {
        let lead = obstacle.map(|d| Lead { gap: d - self.front, speed: 0.0, acc: 0.0 });
        self.drive(net, dt, lead, stop_at)
    }

    /// The acceleration the driver wants (the Intelligent Driver Model): towards the
    /// desired speed on a free road, and a braking term for what is ahead that is gentle
    /// (`decel`) while there is room and only as hard as it must be when there is not.
    /// `lead` is the vehicle ahead, `stop` the distance from the car's origin to where it
    /// has to stop (a light, a junction it gives way at, a bus stop).
    pub fn desired_accel(&self, net: &Network, lead: Option<Lead>, stop: Option<f32>) -> f32 {
        self.desired_accel_bend(net, lead, stop, &|| self.curve_speed(net))
    }

    fn desired_accel_bend(&self, net: &Network, lead: Option<Lead>, stop: Option<f32>, bend: &dyn Fn() -> f32) -> f32 {
        let Some(lane) = net.lanes.get(self.lane) else { return 0.0 };
        // (an emergency vehicle on its drive pulls away hard)
        let (a, b) = (self.accel.max(0.1) * if self.priority_drive { 1.6 } else { 1.0 }, self.decel.max(0.5));
        let v = self.speed;
        // a rail vehicle's pull falls with speed at constant power (and its adhesion with it)
        let a = power_limited(a, self.rail_power, v);
        let limit = |l: &Lane| {
            // (the bonus is the driver's whole margin: his own liking for speed on top of it
            // would take him past it)
            let wanted = if self.priority_drive {
                (l.speed_limit_kmh * EMERGENCY_LIMIT_FACTOR).min(l.speed_limit_kmh + EMERGENCY_LIMIT_PLUS)
            } else {
                l.speed_limit_kmh * self.desire
            };
            wanted.min(self.max_speed_kmh).max(3.0) / 3.6
        };
        let mut v0 = limit(lane);
        if let Some(cap) = self.speed_cap {
            v0 = v0.min(cap.max(0.3));
        }
        // a lower limit on the lanes ahead (a junction's turning lanes, a 30 zone) is
        // reached at that speed, slowing gently (1.2 m/s²) from where it has to: taken only
        // on entering the lane, the new limit threw the model into a -3 m/s² stop at every
        // junction, with nothing in front of the car
        {
            let reach = (v * v / 2.4 + 10.0).min(120.0);
            let mut d = lane.length() - self.s;
            for l in self.upcoming() {
                if d > reach {
                    break;
                }
                if let Some(nl) = net.lanes.get(l) {
                    let vl = limit(nl);
                    if vl < v0 {
                        v0 = v0.min((vl * vl + 2.0 * 1.2 * d.max(0.0)).sqrt());
                    }
                    d += nl.length();
                }
            }
        }
        let mut acc = a * (1.0 - (v / v0).powi(4)).max(-1.5 * b / a);
        if v > v0 {
            // over the limit: back down to it in proportion to the excess, not by the free
            // road's term, which fades out as the speed nears the limit
            acc = acc.min(-(LIMIT_GAIN * (v - v0)).min(1.5 * b));
        }
        // bends: follow the speed profile `curve_speed` lays out (it assumes `BEND_DECEL` of
        // braking), blending in over the last metre per second above it
        let bend = bend();
        if bend < v0 && v > bend - 1.0 {
            let track = -BEND_DECEL + (bend - v) / 0.6;
            let k = ((v - (bend - 1.0)) / 1.0).clamp(0.0, 1.0);
            acc = acc.min(acc + (track - acc) * k);
        }
        let interaction = |gap: f32, lead_speed: f32, s0: f32, headway: f32| -> f32 {
            let s_star = s0 + (v * headway + v * (v - lead_speed) / (2.0 * (a * b).sqrt())).max(0.0);
            -a * (s_star / gap.max(0.05)).powi(2)
        };
        let mut out = acc;
        if let Some(l) = lead {
            let idm = acc + interaction(l.gap, l.speed.max(0.0), self.min_gap, self.headway);
            // The constant-acceleration heuristic (the "ACC" variant of the model): how hard
            // the driver has to brake if the car ahead keeps doing what it does. A car that
            // has just cut in close ahead but drives almost as fast calls for a firm brake
            // and a gap that opens again over the next seconds, not an emergency stop.
            let (vl, s, al) = (l.speed.max(0.0), l.gap.max(0.05), l.acc.min(a));
            let den = vl * vl - 2.0 * s * al;
            let cah = if vl * (v - vl) <= -2.0 * s * al && den > 1e-3 { v * v * al / den } else { al - (v - vl).max(0.0).powi(2) / (2.0 * s) };
            // (a car beside or just ahead that drives away faster gives the model a gap of
            // nothing: its braking term is bounded so that the heuristic decides)
            let idm = idm.max(-2.0 * MAX_BRAKE);
            let acc_lead = if idm >= cah { idm } else { 0.01 * idm + 0.99 * (cah + b * ((idm - cah) / b).tanh()) };
            out = out.min(acc_lead);
        }
        if let Some(d) = stop {
            // A stop line: far away the model's braking term (without a time gap, the line
            // does not move off), gently; once the constant deceleration that stops the car
            // at the line reaches half the driver's comfortable braking, that deceleration.
            // The model alone braked twice as hard as needed when a light turned yellow 25 m
            // ahead, and then eased off.
            let room = (d - self.front - STOP_LINE_GAP).max(0.02);
            let need = v * v / (2.0 * room);
            let idm = (acc + interaction(d - self.front, 0.0, STOP_LINE_GAP, 0.2)).max(-0.5 * b);
            let constant = -need * 1.05;
            let k = ((need - 0.35 * b) / (0.15 * b)).clamp(0.0, 1.0);
            let a_stop = idm + (constant - idm) * k;
            out = out.min(a_stop);
        }
        out.clamp(-MAX_BRAKE, a)
    }

    /// The driver's envelope (see `Envelope`).
    pub fn envelope(&self) -> Envelope {
        Envelope {
            comfort_accel: self.accel,
            comfort_brake: self.decel,
            ordinary_brake: self.ordinary_brake,
            emergency_brake: self.emergency_brake,
            jerk: self.jerk,
            headway: self.headway,
            standstill_gap: self.min_gap,
            reaction: self.reaction,
        }
    }

    /// The acceleration the driver commands for the next `dt` seconds (m/s²), from what
    /// the model wants (`desired_accel`) in two channels. Ordinarily the command stays
    /// within the envelope: no harder than `ordinary_brake`, and it grows by at most `jerk`
    /// a second (more braking goes on at once). Where
    /// braking harder than the ordinary maximum is needed, the collision-prevention channel
    /// takes over: up to `emergency_brake`, at once, and said (`emergency`,
    /// `emergency_began`) - before, such a stop was only the model's output clipped at the
    /// ceiling, and nobody could tell it from a firm brake. A car standing within
    /// `STANDSTILL_SLACK` of the gap it keeps behind a standing car stays there.
    pub fn command(&mut self, net: &Network, dt: f32, lead: Option<Lead>, stop: Option<f32>) -> f32 {
        let bend = if self.lane < net.lanes.len() { self.curve_speed_cached(net) } else { 0.0 };
        let want = self.desired_accel_bend(net, lead, stop, &|| bend);
        let was = self.emergency;
        let limit = -self.ordinary_brake.max(self.decel);
        self.emergency = want < limit || (was && want < limit + EMERGENCY_HYSTERESIS);
        self.emergency_began = self.emergency && !was;
        if self.emergency {
            return want.max(-self.emergency_brake.max(self.ordinary_brake));
        }
        let mut target = want.max(limit);
        let from = if was { want.max(self.cmd) } else { self.cmd };
        if self.speed < 0.05 {
            // (only behind a car: up to a line the model stops where it should, and the
            // junctions count on a car waiting right at its line - held short of it as
            // well, a car waiting to turn and one driving through met at a Spandau junction)
            if lead.is_some_and(|l| l.speed < 0.3 && l.gap < self.min_gap + STANDSTILL_SLACK) {
                target = target.min(0.0);
            }
            if target <= 0.0 {
                return target;
            }
            // (moving off from rest starts from nothing)
            let step = self.jerk * dt;
            return from.max(0.0) + (target - from.max(0.0)).clamp(-step, step);
        }
        let step = self.jerk * dt;
        if target < from {
            // (more braking goes on at once: put on at `jerk` too, the queue rolling past
            // the side road in `scenario_tests` closed up behind each car that braked and
            // never left the side road's car a gap. Gentle braking is eased into - the
            // throttle let go at once, the brake pressed over a moment - else every small
            // correction was a jolt)
            if target > -GENTLE_BRAKE {
                return (from.min(0.0) - BRAKE_JERK * dt).max(target);
            }
            return target;
        }
        from + (target - from).clamp(-step, step)
    }

    /// Advance along the network with the car ahead (`lead`) and a stop point (`stop`,
    /// distance from the car's origin). False at a dead end.
    pub fn drive(&mut self, net: &Network, dt: f32, lead: Option<Lead>, stop: Option<f32>) -> bool {
        self.change_cooldown = (self.change_cooldown - dt).max(0.0);
        self.signal_time = (self.signal_time - dt).max(0.0);
        if self.signal_time <= 0.0 {
            self.signal = 0;
        }
        if net.lanes.get(self.lane).is_none() {
            return false;
        }
        // The body owns the pose (`ai_traffic::body_link`): while it lags behind the plan,
        // or stands off its way, the command starts from the body's own speed, and the
        // plan goes no further than `BODY_MAX_LEAD` ahead of it.
        let body = self.body;
        if body.known && (body.lead > BODY_SLACK || !body.on_way) {
            self.speed = self.speed.min(body.speed.max(0.0));
        }
        let room = if body.known { (BODY_MAX_LEAD - body.lead).max(0.0) } else { f32::INFINITY };
        let mut acc = self.command(net, dt, lead, stop);
        if let Some(cap) = self.accel_cap {
            acc = acc.min(cap);
        }
        // standing: a driver who is held there moves off only after a moment when the way
        // clears (the wave that runs down a queue at a green light)
        if self.speed < 0.05 {
            if acc <= 0.05 {
                // (a hold of a frame or two - a junction that is free and not free by turns
                // as the cars on the ring come and go - winds the reaction back only a
                // little: set back whole every frame, it never ran out, and the car stood at
                // an empty roundabout for minutes, "about to go")
                self.start_timer = if self.held { (self.start_timer + 3.0 * dt).min(self.reaction) } else { self.reaction };
                self.held = true;
                acc = acc.min(0.0);
            } else if self.held {
                self.start_timer -= dt;
                if self.start_timer > 0.0 {
                    acc = 0.0;
                } else {
                    self.held = false;
                }
            }
        } else if self.speed > 0.5 {
            self.held = false;
        }
        self.cmd = acc;
        let v0 = self.speed;
        let mut v1 = (v0 + acc * dt).max(0.0);
        let mut ds = (v0 + v1) * 0.5 * dt;
        if ds > room {
            // (waiting for the body: at its pace)
            ds = room;
            v1 = v1.min(body.speed.max(0.0));
        }
        self.speed = v1;
        self.acc = if dt > 0.0 { (v1 - v0) / dt } else { 0.0 };
        // brake lights: braking, or holding the car on the brake
        let press = acc < BRAKE_LIGHT_ON || (v1 < 0.3 && self.held);
        if self.braking {
            self.brake_lamp += dt;
            if !press && acc > BRAKE_LIGHT_OFF && !(v1 < 0.3 && self.held) && self.brake_lamp >= BRAKE_LIGHT_MIN {
                self.braking = false;
            }
        } else if press {
            self.braking = true;
            self.brake_lamp = 0.0;
        }
        self.s += ds;
        self.odometer += ds;
        // lane change: signal, glide over to the neighbour, then continue there
        if let Some(mut c) = self.change {
            c.s_to += ds;
            if c.wait > 0.0 {
                c.wait = (c.wait - dt).max(0.0);
            } else {
                c.t += ds / c.length;
            }
            let from_len = net.lanes[self.lane].length();
            let mut to_len = net.lanes.get(c.to).map(|l| l.length()).unwrap_or(0.0);
            // A move that is not over where a lane ends carries on across the joint: the
            // lane beside goes on into the way chosen on it, and this lane into its plan
            // (below). Finished at the joint instead, the car jumped sideways by what was
            // left of the move - so a lane change was only started 20 m and more before a
            // lane's end, and on a road of short spline pieces a car stopped behind a parked
            // car just past a joint never got round it. (A timetable vehicle's route moves
            // over well before the end, as before.)
            let carry_on = self.route.is_empty();
            while carry_on && c.t < 1.0 && c.s_to >= to_len && !self.change_plan.is_empty() {
                c.s_to -= to_len;
                c.to = self.change_plan.remove(0);
                to_len = net.lanes.get(c.to).map(|l| l.length()).unwrap_or(0.0);
            }
            let from_ends = self.s >= from_len && (self.planned_next.is_none() || !carry_on);
            if c.t >= 1.0 || from_ends || c.s_to >= to_len {
                // arrived on the new lane: it has no joint behind the car, and the way on
                // is planned afresh from there
                if self.route.get(self.route_index + 1) == Some(&c.to) {
                    self.route_index += 1;
                }
                self.lane = c.to;
                self.s = c.s_to.min(to_len);
                self.change = None;
                self.change_cooldown = 6.0;
                self.prev_lane = None;
                let plan = std::mem::take(&mut self.change_plan);
                self.planned_next = plan.first().copied();
                self.ahead = plan.into_iter().skip(1).collect();
            } else {
                self.change = Some(c);
            }
        }
        // bay offset: a new target starts an S-curve from where the car is now, eight metres
        // long for every metre sideways (at least 8, at most 30)
        if (self.lateral_target - self.lateral_ramp.1).abs() > 1e-3 {
            let from = self.lateral_at(self.odometer - ds);
            let len = ((self.lateral_target - from).abs() * 8.0).clamp(8.0, 30.0);
            self.lateral_ramp = (from, self.lateral_target, self.odometer - ds, len);
        }
        self.lateral = self.lateral_at(self.odometer);
        if self.planned_next.is_none() {
            self.plan_next(net);
        }
        while (self.change.is_none() || self.route.is_empty()) && self.s >= net.lanes[self.lane].length() {
            let l = &net.lanes[self.lane];
            let Some(next) = self.planned_next else {
                // the end of a flight path: the aircraft flies on straight (the way runs on
                // along the end tangent) until the traffic takes it away out of sight
                if l.kind == LaneKind::Air {
                    break;
                }
                return false; // dead end: despawn
            };
            // A timetable may name a path whose intervening map piece is missing.
            // Never move the vehicle directly to the next path across that gap.
            if !self.route.is_empty()
                && !l.next.contains(&next)
                && (net.lanes[next].start() - l.end()).truncate().length() >= 2.0
            {
                return false;
            }
            self.s -= l.length();
            self.prev_lane = Some(self.lane);
            self.lane = next;
            if self.route.is_empty() {
                self.planned_next = if self.ahead.is_empty() { None } else { Some(self.ahead.remove(0)) };
            } else {
                // the plan may have stepped over a lane-change entry of the route
                let from = self.route_index + 1;
                self.route_index = (from..(from + 3).min(self.route.len())).find(|&k| self.route[k] == next).unwrap_or(from);
            }
            self.plan_next(net);
        }
        // keep the plan PLAN_AHEAD metres long as the car eats into it (a route's plan
        // only moves when the car enters its next lane)
        if self.route.is_empty() {
            self.plan_next(net);
        }
        true
    }
}

/// The pull of a vehicle at constant power: at most `p / v` (W/kg over m/s) once that is below the
/// starting effort `a`, never below 0.08 m/s² so that it does not stall on a long climb.
pub(crate) fn power_limited(a: f32, power: Option<f32>, v: f32) -> f32 {
    match power {
        Some(p) => a.min((p / v.max(1.0)).max(0.08)),
        None => a,
    }
}

#[cfg(test)]
mod power_tests {
    use super::power_limited;

    #[test]
    fn a_road_vehicle_is_not_limited() {
        assert_eq!(power_limited(1.4, None, 30.0), 1.4);
    }

    #[test]
    fn the_pull_falls_with_speed_at_constant_power() {
        // 12 W/kg: 1.2 m/s² at 10 m/s, 0.6 at 20, the start effort at a crawl
        assert!((power_limited(1.5, Some(12.0), 10.0) - 1.2).abs() < 1e-5);
        assert!((power_limited(1.5, Some(12.0), 20.0) - 0.6).abs() < 1e-5);
        assert_eq!(power_limited(1.0, Some(12.0), 0.2), 1.0);
        assert_eq!(power_limited(1.0, Some(12.0), 500.0), 0.08);
    }
}
