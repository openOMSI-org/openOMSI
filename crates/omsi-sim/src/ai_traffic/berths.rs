//! The berths of the stops: one timetable bus at a time serves a stop where the route meets
//! it, the others queue for it in the order they came.
//!
//! A berth is a stop occurrence on a route - the stop's map object on the lane the route
//! meets it on (a route out and back along one street meets the same stop twice, once on
//! each side). Its numbers are kept apart, as they mean different things: where the origin
//! of a docked bus comes to rest (`bus_service::Stop::s`), how far across the bay lies
//! (`Stop::bay`), the stretch the docked body covers and that must be free before another
//! bus may pull in (the boarding region, `region`), and how far before the stop the bus
//! moves over into the bay (`bus_service`'s approach distance).
//!
//! The coordinator hands each berth to the bus that is there, then to the nearest bus
//! approaching on the lane (its distance refreshed on every ask, then the step it first asked,
//! then its id). The holder keeps the berth through boarding, the doors closing and pulling
//! out, until its rear has left it; a route change, its removal or the network changing
//! give it up too. Who has not got the berth waits `QUEUE_STANDOFF` before it. Standing
//! right behind the bus at the berth it serves the stop where it stands, as two buses at one
//! stop do in OMSI 2 (`SERVE_QUEUED_REACH`); further back it opens its doors only when the
//! berth is taken for long (a bus on its layover at a stop several lines share), and goes
//! round the holder afterwards (`SERVE_BEHIND_REACH`, `HELD_LONG`).

use super::*;
use std::collections::BTreeMap;

/// A stop occurrence on a route: the stop's map object and the lane the route meets it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BerthKey {
    pub stop: i64,
    pub lane: usize,
}

/// The boarding region reaches this far behind the rear of a docked bus and ahead of the
/// point its origin comes to rest at, plus the front (m): what stands there is in the way
/// of a bus pulling in.
pub const REGION_BEHIND: f32 = 1.0;
pub const REGION_AHEAD: f32 = 1.5;
/// A body that moves faster than this through the boarding region is passing it, not
/// standing in it (m/s).
pub const REGION_PASSING: f32 = 2.0;
/// A bus waiting for the berth stops its front this far before the rear of a bus docked
/// there (m): close up, as buses queue at a stop (10 m left a gap of a bus length, #2429).
pub const QUEUE_STANDOFF: f32 = 3.0;
/// A bus waiting for the berth with its origin this near the stop point (m) - right behind
/// the bus docked there, in the stop's reach - serves the stop where it stands at once, as
/// in OMSI 2 two buses at a stop take on people together (#2429). Further back it waits.
pub const SERVE_QUEUED_REACH: f32 = 25.0;
/// A waiting bus standing this near the stop (its origin to the stop point, m) serves it
/// where it stands when the berth is taken for long.
pub const SERVE_BEHIND_REACH: f32 = 40.0;
/// The berth is taken for long when its holder expects to stand there longer than this
/// (s, a layover), or when something that is not a bus of the queue has stood in its
/// region longer than `BLOCKED_LONG` (the player's bus, a broken-down car).
pub const HELD_LONG: f32 = 60.0;
pub const BLOCKED_LONG: f32 = 20.0;
/// The holder gives the berth up once its rear is this far past the stop point (m).
pub const CLEAR_PAST: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ask {
    /// Its body stands in the berth already.
    there: bool,
    /// The step it first asked in, and the step it last asked in.
    since: u64,
    seen: u64,
    /// How far from the stop it is (dm), refreshed on each ask: of two asking, the one
    /// nearer goes first (it is in front on the lane).
    dist: i32,
    id: u64,
}

#[derive(Debug, Clone, Default)]
struct Berth {
    holder: Option<u64>,
    asks: Vec<Ask>,
    /// Seconds the holder expects to stand there yet (as of its last step).
    holder_standing: f32,
    /// Since when (traffic time) something has stood in the region with nobody holding
    /// the berth.
    blocked_since: Option<f32>,
}

impl Berth {
    /// The asks in the order the berth is handed out: who is there, then who is closer,
    /// then who came first.
    fn first(&self) -> Option<u64> {
        self.asks.iter().min_by_key(|a| (!a.there, a.dist, a.since, a.id)).map(|a| a.id)
    }
}

/// Every berth asked for or held, by stop occurrence.
#[derive(Debug, Clone, Default)]
pub struct Berths {
    berths: BTreeMap<BerthKey, Berth>,
    step: u64,
    generation: u64,
    /// Berths handed out and given back since the start (diagnostics: in the long run
    /// every grant has its release).
    pub granted: u64,
    pub released: u64,
}

impl Berths {
    /// A new step of the traffic. Asks not renewed in the step before are dropped (the bus
    /// went elsewhere), and so is the hold of a car that is no longer on the road. A new
    /// network (`generation`) gives up every berth: returns true then, and the buses ask
    /// again.
    pub fn begin_step(&mut self, generation: u64, alive: impl Fn(u64) -> bool) -> bool {
        self.step += 1;
        if generation != self.generation {
            self.generation = generation;
            let held = self.berths.values().filter(|b| b.holder.is_some()).count() as u64;
            self.released += held;
            self.berths.clear();
            return true;
        }
        let step = self.step;
        let mut released = 0;
        for b in self.berths.values_mut() {
            b.asks.retain(|a| a.seen + 1 >= step && alive(a.id));
            if b.holder.is_some_and(|h| !alive(h)) {
                b.holder = None;
                released += 1;
            }
        }
        self.released += released;
        // (a berth nobody holds or asks for is forgotten, and with it since when something
        // stood in it: the next bus to come times that afresh)
        self.berths.retain(|_, b| b.holder.is_some() || !b.asks.is_empty());
        false
    }

    /// Bus `id` wants berth `key` (again this step); `there`: its body stands in it, `dist`:
    /// its origin is this far before the stop point (m).
    pub fn ask(&mut self, key: BerthKey, id: u64, there: bool, dist: f32) {
        let step = self.step;
        let d = (dist.max(0.0) * 10.0).clamp(0.0, 1e6) as i32;
        let b = self.berths.entry(key).or_default();
        match b.asks.iter_mut().find(|a| a.id == id) {
            Some(a) => {
                a.seen = step;
                a.there |= there;
                a.dist = d;
            }
            None => b.asks.push(Ask { there, since: step, seen: step, dist: d, id }),
        }
    }

    /// Does bus `id` hold berth `key` now? It is handed the berth when nobody holds it, it
    /// is the first of the asks and nothing stands in the region (`free`).
    pub fn grant(&mut self, key: BerthKey, id: u64, free: bool) -> bool {
        let Some(b) = self.berths.get_mut(&key) else {
            return false;
        };
        match b.holder {
            Some(h) => h == id,
            None if free && b.first() == Some(id) => {
                b.holder = Some(id);
                b.asks.retain(|a| a.id != id);
                b.blocked_since = None;
                self.granted += 1;
                true
            }
            None => false,
        }
    }

    /// Who holds berth `key`.
    pub fn holder(&self, key: BerthKey) -> Option<u64> {
        self.berths.get(&key).and_then(|b| b.holder)
    }

    /// The berths bus `id` holds.
    pub fn held_by(&self, id: u64) -> Vec<BerthKey> {
        self.berths.iter().filter(|(_, b)| b.holder == Some(id)).map(|(k, _)| *k).collect()
    }

    /// Whether bus `id` waits in the queue of berth `key`.
    pub fn queued(&self, key: BerthKey, id: u64) -> bool {
        self.berths.get(&key).is_some_and(|b| b.asks.iter().any(|a| a.id == id))
    }

    /// The holder of `key` expects to stand there `secs` more.
    pub fn note_standing(&mut self, key: BerthKey, id: u64, secs: f32) {
        if let Some(b) = self.berths.get_mut(&key).filter(|b| b.holder == Some(id)) {
            b.holder_standing = secs;
        }
    }

    /// Whether something that holds no berth stands in the region of `key` (as seen by a
    /// bus waiting for it) at time `now`.
    pub fn note_blocked(&mut self, key: BerthKey, blocked: bool, now: f32) {
        let b = self.berths.entry(key).or_default();
        if blocked && b.holder.is_none() {
            b.blocked_since.get_or_insert(now);
        } else {
            b.blocked_since = None;
        }
    }

    /// Is berth `key` taken for long (see `HELD_LONG`, `BLOCKED_LONG`)?
    pub fn held_long(&self, key: BerthKey, now: f32) -> bool {
        self.berths.get(&key).is_some_and(|b| match b.holder {
            Some(_) => b.holder_standing > HELD_LONG,
            None => b.blocked_since.is_some_and(|t| now - t > BLOCKED_LONG),
        })
    }

    /// Bus `id` no longer waits for berth `key` (it served the stop from behind).
    pub fn withdraw(&mut self, key: BerthKey, id: u64) {
        if let Some(b) = self.berths.get_mut(&key) {
            b.asks.retain(|a| a.id != id);
        }
    }

    /// Bus `id` gives berth `key` back. True when it held it.
    pub fn release(&mut self, key: BerthKey, id: u64) -> bool {
        let Some(b) = self.berths.get_mut(&key) else {
            return false;
        };
        b.asks.retain(|a| a.id != id);
        if b.holder == Some(id) {
            b.holder = None;
            b.holder_standing = 0.0;
            self.released += 1;
            return true;
        }
        false
    }

    /// Bus `id` leaves every berth and queue (a new route, taken off the road). Returns how
    /// many berths it held; a second call finds none.
    pub fn release_car(&mut self, id: u64) -> usize {
        let mut n = 0;
        for b in self.berths.values_mut() {
            b.asks.retain(|a| a.id != id);
            if b.holder == Some(id) {
                b.holder = None;
                b.holder_standing = 0.0;
                n += 1;
            }
        }
        self.released += n as u64;
        n
    }
}

/// The boarding region of a stop for a bus `rear` + `front` long: along the stop's lane from
/// behind where its rear stands docked to past where its front does (m).
pub fn region(stop_s: f32, front: f32, rear: f32) -> (f32, f32) {
    (stop_s - rear - REGION_BEHIND, stop_s + front + REGION_AHEAD)
}

/// What the traffic around a timetable bus says about its front stop's berth this step
/// (`TrafficSim::berth_scene`).
#[derive(Debug, Clone, Copy, Default)]
pub struct BerthScene {
    /// The stop is one with a berth (a road stop: trains and aircraft keep to their
    /// stations as before).
    pub applies: bool,
    /// Nothing but this bus stands in the boarding region.
    pub free: bool,
    /// This bus's body stands in the region.
    pub there: bool,
    /// Pulling out of the bay now would cut in front of something coming up fast close
    /// behind, or into something standing just ahead.
    pub pull_out_blocked: bool,
}

/// Something coming up behind closer than this, faster than `PULL_OUT_COMING`, keeps a bus
/// in its bay; so does something standing closer than `PULL_OUT_AHEAD` ahead (m, m/s).
pub const PULL_OUT_BEHIND: f32 = 25.0;
pub const PULL_OUT_COMING: f32 = 1.0;
pub const PULL_OUT_AHEAD: f32 = 8.0;

impl TrafficSim {
    /// The step's start for the berths: a new network gives every berth up, and the buses
    /// forget what they held.
    pub fn berths_begin_step(&mut self) {
        let alive: hashbrown::HashSet<u64> = self.cars.iter().map(|c| c.id).collect();
        if self.berths.begin_step(self.lanes_generation, |id| alive.contains(&id)) {
            for b in self.cars.iter_mut().filter_map(|c| c.bus.as_mut()) {
                b.forget_berth();
            }
        }
    }

    /// The berth of car `i`'s front stop as the tick's start has the traffic (`ts`).
    pub fn berth_scene(&self, i: usize, ts: &TickScene) -> BerthScene {
        let car = &self.cars[i];
        let st = &car.state;
        let Some(stop) = car.bus.as_ref().and_then(|b| b.stops.front()) else {
            return BerthScene::default();
        };
        let lane = st.route.get(stop.ri).copied().unwrap_or(st.lane);
        if self.net.lanes.get(lane).is_none_or(|l| l.kind != LaneKind::Street) {
            return BerthScene::default();
        }
        let (lo, hi) = region(stop.s, st.front, st.rear);
        let across = |lat: f32, half_width: f32| (lat - stop.bay).abs() < half_width + car.half_width + 0.2;
        let mut free = true;
        let mut there = false;
        for &(j, s, lat, _) in ts.by_lane.get(&lane).map(Vec::as_slice).unwrap_or(&[]) {
            let o = &self.cars[j];
            let (a, b) = (s - o.state.rear, s + o.state.front);
            if b < lo || a > hi {
                continue;
            }
            if j == i {
                there = true;
            } else if across(lat, o.half_width) && o.state.speed < REGION_PASSING {
                free = false;
            }
        }
        // the player's bus and the other players' vehicles stand in it as well
        let l = &self.net.lanes[lane];
        for p in ts.player.iter().chain(ts.others.iter().map(|o| &o.1)) {
            if p.4.abs() >= REGION_PASSING {
                continue;
            }
            if let Some((s, d)) = l.nearest_point(p.0) {
                if s + p.2 >= lo && s - p.2 <= hi && (d as f32) < p.3 + car.half_width + stop.bay.abs() + 0.2 {
                    free = false;
                }
            }
        }
        BerthScene { applies: true, free, there, pull_out_blocked: self.pull_out_blocked(i, ts) }
    }

    /// Would car `i`, standing in a bay, cut into the traffic if it pulled out now?
    fn pull_out_blocked(&self, i: usize, ts: &TickScene) -> bool {
        let car = &self.cars[i];
        let st = &car.state;
        let mut blocked = false;
        for &(j, s, lat, _) in ts.by_lane.get(&st.lane).map(Vec::as_slice).unwrap_or(&[]) {
            if j == i || (lat - st.lateral).abs() < 0.5 && lat.abs() > 1.0 {
                // (itself, and whoever stands in the bay with it)
                continue;
            }
            let o = &self.cars[j];
            let behind = st.s - st.rear - (s + o.state.front);
            let ahead = s - o.state.rear - (st.s + st.front);
            blocked |= (0.0..PULL_OUT_BEHIND).contains(&behind) && o.state.speed > PULL_OUT_COMING;
            blocked |= (0.0..PULL_OUT_AHEAD).contains(&ahead) && o.state.speed < 0.3 && lat.abs() < 1.0;
        }
        blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: BerthKey = BerthKey { stop: 7, lane: 3 };

    #[test]
    fn the_berth_goes_to_who_came_first_and_is_kept_until_given_back() {
        let mut b = Berths::default();
        b.begin_step(0, |_| true);
        b.ask(K, 9, false, 20.0);
        b.begin_step(0, |_| true);
        b.ask(K, 9, false, 20.0);
        b.ask(K, 4, false, 20.0);
        // 4 has the lower id but came a step later
        assert!(!b.grant(K, 4, true));
        assert!(b.grant(K, 9, true));
        assert!(!b.grant(K, 4, true));
        b.begin_step(0, |_| true);
        b.ask(K, 4, false, 20.0);
        assert!(b.grant(K, 9, false), "kept while it stands there");
        assert!(b.release(K, 9));
        assert!(!b.release(K, 9), "given back once");
        assert!(!b.grant(K, 4, false), "not while something stands in it");
        assert!(b.grant(K, 4, true));
        assert_eq!(b.granted, 2);
        assert_eq!(b.released, 1);
    }

    #[test]
    fn a_bus_already_in_the_berth_comes_first_and_stale_asks_lapse() {
        let mut b = Berths::default();
        b.begin_step(0, |_| true);
        b.ask(K, 1, false, 20.0);
        b.begin_step(0, |_| true);
        b.ask(K, 1, false, 20.0);
        b.ask(K, 2, true, 20.0);
        assert!(b.grant(K, 2, true));
        assert_eq!(b.release_car(2), 1);
        assert_eq!(b.release_car(2), 0);
        // bus 1 stops asking: two steps on it is out of the queue
        b.begin_step(0, |_| true);
        b.begin_step(0, |_| true);
        assert!(!b.queued(K, 1));
    }

    #[test]
    fn a_new_network_and_a_car_gone_give_the_berths_back() {
        let mut b = Berths::default();
        b.begin_step(0, |_| true);
        b.ask(K, 1, false, 20.0);
        assert!(b.grant(K, 1, true));
        assert!(b.begin_step(1, |_| true));
        assert_eq!(b.holder(K), None);
        b.ask(K, 1, false, 20.0);
        assert!(b.grant(K, 1, true));
        assert!(!b.begin_step(1, |id| id != 1));
        assert_eq!(b.holder(K), None);
        assert_eq!(b.granted, b.released);
    }

    #[test]
    fn a_berth_is_taken_for_long_by_a_layover_or_by_something_standing_there() {
        let mut b = Berths::default();
        b.begin_step(0, |_| true);
        b.ask(K, 1, false, 20.0);
        assert!(b.grant(K, 1, true));
        b.note_standing(K, 1, 30.0);
        assert!(!b.held_long(K, 0.0));
        b.note_standing(K, 1, 900.0);
        assert!(b.held_long(K, 0.0));
        b.release(K, 1);
        b.note_blocked(K, true, 100.0);
        assert!(!b.held_long(K, 110.0));
        assert!(b.held_long(K, 100.0 + BLOCKED_LONG + 1.0));
        b.note_blocked(K, false, 130.0);
        assert!(!b.held_long(K, 130.0));
    }

    #[test]
    fn closer_vehicle_gets_berth_ahead_of_earlier_asker_further_away() {
        let mut b = Berths::default();
        b.begin_step(0, |_| true);
        b.ask(K, 9, false, 30.0);
        b.begin_step(0, |_| true);
        b.ask(K, 9, false, 30.0);
        b.ask(K, 4, false, 5.0);
        assert!(b.grant(K, 4, true));
        assert!(!b.grant(K, 9, true));
    }

    #[test]
    fn ask_distance_updates_as_vehicle_approaches() {
        let mut b = Berths::default();
        b.begin_step(0, |_| true);
        b.ask(K, 9, false, 50.0);
        b.ask(K, 4, false, 20.0);
        assert_eq!(b.berths.get(&K).unwrap().first(), Some(4));
        b.begin_step(0, |_| true);
        b.ask(K, 9, false, 10.0);
        b.ask(K, 4, false, 20.0);
        assert_eq!(b.berths.get(&K).unwrap().first(), Some(9));
    }
}
