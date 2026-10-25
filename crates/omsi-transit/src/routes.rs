//! `routes.cfg`: routes without tours.
//!
//! A route is a set of **segments** - a run from one station to another, with the stops in
//! between - and **services** that say how vehicles run over those segments. Segments may be
//! given in any order and joined at any station, so a line with a branch, a loop, a short-turn
//! or an express pattern is just more segments and another service.
//!
//! ```text
//! [route]
//! id M1
//! name Red line
//! mode metro
//! colour 220 40 40
//!
//! [segment]                  // belongs to the route above
//! id s1
//! from Sokol
//! to Aeroport
//! via Voykovskaya, Vodny     // stops between, in order
//! skip Vodny                 // passed without stopping (an express)
//! dwell 25 s                 // the standing time at a stop (default 30 s)
//! speed 80 km/h              // limit on this stretch (optional)
//! direction both             // forward | both: whether the way back is the same stretch
//!
//! [segment]
//! id s2
//! from Aeroport
//! to Khovrino
//!
//! [service]                  // belongs to the route above
//! id peak
//! segments s1 s2             // in order; each begins where the one before ends
//! headway 150 s              // vehicles every ...  (or give trips)
//! from 05:30
//! to 24:30
//! days Mo-Fr
//! turn_back reverse          // reverse | loop | depot:<id>
//! consist 6                  // cars in the train
//! vehicle Vehicles/Metro/81-717.ovh
//! depot Depot_A              // where the vehicles come from and go to
//!
//! [trip]                     // a service may also list departures instead of a headway
//! service peak
//! depart 05:30 05:42 05:54   // or: at 05:30
//! ```

use crate::doc::{parse, parse_clock, Block, Issue, Issues};
use crate::stations::Stations;
use omsi_cfg::transport::{ModeSet, TransportMode};

pub const DEFAULT_DWELL_S: f32 = 30.0;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Segment {
    pub id: String,
    pub from: String,
    pub to: String,
    pub via: Vec<String>,
    pub skip: Vec<String>,
    pub dwell_s: Option<f32>,
    /// m/s
    pub speed: Option<f32>,
    pub one_way: bool,
    pub line: usize,
}

impl Segment {
    /// Every station of the segment in order, ends included.
    pub fn stations(&self) -> Vec<&str> {
        std::iter::once(self.from.as_str()).chain(self.via.iter().map(String::as_str)).chain(std::iter::once(self.to.as_str())).collect()
    }

    /// The stations the vehicle stops at: all but the skipped ones (the ends always stop).
    pub fn stops(&self) -> Vec<&str> {
        let all = self.stations();
        let last = all.len() - 1;
        all.into_iter().enumerate().filter(|(i, s)| *i == 0 || *i == last || !self.skip.iter().any(|k| k.eq_ignore_ascii_case(s))).map(|(_, s)| s).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TurnBack {
    /// The train changes cab at the last station and runs back.
    Reverse,
    /// The vehicles run on round the loop to the first station.
    Loop,
    /// The vehicles go to the depot.
    Depot(String),
}

impl Default for TurnBack {
    fn default() -> Self {
        TurnBack::Reverse
    }
}

/// Days of the week as a bit mask: bit 0 Monday ... bit 6 Sunday.
pub const ALL_DAYS: u8 = 0x7f;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Service {
    pub id: String,
    pub segments: Vec<String>,
    /// Seconds between vehicles; none when the service lists trips.
    pub headway_s: Option<f32>,
    pub from_s: Option<f32>,
    pub to_s: Option<f32>,
    pub days: u8,
    pub turn_back: TurnBack,
    pub consist: Option<u32>,
    pub vehicles: Vec<String>,
    pub depot: Option<String>,
    /// Departures from the first station (seconds), from the service's `[trip]` blocks.
    pub departures: Vec<f32>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Route {
    pub id: String,
    pub name: String,
    pub mode: Option<TransportMode>,
    pub colour: Option<[u8; 3]>,
    pub segments: Vec<Segment>,
    pub services: Vec<Service>,
    pub line: usize,
}

impl Route {
    pub fn segment(&self, id: &str) -> Option<&Segment> {
        self.segments.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    pub fn service(&self, id: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    /// The stations a service's vehicles call at, in order, from first to last; a station
    /// where two segments meet counts once.
    pub fn service_stops(&self, service: &Service) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in &service.segments {
            let Some(seg) = self.segment(id) else { continue };
            for st in seg.stops() {
                if out.last().is_none_or(|l| !l.eq_ignore_ascii_case(st)) {
                    out.push(st.to_string());
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Routes {
    pub list: Vec<Route>,
}

impl Routes {
    pub fn get(&self, id: &str) -> Option<&Route> {
        self.list.iter().find(|r| r.id.eq_ignore_ascii_case(id.trim()))
    }

    pub fn parse(text: &str, file: &str, issues: &mut Vec<Issue>) -> Routes {
        let mut is = Issues::new(file);
        let blocks = parse(text, &mut is);
        let mut out = Routes::default();
        for b in &blocks {
            match b.name.as_str() {
                "route" => out.list.push(read_route(b, &mut is)),
                "segment" => match out.list.last_mut() {
                    Some(r) => r.segments.push(read_segment(b, &mut is)),
                    None => is.error(b.line, "a [segment] needs a [route] before it"),
                },
                "service" => match out.list.last_mut() {
                    Some(r) => r.services.push(read_service(b, &mut is)),
                    None => is.error(b.line, "a [service] needs a [route] before it"),
                },
                "trip" => match out.list.last_mut() {
                    Some(r) => read_trip(r, b, &mut is),
                    None => is.error(b.line, "a [trip] needs a [route] before it"),
                },
                other => is.warn(b.line, format!("unknown block [{other}]")),
            }
        }
        issues.append(&mut is.list);
        out
    }

    /// Checks the routes against the map's stations and depots: every station exists and serves
    /// the route's mode, every service's segments join up, a service is timed in one way.
    pub fn check(&self, file: &str, stations: &Stations, depots: &[String], map_modes: ModeSet, issues: &mut Vec<Issue>) {
        let mut is = Issues::new(file);
        for (i, r) in self.list.iter().enumerate() {
            if r.id.is_empty() {
                is.error(r.line, "a route has no id");
            } else if self.list[..i].iter().any(|o| o.id.eq_ignore_ascii_case(&r.id)) {
                is.error(r.line, format!("route id {} is used twice", r.id));
            }
            let Some(mode) = r.mode else {
                is.error(r.line, format!("route {} has no valid mode", r.id));
                continue;
            };
            if !map_modes.is_empty() && !map_modes.contains(mode) {
                is.error(r.line, format!("route {} is a {mode} route, the map is made for {}", r.id, map_modes.describe()));
            }
            for (j, s) in r.segments.iter().enumerate() {
                if s.id.is_empty() {
                    is.error(s.line, format!("route {}: a segment has no id", r.id));
                } else if r.segments[..j].iter().any(|o| o.id.eq_ignore_ascii_case(&s.id)) {
                    is.error(s.line, format!("route {}: segment id {} is used twice", r.id, s.id));
                }
                if s.from.is_empty() || s.to.is_empty() {
                    is.error(s.line, format!("route {} segment {}: needs from and to", r.id, s.id));
                } else if s.from.eq_ignore_ascii_case(&s.to) && s.via.is_empty() {
                    is.error(s.line, format!("route {} segment {}: begins and ends at {}", r.id, s.id, s.from));
                }
                for st in s.stations() {
                    match stations.get(st) {
                        None => is.error(s.line, format!("route {} segment {}: station {st} is not in stations.cfg", r.id, s.id)),
                        Some(station) if !station.modes.is_empty() && !station.modes.contains(mode) => {
                            is.error(s.line, format!("route {} segment {}: station {st} does not serve {mode}", r.id, s.id));
                        }
                        Some(_) => {}
                    }
                }
                for k in &s.skip {
                    if !s.via.iter().any(|v| v.eq_ignore_ascii_case(k)) {
                        is.warn(s.line, format!("route {} segment {}: skip {k} is not among the via stations", r.id, s.id));
                    }
                }
            }
            for sv in &r.services {
                check_service(r, sv, mode, depots, &mut is);
            }
        }
        issues.append(&mut is.list);
    }
}

fn check_service(r: &Route, sv: &Service, mode: TransportMode, depots: &[String], is: &mut Issues) {
    if sv.id.is_empty() {
        is.error(sv.line, format!("route {}: a service has no id", r.id));
    }
    if sv.segments.is_empty() {
        is.error(sv.line, format!("route {} service {}: no segments", r.id, sv.id));
    }
    let mut prev: Option<&Segment> = None;
    for id in &sv.segments {
        match r.segment(id) {
            None => is.error(sv.line, format!("route {} service {}: segment {id} does not exist", r.id, sv.id)),
            Some(seg) => {
                if let Some(p) = prev {
                    if !p.to.eq_ignore_ascii_case(&seg.from) {
                        is.error(sv.line, format!("route {} service {}: segment {} ends at {} but {} begins at {}", r.id, sv.id, p.id, p.to, seg.id, seg.from));
                    }
                }
                prev = Some(seg);
            }
        }
    }
    if sv.headway_s.is_none() && sv.departures.is_empty() {
        is.error(sv.line, format!("route {} service {}: give a headway or [trip] departures", r.id, sv.id));
    }
    if let Some(h) = sv.headway_s {
        if h < 10.0 {
            is.error(sv.line, format!("route {} service {}: headway {h} s is too short", r.id, sv.id));
        }
        if sv.from_s.is_some() != sv.to_s.is_some() {
            is.warn(sv.line, format!("route {} service {}: give both from and to, or neither", r.id, sv.id));
        }
    }
    if let (Some(a), Some(b)) = (sv.from_s, sv.to_s) {
        if b <= a {
            is.error(sv.line, format!("route {} service {}: to is not after from", r.id, sv.id));
        }
    }
    if sv.days == 0 {
        is.error(sv.line, format!("route {} service {}: no day is valid", r.id, sv.id));
    }
    if let (Some(first), Some(last)) = (sv.segments.iter().find_map(|i| r.segment(i)), sv.segments.iter().rev().find_map(|i| r.segment(i))) {
        if sv.turn_back == TurnBack::Loop && !last.to.eq_ignore_ascii_case(&first.from) {
            is.error(sv.line, format!("route {} service {}: a loop must end where it begins ({} to {})", r.id, sv.id, last.to, first.from));
        }
    }
    for d in sv.depot.iter().chain(match &sv.turn_back {
        TurnBack::Depot(d) => Some(d),
        _ => None,
    }) {
        if !depots.iter().any(|x| x.eq_ignore_ascii_case(d)) {
            is.error(sv.line, format!("route {} service {}: depot {d} is not in depots.cfg", r.id, sv.id));
        }
    }
    if let Some(c) = sv.consist {
        if c == 0 {
            is.error(sv.line, format!("route {} service {}: consist is 0", r.id, sv.id));
        }
        if c > 1 && mode == TransportMode::Bus {
            is.warn(sv.line, format!("route {} service {}: a bus with a consist of {c}", r.id, sv.id));
        }
    }
}

fn read_route(b: &Block, is: &mut Issues) -> Route {
    let mode = b.get("mode").and_then(|m| {
        let r = TransportMode::from_name(m);
        if r.is_none() {
            is.error(b.line_of("mode"), format!("mode {m} is not bus, tram, railway, metro or trolleybus"));
        }
        r
    });
    let colour = b.get("colour").or_else(|| b.get("color")).and_then(|v| {
        let n: Vec<u8> = v.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        (n.len() == 3).then(|| [n[0], n[1], n[2]])
    });
    let id = b.str("id");
    Route { name: b.get("name").map(str::to_string).unwrap_or_else(|| id.clone()), id, mode, colour, segments: Vec::new(), services: Vec::new(), line: b.line }
}

fn read_segment(b: &Block, is: &mut Issues) -> Segment {
    let one_way = match b.get("direction").map(|d| d.trim().to_ascii_lowercase()) {
        None => false,
        Some(d) if d == "both" => false,
        Some(d) if d == "forward" || d == "oneway" => true,
        Some(d) => {
            is.error(b.line_of("direction"), format!("direction {d} is not forward or both"));
            false
        }
    };
    Segment {
        id: b.str("id"),
        from: b.str("from"),
        to: b.str("to"),
        via: b.list("via", true),
        skip: b.list("skip", true),
        dwell_s: b.seconds("dwell"),
        speed: b.speed("speed"),
        one_way,
        line: b.line,
    }
}

fn read_service(b: &Block, is: &mut Issues) -> Service {
    let turn_back = match b.get("turn_back") {
        None => TurnBack::Reverse,
        Some(v) => {
            let v = v.trim();
            if v.eq_ignore_ascii_case("reverse") {
                TurnBack::Reverse
            } else if v.eq_ignore_ascii_case("loop") {
                TurnBack::Loop
            } else if let Some(d) = v.strip_prefix("depot:").or_else(|| v.strip_prefix("Depot:")) {
                TurnBack::Depot(d.trim().to_string())
            } else {
                is.error(b.line_of("turn_back"), format!("turn_back {v} is not reverse, loop or depot:<id>"));
                TurnBack::Reverse
            }
        }
    };
    let clock = |key: &str, is: &mut Issues| {
        b.get(key).and_then(|v| {
            let t = parse_clock(v);
            if t.is_none() {
                is.error(b.line_of(key), format!("{key} {v} is not a time like 05:30"));
            }
            t
        })
    };
    let (from_s, to_s) = (clock("from", is), clock("to", is));
    let days = match b.get("days") {
        None => ALL_DAYS,
        Some(v) => parse_days(v).unwrap_or_else(|| {
            is.error(b.line_of("days"), format!("days {v} is not like Mo-Fr or Sa,Su"));
            0
        }),
    };
    let headway_s = b.get("headway").and_then(|_| {
        let h = b.seconds("headway");
        if h.is_none() {
            is.error(b.line_of("headway"), format!("headway {} is not like 150 s or 4 min", b.str("headway")));
        }
        h
    });
    Service {
        id: b.str("id"),
        segments: b.list("segments", false),
        headway_s,
        from_s,
        to_s,
        days,
        turn_back,
        consist: b.get("consist").and_then(|v| v.trim().parse().ok()),
        vehicles: b.list("vehicle", true),
        depot: b.get("depot").map(|d| d.trim().to_string()),
        departures: Vec::new(),
        line: b.line,
    }
}

fn read_trip(r: &mut Route, b: &Block, is: &mut Issues) {
    let sid = b.str("service");
    let mut times = Vec::new();
    for e in b.all("depart").chain(b.all("at")) {
        for w in e.value.split(|c: char| c.is_whitespace() || c == ',') {
            if w.is_empty() {
                continue;
            }
            match parse_clock(w) {
                Some(t) => times.push(t),
                None => is.error(e.line, format!("{w} is not a time like 05:30")),
            }
        }
    }
    match r.services.iter_mut().find(|s| s.id.eq_ignore_ascii_case(&sid)) {
        Some(s) => s.departures.extend(times),
        None => is.error(b.line, format!("[trip]: service {sid} does not exist in route {}", r.id)),
    }
}

/// `Mo-Fr`, `Sa,Su`, `Mo Wed Fri`, `all`, `weekdays`, `weekend`.
pub fn parse_days(v: &str) -> Option<u8> {
    fn day(w: &str) -> Option<u8> {
        let w = w.to_ascii_lowercase();
        let w = w.trim_end_matches('.');
        Some(match &w[..w.len().min(3)] {
            "mo" | "mon" => 0,
            "tu" | "tue" => 1,
            "we" | "wed" => 2,
            "th" | "thu" => 3,
            "fr" | "fri" => 4,
            "sa" | "sat" => 5,
            "su" | "sun" => 6,
            _ => return None,
        })
    }
    let v = v.trim().to_ascii_lowercase();
    match v.as_str() {
        "all" | "daily" => return Some(ALL_DAYS),
        "weekdays" => return Some(0b001_1111),
        "weekend" | "weekends" => return Some(0b110_0000),
        _ => {}
    }
    let mut mask = 0u8;
    for part in v.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let (a, b) = (day(a)?, day(b)?);
            let mut d = a;
            loop {
                mask |= 1 << d;
                if d == b {
                    break;
                }
                d = (d + 1) % 7;
            }
        } else {
            mask |= 1 << day(part)?;
        }
    }
    (mask != 0).then_some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATIONS: &str = "[station]\nid A\nmodes metro\n[station]\nid B\nmodes metro\n[station]\nid C\nmodes metro\n[station]\nid D\nmodes metro\n[station]\nid X\nmodes tram\n";

    fn stations() -> Stations {
        let mut i = Vec::new();
        Stations::parse(STATIONS, "stations.cfg", &mut i)
    }

    #[test]
    fn a_line_of_segments_and_a_service() {
        let text = "[route]\nid M1\nname Red\nmode metro\ncolour 220 40 40\n[segment]\nid s1\nfrom A\nto C\nvia B\nskip B\ndwell 20 s\nspeed 80 km/h\n[segment]\nid s2\nfrom C\nto D\n[service]\nid peak\nsegments s1 s2\nheadway 150 s\nfrom 05:30\nto 24:30\ndays Mo-Fr\nturn_back depot:Dep\nconsist 6\ndepot Dep\n";
        let mut issues = Vec::new();
        let routes = Routes::parse(text, "routes.cfg", &mut issues);
        routes.check("routes.cfg", &stations(), &["Dep".to_string()], ModeSet::EMPTY, &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        let r = routes.get("m1").unwrap();
        assert_eq!(r.colour, Some([220, 40, 40]));
        let s = r.service("peak").unwrap();
        assert_eq!(s.days, 0b001_1111);
        assert_eq!(s.turn_back, TurnBack::Depot("Dep".into()));
        assert_eq!(r.service_stops(s), vec!["A", "C", "D"], "B is skipped, C counts once");
        assert!((r.segment("s1").unwrap().speed.unwrap() - 80.0 / 3.6).abs() < 1e-3);
        assert_eq!(r.segment("s1").unwrap().stations(), vec!["A", "B", "C"]);
    }

    #[test]
    fn a_branch_is_two_services_over_shared_segments() {
        let text = "[route]\nid M\nmode metro\n[segment]\nid trunk\nfrom A\nto B\n[segment]\nid left\nfrom B\nto C\n[segment]\nid right\nfrom B\nto D\n[service]\nid to_c\nsegments trunk left\nheadway 4 min\n[service]\nid to_d\nsegments trunk right\nheadway 4 min\n";
        let mut issues = Vec::new();
        let routes = Routes::parse(text, "routes.cfg", &mut issues);
        routes.check("routes.cfg", &stations(), &[], ModeSet::EMPTY, &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        let r = routes.get("M").unwrap();
        assert_eq!(r.service_stops(r.service("to_d").unwrap()), vec!["A", "B", "D"]);
    }

    #[test]
    fn trips_replace_a_headway() {
        let text = "[route]\nid R\nmode metro\n[segment]\nid s\nfrom A\nto B\n[service]\nid timed\nsegments s\n[trip]\nservice timed\ndepart 05:30 05:42\nat 06:00\n";
        let mut issues = Vec::new();
        let routes = Routes::parse(text, "routes.cfg", &mut issues);
        routes.check("routes.cfg", &stations(), &[], ModeSet::EMPTY, &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(routes.get("R").unwrap().service("timed").unwrap().departures, vec![19800.0, 20520.0, 21600.0]);
    }

    #[test]
    fn mistakes_are_reported() {
        let text = "[route]\nid R\nmode metro\n[segment]\nid s1\nfrom A\nto B\n[segment]\nid s2\nfrom C\nto X\n[service]\nid broken\nsegments s1 s2 s3\nheadway 2 s\nturn_back loop\ndepot Nowhere\n[service]\nid untimed\nsegments s1\n";
        let mut issues = Vec::new();
        let routes = Routes::parse(text, "routes.cfg", &mut issues);
        routes.check("routes.cfg", &stations(), &[], ModeSet::of(TransportMode::Tram), &mut issues);
        let all: Vec<String> = issues.iter().map(|i| i.to_string()).collect();
        for want in ["does not serve metro", "ends at B but s2 begins at C", "segment s3 does not exist", "too short", "must end where it begins", "depot Nowhere", "give a headway", "the map is made for tram"] {
            assert!(all.iter().any(|t| t.contains(want)), "missing `{want}` in {all:#?}");
        }
    }

    #[test]
    fn days() {
        assert_eq!(parse_days("Mo-Fr"), Some(0b001_1111));
        assert_eq!(parse_days("Sa,Su"), Some(0b110_0000));
        assert_eq!(parse_days("Fri-Mon"), Some(0b111_0001));
        assert_eq!(parse_days("Wed"), Some(0b000_0100));
        assert_eq!(parse_days("weekend"), Some(0b110_0000));
        assert_eq!(parse_days("someday"), None);
    }
}
