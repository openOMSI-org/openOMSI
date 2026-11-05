//! A trolleybus's gear on a vehicle: the poles and the traction of `omsi_trolley`, fed with
//! the vehicle's pose and pedals every physics step, and their results handed back: the force
//! at the wheels, the mechanical brakes' share, and the variables a model animates the poles
//! by and its sounds follow.

use super::*;
use omsi_trolley::{RigInput, RopeCmd, TrolleyDef, TrolleyRig, WireNet};

pub struct TrolleyGear {
    pub rig: TrolleyRig,
    def: Arc<TrolleyDef>,
    pub carrier: Option<usize>,
    /// The overhead wire of the map the vehicle is on.
    pub net: Option<Arc<WireNet>>,
    /// What each pole's rope is doing (set by whoever handles the poles).
    pub ropes: Vec<RopeCmd>,
    /// Put the poles on the wire by themselves for this many seconds (tests and demos).
    pub auto_raise_for: f32,
    /// Take the poles down by themselves for this many seconds (the player's shortcut).
    pub auto_lower_for: f32,
    /// Wheel torque of this step when a modelled drive supplies it (Nm).
    torque: Option<f32>,
    /// What the script's own torque is scaled by (no drive modelled).
    gate: f32,
    /// A plain trolleybus: the poles whose raising the cab's switch started and the motor is
    /// still doing (until they are on the wire).
    legacy_raising: Vec<bool>,
    /// Per pole, whether it lay on its hook at the last step: a switch that still reads 1 as a
    /// pole comes down is our own raised mark, not a request.
    legacy_was_down: Vec<bool>,
    /// The pole the player holds with the mouse: it follows the mouse's ray (`grab_ray`, set
    /// by the window every frame) until a click lets it go (the window sets this to None)
    /// and it springs on to the wire it was aimed at.
    pub grab: Option<usize>,
    pub grab_ray: Option<(glam::DVec3, glam::DVec3)>,
    /// The window gave a fresh ray since the last step.
    pub grab_fresh: bool,
    /// The hand cannot see the wire (the cab: the poles are behind the driver): the motor takes
    /// the pole to the wire nearest its tip by itself, and the hand lets go when it is up.
    pub grab_auto: bool,
    /// The pole is held by its rope (the mouse's button held down on it from outside): it goes
    /// exactly where the mouse puts it, and let go it rises on its spring with no help to the wire.
    pub grab_manual: bool,
    /// The rope's hand moved this much since the last step (elevation, swing; radians).
    pub grab_move: (f64, f64),
    /// Where the window's cursor was last frame, while the rope is held.
    pub grab_cursor: Option<(f32, f32)>,
    grab_angles: Option<(f64, f64)>,
    /// The rope is held by the model's own script (`Trolley_Pole<n>_Rope`), not the window's mouse.
    pub grab_by_script: bool,
    /// Seen from the player, the screen's right is the bus's right (1) or its left (-1).
    pub rope_side: f64,
    rope_was: Vec<f32>,
    /// The slots of each pole's rope variables, found once (None per pole the script lacks them).
    rope_ids: Option<Vec<Option<[omsi_script::VarId; 3]>>>,
    /// The slots of each pole's `Trolley_Pole<n>_Gear`, written by a script that drives the lifting
    /// gear itself.
    gear_ids: Vec<Option<omsi_script::VarId>>,
    grab_idle: f32,
    grab_was: Option<usize>,
    /// A pole just let go of: the motor of a plain trolleybus (the retriever) carries it on to the
    /// wire the hand was aimed at for this many seconds, as it does when the cab raises it.
    assist: Vec<f32>,
    names: Names,
    /// The way the drive last pulled, read off the script's torque (see `step_trolley`).
    last_dir: f32,
    /// The variables of the poles and of the gear as the model reads them: their slots, and
    /// the values of the step (kept to be written again, no allocation).
    var_ids: Vec<omsi_script::VarId>,
    var_values: Vec<f32>,
}

struct Names {
    pole: Vec<[String; 7]>,
    /// Written by a model's script that handles the ropes itself (not declared by the game).
    rope: Vec<[String; 3]>,
    /// Written by a model's script that drives the lifting gear itself.
    gear: Vec<String>,
}

/// How far a hand on the rope turns the pole per pixel of the mouse (rad).
pub const ROPE_RATE: f64 = 0.0025;
/// The most poles whose ropes a script can hold.
const MAX_ROPES: usize = 8;

impl TrolleyGear {
    pub fn for_type(def: &TrolleyDef, seed: u64) -> Option<Box<TrolleyGear>> {
        if def.poles.is_empty() && def.drive.is_none() {
            return None;
        }
        // A legacy bus's script owns its gears and interlocks. Do not replace its
        // traction with a generic drive until those controls have an adapter.
        let pole = (1..=def.poles.len())
            .map(|k| {
                [
                    format!("Trolley_Pole{k}_Elev"),
                    format!("Trolley_Pole{k}_Azim"),
                    format!("Trolley_Pole{k}_State"),
                    format!("Trolley_Pole{k}_Contact"),
                    format!("Trolley_Pole{k}_Force"),
                    format!("Trolley_Pole{k}_Bounce"),
                    format!("Trolley_Pole{k}_Hit"),
                ]
            })
            .collect();
        let rope = (1..=def.poles.len()).map(|k| [format!("Trolley_Pole{k}_Rope"), format!("Trolley_Pole{k}_Rope_X"), format!("Trolley_Pole{k}_Rope_Y")]).collect();
        let gear = (1..=def.poles.len()).map(|k| format!("Trolley_Pole{k}_Gear")).collect();
        let mut rig = TrolleyRig::new(def, seed);
        rig.verbose = omsi_cfg::flags::OMSI_DEBUG_TROLLEY.is_set();
        Some(Box::new(TrolleyGear {
            rig,
            def: Arc::new(def.clone()),
            carrier: None,
            net: None,
            auto_raise_for: 0.0,
            auto_lower_for: 0.0,
            ropes: vec![RopeCmd::Idle; def.poles.len()],
            torque: None,
            gate: 1.0,
            legacy_raising: vec![false; def.poles.len()],
            legacy_was_down: vec![true; def.poles.len()],
            grab: None,
            grab_ray: None,
            grab_fresh: false,
            grab_auto: false,
            grab_manual: false,
            grab_move: (0.0, 0.0),
            grab_cursor: None,
            grab_angles: None,
            grab_by_script: false,
            rope_side: 1.0,
            rope_was: vec![0.0; def.poles.len()],
            rope_ids: None,
            gear_ids: Vec::new(),
            grab_idle: 0.0,
            grab_was: None,
            assist: vec![0.0; def.poles.len()],
            names: Names { pole, rope, gear },
            last_dir: 1.0,
            var_ids: Vec::new(),
            var_values: Vec::new(),
        }))
    }

    /// The mouse on a held pole's rope from outside, the cursor in window pixels (`scale`: per
    /// point): up lets the pole rise, down pulls it down, sideways swings it the way `side` says
    /// the screen's right goes (1 the bus's right). False when the rope cannot be held from here.
    pub fn rope_by_mouse(&mut self, cursor: (f32, f32), scale: f32, side: f64, from_cab: bool) -> bool {
        if from_cab && !self.grab_by_script {
            self.grab = None;
            return false;
        }
        // (a model's own script feeds its rope from its `_drag` instead)
        if let Some(last) = self.grab_cursor.filter(|_| !self.grab_by_script) {
            self.grab_move.0 -= ((cursor.1 - last.1) / scale) as f64 * ROPE_RATE;
            self.grab_move.1 += ((cursor.0 - last.0) / scale) as f64 * ROPE_RATE * side;
        }
        self.grab_cursor = Some(cursor);
        self.grab_fresh = true;
        true
    }
}

impl VehicleInstance {
    /// The poles' variables exist from the start (a model's pole mesh is turned by them: its
    /// animations look their variables up when the vehicle is built).
    pub(super) fn declare_trolley_vars(&mut self) {
        let Some(g) = self.trolley.as_ref() else { return };
        let names: Vec<String> = g.names.pole.iter().flatten().cloned().collect();
        for n in names {
            if self.var(&n).is_none() {
                self.set_engine_var(&n, 0.0);
            }
        }
    }

    /// Put the vehicle under a wire network (the map's).
    pub fn attach_wires(&mut self, net: Option<Arc<WireNet>>) {
        if let Some(g) = self.trolley.as_mut() {
            g.net = net;
        }
    }

    /// The model's own script handles pole `k`'s rope (it has `Trolley_Pole<n>_Rope`).
    pub fn trolley_rope_scripted(&self, k: usize) -> bool {
        self.trolley.as_ref().and_then(|g| g.names.rope.get(k)).is_some_and(|n| self.var(&n[0]).is_some())
    }

    /// Put the poles on the wire by themselves, whether or not they have a motor (the player's
    /// shortcut, or an auto-start); `only_under_wire`: only if each pole lying down has a wire
    /// within its reach. True when they are being put on.
    pub fn trolley_put_on(&mut self, only_under_wire: bool) -> bool {
        let (pos, m, _) = self.trolley_frame();
        let rot = glam::DMat3::from_cols(m.x_axis.truncate().as_dvec3(), m.y_axis.truncate().as_dvec3(), m.z_axis.truncate().as_dvec3());
        let Some(g) = self.trolley.as_deref_mut() else { return false };
        let down: Vec<&omsi_trolley::Pole> = g.rig.poles.iter().filter(|p| p.mode != omsi_trolley::PoleMode::OnWire).collect();
        if down.is_empty() {
            return false;
        }
        if only_under_wire {
            let Some(net) = g.net.as_deref() else { return false };
            let reaches = |p: &omsi_trolley::Pole| {
                let tip = p.base_at(pos, &rot) + rot * omsi_trolley::rotate_tip_pub((p.def.work_angle as f64).to_radians(), 0.0, p.def.length as f64);
                !net.candidates(tip, 3.5).is_empty()
            };
            if !down.iter().all(|p| reaches(p)) {
                return false;
            }
        }
        g.auto_lower_for = 0.0;
        g.auto_raise_for = g.auto_raise_for.max(5.0);
        true
    }

    /// Take the poles down on to their hooks by themselves (the player's shortcut). True when any
    /// was up.
    pub fn trolley_take_off(&mut self) -> bool {
        let Some(g) = self.trolley.as_deref_mut() else { return false };
        if g.rig.poles.iter().all(|p| p.mode == omsi_trolley::PoleMode::Down) {
            return false;
        }
        g.auto_raise_for = 0.0;
        g.auto_lower_for = 5.0;
        true
    }

    /// The player's Ctrl+R: the poles on the wire, or down if one is on; what the driver is told.
    pub fn trolley_toggle(&mut self) -> &'static str {
        if self.trolley.as_ref().is_some_and(|g| g.rig.poles.iter().any(|p| p.mode == omsi_trolley::PoleMode::OnWire)) {
            self.trolley_take_off();
            "Taking the poles down"
        } else {
            self.trolley_put_on(false);
            "Putting the poles on the wire"
        }
    }

    pub fn trolley_frame(&self) -> (glam::DVec3, glam::Mat4, Option<[f32; 6]>) {
        match self.trolley.as_ref().and_then(|g| g.carrier).and_then(|k| self.trailers.get(k)) {
            Some(t) => (t.position, t.body_rotation(), t.ty.def.bounding_box),
            None => (self.position, self.body_rotation(), self.ty.def.bounding_box),
        }
    }

    pub(super) fn adopt_trailer_trolley(&mut self, ty: &VehicleType, carrier: usize) -> bool {
        if self.trolley.is_some() || ty.def.trolley.poles.is_empty() {
            return false;
        }
        self.trolley = TrolleyGear::for_type(&ty.def.trolley, carrier as u64 ^ 0x7f4a_7c15);
        if let Some(g) = self.trolley.as_mut() {
            g.carrier = Some(carrier);
        }
        self.declare_trolley_vars();
        true
    }

    /// The driven wheels' radius (m).
    fn driven_radius(&self) -> f32 {
        let a = self.ty.def.axles.iter().find(|a| a.driven).or(self.ty.def.axles.first());
        a.map_or(0.5, |a| (a.wheel_diameter * 0.5).max(0.2))
    }

    /// Run the trolley gear one physics step, before the scripts' torque is read.
    pub(super) fn step_trolley(&mut self, dt: f32) {
        if self.trolley.is_none() {
            return;
        }
        let (pos, m, _) = self.trolley_frame();
        let rot = glam::DMat3::from_cols(m.x_axis.truncate().as_dvec3(), m.y_axis.truncate().as_dvec3(), m.z_axis.truncate().as_dvec3());
        let def = self.trolley.as_ref().unwrap().def.clone();
        // the way the drive pulls: the model's variable where it has one, else the way the
        // script's own torque points while the pedal is down (reverse gear: negative), kept
        // while the pedal is up
        let dir = match self.var(&def.vars.direction) {
            Some(d) => {
                if d > 0.5 { 1.0 } else if d < -0.5 { -1.0 } else { 0.0 }
            }
            // (only with the typical drive, which waits for the script adapter: see `TrolleyDef`)
            None if def.drive_typical => {
                let torque = self.get(self.v_m_wheel);
                let held = self.trolley.as_ref().map_or(1.0, |g| g.last_dir);
                let now = direction_of(self.physics.controls.throttle, torque, held);
                if let Some(g) = self.trolley.as_mut() {
                    g.last_dir = now;
                }
                now
            }
            None => 1.0,
        };
        let blinker = {
            let l = self.var(&def.vars.indicator_left).is_some_and(|v| v > 0.5);
            let r = self.var(&def.vars.indicator_right).is_some_and(|v| v > 0.5);
            match (l, r) {
                (true, false) => -1,
                (false, true) => 1,
                _ => 0,
            }
        };
        // radio switches: the cab's request, or the indicator in a vehicle that has none
        let radio = match self.var(&def.vars.switch_request) {
            Some(v) if (1.5..2.5).contains(&v) => 2,
            Some(v) if v > 0.5 => 1,
            Some(v) if v < -0.5 => -1,
            Some(_) => 0,
            None => blinker,
        };
        let route_code = self.var(&def.vars.switch_code).map_or(0, |v| v.max(0.0).round() as u32);
        let channel = self.var(&def.vars.switch_channel).map_or(0, |v| v.max(0.0).round() as u32);
        let mut raise_all = self.var(&def.vars.raise_request).map(|v| v);
        // a plain OMSI 2 trolleybus: the cab's two switches (0 folded, 1 raised) ask for the
        // poles, and their meshes follow what our poles do (2: both hidden, ours are drawn)
        let legacy_switch: Vec<(String, f32)> = def
            .legacy
            .as_ref()
            .map(|l| l.switches.iter().map(|n| (n.clone(), self.var(n).unwrap_or(0.0))).collect())
            .unwrap_or_default();
        if self.trolley.as_ref().is_some_and(|g| g.rope_ids.is_none()) {
            let ids = self.trolley.as_ref().unwrap().names.rope.iter().map(|n| {
                let id = |s: &str| omsi_script::compile::with_lower(s, |k| self.var_index.get(k).copied());
                Some([id(&n[0])?, id(&n[1])?, id(&n[2])?])
            }).collect();
            let gear = self.trolley.as_ref().unwrap().names.gear.iter().map(|n| omsi_script::compile::with_lower(n, |k| self.var_index.get(k).copied())).collect();
            let g = self.trolley.as_mut().unwrap();
            g.rope_ids = Some(ids);
            g.gear_ids = gear;
        }
        if let Some(g) = self.trolley.as_deref_mut() {
            for (p, id) in g.rig.poles.iter_mut().zip(&g.gear_ids) {
                p.gear = id.map(|i| self.state.vars[i as usize] as f64);
            }
        }
        let mut rope_vars = [None; MAX_ROPES];
        if let Some(ids) = self.trolley.as_ref().and_then(|g| g.rope_ids.as_ref()) {
            for (slot, [r, x, y]) in rope_vars.iter_mut().zip(ids.iter()).filter_map(|(s, i)| Some((s, (*i)?))) {
                let vars = &mut self.state.vars;
                *slot = Some((vars[r as usize], vars[x as usize], vars[y as usize]));
                vars[x as usize] = 0.0;
                vars[y as usize] = 0.0;
            }
        }
        let script_force = (self.get(self.v_m_wheel) / self.driven_radius().max(0.1)).max(0.0);
        let radius = self.driven_radius();
        let c = self.physics.controls;
        let (speed, accel) = (self.physics.speed, self.physics.a_trans);
        let has_drive = def.drive.is_some();
        let g = self.trolley.as_mut().unwrap();
        // the cab's switch: motorised poles go up (+) or down (-)
        let mut ropes = g.ropes.clone();
        let mut legacy_write: Vec<(String, f32)> = Vec::new();
        for (k, (name, v)) in legacy_switch.iter().enumerate() {
            let Some(p) = g.rig.poles.get(k) else { continue };
            let idle = ropes.get(k) == Some(&RopeCmd::Idle);
            let mode = p.mode;
            let raising = &mut g.legacy_raising[k];
            if matches!(mode, omsi_trolley::PoleMode::OnWire | omsi_trolley::PoleMode::Down) && !(mode == omsi_trolley::PoleMode::Down && *v > 0.5 && *v < 1.5) {
                *raising = false;
            }
            if *raising && idle {
                ropes[k] = RopeCmd::Raise { ray: None, auto: true };
            }
            let was_down = std::mem::replace(&mut g.legacy_was_down[k], mode == omsi_trolley::PoleMode::Down);
            if mode == omsi_trolley::PoleMode::Down {
                if *v > 0.5 && *v < 1.5 && was_down {
                    // a click on the cab's switch (or its key): the hand takes the pole -
                    // it follows the mouse until the next click puts it on the wire
                    g.grab = Some(k);
                    g.grab_idle = 0.0;
                    *raising = false;
                } else {
                    legacy_write.push((name.clone(), 0.0));
                }
            } else {
                // ours is up: the model's meshes are both hidden; the script's toggle (a key)
                // turning the 2 into 0 asks it down
                if *v < 0.5 && idle {
                    *raising = false;
                    ropes[k] = RopeCmd::Pull;
                }
                legacy_write.push((name.clone(), 1.0));
            }
        }
        for (k, v) in rope_vars.iter().enumerate().take(g.rig.poles.len()) {
            let Some((rope, x, y)) = *v else { continue };
            let was = std::mem::replace(&mut g.rope_was[k], rope);
            if rope > 0.5 && was <= 0.5 {
                g.grab = Some(k);
                g.grab_manual = true;
                g.grab_by_script = true;
                g.grab_fresh = true;
            } else if rope <= 0.5 && was > 0.5 && g.grab == Some(k) {
                g.grab = None;
            }
            if rope > 0.5 && g.grab == Some(k) {
                g.grab_move.0 -= y as f64 * ROPE_RATE;
                g.grab_move.1 += x as f64 * ROPE_RATE * g.rope_side;
                g.grab_fresh = true;
            }
            if rope < -0.5 && ropes.get(k) == Some(&RopeCmd::Idle) && g.grab != Some(k) {
                ropes[k] = RopeCmd::Pull;
            }
        }
        // the mouse's hand on a pole: aimed along the mouse's ray
        let was_grab = g.grab_was;
        if was_grab != g.grab {
            // let go: the pole goes on being guided to the wire the hand aimed at
            if let (Some(k), None) = (was_grab, g.grab) {
                if let Some(a) = g.assist.get_mut(k).filter(|_| !g.grab_manual) {
                    *a = 2.2;
                }
            }
            if g.grab.is_none() {
                g.grab_manual = false;
                g.grab_by_script = false;
            }
            g.grab_angles = None;
            g.grab_cursor = None;
            if g.grab.is_none() {
                g.grab_move = (0.0, 0.0);
            }
            g.grab_was = g.grab;
        }
        for (k, a) in g.assist.iter_mut().enumerate() {
            if *a > 0.0 {
                *a -= dt;
                let mode = g.rig.poles.get(k).map(|p| p.mode);
                if matches!(mode, Some(omsi_trolley::PoleMode::Hand | omsi_trolley::PoleMode::Free)) && g.grab != Some(k) && ropes.get(k) == Some(&RopeCmd::Idle) {
                    ropes[k] = if g.grab_auto { RopeCmd::Raise { ray: g.grab_ray, auto: true } } else { RopeCmd::Release };
                } else if mode == Some(omsi_trolley::PoleMode::OnWire) {
                    *a = 0.0;
                }
            }
        }
        if let Some(k) = g.grab {
            if k >= ropes.len() {
                g.grab = None;
            } else {
                if std::mem::take(&mut g.grab_fresh) {
                    g.grab_idle = 0.0;
                } else {
                    g.grab_idle += dt;
                }
                // (a window that does not feed the ray - no mouse - lets the pole go)
                let up = matches!(g.rig.poles.get(k).map(|p| p.mode), Some(omsi_trolley::PoleMode::Free | omsi_trolley::PoleMode::OnWire));
                if g.grab_manual {
                    let p = &g.rig.poles[k];
                    if p.hold_engaged || g.grab_idle > 2.0 {
                        g.grab = None;
                    } else {
                        // (taken where it is: it does not jump to the hand)
                        let (mut theta, mut psi) = *g.grab_angles.get_or_insert((p.theta, p.psi));
                        let (dt_, dp) = std::mem::take(&mut g.grab_move);
                        theta = (theta + dt_).clamp(p.rest(), p.top());
                        psi = (psi + dp).clamp(-1.7, 1.7);
                        g.grab_angles = Some((theta, psi));
                        ropes[k] = RopeCmd::Hold { theta, psi };
                    }
                } else if g.grab_idle > 2.0 || (g.grab_auto && up) {
                    g.grab = None;
                } else {
                    ropes[k] = RopeCmd::Raise { ray: g.grab_ray, auto: g.grab_auto };
                }
            }
        }
        raise_all = raise_all.filter(|_| legacy_switch.is_empty());
        if let Some(r) = raise_all {
            for (k, p) in g.rig.poles.iter().enumerate() {
                if p.def.motor && ropes.get(k) == Some(&RopeCmd::Idle) {
                    if r > 0.5 {
                        ropes[k] = RopeCmd::Raise { ray: None, auto: true };
                    } else if r < -0.5 {
                        ropes[k] = RopeCmd::Pull;
                    }
                }
            }
        }
        if g.auto_raise_for > 0.0 {
            g.auto_raise_for -= dt;
            for r in ropes.iter_mut() {
                *r = RopeCmd::Raise { ray: None, auto: true };
            }
        }
        if g.auto_lower_for > 0.0 {
            g.auto_lower_for -= dt;
            for (r, p) in ropes.iter_mut().zip(&g.rig.poles) {
                if p.mode != omsi_trolley::PoleMode::Down {
                    *r = RopeCmd::Pull;
                }
            }
        }
        let net = g.net.clone();
        let out = *g.rig.step(&RigInput {
            net: net.as_deref(),
            dt,
            pos,
            rot,
            accel,
            speed,
            wheel_radius: radius,
            pedal: c.throttle,
            brake: c.brake,
            dir,
            blinker,
            radio,
            route_code,
            channel,
            ropes: &ropes,
            drive: has_drive,
            script_force,
        });
        g.torque = has_drive.then_some(out.wheel_force * radius);
        g.gate = g.rig.script_gate;
        // the pole variables a model animates by (written by slot once they have been made: a
        // physics step of a trolleybus looked up some twenty names by their text before)
        const GLOBAL: [&str; 7] = ["Trolley_Volts", "Trolley_Amps", "Trolley_Powered", "Trolley_Notch", "Trolley_Arc", "Trolley_Hiss", "Trolley_Battery"];
        let mut vals = std::mem::take(&mut g.var_values);
        vals.clear();
        for (p, hit) in g.rig.poles.iter().zip(&g.rig.hit_level) {
            vals.extend_from_slice(&[p.theta.to_degrees() as f32, p.psi.to_degrees() as f32, p.state_code(), if p.closed { 1.0 } else { 0.0 }, p.force as f32, p.bounce_level(), *hit]);
        }
        vals.extend_from_slice(&[g.rig.volts, g.rig.amps, if g.rig.circuit { 1.0 } else { 0.0 }, out.notch, g.rig.arc, g.rig.hiss, out.battery_soc]);
        let mut ids = std::mem::take(&mut g.var_ids);
        let names: Option<Vec<String>> = (ids.len() != vals.len()).then(|| g.names.pole.iter().flatten().cloned().chain(GLOBAL.iter().map(|s| s.to_string())).collect());
        if let Some(names) = names {
            ids.clear();
            for (n, v) in names.iter().zip(&vals) {
                self.set_engine_var(n, *v);
                if let Some(id) = omsi_script::compile::with_lower(n, |k| self.var_index.get(k).copied()) {
                    ids.push(id);
                }
            }
            if ids.len() != vals.len() {
                ids.clear();
            }
        } else {
            for (id, v) in ids.iter().zip(&vals) {
                self.state.vars[*id as usize] = *v;
            }
        }
        for (n, v) in legacy_write {
            self.set_engine_var(&n, v);
        }
        if let Some(g) = self.trolley.as_mut() {
            g.var_values = vals;
            g.var_ids = ids;
        }
        if has_drive {
            // the drive's own force replaces the script's torque, and its mechanical brakes
            // add to the script's
            self.put(self.v_m_wheel, out.wheel_force * radius);
            let bf = self.get(self.v_brakeforce);
            self.put(self.v_brakeforce, bf + out.mech_brake);
        }
    }

    /// The torque `step_physics` drives with: the modelled drive's, or the script's scaled by
    /// whether the poles bring current (a driving torque only: the script may still brake).
    pub(super) fn trolley_torque(&self, script: f32) -> f32 {
        match self.trolley.as_deref() {
            None => script,
            Some(g) => match g.torque {
                Some(t) => t,
                None if script > 0.0 => script * g.gate,
                None => script,
            },
        }
    }
}

/// The way a drive of a plain trolleybus pulls, read off the bus script's wheel torque while the
/// pedal is down (reverse gear: negative torque), the last one kept while it is up or the script
/// gives none.
fn direction_of(throttle: f32, torque: f32, held: f32) -> f32 {
    if throttle > 0.05 && torque.abs() > 30.0 { torque.signum() } else { held }
}

#[cfg(test)]
mod direction_tests {
    use super::{direction_of, TrolleyDef};
    #[test]
    fn legacy_traction_is_not_replaced_by_a_typical_drive() {
        // Exercise the opt-in path in a fresh process without changing other tests' settings.
        const CHILD: &str = "OMSI_TEST_LEGACY_TROLLEY_DRIVE";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "vehicle::trolley::direction_tests::legacy_traction_is_not_replaced_by_a_typical_drive"])
                .env(CHILD, "1")
                .env("OMSI_REALISTIC_ROAD", "1")
                .status()
                .unwrap();
            assert!(status.success(), "legacy traction must also be preserved in realistic mode");
        } else {
            assert!(crate::rail::realistic(), "the child exercises realistic mode");
        }
        let def = TrolleyDef {
            poles: vec![omsi_trolley::PoleDef::default()],
            typical_drive: Some(omsi_trolley::DriveKind::typical(13_000.0, false)),
            ..Default::default()
        };
        let gear = super::TrolleyGear::for_type(&def, 1).unwrap();
        assert!(gear.def.drive.is_none(), "the bus script keeps its gears and interlocks");
        assert!(gear.torque.is_none(), "no native torque overrides the script");
        assert_eq!(gear.rig.poles.len(), 1, "pole simulation is retained");

        let explicit = TrolleyDef {
            drive: def.typical_drive.clone(),
            ..def
        };
        let gear = super::TrolleyGear::for_type(&explicit, 1).unwrap();
        assert_eq!(gear.def.drive, explicit.drive, "an explicitly configured drive remains available");
    }


    #[test]
    fn follows_the_sign_of_the_torque_under_the_pedal_and_keeps_it_after() {
        assert_eq!(direction_of(0.8, 400.0, 1.0), 1.0);
        assert_eq!(direction_of(0.8, -400.0, 1.0), -1.0);
        assert_eq!(direction_of(0.0, 0.0, -1.0), -1.0, "pedal up: kept");
        assert_eq!(direction_of(0.8, 0.0, -1.0), -1.0, "a script that gives no torque: kept");
        assert_eq!(direction_of(0.8, 12.0, 1.0), 1.0, "noise is not a direction");
    }
}
