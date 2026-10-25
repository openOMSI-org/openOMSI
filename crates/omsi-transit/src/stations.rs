//! `stations.cfg`: the stops and stations of a map and their platforms.
//!
//! ```text
//! [station]
//! id Sokol
//! name Sokol
//! modes metro
//! pos 892000 4196000 31.5        // optional, the map's coordinates
//! object 123456                   // optional: the id code of the map object that is the station
//! hub Sokol_tram                  // optional: stations a passenger changes to on foot
//!
//! [platform]                      // belongs to the station above
//! id 1
//! side right                      // left | right | island | both
//! length 150 m
//! height 1.1 m                    // above the rail head
//! screen_doors yes                // platform screen doors (metro)
//! track 4711                      // the map's spline or object of the track beside it
//! ```

use crate::doc::{parse, Block, Issues};
use omsi_cfg::transport::ModeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Side {
    Left,
    #[default]
    Right,
    Island,
    Both,
}

impl Side {
    pub fn from_name(s: &str) -> Option<Side> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            "island" | "centre" | "center" => Some(Side::Island),
            "both" | "sides" => Some(Side::Both),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Platform {
    pub id: String,
    pub side: Side,
    pub length_m: f32,
    pub height_m: f32,
    pub screen_doors: bool,
    pub track: Option<i64>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub modes: ModeSet,
    pub pos: Option<[f64; 3]>,
    pub object: Option<i64>,
    pub hub: Vec<String>,
    pub platforms: Vec<Platform>,
    pub line: usize,
}

impl Station {
    pub fn platform(&self, id: &str) -> Option<&Platform> {
        self.platforms.iter().find(|p| p.id.eq_ignore_ascii_case(id))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stations {
    pub list: Vec<Station>,
}

impl Stations {
    /// A station by its id (without regard to case).
    pub fn get(&self, id: &str) -> Option<&Station> {
        self.list.iter().find(|s| s.id.eq_ignore_ascii_case(id.trim()))
    }

    pub fn parse(text: &str, file: &str, issues: &mut Vec<crate::doc::Issue>) -> Stations {
        let mut is = Issues::new(file);
        let blocks = parse(text, &mut is);
        let mut out = Stations::default();
        for b in &blocks {
            match b.name.as_str() {
                "station" => out.list.push(read_station(b, &mut is)),
                "platform" => match out.list.last_mut() {
                    Some(s) => s.platforms.push(read_platform(b, &mut is)),
                    None => is.error(b.line, "a [platform] needs a [station] before it"),
                },
                other => is.warn(b.line, format!("unknown block [{other}]")),
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        for s in &out.list {
            if s.id.is_empty() {
                is.error(s.line, "a station has no id");
            } else if seen.iter().any(|x| x.eq_ignore_ascii_case(&s.id)) {
                is.error(s.line, format!("station id {} is used twice", s.id));
            } else {
                seen.push(&s.id);
            }
            let mut pids: Vec<&str> = Vec::new();
            for p in &s.platforms {
                if pids.iter().any(|x| x.eq_ignore_ascii_case(&p.id)) {
                    is.error(p.line, format!("station {}: platform {} is used twice", s.id, p.id));
                }
                pids.push(&p.id);
            }
        }
        for s in &out.list {
            for h in &s.hub {
                if out.get(h).is_none() {
                    is.warn(s.line, format!("station {}: hub {h} is not a station of this file", s.id));
                }
            }
        }
        issues.append(&mut is.list);
        out
    }
}

fn read_station(b: &Block, is: &mut Issues) -> Station {
    let id = b.str("id");
    let mut modes = ModeSet::EMPTY;
    for e in b.all("modes") {
        let m = ModeSet::parse_names(&e.value);
        if m.is_empty() {
            is.error(e.line, format!("no known mode in `{}`", e.value));
        }
        for x in m.iter() {
            modes.insert(x);
        }
    }
    Station {
        name: b.get("name").map(str::to_string).unwrap_or_else(|| id.clone()),
        id,
        modes,
        pos: b.vec3("pos"),
        object: b.get("object").and_then(|v| v.trim().parse().ok()),
        hub: b.list("hub", true),
        platforms: Vec::new(),
        line: b.line,
    }
}

fn read_platform(b: &Block, is: &mut Issues) -> Platform {
    let side = match b.get("side") {
        None => Side::default(),
        Some(v) => Side::from_name(v).unwrap_or_else(|| {
            is.error(b.line_of("side"), format!("side {v} is not left, right, island or both"));
            Side::default()
        }),
    };
    Platform {
        id: b.get("id").map(str::to_string).unwrap_or_else(|| "1".to_string()),
        side,
        length_m: b.metres("length").unwrap_or(0.0),
        height_m: b.metres("height").unwrap_or(0.0),
        screen_doors: b.flag_or("screen_doors", false),
        track: b.get("track").and_then(|v| v.trim().parse().ok()),
        line: b.line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_cfg::transport::TransportMode;

    #[test]
    fn a_station_with_platforms() {
        let mut issues = Vec::new();
        let s = Stations::parse("[station]\nid Sokol\nname Sokol Square\nmodes metro, tram\nhub Sokol_bus\n[platform]\nid 1\nside island\nlength 150 m\nscreen_doors yes\ntrack 77\n[platform]\nid 2\n[station]\nid Sokol_bus\nmodes bus\n", "stations.cfg", &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        let sokol = s.get("sokol").unwrap();
        assert_eq!(sokol.name, "Sokol Square");
        assert!(sokol.modes.contains(TransportMode::Metro) && sokol.modes.contains(TransportMode::Tram));
        assert_eq!(sokol.platforms.len(), 2);
        let p = sokol.platform("1").unwrap();
        assert_eq!((p.side, p.length_m, p.screen_doors, p.track), (Side::Island, 150.0, true, Some(77)));
        assert_eq!(s.get("Sokol_bus").unwrap().name, "Sokol_bus");
    }

    #[test]
    fn mistakes_are_reported() {
        let mut issues = Vec::new();
        Stations::parse("[platform]\nid 1\n[station]\nid A\nmodes spaceship\nhub Nowhere\n[station]\nid a\n", "stations.cfg", &mut issues);
        let text: Vec<String> = issues.iter().map(|i| i.to_string()).collect();
        assert!(text.iter().any(|t| t.contains("needs a [station]")), "{text:?}");
        assert!(text.iter().any(|t| t.contains("no known mode")), "{text:?}");
        assert!(text.iter().any(|t| t.contains("hub Nowhere")), "{text:?}");
        assert!(text.iter().any(|t| t.contains("used twice")), "{text:?}");
    }
}
