//! The light programs: requests, what a light means for a car, railway signals and
//! points, and the LAN host's light clocks on a client.

use super::*;

impl TrafficSim {
    pub fn red_ahead(&self, i: usize) -> Option<f32> {
        let st = &self.cars[i].state;
        let way = self.way_lanes(st, 200.0);
        for (k, &(_, d)) in way.iter().enumerate().skip(1) {
            if d > 150.0 {
                break;
            }
            let Some((c, li)) = self.light_at_entry(&way, k) else {
                continue;
            };
            let Some(ctl) = self.lights.get(c) else {
                continue;
            };
            if !TrafficLightController::allows_go(ctl.state(li)) {
                return Some(d - st.front);
            }
        }
        None
    }

    /// Seconds until the red light car `j` waits for may let it go (0 if none holds it).
    pub fn light_wait(&self, j: usize) -> f32 {
        let st = &self.cars[j].state;
        let way = self.way_lanes(st, 150.0);
        for (k, &(_, d)) in way.iter().enumerate().skip(1) {
            if d > 150.0 {
                break;
            }
            let Some((c, li)) = self.light_at_entry(&way, k) else {
                continue;
            };
            let Some(ctl) = self.lights.get(c) else {
                continue;
            };
            if !TrafficLightController::allows_go(ctl.state(li)) {
                return ctl.time_to_change(li).unwrap_or(0.0);
            }
        }
        0.0
    }

    /// The light that holds a car at the start of `way[k]`: that lane's light, unless the
    /// car has already gone through a light of the same crossing object on the way there
    /// (the lanes before it, back to where it came into that object). A car turning right
    /// went on green and then stopped as it came round the corner, at the light of the
    /// cross traffic on the path its turn joins - a stop line in mid-junction nobody sees.
    pub fn light_at_entry(&self, way: &[(usize, f32)], k: usize) -> Option<(usize, usize)> {
        entry_light(&self.net, way, k)
    }

    /// The light program's verdict for car `i`: where it has to stop (distance from its
    /// origin), or None. As Omsi.exe's path search (0x716970): a light that shows red,
    /// red-and-yellow or yellow (0x710ee0 marks its path 1, 3 or 4 from the phase byte
    /// 0x688a58 derives) holds a car whose front has not reached the light's path yet, at
    /// the path's start; once its front is on the path the light no longer holds it.
    /// There is no choice at yellow: the car brakes for it as for red, and one too close
    /// to stop goes on only because its front gets over the line first. (openOMSI had a
    /// comfortable stop at yellow with a random few going on, a memory that carried a car
    /// that went on yellow through the red, and a car that could not stop at 6.4 m/s² ran
    /// the red - AI cars ran red lights, #818.) The light of the lane the car is on does
    /// not hold it (the search's first segment, 0x716970 `local_1b1`).
    pub fn light_stop(&mut self, i: usize, way: &[(usize, f32)]) -> Option<f32> {
        let front = self.cars[i].state.front;
        let mut stop = None;
        // (the light it went over on yellow, for the red-run count of `stats`)
        let mut amber = self.cars[i].amber;
        for (k, &(_, d)) in way.iter().enumerate().skip(1) {
            if d > 150.0 {
                break;
            }
            let Some((c, li)) = self.light_at_entry(way, k) else {
                continue;
            };
            let Some(ctl) = self.lights.get(c) else {
                continue;
            };
            let aspect = TrafficLightController::aspect(ctl.state(li));
            if light_holds(aspect, d - front) {
                stop = Some(d);
                break;
            }
            if aspect == Aspect::Yellow {
                amber = Some((c, li));
            }
        }
        // the light the car went through on yellow is behind it
        if let Some(a) = amber {
            if !way
                .iter()
                .skip(1)
                .any(|&(l, _)| self.net.lanes[l].traffic_light == Some(a))
            {
                amber = None;
            }
        }
        self.cars[i].amber = amber;
        stop
    }

    /// The railway signals' aspects from where the trains are: a signal shows go (1, or 2
    /// with a `[speedlimit]`) while a train's route is about to enter the track its signal
    /// route covers and no other train is on it; otherwise it shows stop (and falls back to
    /// stop behind the train that passed it).
    pub fn signal_aspects(
        &mut self,
        routes: &[omsi_map::ailists::SignalRoute],
        player_rail: Option<(usize, bool)>,
    ) -> hashbrown::HashMap<i64, f32> {
        let mut out: hashbrown::HashMap<i64, f32> = hashbrown::HashMap::new();
        if routes.is_empty() {
            return out;
        }
        // (the signals of the map's blocks show whether their block is free)
        for (id, a) in self.block_aspects() {
            out.insert(id, a);
        }
        // per train: the map ids it stands on and those of its next lanes
        let mut trains: Vec<(i64, Vec<i64>)> = self
            .cars
            .iter()
            .filter(|c| {
                !c.state.route.is_empty()
                    && self
                        .net
                        .lanes
                        .get(c.state.lane)
                        .map(|l| l.kind == crate::traffic::LaneKind::Rail)
                        .unwrap_or(false)
            })
            .map(|c| {
                let here = self.net.lanes[c.state.lane].key.map(|k| k.id).unwrap_or(-1);
                let ahead = c
                    .state
                    .route
                    .iter()
                    .skip(c.state.route_index + 1)
                    .take(40)
                    .filter_map(|&l| self.net.lanes.get(l).and_then(|l| l.key).map(|k| k.id))
                    .collect();
                (here, ahead)
            })
            .collect();
        // the player's own train (driven on the rails): the lanes ahead of it the way it goes,
        // every branch at a fork (which it takes is not known yet) - its signals stayed at
        // stop, only an AI train ever cleared them
        if let Some((lane, along)) = player_rail.filter(|(l, _)| *l < self.net.lanes.len()) {
            let here = self.net.lanes[lane].key.map(|k| k.id).unwrap_or(-1);
            let mut ahead: Vec<i64> = Vec::new();
            let mut frontier = vec![lane];
            for _ in 0..6 {
                let mut next = Vec::new();
                for l in frontier {
                    let nb: Vec<usize> = if along {
                        self.net.lanes[l].next.clone()
                    } else {
                        self.net.prev.get(l).cloned().unwrap_or_default()
                    };
                    for n in nb {
                        if let Some(k) = self.net.lanes.get(n).and_then(|x| x.key) {
                            if !ahead.contains(&k.id) {
                                ahead.push(k.id);
                            }
                        }
                        next.push(n);
                    }
                }
                frontier = next;
                if frontier.len() > 32 {
                    break;
                }
            }
            trains.push((here, ahead));
        }
        if omsi_cfg::flags::OMSI_DEBUG_SIGNALS.is_set() {
            // where the player's train stands and which routes it is on or coming to
            if let Some((here, ahead)) = trains.last() {
                let on: Vec<i64> = routes
                    .iter()
                    .filter(|r| r.entries.iter().any(|e| e[0] == *here))
                    .map(|r| r.signal.0)
                    .collect();
                let to: Vec<i64> = routes
                    .iter()
                    .filter(|r| {
                        r.entries
                            .iter()
                            .any(|e| ahead.contains(&e[0]) && e[0] != *here)
                    })
                    .map(|r| r.signal.0)
                    .collect();
                thread_local!(static LAST: std::cell::RefCell<(i64, Vec<i64>, Vec<i64>)> = const { std::cell::RefCell::new((i64::MIN, Vec::new(), Vec::new())) });
                LAST.with(|l| {
                    if *l.borrow() != (*here, on.clone(), to.clone()) {
                        log::info!("signals: the train is on piece {here}, on the routes of signals {on:?}, coming to those of {to:?} ({} pieces ahead)", ahead.len());
                        *l.borrow_mut() = (*here, on, to);
                    }
                });
            }
        }
        for r in routes {
            let pieces: hashbrown::HashSet<i64> = r.entries.iter().map(|e| e[0]).collect();
            let occupied = trains.iter().any(|(here, _)| pieces.contains(here));
            let wanted = trains.iter().any(|(here, ahead)| {
                !pieces.contains(here) && ahead.iter().any(|id| pieces.contains(id))
            });
            let aspect = if wanted && !occupied {
                if r.speed_limit.is_some() { 2.0 } else { 1.0 }
            } else {
                0.0
            };
            let e = out.entry(r.signal.0).or_insert(0.0);
            *e = e.max(aspect);
        }
        out
    }

    /// The switches the trains need set: (map object, `[path]` index) of the next lanes of
    /// every train's timetable route. A train throws the points ahead of it to the
    /// `[switchdir]` of the path its route takes (see `World::set_switches`).
    pub fn switch_requests(&self) -> Vec<(i64, u16)> {
        let mut out = Vec::new();
        for c in &self.cars {
            let st = &c.state;
            if st.route.is_empty()
                || self
                    .net
                    .lanes
                    .get(st.lane)
                    .map(|l| l.kind != crate::traffic::LaneKind::Rail)
                    .unwrap_or(true)
            {
                continue;
            }
            for &l in st.route.iter().skip(st.route_index).take(5) {
                if let Some(k) = self.net.lanes.get(l).and_then(|l| l.key) {
                    out.push((k.id, k.path));
                }
            }
        }
        out
    }

    /// `OMSI_DEBUG_LIGHTS`: every change of the lights of the chosen programs, with the
    /// game time and the program's cycle position.
    pub fn log_lights(&mut self) {
        let Some(sel) = self.light_log.as_deref() else {
            return;
        };
        let near = sel.eq_ignore_ascii_case("near");
        let chosen: Vec<usize> = sel
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        let viewer = self.viewer.map(|v| v.pos);
        for (ci, c) in self.lights.iter().enumerate() {
            let pick = sel.eq_ignore_ascii_case("all")
                || chosen.contains(&ci)
                || (near && viewer.is_some());
            if !pick {
                continue;
            }
            if near {
                // the programs within 150 m of the camera (by the lanes they control)
                let vp = viewer.unwrap();
                let close = self.net.lanes.iter().any(|l| {
                    l.traffic_light.map(|t| t.0) == Some(ci) && (l.start() - vp).length() < 150.0
                });
                if !close {
                    continue;
                }
            }
            for li in 0..c.lights.len() {
                let s = c.state(li);
                let prev = self.light_prev[ci][li];
                if s != prev {
                    let h = self.day_time.rem_euclid(86400.0);
                    log::info!(
                        "light {ci}.{li}: {:?} ({s}) at {:02}:{:02}:{:05.2} (cycle {:.2} of {:.0} s{}{})",
                        TrafficLightController::aspect(s),
                        (h / 3600.0) as u32,
                        ((h / 60.0) % 60.0) as u32,
                        h % 60.0,
                        c.time,
                        c.cycle_len(),
                        if c.held { ", held" } else { "" },
                        if c.request.get(li).copied().unwrap_or(false) {
                            ", requested"
                        } else {
                            ""
                        }
                    );
                    self.light_prev[ci][li] = s;
                }
            }
        }
    }

    /// State of light `li` of controller `c` now and the seconds it keeps showing it (the
    /// pedestrians only start across on a green that lasts).
    pub fn light_state(&self, c: usize, li: usize) -> Option<(i32, f32)> {
        let ctl = self.lights.get(c)?;
        Some((ctl.state(li), ctl.remaining(li)))
    }

    /// Seconds until light `li` of controller `c` lets traffic go (0 while it does).
    pub fn light_until_go(&self, c: usize, li: usize) -> Option<f32> {
        self.lights.get(c)?.time_until_go(li)
    }

    /// The lanes a train asks the lights of, out to `reach`: its way as planned and, past the
    /// plan (some ninety metres or ten lanes), its timetable route on, or for a train
    /// without one every branch of the track on.
    pub fn train_way(&self, st: &AiState, reach: f32) -> Vec<(usize, f32)> {
        let mut way = self.way_lanes(st, reach);
        let Some(&(last, d)) = way.last() else {
            return way;
        };
        if d + self.net.lanes[last].length() >= reach {
            return way;
        }
        let from = st.route_index.min(st.route.len());
        if let Some(k) = st.route[from..].iter().position(|&l| l == last) {
            let mut d = d + self.net.lanes[last].length();
            for &l in &st.route[from + k + 1..] {
                if d > reach {
                    break;
                }
                way.push((l, d));
                d += self.net.lanes[l].length();
            }
        } else if st.route.is_empty() {
            way.extend(self.track_after(last, d, reach));
        }
        way
    }

    /// The lanes after `lane` within `reach` metres, on every branch of a fork (which one a
    /// train takes is the points' business), each with the distance to its start from
    /// where `lane` starts at `start` (m). Asking the branch it will not take only turns
    /// that light for nobody.
    pub fn track_after(&self, lane: usize, start: f32, reach: f32) -> Vec<(usize, f32)> {
        let mut out: Vec<(usize, f32)> = Vec::new();
        let mut open = vec![(lane, start + self.net.lanes[lane].length())];
        while let Some((l, to_end)) = open.pop() {
            if to_end > reach || out.len() > 64 {
                continue;
            }
            for &n in &self.net.lanes[l].next {
                if n == lane || out.iter().any(|o| o.0 == n) {
                    continue;
                }
                out.push((n, to_end));
                open.push((n, to_end + self.net.lanes[n].length()));
            }
        }
        out
    }

    /// The street lanes a vehicle standing at `pos` facing `heading` is on and will drive
    /// onto within `reach` metres, each with the distance from the vehicle to its start
    /// (0 for the lane it is on): the way straight on and the gentle turns (within 60° of
    /// the lane before), not every branch of a junction.
    pub fn lanes_ahead_of(&self, pos: DVec3, heading: f64, reach: f32) -> Vec<(usize, f32)> {
        let Some((lane, s, _)) = self
            .net
            .lane_along(pos, heading, LaneKind::Street, 4.0, 45.0)
        else {
            return Vec::new();
        };
        let mut out = vec![(lane, 0.0f32)];
        let mut open = vec![(lane, self.net.lanes[lane].length() - s)];
        while let Some((l, to_end)) = open.pop() {
            if to_end > reach || out.len() > 256 {
                continue;
            }
            let end_heading = self.net.lanes[l].headings.last().copied().unwrap_or(0.0);
            for &n in &self.net.lanes[l].next {
                let nl = &self.net.lanes[n];
                if nl.kind != LaneKind::Street || out.iter().any(|o| o.0 == n) {
                    continue;
                }
                let start = nl.headings.first().copied().unwrap_or(end_heading);
                let turn = ((start - end_heading) as f64 + 540.0).rem_euclid(360.0) - 180.0;
                if turn.abs() > 60.0 {
                    continue;
                }
                out.push((n, to_end));
                open.push((n, to_end + nl.length()));
            }
        }
        out
    }

    /// The `TrafficLightPhase` and `TrafficLightApproach` values of light `li` of
    /// controller `c` (for scenery scripts).
    pub fn light_vars(&self, c: usize, li: usize) -> (f32, f32) {
        self.lights
            .get(c)
            .map(|ctl| {
                (
                    ctl.state(li) as f32,
                    ctl.request.get(li).copied().unwrap_or(false) as i32 as f32,
                )
            })
            .unwrap_or((crate::traffic::UNLINKED_PHASE as f32, 0.0))
    }

    /// The light programs of the crossings within `radius` of `near` (host): (crossing
    /// object, position in the cycle, clock held at a stop point), the nearest first: a
    /// datagram carries only so many, and those left out must be the far ones.
    pub fn light_states(&self, near: DVec3, radius: f64) -> Vec<(i64, f64, bool)> {
        // each controller in range, with how far off its nearest lane is
        let mut ctls: HashMap<usize, f64> = HashMap::new();
        for l in &self.net.lanes {
            let (Some(light), Some(p)) = (l.traffic_light, l.points.first()) else {
                continue;
            };
            let d = (*p - near).truncate().length();
            if d < radius {
                let nearest = ctls.entry(light.0).or_insert(d);
                *nearest = nearest.min(d);
            }
        }
        let mut states: Vec<(f64, i64, f64, bool)> = self
            .controller_of_object
            .iter()
            .filter_map(|(obj, c)| {
                let d = *ctls.get(c)?;
                let ctl = self.lights.get(*c)?;
                Some((d, *obj, ctl.time, ctl.held))
            })
            .collect();
        states.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        states
            .into_iter()
            .map(|(_, obj, time, held)| (obj, time, held))
            .collect()
    }

    /// Where the host's light program of crossing `object` stands (client).
    pub fn set_light_state(&mut self, object: i64, time: f64, held: bool) {
        if let Some(ctl) = self
            .controller_of_object
            .get(&object)
            .and_then(|c| self.lights.get_mut(*c))
        {
            set_mirror_light_clock(ctl, time, held);
        }
    }
}

pub fn mirror_light_tick(ctl: &mut TrafficLightController, dt: f32, day_time: f64) {
    ctl.request.fill(false);
    ctl.start(day_time);
    if !ctl.held {
        ctl.time = (ctl.time + dt.max(0.0) as f64).rem_euclid(ctl.cycle_len());
    }
}

pub fn set_mirror_light_clock(ctl: &mut TrafficLightController, time: f64, held: bool) {
    // A crossing may receive its first snapshot before its first tick. Mark its clock
    // started now, so the time-of-day seed cannot replace the host's position later.
    ctl.start(time - ctl.offset as f64);
    ctl.time = time.rem_euclid(ctl.cycle_len());
    ctl.held = held;
}

pub fn reset_light_runtime(ctl: &mut TrafficLightController, day_time: f64) {
    // Stop/jump bookkeeping belongs to the clock's previous owner. Keep the program
    // and current position, but discard its old requests and visited points.
    ctl.start(day_time);
    let mut fresh = TrafficLightController::new(ctl.lights.clone(), ctl.cycle);
    fresh.offset = ctl.offset;
    fresh.approach = ctl.approach.clone();
    fresh.stops = ctl.stops.clone();
    fresh.start(ctl.time - ctl.offset as f64);
    *ctl = fresh;
}

/// Whether a light showing `aspect` holds a car whose front is `gap` m before its line
/// (negative: over it). Omsi.exe (0x716970) stops an AI vehicle for red, red-and-yellow
/// and yellow alike while the line is still ahead of its front (`0 < local_194`), and for
/// nothing else.
pub fn light_holds(aspect: Aspect, gap: f32) -> bool {
    matches!(aspect, Aspect::Red | Aspect::RedYellow | Aspect::Yellow) && gap > 0.0
}

/// The light that holds a car at the start of `way[k]`.
///
/// A declared entry signal remains authoritative without scenery-object identity. Inside a
/// crossing object a light the car has already gone through on its way there does not hold
/// it again, nor does the light of another approach on a path its turn joins (a car turning
/// right went on green and then stopped as it came round the corner, at the light of the
/// cross traffic - a stop line in mid-junction nobody sees). Another light of the object
/// on the car's own movement does hold: a second stop line of a turn, the one after a
/// signalled footpath, which only the object's own paths lead to (the BRT lanes of Berlin
/// ran through their second red).
pub fn entry_light(net: &Network, way: &[(usize, f32)], k: usize) -> Option<(usize, usize)> {
    let l = way[k].0;
    let light = net.lanes[l].traffic_light?;
    let object = |x: usize| {
        let lane = &net.lanes[x];
        lane.key
            .filter(|_| lane.source == 2)
            .map(|key| (key.tile, key.id))
    };
    let Some(here) = object(l) else {
        return Some(light);
    };
    // (only the paths of this object lead onto it: it is no other approach's entry)
    let inner = net
        .prev
        .get(l)
        .is_some_and(|p| !p.is_empty() && p.iter().all(|&x| object(x) == Some(here)));
    for &(p, _) in way[..k].iter().rev() {
        if object(p) != Some(here) {
            break;
        }
        if let Some(passed) = net.lanes[p].traffic_light {
            return (passed != light && inner).then_some(light);
        }
    }
    Some(light)
}
