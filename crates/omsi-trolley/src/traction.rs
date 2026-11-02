//! Traction electrics: what the pedals, the speed and the line voltage make of the wheel force
//! and of the current drawn from (or given back to) the line.
//!
//! The rheostat drive solves a series-wound motor against a stack of resistors: with the line
//! voltage `V`, the armature and series resistance `Ra`, the external resistance `Rx` of the
//! controller's step and the motor speed `w`, the current `I` satisfies
//! `V = (Ra + Rx) I + k(I) w`, where the flux linkage `k(I) = K I / (I + Ih)` saturates; the
//! torque is `k(I) I`. A controller steps the resistors out (and then the field in) while the
//! current stays under what the pedal asks for, as an accelerating relay does, and the resistors
//! turn what they take into heat. Braking is dynamic: the motors, excited from the line, drive
//! a resistor and the current of the generator, `E / (Ra + Rb)`, retards the bus.
//!
//! The asynchronous drive is an inverter: force up to the corner speed, constant power above,
//! faded out as the line voltage falls and tripped under a limit; braking returns power to the
//! line as far as the section's feeder takes it, a brake resistor burns the rest.

use crate::def::{AsyncDef, BatteryDef, DriveKind, RheostatDef};

#[derive(Debug, Clone, Copy, Default)]
pub struct TractionIn {
    pub dt: f32,
    /// Voltage at the vehicle (V); 0 with no complete circuit through the poles.
    pub volts: f32,
    /// Nominal line voltage (V), for the contactors' under-voltage limit.
    pub nominal: f32,
    pub pedal: f32,
    pub brake: f32,
    /// Selected direction: +1 forward, -1 reverse, 0 neutral.
    pub dir: f32,
    /// Speed along the heading (m/s).
    pub speed: f32,
    pub wheel_radius: f32,
    /// Power the line's feeder takes back (kW).
    pub regen_accept_kw: f32,
    /// Lights, compressor and heater (kW), drawn from the line.
    pub aux_kw: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TractionOut {
    /// Force at the wheels along the heading (N, negative reversing).
    pub wheel_force: f32,
    /// Force of the electric brake against the motion (N, >= 0): already in effect through
    /// the motors, not to be added to the wheel force.
    pub electric_brake: f32,
    /// The mechanical brakes' force (N, >= 0) the pedal still needs.
    pub mech_brake: f32,
    /// Current from the line (A, negative: given back).
    pub line_amps: f32,
    pub charge_amps: f32,
    /// Heat in the resistors (kW).
    pub heat_kw: f32,
    pub notch: f32,
    pub notches: f32,
    /// Contactor operations this step.
    pub clicks: u32,
    pub powered: bool,
    pub battery_soc: f32,
    pub on_battery: bool,
}

#[derive(Debug, Clone)]
enum Kind {
    Rheostat { def: RheostatDef, notch: u32, timer: f32, i_motor: f32 },
    Async { def: AsyncDef, force: f32, tripped: bool },
}

#[derive(Debug, Clone)]
pub struct Traction {
    kind: Kind,
    battery: Option<BatteryDef>,
    /// Battery energy (kWh).
    pub battery_kwh: f32,
}

fn flux(def: &RheostatDef, i: f32, scale: f32) -> f32 {
    def.flux_sat * scale * i / (i + def.flux_half_amps)
}

impl RheostatDef {
    /// External resistance of controller position `n` (0 = all of it in).
    pub fn external_ohm(&self, n: u32) -> f32 {
        if n >= self.steps {
            0.0
        } else {
            self.start_ohm * ((self.steps - n) as f32 / self.steps as f32).powf(1.7)
        }
    }

    pub fn positions(&self) -> u32 {
        self.steps + self.field_steps.len() as u32
    }

    fn field_scale(&self, n: u32) -> f32 {
        if n <= self.steps {
            1.0
        } else {
            self.field_steps.get((n - self.steps - 1) as usize).copied().unwrap_or(1.0)
        }
    }

    /// The motor current at line voltage `v`, motor speed `w` (rad/s) and controller position `n`.
    pub fn motor_current(&self, v: f32, w: f32, n: u32) -> f32 {
        let r = self.armature_ohm + self.external_ohm(n);
        let scale = self.field_scale(n);
        let f = |i: f32| v - r * i - flux(self, i, scale) * w;
        if f(0.0) <= 0.0 {
            return 0.0;
        }
        let (mut lo, mut hi) = (0.0f32, (v / r).max(1.0) * 1.5);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    pub fn motor_torque(&self, i: f32, n: u32) -> f32 {
        flux(self, i, self.field_scale(n)) * i
    }
}

impl Traction {
    pub fn new(drive: &DriveKind, battery: Option<BatteryDef>) -> Traction {
        let kind = match drive {
            DriveKind::Rheostat(d) => Kind::Rheostat { def: d.clone(), notch: 0, timer: 0.0, i_motor: 0.0 },
            DriveKind::Async(d) => Kind::Async { def: d.clone(), force: 0.0, tripped: false },
        };
        let kwh = battery.as_ref().map_or(0.0, |b| b.kwh * 0.7);
        Traction { kind, battery, battery_kwh: kwh }
    }

    pub fn notch(&self) -> u32 {
        match &self.kind {
            Kind::Rheostat { notch, .. } => *notch,
            Kind::Async { .. } => 0,
        }
    }

    pub fn step(&mut self, i: &TractionIn) -> TractionOut {
        let mut o = TractionOut::default();
        let v = i.speed.abs();
        let r = i.wheel_radius.max(0.2);
        match &mut self.kind {
            Kind::Rheostat { def, notch, timer, i_motor } => {
                let positions = def.positions();
                o.notches = positions as f32;
                // the line contactor drops out under-voltage, and over a motor's speed limit
                let w_m = i.speed * i.dir.signum() / r * def.gear_ratio;
                let live = i.volts >= 0.55 * i.nominal.max(1.0) && w_m.abs() < def.overspeed_rad && i.dir != 0.0;
                let before = *notch;
                *timer += i.dt;
                let braking = i.brake > 0.02 && live && def.brake_current > 0.0 && v > 0.8;
                if live && !braking && i.pedal > 0.02 {
                    let i_ref = def.current_min + (def.current_max - def.current_min) * i.pedal;
                    if *i_motor < 0.9 * i_ref && *notch < positions && *timer >= def.step_up_s {
                        *notch += 1;
                        *timer = 0.0;
                    } else if *i_motor > 1.08 * i_ref && *notch > 0 && *timer >= def.step_down_s {
                        *notch -= 1;
                        *timer = 0.0;
                    }
                    let cur = def.motor_current(i.volts, w_m, *notch).min(def.current_max * 1.25);
                    *i_motor = cur;
                    let torque = def.motor_torque(cur, *notch) * def.motors as f32;
                    o.wheel_force = i.dir.signum() * torque * def.gear_ratio * def.gear_efficiency / r;
                    o.line_amps = cur * def.motors as f32;
                    o.heat_kw = cur * cur * def.external_ohm(*notch) * def.motors as f32 * 1e-3;
                    o.powered = true;
                } else {
                    // controller falls back; no current
                    if *notch > 0 && *timer >= def.step_down_s * 0.5 {
                        *notch -= 1;
                        *timer = 0.0;
                    }
                    *i_motor = 0.0;
                }
                let demand = i.brake * def.brake_force;
                if braking {
                    // generator: excited at a share of the saturation flux
                    let k_ex = def.flux_sat * def.brake_excitation;
                    let e = k_ex * (v / r * def.gear_ratio);
                    let target = (i.brake * def.brake_current).min(def.brake_current);
                    let i_g = target.min(e / (def.armature_ohm + 0.04));
                    let t = k_ex * i_g * def.motors as f32;
                    o.electric_brake = (t * def.gear_ratio * def.gear_efficiency / r).min(demand);
                    // fades out as the bus nears a stop
                    o.electric_brake *= (v / 2.5).min(1.0);
                    o.heat_kw = e * i_g * def.motors as f32 * 1e-3;
                    o.line_amps = 4.0;
                    o.powered = true;
                }
                o.mech_brake = (demand - o.electric_brake).max(0.0);
                o.notch = *notch as f32;
                o.clicks = before.abs_diff(*notch);
            }
            Kind::Async { def, force, tripped } => {
                let connected = i.volts > 0.0;
                if connected {
                    if i.volts < def.v_trip {
                        *tripped = true;
                    } else if i.volts >= def.v_min {
                        *tripped = false;
                    }
                }
                let ok = connected && !*tripped && i.dir != 0.0;
                let derate = if i.volts >= def.v_min { 1.0 } else { ((i.volts - def.v_trip) / (def.v_min - def.v_trip).max(1.0)).clamp(0.0, 1.0) };
                let f_avail = def.max_force.min(def.max_power_kw * 1e3 / v.max(0.5)) * derate;
                let aux = if connected { i.aux_kw } else { 0.0 };
                let mut p_el_kw = aux;
                let before_force = *force;
                // drive
                let mut cmd = 0.0;
                let mut on_battery = false;
                if i.brake <= 0.02 && i.pedal > 0.02 && i.dir != 0.0 {
                    if ok {
                        cmd = i.pedal * f_avail;
                    } else if let Some(b) = self.battery.as_ref().filter(|b| self.battery_kwh > 0.01 && v * 3.6 < b.max_kmh) {
                        // away from the wire: the battery drives, up to its power and speed
                        on_battery = true;
                        cmd = i.pedal * (def.max_force * 0.5).min(b.kw * 1e3 / v.max(0.5));
                    }
                }
                let rate = def.force_rate * i.dt;
                *force += (cmd - *force).clamp(-rate, rate);
                if cmd == 0.0 && (*force).abs() < 1.0 {
                    *force = 0.0;
                }
                o.wheel_force = i.dir.signum() * *force;
                let p_wheel_kw = *force * v * 1e-3;
                let mut p_batt_kw = 0.0;
                if on_battery {
                    p_batt_kw = p_wheel_kw / def.efficiency;
                } else if ok {
                    p_el_kw += p_wheel_kw / def.efficiency;
                }
                // braking: back into the line as far as it takes it
                let demand = i.brake * def.brake_force;
                if i.brake > 0.02 && ok && v > 0.3 {
                    let cap = def.regen_force.min(def.regen_power_kw * 1e3 / v.max(0.5));
                    let mut f_reg = demand.min(cap) * (v / 1.2).min(1.0);
                    let p_reg_kw = f_reg * v * 1e-3 * def.efficiency;
                    let accept = i.regen_accept_kw.max(0.0) + aux;
                    if i.volts > def.v_regen_max || p_reg_kw > accept {
                        if def.chopper {
                            // what the line does not take burns in the brake resistor
                            o.heat_kw = (p_reg_kw - accept.min(p_reg_kw)).max(0.0);
                        } else {
                            let allowed = accept.min(p_reg_kw);
                            f_reg *= if p_reg_kw > 0.0 { allowed / p_reg_kw } else { 0.0 };
                        }
                    }
                    let p_back = (f_reg * v * 1e-3 * def.efficiency).min(accept.max(0.0)).max(0.0);
                    p_el_kw -= p_back;
                    o.electric_brake = f_reg;
                    *force = 0.0;
                    o.wheel_force = 0.0;
                }
                o.mech_brake = (demand - o.electric_brake).max(0.0);
                o.line_amps = if i.volts > 1.0 { p_el_kw * 1e3 / i.volts } else { 0.0 };
                o.powered = ok && (i.pedal > 0.02 || o.electric_brake > 0.0);
                o.on_battery = on_battery;
                // battery: spends while it drives, charges from the line at what the line can spare
                if let Some(b) = self.battery.as_ref() {
                    self.battery_kwh -= p_batt_kw * i.dt / 3600.0;
                    if ok && self.battery_kwh < b.kwh {
                        let charge_kw = 12.0f32.min((b.kwh - self.battery_kwh) * 3600.0 / i.dt.max(1e-3)) .min(12.0);
                        self.battery_kwh = (self.battery_kwh + charge_kw * i.dt / 3600.0).min(b.kwh);
                        if i.volts > 1.0 {
                            o.charge_amps = charge_kw * 1e3 / i.volts;
                            o.line_amps += o.charge_amps;
                        }
                    }
                    self.battery_kwh = self.battery_kwh.max(0.0);
                    o.battery_soc = if b.kwh > 0.0 { self.battery_kwh / b.kwh } else { 0.0 };
                }
                o.clicks = u32::from((before_force == 0.0) != (*force == 0.0));
                let _ = before_force;
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rheo() -> Traction {
        Traction::new(&DriveKind::Rheostat(RheostatDef::default()), None)
    }

    fn input(speed: f32, pedal: f32) -> TractionIn {
        TractionIn { dt: 1.0 / 60.0, volts: 600.0, nominal: 600.0, pedal, speed, dir: 1.0, wheel_radius: 0.5, ..Default::default() }
    }

    #[test]
    fn current_falls_as_the_motor_speeds_up() {
        let d = RheostatDef::default();
        let slow = d.motor_current(600.0, 5.0, 3);
        let fast = d.motor_current(600.0, 200.0, 3);
        assert!(slow > fast && fast >= 0.0, "{slow} {fast}");
        // at standstill the resistors limit the current
        let n0 = d.motor_current(600.0, 0.0, 0);
        let n_end = d.motor_current(600.0, 0.0, d.steps);
        assert!(n0 < n_end);
        assert!(n0 < 600.0 / (d.armature_ohm + d.start_ohm) + 1.0);
    }

    #[test]
    fn the_controller_climbs_while_the_bus_accelerates_and_the_resistors_heat() {
        let mut t = rheo();
        let mut speed = 0.0f32;
        let mut max_heat: f32 = 0.0;
        let mut o = TractionOut::default();
        for _ in 0..60 * 20 {
            o = t.step(&input(speed, 1.0));
            max_heat = max_heat.max(o.heat_kw);
            // a 16 t bus
            speed += (o.wheel_force / 16_000.0) * (1.0 / 60.0);
        }
        assert!(speed > 8.0, "speed {speed}");
        assert!(o.notch > 8.0, "notch {}", o.notch);
        assert!(max_heat > 10.0, "heat {max_heat}");
        // all the resistors out at the end: no resistor loss
        assert!(o.heat_kw < 5.0 || o.notch >= 14.0);
    }

    #[test]
    fn the_start_force_is_a_believable_acceleration() {
        let mut t = rheo();
        let mut o = TractionOut::default();
        for _ in 0..60 {
            o = t.step(&input(0.0, 1.0));
        }
        // a 16 t bus: between 0.7 and 1.8 m/s^2
        let a = o.wheel_force / 16_000.0;
        assert!(a > 0.7 && a < 1.8, "a {a}");
        // the line current at the start is some hundreds of amps, not thousands
        assert!(o.line_amps > 100.0 && o.line_amps < 420.0, "{}", o.line_amps);
    }

    #[test]
    fn no_voltage_no_force() {
        let mut t = rheo();
        let mut i = input(5.0, 1.0);
        i.volts = 0.0;
        let o = t.step(&i);
        assert_eq!(o.wheel_force, 0.0);
        assert_eq!(o.line_amps, 0.0);
        assert!(!o.powered);
    }

    #[test]
    fn dynamic_braking_retards_and_the_rest_is_mechanical() {
        let mut t = rheo();
        let mut i = input(10.0, 0.0);
        i.brake = 0.5;
        let o = t.step(&i);
        assert!(o.electric_brake > 1000.0, "{}", o.electric_brake);
        assert!(o.mech_brake >= 0.0);
        // without the line there is no excitation: mechanical brakes only
        i.volts = 0.0;
        let o = t.step(&i);
        assert_eq!(o.electric_brake, 0.0);
        assert!((o.mech_brake - 0.5 * RheostatDef::default().brake_force).abs() < 1.0);
    }

    fn inverter() -> Traction {
        Traction::new(&DriveKind::Async(AsyncDef::default()), Some(BatteryDef { kwh: 4.0, kw: 30.0, max_kmh: 15.0 }))
    }

    #[test]
    fn the_inverter_holds_its_power_above_the_corner_speed() {
        let mut t = inverter();
        let mut i = input(15.0, 1.0);
        let mut o = TractionOut::default();
        for _ in 0..120 {
            o = t.step(&i);
            i.speed = 15.0;
        }
        let p = o.wheel_force * 15.0;
        assert!((p - 170_000.0).abs() < 8_000.0, "{p}");
        // the line carries that over the efficiency
        assert!(o.line_amps > 250.0 && o.line_amps < 340.0, "{}", o.line_amps);
    }

    #[test]
    fn braking_gives_power_back_only_where_the_line_takes_it() {
        let mut i = input(12.0, 0.0);
        i.brake = 0.5;
        i.regen_accept_kw = 0.0;
        i.aux_kw = 6.0;
        let mut t = inverter();
        let o = t.step(&i);
        // the chopper burns it: the line still sees only the auxiliary load
        assert!(o.heat_kw > 10.0, "{}", o.heat_kw);
        assert!(o.line_amps > 0.0);
        let mut i2 = i;
        i2.regen_accept_kw = 200.0;
        let o2 = inverter().step(&i2);
        assert!(o2.line_amps < 0.0, "{}", o2.line_amps);
        assert!(o2.heat_kw < 1.0);
    }

    #[test]
    fn the_battery_moves_the_bus_slowly_without_the_line() {
        let mut t = inverter();
        let mut i = input(2.0, 1.0);
        i.volts = 0.0;
        let mut o = TractionOut::default();
        for _ in 0..120 {
            o = t.step(&i);
        }
        assert!(o.on_battery && o.wheel_force > 1000.0);
        assert!(o.line_amps == 0.0);
        // not past its speed limit
        i.speed = 6.0;
        let mut o = TractionOut::default();
        for _ in 0..60 {
            o = t.step(&i);
        }
        assert_eq!(o.wheel_force, 0.0, "{}", o.wheel_force);
        // a bus without a battery does not move at all
        let mut n = Traction::new(&DriveKind::Async(AsyncDef::default()), None);
        i.speed = 2.0;
        assert_eq!(n.step(&i).wheel_force, 0.0);
    }

    #[test]
    fn a_coasting_bus_charging_its_battery_draws_no_traction_current() {
        let mut t = inverter();
        let o = t.step(&input(8.0, 0.0));
        assert!(o.charge_amps > 15.0, "{}", o.charge_amps);
        assert!((o.line_amps - o.charge_amps).abs() < 1.0, "{} {}", o.line_amps, o.charge_amps);
    }

    #[test]
    fn the_inverter_trips_in_a_deep_sag() {
        let mut t = inverter();
        let mut i = input(8.0, 1.0);
        i.volts = 300.0;
        assert_eq!(t.step(&i).wheel_force, 0.0);
    }
}
