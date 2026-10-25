//! Routes of a map's `transport` folder as timetable lines.
//!
//! A service of `routes.cfg` (segments between stations, a headway or listed departures) becomes
//! a [`Line`] named after its route, with a [`Tour`] of one trip for every departure and the
//! outward and, where the service turns back by reversing, the return [`Trip`]. The game's
//! timetable, duty and AI machinery then runs it as it runs any `.ttl`.
//!
//! Stations are found among the map's bus stops by the `object` of the station (the id code of
//! the stop's object), else by name.

use crate::{Line, TimetableData, Tour, TourTrip, Trip};

/// The standing time at a stop of a segment that gives none (s).
pub const DEFAULT_DWELL_S: f32 = 30.0;
use omsi_transit::routes::{Route, Service, TurnBack};
use omsi_transit::stations::Station;
use omsi_transit::TransitData;

/// A stop of a transit trip.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransitStop {
    /// The stop object's id (what the trip's `stations` hold).
    pub object: i64,
    pub name: String,
    /// Where the station stands, when `stations.cfg` says (else the map's object tells).
    pub pos: Option<[f64; 3]>,
    /// Standing time (s).
    pub dwell_s: f32,
    /// The speed limit of the stretch to the next stop (m/s), when the segment gives one.
    pub speed: Option<f32>,
}

/// A trip made from a route, to be laid on the rails.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitTrip {
    pub trip: String,
    pub mode: omsi_cfg::transport::TransportMode,
    pub stops: Vec<TransitStop>,
    /// Where the vehicle comes out of its depot before it reaches the first stop (the depot's
    /// gate): the trip begins there, and the first stop is reached after the run from it.
    pub lead_in: Option<[f64; 3]>,
}

/// What the merge did, for the log.
#[derive(Debug, Default, PartialEq)]
pub struct Merged {
    pub lines: usize,
    pub trips: usize,
    pub tours: usize,
    pub skipped: Vec<String>,
}

fn stop_id(data: &TimetableData, st: &Station) -> Option<i64> {
    if let Some(o) = st.object {
        if data.bus_stops.iter().any(|b| b.object_id == o) {
            return Some(o);
        }
    }
    data.bus_stops.iter().find(|b| b.name.trim().eq_ignore_ascii_case(st.name.trim()) || b.name.trim().eq_ignore_ascii_case(st.id.trim())).map(|b| b.object_id)
}

/// The decimal day mask of a `[newtour]`: Monday..Sunday in bits 0..6, the holiday bit with
/// Sunday, both school bits set.
fn day_mask(days: u8) -> u32 {
    let mut m = (days & 0x7f) as u32;
    if days & 0x40 != 0 {
        m |= 1 << 7;
    }
    m | (1 << 8) | (1 << 9)
}

/// Departures of a service in minutes after midnight, sorted.
pub fn departures(sv: &Service) -> Vec<f32> {
    let mut out: Vec<f32> = sv.departures.iter().map(|s| s / 60.0).collect();
    if let Some(h) = sv.headway_s.filter(|h| *h >= 10.0) {
        let (from, to) = (sv.from_s.unwrap_or(0.0), sv.to_s.unwrap_or(24.0 * 3600.0));
        let mut t = from;
        while t <= to + 0.5 {
            out.push(t / 60.0);
            t += h;
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    out
}

fn trip_for(name: String, route: &Route, ids: &[i64], last: &str) -> Trip {
    Trip { name, terminus: last.to_string(), line: route.id.clone(), stations: ids.to_vec(), ..Default::default() }
}

/// Adds the services of `transit` to `data` as lines, tours and trips. A service whose stations
/// are not all stops of the map is skipped and named in `skipped`.
pub fn merge_transit(data: &mut TimetableData, transit: &TransitData) -> Merged {
    let mut out = Merged::default();
    // (a line the map has as a .ttl is the map's own and stays as it is)
    let own: Vec<String> = data.lines.iter().map(|l| l.name.to_ascii_lowercase()).collect();
    for route in &transit.routes.list {
        if own.contains(&route.id.to_ascii_lowercase()) {
            out.skipped.push(format!("{}: the map has a timetable line of that name", route.id));
            continue;
        }
        for sv in &route.services {
            let label = format!("{}/{}", route.id, sv.id);
            let names = route.service_stops(sv);
            let mut ids = Vec::new();
            let mut missing = None;
            for n in &names {
                match transit.stations.get(n).and_then(|st| stop_id(data, st)) {
                    Some(id) => ids.push(id),
                    None => {
                        missing = Some(n.clone());
                        break;
                    }
                }
            }
            if let Some(m) = missing {
                out.skipped.push(format!("{label}: station {m} is not a bus stop of the map"));
                continue;
            }
            if ids.len() < 2 {
                out.skipped.push(format!("{label}: fewer than two stops"));
                continue;
            }
            let deps = departures(sv);
            if deps.is_empty() {
                out.skipped.push(format!("{label}: no departures"));
                continue;
            }
            let ai_group = sv.depot.as_ref().and_then(|d| transit.depots.list.iter().find(|x| x.id.eq_ignore_ascii_case(d))).and_then(|d| d.ai_group.clone()).unwrap_or_default();
            let (first, last) = (names.first().cloned().unwrap_or_default(), names.last().cloned().unwrap_or_default());
            let line_name = route.id.clone();
            let out_trip = format!("{}_{}_out", route.id, sv.id);
            let mut back_ids = ids.clone();
            back_ids.reverse();
            let back_trip = format!("{}_{}_back", route.id, sv.id);
            let to_station = |n: &str| transit.stations.get(n).map(|s| s.name.clone()).unwrap_or_else(|| n.to_string());
            data.trips.retain(|t| t.name != out_trip && t.name != back_trip);
            // the dwell and speed of each stop, from the segments the service runs over
            let mut meta: Vec<TransitStop> = Vec::new();
            for sid in &sv.segments {
                let Some(seg) = route.segment(sid) else { continue };
                let stops = seg.stops();
                for (k, st) in stops.iter().enumerate() {
                    let Some(station) = transit.stations.get(st) else { continue };
                    let Some(object) = stop_id(data, station) else { continue };
                    if meta.last().is_some_and(|m| m.object == object) {
                        // (the stop where two segments meet: the later segment's speed to the next)
                        if let Some(m) = meta.last_mut() {
                            m.speed = seg.speed.or(m.speed);
                        }
                        continue;
                    }
                    let _ = k;
                    meta.push(TransitStop { object, name: station.name.clone(), pos: station.pos, dwell_s: seg.dwell_s.unwrap_or(crate::transit::DEFAULT_DWELL_S), speed: seg.speed });
                }
            }
            data.trips.push(trip_for(out_trip.clone(), route, &ids, &to_station(&last)));
            data.transit_trips.retain(|t| t.trip != out_trip && t.trip != back_trip);
            data.transit_trips.push(TransitTrip { trip: out_trip.clone(), mode: route.mode.unwrap_or(omsi_cfg::transport::TransportMode::Bus), stops: meta.clone(), lead_in: None });
            out.trips += 1;
            let reverse = sv.turn_back == TurnBack::Reverse && !route.segment(sv.segments.first().map(String::as_str).unwrap_or("")).is_some_and(|s| s.one_way);
            if reverse {
                data.trips.push(trip_for(back_trip.clone(), route, &back_ids, &to_station(&first)));
                let mut back_meta = meta.clone();
                back_meta.reverse();
                data.transit_trips.push(TransitTrip { trip: back_trip.clone(), mode: route.mode.unwrap_or(omsi_cfg::transport::TransportMode::Bus), stops: back_meta, lead_in: None });
                out.trips += 1;
            }
            // the first departure of the day comes out of the depot's gate, when the service's depot has one
            let gate = sv.depot.as_ref().and_then(|d| transit.depots.list.iter().find(|x| x.id.eq_ignore_ascii_case(d))).and_then(|d| d.gates.first().copied());
            let pull_out = format!("{out_trip}_pullout");
            data.trips.retain(|t| t.name != pull_out);
            data.transit_trips.retain(|t| t.trip != pull_out);
            if let (Some(g), Some(base)) = (gate, data.trips.iter().find(|t| t.name == out_trip).cloned()) {
                data.trips.push(Trip { name: pull_out.clone(), ..base });
                let mut m = data.transit_trips.iter().find(|t| t.trip == out_trip).cloned().unwrap();
                m.trip = pull_out.clone();
                m.lead_in = Some(g);
                data.transit_trips.push(m);
                out.trips += 1;
            }
            let idx = match data.lines.iter().position(|l| l.name.eq_ignore_ascii_case(&line_name)) {
                Some(i) => i,
                None => {
                    data.lines.push(Line { name: line_name.clone(), user_allowed: true, ..Default::default() });
                    out.lines += 1;
                    data.lines.len() - 1
                }
            };
            let mask = day_mask(sv.days).to_string();
            for (k, d) in deps.iter().enumerate() {
                let number = format!("{}-{:02}{:02}", sv.id, (*d as u32 / 60) % 100, *d as u32 % 60);
                // (vehicles run both ways at once: the way back is its own tour, leaving the other
                // end at the same times, not a second trip after the first, whose running time
                // the file does not give)
                data.lines[idx].tours.push(Tour { number: number.clone(), ai_group: ai_group.clone(), extra: mask.clone(), trips: vec![TourTrip { trip: if k == 0 && gate.is_some() { pull_out.clone() } else { out_trip.clone() }, profile: 0, departure: *d }] });
                out.tours += 1;
                if reverse {
                    data.lines[idx].tours.push(Tour { number: format!("{number}r"), ai_group: ai_group.clone(), extra: mask.clone(), trips: vec![TourTrip { trip: back_trip.clone(), profile: 0, departure: *d }] });
                    out.tours += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BusStopEntry;

    fn transit() -> TransitData {
        let dir = std::env::temp_dir().join(format!("omsi-tt-transit-{}", std::process::id()));
        let t = dir.join("transport");
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join("stations.cfg"), "[station]\nid A\nname Alpha\nobject 101\n[station]\nid B\nname Beta\n[station]\nid C\nname Gamma\nobject 103\n[station]\nid Z\nname Zed\n").unwrap();
        std::fs::write(t.join("routes.cfg"), "[route]\nid T1\nmode tram\n[segment]\nid s1\nfrom A\nto C\nvia B\n[segment]\nid s2\nfrom C\nto Z\n[service]\nid all\nsegments s1\nheadway 30 min\nfrom 06:00\nto 07:00\ndays Mo-Fr\n[service]\nid lost\nsegments s1 s2\nheadway 10 min\n").unwrap();
        let mut issues = Vec::new();
        let d = TransitData::load(&dir, &mut issues);
        let _ = std::fs::remove_dir_all(&dir);
        d
    }

    #[test]
    fn a_service_becomes_a_line_of_tours() {
        let mut data = TimetableData::default();
        for (n, id) in [("Alpha", 101), ("Beta", 102), ("Gamma", 103)] {
            data.bus_stops.push(BusStopEntry { name: n.into(), object_id: id, ..Default::default() });
        }
        let m = merge_transit(&mut data, &transit());
        assert_eq!((m.lines, m.tours, m.trips), (1, 6, 2), "{m:?}");
        assert_eq!(m.skipped.len(), 1);
        assert!(m.skipped[0].contains("T1/lost") && m.skipped[0].contains("Zed") || m.skipped[0].contains("Z"), "{:?}", m.skipped);
        let line = &data.lines[0];
        assert_eq!((line.name.as_str(), line.user_allowed), ("T1", true));
        assert_eq!(line.tours[0].number, "all-0600");
        assert_eq!(line.tours[4].trips[0].departure, 420.0);
        assert_eq!(line.tours[1].number, "all-0600r");
        assert_eq!(line.tours[0].extra, (0b001_1111u32 | 1 << 8 | 1 << 9).to_string());
        let out = data.trips.iter().find(|t| t.name == "T1_all_out").unwrap();
        assert_eq!(out.stations, vec![101, 102, 103], "Beta is found by name");
        let back = data.trips.iter().find(|t| t.name == "T1_all_back").unwrap();
        assert_eq!(back.stations, vec![103, 102, 101]);
        assert_eq!(out.terminus, "Gamma");
        // the line text the game reads back
        assert!(line.to_text().contains("[addtrip]\r\nT1_all_out"));
        // the stops of each trip, for the router
        let tt = data.transit_trips.iter().find(|t| t.trip == "T1_all_out").unwrap();
        assert_eq!(tt.stops.iter().map(|s| s.object).collect::<Vec<_>>(), vec![101, 102, 103]);
        assert_eq!(tt.stops[0].dwell_s, 30.0);
        assert_eq!(tt.mode, omsi_cfg::transport::TransportMode::Tram);
        let back = data.transit_trips.iter().find(|t| t.trip == "T1_all_back").unwrap();
        assert_eq!(back.stops[0].object, 103);
    }

    #[test]
    fn departures_come_from_headway_and_trips() {
        let sv = Service { headway_s: Some(1800.0), from_s: Some(21600.0), to_s: Some(25200.0), departures: vec![23400.0], ..Default::default() };
        assert_eq!(departures(&sv), vec![360.0, 390.0, 420.0]);
    }

    #[test]
    fn the_first_departure_comes_out_of_the_depot_gate() {
        let dir = std::env::temp_dir().join(format!("omsi-tt-gate-{}", std::process::id()));
        let t = dir.join("transport");
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join("stations.cfg"), "[station]\nid A\nname Alpha\nobject 101\n[station]\nid B\nname Beta\nobject 102\n").unwrap();
        std::fs::write(t.join("depots.cfg"), "[depot]\nid D\nmodes tram\ngate 5 6 7\n").unwrap();
        std::fs::write(t.join("routes.cfg"), "[route]\nid T\nmode tram\n[segment]\nid s\nfrom A\nto B\n[service]\nid all\nsegments s\nheadway 30 min\nfrom 06:00\nto 07:00\ndepot D\n").unwrap();
        let mut issues = Vec::new();
        let transit = TransitData::load(&dir, &mut issues);
        let _ = std::fs::remove_dir_all(&dir);
        let mut data = TimetableData::default();
        for (n, id) in [("Alpha", 101), ("Beta", 102)] {
            data.bus_stops.push(BusStopEntry { name: n.into(), object_id: id, ..Default::default() });
        }
        merge_transit(&mut data, &transit);
        let first = &data.lines[0].tours[0];
        assert_eq!(first.trips[0].trip, "T_all_out_pullout");
        let later = data.lines[0].tours.iter().find(|t| t.number == "all-0630").unwrap();
        assert_eq!(later.trips[0].trip, "T_all_out", "only the first departure comes from the depot");
        let m = data.transit_trips.iter().find(|t| t.trip == "T_all_out_pullout").unwrap();
        assert_eq!(m.lead_in, Some([5.0, 6.0, 7.0]));
        assert_eq!(m.stops.len(), 2);
        assert_eq!(data.trips.iter().find(|t| t.name == "T_all_out_pullout").unwrap().stations, vec![101, 102]);
    }
}
