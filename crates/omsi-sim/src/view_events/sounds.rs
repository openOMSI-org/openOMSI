//! What the AI cars' sounds read of the traffic, taken as owned data when they are brought up
//! to date (`TrafficSim::snapshot_sounds`): for each car whether it is heard, and for those
//! that are, where it and its coupled parts are, a copy of its variables and the triggers
//! its scripts fired since; and the horns sounded. The sound sets are the view's (the app's
//! `view_sync::traffic_audio`): they do not read the simulation.

use super::*;
use glam::{DVec3, Mat4};
use hashbrown::HashMap;

/// The traffic as its sounds read it (see the module). Its buffers are kept from one to the
/// next.
#[derive(Default)]
pub struct SoundFrame {
    cars: Vec<CarSound>,
    vars: Vec<f32>,
    parts: Vec<Mat4>,
    /// The cars that honked (`TrafficSim::honked`) and are on the road: (heavy, where, type).
    pub honks: Vec<(bool, DVec3, Arc<VehicleType>)>,
}

/// One AI car, as its sounds read it.
pub struct CarSound {
    pub id: u64,
    /// Whether it is heard: near enough to the listener, and not the bus the player rides in
    /// (heard from inside, apart). The rest below is only taken for a car that is heard.
    pub heard: bool,
    pub position: DVec3,
    pub ty: Arc<VehicleType>,
    /// The types of its coupled parts, in their order.
    pub part_types: Vec<Arc<VehicleType>>,
    /// Its fleet number (`VehicleInstance::number`).
    pub number: String,
    /// The body's model-to-world transform.
    pub world: Mat4,
    /// Variable names to slots of its variables.
    pub var_index: Arc<HashMap<String, omsi_script::VarId>>,
    vars: Span,
    parts: Span,
    /// The triggers its scripts fired since its sounds last heard them, the variables of the
    /// moment for those its sound set asked for, and the file triggers.
    pub fired: Vec<String>,
    pub fired_vars: Vec<(String, Vec<f32>)>,
    pub fired_files: Vec<(String, String)>,
}

impl SoundFrame {
    /// The cars in the traffic's order.
    pub fn cars(&self) -> &[CarSound] {
        &self.cars
    }

    /// The variable values of `car`.
    pub fn vars(&self, car: &CarSound) -> &[f32] {
        &self.vars[car.vars.range()]
    }

    /// The model-to-world transforms of `car`'s coupled parts, in their order.
    pub fn parts(&self, car: &CarSound) -> &[Mat4] {
        &self.parts[car.parts.range()]
    }
}

impl CarSound {
    /// The slot of variable `name` among its variables.
    pub fn var_slot(&self, name: &str) -> Option<usize> {
        omsi_script::compile::with_lower(name, |k| self.var_index.get(k).map(|&i| i as usize))
    }
}

impl crate::ai_traffic::TrafficSim {
    /// The cars as their sounds read them now, into `out` (see [`SoundFrame`]): those within
    /// `within` m of `listener` and not `riding` are heard - their fired triggers are taken
    /// (they wait in the car while it is not heard) and the road's state `street_cond` is
    /// given to them (`StreetCond`) before their variables are copied. The horns sounded since
    /// the last time are taken too.
    pub fn snapshot_sounds(
        &mut self,
        out: &mut SoundFrame,
        listener: DVec3,
        within: f64,
        street_cond: f32,
        riding: Option<u64>,
    ) {
        out.cars.clear();
        out.vars.clear();
        out.parts.clear();
        out.honks.clear();
        for c in &mut self.cars {
            let v = &mut c.vehicle;
            let heard = (v.position - listener).length() <= within && riding != Some(c.id);
            let mut car = CarSound {
                id: c.id,
                heard,
                position: v.position,
                ty: v.ty.clone(),
                part_types: Vec::new(),
                number: String::new(),
                world: Mat4::IDENTITY,
                var_index: v.var_index().clone(),
                vars: Span::default(),
                parts: Span::default(),
                fired: Vec::new(),
                fired_vars: Vec::new(),
                fired_files: Vec::new(),
            };
            if heard {
                car.fired = std::mem::take(&mut v.host.fired_triggers);
                car.fired_vars = std::mem::take(&mut v.host.fired_trigger_vars);
                car.fired_files = std::mem::take(&mut v.host.fired_file_triggers);
                v.host.street_cond = street_cond;
                v.set_engine_var("StreetCond", street_cond);
                car.part_types = v.trailers.iter().map(|t| t.ty.clone()).collect();
                car.number = v.number();
                car.world = v.world_transform();
                car.vars = Span::push(&mut out.vars, &v.state.vars);
                let start = out.parts.len() as u32;
                out.parts
                    .extend(v.trailers.iter().map(|t| t.world_transform()));
                car.parts = Span {
                    start,
                    len: out.parts.len() as u32 - start,
                };
            }
            out.cars.push(car);
        }
        for (id, heavy) in std::mem::take(&mut self.honked) {
            if let Some(c) = self.cars.iter().find(|c| c.id == id) {
                out.honks
                    .push((heavy, c.vehicle.position, c.vehicle.ty.clone()));
            }
        }
    }

    /// The triggers car `id`'s sound set wants the variables of the moment kept for
    /// (`VehicleHost::snapshot_triggers`), told when the set is made.
    pub fn set_sound_triggers(&mut self, id: u64, triggers: hashbrown::HashSet<String>) {
        if let Some(c) = self.cars.iter_mut().find(|c| c.id == id) {
            c.vehicle.host.snapshot_triggers = triggers;
        }
    }
}
