//! The enhanced picture's light model on the CPU: the sky's input and its computation on a
//! helper thread, the fog, the exposure the eye adapts to, the clouds' shade on the street.

use crate::*;

/// `OMSI_DEBUG_ENHANCED=n`: the enhanced main pass shows one of its terms alone (1 sun
/// shadow, 2 AO, 3 normal, 4 air transmittance, 5 ambient light, 6 reflection, 7 albedo,
/// 8 direct sun, 9 in-scattered air, 11 distance/depth, 12 alpha mode/terrain/surface,
/// 13 cab/AO/specular occlusion, 14 direct + ambient, 15 lamps and headlights, 16 what
/// glows by itself, 17 roughness/F0/metalness, 10 alpha/glass/envmap); `OMSI_ENV_PHOTO=0`
/// leaves the `[matl_envmap]` photo's structure out of the reflections. A blended surface
/// shows its values as if it were opaque: an invisible layer round the player's bus hides
/// the bus in these views.
pub(crate) fn debug_view() -> f32 {
    static VIEW: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *VIEW.get_or_init(|| {
        omsi_cfg::flags::OMSI_DEBUG_ENHANCED.parse()
            .unwrap_or(0.0)
    })
}

/// The metering: the share of the metered difference that is corrected, its target (log2 of
/// the picture's mean luminance), how far it may darken and brighten (EV), the exposure
/// bias (EV) and the night vision strength. `OMSI_METER=gain,target,dark,bright,bias,night`
/// overrides them for tuning.
pub(crate) fn meter_tuning() -> [f32; 6] {
    static METER: std::sync::OnceLock<[f32; 6]> = std::sync::OnceLock::new();
    *METER.get_or_init(|| {
        let mut m = [
            METER_GAIN,
            METER_TARGET,
            METER_DARKEN,
            METER_BRIGHTEN,
            0.0,
            NIGHT_VISION,
        ];
        if let Some(v) = omsi_cfg::flags::OMSI_METER.var() {
            for (k, x) in v.split(',').take(6).enumerate() {
                if let Ok(x) = x.trim().parse() {
                    m[k] = x;
                }
            }
        }
        m
    })
}

/// The street's scale of the ambient light: how far the sky's occlusion by the houses,
/// trees and hills reaches (m), how much of the occluded sky it takes away, the bounced
/// light's gain and the brightest radiance it is taken from (1 = 10 000 cd/m²: the sun on
/// white plaster is about 2.5). `OMSI_WIDE_AO=reach,strength,gain,max` overrides.
pub(crate) fn occlusion_tuning(street_light: bool) -> [f32; 4] {
    if !street_light {
        return [0.0; 4];
    }
    static T: std::sync::OnceLock<[f32; 4]> = std::sync::OnceLock::new();
    *T.get_or_init(|| {
        let mut t = [WIDE_AO_REACH, WIDE_AO_STRENGTH, BOUNCE_GAIN, BOUNCE_MAX];
        if let Some(v) = omsi_cfg::flags::OMSI_WIDE_AO.var() {
            for (k, x) in v.split(',').take(4).enumerate() {
                if let Ok(x) = x.trim().parse() {
                    t[k] = x;
                }
            }
        }
        t
    })
}
/// Whether the AO pass carries the bounced light in this frame: the Enhanced picture with
/// the AO on, and a gain and a strength to show (it reads the last frame's picture, which
/// then costs a pass and a texture: no other pipeline pays for it).
pub(crate) fn bounce_wanted(ao_on: bool, enhanced: bool, street_light: bool) -> bool {
    let t = occlusion_tuning(street_light);
    ao_on && enhanced && t[1] > 0.0 && t[2] > 0.0
}

/// Where a view-space point of this frame was in the last frame's picture (its clip space),
/// and whether that picture can be read at all: not after a cut (another view, a jump, a
/// long pause, another size), where it shows unrelated surfaces.
pub(crate) fn bounce_history(
    prev: Option<(Mat4, DVec3, (u32, u32), Vec3, Vec3)>,
    view: Mat4,
    ro: DVec3,
    size: (u32, u32),
    pos: Vec3,
    forward: Vec3,
    dt: f32,
) -> ([[f32; 4]; 4], f32) {
    let Some((vp, pro, psize, ppos, pfwd)) = prev else {
        return (Mat4::ZERO.to_cols_array_2d(), 0.0);
    };
    let moved = ((pos.as_dvec3() + ro) - (ppos.as_dvec3() + pro)).length();
    let ok = psize == size && dt < 0.25 && moved < 25.0 && forward.dot(pfwd) > 0.5;
    if !ok {
        return (Mat4::ZERO.to_cols_array_2d(), 0.0);
    }
    ((vp * Mat4::from_translation((ro - pro).as_vec3()) * view.inverse()).to_cols_array_2d(), 1.0)
}

pub(crate) const WIDE_AO_REACH: f32 = 18.0;
pub(crate) const WIDE_AO_STRENGTH: f32 = 0.9;
pub(crate) const BOUNCE_GAIN: f32 = 0.85;
pub(crate) const BOUNCE_MAX: f32 = 4.0;

/// The tone curve's contrast about mid grey for a pre-exposure (its natural log): a
/// camera's by day, falling off through the dusk to a little under none at night, where
/// the eye's own adaptation to the dark between the street lamps lifts it out of black
/// (a street a few hundred times darker than the pool under a lamp is still seen). `OMSI_TONE_CONTRAST=day,night`
/// overrides it.
pub(crate) fn tone_contrast(log_pre: f32) -> f32 {
    static OVERRIDE: std::sync::OnceLock<Option<(f32, f32)>> = std::sync::OnceLock::new();
    let (day, night) = OVERRIDE
        .get_or_init(|| {
            let v = omsi_cfg::flags::OMSI_TONE_CONTRAST.var()?;
            let mut it = v.split(',').map(|x| x.trim().parse::<f32>().ok());
            Some((it.next()??, it.next().flatten().unwrap_or(1.0)))
        })
        .unwrap_or((TONE_CONTRAST_DAY, TONE_CONTRAST_NIGHT));
    // (the pre-exposure is about 1 by day, 70 in the blue hour and 400-700 at night)
    let t = atmosphere::smoothstep(2.0, 8.0, log_pre / std::f32::consts::LN_2);
    day + (night - day) * t
}

/// How much of the sun the enhanced sky's cumulus lets through to the camera: the same
/// cloud the sky shader draws (sky_enhanced.wgsl `cloud_base_shape`, `cloud_sigma`: the
/// shape map's heaps cut by the cover, rounded with height, without the billows), its
/// extinction integrated along the sun's direction through the layer, over a few rays a
/// few tens of metres apart (the penumbra of a cloud a kilometre and a half up).
pub(crate) const CLOUD_SHADOW_EROSION: f32 = 0.15;
/// How far the exposure follows the light into a cloud's shadow (log terms).
pub(crate) const CLOUD_SHADE_ADAPT: f32 = 0.5;

/// The cumulus layer's bottom and top in the render origin's frame (where the camera and the
/// sky shader are): the weather model's heights are above the ground (the fog's base, under
/// the player's vehicle or just under the camera), so a map in the hills keeps its clouds.
pub(crate) fn cloud_levels(lighting: &Lighting, cam_rel: Vec3, ro: DVec3) -> [f32; 2] {
    match lighting.cloud_layer {
        Some([b, t]) => {
            let ground = match lighting.fog_base.or(lighting.inside.map(|v| v.0.z)) {
                Some(z) => (z - ro.z) as f32,
                None => cam_rel.z - 2.0,
            };
            [ground + b, ground + t]
        }
        None => [1400.0, 2800.0],
    }
}

pub(crate) fn cloud_sun_transmittance(shape: &[u8], lighting: &Lighting, cam_rel: Vec3, ro: DVec3) -> f32 {
    let [bottom, top] = cloud_levels(lighting, cam_rel, ro);
    const PERIOD: f32 = 13000.0;
    const SIGMA: f32 = 0.035;
    let s = lighting.sun_dir.normalize_or_zero();
    if s.z <= 0.02 || shape.is_empty() {
        return 1.0;
    }
    let size = (clouds::SHAPE_SIZE / 2) as usize;
    if shape.len() < size * size * 4 {
        return 1.0;
    }
    let texel = |x: i64, y: i64| {
        let (x, y) = (x.rem_euclid(size as i64) as usize, y.rem_euclid(size as i64) as usize);
        let i = (y * size + x) * 4;
        [shape[i] as f32 / 255.0, shape[i + 1] as f32 / 255.0, shape[i + 2] as f32 / 255.0]
    };
    let sample = |u: f32, v: f32| {
        let (x, y) = (u * size as f32 - 0.5, v * size as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = texel(x0, y0);
        let b = texel(x0 + 1, y0);
        let c = texel(x0, y0 + 1);
        let d = texel(x0 + 1, y0 + 1);
        let mut o = [0.0f32; 3];
        for k in 0..3 {
            o[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
        }
        o
    };
    let lin = |a: f32, b: f32, v: f32| ((v - a) / (b - a)).clamp(0.0, 1.0);
    let cover = lighting.cloud_density.clamp(0.0, 1.0);
    // (less the billows' erosion, which the sky shader takes off the heaps' edges and this
    // leaves out: on average about this much of the cover)
    let coverage = (0.3 + cover * 0.55 - CLOUD_SHADOW_EROSION).clamp(0.0, 1.0);
    let origin = glam::Vec2::new(ro.x.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32, ro.y.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32);
    let drift = glam::Vec2::new(lighting.cloud_offset[0], lighting.cloud_offset[1]) * 2500.0;
    let t0 = (bottom - cam_rel.z).max(0.0) / s.z;
    let t1 = ((top - cam_rel.z).max(0.0) / s.z).min(t0 + 12_000.0);
    let steps = 24;
    let ds = (t1 - t0) / steps as f32;
    let side = glam::Vec2::new(-s.y, s.x).normalize_or_zero();
    let mut sum = 0.0;
    let offsets = [glam::Vec2::ZERO, side * 40.0, -side * 40.0, glam::Vec2::new(s.x, s.y).normalize_or_zero() * 40.0, -glam::Vec2::new(s.x, s.y).normalize_or_zero() * 40.0];
    for off in offsets {
        let mut od = 0.0;
        for k in 0..steps {
            let t = t0 + (k as f32 + 0.5) * ds;
            let p = cam_rel + s * t;
            let h = (p.z - bottom) / (top - bottom);
            if !(0.0..1.0).contains(&h) {
                continue;
            }
            let g = glam::Vec2::new(p.x, p.y) + origin + off + drift;
            let m = sample(g.x / PERIOD, g.y / PERIOD);
            let lo = m[1] - 1.0;
            let n = h * h * (0.7 + m[2]) + (1.0 - h).powi(16);
            let base = (m[0] - n - lo) / (1.0 - lo) * (lin(0.0, 0.1, h) - lin(0.6, 1.0, h));
            let x = ((base + coverage - 1.0) / 0.12).clamp(0.0, 1.0);
            let dens = x * x * (3.0 - 2.0 * x) * (h / 0.2).min(1.0);
            od += dens * SIGMA * ds;
        }
        sum += (-od).exp();
    }
    sum / offsets.len() as f32
}

/// How the enhanced picture's weather fog thins out with height (1/m): a 300 m scale height
/// over the fog's base (`layer_depth` in enhanced_common.wgsl).
pub(crate) const FOG_FALLOFF: f32 = 1.0 / 300.0;

/// The weather's own fog in the enhanced picture, its extinction at the base (1/m):
/// vanilla's density, which the culling uses as well; none below 1e-4, where a clear day's
/// air is the sky model's.
pub(crate) fn enhanced_weather_fog(lighting: &Lighting) -> f32 {
    let fog = if lighting.fog_density > 1e-4 {
        lighting.fog_density
    } else {
        0.0
    };
    fog + snowfall_extinction(lighting.snowfall)
}

/// How much the lamps round the camera light the night sky over it, relative to a city's
/// (Spandau's middle is 1): a sky's glow is the lamps' upward and reflected light scattered
/// back down by the air, each source's part falling off with the distance to it as
/// d^-2.5 (Walker's law, "The effects of urban lighting on the brightness of the night
/// sky", 1977; the near field evened out over the first few hundred metres). A village's
/// handful of lamps leaves the stars; a city's thousands turn the sky orange-grey.
pub(crate) const SKY_GLOW_REF: f32 = 830.0;
pub(crate) fn lamp_sky_glow(scene: &Scene, cam_rel: Vec3) -> f32 {
    let ro = scene.render_origin;
    let mut sum = 0.0f32;
    for l in &scene.lights {
        if !l.housed || l.intensity <= 0.0 {
            continue;
        }
        let d = ((l.position - ro).as_vec3() - cam_rel).truncate().length();
        let lum = (l.color[0] + l.color[1] + l.color[2]) / 3.0;
        sum += l.intensity * lum * l.core * l.core / (1.0 + (d / 300.0).powf(2.5));
    }
    sum / SKY_GLOW_REF
}

/// The light of the lamps and the headlights on the ground the camera looks at (irradiance,
/// 1 = 10 000 lux): the log-average over the points where a grid of rays through the
/// picture meets the ground (`ground`: the street's height relative to the render origin,
/// the player's vehicle's), each lit as the enhanced pass lights it (a street lamp down
/// and out, a headlamp ahead and under its cut-off). What the eye adapts to by night,
/// rather than a fixed guess.
///
/// The rays are the picture's own: a dozen points 5 to 30 m ahead on a ground taken 1.6 m
/// under the camera missed the street the eye actually sees from any camera higher than a
/// driver's (it lay in the air there, out of every lamp's reach) and the lamp-lit pools to
/// the sides of a narrow view - the meter read a moonless night while the picture was full
/// of lit streets, and the eye's adaptation to that dark lifted every lamp's light to
/// white (the modded maps' lamps, spaced and placed differently from the stock ones, most).
pub(crate) fn view_lamp_light(lights: &[PointLight], ro: DVec3, cam_rel: Vec3, camera: &Camera, aspect: f32, ground: Option<f32>, seen: &dyn Fn(DVec3) -> f32) -> f32 {
    let ground = ground.filter(|g| g.is_finite() && *g < cam_rel.z - 0.3).unwrap_or(cam_rel.z - 1.6);
    let (f, r, u) = (camera.forward(), camera.right(), camera.up());
    let ty = (camera.fov_deg.to_radians() * 0.5).tan();
    let tx = ty * aspect.max(0.1);
    let mut points = Vec::with_capacity(35);
    for j in 0..5 {
        for i in 0..7 {
            let x = -0.9 + 1.8 * i as f32 / 6.0;
            let y = -0.95 + 1.9 * j as f32 / 4.0;
            let dir = (f + r * (x * tx) + u * (y * ty)).normalize();
            if dir.z >= -0.01 {
                continue;
            }
            // (as far as a lamp's light still shapes what the eye takes to: a pool 300 m
            // off is a point of light, not the scene)
            let t = (ground - cam_rel.z) / dir.z;
            if t > 0.0 && t < 300.0 {
                points.push(cam_rel + dir * t);
            }
        }
    }
    if points.is_empty() {
        // (looking up: the ground round the camera, which the eye has just seen)
        let f = Vec3::new(f.x, f.y, 0.0).normalize_or(Vec3::Y);
        for d in [5.0f32, 10.0, 18.0, 30.0] {
            for a in [-0.5f32, 0.0, 0.5] {
                let (s, c) = a.sin_cos();
                let dir = Vec3::new(f.x * c - f.y * s, f.x * s + f.y * c, 0.0);
                points.push(Vec3::new(cam_rel.x, cam_rel.y, ground) + dir * d);
            }
        }
    }
    let mut per_point = vec![0.0f32; points.len()];
    for l in lights {
        if l.intensity <= 0.0 || l.mode == LightMode::Vanilla {
            continue;
        }
        let p = (l.position - ro).as_vec3();
        let lum = 0.2126 * l.color[0] + 0.7152 * l.color[1] + 0.0722 * l.color[2];
        let core = if l.core > 0.0 { l.core } else { l.radius * 0.125 };
        for (pi, x) in points.iter().enumerate() {
            let to = *x - p;
            let d2 = to.length_squared();
            if d2 >= l.radius * l.radius {
                continue;
            }
            let q = d2 / (l.radius * l.radius);
            let window = (1.0 - q * q) * (1.0 - q * q);
            let t = to / d2.sqrt().max(1e-3);
            let e = if l.beam != 0.0 {
                // a headlamp lights the road it stands on (some 0.8 m under it), by its
                // profile (lamp_air.wgsl `headlamp`)
                let g = Vec3::new(x.x, x.y, p.z - 0.8) - p;
                let gd2 = g.length_squared();
                headlamp_profile(g / gd2.sqrt().max(1e-3), l.direction, l.beam > 0.0) / gd2.max(0.3) * window
            } else {
                let mut e = core * core / (d2 * d2 + core.powi(4)).sqrt() * window;
                if l.direction.length_squared() > 1e-6 {
                    e *= atmosphere::smoothstep(l.cone[1], l.cone[0], t.dot(l.direction.normalize()));
                } else if l.housed {
                    e *= 0.05 + 0.95 * atmosphere::smoothstep(-0.1, 0.3, -t.z);
                }
                e
            };
            per_point[pi] += LAMP_E * l.intensity * lum * e;
        }
    }
    // the log-average, as the eye adapts to a field of view (Reinhard's and Krawczyk's
    // adapting luminance): a headlight's pool in front of the camera does not decide it
    // alone, as the plain mean let it (facing a bus's lights, all else went black). The
    // floor is a moonless night's light, which every point has.
    let floor = 2e-6f32;
    let log_sum: f32 = per_point.iter().map(|v| (v + floor).ln()).sum();
    (log_sum / per_point.len() as f32).exp() - floor + glare_veil(lights, ro, cam_rel, camera.forward(), seen)
}

/// The lamps' and headlights' glare in view as the illuminance whose mid grey has the same
/// luminance (1 = 10 000 lux): the light of every lamp the eye sees, scattered in the eye
/// over the field of view, is a veil the eye adapts to as to any other light - the
/// Stiles-Holladay disability glare, L = 10 x the illuminance at the eye (lux) / the angle
/// off the line of sight squared (degrees), summed over the sources (CIE 146). Metered on
/// the ground between the lamps alone, a village full of street lamps in view read as a
/// moonless night, and the eye's adaptation to that dark lifted every lamp's light to white.
pub(crate) fn glare_veil(lights: &[PointLight], ro: DVec3, cam_rel: Vec3, forward: Vec3, seen: &dyn Fn(DVec3) -> f32) -> f32 {
    let veil: f32 = glare_sources(lights, ro, cam_rel, forward).into_iter().map(|(at, v)| v * seen(at)).sum();
    // (the luminance of a mid-grey surface, 18 %, under this illuminance)
    veil * std::f32::consts::PI / 0.18 / 1e4
}

pub(crate) fn glare_sources(lights: &[PointLight], ro: DVec3, cam_rel: Vec3, forward: Vec3) -> Vec<(DVec3, f32)> {
    let f = forward.normalize_or(Vec3::Y);
    let mut sources = Vec::new();
    for l in lights {
        if l.intensity <= 0.0 || l.mode == LightMode::Vanilla {
            continue;
        }
        let to_lamp = (l.position - ro).as_vec3() - cam_rel;
        let d2 = to_lamp.length_squared();
        // (a lamp out past a few hundred metres is a point among the stars to the eye; one
        // within a metre is the bus's own)
        if !(1.0..300.0 * 300.0).contains(&d2) {
            continue;
        }
        let dist = d2.sqrt();
        let theta = f.dot(to_lamp / dist).clamp(-1.0, 1.0).acos().to_degrees();
        if theta >= 90.0 {
            continue;
        }
        let lum = 0.2126 * l.color[0] + 0.7152 * l.color[1] + 0.0722 * l.color[2];
        let core = if l.core > 0.0 { l.core } else { l.radius * 0.125 };
        // towards the eye: from the lamp
        let t = -to_lamp / dist;
        let k = if l.beam != 0.0 {
            headlamp_profile(t, l.direction, l.beam > 0.0) / (core * core).max(1e-3)
        } else if l.direction.length_squared() > 1e-6 {
            atmosphere::smoothstep(l.cone[1], l.cone[0], t.dot(l.direction.normalize()))
        } else if l.housed {
            0.05 + 0.95 * atmosphere::smoothstep(-0.1, 0.3, -t.z)
        } else {
            1.0
        };
        // the lamp's illuminance at the eye (lux): its core's level, falling off with the
        // square of the distance as on the street
        let e_eye = LAMP_E * 1e4 * l.intensity * lum * k * core * core / d2;
        sources.push((l.position, 10.0 * e_eye / theta.max(1.5).powi(2)));
    }
    sources
}

/// A headlamp's intensity towards `t` (unit, from the lamp): lamp_air.wgsl `headlamp`.
pub(crate) fn headlamp_profile(t: Vec3, dir: Vec3, low: bool) -> f32 {
    let fwd = (dir.truncate() + glam::Vec2::new(1e-6, 0.0)).normalize();
    let ahead = t.truncate().dot(fwd);
    if ahead <= 0.0 {
        return 0.0;
    }
    let across = (t.x * fwd.y - t.y * fwd.x).abs() / ahead;
    let wide = 0.12 * atmosphere::smoothstep(1.0, 0.45, across) + 0.88 * (-across * across / 0.06).exp();
    // (about the horizon of the vehicle's floor, which `dir` lies in: see `lamp_air.wgsl`)
    let (rise, tilt) = (t.z / t.truncate().length().max(1e-3), dir.z / dir.truncate().length().max(1e-3));
    let drop = (tilt - rise) / (1.0 + rise * tilt).max(1e-3);
    let mut up = (0.06 / drop.abs().max(1e-4)).powf(3.4).min(1.0);
    if low {
        up *= atmosphere::smoothstep(-0.012, 0.025, drop);
        return wide * up;
    }
    let hot = (-across * across / 0.012 - drop * drop / 0.0004).exp();
    wide * up + 6.0 * hot
}

/// The extinction of falling snow (1/m) for a snowfall of strength `s` (0..1): the
/// meteorological visibility (the distance at which a dark object keeps 5 % of its
/// contrast, 3.0 / extinction) is what a snowfall's strength is reported by - light snow
/// over 800 m, moderate down to 400 m, heavy under that (WMO / NWS) - and goes about
/// inversely with the snowfall rate (Rasmussen et al., J. Appl. Meteor. 1999). A
/// snowflake's big, flat cross-section takes several times the view of a raindrop of
/// the same water.
pub(crate) fn snowfall_extinction(s: f32) -> f32 {
    if s <= 0.01 {
        return 0.0;
    }
    let visibility = 300.0 / s.min(1.0).powf(0.9);
    3.0 / visibility
}

/// How much of the sun's light gets down through the enhanced fog layer (extinction
/// `sigma` at its base, thinning by `FOG_FALLOFF`) from a sun `sun_z` (the sine of its
/// altitude) high: the layer's whole depth along the way, as the sky shader dims the
/// sun's disc by it (`air_of` towards the sun).
pub(crate) fn sun_through_fog(sigma: f32, sun_z: f32) -> f32 {
    if sigma <= 0.0 {
        return 1.0;
    }
    (-sigma / (FOG_FALLOFF * sun_z.max(0.02))).exp()
}

/// What the enhanced sky is computed from for this light, and how much of the sun the
/// clouds let through (the sky shader's `lights.w`, which lights the clouds and the disc).
pub(crate) fn enhanced_sky_input(lighting: &Lighting, city_glow: f32) -> (atmosphere::SkyInput, f32) {
    let s = lighting.sun_dir.normalize_or_zero();
    // haze: the weather's visibility below a few kilometres thickens the aerosol
    let visibility = 2.3 / lighting.fog_density.max(1e-6);
    let mut day = day_air(lighting);
    if let Some([haze, angstrom, height]) = lighting.air {
        day.haze = haze;
        day.angstrom = angstrom;
        day.aerosol_height = height;
    }
    let haze = (8000.0 / visibility).clamp(1.0, 6.0) * day.haze + 2.0 * lighting.rain;
    // rain and snow fall from a closed deck: whatever the cloud type says, the sun is
    // gone and the sky is the grey dome (a low sun scattered orange in the snowfall)
    let wet_cover = (lighting.rain * 1.5).clamp(0.0, 1.0);
    let sun_visibility = lighting.sun_intensity.clamp(0.0, 1.0) * (1.0 - wet_cover);
    // The sun the street gets: none from under an overcast deck (from a cover of 0.85 on
    // the sky shader draws the closed grey dome, `closed` in `cloud_layer`), and in a
    // weather fog only what the fog above lets through - its disc in the sky is dimmed by
    // the same layer. With the whole of it, an overcast day lit the street with a sun no
    // one saw, and a dense fog glowed white all round the sun (#1106).
    let closed = atmosphere::smoothstep(0.85, 1.0, lighting.cloud_density);
    let reaching = sun_visibility * (1.0 - closed) * sun_through_fog(enhanced_weather_fog(lighting), s.z);
    let input = atmosphere::SkyInput {
        sun_dir: s,
        sun_visibility: reaching,
        overcast: lighting.overcast.clamp(0.0, 1.0).max(wet_cover),
        haze,
        rain: lighting.rain.clamp(0.0, 1.0),
        ground_albedo: 0.2 + 0.45 * lighting.snow.clamp(0.0, 1.0),
        tint: lighting.envir_tint,
        // (rain and mist are water: as grey as the droplets are large)
        angstrom: day.angstrom * (1.0 - 0.6 * lighting.rain.clamp(0.0, 1.0)) * (1.0 - 0.5 * ((haze - 3.0) / 3.0).clamp(0.0, 1.0)),
        aerosol_height: day.aerosol_height,
        strat_aod: day.strat_aod,
        veil: lighting.veil.clamp(0.0, 3.0),
        cumulus: if lighting.enhanced { (lighting.cloud_density.min(0.84)) * (1.0 - closed) } else { 0.0 },
        moon_dir: lighting.moon_dir,
        moon_illum: lighting.moon_illum,
        city_glow,
        deck_tau: lighting.deck_tau.unwrap_or(0.0),
        cloud_base: lighting.cloud_layer.map_or(1400.0, |l| l[0]),
        cloud_top: lighting.cloud_layer.map_or(2800.0, |l| l[1]),
    };
    // (the disc in the sky: what the veil lets through of it as well)
    let veil_t = (-input.veil / s.z.max(0.03)).exp();
    (input, sun_visibility * veil_t)
}

/// The day's own air: what the sky of one calendar day is made of, as the weather of a
/// real day leaves it (see `day_air`).
pub(crate) struct DayAir {
    /// aerosol amount relative to a clear day
    pub(crate) haze: f32,
    /// its Ångström exponent (fine dry particles 1.4-1.6, humid haze down to 0.5)
    pub(crate) angstrom: f32,
    /// the depth of the hazy boundary layer (m)
    pub(crate) aerosol_height: f32,
    /// the stratospheric aerosol's optical depth
    pub(crate) strat_aod: f32,
}

/// The day's own air, drawn from the calendar day so that no two days look quite alike -
/// and with it no two sunsets: winter's air is mostly clean and dry (a deep blue sky,
/// crisp distances, a pale yellow low sun) under a shallow inversion, a summer's often
/// hazy and humid (a milky sky, soft distances) and mixed high by the afternoon's heat,
/// and the far north cleaner than the middle of the continent. The boundary layer follows
/// the sun through the day as a real one does: low and dense in the morning (a pastel
/// sunrise through a thin bright haze), growing with the sun's heat until early afternoon
/// and left standing as the evening's residual layer (a low sun shining through all of
/// it). Now and then the stratosphere holds more aerosol than usual, and that evening's
/// twilight turns purple. `OMSI_DAY_AIR=haze,angstrom[,height,strat]` fixes it.
pub(crate) fn day_air(lighting: &Lighting) -> DayAir {
    static FIXED: std::sync::OnceLock<Option<Vec<f32>>> = std::sync::OnceLock::new();
    let fixed = FIXED.get_or_init(|| {
        let v = omsi_cfg::flags::OMSI_DAY_AIR.var()?;
        Some(v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect())
    });
    let hash = |k: u32| {
        let mut x = lighting.day_seed.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 16;
        x = x.wrapping_mul(0x7FEB_352D);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846C_A68B);
        x ^= x >> 16;
        (x & 0xFF_FFFF) as f32 / 16_777_216.0
    };
    // 0 in midwinter, 1 in high summer (half a year later south of the equator)
    let doy = lighting.day_of_year + if lighting.latitude < 0.0 { 182.0 } else { 0.0 };
    let summer = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * (doy - 20.0) / 365.0).cos();
    let north = 1.0 - 0.3 * atmosphere::smoothstep(55.0, 70.0, lighting.latitude.abs());
    let snow = lighting.snow.clamp(0.0, 1.0);
    let base = (0.72 + 0.6 * summer) * north * (1.0 - 0.25 * snow);
    let mut haze = (base * ((hash(1) - 0.5) * 1.1).exp()).clamp(0.35, 3.0);
    let mut angstrom = (1.45 - 0.65 * summer * hash(2) - 0.15 * hash(3)).clamp(0.5, 1.6);
    // the day's deepest mixing (m): a few hundred metres in winter, up to two kilometres
    // on a hot summer afternoon
    let deepest = (500.0 + 1500.0 * summer) * (0.7 + 0.6 * hash(4));
    // how far the day has mixed it: by the sun's height in the morning (azimuth east of
    // south), all of it from the afternoon on (the residual layer stays into the night)
    let sun_alt = lighting.sun_dir.normalize_or_zero().z.max(0.0).asin().to_degrees();
    let morning = lighting.sun_azimuth < std::f32::consts::PI;
    let grown = if morning { 0.35 + 0.65 * atmosphere::smoothstep(0.0, 35.0, sun_alt) } else { 1.0 };
    let mut aerosol_height = (deepest * grown).max(250.0);
    // mostly a clean stratosphere; one day in a dozen or so a vivid one
    let mut strat_aod = 0.002 + 0.004 * hash(5) + 0.03 * hash(6).powi(8);
    if let Some(f) = fixed {
        haze = f.first().copied().unwrap_or(haze);
        angstrom = f.get(1).copied().unwrap_or(angstrom);
        aerosol_height = f.get(2).copied().unwrap_or(aerosol_height);
        strat_aod = f.get(3).copied().unwrap_or(strat_aod);
    }
    DayAir { haze, angstrom, aerosol_height, strat_aod }
}

/// Has the sky moved on far enough from `a` to be computed again? The sun by a tenth of a
/// degree (half a minute of the day), the weather by a per cent.
pub(crate) fn sky_input_differs(a: &atmosphere::SkyInput, b: &atmosphere::SkyInput) -> bool {
    let near = |x: f32, y: f32, tol: f32| (x - y).abs() <= tol;
    a.sun_dir.dot(b.sun_dir) < 0.999_998
        || !near(a.sun_visibility, b.sun_visibility, 0.01)
        || !near(a.overcast, b.overcast, 0.01)
        || !near(a.haze, b.haze, 0.02)
        || !near(a.rain, b.rain, 0.01)
        || !near(a.ground_albedo, b.ground_albedo, 0.01)
        || !near(a.angstrom, b.angstrom, 0.02)
        || !near(a.aerosol_height, b.aerosol_height, 40.0)
        || !near(a.strat_aod, b.strat_aod, 0.0005)
        || !near(a.veil, b.veil, 0.01)
        || !near(a.cumulus, b.cumulus, 0.02)
        || !near(a.deck_tau, b.deck_tau, 0.5)
        || !near(a.cloud_base, b.cloud_base, 60.0)
        || !near(a.cloud_top, b.cloud_top, 100.0)
        || a.moon_dir.dot(b.moon_dir) < 0.9999
        || !near(a.moon_illum, b.moon_illum, 0.01)
        || !near(a.city_glow, b.city_glow, 0.01)
        || a.tint
            .iter()
            .zip(&b.tint)
            .any(|(x, y)| (*x - *y).abs().max_element() > 0.01)
}

/// A sky job for the sky's helper thread: the input, and where its sky goes.
type SkyJob = (atmosphere::SkyInput, std::sync::mpsc::Sender<atmosphere::SkyState>);

/// Compute a sky on the sky's helper thread, one for the game's life (a thread started
/// for every new sky - every half minute of sun and with every change of the weather -
/// cost its start and its stack's mapping on the frame thread each time).
fn sky_worker_compute(input: atmosphere::SkyInput, tx: std::sync::mpsc::Sender<atmosphere::SkyState>) {
    static WORKER: std::sync::OnceLock<Option<std::sync::mpsc::Sender<SkyJob>>> = std::sync::OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (jobs, rx) = std::sync::mpsc::channel::<SkyJob>();
        std::thread::Builder::new()
            .name("sky".into())
            .spawn(move || {
                while let Ok((mut input, mut tx)) = rx.recv() {
                    // (only the newest of the jobs waiting: the others are stale)
                    while let Ok(newer) = rx.try_recv() {
                        (input, tx) = newer;
                    }
                    let _ = tx.send(atmosphere::SkyState::compute(&input));
                }
            })
            .ok()
            .map(|_| jobs)
    });
    let sent = match worker {
        Some(w) => w.send((input, tx)),
        None => Err(std::sync::mpsc::SendError((input, tx))),
    };
    if let Err(std::sync::mpsc::SendError((input, tx))) = sent {
        // (no helper thread: one for this sky)
        std::thread::spawn(move || {
            let _ = tx.send(atmosphere::SkyState::compute(&input));
        });
    }
}

impl Renderer {
    /// Take in a newly computed sky: its table goes to the GPU, the probe is redrawn (on the
    /// next frame).
    pub(crate) fn install_sky(&mut self, st: atmosphere::SkyState) {
        let mut bytes: Vec<u8> = Vec::with_capacity(st.lut.len() * 8);
        for texel in &st.lut {
            for v in texel {
                bytes.extend_from_slice(&atmosphere::f16_bits(*v).to_le_bytes());
            }
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.sky.sky_lut,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atmosphere::SKY_LUT_W * 8),
                rows_per_image: Some(atmosphere::SKY_LUT_H),
            },
            wgpu::Extent3d {
                width: atmosphere::SKY_LUT_W,
                height: atmosphere::SKY_LUT_H,
                depth_or_array_layers: 1,
            },
        );
        if omsi_cfg::flags::OMSI_DEBUG_SKY.is_set() {
            log::info!("sky: sun {:?} (altitude {:.1}°) sky {:?} ground {:?} exposure {:.3} table scale {:.4} haze {:.2} overcast {:.2} rain {:.2} sun visibility {:.2} moon {:?} (altitude {:.1}°, lit {:.2}) city glow {:.2}", st.sun, st.input.sun_dir.z.asin().to_degrees(), st.sky_horizontal, st.ground, st.exposure, st.lut_scale, st.input.haze, st.input.overcast, st.input.rain, st.input.sun_visibility, st.moon_light, st.input.moon_dir.normalize_or_zero().z.asin().to_degrees(), st.input.moon_illum, st.input.city_glow);
        }
        if let Some(p) = self.tone.probe.as_mut() {
            p.sched.sky_new = true;
        }
        self.sky.sky_state = Some(st);
    }

    /// The enhanced path's light for this frame: the sky (recomputed when the sun or the
    /// weather has moved on), the exposure following it, the sky table and the uniform.
    /// Returns whether the reflection probe is to be drawn this frame.
    pub(crate) fn prepare_enhanced(&mut self, lighting: &Lighting, cam_rel: Vec3, ro: DVec3, dt: f32) -> bool {
        let (input, sun_visibility) = enhanced_sky_input(lighting, self.sky.city_glow.unwrap_or(1.0));
        // (a sky taken in by an earlier frame: the probe's turn has come)
        let sky_new = self.tone.probe.as_mut().is_some_and(|p| std::mem::take(&mut p.sched.sky_new));
        // A new sky takes a few milliseconds: it is computed on a helper thread and taken in
        // when it is ready. A picture on its own, and the first frame, wait for it.
        if let Some((_, rx)) = &self.sky.sky_job {
            match rx.try_recv() {
                Ok(st) => {
                    self.sky.sky_job = None;
                    self.install_sky(st);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.sky.sky_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let pending = self
            .sky.sky_job
            .as_ref()
            .map(|j| j.0)
            .or(self.sky.sky_state.as_ref().map(|st| st.input));
        if pending
            .map(|i| sky_input_differs(&i, &input))
            .unwrap_or(true)
        {
            if self.instant_exposure || self.sky.sky_state.is_none() {
                self.sky.sky_job = None;
                self.install_sky(atmosphere::SkyState::compute(&input));
            } else {
                let (tx, rx) = std::sync::mpsc::channel();
                sky_worker_compute(input, tx);
                self.sky.sky_job = Some((input, rx));
            }
        }
        let input_overcast = input.overcast;
        // the cumulus over the street: how much of the sun gets through them along the sun's
        // way from here, a little blurred (a cloud's shadow has a soft edge tens of metres
        // wide) and followed over a moment as the clouds drift
        let cloud_t = {
            let now = if lighting.enhanced && lighting.cloud_density > 0.0 {
                cloud_sun_transmittance(&self.sky.cloud_shape_cpu, lighting, cam_rel, ro)
            } else {
                1.0
            };
            let k = if self.instant_exposure || dt <= 0.0 { 1.0 } else { 1.0 - (-dt / 0.7).exp() };
            if omsi_cfg::flags::OMSI_DEBUG_SKY.is_set() {
                // (what share of the sky's directions are clear, against the cover asked for)
                let mut clear = 0;
                let mut n = 0;
                for i in 0..64 {
                    let az = i as f32 * 0.7;
                    let el = 0.25 + (i % 8) as f32 * 0.09;
                    let mut l = lighting.clone();
                    l.sun_dir = Vec3::new(el.cos() * az.cos(), el.cos() * az.sin(), el.sin());
                    if cloud_sun_transmittance(&self.sky.cloud_shape_cpu, &l, cam_rel, ro) > 0.5 {
                        clear += 1;
                    }
                    n += 1;
                }
                log::info!("cloud sun: {now:.3} here, {clear} of {n} directions clear (cover {:.2})", lighting.cloud_density);
            }
            let t = self.sky.cloud_sun.map(|c| c + (now - c) * k).unwrap_or(now);
            self.sky.cloud_sun = Some(t);
            t
        };
        let st = self.sky.sky_state.as_ref().expect("sky state");
        // the exposure follows the light over a second or two (in log space); an offscreen
        // picture and the first frame take it at once
        // (the eye takes to a cloud's shadow as to any other light: the exposure is for the
        // sun the street actually gets)
        // (half of the way in log terms: a camera, and the eye, take a cloud's shadow as
        // darker - it is darker - only not as much as the light meter says)
        // (by night the eye adapts to the lamps' and headlights' light it sees, measured,
        // instead of a lit city's average: on a dark country road under a full moon it
        // takes to the moonlight, and the moon's shadows show)
        // ... but never below the lit streets of the district the camera is in: the eye's
        // adaptation to the dark takes minutes (the rods' some twenty), and among lamp-lit
        // streets it stays with them while the view passes over a dark yard or a field -
        // taken to the frame's darkness at once, every lamp's light in a lit village went
        // white. The district's level is a lit street's (`ARTIFICIAL`) by how lit the
        // place round the camera is (`city_glow`, its lamps within a few hundred metres);
        // a country road with no lamps round it has none, and the moonlight still decides.
        let district = atmosphere::ARTIFICIAL * st.input.city_glow.clamp(0.0, 1.0);
        let e_rest = match self.tone.view_lamps {
            Some(v) => (st.e_rest - st.e_artificial + v.max(district)).max(1e-6),
            None => st.e_rest,
        };
        let full = atmosphere::exposure_for(st.e_sun + e_rest).max(1e-6).ln();
        let shaded = atmosphere::exposure_for(st.e_sun * cloud_t + e_rest).max(1e-6).ln();
        let target = full + (shaded - full) * CLOUD_SHADE_ADAPT;
        let log_exposure = match self.tone.exposure {
            // (the eye takes to brighter light within a second, to the dark over several:
            // the cones' light adaptation is fast, their dark adaptation slow)
            Some(e) if !self.instant_exposure && dt > 0.0 => {
                let tau = if target < e { 0.6 } else { 3.0 };
                e + (target - e) * (1.0 - (-dt / tau).exp())
            }
            _ => target,
        };
        self.tone.exposure = Some(log_exposure);
        let pre = log_exposure.exp();
        // the fog's in-scattered light: a white sphere's average in this light
        let axes = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        let avg = axes
            .iter()
            .map(|a| atmosphere::sh_irradiance(&st.sh, *a))
            .fold(Vec3::ZERO, |a, b| a + b)
            / 6.0;
        let fog_rgb = avg * 0.9 / std::f32::consts::PI;
        let weather_fog = enhanced_weather_fog(lighting);
        // (the air near the ground: the Rayleigh part and more than the sky model's
        // average aerosol - a city's air near the ground holds its dust and exhaust, which is what gives a street its depth - the houses a few
        // hundred metres off a shade paler and bluer than the ones beside the road)
        let clear_air = 1.8e-5 + 3.6e-5 * input.haze;
        // the fog lies on the ground under the player's vehicle, or just under the camera
        let base = match lighting.fog_base.or(lighting.inside.map(|v| v.0.z)) {
            Some(z) => (z - ro.z) as f32,
            None => cam_rel.z - 2.0,
        };
        let mut sh = [[0.0f32; 4]; 9];
        for (k, c) in st.sh.iter().enumerate() {
            sh[k] = c.extend(0.0).to_array();
        }
        // the probe is drawn every half second (`PROBE_EVERY`), and for a new sky on the frame
        // after it is taken in (its table upload and the probe's six faces and blurrier
        // levels were one frame's work); it holds the sky divided by the table scale it was
        // drawn with, which the uniform passes on, so a frame on the old one is right
        let instant = self.instant_exposure;
        let (redraw, probe_scale) = match self.tone.probe.as_mut() {
            Some(p) => {
                let redraw = instant || sky_new || p.sched.probe_since >= PROBE_EVERY;
                if redraw {
                    p.sched.probe_since = 0.0;
                    p.scale = st.lut_scale;
                    // (a sky taken in by this frame is in this drawing already)
                    p.sched.sky_new = false;
                } else {
                    p.sched.probe_since += dt.max(0.0);
                }
                (redraw, p.scale)
            }
            None => (false, 1.0),
        };
        // the sky cube's eye (see Probe::cube_eye): the correction for a camera that moved
        // away from it is exact for the base of the clouds, and wrong by up to the layer's
        // depth over their distance; a climb shows far sooner than driving along
        let cam_w = ro + cam_rel.as_dvec3();
        let eye_off = match self.tone.probe.as_mut() {
            Some(p) => {
                let to_clouds = (1400.0 - cam_rel.z as f64).max(120.0);
                let far = match p.cube_eye {
                    Some(e) if p.cube_filled => {
                        let m = cam_w - e;
                        // (a third of what it was: flying the free camera fast, the clouds
                        // drifted with the old cube for 400 m and then jumped back into place)
                        (m.truncate().length() * 0.1 + m.z.abs()) / to_clouds > 0.01
                    }
                    _ => true,
                };
                if far {
                    if p.cube_filled && !instant {
                        // (the cube for the new eye is filled beside the one in use, a
                        // slice a frame: redrawn whole in one frame, that frame took the
                        // six faces' 17 ms)
                        p.sched.start_staging(cam_w);
                    } else {
                        p.cube_eye = Some(cam_w);
                        p.sched.cancel_staging();
                    }
                }
                if p.sched.staging_full() {
                    // (filled: it goes over the one in use in this frame's prepass, and
                    // this frame's uniform has its eye)
                    if let Some(e) = p.sched.staging_eye.take() {
                        if omsi_cfg::flags::OMSI_DEBUG_SKY.is_set() {
                            log::info!("sky cube: new eye {:.0},{:.0},{:.0} ({:.0} m from the camera now)", e.x, e.y, e.z, (e - cam_w).length());
                        }
                        p.cube_eye = Some(e);
                        p.sched.swap = true;
                    }
                    p.sched.staged = 0;
                }
                (p.cube_eye.unwrap_or(cam_w) - cam_w).as_vec3()
            }
            None => Vec3::ZERO,
        };
        // the thunderstorm: its activity, and this frame's flash (OMSI_STORM forces both)
        let forced = forced_storm();
        let storm = forced.map_or(lighting.storm, |f| f.0).clamp(0.0, 1.0);
        let flash = match forced {
            Some((_, Some(seed))) => Lightning::frozen(seed),
            _ => self.tone.flash,
        };
        let layer = match forced {
            Some(_) => [1100.0, 1100.0 + 300.0 + 7000.0 * storm],
            None => cloud_levels(lighting, cam_rel, ro),
        };
        let u = EnhancedUniform {
            // (self-lit surfaces at their own brightness after the metering, see ExposureLog)
            exposure: [pre, 2f32.powf(-self.tone.exposure_log.as_ref().map(|l| l.ev).unwrap_or(0.0)).clamp(0.7, 1.6), pre * WINDOW_RADIANCE, 1.6],
            sun: (st.sun * cloud_t).extend(SUN_RADIUS).to_array(),
            sh,
            ground: st.ground.extend(st.lut_scale).to_array(),
            fog: [weather_fog, FOG_FALLOFF, base, clear_air],
            fog_color: fog_rgb.extend(probe_scale).to_array(),
            weather: [
                lighting.wetness.clamp(0.0, 1.0),
                lighting.snow.clamp(0.0, 1.0),
                lighting.rain.clamp(0.0, 1.0),
                input_overcast,
            ],
            lights: [PROBE_MIPS as f32, LAMP_E, CABIN_E, sun_visibility],
            sun_disc: st.sun_disc.extend(dt).to_array(),
            debug: [
                debug_view(),
                omsi_cfg::flags::OMSI_PUDDLE_F0.parse::<f32>()
                    .filter(|v| v.is_finite()).unwrap_or(0.08).clamp(0.02, 0.2),
                omsi_cfg::flags::OMSI_ENV_PHOTO.parse()
                    .unwrap_or(1.0),
                // the tone curve's contrast, which self-lit pictures undo (`display_level`)
                tone_contrast(log_exposure),
            ],
            eye: eye_off.extend(if lighting.low_clouds { CLOUD_STEPS_LOW } else { CLOUD_STEPS } as f32).to_array(),
            // x how bright an LED panel's dots burn (see `MaterialExtra::led`; the settings'
            // 16 levels give 0 = off .. 3.75), y whether the LED panels' `\S:n` masks keep
            // their mip chain (0: at full resolution, the dots stay visible when small)
            led: [lighting.led_glow, lighting.led_mips, 0.0, 0.0],
            moon: lighting.moon_dir.normalize_or_zero().extend(MOON_RADIUS).to_array(),
            // (w of the first: the veil's optical depth, which the sky draws as the high
            // layer; of the second: how far the veil spreads the sun, which softens shadows)
            cloud_sun: {
                let mut c = st.sun_at.map(|c| c.extend(0.0).to_array());
                c[0][3] = st.input.veil;
                c[1][3] = 1.0 - (-st.input.veil / st.input.sun_dir.z.max(0.03)).exp();
                c
            },
            condensation: lighting.condensation,
            layers: [layer[0], layer[1], lighting.alto, storm],
            flash: [flash.light, flash.dir[0], flash.dir[1], flash.elevation],
            flash2: {
                let (c, s) = (flash.elevation.cos(), flash.elevation.sin());
                [flash.dir[0] * c, flash.dir[1] * c, s, flash.bolt * flash.channel]
            },
            flash3: [flash.seed, 0.0, 0.0, 0.0],
            moon_light: st.moon_light.extend(if lighting.casts_moon_shadows() { 1.0 } else { 0.0 }).to_array(),
            moon_disc: st.moon_disc.extend((1.0 - 0.18 * (st.input.haze - 1.0).max(0.0)).clamp(0.2, 1.0) * 0.55).to_array(),
        };
        self.queue
            .write_buffer(&self.tone.enh_buf, 0, bytemuck::bytes_of(&u));
        self.tone.enh_last = u;
        redraw
    }
}

/// Steps of the enhanced clouds' march through their layer (sky_enhanced.wgsl
/// `cloud_layer`), and with the low cloud quality (`Lighting::low_clouds`).
pub(crate) const CLOUD_STEPS: u32 = 56;
pub(crate) const CLOUD_STEPS_LOW: u32 = 20;

/// Face size of the enhanced sky cube (see `Probe::cube_view`): about as many texels per
/// degree as a 1600-pixel-wide picture has pixels at half its size (at 512 the clouds'
/// edges stood in blocks of three or four pixels).
pub(crate) const SKY_CUBE_SIZE: u32 = 1024;
/// Redraw rounds of a sky cube face: each starts the clouds' steps elsewhere, and the
/// rounds are averaged (all of them at once for a picture on its own or a new sky).
pub(crate) const SKY_CUBE_ROUNDS: u32 = 8;
/// The old picture's share when a face is redrawn in the window.
pub(crate) const SKY_CUBE_HISTORY: f64 = 0.8;
// (a face is redrawn every fourth window frame, a slice at a time, see `SkySched`: the
// clouds drift by a pixel of the cube in seconds, and a face a frame cost 2.9 ms of every
// enhanced frame on an M4)

pub(crate) const PROBE_SIZE: u32 = 64;
pub(crate) const PROBE_MIPS: u32 = 6;
/// Glow levels of the enhanced post path.
pub(crate) const GLOW_LEVELS: usize = 6;
/// The illuminance a light's core gives (maplight colour 1, in the sky model's units:
/// 55 lux, so the street under a lamp gets its 15-25 lux).
pub(crate) const LAMP_E: f32 = 0.0055;
/// Illuminance of a bus saloon's lamps on the seats and the floor (300 lux).
pub(crate) const CABIN_E: f32 = 0.03;
/// A lit window's radiance at night.
pub(crate) const WINDOW_RADIANCE: f32 = 0.0022;
/// The metering (see `meter_tuning`).
pub(crate) const METER_GAIN: f32 = 0.4;
pub(crate) const METER_TARGET: f32 = -2.84;
// (how far the eye adapts to what it looks at beyond what the light model knows. A surplus
// (the sun in view, a snow field) is anchored by white: the eye lets it darken the picture a
// couple of stops at the gain above, and a snow field still comes out well under a stop
// darker. A shortfall has no such anchor - an underpass, a tunnel, a dark cab by day are
// seen after a few seconds almost as bright as the street (the eye's range at one adaptation
// is some five stops, but over seconds it moves across eight or more: a tunnel is a hundred
// to three hundred times darker than the day), so it makes up most of the difference over a
// range of six stops. At night the light model's own exposure is the adaptation (see
// `encode_post`: this range fades out as the pre-exposure rises).)
pub(crate) const METER_DARKEN: f32 = 2.0;
pub(crate) const METER_BRIGHTEN: f32 = 6.0;
pub(crate) const METER_SHORTFALL_GAIN: f32 = 0.85;
/// How far the metering may brighten (EV) and how much of a shortfall it makes up, for the
/// light model's pre-exposure (log2), the tuned reach and the tuned share. The eye's wide, fast
/// correction is for a dark place in daylight - a tunnel, a cab, an underpass: it fades out
/// over the dusk (by a pre-exposure of 8) and is gone by night, where the light model's own
/// exposure is the adaptation and the old gentle metering (1.6 EV, a share of 0.4) is all
/// there is - or every night in the blue hour and the twilight of a summer night would be
/// shown as bright as a white night, which only a sun a few degrees under the horizon makes.
pub(crate) fn shortfall_adaptation(log2_pre: f32, brighten: f32, gain: f32) -> (f32, f32) {
    const GENTLE: f32 = 1.6;
    let day = 1.0 - atmosphere::smoothstep(0.5, 3.0, log2_pre);
    let reach = GENTLE.min(brighten) + (brighten - GENTLE).max(0.0) * day;
    (reach * (1.0 - atmosphere::smoothstep(3.0, 7.0, log2_pre)), gain + (METER_SHORTFALL_GAIN - gain) * day)
}
/// How fast the eye follows a darker and a brighter picture (s): light adaptation of the
/// cones takes about a second (a dazzle after a tunnel's mouth passes in that time), dark
/// adaptation to a fraction of the way some three.
pub(crate) const ADAPT_TO_DARK_S: f32 = 3.0;
pub(crate) const ADAPT_TO_LIGHT_S: f32 = 0.9;
/// The tone curve's contrast about mid grey by day and at night (see `tone_contrast`).
pub(crate) const TONE_CONTRAST_DAY: f32 = 1.22;
pub(crate) const TONE_CONTRAST_NIGHT: f32 = 0.94;
/// How far night vision takes the colour out of a dark scene (post.wgsl `night_vision`).
pub(crate) const NIGHT_VISION: f32 = 0.3;
/// The sun's angular radius: the real 0.2665° (16 minutes of arc). Drawn 40 % larger, with
/// the glare round it, the sun looked far too big (#2400).
pub(crate) const SUN_RADIUS: f32 = 0.00465;
/// The moon's, as drawn (the real one is as large as the sun's; a little more, as the
/// eye sees it).
pub(crate) const MOON_RADIUS: f32 = 0.0062;

#[cfg(test)]
mod beam_tests {
    use super::headlamp_profile;
    use glam::Vec3;

    /// Up a ramp the low beam lights the road along the ramp as it lit the level road (#1897):
    /// its cut-off is the vehicle's floor, not the world's horizon.
    #[test]
    fn a_low_beam_lights_a_ramp_as_the_level_road() {
        let level = Vec3::Y;
        let p = 8f32.to_radians();
        let pitched = Vec3::new(0.0, p.cos(), p.sin());
        for (ahead, down) in [(10.0f32, 1.0f32), (30.0, 1.0), (5.0, 0.5)] {
            let flat = headlamp_profile(Vec3::new(0.0, ahead, -down).normalize(), level, true);
            // the same point on a road rising at the bus's pitch, seen from the pitched lamp
            let t = (pitched * ahead + Vec3::new(0.0, -p.sin(), p.cos()) * -down).normalize();
            let ramp = headlamp_profile(t, pitched, true);
            assert!((flat - ramp).abs() < 1e-3 * flat.max(1.0), "{ahead} m: {flat} level, {ramp} on the ramp");
            assert!(flat > 0.1);
        }
        // above the floor's horizon the low beam is cut off either way
        assert!(headlamp_profile((pitched * 30.0 + Vec3::new(0.0, -p.sin(), p.cos()) * 1.0).normalize(), pitched, true) < 0.05);
    }
}
