//! The company's planning (Omsi-Hub's Planning page, `busbedrijf-planning.md`): a strip of
//! the next seven days, and the chosen day as a Gantt chart - the tours grouped by line, each
//! tour's duties as blocks coloured by who drives them, the bus beside the tour. A driver or
//! a bus is given by tapping it in the side list and then the duty or the tour, or by
//! dragging it there; tapping a duty or a tour shows who could take it. What is given goes
//! into the weekly roster, which repeats every week (`co::plan`); what fell out on the
//! company's own day is filled for that day only. The tools fill the roster as the
//! dispatcher would, clear the day, or repeat it on the other working days. "Drive this
//! duty" opens the Drive page with exactly that duty (as the Career tab's "My duties" does).
//! A line that asks for buses of its own (the line editor's Kind tab) is given only those:
//! the dispatcher takes none other, and a bus that is none of them is refused.
//!
//! The chart fits its room: a tour's row is as tall as the rows shown share, the blocks' words
//! as their size allows (`row_pitch`); a line folds to its heading - what it has planned, what
//! is open - and back, every line at once too, kept with the company (`plan::set_collapsed`).
//! A line's heading, or one of its tours tapped twice, opens its trips and timetable over the
//! planning (`plan_edit`).
//!
//! Beside the roster, the same day in the scheduling studio (`studio`; Luc: "like Optibus
//! Scheduling"): the Vehicles view - a row per bus's block, its trips moved between buses - and
//! the Drivers view - a row per duty, cut and joined - switched with the roster on the tool
//! row.

mod plan_edit;
mod rf_brand;
mod rf_gantt;
mod rf_panels;
mod rf_prefs;
mod rf_print;
mod rf_roster;
mod rf_scroll;
mod rf_tutorial;
mod rosterflow;
mod timeplan;

/// RosterFlow's tabs, for its tour (`company::tutorial`).
pub(super) const RF_ROSTER: usize = rosterflow::ROSTER;
pub(super) const RF_VEHICLES: usize = rosterflow::VEHICLES;
pub(super) const RF_DRIVERS: usize = rosterflow::DRIVERS;
pub(super) const RF_TIMEPLAN: usize = rosterflow::TIMEPLAN;

/// RosterFlow's tab and the timeplan's screen now.
pub(super) fn rf_view(l: &Launcher) -> (usize, usize) {
    (l.company.planning.view, l.company.planning.timeplan.screen())
}

/// RosterFlow on tab `view`, the timeplan on screen `screen` (the tour's; what is in hand let go
/// of when the tab changes).
pub(super) fn rf_set_view(l: &mut Launcher, view: usize, screen: Option<usize>) {
    let p = &mut l.company.planning;
    if p.view != view.min(3) {
        p.view = view.min(3);
        p.sel = None;
        p.arm = None;
        p.rf.settle();
        p.rf.panel = rosterflow::Panel::None;
    }
    if let Some(s) = screen {
        p.timeplan.set_screen(s);
    }
}

/// A drawing of RosterFlow's for the tour's cards (`rf_tutorial::picture`).
pub(super) fn rf_picture(ui: &mut Ui, which: u8, r: Rect, a: f32, t: Option<f32>) {
    rf_tutorial::picture(ui, which, r, a, t);
}

use super::super::flow::Step;
use super::super::state::{CompanyDuty, PlannedBus};
use super::super::theme::*;
use super::super::ui::{id_of, ButtonKind, Key, Ui};
use super::super::{Launcher, Page};
use super::kit::{self, Foot};
use super::{act, day_label, eur, Dialog};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::plan::{self as pl, BusOf, DayPlan, DayTour, Disruption, Fill, Problem, Source, Warn, Who};
use omsi_launcher_lib::company::staff::Block;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::sync::mpsc::{channel, Receiver, Sender};

/// A day's timetable as it was read: map, date, its lines.
type Read = (String, String, Result<Vec<core::LineInfo>, String>);

/// A driver or a bus taken up to be given (tapped, or dragged).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Arm {
    Driver(Who),
    Bus(u32),
}

#[derive(Clone, PartialEq, Debug)]
enum Sel {
    Tour(String, String),
    Duty(String, String, usize),
}

pub struct PlanningView {
    tx: Sender<Read>,
    rx: Receiver<Read>,
    days: Vec<Read>,
    asked: Vec<(String, String)>,
    /// Days read besides the week: the service dialog's seven, from the day the line starts
    /// (kept while the dialog is open).
    extra: Vec<String>,
    /// The day shown: 0 the company's day, up to 6.
    pub(super) day: usize,
    sel: Option<Sel>,
    arm: Option<Arm>,
    /// Pressed on a driver or a bus of the list: dragged while the button is held.
    drag: Option<Arm>,
    /// A duty or a tour carried to a driver or a bus of the list (Luc: everything in the
    /// planning can be dragged): taken up from the open duties at once, from the chart once
    /// the mouse moved off where it was pressed (`press`) - a tap there still chooses it.
    carry: Option<(usize, Option<usize>)>,
    press: Option<(Vec2, usize, Option<usize>)>,
    /// Where the carry was taken up (its label shows once the mouse is off it).
    carry_from: Vec2,
    /// The list's drivers and buses where they are drawn this frame (to drop on).
    targets: Vec<(Rect, Arm)>,
    /// "Clear the day" pressed once: pressed again it clears.
    clear_armed: bool,
    /// The plans made: date, the company's generation they were made for, the plan.
    cache: Vec<(String, u64, DayPlan)>,
    /// The timetable's revision the days were read at (`CompanyView::timetable`: the line
    /// editor wrote it since - an own line's tours were missing until it was read again).
    read_at: u64,
    /// A line to show (from the Lines page): its group scrolled into view once.
    pub(super) focus: Option<String>,
    /// A line's trips and timetable open over the planning (`plan_edit`).
    edit: Option<plan_edit::PlanEdit>,
    /// The bus or duty tapped last, and when (a second tap soon after opens its timetable).
    tapped: Option<(String, f32)>,
    /// What the page shows (RosterFlow's tabs): 0 the roster, 1 Vehicles, 2 Drivers, 3 Timeplan.
    pub(super) view: usize,
    rf: rosterflow::Rf,
    /// RosterFlow's timeplan (`timeplan`), its Timeplan tab.
    timeplan: timeplan::TimeplanView,
}

impl Default for PlanningView {
    fn default() -> Self {
        let (tx, rx) = channel();
        PlanningView { tx, rx, days: Vec::new(), asked: Vec::new(), extra: Vec::new(), day: 0, sel: None, arm: None, drag: None, carry: None, press: None, carry_from: Vec2::ZERO, targets: Vec::new(), clear_armed: false, cache: Vec::new(), read_at: 0, focus: None, edit: None, tapped: None, view: rosterflow::VEHICLES, rf: Default::default(), timeplan: Default::default() }
    }
}

impl PlanningView {
    /// Something is in hand - a driver or a bus armed, a duty or a tour chosen or carried, a
    /// line's timetable open over the planning, a trip taken up or a proposal shown in the
    /// studio - that Escape lets go of first (before it goes back to the company's start
    /// screen).
    pub(super) fn holds_choice(&self) -> bool {
        self.sel.is_some() || self.arm.is_some() || self.drag.is_some() || self.carry.is_some() || self.edit.is_some() || (self.view > 0 && self.rf.holds()) || (self.view == rosterflow::TIMEPLAN && self.timeplan.holds()) || self.rf.tut.holds()
    }

    /// Another page of the company is shown: RosterFlow opens with its opening again.
    pub(super) fn away(&mut self) {
        self.rf.shown = false;
    }

    /// Forget the week read: it is read again (the timetable was written).
    fn forget(&mut self, revision: u64) {
        self.days.clear();
        self.asked.clear();
        self.cache.clear();
        self.read_at = revision;
        // (a read still on its way is of the old timetable: a fresh channel leaves it)
        let (tx, rx) = channel();
        (self.tx, self.rx) = (tx, rx);
    }
}

/// Where a block or a bus is in the chart (for dropping on it).
struct Hit {
    r: Rect,
    tour: usize,
    duty: Option<usize>,
}

fn hhmm(minutes: i32) -> String {
    super::super::state::hhmm(minutes as f64 * 60.0)
}

fn length(minutes: i32) -> String {
    format!("{} h {:02}", minutes / 60, minutes % 60)
}

/// The colours drivers are told apart by (calm, and readable under dark ink).
const DRIVERS: [(u8, u8, u8); 10] = [(110, 168, 230), (96, 196, 170), (196, 150, 214), (226, 184, 110), (140, 190, 120), (226, 150, 130), (150, 206, 240), (200, 200, 120), (180, 160, 230), (120, 200, 210)];
const INK: Color = Color::rgba(14, 18, 28, 1.0);

fn driver_colour(c: &Company, id: u32) -> Color {
    let k = c.staff.iter().position(|e| e.id == id).unwrap_or(id as usize);
    let (r, g, b) = DRIVERS[k % DRIVERS.len()];
    Color::rgba(r, g, b, 1.0)
}

fn first_name(c: &Company, id: u32) -> String {
    c.employee(id).map(|e| e.name.split_whitespace().next().unwrap_or(&e.name).to_string()).unwrap_or_else(|| "?".into())
}

fn who_name(c: &Company, w: Option<Who>) -> String {
    match w {
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.clone()).unwrap_or_else(|| "?".into()),
        Some(Who::Player) => omsi_ui::tr("You").into_owned(),
        Some(Who::Agency) => omsi_ui::tr("Agency driver").into_owned(),
        None => omsi_ui::tr("Nobody").into_owned(),
    }
}

fn source_text(s: Source) -> &'static str {
    match s {
        Source::Roster => "fixed in the roster",
        Source::Auto => "the dispatcher's suggestion",
        Source::Dispatcher => "your choice for today",
        Source::Central => "filled by the central",
        Source::None => "",
    }
}

fn problem_colour(p: Option<Problem>) -> Color {
    match p {
        None | Some(Problem::Unassigned) => WARN,
        Some(_) => DANGER.lighten(0.2),
    }
}

fn line_title(t: &DayTour, duty: Option<usize>) -> String {
    let mut s = format!("{} {}  ·  {} {}", omsi_ui::tr("Line"), t.tour.number, omsi_ui::tr("Tour"), t.tour.tour);
    if let Some(k) = duty {
        s.push_str(&format!("  ·  {} {}", omsi_ui::tr("Duty"), k + 1));
    }
    s
}

// --- the data --------------------------------------------------------------------------------

/// Take in the timetables read, and ask for those of the week not read yet (and, while the
/// service dialog is open, of the line's week it shows).
pub(super) fn work(l: &mut Launcher, c: &Company) {
    let revision = l.company.timetable;
    let service = matches!(l.company.dialog, Some(Dialog::Service { .. }));
    let v = &mut l.company.planning;
    if v.read_at != revision {
        v.forget(revision);
    }
    if !service {
        v.extra.clear();
    }
    while let Ok(r) = v.rx.try_recv() {
        v.days.retain(|d| !(d.0 == r.0 && d.1 == r.1));
        v.days.push(r);
    }
    let mut week: Vec<(String, String)> = (0..7).map(|k| (c.map.clone(), co::dates::add(&c.date, k))).collect();
    for date in &v.extra {
        let day = (c.map.clone(), date.clone());
        if !week.contains(&day) {
            week.push(day);
        }
    }
    v.days.retain(|d| week.iter().any(|w| w.0 == d.0 && w.1 == d.1));
    v.asked.retain(|a| week.contains(a));
    let missing: Vec<(String, String)> = week.into_iter().filter(|w| !v.asked.contains(w)).collect();
    if missing.is_empty() {
        return;
    }
    v.asked.extend(missing.iter().cloned());
    let tx = v.tx.clone();
    std::thread::spawn(move || {
        for (map, date) in missing {
            let r = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"));
            if tx.send((map, date, r)).is_err() {
                break;
            }
        }
    });
}

/// The plan of a day of the strip (None: its timetable is still being read).
pub(super) fn plan_of(l: &mut Launcher, c: &Company, date: &str) -> Option<Result<DayPlan, String>> {
    let generation = l.company.generation;
    let v = &mut l.company.planning;
    if let Some(p) = v.cache.iter().find(|x| x.0 == date && x.1 == generation) {
        return Some(Ok(p.2.clone()));
    }
    let read = v.days.iter().find(|d| d.0 == c.map && d.1 == date)?;
    match &read.2 {
        Err(e) => Some(Err(e.clone())),
        Ok(lines) => {
            let tours = co::network::tours_of_day(c, lines, date);
            let p = pl::day_plan(c, date, tours, &[], &[], false);
            v.cache.retain(|x| x.1 == generation && x.0 != date);
            v.cache.push((date.to_string(), generation, p.clone()));
            Some(Ok(p))
        }
    }
}

/// Tomorrow's plan, for the overview's warnings (None: its timetable is still being read).
pub(super) fn tomorrow(l: &mut Launcher, c: &Company) -> Option<co::day::Plan> {
    work(l, c);
    let date = co::dates::add(&c.date, 1);
    plan_of(l, c, &date)?.ok().map(|p| p.to_plan())
}

/// The blocks a driver has on the plan's day, without one duty (to see whether it fits).
fn blocks_without(p: &DayPlan, id: u32, skip: (usize, usize)) -> Vec<Block> {
    p.duties_of(Who::Staff(id))
        .into_iter()
        .filter(|x| *x != skip)
        .map(|(i, k)| {
            let t = &p.tours[i];
            let d = &t.duties[k];
            Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
        })
        .collect()
}

fn block_of(t: &DayTour, k: usize) -> Block {
    let d = &t.duties[k];
    Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
}

/// What a driver would make of a duty: a word and its colour.
fn fit_of(c: &Company, p: &DayPlan, ti: usize, k: usize, id: u32) -> (String, Color, bool) {
    let Some(e) = c.employee(id) else { return ("?".into(), TEXT_FAINT, false) };
    if !e.employed_on(&p.date) {
        return (omsi_ui::tr("Not employed then").into_owned(), TEXT_FAINT, false);
    }
    if e.sick_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
        return (omsi_ui::tr("Ill").into_owned(), DANGER.lighten(0.2), false);
    }
    if e.holiday_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
        return (omsi_ui::tr("On holiday").into_owned(), TEXT_DIM, false);
    }
    let t = &p.tours[ti];
    let work = blocks_without(p, id, (ti, k));
    let bus = t.bus_of(c);
    match pl::fits_bus(e, &bus, &work, &block_of(t, k), p.today) {
        Err(Problem::Licence | Problem::NoEndorsement | Problem::NoTypeTraining) => {
            let why = co::licences::lack(e, bus.0, bus.1.as_deref()).map(|x| super::people::lack_text(c, &x)).unwrap_or_else(|| omsi_ui::tr("No licence for this bus").into_owned());
            (why, DANGER.lighten(0.2), false)
        }
        Err(_) => (omsi_ui::tr("Clashes with other work").into_owned(), DANGER.lighten(0.2), false),
        Ok(w) => match w.first() {
            Some(Warn::Overtime(m)) => (omsi_ui::tr("Overtime %{t}").replace("%{t}", &length(*m)), WARN, true),
            Some(Warn::Transfer(m)) => (omsi_ui::tr("%{n} min to change stops").replace("%{n}", &m.to_string()), WARN, true),
            Some(Warn::Experience) => (omsi_ui::tr("Little experience for this bus").into_owned(), WARN, true),
            None => (omsi_ui::tr("Fits").into_owned(), OK, true),
        },
    }
}

/// A duty's problem in words; for a driver not qualified for the bus, what they lack
/// ("No type training for the Citaro").
fn problem_text(c: &Company, p: &DayPlan, ti: usize, k: usize, problem: Option<Problem>) -> String {
    let t = &p.tours[ti];
    if matches!(problem, Some(Problem::Licence | Problem::NoEndorsement | Problem::NoTypeTraining)) {
        let rostered = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
        if let Some(e) = rostered.and_then(|w| if let Who::Staff(id) = w { c.employee(id) } else { None }) {
            let (kind, bus) = t.bus_of(c);
            if let Some(x) = co::licences::lack(e, kind, bus.as_deref()) {
                return format!("{}: {}", first_name(c, e.id), super::people::lack_text(c, &x));
            }
        }
    }
    omsi_ui::tr(problem.map(Problem::label).unwrap_or("No driver")).into_owned()
}

/// Where a bus is on the plan's day besides this tour: (word, colour, free).
fn bus_fit(c: &Company, p: &DayPlan, ti: usize, id: u32) -> (String, Color, bool) {
    let Some(v) = c.vehicle(id) else { return ("?".into(), TEXT_FAINT, false) };
    if !v.held_on(&p.date) {
        return (omsi_ui::tr("Gone back then").into_owned(), TEXT_FAINT, false);
    }
    if v.in_workshop(&p.date) || v.condition < 20.0 {
        return (omsi_ui::tr("In the workshop").into_owned(), DANGER.lighten(0.2), false);
    }
    let t = &p.tours[ti];
    if !co::ownline::line_allows(c, &t.tour.line, v) {
        return (omsi_ui::tr("Not one of the line's buses").into_owned(), DANGER.lighten(0.2), false);
    }
    let (a, b) = (t.tour.from(), t.tour.to());
    let other = p.tours.iter().enumerate().find(|(i, x)| *i != ti && x.bus == Some(BusOf::Own(id)) && a < x.tour.to() + co::staff::BUS_MARGIN && x.tour.from() < b + co::staff::BUS_MARGIN);
    if let Some((_, x)) = other {
        return (omsi_ui::tr("On tour %{t} then").replace("%{t}", &format!("{}/{}", x.tour.number, x.tour.tour)), WARN, false);
    }
    if t.tour.wants().is_some_and(|w| w != v.kind.size) {
        return (omsi_ui::tr("Another size than the tour asks for").into_owned(), WARN, true);
    }
    (omsi_ui::tr("Free").into_owned(), OK, true)
}

// --- giving --------------------------------------------------------------------------------

/// Give a duty its driver: into the roster, or - for one that fell out today with somebody
/// fixed for it - for today only.
fn give_driver(l: &mut Launcher, p: &DayPlan, ti: usize, k: usize, who: Option<Who>) {
    let Some(c) = l.company.company.as_ref() else { return };
    let t = &p.tours[ti];
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    let rostered = pl::roster(c, p.weekday, &line, &tour).and_then(|r| r.duties.get(k).copied().flatten());
    let today_only = p.today && t.duties[k].problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some() && who != Some(Who::Player);
    let wd = p.weekday;
    if today_only {
        let fill = match who {
            Some(Who::Staff(id)) => Some(Fill::Colleague { id }),
            Some(Who::Agency) => Some(Fill::Agency),
            _ => Some(Fill::Drop),
        };
        let key = pl::duty_key(&line, &tour, k);
        act(l, |c| {
            pl::set_fill(c, &key, fill);
            Ok(())
        });
    } else {
        // (a piece of a duty joined in the studio: its other pieces go to the same driver)
        let blocks: Vec<co::network::TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
        let others: Vec<(String, String, usize)> = co::studio::partners(c, wd, &blocks, ti, k).into_iter().map(|(i, k)| (p.tours[i].tour.line.clone(), p.tours[i].tour.tour.clone(), k)).collect();
        act(l, |c| {
            pl::set_driver(c, wd, &line, &tour, k, who);
            for (line, tour, k) in &others {
                pl::set_driver(c, wd, line, tour, *k, who);
            }
            Ok(())
        });
    }
}

fn give_bus(l: &mut Launcher, p: &DayPlan, ti: usize, bus: Option<u32>) {
    let Some(c) = l.company.company.as_ref() else { return };
    let t = &p.tours[ti];
    let (line, tour) = (t.tour.line.clone(), t.tour.tour.clone());
    // (a line that asks for buses of its own runs with none other)
    if let Some((v, want)) = bus.and_then(|id| c.vehicle(id)).zip(co::ownline::vehicles_of(c, &line)).filter(|(v, _)| !co::ownline::line_allows(c, &line, v)) {
        let text = omsi_ui::tr("Line %{n} runs with %{buses}: bus %{bus} is none of them.").replace("%{n}", &t.tour.number).replace("%{buses}", &want.summary(&|s| omsi_ui::tr(s).into_owned())).replace("%{bus}", &v.number);
        kit::show(l, kit::Popup::new("directions_bus", "Not one of the line's buses", text, omsi_ui::tr("Buy or lease one of them, or choose other buses for the line in the line editor."), None));
        return;
    }
    let rostered = pl::roster(c, p.weekday, &line, &tour).and_then(|r| r.bus);
    let today_only = p.today && t.bus_problem.is_some_and(|x| x != Problem::Unassigned) && rostered.is_some();
    let wd = p.weekday;
    if today_only {
        let fill = Some(bus.map(|id| Fill::Bus { id }).unwrap_or(Fill::Drop));
        let key = pl::tour_key(&line, &tour);
        act(l, |c| {
            pl::set_fill(c, &key, fill);
            Ok(())
        });
    } else {
        act(l, |c| {
            pl::set_bus(c, wd, &line, &tour, bus);
            Ok(())
        });
    }
}

fn set_fill(l: &mut Launcher, key: String, fill: Option<Fill>) {
    act(l, |c| {
        pl::set_fill(c, &key, fill);
        Ok(())
    });
}

/// What was taken up given to what is under it.
fn give(l: &mut Launcher, p: &DayPlan, arm: Arm, hit: &Hit) {
    match (arm, hit.duty) {
        (Arm::Driver(w), Some(k)) => give_driver(l, p, hit.tour, k, Some(w)),
        (Arm::Bus(id), None) => give_bus(l, p, hit.tour, Some(id)),
        // (a bus dropped on a duty: its tour's)
        (Arm::Bus(id), Some(_)) => give_bus(l, p, hit.tour, Some(id)),
        (Arm::Driver(_), None) => {}
    }
}

/// A duty's trips (`planned`, as the plan has them) in its tour of the day's timetable
/// (`trips`): the first one's place as the game counts it (1 the first) and the minute it
/// leaves, and how many there are. The plan has the trips in the order they leave, an own
/// line's with the depot runs its timetable lacks: each is found by its name and the minute
/// it leaves, and the depot runs are not (None: none of them is).
fn timetable_legs(planned: &[co::network::PlannedTrip], trips: &[core::TripInfo]) -> Option<(usize, i32, usize)> {
    let found: Vec<(usize, i32)> = planned.iter().filter_map(|x| trips.iter().find(|y| y.name == x.name && (y.departure / 60.0).round() as i32 == x.dep).map(|y| (y.index, x.dep))).collect();
    let (first, time) = *found.first()?;
    Some((first, time, found.len()))
}

/// "Drive this duty": the Drive page with exactly this duty - its line and tour, its first
/// trip and as many trips as it has (`--duty-leg`), the company's map, the plan's day, the
/// depot and the tour's bus in its paint. The player starts it there himself.
pub(super) fn drive(l: &mut Launcher, c: &Company, p: &DayPlan, ti: usize, k: usize) {
    let t = &p.tours[ti];
    let d = &t.duties[k];
    // the duty's trips as the game counts them, in the day's timetable
    // (the timetable's tour the game drives: the one its first trip with passengers comes from
    // - a block laid out in the studio may hold trips of several)
    let origin = t.tour.trips.get(d.start..d.end).and_then(|x| x.iter().find(|x| x.counts() && !x.origin.trim().is_empty())).map(|x| x.origin.trim().to_string()).unwrap_or_else(|| t.tour.tour.clone());
    let legs = l.company.planning.days.iter().find(|x| x.0 == c.map && x.1 == p.date).and_then(|x| x.2.as_ref().ok()).and_then(|lines| {
        let line = lines.iter().find(|x| x.name.eq_ignore_ascii_case(&t.tour.line))?;
        let tour = line.tours.iter().find(|x| x.number.trim() == origin)?;
        timetable_legs(t.tour.trips.get(d.start..d.end)?, &tour.trips)
    });
    let Some((first, time, count)) = legs else {
        kit::show(l, kit::Popup::new("timer", "Not yet", omsi_ui::tr("The timetable of the day is still being read."), omsi_ui::tr("Try again in a moment."), None));
        return;
    };
    let (line, tour) = (t.tour.line.clone(), origin);
    let ch = &mut l.state.choice;
    ch.map = c.map.clone();
    ch.entry = -1;
    ch.date = p.date.clone();
    ch.free = false;
    ch.own_line = false;
    ch.composed = true;
    ch.line = Some(line.clone());
    ch.tour = Some(tour.clone());
    ch.time = time;
    ch.start_trip = Some((line.clone(), tour.clone(), first, time));
    ch.legs = vec![format!("{line}|{tour}|{first}|{count}")];
    // (the bus the planning gives the tour: the duty is driven with it, in its livery - not
    // chosen on the bus step, Luc; a rental bus is any bus)
    let planned = t.bus.and_then(|b| if let BusOf::Own(id) = b { c.vehicle(id) } else { None }).filter(|v| !v.bus.trim().is_empty()).map(|v| PlannedBus {
        file: v.bus.clone(),
        name: v.name.clone(),
        paint: v.house_livery.clone().filter(|h| !h.trim().is_empty()).unwrap_or_else(|| v.livery.clone()),
        number: v.number.clone(),
        plate: v.plate.clone(),
    });
    // (the line's own depot file, else the company's)
    let hof = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&line)).map(|x| x.hof_or(&c.depot).to_string()).unwrap_or_else(|| c.depot.clone());
    if !hof.trim().is_empty() {
        ch.hof = hof;
        ch.hof_manual = true;
    }
    l.state.start_company_duty(CompanyDuty { company: c.id.clone(), name: c.name.clone(), bus: planned, ..Default::default() });
    // (a later day's duty: that day's plan for the game too - the company's buses on the road
    // with it; only the company's own day is booked)
    if !p.today {
        if let Err(e) = co::plan::save_drive_plan(&super::data(), &co::plan::live_plan(c, p)) {
            log::warn!("company: the plan of {} for the game: {e:#}", p.date);
        }
    }
    l.state.load_lines();
    l.state.load_ibis();
    super::leave(l);
    l.go(Page::Drive);
    l.drive.step = Step::Duty;
    l.state.set_status(omsi_ui::tr("Your company's duty is set, with the bus its planning gives it: on the map only your company's buses run. Start the duty when you are ready.").into_owned(), false);
}

// --- the page ------------------------------------------------------------------------------

/// The planning page: RosterFlow and its tabs, with its tutorial over them (`rf_tutorial`: its
/// "?" menu, its help sheet and a lesson's guide; the page under what they cover gets no mouse).
pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    work(l, &c);
    let held = rf_tutorial::before(l);
    page(l, area, &c);
    rf_tutorial::after(l, area, held);
}

fn page(l: &mut Launcher, area: Rect, c: &Company) {
    let c = c.clone();
    // (a line's timetable over the page: the page under it without the mouse and the keys)
    let under = l.company.planning.edit.is_some().then(|| super::mask(&mut l.ui));
    let day = l.company.planning.day.min(6);
    let date = co::dates::add(&c.date, day as i64);
    // RosterFlow: its title bar, and the Vehicles, Drivers or Timeplan tab; the Roster tab is
    // this page's own, under the title bar
    let Some(rest) = rosterflow::frame(l, area, &c, &date) else {
        if l.ui.input.keys.contains(&Key::Escape) {
            l.company.planning.rf.let_go();
        }
        if let Some(i) = under {
            l.ui.input = i;
            plan_edit::draw(l, area, &c);
        }
        return;
    };
    let area = rest;
    l.ui.as_drawn();
    rf_roster::page(l, area, &c, &date);
    l.ui.end_as_drawn();
    if l.ui.input.keys.contains(&Key::Escape) {
        l.company.planning.arm = None;
        l.company.planning.sel = None;
    }
    if let Some(i) = under {
        l.ui.input = i;
        plan_edit::draw(l, area, &c);
    }
}

/// A weekday's full name (0 Monday).
fn weekday_name(d: u8) -> String {
    const NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    omsi_ui::tr(NAMES[d as usize % 7]).into_owned()
}

/// Weekdays in words: "Tuesday to Friday" (three or more in a row), else "Monday, Thursday".
fn weekdays_text(days: &[u8]) -> String {
    let in_row = days.windows(2).all(|w| w[1] == w[0] + 1);
    match days {
        [a, .., b] if days.len() >= 3 && in_row => omsi_ui::tr("%{from} to %{to}").replace("%{from}", &weekday_name(*a)).replace("%{to}", &weekday_name(*b)),
        _ => days.iter().map(|d| weekday_name(*d)).collect::<Vec<_>>().join(", "),
    }
}

/// What "Fill the roster" left open, in words ("" when nothing).
pub(super) fn open_text(f: &pl::Filled) -> String {
    let mut parts = Vec::new();
    let open = f.unqualified + f.busy;
    if open > 0 {
        let why = match (f.unqualified, f.busy) {
            (_, 0) => omsi_ui::tr("nobody is qualified for their bus").into_owned(),
            (0, _) => omsi_ui::tr("nobody qualified is free").into_owned(),
            (q, b) => omsi_ui::tr("%{q} with nobody qualified for the bus, %{b} with nobody free").replace("%{q}", &q.to_string()).replace("%{b}", &b.to_string()),
        };
        parts.push(omsi_ui::tr("%{n} duties still open: %{why}.").replace("%{n}", &open.to_string()).replace("%{why}", &why));
    }
    if f.no_bus > 0 {
        parts.push(omsi_ui::tr("%{n} tours still without a bus: none of the fleet is free for them.").replace("%{n}", &f.no_bus.to_string()));
    }
    parts.join(" ")
}

/// A tour's row on the chart is as tall as the room the rows shown share (Luc: "this screen may
/// well be adaptive: with few tours larger, with more tours smaller"), between RosterFlow's
/// compact pitch and a little more (the Roster tab is drawn as its Gantt, `rf_roster`); more
/// rows than fit at the least scroll.
pub(super) const ROW_MIN: f32 = 27.0;
pub(super) const ROW_MAX: f32 = 38.0;
/// The pitch of a tour's row: the room shared by `rows`, between `ROW_MIN` and `ROW_MAX`.
pub(super) fn row_pitch(room: f32, rows: usize) -> f32 {
    if rows == 0 {
        return ROW_MAX;
    }
    (room / rows as f32).floor().clamp(ROW_MIN, ROW_MAX)
}

/// A name's initials ("Jan Meijer": "JM"): a block too narrow for the name.
fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

/// The words a block shows at its width: its name where it fits, else its short form, else
/// none (its tooltip says it all).
fn fit_words<'a>(ui: &Ui, w: f32, px: f32, full: &'a str, short: &'a str) -> Option<&'a str> {
    if !full.is_empty() && ui.width(full, px, Weight::Bold) + 12.0 <= w {
        Some(full)
    } else if !short.is_empty() && ui.width(short, px, Weight::Bold) + 8.0 <= w {
        Some(short)
    } else {
        None
    }
}

/// The same row tapped twice in a short while (the event loop has no double click of its own).
fn second_tap(v: &mut PlanningView, key: String, now: f32) -> bool {
    let twice = v.tapped.as_ref().is_some_and(|(k, t)| *k == key && now - *t < 0.45);
    v.tapped = if twice { None } else { Some((key, now)) };
    twice
}

// --- putting a line into service ----------------------------------------------------------------

/// "Start service from": now, from tomorrow, or from a date; what the next seven days of the
/// line are planned like, and - with gaps - the player's word that he starts with them.
pub(super) fn service_dialog(l: &mut Launcher) {
    let Some(Dialog::Service { line, when, date, gaps }) = &l.company.dialog else { return };
    let (line, when, date, gaps) = (line.clone(), *when, date.clone(), *gaps);
    let Some(c) = l.company.company.clone() else { return };
    let Some(cl) = c.lines.iter().find(|x| x.name == line).cloned() else {
        l.company.dialog = None;
        return;
    };
    let title = omsi_ui::tr("Start the service of line %{n}").replace("%{n}", &cl.number);
    let f = kit::frame(l, 760.0, 640.0, "play_arrow", &title);
    let inner = f.body;
    let mut y = inner.y;
    y += l.ui.paragraph("From the moment you choose, the line's tours run as they are planned. A tour without its bus or a driver is dropped then, with the contract's penalty - plan first, then start.", Vec2::new(inner.x, y), inner.w, kit::BODY, Weight::Regular, TEXT_SOFT) + 16.0;
    kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "From when");
    y += 26.0;
    let labels: Vec<String> = ["Now", "From tomorrow", "From a date"].iter().map(|s| omsi_ui::tr(s).into_owned()).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut w = when;
    l.ui.segmented("plan-service-when", Rect::new(inner.x, y, inner.w.min(520.0), 40.0), &mut w, &refs);
    let mut d = date.clone();
    if w == 2 {
        l.ui.date_field("plan-service-date", Rect::new(inner.x + inner.w.min(520.0) + 14.0, y, (inner.w - inner.w.min(520.0) - 14.0).max(160.0), 40.0), &mut d);
    }
    y += 56.0;
    let from = match w {
        0 => co::clock::now(&c),
        1 => co::clock::moment(&co::dates::add(&c.date, 1), 0),
        _ => co::clock::moment(&d, 0).max(co::clock::now(&c)),
    };
    // (the line's week from the day it starts, read besides the planning's own: from
    // tomorrow or a later date it reaches past it)
    let start_day = co::clock::date_of(from);
    l.company.planning.extra = (0..7).map(|k| co::dates::add(&start_day, k)).collect();
    work(l, &c);
    // the next seven days of the line as planned
    kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "The line's week as it is planned");
    y += 26.0;
    let mut short = 0usize;
    let mut reading = false;
    for k in 0..7 {
        let day = co::dates::add(&start_day, k);
        let row = Rect::new(inner.x, y, inner.w, 30.0);
        l.ui.text_in(&day_label(&day), Rect::new(row.x, row.y, 220.0, row.h), kit::ROWS, Weight::Medium, TEXT, Align::Left);
        let (text, colour) = match plan_of(l, &c, &day) {
            None => {
                reading = true;
                (omsi_ui::tr("Reading the timetable…").into_owned(), TEXT_DIM)
            }
            Some(Err(_)) => (omsi_ui::tr("The timetable could not be read.").into_owned(), WARN),
            Some(Ok(p)) => {
                let mine: Vec<&DayTour> = p.tours.iter().filter(|t| t.tour.line == line && co::clock::moment(&day, t.tour.from() as i64) >= from).collect();
                let covered = mine.iter().filter(|t| t.covered()).count();
                short += mine.len() - covered;
                if mine.is_empty() {
                    (omsi_ui::tr("No tours").into_owned(), TEXT_DIM)
                } else {
                    let t = omsi_ui::tr("%{c} of %{t} tours planned").replace("%{c}", &covered.to_string()).replace("%{t}", &mine.len().to_string());
                    (t, kit::share_colour(covered as f64 / mine.len() as f64, false))
                }
            }
        };
        l.ui.text_in(&text, Rect::new(row.x + 230.0, row.y, row.w - 230.0, row.h), kit::ROWS, Weight::Bold, colour, Align::Left);
        l.ui.p().rect(Rect::new(row.x, row.bottom(), row.w, 1.0), HAIRLINE);
        y += 32.0;
    }
    y += 12.0;
    let mut g = gaps;
    if short > 0 {
        let t = omsi_ui::tr("%{n} tours of these days have no bus or no driver yet: they would be dropped, each with its penalty.").replace("%{n}", &short.to_string());
        y += l.ui.paragraph(&t, Vec2::new(inner.x, y), inner.w, kit::BODY, Weight::Medium, WARN) + 8.0;
        l.ui.toggle("plan-service-gaps", Rect::new(inner.x, y, inner.w, ROW), &mut g, "Start all the same, with these gaps");
    }
    let mut foot = Foot::new(&f);
    let go = foot.right(l, "plan-service-go", "Start service", Some("play_arrow"), ButtonKind::Primary);
    if foot.right(l, "plan-service-cancel", "Cancel", None, ButtonKind::Normal) || f.close {
        l.company.dialog = None;
        return;
    }
    if go {
        if reading {
            kit::show(l, kit::Popup::new("timer", "Not yet", omsi_ui::tr("The line's week is still being read."), omsi_ui::tr("Try again in a moment."), None));
        } else if short > 0 && !g {
            kit::show(l, kit::Popup::new("event", "Not planned yet", omsi_ui::tr("Some of the line's tours have no bus or no driver: they would be dropped, each with the contract's penalty."), omsi_ui::tr("Give them buses and drivers on the planning (\"Fill the roster\" does it for you) - or choose to start with the gaps."), None));
        } else if let Some(at) = act(l, |c| co::network::start_service(c, &line, from)) {
            l.company.dialog = None;
            l.state.set_status(omsi_ui::tr("Line %{n} is in service from %{when}.").replace("%{n}", &cl.number).replace("%{when}", &format!("{} {}", day_label(&co::clock::date_of(at)), co::clock::hhmm(at))), false);
            return;
        }
    }
    l.company.dialog = Some(Dialog::Service { line, when: w, date: d, gaps: g });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_say_what_they_did() {
        assert_eq!(weekdays_text(&[1, 2, 3, 4]), "Tuesday to Friday");
        assert_eq!(weekdays_text(&[0, 1, 3, 4]), "Monday, Tuesday, Thursday, Friday");
        assert_eq!(weekdays_text(&[6]), "Sunday");
        let f = pl::Filled { duties: 9, buses: 4, unqualified: 2, busy: 0, no_bus: 0 };
        assert_eq!(open_text(&f), "2 duties still open: nobody is qualified for their bus.");
        assert_eq!(open_text(&pl::Filled::default()), "");
        let keys = [
            "%{from} to %{to}",
            "Filled: %{d} duties given to drivers, %{b} buses given.",
            "%{n} duties still open: %{why}.",
            "%{q} with nobody qualified for the bus, %{b} with nobody free",
            "The day is cleared: %{d} duties and %{b} buses taken off the roster of %{day}.",
            "The roster of %{day} is now on %{days} too (%{n} days).",
            "The roster of %{day} is now on %{days} too.",
            "Nothing to repeat",
            "Nothing to clear",
            "Nothing could be filled",
            "Train drivers",
        ];
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in keys {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }

    #[test]
    fn the_rows_share_the_room_between_a_least_and_a_most() {
        // few tours: tall, up to the most
        assert_eq!(row_pitch(600.0, 3), ROW_MAX);
        // more: they share the room
        assert_eq!(row_pitch(600.0, 20), 30.0);
        // many: the least, and the chart scrolls
        assert_eq!(row_pitch(600.0, 60), ROW_MIN);
        assert_eq!(row_pitch(600.0, 0), ROW_MAX);
        assert_eq!(initials("Jan  Meijer"), "JM");
        assert_eq!(initials("anna"), "A");
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in ["%{n} without a bus", "%{n} without a driver", "Collapse all", "Expand all", "The line's trips and timetable"] {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }

    #[test]
    fn a_duty_is_driven_from_its_own_first_trip() {
        let trip = |name: &str, index: usize, dep: f64| omsi_launcher_lib::TripInfo { name: name.into(), index, line: "7".into(), from: "A".into(), terminus: "B".into(), departure: dep * 60.0, arrival: (dep + 30.0) * 60.0, stops: Vec::new(), km: 9.0 };
        let timetable = [trip("7 A-B", 1, 360.0), trip("7 B-A", 2, 400.0), trip("7 A-B", 3, 440.0)];
        let planned = |name: &str, dep: i32, depot: bool| co::network::PlannedTrip { name: name.into(), line: if depot { String::new() } else { "7".into() }, dep, arr: dep + 30, stops: if depot { 0 } else { 5 }, empty: depot, ..Default::default() };
        // an own line's tour as the plan has it: the depot runs added before and after
        let tour = [planned("Betriebsfahrt", 352, true), planned("7 A-B", 360, false), planned("7 B-A", 400, false), planned("7 A-B", 440, false), planned("Betriebsfahrt", 470, true)];
        // its first duty: the depot run and two trips; its second: the last trip and the run back
        assert_eq!(timetable_legs(&tour[0..3], &timetable), Some((1, 360, 2)));
        assert_eq!(timetable_legs(&tour[3..5], &timetable), Some((3, 440, 1)));
        // a line of the map's own timetable: the plan's trips are the timetable's
        assert_eq!(timetable_legs(&tour[1..4], &timetable), Some((1, 360, 3)));
        assert_eq!(timetable_legs(&tour[4..5], &timetable), None);
    }
}
