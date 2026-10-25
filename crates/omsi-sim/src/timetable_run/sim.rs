//! The timetable's state at run time (`ScheduleSim`): the day's departures, which of them are
//! due, queued, waiting for their tiles or on the road, the tours' vehicles, the routes on
//! the lanes and the departure boards. It decides which trip leaves when and with which
//! vehicle, and where on its route the bus is; omsi-app's `schedule` puts the buses on the
//! road with it (their vehicles uploaded, spawned, turned round) and hands it the map
//! through [`TimetableWorld`].

mod boards;
mod choice;
mod dispatch;
mod load;
mod place;
mod route;
mod take_on;
mod tours;
pub mod transit;

pub use tours::{TourPlan, TourPlanTrip};

use super::*;
use crate::ai_traffic::TrafficSim;
use crate::VehicleType;
use omsi_timetable::TimetableData;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use choice::{depot_file, pool_depot};
pub use dispatch::{tour_entry, TOUR_ENTRY_LANES};
pub use place::Placing;

/// What the timetable needs of the loaded map: its folders, the AI lists, the tiles, the
/// date and the places of the stops. The world snapshot (`crate::world_data::WorldData`)
/// gives it.
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
    fn object_positions(&self) -> &crate::world_data::ObjectPlaces;
    /// The side stop object `id`'s platform lies on: 0 = right, 1 = the other, 2 = both.
    fn stop_side(&self, id: i64) -> f32;
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
    /// A night tour's trip after 24:00 as the day after the tour began sees it: at its time
    /// less a day (24:30 = 00:30), run when yesterday's mask has the tour (`runs`). The
    /// trip's own departure (24:30) is tonight's, reached by the tour's bus before midnight.
    yesterday: bool,
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
    /// Departures queued while the map loads: their buses may appear in view.
    startup: std::collections::HashSet<usize>,
    /// Layover departures whose stand was taken, with when they try for it again (they
    /// come at their departure time at the latest).
    later_layover: HashMap<usize, f64>,
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
    /// The day bits of the day before (`yesterday_bits`): whose night tours run on today.
    yesterday_bits: (i32, i32),
    /// Per departure: for a night trip of yesterday's tour (`Departure::yesterday`), the
    /// same trip as tonight's (24:30), and the other way round.
    night_twin: Vec<Option<usize>>,
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
    /// Mixed into the draw of a tour's vehicle, so that a tour gets another of its depot's
    /// buses from session to session, as in OMSI (the session's seed; a LAN game's shared
    /// one, `set_lan_seed`). Hashing the tour alone sent the same bus out on it every time
    /// (#2250). 0 (tests, tools): always the same.
    pub vehicle_seed: u64,
    /// Trips made from transport routes whose track is not found yet (lower-case names): their
    /// departures wait for it.
    transit_unrouted: HashSet<String>,
}

/// A tour's bus waits at the end of a trip for the next one of its tour when that leaves
/// within this many seconds; for a longer break it goes (and a bus comes back for it). A
/// tour's bus missing on its layover (the map just loaded) stands at the next trip's
/// first stop as long before it (#2593).
const TOUR_LAYOVER_MAX: f64 = 30.0 * 60.0;

/// A layover bus whose stand was taken tries for it again after this many seconds.
const STAND_RETRY: f64 = 20.0;

/// How far ahead (s) the departure displays look.
const BOARD_AHEAD: f64 = 2.0 * 3600.0;

/// Most departures a page gets for a stop (`omsi.getDepartures`).
const MAX_PAGE_DEPARTURES: usize = 20;
