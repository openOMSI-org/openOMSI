//! Per-frame light sources: scenery coronas/map lights by night, vehicle lights from
//! their script variables (`[light_enh_2]`, `[spotlight]` + `Spot_Select`, `[interiorlight]`).

use crate::{LightSwitch, World};
use glam::{DVec3, Vec3};
use omsi_render::{Corona, LightMode, Lighting, PointLight, Scene};

use omsi_sim::{Daylight, VehicleInstance};

/// A headlight's strength for the enhanced renderer, its beam's at full (`headlamp` in
/// `enhanced.wgsl`): the road from the bumper out to some 12 m lit about as brightly as a
/// street under a lamp, and less beyond.
const HEADLIGHT_INTENSITY: f32 = 1800.0;

/// A `[spotlight]` declaring this range or more is a full beam (the stock buses' 500, the
/// Grand Paris-Moulon Citaro's 450), less a low beam (the stock 100, that Citaro's 200).
const FULL_BEAM_RANGE: f32 = 300.0;

/// A headlight's strength in the classic picture, as a map lamp's (`PointLight::intensity`).
/// Measured against OMSI 2 from above, the stock NL202 at night in Spandau: its pool is
/// some 18 of 255 over the unlit cobbles 1.5 - 2.5 m ahead, 12 at 4 - 5 m and nearly gone
/// at 7 m; this gives 20, 12 and 5 there. (At 1 it was 60, 40 and 15.)
const VANILLA_HEADLIGHT_INTENSITY: f32 = 0.2;

/// Lighting parameters for the renderer from the daylight model.
pub fn lighting_from(d: &Daylight, fog_range: f32) -> Lighting {
    // fog density from the weather's visibility range (an object at `range` is ~90% fogged)
    let density = (2.3 / fog_range.max(50.0)).max(0.00005);
    Lighting {
        sun_dir: d.sun_dir,
        sun_intensity: 1.0,
        sun_color: d.sun_color,
        secondary: d.secondary,
        ambient: d.ambient,
        fog_color: d.sky,
        fog_density: density,
        sky_color: d.sky,
        night: d.night,
        night_maps: Some(if d.lamps_on || d.night >= 0.5 { 1.0 } else { 0.0 }),
        sun_azimuth: d.azimuth_rad,
        sky_weights: d.sky_weights,
        envir_tint: d.envir_tint,
        moon_dir: d.moon_dir,
        moon_illum: d.moon_illum,
        day_of_year: d.day_of_year,
        latitude: d.latitude,
        day_seed: d.day_seed,
        ..Default::default()
    }
}

/// How dark the weather alone makes the day (0..0.45): a closed sky and rain bring the night
/// factor up, so the interior lights and nightmaps start to show.
pub fn gloom_darkness(overcast: f32, rain: f32) -> f32 {
    0.45 * (overcast * 0.5 + rain * 0.5).clamp(0.0, 1.0)
}

/// What the weather does to the light: an overcast sky takes the sun away and turns
/// everything grey and flat, rain darkens it further, fog and snow take their colour
/// from the sky. `cloud_density` 0..1 (1 = closed cover), `precip` 0..1, `snow` 0..1.
pub fn apply_weather(
    l: &mut Lighting,
    cloud_density: f32,
    precip_kind: i32,
    precip: f32,
    snow: f32,
) {
    let o = cloud_density.clamp(0.0, 1.0);
    let overcast = (o - 0.45).max(0.0) / 0.55; // cumulus cover keeps the sun; a closed sky loses it
    let grey = |c: Vec3, k: f32| -> Vec3 {
        let lum = c.dot(Vec3::new(0.3, 0.59, 0.11));
        c.lerp(Vec3::splat(lum), k)
    };
    // the sun: dimmed and whitened under cloud, gone under overcast
    l.sun_intensity *= 1.0 - 0.85 * overcast;
    l.sun_color = grey(l.sun_color, 0.6 * o);
    // the sky light is what is left, greyer and a little darker
    let sky_lum = l.sky_color.dot(Vec3::new(0.3, 0.59, 0.11));
    let cloud_sky = Vec3::splat(sky_lum * 0.82)
        .lerp(Vec3::new(0.62, 0.65, 0.70) * sky_lum.max(0.25) * 1.3, 0.5);
    l.sky_color = l.sky_color.lerp(cloud_sky, overcast * 0.9);
    l.secondary = grey(l.secondary, o * 0.7) * (1.0 - 0.15 * overcast);
    l.ambient = grey(l.ambient, o * 0.7) * (1.0 + 0.25 * overcast);
    l.fog_color = l.fog_color.lerp(l.sky_color, overcast);
    // rain and snow take some light and thicken the air
    // heavy rain by day is dark: the sun is gone, the sky light drops by a third and
    // everything, the bus included, sits in the same grey. The gloom also brings the
    // night factor up a little, so interior lights and nightmaps start to show - by
    // day in a downpour you do want the saloon lights on.
    let rain = if precip_kind != 0 {
        precip.clamp(0.0, 1.0)
    } else {
        0.0
    };
    l.sun_intensity *= 1.0 - 0.75 * rain;
    l.ambient *= 1.0 - 0.28 * rain;
    l.secondary *= 1.0 - 0.32 * rain;
    l.sky_color *= 1.0 - 0.22 * rain;
    l.fog_color *= 1.0 - 0.15 * rain;
    if rain > 0.0 {
        l.fog_density = l.fog_density.max(2.3 / (2500.0 - 1800.0 * rain));
    }
    // the enhanced renderer builds its own sky from these
    l.overcast = overcast;
    l.rain = rain;
    l.snowfall = if precip_kind == 2 { rain } else { 0.0 };
    l.night = l.night.max(gloom_darkness(overcast, rain));
    // (night maps on with the street lamps, or where the weather makes it that dark: a
    // clear dusk showed the lit windows and signs at a fraction, a rainy one at nearly
    // full, #276)
    if l.night >= 0.5 {
        l.night_maps = Some(1.0);
    }
    if snow > 0.0 {
        // snow on the ground throws light back up
        l.ambient *= 1.0 + 0.35 * snow;
        l.secondary *= 1.0 + 0.2 * snow;
        // (the snowy grey only as bright as the daylight: a fixed 0.86 made a winter night
        // in fog a pale grey dusk in the classic picture and in every mirror)
        let day = 1.0 - 0.93 * l.night.clamp(0.0, 1.0);
        l.fog_color = l.fog_color.lerp(Vec3::new(0.86, 0.88, 0.92) * day, 0.4 * snow);
    }
    l.snow = snow;
}

/// Coronas and point lights of one vehicle in its current state.
pub fn vehicle_lights(
    v: &VehicleInstance,
    coronas: &mut Vec<Corona>,
    lights: &mut Vec<PointLight>,
    night: f32,
    fades: &parking_lot::Mutex<CookieFades>,
    lent_beams: bool,
) {
    let ty = &v.ty;
    // def index → loaded mesh index (animation transform)
    let value_of = |name: &str| -> f32 {
        let t = name.trim();
        if let Ok(x) = t.parse::<f32>() {
            return x;
        }
        v.var(t).unwrap_or(0.0)
    };
    let mesh_xf = |def_index: usize| -> glam::Mat4 {
        match ty.meshes.iter().position(|m| m.def_index == def_index) {
            Some(i) => v.mesh_local_transform(i),
            None => v.body_rotation(),
        }
    };
    crate::model_lights_extend(&ty.model, &mesh_xf, v.position, &value_of, &v.light_fade, coronas);
    // An articulated vehicle is one visual bus, but its rear section has its own
    // `[light_enh_2]`/corona declarations and animated meshes.  The old collector only
    // visited the leading section, which made rear lamps, destination lights and section-
    // local headlights appear dead even though the coupled part was rendered correctly.
    // (a train car running its own scripts reads its own copy of the variables: its head
    // and tail lamps are its own, see `omsi_sim::vehicle::CarRun`)
    for t in &v.trailers {
        let part_mesh_xf = |def_index: usize| -> glam::Mat4 {
            match t.ty.meshes.iter().position(|m| m.def_index == def_index) {
                Some(i) => t.mesh_local_transform(i),
                None => t.body_rotation(),
            }
        };
        let car_value = |name: &str| -> f32 { t.run_var(name.trim()).unwrap_or_else(|| value_of(name)) };
        crate::model_lights_extend(&t.ty.model, &part_mesh_xf, t.position, &car_value, &t.light_fade, coronas);
    }
    let body = v.body_rotation();
    // headlights: the spotlight selected by Spot_Select
    // (OMSI_SPOT_SELECT=n: that spotlight on, for checking the headlights in a picture)
    let forced = omsi_cfg::flags::OMSI_SPOT_SELECT.var().and_then(|s| s.trim().parse::<f32>().ok());
    if let Some(sel) = forced.or_else(|| v.var("Spot_Select")) {
        if sel >= 0.0 {
            if let Some(sp) = ty.model.spotlights.get(sel as usize) {
                let vals = sp.values;
                let dl = Vec3::new(vals[3], vals[4], vals[5]).normalize_or_zero();
                let d = body.transform_vector3(dl).normalize_or_zero();
                let level = level_axis(body, dl);
                for (at, share) in spot_origins(&ty.model, ty.def.bounding_box, &vals, &v.light_fade, &value_of) {
                    push_spot(lights, v.position + body.transform_point3(at).as_dvec3(), d, level, &vals, share, night);
                }
            }
        }
    }
    // a vehicle without any spot of its own - every stock AI car: OMSI lights no road in
    // front of them, their headlamps are glows alone - is given a pair of headlamps from its
    // lit front lamps, in the Enhanced picture only (the classic one stays OMSI's)
    if lent_beams && lends_beams(&ty.model) {
        for (at, d, color, share) in headlamps_of_lamps(&ty.model, &v.light_fade, &value_of) {
            let at = v.position + body.transform_point3(at).as_dvec3();
            let level = level_axis(body, d);
            let d = body.transform_vector3(d).normalize_or_zero();
            let vals = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, color[0], color[1], color[2], 100.0, 30.0, 70.0];
            push_enhanced_spot(lights, at, d, level, &vals, share);
        }
    }
    // [spotlight_2] (openOMSI): every spot its variable switches on, the leading vehicle's
    // and its sections', where it is declared - a pair mirrored across the vehicle's axis
    // unless its flag keeps the one lamp - and as bright as a [spotlight] of its colour,
    // shared between the pair, times the variable (0..1)
    let parts = std::iter::once((&ty.model, v.position, body))
        .chain(v.trailers.iter().map(|t| (&t.ty.model, t.position, t.body_rotation())));
    for (model, origin, rot) in parts {
        for sp in &model.spotlights_2 {
            for (pos, dir, share) in spotlight_2_lamps(sp, value_of(sp.variable.as_str())) {
                let at = origin + rot.transform_point3(pos).as_dvec3();
                let d = rot.transform_vector3(dir).normalize_or_zero();
                push_spot(lights, at, d, level_axis(rot, dir), &sp.values, share, night);
            }
        }
    }
    // [spotlight_cookie] (openOMSI): lamps whose beam is a picture, the leading vehicle's
    // and its sections'
    let parts = std::iter::once((&ty.model, v.position, body))
        .chain(v.trailers.iter().map(|t| (&t.ty.model, t.position, t.body_rotation())));
    for (part, (model, origin, rot)) in parts.enumerate() {
        let dir = model.path.parent().unwrap_or(std::path::Path::new(""));
        for (k, c) in model.spotlight_cookies.iter().enumerate() {
            let key = (v as *const VehicleInstance as usize, part, k);
            let on = cookie_fade(&mut fades.lock(), key, value_of(c.variable.as_str()).clamp(0.0, 1.0), c.fade);
            if on <= 0.0 {
                continue;
            }
            let turn = |name: &Option<String>| name.as_deref().map_or(0.0, |n| value_of(n));
            let local = cookie_direction(Vec3::from(c.dir), turn(&c.yaw), turn(&c.pitch));
            let at = origin + rot.transform_point3(Vec3::from(c.pos)).as_dvec3();
            let d = rot.transform_vector3(local).normalize_or_zero();
            push_cookie(lights, at, d, c, on, cookie_picture_id(dir, &c.picture), night);
        }
    }
    // [interiorlight]s light only the meshes listing them and the passengers (per-instance
    // term, see MeshProps::interior); they do not shine on the outside world.
    let _ = &ty.model.interior_lights;
}

fn spot_origins(model: &omsi_model::Model, bounding_box: Option<[f32; 6]>, vals: &[f32; 12], fades: &[f32], value_of: &dyn Fn(&str) -> f32) -> Vec<(Vec3, f32)> {
    let dl = Vec3::new(vals[3], vals[4], vals[5]).normalize_or_zero();
    if dl.y > 0.3 {
        let lamps = spot_headlamps(model, fades, value_of);
        if !lamps.is_empty() {
            return lamps;
        }
    }
    // D3D's spot is only a direction: the stock NL202 puts it 3.8 m behind its
    // nose, where the real one lit the dashboard and the windscreen from inside.
    // It shines from the vehicle's front (or rear) face at its own height. The face
    // is where the model's lamps ([light_enh], [light_enh_2]) reach furthest: the
    // AA-FR Agora's spot sits 4 m behind its headlamps, and moved at most 1.5 m it
    // still shone from inside the bus onto its own front. (Within the bounding box;
    // without lamps, the box alone, by at most 1.5 m: a box grown by mirrors, a
    // coupling or a mod's odd bounds threw the light well ahead of the bus.)
    let mut apex = Vec3::new(vals[0], vals[1], vals[2]);
    let lamps: Vec<[f32; 3]> = model
        .meshes
        .iter()
        .flat_map(|m| m.light_enh.iter().map(|l| l.pos).chain(m.light_enh_2.iter().map(|l| l.pos)))
        .collect();
    let nose = lamps.iter().map(|l| l[1]).reduce(f32::max);
    let tail = lamps.iter().map(|l| l[1]).reduce(f32::min);
    let bb = bounding_box.map(|bb| (bb[4] + bb[1] * 0.5, bb[4] - bb[1] * 0.5));
    if dl.y > 0.3 {
        let face = match (nose.filter(|n| *n > apex.y), bb) {
            (Some(n), Some((front, _))) => Some(n.min(front)),
            (Some(n), None) => Some(n),
            (None, Some((front, _))) => Some(front.min(apex.y + 1.5)),
            (None, None) => None,
        };
        if let Some(face) = face.filter(|f| *f > apex.y) {
            apex.y = face + 0.05;
        } else if let Some(n) = nose.filter(|n| *n + 0.05 < apex.y) {
            // (a spot ahead of the lamps, the Grand Paris-Moulon Citaro's 55 cm: back onto them)
            apex.y = n + 0.05;
        }
    } else if dl.y < -0.3 {
        let face = match (tail.filter(|t| *t < apex.y), bb) {
            (Some(t), Some((_, rear))) => Some(t.max(rear)),
            (Some(t), None) => Some(t),
            (None, Some((_, rear))) => Some(rear.max(apex.y - 1.5)),
            (None, None) => None,
        };
        if let Some(face) = face.filter(|f| *f < apex.y) {
            apex.y = face - 0.05;
        } else if let Some(t) = tail.filter(|t| *t - 0.05 > apex.y) {
            apex.y = t - 0.05;
        }
    }
    // One spot on the vehicle's axis threw one narrow beam: it is split into one
    // per headlamp on that face (within 35 cm of it), as far apart as the mean of
    // their distances from the axis (the outermost are indicators and position
    // lamps). The Agora's headlamps are 1.8 to 2.4 m apart.
    let half_width = bounding_box.map_or(1.25, |bb| (bb[0] * 0.5).min(1.25));
    let on_face: Vec<f32> = lamps
        .iter()
        .filter(|l| (dl.y > 0.3 || dl.y < -0.3) && (l[1] - apex.y).abs() < 0.35)
        .map(|l| (l[0] - apex.x).abs())
        .collect();
    let spread = (on_face.iter().sum::<f32>() / on_face.len().max(1) as f32).min(half_width);
    let sides: &[f32] = if spread > 0.1 { &[-1.0, 1.0] } else { &[0.0] };
    sides.iter().map(|side| (apex + Vec3::X * spread * side, 1.0 / sides.len() as f32)).collect()
}

/// A `[spotlight_cookie]`'s direction in its vehicle's frame (x right, y ahead, z up),
/// turned by its yaw variable (degrees, to the right about the vehicle's up axis) and then
/// its pitch variable (degrees, up about the lamp's own horizontal axis): a cornering lamp,
/// a levelling one.
fn cookie_direction(dir: Vec3, yaw_deg: f32, pitch_deg: f32) -> Vec3 {
    let d = dir.normalize_or_zero();
    if d == Vec3::ZERO {
        return Vec3::Y;
    }
    let d = glam::Quat::from_rotation_z(-yaw_deg.to_radians()) * d;
    let right = d.cross(Vec3::Z);
    if right.length_squared() < 1e-8 {
        return d;
    }
    glam::Quat::from_axis_angle(right.normalize(), pitch_deg.to_radians()) * d
}

/// The lights of a `[spotlight_cookie]` at `at` shining along `d`, switched to `on` (0..1),
/// its picture `picture` (0: not found - its cone lights it): the classic picture's spot, as
/// `push_spot` makes it, and the enhanced picture's, as bright as one headlamp's beam at
/// its strongest where the picture is white, lit by the picture within its cone.
fn push_cookie(lights: &mut Vec<PointLight>, at: DVec3, d: Vec3, c: &omsi_model::SpotlightCookie, on: f32, picture: u16, night: f32) {
    let color = [c.color[0] / 255.0, c.color[1] / 255.0, c.color[2] / 255.0];
    let half = |deg: f32| (deg.clamp(1.0, 170.0) * 0.5).to_radians();
    let tan = [half(c.width).tan(), half(c.height).tan()];
    let cookie = omsi_render::Cookie { picture, tan };
    // (the cone round the picture, and inside it the smaller half angle: what the classic
    // picture and the light in the fog are lit by)
    let outer = cookie.bounding_cos();
    let inner = half(c.width.min(c.height)).cos().max(outer);
    lights.push(PointLight {
        position: at,
        radius: spot_reach(c.range, 45.0),
        color,
        intensity: VANILLA_HEADLIGHT_INTENSITY * on * (0.3 + 0.7 * night),
        direction: d,
        cone: [inner, outer],
        mode: LightMode::Vanilla,
        ..Default::default()
    });
    let reach = spot_reach(c.range, 60.0);
    let weaker = (reach / 60.0).min(1.0).powi(2);
    lights.push(PointLight {
        position: at,
        radius: reach,
        color,
        // (one lamp of a pair: `HEADLIGHT_INTENSITY` is the two headlamps' together)
        intensity: HEADLIGHT_INTENSITY * 0.5 * on * weaker,
        direction: d,
        cone: [inner, outer],
        core: 1.0,
        mode: LightMode::Enhanced,
        cookie: (picture != 0).then_some(cookie),
        ..Default::default()
    });
}

/// The `[spotlight_cookie]` lamps' brightness and when it was last worked out, by vehicle,
/// part and lamp (kept by the world the vehicles drive in: `World::cookie_fades`).
pub(crate) type CookieFades = std::collections::HashMap<(usize, usize, usize), (f32, std::time::Instant)>;

/// How bright a `[spotlight_cookie]` is now, coming to `target` with its time constant
/// `fade` (s; 0 at once): kept per vehicle and lamp from frame to frame. A lamp seen for the
/// first time is at its target (a picture taken offscreen shows it lit).
fn cookie_fade(fades: &mut CookieFades, key: (usize, usize, usize), target: f32, fade: f32) -> f32 {
    if fade <= 0.0 {
        return target;
    }
    let now = std::time::Instant::now();
    // (lamps of vehicles gone for a while are forgotten)
    if fades.len() > 256 {
        fades.retain(|_, (_, t)| now.duration_since(*t).as_secs_f32() < 5.0);
    }
    let (value, last) = fades.entry(key).or_insert((target, now));
    let dt = now.duration_since(*last).as_secs_f32().min(0.25);
    *value = target + (*value - target) * (-dt / fade).exp();
    *last = now;
    *value
}

/// The picture id of a `[spotlight_cookie]`'s beam, looked for as the model's own textures
/// are (see `corona_texture_id`); 0 when there is no such file. A picture found the first
/// time is read and handed to the renderer with the next `upload_corona_textures`.
fn cookie_picture_id(model_dir: &std::path::Path, name: &str) -> u16 {
    let id = corona_texture_id(model_dir, name);
    if id != 0 {
        let mut g = COOKIE_PICTURES.lock().unwrap_or_else(|e| e.into_inner());
        let (known, listed) = g.get_or_insert_with(Default::default);
        if known.insert(id) {
            let path = CORONA_TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|t| t.ids.iter().find(|(_, i)| **i == id).map(|(p, _)| p.clone()));
            if let Some(p) = path {
                listed.push((id, p));
            }
        }
    }
    id
}

/// The beam pictures known, and in the order they were first asked for (each renderer is
/// handed them from where it stands, see `upload_corona_textures`). Process-wide like the
/// numbers in `CORONA_TEXTURES`: a picture file has one number for every renderer.
type CookiePictures = (std::collections::HashSet<u16>, Vec<(u16, std::path::PathBuf)>);
static COOKIE_PICTURES: std::sync::Mutex<Option<CookiePictures>> = std::sync::Mutex::new(None);

/// A `[spotlight_2]`'s lamps in its vehicle's frame - position, direction and share of a
/// spot's light - with its variable at `k`: none when that is off, the lamp and its twin
/// mirrored across the axis unless the flag keeps the one.
fn spotlight_2_lamps(sp: &omsi_model::Spotlight2, k: f32) -> Vec<(Vec3, Vec3, f32)> {
    let k = k.clamp(0.0, 1.0);
    if k <= 0.0 {
        return Vec::new();
    }
    let v = sp.values;
    let sides: &[f32] = if sp.mirrored { &[1.0, -1.0] } else { &[1.0] };
    sides
        .iter()
        .map(|s| (Vec3::new(v[0] * s, v[1], v[2]), Vec3::new(v[3] * s, v[4], v[5]), k / sides.len() as f32))
        .collect()
}

/// A lamp's axis (`local`, in its vehicle's frame) laid into the vehicle's floor, in the
/// world (`rot`: the vehicle's rotation): the way along the road it shines, whose rise is
/// the vehicle's pitch - the horizon of its beam (see `push_enhanced_spot`). Zero for a lamp
/// pointing straight up or down.
fn level_axis(rot: glam::Mat4, local: Vec3) -> Vec3 {
    rot.transform_vector3(Vec3::new(local.x, local.y, 0.0)).normalize_or_zero()
}

/// The lights of one headlamp at `at` shining along `d`, with a `[spotlight]`'s numbers
/// (`vals`: colour 6-8, range 9, inner and outer cone 10 and 11) and `share` of its light;
/// `level` its axis in the vehicle's floor (see [`level_axis`]).
fn push_spot(lights: &mut Vec<PointLight>, at: DVec3, d: Vec3, level: Vec3, vals: &[f32; 12], share: f32, night: f32) {
    let color = [vals[6] / 255.0, vals[7] / 255.0, vals[8] / 255.0];
    // inner and outer cone as full angles (values 10 and 11)
    let (inner, outer) = (vals[10], vals[11]);
    let half = |deg: f32| (deg.clamp(1.0, 179.0) * 0.5).to_radians().cos();
    let cone = [half(inner.min(outer)), half(outer)];
    // vanilla: the spot, lit as the classic picture lights a lamp, only inside
    // its cone (three point lights along its axis stood in for it before: they
    // shone every way, on the bus's own body and saloon)
    lights.push(PointLight {
        position: at,
        radius: spot_reach(vals[9], 45.0),
        color,
        intensity: VANILLA_HEADLIGHT_INTENSITY * share * (0.3 + 0.7 * night),
        direction: d,
        cone,
        mode: LightMode::Vanilla,
        ..Default::default()
    });
    push_enhanced_spot(lights, at, d, level, vals, share);
}

/// At most this many cars, the nearest to the camera within `AI_BEAM_RANGE`, are lent
/// headlamps (`headlamps_of_lamps`): every stock AI car with them was two or three more
/// lights each at night, a hundred and more on a busy street, each looked at by every pixel
/// near it - for cars whose beams far off light a few metres of road nobody sees.
const AI_BEAMS: usize = 12;
const AI_BEAM_RANGE: f64 = 200.0;
/// How dark the scene has to be (0 day, 1 night; the sun's height and the weather's gloom, the
/// larger) for the cars to be lent any beams, and how dark for all of `AI_BEAMS` of them within
/// `AI_BEAM_RANGE`. In between, in a grey or rainy day, only the nearest few close by.
const AI_BEAM_MIN_DARK: f32 = 0.12;
const AI_BEAM_FULL_DARK: f32 = 0.5;
const AI_BEAMS_DIM: usize = 3;
const AI_BEAM_RANGE_DIM: f64 = 60.0;

/// Whether a vehicle has no spot of its own and is lent its headlamps' beams.
fn lends_beams(model: &omsi_model::Model) -> bool {
    model.spotlights.is_empty() && model.spotlights_2.is_empty() && model.spotlight_cookies.is_empty()
}

/// Which of `vehicles` (by index) may be lent their headlamps' beams (lit ones alone get
/// them): the nearest without spots of their own - `AI_BEAMS` within `AI_BEAM_RANGE` of the
/// camera in the dark, `AI_BEAMS_DIM` within `AI_BEAM_RANGE_DIM` in a gloomy day (`dark` from
/// `AI_BEAM_MIN_DARK`), none by day.
fn cars_with_beams(vehicles: &[&VehicleInstance], camera: DVec3, dark: f32) -> Vec<usize> {
    let (count, range) = if dark < AI_BEAM_MIN_DARK {
        return Vec::new();
    } else if dark < AI_BEAM_FULL_DARK {
        (AI_BEAMS_DIM, AI_BEAM_RANGE_DIM)
    } else {
        (AI_BEAMS, AI_BEAM_RANGE)
    };
    let mut near: Vec<(f64, usize)> = vehicles
        .iter()
        .enumerate()
        .filter(|(_, v)| lends_beams(&v.ty.model))
        .map(|(k, v)| ((v.position - camera).length(), k))
        .filter(|(d, _)| *d < range)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter().take(count).map(|(_, k)| k).collect()
}

/// The headlamps a model without a `[spotlight]` has (its position, direction, colour 0..255
/// and share of a pair's light, in the model's frame): its lit `[light_enh_2]` lamps that
/// face forward from its front, white or nearly (no indicator, no rear or roof lamp), one
/// beam per side from the middle of that side's lamps.
fn headlamps_of_lamps(model: &omsi_model::Model, fades: &[f32], value_of: &dyn Fn(&str) -> f32) -> Vec<(Vec3, Vec3, [f32; 3], f32)> {
    let mut found: Vec<FrontLamp> = white_front_lamps(model, fades, value_of).into_iter().filter(|f| f.3 >= 0.5).collect();
    let Some(front) = found.iter().map(|f| f.0.y).reduce(f32::max) else { return Vec::new() };
    found.retain(|f| f.0.y >= front - 0.4);
    let sides = by_side(&found);
    let n = sides.len() as f32;
    sides
        .into_iter()
        .map(|s| {
            let k = s.len() as f32;
            let dir = s.iter().map(|f| f.1).sum::<Vec3>().normalize_or(Vec3::Y);
            // (a lamp's glow sits on its glass: the light leaves just in front of it)
            let pos = s.iter().map(|f| f.0).sum::<Vec3>() / k + dir * 0.1;
            let color = [0, 1, 2].map(|c| s.iter().map(|f| f.2[c]).sum::<f32>() / k);
            (pos, dir, color, 1.0 / n)
        })
        .collect()
}

type FrontLamp = (Vec3, Vec3, [f32; 3], f32, f32);

fn white_front_lamps(model: &omsi_model::Model, fades: &[f32], value_of: &dyn Fn(&str) -> f32) -> Vec<FrontLamp> {
    let mut found = Vec::new();
    let mut li = 0usize;
    for md in &model.meshes {
        for l in &md.light_enh_2 {
            let k = li;
            li += 1;
            let lit = match fades.get(k) {
                Some(f) => *f,
                None => value_of(&l.variable) * if l.factor > 0.0 { l.factor } else { 1.0 },
            };
            let dir = Vec3::from(l.dir).normalize_or_zero();
            let (hi, lo) = (l.color.iter().copied().fold(0.0, f32::max), l.color.iter().copied().fold(f32::MAX, f32::min));
            let pos = Vec3::from(l.pos);
            if l.omni || dir.y < 0.8 || hi < 150.0 || lo < 0.6 * hi || !(0.2..1.6).contains(&pos.z) {
                continue;
            }
            let lit = if lit.is_nan() { 0.0 } else { lit };
            found.push((pos, dir, l.color, lit, lit / if l.factor > 0.0 { l.factor } else { 1.0 }));
        }
    }
    found
}

fn by_side(lamps: &[FrontLamp]) -> Vec<Vec<&FrontLamp>> {
    [lamps.iter().filter(|f| f.0.x < -0.2).collect(), lamps.iter().filter(|f| f.0.x > 0.2).collect(), lamps.iter().filter(|f| f.0.x.abs() <= 0.2).collect()]
        .into_iter()
        .filter(|s: &Vec<_>| !s.is_empty())
        .collect()
}

fn spot_headlamps(model: &omsi_model::Model, fades: &[f32], value_of: &dyn Fn(&str) -> f32) -> Vec<(Vec3, f32)> {
    let mut found = white_front_lamps(model, fades, value_of);
    let Some(front) = found.iter().map(|f| f.0.y).reduce(f32::max) else { return Vec::new() };
    let nose = model.meshes.iter().flat_map(|m| m.light_enh.iter().map(|l| l.pos[1]).chain(m.light_enh_2.iter().map(|l| l.pos[1]))).fold(front, f32::max);
    if front < nose - 0.5 {
        return Vec::new();
    }
    found.retain(|f| f.0.y >= front - 0.4);
    let on = |f: &FrontLamp| f.4 >= 0.5;
    let lit_top = |lamps: &[&FrontLamp]| lamps.iter().filter(|f| on(f)).map(|f| f.0.z).reduce(f32::max);
    let mut sides = by_side(&found);
    let any_lit = sides.iter().filter_map(|s| lit_top(s)).reduce(f32::max);
    let paired = sides.iter().any(|s| s[0].0.x.abs() > 0.2);
    sides.retain(|s| !(s[0].0.x.abs() <= 0.2 && lit_top(s).is_none() && paired));
    let n = sides.len() as f32;
    sides
        .into_iter()
        .map(|side| {
            let lit: Vec<&FrontLamp> = side.iter().copied().filter(|f| on(f)).collect();
            let set = if !lit.is_empty() {
                lit
            } else if any_lit.is_some() {
                side
            } else {
                let most = side.iter().map(|f| f.4).fold(f32::MIN, f32::max);
                side.into_iter().filter(|f| f.4 >= most - 1e-3).collect()
            };
            let target = lit_top(&set).or(any_lit).unwrap_or_else(|| set.iter().map(|f| f.0.z).fold(f32::MIN, f32::max));
            let best = set.iter().map(|f| f.0.z).min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs())).unwrap_or(target);
            let group: Vec<&FrontLamp> = set.into_iter().filter(|f| (f.0.z - best).abs() <= 0.1).collect();
            let dir = group.iter().map(|f| f.1).sum::<Vec3>().normalize_or(Vec3::Y);
            (group.iter().map(|f| f.0).sum::<Vec3>() / group.len() as f32 + dir * 0.1, 1.0 / n)
        })
        .collect()
}

/// A headlamp's light in the Enhanced picture alone (see `push_spot`). A lamp aimed along the
/// road is lit by a road lamp's profile about its horizon, which is the vehicle's floor: its
/// direction is `level`, the axis laid into that floor, whose rise is the vehicle's pitch.
/// (Taken about the world's horizon, the beam stayed level while the bus climbed a ramp and
/// lit less and less of the road rising in front of it, as Omsi.exe's spot - fixed to the
/// bus - does not, #1897.)
fn push_enhanced_spot(lights: &mut Vec<PointLight>, at: DVec3, d: Vec3, level: Vec3, vals: &[f32; 12], share: f32) {
    let color = [vals[6] / 255.0, vals[7] / 255.0, vals[8] / 255.0];
    let (inner, outer) = (vals[10], vals[11]);
    let half = |deg: f32| (deg.clamp(1.0, 179.0) * 0.5).to_radians().cos();
    let cone = [half(inner.min(outer)), half(outer)];
    // enhanced: falling off with the square of the distance from a one-metre core. A lamp
    // declared to reach less far than a low beam is a weaker one: the light that still
    // reaches the low beam's last lit metres at its own range is the square of the ranges
    // smaller (a work light of range 1 shone as bright as a headlamp, #1541)
    let reach = spot_reach(vals[9], 60.0);
    let weaker = (reach / 60.0).min(1.0).powi(2);
    // (a lamp pointing steeply down - a `[spotlight_2]` over a door - keeps its cone: the
    // road lamp's profile is for one aimed along the road)
    let beam = if d.normalize_or_zero().z.abs() >= 0.5 || level == Vec3::ZERO { 0.0 } else if vals[9] >= FULL_BEAM_RANGE { -1.0 } else { 1.0 };
    lights.push(PointLight {
        position: at,
        radius: reach,
        color,
        intensity: HEADLIGHT_INTENSITY * share * weaker,
        direction: if beam != 0.0 { level } else { d },
        cone,
        core: 1.0,
        beam,
        housed: false,
        shadow_owner: None,
        mode: LightMode::Enhanced,
        cookie: None,
        flash: false,
    });
}

/// How far a `[spotlight]` reaches in the picture, from its declared range (value 9):
/// up to `low` metres as declared, as before, and a range past the stock low beam's 100 in
/// proportion to it (`low` metres for 100), at most five times `low`. The stock buses' full beam declares 500 against the low beam's 100; both
/// cut at the same 45 m (60 in Enhanced), the full beam lit no further than the low beam
/// (#941). (The light's core grows with it - a fixed share of the reach - so the full beam
/// is also brighter ahead, as one is.)
/// (A short range is taken as declared, down to half a metre: from 10 m on, a lamp of range
/// 1 lit everything round it like a headlamp, #1541.)
fn spot_reach(range: f32, low: f32) -> f32 {
    range.clamp(0.5, low).max(range * low / 100.0).min(low * 5.0)
}

#[cfg(test)]
mod cookie_tests {
    use super::*;

    /// A cornering lamp's yaw turns it to the right, a levelling lamp's pitch up.
    #[test]
    fn a_cookie_turns_by_its_yaw_and_pitch() {
        let ahead = Vec3::Y;
        let right = cookie_direction(ahead, 30.0, 0.0);
        assert!(right.x > 0.49 && right.x < 0.51 && right.y > 0.86, "{right:?}");
        let up = cookie_direction(ahead, 0.0, 10.0);
        assert!(up.z > 0.17 && up.z < 0.18 && up.x.abs() < 1e-5, "{up:?}");
        assert_eq!(cookie_direction(ahead, 0.0, 0.0), ahead);
    }

    /// A cookie lamp is a classic spot and an enhanced one with its picture; without a
    /// picture found the enhanced one keeps the plain cone.
    #[test]
    fn a_cookie_lamp_falls_back_to_its_cone() {
        let c = omsi_model::SpotlightCookie { dir: [0.0, 1.0, 0.0], color: [255.0; 3], range: 80.0, width: 60.0, height: 20.0, ..Default::default() };
        let mut lights = Vec::new();
        push_cookie(&mut lights, DVec3::ZERO, Vec3::Y, &c, 1.0, 7, 1.0);
        let e = lights.iter().find(|l| l.mode == LightMode::Enhanced).unwrap();
        assert_eq!(e.cookie.map(|c| c.picture), Some(7));
        assert_eq!(e.beam, 0.0);
        assert!(e.cone[1] < 30f32.to_radians().cos(), "the cone holds the picture's corners");
        lights.clear();
        push_cookie(&mut lights, DVec3::ZERO, Vec3::Y, &c, 1.0, 0, 1.0);
        assert!(lights.iter().all(|l| l.cookie.is_none()));
        assert_eq!(lights.len(), 2);
    }
}

#[cfg(test)]
mod bitmap_tests {
    /// A lamp's effect bitmap named `.bmp` but shipped only as a `.dds` of the same name is
    /// found, as a material's texture is (the Helferlein signals' `Eff_li.bmp`); with no
    /// picture of that name at all the standard glow stays.
    #[test]
    fn a_light_bitmap_shipped_as_dds_is_found() {
        let dir = std::env::temp_dir().join("openomsi-light-bitmap-dds-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Signals/texture")).unwrap();
        std::fs::write(dir.join("Signals/texture/Glimmer_lamp.dds"), b"DDS ").unwrap();
        let id = super::corona_texture_id(&dir.join("Signals"), "Glimmer_lamp.bmp");
        assert_ne!(id, 0, "the .dds stands in for the .bmp");
        assert_eq!(id, super::texture_id_of(dir.join("Signals/texture/Glimmer_lamp.dds")));
        assert_eq!(super::corona_texture_id(&dir.join("Signals"), "Nowhere_lamp.bmp"), 0);
    }
}

#[cfg(test)]
mod spot_tests {
    #[test]
    fn a_full_beam_reaches_further_than_the_low_beam() {
        // the stock buses: low beam 100, full beam 500 (#941)
        assert_eq!(super::spot_reach(100.0, 45.0), 45.0);
        assert_eq!(super::spot_reach(500.0, 45.0), 225.0);
        assert_eq!(super::spot_reach(500.0, 60.0), 300.0);
        // short ranges as declared, as before
        assert_eq!(super::spot_reach(30.0, 45.0), 30.0);
        assert_eq!(super::spot_reach(2.0, 45.0), 2.0);
        assert_eq!(super::spot_reach(0.1, 45.0), 0.5);
        assert_eq!(super::spot_reach(5000.0, 45.0), 225.0);
    }

    /// The enhanced picture lights a `[spotlight]` as a low beam below the full beam's range
    /// (the stock 100, the Grand Paris-Moulon Citaro's 200) and as a full beam from it on.
    #[test]
    fn a_spotlight_is_a_low_or_a_full_beam_by_its_range() {
        let beam = |range: f32| {
            let vals = [0.0, 6.5, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, range, 30.0, 80.0];
            let mut lights = Vec::new();
            super::push_spot(&mut lights, glam::DVec3::ZERO, glam::Vec3::Y, glam::Vec3::Y, &vals, 0.5, 1.0);
            lights.iter().find(|l| l.mode == omsi_render::LightMode::Enhanced).map(|l| l.beam)
        };
        assert_eq!([100.0, 200.0, 450.0, 500.0].map(beam), [Some(1.0), Some(1.0), Some(-1.0), Some(-1.0)]);
        // a lamp over a door, pointing down, keeps its cone
        let vals = [0.0, 6.5, 2.5, 0.0, 0.0, -1.0, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
        let mut lights = Vec::new();
        super::push_spot(&mut lights, glam::DVec3::ZERO, glam::Vec3::NEG_Z, glam::Vec3::ZERO, &vals, 0.5, 1.0);
        assert_eq!(lights.iter().find(|l| l.mode == omsi_render::LightMode::Enhanced).map(|l| l.beam), Some(0.0));
    }

    /// A headlamp's beam lies about the vehicle's floor: on a bus pitched up a ramp its
    /// Enhanced light is aimed along the ramp, whatever the lamp's own aim down (#1897).
    #[test]
    fn a_headlamp_beam_pitches_with_its_vehicle() {
        let vals = [0.0, 6.5, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
        let pitch = 8f32.to_radians();
        let body = glam::Mat4::from_rotation_x(pitch);
        let dl = glam::Vec3::new(0.0, 1.0, -0.3).normalize();
        let d = body.transform_vector3(dl);
        let mut lights = Vec::new();
        super::push_spot(&mut lights, glam::DVec3::ZERO, d, super::level_axis(body, dl), &vals, 0.5, 1.0);
        let enh = lights.iter().find(|l| l.mode == omsi_render::LightMode::Enhanced).unwrap();
        assert_eq!(enh.beam, 1.0);
        assert!((enh.direction.z - pitch.sin()).abs() < 1e-4 && (enh.direction.y - pitch.cos()).abs() < 1e-4, "{:?}", enh.direction);
        // (the classic spot keeps the lamp's own aim)
        let van = lights.iter().find(|l| l.mode == omsi_render::LightMode::Vanilla).unwrap();
        assert!((van.direction - d).length() < 1e-5);
    }

    /// `[spotlight_2]`: a pair mirrored across the axis sharing the light, or one lamp, as
    /// bright as its variable says, and nothing while that is off.
    #[test]
    fn a_spotlight_2_is_a_mirrored_pair_or_one_lamp() {
        use glam::Vec3;
        let mut sp = omsi_model::Spotlight2 {
            values: [0.9, 5.9, 0.65, 0.2, 1.0, -0.3, 255.0, 255.0, 233.0, 200.0, 30.0, 80.0],
            variable: "lights_fern".into(),
            mirrored: true,
        };
        let pair = super::spotlight_2_lamps(&sp, 1.0);
        assert_eq!(
            pair,
            [
                (Vec3::new(0.9, 5.9, 0.65), Vec3::new(0.2, 1.0, -0.3), 0.5),
                (Vec3::new(-0.9, 5.9, 0.65), Vec3::new(-0.2, 1.0, -0.3), 0.5),
            ]
        );
        assert!(super::spotlight_2_lamps(&sp, 0.0).is_empty());
        assert!(super::spotlight_2_lamps(&sp, -1.0).is_empty());
        sp.mirrored = false;
        assert_eq!(super::spotlight_2_lamps(&sp, 0.5), [(Vec3::new(0.9, 5.9, 0.65), Vec3::new(0.2, 1.0, -0.3), 0.5)]);
        assert_eq!(super::spotlight_2_lamps(&sp, 3.0)[0].2, 1.0);
    }
    #[test]
    fn a_spotlight_shines_from_its_headlamps() {
        use glam::Vec3;
        let lamp = |x: f32, y: f32, z: f32, variable: &str| omsi_model::LightEnh2 {
            pos: [x, y, z],
            dir: [0.0, 1.0, 0.0],
            color: [255.0, 255.0, 240.0],
            variable: variable.into(),
            ..Default::default()
        };
        let mesh = |light_enh_2: Vec<omsi_model::LightEnh2>| omsi_model::Model { meshes: vec![omsi_model::MeshDef { light_enh_2, ..Default::default() }], ..Default::default() };
        let model = mesh(vec![
            lamp(-0.8, 5.8, 1.05, "abbl_l"),
            lamp(0.8, 5.8, 1.05, "abbl_r"),
            lamp(-0.8, 5.8, 1.2, "fern"),
            lamp(0.8, 5.8, 1.2, "fern"),
            lamp(-0.7, 5.8, 0.6, "nebel"),
            lamp(0.7, 5.8, 0.6, "nebel"),
            lamp(0.0, 5.8, 1.0, "logo"),
            lamp(-1.1, -5.8, 1.4, "stand"),
            lamp(1.1, -5.8, 1.4, "stand"),
            omsi_model::LightEnh2 { color: [255.0, 160.0, 0.0], ..lamp(-1.1, 5.8, 1.05, "abbl_l") },
        ]);
        let bb = Some([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        let low = [0.0, 2.0, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
        let at = |model: &omsi_model::Model, on: &[&str], fades: &[f32], vals: &[f32; 12]| {
            let on: Vec<String> = on.iter().map(|s| s.to_string()).collect();
            let mut at = super::spot_origins(model, bb, vals, fades, &|name: &str| if on.iter().any(|o| o == name) { 1.0 } else { 0.0 });
            at.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
            at
        };
        let near = |got: &[(Vec3, f32)], want: &[(Vec3, f32)]| got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (g.0 - w.0).length() < 1e-4 && g.1 == w.1);
        let pair = |z: f32| [(Vec3::new(-0.8, 5.9, z), 0.5), (Vec3::new(0.8, 5.9, z), 0.5)];
        let got = at(&model, &["abbl_l", "abbl_r", "stand"], &[], &low);
        assert!(near(&got, &pair(1.05)), "low beam, rear markers lit: {got:?}");
        let got = at(&model, &["stand"], &[], &low);
        assert!(near(&got, &pair(1.2)), "rear markers alone: {got:?}");
        let got = at(&model, &["abbl_l", "abbl_r", "nebel"], &[], &low);
        assert!(near(&got, &pair(1.05)), "fog lamps on: {got:?}");
        let got = at(&model, &["abbl_l", "abbl_r", "fern"], &[], &low);
        assert!(near(&got, &pair(1.2)), "full beam: {got:?}");
        let got = at(&model, &["abbl_l"], &[], &low);
        assert!(near(&got, &pair(1.05)), "one bulb out: {got:?}");
        let got = at(&model, &[], &[0.4, 0.4, 0.0, 0.0, 0.0, 0.0, 0.0], &low);
        assert!(near(&got, &pair(1.05)), "low beam fading in: {got:?}");
        let got = at(&model, &["abbl_l", "abbl_r"], &[0.0, 0.0, 1.0, 1.0], &low);
        assert!(near(&got, &pair(1.2)), "the fades rule over the variables: {got:?}");
        let dim = mesh(vec![omsi_model::LightEnh2 { factor: 0.3, ..lamp(-0.8, 5.8, 1.05, "abbl") }, omsi_model::LightEnh2 { factor: 0.3, ..lamp(0.8, 5.8, 1.05, "abbl") }, lamp(-0.7, 5.8, 0.6, "drl"), lamp(0.7, 5.8, 0.6, "drl")]);
        let got = at(&dim, &["abbl", "drl"], &[], &low);
        assert!(near(&got, &pair(1.05)), "a faint headlamp glow over a DRL: {got:?}");
        let rear = [0.0, -5.0, 0.76, 0.0, -1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
        let got = at(&model, &["abbl_l", "abbl_r"], &[], &rear);
        assert!(got.iter().all(|(p, _)| p.z == 0.76 && (p.y + 5.85).abs() < 1e-4), "a rear spot keeps its own height: {got:?}");
        let inside = mesh(vec![lamp(-0.5, 2.0, 1.0, "x"), lamp(0.5, 2.0, 1.0, "x"), omsi_model::LightEnh2 { color: [255.0, 190.0, 0.0], ..lamp(0.9, 5.8, 0.9, "x") }]);
        let got = at(&inside, &["x"], &[], &low);
        assert!(got.iter().all(|(p, _)| p.z == 0.76 && (p.y - 5.85).abs() < 1e-4), "white lamps far behind the nose: {got:?}");
        let got = super::spot_origins(&omsi_model::Model::default(), bb, &low, &[], &|_| 0.0);
        assert!(near(&got, &[(Vec3::new(0.0, 3.55, 0.76), 1.0)]), "no lamps: {got:?}");
    }
}

/// Map lights and scenery coronas are only drawn this close to the camera.
const MAP_LIGHT_RANGE: f64 = 600.0;
const CORONA_RANGE: f64 = 1500.0;
/// A vehicle out of the view (`VehicleInstance::ai_visuals`) shines only this close to the camera.
const UNSEEN_LIGHT_RANGE: f64 = 120.0;
/// How far the camera may move before the nearby static lights are gathered again.
const NEAR_MARGIN: f64 = 100.0;

/// The map's lights and coronas near the camera, switched for the lamp state. Going
/// through every corona of a city map (traffic lights, street lamps, signs) each frame
/// cost a millisecond; the near ones only change when the camera has moved on, the lamps
/// switch or tiles have loaded or gone (the streamer rebuilds the world's lists then).
/// Kept by the world they come from (`World::near_lights`).
#[derive(Default)]
pub(crate) struct NearLights {
    generation: u64,
    lamps_on: bool,
    centre: DVec3,
    counts: (usize, usize),
    lights: Vec<PointLight>,
    coronas: Vec<Corona>,
}

/// Which vehicles may draw their script-authored smoke particles. Scenery smoke and
/// emissive vehicle effects (sparks, flares) are independent of this option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleSmoke {
    All,
    PlayerOnly,
    Off,
}

impl VehicleSmoke {
    pub fn from_setting(s: &str) -> Self {
        match s {
            "player" => Self::PlayerOnly,
            "off" => Self::Off,
            _ => Self::All,
        }
    }
}

/// Fill the scene's lights for this frame, the lamps' cones in the weather `cone`.
pub fn collect(
    world: &World,
    scene: &mut Scene,
    daylight: &Daylight,
    camera_pos: DVec3,
    vehicles: &[&VehicleInstance],
    cone: ConeWeather,
    smoke_mode: VehicleSmoke,
    player_first: bool,
) {
    scene.lights.clear();
    scene.coronas.clear();
    scene.smoke.clear();
    omsi_sim::particles::set_eye(camera_pos);
    let night = daylight.night;
    {
        let mut near = world.near_lights.lock();
        let static_lights = world.static_lights.lock();
        let static_coronas = world.static_coronas.lock();
        let generation = world
            .tiles_generation
            .load(std::sync::atomic::Ordering::Relaxed);
        let stale = near.generation != generation
            || near.lamps_on != daylight.lamps_on
            || near.counts != (static_lights.len(), static_coronas.len())
            || (near.centre - camera_pos).length() > NEAR_MARGIN;
        if stale {
            *near = NearLights {
                generation,
                lamps_on: daylight.lamps_on,
                centre: camera_pos,
                counts: (static_lights.len(), static_coronas.len()),
                lights: Vec::new(),
                coronas: Vec::new(),
            };
            if daylight.lamps_on {
                near.lights.extend(static_lights.iter().filter(|l| {
                    (l.position - camera_pos).length() < MAP_LIGHT_RANGE + NEAR_MARGIN
                }));
            }
            for c in static_coronas.iter() {
                let on = match &c.switch {
                    LightSwitch::Constant(x) => *x,
                    LightSwitch::Night => daylight.lamps_on as i32 as f32,
                    LightSwitch::Variable(_) => daylight.lamps_on as i32 as f32,
                };
                if on <= 0.0
                    || (c.corona.position - camera_pos).length() > CORONA_RANGE + NEAR_MARGIN
                {
                    continue;
                }
                let mut corona = c.corona;
                corona.brightness *= on.min(1.0);
                near.coronas.push(corona);
            }
        }
        scene.lights.extend(
            near.lights
                .iter()
                .filter(|l| (l.position - camera_pos).length() < MAP_LIGHT_RANGE),
        );
        scene.coronas.extend(
            near.coronas
                .iter()
                .filter(|c| (c.position - camera_pos).length() <= CORONA_RANGE),
        );
    }
    // traffic lamps glow with what they show, by day as well
    for lamp in world.light_objects.lock().iter() {
        if (lamp.pos - camera_pos).length() > 1500.0 {
            continue;
        }
        for ((c, _), lit) in lamp.coronas.iter().zip(&lamp.lit) {
            if *lit <= 0.0 {
                continue;
            }
            let mut corona = *c;
            corona.brightness *= lit.min(1.0);
            scene.coronas.push(corona);
        }
    }
    // scenery lamps their own script switches, as bright as it says
    crate::scripted::push_script_lamp_glows(&world.scripted.lock().views, camera_pos, CORONA_RANGE, &mut scene.coronas);
    for list in world.particle_objects.lock().values() {
        for po in list {
            if (po.pos - camera_pos).length() < 1500.0 {
                particle_sprites(&po.set, &mut scene.smoke, &mut scene.coronas);
            }
        }
    }
    if omsi_cfg::flags::OMSI_DEBUG_PARTICLES.is_set() {
        if let Some(p) = scene.smoke.first() {
            log::info!("smoke: {} particles from objects, first at ({:.1}, {:.1}, {:.1}) size {:.2} alpha {:.2}", scene.smoke.len(), p.position.x, p.position.y, p.position.z, p.size, p.alpha);
        }
    }
    let vehicle_smoke = scene.smoke.len();
    // the headlamps lent to cars without spots of their own: the nearest few alone (see
    // `AI_BEAMS`)
    // (by day they light nothing anyone sees, yet any light at all in the scene makes every
    // pixel look through the lamp grid and the weather's fog work the lamps' air light out -
    // 13 ms of a 25 ms frame in a grey winter noon where the cars have their lights on; so
    // how many cars are lent beams, and how far, goes with how dark the scene is, the weather's
    // gloom included, see `cars_with_beams`)
    let dark = if cone.enhanced { night.max(cone.gloom) } else { 1.0 };
    let beams = cars_with_beams(vehicles, camera_pos, dark);
    for (k, v) in vehicles.iter().enumerate() {
        // (what is not drawn does not shine: a car out of the view and beyond the mirrors'
        // reach, or past the coronas' range, is left out whole - at night with hundreds of
        // cars about their lamps made up most of the lights and coronas of a frame)
        let d = (v.position - camera_pos).length();
        if d > CORONA_RANGE || (!v.ai_visuals && d > UNSEEN_LIGHT_RANGE) {
            continue;
        }
        vehicle_lights(v, &mut scene.coronas, &mut scene.lights, night, &world.cookie_fades, beams.contains(&k));
        // light_vehicles puts the player's bus first (if any). LAN guests and AI
        // traffic must not be mistaken for it when no bus is driven locally.
        let vehicle_smoke_enabled = match smoke_mode {
            VehicleSmoke::All => true,
            VehicleSmoke::PlayerOnly => player_first && k == 0,
            VehicleSmoke::Off => false,
        };
        particle_sprites_filtered(&v.particles, &mut scene.smoke, &mut scene.coronas, vehicle_smoke_enabled);
        for t in &v.trailers {
            particle_sprites_filtered(&t.particles, &mut scene.smoke, &mut scene.coronas, vehicle_smoke_enabled);
        }
    }
    crate::puddles::keep_out_of_cab(vehicles, camera_pos, &mut scene.smoke, vehicle_smoke);
    // the lamps' cones in fog: drawn only while the visibility is
    // under 2 km, the fan's radius 2 m times 3 sqrt(100 / visibility) times the glow's
    // strength ((1 - ambient)^2 + 0.8) 0.6 brightness and the light's size; its colour the
    // light's times 0.3 (the viewing angle and the distance take their share in the shader,
    // which is told the visibility in `beam_width`)
    let (vis, night) = cone.get();
    // The halo round a light in fog is half the cone's radius across either way, at 0.2.
    scene.coronas.retain_mut(|c| {
        if !c.beam && !c.halo {
            return true;
        }
        if vis >= 2000.0 {
            return false;
        }
        let glow = (night * night + 0.8) * 0.6 * c.brightness;
        let reach = 3.0 * (100.0 / vis.max(1.0)).sqrt() * glow * c.size;
        c.size = if c.beam { 2.0 * reach } else { reach };
        c.brightness = if c.beam { 0.3 } else { 0.2 };
        c.beam_width = vis.max(1.0);
        c.size > 0.05
    });
    if omsi_cfg::flags::OMSI_DEBUG_CONES.is_set() {
        log::info!("cones: visibility {vis:.0} m, dark {night:.2}, {} cones of {} coronas", scene.coronas.iter().filter(|c| c.beam).count(), scene.coronas.len());
        for c in scene.coronas.iter().filter(|c| c.beam).take(4) {
            log::info!("  cone at ({:.1}, {:.1}, {:.1}) dir {:?} radius {:.2} half angles {:.0}/{:.0} deg tex {}", c.position.x, c.position.y, c.position.z, c.direction, c.size, c.inner_cos.to_degrees(), c.cone_cos.to_degrees(), c.texture);
        }
    }
    if omsi_cfg::flags::OMSI_DEBUG_LIGHT.is_set() {
        scene.lights.push(PointLight {
            position: camera_pos + DVec3::new(0.0, 15.0, -2.0),
            radius: 40.0,
            color: [1.0, 0.9, 0.7],
            intensity: 2.0,
            ..Default::default()
        });
        scene.coronas.push(Corona {
            position: camera_pos + DVec3::new(0.0, 15.0, 0.0),
            size: 1.0,
            color: [1.0, 0.9, 0.7],
            brightness: 1.0,
            direction: Vec3::ZERO,
            cone_cos: -1.0,
            ..Default::default()
        });
        log::info!(
            "static lights: {:?}",
            world
                .static_lights
                .lock()
                .iter()
                .take(3)
                .collect::<Vec<_>>()
        );
        log::info!(
            "static coronas: {:?}",
            world
                .static_coronas
                .lock()
                .iter()
                .take(3)
                .collect::<Vec<_>>()
        );
    }
    // nearest lights first: the grid cells hold a limited number
    // (total_cmp: a light at a NaN position must not end the game)
    scene.lights.sort_by(|a, b| (a.position - camera_pos).length_squared().total_cmp(&(b.position - camera_pos).length_squared()));
}

/// How far Omsi.exe moves every `[smoke]` and `[particle_emitter]` puff towards the eye in
/// depth (m): 0x5a1d54 gives each particle 0.1, which 0x5a2b5c takes off the view depth it
/// projects the puff's depth at.
const SMOKE_Z_OFFSET: f32 = 0.1;

/// The particles of a particle set as the renderer draws them: smoke blended over the scene,
/// and the glowing ones (`--PS_emissive--`: sparks, rockets, a flame) as coronas.
pub fn particle_sprites(set: &omsi_sim::particles::ParticleSet, smoke: &mut Vec<omsi_render::SmokeParticle>, coronas: &mut Vec<Corona>) {
    particle_sprites_filtered(set, smoke, coronas, true);
}

/// Keep glowing effects even when a vehicle's non-emissive smoke is hidden.
fn particle_sprites_filtered(set: &omsi_sim::particles::ParticleSet, smoke: &mut Vec<omsi_render::SmokeParticle>, coronas: &mut Vec<Corona>, smoke_enabled: bool) {
    for (p, def) in set.particles() {
        if !smoke_enabled && !def.emissive {
            continue;
        }
        let alpha = p.alpha();
        if alpha <= 0.002 {
            continue;
        }
        if def.emissive {
            coronas.push(Corona {
                position: p.pos,
                size: (p.size() * 0.5).max(0.02),
                color: p.color,
                brightness: alpha,
                direction: Vec3::ZERO,
                cone_cos: -1.0,
                z_offset: 0.0,
                ..Default::default()
            });
        } else {
            smoke.push(omsi_render::SmokeParticle {
                position: p.pos,
                size: p.size() * 0.5,
                color: p.color,
                alpha,
                spin: p.spin,
                z_offset: SMOKE_Z_OFFSET,
                // (to fade into, where Omsi.exe lets the road cut it off)
                ground: p.ground.is_finite().then_some(p.ground),
            });
        }
    }
}

/// OMSI's picture for every `[smoke]` particle: `Texture\rauch.tga` of the game folder.
pub fn load_smoke_texture(renderer: &mut omsi_render::Renderer, root: &std::path::Path) {
    let path = omsi_cfg::resolve_path(root, "Texture/rauch.tga");
    match omsi_texture::decode_file(&path) {
        Ok(img) => renderer.set_smoke_texture(&img),
        Err(e) => log::warn!("smoke texture {}: {e}", path.display()),
    }
}

/// The pictures of the coronas besides the standard glow: a light's own `bitmap` and the
/// fog cone's `Texture/light_cone.bmp`, numbered as they are first asked for and uploaded
/// before the next frame is drawn (`upload_corona_textures`).
/// Process-wide on purpose: it only numbers picture files (the numbers live on in the
/// world's lights); which of them a renderer holds is the renderer's
/// (`Renderer::pictures_taken`), so a second session's renderer gets them all again.
struct CoronaTextures {
    ids: std::collections::HashMap<std::path::PathBuf, u16>,
    /// Every picture numbered, in the order it was numbered.
    listed: Vec<(u16, std::path::PathBuf)>,
    root: Option<std::path::PathBuf>,
}

static CORONA_TEXTURES: std::sync::Mutex<Option<CoronaTextures>> = std::sync::Mutex::new(None);

/// The game folder the standard pictures (`Texture\…`) are found in.
pub fn set_corona_root(root: &std::path::Path) {
    let mut g = CORONA_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    let t = g.get_or_insert_with(|| CoronaTextures { ids: Default::default(), listed: Vec::new(), root: None });
    t.root = Some(root.to_path_buf());
    // (the game's own pictures are looked for in the new folder)
    for id in [&CONE_ID, &GLOW_ID, &STAR_ID] {
        id.store(UNKNOWN_ID, std::sync::atomic::Ordering::Relaxed);
    }
}

fn texture_id_of(path: std::path::PathBuf) -> u16 {
    let mut g = CORONA_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    let t = g.get_or_insert_with(|| CoronaTextures { ids: Default::default(), listed: Vec::new(), root: None });
    if let Some(id) = t.ids.get(&path) {
        return *id;
    }
    let id = (t.ids.len() + 1).min(u16::MAX as usize) as u16;
    t.ids.insert(path.clone(), id);
    t.listed.push((id, path));
    id
}

/// The picture id of a light's `bitmap`, looked for as the model's own textures are (its
/// folder's `texture\`, the folder, the vehicle's `texture\`, the game's `Texture\`, a `.dds`
/// of the same name first, then the other picture formats); 0 (the standard glow) when there
/// is no such file.
pub fn corona_texture_id(model_dir: &std::path::Path, name: &str) -> u16 {
    // (every lit lamp of every vehicle asks every frame: looked up once, and found again by
    // the folder and the name as they are - a key made of copies of them was two
    // allocations a lamp a frame)
    type Known = std::collections::HashMap<std::path::PathBuf, std::collections::HashMap<String, u16>>;
    static KNOWN: std::sync::Mutex<Option<Known>> = std::sync::Mutex::new(None);
    if let Some(&id) = KNOWN.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(model_dir)).and_then(|m| m.get(name)) {
        return id;
    }
    let root = CORONA_TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|t| t.root.clone()).unwrap_or_default();
    // (found as a material's texture is: the Helferlein signals' `Eff_li.bmp` ships only
    // as `Eff_li.dds`, and looked for under its own name alone the plain glow was drawn)
    let search = crate::texture_dirs(&root, model_dir);
    let search: Vec<&std::path::Path> = search.iter().map(std::path::PathBuf::as_path).collect();
    let id = omsi_texture::find_texture(name, &search).map_or(0, texture_id_of);
    KNOWN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(Default::default)
        .entry(model_dir.to_path_buf())
        .or_default()
        .insert(name.to_string(), id);
    id
}

/// The id of a game picture not looked for yet in the current game folder. (The ids below
/// are numbers of `CORONA_TEXTURES`, the same for every renderer.)
const UNKNOWN_ID: u32 = u32::MAX;
static CONE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(UNKNOWN_ID);
static GLOW_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(UNKNOWN_ID);
static STAR_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(UNKNOWN_ID);

/// One of the game's own light pictures in `Texture\` (0, the built-in glow, if missing).
fn stock_texture_id(name: &str, known: &std::sync::atomic::AtomicU32) -> u16 {
    // (asked for every light of every vehicle every frame: the file is looked up once per
    // game folder, not each time - the lookups were a fifth of a frame's CPU time - and
    // kept where no lock and no key has to be made to read it again)
    let id = known.load(std::sync::atomic::Ordering::Relaxed);
    if id != UNKNOWN_ID {
        return id as u16;
    }
    let root = CORONA_TEXTURES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|t| t.root.clone()).unwrap_or_default();
    let p = omsi_cfg::resolve_path(&root, &format!("Texture/{name}"));
    let id = if omsi_cfg::vfs::is_file(&p) { texture_id_of(p) } else { 0 };
    known.store(id as u32, std::sync::atomic::Ordering::Relaxed);
    id
}

/// The fog cone's picture (`Texture\light_cone.bmp`).
pub fn cone_texture_id() -> u16 {
    stock_texture_id("light_cone.bmp", &CONE_ID)
}

/// A light's glow when it names no bitmap of its own, and the halo round it in fog
/// (`Texture\licht.bmp`, the original).
pub fn glow_texture_id() -> u16 {
    stock_texture_id("licht.bmp", &GLOW_ID)
}

/// The star of a light with effect bit 1 (`Texture\light_effect1.bmp`).
pub fn star_texture_id() -> u16 {
    stock_texture_id("light_effect1.bmp", &STAR_ID)
}

/// Upload the pictures this renderer has not been handed yet (those asked for since the
/// last frame; all of them for a new renderer).
pub fn upload_corona_textures(renderer: &mut omsi_render::Renderer) {
    let (taken, cookies_taken) = renderer.pictures_taken();
    let pending = {
        let g = CORONA_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
        match g.as_ref() {
            Some(t) => t.listed.get(taken..).unwrap_or_default().to_vec(),
            None => return,
        }
    };
    let taken = taken + pending.len();
    for (id, path) in pending {
        match omsi_texture::decode_file(&path) {
            Ok(img) => renderer.set_corona_texture(id, &img),
            Err(e) => log::warn!("corona picture {}: {e}", path.display()),
        }
    }
    // (and the `[spotlight_cookie]` pictures: see `cookie_picture_id`)
    let cookies = COOKIE_PICTURES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|c| c.1.get(cookies_taken..).unwrap_or_default().to_vec()).unwrap_or_default();
    renderer.set_pictures_taken((taken, cookies_taken + cookies.len()));
    for (id, path) in cookies {
        match omsi_texture::decode_file(&path) {
            Ok(img) => renderer.set_cookie_picture(id, &img),
            Err(e) => log::warn!("spotlight cookie picture {}: {e}", path.display()),
        }
    }
}

/// The weather the lamps' cones are drawn in (see [`collect`]): the fog's visibility (m,
/// 0 for none known: no cones) and how dark it is (0 day … 1 night). Falling rain or snow
/// counts only as far as it lowers the visibility.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ConeWeather {
    pub fog_visibility_m: f32,
    pub night: f32,
    /// How dark the weather alone makes the day (`gloom_darkness`): it does not change the
    /// cones, but counts in whether the scene is dark enough for the AI cars' beams.
    pub gloom: f32,
    pub enhanced: bool,
}

impl ConeWeather {
    pub fn new(fog_visibility_m: f32, night: f32) -> ConeWeather {
        ConeWeather { fog_visibility_m, night: night.clamp(0.0, 1.0), gloom: 0.0, enhanced: false }
    }

    /// [`ConeWeather::new`] for the weather `w`, whose clouds and rain darken the day.
    pub fn of(w: &omsi_content::weather::Weather, night: f32) -> ConeWeather {
        let (kind, rate) = omsi_weather::conditions::precip_of(w);
        let overcast = (omsi_content::weather_choice::cover_of(w).clamp(0.0, 1.0) - 0.45).max(0.0) / 0.55;
        let rain = if kind != 0 { rate.clamp(0.0, 1.0) } else { 0.0 };
        ConeWeather { gloom: gloom_darkness(overcast, rain), ..ConeWeather::new(w.fog.0, night) }
    }

    /// The visibility (no fog: a million metres) and the dark.
    fn get(self) -> (f32, f32) {
        let vis = self.fog_visibility_m;
        (if vis > 0.0 { vis } else { 1.0e6 }, self.night)
    }
}

pub use omsi_weather::conditions::vehicle_velocity;
