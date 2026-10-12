//! RosterFlow - "Bus Scheduling & Crew Rostering" - the advanced mode's Planning page (Luc:
//! the company's planning should look and work exactly like Optibus's scheduling module,
//! under a name of its own). The page as Optibus lays it out, top to bottom: the title bar
//! (the mark and the name, the menu's icons, the schedule's name with its saved dot, undo,
//! redo and the history, the day switcher, and the tabs Vehicles, Drivers, Roster and
//! Timeplan), the KPI tiles (opened to four rows with the histograms), the toolbar (SAVE,
//! DISCARD, Stats, Layers, Filter ... OPTIMIZE), the Gantt (`studio`), and the status bar
//! (what is assigned, the Global, Duty and Vehicle issues, the zoom).
//!
//! It has a dark look - Optibus's navy with one blue accent, orange warnings and red errors -
//! and a light one designed to match (`Pal`); the lines keep their own colours in both. The
//! edits are a draft (`co::rosterflow::Flow::draft`) until SAVE: the roster, the day's close
//! and the game keep the saved blocks and duties meanwhile, DISCARD goes back to them.

use super::super::super::ui::{id_of, Key, Ui};
use super::super::super::Launcher;
use super::super::{act, day_label, eur};
use super::{rf_brand, rf_gantt, rf_panels, rf_prefs};
use glam::Vec2;
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, Draft, Figures, Scope};
use omsi_launcher_lib::company::{self as co, Cents, Company};
use omsi_launcher_lib::LineInfo;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::rc::Rc;
use winit::keyboard::KeyCode;

// --- the look --------------------------------------------------------------------------------

/// RosterFlow's colours, the dark look's (Optibus's navy) or the light one's.
#[derive(Clone, Copy)]
pub(super) struct Pal {
    pub bg: Color,
    pub bar: Color,
    pub tool: Color,
    pub grid: Color,
    pub band: Color,
    pub labels: Color,
    pub axis: Color,
    pub status: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub line: Color,
    pub accent: Color,
    pub warn: Color,
    pub err: Color,
    pub ok: Color,
    pub grey: Color,
    pub dead: Color,
    pub pull: Color,
    pub card: Color,
    pub card_head: Color,
    pub card_text: Color,
    pub card_dim: Color,
    pub card_pill: Color,
    pub menu: Color,
    pub menu_text: Color,
    pub sheet: Color,
    pub field: Color,
    pub light: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgba(r, g, b, 1.0)
}

/// The dark look: Optibus's navy, one blue accent, orange warnings, red errors.
pub(super) const DARK: Pal = Pal {
    bg: rgb(39, 46, 56),
    bar: rgb(33, 39, 48),
    tool: rgb(59, 74, 92),
    grid: rgb(39, 46, 56),
    band: rgb(44, 51, 61),
    labels: rgb(29, 33, 39),
    axis: rgb(33, 39, 48),
    status: rgb(31, 36, 44),
    text: rgb(226, 230, 236),
    dim: rgb(150, 158, 172),
    faint: rgb(104, 112, 126),
    line: Color::rgba(255, 255, 255, 0.07),
    accent: rgb(63, 168, 255),
    warn: rgb(255, 166, 38),
    err: rgb(240, 72, 72),
    ok: rgb(70, 196, 120),
    grey: rgb(112, 124, 140),
    dead: rgb(20, 23, 28),
    pull: rgb(72, 86, 106),
    card: rgb(232, 236, 241),
    card_head: rgb(20, 60, 104),
    card_text: rgb(40, 48, 60),
    card_dim: rgb(110, 120, 134),
    card_pill: rgb(208, 214, 222),
    menu: rgb(222, 227, 233),
    menu_text: rgb(36, 44, 56),
    sheet: rgb(46, 54, 66),
    field: rgb(31, 37, 46),
    light: false,
};

/// The light look, made to match: white sheets on a cool grey, the same blue a shade deeper
/// (it reads on white), the warnings and errors as strong.
pub(super) const LIGHT: Pal = Pal {
    bg: rgb(240, 243, 247),
    bar: rgb(255, 255, 255),
    tool: rgb(226, 233, 242),
    grid: rgb(255, 255, 255),
    band: rgb(246, 248, 251),
    labels: rgb(236, 240, 245),
    axis: rgb(244, 246, 249),
    status: rgb(248, 250, 252),
    text: rgb(28, 36, 48),
    dim: rgb(92, 104, 120),
    faint: rgb(150, 160, 174),
    line: Color::rgba(20, 36, 60, 0.09),
    accent: rgb(18, 124, 226),
    warn: rgb(226, 128, 10),
    err: rgb(214, 46, 46),
    ok: rgb(30, 150, 86),
    grey: rgb(150, 160, 174),
    dead: rgb(52, 58, 68),
    pull: rgb(138, 152, 172),
    card: rgb(255, 255, 255),
    card_head: rgb(20, 60, 104),
    card_text: rgb(34, 42, 54),
    card_dim: rgb(110, 120, 134),
    card_pill: rgb(226, 231, 238),
    menu: rgb(255, 255, 255),
    menu_text: rgb(34, 42, 54),
    sheet: rgb(255, 255, 255),
    field: rgb(246, 248, 251),
    light: true,
};

pub(super) fn pal(ui: &Ui) -> Pal {
    if ui.light {
        LIGHT
    } else {
        DARK
    }
}

/// Run `f` with the launcher's own controls (a text field, a list): out of the "as drawn"
/// span RosterFlow draws in, so the light look carries them over as it does elsewhere.
pub(super) fn plain<T>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> T) -> T {
    ui.end_as_drawn();
    let out = f(ui);
    ui.as_drawn();
    out
}

/// A button of the page: `solid` blue, else grey; its label in capitals.
pub(super) fn rf_button(ui: &mut Ui, p: &Pal, name: &str, r: Rect, label: &str, solid: bool, enabled: bool) -> bool {
    let (h, _, clicked) = ui.interact(id_of(name), r);
    let base = if solid { p.accent } else if p.light { rgb(196, 204, 214) } else { rgb(104, 116, 132) };
    let fill = if !enabled { base.alpha(0.45) } else if h { base.lighten(0.08) } else { base };
    ui.p().rounded(r, 2.0, fill);
    let ink = if solid || !p.light { Color::WHITE } else { p.text };
    let words = omsi_ui::tr(label).to_uppercase();
    ui.text_as_is(&words, Vec2::new(r.center().x, r.center().y + 4.5), 12.5, Weight::Bold, ink.alpha(if enabled { 1.0 } else { 0.6 }), Align::Center);
    clicked && enabled
}

/// An icon that is a button; `on` marks it.
pub(super) fn rf_icon(ui: &mut Ui, p: &Pal, name: &str, c: Vec2, icon: &str, tip: &str, on: bool, enabled: bool) -> bool {
    let r = Rect::new(c.x - 15.0, c.y - 15.0, 30.0, 30.0);
    let (h, _, clicked) = ui.interact(id_of(name), r);
    if h && enabled {
        ui.p().rounded(r, 4.0, p.text.alpha(0.08));
    }
    let col = if !enabled { p.faint.alpha(0.5) } else if on { p.accent } else if h { p.text } else { p.dim };
    ui.icon(icon, c, 19.0, col);
    if !tip.is_empty() {
        ui.tooltip(r, tip);
    }
    clicked && enabled
}

/// A word with an icon before it, as the toolbar's Stats, Layers and Filter.
pub(super) fn tool_link(ui: &mut Ui, p: &Pal, name: &str, x: f32, cy: f32, icon: &str, label: &str, on: bool) -> (bool, Rect) {
    let w = 26.0 + ui.width(&omsi_ui::tr(label), 14.0, Weight::Medium) + 10.0;
    let r = Rect::new(x, cy - 14.0, w, 28.0);
    let (h, _, clicked) = ui.interact(id_of(name), r);
    let col = if on { p.accent } else if h { p.text } else { p.text.alpha(0.86) };
    ui.icon(icon, Vec2::new(x + 11.0, cy), 18.0, col);
    ui.text_in(label, Rect::new(x + 26.0, r.y, w - 26.0, r.h), 14.0, Weight::Medium, col, Align::Left);
    (clicked, r)
}

/// A small caps label.
pub(super) fn caps(ui: &mut Ui, r: Rect, text: &str, c: Color, align: Align) {
    let t = omsi_ui::tr(text).to_uppercase();
    ui.text_in(&t, r, 11.0, Weight::Bold, c, align);
}

// --- the state ---------------------------------------------------------------------------------

/// A row of the Gantt by what it is (kept across the day's rebuilds): a bus's block (its line
/// and name) or a duty (its key).
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub(super) enum RowId {
    Block(String, String),
    Duty(String),
}

pub(super) fn block_row(t: &co::network::TourOfDay) -> RowId {
    RowId::Block(t.line.clone(), t.tour.clone())
}

pub(super) fn duty_row(s: &rf::Shaped) -> RowId {
    match (&s.key, s.spare) {
        (Some(k), _) => RowId::Duty(format!("{}/{}/{}/{}", k.line, k.tour, k.first.name, k.first.dep)),
        (None, Some(id)) => RowId::Duty(format!("spare/{id}")),
        _ => RowId::Duty(String::new()),
    }
}

/// What is taken up to be dropped (Optibus's MOVE): trips of a bus, a trip of the stack, a
/// duty's piece; or trips to swap with the row picked next.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Hand {
    Trips { tour: usize, trips: Vec<usize> },
    Stack(usize),
    Piece { tour: usize, duty: usize },
    Swap { tour: usize, trips: Vec<usize>, how: co::studio::Swap },
}

/// Where something taken up may go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Drop {
    Block(usize),
    New,
    Stack,
    Duty(usize),
    NewDuty,
}

/// A drop zone: where, rated 1 to 5 stars, the cost it makes, and whether it breaks a rule.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Zone {
    pub drop: Drop,
    pub ok: Result<(), &'static str>,
    pub stars: u8,
    pub delta: Cents,
    pub clean: bool,
}

/// A card opened by a click (Optibus's tooltips): a trip, a bus, a duty, an event, a zone.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Card {
    Trip { tour: usize, trip: usize, at: Vec2, assign: bool },
    Vehicle { tour: usize, at: Vec2 },
    Event { row: usize, ev: usize, at: Vec2 },
    Stack { k: usize, at: Vec2 },
    Selection { at: Vec2 },
}

/// The right-click menu: where, and on what.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Menu {
    pub at: Vec2,
    pub tour: usize,
    pub trip: usize,
    /// In the Drivers view: the duty row and the piece (block, duty).
    pub duty_row: Option<usize>,
    pub piece: Option<(usize, usize)>,
    /// Custom work under the click: (duty row, its index).
    pub custom: Option<(usize, usize)>,
}

/// What lies open over the page.
#[derive(Clone, Debug, PartialEq, Default)]
pub(super) enum Panel {
    #[default]
    None,
    Issues(Scope),
    Duty(RowId),
    History,
    Stats,
    Layers,
    Filter,
    Zoom,
    Print,
    Day,
    Colour(String),
    GoTo(String),
    Optimise,
    Histogram(u8),
}

/// The Gantt's filter (Optibus's Filter window).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Filter {
    /// Lines left out (their names).
    pub hidden: Vec<String>,
    /// 0 both ways, 1 outbound, 2 inbound.
    pub dir: u8,
    pub from: i32,
    pub to: i32,
    pub locked_only: bool,
    /// Only the rows of these issues (their keys): "Filter Issues".
    pub issues: Option<Vec<String>>,
}

impl Default for Filter {
    fn default() -> Self {
        Filter { hidden: Vec::new(), dir: 0, from: 0, to: 1440 * 2, locked_only: false, issues: None }
    }
}

impl Filter {
    pub fn active(&self) -> bool {
        !self.hidden.is_empty() || self.dir != 0 || self.from > 0 || self.to < 1440 * 2 || self.locked_only
    }
}

/// A column of figures beside the IDs (Optibus's Stats).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stat {
    Start,
    End,
    Spread,
    Paid,
    Driving,
    Km,
    Trips,
    Type,
}

impl Stat {
    pub const ALL: [Stat; 8] = [Stat::Start, Stat::End, Stat::Spread, Stat::Paid, Stat::Driving, Stat::Km, Stat::Trips, Stat::Type];
    pub fn label(self) -> &'static str {
        match self {
            Stat::Start => "Start time",
            Stat::End => "End time",
            Stat::Spread => "Spread",
            Stat::Paid => "Paid time",
            Stat::Driving => "Driving time",
            Stat::Km => "Distance",
            Stat::Trips => "Trips",
            Stat::Type => "Duty type",
        }
    }
}

/// What the trips' chips say (Optibus's Layers).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub(super) struct Layers {
    /// 0 the line, 1 the start and end time, 2 the start and end stop.
    pub label: u8,
    /// The minutes of each layover printed in its gap.
    pub breaks: bool,
    /// A dot on the trips run off their timetable tour (Optibus's custom-trip dot).
    pub moved: bool,
}

/// RosterFlow's state on the planning page.
#[derive(Default)]
pub(in super::super) struct Rf {
    /// The day shown: its date, the company's generation, the board, the company as the studio
    /// sees it (the draft in), and the timetable read.
    pub(super) cache: Option<Cached>,
    /// "Optimise"'s proposal for the date and generation: the board and what to keep.
    pub(super) proposal: Option<(String, u64, Rc<Board>, Vec<co::studio::LineLayout>, Vec<co::studio::JoinedDuty>)>,
    /// The figures before the last edit, of a date (the KPIs' differences).
    pub(super) before: Option<(String, Figures)>,
    /// Undo and redo: the draft before each edit, and the edit in words.
    pub(super) undo: Vec<(Draft, String)>,
    pub(super) redo: Vec<(Draft, String)>,
    pub(super) zoom: f32,
    pub(super) pan: i32,
    pub(super) scroll: f32,
    /// The trips chosen (block, trip) and the row they are in.
    pub(super) sel: Vec<(usize, usize)>,
    pub(super) sel_row: Option<RowId>,
    /// Rows ticked (the batch toolbar), rows pinned to the top.
    pub(super) ticked: Vec<RowId>,
    pub(super) pinned: Vec<RowId>,
    pub(super) hand: Option<Hand>,
    pub(super) zones: Option<Vec<Zone>>,
    /// Pressed on a chip (where, and what): taken up once the mouse moved.
    pub(super) press: Option<(Vec2, Hand)>,
    pub(super) dragging: bool,
    pub(super) card: Option<Card>,
    pub(super) menu: Option<Menu>,
    pub(super) panel: Panel,
    /// The validation panel: the dismissed tab, the issues ticked (their keys), the groups open.
    pub(super) dismissed_tab: bool,
    pub(super) issue_ticks: Vec<String>,
    pub(super) groups_open: Vec<&'static str>,
    pub(super) filter: Filter,
    pub(super) filter_edit: Option<Filter>,
    pub(super) stats: Vec<Stat>,
    pub(super) sort: Option<(Stat, bool)>,
    pub(super) layers: Layers,
    pub(super) fullscreen: bool,
    pub(super) kpis_open: bool,
    /// The last click on a chip (its key) and when: a second one soon after picks it up.
    pub(super) last_click: Option<(String, f32)>,
    /// The opening: when it began; whether the page was shown since it was last left.
    pub(super) intro: Option<f32>,
    pub(super) shown: bool,
    /// The Preferences window, open.
    pub(super) prefs: Option<rf_prefs::PrefsEdit>,
    /// A line to show in the Timeplan tab.
    pub(super) timeplan_line: Option<String>,
    /// A row to scroll into view once (G, a link from a card).
    pub(super) reveal: Option<RowId>,
    /// The drop zone under the mouse (its tooltip).
    pub(super) zone_hover: Option<usize>,
    /// Where the cards, menus and windows lay the last frame (the Gantt under them gets no
    /// mouse), and when one was opened last (its own click does not close it).
    pub(super) over: Vec<Rect>,
    pub(super) opened_at: f32,
    /// The custom work to add in the duty panel: its kind, from, to.
    pub(super) custom_draft: (usize, i32, i32),
    /// How far the duty panel is scrolled.
    pub(super) duty_scroll: f32,
    /// Edits made so far (undone ones too), for the lessons (`rf_tutorial`).
    pub(super) edits: u64,
    /// The tutorial: its "?" menu, its help sheet, a lesson on its way (`rf_tutorial`).
    pub(super) tut: super::rf_tutorial::Tut,
}

/// The day as shown, made once for a date and the company's generation.
#[derive(Clone)]
pub(super) struct Cached {
    pub date: String,
    pub generation: u64,
    pub board: Rc<Board>,
    pub view: Rc<Company>,
    pub lines: Rc<Vec<LineInfo>>,
}

impl Rf {
    /// Something is in hand that Escape lets go of first.
    pub(in super::super) fn holds(&self) -> bool {
        self.hand.is_some() || self.card.is_some() || self.menu.is_some() || self.panel != Panel::None || !self.sel.is_empty() || self.proposal.is_some() || self.prefs.is_some() || self.intro.is_some()
    }

    pub(in super::super) fn let_go(&mut self) {
        if self.prefs.is_some() {
            self.prefs = None;
            return;
        }
        if self.menu.is_some() || self.card.is_some() || self.panel != Panel::None {
            self.menu = None;
            self.card = None;
            self.panel = Panel::None;
            return;
        }
        self.hand = None;
        self.zones = None;
        self.press = None;
        self.dragging = false;
        self.sel.clear();
        self.sel_row = None;
        self.proposal = None;
    }

    /// The hand let go of, the cards closed (after an edit: the day's indices change).
    pub(super) fn settle(&mut self) {
        self.hand = None;
        self.zones = None;
        self.press = None;
        self.dragging = false;
        self.card = None;
        self.menu = None;
        self.sel.clear();
        self.sel_row = None;
        self.zone_hover = None;
    }
}

// --- edits --------------------------------------------------------------------------------------

/// An edit of the draft (`f` on the company as the studio sees it): what was before kept for
/// Undo and for the figures' differences; a refusal said with its reason.
pub(super) fn edit(l: &mut Launcher, label: String, f: impl FnOnce(&mut Company) -> Result<(), &'static str>) -> bool {
    let Some(c) = l.company.company.as_ref() else { return false };
    let mut v = rf::view(c);
    let snapshot = rf::draft_of(c);
    let before = l.company.planning.rf.cache.as_ref().map(|k| (k.date.clone(), k.board.figures.clone()));
    match f(&mut v) {
        Ok(()) => {
            act(l, |c| {
                rf::keep_draft(c, &v);
                Ok(())
            });
            let s = &mut l.company.planning.rf;
            s.edits += 1;
            s.undo.push((snapshot, label));
            if s.undo.len() > 60 {
                s.undo.remove(0);
            }
            s.redo.clear();
            s.before = before;
            s.settle();
            true
        }
        Err(why) => {
            l.state.set_status(omsi_ui::tr(why).into_owned(), true);
            false
        }
    }
}

/// Undo the last edit (`back`), or redo the last undone.
pub(super) fn undo(l: &mut Launcher, back: bool) {
    let Some(c) = l.company.company.as_ref() else { return };
    let now = rf::draft_of(c);
    let s = &mut l.company.planning.rf;
    let item = if back { s.undo.pop() } else { s.redo.pop() };
    let Some((d, label)) = item else { return };
    if back {
        s.redo.push((now, label.clone()));
    } else {
        s.undo.push((now, label.clone()));
    }
    s.settle();
    act(l, |c| {
        rf::set_draft(c, d);
        Ok(())
    });
    let words = if back { omsi_ui::tr("Undone: %{edit}") } else { omsi_ui::tr("Redone: %{edit}") };
    l.state.set_status(words.replace("%{edit}", &label), false);
}

/// Go back to before edit `k` of the history (the later ones can be redone).
pub(super) fn back_to(l: &mut Launcher, k: usize) {
    let n = l.company.planning.rf.undo.len();
    for _ in k..n {
        undo(l, true);
    }
}

/// SAVE: the draft becomes what the company runs.
pub(super) fn save(l: &mut Launcher) {
    if act(l, |c| Ok(rf::save_draft(c))) == Some(true) {
        l.state.set_status(omsi_ui::tr("Saved: the roster, the day and the game run this planning now.").into_owned(), false);
    }
}

/// DISCARD: back to what was saved.
pub(super) fn discard(l: &mut Launcher) {
    if act(l, |c| Ok(rf::discard_draft(c))) == Some(true) {
        let s = &mut l.company.planning.rf;
        s.undo.clear();
        s.redo.clear();
        s.settle();
        l.state.set_status(omsi_ui::tr("Discarded: back to the saved planning.").into_owned(), false);
    }
}

// --- the page -------------------------------------------------------------------------------------

/// The views of the title bar's tabs (the planning's `view`).
pub(super) const ROSTER: usize = 0;
pub(super) const VEHICLES: usize = 1;
pub(super) const DRIVERS: usize = 2;
pub(super) const TIMEPLAN: usize = 3;

/// The title bar's height.
const TITLE_H: f32 = 48.0;
const TOOL_H: f32 = 40.0;
const STATUS_H: f32 = 32.0;

/// RosterFlow on the planning page: its title bar, then the tab's page. Returns the room left
/// for the Roster tab (its own page, drawn by the planning), None when RosterFlow drew it all.
pub(super) fn frame(l: &mut Launcher, area: Rect, c: &Company, date: &str) -> Option<Rect> {
    let p = pal(&l.ui);
    // the opening, when the page opens (again, after a while elsewhere)
    let now = l.ui.time;
    {
        let s = &mut l.company.planning.rf;
        if !s.shown && l.ui.motion {
            s.intro = Some(now);
        }
        s.shown = true;
        if s.zoom < 1.0 {
            s.zoom = 1.0;
        }
    }
    let intro_t = l.company.planning.rf.intro.map(|t| now - t);
    let saved_input = intro_t.filter(|t| rf_brand::playing(*t)).map(|_| {
        let i = l.ui.input.clone();
        let skip = i.pressed || i.keys.contains(&Key::Escape) || i.right_pressed;
        super::super::mask(&mut l.ui);
        (i, skip)
    });
    let view = l.company.planning.view.min(3);
    l.ui.as_drawn();
    if view != ROSTER {
        l.ui.p().rect(area, p.bg);
    }
    let bar = Rect::new(area.x, area.y, area.w, TITLE_H);
    title_bar(l, bar, c, date, &p);
    let body = Rect::new(area.x, bar.bottom(), area.w, (area.bottom() - bar.bottom()).max(0.0));
    let mut out = None;
    match l.company.planning.view.min(3) {
        ROSTER => out = Some(body),
        TIMEPLAN => {
            // ("View in timeplan" from a trip: that line)
            if let Some(line) = l.company.planning.rf.timeplan_line.take() {
                l.company.planning.timeplan.show(&c.id, &line);
            }
            // (the timeplan is drawn in the dark palette and takes the light look the usual way,
            // not RosterFlow's own colours)
            l.ui.end_as_drawn();
            super::timeplan::draw(l, body);
            l.ui.as_drawn();
        }
        _ => studio_page(l, body, c, date, &p, view == DRIVERS),
    }
    l.ui.end_as_drawn();
    // the opening over it all
    if let Some((input, skip)) = saved_input {
        l.ui.input = input;
        let t = intro_t.unwrap_or(rf_brand::INTRO);
        if skip {
            l.company.planning.rf.intro = None;
            l.ui.input.pressed = false;
            l.ui.input.keys.retain(|k| *k != Key::Escape);
        } else {
            rf_brand::opening(&mut l.ui, area, t, p.bg, p.text, p.dim, p.accent);
        }
    } else if intro_t.is_some() {
        l.company.planning.rf.intro = None;
    }
    out
}

/// The title bar: the mark and the name, the menu's icons, the schedule's name, undo, redo
/// and the history, the day, and the tabs.
fn title_bar(l: &mut Launcher, r: Rect, c: &Company, date: &str, p: &Pal) {
    let ui = &mut l.ui;
    ui.p().rect(r, p.bar);
    ui.p().rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), p.line);
    let cy = r.center().y;
    rf_brand::mark(ui, Rect::new(r.x + 10.0, cy - 15.0, 30.0, 30.0), 1.0, 1.0);
    let nw = rf_brand::wordmark(ui, Vec2::new(r.x + 48.0, cy - 6.0), 17.0, p.text, p.accent);
    ui.text_in(rf_brand::SUBTITLE, Rect::new(r.x + 49.0, cy + 3.0, nw.max(190.0) + 40.0, 16.0), 10.5, Weight::Medium, p.dim, Align::Left);
    let mut x = r.x + 48.0 + nw.max(196.0) + 26.0;
    ui.p().rect(Rect::new(x - 12.0, r.y + 10.0, 1.0, r.h - 20.0), p.line);
    let studio = l.company.planning.view == VEHICLES || l.company.planning.view == DRIVERS;
    // the menu's icons: preferences, optimisation settings, print, go to
    let icons: [(&str, &str, &str); 4] = [("rf-prefs", "tune", "Preferences: the rules the duties are checked against and Optimise works to"), ("rf-print", "description", "Print the bus graph or the duty graph as a PDF file"), ("rf-goto", "search", "Go to a vehicle or a duty by its ID (G)"), ("rf-optset", "settings", "Optimisation settings")];
    let mut clicked: Option<&str> = None;
    let icons_x = x;
    for (name, icon, tip) in icons {
        if rf_icon(&mut l.ui, p, name, Vec2::new(x + 15.0, cy), icon, tip, false, true) {
            clicked = Some(name);
        }
        x += 36.0;
    }
    // the "?": the tutorial's menu (its tour, its lessons, its help sheet: `rf_tutorial`)
    let help = Rect::new(x, cy - 15.0, 30.0, 30.0);
    super::super::super::tour::anchor("rf-help", help);
    l.company.planning.rf.tut.help_at = help;
    if rf_icon(&mut l.ui, p, "rf-help", Vec2::new(x + 15.0, cy), "help", "Tutorial: a guided tour, lessons to try yourself and a help sheet", l.company.planning.rf.tut.menu_open(), true) {
        clicked = Some("rf-help");
    }
    x += 36.0;
    let ui = &mut l.ui;
    // the schedule's name, orange and slanted as Optibus's, the dot green when saved
    let unsaved = rf::has_draft(c);
    let name = format!("{}  ·  {}", c.name, day_label(date));
    // (between the menu's icons and undo: the tabs, the day and undo, redo and the history are
    // on the right; a name too long for the room is cut)
    let tabs_w: f32 = ["Vehicles", "Drivers", "Roster", "Timeplan"].iter().map(|t| 48.0 + ui.width(&omsi_ui::tr(t), 13.0, Weight::Bold)).sum();
    let room_end = r.right() - tabs_w - 10.0 - 176.0 - 22.0 - 100.0;
    let room = (room_end - x - 34.0).max(40.0);
    let nw2 = ui.width(&name, 15.0, Weight::Bold).min(room);
    let mid = ((x + room_end) * 0.5).clamp(x + nw2 * 0.5 + 10.0, (room_end - nw2 * 0.5 - 24.0).max(x + nw2 * 0.5 + 10.0));
    ui.push_clip(Rect::new(mid - nw2 * 0.5, r.y, nw2, r.h), 0.0);
    ui.text_as_is(&name, Vec2::new(mid - nw2 * 0.5, cy + 5.0), 15.0, Weight::Bold, p.warn, Align::Left);
    ui.pop_clip();
    let dot = Vec2::new(mid + nw2 * 0.5 + 12.0, cy);
    super::super::super::tour::anchor("rf-menu", Rect::new(icons_x, r.y + 6.0, dot.x + 10.0 - icons_x, r.h - 12.0));
    ui.p().circle(dot, 5.0, if unsaved { p.warn } else { p.ok });
    ui.tooltip(Rect::new(dot.x - 8.0, dot.y - 8.0, 16.0, 16.0), if unsaved { "Changes not saved yet: SAVE makes them the company's planning, DISCARD forgets them" } else { "Saved: the company runs this planning" });
    // the tabs, right: Vehicles, Drivers, Roster, Timeplan
    let tabs: [(usize, &str, &str); 4] = [(VEHICLES, "directions_bus", "Vehicles"), (DRIVERS, "person", "Drivers"), (ROSTER, "group", "Roster"), (TIMEPLAN, "departure_board", "Timeplan")];
    let mut tx = r.right();
    let mut pick_tab = None;
    for (v, icon, label) in tabs.iter().rev() {
        let w = 34.0 + ui.width(&omsi_ui::tr(label), 13.0, Weight::Bold) + 14.0;
        tx -= w;
        let tr = Rect::new(tx, r.y, w, r.h);
        let anchor = match *v {
            VEHICLES => "rf-tab-vehicles",
            DRIVERS => "rf-tab-drivers",
            ROSTER => "rf-tab-roster",
            _ => "rf-tab-timeplan",
        };
        super::super::super::tour::anchor(anchor, tr);
        let on = l.company.planning.view == *v;
        let (h, _, click) = ui.interact(id_of(&format!("rf-tab-{v}")), tr);
        if on {
            ui.p().rect(tr, p.bg);
            ui.p().rect(Rect::new(tr.x, tr.bottom() - 3.0, tr.w, 3.0), p.accent);
        } else if h {
            ui.p().rect(tr, p.text.alpha(0.05));
        }
        let col = if on { p.accent } else if h { p.text } else { p.dim };
        ui.icon(icon, Vec2::new(tr.x + 18.0, cy), 18.0, col);
        ui.text_in(label, Rect::new(tr.x + 32.0, tr.y, w - 36.0, tr.h), 13.0, Weight::Bold, col, Align::Left);
        if click && !on {
            pick_tab = Some(*v);
        }
    }
    // the day: the next seven days (Optibus's service-day switcher)
    let dw = 176.0;
    let dr = Rect::new(tx - dw - 10.0, r.y + 8.0, dw, r.h - 16.0);
    super::super::super::tour::anchor("rf-day", dr);
    let (dh, _, dclick) = ui.interact(id_of("rf-day"), dr);
    ui.p().rect(Rect::new(dr.x - 1.0, r.y + 8.0, 1.0, r.h - 16.0), p.line);
    ui.icon("calendar_month", Vec2::new(dr.x + 18.0, cy), 18.0, if dh { p.text } else { p.dim });
    let wd = co::plan::weekday_of(date);
    let day_words = format!("{} {}", omsi_ui::tr(DAY_NAMES[wd as usize % 7]).to_uppercase(), day_label(date).split_whitespace().skip(1).take(2).collect::<Vec<_>>().join(" ").to_uppercase());
    ui.text_as_is(&day_words, Vec2::new(dr.x + 36.0, cy + 5.0), 13.0, Weight::Bold, p.text, Align::Left);
    ui.icon("expand_more", Vec2::new(dr.right() - 14.0, cy), 18.0, p.dim);
    ui.tooltip(dr, "The day shown: the planning of a weekday holds for that weekday every week");
    // undo, redo, the history
    let hx = dr.x - 22.0;
    super::super::super::tour::anchor("rf-undo", Rect::new(hx - 92.0, cy - 16.0, 104.0, 32.0));
    let can_redo = !l.company.planning.rf.redo.is_empty();
    let can_undo = !l.company.planning.rf.undo.is_empty();
    let hist = rf_icon(&mut l.ui, p, "rf-history", Vec2::new(hx - 4.0, cy), "history", "Schedule history: every change, to go back to", l.company.planning.rf.panel == Panel::History, can_undo);
    let redo = rf_icon(&mut l.ui, p, "rf-redo", Vec2::new(hx - 42.0, cy), "livery_redo", "Redo (Ctrl+Y)", false, can_redo && studio);
    let undo_b = rf_icon(&mut l.ui, p, "rf-undo", Vec2::new(hx - 76.0, cy), "livery_undo", "Undo (Ctrl+Z)", false, can_undo && studio);
    let s = &mut l.company.planning.rf;
    if dclick {
        s.panel = if s.panel == Panel::Day { Panel::None } else { Panel::Day };
    }
    if hist {
        s.panel = if s.panel == Panel::History { Panel::None } else { Panel::History };
    }
    if let Some(v) = pick_tab {
        l.company.planning.view = v;
        l.company.planning.sel = None;
        l.company.planning.arm = None;
        l.company.planning.rf.settle();
        l.company.planning.rf.panel = Panel::None;
    }
    match clicked {
        Some("rf-prefs") => l.company.planning.rf.prefs = Some(rf_prefs::PrefsEdit::new(&c.planning.flow.prefs)),
        Some("rf-print") => toggle(&mut l.company.planning.rf.panel, Panel::Print),
        Some("rf-goto") => toggle(&mut l.company.planning.rf.panel, Panel::GoTo(String::new())),
        Some("rf-optset") => toggle(&mut l.company.planning.rf.panel, Panel::Optimise),
        Some("rf-help") => l.company.planning.rf.tut.toggle_menu(),
        _ => {}
    }
    if undo_b {
        undo(l, true);
    }
    if redo {
        undo(l, false);
    }
}

pub(super) const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub(super) fn toggle(panel: &mut Panel, p: Panel) {
    *panel = if std::mem::discriminant(panel) == std::mem::discriminant(&p) { Panel::None } else { p };
}

/// The day's board, made again when the date or the company changed.
pub(super) fn ensure(l: &mut Launcher, c: &Company, date: &str) -> Result<Cached, String> {
    let generation = l.company.generation;
    if let Some(k) = l.company.planning.rf.cache.as_ref().filter(|k| k.date == date && k.generation == generation) {
        return Ok(k.clone());
    }
    let Some(lines) = l.company.planning.days.iter().find(|d| d.0 == c.map && d.1 == date).map(|d| d.2.clone()) else { return Err(omsi_ui::tr("Reading the timetable…").into_owned()) };
    let lines = lines?;
    // (a layout the timetable no longer fits: back to automatic for its line, said)
    if !co::studio::stale(c, &lines, date).is_empty() {
        let d = date.to_string();
        if let Some(gone) = act(l, |c| Ok(co::studio::retire_stale(c, &lines, &d))) {
            if !gone.is_empty() {
                l.state.set_status(omsi_ui::tr("The timetable changed: line %{n} runs with the automatic blocks and duties again.").replace("%{n}", &gone.join(", ")), true);
            }
        }
    }
    let c = l.company.company.clone().unwrap_or_else(|| c.clone());
    let view = rf::view(&c);
    let board = rf::board(&view, &lines, date);
    let k = Cached { date: date.to_string(), generation: l.company.generation, board: Rc::new(board), view: Rc::new(view), lines: Rc::new(lines) };
    let s = &mut l.company.planning.rf;
    if s.cache.as_ref().is_none_or(|old| old.date != date) {
        s.settle();
        s.undo.clear();
        s.redo.clear();
        s.before = None;
    }
    if s.proposal.as_ref().is_some_and(|p| p.0 != date || p.1 != k.generation) {
        s.proposal = None;
    }
    s.cache = Some(k.clone());
    Ok(k)
}

/// The Vehicles or Drivers view: the KPIs, the toolbar, the Gantt and the status bar.
fn studio_page(l: &mut Launcher, r: Rect, c: &Company, date: &str, p: &Pal, drivers: bool) {
    let k = match ensure(l, c, date) {
        Ok(k) => k,
        Err(e) => {
            l.ui.text_in(&e, Rect::new(r.x + 24.0, r.y + 24.0, r.w - 48.0, 24.0), 14.0, Weight::Medium, p.dim, Align::Left);
            return;
        }
    };
    let proposal = l.company.planning.rf.proposal.as_ref().map(|x| x.2.clone());
    let board = proposal.clone().unwrap_or_else(|| k.board.clone());
    let fullscreen = l.company.planning.rf.fullscreen;
    if l.company.planning.rf.custom_draft.2 == 0 {
        l.company.planning.rf.custom_draft = (0, 810, 900);
    }
    // (what lay over the Gantt the last frame keeps the mouse from what lies under it)
    let m = l.ui.input.mouse;
    let covered = l.company.planning.rf.over.iter().any(|o| o.contains(m));
    let saved = covered.then(|| {
        let i = l.ui.input.clone();
        l.ui.input.mouse = Vec2::new(-1e4, -1e4);
        l.ui.input.pressed = false;
        l.ui.input.released = false;
        l.ui.input.right_pressed = false;
        l.ui.input.wheel = Vec2::ZERO;
        i
    });
    let mut y = r.y;
    if !fullscreen {
        let open = l.company.planning.rf.kpis_open;
        let h = if open { 4.0 * 92.0 + 8.0 } else { 100.0 };
        let against = if proposal.is_some() { Some(k.board.figures.clone()) } else { l.company.planning.rf.before.as_ref().filter(|b| b.0 == date).map(|b| b.1.clone()) };
        kpis(l, Rect::new(r.x, y, r.w, h), &board.figures, against.as_ref(), p);
        y += h;
    }
    let tool = Rect::new(r.x, y, r.w, TOOL_H);
    toolbar(l, tool, &k, p, drivers, proposal.is_some());
    y = tool.bottom();
    let status = Rect::new(r.x, r.bottom() - STATUS_H, r.w, STATUS_H);
    let gantt = Rect::new(r.x, y, r.w, (status.y - y).max(0.0));
    super::super::super::tour::anchor("rf-gantt", gantt);
    if proposal.is_some() {
        let banner = Rect::new(gantt.x, gantt.y, gantt.w, 40.0);
        proposal_banner(l, banner, p, date);
        let g = Rect::new(gantt.x, banner.bottom(), gantt.w, (gantt.h - 40.0).max(0.0));
        if l.company.planning.rf.proposal.is_some() {
            rf_gantt::gantt(l, g, &k, &board, p, drivers, true);
        }
    } else {
        rf_gantt::gantt(l, gantt, &k, &board, p, drivers, false);
    }
    status_bar(l, status, &k, &board, p, drivers);
    if let Some(i) = saved {
        l.ui.input = i;
    }
    if l.company.planning.rf.prefs.is_some() {
        l.company.planning.rf.over.clear();
        rf_prefs::window(l, r, p);
        return;
    }
    rf_panels::overlays(l, r, gantt, status, &k, &board, p, drivers, date);
    keys(l, &k, &board, drivers);
}

/// "Optimise"'s proposal over the Gantt: accept it, or keep the day as it is.
fn proposal_banner(l: &mut Launcher, r: Rect, p: &Pal, date: &str) {
    let ui = &mut l.ui;
    super::super::super::tour::anchor("rf-proposal", r);
    ui.p().rect(r, p.accent.alpha(0.16));
    ui.p().rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), p.accent.alpha(0.5));
    ui.icon("bolt", Vec2::new(r.x + 22.0, r.center().y), 18.0, p.accent);
    ui.text_in("Optimised proposal: the KPIs show it against your planning. Locked vehicles and duties are kept.", Rect::new(r.x + 42.0, r.y, r.w - 360.0, r.h), 13.0, Weight::Medium, p.text, Align::Left);
    let keep = rf_button(ui, p, "rf-keep", Rect::new(r.right() - 150.0, r.y + 6.0, 140.0, r.h - 12.0), "Keep mine", false, true);
    let accept = rf_button(ui, p, "rf-accept", Rect::new(r.right() - 300.0, r.y + 6.0, 140.0, r.h - 12.0), "Accept", true, true);
    if keep {
        l.company.planning.rf.proposal = None;
    }
    if accept {
        if let Some((_, _, _, lays, joined)) = l.company.planning.rf.proposal.take() {
            let wd = co::plan::weekday_of(date);
            if edit(l, omsi_ui::tr("Optimised").into_owned(), |c| {
                rf::apply(c, wd, lays, joined);
                Ok(())
            }) {
                l.state.set_status(omsi_ui::tr("The optimised blocks and duties are in your draft: SAVE keeps them.").into_owned(), false);
            }
        }
    }
}

/// Run Optimise: its proposal is shown, to accept or not.
pub(super) fn optimise(l: &mut Launcher, k: &Cached) {
    let (lays, joined) = rf::optimise(&k.view, &k.lines, &k.date);
    let mut probe = (*k.view).clone();
    let wd = co::plan::weekday_of(&k.date);
    rf::apply(&mut probe, wd, lays.clone(), joined.clone());
    let board = rf::board(&probe, &k.lines, &k.date);
    let s = &mut l.company.planning.rf;
    s.settle();
    s.proposal = Some((k.date.clone(), k.generation, Rc::new(board), lays, joined));
}

// --- the KPI dashboard ---------------------------------------------------------------------------

/// A figure's tile: an icon, the number in blue with its difference, the caption in capitals
/// and its formula under it.
#[allow(clippy::too_many_arguments)]
fn tile(ui: &mut Ui, r: Rect, p: &Pal, icon: &str, value: &str, delta: Option<(String, bool)>, caption: &str, formula: &str) {
    let vw = ui.width(value, 23.0, Weight::Regular);
    let dw = delta.as_ref().map(|d| ui.width(&d.0, 17.0, Weight::Bold) + 8.0).unwrap_or(0.0);
    let total = 30.0 + vw + dw;
    let x0 = r.center().x - total * 0.5;
    let top = r.y + 30.0;
    ui.icon(icon, Vec2::new(x0 + 12.0, top), 24.0, p.dim);
    ui.text_as_is(value, Vec2::new(x0 + 30.0, top + 8.5), 23.0, Weight::Regular, p.accent, Align::Left);
    if let Some((d, good)) = delta {
        ui.text_as_is(&d, Vec2::new(x0 + 30.0 + vw + 8.0, top + 7.0), 16.0, Weight::Bold, if good { p.ok } else { p.err }, Align::Left);
    }
    caps(ui, Rect::new(r.x + 6.0, r.y + 48.0, r.w - 12.0, 16.0), caption, p.text, Align::Center);
    if !formula.is_empty() {
        ui.text_in(formula, Rect::new(r.x + 6.0, r.y + 64.0, r.w - 12.0, 14.0), 10.5, Weight::Regular, p.dim, Align::Center);
    }
}

/// A difference in words and whether it is good (lower is better unless `up`).
fn diff(now: f64, was: Option<f64>, up: bool, fmt: &dyn Fn(f64) -> String) -> Option<(String, bool)> {
    let w = was?;
    let d = now - w;
    if d.abs() < 1e-6 {
        return None;
    }
    let sign = if d > 0.0 { "+" } else { "-" };
    Some((format!("{sign}{}", fmt(d.abs())), (d > 0.0) == up))
}

/// The KPI dashboard: five tiles, opened (the small tab under them) to four rows with the
/// histograms.
fn kpis(l: &mut Launcher, r: Rect, f: &Figures, was: Option<&Figures>, p: &Pal) {
    let ui = &mut l.ui;
    ui.p().rect(r, p.bg);
    let row_h = 92.0;
    let w = r.w / 5.0;
    let pct = |x: f64| format!("{:.0}%", x * 100.0);
    let money = |x: f64| eur(x.round() as Cents);
    let hours = |m: f64| format!("{:.0}", m / 60.0);
    let hm = |m: f64| rf::hm(m.round() as i32);
    let n = |x: f64| format!("{x:.0}");
    let km_word = omsi_ui::tr("km");
    let cells: Vec<(&str, String, Option<(String, bool)>, &str, String)> = vec![
        ("person", f.duties.to_string(), diff(f.duties as f64, was.map(|w| w.duties as f64), false, &n), "Duties", String::new()),
        ("directions_bus", f.blocks.to_string(), diff(f.blocks as f64, was.map(|w| w.blocks as f64), false, &n), "Blocks (PVR: %{n})", String::new()),
        ("route", pct(f.vehicle_eff()), diff(f.vehicle_eff(), was.map(|w| w.vehicle_eff()), true, &|d| format!("{:.0}%", d * 100.0)), "Vehicle efficiency", format!("({} / {} + {}): {:.0} {km_word} / ({:.0} + {:.0}) {km_word}", omsi_ui::tr("Services"), omsi_ui::tr("Deadheads"), omsi_ui::tr("Services"), f.service_km, f.dead_km, f.service_km)),
        ("schedule", pct(f.crew_eff()), diff(f.crew_eff(), was.map(|w| w.crew_eff()), true, &|d| format!("{:.0}%", d * 100.0)), "Crew efficiency", format!("({} / {}): {} / {}", omsi_ui::tr("Driving time"), omsi_ui::tr("Paid time"), hours(f.driving as f64), hours(f.paid as f64))),
        ("payments", money(f.total() as f64), diff(f.total() as f64, was.map(|w| w.total() as f64), false, &money), "Total cost", format!("({} + {}): {} + {}", omsi_ui::tr("Crew"), omsi_ui::tr("Vehicles"), eur(f.crew_cost), eur(f.vehicle_cost))),
    ];
    for (i, (icon, value, delta, caption, formula)) in cells.into_iter().enumerate() {
        let cell = Rect::new(r.x + i as f32 * w, r.y + 4.0, w, row_h);
        let cap = omsi_ui::tr(caption).replace("%{n}", &f.pvr.to_string());
        tile(ui, cell, p, icon, &value, delta, &cap, &formula);
        if i > 0 {
            ui.p().rect(Rect::new(cell.x, cell.y + 18.0, 1.0, row_h - 36.0), p.line);
        }
    }
    super::super::super::tour::anchor("rf-kpi-counts", Rect::new(r.x, r.y + 4.0, 2.0 * w, row_h));
    super::super::super::tour::anchor("rf-kpi-rest", Rect::new(r.x + 2.0 * w, r.y + 4.0, 3.0 * w, row_h));
    let open = l.company.planning.rf.kpis_open;
    if open {
        let avg = |m: i32| if f.duties == 0 { 0.0 } else { m as f64 / f.duties as f64 };
        let rows: Vec<Vec<(&str, String, Option<(String, bool)>, &str, String)>> = vec![
            vec![
                ("sync_alt", f.splits.to_string(), diff(f.splits as f64, was.map(|w| w.splits as f64), false, &n), "Split count", format!("{}: {} h | {:.0}% {}", omsi_ui::tr("Time"), f.split_time / 60, if f.duties == 0 { 0.0 } else { f.splits as f64 * 100.0 / f.duties as f64 }, omsi_ui::tr("of duties"))),
                ("commute", f.changeovers.to_string(), diff(f.changeovers as f64, was.map(|w| w.changeovers as f64), false, &n), "Changeover", format!("{}: {}", omsi_ui::tr("During split"), f.split_changeovers)),
                ("history", hm(avg(f.paid)), diff(avg(f.paid), was.map(|w| if w.duties == 0 { 0.0 } else { w.paid as f64 / w.duties as f64 }), false, &hm), "Average paid time", String::new()),
                ("timer", hours(f.paid as f64), diff(f.paid as f64 / 60.0, was.map(|w| w.paid as f64 / 60.0), false, &n), "Paid time (hours)", String::new()),
                ("schedule", hours(f.spread as f64), diff(f.spread as f64 / 60.0, was.map(|w| w.spread as f64 / 60.0), false, &n), "Spread time (hours)", String::new()),
            ],
            vec![
                ("event", hours(f.work as f64), None, "Work time (hours)", format!("{}: {}", omsi_ui::tr("Average"), hm(avg(f.work)))),
                ("speed", hours(f.driving as f64), None, "Driving time (hours)", String::new()),
                ("directions_bus", hours(f.platform as f64), None, "Platform time (hours)", String::new()),
                ("warning", eur(f.penalties), diff(f.penalties as f64, was.map(|w| w.penalties as f64), false, &money), "Penalties", omsi_ui::tr("Soft rules broken").into_owned()),
                ("tune", eur(f.algorithmic()), diff(f.algorithmic() as f64, was.map(|w| w.algorithmic() as f64), false, &money), "Crew algorithmic cost", format!("({} + {}): {} + {}", omsi_ui::tr("Duty cost"), omsi_ui::tr("Penalties"), eur(f.crew_cost), eur(f.penalties))),
            ],
        ];
        for (k, row) in rows.into_iter().enumerate() {
            for (i, (icon, value, delta, caption, formula)) in row.into_iter().enumerate() {
                let cell = Rect::new(r.x + i as f32 * w, r.y + 4.0 + (k + 1) as f32 * row_h, w, row_h);
                tile(&mut l.ui, cell, p, icon, &value, delta, caption, &formula);
                if i > 0 {
                    l.ui.p().rect(Rect::new(cell.x, cell.y + 18.0, 1.0, row_h - 36.0), p.line);
                }
            }
        }
        // the histograms
        let hy = r.y + 4.0 + 3.0 * row_h;
        let types: Vec<(String, usize)> = f.types.iter().filter(|t| t.1 > 0 || t.0 != rf::OTHER).map(|t| (omsi_ui::tr(&t.0).into_owned(), t.1)).collect();
        let bands: Vec<(String, usize)> = rf::PAID_BANDS.iter().zip(f.paid_bands.iter()).map(|(b, n)| (b.to_string(), *n)).collect();
        let ui = &mut l.ui;
        if histogram(ui, Rect::new(r.x + w * 0.5, hy, w * 1.6, row_h), p, &types, "Duty types", "rf-hist-types") {
            l.company.planning.rf.panel = Panel::Histogram(0);
        }
        if histogram(&mut l.ui, Rect::new(r.x + w * 2.4, hy, w * 1.6, row_h), p, &bands, "Duties paid time", "rf-hist-paid") {
            l.company.planning.rf.panel = Panel::Histogram(1);
        }
    }
    // the small tab to open and close it
    let ui = &mut l.ui;
    let tab = Rect::new(r.center().x - 14.0, r.bottom() - 9.0, 28.0, 9.0);
    let (h, _, click) = ui.interact(id_of("rf-kpi-tab"), tab.inset(-5.0));
    ui.p().rounded(tab, 2.0, if h { p.tool.lighten(0.1) } else { p.tool });
    ui.icon(if open { "expand_less" } else { "expand_more" }, tab.center(), 12.0, p.text);
    ui.tooltip(tab, if open { "Fewer figures" } else { "More figures: splits, changeovers, paid, spread, work, driving and platform time, the histograms" });
    if click {
        l.company.planning.rf.kpis_open = !open;
    }
}

/// A small bar chart (a histogram tile); returns whether it was clicked (to open it large).
pub(super) fn histogram(ui: &mut Ui, r: Rect, p: &Pal, bars: &[(String, usize)], caption: &str, name: &str) -> bool {
    let (h, _, click) = ui.interact(id_of(name), r);
    let most = bars.iter().map(|b| b.1).max().unwrap_or(0).max(1);
    let n = bars.len().max(1) as f32;
    let bw = ((r.w - 20.0) / n).min(56.0);
    let x0 = r.center().x - bw * n * 0.5;
    let base = r.y + r.h - 34.0;
    for (k, (label, v)) in bars.iter().enumerate() {
        let bh = (*v as f32 / most as f32) * (r.h - 58.0);
        let b = Rect::new(x0 + k as f32 * bw + 5.0, base - bh, bw - 10.0, bh.max(1.0));
        ui.p().rect(b, if *v == 0 { p.faint } else { p.accent });
        ui.text_as_is(&v.to_string(), Vec2::new(b.center().x, b.y - 3.0), 10.0, Weight::Medium, p.text, Align::Center);
        ui.text_in(label, Rect::new(b.x - 4.0, base + 2.0, b.w + 8.0, 14.0), 9.5, Weight::Regular, p.dim, Align::Center);
    }
    caps(ui, Rect::new(r.x, base + 18.0, r.w, 14.0), caption, if h { p.accent } else { p.text }, Align::Center);
    click
}

// --- the toolbar -------------------------------------------------------------------------------

fn toolbar(l: &mut Launcher, r: Rect, k: &Cached, p: &Pal, drivers: bool, proposal: bool) {
    let ui = &mut l.ui;
    ui.p().rect(r, p.tool);
    let cy = r.center().y;
    let dirty = rf::has_draft(l.company.company.as_ref().unwrap_or(&k.view));
    let save = rf_button(ui, p, "rf-save", Rect::new(r.x + 4.0, r.y + 4.0, 116.0, r.h - 8.0), "Save", true, dirty && !proposal);
    ui.tooltip(Rect::new(r.x + 4.0, r.y + 4.0, 116.0, r.h - 8.0), "Make the changes the company's planning: the roster, the day and the game run it");
    let discard = rf_button(ui, p, "rf-discard", Rect::new(r.x + 126.0, r.y + 4.0, 116.0, r.h - 8.0), "Discard", false, dirty && !proposal);
    super::super::super::tour::anchor("rf-save", Rect::new(r.x + 4.0, r.y + 4.0, 116.0, r.h - 8.0));
    super::super::super::tour::anchor("rf-discard", Rect::new(r.x + 126.0, r.y + 4.0, 116.0, r.h - 8.0));
    super::super::super::tour::anchor("rf-save-discard", Rect::new(r.x + 4.0, r.y + 4.0, 238.0, r.h - 8.0));
    let mut x = r.x + 256.0;
    let s = &l.company.planning.rf;
    let (stats_on, layers_on, filter_on) = (s.panel == Panel::Stats || !s.stats.is_empty(), s.panel == Panel::Layers, s.panel == Panel::Filter || s.filter.active());
    let (stats, sr) = tool_link(&mut l.ui, p, "rf-stats", x, cy, "signal_cellular_alt", "Stats", stats_on);
    x = sr.right() + 6.0;
    l.ui.p().rect(Rect::new(x, cy - 8.0, 1.0, 16.0), p.text.alpha(0.3));
    let (layers, lr) = tool_link(&mut l.ui, p, "rf-layers", x + 8.0, cy, "view_list", "Layers", layers_on);
    x = lr.right() + 6.0;
    l.ui.p().rect(Rect::new(x, cy - 8.0, 1.0, 16.0), p.text.alpha(0.3));
    let (filter, fr) = tool_link(&mut l.ui, p, "rf-filter", x + 8.0, cy, "filter_list", "Filter", filter_on);
    super::super::super::tour::anchor("rf-tools", Rect::new(r.x + 252.0, r.y + 4.0, fr.right() - r.x - 248.0, r.h - 8.0));
    x = fr.right() + 12.0;
    // the filters on, as chips to take off
    let mut clear_issues = false;
    let mut clear_filter = false;
    if let Some(keys) = &l.company.planning.rf.filter.issues {
        let words = omsi_ui::tr("Issues (%{n})").replace("%{n}", &keys.len().to_string());
        if chip(&mut l.ui, p, "rf-chip-issues", &mut x, cy, &words) {
            clear_issues = true;
        }
    }
    if l.company.planning.rf.filter.active() {
        let words = omsi_ui::tr("Filter").into_owned();
        if chip(&mut l.ui, p, "rf-chip-filter", &mut x, cy, &words) {
            clear_filter = true;
        }
    }
    // the stack, Optimise and its settings, the full screen
    let ui = &mut l.ui;
    let full = rf_icon(ui, p, "rf-full", Vec2::new(r.right() - 20.0, cy), "fullscreen", "Full screen: the KPIs folded away", l.company.planning.rf.fullscreen, true);
    let gear = Rect::new(r.right() - 76.0, r.y + 4.0, 36.0, r.h - 8.0);
    let (gh, _, gclick) = ui.interact(id_of("rf-opt-gear"), gear);
    ui.p().rect(gear, if gh { p.accent.lighten(0.1) } else { p.accent.darken(0.12) });
    ui.icon("settings", gear.center(), 18.0, Color::WHITE);
    ui.tooltip(gear, "Optimisation settings");
    let opt_r = Rect::new(gear.x - 140.0, r.y + 4.0, 140.0, r.h - 8.0);
    super::super::super::tour::anchor("rf-optimise", Rect::new(opt_r.x, opt_r.y, gear.right() - opt_r.x, opt_r.h));
    let opt = rf_button(ui, p, "rf-optimise", opt_r, "Optimize", true, !proposal);
    ui.tooltip(opt_r, "Optimise the day by the preferences: the vehicles' blocks and the duties, the locked ones kept. Shown as a proposal to accept or not");
    let stack_n = k.board.stack.len();
    if stack_n > 0 && !drivers {
        let words = omsi_ui::tr("Stack: %{n} unassigned").replace("%{n}", &stack_n.to_string());
        let w = ui.width(&words, 13.0, Weight::Bold) + 20.0;
        let sr = Rect::new(opt_r.x - w - 14.0, r.y + 7.0, w, r.h - 14.0);
        ui.p().rounded(sr, 3.0, p.err.alpha(0.22));
        ui.text_as_is(&words, Vec2::new(sr.center().x, cy + 4.5), 13.0, Weight::Bold, p.text, Align::Center);
    }
    let s = &mut l.company.planning.rf;
    if stats {
        toggle(&mut s.panel, Panel::Stats);
    }
    if layers {
        toggle(&mut s.panel, Panel::Layers);
    }
    if filter {
        if s.panel == Panel::Filter {
            s.panel = Panel::None;
        } else {
            s.filter_edit = Some(s.filter.clone());
            s.panel = Panel::Filter;
        }
    }
    if clear_issues {
        s.filter.issues = None;
    }
    if clear_filter {
        let issues = s.filter.issues.take();
        s.filter = Filter { issues, ..Default::default() };
    }
    if full {
        s.fullscreen = !s.fullscreen;
    }
    if gclick {
        toggle(&mut s.panel, Panel::Optimise);
    }
    if save {
        super::rosterflow::save(l);
    }
    if discard {
        super::rosterflow::discard(l);
    }
    if opt {
        optimise(l, k);
    }
}

/// A filter's chip with its ×: returns whether the × was clicked.
fn chip(ui: &mut Ui, p: &Pal, name: &str, x: &mut f32, cy: f32, words: &str) -> bool {
    let w = ui.width(words, 12.5, Weight::Medium) + 34.0;
    let r = Rect::new(*x, cy - 12.0, w, 24.0);
    ui.p().rounded(r, 12.0, p.accent.alpha(0.25));
    ui.text_as_is(words, Vec2::new(r.x + 10.0, cy + 4.5), 12.5, Weight::Medium, p.text, Align::Left);
    let xr = Rect::new(r.right() - 22.0, r.y + 2.0, 20.0, 20.0);
    let (h, _, click) = ui.interact(id_of(name), xr);
    ui.icon("close", xr.center(), 14.0, if h { p.err } else { p.text });
    *x = r.right() + 8.0;
    click
}

// --- the status bar -------------------------------------------------------------------------------

fn status_bar(l: &mut Launcher, r: Rect, k: &Cached, board: &Board, p: &Pal, drivers: bool) {
    let ui = &mut l.ui;
    ui.p().rect(r, p.status);
    ui.p().rect(Rect::new(r.x, r.y, r.w, 1.0), p.line);
    let cy = r.center().y;
    let w = rf_brand::wordmark(ui, Vec2::new(r.x + 14.0, cy), 14.0, p.faint, p.faint);
    let mut x = r.x + 14.0 + w + 14.0;
    ui.p().rect(Rect::new(x - 7.0, r.y + 8.0, 1.0, r.h - 16.0), p.line);
    let shown = rf_gantt::shown_rows(l, k, board, drivers);
    let ui = &mut l.ui;
    let words = if drivers {
        omsi_ui::tr("Showing %{a}/%{b} duties").replace("%{a}", &shown.to_string()).replace("%{b}", &board.shaped.len().to_string())
    } else {
        omsi_ui::tr("Showing %{a}/%{b} vehicles (%{c} unscheduled)").replace("%{a}", &shown.to_string()).replace("%{b}", &board.tours.len().to_string()).replace("%{c}", &board.stack.len().to_string())
    };
    let ww = ui.text_as_is(&words, Vec2::new(x, cy + 4.5), 12.5, Weight::Medium, p.dim, Align::Left);
    x += ww + 18.0;
    let f = &board.figures;
    let all = f.assigned == f.trips;
    let in_duty = f.assigned;
    for (icon, a, tip) in [("directions_bus", f.assigned, "Trips with passengers assigned to a vehicle"), ("person", in_duty, "Trips with passengers in a duty")] {
        let col = if all { p.accent } else { p.err };
        ui.icon(icon, Vec2::new(x + 8.0, cy), 15.0, col);
        let t = format!("{a}/{}", f.trips);
        let tw = ui.text_as_is(&t, Vec2::new(x + 20.0, cy + 4.5), 12.5, Weight::Bold, col, Align::Left);
        ui.tooltip(Rect::new(x, r.y, tw + 22.0, r.h), tip);
        x += tw + 38.0;
    }
    super::super::super::tour::anchor("rf-status-counts", Rect::new(r.x + 8.0, r.y + 2.0, x - r.x - 24.0, r.h - 4.0));
    // the issues: Global, Duty, Vehicle, and re-validate
    let c = &k.view;
    let mut rx = r.right() - 14.0;
    // the zoom
    let zoom = l.company.planning.rf.zoom;
    let zt = format!("{:.0}%", zoom * 100.0);
    let zw = 64.0;
    let zr = Rect::new(rx - zw - 22.0, r.y + 4.0, zw + 22.0, r.h - 8.0);
    let ui = &mut l.ui;
    let (zh, _, zclick) = ui.interact(id_of("rf-zoom"), zr);
    ui.icon("zoom_in", Vec2::new(zr.x + 12.0, cy), 16.0, if zh { p.text } else { p.dim });
    ui.text_as_is(&zt, Vec2::new(zr.x + 26.0, cy + 4.5), 12.5, Weight::Medium, p.text, Align::Left);
    ui.icon("expand_more", Vec2::new(zr.right() - 9.0, cy), 15.0, p.dim);
    rx = zr.x - 16.0;
    ui.p().rect(Rect::new(rx + 6.0, r.y + 8.0, 1.0, r.h - 16.0), p.line);
    let refresh = rf_icon(ui, p, "rf-revalidate", Vec2::new(rx - 8.0, cy), "refresh", "Validate again", false, true);
    rx -= 30.0;
    let mut open_scope = None;
    for (scope, label) in [(Scope::Vehicle, "Vehicle Issues: %{n}"), (Scope::Duty, "Duty Issues: %{n}"), (Scope::Global, "Global Issues: %{n}")] {
        let n = board.open(c, scope, false).len();
        let words = omsi_ui::tr(label).replace("%{n}", &n.to_string());
        let ui = &mut l.ui;
        let tw = ui.width(&words, 12.5, Weight::Medium);
        let ir = Rect::new(rx - tw - 26.0, r.y + 3.0, tw + 26.0, r.h - 6.0);
        let (h, _, click) = ui.interact(id_of(&format!("rf-issues-{label}")), ir);
        let on = l.company.planning.rf.panel == Panel::Issues(scope);
        if h || on {
            ui.p().rounded(ir, 3.0, p.text.alpha(0.07));
        }
        let col = if n == 0 { p.dim } else if board.issues.iter().any(|i| i.scope == scope && i.hard && !rf::dismissed(c, board.weekday, &i.key)) { p.err } else { p.warn };
        ui.icon("error", Vec2::new(ir.x + 10.0, cy), 15.0, col);
        ui.text_as_is(&words, Vec2::new(ir.x + 22.0, cy + 4.5), 12.5, Weight::Medium, if h { p.text } else { p.dim }, Align::Left);
        if on {
            // (the panel's pointer)
            ui.p().convex(&[Vec2::new(ir.center().x - 7.0, r.y), Vec2::new(ir.center().x + 7.0, r.y), Vec2::new(ir.center().x, r.y + 6.0)], p.accent);
        }
        if click {
            open_scope = Some(scope);
        }
        rx = ir.x - 10.0;
        ui.p().rect(Rect::new(rx + 4.0, r.y + 8.0, 1.0, r.h - 16.0), p.line);
    }
    super::super::super::tour::anchor("rf-status-issues", Rect::new(rx + 6.0, r.y + 2.0, r.right() - rx - 10.0, r.h - 4.0));
    let s = &mut l.company.planning.rf;
    if let Some(scope) = open_scope {
        if s.panel == Panel::Issues(scope) {
            s.panel = Panel::None;
        } else {
            s.panel = Panel::Issues(scope);
            s.issue_ticks.clear();
        }
    }
    if zclick {
        toggle(&mut s.panel, Panel::Zoom);
    }
    if refresh {
        s.cache = None;
        l.state.set_status(omsi_ui::tr("Validated again.").into_owned(), false);
    }
}

// --- the keys ---------------------------------------------------------------------------------------

/// The shortcuts: G go to, P pin, the arrows along a row and between rows (Shift to widen the
/// choice), Ctrl+Z and Ctrl+Y, Delete to unassign, Escape to let go.
fn keys(l: &mut Launcher, k: &Cached, board: &Board, drivers: bool) {
    if l.ui.focus.is_some() || l.company.planning.rf.prefs.is_some() {
        return;
    }
    let input = l.ui.input.clone();
    if input.ctrl {
        match input.raw_key {
            Some(KeyCode::KeyZ) if input.shift => undo(l, false),
            Some(KeyCode::KeyZ) => undo(l, true),
            Some(KeyCode::KeyY) => undo(l, false),
            _ => {}
        }
        return;
    }
    let text = input.text.to_lowercase();
    if text.contains('g') && !matches!(l.company.planning.rf.panel, Panel::GoTo(_)) {
        l.company.planning.rf.panel = Panel::GoTo(String::new());
        l.ui.input.text.clear();
        return;
    }
    if text.contains('p') {
        if let Some(row) = l.company.planning.rf.sel_row.clone() {
            let s = &mut l.company.planning.rf;
            if let Some(i) = s.pinned.iter().position(|x| *x == row) {
                s.pinned.remove(i);
            } else {
                s.pinned.insert(0, row);
            }
        }
    }
    let arrows: Vec<Key> = input.keys.iter().copied().filter(|k| matches!(k, Key::Left | Key::Right | Key::Up | Key::Down)).collect();
    for a in arrows {
        rf_gantt::arrow_key(l, k, board, drivers, a, input.shift);
    }
    if input.keys.contains(&Key::Delete) && !drivers {
        let sel = l.company.planning.rf.sel.clone();
        if let Some(&(tour, _)) = sel.first() {
            let trips: Vec<usize> = sel.iter().filter(|s| s.0 == tour).map(|s| s.1).collect();
            let wd = board.weekday;
            let tours = board.tours.clone();
            edit(l, omsi_ui::tr("Trips unassigned to the stack").into_owned(), |c| co::studio::unassign(c, wd, &tours, tour, &trips));
        }
    }
}
