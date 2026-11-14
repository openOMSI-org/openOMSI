use crate::{err, Answer};
use omsi_launcher_lib as core;
use omsi_timetable::{Line, Tour, TourTrip, Trip, TripProfile};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

const COPY_MARK: &str = ".openomsi-ttdata-copy";
const CHRONO_CFG_MARK: &str = ".openomsi-chrono-cfg-copy";
/// The trips made in a map's own TTData folder (no copy of it): one name a line, so that a
/// reset can take them away again.
const NEW_TRIPS: &str = ".openomsi-new-trips";
const PARTIAL: &str = "TTData.openomsi-partial";
// (the quote is written as its code, so that a tool reading the file's string literals keeps in step)
const NOT_IN_NAMES: [char; 9] = ['/', '\\', ':', '*', '?', '\u{22}', '<', '>', '|'];

static WRITING: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
pub struct LineEdit {
    name: String,
    #[serde(default)]
    user_allowed: bool,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    tours: Vec<TourEdit>,
}

#[derive(Deserialize)]
struct TourEdit {
    number: String,
    #[serde(default)]
    ai_group: String,
    #[serde(default)]
    extra: String,
    #[serde(default)]
    trips: Vec<TripEdit>,
}

#[derive(Deserialize)]
struct TripEdit {
    trip: String,
    #[serde(default)]
    profile: i32,
    departure: f32,
}

struct Place {
    content: PathBuf,
    omsi: Option<PathBuf>,
    map_dir: PathBuf,
    rel: PathBuf,
}

fn real(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = p;
    loop {
        if let Ok(mut found) = cur.canonicalize() {
            found.extend(rest.iter().rev());
            return found;
        }
        match (cur.file_name(), cur.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_owned());
                cur = parent;
            }
            _ => return p.to_path_buf(),
        }
    }
}

fn within(p: &Path, dir: &Path) -> bool {
    let parts = |x: &Path| -> Vec<String> {
        real(x)
            .components()
            .map(|c| {
                let s = c.as_os_str().to_string_lossy();
                if cfg!(any(windows, target_os = "macos")) {
                    s.to_lowercase()
                } else {
                    s.into_owned()
                }
            })
            .collect()
    };
    parts(p).starts_with(&parts(dir))
}

impl Place {
    fn of(map: &str) -> Result<Place, String> {
        let global = core::content_path(map).map_err(err)?;
        let map_dir = global.parent().map(Path::to_path_buf).ok_or_else(|| core::i18n::tr("{map} is not a map file").replace("{map}", &map))?;
        let rel = PathBuf::from(map.replace('\\', "/")).parent().map(Path::to_path_buf).unwrap_or_default();
        if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(core::i18n::tr("{map} is not a map file").replace("{map}", map));
        }
        let content = core::content_dir().ok_or_else(|| "no content folder".to_string())?;
        let root = core::load_config().root;
        let omsi = core::root_dir().ok().or_else(|| (!root.trim().is_empty()).then(|| PathBuf::from(root.trim())));
        Ok(Place { content, omsi, map_dir, rel })
    }

    fn original(&self, p: &Path) -> bool {
        let Some(omsi) = &self.omsi else { return false };
        within(p, omsi) && !(within(&self.content, omsi) && within(p, &self.content))
    }

    fn ours(&self, p: &Path) -> bool {
        within(p, &self.content) && !self.original(p)
    }

    fn folder(&self) -> String {
        self.map_dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
    }

    fn copy_dir(&self) -> PathBuf {
        self.content.join(&self.rel).join("TTData")
    }

    fn layer_copy_dir(&self, layer_tt: &Path) -> PathBuf {
        if let Ok(rel_layer) = layer_tt.strip_prefix(&self.map_dir) {
            if rel_layer.components().next().is_some_and(|c| c.as_os_str().eq_ignore_ascii_case("Chrono")) {
                return self.content.join(&self.rel).join(rel_layer);
            }
        }
        self.copy_dir()
    }
}

pub fn load(map: String, date: Option<String>) -> Answer {
    let global = core::content_path(&map).map_err(err)?;
    let map_dir = global.parent().map(Path::to_path_buf).ok_or_else(|| core::i18n::tr("{map} is not a map file").replace("{map}", &map))?;
    Ok(read(&map_dir, date.as_deref().unwrap_or("")))
}

fn read(map_dir: &Path, date: &str) -> Value {
    let mut d = core::timetable_for_date(map_dir, date);
    d.lines.sort_by_key(|l| l.name.to_lowercase());
    let lines: Vec<Value> = d
        .lines
        .iter()
        .map(|l| {
            let tours: Vec<Value> = l
                .tours
                .iter()
                .map(|t| {
                    let trips: Vec<Value> = t.trips.iter().map(|x| json!({ "trip": x.trip, "profile": x.profile, "departure": x.departure })).collect();
                    json!({ "number": t.number, "ai_group": t.ai_group, "extra": t.extra, "trips": trips })
                })
                .collect();
            json!({ "name": l.name, "user_allowed": l.user_allowed, "priority": l.priority, "tours": tours })
        })
        .collect();
    let trips: Vec<Value> = d
        .trips
        .iter()
        .map(|t| json!({ "name": t.name, "line": t.line, "terminus": t.terminus, "profiles": t.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "stops": t.stations, "minutes": t.profiles.first().map(|p| p.factor) }))
        .collect();
    // every stop the timetable knows (a name stands for one object), and the station links
    // between them (a trip may follow only these): [from, to, metres]
    let mut seen = std::collections::HashSet::new();
    let stops: Vec<Value> = d.bus_stops.iter().filter(|b| seen.insert(b.object_id)).map(|b| json!({ "id": b.object_id, "name": b.name.trim() })).collect();
    let mut linked = std::collections::HashSet::new();
    let links: Vec<Value> = d.stn_links.iter().filter(|l| linked.insert((l.from_id, l.to_id))).map(|l| json!([l.from_id, l.to_id, l.length.round()])).collect();
    let folder = map_dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    json!({ "folder": folder, "lines": lines, "trips": trips, "stops": stops, "links": links })
}

pub fn save(map: String, lines: Vec<LineEdit>, date: Option<String>) -> Answer {
    let place = Place::of(&map)?;
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let answer = save_lines_on(&place, lines, date.as_deref().unwrap_or(""));
    if answer["saved"].as_array().is_some_and(|s| !s.is_empty()) {
        omsi_cfg::content_changed();
    }
    Ok(answer)
}

#[cfg(test)]
fn save_lines(place: &Place, edits: Vec<LineEdit>) -> Value {
    save_lines_on(place, edits, "")
}

fn save_lines_on(place: &Place, edits: Vec<LineEdit>, date: &str) -> Value {
    let base_tt = omsi_cfg::resolve_path(&place.map_dir, "TTData");
    let known = core::timetable_for_date(&place.map_dir, date).lines;
    let mut saved = Vec::new();
    let mut error = None;
    for e in edits {
        let name = e.name.trim().to_string();
        match save_line(place, &base_tt, &known, e) {
            Ok(()) => saved.push(name),
            Err(why) => {
                error.get_or_insert(core::i18n::tr("line {name}: {why}").replace("{why}", &why).replace("{name}", &name));
            }
        }
    }
    json!({ "saved": saved, "error": error })
}

/// A trip made in the editor: the stops it calls at, in order.
#[derive(Deserialize)]
pub struct TripNew {
    name: String,
    /// The line its displays show (empty: a depot run).
    #[serde(default)]
    line: String,
    /// Selected timetable line whose tour receives this trip when adding from the builder.
    #[serde(default)]
    target_line: Option<String>,
    /// The destination text; the last stop's name when empty.
    #[serde(default)]
    terminus: String,
    stops: Vec<i64>,
    /// The minutes the trip takes, end to end; worked out from the links when 0.
    #[serde(default)]
    minutes: f32,
}

/// How fast a bus covers a link on average, and how long it stands at a stop, for the minutes
/// of a trip nobody gave any.
const SPEED_MPS: f64 = 6.5;
const DWELL_MIN: f64 = 0.35;

pub fn save_trip(map: String, trip: TripNew, date: Option<String>) -> Answer {
    let place = Place::of(&map)?;
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let name = write_trip_on(&place, trip, date.as_deref().unwrap_or(""))?;
    omsi_cfg::content_changed();
    Ok(json!({ "name": name }))
}

#[cfg(test)]
fn write_trip(place: &Place, e: TripNew) -> Result<String, String> {
    write_trip_on(place, e, "")
}

fn write_trip_on(place: &Place, e: TripNew, date: &str) -> Result<String, String> {
    let name = e.name.trim().to_string();
    if !file_name_ok(&name) {
        return Err(core::i18n::tr("not a file name"));
    }
    if e.stops.len() < 2 {
        return Err(core::i18n::tr("A trip needs two stops at least."));
    }
    let data = core::timetable_for_date(&place.map_dir, date);
    if data.trips.iter().any(|t| t.is_named(&name)) {
        return Err(core::i18n::trf("A trip named {name} is there already", &[("name", &name)]));
    }
    let name_of = |id: i64| data.bus_stops.iter().find(|b| b.object_id == id).map(|b| b.name.trim().to_string());
    let mut metres = 0.0;
    for w in e.stops.windows(2) {
        let Some(link) = data.stn_links.iter().find(|l| l.from_id == w[0] && l.to_id == w[1]) else {
            let (a, b) = (name_of(w[0]).unwrap_or_else(|| w[0].to_string()), name_of(w[1]).unwrap_or_else(|| w[1].to_string()));
            return Err(core::i18n::trf("The map has no way from {a} to {b}", &[("a", &a), ("b", &b)]));
        };
        metres += link.length;
    }
    let minutes = if e.minutes > 0.0 { e.minutes } else { ((metres / SPEED_MPS / 60.0 + DWELL_MIN * (e.stops.len() - 2) as f64).ceil() as f32).max(1.0) };
    let terminus = if e.terminus.trim().is_empty() { name_of(*e.stops.last().unwrap_or(&0)).unwrap_or_default() } else { e.terminus.trim().to_string() };
    let trip = Trip {
        terminus,
        line: e.line.trim().to_string(),
        stations: e.stops,
        profiles: vec![TripProfile { name: "standard".into(), factor: minutes, ..Default::default() }],
        ..Default::default()
    };
    let base_tt = omsi_cfg::resolve_path(&place.map_dir, "TTData");
    let chrono = core::chrono_dirs_for_date(&place.map_dir, date);
    let fits = |td: &omsi_timetable::TimetableData| {
        trip.stations.iter().all(|id| td.bus_stops.iter().any(|b| b.object_id == *id))
            && trip.stations.windows(2).all(|w| td.stn_links.iter().any(|l| l.from_id == w[0] && l.to_id == w[1]))
    };
    let needed_chrono = if chrono.is_empty() || fits(&omsi_timetable::TimetableData::load(&place.map_dir)) {
        None
    } else {
        (0..chrono.len())
            .find(|&i| fits(&omsi_timetable::TimetableData::load_with_chrono(&place.map_dir, &chrono[..=i], &[])))
            .or_else(|| Some(chrono.len() - 1))
            .map(|i| omsi_cfg::resolve_path(&chrono[i], "TTData"))
    };
    let target_name = e.target_line.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let line_tt = |line_name: &str| {
        data.lines
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(line_name))
            .and_then(|l| l.path.parent().map(Path::to_path_buf))
    };
    let layer_tt = match needed_chrono {
        Some(ev_tt) => {
            if target_name.and_then(line_tt).is_some_and(|dir| dir == base_tt) {
                return Err(core::i18n::tr("This trip uses a Chrono event stop or link and cannot be added to a base line"));
            }
            ev_tt
        }
        None => target_name
            .or_else(|| (!e.line.trim().is_empty()).then_some(e.line.trim()))
            .and_then(line_tt)
            .unwrap_or_else(|| base_tt.clone()),
    };
    let (dir, in_place) = ensure_writable_tt(place, &layer_tt)?;
    if in_place && !dir.is_dir() {
        std::fs::create_dir_all(&dir).map_err(err)?;
    }
    let target = dir.join(format!("{name}.ttp"));
    if place.original(&target) {
        return Err(core::i18n::tr("it would be written into the OMSI 2 folder"));
    }
    if target.exists() {
        return Err(core::i18n::trf("A trip named {name} is there already", &[("name", &name)]));
    }
    trip.save(&target).map_err(|e| core::i18n::tr(&err(e)))?;
    if in_place && !dir.join(COPY_MARK).is_file() {
        let tracked = dir.join(NEW_TRIPS);
        let mut list = std::fs::read_to_string(&tracked).unwrap_or_default();
        list.push_str(&format!("{name}.ttp
"));
        std::fs::write(&tracked, list).map_err(err)?;
    }
    Ok(name)
}

fn file_name_ok(name: &str) -> bool {
    if name.is_empty() || name.ends_with(['.', ' ']) || name.chars().any(|c| c.is_control() || NOT_IN_NAMES.contains(&c)) {
        return false;
    }
    let stem = name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    let numbered = |p: &str| stem.len() == 4 && stem.starts_with(p) && matches!(stem.as_bytes()[3], b'1'..=b'9');
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") && !numbered("COM") && !numbered("LPT")
}

fn save_line(place: &Place, default_tt: &Path, known: &[Line], e: LineEdit) -> Result<(), String> {
    let name = e.name.trim().to_string();
    if !file_name_ok(&name) {
        return Err(core::i18n::tr("not a file name"));
    }
    let mut tours = Vec::with_capacity(e.tours.len());
    for t in e.tours {
        let mut trips = Vec::with_capacity(t.trips.len());
        for x in t.trips {
            if !x.departure.is_finite() || x.departure < 0.0 {
                return Err(core::i18n::tr("tour {tour}: a departure is not a time").replace("{tour}", &t.number));
            }
            trips.push(TourTrip { trip: x.trip, profile: x.profile, departure: x.departure });
        }
        trips.sort_by(|a, b| a.departure.total_cmp(&b.departure));
        tours.push(Tour { number: t.number, ai_group: t.ai_group, extra: t.extra, trips });
    }
    let path = known.iter().find(|l| l.name.eq_ignore_ascii_case(&name)).map(|l| l.path.clone()).unwrap_or_else(|| default_tt.join(format!("{name}.ttl")));
    let line = Line { path, name, user_allowed: e.user_allowed, priority: e.priority, tours };
    let target = save_target(place, &line, default_tt)?;
    if place.original(&target) {
        return Err(core::i18n::tr("it would be written into the OMSI 2 folder"));
    }
    line.save(&target).map_err(|e| core::i18n::tr(&err(e)))
}

fn ensure_writable_tt(place: &Place, layer_tt: &Path) -> Result<(PathBuf, bool), String> {
    if place.ours(layer_tt) && omsi_cfg::vfs::archive_of(layer_tt).is_none() {
        return Ok((layer_tt.to_path_buf(), true));
    }
    let dir = place.layer_copy_dir(layer_tt);
    if place.original(&dir) {
        return Err(core::i18n::tr("its copy would go into the OMSI 2 folder"));
    }
    if !dir.is_dir() {
        if omsi_cfg::vfs::is_dir(layer_tt) {
            copy_whole(layer_tt, &dir)?;
        } else {
            std::fs::create_dir_all(&dir).map_err(err)?;
            std::fs::write(dir.join(COPY_MARK), b"TTData copied by the openOMSI launcher's timetable editor; its reset deletes this folder\n").map_err(err)?;
        }
        if let (Some(src_ev), Some(dst_ev)) = (layer_tt.parent(), dir.parent()) {
            let src_cfg = src_ev.join("Chrono.cfg");
            let dst_cfg = dst_ev.join("Chrono.cfg");
            if dst_ev != place.content.join(&place.rel) && omsi_cfg::vfs::is_file(&src_cfg) && !dst_cfg.exists() {
                if let Ok(bytes) = omsi_cfg::vfs::read(&src_cfg) {
                    std::fs::write(&dst_cfg, bytes).map_err(err)?;
                    std::fs::write(dst_ev.join(CHRONO_CFG_MARK), b"1").map_err(err)?;
                }
            }
        }
    }
    Ok((dir, false))
}

fn save_target(place: &Place, line: &Line, default_tt: &Path) -> Result<PathBuf, String> {
    let file = line.path.file_name().map(|f| f.to_owned()).unwrap_or_else(|| format!("{}.ttl", line.name).into());
    let layer_tt = line.path.parent().unwrap_or(default_tt);
    let (dir, in_place) = ensure_writable_tt(place, layer_tt)?;
    if in_place && !dir.is_dir() {
        std::fs::create_dir_all(&dir).map_err(err)?;
    }
    let target = dir.join(file);
    if in_place && !dir.join(COPY_MARK).is_file() {
        let orig = PathBuf::from(format!("{}.orig", target.display()));
        if target.is_file() && !orig.exists() {
            std::fs::copy(&target, &orig).map_err(err)?;
        }
    }
    Ok(target)
}

fn copy_whole(original: &Path, dir: &Path) -> Result<(), String> {
    let names = omsi_cfg::vfs::list_dir(original).ok_or_else(|| core::i18n::tr("{folder} cannot be read").replace("{folder}", &original.display().to_string()))?;
    let partial = dir.with_file_name(PARTIAL);
    if partial.exists() {
        std::fs::remove_dir_all(&partial).map_err(err)?;
    }
    std::fs::create_dir_all(&partial).map_err(err)?;
    for (name, is_dir) in names {
        if !is_dir {
            let bytes = omsi_cfg::vfs::read(&original.join(&name)).map_err(err)?;
            std::fs::write(partial.join(&name), bytes).map_err(err)?;
        }
    }
    std::fs::write(partial.join(COPY_MARK), b"TTData copied by the openOMSI launcher's timetable editor; its reset deletes this folder\n").map_err(err)?;
    std::fs::rename(&partial, dir).map_err(err)
}

pub fn reset(map: String) -> Answer {
    let place = Place::of(&map)?;
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let answer = reset_in(&place)?;
    omsi_cfg::content_changed();
    Ok(answer)
}

fn collect_ttdata_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    if !root.is_dir() {
        return;
    }
    let tt = root.join("TTData");
    if tt.is_dir() && !out.contains(&tt) {
        out.push(tt);
    }
    if let Ok(rd) = std::fs::read_dir(root.join("Chrono")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_chrono_ttdata(&p, out);
            }
        }
    }
}

fn collect_chrono_ttdata(dir: &Path, out: &mut Vec<PathBuf>) {
    let tt = dir.join("TTData");
    if tt.is_dir() && !out.contains(&tt) {
        out.push(tt);
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && !p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("TTData")) {
                collect_chrono_ttdata(&p, out);
            }
        }
    }
}

fn reset_in(place: &Place) -> Answer {
    let folder = place.folder();
    let copy_map = place.content.join(&place.rel);
    let mut copy_dirs = Vec::new();
    collect_ttdata_dirs(&copy_map, &mut copy_dirs);
    let mut removed = false;
    for copy in copy_dirs {
        if copy.join(COPY_MARK).is_file() && place.ours(&copy) {
            std::fs::remove_dir_all(&copy).map_err(err)?;
            removed = true;
            if let Some(ev) = copy.parent() {
                if ev != copy_map && ev.join(CHRONO_CFG_MARK).is_file() {
                    std::fs::remove_file(ev.join("Chrono.cfg")).ok();
                    std::fs::remove_file(ev.join(CHRONO_CFG_MARK)).ok();
                    let _ = std::fs::remove_dir(ev);
                }
            }
        }
    }
    if removed {
        return Ok(json!({ "folder": folder, "removed": true, "restored": 0 }));
    }
    let mut own_dirs = Vec::new();
    let own_base = omsi_cfg::resolve_path(&place.map_dir, "TTData");
    if own_base.is_dir() {
        own_dirs.push(own_base.clone());
    }
    collect_ttdata_dirs(&place.map_dir, &mut own_dirs);
    let chrono_root = omsi_cfg::resolve_path(&place.map_dir, "Chrono");
    if chrono_root.is_dir() {
        collect_chrono_ttdata(&chrono_root, &mut own_dirs);
    }
    let mut restored = 0;
    for own in own_dirs {
        if !(place.ours(&own) && own.is_dir()) {
            continue;
        }
        let tracked = own.join(NEW_TRIPS);
        if let Ok(list) = std::fs::read_to_string(&tracked) {
            for name in list.lines().map(str::trim).filter(|n| file_name_ok(n)) {
                if std::fs::remove_file(own.join(name)).is_ok() {
                    restored += 1;
                }
            }
            std::fs::remove_file(&tracked).ok();
        }
        for e in std::fs::read_dir(&own).map_err(err)?.flatten() {
            let p = e.path();
            if let Some(orig) = p.to_str().and_then(|s| s.strip_suffix(".orig")) {
                std::fs::rename(&p, orig).map_err(err)?;
                restored += 1;
            }
        }
        if own != own_base {
            let _ = std::fs::remove_dir(&own);
        }
    }
    Ok(json!({ "folder": folder, "removed": false, "restored": restored }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dirs {
        top: PathBuf,
        omsi: PathBuf,
        content: PathBuf,
    }

    impl Drop for Dirs {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.top).ok();
        }
    }

    fn dirs_at(name: &str, omsi: &str, content: &str) -> Dirs {
        omsi_launcher_lib::i18n::pin_language("ENG");
        let top = std::env::temp_dir().join(format!("openomsi-timetable-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&top).ok();
        let (omsi, content) = (top.join(omsi), top.join(content));
        std::fs::create_dir_all(&content).unwrap();
        std::fs::create_dir_all(&omsi).unwrap();
        Dirs { top, omsi, content }
    }

    fn dirs(name: &str) -> Dirs {
        dirs_at(name, "OMSI 2", "content")
    }

    fn map_at(root: &Path) -> PathBuf {
        let tt = root.join("maps").join("Grundorf").join("TTData");
        std::fs::create_dir_all(&tt).unwrap();
        std::fs::write(tt.join("1.ttl"), "[priority]\r\n0\r\n[newtour]\r\n1\r\nBusses\r\n1023\r\n[addtrip]\r\nA\r\n0\r\n360.000\r\n").unwrap();
        std::fs::write(tt.join("A.ttp"), "[trip]\r\n\r\nHauptbahnhof\r\n1\r\n").unwrap();
        root.join("maps").join("Grundorf")
    }

    fn place(d: &Dirs, map_dir: &Path) -> Place {
        Place { content: d.content.clone(), omsi: Some(d.omsi.clone()), map_dir: map_dir.to_path_buf(), rel: PathBuf::from("maps/Grundorf") }
    }

    fn edit(name: &str, departures: &[f32]) -> LineEdit {
        let trips = departures.iter().map(|d| TripEdit { trip: "A".into(), profile: 0, departure: *d }).collect();
        LineEdit { name: name.into(), user_allowed: true, priority: 0, tours: vec![TourEdit { number: "1".into(), ai_group: "Busses".into(), extra: "1023".into(), trips }] }
    }

    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(snapshot(&p));
            } else {
                out.push((p.clone(), std::fs::read(&p).unwrap()));
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_line_of_the_omsi_folder_is_saved_into_a_whole_copy_in_the_content_folder() {
        let d = dirs("copy");
        let map = map_at(&d.omsi);
        let p = place(&d, &map);
        let before = snapshot(&d.omsi);
        let answer = save_lines(&p, vec![edit("1", &[420.0, 380.0]), edit("N5", &[1450.0])]);
        assert_eq!(answer, json!({ "saved": ["1", "N5"], "error": null }));
        assert_eq!(snapshot(&d.omsi), before);
        let copy = d.content.join("maps").join("Grundorf").join("TTData");
        assert!(copy.join(COPY_MARK).is_file());
        assert!(copy.join("A.ttp").is_file());
        assert!(!copy.with_file_name(PARTIAL).exists());
        let one = Line::load(&copy.join("1.ttl")).unwrap();
        assert_eq!(one.tours[0].trips.iter().map(|t| t.departure).collect::<Vec<_>>(), vec![380.0, 420.0]);
        assert!(one.user_allowed);
        assert_eq!(Line::load(&copy.join("N5.ttl")).unwrap().tours[0].trips[0].departure, 1450.0);
        assert!(!copy.join("1.ttl.orig").exists());
        assert_eq!(reset_in(&p).unwrap(), json!({ "folder": "Grundorf", "removed": true, "restored": 0 }));
        assert!(!copy.exists());
        assert_eq!(snapshot(&d.omsi), before);
    }

    #[test]
    fn a_map_in_a_pack_folder_is_copied_to_the_same_place_in_the_content_folder() {
        let d = dirs("nested");
        let tt = d.omsi.join("maps").join("Pack").join("Grundorf").join("TTData");
        std::fs::create_dir_all(&tt).unwrap();
        std::fs::write(tt.join("A.ttp"), "[trip]\r\n\r\nHauptbahnhof\r\n1\r\n").unwrap();
        let mut p = place(&d, tt.parent().unwrap());
        p.rel = PathBuf::from("maps/Pack/Grundorf");
        assert_eq!(save_lines(&p, vec![edit("1", &[300.0])])["error"], json!(null));
        assert!(d.content.join("maps").join("Pack").join("Grundorf").join("TTData").join("1.ttl").is_file());
    }

    #[test]
    fn a_copy_left_half_made_is_made_again() {
        let d = dirs("partial");
        let map = map_at(&d.omsi);
        let partial = d.content.join("maps").join("Grundorf").join(PARTIAL);
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(partial.join("A.ttp"), "broken").unwrap();
        assert_eq!(save_lines(&place(&d, &map), vec![edit("1", &[300.0])])["error"], json!(null));
        let copy = d.content.join("maps").join("Grundorf").join("TTData");
        assert_eq!(std::fs::read(copy.join("A.ttp")).unwrap(), std::fs::read(map.join("TTData").join("A.ttp")).unwrap());
        assert!(!partial.exists());
    }

    #[test]
    fn a_map_unpacked_in_the_content_folder_keeps_its_first_line_as_orig_for_the_reset() {
        let d = dirs("orig");
        let map = map_at(&d.content);
        let p = place(&d, &map);
        let tt = map.join("TTData");
        let first = std::fs::read(tt.join("1.ttl")).unwrap();
        save_lines(&p, vec![edit("1", &[300.0])]);
        save_lines(&p, vec![edit("1", &[310.0])]);
        assert_eq!(std::fs::read(tt.join("1.ttl.orig")).unwrap(), first);
        assert_eq!(Line::load(&tt.join("1.ttl")).unwrap().tours[0].trips[0].departure, 310.0);
        assert!(!tt.join(COPY_MARK).exists());
        assert_eq!(reset_in(&p).unwrap(), json!({ "folder": "Grundorf", "removed": false, "restored": 1 }));
        assert_eq!(std::fs::read(tt.join("1.ttl")).unwrap(), first);
        assert!(!tt.join("1.ttl.orig").exists());
    }

    #[test]
    fn the_omsi_folder_inside_the_content_folder_is_never_written_to() {
        let d = dirs_at("inside", "Games/OMSI 2", "Games");
        let map = map_at(&d.omsi);
        std::fs::write(map.join("TTData").join("2.ttl.orig"), "kept").unwrap();
        let p = place(&d, &map);
        let before = snapshot(&d.omsi);
        assert_eq!(save_lines(&p, vec![edit("1", &[300.0])])["error"], json!(null));
        assert_eq!(snapshot(&d.omsi), before);
        assert!(d.content.join("maps").join("Grundorf").join("TTData").join("1.ttl").is_file());
        reset_in(&p).unwrap();
        reset_in(&p).unwrap();
        assert_eq!(snapshot(&d.omsi), before);
    }

    #[test]
    fn a_content_folder_inside_the_omsi_folder_is_written_to() {
        let d = dirs_at("contentinside", "OMSI 2", "OMSI 2/openOMSI");
        let map = map_at(&d.omsi);
        let p = place(&d, &map);
        let before = snapshot(&map);
        assert_eq!(save_lines(&p, vec![edit("1", &[300.0])])["error"], json!(null));
        assert_eq!(snapshot(&map), before);
        assert!(d.content.join("maps").join("Grundorf").join("TTData").join("1.ttl").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_map_folder_linked_into_the_omsi_folder_is_never_written_to() {
        let d = dirs("symlink");
        let map = map_at(&d.omsi);
        std::fs::write(map.join("TTData").join("2.ttl.orig"), "kept").unwrap();
        std::fs::create_dir_all(d.content.join("maps")).unwrap();
        std::os::unix::fs::symlink(&map, d.content.join("maps").join("Grundorf")).unwrap();
        let linked = d.content.join("maps").join("Grundorf");
        let p = place(&d, &linked);
        let before = snapshot(&d.omsi);
        let answer = save_lines(&p, vec![edit("1", &[300.0]), edit("N5", &[300.0])]);
        assert_eq!(answer["saved"], json!([]));
        assert!(answer["error"].as_str().unwrap().contains("OMSI 2 folder"));
        reset_in(&p).unwrap();
        assert_eq!(snapshot(&d.omsi), before);
    }

    #[test]
    fn a_name_that_is_no_file_name_writes_nothing() {
        let d = dirs("names");
        let map = map_at(&d.omsi);
        let p = place(&d, &map);
        for bad in ["", "  ", "a/b", "..\\x", "c:d", "e*", "f?", "g\"", "h<", "i>", "j|", "CON", "con", "Nul.txt", "com1", "LPT9.a", "tab\there", "dot.", "bell\u{7}"] {
            let answer = save_lines(&p, vec![edit(bad, &[300.0])]);
            assert_eq!(answer["saved"], json!([]), "{bad:?}");
            assert!(answer["error"].as_str().unwrap().contains("not a file name"), "{bad:?}");
        }
        for good in ["COM0", "CONSOLE", "N5", "Linie 7.1"] {
            assert!(file_name_ok(good), "{good:?}");
        }
        assert!(!d.content.join("maps").exists());
    }

    #[test]
    fn a_departure_before_midnight_is_refused() {
        let d = dirs("negative");
        let map = map_at(&d.omsi);
        let answer = save_lines(&place(&d, &map), vec![edit("1", &[-1.0])]);
        assert_eq!(answer["saved"], json!([]));
        assert!(answer["error"].as_str().unwrap().contains("not a time"));
    }

    #[test]
    fn the_reset_of_a_timetable_not_changed_here_touches_nothing() {
        let d = dirs("untouched");
        let map = map_at(&d.omsi);
        std::fs::write(map.join("TTData").join("2.ttl.orig"), "kept").unwrap();
        let before = snapshot(&d.omsi);
        assert_eq!(reset_in(&place(&d, &map)).unwrap(), json!({ "folder": "Grundorf", "removed": false, "restored": 0 }));
        assert_eq!(snapshot(&d.omsi), before);
    }

    #[test]
    fn the_page_reads_lines_and_the_trips_they_can_run() {
        let d = dirs("read");
        let map = map_at(&d.omsi);
        let v = read(&map, "");
        assert_eq!(v["folder"], json!("Grundorf"));
        assert_eq!(v["lines"][0]["name"], json!("1"));
        assert_eq!(v["lines"][0]["tours"][0]["trips"][0], json!({ "trip": "A", "profile": 0, "departure": 360.0 }));
        assert_eq!(v["trips"][0]["terminus"], json!("Hauptbahnhof"));
    }

    #[test]
    fn chrono_events_layer_into_the_timetable_for_matching_dates() {
        let d = dirs("chrono");
        let map = map_at(&d.omsi);
        let ev = map.join("Chrono").join("Fair");
        std::fs::create_dir_all(ev.join("TTData")).unwrap();
        std::fs::write(ev.join("Chrono.cfg"), "[startdate]\r\n20260917\r\n[enddate]\r\n20260920\r\n[deactivate_lines]\r\n1\r\n1\r\n").unwrap();
        std::fs::write(ev.join("TTData").join("S1.ttl"), "[priority]\r\n1\r\n[newtour]\r\n10\r\nBusses\r\n1023\r\n[addtrip]\r\nFairTrip\r\n0\r\n500.000\r\n").unwrap();
        std::fs::write(ev.join("TTData").join("FairTrip.ttp"), "[trip]\r\n\r\nFairground\r\n1\r\n").unwrap();

        let off_day = read(&map, "2026-09-10");
        assert_eq!(off_day["lines"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect::<Vec<_>>(), ["1"]);
        let active = read(&map, "2026-09-17");
        assert_eq!(active["lines"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect::<Vec<_>>(), ["S1"]);
        assert!(active["trips"].as_array().unwrap().iter().any(|t| t["name"] == "FairTrip" && t["terminus"] == "Fairground"));
    }

    #[test]
    fn chrono_edits_and_trips_stay_inside_event_layer_and_reset_restores_them() {
        let d = dirs("chrono_content");
        let map = map_at(&d.content);
        stops_and_links(&map);
        let ev = map.join("Chrono").join("Fair");
        std::fs::create_dir_all(ev.join("TTData")).unwrap();
        std::fs::write(ev.join("Chrono.cfg"), "[startdate]\r\n20260917\r\n[enddate]\r\n20260920\r\n[deactivate_lines]\r\n1\r\n1\r\n").unwrap();
        std::fs::write(ev.join("TTData").join("S1.ttl"), "[priority]\r\n1\r\n[newtour]\r\n10\r\nBusses\r\n1023\r\n[addtrip]\r\nFairTrip\r\n0\r\n500.000\r\n").unwrap();
        std::fs::write(ev.join("TTData").join("FairTrip.ttp"), "[trip]\r\n\r\nFairground\r\n1\r\n").unwrap();
        std::fs::write(ev.join("TTData").join("Busstops.cfg"), "[busstop]\r\nFairground\r\n0\r\n901\r\n0\r\n0\r\n0\r\n").unwrap();
        std::fs::write(ev.join("TTData").join("StnLinks.cfg"), "[StnLink]\r\n400\r\n103\r\n901\r\n0\r\n0\r\n0\r\n0\r\n0\r\n0\r\n").unwrap();

        let p = place(&d, &map);
        let base_before = snapshot(&map.join("TTData"));
        let s1_before = std::fs::read(ev.join("TTData").join("S1.ttl")).unwrap();

        assert_eq!(save_lines_on(&p, vec![edit("S1", &[620.0])], "2026-09-17")["saved"], json!(["S1"]));
        assert_eq!(write_trip_on(&p, new_trip("FairShuttle", &[103, 901]), "2026-09-17").unwrap(), "FairShuttle");

        assert_eq!(snapshot(&map.join("TTData")), base_before);
        assert!(ev.join("TTData").join("S1.ttl.orig").is_file());
        assert!(ev.join("TTData").join("FairShuttle.ttp").is_file());
        assert_eq!(read(&map, "2026-09-10")["lines"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect::<Vec<_>>(), ["1"]);
        assert_eq!(Line::load(&ev.join("TTData").join("S1.ttl")).unwrap().tours[0].trips[0].departure, 620.0);

        let reset = reset_in(&p).unwrap();
        assert_eq!(reset, json!({ "folder": "Grundorf", "removed": false, "restored": 2 }));
        assert_eq!(std::fs::read(ev.join("TTData").join("S1.ttl")).unwrap(), s1_before);
        assert!(!ev.join("TTData").join("S1.ttl.orig").exists());
        assert!(!ev.join("TTData").join("FairShuttle.ttp").exists());
    }

    #[test]
    fn base_trip_built_on_chrono_date_writes_to_base_and_event_only_trip_cannot_target_base_line() {
        let d = dirs("chrono_base_trip");
        let map = map_at(&d.content);
        stops_and_links(&map);
        let ev = map.join("Chrono").join("Fair");
        std::fs::create_dir_all(&ev).unwrap();
        std::fs::write(ev.join("Chrono.cfg"), "[startdate]\r\n20260917\r\n[enddate]\r\n20260920\r\n").unwrap();
        let ev_tt = ev.join("TTData");

        let p = place(&d, &map);
        let mut base_trip = new_trip("BaseHop", &[101, 102]);
        base_trip.target_line = Some("1".into());
        assert_eq!(write_trip_on(&p, base_trip, "2026-09-17").unwrap(), "BaseHop");
        assert!(map.join("TTData").join("BaseHop.ttp").is_file());
        assert!(!ev_tt.exists());
        let mut l1 = edit("1", &[540.0]);
        l1.tours[0].trips[0].trip = "BaseHop".into();
        let new_line = edit("N1", &[600.0]);
        assert_eq!(save_lines_on(&p, vec![l1, new_line], "2026-09-17")["saved"], json!(["1", "N1"]));
        assert!(map.join("TTData").join("N1.ttl").is_file());
        let off = read(&map, "2026-09-10");
        assert!(off["lines"].as_array().unwrap().iter().any(|l| l["name"] == "1"));
        assert!(off["lines"].as_array().unwrap().iter().any(|l| l["name"] == "N1"));
        assert!(off["trips"].as_array().unwrap().iter().any(|t| t["name"] == "BaseHop"));

        std::fs::create_dir_all(&ev_tt).unwrap();
        std::fs::write(ev_tt.join("Busstops.cfg"), "[busstop]\r\nFairground\r\n0\r\n901\r\n0\r\n0\r\n0\r\n").unwrap();
        std::fs::write(ev_tt.join("StnLinks.cfg"), "[StnLink]\r\n400\r\n103\r\n901\r\n0\r\n0\r\n0\r\n0\r\n0\r\n0\r\n").unwrap();
        let mut bad = new_trip("BadCross", &[103, 901]);
        bad.target_line = Some("1".into());
        assert!(write_trip_on(&p, bad, "2026-09-17").is_err());
        assert!(!map.join("TTData").join("BadCross.ttp").exists());
        assert!(!ev_tt.join("BadCross.ttp").exists());
    }

    #[test]
    fn layer_copy_ignores_chrono_in_parent_path_and_reset_drops_empty_chrono_ttdata() {
        let d = dirs_at("chrono_parent", "Chrono/OMSI 2", "content");
        let map = map_at(&d.omsi);
        let p = place(&d, &map);
        assert_eq!(p.layer_copy_dir(&map.join("TTData")), p.copy_dir());
        assert_eq!(
            p.layer_copy_dir(&map.join("Chrono").join("Fair").join("TTData")),
            d.content.join("maps").join("Grundorf").join("Chrono").join("Fair").join("TTData")
        );

        let c_map = map_at(&d.content);
        let cp = place(&d, &c_map);
        let p_ev = c_map.join("Chrono").join("Parade").join("TTData");
        let (dir, in_place) = ensure_writable_tt(&cp, &p_ev).unwrap();
        assert!(in_place);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NEW_TRIPS), "TempTrip.ttp\n").unwrap();
        std::fs::write(dir.join("TempTrip.ttp"), "[trip]\r\n\r\nEnd\r\n1\r\n").unwrap();
        assert_eq!(reset_in(&cp).unwrap()["restored"], json!(1));
        assert!(!p_ev.exists());
    }

    #[test]
    fn chrono_edits_on_read_only_omsi_map_copy_only_event_layer_and_clean_on_reset() {
        let d = dirs("chrono_ro");
        let map = map_at(&d.omsi);
        let ev = map.join("Chrono").join("Fair");
        std::fs::create_dir_all(ev.join("TTData")).unwrap();
        std::fs::write(ev.join("Chrono.cfg"), "[startdate]\r\n20260917\r\n[enddate]\r\n20260920\r\n").unwrap();
        std::fs::write(ev.join("TTData").join("S1.ttl"), "[priority]\r\n1\r\n[newtour]\r\n10\r\nBusses\r\n1023\r\n[addtrip]\r\nA\r\n0\r\n500.000\r\n").unwrap();

        let p = place(&d, &map);
        let before = snapshot(&d.omsi);
        assert_eq!(save_lines_on(&p, vec![edit("S1", &[715.0])], "2026-09-17"), json!({ "saved": ["S1"], "error": null }));
        assert_eq!(snapshot(&d.omsi), before);
        assert!(!d.content.join("maps").join("Grundorf").join("TTData").exists());
        let copy_ev_tt = d.content.join("maps").join("Grundorf").join("Chrono").join("Fair").join("TTData");
        assert!(copy_ev_tt.join(COPY_MARK).is_file());
        assert_eq!(Line::load(&copy_ev_tt.join("S1.ttl")).unwrap().tours[0].trips[0].departure, 715.0);

        assert_eq!(reset_in(&p).unwrap(), json!({ "folder": "Grundorf", "removed": true, "restored": 0 }));
        assert!(!copy_ev_tt.exists());
        assert_eq!(snapshot(&d.omsi), before);
    }

    fn stops_and_links(map: &Path) {
        let tt = map.join("TTData");
        std::fs::write(tt.join("Busstops.cfg"), "[busstop]\r\nHauptbahnhof\r\n0\r\n101\r\n0\r\n0\r\n0\r\n[busstop]\r\nMarkt\r\n0\r\n102\r\n0\r\n0\r\n0\r\n[busstop]\r\nDepot\r\n0\r\n103\r\n0\r\n0\r\n0\r\n").unwrap();
        std::fs::write(tt.join("StnLinks.cfg"), "[StnLink]\r\n650\r\n101\r\n102\r\n0\r\n0\r\n0\r\n0\r\n0\r\n0\r\n[StnLink]\r\n300\r\n102\r\n103\r\n0\r\n0\r\n0\r\n0\r\n0\r\n0\r\n").unwrap();
    }

    fn new_trip(name: &str, stops: &[i64]) -> TripNew {
        TripNew {
            name: name.into(),
            line: "1".into(),
            target_line: None,
            terminus: String::new(),
            stops: stops.to_vec(),
            minutes: 0.0,
        }
    }

    #[test]
    fn a_trip_made_on_the_map_follows_its_links_and_is_written_into_the_copy() {
        let d = dirs("newtrip");
        let map = map_at(&d.omsi);
        stops_and_links(&map);
        let p = place(&d, &map);
        let before = snapshot(&d.omsi);
        assert_eq!(write_trip(&p, new_trip("Hbf-Depot", &[101, 102, 103])).unwrap(), "Hbf-Depot");
        assert_eq!(snapshot(&d.omsi), before);
        let copy = d.content.join("maps").join("Grundorf").join("TTData");
        let t = Trip::load(&copy.join("Hbf-Depot.ttp")).unwrap();
        assert_eq!((t.terminus.as_str(), t.line.as_str()), ("Depot", "1"));
        assert_eq!(t.stations, vec![101, 102, 103]);
        // 950 m at 6.5 m/s is 2.4 min, and a stop in between stands 0.35: three minutes
        assert_eq!(t.profiles[0].factor, 3.0);
        assert!(write_trip(&p, new_trip("Hbf-Depot", &[101, 102])).unwrap_err().contains("there already"));
    }

    #[test]
    fn a_trip_between_stops_the_map_has_no_link_for_is_refused() {
        let d = dirs("nolink");
        let map = map_at(&d.omsi);
        stops_and_links(&map);
        let p = place(&d, &map);
        let e = write_trip(&p, new_trip("Gap", &[101, 103])).unwrap_err();
        assert!(e.contains("Hauptbahnhof") && e.contains("Depot"), "{e}");
        assert!(write_trip(&p, new_trip("One", &[101])).is_err());
        assert!(!d.content.join("maps").exists());
    }

    #[test]
    fn a_trip_made_in_a_content_map_goes_with_the_reset() {
        let d = dirs("newtripours");
        let map = map_at(&d.content);
        stops_and_links(&map);
        let p = place(&d, &map);
        write_trip(&p, new_trip("Mine", &[101, 102])).unwrap();
        let tt = map.join("TTData");
        assert!(tt.join("Mine.ttp").is_file());
        assert_eq!(reset_in(&p).unwrap()["restored"], json!(1));
        assert!(!tt.join("Mine.ttp").exists() && !tt.join(NEW_TRIPS).exists());
    }

    #[test]
    fn a_trip_that_could_not_be_written_is_not_taken_away_by_the_reset() {
        let d = dirs("newtripfail");
        let map = map_at(&d.content);
        stops_and_links(&map);
        let p = place(&d, &map);
        // a file of that name is there that the timetable does not list (it is no trip file)
        let tt = map.join("TTData");
        std::fs::create_dir_all(tt.join("Theirs.ttp")).unwrap();
        assert!(write_trip(&p, new_trip("Theirs", &[101, 102])).is_err());
        assert!(!tt.join(NEW_TRIPS).exists());
        reset_in(&p).unwrap();
        assert!(tt.join("Theirs.ttp").is_dir());
    }
}
