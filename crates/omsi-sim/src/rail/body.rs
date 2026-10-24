//! A rail car's body on its bogies: three masses-on-springs (heave, pitch, roll) hung on
//! the secondary suspension, pushed by what the car does and by the track it rides.
//!
//! The track puts the car where it is; the body is not fixed to it. It leans out of a bend
//! (the lateral acceleration `v^2 / R` acts at the height of the centre of gravity, the
//! springs give), noses down under braking and up under traction (the same, along the car),
//! and the rails' unevenness - heights of the left and right rail at each bogie - lifts and
//! tips it through the springs and dampers, as a real car rides: low frequencies pass, high
//! ones are filtered. Nothing here is a made-up wobble: every motion is a response of the
//! 2nd-order system to a force or a base motion, and it dies away by its damping.
//!
//! The body's state is worked on in SI units; angles come out in degrees, with the sign of
//! the vehicle's pose (pitch up positive, bank positive to the right).

use super::class::RailClass;

const G: f32 = 9.81;

/// What a class of car is like on its springs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyParams {
    /// Height of the centre of gravity over the rail (m) and over the secondary suspension.
    pub h_cg: f32,
    /// Distance of the bogie pivots (m).
    pub bogie_span: f32,
    /// Distance of the secondary springs across the car (m).
    pub spring_gauge: f32,
    /// Natural frequencies of heave, pitch and roll (Hz) and the damping ratio.
    pub heave_hz: f32,
    pub pitch_hz: f32,
    pub roll_hz: f32,
    pub damping: f32,
    /// Radii of gyration for pitch and roll (m).
    pub r_pitch: f32,
    pub r_roll: f32,
    /// How uneven the rails are, 1 for track in good order, 2 and more for old street track.
    pub quality: f32,
    /// Track in a bend is canted (a railway's outer rail is raised; street track is flat).
    pub canted: bool,
}

impl BodyParams {
    pub fn of(class: RailClass, bogie_span: f32) -> BodyParams {
        let span = bogie_span.clamp(4.0, 24.0);
        match class {
            // rubber-spring bogies, hydraulic dampers, the rails of a street
            RailClass::Tram => BodyParams { h_cg: 0.95, bogie_span: span, spring_gauge: 1.5, heave_hz: 1.25, pitch_hz: 1.45, roll_hz: 1.15, damping: 0.3, r_pitch: span * 0.35, r_roll: 1.0, quality: 1.8, canted: false },
            RailClass::Metro => BodyParams { h_cg: 1.1, bogie_span: span, spring_gauge: 1.9, heave_hz: 1.0, pitch_hz: 1.15, roll_hz: 0.95, damping: 0.3, r_pitch: span * 0.35, r_roll: 1.1, quality: 1.0, canted: true },
            // air or coil springs, a long soft ride, anti-roll bars stiffen the roll
            RailClass::Train => BodyParams { h_cg: 1.5, bogie_span: span, spring_gauge: 2.0, heave_hz: 0.9, pitch_hz: 1.0, roll_hz: 1.0, damping: 0.35, r_pitch: span * 0.35, r_roll: 1.2, quality: 1.0, canted: true },
        }
    }
}

/// The car's motion this step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BodyOut {
    /// The body's rise over the rails' plane (m).
    pub heave: f32,
    pub pitch_deg: f32,
    pub roll_deg: f32,
    /// What a passenger feels, m/s²: along, sideways (positive to the right) and vertically
    /// beside the 1 g.
    pub a_long: f32,
    pub a_side: f32,
    pub a_vert: f32,
}

#[derive(Debug, Clone)]
pub struct Body {
    z: f32,
    zv: f32,
    th: f32,
    thv: f32,
    ph: f32,
    phv: f32,
    /// Distance run, for the track's unevenness.
    s: f64,
    /// The base motions of the last step (to take their rates).
    u_prev: Option<[f32; 3]>,
    phase: [[f64; 2]; 8],
    pub out: BodyOut,
}

impl Default for Body {
    fn default() -> Self {
        Body::new(1)
    }
}

/// Wavelengths of the track's unevenness (m).
const WAVES: [f64; 8] = [2.5, 4.0, 6.0, 9.0, 14.0, 22.0, 35.0, 60.0];

impl Body {
    pub fn new(seed: u64) -> Body {
        let mut x = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let mut phase = [[0.0; 2]; 8];
        for p in phase.iter_mut().flatten() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *p = (x >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU;
        }
        Body { z: 0.0, zv: 0.0, th: 0.0, thv: 0.0, ph: 0.0, phv: 0.0, s: 0.0, u_prev: None, phase, out: BodyOut::default() }
    }

    /// Height of one rail (m) at distance `s` along the track: components over the
    /// wavelengths a car feels, the amplitude growing with the wavelength as a track's
    /// spectrum does (a few millimetres at 30 m, a fraction of one at 3 m for good track).
    pub fn rail_height(&self, s: f64, side: usize, quality: f32) -> f32 {
        let tau = std::f64::consts::TAU;
        let mut z = 0.0f64;
        for (i, w) in WAVES.iter().enumerate() {
            let a = 0.00035 * w.sqrt();
            z += a * (s * tau / w + self.phase[i][side]).sin();
        }
        z as f32 * quality * 0.55
    }

    /// One step. `a_long` is the car's acceleration along the track, `yaw_rate` how fast its
    /// heading turns to the right (rad/s, negative to the left), both known to the caller.
    pub fn step(&mut self, p: &BodyParams, dt: f32, speed: f32, a_long: f32, yaw_rate: f32) -> BodyOut {
        let dt = dt.clamp(0.0, 0.1);
        if dt <= 0.0 {
            return self.out;
        }
        // the lateral acceleration towards the right of the bend; a railway's raised outer rail
        // takes up to a tenth of the g off it
        let mut a_side = speed * yaw_rate;
        if p.canted {
            let cant = (a_side.abs() / G).min(0.1) * G;
            a_side -= cant * a_side.signum();
        }
        let sub = (dt / 0.004).ceil().clamp(1.0, 25.0) as usize;
        let h = dt / sub as f32;
        let (wz, wt, wr) = (6.2832 * p.heave_hz, 6.2832 * p.pitch_hz, 6.2832 * p.roll_hz);
        let zeta = p.damping;
        let sgn = if speed >= 0.0 { 1.0 } else { -1.0 };
        let mut a_vert = 0.0;
        for _ in 0..sub {
            self.s += (speed * h) as f64;
            let half = p.bogie_span as f64 * 0.5 * sgn as f64;
            let (sf, sr) = (self.s + half, self.s - half);
            let (fl, fr) = (self.rail_height(sf, 0, p.quality), self.rail_height(sf, 1, p.quality));
            let (rl, rr) = (self.rail_height(sr, 0, p.quality), self.rail_height(sr, 1, p.quality));
            // base motion at the springs: mean height, pitch of the line of the bogies, cross-level
            let u = [(fl + fr + rl + rr) * 0.25, (((fl + fr) - (rl + rr)) * 0.5) / p.bogie_span * sgn, (((fl + rl) - (fr + rr)) * 0.5) / p.spring_gauge];
            let du = match self.u_prev {
                Some(q) => [(u[0] - q[0]) / h, (u[1] - q[1]) / h, (u[2] - q[2]) / h],
                None => [0.0; 3],
            };
            self.u_prev = Some(u);
            // heave: base excitation through spring and damper
            let az = -2.0 * zeta * wz * (self.zv - du[0]) - wz * wz * (self.z - u[0]);
            a_vert = az;
            // pitch: the same, and the moment of the acceleration about the springs' plane
            // (static: theta = a h / (r^2 w^2), nose up under traction)
            let m_pitch = a_long * p.h_cg / (p.r_pitch * p.r_pitch);
            let ath = -2.0 * zeta * wt * (self.thv - du[1]) - wt * wt * (self.th - u[1]) + m_pitch;
            // roll: leans away from the bend's centre (bank positive to the right, so a bend to
            // the right, a_side > 0, leans left)
            let m_roll = -a_side * p.h_cg / (p.r_roll * p.r_roll);
            let aph = -2.0 * zeta * wr * (self.phv - du[2]) - wr * wr * (self.ph - u[2]) + m_roll;
            self.zv += az * h;
            self.z += self.zv * h;
            self.thv += ath * h;
            self.th += self.thv * h;
            self.phv += aph * h;
            self.ph += self.phv * h;
        }
        // what is felt: the body's integrated vertical acceleration (the springs act on the
        // body's motion relative to the rails, base excitation included) plus the lean's share
        // of the g
        self.out = BodyOut {
            heave: self.z,
            pitch_deg: self.th.to_degrees(),
            roll_deg: self.ph.to_degrees(),
            a_long,
            a_side: a_side + G * self.ph.sin(),
            a_vert,
        };
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tram() -> BodyParams {
        BodyParams::of(RailClass::Tram, 8.0)
    }

    fn settle(b: &mut Body, p: &BodyParams, secs: f32, v: f32, a: f32, yaw: f32) -> BodyOut {
        let mut out = BodyOut::default();
        for _ in 0..(secs / 0.01) as usize {
            out = b.step(p, 0.01, v, a, yaw);
        }
        out
    }

    fn smooth() -> BodyParams {
        BodyParams { quality: 0.0, ..tram() }
    }

    #[test]
    fn braking_dips_the_nose_and_traction_lifts_it() {
        let p = smooth();
        let brake = settle(&mut Body::new(1), &p, 6.0, 10.0, -1.3, 0.0);
        let pull = settle(&mut Body::new(1), &p, 6.0, 10.0, 1.2, 0.0);
        assert!(brake.pitch_deg < -0.05 && brake.pitch_deg > -0.8, "{}", brake.pitch_deg);
        assert!(pull.pitch_deg > 0.05 && pull.pitch_deg < 0.8, "{}", pull.pitch_deg);
    }

    #[test]
    fn a_bend_to_the_right_leans_the_car_to_the_left() {
        let p = smooth();
        // 8 m/s round a 50 m bend: 1.28 m/s² sideways
        let out = settle(&mut Body::new(1), &p, 8.0, 8.0, 0.0, 8.0 / 50.0);
        assert!(out.roll_deg < -0.8 && out.roll_deg > -4.0, "{}", out.roll_deg);
        let left = settle(&mut Body::new(1), &p, 8.0, 8.0, 0.0, -8.0 / 50.0);
        assert!((left.roll_deg + out.roll_deg).abs() < 0.05, "mirror image: {} {}", left.roll_deg, out.roll_deg);
    }

    #[test]
    fn a_railway_cant_takes_most_of_a_mild_bends_lean_away() {
        let street = BodyParams::of(RailClass::Tram, 8.0);
        let line = BodyParams { quality: 0.0, ..BodyParams::of(RailClass::Train, 8.0) };
        let flat = settle(&mut Body::new(1), &BodyParams { quality: 0.0, ..street }, 8.0, 20.0, 0.0, 20.0 / 800.0);
        let canted = settle(&mut Body::new(1), &line, 8.0, 20.0, 0.0, 20.0 / 800.0);
        assert!(flat.roll_deg.abs() > 0.3);
        assert!(canted.roll_deg.abs() < 0.02, "a bend of 800 m at 72 km/h is balanced by its cant: {}", canted.roll_deg);
    }

    #[test]
    fn a_car_on_smooth_straight_track_stands_still_on_its_springs() {
        let p = smooth();
        let out = settle(&mut Body::new(1), &p, 5.0, 15.0, 0.0, 0.0);
        assert!(out.heave.abs() < 1e-5 && out.pitch_deg.abs() < 1e-4 && out.roll_deg.abs() < 1e-4);
    }

    #[test]
    fn real_track_moves_the_car_by_millimetres_and_tenths_of_a_degree() {
        let p = tram();
        let mut b = Body::new(3);
        let (mut z2, mut r2, mut n) = (0.0f32, 0.0f32, 0.0f32);
        for k in 0..6000 {
            let o = b.step(&p, 0.01, 12.0, 0.0, 0.0);
            if k > 500 {
                z2 += o.heave * o.heave;
                r2 += o.roll_deg * o.roll_deg;
                n += 1.0;
            }
        }
        let (z, r) = ((z2 / n).sqrt(), (r2 / n).sqrt());
        assert!(z > 0.0003 && z < 0.012, "heave rms {z} m");
        assert!(r > 0.003 && r < 0.4, "roll rms {r} deg");
        // deterministic
        let mut c = Body::new(3);
        let mut last = BodyOut::default();
        for _ in 0..6000 {
            last = c.step(&p, 0.01, 12.0, 0.0, 0.0);
        }
        assert_eq!(last, b.out);
    }

    #[test]
    fn a_stationary_car_on_uneven_track_feels_no_vertical_acceleration() {
        let p = tram();
        let mut b = Body::new(3);
        // run to a point where the rails are uneven, then stand
        for _ in 0..300 {
            b.step(&p, 0.01, 12.0, 0.0, 0.0);
        }
        let mut out = BodyOut::default();
        for _ in 0..4000 {
            out = b.step(&p, 0.01, 0.0, 0.0, 0.0);
        }
        assert!(out.heave.abs() > 1e-4, "the rails here are uneven: {}", out.heave);
        assert!(out.a_vert.abs() < 1e-3, "settled on its springs: {}", out.a_vert);
    }

    #[test]
    fn a_disturbance_dies_away_and_does_not_ring_for_ever() {
        let p = smooth();
        let mut b = Body::new(1);
        settle(&mut b, &p, 4.0, 10.0, -1.3, 0.0);
        let after = settle(&mut b, &p, 8.0, 10.0, 0.0, 0.0);
        assert!(after.pitch_deg.abs() < 0.01, "{}", after.pitch_deg);
    }
}
