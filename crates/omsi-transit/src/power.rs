//! Where there is current: the overhead wire and the third rail asked at a place.

use crate::electric::Electrification;

/// What a collector finds at a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Supply {
    /// The voltage there (V); 0 off the line, under a dead section or in a break.
    pub volts: f32,
    /// The line's nominal voltage (V).
    pub nominal: f32,
}

impl Supply {
    /// The share of full drive the voltage allows: all of it down to 80 % of nominal, then
    /// falling with the voltage (a sagging line weakens the motors), nothing below 50 %.
    pub fn drive_factor(self) -> f32 {
        if self.nominal <= 0.0 {
            return 1.0;
        }
        let r = self.volts / self.nominal;
        if r >= 0.8 {
            1.0
        } else if r <= 0.5 {
            0.0
        } else {
            (r - 0.5) / 0.3
        }
    }

    pub fn live(self) -> bool {
        self.volts > 0.5 * self.nominal.max(1.0) * 0.5
    }
}

/// How far from the line's centre a collector may be and still touch it (m, horizontally): a
/// pantograph is wide and the wire zigzags over it; a third-rail shoe is narrow.
const WIRE_REACH: f64 = 2.5;
const RAIL_REACH: f64 = 1.2;
/// How far above or below the line a collector may be (m).
const HEIGHT_REACH: f64 = 8.0;

fn segment_distance(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> (f64, f64) {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-9);
    let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + (b[2] - a[2]) * t];
    (((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt(), (p[2] - q[2]).abs())
}

impl Electrification {
    /// The supply at `p`: the live section of the nearest span within reach, less nothing where
    /// a break lies. `None` when the line has no span at all (nothing is known of it).
    pub fn supply_at(&self, p: [f64; 3]) -> Option<Supply> {
        if self.spans.is_empty() {
            return None;
        }
        let reach = if self.third_rail { RAIL_REACH } else { WIRE_REACH };
        let nominal = self.net.voltage;
        let mut best: Option<(f64, &str)> = None;
        for sp in &self.spans {
            for w in sp.points.windows(2) {
                let (d, dz) = segment_distance(p, w[0], w[1]);
                if d <= reach && dz <= HEIGHT_REACH && best.is_none_or(|b| d < b.0) {
                    best = Some((d, sp.section.as_str()));
                }
            }
        }
        let volts = match best {
            None => 0.0,
            Some((_, section)) => {
                let live = self.section(section).is_none_or(|s| s.live);
                let in_gap = self.gaps.iter().any(|g| ((p[0] - g.pos[0]).powi(2) + (p[1] - g.pos[1]).powi(2)).sqrt() < g.length_m as f64 * 0.5);
                if live && !in_gap { nominal } else { 0.0 }
            }
        };
        Some(Supply { volts, nominal })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(third: bool) -> Electrification {
        let mut is = Vec::new();
        let p = if third { "thirdrail" } else { "catenary" };
        let text = format!("[{p}_net]\nvoltage 750\n[{p}_section]\nid A\n[{p}_section]\nid B\nlive no\n[{p}_span]\nsection A\np 0 0 6\np 100 0 6\n[{p}_span]\nsection B\np 100 0 6\np 200 0 6\n[{p}_gap]\npos 50 0 6\nlength 10\n");
        let text = if third { text } else { text.replace("_gap]", "_insulator]") };
        Electrification::parse(&text, "x.cfg", third, &mut is)
    }

    #[test]
    fn current_where_the_section_is_live() {
        let e = line(false);
        let s = e.supply_at([20.0, 1.0, 5.0]).unwrap();
        assert_eq!((s.volts, s.nominal), (750.0, 750.0));
        assert!(s.live());
    }

    #[test]
    fn none_under_a_dead_section_in_a_break_or_off_the_line() {
        let e = line(false);
        assert_eq!(e.supply_at([150.0, 0.0, 5.0]).unwrap().volts, 0.0, "section B has no substation");
        assert_eq!(e.supply_at([52.0, 0.0, 5.0]).unwrap().volts, 0.0, "in the break");
        assert_eq!(e.supply_at([20.0, 0.0, 5.0]).unwrap().volts, 750.0, "outside it");
        assert_eq!(e.supply_at([20.0, 30.0, 5.0]).unwrap().volts, 0.0, "30 m away from the wire");
        assert!(Electrification::default().supply_at([0.0; 3]).is_none(), "a map without a line: nothing is known");
    }

    #[test]
    fn a_third_rail_is_reached_closer_than_a_wire() {
        let wire = line(false);
        let rail = line(true);
        assert!(wire.supply_at([20.0, 2.0, 6.0]).unwrap().live());
        assert!(!rail.supply_at([20.0, 2.0, 6.0]).unwrap().live());
        assert!(rail.supply_at([20.0, 0.8, 6.0]).unwrap().live());
    }

    #[test]
    fn a_sagging_line_weakens_the_drive() {
        let s = |v| Supply { volts: v, nominal: 600.0 }.drive_factor();
        assert_eq!(s(600.0), 1.0);
        assert_eq!(s(500.0), 1.0);
        assert!((s(420.0) - 0.667).abs() < 0.01);
        assert_eq!(s(250.0), 0.0);
        assert_eq!(Supply { volts: 0.0, nominal: 0.0 }.drive_factor(), 1.0);
    }
}
