//! RosterFlow, the bus company's scheduling studio (Luc: the planning should look and work
//! "exactly" like Optibus's scheduling module, under a name of its own): what it adds to the
//! blocks and duties of `studio` - its preferences (the rules the duties are checked against,
//! hard or soft with a penalty, and what Optimise works to), the locks, links and duty notes
//! of each weekday, the duties' shape (sign-on and sign-off, travel to a relief point, relief
//! timing, breaks, duty types, paid time), the issues it validates, its figures, the draft the
//! player edits until he saves it, and Optimise itself.
//!
//! What the game runs is what is saved: the draft (`Flow::draft`) is the studio's alone until
//! "Save" (`save_draft`) - the roster, the day's close and the game keep the saved blocks and
//! duties meanwhile. Relief points stay the line's termini and the depot (Luc's choice), as in
//! `studio`.

use super::economy;
use super::model::{BusKind, BusSize, Cents, Company, Drive};
use super::network::{self, PlannedTrip, TourOfDay};
use super::staff::{self, Block, MIN_PAID};
use super::studio::{self as st, Duty, JoinedDuty, LineLayout, PieceRef, TripRef};
use crate::LineInfo;
use serde::{Deserialize, Serialize};
use std::ops::Range;

// --- the lines' colours ------------------------------------------------------------------------

/// The lines' colours where a line has none of its own: saturated, as the chips in Optibus.
pub const PALETTE: [[u8; 3]; 10] = [[79, 129, 190], [196, 96, 214], [38, 166, 154], [245, 128, 40], [130, 110, 232], [250, 196, 60], [232, 82, 140], [32, 170, 210], [96, 186, 76], [222, 78, 64]];

/// A line's colour (`#rrggbb`): its own, else one of the palette by its place among the
/// company's lines. The studio's chips, the plates and the fleet map all take it from here.
pub fn line_colour(c: &Company, line: &str) -> [u8; 3] {
    let k = c.lines.iter().position(|x| x.name.eq_ignore_ascii_case(line.trim())).unwrap_or(0);
    c.lines.get(k).and_then(|l| parse_hex(&l.colour)).unwrap_or(PALETTE[k % PALETTE.len()])
}

/// `#rrggbb` as its bytes.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Give line `line` its own colour (`None`: back to the palette's).
pub fn set_line_colour(c: &mut Company, line: &str, colour: Option<[u8; 3]>) -> bool {
    let Some(l) = c.lines.iter_mut().find(|x| x.name.eq_ignore_ascii_case(line.trim())) else { return false };
    l.colour = colour.map(hex).unwrap_or_default();
    true
}

// --- the preferences -----------------------------------------------------------------------------

/// A rule of the preferences: on or not, its value, and whether it is a hard rule (never
/// broken by Optimise, an error when broken by hand) or a soft one that costs `penalty`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Limit {
    pub on: bool,
    pub value: i32,
    pub hard: bool,
    pub penalty: Cents,
}

impl Default for Limit {
    fn default() -> Self {
        Limit { on: true, value: 0, hard: true, penalty: 0 }
    }
}

impl Limit {
    const fn hard(value: i32) -> Limit {
        Limit { on: true, value, hard: true, penalty: 0 }
    }

    const fn soft(value: i32, penalty: Cents) -> Limit {
        Limit { on: true, value, hard: false, penalty }
    }

    const fn off(value: i32) -> Limit {
        Limit { on: false, value, hard: false, penalty: 0 }
    }
}

/// When the driver changes within the bus's layover at a relief point.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReliefTiming {
    /// The old driver gets off on arrival, the new one on at departure: the bus stands alone.
    #[default]
    ArrivalDeparture,
    OnArrival,
    OnDeparture,
    MidLayover,
    /// A set time before the departure (`Prefs::relief_before`).
    BeforeDeparture,
}

impl ReliefTiming {
    pub const ALL: [ReliefTiming; 5] = [ReliefTiming::ArrivalDeparture, ReliefTiming::OnArrival, ReliefTiming::OnDeparture, ReliefTiming::MidLayover, ReliefTiming::BeforeDeparture];
    pub const LABELS: [&'static str; 5] = ["On arrival and on departure (unattended)", "On arrival", "On departure", "Mid-layover", "Before departure"];

    pub fn label(self) -> &'static str {
        Self::LABELS[Self::ALL.iter().position(|x| *x == self).unwrap_or(0)]
    }
}

/// How a driver gets to a relief point at another place.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Travel {
    #[default]
    Walk,
    Public,
    Car,
}

impl Travel {
    pub const ALL: [Travel; 3] = [Travel::Walk, Travel::Public, Travel::Car];
    pub const LABELS: [&'static str; 3] = ["Walk", "Public transport", "Staff car"];

    /// The letter on its box in the Gantt (Optibus's W, P, R).
    pub fn letter(self) -> &'static str {
        match self {
            Travel::Walk => "W",
            Travel::Public => "P",
            Travel::Car => "R",
        }
    }

    pub fn label(self) -> &'static str {
        Self::LABELS[Self::ALL.iter().position(|x| *x == self).unwrap_or(0)]
    }

    /// Its minutes as the preferences have them.
    pub fn minutes(self, p: &Prefs) -> i32 {
        match self {
            Travel::Walk => p.walk,
            Travel::Public => p.public,
            Travel::Car => p.car,
        }
    }
}

/// No, yes, or either.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tri {
    No,
    Yes,
    #[default]
    Either,
}

impl Tri {
    pub const ALL: [Tri; 3] = [Tri::No, Tri::Yes, Tri::Either];
    pub const LABELS: [&'static str; 3] = ["No", "Yes", "Either"];

    fn allows(self, v: bool) -> bool {
        match self {
            Tri::No => !v,
            Tri::Yes => v,
            Tri::Either => true,
        }
    }
}

/// A kind of duty the company works (Optibus's duty types): the first whose windows a duty
/// fits is its type; a duty no allowed type fits is "Other", an issue.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct DutyType {
    pub name: String,
    /// The sign-on between these minutes of the day.
    pub start_from: i32,
    pub start_to: i32,
    /// The sign-off at the latest (minutes; past 1440 the next day).
    pub end_by: i32,
    /// The spread (sign-on to sign-off) at most.
    pub max_length: i32,
    pub split: Tri,
    pub allowed: bool,
    pub penalty: Cents,
}

impl Default for DutyType {
    fn default() -> Self {
        DutyType { name: String::new(), start_from: 0, start_to: 1439, end_by: 1800, max_length: 600, split: Tri::Either, allowed: true, penalty: 0 }
    }
}

impl DutyType {
    fn new(name: &str, start: (i32, i32), end_by: i32, max_length: i32, split: Tri) -> DutyType {
        DutyType { name: name.into(), start_from: start.0, start_to: start.1, end_by, max_length, split, allowed: true, penalty: 0 }
    }

    /// Whether a duty from `start` to `end` (split or not) is of this type.
    pub fn fits(&self, start: i32, end: i32, split: bool) -> bool {
        start >= self.start_from && start <= self.start_to && end <= self.end_by && end - start <= self.max_length && self.split.allows(split)
    }
}

/// The name a duty no type fits goes by.
pub const OTHER: &str = "Other";
/// The default duty types' names (translated when shown).
pub const TYPE_NAMES: [&str; 5] = ["Early shift", "Middle shift", "Late shift", "Split shift", "Night shift"];

fn default_types() -> Vec<DutyType> {
    vec![
        DutyType::new(TYPE_NAMES[0], (180, 479), 960, 600, Tri::No),
        DutyType::new(TYPE_NAMES[1], (480, 719), 1200, 600, Tri::No),
        DutyType::new(TYPE_NAMES[2], (720, 1080), 1620, 600, Tri::No),
        DutyType::new(TYPE_NAMES[3], (180, 780), 1380, 780, Tri::Yes),
        DutyType::new(TYPE_NAMES[4], (1081, 1439), 1800, 600, Tri::Either),
    ]
}

/// RosterFlow's preferences (its Preferences window: VEHICLES, DRIVERS, DEPOTS, MISC): what
/// the duties and blocks are checked against and what Optimise works to. Minutes and cents.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Prefs {
    // VEHICLES
    /// The least layover between two trips of a bus at one place.
    pub layover: Limit,
    /// The bus's check before it leaves the depot and after it is back.
    pub pre_trip: i32,
    pub post_trip: i32,
    // DEPOTS
    /// Buses out at once at most (the depot's capacity).
    pub depot_capacity: Limit,
    // DRIVERS
    /// Driver signing: at the duty's start and end, and around a split.
    pub sign_on: i32,
    pub sign_off: i32,
    pub sign_on_split: i32,
    pub sign_off_split: i32,
    pub relief: ReliefTiming,
    pub relief_before: i32,
    /// Duty travel: how drivers get to a relief point elsewhere, and how long it takes.
    pub travel: Travel,
    pub walk: i32,
    pub public: i32,
    pub car: i32,
    /// Duty breaks: a duty working more than `meal_after` needs a break of `meal.value`.
    pub meal: Limit,
    pub meal_after: i32,
    /// Continuous break: at most `continuous.value` without a break of `break_min`.
    pub continuous: Limit,
    pub break_min: i32,
    /// A layover inside a piece counts as a break (Optibus's "attendance counts as break").
    pub layover_breaks: bool,
    /// Time limitation: the spread (sign-on to sign-off) at most.
    pub max_spread: Limit,
    /// Pieces of work at least this long.
    pub min_piece: Limit,
    /// Changes of bus in a duty at most.
    pub changeovers: Limit,
    /// A break between two pieces this long or longer is a split (unpaid).
    pub split_min: i32,
    pub duty_types: Vec<DutyType>,
    // MISC: costs (None: what the company pays, as the studio reckons it)
    /// A driver's hour (cents).
    pub wage: Option<Cents>,
    /// What each duty costs besides its hours (None: three hours' wage).
    pub driver_fixed: Option<Cents>,
    /// What each bus out costs a day, and each kilometre.
    pub vehicle_fixed: Option<Cents>,
    pub per_km: Option<Cents>,
    // the optimiser's settings
    pub optimise_vehicles: bool,
    pub optimise_duties: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            layover: Limit::soft(3, 20_00),
            pre_trip: 5,
            post_trip: 5,
            depot_capacity: Limit::off(20),
            sign_on: 10,
            sign_off: 5,
            sign_on_split: 5,
            sign_off_split: 5,
            relief: ReliefTiming::ArrivalDeparture,
            relief_before: 5,
            travel: Travel::Walk,
            walk: 5,
            public: 15,
            car: 10,
            meal: Limit::hard(30),
            meal_after: 360,
            continuous: Limit::hard(270),
            break_min: 15,
            layover_breaks: true,
            max_spread: Limit::soft(780, 50_00),
            min_piece: Limit::soft(60, 10_00),
            changeovers: Limit::soft(2, 15_00),
            split_min: st::SPLIT_BREAK,
            duty_types: default_types(),
            wage: None,
            driver_fixed: None,
            vehicle_fixed: None,
            per_km: None,
            optimise_vehicles: true,
            optimise_duties: true,
        }
    }
}

// --- what is kept ------------------------------------------------------------------------------

/// A bus's block kept as it is (Optimise leaves it, no edit changes it).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct BlockLock {
    pub weekday: u8,
    pub line: String,
    pub tour: String,
}

/// Trips that stay together on one bus (striped in the Gantt): moved together, kept
/// together by Optimise.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Link {
    pub weekday: u8,
    pub line: String,
    pub trips: Vec<TripRef>,
}

/// Work in a duty that is no trip (Optibus's custom work): a spare turn, office hours.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Custom {
    pub kind: String,
    pub from: i32,
    pub to: i32,
    #[serde(default)]
    pub place: String,
}

/// The kinds of custom work offered (translated when shown).
pub const CUSTOM_KINDS: [&str; 4] = ["Spare", "Office", "Union", "Training"];

/// What the player set of a duty, by its first piece: its type, its lock, how its driver
/// travels between pieces, its custom work, its signing and its paid time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DutyNote {
    pub weekday: u8,
    pub key: PieceRef,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub locked: bool,
    /// The way between piece `n` and the next (by `n`).
    #[serde(default)]
    pub travel: Vec<(u32, Travel)>,
    #[serde(default)]
    pub custom: Vec<Custom>,
    #[serde(default)]
    pub paid: Option<i32>,
    #[serde(default)]
    pub sign_on: Option<i32>,
    #[serde(default)]
    pub sign_off: Option<i32>,
}

impl DutyNote {
    fn is_blank(&self) -> bool {
        self.kind.is_none() && !self.locked && self.travel.is_empty() && self.custom.is_empty() && self.paid.is_none() && self.sign_on.is_none() && self.sign_off.is_none()
    }
}

/// A duty of custom work only (a spare driver's day), begun from scratch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Spare {
    pub weekday: u8,
    pub id: u32,
    pub custom: Vec<Custom>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub paid: Option<i32>,
}

/// The studio's own of each weekday besides the blocks and joined duties.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Sheet {
    pub locks: Vec<BlockLock>,
    pub links: Vec<Link>,
    pub notes: Vec<DutyNote>,
    pub spares: Vec<Spare>,
    /// Issues dismissed: (weekday, the issue's key).
    pub dismissed: Vec<(u8, String)>,
}

impl Sheet {
    pub fn is_empty(&self) -> bool {
        self.locks.is_empty() && self.links.is_empty() && self.notes.is_empty() && self.spares.is_empty() && self.dismissed.is_empty()
    }
}

/// What the player changed and has not saved yet: the blocks, the joined duties and the sheet
/// as the studio shows them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Draft {
    pub layouts: Vec<LineLayout>,
    pub joined: Vec<JoinedDuty>,
    pub sheet: Sheet,
}

/// RosterFlow's part of the company's planning.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Flow {
    pub prefs: Prefs,
    pub sheet: Sheet,
    pub draft: Option<Draft>,
}

impl Flow {
    /// Nothing but the defaults (left out of the saved company).
    pub fn is_empty(&self) -> bool {
        self.prefs == Prefs::default() && self.sheet.is_empty() && self.draft.is_none()
    }
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// "7:05" (minutes of the day; past midnight on, as the service day runs).
pub fn hm(m: i32) -> String {
    let m = m.max(0);
    format!("{}:{:02}", m / 60, m % 60)
}

/// "07:05" on the clock.
pub fn clock(m: i32) -> String {
    let m = m.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

// --- the draft -------------------------------------------------------------------------------

/// The company as the studio shows it: the draft's blocks, duties and sheet in place of the
/// saved ones (the company itself where there is no draft).
pub fn view(c: &Company) -> Company {
    let mut v = c.clone();
    if let Some(d) = c.planning.flow.draft.clone() {
        v.planning.layouts = d.layouts;
        v.planning.joined = d.joined;
        v.planning.flow.sheet = d.sheet;
    }
    v.planning.flow.draft = None;
    v
}

/// Whether there are changes not saved.
pub fn has_draft(c: &Company) -> bool {
    c.planning.flow.draft.as_ref().is_some_and(|d| d.layouts != c.planning.layouts || d.joined != c.planning.joined || d.sheet != c.planning.flow.sheet)
}

/// What the view (`view`, edited) holds as the company's draft.
pub fn keep_draft(c: &mut Company, view: &Company) {
    let d = Draft { layouts: view.planning.layouts.clone(), joined: view.planning.joined.clone(), sheet: view.planning.flow.sheet.clone() };
    let same_as_saved = d.layouts == c.planning.layouts && d.joined == c.planning.joined && d.sheet == c.planning.flow.sheet;
    c.planning.flow.draft = if same_as_saved { None } else { Some(d) };
}

/// The draft as it is now (for "Undo": the state to go back to).
pub fn draft_of(c: &Company) -> Draft {
    c.planning.flow.draft.clone().unwrap_or_else(|| Draft { layouts: c.planning.layouts.clone(), joined: c.planning.joined.clone(), sheet: c.planning.flow.sheet.clone() })
}

/// Put a draft back (Undo, Redo, the history).
pub fn set_draft(c: &mut Company, d: Draft) {
    let mut v = view(c);
    v.planning.layouts = d.layouts;
    v.planning.joined = d.joined;
    v.planning.flow.sheet = d.sheet;
    keep_draft(c, &v);
}

/// "Save": the draft becomes what the company runs. Returns whether there was one.
pub fn save_draft(c: &mut Company) -> bool {
    let Some(d) = c.planning.flow.draft.take() else { return false };
    c.planning.layouts = d.layouts;
    c.planning.joined = d.joined;
    c.planning.flow.sheet = d.sheet;
    true
}

/// "Discard": the draft forgotten, back to what was saved.
pub fn discard_draft(c: &mut Company) -> bool {
    c.planning.flow.draft.take().is_some()
}

// --- locks, links and notes ---------------------------------------------------------------------

/// Whether the bus's block (`line`, `tour`) is locked on `weekday`.
pub fn block_locked(c: &Company, weekday: u8, line: &str, tour: &str) -> bool {
    c.planning.flow.sheet.locks.iter().any(|l| l.weekday == weekday && same(&l.line, line) && same(&l.tour, tour))
}

/// Lock the block or unlock it.
pub fn set_block_lock(c: &mut Company, weekday: u8, line: &str, tour: &str, on: bool) {
    let locks = &mut c.planning.flow.sheet.locks;
    locks.retain(|l| !(l.weekday == weekday && same(&l.line, line) && same(&l.tour, tour)));
    if on {
        locks.push(BlockLock { weekday, line: line.trim().to_string(), tour: tour.trim().to_string() });
    }
}

/// The key a duty is known by: its first piece.
pub fn duty_key(tours: &[TourOfDay], d: &Duty) -> Option<PieceRef> {
    let p = d.pieces.first()?;
    st::piece_ref(tours, p.tour, p.duty)
}

fn note<'a>(c: &'a Company, weekday: u8, key: &PieceRef) -> Option<&'a DutyNote> {
    c.planning.flow.sheet.notes.iter().find(|n| n.weekday == weekday && n.key.is(key))
}

/// The note of a duty to change (made when it has none); blank notes are dropped again by
/// `tidy_notes`.
pub fn note_mut<'a>(c: &'a mut Company, weekday: u8, key: &PieceRef) -> &'a mut DutyNote {
    let notes = &mut c.planning.flow.sheet.notes;
    match notes.iter().position(|n| n.weekday == weekday && n.key.is(key)) {
        Some(i) => &mut notes[i],
        None => {
            notes.push(DutyNote { weekday, key: key.clone(), ..Default::default() });
            notes.last_mut().unwrap()
        }
    }
}

/// Drop the notes that say nothing.
pub fn tidy_notes(c: &mut Company) {
    c.planning.flow.sheet.notes.retain(|n| !n.is_blank());
}

/// Whether the duty with key `key` is locked.
pub fn duty_locked(c: &Company, weekday: u8, key: &PieceRef) -> bool {
    note(c, weekday, key).is_some_and(|n| n.locked)
}

/// Whether the duty piece (block `i`, duty `k`) is in is locked.
pub fn piece_locked(c: &Company, weekday: u8, tours: &[TourOfDay], i: usize, k: usize) -> bool {
    if !c.planning.flow.sheet.notes.iter().any(|n| n.weekday == weekday && n.locked) {
        return false;
    }
    let duties = st::duties(&c.planning.joined, weekday, tours);
    let Some(d) = st::duty_of(&duties, i, k) else { return false };
    duty_key(tours, &duties[d]).is_some_and(|key| duty_locked(c, weekday, &key))
}

/// Lock a duty or unlock it.
pub fn set_duty_lock(c: &mut Company, weekday: u8, key: &PieceRef, on: bool) {
    note_mut(c, weekday, key).locked = on;
    tidy_notes(c);
}

/// The trips linked with `trip` of line `line` on `weekday` (itself among them), none if it is
/// in no link.
pub fn linked(c: &Company, weekday: u8, line: &str, trip: &TripRef) -> Vec<TripRef> {
    c.planning.flow.sheet.links.iter().find(|l| l.weekday == weekday && same(&l.line, line) && l.trips.contains(trip)).map(|l| l.trips.clone()).unwrap_or_default()
}

/// Why trips may not be linked.
pub const LINK_ONE_BUS: &str = "Only trips of one bus can be linked.";
pub const LINK_TWO: &str = "Pick two trips or more to link.";

/// Link the trips `picked` of block `i` (consecutive on that bus): they stay together.
/// Links they were in are merged into the new one.
pub fn link(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, picked: &[usize]) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(st::NOT_FOUND)?;
    let mut refs: Vec<TripRef> = picked.iter().filter_map(|&k| t.trips.get(k)).filter(|x| x.counts() && !x.origin.trim().is_empty()).map(TripRef::of).collect();
    if refs.len() < 2 {
        return Err(LINK_TWO);
    }
    let links = &mut c.planning.flow.sheet.links;
    let mut merged: Vec<TripRef> = Vec::new();
    links.retain(|l| {
        let hit = l.weekday == weekday && same(&l.line, &t.line) && l.trips.iter().any(|r| refs.contains(r));
        if hit {
            merged.extend(l.trips.iter().cloned());
        }
        !hit
    });
    refs.extend(merged);
    refs.sort_by_key(|r| r.dep);
    refs.dedup();
    if !refs.iter().all(|r| t.trips.iter().any(|x| r.is(x))) {
        return Err(LINK_ONE_BUS);
    }
    links.push(Link { weekday, line: t.line.clone(), trips: refs });
    Ok(())
}

/// Take the link of trip `k` of block `i` apart.
pub fn unlink(c: &mut Company, weekday: u8, tours: &[TourOfDay], i: usize, k: usize) -> bool {
    let Some(x) = tours.get(i).and_then(|t| t.trips.get(k)) else { return false };
    let r = TripRef::of(x);
    let line = tours[i].line.clone();
    let before = c.planning.flow.sheet.links.len();
    c.planning.flow.sheet.links.retain(|l| !(l.weekday == weekday && same(&l.line, &line) && l.trips.contains(&r)));
    c.planning.flow.sheet.links.len() != before
}

/// A new duty of custom work only on `weekday` (a spare turn from `from` to `to`). Returns
/// its number.
pub fn add_spare(c: &mut Company, weekday: u8, kind: &str, from: i32, to: i32) -> u32 {
    let id = c.planning.flow.sheet.spares.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    c.planning.flow.sheet.spares.push(Spare { weekday, id, custom: vec![Custom { kind: kind.into(), from, to: to.max(from + 5), place: String::new() }], ..Default::default() });
    id
}

pub fn remove_spare(c: &mut Company, weekday: u8, id: u32) -> bool {
    let before = c.planning.flow.sheet.spares.len();
    c.planning.flow.sheet.spares.retain(|s| !(s.weekday == weekday && s.id == id));
    before != c.planning.flow.sheet.spares.len()
}

/// Dismiss an issue (its key) or bring it back.
pub fn set_dismissed(c: &mut Company, weekday: u8, key: &str, on: bool) {
    let d = &mut c.planning.flow.sheet.dismissed;
    d.retain(|(w, k)| !(*w == weekday && k == key));
    if on {
        d.push((weekday, key.to_string()));
    }
}

pub fn dismissed(c: &Company, weekday: u8, key: &str) -> bool {
    c.planning.flow.sheet.dismissed.iter().any(|(w, k)| *w == weekday && k == key)
}

/// A line renamed: its locks and links follow it.
pub fn rename_line(c: &mut Company, old: &str, new: &str) {
    for l in c.planning.flow.sheet.locks.iter_mut().filter(|l| same(&l.line, old)) {
        l.line = new.to_string();
    }
    for l in c.planning.flow.sheet.links.iter_mut().filter(|l| same(&l.line, old)) {
        l.line = new.to_string();
    }
}

// --- a lesson's snapshot ---------------------------------------------------------------------------

/// What one of RosterFlow's lessons ("Try it") may change in the company, kept before the
/// lesson begins so that "Undo the lesson" puts it all back: the saved blocks and joined
/// duties, RosterFlow's own (its preferences, its sheet and the draft), and the lines' colours.
/// The roster of drivers and buses, the money and the clock are not a lesson's and stay as
/// they are.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    pub layouts: Vec<LineLayout>,
    pub joined: Vec<JoinedDuty>,
    pub flow: Flow,
    /// Each line's own colour (its name, `#rrggbb` or empty for the palette's).
    pub colours: Vec<(String, String)>,
}

impl Snapshot {
    /// The company as a lesson finds it.
    pub fn take(c: &Company) -> Snapshot {
        Snapshot { layouts: c.planning.layouts.clone(), joined: c.planning.joined.clone(), flow: c.planning.flow.clone(), colours: c.lines.iter().map(|l| (l.name.clone(), l.colour.clone())).collect() }
    }

    /// Put the company back as the lesson found it (a line added since keeps its colour).
    /// Returns whether anything changed.
    pub fn restore(&self, c: &mut Company) -> bool {
        let before = Snapshot::take(c);
        c.planning.layouts = self.layouts.clone();
        c.planning.joined = self.joined.clone();
        c.planning.flow = self.flow.clone();
        for l in c.lines.iter_mut() {
            if let Some((_, colour)) = self.colours.iter().find(|(n, _)| same(n, &l.name)) {
                l.colour = colour.clone();
            }
        }
        Snapshot::take(c) != before
    }

    /// The saved planning (not the draft) is as it was: nothing new was saved meanwhile.
    pub fn same_saved(&self, c: &Company) -> bool {
        self.layouts == c.planning.layouts && self.joined == c.planning.joined && self.flow.sheet == c.planning.flow.sheet
    }
}

// --- the stack -------------------------------------------------------------------------------

/// The trips on the stack of `weekday` (taken off every bus): their line, and the trip as the
/// timetable has it.
pub fn stack(c: &Company, lines: &[LineInfo], weekday: u8) -> Vec<(String, PlannedTrip)> {
    let refs: Vec<(&str, &TripRef)> = c.planning.layouts.iter().filter(|l| l.weekday == weekday).flat_map(|l| l.stack.iter().map(move |r| (l.line.as_str(), r))).collect();
    if refs.is_empty() {
        return Vec::new();
    }
    let raw = network::timetable_tours(c, lines);
    let mut out: Vec<(String, PlannedTrip)> = refs.iter().filter_map(|(line, r)| raw.iter().filter(|t| same(&t.line, line)).flat_map(|t| t.trips.iter()).find(|x| r.is(x)).map(|x| (line.to_string(), x.clone()))).collect();
    out.sort_by_key(|(_, x)| x.dep);
    out
}

// --- the duties' shape -------------------------------------------------------------------------

/// What a duty does besides its trips, as the Gantt draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EvKind {
    SignOn,
    SignOff,
    Travel(Travel),
    /// Custom work: its index in the duty's note.
    Custom(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ev {
    pub kind: EvKind,
    pub from: i32,
    pub to: i32,
}

/// A piece of a duty as worked: its block and duty there, and the minutes the driver has it
/// (relief timing and the pre- and post-trip checks in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub tour: usize,
    pub duty: usize,
    pub from: i32,
    pub to: i32,
}

/// A driver's duty in full: its pieces as worked, its events, its times and type.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shaped {
    /// Its duty in `Board::duties` (None: a spare duty of custom work only).
    pub duty: Option<usize>,
    pub spare: Option<u32>,
    pub key: Option<PieceRef>,
    pub spans: Vec<Span>,
    pub events: Vec<Ev>,
    pub custom: Vec<Custom>,
    pub start: i32,
    pub end: i32,
    pub work: i32,
    pub paid: i32,
    pub driving: i32,
    pub platform: i32,
    pub splits: Vec<(i32, i32)>,
    pub changeovers: usize,
    /// Its type (an index into the preferences' types; None: "Other").
    pub kind: Option<usize>,
    /// Its type set by hand.
    pub kind_set: bool,
    pub locked: bool,
    /// Its issues: indices into `Board::issues`.
    pub issues: Vec<usize>,
}

impl Shaped {
    pub fn spread(&self) -> i32 {
        self.end - self.start
    }

    pub fn split(&self) -> bool {
        !self.splits.is_empty()
    }

    /// Its type's name (English, translated when shown).
    pub fn kind_name<'a>(&self, p: &'a Prefs) -> &'a str {
        self.kind.and_then(|k| p.duty_types.get(k)).map(|t| t.name.as_str()).unwrap_or(OTHER)
    }

    /// Its number as the studio shows it: 1001 on, in order, a spare from 9001.
    pub fn label(&self, row: usize) -> String {
        match self.spare {
            Some(id) => format!("{}", 9000 + id),
            None => format!("{}", 1001 + row),
        }
    }
}

/// Where the relief within a layover from `arr` to `dep` is: when the driver who brought the
/// bus leaves it (`incoming` false) or the next one takes it (true).
pub fn relief_at(p: &Prefs, arr: i32, dep: i32, incoming: bool) -> i32 {
    match p.relief {
        ReliefTiming::ArrivalDeparture => {
            if incoming {
                dep
            } else {
                arr
            }
        }
        ReliefTiming::OnArrival => arr,
        ReliefTiming::OnDeparture => dep,
        ReliefTiming::MidLayover => (arr + dep) / 2,
        ReliefTiming::BeforeDeparture => (dep - p.relief_before).max(arr),
    }
}

/// The minutes a piece (block `t`, its trips `r`) is worked.
pub fn span_of(p: &Prefs, t: &TourOfDay, r: &Range<usize>) -> (i32, i32) {
    let trips = t.trips.get(r.clone()).unwrap_or(&[]);
    let (Some(first), Some(last)) = (trips.first(), trips.last()) else { return (0, 0) };
    let from = if r.start == 0 {
        first.dep - p.pre_trip
    } else {
        let prev = &t.trips[r.start - 1];
        relief_at(p, prev.arr, first.dep, true).min(first.dep)
    };
    let end = trips.iter().map(|x| x.arr).max().unwrap_or(last.arr);
    let to = if r.end >= t.trips.len() {
        end + p.post_trip
    } else {
        relief_at(p, last.arr, t.trips[r.end].dep, false).max(end)
    };
    (from, to)
}

fn piece_places(t: &TourOfDay, r: &Range<usize>) -> (String, String) {
    let a = t.trips.get(r.start).map(|x| x.from.trim().to_string()).unwrap_or_default();
    let b = r.end.checked_sub(1).and_then(|e| t.trips.get(e)).map(|x| x.end().trim().to_string()).unwrap_or_default();
    (a, b)
}

/// Which issue it is (the Validation panel's three lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Global,
    Vehicle,
    Duty,
}

/// Something the day breaks or comes near: its words (English, `%{..}` filled from `args`),
/// whether it is an error (a hard rule) or a warning, what it costs, and its row.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub scope: Scope,
    /// The block (`Board::tours`) or duty (`Board::shaped`) it is about.
    pub row: Option<usize>,
    pub text: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub hard: bool,
    pub penalty: Cents,
    /// Its key, the same from one look to the next (to dismiss it).
    pub key: String,
}

// the issues' words
pub const I_TYPE: &str = "Duty type unsatisfied: the duty is %{type}";
pub const I_MEAL: &str = "No meal break: a duty working over %{after} needs a break of %{min}";
pub const I_CONTINUOUS: &str = "%{time} without a break: at most %{max} before a break of %{min}";
pub const I_SPREAD: &str = "Spread of %{time}, over the limit of %{max}";
pub const I_SHORT_PIECE: &str = "A piece of %{time}, shorter than %{min}";
pub const I_CHANGEOVERS: &str = "%{n} changes of bus, more than %{max}";
pub const I_TRAVEL: &str = "No time to get to the relief point at %{stop}: %{need} needed, %{have} there";
pub const I_RULE: &str = "Working time: %{rule}";
pub const I_RULE_TIME: &str = "Working time: %{rule}, %{time}";
pub const I_LAYOVER: &str = "Layover of %{time} at %{stop}, shorter than %{min}";
pub const I_DEADHEAD: &str = "Deadhead from %{from} to %{to}: no trip of the line runs there, its time is estimated";
pub const I_UNASSIGNED: &str = "%{n} trips are not assigned to a vehicle";
pub const I_BUSES: &str = "%{n} buses at the peak, the company has %{m}";
pub const I_DEPOT: &str = "%{n} buses out at once, more than the depot holds (%{max})";
pub const I_DRIVERS: &str = "%{n} duties, %{m} drivers on the staff";

pub const ISSUE_TEXTS: [&str; 15] = [I_TYPE, I_MEAL, I_CONTINUOUS, I_SPREAD, I_SHORT_PIECE, I_CHANGEOVERS, I_TRAVEL, I_RULE, I_RULE_TIME, I_LAYOVER, I_DEADHEAD, I_UNASSIGNED, I_BUSES, I_DEPOT, I_DRIVERS];

/// The issues' short titles (the Validation panel's groups), in `ISSUE_TEXTS`' order.
pub const ISSUE_TITLES: [&str; 15] = ["Duty type unsatisfied", "No meal break", "Driving without a break", "Spread over the limit", "Pieces too short", "Too many changes of bus", "No time to get to the relief point", "Working time", "Working time", "Layovers too short", "Deadheads estimated", "Trips not assigned", "More buses than the company has", "More buses than the depot holds", "More duties than drivers"];

/// An issue's short title (its group in the Validation panel).
pub fn issue_title(text: &str) -> &'static str {
    ISSUE_TEXTS.iter().position(|t| *t == text).map(|k| ISSUE_TITLES[k]).unwrap_or("Issues")
}

/// An issue's words filled in (`tr`: how a text is translated).
pub fn issue_text(i: &Issue, tr: &dyn Fn(&str) -> String) -> String {
    let mut s = tr(i.text);
    for (k, v) in &i.args {
        let v = if *k == "rule" || *k == "type" { tr(v) } else { v.clone() };
        s = s.replace(&format!("%{{{k}}}"), &v);
    }
    s
}

/// The day's figures (the KPI dashboard).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Figures {
    pub duties: usize,
    pub blocks: usize,
    /// Buses out at once at the busiest.
    pub pvr: usize,
    pub service_km: f64,
    pub dead_km: f64,
    pub driving: i32,
    pub paid: i32,
    pub spread: i32,
    pub work: i32,
    pub platform: i32,
    pub splits: usize,
    pub split_time: i32,
    pub changeovers: usize,
    pub split_changeovers: usize,
    pub crew_cost: Cents,
    pub vehicle_cost: Cents,
    pub penalties: Cents,
    /// Duties by type (the types' names, "Other" last).
    pub types: Vec<(String, usize)>,
    /// Duties by paid time: under 5 h, 5-7, 7-9, 9-12, 12 and over.
    pub paid_bands: [usize; 5],
    /// Trips with passengers on a bus, and all of them (the status bar's 819/838).
    pub assigned: usize,
    pub trips: usize,
}

pub const PAID_BANDS: [&str; 5] = ["0-5", "5-7", "7-9", "9-12", "12+"];

impl Figures {
    /// Service kilometres against all a bus runs.
    pub fn vehicle_eff(&self) -> f64 {
        let all = self.service_km + self.dead_km;
        if all <= 0.0 {
            0.0
        } else {
            self.service_km / all
        }
    }

    /// Driving time against paid time.
    pub fn crew_eff(&self) -> f64 {
        if self.paid <= 0 {
            0.0
        } else {
            self.driving as f64 / self.paid as f64
        }
    }

    pub fn total(&self) -> Cents {
        self.crew_cost + self.vehicle_cost
    }

    /// What Optimise works down: the cost and the soft rules' penalties.
    pub fn algorithmic(&self) -> Cents {
        self.crew_cost + self.penalties
    }
}

/// The costs as the preferences set them, else as the company pays: a driver's hour, a
/// duty's fixed part, a bus a day, a kilometre.
pub fn rates(c: &Company) -> (f64, Cents, Cents, f64) {
    let p = &c.planning.flow.prefs;
    let drivers: Vec<Cents> = c.staff.iter().filter(|e| staff::drives(e)).map(|e| e.wage).collect();
    let wage = if drivers.is_empty() { economy::market_wage(45.0, c.price_index) } else { drivers.iter().sum::<Cents>() / drivers.len() as Cents };
    let hour = p.wage.map(|w| w as f64).unwrap_or_else(|| staff::hourly(economy::employer_cost(wage)));
    let fixed = p.driver_fixed.unwrap_or((hour * 3.0).round() as Cents);
    let bus = BusKind { size: BusSize::Solo, drive: Drive::Diesel };
    let per_km = p.per_km.map(|k| k as f64).unwrap_or_else(|| economy::energy_per_km(bus, c.price_index) + economy::maintenance_per_km(bus, 5.0, c.price_index));
    let day = p.vehicle_fixed.unwrap_or(economy::insurance_per_month(bus, c.price_index) / 30);
    (hour, fixed, day, per_km)
}

/// The day as RosterFlow shows it: the blocks, the duties, each duty's shape, the issues and
/// the figures.
#[derive(Clone, Debug, Default)]
pub struct Board {
    pub weekday: u8,
    pub tours: Vec<TourOfDay>,
    pub duties: Vec<Duty>,
    pub shaped: Vec<Shaped>,
    pub issues: Vec<Issue>,
    pub figures: Figures,
    pub stack: Vec<(String, PlannedTrip)>,
}

impl Board {
    /// The issues of `scope` not dismissed (or only the dismissed).
    pub fn open(&self, c: &Company, scope: Scope, dismissed_ones: bool) -> Vec<usize> {
        (0..self.issues.len()).filter(|&i| self.issues[i].scope == scope && dismissed(c, self.weekday, &self.issues[i].key) == dismissed_ones).collect()
    }

    /// The blocks with an issue (the red "!" by their row).
    pub fn block_issues(&self, i: usize) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(move |x| x.scope == Scope::Vehicle && x.row == Some(i))
    }
}

/// The day of `date` (`lines` the timetable read for it) as RosterFlow shows it, from the
/// company as the studio sees it (`view`).
pub fn board(c: &Company, lines: &[LineInfo], date: &str) -> Board {
    let wd = super::plan::weekday_of(date);
    let tours = network::tours_of_day(c, lines, date);
    let stack = stack(c, lines, wd);
    board_of(c, wd, tours, stack)
}

/// The board of these blocks (`tours` as `network::tours_of_day` gives them).
pub fn board_of(c: &Company, wd: u8, tours: Vec<TourOfDay>, stack: Vec<(String, PlannedTrip)>) -> Board {
    let p = &c.planning.flow.prefs;
    let duties = st::duties(&c.planning.joined, wd, &tours);
    let mut issues: Vec<Issue> = Vec::new();
    let mut shaped: Vec<Shaped> = Vec::new();
    for (di, d) in duties.iter().enumerate() {
        let key = duty_key(&tours, d);
        let n = key.as_ref().and_then(|k| note(c, wd, k)).cloned().unwrap_or_default();
        let s = shape(p, &tours, d, Some(di), key, &n);
        shaped.push(s);
    }
    for sp in c.planning.flow.sheet.spares.iter().filter(|s| s.weekday == wd) {
        let n = DutyNote { custom: sp.custom.clone(), kind: sp.kind.clone(), locked: sp.locked, paid: sp.paid, ..Default::default() };
        let mut s = shape(p, &tours, &Duty { pieces: Vec::new(), rules: Vec::new() }, None, None, &n);
        s.spare = Some(sp.id);
        shaped.push(s);
    }
    shaped.sort_by_key(|s| (s.spare.is_some(), s.start));
    // the duties' issues
    for (row, s) in shaped.iter_mut().enumerate() {
        let rk = match (&s.key, s.spare) {
            (Some(k), _) => format!("{}/{}/{}/{}", k.line, k.tour, k.first.name, k.first.dep),
            (None, Some(id)) => format!("spare/{id}"),
            _ => String::new(),
        };
        let mut found: Vec<(&'static str, Vec<(&'static str, String)>, bool, Cents)> = Vec::new();
        let mut add = |text: &'static str, args: Vec<(&'static str, String)>, hard: bool, penalty: Cents| found.push((text, args, hard, penalty));
        if let Some(d) = s.duty.and_then(|k| duties.get(k)) {
            for r in &d.rules {
                match r.minutes() {
                    Some(m) => add(I_RULE_TIME, vec![("rule", r.label().to_string()), ("time", hm(m.abs()))], r.hard(), if r.hard() { 0 } else { 10_00 }),
                    None => add(I_RULE, vec![("rule", r.label().to_string())], r.hard(), 0),
                }
            }
        }
        match s.kind.and_then(|k| p.duty_types.get(k)) {
            Some(t) if t.allowed => {
                if t.penalty > 0 {
                    // (a type allowed at a cost: its penalty counts, no issue)
                }
            }
            Some(t) => add(I_TYPE, vec![("type", t.name.clone())], true, 0),
            None => add(I_TYPE, vec![("type", OTHER.to_string())], true, 0),
        }
        check_breaks(p, &tours, s, &mut add);
        if p.max_spread.on && s.spread() > p.max_spread.value {
            add(I_SPREAD, vec![("time", hm(s.spread())), ("max", hm(p.max_spread.value))], p.max_spread.hard, p.max_spread.penalty);
        }
        if p.min_piece.on {
            if let Some(sp) = s.spans.iter().find(|sp| sp.to - sp.from < p.min_piece.value) {
                add(I_SHORT_PIECE, vec![("time", hm(sp.to - sp.from)), ("min", hm(p.min_piece.value))], p.min_piece.hard, p.min_piece.penalty);
            }
        }
        if p.changeovers.on && s.changeovers as i32 > p.changeovers.value {
            add(I_CHANGEOVERS, vec![("n", s.changeovers.to_string()), ("max", p.changeovers.value.to_string())], p.changeovers.hard, p.changeovers.penalty);
        }
        // travel to a relief point elsewhere in time
        for w in 0..s.spans.len().saturating_sub(1) {
            let (a, b) = (s.spans[w], s.spans[w + 1]);
            let (_, end_a) = piece_places(&tours[a.tour], &network::duties_of(&tours[a.tour]).get(a.duty).cloned().unwrap_or(0..0));
            let (start_b, _) = piece_places(&tours[b.tour], &network::duties_of(&tours[b.tour]).get(b.duty).cloned().unwrap_or(0..0));
            if same(&end_a, &start_b) {
                continue;
            }
            let need: i32 = s.events.iter().filter(|e| e.from >= a.to && e.from < b.from).map(|e| e.to - e.from).sum();
            if b.from - a.to < need {
                add(I_TRAVEL, vec![("stop", start_b.clone()), ("need", hm(need)), ("have", hm((b.from - a.to).max(0)))], true, 0);
            }
        }
        for (text, args, hard, penalty) in found {
            s.issues.push(issues.len());
            issues.push(Issue { scope: Scope::Duty, row: Some(row), text, args, hard, penalty, key: format!("{text}|{rk}") });
        }
    }
    // the blocks' issues
    for (i, t) in tours.iter().enumerate() {
        let line: Vec<&PlannedTrip> = tours.iter().filter(|x| same(&x.line, &t.line)).flat_map(|x| x.trips.iter()).collect();
        for w in t.trips.windows(2) {
            let (x, n) = (&w[0], &w[1]);
            let rk = format!("{}/{}/{}", t.line, t.tour, n.dep);
            if !same(x.end(), &n.from) {
                if !x.empty && !n.empty {
                    let _ = st::travel(&line, x.end(), &n.from);
                    issues.push(Issue { scope: Scope::Vehicle, row: Some(i), text: I_DEADHEAD, args: vec![("from", x.end().trim().to_string()), ("to", n.from.trim().to_string())], hard: false, penalty: 0, key: format!("{I_DEADHEAD}|{rk}") });
                }
            } else if p.layover.on && x.counts() && n.counts() && n.dep - x.arr < p.layover.value {
                issues.push(Issue { scope: Scope::Vehicle, row: Some(i), text: I_LAYOVER, args: vec![("time", hm(n.dep - x.arr)), ("stop", n.from.trim().to_string()), ("min", hm(p.layover.value))], hard: p.layover.hard, penalty: p.layover.penalty, key: format!("{I_LAYOVER}|{rk}") });
            }
        }
    }
    // the day's
    let pvr = st::peak(&tours);
    if !stack.is_empty() {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_UNASSIGNED, args: vec![("n", stack.len().to_string())], hard: true, penalty: 0, key: I_UNASSIGNED.into() });
    }
    if pvr > c.fleet.len() {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_BUSES, args: vec![("n", pvr.to_string()), ("m", c.fleet.len().to_string())], hard: false, penalty: 0, key: I_BUSES.into() });
    }
    if p.depot_capacity.on && pvr as i32 > p.depot_capacity.value {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_DEPOT, args: vec![("n", pvr.to_string()), ("max", p.depot_capacity.value.to_string())], hard: p.depot_capacity.hard, penalty: p.depot_capacity.penalty, key: I_DEPOT.into() });
    }
    let drivers = c.staff.iter().filter(|e| staff::drives(e)).count();
    if shaped.len() > drivers {
        issues.push(Issue { scope: Scope::Global, row: None, text: I_DRIVERS, args: vec![("n", shaped.len().to_string()), ("m", drivers.to_string())], hard: false, penalty: 0, key: I_DRIVERS.into() });
    }
    let figures = figures(c, &tours, &shaped, &issues, pvr, stack.len());
    Board { weekday: wd, tours, duties, shaped, issues, figures, stack }
}

/// A duty's shape (`Shaped`) from its pieces and its note.
fn shape(p: &Prefs, tours: &[TourOfDay], d: &Duty, di: Option<usize>, key: Option<PieceRef>, n: &DutyNote) -> Shaped {
    let mut spans: Vec<Span> = d
        .pieces
        .iter()
        .map(|x| {
            let r = network::duties_of(&tours[x.tour]).get(x.duty).cloned().unwrap_or(0..0);
            let (from, to) = span_of(p, &tours[x.tour], &r);
            Span { tour: x.tour, duty: x.duty, from, to }
        })
        .collect();
    spans.sort_by_key(|s| s.from);
    let mut events: Vec<Ev> = Vec::new();
    let mut splits: Vec<(i32, i32)> = Vec::new();
    let sign_on = n.sign_on.unwrap_or(p.sign_on);
    let sign_off = n.sign_off.unwrap_or(p.sign_off);
    let range = |s: &Span| network::duties_of(&tours[s.tour]).get(s.duty).cloned().unwrap_or(0..0);
    if let Some(f) = spans.first() {
        events.push(Ev { kind: EvKind::SignOn, from: f.from - sign_on, to: f.from });
    }
    for w in 0..spans.len().saturating_sub(1) {
        let (a, b) = (spans[w], spans[w + 1]);
        let gap = b.from - a.to;
        let (_, end_a) = piece_places(&tours[a.tour], &range(&a));
        let (start_b, _) = piece_places(&tours[b.tour], &range(&b));
        let mode = n.travel.iter().find(|(k, _)| *k as usize == w).map(|x| x.1).unwrap_or(p.travel);
        let m = if same(&end_a, &start_b) { 0 } else { mode.minutes(p) };
        if gap >= p.split_min {
            let so = p.sign_off_split;
            let son = p.sign_on_split;
            events.push(Ev { kind: EvKind::SignOff, from: a.to, to: a.to + so });
            if m > 0 {
                events.push(Ev { kind: EvKind::Travel(mode), from: b.from - son - m, to: b.from - son });
            }
            events.push(Ev { kind: EvKind::SignOn, from: b.from - son, to: b.from });
            let (s0, s1) = (a.to + so, b.from - son - m);
            if s1 > s0 {
                splits.push((s0, s1));
            }
        } else if m > 0 {
            events.push(Ev { kind: EvKind::Travel(mode), from: a.to, to: a.to + m });
        }
    }
    if let Some(l) = spans.last() {
        events.push(Ev { kind: EvKind::SignOff, from: l.to, to: l.to + sign_off });
    }
    for (k, cw) in n.custom.iter().enumerate() {
        events.push(Ev { kind: EvKind::Custom(k), from: cw.from, to: cw.to });
    }
    events.sort_by_key(|e| e.from);
    let start = events.iter().map(|e| e.from).chain(spans.iter().map(|s| s.from)).min().unwrap_or(0);
    let end = events.iter().map(|e| e.to).chain(spans.iter().map(|s| s.to)).max().unwrap_or(0);
    let unpaid: i32 = splits.iter().map(|s| s.1 - s.0).sum();
    let work = (end - start - unpaid).max(0);
    let paid = n.paid.unwrap_or(work.max(MIN_PAID));
    let driving: i32 = spans.iter().map(|s| tours[s.tour].trips[range(s)].iter().map(|x| x.arr - x.dep).sum::<i32>()).sum();
    let platform: i32 = spans.iter().map(|s| s.to - s.from).sum();
    let changeovers = spans.windows(2).filter(|w| w[0].tour != w[1].tour).count();
    let split = !splits.is_empty();
    let set = n.kind.as_ref().map(|name| p.duty_types.iter().position(|t| same(&t.name, name)));
    let (kind, kind_set) = match set {
        Some(k) => (k, true),
        None => (p.duty_types.iter().position(|t| t.fits(start, end, split)), false),
    };
    Shaped { duty: di, spare: None, key, spans, events, custom: n.custom.clone(), start, end, work, paid, driving, platform, splits, changeovers, kind, kind_set, locked: n.locked, issues: Vec::new() }
}

/// The breaks of a duty a meal or a rest can be taken in: the gaps between its work, and (if
/// the preferences say so) the layovers inside its pieces.
pub fn breaks_of(p: &Prefs, tours: &[TourOfDay], s: &Shaped) -> Vec<(i32, i32)> {
    let mut busy: Vec<(i32, i32)> = s.spans.iter().map(|x| (x.from, x.to)).chain(s.events.iter().map(|e| (e.from, e.to))).collect();
    busy.sort();
    let mut out: Vec<(i32, i32)> = Vec::new();
    let mut at = busy.first().map(|b| b.1).unwrap_or(0);
    for b in busy.iter().skip(1) {
        if b.0 > at {
            out.push((at, b.0));
        }
        at = at.max(b.1);
    }
    if p.layover_breaks {
        for sp in &s.spans {
            let t = &tours[sp.tour];
            let r = network::duties_of(t).get(sp.duty).cloned().unwrap_or(0..0);
            for k in r.start + 1..r.end {
                let (x, n) = (&t.trips[k - 1], &t.trips[k]);
                if same(x.end(), &n.from) && n.dep > x.arr {
                    out.push((x.arr, n.dep));
                }
            }
        }
    }
    out.sort();
    out
}

fn check_breaks(p: &Prefs, tours: &[TourOfDay], s: &Shaped, add: &mut dyn FnMut(&'static str, Vec<(&'static str, String)>, bool, Cents)) {
    if s.spans.is_empty() {
        return;
    }
    let breaks = breaks_of(p, tours, s);
    if p.meal.on && s.work > p.meal_after && !breaks.iter().any(|b| b.1 - b.0 >= p.meal.value) {
        add(I_MEAL, vec![("after", hm(p.meal_after)), ("min", hm(p.meal.value))], p.meal.hard, p.meal.penalty);
    }
    if p.continuous.on {
        let longest = longest_stretch(s.start, s.end, &breaks, p.break_min);
        if longest > p.continuous.value {
            add(I_CONTINUOUS, vec![("time", hm(longest)), ("max", hm(p.continuous.value)), ("min", hm(p.break_min))], p.continuous.hard, p.continuous.penalty);
        }
    }
}

/// The longest stretch from `start` to `end` without a break of `min` or more.
pub fn longest_stretch(start: i32, end: i32, breaks: &[(i32, i32)], min: i32) -> i32 {
    let mut since = start;
    let mut longest = 0;
    for b in breaks.iter().filter(|b| b.1 - b.0 >= min) {
        if b.0 < since {
            since = since.max(b.1);
            continue;
        }
        longest = longest.max(b.0 - since);
        since = b.1;
    }
    longest.max(end - since)
}

fn figures(c: &Company, tours: &[TourOfDay], shaped: &[Shaped], issues: &[Issue], pvr: usize, stacked: usize) -> Figures {
    let p = &c.planning.flow.prefs;
    let (hour, fixed, day, per_km) = rates(c);
    let mut f = Figures { duties: shaped.len(), blocks: tours.len(), pvr, ..Default::default() };
    for t in tours {
        let line: Vec<&PlannedTrip> = tours.iter().filter(|x| same(&x.line, &t.line)).flat_map(|x| x.trips.iter()).collect();
        f.dead_km += st::deadheads(t, &line).iter().map(|d| d.2).sum::<f64>();
        for x in &t.trips {
            if x.counts() {
                f.service_km += x.km;
                f.assigned += 1;
            } else {
                f.dead_km += x.km;
            }
        }
    }
    f.trips = f.assigned + stacked;
    for s in shaped {
        f.driving += s.driving;
        f.paid += s.paid;
        f.spread += s.spread();
        f.work += s.work;
        f.platform += s.platform;
        if s.split() {
            f.splits += 1;
            f.split_time += s.splits.iter().map(|x| x.1 - x.0).sum::<i32>();
            f.split_changeovers += s.changeovers;
        }
        f.changeovers += s.changeovers;
        let band = match s.paid {
            m if m < 300 => 0,
            m if m < 420 => 1,
            m if m < 540 => 2,
            m if m < 720 => 3,
            _ => 4,
        };
        f.paid_bands[band] += 1;
        if let Some(t) = s.kind.and_then(|k| p.duty_types.get(k)) {
            f.penalties += t.penalty;
        }
    }
    for t in p.duty_types.iter() {
        f.types.push((t.name.clone(), shaped.iter().filter(|s| s.kind.and_then(|k| p.duty_types.get(k)).is_some_and(|x| x.name == t.name)).count()));
    }
    f.types.push((OTHER.to_string(), shaped.iter().filter(|s| s.kind.is_none()).count()));
    let night: i32 = shaped.iter().flat_map(|s| s.spans.iter()).map(|x| staff::night_minutes(x.from, x.to)).sum();
    f.crew_cost = ((f.paid as f64 + night as f64 * staff::NIGHT_SUPPLEMENT) / 60.0 * hour).round() as Cents + fixed * shaped.len() as Cents;
    f.vehicle_cost = ((f.service_km + f.dead_km) * per_km).round() as Cents + day * pvr as Cents;
    f.penalties += issues.iter().filter(|i| !i.hard).map(|i| i.penalty).sum::<Cents>();
    f
}

// --- Optimise --------------------------------------------------------------------------------

/// A run of trips that stays on one bus (a trip with its depot runs, linked trips).
#[derive(Clone, Debug)]
struct Unit {
    trips: Vec<PlannedTrip>,
}

impl Unit {
    fn dep(&self) -> i32 {
        self.trips.first().map(|x| x.dep).unwrap_or(0)
    }

    fn starts_in_depot(&self) -> bool {
        self.trips.first().is_some_and(|x| x.empty)
    }

    fn ends_in_depot(&self) -> bool {
        self.trips.last().is_some_and(|x| x.empty)
    }
}

/// The blocks of one line as Optimise makes them: the locked blocks as they are, every other
/// trip chained onto the bus that is free the latest before it (fewest buses), with time to
/// get there and the layover the preferences ask for.
fn block_line(c: &Company, wd: u8, line: &str, raw: &[&TourOfDay], current: &[&TourOfDay], keep: &[String]) -> LineLayout {
    let p = &c.planning.flow.prefs;
    let mut blocks: Vec<st::BlockLayout> = Vec::new();
    let mut taken: Vec<TripRef> = Vec::new();
    for t in current.iter().filter(|t| keep.iter().any(|k| same(k, &t.tour))) {
        let refs: Vec<TripRef> = t.trips.iter().filter(|x| !x.origin.trim().is_empty()).map(TripRef::of).collect();
        taken.extend(refs.iter().cloned());
        blocks.push(st::BlockLayout { tour: t.tour.clone(), trips: refs, cuts: t.cuts.as_ref().map(|cs| cs.iter().filter_map(|&k| t.trips.get(k)).map(TripRef::of).collect()) });
    }
    // the units: each trip with passengers, with the depot runs before the first of its tour
    // and after the last, the empty runs in the day with the trip before them
    let mut units: Vec<Unit> = Vec::new();
    for t in raw {
        let counting: Vec<usize> = (0..t.trips.len()).filter(|&k| t.trips[k].counts()).collect();
        let (Some(&first), Some(&last)) = (counting.first(), counting.last()) else { continue };
        let mut cur: Option<Unit> = None;
        for (k, x) in t.trips.iter().enumerate() {
            if taken.iter().any(|r| r.is(x)) {
                continue;
            }
            if x.counts() {
                if let Some(u) = cur.take() {
                    units.push(u);
                }
                let mut u = Unit { trips: Vec::new() };
                if k == first {
                    u.trips.extend(t.trips[..k].iter().filter(|y| !taken.iter().any(|r| r.is(y))).cloned());
                }
                u.trips.push(x.clone());
                cur = Some(u);
            } else if k > first {
                if let Some(u) = cur.as_mut() {
                    u.trips.push(x.clone());
                }
            }
            if k == last {
                // (what comes after the last trip goes with it)
                if let Some(u) = cur.as_mut() {
                    u.trips.extend(t.trips[k + 1..].iter().filter(|y| !taken.iter().any(|r| r.is(y))).cloned());
                }
                if let Some(u) = cur.take() {
                    units.push(u);
                }
                break;
            }
        }
    }
    // linked trips one unit
    let links: Vec<&Link> = c.planning.flow.sheet.links.iter().filter(|l| l.weekday == wd && same(&l.line, line)).collect();
    for l in links {
        let at: Vec<usize> = (0..units.len()).filter(|&i| units[i].trips.iter().any(|x| l.trips.iter().any(|r| r.is(x)))).collect();
        if at.len() > 1 {
            let mut merged = Unit { trips: Vec::new() };
            for &i in at.iter().rev() {
                merged.trips.extend(units.remove(i).trips);
            }
            merged.trips.sort_by_key(|x| x.dep);
            units.push(merged);
        }
    }
    units.sort_by_key(|u| u.dep());
    let all: Vec<&PlannedTrip> = raw.iter().flat_map(|t| t.trips.iter()).collect();
    // (open buses: their trips, closed once back in the depot)
    let mut open: Vec<(Vec<PlannedTrip>, bool)> = Vec::new();
    for u in units {
        let mut best: Option<(usize, i32, bool)> = None;
        if !u.starts_in_depot() {
            for (bi, (trips, closed)) in open.iter().enumerate() {
                if *closed {
                    continue;
                }
                let Some(last) = trips.last() else { continue };
                let travel = st::travel(&all, last.end(), &u.trips[0].from);
                let free = last.arr + travel;
                if free > u.dep() {
                    continue;
                }
                let lay_ok = !p.layover.on || u.dep() - last.arr >= p.layover.value;
                if p.layover.on && p.layover.hard && !lay_ok {
                    continue;
                }
                // (the bus free the latest; one keeping the layover first)
                let better = match best {
                    None => true,
                    Some((_, at, ok)) => (lay_ok && !ok) || (lay_ok == ok && last.arr > at),
                };
                if better {
                    best = Some((bi, last.arr, lay_ok));
                }
            }
        }
        let closes = u.ends_in_depot();
        match best {
            Some((bi, _, _)) => {
                open[bi].0.extend(u.trips);
                open[bi].1 = closes;
            }
            None => open.push((u.trips, closes)),
        }
    }
    // their names: the timetable tour each runs most of, else a new number
    let mut names: Vec<String> = blocks.iter().map(|b| b.tour.trim().to_string()).collect();
    for (trips, _) in open {
        let mut count: Vec<(String, usize)> = Vec::new();
        for x in trips.iter().filter(|x| x.counts()) {
            match count.iter_mut().find(|c| c.0 == x.origin.trim()) {
                Some(c) => c.1 += 1,
                None => count.push((x.origin.trim().to_string(), 1)),
            }
        }
        count.sort_by_key(|c| std::cmp::Reverse(c.1));
        let name = count.into_iter().map(|c| c.0).find(|n| !n.is_empty() && !names.contains(n)).unwrap_or_else(|| {
            let mut k = names.iter().filter_map(|n| n.parse::<u32>().ok()).max().unwrap_or(0) + 1;
            while names.contains(&k.to_string()) {
                k += 1;
            }
            k.to_string()
        });
        names.push(name.clone());
        blocks.push(st::BlockLayout { tour: name, trips: trips.iter().filter(|x| !x.origin.trim().is_empty()).map(TripRef::of).collect(), cuts: None });
    }
    LineLayout { weekday: wd, line: line.to_string(), blocks, stack: Vec::new() }
}

/// A piece of work while duties are made: its block and trips, its time and places.
#[derive(Clone, Debug)]
struct Cut {
    tour: usize,
    range: Range<usize>,
    from: i32,
    to: i32,
    at: String,
    till: String,
}

/// Where block `t` can be cut (a relief point where the bus stands, before a trip with
/// passengers), and the pieces Optimise cuts it into: as long as a driver may work without a
/// break, not shorter than the least piece where it can help.
fn cut_block(c: &Company, tours: &[TourOfDay], i: usize, p: &Prefs) -> Vec<Range<usize>> {
    let t = &tours[i];
    let n = t.trips.len();
    if n == 0 {
        return Vec::new();
    }
    let longest = if p.continuous.on { p.continuous.value } else { staff::DUTY_MAX }.min(staff::DAY_MAX);
    let can: Vec<usize> = (1..n).filter(|&k| t.trips[k].counts() && same(t.trips[k - 1].end(), &t.trips[k].from) && st::is_relief(c, tours, &t.line, &t.trips[k].from) && t.trips[..k].iter().any(|x| x.counts())).collect();
    let span = |a: usize, b: usize| t.trips[a..b].iter().map(|x| x.arr).max().unwrap_or(0) - t.trips[a].dep;
    let mut cuts: Vec<usize> = Vec::new();
    let mut s = 0;
    while span(s, n) > longest {
        let fit: Vec<usize> = can.iter().copied().filter(|&k| k > s && span(s, k) <= longest).collect();
        let next = fit.last().copied().or_else(|| can.iter().copied().find(|&k| k > s));
        match next {
            Some(k) => {
                cuts.push(k);
                s = k;
            }
            None => break,
        }
    }
    // (a last piece too short goes into the one before, where that stays within reach)
    if p.min_piece.on {
        if let Some(&k) = cuts.last() {
            let prev = cuts.len().checked_sub(2).map(|j| cuts[j]).unwrap_or(0);
            if span(k, n) < p.min_piece.value && span(prev, n) <= longest + longest / 4 {
                cuts.pop();
            }
        }
    }
    let mut out = Vec::new();
    let mut from = 0;
    for k in cuts.into_iter().chain(std::iter::once(n)) {
        out.push(from..k);
        from = k;
    }
    out
}

/// What a planning costs Optimise: the day's cost, the soft rules' penalties, and every hard
/// rule broken as dear as a bus a day more (so a planning that breaks none is chosen first).
pub fn weighed(b: &Board) -> Cents {
    b.figures.total() + b.figures.penalties + 500_00 * b.issues.iter().filter(|i| i.hard).count() as Cents
}

/// What Optimise makes of `date` (`lines` the timetable read for it), from the company as the
/// studio sees it: the weekday's blocks and joined duties, the locked ones kept - the cheapest
/// (`weighed`) of what the preferences ask for, the duties alone made anew on the blocks as
/// they are, and the day as it is. Nothing is kept; `apply` keeps it.
pub fn optimise(c: &Company, lines: &[LineInfo], date: &str) -> (Vec<LineLayout>, Vec<JoinedDuty>) {
    let wd = super::plan::weekday_of(date);
    let p = &c.planning.flow.prefs;
    let now = (c.planning.layouts.iter().filter(|l| l.weekday == wd).cloned().collect::<Vec<_>>(), c.planning.joined.iter().filter(|j| j.weekday == wd).cloned().collect::<Vec<_>>());
    let mut cands = vec![optimise_with(c, lines, date, p.optimise_vehicles, p.optimise_duties)];
    if p.optimise_vehicles && p.optimise_duties {
        cands.push(optimise_with(c, lines, date, false, true));
    }
    cands.push(now.clone());
    let mut best: Option<(Cents, (Vec<LineLayout>, Vec<JoinedDuty>))> = None;
    for made in cands {
        let mut probe = c.clone();
        apply(&mut probe, wd, made.0.clone(), made.1.clone());
        let cost = weighed(&board(&probe, lines, date));
        if best.as_ref().is_none_or(|b| cost < b.0) {
            best = Some((cost, made));
        }
    }
    best.map(|b| b.1).unwrap_or(now)
}

/// Optimise with the vehicles (`vehicles`) and the duties (`duties`) made anew or kept.
pub fn optimise_with(c: &Company, lines: &[LineInfo], date: &str, vehicles: bool, duties: bool) -> (Vec<LineLayout>, Vec<JoinedDuty>) {
    let wd = super::plan::weekday_of(date);
    let mut p = c.planning.flow.prefs.clone();
    p.optimise_vehicles = vehicles;
    p.optimise_duties = duties;
    let current = network::tours_of_day(c, lines, date);
    let raw = network::timetable_tours(c, lines);
    let duties = st::duties(&c.planning.joined, wd, &current);
    // the locked duties, and the blocks they hold (kept as they are)
    let locked: Vec<usize> = (0..duties.len()).filter(|&d| duty_key(&current, &duties[d]).is_some_and(|k| duty_locked(c, wd, &k))).collect();
    let mut keep: Vec<(String, String)> = c.planning.flow.sheet.locks.iter().filter(|l| l.weekday == wd).map(|l| (l.line.clone(), l.tour.clone())).collect();
    for &d in &locked {
        for pc in &duties[d].pieces {
            let t = &current[pc.tour];
            keep.push((t.line.clone(), t.tour.clone()));
        }
    }
    // the vehicles
    let mut layouts: Vec<LineLayout> = Vec::new();
    for cl in &c.lines {
        let mine_raw: Vec<&TourOfDay> = raw.iter().filter(|t| same(&t.line, &cl.name)).collect();
        if mine_raw.is_empty() {
            continue;
        }
        let mine_now: Vec<&TourOfDay> = current.iter().filter(|t| same(&t.line, &cl.name)).collect();
        let lay = if p.optimise_vehicles {
            let k: Vec<String> = keep.iter().filter(|(l, _)| same(l, &cl.name)).map(|(_, t)| t.clone()).collect();
            block_line(c, wd, &cl.name, &mine_raw, &mine_now, &k)
        } else {
            let mut now = c.planning.layouts.iter().find(|l| l.weekday == wd && same(&l.line, &cl.name)).cloned().unwrap_or_else(|| LineLayout { weekday: wd, line: cl.name.clone(), ..Default::default() });
            now.blocks = mine_now.iter().map(|t| st::BlockLayout { tour: t.tour.clone(), trips: t.trips.iter().filter(|x| !x.origin.trim().is_empty()).map(TripRef::of).collect(), cuts: t.cuts.as_ref().map(|cs| cs.iter().filter_map(|&k| t.trips.get(k)).map(TripRef::of).collect()) }).collect();
            now
        };
        layouts.push(lay);
    }
    let locked_joins: Vec<JoinedDuty> = c.planning.joined.iter().filter(|j| j.weekday == wd).filter(|j| locked.iter().any(|&d| duties[d].pieces.iter().any(|pc| st::piece_ref(&current, pc.tour, pc.duty).is_some_and(|r| j.pieces.iter().any(|x| x.is(&r)))))).cloned().collect();
    if !p.optimise_duties {
        let joined = c.planning.joined.iter().filter(|j| j.weekday == wd).cloned().collect();
        return (layouts, joined);
    }
    // the blocks as they now are, to cut
    let mut probe = c.clone();
    probe.planning.layouts.retain(|l| l.weekday != wd);
    probe.planning.layouts.extend(layouts.iter().cloned());
    probe.planning.joined.retain(|j| j.weekday != wd);
    probe.planning.joined.extend(locked_joins.iter().cloned());
    let tours = network::tours_of_day(&probe, lines, date);
    let kept = |t: &TourOfDay| keep.iter().any(|(l, b)| same(l, &t.line) && same(b, &t.tour));
    let mut cuts: Vec<Cut> = Vec::new();
    let mut fixed_cuts: Vec<(usize, Vec<Range<usize>>)> = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        let ranges = if kept(t) { network::duties_of(t) } else { cut_block(&probe, &tours, i, &p) };
        if kept(t) {
            fixed_cuts.push((i, ranges.clone()));
            continue;
        }
        for r in ranges {
            let (from, to) = span_of(&p, t, &r);
            let (at, till) = piece_places(t, &r);
            cuts.push(Cut { tour: i, range: r, from, to, at, till });
        }
    }
    cuts.sort_by_key(|x| x.from);
    // pair the pieces into duties: each onto the duty it adds the least cost to, or a new one
    let (hour, fixed, _, _) = rates(c);
    let rate = hour / 60.0;
    let paid_of = |ps: &[&Cut]| -> i32 {
        let start = ps.first().map(|x| x.from).unwrap_or(0) - p.sign_on;
        let end = ps.iter().map(|x| x.to).max().unwrap_or(0) + p.sign_off;
        let unpaid: i32 = ps.windows(2).map(|w| w[1].from - w[0].to).filter(|g| *g >= p.split_min).sum();
        (end - start - unpaid).max(MIN_PAID)
    };
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (ci, x) in cuts.iter().enumerate() {
        let alone = fixed as f64 + paid_of(&[x]) as f64 * rate;
        let mut best: Option<(usize, f64)> = None;
        for (gi, g) in groups.iter().enumerate() {
            let last = &cuts[*g.last().unwrap()];
            let gap = x.from - last.to;
            let travel = if same(&last.till, &x.at) { 0 } else { p.travel.minutes(&p) };
            if gap < travel + staff::REST {
                continue;
            }
            let mut ps: Vec<&Cut> = g.iter().map(|&k| &cuts[k]).collect();
            let before = paid_of(&ps);
            ps.push(x);
            if ps.windows(2).filter(|w| w[1].from - w[0].to >= p.split_min).count() > 1 {
                continue;
            }
            let blocks: Vec<Block> = ps.iter().map(|y| Block { key: format!("{}/{}", y.tour, y.range.start), from: y.from, to: y.to, from_stop: y.at.clone(), to_stop: y.till.clone() }).collect();
            if st::rules_of(&blocks).iter().any(|r| r.hard()) {
                continue;
            }
            let spread = ps.last().unwrap().to + p.sign_off - (ps[0].from - p.sign_on);
            let mut penalty = 0.0;
            if p.max_spread.on && spread > p.max_spread.value {
                if p.max_spread.hard {
                    continue;
                }
                penalty += p.max_spread.penalty as f64;
            }
            let changes = ps.windows(2).filter(|w| w[0].tour != w[1].tour).count() as i32;
            if p.changeovers.on && changes > p.changeovers.value {
                if p.changeovers.hard {
                    continue;
                }
                penalty += p.changeovers.penalty as f64;
            }
            let delta = (paid_of(&ps) - before) as f64 * rate + penalty;
            if delta < alone && best.is_none_or(|b| delta < b.1) {
                best = Some((gi, delta));
            }
        }
        match best {
            Some((gi, _)) => groups[gi].push(ci),
            None => groups.push(vec![ci]),
        }
    }
    // pieces next to each other on one bus in one duty are one piece: no cut between them
    let mut starts: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        if kept(t) {
            continue;
        }
        let mine: Vec<usize> = (0..cuts.len()).filter(|&k| cuts[k].tour == i).collect();
        let mut s: Vec<usize> = Vec::new();
        for &k in mine.iter().skip(1) {
            let prev = mine.iter().copied().find(|&j| cuts[j].range.end == cuts[k].range.start);
            let together = prev.is_some_and(|j| groups.iter().any(|g| g.contains(&j) && g.contains(&k)));
            if !together {
                s.push(cuts[k].range.start);
            }
        }
        starts.push((i, s));
    }
    let _ = fixed_cuts;
    for lay in layouts.iter_mut() {
        for b in lay.blocks.iter_mut() {
            let Some(i) = tours.iter().position(|t| same(&t.line, &lay.line) && same(&t.tour, &b.tour)) else { continue };
            if let Some((_, s)) = starts.iter().find(|(k, _)| *k == i) {
                b.cuts = Some(s.iter().filter_map(|&k| tours[i].trips.get(k)).map(TripRef::of).collect());
            }
        }
    }
    // the duties of pieces of more than one run: joined
    let mut joined = locked_joins;
    for g in &groups {
        // (merged runs: the first piece of each run on its bus)
        let mut refs: Vec<PieceRef> = Vec::new();
        for &k in g {
            let x = &cuts[k];
            let run_start = g.iter().any(|&j| cuts[j].tour == x.tour && cuts[j].range.end == x.range.start);
            if !run_start {
                let t = &tours[x.tour];
                refs.push(PieceRef { line: t.line.clone(), tour: t.tour.clone(), first: TripRef::of(&t.trips[x.range.start]) });
            }
        }
        if refs.len() > 1 {
            joined.push(JoinedDuty { weekday: wd, pieces: refs });
        }
    }
    (layouts, joined)
}

/// Keep what Optimise made (`optimise`) for `weekday`: the blocks and joined duties replaced.
pub fn apply(c: &mut Company, weekday: u8, layouts: Vec<LineLayout>, joined: Vec<JoinedDuty>) {
    c.planning.layouts.retain(|l| l.weekday != weekday);
    c.planning.layouts.extend(layouts);
    c.planning.joined.retain(|j| j.weekday != weekday);
    c.planning.joined.extend(joined);
}

/// Every English text the core gives RosterFlow's pages (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    out.extend(ISSUE_TEXTS);
    out.extend(ISSUE_TITLES);
    out.extend(ReliefTiming::LABELS);
    out.extend(Travel::LABELS);
    out.extend(Tri::LABELS);
    out.extend(TYPE_NAMES);
    out.push(OTHER);
    out.extend(CUSTOM_KINDS);
    out.extend([LINK_ONE_BUS, LINK_TWO]);
    out
}

#[cfg(test)]
mod tests;
