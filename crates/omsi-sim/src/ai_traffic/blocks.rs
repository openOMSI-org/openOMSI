//! Block signalling and the points' board for the AI trains.
//!
//! **Blocks.** A block (`signals.cfg`) is a stretch of track that holds one train at a time. Its
//! lanes are found once on the whole map's network (`BlockDef`, by lane key so they carry over
//! to the live network as its tiles load). A train that would enter a block another train is
//! in (on the lane or its twin of the other direction, or the tail of it not yet out) stops
//! before it, as at a red light, and goes on when the block is free. The signal object that
//! guards a block shows stop while it is occupied and go while it is free.
//!
//! **Points.** The `SwitchBoard` decides which positions the trains' requests may put the
//! points in. A switch the map configures (`switches.cfg`) is held while a train stands on it
//! (`interlocked`, the default for `auto` too), is not thrown by AI trains when `manual`, is
//! locked while the signal it names shows go (`locked_by`), and goes back to its default
//! position when no train has asked for it for a while.

use super::*;
use crate::traffic::LaneKey;

/// One block as the map's network found it.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDef {
    pub id: String,
    /// The block's lanes in order: key and whether it is the backwards lane of a two-way path.
    pub lanes: Vec<(LaneKey, bool)>,
    /// The scenery signal object that shows the block's state.
    pub signal: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct Resolved {
    /// The live lanes of the block, with the twins of the other direction.
    lanes: Vec<usize>,
    /// The live lane a train enters the block by.
    entry: Option<usize>,
}

/// The blocks of the map and where they are on the live network.
#[derive(Debug, Clone, Default)]
pub struct BlockSys {
    pub defs: Vec<BlockDef>,
    resolved: Vec<Resolved>,
    generation: Option<u64>,
}

impl TrafficSim {
    /// Give the traffic the map's blocks (found on the whole network by a worker).
    pub fn set_blocks(&mut self, defs: Vec<BlockDef>) {
        self.blocks = BlockSys { resolved: vec![Resolved::default(); defs.len()], defs, generation: None };
    }

    /// Find the live lanes of the blocks again when the network has grown.
    fn resolve_blocks(&mut self) {
        if self.blocks.generation == Some(self.lanes_generation) {
            return;
        }
        self.blocks.generation = Some(self.lanes_generation);
        for (d, r) in self.blocks.defs.iter().zip(self.blocks.resolved.iter_mut()) {
            let mut lanes = Vec::new();
            let mut entry = None;
            for (k, (key, reversed)) in d.lanes.iter().enumerate() {
                let Some(l) = self.net.find(*key, Some(*reversed)) else { continue };
                if k == 0 || entry.is_none() {
                    entry = entry.or(Some(l));
                }
                // (a single track carries trains both ways: the twins hold the block too)
                for &t in self.net.by_key.get(key).into_iter().flatten() {
                    if !lanes.contains(&t) {
                        lanes.push(t);
                    }
                }
            }
            *r = Resolved { lanes, entry };
        }
    }

    /// The lanes a train occupies: where it is, and the lane before while its tail is still
    /// on it.
    fn train_lanes(&self, c: &model::AiCar) -> Vec<usize> {
        let st = &c.state;
        let mut v = vec![st.lane];
        if st.s < st.rear {
            if let Some(prev) = self.net.prev.get(st.lane) {
                v.extend(prev.iter().copied());
            }
        }
        v
    }

    /// Whether a train other than car `except` is in block `b`.
    fn block_busy(&self, b: usize, except: Option<usize>) -> bool {
        let lanes = &self.blocks.resolved[b].lanes;
        if lanes.is_empty() {
            return false;
        }
        let cars = self.cars.iter().enumerate().any(|(j, c)| Some(j) != except && !c.gone && c.is_train() && self.train_lanes(c).iter().any(|l| lanes.contains(l)));
        let player = self.player_rail.is_some_and(|p| p.train && lanes.contains(&p.lane));
        cars || player
    }

    /// Where train `i` has to stop (distance from its origin) before a block another train is
    /// in, or `None`. Only trains are held, only before the block's entry, and not once the
    /// train's nose is over it.
    pub fn block_stop(&mut self, i: usize) -> Option<f32> {
        if self.blocks.defs.is_empty() || !self.cars[i].is_train() {
            return None;
        }
        self.resolve_blocks();
        let st = &self.cars[i].state;
        let reach = (st.speed * st.speed / (2.0 * st.decel.max(0.3)) + 150.0).clamp(200.0, 1500.0);
        let way = self.train_way(st, reach);
        let mine = self.train_lanes(&self.cars[i]);
        let mut stop: Option<f32> = None;
        for (b, r) in self.blocks.resolved.iter().enumerate() {
            let Some(entry) = r.entry else { continue };
            // (already in the block: it is its own)
            if mine.iter().any(|l| r.lanes.contains(l)) {
                continue;
            }
            let Some(&(_, d)) = way.iter().skip(1).find(|(l, _)| *l == entry) else { continue };
            if d - st.front < -0.5 || !self.block_busy(b, Some(i)) {
                continue;
            }
            stop = Some(stop.map_or(d, |s: f32| s.min(d)));
        }
        stop
    }

    /// What the block signals show: go (1) for a free block, stop (0) for one in use, by the
    /// signal object's map id.
    pub fn block_aspects(&mut self) -> hashbrown::HashMap<i64, f32> {
        let mut out = hashbrown::HashMap::new();
        if self.blocks.defs.is_empty() {
            return out;
        }
        self.resolve_blocks();
        for b in 0..self.blocks.defs.len() {
            if let Some(id) = self.blocks.defs[b].signal {
                let a = if self.block_busy(b, None) { 0.0 } else { 1.0 };
                let e = out.entry(id).or_insert(a);
                *e = f32::min(*e, a);
            }
        }
        out
    }

    /// The map objects (rail pieces) a train stands on or has its tail on.
    pub fn rail_occupied_pieces(&self) -> hashbrown::HashSet<i64> {
        let mut out = hashbrown::HashSet::new();
        let mut add = |l: usize| {
            if let Some(k) = self.net.lanes.get(l).and_then(|x| x.key) {
                out.insert(k.id);
            }
        };
        for c in self.cars.iter().filter(|c| !c.gone && self.net.lanes.get(c.state.lane).is_some_and(|l| l.kind == LaneKind::Rail)) {
            self.train_lanes(c).into_iter().for_each(&mut add);
        }
        if let Some(p) = self.player_rail {
            add(p.lane);
            if let Some(prev) = self.net.prev.get(p.lane) {
                if p.s < 30.0 {
                    prev.iter().copied().for_each(&mut add);
                }
            }
        }
        out
    }
}

/// How a switch is operated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwitchDrive {
    /// Thrown for the trains that come, held while one is on it.
    #[default]
    Auto,
    /// Thrown by hand only: AI trains do not throw it.
    Manual,
    /// As `Auto`, and locked while the signal it names shows go.
    Interlocked,
}

/// A switch the map configures.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchRule {
    pub object: i64,
    /// The `[path]` it rests in.
    pub default_path: Option<u16>,
    pub drive: SwitchDrive,
    /// The signal object that locks it while it shows go.
    pub locked_by: Option<i64>,
}

/// Seconds after the last request before a switch goes back to its default.
pub const RETURN_AFTER: f32 = 15.0;

#[derive(Debug, Clone, Default)]
struct SwitchState {
    last: Option<u16>,
    idle: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SwitchBoard {
    rules: Vec<SwitchRule>,
    state: Vec<SwitchState>,
    pub configured: bool,
}

impl SwitchBoard {
    pub fn configure(&mut self, rules: Vec<SwitchRule>) {
        self.state = vec![SwitchState::default(); rules.len()];
        self.rules = rules;
        self.configured = true;
    }

    /// The positions to put the points in this frame: the trains' `requests` as the rules allow
    /// them, and the defaults of switches nobody has asked for lately. `occupied` are the rail
    /// pieces a train is on, `aspects` what the signals show.
    pub fn decide(&mut self, dt: f32, requests: &[(i64, u16)], occupied: &hashbrown::HashSet<i64>, aspects: &hashbrown::HashMap<i64, f32>) -> Vec<(i64, u16)> {
        let mut out: Vec<(i64, u16)> = requests.iter().copied().filter(|(o, _)| !self.rules.iter().any(|r| r.object == *o)).collect();
        for (rule, st) in self.rules.iter().zip(self.state.iter_mut()) {
            let busy = occupied.contains(&rule.object);
            let locked = rule.locked_by.is_some_and(|s| aspects.get(&s).copied().unwrap_or(0.0) > 0.5);
            let asked: Vec<u16> = requests.iter().filter(|(o, _)| *o == rule.object).map(|r| r.1).collect();
            let hold = busy || locked;
            let want = match rule.drive {
                SwitchDrive::Manual => None,
                _ => asked.last().copied(),
            };
            match want {
                Some(p) if !hold || st.last == Some(p) => {
                    st.idle = 0.0;
                    st.last = Some(p);
                    out.push((rule.object, p));
                }
                Some(_) => {
                    // (a request while a train is on the points, or the signal is clear, waits:
                    // the points stay where they are)
                    st.idle = 0.0;
                }
                None => {
                    st.idle += dt;
                    let fresh = st.last.is_none();
                    if (fresh || st.idle >= RETURN_AFTER) && !hold {
                        if let Some(d) = rule.default_path {
                            if st.last != Some(d) {
                                out.push((rule.object, d));
                                st.last = Some(d);
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::scenario_tests::{traffic, Fixture};
    use super::*;
    use crate::traffic::LaneBuilder;
    use glam::DVec3;

    fn rail(start: f64, key: i64) -> crate::traffic::Lane {
        let mut l = LaneBuilder::arc(DVec3::new(0.0, start, 0.0), 0.0, 200.0, 0.0, 0.0, LaneKind::Rail, 1.5);
        l.key = Some(LaneKey { tile: (0, 0), id: key, path: 0 });
        l
    }

    /// Three 200 m rail lanes in a line: 0 -> 1 -> 2.
    fn track() -> Network {
        let mut net = Network::default();
        for k in 0..3 {
            net.lanes.push(rail(k as f64 * 200.0, k as i64 + 1));
        }
        net.lanes[0].next = vec![1];
        net.lanes[1].next = vec![2];
        net.by_key.clear();
        for (i, l) in net.lanes.iter().enumerate() {
            net.by_key.entry(l.key.unwrap()).or_default().push(i);
        }
        net.prev = vec![vec![], vec![0], vec![1]];
        net
    }

    fn train_fixture() -> Fixture {
        let f = Fixture::new();
        std::fs::write(f.dir.join("car.bus"), "[type]\n2\n[model]\nmodel.cfg\n").unwrap();
        let ty = Arc::new(VehicleType::load_ai(&f.dir, &f.dir.join("car.bus")).unwrap());
        Fixture { dir: f.dir.clone(), ty }
    }

    fn add_train(t: &mut TrafficSim, f: &Fixture, lane: usize, s: f32, speed: f32) -> usize {
        let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        let id = t.place_car(vehicle, LaneKind::Rail, lane, s, f.ty.clone(), 1, None, None, Some(speed), None);
        let i = t.cars.iter().position(|c| c.id == id).unwrap();
        // (a timetable route over all three lanes)
        t.cars[i].state.route = vec![0, 1, 2];
        t.cars[i].state.route_index = lane;
        i
    }

    fn key(id: i64) -> LaneKey {
        LaneKey { tile: (0, 0), id, path: 0 }
    }

    #[test]
    fn a_train_stops_before_a_block_another_train_is_in() {
        let f = train_fixture();
        let mut t = traffic(&f, track());
        // block "B" is the middle lane; a train stands in it, another comes up behind
        t.set_blocks(vec![BlockDef { id: "B".into(), lanes: vec![(key(2), false)], signal: Some(900) }]);
        let _ahead = add_train(&mut t, &f, 1, 100.0, 0.0);
        let behind = add_train(&mut t, &f, 0, 50.0, 10.0);
        if !t.cars[behind].is_train() {
            panic!("the fixture is no train");
        }
        let stop = t.block_stop(behind).expect("held before the occupied block");
        assert!((stop - 150.0).abs() < 2.0, "the block's entry is 150 m ahead: {stop}");
        assert_eq!(t.block_aspects().get(&900), Some(&0.0), "its signal shows stop");
    }

    #[test]
    fn a_free_block_lets_the_train_in() {
        let f = train_fixture();
        let mut t = traffic(&f, track());
        t.set_blocks(vec![BlockDef { id: "B".into(), lanes: vec![(key(2), false)], signal: Some(900) }]);
        let only = add_train(&mut t, &f, 0, 50.0, 10.0);
        assert_eq!(t.block_stop(only), None);
        assert_eq!(t.block_aspects().get(&900), Some(&1.0));
    }

    #[test]
    fn a_train_in_the_block_is_not_held_by_it() {
        let f = train_fixture();
        let mut t = traffic(&f, track());
        t.set_blocks(vec![BlockDef { id: "B".into(), lanes: vec![(key(2), false), (key(3), false)], signal: None }]);
        let inside = add_train(&mut t, &f, 1, 50.0, 5.0);
        assert_eq!(t.block_stop(inside), None);
    }

    #[test]
    fn the_tail_of_a_train_keeps_the_block_busy() {
        let f = train_fixture();
        let mut t = traffic(&f, track());
        t.set_blocks(vec![BlockDef { id: "A".into(), lanes: vec![(key(1), false)], signal: Some(7) }]);
        // a long train whose nose is 5 m into lane 1 and whose tail is still on lane 0
        let long = add_train(&mut t, &f, 1, 5.0, 3.0);
        t.cars[long].state.rear = 40.0;
        assert_eq!(t.block_aspects().get(&7), Some(&0.0));
        t.cars[long].state.s = 60.0;
        assert_eq!(t.block_aspects().get(&7), Some(&1.0));
    }

    fn rule(object: i64, default: Option<u16>, drive: SwitchDrive, locked_by: Option<i64>) -> SwitchRule {
        SwitchRule { object, default_path: default, drive, locked_by }
    }

    #[test]
    fn points_are_thrown_for_a_train_and_go_back_to_rest() {
        let mut b = SwitchBoard::default();
        b.configure(vec![rule(5, Some(0), SwitchDrive::Auto, None)]);
        let none = hashbrown::HashSet::new();
        let noasp = hashbrown::HashMap::new();
        // the first frame puts them in their rest position
        assert_eq!(b.decide(0.1, &[], &none, &noasp), vec![(5, 0)]);
        // a train asks for path 1
        assert_eq!(b.decide(0.1, &[(5, 1)], &none, &noasp), vec![(5, 1)]);
        // nobody asks: they wait, then return
        assert!(b.decide(5.0, &[], &none, &noasp).is_empty());
        assert!(b.decide(5.0, &[], &none, &noasp).is_empty());
        assert_eq!(b.decide(6.0, &[], &none, &noasp), vec![(5, 0)]);
        // switches the map does not configure pass through
        assert_eq!(b.decide(0.1, &[(9, 1)], &none, &noasp), vec![(9, 1)]);
    }

    #[test]
    fn points_are_held_under_a_train_and_by_a_clear_signal() {
        let mut b = SwitchBoard::default();
        b.configure(vec![rule(5, Some(0), SwitchDrive::Interlocked, Some(77))]);
        let none = hashbrown::HashSet::new();
        let mut on: hashbrown::HashSet<i64> = hashbrown::HashSet::new();
        on.insert(5);
        let mut clear = hashbrown::HashMap::new();
        clear.insert(77, 1.0);
        let stop = hashbrown::HashMap::new();
        b.decide(0.1, &[], &none, &stop);
        // a train is on the points: a request for the other position waits
        assert!(b.decide(0.1, &[(5, 1)], &on, &stop).is_empty());
        // the signal is clear: locked as well
        assert!(b.decide(0.1, &[(5, 1)], &none, &clear).is_empty());
        // free and the signal at stop: thrown
        assert_eq!(b.decide(0.1, &[(5, 1)], &none, &stop), vec![(5, 1)]);
        // and held in the position it is in, a request for the same one is let through
        assert_eq!(b.decide(0.1, &[(5, 1)], &on, &stop), vec![(5, 1)]);
    }

    #[test]
    fn manual_points_are_not_thrown_by_the_trains() {
        let mut b = SwitchBoard::default();
        b.configure(vec![rule(5, Some(0), SwitchDrive::Manual, None)]);
        let none = hashbrown::HashSet::new();
        let noasp = hashbrown::HashMap::new();
        assert_eq!(b.decide(0.1, &[], &none, &noasp), vec![(5, 0)], "they rest in their default");
        assert!(b.decide(0.1, &[(5, 1)], &none, &noasp).is_empty(), "no AI request throws them");
    }
}
