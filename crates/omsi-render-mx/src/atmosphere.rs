//! The enhanced renderer's light: a physically based atmosphere - Rayleigh and aerosol
//! scattering over a spherical Earth with ozone absorption, single scattering marched
//! along every view ray and the higher orders from a multiple-scattering table (Hillaire,
//! "A Scalable and Production Ready Sky and Atmosphere Rendering Technique", 2020) -
//! evaluated on the CPU whenever the sun, the moon or the weather has moved on.
//!
//! It yields the sun's colour and strength at the ground, the sky's radiance as a small
//! table the sky dome and the reflection probe read, the light the sky and the ground
//! throw on a surface of any orientation (spherical harmonics), and the exposure that
//! suits the whole of it - by night from the moon, whose light goes through the same
//! atmosphere, and the glow a city's lamps throw on its haze and its clouds.
//!
//! The air is not the same every day: how much aerosol there is and of which kind (fine
//! dry particles that scatter blue more than red, or a humid summer haze that scatters
//! all colours alike and whitens the sky) is part of the input, so a crisp winter
//! morning, a milky July afternoon and a clear evening after rain each look their own.
//!
//! The picture is balanced as a camera balances it: for daylight, and then a part of the
//! way towards the light of the moment (the golden hour stays golden, the blue hour blue,
//! but neither as strongly as an unbalanced sensor records them).
//!
//! Units: 1 = 10 000 lux of irradiance (the noon sun is about 10, a street under its lamps
//! about 0.002) and 1 per steradian of radiance.

use glam::Vec3;

/// The sky table: azimuth from the sun (0..π, the sky is symmetric about the sun's
/// vertical) by elevation (-90°..90°, denser at the horizon).
pub const SKY_LUT_W: u32 = 32;
pub const SKY_LUT_H: u32 = 48;

const EARTH_R: f32 = 6_360_000.0;
const ATMO_R: f32 = 6_420_000.0;
/// Scattering coefficients at sea level (1/m) for red, green and blue.
const RAYLEIGH: Vec3 = Vec3::new(5.802e-6, 13.558e-6, 33.1e-6);
const RAYLEIGH_H: f32 = 8000.0;
/// Aerosol scattering at 550 nm of a clear day in a central European city (an optical
/// depth of about 0.05); extinction is a tenth more (absorption).
const MIE: f32 = 4.0e-5;
const MIE_EXT: f32 = 1.11;
/// The aerosol's scale height on an average day (m): the depth of the hazy layer the day's
/// convection mixes up from the ground (`SkyInput::aerosol_height`).
const MIE_H: f32 = 1200.0;
/// The stratospheric aerosol layer (fine sulphate droplets, a few kilometres thick round
/// 20 km): thin, but still lit by the sun long after it has set for the ground - what it
/// scatters is the purple light of a clear twilight, vivid in the years after a volcano.
const STRAT_H: f32 = 20_000.0;
const STRAT_W: f32 = 6000.0;
/// Its colour: small droplets scatter blue more than red.
const STRAT_ANGSTROM: f32 = 1.6;
const OZONE: Vec3 = Vec3::new(0.650e-6, 1.881e-6, 0.085e-6);
/// The wavelengths the three channels stand for (nm), for the aerosol's colour.
const WAVELENGTHS: Vec3 = Vec3::new(615.0, 550.0, 465.0);
/// The eye height the sky is seen from (m): the horizon dips a little below zero.
const OBSERVER_H: f32 = 250.0;
/// The sun's irradiance above the atmosphere.
const SUN_E0: f32 = 12.5;
/// The full moon's irradiance above the atmosphere (about 0.3 lux).
const MOON_E0: f32 = 3.0e-5;
/// The moon's colour against the sun's: moonlight is sunlight reflected off grey-brown
/// rock, a little redder.
const MOON_TINT: Vec3 = Vec3::new(1.04, 1.0, 0.93);
/// A city's own glow on its haze (radiance at the zenith of a clear night, about 0.07 lux
/// on the ground in all) - warm from the lamps, brighter towards the horizon, several
/// times brighter under a cloud deck that throws it back.
const CITY_GLOW: Vec3 = Vec3::new(3.4e-6, 2.85e-6, 2.15e-6);
/// The starlit and airglow sky of a place without lamps, the floor of every night.
const NATURAL_NIGHT: Vec3 = Vec3::new(0.9e-7, 1.0e-7, 1.3e-7);
/// The light a lit city keeps up around the viewer at night (street lamps, windows) as far
/// as the exposure is concerned.
pub const ARTIFICIAL: f32 = 0.0015;
/// How much of the sky at the horizon houses and trees hide (for the ambient light), up to
/// which elevation (degrees), and how much light they throw back.
const SURROUND_MAX: f32 = 0.7;
const SURROUND_ELEVATION: f32 = 18.0;
const SURROUND_ALBEDO: f32 = 0.25;
/// How much the walls a low sun shines on count towards the exposure.
const LOW_SUN_WALLS: f32 = 0.3;
/// How much of the sky light's colour the eye discounts in the shade (as a camera's white
/// balance does: a shaded pavement looks a cool grey in a photograph, not blue).
const SHADE_ADAPTATION: f32 = 0.55;
/// How far the camera's white balance follows the light of the moment from daylight.
const AUTO_WHITE: f32 = 0.32;
/// Irradiance of a sunlit and sky-lit horizontal surface at noon, the exposure reference.
const DAY_REFERENCE: f32 = 11.0;

/// What the sky is made of at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyInput {
    /// Towards the sun, world space (x east, y north, z up).
    pub sun_dir: Vec3,
    /// How much of the direct sun the clouds let through (0..1).
    pub sun_visibility: f32,
    /// A closed cloud cover turns the sky into an even grey dome (0..1).
    pub overcast: f32,
    /// Aerosol amount: 1 a clear day, more for haze, mist and rain.
    pub haze: f32,
    /// The aerosol's Ångström exponent: about 1.4 for fine, dry particles (a deep blue sky,
    /// crisp distances), towards 0.5 for a humid haze of large droplets (a milky, whitish
    /// sky that scatters every colour alike).
    pub angstrom: f32,
    /// The depth of the hazy boundary layer (m): a shallow winter inversion holds the haze
    /// low and thick over the ground, a summer afternoon mixes it two kilometres up - the
    /// low sun shines through more or less of it, which is most of what makes one sunset
    /// pale yellow and the next deep red.
    pub aerosol_height: f32,
    /// The stratospheric aerosol's optical depth at 550 nm (0.002 a clean year .. 0.05
    /// after a large eruption): the purple of the twilight.
    pub strat_aod: f32,
    /// The optical depth of a veil of high ice cloud (cirrostratus, about 8 km up): it dims
    /// the sun along its slant path and scatters what it takes strongly forward, into a
    /// bright white aureole round the sun - a milky sun, soft pale shadows.
    pub veil: f32,
    /// How much of the sky the cumulus covers (0..1): white heaps lit by the sun, which
    /// throw more light down than the blue between them, and greyer (the dome draws them
    /// itself; this is their part in the light on the street).
    pub cumulus: f32,
    /// Rain: the sky light drops under a thick, wet cover (0..1).
    pub rain: f32,
    /// Albedo of the ground the light comes back from (snow is bright).
    pub ground_albedo: f32,
    /// envir.cfg's light colours relative to the stock ones: sun (A), sky (B), ambient (C).
    pub tint: [Vec3; 3],
    /// Towards the moon and how much of its disc is lit (0 new .. 1 full).
    pub moon_dir: Vec3,
    pub moon_illum: f32,
    /// How much a city's lamps light its own sky at night (0 open country .. 1 a city).
    pub city_glow: f32,
    /// The optical depth of the grey deck's liquid water (0: not known - a stratus' 15, in
    /// rain up to a raining nimbostratus' 60).
    pub deck_tau: f32,
    /// The cumulus layer's base and top over the ground (m): the sun it is lit by comes
    /// through the air up to there.
    pub cloud_base: f32,
    pub cloud_top: f32,
}

impl Default for SkyInput {
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(0.3, 0.2, 0.9).normalize(),
            sun_visibility: 1.0,
            overcast: 0.0,
            haze: 1.0,
            angstrom: 1.3,
            aerosol_height: MIE_H,
            strat_aod: 0.004,
            veil: 0.0,
            cumulus: 0.0,
            deck_tau: 0.0,
            cloud_base: 1400.0,
            cloud_top: 2800.0,
            rain: 0.0,
            ground_albedo: 0.2,
            tint: [Vec3::ONE; 3],
            moon_dir: Vec3::new(0.0, -0.5, -0.866),
            moon_illum: 0.0,
            city_glow: 1.0,
        }
    }
}

/// The light of the sky for one `SkyInput`.
#[derive(Debug, Clone)]
pub struct SkyState {
    pub input: SkyInput,
    /// Irradiance of the direct sun on a surface facing it, after the clouds.
    pub sun: Vec3,
    /// The sun disc's irradiance before the clouds (its colour and strength in the sky).
    pub sun_disc: Vec3,
    /// The moon disc's irradiance at the ground before the clouds (balanced as the rest).
    pub moon_disc: Vec3,
    /// The sun's irradiance at the heights of `CLOUD_SUN_HEIGHTS` (the cumulus layer's base,
    /// middle and top, and the high thin layer): what lights the clouds. Up there the sun
    /// shines through less air and sets later - the clouds glow orange and pink while the
    /// street is already in the blue hour, and the high layer last of all.
    pub sun_at: [Vec3; 4],
    /// Sky radiance per table cell divided by `lut_scale`, RGBA (alpha unused).
    pub lut: Vec<[f32; 4]>,
    pub lut_scale: f32,
    /// Irradiance from the sky and the ground as order-2 spherical harmonics, already
    /// convolved with the cosine lobe: E(n) = Σ sh[i] · Y_i(n).
    pub sh: [Vec3; 9],
    /// Irradiance of a horizontal surface from the sky alone (the moon's light included).
    pub sky_horizontal: Vec3,
    /// Radiance of the (distant) ground.
    pub ground: Vec3,
    /// Pre-exposure: scene radiance times this is what the picture holds (mid grey in the
    /// day's light comes out at about 0.18).
    pub exposure: f32,
    /// The light the exposure is set for: the direct sun's part and the rest (luminance),
    /// so that it can follow a cloud's shadow passing over (`exposure_for`).
    pub e_sun: f32,
    pub e_rest: f32,
    /// The moon's irradiance on a surface facing it, after the clouds (balanced as the
    /// rest): a directional light like the sun's, with a shadow of its own by night.
    pub moon_light: Vec3,
    /// The part of `e_rest` that stands for the lamps round the viewer when nothing better
    /// is known (the renderer measures them: `view_lamp_light`).
    pub e_artificial: f32,
}

/// Optical depth per unit coefficient (Rayleigh, Mie, ozone) along a ray from altitude h
/// in the direction with cosine of the zenith angle mu, to the top of the atmosphere;
/// huge where the ray meets the ground (no sunlight arrives along it).
struct DepthTable {
    rows: usize,
    cols: usize,
    depth: Vec<[f32; 4]>,
}

const H_TOP: f32 = ATMO_R - EARTH_R;

impl DepthTable {
    fn build(aerosol_h: f32) -> DepthTable {
        let (rows, cols) = (32usize, 160usize);
        let mut depth = Vec::with_capacity(rows * cols);
        for i in 0..rows {
            let h = Self::row_height(i, rows);
            for j in 0..cols {
                let mu = -1.0 + 2.0 * j as f32 / (cols - 1) as f32;
                depth.push(Self::integrate(h, mu, aerosol_h));
            }
        }
        DepthTable { rows, cols, depth }
    }

    fn row_height(i: usize, rows: usize) -> f32 {
        let x = i as f32 / (rows - 1) as f32;
        x * x * H_TOP
    }

    fn integrate(h: f32, mu: f32, aerosol_h: f32) -> [f32; 4] {
        let r0 = EARTH_R + h;
        if ray_hits_ground(r0, mu).is_some() {
            return [1e9; 4];
        }
        let t_max = ray_exit(r0, mu, ATMO_R);
        let n = 48;
        let mut acc = [0.0f32; 4];
        for k in 0..n {
            // denser near the start, where the air is thick
            let a = k as f32 / n as f32;
            let b = (k + 1) as f32 / n as f32;
            let (t0, t1) = (a * a * t_max, b * b * t_max);
            let t = 0.5 * (t0 + t1);
            let hh = altitude(r0, mu, t);
            let d = densities(hh, aerosol_h);
            let dt = t1 - t0;
            for k in 0..4 {
                acc[k] += d[k] * dt;
            }
        }
        acc
    }

    fn lookup(&self, h: f32, mu: f32) -> [f32; 4] {
        let fr = (h.clamp(0.0, H_TOP) / H_TOP).sqrt() * (self.rows - 1) as f32;
        let fc = ((mu.clamp(-1.0, 1.0) + 1.0) * 0.5) * (self.cols - 1) as f32;
        let (r0, c0) = (fr.floor() as usize, fc.floor() as usize);
        let (r1, c1) = ((r0 + 1).min(self.rows - 1), (c0 + 1).min(self.cols - 1));
        let (tr, tc) = (fr - r0 as f32, fc - c0 as f32);
        let at = |r: usize, c: usize| self.depth[r * self.cols + c];
        let mut out = [0.0f32; 4];
        for (k, o) in out.iter_mut().enumerate() {
            let a = at(r0, c0)[k] * (1.0 - tc) + at(r0, c1)[k] * tc;
            let b = at(r1, c0)[k] * (1.0 - tc) + at(r1, c1)[k] * tc;
            *o = a * (1.0 - tr) + b * tr;
        }
        out
    }
}

/// The depth table for an aerosol scale height (to 50 m), kept for the few heights in use.
fn depth_table(aerosol_h: f32) -> std::sync::Arc<DepthTable> {
    static TABLES: std::sync::Mutex<Vec<(u32, std::sync::Arc<DepthTable>)>> = std::sync::Mutex::new(Vec::new());
    let key = (aerosol_h.clamp(200.0, 5000.0) / 50.0).round() as u32;
    let mut tables = TABLES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, t)) = tables.iter().find(|(k, _)| *k == key) {
        return t.clone();
    }
    let t = std::sync::Arc::new(DepthTable::build(key as f32 * 50.0));
    if tables.len() >= 6 {
        tables.remove(0);
    }
    tables.push((key, t.clone()));
    t
}

/// Air, boundary-layer aerosol, ozone and stratospheric aerosol density (relative to
/// their reference) at altitude h.
fn densities(h: f32, aerosol_h: f32) -> [f32; 4] {
    let ozone = (1.0 - (h - 25_000.0).abs() / 15_000.0).max(0.0);
    let x = (h - STRAT_H) / STRAT_W;
    [(-h / RAYLEIGH_H).exp(), (-h / aerosol_h.max(100.0)).exp(), ozone, (-x * x).exp()]
}

/// Altitude after `t` metres from radius r0 along a ray with zenith cosine mu.
fn altitude(r0: f32, mu: f32, t: f32) -> f32 {
    (r0 * r0 + t * t + 2.0 * r0 * t * mu).max(0.0).sqrt() - EARTH_R
}

/// Distance to where the ray leaves the sphere of radius `radius` (the origin is inside).
fn ray_exit(r0: f32, mu: f32, radius: f32) -> f32 {
    let disc = r0 * r0 * (mu * mu - 1.0) + radius * radius;
    -r0 * mu + disc.max(0.0).sqrt()
}

/// Distance to the ground along the ray, if it meets it.
fn ray_hits_ground(r0: f32, mu: f32) -> Option<f32> {
    if mu >= 0.0 {
        return None;
    }
    let disc = r0 * r0 * (mu * mu - 1.0) + EARTH_R * EARTH_R;
    if disc < 0.0 {
        return None;
    }
    Some((-r0 * mu - disc.sqrt()).max(0.0))
}

/// The aerosol of one `SkyInput`: scattering and extinction at sea level per channel, and
/// how strongly it scatters forward.
#[derive(Clone, Copy)]
struct Aerosol {
    scat: Vec3,
    ext: Vec3,
    g: f32,
    /// the stratospheric layer's scattering and extinction at its peak
    strat_scat: Vec3,
    strat_ext: Vec3,
    /// the boundary layer's scale height (m)
    height: f32,
}

impl Aerosol {
    fn of(input: &SkyInput) -> Aerosol {
        let haze = input.haze.max(0.0);
        let a = input.angstrom.clamp(0.0, 2.0);
        let colour = Vec3::new((WAVELENGTHS.x / 550.0).powf(-a), (WAVELENGTHS.y / 550.0).powf(-a), (WAVELENGTHS.z / 550.0).powf(-a));
        // the same optical depth whatever the layer's depth: a shallow layer is denser
        let height = input.aerosol_height.clamp(200.0, 5000.0);
        let scat = colour * (MIE * haze * MIE_H / height);
        // large droplets scatter more strongly forward: a bright, wide glow round the sun
        let g = 0.72 + 0.08 * (1.3 - a).clamp(0.0, 1.0);
        let sa = STRAT_ANGSTROM;
        let strat_colour = Vec3::new((WAVELENGTHS.x / 550.0).powf(-sa), (WAVELENGTHS.y / 550.0).powf(-sa), (WAVELENGTHS.z / 550.0).powf(-sa));
        // (its column: the Gaussian layer's integral, √π times its half-width)
        let strat_scat = strat_colour * (input.strat_aod.max(0.0) / (STRAT_W * std::f32::consts::PI.sqrt()));
        Aerosol { scat, ext: scat * MIE_EXT, g, strat_scat, strat_ext: strat_scat * 1.02, height }
    }

    /// Rayleigh and aerosol scattering coefficients at a point of these densities.
    fn scattering(&self, d: [f32; 4]) -> (Vec3, Vec3) {
        (RAYLEIGH * d[0], self.scat * d[1] + self.strat_scat * d[3])
    }

    fn extinction(&self, d: [f32; 4]) -> Vec3 {
        RAYLEIGH * d[0] + self.ext * d[1] + OZONE * d[2] + self.strat_ext * d[3]
    }
}

fn transmittance(depth: [f32; 4], air: &Aerosol) -> Vec3 {
    let tau = air.extinction(depth);
    Vec3::new((-tau.x).exp(), (-tau.y).exp(), (-tau.z).exp())
}

fn rayleigh_phase(c: f32) -> f32 {
    3.0 / (16.0 * std::f32::consts::PI) * (1.0 + c * c)
}

/// Cornette-Shanks aerosol phase function.
fn mie_phase(c: f32, g: f32) -> f32 {
    let g2 = g * g;
    3.0 / (8.0 * std::f32::consts::PI) * ((1.0 - g2) * (1.0 + c * c)) / ((2.0 + g2) * (1.0 + g2 - 2.0 * g * c).max(1e-4).powf(1.5))
}

/// The multiple-scattering table: the light scattered twice and more at an altitude for
/// a sun at a zenith cosine, per unit of sun irradiance and of scattering coefficient
/// (Hillaire 2020: the second order from an isotropic phase, the higher ones as a geometric
/// series of the same transfer). It is what keeps the twilight sky lit long after the
/// sun has gone and gives the blue hour its depth.
struct MsTable {
    psi: Vec<Vec3>,
}

const MS_H: usize = 16;
const MS_MU: usize = 24;
const MS_DIRS: usize = 64;
const MS_STEPS: usize = 20;

impl MsTable {
    fn build(air: &Aerosol, ground_albedo: f32) -> MsTable {
        let table = depth_table(air.height);
        let mut psi = Vec::with_capacity(MS_H * MS_MU);
        // the sphere of directions (a Fibonacci lattice)
        let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
        let dirs: Vec<Vec3> = (0..MS_DIRS)
            .map(|k| {
                let z = 1.0 - 2.0 * (k as f32 + 0.5) / MS_DIRS as f32;
                let r = (1.0 - z * z).max(0.0).sqrt();
                let phi = golden * k as f32;
                Vec3::new(r * phi.cos(), r * phi.sin(), z)
            })
            .collect();
        let dw = 4.0 * std::f32::consts::PI / MS_DIRS as f32;
        let iso = 1.0 / (4.0 * std::f32::consts::PI);
        for i in 0..MS_H {
            let x = (i as f32 + 0.5) / MS_H as f32;
            let h = x * x * H_TOP;
            let r0 = EARTH_R + h;
            for j in 0..MS_MU {
                let mu_s = -1.0 + 2.0 * (j as f32 + 0.5) / MS_MU as f32;
                let s = Vec3::new((1.0 - mu_s * mu_s).max(0.0).sqrt(), 0.0, mu_s);
                let mut l2 = Vec3::ZERO;
                let mut f_ms = Vec3::ZERO;
                for d in &dirs {
                    let mu = d.z;
                    let ground = ray_hits_ground(r0, mu);
                    let t_max = ground.unwrap_or_else(|| ray_exit(r0, mu, ATMO_R));
                    let c = d.dot(s);
                    let mut tr = Vec3::ONE;
                    let mut prev = 0.0f32;
                    for k in 0..MS_STEPS {
                        let b = (k + 1) as f32 / MS_STEPS as f32;
                        let t1 = b * b * t_max;
                        let dt = t1 - prev;
                        let t = 0.5 * (prev + t1);
                        prev = t1;
                        let hh = altitude(r0, mu, t);
                        let dens = densities(hh, air.height);
                        let (sr, sm) = air.scattering(dens);
                        let scat = sr + sm;
                        let ext = air.extinction(dens);
                        let step_t = Vec3::new((-ext.x * dt).exp(), (-ext.y * dt).exp(), (-ext.z * dt).exp());
                        // the energy-conserving integral over the step (Frostbite)
                        let integ = (Vec3::ONE - step_t) / ext.max(Vec3::splat(1e-12));
                        let r = (hh + EARTH_R).max(1.0);
                        let mu_sp = ((r0 * mu_s + t * c) / r).clamp(-1.0, 1.0);
                        let t_sun = transmittance(table.lookup(hh, mu_sp), air);
                        l2 += tr * scat * t_sun * integ * iso;
                        f_ms += tr * scat * integ * iso;
                        tr *= step_t;
                    }
                    if let Some(tg) = ground {
                        // the lit ground under the ray throws some back
                        let p = Vec3::new(0.0, 0.0, r0) + *d * tg;
                        let up = p.normalize();
                        let mu_g = up.dot(s);
                        let t_sun = transmittance(table.lookup(0.0, mu_g), air);
                        l2 += tr * t_sun * (mu_g.max(0.0) * ground_albedo / std::f32::consts::PI);
                    }
                }
                // (the second phase function: the light gathered from every direction is
                // scattered once more towards the viewer, isotropically - f_ms carries its
                // phase inside the march already)
                l2 *= dw * iso;
                f_ms *= dw;
                let series = Vec3::ONE / (Vec3::ONE - f_ms).max(Vec3::splat(0.05));
                psi.push(l2 * series);
            }
        }
        MsTable { psi }
    }

    fn lookup(&self, h: f32, mu_s: f32) -> Vec3 {
        let fr = ((h.clamp(0.0, H_TOP) / H_TOP).sqrt() * MS_H as f32 - 0.5).clamp(0.0, (MS_H - 1) as f32);
        let fc = (((mu_s.clamp(-1.0, 1.0) + 1.0) * 0.5) * MS_MU as f32 - 0.5).clamp(0.0, (MS_MU - 1) as f32);
        let (r0, c0) = (fr.floor() as usize, fc.floor() as usize);
        let (r1, c1) = ((r0 + 1).min(MS_H - 1), (c0 + 1).min(MS_MU - 1));
        let (tr, tc) = (fr - r0 as f32, fc - c0 as f32);
        let at = |r: usize, c: usize| self.psi[r * MS_MU + c];
        let a = at(r0, c0).lerp(at(r0, c1), tc);
        let b = at(r1, c0).lerp(at(r1, c1), tc);
        a.lerp(b, tr)
    }
}

/// Elevation (radians) of the centre of table row `v` in 0..1.
pub fn lut_elevation(v: f32) -> f32 {
    let s = (v - 0.5) * 2.0;
    s.signum() * s * s * std::f32::consts::FRAC_PI_2
}

/// Table row coordinate (0..1) of an elevation in radians.
pub fn lut_row(el: f32) -> f32 {
    0.5 + 0.5 * el.signum() * (el.abs() / std::f32::consts::FRAC_PI_2).min(1.0).sqrt()
}

pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Real spherical harmonics up to order 2 in direction d.
fn sh_basis(d: Vec3) -> [f32; 9] {
    [
        0.282_095,
        0.488_603 * d.y,
        0.488_603 * d.z,
        0.488_603 * d.x,
        1.092_548 * d.x * d.y,
        1.092_548 * d.y * d.z,
        0.315_392 * (3.0 * d.z * d.z - 1.0),
        1.092_548 * d.x * d.z,
        0.546_274 * (d.x * d.x - d.y * d.y),
    ]
}

/// Evaluate irradiance SH (as produced in `SkyState::sh`) for a surface normal.
pub fn sh_irradiance(sh: &[Vec3; 9], n: Vec3) -> Vec3 {
    let y = sh_basis(n);
    sh.iter().zip(y).map(|(c, y)| *c * y).fold(Vec3::ZERO, |a, b| a + b)
}

const LUM: Vec3 = Vec3::new(0.2126, 0.7152, 0.0722);

/// The camera's white: the colour of the noon daylight on a white sheet (sun at 60° and
/// a clear sky), which the whole model is divided by - a camera set to daylight.
fn daylight_white() -> Vec3 {
    static WHITE: std::sync::OnceLock<Vec3> = std::sync::OnceLock::new();
    *WHITE.get_or_init(|| {
        let input = SkyInput { sun_dir: Vec3::new(0.5, 0.0, 0.866), ..Default::default() };
        let raw = RawSky::compute(input.sun_dir, &input, SUN_E0);
        let e = raw.sun * input.sun_dir.z + raw.sky_horizontal;
        e / e.dot(LUM)
    })
}

/// The sky lit by one body (the sun or the moon) before white balance, clouds and tints.
struct RawSky {
    /// The body's irradiance at the ground on a surface facing it.
    sun: Vec3,
    lut: Vec<Vec3>,
    sky_horizontal: Vec3,
}

impl RawSky {
    fn compute(body: Vec3, input: &SkyInput, e0: f32) -> RawSky {
        let air = Aerosol::of(input);
        let table = depth_table(air.height);
        let ms = MsTable::build(&air, input.ground_albedo);
        let s = body.normalize_or_zero();
        let r0 = EARTH_R + OBSERVER_H;
        let sun = e0 * transmittance(table.lookup(OBSERVER_H, s.z), &air);
        let (w, h) = (SKY_LUT_W as usize, SKY_LUT_H as usize);
        let mut lut = Vec::with_capacity(w * h);
        let sun_el_cos = Vec3::new(s.x, s.y, 0.0).length();
        for row in 0..h {
            let el = lut_elevation((row as f32 + 0.5) / h as f32);
            let mu = el.sin();
            let t_max = ray_hits_ground(r0, mu).unwrap_or_else(|| ray_exit(r0, mu, ATMO_R));
            for col in 0..w {
                let az = (col as f32 + 0.5) / w as f32 * std::f32::consts::PI;
                // the body lies at azimuth 0 in this frame
                let d = Vec3::new(el.cos() * az.cos(), el.cos() * az.sin(), mu);
                let sl = Vec3::new(sun_el_cos, 0.0, s.z);
                let c = d.dot(sl);
                let (pr, pm) = (rayleigh_phase(c), mie_phase(c, air.g));
                let n = 24;
                let mut acc = Vec3::ZERO;
                let mut tr = Vec3::ONE;
                let mut prev_t = 0.0f32;
                for k in 0..n {
                    let b = (k + 1) as f32 / n as f32;
                    let t1 = b * b * t_max;
                    let t = 0.5 * (prev_t + t1);
                    let dt = t1 - prev_t;
                    prev_t = t1;
                    let hh = altitude(r0, mu, t);
                    let dens = densities(hh, air.height);
                    let r = (hh + EARTH_R).max(1.0);
                    let mu_s = ((r0 * s.z + t * c) / r).clamp(-1.0, 1.0);
                    let t_sun = transmittance(table.lookup(hh, mu_s), &air);
                    let (scat_r, scat_m) = air.scattering(dens);
                    let ext = air.extinction(dens);
                    let step_t = Vec3::new((-ext.x * dt).exp(), (-ext.y * dt).exp(), (-ext.z * dt).exp());
                    let integ = (Vec3::ONE - step_t) / ext.max(Vec3::splat(1e-12));
                    let single = (scat_r * pr + scat_m * pm) * t_sun;
                    let multi = (scat_r + scat_m) * ms.lookup(hh, mu_s);
                    acc += tr * (single + multi) * integ;
                    tr *= step_t;
                }
                lut.push(acc * e0);
            }
        }
        let sky_horizontal = horizontal_irradiance(&lut);
        RawSky { sun, lut, sky_horizontal }
    }
}

/// Heights over the ground (m) of `SkyState::sun_at` (sky_enhanced.wgsl's cloud layer).
pub const CLOUD_SUN_HEIGHTS: [f32; 4] = [1400.0, 2100.0, 2800.0, 7000.0];

/// Irradiance on a horizontal surface from the upper half of a sky table.
fn horizontal_irradiance(lut: &[Vec3]) -> Vec3 {
    let (w, h) = (SKY_LUT_W as usize, SKY_LUT_H as usize);
    let az_step = std::f32::consts::PI / w as f32;
    let mut e = Vec3::ZERO;
    for row in (h / 2)..h {
        let (e0, e1) = (lut_elevation(row as f32 / h as f32), lut_elevation((row + 1) as f32 / h as f32));
        let el = lut_elevation((row as f32 + 0.5) / h as f32);
        for col in 0..w {
            e += lut[row * w + col] * el.sin().max(0.0) * el.cos() * az_step * (e1 - e0) * 2.0;
        }
    }
    e
}

/// Direction of table cell (row, col) for a body at `body` (the table's azimuth 0).
fn cell_dir(body: Vec3, row: usize, col: usize) -> Vec3 {
    let (w, h) = (SKY_LUT_W as usize, SKY_LUT_H as usize);
    let el = lut_elevation((row as f32 + 0.5) / h as f32);
    let az = (col as f32 + 0.5) / w as f32 * std::f32::consts::PI;
    let b_az = body.y.atan2(body.x);
    let a = b_az + az;
    Vec3::new(el.cos() * a.cos(), el.cos() * a.sin(), el.sin())
}

/// The moon's table resampled into the sun's frame (the sky table runs from the sun's
/// azimuth; the moon stands elsewhere): each cell takes the moon's sky in that direction,
/// averaged over the two mirror halves the sun's table folds together.
fn moon_in_sun_frame(moon: &RawSky, moon_dir: Vec3, sun_dir: Vec3) -> Vec<Vec3> {
    let (w, h) = (SKY_LUT_W as usize, SKY_LUT_H as usize);
    let m_az = moon_dir.y.atan2(moon_dir.x);
    let s_az = sun_dir.y.atan2(sun_dir.x);
    let mut out = Vec::with_capacity(w * h);
    for row in 0..h {
        for col in 0..w {
            let az = (col as f32 + 0.5) / w as f32 * std::f32::consts::PI;
            let mut acc = Vec3::ZERO;
            for side in [-1.0f32, 1.0] {
                let a = s_az + side * az;
                let rel = (a - m_az).rem_euclid(2.0 * std::f32::consts::PI);
                let rel = if rel > std::f32::consts::PI { 2.0 * std::f32::consts::PI - rel } else { rel };
                let fc = (rel / std::f32::consts::PI * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
                let (c0, c1) = (fc.floor() as usize, (fc.floor() as usize + 1).min(w - 1));
                let t = fc - c0 as f32;
                acc += moon.lut[row * w + c0].lerp(moon.lut[row * w + c1], t);
            }
            out.push(acc * 0.5);
        }
    }
    out
}

impl SkyState {
    /// The sky, the sun and the ambient light for this input.
    pub fn compute(input: &SkyInput) -> SkyState {
        let s = input.sun_dir.normalize_or_zero();
        let raw = RawSky::compute(s, input, SUN_E0);
        let daylight = daylight_white();
        // the camera's white: daylight, and a part of the way towards the light of the
        // moment while the sun is up (the blue hour stays blue - balanced away, its sky
        // went a muddy purple - and by night the street lamps decide what the eye takes
        // for white, and the moonlit sky stays blue)
        let now = (raw.sun * s.z.max(0.0) + raw.sky_horizontal) / daylight;
        let now = now / now.dot(LUM).max(1e-12);
        let follow = AUTO_WHITE * smoothstep(0.0, 0.08, s.z);
        let white = daylight * Vec3::ONE.lerp(now.clamp(Vec3::splat(0.3), Vec3::splat(3.0)), follow);
        let wb = |c: Vec3| c / white;
        let tint = input.tint.map(|t| t.clamp(Vec3::splat(0.25), Vec3::splat(4.0)));
        let sun_disc_clear = wb(raw.sun) * tint[0];
        // the veil: what it lets through of the disc along the slant path through it
        let veil = input.veil.max(0.0);
        let veil_t = (-veil / s.z.max(0.03)).exp();
        let sun_disc = sun_disc_clear * veil_t;
        let sun_at = {
            let air = Aerosol::of(input);
            let table = depth_table(air.height);
            let heights = [input.cloud_base, 0.5 * (input.cloud_base + input.cloud_top), input.cloud_top, CLOUD_SUN_HEIGHTS[3]];
            let mut at = heights.map(|h| wb(SUN_E0 * transmittance(table.lookup(OBSERVER_H + h, s.z), &air)) * tint[0]);
            // (the cumulus layer lies under the veil, the high layer is the veil)
            for a in at.iter_mut().take(3) {
                *a *= veil_t;
            }
            at
        };
        let sun = sun_disc * input.sun_visibility.clamp(0.0, 1.0);
        let oc = input.overcast.clamp(0.0, 1.0);
        // the moon: its own pass through the same air while it is up and the sun is down
        // far enough for it to matter
        let m = input.moon_dir.normalize_or_zero();
        let moon_e0 = MOON_E0 * input.moon_illum.clamp(0.0, 1.0).powf(2.3);
        let moon = if m.z > -0.05 && s.z < 0.05 && moon_e0 > 1e-9 { Some(RawSky::compute(m, input, moon_e0)) } else { None };
        let moon_disc = moon.as_ref().map(|r| r.sun / daylight * MOON_TINT).unwrap_or(Vec3::ZERO);
        let moon_lut = moon.as_ref().map(|r| moon_in_sun_frame(r, m, s));
        // the clear sky, greyed and evened out under a closed cover (a CIE overcast sky:
        // the zenith three times as bright as the horizon, a little brighter round where
        // the sun stands behind it), holding what the deck lets through of what sun and sky
        // together gave: a conservative scattering layer of optical depth tau passes
        // 1 / (1 + 3/4 (1 - g) tau) of it (two-stream, droplets' g = 0.85) - a grey stratus
        // deck (tau about 15) a little over a third, a raining nimbostratus (60) an eighth -
        // and what the ground throws back up, the deck sends down again: under a cover
        // the light of a snowy day is half again as bright as of a green one
        let clear_global = (raw.sun * s.z.max(0.0) + raw.sky_horizontal).dot(LUM);
        let tau = if input.deck_tau > 0.0 { input.deck_tau } else { 15.0 + 45.0 * input.rain.clamp(0.0, 1.0) };
        let deck_t = 1.0 / (1.0 + 0.75 * (1.0 - 0.85) * tau);
        let overcast_e = clear_global * deck_t / (1.0 - (1.0 - deck_t) * input.ground_albedo.clamp(0.0, 0.9));
        let grey = Vec3::new(0.96, 0.98, 1.0);
        let (w, h) = (SKY_LUT_W as usize, SKY_LUT_H as usize);
        // the cover's shape, normalised below to its irradiance
        let thin = (1.0 - input.rain.clamp(0.0, 1.0)) * smoothstep(-0.05, 0.2, s.z);
        let mut cover_shape = Vec::with_capacity(w * h);
        for row in 0..h {
            for col in 0..w {
                let d = cell_dir(s, row, col);
                let el = d.z.max(0.0);
                let towards_sun = d.dot(s).max(0.0);
                cover_shape.push((1.0 + 2.0 * el) / 3.0 * (1.0 + 1.2 * thin * towards_sun.powi(4)));
            }
        }
        let shape_e = horizontal_irradiance(&cover_shape.iter().map(|&v| Vec3::splat(v)).collect::<Vec<_>>()).x.max(1e-9);
        // a city's glow on its haze and its clouds, the floor of a dark sky
        let glow_gain = input.city_glow.max(0.0) * (1.0 + 2.0 * oc + 0.4 * (input.haze - 1.0).clamp(0.0, 4.0));
        // what the veil scatters of the sun: ice crystals scatter nearly every colour alike
        // and strongly forward (g about 0.75); of the light it takes from the beam some 80 %
        // comes down, spread over the sky round the sun. Its shape is normalised to that
        // irradiance on the ground.
        const VEIL_G: f32 = 0.75;
        let hg = |c: f32| (1.0 - VEIL_G * VEIL_G) / (4.0 * std::f32::consts::PI * (1.0 + VEIL_G * VEIL_G - 2.0 * VEIL_G * c).max(1e-4).powf(1.5));
        let veil_e = sun_disc_clear * s.z.max(0.0) * (1.0 - veil_t) * 0.8;
        let veil_shape: Vec<f32> = (0..w * h).map(|i| hg(cell_dir(s, i / w, i % w).dot(s)) * (cell_dir(s, i / w, i % w).z > 0.0) as i32 as f32).collect();
        let veil_norm = horizontal_irradiance(&veil_shape.iter().map(|&v| Vec3::splat(v)).collect::<Vec<_>>()).x.max(1e-9);
        let mut lut: Vec<Vec3> = Vec::with_capacity(w * h);
        for row in 0..h {
            let el = lut_elevation((row as f32 + 0.5) / h as f32);
            for col in 0..w {
                let i = row * w + col;
                let mut clear = wb(raw.lut[i]) * tint[1];
                if veil > 0.0 && el > 0.0 {
                    // (the part of the clear sky's light that comes from above the veil is
                    // dimmed by it; most of the sky's blue is made below 8 km)
                    let t_cell = (-veil / el.sin().max(0.05)).exp();
                    clear = clear * (0.65 + 0.35 * t_cell) + veil_e * (veil_shape[i] / veil_norm);
                }
                if let Some(ml) = &moon_lut {
                    clear += ml[i] / daylight * MOON_TINT;
                }
                let cover = grey * (overcast_e / shape_e) * cover_shape[i];
                let l = if el >= 0.0 { clear.lerp(cover, oc) } else { clear * (1.0 - oc) };
                let low = 1.0 - el.max(0.0).sin();
                let glow = CITY_GLOW * glow_gain * (0.45 + 1.8 * low * low * low);
                lut.push(l + glow + NATURAL_NIGHT);
            }
        }
        // irradiance on a horizontal surface from the open sky
        let sky_horizontal_open = horizontal_irradiance(&lut);
        // the moon itself lights the street (no shadow of its own: it comes in as part of
        // the sky's light, from its direction)
        let moon_light = moon_disc * (1.0 - 0.9 * oc) * smoothstep(-0.02, 0.06, m.z);
        let sky_horizontal = sky_horizontal_open + moon_light * m.z.max(0.0);
        // The SH of the upper half as a street sees it: the lowest part of the sky is hidden
        // behind houses and trees, which throw back what the sun and the sky give them -
        // warm where they face the sun, dim where they turn away from it. Taken as open sky,
        // the shade was as blue as the sky and a low sun lit it from all round.
        let ground_e = sun * s.z.max(0.0) + sky_horizontal;
        let sun_az = s.y.atan2(s.x);
        // the cumulus seen from below: sunlit edges and grey bases, brighter towards the sun
        let cu = input.cumulus.clamp(0.0, 1.0);
        let cloud_base = (sun_disc * s.z.max(0.0) * 0.5 + sky_horizontal_open * 0.7) / std::f32::consts::PI * Vec3::new(0.97, 0.99, 1.0);
        let az_step = std::f32::consts::PI / w as f32;
        let mut sh = [Vec3::ZERO; 9];
        for row in (h / 2)..h {
            let (e0, e1) = (lut_elevation(row as f32 / h as f32), lut_elevation((row + 1) as f32 / h as f32));
            let el = lut_elevation((row as f32 + 0.5) / h as f32);
            let d_el = e1 - e0;
            let hidden = SURROUND_MAX * (1.0 - smoothstep(0.0, SURROUND_ELEVATION.to_radians(), el));
            for col in 0..w {
                let mut l = lut[row * w + col];
                let az = (col as f32 + 0.5) * az_step;
                if cu > 0.0 && el > 0.0 {
                    // (towards the horizon the heaps stand behind one another and close up)
                    let seen = (cu * (1.0 + 1.5 * (1.0 - el.sin()).powi(2))).min(1.0);
                    let towards = (az.cos() * el.cos() * s.z.max(0.0).powi(2).mul_add(-1.0, 1.0).max(0.0).sqrt() + el.sin() * s.z).max(0.0);
                    l = l.lerp(cloud_base * (1.0 + 0.8 * towards.powi(3)), seen * 0.9);
                }
                // seen towards the sun, a facade is in its own shade; away from it, sunlit
                let facing_sun = (-az.cos()).max(0.0);
                let facade_e = sun * (s.z.max(0.0).powi(2) - 1.0).abs().sqrt() * facing_sun * 0.8 + sky_horizontal * 0.5 + ground_e * input.ground_albedo * 0.5;
                let facade = facade_e * (SURROUND_ALBEDO / std::f32::consts::PI) * tint[2];
                let l = l.lerp(facade, hidden);
                for side in [-1.0f32, 1.0] {
                    let a = sun_az + side * az;
                    let d = Vec3::new(el.cos() * a.cos(), el.cos() * a.sin(), el.sin());
                    let dw = el.cos() * az_step * d_el;
                    for (k, y) in sh_basis(d).iter().enumerate() {
                        sh[k] += l * *y * dw;
                    }
                }
            }
        }
        // (the moonlight itself is a directional light of its own, `moon_light`, which the
        // enhanced pass shades with the moon's shadow: in the harmonics it lit every side a
        // little and cast no shadow at all)
        // the ground: lit by the sun and the sky, seen as the lower half of the sphere
        let ground = (sun * s.z.max(0.0) + sky_horizontal) * input.ground_albedo / std::f32::consts::PI * tint[2];
        let lower = ground_sh(ground);
        for k in 0..9 {
            sh[k] += lower[k];
        }
        // the table's own lower half: the distant ground seen through the air
        for row in 0..h / 2 {
            for col in 0..w {
                let i = row * w + col;
                let el = lut_elevation((row as f32 + 0.5) / h as f32);
                let fade = (-el).sin().clamp(0.0, 1.0).powf(0.35);
                lut[i] = lut[i].lerp(ground, fade * 0.85 + 0.15 * oc);
            }
        }
        // cosine lobe convolution, and the eye's own white balance in the shade: it takes
        // the sky's blue in part for white (a linear map, so it applies to every coefficient)
        let band = [std::f32::consts::PI, 2.0 * std::f32::consts::PI / 3.0, std::f32::consts::PI / 4.0];
        for (k, c) in sh.iter_mut().enumerate() {
            *c *= band[if k == 0 { 0 } else if k < 4 { 1 } else { 2 }];
            let grey = Vec3::splat(c.dot(LUM));
            *c = grey + (*c - grey) * (1.0 - SHADE_ADAPTATION);
        }
        let lut_scale = lut.iter().map(|l| l.max_element()).fold(1e-6f32, f32::max);
        let lut_out = lut.iter().map(|l| [l.x / lut_scale, l.y / lut_scale, l.z / lut_scale, 1.0]).collect();
        // the light the eye adapts to: a low sun still falls fully on the walls that face it,
        // which a horizontal surface alone does not tell (a sunlit facade at seven in the
        // evening came out washed out)
        let sun_facing = s.z.max(0.0) + (1.0 - s.z.max(0.0)) * LOW_SUN_WALLS * (s.z * 20.0).clamp(0.0, 1.0);
        let e_sun = (sun * sun_facing).dot(LUM);
        // (the rest as a surface facing up gets it from the sky and the clouds in it)
        // (a village's few lamps keep less light round the viewer than a city's streets)
        let e_artificial = ARTIFICIAL * input.city_glow.clamp(0.3, 1.0);
        let e_rest = sh_irradiance(&sh, Vec3::Z).dot(LUM).max(sky_horizontal.dot(LUM)) + e_artificial;
        SkyState { input: *input, sun, sun_disc, moon_disc, sun_at, lut: lut_out, lut_scale, sh, sky_horizontal, ground, exposure: exposure_for(e_sun + e_rest), e_sun, e_rest, moon_light, e_artificial }
    }
}

/// The SH of a uniformly bright lower hemisphere (the ground).
fn ground_sh(l: Vec3) -> [Vec3; 9] {
    // projection of the lower half-sphere indicator: Y00 · 2π, Y10 · -π, Y20 · 0
    let mut out = [Vec3::ZERO; 9];
    out[0] = l * (0.282_095 * 2.0 * std::f32::consts::PI);
    out[2] = l * (-0.488_603 * std::f32::consts::PI);
    out
}

/// Pre-exposure for a reference irradiance: full exposure by day, only part of the way
/// up at night (a street at night still looks dark - the eye does not adapt completely).
///
/// How far the eye adapts is its key (Krawczyk, Myszkowski and Seidel, "Lightness
/// perception in tone reproduction for high dynamic range images", 2005, after the
/// lightness perception data of Gilchrist): the mid grey a scene is seen at falls with the
/// luminance the eye is adapted to, `1.03 - 2 / (2 + log10(L + 1))` with L in cd/m². A
/// sunny street (some 6000 cd/m²) is seen at 0.69 of it, an overcast one at 0.6, a street
/// under its lamps (0.5 cd/m²) at 0.12 - two and a half stops darker than a full
/// adaptation would show it - and a moonlit field at 0.03, four and a half. (A plain power
/// law of the light left the lit street only 1.4 stops under that, and the metering lifted
/// it most of the rest of the way: the night came out as a dim, even overcast day.)
pub fn exposure_for(e_ref: f32) -> f32 {
    let e = e_ref.max(1e-6);
    // two thirds of a stop over "mid grey in full light = 0.18" (the tone curve's contrast
    // adds as much again to the bright half), as a camera exposes a sunny street: its soft
    // shoulder holds the sunlit white
    1.6 * std::f32::consts::PI / e * adaptation_key(e) / adaptation_key(DAY_REFERENCE)
}

/// The eye's key (see `exposure_for`) for the adapting irradiance `e` (1 = 10 000 lux): the
/// luminance it adapts to is that of a mid grey surface in this light.
fn adaptation_key(e: f32) -> f32 {
    let l = e * 10_000.0 * 0.18 / std::f32::consts::PI;
    1.03 - 2.0 / (2.0 + (l + 1.0).log10())
}

/// A half float from a float (for the sky table's upload).
pub fn f16_bits(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x007f_ffff;
    if v.is_nan() {
        return 0x7e00;
    }
    if exp >= 31 {
        return sign | 0x7c00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (mant | 0x0080_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    sign | ((exp as u16) << 10) | (((mant + 0x1000) >> 13) as u16).min(0x3ff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(c: Vec3) -> f32 {
        c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
    }

    fn sun_at(deg: f32) -> Vec3 {
        let a = deg.to_radians();
        Vec3::new(0.0, -a.cos(), a.sin())
    }

    #[test]
    fn noon_light_is_white_and_the_shade_realistic() {
        let s = SkyState::compute(&SkyInput { sun_dir: sun_at(60.0), ..Default::default() });
        let global = s.sun * 0.866 + s.sky_horizontal;
        // the camera is balanced for this light
        assert!((global.x / global.y - 1.0).abs() < 0.02 && (global.z / global.y - 1.0).abs() < 0.02, "{global:?}");
        // about 11 units in all; the clear sky gives a tenth to a quarter of it
        assert!((lum(global) - 11.0).abs() < 2.5, "{}", lum(global));
        let share = lum(s.sky_horizontal) / lum(global);
        assert!((0.09..0.25).contains(&share), "sky share {share}");
        // the sun is a little warmer than the sky, the sky bluer than the sun
        assert!(s.sun.x > s.sun.z && s.sky_horizontal.z > s.sky_horizontal.x);
        // SH: a surface facing up gets the sky plus a little from nowhere below
        let up = sh_irradiance(&s.sh, Vec3::Z);
        assert!((lum(up) / lum(s.sky_horizontal) - 1.0).abs() < 0.1, "{up:?} vs {:?}", s.sky_horizontal);
        // a wall gets half the ground's bounce and some of the sky and the houses across the
        // street, the ground's underside only the bounce
        let wall = sh_irradiance(&s.sh, Vec3::X);
        let down = sh_irradiance(&s.sh, -Vec3::Z);
        assert!(lum(wall) > 0.6 * lum(down) && lum(down) > 0.0, "wall {wall:?} down {down:?}");
        // the shade is bluish, but not as blue as the sky
        let shade_blue = up.z / up.x;
        assert!(shade_blue > 1.05 && shade_blue < s.sky_horizontal.z / s.sky_horizontal.x, "{up:?}");
    }

    #[test]
    fn low_sun_is_golden_and_twilight_blue() {
        let low = SkyState::compute(&SkyInput { sun_dir: sun_at(9.6), ..Default::default() });
        assert!(low.sun.x > low.sun.y * 1.15 && low.sun.y > low.sun.z * 1.2, "{:?}", low.sun);
        let dusk = SkyState::compute(&SkyInput { sun_dir: sun_at(-4.0), ..Default::default() });
        assert!(lum(dusk.sun) < 1e-3, "{:?}", dusk.sun);
        assert!(dusk.sky_horizontal.z > dusk.sky_horizontal.x, "{:?}", dusk.sky_horizontal);
        let night = SkyState::compute(&SkyInput { sun_dir: sun_at(-30.0), ..Default::default() });
        // a moonless city night: the glow of its own lamps, a few hundredths of a lux, warm
        assert!(lum(night.sky_horizontal) < 1e-4 && lum(night.sky_horizontal) > 2e-6, "{:?}", night.sky_horizontal);
        assert!(night.sky_horizontal.x > night.sky_horizontal.z, "{:?}", night.sky_horizontal);
        // exposure rises into the night, but not all the way
        assert!(night.exposure > low.exposure * 100.0);
        // a white wall under the street lamps comes out clearly below white, one lit by
        // the night sky alone dark but not black
        let lamp = 0.9 / std::f32::consts::PI * ARTIFICIAL * night.exposure;
        let sky = 0.3 / std::f32::consts::PI * lum(night.sky_horizontal) * night.exposure;
        assert!((0.1..0.6).contains(&lamp) && (0.0004..0.03).contains(&sky), "lamp {lamp} sky {sky}");
    }

    #[test]
    fn the_moon_lights_the_night_and_clouds_throw_back_the_city() {
        let dark = SkyInput { sun_dir: sun_at(-30.0), ..Default::default() };
        let base = SkyState::compute(&dark);
        // a full moon 40 degrees up: several times the light of the glow alone, coming from
        // the moon's side, and its disc in the sky
        let moon_dir = Vec3::new(0.3, 0.7, 0.64).normalize();
        let full = SkyState::compute(&SkyInput { moon_dir, moon_illum: 1.0, ..dark });
        assert!(lum(full.sky_horizontal) > 3.0 * lum(base.sky_horizontal), "{:?} vs {:?}", full.sky_horizontal, base.sky_horizontal);
        assert!(lum(full.moon_disc) > 1e-5, "{:?}", full.moon_disc);
        // (its direct light, which the moon's shadow takes away, stronger than all the
        // night sky's light round it)
        assert!(lum(full.moon_light) > lum(base.sky_horizontal), "{:?}", full.moon_light);
        // a half moon gives far less than half the light
        let half = SkyState::compute(&SkyInput { moon_dir, moon_illum: 0.5, ..dark });
        assert!(lum(half.moon_disc) < 0.3 * lum(full.moon_disc));
        // an overcast city night is brighter than a clear one: the deck throws the lamps back
        let deck = SkyState::compute(&SkyInput { overcast: 1.0, sun_visibility: 0.0, ..dark });
        assert!(lum(deck.sky_horizontal) > 2.0 * lum(base.sky_horizontal), "{:?}", deck.sky_horizontal);
    }

    #[test]
    fn clouds_stay_lit_after_the_sun_has_set_for_the_street() {
        // two degrees below the horizon: no sun in the street, but the cumulus layer and
        // above all the high layer still in a red sun
        let st = SkyState::compute(&SkyInput { sun_dir: sun_at(-2.0), ..Default::default() });
        assert!(lum(st.sun_disc) < 1e-4, "{:?}", st.sun_disc);
        assert!(lum(st.sun_at[3]) > 0.01 && lum(st.sun_at[3]) > lum(st.sun_at[0]), "{:?}", st.sun_at);
        assert!(st.sun_at[3].x > 2.0 * st.sun_at[3].z, "{:?}", st.sun_at[3]);
        // at noon the clouds get about what the street gets, a little more
        let noon = SkyState::compute(&SkyInput { sun_dir: sun_at(60.0), ..Default::default() });
        let r = lum(noon.sun_at[0]) / lum(noon.sun_disc);
        assert!((1.0..1.15).contains(&r), "{r}");
    }

    #[test]
    fn humid_haze_whitens_the_sky_and_reddens_the_sunset() {
        let clean = SkyState::compute(&SkyInput { sun_dir: sun_at(45.0), haze: 0.6, angstrom: 1.5, ..Default::default() });
        let humid = SkyState::compute(&SkyInput { sun_dir: sun_at(45.0), haze: 2.5, angstrom: 0.6, ..Default::default() });
        let blue = |st: &SkyState| st.sky_horizontal.z / st.sky_horizontal.x;
        assert!(blue(&clean) > blue(&humid) * 1.15, "{} {}", blue(&clean), blue(&humid));
        let low_clean = SkyState::compute(&SkyInput { sun_dir: sun_at(4.0), haze: 0.6, angstrom: 1.5, ..Default::default() });
        let low_humid = SkyState::compute(&SkyInput { sun_dir: sun_at(4.0), haze: 2.5, angstrom: 0.6, ..Default::default() });
        // (raw, before the camera's white balance follows it part of the way)
        let red = |st: &SkyState| st.sun_disc.x / st.sun_disc.z.max(1e-9);
        assert!(red(&low_humid) > red(&low_clean), "{} {}", red(&low_humid), red(&low_clean));
    }

    #[test]
    fn overcast_is_grey_and_darker() {
        let clear = SkyState::compute(&SkyInput { sun_dir: sun_at(40.0), ..Default::default() });
        let grey = SkyState::compute(&SkyInput { sun_dir: sun_at(40.0), sun_visibility: 0.0, overcast: 1.0, ..Default::default() });
        let g = grey.sky_horizontal;
        assert!((g.z / g.x) < 1.15, "{g:?}");
        let clear_global = lum(clear.sun * sun_at(40.0).z + clear.sky_horizontal);
        assert!((0.2..0.5).contains(&(lum(g) / clear_global)), "{} of {}", lum(g), clear_global);
        assert!(grey.exposure > clear.exposure * 1.5);
    }

    /// `cargo test -p omsi-render sky_report -- --nocapture --ignored`: the sky as the
    /// screen shows it (sRGB after the exposure, before the metering) for tuning.
    #[test]
    #[ignore]
    fn sky_report() {
        let srgb = |c: f32| {
            let c = c.clamp(0.0, 1.0);
            (if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }) * 255.0
        };
        for (sun_el, oc) in [(60.0f32, 0.0f32), (22.0, 0.0), (8.0, 0.0), (1.0, 0.0), (-4.0, 0.0), (-20.0, 0.0), (40.0, 1.0)] {
            let input = SkyInput { sun_dir: sun_at(sun_el), overcast: oc, sun_visibility: 1.0 - oc, ..Default::default() };
            let st = SkyState::compute(&input);
            let e = st.exposure;
            println!("sun {sun_el:5.1}° overcast {oc}: sun {:?} sky_h {:?} exposure {e:.3} (log2 {:.2})", st.sun, st.sky_horizontal, e.log2());
            let sun_az = input.sun_dir.y.atan2(input.sun_dir.x);
            for rel_az in [0.0f32, 90.0, 180.0] {
                let mut line = format!("  az {rel_az:5.0}:");
                for el in [2.0f32, 8.0, 20.0, 45.0, 89.0] {
                    let a = sun_az + rel_az.to_radians();
                    let d = Vec3::new(el.to_radians().cos() * a.cos(), el.to_radians().cos() * a.sin(), el.to_radians().sin());
                    // the table cell nearest to d
                    let az = {
                        let (s2, d2) = (glam::Vec2::new(input.sun_dir.x, input.sun_dir.y).normalize(), glam::Vec2::new(d.x, d.y).normalize());
                        s2.dot(d2).clamp(-1.0, 1.0).acos()
                    };
                    let col = ((az / std::f32::consts::PI) * SKY_LUT_W as f32).floor().min(SKY_LUT_W as f32 - 1.0) as usize;
                    let row = (lut_row(el.to_radians()) * SKY_LUT_H as f32).floor().min(SKY_LUT_H as f32 - 1.0) as usize;
                    let l = st.lut[row * SKY_LUT_W as usize + col];
                    let c = Vec3::new(l[0], l[1], l[2]) * st.lut_scale * e;
                    line += &format!("  {el:2.0}°({:3.0},{:3.0},{:3.0})", srgb(c.x), srgb(c.y), srgb(c.z));
                }
                println!("{line}");
            }
            let n = |v: Vec3| format!("({:3.0},{:3.0},{:3.0})", srgb(v.x), srgb(v.y), srgb(v.z));
            // a grey card (albedo 0.18) facing up, facing the sun's side, facing away
            let up = (st.sun * input.sun_dir.z.max(0.0) + sh_irradiance(&st.sh, Vec3::Z)) * 0.18 / std::f32::consts::PI * e;
            let shade = sh_irradiance(&st.sh, Vec3::Z) * 0.18 / std::f32::consts::PI * e;
            let wall_away = sh_irradiance(&st.sh, -Vec3::new(input.sun_dir.x, input.sun_dir.y, 0.0).normalize()) * 0.18 / std::f32::consts::PI * e;
            println!("  grey card: sunlit up {}  shaded up {}  wall facing away {}", n(up), n(shade), n(wall_away));
        }
    }

    #[test]
    fn half_floats() {
        for v in [0.0f32, 1.0, -2.5, 0.5, 65504.0, 1e-3, 3.140625] {
            let b = f16_bits(v);
            let sign = if b & 0x8000 != 0 { -1.0 } else { 1.0 };
            let e = ((b >> 10) & 0x1f) as i32;
            let m = (b & 0x3ff) as f32;
            let back = if e == 0 { sign * m / 1024.0 * 2f32.powi(-14) } else { sign * (1.0 + m / 1024.0) * 2f32.powi(e - 15) };
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-6, "{v} -> {back}");
        }
        assert_eq!(f16_bits(1e9), 0x7c00);
    }
}

