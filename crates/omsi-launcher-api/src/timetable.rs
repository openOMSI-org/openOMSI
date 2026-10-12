use crate::{err, Answer};
use omsi_launcher_lib as core;
use omsi_timetable::{Line, TimetableData, Tour, TourTrip};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

const COPY_MARK: &str = ".openomsi-ttdata-copy";
const PARTIAL: &str = "TTData.openomsi-partial";
const NOT_IN_NAMES: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

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
        let map_dir = global.parent().map(Path::to_path_buf).ok_or_else(|| format!("{map} is not a map file"))?;
        let rel = PathBuf::from(map.replace('\\', "/")).parent().map(Path::to_path_buf).unwrap_or_default();
        if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(format!("{map} is not a map file"));
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
}

pub fn load(map: String) -> Answer {
    let global = core::content_path(&map).map_err(err)?;
    let map_dir = global.parent().map(Path::to_path_buf).ok_or_else(|| format!("{map} is not a map file"))?;
    Ok(read(&map_dir))
}

fn read(map_dir: &Path) -> Value {
    let mut d = TimetableData::load(map_dir);
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
        .map(|t| json!({ "name": t.name, "line": t.line, "terminus": t.terminus, "profiles": t.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>() }))
        .collect();
    let folder = map_dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    json!({ "folder": folder, "lines": lines, "trips": trips })
}

pub fn save(map: String, lines: Vec<LineEdit>) -> Answer {
    let place = Place::of(&map)?;
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let answer = save_lines(&place, lines);
    if answer["saved"].as_array().is_some_and(|s| !s.is_empty()) {
        omsi_cfg::content_changed();
    }
    Ok(answer)
}

fn save_lines(place: &Place, edits: Vec<LineEdit>) -> Value {
    let original = omsi_cfg::resolve_path(&place.map_dir, "TTData");
    let known = TimetableData::load(&place.map_dir).lines;
    let mut saved = Vec::new();
    let mut error = None;
    for e in edits {
        let name = e.name.trim().to_string();
        match save_line(place, &original, &known, e) {
            Ok(()) => saved.push(name),
            Err(why) => {
                error.get_or_insert(format!("line {name}: {why}"));
            }
        }
    }
    json!({ "saved": saved, "error": error })
}

fn file_name_ok(name: &str) -> bool {
    if name.is_empty() || name.ends_with(['.', ' ']) || name.chars().any(|c| c.is_control() || NOT_IN_NAMES.contains(&c)) {
        return false;
    }
    let stem = name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    let numbered = |p: &str| stem.len() == 4 && stem.starts_with(p) && matches!(stem.as_bytes()[3], b'1'..=b'9');
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") && !numbered("COM") && !numbered("LPT")
}

fn save_line(place: &Place, original: &Path, known: &[Line], e: LineEdit) -> Result<(), String> {
    let name = e.name.trim().to_string();
    if !file_name_ok(&name) {
        return Err("not a file name".into());
    }
    let mut tours = Vec::with_capacity(e.tours.len());
    for t in e.tours {
        let mut trips = Vec::with_capacity(t.trips.len());
        for x in t.trips {
            if !x.departure.is_finite() || x.departure < 0.0 {
                return Err(format!("tour {}: a departure is not a time", t.number));
            }
            trips.push(TourTrip { trip: x.trip, profile: x.profile, departure: x.departure });
        }
        trips.sort_by(|a, b| a.departure.total_cmp(&b.departure));
        tours.push(Tour { number: t.number, ai_group: t.ai_group, extra: t.extra, trips });
    }
    let path = known.iter().find(|l| l.name.eq_ignore_ascii_case(&name)).map(|l| l.path.clone()).unwrap_or_else(|| original.join(format!("{name}.ttl")));
    let line = Line { path, name, user_allowed: e.user_allowed, priority: e.priority, tours };
    let target = save_target(place, &line, original)?;
    if place.original(&target) {
        return Err("it would be written into the OMSI 2 folder".into());
    }
    line.save(&target).map_err(err)
}

fn save_target(place: &Place, line: &Line, original: &Path) -> Result<PathBuf, String> {
    let file = line.path.file_name().map(|f| f.to_owned()).unwrap_or_else(|| format!("{}.ttl", line.name).into());
    if place.ours(&line.path) && omsi_cfg::vfs::archive_of(&line.path).is_none() {
        let orig = PathBuf::from(format!("{}.orig", line.path.display()));
        let copied = line.path.parent().is_some_and(|d| d.join(COPY_MARK).is_file());
        if !copied && line.path.is_file() && !orig.exists() {
            std::fs::copy(&line.path, &orig).map_err(err)?;
        }
        return Ok(line.path.clone());
    }
    let dir = place.copy_dir();
    if place.original(&dir) {
        return Err("its copy would go into the OMSI 2 folder".into());
    }
    if !dir.is_dir() {
        copy_whole(original, &dir)?;
    }
    Ok(dir.join(file))
}

fn copy_whole(original: &Path, dir: &Path) -> Result<(), String> {
    let names = omsi_cfg::vfs::list_dir(original).ok_or_else(|| format!("{} cannot be read", original.display()))?;
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

fn reset_in(place: &Place) -> Answer {
    let folder = place.folder();
    let copy = place.copy_dir();
    if copy.join(COPY_MARK).is_file() && place.ours(&copy) {
        std::fs::remove_dir_all(&copy).map_err(err)?;
        return Ok(json!({ "folder": folder, "removed": true, "restored": 0 }));
    }
    let own = omsi_cfg::resolve_path(&place.map_dir, "TTData");
    let mut restored = 0;
    if place.ours(&own) && own.is_dir() {
        for e in std::fs::read_dir(&own).map_err(err)?.flatten() {
            let p = e.path();
            if let Some(orig) = p.to_str().and_then(|s| s.strip_suffix(".orig")) {
                std::fs::rename(&p, orig).map_err(err)?;
                restored += 1;
            }
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
        let v = read(&map);
        assert_eq!(v["folder"], json!("Grundorf"));
        assert_eq!(v["lines"][0]["name"], json!("1"));
        assert_eq!(v["lines"][0]["tours"][0]["trips"][0], json!({ "trip": "A", "profile": 0, "departure": 360.0 }));
        assert_eq!(v["trips"][0]["terminus"], json!("Hauptbahnhof"));
    }
}
