//! `ailists.cfg`, `unsched_vehgroups.txt`, `unsched_trafficdens.txt`, `parklist_p.txt`,
//! `humans.txt`, `drivers.txt`, `registrations.txt`.

use omsi_cfg::CfgFile;
use std::path::PathBuf;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiVehicleEntry {
    pub file: String,
    /// Weight (`[aigroup_2]`) or number/registration (`[aigroup_depot_typgroup_2]`).
    pub weight: f32,
    pub number: Option<String>,
    pub registration: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiGroup {
    pub name: String,
    pub vehicles: Vec<AiVehicleEntry>,
    /// Depot groups: HOF name.
    pub hof: Option<String>,
    pub is_depot: bool,
    /// Depot type groups: vehicle file + list of numbers/registrations.
    pub typgroups: Vec<AiTypGroup>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiTypGroup {
    pub file: String,
    pub entries: Vec<DepotEntry>,
}

/// One vehicle of a depot's type group: an `[aigroup_depot_typgroup_2]` line is its fleet
/// number, plate, repaint and first and last day (YYYYMMDD), tab-separated (Omsi.exe
/// 0x780a58 splits it at each tab with 0x7ef900, empty fields kept); OMSI 1's
/// `[aigroup_depot_typgroup]` gives the number and the repaint on two lines (0x7814b0).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DepotEntry {
    pub number: String,
    pub registration: String,
    pub paint: String,
    pub from: Option<i32>,
    pub to: Option<i32>,
}

impl DepotEntry {
    /// A `[aigroup_depot_typgroup_2]` line.
    pub fn parse(line: &str) -> DepotEntry {
        let mut f = line.split('\t');
        let mut next = || f.next().unwrap_or("").to_string();
        let (number, registration, paint, from, to) = (next(), next(), next(), next(), next());
        let date = |s: &str| s.trim().parse::<i32>().ok();
        DepotEntry {
            number: number.trim().to_string(),
            registration,
            paint,
            from: date(&from),
            to: date(&to),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiLists {
    pub groups: Vec<AiGroup>,
    /// The default group (index into `groups`): the `[ailist]` header's second number, the
    /// "NotInGroup" group it makes for -1, else the first group (Omsi.exe 0x78093c sets 0,
    /// 0x780a58 reads the header). A map without `unsched_vehgroups.txt` takes its random
    /// traffic from this group alone (0x785f98: the one group it makes then has no name,
    /// and a nameless group is the default group).
    pub default_group: usize,
}

/// Put an OMSI 1 car group into `groups`: in the place of a car group of the same name read
/// before (it starts over), else at the end. Its index.
fn put_car_group(groups: &mut Vec<AiGroup>, group: AiGroup) -> usize {
    let key = group.name.trim().to_ascii_lowercase();
    for (i, old) in groups.iter_mut().enumerate() {
        if !old.is_depot && old.name.trim().to_ascii_lowercase() == key {
            *old = group;
            return i;
        }
    }
    groups.push(group);
    groups.len() - 1
}

impl AiLists {
    pub fn load(path: &Path) -> Result<AiLists, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }

    pub fn parse(f: &CfgFile) -> AiLists {
        let mut a = AiLists::default();
        // the original lower-cases every line of this file before looking at it
        let mut r = f.reader().with_rule(omsi_cfg::KeywordRule::AnyCase);
        // the old format's list (`[ailist]`): (default group, vehicles)
        let mut legacy: Option<(i32, Vec<String>)> = None;
        // the group the type groups that follow belong to: only a depot takes them (Omsi.exe
        // 0x780a58 logs "No [aigroup_depot_typgroup] allowed after ..." for any other)
        let mut depot: Option<usize> = None;
        // OMSI 1's groups name their vehicles by their place in `[ailist]`: (group, indices)
        // and (group, type group, index), turned into files once the whole file is read
        let mut old_groups: Vec<(usize, Vec<i64>)> = Vec::new();
        let mut old_typgroups: Vec<(usize, usize, i64)> = Vec::new();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                // the original: a line it passes over, the default group's index, a
                // count and that many vehicle files
                "ailist" => {
                    let _ = r.str();
                    let default = r.str().trim().parse::<i32>().unwrap_or(-1);
                    let n = r.str().trim().parse::<usize>().unwrap_or(0).min(10_000);
                    // (kept with empty lines: OMSI 1's groups count places in this list)
                    let files: Vec<String> = (0..n).map(|_| r.str().trim().to_string()).collect();
                    legacy = Some((default, files));
                }
                // OMSI 1 (Omsi.exe 0x780a58 at 0x780bf0): the group's name, a count, that
                // many places in the `[ailist]` list (weight 1 each) and the depot line, which
                // may be empty - no `[end]`. A car group of a name read before starts over in
                // its place. Read as `[aigroup_2]`, OMSI 1 Velbert's first group took the
                // rest of the file with it and the map had no traffic.
                "aigroup" => {
                    let name = r.str().to_string();
                    let lines_left = r.lines().len().saturating_sub(r.pos());
                    let count = r.str().trim().parse::<usize>().unwrap_or(0).min(10_000).min(lines_left);
                    let mut places = Vec::with_capacity(count);
                    for _ in 0..count {
                        if let Ok(place) = r.str().trim().parse::<i64>() {
                            places.push(place);
                        }
                    }
                    let depot_line = r.str().trim();
                    let hof = if depot_line.is_empty() { None } else { Some(depot_line.to_string()) };
                    let slot = put_car_group(&mut a.groups, AiGroup { name, hof, ..Default::default() });
                    old_groups.retain(|(g, _)| *g != slot);
                    old_groups.push((slot, places));
                    depot = None;
                }
                "aigroup_2" => {
                    let name = r.str().to_string();
                    // second header line: the depot (hof) name, empty for plain car groups
                    let hof = r.str().trim().to_string();
                    // (a map that leaves the depot line out altogether starts the list right
                    // there: Novi Sad's "Trucks" group - that line is a vehicle, not a depot)
                    let lower = hof.to_ascii_lowercase();
                    let vehicle = lower.contains(".bus") || lower.contains(".ovh") || lower.contains(".sco");
                    let mut g = AiGroup { name, hof: if hof.is_empty() || vehicle { None } else { Some(hof.clone()) }, ..Default::default() };
                    if vehicle {
                        let (file, w) = match hof.split_once('\t') {
                            Some((f, w)) => (f.trim().to_string(), omsi_cfg::parse_f32(w)),
                            None => (hof.trim().to_string(), 1.0),
                        };
                        g.vehicles.push(AiVehicleEntry { file, weight: w, number: None, registration: None });
                    }
                    for l in r.until("[end]") {
                        let l = l.trim_end();
                        if l.trim().is_empty() {
                            continue;
                        }
                        let (file, w) = match l.split_once('\t') {
                            Some((f, w)) => (f.trim().to_string(), omsi_cfg::parse_f32(w)),
                            None => (l.trim().to_string(), 1.0),
                        };
                        g.vehicles.push(AiVehicleEntry { file, weight: w, number: None, registration: None });
                    }
                    a.groups.push(g);
                    depot = None;
                }
                "aigroup_depot" => {
                    let name = r.str().to_string();
                    let hof = r.str().to_string();
                    a.groups.push(AiGroup { name, hof: Some(hof), is_depot: true, ..Default::default() });
                    depot = Some(a.groups.len() - 1);
                }
                // OMSI 1 (0x7814b0): a place in the `[ailist]` list, then each vehicle's
                // number and its repaint on two lines, until `[end]`
                "aigroup_depot_typgroup" => {
                    let place = r.str().trim().parse::<i64>().unwrap_or(-1);
                    let mut tg = AiTypGroup::default();
                    let mut lines = r.until("[end]").into_iter();
                    while let Some(number) = lines.next() {
                        let paint = lines.next().unwrap_or("");
                        if number.trim().is_empty() {
                            continue;
                        }
                        tg.entries.push(DepotEntry { number: number.trim().to_string(), paint: paint.trim().to_string(), ..Default::default() });
                    }
                    if let Some((i, g)) = depot.and_then(|i| Some((i, a.groups.get_mut(i)?))) {
                        g.typgroups.push(tg);
                        old_typgroups.push((i, g.typgroups.len() - 1, place));
                    }
                }
                "aigroup_depot_typgroup_2" => {
                    let file = r.str().to_string();
                    let mut tg = AiTypGroup { file, entries: Vec::new() };
                    for l in r.until("[end]") {
                        if l.trim().is_empty() {
                            continue;
                        }
                        tg.entries.push(DepotEntry::parse(l));
                    }
                    if let Some(g) = depot.and_then(|i| a.groups.get_mut(i)) {
                        g.typgroups.push(tg);
                    }
                }
                _ => {}
            }
        }
        // OMSI 1's places in `[ailist]` as the files there (a place outside the list is
        // passed over)
        let listed = legacy.as_ref().map(|(_, f)| f.as_slice()).unwrap_or(&[]);
        let file_at = |i: i64| usize::try_from(i).ok().and_then(|i| listed.get(i)).filter(|f| !f.is_empty()).cloned();
        for (g, places) in old_groups {
            a.groups[g].vehicles = places.into_iter().filter_map(file_at).map(|file| AiVehicleEntry { file, weight: 1.0, number: None, registration: None }).collect();
        }
        for (g, t, place) in old_typgroups {
            a.groups[g].typgroups[t].file = file_at(place).unwrap_or_default();
        }
        // no default group given: every listed vehicle that is in no group makes the group
        // "NotInGroup" (weight 1), as OMSI does after reading the file
        if let Some((default, files)) = legacy {
            if default < 0 || default as usize >= a.groups.len() {
                let grouped: std::collections::HashSet<String> = a.groups.iter().flat_map(|g| g.vehicles.iter().map(|v| v.file.to_ascii_lowercase())).collect();
                let vehicles: Vec<AiVehicleEntry> = files
                    .into_iter()
                    .filter(|f| !f.is_empty() && !grouped.contains(&f.to_ascii_lowercase()))
                    .map(|file| AiVehicleEntry { file, weight: 1.0, number: None, registration: None })
                    .collect();
                if !vehicles.is_empty() {
                    a.default_group = a.groups.len();
                    a.groups.push(AiGroup { name: "NotInGroup".into(), vehicles, ..Default::default() });
                }
            } else {
                a.default_group = default as usize;
            }
        }
        a
    }

    /// The groups of a later file (a chrono's update) read on top of these: a group of a
    /// name these have (any case, the same kind - car group or depot) starts over in its
    /// place with the update's depot line and list, else it is added. Omsi.exe 0x780a58
    /// looks the name up in the groups it has and empties that group's list - a chrono's
    /// `ailists_#upd.cfg` lists a group's whole new set of cars and buses (#2315).
    pub fn update(&mut self, later: AiLists) {
        for g in later.groups {
            let same = |x: &AiGroup| x.is_depot == g.is_depot && x.name.trim().eq_ignore_ascii_case(g.name.trim());
            match self.groups.iter().position(same) {
                Some(i) => self.groups[i] = g,
                None => self.groups.push(g),
            }
        }
    }
}

/// The file a player's own edits of a map's AI lists are kept in (the launcher's "AI Lists"
/// page), in the map's folder of the content folder: the same format as `ailists_#upd.cfg`,
/// read last, on top of whatever list the date's scenario gives - a group of it replaces the
/// group of that name, so that a player's change holds on every date and "reset" is deleting
/// the file. Not an OMSI 2 file: the original never reads it.
pub const USER_LISTS_FILE: &str = "ailists_#user.cfg";

impl AiGroup {
    /// The group as the lines of an `ailists.cfg` (CR LF), which `AiLists::parse` reads back
    /// as the same group.
    pub fn to_cfg(&self) -> String {
        let clean = |s: &str| s.replace(['\r', '\n', '\t'], " ");
        let mut o = String::new();
        if self.is_depot {
            o.push_str(&format!("[aigroup_depot]\r\n{}\r\n{}\r\n", clean(&self.name), clean(self.hof.as_deref().unwrap_or(""))));
            for tg in &self.typgroups {
                o.push_str(&format!("[aigroup_depot_typgroup_2]\r\n{}\r\n", clean(&tg.file)));
                for e in &tg.entries {
                    let date = |d: Option<i32>| d.map(|d| d.to_string()).unwrap_or_default();
                    let mut line = vec![clean(&e.number), clean(&e.registration), clean(&e.paint), date(e.from), date(e.to)];
                    // (trailing empty fields are left out, as the original's files do)
                    while line.len() > 1 && line.last().is_some_and(|l| l.is_empty()) {
                        line.pop();
                    }
                    o.push_str(&line.join("\t"));
                    o.push_str("\r\n");
                }
                o.push_str("[end]\r\n");
            }
        } else {
            o.push_str(&format!("[aigroup_2]\r\n{}\r\n{}\r\n", clean(&self.name), clean(self.hof.as_deref().unwrap_or(""))));
            for v in &self.vehicles {
                o.push_str(&format!("{}\t{}\r\n", clean(&v.file), v.weight));
            }
            o.push_str("[end]\r\n");
        }
        o
    }
}

impl AiLists {
    /// The player's own groups of a map (`USER_LISTS_FILE`), none when there is no such file.
    pub fn user_edits(map_dir: &Path) -> AiLists {
        // (the file is in the content folder, the map in the OMSI folder or an archive: every
        // copy of the map's folder is looked in)
        let Some(file) = omsi_cfg::mirrored_dirs(map_dir)
            .into_iter()
            .map(|d| omsi_cfg::resolve_path(&d, USER_LISTS_FILE))
            .find(|f| omsi_cfg::vfs::is_file(f))
        else {
            return AiLists::default();
        };
        AiLists::load(&file).unwrap_or_else(|e| {
            log::warn!("AI lists: {} cannot be read ({e}), it is left out", file.display());
            AiLists::default()
        })
    }
}

/// A plain list file (one entry per line, blank lines ignored).
pub fn load_list(path: &Path) -> Vec<String> {
    match CfgFile::read(path) {
        Ok(f) => f.lines.iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
        Err(_) => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnschedGroup {
    pub name: String,
    pub factor: f32,
    /// (day-of-week mask, list of (hour, density))
    pub densities: Vec<(i32, Vec<(f32, f32)>)>,
}

/// `unsched_trafficdens.txt`
pub fn parse_unsched_trafficdens(f: &CfgFile) -> Vec<UnschedGroup> {
    let mut out: Vec<UnschedGroup> = Vec::new();
    let mut r = f.reader();
    while let Some(k) = r.next_keyword() {
        match k.as_str() {
            "group" => {
                let name = r.str().to_string();
                let factor = r.f32();
                out.push(UnschedGroup { name, factor, densities: Vec::new() });
            }
            "set_day_of_week" => {
                let d = r.i32();
                if let Some(g) = out.last_mut() {
                    g.densities.push((d, Vec::new()));
                }
            }
            "trafficdensity" => {
                let t = r.f32();
                let d = r.f32();
                if let Some(g) = out.last_mut() {
                    if g.densities.is_empty() {
                        g.densities.push((0, Vec::new()));
                    }
                    g.densities.last_mut().unwrap().1.push((t, d));
                }
            }
            _ => {}
        }
    }
    out
}

/// `unsched_vehgroups.txt`: (group name, default density class)
pub fn parse_unsched_vehgroups(f: &CfgFile) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    let mut r = f.reader();
    while let Some(k) = r.next_keyword() {
        if k == "group" {
            let n = r.str().to_string();
            let d = r.i32();
            out.push((n, d));
        }
    }
    out
}

/// Chrono `Chrono.cfg`
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChronoCfg {
    pub start_date: i32,
    pub end_date: i32,
    pub ticket_pack: Option<String>,
    pub money_system: Option<String>,
    pub deactivate_lines: Vec<String>,
}

pub fn parse_chrono_cfg(f: &CfgFile) -> ChronoCfg {
    let mut c = ChronoCfg::default();
    let mut r = f.reader();
    while let Some(k) = r.next_keyword() {
        match k.as_str() {
            "startdate" => c.start_date = r.i32(),
            "enddate" => c.end_date = r.i32(),
            "ticketpack" => c.ticket_pack = Some(r.str().to_string()),
            "moneysystem" => c.money_system = Some(r.str().to_string()),
            // a count, then that many line names (the original: StrToInt, then ReadLn
            // n times). The count is not a line: read as one it took line "1" or "2" off.
            "deactivate_lines" => {
                let n = (r.i32().max(0) as usize).min(r.lines().len().saturating_sub(r.pos()));
                c.deactivate_lines = (0..n).map(|_| r.str().trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
            _ => {}
        }
    }
    c
}

impl ChronoCfg {
    /// Is the scenario in force on `date` (YYYYMMDD)? As the original: a scenario
    /// without any date never is; `[startdate]` and `[enddate]` both count as in force.
    pub fn active_on(&self, date: i32) -> bool {
        (self.start_date != 0 || self.end_date != 0) && (self.start_date == 0 || date >= self.start_date) && (self.end_date == 0 || date <= self.end_date)
    }
}

/// The `Chrono.cfg` files under a map's `Chrono` folder in the game's order (OMSI
/// the original): depth first, a folder's subfolders before its own files, names in the
/// order Windows lists them (case-insensitive). A later scenario overrides an earlier one.
fn chrono_cfgs(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<(String, std::ffi::OsString, bool, PathBuf)> = Vec::new();
    for d in omsi_cfg::mirrored_dirs(dir) {
        for (n, is_dir) in omsi_cfg::vfs::list_dir(&d).unwrap_or_default() {
            let key = n.to_string_lossy().to_uppercase();
            if !entries.iter().any(|e| e.0 == key) {
                entries.push((key, n, is_dir, d.clone()));
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, n, is_dir, _) in &entries {
        if *is_dir {
            chrono_cfgs(&dir.join(n), out);
        }
    }
    for (_, n, is_dir, d) in &entries {
        if !*is_dir && n.to_string_lossy().eq_ignore_ascii_case("Chrono.cfg") {
            out.push(d.join(n));
        }
    }
}

/// Chrono folders of a map that are active on `date` (YYYYMMDD), in the game's order.
pub fn active_chrono_dirs(map_dir: &Path, date: i32) -> Vec<PathBuf> {
    let mut cfgs = Vec::new();
    chrono_cfgs(&omsi_cfg::resolve_path(map_dir, "Chrono"), &mut cfgs);
    cfgs.into_iter()
        .filter(|p| CfgFile::read(p).map(|f| parse_chrono_cfg(&f).active_on(date)).unwrap_or(false))
        .filter_map(|p| p.parent().map(|d| d.to_path_buf()))
        .collect()
}

/// The lines the given (active) chrono folders take off the timetable (`[deactivate_lines]`),
/// each with the folder that does it, in folder order. A scenario takes a line off only for
/// the folders before it (the map's own TTData and earlier scenarios): a later one may bring
/// the line back (see `TimetableData::load_with_chrono`).
pub fn chrono_deactivated_lines(chrono_dirs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for d in chrono_dirs {
        let cfg = Some(omsi_cfg::resolve_path(d, "Chrono.cfg")).filter(|p| omsi_cfg::vfs::is_file(p));
        if let Some(f) = cfg.and_then(|p| CfgFile::read(&p).ok()) {
            out.extend(parse_chrono_cfg(&f).deactivate_lines.into_iter().map(|l| (l, d.clone())));
        }
    }
    out
}

/// The map's `ailists.cfg` with the updates of the given (active) chrono folders. As
/// Omsi.exe 0x785f98 (0x78a1df..0x78a510): from the newest scenario back to the map, a
/// folder's own `ailists.cfg` is a whole new list that ends the search, else its
/// `ailists_#upd.cfg` is an update; the files are then read oldest first (0x780a58), each
/// group of an update replacing the group of that name. A scenario with a full list no
/// longer mixes in the map's old cars (#2315); Berlin's buses change to "Spandau 1994" on
/// 29 May 1994.
pub fn ailists_with_chrono(map_dir: &Path, chrono_dirs: &[PathBuf]) -> AiLists {
    let mut ailists = ailists_with_chrono_base(map_dir, chrono_dirs);
    ailists.update(AiLists::user_edits(map_dir));
    ailists
}

/// [`ailists_with_chrono`] without the player's own edits: what the map and its scenarios
/// give, the "defaults" the launcher's AI Lists page resets to.
pub fn ailists_with_chrono_base(map_dir: &Path, chrono_dirs: &[PathBuf]) -> AiLists {
    let mut files = Vec::new();
    for dir in chrono_dirs.iter().rev().map(|d| d.as_path()).chain(std::iter::once(map_dir)) {
        let full = omsi_cfg::resolve_path(dir, "ailists.cfg");
        if omsi_cfg::vfs::is_file(&full) {
            files.push(full);
            break;
        }
        let update = omsi_cfg::resolve_path(dir, "ailists_#upd.cfg");
        if omsi_cfg::vfs::is_file(&update) {
            files.push(update);
        }
    }
    let mut files = files.into_iter().rev();
    let map_list = omsi_cfg::resolve_path(map_dir, "ailists.cfg");
    let mut ailists = match files.next() {
        Some(base) => AiLists::load(&base).unwrap_or_else(|e| {
            // (a scenario's list that cannot be read left the map without any traffic)
            log::warn!("AI lists: {} cannot be read ({e}), the map's own list is used", base.display());
            if base != map_list { AiLists::load(&map_list).unwrap_or_default() } else { AiLists::default() }
        }),
        None => AiLists::default(),
    };
    for f in files {
        match AiLists::load(&f) {
            Ok(l) => ailists.update(l),
            Err(e) => log::warn!("AI lists: the update {} cannot be read ({e})", f.display()),
        }
    }
    ailists
}

/// The depot file (`.hof` name) the map's own buses use on `date` (YYYYMMDD): the first
/// depot's, with the chrono scenarios of that date.
pub fn depot_hof_on(map_dir: &Path, date: i32) -> Option<String> {
    let l = ailists_with_chrono(map_dir, &active_chrono_dirs(map_dir, date));
    l.groups.iter().filter(|g| g.is_depot).chain(l.groups.iter()).find_map(|g| g.hof.clone())
}

/// A date as the game and the launcher write it (`YYYY-MM-DD`) as the chrono's `YYYYMMDD`.
pub fn date_code(date: &str) -> Option<i32> {
    let v: Vec<i32> = date.split('-').filter_map(|x| x.trim().parse().ok()).collect();
    match v[..] {
        [y, m, d] if (1..=12).contains(&m) && (1..=31).contains(&d) => Some(y * 10000 + m * 100 + d),
        _ => None,
    }
}

/// Whether a depot vehicle is in service on `date` (YYYYMMDD): from its first day, if it has
/// one, to its last, if it has one (0x781a15, 0x781a47).
pub fn typgroup_entry_valid(entry: &DepotEntry, date: i32) -> bool {
    entry.from.is_none_or(|f| f <= date) && entry.to.is_none_or(|t| t >= date)
}

/// `signalroutes.cfg` (unit `mc_fahrstrasse`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignalRoute {
    pub kind: i32,
    pub signal: (i64, i32),
    pub entries: Vec<[i64; 4]>,
    pub dist_signal: Option<i64>,
    pub next_signal: Option<i64>,
    pub speed_limit: Option<f32>,
}

pub fn parse_signalroutes(f: &CfgFile) -> Vec<SignalRoute> {
    let mut out: Vec<SignalRoute> = Vec::new();
    let mut r = f.reader();
    while let Some(k) = r.next_keyword() {
        match k.as_str() {
            "signalroute" => out.push(SignalRoute { kind: r.i32(), ..Default::default() }),
            "signal" => {
                let a = r.i64();
                let b = r.i32();
                if let Some(s) = out.last_mut() {
                    s.signal = (a, b);
                }
            }
            "entry" => {
                let e = [r.i64(), r.i64(), r.i64(), r.i64()];
                if let Some(s) = out.last_mut() {
                    s.entries.push(e);
                }
            }
            "distsignal" => {
                let v = r.i64();
                if let Some(s) = out.last_mut() {
                    s.dist_signal = Some(v);
                }
            }
            "nextsignal" => {
                let v = r.i64();
                if let Some(s) = out.last_mut() {
                    s.next_signal = Some(v);
                }
            }
            "speedlimit" => {
                let v = r.f32();
                if let Some(s) = out.last_mut() {
                    s.speed_limit = Some(v);
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    include!("../../../tools/test-support/original_root.rs");
    use super::*;

    #[test]
    fn chrono_folders_of_every_content_root_are_listed() {
        let tmp = std::env::temp_dir().join(format!("omsi-chrono-roots-{}", std::process::id()));
        let (overlay, base) = (tmp.join("openomsi"), tmp.join("omsi2"));
        for (root, folder) in [(&base, "Base"), (&base, "Shared"), (&overlay, "Mod"), (&overlay, "Shared")] {
            let d = root.join("maps/ChronoRoots/Chrono").join(folder);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("Chrono.cfg"), "").unwrap();
        }
        omsi_cfg::add_content_root(overlay.clone());
        omsi_cfg::add_content_root(base.clone());
        let mut cfgs = Vec::new();
        chrono_cfgs(&omsi_cfg::resolve_path(&base.join("maps/ChronoRoots"), "Chrono"), &mut cfgs);
        omsi_cfg::remove_content_root(&overlay);
        omsi_cfg::remove_content_root(&base);
        let _ = std::fs::remove_dir_all(&tmp);
        let chrono = |root: &Path, folder: &str| root.join("maps/ChronoRoots/Chrono").join(folder).join("Chrono.cfg");
        assert_eq!(cfgs, [chrono(&base, "Base"), chrono(&overlay, "Mod"), chrono(&overlay, "Shared")]);
    }

    #[test]
    fn date_codes() {
        assert_eq!(date_code("2026-09-17"), Some(20260917));
        assert_eq!(date_code("1989-5-30"), Some(19890530));
        assert_eq!(date_code("1989-13-30"), None);
        assert_eq!(date_code(""), None);
    }

    /// Spandau's 1991 timetable change takes line "5 & 5N" off: on any later date the chrono
    /// that does it is active and names the line; before it, nothing takes the line off.
    #[test]
    #[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
    fn spandau_takes_line_5_off_in_1991() {
        let root = original_root();
        let map = root.join("maps/Berlin-Spandau");
        require_content(&[&map.join("Chrono")]);
        let later = chrono_deactivated_lines(&active_chrono_dirs(&map, 20260917));
        let by = later.iter().find(|(l, _)| l == "5 & 5N").map(|(_, d)| d.file_name().unwrap().to_string_lossy().into_owned());
        assert_eq!(by.as_deref(), Some("1000_FPW_19910602"), "{later:?}");
        let before = chrono_deactivated_lines(&active_chrono_dirs(&map, 19890530));
        assert!(!before.iter().any(|(l, _)| l == "5 & 5N"), "{before:?}");
        // the count before the names is no line
        assert!(!later.iter().any(|(l, _)| l == "1" || l == "17" || l == "2"), "{later:?}");
    }

    #[test]
    fn chrono_dates_are_inclusive_and_need_one() {
        let c = |s, e| ChronoCfg { start_date: s, end_date: e, ..Default::default() };
        assert!(!c(0, 0).active_on(19900101));
        assert!(c(19900910, 19900923).active_on(19900923));
        assert!(!c(19900910, 19900923).active_on(19900924));
        assert!(c(0, 19870430).active_on(19800101));
        assert!(c(19870501, 0).active_on(20260101));
    }
}

#[cfg(test)]
mod legacy_tests {
    #[test]
    fn the_old_ailist_makes_a_group_of_its_own() {
        let f = omsi_cfg::CfgFile::from_str("ailists.cfg", "[ailist]\n0\n-1\n2\nvehicles\\A\\a.bus\nvehicles\\B\\b.ovh\n");
        let a = super::AiLists::parse(&f);
        assert_eq!(a.groups.len(), 1);
        assert_eq!(a.groups[0].name, "NotInGroup");
        assert_eq!(a.groups[0].vehicles.len(), 2);
        assert_eq!(a.default_group, 0);
    }

    #[test]
    fn the_default_group_is_the_first_unless_the_header_names_one() {
        let groups = "[aigroup_2]\nNormalCars\n\nvehicles\\A\\a.bus\t7\n[end]\n[aigroup_2]\nAmbulance\n\nvehicles\\B\\b.ovh\n[end]\n";
        let a = super::AiLists::parse(&omsi_cfg::CfgFile::from_str("ailists.cfg", groups));
        assert_eq!(a.default_group, 0);
        let named = format!("[ailist]\n0\n1\n0\n{groups}");
        let a = super::AiLists::parse(&omsi_cfg::CfgFile::from_str("ailists.cfg", &named));
        assert_eq!(a.groups[a.default_group].name, "Ambulance");
    }

    /// An OMSI 1 list made up for the test: places in `[ailist]` (an empty line counts as
    /// a place), a car group read twice, a depot with an old type group, a new group after
    /// them, and the cars no group takes.
    #[test]
    fn old_groups_point_into_the_vehicle_list() {
        let list = [
            "[ailist]", "", "-1", "6",
            "Vehicles\\Sedan_A\\sedan.ovh", "", "Vehicles\\Truck_7t\\truck.ovh",
            "Vehicles\\Bus_SL\\sl.bus", "Vehicles\\Bicycle\\bike.ovh", "Vehicles\\Pickup\\pickup.ovh",
            "[aigroup]", "Innenstadt", "3", "0", "4", "9", "",
            "[aigroup_depot]", "Nordbus", "Betriebshof Nord",
            "[aigroup_depot_typgroup]", "3", "4711", "weiss", "4712", "", "[end]",
            "[aigroup]", "Umland", "1", "2", "Betriebshof Nord",
            "[aigroup]", "INNENSTADT ", "1", "5", "",
            "[aigroup_2]", "Lieferwagen", "", "Vehicles\\Van\\van.ovh\t2", "[end]",
        ]
        .join("\n");
        let lists = super::AiLists::parse(&omsi_cfg::CfgFile::from_str("ailists.cfg", &list));
        let by_name = |n: &str| lists.groups.iter().find(|g| g.name.trim().eq_ignore_ascii_case(n)).unwrap();
        let cars = |n: &str| by_name(n).vehicles.iter().map(|v| v.file.rsplit('\\').next().unwrap().to_string()).collect::<Vec<_>>();
        // read again, "Innenstadt" starts over where it was: only the pickup (place 5)
        assert_eq!(lists.groups[0].name, "INNENSTADT");
        assert_eq!(cars("innenstadt"), ["pickup.ovh"]);
        assert!(by_name("innenstadt").vehicles.iter().all(|v| v.weight == 1.0));
        assert_eq!(cars("umland"), ["truck.ovh"]);
        assert_eq!(by_name("umland").hof.as_deref(), Some("Betriebshof Nord"));
        let depot = by_name("nordbus");
        assert!(depot.is_depot);
        let fleet = &depot.typgroups[..];
        assert_eq!(fleet.len(), 1);
        assert!(fleet[0].file.ends_with("sl.bus"), "{}", fleet[0].file);
        let pairs: Vec<(&str, &str)> = fleet[0].entries.iter().map(|e| (&e.number[..], &e.paint[..])).collect();
        assert_eq!(pairs, [("4711", "weiss"), ("4712", "")]);
        assert_eq!(by_name("lieferwagen").vehicles[0].weight, 2.0);
        // the sedan and the bicycle were "Innenstadt"'s first time only; the bus is a
        // depot's: all three are left over
        let rest = by_name("NotInGroup");
        assert_eq!(lists.groups[lists.default_group].name, rest.name);
        assert_eq!(cars("NotInGroup"), ["sedan.ovh", "sl.bus", "bike.ovh"]);
    }

    #[test]
    fn a_chrono_count_larger_than_the_file_reads_what_is_there() {
        let f = omsi_cfg::CfgFile::from_str("chrono.cfg", "[deactivate_lines]\n2147483647\n5 & 5N\n");
        assert_eq!(super::parse_chrono_cfg(&f).deactivate_lines, vec!["5 & 5N".to_string()]);
    }
}

#[cfg(test)]
mod chrono_hof_tests {
    include!("../../../tools/test-support/original_root.rs");

    #[test]
    #[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
    fn berlin_changes_its_depot_file_with_the_date() {
        let dir = &original_root().join("maps/Berlin-Spandau");
        require_content(&[dir]);
        let at = |d| super::depot_hof_on(dir, d).unwrap_or_default();
        assert_eq!(at(19940601), "Spandau 1994");
        assert_ne!(at(19870101), "Spandau 1994");
        assert_eq!(at(19891201), "Spandau 1989-12");
    }
}

#[cfg(test)]
mod chrono_list_tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omsi-map-chrono-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const MAP: &str = "[aigroup_2]\nNormalCars\n\nold\\golf.bus\t7\nold\\bx.ovh\t4\n[end]\n[aigroup_2]\nTrucks\n\nold\\f90.bus\n[end]\n\
        [aigroup_depot]\nBusses\nGrundorf\n[aigroup_depot_typgroup_2]\nold\\sd77.bus\n2701\n[end]\n";

    /// An update lists a group's whole new set: the group of that name starts over with it, in
    /// its place, and the groups it does not name stay.
    #[test]
    fn an_update_replaces_the_groups_it_names() {
        let mut a = AiLists::parse(&CfgFile::from_str("ailists.cfg", MAP));
        let upd = "[aigroup_2]\nnormalcars\n\nnew\\golf.bus\t3\n[end]\n[aigroup_depot]\nBusses\nGrundorf 2002\n[aigroup_depot_typgroup_2]\nnew\\c2.bus\n2530\n[end]\n";
        a.update(AiLists::parse(&CfgFile::from_str("ailists_#upd.cfg", upd)));
        assert_eq!(a.groups.len(), 3);
        assert_eq!(a.groups[0].vehicles.iter().map(|v| v.file.as_str()).collect::<Vec<_>>(), ["new\\golf.bus"]);
        assert_eq!(a.groups[1].name, "Trucks");
        assert_eq!(a.groups[2].hof.as_deref(), Some("Grundorf 2002"));
        assert_eq!(a.groups[2].typgroups.len(), 1);
        assert_eq!(a.groups[2].typgroups[0].file, "new\\c2.bus");
    }

    /// Within one file two groups of a name stay two groups, and across files a car group
    /// never replaces a depot of its name (nor a depot a car group).
    #[test]
    fn groups_replace_only_across_files_and_of_their_kind() {
        let twice = "[aigroup_2]\nCars\n\na.bus\n[end]\n[aigroup_2]\nCars\n\nb.bus\n[end]\n";
        assert_eq!(AiLists::parse(&CfgFile::from_str("ailists.cfg", twice)).groups.len(), 2);
        let mut a = AiLists::parse(&CfgFile::from_str("ailists.cfg", MAP));
        a.update(AiLists::parse(&CfgFile::from_str("ailists_#upd.cfg", "[aigroup_2]\nBusses\n\nnew\\car.bus\n[end]\n")));
        assert_eq!(a.groups.len(), 4);
        assert!(a.groups[2].is_depot && a.groups[2].typgroups.len() == 1);
        assert!(!a.groups[3].is_depot && a.groups[3].name == "Busses");
    }

    /// A type group after a car group belongs to no depot (the original logs it and skips it).
    #[test]
    fn a_type_group_needs_a_depot_before_it() {
        let f = "[aigroup_depot]\nBusses\nGrundorf\n[aigroup_2]\nRegionalbahn\n\n[aigroup_depot_typgroup_2]\ntrains\\628.zug\n[end]\n";
        let a = AiLists::parse(&CfgFile::from_str("ailists.cfg", f));
        assert!(a.groups.iter().all(|g| g.typgroups.is_empty()), "{a:?}");
    }

    /// A scenario's own `ailists.cfg` is the whole list on its dates: none of the map's cars
    /// stay mixed in, and a later scenario's update still goes on top (#2315).
    #[test]
    fn a_scenario_with_a_full_list_replaces_the_map_list() {
        let map = dir("map");
        std::fs::write(map.join("ailists.cfg"), MAP).unwrap();
        let euro = map.join("Chrono").join("20020101_EURO");
        std::fs::create_dir_all(&euro).unwrap();
        std::fs::write(euro.join("ailists.cfg"), "[aigroup_2]\nNormalCars\n\nnew\\kipkw1.bus\t25\n[end]\n").unwrap();
        let later = map.join("Chrono").join("20050101");
        std::fs::create_dir_all(&later).unwrap();
        std::fs::write(later.join("ailists_#upd.cfg"), "[aigroup_2]\nTaxis\n\nnew\\taxi.bus\n[end]\n").unwrap();
        let a = ailists_with_chrono(&map, &[euro.clone(), later.clone()]);
        assert_eq!(a.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["NormalCars", "Taxis"]);
        assert_eq!(a.groups[0].vehicles.len(), 1);
        // only the update: the map's list with the update's group added
        let b = ailists_with_chrono(&map, &[later]);
        assert_eq!(b.groups.len(), 4);
        let _ = std::fs::remove_dir_all(&map);
    }
}

#[cfg(test)]
mod user_edit_tests {
    use super::*;

    const MAP: &str = "[aigroup_2]\nNormalCars\n\nold\\golf.bus\t7\nold\\bx.ovh\t4\n[end]\n[aigroup_2]\nTrucks\n\nold\\f90.bus\n[end]\n\
        [aigroup_depot]\nBusses\nGrundorf\n[aigroup_depot_typgroup_2]\nold\\sd77.bus\n2701\tXX-1\tred\t19900101\t19991231\n2702\n[end]\n";

    /// What the page writes comes back from the reader as the same groups.
    #[test]
    fn a_group_written_is_read_back() {
        let a = AiLists::parse(&CfgFile::from_str("ailists.cfg", MAP));
        let text: String = a.groups.iter().map(AiGroup::to_cfg).collect();
        let b = AiLists::parse(&CfgFile::from_str(USER_LISTS_FILE, &text));
        assert_eq!(a.groups, b.groups);
    }

    /// The player's file is read last: its groups replace the map's by name on any date, the
    /// groups it does not name stay, and without the file the list is the map's own.
    #[test]
    fn the_players_file_goes_on_top() {
        let map = std::env::temp_dir().join(format!("omsi-map-user-lists-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&map);
        std::fs::create_dir_all(&map).unwrap();
        std::fs::write(map.join("ailists.cfg"), MAP).unwrap();
        assert_eq!(ailists_with_chrono(&map, &[]), ailists_with_chrono_base(&map, &[]));
        let mine = AiGroup { name: "normalcars".into(), vehicles: vec![AiVehicleEntry { file: "new\\kipkw.bus".into(), weight: 25.0, ..Default::default() }], ..Default::default() };
        std::fs::write(map.join(USER_LISTS_FILE), mine.to_cfg()).unwrap();
        let on_top = ailists_with_chrono(&map, &[]);
        assert_eq!(on_top.groups.len(), 3);
        assert_eq!(on_top.groups[0].vehicles.iter().map(|v| (v.file.as_str(), v.weight)).collect::<Vec<_>>(), [("new\\kipkw.bus", 25.0)]);
        assert_eq!(on_top.groups[1].name, "Trucks");
        // the defaults are still the map's
        assert_eq!(ailists_with_chrono_base(&map, &[]).groups[0].vehicles.len(), 2);
        let _ = std::fs::remove_dir_all(&map);
    }
}
