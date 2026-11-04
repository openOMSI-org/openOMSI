//! What one row of the traffic table holds: the state of a vehicle in the units it travels
//! in, so that two rows are equal exactly when they would be drawn the same, and a row the
//! host holds is the row a client decodes (nothing is rounded twice).

/// What a vehicle is, for the client's choices of how to draw and hear it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Kind {
    #[default]
    Car,
    /// A timetable bus (or any vehicle with a line).
    Bus,
    /// A tram or a train, on its rails.
    Rail,
    Motorcycle,
    Truck,
    Other,
}

impl Kind {
    pub fn code(self) -> u64 {
        match self {
            Kind::Car => 0,
            Kind::Bus => 1,
            Kind::Rail => 2,
            Kind::Motorcycle => 3,
            Kind::Truck => 4,
            Kind::Other => 5,
        }
    }

    pub fn from_code(c: u64) -> Kind {
        match c {
            0 => Kind::Car,
            1 => Kind::Bus,
            2 => Kind::Rail,
            3 => Kind::Motorcycle,
            4 => Kind::Truck,
            _ => Kind::Other,
        }
    }
}

/// What a vehicle looks like: its file (relative to a content root), repaint and its name.
/// Many vehicles share one; it is told to a client once.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Look {
    pub file: String,
    pub scheme: Option<u8>,
    /// The repaint's name: its index counts the repaints installed on the host, a game with
    /// others in its content folder takes another one by that number.
    pub paint: String,
}

/// What a timetable bus shows: its line and its destination (`#<index>` of the depot file's
/// destinations, `#-1` for none), its fleet number and registration (the `number` and
/// `ident` string variables) and the depot file it reads them from (its file name without
/// the extension; empty when it has none). Empty for the random traffic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Meta {
    pub line: String,
    pub destination: String,
    pub number: String,
    pub ident: String,
    pub hof: String,
}

pub const MAX_LINE: usize = 16;
pub const MAX_DESTINATION: usize = 64;
pub const MAX_NUMBER: usize = 32;
pub const MAX_IDENT: usize = 32;
pub const MAX_HOF: usize = 64;

pub const SPEED_STEP: f64 = 0.05;
pub const STEER_STEP: f64 = 0.5;
pub const ATTITUDE_STEP: f64 = 0.1;
/// Heading steps in a turn.
pub const HEADING_STEPS: u32 = 4096;

pub const FLAGS_BITS: u32 = 6;

/// The state of one vehicle: position in centimetres (world coordinates, as the map has
/// them), heading, attitude, and what its scripts read of the drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Row {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// 0 .. 4095, degrees clockwise from north in 360/4096.
    pub heading: u16,
    /// 0.1 degrees.
    pub pitch: i8,
    pub bank: i8,
    /// 0.05 m/s, + forward, within ±1023.
    pub speed: i16,
    /// Front wheel angle in 0.5 degrees, + right.
    pub steer: i8,
    /// Bits 0-1 indicators (0 off, 1 left, 2 right, 3 hazard), bit 2 brake lights, bit 3
    /// headlights, bits 4-5 the stop (0 none, 1 boarding, 2 leaving).
    pub flags: u8,
}

fn q(v: f64, step: f64, lo: i64, hi: i64) -> i64 {
    if v.is_finite() {
        ((v / step).round() as i64).clamp(lo, hi)
    } else {
        0
    }
}

impl Row {
    /// A row from the simulation's numbers: position (m), heading, pitch and bank (degrees),
    /// speed (m/s), steer (degrees, + right), indicators 0-3, brake, headlights and
    /// `at_station` (1 boarding, -1 leaving, 0 not).
    #[allow(clippy::too_many_arguments)]
    pub fn from_state(
        pos: [f64; 3],
        heading: f64,
        pitch: f64,
        bank: f64,
        speed: f64,
        steer: f64,
        blinker: u8,
        brake: bool,
        lights: bool,
        at_station: i8,
    ) -> Row {
        let cm = |v: f64| {
            if v.is_finite() {
                (v * 100.0).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
            } else {
                0
            }
        };
        let station = match at_station {
            1 => 1u8,
            -1 => 2,
            _ => 0,
        };
        let heading = if heading.is_finite() { heading } else { 0.0 };
        Row {
            x: cm(pos[0]),
            y: cm(pos[1]),
            z: cm(pos[2]),
            heading: ((heading.rem_euclid(360.0) / 360.0 * HEADING_STEPS as f64).round() as u32 % HEADING_STEPS) as u16,
            pitch: q(pitch, ATTITUDE_STEP, -128, 127) as i8,
            bank: q(bank, ATTITUDE_STEP, -128, 127) as i8,
            speed: q(speed, SPEED_STEP, -1023, 1023) as i16,
            steer: q(steer, STEER_STEP, -128, 127) as i8,
            flags: blinker.min(3) | (brake as u8) << 2 | (lights as u8) << 3 | station << 4,
        }
    }

    pub fn position(&self) -> [f64; 3] {
        [self.x as f64 * 0.01, self.y as f64 * 0.01, self.z as f64 * 0.01]
    }

    pub fn heading_deg(&self) -> f64 {
        self.heading as f64 * 360.0 / HEADING_STEPS as f64
    }

    pub fn pitch_deg(&self) -> f64 {
        self.pitch as f64 * ATTITUDE_STEP
    }

    pub fn bank_deg(&self) -> f64 {
        self.bank as f64 * ATTITUDE_STEP
    }

    pub fn speed_ms(&self) -> f64 {
        self.speed as f64 * SPEED_STEP
    }

    pub fn steer_deg(&self) -> f64 {
        self.steer as f64 * STEER_STEP
    }

    pub fn blinker(&self) -> u8 {
        self.flags & 3
    }

    pub fn brake(&self) -> bool {
        self.flags & 4 != 0
    }

    pub fn lights(&self) -> bool {
        self.flags & 8 != 0
    }

    /// 1 boarding, -1 leaving, 0 not at a stop.
    pub fn at_station(&self) -> i8 {
        match self.flags >> 4 & 3 {
            1 => 1,
            2 => -1,
            _ => 0,
        }
    }

    /// The horizontal distance to `p` (m).
    pub fn distance_to(&self, p: [f64; 3]) -> f64 {
        let a = self.position();
        ((a[0] - p[0]).powi(2) + (a[1] - p[1]).powi(2)).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_keeps_what_the_wire_can_carry() {
        let r = Row::from_state([892_248.186, 4_196_461.374, 33.214], 271.3, -1.24, 0.4, 13.85, -7.5, 2, true, true, -1);
        assert_eq!((r.x, r.y, r.z), (89_224_819, 419_646_137, 3321));
        assert!((r.heading_deg() - 271.3).abs() < 0.05);
        assert!((r.pitch_deg() + 1.2).abs() < 1e-9 && (r.bank_deg() - 0.4).abs() < 1e-9);
        assert!((r.speed_ms() - 13.85).abs() < 0.026 && (r.steer_deg() + 7.5).abs() < 1e-9);
        assert_eq!((r.blinker(), r.brake(), r.lights(), r.at_station()), (2, true, true, -1));
        assert!(r.flags < 1 << FLAGS_BITS);
    }

    #[test]
    fn nothing_odd_gets_into_a_row() {
        let r = Row::from_state([f64::NAN, f64::INFINITY, 1e30], f64::NAN, 1e9, -1e9, 1e9, f64::NAN, 9, false, false, 7);
        assert_eq!((r.x, r.y), (0, 0));
        assert_eq!(r.z, i32::MAX);
        assert_eq!((r.pitch, r.bank), (127, -128));
        assert_eq!(r.speed, 1023);
        assert_eq!((r.heading, r.steer, r.blinker(), r.at_station()), (0, 0, 3, 0));
        // 359.99 degrees rounds round to north, not past the end
        assert_eq!(Row::from_state([0.0; 3], 359.999, 0.0, 0.0, 0.0, 0.0, 0, false, false, 0).heading, 0);
    }
}
