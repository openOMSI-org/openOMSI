//! What the road and the air do to a road vehicle, from physics rather than from numbers
//! picked for one bus.
//!
//! * The air: density from the temperature, drag and side force from the vehicle's own size
//!   (its `[boundingbox]`) and the wind relative to it (`air_force`).
//! * The road: grip and rolling resistance of the surface under each tyre (the `[surface]`
//!   ids of OMSI 2's textures), wet or dry, and the road's unevenness, which is what shakes
//!   a bus on cobbles and on a concrete-slab road (`roughness`).
//!
//! Everything is a pure function of what the `.bus` file and the map already say, so a mod
//! bus needs nothing of its own: a vehicle whose file gives no size gets the size its mass
//! suggests, and a surface id outside OMSI's eight is asphalt.

use glam::Vec3;

/// Specific gas constant of dry air (J/kg/K).
const GAS_CONSTANT: f32 = 287.05;
/// Sea level pressure (Pa).
const SEA_LEVEL_PRESSURE: f32 = 101_325.0;

/// Air density (kg/m³) at `temp_c` degrees and `altitude` metres above sea level (ISA
/// pressure): 1.20 at 20 °C, 1.29 at freezing.
pub fn air_density(temp_c: f32, altitude: f32) -> f32 {
    let p = SEA_LEVEL_PRESSURE * (1.0 - 2.255_77e-5 * altitude.clamp(-400.0, 4000.0)).powf(5.255_88);
    p / (GAS_CONSTANT * (temp_c.clamp(-60.0, 60.0) + 273.15))
}

/// How a vehicle presents itself to the air, in its own frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aero {
    /// Drag coefficient times frontal area (m²) and side coefficient times side area (m²).
    pub cd_a: f32,
    pub cs_a: f32,
    /// Where the side force acts against the centre of gravity: the height above it that
    /// rolls the body, and the distance ahead of it (the centre of pressure of a bus lies
    /// forward of its middle) that turns it into the wind.
    pub lever: f32,
    pub arm: f32,
}

impl Aero {
    /// From the `[boundingbox]` (`size` = width, length, height) or, for a file without one,
    /// from the mass: a vehicle of 6 t and more is taken as a bus, below as a car or a van.
    ///
    /// The drag coefficient rises with the height of the body, as the shape does: 0.30 for a
    /// car of 1.4 m, 0.65 for a bus of 3 m (a flat front, wheel arches, a square tail: the
    /// 0.55-0.75 measured on city buses). The frontal area is the box less the ground
    /// clearance and the rounded top corners; the side area the length times the same.
    pub fn of(size: Option<[f32; 3]>, mass_kg: f32) -> Aero {
        let [width, length, height] = size.filter(|s| s.iter().all(|x| x.is_finite() && *x > 0.3)).unwrap_or(if mass_kg >= 6000.0 {
            [2.5, (mass_kg / 1000.0).clamp(6.0, 18.0) + 2.0, 3.0]
        } else {
            [1.8, 4.5, 1.5]
        });
        let body = (height - 0.25).max(0.5);
        let cd = 0.30 + 0.35 * ((height - 1.5) / 1.5).clamp(0.0, 1.0);
        Aero { cd_a: cd * width * body * 0.92, cs_a: 0.9 * length * body * 0.9, lever: 0.1 * height, arm: 0.05 * length }
    }
}

/// The force (N, body frame: x right, y forward) the air puts on a vehicle moving at
/// `v_body` through air at `wind_body` (both in the body frame, m/s), and the torque it
/// makes about the centre of gravity (N m, body frame: y rolls the body, positive to the
/// right, z turns it, positive to the left).
pub fn air_force(aero: &Aero, rho: f32, v_body: Vec3, wind_body: Vec3) -> (Vec3, Vec3) {
    let rel = Vec3::new(v_body.x - wind_body.x, v_body.y - wind_body.y, 0.0);
    let speed = rel.length();
    if speed < 1e-3 {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    let q = 0.5 * rho * speed;
    let force = Vec3::new(-q * aero.cs_a * rel.x, -q * aero.cd_a * rel.y, 0.0);
    // r x F for a side force acting `lever` above and `arm` ahead of the centre of gravity
    let torque = Vec3::new(0.0, aero.lever * force.x, -aero.arm * force.x);
    (force, torque)
}

/// Density (kg/m3) of the air the scripts of the OMSI 2 buses (bremse.osc) take for their drag.
pub const SCRIPT_DENSITY: f32 = 1.2;

/// What the weather adds to the air drag the vehicle's own scripts already put on its brakes
/// (those take still air of [`SCRIPT_DENSITY`]): the force and torque of the wind and of a
/// denser or thinner air than that, and the side force of a crosswind.
pub fn wind_force(aero: &Aero, rho: f32, v_body: Vec3, wind_body: Vec3) -> (Vec3, Vec3) {
    let (f, t) = air_force(aero, rho, v_body, wind_body);
    let (f0, t0) = air_force(aero, SCRIPT_DENSITY, v_body, Vec3::ZERO);
    (f - f0, t - t0)
}

/// What a surface is made of, as OMSI 2's `[surface]` ids say (0 asphalt, 1 concrete, 2
/// cobblestone, 3 dirt, 4 grass, 5 gravel, 6 snow, 7 deep snow).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// Grip against that of dry asphalt, dry and wet (the road's own weather grip, see
    /// `road_grip`, comes on top of it: a road in the snow is slippery whatever it is made of).
    pub grip_dry: f32,
    pub grip_wet: f32,
    /// Rolling resistance against that of asphalt.
    pub rolling: f32,
    /// The road's unevenness as ISO 8608 describes it: the power spectral density of its
    /// height (m³) at a spatial frequency of 0.1 cycles per metre, falling with the square of
    /// the frequency. The standard's classes: A (very good) 16e-6, B 64e-6, C 256e-6, D
    /// 1024e-6, E (very poor) 4096e-6.
    pub gd: f32,
}

impl Surface {
    /// RMS height (m) of the unevenness between the wavelengths a tyre follows
    /// ([`WAVE_MIN`] to [`WAVE_MAX`]), without a concrete road's joints.
    pub fn rms(&self) -> f32 {
        components(self.gd).iter().map(|c| 0.5 * c.amp * c.amp).sum::<f32>().sqrt()
    }
}

pub fn surface(id: u8) -> Surface {
    // (from the usual figures of vehicle dynamics: the peak friction of rubber on dry
    // asphalt 0.8-0.9 and wet 0.5-0.7, on dry sett pavement 0.6-0.7 and wet 0.4-0.5, on
    // packed earth 0.5-0.65 and mud 0.3-0.4, on grass 0.35-0.5 and wet 0.2-0.3, on gravel
    // 0.5-0.6, on packed snow 0.2-0.3; rolling resistance coefficients 0.008 on asphalt,
    // 0.012 on sett pavement, 0.03 on earth, 0.035 on grass, 0.02 on gravel, 0.025 on snow
    // and 0.06 in deep snow)
    match id {
        1 => Surface { grip_dry: 1.0, grip_wet: 1.0, rolling: 1.0, gd: 90e-6 },
        2 => Surface { grip_dry: 0.78, grip_wet: 0.74, rolling: 1.5, gd: 900e-6 },
        3 => Surface { grip_dry: 0.72, grip_wet: 0.6, rolling: 3.5, gd: 1800e-6 },
        4 => Surface { grip_dry: 0.5, grip_wet: 0.45, rolling: 4.0, gd: 3000e-6 },
        5 => Surface { grip_dry: 0.68, grip_wet: 0.72, rolling: 2.5, gd: 1500e-6 },
        6 => Surface { grip_dry: 0.35, grip_wet: 0.35, rolling: 3.0, gd: 400e-6 },
        7 => Surface { grip_dry: 0.3, grip_wet: 0.3, rolling: 7.0, gd: 1200e-6 },
        _ => Surface { grip_dry: 1.0, grip_wet: 1.0, rolling: 1.0, gd: 25e-6 },
    }
}

/// The factor on the road's weather grip for a tyre on surface `id` when the road is `wet`
/// (0 dry .. 1 wet). A snow-covered road (`wet` above 1) is the weather's alone: the surface
/// is under the snow. A surface that is snow itself cannot be slipperier than the weather
/// makes it, hence the caller takes the lesser of the two.
pub fn grip_scale(id: u8, wet: f32) -> f32 {
    let s = surface(id);
    let w = wet.clamp(0.0, 1.0);
    let scale = s.grip_dry + (s.grip_wet - s.grip_dry) * w;
    if wet > 1.0 {
        scale + (1.0 - scale) * (wet - 1.0).clamp(0.0, 1.0)
    } else {
        scale
    }
}

/// Rolling resistance of a tyre on `id` against the file's figure (which is for a road):
/// the surface's coefficient, rising with the speed (the tyre's hysteresis grows with the
/// frequency it is flexed at: +25 % at 80 km/h, +100 % at 160), and with the load the tyre
/// carries against the load it was rated for.
pub fn rolling_scale(id: u8, speed: f32, load_ratio: f32) -> f32 {
    let v = speed.abs() / 44.4;
    surface(id).rolling * (1.0 + v * v) * load_ratio.clamp(0.2, 2.0)
}

/// A hashed value in -1..1 (splitmix64).
fn hash(i: i64, seed: u32) -> f32 {
    let mut z = (i as u64).wrapping_add((seed as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    ((z >> 11) as f64 / (1u64 << 52) as f64 - 1.0) as f32
}

/// The shortest and longest wavelength (m) of the road a tyre follows. Shorter than a tyre's
/// contact patch (a fifth of a metre) the stones and the grain are lost between the tread's
/// blocks; longer than thirty metres is the road's course, which the ground already has.
pub const WAVE_MIN: f32 = 0.2;
pub const WAVE_MAX: f32 = 30.0;
const COMPONENTS: usize = 28;

/// One sine of a road's profile: its spatial frequency (cycles/m), its amplitude (m), its
/// phase on the left track and how far the right track's phase differs.
#[derive(Debug, Clone, Copy)]
struct Component {
    n: f32,
    amp: f32,
    phase: f32,
    apart: f32,
}

/// A road of ISO 8608 roughness `gd` as sines spread evenly over the logarithm of the
/// spatial frequency: a sine of frequency n standing for the band n ± dn/2 carries the
/// variance 2 G(n) dn, with G(n) = gd (n / 0.1)^-2. The phases are fixed by hash so that
/// the same road is the same road every time it is driven. The right track follows the
/// left in the long waves (the road's own undulation) and drifts apart from it in the
/// short ones.
fn components(gd: f32) -> [Component; COMPONENTS] {
    let (lo, hi) = (1.0 / WAVE_MAX, 1.0 / WAVE_MIN);
    let step = (hi / lo).ln() / COMPONENTS as f32;
    std::array::from_fn(|i| {
        let n = lo * (step * (i as f32 + 0.5)).exp();
        let dn = n * step;
        let g = gd * (n / 0.1).powi(-2);
        let coherence = 1.0 / (1.0 + (n / 0.12).powi(2));
        Component { n, amp: (2.0 * g * dn).sqrt(), phase: std::f32::consts::PI * hash(i as i64, 11), apart: std::f32::consts::PI * hash(i as i64, 29) * (1.0 - coherence) }
    })
}

fn spectra() -> &'static [[Component; COMPONENTS]; 8] {
    static S: std::sync::OnceLock<[[Component; COMPONENTS]; 8]> = std::sync::OnceLock::new();
    S.get_or_init(|| std::array::from_fn(|id| components(surface(id as u8).gd)))
}

/// The height (m) of the road surface of kind `id` at distance `s` along it, for the track
/// of one tyre: `side` 0 left, 1 right. A rear axle drives over what the front axle drove
/// over a wheelbase earlier, because it is asked at `s` plus its offset - which is what
/// makes a long bus pitch less than a short one on the same road.
///
/// A concrete-slab road (`id` 1) has a joint every few metres where one slab stands a few
/// millimetres above the next - the thump a bus takes on an old Plattenbau road.
pub fn roughness(id: u8, s: f64, side: u8) -> f32 {
    // (an id past OMSI's eight is asphalt)
    let set = &spectra()[if id <= 7 { id as usize } else { 0 }];
    let mut z = 0.0f32;
    for c in set {
        // (the phase from the position modulo the wavelength, exact in f64 at kilometres)
        let cycles = (s * c.n as f64).rem_euclid(1.0) as f32;
        let phase = std::f32::consts::TAU * cycles + c.phase + if side == 0 { 0.0 } else { c.apart };
        z += c.amp * phase.cos();
    }
    if id == 1 {
        z += slab_joints(s);
    }
    z
}

/// Slabs of 5 m (±0.5 m) whose level differs by up to ±3 mm from the next; the step is
/// eased over 6 cm, the length of a tyre's contact patch laid on the joint.
fn slab_joints(s: f64) -> f32 {
    const SLAB: f64 = 5.0;
    const EASE: f64 = 0.06;
    let k = (s / SLAB).floor();
    let at = s - k * SLAB;
    let level = |k: f64| 0.003 * hash(k as i64, 5);
    let t = (at / EASE).clamp(0.0, 1.0) as f32;
    let u = t * t * (3.0 - 2.0 * t);
    level(k - 1.0) + (level(k) - level(k - 1.0)) * u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_is_thinner_when_warm_and_high() {
        assert!((air_density(20.0, 0.0) - 1.204).abs() < 0.01);
        assert!(air_density(-10.0, 0.0) > air_density(30.0, 0.0));
        assert!(air_density(15.0, 1500.0) < air_density(15.0, 0.0) * 0.9);
    }

    #[test]
    fn a_bus_is_blunter_than_a_car() {
        let bus = Aero::of(Some([2.55, 12.0, 3.0]), 12_000.0);
        let car = Aero::of(Some([1.75, 4.2, 1.4]), 1_200.0);
        assert!(bus.cd_a > 4.0 && bus.cd_a < 5.5, "bus CdA {}", bus.cd_a);
        assert!(car.cd_a > 0.5 && car.cd_a < 0.8, "car CdA {}", car.cd_a);
        // a file without a size is sized by its mass
        assert!(Aero::of(None, 12_000.0).cd_a > Aero::of(None, 1_200.0).cd_a * 4.0);
        // and a size that is no number does not poison the force
        assert!(Aero::of(Some([f32::NAN, 12.0, 3.0]), 12_000.0).cd_a.is_finite());
    }

    #[test]
    fn drag_follows_the_square_of_the_speed_and_the_wind() {
        let a = Aero::of(Some([2.55, 12.0, 3.0]), 12_000.0);
        let rho = 1.2;
        let at = |v: f32, wind: f32| air_force(&a, rho, Vec3::new(0.0, v, 0.0), Vec3::new(0.0, wind, 0.0)).0.y;
        // 80 km/h in still air: ½ ρ CdA v² = 1.5-1.6 kN on a 12 m bus
        let f80 = at(22.2, 0.0);
        assert!(f80 < -1200.0 && f80 > -1900.0, "{f80}");
        assert!((at(44.4, 0.0) / f80 - 4.0).abs() < 0.01);
        // a 10 m/s tailwind takes most of it off, a headwind adds
        assert!(at(22.2, 10.0).abs() < f80.abs() * 0.4);
        assert!(at(22.2, -10.0).abs() > f80.abs() * 1.8);
    }

    #[test]
    fn a_crosswind_pushes_the_bus_downwind_and_leans_it() {
        let a = Aero::of(Some([2.55, 12.0, 3.0]), 12_000.0);
        // wind from the right (blowing towards -x of the body frame... the body is moving
        // at 20 m/s and the wind blows 15 m/s towards the left): relative air comes from the
        // right and the force pushes left
        let (f, t) = air_force(&a, 1.2, Vec3::new(0.0, 20.0, 0.0), Vec3::new(-15.0, 0.0, 0.0));
        assert!(f.x < -2000.0, "push {}", f.x);
        assert!(t.y < 0.0, "roll {}", t.y);
        assert!(t.z > 0.0, "the centre of pressure ahead turns the bus into the wind (to the left here)");
        assert!(f.y < 0.0);
        assert_eq!(air_force(&a, 1.2, Vec3::ZERO, Vec3::ZERO).0, Vec3::ZERO);
    }

    #[test]
    fn only_the_weather_is_added_to_what_the_scripts_already_do() {
        let a = Aero::of(Some([2.55, 12.0, 3.0]), 12_000.0);
        let v = Vec3::new(0.0, 22.2, 0.0);
        // still air of the scripts' density: nothing on top of their drag
        let (f, t) = wind_force(&a, SCRIPT_DENSITY, v, Vec3::ZERO);
        assert!(f.length() < 1e-3 && t.length() < 1e-3, "{f:?}");
        // a headwind and a thicker air add, a tailwind takes off
        assert!(wind_force(&a, SCRIPT_DENSITY, v, Vec3::new(0.0, -10.0, 0.0)).0.y < -500.0);
        assert!(wind_force(&a, SCRIPT_DENSITY, v, Vec3::new(0.0, 10.0, 0.0)).0.y > 500.0);
        assert!(wind_force(&a, 1.32, v, Vec3::ZERO).0.y < -50.0);
        // a crosswind pushes
        assert!(wind_force(&a, SCRIPT_DENSITY, v, Vec3::new(-15.0, 0.0, 0.0)).0.x < -2000.0);
    }

    #[test]
    fn surfaces_hold_as_the_figures_say() {
        let dry = |id| grip_scale(id, 0.0);
        let wet = |id| grip_scale(id, 1.0);
        assert_eq!(dry(0), 1.0);
        assert_eq!(dry(77), 1.0, "an id outside OMSI's eight is asphalt");
        assert!(dry(2) < dry(0) && dry(4) < dry(2) && dry(6) < dry(4));
        assert!(wet(3) < dry(3), "mud is slipperier than dry earth");
        assert!(wet(5) >= dry(5) - 1e-6, "gravel drains");
        // under snow the surface no longer matters
        assert!(grip_scale(4, 2.0) > 0.99);
    }

    #[test]
    fn rolling_rises_with_speed_load_and_softness() {
        assert!(rolling_scale(4, 10.0, 1.0) > rolling_scale(0, 10.0, 1.0) * 3.0);
        assert!((rolling_scale(0, 22.2, 1.0) / rolling_scale(0, 0.0, 1.0) - 1.25).abs() < 0.03);
        assert!(rolling_scale(0, 10.0, 0.0) < rolling_scale(0, 10.0, 1.0) * 0.25);
        assert_eq!(rolling_scale(0, -10.0, 1.0), rolling_scale(0, 10.0, 1.0));
    }

    /// RMS of the road a tyre follows over a long run (5 km, to take in the longest waves).
    fn rms_of(id: u8) -> f32 {
        let n = 400_000;
        let sum: f64 = (0..n).map(|i| roughness(id, i as f64 * 0.0125 + 3.1, 0).powi(2) as f64).sum();
        (sum / n as f64).sqrt() as f32
    }

    #[test]
    fn roughness_has_the_rms_of_its_surface() {
        for id in [0u8, 2, 3, 4, 5, 6, 7] {
            let (got, want) = (rms_of(id), surface(id).rms());
            assert!((got / want - 1.0).abs() < 0.1, "surface {id}: {got} against {want}");
        }
        // the figures of the standard: good asphalt a few millimetres over thirty metres of
        // wavelength, cobbles a centimetre or two, a meadow three
        assert!(surface(0).rms() > 0.002 && surface(0).rms() < 0.005, "{}", surface(0).rms());
        assert!(surface(2).rms() > 0.010 && surface(2).rms() < 0.025, "{}", surface(2).rms());
        assert!(surface(4).rms() > 0.025 && surface(4).rms() < 0.045, "{}", surface(4).rms());
        // concrete adds its joints to the surface's own unevenness
        assert!(rms_of(1) > surface(1).rms() && rms_of(1) < surface(1).rms() * 1.6, "{}", rms_of(1));
    }

    #[test]
    fn rougher_surfaces_shake_more_and_the_profile_is_smooth() {
        assert!(rms_of(2) > rms_of(0) * 3.0);
        assert!(rms_of(4) > rms_of(2));
        // continuous, except across a slab joint (eased over 6 cm)
        let mut worst = 0.0f32;
        for i in 0..20_000 {
            let s = i as f64 * 0.005;
            worst = worst.max((roughness(2, s + 0.005, 0) - roughness(2, s, 0)).abs());
        }
        assert!(worst < 0.0015, "cobbles jump {worst} m in 5 mm");
        // the same place, the same road
        assert_eq!(roughness(2, 123.456, 1), roughness(2, 123.456, 1));
    }

    #[test]
    fn the_two_tracks_share_the_long_waves_and_differ_in_the_short() {
        let n = 40_000;
        let (mut cov, mut a2, mut b2) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..n {
            let s = i as f64 * 0.05;
            let (l, r) = (roughness(0, s, 0) as f64, roughness(0, s, 1) as f64);
            cov += l * r;
            a2 += l * l;
            b2 += r * r;
        }
        let corr = cov / (a2 * b2).sqrt();
        assert!(corr > 0.5 && corr < 0.95, "correlation {corr}");
    }

    #[test]
    fn a_slab_road_has_joints_every_five_metres() {
        // the level on one slab and on the next differs by a few millimetres, and the step
        // between them is eased: no jump within a millimetre of a joint's start
        let before = slab_joints(4.99);
        let after = slab_joints(5.2);
        assert!((after - before).abs() <= 0.006);
        assert!((slab_joints(5.0) - slab_joints(4.999)).abs() < 1e-4);
        let steps = (1..60).filter(|k| (slab_joints(*k as f64 * 5.0 + 0.3) - slab_joints(*k as f64 * 5.0 - 0.3)).abs() > 0.001).count();
        assert!(steps > 30, "{steps} of 59 joints step by more than a millimetre");
    }
}
