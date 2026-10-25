//! What a vehicle file says about its trolley gear: the poles on the roof and the traction
//! drive. The blocks are openOMSI's own (OMSI 2 has no overhead wire of its own; its trolleybus
//! add-ons run a plugin beside the game): a `[trolley_pole]` per pole, one `[trolley_rheostat]`
//! or `[trolley_async]` drive, an optional `[trolley_battery]` and `[trolley_vars]`. Inside a
//! block every line is `key value ...`; keys left out keep the defaults below, and anything
//! unknown is skipped.

use glam::Vec3;

/// One pole (boom) on the roof.
#[derive(Debug, Clone, PartialEq)]
pub struct PoleDef {
    /// -1 the pole of the left wire, +1 of the right one (seen from the driver).
    pub side: i8,
    /// The hinge on the roof (bus frame: x right, y forward, z up, m).
    pub base: Vec3,
    /// Length of the boom from hinge to the shoe's groove (m).
    pub length: f32,
    /// Elevation of the boom lying on its hook, and in its working position (degrees).
    pub rest_angle: f32,
    pub work_angle: f32,
    /// What the spring presses the shoe up against the wire with at the working angle (N).
    pub tip_force: f32,
    /// Mass of the boom with its rope (kg) and of the head and shoe at the tip (kg).
    pub mass: f32,
    pub head_mass: f32,
    /// How deep the wire sits in the shoe's groove (m): a wire that lifts out of the shoe by
    /// more than this has come off.
    pub groove_depth: f32,
    /// Half the width the shoe's horns catch a wire in (m).
    pub funnel: f32,
    /// How far the boom may stand off the wire's direction, seen from above (degrees).
    pub max_wire_angle: f32,
    /// How much the wire's direction may change under the shoe in one go, as a change of the
    /// shoe's sideways speed (m/s): a kink taken faster pulls the wire out.
    pub kink_speed: f32,
    /// A rope retriever pulls a pole that has come off down to its hook by itself (degrees a
    /// second it pulls with, 0 without).
    pub retriever: f32,
    /// A motor raises and lowers the pole from the cab.
    pub motor: bool,
    /// The game draws the boom (a vehicle model that carries its own poles says 0 and takes
    /// the pole variables instead).
    pub visible: bool,
    /// How far from the hinge the rope can be handled from the ground (m).
    pub reach: f32,
}

impl Default for PoleDef {
    fn default() -> Self {
        Self {
            side: 1,
            base: Vec3::new(0.35, -1.0, 3.0),
            length: 6.6,
            rest_angle: 2.5,
            work_angle: 26.0,
            tip_force: 120.0,
            mass: 16.0,
            head_mass: 3.0,
            groove_depth: 0.06,
            funnel: 0.12,
            max_wire_angle: 40.0,
            kink_speed: 3.0,
            retriever: 0.0,
            motor: false,
            visible: true,
            reach: 9.0,
        }
    }
}

/// A series-wound DC drive with a resistor controller (ZiU style).
#[derive(Debug, Clone, PartialEq)]
pub struct RheostatDef {
    /// Motors in parallel (each carries the same current; the line gives all of it).
    pub motors: u32,
    /// Motor to wheel gearing and its efficiency.
    pub gear_ratio: f32,
    pub gear_efficiency: f32,
    /// Armature plus series field resistance (ohm).
    pub armature_ohm: f32,
    /// Flux linkage at saturation (V s/rad) and the current at which half of it is built up (A).
    pub flux_sat: f32,
    pub flux_half_amps: f32,
    /// Resistor steps (the first is the whole external resistance, the last none) and the
    /// whole external resistance (ohm); the steps fall as a power law, as a resistor stack is
    /// tapped for an even climb.
    pub steps: u32,
    pub start_ohm: f32,
    /// Field weakening steps after the last resistor step (share of the flux kept).
    pub field_steps: Vec<f32>,
    /// The controller's pace: seconds between two steps while it climbs / falls.
    pub step_up_s: f32,
    pub step_down_s: f32,
    /// The pedal sets this current at its ends (A, per motor): the controller climbs while
    /// the current is under it.
    pub current_min: f32,
    pub current_max: f32,
    /// Dynamic (rheostatic) braking: the largest generator current per motor (A) and its
    /// excitation as a share of the saturation flux; 0 turns it off.
    pub brake_current: f32,
    pub brake_excitation: f32,
    /// Total mechanical brake force when the pedal is down (N).
    pub brake_force: f32,
    /// The motor's speed limit (rad/s); beyond it the contactor opens.
    pub overspeed_rad: f32,
}

impl Default for RheostatDef {
    fn default() -> Self {
        Self {
            motors: 1,
            gear_ratio: 9.6,
            gear_efficiency: 0.93,
            armature_ohm: 0.14,
            flux_sat: 3.3,
            flux_half_amps: 25.0,
            steps: 14,
            start_ohm: 2.6,
            field_steps: vec![0.72, 0.5],
            step_up_s: 0.22,
            step_down_s: 0.08,
            current_min: 120.0,
            current_max: 310.0,
            brake_current: 220.0,
            brake_excitation: 0.75,
            brake_force: 56_000.0,
            overspeed_rad: 380.0,
        }
    }
}

/// A three-phase asynchronous drive behind an inverter.
#[derive(Debug, Clone, PartialEq)]
pub struct AsyncDef {
    /// Wheel force available from standstill (N) and the power it keeps above its corner
    /// speed (W, at the wheels).
    pub max_force: f32,
    pub max_power_kw: f32,
    /// Efficiency of the motor and inverter together.
    pub efficiency: f32,
    /// Regenerative power at most (kW at the wheels) and the largest braking force (N).
    pub regen_power_kw: f32,
    pub regen_force: f32,
    /// The roof's brake resistor takes what the line will not.
    pub chopper: bool,
    /// Line voltage the inverter works between: it fades out below `v_min`, trips under
    /// `v_trip`, and keeps regenerating only below `v_regen_max` (V).
    pub v_min: f32,
    pub v_trip: f32,
    pub v_regen_max: f32,
    /// How quickly the force follows the pedal (N/s).
    pub force_rate: f32,
    /// Total mechanical brake force when the pedal is down (N).
    pub brake_force: f32,
}

impl Default for AsyncDef {
    fn default() -> Self {
        Self {
            max_force: 30_000.0,
            max_power_kw: 170.0,
            efficiency: 0.9,
            regen_power_kw: 150.0,
            regen_force: 26_000.0,
            chopper: true,
            v_min: 480.0,
            v_trip: 380.0,
            v_regen_max: 720.0,
            force_rate: 60_000.0,
            brake_force: 56_000.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DriveKind {
    Rheostat(RheostatDef),
    Async(AsyncDef),
}

/// An auxiliary battery drive for manoeuvring away from the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct BatteryDef {
    pub kwh: f32,
    /// The most it drives with (kW at the wheels) and the speed it works up to (km/h).
    pub kw: f32,
    pub max_kmh: f32,
}

/// Names of the script variables the gear reads and writes. A name the vehicle's variable
/// list does not have is left alone.
#[derive(Debug, Clone, PartialEq)]
pub struct VarNames {
    pub direction: String,
    pub indicator_left: String,
    pub indicator_right: String,
    pub raise_request: String,
}

impl Default for VarNames {
    fn default() -> Self {
        Self {
            direction: "Trolley_Direction".into(),
            indicator_left: "Blinker_Left".into(),
            indicator_right: "Blinker_Right".into(),
            raise_request: "Trolley_Poles_Raise".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrolleyDef {
    pub poles: Vec<PoleDef>,
    pub drive: Option<DriveKind>,
    /// The drive a plain trolleybus could get when its file has none. Not switched on yet: it
    /// waits for an adapter for the bus script's gears, engine and door brake (see
    /// `TrolleyGear::for_type`).
    pub typical_drive: Option<DriveKind>,
    /// `drive` is that typical one (never set until that adapter exists).
    pub drive_typical: bool,
    /// Lights, compressor and heater: taken from the line (kW).
    pub aux_kw: f32,
    pub battery: Option<BatteryDef>,
    pub vars: VarNames,
    /// A trolleybus made for OMSI 2 as it is: a bus script drives it and its poles are two
    /// meshes the cockpit switches `cp_SHTANGALEV`/`cp_SHTANGAPRAV` show (folded or raised).
    pub legacy: Option<LegacyDef>,
}

/// How a plain OMSI 2 trolleybus is told from its files (see `docs/TROLLEY_MODS.md`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LegacyDef {
    /// Per pole, the variable that is 0 with the pole folded and 1 raised.
    pub switches: Vec<String>,
    /// The control is a chopper/inverter (thyristor-pulse, IGBT) and not a rheostat controller:
    /// it draws no starting current beyond what the power asks, and does not click.
    pub chopper: bool,
    /// Per pole, the model mesh (an index into the model's meshes) that shows it raised: it is
    /// turned about the pole's hinge by the pole's own angles, so that the model's own boom
    /// follows the hand and the wire instead of a drawn one.
    pub mesh_defs: Vec<usize>,
}

impl DriveKind {
    /// The drive a trolleybus of `mass` kg (loaded) has when its file says nothing: a rheostat
    /// controller (ZiU style), or an inverter drive where the model's description names a
    /// chopper or asynchronous control. The ratings follow the mass as the buses' do - 13 t
    /// has a 110 kW motor, 310 A, 1.4 m/s² off the line - so a short bus is not overpowered
    /// and an articulated one has the force to start.
    pub fn typical(mass: f32, chopper: bool) -> DriveKind {
        let k = (mass / 13_000.0).clamp(0.5, 2.2);
        if chopper {
            let a = AsyncDef::default();
            DriveKind::Async(AsyncDef { max_force: a.max_force * k, max_power_kw: a.max_power_kw * k, regen_power_kw: a.regen_power_kw * k, regen_force: a.regen_force * k, brake_force: a.brake_force * k, ..a })
        } else {
            let r = RheostatDef::default();
            DriveKind::Rheostat(RheostatDef { current_min: r.current_min * k, current_max: r.current_max * k, brake_current: r.brake_current * k, brake_force: r.brake_force * k, ..r })
        }
    }
}

impl TrolleyDef {
    pub fn is_empty(&self) -> bool {
        self.poles.is_empty()
    }
}

/// The `key value ...` lines of a block as (lower-case key, values), a `//` ending a line's
/// meaning.
fn pairs<'a>(lines: &[&'a str]) -> Vec<(String, Vec<&'a str>)> {
    lines
        .iter()
        .filter_map(|l| {
            let l = l.split("//").next().unwrap_or("").trim();
            let mut it = l.split_whitespace();
            let k = it.next()?;
            Some((k.to_ascii_lowercase(), it.collect()))
        })
        .collect()
}

fn num(v: &[&str], i: usize) -> Option<f32> {
    v.get(i).and_then(|s| s.replace(',', ".").parse::<f32>().ok()).filter(|x| x.is_finite())
}

fn flag(v: &[&str]) -> bool {
    v.first().is_none_or(|s| !matches!(*s, "0" | "false" | "no" | "off"))
}

impl PoleDef {
    pub fn parse(lines: &[&str]) -> PoleDef {
        let mut p = PoleDef::default();
        for (k, v) in pairs(lines) {
            match k.as_str() {
                "side" => {
                    if let Some(x) = num(&v, 0) {
                        p.side = if x < 0.0 { -1 } else { 1 };
                    } else if let Some(s) = v.first() {
                        p.side = if s.eq_ignore_ascii_case("left") { -1 } else { 1 };
                    }
                }
                "pos" | "base" => {
                    if let (Some(x), Some(y), Some(z)) = (num(&v, 0), num(&v, 1), num(&v, 2)) {
                        p.base = Vec3::new(x, y, z);
                    }
                }
                "length" => p.length = num(&v, 0).unwrap_or(p.length).clamp(2.0, 12.0),
                "rest_angle" => p.rest_angle = num(&v, 0).unwrap_or(p.rest_angle).clamp(-10.0, 30.0),
                "work_angle" => p.work_angle = num(&v, 0).unwrap_or(p.work_angle).clamp(5.0, 70.0),
                "tip_force" => p.tip_force = num(&v, 0).unwrap_or(p.tip_force).clamp(20.0, 600.0),
                "mass" => p.mass = num(&v, 0).unwrap_or(p.mass).clamp(2.0, 80.0),
                "head_mass" => p.head_mass = num(&v, 0).unwrap_or(p.head_mass).clamp(0.2, 20.0),
                "groove_depth" => p.groove_depth = num(&v, 0).unwrap_or(p.groove_depth).clamp(0.005, 0.15),
                "funnel" => p.funnel = num(&v, 0).unwrap_or(p.funnel).clamp(0.01, 0.4),
                "max_wire_angle" => p.max_wire_angle = num(&v, 0).unwrap_or(p.max_wire_angle).clamp(5.0, 80.0),
                "kink_speed" => p.kink_speed = num(&v, 0).unwrap_or(p.kink_speed).clamp(0.2, 10.0),
                "retriever" => p.retriever = if v.len() > 0 && num(&v, 0).is_none() { if flag(&v) { 35.0 } else { 0.0 } } else { num(&v, 0).unwrap_or(0.0).max(0.0) },
                "motor" => p.motor = flag(&v),
                "visible" => p.visible = flag(&v),
                "reach" => p.reach = num(&v, 0).unwrap_or(p.reach).clamp(1.0, 30.0),
                _ => {}
            }
        }
        p
    }
}

impl RheostatDef {
    pub fn parse(lines: &[&str]) -> RheostatDef {
        let mut d = RheostatDef::default();
        for (k, v) in pairs(lines) {
            let x = num(&v, 0);
            match k.as_str() {
                "motors" => d.motors = x.map_or(d.motors, |x| x.clamp(1.0, 8.0) as u32),
                "gear_ratio" => d.gear_ratio = x.unwrap_or(d.gear_ratio).clamp(1.0, 60.0),
                "gear_efficiency" => d.gear_efficiency = x.unwrap_or(d.gear_efficiency).clamp(0.5, 1.0),
                "armature_ohm" => d.armature_ohm = x.unwrap_or(d.armature_ohm).clamp(0.01, 5.0),
                "flux_sat" => d.flux_sat = x.unwrap_or(d.flux_sat).clamp(0.2, 30.0),
                "flux_half_amps" => d.flux_half_amps = x.unwrap_or(d.flux_half_amps).clamp(1.0, 500.0),
                "steps" => d.steps = x.map_or(d.steps, |x| x.clamp(2.0, 60.0) as u32),
                "start_ohm" => d.start_ohm = x.unwrap_or(d.start_ohm).clamp(0.0, 50.0),
                "field_steps" => {
                    d.field_steps = v.iter().filter_map(|s| s.replace(',', ".").parse::<f32>().ok()).filter(|f| *f > 0.1 && *f < 1.0).collect();
                }
                "step_up_s" => d.step_up_s = x.unwrap_or(d.step_up_s).clamp(0.02, 3.0),
                "step_down_s" => d.step_down_s = x.unwrap_or(d.step_down_s).clamp(0.01, 3.0),
                "current_min" => d.current_min = x.unwrap_or(d.current_min).max(1.0),
                "current_max" => d.current_max = x.unwrap_or(d.current_max).max(d.current_min),
                "brake_current" => d.brake_current = x.unwrap_or(d.brake_current).max(0.0),
                "brake_excitation" => d.brake_excitation = x.unwrap_or(d.brake_excitation).clamp(0.05, 1.0),
                "brake_force" => d.brake_force = x.unwrap_or(d.brake_force).max(0.0),
                "overspeed" => d.overspeed_rad = x.unwrap_or(d.overspeed_rad).max(50.0),
                _ => {}
            }
        }
        d.current_max = d.current_max.max(d.current_min);
        d
    }
}

impl AsyncDef {
    pub fn parse(lines: &[&str]) -> AsyncDef {
        let mut d = AsyncDef::default();
        for (k, v) in pairs(lines) {
            let x = num(&v, 0);
            match k.as_str() {
                "max_force" => d.max_force = x.unwrap_or(d.max_force).max(0.0),
                "max_power_kw" => d.max_power_kw = x.unwrap_or(d.max_power_kw).max(1.0),
                "efficiency" => d.efficiency = x.unwrap_or(d.efficiency).clamp(0.5, 1.0),
                "regen_power_kw" => d.regen_power_kw = x.unwrap_or(d.regen_power_kw).max(0.0),
                "regen_force" => d.regen_force = x.unwrap_or(d.regen_force).max(0.0),
                "chopper" => d.chopper = flag(&v),
                "v_min" => d.v_min = x.unwrap_or(d.v_min),
                "v_trip" => d.v_trip = x.unwrap_or(d.v_trip),
                "v_regen_max" => d.v_regen_max = x.unwrap_or(d.v_regen_max),
                "force_rate" => d.force_rate = x.unwrap_or(d.force_rate).max(100.0),
                "brake_force" => d.brake_force = x.unwrap_or(d.brake_force).max(0.0),
                _ => {}
            }
        }
        d
    }
}

impl BatteryDef {
    pub fn parse(lines: &[&str]) -> BatteryDef {
        let mut b = BatteryDef { kwh: 4.0, kw: 30.0, max_kmh: 15.0 };
        for (k, v) in pairs(lines) {
            let x = num(&v, 0);
            match k.as_str() {
                "kwh" => b.kwh = x.unwrap_or(b.kwh).max(0.0),
                "kw" => b.kw = x.unwrap_or(b.kw).max(0.0),
                "max_kmh" => b.max_kmh = x.unwrap_or(b.max_kmh).max(1.0),
                _ => {}
            }
        }
        b
    }
}

impl VarNames {
    pub fn parse(lines: &[&str], into: &mut VarNames) {
        for (k, v) in pairs(lines) {
            let Some(n) = v.first() else { continue };
            match k.as_str() {
                "direction" => into.direction = n.to_string(),
                "indicator_left" => into.indicator_left = n.to_string(),
                "indicator_right" => into.indicator_right = n.to_string(),
                "raise_request" => into.raise_request = n.to_string(),
                _ => {}
            }
        }
    }
}

impl TrolleyDef {
    /// One of the blocks, by its keyword (lower case) and its lines; false when it is not a
    /// trolley keyword.
    pub fn take_block(&mut self, keyword: &str, lines: &[&str]) -> bool {
        match keyword {
            "trolley_pole" => self.poles.push(PoleDef::parse(lines)),
            "trolley_rheostat" => self.drive = Some(DriveKind::Rheostat(RheostatDef::parse(lines))),
            "trolley_async" => self.drive = Some(DriveKind::Async(AsyncDef::parse(lines))),
            "trolley_battery" => self.battery = Some(BatteryDef::parse(lines)),
            "trolley_vars" => VarNames::parse(lines, &mut self.vars),
            "trolley_aux" => {
                if let Some(x) = pairs(lines).first().and_then(|(k, v)| num(v, 0).or_else(|| k.parse().ok())) {
                    self.aux_kw = x.max(0.0);
                }
            }
            _ => return false,
        }
        true
    }

    pub fn keywords() -> &'static [&'static str] {
        &["trolley_pole", "trolley_rheostat", "trolley_async", "trolley_battery", "trolley_vars", "trolley_aux"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pole_block_overrides_defaults() {
        let p = PoleDef::parse(&["side left", "pos -0.4 -1.2 3.05", "length 7.0 // m", "retriever 1"]);
        assert_eq!(p.side, -1);
        assert_eq!(p.base, Vec3::new(-0.4, -1.2, 3.05));
        assert_eq!(p.length, 7.0);
        assert!(p.retriever > 0.0);
        assert_eq!(p.tip_force, 120.0);
    }

    #[test]
    fn blocks_are_taken_by_keyword() {
        let mut d = TrolleyDef::default();
        assert!(d.take_block("trolley_pole", &["side 1"]));
        assert!(d.take_block("trolley_rheostat", &["steps 10", "field_steps 0.6 0.4"]));
        assert!(!d.take_block("mass", &[]));
        assert_eq!(d.poles.len(), 1);
        match d.drive {
            Some(DriveKind::Rheostat(r)) => {
                assert_eq!(r.steps, 10);
                assert_eq!(r.field_steps, vec![0.6, 0.4]);
            }
            _ => panic!("rheostat drive"),
        }
    }

    #[test]
    fn the_typical_drive_follows_the_mass_and_the_kind() {
        let force = |d: &DriveKind| match d {
            DriveKind::Rheostat(r) => r.brake_force,
            DriveKind::Async(a) => a.max_force,
        };
        let small = DriveKind::typical(8_000.0, false);
        let big = DriveKind::typical(26_000.0, false);
        assert!(matches!(small, DriveKind::Rheostat(_)) && matches!(big, DriveKind::Rheostat(_)));
        assert!(force(&big) > force(&small) * 2.0, "an articulated bus has the force to start");
        assert!(matches!(DriveKind::typical(13_000.0, true), DriveKind::Async(_)));
        // a 13 t bus is the reference one
        assert_eq!(force(&DriveKind::typical(13_000.0, false)), RheostatDef::default().brake_force);
        // a toy mass and a freak one stay within bounds
        assert_eq!(force(&DriveKind::typical(1.0, false)), RheostatDef::default().brake_force * 0.5);
        assert_eq!(force(&DriveKind::typical(1e9, false)), RheostatDef::default().brake_force * 2.2);
    }
}
