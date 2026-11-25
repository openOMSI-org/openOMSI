//! The weather's state between the frames: what falls (`precip_of`), how wet the roads
//! are, the clouds' drift with the wind, and how fast a vehicle meets the air.

use omsi_content::weather::Weather;
use std::path::Path;

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
/// the way the original's do. The enhanced clouds take the same direction: they go with
/// the air the smoke and the rain below them move in (`wind_vector`; the sky samples at
/// the place plus the drift, so the clouds move against it).
pub fn cloud_drift_step(d: &mut [f64; 4], w: &Weather, secs: f64) {
    let omsi_dir = w.wind.0 as f64 * std::f64::consts::PI / 200.0;
    let speed = w.wind.1 as f64;
    let s = secs * speed / 2500.0;
    d[0] = (d[0] + omsi_dir.sin() * s).rem_euclid(CLOUD_DRIFT_WRAP);
    d[1] = (d[1] + omsi_dir.cos() * s).rem_euclid(CLOUD_DRIFT_WRAP);
    let period = CLOUD_PERIOD as f64;
    d[2] = (d[2] + omsi_dir.sin() * speed * secs).rem_euclid(period);
    d[3] = (d[3] + omsi_dir.cos() * speed * secs).rem_euclid(period);
}

/// The air's velocity (m/s, world: x east, y north, z up) in this weather, as Omsi.exe
/// works it out every frame (`TWeather.Calculate`, 0x753428, its `ActualWindVec`):
/// `-speed * (sin, cos)` of the direction times pi/200. The wind blows *from* the
/// direction given, as the METAR reader at 0x63ce40 stores it (degrees), but Omsi.exe
/// turns it with pi/200 - as if it were in gradians - so a 90 deg wind comes from 81 deg,
/// and 360 deg from 324. The smoke (0x5a238c), the rain and snow (`precipVec`, the fall
/// plus this, 0x755798) and the classic clouds (see `cloud_drift_step`) all move with it,
/// so everything here that the air carries takes this one vector.
pub fn wind_vector(w: &Weather) -> glam::Vec3 {
    let (dir, speed) = w.wind;
    let a = dir * std::f32::consts::PI / 200.0;
    -speed * glam::Vec3::new(a.sin(), a.cos(), 0.0)
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

/// What of this weather Omsi.exe's daylight takes in (the street lamps, the AI's lights,
/// the `[NightMapMode]` night textures, `Envir_Brightness`): its `[fog]`, what falls, and
/// its clouds - `overcast` is whether the clouds.cfg type it names is an `ovc` deck, `None`
/// when it names none (`-1`) or one clouds.cfg lacks. Clouds that are only drawn
/// (`decorative_clouds`) are no clouds here.
pub fn sky_weather(w: &Weather, overcast: Option<bool>) -> omsi_sim::daylight::SkyWeather {
    let kind = w.precip.first().copied().unwrap_or(0.0) as i32;
    omsi_sim::daylight::SkyWeather {
        visibility: w.fog.0,
        fog_density: w.fog.1,
        precip_kind: kind,
        precip_strength: w.precip.get(1).copied().unwrap_or(0.0).clamp(0.0, 255.0),
        clouds: overcast.filter(|_| !w.decorative_clouds && !w.clouds.0.trim().starts_with("-1")),
    }
}

/// [`sky_weather`] with the cloud type looked up in the game's `Weather\clouds.cfg` (see
/// [`overcast`]): what the weather says, whatever the renderer drew of it.
pub fn daylight_weather(w: &Weather) -> omsi_sim::daylight::SkyWeather {
    let ovc = if w.decorative_clouds {
        None
    } else {
        overcast(&w.clouds.0)
    };
    sky_weather(w, ovc)
}

/// A `[cloudtype]` of `Weather/clouds.cfg`: its texture (under `Texture\`), the metres of
/// ground a tile of it covers, and whether it is an `ovc` deck (else `sct`).
#[derive(Debug, Clone, PartialEq)]
pub struct CloudType {
    pub texture: String,
    pub size: f32,
    pub overcast: bool,
}

/// The `[cloudtype]` entry named `kind` (`-1` or empty: none) of `root`'s clouds.cfg, read
/// as Omsi.exe reads it (0x754664): name, texture, size, then `ovc` or `sct`.
pub fn cloud_type(root: &Path, kind: &str) -> Option<CloudType> {
    if kind.is_empty() || kind.starts_with("-1") {
        return None;
    }
    let cfg = omsi_cfg::vfs::read(&root.join("Weather").join("clouds.cfg")).ok()?;
    cloud_type_in(&omsi_cfg::codepage::decode(&cfg), kind)
}

/// The `[cloudtype]` named `kind` in the text of a clouds.cfg.
pub fn cloud_type_in(text: &str, kind: &str) -> Option<CloudType> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    (0..lines.len()).find_map(|i| {
        if !lines[i].eq_ignore_ascii_case("[cloudtype]")
            || !lines
                .get(i + 1)
                .is_some_and(|n| n.eq_ignore_ascii_case(kind))
        {
            return None;
        }
        Some(CloudType {
            texture: lines.get(i + 2)?.to_string(),
            size: lines
                .get(i + 3)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| *v > 0.0)
                .unwrap_or(2000.0),
            overcast: lines
                .get(i + 4)
                .is_some_and(|v| v.eq_ignore_ascii_case("ovc")),
        })
    })
}

/// Whether the cloud type `kind` is an `ovc` deck (`None`: no clouds, or a type clouds.cfg
/// lacks), from the `Weather\clouds.cfg` of the first content root that has one. Each type
/// is read once while the content stays the same: the daylight asks every frame.
pub fn overcast(kind: &str) -> Option<bool> {
    type Seen = (u64, Vec<(String, Option<bool>)>);
    static SEEN: std::sync::Mutex<Option<Seen>> = std::sync::Mutex::new(None);
    let kind = kind.trim();
    if kind.is_empty() || kind.starts_with("-1") {
        return None;
    }
    let generation = omsi_cfg::content_generation();
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.as_ref().is_none_or(|(g, _)| *g != generation) {
        *seen = Some((generation, Vec::new()));
    }
    let (_, known) = seen.as_mut()?;
    if let Some((_, o)) = known.iter().find(|(k, _)| k.eq_ignore_ascii_case(kind)) {
        return *o;
    }
    let o = omsi_cfg::find_in_roots("Weather\\clouds.cfg")
        .and_then(|(_, path)| omsi_cfg::vfs::read(&path).ok())
        .and_then(|cfg| cloud_type_in(&omsi_cfg::codepage::decode(&cfg), kind))
        .map(|t| t.overcast);
    known.push((kind.to_string(), o));
    o
}

/// The weather's precipitation strength (0..1, of its 255) whatever falls, or nothing: the
/// scripts' `PrecipRate` system variable, which Omsi.exe sets every frame to the
/// strength / 255 alone (0x6ff020) - a `#CAVOK` weather's `0` / `32` reads 0.125 there,
/// where `precip_of` says nothing falls.
pub fn precip_strength(w: &Weather) -> f32 {
    (w.precip.get(1).copied().unwrap_or(0.0) / 255.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod strength_tests {
    use super::*;

    #[test]
    fn the_strength_reads_whatever_falls() {
        let w = |precip: Vec<f32>| Weather {
            precip,
            ..Weather::default()
        };
        // #CAVOK's "0 / 32": nothing falls, yet the scripts read 32 / 255
        let cavok = w(vec![0.0, 32.0]);
        assert_eq!(precip_of(&cavok), (0, 0.0));
        assert_eq!(precip_strength(&cavok), 32.0 / 255.0);
        assert_eq!(precip_strength(&w(vec![1.0, 255.0])), 1.0);
        assert_eq!(precip_strength(&w(vec![2.0])), 0.0);
        assert_eq!(precip_strength(&w(vec![])), 0.0);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_types_are_read_as_omsi_reads_them() {
        let cfg = "\tHier werden alle Wolkentypen definiert:\n\n\t[cloudtype]\n\tname\n\ttexture\n\n[cloudtype]\nCumulus 2\nCumulus_2.tga\n2000\nsct\n\n[cloudtype]\nOvercast 1\nOVC_1.bmp\n1000\novc\n";
        assert_eq!(
            cloud_type_in(cfg, "cumulus 2"),
            Some(CloudType {
                texture: "Cumulus_2.tga".into(),
                size: 2000.0,
                overcast: false
            })
        );
        assert_eq!(
            cloud_type_in(cfg, "Overcast 1"),
            Some(CloudType {
                texture: "OVC_1.bmp".into(),
                size: 1000.0,
                overcast: true
            })
        );
        assert_eq!(cloud_type_in(cfg, "Cumulus 9"), None);
    }

    #[test]
    fn the_daylight_sees_the_weathers_clouds_not_the_drawn_ones() {
        let mut w = Weather {
            fog: (8000.0, 0.9),
            clouds: ("Overcast 1".into(), 300.0),
            precip: vec![1.0, 120.0, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        let s = sky_weather(&w, Some(true));
        assert_eq!(s.clouds, Some(true));
        assert_eq!((s.visibility, s.fog_density), (8000.0, 0.9));
        assert_eq!((s.precip_kind, s.precip_strength), (1, 120.0));
        // no clouds named, or a clouds.cfg without the type: a clear sky
        assert_eq!(sky_weather(&w, None).clouds, None);
        w.clouds.0 = "-1".into();
        assert_eq!(sky_weather(&w, Some(false)).clouds, None);
        // openOMSI's fair-weather cumulus in a sky without a weather are only drawn
        w.clouds.0 = "Cumulus 1".into();
        w.decorative_clouds = true;
        assert_eq!(sky_weather(&w, Some(false)).clouds, None);
        assert_eq!(daylight_weather(&w).clouds, None);
        // and a sky without clouds needs no clouds.cfg
        assert_eq!(overcast("-1"), None);
        assert_eq!(overcast(""), None);
    }

    fn windy(dir: f32, speed: f32) -> Weather {
        Weather {
            wind: (dir, speed),
            ..Weather::default()
        }
    }

    #[test]
    fn the_wind_blows_from_its_direction_turned_as_omsi_does() {
        // from the north: the air moves south
        let v = wind_vector(&windy(0.0, 4.0));
        assert!(v.abs_diff_eq(glam::Vec3::new(0.0, -4.0, 0.0), 1e-5), "{v}");
        // 100 is a right angle to Omsi.exe (pi/200): from the east, the air moves west
        let v = wind_vector(&windy(100.0, 2.0));
        assert!(v.abs_diff_eq(glam::Vec3::new(-2.0, 0.0, 0.0), 1e-5), "{v}");
        // 90 comes from 81 deg: the air moves a little south of west
        let v = wind_vector(&windy(90.0, 1.0));
        let a = 90.0f32 * std::f32::consts::PI / 200.0;
        assert!(
            v.abs_diff_eq(glam::Vec3::new(-a.sin(), -a.cos(), 0.0), 1e-6),
            "{v}"
        );
        assert!(v.y < 0.0);
        // a calm moves nothing, and the vector is level
        assert_eq!(wind_vector(&windy(123.0, 0.0)), glam::Vec3::ZERO);
        assert_eq!(wind_vector(&windy(57.0, 9.0)).z, 0.0);
    }

    #[test]
    fn both_skies_drift_along_omsis_angle() {
        // 100 taken as pi/2: the drift grows east only, 50 m in 10 s at 5 m/s
        let d = cloud_drift_at(&windy(100.0, 5.0), 10.0);
        assert!(
            (d[0] - 50.0 / 2500.0).abs() < 1e-9 && d[1].abs() < 1e-9,
            "{d:?}"
        );
        assert!((d[2] - 50.0).abs() < 1e-6 && d[3].abs() < 1e-6, "{d:?}");
        // the drift runs against the air: the sky samples at the place plus the drift
        let v = wind_vector(&windy(100.0, 5.0));
        assert!(v.x < 0.0 && d[0] > 0.0);
    }
}
