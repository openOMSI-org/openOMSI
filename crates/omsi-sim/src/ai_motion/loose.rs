//! An AI vehicle knocked off its route by the player (OMSI 2's accident, manual 2.11): it
//! slides and turns on its locked wheels and stays where it ends up until it is cleared.

use super::*;

/// m/s² (about 0.7 g on locked wheels) and deg/s².
const LOOSE_FRICTION: f64 = 7.0;
const LOOSE_YAW_FRICTION: f64 = 120.0;
/// m/s, deg/s, m: a bus at speed sends a car flying, but not out of the loaded world.
const LOOSE_MAX_SPEED: f64 = 25.0;
const LOOSE_MAX_YAW_RATE: f64 = 120.0;
const LOOSE_MAX_OFFSET: f64 = 60.0;

impl AiBody {
    /// A plate of the body's length and width turns by 12 (r × n) Δv / (L² + W²) rad/s. Only
    /// a loose body turns: on its route the driver holds the heading.
    pub fn collision_spin(&mut self, lever: DVec2, direction: DVec2, delta_v: f32, width: f32) {
        if !self.loose || !lever.is_finite() || !direction.is_finite() || !delta_v.is_finite() {
            return;
        }
        let n = direction.normalize_or_zero();
        let length = (self.front_long - self.rear_long).abs().max(1.0) as f64 + 1.5;
        let width = width.max(1.0) as f64;
        // (counter-clockwise positive, while the heading turns clockwise)
        let omega = 12.0 * (lever.x * n.y - lever.y * n.x) * delta_v as f64
            / (length * length + width * width);
        self.impact_yaw_rate = (self.impact_yaw_rate - omega.to_degrees())
            .clamp(-LOOSE_MAX_YAW_RATE, LOOSE_MAX_YAW_RATE);
    }

    pub fn set_loose(&mut self, loose: bool) {
        if self.loose && !loose {
            self.impact_offset = DVec2::ZERO;
            self.impact_velocity = DVec2::ZERO;
            self.impact_yaw = 0.0;
            self.impact_yaw_rate = 0.0;
        }
        self.loose = loose;
    }

    pub fn is_loose(&self) -> bool {
        self.loose
    }

    pub(super) fn max_impact_speed(&self) -> f64 {
        if self.loose { LOOSE_MAX_SPEED } else { 12.0 }
    }

    /// The steps of `settle_collision` for a loose body: it slides and turns on its locked
    /// wheels until they hold it, and nothing draws it back to its route.
    pub(super) fn settle_loose(&mut self, steps: usize, step: f64) {
        for _ in 0..steps {
            let v = self.impact_velocity.length();
            let dv = (LOOSE_FRICTION * step).min(v);
            if v > 0.0 {
                self.impact_velocity -= self.impact_velocity / v * dv;
            }
            self.impact_offset += self.impact_velocity * step;
            let r = self.impact_yaw_rate;
            let dr = (LOOSE_YAW_FRICTION * step).min(r.abs());
            self.impact_yaw_rate = r - r.signum() * dr;
            self.impact_yaw = (self.impact_yaw + self.impact_yaw_rate * step).rem_euclid(360.0);
        }
        self.impact_offset = self.impact_offset.clamp_length_max(LOOSE_MAX_OFFSET);
        self.position.x += self.impact_offset.x;
        self.position.y += self.impact_offset.y;
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::golf;
    use super::*;

    #[test]
    fn a_loose_car_slides_off_its_path_and_stays_there() {
        let mut body = AiBody::new(&golf(), MotionKind::Road);
        body.place(&|_| DVec3::ZERO, None, None, 0.0);
        body.set_loose(true);
        let dv = body.collision_impulse(DVec2::new(1.0, 0.0), 10.0, 12_000.0, 1_000.0);
        assert!(dv > 9.0, "the car takes the bus's blow: {dv}");
        body.collision_spin(DVec2::new(0.0, 2.0), DVec2::new(1.0, 0.0), dv, 1.8);
        assert!(
            body.impact_yaw_rate > 0.0,
            "hit at the front from the left, it turns clockwise"
        );
        for _ in 0..300 {
            body.step(0.02, 0.0, &|_| DVec3::ZERO, None, None);
        }
        assert!(
            body.impact_velocity.length() < 1e-9,
            "stopped by its locked wheels"
        );
        let x = body.position.x;
        assert!(x > 5.0, "it slid off its path: {x:.2} m");
        assert!(
            body.impact_yaw > 10.0 && body.impact_yaw < 350.0,
            "turned {:.1} deg",
            body.impact_yaw
        );
        for _ in 0..100 {
            body.step(0.02, 0.0, &|_| DVec3::ZERO, None, None);
        }
        assert_eq!(body.position.x, x, "nothing draws a loose car back");
        body.set_loose(false);
        body.step(0.02, 0.0, &|_| DVec3::ZERO, None, None);
        assert!(
            body.position.x.abs() < 1e-6,
            "cleared, it stands on its way again"
        );
        assert_eq!(body.impact_yaw, 0.0);
    }

    #[test]
    fn a_car_on_its_route_does_not_spin() {
        let mut body = AiBody::new(&golf(), MotionKind::Road);
        body.place(&|_| DVec3::ZERO, None, None, 0.0);
        body.collision_spin(DVec2::new(0.0, 2.0), DVec2::new(1.0, 0.0), 5.0, 1.8);
        assert_eq!(body.impact_yaw_rate, 0.0);
    }
}
