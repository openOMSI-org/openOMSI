//! Vehicle lamp sprites and the classic and Enhanced lights that illuminate the road.

use super::cookie_picture_id;
use glam::{DVec3, Vec3};
use omsi_render::{Corona, LightMode, PointLight};
use omsi_sim::VehicleInstance;

/// A headlight's strength for the enhanced renderer, its beam's at full (`headlamp` in
/// `enhanced.wgsl`): the road from the bumper out to some 12 m lit about as brightly as a
/// street under a lamp, and less beyond.
const HEADLIGHT_INTENSITY: f32 = 1800.0;

/// The road-headlamp profile has a brighter high-beam core of its own. At the
/// renderer's 55 lux per intensity unit, the old strength gave each lamp nearly
/// 50,000 cd on low beam and 350,000 cd on high beam. This scale brings the generic
/// pair to 33,000 / 231,000 cd, without dimming work lights or authored cookies.
const ROAD_HEADLIGHT_GAIN: f32 = 1.0 / 3.0;

/// A `[spotlight]` declaring this range or more is a full beam (the stock buses' 500, the
/// Grand Paris-Moulon Citaro's 450), less a low beam (the stock 100, that Citaro's 200).
const FULL_BEAM_RANGE: f32 = 300.0;

/// How far the classic picture's headlight is followed (m), at most: Omsi.exe gives its
/// Direct3D spot four times the declared range, where its light has long gone (the stock low
/// beam's, range 100, is down to 3 per cent of the bumper's at 45 m), and a light that far
/// would take a place in every light-grid cell of the street.
const VANILLA_SPOT_REACH: f32 = 150.0;

/// Coronas and point lights of one vehicle in its current state.
pub fn vehicle_lights(
    v: &VehicleInstance,
    coronas: &mut Vec<Corona>,
    lights: &mut Vec<PointLight>,
    // the player's vehicle: Omsi.exe lights the road by its [spotlight] alone (light 3)
    player: bool,
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
    crate::vehicle_model_lights_extend(
        &ty.model,
        &mesh_xf,
        v.position,
        &value_of,
        &v.light_fade,
        coronas,
    );
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
        let car_value =
            |name: &str| -> f32 { t.run_var(name.trim()).unwrap_or_else(|| value_of(name)) };
        crate::vehicle_model_lights_extend(
            &t.ty.model,
            &part_mesh_xf,
            t.position,
            &car_value,
            &t.light_fade,
            coronas,
        );
    }
    let body = v.body_rotation();
    // headlights: the spotlight selected by Spot_Select
    // (OMSI_SPOT_SELECT=n: that spotlight on, for checking the headlights in a picture)
    let forced = omsi_cfg::flags::OMSI_SPOT_SELECT
        .var()
        .and_then(|s| s.trim().parse::<f32>().ok());
    if let Some(sel) = forced.or_else(|| v.var("Spot_Select")) {
        if sel >= 0.0 {
            if let Some(sp) = ty.model.spotlights.get(sel as usize) {
                let vals = sp.values;
                let dl = Vec3::new(vals[3], vals[4], vals[5]).normalize_or_zero();
                let d = body.transform_vector3(dl).normalize_or_zero();
                let level = level_axis(body, dl);
                let origins = spot_origins(
                    &ty.model,
                    ty.def.bounding_box,
                    &vals,
                    &v.light_fade,
                    &value_of,
                );
                // the classic picture: Omsi.exe's one spot, the player's, where the
                // [spotlight] puts it (its light leaves from the vehicle's face, see
                // `push_classic_spot`) - wherever that is: the Karosa B951's full beams stand
                // 5 m up, over its roof
                let total: f32 = origins.iter().map(|o| o.1).sum();
                if player && total > 0.0 {
                    let face = origins.iter().map(|(at, share)| *at * *share).sum::<Vec3>() / total;
                    let at = Vec3::new(vals[0], face.y, vals[2]);
                    let source = v.position
                        + body
                            .transform_point3(Vec3::new(vals[0], vals[1], vals[2]))
                            .as_dvec3();
                    push_classic_spot(
                        lights,
                        v.position + body.transform_point3(at).as_dvec3(),
                        d,
                        &vals,
                        1.0,
                        source,
                        true,
                    );
                }
                for (at, share) in origins {
                    push_enhanced_spot(
                        lights,
                        v.position + body.transform_point3(at).as_dvec3(),
                        d,
                        level,
                        &vals,
                        share,
                    );
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
            let vals = [
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, color[0], color[1], color[2], 100.0, 30.0, 70.0,
            ];
            push_enhanced_spot(lights, at, d, level, &vals, share);
        }
    }
    // [spotlight_2] (openBus): every spot its variable switches on, the leading vehicle's
    // and its sections', where it is declared - a pair mirrored across the vehicle's axis
    // unless its flag keeps the one lamp - and as bright as a [spotlight] of its colour,
    // shared between the pair, times the variable (0..1)
    let parts = std::iter::once((&ty.model, v.position, body)).chain(
        v.trailers
            .iter()
            .map(|t| (&t.ty.model, t.position, t.body_rotation())),
    );
    for (model, origin, rot) in parts {
        for sp in &model.spotlights_2 {
            for (pos, dir, share) in spotlight_2_lamps(sp, value_of(sp.variable.as_str())) {
                let at = origin + rot.transform_point3(pos).as_dvec3();
                let d = rot.transform_vector3(dir).normalize_or_zero();
                push_spot(lights, at, d, level_axis(rot, dir), &sp.values, share);
            }
        }
    }
    // [spotlight_cookie] (openBus): lamps whose beam is a picture, the leading vehicle's
    // and its sections'
    let parts = std::iter::once((&ty.model, v.position, body)).chain(
        v.trailers
            .iter()
            .map(|t| (&t.ty.model, t.position, t.body_rotation())),
    );
    for (part, (model, origin, rot)) in parts.enumerate() {
        let dir = model.path.parent().unwrap_or(std::path::Path::new(""));
        for (k, c) in model.spotlight_cookies.iter().enumerate() {
            let key = (v as *const VehicleInstance as usize, part, k);
            let on = cookie_fade(
                &mut fades.lock(),
                key,
                value_of(c.variable.as_str()).clamp(0.0, 1.0),
                c.fade,
            );
            if on <= 0.0 {
                continue;
            }
            let turn = |name: &Option<String>| name.as_deref().map_or(0.0, |n| value_of(n));
            let local = cookie_direction(Vec3::from(c.dir), turn(&c.yaw), turn(&c.pitch));
            let at = origin + rot.transform_point3(Vec3::from(c.pos)).as_dvec3();
            let d = rot.transform_vector3(local).normalize_or_zero();
            push_cookie(lights, at, d, c, on, cookie_picture_id(dir, &c.picture));
        }
    }
    // [interiorlight]s light only the meshes listing them and the passengers (per-instance
    // term, see MeshProps::interior); they do not shine on the outside world.
    let _ = &ty.model.interior_lights;
}

fn spot_origins(
    model: &omsi_model::Model,
    bounding_box: Option<[f32; 6]>,
    vals: &[f32; 12],
    fades: &[f32],
    value_of: &dyn Fn(&str) -> f32,
) -> Vec<(Vec3, f32)> {
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
        .flat_map(|m| {
            m.light_enh
                .iter()
                .map(|l| l.pos)
                .chain(m.light_enh_2.iter().map(|l| l.pos))
        })
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
    sides
        .iter()
        .map(|side| (apex + Vec3::X * spread * side, 1.0 / sides.len() as f32))
        .collect()
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
fn push_cookie(
    lights: &mut Vec<PointLight>,
    at: DVec3,
    d: Vec3,
    c: &omsi_model::SpotlightCookie,
    on: f32,
    picture: u16,
) {
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
        radius: classic_spot_reach(c.range),
        color,
        intensity: on,
        direction: d,
        cone: [inner, outer],
        mode: LightMode::Vanilla,
        d3d_spot: Some(omsi_render::D3dSpot {
            range: c.range.max(0.1),
            source: at,
            player: false,
        }),
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
pub(crate) type CookieFades =
    std::collections::HashMap<(usize, usize, usize), (f32, std::time::Instant)>;

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
        .map(|s| {
            (
                Vec3::new(v[0] * s, v[1], v[2]),
                Vec3::new(v[3] * s, v[4], v[5]),
                k / sides.len() as f32,
            )
        })
        .collect()
}

/// A lamp's axis (`local`, in its vehicle's frame) laid into the vehicle's floor, in the
/// world (`rot`: the vehicle's rotation): the way along the road it shines, whose rise is
/// the vehicle's pitch - the horizon of its beam (see `push_enhanced_spot`). Zero for a lamp
/// pointing straight up or down.
fn level_axis(rot: glam::Mat4, local: Vec3) -> Vec3 {
    rot.transform_vector3(Vec3::new(local.x, local.y, 0.0))
        .normalize_or_zero()
}

/// The lights of one headlamp at `at` shining along `d`, with a `[spotlight]`'s numbers
/// (`vals`: colour 6-8, range 9, inner and outer cone 10 and 11) and `share` of its light;
/// `level` its axis in the vehicle's floor (see [`level_axis`]).
fn push_spot(
    lights: &mut Vec<PointLight>,
    at: DVec3,
    d: Vec3,
    level: Vec3,
    vals: &[f32; 12],
    share: f32,
) {
    push_classic_spot(lights, at, d, vals, share, at, false);
    push_enhanced_spot(lights, at, d, level, vals, share);
}

/// How far the classic picture follows a spot of this declared range (see
/// `VANILLA_SPOT_REACH`).
fn classic_spot_reach(range: f32) -> f32 {
    (range.max(0.1) * 4.0).min(VANILLA_SPOT_REACH)
}

/// The classic picture's spot, as Omsi.exe sets up its Direct3D spot light for the
/// `[spotlight]` the script selects (0x708acc): at the spotlight's own place, its colour
/// (0..255, 0x5f3f71) times 0.3 as diffuse light and 0.1 as ambient light, its cone's full
/// angles (degrees to radians, 0x7f1f30) with a falloff of 2, attenuation
/// 1 / (0.2 + (1 / range) d + (40 / range²) d²) (0.01 x 100 / range and 0.004 x (100 /
/// range)²). `source` is the declared position transformed with the vehicle body; `at`
/// stays at the vehicle's face for occlusion (`spot_origins` - the stock NL202's spot
/// is 3.8 m inside the bus). The player's spot is shaded through the camera uniform.
pub(super) fn push_classic_spot(
    lights: &mut Vec<PointLight>,
    at: DVec3,
    d: Vec3,
    vals: &[f32; 12],
    share: f32,
    source: DVec3,
    player: bool,
) {
    let color = [vals[6] / 255.0, vals[7] / 255.0, vals[8] / 255.0];
    // inner and outer cone as full angles (values 10 and 11)
    let (inner, outer) = (vals[10], vals[11]);
    let half = |deg: f32| (deg.clamp(1.0, 179.0) * 0.5).to_radians().cos();
    let cone = [half(inner.min(outer)), half(outer)];
    lights.push(PointLight {
        position: at,
        radius: classic_spot_reach(vals[9]),
        color,
        intensity: share,
        direction: d,
        cone,
        mode: LightMode::Vanilla,
        d3d_spot: Some(omsi_render::D3dSpot {
            range: vals[9].max(0.1),
            source,
            player,
        }),
        ..Default::default()
    });
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
    model.spotlights.is_empty()
        && model.spotlights_2.is_empty()
        && model.spotlight_cookies.is_empty()
}

/// Which of `vehicles` (by index) may be lent their headlamps' beams (lit ones alone get
/// them): the nearest without spots of their own - `AI_BEAMS` within `AI_BEAM_RANGE` of the
/// camera in the dark, `AI_BEAMS_DIM` within `AI_BEAM_RANGE_DIM` in a gloomy day (`dark` from
/// `AI_BEAM_MIN_DARK`), none by day.
pub(super) fn cars_with_beams(
    vehicles: &[&VehicleInstance],
    camera: DVec3,
    dark: f32,
) -> Vec<usize> {
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
fn headlamps_of_lamps(
    model: &omsi_model::Model,
    fades: &[f32],
    value_of: &dyn Fn(&str) -> f32,
) -> Vec<(Vec3, Vec3, [f32; 3], f32)> {
    let mut found: Vec<FrontLamp> = white_front_lamps(model, fades, value_of)
        .into_iter()
        .filter(|f| f.3 >= 0.5)
        .collect();
    let Some(front) = found.iter().map(|f| f.0.y).reduce(f32::max) else {
        return Vec::new();
    };
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

fn white_front_lamps(
    model: &omsi_model::Model,
    fades: &[f32],
    value_of: &dyn Fn(&str) -> f32,
) -> Vec<FrontLamp> {
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
            let (hi, lo) = (
                l.color.iter().copied().fold(0.0, f32::max),
                l.color.iter().copied().fold(f32::MAX, f32::min),
            );
            let pos = Vec3::from(l.pos);
            if l.omni || dir.y < 0.8 || hi < 150.0 || lo < 0.6 * hi || !(0.2..1.6).contains(&pos.z)
            {
                continue;
            }
            let lit = if lit.is_nan() { 0.0 } else { lit };
            found.push((
                pos,
                dir,
                l.color,
                lit,
                lit / if l.factor > 0.0 { l.factor } else { 1.0 },
            ));
        }
    }
    found
}

fn by_side(lamps: &[FrontLamp]) -> Vec<Vec<&FrontLamp>> {
    [
        lamps.iter().filter(|f| f.0.x < -0.2).collect(),
        lamps.iter().filter(|f| f.0.x > 0.2).collect(),
        lamps.iter().filter(|f| f.0.x.abs() <= 0.2).collect(),
    ]
    .into_iter()
    .filter(|s: &Vec<_>| !s.is_empty())
    .collect()
}

fn spot_headlamps(
    model: &omsi_model::Model,
    fades: &[f32],
    value_of: &dyn Fn(&str) -> f32,
) -> Vec<(Vec3, f32)> {
    let mut found = white_front_lamps(model, fades, value_of);
    let Some(front) = found.iter().map(|f| f.0.y).reduce(f32::max) else {
        return Vec::new();
    };
    let nose = model
        .meshes
        .iter()
        .flat_map(|m| {
            m.light_enh
                .iter()
                .map(|l| l.pos[1])
                .chain(m.light_enh_2.iter().map(|l| l.pos[1]))
        })
        .fold(front, f32::max);
    if front < nose - 0.5 {
        return Vec::new();
    }
    found.retain(|f| f.0.y >= front - 0.4);
    let on = |f: &FrontLamp| f.4 >= 0.5;
    let lit_top = |lamps: &[&FrontLamp]| {
        lamps
            .iter()
            .filter(|f| on(f))
            .map(|f| f.0.z)
            .reduce(f32::max)
    };
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
            let target = lit_top(&set)
                .or(any_lit)
                .unwrap_or_else(|| set.iter().map(|f| f.0.z).fold(f32::MIN, f32::max));
            let best = set
                .iter()
                .map(|f| f.0.z)
                .min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs()))
                .unwrap_or(target);
            let group: Vec<&FrontLamp> = set
                .into_iter()
                .filter(|f| (f.0.z - best).abs() <= 0.1)
                .collect();
            let dir = group
                .iter()
                .map(|f| f.1)
                .sum::<Vec3>()
                .normalize_or(Vec3::Y);
            (
                group.iter().map(|f| f.0).sum::<Vec3>() / group.len() as f32 + dir * 0.1,
                1.0 / n,
            )
        })
        .collect()
}

/// A headlamp's light in the Enhanced picture alone (see `push_spot`). A lamp aimed along the
/// road is lit by a road lamp's profile about its horizon, which is the vehicle's floor: its
/// direction is `level`, the axis laid into that floor, whose rise is the vehicle's pitch.
/// (Taken about the world's horizon, the beam stayed level while the bus climbed a ramp and
/// lit less and less of the road rising in front of it, as Omsi.exe's spot - fixed to the
/// bus - does not, #1897.)
pub(super) fn push_enhanced_spot(
    lights: &mut Vec<PointLight>,
    at: DVec3,
    d: Vec3,
    level: Vec3,
    vals: &[f32; 12],
    share: f32,
) {
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
    let beam = if d.normalize_or_zero().z.abs() >= 0.5 || level == Vec3::ZERO {
        0.0
    } else if vals[9] >= FULL_BEAM_RANGE {
        -1.0
    } else {
        1.0
    };
    lights.push(PointLight {
        position: at,
        radius: reach,
        color,
        intensity: HEADLIGHT_INTENSITY
            * share
            * weaker
            * if beam != 0.0 {
                ROAD_HEADLIGHT_GAIN
            } else {
                1.0
            },
        direction: if beam != 0.0 { level } else { d },
        cone,
        core: 1.0,
        beam,
        housed: false,
        shadow_owner: None,
        mode: LightMode::Enhanced,
        cookie: None,
        flash: false,
        d3d_spot: None,
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
    range
        .clamp(0.5, low)
        .max(range * low / 100.0)
        .min(low * 5.0)
}

#[cfg(test)]
mod cookie_tests {
    use super::*;

    /// A cornering lamp's yaw turns it to the right, a levelling lamp's pitch up.
    #[test]
    fn a_cookie_turns_by_its_yaw_and_pitch() {
        let ahead = Vec3::Y;
        let right = cookie_direction(ahead, 30.0, 0.0);
        assert!(
            right.x > 0.49 && right.x < 0.51 && right.y > 0.86,
            "{right:?}"
        );
        let up = cookie_direction(ahead, 0.0, 10.0);
        assert!(up.z > 0.17 && up.z < 0.18 && up.x.abs() < 1e-5, "{up:?}");
        assert_eq!(cookie_direction(ahead, 0.0, 0.0), ahead);
    }

    /// A cookie lamp is a classic spot and an enhanced one with its picture; without a
    /// picture found the enhanced one keeps the plain cone.
    #[test]
    fn a_cookie_lamp_falls_back_to_its_cone() {
        let c = omsi_model::SpotlightCookie {
            dir: [0.0, 1.0, 0.0],
            color: [255.0; 3],
            range: 80.0,
            width: 60.0,
            height: 20.0,
            ..Default::default()
        };
        let mut lights = Vec::new();
        push_cookie(&mut lights, DVec3::ZERO, Vec3::Y, &c, 1.0, 7);
        let e = lights
            .iter()
            .find(|l| l.mode == LightMode::Enhanced)
            .unwrap();
        assert_eq!(e.cookie.map(|c| c.picture), Some(7));
        assert_eq!(e.beam, 0.0);
        assert!(
            e.cone[1] < 30f32.to_radians().cos(),
            "the cone holds the picture's corners"
        );
        lights.clear();
        push_cookie(&mut lights, DVec3::ZERO, Vec3::Y, &c, 1.0, 0);
        assert!(lights.iter().all(|l| l.cookie.is_none()));
        assert_eq!(lights.len(), 2);
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
            let vals = [
                0.0, 6.5, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, range, 30.0, 80.0,
            ];
            let mut lights = Vec::new();
            super::push_spot(
                &mut lights,
                glam::DVec3::ZERO,
                glam::Vec3::Y,
                glam::Vec3::Y,
                &vals,
                0.5,
            );
            lights
                .iter()
                .find(|l| l.mode == omsi_render::LightMode::Enhanced)
                .map(|l| l.beam)
        };
        assert_eq!(
            [100.0, 200.0, 450.0, 500.0].map(beam),
            [Some(1.0), Some(1.0), Some(-1.0), Some(-1.0)]
        );
        // a lamp over a door, pointing down, keeps its cone
        let vals = [
            0.0, 6.5, 2.5, 0.0, 0.0, -1.0, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0,
        ];
        let mut lights = Vec::new();
        super::push_spot(
            &mut lights,
            glam::DVec3::ZERO,
            glam::Vec3::NEG_Z,
            glam::Vec3::ZERO,
            &vals,
            0.5,
        );
        assert_eq!(
            lights
                .iter()
                .find(|l| l.mode == omsi_render::LightMode::Enhanced)
                .map(|l| l.beam),
            Some(0.0)
        );
    }

    /// The classic picture's `[spotlight]` is Omsi.exe's Direct3D spot (0x708acc): one light,
    /// its colour 0..1, its range as declared for the shader's attenuation, its cone's half
    /// angles, as strong by day as by night; a `[spotlight_2]` is lit the same way per pixel.
    #[test]
    fn a_spotlight_is_omsis_direct3d_spot_in_the_classic_picture() {
        let vals = [
            0.0, 2.0, 0.863, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 70.0,
        ];
        let mut lights = Vec::new();
        let source = glam::DVec3::new(0.0, -3.9, 0.0);
        super::push_classic_spot(
            &mut lights,
            glam::DVec3::ZERO,
            glam::Vec3::Y,
            &vals,
            1.0,
            source,
            true,
        );
        assert_eq!(lights.len(), 1);
        let l = &lights[0];
        assert_eq!(l.mode, omsi_render::LightMode::Vanilla);
        assert_eq!(
            l.d3d_spot,
            Some(omsi_render::D3dSpot {
                range: 100.0,
                source,
                player: true
            })
        );
        assert_eq!((l.intensity, l.radius), (1.0, super::VANILLA_SPOT_REACH));
        assert_eq!(l.color, [1.0, 1.0, 233.0 / 255.0]);
        assert!(
            (l.cone[0] - 15f32.to_radians().cos()).abs() < 1e-6
                && (l.cone[1] - 35f32.to_radians().cos()).abs() < 1e-6,
            "{:?}",
            l.cone
        );
        // (a short range: four times it, as Direct3D cuts the light off)
        let mut short = vals;
        short[9] = 5.0;
        let mut lights = Vec::new();
        super::push_spot(
            &mut lights,
            glam::DVec3::ZERO,
            glam::Vec3::Y,
            glam::Vec3::Y,
            &short,
            0.5,
        );
        let van = lights
            .iter()
            .find(|l| l.mode == omsi_render::LightMode::Vanilla)
            .unwrap();
        assert_eq!(
            (van.radius, van.intensity, van.d3d_spot),
            (
                20.0,
                0.5,
                Some(omsi_render::D3dSpot {
                    range: 5.0,
                    source: glam::DVec3::ZERO,
                    player: false
                })
            )
        );
    }

    /// A headlamp's beam lies about the vehicle's floor: on a bus pitched up a ramp its
    /// Enhanced light is aimed along the ramp, whatever the lamp's own aim down (#1897).
    #[test]
    fn a_headlamp_beam_pitches_with_its_vehicle() {
        let vals = [
            0.0, 6.5, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0,
        ];
        let pitch = 8f32.to_radians();
        let body = glam::Mat4::from_rotation_x(pitch);
        let dl = glam::Vec3::new(0.0, 1.0, -0.3).normalize();
        let d = body.transform_vector3(dl);
        let mut lights = Vec::new();
        super::push_spot(
            &mut lights,
            glam::DVec3::ZERO,
            d,
            super::level_axis(body, dl),
            &vals,
            0.5,
        );
        let enh = lights
            .iter()
            .find(|l| l.mode == omsi_render::LightMode::Enhanced)
            .unwrap();
        assert_eq!(enh.beam, 1.0);
        assert!(
            (enh.direction.z - pitch.sin()).abs() < 1e-4
                && (enh.direction.y - pitch.cos()).abs() < 1e-4,
            "{:?}",
            enh.direction
        );
        // (the classic spot keeps the lamp's own aim)
        let van = lights
            .iter()
            .find(|l| l.mode == omsi_render::LightMode::Vanilla)
            .unwrap();
        assert!((van.direction - d).length() < 1e-5);
    }

    /// `[spotlight_2]`: a pair mirrored across the axis sharing the light, or one lamp, as
    /// bright as its variable says, and nothing while that is off.
    #[test]
    fn a_spotlight_2_is_a_mirrored_pair_or_one_lamp() {
        use glam::Vec3;
        let mut sp = omsi_model::Spotlight2 {
            values: [
                0.9, 5.9, 0.65, 0.2, 1.0, -0.3, 255.0, 255.0, 233.0, 200.0, 30.0, 80.0,
            ],
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
        assert_eq!(
            super::spotlight_2_lamps(&sp, 0.5),
            [(Vec3::new(0.9, 5.9, 0.65), Vec3::new(0.2, 1.0, -0.3), 0.5)]
        );
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
        let mesh = |light_enh_2: Vec<omsi_model::LightEnh2>| omsi_model::Model {
            meshes: vec![omsi_model::MeshDef {
                light_enh_2,
                ..Default::default()
            }],
            ..Default::default()
        };
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
            omsi_model::LightEnh2 {
                color: [255.0, 160.0, 0.0],
                ..lamp(-1.1, 5.8, 1.05, "abbl_l")
            },
        ]);
        let bb = Some([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        let low = [
            0.0, 2.0, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0,
        ];
        let at = |model: &omsi_model::Model, on: &[&str], fades: &[f32], vals: &[f32; 12]| {
            let on: Vec<String> = on.iter().map(|s| s.to_string()).collect();
            let mut at = super::spot_origins(model, bb, vals, fades, &|name: &str| {
                if on.iter().any(|o| o == name) {
                    1.0
                } else {
                    0.0
                }
            });
            at.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
            at
        };
        let near = |got: &[(Vec3, f32)], want: &[(Vec3, f32)]| {
            got.len() == want.len()
                && got
                    .iter()
                    .zip(want)
                    .all(|(g, w)| (g.0 - w.0).length() < 1e-4 && g.1 == w.1)
        };
        let pair = |z: f32| {
            [
                (Vec3::new(-0.8, 5.9, z), 0.5),
                (Vec3::new(0.8, 5.9, z), 0.5),
            ]
        };
        let got = at(&model, &["abbl_l", "abbl_r", "stand"], &[], &low);
        assert!(
            near(&got, &pair(1.05)),
            "low beam, rear markers lit: {got:?}"
        );
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
        assert!(
            near(&got, &pair(1.2)),
            "the fades rule over the variables: {got:?}"
        );
        let dim = mesh(vec![
            omsi_model::LightEnh2 {
                factor: 0.3,
                ..lamp(-0.8, 5.8, 1.05, "abbl")
            },
            omsi_model::LightEnh2 {
                factor: 0.3,
                ..lamp(0.8, 5.8, 1.05, "abbl")
            },
            lamp(-0.7, 5.8, 0.6, "drl"),
            lamp(0.7, 5.8, 0.6, "drl"),
        ]);
        let got = at(&dim, &["abbl", "drl"], &[], &low);
        assert!(
            near(&got, &pair(1.05)),
            "a faint headlamp glow over a DRL: {got:?}"
        );
        let rear = [
            0.0, -5.0, 0.76, 0.0, -1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0,
        ];
        let got = at(&model, &["abbl_l", "abbl_r"], &[], &rear);
        assert!(
            got.iter()
                .all(|(p, _)| p.z == 0.76 && (p.y + 5.85).abs() < 1e-4),
            "a rear spot keeps its own height: {got:?}"
        );
        let inside = mesh(vec![
            lamp(-0.5, 2.0, 1.0, "x"),
            lamp(0.5, 2.0, 1.0, "x"),
            omsi_model::LightEnh2 {
                color: [255.0, 190.0, 0.0],
                ..lamp(0.9, 5.8, 0.9, "x")
            },
        ]);
        let got = at(&inside, &["x"], &[], &low);
        assert!(
            got.iter()
                .all(|(p, _)| p.z == 0.76 && (p.y - 5.85).abs() < 1e-4),
            "white lamps far behind the nose: {got:?}"
        );
        let got = super::spot_origins(&omsi_model::Model::default(), bb, &low, &[], &|_| 0.0);
        assert!(
            near(&got, &[(Vec3::new(0.0, 3.55, 0.76), 1.0)]),
            "no lamps: {got:?}"
        );
    }
}
