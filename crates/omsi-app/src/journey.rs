//! The journey's timetable (#800, #1061): the stops of the duty's trips with their planned
//! and actual arrival and departure and how far off those were, as the logbooks of the
//! virtual bus companies keep them. It is written to the content folder's `Journeys` every
//! time the bus reaches or leaves a stop, so it is there however the session ends.

use crate::career::{EARLY_DEPARTURE, LATE_ARRIVAL};
use crate::schedule::{PlannedTrip, PlayerDuty};
use std::path::PathBuf;

/// The time to open the doors before the summary takes the mouse (s).
const SUMMARY_DELAY: f32 = 5.0;

/// One stop of a trip in the log.
struct Row {
    trip: usize,
    stop: usize,
    name: String,
    /// Planned arrival and departure, and the actual ones (s of the duty's day).
    plan: (f64, f64),
    arrived: Option<f64>,
    left: Option<f64>,
    /// The trip begins here (no arrival to tell) / ends here (no departure).
    first: bool,
    last: bool,
    /// The bus's odometer when it came to the stop (km).
    odo: Option<f64>,
}

impl Row {
    fn served(&self) -> bool {
        self.arrived.is_some() || self.left.is_some()
    }

    fn late_early(&self) -> (bool, bool) {
        let late = !self.first && self.arrived.is_some_and(|a| (a - self.plan.0).round() > LATE_ARRIVAL);
        let early = !self.last && self.left.is_some_and(|l| (l - self.plan.1).round() < EARLY_DEPARTURE);
        (late, early)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TripRating {
    pub driving: f64,
    pub comfort: f64,
    pub tickets: f64,
    pub passengers: i32,
    pub tickets_asked: i32,
}

impl TripRating {
    pub(crate) fn comfort_rated(&self) -> Option<f64> {
        (self.passengers > 0).then_some(self.comfort)
    }

    pub(crate) fn tickets_rated(&self) -> Option<f64> {
        (self.tickets_asked > 0).then_some(self.tickets)
    }
}

/// A trip's summary as it ended (`TripSummary::text`, the panel's table): each stop served
/// with its time planned and as driven, how far off that was and the odometer there - the
/// first stop by its departure, the others by their arrival.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TripSummary {
    /// "Trip 2  |  5 to Muzealna  |  08:20:00" and the log's head line.
    pub title: String,
    pub head: String,
    /// Stop, planned, actual, difference, odometer (km); blank where it was passed by.
    pub rows: Vec<[String; 5]>,
    /// Kilometres from the first stop to the last, and the stops served of all.
    pub km: Option<f64>,
    pub served: (usize, usize),
    pub on_time: usize,
    pub driven: Option<f64>,
    pub rating: Option<TripRating>,
    /// What the file is to be called (the log's own name and the trip's number).
    pub stem: String,
    pub dir: PathBuf,
}

impl TripSummary {
    /// The table as text, in columns (copied, or saved to the Journeys folder).
    pub(crate) fn text(&self) -> String {
        let head = ["Stop", "Plan", "Actual", "Diff", "Odometer"];
        let width = |k: usize| self.rows.iter().map(|r| r[k].chars().count()).chain([head[k].chars().count()]).max().unwrap_or(0);
        let w: Vec<usize> = (0..5).map(width).collect();
        let line = |cells: [&str; 5]| -> String {
            let mut out = format!("{:<w0$}", cells[0], w0 = w[0]);
            for k in 1..5 {
                out.push_str(&format!("  {:>wk$}", cells[k], wk = w[k]));
            }
            out.trim_end().to_string()
        };
        let mut out = format!("{}\n{}\n\n{}\n", self.head, self.title, line(head));
        out.push_str(&"-".repeat(w.iter().sum::<usize>() + 8));
        out.push('\n');
        for r in &self.rows {
            out.push_str(&line([&r[0], &r[1], &r[2], &r[3], &r[4]]));
            out.push('\n');
        }
        out.push_str(&format!("\nStops served: {} of {}", self.served.0, self.served.1));
        if let Some(km) = self.km {
            out.push_str(&format!(", {km:.1} km"));
        }
        out.push_str(&format!("\nOn time: {} of {}", self.on_time, self.served.0));
        if let Some(d) = self.driven {
            out.push_str(&format!("\nTime driven: {}", hms(d)));
        }
        if let Some(r) = self.rating {
            let percent = |p: Option<f64>| p.map(|p| format!("{p:.0} %")).unwrap_or_else(|| "-".into());
            out.push_str(&format!("\nPassengers: {}, driving {:.0} %, comfort {}, ticket selling {}", r.passengers, r.driving, percent(r.comfort_rated()), percent(r.tickets_rated())));
        }
        out.push_str(&format!("\nRating: {} of 5\n", self.stars()));
        out
    }

    pub(crate) fn score(&self) -> f64 {
        let mut parts = Vec::new();
        if self.served.1 > 0 {
            parts.push(100.0 * self.on_time as f64 / self.served.1 as f64);
        }
        if let Some(r) = self.rating {
            parts.push(r.driving);
            parts.extend(r.comfort_rated());
            parts.extend(r.tickets_rated());
        }
        if parts.is_empty() {
            return 0.0;
        }
        parts.iter().sum::<f64>() / parts.len() as f64
    }

    pub(crate) fn stars(&self) -> u8 {
        match self.score() {
            s if s >= 90.0 => 5,
            s if s >= 75.0 => 4,
            s if s >= 60.0 => 3,
            s if s >= 40.0 => 2,
            _ => 1,
        }
    }

    pub(crate) fn mood(&self) -> Option<&'static str> {
        let r = self.rating.filter(|r| r.passengers > 0)?;
        Some(match (r.driving + r.comfort) / 2.0 {
            m if m >= 90.0 => "Your passengers enjoyed the ride",
            m if m >= 70.0 => "Your passengers were satisfied",
            m if m >= 50.0 => "Your passengers were not very happy",
            _ => "Your passengers were unhappy",
        })
    }

    /// The summary written to the Journeys folder; the file it went to.
    pub(crate) fn save(&self) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let path = (1..100)
            .map(|n| self.dir.join(if n == 1 { format!("{}.txt", self.stem) } else { format!("{} ({n}).txt", self.stem) }))
            .find(|p| !p.exists())
            .unwrap_or_else(|| self.dir.join(format!("{}.txt", self.stem)));
        std::fs::write(&path, self.text())?;
        Ok(path)
    }
}

pub(crate) struct Journey {
    /// The duty it is the log of: line, tour and where its trips begin in the tour.
    key: (String, String, usize),
    /// The lines above the trips (driver, map, bus, date; line and tour).
    head: String,
    /// Each trip's title.
    trips: Vec<String>,
    rows: Vec<Row>,
    dir: PathBuf,
    /// The file, chosen when there is first something to write.
    path: Option<PathBuf>,
    /// The summary of a trip that has just ended, until the game shows it, and the last.
    ready: Option<TripSummary>,
    last: Option<TripSummary>,
    still: f32,
}

impl Journey {
    /// The log of the duty `key` (line, tour, where its `trips` begin in the tour), nothing
    /// driven yet; `head` comes first in it.
    fn new(key: (String, String, usize), trips_planned: &[PlannedTrip], head: &str, dir: PathBuf) -> Journey {
        let mut trips = Vec::new();
        let mut rows = Vec::new();
        for (t, trip) in trips_planned.iter().enumerate() {
            let line = if trip.line.trim().is_empty() { key.0.trim() } else { trip.line.trim() };
            trips.push(format!("Trip {}  |  {} to {}  |  {}", key.2 + t + 1, line, trip.terminus.trim(), hms(trip.departure)));
            let n = trip.stops.len();
            // (the stations a depot run passes are not stops)
            for (k, s) in trip.stops.iter().enumerate().filter(|(_, s)| s.stops) {
                rows.push(Row { trip: t, stop: k, name: s.name.trim().to_string(), plan: (s.arr, s.dep), arrived: None, left: None, first: k == 0, last: k + 1 == n, odo: None });
            }
        }
        let line = format!("Line {}, tour {}", key.0.trim(), key.1.trim());
        let head = format!("{}\n{line}", head.trim_end());
        Journey { key, head, trips, rows, dir, path: None, ready: None, last: None, still: 0.0 }
    }

    /// The odometer (km) at the stop the bus has just come to or left, if it has none yet.
    fn odometer(&mut self, at: (usize, usize), km: f64) {
        if let Some(r) = self.row(at) {
            r.odo.get_or_insert(km);
        }
    }

    /// Trip `t` (its index in the duty's trips) has ended: its summary is made.
    pub(crate) fn trip_ended(&mut self, t: usize, rating: Option<TripRating>) {
        let rows: Vec<&Row> = self.rows.iter().filter(|r| r.trip == t).collect();
        if rows.iter().all(|r| r.arrived.is_none() && r.left.is_none()) {
            return;
        }
        let cells = rows
            .iter()
            .map(|r| {
                let (plan, real) = if r.first { (r.plan.1, r.left) } else { (r.plan.0, r.arrived) };
                let [p, a, d] = times(plan, real);
                [r.name.clone(), p, a, d, r.odo.map(|k| format!("{k:.1} km")).unwrap_or_default()]
            })
            .collect();
        let served = rows.iter().filter(|r| r.served()).count();
        let on_time = rows.iter().filter(|r| r.served() && r.late_early() == (false, false)).count();
        let start = rows.iter().find_map(|r| r.left.or(r.arrived));
        let end = rows.iter().rev().find_map(|r| r.arrived.or(r.left));
        let driven = start.zip(end).map(|(a, b)| b - a).filter(|d| *d > 0.0);
        let odo: Vec<f64> = rows.iter().filter_map(|r| r.odo).collect();
        let km = (odo.len() >= 2).then(|| odo[odo.len() - 1] - odo[0]);
        let stem = match self.path.as_ref().and_then(|p| p.file_stem()) {
            Some(s) => format!("{} trip {}", s.to_string_lossy(), self.key.2 + t + 1),
            None => format!("line {} tour {} trip {}", self.key.0.trim(), self.key.1.trim(), self.key.2 + t + 1),
        };
        let s = TripSummary { title: self.trips.get(t).cloned().unwrap_or_default(), head: self.head.clone(), rows: cells, km, served: (served, rows.len()), on_time, driven, rating, stem, dir: self.dir.clone() };
        self.ready = Some(s.clone());
        self.last = Some(s);
    }

    /// The summary of the trip that has just ended, once.
    pub(crate) fn take_summary(&mut self) -> Option<TripSummary> {
        self.ready.take()
    }

    /// Keep the automatic summary pending while driving: opening it takes the mouse
    /// away from the bus. Use the timetable's standstill threshold (0.1 m/s).
    pub(crate) fn take_summary_when_stopped(&mut self, speed_kmh: f32, dt: f32) -> Option<TripSummary> {
        self.still = if speed_kmh.abs() < 0.36 { self.still + dt } else { 0.0 };
        if self.still >= SUMMARY_DELAY {
            self.take_summary()
        } else {
            None
        }
    }

    /// The summary of the last trip that ended.
    pub(crate) fn last_summary(&self) -> Option<TripSummary> {
        self.last.clone()
    }

    /// The bus arrived at stop `at` (trip, stop) `late` s after its time (negative: early).
    /// True when that is news.
    fn arrived(&mut self, at: (usize, usize), late: f64) -> bool {
        match self.row(at) {
            Some(r) if r.arrived.is_none() => {
                r.arrived = Some(r.plan.0 + late);
                true
            }
            _ => false,
        }
    }

    /// The bus left stop `at`, having arrived and left `late` s after its times.
    fn left(&mut self, at: (usize, usize), late: (f64, f64)) -> bool {
        let Some(r) = self.row(at) else { return false };
        r.arrived.get_or_insert(r.plan.0 + late.0);
        r.left = Some(r.plan.1 + late.1);
        true
    }

    fn row(&mut self, (trip, stop): (usize, usize)) -> Option<&mut Row> {
        self.rows.iter_mut().find(|r| r.trip == trip && r.stop == stop)
    }

    /// The log as it is written.
    fn text(&self) -> String {
        let rule = "-".repeat(108);
        let mut out = format!("openOMSI journey log\n{}\n", self.head);
        let (mut served, mut late, mut early) = (0, 0, 0);
        for (t, title) in self.trips.iter().enumerate() {
            let rows: Vec<&Row> = self.rows.iter().filter(|r| r.trip == t).collect();
            // (a trip is in it once the bus has reached one of its stops: a tour has dozens)
            if rows.iter().all(|r| r.arrived.is_none() && r.left.is_none()) {
                continue;
            }
            out.push_str(&format!("\n{title}\n{rule}\n{:<30}  {:>9} {:>9} {:>10}   {:>9} {:>9} {:>10}   Status\n{rule}\n", "Stop", "Arr. plan", "Arr. real", "Diff", "Dep. plan", "Dep. real", "Diff"));
            for (i, r) in rows.iter().enumerate() {
                let arr = if r.first { [String::new(), String::new(), String::new()] } else { times(r.plan.0, r.arrived) };
                let dep = if r.last { [String::new(), String::new(), String::new()] } else { times(r.plan.1, r.left) };
                let (is_late, is_early) = r.late_early();
                let status = if r.served() {
                    served += 1;
                    late += is_late as usize;
                    early += is_early as usize;
                    match (is_late, is_early) {
                        (true, true) => "late, left early",
                        (true, false) => "late",
                        (false, true) => "left early",
                        _ => "OK",
                    }
                } else if rows[i + 1..].iter().any(|x| x.arrived.is_some() || x.left.is_some()) {
                    "missed"
                } else {
                    ""
                };
                let name: String = r.name.chars().take(30).collect();
                out.push_str(format!("{name:<30}  {:>9} {:>9} {:>10}   {:>9} {:>9} {:>10}   {status}", arr[0], arr[1], arr[2], dep[0], dep[1], dep[2]).trim_end());
                out.push('\n');
            }
        }
        out.push_str(&format!("{rule}\nStops served: {served}, late: {late}, left early: {early}\n"));
        out
    }

    /// Write the log (its file is named by the real date and time it began, and the line and tour).
    fn write(&mut self) {
        if self.path.is_none() {
            let (y, mo, d, h, mi) = omsi_launcher_lib::local_now().unwrap_or((1970, 1, 1, 0, 0));
            let clean = |s: &str| s.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect::<String>();
            let stem = format!("{y:04}-{mo:02}-{d:02} {h:02}-{mi:02} line {} tour {}", clean(&self.key.0), clean(&self.key.1));
            if let Err(e) = std::fs::create_dir_all(&self.dir) {
                log::warn!("journey log: {}: {e}", self.dir.display());
                return;
            }
            // (another session begun in the same minute keeps its own)
            let path = (1..100).map(|n| self.dir.join(if n == 1 { format!("{stem}.txt") } else { format!("{stem} ({n}).txt") })).find(|p| !p.exists());
            let Some(path) = path else { return };
            log::info!("journey log: {}", path.display());
            self.path = Some(path);
        }
        if let Some(p) = self.path.as_ref() {
            if let Err(e) = std::fs::write(p, self.text()) {
                log::warn!("journey log: {}: {e}", p.display());
            }
        }
    }
}

/// Keep `slot` the log of `duty` and note in it what the duty's update did, written at once
/// when that changed it: the bus left the stop it was due at before the update, `due`,
/// `served` seconds late (arrival, departure: what `PlayerDuty::update` returned), and
/// arrived at the one it is due at now (`PlayerDuty::arrived`). `head` (driver, map, bus,
/// date) is asked for when a log begins.
pub(crate) fn note(slot: &mut Option<Journey>, duty: &PlayerDuty, due: (usize, usize), served: Option<(f64, f64)>, odo_km: f64, root: &std::path::Path, head: impl FnOnce() -> String) {
    // (every frame: the key is compared, not made)
    if slot.as_ref().is_none_or(|j| (j.key.0.as_str(), j.key.1.as_str(), j.key.2) != (duty.line.as_str(), duty.tour.as_str(), duty.first_trip)) {
        let dir = crate::startup::content_dir().unwrap_or_else(|| root.to_path_buf()).join("Journeys");
        *slot = Some(Journey::new((duty.line.clone(), duty.tour.clone(), duty.first_trip), &duty.trips, &head(), dir));
    }
    let Some(j) = slot.as_mut() else { return };
    let left = served.is_some_and(|s| j.left(due, s));
    if left {
        j.odometer(due, odo_km);
    }
    let arrived = duty.arrived().is_some_and(|late| j.arrived((duty.trip_index, duty.next_stop), late));
    if arrived {
        j.odometer((duty.trip_index, duty.next_stop), odo_km);
    }
    if left || arrived {
        j.write();
    }
}

/// The log's first line: the driver, the map, the bus and the game's date.
pub(crate) fn head(career: &crate::career::Career, map: &str, bus: &omsi_sim::VehicleInstance, clock: &omsi_sim::SimClock) -> String {
    let driver = career.driver.as_ref().map(|d| d.name.clone()).or_else(|| career.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()));
    let (day, month) = clock.day_month();
    let bus = format!("{} {}", bus.ty.def.manufacturer.trim(), bus.ty.def.type_name.trim());
    let mut parts: Vec<String> = driver.map(|d| format!("Driver {}", d.trim())).into_iter().collect();
    parts.extend([map.trim().to_string(), bus.trim().to_string(), format!("{:04}-{month:02}-{day:02}", clock.year)]);
    parts.join("  |  ")
}

/// A time of day as HH:MM:SS (a duty's times may run past midnight or before it).
pub(crate) fn hms(t: f64) -> String {
    let s = t.round().rem_euclid(86400.0) as i64;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Planned, actual and the difference (+ late, − early), blank while it has not happened.
fn times(plan: f64, real: Option<f64>) -> [String; 3] {
    match real {
        Some(r) => {
            let d = (r - plan).round() as i64;
            let a = d.abs();
            [hms(plan), hms(r), format!("{}{:02}:{:02}:{:02}", if d < 0 { "-" } else { "+" }, a / 3600, a / 60 % 60, a % 60)]
        }
        None => [hms(plan), String::new(), String::new()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::PlannedStop;

    fn trip(stops: &[(&str, f64)]) -> PlannedTrip {
        PlannedTrip {
            name: "5_A".into(),
            line: "5".into(),
            terminus: "Muzealna".into(),
            departure: stops[0].1,
            end: stops[stops.len() - 1].1,
            stops: stops.iter().map(|(n, t)| PlannedStop { object_id: 0, name: n.to_string(), arr: *t, dep: *t, position: None, dir: Default::default(), stops: true }).collect(),
        }
    }

    #[test]
    fn automatic_summary_waits_for_standstill_and_appears_once() {
        let trips = [trip(&[("First", 0.0), ("Last", 60.0)])];
        let mut j = Journey::new(("5".into(), "1".into(), 0), &trips, "Test", PathBuf::new());
        j.arrived((0, 1), 0.0);
        j.trip_ended(0, None);
        let expected = j.last_summary().expect("the completed trip is already recorded");

        for speed in [20.0, -20.0, 1.0, -1.0, 0.36, -0.36] {
            assert!(j.take_summary_when_stopped(speed, 10.0).is_none(), "still moving at {speed} km/h");
        }
        assert_eq!(j.take_summary_when_stopped(0.0, 10.0), Some(expected.clone()));
        assert!(j.take_summary_when_stopped(0.0, 10.0).is_none(), "closing it must not reopen it");
        assert_eq!(j.last_summary(), Some(expected), "statistics remain available in the quick menu");
    }

    #[test]
    fn automatic_summary_tolerates_standstill_physics_jitter() {
        let trips = [trip(&[("First", 0.0), ("Last", 60.0)])];
        for speed in [0.01, -0.01, 0.35, -0.35] {
            let mut j = Journey::new(("5".into(), "1".into(), 0), &trips, "Test", PathBuf::new());
            j.arrived((0, 1), 0.0);
            j.trip_ended(0, None);
            assert!(j.take_summary_when_stopped(speed, 0.0).is_none(), "the doors first");
            assert!(j.take_summary_when_stopped(speed, 5.0).is_some(), "stopped at {speed} km/h");
        }
    }

    #[test]
    fn the_summary_rates_the_trip_and_tells_how_the_passengers_felt() {
        let trips = [trip(&[("First", 0.0), ("Last", 60.0)])];
        let mut j = Journey::new(("5".into(), "1".into(), 0), &trips, "Test", PathBuf::new());
        j.left((0, 0), (0.0, 0.0));
        j.arrived((0, 1), 0.0);
        j.trip_ended(0, Some(TripRating { driving: 95.0, comfort: 100.0, tickets: 90.0, passengers: 12, tickets_asked: 8 }));
        let s = j.take_summary().expect("a summary");
        assert_eq!((s.served, s.on_time, s.driven), ((2, 2), 2, Some(60.0)));
        assert_eq!(s.stars(), 5);
        assert_eq!(s.mood(), Some("Your passengers enjoyed the ride"));
        let t = s.text();
        assert!(t.contains("On time: 2 of 2") && t.contains("Time driven: 00:01:00") && t.contains("Passengers: 12, driving 95 %, comfort 100 %, ticket selling 90 %") && t.contains("Rating: 5 of 5"), "{t}");

        let rough = TripSummary { on_time: 0, rating: Some(TripRating { driving: 30.0, comfort: 40.0, tickets: 100.0, passengers: 5, tickets_asked: 3 }), ..s.clone() };
        assert_eq!(rough.stars(), 2);
        assert_eq!(rough.mood(), Some("Your passengers were unhappy"));
        let middling = TripSummary { rating: Some(TripRating { driving: 70.0, comfort: 50.0, tickets: 100.0, passengers: 5, tickets_asked: 3 }), ..s.clone() };
        assert_eq!(middling.stars(), 4);
        assert_eq!(middling.mood(), Some("Your passengers were not very happy"));
        let nobody = TripSummary { rating: Some(TripRating { driving: 50.0, comfort: 100.0, tickets: 100.0, passengers: 0, tickets_asked: 0 }), ..s.clone() };
        assert_eq!(nobody.mood(), None);
        assert_eq!(nobody.stars(), 4, "comfort and tickets nobody rated do not count");
        assert!(nobody.text().contains("Passengers: 0, driving 50 %, comfort -, ticket selling -"), "{}", nobody.text());
        let skipped = TripSummary { served: (2, 10), on_time: 2, rating: None, ..s.clone() };
        assert_eq!(skipped.stars(), 1, "stops driven past are not on time");
        let unrated = TripSummary { rating: None, ..s };
        assert_eq!(unrated.stars(), 5);
        assert_eq!(unrated.mood(), None);
        assert!(!unrated.text().contains("Passengers:"));
    }

    #[test]
    fn automatic_summary_leaves_time_to_open_the_doors() {
        let trips = [trip(&[("First", 0.0), ("Last", 60.0)])];
        let mut j = Journey::new(("5".into(), "1".into(), 0), &trips, "Test", PathBuf::new());
        j.arrived((0, 1), 0.0);
        j.trip_ended(0, None);
        for _ in 0..299 {
            assert!(j.take_summary_when_stopped(0.0, 1.0 / 60.0).is_none(), "the doors are still opening");
        }
        assert!(j.take_summary_when_stopped(20.0, 1.0 / 60.0).is_none(), "moved off before it showed");
        for _ in 0..299 {
            assert!(j.take_summary_when_stopped(0.0, 1.0 / 60.0).is_none(), "the wait starts again at the next stop");
        }
        assert!(j.take_summary_when_stopped(0.0, 2.0 / 60.0).is_some(), "five seconds at a standstill");
    }

    /// A trip driven stop by stop: the first stop tells its departure only, the last its
    /// arrival only, a stop driven past is missed, a late arrival says so - and the file
    /// has it all at every stop.
    #[test]
    fn the_log_has_every_stop_with_its_times() {
        let t0 = 8.0 * 3600.0;
        let trips = [trip(&[("Metro Mlociny", t0), ("Zajezdnia", t0 + 300.0), ("Prozy", t0 + 600.0), ("Muzealna", t0 + 900.0)]), trip(&[("Muzealna", t0 + 1200.0), ("Metro Mlociny", t0 + 2100.0)])];
        let dir = std::env::temp_dir().join(format!("omsi-journey-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut j = Journey::new(("5".into(), "1".into(), 0), &trips, "Driver Test | Grundorf | MAN SD77 | 1989-05-30", dir.clone());
        assert!(j.arrived((0, 0), 20.0));
        assert!(!j.arrived((0, 0), 30.0), "the first arrival counts");
        assert!(j.left((0, 0), (20.0, 57.0)));
        j.write();
        let path = j.path.clone().expect("written");
        assert!(std::fs::read_to_string(&path).unwrap().contains("08:00:57"));
        // four minutes late at the second stop, past the third, early at the last
        assert!(j.arrived((0, 1), 240.0));
        assert!(j.left((0, 1), (240.0, 270.0)));
        assert!(j.arrived((0, 3), -30.0));
        j.write();
        let text = std::fs::read_to_string(&path).unwrap();
        let line = |name: &str| text.lines().find(|l| l.starts_with(name)).unwrap_or_else(|| panic!("no {name} in\n{text}")).to_string();
        assert!(text.contains("Line 5, tour 1") && text.contains("Driver Test") && text.contains("Trip 1  |  5 to Muzealna  |  08:00:00"), "{text}");
        assert!(line("Metro Mlociny").contains("08:00:00  08:00:57  +00:00:57"), "{text}");
        assert!(line("Metro Mlociny").ends_with("OK"), "{text}");
        assert!(line("Zajezdnia").contains("08:05:00  08:09:00  +00:04:00"), "{text}");
        assert!(line("Zajezdnia").contains("08:05:00  08:09:30  +00:04:30"), "{text}");
        assert!(line("Zajezdnia").ends_with("late"), "{text}");
        assert!(line("Prozy").ends_with("missed"), "{text}");
        assert!(line("Muzealna").contains("08:15:00  08:14:30  -00:00:30"), "{text}");
        assert!(!text.contains("Trip 2"), "a trip not begun is not in it yet: {text}");
        assert!(text.contains("Stops served: 3, late: 1, left early: 0"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
        // the trip's summary: every stop, by its arrival (the first by its departure), the
        // odometer where it was read
        j.odometer((0, 0), 1234.0);
        j.odometer((0, 1), 1235.25);
        j.odometer((0, 3), 1236.5);
        j.trip_ended(0, None);
        let s = j.take_summary().expect("a summary");
        assert!(j.take_summary().is_none() && j.last_summary().is_some());
        assert_eq!(s.rows.len(), 4);
        assert_eq!(s.rows[0], ["Metro Mlociny".to_string(), "08:00:00".into(), "08:00:57".into(), "+00:00:57".into(), "1234.0 km".into()]);
        assert_eq!(s.rows[2][2], "", "passed by");
        assert_eq!(s.served, (3, 4));
        assert_eq!(s.on_time, 2, "late at Zajezdnia");
        assert_eq!(s.driven, Some(813.0));
        assert_eq!(s.km, Some(2.5));
        let t = s.text();
        assert!(t.contains("Zajezdnia") && t.contains("+00:04:00") && t.contains("1235.2 km") && t.contains("Stops served: 3 of 4, 2.5 km"), "{t}");
        let saved = s.save().expect("saved");
        assert!(saved.starts_with(&dir) && std::fs::read_to_string(&saved).unwrap() == t);
        let _ = std::fs::remove_dir_all(&dir);
        // a duty taken up at its second trip: the first is not in it
        let mut j = Journey::new(("5".into(), "1".into(), 0), &trips, "Grundorf", dir.clone());
        assert!(j.arrived((1, 0), -60.0));
        let text = j.text();
        assert!(text.contains("Trip 2  |  5 to Muzealna  |  08:20:00") && !text.contains("Trip 1"), "{text}");
    }
}
