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
    /// The engine runs (and with it the heating and the defroster).
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
    /// A bus whose scripts keep their own cabin air (`Cabinair_Temp`, `Cabinair_absHum`; its
    /// heating, fans, vent windows and doors are theirs): the temperature (degC), the
    /// humidity (g/m3) and the air its front heater blows while the fan runs (degC). The
    /// glass then answers to the air the driver sees on the bus's own gauges instead of a
    /// second, guessed cabin.
    pub scripted: Option<(f32, f32, Option<f32>)>,
}

/// Saturation vapour density (g/m³) at `t` °C (Magnus over water, Alduchov and Eskridge).
pub fn saturation(t: f32) -> f32 {
    let e = 6.1094 * (17.625 * t / (t + 243.04)).exp();
    216.7 * e / (t + 273.15)
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
        CabinAir { temp: 0.0, vapour: 0.0, coolant: 0.0, film: [0.0; 3], defrost: 0.0, vehicle: None }
    }

    /// The glass's inner temperature for a pane with `h_in` on its inner side, under air at
    /// `air` °C, with the street at `out` and the airstream at `speed`.
    fn glass_temp(h_in: f32, air: f32, out: f32, speed: f32) -> f32 {
        // (outside: Jürges' convection over a plate, 5.7 + 3.8 v W/m²K; the glass itself is
        // thin enough to leave out)
        let h_out = 5.7 + 3.8 * speed.max(0.0);
        (h_in * air + h_out * out) / (h_in + h_out)
    }

    /// The air the heater core blows out (°C).
    fn heater_outlet(&self, i: &CabinInputs) -> f32 {
        match i.scripted {
            Some((_, _, Some(jet))) => jet.max(self.temp),
            Some(_) => self.temp,
            None => self.temp + HEATER_EFFECTIVENESS * (self.coolant - self.temp).max(0.0),
        }
    }

    /// The water the defroster's jet takes off the windscreen (g/m²s; below 0 it condenses
    /// there too).
    fn defroster_drying(&self, i: &CabinInputs) -> f32 {
        let t = Self::glass_temp(H_DEFROST, self.heater_outlet(i), i.outside, i.speed + 1.0);
        H_DEFROST / AIR_RHO_CP * (saturation(t) - self.vapour)
    }

    /// The windscreen's glass where the defroster blows dries rather than mists.
    #[cfg(test)]
    pub fn defrosted_glass_clear(&self, i: &CabinInputs) -> bool {
        i.engine && self.defroster_drying(i) > 0.0
    }

    /// Carry the state `dt` seconds on.
    pub fn step(&mut self, dt: f32, i: &CabinInputs) {
        let moved = self.vehicle.is_some_and(|(v, p)| v != i.vehicle || (p - i.position).length() > 50.0 + 60.0 * dt.max(0.0) as f64);
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
        let ach = 3.0 + FRESH_AIR * i.people as f32 / volume + 0.35 * i.speed.max(0.0) + 120.0 * i.doors_open as f32;
        let leave = ach / 3600.0;
        if let Some((t, hum, _)) = i.scripted {
            // the bus keeps its own air: take it as it is (what the air holds above its
            // saturation condenses on every cold surface anyway)
            self.temp = t;
            self.vapour = hum.min(saturation(t));
        }
        // the coolant warms with the engine running and cools when it stops
        let (aim, tau) = if i.engine { (COOLANT_HOT, COOLANT_WARMUP) } else { (i.outside, COOLANT_COOLDOWN) };
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
        if i.scripted.is_none() {
            self.temp += (settle - self.temp) * (1.0 - (-dt * loss / capacity).exp());
        }
        // the vapour: the people's, and what leaves with the air
        let source = i.people as f32 * (BREATH + if i.precip { WET_COAT } else { 0.0 }) / 3600.0;
        if i.scripted.is_none() {
            self.vapour += source / volume * dt;
            self.vapour += (out_vapour - self.vapour) * (1.0 - (-dt * leave).exp());
            // (no more than the air holds: the rest condenses on every cold surface)
            self.vapour = self.vapour.min(saturation(self.temp));
        }
        // the panes: windscreen, sides, rear (the airstream meets the windscreen head on,
        // runs along the sides and leaves the rear window in the bus's wake)
        let stream = [1.0, 0.8, 0.35];
        for (k, film) in self.film.iter_mut().enumerate() {
            let t_glass = Self::glass_temp(H_INSIDE, self.temp, i.outside, i.speed * stream[k] + 1.0);
            // mass transfer coefficient by Lewis' analogy: h / (rho c_p) (m/s)
            let flux = H_INSIDE / AIR_RHO_CP * (self.vapour - saturation(t_glass));
            *film = (*film + flux * dt).clamp(0.0, 80.0);
        }
        // the defroster's patch grows as fast as its jet dries the windscreen's film, and
        // closes again as fast as the water condenses where it no longer dries
        let (target, rate) = if i.engine {
            let dry = self.defroster_drying(i);
            (if dry > 0.0 { DEFROST_REACH } else { 0.0 }, dry.abs())
        } else {
            let t_glass = Self::glass_temp(H_INSIDE, self.temp, i.outside, i.speed + 1.0);
            (0.0, (H_INSIDE / AIR_RHO_CP * (self.vapour - saturation(t_glass))).max(0.0))
        };
        self.defrost += (target - self.defrost) * (1.0 - (-dt * rate / self.film[0].max(0.5)).exp());
    }

    /// For the renderer: each pane's film as an optical depth (windscreen, sides, rear: the
    /// share of the light it scatters is 1 - e^-depth) and how far the defroster has
    /// cleared the windscreen (0..1).
    pub fn appearance(&self) -> [f32; 4] {
        [self.film[0] / MILKY, self.film[1] / MILKY, self.film[2] / MILKY, self.defrost]
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
pub fn inputs_for(v: &omsi_sim::VehicleInstance, w: &omsi_content::weather::Weather, riders: usize, doors_open: usize) -> CabinInputs {
    let (kind, rate) = crate::conditions::precip_of(w);
    let [x, y, z] = v.ty.def.bounding_box.map(|bb| [bb[0], bb[1], bb[2]]).unwrap_or([2.5, 12.0, 3.1]);
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
        CabinInputs { outside: 7.0, dew_point: 6.0, speed: 10.0, engine: true, people: 31, doors_open: 0, precip: true, volume: 70.0, skin: 150.0, vehicle: 1, position: glam::DVec3::ZERO, scripted: None }
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
        let c = run(CabinInputs { people: 1, precip: false, dew_point: -2.0, ..inputs() }, 15.0);
        assert!(milky(&c, 1) < 0.2, "{c:?}");
    }

    #[test]
    fn the_heating_warms_the_cabin_in_some_minutes() {
        let c = run(CabinInputs { people: 1, ..inputs() }, 5.0);
        assert!(c.temp < 17.0, "{c:?}");
        let c = run(CabinInputs { people: 1, ..inputs() }, 25.0);
        assert!((c.temp - HEATED).abs() < 2.0, "{c:?}");
    }

    #[test]
    fn open_doors_and_dry_air_clear_the_glass() {
        let mut c = run(inputs(), 15.0);
        let fogged = c.film[1];
        let dry = CabinInputs { people: 1, precip: false, dew_point: -10.0, doors_open: 2, ..inputs() };
        for _ in 0..600 {
            c.step(1.0, &dry);
        }
        assert!(c.film[1] < fogged * 0.5, "{fogged} -> {}", c.film[1]);
    }

    #[test]
    fn the_defroster_clears_the_windscreen() {
        let c = run(inputs(), 15.0);
        assert!(c.defrost > 0.5 && c.defrosted_glass_clear(&inputs()), "{c:?}");
    }

    /// 17 degC in the cabin, 7 outside, the air as damp as the street's plus a few
    /// passengers' breath (the bus's own gauges): the glass stays above the dew point, so
    /// a light mist on it goes within some minutes (slower the colder the airstream
    /// makes the glass), and a clear pane stays clear.
    #[test]
    fn at_17_inside_and_7_outside_the_glass_clears() {
        let street = saturation(6.0);
        for speed in [0.0, 5.0, 12.0] {
            let i = CabinInputs { speed, scripted: Some((17.0, street + 0.3, None)), people: 8, ..inputs() };
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
            let i = CabinInputs { speed: 8.0, scripted: Some((t, street + 1.2, None)), ..inputs() };
            let mut c = CabinAir::new();
            c.step(1.0, &i);
            c.film = [10.0; 3];
            for _ in 0..(10 * 60) {
                c.step(1.0, &i);
            }
            c.film[1]
        };
        assert!(after(21.0) < after(17.0) - 1.0, "{} {}", after(21.0), after(17.0));
    }

    #[test]
    fn another_bus_starts_afresh() {
        let mut c = run(inputs(), 15.0);
        assert!(c.film[1] > 1.0);
        c.step(1.0, &CabinInputs { vehicle: 2, ..inputs() });
        assert!(c.film[1] < 0.1 && c.temp < 8.0, "{c:?}");
        let mut c = run(inputs(), 15.0);
        c.step(1.0, &CabinInputs { position: glam::DVec3::new(500.0, 0.0, 0.0), ..inputs() });
        assert!(c.film[1] < 0.1, "{c:?}");
    }

    #[test]
    fn a_door_that_is_entry_and_exit_counts_once() {
        let d = |x: f32, open| (glam::Vec3::new(x, 5.0, 0.0), glam::DVec3::ZERO, 1.0, open);
        assert_eq!(open_doors(&[d(1.0, true), d(1.3, true), d(1.0, false), d(1.2, true)]), 1);
        assert_eq!(open_doors(&[d(1.0, true), d(-1.0, false)]), 1);
        let far = (glam::Vec3::new(1.0, -4.0, 0.0), glam::DVec3::ZERO, 1.0, true);
        assert_eq!(open_doors(&[d(1.0, true), far]), 2);
    }
}
