//! Sun position and the three envir.cfg lights (direct sun A, from above B, ambient C)
//! interpolated over the sun altitude (unit `mc_himmel`).

use crate::clock::SimClock;
use glam::Vec3;
use omsi_content::Envir;

#[derive(Debug, Clone, Copy)]
pub struct Daylight {
    /// Unit vector towards the sun (x east, y north, z up).
    pub sun_dir: Vec3,
    pub altitude_deg: f32,
    pub sun_color: Vec3,
    pub secondary: Vec3,
    pub ambient: Vec3,
    pub sky: Vec3,
    /// 0 by day, 1 by night (nightmaps, coronas).
    pub night: f32,
    /// Street lights on (the `NightlightA` scenery variable): [`Daylight::daylight_sum`]
    /// below 0.6 (0x6ff1bc).
    pub lamps_on: bool,
    /// The daylight by the sun's altitude alone (1 day … 0 night, a ramp from +6 to -6
    /// degrees). Nothing Omsi.exe switches goes by it: see [`Daylight::daylight_sum`].
    pub brightness: f32,
    /// Omsi.exe's daylight sum under the weather (0..1, weather +0xb0, see
    /// [`SkyWeather::daylight_sum`]): what it switches its lights by - the street lamps
    /// below 0.6 (0x6ff1bc), a scenery object's `NightlightA` and night textures below its
    /// `[NightMapMode]` threshold (0x6119d8), the AI's lights below 0.75 (0x7d9128).
    pub daylight_sum: f32,
    /// Omsi.exe's daylight sum with the sun's own share (0..1, weather +0xac): the day's
    /// part of the `Envir_Brightness` vehicle variable, see [`Daylight::envir_brightness`].
    pub sky_light: f32,
    /// Sun azimuth in radians, clockwise from north.
    pub azimuth_rad: f32,
    /// Blend of the envir sky textures: day, twilight (peaks at sunrise), night.
    pub sky_weights: [f32; 3],
    /// envir.cfg's three light colours relative to the stock file's at this sun altitude
    /// (A, B, C; 1 = stock): the enhanced renderer tints its physical sky with them.
    pub envir_tint: [Vec3; 3],
    /// Unit vector towards the moon (x east, y north, z up).
    pub moon_dir: Vec3,
    /// How much of the moon's disc is lit (0 new moon .. 1 full moon).
    pub moon_illum: f32,
    /// The day of the year (1..366) and the map's latitude (degrees north): the season.
    pub day_of_year: f32,
    pub lat_deg: f32,
    /// One number per calendar day: what the enhanced renderer draws the day's own air
    /// from (how clear or hazy it is), so that no two days look quite alike.
    pub day_seed: u32,
}

/// Days since 2000-01-01 12:00 UTC (J2000) for the map clock at `place`.
fn days_j2000(clock: &SimClock, place: &SunPlace) -> f64 {
    let y = clock.year;
    // whole days from 2000-01-01 to the start of this year
    let mut days = 0i64;
    if y >= 2000 {
        for yy in 2000..y {
            days += crate::clock::days_in_year(yy) as i64;
        }
    } else {
        for yy in y..2000 {
            days -= crate::clock::days_in_year(yy) as i64;
        }
    }
    let utc_hours = clock.time / 3600.0 - place.timezone - place.dst_hours(clock);
    days as f64 + (clock.day_of_year - 1) as f64 + utc_hours / 24.0 - 0.5
}

/// The moon's altitude, azimuth (degrees; azimuth clockwise from north) and lit fraction
/// for the map clock at `place`: a low-precision lunar theory (the main terms of the
/// longitude and latitude, about half a degree), which is all a sky needs.
pub fn moon_position(clock: &SimClock, place: &SunPlace) -> (f64, f64, f64) {
    let d = days_j2000(clock, place);
    let rad = |x: f64| x.to_radians();
    let l = 218.316 + 13.176_396 * d;
    let m = 134.963 + 13.064_993 * d;
    let f = 93.272 + 13.229_350 * d;
    let lon = rad(l + 6.289 * rad(m).sin());
    let lat = rad(5.128 * rad(f).sin());
    let eps = rad(23.439);
    let ra = (lon.sin() * eps.cos() - lat.tan() * eps.sin()).atan2(lon.cos());
    let dec = (lat.sin() * eps.cos() + lat.cos() * eps.sin() * lon.sin()).asin();
    let gmst = 280.460_618_37 + 360.985_647_366_29 * d;
    let ha = rad(gmst + place.lon_deg) - ra;
    let phi = rad(place.lat_deg);
    let sin_alt = phi.sin() * dec.sin() + phi.cos() * dec.cos() * ha.cos();
    let alt = sin_alt.clamp(-1.0, 1.0).asin();
    let az = (-ha.sin()).atan2(dec.tan() * phi.cos() - phi.sin() * ha.cos());
    // the sun's ecliptic longitude, for the phase
    let g = rad(357.529 + 0.985_600_28 * d);
    let sun_lon =
        rad(280.459 + 0.985_647_36 * d) + rad(1.915) * g.sin() + rad(0.020) * (2.0 * g).sin();
    let elong = (lat.cos() * (lon - sun_lon).cos()).clamp(-1.0, 1.0).acos();
    let illum = (1.0 - elong.cos()) * 0.5;
    (alt.to_degrees(), az.to_degrees().rem_euclid(360.0), illum)
}

/// Where the map lies and how its clocks run, from its `timezone.txt` (OMSI
/// the original: `[timezone]` hours east of UTC, `[location]` latitude and longitude,
/// `[DST]` periods). Without the file the exe keeps Spandau: 52.505 N, 13.2782 E, UTC+1.
#[derive(Debug, Clone, PartialEq)]
pub struct SunPlace {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub timezone: f64,
    /// Daylight saving periods: first and last day (`YYYYMMDD`), the hour it starts on the
    /// first day and ends on the last, and the hours the clocks go forward.
    pub dst: Vec<(i32, i32, f32, f32, f32)>,
}

impl SunPlace {
    pub fn locate(&mut self, timezone: f64, lat_lon: Option<(f64, f64)>) {
        self.timezone = timezone;
        match lat_lon {
            Some((lat, lon)) => (self.lat_deg, self.lon_deg) = (lat, lon),
            None if timezone != SunPlace::default().timezone => self.lon_deg = timezone * 15.0,
            None => {}
        }
    }
}

impl Default for SunPlace {
    fn default() -> Self {
        SunPlace {
            lat_deg: 52.505,
            lon_deg: 13.2782,
            timezone: 1.0,
            dst: Vec::new(),
        }
    }
}

impl SunPlace {
    /// The hours the clock is ahead of standard time at this moment (0 outside summer time).
    pub fn dst_hours(&self, clock: &SimClock) -> f64 {
        let date = clock.date_code();
        let hour = clock.time / 3600.0;
        for &(start, end, h0, h1, offset) in &self.dst {
            let after_start = date > start || (date == start && hour >= h0 as f64);
            let before_end = date < end || (date == end && hour < h1 as f64);
            if after_start && before_end {
                return offset as f64;
            }
        }
        0.0
    }

    /// Local solar time (hours) for the map clock: the clock minus its time zone and summer
    /// time is UTC, and the sun stands south at 12:00 plus 4 minutes a degree west.
    pub fn solar_hours(&self, clock: &SimClock) -> f64 {
        clock.time / 3600.0 - self.timezone - self.dst_hours(clock) + self.lon_deg / 15.0
    }
}

static PLACE: std::sync::RwLock<Option<std::sync::Arc<SunPlace>>> = std::sync::RwLock::new(None);

/// The loaded map's place (see [`SunPlace`]); every sun position is computed for it.
pub fn set_place(p: SunPlace) {
    *PLACE.write().unwrap_or_else(|e| e.into_inner()) = Some(std::sync::Arc::new(p));
}

pub fn place() -> std::sync::Arc<SunPlace> {
    PLACE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// The obliquity of the ecliptic Omsi.exe works with (radians, 23.44 degrees).
const OMSI_OBLIQUITY: f64 = 0.409_087_723_374_950_9;

/// Sun altitude and azimuth (degrees; azimuth clockwise from north) for the map clock at
/// `place`, placed as Omsi.exe places its sun (THimmel 0x5d8ad0 with TGestirn 0x5d8498),
/// which is what the `SunAlt` script variable and the envir.cfg lights follow:
///
/// - the sidereal time at Greenwich is 6.69 h plus 24 h for every 365 days of the year
///   (the whole day times 24 divided by 365) plus the clock's UTC hours (the clock less the
///   map's time zone and summer time), and the map's longitude makes it local;
/// - the sun runs evenly round the ecliptic from the equinox on day 80; its right ascension
///   is taken from that longitude over the tilted equator, so the sun stands south up to ten
///   minutes before or after noon over the year (in early May at about 11:53);
/// - its declination is the sine of the ecliptic's tilt times the sine of that longitude,
///   used as an angle in radians: 22.8 degrees at the solstices, not 23.4, so the midsummer
///   sun stands about 0.65 degrees lower than the real one and the midwinter sun higher.
pub fn sun_position(clock: &SimClock, place: &SunPlace) -> (f64, f64) {
    use std::f64::consts::PI;
    let doy = clock.day_of_year;
    let gmst_hours = (doy * 24) as f64 / 365.0 + 6.69 + clock.time / 3600.0
        - place.timezone
        - place.dst_hours(clock);
    let local_sidereal = gmst_hours * 2.0 * PI / 24.0 + place.lon_deg.to_radians();
    let ecliptic = ((doy - 80) * 2) as f64 * PI / 365.0;
    let right_ascension = (OMSI_OBLIQUITY.cos() * ecliptic.sin()).atan2(ecliptic.cos());
    let decl = OMSI_OBLIQUITY.sin() * ecliptic.sin();
    let h = local_sidereal - right_ascension;
    let lat = place.lat_deg.to_radians();
    let sin_alt = decl.sin() * lat.sin() + decl.cos() * h.cos() * lat.cos();
    let alt = sin_alt.clamp(-1.0, 1.0).asin();
    // (Omsi.exe's azimuth counts from the south towards the west; from the north here)
    let az_south = h.sin().atan2(h.cos() * lat.sin() - decl.tan() * lat.cos());
    (
        alt.to_degrees(),
        (az_south.to_degrees() + 180.0).rem_euclid(360.0),
    )
}

/// The blend of the envir.cfg sky pictures (day, twilight, night) at a sun height of `alt`
/// degrees, as Omsi.exe draws its dome (0x5d8e98): above the end of the twilight the day
/// picture, below its start the night one, and between them the twilight picture blended
/// towards the day (above the horizon) or the night (below it) by a weight in 256ths of
/// the sun's height over the threshold. Omsi.exe turns the envir.cfg thresholds into
/// radians by pi/200 where its light colours take pi/180, so its sky reaches the day at
/// 0.9 of the twilight's end (9 degrees for the stock 10) and the night at 0.9 of its start.
pub fn omsi_sky_weights(alt: f32, twilight: (f32, f32)) -> [f32; 3] {
    let (start, end) = (twilight.0 * 0.9, twilight.1 * 0.9);
    if alt > end {
        [1.0, 0.0, 0.0]
    } else if alt > 0.0 && end > 0.0 {
        let w = ((alt * 256.0 / end).round()).min(255.0) / 255.0;
        [w, 1.0 - w, 0.0]
    } else if alt >= start && start < 0.0 {
        let w = ((alt * 256.0 / start).round()).min(255.0) / 255.0;
        [0.0, 1.0 - w, w]
    } else if alt > 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn stops(colors: &[[f32; 3]; 5], twilight: (f32, f32), alt: f32) -> Vec3 {
    let alts = [-90.0, twilight.0, 0.0, twilight.1, 90.0];
    let mut i = 0;
    while i + 1 < 4 && alt > alts[i + 1] {
        i += 1;
    }
    let t = ((alt - alts[i]) / (alts[i + 1] - alts[i]).max(1e-3)).clamp(0.0, 1.0);
    let a = Vec3::from(colors[i]);
    let b = Vec3::from(colors[i + 1]);
    a.lerp(b, t) / 255.0
}

impl Daylight {
    pub fn compute(clock: &SimClock, envir: Option<&Envir>) -> Daylight {
        let (alt, az) = sun_position(clock, &place());
        let (sa, ca) = (alt.to_radians().sin() as f32, alt.to_radians().cos() as f32);
        let (saz, caz) = (az.to_radians().sin() as f32, az.to_radians().cos() as f32);
        let sun_dir = Vec3::new(ca * saz, ca * caz, sa).normalize_or_zero();
        let alt = alt as f32;
        let default_envir;
        let envir = match envir {
            Some(e) => e,
            None => {
                default_envir = default_envir_values();
                &default_envir
            }
        };
        let tw = envir.twilight_start_end;
        let a = stops(&envir.light_color_a, tw, alt);
        let b = stops(&envir.light_color_b, tw, alt);
        let c = stops(&envir.light_color_c, tw, alt);
        // scaled so that a sunlit white surface does not blow out
        let sun_color = a * 0.7;
        let secondary = b * 0.5;
        let ambient = (c * 0.45).max(Vec3::splat(0.05));
        // the sky follows the light from above; deep blue at day, near black at night
        let day = ((alt + 6.0) / 16.0).clamp(0.0, 1.0);
        let sky = Vec3::new(0.55, 0.70, 0.92) * day
            + Vec3::new(0.01, 0.01, 0.03) * (1.0 - day)
            + (a * 0.15 * (1.0 - day) * day * 4.0).min(Vec3::splat(0.3));
        let night = ((tw.1 - alt) / tw.1.max(1.0)).clamp(0.0, 1.0);
        let sky_weights = omsi_sky_weights(alt, tw);
        let stock = default_envir_values();
        let ratio = |own: Vec3, base: Vec3| -> Vec3 {
            // where the stock light is (nearly) black there is nothing to compare with
            let r = |o: f32, b: f32| {
                if b > 2.0 / 255.0 {
                    (o / b).clamp(0.25, 4.0)
                } else {
                    1.0
                }
            };
            Vec3::new(r(own.x, base.x), r(own.y, base.y), r(own.z, base.z))
        };
        let envir_tint = [
            ratio(
                a,
                stops(&stock.light_color_a, stock.twilight_start_end, alt),
            ),
            ratio(
                b,
                stops(&stock.light_color_b, stock.twilight_start_end, alt),
            ),
            ratio(
                c,
                stops(&stock.light_color_c, stock.twilight_start_end, alt),
            ),
        ];
        let brightness = ((alt + 6.0) / 12.0).clamp(0.0, 1.0);
        // Omsi.exe's daylight under the weather (+0xb0), which its lamps, night textures and
        // AI lights switch by (see `daylight_sum`): under clouds by the clouds' share of the
        // sun, else by the sun's own
        let weather = sky_weather();
        let sun = omsi_sun_share(alt);
        let daylight_sum = weather.daylight_sum(a, b, c, weather.cloud_share().unwrap_or(sun));
        let sky_light = weather.daylight_sum(a, b, c, sun);
        let here = place();
        let (m_alt, m_az, moon_illum) = moon_position(clock, &here);
        let (ma, mz) = (m_alt.to_radians(), m_az.to_radians());
        let moon_dir = Vec3::new(
            (ma.cos() * mz.sin()) as f32,
            (ma.cos() * mz.cos()) as f32,
            ma.sin() as f32,
        )
        .normalize_or_zero();
        let day_seed =
            (clock.year.rem_euclid(10_000) as u32) * 400 + clock.day_of_year.max(0) as u32;
        Daylight {
            sun_dir,
            altitude_deg: alt,
            sun_color,
            secondary,
            ambient,
            sky,
            night,
            lamps_on: daylight_sum < 0.6,
            brightness,
            daylight_sum,
            sky_light,
            azimuth_rad: az.to_radians() as f32,
            sky_weights,
            envir_tint,
            moon_dir,
            moon_illum: moon_illum as f32,
            day_of_year: clock.day_of_year as f32,
            lat_deg: here.lat_deg as f32,
            day_seed,
        }
    }
}

impl Daylight {
    /// The `Envir_Brightness` vehicle variable as Omsi.exe sets it for every road vehicle
    /// (0x7d8735): the mean of the light the tile's light map throws on the vehicle (its
    /// colour at the vehicle's place while the lamps are on, 0x61378c) plus the mean of
    /// light A, at most 1. The stock buses scale their windows' alpha by it (`[alphascale]
    /// Envir_Brightness` on Fenster_braun.tga and the like): by the sun's ramp alone it was
    /// 0 at every night, and a bus under the street lamps had no glass at all (#624).
    pub fn envir_brightness(&self, light_map: Option<Vec3>) -> f32 {
        let lm = light_map
            .filter(|_| self.lamps_on)
            .map(|c| (c.x + c.y + c.z) / 3.0)
            .unwrap_or(0.0);
        (self.sky_light + lm.max(0.0)).clamp(0.0, 1.0)
    }
}

/// What of the weather Omsi.exe's daylight takes in (TWeather 0x753120): the `[fog]`
/// visibility (m) and density, what falls (`[precip]` kind 0 none, 1 rain, 2 snow, and its
/// strength 0..255), and the clouds: `None` for none (`-1`), else whether the clouds.cfg
/// type is an `ovc` deck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyWeather {
    pub visibility: f32,
    pub fog_density: f32,
    pub precip_kind: i32,
    pub precip_strength: f32,
    pub clouds: Option<bool>,
}

impl Default for SkyWeather {
    /// Omsi.exe's starting weather: 50 km, density 1, nothing falling, no clouds.
    fn default() -> Self {
        SkyWeather {
            visibility: 50_000.0,
            fog_density: 1.0,
            precip_kind: 0,
            precip_strength: 0.0,
            clouds: None,
        }
    }
}

static SKY_WEATHER: std::sync::RwLock<Option<SkyWeather>> = std::sync::RwLock::new(None);

/// The weather the daylight is reckoned under from now on (see [`SkyWeather`]): the
/// session's (the window's, an offscreen run's), `SkyWeather::default()` without one. A
/// look at a vehicle outside the session (the showroom) leaves it alone.
pub fn set_sky_weather(w: SkyWeather) {
    *SKY_WEATHER.write().unwrap_or_else(|e| e.into_inner()) = Some(w);
}

pub fn sky_weather() -> SkyWeather {
    SKY_WEATHER
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_default()
}

/// The sun's share of Omsi.exe's daylight at an altitude of `alt` degrees (weather +0xcc,
/// set where the clouds are drawn, 0x754e44): 0 below -0.02 rad, 1 above +0.02 rad, linear
/// between. (Under drawn clouds Omsi.exe takes it down further by the cloud texture's cover
/// at the sun, which only the renderer knows.)
pub fn omsi_sun_share(alt: f32) -> f32 {
    ((alt.to_radians() + 0.02) / 0.04).clamp(0.0, 1.0)
}

impl SkyWeather {
    /// The visibility and the fog density Omsi.exe lights by (0x753120): rain shortens the
    /// visibility to 100 km / strength, snow to 20 km / strength; snow makes the fog at
    /// least 0.8 dense, an `ovc` deck at most 0.7.
    pub fn effective_fog(&self) -> (f32, f32) {
        let s = self.precip_strength;
        let falling = s > 0.0;
        let visibility = match self.precip_kind {
            1 if falling => self.visibility.min(100_000.0 / s),
            2 if falling => self.visibility.min(20_000.0 / s),
            _ => self.visibility,
        };
        let density = if self.precip_kind == 2 && falling {
            self.fog_density.max(0.8)
        } else if self.clouds == Some(true) {
            self.fog_density.min(0.7)
        } else {
            self.fog_density
        };
        (visibility, density)
    }

    /// The share Omsi.exe gives the sun under these clouds (0x753120): none under an `ovc`
    /// deck, half under other clouds; `None` without clouds (the sun's own share counts).
    pub fn cloud_share(&self) -> Option<f32> {
        self.clouds.map(|ovc| if ovc { 0.0 } else { 0.5 })
    }

    /// Omsi.exe's daylight sum (0x752c34), with the sun's share `k` (0..1) and envir.cfg's
    /// three lights `a`, `b`, `c` (0..1) at the sun's altitude: the visibility lets through
    /// 2^(-1000 m / visibility) of the light; the sun's light A gets (1 - s) of it and the
    /// sky's light B s, where s runs from 0.2 in the sun to 1 without it (where the light
    /// also falls to 0.3); the ambient light C takes 1 - 0.45 of A's weight; and without
    /// the sun all of it is dimmed to 0.9 x the fog density. The mean over red, green and
    /// blue of (A / 2 + B / 2 + C), each at most 1.
    pub fn daylight_sum(&self, a: Vec3, b: Vec3, c: Vec3, k: f32) -> f32 {
        let k = k.clamp(0.0, 1.0);
        let (visibility, density) = self.effective_fog();
        let clear = (-1000.0 * std::f32::consts::LN_2 / visibility.max(1.0)).exp();
        let dim = k + (1.0 - k) * 0.9 * density;
        let through = clear * (k + (1.0 - k) * 0.3);
        let s = 0.2 + 0.8 * (1.0 - k);
        let wa = ((1.0 - s) * through).min(1.0);
        let wb = (s * through).min(1.0);
        let wc = (1.0 - 0.45 * wa).clamp(0.0, 1.0);
        let sum = (a * wa * dim / 2.0 + b * wb * dim / 2.0 + c * wc * dim).min(Vec3::ONE);
        (sum.x + sum.y + sum.z) / 3.0
    }
}

fn default_envir_values() -> Envir {
    Envir {
        twilight_start_end: (-18.0, 10.0),
        light_color_a: [
            [0.0; 3],
            [0.0; 3],
            [200.0, 100.0, 20.0],
            [255.0, 255.0, 240.0],
            [255.0; 3],
        ],
        light_color_b: [
            [5.0, 5.0, 10.0],
            [10.0, 10.0, 20.0],
            [30.0, 20.0, 40.0],
            [60.0, 70.0, 80.0],
            [80.0, 90.0, 100.0],
        ],
        light_color_c: [
            [2.0, 2.0, 5.0],
            [15.0, 15.0, 25.0],
            [40.0, 50.0, 60.0],
            [220.0; 3],
            [230.0, 230.0, 255.0],
        ],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_sky_pictures_blend_as_omsi_blends_them() {
        let tw = (-18.0, 10.0);
        assert_eq!(omsi_sky_weights(30.0, tw), [1.0, 0.0, 0.0]);
        // (the day reached at 9 degrees, 0.9 of the twilight's end)
        assert_eq!(omsi_sky_weights(9.5, tw), [1.0, 0.0, 0.0]);
        assert_eq!(omsi_sky_weights(0.0, tw), [0.0, 1.0, 0.0]);
        let w = omsi_sky_weights(4.5, tw);
        assert!(
            (w[0] - 128.0 / 255.0).abs() < 1e-6 && (w[0] + w[1] - 1.0).abs() < 1e-6,
            "{w:?}"
        );
        let w = omsi_sky_weights(-8.1, tw);
        assert!((w[2] - 128.0 / 255.0).abs() < 1e-6 && w[0] == 0.0, "{w:?}");
        assert_eq!(omsi_sky_weights(-17.0, tw), [0.0, 0.0, 1.0]);
    }

    use super::*;

    fn clock_at(hours: f64) -> SimClock {
        SimClock {
            day_of_year: 182,
            time: hours * 3600.0,
            ..Default::default()
        }
    }

    #[test]
    fn the_sun_follows_the_maps_time_zone_and_summer_time() {
        let mut spandau = SunPlace::default();
        spandau.dst.push((19890326, 19890924, 2.0, 3.0, 1.0));
        // 21 June 1989: summer time, the sun due south at about 13:07 local
        let mut c = SimClock {
            year: 1989,
            day_of_year: 172,
            time: 13.12 * 3600.0,
            ..Default::default()
        };
        let (alt, az) = sun_position(&c, &spandau);
        assert!(
            (az - 180.0).abs() < 3.0 && (alt - 61.0).abs() < 1.5,
            "summer noon: alt {alt:.1} az {az:.1}"
        );
        // 21 December: standard time, south at about 12:07
        c.day_of_year = 355;
        c.time = 12.12 * 3600.0;
        let (alt, az) = sun_position(&c, &spandau);
        assert!(
            (az - 180.0).abs() < 3.0 && (alt - 14.0).abs() < 1.5,
            "winter noon: alt {alt:.1} az {az:.1}"
        );
        // without summer time the June sun would be an hour further on
        c.day_of_year = 172;
        c.time = 13.12 * 3600.0;
        let (_, az) = sun_position(&c, &SunPlace::default());
        assert!(az > 195.0, "no DST: az {az:.1}");
    }

    #[test]
    fn the_sun_runs_as_omsi_runs_it() {
        // early May at 45 N on the Greenwich meridian: Omsi.exe's sun stands highest at about
        // 11:53 (its right ascension off the tilted equator), at 90 - 45 + 16.21 degrees
        let place = SunPlace {
            lat_deg: 45.0,
            lon_deg: 0.0,
            timezone: 0.0,
            dst: Vec::new(),
        };
        let at = |doy: i32, minutes: i32| {
            sun_position(
                &SimClock {
                    day_of_year: doy,
                    time: minutes as f64 * 60.0,
                    ..Default::default()
                },
                &place,
            )
        };
        let (best_min, best_alt) = (600..800)
            .map(|m| (m, at(126, m).0))
            .fold((0, f64::MIN), |b, x| if x.1 > b.1 { x } else { b });
        assert!((712..=714).contains(&best_min), "noon at {best_min} min");
        assert!((best_alt - 61.22).abs() < 0.02, "alt {best_alt}");
        let (_, az) = at(126, best_min);
        assert!((az - 180.0).abs() < 1.0, "az {az}");
        // midsummer: the declination is sin(23.44) radians, 22.79 degrees
        let top = (600..840).map(|m| at(171, m).0).fold(f64::MIN, f64::max);
        assert!((top - (45.0 + 22.79)).abs() < 0.02, "midsummer top {top}");
        // morning east, afternoon west
        assert!((90.0..180.0).contains(&at(126, 600).1));
        assert!((180.0..270.0).contains(&at(126, 840).1));
    }

    #[test]
    fn the_moon_rises_and_waxes_on_time() {
        // full moon of 20 January 2000 04:40 UTC: nearly all lit, opposite the sun, high
        // in a Berlin winter night
        let berlin = SunPlace {
            lat_deg: 52.5,
            lon_deg: 13.4,
            timezone: 0.0,
            dst: Vec::new(),
        };
        let c = SimClock {
            year: 2000,
            day_of_year: 20,
            time: 4.67 * 3600.0,
            ..Default::default()
        };
        let (alt, _, illum) = moon_position(&c, &berlin);
        assert!(illum > 0.98, "illum {illum}");
        assert!(alt > 10.0, "alt {alt}");
        // new moon of 6 January 2000 18:14 UTC: dark
        let c = SimClock {
            year: 2000,
            day_of_year: 6,
            time: 18.23 * 3600.0,
            ..Default::default()
        };
        assert!(moon_position(&c, &berlin).2 < 0.02);
        // first quarter of 14 January 2000 13:34 UTC: half lit, in the south-east to south
        // in the afternoon
        let c = SimClock {
            year: 2000,
            day_of_year: 14,
            time: 13.57 * 3600.0,
            ..Default::default()
        };
        let (alt, az, illum) = moon_position(&c, &berlin);
        assert!((illum - 0.5).abs() < 0.08, "illum {illum}");
        assert!(
            (90.0..200.0).contains(&az) && alt > 0.0,
            "alt {alt} az {az}"
        );
    }

    #[test]
    fn envir_brightness_is_the_daylight_plus_the_light_map_under_the_lamps() {
        // a December midnight: only the faint night sky (lights B and C), so the light
        // map's light is nearly all of it
        let night = Daylight::compute(
            &SimClock {
                day_of_year: 355,
                time: 0.0,
                ..Default::default()
            },
            None,
        );
        assert!(night.lamps_on && night.daylight_sum < 0.6);
        let dark = night.envir_brightness(None);
        assert!(dark > 0.0 && dark < 0.06, "{dark}");
        let lit = night.envir_brightness(Some(Vec3::new(0.6, 0.5, 0.4)));
        assert!((lit - 0.5 - dark).abs() < 1e-5, "{lit}");
        // noon: the daylight is 1, and the light map is off by day
        let noon = Daylight::compute(&clock_at(12.0), None);
        assert!(!noon.lamps_on && noon.daylight_sum > 0.95);
        assert!(noon.envir_brightness(None) > 0.95);
        assert_eq!(
            noon.envir_brightness(Some(Vec3::ONE)),
            noon.envir_brightness(None)
        );
    }

    #[test]
    fn the_lamps_follow_omsis_daylight_sum_under_the_weather() {
        // stock envir.cfg lights at a sun altitude (as `Daylight::compute` takes them)
        let envir = default_envir_values();
        let lights = |alt: f32| {
            let tw = envir.twilight_start_end;
            (
                stops(&envir.light_color_a, tw, alt),
                stops(&envir.light_color_b, tw, alt),
                stops(&envir.light_color_c, tw, alt),
            )
        };
        let lamps = |w: &SkyWeather, alt: f32| {
            let (a, b, c) = lights(alt);
            w.daylight_sum(a, b, c, w.cloud_share().unwrap_or(omsi_sun_share(alt))) < 0.6
        };
        // a clear sky: on below about 4.5 degrees of sun, not at the horizon as by the
        // sun's height alone
        let clear = SkyWeather::default();
        assert!(lamps(&clear, 4.0) && !lamps(&clear, 5.0));
        assert!(!lamps(&clear, 30.0) && lamps(&clear, -3.0));
        // the noon sun and the night sky by hand: (A/2 x 0.8 + B/2 x 0.2 + C x 0.645) at
        // 50 km (x 0.986), each colour at most 1
        let (a, b, c) = lights(90.0);
        assert_eq!(clear.daylight_sum(a, b, c, 1.0), 1.0);
        let (a, b, c) = (Vec3::splat(0.2), Vec3::splat(0.1), Vec3::splat(0.3));
        let v = (-std::f32::consts::LN_2 / 50.0).exp();
        let by_hand = 0.2 * 0.8 * v / 2.0 + 0.1 * 0.2 * v / 2.0 + 0.3 * (1.0 - 0.45 * 0.8 * v);
        assert!((clear.daylight_sum(a, b, c, 1.0) - by_hand).abs() < 1e-5);
        // an overcast deck: no sun's share, the fog held to 0.7 - the lamps stay on through
        // a winter's day and only go out round a summer noon
        let ovc = SkyWeather {
            clouds: Some(true),
            ..SkyWeather::default()
        };
        assert!(lamps(&ovc, 15.0) && lamps(&ovc, 40.0));
        assert!(!lamps(&ovc, 75.0));
        // rain and snow shorten the visibility, snow thickens the fog (the lamps only come
        // on in it by day under the clouds such weather brings)
        let rain = SkyWeather {
            precip_kind: 1,
            precip_strength: 200.0,
            ..SkyWeather::default()
        };
        assert_eq!(rain.effective_fog(), (500.0, 1.0));
        let snow = SkyWeather {
            precip_kind: 2,
            precip_strength: 100.0,
            fog_density: 0.5,
            ..SkyWeather::default()
        };
        assert_eq!(snow.effective_fog(), (200.0, 0.8));
        // the sun's share rises over +-0.02 rad round the horizon
        assert_eq!(omsi_sun_share(-2.0), 0.0);
        assert_eq!(omsi_sun_share(2.0), 1.0);
        assert!((omsi_sun_share(0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn stock_envir_leaves_the_enhanced_sky_untinted() {
        let own = default_envir_values();
        for h in [3.0, 6.0, 12.0, 19.0, 23.0] {
            assert_eq!(
                Daylight::compute(&clock_at(h), None).envir_tint,
                [Vec3::ONE; 3],
                "{h} h"
            );
            assert_eq!(
                Daylight::compute(&clock_at(h), Some(&own)).envir_tint,
                [Vec3::ONE; 3],
                "{h} h"
            );
        }
    }

    #[test]
    fn a_warmer_envir_tints_the_sun() {
        let mut warm = default_envir_values();
        // the midday sun as warm as a late afternoon's
        warm.light_color_a[4] = [255.0, 230.0, 170.0];
        warm.light_color_a[3] = [255.0, 230.0, 160.0];
        let d = Daylight::compute(&clock_at(12.0), Some(&warm));
        let a = d.envir_tint[0];
        assert!((a.x - 1.0).abs() < 1e-3 && a.y < 0.95 && a.z < 0.7, "{a:?}");
        assert_eq!(d.envir_tint[1], Vec3::ONE);
        // by night the stock sun is black: nothing to compare with, no tint
        assert_eq!(
            Daylight::compute(&clock_at(1.0), Some(&warm)).envir_tint[0],
            Vec3::ONE
        );
    }

    #[test]
    fn a_zone_without_a_location_keeps_its_afternoon_light() {
        let c = SimClock {
            day_of_year: 69,
            time: 16.25 * 3600.0,
            ..Default::default()
        };
        let mut caxias = SunPlace::default();
        caxias.locate(-3.0, None);
        assert!(sun_position(&c, &caxias).0 > 10.0);
        assert_eq!(
            caxias.lat_deg,
            SunPlace::default().lat_deg,
            "the climate keeps its latitude"
        );
        let mut moscow = SunPlace::default();
        moscow.locate(3.0, None);
        assert_eq!(
            (moscow.lat_deg, moscow.lon_deg),
            (SunPlace::default().lat_deg, 45.0)
        );
        let mut berlin = SunPlace::default();
        berlin.locate(-3.0, Some((52.505, 13.2782)));
        assert!(sun_position(&c, &berlin).0 < 0.0);
        let mut spandau = SunPlace::default();
        spandau.locate(1.0, None);
        assert_eq!(spandau.lon_deg, SunPlace::default().lon_deg);
    }
}
