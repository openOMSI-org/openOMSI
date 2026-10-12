//! The timetable's trip times and the stations a trip calls at.

use super::*;

impl TripTimes {
    pub fn new(
        stations: &[i64],
        profile: Option<&omsi_timetable::TripProfile>,
        link_length: &dyn Fn(i64, i64) -> Option<f64>,
    ) -> TripTimes {
        let n = stations.len();
        let duration = profile
            .map(|p| p.factor as f64 * 60.0)
            .filter(|d| *d > 0.0)
            .unwrap_or(600.0);
        let mut arr: Vec<Option<f64>> = vec![None; n];
        let mut dep: Vec<Option<f64>> = vec![None; n];
        let mut stops = vec![true; n];
        let mut kinds = vec![0u8; n];
        let mut holds = vec![false; n];
        if let Some(p) = profile {
            let at = |i: i32| usize::try_from(i).ok().filter(|i| *i < n);
            for (i, m) in &p.man_arr_time {
                if let Some(i) = at(*i) {
                    arr[i] = Some(*m as f64 * 60.0);
                    holds[i] = true;
                }
            }
            for (i, m) in &p.man_dep_time {
                if let Some(i) = at(*i) {
                    dep[i] = Some(*m as f64 * 60.0);
                    holds[i] = true;
                }
            }
            for (i, v) in &p.other_stopping {
                if let Some(i) = at(*i) {
                    kinds[i] = (*v).clamp(0, 255) as u8;
                    if *v == 2 {
                        stops[i] = false;
                    }
                }
            }
        }
        // the trip leaves its first station at its departure and reaches the last one after
        // the profile's duration, unless the profile times them itself
        if n > 0 && arr[0].is_none() && dep[0].is_none() {
            dep[0] = Some(0.0);
        }
        if n > 1 && arr[n - 1].is_none() && dep[n - 1].is_none() {
            arr[n - 1] = Some(duration);
        }
        // the distance to every station along the links (a missing link counts 500 m)
        let mut along = vec![0.0f64; n];
        for i in 1..n {
            along[i] = along[i - 1]
                + link_length(stations[i - 1], stations[i])
                .unwrap_or(500.0)
                .max(1.0);
        }
        let timed: Vec<usize> = (0..n)
            .filter(|&i| arr[i].is_some() || dep[i].is_some())
            .collect();
        let mut out = Vec::with_capacity(n);
        let mut last = 0.0f64;
        for i in 0..n {
            let (a, d) = match (arr[i], dep[i]) {
                (Some(a), Some(d)) => (a, d.max(a)),
                (Some(a), None) => (a, a),
                (None, Some(d)) => (d, d),
                (None, None) => {
                    let p = timed.iter().rev().find(|&&k| k < i).copied();
                    let q = timed.iter().find(|&&k| k > i).copied();
                    let t = match (p, q) {
                        (Some(p), Some(q)) => {
                            let t0 = dep[p].or(arr[p]).unwrap_or(0.0);
                            let t1 = arr[q].or(dep[q]).unwrap_or(t0);
                            let span = along[q] - along[p];
                            if span > 0.0 {
                                t0 + (t1 - t0) * (along[i] - along[p]) / span
                            } else {
                                t0
                            }
                        }
                        (Some(p), None) => dep[p].or(arr[p]).unwrap_or(0.0),
                        (None, Some(q)) => arr[q].or(dep[q]).unwrap_or(0.0),
                        (None, None) => 0.0,
                    };
                    (t, t)
                }
            };
            // never back in time
            let a = a.max(last);
            let d = d.max(a);
            last = d;
            out.push((a, d));
        }
        // a trip with a route of stations lasts until it reaches the last one
        let duration = if n > 1 { out[n - 1].0 } else { duration };
        TripTimes {
            stations: out,
            stops,
            kinds,
            holds,
            duration: duration.max(1.0),
        }
    }
}

/// The stations a trip calls at: its `[station_typ2]` objects, or the objects of the older
/// `[station]` records the trains, the ferry and the U-Bahn of Spandau still use (their first
/// line is the object id).
/// The station targets of [`Schedule::stop_targets`] from the trips' stops and termini.
/// A stop is never a target of itself or of another stop of the same name (the platforms
/// of one station, the first and last stop of a circular line): somebody waiting there who
/// drew it got in, found the bus at their stop and got straight off again, over and over,
/// every one of them adding another pedestrian (#795).
pub fn station_targets(trips: impl Iterator<Item = (Vec<i64>, String)>, name_of: impl Fn(i64) -> String) -> HashMap<i64, Vec<(String, HashSet<String>)>> {
    let mut named: HashMap<i64, Vec<(String, HashSet<String>)>> = HashMap::new();
    for (stations, terminus) in trips {
        for (k, from) in stations.iter().enumerate() {
            let here = name_of(*from);
            let targets = named.entry(*from).or_default();
            for to in &stations[k + 1..] {
                let to = name_of(*to);
                if to == here {
                    continue;
                }
                match targets.iter_mut().find(|t| t.0 == to) {
                    Some(t) => {
                        t.1.insert(terminus.clone());
                    }
                    None => targets.push((to, HashSet::from_iter([terminus.clone()]))),
                }
            }
        }
    }
    named
}

pub fn trip_stations(trip: &omsi_timetable::Trip) -> Vec<i64> {
    if !trip.stations.is_empty() {
        return trip.stations.clone();
    }
    trip.stations_legacy
        .iter()
        .filter_map(|s| s.first().and_then(|id| id.trim().parse::<i64>().ok()))
        .collect()
}

/// The tile each station of `trip` is placed in (its index in global.cfg's `[map]` list), as
/// the timetable says: a type-1 `[station]` record's fourth line, else the stop's
/// `Busstops.cfg` entry; None where it says nothing. A map joined from several repeats its
/// ids (Berlin 186: half its objects share their id with one of another tile), and by the id
/// alone a stop was found on another tile - the navigator drew it where no road was.
pub fn trip_station_tiles(trip: &omsi_timetable::Trip, bus_stops: &[omsi_timetable::BusStopEntry]) -> Vec<Option<usize>> {
    if !trip.stations.is_empty() {
        return trip.stations.iter().map(|id| bus_stops.iter().find(|b| b.object_id == *id).and_then(|b| usize::try_from(b.group).ok())).collect();
    }
    trip.stations_legacy
        .iter()
        .filter(|s| s.first().and_then(|id| id.trim().parse::<i64>().ok()).is_some())
        .map(|s| s.get(3).and_then(|t| t.trim().parse::<usize>().ok()))
        .collect()
}

/// A type-1 station names its entry in the trip's .ttr, not just a nearby pole.
/// In Recife, paired boarding/alighting boxes sit on opposite sides of the same
/// path; a platform-side search across the whole route can move one to another visit.
pub fn trip_station_steps(
    trip: &omsi_timetable::Trip,
    track: bool,
    steps: usize,
) -> Vec<Option<usize>> {
    if !track || !trip.stations.is_empty() {
        return vec![None; trip_stations(trip).len()];
    }
    trip.stations_legacy
        .iter()
        .filter(|s| s.first().is_some_and(|id| id.trim().parse::<i64>().is_ok()))
        .map(|s| {
            s.get(1)
                .and_then(|i| i.trim().parse::<usize>().ok())
                .filter(|&i| i < steps)
        })
        .collect()
}

/// Map the authored entry into a streamed section after absent paths and inserted
/// connectors. A station beyond this section waits for its own path to load.
pub fn station_route(step: Option<usize>, first: usize, slots: &[Slot], index: &[usize]) -> StopRoute {
    let Some(step) = step else {
        return StopRoute::Nearest;
    };
    let Some(at) = step
        .checked_sub(first)
        .filter(|&i| matches!(slots.get(i), Some(Slot::Lane(_))))
    else {
        return StopRoute::Outside;
    };
    let ordinal = slots[..at]
        .iter()
        .filter(|s| matches!(s, Slot::Lane(_)))
        .count();
    index
        .get(ordinal)
        .copied()
        .map(StopRoute::Track)
        .unwrap_or(StopRoute::Outside)
}
