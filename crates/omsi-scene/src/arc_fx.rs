//! The sparks and flashes of an arc where a current is broken, drawn as glare and light: a
//! rail vehicle's pantograph leaving the wire under load. (A trolleybus's poles show theirs
//! the same way in `trolley_view`.)

use glam::{DVec3, Vec3};
use omsi_render::{Corona, PointLight, Scene};
use omsi_sim::VehicleInstance;
use omsi_trolley::{Spark, SparkKind};

struct Particle {
    pos: DVec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    heat: f32,
}

struct Flash {
    pos: DVec3,
    age: f32,
    energy: f32,
}

pub struct ArcFx {
    particles: Vec<Particle>,
    flashes: Vec<Flash>,
    rng: u64,
    /// Sparks taken so far (logs and tests).
    pub sparks_seen: u32,
    /// Where the pantograph's head glows while an arc burns, and how hard.
    glow: Option<(DVec3, f32)>,
}

impl Default for ArcFx {
    fn default() -> Self {
        ArcFx { particles: Vec::new(), flashes: Vec::new(), rng: 0x2545_f491_4f6c_dd1d, sparks_seen: 0, glow: None }
    }
}

impl ArcFx {
    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        ((self.rng >> 40) as f32) / (1u32 << 24) as f32
    }

    /// Per frame: the arcs the vehicle's pantograph struck since the last frame, and the
    /// sparks flying.
    pub fn update(&mut self, dt: f32, vehicle: &mut VehicleInstance) {
        for s in vehicle.take_pantograph_sparks() {
            self.add(s);
        }
        self.glow = vehicle.pantograph().filter(|g| g.out.arc > 0.0).and_then(|g| vehicle.pantograph_head().map(|p| (p, g.out.arc)));
        self.advance(dt);
    }

    pub fn add(&mut self, s: Spark) {
        self.sparks_seen += 1;
        let n = ((s.energy.sqrt() * 1.1) as usize).clamp(2, 40);
        let (speed, heat0) = match s.kind {
            SparkKind::Dewire => (7.0, 1.0),
            SparkKind::Insulator => (5.5, 1.0),
            SparkKind::Break => (4.5, 0.9),
            SparkKind::Frog => (3.5, 0.7),
            SparkKind::Make => (3.0, 0.6),
        };
        for _ in 0..n {
            let (a, b, c) = (self.rand(), self.rand(), self.rand());
            let az = a * std::f32::consts::TAU;
            let up = b * 0.9 - 0.25;
            let sp = speed * (0.35 + 0.65 * c);
            let vel = Vec3::new(az.cos() * (1.0 - up.abs() * 0.5), az.sin() * (1.0 - up.abs() * 0.5), up) * sp;
            let life = 0.12 + 0.35 * self.rand();
            let size = 0.03 + 0.035 * self.rand();
            self.particles.push(Particle { pos: s.pos, vel, age: 0.0, life, size, heat: heat0 });
        }
        if self.particles.len() > 600 {
            let extra = self.particles.len() - 600;
            self.particles.drain(0..extra);
        }
        self.flashes.push(Flash { pos: s.pos, age: 0.0, energy: s.energy });
    }

    fn advance(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            p.vel.z -= 9.81 * 0.9 * dt;
            p.vel *= (1.0 - 1.2 * dt).max(0.0);
            p.pos += (p.vel * dt).as_dvec3();
        }
        self.particles.retain(|p| p.age < p.life);
        for f in &mut self.flashes {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < 0.18);
    }

    /// The glare and light of this frame's sparks and arcs.
    pub fn push_effects(&self, scene: &mut Scene) {
        for p in &self.particles {
            let t = p.age / p.life;
            let heat = p.heat * (1.0 - t);
            let color = [1.0, 0.55 + 0.4 * heat, 0.25 + 0.65 * heat * heat];
            scene.coronas.push(Corona { position: p.pos, size: p.size * (1.0 + 2.0 * (1.0 - t)), color, brightness: 2.5 * (1.0 - t * t), direction: Vec3::ZERO, cone_cos: -1.0, ..Default::default() });
        }
        for f in &self.flashes {
            let t = f.age / 0.18;
            let k = (1.0 - t) * (1.0 - t);
            let size = 0.12 + f.energy.sqrt() * 0.04;
            scene.coronas.push(Corona { position: f.pos, size, color: [0.8, 0.88, 1.0], brightness: 3.0 * k, direction: Vec3::ZERO, cone_cos: -1.0, ..Default::default() });
            scene.lights.push(PointLight { position: f.pos, radius: 6.0 + f.energy.sqrt() * 0.9, color: [0.7, 0.82, 1.0], intensity: (f.energy.sqrt() / 7.0).min(4.0) * k, ..Default::default() });
        }
        if let Some((p, arc)) = self.glow {
            scene.coronas.push(Corona { position: p, size: 0.08 + 0.1 * arc, color: [0.8, 0.88, 1.0], brightness: 2.5, direction: Vec3::ZERO, cone_cos: -1.0, ..Default::default() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spark(energy: f32) -> Spark {
        Spark { pos: DVec3::new(1.0, 2.0, 6.0), energy, amps: 100.0, kind: SparkKind::Break }
    }

    #[test]
    fn a_spark_throws_particles_and_a_flash_that_die_away() {
        let mut fx = ArcFx::default();
        fx.add(spark(100.0));
        assert_eq!(fx.sparks_seen, 1);
        assert!(fx.particles.len() >= 2 && fx.flashes.len() == 1);
        for _ in 0..100 {
            fx.advance(0.01);
        }
        assert!(fx.particles.is_empty() && fx.flashes.is_empty());
    }

    #[test]
    fn the_sparks_fall_and_the_number_is_bounded() {
        let mut fx = ArcFx::default();
        for _ in 0..200 {
            fx.add(spark(1.0e6));
        }
        assert!(fx.particles.len() <= 600);
        let z0 = fx.particles.iter().map(|p| p.pos.z).sum::<f64>() / fx.particles.len() as f64;
        fx.advance(0.1);
        let z1 = fx.particles.iter().map(|p| p.pos.z).sum::<f64>() / fx.particles.len().max(1) as f64;
        assert!(z1 < z0 + 0.5, "drag and gravity bring them down");
    }

    #[test]
    fn a_stronger_arc_throws_more() {
        let (mut a, mut b) = (ArcFx::default(), ArcFx::default());
        a.add(spark(4.0));
        b.add(spark(400.0));
        assert!(b.particles.len() > a.particles.len());
    }
}
