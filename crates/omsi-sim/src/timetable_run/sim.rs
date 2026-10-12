//! The timetable's state at run time (`ScheduleSim`): the day's departures, which of them are
//! due, queued, waiting for their tiles or on the road, the tours' vehicles, the routes on
//! the lanes and the departure boards. It decides which trip leaves when and with which
//! vehicle, and where on its route the bus is; omsi-app's `schedule` puts the buses on the
//! road with it (their vehicles uploaded, spawned, turned round) and hands it the map
//! through [`TimetableWorld`].

mod boards;
mod choice;
mod company;
mod dispatch;
mod load;
mod place;
mod route;
mod take_on;
mod tours;

use super::*;
use crate::ai_traffic::TrafficSim;
use crate::VehicleType;
use glam::DVec3;
use omsi_timetable::TimetableData;
use parking_lot::MutexGuard;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use choice::{depot_file, pool_depot};
pub use company::{CompanyBus, TimetableCompany};
pub use dispatch::{tour_entry, TOUR_ENTRY_LANES};
pub use place::Placing;

/// What the timetable needs of the loaded map: its folders, the AI lists, the tiles, the
/// date and the places of the stops. omsi-app's `World` gives it.
pub trait TimetableWorld {
    /// The game's folder.
    fn root(&self) -> &Path;
    /// The map's folder.
    fn map_dir(&self) -> &Path;
    /// The chrono folders in force on the map's date.
    fn chrono_dirs(&self) -> Vec<PathBuf>;
    /// The map's `ailists.cfg`.
    fn ailists(&self) -> &omsi_map::AiLists;
    /// The tiles of `global.cfg` in the order the timetable's tile indices count them.
    fn raw_tiles(&self) -> Vec<(i32, i32)>;
    /// The map has tile `key` (listed in global.cfg, and its file exists).
    fn has_tile(&self, key: (i32, i32)) -> bool;
    /// The map's date (yyyymmdd).
    fn date(&self) -> i32;
    /// World position and rotation of every loaded map object by id.
    fn object_positions(&self) -> MutexGuard<'_, HashMap<i64, (DVec3, [f64; 3])>>;
    /// The side stop object `id`'s platform lies on: 0 = right, 1 = the other, 2 = both.
    fn stop_side(&self, id: i64) -> f32;
    /// Where object `id` placed on tile `tile` (its index in global.cfg's `[map]` list, as a
    /// timetable's station names it) stands: on a map whose ids repeat across tiles - one
    /// joined from several - the one of that tile, else the map's object of that id (here:
    /// the latter; omsi-app's `World` knows the objects of each tile).
    fn object_of_tile(&self, tile: Option<usize>, id: i64) -> Option<(DVec3, [f64; 3])> {
        let _ = tile;
        self.object_positions().get(&id).copied()
    }
}

struct Departure {
    /// Seconds since midnight.
    time: f64,
    trip: usize,
    /// The trip's profile the tour runs it with (`[addtrip]`).
    profile: usize,
    line: String,
    ai_group: String,
    tour: String,
    /// The tour's validity mask (bits 0-6 Monday..Sunday, 7 public holiday, 8 school
    /// holidays, 9 school days): every tour's departures are kept, and which run is decided
    /// by the day (`Schedule::runs`), so a session carries on past midnight.
    mask: i32,
    spawned: bool,
}

/// What became of a departure that was due.
pub enum Placed {
    Spawned,
    /// The part of the route the bus is on now is not loaded: try again later.
    Wait,
    /// Nothing to do any more (the trip is over, or has no route or no vehicle).
    Drop,
    /// A car stands where the bus would appear: try again at the next call.
    Busy,
}

/// A scheduled bus whose route stops short of a tile that has not brought its lanes yet: the
/// route is carried on as the tiles come.
struct RunningTrip {
    car: u64,
    steps: Vec<Step>,
    /// The first step its route does not have yet.
    next: usize,
    /// The trip's stations with their departure times, and which of them the bus stops at
    /// already or has passed.
    stations: Vec<(i64, f64)>,
    served: Vec<bool>,
    /// Authored track entry of each type-1 station, retained while tiles stream in.
    station_steps: Vec<Option<usize>>,
    /// The tile each station names (`trip_station_tiles`): where ids repeat across tiles,
    /// the stop is the object of that one.
    station_tiles: Vec<Option<usize>>,
}

/// How long before a trip is due at a stop the people for it turn up there (s).
pub const PAX_SPAWN_AHEAD: f64 = 15.0 * 60.0;

/// How far from a route a bus stop may stand when the route is only a part of the trip (a
/// stop of the missing part would otherwise be put on the nearest point of this one).
const STOP_REACH: f64 = 25.0;

/// The vehicle a departure is driven with. A tour keeps its bus all day, as in OMSI: the
/// choice comes from the tour, not from the order the departures happen to spawn in, so the
/// timetable knows which vehicles its next minutes need before they are due.
pub struct Choice {
    pub ty: Arc<VehicleType>,
    pub number: Option<(String, String)>,
    pub hof: Option<Arc<omsi_vehicle::Hof>>,
    pub scheme: Option<usize>,
    /// A `.zug` train: its cars, the first one being `ty`.
    pub train: Option<Vec<(Arc<VehicleType>, bool)>>,
}

/// A train's cars: (car type, reversed), the first car leads.
pub type TrainCars = Vec<(Arc<VehicleType>, bool)>;

/// A depot's vehicle: its type, its fleet from the ailists, its depot file.
pub type DepotVehicle = (Arc<VehicleType>, Vec<omsi_map::DepotEntry>, Option<Arc<omsi_vehicle::Hof>>);

pub struct ScheduleSim {
    pub data: TimetableData,
    departures: Vec<Departure>,
    /// Depot vehicles per AI group: (type, its fleet from the ailists, depot file).
    depots: HashMap<String, Vec<DepotVehicle>>,
    /// The player's own lines that ask for buses of a kind, a maker or a model (the line
    /// editor's `lines::VEHICLES_FILE`): per line (its name, lower case) the vehicles its
    /// trips are driven with, in the shape of `depots` (omsi-app's `Schedule::new` works
    /// them out: `set_line_buses`).
    line_buses: HashMap<String, Vec<DepotVehicle>>,
    tile_coords: Vec<(i32, i32)>,
    /// Trains per AI group: list of (car type, reversed), first car leads.
    trains: HashMap<String, Vec<TrainCars>>,
    /// Plain `[aigroup_2]` vehicle pools, loaded the first time a trip asks for one
    /// (the Tegel approaches are flown by the group's own aircraft, not by depot buses).
    pools: HashMap<String, Vec<Arc<VehicleType>>>,
    /// Per vehicle folder: the depot file a bus of a plain `[aigroup_2]` runs with
    /// (see [`pool_depot`]), looked up the first time such a bus is chosen.
    pool_hofs: HashMap<PathBuf, Option<Arc<omsi_vehicle::Hof>>>,
    /// Departures that are due but not on the road yet. Putting twenty minutes of a Berlin
    /// timetable on the map at once costs several seconds in one frame, so they are spawned
    /// a few at a time.
    pending: std::collections::VecDeque<usize>,
    /// Per departure: the previous departure of the same tour (its bus is the same one).
    tour_prev: Vec<Option<usize>>,
    /// Due departures whose bus would be on a part of its route that is not loaded: tried
    /// again when tiles bring lanes and as time moves the bus on.
    waiting: Vec<usize>,
    /// Buses on the road whose route is still to be carried on.
    running: Vec<RunningTrip>,
    /// `Traffic::lanes_generation` when the waiting departures and the running routes were
    /// last looked at, and the time of day of the last retry.
    seen_generation: u64,
    last_retry: f64,
    /// The departure each timetable bus on the road runs (by car id): a bus the traffic took
    /// off with its unloaded tile goes back to `waiting` and returns with the tiles, at the
    /// place its timetable puts it then.
    car_departure: HashMap<u64, usize>,
    /// Waiting departures whose vehicle is timed to reach loaded lanes at this time of day:
    /// they are tried again then, so that a plane coming in over tiles nobody loads appears
    /// where its path enters the loaded ones, not up to half a minute later in mid-air.
    retry_at: HashMap<usize, f64>,
    /// Per trip and profile: when its bus is at its stations.
    times: Vec<Vec<TripTimes>>,
    /// Per bus stop (map object id): the trips that call there, as (trip, station index).
    visits: HashMap<i64, Vec<(usize, usize)>>,
    /// Per trip: today's departures that run it.
    trip_departures: Vec<Vec<usize>>,
    /// When the departure boards were last made (time of day).
    boards_made: f64,
    /// The tour the player drives (line, tour): the timetable does not run it as well.
    player_tour: Option<(String, String)>,
    /// With a single trip picked: the departure (s of the day) of that trip; the rest of
    /// the tour stays the AI's.
    player_departure: Option<f64>,
    /// A duty of trips from several tours (`player_plan`): those trips (line, tour, departure
    /// in s of the day) are the player's, the rest of their tours stays the AI's.
    player_trips: Vec<(String, String, f64)>,
    /// A player tour was just taken over: its buses already on the road go at the next tick.
    purge_player_tour: bool,
    /// LAN play (host): the tours the other players drive (line, tour; lower case), left to
    /// them like our own.
    lan_tours: HashSet<(String, String)>,
    /// Time of today's timetable at the last tick (s).
    last_tod: f64,
    /// The lines the date's chrono folders take off the timetable, with the folder that does
    /// it: why a duty on such a line cannot be driven.
    deactivated: Vec<(String, PathBuf)>,
    /// Stations some trip stops at on its way or ends at (not only starts from).
    served: std::collections::HashSet<i64>,
    /// Per first station: whether another trip's bus stops there or within a bus length
    /// or two of it (maps often put one stop object per line at the same kerb), once its
    /// position is known.
    shared_stand: HashMap<i64, bool>,
    /// Departures queued while the map loads: their buses may appear in view.
    startup: std::collections::HashSet<usize>,
    /// Layover departures whose stand was taken: they come at their departure time.
    later_layover: std::collections::HashSet<usize>,
    /// Per departure: the next departure of the same tour (its bus takes it on).
    tour_next: Vec<Option<usize>>,
    /// Departures due while their tour's bus is still on its previous trip: that bus takes
    /// them on when it gets there, as in OMSI a tour keeps its bus from trip to trip.
    awaiting: std::collections::HashSet<usize>,
    /// The clock was set (`restart`): the next tick puts the buses out as a start does.
    restarted: bool,
    /// The map's holidays, for the day's tours.
    calendar: omsi_map::Calendar,
    /// The date (yyyymmdd) the departures are for, and the mask bits it selects (day,
    /// school); `set_day` moves them on at midnight.
    day: i32,
    day_bits: (i32, i32),
    /// The weekday bit of the next day (night tours run on into it).
    next_day_bit: i32,
    /// Where today's midnight lies on the traffic's clock (`Traffic::day_time` counts on past
    /// 24:00): a departure leaves at `day_base + time` (`dep_time`), and the date moves on
    /// when the clock passes the next midnight.
    day_base: f64,
    /// The current date (its time of day is not used).
    date_clock: crate::SimClock,
    /// The map's `car_use/*.ocu`: which vehicles serve which line's tours.
    car_use: Vec<omsi_timetable::CarUse>,
    /// Per tour (`tour_key_of`): the depot vehicle (index in its group) and fleet number
    /// it runs with today - from `car_use`, else drawn the first time the tour is due. A
    /// number is given to one tour only (`used_numbers`), as in OMSI:
    /// hashing each tour to a number put the same fleet number on two buses at once.
    tour_vehicle: HashMap<u64, (usize, usize)>,
    used_numbers: HashSet<(String, String)>,
    /// The player's bus company on this map and day (omsi-app's `company_live`, see
    /// [`TimetableCompany`]): its tours run with its own buses, and what they drive is
    /// reported to it. None: a drive of the player's own reads nothing of a company.
    company: Option<Box<dyn TimetableCompany>>,
    /// Per departure: kept off the road for the player's bus company
    /// (`TimetableCompany::keeps_off`), worked out with the day's plan.
    company_off: Vec<bool>,
    /// Mixed into the draw of a tour's vehicle, so that a tour gets another of its depot's
    /// buses from session to session, as in OMSI (the session's seed; a LAN game's shared
    /// one, `set_lan_seed`). Hashing the tour alone sent the same bus out on it every time
    /// (#2250). 0 (tests, tools): always the same.
    pub vehicle_seed: u64,
}

/// A tour's bus waits at the end of a trip for the next one of its tour when that leaves
/// within this many seconds; for a longer break it goes (and a bus comes back for it).
const TOUR_LAYOVER_MAX: f64 = 30.0 * 60.0;

/// How early a bus waits at its first stop for its departure (s): a quarter of an hour at a
/// stand of its own, a minute where other buses stop as well (a layover bus there made
/// every bus of the other lines queue behind it until it left).
const LAYOVER: f64 = 900.0;

const LAYOVER_SHARED: f64 = 60.0;

/// How far ahead (s) the departure displays look.
const BOARD_AHEAD: f64 = 2.0 * 3600.0;

/// Most departures a page gets for a stop (`omsi.getDepartures`).
const MAX_PAGE_DEPARTURES: usize = 20;
