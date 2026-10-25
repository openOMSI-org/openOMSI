//! The ambience heard around the listener, from real recordings: the place (the town, the
//! courtyard, the wood, the field, the water, the far motorway and railway), the weather
//! (rain on what it falls on, snow, wind in what it blows through, thunder), the creatures
//! of the season and the hour, the life of the town, and the objects the map has (bells,
//! schools, shops, stations …) at their own times.
//!
//! Only what the map says is there is heard - its houses, trees, lanes, tracks, wires and
//! water, and the objects its authors filed as what they are (see `catalog`). The map's own
//! sounds (`Sounds\rain_outside.wav`, its ambient sound objects, an object's `[sound]`) keep
//! playing; ours give way to them.

pub use omsi_soundscape::{pack, place, rules};
use omsi_soundscape::voices;

use glam::DVec3;
use omsi_audio::AudioEngine;
use pack::Pack;
use place::Place;
use rules::{Ctx, Where};
use voices::{Layer, Placing};

/// What the frame hands over.
pub struct Moment<'a> {
    pub world: Option<&'a crate::scene::World>,
    pub weather: Option<&'a omsi_content::weather::Weather>,
    pub clock: &'a omsi_sim::SimClock,
    /// The sun's height (degrees).
    pub sun: f32,
    /// How wet the roads are (0 … 1).
    pub wetness: f32,
    pub ear: DVec3,
    /// In a vehicle's cab or saloon, and how open it is to the street (its
    /// `Snd_OutsideVol`: doors and windows).
    pub inside: bool,
    pub open: f32,
    /// People walking near the listener.
    pub people: u32,
    pub paused: bool,
    pub dt: f32,
}

/// Seconds between two looks at what stands around (or when the listener jumped).
const LOOK_EVERY: f32 = 0.5;
/// One-shots heard at once at most.
const MAX_EVENTS_AT_ONCE: usize = 6;

pub struct Soundscape {
    pub enabled: bool,
    pub volume: f32,
    pack: Option<Pack>,
    looked: Option<(DVec3, f32)>,
    place: Place,
    lht: Option<bool>,
    /// The layers by slot (around the listener) and the objects' loops by (slot, object).
    layers: hashbrown::HashMap<&'static str, Layer>,
    spots: hashbrown::HashMap<(&'static str, i64), Layer>,
    /// One-shots waiting for their clip, and the times already rung today.
    pending: Vec<(&'static str, f32, Placing, f64)>,
    fired: hashbrown::HashSet<(i64, u32, i32)>,
    /// A church striking the hour: the strokes still to come (slot, placing, when).
    strokes: Vec<(&'static str, Placing, f64)>,
    last_take: hashbrown::HashMap<String, usize>,
    /// The one-shots started since the last debug line.
    heard: Vec<&'static str>,
    /// How wet the trees are after rain (0 … 1).
    wet_trees: f32,
    now: f64,
    rng: u64,
    /// What is heard (for `OMSI_DEBUG_SOUND` and the debug line).
    pub last: String,
}

impl Soundscape {
    pub fn new(enabled: bool, volume: f32) -> Soundscape {
        let pack = Pack::find();
        if pack.is_none() {
            log::info!("ambience: no recordings found (an `ambience` folder beside the program, ~/.openomsi/ambience or OMSI_AMBIENCE_DIR)");
            if enabled {
                Pack::fetch_if_missing();
            }
        }
        Soundscape {
            enabled,
            volume,
            pack,
            looked: None,
            place: Place::default(),
            lht: None,
            layers: hashbrown::HashMap::new(),
            spots: hashbrown::HashMap::new(),
            pending: Vec::new(),
            fired: hashbrown::HashSet::new(),
            strokes: Vec::new(),
            last_take: hashbrown::HashMap::new(),
            heard: Vec::new(),
            wet_trees: 0.0,
            now: 0.0,
            rng: 0x9E37_79B9_7F4A_7C15,
            last: String::new(),
        }
    }

    /// A lightning strike the picture showed: its thunder is heard `delay` seconds later (the
    /// speed of sound over `distance` metres), from where the flash was (`dir`).
    pub fn thunder(&mut self, delay: f32, distance: f32, dir: [f32; 2], ear: DVec3) {
        if !self.active() {
            return;
        }
        let slot = if distance < 1500.0 { "thunder.close" } else if distance < 4000.0 { "thunder.mid" } else { "thunder.far" };
        let at = ear + DVec3::new((dir[0] * distance) as f64, (dir[1] * distance) as f64, 800.0);
        let placing = Placing { at: Some(at), range: reference(slot), lowpass: air(at, ear) };
        self.strokes.push((slot, placing, self.now + delay as f64));
    }

    /// Whether there are recordings to play (the map's own rain then gives way to ours).
    pub fn active(&self) -> bool {
        self.enabled && self.pack.is_some()
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// One frame.
    pub fn update(&mut self, engine: &AudioEngine, m: Moment) {
        let Some(mut pack) = self.pack.take() else { return };
        if !self.enabled || !engine.enabled || m.paused {
            self.silence(engine);
            self.pack = Some(pack);
            return;
        }
        self.now += m.dt as f64;
        let dt = m.dt;
        // what stands around: twice a second, or at once after a jump
        let again = self.looked.is_none_or(|(at, t)| t >= LOOK_EVERY || (at - m.ear).length() > 30.0);
        if again {
            if let Some(w) = m.world {
                self.place = w.sound_place(m.ear);
                if omsi_cfg::flags::OMSI_DEBUG_SOUND.is_set() && self.looked.is_none() {
                    for (k, at, id) in &self.place.spots {
                        log::info!("ambience: {} #{id} at ({:.0}, {:.0}, {:.0}), {:.0} m", k.name(), at.x, at.y, at.z, (*at - m.ear).length());
                    }
                }
                if self.lht.is_none() {
                    let uk = w.global.left_hand_traffic;
                    self.lht = Some(uk);
                    pack.set_region(if uk { "uk" } else { "de" });
                }
            }
            self.looked = Some((m.ear, 0.0));
        } else if let Some((_, t)) = self.looked.as_mut() {
            *t += dt;
        }
        let place = std::mem::take(&mut self.place);
        let c = self.ctx(&m, &place);
        // through the bodywork: quieter and dull, unless the doors stand open
        let open = if m.inside { m.open.clamp(0.0, 1.0) } else { 1.0 };
        let through = 0.22 + 0.78 * open;
        let lowpass = if open < 0.95 { 500.0 + 6000.0 * open * open } else { 0.0 };
        let master = self.volume * through;
        // ---- the layers around
        let mut wanted: hashbrown::HashMap<&'static str, (f32, Option<(DVec3, f32)>)> = hashbrown::HashMap::new();
        for (slot, w, from) in rules::beds(&c) {
            if pack.has(slot) {
                wanted.insert(slot, (w, from));
            }
        }
        for slot in wanted.keys() {
            self.layers.entry(slot).or_insert_with(|| Layer::new(slot));
        }
        let r = self.rand();
        for (slot, layer) in self.layers.iter_mut() {
            let (w, from) = wanted.get(slot).copied().unwrap_or((0.0, None));
            layer.target = w * master;
            layer.placing = Placing { at: from.map(|f| f.0), range: from.map(|f| f.1).unwrap_or(1.0), lowpass };
            layer.tick(engine, &pack, self.now, dt, level(slot), r);
        }
        self.layers.retain(|_, l| !l.is_silent());
        // ---- the objects' loops
        let mut loops = rules::spot_loops(&c);
        // (the nearest few of each kind: a row of lamps hums from the lamp one stands under)
        loops.sort_by(|a, b| (a.1 - m.ear).length().total_cmp(&(b.1 - m.ear).length()));
        let mut per_slot: hashbrown::HashMap<&'static str, usize> = hashbrown::HashMap::new();
        let mut want_spots: hashbrown::HashMap<(&'static str, i64), (DVec3, f32, f32)> = hashbrown::HashMap::new();
        let mut taken: Vec<(&'static str, DVec3)> = Vec::new();
        for (slot, at, range, w, id) in loops {
            let n = per_slot.entry(slot).or_insert(0);
            if *n >= 3 || !pack.has(slot) {
                continue;
            }
            // (one building of several parts - a supermarket, a station - sounds once)
            if taken.iter().any(|(s, p)| *s == slot && (*p - at).length() < 60.0) {
                continue;
            }
            taken.push((slot, at));
            *n += 1;
            want_spots.insert((slot, id), (at, range, w));
        }
        for key in want_spots.keys() {
            self.spots.entry(*key).or_insert_with(|| Layer::new(key.0));
        }
        for (key, layer) in self.spots.iter_mut() {
            let (at, range, w) = want_spots.get(key).copied().unwrap_or((layer.placing.at.unwrap_or(m.ear), layer.placing.range, 0.0));
            layer.target = w * master;
            layer.placing = Placing { at: Some(at), range, lowpass: lowpass.max(air(at, m.ear)) };
            layer.tick(engine, &pack, self.now, dt, level(key.0), r);
        }
        self.spots.retain(|_, l| !l.is_silent());
        // ---- one-shots
        self.events(&pack, &c, master, lowpass, dt);
        self.timed(&pack, &c, master, lowpass);
        self.flush(engine, &pack);
        // (only read for OMSI_DEBUG_SOUND's lines: written out every frame it was a dozen
        // strings made for nobody)
        if omsi_cfg::flags::OMSI_DEBUG_SOUND.is_set() {
            self.last = self.describe(&c);
        }
        self.place = place;
        self.pack = Some(pack);
    }

    fn ctx<'p>(&mut self, m: &Moment, place: &'p Place) -> Ctx<'p> {
        let (kind, rate) = m.weather.map(crate::weather_setup::precip_of).unwrap_or((0, 0.0));
        let rain = if kind == 1 { rate } else { 0.0 };
        // the trees take up rain and drip it off over some twenty minutes after
        self.wet_trees = if rain > 0.05 { (self.wet_trees + m.dt * rain / 120.0).min(1.0) } else { (self.wet_trees - m.dt / 1200.0).max(0.0) };
        // a thunderstorm: towering clouds over a warm shower (thunder in a snowfall is a rarity
        // not worth making up)
        // (its thunder follows the flashes the picture shows, `Soundscape::thunder`: the random
        // peals the rules would add are left out)
        let storm = false;
        let (d, mo) = m.clock.day_month();
        let h = m.clock.hour();
        let day = m.world.map(|w| w.day_kind(m.clock)).unwrap_or(crate::scene::DayKind { workday: m.clock.weekday() < 5, holiday: false, school_holiday: false });
        Ctx {
            place,
            ear: m.ear,
            hour: h.rem_euclid(24.0),
            doy: m.clock.day_of_year as f32,
            weekday: m.clock.weekday(),
            day,
            sun: m.sun,
            rain,
            snow: if kind == 2 { rate } else { 0.0 },
            temp: m.weather.map(|w| w.temp.0).unwrap_or(15.0),
            wind: m.weather.map(|w| w.wind.1).unwrap_or(0.0),
            snow_lying: m.weather.is_some_and(|w| w.snow || w.snow_on_road),
            wetness: m.wetness,
            storm,
            wet_trees: self.wet_trees,
            uk: self.lht.unwrap_or(false),
            people: m.people,
            new_year: (mo == 12 && d == 31 && h >= 18.0) || (mo == 1 && d == 1 && h < 2.5),
        }
    }

    /// The one-shots of the creatures, the weather, the far life and the objects.
    fn events(&mut self, pack: &Pack, c: &Ctx, master: f32, lowpass: f32, dt: f32) {
        let mut due: Vec<(&'static str, Placing)> = Vec::new();
        for (slot, per_minute, w) in rules::events(c) {
            if !pack.has(slot) || self.rand() >= per_minute * density(slot) * dt / 60.0 {
                continue;
            }
            let at = match w {
                Where::At(p) => p,
                Where::Around { near, far, up } => {
                    let a = self.rand() * std::f32::consts::TAU;
                    let d = near + (far - near) * self.rand();
                    c.ear + DVec3::new((a.sin() * d) as f64, (a.cos() * d) as f64, (up * (0.5 + self.rand())) as f64)
                }
            };
            due.push((slot, Placing { at: Some(at), range: reference(slot), lowpass: lowpass.max(air(at, c.ear)) }));
        }
        for (slot, per_minute, at, range) in rules::spot_events(c) {
            if pack.has(slot) && self.rand() < per_minute * density(slot) * dt / 60.0 {
                due.push((slot, Placing { at: Some(at), range, lowpass: lowpass.max(air(at, c.ear)) }));
            }
        }
        for (slot, placing) in due {
            if self.pending.len() < MAX_EVENTS_AT_ONCE {
                self.pending.push((slot, master, placing, self.now + 30.0));
            }
        }
    }

    /// The bells and the school gongs at their minute (each once).
    fn timed(&mut self, pack: &Pack, c: &Ctx, master: f32, lowpass: f32) {
        for t in rules::timed(c) {
            if !pack.has(t.slot) || !self.fired.insert((t.key.0, t.key.1, c.doy as i32)) {
                continue;
            }
            let placing = Placing { at: Some(t.at), range: t.range, lowpass: lowpass.max(air(t.at, c.ear)) };
            if t.strokes > 1 || t.slot == "church.strike" {
                for k in 0..t.strokes {
                    self.strokes.push((t.slot, placing, self.now + k as f64 * 2.6));
                }
            } else {
                self.pending.push((t.slot, master, placing, self.now + 30.0));
            }
        }
        // (a new day: yesterday's are forgotten)
        if self.fired.len() > 4096 {
            let today = c.doy as i32;
            self.fired.retain(|k| k.2 == today);
        }
        let now = self.now;
        let (ready, later): (Vec<_>, Vec<_>) = self.strokes.drain(..).partition(|s| s.2 <= now);
        self.strokes = later;
        for (slot, placing, _) in ready {
            self.pending.push((slot, master, placing, now + 10.0));
        }
    }

    /// Start the one-shots whose clips are read (they are read in the background the first
    /// time); one waiting too long is dropped.
    fn flush(&mut self, engine: &AudioEngine, pack: &Pack) {
        let pending = std::mem::take(&mut self.pending);
        for (slot, gain, placing, until) in pending {
            let r = self.rand();
            if voices::one_shot(engine, pack, slot, level(slot), gain, placing, r, &mut self.last_take) {
                if self.heard.len() < 64 {
                    self.heard.push(slot);
                }
                continue;
            }
            if self.now < until {
                self.pending.push((slot, gain, placing, until));
            }
        }
    }

    fn silence(&mut self, engine: &AudioEngine) {
        for l in self.layers.values_mut().chain(self.spots.values_mut()) {
            l.stop(engine);
        }
        self.layers.clear();
        self.spots.clear();
        self.pending.clear();
        self.strokes.clear();
    }

    /// What was heard lately (for the debug line), and forget it.
    pub fn take_heard(&mut self) -> String {
        let mut h = std::mem::take(&mut self.heard);
        h.sort();
        let mut out: Vec<String> = Vec::new();
        for g in h.chunk_by(|a, b| a == b) {
            out.push(if g.len() > 1 { format!("{} x{}", g[0], g.len()) } else { g[0].to_string() });
        }
        out.join(", ")
    }

    fn describe(&self, c: &Ctx) -> String {
        let mut beds: Vec<(&str, f32)> = self.layers.iter().map(|(s, l)| (*s, l.gain())).filter(|(_, g)| *g > 0.005).collect();
        beds.sort_by(|a, b| b.1.total_cmp(&a.1));
        let spots: Vec<String> = self.spots.keys().map(|(s, id)| format!("{s}#{id}")).collect();
        let mut known: Vec<&str> = c.place.spots.iter().map(|(k, _, _)| k.name()).collect();
        known.sort();
        known.dedup();
        let p = c.place;
        format!(
            "day {:.0} {:02.0}:{:02.0} wd {}{}, sun {:.0}, rain {:.2} snow {:.2} wind {:.1} temp {:.0}; ear ({:.0}, {:.0}, {:.0}), urban {:.2} h {:.0} m, trees {:.2}/{:.2}, open {:.2}, water {}, roads {:.2}/{:.2}, tracks {}, wire {}, tunnel {}, {} objects ({}); layers {}; objects {}; waiting {}",
            c.doy,
            c.hour.floor(),
            (c.hour.fract() * 60.0).floor(),
            c.weekday,
            if c.new_year { " new year" } else { "" },
            c.sun,
            c.rain,
            c.snow,
            c.wind,
            c.temp,
            c.ear.x,
            c.ear.y,
            c.ear.z,
            p.urban,
            p.height,
            p.deciduous,
            p.conifer,
            p.open,
            p.water.map(|w| format!("{:.0} m at ({:.0}, {:.0})", w.0, w.1.x, w.1.y)).unwrap_or("-".into()),
            p.fast_roads,
            p.far_roads,
            p.tracks,
            p.wire.map(|w| format!("{w:.0} m")).unwrap_or("-".into()),
            p.tunnel,
            p.spots.len(),
            known.join(", "),
            beds.iter().map(|(s, g)| format!("{s} {:.0} dB", 20.0 * g.max(1.0e-6).log10())).collect::<Vec<_>>().join(", "),
            spots.join(", "),
            self.pending.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")
        )
    }
}

/// `OMSI_AMBIENCE_DENSITY`: how many times as often a one-shot of `slot` comes (1 unset).
fn density(slot: &str) -> f32 {
    let Some(v) = omsi_cfg::flags::OMSI_AMBIENCE_DENSITY.var() else { return 1.0 };
    if let Ok(all) = v.trim().parse::<f32>() {
        return all.max(0.0);
    }
    v.split(',')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| k.trim() == slot)
        .and_then(|(_, f)| f.trim().parse::<f32>().ok())
        .unwrap_or(1.0)
        .max(0.0)
}

/// How loud a slot is heard (LUFS) at full weight - for a layer around the listener - or at
/// its reference distance (`reference`) for a sound placed somewhere.
pub fn level(slot: &str) -> f32 {
    match slot {
        // places: a far, even wash under everything
        "place.city_day" => -31.0,
        "place.city_night" => -36.0,
        "place.courtyard_day" | "place.suburb" => -35.0,
        "place.courtyard_night" | "place.village" => -39.0,
        "place.motorway" => -33.0,
        "place.arterial" => -36.0,
        "place.railway" | "place.industrial" => -35.0,
        "place.tunnel" => -38.0,
        // nature
        "nature.dawn_urban" | "nature.dawn_wood" => -31.0,
        "nature.wood_deciduous" | "nature.wood_conifer" | "nature.field_summer" | "nature.garden" => -34.0,
        "nature.wood_winter" | "nature.field_autumn" | "nature.night_country" => -40.0,
        "nature.night_crickets" | "nature.frogs" => -33.0,
        "nature.water" => -35.0,
        "nature.bees" => -40.0,
        // weather: rain is loud, close and everywhere
        "rain.light" => -32.0,
        "rain.moderate" => -27.0,
        "rain.heavy" => -23.0,
        "rain.asphalt" | "rain.foliage" => -30.0,
        "rain.puddles" | "rain.downpipe" | "rain.drain" => -35.0,
        "rain.drips" => -36.0,
        "rain.shelter" => -28.0,
        "snow.light" => -44.0,
        "snow.wet" => -34.0,
        "snow.blizzard" => -26.0,
        "hail" => -24.0,
        "wind.light" => -38.0,
        "wind.moderate" => -32.0,
        "wind.strong" => -26.0,
        "wind.deciduous" | "wind.conifer" => -31.0,
        "wind.bare" | "wind.leaves_ground" => -35.0,
        "wind.wires" => -36.0,
        "crowd.de" | "crowd.en" => -36.0,
        // placed: at their reference distance
        s if s.starts_with("thunder.") => -14.0,
        s if s.starts_with("bird.") => -26.0,
        "animal.dog" | "animal.rooster" | "animal.cow" => -22.0,
        "animal.frog" => -30.0,
        "far.aircraft" | "far.helicopter" => -26.0,
        s if s.starts_with("far.siren") => -20.0,
        "far.fireworks" => -16.0,
        s if s.starts_with("far.") => -26.0,
        s if s.starts_with("church.") => -21.0,
        s if s.starts_with("school.") => -24.0,
        s if s.starts_with("station.") => -28.0,
        "hum.substation" | "hum.lamp" => -38.0,
        "ped.blind_de" | "ped.puffin_uk" => -30.0,
        _ => -30.0,
    }
}

/// The distance (m) at which a placed one-shot of `slot` is heard at its `level`; farther,
/// it falls off as 1/d.
fn reference(slot: &str) -> f32 {
    match slot {
        s if s.starts_with("thunder.") => 1000.0,
        s if s.starts_with("bird.") => 15.0,
        "far.aircraft" => 1500.0,
        "far.helicopter" => 300.0,
        s if s.starts_with("far.siren") => 200.0,
        "far.fireworks" => 300.0,
        "far.train" | "far.rail_clatter" => 60.0,
        "animal.dog" | "animal.rooster" | "animal.cow" => 30.0,
        _ => 30.0,
    }
}

/// The air takes the treble of a far sound (some 5 dB per km at 4 kHz): a low-pass that
/// closes with the distance.
fn air(at: DVec3, ear: DVec3) -> f32 {
    let d = (at - ear).length() as f32;
    if d < 300.0 {
        0.0
    } else {
        (16_000.0 * 300.0 / d).clamp(1200.0, 16_000.0)
    }
}
