//! The road dirt a vehicle gathers as it drives (openOMSI, #1386): what its wheels throw up
//! from a wet road, the film the air behind it lays on its back, the dust of dry roads and
//! the slush and salt of a winter road, each by how far and how fast it drove in what weather,
//! and what the rain washes off again. The enhanced picture lays it over the vehicle where
//! each lands (`omsi_render::VehicleDirt`, the `procedural_dirt` setting); OMSI's own
//! `Dirt_Norm` is kept as it was.

static PROCEDURAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the enhanced picture draws the road dirt (the `procedural_dirt` setting, and an
/// enhanced picture to draw it): then it takes the place of the vehicles' own dirt overlays
/// (`[alphascale] Dirt_Norm`), which are hidden - one of the two shows, never both.
pub fn set_procedural_dirt(on: bool) {
    PROCEDURAL.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn procedural_dirt() -> bool {
    PROCEDURAL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether a body of `def` shows the road dirt in place of its own dirt overlays: not one
/// whose file says `[procedural_dirt_off]`, and only while the picture lays it on its vehicle
/// (`drawn`, `VehicleInstance::road_dirt_drawn`) - a vehicle past the picture's limit keeps
/// its own dirt instead of standing there washed clean.
pub fn procedural_for(def: &omsi_vehicle::Vehicle, drawn: bool) -> bool {
    procedural_dirt() && !def.procedural_dirt_off && drawn
}

/// How dirty the parts of a body are (0..1 each), and of what.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RoadDirt {
    /// Wheel spray: behind and above each wheel, the wheel arches and the lower sides.
    pub spray: f32,
    /// Road film: the lower body all round and the back, which the wake behind the vehicle
    /// coats.
    pub film: f32,
    /// Dust: thin, all over, from dry roads and dry ground.
    pub dust: f32,
    /// What the dirt is made of, as shares of what was laid on: slush and road salt (grey,
    /// white where it dries) ...
    pub slush: f32,
    /// ... and mud (dark brown; off the road, wet earth) - the rest is ordinary road grime.
    pub mud: f32,
    /// How wet it is now (0 dry and matt, 1 freshly sprayed and glossy).
    pub wet: f32,
}

/// What the vehicle drives through this frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct DirtWeather {
    /// Speed (km/h, either way).
    pub kmh: f32,
    /// The road: 0 dry, 1 wet, 2 snow (`StreetCond`).
    pub street: f32,
    /// What falls (0..1) and whether it is snow.
    pub precip: f32,
    pub snow: bool,
    /// The wheels' share off the road (`VehicleHost::offroad`).
    pub offroad: f32,
}

/// Seconds of driving at 50 km/h on a wet road for the spray to cover its zone to 63 %: a
/// wet hour leaves the arches and the lower sides plainly dirty.
const SPRAY_SECS: f32 = 2400.0;
const FILM_SECS: f32 = 4800.0;
/// A dry day's dust: half covered after several hours.
const DUST_SECS: f32 = 30_000.0;
/// Heavy rain washes the exposed dust off in a few minutes, the caked spray far slower.
const WASH_DUST_SECS: f32 = 300.0;
const WASH_SPRAY_SECS: f32 = 5400.0;
/// A wet body dries in ten minutes or so.
const DRY_SECS: f32 = 600.0;

impl RoadDirt {
    /// The road dirt of a body that comes back with OMSI's `Dirt_Norm` of `dirt` (a saved
    /// situation, a depot bus): spread as driving leaves it, so a bus parked dirty comes back
    /// dirty. A new vehicle starts clean.
    pub fn from_dirt_norm(dirt: f32) -> RoadDirt {
        let d = if dirt.is_finite() {
            dirt.clamp(0.0, 1.0)
        } else {
            0.0
        };
        RoadDirt {
            spray: d,
            film: d * 0.8,
            dust: d * 0.4,
            ..Default::default()
        }
    }

    /// Whether nothing is on the body.
    pub fn clean(&self) -> bool {
        self.spray + self.film + self.dust < 1e-3
    }

    /// `dt` seconds of driving through `w`.
    pub fn step(&mut self, dt: f32, w: DirtWeather) {
        self.step_at(dt, w, 1.0);
    }

    /// `step` for a vehicle that gathers dirt `rate` times as fast as any (its
    /// `[procedural_dirt_rate]`); the rain washes it off as fast as any.
    pub fn step_at(&mut self, dt: f32, w: DirtWeather, rate: f32) {
        if !(dt > 0.0) {
            return;
        }
        let rate = if rate.is_finite() { rate.max(0.0) } else { 1.0 };
        let speed = (w.kmh.abs() / 50.0).clamp(0.0, 2.0);
        let wet_road = w.street.clamp(0.0, 1.0).max(w.precip.clamp(0.0, 1.0) * 0.8);
        let snowy = (w.street - 1.0).clamp(0.0, 1.0);
        let off = w.offroad.clamp(0.0, 1.0);
        let dry = (1.0 - wet_road) * (1.0 - snowy);
        // what the wheels throw up: water and grime off a wet road, slush off a snowy one
        // (the most of all), mud off wet ground
        let spray = speed * (wet_road + 1.5 * snowy + 3.0 * off * wet_road.max(0.3));
        let film = speed * (wet_road + snowy);
        let dust = speed * (dry + 4.0 * off * dry);
        let gain = |x: &mut f32, rate: f32, secs: f32| {
            *x += (1.0 - *x) * (1.0 - (-dt * rate / secs).exp());
        };
        let (spray, film, dust) = (spray * rate, film * rate, dust * rate);
        let laid = spray / SPRAY_SECS + film / FILM_SECS + dust / DUST_SECS;
        gain(&mut self.spray, spray, SPRAY_SECS);
        gain(&mut self.film, film, FILM_SECS);
        gain(&mut self.dust, dust, DUST_SECS);
        // what it is made of: the share of each in what is laid on now, mixed into what was
        if laid > 0.0 {
            let amount = self.spray + self.film + self.dust;
            let k = (laid * dt / amount.max(0.05)).clamp(0.0, 1.0);
            let now_slush = (1.5 * snowy) / (wet_road + 1.5 * snowy + 3.0 * off + dry).max(1e-3);
            let now_mud = (3.0 * off) / (wet_road + 1.5 * snowy + 3.0 * off + dry).max(1e-3);
            self.slush += (now_slush.clamp(0.0, 1.0) - self.slush) * k;
            self.mud += (now_mud.clamp(0.0, 1.0) - self.mud) * k;
        }
        // the rain washes: the dust soon, the caked spray and film slowly (the wheels keep
        // laying it on below)
        let rain = if w.snow {
            0.0
        } else {
            w.precip.clamp(0.0, 1.0)
        };
        if rain > 0.0 {
            self.dust *= (-dt * rain / WASH_DUST_SECS).exp();
            self.spray *= (-dt * rain / WASH_SPRAY_SECS).exp();
            self.film *= (-dt * rain / WASH_SPRAY_SECS).exp();
        }
        // wet while it rains or the wheels spray, drying otherwise
        let wetting = (rain + speed * (wet_road + snowy) * 0.5).clamp(0.0, 1.0);
        self.wet += (wetting - self.wet)
            * (1.0 - (-dt / if wetting > self.wet { 20.0 } else { DRY_SECS }).exp());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(minutes: f32, w: DirtWeather) -> RoadDirt {
        let mut d = RoadDirt::default();
        for _ in 0..(minutes * 60.0) as usize {
            d.step(1.0, w);
        }
        d
    }

    #[test]
    fn a_wet_hour_dirties_the_lower_body_a_dry_one_barely() {
        let wet = drive(
            60.0,
            DirtWeather {
                kmh: 40.0,
                street: 1.0,
                ..Default::default()
            },
        );
        let dry = drive(
            60.0,
            DirtWeather {
                kmh: 40.0,
                ..Default::default()
            },
        );
        assert!(wet.spray > 0.6 && wet.film > 0.3, "{wet:?}");
        assert!(dry.spray < 0.01 && dry.dust < 0.15, "{dry:?}");
        assert!(wet.wet > 0.3 && dry.wet < 0.01);
    }

    #[test]
    fn a_bus_comes_back_as_dirty_as_it_was_parked() {
        assert!(RoadDirt::default().clean());
        assert!(RoadDirt::from_dirt_norm(0.0).clean());
        let d = RoadDirt::from_dirt_norm(0.6);
        assert_eq!(d.spray, 0.6);
        assert!(d.film > 0.0 && d.dust > 0.0 && d.wet == 0.0);
    }

    #[test]
    fn standing_still_gathers_nothing_and_rain_washes_dust() {
        let still = drive(
            60.0,
            DirtWeather {
                street: 1.0,
                precip: 0.0,
                ..Default::default()
            },
        );
        assert!(still.clean(), "{still:?}");
        let mut d = RoadDirt {
            dust: 0.5,
            spray: 0.5,
            ..Default::default()
        };
        for _ in 0..600 {
            d.step(
                1.0,
                DirtWeather {
                    precip: 1.0,
                    ..Default::default()
                },
            );
        }
        assert!(d.dust < 0.1, "{d:?}");
        assert!(d.spray > 0.4, "{d:?}");
    }

    #[test]
    fn winter_roads_lay_slush_fields_mud() {
        let winter = drive(
            30.0,
            DirtWeather {
                kmh: 50.0,
                street: 2.0,
                ..Default::default()
            },
        );
        let field = drive(
            10.0,
            DirtWeather {
                kmh: 20.0,
                street: 1.0,
                offroad: 1.0,
                ..Default::default()
            },
        );
        assert!(winter.slush > 0.5 && winter.mud < 0.1, "{winter:?}");
        assert!(field.mud > 0.4, "{field:?}");
    }
}
