//! The player's bus company on the map (omsi-app's `company_live`, the launcher's company
//! planning): in a drive for it (`--company`) its tours run with its own buses, a tour it
//! dropped today does not run, the map's own buses stay in their depots, and each trip a
//! company bus drives to its end is reported to it once.

use super::*;

/// What the timetable asks of the player's bus company on this map and day. omsi-app's
/// `CompanyLive` gives it (the plan the launcher writes for the game, and the company's
/// books the trips are reported to).
pub trait TimetableCompany: Send + Sync {
    /// The company has a plan for this map and day.
    fn active(&self) -> bool;
    /// A drive from the company's planning: only its buses run, plan for the day or not.
    fn own_only(&self) -> bool;
    /// The day moved on (midnight; `yyyymmdd`): the plan of the new day, if there is one.
    fn set_day(&mut self, date_code: i32);
    /// Whether the timetable keeps a tour off the road: one the company dropped today, and in
    /// a drive for the company every tour of the map's own buses (`map_bus`) that is not the
    /// company's. Trains and planes run as ever.
    fn keeps_off(&self, line: &str, tour: &str, map_bus: bool) -> bool;
    /// Whether the tour is one of the company's today.
    fn runs_tour(&self, line: &str, tour: &str) -> bool;
    /// A trip of the company its bus drove to the end (departure `index`, of `line`/`tour`,
    /// left at minute `dep`), with its kilometres, its stops and how late it was: reported to
    /// the company once.
    fn report(&mut self, index: usize, line: &str, tour: &str, dep: i32, km: f64, stops: usize, delay: f64);
    /// The company's own bus for its tour `line`/`tour` (`root`: the game's folder its bus
    /// files are found in). None: not the company's tour, or a rental bus - the map's own
    /// vehicle then.
    fn bus(&mut self, root: &Path, line: &str, tour: &str) -> Option<CompanyBus>;
}

/// A bus of the player's company for one of its tours (`TimetableCompany::bus`).
pub struct CompanyBus {
    pub ty: Arc<VehicleType>,
    /// Its fleet number and plate.
    pub number: (String, String),
    /// The depot file it carries (the line's own, else the company's); None when neither is
    /// named or found - the depot group's then.
    pub hof: Option<Arc<omsi_vehicle::Hof>>,
    /// Its paint (None: the model's own).
    pub scheme: Option<usize>,
}

/// Whether a departure is one of the map's own buses - what a drive for the player's bus
/// company leaves in the depot: not a train (a `.zug` group), nothing on a track or without
/// stops (trains, planes), nor a depot of rail vehicles only (trams).
fn is_map_bus(train: bool, track: bool, stops: bool, all_rail: bool) -> bool {
    !train && !track && stops && !all_rail
}

impl ScheduleSim {
    /// A drive for the player's bus company (omsi-app's `--company`): its plan of the day,
    /// and the map's own buses kept in their depots. Called before `precache`.
    pub fn set_company(&mut self, company: Box<dyn TimetableCompany>) {
        self.company = Some(company);
        self.mark_company();
    }

    /// The date (yyyymmdd) the departures are for.
    pub fn day(&self) -> i32 {
        self.day
    }

    /// Which departures the company keeps off the road today (`TimetableCompany::keeps_off`).
    pub(super) fn mark_company(&mut self) {
        let Some(company) = self.company.as_deref().filter(|c| c.own_only() || c.active()) else {
            self.company_off.clear();
            return;
        };
        let mut track: HashMap<usize, bool> = HashMap::new();
        let mut off = Vec::with_capacity(self.departures.len());
        for d in &self.departures {
            let on_track = *track.entry(d.trip).or_insert_with(|| self.is_track(&self.data.trips[d.trip].name));
            let group = d.ai_group.to_ascii_lowercase();
            let train = self.trains.contains_key(&group);
            let stops = !trip_stations(&self.data.trips[d.trip]).is_empty();
            let all_rail = self
                .line_buses
                .get(&d.line.to_ascii_lowercase())
                .or_else(|| self.depots.get(&group))
                .is_some_and(|v| !v.is_empty() && v.iter().all(|x| x.0.def.is_rail()));
            off.push(company.keeps_off(&d.line, &d.tour, is_map_bus(train, on_track, stops, all_rail)));
        }
        self.company_off = off;
        let kept = (0..self.departures.len()).filter(|&i| self.company_off[i] && {
            let m = self.departures[i].mask;
            m & self.day_bits.0 != 0 && m & self.day_bits.1 != 0
        }).count();
        log::info!("timetable: {kept} of today's departures stay in their depots for the company");
    }

    /// Departure `i` is kept off the road for the player's bus company - never the player's
    /// own trips, nor another LAN player's.
    pub(super) fn kept_off(&self, i: usize) -> bool {
        self.company_off.get(i).copied().unwrap_or(false) && !self.is_player_tour(i)
    }

    /// Whether trip `name` runs a track (`.ttr`: trains, planes), as `steps_of` finds it.
    fn is_track(&self, name: &str) -> bool {
        let track_name = self.data.trip(name).map(|t| t.display_name.trim()).filter(|n| !n.is_empty()).unwrap_or(name);
        self.data.tracks.iter().any(|t| t.path.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(track_name)))
    }

    /// Timetable bus `car` drove its trip to the end, `delay` seconds late: a trip of the
    /// player's company is told to the company (`report_company_trip`).
    pub fn car_trip_done(&mut self, car: u64, delay: f64) {
        if let Some(&k) = self.car_departure.get(&car) {
            self.report_company_trip(k, delay);
        }
    }

    /// A trip of the player's company that its bus drove to the end (departure `k`): told to
    /// the company once, with its kilometres and how late it was.
    fn report_company_trip(&mut self, k: usize, delay: f64) {
        let d = &self.departures[k];
        if !self.company.as_deref().is_some_and(|c| c.active() && c.runs_tour(&d.line, &d.tour)) {
            return;
        }
        let trip = &self.data.trips[d.trip];
        let stations = trip_stations(trip);
        let (steps, _) = self.steps_of(&trip.name, &stations);
        let km = steps.iter().map(|s| s.length).sum::<f64>() / 1000.0;
        let (line, tour, dep) = (d.line.clone(), d.tour.clone(), (d.time / 60.0).round() as i32);
        if let Some(c) = self.company.as_deref_mut() {
            c.report(k, &line, &tour, dep, km, stations.len(), delay);
        }
    }

    /// The vehicle of a tour the player's company runs: its own bus, in its paint, with its
    /// fleet number and plate, carrying the company's depot (None: not the company's tour,
    /// or a rental bus - the map's own vehicle then).
    pub(super) fn company_choice(&mut self, i: usize, world: &dyn TimetableWorld) -> Option<Choice> {
        let company = self.company.as_deref_mut().filter(|c| c.active())?;
        let d = &self.departures[i];
        let bus = company.bus(world.root(), &d.line, &d.tour)?;
        let group = d.ai_group.to_ascii_lowercase();
        // (the line's own depot file, else the company's, else the depot group's)
        let hof = bus.hof.or_else(|| self.depots.get(&group).and_then(|v| v.first()).and_then(|x| x.2.clone()));
        Some(Choice { ty: bus.ty, number: Some(bus.number), hof, scheme: bus.scheme, train: None })
    }
}
