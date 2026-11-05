//! Traffic light programs: the cycle clock, its stops and jumps, and the aspects it shows.

/// A `[traffic_light_stop]` / `[traffic_light_jump]` of a light program (`TAmpelStop`:
/// ampel, time, jumptime, ifAnf). When the cycle clock reaches `time` and the condition
/// holds, the clock waits there (stop) or continues at `jump_to` (jump). With `if_request`
/// set the condition is "nobody is asking at `light`": the stock railway crossings keep the
/// road green at 1 s until a train approaches, the bus loop of Heerstraße skips its bus
/// phase when no bus is waiting, the depot gates jump over their phases when nothing comes.
/// Without it the condition is "somebody is asking": a gate stays green while buses keep
/// coming, a barrier stays closed while the train is still there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightStop {
    pub light: usize,
    pub time: f32,
    pub if_request: bool,
    pub jump_to: Option<f32>,
}

/// How far before its stop line a vehicle asks a light for green when the program gives no
/// `[approachdist]` (m): about four seconds at town speed.
pub const DEFAULT_APPROACH: f32 = 50.0;

/// From how far before the stop line (m) a rail vehicle asks a light whose program gives
/// no distance (unset or 0, as level crossings mostly do): about its braking distance from
/// 100 km/h, so the barriers are down when it comes.
pub const RAIL_ASK_DISTANCE: f32 = 600.0;

/// A vehicle waiting with its front within this of the stop line (m) asks its light, the
/// `[approachdist]` notwithstanding (with 0 a car half a metre short never asked).
pub const WAITING_AT_LINE: f32 = 3.0;

/// The `TrafficLightPhase` of an object no crossing light drives: a signal without a
/// `[varparent]`, one whose crossing has no program, any other scripted object. Omsi.exe
/// binds every placed object's `TrafficLightPhase` and `TrafficLightApproach` to a light
/// of its parent crossing ("RefreshAmpelParenting", 0x77d460) and, failing that, to a
/// shared dummy variable in .bss (0x8619a4), which reads 0: red, not "no light".
pub const UNLINKED_PHASE: i32 = 0;

/// Traffic light program of a crossing object instance (`TAmpelGroup`): its lights'
/// phases and one cycle clock that runs on game time. The clock is state, not a function
/// of the time of day, because the program may wait at a stop point or jump.
#[derive(Debug, Clone)]
pub struct TrafficLightController {
    /// Per light: phases (state, duration).
    pub lights: Vec<Vec<(i32, f32)>>,
    /// Cycle length (`[traffic_lights_group]`); 0 = the longest light's phases.
    pub cycle: f32,
    pub offset: f32,
    /// `[approachdist]` per light.
    pub approach: Vec<Option<f32>>,
    pub stops: Vec<LightStop>,
    /// Position in the cycle (s).
    pub time: f64,
    /// Per light: a vehicle (or a pedestrian) is asking for it this frame.
    pub request: Vec<bool>,
    /// The clock waits at a stop point.
    pub held: bool,
    /// Points already let past at this instant. Remember all of them so coincident
    /// inactive points cannot keep making one another eligible again.
    passed: Vec<usize>,
    /// A short backwards jump has replayed its stretch once. Do not take it again before
    /// the clock has moved past its source time.
    rewound: Option<usize>,
    started: bool,
}

impl TrafficLightController {
    pub fn new(lights: Vec<Vec<(i32, f32)>>, cycle: f32) -> TrafficLightController {
        let n = lights.len();
        TrafficLightController { lights, cycle, offset: 0.0, approach: vec![None; n], stops: Vec::new(), time: 0.0, request: vec![false; n], held: false, passed: Vec::new(), rewound: None, started: false }
    }

    /// From the `[traffic_light]` program of a crossing object: (per light: name, phases
    /// as (state, seconds), `[approachdist]`), the cycle, the stop and jump points.
    pub fn from_program(lights: Vec<(Vec<(i32, f32)>, Option<f32>)>, cycle: Option<f32>, stops: &[[f32; 3]], jumps: &[[f32; 4]]) -> TrafficLightController {
        let approach = lights.iter().map(|l| l.1).collect();
        let mut c = TrafficLightController::new(lights.into_iter().map(|l| l.0).collect(), cycle.unwrap_or(0.0));
        c.approach = approach;
        for s in stops {
            c.stops.push(LightStop { light: s[0].max(0.0) as usize, time: s[1], if_request: s[2] > 0.5, jump_to: None });
        }
        for j in jumps {
            c.stops.push(LightStop { light: j[0].max(0.0) as usize, time: j[1], if_request: j[2] > 0.5, jump_to: Some(j[3]) });
        }
        c
    }

    /// Length of the cycle in seconds.
    pub fn cycle_len(&self) -> f64 {
        if self.cycle > 0.0 {
            return self.cycle as f64;
        }
        self.lights.iter().map(|p| p.iter().map(|x| x.1).sum::<f32>()).fold(0.0f32, f32::max).max(1.0) as f64
    }

    /// Set the clock from the time of day (s) the first time the program runs: crossings
    /// with the same cycle length then run in step, as a coordinated street would.
    pub fn start(&mut self, day_time: f64) {
        if !self.started {
            self.time = (day_time + self.offset as f64).rem_euclid(self.cycle_len());
            self.started = true;
        }
    }

    /// Request distance of light `i` (m).
    pub fn approach_dist(&self, i: usize) -> f32 {
        self.approach.get(i).copied().flatten().unwrap_or(DEFAULT_APPROACH)
    }

    /// Whether light `i` is asked by a vehicle `gap` m before its stop line (below 0: over
    /// it) going `speed` m/s. The program's own distance holds for every vehicle; where it
    /// gives none (unset or 0) a rail vehicle asks from [`RAIL_ASK_DISTANCE`], a road one
    /// from the default when unset and only at the line when 0. Whoever waits at the line
    /// asks.
    pub fn is_asked_by(&self, i: usize, gap: f32, speed: f32, rail: bool) -> bool {
        if speed.abs() < 0.5 && gap <= WAITING_AT_LINE {
            return true;
        }
        let set = self.approach.get(i).copied().flatten();
        let distance = match (set.filter(|&d| d > 0.0), rail) {
            (Some(d), _) => d,
            (None, true) => RAIL_ASK_DISTANCE,
            (None, false) => set.unwrap_or(DEFAULT_APPROACH),
        };
        gap <= distance
    }

    /// Run the cycle clock on by `dt` seconds of game time, honouring the stop and jump
    /// points with this frame's requests (`request`, set by the caller before).
    pub fn advance(&mut self, dt: f32) {
        let cycle = self.cycle_len();
        let mut left = dt.max(0.0) as f64;
        self.held = false;
        let clear_rewind = |this: &mut Self, move_by: f64| {
            let Some(k) = this.rewound else { return };
            let source = this.stops[k].time as f64;
            if ((this.time - source).abs() < 1e-6 && move_by > 1e-6)
                || (this.time < source && this.time + move_by > source + 1e-6)
            {
                this.rewound = None;
            }
        };
        // Allow every coincident inactive point without losing this frame's time,
        // but still bound authored jump loops.
        for _ in 0..16 + self.stops.len() {
            let mut best: Option<(usize, f64)> = None;
            for (k, p) in self.stops.iter().enumerate() {
                if self.rewound == Some(k) {
                    continue;
                }
                let d = (p.time as f64 - self.time).rem_euclid(cycle);
                let d = if d > cycle - 1e-6 { 0.0 } else { d };
                if d < 1e-6 && self.passed.contains(&k) {
                    continue;
                }
                if d <= left && best.map(|b| d < b.1).unwrap_or(true) {
                    best = Some((k, d));
                }
            }
            let Some((k, d)) = best else {
                if left > 0.0 {
                    clear_rewind(self, left);
                    // (a sliver of a frame left over does not make the clock leave the
                    // points it stands on)
                    if left > 1e-6 {
                        self.passed.clear();
                    }
                }
                self.time = (self.time + left).rem_euclid(cycle);
                return;
            };
            if d > 1e-6 {
                self.passed.clear();
            }
            clear_rewind(self, d);
            self.time = (self.time + d).rem_euclid(cycle);
            left -= d;
            let p = self.stops[k];
            let asked = if p.if_request {
                self.stops.iter().any(|q| q.if_request && q.time == p.time && q.jump_to == p.jump_to && self.request.get(q.light).copied().unwrap_or(false))
            } else {
                self.request.get(p.light).copied().unwrap_or(false)
            };
            let active = if p.if_request { !asked } else { asked };
            // A stop does not hold the changeover to green: while some light shows
            // red-and-yellow the clock goes on. London's Brixton Road programs put a stop a
            // second into red-and-yellow, and a queue waiting there held the junction in the
            // changeover for good. Any other phase may hold: green, and also flashing
            // yellow, dark or all red (mod level crossings rest with dark road lights,
            // night programs without a jump).
            let holdable = p.jump_to.is_some() || !self.in_changeover();
            if !active || !holdable {
                self.passed.push(k);
                continue;
            }
            match p.jump_to {
                Some(to) => {
                    // A jump a couple of seconds back while somebody asks extends the
                    // current phase. It is not a loop: after replaying that small stretch,
                    // continue through it. One taken while nobody asks is the program's
                    // rest: the main road stays green round it until a request comes
                    // (Winsenburg's win-4: replayed once, it ran through yellow and red
                    // with nobody waiting and flickered between green and yellow, #1722).
                    // A jump past the cycle's end starts the next cycle at 0, it is not
                    // wrapped round: Städtedreieck's Bergerplatz jumps to 130 s of its
                    // 127 s cycle, and taken as 3 s it ran its night flashing in every
                    // daytime cycle.
                    let jump_to = f64::from(to);
                    let target = if jump_to < cycle { jump_to.rem_euclid(cycle) } else { 0.0 };
                    let no_progress = ((target - p.time as f64).rem_euclid(cycle)).abs() < 1e-6;
                    self.rewound = (!p.if_request && to > 1e-6 && to < p.time - 1e-6).then_some(k);
                    self.time = target;
                    // The clock lands on the point it jumped to: the points that lie exactly
                    // there are behind it, as the ones it was jumped over are. Taken again at
                    // the landing, win-4's "nobody asks at the bus light: back to 2 s" fired
                    // on arriving at 22 s by the other bus's jump and sent the clock round
                    // 2 -> 22 -> 2 for good: the second bus light never got its green (#3092).
                    self.passed.clear();
                    for (j, q) in self.stops.iter().enumerate() {
                        if ((q.time as f64 - target).rem_euclid(cycle) < 1e-6 || (target - q.time as f64).rem_euclid(cycle) < 1e-6) && j != k {
                            self.passed.push(j);
                        }
                    }
                    // A jump at the cycle boundary can target zero (for example TCSP17's
                    // `[traffic_light_jump] 4 84 1 0`). At cycle position zero that is a
                    // no-op: without remembering this event, it fires again at distance
                    // zero every frame and the whole controller stays frozen at zero.
                    if no_progress {
                        self.passed.push(k);
                    }
                    if left <= 0.0 {
                        return;
                    }
                }
                None => {
                    self.time = p.time as f64;
                    self.held = true;
                    self.passed.clear();
                    return;
                }
            }
        }
    }

    /// Some light shows red-and-yellow: the program is changing over to green.
    fn in_changeover(&self) -> bool {
        (0..self.lights.len()).any(|i| Self::aspect(self.state(i)) == Aspect::RedYellow && self.lights[i].iter().any(|&(s, _)| matches!(Self::aspect(s), Aspect::Green | Aspect::GreenYellow)))
    }

    /// State of light `i` at position `x` of the cycle (s).
    pub fn state_at(&self, i: usize, x: f64) -> i32 {
        let phases = match self.lights.get(i) {
            Some(p) if !p.is_empty() => p,
            _ => return 6,
        };
        let mut x = x.rem_euclid(self.cycle_len()) as f32;
        for (state, dur) in phases {
            if x < *dur {
                return *state;
            }
            x -= dur;
        }
        // after the last phase its state holds until the cycle starts again
        phases.last().map(|p| p.0).unwrap_or(0)
    }

    /// Current state of light `i` (the `TrafficLightPhase` value of its lamps).
    pub fn state(&self, i: usize) -> i32 {
        self.state_at(i, self.time)
    }

    /// Seconds until light `i` next shows another state, if the clock keeps running (None
    /// within a whole cycle).
    pub fn time_to_change(&self, i: usize) -> Option<f32> {
        let now = self.state(i);
        let cycle = self.cycle_len();
        let mut t = 0.25;
        while t <= cycle {
            if self.state_at(i, self.time + t) != now {
                return Some(t as f32);
            }
            t += 0.25;
        }
        None
    }

    /// Seconds until light `i` lets vehicles go (0 while it does), if the clock keeps
    /// running; None when it never does within a cycle.
    pub fn time_until_go(&self, i: usize) -> Option<f32> {
        if Self::allows_go(self.state(i)) {
            return Some(0.0);
        }
        let cycle = self.cycle_len();
        let mut t = 0.25;
        while t <= cycle {
            if Self::allows_go(self.state_at(i, self.time + t)) {
                return Some(t as f32);
            }
            t += 0.25;
        }
        None
    }

    /// Index of the phase light `i` is in (debugging).
    pub fn phase_index(&self, i: usize) -> i32 {
        let phases = match self.lights.get(i) {
            Some(p) if !p.is_empty() => p,
            _ => return -1,
        };
        let mut x = self.time.rem_euclid(self.cycle_len()) as f32;
        for (k, (_, dur)) in phases.iter().enumerate() {
            if x < *dur {
                return k as i32;
            }
            x -= dur;
        }
        phases.len() as i32 - 1
    }

    /// Seconds until light `i` leaves the state it shows now while the clock runs on
    /// (phases in a row with the same state count as one; a stop point may hold it
    /// longer). A pedestrian starts across only when the green lasts.
    pub fn remaining(&self, i: usize) -> f32 {
        let phases = match self.lights.get(i) {
            Some(p) if !p.is_empty() => p,
            _ => return f32::INFINITY,
        };
        let cycle = self.cycle_len() as f32;
        let x = self.time.rem_euclid(self.cycle_len()) as f32;
        // the schedule of one cycle: (state, start, end); a last phase of length 0 (or a
        // cycle longer than the phases) lasts until the cycle ends
        let mut spans: Vec<(i32, f32, f32)> = Vec::new();
        let mut start = 0.0;
        for (k, (state, dur)) in phases.iter().enumerate() {
            let end = if k + 1 == phases.len() { cycle.max(start + dur) } else { start + dur };
            spans.push((*state, start, end));
            start = end;
        }
        let Some(k) = spans.iter().position(|s| x < s.2).or(Some(spans.len() - 1)) else { return f32::INFINITY };
        let state = spans[k].0;
        if spans.iter().all(|s| s.0 == state) {
            return f32::INFINITY;
        }
        // run on through the following phases (wrapping round) while the state stays
        let mut left = spans[k].2 - x;
        let mut j = (k + 1) % spans.len();
        while spans[j].0 == state && j != k {
            left += spans[j].2 - spans[j].1;
            j = (j + 1) % spans.len();
        }
        left
    }

    /// OMSI light states as the stock lamp scripts read them: 0..2 red, 3..5 red and
    /// yellow, 6..8 green (8: the GDR's green with yellow), 9..11 yellow, 12 and above dark.
    /// (The code once read 8 as yellow and 9 as all-red: every stock program's yellow, 9,
    /// showed red, so the lights went from green straight to red.)
    pub fn aspect(state: i32) -> Aspect {
        match state {
            0..=2 => Aspect::Red,
            3..=5 => Aspect::RedYellow,
            6 | 7 => Aspect::Green,
            8 => Aspect::GreenYellow,
            9..=11 => Aspect::Yellow,
            _ => Aspect::Dark,
        }
    }

    /// May a vehicle drive over the stop line now (not counting yellow, which is the
    /// driver's decision)?
    pub fn allows_go(state: i32) -> bool {
        matches!(Self::aspect(state), Aspect::Green | Aspect::Dark)
    }

    /// Lamp variables (red, yellow, green) of a state, for lamp objects without a script
    /// (the rules of the stock `ampel1_ddr.osc`, which agree with `ampel1.osc` for the
    /// states the West Berlin programs use).
    pub fn lamps(state: i32) -> (bool, bool, bool) {
        (matches!(state, 0..=5), matches!(state, 3..=5 | 8..=11), matches!(state, 6..=8))
    }
}

/// What a traffic light shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aspect {
    Red,
    RedYellow,
    Green,
    GreenYellow,
    Yellow,
    Dark,
}
