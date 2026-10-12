//! RosterFlow's tutorial (Luc: "build a tutorial for the new RosterFlow: I no longer understand
//! how it works, let alone other players"). Three parts, all from the "?" in RosterFlow's title
//! bar:
//!
//! * **The guided tour** (`tour::start_flow`): the tour's spotlight and bubble over RosterFlow -
//!   what it is for (the chain from the timeplan to the roster, and SAVE), how to read its
//!   screen, and each tab with its main controls lit. It starts by itself the first time a
//!   driver opens RosterFlow (once its opening has played), and the "?" brings it back, from
//!   where it was left or from any of its chapters. What a driver has seen is kept beside the
//!   company's tour (`company::tutorial::FlowSeen`).
//! * **The lessons** ("Try it yourself"): short tasks done on the player's own company - move a
//!   trip, cut a duty and join it again, build a duty, fix an issue, let OPTIMIZE propose, plan
//!   a timetable, colour a line, SAVE and DISCARD. Each step has its goal in one line and ticks
//!   itself off once the player has done it (`done`, on what RosterFlow shows: `Probe`); the
//!   tour's guide (`tour::guide`) rings the part the step is about without taking the page from
//!   the player. Before a lesson the planning is kept (`rf::Snapshot`, and the timeplan's line):
//!   "End lesson" keeps the changes or undoes the lesson. A lesson that needs something the
//!   company has not (an own line for the timeplan, an issue to fix) says what to do first.
//! * **The help sheet** ("RosterFlow in 2 minutes"): the chain, the words with a small drawing
//!   of each chip and symbol, the shortcuts, and what to do when something is red.

use super::super::super::tour::{self, Guide, GuideCmd, GuideMode, GuideState};
use super::super::super::ui::{id_of, Input, Key, Ui};
use super::super::super::{mobile, Launcher};
use super::super::{act, tutorial as seen};
use super::rf_brand::arrow_chip;
use super::rf_gantt::{self as gantt, CHIP};
use super::rosterflow::{caps, pal, Drop, Hand, Pal, Panel, Zone, DRIVERS, TIMEPLAN, VEHICLES};
use super::timeplan::{self, TpProbe};
use glam::Vec2;
use omsi_launcher_lib::company::rosterflow as rf;
use omsi_launcher_lib::lines::LineDesign;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

// --- the state --------------------------------------------------------------------------------

/// The tutorial's own on the planning page.
#[derive(Default)]
pub(super) struct Tut {
    /// Where the "?" is (the menu hangs under it).
    pub(super) help_at: Rect,
    menu: bool,
    sheet: bool,
    lesson: Option<Lesson>,
    /// The driver the first-time tour was looked at for (this session).
    asked_for: Option<String>,
    guide: GuideState,
    /// What the menu and the guide covered last frame: the page gets no mouse there.
    covers: Vec<Rect>,
}

impl Tut {
    /// The menu or the help sheet is open (Escape closes it first).
    pub(super) fn holds(&self) -> bool {
        self.menu || self.sheet
    }

    pub(super) fn menu_open(&self) -> bool {
        self.menu
    }

    pub(super) fn toggle_menu(&mut self) {
        self.menu = !self.menu;
        self.sheet = false;
    }
}

// --- the lessons ------------------------------------------------------------------------------

/// The lessons, in the menu's order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum LessonId {
    MoveTrip,
    CutJoin,
    BuildDuty,
    FixIssue,
    Optimise,
    Timeplan,
    Colour,
    SaveDiscard,
}

pub(super) const LESSONS: [LessonId; 8] = [LessonId::MoveTrip, LessonId::CutJoin, LessonId::BuildDuty, LessonId::FixIssue, LessonId::Optimise, LessonId::Timeplan, LessonId::Colour, LessonId::SaveDiscard];

/// What ticks a step off, looked at on what RosterFlow shows (`Probe`) against the step's
/// beginning.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Check {
    /// A step to read: "Got it" goes on.
    Info,
    View(usize),
    /// Vehicles or Drivers.
    Studio,
    HoldTrips,
    HoverZone,
    Edited,
    Undone,
    DutiesUp,
    DutiesDown,
    HoldPiece,
    PanelDuty,
    LockedUp,
    PanelIssues,
    Jumped,
    IssuesDown,
    Proposal,
    ProposalGone,
    TpScreen(usize),
    TpChanged,
    TpTripsUp,
    TpSaved,
    TpRefreshed,
    PanelColour,
    ColourChanged,
    Draft,
    Discarded,
    Saved,
}

/// A lesson's step: its goal in one line, a sentence or two on how, the part it is about (the
/// name it is drawn under: `tour::anchor`; "a|b": the first of them drawn), and what ticks it off.
#[derive(Clone, Copy, Debug)]
pub(super) struct Step {
    pub goal: &'static str,
    pub hint: &'static str,
    pub target: &'static str,
    pub check: Check,
}

const fn step(goal: &'static str, hint: &'static str, target: &'static str, check: Check) -> Step {
    Step { goal, hint, target, check }
}

const OPEN_VEHICLES: Step = step("Open the Vehicles tab", "Each row there is one bus's day of trips.", "rf-tab-vehicles", Check::View(VEHICLES));

const MOVE_TRIP: [Step; 6] = [
    OPEN_VEHICLES,
    step("Pick up a trip", "Drag a coloured chip, or double-click it. Slots appear on every bus it could go to.", "rf-chip", Check::HoldTrips),
    step("Read the stars and the cost", "Hold the mouse over a slot: five stars is the cheapest choice, blue breaks no rule, and the card shows what it costs.", "rf-gantt", Check::HoverZone),
    step("Drop it on another bus", "Click a slot, or let go of the chip over it. Escape puts it back.", "rf-gantt", Check::Edited),
    step("See the figures change", "The green or red numbers beside the figures show what your move did: green is better, red is worse.", "rf-kpi-rest", Check::Info),
    step("Undo it", "Press Ctrl+Z, or the undo arrow at the top. The trip goes back to its bus.", "rf-undo", Check::Undone),
];

const CUT_JOIN: [Step; 5] = [
    step("Open the Vehicles tab", "Duties are cut from the buses' days, so you start here.", "rf-tab-vehicles", Check::View(VEHICLES)),
    step("Cut a duty at a terminus", "Hold the mouse over the gap between two trips. Where a blue line shows, a driver can take over: click to cut there.", "rf-row", Check::DutiesUp),
    step("Look at it in Drivers", "There is one duty more now: the second driver's day starts where you cut.", "rf-tab-drivers", Check::View(DRIVERS)),
    step("Go back to Vehicles", "The arrows on the bus's row mark your cut.", "rf-tab-vehicles", Check::View(VEHICLES)),
    step("Join the two duties again", "Click the arrows where the drivers change. The two duties become one.", "rf-relief", Check::DutiesDown),
];

const BUILD_DUTY: [Step; 6] = [
    step("Open the Drivers tab", "Each row there is one driver's working day.", "rf-tab-drivers", Check::View(DRIVERS)),
    step("Pick up a piece of work", "Drag a chip of a short duty, or double-click it: the whole part on that bus comes along.", "rf-chip", Check::HoldPiece),
    step("Drop it on another duty", "Click a slot: the two become one duty, with a break between them. No slot? The duty would get too long: take a shorter piece.", "rf-gantt", Check::Edited),
    step("Open a duty", "Click a duty's ID on the left. Its panel shows what the duty is made of.", "rf-row-id", Check::PanelDuty),
    step("Read sign-on, sign-off and breaks", "Sign-on and sign-off are when the driver starts and stops. The breaks are between the pieces of work; a long unpaid one is a split.", "rf-duty-panel", Check::Info),
    step("Lock the duty", "Click the lock at the top of the panel. OPTIMIZE will leave this duty as it is.", "rf-duty-lock", Check::LockedUp),
];

const FIX_ISSUE: [Step; 4] = [
    step("Open Vehicles or Drivers", "The issues are counted in the status bar at the bottom of both.", "rf-tab-vehicles", Check::Studio),
    step("Open the issues", "Click Duty Issues or Vehicle Issues in the status bar.", "rf-status-issues", Check::PanelIssues),
    step("Jump to an issue", "Click an issue in the list: RosterFlow shows its row.", "rf-issues-panel", Check::Jumped),
    step("Fix the issue", "Move a trip, cut or join a duty, or change the rules under Preferences. If it is fine as it is, tick it and dismiss it.", "rf-gantt", Check::IssuesDown),
];

const OPTIMISE: [Step; 4] = [
    step("Open Vehicles or Drivers", "OPTIMIZE is on the toolbar of both.", "rf-tab-vehicles", Check::Studio),
    step("Click OPTIMIZE", "RosterFlow works out the blocks and duties by itself. Locked rows stay as they are.", "rf-optimise", Check::Proposal),
    step("Compare the figures", "The figures now show the proposal against your planning: green where it is better, red where it is worse.", "rf-kpi-rest", Check::Info),
    step("Accept it or keep yours", "Accept puts the proposal in your draft; Keep mine forgets it.", "rf-proposal", Check::ProposalGone),
];

const TIMEPLAN_STEPS: [Step; 7] = [
    step("Open the Timeplan tab", "Here a line gets its running times and its timetable.", "rf-tab-timeplan", Check::View(TIMEPLAN)),
    step("Show the running times", "Click the clock icon on the right of the bar.", "rf-tp-bar", Check::TpScreen(0)),
    step("Change a running time", "Click a box with minutes in a time band, type a new number and press Enter. Every trip in that band moves with it.", "rf-tp-body", Check::TpChanged),
    step("Show the timetable", "Click the grid icon on the right of the bar. Each row is a trip, with its time at every stop.", "rf-tp-bar", Check::TpScreen(1)),
    step("Add trips by headway", "Click the clock tool above the table. Give the first and the last departure - in a quiet part of the day, such as the late evening - and the minutes between buses, then the tick.", "rf-tp-add-ok|rf-tp-add", Check::TpTripsUp),
    step("Save the timetable", "Save writes the line's new timetable. If it asks you to mend something first, two trips leave at the same minute: undo them (the arrow in the bar) and add them at other times.", "rf-tp-save", Check::TpSaved),
    step("Refresh the blocks", "Click UNSYNCED - Refresh in the status bar: the buses' blocks and the duties are made again for the new trips.", "rf-tp-refresh", Check::TpRefreshed),
];

const COLOUR: [Step; 3] = [
    OPEN_VEHICLES,
    step("Open the line's colour", "Right-click a trip and choose Line colour.", "rf-chip", Check::PanelColour),
    step("Pick a colour", "Click a colour: every chip of the line takes it at once. Automatic gives it back its old one.", "rf-colour", Check::ColourChanged),
];

const SAVE_DISCARD: [Step; 5] = [
    OPEN_VEHICLES,
    step("Make a change", "Move a trip or cut a duty. The dot at the top turns orange: there are changes not saved.", "rf-gantt", Check::Draft),
    step("DISCARD it", "DISCARD forgets your changes and goes back to what was saved.", "rf-discard", Check::Discarded),
    step("Make a change again", "Any change will do.", "rf-gantt", Check::Draft),
    step("SAVE it", "SAVE makes it the company's planning: the roster, the day and the game run it now. The dot turns green.", "rf-save", Check::Saved),
];

impl LessonId {
    /// Its name in the kept file.
    pub(super) fn id(self) -> &'static str {
        match self {
            LessonId::MoveTrip => "move-trip",
            LessonId::CutJoin => "cut-join",
            LessonId::BuildDuty => "build-duty",
            LessonId::FixIssue => "fix-issue",
            LessonId::Optimise => "optimise",
            LessonId::Timeplan => "timeplan",
            LessonId::Colour => "colour",
            LessonId::SaveDiscard => "save-discard",
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            LessonId::MoveTrip => "Move a trip to another bus",
            LessonId::CutJoin => "Cut a duty and join it again",
            LessonId::BuildDuty => "Build a duty by hand",
            LessonId::FixIssue => "Fix an issue",
            LessonId::Optimise => "Let OPTIMIZE propose",
            LessonId::Timeplan => "Plan a timetable",
            LessonId::Colour => "Give a line its own colour",
            LessonId::SaveDiscard => "SAVE and DISCARD",
        }
    }

    pub(super) fn steps(self) -> &'static [Step] {
        match self {
            LessonId::MoveTrip => &MOVE_TRIP,
            LessonId::CutJoin => &CUT_JOIN,
            LessonId::BuildDuty => &BUILD_DUTY,
            LessonId::FixIssue => &FIX_ISSUE,
            LessonId::Optimise => &OPTIMISE,
            LessonId::Timeplan => &TIMEPLAN_STEPS,
            LessonId::Colour => &COLOUR,
            LessonId::SaveDiscard => &SAVE_DISCARD,
        }
    }
}

/// What a lesson needs first, said instead of beginning it.
const NO_TRIPS: &str = "This day has no trips yet. Give the company a line first (on the Lines page), then come back.";
const ONE_BUS: &str = "Moving a trip needs at least two buses on the day. Add a line or more trips first, then come back.";
const NOTHING_RED: &str = "Nothing is red right now: every rule is kept. Come back when a number shows beside the issues in the status bar.";

/// What RosterFlow shows now, as far as the lessons look.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Probe {
    pub view: usize,
    /// The day's board is made (Vehicles or Drivers was shown).
    pub loaded: bool,
    pub holding_trips: bool,
    pub holding_piece: bool,
    pub zone_hover: bool,
    pub undo: usize,
    pub redo: usize,
    pub edits: u64,
    pub blocks: usize,
    pub duties: usize,
    /// Duties locked.
    pub locked: usize,
    /// Issues open (not dismissed).
    pub issues: usize,
    pub panel_duty: bool,
    pub panel_issues: bool,
    pub panel_colour: bool,
    pub sel_row: Option<String>,
    pub proposal: bool,
    pub draft: bool,
    /// The saved planning is the lesson's beginning's.
    pub saved_same: bool,
    pub colours: Vec<String>,
    pub tp: TpProbe,
}

/// Whether check `c` is met by `p`, against `s` at the step's beginning.
pub(super) fn done(c: Check, p: &Probe, s: &Probe) -> bool {
    match c {
        Check::Info => false,
        Check::View(v) => p.view == v,
        Check::Studio => p.view == VEHICLES || p.view == DRIVERS,
        Check::HoldTrips => p.holding_trips,
        Check::HoverZone => p.zone_hover,
        Check::Edited => p.edits > s.edits,
        Check::Undone => p.undo < s.undo || p.redo > s.redo,
        Check::DutiesUp => p.loaded && p.duties > s.duties,
        Check::DutiesDown => p.loaded && p.duties < s.duties,
        Check::HoldPiece => p.holding_piece,
        Check::PanelDuty => p.panel_duty,
        Check::LockedUp => p.locked > s.locked,
        Check::PanelIssues => p.panel_issues,
        Check::Jumped => p.sel_row.is_some() && p.sel_row != s.sel_row,
        Check::IssuesDown => p.loaded && p.issues < s.issues,
        Check::Proposal => p.proposal,
        Check::ProposalGone => !p.proposal,
        Check::TpScreen(k) => p.view == TIMEPLAN && p.tp.ready && p.tp.screen == k,
        Check::TpChanged => p.tp.changes > s.tp.changes,
        Check::TpTripsUp => p.tp.trips > s.tp.trips,
        Check::TpSaved => s.tp.dirty && !p.tp.dirty && p.tp.unsynced,
        Check::TpRefreshed => s.tp.unsynced && !p.tp.unsynced,
        Check::PanelColour => p.panel_colour,
        Check::ColourChanged => p.colours != s.colours,
        Check::Draft => p.draft,
        Check::Discarded => !p.draft && p.saved_same,
        Check::Saved => !p.draft && !p.saved_same,
    }
}

/// Whether lesson `id` can be done on what RosterFlow shows: None while that is not known yet
/// (the day not made, the timeplan not opened), Some(None) when it can, else what to do first.
pub(super) fn needs(id: LessonId, p: &Probe) -> Option<Option<&'static str>> {
    if id == LessonId::Timeplan {
        return if p.tp.ready {
            Some(None)
        } else {
            p.tp.why.map(Some)
        };
    }
    if !p.loaded {
        return None;
    }
    Some(match id {
        _ if p.blocks == 0 => Some(NO_TRIPS),
        LessonId::MoveTrip if p.blocks < 2 => Some(ONE_BUS),
        LessonId::FixIssue if p.issues == 0 => Some(NOTHING_RED),
        _ => None,
    })
}

/// How a lesson on its way stands.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Working,
    Ending,
    Done,
    Blocked(&'static str),
}

/// A lesson on its way: its step, the steps done, what RosterFlow showed as the step began,
/// and the company as the lesson found it.
struct Lesson {
    id: LessonId,
    at: usize,
    done: Vec<bool>,
    start: Probe,
    snap: rf::Snapshot,
    /// The timeplan's line as the lesson found it (the timeplan lesson; kept once it is read).
    tp: Option<(String, LineDesign, Vec<(String, usize)>)>,
    stage: Stage,
    /// What it needs was looked at.
    checked: bool,
}

/// What RosterFlow shows now (`snap`: the lesson's beginning, for `saved_same`).
fn probe(l: &Launcher, snap: Option<&rf::Snapshot>) -> Probe {
    let s = &l.company.planning.rf;
    let c = l.company.company.as_ref();
    let mut p = Probe {
        view: l.company.planning.view,
        holding_trips: matches!(s.hand, Some(Hand::Trips { .. }) | Some(Hand::Stack(_))),
        holding_piece: matches!(s.hand, Some(Hand::Piece { .. })),
        zone_hover: s.zone_hover.is_some(),
        undo: s.undo.len(),
        redo: s.redo.len(),
        edits: s.edits,
        panel_duty: matches!(s.panel, Panel::Duty(_)),
        panel_issues: matches!(s.panel, Panel::Issues(_)),
        panel_colour: matches!(s.panel, Panel::Colour(_)),
        sel_row: s.sel_row.as_ref().map(|r| format!("{r:?}")),
        proposal: s.proposal.is_some(),
        draft: c.is_some_and(rf::has_draft),
        saved_same: match (snap, c) {
            (Some(sn), Some(c)) => sn.same_saved(c),
            _ => true,
        },
        colours: c.map(|c| c.lines.iter().map(|x| x.colour.clone()).collect()).unwrap_or_default(),
        tp: l.company.planning.timeplan.probe(),
        ..Default::default()
    };
    if let Some(k) = s.cache.as_ref() {
        let b = &k.board;
        p.loaded = true;
        p.blocks = b.figures.blocks;
        p.duties = b.figures.duties;
        p.locked = b.shaped.iter().filter(|x| x.locked).count();
        p.issues = b.issues.iter().filter(|i| !rf::dismissed(&k.view, b.weekday, &i.key)).count();
    }
    p
}

/// Begin lesson `id`: the company kept as it is now.
fn begin_lesson(l: &mut Launcher, id: LessonId) {
    let Some(c) = l.company.company.as_ref() else { return };
    let snap = rf::Snapshot::take(c);
    let start = probe(l, Some(&snap));
    let tp = (id == LessonId::Timeplan).then(|| l.company.planning.timeplan.keep_for_lesson()).flatten();
    let n = id.steps().len();
    let t = &mut l.company.planning.rf.tut;
    t.menu = false;
    t.sheet = false;
    t.lesson = Some(Lesson { id, at: 0, done: vec![false; n], start, snap, tp, stage: Stage::Working, checked: false });
    log::info!("rosterflow: the lesson {} begins", id.id());
}

/// End the lesson on its way: its changes kept, or the company put back as it found it.
fn end_lesson(l: &mut Launcher, undo: bool) {
    let Some(lesson) = l.company.planning.rf.tut.lesson.take() else { return };
    if !undo {
        l.state.set_status(omsi_ui::tr("The lesson is over: your changes stay.").into_owned(), false);
        return;
    }
    let mut ok = true;
    if let Some((line, design, unsynced)) = lesson.tp.as_ref() {
        ok = timeplan::restore(l, line, design, unsynced.clone());
    }
    let snap = lesson.snap.clone();
    act(l, |c| {
        snap.restore(c);
        Ok(())
    });
    let s = &mut l.company.planning.rf;
    s.undo.clear();
    s.redo.clear();
    s.settle();
    s.proposal = None;
    s.before = None;
    s.panel = Panel::None;
    s.cache = None;
    if ok {
        l.state.set_status(omsi_ui::tr("The lesson is undone: the planning is as it was before.").into_owned(), false);
    } else {
        l.state.set_status(omsi_ui::tr("The planning is as it was, but the timetable could not be put back: change it back in the Timeplan tab.").into_owned(), true);
    }
    log::info!("rosterflow: the lesson {} undone", lesson.id.id());
}

// --- once a frame ----------------------------------------------------------------------------------

/// Before the planning page is drawn: Escape closes the menu or the help sheet; the help sheet
/// takes all the input, and what the menu and the guide covered last frame takes the mouse.
/// Returns the input to give back afterwards.
pub(super) fn before(l: &mut Launcher) -> Option<Input> {
    let t = &mut l.company.planning.rf.tut;
    if (t.menu || t.sheet) && l.ui.input.keys.contains(&Key::Escape) {
        t.menu = false;
        t.sheet = false;
        l.ui.input.keys.retain(|k| *k != Key::Escape);
    }
    let over = t.covers.iter().any(|r| r.contains(l.ui.input.mouse));
    if !t.sheet && !over {
        return None;
    }
    let saved = l.ui.input.clone();
    let i = &mut l.ui.input;
    i.mouse = Vec2::new(-1e4, -1e4);
    (i.pressed, i.released, i.right_pressed, i.double_click) = (false, false, false, false);
    i.wheel = Vec2::ZERO;
    if t.sheet {
        i.keys.clear();
        i.text.clear();
        i.raw_key = None;
    }
    Some(saved)
}

/// Once the planning page is drawn: the tour started the first time, a lesson's guide, the
/// menu, the help sheet.
pub(super) fn after(l: &mut Launcher, area: Rect, held: Option<Input>) {
    if let Some(i) = held {
        l.ui.input = i;
    }
    l.company.planning.rf.tut.covers.clear();
    first_time(l);
    if !tour::active(l) {
        lesson_frame(l, area);
    }
    if l.company.planning.rf.tut.menu {
        menu(l);
    }
    if l.company.planning.rf.tut.sheet {
        sheet(l);
    }
}

/// The tour by itself, the first time a driver opens RosterFlow (once its opening has played;
/// not on a phone, not over a dialog or another tour).
fn first_time(l: &mut Launcher) {
    if mobile::mobile() || tour::active(l) || l.company.planning.rf.intro.is_some() || l.company.dialog.is_some() || l.company.planning.rf.tut.lesson.is_some() {
        return;
    }
    let profile = l.state.config.profile.clone();
    if l.company.planning.rf.tut.asked_for.as_deref() == Some(profile.as_str()) {
        return;
    }
    l.company.planning.rf.tut.asked_for = Some(profile.clone());
    let mut s = seen::flow_of(&profile);
    if s.seen {
        return;
    }
    s.seen = true;
    seen::flow_keep(&profile, s);
    tour::start_flow(l, 0);
}

/// The lesson on its way: its step looked at, its guide drawn and answered.
fn lesson_frame(l: &mut Launcher, area: Rect) {
    let Some(mut lesson) = l.company.planning.rf.tut.lesson.take() else { return };
    let p = probe(l, Some(&lesson.snap));
    // (the timeplan's line kept as soon as it is read)
    if lesson.id == LessonId::Timeplan && lesson.tp.is_none() {
        lesson.tp = l.company.planning.timeplan.keep_for_lesson();
        if lesson.tp.is_some() {
            lesson.start.tp = p.tp.clone();
        }
    }
    if !lesson.checked && lesson.stage == Stage::Working {
        match needs(lesson.id, &p) {
            Some(Some(why)) => {
                lesson.checked = true;
                lesson.stage = Stage::Blocked(why);
            }
            Some(None) => lesson.checked = true,
            None => {}
        }
    }
    let steps = lesson.id.steps();
    if lesson.stage == Stage::Working {
        // (a step's check met: on to the next, from what RosterFlow shows now)
        while lesson.at < steps.len() && steps[lesson.at].check != Check::Info && done(steps[lesson.at].check, &p, &lesson.start) {
            lesson.done[lesson.at] = true;
            lesson.at += 1;
            lesson.start = p.clone();
        }
        if lesson.at >= steps.len() {
            lesson.stage = Stage::Done;
            let profile = l.state.config.profile.clone();
            seen::flow_lesson_done(&profile, lesson.id.id());
        }
    }
    let list: Vec<(&'static str, bool)> = steps.iter().zip(lesson.done.iter()).map(|(s, d)| (s.goal, *d)).collect();
    let now = steps.get(lesson.at.min(steps.len().saturating_sub(1))).copied();
    let (title, text, mode) = match lesson.stage {
        Stage::Working => {
            let s = now.unwrap_or(steps[0]);
            (s.goal, s.hint, GuideMode::Step { info: s.check == Check::Info })
        }
        Stage::Ending => ("End the lesson?", "Keep what you changed, or put everything back as it was before the lesson.", GuideMode::Ending),
        Stage::Done => ("Well done!", "You did every step of this lesson. Keep what you changed, or put it all back as it was.", GuideMode::Done),
        Stage::Blocked(why) => ("First this", why, GuideMode::Blocked),
    };
    let target = match lesson.stage {
        Stage::Working => now.map(|s| s.target).filter(|t| !t.is_empty()).and_then(|t| t.split('|').find_map(tour::anchored_rect)),
        _ => None,
    };
    // (the guide lies over RosterFlow's Gantt, clear of its title bar and status bar)
    let room = Rect::new(area.x, area.y + 48.0, area.w, (area.h - 48.0 - 34.0).max(120.0));
    let mut avoid = l.company.planning.rf.over.clone();
    avoid.extend(tour::anchored_rect("rf-tp-form"));
    let g = Guide { lesson: lesson.id.title(), title, text, steps: &list, at: lesson.at, target, room, avoid: &avoid, mode };
    let mut st = std::mem::take(&mut l.company.planning.rf.tut.guide);
    let (cmd, rect) = tour::guide(&mut l.ui, &g, &mut st);
    let t = &mut l.company.planning.rf.tut;
    t.guide = st;
    t.covers.push(rect);
    let mut finish: Option<bool> = None;
    match cmd {
        Some(GuideCmd::Next) => {
            if lesson.at < steps.len() {
                lesson.done[lesson.at] = true;
                lesson.at += 1;
                lesson.start = p.clone();
            }
        }
        Some(GuideCmd::End) => lesson.stage = Stage::Ending,
        Some(GuideCmd::Resume) => lesson.stage = if lesson.at >= steps.len() { Stage::Done } else { Stage::Working },
        Some(GuideCmd::Keep) => finish = Some(false),
        Some(GuideCmd::Undo) => finish = Some(true),
        Some(GuideCmd::Close) => finish = Some(false),
        None => {}
    }
    l.company.planning.rf.tut.lesson = Some(lesson);
    if let Some(undo) = finish {
        end_lesson(l, undo);
    }
}

// --- the "?" menu -----------------------------------------------------------------------------------

/// A row of the menu.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Item {
    Head(&'static str),
    /// The tour from this stop (a chapter's first; `usize::MAX`: where it was left).
    Tour(&'static str, usize),
    Lesson(LessonId),
    Sheet,
}

fn menu_items() -> Vec<Item> {
    let mut v = vec![Item::Head("Guided tour"), Item::Tour("", usize::MAX)];
    v.extend(tour::flow_chapters().into_iter().map(|(name, at)| Item::Tour(name, at)));
    v.push(Item::Head("Try it yourself"));
    v.extend(LESSONS.iter().map(|id| Item::Lesson(*id)));
    v.push(Item::Head("Help"));
    v.push(Item::Sheet);
    v
}

const MENU_W: f32 = 312.0;
const MENU_ROW: f32 = 30.0;
const MENU_HEAD: f32 = 28.0;

/// The menu under the "?": the tour (from where it was left, or a chapter), the lessons with a
/// tick when done, and the help sheet.
fn menu(l: &mut Launcher) {
    let p = pal(&l.ui);
    let items = menu_items();
    let at = l.company.planning.rf.tut.help_at;
    let size = l.ui.size;
    let content: f32 = items.iter().map(|i| if matches!(i, Item::Head(_)) { MENU_HEAD } else { MENU_ROW }).sum::<f32>() + 12.0;
    let w = MENU_W.min(size.x - 16.0);
    let x = at.x.min(size.x - w - 8.0).max(8.0);
    let y = at.bottom() + 6.0;
    let h = content.min(size.y - y - 8.0).max(120.0);
    let r = Rect::new(x, y, w, h);
    l.company.planning.rf.tut.covers.push(r);
    let profile = l.state.config.profile.clone();
    let s = seen::flow_of(&profile);
    let resume = seen::flow_resume_at(&s, tour::flow_len());
    l.ui.as_drawn();
    l.ui.solid(r);
    l.ui.p().shadow(r, 6.0, 18.0, Color::rgba(0, 0, 0, 0.35));
    l.ui.p().rounded(r, 4.0, p.menu);
    let mut picked: Option<Item> = None;
    let ink = p.menu_text;
    let dim = p.card_dim;
    let accent = p.accent;
    let ok = p.ok;
    let light = p.light;
    l.ui.scroll_area("rf-tut-menu", r.inset(2.0), &mut |ui: &mut Ui, v: Rect| {
        let mut y = v.y + 4.0;
        for (k, it) in items.iter().enumerate() {
            match *it {
                Item::Head(words) => {
                    let t = omsi_ui::tr(words).to_uppercase();
                    ui.text_as_is(&t, Vec2::new(v.x + 14.0, y + 19.0), 10.5, Weight::Bold, dim, Align::Left);
                    y += MENU_HEAD;
                }
                _ => {
                    let rr = Rect::new(v.x, y, v.w, MENU_ROW);
                    let (h, _, click) = ui.interact(id_of(&format!("rf-tut-item-{k}")), rr);
                    if h {
                        ui.p().rect(rr, if light { accent.alpha(0.10) } else { Color::rgba(20, 36, 60, 0.10) });
                    }
                    let (icon, words, done): (&str, String, bool) = match *it {
                        Item::Tour("", _) => ("tour", omsi_ui::tr(if resume > 0 { "Go on with the tour" } else { "Take the tour" }).into_owned(), s.done),
                        Item::Tour(chapter, _) => ("chevron_right", omsi_ui::tr(chapter).into_owned(), false),
                        Item::Lesson(id) => ("school", omsi_ui::tr(id.title()).into_owned(), s.lessons.iter().any(|x| x == id.id())),
                        _ => ("menu_book", omsi_ui::tr("RosterFlow in 2 minutes").into_owned(), false),
                    };
                    let indent = if matches!(it, Item::Tour(c, _) if !c.is_empty()) { 16.0 } else { 0.0 };
                    ui.icon(icon, Vec2::new(v.x + 22.0 + indent, rr.center().y), 17.0, if h { accent } else { dim });
                    ui.text_as_is(&words, Vec2::new(v.x + 40.0 + indent, rr.center().y + 4.5), 13.0, Weight::Medium, if h { accent } else { ink }, Align::Left);
                    if done {
                        ui.icon("check_circle", Vec2::new(v.right() - 18.0, rr.center().y), 17.0, ok);
                    }
                    if click {
                        picked = Some(*it);
                    }
                    y += MENU_ROW;
                }
            }
        }
        y - v.y + 8.0
    });
    l.ui.end_as_drawn();
    // a click elsewhere closes it (not the "?" itself: that click toggles it)
    if l.ui.input.pressed && !r.contains(l.ui.input.mouse) && !at.contains(l.ui.input.mouse) {
        l.company.planning.rf.tut.menu = false;
    }
    let Some(item) = picked else { return };
    l.company.planning.rf.tut.menu = false;
    match item {
        Item::Tour(_, k) => {
            let at = if k == usize::MAX { resume } else { k };
            tour::start_flow(l, at);
        }
        Item::Lesson(id) => begin_lesson(l, id),
        Item::Sheet => l.company.planning.rf.tut.sheet = true,
        Item::Head(_) => {}
    }
}

// --- the help sheet ----------------------------------------------------------------------------------

/// The chain, as the sheet says it: the tab's icon, its name, what it is for.
const CHAIN: [(&str, &str, &str); 5] = [
    ("departure_board", "Timeplan", "When the buses run: each line's running times and timetable."),
    ("directions_bus", "Vehicles", "Which bus drives which trips. One bus's day is a block."),
    ("person", "Drivers", "Who drives. Blocks are cut into duties at a terminus or the depot."),
    ("group", "Roster", "Which driver and which bus, for each weekday."),
    ("save", "Save", "Nothing counts until you save. DISCARD goes back to what was saved."),
];

/// A word's small drawing in the sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Sym {
    Trip,
    Direction,
    Block,
    Duty,
    Piece,
    Relief,
    SignOn,
    Deadhead,
    Pull,
    Layover,
    Split,
    Travel,
    Slot,
    Stack,
    Lock,
    Draft,
    Pvr,
    Band,
    Running,
    Headway,
    Pattern,
    Issue,
}

/// The words, in plain words, each with its drawing.
pub(super) const GLOSSARY: [(Sym, &str, &str); 22] = [
    (Sym::Trip, "Trip (a chip)", "One run of a bus from one end of the line to the other, with passengers. Shown as a chip in the line's colour."),
    (Sym::Direction, "Direction", "The pointed end of a chip shows the way the trip runs: to the right is outbound, to the left inbound."),
    (Sym::Block, "Block", "One bus's whole day: every trip it drives, from leaving the depot until it is back. Also called a vehicle schedule."),
    (Sym::Duty, "Duty", "One driver's working day, from sign-on to sign-off."),
    (Sym::Piece, "Piece of work", "The part of a duty spent on one bus. A duty can have pieces on different buses."),
    (Sym::Relief, "Relief", "Where one driver hands the bus to the next: always at a terminus or at the depot (the relief point)."),
    (Sym::SignOn, "Sign-on and sign-off", "The start and the end of a duty, with a few minutes to check the bus."),
    (Sym::Deadhead, "Deadhead", "A bus driving without passengers, for example to where its next trip starts. An orange dot means its time is a guess."),
    (Sym::Pull, "Pull-out and pull-in", "The bus leaving the depot in the morning and going back at night."),
    (Sym::Layover, "Layover", "The wait at the end of the line before the next trip."),
    (Sym::Split, "Split", "A long unpaid break in the middle of a duty: the driver works early and again later."),
    (Sym::Travel, "Travel", "How a driver gets to the next bus: W walking, P by public transport, R by road in a company car."),
    (Sym::Slot, "Drop slot", "Where something picked up may go, with stars for how good it is: blue breaks no rule, red breaks one."),
    (Sym::Stack, "Stack", "Trips no bus drives yet. They wait above the rows until you give them a bus."),
    (Sym::Lock, "Locked rows", "A locked bus or duty stays as it is: OPTIMIZE and your other edits leave it alone."),
    (Sym::Draft, "Draft", "Your changes before SAVE (orange dot). The game keeps running the saved planning (green dot) until you save."),
    (Sym::Pvr, "PVR", "Peak vehicle requirement: the most buses out at the same time. That is how many buses the day needs."),
    (Sym::Band, "Time band", "A part of the day with its own running times: the rush hour is often slower than the evening."),
    (Sym::Running, "Running time", "The minutes a bus needs from one stop to the next."),
    (Sym::Headway, "Headway", "Every how many minutes a bus leaves: a headway of 10 minutes is six buses an hour."),
    (Sym::Pattern, "Pattern", "One version of a line's route: the full run, or a short one that leaves out stops."),
    (Sym::Issue, "Issue", "A rule that is broken, such as a duty without a meal break. Red is a hard rule, orange a soft one that costs a penalty."),
];

/// The shortcuts: the keys (`true`: words to translate) and what they do.
pub(super) const SHORTCUTS: [(&str, bool, &str); 12] = [
    ("Click a chip", true, "Choose the trip and open its card"),
    ("Double-click or drag", true, "Pick it up to move it"),
    ("Shift + click", true, "Choose more trips of one row"),
    ("Right-click", true, "Menu: move, swap, unassign, line colour and more"),
    ("Arrow keys", true, "To the next trip; with Shift the choice grows"),
    ("G", false, "Go to a bus or a duty by its ID"),
    ("P", false, "Pin the chosen row to the top"),
    ("Delete", true, "Put the chosen trips on the stack"),
    ("Ctrl + Z  /  Ctrl + Y", false, "Undo / redo"),
    ("Ctrl + wheel", true, "Zoom in and out on the hours"),
    ("Shift + wheel", true, "Move along the day"),
    ("Escape", true, "Let go, close what is open"),
];

/// What to do when something is red.
pub(super) const RED: [&str; 6] = [
    "Red numbers in the status bar: some trips have no bus or no driver. Look at the stack above the rows, or let OPTIMIZE fill the gaps.",
    "A red \"!\" by a row: hold the mouse over it to read which rule is broken. Click the issue counter in the status bar to see them all, and click one to jump to its row.",
    "A red slot while you move something: dropping there breaks a rule. You may still do it, but the day gets an issue.",
    "A red difference beside a figure: your last change made it worse. Ctrl+Z takes it back.",
    "More duties than drivers (a Global issue): hire drivers on the Staff page, or make fewer, longer duties.",
    "Orange is a soft rule: allowed, but it adds a penalty to the cost. Change the rules under Preferences (the first icon at the top).",
];

/// The sheet's words besides those above.
pub(super) const SHEET_WORDS: [&str; 6] = ["RosterFlow in 2 minutes", "Plan your bus company's day in four steps, then save.", "The chain", "Words", "Shortcuts", "If something is red"];

/// The help sheet over RosterFlow: the chain, the words, the shortcuts, what to do when
/// something is red. Two columns where there is room, one on a narrow window.
fn sheet(l: &mut Launcher) {
    let p = pal(&l.ui);
    let size = l.ui.size;
    let screen = Rect::new(0.0, 0.0, size.x, size.y);
    let w = (size.x - 32.0).min(1040.0);
    let h = (size.y - 48.0).min(760.0);
    let r = Rect::new((size.x - w) * 0.5, (size.y - h) * 0.5, w, h);
    l.ui.as_drawn();
    l.ui.solid(screen);
    l.ui.p().rect(screen, Color::rgba(4, 8, 18, 0.55));
    l.ui.p().shadow(r, 8.0, 40.0, Color::rgba(0, 0, 0, 0.45));
    l.ui.p().rounded(r, 8.0, p.sheet);
    // the head: the mark, the title, a line under it, the close
    let ui = &mut l.ui;
    super::rf_brand::mark(ui, Rect::new(r.x + 20.0, r.y + 16.0, 34.0, 34.0), 1.0, 1.0);
    ui.text_in(SHEET_WORDS[0], Rect::new(r.x + 66.0, r.y + 12.0, w - 130.0, 26.0), 19.0, Weight::Bold, p.text, Align::Left);
    ui.text_in(SHEET_WORDS[1], Rect::new(r.x + 66.0, r.y + 36.0, w - 130.0, 18.0), 12.5, Weight::Medium, p.dim, Align::Left);
    let xr = Rect::new(r.right() - 46.0, r.y + 16.0, 30.0, 30.0);
    let (xh, _, close) = ui.interact(id_of("rf-sheet-close"), xr);
    ui.icon("close", xr.center(), 20.0, if xh { p.err } else { p.dim });
    ui.p().rect(Rect::new(r.x, r.y + 66.0, r.w, 1.0), p.line);
    let body = Rect::new(r.x + 4.0, r.y + 68.0, r.w - 8.0, r.h - 72.0);
    let two = w >= 760.0;
    let pp = p;
    l.ui.scroll_area("rf-sheet", body, &mut |ui: &mut Ui, v: Rect| {
        let pad = 20.0;
        if two {
            let colw = (v.w - 3.0 * pad) * 0.5;
            let left = Rect::new(v.x + pad, v.y, colw, 0.0);
            let right = Rect::new(v.x + 2.0 * pad + colw, v.y, colw, 0.0);
            let a = chain_section(ui, &pp, left.x, left.y + 14.0, colw);
            let a = words_section(ui, &pp, left.x, a + 10.0, colw);
            let b = keys_section(ui, &pp, right.x, right.y + 14.0, colw);
            let b = red_section(ui, &pp, right.x, b + 10.0, colw);
            a.max(b) - v.y + 20.0
        } else {
            let colw = v.w - 2.0 * pad;
            let x = v.x + pad;
            let y = chain_section(ui, &pp, x, v.y + 14.0, colw);
            let y = words_section(ui, &pp, x, y + 10.0, colw);
            let y = keys_section(ui, &pp, x, y + 10.0, colw);
            let y = red_section(ui, &pp, x, y + 10.0, colw);
            y - v.y + 20.0
        }
    });
    l.ui.end_as_drawn();
    let outside = l.ui.input.pressed && !r.contains(l.ui.input.mouse);
    if close || outside {
        l.company.planning.rf.tut.sheet = false;
    }
}

/// A section's heading; returns where its body begins.
fn heading(ui: &mut Ui, p: &Pal, x: f32, y: f32, w: f32, text: &str) -> f32 {
    caps(ui, Rect::new(x, y, w, 18.0), text, p.accent, Align::Left);
    ui.p().rect(Rect::new(x, y + 22.0, w, 1.0), p.line);
    y + 32.0
}

fn chain_section(ui: &mut Ui, p: &Pal, x: f32, y: f32, w: f32) -> f32 {
    let mut y = heading(ui, p, x, y, w, SHEET_WORDS[2]);
    for (k, (icon, name, text)) in CHAIN.iter().enumerate() {
        let badge = Rect::new(x, y + 2.0, 26.0, 26.0);
        ui.p().rounded(badge, 13.0, if k == 4 { p.ok } else { p.accent });
        ui.icon(icon, badge.center(), 15.0, Color::WHITE);
        let tw = w - 40.0;
        ui.text_in(name, Rect::new(x + 38.0, y, tw, 18.0), 13.5, Weight::Bold, p.text, Align::Left);
        let th = ui.paragraph(text, Vec2::new(x + 38.0, y + 18.0), tw, 12.5, Weight::Regular, p.dim);
        y += (18.0 + th).max(32.0) + 8.0;
    }
    y
}

fn words_section(ui: &mut Ui, p: &Pal, x: f32, y: f32, w: f32) -> f32 {
    let mut y = heading(ui, p, x, y, w, SHEET_WORDS[3]);
    let draw_w = 96.0;
    for (sym, term, text) in GLOSSARY.iter() {
        let tw = w - draw_w - 14.0;
        let th = ui.paragraph_height(text, tw, 12.5, Weight::Regular);
        let rh = (20.0 + th).max(36.0);
        let d = Rect::new(x, y + 2.0, draw_w, 30.0);
        ui.p().rounded(d, 4.0, p.grid);
        ui.p().rounded_border(d, 4.0, 1.0, p.line);
        ui.push_clip(d, 4.0);
        symbol(ui, p, *sym, d);
        ui.pop_clip();
        ui.text_in(term, Rect::new(x + draw_w + 14.0, y, tw, 18.0), 13.5, Weight::Bold, p.text, Align::Left);
        ui.paragraph(text, Vec2::new(x + draw_w + 14.0, y + 18.0), tw, 12.5, Weight::Regular, p.dim);
        y += rh + 10.0;
    }
    y
}

fn keys_section(ui: &mut Ui, p: &Pal, x: f32, y: f32, w: f32) -> f32 {
    let mut y = heading(ui, p, x, y, w, SHEET_WORDS[4]);
    let kw = (w * 0.42).min(200.0);
    for (keys, words, what) in SHORTCUTS.iter() {
        let label = if *words { omsi_ui::tr(keys).into_owned() } else { keys.to_string() };
        let lw = ui.width(&label, 12.0, Weight::Bold) + 16.0;
        let cap = Rect::new(x, y + 2.0, lw.min(kw), 24.0);
        ui.p().rounded(cap, 4.0, p.tool);
        ui.p().rounded_border(cap, 4.0, 1.0, p.line);
        ui.text_as_is(&label, Vec2::new(cap.x + 8.0, cap.center().y + 4.0), 12.0, Weight::Bold, p.text, Align::Left);
        let th = ui.paragraph(what, Vec2::new(x + kw + 12.0, y + 4.0), w - kw - 12.0, 12.5, Weight::Regular, p.dim);
        y += th.max(24.0) + 10.0;
    }
    y
}

fn red_section(ui: &mut Ui, p: &Pal, x: f32, y: f32, w: f32) -> f32 {
    let mut y = heading(ui, p, x, y, w, SHEET_WORDS[5]);
    for (k, tip) in RED.iter().enumerate() {
        let col = if k == RED.len() - 1 { p.warn } else { p.err };
        ui.text_as_is("!", Vec2::new(x + 6.0, y + 15.0), 16.0, Weight::Black, col, Align::Center);
        let th = ui.paragraph(tip, Vec2::new(x + 24.0, y + 1.0), w - 24.0, 12.5, Weight::Regular, p.text.alpha(0.9));
        y += th + 12.0;
    }
    y
}

// --- the drawings -----------------------------------------------------------------------------------

/// The palette's line colours the drawings use.
const BLUE: Color = Color::rgba(79, 129, 190, 1.0);
const PINK: Color = Color::rgba(232, 82, 140, 1.0);
const ORANGE: Color = Color::rgba(245, 128, 40, 1.0);

/// A word's drawing in `r`, in RosterFlow's symbols.
pub(super) fn symbol(ui: &mut Ui, p: &Pal, sym: Sym, r: Rect) {
    let cy = r.center().y;
    let (x0, x1) = (r.x + 8.0, r.right() - 8.0);
    let chip_at = |ui: &mut Ui, a: f32, b: f32, c: Color, right: bool, label: &str| {
        gantt::chip(ui, Rect::new(a, cy - CHIP * 0.5, b - a, CHIP), c, right, label, None, false, false, 1.0);
    };
    match sym {
        Sym::Trip => chip_at(ui, x0 + 10.0, x1 - 10.0, BLUE, true, "1"),
        Sym::Direction => {
            chip_at(ui, x0, x0 + 34.0, BLUE, true, "");
            chip_at(ui, x1 - 34.0, x1, BLUE, false, "");
        }
        Sym::Block => {
            gantt::pull(ui, p, x0, x0 + 10.0, cy, false);
            chip_at(ui, x0 + 13.0, x0 + 33.0, BLUE, true, "");
            chip_at(ui, x0 + 36.0, x0 + 56.0, BLUE, false, "");
            chip_at(ui, x0 + 59.0, x1 - 13.0, BLUE, true, "");
            gantt::pull(ui, p, x1 - 10.0, x1, cy, false);
        }
        Sym::Duty => {
            gantt::t_bar(ui, p, x0, x0 + 10.0, cy, true);
            chip_at(ui, x0 + 12.0, x0 + 40.0, PINK, true, "");
            chip_at(ui, x0 + 42.0, x1 - 12.0, PINK, false, "");
            gantt::t_bar(ui, p, x1 - 10.0, x1, cy, false);
        }
        Sym::Piece => {
            chip_at(ui, x0, x0 + 34.0, BLUE, true, "");
            gantt::relief_arrow(ui, p, x0 + 40.0, cy, true, true);
            chip_at(ui, x0 + 46.0, x1, ORANGE, true, "");
        }
        Sym::Relief => {
            chip_at(ui, x0, x0 + 28.0, BLUE, true, "");
            gantt::relief_arrow(ui, p, x0 + 34.0, cy, false, true);
            gantt::relief_arrow(ui, p, x1 - 34.0, cy, true, false);
            chip_at(ui, x1 - 28.0, x1, BLUE, false, "");
        }
        Sym::SignOn => {
            gantt::t_bar(ui, p, x0 + 4.0, x0 + 24.0, cy, true);
            chip_at(ui, x0 + 26.0, x1 - 26.0, PINK, true, "");
            gantt::t_bar(ui, p, x1 - 24.0, x1 - 4.0, cy, false);
        }
        Sym::Deadhead => {
            chip_at(ui, x0, x0 + 26.0, BLUE, true, "");
            gantt::deadhead(ui, p, x0 + 30.0, x1 - 30.0, cy, true);
            chip_at(ui, x1 - 26.0, x1, BLUE, true, "");
        }
        Sym::Pull => {
            gantt::pull(ui, p, x0, x0 + 18.0, cy, false);
            chip_at(ui, x0 + 22.0, x1 - 22.0, BLUE, true, "");
            gantt::pull(ui, p, x1 - 18.0, x1, cy, false);
        }
        Sym::Layover => {
            chip_at(ui, x0, x0 + 26.0, BLUE, true, "");
            ui.text_as_is("5m", Vec2::new(r.center().x, cy + 3.5), 9.5, Weight::Medium, p.dim, Align::Center);
            chip_at(ui, x1 - 26.0, x1, BLUE, false, "");
        }
        Sym::Split => {
            chip_at(ui, x0, x0 + 16.0, PINK, true, "");
            let line = if p.light { p.dim } else { Color::rgba(220, 224, 230, 0.9) };
            ui.p().rect(Rect::new(x0 + 18.0, cy - 0.5, x1 - x0 - 36.0, 1.0), line);
            let tw = ui.width(&omsi_ui::tr("Split"), 9.5, Weight::Medium) + 6.0;
            let lr = Rect::new(r.center().x - tw * 0.5, cy - 6.0, tw, 12.0);
            ui.p().rect(lr, p.grid);
            ui.text_in("Split", lr, 9.5, Weight::Medium, line, Align::Center);
            chip_at(ui, x1 - 16.0, x1, PINK, false, "");
        }
        Sym::Travel => {
            for (k, m) in ["W", "P", "R"].iter().enumerate() {
                let br = Rect::new(x0 + 4.0 + k as f32 * 26.0, cy - CHIP * 0.45, 20.0, CHIP * 0.9);
                ui.p().rounded(br, 3.0, p.dead);
                ui.p().rounded_border(br, 3.0, 1.3, p.warn);
                ui.text_as_is(m, Vec2::new(br.center().x, cy + 4.0), 10.5, Weight::Bold, p.warn, Align::Center);
            }
        }
        Sym::Slot => {
            let z = Zone { drop: Drop::New, ok: Ok(()), stars: 5, delta: 0, clean: true };
            gantt::draw_zone(ui, p, Rect::new(x0, cy - CHIP * 0.5 - 1.0, 36.0, CHIP + 2.0), &z, true, false);
            let bad = Zone { stars: 2, clean: false, ..z };
            gantt::draw_zone(ui, p, Rect::new(x1 - 36.0, cy - CHIP * 0.5 - 1.0, 36.0, CHIP + 2.0), &bad, true, false);
        }
        Sym::Stack => {
            ui.icon("inventory_2", Vec2::new(x0 + 8.0, cy), 15.0, p.dim);
            chip_at(ui, x0 + 22.0, x1, ORANGE, true, "");
        }
        Sym::Lock => {
            ui.icon("lock", Vec2::new(x0 + 8.0, cy), 15.0, p.accent);
            chip_at(ui, x0 + 22.0, x1, BLUE, true, "");
        }
        Sym::Draft => {
            ui.p().circle(Vec2::new(r.center().x - 16.0, cy), 5.0, p.warn);
            ui.p().circle(Vec2::new(r.center().x + 16.0, cy), 5.0, p.ok);
        }
        Sym::Pvr => {
            for k in 0..3 {
                ui.icon("directions_bus", Vec2::new(r.center().x - 22.0 + k as f32 * 22.0, cy), 18.0, p.accent);
            }
        }
        Sym::Band => {
            let mid = r.center().x;
            ui.p().rect(Rect::new(x0, cy - 9.0, mid - x0 - 2.0, 18.0), p.accent.alpha(0.25));
            ui.p().rect(Rect::new(mid + 2.0, cy - 9.0, x1 - mid - 2.0, 18.0), p.accent.alpha(0.55));
        }
        Sym::Running => {
            let br = Rect::new(r.center().x - 20.0, cy - 10.0, 40.0, 20.0);
            ui.p().rounded(br, 3.0, p.field);
            ui.p().rounded_border(br, 3.0, 1.0, p.line);
            ui.text_as_is("7 m", Vec2::new(br.center().x, cy + 4.0), 11.0, Weight::Bold, p.text, Align::Center);
        }
        Sym::Headway => {
            for k in 0..5 {
                let x = x0 + 4.0 + k as f32 * ((x1 - x0 - 8.0) / 4.0);
                ui.p().rect(Rect::new(x - 1.0, cy - 8.0, 2.0, 16.0), p.accent);
            }
        }
        Sym::Pattern => {
            ui.p().rect(Rect::new(x0, cy - 6.0, x1 - x0, 3.0), BLUE);
            ui.p().rect(Rect::new(x0 + 18.0, cy + 4.0, x1 - x0 - 40.0, 3.0), BLUE.alpha(0.6));
        }
        Sym::Issue => {
            ui.text_as_is("!", Vec2::new(x0 + 6.0, cy + 5.5), 15.0, Weight::Black, p.err, Align::Center);
            ui.text_as_is("!", Vec2::new(x0 + 18.0, cy + 5.5), 15.0, Weight::Black, p.warn, Align::Center);
            chip_at(ui, x0 + 28.0, x1, BLUE, true, "");
        }
    }
}

/// The chain's names and the symbols' names on the tour's cards.
pub(super) const PICTURE_WORDS: [&str; 8] = ["Depot run", "Sign-on", "Trip", "Empty run", "Driver change", "Split", "Walk", "Broken rule"];

/// A drawing for the tour's cards (0: the chain, 1: the symbols) in `r`, `a` of it seen, `t`
/// seconds since the card came (None: standing still). In RosterFlow's own look.
pub(super) fn picture(ui: &mut Ui, which: u8, r: Rect, a: f32, t: Option<f32>) {
    let p = pal(ui);
    ui.as_drawn();
    ui.p().rounded(r, 8.0, p.bg.alpha(a));
    if which == 0 {
        chain_picture(ui, &p, r, t);
    } else {
        symbols_picture(ui, &p, r);
    }
    ui.end_as_drawn();
}

/// The chain: the four tabs and SAVE as cards, joined by arrows; with the motion on, a light
/// goes along them.
fn chain_picture(ui: &mut Ui, p: &Pal, r: Rect, t: Option<f32>) {
    let inner = r.inset(18.0);
    let gap = 22.0;
    let bw = (inner.w - 4.0 * gap) / 5.0;
    let bh = (inner.h - 20.0).min(120.0);
    let y = inner.y + (inner.h - bh) * 0.5;
    let lit = t.map(|t| ((t / 0.9) as usize) % 6);
    for (k, (icon, name, _)) in CHAIN.iter().enumerate() {
        let b = Rect::new(inner.x + k as f32 * (bw + gap), y, bw, bh);
        let on = lit == Some(k);
        let save = k == 4;
        let col = if save { p.ok } else { p.accent };
        ui.p().rounded(b, 6.0, if on { col.alpha(0.22) } else { p.grid });
        ui.p().rounded_border(b, 6.0, if on { 2.0 } else { 1.0 }, if on { col } else { p.line });
        let badge = Vec2::new(b.center().x, b.y + bh * 0.34);
        ui.p().circle(badge, 17.0, col);
        ui.icon(icon, badge, 19.0, Color::WHITE);
        ui.text_in(name, Rect::new(b.x + 2.0, b.y + bh * 0.62, b.w - 4.0, 18.0), 12.0, Weight::Bold, p.text, Align::Center);
        if !save {
            ui.text_as_is(&format!("{}", k + 1), Vec2::new(b.center().x, b.y + bh * 0.62 + 32.0), 11.0, Weight::Bold, p.dim, Align::Center);
        }
        if k < 4 {
            let ax = b.right() + gap * 0.5;
            let ay = b.center().y;
            ui.p().rect(Rect::new(ax - 7.0, ay - 1.0, 10.0, 2.0), p.dim);
            ui.p().convex(&[Vec2::new(ax + 2.0, ay - 5.0), Vec2::new(ax + 8.0, ay), Vec2::new(ax + 2.0, ay + 5.0)], p.dim);
        }
    }
}

/// The symbols of the Gantt, each with its name: two rows of four.
fn symbols_picture(ui: &mut Ui, p: &Pal, r: Rect) {
    let inner = r.inset(14.0);
    let cw = inner.w / 4.0;
    let rh = inner.h / 2.0;
    for k in 0..8 {
        let cell = Rect::new(inner.x + (k % 4) as f32 * cw, inner.y + (k / 4) as f32 * rh, cw, rh);
        let d = Rect::new(cell.x + 10.0, cell.y + rh * 0.18, cw - 20.0, 30.0);
        let cy = d.center().y;
        let (a, b) = (d.x + 6.0, d.right() - 6.0);
        match k {
            0 => gantt::pull(ui, p, a + 10.0, b - 10.0, cy, false),
            1 => {
                gantt::t_bar(ui, p, a + 6.0, a + 30.0, cy, true);
                arrow_chip(ui, Rect::new(a + 32.0, cy - CHIP * 0.5, b - a - 38.0, CHIP), PINK, true);
            }
            2 => gantt::chip(ui, Rect::new(a + 4.0, cy - CHIP * 0.5, b - a - 8.0, CHIP), BLUE, true, "1", None, false, false, 1.0),
            3 => gantt::deadhead(ui, p, a + 14.0, b - 14.0, cy, true),
            4 => {
                gantt::relief_arrow(ui, p, d.center().x - 8.0, cy, false, true);
                gantt::relief_arrow(ui, p, d.center().x + 8.0, cy, true, false);
            }
            5 => symbol(ui, p, Sym::Split, d),
            6 => {
                let br = Rect::new(d.center().x - 12.0, cy - CHIP * 0.45, 24.0, CHIP * 0.9);
                ui.p().rounded(br, 3.0, p.dead);
                ui.p().rounded_border(br, 3.0, 1.3, p.warn);
                ui.text_as_is("W", Vec2::new(br.center().x, cy + 4.0), 10.5, Weight::Bold, p.warn, Align::Center);
            }
            _ => {
                ui.text_as_is("!", Vec2::new(d.center().x, cy + 6.0), 18.0, Weight::Black, p.err, Align::Center);
            }
        }
        ui.text_in(PICTURE_WORDS[k], Rect::new(cell.x + 2.0, d.bottom() + 6.0, cw - 4.0, 16.0), 11.0, Weight::Medium, p.dim, Align::Center);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_at(view: usize) -> Probe {
        Probe { view, loaded: true, blocks: 4, duties: 6, issues: 2, saved_same: true, ..Default::default() }
    }

    /// The steps of `id` gone through on the probes given, one after the other: how many tick
    /// off by themselves (a step to read never does).
    fn run(id: LessonId, probes: &[Probe]) -> usize {
        let steps = id.steps();
        let mut at = 0;
        let mut start = probes[0].clone();
        for p in probes {
            while at < steps.len() && done(steps[at].check, p, &start) {
                at += 1;
                start = p.clone();
            }
            if at < steps.len() && steps[at].check == Check::Info {
                at += 1;
                start = p.clone();
            }
        }
        at
    }

    #[test]
    fn moving_a_trip_ticks_off_as_the_player_does_it() {
        let a = probe_at(super::super::rosterflow::ROSTER);
        let vehicles = Probe { view: VEHICLES, ..a.clone() };
        let holding = Probe { holding_trips: true, ..vehicles.clone() };
        let over = Probe { zone_hover: true, ..holding.clone() };
        let dropped = Probe { edits: 1, undo: 1, draft: true, ..vehicles.clone() };
        let undone = Probe { undo: 0, redo: 1, ..dropped.clone() };
        // nothing done: nothing ticks
        assert_eq!(run(LessonId::MoveTrip, &[a.clone(), a.clone()]), 0);
        // half way: open, picked up - not yet dropped
        assert_eq!(run(LessonId::MoveTrip, &[a.clone(), vehicles.clone(), holding.clone()]), 2);
        // the whole lesson (the figures' step is one to read)
        assert_eq!(run(LessonId::MoveTrip, &[a, vehicles, holding, over, dropped, undone]), MOVE_TRIP.len());
    }

    #[test]
    fn cutting_and_joining_counts_the_duties() {
        let v = probe_at(VEHICLES);
        let cut = Probe { duties: 7, edits: 1, ..v.clone() };
        let drivers = Probe { view: DRIVERS, ..cut.clone() };
        let back = Probe { view: VEHICLES, ..cut.clone() };
        let joined = Probe { duties: 6, edits: 2, ..back.clone() };
        assert_eq!(run(LessonId::CutJoin, &[v.clone(), cut.clone(), drivers.clone(), back.clone()]), 4);
        assert_eq!(run(LessonId::CutJoin, &[v, cut, drivers, back, joined]), CUT_JOIN.len());
        // a day not made yet is no duty more or less
        assert!(!done(Check::DutiesUp, &Probe { loaded: false, duties: 9, ..Default::default() }, &probe_at(VEHICLES)));
    }

    #[test]
    fn save_and_discard_are_told_apart() {
        let s = probe_at(VEHICLES);
        let changed = Probe { draft: true, ..s.clone() };
        let discarded = Probe { draft: false, saved_same: true, ..s.clone() };
        let saved = Probe { draft: false, saved_same: false, ..s.clone() };
        assert!(done(Check::Discarded, &discarded, &changed) && !done(Check::Saved, &discarded, &changed));
        assert!(done(Check::Saved, &saved, &changed) && !done(Check::Discarded, &saved, &changed));
        assert_eq!(run(LessonId::SaveDiscard, &[s, changed.clone(), discarded, changed, saved]), SAVE_DISCARD.len());
    }

    #[test]
    fn the_timeplan_lesson_follows_the_timeplan() {
        let tp = |screen: usize, changes: u64, trips: usize, dirty: bool, unsynced: bool| Probe { view: TIMEPLAN, tp: TpProbe { ready: true, screen, changes, trips, dirty, unsynced, why: None }, ..probe_at(TIMEPLAN) };
        let probes = [probe_at(VEHICLES), tp(0, 0, 10, false, false), tp(0, 1, 10, true, false), tp(1, 1, 10, true, false), tp(1, 2, 14, true, false), tp(1, 2, 14, false, true), tp(1, 2, 14, false, false)];
        assert_eq!(run(LessonId::Timeplan, &probes), TIMEPLAN_STEPS.len());
        // saved without having changed anything does not count as saving the lesson's change
        assert!(!done(Check::TpSaved, &tp(1, 0, 10, false, true), &tp(1, 0, 10, false, true)));
    }

    #[test]
    fn a_lesson_says_what_it_needs_first() {
        let p = probe_at(VEHICLES);
        assert_eq!(needs(LessonId::MoveTrip, &p), Some(None));
        // not known before the day is made
        assert_eq!(needs(LessonId::MoveTrip, &Probe { loaded: false, ..p.clone() }), None);
        assert_eq!(needs(LessonId::CutJoin, &Probe { blocks: 0, ..p.clone() }), Some(Some(NO_TRIPS)));
        assert_eq!(needs(LessonId::MoveTrip, &Probe { blocks: 1, ..p.clone() }), Some(Some(ONE_BUS)));
        assert_eq!(needs(LessonId::FixIssue, &Probe { issues: 0, ..p.clone() }), Some(Some(NOTHING_RED)));
        // the timeplan: once it is read, a line of the map's not edited says so
        assert_eq!(needs(LessonId::Timeplan, &p), None);
        let map_line = Probe { tp: TpProbe { why: Some("A line of the map's own timetable"), ..Default::default() }, ..p.clone() };
        assert_eq!(needs(LessonId::Timeplan, &map_line), Some(Some("A line of the map's own timetable")));
        assert_eq!(needs(LessonId::Timeplan, &Probe { tp: TpProbe { ready: true, ..Default::default() }, ..p }), Some(None));
    }

    #[test]
    fn every_lesson_has_steps_and_its_own_id() {
        let mut ids: Vec<&str> = LESSONS.iter().map(|l| l.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), LESSONS.len());
        for l in LESSONS {
            assert!(l.steps().len() >= 3, "{l:?}");
            // (the last step is one the player does, so a lesson is never done by reading)
            assert_ne!(l.steps().last().map(|s| s.check), Some(Check::Info), "{l:?}");
        }
    }

    /// Every text of the tutorial in RosterFlow's look.
    fn texts() -> Vec<&'static str> {
        let mut v: Vec<&'static str> = Vec::new();
        for l in LESSONS {
            v.push(l.title());
            for s in l.steps() {
                v.push(s.goal);
                v.push(s.hint);
            }
        }
        v.extend([NO_TRIPS, ONE_BUS, NOTHING_RED]);
        for (_, name, text) in CHAIN {
            v.extend([name, text]);
        }
        for (_, term, text) in GLOSSARY {
            v.extend([term, text]);
        }
        for (keys, words, what) in SHORTCUTS {
            if words {
                v.push(keys);
            }
            v.push(what);
        }
        v.extend(RED);
        v.extend(SHEET_WORDS);
        v.extend(PICTURE_WORDS);
        v.extend([
            "Guided tour",
            "Try it yourself",
            "Help",
            "Take the tour",
            "Go on with the tour",
            "End the lesson?",
            "Keep what you changed, or put everything back as it was before the lesson.",
            "Well done!",
            "You did every step of this lesson. Keep what you changed, or put it all back as it was.",
            "First this",
            "The lesson is over: your changes stay.",
            "The lesson is undone: the planning is as it was before.",
            "The planning is as it was, but the timetable could not be put back: change it back in the Timeplan tab.",
            "End lesson",
            "Got it",
            "Go on with the lesson",
            "Undo the lesson",
            "Keep my changes",
            "Close",
            "Tutorial: a guided tour, lessons to try yourself and a help sheet",
        ]);
        v
    }

    #[test]
    fn the_lessons_and_the_help_sheet_are_translated() {
        let mut missing = Vec::new();
        for t in texts() {
            for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
                if crate::_rust_i18n_try_translate(lang, t).is_none() {
                    missing.push(format!("{lang}: {t}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn every_lessons_part_is_drawn_under_its_name() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/company/planning");
        let src: String = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "rs")).map(|e| std::fs::read_to_string(e.path()).unwrap()).collect();
        for l in LESSONS {
            for s in l.steps().iter().filter(|s| !s.target.is_empty()) {
                let named = s.target.split('|').all(|t| src.contains(&format!("anchor(\"{t}\"")) || (t.starts_with("rf-tab-") && src.contains(&format!("\"{t}\""))));
                assert!(named, "{l:?}: {} is drawn nowhere", s.target);
            }
        }
    }
}
