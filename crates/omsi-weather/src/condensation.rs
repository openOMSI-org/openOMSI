//! Condensation on the player's bus's windows: the cabin's air and the water it leaves on
//! the glass, worked out as it happens in a real bus.
//!
//! * The cabin's heat: the heating's convectors draw on the engine's coolant, which warms up
//!   over some minutes once the engine runs; the people give off their body heat; the body's
//!   skin and the air that leaves carry heat to the street. The cabin's air and its fittings
//!   (seats, panels, floor) warm and cool together.
//! * Its water vapour comes from the people breathing in it - some 45 g an hour each, the
//!   driver included, and more from wet coats while it rains or snows - and goes with the
//!   air that leaves: the fans' few air changes an hour, the airstream through the vents
//!   while driving, far more through an open door.
//! * A pane's inner face settles between the cabin's air and the street's by the heat each
//!   side carries to it: still air inside (some 8 W/m²K), outside a film that grows with
//!   the speed the bus drives through the air. The windscreen has the heater's warm air
//!   blown up it (the defroster): where the jet's glass is warm enough to dry, it clears
//!   from below as fast as the jet evaporates the film.
//! * Where the glass is colder than the cabin air's dew point, water condenses on it at the
//!   rate the air brings the vapour there (the heat transfer's mass analogue, Lewis'
//!   relation); where it is warmer, the film evaporates again. A few grams a square metre of
//!   fine droplets is a milky film; tens of grams run together into drops.
//!
//! A bus full of wet passengers on an autumn evening fogs up within minutes, its windscreen
//! clear in a widening patch above the defroster; an empty bus on a dry winter's day stays
//! clear, and a cold bus stood with its engine off fogs up as soon as people breathe in it.

use omsi_sim::vehicle::ScriptedCabinAir;

/// The state of the cabin's air and of its glass.
#[derive(Debug, Clone)]
pub struct CabinAir {
    /// The cabin air's temperature (°C) and water vapour density (g/m³).
    pub temp: f32,
    pub vapour: f32,
    /// The engine's coolant (°C), which feeds the heating and the defroster.
    pub coolant: f32,
    /// Water on the inner face of the windscreen, the side windows and the rear window
    /// (g/m²).
    pub film: [f32; 3],
    /// How far the defroster has cleared the windscreen (0..1: the clear patch's size).
    pub defrost: f32,
    /// The vehicle this state belongs to (its type) and where it was last: another bus, or
    /// the same one put somewhere else (a new situation), starts afresh.
    vehicle: Option<(usize, glam::DVec3)>,
}

/// What the cabin's air is exposed to this moment.
#[derive(Debug, Clone, Copy)]
pub struct CabinInputs {
    /// The street's temperature and dew point (°C).
    pub outside: f32,
    pub dew_point: f32,
    /// The bus's speed through the air (m/s).
    pub speed: f32,
    /// The engine runs (powering the built-in heating and defroster if unscripted).
    pub engine: bool,
    /// People aboard, the driver included, and how many doors stand open.
    pub people: usize,
    pub doors_open: usize,
    /// It rains or snows (wet coats).
    pub precip: bool,
    /// The cabin's volume (m³) and the area of the body's skin around it (m²).
    pub volume: f32,
    pub skin: f32,
    /// Which vehicle (its type's address) and where it is.
    pub vehicle: usize,
    pub position: glam::DVec3,
    /// The vehicle's own cabin temperature, humidity when available, and front blower.
    pub scripted: Option<ScriptedCabinAir>,
}

/// Saturation vapour density (g/m³) at `t` °C (Magnus over water, Alduchov and Eskridge).
pub fn saturation(t: f32) -> f32 {
    let e = 6.1094 * (17.625 * t / (t + 243.04)).exp();
    216.7 * e / (t + 273.15)
}

/// Saturation vapour density (g/m³) over ice at `t` °C (Magnus over ice, Alduchov and
/// Eskridge): what glass colder than freezing takes out of the air as frost. Over water the
/// air there would have to hold a fifth more at -10 °C, and a frosty winter's cabin kept its
/// windows clear where they ice over.
pub fn saturation_ice(t: f32) -> f32 {
    let e = 6.1121 * (22.587 * t / (t + 273.86)).exp();
    216.7 * e / (t + 273.15)
}

/// What a surface at `t` °C takes the air's vapour down to: over ice below freezing.
pub fn saturation_on(t: f32) -> f32 {
    if t < 0.0 {
        saturation_ice(t)
    } else {
        saturation(t)
    }
}

/// The dew point (°C) of air holding `vapour` g/m³: the temperature whose saturation it is.
/// (`Weather::temp.1` is the absolute humidity in g/m³, not a temperature.)
pub fn dew_point_of(vapour: f32) -> f32 {
    let (mut lo, mut hi) = (-80.0f32, 60.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if saturation(mid) < vapour {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The heating's thermostat (°C) and the band over which it turns from off to full (K).
const HEATED: f32 = 20.0;
const THERMOSTAT_BAND: f32 = 2.0;
/// A city bus's heating at full power, its convectors and the front box together (W).
const HEATER_POWER: f32 = 25_000.0;
/// The coolant once the engine is warm (°C), and how fast it gets there (s) and loses its
/// heat with the engine off.
const COOLANT_HOT: f32 = 85.0;
const COOLANT_WARMUP: f32 = 400.0;
const COOLANT_COOLDOWN: f32 = 2400.0;
/// The heater core's effectiveness: the air leaves it this far from the cabin's
/// temperature towards the coolant's.
const HEATER_EFFECTIVENESS: f32 = 0.5;
/// Volumetric heat capacity of air (J/m³K), and of the cabin's fittings per cubic metre of
/// cabin (J/m³K: some 600 kg of seats, panels and floor in a 70 m³ saloon).
const AIR_RHO_CP: f32 = 1206.0;
const FITTINGS_CP: f32 = 8600.0;
/// The body skin's mean heat transmittance, glass and insulated panels together (W/m²K).
const SKIN_U: f32 = 3.5;
/// Heat transfer to the glass from still cabin air, from the defroster's jet (W/m²K).
const H_INSIDE: f32 = 8.0;
const H_DEFROST: f32 = 28.0;
/// The share of the windscreen the defroster's jet reaches.
const DEFROST_REACH: f32 = 0.85;
/// Even with hot air, the glass takes time to warm and the clear patch spreads up the pane.
/// This time constant limits a strong jet to 90% of its reach after about 3½ minutes.
const DEFROST_WARMUP: f32 = 90.0;
/// A person's body heat and water vapour from breathing, and more from wet clothes (W, g/h).
const BODY_HEAT: f32 = 100.0;
const BREATH: f32 = 45.0;
const WET_COAT: f32 = 30.0;
/// The fresh air brought in for each person aboard (m3/h).
const FRESH_AIR: f32 = 15.0;
/// The film of fine droplets whose optical depth is 1: some 3 g/m² turns glass milky.
pub const MILKY: f32 = 3.0;

impl CabinAir {
    pub fn new() -> CabinAir {
        CabinAir {
            temp: 0.0,
            vapour: 0.0,
            coolant: 0.0,
            film: [0.0; 3],
            defrost: 0.0,
            vehicle: None,
        }
    }

    /// The glass's inner temperature for a pane with `h_in` on its inner side, under air at
    /// `air` °C, with the street at `out` and the airstream at `speed`.
    fn glass_temp(h_in: f32, air: f32, out: f32, speed: f32) -> f32 {
        // (outside: Jürges' convection over a plate, 5.7 + 3.8 v W/m²K; the glass itself is
        // thin enough to leave out)
        let h_out = 5.7 + 3.8 * speed.max(0.0);
        (h_in * air + h_out * out) / (h_in + h_out)
    }

    /// The water the defroster's jet takes off the windscreen (g/m²s; below 0 it condenses
    /// there too), when air is actually flowing.
    fn defroster_drying(&self, i: &CabinInputs) -> Option<f32> {
        let outlet = match i.scripted {
            Some(air) => air.front_air.map(|t| t.max(self.temp)),
            None => i
                .engine
                .then(|| self.temp + HEATER_EFFECTIVENESS * (self.coolant - self.temp).max(0.0)),
        }?;
        let t = Self::glass_temp(H_DEFROST, outlet, i.outside, i.speed + 1.0);
        Some(H_DEFROST / AIR_RHO_CP * (saturation_on(t) - self.vapour))
    }

    /// The windscreen's glass where the defroster blows dries rather than mists.
    #[cfg(test)]
    pub fn defrosted_glass_clear(&self, i: &CabinInputs) -> bool {
        self.defroster_drying(i).is_some_and(|dry| dry > 0.0)
    }

    /// Carry the state `dt` seconds on.
    pub fn step(&mut self, dt: f32, i: &CabinInputs) {
        let moved = self.vehicle.is_some_and(|(v, p)| {
            v != i.vehicle || (p - i.position).length() > 50.0 + 60.0 * dt.max(0.0) as f64
        });
        if self.vehicle.is_none() || moved {
            // a bus that has stood: as warm and as humid as the street, dry glass
            *self = CabinAir::new();
            self.temp = i.outside;
            self.coolant = i.outside;
            self.vapour = saturation(i.dew_point);
        }
        self.vehicle = Some((i.vehicle, i.position));
        let dt = dt.clamp(0.0, 5.0);
        if dt <= 0.0 {
            return;
        }
        let volume = i.volume.max(10.0);
        let out_vapour = saturation(i.dew_point);
        // air changes an hour: the fans, the airstream through the vents, the doors
        // (the fresh air a city bus is built to bring in is some 15 m3/h a person, on top of
        // the leaks)
        let ach = 3.0
            + FRESH_AIR * i.people as f32 / volume
            + 0.35 * i.speed.max(0.0)
            + 120.0 * i.doors_open as f32;
        let leave = ach / 3600.0;
        // the coolant warms with the engine running and cools when it stops
        let (aim, tau) = if i.engine {
            (COOLANT_HOT, COOLANT_WARMUP)
        } else {
            (i.outside, COOLANT_COOLDOWN)
        };
        self.coolant += (aim - self.coolant) * (1.0 - (-dt / tau).exp());
        // the cabin's heat: the heating (thermostat, as far as the coolant's heat allows),
        // the people, the skin and the air that leaves
        let heat = if i.engine {
            let demand = ((HEATED - self.temp) / THERMOSTAT_BAND).clamp(0.0, 1.0);
            let available = ((self.coolant - self.temp) / (COOLANT_HOT - HEATED)).clamp(0.0, 1.0);
            HEATER_POWER * demand.min(available)
        } else {
            0.0
        };
        let loss = SKIN_U * i.skin + AIR_RHO_CP * volume * leave;
        let capacity = (AIR_RHO_CP + FITTINGS_CP) * volume;
        let gain = heat + BODY_HEAT * i.people as f32;
        // (exact over the step for the losses, which can be quick with the doors open)
        let settle = i.outside + gain / loss;
        if let Some(air) = i.scripted {
            self.temp = air.temperature;
        } else {
            self.temp += (settle - self.temp) * (1.0 - (-dt * loss / capacity).exp());
        }
        // the vapour: the people's, and what leaves with the air
        // (in a frost the snow and the slush on the boots and the coats melt in the warm
        // cabin as the falling snow's does)
        let wet = i.precip || i.outside < 2.0;
        let source = i.people as f32 * (BREATH + if wet { WET_COAT } else { 0.0 }) / 3600.0;
        if let Some(humidity) = i.scripted.and_then(|air| air.humidity) {
            self.vapour = humidity;
        } else {
            self.vapour += source / volume * dt;
            self.vapour += (out_vapour - self.vapour) * (1.0 - (-dt * leave).exp());
        }
        // No more than the air holds: the rest condenses on cold surfaces.
        self.vapour = self.vapour.min(saturation(self.temp));
        // the panes: windscreen, sides, rear (the airstream meets the windscreen head on,
        // runs along the sides and leaves the rear window in the bus's wake)
        let stream = [1.0, 0.8, 0.35];
        for (k, film) in self.film.iter_mut().enumerate() {
            let t_glass =
                Self::glass_temp(H_INSIDE, self.temp, i.outside, i.speed * stream[k] + 1.0);
            // mass transfer coefficient by Lewis' analogy: h / (rho c_p) (m/s)
            let flux = H_INSIDE / AIR_RHO_CP * (self.vapour - saturation_on(t_glass));
            *film = (*film + flux * dt).clamp(0.0, 80.0);
        }
        // the defroster's patch grows as fast as its jet dries the windscreen's film, and
        // closes again as fast as the water condenses where it no longer dries
        let film = self.film[0].max(0.5);
        let (target, rate) = if let Some(dry) = self.defroster_drying(i) {
            if dry > 0.0 {
                (DEFROST_REACH, (dry / film).min(1.0 / DEFROST_WARMUP))
            } else {
                (0.0, -dry / film)
            }
        } else {
            let t_glass = Self::glass_temp(H_INSIDE, self.temp, i.outside, i.speed + 1.0);
            (
                0.0,
                (H_INSIDE / AIR_RHO_CP * (self.vapour - saturation_on(t_glass))).max(0.0) / film,
            )
        };
        self.defrost += (target - self.defrost) * (1.0 - (-dt * rate).exp());
    }

    /// For the renderer: each pane's film as an optical depth (windscreen, sides, rear: the
    /// share of the light it scatters is 1 - e^-depth) and how far the defroster has
    /// cleared the windscreen (0..1).
    pub fn appearance(&self) -> [f32; 4] {
        [
            self.film[0] / MILKY,
            self.film[1] / MILKY,
            self.film[2] / MILKY,
            self.defrost,
        ]
    }
}

impl Default for CabinAir {
    fn default() -> Self {
        Self::new()
    }
}

/// How many doors stand open among `doors` (`humans::cabin_doors`): a door that is both an
/// entry and an exit, its points a step apart, is one door.
pub fn open_doors(doors: &[(glam::Vec3, glam::DVec3, f32, bool)]) -> usize {
    let mut seen: Vec<glam::Vec3> = Vec::new();
    for d in doors.iter().filter(|d| d.3) {
        if !seen.iter().any(|s| s.distance(d.0) < 1.2) {
            seen.push(d.0);
        }
    }
    seen.len()
}

/// The inputs for the player's vehicle `v` under weather `w`: the street's temperature and
/// dew point, the speed, the engine, the people aboard (the driver and `riders`), the open
/// doors and the cabin (its `[boundingbox]`: the volume less the seats and the walls, the
/// skin around it).
pub fn inputs_for(
    v: &omsi_sim::VehicleInstance,
    w: &omsi_content::weather::Weather,
    riders: usize,
    doors_open: usize,
) -> CabinInputs {
    let (kind, rate) = crate::conditions::precip_of(w);
    let [x, y, z] =
        v.ty.def
            .bounding_box
            .map(|bb| [bb[0], bb[1], bb[2]])
            .unwrap_or([2.5, 12.0, 3.1]);
    CabinInputs {
        outside: w.temp.0,
        dew_point: dew_point_of(w.temp.1.max(0.0)).min(w.temp.0),
        speed: crate::conditions::vehicle_velocity(v).length(),
        engine: omsi_sim::startup::engine_running(v),
        people: riders + 1,
        doors_open,
        precip: kind != 0 && rate > 0.05,
        volume: x * y * z * 0.75,
        skin: 2.0 * (x * y + y * z + z * x),
        vehicle: std::sync::Arc::as_ptr(&v.ty) as usize,
        position: v.position,
        scripted: v.scripted_cabin_air(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(i: CabinInputs, minutes: f32) -> CabinAir {
        let mut c = CabinAir::new();
        for _ in 0..(minutes * 60.0) as usize {
            c.step(1.0, &i);
        }
        c
    }

    fn inputs() -> CabinInputs {
        CabinInputs {
            outside: 7.0,
            dew_point: 6.0,
            speed: 10.0,
            engine: true,
            people: 31,
            doors_open: 0,
            precip: true,
            volume: 70.0,
            skin: 150.0,
            vehicle: 1,
            position: glam::DVec3::ZERO,
            scripted: None,
        }
    }

    fn milky(c: &CabinAir, k: usize) -> f32 {
        1.0 - (-c.appearance()[k]).exp()
    }

    #[test]
    fn the_dew_point_inverts_the_saturation() {
        for t in [-20.0, -5.0, 0.0, 12.0, 30.0] {
            assert!((dew_point_of(saturation(t)) - t).abs() < 0.05, "{t}");
        }
    }

    #[test]
    fn a_full_bus_on_a_wet_autumn_evening_fogs_up() {
        let c = run(inputs(), 15.0);
        assert!(milky(&c, 1) > 0.5, "{c:?}");
    }

    #[test]
    fn an_empty_bus_stays_clear() {
        let c = run(
            CabinInputs {
                people: 1,
                precip: false,
                dew_point: -2.0,
                ..inputs()
            },
            15.0,
        );
        assert!(milky(&c, 1) < 0.2, "{c:?}");
    }

    #[test]
    fn the_heating_warms_the_cabin_in_some_minutes() {
        let c = run(
            CabinInputs {
                people: 1,
                ..inputs()
            },
            5.0,
        );
        assert!(c.temp < 17.0, "{c:?}");
        let c = run(
            CabinInputs {
                people: 1,
                ..inputs()
            },
            25.0,
        );
        assert!((c.temp - HEATED).abs() < 2.0, "{c:?}");
    }

    #[test]
    fn open_doors_and_dry_air_clear_the_glass() {
        let mut c = run(inputs(), 15.0);
        let fogged = c.film[1];
        let dry = CabinInputs {
            people: 1,
            precip: false,
            dew_point: -10.0,
            doors_open: 2,
            ..inputs()
        };
        for _ in 0..600 {
            c.step(1.0, &dry);
        }
        assert!(c.film[1] < fogged * 0.5, "{fogged} -> {}", c.film[1]);
    }

    #[test]
    fn the_defroster_clears_the_windscreen() {
        let c = run(inputs(), 15.0);
        assert!(
            c.defrost > 0.5 && c.defrosted_glass_clear(&inputs()),
            "{c:?}"
        );
    }

    #[test]
    fn scripted_cabin_can_defog_with_simulated_humidity() {
        let mut i = CabinInputs {
            people: 1,
            precip: false,
            scripted: Some(ScriptedCabinAir {
                temperature: 7.0,
                humidity: None,
                front_air: None,
            }),
            ..inputs()
        };
        let mut c = run(i, 2.0);
        assert_eq!(c.defrost, 0.0);
        c.film = [3.0; 3];
        i.scripted = Some(ScriptedCabinAir {
            temperature: 21.0,
            humidity: None,
            front_air: Some(48.0),
        });
        for _ in 0..600 {
            c.step(1.0, &i);
        }
        assert_eq!(c.temp, 21.0);
        assert!(c.film.iter().all(|film| *film < 1.0), "{c:?}");
        assert!(c.defrost > 0.5, "{c:?}");
    }

    #[test]
    fn a_hot_jet_takes_minutes_to_clear_a_fogged_windscreen() {
        let mut i = CabinInputs {
            outside: 1.0,
            dew_point: 1.0,
            speed: 0.0,
            scripted: Some(ScriptedCabinAir {
                temperature: 10.0,
                humidity: Some(8.0),
                front_air: None,
            }),
            ..inputs()
        };
        let mut c = run(i, 8.0);
        assert!(c.film[0] > MILKY && c.defrost == 0.0, "{c:?}");
        i.scripted.as_mut().unwrap().front_air = Some(48.0);
        for _ in 0..60 {
            c.step(1.0, &i);
        }
        assert!(c.defrost < DEFROST_REACH * 0.55, "{c:?}");
        for _ in 60..270 {
            c.step(1.0, &i);
        }
        assert!(c.defrost > DEFROST_REACH * 0.9, "{c:?}");
    }

    /// 17 degC in the cabin, 7 outside, the air as damp as the street's plus a few
    /// passengers' breath (the bus's own gauges): the glass stays above the dew point, so
    /// a light mist on it goes within some minutes (slower the colder the airstream
    /// makes the glass), and a clear pane stays clear.
    #[test]
    fn at_17_inside_and_7_outside_the_glass_clears() {
        let street = saturation(6.0);
        for speed in [0.0, 5.0, 12.0] {
            let i = CabinInputs {
                speed,
                scripted: Some(ScriptedCabinAir {
                    temperature: 17.0,
                    humidity: Some(street + 0.3),
                    front_air: None,
                }),
                people: 8,
                ..inputs()
            };
            let mut c = CabinAir::new();
            c.step(1.0, &i);
            c.film = [4.0; 3];
            for _ in 0..(25 * 60) {
                c.step(1.0, &i);
            }
            println!("v{speed} film {:?}", c.film);
            assert!(c.film.iter().all(|f| *f < 1.0), "v{speed}: {c:?}");
        }
    }

    /// The same cabin with the bus's heating up (21 degC): it clears sooner than at 17.
    #[test]
    fn a_warmer_cabin_clears_sooner() {
        let street = saturation(6.0);
        let after = |t: f32| {
            let i = CabinInputs {
                speed: 8.0,
                scripted: Some(ScriptedCabinAir {
                    temperature: t,
                    humidity: Some(street + 1.2),
                    front_air: None,
                }),
                ..inputs()
            };
            let mut c = CabinAir::new();
            c.step(1.0, &i);
            c.film = [10.0; 3];
            for _ in 0..(10 * 60) {
                c.step(1.0, &i);
            }
            c.film[1]
        };
        assert!(
            after(21.0) < after(17.0) - 1.0,
            "{} {}",
            after(21.0),
            after(17.0)
        );
    }

    /// -16 degC outside, the bus's own script keeping its saloon at 15 degC with some 3 g/m3
    /// of the passengers' breath in it: at 30 km/h the cold glass frosts over within some
    /// minutes (as ice: over water it took twice as long), standing still it stays clear.
    #[test]
    fn a_heated_bus_in_a_hard_frost_ices_its_windows_while_it_drives() {
        let cold = CabinInputs {
            outside: -16.0,
            dew_point: -25.0,
            precip: false,
            people: 15,
            scripted: Some(ScriptedCabinAir {
                temperature: 15.0,
                humidity: Some(3.0),
                front_air: None,
            }),
            ..inputs()
        };
        let film_after = |speed: f32| {
            let mut c = CabinAir::new();
            for _ in 0..(12 * 60) {
                c.step(1.0, &CabinInputs { speed, ..cold });
            }
            c.film[1]
        };
        assert!(film_after(8.0) > MILKY, "{}", film_after(8.0));
        assert!(film_after(0.0) < 0.5, "{}", film_after(0.0));
        assert!(saturation_ice(-10.0) < saturation(-10.0) * 0.95);
    }

    #[test]
    fn another_bus_starts_afresh() {
        let mut c = run(inputs(), 15.0);
        assert!(c.film[1] > 1.0);
        c.step(
            1.0,
            &CabinInputs {
                vehicle: 2,
                ..inputs()
            },
        );
        assert!(c.film[1] < 0.1 && c.temp < 8.0, "{c:?}");
        let mut c = run(inputs(), 15.0);
        c.step(
            1.0,
            &CabinInputs {
                position: glam::DVec3::new(500.0, 0.0, 0.0),
                ..inputs()
            },
        );
        assert!(c.film[1] < 0.1, "{c:?}");
    }

    #[test]
    fn a_door_that_is_entry_and_exit_counts_once() {
        let d = |x: f32, open| (glam::Vec3::new(x, 5.0, 0.0), glam::DVec3::ZERO, 1.0, open);
        assert_eq!(
            open_doors(&[d(1.0, true), d(1.3, true), d(1.0, false), d(1.2, true)]),
            1
        );
        assert_eq!(open_doors(&[d(1.0, true), d(-1.0, false)]), 1);
        let far = (
            glam::Vec3::new(1.0, -4.0, 0.0),
            glam::DVec3::ZERO,
            1.0,
            true,
        );
        assert_eq!(open_doors(&[d(1.0, true), far]), 2);
    }
}
