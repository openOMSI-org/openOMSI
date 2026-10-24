//! A rail vehicle's pantograph, on the vehicle: how high its head reaches (measured on the
//! model's own meshes and animations), the wire above the track that stops it, the current
//! that the contact brings, and the sparks where the contact breaks. The mechanics are
//! [`omsi_trolley::Pantograph`]'s; this feeds them the vehicle's variable, speed and current
//! and hands back what the model shows and whether the drive has power.
//!
//! The script's pantograph variable is left as the scripts wrote it (the switch, the door
//! leaf of a bus script); only the animations are shown another value, the one at which the
//! head touches the wire, so a raised pantograph rises *until it meets the wire*.

use super::*;
use crate::rail::RailClass;
use omsi_trolley::{PantoIn, PantoOut, PantoShape, Pantograph, Spark, WireAbove};

/// The wire above the head, as the one who knows the track says it (the game, from the map's
/// wire or the standard height over the rails). Heights are over the rails' plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Catenary {
    pub height: f32,
    /// How far the wire lies to the right of the track's axis (m): the stagger of a contact
    /// wire, which a pantograph's wide strip wears evenly with.
    pub lateral: f32,
    pub live: bool,
    pub hard: f32,
    pub roughness: f32,
}

pub struct PantoGear {
    rig: Pantograph,
    /// The head's travel, measured on first use (`Some(unusable)` where the model gave nothing).
    shape: Option<PantoShape>,
    wire: Option<Catenary>,
    run: f64,
    /// The current the drive draws (A) and the line's voltage (V).
    amps: f32,
    volts: f32,
    pub out: PantoOut,
    sparks: Vec<Spark>,
    ids: Vec<omsi_script::VarId>,
    log: f32,
}

/// Nominal line voltage of a class (V).
pub fn line_volts(class: Option<RailClass>) -> f32 {
    match class {
        Some(RailClass::Train) => 15_000.0,
        Some(RailClass::Metro) => 750.0,
        _ => 600.0,
    }
}

/// What a class takes from the line besides its traction (lights, compressors, heating), kW.
fn aux_kw(class: Option<RailClass>) -> f32 {
    match class {
        Some(RailClass::Train) => 30.0,
        Some(RailClass::Metro) => 40.0,
        _ => 6.0,
    }
}

const NAMES: [&str; 5] = ["Panto_Height", "Panto_Contact", "Panto_Force", "Panto_Arc", "Panto_Powered"];

impl PantoGear {
    pub(super) fn new(seed: u64) -> PantoGear {
        PantoGear { rig: Pantograph::new(seed), shape: None, wire: None, run: 0.0, amps: 0.0, volts: 600.0, out: PantoOut::default(), sparks: Vec::new(), ids: Vec::new(), log: 0.0 }
    }

    /// The pantograph's travel, if it has been measured and is worth the name.
    pub fn shape(&self) -> Option<&PantoShape> {
        self.shape.as_ref().filter(|s| s.is_usable())
    }

    pub fn losses(&self) -> u32 {
        self.rig.losses
    }
}

impl VehicleInstance {
    pub(super) fn init_pantograph(&mut self) {
        if self.ty.def.pantograph_var.is_some() && self.ty.def.is_rail() {
            let seed = (self.ty.def.path.to_string_lossy().len() as u64) ^ 0x51ed_270b_1f3a_9c47;
            self.panto = Some(Box::new(PantoGear::new(seed)));
        }
    }

    /// Put the wire above the track at the pantograph's place (`None`: the map has none, and a
    /// raised pantograph then simply goes to its stop and has power).
    pub fn set_catenary(&mut self, c: Option<Catenary>) {
        if let Some(g) = self.panto.as_mut() {
            g.wire = c;
        }
    }

    /// The pantograph's state, for a vehicle that has one.
    pub fn pantograph(&self) -> Option<&PantoGear> {
        self.panto.as_deref()
    }

    /// Where the pantograph's head is over the roof, in the world (the head at its working
    /// height when the travel is known, else at the roof's level): to look up the wire there.
    pub fn pantograph_head(&self) -> Option<DVec3> {
        let g = self.panto.as_deref()?;
        let s = g.shape()?;
        let rot = self.body_rotation();
        let p = rot.transform_point3(Vec3::new(s.head[0], s.head[1], g.out.height.max(s.bottom())));
        Some(self.position + p.as_dvec3())
    }

    /// The arcs struck since the last call, with their place in the world.
    pub fn take_pantograph_sparks(&mut self) -> Vec<Spark> {
        self.panto.as_mut().map(|g| std::mem::take(&mut g.sparks)).unwrap_or_default()
    }

    /// Note the current the drive asks for (the rail dynamics know the force and the speed).
    pub(super) fn note_traction_current(&mut self, drive: f32, speed: f32) {
        let class = RailClass::of(&self.ty.def);
        let volts = line_volts(class);
        if let Some(g) = self.panto.as_mut() {
            // electrical power from mechanical (a motor and gear lose 15 %) and the auxiliaries; a
            // braking vehicle gives back, which the pantograph does not count as a draw
            let mech = (drive * speed).max(0.0);
            g.volts = volts;
            g.amps = (mech / 0.85 + aux_kw(class) * 1000.0) / volts;
        }
    }

    /// The pantograph's variable as the scripts wrote it, 0 to 1.
    fn panto_command(&self) -> Option<f32> {
        let name = self.ty.def.pantograph_var.as_deref()?;
        self.var(name).map(|v| v.clamp(0.0, 1.0))
    }

    /// What the animations are shown for the pantograph's variable instead of the script's.
    pub(super) fn pantograph_shown(&self) -> Option<(omsi_script::VarId, f32)> {
        let g = self.panto.as_deref().filter(|_| crate::rail::realistic())?;
        g.shape()?;
        let id = self.var_slot(self.ty.def.pantograph_var.as_deref()?)?;
        Some((id as omsi_script::VarId, g.out.shown))
    }

    /// Measure the head's travel on the model: the highest point of the meshes the
    /// pantograph's variable moves, at 33 steps of it.
    fn measure_pantograph(&self) -> PantoShape {
        let n = 32usize;
        let none = PantoShape::default();
        let Some(pid) = self.ty.def.pantograph_var.as_deref().and_then(|v| self.var_slot(v)) else { return none };
        let moved = |i: usize, seen: &mut Vec<usize>| -> bool {
            // a mesh moves with the variable if its own animations or its parents' use it
            let mut k = Some(i);
            while let Some(c) = k {
                if seen.contains(&c) {
                    break;
                }
                seen.push(c);
                let Some(a) = self.animators.get(c) else { break };
                if a.anims.iter().any(|(_, _, st)| st.var.map(|v| v as usize) == Some(pid)) {
                    return true;
                }
                k = a.parent;
            }
            false
        };
        let meshes: Vec<usize> = (0..self.ty.meshes.len()).filter(|&i| !self.ty.meshes[i].data.positions.is_empty() && moved(i, &mut Vec::new())).collect();
        if meshes.is_empty() {
            return none;
        }
        let mut heights = Vec::with_capacity(n + 1);
        let mut head = [0.0f32; 2];
        let mut half = 0.5f32;
        // (one copy of the variables and the animators for all the steps: the animations are
        // set back to start each time)
        let mut vars = self.state.vars.clone();
        let mut probes = self.animators.clone();
        for step in 0..=n {
            let t = step as f32 / n as f32;
            vars[pid] = t;
            let mut transforms: Vec<Mat4> = probes
                .iter_mut()
                .map(|a| {
                    for (_, _, st) in a.anims.iter_mut() {
                        st.initialized = false;
                    }
                    a.update(0.0, &vars)
                })
                .collect();
            crate::anim::apply_parents(&probes, &mut transforms);
            let mut top = f32::MIN;
            let mut pts: Vec<Vec3> = Vec::new();
            for &i in &meshes {
                let m = transforms.get(i).copied().unwrap_or(Mat4::IDENTITY);
                for p in &self.ty.meshes[i].data.positions {
                    let q = m.transform_point3(*p);
                    top = top.max(q.z);
                    pts.push(q);
                }
            }
            heights.push(top);
            if step == n {
                // the head: what lies within 12 cm of the top, where it is and how wide
                let near: Vec<&Vec3> = pts.iter().filter(|p| p.z >= top - 0.12).collect();
                if !near.is_empty() {
                    let (x0, x1) = near.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.x), a.1.max(p.x)));
                    let (y0, y1) = near.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.y), a.1.max(p.y)));
                    head = [(x0 + x1) * 0.5, (y0 + y1) * 0.5];
                    half = ((x1 - x0) * 0.5).clamp(0.3, 1.2);
                }
            }
        }
        PantoShape { heights, head, half_width: half }
    }

    /// One physics step of the pantograph: the head against the wire, the current, the arcs.
    pub(super) fn step_pantograph(&mut self, dt: f32) {
        if self.panto.is_none() || !crate::rail::realistic() {
            return;
        }
        if self.panto.as_ref().is_some_and(|g| g.shape.is_none()) {
            let shape = self.measure_pantograph();
            if omsi_cfg::flags::OMSI_DEBUG_RAIL.is_set() {
                log::info!(
                    "pantograph: head from {:.2} m to {:.2} m over the origin at ({:.2}, {:.2}), strip {:.2} m{}",
                    shape.bottom(), shape.top(), shape.head[0], shape.head[1], shape.half_width * 2.0,
                    if shape.is_usable() { "" } else { " - not usable, the pantograph keeps the old rule" }
                );
            }
            if let Some(g) = self.panto.as_mut() {
                g.shape = Some(shape);
            }
        }
        let Some(cmd) = self.panto_command() else { return };
        let speed = self.physics.speed;
        let head_at = self.pantograph_head().unwrap_or(self.position);
        let Some(g) = self.panto.as_mut() else { return };
        let Some(shape) = g.shape.as_ref().filter(|s| s.is_usable()) else { return };
        let wire = g.wire.map(|c| WireAbove { height: c.height, lateral: c.lateral - shape.head[0], live: c.live, hard: c.hard, roughness: c.roughness });
        g.run += (speed * dt) as f64;
        let out = g.rig.step(&PantoIn { dt, shape, cmd, wire, run: g.run, speed: speed.abs(), amps: g.amps, volts: g.volts, head_at });
        g.out = out;
        let sparks = g.rig.take_sparks();
        g.sparks.extend(sparks);
        if g.sparks.len() > 48 {
            let extra = g.sparks.len() - 48;
            g.sparks.drain(0..extra);
        }
        // the numbers a model or a sound can follow
        let vals = [out.height, out.contact as u8 as f32, out.force, out.arc, out.powered as u8 as f32];
        if g.ids.len() != vals.len() {
            for (n, v) in NAMES.iter().zip(vals) {
                self.set_engine_var(n, v);
            }
            let ids: Vec<omsi_script::VarId> = NAMES.iter().filter_map(|n| omsi_script::compile::with_lower(n, |k| self.var_index.get(k).copied())).collect();
            if let Some(g) = self.panto.as_mut() {
                g.ids = if ids.len() == vals.len() { ids } else { Vec::new() };
            }
        } else {
            for (id, v) in g.ids.iter().zip(vals) {
                self.state.vars[*id as usize] = v;
            }
        }
        if omsi_cfg::flags::OMSI_DEBUG_RAIL.is_set() {
            let g = self.panto.as_mut().unwrap();
            g.log += dt;
            if g.log >= 0.5 {
                g.log = 0.0;
                log::info!(
                    "pantograph: cmd {:.2} shown {:.2} head {:.3} m wire {:?} contact {} powered {} force {:.0} N, {:.0} A, {} losses",
                    cmd, out.shown, out.height, g.wire.map(|c| (c.height * 100.0).round() / 100.0), out.contact, out.powered, out.force, g.amps, g.rig.losses
                );
            }
        }
    }

    /// Whether the pantograph, as the wire sees it, brings current: `None` for a vehicle whose
    /// pantograph is not simulated (none, or its travel could not be measured).
    pub(super) fn pantograph_current(&self) -> Option<bool> {
        let g = self.panto.as_deref().filter(|_| crate::rail::realistic())?;
        g.shape()?;
        Some(g.out.powered)
    }
}
