//! The rail blocks of a vehicle file (openOMSI keywords; OMSI 2 skips them): the traction
//! curve, the brake system and the couplers of a tram, metro train or railway vehicle.
//!
//! ```text
//! [rail_traction]
//! point 0 80          // km/h  kN: the most the motors pull at that speed (repeat, ascending)
//! point 40 80
//! point 90 36
//! power_kw 800        // the continuous power: the pull never passes P / v
//! efficiency 0.9      // drive line, motors to rail (default 0.9)
//! regen_point 10 60   // km/h  kN: the electric brake (repeat); none: no electric brake
//! regen_point 80 60
//!
//! [rail_power]
//! catenary            // catenary | third_rail | diesel: where the current comes from
//!
//! [rail_brake]
//! type air            // air | ep | direct
//! service 1.1         // m/s², full service application (default: the class's)
//! emergency 1.6       // m/s², emergency (default: the class's)
//! pipe_wave 250       // m/s, how fast a change runs down the brake pipe (air)
//! apply_time 4        // s, a brake cylinder fills to its pressure (air)
//! release_time 12     // s, it empties
//! blend 1             // 1: the electric brake goes first, the friction brake makes up the rest
//!
//! [rail_coupler]
//! type scharfenberg   // screw | buffer | scharfenberg | tightlock
//! slack 0.03          // m, the free play of a coupling pair
//! stiffness 3000      // kN/m, the draft gear
//! damping 80          // kN s/m
//! max_force 900       // kN, beyond it the train breaks in two
//! ```

/// Where a rail vehicle gets its current.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerKind {
    /// Worked out from the vehicle: a `[contact_shoe]` takes it from a third rail, a pantograph
    /// the file declares from the overhead wire, otherwise it needs none.
    #[default]
    Auto,
    Catenary,
    ThirdRail,
    /// Carries its own power (diesel, battery): no line needed.
    OnBoard,
}

impl PowerKind {
    pub fn from_name(s: &str) -> PowerKind {
        match s.trim().to_ascii_lowercase().as_str() {
            "catenary" | "overhead" | "wire" | "pantograph" => PowerKind::Catenary,
            "third_rail" | "thirdrail" | "third rail" | "shoe" => PowerKind::ThirdRail,
            "diesel" | "battery" | "onboard" | "none" => PowerKind::OnBoard,
            _ => PowerKind::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RailTraction {
    /// (speed in m/s, force in N), ascending.
    pub points: Vec<(f32, f32)>,
    /// Continuous power at the wheels in W; 0: no limit.
    pub power_w: f32,
    pub efficiency: f32,
    /// The electric brake: (speed in m/s, force in N), ascending. Empty: none.
    pub regen: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrakeKind {
    /// Compressed-air brake with a brake pipe: a change runs down the train as a wave and the
    /// cylinders fill and empty slowly.
    #[default]
    Air,
    /// Electro-pneumatic: every car is told at once by wire, the cylinders still fill.
    Ep,
    /// The brake force follows the handle at once (a tram's electro-hydraulic brake).
    Direct,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RailBrake {
    pub kind: BrakeKind,
    pub service: Option<f32>,
    pub emergency: Option<f32>,
    pub pipe_wave: f32,
    pub apply_time: f32,
    pub release_time: f32,
    pub blend: bool,
}

impl Default for RailBrake {
    fn default() -> Self {
        RailBrake { kind: BrakeKind::Air, service: None, emergency: None, pipe_wave: 250.0, apply_time: 4.0, release_time: 12.0, blend: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CouplerKind {
    Screw,
    Buffer,
    #[default]
    Scharfenberg,
    TightLock,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailCoupler {
    pub kind: CouplerKind,
    /// m
    pub slack: f32,
    /// N/m
    pub stiffness: f32,
    /// N s/m
    pub damping: f32,
    /// N
    pub max_force: f32,
}

impl RailCoupler {
    /// The usual figures of a kind of coupling.
    pub fn typical(kind: CouplerKind) -> RailCoupler {
        match kind {
            CouplerKind::Screw => RailCoupler { kind, slack: 0.30, stiffness: 2.0e6, damping: 1.0e5, max_force: 850_000.0 },
            CouplerKind::Buffer => RailCoupler { kind, slack: 0.12, stiffness: 4.0e6, damping: 1.5e5, max_force: 1_000_000.0 },
            CouplerKind::Scharfenberg => RailCoupler { kind, slack: 0.03, stiffness: 3.0e6, damping: 8.0e4, max_force: 900_000.0 },
            CouplerKind::TightLock => RailCoupler { kind, slack: 0.06, stiffness: 3.5e6, damping: 1.0e5, max_force: 1_000_000.0 },
        }
    }
}

impl Default for RailCoupler {
    fn default() -> Self {
        RailCoupler::typical(CouplerKind::Scharfenberg)
    }
}

fn pairs(lines: &[&str]) -> Vec<(String, String)> {
    lines
        .iter()
        .filter_map(|l| {
            let l = l.split("//").next().unwrap_or("").trim();
            let (k, v) = l.split_once(char::is_whitespace)?;
            Some((k.to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect()
}

fn nums(v: &str) -> Vec<f32> {
    v.split_whitespace().filter_map(|w| w.replace(',', ".").parse().ok()).collect()
}

fn curve(points: &mut Vec<(f32, f32)>) {
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    points.dedup_by(|a, b| a.0 == b.0);
}

impl RailTraction {
    pub fn parse(lines: &[&str]) -> RailTraction {
        let mut t = RailTraction { efficiency: 0.9, ..Default::default() };
        for (k, v) in pairs(lines) {
            let n = nums(&v);
            match k.as_str() {
                "point" if n.len() >= 2 && n[1] >= 0.0 => t.points.push((n[0].max(0.0) / 3.6, n[1] * 1000.0)),
                "regen_point" if n.len() >= 2 && n[1] >= 0.0 => t.regen.push((n[0].max(0.0) / 3.6, n[1] * 1000.0)),
                "power_kw" if !n.is_empty() => t.power_w = n[0].max(0.0) * 1000.0,
                "efficiency" if !n.is_empty() => t.efficiency = n[0].clamp(0.3, 1.0),
                _ => {}
            }
        }
        curve(&mut t.points);
        curve(&mut t.regen);
        t
    }

    fn interp(points: &[(f32, f32)], v: f32) -> f32 {
        match points {
            [] => 0.0,
            [only] => only.1,
            _ => {
                if v <= points[0].0 {
                    return points[0].1;
                }
                for w in points.windows(2) {
                    if v <= w[1].0 {
                        let f = (v - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
                        return w[0].1 + (w[1].1 - w[0].1) * f;
                    }
                }
                // beyond the last point the pull falls as the power does (F ~ 1/v)
                let (v1, f1) = points[points.len() - 1];
                f1 * v1 / v.max(v1)
            }
        }
    }

    /// The most the motors pull at `v` m/s (N): the curve, no more than the power allows.
    pub fn pull(&self, v: f32) -> f32 {
        let v = v.abs();
        let mut f = Self::interp(&self.points, v);
        if self.power_w > 0.0 {
            f = f.min(self.power_w * self.efficiency / v.max(0.5));
        }
        f
    }

    /// The most the electric brake holds at `v` m/s (N); 0 without one. It fades to nothing
    /// near a standstill (the motors cannot excite themselves below ~3 km/h).
    pub fn electric_brake(&self, v: f32) -> f32 {
        let v = v.abs();
        let fade = (v / 0.8).clamp(0.0, 1.0);
        Self::interp(&self.regen, v) * fade
    }
}

impl RailBrake {
    pub fn parse(lines: &[&str]) -> RailBrake {
        let mut b = RailBrake::default();
        for (k, v) in pairs(lines) {
            let n = nums(&v);
            match k.as_str() {
                "type" => {
                    b.kind = match v.trim().to_ascii_lowercase().as_str() {
                        "ep" | "electropneumatic" => BrakeKind::Ep,
                        "direct" | "electric" | "hydraulic" => BrakeKind::Direct,
                        _ => BrakeKind::Air,
                    }
                }
                "service" if !n.is_empty() && n[0] > 0.0 => b.service = Some(n[0]),
                "emergency" if !n.is_empty() && n[0] > 0.0 => b.emergency = Some(n[0]),
                "pipe_wave" if !n.is_empty() => b.pipe_wave = n[0].clamp(50.0, 340.0),
                "apply_time" if !n.is_empty() => b.apply_time = n[0].clamp(0.1, 60.0),
                "release_time" if !n.is_empty() => b.release_time = n[0].clamp(0.1, 120.0),
                "blend" if !n.is_empty() => b.blend = n[0] > 0.5,
                _ => {}
            }
        }
        b
    }
}

impl RailCoupler {
    pub fn parse(lines: &[&str]) -> RailCoupler {
        let kind = pairs(lines)
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| match v.trim().to_ascii_lowercase().as_str() {
                "screw" => CouplerKind::Screw,
                "buffer" | "buffers" => CouplerKind::Buffer,
                "tightlock" | "tight_lock" => CouplerKind::TightLock,
                _ => CouplerKind::Scharfenberg,
            })
            .unwrap_or_default();
        let mut c = RailCoupler::typical(kind);
        for (k, v) in pairs(lines) {
            let n = nums(&v);
            match k.as_str() {
                "slack" if !n.is_empty() => c.slack = n[0].clamp(0.0, 1.0),
                "stiffness" if !n.is_empty() => c.stiffness = (n[0] * 1000.0).clamp(1.0e5, 1.0e8),
                "damping" if !n.is_empty() => c.damping = (n[0] * 1000.0).clamp(0.0, 1.0e7),
                "max_force" if !n.is_empty() => c.max_force = (n[0] * 1000.0).max(50_000.0),
                _ => {}
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traction_curve_and_power() {
        let t = RailTraction::parse(&["point 90 36", "point 0 80", "point 40 80 // flat", "power_kw 800", "regen_point 80 60", "regen_point 10 60"]);
        assert_eq!(t.points.len(), 3);
        assert_eq!(t.points[0], (0.0, 80_000.0));
        assert!((t.pull(0.0) - 80_000.0).abs() < 1.0);
        // 800 kW x 0.9 / 11.1 m/s = 65 kN would be the power limit; the curve gives 80 kN: power wins
        let at40 = t.pull(40.0 / 3.6);
        assert!((at40 - 800_000.0 * 0.9 / (40.0 / 3.6)).abs() < 50.0, "{at40}");
        // beyond the last point it falls like 1/v
        assert!(t.pull(120.0 / 3.6) < t.pull(90.0 / 3.6));
        assert_eq!(t.pull(-10.0), t.pull(10.0), "either way");
        assert_eq!(t.electric_brake(0.0), 0.0);
        assert!((t.electric_brake(10.0) - 60_000.0).abs() < 1.0);
        assert_eq!(RailTraction::default().pull(5.0), 0.0);
    }

    #[test]
    fn brake_and_coupler_blocks() {
        let b = RailBrake::parse(&["type ep", "service 1.1", "pipe_wave 900", "apply_time 2", "blend 0"]);
        assert_eq!((b.kind, b.service, b.blend), (BrakeKind::Ep, Some(1.1), false));
        assert_eq!(b.pipe_wave, 340.0, "no faster than sound");
        assert_eq!(RailBrake::default().kind, BrakeKind::Air);
        let c = RailCoupler::parse(&["type screw", "slack 0.2", "max_force 600"]);
        assert_eq!((c.kind, c.slack, c.max_force), (CouplerKind::Screw, 0.2, 600_000.0));
        assert_eq!(c.stiffness, RailCoupler::typical(CouplerKind::Screw).stiffness);
    }
}
