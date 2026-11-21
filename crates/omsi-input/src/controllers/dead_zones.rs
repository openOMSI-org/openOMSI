//! A dead zone for each axis the settings can give one to (#1344).

use super::Func;

/// A dead zone for each axis the settings can give one to (#1344): an old pedal whose
/// potentiometer trembles at rest wants its own, while the wheel is smooth, and the other way
/// round. An axis with none set here follows the setting for all of them (`ctrl_deadzone`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Zones {
    pub steer: Option<f32>,
    pub throttle: Option<f32>,
    pub brake: Option<f32>,
    pub clutch: Option<f32>,
}

impl Zones {
    /// The widest a dead zone may be, as the general setting has it.
    pub const MAX: f32 = 0.3;

    /// Read from the settings' `throttle=0.12;brake=0.08` (`steering`, `gas` and
    /// `throttlebrake` are the other names it knows; a decimal comma is read as such, the
    /// entries being told apart by the semicolon only). What cannot be read, and what is not an
    /// axis it knows, is left out.
    pub fn parse(s: &str) -> Zones {
        let mut z = Zones::default();
        for part in s.split(';') {
            let Some((name, v)) = part.split_once('=') else {
                continue;
            };
            let Ok(v) = v.trim().replace(',', ".").parse::<f32>() else {
                continue;
            };
            if !v.is_finite() {
                continue;
            }
            let v = v.clamp(0.0, Self::MAX);
            match name.trim().to_ascii_lowercase().as_str() {
                "steer" | "steering" | "wheel" => z.steer = Some(v),
                "throttle" | "gas" | "throttlebrake" => z.throttle = Some(v),
                "brake" => z.brake = Some(v),
                "clutch" => z.clutch = Some(v),
                _ => {}
            }
        }
        z
    }

    /// The settings' form of them, the axes it has something for only.
    pub fn write(&self) -> String {
        let mut out: Vec<String> = Vec::new();
        for (name, v) in [
            ("steer", self.steer),
            ("throttle", self.throttle),
            ("brake", self.brake),
            ("clutch", self.clutch),
        ] {
            if let Some(v) = v {
                out.push(format!("{name}={}", trim_zone(v.clamp(0.0, Self::MAX))));
            }
        }
        out.join(";")
    }

    /// The dead zone of what the axis `f` does: its own, or `all` - the setting for the axes
    /// without one.
    pub fn of(&self, f: Func, all: f32) -> f32 {
        let own = match f {
            Func::Steering => self.steer,
            Func::Throttle => self.throttle,
            Func::Brake => self.brake,
            Func::Clutch => self.clutch,
            Func::ThrottleBrake => self.throttle.or(self.brake),
            Func::LookX | Func::LookY => None,
        };
        own.unwrap_or(all).clamp(0.0, Self::MAX)
    }
}

/// A zone as the settings write it: two places, no trailing zeros (`0.12`, `0`, `0.075`).
fn trim_zone(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// An axis' value with its dead zone taken off, as the axis loop applies it: round the
/// wheel's centre, or at a pedal's rest (#1344).
pub fn axis_zone(v: f32, f: Func, dz: f32) -> f32 {
    let dz = dz.clamp(0.0, Zones::MAX);
    match f {
        Func::ThrottleBrake => v.signum() * ((v.abs() - dz).max(0.0) / (1.0 - dz)),
        _ => ((v + 1.0 - 2.0 * dz).max(0.0) / (1.0 - dz)) - 1.0,
    }
}
