//! The company's inbox: what happens to it and around it, told as messages - events in the
//! world (a fuel price, a busy weekend), its staff, its buses, its workshop, its research, its
//! contracts and goals, its money. Every part of the company posts here (`post`).
//!
//! The world's events (City Bus Manager's: a supply shortage that drives the diesel price up
//! until Monday, environment days with more passengers, ...) come by chance, but the same
//! company on the same day draws the same (`rng`): each day of the calendar may bring one
//! event, told a few days before it starts (`LEAD`), again when it starts and when it is over.
//! While it runs it does what it says: the diesel's price (`fuel_price_factor`), how many ride
//! (`demand_factor`), and on a line - roadworks, a diversion - or in the weather, trips held
//! up (`line_effect`).
//!
//! What the day's close already knows - its notes and its staff's (`day::DayReport`) - comes
//! in as messages too (`bridge`), so the inbox is full from the first day.

use super::clock::Level;
use super::dates;
use super::day::{DayReport, Note};
use super::economy;
use super::model::{BookingKind, Company};
use super::rng::Rng;
use super::staff::StaffNote;
use serde::{Deserialize, Serialize};

/// How many messages the inbox keeps (the oldest read ones go first).
pub const KEPT: usize = 300;
/// How many days an event is kept after it ended.
const EVENTS_KEPT: i64 = 30;
/// The chance that an event of the world starts on a day.
const EVENT_CHANCE: f64 = 0.2;
/// At most this many events run (or are coming) at once.
const EVENTS_AT_ONCE: usize = 3;
/// How many days before it starts an event is told (at least, at most).
pub const LEAD: (i64, i64) = (2, 5);

/// What a message is about.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Notice,
    Event,
    Staff,
    Fleet,
    Workshop,
    Research,
    Contract,
    Goal,
    Finance,
    /// The lines and their service: concessions, tours not covered, full buses.
    Lines,
}

impl Kind {
    pub const ALL: [Kind; 10] = [Kind::Event, Kind::Goal, Kind::Contract, Kind::Staff, Kind::Fleet, Kind::Workshop, Kind::Lines, Kind::Finance, Kind::Research, Kind::Notice];

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Kind::Notice => "Notices",
            Kind::Event => "Events",
            Kind::Staff => "Staff",
            Kind::Fleet => "Fleet",
            Kind::Workshop => "Workshop",
            Kind::Research => "Research",
            Kind::Contract => "Contracts",
            Kind::Goal => "Goals",
            Kind::Finance => "Finances",
            Kind::Lines => "Lines",
        }
    }
}

/// A message: its title and text are English and translated when shown, `%{name}` in them
/// filled from `args`; an event's days run `from` to `until`.
///
/// The pages write some places their own way, by their names: `amount`, `reward`, `cost` and
/// `charge` are whole cents (money), `price` and `price_after` cents with their decimals (a
/// litre of diesel), `date`, `from` and `until` days (`YYYY-MM-DD`), `month` a `YYYY-MM`,
/// `count` a number to be grouped, `stars` a number with one decimal; `what`, `why`, `area`,
/// `job` and `role` are English words translated. The rest is put in as it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Message {
    pub id: u64,
    /// The company's day it came.
    pub date: String,
    pub kind: Kind,
    pub title: String,
    pub text: String,
    #[serde(default)]
    pub args: Vec<(String, String)>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub read: bool,
    /// How much it matters (its colour).
    #[serde(default = "info")]
    pub level: Level,
    /// A goal of the contracts whose progress it shows (`contracts::Goal::id`).
    #[serde(default)]
    pub goal: Option<u32>,
    /// A progress it shows besides: done, of.
    #[serde(default)]
    pub progress: Option<(f64, f64)>,
    /// An unread message of the same key is replaced by it (a warning repeated day by day).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// The event of the world it tells (`Event::id`).
    #[serde(default)]
    pub event: Option<u64>,
}

fn info() -> Level {
    Level::Info
}

impl Message {
    pub fn new(kind: Kind, title: impl Into<String>, text: impl Into<String>) -> Message {
        Message { kind, title: title.into(), text: text.into(), level: Level::Info, ..Default::default() }
    }

    /// With `%{name}` filled by `value`.
    pub fn arg(mut self, name: &str, value: impl Into<String>) -> Message {
        self.args.push((name.to_string(), value.into()));
        self
    }

    /// Of this importance.
    pub fn level(mut self, level: Level) -> Message {
        self.level = level;
        self
    }

    /// With its days (an event's): a bar of them shows how far into them the company is.
    pub fn days(mut self, from: &str, until: &str) -> Message {
        self.from = Some(from.to_string());
        self.until = Some(until.to_string());
        self
    }

    /// Showing the progress of the goal `id`.
    pub fn goal(mut self, id: u32) -> Message {
        self.goal = Some(id);
        self
    }

    /// Showing a progress: `done` of `of`.
    pub fn progress(mut self, done: f64, of: f64) -> Message {
        self.progress = Some((done, of));
        self
    }

    /// Replacing an unread message of the same key.
    pub fn key(mut self, key: &str) -> Message {
        self.key = key.to_string();
        self
    }

    /// Dated `date` (else the company's day it is posted).
    pub fn dated(mut self, date: &str) -> Message {
        self.date = date.to_string();
        self
    }

    /// The value of the place `name`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.args.iter().find(|a| a.0 == name).map(|a| a.1.as_str())
    }
}

/// `text` with its `%{name}` places filled from `args` as they are (the pages translate the
/// text first and write some values their way: see `Message`).
pub fn fill(text: &str, args: &[(String, String)]) -> String {
    let mut t = text.to_string();
    for (k, v) in args {
        t = t.replace(&format!("%{{{k}}}"), v);
    }
    t
}

/// The messages, newest last; the world's events.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Inbox {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub next_id: u64,
    /// The world's events: coming, running, and over (for `EVENTS_KEPT` days).
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub next_event: u64,
    /// The welcome was said, and the events already running were looked for (`welcome`).
    #[serde(default)]
    pub welcomed: bool,
}

impl Inbox {
    /// Messages not read yet.
    pub fn unread(&self) -> usize {
        self.messages.iter().filter(|m| !m.read).count()
    }

    pub fn get(&self, id: u64) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    pub fn event(&self, id: u64) -> Option<&Event> {
        self.events.iter().find(|e| e.id == id)
    }

    /// A message marked read (or unread again).
    pub fn mark(&mut self, id: u64, read: bool) {
        if let Some(m) = self.messages.iter_mut().find(|m| m.id == id) {
            m.read = read;
        }
    }

    /// Every message of `kinds` (all: empty) marked read.
    pub fn mark_all(&mut self, kinds: &[Kind]) {
        for m in self.messages.iter_mut().filter(|m| kinds.is_empty() || kinds.contains(&m.kind)) {
            m.read = true;
        }
    }

    pub fn delete(&mut self, id: u64) {
        self.messages.retain(|m| m.id != id);
    }

    /// The read messages of `kinds` (all: empty) deleted.
    pub fn delete_read(&mut self, kinds: &[Kind]) {
        self.messages.retain(|m| !m.read || !(kinds.is_empty() || kinds.contains(&m.kind)));
    }
}

/// Put a message in the inbox (dated today unless it has a date). One with a key replaces an
/// unread one of the same key; the oldest read ones go when there are more than `KEPT`.
pub fn post(c: &mut Company, mut m: Message) {
    if m.date.is_empty() {
        m.date = c.date.clone();
    }
    if !m.key.is_empty() {
        c.inbox.messages.retain(|x| x.read || x.key != m.key);
    }
    c.inbox.next_id += 1;
    m.id = c.inbox.next_id;
    c.inbox.messages.push(m);
    while c.inbox.messages.len() > KEPT {
        let at = c.inbox.messages.iter().position(|x| x.read).unwrap_or(0);
        c.inbox.messages.remove(at);
    }
}

/// Messages not read yet (for a badge on the company's pages).
pub fn unread(c: &Company) -> usize {
    c.inbox.unread()
}

// --- the world's events -----------------------------------------------------------------------

/// An event of the world: its days and what it does while it runs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Event {
    pub id: u64,
    /// Its kind (the catalogue's key: "fuel_shortage", "fair", ...).
    pub key: String,
    pub from: String,
    pub until: String,
    /// The diesel's price and how many ride, against the usual (1: as ever), and the chance
    /// a trip runs late, added.
    #[serde(default = "one")]
    pub fuel: f64,
    #[serde(default = "one")]
    pub demand: f64,
    #[serde(default)]
    pub late: f64,
    /// Only on this line (its number); None: everywhere.
    #[serde(default)]
    pub line: Option<String>,
    /// Its start and its end were told.
    #[serde(default)]
    pub started: bool,
    #[serde(default)]
    pub ended: bool,
}

fn one() -> f64 {
    1.0
}

impl Event {
    /// It runs on `date`.
    pub fn on(&self, date: &str) -> bool {
        self.from.as_str() <= date && date <= self.until.as_str()
    }

    /// Its days.
    pub fn days(&self) -> i64 {
        dates::between(&self.from, &self.until) + 1
    }

    /// Its title (English) in the catalogue.
    pub fn title(&self) -> &'static str {
        template(&self.key).map(|t| t.title).unwrap_or("Events")
    }
}

/// A kind of event: what it is called and says, when it comes, how long it lasts and what it
/// does (each drawn between its two ends).
struct Template {
    key: &'static str,
    title: &'static str,
    /// Said when it is told (`%{from}`, `%{until}`, `%{n}` the line, `%{price}` and
    /// `%{price_after}` the diesel's price a litre).
    text: &'static str,
    /// How often against the others.
    weight: f64,
    /// The months it comes in (empty: all year), and whether it starts only on a weekend.
    months: &'static [u32],
    weekend: bool,
    days: (i64, i64),
    fuel: (f64, f64),
    demand: (f64, f64),
    late: (f64, f64),
    /// On one of the company's lines.
    line: bool,
}

const NONE: (f64, f64) = (1.0, 1.0);
const NO_DELAY: (f64, f64) = (0.0, 0.0);

/// The events of the world.
const CATALOGUE: &[Template] = &[
    Template { key: "fuel_shortage", title: "Fuel price change", text: "Due to a supply shortage at the refineries, diesel costs %{price_after} a litre instead of %{price} from %{from} until %{until}.", weight: 1.2, months: &[], weekend: false, days: (3, 5), fuel: (1.30, 1.50), demand: NONE, late: NO_DELAY, line: false },
    Template { key: "fuel_drop", title: "Diesel gets cheaper", text: "The oil price has fallen: from %{from} until %{until} diesel costs %{price_after} a litre instead of %{price}.", weight: 0.8, months: &[], weekend: false, days: (4, 8), fuel: (0.80, 0.90), demand: NONE, late: NO_DELAY, line: false },
    Template { key: "environment_days", title: "Environment days", text: "From %{from} until %{until} the town asks people to leave their cars at home: more passengers are to be expected. Make sure there are enough buses, and use extra ones if you can.", weight: 1.0, months: &[4, 5, 6, 9, 10], weekend: false, days: (2, 4), fuel: NONE, demand: (1.12, 1.22), late: NO_DELAY, line: false },
    Template { key: "fair", title: "Trade fair", text: "A trade fair comes to town from %{from} until %{until}: many of its visitors take the bus.", weight: 1.0, months: &[1, 2, 3, 4, 5, 9, 10, 11], weekend: false, days: (2, 4), fuel: NONE, demand: (1.15, 1.30), late: NO_DELAY, line: false },
    Template { key: "football", title: "Football match", text: "A big match on %{from}: the fans fill the buses before and after the game.", weight: 1.0, months: &[1, 2, 3, 4, 5, 8, 9, 10, 11, 12], weekend: true, days: (1, 1), fuel: NONE, demand: (1.25, 1.40), late: (0.02, 0.04), line: false },
    Template { key: "christmas_market", title: "Christmas market", text: "The Christmas market is open from %{from} until %{until}: busy buses in the afternoons and evenings.", weight: 3.0, months: &[11, 12], weekend: false, days: (5, 9), fuel: NONE, demand: (1.12, 1.22), late: NO_DELAY, line: false },
    Template { key: "rail_strike", title: "Railway strike", text: "The railway workers are on strike from %{from} until %{until}: their passengers take the bus.", weight: 0.6, months: &[], weekend: false, days: (1, 3), fuel: NONE, demand: (1.30, 1.50), late: (0.02, 0.05), line: false },
    Template { key: "town_festival", title: "Town festival", text: "The town celebrates from %{from} until %{until}: more people are out and about, and some streets are closed.", weight: 1.0, months: &[5, 6, 7, 8, 9], weekend: false, days: (2, 3), fuel: NONE, demand: (1.15, 1.25), late: (0.03, 0.05), line: false },
    Template { key: "concert", title: "Open-air concert", text: "An open-air concert on %{from} brings crowds into town.", weight: 0.8, months: &[6, 7, 8], weekend: true, days: (1, 1), fuel: NONE, demand: (1.20, 1.30), late: NO_DELAY, line: false },
    Template { key: "marathon", title: "Town marathon", text: "The marathon on %{from} closes the streets in the centre: the buses run late, and many come to watch.", weight: 0.5, months: &[4, 5, 9, 10], weekend: true, days: (1, 1), fuel: NONE, demand: (1.05, 1.12), late: (0.08, 0.12), line: false },
    Template { key: "tourists", title: "Tourist season", text: "From %{from} until %{until} the town is full of tourists: more passengers on the buses.", weight: 0.8, months: &[7, 8], weekend: false, days: (7, 14), fuel: NONE, demand: (1.06, 1.12), late: NO_DELAY, line: false },
    Template { key: "school_holidays", title: "School holidays", text: "The schools are closed from %{from} until %{until}: fewer pupils on the buses.", weight: 1.2, months: &[2, 4, 7, 8, 10, 12], weekend: false, days: (7, 14), fuel: NONE, demand: (0.80, 0.90), late: NO_DELAY, line: false },
    Template { key: "heat_wave", title: "Heat wave", text: "Temperatures above 35 degrees from %{from} until %{until}: fewer people go out.", weight: 1.0, months: &[6, 7, 8], weekend: false, days: (3, 6), fuel: NONE, demand: (0.85, 0.93), late: NO_DELAY, line: false },
    Template { key: "storm", title: "Storm warning", text: "A storm sweeps over the region from %{from} until %{until}: fewer passengers, fallen trees and delays.", weight: 0.8, months: &[1, 2, 3, 10, 11, 12], weekend: false, days: (1, 2), fuel: NONE, demand: (0.75, 0.88), late: (0.08, 0.15), line: false },
    Template { key: "snow", title: "Snow and ice", text: "Snow and black ice from %{from} until %{until}: the buses run slowly and late, and fewer people go out.", weight: 1.2, months: &[12, 1, 2], weekend: false, days: (2, 4), fuel: NONE, demand: (0.85, 0.95), late: (0.10, 0.20), line: false },
    Template { key: "roadworks", title: "Roadworks on line %{n}", text: "Roadworks on the route of line %{n} from %{from} until %{until}: its buses are held up.", weight: 1.2, months: &[3, 4, 5, 6, 7, 8, 9, 10], weekend: false, days: (5, 12), fuel: NONE, demand: NONE, late: (0.10, 0.18), line: true },
    Template { key: "diversion", title: "Diversion on line %{n}", text: "A street on line %{n} is closed from %{from} until %{until}: its buses take a diversion, run late and miss some of their passengers.", weight: 0.8, months: &[], weekend: false, days: (3, 8), fuel: NONE, demand: (0.80, 0.90), late: (0.06, 0.12), line: true },
];

fn template(key: &str) -> Option<&'static Template> {
    CATALOGUE.iter().find(|t| t.key == key)
}

/// The diesel's usual price a litre today (cents), before the events.
pub fn usual_diesel(c: &Company) -> f64 {
    economy::DIESEL_PER_L * c.price_index
}

/// The event (if any) that starts on the day `start` (a day number), and how many days before
/// it is told: drawn from the company and the day, so the same day draws the same.
fn drawn(c: &Company, start: i64) -> Option<(i64, Event)> {
    let mut rng = Rng::of(&[&c.id, "events"], start);
    if !rng.chance(EVENT_CHANCE) {
        return None;
    }
    let lead = rng.int(LEAD.0, LEAD.1);
    let (_, month, _) = dates::civil_from_days(start);
    let weekend = dates::weekday(start) >= 5;
    let mut lines: Vec<String> = c.lines.iter().map(|l| l.number.clone()).collect();
    lines.sort();
    lines.dedup();
    let fits: Vec<&Template> = CATALOGUE.iter().filter(|t| (t.months.is_empty() || t.months.contains(&month)) && (!t.weekend || weekend) && (!t.line || !lines.is_empty())).collect();
    let all: f64 = fits.iter().map(|t| t.weight).sum();
    if all <= 0.0 {
        return None;
    }
    let mut x = rng.range(0.0, all);
    let t = *fits.iter().find(|t| {
        x -= t.weight;
        x < 0.0
    })?;
    let days = rng.int(t.days.0, t.days.1);
    let round = |v: f64| (v * 100.0).round() / 100.0;
    let fuel = round(rng.range(t.fuel.0, t.fuel.1));
    let demand = round(rng.range(t.demand.0, t.demand.1));
    let late = round(rng.range(t.late.0, t.late.1));
    let line = if t.line { rng.pick(&lines).cloned() } else { None };
    let from = dates::fmt(start);
    let until = dates::fmt(start + days - 1);
    Some((lead, Event { id: 0, key: t.key.to_string(), from, until, fuel, demand, late, line, started: false, ended: false }))
}

/// How much an event matters while it runs (its colour).
fn level_of(e: &Event) -> Level {
    if e.fuel > 1.0 || e.demand < 1.0 || e.late > 0.0 {
        Level::Warn
    } else if e.fuel < 1.0 {
        Level::Good
    } else {
        Level::Info
    }
}

/// An event's message: its title, `text`, its days and places.
fn event_message(c: &Company, e: &Event, title: &str, text: &str, date: &str) -> Message {
    let mut m = Message::new(Kind::Event, title, text).days(&e.from, &e.until).dated(date).arg("from", e.from.clone()).arg("until", e.until.clone());
    m.event = Some(e.id);
    if let Some(n) = &e.line {
        m = m.arg("n", n.clone());
    }
    if e.fuel != 1.0 {
        let usual = usual_diesel(c);
        m = m.arg("price", format!("{usual:.1}")).arg("price_after", format!("{:.1}", usual * e.fuel));
    }
    m
}

/// An event taken in (unless one of its kind overlaps it, or too many run then) and told,
/// dated `told`.
fn add_event(c: &mut Company, mut e: Event, told: &str) -> bool {
    let overlaps = |x: &Event| x.from <= e.until && e.from <= x.until;
    if c.inbox.events.iter().any(|x| x.key == e.key && overlaps(x)) || c.inbox.events.iter().filter(|x| overlaps(x)).count() >= EVENTS_AT_ONCE {
        return false;
    }
    let Some(t) = template(&e.key) else { return false };
    c.inbox.next_event += 1;
    e.id = c.inbox.next_event;
    let m = event_message(c, &e, t.title, t.text, told).level(level_of(&e));
    c.inbox.events.push(e);
    post(c, m);
    true
}

/// The event's start or end told (dated `date`).
fn tell_event(c: &mut Company, e: &Event, start: bool, date: &str) {
    let Some(t) = template(&e.key) else { return };
    let text = match (start, e.days()) {
        (true, 1) => "It is today.",
        (true, _) => "It begins today and lasts until %{until}.",
        (false, _) => "It is over: everything is as usual again.",
    };
    let level = if start { level_of(e) } else { Level::Minor };
    let m = event_message(c, e, t.title, text, date).level(level);
    post(c, m);
}

/// The events running on `date`.
pub fn running<'a>(c: &'a Company, date: &'a str) -> impl Iterator<Item = &'a Event> + 'a {
    c.inbox.events.iter().filter(move |e| e.on(date))
}

/// The diesel's price on `date` against the usual (1: as ever): the supplier's fixed price
/// when the company has one (`contracts`), else the market's, as its events make it.
pub fn fuel_price_factor(c: &Company, date: &str) -> f64 {
    if let Some(f) = super::contracts::fixed_fuel(c, date) {
        return f;
    }
    market_fuel_factor(c, date)
}

/// The market's diesel price on `date` against the usual, from the events running then.
pub fn market_fuel_factor(c: &Company, date: &str) -> f64 {
    running(c, date).map(|e| e.fuel).product()
}

/// How many ride on `date` against the usual (1: as ever), from the events running then
/// everywhere (those on a line: `line_effect`).
pub fn demand_factor(c: &Company, date: &str) -> f64 {
    running(c, date).filter(|e| e.line.is_none()).map(|e| e.demand).product()
}

/// What the events running on `date` do to the line `number`: the chance a trip runs late,
/// added (the weather's everywhere, roadworks on the line), and how many ride on it against
/// the usual (a diversion).
pub fn line_effect(c: &Company, date: &str, number: &str) -> (f64, f64) {
    let mut late = 0.0;
    let mut demand = 1.0;
    for e in running(c, date) {
        match &e.line {
            None => late += e.late,
            Some(n) if n.eq_ignore_ascii_case(number) => {
                late += e.late;
                demand *= e.demand;
            }
            Some(_) => {}
        }
    }
    (late, demand)
}

/// The inbox's first day: a word of welcome, the world's events already running or coming in
/// the next days, and the week's goals on offer. Once per company (an older company gets it
/// when its inbox is first looked at, or at its next close).
pub fn welcome(c: &mut Company) {
    if c.inbox.welcomed {
        return;
    }
    c.inbox.welcomed = true;
    let date = c.date.clone();
    if let Some(today) = dates::parse(&date) {
        for x in (today - 6)..=(today + LEAD.1) {
            if let Some((_, mut e)) = drawn(c, x) {
                if e.until.as_str() >= date.as_str() {
                    e.started = e.from.as_str() <= date.as_str();
                    add_event(c, e, &date);
                }
            }
        }
    }
    super::contracts::refresh(c, &date);
    // (the welcome last: the newest, on top of the list)
    post(c, Message::new(Kind::Notice, WELCOME.0, WELCOME.1));
}

const WELCOME: (&str, &str) = (
    "Welcome to your inbox",
    "Here your company tells you what happens: events in town that bring more or fewer passengers or change the diesel's price, your staff, your buses and the workshop, your money, and the goals and contracts you can take on. A new message is marked until you have read it.",
);

/// The inbox's day (`day::close_day`): what the day's close knows told, the events that start
/// tomorrow and those over today told (dated tomorrow, the morning's news), and the events of
/// the coming days drawn.
pub fn day(c: &mut Company, date: &str, report: &mut DayReport) {
    welcome(c);
    bridge(c, date, report);
    let tomorrow = dates::add(date, 1);
    let ended: Vec<Event> = c.inbox.events.iter().filter(|e| !e.ended && e.until.as_str() <= date).cloned().collect();
    for e in ended {
        tell_event(c, &e, false, &tomorrow);
        if let Some(x) = c.inbox.events.iter_mut().find(|x| x.id == e.id) {
            (x.ended, x.started) = (true, true);
        }
    }
    let starting: Vec<Event> = c.inbox.events.iter().filter(|e| !e.started && e.from.as_str() <= tomorrow.as_str()).cloned().collect();
    for e in starting {
        tell_event(c, &e, true, &tomorrow);
        if let Some(x) = c.inbox.events.iter_mut().find(|x| x.id == e.id) {
            x.started = true;
        }
    }
    if let Some(d) = dates::parse(date) {
        for x in (d + LEAD.0)..=(d + LEAD.1) {
            if let Some((lead, e)) = drawn(c, x) {
                if lead == x - d && !c.inbox.events.iter().any(|y| y.key == e.key && y.from == e.from) {
                    add_event(c, e, &tomorrow);
                }
            }
        }
    }
    let gone = dates::add(date, -EVENTS_KEPT);
    c.inbox.events.retain(|e| e.until.as_str() >= gone.as_str());
}

// --- what the day's close knows ----------------------------------------------------------------

fn arg(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

/// What the day's close knows told: its notes and its staff's notes as messages (dated the day
/// closed); the day's tours not covered, penalties, fines, complaints and full buses (one
/// message each, replaced while unread); the buses that go back tonight, the loan rates of a
/// month's end, and the cash when it runs low.
pub fn bridge(c: &mut Company, date: &str, report: &DayReport) {
    let mut out: Vec<Message> = Vec::new();
    let mut add = |kind: Kind, level: Level, key: &str, (title, text): (&str, &str), args: Vec<(String, String)>| {
        let mut m = Message::new(kind, title, text).level(level).dated(date).key(key);
        m.args = args;
        out.push(m);
    };
    for n in &report.notes {
        match n {
            Note::Breakdown { number, until, cost } => add(Kind::Fleet, Level::Bad, "", BREAKDOWN, vec![arg("n", number), arg("date", until), arg("amount", cost)]),
            Note::Service { number } => add(Kind::Workshop, Level::Info, "", SERVICE, vec![arg("n", number)]),
            Note::Returned { number, name } => add(Kind::Fleet, Level::Info, "", RETURNED, vec![arg("n", number), arg("name", name)]),
            Note::LoanPaid { purpose } => add(Kind::Finance, Level::Good, "", LOAN_PAID, vec![arg("what", purpose)]),
            Note::Month { month, result } => add(Kind::Finance, if *result >= 0 { Level::Good } else { Level::Warn }, "", MONTH, vec![arg("month", month), arg("amount", result)]),
            Note::Built { area } => add(Kind::Notice, Level::Good, "", BUILT, vec![arg("what", area)]),
            Note::BayWait { number } => add(Kind::Workshop, Level::Warn, "", BAY_WAIT, vec![arg("n", number)]),
            Note::Won { number, until } => add(Kind::Lines, Level::Good, "", WON, vec![arg("n", number), arg("date", until)]),
            Note::Lost { number, winner } => add(Kind::Lines, Level::Warn, "", LOST, vec![arg("n", number), arg("who", winner)]),
            Note::Ended { number } => add(Kind::Lines, Level::Bad, "", ENDED, vec![arg("n", number)]),
            Note::NotStarted { number, days, charge } => add(Kind::Lines, Level::Bad, "", NOT_STARTED, vec![arg("n", number), arg("days", days), arg("amount", charge)]),
            Note::Accident { number, cost } => add(Kind::Fleet, Level::Bad, "", ACCIDENT, vec![arg("n", number), arg("amount", cost)]),
        }
    }
    for s in &report.staff {
        match s {
            StaffNote::Sick { name, until } => add(Kind::Staff, Level::Warn, "", ILL, vec![arg("name", name), arg("date", until)]),
            StaffNote::Holiday { name, until } => add(Kind::Staff, Level::Minor, "", HOLIDAY, vec![arg("name", name), arg("date", until)]),
            StaffNote::Resigned { name, until } => add(Kind::Staff, Level::Bad, "", RESIGNED, vec![arg("name", name), arg("date", until)]),
            StaffNote::Unhappy { name } => add(Kind::Staff, Level::Warn, "", UNHAPPY, vec![arg("name", name)]),
            StaffNote::Left { name } => add(Kind::Staff, Level::Info, "", LEFT, vec![arg("name", name)]),
        }
    }
    // (the buses whose lease or rental ends tonight: the close lets them go after the inbox's
    // turn)
    let tomorrow = dates::add(date, 1);
    let back: Vec<(String, String)> = c.fleet.iter().filter(|v| v.held_on(date) && !v.held_on(&tomorrow)).map(|v| (v.number.clone(), v.name.clone())).collect();
    for (number, name) in back {
        if !report.notes.iter().any(|n| matches!(n, Note::Returned { number: x, .. } if *x == number)) {
            add(Kind::Fleet, Level::Info, "", RETURNED, vec![arg("n", number), arg("name", name)]);
        }
    }
    // (the loan rates of a month's end)
    if report.notes.iter().any(|n| matches!(n, Note::Month { .. })) {
        let today = || c.ledger.iter().rev().take_while(|b| b.date == date);
        let rates: i64 = -today().filter(|b| matches!(b.kind, BookingKind::Interest | BookingKind::Repayment)).map(|b| b.amount).sum::<i64>();
        let interest: i64 = -today().filter(|b| b.kind == BookingKind::Interest).map(|b| b.amount).sum::<i64>();
        if rates > 0 {
            add(Kind::Finance, Level::Info, "", RATES, vec![arg("amount", rates), arg("cost", interest)]);
        }
    }
    if report.uncovered > 0 {
        add(Kind::Lines, Level::Bad, "uncovered", UNCOVERED, vec![arg("trips", report.dropped), arg("tours", report.uncovered), arg("buses", report.short_buses), arg("duties", report.short_drivers)]);
    }
    if report.penalties > 0 {
        add(Kind::Finance, Level::Warn, "penalties", PENALTIES, vec![arg("amount", report.penalties)]);
    }
    if report.incidents.fines > 0 {
        add(Kind::Staff, Level::Warn, "fines", FINES, vec![arg("n", report.incidents.fines), arg("amount", report.incidents.fine_cost)]);
    }
    if report.incidents.complaints > 0 {
        add(Kind::Notice, Level::Warn, "complaints", COMPLAINTS, vec![arg("n", report.incidents.complaints)]);
    }
    if report.crowded > 0 {
        add(Kind::Lines, Level::Warn, "crowded", CROWDED, vec![arg("n", report.crowded), arg("p", report.left_behind)]);
    }
    // (the cash: under a month's wages, or gone)
    let wages: i64 = c.staff.iter().map(super::staff::monthly_cost).sum();
    if c.cash < 0 {
        add(Kind::Finance, Level::Bad, "cash", IN_THE_RED, vec![arg("amount", c.cash)]);
    } else if c.cash < wages {
        add(Kind::Finance, Level::Warn, "cash", LOW_CASH, vec![arg("amount", c.cash), arg("cost", wages)]);
    }
    for m in out {
        post(c, m);
    }
}

const BREAKDOWN: (&str, &str) = ("Breakdown", "Bus %{n} broke down on its tour: in the workshop until %{date}, repairs %{amount}.");
const SERVICE: (&str, &str) = ("Service due", "Bus %{n} is due for its service: in the workshop tomorrow.");
const RETURNED: (&str, &str) = ("A bus went back", "Bus %{n} (%{name}) went back: its lease or rental ended.");
const LOAN_PAID: (&str, &str) = ("Loan paid off", "A loan is paid off: %{what}.");
const MONTH: (&str, &str) = ("The month is closed", "%{month} is closed: wages, leases, insurance, the depot and loan rates are booked. The month's result: %{amount}.");
const BUILT: (&str, &str) = ("Building work done", "The depot's building work is done: %{what}.");
const BAY_WAIT: (&str, &str) = ("Waiting for a workshop bay", "Bus %{n} waits for a free workshop bay.");
const WON: (&str, &str) = ("Concession won", "The concession for line %{n} is yours until %{date}. Plan its tours and start its service.");
const LOST: (&str, &str) = ("Tender lost", "%{who} won the tender for line %{n}.");
const ENDED: (&str, &str) = ("Concession ended", "The concession for line %{n} has ended: the line is no longer yours.");
const NOT_STARTED: (&str, &str) = ("Line not in service", "Line %{n} is still not in service, %{days} days after it was taken on: the authority charged %{amount}. Plan its tours and start its service.");
const ACCIDENT: (&str, &str) = ("Accident", "Bus %{n} had an accident on its tour: damage %{amount}.");
const ILL: (&str, &str) = ("Ill", "%{name} is ill until %{date}.");
const HOLIDAY: (&str, &str) = ("On holiday", "%{name} is on holiday until %{date}.");
const RESIGNED: (&str, &str) = ("Notice handed in", "%{name} has resigned; their last day is %{date}.");
const UNHAPPY: (&str, &str) = ("Unhappy", "%{name} is unhappy with their pay or their hours.");
const LEFT: (&str, &str) = ("Left the company", "%{name} has left the company.");
const RATES: (&str, &str) = ("Loan rates paid", "The month's loan rates were paid: %{amount}, of which interest %{cost}.");
const UNCOVERED: (&str, &str) = ("Tours not covered", "%{trips} trips dropped: %{tours} tours were not covered - %{buses} without a bus, %{duties} duties without a driver.");
const PENALTIES: (&str, &str) = ("Contract penalties", "Contract penalties %{amount}: the authority charges for every trip dropped or late, and for a concession not started.");
const FINES: (&str, &str) = ("Traffic fines", "Your drivers were fined %{n} times in traffic: %{amount}. Defensive driving halves it.");
const COMPLAINTS: (&str, &str) = ("Passenger complaints", "%{n} passengers complained about their trip. The customer service course halves the complaints.");
const CROWDED: (&str, &str) = ("Full buses", "%{n} trips of your own lines were full; %{p} passengers were left at the stop. Bigger buses help.");
const IN_THE_RED: (&str, &str) = ("The company is in the red", "The company has %{amount} in cash: it is overdrawn. Take a loan, sell a bus or cut costs before the wages are due.");
const LOW_CASH: (&str, &str) = ("Low cash", "The company has %{amount} in cash, less than a month's wages (%{cost}).");

/// Every English text the inbox writes (titles and texts), for the translations' check.
pub fn texts() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = CATALOGUE.iter().flat_map(|t| [t.title, t.text]).collect();
    out.extend(Kind::ALL.iter().map(|k| k.label()));
    out.extend(["It is today.", "It begins today and lasts until %{until}.", "It is over: everything is as usual again."]);
    for (a, b) in [
        WELCOME, BREAKDOWN, SERVICE, RETURNED, LOAN_PAID, MONTH, BUILT, BAY_WAIT, WON, LOST, ENDED, NOT_STARTED, ACCIDENT, ILL, HOLIDAY, RESIGNED, UNHAPPY, LEFT, RATES, UNCOVERED, PENALTIES, FINES, COMPLAINTS, CROWDED, IN_THE_RED, LOW_CASH,
    ] {
        out.extend([a, b]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::company::model::{CompanyLine, Difficulty};

    fn company() -> Company {
        let mut c = found(&Founding { name: "Post".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), added: c.date.clone(), service_from: Some(0), ..Default::default() });
        c
    }

    /// The company's days closed by the inbox alone, from its date for `n` days.
    fn run(c: &mut Company, n: i64) {
        for _ in 0..n {
            let date = c.date.clone();
            let mut r = DayReport { date: date.clone(), ..Default::default() };
            day(c, &date, &mut r);
            c.date = dates::add(&date, 1);
        }
    }

    #[test]
    fn a_message_is_posted_replaced_and_kept_within_bounds() {
        let mut c = company();
        post(&mut c, Message::new(Kind::Staff, "Ill", "%{name} is ill until %{date}.").arg("name", "Anna").arg("date", "2024-03-06"));
        let m = c.inbox.messages.last().unwrap();
        assert_eq!((m.id, m.date.as_str(), m.level, m.read), (1, "2024-03-04", Level::Info, false));
        assert_eq!(fill(&m.text, &m.args), "Anna is ill until 2024-03-06.");
        // a warning with a key replaces its unread one, not a read one
        post(&mut c, Message::new(Kind::Finance, "Low cash", "a").key("cash"));
        post(&mut c, Message::new(Kind::Finance, "Low cash", "b").key("cash"));
        assert_eq!(c.inbox.messages.iter().filter(|m| m.key == "cash").count(), 1);
        let id = c.inbox.messages.last().unwrap().id;
        c.inbox.mark(id, true);
        post(&mut c, Message::new(Kind::Finance, "Low cash", "c").key("cash"));
        assert_eq!(c.inbox.messages.iter().filter(|m| m.key == "cash").count(), 2);
        assert_eq!(c.inbox.unread(), 2);
        // never more than KEPT: the read ones go first
        for k in 0..KEPT + 20 {
            post(&mut c, Message::new(Kind::Notice, "n", k.to_string()));
        }
        assert_eq!(c.inbox.messages.len(), KEPT);
        assert!(c.inbox.messages.iter().all(|m| !m.read));
        c.inbox.mark_all(&[Kind::Notice]);
        c.inbox.delete_read(&[]);
        assert!(c.inbox.messages.iter().all(|m| m.kind != Kind::Notice));
    }

    #[test]
    fn an_older_inbox_loads() {
        // (the first round's file: no level, no events)
        let json = r#"{"messages":[{"id":3,"date":"2024-01-02","kind":"staff","title":"x","text":"y"}],"next_id":3}"#;
        let i: Inbox = serde_json::from_str(json).unwrap();
        assert_eq!(i.messages[0].level, Level::Info);
        assert!(i.events.is_empty() && !i.welcomed && i.messages[0].goal.is_none());
        let e: Event = serde_json::from_str(r#"{"id":1,"key":"fair","from":"2024-01-02","until":"2024-01-03"}"#).unwrap();
        assert_eq!((e.fuel, e.demand, e.late), (1.0, 1.0, 0.0));
        // a message is written without an empty key
        let m = serde_json::to_string(&Message::new(Kind::Notice, "a", "b")).unwrap();
        assert!(!m.contains("\"key\""));
    }

    #[test]
    fn events_come_told_ahead_and_the_same_on_the_same_days() {
        let mut a = company();
        let mut b = company();
        run(&mut a, 120);
        run(&mut b, 120);
        assert_eq!(a.inbox.events, b.inbox.events);
        assert_eq!(a.inbox.messages, b.inbox.messages);
        // (about one event a week; those over for a month are let go)
        let mut told: Vec<u64> = a.inbox.messages.iter().filter_map(|m| m.event).collect();
        told.dedup();
        assert!(told.len() >= 10, "{}", told.len());
        assert!(a.inbox.events.iter().all(|e| e.until.as_str() >= dates::add(&a.date, -EVENTS_KEPT - 1).as_str()));
        // each told before it started, again when it started and when it was over (the
        // welcome's own excepted: it may have run already)
        let first = dates::add("2024-03-04", LEAD.1);
        for e in a.inbox.events.iter().filter(|e| e.until.as_str() < a.date.as_str() && e.from > first) {
            let told: Vec<&Message> = a.inbox.messages.iter().filter(|m| m.event == Some(e.id)).collect();
            assert_eq!(told.len(), 3, "{e:?}");
            assert!(told[0].date < e.from, "{e:?}");
            assert_eq!(told[1].date, e.from);
            assert_eq!(told[2].date, dates::add(&e.until, 1));
            assert!(e.started && e.ended);
        }
        // never two of a kind at once, never more than three
        for d in 0..120 {
            let date = dates::add("2024-03-04", d);
            let on: Vec<&Event> = running(&a, &date).collect();
            assert!(on.len() <= EVENTS_AT_ONCE);
            for (i, x) in on.iter().enumerate() {
                assert!(on[i + 1..].iter().all(|y| y.key != x.key));
            }
        }
        // the season: no Christmas market from March to June
        assert!(a.inbox.events.iter().all(|e| e.key != "christmas_market"));
        // another company draws other days
        let mut other = company();
        other.id = "another".into();
        run(&mut other, 120);
        assert_ne!(other.inbox.events.iter().map(|e| &e.from).collect::<Vec<_>>(), a.inbox.events.iter().map(|e| &e.from).collect::<Vec<_>>());
    }

    #[test]
    fn a_running_event_moves_the_fuel_the_riders_and_the_line() {
        let mut c = company();
        let mut e = |key: &str, from: &str, until: &str, fuel: f64, demand: f64, late: f64, line: Option<&str>| {
            c.inbox.next_event += 1;
            let id = c.inbox.next_event;
            c.inbox.events.push(Event { id, key: key.into(), from: from.into(), until: until.into(), fuel, demand, late, line: line.map(Into::into), started: false, ended: false });
        };
        e("fuel_shortage", "2024-03-08", "2024-03-11", 1.44, 1.0, 0.0, None);
        e("fair", "2024-03-10", "2024-03-10", 1.0, 1.2, 0.0, None);
        e("roadworks", "2024-03-01", "2024-03-20", 1.0, 1.0, 0.15, Some("5"));
        e("diversion", "2024-03-01", "2024-03-20", 1.0, 0.8, 0.1, Some("7"));
        assert_eq!(fuel_price_factor(&c, "2024-03-07"), 1.0);
        assert!((fuel_price_factor(&c, "2024-03-08") - 1.44).abs() < 1e-9);
        assert!((demand_factor(&c, "2024-03-10") - 1.2).abs() < 1e-9);
        assert_eq!(demand_factor(&c, "2024-03-11"), 1.0);
        assert_eq!(line_effect(&c, "2024-03-10", "5"), (0.15, 1.0));
        assert_eq!(line_effect(&c, "2024-03-10", "7"), (0.1, 0.8));
        assert_eq!(line_effect(&c, "2024-03-25", "5"), (0.0, 1.0));
    }

    #[test]
    fn a_day_closed_feels_the_events() {
        use super::super::day::close_day;
        use super::super::market::{self, MarketBus, Payment};
        use super::super::network::{tests::trip, TourOfDay};
        use super::super::staff::{applicants, hire};
        let mut c = company();
        c.planning.auto = true;
        c.cash += 5_000_000_00;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..2 {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        let id = c.id.clone();
        while c.staff.len() < 4 {
            for a in applicants(&c) {
                if c.staff.len() < 4 && a.licence == crate::company::model::Licence::D {
                    hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until) = (None, None);
        }
        c.inbox.welcomed = true;
        let tours = || (0..2).map(|k| TourOfDay { line: "Linie5".into(), number: "5".into(), tour: (k + 1).to_string(), ai_group: String::new(), trips: (0..10).map(|n| trip(6 * 60 + k * 30 + n * 60, 6 * 60 + k * 30 + n * 60 + 50, 12)).collect(), unplanned: false, cuts: None }).collect::<Vec<_>>();
        let mut plain = c.clone();
        let base = close_day(&mut plain, tours(), &[]);
        let date = c.date.clone();
        let event = |key: &str, fuel: f64, demand: f64, late: f64| Event { id: 0, key: key.into(), from: date.clone(), until: date.clone(), fuel, demand, late, line: (key == "roadworks").then(|| "5".to_string()), started: true, ended: false };
        c.inbox.events = vec![event("fuel_shortage", 1.5, 1.0, 0.0), event("fair", 1.0, 1.3, 0.0), event("roadworks", 1.0, 1.0, 0.6)];
        let busy = close_day(&mut c, tours(), &[]);
        let energy = |c: &Company| -c.month(&date[..7]).get(BookingKind::Energy) as f64;
        // the diesel at half as much again, more riders, trips late on the roadworks' line
        assert!((energy(&c) / energy(&plain) - 1.5).abs() < 0.01, "{} {}", energy(&c), energy(&plain));
        assert!(busy.passengers as f64 > base.passengers as f64 * 1.2, "{} {}", busy.passengers, base.passengers);
        assert!(busy.late > base.late + 5, "{} {}", busy.late, base.late);
    }

    #[test]
    fn the_days_notes_come_in_as_messages() {
        let mut c = company();
        let date = c.date.clone();
        let mut r = DayReport { date: date.clone(), ..Default::default() };
        r.notes.push(Note::Breakdown { number: "101".into(), until: "2024-03-06".into(), cost: 1_200_00 });
        r.notes.push(Note::Won { number: "7".into(), until: "2030-03-01".into() });
        r.staff.push(StaffNote::Sick { name: "Anna Becker".into(), until: "2024-03-07".into() });
        (r.uncovered, r.dropped, r.short_buses) = (2, 12, 2);
        r.penalties = 960_00;
        day(&mut c, &date, &mut r);
        let has = |kind: Kind, title: &str| c.inbox.messages.iter().any(|m| m.kind == kind && m.title == title && m.date == date);
        assert!(has(Kind::Fleet, "Breakdown") && has(Kind::Lines, "Concession won") && has(Kind::Staff, "Ill"));
        assert!(has(Kind::Lines, "Tours not covered") && has(Kind::Finance, "Contract penalties"));
        let b = c.inbox.messages.iter().find(|m| m.title == "Breakdown").unwrap();
        assert_eq!((b.value("n"), b.value("amount"), b.level), (Some("101"), Some("120000"), Level::Bad));
        // the welcome came before the day's notes, once
        let at = |title: &str| c.inbox.messages.iter().position(|m| m.title == title).unwrap();
        assert!(at(WELCOME.0) < at("Breakdown"));
        // the next day's penalties replace the unread message of the day before
        let mut r2 = DayReport { date: dates::add(&date, 1), penalties: 80_00, ..Default::default() };
        day(&mut c, &dates::add(&date, 1), &mut r2);
        assert_eq!(c.inbox.messages.iter().filter(|m| m.title == "Contract penalties").count(), 1);
        assert_eq!(c.inbox.messages.iter().filter(|m| m.title == WELCOME.0).count(), 1);
        // and the cash: a company in the red is told
        c.cash = -5_00;
        day(&mut c, &dates::add(&date, 2), &mut DayReport::default());
        assert!(c.inbox.messages.iter().any(|m| m.title == IN_THE_RED.0 && m.level == Level::Bad));
    }

    #[test]
    fn every_text_of_the_catalogue_names_its_places() {
        for t in CATALOGUE {
            assert!(t.text.contains("%{from}"), "{}", t.key);
            assert_eq!(t.line, t.title.contains("%{n}") && t.text.contains("%{n}"), "{}", t.key);
            assert_eq!(t.fuel != NONE, t.text.contains("%{price_after}"), "{}", t.key);
            assert!(t.days.0 >= 1 && t.days.0 <= t.days.1);
            assert!(t.days.1 > 1 || !t.text.contains("%{until}"), "{}", t.key);
        }
        assert!(texts().len() > 60);
    }
}
