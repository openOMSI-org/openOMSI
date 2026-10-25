//! The cars of a train and the couplings between them.
//!
//! Each car is a mass on the rails; between two cars the coupling has a free play (`slack`) in
//! which it pulls and pushes nothing, then a draft gear (a spring with damping). A train that is
//! started takes up its slack car by car and jerks; one that is braked from the head runs in. The
//! force a coupling carries is checked against its strength: beyond it the train breaks in two.

use omsi_vehicle::rail_spec::RailCoupler;

#[derive(Debug, Clone)]
pub struct Chain {
    pub coupler: RailCoupler,
    pub mass: Vec<f32>,
    /// Speed of each car along the track (m/s), the head first.
    pub speed: Vec<f32>,
    /// How far each coupling is stretched (m, positive = the cars are apart); within the slack it
    /// carries nothing. Between car `i` and `i + 1`.
    pub gap: Vec<f32>,
    /// The force of each coupling (N, positive = pulling).
    pub force: Vec<f32>,
    /// The first coupling that was overloaded, if the train has broken.
    pub broken: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ChainOut {
    /// The mean speed of the cars (m/s).
    pub speed: f32,
    /// The greatest force in any coupling (N), pulling or pushing.
    pub peak: f32,
}

impl Chain {
    pub fn new(coupler: RailCoupler, mass: Vec<f32>, v0: f32) -> Chain {
        let n = mass.len().max(1);
        Chain { coupler, mass: if mass.is_empty() { vec![1000.0] } else { mass }, speed: vec![v0; n], gap: vec![0.0; n.saturating_sub(1)], force: vec![0.0; n.saturating_sub(1)], broken: None }
    }

    /// One step of `dt`; `external[i]` is the net force on car `i` from outside the couplings
    /// (drive, brake, resistance, grade; positive forward). The cars hold their couplings'
    /// stretch from step to step, so the slack is taken up over time.
    pub fn step(&mut self, dt: f32, external: &[f32]) -> ChainOut {
        let n = self.mass.len();
        let sub = ((dt / 0.002).ceil() as usize).clamp(1, 100);
        let h = dt.clamp(0.0, 0.25) / sub as f32;
        let half = self.coupler.slack * 0.5;
        let mut peak = 0.0f32;
        for _ in 0..sub {
            for i in 0..n - 1 {
                // car i is ahead of car i+1: the coupling stretches when the front car is faster
                let rel = self.speed[i] - self.speed[i + 1];
                self.gap[i] += rel * h;
                // free play: between -half and +half nothing; beyond it the draft gear works
                let g = self.gap[i];
                let f = if g > half {
                    (self.coupler.stiffness * (g - half) + self.coupler.damping * rel.max(0.0)).max(0.0)
                } else if g < -half {
                    (self.coupler.stiffness * (g + half) + self.coupler.damping * rel.min(0.0)).min(0.0)
                } else {
                    0.0
                };
                self.force[i] = f;
                peak = peak.max(f.abs());
            }
            for i in 0..n {
                let mut f = external.get(i).copied().unwrap_or(0.0);
                if i > 0 {
                    // the coupling behind car i-1 pulls it back, pulls car i forward
                    f += self.force[i - 1];
                }
                if i + 1 < n {
                    f -= self.force[i];
                }
                self.speed[i] += f / self.mass[i] * h;
            }
        }
        if self.broken.is_none() {
            self.broken = self.force.iter().position(|f| f.abs() > self.coupler.max_force);
        }
        ChainOut { speed: self.speed.iter().zip(&self.mass).map(|(v, m)| v * m).sum::<f32>() / self.mass.iter().sum::<f32>(), peak }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::rail_spec::CouplerKind;

    fn train(cars: usize, kind: CouplerKind) -> Chain {
        Chain::new(RailCoupler::typical(kind), vec![40_000.0; cars], 0.0)
    }

    fn pull_head(c: &mut Chain, force: f32, secs: f32) -> f32 {
        let mut peak = 0.0f32;
        for _ in 0..(secs / 0.01) as usize {
            let mut ext = vec![0.0; c.mass.len()];
            ext[0] = force;
            peak = peak.max(c.step(0.01, &ext).peak);
        }
        peak
    }

    #[test]
    fn starting_takes_up_the_slack_car_by_car() {
        let mut c = train(6, CouplerKind::Screw);
        // 60 kN at the head of 240 t: ~0.25 m/s²
        pull_head(&mut c, 60_000.0, 0.4);
        // the head is moving already; the last car, with 0.3 m of slack in each of five couplings, not yet
        assert!(c.speed[0] > c.speed[5] + 0.03, "head {} tail {}", c.speed[0], c.speed[5]);
        assert!(c.gap[0] > c.gap[4], "the front coupling is stretched first");
        pull_head(&mut c, 60_000.0, 20.0);
        // in the end they all run together
        assert!((c.speed[0] - c.speed[5]).abs() < 0.05, "head {} tail {}", c.speed[0], c.speed[5]);
        let a = c.speed[5] / 20.4;
        assert!((a - 0.25).abs() < 0.04, "{a}");
    }

    #[test]
    fn a_tight_coupler_jerks_less_than_a_screw_one() {
        let jerk = |kind| {
            let mut c = train(6, kind);
            pull_head(&mut c, 80_000.0, 8.0)
        };
        let (screw, tight) = (jerk(CouplerKind::Screw), jerk(CouplerKind::Scharfenberg));
        // (more slack = more run-up before the pull comes: a bigger snatch)
        assert!(screw > tight, "screw {screw} tight {tight}");
    }

    #[test]
    fn braking_from_the_head_runs_the_cars_in() {
        let mut c = train(4, CouplerKind::Buffer);
        c.speed = vec![10.0; 4];
        let mut peak = 0.0f32;
        for _ in 0..600 {
            let mut ext = vec![0.0; 4];
            ext[0] = -150_000.0;
            peak = peak.max(c.step(0.01, &ext).peak);
        }
        assert!(c.force.iter().all(|f| *f <= 1.0), "buffers push when the head slows: {:?}", c.force);
        assert!(peak > 10_000.0, "{peak}");
    }

    #[test]
    fn too_great_a_pull_breaks_the_train() {
        let mut c = train(3, CouplerKind::Screw);
        pull_head(&mut c, 2_000_000.0, 3.0);
        assert!(c.broken.is_some());
        let mut ok = train(3, CouplerKind::Screw);
        pull_head(&mut ok, 80_000.0, 10.0);
        assert!(ok.broken.is_none());
    }

    #[test]
    fn one_car_is_just_a_mass() {
        let mut c = Chain::new(RailCoupler::default(), vec![20_000.0], 0.0);
        let mut out = ChainOut::default();
        for _ in 0..10 {
            out = c.step(0.1, &[20_000.0]);
        }
        assert!((out.speed - 1.0).abs() < 0.01);
    }
}
