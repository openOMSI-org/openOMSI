//! The overhead wire network: spans (a pair of wires drawn as one centre line with the wires
//! `spacing` apart), the sections they belong to (live or dead, with the feeder's
//! resistance), the nodes where spans meet - insulators between sections, ordinary joints and
//! switches (points) - and the walking of a shoe along the wires through all of it.
//!
//! The network is described in a map's `trolley.txt` (see [`WireNet::parse`]); OMSI 2 itself
//! knows no overhead wire. A span is a polyline in world coordinates (x east, y north, z up,
//! m) and has an orientation, from its first point to its last: the wire on the right of
//! that direction is the positive one, the left one the negative.

use glam::DVec3;
use hashbrown::HashMap;
use std::sync::Mutex;

/// Where a shoe's contact point lies: on this span, this far (m) from its first point, on the
/// wire of this side (0 right/positive, 1 left/negative, seen along the span).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    pub span: u32,
    pub s: f64,
    pub side: u8,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub id: i32,
    pub name: String,
    /// Fed from a substation.
    pub live: bool,
    /// The power the section's feeder takes back from a vehicle braking on it (kW); a
    /// substation without a reversible rectifier takes none.
    pub regen_kw: f32,
    /// The feeder's resistance up to the section (ohm, both wires together).
    pub feeder_ohm: f32,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub section: usize,
    pub pts: Vec<DVec3>,
    /// Length along the polyline up to each point.
    pub cum: Vec<f64>,
    pub spacing: f32,
    /// Node at the first and the last point.
    pub ends: [u32; 2],
    /// At each point the right-hand offset direction, mitred at the bends (its length is
    /// 1 / cos of half the bend), so that the wires run on without a jump at a bend.
    pub mit: Vec<DVec3>,
}

fn mitres(pts: &[DVec3]) -> Vec<DVec3> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let a = if i > 0 { right_of(pts[i] - pts[i - 1]) } else { right_of(pts[1] - pts[0]) };
            let b = if i + 1 < n { right_of(pts[i + 1] - pts[i]) } else { a };
            let m = (a + b).normalize_or(a);
            let c = m.dot(a).max(0.5);
            m / c
        })
        .collect()
}

impl Span {
    pub fn len(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }

    /// Index of the segment holding length `s`.
    fn segment(&self, s: f64) -> usize {
        let i = self.cum.partition_point(|c| *c <= s);
        i.saturating_sub(1).min(self.pts.len().saturating_sub(2))
    }

    /// The centre line's point and unit tangent (3-D) at length `s`.
    pub fn at(&self, s: f64) -> (DVec3, DVec3) {
        let i = self.segment(s);
        let (a, b) = (self.pts[i], self.pts[i + 1]);
        let seg = (self.cum[i + 1] - self.cum[i]).max(1e-9);
        let t = ((s - self.cum[i]) / seg).clamp(0.0, 1.0);
        (a.lerp(b, t), (b - a).normalize_or(DVec3::X))
    }

    /// The point of the wire of `side` (0 right, 1 left) at length `s`, and the tangent.
    pub fn wire_at(&self, s: f64, side: u8) -> (DVec3, DVec3) {
        let i = self.segment(s);
        let seg = (self.cum[i + 1] - self.cum[i]).max(1e-9);
        let t = ((s - self.cum[i]) / seg).clamp(0.0, 1.0);
        let h = self.spacing as f64 * 0.5 * if side == 0 { 1.0 } else { -1.0 };
        let a = self.pts[i] + self.mit[i] * h;
        let b = self.pts[i + 1] + self.mit[i + 1] * h;
        (a.lerp(b, t), (self.pts[i + 1] - self.pts[i]).normalize_or(DVec3::X))
    }

    /// The end points of segment `i` of the wire of `side`.
    pub fn wire_segment(&self, i: usize, side: u8) -> (DVec3, DVec3) {
        let h = self.spacing as f64 * 0.5 * if side == 0 { 1.0 } else { -1.0 };
        (self.pts[i] + self.mit[i] * h, self.pts[i + 1] + self.mit[i + 1] * h)
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub pos: DVec3,
    /// The spans' ends that meet here: (span, 0 first point / 1 last point).
    pub ends: Vec<(u32, u8)>,
    /// A dead stretch (m) around the node where two sections meet.
    pub insulator: f32,
    pub switch: Option<u32>,
}

/// What sets a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchMode {
    /// The current the vehicle draws as its shoe passes the contact plate before the frog:
    /// power on takes the branch, coasting the straight way (`invert` the other way round).
    Power,
    /// The vehicle's indicator on the side of the branch takes the branch, otherwise
    /// straight.
    Indicator,
    /// Stays as it is (the network's script or a test sets it).
    Fixed,
}

#[derive(Debug, Clone)]
pub struct SwitchDef {
    pub name: String,
    pub node: u32,
    /// The ends at the frog: the one the vehicles arrive at, and the two they leave by.
    pub toe: (u32, u8),
    pub straight: (u32, u8),
    pub branch: (u32, u8),
    pub mode: SwitchMode,
    /// The contact plate lies this far before the frog on the toe span (m).
    pub trigger: f32,
    /// Where the contact plate lies: a span, the length along it, and the end of that span the
    /// frog lies towards. It is `trigger` metres before the frog along the way in, over short
    /// spans and plain joints too (a junction cut into short spans has its plate on the span
    /// before the toe).
    pub plate: (u32, f64, u8),
    /// The current that counts as power on (A).
    pub sense_amps: f32,
    pub invert: bool,
    /// The branch leaves to the right, seen from the toe.
    pub branch_right: bool,
    pub initial_branch: bool,
}

#[derive(Debug, Clone)]
struct Dyn {
    live: Vec<bool>,
    /// Current each section carries now (A) and what vehicles add up to for the next step.
    current: Vec<f32>,
    accum: Vec<f32>,
    branch: Vec<bool>,
    /// A switch holds its position this long after it was set (s).
    lock: Vec<f32>,
}

/// The network. Everything but the sections' power and the switches' positions is fixed once
/// it is built.
pub struct WireNet {
    pub nominal_volts: f32,
    /// How uneven the contact wire is (m): wear, ice, dirt. The shoe leaves it where it rises
    /// faster than the pole can follow.
    pub roughness: f32,
    /// The wire was read off splines the map draws itself: the game draws no wire of its own.
    pub drawn_by_map: bool,
    pub sections: Vec<Section>,
    pub spans: Vec<Span>,
    pub nodes: Vec<Node>,
    pub switches: Vec<SwitchDef>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    dynamic: Mutex<Dyn>,
}

impl std::fmt::Debug for WireNet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WireNet({} spans, {} nodes, {} switches)", self.spans.len(), self.nodes.len(), self.switches.len())
    }
}

const CELL: f64 = 32.0;
/// Ends closer than this are one node (m). Map authors draw the three wires of a junction
/// up to a metre apart; the wire's own joints are exact.
pub const JOIN: f64 = 1.2;
/// The default dead stretch where two sections meet (m).
pub const DEFAULT_INSULATOR: f32 = 0.8;

fn cell_of(p: DVec3) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

fn horizontal(v: DVec3) -> DVec3 {
    DVec3::new(v.x, v.y, 0.0)
}

/// The right-hand side of a tangent, horizontal and unit.
pub fn right_of(t: DVec3) -> DVec3 {
    DVec3::new(t.y, -t.x, 0.0).normalize_or(DVec3::X)
}

/// The numbers after the first word of a line.
fn numbers(l: &str) -> Vec<f64> {
    l.split_whitespace().skip(1).filter_map(|w| w.replace(',', ".").parse::<f64>().ok()).collect()
}

fn words(l: &str) -> Vec<&str> {
    l.split_whitespace().collect()
}

struct RawSwitch {
    name: String,
    pos: DVec3,
    mode: SwitchMode,
    trigger: f32,
    sense: f32,
    invert: bool,
    initial_branch: bool,
}

impl WireNet {
    /// Read `trolley.txt`:
    ///
    /// ```text
    /// [trolley_net]
    /// voltage 600            nominal line voltage (V)
    /// spacing 0.6            default distance between the two wires (m)
    /// roughness 0.0012      unevenness of the contact wire (m)
    ///
    /// [trolley_section]
    /// id 1
    /// name Depot
    /// live 1                 0 = dead (no substation feeds it)
    /// regen_kw 0             power the feeder takes back from a braking vehicle
    /// feeder_ohm 0.05
    ///
    /// [trolley_span]
    /// section 1
    /// spacing 0.6            (optional)
    /// p 100.0 200.0 5.8      the points of the centre line, each a line, in the direction
    /// p 112.0 200.0 5.7      the wire's positive polarity is on the right of
    ///
    /// [trolley_switch]       at a joint of three spans
    /// pos 140 200 5.7
    /// mode power             power | indicator | fixed
    /// trigger 9              plate this far before the frog (m)
    /// sense 12               current counted as power on (A)
    /// invert 0
    /// initial straight
    ///
    /// [trolley_insulator]    at a joint of two spans
    /// pos 160 200 5.7
    /// length 1.0
    /// ```
    pub fn parse(text: &str) -> WireNet {
        let mut nominal = 600.0f32;
        let mut spacing = 0.6f32;
        let mut roughness = 0.0012f32;
        let mut sections: Vec<Section> = Vec::new();
        struct RawSpan {
            section: i32,
            spacing: Option<f32>,
            pts: Vec<DVec3>,
        }
        let mut spans: Vec<RawSpan> = Vec::new();
        let mut switches: Vec<RawSwitch> = Vec::new();
        let mut insulators: Vec<(DVec3, f32)> = Vec::new();
        let mut block = String::new();
        let mut sec: Option<Section> = None;
        let mut insulator: Option<(DVec3, f32)> = None;
        let flush = |block: &str, sec: &mut Option<Section>, ins: &mut Option<(DVec3, f32)>, sections: &mut Vec<Section>, insulators: &mut Vec<(DVec3, f32)>| {
            if block == "trolley_section" {
                if let Some(s) = sec.take() {
                    sections.push(s);
                }
            }
            if block == "trolley_insulator" {
                if let Some(i) = ins.take() {
                    insulators.push(i);
                }
            }
        };
        for raw in text.lines() {
            let l = raw.split("//").next().unwrap_or("").trim();
            if l.is_empty() {
                continue;
            }
            if let Some(k) = l.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                flush(&block, &mut sec, &mut insulator, &mut sections, &mut insulators);
                block = k.trim().to_ascii_lowercase();
                match block.as_str() {
                    "trolley_section" => sec = Some(Section { id: sections.len() as i32 + 1, name: String::new(), live: true, regen_kw: 0.0, feeder_ohm: 0.05 }),
                    "trolley_span" => spans.push(RawSpan { section: 0, spacing: None, pts: Vec::new() }),
                    "trolley_switch" => switches.push(RawSwitch { name: String::new(), pos: DVec3::ZERO, mode: SwitchMode::Power, trigger: 9.0, sense: 12.0, invert: false, initial_branch: false }),
                    "trolley_insulator" => insulator = Some((DVec3::ZERO, DEFAULT_INSULATOR)),
                    _ => {}
                }
                continue;
            }
            let w = words(l);
            let key = w[0].to_ascii_lowercase();
            let n = numbers(l);
            let first = w.get(1).copied().unwrap_or("");
            match block.as_str() {
                "trolley_net" => match key.as_str() {
                    "voltage" => nominal = n.first().copied().unwrap_or(600.0).clamp(100.0, 3000.0) as f32,
                    "spacing" => spacing = n.first().copied().unwrap_or(0.6).clamp(0.2, 2.0) as f32,
                    "roughness" => roughness = n.first().copied().unwrap_or(0.0012).clamp(0.0, 0.05) as f32,
                    _ => {}
                },
                "trolley_section" => {
                    if let Some(s) = sec.as_mut() {
                        match key.as_str() {
                            "id" => s.id = n.first().copied().unwrap_or(0.0) as i32,
                            "name" => s.name = l[w[0].len()..].trim().to_string(),
                            "live" => s.live = !matches!(first, "0" | "false" | "no" | "off"),
                            "regen_kw" => s.regen_kw = n.first().copied().unwrap_or(0.0).max(0.0) as f32,
                            "feeder_ohm" => s.feeder_ohm = n.first().copied().unwrap_or(0.05).clamp(0.0, 5.0) as f32,
                            _ => {}
                        }
                    }
                }
                "trolley_span" => {
                    if let Some(s) = spans.last_mut() {
                        match key.as_str() {
                            "section" => s.section = n.first().copied().unwrap_or(0.0) as i32,
                            "spacing" => s.spacing = n.first().map(|x| x.clamp(0.2, 2.0) as f32),
                            "p" if n.len() >= 3 => s.pts.push(DVec3::new(n[0], n[1], n[2])),
                            _ => {}
                        }
                    }
                }
                "trolley_switch" => {
                    if let Some(s) = switches.last_mut() {
                        match key.as_str() {
                            "name" => s.name = l[w[0].len()..].trim().to_string(),
                            "pos" if n.len() >= 3 => s.pos = DVec3::new(n[0], n[1], n[2]),
                            "mode" => {
                                s.mode = match first.to_ascii_lowercase().as_str() {
                                    "indicator" | "blinker" | "radio" => SwitchMode::Indicator,
                                    "fixed" | "manual" => SwitchMode::Fixed,
                                    _ => SwitchMode::Power,
                                }
                            }
                            "trigger" => s.trigger = n.first().copied().unwrap_or(9.0).clamp(0.5, 60.0) as f32,
                            "sense" => s.sense = n.first().copied().unwrap_or(12.0).max(0.0) as f32,
                            "invert" => s.invert = !matches!(first, "0" | "false" | "no" | "off"),
                            "initial" => s.initial_branch = first.eq_ignore_ascii_case("branch"),
                            _ => {}
                        }
                    }
                }
                "trolley_insulator" => {
                    if let Some(i) = insulator.as_mut() {
                        match key.as_str() {
                            "pos" if n.len() >= 3 => i.0 = DVec3::new(n[0], n[1], n[2]),
                            "length" => i.1 = n.first().copied().unwrap_or(0.8).clamp(0.0, 20.0) as f32,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        flush(&block, &mut sec, &mut insulator, &mut sections, &mut insulators);
        if sections.is_empty() {
            sections.push(Section { id: 0, name: "all".into(), live: true, regen_kw: 0.0, feeder_ohm: 0.05 });
        }
        let mut net = WireNet {
            nominal_volts: nominal,
            roughness,
            drawn_by_map: false,
            sections,
            spans: Vec::new(),
            nodes: Vec::new(),
            switches: Vec::new(),
            grid: HashMap::new(),
            dynamic: Mutex::new(Dyn { live: Vec::new(), current: Vec::new(), accum: Vec::new(), branch: Vec::new(), lock: Vec::new() }),
        };
        for r in spans {
            if r.pts.len() < 2 {
                continue;
            }
            let mut pts: Vec<DVec3> = Vec::with_capacity(r.pts.len());
            for p in r.pts {
                if pts.last().is_none_or(|q| q.distance(p) > 1e-3) {
                    pts.push(p);
                }
            }
            if pts.len() < 2 {
                continue;
            }
            // a piece shorter than the joining distance would join its own two ends and
            // collapse; the wire closes over it as it is (its neighbours' ends are within JOIN)
            let polyline: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            if pts[0].distance(*pts.last().unwrap()) <= JOIN && polyline < 2.0 * JOIN {
                continue;
            }
            let mut cum = vec![0.0];
            for w in pts.windows(2) {
                cum.push(cum.last().unwrap() + w[0].distance(w[1]));
            }
            let section = net.sections.iter().position(|s| s.id == r.section).unwrap_or(0);
            let mit = mitres(&pts);
            net.spans.push(Span { section, pts, cum, spacing: r.spacing.unwrap_or(spacing), ends: [0, 0], mit });
        }
        net.build_nodes(&switches, &insulators);
        net.build_grid();
        let d = Dyn {
            live: net.sections.iter().map(|s| s.live).collect(),
            current: vec![0.0; net.sections.len()],
            accum: vec![0.0; net.sections.len()],
            branch: net.switches.iter().map(|s| s.initial_branch).collect(),
            lock: vec![0.0; net.switches.len()],
        };
        *net.dynamic.lock().unwrap() = d;
        net
    }

    fn build_nodes(&mut self, raw_switches: &[RawSwitch], insulators: &[(DVec3, f32)]) {
        // ends joined by position
        let mut nodes: Vec<Node> = Vec::new();
        // (a grid of the nodes so far, cells of the joining distance: a map's wire has thousands
        // of span ends, and a search through all of them for each was a pause at the spawn)
        let cell = |p: DVec3| ((p.x / JOIN).floor() as i64, (p.y / JOIN).floor() as i64);
        let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
        for si in 0..self.spans.len() {
            for end in 0..2u8 {
                let p = if end == 0 { self.spans[si].pts[0] } else { *self.spans[si].pts.last().unwrap() };
                let (cx, cy) = cell(p);
                let found = (-1..=1)
                    .flat_map(|dx| (-1..=1).map(move |dy| (cx + dx, cy + dy)))
                    .filter_map(|c| grid.get(&c))
                    .flatten()
                    .copied()
                    .filter(|i| nodes[*i].pos.distance(p) <= JOIN)
                    // (a span's last end never takes the node of its own first end: a short or
                    // curled piece would close on itself and collapse)
                    .filter(|i| !(end == 1 && nodes[*i].ends.contains(&(si as u32, 0))))
                    .min();
                let ni = found.unwrap_or_else(|| {
                    nodes.push(Node { pos: p, ends: Vec::new(), insulator: 0.0, switch: None });
                    grid.entry((cx, cy)).or_default().push(nodes.len() - 1);
                    nodes.len() - 1
                });
                nodes[ni].ends.push((si as u32, end));
                self.spans[si].ends[end as usize] = ni as u32;
            }
        }
        // ends drawn up to JOIN apart meet in one point, so a shoe does not jump as it crosses
        let mut touched: Vec<usize> = Vec::new();
        for n in nodes.iter_mut() {
            if n.ends.len() < 2 {
                continue;
            }
            let pt = |e: &(u32, u8)| {
                let s = &self.spans[e.0 as usize];
                if e.1 == 0 { s.pts[0] } else { *s.pts.last().unwrap() }
            };
            let c = n.ends.iter().map(pt).sum::<DVec3>() / n.ends.len() as f64;
            n.pos = c;
            for (si, end) in &n.ends {
                let s = &mut self.spans[*si as usize];
                if *end == 0 { s.pts[0] = c } else { *s.pts.last_mut().unwrap() = c }
                touched.push(*si as usize);
            }
        }
        touched.sort_unstable();
        touched.dedup();
        for si in touched {
            let s = &mut self.spans[si];
            s.cum = std::iter::once(0.0).chain(s.pts.windows(2).scan(0.0, |a, w| { *a += w[0].distance(w[1]); Some(*a) })).collect();
            s.mit = mitres(&s.pts);
        }
        // where two sections meet there is a dead stretch, unless an insulator says how long
        for n in nodes.iter_mut() {
            let first = n.ends.first().map(|e| self.spans[e.0 as usize].section);
            if n.ends.iter().any(|e| Some(self.spans[e.0 as usize].section) != first) {
                n.insulator = DEFAULT_INSULATOR;
            }
        }
        for (pos, len) in insulators {
            if let Some(n) = nodes.iter_mut().filter(|n| n.pos.distance(*pos) < 2.0).min_by(|a, b| a.pos.distance(*pos).total_cmp(&b.pos.distance(*pos))) {
                n.insulator = *len;
            }
        }
        self.nodes = nodes;
        self.share_offsets_at_nodes();
        for r in raw_switches {
            let Some(ni) = (0..self.nodes.len()).filter(|i| self.nodes[*i].ends.len() == 3 && self.nodes[*i].pos.distance(r.pos) < 3.0).min_by(|a, b| self.nodes[*a].pos.distance(r.pos).total_cmp(&self.nodes[*b].pos.distance(r.pos))) else {
                log::warn!("trolley switch '{}' at ({:.1}, {:.1}) has no joint of three spans", r.name, r.pos.x, r.pos.y);
                continue;
            };
            let ends = self.nodes[ni].ends.clone();
            let dirs: Vec<DVec3> = ends.iter().map(|e| self.end_dir(*e)).collect();
            // the toe points away from the other two
            let toe_i = (0..3)
                .min_by(|a, b| {
                    let score = |i: usize| (0..3).filter(|j| *j != i).map(|j| dirs[i].dot(dirs[j])).sum::<f64>();
                    score(*a).total_cmp(&score(*b))
                })
                .unwrap();
            let others: Vec<usize> = (0..3).filter(|i| *i != toe_i).collect();
            let fwd = -dirs[toe_i];
            let (s_i, b_i) = if dirs[others[0]].dot(fwd) >= dirs[others[1]].dot(fwd) { (others[0], others[1]) } else { (others[1], others[0]) };
            let branch_right = fwd.x * dirs[b_i].y - fwd.y * dirs[b_i].x < 0.0;
            let sw = SwitchDef {
                name: r.name.clone(),
                node: ni as u32,
                toe: ends[toe_i],
                straight: ends[s_i],
                branch: ends[b_i],
                mode: r.mode,
                trigger: r.trigger,
                plate: self.plate_for(ends[toe_i], r.trigger as f64),
                sense_amps: r.sense,
                invert: r.invert,
                branch_right,
                initial_branch: r.initial_branch,
            };
            self.nodes[ni].switch = Some(self.switches.len() as u32);
            self.switches.push(sw);
        }
    }

    /// The contact plate `trigger` metres before a frog, walking back from the toe over plain
    /// joints (two spans meeting). Where the way back ends first, the plate lies just inside
    /// the last span's far end.
    fn plate_for(&self, toe: (u32, u8), trigger: f64) -> (u32, f64, u8) {
        let mut near = toe;
        let mut left = trigger;
        for _ in 0..64 {
            let sp = &self.spans[near.0 as usize];
            let len = sp.len();
            if left <= len {
                return (near.0, if near.1 == 0 { left } else { len - left }, near.1);
            }
            left -= len;
            let far = 1 - near.1;
            let n = &self.nodes[sp.ends[far as usize] as usize];
            match n.ends.as_slice() {
                [a, b] => {
                    let next = if *a == (near.0, far) { *b } else { *a };
                    near = next;
                }
                _ => {
                    let inset = 0.3f64.min(len * 0.5);
                    return (near.0, if far == 0 { inset } else { len - inset }, near.1);
                }
            }
        }
        (near.0, 0.0, near.1)
    }

    /// Where spans meet, the wires of all of them leave the joint from the same place: the
    /// side a span's wire lies on at its end is the one the joint's common direction says (the
    /// first span's), whatever way round the span is drawn. Without it a switch's wires would
    /// jump by the difference of their offsets as a shoe crosses the joint.
    fn share_offsets_at_nodes(&mut self) {
        for ni in 0..self.nodes.len() {
            let ends = self.nodes[ni].ends.clone();
            if ends.len() < 2 {
                continue;
            }
            // (three spans: a switch, and the way from its toe)
            let reference = if ends.len() == 3 { self.toe_of(&ends) } else { ends[0] };
            let shared = right_of(-self.end_dir(reference));
            for (si, end) in ends {
                let s = &mut self.spans[si as usize];
                let n = s.pts.len();
                let (idx, own) = if end == 0 { (0, right_of(s.pts[1] - s.pts[0])) } else { (n - 1, right_of(s.pts[n - 1] - s.pts[n - 2])) };
                s.mit[idx] = if own.dot(shared) >= 0.0 { shared } else { -shared };
            }
        }
    }

    /// The end that points away from the others: the toe of a switch.
    fn toe_of(&self, ends: &[(u32, u8)]) -> (u32, u8) {
        let dirs: Vec<DVec3> = ends.iter().map(|e| self.end_dir(*e)).collect();
        let toe = (0..ends.len())
            .min_by(|a, b| {
                let score = |i: usize| (0..dirs.len()).filter(|j| *j != i).map(|j| dirs[i].dot(dirs[j])).sum::<f64>();
                score(*a).total_cmp(&score(*b))
            })
            .unwrap_or(0);
        ends[toe]
    }

    fn build_grid(&mut self) {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (si, s) in self.spans.iter().enumerate() {
            for w in s.pts.windows(2) {
                let (c0, c1) = (cell_of(w[0].min(w[1])), cell_of(w[0].max(w[1])));
                for cx in c0.0..=c1.0 {
                    for cy in c0.1..=c1.1 {
                        let v = grid.entry((cx, cy)).or_default();
                        if v.last() != Some(&(si as u32)) && !v.contains(&(si as u32)) {
                            v.push(si as u32);
                        }
                    }
                }
            }
        }
        self.grid = grid;
    }

    /// Unit vector (horizontal) from the node into the span, at an end.
    pub fn end_dir(&self, e: (u32, u8)) -> DVec3 {
        let s = &self.spans[e.0 as usize];
        let n = s.pts.len();
        let d = if e.1 == 0 { s.pts[1] - s.pts[0] } else { s.pts[n - 2] - s.pts[n - 1] };
        horizontal(d).normalize_or(DVec3::X)
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// The spans with a point within `radius` of `p` (horizontally, up to the grid's cell).
    pub fn near(&self, p: DVec3, radius: f64) -> Vec<u32> {
        let (c0, c1) = (cell_of(p - DVec3::splat(radius)), cell_of(p + DVec3::splat(radius)));
        let mut out: Vec<u32> = Vec::new();
        for cx in c0.0..=c1.0 {
            for cy in c0.1..=c1.1 {
                if let Some(v) = self.grid.get(&(cx, cy)) {
                    for s in v {
                        if !out.contains(s) {
                            out.push(*s);
                        }
                    }
                }
            }
        }
        out
    }

    /// The position of a wire of a contact and the span's tangent there.
    pub fn wire_point(&self, c: &Contact) -> (DVec3, DVec3) {
        self.spans[c.span as usize].wire_at(c.s, c.side)
    }

    /// The centre line point at a contact.
    pub fn centre_point(&self, c: &Contact) -> DVec3 {
        self.spans[c.span as usize].at(c.s).0
    }

    // ---- power ----

    /// Whether the wire at a contact is fed: its section is live and the point is not in the
    /// dead stretch around an insulator.
    pub fn is_live(&self, c: &Contact) -> bool {
        let sp = &self.spans[c.span as usize];
        let d = self.dynamic.lock().unwrap();
        d.live.get(sp.section).copied().unwrap_or(true) && !self.in_gap(c)
    }

    pub fn in_gap(&self, c: &Contact) -> bool {
        let sp = &self.spans[c.span as usize];
        let len = sp.len();
        let g0 = self.nodes[sp.ends[0] as usize].insulator as f64 * 0.5;
        let g1 = self.nodes[sp.ends[1] as usize].insulator as f64 * 0.5;
        c.s < g0 || c.s > len - g1
    }

    pub fn section_of(&self, c: &Contact) -> usize {
        self.spans[c.span as usize].section
    }

    pub fn set_section_live(&self, section: usize, live: bool) {
        if let Some(l) = self.dynamic.lock().unwrap().live.get_mut(section) {
            *l = live;
        }
    }

    pub fn section_live(&self, section: usize) -> bool {
        self.dynamic.lock().unwrap().live.get(section).copied().unwrap_or(false)
    }

    /// Section by its id in the file.
    pub fn section_index(&self, id: i32) -> Option<usize> {
        self.sections.iter().position(|s| s.id == id)
    }

    /// A vehicle draws `amps` from a section for this step (negative when it feeds back).
    pub fn add_current(&self, section: usize, amps: f32) {
        if let Some(a) = self.dynamic.lock().unwrap().accum.get_mut(section) {
            *a += amps;
        }
    }

    /// The step's currents become the ones the voltage drop is worked out with.
    pub fn end_step(&self, dt: f32) {
        let mut d = self.dynamic.lock().unwrap();
        let Dyn { current, accum, lock, .. } = &mut *d;
        for (c, a) in current.iter_mut().zip(accum.iter_mut()) {
            *c = *a;
            *a = 0.0;
        }
        for l in lock.iter_mut() {
            *l = (*l - dt).max(0.0);
        }
    }

    /// The voltage a vehicle finds on a section drawing `own` amps itself (the current other
    /// vehicles draw drops the voltage too): the substation's nominal less the feeder's drop.
    pub fn section_volts(&self, section: usize, own: f32) -> f32 {
        let d = self.dynamic.lock().unwrap();
        if !d.live.get(section).copied().unwrap_or(false) {
            return 0.0;
        }
        let other = d.current.get(section).copied().unwrap_or(0.0);
        let r = self.sections.get(section).map_or(0.05, |s| s.feeder_ohm);
        (self.nominal_volts - (other + own) * r).max(0.0)
    }

    /// How much power the section's feeder takes back (kW).
    pub fn regen_capacity_kw(&self, section: usize) -> f32 {
        let d = self.dynamic.lock().unwrap();
        if !d.live.get(section).copied().unwrap_or(false) {
            return 0.0;
        }
        let drawn = d.current.get(section).copied().unwrap_or(0.0).max(0.0) * self.nominal_volts * 1e-3;
        self.sections.get(section).map_or(0.0, |s| s.regen_kw) + drawn
    }

    // ---- switches ----

    pub fn switch_branch(&self, sw: usize) -> bool {
        self.dynamic.lock().unwrap().branch.get(sw).copied().unwrap_or(false)
    }

    pub fn set_switch(&self, sw: usize, branch: bool) {
        if let Some(b) = self.dynamic.lock().unwrap().branch.get_mut(sw) {
            *b = branch;
        }
    }

    /// A shoe passes the contact plate of a switch: the position the vehicle asks for, if the
    /// switch is free. Returns whether it changed.
    pub fn trigger_switch(&self, sw: usize, current: f32, blinker: i8) -> bool {
        let def = &self.switches[sw];
        let want = match def.mode {
            SwitchMode::Fixed => return false,
            SwitchMode::Power => (current.abs() > def.sense_amps) != def.invert,
            SwitchMode::Indicator => (blinker > 0 && def.branch_right) || (blinker < 0 && !def.branch_right),
        };
        let mut d = self.dynamic.lock().unwrap();
        if d.lock[sw] > 0.0 {
            return false;
        }
        d.lock[sw] = 2.5;
        let was = d.branch[sw];
        d.branch[sw] = want;
        was != want
    }

    /// Where a shoe arriving at `from` of a node leaves it.
    fn exit_at(&self, node: u32, from: (u32, u8)) -> Option<(u32, u8)> {
        let n = &self.nodes[node as usize];
        if let Some(si) = n.switch {
            let sw = &self.switches[si as usize];
            if from == sw.toe {
                return Some(if self.switch_branch(si as usize) { sw.branch } else { sw.straight });
            }
            return Some(sw.toe);
        }
        let others: Vec<(u32, u8)> = n.ends.iter().copied().filter(|e| *e != from).collect();
        match others.len() {
            0 => None,
            1 => Some(others[0]),
            _ => {
                // a crossing of lines: the shoe goes straight on
                let d_in = -self.end_dir(from);
                others.into_iter().max_by(|a, b| self.end_dir(*a).dot(d_in).total_cmp(&self.end_dir(*b).dot(d_in)))
            }
        }
    }

    /// The largest horizontal turn (radians) at the polyline's points between lengths `a` and `b`.
    fn kink_between(&self, span: u32, a: f64, b: f64) -> f64 {
        let sp = &self.spans[span as usize];
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let mut worst: f64 = 0.0;
        for i in 1..sp.pts.len() - 1 {
            if sp.cum[i] > lo && sp.cum[i] < hi {
                let t0 = horizontal(sp.pts[i] - sp.pts[i - 1]).normalize_or(DVec3::X);
                let t1 = horizontal(sp.pts[i + 1] - sp.pts[i]).normalize_or(DVec3::X);
                worst = worst.max(t0.dot(t1).clamp(-1.0, 1.0).acos());
            }
        }
        worst
    }

    /// Move a contact `dist` metres (>= 0) along the wire in direction `dir` (+1 towards the
    /// span's last point), through nodes. `info` collects what is passed on the way.
    pub fn walk(&self, c: &mut Contact, dir: &mut i8, mut dist: f64, info: &mut WalkInfo) -> Result<(), WalkEnd> {
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > 64 {
                return Err(WalkEnd::DeadEnd);
            }
            let len = self.spans[c.span as usize].len();
            let target = c.s + *dir as f64 * dist;
            if (0.0..=len).contains(&target) {
                info.kink = info.kink.max(self.kink_between(c.span, c.s, target));
                c.s = target;
                return Ok(());
            }
            let end: u8 = if *dir > 0 { 1 } else { 0 };
            let travelled = if end == 1 { len - c.s } else { c.s };
            info.kink = info.kink.max(self.kink_between(c.span, c.s, if end == 1 { len } else { 0.0 }));
            dist = (dist - travelled).max(0.0);
            let node = self.spans[c.span as usize].ends[end as usize];
            let Some((ns, ne)) = self.exit_at(node, (c.span, end)) else {
                c.s = if end == 1 { len } else { 0.0 };
                return Err(WalkEnd::DeadEnd);
            };
            // the turn at the joint
            let d_in = -self.end_dir((c.span, end));
            let d_out = self.end_dir((ns, ne));
            info.kink = info.kink.max(d_in.dot(d_out).clamp(-1.0, 1.0).acos());
            info.nodes.push(NodePass { node, from: (c.span, end), to: (ns, ne) });
            if end == ne {
                c.side ^= 1;
            }
            c.span = ns;
            c.s = if ne == 0 { 0.0 } else { self.spans[ns as usize].len() };
            *dir = if ne == 0 { 1 } else { -1 };
        }
    }

    /// Put `c` where the tip of a boom of length `len` hinged at `base` touches the wire:
    /// the contact point moves along the wire (through joints and switches, as the switches
    /// stand) until it lies `len` from the hinge. Returns what was passed, or why the wire
    /// cannot be reached.
    pub fn solve_tip(&self, c: &mut Contact, base: DVec3, len: f64) -> Result<WalkInfo, SolveErr> {
        let mut info = WalkInfo::default();
        for _ in 0..16 {
            let (p, t) = self.wire_point(c);
            let r = p - base;
            let dist = r.length().max(1e-6);
            let f = dist - len;
            if f.abs() < 2e-6 {
                return Ok(info);
            }
            // how fast the distance grows along the wire: the wire at right angles to the
            // radius cannot be followed
            let mut g = t.dot(r) / dist;
            if g.abs() < 0.05 {
                g = if g < 0.0 { -0.05 } else { 0.05 };
            }
            let step = (-f / g).clamp(-3.0, 3.0);
            let mut dir: i8 = if step >= 0.0 { 1 } else { -1 };
            if self.walk(c, &mut dir, step.abs(), &mut info).is_err() {
                return Err(SolveErr::EndOfWire);
            }
        }
        let (p, _) = self.wire_point(c);
        if (p - base).length() > len + 0.05 {
            Err(SolveErr::OutOfReach)
        } else {
            Ok(info)
        }
    }


    /// The wire a ray (a player's line of sight) passes nearest within `max_miss` metres,
    /// among the wires within `reach` of a boom's hinge: its contact, its point and the miss.
    pub fn aim_wire(&self, origin: DVec3, dir: DVec3, base: DVec3, reach: f64, side_for: &dyn Fn(DVec3) -> Option<u8>, max_miss: f64) -> Option<(Contact, DVec3, f64)> {
        let dir = dir.normalize_or(DVec3::X);
        let mut best: Option<(Contact, DVec3, f64)> = None;
        for si in self.near(base, reach + 3.0) {
            let sp = &self.spans[si as usize];
            for sd in 0..2u8 {
                for i in 0..sp.pts.len() - 1 {
                    let t = (sp.pts[i + 1] - sp.pts[i]).normalize_or(DVec3::X);
                    if side_for(t).is_some_and(|s| s != sd) {
                        continue;
                    }
                    let (a, b) = sp.wire_segment(i, sd);
                    let (u, miss) = ray_segment(origin, dir, a, b);
                    let p = a.lerp(b, u);
                    if miss > max_miss || p.distance(base) > reach + 0.3 {
                        continue;
                    }
                    if best.as_ref().is_none_or(|x| miss < x.2) {
                        let s = sp.cum[i] + u * (sp.cum[i + 1] - sp.cum[i]);
                        best = Some((Contact { span: si, s, side: sd }, p, miss));
                    }
                }
            }
        }
        best
    }

    /// Move a contact that lies ahead of a boom's hinge (a wire aimed at above the bus's own
    /// front half) back along the wire to behind it, where a trailing boom reaches the wire.
    pub fn place_behind(&self, c: &mut Contact, base: DVec3, fwd: DVec3, len: f64) {
        let (p, t) = self.wire_point(c);
        let ahead = (p - base).dot(horizontal(fwd).normalize_or(DVec3::Y));
        if ahead <= -0.5 {
            return;
        }
        let mut dir: i8 = if t.dot(fwd) > 0.0 { -1 } else { 1 };
        let mut info = WalkInfo::default();
        let _ = self.walk(c, &mut dir, ahead + len * 0.9, &mut info);
    }

    /// Every wire close to a point, for a free tip looking for a wire to take: the contact on
    /// each wire nearest the point.
    pub fn candidates(&self, q: DVec3, radius: f64) -> Vec<(Contact, f64)> {
        let mut out = Vec::new();
        for si in self.near(q, radius) {
            let sp = &self.spans[si as usize];
            // (a span has up to six hundred points and this is asked of a free pole every
            // step: the segments far from the point are passed over by their ends alone)
            let m = radius + sp.spacing as f64 + 0.5;
            for side in 0..2u8 {
                let mut best: Option<(f64, f64)> = None;
                for i in 0..sp.pts.len() - 1 {
                    let (p0, p1) = (sp.pts[i], sp.pts[i + 1]);
                    if q.x < p0.x.min(p1.x) - m || q.x > p0.x.max(p1.x) + m || q.y < p0.y.min(p1.y) - m || q.y > p0.y.max(p1.y) + m {
                        continue;
                    }
                    let (a, b) = sp.wire_segment(i, side);
                    let ab = b - a;
                    let l2 = ab.length_squared().max(1e-12);
                    let u = ((q - a).dot(ab) / l2).clamp(0.0, 1.0);
                    let d = (a + ab * u).distance(q);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, sp.cum[i] + u * (sp.cum[i + 1] - sp.cum[i])));
                    }
                }
                if let Some((d, s)) = best {
                    if d <= radius {
                        out.push((Contact { span: si, s, side }, d));
                    }
                }
            }
        }
        out
    }
}

/// Closest approach of a ray (`origin` + t `dir`, t >= 0) to the segment a-b: the segment's
/// parameter (0..1) and the distance.
pub fn ray_segment(origin: DVec3, dir: DVec3, a: DVec3, b: DVec3) -> (f64, f64) {
    let e = b - a;
    let w0 = origin - a;
    let (dd, de, ee) = (dir.dot(dir), dir.dot(e), e.dot(e).max(1e-12));
    let (dw, ew) = (dir.dot(w0), e.dot(w0));
    let den = dd * ee - de * de;
    let mut u = if den.abs() > 1e-9 { (dd * ew - de * dw) / den } else { ew / ee };
    u = u.clamp(0.0, 1.0);
    let t = ((de * u - dw) / dd).max(0.0);
    let u = ((origin + dir * t - a).dot(e) / ee).clamp(0.0, 1.0);
    let t = (dir.dot(a + e * u - origin) / dd).max(0.0);
    (u, (origin + dir * t).distance(a + e * u))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkEnd {
    DeadEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveErr {
    EndOfWire,
    OutOfReach,
}

#[derive(Debug, Clone, Copy)]
pub struct NodePass {
    pub node: u32,
    pub from: (u32, u8),
    pub to: (u32, u8),
}

#[derive(Debug, Clone, Default)]
pub struct WalkInfo {
    /// The sharpest horizontal turn (radians) of the wire met on the way.
    pub kink: f64,
    pub nodes: Vec<NodePass>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight line along x with a branch leaving to the left at x = 50.
    pub(crate) fn y_net() -> WireNet {
        WireNet::parse(
            "[trolley_net]\nvoltage 600\n\
             [trolley_section]\nid 1\nlive 1\n\
             [trolley_span]\nsection 1\np 0 0 5.5\np 25 0 5.5\np 50 0 5.5\n\
             [trolley_span]\nsection 1\np 50 0 5.5\np 75 0 5.5\np 100 0 5.5\n\
             [trolley_span]\nsection 1\np 50 0 5.5\np 70 6 5.5\np 90 20 5.5\n\
             [trolley_switch]\npos 50 0 5.5\nmode power\ntrigger 8\nsense 10\n",
        )
    }

    /// A 0.8 m piece beside the frog (shorter than the joining distance) is closed over, not
    /// joined to itself: the frog still has its three ends and the straight way runs through.
    #[test]
    fn a_stub_shorter_than_the_join_distance_does_not_break_the_frog() {
        let n = WireNet::parse(
            "[trolley_section]\nid 1\nlive 1\n\
             [trolley_span]\nsection 1\np 0 0 5.5\np 25 0 5.5\np 50 0 5.5\n\
             [trolley_span]\nsection 1\np 50 0 5.5\np 50.8 0 5.5\n\
             [trolley_span]\nsection 1\np 50.8 0 5.5\np 75 0 5.5\np 100 0 5.5\n\
             [trolley_span]\nsection 1\np 50 0 5.5\np 70 6 5.5\np 90 20 5.5\n\
             [trolley_switch]\npos 50 0 5.5\nmode fixed\n",
        );
        assert_eq!(n.spans.len(), 3, "the stub is dropped");
        assert_eq!(n.switches.len(), 1);
        assert!(n.spans.iter().all(|s| s.len() > 1.0));
    }

    /// A piece longer than twice the joining distance whose ends curl close together keeps its
    /// two ends apart: they are two nodes, and the neighbours at the first end meet there.
    #[test]
    fn a_curled_piece_does_not_join_its_own_two_ends() {
        let n = WireNet::parse(
            "[trolley_section]\nid 1\nlive 1\n\
             [trolley_span]\nsection 1\np 0 0 5.5\np 20 0 5.5\n\
             [trolley_span]\nsection 1\np 20 0 5.5\np 22 1.5 5.5\np 22.5 4 5.5\np 21 5 5.5\np 20.6 0.8 5.5\n\
             [trolley_span]\nsection 1\np 20.6 0.8 5.5\np 40 0 5.5\n",
        );
        let curl = &n.spans[1];
        assert_ne!(curl.ends[0], curl.ends[1], "both ends in one node");
        assert!(curl.len() > 4.0);
    }

    #[test]
    fn spans_join_into_nodes_and_a_switch() {
        let n = y_net();
        assert_eq!(n.spans.len(), 3);
        assert_eq!(n.switches.len(), 1);
        let sw = &n.switches[0];
        assert_eq!(sw.toe.0, 0);
        assert_eq!(sw.straight.0, 1);
        assert_eq!(sw.branch.0, 2);
        // the branch leaves to the left (north) of a vehicle heading east
        assert!(!sw.branch_right);
    }

    #[test]
    fn a_shoe_takes_the_branch_the_switch_is_set_to() {
        let n = y_net();
        let mut dir = 1i8;
        let mut c = Contact { span: 0, s: 40.0, side: 0 };
        let mut info = WalkInfo::default();
        n.walk(&mut c, &mut dir, 20.0, &mut info).unwrap();
        assert_eq!(c.span, 1);
        assert!((c.s - 10.0).abs() < 1e-9);
        n.set_switch(0, true);
        let mut c = Contact { span: 0, s: 40.0, side: 0 };
        let mut dir = 1i8;
        n.walk(&mut c, &mut dir, 20.0, &mut WalkInfo::default()).unwrap();
        assert_eq!(c.span, 2);
        // the branch turns away from the main line: the walk saw a kink
        let mut info = WalkInfo::default();
        let mut c = Contact { span: 0, s: 40.0, side: 0 };
        n.walk(&mut c, &mut 1i8.clone(), 20.0, &mut info).unwrap();
        assert!(info.kink > 0.2, "{}", info.kink);
    }

    #[test]
    fn the_end_of_a_wire_is_a_dead_end() {
        let n = y_net();
        let mut c = Contact { span: 1, s: 20.0, side: 0 };
        let mut dir = 1i8;
        assert!(n.walk(&mut c, &mut dir, 100.0, &mut WalkInfo::default()).is_err());
    }

    #[test]
    fn trailing_through_the_frog_goes_to_the_toe_whatever_the_position() {
        let n = y_net();
        for branch in [false, true] {
            n.set_switch(0, branch);
            let mut c = Contact { span: 2, s: 5.0, side: 0 };
            let mut dir = -1i8;
            n.walk(&mut c, &mut dir, 10.0, &mut WalkInfo::default()).unwrap();
            assert_eq!(c.span, 0);
            assert_eq!(dir, -1);
        }
    }

    #[test]
    fn solving_the_tip_puts_it_a_boom_length_from_the_hinge() {
        let n = y_net();
        let base = DVec3::new(30.0, 0.0, 3.0);
        let mut c = Contact { span: 0, s: 20.0, side: 0 };
        n.solve_tip(&mut c, base, 6.5).unwrap();
        let (p, _) = n.wire_point(&c);
        assert!(((p - base).length() - 6.5).abs() < 1e-3);
        // the pole trails behind the hinge: the contact is behind (west of) it
        assert!(p.x < base.x);
    }

    #[test]
    fn a_wire_too_far_cannot_be_reached() {
        let n = y_net();
        let base = DVec3::new(30.0, 0.0, -10.0);
        let mut c = Contact { span: 0, s: 20.0, side: 0 };
        assert!(n.solve_tip(&mut c, base, 6.5).is_err());
    }

    #[test]
    fn dead_sections_and_insulator_gaps_carry_no_current() {
        let n = WireNet::parse(
            "[trolley_section]\nid 1\nlive 1\n[trolley_section]\nid 2\nlive 0\n\
             [trolley_span]\nsection 1\np 0 0 5\np 20 0 5\n[trolley_span]\nsection 2\np 20 0 5\np 40 0 5\n",
        );
        assert_eq!(n.spans.len(), 2);
        assert!(n.is_live(&Contact { span: 0, s: 10.0, side: 0 }));
        assert!(!n.is_live(&Contact { span: 1, s: 10.0, side: 0 }));
        // the stretch around the insulator is dead even on the live side
        assert!(!n.is_live(&Contact { span: 0, s: 19.8, side: 0 }));
        assert!(n.is_live(&Contact { span: 0, s: 19.0, side: 0 }));
    }

    #[test]
    fn power_sensed_switch_follows_the_current() {
        let n = y_net();
        assert!(n.trigger_switch(0, 80.0, 0));
        assert!(n.switch_branch(0));
        // held while the vehicle passes
        assert!(!n.trigger_switch(0, 0.0, 0));
        assert!(n.switch_branch(0));
        n.end_step(3.0);
        assert!(n.trigger_switch(0, 0.0, 0));
        assert!(!n.switch_branch(0));
    }

    #[test]
    fn voltage_sags_with_the_current_drawn() {
        let n = y_net();
        let v0 = n.section_volts(0, 0.0);
        let v1 = n.section_volts(0, 300.0);
        assert!(v1 < v0 && v0 <= 600.0);
        assert!((v0 - v1 - 300.0 * 0.05).abs() < 1e-3);
    }
}
