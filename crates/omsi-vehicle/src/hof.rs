//! `.hof` depot files: termini, bus stop display strings, IBIS info system.

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Terminus {
    pub code: i32,
    /// Texture change id.
    pub texture_id: String,
    /// Bus stop where everybody leaves (None for `_allexit`).
    pub terminus_stop: Option<String>,
    pub all_exit: bool,
    pub strings: Vec<String>,
}

impl Terminus {
    /// Name used for destination text while preserving the original HOF string indices.
    pub fn display_name(&self) -> String {
        let first = self.strings.iter().find(|s| !s.trim().is_empty()).map(String::as_str);
        if first.is_some_and(|s| is_destination_image_path(s)) {
            return self.texture_id.trim().to_string();
        }
        first.map(str::trim).filter(|s| !s.is_empty()).unwrap_or_else(|| self.texture_id.trim()).to_string()
    }

    /// Name used by the destination menu: the identifier on the second `[addterminus]` line.
    pub fn menu_name(&self) -> String {
        let name = self.texture_id.trim();
        if !name.is_empty() {
            return name.to_string();
        }
        self.display_name()
    }
}

fn is_destination_image_path(s: &str) -> bool {
    let s = s.trim().to_ascii_lowercase();
    s.ends_with(".bmp") || s.ends_with(".tga") || s.ends_with(".png")
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BusStop {
    pub ident: String,
    pub strings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InfoTrip {
    pub code: String,
    pub name: String,
    pub route: String,
    pub line: String,
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hof {
    pub path: PathBuf,
    pub name: String,
    pub service_trip: String,
    pub global_strings: Vec<String>,
    pub string_count_terminus: usize,
    pub string_count_busstop: usize,
    pub termini: Vec<Terminus>,
    pub bus_stops: Vec<BusStop>,
    pub info_trips: Vec<InfoTrip>,
    pub info_busstop_lists: Vec<Vec<String>>,
    pub info_busstops: Vec<Vec<String>>,
}

/// The name of the destination `Hof::with_free_text` adds (its `[addterminus]` identifier).
pub const FREE_TEXT: &str = "(free text)";

impl Hof {
    pub fn depot_name(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.name.clone())
    }

    /// This depot file with one destination more, or the one it had of the kind changed:
    /// `lines` typed by the driver as the displays' text (the destination form's two free
    /// lines). Its strings are those a destination of the file has, as many: the first holds
    /// the first line, the second the second, every other the first again (a side or rear
    /// sign of one line shows the destination's main line); a display that reads its text
    /// through the scripts' `GetTerminusString` shows them. Both empty: a destination with
    /// nothing to show, the display cleared. Its code is the file's highest and one, so no
    /// destination of the file is taken for it. Returns the file and its index.
    pub fn with_free_text(&self, lines: [&str; 2]) -> (Hof, usize) {
        let mut hof = self.clone();
        let n = self.string_count_terminus.max(self.termini.iter().map(|t| t.strings.len()).max().unwrap_or(0)).max(2);
        let (a, b) = (lines[0].trim(), lines[1].trim());
        let strings = (0..n).map(|k| if k == 1 { b } else { a }.to_string()).collect();
        let index = hof.termini.iter().position(|t| t.texture_id == FREE_TEXT).unwrap_or(hof.termini.len());
        let code = self.termini.iter().filter(|t| t.texture_id != FREE_TEXT).map(|t| t.code).max().unwrap_or(0) + 1;
        let t = Terminus { code, texture_id: FREE_TEXT.to_string(), terminus_stop: None, all_exit: true, strings };
        if index == hof.termini.len() {
            hof.termini.push(t);
        } else {
            hof.termini[index] = t;
        }
        (hof, index)
    }

    pub fn load(path: &Path) -> Result<Hof, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }

    /// Whether the IBIS trips carry line `line` (a map's line name, "11-11s"): a trip coded
    /// line x 100 + route, or one named after the line ("91.06A -> MASSY GARE").
    pub fn has_line(&self, line: &str) -> bool {
        let l = line.trim().to_ascii_lowercase();
        if l.is_empty() {
            return false;
        }
        let digits: String = l.chars().take_while(|c| c.is_ascii_digit()).collect();
        let number = digits.parse::<i32>().ok().filter(|n| *n > 0);
        let encoded = encoded_hof_line(&l);
        let boundary = |rest: &str| rest.chars().next().is_none_or(|c| !c.is_ascii_alphanumeric());
        self.info_trips.iter().any(|t| {
            let code = omsi_cfg::parse_i32(&t.code);
            let head = t.name.split("->").next().unwrap_or("").trim().to_ascii_lowercase();
            let named = !head.is_empty()
                && (l.strip_prefix(head.as_str()).is_some_and(boundary)
                    || (l.ends_with(|c: char| c.is_ascii_digit())
                        && head.strip_prefix(l.as_str()).is_some_and(|r| r.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))));
            named
                || number.is_some_and(|n| code >= 100 && code / 100 == n)
                || encoded.is_some_and(|enc| code >= 100 && code / 100 == enc as i32)
        })
    }

    /// Only the `[name]` of a depot file ("" when it has none), kept for the session: the
    /// searches by name below read every depot file of every vehicle folder, and parsing
    /// each whole (termini, stops, the IVU trips) made a big installation's start take
    /// minutes.
    pub fn read_name(path: &Path) -> Option<String> {
        type Names = std::collections::HashMap<PathBuf, Option<String>>;
        static CACHE: std::sync::OnceLock<std::sync::Mutex<(u64, Names)>> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let generation = omsi_cfg::content_generation();
        {
            let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
            if c.0 != generation {
                *c = (generation, Names::new());
            }
            if let Some(n) = c.1.get(path) {
                return n.clone();
            }
        }
        let name = CfgFile::read(path).ok().map(|f| {
            let mut r = f.reader().with_rule(omsi_cfg::KeywordRule::TrimEnd);
            while let Some(k) = r.next_keyword() {
                if k == "name" {
                    return r.str().to_string();
                }
            }
            String::new()
        });
        cache.lock().unwrap_or_else(|e| e.into_inner()).1.insert(path.to_path_buf(), name.clone());
        name
    }

    pub fn parse(f: &CfgFile) -> Hof {
        let mut h = Hof { path: f.path.clone(), string_count_terminus: 0, string_count_busstop: 0, ..Default::default() };
        // `stringcount_terminus` / `stringcount_busstop` are bare (unbracketed) directives.
        for (i, l) in f.lines.iter().enumerate() {
            let w = l.trim_end();
            if w.eq_ignore_ascii_case("stringcount_terminus") {
                h.string_count_terminus = f.lines.get(i + 1).map(|s| omsi_cfg::parse_i64(s).max(0) as usize).unwrap_or(0);
            } else if w.eq_ignore_ascii_case("stringcount_busstop") {
                h.string_count_busstop = f.lines.get(i + 1).map(|s| omsi_cfg::parse_i64(s).max(0) as usize).unwrap_or(0);
            }
        }
        // the stock depot files are spreadsheet exports: every line ends in tabs, which the
        // original cuts off (with spaces and quotes) before looking at it
        let mut r = f.reader().with_rule(omsi_cfg::KeywordRule::TrimEnd);
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "name" => h.name = r.str().to_string(),
                "servicetrip" => h.service_trip = r.str().to_string(),
                "global_strings" => {
                    let n = r.count();
                    h.global_strings = (0..n).map(|_| r.str().to_string()).collect();
                }
                "addterminus" | "addterminus_allexit" => {
                    // Despite the SDK comment, no shipped file carries a separate terminus
                    // station line: every record is code, ident, then stringcount strings
                    // (verified over all stock .hof files). The ident doubles as the station.
                    let all_exit = k.ends_with("allexit");
                    let code = r.i32();
                    let texture_id = r.str().to_string();
                    let terminus_stop = if all_exit { None } else { Some(texture_id.clone()) };
                    let strings: Vec<String> = (0..h.string_count_terminus).map(|_| r.str().to_string()).collect();
                    h.termini.push(Terminus { code, texture_id, terminus_stop, all_exit, strings });
                }
                "addterminus_list" => {
                    // One row per terminus, tab separated: a flag column (`{ALLEX}` or
                    // empty), the code, the ident and the display strings - every one of the
                    // 3 549 rows of the stock and installed depot files has the flag column,
                    // empty ones included. Reading the code from the flag column gave every
                    // terminus without `{ALLEX}` the code 0 and its code as ident: typed
                    // destination codes were "wrong", a route found no terminus and the
                    // displays fell back to the first (empty) entry.
                    for l in r.until("[end]") {
                        if l.trim().is_empty() {
                            continue;
                        }
                        let cols: Vec<&str> = l.split('\t').collect();
                        let all_exit = cols[0].trim().eq_ignore_ascii_case("{ALLEX}");
                        let code = omsi_cfg::parse_i32(cols.get(1).unwrap_or(&"0"));
                        let texture_id = cols.get(2).unwrap_or(&"").trim().to_string();
                        let terminus_stop = if all_exit { None } else { Some(texture_id.clone()) };
                        let mut strings: Vec<String> = cols.iter().skip(3).map(|s| s.to_string()).collect();
                        if h.string_count_terminus > 0 {
                            strings.resize(h.string_count_terminus, String::new());
                        }
                        h.termini.push(Terminus { code, texture_id, terminus_stop, all_exit, strings });
                    }
                }
                "addbusstop" => {
                    let ident = r.str().to_string();
                    let strings = (0..h.string_count_busstop).map(|_| r.str().to_string()).collect();
                    h.bus_stops.push(BusStop { ident, strings });
                }
                "addbusstop_list" => {
                    for l in r.until("[end]") {
                        if l.trim().is_empty() {
                            continue;
                        }
                        let mut cols = l.split('\t');
                        let ident = cols.next().unwrap_or("").trim().to_string();
                        h.bus_stops.push(BusStop { ident, strings: cols.map(|s| s.to_string()).collect() });
                    }
                }
                "infosystem_trip" => {
                    let code = r.str().to_string();
                    let name = r.str().to_string();
                    let route = r.str().to_string();
                    let line = r.str().to_string();
                    h.info_trips.push(InfoTrip { code, name, route, line, extra: Vec::new() });
                    // every trip has a stop list, empty until one follows (THof.LoadFromFile
                    // 0x7ea142), so that the lists stay in step with the trips
                    h.info_busstop_lists.push(Vec::new());
                }
                "infosystem_busstop_list" => {
                    // the list of the trip read last (0x7ea16f: DynArrayHigh of the trips);
                    // pushed as one more list, a trip without one (the IVU data routes of
                    // some depot files) gave every later trip the stops of the one before
                    let n = r.count();
                    let list: Vec<String> = (0..n).map(|_| r.str().to_string()).collect();
                    if let Some(last) = h.info_busstop_lists.last_mut() {
                        *last = list;
                    }
                }
                "infosystem_busstop" => h.info_busstops.push((0..3).map(|_| r.str().to_string()).collect()),
                _ => {}
            }
        }
        h
    }

    pub fn terminus_by_code(&self, code: i32) -> Option<&Terminus> {
        self.termini.iter().find(|t| t.code == code)
    }
}

/// The `.hof` files available to a vehicle. Files next to the vehicle come first, merged
/// over all content roots (a mod's depot beside the installation's; archives read in place
/// too), followed by the shared top-level `HOFs/` folder. A vehicle-local file hides a
/// shared file of the same name; within each group files are sorted by name.
pub fn depot_files(dir: &Path) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut local: Vec<PathBuf> = Vec::new();
    for d in omsi_cfg::mirrored_dirs(dir) {
        for p in omsi_cfg::vfs::read_dir_paths(&d) {
            if !p.extension().map(|e| e.eq_ignore_ascii_case("hof")).unwrap_or(false) {
                continue;
            }
            if seen.insert(p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase()) {
                local.push(p);
            }
        }
    }
    local.sort_by_key(|f| f.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase());

    let mut shared: Vec<PathBuf> = omsi_cfg::read_dir_merged("HOFs")
        .into_iter()
        .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("hof")).unwrap_or(false))
        .filter(|p| seen.insert(p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase()))
        .collect();
    shared.sort_by_key(|f| f.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase());

    local.extend(shared);
    local
}

/// The depot file of `dir` called `name`: by its file name (without `.hof`) or its `[name]`.
pub fn depot_in(dir: &Path, name: &str) -> Option<Hof> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let files = depot_files(dir);
    if let Some(f) = files.iter().find(|f| f.file_stem().map(|s| s.to_string_lossy().trim().eq_ignore_ascii_case(name)).unwrap_or(false)) {
        if let Ok(h) = Hof::load(f) {
            return Some(h);
        }
    }
    files
        .iter()
        .filter(|f| Hof::read_name(f).is_some_and(|n| n.trim().eq_ignore_ascii_case(name)))
        .find_map(|f| Hof::load(f).ok())
}

/// The words of a depot or map name that tell one place from another: four letters or
/// more, not a year or a number, not a word every depot file has ("Linie 20", "Hof").
fn place_words(s: &str) -> Vec<String> {
    const COMMON: [&str; 14] = ["linie", "line", "lines", "depot", "omsi", "maps", "version", "final", "neue", "update", "addon", "fixed", "standard", "default"];
    s.split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_lowercase())
        .filter(|w| w.chars().count() >= 4 && !w.chars().all(|c| c.is_ascii_digit()) && !COMMON.contains(&w.as_str()))
        .collect()
}

/// Of `names` (the depot files a bus has), the one that belongs to the place `hints` name
/// (the map's depot names, its title, its folder): the one sharing the most of their
/// words, the longer words counting more; the first of equals. None when none shares one.
///
/// OMSI asks the driver which of the bus's depot files to use; taking the first of them
/// when none is called exactly as the map wants put a bus on Hamburg's Linie 20 with the
/// Grundorf depot of its folder - no line and no destination its IBIS knew (#896).
pub fn closest_name(names: &[&str], hints: &[&str]) -> Option<usize> {
    let wanted: Vec<String> = hints.iter().flat_map(|h| place_words(h)).collect();
    let mut best: Option<(usize, usize)> = None;
    for (i, n) in names.iter().enumerate() {
        let mut words = place_words(n);
        words.dedup();
        let score: usize = words.iter().filter(|w| wanted.contains(w)).map(|w| w.chars().count()).sum();
        if score > 0 && best.is_none_or(|(_, b)| score > b) {
            best = Some((i, score));
        }
    }
    best.map(|(i, _)| i)
}

/// The depot file of `dir` that belongs to the place `hints` name (see [`closest_name`]),
/// by its file name or its `[name]`.
pub fn depot_like(dir: &Path, hints: &[&str]) -> Option<Hof> {
    let files = depot_files(dir);
    let names: Vec<String> = files
        .iter()
        .map(|f| {
            let stem = f.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            match Hof::read_name(f) {
                Some(n) => format!("{stem} {n}"),
                None => stem,
            }
        })
        .collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    closest_name(&refs, hints).and_then(|i| Hof::load(&files[i]).ok())
}

/// The depot file called `name` in the shared `HOFs/` folder, any map folder (`maps/*/`) or any vehicle folder (`Vehicles/*/`) of any content root.
///
/// A depot file belongs to a map, not to a bus model: it lists the map's termini, stops and
/// IBIS codes. A mod bus brings only the depot of the map it was made on (the O530 Citaro
/// pack has Grundorf.hof alone), and on another map every code of the timetable was then
/// unknown to its IBIS - no line and no destination on any display. OMSI players copy the
/// map's .hof into such a folder; this finds the copy that is already installed with
/// another bus.
pub fn depot_anywhere(name: &str) -> Option<Hof> {
    // (asked for every type of AI bus without the map's depot: the file found is kept for
    // the session, and a big installation's thousands of folders are gone through once)
    type Found = std::collections::HashMap<String, Option<PathBuf>>;
    static FOUND: std::sync::OnceLock<std::sync::Mutex<(u64, Found)>> = std::sync::OnceLock::new();
    let found = FOUND.get_or_init(Default::default);
    let key = name.trim().to_ascii_lowercase();
    let generation = omsi_cfg::content_generation();
    let known = {
        let mut f = found.lock().unwrap_or_else(|e| e.into_inner());
        if f.0 != generation {
            *f = (generation, Found::new());
        }
        f.1.get(&key).cloned()
    };
    if let Some(path) = known {
        return path.and_then(|p| Hof::load(&p).ok());
    }
    let sorted = |folder: &str| {
        let mut dirs: Vec<PathBuf> = omsi_cfg::read_dir_merged(folder).into_iter().filter(|d| omsi_cfg::vfs::is_dir(d)).collect();
        dirs.sort_by_key(|d| d.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase());
        dirs
    };
    let mut dirs = sorted("maps");
    dirs.extend(sorted("Vehicles"));
    let h = dirs.iter().find_map(|d| depot_in(d, name));
    found.lock().unwrap_or_else(|e| e.into_inner()).1.insert(key, h.as_ref().map(|h| h.path.clone()));
    h
}

#[cfg(test)]
mod tests {
    #[test]
    fn free_text_is_a_destination_of_its_own() {
        let hof = Hof {
            string_count_terminus: 3,
            termini: vec![
                Terminus { code: 12, texture_id: "Rathaus".into(), strings: vec!["Rathaus".into(), "Spandau".into(), "Rathaus".into()], ..Default::default() },
                Terminus { code: 40, texture_id: "Hbf".into(), strings: vec!["Hbf".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let (one, i) = hof.with_free_text(["Sonderfahrt", " Betriebshof "]);
        assert_eq!(i, 2);
        assert_eq!(one.termini[2].strings, ["Sonderfahrt", "Betriebshof", "Sonderfahrt"]);
        assert_eq!(one.termini[2].code, 41);
        assert_eq!(one.termini[2].menu_name(), FREE_TEXT);
        // typed again: the same destination changed, not another one added
        let (two, j) = one.with_free_text(["", ""]);
        assert_eq!((j, two.termini.len(), two.termini[2].code), (2, 3, 41));
        assert!(two.termini[2].strings.iter().all(|s| s.is_empty()));
        assert_eq!(two.termini[..2], hof.termini[..]);
    }

    #[test]
    fn a_depot_file_carries_a_line_by_its_trip_codes_or_names() {
        let trip = |code: &str, name: &str| InfoTrip { code: code.into(), name: name.into(), ..Default::default() };
        let global = Hof { info_trips: vec![trip("1101", "11 -> PETIT VILTAIN"), trip("1401", "14 -> MOULON")], ..Default::default() };
        let inter = Hof { info_trips: vec![trip("001", "91.06A -> MASSY GARE"), trip("005", "91.06C -> MASSY GARE")], ..Default::default() };
        assert!(global.has_line("11-11s") && global.has_line("14"));
        assert!(!global.has_line("1") && !global.has_line("91.06C") && !global.has_line(""));
        assert!(inter.has_line("91.06C") && inter.has_line("91.06") && !inter.has_line("11-11s"));
    }

    #[test]
    fn encoded_prefixed_lines_find_their_routes() {
        let trip = |code: &str, name: &str, route: &str, line: &str| InfoTrip {
            code: code.into(),
            name: name.into(),
            route: route.into(),
            line: line.into(),
            ..Default::default()
        };
        let hof = Hof {
            termini: vec![
                Terminus {
                    code: 712,
                    texture_id: "Birkenweg".into(),
                    strings: vec!["Birkenweg".into()],
                    ..Default::default()
                },
                Terminus {
                    code: 154,
                    texture_id: "S+U Rathaus Spandau".into(),
                    strings: vec!["S+U Rathaus Spandau".into()],
                    ..Default::default()
                },
            ],
            info_trips: vec![
                trip("1501", "Day 15 -> Birkenweg", "712", "15"),
                trip("91502", "Birkenweg", "712", "915"),
                trip("93402", "Spandau", "154", "934"),
                trip("53402", "Kladow", "154", ""),
                trip("4101", "Hbf", "154", "41"),
                trip("101", "91.06A -> MASSY GARE", "154", "1"),
            ],
            ..Default::default()
        };
        assert_eq!(routes_to(&hof, "N15", &[712]), vec![1]);
        assert_eq!(routes_to(&hof, "N34", &[154]), vec![2]);
        assert_eq!(routes_to(&hof, "X34", &[154]), vec![3]);
        assert_eq!(routes_to(&hof, "M41", &[154]), Vec::<usize>::new());
        assert_eq!(routes_to(&hof, "91", &[154]), Vec::<usize>::new());
        assert!(hof.has_line("N15") && hof.has_line("N34") && hof.has_line("X34"));
        assert!(!hof.has_line("M41"));
    }

    use super::*;

    /// #896: the bus's own depot of the map's place, not the first of its folder.
    #[test]
    fn closest_depot_name_is_the_maps_place() {
        let names = ["Grundorf", "Hamburg Linie 20", "Spandau 2019"];
        assert_eq!(closest_name(&names, &["Hamburg_Linie_20", "Linie 20"]), Some(1));
        assert_eq!(closest_name(&names, &["Spandau 1986"]), Some(2));
        assert_eq!(closest_name(&names, &["Berlin-Spandau"]), Some(2));
        assert_eq!(closest_name(&names, &["Thüringer Wald"]), None);
        assert_eq!(closest_name(&["Berlin X10", "Spandau"], &["Berlin-Spandau"]), Some(1));
        assert_eq!(closest_name(&["Linie 20"], &["Linie 7"]), None);
    }

    #[test]
    fn shared_depots_are_available_but_vehicle_copy_wins() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("openomsi-shared-hof-{stamp}"));
        let bus = root.join("Vehicles/TestBus");
        let shared = root.join("HOFs");
        std::fs::create_dir_all(&bus).unwrap();
        std::fs::create_dir_all(&shared).unwrap();

        let local_name = format!("local-{stamp}.hof");
        let shared_name = format!("shared-{stamp}.hof");
        let same_name = format!("same-{stamp}.hof");
        std::fs::write(bus.join(&local_name), "[name]\nLocal\n").unwrap();
        std::fs::write(shared.join(&shared_name), "[name]\nShared\n").unwrap();
        std::fs::write(bus.join(&same_name), "[name]\nLocal duplicate\n").unwrap();
        std::fs::write(shared.join(&same_name), "[name]\nShared duplicate\n").unwrap();

        omsi_cfg::add_content_root(root.clone());
        let files = depot_files(&bus);
        let named = |p: &Path, name: &str| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n == name);
        let local_i = files.iter().position(|p| named(p, &local_name)).unwrap();
        let shared_i = files.iter().position(|p| named(p, &shared_name)).unwrap();
        let duplicate = files.iter().find(|p| named(p, &same_name)).unwrap();
        assert!(local_i < shared_i);
        assert_eq!(duplicate, &bus.join(&same_name));
        assert_eq!(depot_in(&bus, "Shared").map(|h| h.name), Some("Shared".into()));

        omsi_cfg::remove_content_root(&root);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_depot_kept_in_its_map_folder_is_found() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("openomsi-map-hof-{stamp}"));
        let map = root.join("maps/Moulon");
        std::fs::create_dir_all(&map).unwrap();
        std::fs::write(map.join(format!("company-{stamp}.hof")), format!("[name]\nCompany {stamp}\n")).unwrap();

        omsi_cfg::add_content_root(root.clone());
        let found = depot_anywhere(&format!("Company {stamp}")).map(|h| h.path);
        omsi_cfg::remove_content_root(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, Some(map.join(format!("company-{stamp}.hof"))));
    }

    #[test]
    fn depot_name_prefers_file_stem_over_shared_internal_name() {
        let dir = std::env::temp_dir().join(format!("openomsi-hof-stem-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("HOF1.hof"), "[name]\nShared\n").unwrap();
        std::fs::write(dir.join("HOF2.hof"), "[name]\nShared\n").unwrap();
        let h2 = Hof::load(&dir.join("HOF2.hof")).unwrap();
        assert_eq!(h2.depot_name(), "HOF2");
        let picked = depot_in(&dir, &h2.depot_name()).unwrap();
        assert_eq!(picked.path, dir.join("HOF2.hof"));
        let memory_only = Hof { name: "Fallback".into(), ..Default::default() };
        assert_eq!(memory_only.depot_name(), "Fallback");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn terminus_list_columns() {
        let text = "stringcount_terminus\r\n3\r\n\r\n[addterminus_list]\r\n{ALLEX}\t13\tBetriebsfahrt\tBETRIEBSFAHRT\t\tBETRIEBSFAHRT\t\t\r\n\t282\tU Ruhleben\tRUHLEBEN\tU-BAHNHOF\tRUHLEBEN  \t\t\t\r\n[end]\r\n";
        let h = Hof::parse(&CfgFile::from_str("test.hof", text));
        assert_eq!(h.termini.len(), 2);
        assert_eq!((h.termini[0].code, h.termini[0].texture_id.as_str(), h.termini[0].all_exit), (13, "Betriebsfahrt", true));
        assert_eq!(h.termini[0].strings, vec!["BETRIEBSFAHRT", "", "BETRIEBSFAHRT"]);
        assert_eq!((h.termini[1].code, h.termini[1].texture_id.as_str(), h.termini[1].all_exit), (282, "U Ruhleben", false));
        assert_eq!(h.termini[1].terminus_stop.as_deref(), Some("U Ruhleben"));
        assert_eq!(h.termini[1].strings, vec!["RUHLEBEN", "U-BAHNHOF", "RUHLEBEN  "]);
        assert_eq!(h.terminus_by_code(282).map(|t| t.texture_id.as_str()), Some("U Ruhleben"));
    }

    #[test]
    fn legacy_hof_destination_name_falls_back_to_ident() {
        let text = "stringcount_terminus\n6\n[addterminus]\n71910\n71 Eden Tunnel\n\n\n\n\nLegacyRoute\\71Y_1.bmp\n71Y\n";
        let h = Hof::parse(&CfgFile::from_str("legacy.hof", text));
        let t = h.terminus_by_code(71910).unwrap();
        assert_eq!(t.strings[0], "");
        assert_eq!(t.strings[4], "LegacyRoute\\71Y_1.bmp");
        assert_eq!(t.strings[5], "71Y");
        assert_eq!(t.display_name(), "71 Eden Tunnel");
        assert_eq!(t.menu_name(), "71 Eden Tunnel");
    }

    #[test]
    fn numeric_hof_menu_name_uses_the_ident() {
        let t = Terminus { texture_id: "910".into(), strings: vec!["AEC".into()], ..Default::default() };
        assert_eq!(t.menu_name(), "910");
        assert_eq!(t.display_name(), "AEC");
    }

    #[test]
    fn menu_names_do_not_use_matrix_text_or_empty_destination_markers() {
        let t = Terminus { texture_id: " Matrix check ".into(), strings: vec!["###".into(), "M A T R I X".into()], ..Default::default() };
        assert_eq!(t.menu_name(), "Matrix check");
        assert_eq!(t.display_name(), "###");

        let empty = Terminus { code: 0, texture_id: "empty".into(), all_exit: true, strings: vec!["A".into()], ..Default::default() };
        assert_eq!(empty.menu_name(), "empty");
        assert_eq!(empty.strings, vec!["A"]);
    }

    #[test]
    fn unnamed_hof_menu_entry_falls_back_to_display_text() {
        let t = Terminus { strings: vec!["".into(), "Fallback".into()], ..Default::default() };
        assert_eq!(t.menu_name(), "Fallback");
    }

    #[test]
    fn destination_names_and_blank_displays_survive_both_hof_formats() {
        // Synthetic records: both formats keep a destination whose display is blank.
        let records = [
            "[addterminus_allexit]\n0\nempty\n\n\n\n\n\n[addterminus_allexit]\n505\nMatrix check\n###\nM A T R I X\nCHECK\nx\nMC\n",
            "[addterminus_list]\n{ALLEX}\t0\tempty\t\t\t\t\t\n{ALLEX}\t505\tMatrix check\t###\tM A T R I X\tCHECK\tx\tMC\n[end]\n",
        ];
        for records in records {
            let h = Hof::parse(&CfgFile::from_str("menu.hof", &format!("stringcount_terminus\n5\n{records}")));
            assert_eq!(h.termini.len(), 2);
            let empty = &h.termini[0];
            assert_eq!((empty.code, empty.menu_name(), empty.all_exit), (0, "empty".into(), true));
            assert_eq!(empty.strings, vec![""; 5]);
            let check = &h.termini[1];
            assert_eq!((check.code, check.menu_name()), (505, "Matrix check".into()));
            assert_eq!(check.strings, vec!["###", "M A T R I X", "CHECK", "x", "MC"]);
        }
    }

    #[test]
    fn route_label_menu_name_uses_the_ident() {
        let t = Terminus { texture_id: "66: South Valley Railway Station Circular".into(), strings: vec!["S.VALLEY STN CIR".into()], ..Default::default() };
        assert_eq!(t.menu_name(), "66: South Valley Railway Station Circular");
    }

    /// #667: a trip without a stop list (an IVU data route) keeps the lists of the trips
    /// after it on their own trips.
    #[test]
    fn stop_lists_belong_to_the_trip_before_them() {
        let text = "[infosystem_trip]\r\n45581\r\nZOB-HOHENECK\r\n81\r\n455\r\n\r\n\
            [infosystem_busstop_list]\r\n2\r\nZOB\r\nHoheneck\r\n\r\n\
            [infosystem_trip]\r\n455900\r\nIVU\r\n81\r\n455\r\n\r\n\
            [infosystem_trip]\r\n45503\r\nHBF-BERGERFUERTH\r\n3\r\n455\r\n\r\n\
            [infosystem_busstop_list]\r\n3\r\nHauptbahnhof\r\nMarkt\r\nBergerfuerth\r\n";
        let h = Hof::parse(&CfgFile::from_str("test.hof", text));
        assert_eq!(h.info_trips.len(), 3);
        assert_eq!(h.info_busstop_lists.len(), 3);
        assert_eq!(h.info_busstop_lists[0], vec!["ZOB", "Hoheneck"]);
        assert!(h.info_busstop_lists[1].is_empty());
        assert_eq!(h.info_busstop_lists[2], vec!["Hauptbahnhof", "Markt", "Bergerfuerth"]);
    }
}

/// A timetable's terminus is a stop name, while a HOF often uses a shorter
/// destination/texture name for the same stop (Berlin 5E: `Spektefeld
/// Schulzentrum` vs. `Spektefeld`).  Exact matches remain preferred; the
/// boundary-aware prefix fallback handles those stock abbreviations without
/// making unrelated destinations match. The name a row goes by (its ident, its stop, the
/// first line of its sign) counts before another of its lines that reads the same: the
/// district line of Spandau's Machandelweg reads RUHLEBEN, the sign of U Ruhleben, and
/// U Ruhleben by its sign text (what a LAN player's bus says it shows) was Machandelweg.
pub fn terminus_match_score(t: &Terminus, wanted: &str) -> u8 {
    let wanted = wanted.split_whitespace().collect::<Vec<_>>().join(" ");
    if wanted.is_empty() {
        return 0;
    }
    let shown = t.strings.iter().position(|s| !s.trim().is_empty());
    let mut score = 0;
    for (k, candidate) in std::iter::once(t.texture_id.as_str())
        .chain(std::iter::once(t.terminus_stop.as_deref().unwrap_or("")))
        .chain(t.strings.iter().map(String::as_str))
        .enumerate()
    {
        let candidate = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
        let candidate_lower = candidate.to_lowercase();
        let wanted_lower = wanted.to_lowercase();
        if candidate_lower == wanted_lower {
            score = score.max(if k < 2 || Some(k - 2) == shown { 3 } else { 2 });
        } else if wanted_lower.starts_with(&(candidate_lower.clone() + " ")) {
            score = score.max(1);
        } else if candidate_lower.starts_with(&(wanted_lower + " ")) {
            score = score.max(1);
        }
    }
    score
}

/// Every row of the depot file a trip's destination names, in file order: those whose
/// ident is the name, else those of the best looser match. A depot file may give one
/// destination a row per route, each with a code of its own (four "ul. Xutorskaya" of
/// codes 92, 120, 123 and 124, #738).
pub fn termini_named(hof: &Hof, wanted: &str) -> Vec<usize> {
    let exact = wanted.trim();
    let rows: Vec<usize> = (0..hof.termini.len()).filter(|&i| hof.termini[i].texture_id == exact).collect();
    if !rows.is_empty() {
        return rows;
    }
    let scores: Vec<u8> = hof.termini.iter().map(|t| terminus_match_score(t, wanted)).collect();
    let best = scores.iter().copied().max().unwrap_or(0);
    (0..scores.len()).filter(|&i| best > 0 && scores[i] == best).collect()
}

/// The depot file's terminus a trip of `line` to `terminus` through `stops` ends at, and
/// the route it takes ([`pick_route`]): of the rows of that name (see [`termini_named`]) the
/// one the line's route through the trip's stops leads to. Taking the first of them, the
/// IBIS got the first's code, a route of the line to it or none at all, whichever route
/// the trip drove (line 39 to "ul. Xutorskaya" typed destination 92, where its route ends
/// at 120). The first row when no route of the line goes to any of them.
pub fn trip_terminus(
    hof: &Hof,
    line: &str,
    terminus: &str,
    stops: &[&str],
) -> Option<(usize, Option<usize>)> {
    let rows = termini_named(hof, terminus);
    let first = *rows.first()?;
    let mut codes: Vec<i32> = Vec::new();
    for &i in &rows {
        if !codes.contains(&hof.termini[i].code) {
            codes.push(hof.termini[i].code);
        }
    }
    let route = pick_route(hof, &routes_to(hof, line, &codes), stops);
    let ti = route
        .map(|r| omsi_cfg::parse_i32(&hof.info_trips[r].route))
        .and_then(|code| rows.iter().copied().find(|&i| hof.termini[i].code == code))
        .unwrap_or(first);
    Some((ti, route))
}

fn encoded_hof_line(line: &str) -> Option<u32> {
    let s = line.trim();
    let mut chars = s.chars();
    let prefix = chars.next().filter(|c| c.is_ascii_alphabetic())?.to_ascii_uppercase();
    let rest = chars.as_str();
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let num = rest.parse::<u32>().ok().filter(|&n| n < 100)?;
    match prefix {
        'N' => Some(900 + num),
        'X' => Some(500 + num),
        _ => None,
    }
}

/// The depot file's routes of `line` to a terminus with one of `codes`, in file order. The
/// route code is the line's number and two digits: a driver types those, whatever the
/// route's line string says (Grundorf's 7601 to Krankenhaus has "TML").
pub fn routes_to(hof: &Hof, line: &str, codes: &[i32]) -> Vec<usize> {
    let trimmed = line.trim();
    let line_digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    let line_number = line_digits.parse::<u32>().ok();
    let encoded = encoded_hof_line(trimmed);
    hof.info_trips
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            if !codes.contains(&omsi_cfg::parse_i32(&t.route)) {
                return false;
            }
            let hof_line = t
                .line
                .trim()
                .parse::<u32>()
                .ok()
                .or_else(|| t.code.trim().parse::<u32>().ok().map(|c| c / 100));
            t.line.trim().eq_ignore_ascii_case(trimmed)
                || (!line_digits.is_empty() && t.line.trim() == line_digits)
                || (line_number.is_some()
                    && t.code.trim().parse::<u32>().ok().map(|c| c / 100) == line_number)
                || encoded.is_some_and(|enc| hof_line == Some(enc))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Which of `routes` a trip through `stops` (the timetable's station names) runs: one
/// starting at the trip's first stop (the IBIS starts its count there), of those the one
/// that has most of the trip's stops in the trip's order, then the one as long as the trip
/// (a short working's route rather than the long one it is part of). Nothing in the
/// timetable ties a trip to a route of the depot file - a driver picks it by its stops -
/// and a line often has several routes to one terminus; taking the first whose first stop
/// was spelt as the timetable spells it typed the wrong one whenever the spellings
/// differed. None without routes; the first route when no stop matches any.
pub fn pick_route(hof: &Hof, routes: &[usize], stops: &[&str]) -> Option<usize> {
    let trip: Vec<(String, Vec<String>)> = stops
        .iter()
        .map(|s| (s.trim().to_lowercase(), stop_words(s)))
        .filter(|(s, _)| !s.is_empty())
        .collect();
    let mut best: Option<(usize, (bool, usize, std::cmp::Reverse<usize>))> = None;
    for &r in routes {
        let list = hof
            .info_busstop_lists
            .get(r)
            .map(|l| l.as_slice())
            .unwrap_or(&[]);
        let names: Vec<Vec<(String, Vec<String>)>> =
            list.iter().map(|id| ident_names(hof, id)).collect();
        // the longest common subsequence: a stop missing on either side costs nothing
        // but itself, and a stop the route lists twice is not
        // matched past the rest of the trip
        let mut row = vec![0usize; names.len() + 1];
        for t in &trip {
            let mut diag = 0;
            for (j, n) in names.iter().enumerate() {
                let up = row[j + 1];
                row[j + 1] = if same_stop(n, t) { diag + 1 } else { up.max(row[j]) };
                diag = up;
            }
        }
        let found = row[names.len()];
        let starts = match (trip.first(), names.first()) {
            (Some(t), Some(n)) => same_stop(n, t),
            _ => false,
        };
        let score = (
            starts,
            found,
            std::cmp::Reverse(names.len().abs_diff(stops.len())),
        );
        if best.as_ref().is_none_or(|(_, b)| score > *b) {
            best = Some((r, score));
        }
    }
    match best {
        Some((r, (_, found, _))) if found > 0 => Some(r),
        _ => routes.first().copied(),
    }
}

/// The names a stop of a route's list goes by: its ident (before a `#`) and the strings
/// the depot file's `[addbusstop]` of that ident gives it, each lowercased and as its
/// [`stop_words`].
pub fn ident_names(hof: &Hof, ident: &str) -> Vec<(String, Vec<String>)> {
    let ident = ident.split('#').next().unwrap_or("").trim();
    let mut names = vec![ident.to_string()];
    for b in &hof.bus_stops {
        if b.ident.trim().eq_ignore_ascii_case(ident) {
            names.extend(b.strings.iter().map(|s| s.trim().to_string()));
        }
    }
    names
        .into_iter()
        .filter(|n| !n.is_empty())
        .map(|n| (n.to_lowercase(), stop_words(&n)))
        .collect()
}

/// One stop of a route (its [`ident_names`]) and a timetable stop (lowercased, and its
/// [`stop_words`]) are the same: a name equal, one the start of the other, or the same
/// words.
pub fn same_stop(names: &[(String, Vec<String>)], stop: &(String, Vec<String>)) -> bool {
    names.iter().any(|(raw, words)| {
        !raw.is_empty()
            && (*raw == stop.0
            || raw.starts_with(&stop.0)
            || stop.0.starts_with(raw.as_str())
            || (!words.is_empty() && *words == stop.1))
    })
}

/// A stop name as the set of its words, so that the map's and the depot file's spellings
/// of one stop meet: in any order ("Nordstadt Bhf", "Bhf Nordstadt"), with any punctuation
/// ("Bhf. Nordstadt") and without one-letter prefixes ("F_Kirchweg", "Kirchweg").
pub fn stop_words(name: &str) -> Vec<String> {
    let mut w: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 1)
        .map(|w| w.to_lowercase())
        .collect();
    w.sort();
    w
}
