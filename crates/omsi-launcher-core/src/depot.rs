//! The player's own buses (`~/.openomsi/depot.json`): each keeps its look, its counters and its wear.

use crate::install::now_secs;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The fuel in litres, as OMSI's engine scripts keep it.
pub const FUEL: &str = "engine_tank_content";
/// How dirty the body is (0..1), fed to the model by the engine.
pub const DIRT: &str = "Dirt_Norm";

/// A script variable a bus carries from one session to the next.
pub fn is_wear_var(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "engine_tank_content" | "dirt_norm" | "kmcounter_km" | "kmcounter_m" | "elec_battery_load" | "elec_battery_q_curr" | "elec_battery_age" | "cp_zentralschmierung_nextkm"
    ) || (n.starts_with("collision_energy") && !n.ends_with("_threshold"))
        // OMSI's own wear: the bulbs' hours left, the automatic door's cycles left, and the
        // failures a collision leaves (the stock scripts, and the mods built on them)
        || (n.contains("_bulb") && n.ends_with("_lifetime"))
        || n.starts_with("door_malf_")
        || n.ends_with("_failure_general")
}

/// Whether a wear variable's value is worth keeping: a bulb's hours or a door's cycles drawn
/// with "Maintenance: Infinite" (`wearlifespan` 1.5e6) are no wear at all, and kept they would
/// stay endless once the setting wears again (OMSI's longest, "Good", is 20000 hours).
pub fn keeps(name: &str, value: f32) -> bool {
    let n = name.to_ascii_lowercase();
    !((n.ends_with("_lifetime") || n.starts_with("door_malf_")) && value > 1.0e5)
}

/// The part of the bus a wear variable belongs to (its id and its name), and whether the
/// value says it is broken: a bulb with no hours left, a door with no cycles left, a
/// failure set by a collision, damage taken.
fn part(name: &str, value: f32) -> Option<(&'static str, &'static str, bool)> {
    let n = name.to_ascii_lowercase();
    if n.ends_with("_failure_general") {
        let (id, label) = match n.split('_').next() {
            Some("engine") => ("engine", "Engine failure"),
            Some("elec") => ("elec", "Electrical failure"),
            Some("antrieb") => ("drive", "Drivetrain failure"),
            _ => ("system", "System failure"),
        };
        return Some((id, label, value >= 0.5));
    }
    if n.contains("_bulb") && n.ends_with("_lifetime") {
        let (id, label) = if n.contains("blinker_l") {
            ("blinker_l", "Left indicator bulb blown")
        } else if n.contains("blinker_r") {
            ("blinker_r", "Right indicator bulb blown")
        } else if n.contains("abbl") {
            ("headlight", "Headlight bulb blown")
        } else if n.contains("edge") {
            ("side_light", "Side light bulb blown")
        } else if n.contains("fahrerlicht") {
            ("driver_light", "Driver's light bulb blown")
        } else {
            ("bulb", "A bulb blown")
        };
        return Some((id, label, value < 0.0));
    }
    if n.starts_with("door_malf_") {
        return Some(("door", "Automatic door faulty", n.ends_with("_remcyc") && value < 0.0));
    }
    if n.starts_with("collision_energy") {
        return Some(("body", "Body damage", value > 0.0));
    }
    None
}

/// A part of a depot bus that no longer works.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fault {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DepotBus {
    pub id: String,
    pub name: String,
    /// The `.bus` file, relative to the OMSI folder.
    pub bus: String,
    pub paint: String,
    pub plate: String,
    pub number: String,
    pub added: u64,
    pub last_used: u64,
    pub sessions: u32,
    pub metres: f64,
    pub seconds: f64,
    pub stops: i32,
    pub tickets: i32,
    pub takings: f64,
    pub crashes: i32,
    /// Litres burnt, counted between the start and the end of each session.
    pub fuel_used: f64,
    /// The wear variables as the last session left them (none yet: as the bus comes).
    pub wear: Vec<(String, f32)>,
    /// The last sessions, the newest first.
    pub history: Vec<Run>,
    /// The depot it is kept in (one of [`Depot::groups`]; empty: none).
    pub group: String,
}

/// One session of a depot bus.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Run {
    pub time: u64,
    pub map: String,
    pub line: String,
    pub tour: String,
    pub metres: f64,
    pub seconds: f64,
    pub stops: i32,
    pub tickets: i32,
    pub crashes: i32,
    pub fuel: f64,
}

/// How many sessions a bus keeps in its history.
const HISTORY: usize = 30;

impl DepotBus {
    pub fn wear_var(&self, name: &str) -> Option<f32> {
        self.wear.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| *v)
    }

    pub fn fuel(&self) -> Option<f32> {
        self.wear_var(FUEL)
    }

    pub fn dirt(&self) -> Option<f32> {
        self.wear_var(DIRT)
    }

    /// What its odometer shows (km), once a session has read it.
    pub fn odometer(&self) -> Option<f64> {
        Some(self.wear_var("kmcounter_km")? as f64 + self.wear_var("kmcounter_m").unwrap_or(0.0) as f64 / 1000.0)
    }

    /// The depot's services: "refuel" (the tank as the bus comes, full), "wash", "repair"
    /// (every part) or "repair:<part>" (one, see [`DepotBus::faults`]); false for another word.
    /// A part repaired is dropped from the wear, so its script gives it new hours or cycles
    /// at the next start, as OMSI's workshop does.
    pub fn service(&mut self, kind: &str) -> bool {
        match kind {
            "refuel" => self.wear.retain(|(n, _)| !n.eq_ignore_ascii_case(FUEL)),
            "wash" => {
                self.wear.retain(|(n, _)| !n.eq_ignore_ascii_case(DIRT));
                self.wear.push((DIRT.into(), 0.0));
            }
            "repair" => self.wear.retain(|(n, v)| part(n, *v).is_none()),
            _ => {
                let Some(id) = kind.strip_prefix("repair:") else { return false };
                // (the energy a collision built up goes with the failure it caused, as OMSI's
                // Collision_Malfunction_Reset does: the engine and the drive with the rear's)
                let energy = match id {
                    "engine" | "drive" => Some("collision_energy_eng"),
                    "elec" => Some("collision_energy"),
                    _ => None,
                };
                self.wear.retain(|(n, v)| part(n, *v).is_none_or(|(p, _, _)| p != id) && !energy.is_some_and(|e| n.eq_ignore_ascii_case(e)));
            }
        }
        true
    }

    /// The parts that no longer work, each once, in the order the bus's scripts list them.
    pub fn faults(&self) -> Vec<Fault> {
        let mut out: Vec<Fault> = Vec::new();
        for (n, v) in &self.wear {
            if let Some((id, label, true)) = part(n, *v) {
                if !out.iter().any(|f| f.id == id) {
                    out.push(Fault { id, label });
                }
            }
        }
        out
    }

}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Depot {
    pub buses: Vec<DepotBus>,
    /// The player's depots, in their order (a map's, a company's...): the buses are sorted
    /// into them.
    pub groups: Vec<String>,
}

impl Depot {
    pub fn get(&self, id: &str) -> Option<&DepotBus> {
        self.buses.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut DepotBus> {
        self.buses.iter_mut().find(|b| b.id == id)
    }

    /// Adds a bus under a new id and returns that id.
    pub fn add(&mut self, mut b: DepotBus) -> String {
        let mut n = now_secs();
        while self.buses.iter().any(|x| x.id == format!("{n:x}")) {
            n += 1;
        }
        b.id = format!("{n:x}");
        b.added = now_secs();
        let id = b.id.clone();
        self.buses.push(b);
        id
    }

    pub fn remove(&mut self, id: &str) {
        self.buses.retain(|b| b.id != id);
    }

    /// The depots, with any a bus names that the list lacks (an edited file) at the end.
    pub fn group_names(&self) -> Vec<String> {
        let mut out = self.groups.clone();
        for b in &self.buses {
            if !b.group.is_empty() && !out.iter().any(|g| g == &b.group) {
                out.push(b.group.clone());
            }
        }
        out
    }

    /// A new depot; false for an empty name or one there is already.
    pub fn add_group(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.group_names().iter().any(|g| g.eq_ignore_ascii_case(name)) {
            return false;
        }
        self.groups.push(name.to_string());
        true
    }

    /// A depot renamed, with its buses; false when the new name is empty or taken.
    pub fn rename_group(&mut self, from: &str, to: &str) -> bool {
        let to = to.trim();
        if to.is_empty() || self.group_names().iter().any(|g| g != from && g.eq_ignore_ascii_case(to)) {
            return false;
        }
        for g in self.groups.iter_mut().filter(|g| *g == from) {
            *g = to.to_string();
        }
        for b in self.buses.iter_mut().filter(|b| b.group == from) {
            b.group = to.to_string();
        }
        true
    }

    /// A depot removed: its buses stay, in no depot.
    pub fn remove_group(&mut self, name: &str) {
        self.groups.retain(|g| g != name);
        for b in self.buses.iter_mut().filter(|b| b.group == name) {
            b.group.clear();
        }
    }
}

/// Letters of French plates (SIV): no I, O or U, which read as 1, 0 and V.
const PLATE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTVWXYZ";

/// A French plate (`AB-123-CD`) from `seed`, as the SIV gives them: no SS, no WW in front, no 000.
pub fn french_plate(seed: u64) -> String {
    let mut x = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut next = |n: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % n
    };
    loop {
        let mut pair = || [PLATE_LETTERS[next(PLATE_LETTERS.len() as u64) as usize] as char, PLATE_LETTERS[next(PLATE_LETTERS.len() as u64) as usize] as char];
        let (a, c) = (pair(), pair());
        let n = next(999) + 1;
        if a == ['S', 'S'] || c == ['S', 'S'] || a == ['W', 'W'] {
            continue;
        }
        return format!("{}{}-{n:03}-{}{}", a[0], a[1], c[0], c[1]);
    }
}

impl Depot {
    /// A French plate none of the depot's buses has.
    pub fn new_plate(&self, seed: u64) -> String {
        (0..).map(|k| french_plate(seed.wrapping_add(k))).find(|p| !self.buses.iter().any(|b| b.plate.eq_ignore_ascii_case(p))).unwrap_or_default()
    }
}

/// What one session did to a bus.
#[derive(Debug, Clone, Default)]
pub struct Outing {
    pub map: String,
    pub line: String,
    pub tour: String,
    pub metres: f64,
    pub seconds: f64,
    pub stops: i32,
    pub tickets: i32,
    pub takings: f64,
    pub crashes: i32,
}

pub fn path() -> PathBuf {
    crate::data_dir().join("depot.json")
}

pub fn load() -> Result<Depot> {
    load_from(&path())
}

pub fn save(d: &Depot) -> Result<()> {
    save_to(&path(), d)
}

pub fn load_from(p: &Path) -> Result<Depot> {
    match std::fs::read(p) {
        Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| format!("reading {}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Depot::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", p.display())),
    }
}

/// One change of the file at a time in this process: the launcher's commands run in
/// parallel, and two of them loading the depot before either saved lost one change.
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Load the depot, change it with `f` and save it, the whole way under [`WRITE`].
pub fn update<R>(f: impl FnOnce(&mut Depot) -> Result<R>) -> Result<R> {
    update_in(&path(), f)
}

pub fn update_in<R>(p: &Path, f: impl FnOnce(&mut Depot) -> Result<R>) -> Result<R> {
    let _one = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut d = load_from(p)?;
    let r = f(&mut d)?;
    save_to(p, &d)?;
    Ok(r)
}

/// Written aside first, so that a game and the launcher saving at once never leave half a
/// file (under a name of its own: the process, its thread and a count).
pub fn save_to(p: &Path, d: &Depot) -> Result<()> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = p.with_extension(format!("json.{}.{:?}.{n}.tmp", std::process::id(), std::thread::current().id()).replace(['(', ')'], ""));
    std::fs::write(&tmp, serde_json::to_vec_pretty(d)?)?;
    std::fs::rename(&tmp, p).with_context(|| format!("writing {}", p.display()))
}

/// Adds a session to bus `id` in the file at `p` and keeps the wear it ended with; false when the bus is gone.
pub fn record_in(p: &Path, id: &str, outing: &Outing, wear: Vec<(String, f32)>) -> Result<bool> {
    update_in(p, |d| {
        let Some(b) = d.get_mut(id) else { return Ok(false) };
        let start = b.fuel();
        let end = wear.iter().find(|(n, _)| n.eq_ignore_ascii_case(FUEL)).map(|(_, v)| *v);
        let fuel = match (start, end) {
            (Some(s), Some(e)) => (s - e).max(0.0) as f64,
            _ => 0.0,
        };
        b.fuel_used += fuel;
        b.history.insert(0, Run { time: now_secs(), map: outing.map.clone(), line: outing.line.clone(), tour: outing.tour.clone(), metres: outing.metres, seconds: outing.seconds, stops: outing.stops, tickets: outing.tickets, crashes: outing.crashes, fuel });
        b.history.truncate(HISTORY);
        b.sessions += 1;
        b.metres += outing.metres;
        b.seconds += outing.seconds;
        b.stops += outing.stops;
        b.tickets += outing.tickets;
        b.takings += outing.takings;
        b.crashes += outing.crashes;
        b.last_used = now_secs();
        b.wear = wear;
        Ok(true)
    })
}

pub fn record(id: &str, outing: &Outing, wear: Vec<(String, f32)>) -> Result<bool> {
    record_in(&path(), id, outing, wear)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omsi-depot-test-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d.join("depot.json")
    }

    #[test]
    fn wear_is_fuel_battery_dirt_and_damage_not_thresholds() {
        for n in ["engine_tank_content", "Dirt_Norm", "kmcounter_km", "elec_battery_load", "collision_energy", "collision_energy_eng"] {
            assert!(is_wear_var(n), "{n}");
        }
        for n in ["collision_energy_eng_threshold", "engine_RPM", "door_0", "cp_odometer_1"] {
            assert!(!is_wear_var(n), "{n}");
        }
    }

    #[test]
    fn french_plates_follow_the_siv() {
        for seed in 0..5000u64 {
            let p = french_plate(seed);
            let c: Vec<char> = p.chars().collect();
            assert_eq!(c.len(), 9, "{p}");
            assert!(c[2] == '-' && c[6] == '-', "{p}");
            for i in [0, 1, 7, 8] {
                assert!(c[i].is_ascii_uppercase() && !"IOU".contains(c[i]), "{p}");
            }
            assert!(c[3..6].iter().all(|d| d.is_ascii_digit()) && &p[3..6] != "000", "{p}");
            assert!(&p[0..2] != "SS" && &p[7..9] != "SS" && &p[0..2] != "WW", "{p}");
        }
        assert_ne!(french_plate(1), french_plate(2));
        let mut d = Depot::default();
        let taken = d.new_plate(42);
        d.add(DepotBus { plate: taken.clone(), ..Default::default() });
        assert_ne!(d.new_plate(42), taken);
    }

    #[test]
    fn a_missing_file_is_an_empty_depot_and_a_saved_one_reads_back() {
        let p = scratch("roundtrip");
        let _ = std::fs::remove_file(&p);
        assert_eq!(load_from(&p).unwrap(), Depot::default());
        let mut d = Depot::default();
        let id = d.add(DepotBus { name: "Mon Citaro".into(), plate: "GP-123-MO".into(), ..Default::default() });
        save_to(&p, &d).unwrap();
        let back = load_from(&p).unwrap();
        assert_eq!(back, d);
        assert_eq!(back.get(&id).unwrap().plate, "GP-123-MO");
    }

    #[test]
    fn a_session_adds_up_and_leaves_its_wear() {
        let p = scratch("record");
        let mut d = Depot::default();
        let id = d.add(DepotBus { wear: vec![(FUEL.into(), 200.0)], metres: 1000.0, ..Default::default() });
        save_to(&p, &d).unwrap();
        let outing = Outing { map: "maps/Grundorf/global.cfg".into(), line: "1".into(), metres: 12_000.0, seconds: 1800.0, stops: 14, tickets: 3, takings: 6.3, crashes: 1, ..Default::default() };
        let wear = vec![(FUEL.into(), 188.5), ("collision_energy".into(), 40.0)];
        assert!(record_in(&p, &id, &outing, wear.clone()).unwrap());
        let b = load_from(&p).unwrap().get(&id).cloned().unwrap();
        assert_eq!((b.sessions, b.metres, b.stops, b.tickets, b.crashes), (1, 13_000.0, 14, 3, 1));
        assert!((b.fuel_used - 11.5).abs() < 1e-6);
        assert_eq!(b.wear, wear);
        assert_eq!(b.faults(), [Fault { id: "body", label: "Body damage" }]);
        assert!(!record_in(&p, "gone", &outing, Vec::new()).unwrap());
        assert_eq!((b.history.len(), b.history[0].line.as_str(), b.history[0].fuel), (1, "1", 11.5));
    }

    #[test]
    fn the_wear_of_omsi_s_scripts_is_kept_and_its_broken_parts_listed() {
        for n in ["lights_blinker_l_bulb_3_lifetime", "lights_abbl_bulb_1_lifetime", "door_malf_autobdoor_remcyc", "engine_failure_general"] {
            assert!(is_wear_var(n), "{n}");
        }
        assert!(!is_wear_var("lights_blinker_l") && !is_wear_var("lights_bulb_signal_livetime_h"));
        assert!(keeps("lights_abbl_bulb_1_lifetime", 1500.0) && !keeps("lights_abbl_bulb_1_lifetime", 2.5e8) && keeps(FUEL, 2.5e8));
        let wear = |v: &[(&str, f32)]| v.iter().map(|(n, x)| (n.to_string(), *x)).collect::<Vec<_>>();
        let mut b = DepotBus {
            wear: wear(&[
                ("lights_blinker_l_bulb_1_lifetime", 120.0),
                ("lights_blinker_l_bulb_2_lifetime", -0.5),
                ("lights_blinker_r_bulb_1_lifetime", 300.0),
                ("lights_abbl_bulb_2_lifetime", -2.0),
                ("door_malf_autobdoor_remcyc", 40.0),
                ("door_malf_autobdoor_reopen_prob", -1.0),
                ("engine_failure_general", 1.0),
                ("collision_energy_eng", 80.0),
                ("collision_energy", 0.0),
                (FUEL, 90.0),
            ]),
            ..Default::default()
        };
        let ids = |b: &DepotBus| b.faults().iter().map(|f| f.id).collect::<Vec<_>>();
        assert_eq!(ids(&b), ["blinker_l", "headlight", "engine", "body"]);
        assert!(b.service("repair:blinker_l"));
        assert_eq!(ids(&b), ["headlight", "engine", "body"]);
        // (every bulb of the left indicator gets new hours, the right one's stay as they are)
        assert!(b.wear_var("lights_blinker_l_bulb_1_lifetime").is_none() && b.wear_var("lights_blinker_r_bulb_1_lifetime").is_some());
        assert!(b.service("repair:engine"));
        assert_eq!(ids(&b), ["headlight"]);
        assert!(b.service("repair"));
        assert!(b.faults().is_empty() && b.fuel() == Some(90.0) && b.wear_var("door_malf_autobdoor_remcyc").is_none());
        assert!(!b.service("paint"));
    }

    #[test]
    fn buses_are_sorted_into_depots() {
        let mut d = Depot::default();
        assert!(d.add_group("Grand Paris-Moulon") && d.add_group("St-Servan"));
        assert!(!d.add_group(" st-servan ") && !d.add_group(""));
        let a = d.add(DepotBus { group: "St-Servan".into(), ..Default::default() });
        let b = d.add(DepotBus { group: "Ahlheim".into(), ..Default::default() });
        assert_eq!(d.group_names(), ["Grand Paris-Moulon", "St-Servan", "Ahlheim"]);
        assert!(!d.rename_group("St-Servan", "grand paris-moulon"));
        assert!(d.rename_group("St-Servan", "Saint-Servan"));
        assert_eq!(d.get(&a).unwrap().group, "Saint-Servan");
        d.remove_group("Saint-Servan");
        assert_eq!((d.get(&a).unwrap().group.as_str(), d.get(&b).unwrap().group.as_str()), ("", "Ahlheim"));
        assert_eq!(d.groups, ["Grand Paris-Moulon"]);
    }

    #[test]
    fn changes_made_at_once_are_all_kept() {
        let p = scratch("parallel");
        let _ = std::fs::remove_file(&p);
        let threads: Vec<_> = (0..16)
            .map(|i| {
                let p = p.clone();
                std::thread::spawn(move || update_in(&p, |d| Ok(d.add(DepotBus { name: format!("bus {i}"), ..Default::default() }))).unwrap())
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(load_from(&p).unwrap().buses.len(), 16);
    }

    #[test]
    fn the_history_keeps_the_last_sessions() {
        let p = scratch("history");
        let mut d = Depot::default();
        let id = d.add(DepotBus::default());
        save_to(&p, &d).unwrap();
        for _ in 0..40 {
            assert!(record_in(&p, &id, &Outing::default(), Vec::new()).unwrap());
        }
        assert_eq!(load_from(&p).unwrap().get(&id).unwrap().history.len(), HISTORY);
    }

    #[test]
    fn the_services_fill_clean_and_repair() {
        let mut b = DepotBus { wear: vec![(FUEL.into(), 40.0), (DIRT.into(), 0.8), ("collision_energy".into(), 30.0), ("kmcounter_km".into(), 9.0)], ..Default::default() };
        assert!(b.service("refuel") && b.service("wash") && b.service("repair"));
        assert_eq!((b.fuel(), b.dirt(), b.faults().is_empty(), b.wear_var("kmcounter_km")), (None, Some(0.0), true, Some(9.0)));
        assert!(!b.service("paint"));
    }
}
