//! A map's say over the arcs of a shoe crossing a frog or a section insulator under current.

/// Above `min_amps` (A) a crossing arcs with probability `chance`, its energy times `scale`.
/// The default is the game's own: every crossing above 3 A, at full energy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcRule {
    pub chance: f32,
    pub scale: f32,
    pub min_amps: f32,
}

impl Default for ArcRule {
    fn default() -> Self {
        Self { chance: 1.0, scale: 1.0, min_amps: 3.0 }
    }
}

impl ArcRule {
    /// Read one `[trolley_net]` key without its `frog_arc_` / `insulator_arc_` prefix.
    pub(crate) fn set(&mut self, key: &str, value: Option<f64>) {
        let value = value.filter(|v| v.is_finite());
        match key {
            "chance" => self.chance = value.unwrap_or(1.0).clamp(0.0, 1.0) as f32,
            "scale" => self.scale = value.unwrap_or(1.0).clamp(0.0, 10.0) as f32,
            "min_amps" => self.min_amps = value.unwrap_or(3.0).clamp(0.0, 2000.0) as f32,
            _ => log::warn!("trolley.txt [trolley_net]: unknown arc setting '{key}'"),
        }
    }
}
