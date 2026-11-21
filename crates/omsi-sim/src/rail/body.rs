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

/// How much of the worked-out roll a car shows: the full lean, as true to the springs as it is,
/// looked too much on the screen.
pub const ROLL_SHOWN: f32 = 0.5;

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
    /// How much the sideways acceleration turns the body (rad/s² per m/s²; Omsi.exe's lever
    /// `wagenkasten_hebelarm_Z`) and the damping ratios of the roll, the pitch and the heave
    /// alone (the class's `damping`, or what the car's file gives).
    pub roll_arm: f32,
    pub roll_damping: f32,
    pub pitch_damping: f32,
    pub heave_damping: f32,
    /// How uneven the rails are, 1 for track in good order, 2 and more for old street track.
    pub quality: f32,
    /// Track in a bend is canted (a railway's outer rail is raised; street track is flat).
    pub canted: bool,
    /// Heave, pitch and roll that the car's file switches off: the body follows the rails in
    /// them without a swing of its own, and leans only by its file's lever.
    pub rigid: [bool; 3],
}

impl BodyParams {
    pub fn of(class: RailClass, bogie_span: f32) -> BodyParams {
        let span = bogie_span.clamp(4.0, 24.0);
        let (h_cg, spring_gauge, heave_hz, pitch_hz, roll_hz, damping, r_roll, quality, canted) =
            match class {
                // rubber-spring bogies, hydraulic dampers, the rails of a street
                RailClass::Tram => (0.95, 1.5, 1.25, 1.45, 1.15, 0.3, 1.0, 1.8, false),
                RailClass::Metro => (1.1, 1.9, 1.0, 1.15, 0.95, 0.3, 1.1, 1.0, true),
                // air or coil springs, a long soft ride, anti-roll bars stiffen the roll
                RailClass::Train => (1.5, 2.0, 0.9, 1.0, 1.0, 0.35, 1.2, 1.0, true),
            };
        BodyParams {
            h_cg,
            bogie_span: span,
            spring_gauge,
            heave_hz,
            pitch_hz,
            roll_hz,
            damping,
            r_pitch: span * 0.35,
            r_roll,
            roll_arm: h_cg / (r_roll * r_roll),
            roll_damping: damping,
            pitch_damping: damping,
            heave_damping: damping,
            quality,
            canted,
            rigid: [false; 3],
        }
    }

    /// The class's car with the motions its file gives: `[rail_body_osc]` is the lever, then
    /// angular frequency (rad/s) and decay `d` (a swing dies as e^(-d t / 2), Omsi.exe
    /// 0x7f2a18) of the roll, the pitch and the heave. A motion whose decay is 4 times its
    /// frequency or more, or that has no frequency, is switched off: gone at once, the body
    /// follows the rails in it (the NF6D's `0`, then `1 10000` three times: no lever, no
    /// motion of its own at all). A file without `[rail_body_osc]` rides on the class's
    /// springs.
    pub fn for_car(def: &omsi_vehicle::Vehicle, class: RailClass, bogie_span: f32) -> BodyParams {
        let mut p = BodyParams::of(class, bogie_span);
        let Some([arm, osc @ ..]) = def.rail_body_osc else {
            return p;
        };
        let swings = |omega: f32, d: f32| omega > 0.0 && d >= 0.0 && d < 4.0 * omega;
        let tau = std::f32::consts::TAU;
        p.roll_arm = arm.max(0.0);
        let [roll, pitch, heave] = [0, 1, 2].map(|k| (osc[2 * k], osc[2 * k + 1]));
        if swings(roll.0, roll.1) {
            p.roll_hz = roll.0 / tau;
            p.roll_damping = roll.1 / (2.0 * roll.0);
        } else {
            p.rigid[2] = true;
            p.roll_hz = roll.0.max(0.0) / tau;
        }
        if swings(pitch.0, pitch.1) {
            p.pitch_hz = pitch.0 / tau;
            p.pitch_damping = pitch.1 / (2.0 * pitch.0);
        } else {
            p.rigid[1] = true;
        }
        if swings(heave.0, heave.1) {
            p.heave_hz = heave.0 / tau;
            p.heave_damping = heave.1 / (2.0 * heave.0);
        } else {
            p.rigid[0] = true;
        }
        p
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
        Body {
            z: 0.0,
            zv: 0.0,
            th: 0.0,
            thv: 0.0,
            ph: 0.0,
            phv: 0.0,
            s: 0.0,
            u_prev: None,
            phase,
            out: BodyOut::default(),
        }
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
    pub fn step(
        &mut self,
        p: &BodyParams,
        dt: f32,
        speed: f32,
        a_long: f32,
        yaw_rate: f32,
    ) -> BodyOut {
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
        let sgn = if speed >= 0.0 { 1.0 } else { -1.0 };
        let mut a_vert = 0.0;
        for _ in 0..sub {
            self.s += (speed * h) as f64;
            let half = p.bogie_span as f64 * 0.5 * sgn as f64;
            let (sf, sr) = (self.s + half, self.s - half);
            let (fl, fr) = (
                self.rail_height(sf, 0, p.quality),
                self.rail_height(sf, 1, p.quality),
            );
            let (rl, rr) = (
                self.rail_height(sr, 0, p.quality),
                self.rail_height(sr, 1, p.quality),
            );
            // base motion at the springs: mean height, pitch of the line of the bogies, cross-level
            let u = [
                (fl + fr + rl + rr) * 0.25,
                (((fl + fr) - (rl + rr)) * 0.5) / p.bogie_span * sgn,
                (((fl + rl) - (fr + rr)) * 0.5) / p.spring_gauge,
            ];
            let du = match self.u_prev {
                Some(q) => [(u[0] - q[0]) / h, (u[1] - q[1]) / h, (u[2] - q[2]) / h],
                None => [0.0; 3],
            };
            self.u_prev = Some(u);
            // heave: base excitation through spring and damper
            let az = -2.0 * p.heave_damping * wz * (self.zv - du[0]) - wz * wz * (self.z - u[0]);
            a_vert = az;
            // pitch: the same, and the moment of the acceleration about the springs' plane
            // (static: theta = a h / (r^2 w^2), nose up under traction)
            let m_pitch = a_long * p.h_cg / (p.r_pitch * p.r_pitch);
            let ath = -2.0 * p.pitch_damping * wt * (self.thv - du[1]) - wt * wt * (self.th - u[1])
                + m_pitch;
            // roll: leans away from the bend's centre (bank positive to the right, so a bend to
            // the right, a_side > 0, leans left)
            let m_roll = -a_side * p.roll_arm;
            let aph = -2.0 * p.roll_damping * wr * (self.phv - du[2]) - wr * wr * (self.ph - u[2])
                + m_roll;
            self.zv += az * h;
            self.z += self.zv * h;
            self.thv += ath * h;
            self.th += self.thv * h;
            self.phv += aph * h;
            self.ph += self.phv * h;
            // a motion the file switches off follows the rails, the roll leaning by the
            // file's lever as far as its spring holds it (none without a lever or spring)
            if p.rigid[0] {
                (self.z, self.zv) = (u[0], du[0]);
                a_vert = 0.0;
            }
            if p.rigid[1] {
                (self.th, self.thv) = (u[1], du[1]);
            }
            if p.rigid[2] {
                let lean = if wr > 0.0 { m_roll / (wr * wr) } else { 0.0 };
                (self.ph, self.phv) = (u[2] + lean, du[2]);
            }
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
        BodyParams {
            quality: 0.0,
            ..tram()
        }
    }

    /// A file that switches its body's motions off (the NF6D's `[rail_body_osc]`: no lever,
    /// decay 10000 against a frequency of 1) gets no lean, dip or bounce of its own, not the
    /// class's springs; the same car without the section leans in a bend.
    #[test]
    fn a_body_its_file_switches_off_does_not_lean_or_rock() {
        let mut def = omsi_vehicle::Vehicle {
            rail_body_osc: Some([0.0, 1.0, 10000.0, 1.0, 10000.0, 1.0, 10000.0]),
            ..Default::default()
        };
        let off = BodyParams {
            quality: 0.0,
            ..BodyParams::for_car(&def, RailClass::Tram, 8.0)
        };
        assert_eq!(off.rigid, [true; 3]);
        let out = settle(&mut Body::new(1), &off, 8.0, 8.0, -1.3, 8.0 / 50.0);
        assert!(
            out.roll_deg.abs() < 1e-4 && out.pitch_deg.abs() < 1e-4 && out.heave.abs() < 1e-6,
            "{out:?}"
        );
        // on uneven track it rides the rails: its roll is their cross-level
        let rough = BodyParams::for_car(&def, RailClass::Tram, 8.0);
        let mut b = Body::new(1);
        let out = settle(&mut b, &rough, 4.0, 8.0, 0.0, 0.0);
        assert!(out.roll_deg.abs() < 0.2, "{out:?}");
        def.rail_body_osc = None;
        let class = BodyParams {
            quality: 0.0,
            ..BodyParams::for_car(&def, RailClass::Tram, 8.0)
        };
        let leans = settle(&mut Body::new(1), &class, 8.0, 8.0, 0.0, 8.0 / 50.0);
        assert!(leans.roll_deg < -0.8, "{leans:?}");
    }

    /// A file's own swing is taken: its lever, roll, pitch and heave frequencies.
    #[test]
    fn a_body_its_file_lets_swing_takes_the_files_springs() {
        let def = omsi_vehicle::Vehicle {
            rail_body_osc: Some([0.6, 6.0, 3.0, 8.0, 4.0, 7.0, 3.5]),
            ..Default::default()
        };
        let p = BodyParams::for_car(&def, RailClass::Tram, 8.0);
        assert_eq!(p.rigid, [false; 3]);
        let tau = std::f32::consts::TAU;
        assert!((p.roll_arm - 0.6).abs() < 1e-6);
        assert!((p.roll_hz - 6.0 / tau).abs() < 1e-6);
        assert!((p.pitch_hz - 8.0 / tau).abs() < 1e-6);
        assert!((p.heave_hz - 7.0 / tau).abs() < 1e-6);
        // (a swing dies as e^(-d t / 2): the damping ratio is d / 2ω)
        assert!((p.roll_damping - 0.25).abs() < 1e-6);
        assert!((p.pitch_damping - 0.25).abs() < 1e-6);
        assert!((p.heave_damping - 0.25).abs() < 1e-6);
    }

    /// The pitch and the heave die away by the decay the car's file gives each of them: of
    /// two cars alike but for it, the one with the higher decay comes to rest sooner, and
    /// the swing left after a second is the file's e^(-d t / 2).
    #[test]
    fn a_body_its_file_damps_harder_stops_pitching_and_bouncing_sooner() {
        let car = |d_pitch: f32, d_heave: f32| {
            let def = omsi_vehicle::Vehicle {
                rail_body_osc: Some([0.0, 6.0, 1.0, 8.0, d_pitch, 7.0, d_heave]),
                ..Default::default()
            };
            BodyParams {
                quality: 0.0,
                ..BodyParams::for_car(&def, RailClass::Tram, 8.0)
            }
        };
        // the largest swing in the second after one second, from a start tipped and lifted
        let left = |p: &BodyParams| {
            let mut b = Body::new(1);
            (b.th, b.z) = (0.01, 0.01);
            let (mut pitch, mut heave) = (0.0f32, 0.0f32);
            for i in 0..200 {
                b.step(p, 0.01, 0.0, 0.0, 0.0);
                if i >= 100 {
                    pitch = pitch.max(b.th.abs());
                    heave = heave.max(b.z.abs());
                }
            }
            (pitch / 0.01, heave / 0.01)
        };
        let (soft, hard) = (left(&car(1.0, 1.0)), left(&car(6.0, 6.0)));
        assert!(hard.0 < soft.0 * 0.2, "pitch: {hard:?} against {soft:?}");
        assert!(hard.1 < soft.1 * 0.2, "heave: {hard:?} against {soft:?}");
        // e^(-1 · 1 / 2) = 0.61 and e^(-6 · 1 / 2) = 0.05 of the start, near enough
        assert!(
            (0.45..0.7).contains(&soft.0) && (0.45..0.7).contains(&soft.1),
            "{soft:?}"
        );
        assert!(hard.0 < 0.08 && hard.1 < 0.08, "{hard:?}");
        // and each motion by its own decay
        let mixed = left(&car(6.0, 1.0));
        assert!(mixed.0 < 0.08 && mixed.1 > 0.45, "{mixed:?}");
    }

    #[test]
    fn braking_dips_the_nose_and_traction_lifts_it() {
        let p = smooth();
        let brake = settle(&mut Body::new(1), &p, 6.0, 10.0, -1.3, 0.0);
        let pull = settle(&mut Body::new(1), &p, 6.0, 10.0, 1.2, 0.0);
        assert!(
            brake.pitch_deg < -0.05 && brake.pitch_deg > -0.8,
            "{}",
            brake.pitch_deg
        );
        assert!(
            pull.pitch_deg > 0.05 && pull.pitch_deg < 0.8,
            "{}",
            pull.pitch_deg
        );
    }

    #[test]
    fn a_bend_to_the_right_leans_the_car_to_the_left() {
        let p = smooth();
        // 8 m/s round a 50 m bend: 1.28 m/s² sideways
        let out = settle(&mut Body::new(1), &p, 8.0, 8.0, 0.0, 8.0 / 50.0);
        assert!(
            out.roll_deg < -0.8 && out.roll_deg > -4.0,
            "{}",
            out.roll_deg
        );
        let left = settle(&mut Body::new(1), &p, 8.0, 8.0, 0.0, -8.0 / 50.0);
        assert!(
            (left.roll_deg + out.roll_deg).abs() < 0.05,
            "mirror image: {} {}",
            left.roll_deg,
            out.roll_deg
        );
    }

    #[test]
    fn a_railway_cant_takes_most_of_a_mild_bends_lean_away() {
        let street = BodyParams::of(RailClass::Tram, 8.0);
        let line = BodyParams {
            quality: 0.0,
            ..BodyParams::of(RailClass::Train, 8.0)
        };
        let flat = settle(
            &mut Body::new(1),
            &BodyParams {
                quality: 0.0,
                ..street
            },
            8.0,
            20.0,
            0.0,
            20.0 / 800.0,
        );
        let canted = settle(&mut Body::new(1), &line, 8.0, 20.0, 0.0, 20.0 / 800.0);
        assert!(flat.roll_deg.abs() > 0.3);
        assert!(
            canted.roll_deg.abs() < 0.02,
            "a bend of 800 m at 72 km/h is balanced by its cant: {}",
            canted.roll_deg
        );
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
        assert!(
            out.heave.abs() > 1e-4,
            "the rails here are uneven: {}",
            out.heave
        );
        assert!(
            out.a_vert.abs() < 1e-3,
            "settled on its springs: {}",
            out.a_vert
        );
    }

    #[test]
    fn a_cars_file_sets_its_roll_unless_it_switches_it_off() {
        let car = |osc| omsi_vehicle::Vehicle {
            rail_body_osc: Some(osc),
            ..Default::default()
        };
        // the two-axle tram's: a stiff old car, a tenth of the lever
        let own = BodyParams::for_car(
            &car([0.1, 6.0, 6.0, 15.0, 15.0, 8.0, 4.0]),
            RailClass::Tram,
            8.0,
        );
        assert_eq!(own.roll_arm, 0.1);
        assert!((own.roll_hz - 0.955).abs() < 0.01 && (own.roll_damping - 0.5).abs() < 1e-6);
        // the NF6D's switches it off (see `a_body_its_file_switches_off_does_not_lean_or_rock`);
        // a car without the section rides on the class's springs
        let off = BodyParams::for_car(
            &car([0.0, 1.0, 10000.0, 1.0, 10000.0, 1.0, 10000.0]),
            RailClass::Tram,
            8.0,
        );
        assert_eq!(off.rigid, [true; 3]);
        let class = BodyParams::for_car(&Default::default(), RailClass::Tram, 8.0);
        assert_eq!(class, BodyParams::of(RailClass::Tram, 8.0));
        // both lean, and the street tram of the class by what a real one does: 0.1 to 0.3 of
        // a radian per g
        let lean = |p: BodyParams| {
            settle(
                &mut Body::new(1),
                &BodyParams { quality: 0.0, ..p },
                10.0,
                8.0,
                0.0,
                8.0 / 50.0,
            )
            .roll_deg
            .abs()
        };
        assert!(lean(own) > 0.1 && lean(own) < lean(class));
        let per_g = lean(class).to_radians() / (1.28 / G);
        assert!((0.1..=0.3).contains(&per_g), "{per_g}");
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
