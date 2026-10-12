//! The company's research: what it has found out and built (buses of other sizes, upgrades
//! of its buses, things for its passengers, its premises, its staff and its workshop), each at
//! a price in money and trophies, worked on by its researchers day by day.
//!
//! As City Bus Manager's research tree: the topics stand in sections, a topic has one stage
//! or several (each stage dearer than the one before), and some need another first (or a
//! company level). One research runs at a time: it is paid when it starts - its money booked
//! as research, its trophies (earned with goals and contracts) spent - and the researchers
//! (`Role::Researcher`, at work that day) work on it every day, the experienced and calm ones
//! more; without researchers nothing moves. A stage done is a key in `Research::done` (the
//! topic's own for a single stage, `<topic>_<n>` for the stages of one with more: "fuel_2"),
//! and works from then on through the functions at the end (`fuel_factor`, `demand_factor`,
//! `late_factor`, `wear_factor`, ...), which the day, the depot, the market and the stores ask.

use super::day::DayReport;
use super::inbox::{self, Kind, Message};
use super::levels::{self, Feature};
use super::model::{BookingKind, BusSize, Cents, Company, Difficulty, Employee, Role};
use serde::{Deserialize, Serialize};

/// The keys of research other parts of the company ask for.
pub mod keys {
    /// WLAN on the buses: the internet contracts (`contracts`) are offered with it.
    pub const WLAN: &str = "wlan";
    /// Ticket inspectors may be hired (`staff`).
    pub const TICKET_INSPECTORS: &str = "ticket_inspectors";
    /// The shift schedule of the other staff (`staff`).
    pub const SHIFT_SCHEDULE: &str = "shift_schedule";
    /// The dealers sell the company articulated buses (`market::kind_allowed`).
    pub const ARTICULATED: &str = "articulated";
    /// The dealers sell the company double-deckers.
    pub const DOUBLE_DECKERS: &str = "double_deckers";
}

/// What the company has researched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Research {
    /// The keys of the research done.
    #[serde(default)]
    pub done: Vec<String>,
    /// The research under way.
    #[serde(default)]
    pub running: Option<Running>,
    /// What an older company could do already was marked done (`migrate`; a company's first
    /// day marks it too).
    #[serde(default)]
    pub checked: bool,
}

impl Research {
    /// Whether the research `key` is done.
    pub fn has(&self, key: &str) -> bool {
        self.done.iter().any(|k| k == key)
    }
}

/// A research under way: a topic's stage, the work it takes (researcher-days) and the work
/// done, and what was paid for it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Running {
    pub topic: String,
    pub stage: u32,
    pub work: f64,
    #[serde(default)]
    pub done: f64,
    pub started: String,
    #[serde(default)]
    pub money: Cents,
    #[serde(default)]
    pub trophies: i64,
    /// The inbox was told that nobody works on it (said once, until work is done again).
    #[serde(default)]
    pub idle_told: bool,
}

impl Running {
    /// How far it is, 0 to 1.
    pub fn share(&self) -> f64 {
        if self.work <= 0.0 {
            1.0
        } else {
            (self.done / self.work).clamp(0.0, 1.0)
        }
    }
}

// --- the tree ------------------------------------------------------------------------------------

/// The sections of the tree.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Buses,
    Upgrades,
    Passengers,
    Premises,
    Staff,
    Workshop,
    Area,
    Tools,
}

impl Section {
    pub const ALL: [Section; 8] = [Section::Buses, Section::Upgrades, Section::Passengers, Section::Premises, Section::Staff, Section::Workshop, Section::Area, Section::Tools];

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Section::Buses => "Buses and their running",
            Section::Upgrades => "Bus upgrades",
            Section::Passengers => "Passenger satisfaction",
            Section::Premises => "Company premises",
            Section::Staff => "Staff and their work",
            Section::Workshop => "Workshop and spare parts",
            Section::Area => "Expand the area of operation",
            Section::Tools => "Additional tools",
        }
    }
}

/// What a topic needs before it can be researched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Another research (its key: a topic's, or a stage's `<topic>_<n>`).
    Done(&'static str),
    /// The company's level (`levels`).
    Level(u32),
}

/// A topic of the tree. Its money, trophies and work are its first stage's (the money at
/// Realistic, at founding); every stage after costs more (`cost`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Topic {
    pub key: &'static str,
    pub section: Section,
    /// Its name and what it does (English, translated when shown).
    pub title: &'static str,
    pub info: &'static str,
    /// The launcher's icon for it.
    pub icon: &'static str,
    pub stages: u32,
    pub money: Cents,
    pub trophies: i64,
    /// Trophies more for every stage after the first.
    pub trophy_step: i64,
    /// Researcher-days its first stage takes.
    pub work: f64,
    pub needs: &'static [Need],
}

#[allow(clippy::too_many_arguments)]
const fn t(key: &'static str, section: Section, title: &'static str, info: &'static str, icon: &'static str, stages: u32, money: Cents, trophies: i64, trophy_step: i64, work: f64, needs: &'static [Need]) -> Topic {
    Topic { key, section, title, info, icon, stages, money, trophies, trophy_step, work, needs }
}

use Section as S;

/// The research tree, section by section.
pub const TOPICS: [Topic; 28] = [
    // the buses
    t(keys::ARTICULATED, S::Buses, "Articulated buses", "Your dealers sell you articulated buses: 18 metres, half as many places again as a solo bus.", "directions_bus", 1, 40_000_00, 0, 0, 6.0, &[Need::Level(2)]),
    t(keys::DOUBLE_DECKERS, S::Buses, "Double-deckers", "Your dealers sell you double-deckers: many seats on the footprint of a solo bus.", "directions_bus", 1, 60_000_00, 25, 0, 10.0, &[Need::Level(4)]),
    t("fuel", S::Buses, "Reduce fuel consumption", "Each stage: the buses use 2 % less diesel and power.", "speed", 5, 15_000_00, 0, 10, 4.0, &[]),
    t("dirt", S::Buses, "Reduce dirt on the buses", "Each stage: the buses get 10 % less dirty a day.", "water_drop", 3, 10_000_00, 5, 10, 4.0, &[]),
    t("wear", S::Buses, "Reduce wear and tear", "Each stage: 5 % less wear and maintenance a kilometre.", "construction", 4, 20_000_00, 0, 10, 5.0, &[]),
    // the buses' upgrades
    t("air_conditioning", S::Upgrades, "Air conditioning", "Cool buses in summer: 2 % more passengers, and a little goodwill every day.", "ac_unit", 1, 25_000_00, 10, 0, 6.0, &[]),
    t("passenger_info", S::Upgrades, "Passenger information system", "Screens with the next stops and the connections: 2 % more passengers.", "departure_board", 1, 30_000_00, 15, 0, 7.0, &[]),
    t(keys::WLAN, S::Upgrades, "WLAN", "Internet on board: 2 % more passengers, and the internet contracts open on the Contracts page.", "wifi", 1, 20_000_00, 20, 0, 6.0, &[Need::Done("passenger_info")]),
    t("usb", S::Upgrades, "USB charging ports", "The passengers charge their phones: 1.5 % more passengers.", "bolt", 1, 15_000_00, 20, 0, 5.0, &[Need::Done("air_conditioning")]),
    t("premium_seats", S::Upgrades, "Premium seats", "Comfortable seats: 3 % more passengers.", "airline_seat_recline_normal", 1, 45_000_00, 35, 0, 9.0, &[Need::Done("usb")]),
    // the passengers
    t("stops", S::Passengers, "Bus stop upgrades", "Each stage: shelters, benches and timetables at the stops: 1.5 % more passengers, and a little goodwill every day.", "location_on", 2, 30_000_00, 15, 15, 6.0, &[]),
    t("app", S::Passengers, "Mobile app", "Each stage: tickets and live departures on the phone: 1 % more passengers.", "smartphone", 3, 25_000_00, 20, 10, 6.0, &[Need::Done("passenger_info")]),
    t("ticket_machines", S::Passengers, "Ticket vending machines", "Each stage: tickets bought before boarding: 1 % more passengers, and 3 % fewer late trips.", "confirmation_number", 3, 35_000_00, 10, 10, 6.0, &[]),
    t("priority", S::Passengers, "Bus priority on the routes", "Each stage: bus lanes and traffic lights that wait for the bus: 10 % fewer late trips.", "traffic", 3, 40_000_00, 25, 15, 8.0, &[Need::Done("stops_1")]),
    // the premises
    t("premises", S::Premises, "Expand the company premises", "Each stage: six more places for buses at the depot.", "garage", 3, 50_000_00, 15, 15, 8.0, &[]),
    t("external_depots", S::Premises, "External depots", "Small yards out on the network: ten more places for buses, and repairs out there.", "home", 1, 80_000_00, 40, 0, 12.0, &[Need::Done("premises_1"), Need::Level(3)]),
    // the staff
    t("auto_cleaning", S::Staff, "Automatic cleaning of the buses", "The cleaners' rounds plan themselves: the fleet never gets dirtier than 55 % clean.", "autorenew", 1, 15_000_00, 10, 0, 5.0, &[]),
    t("auto_washing", S::Staff, "Automatic washing of the buses", "The washing bay runs by itself: it washes half as many buses again a night.", "water_drop", 1, 20_000_00, 15, 0, 6.0, &[Need::Done("auto_cleaning")]),
    t("auto_refuelling", S::Staff, "Automatic refuelling of the buses", "The pumps and the chargers run to a schedule: they serve half as many buses again a night.", "sync_alt", 1, 25_000_00, 20, 0, 7.0, &[Need::Done("auto_cleaning")]),
    t("auto_repair", S::Staff, "Automatic repair at the external depots", "A bus that breaks down is repaired at the nearest external depot: back a day sooner.", "construction", 1, 30_000_00, 30, 0, 8.0, &[Need::Done("external_depots")]),
    t(keys::TICKET_INSPECTORS, S::Staff, "Ticket inspectors", "Ticket inspectors may be hired on the Staff page: fewer passengers ride without a ticket.", "badge", 1, 20_000_00, 10, 0, 5.0, &[]),
    t("inspection", S::Staff, "Working method of the ticket inspectors", "Each stage: the ticket inspectors check 15 % more passengers.", "visibility", 3, 10_000_00, 15, 10, 4.0, &[Need::Done(keys::TICKET_INSPECTORS)]),
    t("food", S::Staff, "Better food for the employees", "Each stage: the canteen cooks better: every day the staff are a little happier and less stressed.", "groups", 3, 12_000_00, 10, 10, 4.0, &[]),
    // the workshop
    t("parts", S::Workshop, "Cheaper spare parts", "Each stage: the spare parts cost 10 % less in the workshop's stores.", "inventory_2", 3, 15_000_00, 0, 15, 5.0, &[]),
    t("repairs", S::Workshop, "Faster repairs", "Each stage: the repairs take 10 % less time.", "timer", 3, 20_000_00, 15, 15, 6.0, &[]),
    // the area
    t("area", S::Area, "More concessions", "Each stage: the authority lets the company hold one concession more.", "map", 3, 60_000_00, 30, 20, 10.0, &[Need::Level(3)]),
    t("park_and_ride", S::Area, "Park and ride", "Car parks at the termini: 2 % more passengers.", "local_parking", 1, 40_000_00, 25, 0, 8.0, &[Need::Done("area_1")]),
    // the tools
    t(keys::SHIFT_SCHEDULE, S::Tools, "Shift schedule", "The shift schedule of the staff besides the drivers: their core hours, and who works outside them.", "calendar_month", 1, 10_000_00, 5, 0, 4.0, &[]),
];

/// The topic of a key: a topic's own, or one of its stages' (`fuel_2`).
pub fn topic(key: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|t| t.key == key).or_else(|| {
        let (base, n) = key.rsplit_once('_')?;
        let n: u32 = n.parse().ok()?;
        TOPICS.iter().find(|t| t.key == base && t.stages > 1 && (1..=t.stages).contains(&n))
    })
}

/// The stage a key names (1 for a single stage's).
pub fn stage_of(key: &str) -> u32 {
    match topic(key) {
        Some(t) if t.key != key => key.rsplit_once('_').and_then(|(_, n)| n.parse().ok()).unwrap_or(1),
        _ => 1,
    }
}

/// The key of a topic's stage: the topic's own for a single stage, `<topic>_<n>` for more.
pub fn stage_key(t: &Topic, stage: u32) -> String {
    if t.stages <= 1 {
        t.key.to_string()
    } else {
        format!("{}_{}", t.key, stage)
    }
}

/// How many stages of a topic are done (one after the other).
pub fn stages_done(r: &Research, t: &Topic) -> u32 {
    (1..=t.stages).take_while(|n| r.has(&stage_key(t, *n))).count() as u32
}

/// How many stages of the topic `key` the company has done.
fn n(c: &Company, key: &str) -> u32 {
    TOPICS.iter().find(|t| t.key == key).map(|t| stages_done(&c.research, t)).unwrap_or(0)
}

/// What a stage costs: money (now, at the company's difficulty), trophies, and the work
/// (researcher-days).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cost {
    pub money: Cents,
    pub trophies: i64,
    pub work: f64,
}

/// What research costs against Realistic.
fn cost_factor(d: Difficulty) -> f64 {
    match d {
        Difficulty::Easy => 0.7,
        Difficulty::Realistic => 1.0,
        Difficulty::Hard => 1.3,
    }
}

/// What a topic's stage costs the company now: every stage after the first 60 % more money
/// and half as much work again.
pub fn cost(c: &Company, t: &Topic, stage: u32) -> Cost {
    let k = stage.max(1) as f64 - 1.0;
    let money = t.money as f64 * (1.0 + 0.6 * k) * cost_factor(c.difficulty) * c.price_index.max(0.0);
    Cost { money: ((money / 100_00 as f64).round() as Cents) * 100_00, trophies: t.trophies + t.trophy_step * k as i64, work: t.work * (1.0 + 0.5 * k) }
}

/// Whether the company has what a need asks.
pub fn met(c: &Company, need: Need) -> bool {
    match need {
        Need::Done(key) => c.research.has(key),
        Need::Level(lv) => levels::level(c) >= lv,
    }
}

/// What the topic still needs before its next stage can start.
pub fn unmet(c: &Company, t: &Topic) -> Vec<Need> {
    t.needs.iter().copied().filter(|n| !met(c, *n)).collect()
}

/// Where a topic stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// All its stages are done.
    Done,
    /// Its next stage is under way.
    Running,
    /// Something it needs is missing.
    Locked,
    /// Its next stage can be started.
    Open,
}

pub fn state(c: &Company, t: &Topic) -> State {
    if stages_done(&c.research, t) >= t.stages {
        State::Done
    } else if c.research.running.as_ref().is_some_and(|r| r.topic == t.key) {
        State::Running
    } else if !unmet(c, t).is_empty() {
        State::Locked
    } else {
        State::Open
    }
}

/// Start the next stage of a topic: its money booked, its trophies spent. Returns the stage.
pub fn start(c: &mut Company, key: &str) -> Result<u32, &'static str> {
    let Some(t) = TOPICS.iter().find(|t| t.key == key) else { return Err("There is no such research.") };
    let stage = stages_done(&c.research, t) + 1;
    if stage > t.stages {
        return Err("This research is done.");
    }
    if c.research.running.is_some() {
        return Err("Another research is running: wait for it, or stop it.");
    }
    let missing = unmet(c, t);
    if missing.iter().any(|n| matches!(n, Need::Done(_))) {
        return Err("This research needs another one first.");
    }
    if !missing.is_empty() {
        return Err("This research opens at a higher company level.");
    }
    let k = cost(c, t, stage);
    if c.trophies < k.trophies {
        return Err("Not enough trophies: the company earns them with goals and contracts.");
    }
    if c.cash < k.money {
        return Err("Not enough cash.");
    }
    c.book(BookingKind::Research, -k.money, format!("{} {}/{}", t.title, stage, t.stages), false);
    c.trophies -= k.trophies;
    c.research.running = Some(Running { topic: t.key.to_string(), stage, work: k.work, done: 0.0, started: c.date.clone(), money: k.money, trophies: k.trophies, idle_told: false });
    c.research.checked = true;
    Ok(stage)
}

/// Stop the research under way: its trophies come back, and half its money (the other half
/// is lost). Returns its topic.
pub fn stop(c: &mut Company) -> Result<String, &'static str> {
    let Some(r) = c.research.running.take() else { return Err("No research is running.") };
    let title = topic(&r.topic).map(|t| t.title).unwrap_or("Research");
    c.book(BookingKind::Research, r.money / 2, format!("{title}: stopped"), false);
    c.trophies += r.trophies;
    Ok(r.topic)
}

// --- the researchers ---------------------------------------------------------------------------

/// The researchers at work on `date`: employed, not ill, on holiday or on a course.
pub fn researchers<'a>(c: &'a Company, date: &'a str) -> impl Iterator<Item = &'a Employee> + 'a {
    // (at work by their role's rota: a researcher's day off is no research - `staff::at_work`)
    c.staff.iter().filter(move |e| e.role == Role::Researcher && super::staff::at_work(e, date))
}

/// The work a researcher does in a day (researcher-days): from a quarter for a stressed
/// beginner to more than one and a half for a calm expert; nearly 0.9 for one of some years.
pub fn output(e: &Employee) -> f64 {
    (0.5 + e.experience.clamp(0.0, 100.0) / 100.0) * (1.1 - 0.6 * e.stress.clamp(0.0, 100.0) / 100.0)
}

/// The work the researchers do on `date`.
pub fn daily_work(c: &Company, date: &str) -> f64 {
    researchers(c, date).map(output).sum()
}

/// Days the research under way still takes at today's pace (None: nothing runs, or nobody
/// works on it).
pub fn days_left(c: &Company) -> Option<f64> {
    let r = c.research.running.as_ref()?;
    let pace = daily_work(c, &c.date);
    (pace > 0.0).then(|| ((r.work - r.done).max(0.0) / pace).ceil())
}

/// The research's day (`day::close_day`): what is done works on (the food, the goodwill the
/// passengers feel), and the researchers work on what runs; a stage done is told in the inbox,
/// and a research nobody works on once.
pub fn day(c: &mut Company, date: &str, _report: &mut DayReport) {
    c.research.checked = true;
    // better food: the staff at work a little happier and calmer
    let food = n(c, "food") as f64;
    if food > 0.0 {
        for e in c.staff.iter_mut().filter(|e| e.employed_on(date) && !e.absent(date)) {
            e.satisfaction = (e.satisfaction + 0.05 * food).min(100.0);
            e.stress = (e.stress - 0.05 * food).max(0.0);
        }
    }
    // what the passengers feel every day: a little goodwill
    let comfort = n(c, "air_conditioning") + n(c, "stops");
    if comfort > 0 {
        c.reputation = (c.reputation + 0.01 * comfort as f64).clamp(0.0, 100.0);
    }
    // the work on what runs
    let Some(mut run) = c.research.running.clone() else { return };
    let Some(t) = topic(&run.topic) else {
        c.research.running = None;
        return;
    };
    let work = daily_work(c, date);
    if work <= 0.0 {
        if !run.idle_told {
            run.idle_told = true;
            inbox::post(c, Message::new(Kind::Research, "The research stands still", "Nobody works on the research: hire researchers on the Staff page.").arg("name", t.title));
        }
        c.research.running = Some(run);
        return;
    }
    run.done += work;
    run.idle_told = false;
    if run.done + 1e-9 < run.work {
        c.research.running = Some(run);
        return;
    }
    c.research.running = None;
    let key = stage_key(t, run.stage);
    if !c.research.has(&key) {
        c.research.done.push(key);
    }
    let m = if t.stages <= 1 {
        Message::new(Kind::Research, t.title, "The research is done: it works from today.")
    } else {
        Message::new(Kind::Research, t.title, "Stage %{stage} of %{stages} is done: it works from today.").arg("stage", run.stage.to_string()).arg("stages", t.stages.to_string())
    };
    inbox::post(c, m);
}

/// An older company brought up to date (`store::migrate`): what it could buy before - the
/// articulated buses and double-deckers its level opened, those it has - stays open, as
/// research done.
pub fn migrate(c: &mut Company) {
    if c.research.checked {
        return;
    }
    c.research.checked = true;
    let owns = |c: &Company, s: BusSize| c.fleet.iter().any(|v| v.kind.size == s);
    let mark = |c: &mut Company, key: &str| {
        if !c.research.has(key) {
            c.research.done.push(key.to_string());
        }
    };
    if owns(c, BusSize::Articulated) || levels::unlocked(c, Feature::ArticulatedBuses) {
        mark(c, keys::ARTICULATED);
    }
    if owns(c, BusSize::Double) || levels::unlocked(c, Feature::DoubleDeckers) {
        mark(c, keys::DOUBLE_DECKERS);
    }
}

// --- what the research does ----------------------------------------------------------------------

/// Whether the research lets the company buy buses of this size (midibuses and solo buses
/// need none).
pub fn allows(c: &Company, size: BusSize) -> bool {
    match size {
        BusSize::Articulated => c.research.has(keys::ARTICULATED),
        BusSize::Double => c.research.has(keys::DOUBLE_DECKERS),
        BusSize::Midi | BusSize::Solo => true,
    }
}

/// What the research done does to the fuel a bus uses (1: nothing): 2 % less a stage.
pub fn fuel_factor(c: &Company) -> f64 {
    1.0 - 0.02 * n(c, "fuel") as f64
}

/// What the research done does to how many ride (1: nothing): the buses' upgrades, the stops,
/// the app, the ticket machines, park and ride.
pub fn demand_factor(c: &Company) -> f64 {
    let x = |key: &str, share: f64| n(c, key) as f64 * share;
    1.0 + x("air_conditioning", 0.02) + x("passenger_info", 0.02) + x(keys::WLAN, 0.02) + x("usb", 0.015) + x("premium_seats", 0.03) + x("stops", 0.015) + x("app", 0.01) + x("ticket_machines", 0.01) + x("park_and_ride", 0.02)
}

/// What it does to the chance of a trip being late (1: nothing): bus priority 10 % less a
/// stage, the ticket machines 3 %.
pub fn late_factor(c: &Company) -> f64 {
    (1.0 - 0.1 * n(c, "priority") as f64) * (1.0 - 0.03 * n(c, "ticket_machines") as f64)
}

/// What it does to a kilometre's wear and maintenance (1: nothing): 5 % less a stage.
pub fn wear_factor(c: &Company) -> f64 {
    1.0 - 0.05 * n(c, "wear") as f64
}

/// What it does to the dirt a day brings the fleet (1: nothing): 10 % less a stage.
pub fn dirt_factor(c: &Company) -> f64 {
    1.0 - 0.1 * n(c, "dirt") as f64
}

/// How clean the fleet stays at least without a washing bay (the cleaners' rounds).
pub fn clean_floor(c: &Company) -> f64 {
    if c.research.has("auto_cleaning") {
        55.0
    } else {
        35.0
    }
}

/// What it does to the buses the washing bay washes a night.
pub fn wash_factor(c: &Company) -> f64 {
    if c.research.has("auto_washing") {
        1.5
    } else {
        1.0
    }
}

/// What it does to the buses the pumps and chargers serve a night.
pub fn refuel_factor(c: &Company) -> f64 {
    if c.research.has("auto_refuelling") {
        1.5
    } else {
        1.0
    }
}

/// Places for buses the premises and the external depots add to the depot's.
pub fn extra_places(c: &Company) -> u32 {
    6 * n(c, "premises") + 10 * n(c, "external_depots")
}

/// Concessions more the company may hold.
pub fn extra_concessions(c: &Company) -> usize {
    n(c, "area") as usize
}

/// What the spare parts cost against their list price: 10 % less a stage.
pub fn parts_factor(c: &Company) -> f64 {
    1.0 - 0.1 * n(c, "parts") as f64
}

/// What a repair's time is against the usual: 10 % less a stage.
pub fn repair_factor(c: &Company) -> f64 {
    1.0 - 0.1 * n(c, "repairs") as f64
}

/// Whether a bus that breaks down is repaired at an external depot (a day sooner).
pub fn auto_repair(c: &Company) -> bool {
    c.research.has("auto_repair") && c.research.has("external_depots")
}

/// How many passengers the ticket inspectors check against the usual: 15 % more a stage.
pub fn inspection_factor(c: &Company) -> f64 {
    1.0 + 0.15 * n(c, "inspection") as f64
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::company::dates;

    fn company() -> Company {
        let mut c = found(&Founding { name: "Forschung".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.trophies = 500;
        c
    }

    /// Someone working on the research (`exp` experience, `stress`).
    pub(crate) fn researcher(c: &mut Company, exp: f64, stress: f64) -> u32 {
        let mut a = super::super::staff::applicants(c)[0].clone();
        a.no = 900 + c.staff.len() as u32;
        let id = super::super::staff::hire(c, &a).unwrap();
        let e = c.staff.iter_mut().find(|e| e.id == id).unwrap();
        (e.role, e.experience, e.stress, e.sick_until, e.holiday_until) = (Role::Researcher, exp, stress, None, None);
        id
    }

    fn close(c: &mut Company) {
        let date = c.date.clone();
        let mut r = DayReport::default();
        day(c, &date, &mut r);
        c.date = dates::add(&date, 1);
    }

    #[test]
    fn the_tree_hangs_together() {
        // keys unique, the keys others ask for there, every need one that exists
        for (i, a) in TOPICS.iter().enumerate() {
            assert!(TOPICS.iter().skip(i + 1).all(|b| b.key != a.key), "{}", a.key);
            assert!(a.stages >= 1 && a.work > 0.0 && a.money > 0);
            for need in a.needs {
                if let Need::Done(k) = need {
                    assert!(topic(k).is_some(), "{} needs {}", a.key, k);
                }
            }
        }
        for k in [keys::WLAN, keys::TICKET_INSPECTORS, keys::SHIFT_SCHEDULE, keys::ARTICULATED, keys::DOUBLE_DECKERS] {
            let t = topic(k).unwrap();
            assert_eq!(stage_key(t, 1), k);
        }
        let fuel = topic("fuel").unwrap();
        assert_eq!((stage_key(fuel, 2).as_str(), stage_of("fuel_2"), stage_of("wlan")), ("fuel_2", 2, 1));
        assert_eq!(topic("fuel_3").map(|t| t.key), Some("fuel"));
        assert!(topic("fuel_9").is_none() && topic("ticket_inspectors").is_some_and(|t| t.key == keys::TICKET_INSPECTORS));
        // every section has topics
        assert!(Section::ALL.iter().all(|s| TOPICS.iter().any(|t| t.section == *s)));
    }

    #[test]
    fn a_research_is_paid_when_it_starts_and_the_researchers_finish_it() {
        let mut c = company();
        let (cash, trophies) = (c.cash, c.trophies);
        let t = topic("passenger_info").unwrap();
        let k = cost(&c, t, 1);
        assert_eq!(start(&mut c, "passenger_info"), Ok(1));
        assert_eq!((c.cash, c.trophies), (cash - k.money, trophies - k.trophies));
        assert_eq!(c.month(&dates::month_of(&c.date)).get(BookingKind::Research), -k.money);
        assert_eq!(state(&c, t), State::Running);
        // one at a time
        assert_eq!(start(&mut c, "fuel"), Err("Another research is running: wait for it, or stop it."));
        // without researchers nothing moves: said once in the inbox
        close(&mut c);
        close(&mut c);
        assert_eq!(c.research.running.as_ref().unwrap().done, 0.0);
        assert_eq!(c.inbox.messages.iter().filter(|m| m.kind == Kind::Research).count(), 1);
        assert_eq!(days_left(&c), None);
        // two researchers: done in a few days, told in the inbox, and it works
        researcher(&mut c, 40.0, 20.0);
        researcher(&mut c, 40.0, 20.0);
        assert_eq!(days_left(&c), Some(4.0));
        let mut days = 0;
        while c.research.running.is_some() && days < 30 {
            close(&mut c);
            days += 1;
        }
        // (four working days: the researchers keep their role's rota, five days in seven, so a
        // weekend between makes it six - `staff::at_work`)
        assert!((4..=6).contains(&days), "{days}");
        assert!(c.research.has("passenger_info") && state(&c, t) == State::Done);
        assert!(c.inbox.messages.last().is_some_and(|m| m.kind == Kind::Research && m.title == t.title));
        assert!((demand_factor(&c) - 1.02).abs() < 1e-9);
        // it opens what needs it
        assert_eq!(state(&c, topic(keys::WLAN).unwrap()), State::Open);
        assert_eq!(start(&mut c, "passenger_info"), Err("This research is done."));
    }

    #[test]
    fn experienced_calm_researchers_work_more() {
        let mut c = company();
        let a = researcher(&mut c, 90.0, 5.0);
        let b = researcher(&mut c, 5.0, 90.0);
        let (oa, ob) = (output(c.employee(a).unwrap()), output(c.employee(b).unwrap()));
        assert!(oa > 1.4 && ob < 0.35, "{oa} {ob}");
        // nobody else researches; nor does someone ill
        c.staff.iter_mut().find(|e| e.id == b).unwrap().sick_until = Some(c.date.clone());
        let date = c.date.clone();
        assert!((daily_work(&c, &date) - oa).abs() < 1e-9);
        c.staff.iter_mut().find(|e| e.id == a).unwrap().role = Role::Driver;
        assert_eq!(daily_work(&c, &date), 0.0);
    }

    #[test]
    fn what_is_missing_keeps_a_research_closed() {
        let mut c = company();
        // a need not met, the level not reached, the trophies short, the cash short
        assert_eq!(state(&c, topic(keys::WLAN).unwrap()), State::Locked);
        assert_eq!(unmet(&c, topic(keys::WLAN).unwrap()), vec![Need::Done("passenger_info")]);
        assert_eq!(start(&mut c, keys::WLAN), Err("This research needs another one first."));
        assert_eq!(start(&mut c, keys::ARTICULATED), Err("This research opens at a higher company level."));
        c.trophies = 3;
        assert_eq!(start(&mut c, "air_conditioning"), Err("Not enough trophies: the company earns them with goals and contracts."));
        c.trophies = 500;
        c.cash = 100;
        assert_eq!(start(&mut c, "air_conditioning"), Err("Not enough cash."));
        // stopped: the trophies back and half the money
        c.cash = 1_000_000_00;
        start(&mut c, "air_conditioning").unwrap();
        let k = cost(&c, topic("air_conditioning").unwrap(), 1);
        assert_eq!(stop(&mut c), Ok("air_conditioning".to_string()));
        assert_eq!((c.cash, c.trophies), (1_000_000_00 - k.money + k.money / 2, 500));
        assert!(c.research.running.is_none());
        assert_eq!(stop(&mut c), Err("No research is running."));
    }

    #[test]
    fn stages_cost_more_and_add_up() {
        let mut c = company();
        let fuel = topic("fuel").unwrap();
        let (one, two) = (cost(&c, fuel, 1), cost(&c, fuel, 2));
        assert!(two.money > one.money && two.trophies > one.trophies && two.work > one.work);
        assert_eq!(one.trophies, 0);
        // harder companies pay more
        c.difficulty = Difficulty::Hard;
        assert!(cost(&c, fuel, 1).money > one.money);
        c.research.done = vec!["fuel_1".into(), "fuel_2".into(), "fuel_4".into()];
        // (the stages one after the other: the fourth without the third does not count)
        assert_eq!(stages_done(&c.research, fuel), 2);
        assert!((fuel_factor(&c) - 0.96).abs() < 1e-9);
        c.research.done = ["priority_1", "ticket_machines_1", "wear_1", "premises_1", "premises_2", "external_depots", "area_1", "parts_1", "repairs_1"].iter().map(|s| s.to_string()).collect();
        assert!((late_factor(&c) - 0.9 * 0.97).abs() < 1e-9);
        assert!((wear_factor(&c) - 0.95).abs() < 1e-9);
        assert_eq!((extra_places(&c), extra_concessions(&c)), (22, 1));
        assert!((parts_factor(&c) - 0.9).abs() < 1e-9 && (repair_factor(&c) - 0.9).abs() < 1e-9);
        assert!(!auto_repair(&c));
        // nothing done: everything as ever
        let plain = company();
        assert_eq!((fuel_factor(&plain), demand_factor(&plain), late_factor(&plain), wear_factor(&plain), dirt_factor(&plain)), (1.0, 1.0, 1.0, 1.0, 1.0));
        assert_eq!((clean_floor(&plain), wash_factor(&plain), refuel_factor(&plain), inspection_factor(&plain)), (35.0, 1.0, 1.0, 1.0));
    }

    /// A company running a line with buses and drivers (as the day's tests have it).
    fn running() -> Company {
        use super::super::market::{self, MarketBus, Payment};
        let mut c = company();
        c.map = "maps/Grundorf/global.cfg".into();
        c.lines.push(super::super::model::CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        c.planning.auto = true;
        c.cash += 5_000_000_00;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..2 {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        for a in super::super::staff::applicants(&c).into_iter().filter(|a| a.licence == super::super::model::Licence::D).take(4) {
            super::super::staff::hire(&mut c, &a).unwrap();
        }
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until) = (None, None);
        }
        c
    }

    fn tours() -> Vec<super::super::network::TourOfDay> {
        let tour = |no: &str, from: i32| super::super::network::TourOfDay { line: "Linie5".into(), number: "5".into(), tour: no.into(), ai_group: String::new(), trips: (0..8).map(|k| super::super::network::tests::trip(from + k * 60, from + 50 + k * 60, 12)).collect(), unplanned: false, cuts: None };
        vec![tour("1", 6 * 60), tour("2", 6 * 60 + 30)]
    }

    #[test]
    fn the_day_feels_what_was_researched() {
        let mut plain = running();
        let mut done = plain.clone();
        done.research.done = ["fuel_1", "fuel_2", "fuel_3", "fuel_4", "fuel_5", "wear_1", "wear_2", "air_conditioning", "passenger_info", "premium_seats", "usb", "priority_1", "priority_2", "priority_3"].iter().map(|s| s.to_string()).collect();
        let month = dates::month_of(&plain.date);
        let (a, b) = (super::super::day::close_day(&mut plain, tours(), &[]), super::super::day::close_day(&mut done, tours(), &[]));
        let (ea, eb) = (plain.month(&month).get(BookingKind::Energy), done.month(&month).get(BookingKind::Energy));
        // 10 % less fuel, 10 % less maintenance, the buses worn less, more passengers, fewer late
        assert!(ea < 0 && ((eb as f64 / ea as f64) - 0.9).abs() < 0.01, "{ea} {eb}");
        let (ma, mb) = (plain.month(&month).get(BookingKind::Maintenance), done.month(&month).get(BookingKind::Maintenance));
        assert!(((mb as f64 / ma as f64) - 0.9).abs() < 0.01, "{ma} {mb}");
        assert!(done.fleet.iter().zip(plain.fleet.iter()).all(|(x, y)| x.condition > y.condition));
        assert!(b.passengers > a.passengers && b.late <= a.late, "{} {} {} {}", a.passengers, b.passengers, a.late, b.late);
        // the depot's places and the concessions grow with the premises and the area
        let (places, conc) = (super::super::depot::places(&plain), levels::max_concessions(&plain));
        done.research.done.extend(["premises_1", "external_depots", "area_1"].iter().map(|s| s.to_string()));
        assert_eq!((super::super::depot::places(&done), levels::max_concessions(&done)), (places + 16, conc + 1));
    }

    #[test]
    fn better_food_lifts_the_staff() {
        let mut c = company();
        let id = researcher(&mut c, 40.0, 50.0);
        c.research.done = vec!["food_1".into(), "food_2".into()];
        let sat = c.employee(id).unwrap().satisfaction;
        close(&mut c);
        let e = c.employee(id).unwrap();
        assert!(e.satisfaction > sat && e.stress < 50.0);
    }

    #[test]
    fn an_older_company_keeps_the_buses_its_level_opened() {
        // an older company at level 2: the articulated buses it could buy stay open, the
        // double-deckers need research
        let mut c = company();
        c.research.checked = false;
        c.progress.xp = levels::LEVEL_XP[1];
        assert!(!allows(&c, BusSize::Articulated));
        let c = super::super::store::migrate(c);
        assert!(allows(&c, BusSize::Articulated) && !allows(&c, BusSize::Double) && allows(&c, BusSize::Solo));
        assert!(super::super::market::size_allowed(&c, BusSize::Articulated).is_ok());
        // one with a double-decker keeps buying them
        let mut d = company();
        d.research.checked = false;
        d.cash += 1_000_000_00;
        d.progress.xp = levels::LEVEL_XP[9];
        d.research.done.push(keys::DOUBLE_DECKERS.into());
        let bus = super::super::market::MarketBus { file: "Vehicles/DD/DD.bus".into(), name: "DD".into(), kind: super::super::model::BusKind { size: BusSize::Double, drive: Default::default() }, ..Default::default() };
        super::super::market::buy_new(&mut d, &bus, super::super::market::Payment::Cash, "").unwrap();
        d.research.done.clear();
        d.progress.xp = 0;
        migrate(&mut d);
        assert!(allows(&d, BusSize::Double) && !allows(&d, BusSize::Articulated));
        // a new company is checked from its founding: what its level opens later is researched
        let mut e = company();
        e.progress.xp = levels::LEVEL_XP[5];
        migrate(&mut e);
        assert!(!allows(&e, BusSize::Articulated) && !allows(&e, BusSize::Double));
        let mut f = company();
        f.research.checked = false;
        close(&mut f);
        f.progress.xp = levels::LEVEL_XP[5];
        migrate(&mut f);
        assert!(!allows(&f, BusSize::Articulated));
        // an older file without the research reads, and is unchecked
        let old: Research = serde_json::from_str("{}").unwrap();
        assert_eq!(old, Research::default());
        let old: Research = serde_json::from_str(r#"{"done":["wlan"]}"#).unwrap();
        assert!(old.has(keys::WLAN) && !old.checked && old.running.is_none());
    }
}
