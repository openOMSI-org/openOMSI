//! When and how loud each slot is heard: from what stands around (`place`), the hour and the
//! day (the sun's height, the map's holidays), the season and the weather. The species sing
//! in the weeks and at the hours they sing in Central Europe; the bells ring the hours and
//! the services; the shops are open when shops are.

use super::catalog::Spot;
use super::place::Place;
use glam::DVec3;
use omsi_map::calendar::DayKind;

/// The moment, as the rules read it.
#[derive(Debug, Clone)]
pub struct Ctx<'a> {
    pub place: &'a Place,
    pub ear: DVec3,
    /// Hour of the day (0 … 24), day of the year (1 … 366), weekday (0 Monday … 6 Sunday).
    pub hour: f32,
    pub doy: f32,
    pub weekday: i32,
    pub day: DayKind,
    /// The sun's height over the horizon (degrees).
    pub sun: f32,
    /// Rain and snow (0 … 1 each, OMSI's precipitation rate), hail.
    pub rain: f32,
    pub snow: f32,
    /// Air temperature (°C), wind at 10 m (m/s).
    pub temp: f32,
    pub wind: f32,
    /// Snow lying on the ground; how wet the roads are (0 … 1).
    pub snow_lying: bool,
    pub wetness: f32,
    /// A thundery sky.
    pub storm: bool,
    /// How wet the trees still are after rain (0 … 1): they drip it off for a while.
    pub wet_trees: f32,
    /// A left-hand-traffic map: Britain (its sirens, its crossings, its bells).
    pub uk: bool,
    /// People walking within some 40 m.
    pub people: u32,
    /// New Year's Eve and night.
    pub new_year: bool,
}

/// Where a one-shot is heard from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Where {
    /// Somewhere around, between `near` and `far` metres away, `up` metres up.
    Around { near: f32, far: f32, up: f32 },
    /// At this place.
    At(DVec3),
}

/// A smooth step from 0 at `a` to 1 at `b`.
pub fn step(x: f32, a: f32, b: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 1 between days `from` and `to` of the year (wrapping over New Year), easing in and out
/// over `ramp` days.
pub fn season(doy: f32, from: f32, to: f32, ramp: f32) -> f32 {
    window(doy, from, to, ramp, 365.0)
}

/// 1 between hours `from` and `to` (wrapping over midnight), easing over `ramp` hours.
pub fn hours(h: f32, from: f32, to: f32, ramp: f32) -> f32 {
    window(h, from, to, ramp, 24.0)
}

/// 1 between `from` and `to` on a circle of `period` (`to` may lie past the end: 22 to 26
/// is ten at night to two in the morning), easing over `ramp`.
fn window(x: f32, from: f32, to: f32, ramp: f32, period: f32) -> f32 {
    let to = if to < from { to + period } else { to };
    let inside = |v: f32| step(v, from - ramp, from) * (1.0 - step(v, to, to + ramp));
    inside(x).max(inside(x + period)).max(inside(x - period))
}

impl Ctx<'_> {
    pub fn daylight(&self) -> f32 {
        step(self.sun, -6.0, 0.0)
    }
    pub fn night(&self) -> f32 {
        1.0 - step(self.sun, -10.0, -3.0)
    }
    /// Around sunrise, in the morning: the dawn chorus.
    pub fn dawn(&self) -> f32 {
        if self.hour > 12.0 {
            return 0.0;
        }
        step(self.sun, -10.0, -4.0) * (1.0 - step(self.sun, 6.0, 14.0))
    }
    /// The leaves are out (budding late April, falling in early November).
    pub fn leafy(&self) -> f32 {
        season(self.doy, 115.0, 300.0, 15.0)
    }
    pub fn spring_summer(&self) -> f32 {
        season(self.doy, 75.0, 200.0, 15.0)
    }
    pub fn summer(&self) -> f32 {
        season(self.doy, 152.0, 243.0, 12.0)
    }
    pub fn autumn(&self) -> f32 {
        season(self.doy, 255.0, 330.0, 10.0)
    }
    pub fn winter(&self) -> f32 {
        season(self.doy, 340.0, 60.0, 15.0)
    }
    /// What keeps the animals quiet: rain, a gale, hard frost.
    pub fn calm(&self) -> f32 {
        (1.0 - self.rain * 0.85 - self.snow * 0.5).max(0.0)
            * (1.0 - step(self.wind, 9.0, 16.0))
            * step(self.temp, -10.0, -3.0)
    }
    pub fn trees(&self) -> f32 {
        self.place.deciduous.max(self.place.conifer)
    }
    /// A garden or a park in a town: houses and trees.
    pub fn garden(&self) -> f32 {
        (self.place.urban * 2.0).min(1.0) * self.trees()
    }
    /// A village: a few low houses, open land and trees around.
    pub fn village(&self) -> f32 {
        let p = self.place;
        let few = step(p.urban, 0.02, 0.12) * (1.0 - step(p.urban, 0.35, 0.6));
        few * (1.0 - step(p.height, 9.0, 14.0)) * (p.open + p.deciduous).min(1.0)
    }
    /// Houses with gardens: low, not dense.
    pub fn suburb(&self) -> f32 {
        let p = self.place;
        step(p.urban, 0.15, 0.35)
            * (1.0 - step(p.urban, 0.7, 0.9))
            * (1.0 - step(p.height, 10.0, 15.0))
    }
    /// The town proper: dense or tall.
    pub fn town(&self) -> f32 {
        let p = self.place;
        step(p.urban, 0.4, 0.8).max(step(p.height, 12.0, 18.0) * step(p.urban, 0.25, 0.5))
    }
    /// A courtyard: in the town, well back from any street.
    pub fn courtyard(&self) -> f32 {
        let back = self.place.road.map(|d| step(d, 25.0, 45.0)).unwrap_or(1.0);
        step(self.place.urban, 0.35, 0.6) * back
    }
    /// The water heard: near it, and as much of it as there is.
    fn water_near(&self, reach: f32) -> f32 {
        self.place
            .water
            .map(|(d, _)| 1.0 - step(d, reach * 0.3, reach))
            .unwrap_or(0.0)
            * self.place.water_amount.sqrt()
    }
    fn workday(&self) -> bool {
        self.day.workday && !self.day.holiday
    }
    /// How much traffic is on the roads (0 … 1): the night's few cars, the morning and the
    /// evening rush, a quieter weekend that starts later.
    pub fn traffic(&self) -> f32 {
        let h = self.hour;
        let workday = self.workday();
        let rush = if workday {
            hours(h, 6.5, 9.0, 1.0).max(hours(h, 15.5, 18.5, 1.0))
        } else {
            hours(h, 11.0, 18.0, 2.0) * 0.8
        };
        let day = hours(h, 6.0, 21.0, 1.5) * if workday { 0.75 } else { 0.55 };
        (0.08 + 0.92 * rush.max(day)).min(1.0)
    }

    /// Sunday or a public holiday: the quiet day.
    fn sunday(&self) -> bool {
        self.weekday == 6 || self.day.holiday
    }
}

/// The continuous layers: slot, weight (0 … 1), and from where (`None`: around).
pub fn beds(c: &Ctx) -> Vec<(&'static str, f32, Option<(DVec3, f32)>)> {
    let p = c.place;
    let mut out: Vec<(&'static str, f32, Option<(DVec3, f32)>)> = Vec::new();
    let mut add = |slot: &'static str, w: f32| {
        if w > 0.002 {
            out.push((slot, w.min(1.0), None));
        }
    };
    let day = c.daylight();
    let night = c.night();
    let calm = c.calm();
    // ---- places
    let town = c.town() * (1.0 - c.courtyard() * 0.7);
    add(
        "place.city_day",
        town * (1.0 - night) * (0.5 + 0.5 * c.traffic()),
    );
    add("place.city_night", town * night);
    add("place.courtyard_day", c.courtyard() * (1.0 - night));
    add("place.courtyard_night", c.courtyard() * night);
    add("place.suburb", c.suburb() * (1.0 - night * 0.6));
    add("place.village", c.village() * (1.0 - night * 0.7));
    let traffic = c.traffic();
    add("place.arterial", p.far_roads * 0.8 * traffic);
    add("place.railway", step(p.tracks as f32, 2.5, 6.0));
    let workshops = p
        .near(Spot::Workshop)
        .map(|(_, at, _)| 1.0 - step((*at - c.ear).length() as f32, 40.0, 160.0))
        .fold(0.0f32, f32::max);
    add(
        "place.industrial",
        workshops * if c.workday() { 1.0 } else { 0.3 },
    );
    add("place.tunnel", if p.tunnel { 1.0 } else { 0.0 });
    // ---- nature
    let map_wood = p.map_has(Spot::OmsiForest, c.ear, 200.0);
    let map_field = p.map_has(Spot::OmsiField, c.ear, 200.0);
    let wood = if map_wood { 0.3 } else { 1.0 };
    let field = if map_field { 0.3 } else { 1.0 };
    let birds = c.spring_summer() * calm;
    add(
        "nature.dawn_urban",
        c.dawn() * birds * c.garden().max(c.suburb() * 0.6),
    );
    add(
        "nature.dawn_wood",
        c.dawn() * birds * c.trees() * (1.0 - p.urban) * wood,
    );
    add(
        "nature.wood_deciduous",
        p.deciduous * c.leafy() * day * calm * (1.0 - p.urban) * wood,
    );
    add(
        "nature.wood_conifer",
        p.conifer * day * calm * (1.0 - p.urban) * wood,
    );
    add(
        "nature.wood_winter",
        c.trees() * c.winter() * day * (1.0 - p.urban) * wood,
    );
    let fields = step(p.open, 0.3, 0.7);
    add(
        "nature.field_summer",
        fields * c.summer().max(c.spring_summer() * 0.6) * day * calm * field,
    );
    add(
        "nature.field_autumn",
        fields * c.autumn() * day * calm * field,
    );
    add(
        "nature.night_crickets",
        (p.open + c.trees()).min(1.0)
            * (1.0 - p.urban * 0.7)
            * night
            * season(c.doy, 165.0, 255.0, 15.0)
            * step(c.temp, 12.0, 17.0)
            * calm,
    );
    add(
        "nature.night_country",
        (1.0 - p.urban) * night * (1.0 - step(c.temp, 12.0, 17.0) * c.summer()) * calm.max(0.4),
    );
    add(
        "nature.water",
        c.water_near(120.0) * calm.max(0.5) * (1.0 - night * 0.5),
    );
    add(
        "nature.frogs",
        c.water_near(150.0)
            * season(c.doy, 70.0, 165.0, 10.0)
            * (1.0 - step(c.sun, 0.0, 15.0))
            * step(c.temp, 6.0, 10.0),
    );
    add(
        "nature.garden",
        c.garden() * c.spring_summer().max(0.3) * day * calm * (1.0 - c.dawn()),
    );
    add(
        "nature.bees",
        c.trees()
            * season(c.doy, 105.0, 255.0, 10.0)
            * step(c.sun, 10.0, 25.0)
            * step(c.temp, 13.0, 18.0)
            * (1.0 - c.rain).powi(4)
            * 0.5,
    );
    // ---- weather
    let open_sky = if p.tunnel { 0.0 } else { 1.0 };
    let r = c.rain * open_sky;
    add("rain.light", r * (1.0 - step(c.rain, 0.25, 0.5)));
    add(
        "rain.moderate",
        r * step(c.rain, 0.15, 0.35) * (1.0 - step(c.rain, 0.6, 0.85)),
    );
    add("rain.heavy", r * step(c.rain, 0.55, 0.85));
    let paved = (p.urban * 1.5).min(1.0);
    add("rain.asphalt", r * paved * 0.8);
    add(
        "rain.foliage",
        r * (p.deciduous * c.leafy() + p.conifer).min(1.0),
    );
    add(
        "rain.puddles",
        c.rain.min(1.0) * c.wetness * paved * open_sky * 0.7,
    );
    let gutters = step(p.urban, 0.2, 0.5) * open_sky;
    add("rain.downpipe", gutters * (r * 0.8).max(c.wet_trees * 0.4));
    add(
        "rain.drain",
        gutters * c.wetness * (r.max(c.wet_trees * 0.5)) * 0.6,
    );
    add(
        "rain.drips",
        c.wet_trees * (1.0 - r) * (p.deciduous * c.leafy() + p.conifer).min(1.0) * open_sky,
    );
    let s = c.snow * open_sky;
    add(
        "snow.light",
        s * step(-c.temp, -1.0, 0.5) * (1.0 - step(c.wind, 8.0, 13.0)),
    );
    add("snow.wet", s * step(c.temp, -0.5, 1.0));
    add("snow.blizzard", s * step(c.wind, 8.0, 14.0));
    let w = c.wind * open_sky;
    let exposed = 1.0 - 0.5 * p.urban;
    add(
        "wind.light",
        step(w, 1.5, 4.0) * (1.0 - step(w, 7.0, 10.0)) * exposed * 0.7,
    );
    add(
        "wind.moderate",
        step(w, 5.0, 8.0) * (1.0 - step(w, 12.0, 15.0)) * exposed,
    );
    add("wind.strong", step(w, 11.0, 16.0) * exposed);
    let breeze = step(w, 2.0, 9.0);
    add("wind.deciduous", p.deciduous * c.leafy() * breeze);
    add("wind.conifer", p.conifer * breeze);
    add(
        "wind.bare",
        p.deciduous * (1.0 - c.leafy()) * step(w, 4.0, 11.0),
    );
    add(
        "wind.leaves_ground",
        c.autumn() * (p.deciduous + paved * 0.5).min(1.0) * step(w, 3.0, 8.0) * (1.0 - c.wetness),
    );
    add(
        "wind.wires",
        p.wire.map(|d| 1.0 - step(d, 8.0, 30.0)).unwrap_or(0.0) * step(w, 6.0, 12.0),
    );
    // ---- people
    let crowd = step(c.people as f32, 4.0, 18.0) * (1.0 - night * 0.5);
    add(if c.uk { "crowd.en" } else { "crowd.de" }, crowd);
    // the far motorway comes from its side
    if p.fast_roads > 0.002 {
        out.push((
            "place.motorway",
            (p.fast_roads * (0.3 + 0.7 * traffic)).min(1.0),
            Some((c.ear + p.fast_roads_from * 300.0, 300.0)),
        ));
    }
    out
}

/// The one-shots: slot, how many a minute, and where.
pub fn events(c: &Ctx) -> Vec<(&'static str, f32, Where)> {
    let p = c.place;
    let mut out = Vec::new();
    let mut add = |slot: &'static str, per_minute: f32, w: Where| {
        if per_minute > 1.0e-4 {
            out.push((slot, per_minute, w));
        }
    };
    let calm = c.calm();
    let day = c.daylight();
    let dawn = c.dawn();
    let night = c.night();
    let evening = hours(c.hour, 17.5, 21.5, 1.0) * (1.0 - night * 0.5);
    let trees = c.trees();
    let garden = c.garden();
    let town = c.town();
    let near_bird = Where::Around {
        near: 12.0,
        far: 70.0,
        up: 6.0,
    };
    // ---- birds (song period, hour, habitat)
    let habitat_song = (garden + trees * (1.0 - p.urban) + c.suburb() * 0.7).min(1.0);
    add(
        "bird.blackbird",
        1.6 * season(c.doy, 60.0, 196.0, 10.0)
            * (dawn * 2.0 + evening * 1.5 + day * 0.3)
            * habitat_song
            * calm,
        near_bird,
    );
    add(
        "bird.robin",
        1.0 * (1.0 - season(c.doy, 191.0, 232.0, 8.0))
            * (dawn * 1.5 + day * 0.4 + evening)
            * habitat_song
            * calm,
        near_bird,
    );
    add(
        "bird.chaffinch",
        1.4 * season(c.doy, 51.0, 191.0, 8.0)
            * (dawn + day * 0.7)
            * (trees + garden).min(1.0)
            * calm,
        near_bird,
    );
    add(
        "bird.great_tit",
        1.0 * season(c.doy, 15.0, 166.0, 10.0) * (dawn + day * 0.6) * habitat_song * calm,
        near_bird,
    );
    add(
        "bird.blue_tit",
        0.6 * season(c.doy, 32.0, 152.0, 10.0) * (dawn + day * 0.5) * habitat_song * calm,
        near_bird,
    );
    add(
        "bird.sparrow",
        2.0 * day
            * hours(c.hour, 6.0, 19.5, 1.0)
            * (step(p.urban, 0.1, 0.4) + c.village()).min(1.0)
            * calm,
        Where::Around {
            near: 5.0,
            far: 35.0,
            up: 3.0,
        },
    );
    add(
        "bird.wood_pigeon",
        0.5 * season(c.doy, 60.0, 270.0, 15.0) * day * (garden + trees).min(1.0) * calm,
        near_bird,
    );
    add(
        "bird.feral_pigeon",
        0.7 * day * town * calm,
        Where::Around {
            near: 6.0,
            far: 40.0,
            up: 10.0,
        },
    );
    add(
        "bird.swift",
        1.5 * season(c.doy, 125.0, 217.0, 6.0)
            * (evening * 1.5 + day * 0.3)
            * step(p.height, 8.0, 14.0).max(town)
            * calm,
        Where::Around {
            near: 20.0,
            far: 80.0,
            up: 25.0,
        },
    );
    add(
        "bird.crow",
        0.6 * day * calm,
        Where::Around {
            near: 30.0,
            far: 200.0,
            up: 10.0,
        },
    );
    add(
        "bird.rook",
        0.6 * day * (p.open + c.village()).min(1.0) * calm,
        Where::Around {
            near: 40.0,
            far: 250.0,
            up: 12.0,
        },
    );
    add(
        "bird.jackdaw",
        0.5 * day * (town + c.village()).min(1.0) * calm + 0.8 * evening * town * c.winter(),
        Where::Around {
            near: 30.0,
            far: 150.0,
            up: 15.0,
        },
    );
    add(
        "bird.magpie",
        0.5 * day * (c.suburb() + garden).min(1.0) * calm,
        Where::Around {
            near: 15.0,
            far: 90.0,
            up: 6.0,
        },
    );
    add(
        "bird.starling",
        0.6 * (season(c.doy, 60.0, 166.0, 10.0) + 0.6 * c.autumn())
            * day
            * (c.suburb() + p.open * 0.5 + garden).min(1.0)
            * calm,
        near_bird,
    );
    if !map_has_field(c) {
        add(
            "bird.skylark",
            1.0 * season(c.doy, 46.0, 212.0, 10.0) * day * step(p.open, 0.5, 0.8) * calm,
            Where::Around {
                near: 40.0,
                far: 200.0,
                up: 40.0,
            },
        );
    }
    add(
        "bird.tawny_owl",
        0.4 * night * (0.5 + c.winter() + c.autumn()).min(1.0) * trees * calm,
        Where::Around {
            near: 40.0,
            far: 300.0,
            up: 10.0,
        },
    );
    if !c.uk {
        add(
            "bird.nightingale",
            1.2 * season(c.doy, 110.0, 171.0, 6.0)
                * (night + dawn + day * 0.3).min(1.0)
                * p.deciduous
                * calm,
            near_bird,
        );
    }
    add(
        "bird.cuckoo",
        0.3 * season(c.doy, 105.0, 181.0, 6.0)
            * day
            * (trees * (1.0 - p.urban) + p.open * 0.3).min(1.0)
            * calm,
        Where::Around {
            near: 100.0,
            far: 500.0,
            up: 10.0,
        },
    );
    add(
        "bird.woodpecker",
        0.4 * (season(c.doy, 32.0, 140.0, 10.0) + 0.4) * day * trees * (1.0 - p.urban) * calm,
        Where::Around {
            near: 30.0,
            far: 200.0,
            up: 8.0,
        },
    );
    add(
        "bird.geese",
        0.15 * (season(c.doy, 263.0, 334.0, 7.0) + season(c.doy, 46.0, 90.0, 7.0))
            * (1.0 - night * 0.6)
            * calm.max(0.3),
        Where::Around {
            near: 150.0,
            far: 600.0,
            up: 120.0,
        },
    );
    let water = c.water_near(200.0);
    add(
        "bird.mallard",
        0.6 * water * (day + night * 0.2) * calm,
        water_place(c),
    );
    add(
        "bird.gull",
        0.5 * (water + town * c.winter() * 0.6).min(1.0) * day * calm,
        Where::Around {
            near: 30.0,
            far: 200.0,
            up: 20.0,
        },
    );
    // ---- other animals
    add(
        "animal.dog",
        0.25 * (c.suburb() + c.village()).min(1.0) * hours(c.hour, 7.0, 22.0, 1.0),
        Where::Around {
            near: 80.0,
            far: 400.0,
            up: 1.0,
        },
    );
    add(
        "animal.frog",
        0.8 * c.water_near(150.0)
            * season(c.doy, 70.0, 165.0, 10.0)
            * (1.0 - step(c.sun, 0.0, 15.0))
            * step(c.temp, 6.0, 10.0),
        water_place(c),
    );
    let rooster_place = p.near(Spot::Farm).next().map(|(_, at, _)| *at);
    if !p.map_has(Spot::OmsiRooster, c.ear, 500.0) {
        if let Some(at) = rooster_place {
            add(
                "animal.rooster",
                (dawn * 1.5 + day * 0.1) * hours(c.hour, 4.0, 11.0, 1.0).max(0.1),
                Where::At(at),
            );
        }
    }
    for (_, at, _) in p.near(Spot::Cows) {
        add(
            "animal.cow",
            0.4 * season(c.doy, 100.0, 300.0, 10.0) * hours(c.hour, 6.0, 21.0, 1.0),
            Where::At(*at),
        );
    }
    // ---- far life
    add(
        "far.aircraft",
        0.15 * hours(c.hour, 6.0, 23.0, 0.5),
        Where::Around {
            near: 1500.0,
            far: 4000.0,
            up: 1500.0,
        },
    );
    add(
        "far.helicopter",
        0.02 * town * hours(c.hour, 7.0, 22.0, 1.0),
        Where::Around {
            near: 400.0,
            far: 1500.0,
            up: 200.0,
        },
    );
    add(
        if c.uk { "far.siren_uk" } else { "far.siren_de" },
        0.08 * town.max(c.suburb() * 0.4),
        Where::Around {
            near: 400.0,
            far: 2000.0,
            up: 2.0,
        },
    );
    let gardening = season(c.doy, 90.0, 290.0, 10.0)
        * (1.0 - c.rain).powi(4)
        * (1.0 - c.wetness)
        * if c.sunday() {
            0.0
        } else {
            hours(c.hour, 9.0, 12.0, 0.5).max(hours(c.hour, 15.0, 19.0, 0.5))
        };
    add(
        "far.lawnmower",
        0.15 * gardening * (c.suburb() + c.village()).min(1.0),
        Where::Around {
            near: 40.0,
            far: 180.0,
            up: 1.0,
        },
    );
    let weekdays = if c.workday() {
        hours(c.hour, 7.0, 17.0, 0.5)
    } else {
        0.0
    };
    add(
        "far.chainsaw",
        0.05 * weekdays * (c.village() + trees * (1.0 - p.urban)).min(1.0),
        Where::Around {
            near: 200.0,
            far: 700.0,
            up: 2.0,
        },
    );
    add(
        "far.tractor",
        0.07 * weekdays
            * season(c.doy, 60.0, 320.0, 10.0)
            * (c.village() + p.open * (1.0 - p.urban)).min(1.0),
        Where::Around {
            near: 150.0,
            far: 600.0,
            up: 2.0,
        },
    );
    add(
        "far.door",
        0.4 * (c.courtyard() + c.suburb() * 0.5 + town * 0.4).min(1.0)
            * hours(c.hour, 6.0, 23.0, 1.0),
        Where::Around {
            near: 15.0,
            far: 70.0,
            up: 1.0,
        },
    );
    add(
        "far.bin_lorry",
        0.08 * if c.workday() {
            hours(c.hour, 6.5, 9.5, 0.3)
        } else {
            0.0
        } * (town + c.suburb()).min(1.0),
        Where::Around {
            near: 60.0,
            far: 250.0,
            up: 2.0,
        },
    );
    add(
        "far.snow_shovel",
        0.3 * if c.snow_lying {
            hours(c.hour, 6.0, 10.0, 0.5)
        } else {
            0.0
        } * (c.suburb() + town * 0.6 + c.village()).min(1.0),
        Where::Around {
            near: 20.0,
            far: 120.0,
            up: 1.0,
        },
    );
    add(
        "far.leaf_blower",
        0.08 * c.autumn() * weekdays * (1.0 - c.rain) * (c.suburb() + town * 0.5).min(1.0),
        Where::Around {
            near: 50.0,
            far: 250.0,
            up: 1.0,
        },
    );
    if c.new_year {
        // a few rockets in the evening, more and more towards midnight, the town ablaze from
        // five to twelve until half past, dying away by two
        let h = if c.hour < 12.0 { c.hour + 24.0 } else { c.hour };
        let rate = 1.0
            + 4.0 * step(h, 22.0, 23.9)
            + 15.0 * step(h, 23.85, 23.95) * (1.0 - step(h, 24.5, 26.0));
        add(
            "far.fireworks",
            rate * (town + c.suburb() + c.village()).min(1.0).max(0.4),
            Where::Around {
                near: 80.0,
                far: 1500.0,
                up: 60.0,
            },
        );
    }
    if let Some((d, at)) = p.rail {
        add(
            "far.train",
            0.25 * (1.0 - step(d, 150.0, 700.0)) * hours(c.hour, 4.5, 24.0, 0.5),
            Where::At(at),
        );
        add(
            "far.rail_clatter",
            0.15 * (1.0 - step(d, 60.0, 350.0)) * hours(c.hour, 4.5, 24.0, 0.5),
            Where::At(at),
        );
    }
    for (_, at, _) in p.near(Spot::Workshop) {
        add("far.forklift", 0.2 * weekdays, Where::At(*at));
    }
    // ---- thunder
    if c.storm {
        add(
            "thunder.close",
            0.4,
            Where::Around {
                near: 400.0,
                far: 1500.0,
                up: 800.0,
            },
        );
        add(
            "thunder.mid",
            0.8,
            Where::Around {
                near: 1500.0,
                far: 4000.0,
                up: 1500.0,
            },
        );
        add(
            "thunder.far",
            1.0,
            Where::Around {
                near: 4000.0,
                far: 12000.0,
                up: 2000.0,
            },
        );
        if c.rain > 0.5 {
            add(
                "thunder.rain",
                0.2,
                Where::Around {
                    near: 800.0,
                    far: 3000.0,
                    up: 1000.0,
                },
            );
        }
    }
    out
}

fn map_has_field(c: &Ctx) -> bool {
    c.place.map_has(Spot::OmsiField, c.ear, 300.0)
}

/// By the water: at its nearest cell, or somewhere around when there is none.
fn water_place(c: &Ctx) -> Where {
    match c.place.water {
        Some((_, at)) => Where::At(at + DVec3::new(0.0, 0.0, 0.5)),
        None => Where::Around {
            near: 20.0,
            far: 60.0,
            up: 0.0,
        },
    }
}

/// The loops of the objects around: slot, where, its `[3d]` range (m), weight.
pub fn spot_loops(c: &Ctx) -> Vec<(&'static str, DVec3, f32, f32, i64)> {
    let mut out = Vec::new();
    let open = |from: f32, to: f32| hours(c.hour, from, to, 0.25);
    // (nobody plays outdoors in a proper rain)
    let dry = 1.0 - step(c.rain, 0.05, 0.25);
    let workday = c.workday();
    let saturday = c.weekday == 5 && !c.day.holiday;
    let school_day = workday && !c.day.school_holiday;
    for (kind, at, id) in &c.place.spots {
        let d = (*at - c.ear).length() as f32;
        let (slot, range, w): (&'static str, f32, f32) = match kind {
            Spot::PetrolStation => ("petrol.forecourt", 12.0, open(6.0, 22.0)),
            Spot::Supermarket => (
                "supermarket.outside",
                15.0,
                if workday || saturday {
                    open(7.0, 21.0)
                } else {
                    0.0
                },
            ),
            Spot::SnackBar => (
                "imbiss.outside",
                8.0,
                open(10.0, 20.0) * if c.sunday() { 0.5 } else { 1.0 },
            ),
            Spot::Pool => (
                "pool.outdoor",
                40.0,
                season(c.doy, 145.0, 245.0, 5.0)
                    * open(10.0, 19.0)
                    * step(c.temp, 17.0, 21.0)
                    * dry,
            ),
            Spot::SportsGround => {
                let on = if workday {
                    open(16.0, 20.0)
                } else if saturday {
                    open(13.0, 18.0)
                } else {
                    open(10.0, 13.0).max(open(14.5, 17.0))
                };
                (
                    "sports.football",
                    45.0,
                    on * c.daylight().max(0.5 * (1.0 - c.winter()))
                        * (1.0 - step(c.rain, 0.3, 0.7))
                        * (1.0 - c.snow),
                )
            }
            Spot::Tennis => (
                "sports.tennis",
                20.0,
                season(c.doy, 100.0, 290.0, 7.0) * open(9.0, 20.0) * dry * c.daylight(),
            ),
            Spot::Farm => ("farm.yard", 25.0, open(6.0, 20.0) * 0.6),
            Spot::BusDepot => ("depot.yard", 40.0, open(4.0, 24.0)),
            Spot::Workshop => (
                "workshop.metal",
                20.0,
                if workday { open(7.0, 16.0) } else { 0.0 },
            ),
            Spot::RoadWorks => {
                // a site, not a lone barrier: three of them together
                let site = c
                    .place
                    .near(Spot::RoadWorks)
                    .filter(|(_, b, _)| (*b - *at).length() < 30.0)
                    .count()
                    >= 3;
                (
                    "roadworks",
                    25.0,
                    if site && workday {
                        open(7.0, 16.0) * (1.0 - c.rain * 0.5)
                    } else {
                        0.0
                    },
                )
            }
            Spot::Substation => ("hum.substation", 4.0, 1.0),
            Spot::SodiumLamp => ("hum.lamp", 1.5, 1.0 - step(c.sun, -3.0, 2.0)),
            Spot::Station | Spot::SBahnStation => ("station.platform", 15.0, open(4.5, 24.5)),
            Spot::PedestrianSignal => (
                if c.uk {
                    "ped.puffin_uk"
                } else {
                    "ped.blind_de"
                },
                2.0,
                open(6.0, 22.0).max(0.5),
            ),
            Spot::School => (
                "school.yard",
                35.0,
                if school_day {
                    school_break(c.hour)
                } else {
                    0.0
                } * (0.2 + 0.8 * dry),
            ),
            Spot::Kindergarten => (
                "kindergarten.yard",
                25.0,
                if workday {
                    open(9.5, 11.5).max(open(15.0, 16.5))
                } else {
                    0.0
                } * dry
                    * step(c.temp, -3.0, 3.0),
            ),
            _ => continue,
        };
        // (out of earshot: twenty times its range away a source is 26 dB down, under the
        // place's own wash)
        if w > 0.002 && d < range * 20.0 {
            out.push((slot, *at, range, w, *id));
        }
    }
    out
}

/// 1 during a school's breaks (09:30-09:50, 11:25-11:45).
fn school_break(h: f32) -> f32 {
    hours(h, 9.5, 9.83, 0.03).max(hours(h, 11.42, 11.75, 0.03))
}

/// A sound an object makes at a set time: slot, where, range (m), how many strokes (a
/// church's hour) - fired once when its minute comes.
#[derive(Debug, Clone, PartialEq)]
pub struct Timed {
    pub slot: &'static str,
    pub at: DVec3,
    pub range: f32,
    pub strokes: u32,
    /// What makes it once a day: (object id, minute of the day).
    pub key: (i64, u32),
}

/// The objects' set times falling in the minute of `c.hour`.
pub fn timed(c: &Ctx) -> Vec<Timed> {
    let minute = (c.hour * 60.0).floor() as u32;
    let mut out = Vec::new();
    let school_day = c.workday() && !c.day.school_holiday;
    for (kind, at, id) in &c.place.spots {
        let mut fire = |slot: &'static str, range: f32, strokes: u32| {
            out.push(Timed {
                slot,
                at: *at,
                range,
                strokes,
                key: (*id, minute),
            })
        };
        match kind {
            Spot::ChurchProtestant | Spot::ChurchCatholic | Spot::Church => {
                // the hours, from seven in the morning to ten at night
                if minute % 60 == 0 && (7..=22).contains(&(minute / 60)) {
                    let h = (minute / 60) % 12;
                    fire("church.strike", 120.0, if h == 0 { 12 } else { h });
                }
                if c.uk {
                    // Sunday service ringing and the practice night
                    if (c.weekday == 6 && minute == 10 * 60 + 15)
                        || (c.weekday == 3 && minute == 19 * 60 + 30)
                    {
                        fire("church.change_ringing", 150.0, 1);
                    }
                    continue;
                }
                let catholic = *kind == Spot::ChurchCatholic;
                // Sunday morning before the service, Saturday evening ringing the Sunday in
                let service = if catholic { 9 * 60 + 15 } else { 9 * 60 + 45 };
                if (c.weekday == 6 && minute == service) || (c.weekday == 5 && minute == 18 * 60) {
                    fire(
                        if catholic {
                            "church.peal_catholic"
                        } else {
                            "church.peal_protestant"
                        },
                        150.0,
                        1,
                    );
                }
                if catholic && [7 * 60 + 2, 12 * 60 + 2, 18 * 60 + 2].contains(&minute) {
                    fire("church.angelus", 120.0, 1);
                }
            }
            Spot::School if school_day => {
                if [
                    7 * 60 + 55,
                    9 * 60 + 30,
                    9 * 60 + 50,
                    11 * 60 + 25,
                    11 * 60 + 45,
                    13 * 60 + 15,
                ]
                .contains(&minute)
                {
                    fire(
                        if c.uk || id % 2 == 1 {
                            "school.bell"
                        } else {
                            "school.gong"
                        },
                        30.0,
                        1,
                    );
                }
            }
            _ => {}
        }
    }
    out
}

/// The objects' irregular one-shots: slot, a minute's rate, where, range.
pub fn spot_events(c: &Ctx) -> Vec<(&'static str, f32, DVec3, f32)> {
    let mut out = Vec::new();
    for (kind, at, _) in &c.place.spots {
        // (a platform's loudspeakers carry a couple of hundred metres, a depot's air less)
        if (*at - c.ear).length() > 250.0 {
            continue;
        }
        match kind {
            Spot::Station | Spot::SBahnStation => {
                let on = hours(c.hour, 4.5, 24.5, 0.25);
                out.push(("station.announce", 0.4 * on, *at, 30.0));
                // (the Berlin S-Bahn's door signal; a British guard's whistle)
                if *kind == Spot::SBahnStation || c.uk {
                    out.push(("station.door_warning", 0.3 * on, *at, 20.0));
                }
            }
            Spot::BusDepot => {
                out.push(("depot.air", 0.6 * hours(c.hour, 4.0, 24.0, 0.25), *at, 20.0))
            }
            Spot::Farm if c.workday() => out.push((
                "farm.tractor",
                0.05 * hours(c.hour, 7.0, 18.0, 0.5) * season(c.doy, 60.0, 320.0, 10.0),
                *at,
                30.0,
            )),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(place: &Place) -> Ctx<'_> {
        Ctx {
            place,
            ear: DVec3::ZERO,
            hour: 12.0,
            doy: 172.0,
            weekday: 2,
            day: DayKind {
                workday: true,
                holiday: false,
                school_holiday: false,
            },
            sun: 55.0,
            rain: 0.0,
            snow: 0.0,
            temp: 22.0,
            wind: 3.0,
            snow_lying: false,
            wetness: 0.0,
            storm: false,
            wet_trees: 0.0,
            uk: false,
            people: 0,
            new_year: false,
        }
    }

    fn slots(v: &[(&'static str, f32, Option<(DVec3, f32)>)]) -> Vec<&'static str> {
        v.iter().map(|b| b.0).collect()
    }

    #[test]
    fn seasons_wrap_over_new_year() {
        assert_eq!(season(10.0, 340.0, 60.0, 15.0), 1.0);
        assert_eq!(season(200.0, 340.0, 60.0, 15.0), 0.0);
        assert_eq!(season(150.0, 100.0, 200.0, 10.0), 1.0);
        assert_eq!(hours(23.5, 22.0, 2.0, 0.5), 1.0);
        assert_eq!(hours(12.0, 22.0, 2.0, 0.5), 0.0);
    }

    #[test]
    fn a_summer_noon_in_the_town_and_a_winter_night_in_the_country() {
        let town = Place {
            urban: 0.9,
            height: 18.0,
            open: 0.05,
            deciduous: 0.2,
            ..Default::default()
        };
        let c = ctx(&town);
        let b = slots(&beds(&c));
        assert!(
            b.contains(&"place.city_day") && !b.contains(&"place.city_night"),
            "{b:?}"
        );
        assert!(!b.contains(&"rain.light") && !b.contains(&"nature.field_summer"));
        let country = Place {
            urban: 0.0,
            open: 1.0,
            ..Default::default()
        };
        let mut c = ctx(&country);
        c.hour = 1.0;
        c.sun = -30.0;
        c.doy = 15.0;
        c.temp = -2.0;
        let b = slots(&beds(&c));
        assert!(
            b.contains(&"nature.night_country") && !b.contains(&"nature.night_crickets"),
            "{b:?}"
        );
        assert!(!b.iter().any(|s| s.starts_with("place.city")));
        // no blackbird at a January midnight
        assert!(!events(&c).iter().any(|e| e.0 == "bird.blackbird"));
    }

    #[test]
    fn rain_fades_through_its_strengths() {
        let p = Place {
            urban: 0.8,
            ..Default::default()
        };
        let mut c = ctx(&p);
        c.rain = 0.1;
        let b = slots(&beds(&c));
        assert!(b.contains(&"rain.light") && !b.contains(&"rain.heavy"));
        c.rain = 0.95;
        let b = slots(&beds(&c));
        assert!(b.contains(&"rain.heavy") && !b.contains(&"rain.light"));
    }

    #[test]
    fn the_bells_strike_the_hour_and_ring_on_sunday() {
        let p = Place {
            spots: vec![(Spot::ChurchProtestant, DVec3::new(300.0, 0.0, 0.0), 5)],
            ..Default::default()
        };
        let mut c = ctx(&p);
        c.hour = 15.0 + 0.2 / 60.0;
        let t = timed(&c);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].slot, t[0].strokes), ("church.strike", 3));
        c.hour = 3.0;
        assert!(timed(&c).is_empty(), "no strokes at night");
        c.weekday = 6;
        c.hour = 9.75 + 0.1 / 60.0;
        assert_eq!(timed(&c)[0].slot, "church.peal_protestant");
    }

    #[test]
    fn shops_close_and_schools_have_holidays() {
        let p = Place {
            spots: vec![
                (Spot::Supermarket, DVec3::new(20.0, 0.0, 0.0), 1),
                (Spot::School, DVec3::new(30.0, 0.0, 0.0), 2),
            ],
            ..Default::default()
        };
        let mut c = ctx(&p);
        c.hour = 9.6;
        let l: Vec<_> = spot_loops(&c).iter().map(|s| s.0).collect();
        assert!(
            l.contains(&"supermarket.outside") && l.contains(&"school.yard"),
            "{l:?}"
        );
        c.weekday = 6;
        c.day.workday = false;
        assert!(spot_loops(&c).is_empty(), "Sunday");
        c.weekday = 2;
        c.day = DayKind {
            workday: true,
            holiday: false,
            school_holiday: true,
        };
        let l: Vec<_> = spot_loops(&c).iter().map(|s| s.0).collect();
        assert!(
            l.contains(&"supermarket.outside") && !l.contains(&"school.yard"),
            "{l:?}"
        );
    }

    #[test]
    fn fireworks_at_new_year_only() {
        let town = Place {
            urban: 0.6,
            height: 14.0,
            ..Default::default()
        };
        let mut c = ctx(&town);
        c.doy = 365.0;
        c.hour = 23.97;
        c.sun = -60.0;
        c.new_year = true;
        let e: Vec<_> = events(&c).iter().map(|e| (e.0, e.1)).collect();
        assert!(
            e.iter().any(|(s, r)| *s == "far.fireworks" && *r > 1.0),
            "{e:?}"
        );
        c.new_year = false;
        assert!(!events(&c).iter().any(|e| e.0 == "far.fireworks"));
    }
}
