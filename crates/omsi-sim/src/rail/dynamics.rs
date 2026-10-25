//! A consist on its track: the forces along the rails and the speed they make.
//!
//! The track fixes where the vehicle is and which way it points; this says how fast it
//! goes. The forces are those of railway engineering, not numbers picked for a vehicle:
//!
//! * the running resistance of the gear (rolling and journals, rising with speed) and of the
//!   air (the front, and the skin of every car along the train's length), in a tunnel's
//!   narrower air twice as much;
//! * the curve resistance (Röckl): the wheels of a bogie are dragged across the rails in
//!   a bend, 650 / (R - 55) N per kN of weight above 300 m and 500 / (R - 30) below;
//! * the grade, weight times its slope;
//! * the adhesion between wheel and rail, which limits what the driven wheels can pass on
//!   (Curtius and Kniffler's 0.161 + 7.5 / (v + 44) with v in km/h, less on wet, leafy,
//!   snowy or icy rails) and what the brakes can hold before the wheels slide;
//! * the drive and the brakes themselves, from the scripts, or from the class's typical
//!   performance where there are none.
//!
//! Rotating parts - wheels, motors, gears - add some 6 % to the mass that accelerates.

use super::class::RailClass;

/// What is moved.
#[derive(Debug, Clone, Copy)]
pub struct Consist {
    pub class: RailClass,
    /// Total mass of all cars (kg).
    pub mass: f32,
    /// The share of the weight on driven axles, 0..1.
    pub driven_share: f32,
    pub cars: usize,
    /// Drag coefficient times frontal area of the leading car (m²) and the length of the
    /// train (m).
    pub cd_a: f32,
    pub length: f32,
}

/// What the track and the weather put in the way.
#[derive(Debug, Clone, Copy)]
pub struct Conditions {
    /// Rise per metre of track in the direction of travel (negative downhill).
    pub grade: f32,
    /// Radius of the curve (m), infinite on the straight.
    pub radius: f32,
    /// The rails' state as `StreetCond`: 0 dry, 1 wet, 2 snow.
    pub wetness: f32,
    pub temperature: f32,
    pub in_tunnel: bool,
    /// Air density (kg/m³) and the wind along the track against the direction of travel
    /// (m/s, positive = headwind).
    pub air_density: f32,
    pub headwind: f32,
    /// Sand on the rails.
    pub sanding: bool,
}

impl Default for Conditions {
    fn default() -> Self {
        Conditions { grade: 0.0, radius: f32::INFINITY, wetness: 0.0, temperature: 15.0, in_tunnel: false, air_density: 1.2, headwind: 0.0, sanding: false }
    }
}

/// What the vehicle's drive and brakes ask for, in N (drive positive forward of the
/// vehicle; brakes sizes that oppose the motion).
#[derive(Debug, Clone, Copy, Default)]
pub struct Forces {
    pub drive: f32,
    /// The wheel brakes: limited by what the rails hold.
    pub brake: f32,
    /// A brake that does not work through the wheels - a tram's magnetic track brake, an
    /// eddy-current brake - and so is not limited by adhesion.
    pub track_brake: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Outcome {
    /// Speed along the track after the step (m/s, forward positive).
    pub speed: f32,
    /// Acceleration along the track (m/s²).
    pub accel: f32,
    /// The driven wheels spin: the drive asked for more than the rails give.
    pub spinning: bool,
    /// The brakes held more than the rails give: the wheels slide.
    pub sliding: bool,
    /// Total resistance at this speed (N, positive).
    pub resistance: f32,
    /// The greatest force any coupling of the train carries (N); 0 without a coupler model.
    pub coupler_force: f32,
}

const G: f32 = 9.81;
/// Mass of the rotating parts as a share of the whole.
const ROTATING: f32 = 0.06;

/// Adhesion coefficient between a wheel and a rail at `v` m/s.
pub fn adhesion(v: f32, wetness: f32, temperature: f32, sanding: bool) -> f32 {
    let kmh = v.abs() * 3.6;
    // Curtius and Kniffler for clean dry rails (0.161 + 7.5 / (v + 44)), the "good
    // conditions" figure; the slip of a driving wheel is accounted for by the limit
    let dry = 0.161 + 7.5 / (kmh + 44.0);
    // wet rails 0.7 of it, leaves or snow 0.4, ice 0.2
    let w = wetness.clamp(0.0, 2.0);
    let mut factor = if w <= 1.0 { 1.0 - 0.3 * w } else { 0.7 - 0.3 * (w - 1.0) };
    if temperature <= 0.0 && w > 0.05 {
        factor = factor.min(0.2 + 0.5 * (1.0 - ((0.5 - temperature) / 3.0).clamp(0.0, 1.0)));
    }
    // sand brings a wet rail back to near dry
    let mu = dry * factor;
    if sanding { mu.max(dry * 0.85).min(dry) } else { mu }
}

/// Curve resistance, as a fraction of the weight (Röckl).
pub fn curve_resistance(radius: f32) -> f32 {
    if !radius.is_finite() || radius > 2000.0 {
        return 0.0;
    }
    let r = radius.max(30.0);
    let per_kn = if r >= 300.0 { 650.0 / (r - 55.0) } else { 500.0 / (r - 30.0).max(10.0) };
    per_kn * 1e-3
}

/// Resistance to motion at speed `v` (m/s, any sign), N, positive, not counting the grade.
pub fn resistance(c: &Consist, cond: &Conditions, v: f32) -> f32 {
    let (a, b) = c.class.running_resistance();
    let weight = c.mass * G;
    // a train that stands needs more to start: the lubricant has been squeezed from the
    // journals, the wheels have settled on the rails
    let start = 1.0 + 0.8 / (1.0 + (v.abs() / 0.6).powi(2));
    let run = weight * (a + b * v.abs()) * start;
    let curve = weight * curve_resistance(cond.radius) * if c.class == RailClass::Tram { 1.2 } else { 1.0 };
    // the air: the front, plus the skin friction along the length (a car of 20 m has about
    // 0.4 m² of "front" in friction), twice in a tunnel
    let rel = v.abs() + cond.headwind * v.signum();
    let skin = 0.4 * c.cars.saturating_sub(1) as f32;
    let air = 0.5 * cond.air_density * (c.cd_a + skin) * rel * rel.abs() * if cond.in_tunnel { 2.0 } else { 1.0 };
    run + curve + air
}

/// One step of `dt` seconds. The drive and the brake are limited by adhesion; what is left
/// accelerates the train (with the rotating parts' share added to its mass) against the
/// resistance and the grade. A train at rest stays at rest unless the drive overcomes the
/// grade and the starting resistance.
pub fn step(c: &Consist, cond: &Conditions, f: &Forces, v0: f32, dt: f32) -> Outcome {
    let dt = dt.clamp(0.0, 0.25);
    let m = c.mass.max(1000.0);
    let m_eff = m * (1.0 + ROTATING);
    let weight = m * G;
    let sub = ((dt / 0.01).ceil() as usize).clamp(1, 25);
    let h = dt / sub as f32;
    let mut v = v0;
    let (mut spinning, mut sliding) = (false, false);
    let mut resist_now = 0.0;
    for _ in 0..sub {
        let mu = adhesion(v, cond.wetness, cond.temperature, cond.sanding);
        // what the driven wheels hold, and what the brakes of every wheel hold
        let traction_limit = mu * weight * c.driven_share.clamp(0.05, 1.0);
        let brake_limit = 0.9 * mu * weight;
        let drive = f.drive.clamp(-traction_limit, traction_limit);
        spinning |= f.drive.abs() > traction_limit * 1.02;
        // wheels that slide hold less than wheels that grip
        let brake_req = f.brake.max(0.0);
        let brake = if brake_req > brake_limit * 1.02 {
            sliding = true;
            brake_limit * 0.75
        } else {
            brake_req
        } + f.track_brake.max(0.0);
        let grade_force = weight * cond.grade.clamp(-0.2, 0.2);
        let resist = resistance(c, cond, v);
        resist_now = resist;
        // everything that pushes along the track except what opposes the motion
        let push = drive - grade_force;
        if v.abs() < 0.02 {
            // at rest: holds if the push is within what holds it, else it starts
            let hold = resist + brake;
            if push.abs() <= hold {
                v = 0.0;
                continue;
            }
            let net = push - hold * push.signum();
            v += net / m_eff * h;
        } else {
            let net = push - v.signum() * (resist + brake);
            let nv = v + net / m_eff * h;
            // resistances and brakes stop a train; they do not turn it round
            if nv.signum() != v.signum() && (push - v.signum() * 0.0).abs() <= resist + brake {
                v = 0.0;
            } else {
                v = nv;
            }
        }
    }
    Outcome { speed: v, accel: (v - v0) / dt.max(1e-4), spinning, sliding, resistance: resist_now, coupler_force: 0.0 }
}

/// The drive and brake a class gives a pedal-driven vehicle (one whose scripts give none):
/// full throttle is the starting effort up to the speed where the rated power runs out
/// (F = P / v), the brake pedal a share of the service deceleration, the last tenth of it
/// the emergency one.
pub fn pedal_forces(class: RailClass, mass: f32, throttle: f32, brake: f32, v: f32, reverse: bool) -> Forces {
    let p = class.performance();
    let m_eff = mass * (1.0 + ROTATING);
    let power = p.kw_per_tonne * 1000.0 * mass / 1000.0;
    let effort = (p.start_accel * m_eff).min(power / v.abs().max(0.5));
    let drive = throttle.clamp(0.0, 1.0) * effort * if reverse { -1.0 } else { 1.0 };
    let b = brake.clamp(0.0, 1.0);
    let decel = if b < 0.9 { p.service_brake * (b / 0.9) } else { p.service_brake + (p.emergency_brake - p.service_brake) * (b - 0.9) / 0.1 };
    // a tram's emergency stop is made by its magnetic track brake besides the wheels' own
    // service brake, which the rails could not hold alone
    if class == RailClass::Tram && b >= 0.9 {
        Forces { drive: 0.0, brake: p.service_brake * m_eff, track_brake: (decel - p.service_brake) * m_eff }
    } else {
        Forces { drive, brake: decel * m_eff, track_brake: 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tram() -> Consist {
        Consist { class: RailClass::Tram, mass: 36_000.0, driven_share: 1.0, cars: 2, cd_a: 6.0, length: 28.0 }
    }

    fn train() -> Consist {
        Consist { class: RailClass::Train, mass: 400_000.0, driven_share: 0.25, cars: 8, cd_a: 8.0, length: 160.0 }
    }

    /// Seconds and metres to run `secs` with `forces` from `v0`.
    fn run(c: &Consist, cond: &Conditions, mut forces: impl FnMut(f32) -> Forces, v0: f32, secs: f32) -> (f32, f32, Outcome) {
        let (mut v, mut s, mut last) = (v0, 0.0, Outcome::default());
        let dt = 0.02;
        for _ in 0..(secs / dt) as usize {
            last = step(c, cond, &forces(v), v, dt);
            v = last.speed;
            s += v * dt;
        }
        (v, s, last)
    }

    #[test]
    fn adhesion_follows_the_figures_of_railway_engineering() {
        // dry rail: 0.33 at a standstill, 0.20 at 100 km/h, 0.17 at 300
        assert!((adhesion(0.0, 0.0, 15.0, false) - 0.33).abs() < 0.01);
        assert!((adhesion(27.8, 0.0, 15.0, false) - 0.21).abs() < 0.01);
        assert!(adhesion(83.0, 0.0, 15.0, false) < 0.19);
        // wet rails give 70 %, leaves and snow less, ice least; sand restores
        let dry = adhesion(10.0, 0.0, 15.0, false);
        assert!((adhesion(10.0, 1.0, 15.0, false) / dry - 0.7).abs() < 0.01);
        assert!(adhesion(10.0, 2.0, 5.0, false) < dry * 0.45);
        assert!(adhesion(10.0, 1.0, -4.0, false) < dry * 0.25);
        assert!(adhesion(10.0, 1.0, 15.0, true) > adhesion(10.0, 1.0, 15.0, false));
    }

    #[test]
    fn curves_cost_what_roeckl_says() {
        assert_eq!(curve_resistance(f32::INFINITY), 0.0);
        // 650 / (R - 55) N/kN: 2.65 at 300 m, and a tram's 25 m curve is the sharpest
        assert!((curve_resistance(300.0) - 0.00265).abs() < 1e-4);
        assert!(curve_resistance(100.0) > curve_resistance(300.0) * 2.0);
        assert!(curve_resistance(25.0).is_finite());
    }

    #[test]
    fn a_train_at_speed_feels_its_air_and_its_length() {
        let c = train();
        let cond = Conditions::default();
        let low = resistance(&c, &cond, 5.0);
        let high = resistance(&c, &cond, 40.0);
        assert!(high > low * 3.0);
        // 8 cars at 144 km/h: Davis-like 8-12 kN for a 400 t train
        assert!(high > 8_000.0 && high < 25_000.0, "{high}");
        // a tunnel doubles the air's part
        let tunnel = resistance(&c, &Conditions { in_tunnel: true, ..cond }, 40.0);
        assert!(tunnel > high);
        // standing costs more than rolling slowly
        assert!(resistance(&c, &cond, 0.0) > resistance(&c, &cond, 1.5) * 1.3);
    }

    #[test]
    fn a_tram_accelerates_and_brakes_like_one() {
        let c = tram();
        let cond = Conditions::default();
        let p = RailClass::Tram.performance();
        // full throttle for 10 s: ~1.1 m/s² from rest (start effort minus resistance)
        let (v, _, _) = run(&c, &cond, |v| pedal_forces(RailClass::Tram, c.mass, 1.0, 0.0, v, false), 0.0, 10.0);
        assert!(v > 9.0 && v < 12.5, "{v}");
        // service brake from 50 km/h stops in 5.5 s... 1.3 m/s²: 13.9^2/(2*1.3) = 74 m
        let (v, s, _) = run(&c, &cond, |v| pedal_forces(RailClass::Tram, c.mass, 0.0, 0.9, v, false), 13.9, 20.0);
        assert_eq!(v, 0.0);
        assert!(s > 55.0 && s < 80.0, "stops in {s} m");
        // emergency is harder
        let (_, s_em, _) = run(&c, &cond, |v| pedal_forces(RailClass::Tram, c.mass, 0.0, 1.0, v, false), 13.9, 20.0);
        assert!(s_em < s * 0.6, "{s_em} against {s}");
        assert!(p.emergency_brake > p.service_brake);
    }

    #[test]
    fn a_train_needs_a_long_way_to_stop() {
        let c = train();
        let cond = Conditions::default();
        let (v, s, _) = run(&c, &cond, |v| pedal_forces(RailClass::Train, c.mass, 0.0, 0.9, v, false), 36.0, 90.0);
        assert_eq!(v, 0.0);
        // 130 km/h at 0.9 m/s²: 36²/(1.8) = 720 m
        assert!(s > 600.0 && s < 850.0, "{s}");
    }

    #[test]
    fn wet_rails_make_the_wheels_spin_and_slide() {
        let c = tram();
        let dry = Conditions::default();
        let leaves = Conditions { wetness: 2.0, temperature: 4.0, ..dry };
        let big = Forces { drive: 90_000.0, ..Forces::default() };
        assert!(!step(&c, &dry, &big, 3.0, 0.02).spinning);
        let out = step(&c, &leaves, &big, 3.0, 0.02);
        assert!(out.spinning, "the driven wheels spin on a leafy rail");
        let hard_brake = Forces { brake: 220_000.0, ..Forces::default() };
        assert!(step(&c, &leaves, &hard_brake, 10.0, 0.02).sliding);
        assert!(!step(&c, &dry, &Forces { brake: 70_000.0, ..Forces::default() }, 10.0, 0.02).sliding);
        // slipping gives less than gripping would
        let grip = run(&c, &dry, |_| Forces { drive: 90_000.0, ..Forces::default() }, 0.0, 6.0).0;
        let slip = run(&c, &leaves, |_| Forces { drive: 90_000.0, ..Forces::default() }, 0.0, 6.0).0;
        assert!(slip < grip * 0.8, "{slip} against {grip}");
    }

    #[test]
    fn a_grade_holds_back_a_climbing_train_and_runs_a_free_one_away() {
        let c = train();
        let up = Conditions { grade: 0.03, ..Conditions::default() };
        let down = Conditions { grade: -0.03, ..Conditions::default() };
        // coasting up a 30 per mille grade at 20 m/s decelerates ~0.3 m/s²
        let o = step(&c, &up, &Forces::default(), 20.0, 0.1);
        assert!(o.accel < -0.25 && o.accel > -0.45, "{}", o.accel);
        // down, it runs away to the speed where the air takes it all: well above 100 km/h
        let (v, _, _) = run(&c, &down, |_| Forces::default(), 5.0, 240.0);
        assert!(v > 28.0, "{v}");
        // a standing train on a grade stays put with its brake on, rolls without
        assert_eq!(step(&c, &down, &Forces { brake: 150_000.0, ..Forces::default() }, 0.0, 0.1).speed, 0.0);
        let steep = Conditions { grade: -0.05, ..down };
        let (v, _, _) = run(&c, &steep, |_| Forces::default(), 0.0, 3.0);
        assert!(v > 1.0, "a free train on a 5 % downgrade rolls away: {v}");
    }

    #[test]
    fn a_train_that_stops_on_the_flat_stays_stopped() {
        let c = tram();
        let cond = Conditions::default();
        let mut v = 0.3;
        for _ in 0..500 {
            v = step(&c, &cond, &Forces::default(), v, 0.02).speed;
        }
        assert_eq!(v, 0.0);
        // and is not turned round by its own resistance
        let o = step(&c, &cond, &Forces { brake: 50_000.0, ..Forces::default() }, 0.05, 0.1);
        assert!(o.speed >= 0.0);
    }

    #[test]
    fn a_tight_bend_slows_a_coasting_tram() {
        let c = tram();
        let straight = step(&c, &Conditions::default(), &Forces::default(), 8.0, 1.0);
        let bend = step(&c, &Conditions { radius: 25.0, ..Conditions::default() }, &Forces::default(), 8.0, 1.0);
        assert!(bend.accel < straight.accel * 1.8 && bend.accel < straight.accel - 0.3, "{} against {}", bend.accel, straight.accel);
    }

    #[test]
    fn reversing_drives_backwards_and_the_rest_follows() {
        let c = tram();
        let cond = Conditions::default();
        let (v, _, _) = run(&c, &cond, |v| pedal_forces(RailClass::Tram, c.mass, 1.0, 0.0, v, true), 0.0, 5.0);
        assert!(v < -3.0, "{v}");
    }
}
