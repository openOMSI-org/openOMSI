//! The weather's state between the frames: what falls (`precip_of`), how wet the roads
//! are, the clouds' drift with the wind, and how fast a vehicle meets the air.

use omsi_content::weather::Weather;

/// The classic sky's cloud tiling (m): the same as `omsi_render::VANILLA_CLOUD_PERIOD`, which
/// this crate cannot see (a test in omsi-app holds the two together).
pub const CLOUD_PERIOD: f32 = 10000.0;

/// The enhanced clouds' drift is kept modulo this many 2500 m tiles: a whole number of
/// periods of every texture the clouds are read from (the shape map, 13 km: 26 tiles; the
/// cover field, 14 km x 1 / 0.35 = 40 km: 16 tiles; the billows, 420 m at 1.3 times the
/// drift: 42 tiles), so the wrap moves none of them. Kept modulo one tile, as it was, the
/// whole sky jumped by 2500 m whenever the drift went round - every eight minutes or so in a
/// 5 m/s wind - and the sky cube blended the old clouds and the new for seconds after. The
/// wrap is 10 920 km of wind now; the high layer's noise (not periodic) jumps once in that.
pub const CLOUD_DRIFT_WRAP: f64 = 4368.0;

/// The clouds' drift after `time` seconds of a steady wind (see `cloud_drift_step`).
pub fn cloud_drift_at(w: &Weather, time: f64) -> [f64; 4] {
    let mut d = [0.0; 4];
    cloud_drift_step(&mut d, w, time);
    d
}

/// Move the clouds on by `secs` of [wind] direction (deg) speed (m/s), in 2500 m tiles,
/// modulo `CLOUD_DRIFT_WRAP` (in f64: a frame's step at 240 frames a second is a few
/// millionths of a tile, which an f32 drops near the wrap). (Taken
/// from the absolute time, every change of the wind while a weather blends in moved the
/// whole sky by time x change.) `d[2..4]`: the classic sky's clouds, in metres (modulo
/// `VANILLA_CLOUD_PERIOD`) as Omsi.exe moves them (0x753428) - by the wind's speed along
/// its direction taken as degrees x pi/200, its slip from pi/180 kept, so that they drift
/// the way the original's do.
pub fn cloud_drift_step(d: &mut [f64; 4], w: &Weather, secs: f64) {
    let (dir, speed) = (w.wind.0.to_radians() as f64, w.wind.1 as f64);
    let s = secs * speed / 2500.0;
    d[0] = (d[0] + dir.sin() * s).rem_euclid(CLOUD_DRIFT_WRAP);
    d[1] = (d[1] + dir.cos() * s).rem_euclid(CLOUD_DRIFT_WRAP);
    let omsi_dir = w.wind.0 as f64 * std::f64::consts::PI / 200.0;
    let period = CLOUD_PERIOD as f64;
    d[2] = (d[2] + omsi_dir.sin() * speed * secs).rem_euclid(period);
    d[3] = (d[3] + omsi_dir.cos() * speed * secs).rem_euclid(period);
}

/// How wet the roads are: rain soaks them in a few minutes, sunshine dries them in about
/// twenty. `secs` is how long this weather has been running.
pub fn road_wetness(rate: f32, secs: f64, start: f32) -> f32 {
    if rate > 0.0 {
        (start + secs as f32 * rate / 180.0).clamp(0.0, 1.0)
    } else {
        (start - secs as f32 / 1200.0).clamp(0.0, 1.0)
    }
}

/// Push the weather into a vehicle's script host.
pub fn precip_of(w: &Weather) -> (i32, f32) {
    let kind = w.precip.first().copied().unwrap_or(0.0) as i32;
    let rate = if kind == 0 {
        0.0
    } else {
        (w.precip.get(1).copied().unwrap_or(0.0) / 255.0).clamp(0.0, 1.0)
    };
    (kind, rate)
}

/// What of this weather Omsi.exe's daylight takes in (the street lamps, `Envir_Brightness`):
/// its `[fog]`, what falls, and its clouds - `overcast` is whether the clouds.cfg type it
/// names is an `ovc` deck, `None` when it names none (`-1`) or one clouds.cfg lacks.
pub fn sky_weather(w: &Weather, overcast: Option<bool>) -> omsi_sim::daylight::SkyWeather {
    let kind = w.precip.first().copied().unwrap_or(0.0) as i32;
    omsi_sim::daylight::SkyWeather {
        visibility: w.fog.0,
        fog_density: w.fog.1,
        precip_kind: kind,
        precip_strength: w.precip.get(1).copied().unwrap_or(0.0).clamp(0.0, 255.0),
        clouds: overcast.filter(|_| !w.clouds.0.trim().starts_with("-1")),
    }
}

/// The road under the wheels as the scripts and the sound configurations read it
/// (`StreetCond`): 0 dry, 1 wet, 2 covered in snow. A snowfall (or a weather with
/// `[snowOnRoad]`) puts it into the upper half, everything else follows the water film.
pub fn street_condition(w: &Weather, wetness: f32) -> f32 {
    let wet = wetness.clamp(0.0, 1.0);
    if w.snow_on_road || precip_of(w).0 == 2 {
        1.0 + wet
    } else {
        wet
    }
}

/// How wet the roads are when a session starts: what this weather has already left on them
/// (`[groundwet]`, 0 … 255), and at least what the rain falling now would soak them to.
pub fn initial_wetness(w: &Weather) -> f32 {
    let rate = precip_of(w).1;
    let falling = if rate > 0.0 {
        (0.4 + rate).min(1.0)
    } else {
        0.0
    };
    (w.ground_wet[0] / 255.0).clamp(0.0, 1.0).max(falling)
}

/// A vehicle's velocity (m/s, world) from its heading and speed: the airstream its glass
/// meets (`Lighting::glass_wind`).
pub fn vehicle_velocity(v: &omsi_sim::VehicleInstance) -> glam::Vec3 {
    let h = v.heading.to_radians();
    glam::Vec3::new(h.sin() as f32, h.cos() as f32, 0.0) * v.physics.speed
}
