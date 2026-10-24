//! The physical weather (`weather = natural`, and what a session runs on when no weather is
//! chosen): a column of the atmosphere over the map whose state the physics carries
//! forward through the hours, instead of one fixed `.owt` state. Clouds, fog, haze, rain
//! and the light that comes of them are consequences of that state, not choices.
//!
//! What moves it:
//! * **the large-scale weather**: highs and lows travel past (three superposed planetary
//!   waves of 4-10 days, stronger in winter, their phases those of the year). Falling
//!   pressure ahead of a low means rising air: the free troposphere cools and moistens,
//!   cirrus arrives first (the warm front's upper cloud runs half a day ahead), then the
//!   grey deck, then the rain; behind it the air sinks and dries and the sky clears. The
//!   winds blow with the pressure gradient, from the south-west ahead of a low, from the
//!   north-west behind it; warm air comes ahead of it and cold air after it.
//! * **the surface energy balance**: the sun's short-wave light through the clouds (Kasten
//!   and Czeplak's cloud transmission) warms the ground and the air on it, the ground's
//!   long-wave radiation cools it, much more under a clear sky than under cloud and humid
//!   air; the air relaxes towards what the large-scale flow brings.
//! * **the boundary layer**: the day's sensible heat mixes the air from the ground up
//!   (encroachment into a stable layer, about 250 m an hour on a summer morning); where
//!   its top rises above the lifting condensation level (125 m per degree of dew-point
//!   spread) cumulus forms, with its base at that height, and it dissolves again when the
//!   heating stops in the evening. At night the mixed layer collapses into a shallow
//!   stable one with yesterday's residual layer above it.
//! * **water**: the air's humidity relaxes to what the flow brings and the wet ground
//!   gives off; when the ground air cools to its dew point on a calm clear night, fog
//!   forms, and the morning sun burns it off; a saturated low layer under an inversion is
//!   grey stratus. Rain falls from a saturated, rising troposphere (and from big cumulus on
//!   a hot humid afternoon), and wets the ground.
//! * **aerosol**: the city's dust and exhaust collect in the air, most under a sinking
//!   high with light winds; rain washes it out and wind carries it off; in humid air the
//!   particles swell (a milky sky, softer distances) - which is why the evening after rain
//!   is crystal clear with a pale sunset, and a sultry evening in a high ends in red.

use omsi_content::weather::Weather;
mod observations;
pub use observations::Observations;

fn cloud_kind(total: f32, deck: f32) -> &'static str {
    if deck > 0.85 { "Overcast 1" }
    else if total > 0.6 { "Cumulus 3" }
    else if total > 0.35 { "Cumulus 2" }
    else if total > 0.08 { "Cumulus 1" }
    else { "-1" }
}

/// The model's state at one moment.
#[derive(Debug, Clone)]
pub struct WeatherModel {
    /// Hours since the model's epoch (local solar time of the map, continuous over days).
    hours: f64,
    /// Seed of the year's large-scale waves.
    year_seed: u32,
    latitude: f32,
    /// Air temperature at 2 m (°C).
    pub temp: f32,
    /// Specific humidity at 2 m (g/kg).
    pub q: f32,
    /// Depth of the mixed layer now (m) and of the day's residual layer (m).
    pub mixed: f32,
    pub residual: f32,
    /// Relative humidity of the free troposphere (850-700 hPa), 0..1.2 (over 1 it rains).
    pub rh_up: f32,
    /// Cumulus cover (0..0.85), the grey deck (0..1), the cirrus veil's optical depth.
    pub cumulus: f32,
    pub deck: f32,
    pub veil: f32,
    /// Fog (0 none .. 1 thick).
    pub fog: f32,
    /// Rain or snow rate (0..1).
    pub precip: f32,
    /// Aerosol load relative to an ordinary day.
    pub aerosol: f32,
    /// How wet the ground is (0..1).
    pub ground_wet: f32,
    /// How much of the ground the snow covers (0..1), from its depth.
    pub snow_cover: f32,
    /// The lying snow's depth (cm): a cold spell's snowfall stays for days or weeks, and
    /// thaws by the warmth of the air (degree-hours), by the sun and under rain.
    pub snow_depth: f32,
    /// Thunderstorm activity (0 none .. 1 a violent storm): deep convection, which needs
    /// warm moist air under a moist, rising troposphere.
    pub storm: f32,
    /// Optical depth of the grey deck (stratus, altostratus, nimbostratus): the liquid
    /// water in its column. 0.15 per g/m² for droplets of 10 µm - a thin stratus (30 g/m²)
    /// has 5, a raining nimbostratus (500 g/m²) some 75.
    pub deck_tau: f32,
    /// How deep the cumulus layer stands (m): the convection stops at the inversion (a
    /// shallow stratocumulus sheet under a high), or goes on up through a moist,
    /// lifting troposphere (towering cumulus), up to the tropopause in a thunderstorm.
    pub cu_depth: f32,
    /// Altocumulus (cover of the layer at 2-6 km): moist air at the middle levels
    /// ahead of a front, before the troposphere is saturated.
    pub alto: f32,
}

/// What the renderer takes from the model besides the `.owt` values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelSky {
    pub cumulus: f32,
    pub deck: f32,
    pub veil: f32,
    /// aerosol amount relative to a clear day, its Ångström exponent, its layer depth (m)
    pub haze: f32,
    pub angstrom: f32,
    pub aerosol_height: f32,
    /// the cumulus layer's base and top over the ground (m)
    pub cu_base: f32,
    pub cu_top: f32,
    pub alto: f32,
    pub deck_tau: f32,
    pub storm: f32,
}

/// The model of a session: its state, and where and from when it runs. Kept by the
/// session that runs on it (the window's, the offscreen run, the showroom's look), which
/// carries it on with its clock.
#[derive(Debug, Clone)]
pub struct ModelRun {
    model: WeatherModel,
    year: i32,
    latitude: f64,
    longitude: f64,
    /// The clock time (s of the day, day of the year) the model stands at.
    at: (i32, f64),
}

/// Whether a weather choice means this model: `natural`, or none at all.
pub use omsi_content::weather_choice::is_natural;

impl ModelRun {
    /// Start the model for a session beginning at `clock` (at the map's place as far as it
    /// is known yet: `refresh` starts it again once the map has told its own) and give its
    /// weather.
    pub fn start(clock: &omsi_sim::SimClock) -> (ModelRun, Weather) {
        let place = omsi_sim::daylight::place();
        let model = WeatherModel::new(clock, &place);
        let w = model.weather();
        log::info!("weather: the physical model, {:.1} °C, {:.0} % humidity, cumulus {:.2}, deck {:.2}, veil {:.2}, fog {:.2}, rain {:.2}, visibility {:.0} m", model.temp, model.humidity() * 100.0, model.cumulus, model.deck, model.veil, model.fog, model.precip, w.fog.0);
        (ModelRun { model, year: clock.year, latitude: place.latitude, longitude: place.longitude, at: (clock.day_of_year, clock.time) }, w)
    }

    /// Carry the model on to `clock` (started again where the map's place differs from the
    /// one it was started for, or the clock jumped back or more than a day on) and give its
    /// weather.
    pub fn refresh(&mut self, clock: &omsi_sim::SimClock) -> Weather {
        let s = self;
        let place = omsi_sim::daylight::place();
        let elapsed = (clock.day_of_year - s.at.0) as f64 * 86_400.0 + (clock.time - s.at.1);
        let moved = (place.latitude - s.latitude).abs() > 0.01 || (place.longitude - s.longitude).abs() > 0.01;
        if moved || clock.year != s.year || !(0.0..=86_400.0).contains(&elapsed) {
            s.model = WeatherModel::new(clock, &place);
            s.year = clock.year;
            s.latitude = place.latitude;
            s.longitude = place.longitude;
        } else if elapsed > 0.0 {
            s.model.step(clock.year, &place, (elapsed / 3600.0) as f32);
        }
        s.at = (clock.day_of_year, clock.time);
        s.model.weather()
    }

    /// The sky the model shows now (for `weather_lighting`).
    pub fn sky(&self) -> ModelSky {
        self.model.sky()
    }
}

/// How far back the model starts before the session (h).
const SPIN_UP_HOURS: f64 = 21.0 * 24.0;

fn hash(seed: u32, k: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B) ^ 0x27d4_eb2d;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x & 0xFF_FFFF) as f32 / 16_777_216.0
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Saturation specific humidity (g/kg) at a temperature (°C), at 1000 hPa (Magnus).
fn q_sat(t: f32) -> f32 {
    let e = 6.112 * (17.67 * t / (t + 243.5)).exp();
    622.0 * e / (1000.0 - 0.378 * e)
}

/// Dew point (°C) of air with specific humidity q (g/kg), at 1000 hPa.
fn dew_point(q: f32) -> f32 {
    let e = (q.max(0.01) * 1000.0 / (622.0 + 0.378 * q.max(0.01))).max(1e-3);
    let g = (e / 6.112).ln();
    243.5 * g / (17.67 - g)
}

/// The season: 0 in midwinter, 1 in high summer (half a year later south of the equator).
fn summer(day_of_year: f32, latitude: f32) -> f32 {
    let doy = day_of_year + if latitude < 0.0 { 182.0 } else { 0.0 };
    0.5 - 0.5 * (2.0 * std::f32::consts::PI * (doy - 20.0) / 365.0).cos()
}

/// Day of the year (1-based) and sun altitude (degrees) at model hour `h`.
fn sun_at(year: i32, hours: f64, place: &omsi_sim::daylight::SunPlace) -> (f32, f32) {
    let day = (hours / 24.0).floor();
    let solar = hours - day * 24.0;
    let len = omsi_sim::clock::days_in_year(year) as f64;
    let doy = (day.rem_euclid(len) + 1.0) as i32;
    // the clock that shows this solar time at the place
    let clock_h = solar + place.timezone - place.longitude / 15.0;
    let mut clock = omsi_sim::SimClock { year, day_of_year: doy, time: clock_h * 3600.0, ..Default::default() };
    clock.time += place.dst_hours(&clock) * 3600.0;
    let (alt, _) = omsi_sim::daylight::sun_position(&clock, place);
    (doy as f32, alt as f32)
}

impl WeatherModel {
    /// The large-scale flow at model hour `h`: the pressure anomaly (hPa) and its tendency
    /// (hPa per day). Three travelling waves; winter's storms are deeper than summer's.
    fn synoptic(&self, h: f64, summer: f32) -> (f32, f32) {
        let days = h / 24.0;
        // (incommensurate periods, each wave's strength itself swelling and fading over
        // weeks: a stormy spell, a long blocking high - never the same week twice)
        let periods = [3.4f64, 4.9, 6.6, 9.1, 14.3];
        let amps = [3.0f32, 4.0, 3.5, 3.0, 2.5];
        let mut p = 0.0f32;
        let mut dp = 0.0f32;
        let k = 1.25 - 0.55 * summer;
        for (i, (&t, &a)) in periods.iter().zip(&amps).enumerate() {
            let phase = hash(self.year_seed, i as u32) as f64 * std::f64::consts::TAU;
            let mod_t = 23.0 + 11.0 * hash(self.year_seed, 10 + i as u32) as f64;
            let mod_phase = hash(self.year_seed, 20 + i as u32) as f64 * std::f64::consts::TAU;
            let swell = (0.55 + 0.45 * (std::f64::consts::TAU * days / mod_t + mod_phase).sin()) as f32;
            let w = std::f64::consts::TAU / t;
            p += a * k * swell * (w * days + phase).sin() as f32;
            dp += a * k * swell * (w as f32) * (w * days + phase).cos() as f32;
        }
        (p, dp)
    }

    /// The lift of the large-scale flow (-1.5 sinking .. 1.5 rising): falling pressure ahead
    /// of a low and the low itself lift the air, rising pressure and a high sink it.
    fn lift(&self, h: f64, summer: f32) -> f32 {
        let (p, dp) = self.synoptic(h, summer);
        (-dp / 8.0 - p / 16.0).clamp(-1.5, 1.5)
    }

    /// The climate's mean temperature (°C) for the day of the year at the latitude.
    fn climate_temp(&self, doy: f32) -> f32 {
        // (27 °C at the equator, 10 at Berlin's 52.5°, -3 at 70°; the seasons' swing grows
        // towards the poles)
        let lat = self.latitude.abs();
        let mean = 27.0 - 0.0062 * lat * lat;
        let amp = (0.18 * lat).clamp(1.0, 14.0);
        mean + amp * (summer(doy, self.latitude) * 2.0 - 1.0)
    }

    /// A model for the clock at `place`, spun up over the three weeks before it so that what
    /// it shows at the start already follows from the days before - the snow of last
    /// week's cold spell included.
    pub fn new(clock: &omsi_sim::SimClock, place: &omsi_sim::daylight::SunPlace) -> WeatherModel {
        let solar = place.solar_hours(clock);
        let hours = (clock.day_of_year - 1) as f64 * 24.0 + solar;
        let year_seed = (clock.year.rem_euclid(100_000) as u32).wrapping_mul(2_654_435_761);
        let mut m = WeatherModel {
            hours: hours - SPIN_UP_HOURS,
            year_seed,
            latitude: place.latitude as f32,
            temp: 10.0,
            q: 6.0,
            mixed: 300.0,
            residual: 800.0,
            rh_up: 0.6,
            cumulus: 0.0,
            deck: 0.3,
            veil: 0.0,
            fog: 0.0,
            precip: 0.0,
            aerosol: 1.0,
            ground_wet: 0.0,
            snow_cover: 0.0,
            snow_depth: 0.0,
            storm: 0.0,
            deck_tau: 8.0,
            cu_depth: 800.0,
            alto: 0.0,
        };
        let (doy, _) = sun_at(clock.year, m.hours, place);
        m.temp = m.climate_temp(doy);
        m.q = q_sat(m.temp) * 0.7;
        while m.hours < hours {
            m.step(clock.year, place, (hours - m.hours).min(1.0) as f32);
        }
        m
    }

    /// Carry the state forward by `dt` hours (steps of at most ten minutes).
    pub fn step(&mut self, year: i32, place: &omsi_sim::daylight::SunPlace, dt: f32) {
        let mut left = dt;
        while left > 1e-6 {
            let h = left.min(1.0 / 6.0);
            self.step_once(year, place, h);
            left -= h;
        }
    }

    fn step_once(&mut self, year: i32, place: &omsi_sim::daylight::SunPlace, dt: f32) {
        let (doy, alt) = sun_at(year, self.hours, place);
        let s = summer(doy, self.latitude);
        let (p, dp) = self.synoptic(self.hours, s);
        let lift = self.lift(self.hours, s);
        let wind = 2.0 + 0.55 * dp.abs() + 0.12 * p.abs();
        // --- radiation at the ground
        let total_cloud = (self.cumulus + self.deck * (1.0 - self.cumulus)).clamp(0.0, 1.0);
        let sin_alt = alt.to_radians().sin().max(0.0);
        let clear_sw = 1000.0 * sin_alt.powf(1.15) * (-0.09 * self.aerosol / sin_alt.max(0.05)).exp();
        let cloud_t = 1.0 - 0.75 * total_cloud.powf(3.4);
        let sw = clear_sw * cloud_t * (-0.35 * self.veil).exp() * (1.0 - 0.6 * self.fog);
        let albedo = 0.18 + 0.5 * self.snow_cover;
        let sw_abs = sw * (1.0 - albedo);
        let rh_s = (self.q / q_sat(self.temp)).clamp(0.0, 1.2);
        let lw_out = 135.0 * (1.0 - 0.8 * total_cloud.max(self.fog)) * (1.0 - 0.35 * rh_s.min(1.0));
        // --- the air at the ground: heated and cooled, and brought by the flow
        // (warm air ahead of a low - pressure falling - and cold air behind it)
        let advected = self.climate_temp(doy) - (0.45 * dp).clamp(-6.0, 6.0);
        let heat = sw_abs - lw_out;
        // (by day the heat goes into the whole mixed layer; at night the cooling stays in a
        // shallow stable layer over the ground, which cools fast)
        // (and is cut off from the flow above it: on a calm clear night the ground air cools
        // on its own, down to its dew point)
        let (capacity, coupling) = if heat > 0.0 { (340.0, 14.0) } else { (170.0, 14.0 + 20.0 * (1.0 - (wind / 6.0).min(1.0))) };
        self.temp += dt * (heat / capacity - (self.temp - advected) / coupling);
        // --- the boundary layer: mixed up by the day's heat, collapsing at night
        // (a Bowen ratio of about one: half the net radiation heats the air)
        let sensible = (0.5 * (sw_abs - lw_out) - 15.0).max(0.0);
        if sensible > 0.0 {
            let grow = 3600.0 * sensible / (1200.0 * 0.0035 * self.mixed.max(100.0));
            // (sinking air presses it down)
            self.mixed += dt * (grow - 60.0 * (-lift).max(0.0));
        } else {
            self.mixed += dt * (150.0 - self.mixed) / 1.2;
        }
        self.mixed = self.mixed.clamp(100.0, 2600.0);
        self.residual = if self.mixed > self.residual { self.mixed } else { self.residual + dt * (self.mixed - self.residual) / 30.0 };
        // --- water near the ground
        let rh_bg = (0.72 + 0.12 * lift + 0.08 * (1.0 - s)).clamp(0.4, 0.95);
        let q_bg = q_sat(advected) * rh_bg;
        let evaporation = self.ground_wet * (q_sat(self.temp) - self.q).max(0.0) * (0.05 + sw_abs / 4000.0);
        // (the plants' transpiration by day: the summer's main source of the air's water)
        let transpiration = sw_abs / 2500.0 * (0.25 + 0.75 * s) * (1.0 - self.snow_cover);
        self.q += dt * ((q_bg - self.q) / 16.0 + evaporation + transpiration);
        let qs = q_sat(self.temp);
        let td = dew_point(self.q.min(qs));
        // fog: the ground air at its dew point on a calm night; burnt off by the sun
        let saturated = self.q >= qs * 0.985;
        if saturated && wind < 3.5 && sw_abs < 120.0 {
            self.fog += dt * 0.5;
        } else {
            self.fog -= dt * (0.15 + sw_abs / 250.0 + wind / 10.0) * self.fog.max(0.15);
        }
        self.fog = self.fog.clamp(0.0, 1.0);
        if self.q > qs {
            // (dew, and the fog droplets)
            self.q = qs;
        }
        // --- the free troposphere: moistened and cooled by rising air, dried by sinking
        let rh_up_bg = (0.6 + 0.06 * (1.0 - s)).clamp(0.3, 0.8);
        self.rh_up += dt * (0.032 * lift - (self.rh_up - rh_up_bg) / 30.0 - 0.1 * self.precip);
        self.rh_up = self.rh_up.clamp(0.1, 1.25);
        // --- clouds
        // cumulus: where the mixed layer reaches the condensation level
        let lcl = 125.0 * (self.temp - td).max(0.0);
        let reach = (self.mixed - lcl) / (0.3 * lcl + 200.0);
        let cu_target = if sensible > 0.0 { smoothstep(0.0, 1.0, reach) * 0.85 * (0.45 + 0.55 * self.rh_up.min(1.0)) * (1.0 + 0.5 * lift.min(0.0)) } else { 0.0 };
        let cu_tau = if cu_target > self.cumulus { 0.6 } else { 1.0 };
        self.cumulus += dt * (cu_target - self.cumulus) / cu_tau;
        self.cumulus = self.cumulus.clamp(0.0, 0.85);
        // the grey deck: a saturated rising troposphere (nimbostratus, altostratus), or a
        // saturated layer under an inversion (stratus) where the day does not mix it out
        let mid = smoothstep(0.76, 0.96, self.rh_up);
        let low = smoothstep(0.86, 0.97, rh_s) * (1.0 - smoothstep(80.0, 300.0, sensible)) * (0.6 + 0.4 * (1.0 - s));
        let deck_target = mid.max(low * 0.97);
        self.deck += dt * (deck_target - self.deck) / 1.5;
        self.deck = self.deck.clamp(0.0, 1.0);
        // its water: a saturated, strongly rising troposphere piles up a nimbostratus (liquid
        // water path of hundreds of g/m², optical depth up to 75), a weakly rising one a thin
        // altostratus (a few); a stratus under an inversion holds some 30-150 g/m² - thicker
        // in the cold season, when little sun mixes it out, and as it nears the rain
        let mid_tau = 4.0 + 70.0 * smoothstep(0.78, 1.12, self.rh_up) * (0.35 + 0.65 * lift.max(0.0).min(1.0));
        let low_tau = 6.0 + 26.0 * (1.0 - s) + 30.0 * smoothstep(0.9, 1.0, rh_s);
        let tau_target = mid_tau.max(low * low_tau);
        self.deck_tau += dt * (tau_target - self.deck_tau) / 1.0;
        self.deck_tau = self.deck_tau.clamp(1.5, 90.0);
        // altocumulus: the middle levels moist but not yet saturated
        let alto_target = smoothstep(0.55, 0.78, self.rh_up) * (1.0 - smoothstep(0.78, 0.95, self.rh_up)) * 0.9;
        self.alto += dt * (alto_target - self.alto) / 2.0;
        self.alto = self.alto.clamp(0.0, 0.95);
        // the veil: the warm front's cirrus, half a day ahead of its lift
        let ahead = self.lift(self.hours + 12.0, s);
        let veil_target = (ahead * 1.3 - 0.2).clamp(0.0, 2.0) * (1.0 - mid);
        self.veil += dt * (veil_target - self.veil) / 3.0;
        // --- rain: from a saturated rising troposphere, or a big cumulus on a sultry day
        let frontal = ((self.rh_up - 0.95) * 6.0).clamp(0.0, 1.0) * smoothstep(0.2, 0.7, lift);
        // (a cumulus grows into a shower cloud when it is deep - a lot of it, warm moist air
        // under a moist troposphere that is not sinking)
        let showers = smoothstep(0.45, 0.75, self.cumulus) * smoothstep(15.0, 24.0, self.temp) * smoothstep(0.6, 0.85, self.rh_up) * smoothstep(-0.4, 0.1, lift) * 0.8;
        let target = frontal.max(showers);
        // thunderstorms: a deep convective cloud needs the heat and the moisture of the
        // shower's recipe and a free troposphere that does not sink; the strongest ones
        // come on the hottest afternoons (and over a front's warm sector)
        let storm_target = smoothstep(0.5, 0.8, self.cumulus) * smoothstep(19.0, 28.0, self.temp) * smoothstep(0.66, 0.9, self.rh_up) * smoothstep(-0.2, 0.3, lift) * (0.4 + 0.6 * s);
        self.storm += dt * (storm_target - self.storm) / 0.7;
        self.storm = self.storm.clamp(0.0, 1.0);
        // the cumulus layer's depth: convection to the inversion - a shallow sheet in a
        // high (sinking air), towers when the troposphere is moist and lifting, the whole
        // troposphere in a storm
        let conv = (0.3 + 0.45 * lift + 0.9 * (self.rh_up - 0.6)).clamp(0.0, 1.0);
        let depth_target = (300.0 + 2300.0 * conv + 7000.0 * self.storm).min(9500.0);
        self.cu_depth += dt * (depth_target - self.cu_depth) / 1.0;
        self.cu_depth = self.cu_depth.clamp(250.0, 9500.0);
        self.precip += dt * (target.max(0.7 * self.storm) - self.precip) / 0.5;
        self.precip = self.precip.clamp(0.0, 1.0);
        // (it falls as snow up to about 1.5 °C at the ground: the flakes melt on the way
        // down only in warmer air)
        let snowing = self.precip > 0.05 && self.temp < 1.5;
        // the ground: wetted by rain, dried by sun and wind
        self.ground_wet += dt * (self.precip * 2.0 - (sw_abs / 400.0 + wind / 30.0 + 0.02) * self.ground_wet);
        self.ground_wet = self.ground_wet.clamp(0.0, 1.0);
        // the snow: a heavy fall lays some 2 cm an hour; it thaws by about a millimetre an
        // hour per degree of warm air, by the sun it absorbs and under warm rain
        let fall = if snowing { self.precip * 2.0 } else { 0.0 };
        let melt = 0.1 * self.temp.max(0.0) + sw_abs * (1.0 - 0.7) / 900.0 * (self.temp > -3.0) as i32 as f32 + if self.precip > 0.05 && !snowing { self.precip * 0.4 } else { 0.0 };
        self.snow_depth = (self.snow_depth + dt * (fall - melt)).clamp(0.0, 80.0);
        self.snow_cover = smoothstep(0.0, 3.0, self.snow_depth);
        // --- aerosol: the city's emissions, trapped under a high, washed out, blown away
        let trapping = 1.0 + 0.8 * (-lift).max(0.0);
        let source = 0.02 * trapping;
        let sink = self.aerosol * (0.012 * wind / 5.0 + 0.35 * self.precip + 0.004 * self.mixed / 1000.0);
        self.aerosol += dt * (source - sink - (self.aerosol - 0.8) * 0.004);
        self.aerosol = self.aerosol.clamp(0.25, 4.0);
        self.hours += dt as f64;
    }

    /// Relative humidity at the ground (0..1).
    pub fn humidity(&self) -> f32 {
        (self.q / q_sat(self.temp)).clamp(0.0, 1.0)
    }

    /// The cumulus base over the ground (m): the lifting condensation level, 125 m per
    /// degree of the dew-point spread, kept to what the sky model can draw.
    pub fn cloud_base(&self) -> f32 {
        (125.0 * (self.temp - dew_point(self.q.min(q_sat(self.temp)))).max(0.0)).clamp(450.0, 3200.0)
    }

    /// The sky for the renderer.
    pub fn sky(&self) -> ModelSky {
        let rh = self.humidity();
        // hygroscopic growth: particles swell in humid air and scatter more and greyer
        let growth = (1.0 - rh.min(0.93)).powf(-0.45);
        ModelSky {
            cumulus: self.cumulus,
            deck: self.deck,
            veil: self.veil,
            haze: (self.aerosol * growth).clamp(0.3, 6.0),
            angstrom: (1.55 - 0.9 * smoothstep(0.55, 0.95, rh)).clamp(0.45, 1.6),
            aerosol_height: self.mixed.max(0.7 * self.residual).max(250.0),
            cu_base: self.cloud_base(),
            cu_top: self.cloud_base() + self.cu_depth,
            alto: self.alto,
            deck_tau: self.deck_tau,
            storm: self.storm,
        }
    }

    /// The weather as OMSI's `.owt` values (what the vehicles, the sounds, the roads and the
    /// vanilla sky go by).
    pub fn weather(&self) -> Weather {
        let rh = self.humidity();
        let growth = (1.0 - rh.min(0.93)).powf(-0.45);
        // visibility (m): the aerosol and its growth, fog, rain or snow
        let clear = 45_000.0 / (self.aerosol * growth);
        let falling = 1.0 + 6.0 * self.precip * if self.temp < 1.0 { 3.0 } else { 1.0 };
        let vis = (clear / falling).min(50_000.0);
        let vis = (vis.ln() * (1.0 - self.fog) + 150f32.ln() * self.fog).exp();
        let total = self.cumulus.max(self.deck);
        let kind = cloud_kind(total, self.deck);
        let lcl = 125.0 * (self.temp - dew_point(self.q)).max(0.0);
        let (p, dp) = self.synoptic(self.hours, 0.5);
        let wind_speed = 2.0 + 0.55 * dp.abs() + 0.12 * p.abs();
        // south-west ahead of a low (pressure falling), north-west behind it
        let wind_dir = if dp < 0.0 { 225.0 } else { 300.0 } + 20.0 * (p / 10.0).clamp(-1.0, 1.0);
        let precip = if self.precip > 0.03 {
            vec![if self.temp < 1.0 { 2.0 } else { 1.0 }, (self.precip * 255.0).clamp(20.0, 255.0), 0.0, 0.0, 0.0]
        } else {
            vec![0.0, 0.0, 0.0, 0.0, 0.0]
        };
        Weather {
            name: "Natural".into(),
            description: String::new(),
            fog: (vis.max(80.0), 1.0),
            wind: (wind_dir, wind_speed),
            temp: (self.temp, omsi_content::weather_choice::absolute_humidity(self.temp, rh * 100.0)),
            pressure: 1013.0 + p,
            clouds: (kind.into(), lcl.max(200.0)),
            precip,
            ground_wet: [self.ground_wet * 255.0, 255.0, 115.0],
            snow: self.snow_cover > 0.3 || (self.precip > 0.05 && self.temp < 1.5),
            snow_on_road: (self.snow_depth > 2.0 && self.temp < 0.5) || (self.precip > 0.3 && self.temp < 0.0),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place() -> omsi_sim::daylight::SunPlace {
        omsi_sim::daylight::SunPlace::default()
    }

    fn run(year: i32, doy: i32, days: i32, mut f: impl FnMut(&WeatherModel, f64)) {
        let clock = omsi_sim::SimClock { year, day_of_year: doy, time: 0.0, ..Default::default() };
        let mut m = WeatherModel::new(&clock, &place());
        for k in 0..(days * 24 * 2) {
            m.step(year, &place(), 0.5);
            f(&m, k as f64 * 0.5);
        }
    }

    #[test]
    fn days_warm_up_and_nights_cool_down() {
        // over a summer month: afternoons warmer than the mornings by several degrees on
        // average, and the temperature within what a Berlin summer has
        let (mut morning, mut afternoon, mut n) = (0.0, 0.0, 0);
        run(2026, 180, 30, |m, h| {
            let hod = (h + 72.0).rem_euclid(24.0);
            assert!((-2.0..40.0).contains(&m.temp), "{} at {h}", m.temp);
            if (5.5..6.0).contains(&hod) {
                morning += m.temp;
                n += 1;
            }
            if (15.0..15.5).contains(&hod) {
                afternoon += m.temp;
            }
        });
        let range = (afternoon - morning) / n as f32;
        assert!((4.0..16.0).contains(&range), "diurnal range {range}");
    }

    #[test]
    fn the_year_has_grey_days_rain_fog_and_clear_evenings() {
        let mut grey_winter = 0;
        let mut grey_summer = 0;
        let mut rain_hours = 0;
        let mut fog_mornings = 0;
        let mut cumulus_afternoons = 0;
        let mut clear_evenings = 0;
        run(2026, 1, 45, |m, h| {
            let hod = (h + 72.0).rem_euclid(24.0);
            if (13.0..13.5).contains(&hod) && m.deck > 0.85 {
                grey_winter += 1;
            }
            if m.precip > 0.1 {
                rain_hours += 1;
            }
        });
        run(2026, 170, 45, |m, h| {
            let hod = (h + 72.0).rem_euclid(24.0);
            if (13.0..13.5).contains(&hod) && m.deck > 0.85 {
                grey_summer += 1;
            }
            if (15.0..15.5).contains(&hod) && m.cumulus > 0.25 && m.deck < 0.5 {
                cumulus_afternoons += 1;
            }
            if (20.0..20.5).contains(&hod) && m.cumulus < 0.15 && m.deck < 0.3 {
                clear_evenings += 1;
            }
        });
        run(2026, 280, 45, |m, h| {
            let hod = (h + 72.0).rem_euclid(24.0);
            if (6.5..7.0).contains(&hod) && m.fog > 0.4 {
                fog_mornings += 1;
            }
        });
        assert!(grey_winter > grey_summer + 5, "grey days: winter {grey_winter}, summer {grey_summer}");
        assert!((15..300).contains(&rain_hours), "rain {rain_hours} half-hours in 45 winter days");
        assert!(cumulus_afternoons >= 8, "cumulus afternoons {cumulus_afternoons}");
        assert!(clear_evenings >= 5, "clear evenings {clear_evenings}");
        assert!(fog_mornings >= 1, "foggy autumn mornings {fog_mornings}");
    }

    #[test]
    fn rain_washes_the_air_and_a_high_fills_it_with_haze() {
        // the haze right after rain against the haze before it
        let mut after_rain = Vec::new();
        let mut dry_long = Vec::new();
        let mut since = 1000.0;
        run(2026, 120, 90, |m, _| {
            if m.precip > 0.2 {
                since = 0.0;
            } else {
                since += 0.5;
            }
            if (6.0..12.0).contains(&since) {
                after_rain.push(m.aerosol);
            }
            if since > 96.0 {
                dry_long.push(m.aerosol);
            }
        });
        let avg = |v: &Vec<f32>| v.iter().sum::<f32>() / v.len().max(1) as f32;
        assert!(!after_rain.is_empty() && !dry_long.is_empty());
        assert!(avg(&dry_long) > avg(&after_rain) * 1.3, "{} vs {}", avg(&dry_long), avg(&after_rain));
    }

    #[test]
    fn the_state_changes_smoothly() {
        let mut last: Option<WeatherModel> = None;
        run(2026, 200, 10, |m, _| {
            if let Some(l) = &last {
                assert!((m.temp - l.temp).abs() < 2.5, "{} -> {}", l.temp, m.temp);
                assert!((m.deck - l.deck).abs() < 0.4 && (m.cumulus - l.cumulus).abs() < 0.5);
            }
            last = Some(m.clone());
        });
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;
    use omsi_sim::daylight::{place, set_place, SunPlace};
    use omsi_sim::SimClock;

    /// The session model as it was kept before `ModelRun`: one `Option<Session>` and the sky
    /// it showed, with `start`, `stop` and `refresh` as they were (statics then, locals here).
    #[derive(Default)]
    struct Before {
        session: Option<(WeatherModel, i32, f64, f64, (i32, f64))>,
        current: Option<ModelSky>,
    }

    impl Before {
        fn start(&mut self, clock: &SimClock) -> Weather {
            let place = place();
            let model = WeatherModel::new(clock, &place);
            let w = model.weather();
            self.current = Some(model.sky());
            self.session = Some((model, clock.year, place.latitude, place.longitude, (clock.day_of_year, clock.time)));
            w
        }

        fn stop(&mut self) {
            self.session = None;
            self.current = None;
        }

        fn refresh(&mut self, clock: &SimClock) -> Option<Weather> {
            let s = self.session.as_mut()?;
            let place = place();
            let elapsed = (clock.day_of_year - s.4 .0) as f64 * 86_400.0 + (clock.time - s.4 .1);
            let moved = (place.latitude - s.2).abs() > 0.01 || (place.longitude - s.3).abs() > 0.01;
            if moved || clock.year != s.1 || !(0.0..=86_400.0).contains(&elapsed) {
                s.0 = WeatherModel::new(clock, &place);
                s.1 = clock.year;
                s.2 = place.latitude;
                s.3 = place.longitude;
            } else if elapsed > 0.0 {
                s.0.step(clock.year, &place, (elapsed / 3600.0) as f32);
            }
            s.4 = (clock.day_of_year, clock.time);
            self.current = Some(s.0.sky());
            Some(s.0.weather())
        }
    }

    /// Two sessions, a stop and a start again, frame steps, minutes, midnight, a jump of
    /// more than a day, a jump back, a new year and a map at another place: the run gives
    /// the weather and the sky the statics gave, bit for bit, at every step.
    #[test]
    fn a_run_steps_as_the_session_statics_did() {
        let berlin = SunPlace::default();
        let elsewhere = SunPlace { latitude: 50.7, longitude: -3.5, ..SunPlace::default() };
        set_place(berlin.clone());
        let mut before = Before::default();
        let mut run: Option<ModelRun>;
        let mut clock = SimClock { year: 2026, day_of_year: 100, time: 6.0 * 3600.0, ..Default::default() };
        let mut steps = 0;
        let check = |before: &mut Before, run: &mut Option<ModelRun>, clock: &SimClock| {
            let old = before.refresh(clock);
            let new = run.as_mut().map(|r| r.refresh(clock));
            assert_eq!(format!("{old:?}"), format!("{new:?}"), "weather at {clock:?}");
            assert_eq!(before.current, run.as_ref().map(|r| r.sky()), "sky at {clock:?}");
        };
        for session in 0..2 {
            let w_old = before.start(&clock);
            let (r, w_new) = ModelRun::start(&clock);
            assert_eq!(format!("{w_old:?}"), format!("{w_new:?}"));
            run = Some(r);
            assert_eq!(before.current, run.as_ref().map(|r| r.sky()));
            // frames of a sixtieth of a second, then of a second, then minutes over midnight
            for (n, dt) in [(120, 1.0 / 60.0), (600, 1.0), (200, 17.0 * 60.0)] {
                for _ in 0..n {
                    clock.time += dt;
                    if clock.time >= 86_400.0 {
                        clock.time -= 86_400.0;
                        clock.day_of_year += 1;
                    }
                    check(&mut before, &mut run, &clock);
                    steps += 1;
                }
            }
            // more than a day on, back an hour, a new year, another place, and back home
            clock.day_of_year += 2;
            check(&mut before, &mut run, &clock);
            clock.time = (clock.time - 3600.0).max(0.0);
            check(&mut before, &mut run, &clock);
            clock.year += 1;
            check(&mut before, &mut run, &clock);
            set_place(elsewhere.clone());
            clock.time += 600.0;
            check(&mut before, &mut run, &clock);
            set_place(berlin.clone());
            clock.time += 600.0;
            check(&mut before, &mut run, &clock);
            steps += 5;
            // the session ends (another weather): no model until the next one starts
            before.stop();
            run = None;
            check(&mut before, &mut run, &clock);
            clock.day_of_year = 200 + session * 90;
        }
        assert!(steps > 1800);
    }
}

#[cfg(test)]
mod report {
    use super::*;

    /// `cargo test -p omsi-app weather_report -- --ignored --nocapture`: day by day.
    #[test]
    #[ignore]
    fn weather_report() {
        let place = omsi_sim::daylight::SunPlace::default();
        for (doy0, label) in [(1, "winter"), (100, "spring"), (180, "summer"), (280, "autumn")] {
            println!("== {label}");
            let clock = omsi_sim::SimClock { year: 2026, day_of_year: doy0, time: 0.0, ..Default::default() };
            let mut m = WeatherModel::new(&clock, &place);
            let mut stats = [0.0f32; 6];
            let mut snow_days = 0;
            for day in 0..30 {
                let mut line = format!("d{day:2}");
                for hour in 0..24 {
                    m.step(2026, &place, 1.0);
                    if hour == 14 {
                        stats[3] = m.temp;
                        stats[4] = m.mixed;
                        stats[5] = 125.0 * (m.temp - dew_point(m.q)).max(0.0);
                    }
                    if hour % 3 == 2 {
                        let c = if m.precip > 0.1 && m.temp < 1.5 { '*' } else if m.precip > 0.1 { 'R' } else if m.fog > 0.4 { 'F' } else if m.deck > 0.85 { '#' } else if m.deck > 0.5 { '=' } else if m.cumulus > 0.45 { 'C' } else if m.cumulus > 0.15 { 'c' } else if m.veil > 0.5 { '~' } else { '.' };
                        line.push(c);
                    }
                    stats[0] += (m.precip > 0.1) as i32 as f32;
                    stats[1] += (m.deck > 0.85) as i32 as f32;
                    stats[2] += (m.fog > 0.4) as i32 as f32;
                    stats[3 + 0] += 0.0;
                    if hour == 12 && m.snow_cover > 0.5 { snow_days += 1; }
                }
                let w = m.weather();
                println!("{line}  14h T {:4.1} mix {:4.0} lcl {:4.0} | 23h T {:5.1} rh {:3.0}% vis {:6.0} haze {:.2} mix {:4.0} rh_up {:.2} p {:6.1}", stats[3], stats[4], stats[5], m.temp, m.humidity() * 100.0, w.fog.0, m.sky().haze, m.mixed, m.rh_up, w.pressure);
                let _ = &w;
            }
            println!("rain {:.0}% grey {:.0}% fog {:.0}% snow-covered days {snow_days}", stats[0] / 7.2, stats[1] / 7.2, stats[2] / 7.2);
        }
    }
}
