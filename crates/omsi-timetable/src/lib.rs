//! `maps/<map>/TTData/*` - bus stops, station links, trips, tracks and lines.

pub mod transit;

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

/// `Busstops.cfg` `[busstop]`: name, index within the group, object id, ?, ?, ?
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BusStopEntry {
    pub name: String,
    pub group: i32,
    pub object_id: i64,
    pub params: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StnLinkEntry {
    pub values: [f64; 7],
}

/// `StnLinks.cfg` `[StnLink]`: length, from stop id, to stop id, then 6 more numbers,
/// followed by `[StnLink_entry]` path steps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StnLink {
    pub length: f64,
    pub from_id: i64,
    pub to_id: i64,
    pub params: [f64; 6],
    pub entries: Vec<StnLinkEntry>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TripProfile {
    pub name: String,
    pub factor: f32,
    pub man_arr_time: Vec<(i32, f32)>,
    pub man_dep_time: Vec<(i32, f32)>,
    pub other_stopping: Vec<(i32, i32)>,
}

/// `*.ttp`
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trip {
    pub path: PathBuf,
    pub name: String,
    /// First `[trip]` line: the track (`.ttr`) the trip runs on, for a trip that has one -
    /// the trains, and the type-1 trips of mod maps whose `[station]` records point into
    /// it - else empty (a bus trip goes by its station links).
    pub display_name: String,
    pub terminus: String,
    pub line: String,
    pub train_reverse: bool,
    pub stations: Vec<i64>,
    /// legacy `[station]` blocks (8 lines)
    pub stations_legacy: Vec<Vec<String>>,
    pub profiles: Vec<TripProfile>,
}

/// `*.ttr` `[track_entry]`
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackEntry {
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    pub path: PathBuf,
    pub entries: Vec<TrackEntry>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TourTrip {
    pub trip: String,
    pub profile: i32,
    /// departure, minutes after midnight
    pub departure: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tour {
    pub number: String,
    pub ai_group: String,
    pub extra: String,
    pub trips: Vec<TourTrip>,
}

/// `*.ttl`
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub path: PathBuf,
    pub name: String,
    pub user_allowed: bool,
    pub priority: i32,
    pub tours: Vec<Tour>,
}

/// `car_use/*.ocu`
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CarUse {
    pub valid: (i32, i32),
    pub line: String,
    pub only_types: Vec<String>,
    pub types_prefered: Option<(f32, Vec<String>)>,
    pub number_tour: Vec<(String, String)>,
    pub type_tour: Vec<String>,
}

pub fn parse_busstops(f: &CfgFile) -> Vec<BusStopEntry> {
    let mut out = Vec::new();
    let mut r = f.reader();
    while let Some(k) = r.next_keyword() {
        if k == "busstop" {
            let name = r.str().to_string();
            let group = r.i32();
            let object_id = r.i64();
            let params = r.f64s::<3>();
            out.push(BusStopEntry { name, group, object_id, params });
        }
    }
    out
}

pub fn parse_stnlinks(f: &CfgFile) -> Vec<StnLink> {
    let mut out: Vec<StnLink> = Vec::new();
    let mut r = f.reader();
    while let Some(k) = r.next_keyword() {
        match k.as_str() {
            "stnlink" => {
                let length = r.f64();
                let from_id = r.i64();
                let to_id = r.i64();
                let params = r.f64s::<6>();
                out.push(StnLink { length, from_id, to_id, params, entries: Vec::new() });
            }
            "stnlink_entry" => {
                let values = r.f64s::<7>();
                if let Some(l) = out.last_mut() {
                    l.entries.push(StnLinkEntry { values });
                }
            }
            _ => {}
        }
    }
    out
}

impl Trip {
    pub fn load(path: &Path) -> Result<Trip, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }
    pub fn parse(f: &CfgFile) -> Trip {
        let mut t = Trip { path: f.path.clone(), name: f.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), ..Default::default() };
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "trip" => {
                    // three lines: its track (usually empty), the terminus, the line string
                    t.display_name = r.str().trim().to_string();
                    t.terminus = r.str().trim().to_string();
                    t.line = r.str().trim().to_string();
                }
                "trainreverse" => t.train_reverse = true,
                "station_typ2" => t.stations.push(r.i64()),
                "station" => t.stations_legacy.push((0..8).map(|_| r.str().to_string()).collect()),
                "profile" => {
                    let name = r.str().to_string();
                    let factor = r.f32();
                    t.profiles.push(TripProfile { name, factor, ..Default::default() });
                }
                "profile_man_arr_time" => {
                    let i = r.i32();
                    let v = r.f32();
                    if let Some(p) = t.profiles.last_mut() {
                        p.man_arr_time.push((i, v));
                    }
                }
                "profile_man_dep_time" => {
                    let i = r.i32();
                    let v = r.f32();
                    if let Some(p) = t.profiles.last_mut() {
                        p.man_dep_time.push((i, v));
                    }
                }
                "profile_otherstopping" => {
                    let i = r.i32();
                    let v = r.i32();
                    if let Some(p) = t.profiles.last_mut() {
                        p.other_stopping.push((i, v));
                    }
                }
                _ => {}
            }
        }
        t
    }
}

impl Track {
    pub fn load(path: &Path) -> Result<Track, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut t = Track { path: f.path.clone(), entries: Vec::new() };
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            if k == "track_entry" {
                let mut values = Vec::with_capacity(7);
                for _ in 0..6 {
                    values.push(r.f64());
                }
                // optional 7th value in newer files
                let save = r.pos();
                let l = r.str();
                if !l.trim().is_empty() && omsi_cfg::keyword_of(l).is_none() && !l.trim().ends_with(':') {
                    values.push(omsi_cfg::parse_f64(l));
                } else {
                    r.seek(save);
                }
                t.entries.push(TrackEntry { values });
            }
        }
        Ok(t)
    }
}

impl Line {
    pub fn load(path: &Path) -> Result<Line, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }
    pub fn parse(f: &CfgFile) -> Line {
        let mut l = Line { path: f.path.clone(), name: f.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), ..Default::default() };
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "userallowed" => l.user_allowed = true,
                "priority" => l.priority = r.i32(),
                "newtour" => {
                    let number = r.str().to_string();
                    let ai_group = r.str().to_string();
                    let extra = r.str().to_string();
                    l.tours.push(Tour { number, ai_group, extra, trips: Vec::new() });
                }
                "addtrip" => {
                    let trip = r.str().to_string();
                    let profile = r.i32();
                    let departure = r.f32();
                    if let Some(t) = l.tours.last_mut() {
                        t.trips.push(TourTrip { trip, profile, departure });
                    }
                }
                _ => {}
            }
        }
        l
    }
}

impl Line {
    /// The line as OMSI writes a `.ttl`: the header, `[userallowed]`, `[priority]`, then each
    /// tour with its trips (a `Dep.: h:m:s` note above each, as the game's editor puts it).
    pub fn to_text(&self) -> String {
        let mut o = String::new();
        o.push_str("-----------------------\r\nTime Table Line File\r\n-----------------------\r\n\r\nCreated with openOMSI\r\n\r\n");
        if self.user_allowed {
            o.push_str("[userallowed]\r\n\r\n");
        }
        o.push_str(&format!("[priority]\r\n{}\r\n", self.priority));
        for t in &self.tours {
            o.push_str("------------------------------------\r\n\r\n");
            o.push_str(&format!("[newtour]\r\n{}\r\n{}\r\n{}\r\n\r\n------------------------------------\r\n\r\n", t.number, t.ai_group, t.extra));
            for tr in &t.trips {
                let secs = (tr.departure as f64 * 60.0).round() as i64;
                o.push_str(&format!("  Dep.: {}:{}:{}\r\n", secs / 3600, secs / 60 % 60, secs % 60));
                o.push_str(&format!("[addtrip]\r\n{}\r\n{}\r\n{:.3}\r\n\r\n", tr.trip, tr.profile, tr.departure));
            }
        }
        o
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_text(path, &self.to_text())
    }
}

/// Writes a timetable file in the code page its folder's files are in (the file's own, when
/// it has one already), so a text typed in the launcher reads back the same in the game.
pub fn write_text(path: &Path, text: &str) -> std::io::Result<()> {
    use omsi_cfg::codepage::{detect, CodePage};
    let mut pages = Vec::new();
    if let Ok(old) = std::fs::read(path) {
        if !old.is_ascii() {
            pages.push(detect(&old));
        }
    }
    if let Some(page) = path.parent().and_then(timetable_code_page) {
        pages.push(page);
    }
    pages.extend([CodePage::Windows1252, CodePage::Windows1250, CodePage::Windows1251]);
    for page in pages {
        let (bytes, _, unmappable) = page.encoding().encode(text);
        if !unmappable && omsi_cfg::decode_text(&bytes) == text {
            return std::fs::write(path, bytes);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "its texts fit in no code page the map's timetable is written in"))
}

impl Trip {
    /// The trip as OMSI writes a `.ttp`: its terminus and line, the stops it calls at in
    /// order, and its profiles (name and total minutes; the game spreads the minutes over the
    /// stops by their distances where no stop has a time of its own).
    pub fn to_text(&self) -> String {
        let mut o = String::new();
        o.push_str("-----------------------\r\nTime Table Trip File\r\n-----------------------\r\n\r\nCreated with openOMSI\r\n\r\n");
        o.push_str(&format!("[trip]\r\n{}\r\n{}\r\n{}\r\n\r\n", self.display_name, self.terminus, self.line));
        o.push_str(".........................\r\n        Stations\r\n.........................\r\n\r\n");
        for id in &self.stations {
            o.push_str(&format!("[station_typ2]\r\n{id}\r\n\r\n"));
        }
        o.push_str("\r\n.........................\r\n        Profiles\r\n.........................\r\n\r\n");
        for p in &self.profiles {
            o.push_str(&format!("[profile]\r\n{}\r\n{:.3}\r\n\r\n", p.name, p.factor));
            for (i, v) in &p.man_dep_time {
                o.push_str(&format!("[profile_man_dep_time]\r\n{i}\r\n{v:.3}\r\n"));
            }
        }
        o
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_text(path, &self.to_text())
    }
}

fn timetable_code_page(dir: &Path) -> Option<omsi_cfg::codepage::CodePage> {
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    names.sort();
    names
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| ["ttp", "ttl", "cfg"].iter().any(|x| e.eq_ignore_ascii_case(x))))
        .filter_map(|p| std::fs::read(p).ok())
        .find(|b| !b.is_ascii())
        .map(|b| omsi_cfg::codepage::detect(&b))
}

impl CarUse {
    /// Whether the record is in force on `date` (yyyymmdd): OMSI keeps
    /// only a `[valid]` whose start is not after its end, and the original applies it
    /// from its start to its end day, both inclusive.
    pub fn valid_on(&self, date: i32) -> bool {
        let (a, b) = self.valid;
        a <= b && a <= date && date <= b
    }

    /// The types a tour of the line is given (`[onlytypes]`: always, `[types_prefered]`:
    /// with its probability), with that probability.
    pub fn types(&self) -> Option<(f32, &[String])> {
        if !self.only_types.is_empty() {
            return Some((1.0, &self.only_types));
        }
        self.types_prefered.as_ref().map(|(f, l)| (*f, l.as_slice()))
    }

    /// Every `car_use/*.ocu` of a map folder, in the order the folder lists them.
    pub fn load_dir(map_dir: &Path) -> Vec<CarUse> {
        // (every copy of the folder: the player's own rules of the launcher's AI Lists page are
        // in the content folder, the map's in the OMSI folder; a name in both is the content's)
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for d in omsi_cfg::mirrored_dirs(&omsi_cfg::resolve_path(map_dir, "car_use")) {
            for p in omsi_cfg::vfs::read_dir_paths(&d) {
                let is_ocu = p.extension().map(|e| e.eq_ignore_ascii_case("ocu")).unwrap_or(false);
                let name = p.file_name().map(|n| n.to_ascii_lowercase());
                if is_ocu && !files.iter().any(|f| f.file_name().map(|n| n.to_ascii_lowercase()) == name) {
                    files.push(p);
                }
            }
        }
        // (a name starting with `!` - the launcher's rules - sorts before the map's: the first
        // rule to give a tour its bus keeps it)
        files.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
        files.iter().filter_map(|p| CarUse::load(p).map_err(|e| log::warn!("{}: {e}", p.display())).ok()).collect()
    }

    pub fn load(path: &Path) -> Result<CarUse, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut c = CarUse::default();
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "valid" => {
                    let a = r.i32();
                    let b = r.i32();
                    c.valid = (a, b);
                }
                "line" => c.line = r.str().to_string(),
                "onlytypes" => c.only_types = r.until("[end]").into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                "types_prefered" => {
                    let factor = r.f32();
                    let list = r.rest_of_block().into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    c.types_prefered = Some((factor, list));
                }
                "number_tour" => {
                    for l in r.rest_of_block() {
                        if let Some((a, b)) = l.split_once('\t') {
                            c.number_tour.push((a.trim().to_string(), b.trim().to_string()));
                        }
                    }
                }
                "type_tour" => c.type_tour.push(r.str().to_string()),
                _ => {}
            }
        }
        Ok(c)
    }
}

/// All timetable data of a map.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimetableData {
    pub bus_stops: Vec<BusStopEntry>,
    pub stn_links: Vec<StnLink>,
    pub trips: Vec<Trip>,
    pub tracks: Vec<Track>,
    pub lines: Vec<Line>,
    pub errors: Vec<String>,
    /// Trips made from the map's `transport` routes that run on rails or off the station
    /// links: their route over the track is found once the map's network is known (see
    /// `omsi_sim::timetable_run::transit`).
    pub transit_trips: Vec<transit::TransitTrip>,
}

impl TimetableData {
    /// Load `TTData` under a map directory.
    pub fn load(map_dir: &Path) -> TimetableData {
        let mut d = TimetableData::default();
        d.merge_dir(&omsi_cfg::resolve_path(map_dir, "TTData"), true);
        d
    }

    /// Load `TTData` of a map plus the active chrono folders' (in the game's order), as
    /// the original does: stops, links, trips and tracks of a later folder replace
    /// those of the same name; a line (`<name>.ttl`) is taken from the last folder that has
    /// it, unless a scenario after that folder takes it off with `[deactivate_lines]`
    /// (`deactivated`: line and the chrono folder saying so). A scenario can so replace a
    /// line of the map, take it off, and a later one bring it back.
    pub fn load_with_chrono(map_dir: &Path, chrono_dirs: &[PathBuf], deactivated: &[(String, PathBuf)]) -> TimetableData {
        let mut folders = vec![omsi_cfg::resolve_path(map_dir, "TTData")];
        for c in chrono_dirs {
            let dir = omsi_cfg::resolve_path(c, "TTData");
            if omsi_cfg::vfs::is_dir(&dir) {
                folders.push(dir);
            } else {
                folders.push(PathBuf::new());
            }
        }
        let mut d = TimetableData::default();
        for f in folders.iter().filter(|f| !f.as_os_str().is_empty()) {
            d.merge_dir(f, false);
        }
        // the lines, from the last folder back: folder k is the map's (0) or chrono k-1's
        let off_after = |name: &str, k: usize| {
            deactivated.iter().any(|(l, dir)| l.trim().eq_ignore_ascii_case(name.trim()) && chrono_dirs.iter().position(|c| c == dir).is_some_and(|ci| ci + 1 > k))
        };
        for (k, f) in folders.iter().enumerate().rev() {
            if f.as_os_str().is_empty() {
                continue;
            }
            let mut names: Vec<PathBuf> = omsi_cfg::vfs::read_dir_paths(f);
            names.sort_by_key(|p| p.to_string_lossy().to_uppercase());
            for p in names {
                if !p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("ttl")) {
                    continue;
                }
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if d.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&stem)) || off_after(&stem, k) {
                    continue;
                }
                match Line::load(&p) {
                    Ok(l) => d.lines.push(l),
                    Err(e) => d.errors.push(e.to_string()),
                }
            }
        }
        // the routes of the map's `transport` folder run as lines too
        let mut issues = Vec::new();
        let transit = omsi_transit::TransitData::load(map_dir, &mut issues);
        if !transit.routes.list.is_empty() {
            let merged = transit::merge_transit(&mut d, &transit);
            log::info!("transport routes as timetable lines: {} lines, {} tours, {} trips", merged.lines, merged.tours, merged.trips);
            for s in &merged.skipped {
                log::warn!("transport routes: {s}");
            }
        }
        d
    }

    fn merge_dir(&mut self, dir: &Path, lines: bool) {
        let d = self;
        if let Ok(f) = CfgFile::read(dir.join("Busstops.cfg")) {
            for b in parse_busstops(&f) {
                if !d.bus_stops.iter().any(|x| x.object_id == b.object_id && x.group == b.group) {
                    d.bus_stops.push(b);
                }
            }
        }
        if let Ok(f) = CfgFile::read(dir.join("StnLinks.cfg")) {
            for l in parse_stnlinks(&f) {
                d.stn_links.retain(|x| !(x.from_id == l.from_id && x.to_id == l.to_id));
                d.stn_links.push(l);
            }
        }
        let mut names: Vec<PathBuf> = omsi_cfg::vfs::read_dir_paths(dir);
        names.sort();
        for p in names {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let res = match ext.as_str() {
                "ttp" => Trip::load(&p).map(|t| {
                    d.trips.retain(|x| !x.name.eq_ignore_ascii_case(&t.name));
                    d.trips.push(t)
                }),
                "ttr" => Track::load(&p).map(|t| {
                    d.tracks.retain(|x| x.path.file_stem().map(|s| !s.to_string_lossy().eq_ignore_ascii_case(&stem)).unwrap_or(true));
                    d.tracks.push(t)
                }),
                "ttl" if lines => Line::load(&p).map(|t| {
                    d.lines.retain(|x| !x.name.eq_ignore_ascii_case(&t.name));
                    d.lines.push(t)
                }),
                _ => Ok(()),
            };
            if let Err(e) = res {
                d.errors.push(e.to_string());
            }
        }
    }

    pub fn trip(&self, name: &str) -> Option<&Trip> {
        self.trips.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;

    #[test]
    fn line_text_reads_back() {
        let l = Line {
            name: "76".into(),
            user_allowed: true,
            priority: 1,
            tours: vec![Tour { number: "1".into(), ai_group: "Busses".into(), extra: "799".into(), trips: vec![TourTrip { trip: "76_BH-Kk".into(), profile: 0, departure: 247.0 }, TourTrip { trip: "76_Kk-BH".into(), profile: 1, departure: 262.5 }] }],
            ..Default::default()
        };
        let f = CfgFile::from_str("76.ttl", &l.to_text());
        let back = Line::parse(&f);
        assert_eq!(back.tours, l.tours);
        assert!(back.user_allowed);
        assert_eq!(back.priority, 1);
    }

    fn line_of(trip: &str) -> Line {
        Line { name: "5".into(), tours: vec![Tour { number: "1".into(), ai_group: "Busses".into(), extra: String::new(), trips: vec![TourTrip { trip: trip.into(), profile: 0, departure: 360.0 }] }], ..Default::default() }
    }

    fn folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-ttl-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_cyrillic_trip_name_is_written_in_the_maps_code_page_and_read_back() {
        let d = folder("cyrillic");
        let (ttp, _, _) = omsi_cfg::codepage::CodePage::Windows1251.encoding().encode("[trip]\r\n\r\nВокзал\r\n5\r\n");
        std::fs::write(d.join("5_Вокзал.ttp"), ttp).unwrap();
        let line = line_of("5_Площадь-Вокзал");
        line.save(&d.join("5.ttl")).unwrap();
        let bytes = std::fs::read(d.join("5.ttl")).unwrap();
        assert!(!bytes.windows(2).any(|w| w == b"&#"));
        assert_eq!(Line::load(&d.join("5.ttl")).unwrap().tours[0].trips[0].trip, "5_Площадь-Вокзал");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_new_line_with_cyrillic_trips_in_an_ascii_folder_is_still_read_back() {
        let d = folder("plain");
        line_of("5_Площадь-Вокзал").save(&d.join("5.ttl")).unwrap();
        assert_eq!(Line::load(&d.join("5.ttl")).unwrap().tours[0].trips[0].trip, "5_Площадь-Вокзал");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn texts_no_code_page_holds_are_refused_and_nothing_is_written() {
        let d = folder("mixed");
        assert!(line_of("5_Łódź-Вокзал").save(&d.join("5.ttl")).is_err());
        assert!(!d.join("5.ttl").exists());
        std::fs::remove_dir_all(&d).ok();
    }
}

#[cfg(test)]
mod stock_line_tests {
    use super::*;
    include!("../../../tools/test-support/original_root.rs");

    /// Every line of the stock maps reads back the same after it is written.
    #[test]
    #[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
    fn stock_lines_round_trip() {
        let maps = original_root().join("maps");
        let mut n = 0;
        for m in ["Grundorf", "Berlin-Spandau"] {
            let d = maps.join(m).join("TTData");
            require_content(&[&d]);
            let rd = std::fs::read_dir(&d).unwrap();
            for e in rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("ttl"))) {
                let l = Line::load(&e.path()).unwrap();
                let back = Line::parse(&CfgFile::from_str(e.path(), &l.to_text()));
                assert_eq!(back.tours, l.tours, "{}", e.path().display());
                assert_eq!((back.user_allowed, back.priority), (l.user_allowed, l.priority));
                n += 1;
            }
        }
        eprintln!("{n} lines");
    }
}

#[cfg(test)]
mod trip_text_tests {
    use super::*;

    #[test]
    fn a_written_trip_reads_back_the_same() {
        let t = Trip {
            terminus: "Nervenklinik".into(),
            line: "130".into(),
            stations: vec![3211605, 3211600, 524],
            profiles: vec![TripProfile { name: "standard".into(), factor: 12.5, ..Default::default() }],
            ..Default::default()
        };
        let back = Trip::parse(&CfgFile::from_str(PathBuf::from("T.ttp"), &t.to_text()));
        assert_eq!((back.terminus.as_str(), back.line.as_str()), ("Nervenklinik", "130"));
        assert_eq!(back.display_name, "");
        assert_eq!(back.stations, t.stations);
        assert_eq!(back.profiles.len(), 1);
        assert_eq!((back.profiles[0].name.as_str(), back.profiles[0].factor), ("standard", 12.5));
    }
}
