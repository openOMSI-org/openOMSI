//! Scheduled AI buses: the map's timetable lines (`TTData`) put buses on their tracks at the
//! tour departure times; they follow the track lanes and stop at the trip's stations.
//!
//! The timetable itself is omsi-sim's `timetable_run` (`ScheduleSim`: the departures and
//! which are due, the tours' vehicles, the routes on the lanes, the departure boards; the
//! trip times, the IBIS, the player's duty), which does not touch the GPU. `Schedule` holds
//! it (`Deref` to `ScheduleSim`) with what the GPU needs: `fleet` reads and uploads the
//! vehicles ahead, `dispatch` and `spawn` put the buses on the road and take them off.

mod dispatch;
mod fleet;
mod spawn;
#[cfg(test)]
pub(crate) mod tests;

use crate::scene::World;
use crate::traffic::Traffic;
use crate::view_sync::traffic::TrafficView;
use hashbrown::{HashMap, HashSet};
use omsi_sim::VehicleType;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use omsi_sim::timetable_run::*;

pub use omsi_sim::timetable_run::{PlannedStop, PlannedTrip, PlayerDuty};
// (the tests of `schedule_paper` name it by this path)
#[cfg_attr(not(test), allow(unused_imports))]
pub use omsi_sim::timetable_run::StopDir;
pub(crate) use omsi_sim::timetable_run::{
    has_roller_blind, hhmm, ibis_stop_index, ibis_target, player_ibis, set_ai_destination,
    set_ai_destination_at, set_player_destination_at, set_player_destination_directly,
    shown_destination, turn_roller_blind, BlindPick,
};

/// The timetable (`ScheduleSim`, which everything of the timetable reads through `Deref`)
/// with the vehicle sets it reads and uploads ahead for the GPU.
pub struct Schedule {
    pub sim: ScheduleSim,
    next_number: usize,
    /// Vehicle sets being read ahead on the workers, with the type to upload them with, and
    /// those read and waiting for their upload.
    fleet_reading: HashMap<crate::scene::VehicleKey, Arc<VehicleType>>,
    fleet_ready: Arc<parking_lot::Mutex<Vec<crate::scene::VehicleKey>>>,
    /// Time of day of the last look at the next departures' vehicles.
    fleet_check: f64,
    /// Seconds until the next look for due departures (see `frame_traffic`).
    pub(crate) tick_t: f32,
}

impl std::ops::Deref for Schedule {
    type Target = ScheduleSim;
    fn deref(&self) -> &ScheduleSim {
        &self.sim
    }
}

impl std::ops::DerefMut for Schedule {
    fn deref_mut(&mut self) -> &mut ScheduleSim {
        &mut self.sim
    }
}


/// The player's bus company as the timetable asks it (`omsi_sim::timetable_run::
/// TimetableCompany`; see `company_live`).
impl TimetableCompany for crate::company_live::CompanyLive {
    fn active(&self) -> bool {
        crate::company_live::CompanyLive::active(self)
    }
    fn own_only(&self) -> bool {
        crate::company_live::CompanyLive::own_only(self)
    }
    fn set_day(&mut self, date_code: i32) {
        crate::company_live::CompanyLive::set_day(self, date_code)
    }
    fn keeps_off(&self, line: &str, tour: &str, map_bus: bool) -> bool {
        crate::company_live::CompanyLive::keeps_off(self, line, tour, map_bus)
    }
    fn runs_tour(&self, line: &str, tour: &str) -> bool {
        self.tour(line, tour).is_some()
    }
    fn report(&mut self, index: usize, line: &str, tour: &str, dep: i32, km: f64, stops: usize, delay: f64) {
        crate::company_live::CompanyLive::report(self, index, line, tour, dep, km, stops, delay)
    }
    /// Its own bus, in its paint, with its fleet number and plate, carrying the line's depot
    /// file, else the company's (None: not the company's tour, or a rental bus).
    fn bus(&mut self, root: &Path, line: &str, tour: &str) -> Option<CompanyBus> {
        let lt = self.tour(line, tour)?.clone();
        let ty = self.vehicle_type(root, &lt.bus)?;
        let depot = Some(lt.hof.trim().to_string()).filter(|h| !h.is_empty()).or_else(|| self.depot().map(str::to_string));
        let hof = depot.and_then(|name| depot_file(&mut self.hofs, ty.def.dir(), &name));
        let scheme = if lt.paint.trim().is_empty() { None } else { ty.paint_schemes.iter().position(|s| s.name.trim().eq_ignore_ascii_case(lt.paint.trim())) };
        Some(CompanyBus { ty, number: (lt.fleet_number.clone(), lt.plate.clone()), hof, scheme })
    }
}

impl Schedule {
    /// `clock` gives the date: tours carry a validity mask (bits 0-6 Monday…Sunday, 7 public
    /// holiday, 8 school days, 9 school holidays) that selects which run today.
    pub fn new(root: &Path, world: &World, clock: &omsi_sim::SimClock) -> Schedule {
        let mut sim = ScheduleSim::new(root, world, clock);
        // the player's own lines that ask for buses of their own (`line_buses`)
        let buses = line_buses(root, world, &sim);
        if !buses.is_empty() {
            sim.set_line_buses(buses);
        }
        Schedule {
            sim,
            next_number: 0,
            fleet_reading: HashMap::new(),
            fleet_ready: Default::default(),
            fleet_check: f64::NEG_INFINITY,
            // (a second after the population's, never in the same frame)
            tick_t: 1.0,
        }
    }

    /// The lanes a trip runs on (for the navigator): its track, else the station links
    /// between its stops, as far as the tiles have brought them - and whether that is all.
    pub fn trip_route(&self, world: &World, traffic: &Traffic, trip_name: &str) -> (Vec<usize>, bool) {
        self.sim.trip_route(world, &traffic.sim, trip_name)
    }

    /// `OMSI_CHECK_TRIPS=1`: the routes of every trip on the loaded lanes checked (see
    /// `ScheduleSim::check_routes`).
    pub fn check_routes(&self, world: &World, traffic: &mut Traffic) {
        self.sim.check_routes(world, &mut traffic.sim)
    }

    /// A drive for the player's bus company (`--company`): its plan of the day, and the map's
    /// own buses kept in their depots. Called before `precache`.
    pub fn set_company(&mut self, world: &World, id: &str) {
        let company = crate::company_live::CompanyLive::for_company(&world.map_dir, self.sim.day(), crate::company_live::data_dir(), id);
        self.sim.set_company(Box::new(company));
    }

    /// A duty of trips from several tours, as the launcher puts one together (`--duty-leg`,
    /// see `omsi_launcher_lib::compose`): each leg is `line|tour|first|trips`, so many trips
    /// of the tour from its trip `first` (1 = the first) on (see `ScheduleSim::player_plan`).
    pub fn player_plan(&mut self, world: &World, legs: &[String]) -> Result<PlayerDuty, String> {
        let mut parts = Vec::with_capacity(legs.len());
        for leg in legs {
            let Some(part) = omsi_launcher_lib::compose::Block::parse(leg) else {
                return Err(format!("'{leg}' is not a part of a duty (line|tour|first trip|trips)"));
            };
            parts.push(part);
        }
        self.sim.player_plan(world, &parts)
    }

    /// Assign the player a tour of a line (see `ScheduleSim::player_duty`).
    pub fn player_duty(
        &mut self,
        world: &World,
        line: &str,
        tour: &str,
        now: f64,
        trip: Option<&str>,
        whole_tour: bool,
    ) -> Result<PlayerDuty, String> {
        self.sim.player_duty(world, line, tour, now, trip, whole_tour)
    }

    /// Make the departure boards of the stops whose displays are near
    /// (`World::timetable_boards`), and hand the scenery the time of day. The boards are
    /// made at most once a second: the buses due at each stop in the next two hours,
    /// soonest first - the timetable buses with the delay they run with, and the player's.
    pub fn update_boards(
        &mut self,
        world: &World,
        traffic: Option<&Traffic>,
        duty: Option<&PlayerDuty>,
        clock: &omsi_sim::SimClock,
    ) {
        let now = clock.time;
        let mut boards = world.timetable_boards.lock();
        boards.clock = Some(clock.clone());
        // (a stop a page asks for anew is made at once, not up to a second later: the boards of
        // the map's own displays may just have been made without it)
        let new_name = boards.wanted_names.iter().any(|k| !boards.departures.contains_key(k));
        if (self.sim.boards_fresh(now) && !new_name) || (boards.wanted.is_empty() && boards.wanted_names.is_empty()) {
            return;
        }
        let (wanted, wanted_names) = (boards.wanted.clone(), boards.wanted_names.clone());
        let (by_stop, departures) =
            self.sim.make_boards(traffic.map(|t| &t.sim), wanted, wanted_names, duty, clock);
        boards.by_stop = by_stop;
        boards.departures = departures;
        boards.departures_gen = boards.departures_gen.wrapping_add(1);
    }
}

/// The player's own lines that ask for buses of a kind, a maker or a model (the line editor's
/// `lines::VEHICLES_FILE`): per line (its name, lower case) such buses of the line's depot
/// group, else of any of the map's depot groups, else the bus files the line names, loaded
/// here (without a fleet number, with the depot file of the line's group) - in the shape of
/// the timetable's depots (`ScheduleSim::set_line_buses`).
fn line_buses(root: &Path, world: &World, sim: &ScheduleSim) -> HashMap<String, Vec<DepotVehicle>> {
    let mut line_buses = HashMap::new();
    let wanted = omsi_launcher_lib::lines::read_line_buses(&omsi_cfg::resolve_path(&world.map_dir, "TTData"));
    if wanted.is_empty() {
        return line_buses;
    }
    let depots = sim.depots();
    let mut groups: Vec<&String> = depots.keys().collect();
    groups.sort();
    let mut hof_cache: HashMap<(PathBuf, String), Option<Arc<omsi_vehicle::Hof>>> = HashMap::new();
    for (line, want) in &wanted {
        let group = sim.data.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).and_then(|l| l.tours.first()).map(|t| t.ai_group.to_ascii_lowercase()).unwrap_or_default();
        let fits = |t: &VehicleType| want.allows(&bus_facts(&t.def));
        let mut chosen: Vec<DepotVehicle> = depots.get(&group).map(|v| v.iter().filter(|x| fits(&x.0)).cloned().collect()).unwrap_or_default();
        if chosen.is_empty() {
            let mut seen: HashSet<PathBuf> = HashSet::new();
            for g in &groups {
                for x in depots[*g].iter().filter(|x| fits(&x.0)) {
                    if seen.insert(x.0.def.path.clone()) {
                        chosen.push(x.clone());
                    }
                }
            }
        }
        if chosen.is_empty() {
            let hof = world.ailists.groups.iter().find(|g| g.name.eq_ignore_ascii_case(&group)).and_then(|g| g.hof.clone());
            for file in want.buses.iter().flat_map(|p| if p.file.trim().is_empty() { p.files.clone() } else { vec![p.file.clone()] }) {
                let path = omsi_cfg::resolve_path(root, &file);
                let Ok(t) = VehicleType::load_ai(root, &path).map(Arc::new).map_err(|e| log::debug!("{}: {e}", path.display())) else { continue };
                if chosen.iter().any(|x| x.0.def.path == t.def.path) {
                    continue;
                }
                let h = hof.as_ref().and_then(|h| depot_file(&mut hof_cache, t.def.dir(), h));
                chosen.push((t, Vec::new(), h));
            }
        }
        if chosen.is_empty() {
            log::warn!("timetable: line {line} asks for buses the map's depots do not have: its depot group's drive it");
        } else {
            log::info!("timetable: line {line} runs with {}", chosen.iter().map(|x| x.0.def.path.file_stem().unwrap_or_default().to_string_lossy().into_owned()).collect::<Vec<_>>().join(", "));
            line_buses.insert(line.to_ascii_lowercase(), chosen);
        }
    }
    line_buses
}

/// What a line's choice of buses (`service::LineVehicles`) is weighed against of a vehicle
/// the game loaded: its file from `Vehicles/` on, its maker and model as the launcher's bus
/// picker groups them, and its kind (its length, height and trailer section).
fn bus_facts(def: &omsi_vehicle::Vehicle) -> omsi_launcher_lib::service::BusFacts {
    use omsi_launcher_lib::service;
    let path = def.path.to_string_lossy().replace('\\', "/");
    let file = path.to_ascii_lowercase().rfind("vehicles/").map(|k| path[k..].to_string()).unwrap_or_else(|| path.clone());
    let folder = file.split('/').nth(1).unwrap_or_default().to_string();
    let (maker, model) = service::maker_model(&def.manufacturer, &def.type_name, &file, &folder);
    let bb = def.bounding_box;
    let class = service::classify(&[&def.manufacturer, &def.type_name, &file], def.couple_back.is_some(), bb.map(|b| b[1]), bb.map(|b| b[2]));
    service::BusFacts { name: format!("{maker} {model}"), file, maker, model, class }
}

impl Schedule {
    /// The timetable buses on the road, for outside tools (`telemetry`): line, tour, trip file,
    /// next stop (map object), standing at it, delay, position and fleet number of each.
    pub fn telemetry_ai(&self, traffic: Option<&crate::traffic::Traffic>) -> serde_json::Value {
        let Some(t) = traffic else { return serde_json::Value::Array(Vec::new()) };
        let list = self.sim.ai_bus_rows(&t.sim);
        serde_json::Value::Array(list.into_iter().map(|b| serde_json::json!({
            "id": b.id,
            "line": b.line,
            "tour": b.tour,
            "trip": b.trip,
            "terminus": b.terminus,
            "depart": b.depart,
            "next_stop_id": b.next_stop_id,
            "at_stop": b.at_stop,
            "trip_done": b.trip_done,
            "delay_s": b.delay_s,
            "x": b.x, "y": b.y,
            "number": b.number,
        })).collect())
    }
}
