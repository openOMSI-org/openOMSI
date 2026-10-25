//! What each piece of rail track is, for a map the vanilla editor made.
//!
//! OMSI 2's editor knows one kind of rail (`[path]` type 2) and nothing of trams, trains or
//! metros, and it cannot be changed. A map maker therefore says what a track is in one of
//! three ways, tried in this order:
//!
//! 1. A rule in `<map>/openomsi-rail.cfg` (see `docs/RAIL.md`): by the spline or object
//!    file the track belongs to, by its tile, id and path, or both.
//! 2. The name of the spline profile or object: `tram_*`, `strassenbahn_*` and `*_tram`
//!    are tram track; `metro_*`, `ubahn_*`, `subway_*` metro; `train_*`, `sbahn_*` and
//!    `*railway*` railway. A generic name (`rail_typ001`, `gleis`) says nothing: the track's
//!    surroundings decide. A map maker picks the right profile in the editor and the game
//!    knows.
//! 3. The track's surroundings: beside a street lane it is tram track; a loop tighter than
//!    a railway ever bends is, too; the rest is railway (`TrackClass::of`).

use super::class::{RailClass, TrackClass, TrackFacts};
use crate::traffic::{Lane, LaneKind, Network};
use std::path::Path;

/// What is known of a lane's track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackInfo {
    pub class: TrackClass,
    /// The rail vehicles it is for: `None` any that fits its class.
    pub only: Option<RailClass>,
    pub third_rail: bool,
    pub tunnel: bool,
    /// A limit the map maker gave (km/h), over the curve's own.
    pub speed_kmh: Option<f32>,
}

/// The file's name for the vehicle kind a track is for, and the track kind it implies.
fn name_hint(name: &str) -> Option<(Option<RailClass>, TrackClass, bool)> {
    let n = name.to_ascii_lowercase();
    let n = n.rsplit(['/', '\\']).next().unwrap_or(&n);
    let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
    if has(&["metro", "ubahn", "u-bahn", "u_bahn", "subway", "u-bhf", "ubhf"]) {
        Some((Some(RailClass::Metro), TrackClass::Mainline, true))
    } else if has(&["tram", "strassenbahn", "straßenbahn", "str_bahn", "tramway"]) {
        Some((Some(RailClass::Tram), TrackClass::Street, false))
    } else if has(&["train", "railway", "sbahn", "s-bahn", "eisenbahn", "fernbahn", "mainline"]) {
        Some((None, TrackClass::Mainline, false))
    } else {
        None
    }
}

/// One rule of `openomsi-rail.cfg`: what it matches (every given matcher must) and what it
/// says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackRule {
    /// A glob (`*` any run of characters, case-insensitive) on the spline profile's or the
    /// object's file name.
    pub file: Option<String>,
    pub tile: Option<(i32, i32)>,
    pub id: Option<i64>,
    pub path: Option<u16>,
    pub class: Option<RailClass>,
    pub street: Option<bool>,
    pub third_rail: Option<bool>,
    pub tunnel: Option<bool>,
    pub speed_kmh: Option<f32>,
}

/// A glob with `*` and `?`, case-insensitive, matched on the whole text.
pub fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|k| go(rest, &t[k..])),
            Some(('?', rest)) => !t.is_empty() && go(rest, &t[1..]),
            Some((c, rest)) => t.first().is_some_and(|x| x == c) && go(rest, &t[1..]),
        }
    }
    let norm = |s: &str| s.to_lowercase().replace('\\', "/").chars().collect::<Vec<_>>();
    let (p, t) = (norm(pattern), norm(text));
    // a pattern without a folder matches the file name alone
    let t: &[char] = if pattern.contains(['/', '\\']) { &t } else { &t[t.iter().rposition(|c| *c == '/').map(|i| i + 1).unwrap_or(0)..] };
    go(&p, t)
}

impl TrackRule {
    pub fn matches(&self, lane: &Lane) -> bool {
        if let Some(k) = lane.key {
            if self.tile.is_some_and(|t| t != k.tile) || self.id.is_some_and(|i| i != k.id) || self.path.is_some_and(|p| p != k.path) {
                return false;
            }
        } else if self.tile.is_some() || self.id.is_some() || self.path.is_some() {
            return false;
        }
        self.file.as_deref().is_none_or(|f| glob(f, &lane.name))
    }
}

/// `openomsi-rail.cfg` of a map.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RailMapConfig {
    pub rules: Vec<TrackRule>,
}

pub const FILE: &str = "openomsi-rail.cfg";

impl RailMapConfig {
    /// The rules of a map's `transport` folder (`network.cfg` and the diverging speeds of
    /// `switches.cfg`) after the file's own: the newer, more exact description wins.
    pub fn add_transit(&mut self, transit: &omsi_transit::TransitData) {
        use omsi_cfg::transport::TransportMode;
        use omsi_transit::infra::Power;
        for r in &transit.network.rules {
            let rail: Vec<TransportMode> = r.modes.iter().filter(|m| m.is_rail()).collect();
            let class = match rail.as_slice() {
                [TransportMode::Tram] => Some(RailClass::Tram),
                [TransportMode::Metro] => Some(RailClass::Metro),
                [TransportMode::Railway] => Some(RailClass::Train),
                _ => None,
            };
            let third_rail = match r.power {
                Power::ThirdRail => Some(true),
                Power::Catenary | Power::Diesel => Some(false),
                _ => None,
            };
            self.rules.push(TrackRule {
                file: r.file.clone(),
                tile: r.tile,
                id: r.id,
                path: None,
                class,
                street: r.street,
                third_rail,
                tunnel: r.tunnel,
                speed_kmh: r.speed.map(|v| v * 3.6),
            });
        }
        for w in &transit.switches.list {
            let (Some(speed), Some(id)) = (w.diverging_speed, w.object) else { continue };
            for &path in &w.diverging_paths {
                self.rules.push(TrackRule { tile: w.tile, id: Some(id), path: Some(path), speed_kmh: Some(speed * 3.6), ..Default::default() });
            }
        }
    }

    pub fn load(map_dir: &Path) -> RailMapConfig {
        match std::fs::read(map_dir.join(FILE)) {
            Ok(b) => Self::parse(&String::from_utf8_lossy(&b)),
            Err(_) => RailMapConfig::default(),
        }
    }

    /// `[track]` sections of `key = value` lines (`#` and `;` start comments). Unknown keys
    /// and sections are ignored, so a newer file still loads.
    pub fn parse(text: &str) -> RailMapConfig {
        let mut cfg = RailMapConfig::default();
        let mut cur: Option<TrackRule> = None;
        let yes = |v: &str| matches!(v.to_ascii_lowercase().as_str(), "1" | "yes" | "true" | "on");
        for raw in text.lines() {
            let line = raw.split(['#', ';']).next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                cfg.rules.extend(cur.take());
                cur = name.trim().eq_ignore_ascii_case("track").then(TrackRule::default);
                continue;
            }
            let (Some(rule), Some((k, v))) = (cur.as_mut(), line.split_once('=')) else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            let nums = || v.split_whitespace().filter_map(|x| x.parse::<i64>().ok()).collect::<Vec<_>>();
            match k.as_str() {
                "file" | "spline" | "object" | "profile" => rule.file = Some(v.to_string()),
                "tile" => {
                    let n = nums();
                    if n.len() >= 2 {
                        rule.tile = Some((n[0] as i32, n[1] as i32));
                    }
                }
                "id" => rule.id = v.parse().ok(),
                "path" => rule.path = v.parse().ok(),
                "class" | "for" => rule.class = RailClass::from_name(v),
                "street" => rule.street = Some(yes(v)),
                "third_rail" => rule.third_rail = Some(yes(v)),
                "tunnel" => rule.tunnel = Some(yes(v)),
                "speed" => rule.speed_kmh = v.parse().ok().filter(|s: &f32| *s > 0.0),
                _ => {}
            }
        }
        cfg.rules.extend(cur);
        cfg
    }

    /// What the rules say of a lane: every matching rule counts, the later one over the
    /// earlier.
    fn said(&self, lane: &Lane) -> TrackRule {
        let mut out = TrackRule::default();
        for r in self.rules.iter().filter(|r| r.matches(lane)) {
            out.class = r.class.or(out.class);
            out.street = r.street.or(out.street);
            out.third_rail = r.third_rail.or(out.third_rail);
            out.tunnel = r.tunnel.or(out.tunnel);
            out.speed_kmh = r.speed_kmh.or(out.speed_kmh);
        }
        out
    }
}

/// The speed a curve of `radius` m allows (km/h) on `class` of track: the speed at which
/// the sideways acceleration is 0.8 m/s² in a street (what a standing passenger of a tram
/// bears) and 1.4 m/s² on a railway, whose outer rail is raised to take most of the rest
/// (the usual limit of 1.0 to 1.5 m/s² unbalanced... with the cant counted in).
pub fn curve_speed_kmh(radius: f32, class: TrackClass) -> f32 {
    let a = match class {
        TrackClass::Street => 0.8,
        TrackClass::Mainline => 1.4,
    };
    (a * radius.max(1.0)).sqrt() * 3.6
}

/// The tightest curve of a lane (m), infinite if it has none.
pub fn tightest_radius(lane: &Lane) -> f32 {
    let k = lane.curvature.iter().fold(0.0f32, |m, c| m.max(c.abs()));
    if k > 1e-4 { 1.0 / k } else { f32::INFINITY }
}

/// Tell every rail lane of the network what it is, as far as the rules, the names and the
/// surroundings say, and bring the speed limits of the rail lanes within what their curves
/// allow (a branch of a turnout is a curve and gets its own limit by it). Returns how many
/// rail lanes there were.
pub fn classify(net: &mut Network, cfg: &RailMapConfig) -> usize {
    // street lanes by 10 m cells, for "a street beside the track"
    let mut cells: hashbrown::HashSet<(i64, i64)> = hashbrown::HashSet::new();
    for l in net.lanes.iter().filter(|l| l.kind == LaneKind::Street) {
        for p in l.points.iter() {
            cells.insert(((p.x / 10.0).floor() as i64, (p.y / 10.0).floor() as i64));
        }
    }
    let street_near = |p: glam::DVec3| {
        let (cx, cy) = ((p.x / 10.0).floor() as i64, (p.y / 10.0).floor() as i64);
        (-1..=1).any(|dx| (-1..=1).any(|dy| cells.contains(&(cx + dx, cy + dy))))
    };
    let mut count = 0;
    for l in net.lanes.iter_mut().filter(|l| l.kind == LaneKind::Rail) {
        count += 1;
        let said = cfg.said(l);
        let hint = name_hint(&l.name);
        let radius = tightest_radius(l);
        let beside = || {
            let n = l.points.len();
            let step = (n / 6).max(1);
            (0..n).step_by(step).filter(|&i| street_near(l.points[i])).count() * 2 >= (n / step).max(1)
        };
        let third_rail = said.third_rail.unwrap_or(hint.is_some_and(|h| h.2));
        let class = match (said.street, hint) {
            (Some(true), _) => TrackClass::Street,
            (Some(false), _) => TrackClass::Mainline,
            (None, Some((_, c, _))) => c,
            (None, None) => TrackClass::of(TrackFacts { street_beside: beside(), min_radius: radius, third_rail }),
        };
        let info = TrackInfo {
            class,
            only: said.class.or(hint.and_then(|h| h.0)),
            third_rail,
            tunnel: said.tunnel.unwrap_or(false),
            speed_kmh: said.speed_kmh,
        };
        // the limit: the map maker's, else the lane's own, brought down by its curve
        let given = info.speed_kmh.unwrap_or(l.speed_limit_kmh);
        l.speed_limit_kmh = given.min(curve_speed_kmh(radius, class).max(10.0));
        l.track = Some(info);
    }
    count
}

#[cfg(test)]
mod transit_tests {
    use super::*;

    #[test]
    fn the_transport_folder_adds_rules_after_the_files() {
        let dir = std::env::temp_dir().join(format!("omsi_track_transit_{}", std::process::id()));
        let t = dir.join("transport");
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join("network.cfg"), "[track]\nfile Splines/*/tram_*.sli\nmodes tram\nstreet yes\npower catenary\nspeed 40 km/h\n[track]\nid 9\nmodes metro\npower third_rail\ntunnel yes\n[track]\nid 10\nmodes tram railway\n").unwrap();
        std::fs::write(t.join("switches.cfg"), "[switch]\nid W\nmodes tram\nobject 77\ntile 1 2\ndiverging_path 1\ndiverging_speed 15 km/h\n").unwrap();
        let mut issues = Vec::new();
        let transit = omsi_transit::TransitData::load(&dir, &mut issues);
        let _ = std::fs::remove_dir_all(&dir);
        let mut cfg = RailMapConfig::parse("[track]\nid 5\nclass = train\n");
        cfg.add_transit(&transit);
        assert_eq!(cfg.rules.len(), 5, "{issues:?}");
        let tram = &cfg.rules[1];
        assert_eq!((tram.class, tram.street, tram.third_rail), (Some(RailClass::Tram), Some(true), Some(false)));
        assert!((tram.speed_kmh.unwrap() - 40.0).abs() < 0.01);
        let metro = &cfg.rules[2];
        assert_eq!((metro.class, metro.third_rail, metro.tunnel), (Some(RailClass::Metro), Some(true), Some(true)));
        assert_eq!(cfg.rules[3].class, None, "tram and railway: any");
        let sw = &cfg.rules[4];
        assert_eq!((sw.tile, sw.id, sw.path), (Some((1, 2)), Some(77), Some(1)));
        assert!((sw.speed_kmh.unwrap() - 15.0).abs() < 0.01);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::LaneBuilder;
    use glam::DVec3;

    fn straight(x: f64, kind: LaneKind, name: &str) -> Lane {
        let mut l = LaneBuilder::polyline(vec![DVec3::new(x, 0.0, 0.0), DVec3::new(x, 200.0, 0.0)], kind, 1.435);
        l.name = name.into();
        l
    }

    #[test]
    fn globs_match_file_names() {
        assert!(glob("tram_*.sli", "Tram_Gruen.sli"));
        assert!(glob("Splines/Custom/*.sli", "splines\\custom\\x.sli"));
        assert!(glob("*tram*", "my_tram_track.sli"));
        assert!(!glob("tram_*.sli", "rail_typ001.sli"));
        assert!(glob("rail_typ00?.sli", "rail_typ001.sli"));
        // a bare name matches the file name, not the folder
        assert!(glob("rail_typ001.sli", "Splines/Marcel/rail_typ001.sli"));
    }

    #[test]
    fn the_file_is_read() {
        let cfg = RailMapConfig::parse(
            "# my map\n[track]\nspline = tram_*.sli\nclass = tram\nspeed = 40\n\n[track]\ntile = 3 -4\nid = 77 ; the viaduct\npath = 1\ntunnel = yes\nthird_rail = on\n[future]\nx = y\n",
        );
        assert_eq!(cfg.rules.len(), 2);
        assert_eq!(cfg.rules[0].class, Some(RailClass::Tram));
        assert_eq!(cfg.rules[0].speed_kmh, Some(40.0));
        assert_eq!(cfg.rules[1].tile, Some((3, -4)));
        assert_eq!((cfg.rules[1].id, cfg.rules[1].path), (Some(77), Some(1)));
        assert_eq!((cfg.rules[1].tunnel, cfg.rules[1].third_rail), (Some(true), Some(true)));
        assert_eq!(RailMapConfig::parse("").rules.len(), 0);
    }

    #[test]
    fn a_track_is_what_its_rule_name_or_surroundings_say() {
        let mut net = Network {
            lanes: vec![
                straight(0.0, LaneKind::Street, "str_2spur_9m.sli"),
                straight(3.0, LaneKind::Rail, "sbahn_typ001.sli"),       // 1: railway by name, though beside a street
                straight(3.0, LaneKind::Rail, "plain.sli"),              // 2: beside the street: tram
                straight(500.0, LaneKind::Rail, "plain.sli"),            // 3: alone: railway
                straight(900.0, LaneKind::Rail, "U-Bahn_tunnel.sli"),    // 4: metro by name
                straight(1300.0, LaneKind::Rail, "weird.sli"),           // 5: told by a rule
            ],
            ..Default::default()
        };
        let cfg = RailMapConfig::parse("[track]\nfile = weird.sli\nclass = tram\nstreet = yes\nspeed = 30\ntunnel = yes\n");
        assert_eq!(classify(&mut net, &cfg), 5);
        let t = |i: usize| net.lanes[i].track.unwrap();
        assert_eq!(t(1).class, TrackClass::Mainline);
        assert_eq!(t(2).class, TrackClass::Street);
        assert_eq!(t(3).class, TrackClass::Mainline);
        assert_eq!((t(4).class, t(4).only, t(4).third_rail), (TrackClass::Mainline, Some(RailClass::Metro), true));
        assert_eq!((t(5).class, t(5).only, t(5).tunnel, t(5).speed_kmh), (TrackClass::Street, Some(RailClass::Tram), true, Some(30.0)));
        assert_eq!(net.lanes[5].speed_limit_kmh, 30.0);
        assert!(net.lanes[0].track.is_none(), "a street is no track");
    }

    #[test]
    fn a_curve_brings_the_speed_down() {
        // a tram loop of 25 m radius: 0.8 m/s² is 16 km/h; a railway bend of 300 m 74 km/h
        assert!((curve_speed_kmh(25.0, TrackClass::Street) - 16.1).abs() < 0.5);
        assert!((curve_speed_kmh(300.0, TrackClass::Mainline) - 73.8).abs() < 1.0);
        let mut net = Network { lanes: vec![LaneBuilder::arc(DVec3::ZERO, 0.0, 60.0, 25.0, 0.0, LaneKind::Rail, 1.435)], ..Default::default() };
        classify(&mut net, &RailMapConfig::default());
        let l = &net.lanes[0];
        assert!(l.speed_limit_kmh < 20.0 && l.speed_limit_kmh >= 10.0, "{}", l.speed_limit_kmh);
        assert_eq!(l.track.unwrap().class, TrackClass::Street, "a 25 m loop is no railway");
    }
}
