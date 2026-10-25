//! The launcher's AI Lists page: which AI cars, lorries and buses a map's traffic and depots
//! are made of, changed by the player from the library of every AI vehicle installed, and
//! which buses run on which line.
//!
//! The changes are kept as the files OMSI itself reads, in the map's folder of the content
//! folder (the map's own files stay as they are): the groups the player changed as
//! `ailists_#user.cfg` (see `omsi_map::ailists::USER_LISTS_FILE`) and a line's rule as a
//! `car_use` record (`[onlytypes]`, or `[types_prefered]` with its probability). Resetting is
//! deleting them.

use super::*;
use omsi_map::ailists::{AiGroup, AiLists, AiTypGroup, AiVehicleEntry, DepotEntry, USER_LISTS_FILE};
use omsi_timetable::CarUse;

/// The names of the rule files made here; the `!` sorts them before the map's own records,
/// and the first record to give a tour its bus keeps it.
const RULE_PREFIX: &str = "!openomsi-ai-lists-";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct VehicleJson {
    pub file: String,
    pub weight: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EntryJson {
    pub number: String,
    #[serde(default)]
    pub registration: String,
    #[serde(default)]
    pub paint: String,
    #[serde(default)]
    pub from: Option<i32>,
    #[serde(default)]
    pub to: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TypGroupJson {
    pub file: String,
    pub entries: Vec<EntryJson>,
}

/// One AI group as the page edits it: a car group (a weighted list of vehicle files) or a
/// depot (types, each with the fleet numbers and repaints of its buses).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GroupJson {
    pub name: String,
    #[serde(default)]
    pub depot: bool,
    #[serde(default)]
    pub hof: Option<String>,
    #[serde(default)]
    pub vehicles: Vec<VehicleJson>,
    #[serde(default)]
    pub typgroups: Vec<TypGroupJson>,
}

impl From<&AiGroup> for GroupJson {
    fn from(g: &AiGroup) -> GroupJson {
        GroupJson {
            name: g.name.clone(),
            depot: g.is_depot,
            hof: g.hof.clone(),
            vehicles: g.vehicles.iter().map(|v| VehicleJson { file: v.file.clone(), weight: v.weight }).collect(),
            typgroups: g
                .typgroups
                .iter()
                .map(|t| TypGroupJson {
                    file: t.file.clone(),
                    entries: t.entries.iter().map(|e| EntryJson { number: e.number.clone(), registration: e.registration.clone(), paint: e.paint.clone(), from: e.from, to: e.to }).collect(),
                })
                .collect(),
        }
    }
}

impl GroupJson {
    fn to_group(&self) -> AiGroup {
        AiGroup {
            name: self.name.clone(),
            vehicles: self.vehicles.iter().map(|v| AiVehicleEntry { file: v.file.clone(), weight: v.weight.max(0.0), number: None, registration: None }).collect(),
            hof: self.hof.clone().filter(|h| !h.trim().is_empty()),
            is_depot: self.depot,
            typgroups: self
                .typgroups
                .iter()
                .map(|t| AiTypGroup {
                    file: t.file.clone(),
                    entries: t.entries.iter().map(|e| DepotEntry { number: e.number.clone(), registration: e.registration.clone(), paint: e.paint.clone(), from: e.from, to: e.to }).collect(),
                })
                .collect(),
        }
    }
}

/// What a line's buses are limited to: `only` the types, or `prefer` them with a
/// probability (the other tours drawing from the whole depot).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RuleJson {
    pub line: String,
    /// `only` or `prefer`
    pub mode: String,
    #[serde(default = "one")]
    pub factor: f32,
    pub types: Vec<String>,
}

fn one() -> f32 {
    1.0
}

/// A vehicle of the library: any `.bus` or `.ovh` of the content (rail vehicles are not
/// street traffic and left out).
#[derive(Serialize, Clone, Debug)]
pub struct AiVehicle {
    /// As the lists name it: `Vehicles\Folder\file.bus`
    pub file: String,
    pub name: String,
    pub folder: String,
    /// `bus` (one the player can drive), `ai` (made for the traffic: cars, lorries, buses) or
    /// `aircraft`
    pub kind: String,
}

struct Place {
    content: PathBuf,
    map_dir: PathBuf,
    rel: PathBuf,
}

fn place(map: &str) -> Result<Place> {
    let global = resolve_content(map)?;
    let map_dir = global.parent().map(Path::to_path_buf).with_context(|| format!("{map} is not a map file"))?;
    let rel = PathBuf::from(map.replace('\\', "/")).parent().map(Path::to_path_buf).unwrap_or_default();
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
        return Err(anyhow!("{map} is not a map file"));
    }
    let content = content_dir().context("no content folder")?;
    Ok(Place { content, map_dir, rel })
}

impl Place {
    fn lists_file(&self) -> PathBuf {
        self.content.join(&self.rel).join(USER_LISTS_FILE)
    }
    fn rules_dir(&self) -> PathBuf {
        self.content.join(&self.rel).join("car_use")
    }
}

fn files_of(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ocu")) && p.file_name().is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().starts_with(prefix)))
        .collect();
    v.sort();
    v
}

/// Every `.bus` and `.ovh` under the content's `Vehicles` folders, read once and kept until
/// `refresh`.
pub fn vehicles(refresh: bool) -> Result<Vec<AiVehicle>> {
    use rayon::prelude::*;
    static KEPT: std::sync::Mutex<Option<Vec<AiVehicle>>> = std::sync::Mutex::new(None);
    if !refresh {
        if let Some(k) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return Ok(k.clone());
        }
    }
    root()?;
    fn walk(d: &Path, rel: &str, depth: u32, out: &mut Vec<(String, PathBuf)>) {
        let mut list = omsi_cfg::vfs::list_dir(d).unwrap_or_default();
        list.sort();
        for (n, is_dir) in list {
            let name = n.to_string_lossy().to_string();
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}\\{name}") };
            if is_dir {
                if depth > 0 && !name.starts_with('.') {
                    walk(&d.join(&n), &r, depth - 1, out);
                }
            } else if Path::new(&name).extension().is_some_and(|e| e.eq_ignore_ascii_case("bus") || e.eq_ignore_ascii_case("ovh")) {
                out.push((r, d.join(&n)));
            }
        }
    }
    let folders = merged_folders("Vehicles");
    let mut lists: Vec<Vec<AiVehicle>> = folders
        .par_iter()
        .map(|(folder, dirs)| {
            let mut files: Vec<(String, PathBuf)> = Vec::new();
            for d in dirs {
                walk(d, "", 8, &mut files);
            }
            // (a file of the content folder hides the one at the same place in the installation)
            let mut seen = std::collections::HashSet::new();
            files.retain(|(r, _)| seen.insert(r.to_ascii_lowercase()));
            files
                .into_iter()
                .filter_map(|(r, f)| {
                    let v = omsi_vehicle::Vehicle::load(&f).ok()?;
                    if v.is_rail() {
                        return None;
                    }
                    let air = matches!(v.kind, omsi_vehicle::vehicle::VehicleKind::Other(3));
                    let named = format!("{} {}", v.manufacturer.trim(), v.type_name.trim());
                    let stem = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let name = if named.trim().is_empty() { display_bus_name(&stem) } else { display_bus_name(&named) };
                    // (what OMSI offers the player to drive has a `[friendlyname]`; the rest is the
                    // AI's own: cars, lorries and the buses made for the traffic)
                    let kind = if air {
                        "aircraft"
                    } else if v.is_selectable() {
                        "bus"
                    } else {
                        "ai"
                    };
                    Some(AiVehicle { file: format!("Vehicles\\{folder}\\{r}"), name, folder: folder.clone(), kind: kind.into() })
                })
                .collect()
        })
        .collect();
    let mut out: Vec<AiVehicle> = lists.drain(..).flatten().collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.file.cmp(&b.file)));
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some(out.clone());
    Ok(out)
}

/// The repaint names of one vehicle of the library (read on demand: it opens the model).
pub fn paints(file: &str) -> Result<Vec<String>> {
    let path = resolve_content(&file.replace('\\', "/"))?;
    let v = omsi_vehicle::Vehicle::load(&path).map_err(|e| anyhow!("{file}: {e}"))?;
    Ok(paint_schemes(&v).0)
}

fn rule_of(c: &CarUse) -> Option<RuleJson> {
    let (mode, factor, types) = if !c.only_types.is_empty() {
        ("only", 1.0, c.only_types.clone())
    } else {
        let (f, l) = c.types_prefered.clone()?;
        ("prefer", f, l)
    };
    Some(RuleJson { line: c.line.trim().to_string(), mode: mode.into(), factor, types })
}

/// Everything the page shows of one map on `date`.
pub fn view(map: &str, date: &str) -> Result<Value> {
    let p = place(map)?;
    let date = if date.trim().is_empty() { DEFAULT_DATE } else { date.trim() };
    let code = omsi_map::date_code(date).with_context(|| i18n::trf("'{date}' is not a date (YYYY-MM-DD)", &[("date", &date)]))?;
    let chrono = omsi_map::active_chrono_dirs(&p.map_dir, code);
    let base = omsi_map::ailists::ailists_with_chrono_base(&p.map_dir, &chrono);
    let user = AiLists::user_edits(&p.map_dir);
    let mut effective = base.clone();
    effective.update(user.clone());
    // the groups the random traffic is made of: those `unsched_vehgroups.txt` names, else
    // the default group alone (as the game reads them)
    let unscheduled: Option<Vec<String>> = omsi_cfg::CfgFile::read(&omsi_cfg::resolve_path(&p.map_dir, "unsched_vehgroups.txt"))
        .ok()
        .map(|f| omsi_map::ailists::parse_unsched_vehgroups(&f).into_iter().map(|(n, _)| n.trim().to_ascii_lowercase()).collect());
    let same = |a: &AiGroup, b: &AiGroup| a.is_depot == b.is_depot && a.name.trim().eq_ignore_ascii_case(b.name.trim());
    let groups: Vec<Value> = effective
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let random = !g.is_depot
                && g.hof.is_none()
                && match &unscheduled {
                    Some(names) => names.contains(&g.name.trim().to_ascii_lowercase()),
                    None => i == effective.default_group,
                };
            let mut j = serde_json::to_value(GroupJson::from(g)).unwrap_or(Value::Null);
            j["random"] = json!(random);
            j["modified"] = json!(user.groups.iter().any(|u| same(u, g)));
            j
        })
        .collect();
    let defaults: Vec<GroupJson> = base.groups.iter().map(GroupJson::from).collect();
    // the lines and the groups of their tours
    let off = omsi_map::chrono_deactivated_lines(&chrono);
    let data = omsi_timetable::TimetableData::load_with_chrono(&p.map_dir, &chrono, &off);
    let ours: Vec<RuleJson> = files_of(&p.rules_dir(), RULE_PREFIX).iter().filter_map(|f| CarUse::load(f).ok()).filter_map(|c| rule_of(&c)).collect();
    let all_rules = CarUse::load_dir(&p.map_dir);
    let lines: Vec<Value> = data
        .lines
        .iter()
        .filter(|l| !l.tours.is_empty())
        .map(|l| {
            let mut groups: Vec<String> = Vec::new();
            for t in &l.tours {
                if !t.ai_group.trim().is_empty() && !groups.iter().any(|g| g.eq_ignore_ascii_case(t.ai_group.trim())) {
                    groups.push(t.ai_group.trim().to_string());
                }
            }
            let line = l.name.trim();
            let mine = ours.iter().find(|r| r.line.eq_ignore_ascii_case(line));
            let total = all_rules.iter().filter(|c| c.line.trim().eq_ignore_ascii_case(line)).count();
            let own = ours.iter().filter(|r| r.line.eq_ignore_ascii_case(line)).count();
            json!({ "name": line, "tours": l.tours.len(), "groups": groups, "rule": mine, "map_rule": total > own })
        })
        .collect();
    Ok(json!({
        "map": map,
        "date": date,
        "groups": groups,
        "defaults": defaults,
        "default_group": effective.default_group,
        "lines": lines,
        "edited": !user.groups.is_empty() || !ours.is_empty(),
    }))
}

/// Keep the player's version of the AI lists of `map`: of the groups given, the ones that
/// differ from what the map gives on `date` are written; the lines' rules replace the old
/// ones. Nothing differing and no rule: the player's files are gone, the map as it was.
pub fn save(map: &str, date: &str, groups: &[GroupJson], rules: &[RuleJson]) -> Result<Value> {
    let p = place(map)?;
    let date = if date.trim().is_empty() { DEFAULT_DATE } else { date.trim() };
    let code = omsi_map::date_code(date).with_context(|| i18n::trf("'{date}' is not a date (YYYY-MM-DD)", &[("date", &date)]))?;
    let base = omsi_map::ailists::ailists_with_chrono_base(&p.map_dir, &omsi_map::active_chrono_dirs(&p.map_dir, code));
    let mut text = String::new();
    let mut changed = 0;
    for g in groups {
        let own = g.to_group();
        let same_as_map = base.groups.iter().find(|b| b.is_depot == own.is_depot && b.name.trim().eq_ignore_ascii_case(own.name.trim())).map(GroupJson::from).is_some_and(|b| b == *g);
        if !same_as_map {
            text.push_str(&own.to_cfg());
            changed += 1;
        }
    }
    let clean = |s: &str| s.replace(['\r', '\n'], " ").trim().to_string();
    // Everything is written under temporary names first and put in place after: a write that
    // fails (disk full, a file held by a virus scanner) leaves the player's old files as they were.
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new(); // (temporary, final)
    let mut stage = |to: PathBuf, text: String| -> Result<()> {
        std::fs::create_dir_all(to.parent().context("folder")?)?;
        let tmp = to.with_extension("tmp");
        std::fs::write(&tmp, text).with_context(|| format!("writing {}", to.display()))?;
        staged.push((tmp, to));
        Ok(())
    };
    let mut made = 0;
    let written = (|| -> Result<()> {
        if !text.is_empty() {
            stage(p.lists_file(), text)?;
        }
        for r in rules.iter().filter(|r| !r.line.trim().is_empty() && !r.types.is_empty() && (r.mode == "only" || r.mode == "prefer")) {
            let types: String = r.types.iter().map(|t| format!("{}\r\n", clean(t))).collect();
            let body = if r.mode == "only" {
                format!("[onlytypes]\r\n{types}[end]\r\n")
            } else {
                format!("[types_prefered]\r\n{}\r\n{types}[end]\r\n", r.factor.clamp(0.0, 1.0))
            };
            let text = format!("[valid]\r\n10000101\r\n99991231\r\n\r\n[line]\r\n{}\r\n\r\n{body}", clean(&r.line));
            stage(p.rules_dir().join(format!("{RULE_PREFIX}{:03}.ocu", made + 1)), text)?;
            made += 1;
        }
        Ok(())
    })();
    if let Err(e) = written {
        for (tmp, _) in &staged {
            let _ = std::fs::remove_file(tmp);
        }
        return Err(e);
    }
    // in place: the old files go, the new ones come
    if !staged.iter().any(|(_, to)| *to == p.lists_file()) {
        let _ = std::fs::remove_file(p.lists_file());
    }
    for old in files_of(&p.rules_dir(), RULE_PREFIX) {
        let _ = std::fs::remove_file(old);
    }
    for (tmp, to) in &staged {
        std::fs::rename(tmp, to).with_context(|| format!("writing {}", to.display()))?;
    }
    omsi_cfg::content_changed();
    Ok(json!({ "groups": changed, "rules": made }))
}

/// Back to what the map gives: the player's lists and rules of `map` deleted.
pub fn reset(map: &str) -> Result<()> {
    let p = place(map)?;
    let _ = std::fs::remove_file(p.lists_file());
    for f in files_of(&p.rules_dir(), RULE_PREFIX) {
        let _ = std::fs::remove_file(f);
    }
    omsi_cfg::content_changed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_survive_the_page() {
        let g = AiGroup {
            name: "Busses".into(),
            hof: Some("Grundorf".into()),
            is_depot: true,
            typgroups: vec![AiTypGroup { file: "Vehicles\\a\\b.bus".into(), entries: vec![DepotEntry { number: "1".into(), registration: "X".into(), paint: "red".into(), from: Some(19900101), to: None }] }],
            ..Default::default()
        };
        assert_eq!(GroupJson::from(&GroupJson::from(&g).to_group()), GroupJson::from(&g));
    }

    #[test]
    fn rules_read_back() {
        let c = CarUse { line: " 5 ".into(), types_prefered: Some((0.5, vec!["Vehicles\\a\\b.bus".into()])), ..Default::default() };
        let r = rule_of(&c).unwrap();
        assert_eq!((r.line.as_str(), r.mode.as_str(), r.factor), ("5", "prefer", 0.5));
    }
}
