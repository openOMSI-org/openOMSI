//! The automated manual (#713) and the gate a gear lever was put into last (#1832).

use super::Player;
use omsi_input::auto_shift::auto_shift_gear;

impl Player {
    /// The automated manual (the settings' `auto_shift`, #713; not an OMSI feature): on a
    /// manual gearbox worked through gates, first gear goes in when the throttle is pressed
    /// in neutral at a standstill, the next gear up once the engine turns well above its idle
    /// (later the more throttle), and the next down when it falls back towards it. The
    /// clutch is worked as for a shift by key.
    pub(crate) fn tick_auto_shift(&mut self, dt: f32, throttle: f32, brake: f32) {
        self.auto_shift_wait = (self.auto_shift_wait - dt).max(0.0);
        let read = self.script_gate_gear();
        if self.gates.tick(dt, read) {
            log::info!("gear lever: the script's gear variable did not follow gate {:?} (stayed {read:?}); the gate put in last is taken as the gear", self.gates.last);
        }
        if !self.auto_shift || !self.vehicle.ty.program.manual_gearbox() {
            return;
        }
        let Some(cur) = self.gate_gear() else { return };
        let Some(n) = ["engine_n", "antrieb_eng_n", "engine_rpm", "motor_n", "motor_rpm"].iter().find_map(|v| self.vehicle.var(v)) else { return };
        let kmh = self.vehicle.physics.velocity_kmh().abs();
        // the idle speed, from the engine running free at a standstill
        if n > 300.0 && kmh < 1.0 && throttle < 0.02 && (cur == 0 || self.axes.clutch > 0.9) {
            self.auto_shift_idle = if self.auto_shift_idle > 0.0 { self.auto_shift_idle + (n - self.auto_shift_idle) * (dt / 2.0).min(1.0) } else { n };
        }
        // (not while the clutch is still coming up from the last shift - except in neutral,
        // or rolling to a stop, where the automatic clutch holds it down anyway)
        if n < 300.0 || self.auto_shift_wait > 0.0 || (self.axes.clutch > 0.5 && cur != 0 && kmh >= 5.0) {
            return;
        }
        // (the governor's speed: full throttle, and the engine no longer rising for most of
        // a second)
        if throttle > 0.8 {
            let (peak, flat) = &mut self.auto_shift_climb;
            if n > *peak * 1.01 {
                (*peak, *flat) = (n, 0.0);
            } else {
                *flat += dt;
                if *flat > 0.8 {
                    self.auto_shift_max = self.auto_shift_max.max(*peak);
                }
            }
        } else {
            self.auto_shift_climb = (0.0, 0.0);
        }
        let idle = if self.auto_shift_idle > 300.0 { self.auto_shift_idle } else { 700.0 };
        let top = (1..=12).take_while(|g| self.vehicle.ty.program.trigger(&format!("kw_s_{g}")).is_some()).last().unwrap_or(0);
        let to = auto_shift_gear(cur, top, n / idle, self.auto_shift_max / idle, throttle, brake, kmh);
        if to != cur && self.shift_gate_to(to) {
            self.auto_shift_wait = 1.5;
        }
    }
}
