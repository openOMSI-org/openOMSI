//! The people aboard as part of the vehicle's weight (openOMSI, the `passenger_mass` setting,
//! #4014; see `rigid::load` and `people::load`).

use super::*;

impl VehicleInstance {
    /// Carry `loads` (`PeopleSim::loads`: each section's riders, front first) from now on:
    /// the front section's body takes its riders' weight where they are, a rear section's
    /// the weight of its own. Empty sections, or none: the vehicle as its files make it.
    pub fn set_passenger_load(&mut self, loads: &[Vec<(f32, Vec3)>]) {
        let front: &[(f32, Vec3)] = loads.first().map_or(&[], |l| l.as_slice());
        if let Some(rb) = self.rigid.as_mut() {
            rb.set_load(front);
        }
        let kg: f32 = front.iter().map(|l| l.0.max(0.0)).sum();
        self.physics.mass_kg = self.ty.def.mass_kg().max(500.0) + kg;
        for (k, t) in self.trailers.iter_mut().enumerate() {
            t.load_kg = loads
                .get(k + 1)
                .map_or(0.0, |l| l.iter().map(|x| x.0.max(0.0)).sum());
        }
    }

    /// The weight of the people the vehicle carries (kg, every section).
    pub fn passenger_load_kg(&self) -> f32 {
        (self.physics.mass_kg - self.ty.def.mass_kg().max(500.0)).max(0.0)
            + self.trailers.iter().map(|t| t.load_kg).sum::<f32>()
    }
}
