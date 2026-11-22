//! Per-frame light sources: scenery coronas/map lights by night, vehicle lights from
//! their script variables (`[light_enh_2]`, `[spotlight]` + `Spot_Select`, `[interiorlight]`).

use crate::{LightSwitch, World};
use glam::{DVec3, Vec3};
use omsi_render::{Corona, LightMode, Lighting, PointLight, Scene};

use omsi_sim::{Daylight, VehicleInstance};

mod vehicle;

pub(crate) use vehicle::CookieFades;
pub use vehicle::vehicle_lights;
use vehicle::{cars_with_beams, push_classic_spot, push_enhanced_spot};

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
        night_maps: Some(if d.lamps_on || d.night >= 0.5 {
            1.0
        } else {
            0.0
        }),
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
    // (a downpour much more so than a shower: at the heaviest the day is a dusk, a fifth or so
    // of an overcast day's light - taken a third off, a cloudburst at noon looked like any
    // grey day)
    let heavy = rain.powf(1.3);
    l.sun_intensity *= 1.0 - 0.85 * rain;
    l.ambient *= 1.0 - 0.55 * heavy;
    l.secondary *= 1.0 - 0.6 * heavy;
    l.sky_color *= 1.0 - 0.45 * heavy;
    l.fog_color *= 1.0 - 0.4 * heavy;
    if rain > 0.0 {
        l.fog_density = l.fog_density.max(2.3 / (2500.0 - 1800.0 * rain));
    }
    // the enhanced renderer builds its own sky from these
    l.overcast = overcast;
    l.rain = rain;
    l.snowfall = if precip_kind == 2 { rain } else { 0.0 };
    l.night = l.night.max(gloom_darkness(overcast, rain));
    // (by day in a downpour some of the houses' windows light up: the setting `rain_lights`,
    // the shader picks which, `rain_lit_windows`)
    l.rain_lights = ((rain - 0.35) / 0.45).clamp(0.0, 1.0);
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
        l.fog_color = l
            .fog_color
            .lerp(Vec3::new(0.86, 0.88, 0.92) * day, 0.4 * snow);
    }
    l.snow = snow;
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
            let path = CORONA_TEXTURES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .and_then(|t| {
                    t.ids
                        .iter()
                        .find(|(_, i)| **i == id)
                        .map(|(p, _)| p.clone())
                });
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
type CookiePictures = (
    std::collections::HashSet<u16>,
    Vec<(u16, std::path::PathBuf)>,
);
static COOKIE_PICTURES: std::sync::Mutex<Option<CookiePictures>> = std::sync::Mutex::new(None);

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
        assert_eq!(
            id,
            super::texture_id_of(dir.join("Signals/texture/Glimmer_lamp.dds"))
        );
        assert_eq!(
            super::corona_texture_id(&dir.join("Signals"), "Nowhere_lamp.bmp"),
            0
        );
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

/// Gain for white headlights on *other* vehicles in every graphics mode.
/// This is applied only to the new per-vehicle slice, never the map's street lamps.
/// In Enhanced the beam profile (or an authored cookie) lights the pavement itself,
/// so changing only the corona would leave the bright patch on the road unchanged.
fn scale_traffic_headlights(coronas: &mut [Corona], lights: &mut [PointLight], gain: f32) {
    let gain = if gain.is_finite() {
        gain.clamp(0.0, 2.0)
    } else {
        1.0
    };
    if gain == 1.0 {
        return;
    }
    for corona in coronas {
        if corona.color.iter().all(|c| *c >= 0.65) {
            corona.brightness *= gain;
        }
    }
    for light in lights {
        if !light.color.iter().all(|c| *c >= 0.65) {
            continue;
        }
        let headlamp = match light.mode {
            LightMode::Vanilla => light.d3d_spot.is_some(),
            LightMode::Enhanced => light.beam != 0.0 || light.cookie.is_some(),
            LightMode::Both => false,
        };
        if headlamp {
            light.intensity *= gain;
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
    traffic_headlights: f32,
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
    crate::scripted::push_script_lamp_glows(
        &world.scripted.lock().views,
        camera_pos,
        CORONA_RANGE,
        &mut scene.coronas,
    );
    for list in world.particle_objects.lock().values() {
        for po in list {
            if (po.pos - camera_pos).length() < 1500.0 {
                particle_sprites(&po.set, &mut scene.smoke, &mut scene.coronas);
            }
        }
    }
    if omsi_cfg::flags::OMSI_DEBUG_PARTICLES.is_set() {
        if let Some(p) = scene.smoke.first() {
            log::info!(
                "smoke: {} particles from objects, first at ({:.1}, {:.1}, {:.1}) size {:.2} alpha {:.2}",
                scene.smoke.len(),
                p.position.x,
                p.position.y,
                p.position.z,
                p.size,
                p.alpha
            );
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
    let dark = if cone.enhanced {
        night.max(cone.gloom)
    } else {
        1.0
    };
    let beams = cars_with_beams(vehicles, camera_pos, dark);
    for (k, v) in vehicles.iter().enumerate() {
        // (what is not drawn does not shine: a car out of the view and beyond the mirrors'
        // reach, or past the coronas' range, is left out whole - at night with hundreds of
        // cars about their lamps made up most of the lights and coronas of a frame)
        let d = (v.position - camera_pos).length();
        if d > CORONA_RANGE || (!v.ai_visuals && d > UNSEEN_LIGHT_RANGE) {
            continue;
        }
        let corona_start = scene.coronas.len();
        let light_start = scene.lights.len();
        vehicle_lights(
            v,
            &mut scene.coronas,
            &mut scene.lights,
            player_first && k == 0,
            &world.cookie_fades,
            beams.contains(&k),
        );
        // Include Enhanced's physically shaded road beams, not just Vanilla's D3D spots.
        // Never scale the driven bus, stationary lamps or colored brake/indicator lamps.
        if !(player_first && k == 0) {
            scale_traffic_headlights(
                &mut scene.coronas[corona_start..],
                &mut scene.lights[light_start..],
                traffic_headlights,
            );
        }
        // light_vehicles puts the player's bus first (if any). LAN guests and AI
        // traffic must not be mistaken for it when no bus is driven locally.
        let vehicle_smoke_enabled = match smoke_mode {
            VehicleSmoke::All => true,
            VehicleSmoke::PlayerOnly => player_first && k == 0,
            VehicleSmoke::Off => false,
        };
        particle_sprites_filtered(
            &v.particles,
            &mut scene.smoke,
            &mut scene.coronas,
            vehicle_smoke_enabled,
        );
        for t in &v.trailers {
            particle_sprites_filtered(
                &t.particles,
                &mut scene.smoke,
                &mut scene.coronas,
                vehicle_smoke_enabled,
            );
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
        log::info!(
            "cones: visibility {vis:.0} m, dark {night:.2}, {} cones of {} coronas",
            scene.coronas.iter().filter(|c| c.beam).count(),
            scene.coronas.len()
        );
        for c in scene.coronas.iter().filter(|c| c.beam).take(4) {
            log::info!(
                "  cone at ({:.1}, {:.1}, {:.1}) dir {:?} radius {:.2} half angles {:.0}/{:.0} deg tex {}",
                c.position.x,
                c.position.y,
                c.position.z,
                c.direction,
                c.size,
                c.inner_cos.to_degrees(),
                c.cone_cos.to_degrees(),
                c.texture
            );
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
    scene.lights.sort_by(|a, b| {
        (a.position - camera_pos)
            .length_squared()
            .total_cmp(&(b.position - camera_pos).length_squared())
    });
}

/// The player's flashlight: a hand lamp held beside the eye and aimed where the view looks,
/// a narrow bright cone with a soft edge that lights the way by night (or the inside of a dark
/// cab by day) and reaches 50 m. It shines in both pictures and puts itself first among the
/// lights, so the light grid's cells (which hold a limited number) never drop it.
pub fn flashlight(lights: &mut Vec<PointLight>, eye: DVec3, forward: Vec3, right: Vec3, up: Vec3) {
    let d = forward.normalize_or_zero();
    if d == Vec3::ZERO {
        return;
    }
    // (held a hand's width to the right and below the eye: from the eye itself the cone
    // would light the very pixels it is looked through, without the shading a torch has)
    let at = eye + (right * 0.18 - up * 0.14).as_dvec3();
    // values as a `[spotlight]` declares them: position, direction, colour, range, inner and
    // outer cone (degrees, full angles); a warm white, 50 m, 14 and 42 degrees
    let vals = [
        0.0, 0.0, 0.0, d.x, d.y, d.z, 255.0, 240.0, 215.0, 50.0, 14.0, 42.0,
    ];
    // (the classic picture's spot is Omsi.exe's Direct3D light, as bright by day as by night)
    push_classic_spot(lights, at, d, &vals, FLASHLIGHT_CLASSIC_SHARE, at, false);
    push_enhanced_spot(lights, at, d, Vec3::ZERO, &vals, FLASHLIGHT_SHARE);
}

/// The flashlight held at the camera's eye, shining along its view (`flashlight`).
pub fn flashlight_toward(scene: &mut Scene, cam: &omsi_render::Camera, dir: Vec3) {
    let right = dir.cross(Vec3::Z).normalize_or(cam.right());
    let mut torch = Vec::with_capacity(2);
    flashlight(&mut torch, cam.position, dir, right, right.cross(dir));
    scene.lights.splice(0..0, torch);
}

pub fn flashlight_ahead(scene: &mut Scene, cam: &omsi_render::Camera) {
    let mut torch = Vec::with_capacity(2);
    flashlight(
        &mut torch,
        cam.position,
        cam.forward(),
        cam.right(),
        cam.up(),
    );
    // (first of the lights: they are sorted by distance and a grid cell holds a few)
    scene.lights.splice(0..0, torch);
}

/// A flashlight's share of a headlamp pair's strength: one small lamp, not two big ones.
const FLASHLIGHT_SHARE: f32 = 0.45;

/// The same lamp's share in the classic picture, whose spot is attenuated by its range (a
/// 50 m range keeps half of the light at 10 m).
const FLASHLIGHT_CLASSIC_SHARE: f32 = 1.0;

/// How far Omsi.exe moves every `[smoke]` and `[particle_emitter]` puff towards the eye in
/// depth (m): 0x5a1d54 gives each particle 0.1, which 0x5a2b5c takes off the view depth it
/// projects the puff's depth at.
const SMOKE_Z_OFFSET: f32 = 0.1;

/// The particles of a particle set as the renderer draws them: smoke blended over the scene,
/// and the glowing ones (`--PS_emissive--`: sparks, rockets, a flame) as coronas.
pub fn particle_sprites(
    set: &omsi_sim::particles::ParticleSet,
    smoke: &mut Vec<omsi_render::SmokeParticle>,
    coronas: &mut Vec<Corona>,
) {
    particle_sprites_filtered(set, smoke, coronas, true);
}

/// Keep glowing effects even when a vehicle's non-emissive smoke is hidden.
fn particle_sprites_filtered(
    set: &omsi_sim::particles::ParticleSet,
    smoke: &mut Vec<omsi_render::SmokeParticle>,
    coronas: &mut Vec<Corona>,
    smoke_enabled: bool,
) {
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
    let t = g.get_or_insert_with(|| CoronaTextures {
        ids: Default::default(),
        listed: Vec::new(),
        root: None,
    });
    t.root = Some(root.to_path_buf());
    // (the game's own pictures are looked for in the new folder)
    for id in [&CONE_ID, &GLOW_ID, &STAR_ID] {
        id.store(UNKNOWN_ID, std::sync::atomic::Ordering::Relaxed);
    }
}

fn texture_id_of(path: std::path::PathBuf) -> u16 {
    let mut g = CORONA_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    let t = g.get_or_insert_with(|| CoronaTextures {
        ids: Default::default(),
        listed: Vec::new(),
        root: None,
    });
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
    type Known =
        std::collections::HashMap<std::path::PathBuf, std::collections::HashMap<String, u16>>;
    static KNOWN: std::sync::Mutex<Option<Known>> = std::sync::Mutex::new(None);
    if let Some(&id) = KNOWN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(model_dir))
        .and_then(|m| m.get(name))
    {
        return id;
    }
    let root = CORONA_TEXTURES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|t| t.root.clone())
        .unwrap_or_default();
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
    let root = CORONA_TEXTURES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|t| t.root.clone())
        .unwrap_or_default();
    let p = omsi_cfg::resolve_path(&root, &format!("Texture/{name}"));
    let id = if omsi_cfg::vfs::is_file(&p) {
        texture_id_of(p)
    } else {
        0
    };
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
    let cookies = COOKIE_PICTURES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|c| c.1.get(cookies_taken..).unwrap_or_default().to_vec())
        .unwrap_or_default();
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
        ConeWeather {
            fog_visibility_m,
            night: night.clamp(0.0, 1.0),
            gloom: 0.0,
            enhanced: false,
        }
    }

    /// [`ConeWeather::new`] for the weather `w`, whose clouds and rain darken the day.
    pub fn of(w: &omsi_content::weather::Weather, night: f32) -> ConeWeather {
        let (kind, rate) = omsi_weather::conditions::precip_of(w);
        let overcast =
            (omsi_content::weather_choice::cover_of(w).clamp(0.0, 1.0) - 0.45).max(0.0) / 0.55;
        let rain = if kind != 0 { rate.clamp(0.0, 1.0) } else { 0.0 };
        ConeWeather {
            gloom: gloom_darkness(overcast, rain),
            ..ConeWeather::new(w.fog.0, night)
        }
    }

    /// The visibility (no fog: a million metres) and the dark.
    fn get(self) -> (f32, f32) {
        let vis = self.fog_visibility_m;
        (if vis > 0.0 { vis } else { 1.0e6 }, self.night)
    }
}

pub use omsi_weather::conditions::vehicle_velocity;

#[cfg(test)]
mod traffic_headlight_brightness_tests {
    use super::*;

    #[test]
    fn traffic_brightness_scales_classic_and_enhanced_beams_but_not_other_lamps() {
        let mut coronas = [
            Corona {
                color: [1.0, 0.95, 0.90],
                brightness: 1.0,
                ..Default::default()
            },
            Corona {
                color: [1.0, 0.2, 0.1],
                brightness: 1.0,
                ..Default::default()
            },
        ];
        let mut lights = [
            PointLight {
                color: [1.0; 3],
                mode: LightMode::Vanilla,
                d3d_spot: Some(omsi_render::D3dSpot {
                    range: 100.0,
                    source: DVec3::ZERO,
                    player: false,
                }),
                intensity: 3.0,
                ..Default::default()
            },
            PointLight {
                color: [1.0; 3],
                mode: LightMode::Enhanced,
                beam: 1.0,
                intensity: 10.0,
                ..Default::default()
            },
            PointLight {
                color: [1.0, 0.2, 0.1],
                mode: LightMode::Enhanced,
                beam: 1.0,
                intensity: 7.0,
                ..Default::default()
            },
            PointLight {
                color: [1.0; 3],
                mode: LightMode::Enhanced,
                intensity: 9.0,
                ..Default::default()
            },
        ];
        scale_traffic_headlights(&mut coronas, &mut lights, 0.25);
        assert_eq!([coronas[0].brightness, coronas[1].brightness], [0.25, 1.0]);
        assert_eq!(lights.map(|l| l.intensity), [0.75, 2.5, 7.0, 9.0]);
    }

    #[test]
    fn traffic_brightness_zero_disables_road_beams_and_invalid_gain_is_neutral() {
        let mut beam = [PointLight {
            color: [1.0; 3],
            mode: LightMode::Enhanced,
            beam: -1.0,
            intensity: 10.0,
            ..Default::default()
        }];
        scale_traffic_headlights(&mut [], &mut beam, f32::NAN);
        assert_eq!(beam[0].intensity, 10.0);
        scale_traffic_headlights(&mut [], &mut beam, 0.0);
        assert_eq!(beam[0].intensity, 0.0);
    }
}
