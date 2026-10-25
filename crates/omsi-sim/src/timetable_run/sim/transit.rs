//! The trips made from a map's `transport` routes, laid on the track.
//!
//! A route gives stations, not lanes. Once the whole map's network is known (the navigator's
//! `World::navigation_map`) each trip is routed here: the lanes of the right kind (rail for trams,
//! metro and railway vehicles, street for buses and trolleybuses) nearest each station are the
//! candidates, and the shortest way over the network through them, station by station, is the
//! trip's track. The track enters the timetable as a `.ttr` would, and the times come from the
//! length of each stretch, the speed the segment allows and the standing time at the stops.

use super::*;
use crate::traffic::{Lane, LaneKey, LaneKind, Network};
use glam::DVec3;
use omsi_cfg::transport::TransportMode;
use omsi_timetable::transit::TransitTrip;
use omsi_timetable::{Track, TrackEntry, TripProfile};

/// A trip's route over the network.
#[derive(Debug, Clone, PartialEq)]
pub struct Routed {
    /// The lanes of the way, in order.
    pub lanes: Vec<usize>,
    /// The way's lanes as the timetable names them: key and length (m). Lanes the map's files do
    /// not name (connectors) are left out; the router joins the gaps again.
    pub steps: Vec<(LaneKey, f32)>,
    /// The length of the way between each pair of stops (m).
    pub hop_m: Vec<f32>,
}

/// How far from its lane a station may stand (m).
const REACH: f64 = 25.0;
/// Candidate lanes kept per station: the two tracks of a double line, and a spare.
const CANDIDATES: usize = 3;

/// The kind of lane a mode runs on.
pub fn lane_kind(mode: TransportMode) -> LaneKind {
    if mode.is_rail() { LaneKind::Rail } else { LaneKind::Street }
}

fn distance_to(lane: &Lane, p: DVec3) -> Option<(f32, f64)> {
    let mut best: Option<(f32, f64)> = None;
    for k in 0..lane.points.len().saturating_sub(1) {
        let (a, b) = (lane.points[k], lane.points[k + 1]);
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
        let d = (a + ab * t - p).truncate().length();
        if best.is_none_or(|b| d < b.1) {
            best = Some((lane.dist[k] + (lane.dist[k + 1] - lane.dist[k]) * t as f32, d));
        }
    }
    best
}

/// The lanes of `kind` within `reach` of `p`, nearest first: (lane, distance along it, distance to it).
pub fn candidates(net: &Network, p: DVec3, kind: LaneKind, reach: f64) -> Vec<(usize, f32, f64)> {
    let mut v: Vec<(usize, f32, f64)> = net
        .lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == kind && l.points.len() > 1)
        .filter_map(|(i, l)| distance_to(l, p).filter(|d| d.1 <= reach).map(|(s, d)| (i, s, d)))
        .collect();
    v.sort_by(|a, b| a.2.total_cmp(&b.2));
    v.truncate(CANDIDATES);
    v
}

/// The way between two stations given as a lane and a place on it: the lanes and the length.
fn hop(net: &Network, a: (usize, f32), b: (usize, f32), cache: &mut hashbrown::HashMap<(usize, usize), Option<Vec<usize>>>) -> Option<(Vec<usize>, f32)> {
    if a.0 == b.0 {
        return (b.1 >= a.1).then(|| (vec![a.0], b.1 - a.1));
    }
    let path = cache.entry((a.0, b.0)).or_insert_with(|| net.shortest_path(a.0, b.0)).clone()?;
    let middle: f32 = path[1..path.len() - 1].iter().map(|&l| net.lanes[l].length()).sum();
    Some((path, (net.lanes[a.0].length() - a.1) + middle + b.1))
}

/// Route a trip through its stations' places. `Err` says which station has no track near it, or
/// between which two stations there is no way.
pub fn route(net: &Network, mode: TransportMode, names: &[String], places: &[DVec3]) -> Result<Routed, String> {
    let kind = lane_kind(mode);
    if places.len() < 2 {
        return Err("fewer than two stations".into());
    }
    let cands: Vec<Vec<(usize, f32, f64)>> = places.iter().map(|&p| candidates(net, p, kind, REACH)).collect();
    for (i, c) in cands.iter().enumerate() {
        if c.is_empty() {
            return Err(format!("station {} has no {} within {REACH} m", names[i], if kind == LaneKind::Rail { "track" } else { "street" }));
        }
    }
    // Viterbi over the candidates: the cost to be at candidate j of station i
    let mut cache = hashbrown::HashMap::new();
    let mut cost: Vec<Vec<f32>> = vec![cands[0].iter().map(|c| c.2 as f32).collect()];
    let mut from: Vec<Vec<usize>> = vec![vec![0; cands[0].len()]];
    for i in 1..cands.len() {
        let mut row = vec![f32::INFINITY; cands[i].len()];
        let mut back = vec![0usize; cands[i].len()];
        for (j, cj) in cands[i].iter().enumerate() {
            for (k, ck) in cands[i - 1].iter().enumerate() {
                if cost[i - 1][k].is_infinite() {
                    continue;
                }
                if let Some((_, len)) = hop(net, (ck.0, ck.1), (cj.0, cj.1), &mut cache) {
                    let c = cost[i - 1][k] + len + cj.2 as f32;
                    if c < row[j] {
                        row[j] = c;
                        back[j] = k;
                    }
                }
            }
        }
        if row.iter().all(|c| c.is_infinite()) {
            return Err(format!("no way over the track from {} to {}", names[i - 1], names[i]));
        }
        cost.push(row);
        from.push(back);
    }
    let mut pick = vec![0usize; cands.len()];
    pick[cands.len() - 1] = (0..cands[cands.len() - 1].len()).min_by(|&a, &b| cost[cands.len() - 1][a].total_cmp(&cost[cands.len() - 1][b])).unwrap_or(0);
    for i in (1..cands.len()).rev() {
        pick[i - 1] = from[i][pick[i]];
    }
    let mut lanes: Vec<usize> = Vec::new();
    let mut hop_m = Vec::new();
    for i in 1..cands.len() {
        let (a, b) = (cands[i - 1][pick[i - 1]], cands[i][pick[i]]);
        let (path, len) = hop(net, (a.0, a.1), (b.0, b.1), &mut cache).ok_or("no way")?;
        hop_m.push(len);
        for l in path {
            if lanes.last() != Some(&l) {
                lanes.push(l);
            }
        }
    }
    let steps = lanes.iter().filter_map(|&l| net.lanes[l].key.map(|k| (k, net.lanes[l].length()))).collect();
    Ok(Routed { lanes, steps, hop_m })
}

/// The speed a mode cruises at where its segments say nothing (m/s) and how hard it accelerates
/// and brakes (m/s²).
fn kinematics(mode: TransportMode) -> (f32, f32) {
    match mode {
        TransportMode::Tram => (13.9, 1.0),
        TransportMode::Metro => (19.4, 1.0),
        TransportMode::Railway => (27.8, 0.7),
        TransportMode::Bus | TransportMode::Trolleybus => (11.1, 1.0),
    }
}

/// The time to cover `d` m from a stop to the next: cruise at `v` (or less, where the stretch is
/// too short to reach it), accelerating and braking at `a`.
pub fn hop_time(d: f32, v: f32, a: f32) -> f32 {
    let d = d.max(1.0);
    let v = v.min((a * d).sqrt()).max(1.0);
    d / v + v / a
}

/// The profile of a routed trip: the times it reaches and leaves each stop.
pub fn profile(trip: &TransitTrip, hop_m: &[f32]) -> TripProfile {
    let (cruise, accel) = kinematics(trip.mode);
    let mut arr = Vec::new();
    let mut dep = Vec::new();
    let mut t = 0.0f32;
    // a run from the depot's gate comes first: the vehicle reaches its first stop after it
    let lead = trip.lead_in.is_some() && hop_m.len() == trip.stops.len();
    let (lead_m, hop_m) = if lead { (hop_m[0], &hop_m[1..]) } else { (0.0, hop_m) };
    if lead {
        // (out of a depot yard at a yard's pace)
        t += hop_time(lead_m, cruise.min(8.0), accel);
        arr.push((0, t / 60.0));
    }
    for (i, st) in trip.stops.iter().enumerate() {
        if i > 0 {
            let v = trip.stops[i - 1].speed.unwrap_or(cruise);
            t += hop_time(hop_m.get(i - 1).copied().unwrap_or(500.0), v, accel);
            arr.push((i as i32, t / 60.0));
        }
        if i + 1 < trip.stops.len() {
            // (the trip leaves its first stop at its departure: the standing there is the previous
            // trip's layover; one that comes from the gate stands there as at any stop)
            if i > 0 || lead {
                t += st.dwell_s;
            }
            dep.push((i as i32, t / 60.0));
        }
    }
    TripProfile { name: "transit".into(), factor: t / 60.0, man_arr_time: arr, man_dep_time: dep, other_stopping: Vec::new() }
}

impl ScheduleSim {
    /// The trips made from transport routes whose track is not found yet.
    pub fn transit_wanted(&self) -> Vec<TransitTrip> {
        self.data.transit_trips.iter().filter(|t| self.transit_unrouted.contains(&t.trip.to_ascii_lowercase())).cloned().collect()
    }

    /// Whether any trip waits for its track.
    pub fn transit_pending(&self) -> bool {
        !self.transit_unrouted.is_empty()
    }

    /// Lay the routed trips on their tracks: a track for each, the times from its length. A trip
    /// that could not be routed is given up (its departures are dropped, as one without a route).
    pub fn install_transit(&mut self, done: Vec<(String, Result<Routed, String>)>) {
        for (name, result) in done {
            self.transit_unrouted.remove(&name.to_ascii_lowercase());
            let Some(ti) = self.data.trips.iter().position(|t| t.name.eq_ignore_ascii_case(&name)) else { continue };
            let Some(meta) = self.data.transit_trips.iter().find(|t| t.trip.eq_ignore_ascii_case(&name)).cloned() else { continue };
            match result {
                Err(why) => log::warn!("transport route {name}: {why}"),
                Ok(r) => {
                    let entries: Vec<TrackEntry> = r
                        .steps
                        .iter()
                        .filter_map(|(k, len)| {
                            let tile = self.tile_coords.iter().position(|t| *t == k.tile)?;
                            Some(TrackEntry { values: vec![k.id as f64, k.path as f64, tile as f64, 0.0, *len as f64] })
                        })
                        .collect();
                    let p = profile(&meta, &r.hop_m);
                    log::info!("transport route {name}: {} lanes, {:.1} km, {:.1} min", r.lanes.len(), r.hop_m.iter().sum::<f32>() / 1000.0, p.factor);
                    self.data.tracks.retain(|t| t.path.file_stem().is_none_or(|s| !s.to_string_lossy().eq_ignore_ascii_case(&name)));
                    self.data.tracks.push(Track { path: std::path::PathBuf::from(format!("{name}.ttr")), entries });
                    self.data.trips[ti].profiles = vec![p.clone()];
                    let stations = trip_stations(&self.data.trips[ti]);
                    self.times[ti] = vec![TripTimes::new(&stations, Some(&p), &|_, _| None)];
                }
            }
        }
    }

    /// The places of a trip's stops: the station's own, else the map's object (`places` holds
    /// every object of the map).
    pub fn transit_places(trip: &TransitTrip, places: &hashbrown::HashMap<i64, DVec3>) -> Vec<Option<DVec3>> {
        trip.stops.iter().map(|s| s.pos.map(|p| DVec3::new(p[0], p[1], p[2])).or_else(|| places.get(&s.object).copied())).collect()
    }
}

/// Find the lanes of the map's signalling blocks: from where each begins to where it ends,
/// over the rail lanes. A block that cannot be found is named in the `Err`.
pub fn route_blocks(net: &Network, signals: &omsi_transit::Signals) -> Vec<(String, Result<crate::ai_traffic::blocks::BlockDef, String>)> {
    signals
        .blocks
        .iter()
        .filter_map(|b| Some((b, b.from?, b.to?)))
        .map(|(b, from, to)| {
            let pts = [DVec3::new(from[0], from[1], from[2]), DVec3::new(to[0], to[1], to[2])];
            let result = route(net, TransportMode::Railway, &[format!("{} begin", b.id), format!("{} end", b.id)], &pts).map(|r| crate::ai_traffic::blocks::BlockDef {
                id: b.id.clone(),
                lanes: r.lanes.iter().filter_map(|&l| net.lanes[l].key.map(|k| (k, net.lanes[l].reversed))).collect(),
                signal: signals.signal_of(&b.id).and_then(|s| s.object),
            });
            (b.id.clone(), result)
        })
        .collect()
}

/// Route every wanted trip (what the worker of the game does once the map's network is read).
pub fn route_all(net: &Network, places: &hashbrown::HashMap<i64, DVec3>, wanted: &[TransitTrip]) -> Vec<(String, Result<Routed, String>)> {
    wanted
        .iter()
        .map(|t| {
            let at = ScheduleSim::transit_places(t, places);
            let result = match at.iter().position(|p| p.is_none()) {
                Some(i) => Err(format!("station {} has no place (give `pos` in stations.cfg, or the stop object is not on the map)", t.stops[i].name)),
                None => {
                    let mut names: Vec<String> = t.stops.iter().map(|s| s.name.clone()).collect();
                    let mut pts: Vec<DVec3> = at.into_iter().flatten().collect();
                    let plain = route(net, t.mode, &names, &pts);
                    match t.lead_in {
                        // the depot's gate in front of the first stop; a gate with no track by it
                        // (or none leading to the stop) is left out, the trip begins at the stop
                        Some(g) => {
                            names.insert(0, "the depot gate".into());
                            pts.insert(0, DVec3::new(g[0], g[1], g[2]));
                            route(net, t.mode, &names, &pts).or_else(|why| {
                                log::warn!("transport route {}: {why}; it begins at its first stop", t.trip);
                                plain
                            })
                        }
                        None => plain,
                    }
                }
            };
            (t.trip.clone(), result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_timetable::transit::TransitStop;

    fn lane(kind: LaneKind, from: (f64, f64), to: (f64, f64), key: i64) -> Lane {
        let mut l = crate::traffic::LaneBuilder::polyline(vec![DVec3::new(from.0, from.1, 0.0), DVec3::new(to.0, to.1, 0.0)], kind, 3.0);
        l.key = Some(LaneKey { tile: (0, 0), id: key, path: 0 });
        l
    }

    /// A line of three lanes, 0 to 300 m, with a second track beside it going back.
    fn net() -> Network {
        let mut n = Network::default();
        n.lanes.push(lane(LaneKind::Rail, (0.0, 0.0), (100.0, 0.0), 1));
        n.lanes.push(lane(LaneKind::Rail, (100.0, 0.0), (200.0, 0.0), 2));
        n.lanes.push(lane(LaneKind::Rail, (200.0, 0.0), (300.0, 0.0), 3));
        n.lanes.push(lane(LaneKind::Rail, (300.0, 5.0), (200.0, 5.0), 4));
        n.lanes.push(lane(LaneKind::Rail, (200.0, 5.0), (0.0, 5.0), 5));
        n.lanes[0].next = vec![1];
        n.lanes[1].next = vec![2];
        n.lanes[3].next = vec![4];
        n
    }

    #[test]
    fn a_trip_is_routed_over_the_lanes_between_its_stations() {
        let r = route(&net(), TransportMode::Tram, &["A".into(), "B".into(), "C".into()], &[DVec3::new(20.0, 1.0, 0.0), DVec3::new(150.0, 1.0, 0.0), DVec3::new(280.0, 1.0, 0.0)]).unwrap();
        assert_eq!(r.lanes, vec![0, 1, 2]);
        assert_eq!(r.steps.iter().map(|s| s.0.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!((r.hop_m[0] - 130.0).abs() < 1.0 && (r.hop_m[1] - 130.0).abs() < 1.0, "{:?}", r.hop_m);
    }

    #[test]
    fn the_way_back_takes_the_other_track() {
        // the stations stand between the two tracks; going back the router must use the track that runs back
        let r = route(&net(), TransportMode::Tram, &["C".into(), "A".into()], &[DVec3::new(280.0, 4.0, 0.0), DVec3::new(20.0, 4.0, 0.0)]).unwrap();
        assert_eq!(r.lanes, vec![3, 4], "{:?}", r.lanes);
    }

    #[test]
    fn mistakes_are_named() {
        let n = net();
        let far = route(&n, TransportMode::Tram, &["A".into(), "Nowhere".into()], &[DVec3::new(20.0, 1.0, 0.0), DVec3::new(150.0, 400.0, 0.0)]);
        assert!(far.unwrap_err().contains("Nowhere"));
        // against the direction of the only track there is no way
        let mut one = net();
        one.lanes.truncate(3);
        let no_way = route(&one, TransportMode::Tram, &["C".into(), "A".into()], &[DVec3::new(280.0, -1.0, 0.0), DVec3::new(20.0, -1.0, 0.0)]);
        assert!(no_way.unwrap_err().contains("no way"));
        // a bus needs a street
        assert!(route(&n, TransportMode::Bus, &["A".into(), "B".into()], &[DVec3::new(20.0, 1.0, 0.0), DVec3::new(150.0, 1.0, 0.0)]).is_err());
    }

    #[test]
    fn times_follow_the_length_the_speed_and_the_standing() {
        let trip = TransitTrip {
            trip: "T".into(),
            mode: TransportMode::Metro,
            stops: vec![
                TransitStop { object: 1, name: "A".into(), dwell_s: 20.0, ..Default::default() },
                TransitStop { object: 2, name: "B".into(), dwell_s: 30.0, speed: Some(10.0), ..Default::default() },
                TransitStop { object: 3, name: "C".into(), dwell_s: 20.0, ..Default::default() },
            ],
            lead_in: None,
        };
        let p = profile(&trip, &[1500.0, 1000.0]);
        // A to B at the metro's 19.4 m/s cruise: 1500/19.4 + 19.4/1 = 96.7 s; B to C at 10 m/s: 100 + 10 = 110 s
        let (arr_b, dep_b, arr_c) = (p.man_arr_time[0].1 * 60.0, p.man_dep_time[1].1 * 60.0, p.man_arr_time[1].1 * 60.0);
        assert!((arr_b - 96.7).abs() < 1.5, "{arr_b}");
        assert!((dep_b - (arr_b + 30.0)).abs() < 0.1);
        assert!((arr_c - (dep_b + 110.0)).abs() < 1.5, "{arr_c}");
        // a hop too short to reach cruise speed is slower than cruising
        assert!(hop_time(200.0, 19.4, 1.0) > 200.0 / 19.4 + 5.0);
    }

    struct TestWorld {
        path: std::path::PathBuf,
        ai: omsi_map::AiLists,
        positions: crate::world_data::ObjectPlaces,
    }

    impl TimetableWorld for TestWorld {
        fn root(&self) -> &Path { &self.path }
        fn map_dir(&self) -> &Path { &self.path }
        fn chrono_dirs(&self) -> Vec<std::path::PathBuf> { Vec::new() }
        fn ailists(&self) -> &omsi_map::AiLists { &self.ai }
        fn raw_tiles(&self) -> Vec<(i32, i32)> { vec![(0, 0)] }
        fn has_tile(&self, _: (i32, i32)) -> bool { true }
        fn date(&self) -> i32 { 20221010 }
        fn object_positions(&self) -> &crate::world_data::ObjectPlaces { &self.positions }
        fn stop_side(&self, _: i64) -> f32 { 0.0 }
    }

    /// A map folder with a `transport` folder: its services are in the timetable at once, wait for
    /// their track, and run by it when it is installed.
    #[test]
    fn transport_routes_become_departures_that_wait_for_their_track() {
        let dir = std::env::temp_dir().join(format!("omsi_transit_sim_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("TTData")).unwrap();
        std::fs::create_dir_all(dir.join("transport")).unwrap();
        std::fs::write(dir.join("TTData/Busstops.cfg"), "[busstop]\nAlpha\n0\n101\n0\n0\n0\n[busstop]\nBeta\n0\n102\n0\n0\n0\n").unwrap();
        std::fs::write(dir.join("transport/stations.cfg"), "[station]\nid A\nname Alpha\nobject 101\nmodes tram\n[station]\nid B\nname Beta\nobject 102\nmodes tram\n").unwrap();
        std::fs::write(dir.join("transport/routes.cfg"), "[route]\nid T1\nmode tram\n[segment]\nid s\nfrom A\nto B\ndwell 20 s\n[service]\nid all\nsegments s\nheadway 10 min\nfrom 06:00\nto 06:30\nturn_back reverse\n").unwrap();
        let world = TestWorld { path: dir.clone(), ai: Default::default(), positions: Default::default() };
        let mut sim = ScheduleSim::new(&dir, &world, &crate::SimClock::default());
        assert!(sim.transit_pending());
        assert_eq!(sim.transit_wanted().len(), 2, "outward and back");
        assert_eq!(sim.data.lines[0].tours.len(), 8, "four departures each way");
        // routed: a track of two lanes 600 m long, and the times from it
        let key = |id| LaneKey { tile: (0, 0), id, path: 0 };
        let r = Routed { lanes: vec![0, 1], steps: vec![(key(1), 300.0), (key(2), 300.0)], hop_m: vec![600.0] };
        sim.install_transit(vec![("T1_all_out".into(), Ok(r)), ("T1_all_back".into(), Err("no way".into()))]);
        assert!(!sim.transit_pending());
        let track = sim.data.tracks.iter().find(|t| t.path.file_stem().is_some_and(|s| s == "T1_all_out")).expect("the track");
        assert_eq!(track.entries.len(), 2);
        assert_eq!(track.entries[0].values[..3], [1.0, 0.0, 0.0]);
        let ti = sim.data.trips.iter().position(|t| t.name == "T1_all_out").unwrap();
        let t = &sim.times[ti][0];
        // 600 m at a tram's 13.9 m/s cruise: leaving A at once, 600/13.9+13.9 = 57 s
        assert!((t.stations[1].0 - 57.1).abs() < 2.0, "{:?}", t.stations);
        assert!(t.duration > 50.0 && t.duration < 70.0);
        // (the trip that could not be routed is given up, not held for ever)
        assert!(sim.transit_wanted().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_block_is_the_lanes_between_its_ends() {
        let signals = {
            let dir = std::env::temp_dir().join(format!("omsi_blocks_route_{}", std::process::id()));
            let t = dir.join("transport");
            std::fs::create_dir_all(&t).unwrap();
            std::fs::write(t.join("signals.cfg"), "[signal]\nid S\nmodes railway\nobject 900\nblock B\n[block]\nid B\nmodes railway\nfrom 120 0 0\nto 280 0 0\n[block]\nid Far\nmodes railway\nfrom 120 0 0\nto 280 900 0\n").unwrap();
            let mut issues = Vec::new();
            let d = omsi_transit::TransitData::load(&dir, &mut issues);
            let _ = std::fs::remove_dir_all(&dir);
            d.signals
        };
        let found = route_blocks(&net(), &signals);
        let (id, ok) = &found[0];
        assert_eq!(id, "B");
        let def = ok.as_ref().unwrap();
        assert_eq!(def.lanes.iter().map(|l| l.0.id).collect::<Vec<_>>(), vec![2, 3], "from the middle lane to the last");
        assert_eq!(def.signal, Some(900));
        assert!(found[1].1.is_err(), "the far end has no track near it");
    }

    #[test]
    fn a_run_from_the_gate_comes_before_the_first_stop() {
        let mut trip = TransitTrip {
            trip: "T".into(),
            mode: TransportMode::Tram,
            stops: vec![
                TransitStop { object: 1, name: "A".into(), dwell_s: 20.0, ..Default::default() },
                TransitStop { object: 2, name: "B".into(), dwell_s: 20.0, ..Default::default() },
            ],
            lead_in: Some([0.0, 0.0, 0.0]),
        };
        // gate to A 300 m, A to B 600 m
        let p = profile(&trip, &[300.0, 600.0]);
        let arr_a = p.man_arr_time.iter().find(|a| a.0 == 0).map(|a| a.1 * 60.0).expect("arrives at the first stop after the run");
        assert!(arr_a > 30.0, "{arr_a}: it comes in from elsewhere (not 'begins at its stop')");
        let dep_a = p.man_dep_time.iter().find(|a| a.0 == 0).unwrap().1 * 60.0;
        assert!((dep_a - (arr_a + 20.0)).abs() < 0.2, "it stands at A as at any stop");
        let arr_b = p.man_arr_time.iter().find(|a| a.0 == 1).unwrap().1 * 60.0;
        assert!((arr_b - (dep_a + hop_time(600.0, 13.9, 1.0))).abs() < 0.5);
        // a gate that could not be routed: the hops are the stops' own, no lead
        trip.lead_in = Some([0.0; 3]);
        let q = profile(&trip, &[600.0]);
        assert!(q.man_arr_time.iter().all(|a| a.0 != 0), "no arrival at stop 0: the trip begins there");
    }
}
