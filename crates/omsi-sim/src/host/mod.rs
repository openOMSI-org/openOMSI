//! Script host for road vehicles: system variables, callbacks, sound triggers.

use crate::scripttex::{FontTable, ScriptTexture};
use crate::texttex::FontLibrary;
use crate::SimClock;
use omsi_script::{Host, NameId, Stacks, State, SysVar};
use omsi_vehicle::hof::Hof;
use parking_lot::Mutex;
use std::sync::Arc;

mod draw;
mod info;

/// How many stops a page's departures are kept for at the same time (`omsi.getDepartures`).
pub const MAX_HTML_DEPARTURE_STOPS: usize = 8;

/// `wearlifespan` of a vehicle that does not wear (OMSI: every AI vehicle, and the
/// player's with the maintenance option "infinite").
pub const AI_WEAR_LIFESPAN: f32 = 1.5e6;

#[derive(Default)]
pub struct VehicleHost {
    /// The paint scheme the vehicle is made with (`Some(None)`: the model's own textures),
    /// set before it is made: `Colorscheme` and the scheme's `[setvar]`s are there for its
    /// `{init}`, as Omsi.exe sets them when it makes the vehicle (0x70a174), before the
    /// scripts start. None: not known yet (`apply_paint_vars` later).
    pub paint_scheme: Option<Option<usize>>,
    /// Variables set before `{init}` too, after the paint scheme's: the player's bus options
    /// (`--setvar`, the launcher's bus step). They are `[setvar]`s of the bus's liveries, and
    /// a script that takes those over in `{init}` (the Kajosoft O530's `setvar_reset` copies
    /// them into its own) never saw them set after the spawn.
    pub start_vars: Vec<(String, f32)>,
    /// Fleet number and registration chosen by the vehicle dialog. They are copied to the
    /// script's `number` / `ident` strings before `{init}`, like Omsi.exe does.
    pub initial_number: Option<String>,
    pub initial_ident: Option<String>,
    /// A time of day a script wrote to `(S.S.Time)` this frame: the game's clock takes it.
    pub time_written: Option<f64>,
    pub clock: SimClock,
    pub mouse: (f32, f32),
    pub precip_type: f32,
    pub precip_rate: f32,
    /// World-space weather air velocity (m/s), used by water on the glass.
    pub wind: glam::Vec3,
    /// `StreetCond`: how the road under the vehicle is - 0 dry, 1 wet, 2 covered in snow,
    /// and everything in between. The engine feeds it like `Dirt_Norm` (no varlist declares
    /// it); the stock sound configurations fade `Sounds\WetLane_1.wav` in over 0 … 1 and
    /// `WetLane_2.wav` over 1 … 2, and `spray.osc` raises the wheel spray while it is
    /// between 1 and 2.
    pub street_cond: f32,
    /// People aboard (`humans_count`), set by whoever carries them.
    pub humans_count: f32,
    /// 1 while the vehicle drives a timetable duty (`schedule_active`).
    pub schedule_active: f32,
    /// The odometer's reading when the session began (km).
    pub km_base: f64,
    pub temperature: f32,
    pub abs_humidity: f32,
    pub sun_alt: f32,
    /// Last collision: position (vehicle frame, m) and energy (kJ), for the coll_* sysvars.
    /// The energy is this frame's crash: the vehicle clears it once its `collision` block
    /// has run.
    pub coll_pos: [f32; 3],
    pub coll_energy: f32,
    /// `wearlifespan`: how long wearing parts last, 1 = normal. It scales every random
    /// lifetime the scripts draw (rear-door automatic, bulbs, the matrix display). At 0 the
    /// SD200's rear door was "worn out" on its first closing and reopened by itself with the
    /// opening sound from then on.
    pub wear_lifespan: f32,
    /// Ground height (m) under a point given in the vehicle frame, minus the vehicle's own
    /// height: filled every frame by `VehicleInstance::update` for `GetHeightAbovePoint`.
    pub ground_probe: Option<Arc<dyn Fn(f32, f32, f32) -> f32 + Send + Sync>>,
    /// People standing on each `paths.cfg` link inside the cabin (`GetHumanCountOnPathLink`).
    pub humans_on_path_link: Vec<u32>,
    /// People sitting on each `[passpos]` (`GetHumanCountOnSeat`).
    pub humans_on_seat: Vec<u32>,
    /// Callbacks a script asked for that this host does not provide (reported once each).
    unknown_callbacks: Vec<String>,
    /// Per name of the program (`NameId`): the callback it was found to be, as
    /// `Cb as u8 + 1` (0: not looked at yet).
    callback_ids: Vec<u8>,
    pub auto_clutch: f32,
    pub no_sound: f32,
    pub fired_triggers: Vec<String>,
    /// Triggers (lower case) whose sounds read variables in their volume curves: when one
    /// fires, the variables of that moment are kept in `fired_trigger_vars` (the door's
    /// hit sound reads `doorSpeed_<n>`, which the script turns round right after it).
    pub snapshot_triggers: hashbrown::HashSet<String>,
    pub fired_trigger_vars: Vec<(String, Vec<f32>)>,
    /// `(T.F.name)` triggers of this frame: (trigger, sound file relative to the sound folder).
    pub fired_file_triggers: Vec<(String, String)>,
    pub messages: Vec<String>,
    /// Fonts registered by `GetFontIndex` (index = position), loaded through `font_lib`.
    pub fonts: FontTable,
    pub font_lib: Option<Arc<Mutex<FontLibrary>>>,
    /// The display font the player chose for the vehicle's destination signs and the fonts its
    /// sign scripts ask for that it takes the place of (see `set_display_font`).
    pub display_font: Option<ScriptDisplayFont>,
    /// How often a script drew a text with less than no space between its letters (`STTextOut`
    /// spacing below 0): what a matrix that finds no size its text fits in does - a sign of a
    /// display font that came out wider than the fonts it replaces (`omsi-check --display-font`).
    pub negative_spacing: u32,
    /// `[scripttexture]` images drawn by the `ST*` callbacks.
    pub script_textures: Vec<ScriptTexture>,
    /// Folder for `STLoadTex` paths (the vehicle directory).
    pub content_dir: std::path::PathBuf,
    /// Depot file (termini, bus stop strings, IBIS trips) behind the `Get*` callbacks.
    pub hof: Option<Arc<Hof>>,
    /// The map's ticket pack for `GetTicketName/Value`.
    pub tickets: Option<Arc<omsi_content::tickets::TicketPack>>,
    /// Coins the driver handed out with `GiveChangeCoin` (currency coin indices), taken by
    /// the passenger system.
    pub change_coins: Vec<usize>,
    /// The script's `number` string variable (the fleet number): `NrSpecRandom` is
    /// specific to it.
    pub number_var: Option<u32>,
    /// What the bus radio plays (`GetRadioName`, `GetRadioSong`): the station's name and
    /// the title it gives, both empty while the radio is off. The game fills them.
    pub radio_name: String,
    pub radio_song: String,
    /// Timetable information for the `GetTT*` callbacks (line, delay in s, stops).
    pub tt_line: String,
    pub tt_delay: f32,
    pub tt_stops: Vec<(String, f32, f32)>,
    /// The map objects of `tt_stops` (0: not known).
    pub tt_stop_ids: Vec<i64>,
    pub tt_terminus_index: i32,
    pub tt_busstop_index: i32,
    /// The next buses due at the bus stop a scenery object belongs to (its `[varparent]`),
    /// soonest first, for the `GetArrBus*` callbacks of the stop's departure displays.
    pub arrivals: Vec<Arrival>,
    /// Route, line and destination requests of the vehicle's HTML pages, taken by the game
    /// (`VehicleInstance::take_html_requests`).
    pub html_requests: Vec<crate::htmltex::HtmlRequest>,
    /// The departures the game made for the stops the pages asked for (`omsi.getDepartures`),
    /// by key (trimmed, lower case): (line, destination, timestamp), soonest first.
    pub html_departures: std::collections::HashMap<String, Vec<crate::vehicle_api::Departure>>,
    /// Which board generation of the game `html_departures` is from.
    pub html_departures_gen: u64,
    /// The stops the pages asked departures for, taken by the game: the `MAX_HTML_DEPARTURE_STOPS`
    /// asked most recently, the oldest first (see [`VehicleHost::want_departures`]).
    pub html_departure_wants: Vec<String>,
}

/// A display font on the matrices a vehicle's scripts draw (`texttex::script_signs`): the
/// font chosen, and the names (lower case) of the fonts it replaces - `GetFontIndex` hands out
/// the chosen one for those, fitted to each (`FontLibrary::display_font`), when the font is
/// one that writes letters or the line number (`texttex::is_letter_font`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptDisplayFont {
    pub spec: omsi_content::dotfont::DisplayFontSpec,
    pub fonts: Vec<String>,
}

/// A bus due at a stop (`GetArrBusLine`, `GetArrBusTerminus`, `GetArrBusTimeDiff`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Arrival {
    pub line: String,
    pub terminus: String,
    /// Seconds until it arrives: its timetable time plus its delay, less the time of day.
    /// Negative once it is due and not there yet (the stock display shows 0 and blinks).
    pub due: f32,
}

/// The callbacks the host provides (`(M.V.name)`, by their lower-case names): a script's
/// call is matched to one once per name of its program (`VehicleHost::callback_kind`), not
/// compared with every name on every call - the matrix displays call them for every pixel.
macro_rules! callbacks {
    ($($v:ident = $n:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Cb {
            $($v,)*
        }

        impl Cb {
            const ALL: &'static [Cb] = &[$(Cb::$v,)*];

            fn name(self) -> &'static str {
                match self {
                    $(Cb::$v => $n,)*
                }
            }

            fn of(lname: &str) -> Option<Cb> {
                match lname {
                    $($n => Some(Cb::$v),)*
                    _ => None,
                }
            }
        }
    };
}

callbacks! {
    GetFontIndex = "getfontindex",
    TextLength = "textlength",
    StNewTex = "stnewtex",
    StLock = "stlock",
    StUnlock = "stunlock",
    StFilter = "stfilter",
    StSetColor = "stsetcolor",
    StDrawPixel = "stdrawpixel",
    StDrawRect = "stdrawrect",
    StTextOut = "sttextout",
    StReadPixel = "streadpixel",
    StGetR = "stgetr",
    StGetG = "stgetg",
    StGetB = "stgetb",
    StGetA = "stgeta",
    StCopyColor = "stcopycolor",
    StLoadTex = "stloadtex",
    GetRouteIndex = "getrouteindex",
    GetRouteTerminusIndex = "getrouteterminusindex",
    GetTerminusCode = "getterminuscode",
    GetTerminusIndex = "getterminusindex",
    GetTerminusString = "getterminusstring",
    GetBusstopCount = "getbusstopcount",
    GetRouteBusstopIdent = "getroutebusstopident",
    GetBusstopIndex = "getbusstopindex",
    GetBusstopString = "getbusstopstring",
    GetDepotStringGlobal = "getdepotstringglobal",
    GetTTLineString = "getttlinestring",
    GetTTDelay = "getttdelay",
    GetTTBusstopCount = "getttbusstopcount",
    GetTTBusstopIndex = "getttbusstopindex",
    GetTTerminusIndex = "gettterminusindex",
    GetTTTerminusIndex = "getttterminusindex",
    GetHeightAbovePoint = "getheightabovepoint",
    GetHumanCountOnPathLink = "gethumancountonpathlink",
    GetTTBusstopName = "getttbusstopname",
    GetTTBusstopDep = "getttbusstopdep",
    GetTTBusstopArr = "getttbusstoparr",
    GiveChangeCoin = "givechangecoin",
    NrSpecRandom = "nrspecrandom",
    GetTicketName = "getticketname",
    GetTTicketName = "gettticketname",
    GetTicketValue = "getticketvalue",
    GetHumanCountOnSeat = "gethumancountonseat",
    GetArrBusLine = "getarrbusline",
    GetArrBusTerminus = "getarrbusterminus",
    GetArrBusTimeDiff = "getarrbustimediff",
    GetRadioName = "getradioname",
    GetRadioSong = "getradiosong",
}

/// The weather's air temperature and humidity (bits of two f32), known before any vehicle
/// is made: the `{init}` of a bus reads `Weather_Temperature` for its engine and cabin (the
/// PAZ and LAZ carburettor engines, every heater script). Set by the game whenever the
/// weather is loaded or changes; NaN until then.
static AMBIENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

/// Tell vehicles made from now on what the weather is (see [`VehicleHost::new`]).
pub fn set_ambient_weather(temperature: f32, abs_humidity: f32) {
    let bits = ((temperature.to_bits() as u64) << 32) | abs_humidity.to_bits() as u64;
    AMBIENT.store(bits, std::sync::atomic::Ordering::Relaxed);
}

/// The weather's temperature (°C) and absolute humidity, once a weather has been set.
pub fn ambient_weather() -> Option<(f32, f32)> {
    let bits = AMBIENT.load(std::sync::atomic::Ordering::Relaxed);
    (bits != u64::MAX).then(|| (f32::from_bits((bits >> 32) as u32), f32::from_bits(bits as u32)))
}

impl VehicleHost {
    /// A page asked for the departures of stop `key` (trimmed, lower case). The game keeps the
    /// stops asked most recently: one asked again moves to the back, a new one past the limit
    /// pushes out the one asked longest ago. (A fixed first-come list stayed full for good, and
    /// every later stop - the bus drives on to new ones - got no departures at all.)
    pub fn want_departures(&mut self, key: String) {
        if let Some(i) = self.html_departure_wants.iter().position(|k| *k == key) {
            let k = self.html_departure_wants.remove(i);
            self.html_departure_wants.push(k);
            return;
        }
        if self.html_departure_wants.len() >= MAX_HTML_DEPARTURE_STOPS {
            self.html_departure_wants.remove(0);
        }
        self.html_departure_wants.push(key);
    }

    pub fn new(clock: SimClock) -> Self {
        // the weather is there before {init} runs: made at 0 °C (the value before the first
        // weather update) every engine was cold, and the PAZ's carburettor engine, which
        // wants its choke below 0 °C, caught and died again on the key
        let (temperature, abs_humidity) = ambient_weather().unwrap_or((0.0, 0.0));
        // `AutoClutch` is 1 unless OMSI's options say `[no_automaticClutch]` (the exe sets
        // 1.0 before reading them): the manual gearboxes of the Sprinters, the PAZ and the
        // LAZ work their clutch themselves then, and at 0 every gear a driver without a
        // clutch pedal put in was thrown out again at once
        Self { clock, wear_lifespan: AI_WEAR_LIFESPAN, auto_clutch: 1.0, temperature, abs_humidity, ..Default::default() }
    }

    /// A host for trying the scripts out on a copy of the vehicle's state (the IBIS typist
    /// plans its key presses that way): the clock, weather, depot, tickets and timetable,
    /// but no fonts, script textures or sounds.
    pub fn scratch(&self) -> VehicleHost {
        VehicleHost {
            clock: self.clock.clone(),
            precip_type: self.precip_type,
            precip_rate: self.precip_rate,
            wind: self.wind,
            street_cond: self.street_cond,
            temperature: self.temperature,
            abs_humidity: self.abs_humidity,
            sun_alt: self.sun_alt,
            wear_lifespan: self.wear_lifespan,
            humans_on_path_link: self.humans_on_path_link.clone(),
            humans_on_seat: self.humans_on_seat.clone(),
            unknown_callbacks: self.unknown_callbacks.clone(),
            auto_clutch: self.auto_clutch,
            no_sound: self.no_sound,
            content_dir: self.content_dir.clone(),
            hof: self.hof.clone(),
            tickets: self.tickets.clone(),
            tt_line: self.tt_line.clone(),
            tt_delay: self.tt_delay,
            tt_stops: self.tt_stops.clone(),
            tt_stop_ids: self.tt_stop_ids.clone(),
            tt_terminus_index: self.tt_terminus_index,
            tt_busstop_index: self.tt_busstop_index,
            ..Default::default()
        }
    }

    /// The callback `name` (script name `id`) is, found once per name: the one kept for
    /// `id` is taken when its name is `name` (a host's scripts keep their ids, but the check
    /// costs one comparison), else it is looked up by its lower-case name.
    fn callback_kind(&mut self, name: &str, id: NameId) -> Option<Cb> {
        let i = id as usize;
        if let Some(cb) = self.callback_ids.get(i).and_then(|&k| Cb::ALL.get((k as usize).wrapping_sub(1))) {
            if cb.name().eq_ignore_ascii_case(name) {
                return Some(*cb);
            }
        }
        let cb = omsi_script::compile::with_lower(name, Cb::of)?;
        if i < 1 << 16 {
            if self.callback_ids.len() <= i {
                self.callback_ids.resize(i + 1, 0);
            }
            self.callback_ids[i] = cb as u8 + 1;
        }
        Some(cb)
    }

    /// Callbacks the scripts called that this host does not provide (lower case).
    pub fn unknown_callbacks(&self) -> &[String] {
        &self.unknown_callbacks
    }

    /// Draw the matrices of the vehicle's sign scripts in the display font `font` (None: in
    /// their own fonts again). The fonts the scripts have asked for so far change in their
    /// places (the indices the scripts hold stay right); a matrix shows it when it next
    /// writes its destination.
    pub fn set_display_font(&mut self, font: Option<ScriptDisplayFont>) {
        self.display_font = font.filter(|f| !f.fonts.is_empty());
        let names: Vec<(usize, String)> = self.fonts.entries.iter().enumerate().map(|(i, e)| (i, e.0.clone())).collect();
        for (i, name) in names {
            let atlas = self.font_for(&name);
            self.fonts.entries[i].1 = atlas;
        }
    }

    /// The font a script asking `GetFontIndex` for `name` draws with: the installed one, or
    /// in its place the display font chosen for the vehicle's signs.
    fn font_for(&self, name: &str) -> Option<Arc<omsi_content::font::FontAtlas>> {
        // an empty name or a depot string that is no font (the Krüger matrix asks for the
        // custom fonts of termini 14991.. this way) is looked up once and remembered as missing
        if name.trim().is_empty() {
            return None;
        }
        let lib = self.font_lib.as_ref()?;
        let own = lib.lock().load(name);
        let Some(df) = self.display_font.as_ref().filter(|d| d.fonts.iter().any(|f| f.eq_ignore_ascii_case(name.trim()))) else { return own };
        let Some(a) = own.as_ref().filter(|a| crate::texttex::is_letter_font(&a.font)) else { return own };
        match lib.lock().display_font_for(&df.spec, Some(a), a.font.height) {
            Some(sub) => {
                log::info!("display font: the matrix's \"{}\" ({} px) drawn in \"{}\"", name.trim(), a.font.height, df.spec.name);
                Some(sub)
            }
            None => own,
        }
    }

    /// The font behind a `GetFontIndex` result; -1 (a missing font) draws nothing.
    fn font_atlas(&self, index: i32) -> Option<Arc<omsi_content::font::FontAtlas>> {
        usize::try_from(index).ok().and_then(|i| self.fonts.entries.get(i)).and_then(|f| f.1.clone())
    }

    fn terminus(&self, index: i32) -> Option<&omsi_vehicle::hof::Terminus> {
        self.hof.as_ref().and_then(|h| h.termini.get(usize::try_from(index).ok()?))
    }

    fn arrival(&self, index: i32) -> Option<&Arrival> {
        self.arrivals.get(usize::try_from(index).ok()?)
    }

    fn bus_stop(&self, index: i32) -> Option<&omsi_vehicle::hof::BusStop> {
        self.hof.as_ref().and_then(|h| h.bus_stops.get(usize::try_from(index).ok()?))
    }
}

/// The `(M.V.…)` callbacks this host answers (lower case); anything else reads 0.
pub const PROVIDED_CALLBACKS: &[&str] = &[
    "getfontindex", "textlength", "stnewtex", "stlock", "stunlock", "stfilter", "stsetcolor", "stdrawpixel", "stdrawrect", "sttextout", "streadpixel", "stgetr", "stgetg", "stgetb", "stgeta", "stcopycolor", "stloadtex",
    "getrouteindex", "getrouteterminusindex", "getterminuscode", "getterminusindex", "getterminusstring", "getbusstopcount", "getroutebusstopident", "getbusstopindex", "getbusstopstring", "getdepotstringglobal",
    "getttlinestring", "getttdelay", "getttbusstopcount", "getttbusstopindex", "gettterminusindex", "getttterminusindex", "getttbusstopname", "getttbusstopdep", "getttbusstoparr",
    "getheightabovepoint", "gethumancountonpathlink", "gethumancountonseat", "givechangecoin", "nrspecrandom", "getticketname", "gettticketname", "getticketvalue",
    "getarrbusline", "getarrbusterminus", "getarrbustimediff",
];

/// A number a script hands a callback, as Omsi.exe takes it: rounded to the nearest
/// integer, a half to the even one (`fistp` under Delphi's control word; TRoadVehicleInst
/// 0x7d28c4 and TComplMapObjInst 0x7bb10c convert every argument with sub_404c7c), not cut
/// off - 1100.9999 out of float arithmetic is 1101.
fn arg_i32(v: f32) -> i32 {
    v.round_ties_even() as i32
}

/// The same as an index (a negative one names nothing).
fn arg_idx(v: f32) -> usize {
    usize::try_from(arg_i32(v)).unwrap_or(usize::MAX)
}

impl Host for VehicleHost {
    fn sys_var(&mut self, v: SysVar) -> f32 {
        match v {
            SysVar::Timegap => self.clock.timegap,
            SysVar::GetTime => self.clock.run_time as f32,
            SysVar::NoSound => self.no_sound,
            SysVar::Pause => if self.clock.paused { 1.0 } else { 0.0 },
            // seconds since midnight ("Time in seconds", says the stock clock object): every
            // script divides it by 3600 for the hour (IBIS and ticket printer clocks) or
            // compares it with 28800/72000 (the Citaro's door chime by day). Handing out hours
            // made every clock read 00:00 and the day-time checks think it was night.
            SysVar::Time => self.clock.time as f32,
            SysVar::Day => self.clock.day_month().0 as f32,
            SysVar::Month => self.clock.day_month().1 as f32,
            SysVar::Year => self.clock.year as f32,
            SysVar::DayOfYear => self.clock.day_of_year as f32,
            SysVar::MouseX => self.mouse.0,
            SysVar::MouseY => self.mouse.1,
            SysVar::PrecipType => self.precip_type,
            SysVar::PrecipRate => self.precip_rate,
            SysVar::CollPosX => self.coll_pos[0],
            SysVar::CollPosY => self.coll_pos[1],
            SysVar::CollPosZ => self.coll_pos[2],
            // Read as often as the block likes: the stock collision block adds it to the
            // general damage and then again to the engine's, and a value gone after the
            // first read left the engine and the drivetrain whole after any crash.
            SysVar::CollEnergy => self.coll_energy,
            SysVar::WeatherTemperature => self.temperature,
            SysVar::WeatherAbsHum => self.abs_humidity,
            SysVar::WearLifespan => self.wear_lifespan,
            SysVar::AutoClutch => self.auto_clutch,
            SysVar::SunAlt => self.sun_alt,
        }
    }

    /// `(S.S.x)`: OMSI writes through the system variable's pointer into the engine's
    /// own value (`TXPC_calcblock_var`). What the engine sets anew every frame (the frame
    /// time, the mouse, the weather, the sun) takes the write for the rest of this frame
    /// only; the clock keeps it - `time_written` hands a new time of day to the game - and so
    /// do the collision values and the switches.
    fn set_sys_var(&mut self, v: SysVar, value: f32) {
        if !value.is_finite() {
            return;
        }
        match v {
            SysVar::Timegap => self.clock.timegap = value,
            SysVar::GetTime => self.clock.run_time = value as f64,
            SysVar::NoSound => self.no_sound = value,
            SysVar::Pause => self.clock.paused = value > 0.5,
            SysVar::Time => {
                self.clock.time = (value as f64).rem_euclid(86400.0);
                self.time_written = Some(self.clock.time);
            }
            SysVar::Day | SysVar::Month | SysVar::Year => {
                let (d, m) = self.clock.day_month();
                let (y, m, d) = match v {
                    SysVar::Day => (self.clock.year, m, value as i32),
                    SysVar::Month => (self.clock.year, value as i32, d),
                    _ => (value as i32, m, d),
                };
                self.clock.set_date(y, m, d);
            }
            SysVar::DayOfYear => self.clock.day_of_year = (value as i32).clamp(1, crate::clock::days_in_year(self.clock.year)),
            SysVar::MouseX => self.mouse.0 = value,
            SysVar::MouseY => self.mouse.1 = value,
            SysVar::PrecipType => self.precip_type = value,
            SysVar::PrecipRate => self.precip_rate = value,
            SysVar::CollPosX => self.coll_pos[0] = value,
            SysVar::CollPosY => self.coll_pos[1] = value,
            SysVar::CollPosZ => self.coll_pos[2] = value,
            SysVar::CollEnergy => self.coll_energy = value,
            SysVar::WeatherTemperature => self.temperature = value,
            SysVar::WeatherAbsHum => self.abs_humidity = value,
            SysVar::WearLifespan => self.wear_lifespan = value,
            SysVar::AutoClutch => self.auto_clutch = value,
            SysVar::SunAlt => self.sun_alt = value,
        }
    }

    fn callback(&mut self, name: &str, id: NameId, stacks: &mut Stacks, state: &mut State) {
        let cb = self.callback_kind(name, id);
        if debug_text() && omsi_script::compile::with_lower(name, |l| l.starts_with("st")) {
            log::info!("callback {name} stack {:?} strings {:?}", &stacks.st[..stacks.st.len().min(8)], stacks.sst.last());
        }
        match cb {
            // the fonts and the `[scripttexture]` drawing (see `draw`), and everything the scripts
            // ask of the depot, the timetable, the passengers and the radio (see `info`)
            Some(Cb::GetFontIndex | Cb::TextLength | Cb::StNewTex | Cb::StLock | Cb::StUnlock | Cb::StFilter | Cb::StSetColor | Cb::StDrawPixel | Cb::StDrawRect | Cb::StTextOut | Cb::StReadPixel | Cb::StGetR | Cb::StGetG | Cb::StGetB | Cb::StGetA | Cb::StCopyColor | Cb::StLoadTex) => self.draw_callback(cb, stacks),
            Some(_) => self.info_callback(cb, stacks, state),
            None => {
                // numeric callbacks default to 0; say once which one the host lacks
                if !omsi_script::compile::with_lower(name, |l| self.unknown_callbacks.iter().any(|n| n == l)) {
                    log::warn!("script callback (M.V.{name}) is not provided; it reads 0");
                    self.unknown_callbacks.push(name.to_ascii_lowercase());
                }
                stacks.push(0.0);
            }
        }
    }

    fn sound_trigger_file(&mut self, name: &str, file: &str) {
        // `" " (T.F.ev_nbs_on)` is how the MB_C2 gearbox script says "no sound" (every
        // frame in neutral, 80 times a minute per bus at Ahlheim): nothing to play
        if file.trim().is_empty() {
            return;
        }
        self.fired_file_triggers.push((name.to_string(), file.to_string()));
    }
    fn sound_trigger(&mut self, name: &str, _id: NameId) {
        self.fired_triggers.push(name.to_string());
    }
    fn sound_trigger_vars(&mut self, name: &str, id: NameId, vars: &[f32]) {
        self.sound_trigger(name, id);
        // (asked in place; the lower-case name is made only for a trigger that is kept - a
        // copy was made for every trigger fired by a vehicle with such sounds)
        if !self.snapshot_triggers.is_empty() && omsi_script::compile::with_lower(name, |k| self.snapshot_triggers.contains(k)) {
            self.fired_trigger_vars.push((name.to_ascii_lowercase(), vars.to_vec()));
        }
    }

    /// `$msg`: kept as the last few (OMSI shows the latest on its debug line; every
    /// AI bus says one per stop, and the list grew all session).
    fn message(&mut self, text: &str) {
        const KEEP: usize = 16;
        log::debug!("script message: {text}");
        if self.messages.len() >= KEEP {
            self.messages.remove(0);
        }
        self.messages.push(text.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripttex::ScriptTexture;
    use omsi_script::{compile, CompileInput, Vm};

    /// Every callback is found by its name in any letter case, under any script name id,
    /// also when the id was another name's before; an unknown one is reported once.
    #[test]
    fn callbacks_are_found_by_name_under_any_id() {
        let mut host = VehicleHost::new(SimClock::default());
        for (k, cb) in Cb::ALL.iter().enumerate() {
            assert_eq!(cb.name(), cb.name().to_ascii_lowercase());
            assert_eq!(Cb::of(cb.name()), Some(*cb));
            let upper = cb.name().to_ascii_uppercase();
            for id in [k as NameId, 0, 3, 70_000] {
                assert_eq!(host.callback_kind(&upper, id), Some(*cb), "{upper} as {id}");
                assert_eq!(host.callback_kind(cb.name(), id), Some(*cb));
            }
        }
        assert_eq!(host.callback_kind("STDrawPixelX", 3), None);
        assert_eq!(host.callback_kind("STDrawPixel", 3), Some(Cb::StDrawPixel));
        let p = compile(&CompileInput::default());
        let mut state = State::new(&p);
        let mut stacks = Stacks::default();
        for _ in 0..3 {
            host.callback("No_Such_Callback", 5, &mut stacks, &mut state);
        }
        assert_eq!(host.unknown_callbacks(), ["no_such_callback".to_string()]);
        assert_eq!(stacks.st[..3], [0.0; 3]);
    }

    /// A trigger whose sounds read variables keeps them as they stand (by its lower-case
    /// name, whatever case the script writes); the others only fire.
    #[test]
    fn a_snapshot_trigger_keeps_the_variables() {
        let mut host = VehicleHost::new(SimClock::default());
        host.snapshot_triggers.insert("ev_door_open".to_string());
        host.sound_trigger_vars("EV_Door_Open", 0, &[1.0, 2.0]);
        host.sound_trigger_vars("ev_other", 1, &[3.0]);
        assert_eq!(host.fired_triggers, ["EV_Door_Open", "ev_other"]);
        assert_eq!(host.fired_trigger_vars, vec![("ev_door_open".to_string(), vec![1.0, 2.0])]);
    }

    #[test]
    fn departure_wants_keep_the_stops_asked_most_recently() {
        let mut host = VehicleHost::new(SimClock::default());
        for i in 0..MAX_HTML_DEPARTURE_STOPS + 3 {
            host.want_departures(format!("stop {i}"));
        }
        assert_eq!(host.html_departure_wants.len(), MAX_HTML_DEPARTURE_STOPS);
        assert_eq!(host.html_departure_wants.first().map(String::as_str), Some("stop 3"));
        assert_eq!(host.html_departure_wants.last().map(String::as_str), Some("stop 10"));
        // asked again: moves to the back and nothing is pushed out
        host.want_departures("stop 3".to_string());
        assert_eq!(host.html_departure_wants.len(), MAX_HTML_DEPARTURE_STOPS);
        assert_eq!(host.html_departure_wants.last().map(String::as_str), Some("stop 3"));
        assert_eq!(host.html_departure_wants.first().map(String::as_str), Some("stop 4"));
    }

    /// A callback's argument is rounded, as Omsi.exe's `fistp` does: 1100.9999 out of the
    /// script's arithmetic names terminus 1101, not 1100.
    #[test]
    fn callback_arguments_are_rounded() {
        let p = compile(&CompileInput::default());
        let mut state = State::new(&p);
        let mut host = VehicleHost::new(SimClock::default());
        let terminus = |code: i32, name: &str| omsi_vehicle::hof::Terminus { code, strings: vec![name.to_string()], ..Default::default() };
        host.hof = Some(Arc::new(Hof { termini: vec![terminus(1100, "A"), terminus(1101, "B")], ..Default::default() }));
        let mut stacks = Stacks::default();
        stacks.push(1100.9999);
        host.callback("GetTerminusIndex", 0, &mut stacks, &mut state);
        assert_eq!(stacks.pop(), 1.0);
        // index 0.9999, string 0: the second terminus' first string
        stacks.push(0.9999);
        stacks.push(0.0);
        host.callback("GetTerminusString", 0, &mut stacks, &mut state);
        assert_eq!(stacks.pop_str(), "B");
        // a half goes to the even number, as under Delphi's control word
        assert_eq!((arg_i32(0.5), arg_i32(1.5), arg_i32(2.5), arg_i32(-0.5)), (0, 2, 2, 0));
        assert_eq!(arg_idx(-1.0), usize::MAX);
    }

    /// A 24-bit BMP of `w` x `h`, white where `lit`.
    fn bmp(w: u32, h: u32, lit: impl Fn(u32, u32) -> bool) -> Vec<u8> {
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut b: Vec<u8> = Vec::new();
        b.extend(b"BM");
        b.extend(size.to_le_bytes());
        b.extend([0u8; 4]);
        b.extend(54u32.to_le_bytes());
        b.extend(40u32.to_le_bytes());
        b.extend(w.to_le_bytes());
        b.extend(h.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(24u16.to_le_bytes());
        b.extend([0u8; 24]);
        for y in (0..h).rev() {
            let mut r: Vec<u8> = (0..w).flat_map(|x| if lit(x, y) { [255u8; 3] } else { [0u8; 3] }).collect();
            r.resize(row as usize, 0);
            b.extend(r);
        }
        b
    }

    /// A matrix a script draws with fonts it asks for by name (the Krüger matrices) is drawn
    /// in the display font chosen for the bus: its letter fonts become the chosen one, fitted
    /// to their letters - as tall, on the same line, about as wide - also those it asked for
    /// before the font was chosen; its pictograms keep theirs, and so do the pictograms in a
    /// letter font (a `{` the script writes for a box of its own).
    #[test]
    fn a_matrix_script_draws_its_letters_in_the_chosen_display_font() {
        let dir = std::env::temp_dir().join(format!("omsi_host_display_font_{}", std::process::id()));
        let fonts = dir.join("Fonts");
        std::fs::create_dir_all(&fonts).unwrap();
        // a pixel font of 16 rows (each letter a block of 7 x 12 from row 2, a '{' a
        // chequer), and a pictogram font
        let letters: String = ('A'..='Z').collect();
        let mut oft = String::new();
        for name in ["Pix 16", "Pix Pictogram 16"] {
            oft.push_str(&format!("[newfont]\n{name}\npix.bmp\npix.bmp\n16\n1\n\n"));
            for (i, c) in letters.chars().enumerate() {
                oft.push_str(&format!("[char]\n{c}\n{}\n{}\n0\n\n", i * 8, i * 8 + 7));
            }
            oft.push_str("[char]\n{\n208\n214\n0\n\n");
        }
        std::fs::write(fonts.join("Pix.oft"), oft).unwrap();
        std::fs::write(fonts.join("pix.bmp"), bmp(216, 16, |x, y| if x < 208 { x % 8 != 7 && (2..14).contains(&y) } else { (x + y) % 2 == 0 })).unwrap();
        let ttf = dir.join("Hanken.ttf");
        std::fs::write(&ttf, include_bytes!("../../../../assets/fonts/HankenGrotesk/HankenGrotesk-latin-700.ttf")).unwrap();
        let p = compile(&CompileInput::default());
        let mut state = State::new(&p);
        let mut host = VehicleHost::new(SimClock::default());
        host.font_lib = Some(Arc::new(Mutex::new(FontLibrary::new(&dir))));
        let mut stacks = Stacks::default();
        let mut index = |host: &mut VehicleHost, name: &str| {
            stacks.push_str(name.to_string());
            host.callback("GetFontIndex", 0, &mut stacks, &mut state);
            stacks.pop()
        };
        // asked for before a font is chosen: its own
        assert_eq!(index(&mut host, "Pix 16"), 0.0);
        let own = host.font_atlas(0).unwrap();
        assert_eq!(own.font.name.as_str(), "Pix 16");
        let spec = omsi_content::dotfont::DisplayFontSpec::vector("Hanken Grotesk Bold", &ttf, 0);
        host.set_display_font(Some(ScriptDisplayFont { spec, fonts: vec!["pix 16".into(), "pix pictogram 16".into()] }));
        // the font the script holds is now the chosen one, as high as its own; the pictograms stay
        let chosen = host.font_atlas(0).unwrap();
        assert_eq!((chosen.font.name.as_str(), chosen.font.height), ("Hanken Grotesk Bold", 16));
        assert_eq!(index(&mut host, "Pix 16"), 0.0, "the same index");
        assert_eq!(index(&mut host, "Pix Pictogram 16"), 1.0);
        assert_eq!(host.font_atlas(1).map(|a| a.font.name.clone()).as_deref(), Some("Pix Pictogram 16"));
        // a missing font stays missing
        assert_eq!(index(&mut host, "++"), -1.0);
        // the script measures in it - about as wide as in its own, so it picks its sizes as
        // there - and draws it, every pixel on or off (a lit LED or none)
        stacks.push_str("HUB".to_string());
        stacks.push(0.0);
        host.callback("TextLength", 0, &mut stacks, &mut state);
        let w = stacks.pop();
        assert_eq!(w, chosen.font.text_width("HUB") as f32);
        let mine = own.font.text_width("HUB") as f32;
        assert!((w - mine).abs() <= mine * 0.15, "HUB is {w} px in the chosen font, {mine} in its own");
        host.script_textures.push(ScriptTexture::new(128, 32));
        for v in [0.0, 2.0, 8.0, 0.0, 1.0, 0.0] {
            stacks.push(v);
        }
        stacks.push_str("HUB".to_string());
        host.callback("STTextOut", 0, &mut stacks, &mut state);
        let alphas: Vec<u8> = host.script_textures[0].rgba.chunks(4).map(|c| c[3]).collect();
        assert!(alphas.iter().any(|a| *a == 255) && alphas.iter().all(|a| *a == 0 || *a == 255));
        // (the letters in the rows of its own: 2 to 13 of the line drawn from row 8)
        let rows: Vec<usize> = (0..32).filter(|y| (0..128).any(|x| alphas[y * 128 + x] > 0)).collect();
        assert_eq!((rows.first(), rows.last()), (Some(&10), Some(&21)), "{rows:?}");
        assert_eq!(host.negative_spacing, 0);
        // the '{' of the letter font is drawn with its own pixels
        let pixels = |a: &omsi_content::font::FontAtlas, ch: char| -> Vec<u8> {
            let g = a.font.glyph(ch).unwrap();
            (0..16).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize]).collect()
        };
        assert_eq!(pixels(&chosen, '{'), pixels(&own, '{'));
        assert_ne!(pixels(&chosen, 'H'), pixels(&own, 'H'));
        // a text spaced closer than nothing is counted (what `omsi-check --display-font` reports)
        for v in [0.0, 2.0, 8.0, 0.0, 1.0, -1.0] {
            stacks.push(v);
        }
        stacks.push_str("HUB".to_string());
        host.callback("STTextOut", 0, &mut stacks, &mut state);
        assert_eq!(host.negative_spacing, 1);
        // back to its own
        host.set_display_font(None);
        assert_eq!(host.font_atlas(0).map(|a| a.font.name.clone()).as_deref(), Some("Pix 16"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_radio_callbacks_give_the_station_and_its_song() {
        let p = compile(&CompileInput::default());
        let mut state = State::new(&p);
        let mut host = VehicleHost::new(SimClock::default());
        let mut stacks = Stacks::default();
        host.callback("GetRadioName", 0, &mut stacks, &mut state);
        assert_eq!(stacks.pop_str(), "", "the radio off");
        host.radio_name = "radioeins".into();
        host.radio_song = "Artist - Title".into();
        host.callback("GetRadioName", 0, &mut stacks, &mut state);
        host.callback("getradiosong", 0, &mut stacks, &mut state);
        assert_eq!(stacks.pop_str(), "Artist - Title");
        assert_eq!(stacks.pop_str(), "radioeins");
    }

    #[test]
    fn script_texture_colour_is_kept_per_texture() {
        let p = compile(&CompileInput::default());
        let mut state = State::new(&p);
        let mut host = VehicleHost::new(SimClock::default());
        host.script_textures.push(ScriptTexture::new(2, 1));
        host.script_textures.push(ScriptTexture::new(2, 1));
        host.script_textures[0].put(1, 0, [12, 34, 56, 78]);
        host.script_textures[1].color = [1, 2, 3, 255];
        let mut stacks = Stacks::default();

        // RHLib reads a source pixel, then asks its target texture whether its
        // previously selected drawing colour is transparent.
        stacks.push(0.0);
        stacks.push(1.0);
        stacks.push(0.0);
        host.callback("STReadPixel", 0, &mut stacks, &mut state);

        stacks.push(0.0);
        host.callback("STGetA", 0, &mut stacks, &mut state);
        assert_eq!(stacks.pop(), 78.0);
        stacks.push(1.0);
        host.callback("STGetA", 0, &mut stacks, &mut state);
        assert_eq!(stacks.pop(), 255.0);
    }

    #[test]
    fn stfilter_marks_a_script_texture_for_mipmaps() {
        let dir = std::env::temp_dir().join(format!("omsi_host_stfilter_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("matrix.osc");
        std::fs::write(
            &script,
            "{trigger:matrix_refresh}\n0 (M.V.STLock)\n0 (M.V.STUnlock)\n0 (M.V.STFilter)\n{end}\n",
        )
            .unwrap();
        let p = compile(&CompileInput {
            scripts: vec![script],
            ..Default::default()
        });
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        let mut host = VehicleHost::new(SimClock::default());
        host.script_textures.push(ScriptTexture::new(32, 16));
        host.script_textures[0].dirty = false;
        let mut st = State::new(&p);
        let mut vm = Vm::new();

        assert!(vm.run_trigger(&p, "matrix_refresh", &mut st, &mut host));
        assert!(!host.script_textures[0].locked);
        assert!(host.script_textures[0].mipmaps);
        assert!(host.script_textures[0].dirty);
    }

    #[test]
    fn atron_unlock_filter_relock_publishes_the_released_image() {
        let dir = std::env::temp_dir().join(format!(
            "omsi_host_atron_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("atron.osc");
        std::fs::write(
            &script,
            "{trigger:draw}\n0 (M.V.STLock)\n0 255 255 255 255 (M.V.STSetColor)\n0 1 1 (M.V.STDrawPixel)\n0 (M.V.STUnlock)\n0 (M.V.STFilter)\n0 (M.V.STLock)\n0 2 1 (M.V.STDrawPixel)\n{end}\n{trigger:publish}\n0 (M.V.STUnlock)\n0 (M.V.STFilter)\n0 (M.V.STLock)\n{end}\n",
        ).unwrap();
        let p = compile(&CompileInput {
            scripts: vec![script],
            ..Default::default()
        });
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        let mut host = VehicleHost::new(SimClock::default());
        host.script_textures.push(ScriptTexture::new(4, 2));
        let mut state = State::new(&p);
        let mut vm = Vm::new();
        assert!(vm.run_trigger(&p, "draw", &mut state, &mut host));
        let t = &mut host.script_textures[0];
        assert!(t.locked);
        assert!(t.mipmaps);
        let first = t.take_upload().expect("STUnlock must survive STLock in the same frame");
        assert_eq!(&first[20..24], &[255; 4]);
        assert_eq!(
            &first[24..28],
            &[0; 4],
            "edits after relocking wait for the next unlock"
        );
        assert!(t.take_upload().is_none());
        assert!(vm.run_trigger(&p, "publish", &mut state, &mut host));
        let second = host.script_textures[0].take_upload().unwrap();
        assert_eq!(&second[24..28], &[255; 4]);
        assert!(vm.run_trigger(&p, "publish", &mut state, &mut host));
        assert!(
            host.script_textures[0].take_upload().is_none(),
            "filtering unchanged pixels needs no new upload"
        );
    }

    #[test]
    fn atron_arrival_check_does_not_clear_sales_text_without_a_timetable() {
        let dir = std::env::temp_dir().join(format!(
            "omsi_host_atron_arrival_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("arrival.osc");
        std::fs::write(&script, "{trigger:check}\n(M.V.GetTTBusstopIndex) 1 - -1 >=\n{if}\n0 (M.V.STNewTex)\n{endif}\n{end}\n").unwrap();
        let p = compile(&CompileInput {
            scripts: vec![script],
            ..Default::default()
        });
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        let mut host = VehicleHost::new(SimClock::default());
        host.script_textures.push(ScriptTexture::new(1, 1));
        host.script_textures[0].put(0, 0, [255; 4]);
        let mut state = State::new(&p);
        let mut vm = Vm::new();
        assert!(vm.run_trigger(&p, "check", &mut state, &mut host));
        assert_eq!(host.script_textures[0].rgba, vec![255; 4]);
        // The first stop of an actual timetable still reports an arrival.
        host.tt_stops.push(("First stop".into(), 0.0, 0.0));
        assert!(vm.run_trigger(&p, "check", &mut state, &mut host));
        assert_eq!(host.script_textures[0].rgba, vec![0; 4]);
        host.script_textures[0].put(0, 0, [255; 4]);
        host.tt_stops.clear();
        assert!(vm.run_trigger(&p, "check", &mut state, &mut host));
        assert_eq!(host.script_textures[0].rgba, vec![255; 4]);
    }

    /// The engine part of the stock SD200/SD202/NL202 collision block: a rear hit low down
    /// damages the engine, one at the front only the general account.
    #[test]
    fn a_collision_block_reads_the_energy_twice() {
        let dir = std::env::temp_dir().join(format!("omsi_host_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("collision.osc");
        std::fs::write(
            &script,
            "{trigger:collision}\n(L.L.collision_energy) (L.S.coll_energy) + (S.L.collision_energy)\n(L.S.coll_pos_y) -4.70 <\n(L.S.coll_pos_z) 1.10 < &&\n{if}\n(L.L.collision_energy_eng) (L.S.coll_energy) + (S.L.collision_energy_eng)\n{endif}\n{end}\n",
        )
            .unwrap();
        let vars = dir.join("vars.txt");
        std::fs::write(&vars, "collision_energy\ncollision_energy_eng\n").unwrap();
        let p = compile(&CompileInput { varlists: vec![vars], scripts: vec![script], ..Default::default() });
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        let mut host = VehicleHost::new(SimClock::default());
        let mut st = State::new(&p);
        let mut vm = Vm::new();
        host.coll_pos = [-1.23, -5.73, 0.90];
        host.coll_energy = 115.5;
        assert!(vm.run_trigger(&p, "collision", &mut st, &mut host));
        host.coll_pos = [1.23, 5.62, 0.88];
        host.coll_energy = 20.0;
        vm.run_trigger(&p, "collision", &mut st, &mut host);
        assert_eq!(st.get(p.var("collision_energy").unwrap()), 135.5);
        assert_eq!(st.get(p.var("collision_energy_eng").unwrap()), 115.5);
    }
}

fn debug_text() -> bool {
    omsi_cfg::flags::OMSI_DEBUG_TEXT.is_set()
}
