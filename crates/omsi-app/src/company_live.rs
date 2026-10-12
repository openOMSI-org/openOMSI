//! The player's bus company in the game (the launcher's company, `omsi_launcher_lib::company`):
//! time runs mixed - while the player drives, the company's tours on the map are run by the
//! AI with the company's own buses, and what they drive goes back to the company's books
//! (`company::day::record_live` through `store::append_live`); the days not played are
//! settled with "Close the day".
//!
//! The launcher writes the company's day for the game (`company::plan::LivePlan`, each tour
//! with its bus, paint, fleet number and plate, or dropped). The timetable takes it only in a
//! drive from the company's planning (`--company`, Luc: in the company's mode only its own
//! buses; in a duty or tour of one's own the map's as ever), when the map and the date are
//! the company's: those tours get that bus instead of one of the map's depot
//! (`Schedule::choose`), a dropped tour does not run, the map's other timetable buses stay in
//! their depots (`keeps_off`), and each trip a company bus drives to its end is reported
//! once.

use omsi_launcher_lib::company::plan::{self, LivePlan, LiveTour};
use omsi_launcher_lib::company::store;
use omsi_sim::VehicleType;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub struct CompanyLive {
    map_dir: PathBuf,
    data: PathBuf,
    /// The company the game was started for (`--company`): only its tours run. None: a drive
    /// of the player's own, the map's timetable as ever.
    company: Option<String>,
    plan: Option<LivePlan>,
    /// The plan is the company's own day: what its buses drive is reported (a later day's,
    /// for a duty driven ahead, only puts them on the road).
    reports: bool,
    /// The company's bus types, loaded the first time a tour wants one (None: did not load).
    types: HashMap<String, Option<Arc<VehicleType>>>,
    /// Departures already reported.
    reported: HashSet<usize>,
    /// The company's depot file as each bus folder has it (the timetable's `depot_file`).
    pub(crate) hofs: hashbrown::HashMap<(PathBuf, String), Option<Arc<omsi_vehicle::Hof>>>,
}

/// openOMSI's own data folder, `~/.openomsi` (the launcher's `data_dir`; not made here).
pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    home.join(".openomsi")
}

/// `20240304` as `2024-03-04`.
fn date_of(code: i32) -> String {
    format!("{:04}-{:02}-{:02}", code / 10000, (code / 100) % 100, code % 100)
}

impl CompanyLive {
    /// A drive for company `id` (`--company`): its plan of this map and date (from `data`, the
    /// launcher's data folder) - its own day's, else a duty driven ahead's.
    pub fn for_company(map_dir: &Path, date_code: i32, data: PathBuf, id: &str) -> CompanyLive {
        let mut c = CompanyLive { map_dir: map_dir.to_path_buf(), data, company: Some(id.trim().to_string()), ..Default::default() };
        c.set_day(date_code);
        c
    }

    /// The day moved on (midnight): the plan of the new day, if there is one.
    pub fn set_day(&mut self, date_code: i32) {
        self.plan = None;
        self.reports = false;
        self.reported.clear();
        let Some(id) = self.company.clone() else { return };
        let date = date_of(date_code);
        let fits = |p: &LivePlan| p.company == id && p.is_for(&self.map_dir, &date);
        if let Some(p) = plan::live_plans(&self.data).into_iter().find(|p| fits(p)) {
            self.plan = Some(p);
            self.reports = true;
        } else {
            self.plan = plan::drive_plan(&self.data, &id).filter(|p| fits(p));
        }
        match &self.plan {
            Some(p) => {
                let buses = p.tours.iter().filter(|t| t.vehicle.is_some()).count();
                let dropped = p.tours.iter().filter(|t| t.dropped).count();
                log::info!("company {id}: only its own buses run today ({date}): {buses} of its tours with its buses, {dropped} dropped; the map's other timetable buses stay in their depots");
            }
            None => log::info!("company {id}: no plan of it for {date} on this map - none of the map's timetable buses run"),
        }
    }

    pub fn active(&self) -> bool {
        self.plan.is_some()
    }

    /// A drive from the company's planning: only its buses run, plan for the day or not.
    pub fn own_only(&self) -> bool {
        self.company.is_some()
    }

    /// Whether the timetable keeps a tour off the road: one the company dropped today, and in
    /// a drive for the company every tour of the map's own buses (`map_bus`) that is not the
    /// company's. Trains and planes run as ever.
    pub fn keeps_off(&self, line: &str, tour: &str, map_bus: bool) -> bool {
        match self.tour(line, tour) {
            Some(t) => t.dropped,
            None => self.own_only() && map_bus,
        }
    }

    /// The company's tour, if it is one.
    pub fn tour(&self, line: &str, tour: &str) -> Option<&LiveTour> {
        self.plan.as_ref()?.tour(line, tour)
    }

    /// The depot file the company's buses carry.
    pub fn depot(&self) -> Option<&str> {
        self.plan.as_ref().map(|p| p.depot.as_str()).filter(|d| !d.trim().is_empty())
    }

    /// The bus type of a company bus file (loaded once).
    pub fn vehicle_type(&mut self, root: &Path, file: &str) -> Option<Arc<VehicleType>> {
        let key = file.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        self.types
            .entry(key)
            .or_insert_with(|| {
                let path = omsi_cfg::resolve_path(root, file.trim());
                match VehicleType::load_ai(root, &path) {
                    Ok(t) => Some(Arc::new(t)),
                    Err(e) => {
                        log::warn!("company bus {file}: {e}");
                        None
                    }
                }
            })
            .clone()
    }

    /// A company trip its bus drove to the end (departure `index`, of `line`/`tour`, left at
    /// minute `dep`): reported to the company once.
    pub fn report(&mut self, index: usize, line: &str, tour: &str, dep: i32, km: f64, stops: usize, delay: f64) {
        let Some(p) = self.plan.as_ref().filter(|_| self.reports) else { return };
        if self.reported.contains(&index) {
            return;
        }
        let Some(ev) = p.trip_event(line, tour, dep, km, stops, delay) else { return };
        self.reported.insert(index);
        if let Err(e) = store::append_live(&self.data, &p.company, &ev) {
            log::warn!("company {}: a trip of line {line} tour {tour} not reported: {e:#}", p.company);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::day::LiveEvent;

    #[test]
    fn the_company_day_is_taken_for_its_map_and_date_and_trips_are_reported_once() {
        let data = std::env::temp_dir().join(format!("openomsi-company-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: "Live".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        c.depot = "Grundorf.hof".into();
        store::save(&data, &c).unwrap();
        let lp = LivePlan {
            company: c.id.clone(),
            map: c.map.clone(),
            date: c.date.clone(),
            depot: c.depot.clone(),
            difficulty: c.difficulty,
            tours: vec![
                LiveTour { line: "Linie5".into(), number: "5".into(), tour: "1".into(), vehicle: Some(3), bus: "Vehicles/X/x.bus".into(), ..Default::default() },
                LiveTour { line: "Linie5".into(), number: "5".into(), tour: "2".into(), dropped: true, ..Default::default() },
            ],
        };
        plan::save_live_plan(&data, &lp).unwrap();
        let grundorf = Path::new("C:/OMSI 2/maps/Grundorf");
        // a drive of the player's own on the company's map and day: the map as ever, nothing
        // of the company, nothing reported
        let mut own = CompanyLive::default();
        own.set_day(20240304);
        assert!(!own.active() && !own.keeps_off("Linie9", "1", true));
        own.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        // another map, another day: no plan - and in the company's drive none of the map's buses
        assert!(!CompanyLive::for_company(Path::new("C:/OMSI 2/maps/Ahlheim"), 20240304, data.clone(), &c.id).active());
        let later = CompanyLive::for_company(grundorf, 20240305, data.clone(), &c.id);
        assert!(!later.active() && later.keeps_off("Linie9", "1", true));
        // another company's drive on that map and day: not this company's plan
        let other = CompanyLive::for_company(grundorf, 20240304, data.clone(), "someone-else");
        assert!(!other.active() && other.keeps_off("Linie5", "1", true));
        let mut live = CompanyLive::for_company(grundorf, 20240304, data.clone(), &c.id);
        assert!(live.active());
        assert_eq!(live.depot(), Some("Grundorf.hof"));
        // its tour runs, its dropped tour not, the map's bus not, a train as ever
        assert!(!live.keeps_off("Linie5", "1", true) && live.keeps_off("linie5", "2", true));
        assert!(live.keeps_off("Linie9", "1", true) && !live.keeps_off("Linie9", "1", false));
        assert_eq!(live.tour("Linie5", "1").and_then(|t| t.vehicle), Some(3));
        live.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        live.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        live.report(18, "Linie9", "1", 7 * 60, 12.5, 14, 75.0);
        let base = c.clone();
        store::take_live(&data, &mut c);
        assert_eq!(c.live.len(), 1);
        assert!(matches!(&c.live[0], LiveEvent::Trip { vehicle: Some(3), dep: Some(360), delay, .. } if *delay == 75.0));
        // a duty driven ahead: that day's plan puts the buses on the road, nothing is reported
        plan::save_drive_plan(&data, &LivePlan { date: "2024-03-05".into(), ..lp.clone() }).unwrap();
        let mut ahead = CompanyLive::for_company(grundorf, 20240305, data.clone(), &c.id);
        assert!(ahead.active() && !ahead.keeps_off("Linie5", "1", true));
        ahead.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        // (the day's trips read again into the company as it was: still the one)
        let mut again = base.clone();
        store::take_live(&data, &mut again);
        assert_eq!(again.live.len(), 1);
        assert_eq!(plan::live_plans(&data).len(), 1);
        assert!(plan::drive_plan(&data, "../x").is_none());
        let _ = std::fs::remove_dir_all(&data);
    }
}
