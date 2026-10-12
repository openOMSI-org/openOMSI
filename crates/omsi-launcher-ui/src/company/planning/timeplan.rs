//! RosterFlow's timeplan (the planning, phase 1b; Luc: the company's planning should look and
//! work "exactly" like Optibus Planning): a line's running times and its timetable, in
//! Optibus's dark slate look with one sky-blue accent - orange for what was typed over and for
//! warnings, a red "!" for errors - and the light look made of it.
//!
//! On top the context bar: the line, the day group (Mon–Fri, Saturday, Sunday), undo and redo,
//! OUTBOUND / INBOUND, the two screens (running times, timetable) and saving. Both screens have
//! the stop strip: the stops' names at 45° in grey italics, each stop's code under it in white,
//! and a rail in the line's colour with a dot per stop (a timepoint full, a plain stop white
//! inside; a dot clicked on the running times is made one or the other).
//!
//! * **Running times**: the time bands on a rail from 00:00 to 36:00 (⊕ adds one, the bin takes
//!   one out, a band's time clicked is typed anew) and per band a row of boxes with the minutes
//!   between two stops ("7 m"), clicked and typed; the trip's total at the end. A change takes
//!   every trip with it at once (`co::timeplan::retime`).
//! * **Timetable**: the figures on top (number of trips, trips by pattern, trips by hour, each
//!   opened into a larger chart); the frozen columns PATTERN, VEHICLE TYPE, START, HEADWAY, then
//!   the time at every stop - a dash where a pattern skips one, a time typed over in orange
//!   with how far it is off. A start or a headway typed moves every later trip; trips are added
//!   one at a time or by headway from a control point ("10,20" alternates), and taken out.
//! * **Validation**: the issues of the day group in a small panel off the status bar.
//!
//! Saved, the line's timetable is the company's as every own line's is (`plan_edit::commit`: a
//! line in service from the day chosen, else at once; a line of the map's at once), and the
//! line is marked UNSYNCED until "Refresh" lets the scheduling studio make its blocks anew.
//!
//! Until RosterFlow's shell has a "Timeplan" tab, the Planning page's header opens it
//! (`PlanningView::timeplan`); the shell calls `draw` with the room it gives it.

use super::super::super::theme::*;
use super::super::super::ui::{id_of, ButtonKind, Key, Ui};
use super::super::super::Launcher;
use super::super::{act, day_label, kit, reload_timetable};
use super::plan_edit::{self, Commit, Own};
use glam::Vec2;
use omsi_launcher_lib::company::timeplan::{self as tp, Level, RunSet, Side};
use omsi_launcher_lib::company::{self as co, ownline, Company, CompanyLine};
use omsi_launcher_lib::lines::{self as reg, LineDesign, DAY_GROUPS};
use omsi_launcher_lib::service::VehicleClass;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

// --- the look ---------------------------------------------------------------------------------------

/// Optibus's dark slate: the page, the panels, a cell, its edge (the light look is made of them).
const BG: Color = Color::rgba(31, 34, 41, 1.0);
const PANE: Color = Color::rgba(38, 43, 52, 1.0);
const CELL: Color = Color::rgba(44, 50, 60, 1.0);
const CELL_HOVER: Color = Color::rgba(52, 59, 71, 1.0);
const BORDER: Color = Color::rgba(58, 66, 80, 1.0);
/// The one accent, and what was typed over or warns, and what is wrong.
const SKY: Color = Color::rgba(66, 165, 245, 1.0);
const ORANGE: Color = Color::rgba(243, 156, 18, 1.0);
const RED: Color = Color::rgba(229, 57, 53, 1.0);
/// The stops' names over the strip.
const NAME: Color = Color::rgba(196, 202, 214, 1.0);

/// The timeplan's scrollbars (`rf_scroll`): its faint track, the thumb, the thumb held.
const BARS: super::rf_scroll::Look = super::rf_scroll::Look { track: Color::rgba(58, 66, 80, 0.55), thumb: Color::rgba(96, 106, 124, 1.0), hot: Color::rgba(140, 150, 168, 1.0) };

/// The context bar, the figures' band, the status bar.
const BAR_H: f32 = 48.0;
const KPI_H: f32 = 74.0;
const STATUS_H: f32 = 32.0;
/// The strip: room for the names at 45° above the rail.
const STRIP_H: f32 = 150.0;
/// A band's row of running times, a trip's row.
const RT_ROW: f32 = 46.0;
const TT_ROW: f32 = 36.0;
/// The running times' left column (the bands' rail).
const LEFT: f32 = 168.0;
/// The timetable's frozen columns: the row's number, PATTERN, VEHICLE TYPE, START, HEADWAY.
const FROZEN: [f32; 5] = [40.0, 74.0, 104.0, 78.0, 84.0];
/// The form that adds trips by headway.
const FORM_H: f32 = 64.0;

/// Why a line cannot be planned here.
const MAP_NOT_EDITED: &str = "A line of the map's own timetable: open it once in the line editor (Lines page) to plan its timetable here.";
const NO_LINE: &str = "The company runs no line yet: add one on the Lines page first.";

/// The screens.
const RUNNING: usize = 0;
const TIMETABLE: usize = 1;

// --- what the screen keeps -----------------------------------------------------------------------

/// The line being planned: its line in the registry (`plan_edit::Own`; its `draft` the one
/// changed here, as a table of trips) and whether it is a line of the map's.
#[derive(Clone)]
struct Work {
    own: Own,
    map: bool,
}

/// A cell chosen to type into.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Cell {
    /// A running time: the band and the segment (of the pattern shown).
    Run { band: usize, seg: usize },
    /// When a band begins.
    BandFrom(usize),
    /// A trip's start, its headway, its time at a stop (by its place in the table).
    Start(usize),
    Headway(usize),
    Time(usize, usize),
}

/// The form that adds trips by headway: the pattern, the control point, the first and the last
/// departure there, and the headways.
#[derive(Clone, Debug, Default)]
struct Adding {
    pattern: usize,
    control: usize,
    first: String,
    last: String,
    headway: String,
}

#[derive(Default)]
pub(in super::super) struct TimeplanView {
    /// Shown over the Planning page (its header's "Timeplan").
    pub(in super::super) open: bool,
    /// The company and its line (timetable name) chosen, and the pair `work` was read for.
    company: String,
    line: String,
    loaded: Option<(String, String)>,
    work: Option<Work>,
    why: Option<&'static str>,
    /// The day group, the side (0 OUTBOUND, 1 INBOUND), the pattern whose running times are
    /// shown (its place among the side's), the screen.
    day: usize,
    side: usize,
    pattern: usize,
    tab: usize,
    /// The line before each change, and the changes undone.
    undo: Vec<LineDesign>,
    redo: Vec<LineDesign>,
    cell: Option<Cell>,
    text: String,
    /// The trip chosen (its place in the table).
    sel: Option<usize>,
    adding: Option<Adding>,
    /// A figure opened into its chart.
    chart: Option<usize>,
    issues_open: bool,
    /// Only the timepoints as the timetable's columns.
    points_only: bool,
    scroll: f32,
    hscroll: f32,
    /// Lines saved since the scheduling studio made their blocks (line, day group).
    unsynced: Vec<(String, usize)>,
    /// The day a change of a line in service begins.
    from: String,
    /// Something asked to leave with changes not saved: pressed again it leaves.
    leaving: bool,
    /// Changes made so far (undone ones too), for RosterFlow's lessons.
    changes: u64,
}

/// What RosterFlow's lessons see of the timeplan (`TimeplanView::probe`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TpProbe {
    /// A line is open to plan; else why not (None: not read yet).
    pub ready: bool,
    pub why: Option<&'static str>,
    pub screen: usize,
    pub dirty: bool,
    pub changes: u64,
    /// The line's trips (all its day groups).
    pub trips: usize,
    /// The line was saved since the studio made its blocks.
    pub unsynced: bool,
}

impl TimeplanView {
    /// Something is in hand that Escape lets go of first.
    /// Open the line `line` (timetable name) of the company `company` ("View in timeplan").
    pub(in super::super) fn show(&mut self, company: &str, line: &str) {
        self.company = company.to_string();
        self.line = line.to_string();
    }

    pub(in super::super) fn holds(&self) -> bool {
        self.cell.is_some() || self.adding.is_some() || self.chart.is_some() || self.issues_open || self.sel.is_some()
    }

    /// The screen shown: 0 the running times, 1 the timetable.
    pub(in super::super) fn screen(&self) -> usize {
        self.tab
    }

    /// Show the running times (0) or the timetable (1) - RosterFlow's tour does.
    pub(in super::super) fn set_screen(&mut self, tab: usize) {
        let tab = tab.min(TIMETABLE);
        if self.tab != tab {
            self.tab = tab;
            self.cell = None;
            self.text.clear();
        }
    }

    /// What RosterFlow's lessons look at (`rf_tutorial`).
    pub(super) fn probe(&self) -> TpProbe {
        TpProbe {
            ready: self.work.is_some(),
            why: self.why,
            screen: self.tab,
            dirty: self.dirty(),
            changes: self.changes,
            trips: self.draft().map(|d| d.table.len()).unwrap_or(0),
            unsynced: self.unsynced.iter().any(|u| u.0.eq_ignore_ascii_case(&self.line)),
        }
    }

    /// The line shown as a lesson finds it (`rf_tutorial`): its name, the line as it is saved
    /// (as a table of trips), and what was UNSYNCED then.
    pub(super) fn keep_for_lesson(&self) -> Option<(String, LineDesign, Vec<(String, usize)>)> {
        let w = self.work.as_ref()?;
        Some((self.line.clone(), w.own.base.clone(), self.unsynced.clone()))
    }

    fn let_go(&mut self) {
        self.cell = None;
        self.text.clear();
        self.adding = None;
        self.chart = None;
        self.issues_open = false;
        self.sel = None;
    }

    fn dirty(&self) -> bool {
        self.work.as_ref().is_some_and(|w| w.own.draft != w.own.base)
    }

    fn draft(&self) -> Option<&LineDesign> {
        self.work.as_ref().map(|w| &w.own.draft)
    }

    /// The line changed by `f`, the line before kept to be undone.
    fn change(&mut self, f: impl FnOnce(&mut LineDesign)) {
        let Some(w) = self.work.as_mut() else { return };
        let before = w.own.draft.clone();
        f(&mut w.own.draft);
        if w.own.draft != before {
            self.changes += 1;
            self.undo.push(before);
            if self.undo.len() > 80 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    /// The running times of the day group shown and direction `dir` changed by `f`: every trip
    /// of theirs takes them at once.
    fn change_run(&mut self, dir: usize, f: impl FnOnce(&mut RunSet)) {
        let day = self.day;
        self.change(|line| {
            let Some(d) = line.directions.get(dir).cloned() else { return };
            let mut plan = tp::plan_of(line);
            let old = plan.set(line, day, dir);
            let mut new = old.clone();
            f(&mut new);
            new.fit(&d);
            tp::retime(line, day, dir, &old, &new);
            plan.put(new);
            line.timeplan = Some(plan);
        });
    }

    fn undo(&mut self) {
        if let (Some(prev), Some(w)) = (self.undo.pop(), self.work.as_mut()) {
            self.redo.push(std::mem::replace(&mut w.own.draft, prev));
            self.cell = None;
            self.sel = None;
        }
    }

    fn redo(&mut self) {
        if let (Some(next), Some(w)) = (self.redo.pop(), self.work.as_mut()) {
            self.undo.push(std::mem::replace(&mut w.own.draft, next));
            self.cell = None;
            self.sel = None;
        }
    }
}

/// The company line's line in the line editor's registry, made a table of trips to plan - or
/// why it cannot be planned here.
fn load(c: &Company, cl: &CompanyLine) -> Result<Work, &'static str> {
    let (mut own, map) = if cl.own {
        (plan_edit::load(c, cl)?, false)
    } else {
        let path = reg::registry_path(&reg::map_folder(&c.map));
        let r = reg::load_registry(&path);
        let line = r.map_line(&cl.name).cloned().ok_or(MAP_NOT_EDITED)?;
        let stem = reg::stems(&r).get(&line.id).cloned().unwrap_or_default();
        (plan_edit::own_of(line.id, path, stem, line), true)
    };
    tp::as_table(&mut own.base);
    own.draft = own.base.clone();
    Ok(Work { own, map })
}

// --- times and words -------------------------------------------------------------------------------

/// A time typed for a trip at `near`: the day's, or a day later when that is nearer (00:05 for a
/// trip of the late evening is after midnight).
fn near_time(t: f32, near: f32) -> f32 {
    if t + 1440.0 <= tp::DAY_SPAN && (t + 1440.0 - near).abs() < (t - near).abs() {
        t + 1440.0
    } else {
        t
    }
}

/// The vehicle types a line allows, short ("SD,DD"; "All" when any).
fn vehicle_types(l: &LineDesign) -> String {
    if l.vehicles.classes.is_empty() {
        return omsi_ui::tr("All").into_owned();
    }
    l.vehicles
        .classes
        .iter()
        .map(|c| match c {
            VehicleClass::Minibus => "MB",
            VehicleClass::Midi => "MD",
            VehicleClass::Solo => "SD",
            VehicleClass::Articulated => "AR",
            VehicleClass::Double => "DD",
            VehicleClass::Coach => "CO",
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// A day group's name as the context bar shows it.
fn day_name(k: usize) -> String {
    omsi_ui::tr(DAY_GROUPS.get(k).map(|d| d.0).unwrap_or("Mon - Fri")).into_owned()
}

/// A line's colour on the rail.
fn line_colour(l: &LineDesign) -> Color {
    let hex = l.colour.trim();
    if hex.is_empty() { SKY } else { super::super::super::ownlines::colour_of(hex) }
}

/// The columns of the strip and the grid: where column `j`'s middle is.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cols {
    x: f32,
    w: f32,
    scroll: f32,
}

impl Cols {
    fn cx(&self, j: usize) -> f32 {
        self.x + (j as f32 + 0.5) * self.w - self.scroll
    }
}

/// Columns of `n` in `room` pixels: as wide as they share it, between `least` and `most`
/// (wider than the room: scrolled).
fn fit_cols(x: f32, room: f32, n: usize, least: f32, most: f32, scroll: &mut f32) -> Cols {
    let w = (room / n.max(1) as f32).clamp(least, most);
    let over = (w * n as f32 - room).max(0.0);
    *scroll = scroll.clamp(0.0, over);
    Cols { x, w, scroll: *scroll }
}

// --- the screen ------------------------------------------------------------------------------------

/// RosterFlow's timeplan in `area` (the shell's Timeplan tab; until then the Planning page's).
pub(super) fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let mut v = std::mem::take(&mut l.company.planning.timeplan);
    screen(l, &mut v, area, &c);
    l.company.planning.timeplan = v;
}

/// "Undo the lesson" of RosterFlow's timeplan lesson (`rf_tutorial`): line `line` as the lesson
/// found it (`design`, `unsynced`). Changes not saved are dropped; a timetable saved meanwhile
/// is saved back the way it was saved (`save`). Returns false when it could not be saved back
/// (the status line says why).
pub(super) fn restore(l: &mut Launcher, line: &str, design: &LineDesign, unsynced: Vec<(String, usize)>) -> bool {
    let Some(c) = l.company.company.clone() else { return false };
    let mut v = std::mem::take(&mut l.company.planning.timeplan);
    (v.company, v.line, v.loaded) = (c.id.clone(), line.to_string(), None);
    load_shown(&mut v, &c);
    let mut ok = true;
    if let Some(w) = v.work.as_mut().filter(|w| w.own.base != *design) {
        w.own.draft = design.clone();
        ok = save(l, &mut v);
    }
    v.unsynced = unsynced;
    v.loaded = None;
    l.company.planning.timeplan = v;
    ok
}

/// The line chosen (the first own line, else the first), read when another is chosen.
fn load_shown(v: &mut TimeplanView, c: &Company) {
    if v.company != c.id || !c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&v.line)) {
        v.company = c.id.clone();
        v.line = c.lines.iter().find(|x| x.own).or(c.lines.first()).map(|x| x.name.clone()).unwrap_or_default();
    }
    let key = (v.company.clone(), v.line.clone());
    if v.loaded.as_ref() != Some(&key) {
        v.loaded = Some(key);
        (v.undo, v.redo, v.sel, v.cell, v.adding, v.scroll, v.hscroll, v.pattern, v.leaving) = (Vec::new(), Vec::new(), None, None, None, 0.0, 0.0, 0, false);
        v.text.clear();
        match c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&v.line)) {
            Some(cl) => match load(c, cl) {
                Ok(w) => {
                    let pending = w.own.saved.pending_from.clone();
                    v.from = Some(pending).filter(|d| co::dates::between(&c.date, d) >= 1).unwrap_or_else(|| co::dates::add(&c.date, 1));
                    v.work = Some(w);
                    v.why = None;
                }
                Err(why) => (v.work, v.why) = (None, Some(why)),
            },
            None => (v.work, v.why) = (None, Some(NO_LINE)),
        }
    }
}

fn screen(l: &mut Launcher, v: &mut TimeplanView, area: Rect, c: &Company) {
    load_shown(v, c);
    l.ui.solid(area);
    l.ui.p().rounded(area, 12.0, BG);
    let inner = area.inset(14.0);
    // (the chart opened from a figure lies over the screen: the screen under it without the mouse)
    let under = v.chart.is_some().then(|| super::super::mask(&mut l.ui));
    // (the context bar stays where it is on both screens; the figures under it on the timetable)
    let mut y = inner.y;
    super::super::super::tour::anchor("rf-tp-bar", Rect::new(inner.x, y, inner.w, BAR_H));
    context_bar(l, v, c, Rect::new(inner.x, y, inner.w, BAR_H));
    y += BAR_H + 6.0;
    if v.tab == TIMETABLE && v.work.is_some() {
        kpis(l, v, Rect::new(inner.x, y, inner.w, KPI_H));
        y += KPI_H + 6.0;
    }
    let body = Rect::new(inner.x, y, inner.w, (inner.bottom() - STATUS_H - 6.0 - y).max(0.0));
    if v.work.is_some() {
        super::super::super::tour::anchor("rf-tp-body", body);
    }
    let status = Rect::new(inner.x, inner.bottom() - STATUS_H, inner.w, STATUS_H);
    match (&v.work, v.why) {
        (None, why) => {
            let text = omsi_ui::tr(why.unwrap_or(NO_LINE));
            l.ui.paragraph(&text, Vec2::new(body.x + 24.0, body.y + 28.0), (body.w - 48.0).max(0.0), kit::BODY, Weight::Regular, TEXT_SOFT);
        }
        (Some(_), _) => {
            l.ui.push_clip(body, 0.0);
            if v.tab == RUNNING {
                running_times(l, v, body);
            } else {
                timetable(l, v, body);
            }
            l.ui.pop_clip();
            status_bar(l, v, c, status);
        }
    }
    if let Some(i) = under {
        l.ui.input = i;
        chart_card(l, v, area);
    } else if v.issues_open {
        issues_panel(l, v, status);
    }
    keys(l, v);
}

// --- the context bar -------------------------------------------------------------------------------

fn context_bar(l: &mut Launcher, v: &mut TimeplanView, c: &Company, r: Rect) {
    l.ui.p().rounded(r, 8.0, PANE);
    let cy = r.center().y;
    let fh = 34.0;
    let mut x = r.x + 8.0;
    // the line
    let lines: Vec<&CompanyLine> = c.lines.iter().collect();
    let labels: Vec<String> = lines.iter().map(|x| if x.caption.trim().is_empty() { x.number.clone() } else { format!("{} · {}", x.number, x.caption.trim()) }).collect();
    crate::mt::protect(labels.iter().map(String::as_str));
    if !labels.is_empty() {
        let mut k = lines.iter().position(|x| x.name.eq_ignore_ascii_case(&v.line)).unwrap_or(0);
        let lr = Rect::new(x, cy - fh * 0.5, 220.0, fh);
        if l.ui.select("tp-line", lr, &mut k, &labels) {
            if v.dirty() && !v.leaving {
                v.leaving = true;
                l.state.set_status(omsi_ui::tr("The timeplan has changes not saved: choose again to drop them.").into_owned(), true);
            } else if let Some(cl) = lines.get(k) {
                v.line = cl.name.clone();
            }
        }
        x = lr.right() + 8.0;
    }
    // the day group
    let mut day = v.day.min(2);
    let days: Vec<String> = (0..DAY_GROUPS.len()).map(day_name).collect();
    let dr = Rect::new(x, cy - fh * 0.5, 150.0, fh);
    if l.ui.select("tp-day", dr, &mut day, &days) && day != v.day {
        v.day = day;
        v.let_go();
        v.scroll = 0.0;
    }
    x = dr.right() + 14.0;
    // undo, redo
    if l.ui.icon_button("tp-undo", Vec2::new(x + 14.0, cy), 14.0, "turn_left", "Undo") {
        v.undo();
    }
    if l.ui.icon_button("tp-redo", Vec2::new(x + 46.0, cy), 14.0, "turn_right", "Redo") {
        v.redo();
    }
    l.ui.p().rect(Rect::new(x + 70.0, r.y + 10.0, 1.0, r.h - 20.0), BORDER);
    let left_end = x + 80.0;
    // the screens and saving, from the right
    let mut rx = r.right() - 8.0;
    let dirty = v.dirty();
    let save_label = save_label(v);
    let sw = (l.ui.width(&omsi_ui::tr(&save_label), 13.0, Weight::Bold) + 52.0).max(110.0);
    let sr = Rect::new(rx - sw, cy - fh * 0.5, sw, fh);
    super::super::super::tour::anchor("rf-tp-save", sr);
    if l.ui.button("tp-save", sr, &save_label, Some("save"), if dirty { ButtonKind::Primary } else { ButtonKind::Normal }) && dirty {
        save(l, v);
    }
    rx = sr.x - 10.0;
    // (a line in service: the day the change begins, tomorrow at the soonest)
    let runs = v.work.as_ref().is_some_and(|w| !w.map) && c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&v.line)).is_some_and(|cl| cl.service_from.is_some());
    let dw = 200.0;
    let dx = rx - dw;
    if dirty && in_service(v, c) {
        let days = co::dates::between(&c.date, &v.from);
        if days < 1 {
            v.from = co::dates::add(&c.date, 1);
        }
        if l.ui.icon_button("tp-from-earlier", Vec2::new(dx + 12.0, cy), 12.0, "chevron_left", "A day earlier") && days > 1 {
            v.from = co::dates::add(&v.from, -1);
        }
        let short = day_label(&v.from);
        l.ui.text_in(&format!("{} {short}", omsi_ui::tr("From")), Rect::new(dx + 26.0, r.y, dw - 52.0, r.h), 12.5, Weight::Bold, TEXT, Align::Center);
        if l.ui.icon_button("tp-from-later", Vec2::new(dx + dw - 12.0, cy), 12.0, "chevron_right", "A day later") && days < 60 {
            v.from = co::dates::add(&v.from, 1);
        }
    }
    if runs {
        rx = dx - 8.0;
    }
    l.ui.p().rect(Rect::new(rx - 2.0, r.y + 10.0, 1.0, r.h - 20.0), BORDER);
    rx -= 10.0;
    for (tab, icon, tip) in [(TIMETABLE, "grid_view", "Timetable"), (RUNNING, "schedule", "Running times")] {
        let b = Rect::new(rx - 40.0, r.y + 6.0, 40.0, r.h - 12.0);
        let (h, _, clicked) = l.ui.interact(id_of(&format!("tp-tab-{tab}")), b);
        let on = v.tab == tab;
        if on || h {
            l.ui.p().rounded(b, 6.0, if on { SKY.alpha(0.18) } else { CELL_HOVER });
        }
        if on {
            l.ui.p().rect(Rect::new(b.x + 6.0, b.bottom() - 3.0, b.w - 12.0, 3.0), SKY);
        }
        l.ui.icon(icon, b.center(), 20.0, if on { SKY } else if h { TEXT } else { TEXT_DIM });
        l.ui.tooltip(b, tip);
        if clicked && !on {
            v.tab = tab;
            v.cell = None;
            v.text.clear();
        }
        rx = b.x - 4.0;
    }
    // OUTBOUND / INBOUND in the middle of what is left
    let mid = (left_end + rx) * 0.5;
    let words = [omsi_ui::tr("Outbound").to_uppercase(), omsi_ui::tr("Inbound").to_uppercase()];
    let ws: Vec<f32> = words.iter().map(|w| l.ui.width(w, 13.5, Weight::Bold)).collect();
    let gap = 36.0;
    let mut tx = mid - (ws[0] + ws[1] + gap) * 0.5;
    let has_inbound = v.draft().is_some_and(|d| d.directions.len() > 1);
    for side in 0..2 {
        let tr_ = Rect::new(tx - 6.0, r.y + 4.0, ws[side] + 12.0, r.h - 8.0);
        let enabled = side == 0 || has_inbound;
        let (h, _, clicked) = l.ui.interact(id_of(&format!("tp-side-{side}")), tr_);
        let on = v.side == side;
        let col = if on { TEXT } else if !enabled { TEXT_FAINT } else if h { TEXT_SOFT } else { TEXT_DIM };
        l.ui.text_in(&words[side], Rect::new(tx, r.y, ws[side] + 2.0, r.h), 13.5, Weight::Bold, col, Align::Left);
        if on {
            l.ui.p().rect(Rect::new(tx, r.bottom() - 9.0, ws[side], 3.0), SKY);
        }
        if clicked && enabled && !on {
            v.side = side;
            v.pattern = 0;
            v.let_go();
            (v.scroll, v.hscroll) = (0.0, 0.0);
        }
        tx += ws[side] + gap;
    }
}

/// The line in service: a change of its timetable begins on a day to come.
fn in_service(v: &TimeplanView, c: &Company) -> bool {
    let Some(w) = v.work.as_ref() else { return false };
    !w.map && c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(&v.line)).is_some_and(|cl| cl.service_from.is_some()) && reg::timetable_differs(&w.own.live, &w.own.draft)
}

fn save_label(v: &TimeplanView) -> String {
    if v.dirty() { "Save".into() } else { "Saved".into() }
}

// --- the strip --------------------------------------------------------------------------------------

/// A stop of the strip: its name, its code, whether it is a timepoint.
struct Dot {
    name: String,
    code: String,
    timepoint: bool,
}

/// The strip in `clip`: the names at 45° in grey italics, the codes under them, the rail in the
/// line's colour at `rail_y` with a dot per stop (a timepoint full, a plain stop white inside),
/// the relief mark at the first. Returns the dot clicked.
fn strip(ui: &mut Ui, id: &str, dots: &[Dot], cols: Cols, clip: Rect, rail_y: f32, colour: Color, clickable: bool) -> Option<usize> {
    if dots.is_empty() {
        return None;
    }
    ui.push_clip(Rect::new(clip.x - 30.0, clip.y, clip.w + 200.0, clip.h), 0.0);
    let angle = -std::f32::consts::FRAC_PI_4;
    let (sn, cs) = (angle.sin(), angle.cos());
    for (j, d) in dots.iter().enumerate() {
        let cx = cols.cx(j);
        if cx < clip.x - cols.w || cx > clip.right() + cols.w {
            continue;
        }
        // the name, rising to the right from just over the code
        let mut name = d.name.clone();
        while ui.width(&name, 14.0, Weight::Medium) > 160.0 && name.chars().count() > 4 {
            name = name.chars().take(name.chars().count() - 2).collect::<String>() + "…";
            name = name.replace("……", "…");
        }
        let w = ui.width(&name, 14.0, Weight::Medium);
        let foot = Vec2::new(cx - 8.0, rail_y - 34.0);
        let at = foot + Vec2::new(cs, sn) * (w * 0.5);
        ui.text_rotated_italic(&name, 14.0, Weight::Medium, at, angle, NAME);
        ui.text_in(&d.code, Rect::new(cx - cols.w * 0.5, rail_y - 27.0, cols.w, 16.0), 12.0, Weight::Bold, TEXT, Align::Center);
    }
    // the rail, in the line's colour as it is in either look
    let (a, b) = (cols.cx(0), cols.cx(dots.len() - 1));
    ui.as_drawn();
    ui.p().rounded(Rect::new(a, rail_y - 2.5, (b - a).max(0.0), 5.0), 2.5, colour);
    ui.end_as_drawn();
    ui.icon("unfold_more", Vec2::new(a - 18.0, rail_y), 14.0, TEXT_SOFT);
    let mut hit = None;
    for (j, d) in dots.iter().enumerate() {
        let c = Vec2::new(cols.cx(j), rail_y);
        if c.x < clip.x - 10.0 || c.x > clip.right() + 10.0 {
            continue;
        }
        let r = Rect::new(c.x - 10.0, c.y - 10.0, 20.0, 20.0);
        let h = clickable && ui.hover(r);
        ui.as_drawn();
        if d.timepoint {
            ui.p().circle(c, if h { 8.5 } else { 7.0 }, colour);
        } else {
            ui.p().circle(c, if h { 7.5 } else { 6.0 }, colour);
            ui.p().circle(c, if h { 4.5 } else { 3.5 }, Color::WHITE);
        }
        ui.end_as_drawn();
        if clickable {
            let (_, _, clicked) = ui.interact(id_of(&format!("{id}-dot-{j}")), r);
            ui.tooltip(r, if d.timepoint { "Timepoint: click to make it a plain stop" } else { "Plain stop: click to make it a timepoint" });
            if clicked {
                hit = Some(j);
            }
        }
    }
    ui.pop_clip();
    hit
}

/// The stops of a direction as the strip's dots.
fn dots_of(l: &LineDesign, dir: usize) -> Vec<Dot> {
    let plan = tp::plan_of(l);
    let Some(d) = l.directions.get(dir) else { return Vec::new() };
    let names: Vec<&str> = d.stops.iter().map(|s| s.name.as_str()).collect();
    crate::mt::protect(names);
    d.stops.iter().enumerate().map(|(k, s)| Dot { name: s.name.trim().to_string(), code: s.id.to_string(), timepoint: plan.is_timepoint(dir, d, k) }).collect()
}

/// A typed cell's text: what is being typed, with a caret.
fn typed(ui: &mut Ui, text: &str, r: Rect, px: f32) {
    let w = ui.text_in(text, r, px, Weight::Regular, TEXT, Align::Center);
    if (ui.time % 1.0) < 0.55 {
        let x = r.center().x + w * 0.5 + 1.0;
        ui.p().rect(Rect::new(x, r.center().y - px * 0.6, 1.5, px * 1.2), SKY);
    }
}

/// A box of the grids: its fill (hovered, chosen), its edge; returns whether it was clicked.
fn grid_cell(ui: &mut Ui, id: &str, r: Rect, chosen: bool) -> bool {
    let (h, _, clicked) = ui.interact(id_of(id), r);
    ui.p().rect(r, if h { CELL_HOVER } else { CELL });
    ui.p().rect(Rect::new(r.x, r.y, r.w, 1.0), BORDER);
    ui.p().rect(Rect::new(r.x, r.y, 1.0, r.h), BORDER);
    if chosen {
        ui.p().rounded_border(r.inset(1.0), 2.0, 2.0, SKY);
    }
    clicked
}

// --- running times -----------------------------------------------------------------------------------

fn running_times(l: &mut Launcher, v: &mut TimeplanView, r: Rect) {
    let Some(line) = v.draft().cloned() else { return };
    let sides = tp::sides(&line);
    let side = &sides[v.side.min(1)];
    if side.patterns.is_empty() {
        l.ui.paragraph(&omsi_ui::tr("The line has no inbound direction."), Vec2::new(r.x + 24.0, r.y + 24.0), r.w - 48.0, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    v.pattern = v.pattern.min(side.patterns.len() - 1);
    let dir = side.patterns[v.pattern];
    let n = line.directions[dir].stops.len();
    let set = tp::run_set(&line, v.day, dir);
    let colour = line_colour(&line);
    l.ui.p().rounded(r, 8.0, PANE);
    // the left column: the pattern, then the bands' rail
    let left = Rect::new(r.x, r.y, LEFT, r.h);
    l.ui.p().rect(Rect::new(left.right() - 1.0, r.y + 8.0, 1.0, r.h - 16.0), BORDER);
    let rail_y = r.y + STRIP_H;
    if side.patterns.len() > 1 {
        let labels: Vec<String> = (0..side.patterns.len()).map(|k| format!("{}  ·  {}", tp::pattern_code(k), line.directions[side.patterns[k]].destination())).collect();
        crate::mt::protect(labels.iter().map(String::as_str));
        let mut k = v.pattern;
        if l.ui.select("tp-rt-pattern", Rect::new(left.x + 12.0, rail_y - 17.0, LEFT - 24.0, 34.0), &mut k, &labels) {
            v.pattern = k;
            v.cell = None;
        }
    } else {
        let code = Rect::new(left.x + 12.0, rail_y - 17.0, 54.0, 34.0);
        l.ui.p().rounded(code, 6.0, CELL);
        l.ui.text_in("-", code, 15.0, Weight::Bold, SKY, Align::Center);
        l.ui.tooltip(code, "The main pattern");
    }
    if n < 2 {
        l.ui.paragraph(&omsi_ui::tr("This pattern has fewer than two stops: give it its stops in the line editor."), Vec2::new(left.right() + 24.0, r.y + 24.0), r.w - LEFT - 48.0, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    // the strip over the segments
    let room = Rect::new(left.right() + 40.0, r.y, (r.w - LEFT - 150.0).max(100.0), r.h);
    let cols = fit_cols(room.x, room.w, n, 70.0, 150.0, &mut v.hscroll);
    let dots = dots_of(&line, dir);
    if let Some(k) = strip(&mut l.ui, "tp-rt", &dots, cols, room, rail_y, colour, true) {
        let d = line.directions[dir].clone();
        v.change(|line| {
            let mut plan = tp::plan_of(line);
            plan.toggle_timepoint(dir, &d, k);
            line.timeplan = Some(plan);
        });
    }
    // the bands: a row each, its time on the rail at the left
    let gy = rail_y + 44.0;
    let rail_x = left.x + 132.0;
    let bands = set.bands.len();
    let max_rows = ((r.bottom() - gy - 60.0) / RT_ROW).floor().max(1.0) as usize;
    let shown = bands.min(max_rows.max(1));
    l.ui.p().rect(Rect::new(rail_x - 1.5, gy, 3.0, shown as f32 * RT_ROW + 34.0), BORDER);
    kit::caps(&mut l.ui, Rect::new(left.x + 12.0, gy - 26.0, LEFT - 24.0, 14.0), "Time bands");
    let mut remove: Option<usize> = None;
    let mut add_at: Option<f32> = None;
    for k in 0..shown {
        let band = &set.bands[k];
        let y = gy + k as f32 * RT_ROW;
        let row = Rect::new(left.x, y, r.right() - left.x, RT_ROW);
        let hovered = l.ui.hover(row);
        // its time, clicked to be typed anew (the first is 00:00 always)
        let tr_ = Rect::new(left.x + 40.0, y, 76.0, RT_ROW);
        let chosen = v.cell == Some(Cell::BandFrom(k));
        if chosen && !v.text.is_empty() {
            typed(&mut l.ui, &v.text, tr_, 14.0);
        } else {
            l.ui.text_in(&tp::hm(band.from), tr_, 14.0, Weight::Medium, if chosen { SKY } else { TEXT }, Align::Right);
        }
        if k > 0 {
            let (_, _, clicked) = l.ui.interact(id_of(&format!("tp-band-from-{k}")), tr_);
            l.ui.tooltip(tr_, "When the band begins: click and type a time");
            if clicked {
                v.cell = Some(Cell::BandFrom(k));
                v.text.clear();
            }
        }
        l.ui.p().circle(Vec2::new(rail_x, y + RT_ROW * 0.5), 6.0, if chosen || hovered { TEXT_DIM } else { BORDER.lighten(0.25) });
        // the bin, on the band the mouse is over
        if bands > 1 && hovered && l.ui.icon_button(&format!("tp-band-del-{k}"), Vec2::new(left.x + 18.0, y + RT_ROW * 0.5), 12.0, "delete", "Take the band out") {
            remove = Some(k);
        }
        // ⊕ between this band and the next: a band in the middle
        if k + 1 < bands {
            let between = Rect::new(rail_x - 12.0, y + RT_ROW - 12.0, 24.0, 24.0);
            if l.ui.hover(between) || hovered {
                let mid = ((band.from + set.bands[k + 1].from) * 0.5 / 15.0).round() * 15.0;
                if l.ui.icon_button(&format!("tp-band-add-{k}"), between.center(), 9.0, "add", "Add a time band here") {
                    add_at = Some(mid);
                }
            }
        }
        // its running times
        for i in 0..n - 1 {
            let x0 = cols.cx(i);
            let cr = Rect::new(x0, y, cols.cx(i + 1) - x0, RT_ROW);
            if cr.right() < room.x - 2.0 || cr.x > room.right() + cols.w {
                continue;
            }
            let chosen = v.cell == Some(Cell::Run { band: k, seg: i });
            if grid_cell(&mut l.ui, &format!("tp-rt-{k}-{i}"), cr, chosen) {
                v.cell = Some(Cell::Run { band: k, seg: i });
                v.text.clear();
            }
            if chosen && !v.text.is_empty() {
                typed(&mut l.ui, &format!("{} m", v.text), cr, 18.0);
            } else {
                let m = band.run.get(i).copied().unwrap_or(0.0);
                minutes_cell(&mut l.ui, cr, m);
            }
        }
        // the trip's total in this band (a trip leaving when it begins)
        let tx = cols.cx(n - 1) + 14.0;
        if tx < r.right() - 20.0 {
            let total = set.total(band.from, n);
            l.ui.text_in(&format!("= {} m", tp::minutes_text(total)), Rect::new(tx, y, r.right() - tx - 8.0, RT_ROW), 15.0, Weight::Bold, TEXT_SOFT, Align::Left);
        }
    }
    if shown < bands {
        l.ui.text_in(&omsi_ui::tr("%{n} more bands").replace("%{n}", &(bands - shown).to_string()), Rect::new(left.x + 12.0, gy + shown as f32 * RT_ROW, LEFT, 20.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
    }
    // the right edge of the grid, and its caption
    let total_x = cols.cx(n - 1) + 14.0;
    if total_x < r.right() - 20.0 {
        kit::caps(&mut l.ui, Rect::new(total_x, gy - 26.0, r.right() - total_x, 14.0), "Trip time");
    }
    let last = cols.cx(n - 1);
    l.ui.p().rect(Rect::new(last, gy, 1.0, shown as f32 * RT_ROW), BORDER);
    l.ui.p().rect(Rect::new(cols.cx(0), gy + shown as f32 * RT_ROW, last - cols.cx(0), 1.0), BORDER);
    // 36:00 and ⊕ at the end of the rail
    let ey = gy + shown as f32 * RT_ROW + 22.0;
    l.ui.text_in("36:00", Rect::new(left.x + 40.0, ey - 10.0, 76.0, 20.0), 13.0, Weight::Medium, TEXT_DIM, Align::Right);
    if l.ui.icon_button("tp-band-add-end", Vec2::new(rail_x, ey), 10.0, "add", "Add a time band") {
        let last_from = set.bands.last().map(|b| b.from).unwrap_or(0.0);
        add_at = Some(if last_from < 7.0 * 60.0 { 7.0 * 60.0 } else { (last_from + 120.0).min(tp::DAY_SPAN - 60.0) });
    }
    l.ui.tooltip(Rect::new(rail_x - 12.0, ey - 12.0, 24.0, 24.0), "Add a time band");
    // the wheel scrolls the stops sideways, and the bar under them
    if l.ui.hover(room) && (l.ui.input.wheel.x.abs() > 0.0 || (l.ui.input.shift && l.ui.input.wheel.y.abs() > 0.0)) {
        let d = if l.ui.input.wheel.x.abs() > 0.0 { l.ui.input.wheel.x } else { l.ui.input.wheel.y };
        v.hscroll -= d * 60.0;
        l.ui.input.wheel = Vec2::ZERO;
    }
    let hbar = Rect::new(room.x, r.bottom() - super::rf_scroll::BAR - 4.0, room.w, super::rf_scroll::BAR);
    if let Some(x) = super::rf_scroll::bar(&mut l.ui, &BARS, "tp-rt-h", hbar, false, cols.scroll, room.w, cols.w * n as f32) {
        v.hscroll = x;
    }
    if let Some(k) = remove {
        v.change_run(dir, |s| {
            s.remove_band(k);
        });
        v.cell = None;
    }
    if let Some(t) = add_at {
        v.change_run(dir, |s| {
            s.add_band(t);
        });
        let k = tp::run_set(v.draft().unwrap_or(&line), v.day, dir).bands.iter().position(|b| (b.from - t).abs() < 0.5);
        if let Some(k) = k {
            v.cell = Some(Cell::BandFrom(k));
            v.text.clear();
        }
    }
}

/// A running time as Optibus shows it: the minutes large, a small grey "m" after them.
fn minutes_cell(ui: &mut Ui, r: Rect, m: f32) {
    let big = tp::minutes_text(m);
    let col = if m < -1e-3 { RED } else if m.abs() <= 1e-3 { ORANGE } else { TEXT };
    let w1 = ui.width(&big, 19.0, Weight::Regular);
    let w2 = ui.width("m", 12.0, Weight::Regular);
    let x = r.center().x - (w1 + 5.0 + w2) * 0.5;
    ui.text_in(&big, Rect::new(x, r.y, w1 + 2.0, r.h), 19.0, Weight::Regular, col, Align::Left);
    ui.text_in("m", Rect::new(x + w1 + 5.0, r.y + 2.0, w2 + 2.0, r.h), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
}

// --- the timetable -----------------------------------------------------------------------------------

/// The columns the timetable shows of a side: all, or its timepoints only (a column is one when
/// a pattern that stops there has it as one).
fn shown_columns(line: &LineDesign, side: &Side, points_only: bool) -> Vec<usize> {
    let plan = tp::plan_of(line);
    (0..side.columns.len())
        .filter(|&j| {
            !points_only
                || side.patterns.iter().enumerate().any(|(p, &dir)| side.col_of[p].iter().position(|c| *c == j).is_some_and(|k| line.directions.get(dir).is_some_and(|d| plan.is_timepoint(dir, d, k))))
        })
        .collect()
}

fn timetable(l: &mut Launcher, v: &mut TimeplanView, r: Rect) {
    let Some(line) = v.draft().cloned() else { return };
    let sides = tp::sides(&line);
    let side = sides[v.side.min(1)].clone();
    l.ui.p().rounded(r, 8.0, PANE);
    if side.patterns.is_empty() {
        l.ui.paragraph(&omsi_ui::tr("The line has no inbound direction."), Vec2::new(r.x + 24.0, r.y + 24.0), r.w - 48.0, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    let rows = tp::rows(&line, v.day, &side);
    let cols_shown = shown_columns(&line, &side, v.points_only);
    let frozen_w: f32 = FROZEN.iter().sum();
    let room = Rect::new(r.x + frozen_w + 10.0, r.y, (r.w - frozen_w - 20.0).max(80.0), r.h);
    let cols = fit_cols(room.x, room.w, cols_shown.len(), 78.0, 124.0, &mut v.hscroll);
    let rail_y = r.y + STRIP_H - 30.0;
    let colour = line_colour(&line);
    // the strip over the columns (the side's stops, all patterns together)
    let plan = tp::plan_of(&line);
    let dots: Vec<Dot> = cols_shown
        .iter()
        .map(|&j| {
            let c = &side.columns[j];
            let timepoint = side.patterns.iter().enumerate().any(|(p, &dir)| side.col_of[p].iter().position(|x| *x == j).is_some_and(|k| line.directions.get(dir).is_some_and(|d| plan.is_timepoint(dir, d, k))));
            Dot { name: c.name.clone(), code: c.id.to_string(), timepoint }
        })
        .collect();
    crate::mt::protect(dots.iter().map(|d| d.name.as_str()));
    strip(&mut l.ui, "tp-tt", &dots, cols, room, rail_y, colour, false);
    // the tools over the frozen columns
    let ty = rail_y - 16.0;
    let mut tx = r.x + 10.0;
    let tool = |ui: &mut Ui, name: &str, x: f32, icon: &str, tip: &str, on: bool| -> bool {
        let b = Rect::new(x, ty, 34.0, 32.0);
        let (h, _, clicked) = ui.interact(id_of(name), b);
        ui.p().rounded(b, 6.0, if on { SKY.alpha(0.2) } else if h { CELL_HOVER } else { CELL });
        ui.icon(icon, b.center(), 18.0, if on { SKY } else if h { TEXT } else { TEXT_SOFT });
        ui.tooltip(b, tip);
        clicked
    };
    super::super::super::tour::anchor("rf-tp-add", Rect::new(tx, ty, 74.0, 32.0));
    let add_one = tool(&mut l.ui, "tp-add-one", tx, "add", "Add a single trip (after the one chosen, at its headway)", false);
    tx += 40.0;
    let add_many = tool(&mut l.ui, "tp-add-many", tx, "schedule", "Add trips by headway from a control point", v.adding.is_some());
    tx += 40.0;
    let delete = tool(&mut l.ui, "tp-delete", tx, "delete", "Take the chosen trip out", false);
    tx += 40.0;
    let revert = tool(&mut l.ui, "tp-revert", tx, "restart_alt", "The chosen trip's times from the running times again (its typed times go)", false);
    tx += 40.0;
    if tool(&mut l.ui, "tp-flag", tx, "flag", "Timepoints only", v.points_only) {
        v.points_only = !v.points_only;
    }
    // the header of the frozen columns
    let hy = rail_y + 22.0;
    let heads = ["#", "Pattern", "Vehicle type", "Start", "Headway"];
    let mut x = r.x;
    for (k, w) in FROZEN.iter().enumerate() {
        let hr = Rect::new(x, hy, *w, 34.0);
        l.ui.p().rect(hr, CELL);
        l.ui.p().rect(Rect::new(hr.right() - 1.0, hr.y, 1.0, hr.h), BORDER);
        let word = if k == 0 { "#".to_string() } else { omsi_ui::tr(heads[k]).to_uppercase() };
        l.ui.text_in(&word, hr.inset(6.0), 11.0, Weight::Bold, TEXT_SOFT, if k == 0 { Align::Center } else { Align::Left });
        x += w;
    }
    let mut y0 = hy + 34.0;
    // the form that adds trips by headway
    if v.adding.is_some() {
        add_form(l, v, &line, &side, Rect::new(r.x, y0, r.w, FORM_H));
        y0 += FORM_H;
    }
    // the trips
    let wide = cols.w * cols_shown.len() as f32 > room.w + 0.5;
    let hbar_h = if wide { super::rf_scroll::BAR + 2.0 } else { 0.0 };
    let grid = Rect::new(r.x, y0, r.w, (r.bottom() - y0 - 4.0 - hbar_h).max(0.0));
    let visible = (grid.h / TT_ROW).floor().max(1.0) as usize;
    let max_scroll = rows.len().saturating_sub(visible) as f32;
    if l.ui.hover(grid) && l.ui.input.wheel.y.abs() > 0.0 && !l.ui.input.shift {
        v.scroll = (v.scroll - l.ui.input.wheel.y.signum() * 3.0).clamp(0.0, max_scroll);
        l.ui.input.wheel = Vec2::ZERO;
    }
    if l.ui.hover(Rect::new(room.x, r.y, room.w, r.h)) && (l.ui.input.wheel.x.abs() > 0.0 || (l.ui.input.shift && l.ui.input.wheel.y.abs() > 0.0)) {
        let d = if l.ui.input.wheel.x.abs() > 0.0 { l.ui.input.wheel.x } else { l.ui.input.wheel.y };
        v.hscroll -= d * 60.0;
        l.ui.input.wheel = Vec2::ZERO;
    }
    v.scroll = v.scroll.clamp(0.0, max_scroll);
    let first = v.scroll as usize;
    if rows.is_empty() {
        l.ui.text_in(&omsi_ui::tr("No trips on this day: add them by headway with the clock above, or one at a time with +."), Rect::new(r.x + 16.0, y0 + 8.0, r.w - 32.0, 30.0), 13.5, Weight::Regular, TEXT_DIM, Align::Left);
    }
    let sets: Vec<RunSet> = side.patterns.iter().map(|&dir| tp::run_set(&line, v.day, dir)).collect();
    let vt = vehicle_types(&line);
    crate::mt::protect([vt.as_str()]);
    l.ui.push_clip(grid, 0.0);
    for (k, &i) in rows.iter().enumerate().skip(first).take(visible + 1) {
        let y = grid.y + (k - first) as f32 * TT_ROW;
        let t = &line.table[i];
        let Some(p) = side.pattern(t.dir) else { continue };
        let times = tp::trip(&line, i);
        let chosen_row = v.sel == Some(i);
        // the frozen columns
        let mut x = r.x;
        let rr = Rect::new(r.x, y, r.w, TT_ROW);
        if chosen_row {
            l.ui.p().rect(rr, SKY.alpha(0.14));
        }
        let nr = Rect::new(x, y, FROZEN[0], TT_ROW);
        let (nh, _, nclick) = l.ui.interact(id_of(&format!("tp-row-{i}")), nr);
        l.ui.text_in(&(k + 1).to_string(), nr, 12.0, Weight::Regular, if chosen_row || nh { TEXT } else { TEXT_DIM }, Align::Center);
        if nclick {
            v.sel = if chosen_row { None } else { Some(i) };
        }
        x += FROZEN[0];
        let pr = Rect::new(x, y, FROZEN[1], TT_ROW);
        let (_, _, pclick) = l.ui.interact(id_of(&format!("tp-rowp-{i}")), pr);
        l.ui.p().rect(Rect::new(x, y, 1.0, TT_ROW), BORDER);
        l.ui.text_in(&tp::pattern_code(p), pr, 15.0, Weight::Bold, SKY, Align::Center);
        if pclick {
            v.sel = Some(i);
        }
        x += FROZEN[1];
        let vr = Rect::new(x, y, FROZEN[2], TT_ROW);
        l.ui.p().rect(Rect::new(x, y, 1.0, TT_ROW), BORDER);
        l.ui.text_in(&vt, vr, 13.5, Weight::Regular, TEXT, Align::Center);
        x += FROZEN[2];
        let dep = times.first().copied().unwrap_or(0.0);
        let sr = Rect::new(x, y, FROZEN[3], TT_ROW);
        let chosen = v.cell == Some(Cell::Start(i));
        if grid_cell(&mut l.ui, &format!("tp-start-{i}"), sr, chosen) {
            v.cell = Some(Cell::Start(i));
            v.text.clear();
            v.sel = Some(i);
        }
        if chosen && !v.text.is_empty() {
            typed(&mut l.ui, &v.text, sr, 15.0);
        } else {
            l.ui.text_in(&tp::hm(dep), sr, 15.0, Weight::Medium, TEXT, Align::Center);
        }
        x += FROZEN[3];
        let hr = Rect::new(x, y, FROZEN[4], TT_ROW);
        let chosen = v.cell == Some(Cell::Headway(i));
        if k > 0 {
            if grid_cell(&mut l.ui, &format!("tp-headway-{i}"), hr, chosen) {
                v.cell = Some(Cell::Headway(i));
                v.text.clear();
                v.sel = Some(i);
            }
            if chosen && !v.text.is_empty() {
                typed(&mut l.ui, &v.text, hr, 15.0);
            } else {
                let prev = tp::departure(&line, rows[k - 1]);
                l.ui.text_in(&tp::minutes_text(dep - prev), hr, 15.0, Weight::Medium, TEXT, Align::Center);
            }
        } else {
            l.ui.p().rect(hr, CELL);
            l.ui.text_in("-", hr, 15.0, Weight::Medium, TEXT_DIM, Align::Center);
        }
        l.ui.p().rect(Rect::new(r.x, y + TT_ROW - 1.0, frozen_w, 1.0), BORDER);
        // the time at every stop shown
        let off = tp::offsets(&sets[p], &times);
        l.ui.push_clip(Rect::new(room.x - 2.0, grid.y, room.w + 4.0, grid.h), 0.0);
        for (jj, &j) in cols_shown.iter().enumerate() {
            let x0 = cols.cx(jj) - cols.w * 0.5;
            let cr = Rect::new(x0, y, cols.w, TT_ROW);
            if cr.right() < room.x || cr.x > room.right() {
                continue;
            }
            let stop = side.col_of[p].iter().position(|c| *c == j);
            match stop {
                None => {
                    l.ui.p().rect(cr, CELL);
                    l.ui.p().rect(Rect::new(cr.x, cr.y, 1.0, cr.h), BORDER);
                    l.ui.text_in("—", cr, 15.0, Weight::Regular, TEXT_SOFT, Align::Center);
                }
                Some(s) => {
                    let chosen = v.cell == Some(Cell::Time(i, s));
                    if grid_cell(&mut l.ui, &format!("tp-time-{i}-{s}"), cr, chosen) {
                        v.cell = Some(Cell::Time(i, s));
                        v.text.clear();
                        v.sel = Some(i);
                    }
                    let t = times.get(s).copied().unwrap_or(0.0);
                    let o = off.get(s).copied().unwrap_or(0.0);
                    if chosen && !v.text.is_empty() {
                        typed(&mut l.ui, &v.text, cr, 16.0);
                    } else if o.abs() >= 0.5 {
                        // typed over: orange, in italics, which way it is off
                        let text = format!("{} {}", if o > 0.0 { "+" } else { "−" }, tp::hm(t));
                        l.ui.text_rotated_italic(&text, 15.5, Weight::Medium, cr.center(), 0.0, ORANGE);
                        l.ui.tooltip(cr, &omsi_ui::tr("From the running times: %{time}").replace("%{time}", &tp::hm(t - o)));
                    } else {
                        let col = if t > tp::DAY_SPAN { RED } else { TEXT };
                        l.ui.text_in(&tp::hm(t), cr, 16.0, Weight::Regular, col, Align::Center);
                    }
                }
            }
        }
        l.ui.p().rect(Rect::new(room.x, y + TT_ROW - 1.0, room.w, 1.0), BORDER);
        l.ui.pop_clip();
    }
    l.ui.pop_clip();
    // the scrollbars: down the trips (a row at a time), along the stops
    let track = Rect::new(r.right() - super::rf_scroll::BAR, grid.y, super::rf_scroll::BAR, grid.h);
    if let Some(k) = super::rf_scroll::bar(&mut l.ui, &BARS, "tp-tt-v", track, true, v.scroll, visible as f32, rows.len() as f32) {
        v.scroll = k.round().clamp(0.0, max_scroll);
    }
    if wide {
        let hbar = Rect::new(room.x, grid.bottom() + 2.0, room.w, super::rf_scroll::BAR);
        if let Some(x) = super::rf_scroll::bar(&mut l.ui, &BARS, "tp-tt-h", hbar, false, cols.scroll, room.w, cols.w * cols_shown.len() as f32) {
            v.hscroll = x;
        }
    }
    // the tools' work
    let chosen_k = v.sel.and_then(|i| rows.iter().position(|x| *x == i));
    if add_one {
        let dir = v.sel.and_then(|i| line.table.get(i)).map(|t| t.dir).unwrap_or(side.patterns[0]);
        let p = side.pattern(dir).unwrap_or(0);
        let mine: Vec<usize> = rows.iter().copied().filter(|&i| line.table[i].dir == dir).collect();
        let after = v.sel.filter(|i| mine.contains(i)).or(mine.last().copied());
        let headway = match after.and_then(|a| mine.iter().position(|x| *x == a)) {
            Some(k) if k > 0 => tp::departure(&line, mine[k]) - tp::departure(&line, mine[k - 1]),
            _ => 15.0,
        };
        let dep = after.map(|a| tp::departure(&line, a) + headway.max(1.0)).unwrap_or(tp::FIRST_TRIP);
        let day = v.day;
        let set = sets[p].clone();
        let mut added = None;
        v.change(|l| added = tp::add_trip(l, day, dir, dep, &set));
        v.sel = added;
        scroll_to(v, &line, &side, added);
    }
    if add_many {
        v.adding = match v.adding {
            Some(_) => None,
            None => Some(Adding { pattern: chosen_k.and_then(|k| line.table.get(rows[k])).and_then(|t| side.pattern(t.dir)).unwrap_or(0), control: 0, first: "06:00".into(), last: "22:00".into(), headway: "15".into() }),
        };
    }
    if delete {
        if let Some(i) = v.sel {
            v.change(|l| {
                tp::delete_trip(l, i);
            });
            v.sel = None;
            v.cell = None;
        } else {
            l.state.set_status(omsi_ui::tr("Choose a trip first: click its number.").into_owned(), false);
        }
    }
    if revert {
        if let Some(i) = v.sel {
            let p = line.table.get(i).and_then(|t| side.pattern(t.dir)).unwrap_or(0);
            let set = sets[p].clone();
            v.change(|l| tp::revert(l, i, &set));
        } else {
            l.state.set_status(omsi_ui::tr("Choose a trip first: click its number.").into_owned(), false);
        }
    }
}

/// The rows scrolled so the trip `i` (its place in the table) is in view.
fn scroll_to(v: &mut TimeplanView, _before: &LineDesign, side: &Side, i: Option<usize>) {
    let (Some(i), Some(line)) = (i, v.draft()) else { return };
    let rows = tp::rows(line, v.day, side);
    if let Some(k) = rows.iter().position(|x| *x == i) {
        let k = k as f32;
        if k < v.scroll || k > v.scroll + 8.0 {
            v.scroll = (k - 4.0).max(0.0);
        }
    }
}

/// The form that adds trips by headway (Optibus's "Add trips"): CONTROL POINT, START TIME, END
/// TIME, HEADWAY, and ✓ to make them.
fn add_form(l: &mut Launcher, v: &mut TimeplanView, line: &LineDesign, side: &Side, r: Rect) {
    let Some(mut a) = v.adding.clone() else { return };
    super::super::super::tour::anchor("rf-tp-form", r);
    l.ui.p().rect(r, SKY.alpha(0.07));
    l.ui.p().rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), BORDER);
    let fy = r.y + 24.0;
    let fh = 32.0;
    let mut x = r.x + 10.0;
    let label = |ui: &mut Ui, x: f32, w: f32, text: &str| {
        kit::caps(ui, Rect::new(x, r.y + 5.0, w, 14.0), text);
    };
    // the pattern (a side with more than one)
    if side.patterns.len() > 1 {
        let codes: Vec<String> = (0..side.patterns.len()).map(tp::pattern_code).collect();
        label(&mut l.ui, x, 80.0, "Pattern");
        let mut k = a.pattern.min(codes.len() - 1);
        if l.ui.select("tp-add-pattern", Rect::new(x, fy, 80.0, fh), &mut k, &codes) {
            a.pattern = k;
            a.control = 0;
        }
        x += 90.0;
    }
    a.pattern = a.pattern.min(side.patterns.len() - 1);
    let dir = side.patterns[a.pattern];
    let stops: Vec<String> = line.directions[dir].stops.iter().map(|s| s.name.trim().to_string()).collect();
    crate::mt::protect(stops.iter().map(String::as_str));
    label(&mut l.ui, x, 220.0, "Control point");
    if !stops.is_empty() {
        let mut k = a.control.min(stops.len() - 1);
        if l.ui.select("tp-add-control", Rect::new(x, fy, 220.0, fh), &mut k, &stops) {
            a.control = k;
        }
    }
    x += 230.0;
    label(&mut l.ui, x, 90.0, "Start time");
    l.ui.text_input("tp-add-first", Rect::new(x, fy, 90.0, fh), &mut a.first, "06:00", None);
    x += 100.0;
    label(&mut l.ui, x, 90.0, "End time");
    l.ui.text_input("tp-add-last", Rect::new(x, fy, 90.0, fh), &mut a.last, "22:00", None);
    x += 100.0;
    label(&mut l.ui, x, 120.0, "Headway");
    l.ui.text_input("tp-add-headway", Rect::new(x, fy, 84.0, fh), &mut a.headway, "10,20", None);
    l.ui.text_in(&omsi_ui::tr("min"), Rect::new(x + 90.0, fy, 40.0, fh), 12.0, Weight::Bold, TEXT_DIM, Align::Left);
    l.ui.tooltip(Rect::new(x, fy, 84.0, fh), "Minutes between the trips: several, such as 10,20, are taken in turn");
    x += 140.0;
    // what it makes, or what is wrong
    let first = tp::parse_time(&a.first);
    let last = tp::parse_time(&a.last);
    let heads = tp::parse_headways(&a.headway);
    let set = tp::run_set(line, v.day, dir);
    let n = line.directions[dir].stops.len();
    let made = match (first, last, &heads) {
        (Some(f), Some(t), Some(h)) if t >= f => Some(tp::generate(&set, n, a.control, f, t, h)),
        _ => None,
    };
    let note = match &made {
        Some(m) => omsi_ui::tr("%{n} trips").replace("%{n}", &m.len().to_string()),
        None => omsi_ui::tr("Type the times as 06:00 and the headway in minutes").into_owned(),
    };
    let ok_r = Rect::new(r.right() - 52.0, fy - 2.0, 40.0, fh + 4.0);
    let no_r = Rect::new(ok_r.x - 46.0, fy - 2.0, 40.0, fh + 4.0);
    l.ui.text_in(&note, Rect::new(x, fy, (no_r.x - x - 8.0).max(0.0), fh), 13.0, Weight::Medium, if made.is_some() { TEXT_SOFT } else { ORANGE }, Align::Left);
    let cancel = l.ui.button("tp-add-no", no_r, "", Some("close"), ButtonKind::Normal);
    super::super::super::tour::anchor("rf-tp-add-ok", ok_r);
    let ok = l.ui.button("tp-add-ok", ok_r, "", Some("check"), ButtonKind::Primary);
    l.ui.tooltip(ok_r, "Make the trips");
    v.adding = Some(a);
    if cancel {
        v.adding = None;
    }
    if ok {
        if let Some(m) = made {
            let day = v.day;
            let count = m.len();
            v.change(|l| {
                tp::add_generated(l, day, dir, m);
            });
            v.adding = None;
            l.state.set_status(omsi_ui::tr("%{n} trips added.").replace("%{n}", &count.to_string()), false);
        }
    }
}

// --- the figures ----------------------------------------------------------------------------------------

/// The trips of the day group and side shown: their places in the table, and the side.
fn shown_rows(v: &TimeplanView) -> Option<(LineDesign, Side, Vec<usize>)> {
    let line = v.draft()?.clone();
    let side = tp::sides(&line)[v.side.min(1)].clone();
    let rows = tp::rows(&line, v.day, &side);
    Some((line, side, rows))
}

const CHARTS: [&str; 3] = ["Number of trips", "Trip count by pattern", "Trip count by hour"];

/// The three figures: number of trips, trips by pattern, trips by hour - each opened into its
/// chart.
fn kpis(l: &mut Launcher, v: &mut TimeplanView, r: Rect) {
    let Some((line, side, rows)) = shown_rows(v) else { return };
    let gap = 10.0;
    let w = (r.w - gap * 2.0) / 3.0;
    let by_pattern = tp::by_pattern(&line, &side, &rows);
    let by_hour = tp::by_hour(&line, &rows);
    for k in 0..3 {
        let cell = Rect::new(r.x + k as f32 * (w + gap), r.y, w, r.h);
        let (h, _, clicked) = l.ui.interact(id_of(&format!("tp-kpi-{k}")), cell);
        l.ui.p().rounded(cell, 8.0, if h { CELL } else { PANE });
        let caption = Rect::new(cell.x, cell.bottom() - 26.0, cell.w, 18.0);
        l.ui.text_in(&omsi_ui::tr(CHARTS[k]).to_uppercase(), caption, 11.5, Weight::Bold, TEXT_SOFT, Align::Center);
        let top = Rect::new(cell.x + 16.0, cell.y + 10.0, cell.w - 32.0, cell.h - 40.0);
        match k {
            0 => {
                let num = rows.len().to_string();
                let nw = l.ui.width(&num, 28.0, Weight::Regular);
                let x = top.center().x - (nw + 50.0) * 0.5;
                l.ui.icon("location_on", Vec2::new(x + 10.0, top.center().y), 20.0, TEXT_DIM);
                l.ui.icon("directions_bus", Vec2::new(x + 32.0, top.center().y), 20.0, TEXT_DIM);
                l.ui.text_in(&num, Rect::new(x + 50.0, top.y, nw + 4.0, top.h), 28.0, Weight::Regular, SKY, Align::Left);
            }
            1 => mini_bars(&mut l.ui, top, &by_pattern),
            _ => mini_bars(&mut l.ui, top, &by_hour),
        }
        l.ui.tooltip(cell, "Click to open the chart");
        if clicked {
            v.chart = Some(k);
        }
    }
}

/// A small bar chart (the figures' tiles).
fn mini_bars(ui: &mut Ui, r: Rect, values: &[u32]) {
    let n = values.len().max(1);
    let most = values.iter().copied().max().unwrap_or(0).max(1) as f32;
    let bw = (r.w / n as f32).min(26.0);
    let x0 = r.center().x - bw * n as f32 * 0.5;
    ui.p().rect(Rect::new(x0, r.bottom(), bw * n as f32, 1.0), BORDER);
    for (k, v) in values.iter().enumerate() {
        let h = (r.h - 4.0) * *v as f32 / most;
        if h > 0.0 {
            ui.p().rect(Rect::new(x0 + k as f32 * bw + bw * 0.15, r.bottom() - h, bw * 0.7, h), SKY);
        }
    }
}

/// A figure as a larger chart over the screen: a blue title, bars with their count and share on
/// top, ‹ › to the other charts.
fn chart_card(l: &mut Launcher, v: &mut TimeplanView, area: Rect) {
    let Some(k) = v.chart else { return };
    let Some((line, side, rows)) = shown_rows(v) else { return };
    let w = (area.w - 80.0).min(860.0);
    let h = 420.0f32.min(area.h - 60.0);
    let r = Rect::new(area.center().x - w * 0.5, area.center().y - h * 0.5, w, h);
    l.ui.p().rect(area, Color::rgba(0, 0, 0, 0.45));
    l.ui.solid(r);
    l.ui.p().shadow(r, 10.0, 24.0, Color::rgba(0, 0, 0, 0.5));
    l.ui.p().rounded(r, 10.0, PANE);
    l.ui.p().rounded_border(r, 10.0, 1.0, BORDER);
    let head = Rect::new(r.x + 20.0, r.y + 14.0, r.w - 140.0, 26.0);
    l.ui.text_in(&omsi_ui::tr(CHARTS[k]).to_uppercase(), head, 15.0, Weight::Bold, SKY, Align::Left);
    let closing = l.ui.icon_button("tp-chart-close", Vec2::new(r.right() - 24.0, r.y + 27.0), 13.0, "close", "Close");
    let prev = l.ui.icon_button("tp-chart-prev", Vec2::new(r.right() - 88.0, r.y + 27.0), 13.0, "chevron_left", "The chart before");
    let next = l.ui.icon_button("tp-chart-next", Vec2::new(r.right() - 58.0, r.y + 27.0), 13.0, "chevron_right", "The next chart");
    let body = Rect::new(r.x + 24.0, r.y + 70.0, r.w - 48.0, r.h - 110.0);
    let total = rows.len().max(1) as f32;
    let (values, labels): (Vec<u32>, Vec<String>) = match k {
        0 => (vec![rows.len() as u32], vec![format!("{} · {}", line.number.trim(), day_name(v.day))]),
        1 => (tp::by_pattern(&line, &side, &rows), (0..side.patterns.len()).map(tp::pattern_code).collect()),
        _ => (tp::by_hour(&line, &rows).to_vec(), (0..36).map(|h| format!("{h:02}")).collect()),
    };
    crate::mt::protect(labels.iter().map(String::as_str));
    let n = values.len().max(1);
    let most = values.iter().copied().max().unwrap_or(0).max(1) as f32;
    let bw = (body.w / n as f32).min(120.0);
    let x0 = body.center().x - bw * n as f32 * 0.5;
    l.ui.p().rect(Rect::new(x0, body.bottom(), bw * n as f32, 1.0), BORDER);
    for (i, val) in values.iter().enumerate() {
        let bh = (body.h - 34.0) * *val as f32 / most;
        let bx = x0 + i as f32 * bw;
        if bh > 0.0 {
            l.ui.p().rect(Rect::new(bx + bw * 0.15, body.bottom() - bh, bw * 0.7, bh), SKY);
        }
        if *val > 0 {
            let top = if n > 12 { val.to_string() } else { format!("{val} ({:.0}%)", *val as f32 * 100.0 / total) };
            l.ui.text_in(&top, Rect::new(bx - 10.0, body.bottom() - bh - 20.0, bw + 20.0, 16.0), if n > 12 { 10.5 } else { 13.0 }, Weight::Bold, TEXT, Align::Center);
        }
        if n <= 12 || i % 2 == 0 {
            l.ui.text_in(&labels[i], Rect::new(bx - 6.0, body.bottom() + 6.0, bw + 12.0, 16.0), 11.0, Weight::Medium, TEXT_DIM, Align::Center);
        }
    }
    if closing || (l.ui.input.pressed && !r.contains(l.ui.input.mouse)) {
        v.chart = None;
    } else if prev {
        v.chart = Some((k + 2) % 3);
    } else if next {
        v.chart = Some((k + 1) % 3);
    }
}

// --- the status bar and the issues ------------------------------------------------------------------------

fn status_bar(l: &mut Launcher, v: &mut TimeplanView, c: &Company, r: Rect) {
    let Some(line) = v.draft().cloned() else { return };
    let issues = tp::issues(&line, v.day);
    let errors: usize = issues.iter().filter(|i| i.level == Level::Error).map(|i| i.n.max(1)).sum();
    let warnings: usize = issues.iter().filter(|i| i.level == Level::Warning).map(|i| i.n.max(1)).sum();
    let mut x = r.right();
    // the issues of the day group
    let words = omsi_ui::tr("Line %{n}: %{k}").replace("%{n}", line.number.trim()).replace("%{k}", &(errors + warnings).to_string());
    let ww = l.ui.width(&words, 12.5, Weight::Bold) + 40.0;
    let ir = Rect::new(x - ww, r.y + 2.0, ww, r.h - 4.0);
    let (h, _, clicked) = l.ui.interact(id_of("tp-issues"), ir);
    let col = if errors > 0 { RED } else if warnings > 0 { ORANGE } else { TEXT_DIM };
    l.ui.p().rounded(ir, 6.0, if h || v.issues_open { CELL_HOVER } else { PANE });
    l.ui.icon(if errors > 0 { "error" } else if warnings > 0 { "warning" } else { "check_circle" }, Vec2::new(ir.x + 16.0, ir.center().y), 16.0, col);
    l.ui.text_in(&words, Rect::new(ir.x + 30.0, ir.y, ir.w - 34.0, ir.h), 12.5, Weight::Bold, col, Align::Left);
    l.ui.tooltip(ir, "The issues of this day group");
    if clicked {
        v.issues_open = !v.issues_open;
    }
    x = ir.x - 10.0;
    // saved since the studio made the line's blocks: UNSYNCED, and Refresh
    let line_name = v.line.clone();
    let unsynced: Vec<usize> = v.unsynced.iter().filter(|u| u.0.eq_ignore_ascii_case(&line_name)).map(|u| u.1).collect();
    if !unsynced.is_empty() {
        let text = format!("{} – {}", omsi_ui::tr("UNSYNCED"), omsi_ui::tr("Refresh"));
        let uw = l.ui.width(&text, 12.5, Weight::Bold) + 40.0;
        let ur = Rect::new(x - uw, r.y + 2.0, uw, r.h - 4.0);
        super::super::super::tour::anchor("rf-tp-refresh", ur);
        let (h, _, clicked) = l.ui.interact(id_of("tp-refresh"), ur);
        l.ui.p().rounded(ur, 6.0, if h { ORANGE } else { ORANGE.alpha(0.85) });
        l.ui.icon("refresh", Vec2::new(ur.x + 16.0, ur.center().y), 16.0, Color::rgba(30, 24, 10, 1.0));
        l.ui.text_in(&text, Rect::new(ur.x + 30.0, ur.y, ur.w - 34.0, ur.h), 12.5, Weight::Bold, Color::rgba(30, 24, 10, 1.0), Align::Left);
        l.ui.tooltip(ur, "The timetable changed since the scheduling studio made the blocks of this line: make its blocks and duties anew");
        if clicked {
            let days = unsynced.clone();
            let name = line_name.clone();
            if act(l, |c| {
                for d in &days {
                    tp::resync(c, &name, *d);
                }
                Ok(())
            })
            .is_some()
            {
                v.unsynced.retain(|u| !u.0.eq_ignore_ascii_case(&name));
                reload_timetable(l);
                l.state.set_status(omsi_ui::tr("Line %{n}: the scheduling studio makes its blocks and duties anew.").replace("%{n}", line.number.trim()), false);
            }
        }
        x = ur.x - 10.0;
    }
    // what is going on, on the left
    let note = if v.leaving {
        (omsi_ui::tr("Changes not saved: press again to leave without them.").into_owned(), ORANGE)
    } else if let Some(w) = v.work.as_ref().filter(|w| !w.own.saved.pending_from.is_empty()) {
        (omsi_ui::tr("A new timetable waits for %{date}: it is the one shown, against the tours that run until then.").replace("%{date}", &day_label(&w.own.saved.pending_from)), TEXT_DIM)
    } else if v.cell.is_some() {
        (omsi_ui::tr("Type and press Enter (Tab: the next cell, Escape: leave it). Up and Down change it by a minute.").into_owned(), TEXT_DIM)
    } else if v.dirty() {
        (omsi_ui::tr("Changes not saved yet.").into_owned(), TEXT_DIM)
    } else {
        let _ = c;
        (omsi_ui::tr("Click a cell to type into it.").into_owned(), TEXT_DIM)
    };
    l.ui.text_in(&note.0, Rect::new(r.x + 6.0, r.y, (x - r.x - 12.0).max(0.0), r.h), 12.5, Weight::Medium, note.1, Align::Left);
}

/// The issues of the day group in a small card over the status bar: a red "!" for an error, an
/// orange one for a warning, how many; a row clicked shows its first trip.
fn issues_panel(l: &mut Launcher, v: &mut TimeplanView, status: Rect) {
    let Some(line) = v.draft().cloned() else { return };
    let issues = tp::issues(&line, v.day);
    let w = 460.0f32.min(status.w);
    let h = 52.0 + issues.len().max(1) as f32 * 34.0;
    let r = Rect::new(status.right() - w, status.y - h - 6.0, w, h);
    l.ui.solid(r);
    l.ui.p().shadow(r, 8.0, 18.0, Color::rgba(0, 0, 0, 0.45));
    l.ui.p().rounded(r, 8.0, PANE);
    l.ui.p().rounded_border(r, 8.0, 1.0, BORDER);
    let head = Rect::new(r.x, r.y, r.w, 38.0);
    l.ui.p().rounded(head, 8.0, SKY);
    l.ui.p().rect(Rect::new(head.x, head.bottom() - 8.0, head.w, 8.0), SKY);
    l.ui.text_in(&format!("{} {} · {}", omsi_ui::tr("Line"), line.number.trim(), day_name(v.day)), Rect::new(head.x + 14.0, head.y, head.w - 60.0, head.h), 13.5, Weight::Bold, Color::WHITE, Align::Left);
    if l.ui.icon_button("tp-issues-close", Vec2::new(head.right() - 20.0, head.center().y), 12.0, "close", "Close") {
        v.issues_open = false;
    }
    if issues.is_empty() {
        l.ui.text_in(&omsi_ui::tr("No issues: the timeplan of this day group is valid."), Rect::new(r.x + 14.0, head.bottom() + 6.0, r.w - 28.0, 28.0), 13.0, Weight::Medium, TEXT_SOFT, Align::Left);
    }
    for (k, i) in issues.iter().enumerate() {
        let row = Rect::new(r.x + 6.0, head.bottom() + 6.0 + k as f32 * 34.0, r.w - 12.0, 32.0);
        let (h, _, clicked) = l.ui.interact(id_of(&format!("tp-issue-{k}")), row);
        if h {
            l.ui.p().rounded(row, 6.0, CELL_HOVER);
        }
        let col = if i.level == Level::Error { RED } else { ORANGE };
        l.ui.icon(if i.level == Level::Error { "error" } else { "warning" }, Vec2::new(row.x + 16.0, row.center().y), 17.0, col);
        let count = i.n.max(1).to_string();
        let cw = l.ui.width(&count, 11.5, Weight::Bold) + 14.0;
        let badge = Rect::new(row.x + 32.0, row.center().y - 10.0, cw, 20.0);
        l.ui.p().rounded(badge, 10.0, BORDER);
        l.ui.text_in(&count, badge, 11.5, Weight::Bold, TEXT, Align::Center);
        let kind = if i.level == Level::Error { omsi_ui::tr("Error") } else { omsi_ui::tr("Warning") };
        let text = format!("{kind} - {}", omsi_ui::tr(i.text).replace("%{n}", &i.n.to_string()));
        l.ui.text_in(&text, Rect::new(badge.right() + 10.0, row.y, row.right() - badge.right() - 14.0, row.h), 12.5, Weight::Medium, TEXT, Align::Left);
        if clicked {
            if let Some(&t) = i.trips.first() {
                if let Some(dir) = line.table.get(t).map(|x| x.dir) {
                    let sides = tp::sides(&line);
                    v.side = if sides[1].patterns.contains(&dir) { 1 } else { 0 };
                    v.tab = TIMETABLE;
                    v.sel = Some(t);
                    let side = sides[v.side].clone();
                    scroll_to(v, &line, &side, Some(t));
                }
            } else if i.text == tp::NEGATIVE || i.text == tp::ZERO {
                v.tab = RUNNING;
            }
            v.issues_open = false;
        }
    }
    if l.ui.input.pressed && !r.contains(l.ui.input.mouse) && !status.contains(l.ui.input.mouse) {
        v.issues_open = false;
    }
}

// --- typing ------------------------------------------------------------------------------------------------

/// The keys: typed into the chosen cell (Enter takes it, Tab takes it and goes on, Escape leaves
/// it, Up and Down change it by a minute); Delete takes the chosen trip out; Escape lets go of
/// what is in hand, then leaves the timeplan.
fn keys(l: &mut Launcher, v: &mut TimeplanView) {
    if l.ui.focus.is_some() {
        return;
    }
    let keys = l.ui.input.keys.clone();
    let text = l.ui.input.text.clone();
    if let Some(cell) = v.cell {
        for ch in text.chars() {
            if ch.is_ascii_digit() || matches!(ch, ':' | '.' | ',' | '-') {
                v.text.push(ch);
            }
        }
        for k in keys {
            match k {
                Key::Backspace => {
                    v.text.pop();
                }
                Key::Escape => {
                    v.cell = None;
                    v.text.clear();
                }
                Key::Enter | Key::Tab => {
                    let t = std::mem::take(&mut v.text);
                    if !t.is_empty() {
                        apply(l, v, cell, &t);
                    }
                    v.cell = if k == Key::Tab { next_cell(v, cell) } else { None };
                }
                Key::Up | Key::Down => {
                    v.text.clear();
                    nudge(v, cell, if k == Key::Up { 1.0 } else { -1.0 });
                }
                _ => {}
            }
        }
        return;
    }
    for k in keys {
        match k {
            Key::Delete if v.sel.is_some() && v.tab == TIMETABLE => {
                let i = v.sel.take().unwrap();
                v.change(|l| {
                    tp::delete_trip(l, i);
                });
            }
            Key::Escape if v.holds() => v.let_go(),
            Key::Escape => {
                if v.dirty() && !v.leaving {
                    v.leaving = true;
                } else {
                    v.open = false;
                    v.leaving = false;
                    v.loaded = None;
                }
            }
            _ => {}
        }
    }
}

/// The direction of the running times shown.
fn shown_dir(v: &TimeplanView) -> Option<usize> {
    let line = v.draft()?;
    let side = &tp::sides(line)[v.side.min(1)];
    side.patterns.get(v.pattern.min(side.patterns.len().saturating_sub(1))).copied()
}

/// What was typed into `cell` taken.
fn apply(l: &mut Launcher, v: &mut TimeplanView, cell: Cell, text: &str) {
    let Some(line) = v.draft().cloned() else { return };
    let bad = |l: &mut Launcher, what: &str| l.state.set_status(omsi_ui::tr(what).into_owned(), true);
    match cell {
        Cell::Run { band, seg } => match tp::parse_minutes(text) {
            Some(m) => {
                if let Some(dir) = shown_dir(v) {
                    v.change_run(dir, |s| {
                        if let Some(x) = s.bands.get_mut(band).and_then(|b| b.run.get_mut(seg)) {
                            *x = m;
                        }
                    });
                }
            }
            None => bad(l, "Type the minutes, such as 7"),
        },
        Cell::BandFrom(k) => match tp::parse_time(text) {
            Some(t) => {
                if let Some(dir) = shown_dir(v) {
                    v.change_run(dir, |s| {
                        s.move_band(k, t);
                    });
                }
            }
            None => bad(l, "Type a time such as 07:45 (up to 36:00)"),
        },
        Cell::Start(i) | Cell::Headway(i) => {
            let Some(dir) = line.table.get(i).map(|t| t.dir) else { return };
            let sides = tp::sides(&line);
            let side = if sides[1].patterns.contains(&dir) { &sides[1] } else { &sides[0] };
            let rows = tp::rows(&line, v.day, side);
            let Some(k) = rows.iter().position(|x| *x == i) else { return };
            if let Cell::Start(_) = cell {
                match tp::parse_time(text) {
                    Some(t) => {
                        let t = near_time(t, tp::departure(&line, i));
                        v.change(|l| tp::set_start(l, &rows, k, t));
                    }
                    None => bad(l, "Type a time such as 07:45 (up to 36:00)"),
                }
            } else {
                match tp::parse_minutes(text).filter(|m| *m >= 0.0) {
                    Some(m) => v.change(|l| tp::set_headway(l, &rows, k, m)),
                    None => bad(l, "Type the minutes, such as 7"),
                }
            }
        }
        Cell::Time(i, s) => match tp::parse_time(text) {
            Some(t) => {
                let at = tp::trip(&line, i).get(s).copied().unwrap_or(t);
                let t = near_time(t, at);
                v.change(|l| tp::set_cell(l, i, s, t));
            }
            None => bad(l, "Type a time such as 07:45 (up to 36:00)"),
        },
    }
}

/// `cell` a minute later (earlier: `by` -1): a running time, a band's beginning, a start, a
/// headway, a time.
fn nudge(v: &mut TimeplanView, cell: Cell, by: f32) {
    let Some(line) = v.draft().cloned() else { return };
    match cell {
        Cell::Run { band, seg } => {
            if let Some(dir) = shown_dir(v) {
                v.change_run(dir, |s| {
                    if let Some(x) = s.bands.get_mut(band).and_then(|b| b.run.get_mut(seg)) {
                        *x = (*x + by).max(0.0);
                    }
                });
            }
        }
        Cell::BandFrom(k) => {
            if let Some(dir) = shown_dir(v) {
                v.change_run(dir, |s| {
                    let from = s.bands.get(k).map(|b| b.from).unwrap_or(0.0);
                    s.move_band(k, from + by * 5.0);
                });
            }
        }
        Cell::Start(i) | Cell::Headway(i) => {
            let Some(dir) = line.table.get(i).map(|t| t.dir) else { return };
            let sides = tp::sides(&line);
            let side = if sides[1].patterns.contains(&dir) { &sides[1] } else { &sides[0] };
            let rows = tp::rows(&line, v.day, side);
            if let Some(k) = rows.iter().position(|x| *x == i) {
                let t = tp::departure(&line, i) + by;
                v.change(|l| tp::set_start(l, &rows, k, t));
            }
        }
        Cell::Time(i, s) => {
            let t = tp::trip(&line, i).get(s).copied().unwrap_or(0.0) + by;
            v.change(|l| tp::set_cell(l, i, s, t));
        }
    }
}

/// The cell Tab goes on to: the next segment, the next stop, the headway after the start.
fn next_cell(v: &TimeplanView, cell: Cell) -> Option<Cell> {
    let line = v.draft()?;
    match cell {
        Cell::Run { band, seg } => {
            let dir = shown_dir(v)?;
            let n = line.directions.get(dir)?.stops.len().saturating_sub(1);
            let bands = tp::run_set(line, v.day, dir).bands.len();
            if seg + 1 < n {
                Some(Cell::Run { band, seg: seg + 1 })
            } else if band + 1 < bands {
                Some(Cell::Run { band: band + 1, seg: 0 })
            } else {
                None
            }
        }
        Cell::Start(i) => Some(Cell::Headway(i)),
        Cell::Time(i, s) => {
            let n = line.table.get(i).and_then(|t| line.directions.get(t.dir)).map(|d| d.stops.len()).unwrap_or(0);
            (s + 1 < n).then_some(Cell::Time(i, s + 1))
        }
        _ => None,
    }
}

// --- saving -------------------------------------------------------------------------------------------------

/// The trips of each day group, to see which changed.
fn day_trips(l: &LineDesign, day: usize) -> Vec<(usize, Vec<i32>)> {
    let mut v: Vec<(usize, Vec<i32>)> = l.table.iter().filter(|t| tp::in_day(l, t, day)).map(|t| (t.dir, reg::trip_times(l, t).iter().map(|x| x.round() as i32).collect())).collect();
    v.sort();
    v
}

/// The timeplan saved as the company's lines' timetables are (`plan_edit::commit`; a line of the
/// map's at once, its changed route approved as the line editor has it), then the line read
/// again and marked UNSYNCED for the scheduling studio on the day groups it changed.
fn save(l: &mut Launcher, v: &mut TimeplanView) -> bool {
    let Some(w) = v.work.clone() else { return false };
    for d in 0..DAY_GROUPS.len() {
        if let Some(e) = tp::issues(&w.own.draft, d).into_iter().find(|i| i.level == Level::Error) {
            let text = omsi_ui::tr("%{day}: %{issue} - mend it first.").replace("%{day}", &day_name(d)).replace("%{issue}", &omsi_ui::tr(e.text).replace("%{n}", &e.n.to_string()));
            l.state.set_status(text, true);
            v.day = d;
            v.issues_open = true;
            return false;
        }
    }
    let changed: Vec<usize> = (0..DAY_GROUPS.len()).filter(|&d| day_trips(&w.own.base, d) != day_trips(&w.own.draft, d)).collect();
    let saved = if w.map {
        save_map(l, v, &w)
    } else {
        match plan_edit::commit(l, &v.company, &v.line, &w.own, &mut v.from) {
            Commit::Saved => true,
            Commit::Stale(_) | Commit::Gone => {
                v.loaded = None;
                false
            }
            Commit::Kept => false,
        }
    };
    if saved {
        // (the line under its new name, if its number changed it)
        let id = w.own.id;
        if let Some(name) = l.company.company.as_ref().and_then(|c| c.lines.iter().find(|x| x.own && x.plan.as_ref().is_some_and(|p| p.line_id == id)).map(|x| x.name.clone())) {
            v.line = name;
        }
        for d in changed {
            if !v.unsynced.iter().any(|u| u.0.eq_ignore_ascii_case(&v.line) && u.1 == d) {
                v.unsynced.push((v.line.clone(), d));
            }
        }
        v.loaded = None;
    }
    saved
}

/// A line of the map's the company runs, saved: its changed route paid for
/// (`ownline::apply_map_change`), the registry and the timetable written.
fn save_map(l: &mut Launcher, v: &mut TimeplanView, w: &Work) -> bool {
    let o = &w.own;
    if let Some(p) = reg::problems(&o.draft).first() {
        l.state.set_status(omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string()), true);
        return false;
    }
    let r = reg::load_registry(&o.path);
    let Some(on_disk) = r.line(o.id).cloned() else {
        l.state.set_status(omsi_ui::tr(plan_edit::NOT_FOUND).into_owned(), true);
        v.loaded = None;
        return false;
    };
    if on_disk != o.saved {
        kit::show(l, kit::Popup::new("departure_board", "Changed meanwhile", omsi_ui::tr("The line's timetable was changed meanwhile - in the line editor, or its new timetable began: it is shown as it is now."), "", None));
        v.loaded = None;
        return false;
    }
    let Some(ttl) = o.draft.map.as_ref().map(|m| m.ttl.clone()) else { return false };
    let Some(map) = l.company.company.as_ref().map(|c| c.map.clone()) else { return false };
    let (old, new) = (o.live.clone(), o.draft.clone());
    let Some(paid) = act(l, |c| ownline::apply_map_change(c, &ttl, &old, &new, false)) else { return false };
    let number = new.number.trim().to_string();
    let result = plan_edit::write_line(l, &map, &o.path, LineDesign { live: None, pending_from: String::new(), ..new });
    reload_timetable(l);
    let text = omsi_ui::tr("Line %{n} saved.").replace("%{n}", &number) + &plan_edit::approved(paid);
    match result {
        Ok(None) => l.state.set_status(text, false),
        Ok(Some(depots)) => l.state.set_status(format!("{text} {}: {depots}", omsi_ui::tr("A depot file could not be written (displays and IBIS)")), true),
        Err(err) => l.state.set_status(format!("{}: {err}", omsi_ui::tr("The timetable files could not be written")), true),
    }
    true
}

/// Every English text of the timeplan (for the translations' check).
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn texts() -> Vec<&'static str> {
    let mut v = vec![
        MAP_NOT_EDITED,
        NO_LINE,
        "The timeplan has changes not saved: choose again to drop them.",
        "Undo",
        "Redo",
        "A day earlier",
        "A day later",
        "From",
        "Timetable",
        "Running times",
        "Outbound",
        "Inbound",
        "Save",
        "Saved",
        "Timepoint: click to make it a plain stop",
        "Plain stop: click to make it a timepoint",
        "The line has no inbound direction.",
        "The main pattern",
        "This pattern has fewer than two stops: give it its stops in the line editor.",
        "Time bands",
        "When the band begins: click and type a time",
        "Take the band out",
        "Add a time band here",
        "Add a time band",
        "%{n} more bands",
        "Trip time",
        "Add a single trip (after the one chosen, at its headway)",
        "Add trips by headway from a control point",
        "Take the chosen trip out",
        "The chosen trip's times from the running times again (its typed times go)",
        "Timepoints only",
        "Pattern",
        "Vehicle type",
        "Start",
        "Headway",
        "No trips on this day: add them by headway with the clock above, or one at a time with +.",
        "All",
        "From the running times: %{time}",
        "Choose a trip first: click its number.",
        "Control point",
        "Start time",
        "End time",
        "min",
        "Minutes between the trips: several, such as 10,20, are taken in turn",
        "%{n} trips",
        "Type the times as 06:00 and the headway in minutes",
        "Make the trips",
        "%{n} trips added.",
        "Click to open the chart",
        "Close",
        "The chart before",
        "The next chart",
        "Line %{n}: %{k}",
        "The issues of this day group",
        "UNSYNCED",
        "Refresh",
        "The timetable changed since the scheduling studio made the blocks of this line: make its blocks and duties anew",
        "Line %{n}: the scheduling studio makes its blocks and duties anew.",
        "Changes not saved: press again to leave without them.",
        "Type and press Enter (Tab: the next cell, Escape: leave it). Up and Down change it by a minute.",
        "Changes not saved yet.",
        "Click a cell to type into it.",
        "Line",
        "No issues: the timeplan of this day group is valid.",
        "Error",
        "Warning",
        "Type the minutes, such as 7",
        "Type a time such as 07:45 (up to 36:00)",
        "%{day}: %{issue} - mend it first.",
        "Line %{n} saved.",
        "Timeplan",
        "The line's running times and timetable, as a scheduler plans them",
    ];
    v.extend(CHARTS);
    v.extend(tp::texts());
    v.extend(DAY_GROUPS.iter().map(|d| d.0));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_share_the_room_and_scroll_when_they_do_not_fit() {
        let mut s = 0.0;
        let c = fit_cols(100.0, 600.0, 6, 70.0, 150.0, &mut s);
        assert_eq!((c.w, c.cx(0)), (100.0, 150.0));
        // too many: as narrow as allowed, scrolled no further than the last column
        let mut s = 5000.0;
        let c = fit_cols(0.0, 600.0, 20, 70.0, 150.0, &mut s);
        assert_eq!((c.w, s), (70.0, 800.0));
        assert_eq!(c.cx(19), 19.5 * 70.0 - 800.0);
    }

    #[test]
    fn a_time_typed_for_a_late_trip_is_after_midnight() {
        assert_eq!(near_time(5.0, 1430.0), 1445.0);
        assert_eq!(near_time(425.0, 420.0), 425.0);
        // (never past 36:00)
        assert_eq!(near_time(800.0, 2000.0), 800.0);
    }

    #[test]
    fn the_vehicle_types_are_short() {
        let mut l = LineDesign::default();
        l.vehicles.classes = vec![VehicleClass::Solo, VehicleClass::Double];
        assert_eq!(vehicle_types(&l), "SD,DD");
    }

    #[test]
    fn every_text_of_the_timeplan_is_translated() {
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in texts() {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }
}
