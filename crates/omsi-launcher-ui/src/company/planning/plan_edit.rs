//! A line's trips and timetable changed on the planning (Luc: "from the bus company's planning
//! menu you must be able to change the line's trips and timetable in an intuitive way"): a
//! sheet over the planning, opened from a line's heading or from one of its tours. A group of
//! days at a time, its departures lie per direction on a time axis - one dragged to another
//! minute, the axis clicked to add one there, the one chosen typed or taken out - and in a
//! list; the line's pattern is changed in one go (the first and the last departure, how
//! often, how long a bus stands at the end) as the line editor's timetable is. Under the
//! departures, on the same hours, the tours they make - the buses the line needs - against
//! those that run now: kept, changed, new, dropped; and what the line asks of the fleet.
//!
//! Saved as the line editor saves a timetable: an own line in service from a day the player
//! chooses, tomorrow at the soonest (`ownline::schedule_change`: today's tours run as they
//! are), else at once (`ownline::apply_change`) - the tours it changes get their buses and
//! drivers anew; the registry, the map's timetable and the depot files written, the planning
//! read again. A line of the map's timetable shows its trips as they run: its timetable is the
//! map's own.

use super::super::super::lineeditor;
use super::super::super::theme::*;
use super::super::super::ui::{id_of, ButtonKind, Key, Ui};
use super::super::super::Launcher;
use super::super::{act, day_label, kit, line_plate, map_lines, reload_timetable};
use super::plan_of;
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::network::TourOfDay;
use omsi_launcher_lib::company::plan::{BusOf, DayPlan};
use omsi_launcher_lib::company::{self as co, ownline, Company, CompanyLine};
use omsi_launcher_lib::lines::{self as reg, DayPattern, LineDesign, TableTrip, TimeBand, TourDiff, DAY_GROUPS};
use omsi_launcher_lib::{linehof, owndepot};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::path::{Path, PathBuf};

/// Why a line's trips are only shown: a line of the map's timetable, or an own line the line
/// editor's registry does not have.
const MAP_LINE: &str = "A line of the map's timetable: its trips are the map's own and are not changed here.";
pub(super) const NOT_FOUND: &str = "The line was not found among the line editor's lines of the company's map: its trips are shown as they run.";

/// A direction's lane on the axis, and the label column beside the lanes and the tours.
const LANE_H: f32 = 50.0;
const LABEL_W: f32 = 168.0;
/// A tour's row under the departures: as tall as the room the tours share allows, between
/// these (as the planning's chart is).
const TOUR_H: f32 = 22.0;
const TOUR_MAX: f32 = 40.0;

// --- times ------------------------------------------------------------------------------------------

/// Minutes as the timetable shows them ("07:05"; past midnight from 00:00 on).
fn hm(minutes: f32) -> String {
    let m = minutes.round() as i32;
    format!("{:02}:{:02}", m.div_euclid(60).rem_euclid(24), m.rem_euclid(60))
}

/// A time typed: "7:05", "07.05", "0705" or "705" (None: not a time).
fn parse_hm(s: &str) -> Option<i32> {
    let s = s.trim();
    let (h, m) = match s.split_once([':', '.', 'h']) {
        Some((h, m)) => (h.trim().parse::<i32>().ok()?, m.trim().parse::<i32>().ok()?),
        None if s.len() >= 3 && s.chars().all(|c| c.is_ascii_digit()) => (s[..s.len() - 2].parse().ok()?, s[s.len() - 2..].parse().ok()?),
        None => return None,
    };
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

/// A time being typed is whole, taken without Enter: four digits ("0705"), or two of minutes
/// after ':', '.' or 'h' ("7:05") - not "07:0", on its way to "07:05".
fn whole_time(s: &str) -> bool {
    let s = s.trim();
    match s.split_once([':', '.', 'h']) {
        Some((_, m)) => m.trim().len() == 2,
        None => s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()),
    }
}

/// A time typed (`m`, minutes of the day) for a departure at `before`: the day's or the next
/// day's, whichever is nearer - 00:05 for a trip of the late evening is after midnight.
fn typed_minutes(m: i32, before: f32) -> f32 {
    let m = m as f32;
    if (m + 1440.0 - before).abs() < (m - before).abs() {
        m + 1440.0
    } else {
        m
    }
}

// --- the line's departures, and changing them -----------------------------------------------------

/// A departure of the group of days shown: its direction (among the line's directions with
/// stops), when it leaves and arrives, and - the timetable a table - its trip in the table.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Dep {
    dir: usize,
    dep: f32,
    arr: f32,
    at: Option<usize>,
}

/// A departure chosen (or dragged): its direction and minute, and its trip in the table.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pick {
    dir: usize,
    dep: f32,
    at: Option<usize>,
}

impl Dep {
    fn pick(&self) -> Pick {
        Pick { dir: self.dir, dep: self.dep, at: self.at }
    }

    /// It is the departure `p` chose.
    fn is(&self, p: &Pick) -> bool {
        p.dir == self.dir
            && match (p.at, self.at) {
                (Some(a), Some(b)) => a == b,
                _ => (p.dep - self.dep).abs() < 0.5,
            }
    }
}

/// The directions with stops (two at the most): the lanes of the axis.
fn dirs(l: &LineDesign) -> usize {
    l.directions.iter().filter(|d| d.stops.len() >= 2).count().min(2)
}

/// Every departure of a group of days (`day`, an index into `DAY_GROUPS`) in order of the
/// minute it leaves: the table's trips, or the day pattern's in every direction.
fn deps(l: &LineDesign, day: usize) -> Vec<Dep> {
    let n = dirs(l);
    let mut out = Vec::new();
    if !l.service.allows(day) {
        return out;
    }
    if l.table_on {
        for (i, t) in l.table.iter().enumerate().filter(|(_, t)| t.dir < n && t.day == day && !t.times.is_empty()) {
            let times = reg::trip_times(l, t);
            let dep = times.first().copied().unwrap_or(0.0);
            out.push(Dep { dir: t.dir, dep, arr: times.last().copied().unwrap_or(dep).max(dep + 1.0), at: Some(i) });
        }
    } else if let Some(p) = l.days.get(day) {
        let run = reg::run_minutes(l);
        for dir in 0..n {
            for (dep, _) in p.departures() {
                out.push(Dep { dir, dep, arr: dep + run.get(dir).copied().unwrap_or(1.0), at: None });
            }
        }
    }
    out.sort_by(|a, b| a.dep.total_cmp(&b.dep).then(a.dir.cmp(&b.dir)));
    out
}

/// The timetable as a table of trips - the patterns' own, as the line editor's "As a table"
/// starts it: every departure can be changed on its own then.
fn as_table(l: &mut LineDesign) {
    if !l.table_on {
        l.table = reg::table_from_patterns(l);
        l.table_on = true;
    }
}

/// The trip of the table a departure is: the one it names, else the one of its direction
/// leaving at its minute.
fn trip_of(l: &LineDesign, day: usize, p: Pick) -> Option<usize> {
    if let Some(i) = p.at.filter(|&i| l.table.get(i).is_some_and(|t| t.dir == p.dir && t.day == day)) {
        return Some(i);
    }
    l.table.iter().position(|t| t.dir == p.dir && t.day == day && (t.departure() - p.dep).abs() < 0.5)
}

/// A departure moved to `to` (minutes, not before midnight): its whole trip with it. Returns
/// it as it is now.
fn move_dep(l: &mut LineDesign, day: usize, p: Pick, to: f32) -> Option<Pick> {
    as_table(l);
    let i = trip_of(l, day, p)?;
    let t = &mut l.table[i];
    t.shift(to.max(0.0).round() - t.departure());
    Some(Pick { dir: p.dir, dep: t.departure(), at: Some(i) })
}

/// A trip added in direction `dir`, leaving at `at`: with the times to the stops of the trip
/// of that direction leaving nearest to it (its direction's own when it has none). Returns it.
fn add_dep(l: &mut LineDesign, day: usize, dir: usize, at: f32) -> Option<Pick> {
    let stops = l.directions.get(dir).map(|d| d.stops.len()).filter(|n| *n >= 2)?;
    as_table(l);
    let at = at.max(0.0).round();
    let near = l.table.iter().filter(|t| t.dir == dir && t.day == day && t.times.len() == stops).min_by(|a, b| (a.departure() - at).abs().total_cmp(&(b.departure() - at).abs()));
    let rel = near.map(TableTrip::relative).unwrap_or_else(|| reg::fitted_times(&l.directions[dir]));
    l.table.push(TableTrip { dir, day, times: rel.iter().map(|x| at + x).collect(), tour: String::new() });
    Some(Pick { dir, dep: at, at: Some(l.table.len() - 1) })
}

/// A departure taken out (whether it was found).
fn remove_dep(l: &mut LineDesign, day: usize, p: Pick) -> bool {
    as_table(l);
    match trip_of(l, day, p) {
        Some(i) => {
            l.table.remove(i);
            true
        }
        None => false,
    }
}

/// A group of days' pattern changed in one go (`f`): a timetable that is a table gets that
/// group's trips anew from it, as the line editor's patterns fill it - the other groups' trips
/// stay as they are.
fn set_pattern(l: &mut LineDesign, day: usize, f: impl FnOnce(&mut DayPattern)) {
    if l.days.len() < DAY_GROUPS.len() {
        l.days = reg::default_days();
    }
    let before: Vec<f32> = l.days[day].departures().iter().map(|d| d.0).collect();
    f(&mut l.days[day]);
    let after: Vec<f32> = l.days[day].departures().iter().map(|d| d.0).collect();
    if l.table_on && before != after {
        let fresh = reg::table_from_patterns(l);
        l.table.retain(|t| t.day != day);
        l.table.extend(fresh.into_iter().filter(|t| t.day == day));
        l.table.sort_by(|a, b| a.day.cmp(&b.day).then(a.departure().total_cmp(&b.departure())).then(a.dir.cmp(&b.dir)));
    }
}

// --- what the line comes to ----------------------------------------------------------------------------

/// What a group of days asks of the company: its trips with passengers, its tours, the buses
/// they need at the busiest, the drivers' duties, the kilometres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Needs {
    trips: usize,
    tours: usize,
    buses: usize,
    duties: usize,
    km: f64,
}

fn needs(tours: &[TourOfDay]) -> Needs {
    Needs {
        trips: tours.iter().flat_map(|t| t.trips.iter()).filter(|x| !x.empty).count(),
        tours: tours.len(),
        buses: ownline::busiest(tours),
        duties: tours.iter().map(|t| co::network::duties_of(t).len()).sum(),
        km: tours.iter().map(|t| t.km()).sum(),
    }
}

/// How a tour fares against the line's tours that run now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    Kept,
    Changed,
    New,
    Gone,
}

/// A tour under the departures: its number, how it fares, the bus the planning's day gives it,
/// and its trips (leaving, arriving, the lane: None for a depot run).
struct Row {
    number: String,
    fate: Fate,
    bus: Option<String>,
    legs: Vec<(i32, i32, Option<usize>)>,
}

/// What the sheet shows of a group of days, made again whenever the line changes.
struct Shown {
    key: (u64, usize),
    /// Each lane's destination (empty: the other trips of a map's line).
    lanes: Vec<String>,
    deps: Vec<Dep>,
    rows: Vec<Row>,
    now: Needs,
    then: Needs,
    diff: TourDiff,
    problems: Vec<reg::Problem>,
}

/// The tour's bus on the planning's day (when it is a day of the group shown).
fn bus_of(c: &Company, plan: Option<&DayPlan>, line: &str, tour: &str) -> Option<String> {
    let t = plan?.tours.iter().find(|t| t.tour.line.eq_ignore_ascii_case(line) && t.tour.tour.trim() == tour.trim())?;
    match t.bus? {
        BusOf::Own(id) => c.vehicle(id).map(|v| v.number.clone()),
        BusOf::Rental => Some(omsi_ui::tr("Rental bus").into_owned()),
    }
}

/// An own line's group of days: its departures as changed, its tours against those that run
/// now (`plan`: the planning's day, when it is one of the group).
fn shown_own(c: &Company, cl: &CompanyLine, o: &Own, day: usize, plan: Option<&DayPlan>, key: (u64, usize)) -> Shown {
    let lanes = o.draft.directions.iter().filter(|d| d.stops.len() >= 2).take(2).map(|d| d.destination()).collect();
    let now = ownline::design_tours(c, cl, &o.live, &o.stem, day);
    let then = ownline::design_tours(c, cl, &o.draft, &o.stem, day);
    let diff = reg::tour_diff(&o.live, &o.draft);
    let lane = |name: &str| (0..2).find(|k| reg::trip_name(&o.stem, *k) == name);
    let row = |t: &TourOfDay, fate: Fate| Row {
        number: t.tour.clone(),
        fate,
        bus: bus_of(c, plan, &cl.name, &t.tour),
        legs: t.trips.iter().map(|x| (x.dep, x.arr, if x.empty { None } else { lane(&x.name) })).collect(),
    };
    let fate = |n: &str| {
        if diff.added.iter().any(|x| x == n) {
            Fate::New
        } else if diff.changed.iter().any(|x| x == n) {
            Fate::Changed
        } else {
            Fate::Kept
        }
    };
    let mut rows: Vec<Row> = then.iter().map(|t| row(t, fate(&t.tour))).collect();
    rows.extend(now.iter().filter(|t| diff.gone.contains(&t.tour)).map(|t| row(t, Fate::Gone)));
    Shown { key, lanes, deps: deps(&o.draft, day), now: needs(&now), then: needs(&then), rows, diff, problems: reg::problems(&o.draft) }
}

/// A line shown as it runs (a map's line): its tours of the planning's day, its trips in a lane
/// for each of the two places they go to most, the others in a third.
fn shown_fixed(c: &Company, line: &str, plan: Option<&DayPlan>, key: (u64, usize)) -> Shown {
    let tours: Vec<TourOfDay> = plan.map(|p| p.tours.iter().filter(|t| t.tour.line.eq_ignore_ascii_case(line)).map(|t| t.tour.clone()).collect()).unwrap_or_default();
    let mut ends: Vec<(String, usize)> = Vec::new();
    for x in tours.iter().flat_map(|t| t.trips.iter()).filter(|x| !x.empty) {
        match ends.iter_mut().find(|e| e.0 == x.to) {
            Some(e) => e.1 += 1,
            None => ends.push((x.to.clone(), 1)),
        }
    }
    ends.sort_by(|a, b| b.1.cmp(&a.1));
    let mut lanes: Vec<String> = ends.iter().take(2).map(|e| e.0.clone()).collect();
    if ends.len() > 2 {
        lanes.push(String::new());
    }
    let lane = |to: &str| lanes.iter().take(2).position(|x| x == to).unwrap_or(2);
    let mut deps: Vec<Dep> = tours.iter().flat_map(|t| t.trips.iter()).filter(|x| !x.empty).map(|x| Dep { dir: lane(&x.to), dep: x.dep as f32, arr: x.arr as f32, at: None }).collect();
    deps.sort_by(|a, b| a.dep.total_cmp(&b.dep).then(a.dir.cmp(&b.dir)));
    let rows = tours.iter().map(|t| Row { number: t.tour.clone(), fate: Fate::Kept, bus: bus_of(c, plan, line, &t.tour), legs: t.trips.iter().map(|x| (x.dep, x.arr, if x.empty { None } else { Some(lane(&x.to)) })).collect() }).collect();
    let n = needs(&tours);
    Shown { key, lanes, deps, rows, now: n, then: n, diff: TourDiff::default(), problems: Vec::new() }
}

/// The hours the axis shows: the trips' with a little room, six at the least.
fn span_of(s: &Shown) -> (i32, i32) {
    let times = s.deps.iter().map(|d| (d.dep, d.arr)).chain(s.rows.iter().flat_map(|r| r.legs.iter().map(|g| (g.0 as f32, g.1 as f32))));
    let (a, z) = times.fold((f32::MAX, f32::MIN), |(a, z), (x, y)| (a.min(x), z.max(y)));
    if a > z {
        return (5 * 60, 24 * 60);
    }
    let mut t0 = (((a - 20.0) / 60.0).floor() as i32 * 60).max(0);
    let mut t1 = (((z + 20.0) / 60.0).ceil() as i32 * 60).min(28 * 60);
    if t1 - t0 < 6 * 60 {
        t1 = (t0 + 6 * 60).min(28 * 60);
        t0 = (t1 - 6 * 60).max(0);
    }
    (t0, t1)
}

/// Where the minutes lie across the axis.
#[derive(Clone, Copy, Debug)]
struct Axis {
    x: f32,
    w: f32,
    t0: i32,
    t1: i32,
}

impl Axis {
    fn ppm(&self) -> f32 {
        self.w / (self.t1 - self.t0).max(60) as f32
    }

    fn x_of(&self, m: f32) -> f32 {
        self.x + (m - self.t0 as f32) * self.ppm()
    }

    fn minute(&self, x: f32) -> f32 {
        (self.t0 as f32 + (x - self.x) / self.ppm()).clamp(0.0, (self.t1 - 1) as f32)
    }

    /// The whole hours on the axis, every `every`-th.
    fn hours(&self, every: i32) -> Vec<i32> {
        ((self.t0 + 59).div_euclid(60)..=self.t1.div_euclid(60)).filter(|h| h % every == 0).collect()
    }
}

/// A lane's colour: the way out the accent's, the way back a calm blue (told apart from the
/// accent whatever it is), the rest dim.
fn lane_colour(k: Option<usize>) -> Color {
    match k {
        Some(0) => accent(),
        Some(1) => EARLY_SOFT,
        Some(_) => TEXT_SOFT,
        None => TEXT_FAINT,
    }
}

// --- the sheet ------------------------------------------------------------------------------------------

/// An own line being changed: its registry and its id there, its file stem, the line as the
/// registry has it (`saved`, a change waiting for its day included), the same without that
/// day's mark (`base`, what "changed" is against), as the map's timetable runs it now
/// (`live`) and as changed here (`draft`).
#[derive(Clone)]
pub(super) struct Own {
    pub(super) id: u64,
    pub(super) path: PathBuf,
    pub(super) stem: String,
    pub(super) saved: LineDesign,
    pub(super) base: LineDesign,
    pub(super) live: LineDesign,
    pub(super) draft: LineDesign,
}

pub(super) fn own_of(id: u64, path: PathBuf, stem: String, saved: LineDesign) -> Own {
    let base = LineDesign { live: None, pending_from: String::new(), ..saved.clone() };
    let live = match saved.live.as_deref() {
        Some(x) => LineDesign { id, live: None, pending_from: String::new(), ..x.clone() },
        None => base.clone(),
    };
    Own { id, path, stem, saved, draft: base.clone(), base, live }
}

/// The company line's own line in the line editor's registry, to change - or why its trips
/// are only shown.
pub(super) fn load(c: &Company, cl: &CompanyLine) -> Result<Own, &'static str> {
    if !cl.own {
        return Err(MAP_LINE);
    }
    let id = cl.plan.as_ref().map(|p| p.line_id).ok_or(NOT_FOUND)?;
    let path = reg::registry_path(&reg::map_folder(&c.map));
    let r = reg::load_registry(&path);
    let saved = r.line(id).cloned().ok_or(NOT_FOUND)?;
    let stem = reg::stems(&r).get(&id).cloned().unwrap_or_else(|| cl.name.clone());
    Ok(own_of(id, path, stem, saved))
}

/// The sheet over the planning (`PlanningView::edit`).
pub(in super::super) struct PlanEdit {
    /// The company (its id: another one opened meanwhile closes the sheet), its line (the
    /// timetable name), the planning's day it was opened from, the tour it was opened from
    /// (marked), and the group of days shown.
    company: String,
    line: String,
    date: String,
    tour: Option<String>,
    day: usize,
    own: Option<Own>,
    /// Why the line's trips are only shown (a map's line).
    fixed: Option<&'static str>,
    /// The line before each change, to undo; the last change (a control pulled at a stretch is
    /// one change) and when.
    undo: Vec<LineDesign>,
    last: (String, f32),
    rev: u64,
    shown: Option<Shown>,
    sel: Option<Pick>,
    drag: Option<Drag>,
    /// Pressed on an empty place of a lane: a trip added there when it is let go.
    press: Option<(usize, f32)>,
    /// The axis's hours, held while a departure is dragged; the hours zoomed into with the
    /// wheel (None: the whole day's), and where the axis was taken to be moved along.
    span: Option<(i32, i32)>,
    view: Option<(i32, i32)>,
    pan: Option<(f32, (i32, i32), bool)>,
    /// The time typed for the departure chosen.
    text: String,
    /// The day a change of a line in service begins (tomorrow at the soonest).
    from: String,
    /// Closing with changes not saved: asked first.
    leaving: bool,
}

/// A departure being dragged: the one taken, its minute then, where the mouse took it.
#[derive(Clone, Copy, Debug)]
struct Drag {
    pick: Pick,
    dep0: f32,
    x0: f32,
    moved: bool,
}

impl PlanEdit {
    /// The line changed by `f` (an own line's), the line before kept to be undone - once for a
    /// control worked at a stretch (`what`).
    fn edit<R>(&mut self, now: f32, what: &str, f: impl FnOnce(&mut LineDesign) -> R) -> Option<R> {
        let o = self.own.as_mut()?;
        if self.last.0 != what || now - self.last.1 > 1.0 {
            self.undo.push(o.draft.clone());
            if self.undo.len() > 60 {
                self.undo.remove(0);
            }
        }
        self.last = (what.to_string(), now);
        let out = f(&mut o.draft);
        self.rev += 1;
        Some(out)
    }

    fn dirty(&self) -> bool {
        self.own.as_ref().is_some_and(|o| o.draft != o.base)
    }
}

/// Open the sheet for a company line, from the planning's day `date` (and one of its tours).
pub(in super::super) fn open(l: &mut Launcher, line: &str, date: &str, tour: Option<String>) {
    let Some(c) = l.company.company.as_ref() else { return };
    let Some(cl) = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line)) else { return };
    let (own, fixed) = match load(c, cl) {
        Ok(o) => (Some(o), None),
        Err(why) => (None, Some(why)),
    };
    let tomorrow = co::dates::add(&c.date, 1);
    let from = own.as_ref().map(|o| o.saved.pending_from.clone()).filter(|d| co::dates::between(&c.date, d) >= 1).unwrap_or(tomorrow);
    // (the group of days of the planning's day; one the line does not run on: the first it does)
    let mut day = ownline::day_type(date);
    if let Some(o) = &own {
        if !o.draft.service.allows(day) {
            day = (0..DAY_GROUPS.len()).find(|k| o.draft.service.allows(*k)).unwrap_or(0);
        }
    }
    let e = PlanEdit { company: c.id.clone(), line: cl.name.clone(), date: date.to_string(), tour, day, own, fixed, undo: Vec::new(), last: (String::new(), f32::MIN), rev: 0, shown: None, sel: None, drag: None, press: None, span: None, view: None, pan: None, text: String::new(), from, leaving: false };
    let v = &mut l.company.planning;
    (v.arm, v.drag, v.carry, v.press) = (None, None, None, None);
    v.edit = Some(e);
}

/// The sheet over the planning's page (`area`), while it is open.
pub(in super::super) fn draw(l: &mut Launcher, area: Rect, c: &Company) {
    let Some(mut e) = l.company.planning.edit.take() else { return };
    if sheet(l, area, c, &mut e) && l.company.planning.edit.is_none() {
        l.company.planning.edit = Some(e);
    }
}

/// What the sheet shows, made again when the line changed or another group of days is shown.
fn refresh(c: &Company, cl: &CompanyLine, e: &mut PlanEdit, plan: Option<&DayPlan>) {
    let key = (e.rev, e.day);
    if e.shown.as_ref().is_some_and(|s| s.key == key) {
        return;
    }
    // (the planning's day gives its tours their buses only when it is a day of the group)
    let plan = plan.filter(|p| e.own.is_none() || ownline::day_type(&p.date) == e.day);
    e.shown = Some(match &e.own {
        Some(o) => shown_own(c, cl, o, e.day, plan, key),
        None => shown_fixed(c, &cl.name, plan, key),
    });
}

/// The sheet: its head, the axis with the tours under it, the departure chosen, the pattern in
/// one go, what the line needs, the list, and its foot. Returns whether it stays open.
fn sheet(l: &mut Launcher, area: Rect, c: &Company, e: &mut PlanEdit) -> bool {
    // (another company open meanwhile, or the line given up: nothing to show)
    if c.id != e.company {
        return false;
    }
    let Some(cl) = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&e.line)).cloned() else { return false };
    let plan = plan_of(l, c, &e.date).and_then(|p| p.ok());
    let now = l.ui.time;
    l.ui.solid(area);
    l.ui.p().shadow(area, SHEET_RADIUS, 24.0, Color::rgba(0, 0, 0, 0.55));
    l.ui.panel(area);
    let x = area.x + 24.0;
    let w = area.w - 48.0;
    let editable = e.own.is_some();
    let mut close = false;
    // the head: the line, its groups of days, the cross
    let y = area.y + 18.0;
    let pw = line_plate(&mut l.ui, Vec2::new(x, y + 2.0), &cl, 30.0);
    let title = omsi_ui::tr("Trips and timetable of line %{n}").replace("%{n}", &cl.number);
    let seg_w = 380.0f32.min(w * 0.4);
    let tw = (w - pw - 14.0 - seg_w - 70.0).max(80.0);
    l.ui.text_in(&title, Rect::new(x + pw + 14.0, y - 2.0, tw, 24.0), 19.0, Weight::Bold, TEXT, Align::Left);
    let caption = if cl.title.trim().is_empty() { cl.caption.clone() } else { cl.title.trim().to_string() };
    l.ui.text_in(&caption, Rect::new(x + pw + 14.0, y + 22.0, tw, 18.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
    if l.ui.icon_button("pe-close", Vec2::new(area.right() - 32.0, y + 16.0), 16.0, "close", "Close") {
        close = true;
    }
    let seg = Rect::new(area.right() - 64.0 - seg_w, y, seg_w, 34.0);
    match &e.own {
        Some(o) => {
            let labels: Vec<&str> = DAY_GROUPS.iter().map(|g| g.0).collect();
            let off: Vec<usize> = (0..DAY_GROUPS.len()).filter(|k| !o.draft.service.allows(*k)).collect();
            let mut d = e.day;
            if l.ui.segmented_some("pe-day", seg, &mut d, &labels, &off) && d != e.day {
                (e.day, e.sel, e.drag, e.text) = (d, None, None, String::new());
            }
        }
        None => {
            l.ui.text_in(&day_label(&e.date), seg, kit::ROWS, Weight::Bold, TEXT_SOFT, Align::Right);
        }
    }
    // what a change does, said under the head
    let running = cl.service_from.is_some();
    let (note, colour) = match (&e.own, e.fixed) {
        (_, Some(why)) => (omsi_ui::tr(why).into_owned(), WARN),
        (Some(o), _) if !o.saved.pending_from.is_empty() => (omsi_ui::tr("A new timetable waits for %{date}: it is the one shown, against the tours that run until then.").replace("%{date}", &day_label(&o.saved.pending_from)), accent_2()),
        (Some(o), _) if o.draft.table_on && !o.base.table_on => (omsi_ui::tr("Moved by hand, the timetable is a table of trips now (the line editor's \"As a table\"): every departure on its own.").into_owned(), accent_2()),
        _ if running => (omsi_ui::tr("In service: today's tours run as they are; a change begins on the day you choose, tomorrow at the soonest.").into_owned(), TEXT_DIM),
        _ => (omsi_ui::tr("Not in service yet: a change counts at once.").into_owned(), TEXT_DIM),
    };
    l.ui.text_in(&note, Rect::new(x, area.y + 62.0, w, 20.0), kit::NOTE, Weight::Medium, colour, Align::Left);
    let foot_h = 56.0;
    let body = Rect::new(x, area.y + 94.0, w, (area.bottom() - 18.0 - foot_h - area.y - 94.0).max(120.0));
    let left_w = (body.w * 0.62).floor();
    let left = Rect::new(body.x, body.y, left_w, body.h);
    let right = Rect::new(body.x + left_w + 22.0, body.y, body.w - left_w - 22.0, body.h);
    refresh(c, &cl, e, plan.as_ref());
    // the axis: the whole day's hours, or those zoomed into (held while a departure is dragged)
    let lanes = e.shown.as_ref().map(|s| s.lanes.len()).unwrap_or(0).max(1);
    let whole = e.shown.as_ref().map(span_of).unwrap_or((300, 1440));
    e.view = e.view.and_then(|v| fit_view(v, whole));
    let (t0, t1) = e.span.or(e.view).unwrap_or(whole);
    let ax = Axis { x: left.x + LABEL_W, w: (left.w - LABEL_W - 6.0).max(60.0), t0, t1 };
    let head_y = left.y + 22.0;
    zoom(l, e, ax, whole, Rect::new(ax.x, head_y, ax.w, 20.0 + lanes as f32 * LANE_H));
    drag_step(l, e, ax, now);
    keys(l, e, now, &mut close);
    refresh(c, &cl, e, plan.as_ref());
    axis_lanes(l, e, ax, head_y, now);
    // (a departure added or moved there: shown so at once)
    refresh(c, &cl, e, plan.as_ref());
    let after_lanes = head_y + 20.0 + lanes as f32 * LANE_H;
    let tb_y = after_lanes + 2.0;
    if editable {
        trip_tools(l, e, Rect::new(left.x, tb_y, left.w, 36.0), now);
        refresh(c, &cl, e, plan.as_ref());
    }
    let tours_y = if editable { tb_y + 46.0 } else { tb_y + 6.0 };
    tours_chart(l, e, ax, Rect::new(left.x, tours_y, left.w, (left.bottom() - tours_y).max(40.0)));
    // the right, scrolled as one: what it needs (seen at once as the departures change), the
    // pattern in one go, the list
    l.ui.scroll_area("pe-right", right, &mut |ui, v| {
        let w = v.w - 12.0;
        let mut y = needs_card(ui, e, c, &cl, Rect::new(v.x, v.y, w, 0.0)) + 18.0;
        if editable {
            ui.p().rect(Rect::new(v.x, y - 10.0, w, 1.0), HAIRLINE);
            y = pattern_card(ui, e, Rect::new(v.x, y, w, 0.0), now) + 18.0;
        }
        ui.p().rect(Rect::new(v.x, y - 10.0, w, 1.0), HAIRLINE);
        y = list_card(ui, e, Rect::new(v.x, y, w, 0.0));
        y - v.y + 8.0
    });
    // the foot
    let foot = Rect::new(x, area.bottom() - 18.0 - 40.0, w, 40.0);
    l.ui.p().rect(Rect::new(x, foot.y - 14.0, w, 1.0), HAIRLINE);
    match foot_row(l, e, c, &cl, foot, close) {
        Leave::Stay => true,
        Leave::Close => false,
        Leave::Save => !save(l, e),
    }
}

/// The hours zoomed into kept within the whole day's, two at the least (None: they are the
/// whole day's).
fn fit_view(v: (i32, i32), whole: (i32, i32)) -> Option<(i32, i32)> {
    let span = (v.1 - v.0).clamp(120, 28 * 60);
    if span >= whole.1 - whole.0 {
        return None;
    }
    let t0 = v.0.clamp(whole.0, whole.1 - span);
    Some((t0, t0 + span))
}

/// The wheel over the lanes zooms into the hours under the mouse (a dense timetable's
/// departures apart enough to be taken one by one), and out again to the whole day; the axis
/// taken on an empty place and moved goes along the day (`axis_lanes`).
fn zoom(l: &mut Launcher, e: &mut PlanEdit, ax: Axis, whole: (i32, i32), lanes: Rect) {
    let ui = &mut l.ui;
    let wheel = ui.input.wheel.y;
    if wheel.abs() > 0.0 && ui.hover(lanes) && !ui.wheel_taken() && e.drag.is_none() {
        let m = ax.minute(ui.input.mouse.x);
        let span = (ax.t1 - ax.t0) as f32;
        let to = (span * 0.8f32.powf(wheel)).clamp(120.0, (whole.1 - whole.0) as f32);
        let t0 = (m - (m - ax.t0 as f32) * to / span).round() as i32;
        e.view = fit_view((t0, t0 + to.round() as i32), whole);
        ui.input.wheel = Vec2::ZERO;
    }
    // the axis moved along, held on an empty place
    if let Some((x0, (a, z), moved)) = e.pan {
        if !ui.input.down {
            e.pan = None;
        } else {
            let dx = ui.input.mouse.x - x0;
            if moved || dx.abs() > 4.0 {
                e.pan = Some((x0, (a, z), true));
                // (a press that moves is no click: no trip added where it began)
                e.press = None;
                if e.view.is_some() {
                    let by = (dx / ax.ppm()).round() as i32;
                    e.view = fit_view((a - by, z - by), whole);
                    ui.cursor = winit::window::CursorIcon::Grabbing;
                }
            }
        }
    }
}

/// A departure dragged along its lane: moved minute by minute while it is held (the trip with
/// it), let go where it lies.
fn drag_step(l: &mut Launcher, e: &mut PlanEdit, ax: Axis, now: f32) {
    let Some(mut d) = e.drag else { return };
    let input = &l.ui.input;
    if !input.down {
        e.drag = None;
        e.span = None;
        e.last = (String::new(), f32::MIN);
        return;
    }
    let dx = input.mouse.x - d.x0;
    if !d.moved && dx.abs() > 3.0 {
        d.moved = true;
    }
    if d.moved {
        l.ui.cursor = winit::window::CursorIcon::Grabbing;
        let to = (d.dep0 + dx / ax.ppm()).round().clamp(0.0, (ax.t1 - 1) as f32);
        let day = e.day;
        if (to - d.pick.dep).abs() >= 0.5 {
            if let Some(Some(p)) = e.edit(now, "drag", |line| move_dep(line, day, d.pick, to)) {
                d.pick = p;
                e.sel = Some(p);
                e.text.clear();
            }
        }
    }
    e.drag = Some(d);
}

/// The keys for the departure chosen: the arrows a minute earlier or later (five with Shift),
/// Delete takes it out; Escape lets it go - and, nothing chosen, closes the sheet.
fn keys(l: &mut Launcher, e: &mut PlanEdit, now: f32, close: &mut bool) {
    if l.ui.focus.is_some() || e.drag.is_some() {
        return;
    }
    let keys = l.ui.input.keys.clone();
    let step = if l.ui.input.shift { 5.0 } else { 1.0 };
    let day = e.day;
    for k in keys {
        match (k, e.sel) {
            (Key::Escape, Some(_)) => e.sel = None,
            (Key::Escape, None) => *close = true,
            (Key::Left | Key::Right, Some(p)) if e.own.is_some() => {
                let to = p.dep + if k == Key::Left { -step } else { step };
                if let Some(Some(q)) = e.edit(now, "keys", |line| move_dep(line, day, p, to)) {
                    e.sel = Some(q);
                }
            }
            (Key::Delete | Key::Backspace, Some(p)) if e.own.is_some() => {
                e.edit(now, "delete", |line| remove_dep(line, day, p));
                e.sel = None;
                e.last = (String::new(), f32::MIN);
            }
            _ => {}
        }
    }
}

/// The hours over the lanes, and a lane per direction with its departures: dragged to move
/// one, an empty place clicked to add one there.
fn axis_lanes(l: &mut Launcher, e: &mut PlanEdit, ax: Axis, y: f32, now: f32) {
    let Some(s) = e.shown.as_ref() else { return };
    let ui = &mut l.ui;
    let left_x = ax.x - LABEL_W;
    kit::caps(ui, Rect::new(left_x, y - 22.0, LABEL_W + ax.w, 16.0), "Departures");
    let mut hint_right = ax.x + ax.w;
    if e.view.is_some() {
        let label = omsi_ui::tr("Whole day");
        let bw = ui.width(&label, 13.0, Weight::Medium) + 13.0 * 1.3 + 6.0 + 26.0;
        let b = Rect::new(ax.x + ax.w - bw, y - 30.0, bw, 26.0);
        if ui.button("pe-whole-day", b, "Whole day", Some("zoom_out"), ButtonKind::Ghost) {
            e.view = None;
        }
        hint_right = b.x - 10.0;
    }
    let hint = if e.own.is_some() { "Drag a departure to move it · click the axis to add one · Delete takes the one chosen out · the wheel zooms" } else { "The wheel over the departures zooms into the hours" };
    ui.text_in(&omsi_ui::tr(hint), Rect::new(left_x + 150.0, y - 24.0, (hint_right - left_x - 150.0).max(0.0), 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Right);
    // the hours
    let ppm = ax.ppm();
    let every = if ppm * 60.0 >= 40.0 { 1 } else if ppm * 60.0 >= 20.0 { 2 } else { 3 };
    let lanes_h = s.lanes.len().max(1) as f32 * LANE_H;
    for h in ax.hours(every) {
        let hx = ax.x_of((h * 60) as f32);
        ui.text_in(&format!("{:02}", h % 24), Rect::new(hx - 16.0, y, 32.0, 16.0), 12.0, Weight::Bold, TEXT_DIM, Align::Center);
        ui.p().rect(Rect::new(hx, y + 18.0, 1.0, lanes_h - 8.0), HAIRLINE.alpha(0.6));
    }
    let editable = e.own.is_some();
    let mouse = ui.input.mouse;
    let (pressed, released) = (ui.input.pressed, ui.input.released);
    let mut add: Option<(usize, f32)> = None;
    let mut pick: Option<(Pick, f32)> = None;
    let mut empty_press: Option<(usize, f32)> = None;
    let tour_deps: Vec<(i32, Option<usize>)> = e
        .tour
        .as_ref()
        .and_then(|n| s.rows.iter().find(|r| &r.number == n && r.fate != Fate::Gone))
        .map(|r| r.legs.iter().filter(|g| g.2.is_some()).map(|g| (g.0, g.2)).collect())
        .unwrap_or_default();
    for (k, dest) in s.lanes.iter().enumerate() {
        let ly = y + 20.0 + k as f32 * LANE_H;
        let lr = Rect::new(ax.x, ly, ax.w, LANE_H - 8.0);
        let colour = lane_colour(Some(k));
        // the lane's words: which way, where to, how many
        let mine: Vec<&Dep> = s.deps.iter().filter(|d| d.dir == k).collect();
        let way = lane_word(editable, k, dest);
        ui.p().rounded(Rect::new(left_x, ly + 4.0, 4.0, LANE_H - 16.0), 2.0, colour);
        ui.text_in(&way.to_uppercase(), Rect::new(left_x + 12.0, ly + 1.0, LABEL_W - 20.0, 14.0), 11.0, Weight::Bold, TEXT_DIM, Align::Left);
        let to = if dest.is_empty() { String::new() } else { format!("→ {dest}") };
        ui.text_in(&to, Rect::new(left_x + 12.0, ly + 14.0, LABEL_W - 20.0, 16.0), 13.0, Weight::Bold, TEXT, Align::Left);
        ui.text_in(&omsi_ui::tr("%{n} departures").replace("%{n}", &mine.len().to_string()), Rect::new(left_x + 12.0, ly + 29.0, LABEL_W - 20.0, 14.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
        ui.p().rounded(lr, 8.0, FIELD.alpha(0.7));
        let (h, _, clicked) = ui.interact(id_of(&format!("pe-lane-{k}")), lr);
        // (the departure under the mouse: the nearest within a few points)
        let near = if h { mine.iter().copied().min_by(|a, b| (ax.x_of(a.dep) - mouse.x).abs().total_cmp(&(ax.x_of(b.dep) - mouse.x).abs())).filter(|d| (ax.x_of(d.dep) - mouse.x).abs() <= 7.0) } else { None };
        for (i, d) in mine.iter().enumerate() {
            let px = ax.x_of(d.dep);
            if px < lr.x - 2.0 || px > lr.right() + 2.0 {
                continue;
            }
            let chosen = e.sel.is_some_and(|p| d.is(&p));
            let hot = near.is_some_and(|n| n == *d);
            let marked = tour_deps.iter().any(|(m, lane)| *lane == Some(k) && (*m as f32 - d.dep).abs() < 0.6);
            if chosen {
                // its trip from leaving to arriving under it
                let ex = ax.x_of(d.arr).min(lr.right());
                ui.p().rounded(Rect::new(px, lr.bottom() - 9.0, (ex - px).max(2.0), 4.0), 2.0, colour.alpha(0.55));
                ui.p().rounded(Rect::new(px - 3.0, lr.y + 4.0, 6.0, 26.0), 3.0, TEXT);
            } else {
                let c = if hot { colour.lighten(0.25) } else if marked { colour } else { colour.alpha(0.85) };
                ui.p().rounded(Rect::new(px - 1.5, lr.y + 7.0, if hot { 4.0 } else { 3.0 }, 20.0), 1.5, c);
            }
            if marked {
                ui.p().circle(Vec2::new(px, lr.y + 4.0), 2.5, accent_2());
            }
            // its time under it, where its neighbours leave it room
            let room = |j: Option<&&Dep>| j.map(|o| (ax.x_of(o.dep) - px).abs()).unwrap_or(f32::MAX);
            let free = room(if i > 0 { mine.get(i - 1) } else { None }).min(room(mine.get(i + 1)));
            if chosen || free >= 38.0 {
                ui.text_in(&hm(d.dep), Rect::new(px - 20.0, lr.y + 27.0, 40.0, 14.0), 11.0, if chosen { Weight::Bold } else { Weight::Regular }, if chosen { TEXT } else { TEXT_DIM }, Align::Center);
            }
        }
        if let Some(d) = near {
            ui.tooltip(lr, &format!("{} – {}", hm(d.dep), hm(d.arr)));
            if editable {
                ui.cursor = winit::window::CursorIcon::Grab;
            }
        }
        if h && editable && near.is_none() && e.drag.is_none() {
            // where a trip would be added
            let m = ax.minute(mouse.x).round();
            let gx = ax.x_of(m);
            ui.p().rounded(Rect::new(gx - 1.0, lr.y + 7.0, 2.0, 20.0), 1.0, colour.alpha(0.4));
            ui.text_in(&format!("+ {}", hm(m)), Rect::new(gx - 26.0, lr.y + 27.0, 52.0, 14.0), 11.0, Weight::Bold, colour, Align::Center);
        }
        if h && pressed {
            match near {
                Some(d) => pick = Some((d.pick(), mouse.x)),
                None => empty_press = Some((k, ax.minute(mouse.x).round())),
            }
        }
        if clicked {
            if let Some((pk, m)) = e.press.filter(|p| p.0 == k) {
                add = Some((pk, m));
            }
        }
    }
    // the drag's minute over the lane while it moves
    if let Some(d) = e.drag.filter(|d| d.moved) {
        if let Some(dep) = s.deps.iter().find(|x| x.is(&d.pick)) {
            let px = ax.x_of(dep.dep);
            let ly = y + 20.0 + dep.dir as f32 * LANE_H;
            let by = (dep.dep - d.dep0).round() as i32;
            let text = if by == 0 { hm(dep.dep) } else { format!("{}  ({}{} min)", hm(dep.dep), if by > 0 { "+" } else { "−" }, by.abs()) };
            let tw = ui.width(&text, 12.5, Weight::Bold) + 16.0;
            let b = Rect::new(px - tw * 0.5, ly - 22.0, tw, 22.0);
            ui.p().shadow(b, 6.0, 8.0, Color::rgba(0, 0, 0, 0.4));
            ui.p().rounded(b, 6.0, accent());
            ui.text_in(&text, b, 12.5, Weight::Bold, on_accent(), Align::Center);
        }
    }
    if let Some((p, mx)) = pick {
        e.sel = Some(p);
        e.text.clear();
        e.press = None;
        if editable {
            e.drag = Some(Drag { pick: p, dep0: p.dep, x0: mx, moved: false });
            e.span = Some((ax.t0, ax.t1));
            e.last = (String::new(), f32::MIN);
        }
    } else if let Some(p) = empty_press {
        // (an empty place: a trip added there when it is let go - a line of the map's only
        // shown - unless the axis is moved along)
        e.press = editable.then_some(p);
        e.pan = Some((mouse.x, (ax.t0, ax.t1), false));
    }
    if let Some((k, m)) = add {
        let day = e.day;
        if let Some(Some(p)) = e.edit(now, "add", |line| add_dep(line, day, k, m)) {
            e.sel = Some(p);
            e.text.clear();
        }
        e.last = (String::new(), f32::MIN);
    }
    if released {
        e.press = None;
    }
}

/// The departure chosen: when it leaves and arrives, its time typed, moved a few minutes, a
/// copy a headway later, or taken out.
fn trip_tools(l: &mut Launcher, e: &mut PlanEdit, r: Rect, now: f32) {
    let Some(s) = e.shown.as_ref() else { return };
    let ui = &mut l.ui;
    ui.p().rounded(r, 8.0, Color::WHITE.alpha(0.03));
    let chosen = e.sel.and_then(|p| s.deps.iter().find(|d| d.is(&p)).copied());
    let Some(d) = chosen else {
        e.sel = None;
        ui.text_in(&omsi_ui::tr("Choose a departure on the axis or in the list: drag it, type its time, or take it out."), Rect::new(r.x + 12.0, r.y, r.w - 24.0, r.h), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    };
    let way = if d.dir == 0 { omsi_ui::tr("Outbound") } else { omsi_ui::tr("Return") };
    let tour = s.rows.iter().find(|row| row.fate != Fate::Gone && row.legs.iter().any(|g| g.2 == Some(d.dir) && (g.0 as f32 - d.dep).abs() < 0.6)).map(|row| format!("  ·  {} {}", omsi_ui::tr("Tour"), row.number)).unwrap_or_default();
    let words = format!("{} – {}  ·  {way}{tour}", hm(d.dep), hm(d.arr));
    let headway = e.own.as_ref().and_then(|o| o.draft.days.get(e.day)).map(|p| if p.bands.is_empty() { p.headway } else { p.clean_bands().first().map(|b| b.headway).unwrap_or(p.headway) }).unwrap_or(20.0).max(1.0);
    // the tools from the right: take out, copy, move, the time typed
    let mut bx = r.right() - 8.0;
    let mut act: Option<f32> = None;
    let mut copy = false;
    let mut delete = false;
    let bh = r.h - 8.0;
    if ui.icon_button("pe-delete", Vec2::new(bx - 14.0, r.center().y), 13.0, "delete", "Take the trip out") {
        delete = true;
    }
    bx -= 34.0;
    let cw = ui.width(&omsi_ui::tr("Copy"), 13.0, Weight::Medium) + 28.0;
    let cr = Rect::new(bx - cw, r.y + 4.0, cw, bh);
    if ui.button("pe-copy", cr, "Copy", None, ButtonKind::Ghost) {
        copy = true;
    }
    ui.tooltip(cr, &omsi_ui::tr("A copy of the trip, %{n} minutes later").replace("%{n}", &format!("{headway:.0}")));
    bx = cr.x - 10.0;
    for (m, label) in [(5.0, "+5"), (1.0, "+1"), (-1.0, "-1"), (-5.0, "-5")] {
        let br = Rect::new(bx - 38.0, r.y + 4.0, 38.0, bh);
        if ui.button(&format!("pe-shift-{label}"), br, label, None, ButtonKind::Ghost) {
            act = Some(d.dep + m);
        }
        ui.tooltip(br, "The whole trip so many minutes earlier or later");
        bx = br.x - 2.0;
    }
    let tr_ = Rect::new(bx - 84.0, r.y + 4.0, 76.0, bh);
    let mut text = e.text.clone();
    ui.text_input("pe-time", tr_, &mut text, &hm(d.dep), None);
    let typed = parse_hm(&text).filter(|_| whole_time(&text) || ui.input.keys.contains(&Key::Enter));
    ui.text_in(&words, Rect::new(r.x + 12.0, r.y, (tr_.x - r.x - 20.0).max(40.0), r.h), 13.0, Weight::Bold, TEXT, Align::Left);
    e.text = text;
    let day = e.day;
    let p = d.pick();
    if let Some(m) = typed {
        let to = typed_minutes(m, d.dep);
        if let Some(Some(q)) = e.edit(now, "typed", |line| move_dep(line, day, p, to)) {
            e.sel = Some(q);
        }
        e.text.clear();
        l.ui.focus = None;
    } else if let Some(to) = act {
        if let Some(Some(q)) = e.edit(now, "shift", |line| move_dep(line, day, p, to)) {
            e.sel = Some(q);
        }
    } else if copy {
        if let Some(Some(q)) = e.edit(now, "copy", |line| add_dep(line, day, p.dir, p.dep + headway)) {
            e.sel = Some(q);
        }
        e.last = (String::new(), f32::MIN);
    } else if delete {
        e.edit(now, "delete", |line| remove_dep(line, day, p));
        e.sel = None;
        e.last = (String::new(), f32::MIN);
    }
}

/// The tours of the group of days under the departures, on the same hours: each a bus's day,
/// its trips coloured by their direction - against those that run now (kept, changed, new,
/// dropped), with the bus the planning's day gives it.
fn tours_chart(l: &mut Launcher, e: &mut PlanEdit, ax: Axis, r: Rect) {
    let Some(s) = e.shown.as_ref() else { return };
    let ui = &mut l.ui;
    let editable = e.own.is_some();
    let title = omsi_ui::tr("Tours: the buses it needs (%{n})").replace("%{n}", &s.rows.iter().filter(|r| r.fate != Fate::Gone).count().to_string());
    kit::caps(ui, Rect::new(r.x, r.y, r.w, 16.0), &title);
    if editable {
        // the legend of what changes
        let mut lx = r.right();
        for (word, colour) in [("dropped", DANGER.lighten(0.2)), ("new", OK), ("changed", WARN)] {
            let t = omsi_ui::tr(word);
            let tw = ui.width(&t, 11.5, Weight::Bold) + 14.0;
            lx -= tw;
            ui.p().circle(Vec2::new(lx + 4.0, r.y + 8.0), 3.5, colour);
            ui.text_in(&t, Rect::new(lx + 11.0, r.y, tw - 11.0, 16.0), 11.5, Weight::Bold, TEXT_DIM, Align::Left);
            lx -= 8.0;
        }
    }
    let rows = Rect::new(r.x, r.y + 22.0, r.w, (r.h - 22.0).max(20.0));
    if s.rows.is_empty() {
        ui.text_in(&omsi_ui::tr("No tours on these days."), Rect::new(rows.x, rows.y, rows.w, 22.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let ppm = ax.ppm();
    let every = if ppm * 60.0 >= 40.0 { 1 } else if ppm * 60.0 >= 20.0 { 2 } else { 3 };
    let hours = ax.hours(every);
    let focus = e.tour.clone();
    let pitch = (rows.h / s.rows.len() as f32).floor().clamp(TOUR_H, TOUR_MAX);
    let bar = ((pitch - 2.0) * 0.55).round().clamp(12.0, 20.0);
    ui.scroll_area("pe-tours", rows, &mut |ui, v| {
        let mut y = v.y;
        for row in &s.rows {
            let rr = Rect::new(v.x, y, v.w - 8.0, pitch - 2.0);
            y += pitch;
            if !ui.rect_visible(rr) {
                continue;
            }
            let gone = row.fate == Fate::Gone;
            if focus.as_deref() == Some(row.number.as_str()) && !gone {
                ui.p().rounded(rr, 5.0, accent().alpha(0.12));
            }
            for &h in &hours {
                ui.p().rect(Rect::new(ax.x_of((h * 60) as f32), rr.y, 1.0, rr.h), HAIRLINE.alpha(0.4));
            }
            let (mark, colour) = match row.fate {
                Fate::Kept => ("", TEXT),
                Fate::Changed => ("changed", WARN),
                Fate::New => ("new", OK),
                Fate::Gone => ("dropped", DANGER.lighten(0.2)),
            };
            ui.text_in(&format!("{} {}", omsi_ui::tr("Tour"), row.number), Rect::new(rr.x + 4.0, rr.y, 66.0, rr.h), 12.5, Weight::Bold, if gone { TEXT_DIM } else { colour }, Align::Left);
            if !mark.is_empty() {
                ui.p().circle(Vec2::new(rr.x + 74.0, rr.center().y), 3.5, colour);
            }
            // (the bus the planning's day gives it: a changed or dropped tour loses it)
            if let Some(b) = &row.bus {
                let lose = matches!(row.fate, Fate::Changed | Fate::Gone);
                let text = if lose { omsi_ui::tr("loses %{bus}").replace("%{bus}", b) } else { b.clone() };
                ui.text_in(&text, Rect::new(rr.x + 82.0, rr.y, LABEL_W - 90.0, rr.h), 11.5, Weight::Medium, if lose { WARN } else { TEXT_DIM }, Align::Left);
            }
            for &(a, z, lane) in &row.legs {
                let xa = ax.x_of(a as f32).max(ax.x);
                let xz = ax.x_of(z as f32).min(ax.x + ax.w);
                if xz <= xa {
                    continue;
                }
                let col = lane_colour(lane).alpha(if gone { 0.3 } else if lane.is_none() { 0.5 } else { 0.9 });
                let hgt = if lane.is_none() { bar * 0.5 } else { bar };
                ui.p().rounded(Rect::new(xa, rr.center().y - hgt * 0.5, (xz - xa - 1.0).max(1.5), hgt), 3.0, col);
            }
            let tip = match row.fate {
                Fate::Kept => omsi_ui::tr("Runs as it does now").into_owned(),
                Fate::Changed => omsi_ui::tr("Its trips change: its bus and drivers are given anew").into_owned(),
                Fate::New => omsi_ui::tr("A tour more: a bus and drivers to give").into_owned(),
                Fate::Gone => omsi_ui::tr("No longer driven: its bus and drivers are free").into_owned(),
            };
            ui.tooltip(rr, &tip);
        }
        y - v.y
    });
}

/// "In one go": the group of days' pattern - whether it runs, its first and last departure and
/// how often (or its time bands), how long a bus stands at the end. Returns where it ends.
fn pattern_card(ui: &mut Ui, e: &mut PlanEdit, r: Rect, now: f32) -> f32 {
    let Some(o) = e.own.as_ref() else { return r.y };
    let day = e.day;
    let kind = o.draft.service;
    let Some(mut p) = o.draft.days.get(day).cloned() else { return r.y };
    let table_on = o.draft.table_on;
    let mut y = r.y;
    kit::caps(ui, Rect::new(r.x, y, r.w * 0.5, 16.0), "In one go");
    if !kind.allows(day) {
        let why = omsi_ui::tr("%{kind} does not run then").replace("%{kind}", &omsi_ui::tr(kind.label()));
        ui.text_in(&why, Rect::new(r.x, y + 22.0, r.w, 20.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
        return y + 44.0;
    }
    let mut what: Option<&str> = None;
    let mut on = p.on;
    if ui.toggle("pe-on", Rect::new(r.x + r.w * 0.45, y - 6.0, r.w * 0.55, 28.0), &mut on, "Runs on these days") {
        p.on = on;
        what = Some("pe-on");
    }
    y += 26.0;
    if p.on {
        let mut mode = usize::from(!p.bands.is_empty());
        if ui.segmented("pe-mode", Rect::new(r.x, y, r.w, 32.0), &mut mode, &["One headway", "Time bands"]) {
            p.bands = if mode == 1 { reg::default_bands(day) } else { Vec::new() };
            what = Some("pe-mode");
        }
        y += 40.0;
        let half = (r.w - GAP) * 0.5;
        if p.bands.is_empty() {
            ui.text_in("First", Rect::new(r.x, y, half, 16.0), 11.5, Weight::Bold, TEXT_DIM, Align::Left);
            ui.text_in("Last", Rect::new(r.x + half + GAP, y, half, 16.0), 11.5, Weight::Bold, TEXT_DIM, Align::Left);
            y += 18.0;
            let (mut first, mut last) = (p.first.round() as i32, p.last.round() as i32);
            if ui.time_field("pe-first", Rect::new(r.x, y, half, 34.0), &mut first) {
                p.first = first as f32;
                what = Some("pe-first");
            }
            if ui.time_field("pe-last", Rect::new(r.x + half + GAP, y, half, 34.0), &mut last) {
                p.last = last as f32;
                what = Some("pe-last");
            }
            y += 42.0;
            if ui.slider("pe-every", Rect::new(r.x, y, r.w, 28.0), &mut p.headway, 3.0, 120.0, 1.0, "Every", &|x| format!("{x:.0} min")) {
                what = Some("pe-every");
            }
            y += 32.0;
        } else {
            // the bands: from - to, how often (on one row where there is room, else on two)
            let one_row = r.w >= 560.0;
            let tw = if one_row { 136.0 } else { ((r.w - 16.0 - 30.0) * 0.5).max(118.0) };
            let mut remove = None;
            for (i, b) in p.bands.iter_mut().enumerate() {
                let (mut from, mut to) = (b.from.round() as i32, if b.to >= reg::DAY_END { 0 } else { b.to.round() as i32 });
                if ui.time_field(&format!("pe-band-from-{i}"), Rect::new(r.x, y, tw, 32.0), &mut from) {
                    b.from = from as f32;
                    what = Some("pe-band");
                }
                ui.text_in("–", Rect::new(r.x + tw, y, 16.0, 32.0), 14.0, Weight::Medium, TEXT_DIM, Align::Center);
                if ui.time_field(&format!("pe-band-to-{i}"), Rect::new(r.x + tw + 16.0, y, tw, 32.0), &mut to) {
                    // (00:00 at a band's end is midnight)
                    b.to = if to == 0 { reg::DAY_END } else { to as f32 };
                    what = Some("pe-band");
                }
                if ui.icon_button(&format!("pe-band-x-{i}"), Vec2::new(r.right() - 12.0, y + 16.0), 10.0, "close", "Take the band out") {
                    remove = Some(i);
                }
                let (sx, sy) = if one_row { (r.x + 2.0 * tw + 28.0, y + 2.0) } else { (r.x, y + 36.0) };
                if ui.slider(&format!("pe-band-every-{i}"), Rect::new(sx, sy, r.right() - sx - 30.0, 28.0), &mut b.headway, 3.0, 60.0, 1.0, if one_row { "" } else { "Every" }, &|x| format!("{x:.0} min")) {
                    what = Some("pe-band");
                }
                y += if one_row { 38.0 } else { 72.0 };
                if !one_row {
                    ui.p().rect(Rect::new(r.x, y - 4.0, r.w, 1.0), HAIRLINE);
                }
            }
            if let Some(i) = remove {
                p.bands.remove(i);
                what = Some("pe-band-x");
            }
            if ui.button("pe-band-add", Rect::new(r.x, y, r.w, 30.0), "Add a band", Some("add"), ButtonKind::Ghost) {
                let from = p.bands.iter().map(|b| b.to).fold(0.0, f32::max).min(reg::DAY_END - 60.0);
                p.bands.push(TimeBand { from, to: (from + 120.0).min(reg::DAY_END), headway: 30.0, size: None });
                what = Some("pe-band-add");
            }
            y += 36.0;
        }
        if ui.slider("pe-layover", Rect::new(r.x, y, r.w, 28.0), &mut p.layover, 0.0, 30.0, 1.0, "Stands", &|x| format!("{x:.0} min")) {
            what = Some("pe-layover");
        }
        y += 30.0;
        if table_on {
            y += ui.paragraph("A change here makes these days' trips anew from the pattern: the departures moved by hand on these days go.", Vec2::new(r.x, y), r.w, 11.5, Weight::Regular, TEXT_DIM) + 4.0;
        }
    }
    if let Some(what) = what {
        let table = e.own.as_ref().is_some_and(|o| o.draft.table_on);
        e.edit(now, what, |line| set_pattern(line, day, |x| *x = p));
        // (a table made anew: what was chosen in it is gone)
        if table {
            e.sel = None;
        }
    }
    y
}

/// What the group of days asks of the company now and as changed: trips, tours, the buses at
/// the busiest against the fleet's for the line, the drivers' duties, the kilometres; and what
/// changes in the tours of every day. Returns where it ends.
fn needs_card(ui: &mut Ui, e: &mut PlanEdit, c: &Company, cl: &CompanyLine, r: Rect) -> f32 {
    let Some(s) = e.shown.as_ref() else { return r.y };
    let editable = e.own.is_some();
    let mut y = r.y;
    kit::caps(ui, Rect::new(r.x, y, r.w * 0.5, 16.0), "What it needs");
    let (c1, c2) = (r.x + r.w * 0.58, r.x + r.w * 0.79);
    let col_w = r.w * 0.21;
    if editable {
        ui.text_in(&omsi_ui::tr("Now").to_uppercase(), Rect::new(c1, y, col_w, 16.0), kit::CAPS, Weight::Bold, TEXT_DIM, Align::Right);
        ui.text_in(&omsi_ui::tr("Changed").to_uppercase(), Rect::new(c2, y, col_w, 16.0), kit::CAPS, Weight::Bold, TEXT_DIM, Align::Right);
    }
    y += 22.0;
    let km = |x: f64| format!("{} km", super::super::grouped(x));
    let figures: [(&str, String, String, bool); 5] = [
        ("Trips", s.now.trips.to_string(), s.then.trips.to_string(), false),
        ("Tours", s.now.tours.to_string(), s.then.tours.to_string(), false),
        ("Buses at the busiest hour", s.now.buses.to_string(), s.then.buses.to_string(), s.then.buses > s.now.buses),
        ("Duties for drivers", s.now.duties.to_string(), s.then.duties.to_string(), s.then.duties > s.now.duties),
        ("Kilometres a day", km(s.now.km), km(s.then.km), false),
    ];
    for (label, a, b, more) in figures.iter() {
        ui.text_in(&omsi_ui::tr(label), Rect::new(r.x, y, r.w * 0.58, 22.0), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
        if editable {
            ui.text_in(a, Rect::new(c1, y, col_w, 22.0), 13.0, Weight::Medium, TEXT_DIM, Align::Right);
            ui.text_in(b, Rect::new(c2, y, col_w, 22.0), 13.0, Weight::Bold, if a != b { if *more { WARN } else { accent_2() } } else { TEXT }, Align::Right);
        } else {
            ui.text_in(b, Rect::new(c2, y, col_w, 22.0), 13.0, Weight::Bold, TEXT, Align::Right);
        }
        ui.p().rect(Rect::new(r.x, y + 23.0, r.w, 1.0), HAIRLINE);
        y += 25.0;
    }
    y += 4.0;
    // the fleet's buses the line may run with
    let have = c.fleet.iter().filter(|v| v.held_on(&c.date) && ownline::line_allows(c, &cl.name, v)).count();
    let (text, colour) = if s.then.buses > have {
        (omsi_ui::tr("At the busiest it needs %{n} buses; the fleet has %{have} for the line.").replace("%{n}", &s.then.buses.to_string()).replace("%{have}", &have.to_string()), WARN)
    } else {
        (omsi_ui::tr("The fleet has %{have} buses for the line.").replace("%{have}", &have.to_string()), TEXT_DIM)
    };
    y += ui.paragraph(&text, Vec2::new(r.x, y), r.w, 12.5, Weight::Regular, colour) + 4.0;
    if editable {
        let d = &s.diff;
        let tours = omsi_ui::tr("%{k} kept, %{c} changed, %{a} new, %{g} dropped").replace("%{k}", &d.kept.len().to_string()).replace("%{c}", &d.changed.len().to_string()).replace("%{a}", &d.added.len().to_string()).replace("%{g}", &d.gone.len().to_string());
        let line = format!("{}: {tours}", omsi_ui::tr("Tours of every day"));
        y += ui.paragraph(&line, Vec2::new(r.x, y), r.w, 12.5, Weight::Regular, TEXT_SOFT) + 2.0;
        let replan = d.replan();
        if !replan.is_empty() {
            let t = omsi_ui::tr("Given their bus and drivers anew: tours %{list}").replace("%{list}", &replan.join(", "));
            y += ui.paragraph(&t, Vec2::new(r.x, y), r.w, 12.5, Weight::Regular, WARN) + 2.0;
        }
    }
    y
}

/// The departures as a list, per direction: a time each, chosen with a tap. Returns where it
/// ends.
fn list_card(ui: &mut Ui, e: &mut PlanEdit, r: Rect) -> f32 {
    let Some(s) = e.shown.as_ref() else { return r.y };
    kit::caps(ui, Rect::new(r.x, r.y, r.w, 16.0), "As a list");
    let mut pick: Option<Pick> = None;
    let sel = e.sel;
    let editable = e.own.is_some();
    let mut y = r.y + 24.0;
    let (cw, ch, gap) = (54.0, 24.0, 5.0);
    let per = (((r.w + gap) / (cw + gap)).floor() as usize).max(1);
    for (k, dest) in s.lanes.iter().enumerate() {
        let mine: Vec<&Dep> = s.deps.iter().filter(|d| d.dir == k).collect();
        let way = lane_word(editable, k, dest);
        let head = if dest.is_empty() { way } else { format!("{way}  →  {dest}") };
        ui.p().rounded(Rect::new(r.x, y + 4.0, 4.0, 12.0), 2.0, lane_colour(Some(k)));
        ui.text_in(&head, Rect::new(r.x + 10.0, y, r.w - 10.0, 20.0), 12.5, Weight::Bold, TEXT_SOFT, Align::Left);
        y += 24.0;
        for (i, d) in mine.iter().enumerate() {
            let cr = Rect::new(r.x + (i % per) as f32 * (cw + gap), y + (i / per) as f32 * (ch + gap), cw, ch);
            if !ui.rect_visible(cr) {
                continue;
            }
            let on = sel.is_some_and(|p| d.is(&p));
            let (h, _, clicked) = ui.interact(id_of(&format!("pe-chip-{k}-{i}")), cr);
            ui.p().rounded(cr, 6.0, if on { lane_colour(Some(k)) } else if h { HOVER } else { FIELD });
            ui.text_in(&hm(d.dep), cr, 12.5, if on { Weight::Bold } else { Weight::Medium }, if on { on_accent() } else { TEXT }, Align::Center);
            ui.tooltip(cr, &format!("{} – {}", hm(d.dep), hm(d.arr)));
            if clicked {
                pick = Some(d.pick());
            }
        }
        y += mine.len().div_ceil(per) as f32 * (ch + gap) + 10.0;
    }
    if let Some(p) = pick {
        e.sel = if sel == Some(p) { None } else { Some(p) };
        e.text.clear();
    }
    y
}

/// A lane's word: an own line's way out and back, a map's line's trips (the third lane its
/// other trips).
fn lane_word(editable: bool, k: usize, dest: &str) -> String {
    let word = if editable {
        if k == 0 {
            "Outbound"
        } else {
            "Return"
        }
    } else if dest.is_empty() {
        "Other trips"
    } else {
        "Trips"
    };
    omsi_ui::tr(word).into_owned()
}

/// What the foot asked for.
enum Leave {
    Stay,
    Close,
    Save,
}

/// The foot: undo and start again; what is wrong, or from which day a change of a line in
/// service begins; save and close - and, closing with changes not saved, the question.
fn foot_row(l: &mut Launcher, e: &mut PlanEdit, c: &Company, cl: &CompanyLine, r: Rect, close: bool) -> Leave {
    let dirty = e.dirty();
    if close && !dirty {
        return Leave::Close;
    }
    if close {
        e.leaving = true;
    }
    let ui = &mut l.ui;
    if e.leaving {
        ui.text_in(&omsi_ui::tr("Close without saving the changes?"), Rect::new(r.x, r.y, r.w * 0.5, r.h), kit::BODY, Weight::Bold, WARN, Align::Left);
        let lw = 180.0;
        if ui.button("pe-leave", Rect::new(r.right() - lw, r.y, lw, r.h), "Close without saving", Some("close"), ButtonKind::Danger) {
            return Leave::Close;
        }
        if ui.button("pe-stay", Rect::new(r.right() - lw - 10.0 - 160.0, r.y, 160.0, r.h), "Keep changing", None, ButtonKind::Normal) {
            e.leaving = false;
        }
        return Leave::Stay;
    }
    let mut bx = r.right();
    let close_w = 116.0;
    if ui.button("pe-done", Rect::new(bx - close_w, r.y, close_w, r.h), "Close", None, ButtonKind::Normal) {
        if dirty {
            e.leaving = true;
            return Leave::Stay;
        }
        return Leave::Close;
    }
    bx -= close_w + 10.0;
    let Some(o) = e.own.as_ref() else {
        return Leave::Stay;
    };
    let problems = e.shown.as_ref().map(|s| s.problems.clone()).unwrap_or_default();
    let running = cl.service_from.is_some();
    let differs = reg::timetable_differs(&o.live, &o.draft);
    let later = running && differs;
    let save_label = if later { omsi_ui::tr("Save from %{date}").replace("%{date}", &short_day(&e.from)) } else { omsi_ui::tr("Save").into_owned() };
    let sw = (ui.width(&save_label, 13.0, Weight::Bold) + 13.0 * 1.3 + 6.0 + 44.0).max(130.0);
    let can = dirty && problems.is_empty();
    let mut out = Leave::Stay;
    if ui.button("pe-save", Rect::new(bx - sw, r.y, sw, r.h), &save_label, Some("save"), if can { ButtonKind::Primary } else { ButtonKind::Normal }) && dirty {
        match problems.first() {
            Some(p) => l.state.set_status(omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string()), true),
            None => out = Leave::Save,
        }
    }
    let ui = &mut l.ui;
    bx -= sw + 14.0;
    // a line in service: from which day (tomorrow at the soonest, two months at the most)
    if later {
        let days = co::dates::between(&c.date, &e.from);
        if days < 1 {
            e.from = co::dates::add(&c.date, 1);
        }
        let days = co::dates::between(&c.date, &e.from);
        let dw = 250.0;
        let dx = bx - dw;
        if ui.icon_button("pe-from-earlier", Vec2::new(dx + 14.0, r.center().y), 13.0, "chevron_left", "A day earlier") && days > 1 {
            e.from = co::dates::add(&e.from, -1);
        }
        let when = if days <= 1 { format!("{}  ({})", day_label(&e.from), omsi_ui::tr("tomorrow")) } else { day_label(&e.from) };
        ui.text_in(&format!("{} {when}", omsi_ui::tr("From")), Rect::new(dx + 30.0, r.y, dw - 60.0, r.h), 13.0, Weight::Bold, TEXT, Align::Center);
        if ui.icon_button("pe-from-later", Vec2::new(dx + dw - 14.0, r.center().y), 13.0, "chevron_right", "A day later") && days < 60 {
            e.from = co::dates::add(&e.from, 1);
        }
        bx = dx - 12.0;
    }
    // undo, start again
    let mut lx = r.x;
    let uw = ui.width(&omsi_ui::tr("Undo"), 13.0, Weight::Medium) + 13.0 * 1.3 + 6.0 + 28.0;
    if ui.button("pe-undo", Rect::new(lx, r.y, uw, r.h), "Undo", Some("history"), ButtonKind::Ghost) {
        if let (Some(prev), Some(o)) = (e.undo.pop(), e.own.as_mut()) {
            o.draft = prev;
            e.rev += 1;
            e.sel = None;
            e.last = (String::new(), f32::MIN);
        }
    }
    lx += uw + 6.0;
    let aw = ui.width(&omsi_ui::tr("Start again"), 13.0, Weight::Medium) + 13.0 * 1.3 + 6.0 + 28.0;
    if ui.button("pe-again", Rect::new(lx, r.y, aw, r.h), "Start again", Some("restart_alt"), ButtonKind::Ghost) && dirty {
        if let Some(o) = e.own.as_mut() {
            e.undo.push(o.draft.clone());
            o.draft = o.base.clone();
            e.rev += 1;
            e.sel = None;
        }
    }
    ui.tooltip(Rect::new(lx, r.y, aw, r.h), "The line's timetable as it is saved");
    lx += aw + 14.0;
    // what is wrong, or what a save does
    let (text, colour) = match problems.first() {
        Some(p) => (omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string()), WARN),
        None if !dirty => (omsi_ui::tr("No changes yet.").into_owned(), TEXT_DIM),
        None if later => (omsi_ui::tr("Today's tours run as they are.").into_owned(), TEXT_DIM),
        None => (omsi_ui::tr("Saved, it counts at once.").into_owned(), TEXT_DIM),
    };
    ui.text_in(&text, Rect::new(lx, r.y, (bx - lx).max(0.0), r.h), 12.5, Weight::Medium, colour, Align::Left);
    out
}

/// A day as short as a button wants it: "Tue 5 Jun".
fn short_day(date: &str) -> String {
    let full = day_label(date);
    // (the year off the end)
    match full.rsplit_once(' ') {
        Some((head, year)) if year.chars().all(|c| c.is_ascii_digit()) => head.to_string(),
        _ => full,
    }
}

// --- saving -----------------------------------------------------------------------------------------------

/// The change saved as the line editor saves a line's timetable: a line in service from the
/// day chosen (`ownline::schedule_change`), else at once (`ownline::apply_change`, its changed
/// tours' buses and drivers taken off the roster); then the registry, the map's timetable and
/// the depot files written, and the planning read again. Returns whether the sheet closes.
fn save(l: &mut Launcher, e: &mut PlanEdit) -> bool {
    let Some(o) = e.own.clone() else { return false };
    match commit(l, &e.company, &e.line, &o, &mut e.from) {
        Commit::Saved | Commit::Gone => true,
        Commit::Kept => false,
        Commit::Stale(on_disk) => {
            e.own = Some(own_of(o.id, o.path.clone(), o.stem.clone(), on_disk));
            (e.undo, e.sel, e.rev) = (Vec::new(), None, e.rev + 1);
            false
        }
    }
}

/// What came of saving an own line's timetable (`commit`).
pub(super) enum Commit {
    /// Saved (said in the status line).
    Saved,
    /// Not saved, and nothing to show any more (the company or its line gone).
    Gone,
    /// Not saved (why is said): the draft is kept.
    Kept,
    /// Not saved: the registry has the line otherwise now (as it is there) - changed in the
    /// line editor meanwhile, or its new timetable began.
    Stale(LineDesign),
}

/// The draft of an own line (`o`, the company `company`'s line `line`) saved as the line
/// editor saves a line's timetable (`save`; RosterFlow's timeplan saves through it too): a
/// line in service from the day `from` (moved to tomorrow when it is sooner), else at once.
pub(super) fn commit(l: &mut Launcher, company: &str, line: &str, o: &Own, from: &mut String) -> Commit {
    let Some(c) = l.company.company.clone().filter(|c| c.id == company) else { return Commit::Gone };
    let Some(cl) = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line)).cloned() else { return Commit::Gone };
    if let Some(p) = reg::problems(&o.draft).first() {
        l.state.set_status(omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string()), true);
        return Commit::Kept;
    }
    // (the line as the registry has it now: changed meanwhile - in the line editor, or its new
    // timetable began - it is shown as it is, and saved from there)
    let r = reg::load_registry(&o.path);
    let Some(on_disk) = r.line(o.id).cloned() else {
        l.state.set_status(omsi_ui::tr(NOT_FOUND).into_owned(), true);
        return Commit::Gone;
    };
    if on_disk != o.saved {
        kit::show(l, kit::Popup::new("departure_board", "Changed meanwhile", omsi_ui::tr("The line's timetable was changed meanwhile - in the line editor, or its new timetable began: it is shown as it is now."), "", None));
        return Commit::Stale(on_disk);
    }
    let Some(lines) = map_lines(&l.company).map(|x| x.to_vec()) else {
        kit::show(l, kit::Popup::new("timer", "Not yet", omsi_ui::tr("The timetable of the day is still being read."), omsi_ui::tr("Try again in a moment."), None));
        return Commit::Kept;
    };
    if core::content_dir().is_none() {
        l.state.set_status(omsi_ui::tr("There is no content folder to write the timetable into.").into_owned(), true);
        return Commit::Kept;
    }
    let sl = ownline::StopLines::of(&lines, &o.stem);
    let shape = ownline::shape_of(&o.draft, &|s| sl.count(s));
    let diff = reg::timetable_differs(&o.live, &o.draft).then(|| reg::tour_diff(&o.live, &o.draft));
    let number = o.draft.number.trim().to_string();
    let (draft, stem) = (o.draft.clone(), o.stem.clone());
    let (line, text) = match &diff {
        // a line in service: from the day chosen, today's tours as they are
        Some(d) if cl.service_from.is_some() => {
            if co::dates::between(&c.date, from) < 1 {
                *from = co::dates::add(&c.date, 1);
            }
            let from = from.clone();
            let replan = d.replan();
            let Some(paid) = act(l, |c| ownline::schedule_change(c, &draft, &stem, &shape, &from, replan)) else { return Commit::Kept };
            let t = omsi_ui::tr("Line %{n}: the new timetable begins on %{date}; today's tours run as they are.").replace("%{n}", &number).replace("%{date}", &day_label(&from)) + &approved(paid);
            (LineDesign { live: Some(Box::new(o.live.clone())), pending_from: from, draft: false, ..draft }, t)
        }
        // else at once: the tours it changed get their buses and drivers anew
        _ => {
            let replan = diff.as_ref().map(|d| d.replan()).unwrap_or_default();
            let anew = replan.len() + diff.as_ref().map(|d| d.added.len()).unwrap_or(0);
            let (d2, s2) = (draft.clone(), stem.clone());
            let paid = act(l, |c| {
                let paid = ownline::apply_change(c, &d2, &s2, &shape)?;
                co::plan::forget_tours(c, &s2, &replan);
                Ok(paid)
            });
            let Some(paid) = paid else { return Commit::Kept };
            let t = if anew > 0 { omsi_ui::tr("Line %{n} saved: %{k} of its tours to plan anew.").replace("%{k}", &anew.to_string()) } else { omsi_ui::tr("Line %{n} saved.").into_owned() }.replace("%{n}", &number) + &approved(paid);
            (LineDesign { live: None, pending_from: String::new(), draft: false, ..draft }, t)
        }
    };
    let result = write_line(l, &c.map, &o.path, line);
    reload_timetable(l);
    match result {
        Ok(None) => l.state.set_status(text, false),
        Ok(Some(depots)) => l.state.set_status(format!("{text} {}: {depots}", omsi_ui::tr("A depot file could not be written (displays and IBIS)")), true),
        Err(err) => l.state.set_status(format!("{}: {err}", omsi_ui::tr("The timetable files could not be written")), true),
    }
    Commit::Saved
}

/// What a change paid for its route's approval, said after it (nothing for a timetable alone:
/// the route is the line editor's).
pub(super) fn approved(paid: co::Cents) -> String {
    if paid > 0 {
        format!(" {}", omsi_ui::tr("Its route is approved for %{amount}.").replace("%{amount}", &super::super::eur(paid)))
    } else {
        String::new()
    }
}

/// A line written into the map's registry, its timetable and its depot files as the line
/// editor writes them (the codes given before the registry is saved, the blocks after the
/// timetable); the line editor open on the map takes it too. Returns a depot file's error.
pub(super) fn write_line(l: &mut Launcher, map: &str, path: &Path, mut line: LineDesign) -> Result<Option<String>, String> {
    let content = core::content_dir().ok_or_else(|| "no content folder".to_string())?;
    let mut r = reg::load_registry(path);
    let id = line.id;
    line.modified = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let Some(x) = r.line_mut(id) else { return Err(omsi_ui::tr(NOT_FOUND).into_owned()) };
    *x = line;
    let global = omsi_cfg::find_in_roots(map).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(Path::new(&l.state.config.root), map));
    let map_dir = global.parent().map(Path::to_path_buf).unwrap_or_default();
    let groups = linehof::group_depots(&map_dir);
    let (bases, original) = core::depot_roots();
    let plan = linehof::prepare(&mut r, &groups, &bases);
    owndepot::assign(&mut r, &owndepot::dir(), &groups, &bases);
    reg::save_registry(path, &r)?;
    let exported = reg::export_to_map(&content, &map_dir, &r);
    let (_, err) = linehof::write(&content, original.as_deref(), &r, &groups, &plan);
    let (_, own_err) = owndepot::write_lines(&r, &owndepot::dir(), Some(&content));
    let err = err.or(own_err);
    if err.is_none() && r.texts < reg::TEXTS_VERSION {
        r.texts = reg::TEXTS_VERSION;
        if let Err(e) = reg::save_registry(path, &r) {
            log::warn!("planning: the registry {} not saved after its depot files: {e}", path.display());
        }
    }
    lineeditor::took_line(l, path, &r, id);
    omsi_cfg::content_changed();
    l.state.load_lines();
    exported?;
    Ok(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::lines::{Direction, Leg, StopRef};

    /// A line of six stops out and back (about two minutes a stop), every 20 minutes from six
    /// to eight on working days.
    fn line() -> LineDesign {
        let dir = |rev: bool| {
            let mut stops: Vec<StopRef> = (0..6).map(|i| StopRef { id: i + 1, name: format!("Stop {i}"), ..Default::default() }).collect();
            if rev {
                stops.reverse();
            }
            let legs = (0..5).map(|_| Leg { length: 600.0, ok: true, ..Default::default() }).collect();
            let mut d = Direction { stops, legs, ..Default::default() };
            d.refresh_times();
            d
        };
        let mut l = LineDesign { id: 4, number: "4".into(), ai_group: "Busse".into(), directions: vec![dir(false), dir(true)], ..Default::default() };
        l.days[0] = DayPattern { days: DAY_GROUPS[0].1, first: 360.0, last: 480.0, headway: 20.0, layover: 5.0, ..Default::default() };
        l
    }

    #[test]
    fn times_are_typed_and_shown() {
        assert_eq!(hm(425.0), "07:05");
        assert_eq!(hm(1445.0), "00:05");
        assert_eq!(parse_hm("0705"), Some(425));
        assert_eq!(parse_hm("7:05"), Some(425));
        assert_eq!(parse_hm("25:00"), None);
        assert!(whole_time("07:05") && whole_time("0705") && !whole_time("07:0"));
        assert_eq!(typed_minutes(5, 1430.0), 1445.0);
        assert_eq!(typed_minutes(425, 420.0), 425.0);
    }

    #[test]
    fn a_departure_is_moved_added_and_taken_out_as_a_trip_of_the_table() {
        let mut l = line();
        // the pattern's: seven departures a direction, both ways
        let d = deps(&l, 0);
        assert_eq!(d.len(), 14);
        assert!(d.iter().all(|x| x.at.is_none() && x.arr > x.dep));
        assert!(deps(&l, 1).len() > 0, "Saturday as a new line has it");
        // moved: the timetable a table now, the trip with its stops later as a whole
        let p = d.iter().find(|x| x.dir == 1 && x.dep == 400.0).unwrap().pick();
        let q = move_dep(&mut l, 0, p, 407.0).unwrap();
        assert!(l.table_on && q.at.is_some() && q.dep == 407.0);
        let t = &l.table[q.at.unwrap()];
        assert_eq!((t.dir, t.day), (1, 0));
        assert!(t.times.iter().all(|x| *x >= 407.0) && t.first_fall().is_none());
        // the other groups' trips are the patterns' still
        assert_eq!(deps(&l, 1).len(), {
            let mut fresh = line();
            fresh.table_on = false;
            deps(&fresh, 1).len()
        });
        // a trip added between two: the times of its neighbour's, the minute rounded
        let a = add_dep(&mut l, 0, 0, 409.6).unwrap();
        assert_eq!(a.dep, 410.0);
        assert_eq!(deps(&l, 0).iter().filter(|x| x.dir == 0).count(), 8);
        let added = &l.table[a.at.unwrap()];
        let near = l.table.iter().find(|t| t.dir == 0 && t.day == 0 && t.departure() == 400.0).unwrap();
        assert_eq!(added.relative(), near.relative());
        // taken out, by its minute when the table changed meanwhile
        assert!(remove_dep(&mut l, 0, Pick { dir: 0, dep: 410.0, at: None }));
        assert_eq!(deps(&l, 0).iter().filter(|x| x.dir == 0).count(), 7);
        assert!(!remove_dep(&mut l, 0, Pick { dir: 0, dep: 999.0, at: None }));
        // not before midnight
        let first = deps(&l, 0)[0].pick();
        assert_eq!(move_dep(&mut l, 0, first, -30.0).unwrap().dep, 0.0);
    }

    #[test]
    fn the_pattern_in_one_go_fills_its_days_anew_and_keeps_the_others() {
        let mut l = line();
        // as patterns: the headway changes the departures at once
        set_pattern(&mut l, 0, |p| p.headway = 30.0);
        assert_eq!(deps(&l, 0).iter().filter(|x| x.dir == 0).count(), 5);
        // as a table: these days' trips made anew, a Saturday moved by hand kept
        let sat = deps(&l, 1)[0].pick();
        move_dep(&mut l, 1, sat, sat.dep + 3.0).unwrap();
        let moved = deps(&l, 1)[0].dep;
        set_pattern(&mut l, 0, |p| p.last = 540.0);
        assert_eq!(deps(&l, 0).iter().filter(|x| x.dir == 0).count(), 7);
        assert_eq!(deps(&l, 1)[0].dep, moved);
        // a change that leaves the departures as they are (how long a bus stands) keeps the table
        let before = l.table.clone();
        set_pattern(&mut l, 0, |p| p.layover = 9.0);
        assert_eq!(l.table, before);
        assert_eq!(l.days[0].layover, 9.0);
    }

    #[test]
    fn the_axis_shows_the_trips_hours() {
        let s = |deps: Vec<Dep>| Shown { key: (0, 0), lanes: vec![String::new()], deps, rows: Vec::new(), now: Needs::default(), then: Needs::default(), diff: TourDiff::default(), problems: Vec::new() };
        let d = |dep: f32, arr: f32| Dep { dir: 0, dep, arr, at: None };
        assert_eq!(span_of(&s(vec![d(365.0, 400.0), d(1300.0, 1340.0)])), (5 * 60, 23 * 60));
        // six hours at the least, and nothing: the day's working hours
        assert_eq!(span_of(&s(vec![d(600.0, 630.0)])), (9 * 60, 15 * 60));
        assert_eq!(span_of(&s(Vec::new())), (5 * 60, 24 * 60));
        let ax = Axis { x: 100.0, w: 600.0, t0: 360, t1: 960 };
        assert_eq!(ax.x_of(660.0), 400.0);
        assert_eq!(ax.minute(400.0), 660.0);
        // (not before midnight, nor past the axis's end)
        assert_eq!(Axis { t0: 0, ..ax }.minute(50.0), 0.0);
        assert_eq!(ax.minute(2000.0), 959.0);
        // (a zoomed axis from 11:20: its hours from 12 on)
        assert_eq!(Axis { t0: 680, t1: 860, ..ax }.hours(1), vec![12, 13, 14]);
        assert_eq!(ax.hours(2), vec![6, 8, 10, 12, 14, 16]);
        // zoomed: within the whole day, two hours at the least, the whole day none
        assert_eq!(fit_view((200, 260), (300, 1440)), Some((300, 420)));
        assert_eq!(fit_view((1400, 1500), (300, 1440)), Some((1320, 1440)));
        assert_eq!(fit_view((300, 1440), (300, 1440)), None);
    }

    #[test]
    fn the_sheets_texts_are_translated() {
        let keys = [
            MAP_LINE,
            NOT_FOUND,
            "Trips and timetable of line %{n}",
            "Drag a departure to move it · click the axis to add one · Delete takes the one chosen out · the wheel zooms",
            "The wheel over the departures zooms into the hours",
            "Whole day",
            "%{n} departures",
            "Other trips",
            "Tours: the buses it needs (%{n})",
            "loses %{bus}",
            "Buses at the busiest hour",
            "Duties for drivers",
            "Kilometres a day",
            "Tours of every day",
            "Given their bus and drivers anew: tours %{list}",
            "Save from %{date}",
            "Line %{n} saved: %{k} of its tours to plan anew.",
            "Line %{n} saved.",
            "changed",
            "new",
            "dropped",
        ];
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in keys {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }
}
