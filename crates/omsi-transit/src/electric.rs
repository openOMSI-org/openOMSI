//! `catenary.cfg` and `thirdrail.cfg`: where the current comes from.
//!
//! One format for the overhead wire of trams, trolleybuses and railways, and one for a third
//! rail; both describe sections fed by substations, the spans of the line, the breaks between
//! sections and, for the overhead, the frogs.
//!
//! ```text
//! [catenary_net]
//! kind dc                  // dc | ac
//! voltage 600              // V
//! wires 2                  // 1 (tram, railway) or 2 (trolleybus: positive and negative)
//! spacing 0.6              // m between two wires
//! height 5.5               // m above the rail head or the road, when a span gives no z
//! modes tram trolleybus
//!
//! [catenary_section]
//! id S1
//! name Substation A
//! live yes
//! feeder_ohm 0.05
//! regen_kw 0               // power it takes back from a braking vehicle
//!
//! [catenary_span]          // the centre line of the wire, as many points as it has bends
//! section S1
//! p 395.0 -185.4 6.10
//! p 392.0 -185.4 6.05
//!
//! [catenary_insulator]     // a break between two sections
//! pos 300 -185.4 6.1
//! length 0.8
//! between S1 S2
//!
//! [catenary_switch]        // a frog in the overhead: see switches.cfg for the track switch
//! id W1
//! pos 262 -185.4 6.1
//! mode power               // power | indicator | fixed
//! ```
//!
//! `thirdrail.cfg` has `[thirdrail_net]` (`voltage 750`, `side right`, `kind top|bottom`),
//! `[thirdrail_section]`, `[thirdrail_span]`, `[thirdrail_gap]` (a break where the rail ends,
//! at a crossing or a switch; a collector shoe bridges it only when two shoes are far enough
//! apart: `length 12`), the same keys as the overhead's.

use crate::doc::{parse, Block, Issue, Issues};
use omsi_cfg::transport::ModeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Current {
    #[default]
    Dc,
    Ac,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Net {
    pub kind: Current,
    pub voltage: f32,
    pub wires: u8,
    pub spacing_m: f32,
    pub height_m: f32,
    pub modes: ModeSet,
    /// Third rail only: which side of the track (-1 left, 1 right) and whether the shoe runs on
    /// the top or the bottom of the rail.
    pub side: i8,
    pub bottom_contact: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Section {
    pub id: String,
    pub name: String,
    pub live: bool,
    pub feeder_ohm: f32,
    pub regen_kw: f32,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub section: String,
    pub points: Vec<[f64; 3]>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gap {
    pub pos: [f64; 3],
    pub length_m: f32,
    pub between: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WireSwitch {
    pub id: String,
    pub pos: [f64; 3],
    pub mode: String,
    pub line: usize,
}

/// An electrification file: the overhead wire (`catenary`) or the third rail.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Electrification {
    pub third_rail: bool,
    pub net: Net,
    pub sections: Vec<Section>,
    pub spans: Vec<Span>,
    pub gaps: Vec<Gap>,
    pub switches: Vec<WireSwitch>,
}

impl Electrification {
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    /// Total length of the spans, in metres.
    pub fn length_m(&self) -> f64 {
        self.spans
            .iter()
            .map(|s| s.points.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt()).sum::<f64>())
            .sum()
    }

    pub fn parse(text: &str, file: &str, third_rail: bool, issues: &mut Vec<Issue>) -> Electrification {
        let mut is = Issues::new(file);
        let blocks = parse(text, &mut is);
        let p = if third_rail { "thirdrail" } else { "catenary" };
        let mut out = Electrification { third_rail, ..Default::default() };
        out.net.wires = if third_rail { 1 } else { 1 };
        let mut have_net = false;
        for b in &blocks {
            match b.name.strip_prefix(p).and_then(|s| s.strip_prefix('_')) {
                Some("net") => {
                    have_net = true;
                    out.net = read_net(b, third_rail, &mut is);
                }
                Some("section") => out.sections.push(Section {
                    id: b.str("id"),
                    name: b.get("name").map(str::to_string).unwrap_or_else(|| b.str("id")),
                    live: b.flag_or("live", true),
                    feeder_ohm: b.f32_or("feeder_ohm", 0.05),
                    regen_kw: b.f32_or("regen_kw", 0.0),
                    line: b.line,
                }),
                Some("span") => {
                    let mut points = Vec::new();
                    for e in b.all("p") {
                        let n: Vec<f64> = e.value.split_whitespace().filter_map(|w| w.parse().ok()).collect();
                        if n.len() == 3 {
                            points.push([n[0], n[1], n[2]]);
                        } else {
                            is.error(e.line, format!("a point needs x y z, got `{}`", e.value));
                        }
                    }
                    out.spans.push(Span { section: b.str("section"), points, line: b.line });
                }
                Some("insulator") | Some("gap") => {
                    let pos = b.vec3("pos");
                    if pos.is_none() {
                        is.error(b.line, "a break needs pos x y z");
                    }
                    out.gaps.push(Gap { pos: pos.unwrap_or_default(), length_m: b.metres("length").unwrap_or(if third_rail { 12.0 } else { 0.8 }), between: b.list("between", false), line: b.line });
                }
                Some("switch") if !third_rail => {
                    let pos = b.vec3("pos");
                    if pos.is_none() {
                        is.error(b.line, "a frog needs pos x y z");
                    }
                    let mode = b.get("mode").unwrap_or("fixed").trim().to_ascii_lowercase();
                    if !["power", "indicator", "fixed"].contains(&mode.as_str()) {
                        is.error(b.line_of("mode"), format!("mode {mode} is not power, indicator or fixed"));
                    }
                    out.switches.push(WireSwitch { id: b.str("id"), pos: pos.unwrap_or_default(), mode, line: b.line });
                }
                _ => is.warn(b.line, format!("unknown block [{}]", b.name)),
            }
        }
        if !have_net {
            is.error(0, format!("[{p}_net] is missing"));
        }
        for (i, s) in out.sections.iter().enumerate() {
            if s.id.is_empty() {
                is.error(s.line, "a section has no id");
            } else if out.sections[..i].iter().any(|o| o.id.eq_ignore_ascii_case(&s.id)) {
                is.error(s.line, format!("section id {} is used twice", s.id));
            }
        }
        for sp in &out.spans {
            if sp.points.len() < 2 {
                is.error(sp.line, "a span needs at least two points");
            }
            if out.section(&sp.section).is_none() {
                is.error(sp.line, format!("span: section {} does not exist", sp.section));
            }
        }
        for g in &out.gaps {
            for s in &g.between {
                if out.section(s).is_none() {
                    is.error(g.line, format!("break: section {s} does not exist"));
                }
            }
        }
        issues.append(&mut is.list);
        out
    }
}

fn read_net(b: &Block, third_rail: bool, is: &mut Issues) -> Net {
    let kind = match b.get("kind").map(|k| k.trim().to_ascii_lowercase()) {
        None => Current::Dc,
        Some(k) if k == "dc" => Current::Dc,
        Some(k) if k == "ac" => Current::Ac,
        Some(k) if third_rail && (k == "top" || k == "bottom") => Current::Dc,
        Some(k) => {
            is.error(b.line_of("kind"), format!("kind {k} is not dc or ac"));
            Current::Dc
        }
    };
    let voltage = b.f32_or("voltage", if third_rail { 750.0 } else { 600.0 });
    if voltage <= 0.0 {
        is.error(b.line_of("voltage"), "voltage must be above 0");
    }
    let wires = b.i32("wires").unwrap_or(1);
    if !(1..=2).contains(&wires) {
        is.error(b.line_of("wires"), "wires is 1 or 2");
    }
    let mut modes = ModeSet::EMPTY;
    for e in b.all("modes") {
        for m in ModeSet::parse_names(&e.value).iter() {
            modes.insert(m);
        }
    }
    let side = match b.get("side").map(|s| s.trim().to_ascii_lowercase()) {
        None => 1,
        Some(s) if s == "right" => 1,
        Some(s) if s == "left" => -1,
        Some(s) => {
            is.error(b.line_of("side"), format!("side {s} is not left or right"));
            1
        }
    };
    let bottom_contact = b.get("kind").is_some_and(|k| k.trim().eq_ignore_ascii_case("bottom"));
    Net { kind, voltage, wires: wires.clamp(1, 2) as u8, spacing_m: b.metres("spacing").unwrap_or(0.6), height_m: b.metres("height").unwrap_or(5.5), modes, side, bottom_contact }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_cfg::transport::TransportMode;

    #[test]
    fn an_overhead_wire() {
        let text = "[catenary_net]\nkind dc\nvoltage 600\nwires 2\nmodes tram trolleybus\n[catenary_section]\nid S1\n[catenary_section]\nid S2\nlive no\n[catenary_span]\nsection S1\np 0 0 6\np 30 0 6\n[catenary_span]\nsection S2\np 30 0 6\np 60 0 6.2\n[catenary_insulator]\npos 30 0 6\nbetween S1 S2\n[catenary_switch]\nid W1\npos 10 0 6\nmode power\n";
        let mut issues = Vec::new();
        let e = Electrification::parse(text, "catenary.cfg", false, &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(e.net.wires, 2);
        assert!(e.net.modes.contains(TransportMode::Trolleybus));
        assert!(!e.section("s2").unwrap().live);
        assert!((e.length_m() - 60.01).abs() < 0.05, "{}", e.length_m());
        assert_eq!(e.gaps.len(), 1);
        assert_eq!(e.gaps[0].length_m, 0.8);
        assert_eq!(e.switches[0].mode, "power");
    }

    #[test]
    fn a_third_rail() {
        let text = "[thirdrail_net]\nvoltage 750\nside left\nkind bottom\n[thirdrail_section]\nid T\n[thirdrail_span]\nsection T\np 0 0 0.2\np 100 0 0.2\n[thirdrail_gap]\npos 50 0 0.2\n";
        let mut issues = Vec::new();
        let e = Electrification::parse(text, "thirdrail.cfg", true, &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!((e.net.side, e.net.bottom_contact, e.net.voltage), (-1, true, 750.0));
        assert_eq!(e.gaps[0].length_m, 12.0);
    }

    #[test]
    fn mistakes_are_reported() {
        let text = "[catenary_section]\nid A\n[catenary_section]\nid a\n[catenary_span]\nsection B\np 1 2\n[catenary_insulator]\nbetween A C\n";
        let mut issues = Vec::new();
        Electrification::parse(text, "catenary.cfg", false, &mut issues);
        let all: Vec<String> = issues.iter().map(|i| i.to_string()).collect();
        for want in ["[catenary_net] is missing", "used twice", "needs x y z", "at least two points", "section B does not exist", "needs pos", "section C does not exist"] {
            assert!(all.iter().any(|t| t.contains(want)), "missing `{want}` in {all:#?}");
        }
    }
}
