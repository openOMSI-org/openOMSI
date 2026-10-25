//! `network.cfg`, `switches.cfg`, `depots.cfg`, `signals.cfg`: the track and what stands on it.
//!
//! ```text
//! # network.cfg: which tracks are for which mode (later rules win)
//! [track]
//! file Splines/MyMap/tram_*.sli   // glob on the spline profile or object file
//! tile 12 5                       // optional
//! id 4711                         // optional
//! modes tram
//! street yes
//! power catenary                  // catenary | third_rail | none | diesel
//! tunnel no
//! speed 40 km/h
//! gauge 1435                      // mm, optional
//!
//! # switches.cfg
//! [switch]
//! id W12
//! modes tram railway
//! kind simple                     // simple | crossover | double_slip | diamond | scissors | wire
//! object 123456                   // the map object (its id code) or pos x y z
//! branches 2
//! default straight                // straight | diverging
//! drive auto                      // auto (by route) | manual (driver) | interlocked
//! diverging_speed 25 km/h
//! locked_by S12                   // a signal
//!
//! # depots.cfg
//! [depot]
//! id Depot_A
//! name Depot A
//! modes tram
//! capacity 24
//! ai_group Trams_North
//! gate 100 200 6                  // where vehicles leave and enter the network
//! station Depot_A_stop            // the station vehicles are scheduled from
//! turning_loop yes
//!
//! # signals.cfg
//! [signal]
//! id S12
//! modes railway metro
//! kind main                       // main | distant | shunt | block | speed | tram
//! object 123456
//! block B7                        // the block it guards
//! [block]
//! id B7
//! from 892000 4196000 31.5        // where the block begins on the track (the map's coordinates)
//! to 892850 4196000 31.5          // and where it ends; the game routes the lanes between them
//! length 850 m                    // optional with from and to
//! modes railway metro
//! next B8
//! ```

use crate::doc::{parse, Block, Issue, Issues};
use omsi_cfg::transport::ModeSet;

fn modes_of(b: &Block, is: &mut Issues, required: bool) -> ModeSet {
    let mut modes = ModeSet::EMPTY;
    for e in b.all("modes").chain(b.all("mode")) {
        let m = ModeSet::parse_names(&e.value);
        if m.is_empty() {
            is.error(e.line, format!("no known mode in `{}`", e.value));
        }
        for x in m.iter() {
            modes.insert(x);
        }
    }
    if required && modes.is_empty() && b.get("modes").is_none() && b.get("mode").is_none() {
        is.error(b.line, format!("[{}] needs modes", b.name));
    }
    modes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Power {
    #[default]
    Unspecified,
    Catenary,
    ThirdRail,
    None,
    Diesel,
}

impl Power {
    pub fn from_name(s: &str) -> Option<Power> {
        match s.trim().to_ascii_lowercase().as_str() {
            "catenary" | "overhead" | "wire" => Some(Power::Catenary),
            "third_rail" | "thirdrail" | "third rail" => Some(Power::ThirdRail),
            "none" | "unpowered" => Some(Power::None),
            "diesel" => Some(Power::Diesel),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackRule {
    pub file: Option<String>,
    pub tile: Option<(i32, i32)>,
    pub id: Option<i64>,
    pub modes: ModeSet,
    pub street: Option<bool>,
    pub power: Power,
    pub tunnel: Option<bool>,
    pub speed: Option<f32>,
    pub gauge_mm: Option<f32>,
    pub line: usize,
}

impl TrackRule {
    /// Does the rule apply to a spline/object of this file, tile and id?
    pub fn matches(&self, file: &str, tile: (i32, i32), id: i64) -> bool {
        self.file.as_deref().is_none_or(|g| glob(g, file)) && self.tile.is_none_or(|t| t == tile) && self.id.is_none_or(|i| i == id)
    }
}

/// `*` and `?` wildcards, without regard to case or the direction of slashes.
pub fn glob(pattern: &str, text: &str) -> bool {
    fn norm(s: &str) -> Vec<char> {
        s.replace('\\', "/").to_lowercase().chars().collect()
    }
    fn go(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|i| go(rest, &t[i..])),
            Some(('?', rest)) => !t.is_empty() && go(rest, &t[1..]),
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    go(&norm(pattern), &norm(text))
}

/// What the rules say about one piece of track (later rules win, field by field).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackFacts {
    pub modes: ModeSet,
    pub street: Option<bool>,
    pub power: Power,
    pub tunnel: Option<bool>,
    pub speed: Option<f32>,
    pub gauge_mm: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Network {
    pub rules: Vec<TrackRule>,
}

impl Network {
    pub fn facts(&self, file: &str, tile: (i32, i32), id: i64) -> TrackFacts {
        let mut f = TrackFacts::default();
        for r in self.rules.iter().filter(|r| r.matches(file, tile, id)) {
            if !r.modes.is_empty() {
                f.modes = r.modes;
            }
            f.street = r.street.or(f.street);
            if r.power != Power::Unspecified {
                f.power = r.power;
            }
            f.tunnel = r.tunnel.or(f.tunnel);
            f.speed = r.speed.or(f.speed);
            f.gauge_mm = r.gauge_mm.or(f.gauge_mm);
        }
        f
    }

    pub fn parse(text: &str, file: &str, issues: &mut Vec<Issue>) -> Network {
        let mut is = Issues::new(file);
        let blocks = parse(text, &mut is);
        let mut out = Network::default();
        for b in &blocks {
            if b.name != "track" {
                is.warn(b.line, format!("unknown block [{}]", b.name));
                continue;
            }
            let tile = b.get("tile").and_then(|v| {
                let n: Vec<i32> = v.split_whitespace().filter_map(|w| w.parse().ok()).collect();
                (n.len() == 2).then(|| (n[0], n[1]))
            });
            if b.get("tile").is_some() && tile.is_none() {
                is.error(b.line_of("tile"), "tile needs two numbers");
            }
            let power = match b.get("power") {
                None => Power::Unspecified,
                Some(v) => Power::from_name(v).unwrap_or_else(|| {
                    is.error(b.line_of("power"), format!("power {v} is not catenary, third_rail, diesel or none"));
                    Power::Unspecified
                }),
            };
            if b.get("file").is_none() && tile.is_none() && b.get("id").is_none() {
                is.error(b.line, "a [track] rule needs file, tile or id (else it would match everything)");
            }
            out.rules.push(TrackRule {
                file: b.get("file").map(str::to_string),
                tile,
                id: b.get("id").and_then(|v| v.trim().parse().ok()),
                modes: modes_of(b, &mut is, false),
                street: b.flag("street"),
                power,
                tunnel: b.flag("tunnel"),
                speed: b.speed("speed"),
                gauge_mm: b.f32("gauge"),
                line: b.line,
            });
        }
        issues.append(&mut is.list);
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwitchKind {
    #[default]
    Simple,
    Crossover,
    DoubleSlip,
    Diamond,
    Scissors,
    Wire,
}

impl SwitchKind {
    pub fn from_name(s: &str) -> Option<SwitchKind> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "simple" => SwitchKind::Simple,
            "crossover" => SwitchKind::Crossover,
            "double_slip" | "slip" => SwitchKind::DoubleSlip,
            "diamond" | "crossing" => SwitchKind::Diamond,
            "scissors" => SwitchKind::Scissors,
            "wire" | "overhead" => SwitchKind::Wire,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Drive {
    /// Set by the route the vehicle drives.
    #[default]
    Auto,
    /// Thrown by the driver.
    Manual,
    /// Set by the signalling and held while a signal protects it.
    Interlocked,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Switch {
    pub id: String,
    pub modes: ModeSet,
    pub kind: SwitchKind,
    pub object: Option<i64>,
    pub pos: Option<[f64; 3]>,
    pub branches: u8,
    pub default_diverging: bool,
    pub drive: Drive,
    pub diverging_speed: Option<f32>,
    pub locked_by: Option<String>,
    /// The tile of the object and the paths of it that are the diverging branches: where the
    /// speed limit of `diverging_speed` applies.
    pub tile: Option<(i32, i32)>,
    pub diverging_paths: Vec<u16>,
    /// The `[path]` of the straight way.
    pub straight_path: u16,
    pub line: usize,
}

impl Switch {
    /// The `[path]` the switch rests in: the straight way, or the first diverging one.
    pub fn default_path(&self) -> Option<u16> {
        if self.default_diverging { self.diverging_paths.first().copied() } else { Some(self.straight_path) }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Depot {
    pub id: String,
    pub name: String,
    pub modes: ModeSet,
    pub capacity: u32,
    pub ai_group: Option<String>,
    pub gates: Vec<[f64; 3]>,
    pub station: Option<String>,
    pub turning_loop: bool,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignalKind {
    #[default]
    Main,
    Distant,
    Shunt,
    Block,
    Speed,
    Tram,
}

impl SignalKind {
    pub fn from_name(s: &str) -> Option<SignalKind> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "main" => SignalKind::Main,
            "distant" => SignalKind::Distant,
            "shunt" => SignalKind::Shunt,
            "block" => SignalKind::Block,
            "speed" => SignalKind::Speed,
            "tram" => SignalKind::Tram,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Signal {
    pub id: String,
    pub modes: ModeSet,
    pub kind: SignalKind,
    pub object: Option<i64>,
    pub block: Option<String>,
    pub line: usize,
}

/// A stretch of track that holds one vehicle at a time (railway and metro signalling).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block_ {
    pub id: String,
    pub modes: ModeSet,
    pub length_m: f32,
    pub next: Vec<String>,
    /// Where the block begins and ends on the track, when the file says.
    pub from: Option<[f64; 3]>,
    pub to: Option<[f64; 3]>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Switches {
    pub list: Vec<Switch>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Depots {
    pub list: Vec<Depot>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Signals {
    pub signals: Vec<Signal>,
    pub blocks: Vec<Block_>,
}

fn id_check<'a>(is: &mut Issues, what: &str, items: impl Iterator<Item = (&'a str, usize)>) {
    let mut seen: Vec<&str> = Vec::new();
    for (id, line) in items {
        if id.is_empty() {
            is.error(line, format!("a {what} has no id"));
        } else if seen.iter().any(|s| s.eq_ignore_ascii_case(id)) {
            is.error(line, format!("{what} id {id} is used twice"));
        } else {
            seen.push(id);
        }
    }
}

impl Switches {
    pub fn parse(text: &str, file: &str, issues: &mut Vec<Issue>) -> Switches {
        let mut is = Issues::new(file);
        let blocks = parse(text, &mut is);
        let mut out = Switches::default();
        for b in &blocks {
            if b.name != "switch" {
                is.warn(b.line, format!("unknown block [{}]", b.name));
                continue;
            }
            let kind = match b.get("kind") {
                None => SwitchKind::Simple,
                Some(k) => SwitchKind::from_name(k).unwrap_or_else(|| {
                    is.error(b.line_of("kind"), format!("kind {k} is not simple, crossover, double_slip, diamond, scissors or wire"));
                    SwitchKind::Simple
                }),
            };
            let drive = match b.get("drive").map(|d| d.trim().to_ascii_lowercase()) {
                None => Drive::Auto,
                Some(d) if d == "auto" => Drive::Auto,
                Some(d) if d == "manual" => Drive::Manual,
                Some(d) if d == "interlocked" => Drive::Interlocked,
                Some(d) => {
                    is.error(b.line_of("drive"), format!("drive {d} is not auto, manual or interlocked"));
                    Drive::Auto
                }
            };
            let default_diverging = match b.get("default").map(|d| d.trim().to_ascii_lowercase()) {
                None => false,
                Some(d) if d == "straight" => false,
                Some(d) if d == "diverging" => true,
                Some(d) => {
                    is.error(b.line_of("default"), format!("default {d} is not straight or diverging"));
                    false
                }
            };
            let sw = Switch {
                id: b.str("id"),
                modes: modes_of(b, &mut is, true),
                kind,
                object: b.get("object").and_then(|v| v.trim().parse().ok()),
                pos: b.vec3("pos"),
                branches: b.i32("branches").unwrap_or(2).clamp(2, 8) as u8,
                default_diverging,
                drive,
                diverging_speed: b.speed("diverging_speed"),
                locked_by: b.get("locked_by").map(|s| s.trim().to_string()),
                tile: b.get("tile").and_then(|v| {
                    let n: Vec<i32> = v.split_whitespace().filter_map(|w| w.parse().ok()).collect();
                    (n.len() == 2).then(|| (n[0], n[1]))
                }),
                straight_path: b.i32("straight_path").unwrap_or(0).clamp(0, 255) as u16,
                diverging_paths: b.all("diverging_path").flat_map(|e| e.value.split_whitespace().filter_map(|w| w.parse().ok()).collect::<Vec<u16>>()).collect(),
                line: b.line,
            };
            if sw.diverging_speed.is_some() && sw.diverging_paths.is_empty() {
                is.warn(b.line, format!("switch {}: diverging_speed needs diverging_path (and tile) to say which branch it limits", sw.id));
            }
            if sw.object.is_none() && sw.pos.is_none() {
                is.error(b.line, format!("switch {} needs object or pos", sw.id));
            }
            out.list.push(sw);
        }
        id_check(&mut is, "switch", out.list.iter().map(|s| (s.id.as_str(), s.line)));
        issues.append(&mut is.list);
        out
    }

    pub fn get(&self, id: &str) -> Option<&Switch> {
        self.list.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }
}

impl Depots {
    pub fn parse(text: &str, file: &str, issues: &mut Vec<Issue>) -> Depots {
        let mut is = Issues::new(file);
        let blocks = parse(text, &mut is);
        let mut out = Depots::default();
        for b in &blocks {
            if b.name != "depot" {
                is.warn(b.line, format!("unknown block [{}]", b.name));
                continue;
            }
            let mut gates = Vec::new();
            for e in b.all("gate") {
                let n: Vec<f64> = e.value.split_whitespace().filter_map(|w| w.parse().ok()).collect();
                if n.len() == 3 {
                    gates.push([n[0], n[1], n[2]]);
                } else {
                    is.error(e.line, format!("a gate needs x y z, got `{}`", e.value));
                }
            }
            let id = b.str("id");
            out.list.push(Depot {
                name: b.get("name").map(str::to_string).unwrap_or_else(|| id.clone()),
                id,
                modes: modes_of(b, &mut is, true),
                capacity: b.i32("capacity").unwrap_or(0).max(0) as u32,
                ai_group: b.get("ai_group").map(|s| s.trim().to_string()),
                gates,
                station: b.get("station").map(|s| s.trim().to_string()),
                turning_loop: b.flag_or("turning_loop", false),
                line: b.line,
            });
        }
        id_check(&mut is, "depot", out.list.iter().map(|d| (d.id.as_str(), d.line)));
        for d in &out.list {
            if d.gates.is_empty() {
                is.warn(d.line, format!("depot {} has no gate: vehicles cannot enter or leave", d.id));
            }
        }
        issues.append(&mut is.list);
        out
    }

    pub fn ids(&self) -> Vec<String> {
        self.list.iter().map(|d| d.id.clone()).collect()
    }
}

impl Signals {
    /// The signal that guards block `id`.
    pub fn signal_of(&self, block: &str) -> Option<&Signal> {
        self.signals.iter().find(|s| s.block.as_deref().is_some_and(|b| b.eq_ignore_ascii_case(block)))
    }

    pub fn parse(text: &str, file: &str, issues: &mut Vec<Issue>) -> Signals {
        let mut is = Issues::new(file);
        let blocks = parse(text, &mut is);
        let mut out = Signals::default();
        for b in &blocks {
            match b.name.as_str() {
                "signal" => {
                    let kind = match b.get("kind") {
                        None => SignalKind::Main,
                        Some(k) => SignalKind::from_name(k).unwrap_or_else(|| {
                            is.error(b.line_of("kind"), format!("kind {k} is not main, distant, shunt, block, speed or tram"));
                            SignalKind::Main
                        }),
                    };
                    out.signals.push(Signal { id: b.str("id"), modes: modes_of(b, &mut is, true), kind, object: b.get("object").and_then(|v| v.trim().parse().ok()), block: b.get("block").map(|s| s.trim().to_string()), line: b.line });
                }
                "block" => out.blocks.push(Block_ { id: b.str("id"), modes: modes_of(b, &mut is, true), length_m: b.metres("length").unwrap_or(0.0), next: b.list("next", false), from: b.vec3("from"), to: b.vec3("to"), line: b.line }),
                other => is.warn(b.line, format!("unknown block [{other}]")),
            }
        }
        id_check(&mut is, "signal", out.signals.iter().map(|s| (s.id.as_str(), s.line)));
        id_check(&mut is, "block", out.blocks.iter().map(|s| (s.id.as_str(), s.line)));
        for s in &out.signals {
            if let Some(bl) = &s.block {
                if !out.blocks.iter().any(|x| x.id.eq_ignore_ascii_case(bl)) {
                    is.error(s.line, format!("signal {}: block {bl} does not exist", s.id));
                }
            }
        }
        for b in &out.blocks {
            if b.length_m <= 0.0 && (b.from.is_none() || b.to.is_none()) {
                is.error(b.line, format!("block {} has no length (give length, or from and to)", b.id));
            }
            if b.from.is_some() != b.to.is_some() {
                is.error(b.line, format!("block {}: give both from and to, or neither", b.id));
            }
            for n in &b.next {
                if !out.blocks.iter().any(|x| x.id.eq_ignore_ascii_case(n)) {
                    is.error(b.line, format!("block {}: next block {n} does not exist", b.id));
                }
            }
        }
        issues.append(&mut is.list);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_cfg::transport::TransportMode;

    #[test]
    fn globs() {
        assert!(glob("Splines/*/tram_*.sli", "splines\\MyMap\\Tram_a.sli"));
        assert!(!glob("tram_*.sli", "bus_a.sli"));
        assert!(glob("a?c", "abc"));
    }

    #[test]
    fn later_track_rules_win() {
        let text = "[track]\nfile rail_*.sli\nmodes railway\npower catenary\nspeed 120 km/h\n[track]\nfile rail_ks.sli\nmodes tram\nstreet yes\nspeed 40 km/h\n[track]\ntile 1 2\nid 9\ntunnel yes\n";
        let mut issues = Vec::new();
        let n = Network::parse(text, "network.cfg", &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        let f = n.facts("rail_ks.sli", (0, 0), 1);
        assert!(f.modes.contains(TransportMode::Tram) && !f.modes.contains(TransportMode::Railway));
        assert_eq!((f.street, f.power), (Some(true), Power::Catenary));
        assert!((f.speed.unwrap() - 40.0 / 3.6).abs() < 1e-3);
        assert_eq!(n.facts("rail_x.sli", (0, 0), 1).modes, ModeSet::of(TransportMode::Railway));
        assert_eq!(n.facts("x", (1, 2), 9).tunnel, Some(true));
        assert_eq!(n.facts("x", (1, 2), 8).tunnel, None);
    }

    #[test]
    fn switches_depots_signals() {
        let mut issues = Vec::new();
        let sw = Switches::parse("[switch]\nid W1\nmodes tram railway\nkind scissors\nobject 5\ntile 3 4\ndiverging_path 1\ndiverging_path 2\nbranches 3\ndefault diverging\ndrive interlocked\ndiverging_speed 25 km/h\nlocked_by S1\n", "switches.cfg", &mut issues);
        let d = Depots::parse("[depot]\nid D\nmodes tram\ncapacity 20\nai_group G\ngate 1 2 3\nturning_loop yes\n", "depots.cfg", &mut issues);
        let sg = Signals::parse("[signal]\nid S1\nmodes railway\nkind distant\nblock B1\n[block]\nid B1\nmodes railway\nlength 850 m\nnext B2\n[block]\nid B2\nmodes railway\nfrom 0 0 0\nto 1000 0 0\n", "signals.cfg", &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        let w = sw.get("w1").unwrap();
        assert_eq!((w.kind, w.drive, w.branches, w.default_diverging), (SwitchKind::Scissors, Drive::Interlocked, 3, true));
        assert_eq!((w.tile, w.diverging_paths.clone()), (Some((3, 4)), vec![1, 2]));
        assert!(d.list[0].turning_loop && d.list[0].gates.len() == 1);
        assert_eq!(sg.blocks[1].to, Some([1000.0, 0.0, 0.0]));
        assert_eq!(sg.signal_of("b1").map(|s| s.id.as_str()), Some("S1"));
        assert_eq!(sw.get("W1").unwrap().default_path(), Some(1));
        assert_eq!(sg.signals[0].kind, SignalKind::Distant);
    }

    #[test]
    fn mistakes_are_reported() {
        let mut issues = Vec::new();
        Switches::parse("[switch]\nid W\nkind weird\n[switch]\nid w\nmodes tram\nobject 1\n", "switches.cfg", &mut issues);
        Depots::parse("[depot]\nid D\nmodes tram\n", "depots.cfg", &mut issues);
        Signals::parse("[signal]\nid S\nmodes metro\nblock X\n[block]\nid B\nmodes metro\nnext Y\n", "signals.cfg", &mut issues);
        Network::parse("[track]\nmodes tram\n", "network.cfg", &mut issues);
        let all: Vec<String> = issues.iter().map(|i| i.to_string()).collect();
        for want in ["kind weird", "[switch] needs modes", "needs object or pos", "used twice", "has no gate", "block X does not exist", "has no length", "next block Y", "would match everything"] {
            assert!(all.iter().any(|t| t.contains(want)), "missing `{want}` in {all:#?}");
        }
    }
}
