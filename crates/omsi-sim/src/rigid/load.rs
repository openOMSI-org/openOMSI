//! The people aboard as part of the body (openOMSI, the `passenger_mass` setting, #4014):
//! their weight adds to the body's mass, moves its centre of gravity towards where they are
//! and adds to its moments of inertia, so a full bus pulls away, brakes, climbs, leans and
//! sits on its springs as the heavier body it is.

use super::*;

/// What a body was before any load (`RigidBody::unloaded`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unloaded {
    pub mass: f32,
    pub inertia: Vec3,
    pub cog: Vec3,
    pub body_freq: f32,
}

/// The squared distances of `d` from the body's three axes (right, forward, up): what a point
/// mass there adds to the inertia about each, per kg.
fn perp(d: Vec3) -> Vec3 {
    Vec3::new(
        d.y * d.y + d.z * d.z,
        d.x * d.x + d.z * d.z,
        d.x * d.x + d.y * d.y,
    )
}

impl RigidBody {
    /// Carry `loads` - each a weight (kg) and its centre of gravity in the model frame (x
    /// right, y forward, z up, m, from the model origin) - in place of what it carried
    /// before; none: the body as its file made it. The body stays where it is in the world
    /// (its centre of gravity moves within it, the model origin does not move).
    pub fn set_load(&mut self, loads: &[(f32, Vec3)]) {
        let base = *self.unloaded.get_or_insert(Unloaded {
            mass: self.mass,
            inertia: self.inertia,
            cog: self.cog,
            body_freq: self.body_freq,
        });
        let loads = loads
            .iter()
            .filter(|(kg, p)| kg.is_finite() && *kg > 0.0 && p.is_finite());
        let kg: f32 = loads.clone().map(|l| l.0).sum();
        let mass = base.mass + kg;
        let cog =
            (base.cog * base.mass + loads.clone().fold(Vec3::ZERO, |a, (m, p)| a + *p * *m)) / mass;
        let inertia = base.inertia
            + perp(base.cog - cog) * base.mass
            + loads.fold(Vec3::ZERO, |a, (m, p)| a + perp(*p - cog) * *m);
        // (a rider getting on or off is a step change of a few dozen kg: below a tenth of
        // that, and a millimetre of the centre of gravity, nothing is worth moving)
        if (mass - self.mass).abs() < 2.0 && (cog - self.cog).length() < 1e-3 {
            return;
        }
        // the world position is the centre of gravity's: it moves with it in the body
        self.position += self.orientation.mul_vec3(cog - self.cog).as_dvec3();
        self.mass = mass;
        self.cog = cog;
        self.inertia = inertia;
        // (the springs' frequency under the body: sqrt(springs / mass))
        self.body_freq = base.body_freq * (base.mass / mass).sqrt();
    }

    /// The weight the body carries over its file's (kg).
    pub fn load_kg(&self) -> f32 {
        self.unloaded.map_or(0.0, |u| self.mass - u.mass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> RigidBody {
        let mut rb = RigidBody::from_definition(&omsi_vehicle::Vehicle::default(), &[]);
        rb.mass = 10_000.0;
        rb.cog = Vec3::new(0.0, 0.0, 1.0);
        rb.inertia = Vec3::new(100_000.0, 20_000.0, 100_000.0);
        rb.body_freq = 2.0;
        rb
    }

    #[test]
    fn a_load_adds_mass_moves_the_cog_and_comes_off_again() {
        let mut rb = body();
        rb.place(DVec3::ZERO, 0.0);
        let origin = rb.origin();
        // 20 people of 75 kg at the back, 4 m behind the origin
        let loads = vec![(75.0, Vec3::new(0.0, -4.0, 1.5)); 20];
        rb.set_load(&loads);
        assert_eq!(rb.mass, 11_500.0);
        assert!(
            (rb.cog.y - (-4.0 * 1500.0 / 11_500.0)).abs() < 1e-4,
            "{:?}",
            rb.cog
        );
        assert!(rb.inertia.x > 100_000.0 + 1500.0 * 16.0 * 0.8);
        assert!(rb.body_freq < 2.0);
        assert!((rb.load_kg() - 1500.0).abs() < 1e-3);
        // the model origin stays where it was
        assert!((rb.origin() - origin).length() < 1e-6);
        rb.set_load(&[]);
        assert_eq!(rb.mass, 10_000.0);
        assert!((rb.cog - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-6);
        assert!((rb.inertia - Vec3::new(100_000.0, 20_000.0, 100_000.0)).length() < 1e-2);
        assert_eq!(rb.body_freq, 2.0);
        assert!((rb.origin() - origin).length() < 1e-6);
    }

    #[test]
    fn nothing_loaded_leaves_the_body_as_it_was() {
        let mut rb = body();
        let before = (rb.mass, rb.cog, rb.inertia, rb.body_freq);
        rb.set_load(&[]);
        assert_eq!((rb.mass, rb.cog, rb.inertia, rb.body_freq), before);
        rb.set_load(&[(f32::NAN, Vec3::ZERO), (-5.0, Vec3::ZERO)]);
        assert_eq!((rb.mass, rb.cog, rb.inertia, rb.body_freq), before);
    }
}
