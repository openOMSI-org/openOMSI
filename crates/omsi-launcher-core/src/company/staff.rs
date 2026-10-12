//! The company's people: the labour market of every role (new applicants every week), hiring and
//! dismissing with notice, what a working day does to them (experience, satisfaction, stress,
//! friendliness, illness, holidays, leaving), and the working-time rules a duty must keep -
//! Omsi-Hub's `planregels.ts`: a duty of at most 9½ hours, a working day of at most 10 (overtime
//! above 8), 20 minutes between two pieces of work, 11 hours' rest overnight, a transfer to
//! another stop needs 45 minutes, and an articulated or double-decker bus wants some experience.
//!
//! Besides the drivers the company has mechanics, researchers, cleaners, service employees and
//! ticket inspectors (City Bus Manager's staff). Only drivers drive (`drives`: the planning, the
//! licences and the type trainings ask it). The others work by their role's hours (`Hours`: core
//! hours when all of them who work that day are there, and how many stay on after them - the
//! least stressed), five days a week by a rota, and are paid by the hour: their monthly wage is
//! forty hours a week, the night (20:00 to 06:00) is paid half as much again, and a day worked
//! pays at least four hours. What their day does is `roles_day`: the mechanics do part of the
//! maintenance and repairs in the company's own workshop (`workshop_share`, a broken-down bus
//! back sooner), the cleaners keep the buses clean (`Vehicle::clean`, which the passengers
//! notice), the service centre answers the passengers' complaints and lifts the reputation, the
//! ticket inspectors bring in the fares dodgers would not pay (`fare_gain`); the researchers'
//! work is counted by `research`.
//!
//! Every person has a portrait of their own (`PORTRAITS` of them): no two people of a company -
//! its staff and the week's applicants together - share one, a new face is one the company never
//! gave before while there is one, and a face comes free again when its person leaves
//! (`free_portraits`, `migrate`).

use super::dates;
use super::economy;
use super::inbox::{self, Kind, Message};
use super::licences::Endorsement;
use super::model::{BookingKind, BusSize, Cents, Company, Employee, Licence, Role, Skills, Taken, PORTRAITS};
use super::rng::Rng;
use super::training::CourseKind;
use serde::{Deserialize, Serialize};

// --- working time (planregels.ts) ----------------------------------------------------------

/// A duty lasts at most this long (minutes); a longer tour is split.
pub const DUTY_MAX: i32 = 570;
/// A tour is split only where the bus stands at least this long.
pub const SPLIT_PAUSE: i32 = 3;
/// Rest between two pieces of work, overnight rest, time for a transfer between stops.
pub const REST: i32 = 20;
pub const NIGHT_REST: i32 = 660;
pub const TRANSFER: i32 = 45;
/// A working day's target (overtime above it) and its hard limit.
pub const DAY_TARGET: i32 = 480;
pub const DAY_MAX: i32 = 600;
/// Between two tours of the same bus.
pub const BUS_MARGIN: i32 = 10;
/// Under this experience an articulated bus or a double-decker is a warning.
pub const EXPERIENCE_LARGE: f64 = 25.0;
/// Days a week someone works at most.
pub const WEEK_DAYS: u32 = 5;
/// Holiday days a year (the German regional tariffs give about 30).
pub const HOLIDAYS: u32 = 30;

/// A piece of work of one driver or bus: minutes of the day, and the stops it begins and
/// ends at.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Block {
    pub key: String,
    pub from: i32,
    pub to: i32,
    pub from_stop: String,
    pub to_stop: String,
}

pub fn overlaps(a: &Block, b: &Block, margin: i32) -> bool {
    a.from < b.to + margin && b.from < a.to + margin
}

pub fn work_minutes(blocks: &[Block]) -> i32 {
    blocks.iter().map(|b| b.to - b.from).sum()
}

/// How a new piece of work fits what a driver has already (`toets`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Check {
    /// The key of a block it overlaps.
    pub double: Option<String>,
    /// The working day goes over `DAY_MAX`.
    pub too_long: bool,
    /// The shortest pause to a neighbouring block.
    pub rest: Option<i32>,
    /// That pause, when it is shorter than `TRANSFER` and the stops differ.
    pub transfer: Option<i32>,
    /// Minutes over `DAY_TARGET`.
    pub overtime: i32,
    /// The shortest rest against the day before (its last end, minutes of that day) or the
    /// day after (its first start).
    pub night_rest: Option<i32>,
}

impl Check {
    /// Allowed at all (the hard limits; the rest are warnings).
    pub fn allowed(&self) -> bool {
        self.double.is_none() && !self.too_long && self.rest.is_none_or(|r| r >= REST) && self.night_rest.is_none_or(|r| r >= NIGHT_REST)
    }
}

pub fn check(existing: &[Block], new: &Block, prev_end: Option<i32>, next_start: Option<i32>) -> Check {
    let others: Vec<&Block> = existing.iter().filter(|b| b.key != new.key).collect();
    let double = others.iter().find(|b| overlaps(b, new, 0)).map(|b| b.key.clone());
    let mut all: Vec<&Block> = others.clone();
    all.push(new);
    all.sort_by_key(|b| b.from);
    let work: i32 = all.iter().map(|b| b.to - b.from).sum();
    let at = all.iter().position(|b| std::ptr::eq(*b, new)).unwrap_or(0);
    let mut pairs = Vec::new();
    if at > 0 {
        pairs.push((all[at - 1], new));
    }
    if at + 1 < all.len() {
        pairs.push((new, all[at + 1]));
    }
    let (mut rest, mut transfer) = (None::<i32>, None::<i32>);
    for (a, b) in pairs {
        let pause = b.from - a.to;
        if pause < 0 {
            continue;
        }
        rest = Some(rest.map_or(pause, |r| r.min(pause)));
        if pause < TRANSFER && !a.to_stop.is_empty() && !b.from_stop.is_empty() && a.to_stop != b.from_stop {
            transfer = Some(transfer.map_or(pause, |t| t.min(pause)));
        }
    }
    let mut nights = Vec::new();
    if let Some(p) = prev_end {
        nights.push(1440 + all[0].from - p);
    }
    if let Some(n) = next_start {
        nights.push(1440 + n - all[all.len() - 1].to);
    }
    Check { double, too_long: work > DAY_MAX, rest, transfer, overtime: (work - DAY_TARGET).max(0), night_rest: nights.into_iter().min() }
}

/// May drive a bus of this size at all (a driver, and the licence: D1 only midibuses).
pub fn may_drive(e: &Employee, size: BusSize) -> bool {
    drives(e) && (e.licence == Licence::D || size == BusSize::Midi)
}

/// Drives it without a warning (an articulated bus or a double-decker wants experience).
pub fn qualified(e: &Employee, size: BusSize) -> bool {
    may_drive(e, size) && (!matches!(size, BusSize::Articulated | BusSize::Double) || e.experience >= EXPERIENCE_LARGE)
}

// --- the roles -------------------------------------------------------------------------------

/// Whether someone drives buses. Only the drivers are planned on duties, counted as drivers,
/// trained for licences and bus types, or listed among the planning's drivers.
pub fn drives(e: &Employee) -> bool {
    e.role == Role::Driver
}

/// The roles besides the drivers: they work by their role's hours (`Hours`).
pub const OTHERS: [Role; 5] = [Role::Mechanic, Role::Researcher, Role::Cleaner, Role::Service, Role::Inspector];

/// A role's name for many ("Mechanics"; English, translated when shown).
pub fn plural(role: Role) -> &'static str {
    match role {
        Role::Driver => "Bus drivers",
        Role::Mechanic => "Mechanics",
        Role::Researcher => "Researchers",
        Role::Cleaner => "Cleaners",
        Role::Service => "Service employees",
        Role::Inspector => "Ticket inspectors",
    }
}

/// What someone of a role does at work, in a few words.
pub fn activity(role: Role) -> &'static str {
    match role {
        Role::Driver => "Driving a bus",
        Role::Mechanic => "Repairing and servicing buses",
        Role::Researcher => "Researching",
        Role::Cleaner => "Cleaning buses",
        Role::Service => "Working in the service centre",
        Role::Inspector => "Checking tickets",
    }
}

/// The launcher's icon for a role.
pub fn icon(role: Role) -> &'static str {
    match role {
        Role::Driver => "directions_bus",
        Role::Mechanic => "build",
        Role::Researcher => "science",
        Role::Cleaner => "cleaning_services",
        Role::Service => "support_agent",
        Role::Inspector => "confirmation_number",
    }
}

fn key(role: Role) -> &'static str {
    match role {
        Role::Driver => "driver",
        Role::Mechanic => "mechanic",
        Role::Researcher => "researcher",
        Role::Cleaner => "cleaner",
        Role::Service => "service",
        Role::Inspector => "inspector",
    }
}

fn role_index(role: Role) -> u32 {
    Role::ALL.iter().position(|r| *r == role).unwrap_or(0) as u32
}

/// A skill level, from experience (as the staff cards of City Bus Manager show it).
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub enum Grade {
    Trainee,
    Novice,
    Experienced,
    Expert,
}

impl Grade {
    pub const ALL: [Grade; 4] = [Grade::Trainee, Grade::Novice, Grade::Experienced, Grade::Expert];

    pub fn of(experience: f64) -> Grade {
        if experience >= 75.0 {
            Grade::Expert
        } else if experience >= 40.0 {
            Grade::Experienced
        } else if experience >= 15.0 {
            Grade::Novice
        } else {
            Grade::Trainee
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Grade::Trainee => "Trainee",
            Grade::Novice => "Novice",
            Grade::Experienced => "Experienced",
            Grade::Expert => "Expert",
        }
    }
}

/// What a role earns on the market a month (gross) with `experience`, at the prices of
/// `price_index`. At founding: a driver €2,900 to €3,800 (`economy::market_wage`), a mechanic
/// €3,000 to €4,000, a researcher €4,000 to €5,800, a cleaner €2,250 to €2,600, a service
/// employee €2,550 to €3,200, a ticket inspector €2,600 to €3,200 (German tariffs of the
/// mid-2020s, rounded).
pub fn market_wage(role: Role, experience: f64, price_index: f64) -> Cents {
    let (base, top): (f64, f64) = match role {
        Role::Driver => return economy::market_wage(experience, price_index),
        Role::Mechanic => (3_000_00.0, 1_000_00.0),
        Role::Researcher => (4_000_00.0, 1_800_00.0),
        Role::Cleaner => (2_250_00.0, 350_00.0),
        Role::Service => (2_550_00.0, 650_00.0),
        Role::Inspector => (2_600_00.0, 600_00.0),
    };
    let x = experience.clamp(0.0, 100.0) / 100.0;
    (((base + top * x) * price_index / 10_00.0).round() as Cents) * 10_00
}

/// The hours a month's wage pays: forty a week.
pub const MONTH_HOURS: f64 = 40.0 * 52.0 / 12.0;
/// The night (20:00 to 06:00) is paid half as much again.
pub const NIGHT_FROM: i32 = 20 * 60;
pub const NIGHT_TO: i32 = 6 * 60;
pub const NIGHT_SUPPLEMENT: f64 = 0.5;
/// A day worked pays at least four hours.
pub const MIN_PAID: i32 = 240;
/// Nobody besides the drivers works more than twelve hours a day.
pub const ROLE_DAY_MAX: i32 = 720;
/// What a day away on the rota pays (a holiday, an illness, a course).
pub const STANDARD_DAY: i32 = 480;

/// An hour of a monthly wage (cents, gross).
pub fn hourly(wage: Cents) -> f64 {
    wage as f64 / MONTH_HOURS
}

/// The minutes of `from..to` (minutes of a day; `to` may pass midnight) that fall in the
/// night, 20:00 to 06:00.
pub fn night_minutes(from: i32, to: i32) -> i32 {
    in_window(from, to, NIGHT_FROM, NIGHT_TO)
}

/// The minutes of `from..to` (minutes of a day, `to` up to a day later) that fall in the daily
/// window `a..b` (minutes of the day; `b` before `a` passes midnight).
pub fn in_window(from: i32, to: i32, a: i32, b: i32) -> i32 {
    let len = (b - a).rem_euclid(1440);
    (-1..=2).map(|k| (to.min(a + 1440 * k + len) - from.max(a + 1440 * k)).max(0)).sum()
}

/// The courses someone of a role may take (`training::enrol`): a driver all the staff's
/// courses and the licences, the others those of their work.
pub fn courses_for(role: Role) -> &'static [CourseKind] {
    match role {
        Role::Driver => &CourseKind::STAFF,
        Role::Mechanic => &[CourseKind::Workshop, CourseKind::FirstAid],
        Role::Researcher | Role::Cleaner => &[CourseKind::FirstAid],
        Role::Service => &[CourseKind::CustomerService, CourseKind::FirstAid],
        Role::Inspector => &[CourseKind::CustomerService, CourseKind::Ticketing, CourseKind::FirstAid],
    }
}

/// Whether the company may hire someone of a role: ticket inspectors only once it has
/// researched them.
pub fn hireable(c: &Company, role: Role) -> Result<(), &'static str> {
    if role == Role::Inspector && !c.research.has(super::research::keys::TICKET_INSPECTORS) {
        return Err("Ticket inspectors can be hired once the company has researched ticket inspection.");
    }
    Ok(())
}

/// The friendliness someone settles at when nothing weighs on them: their experience and their
/// way with people (the service skill).
pub fn friendliness_of(experience: f64, service: f64) -> f64 {
    ((28.0 + 0.25 * experience + 0.4 * service).clamp(5.0, 98.0) * 10.0).round() / 10.0
}

/// What the passengers make of a driver's friendliness (on how many ride: ±3 %).
pub fn friendly_factor(friendliness: f64) -> f64 {
    0.97 + 0.06 * friendliness.clamp(0.0, 100.0) / 100.0
}

/// What the passengers make of a bus's cleanliness (on how many ride: 75 is as ever, a dirty
/// bus loses up to 6 %).
pub fn clean_factor(clean: f64) -> f64 {
    0.94 + 0.08 * clean.clamp(0.0, 100.0) / 100.0
}

// --- the working hours of the roles besides the drivers ---------------------------------------

/// A role's working hours: its core hours (minutes of the day; `to` before `from` passes
/// midnight), when all of them who work that day are there, and how many stay on after them.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Hours {
    pub from: i32,
    pub to: i32,
    #[serde(default)]
    pub outside: u32,
}

impl Hours {
    /// The core hours' length in minutes (up to a day).
    pub fn length(&self) -> i32 {
        let l = (self.to - self.from).rem_euclid(1440);
        if l == 0 {
            1440
        } else {
            l
        }
    }
}

/// A role's hours until the company sets its own: eight hours by day, nobody after them (the
/// cleaners in the afternoon and evening, as the buses come back).
pub fn default_hours(role: Role) -> Hours {
    let (from, to) = match role {
        Role::Researcher => (8 * 60, 16 * 60),
        Role::Cleaner => (12 * 60, 20 * 60),
        Role::Service => (7 * 60, 15 * 60),
        _ => (6 * 60, 14 * 60),
    };
    Hours { from, to, outside: 0 }
}

/// Someone's request for a raise, waiting for an answer until `until`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RaiseAsk {
    pub id: u32,
    /// 0.05 = 5 %.
    pub share: f64,
    pub asked: String,
    pub until: String,
}

/// What a role besides the drivers did on a day closed (for the pages).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoleDay {
    pub role: Role,
    pub at_work: u32,
    /// Hours paid, and of them at night.
    pub paid: f64,
    pub night: f64,
    /// The wages with the employer's share.
    pub wages: Cents,
    /// The share of the hours outside the core the company asked for that someone stayed.
    pub covered: f64,
}

/// The last day closed of the roles besides the drivers: their hours and wages, and what their
/// work did.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RolesDay {
    pub date: String,
    pub roles: Vec<RoleDay>,
    /// The in-house share of the maintenance and repairs.
    pub workshop: f64,
    /// Buses the cleaners cleaned, and the fleet's cleanliness after.
    pub cleaned: u32,
    pub clean: f64,
    /// The passengers' complaints, and those the service centre answered.
    pub complaints: u32,
    pub handled: u32,
    /// What the service centre did to the reputation.
    pub service_reputation: f64,
}

/// How the staff works: the hours of the roles besides the drivers, the automatic hiring, how
/// wages follow prices, the requests for a raise, and the portraits the company has given.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StaffRules {
    /// The roles' hours (a role not here: `default_hours`).
    #[serde(default)]
    pub hours: Vec<(Role, Hours)>,
    /// Hire at night from the week's applicants when a role has fewer than its minimum.
    #[serde(default)]
    pub auto_hire: bool,
    #[serde(default)]
    pub minimum: Vec<(Role, u32)>,
    /// Every wage rises with the prices at a month's end.
    #[serde(default = "yes")]
    pub follow_prices: bool,
    /// The price index the wages were last set at.
    #[serde(default)]
    pub wage_index: f64,
    /// A request for a raise of up to 5 % is granted without asking.
    #[serde(default)]
    pub grant_small: bool,
    /// Requests for a raise waiting for an answer.
    #[serde(default)]
    pub asks: Vec<RaiseAsk>,
    /// The portraits the company has given (to its people and its applicants), the longest ago
    /// first.
    #[serde(default)]
    pub portraits_used: Vec<u32>,
    #[serde(default)]
    pub last: Option<RolesDay>,
}

fn yes() -> bool {
    true
}

impl Default for StaffRules {
    fn default() -> Self {
        StaffRules { hours: Vec::new(), auto_hire: false, minimum: Vec::new(), follow_prices: true, wage_index: 0.0, grant_small: false, asks: Vec::new(), portraits_used: Vec::new(), last: None }
    }
}

impl StaffRules {
    pub fn hours_of(&self, role: Role) -> Hours {
        self.hours.iter().find(|h| h.0 == role).map(|h| h.1).unwrap_or_else(|| default_hours(role))
    }

    /// Set a role's hours: core hours of an hour at least and `ROLE_DAY_MAX` at most (a longer
    /// core ends then).
    pub fn set_hours(&mut self, role: Role, h: Hours) {
        let from = h.from.rem_euclid(1440);
        let len = (h.to - h.from).rem_euclid(1440);
        let len = if len == 0 { ROLE_DAY_MAX } else { len.clamp(60, ROLE_DAY_MAX) };
        let h = Hours { from, to: (from + len).rem_euclid(1440), outside: h.outside };
        match self.hours.iter_mut().find(|x| x.0 == role) {
            Some(x) => x.1 = h,
            None => self.hours.push((role, h)),
        }
    }

    pub fn minimum_of(&self, role: Role) -> u32 {
        self.minimum.iter().find(|m| m.0 == role).map(|m| m.1).unwrap_or(0)
    }

    pub fn set_minimum(&mut self, role: Role, n: u32) {
        match self.minimum.iter_mut().find(|x| x.0 == role) {
            Some(x) => x.1 = n,
            None => self.minimum.push((role, n)),
        }
    }

    /// Someone's request for a raise, when one waits.
    pub fn ask_of(&self, id: u32) -> Option<&RaiseAsk> {
        self.asks.iter().find(|a| a.id == id)
    }
}

/// Set a role's hours. Changing them asks for the research of the shift schedule.
pub fn set_hours(c: &mut Company, role: Role, h: Hours) -> Result<(), &'static str> {
    if role == Role::Driver {
        return Err("Bus drivers work their duties: plan them on the Planning page.");
    }
    if !c.research.has(super::research::keys::SHIFT_SCHEDULE) {
        return Err("The working hours can be set once the company has researched the shift schedule.");
    }
    c.staff_rules.set_hours(role, h);
    Ok(())
}

/// Whether someone besides the drivers works on `date` by the rota: five days a week, their two
/// free days following their number round the week (so a role is never all off at once).
pub fn on_rota(e: &Employee, date: &str) -> bool {
    let wd = dates::parse(date).map(dates::weekday).unwrap_or(0);
    (wd + e.id % 7) % 7 < 5
}

/// At work on `date`: employed, not away, and - besides the drivers - on the rota.
pub fn at_work(e: &Employee, date: &str) -> bool {
    e.employed_on(date) && !e.absent(date) && (drives(e) || on_rota(e, date))
}

/// Someone's working day by their role's hours (minutes of the day; a time may pass midnight).
#[derive(Clone, Debug, PartialEq)]
pub struct Shift {
    pub id: u32,
    pub role: Role,
    /// The core hours.
    pub from: i32,
    pub to: i32,
    /// Hours outside the core when they stay on (in turn with others: it may begin later than the
    /// core ends).
    pub extra: Option<(i32, i32)>,
}

impl Shift {
    pub fn minutes(&self) -> i32 {
        self.to - self.from + self.extra.map_or(0, |(a, b)| b - a)
    }

    pub fn night(&self) -> i32 {
        night_minutes(self.from, self.to) + self.extra.map_or(0, |(a, b)| night_minutes(a, b))
    }

    /// The minutes paid: at least four hours.
    pub fn paid(&self) -> i32 {
        self.minutes().max(MIN_PAID)
    }

    /// When their day ends (minutes from the start of the day; past midnight more than 1440).
    pub fn end(&self) -> i32 {
        self.extra.map_or(self.to, |(_, b)| b.max(self.to))
    }

    /// The minutes of their day that fall in the daily window `a..b`.
    pub fn within(&self, a: i32, b: i32) -> i32 {
        in_window(self.from, self.to, a, b) + self.extra.map_or(0, |(x, y)| in_window(x, y, a, b))
    }

    /// At work at `minute` of the day.
    pub fn at(&self, minute: i32) -> bool {
        let m = minute.rem_euclid(1440);
        let inside = |a: i32, b: i32| (a..b).contains(&m) || (a..b).contains(&(m + 1440));
        inside(self.from, self.to) || self.extra.is_some_and(|(a, b)| inside(a, b))
    }
}

/// Who of a role besides the drivers works on `date`, and when: all of them at work the core
/// hours; after them the hours outside the core for as many places as the role's `outside`
/// asks, each place kept in turn by the least stressed (nobody over `ROLE_DAY_MAX`). Returns the
/// shifts and the minutes outside the core that were asked for but nobody stayed.
pub fn shifts(c: &Company, date: &str, role: Role) -> (Vec<Shift>, i32) {
    if role == Role::Driver {
        return (Vec::new(), 0);
    }
    let h = c.staff_rules.hours_of(role);
    let len = h.length();
    let people: Vec<&Employee> = c.staff.iter().filter(|e| e.role == role && at_work(e, date)).collect();
    let mut out: Vec<Shift> = people.iter().map(|e| Shift { id: e.id, role, from: h.from, to: h.from + len, extra: None }).collect();
    let rest = 1440 - len;
    let mut uncovered = 0;
    if h.outside > 0 && rest > 0 {
        let mut order: Vec<usize> = (0..out.len()).collect();
        order.sort_by(|&a, &b| people[a].stress.total_cmp(&people[b].stress).then(people[a].id.cmp(&people[b].id)));
        let room = (ROLE_DAY_MAX - len).max(0);
        let mut next = order.into_iter();
        for _ in 0..h.outside {
            let mut t = 0;
            while t < rest && room > 0 {
                let Some(k) = next.next() else { break };
                let take = room.min(rest - t);
                let a = h.from + len + t;
                out[k].extra = Some((a, a + take));
                t += take;
            }
            uncovered += rest - t;
        }
    }
    (out, uncovered)
}

/// The shifts of every role besides the drivers on `date`.
pub fn all_shifts(c: &Company, date: &str) -> Vec<Shift> {
    OTHERS.iter().flat_map(|r| shifts(c, date, *r).0).collect()
}

/// Someone's shift on `date` (None: a driver, or not at work).
pub fn shift_of(c: &Company, date: &str, e: &Employee) -> Option<Shift> {
    if drives(e) {
        return None;
    }
    shifts(c, date, e.role).0.into_iter().find(|s| s.id == e.id)
}

// --- what the roles' work does ------------------------------------------------------------------

/// The most of the maintenance and repairs the company's own mechanics do (the outside garage's
/// labour and margin saved).
pub const WORKSHOP_MAX: f64 = 0.30;
/// What a line checked all day brings in fares that dodgers would not have paid (and their
/// fines).
pub const INSPECT_GAIN: f64 = 0.06;

/// How good someone is at their work: their experience, and the course of their work.
fn skill(c: &Company, e: &Employee) -> f64 {
    let course = match e.role {
        Role::Mechanic => super::training::trained(c, Some(e.id), CourseKind::Workshop),
        Role::Inspector => super::training::trained(c, Some(e.id), CourseKind::Ticketing),
        Role::Service => super::training::trained(c, Some(e.id), CourseKind::CustomerService),
        _ => false,
    };
    0.6 + 0.4 * e.experience.clamp(0.0, 100.0) / 100.0 + if course { 0.15 } else { 0.0 }
}

/// The hours a role's work wants on `date` and the hours it gets (weighed by skill and by when
/// they fall: the cleaners clean best at night, when the buses are in; the service centre and the
/// inspectors work while the buses run).
pub fn workload(c: &Company, date: &str, role: Role, shifts: &[Shift]) -> (f64, f64) {
    let buses = c.fleet.iter().filter(|v| v.held_on(date)).count() as f64;
    let got: f64 = shifts
        .iter()
        .filter(|s| s.role == role)
        .filter_map(|s| c.employee(s.id).map(|e| (s, skill(c, e))))
        .map(|(s, k)| {
            let h = match role {
                Role::Cleaner => (s.night() as f64 + 0.5 * (s.minutes() - s.night()) as f64) / 60.0,
                Role::Service => s.within(6 * 60, 22 * 60) as f64 / 60.0,
                Role::Inspector => s.within(5 * 60, 24 * 60) as f64 / 60.0,
                _ => s.minutes() as f64 / 60.0,
            };
            h * k
        })
        .sum();
    let lines = c.lines.iter().filter(|l| in_service(l, date)).count() as f64;
    let riders = c.history.last().map(|h| h.passengers as f64).unwrap_or(0.0);
    let want = match role {
        Role::Mechanic => 0.6 * buses,
        Role::Cleaner => 0.67 * buses,
        Role::Service => if lines > 0.0 { 2.0 + riders / 800.0 } else { 0.0 },
        Role::Inspector => 4.0 * lines,
        _ => got,
    };
    (want, got)
}

fn in_service(l: &super::model::CompanyLine, date: &str) -> bool {
    super::network::in_service(l, super::clock::moment(date, 24 * 60 - 1))
}

/// The share of the maintenance and repairs the company's mechanics do in its own workshop on
/// `date` (up to `WORKSHOP_MAX` when there are mechanics enough for the fleet).
pub fn workshop_share(c: &Company, date: &str) -> f64 {
    let (want, got) = workload(c, date, Role::Mechanic, &shifts(c, date, Role::Mechanic).0);
    if want <= 0.0 || got <= 0.0 {
        return 0.0;
    }
    WORKSHOP_MAX * (got / want).min(1.0)
}

/// The mechanics at work on `date`.
pub fn mechanics_at_work(c: &Company, date: &str) -> usize {
    c.staff.iter().filter(|e| e.role == Role::Mechanic && at_work(e, date)).count()
}

/// The lines the ticket inspectors at work check on `date`: each inspector takes one, the
/// busiest lines first (by their kilometres), and a line is checked all day with eight hours of
/// inspectors on it. Returns (the line's name, the share of the day checked, the inspectors).
pub fn inspected(c: &Company, date: &str) -> Vec<(String, f64, Vec<u32>)> {
    let (sh, _) = shifts(c, date, Role::Inspector);
    let mut lines: Vec<&super::model::CompanyLine> = c.lines.iter().filter(|l| in_service(l, date)).collect();
    if sh.is_empty() || lines.is_empty() {
        return Vec::new();
    }
    lines.sort_by(|a, b| b.km.total_cmp(&a.km).then(a.name.cmp(&b.name)));
    let mut out: Vec<(String, f64, Vec<u32>)> = Vec::new();
    for (k, s) in sh.iter().enumerate() {
        let line = &lines[k % lines.len()].name;
        let skill = c.employee(s.id).map(|e| skill(c, e)).unwrap_or(0.6);
        let share = s.within(5 * 60, 24 * 60) as f64 / 480.0 * skill;
        match out.iter_mut().find(|x| &x.0 == line) {
            Some(x) => {
                x.1 = (x.1 + share).min(1.0);
                x.2.push(s.id);
            }
            None => out.push((line.clone(), share.min(1.0), vec![s.id])),
        }
    }
    out
}

/// What the ticket inspectors add to the fares of a line (its name) on `date`: 0.06 = 6 %.
pub fn fare_gain(c: &Company, date: &str, line: &str) -> f64 {
    // (the inspectors' working method, researched, finds more: `research::inspection_factor`)
    inspected(c, date).iter().find(|x| x.0.eq_ignore_ascii_case(line)).map_or(0.0, |x| INSPECT_GAIN * x.1 * super::research::inspection_factor(c))
}

// --- the day of the roles besides the drivers ---------------------------------------------------

/// A day as it was for someone's needs.
#[derive(Clone, Copy, Debug, Default)]
struct DayOf {
    worked: bool,
    minutes: i32,
    night: i32,
    short_rest: bool,
    /// Too few colleagues: 0 (enough) to 1.
    pressure: f64,
    /// On holiday, ill or on a course.
    away: bool,
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Stress follows the day: overtime, night hours, a short rest and too few colleagues raise it, a
/// day off brings it down. Friendliness grows with experience and the service skill and falls
/// with stress.
fn needs(e: &mut Employee, d: &DayOf) {
    let (target, rate) = if d.worked {
        let over = (d.minutes - DAY_TARGET).max(0) as f64 / 60.0;
        (12.0 + 9.0 * over + 2.5 * d.night as f64 / 60.0 + if d.short_rest { 12.0 } else { 0.0 } + 30.0 * d.pressure, 0.2)
    } else {
        (5.0, if d.away { 0.25 } else { 0.3 })
    };
    e.stress = round1((e.stress + (target.min(100.0) - e.stress) * rate).clamp(0.0, 100.0));
    let f = (friendliness_of(e.experience, e.skills.service) - (e.stress - 40.0).max(0.0) * 0.6).clamp(0.0, 100.0);
    e.friendliness = round1((e.friendliness + (f - e.friendliness) * 0.05).clamp(0.0, 100.0));
}

/// The day of the staff besides the drivers (`day::close_day`, after the night of the people):
/// their hours paid (the night supplement, at least four hours; a standard day for who is away
/// on a day of the rota), their experience, stress and friendliness, and what their work did -
/// the cleaners clean the dirtiest buses, the service centre answers the day's complaints and
/// lifts the reputation (the mechanics' share is booked with the day, `levels::day_closed`; the
/// inspectors' fares with the trips). Then for everyone: requests for a raise, the wages
/// following the prices at a month's end, the automatic hiring, and what the inbox is told.
pub fn roles_day(c: &mut Company, date: &str, report: &mut super::day::DayReport) {
    let tomorrow = dates::add(date, 1);
    let day_no = dates::parse(date).unwrap_or(0);
    let mut out = RolesDay { date: date.to_string(), ..Default::default() };
    let mut all: Vec<Shift> = Vec::new();
    let mut pressure: Vec<(Role, f64)> = Vec::new();
    // 1. the hours paid, role by role
    for role in OTHERS {
        let (sh, uncovered) = shifts(c, date, role);
        let (want, got) = workload(c, date, role, &sh);
        pressure.push((role, if want > 0.0 && role != Role::Researcher { ((want / got.max(0.1) - 1.0) * 1.5).clamp(0.0, 1.0) } else { 0.0 }));
        let mut gross = 0.0;
        let (mut paid, mut night) = (0.0, 0.0);
        for e in c.staff.iter().filter(|e| e.role == role && e.employed_on(date)) {
            let h = hourly(e.wage);
            match sh.iter().find(|s| s.id == e.id) {
                Some(s) => {
                    gross += h * (s.paid() as f64 + NIGHT_SUPPLEMENT * s.night() as f64) / 60.0;
                    paid += s.paid() as f64 / 60.0;
                    night += s.night() as f64 / 60.0;
                }
                None if e.absent(date) && on_rota(e, date) => {
                    gross += h * STANDARD_DAY as f64 / 60.0;
                    paid += STANDARD_DAY as f64 / 60.0;
                }
                None => {}
            }
        }
        let wages = (gross * (1.0 + economy::EMPLOYER_SHARE)).round() as Cents;
        if wages > 0 {
            c.book(BookingKind::Wages, -wages, format!("{}: hours of the day ({})", plural(role), sh.len()), false);
        }
        let asked = c.staff_rules.hours_of(role).outside as i32 * (1440 - c.staff_rules.hours_of(role).length());
        let covered = if asked > 0 { (asked - uncovered) as f64 / asked as f64 } else { 1.0 };
        out.roles.push(RoleDay { role, at_work: sh.len() as u32, paid, night, wages, covered });
        all.extend(sh);
    }
    out.workshop = workshop_share(c, date);

    // 2. the cleaners: every bus gets dirtier (one that ran more), the dirtiest are cleaned first
    let (_, got) = workload(c, date, Role::Cleaner, &all);
    let mut capacity = got * 1.5;
    for v in c.fleet.iter_mut().filter(|v| v.held_on(date)) {
        let dirt = if v.in_workshop(date) { 1.5 } else { 7.0 };
        // (the drivers sweep their bus at the end of the day: it never gets filthy)
        v.clean = (v.clean - dirt).max(40.0f64.min(v.clean));
    }
    let mut order: Vec<usize> = (0..c.fleet.len()).filter(|&k| c.fleet[k].held_on(date)).collect();
    order.sort_by(|&a, &b| c.fleet[a].clean.total_cmp(&c.fleet[b].clean));
    for k in order {
        if capacity <= 0.0 {
            break;
        }
        let v = &mut c.fleet[k];
        let part = capacity.min(1.0);
        v.clean = (v.clean + (100.0 - v.clean) * part).min(100.0);
        capacity -= 1.0;
        if part >= 1.0 {
            out.cleaned += 1;
        }
    }
    let held: Vec<f64> = c.fleet.iter().filter(|v| v.held_on(date)).map(|v| v.clean).collect();
    out.clean = if held.is_empty() { 0.0 } else { round1(held.iter().sum::<f64>() / held.len() as f64) };
    // (clean buses please the passengers, dirty ones are talked about)
    let mut lift = if held.is_empty() { 0.0 } else { (out.clean - 70.0) / 30.0 * 0.03 };

    // 3. the service centre: the day's complaints answered, and the passengers' trust
    let (want, got) = workload(c, date, Role::Service, &all);
    let service: Vec<&Employee> = all.iter().filter(|s| s.role == Role::Service).filter_map(|s| c.employee(s.id)).collect();
    if !service.is_empty() {
        let kind = service.iter().map(|e| e.friendliness).sum::<f64>() / service.len() as f64 / 60.0;
        out.complaints = report.incidents.complaints;
        out.handled = ((got * 1.5 * kind).floor() as u32).min(out.complaints);
        let open = if want > 0.0 { (got / want).min(1.0) } else { 0.0 };
        out.service_reputation = 0.03 * out.handled as f64 + 0.05 * open * kind.min(1.2);
        lift += out.service_reputation;
    }
    if lift != 0.0 {
        let before = c.reputation;
        c.reputation = (c.reputation + lift).clamp(0.0, 100.0);
        report.reputation = c.reputation;
        report.reputation_change += c.reputation - before;
    }

    // 4. their experience, stress and friendliness
    let mut tired: Vec<String> = Vec::new();
    for e in c.staff.iter_mut().filter(|e| !drives(e) && e.employed_on(date)) {
        let s = all.iter().find(|s| s.id == e.id);
        let short_rest = match (s, e.last_end) {
            (Some(s), Some(prev)) => 1440 + s.from - prev < NIGHT_REST,
            _ => false,
        };
        let d = match s {
            Some(s) => DayOf { worked: true, minutes: s.minutes(), night: s.night(), short_rest, pressure: pressure.iter().find(|p| p.0 == e.role).map_or(0.0, |p| p.1), away: false },
            None => DayOf { away: e.absent(date), ..Default::default() },
        };
        if let Some(s) = s {
            e.experience = (e.experience + 0.12).min(100.0);
            e.days_worked += 1;
            e.week_days += 1;
            e.last_end = Some(s.end());
        } else {
            e.last_end = None;
        }
        let was = e.stress;
        needs(e, &d);
        if e.stress >= 85.0 && was < 85.0 {
            tired.push(e.name.clone());
        }
    }
    for name in tired {
        inbox::post(c, Message::new(Kind::Staff, "%{name} is worn out", "%{name} is badly stressed. Overtime, nights, short rests and too few colleagues wear people out: they fall ill more often and are less friendly. Days off and more colleagues help.").arg("name", name));
    }

    // 5. requests for a raise: unanswered ones are refused by silence; the underpaid ask
    let mut rng = Rng::of(&[&c.id, "raises"], day_no);
    let gone: Vec<RaiseAsk> = c.staff_rules.asks.iter().filter(|a| dates::between(&a.until, date) > 0 || c.employee(a.id).is_none_or(|e| e.notice_until.is_some())).cloned().collect();
    c.staff_rules.asks.retain(|a| !gone.contains(a));
    for a in gone {
        let Some(e) = c.staff.iter_mut().find(|e| e.id == a.id) else { continue };
        if e.notice_until.is_some() {
            continue;
        }
        e.satisfaction = (e.satisfaction - 8.0).max(0.0);
        let name = e.name.clone();
        inbox::post(c, Message::new(Kind::Staff, "No answer for %{name}", "%{name} asked for a raise and heard nothing back: they take it as a no, and are less satisfied.").arg("name", name));
    }
    let price_index = c.price_index;
    let mut asking: Vec<(u32, f64)> = Vec::new();
    for e in c.staff.iter().filter(|e| e.employed_on(&tomorrow) && e.notice_until.is_none() && dates::between(&e.hired, date) >= 60) {
        if c.staff_rules.ask_of(e.id).is_some() {
            continue;
        }
        let market = market_wage(e.role, e.experience, price_index) as f64;
        let draw = rng.f64();
        if (e.wage as f64) < market * 0.99 && e.satisfaction < 60.0 && draw < 1.0 / 45.0 {
            let share = ((market / e.wage.max(1) as f64 - 1.0 + 0.02).clamp(0.03, 0.12) * 100.0).round() / 100.0;
            asking.push((e.id, share));
        }
    }
    for (id, share) in asking {
        let name = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
        let percent = format!("{:.0}", share * 100.0);
        if c.staff_rules.grant_small && share <= 0.05 + 1e-9 {
            raise(c, id, share);
            inbox::post(c, Message::new(Kind::Staff, "%{name} got a raise", "%{name} asked for %{n} % more pay, and got it: raises of up to 5 % are granted without asking (the Staff page's settings).").arg("name", name).arg("n", percent));
        } else {
            c.staff_rules.asks.push(RaiseAsk { id, share, asked: date.to_string(), until: dates::add(date, 7) });
            inbox::post(c, Message::new(Kind::Staff, "%{name} asks for a raise", "%{name} asks for %{n} % more pay: they earn less than the market pays for their experience. Answer on the Staff page within a week; no answer is a no.").arg("name", name).arg("n", percent));
        }
    }

    // 6. the wages follow the prices at a month's end
    if dates::last_of_month(date) {
        let index = c.price_index;
        if c.staff_rules.follow_prices && c.staff_rules.wage_index > 0.0 && index > c.staff_rules.wage_index {
            let k = index / c.staff_rules.wage_index;
            for e in c.staff.iter_mut() {
                e.wage = ((e.wage as f64 * k / 10_00 as f64).round() as Cents) * 10_00;
            }
        }
        c.staff_rules.wage_index = index;
    }

    // 7. the automatic hiring: a role under its minimum takes the week's best applicant
    if c.staff_rules.auto_hire {
        for role in Role::ALL {
            let min = c.staff_rules.minimum_of(role) as usize;
            if staying(c, role, &tomorrow) >= min {
                continue;
            }
            if let Some(name) = hire_best(c, role) {
                inbox::post(c, Message::new(Kind::Staff, "%{name} was hired", "%{name} starts tomorrow: the automatic hiring keeps the minimum you set for their role (the Staff page's settings).").arg("name", name));
            }
        }
    }

    // 8. a new week: this week's applicants' faces count as given; and on a Monday, a role short of
    //    hands says so
    if dates::week_of(&tomorrow) != dates::week_of(date) {
        for a in market(c) {
            note_portrait(c, a.portrait);
        }
    }
    if dates::parse(&tomorrow).map(dates::weekday) == Some(0) {
        for (role, p) in pressure {
            if p < 0.4 {
                continue;
            }
            let (title, text) = match role {
                Role::Mechanic => ("The workshop is short of mechanics", "Your mechanics cannot keep up with the fleet: more of the maintenance and repairs go to an outside garage, and they are under stress. Hire mechanics, or let them work longer hours."),
                Role::Cleaner => ("The buses are not cleaned enough", "Your cleaners cannot keep up with the fleet: the passengers notice dirty buses. Hire cleaners, or let them work at night, when the buses are in."),
                Role::Service => ("The service centre is overrun", "Too few service employees for your passengers: complaints go unanswered and they are under stress. Hire service employees."),
                _ => continue,
            };
            inbox::post(c, Message::new(Kind::Staff, title, text));
        }
    }
    c.staff_rules.last = Some(out);
}

/// The people of a role who work on `day` and are not leaving.
pub fn staying(c: &Company, role: Role, day: &str) -> usize {
    c.staff.iter().filter(|e| e.role == role && e.employed_on(day) && e.notice_until.is_none()).count()
}

/// The week's best applicant of a role with the D licence: the most experience for what they
/// ask against the market (None: nobody, or a role the company may not hire yet).
pub fn best_applicant(c: &Company, role: Role) -> Option<Applicant> {
    hireable(c, role).ok()?;
    let pi = c.price_index;
    applicants_of(c, role).into_iter().filter(|a| a.licence == Licence::D).max_by(|a, b| {
        let score = |x: &Applicant| x.experience - 100.0 * (x.wage as f64 / market_wage(role, x.experience, pi) as f64 - 1.0);
        score(a).total_cmp(&score(b))
    })
}

/// Hire the week's best applicant of a role while the cash holds two months of what they cost
/// (the automatic hiring, and the easy mode's `autopilot`). Returns their name.
pub fn hire_best(c: &mut Company, role: Role) -> Option<String> {
    let a = best_applicant(c, role)?;
    if c.cash < 2 * economy::employer_cost(a.wage) {
        return None;
    }
    hire(c, &a).ok().map(|_| a.name)
}

/// Answer someone's request for a raise: granted, they get it (and like it); refused, they are
/// less satisfied.
pub fn answer_raise(c: &mut Company, id: u32, grant: bool) -> Result<(), &'static str> {
    let Some(k) = c.staff_rules.asks.iter().position(|a| a.id == id) else { return Err("They have not asked for a raise.") };
    let ask = c.staff_rules.asks.remove(k);
    if grant {
        raise(c, id, ask.share);
        if let Some(e) = c.staff.iter_mut().find(|e| e.id == id) {
            e.satisfaction = (e.satisfaction + 5.0).min(100.0);
        }
    } else if let Some(e) = c.staff.iter_mut().find(|e| e.id == id) {
        e.satisfaction = (e.satisfaction - 6.0).max(0.0);
    }
    Ok(())
}

// --- the portraits --------------------------------------------------------------------------

fn valid(p: u32) -> bool {
    (1..=PORTRAITS).contains(&p)
}

/// The portraits of women, in order (the rest are men's), told by eye from the pictures (Luc:
/// the male faces with the male names, the female with the female).
const FEMALE_PORTRAITS: [u32; 145] = [
    2, 3, 5, 7, 10, 11, 13, 17, 20, 21, 23, 24, 26, 27, 28, 33, 35, 38, 39, 43, 45, 47, 49, 50,
    51, 53, 54, 56, 57, 58, 64, 66, 68, 72, 75, 79, 80, 83, 85, 86, 87, 90, 92, 93, 96, 99, 100, 102,
    104, 106, 108, 110, 112, 114, 116, 118, 120, 122, 124, 126, 128, 130, 132, 134, 136, 138, 140, 141, 143, 145, 147, 149,
    152, 154, 156, 158, 160, 161, 163, 165, 167, 170, 172, 174, 176, 178, 180, 181, 183, 185, 187, 189, 192, 194, 196, 198,
    200, 201, 204, 206, 208, 210, 212, 214, 216, 218, 220, 221, 223, 226, 228, 232, 234, 236, 238, 242, 244, 246, 248, 250,
    252, 254, 256, 258, 260, 261, 263, 265, 267, 269, 272, 274, 276, 278, 280, 282, 284, 286, 288, 290, 291, 293, 295, 297,
    299,
];

/// Whether portrait `p` is a woman's.
pub fn female_portrait(p: u32) -> bool {
    FEMALE_PORTRAITS.binary_search(&p).is_ok()
}

/// Whether the person called `name` is a woman (by their first name: Some(true)), a man
/// (Some(false)), or the name says nothing (None: any face).
pub fn female_name(name: &str) -> Option<bool> {
    let first = name.split_whitespace().next()?;
    FIRST.iter().chain(OLDER_FIRST.iter()).find(|(n, _)| n.eq_ignore_ascii_case(first)).map(|(_, f)| *f)
}

/// Whether face `p` suits the person called `name`.
fn suits(name: &str, p: u32) -> bool {
    female_name(name).is_none_or(|f| f == female_portrait(p))
}

/// The first face of `free` that suits `name` (taken out of it), else none.
fn take_suiting(free: &mut Vec<u32>, name: &str) -> u32 {
    match free.iter().position(|p| suits(name, *p)) {
        Some(k) => free.remove(k),
        None => 0,
    }
}

/// A face for someone called `name` when none was free (a company of more than `PORTRAITS`
/// people): one of their gender's, by `n`.
pub fn fallback_portrait(name: &str, n: u32) -> u32 {
    let pool: Vec<u32> = (1..=PORTRAITS).filter(|p| suits(name, *p)).collect();
    if pool.is_empty() {
        n % PORTRAITS + 1
    } else {
        pool[n as usize % pool.len()]
    }
}

/// The order a company gives its faces in (its own shuffle of all of them).
fn portrait_order(c: &Company) -> Vec<u32> {
    let mut v: Vec<u32> = (1..=PORTRAITS).collect();
    let mut rng = Rng::of(&[&c.id, "portraits"], 0);
    for i in (1..v.len()).rev() {
        let j = rng.int(0, i as i64) as usize;
        v.swap(i, j);
    }
    v
}

/// The portraits free to give besides `held`, best first: the faces the company never gave,
/// then those it gave before whose person or applicant is gone (the longest ago first).
pub fn free_portraits(c: &Company, held: &[u32]) -> Vec<u32> {
    let mut used = [false; PORTRAITS as usize + 1];
    for p in c.staff_rules.portraits_used.iter().filter(|p| valid(**p)) {
        used[*p as usize] = true;
    }
    let mut taken = [false; PORTRAITS as usize + 1];
    for p in held.iter().filter(|p| valid(**p)) {
        taken[*p as usize] = true;
    }
    let mut out: Vec<u32> = portrait_order(c).into_iter().filter(|p| !used[*p as usize] && !taken[*p as usize]).collect();
    let mut seen = [false; PORTRAITS as usize + 1];
    for p in c.staff_rules.portraits_used.iter().copied().filter(|p| valid(*p)) {
        if !taken[p as usize] && !seen[p as usize] {
            seen[p as usize] = true;
            out.push(p);
        }
    }
    out
}

/// A face given: counted as used (at the end of the list: given the latest).
fn note_portrait(c: &mut Company, p: u32) {
    if !valid(p) {
        return;
    }
    let used = &mut c.staff_rules.portraits_used;
    used.retain(|x| *x != p);
    used.push(p);
}

/// The faces the staff holds.
fn held_portraits(c: &Company) -> Vec<u32> {
    c.staff.iter().map(|e| e.portrait).filter(|p| valid(*p)).collect()
}

/// A company opened: everyone gets a face of their own (who has none, or one another has too,
/// gets a free one - a company of more than `PORTRAITS` people leaves the rest without), and
/// the faces held count as given.
pub fn migrate(c: &mut Company) {
    let mut held: Vec<u32> = Vec::new();
    let mut without: Vec<usize> = Vec::new();
    for (k, e) in c.staff.iter().enumerate() {
        // (a face of the other gender than the name's is given anew)
        if valid(e.portrait) && !held.contains(&e.portrait) && suits(&e.name, e.portrait) {
            held.push(e.portrait);
        } else {
            without.push(k);
        }
    }
    for p in held.clone() {
        if !c.staff_rules.portraits_used.contains(&p) {
            c.staff_rules.portraits_used.push(p);
        }
    }
    for k in without {
        let mut free = free_portraits(c, &held);
        let p = take_suiting(&mut free, &c.staff[k].name);
        c.staff[k].portrait = p;
        if valid(p) {
            held.push(p);
            note_portrait(c, p);
        }
    }
}

// --- the labour market ---------------------------------------------------------------------

/// The applicants' first names, each with whether it is a woman's (their face follows it).
const FIRST: [(&str, bool); 40] = [
    ("Anna", true), ("Ben", false), ("Carla", true), ("Dennis", false), ("Elif", true), ("Frank", false), ("Greta", true), ("Hakan", false), ("Ines", true), ("Jan", false),
    ("Klaus", false), ("Lena", true), ("Mehmet", false), ("Nina", true), ("Olaf", false), ("Petra", true), ("Rainer", false), ("Sanne", true), ("Tobias", false), ("Ute", true),
    ("Volker", false), ("Wiebke", true), ("Yusuf", false), ("Zoë", true), ("Bram", false), ("Daan", false), ("Emma", true), ("Fleur", true), ("Joost", false), ("Marieke", true),
    ("Pieter", false), ("Sophie", true), ("Agnieszka", true), ("Piotr", false), ("Olena", true), ("Dmytro", false), ("Marco", false), ("Aylin", true), ("Jonas", false), ("Heike", true),
];
/// First names no applicant gets any more, which people of older companies still carry (and
/// the rivals' managers): their face follows them too (`female_name`).
const OLDER_FIRST: [(&str, bool); 12] = [
    ("Mia", true), ("Lea", true), ("Clara", true), ("Nora", true), ("Lisa", true), ("Julia", true),
    ("Paul", false), ("Felix", false), ("Lukas", false), ("Tim", false), ("Max", false), ("Erik", false),
];
const LAST: [&str; 32] = [
    "Becker", "de Vries", "Fischer", "Hoffmann", "Jansen", "Kaya", "Krüger", "Meijer", "Müller", "Peters", "Richter", "Schmidt", "Schulz", "van Dijk", "Vogel", "Wagner", "Weber", "Wolf", "Yilmaz", "Zimmermann",
    "Bakker", "Visser", "Smit", "Mulder", "Kowalski", "Nowak", "Shevchenko", "Bondarenko", "Rossi", "Demir", "Lange", "Koch",
];

/// Someone applying this week.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Applicant {
    pub no: u32,
    pub name: String,
    pub age: u32,
    pub experience: f64,
    pub licence: Licence,
    /// The monthly gross wage they ask.
    pub wage: Cents,
    pub reliability: f64,
    pub skills: Skills,
    /// Endorsements they bring (`licences`).
    #[serde(default)]
    pub endorsements: Vec<Endorsement>,
    /// The work they apply for.
    #[serde(default)]
    pub role: Role,
    /// Their face, one nobody of the company has (0: none was free).
    #[serde(default)]
    pub portrait: u32,
}

impl Applicant {
    /// Their face, 1 to `PORTRAITS` (one of their gender's by their number when none was free).
    pub fn portrait(&self) -> u32 {
        if valid(self.portrait) {
            self.portrait
        } else {
            fallback_portrait(&self.name, self.no.wrapping_mul(53))
        }
    }

    /// How friendly they will be.
    pub fn friendliness(&self) -> f64 {
        friendliness_of(self.experience, self.skills.service)
    }
}

/// The driver applicants of the company's week: the same all week, new ones on Monday, those
/// hired gone. More beginners than old hands, as on the real market; wages around the
/// market's for their experience.
pub fn applicants(c: &Company) -> Vec<Applicant> {
    market(c).into_iter().filter(|a| a.role == Role::Driver).collect()
}

/// The applicants of a role this week.
pub fn applicants_of(c: &Company, role: Role) -> Vec<Applicant> {
    market(c).into_iter().filter(|a| a.role == role).collect()
}

/// Every applicant of the week, the drivers first, each with a face of their own.
pub fn market(c: &Company) -> Vec<Applicant> {
    let mut out = drivers_market(c);
    for role in OTHERS {
        out.extend(role_market(c, role));
    }
    // (a face of their gender's: the men's faces for men's names, the women's for women's)
    let mut free = free_portraits(c, &held_portraits(c));
    for a in out.iter_mut() {
        a.portrait = take_suiting(&mut free, &a.name);
    }
    out
}

fn taken(c: &Company, week: i64, no: u32) -> bool {
    c.taken.week == week && c.taken.applicants.contains(&no)
}

fn drivers_market(c: &Company) -> Vec<Applicant> {
    let r = economy::rules(c.difficulty);
    let week = dates::week_of(&c.date);
    let mut rng = Rng::of(&[&c.id, "applicants"], week);
    let mut out = Vec::new();
    for no in 0..r.applicants as u32 {
        let name = format!("{} {}", rng.pick(&FIRST).map(|f| f.0).unwrap_or("Alex"), rng.pick(&LAST).unwrap_or(&"Weber"));
        let experience = (rng.f64().powf(1.5) * 90.0 + 3.0).round();
        let age = (21.0 + experience * 0.35 + rng.range(0.0, 18.0)).round().min(63.0) as u32;
        let licence = if rng.chance(0.1) { Licence::D1 } else { Licence::D };
        let skill = |rng: &mut Rng| (35.0 + experience * 0.45 + rng.range(-15.0, 20.0)).clamp(10.0, 100.0).round();
        let skills = Skills { driving: skill(&mut rng), punctuality: skill(&mut rng), service: skill(&mut rng) };
        let reliability = (rng.range(0.72, 0.99) * 100.0).round() / 100.0;
        // (endorsements of their own, more with experience; each asks 3 % more)
        let mut endorsements = Vec::new();
        for (x, p) in [(Endorsement::Articulated, 0.25 + experience / 200.0), (Endorsement::DoubleDecker, 0.1 + experience / 400.0), (Endorsement::Electric, 0.15 + experience / 400.0)] {
            if licence == Licence::D && rng.chance(p) {
                endorsements.push(x);
            }
        }
        let wage = ((economy::market_wage(experience, c.price_index) as f64 * rng.range(0.94, 1.10) * (1.0 + 0.03 * endorsements.len() as f64) / 10_00 as f64).round() as Cents) * 10_00;
        if !taken(c, week, no) {
            out.push(Applicant { no, name, age, experience, licence, wage, reliability, skills, endorsements, role: Role::Driver, portrait: 0 });
        }
    }
    out
}

/// The applicants of a role besides the drivers: fewer of them (Easy four a week, Realistic
/// three, Hard two), numbered from a hundred for each role.
fn role_market(c: &Company, role: Role) -> Vec<Applicant> {
    let r = economy::rules(c.difficulty);
    let week = dates::week_of(&c.date);
    let mut rng = Rng::of(&[&c.id, "applicants", key(role)], week);
    let mut out = Vec::new();
    for k in 0..(r.applicants / 2).max(2) as u32 {
        let no = 100 * role_index(role) + k;
        let name = format!("{} {}", rng.pick(&FIRST).map(|f| f.0).unwrap_or("Alex"), rng.pick(&LAST).unwrap_or(&"Weber"));
        let experience = (rng.f64().powf(1.3) * 90.0 + 3.0).round();
        let age = (19.0 + experience * 0.35 + rng.range(0.0, 20.0)).round().min(64.0) as u32;
        let skill = |rng: &mut Rng, base: f64| (base + experience * 0.3 + rng.range(-15.0, 22.0)).clamp(10.0, 100.0).round();
        let service = skill(&mut rng, if matches!(role, Role::Service | Role::Inspector) { 45.0 } else { 32.0 });
        let punctuality = skill(&mut rng, 38.0);
        let reliability = (rng.range(0.74, 0.99) * 100.0).round() / 100.0;
        let wage = ((market_wage(role, experience, c.price_index) as f64 * rng.range(0.94, 1.10) / 10_00 as f64).round() as Cents) * 10_00;
        if !taken(c, week, no) {
            out.push(Applicant { no, name, age, experience, licence: Licence::D, wage, reliability, skills: Skills { driving: 0.0, punctuality, service }, endorsements: Vec::new(), role, portrait: 0 });
        }
    }
    out
}

/// Hire an applicant of the week. Returns their id.
pub fn hire(c: &mut Company, a: &Applicant) -> Result<u32, &'static str> {
    hireable(c, a.role)?;
    let week = dates::week_of(&c.date);
    if c.taken.week == week && c.taken.applicants.contains(&a.no) {
        return Err("They have found work elsewhere.");
    }
    if c.taken.week != week {
        c.taken = Taken { week, used: Vec::new(), applicants: Vec::new() };
    }
    c.taken.applicants.push(a.no);
    c.counters.employee += 1;
    let id = c.counters.employee;
    // (the year's holidays pro rata)
    let left = dates::parse(&c.date).map(dates::civil_from_days).map(|(y, m, day)| {
        let year_end = dates::days_from_civil(y, 12, 31);
        let today = dates::days_from_civil(y, m, day);
        ((year_end - today + 1) as f64 / 365.0 * HOLIDAYS as f64).round() as u32
    });
    // (their face: the one they applied with, unless someone has it by now)
    let held = held_portraits(c);
    let portrait = if valid(a.portrait) && !held.contains(&a.portrait) { a.portrait } else { free_portraits(c, &held).first().copied().unwrap_or(0) };
    note_portrait(c, portrait);
    c.staff.push(Employee {
        id,
        name: a.name.clone(),
        age: a.age,
        experience: a.experience,
        licence: a.licence,
        wage: a.wage,
        reliability: a.reliability,
        skills: a.skills,
        satisfaction: 65.0,
        hired: c.date.clone(),
        notice_until: None,
        resigned: false,
        sick_until: None,
        holiday_until: None,
        training_until: None,
        holiday_left: left.unwrap_or(HOLIDAYS),
        week_days: 0,
        last_end: None,
        days_worked: 0,
        endorsements: a.endorsements.clone(),
        types: Vec::new(),
        role: a.role,
        stress: 15.0,
        friendliness: a.friendliness(),
        portrait,
    });
    // (the induction: the fleet's models)
    if a.role == Role::Driver {
        super::licences::induction(c, id);
    }
    Ok(id)
}

/// Dismiss someone: they work their notice and leave after it. Returns their last day.
pub fn dismiss(c: &mut Company, id: u32) -> Result<String, &'static str> {
    let r = economy::rules(c.difficulty);
    let today = c.date.clone();
    let Some(e) = c.staff.iter_mut().find(|e| e.id == id) else { return Err("They do not work here.") };
    if e.notice_until.is_some() {
        return Err("They are leaving already.");
    }
    let until = dates::add(&today, r.notice_days);
    e.notice_until = Some(until.clone());
    e.resigned = false;
    // (the others do not like it)
    for o in c.staff.iter_mut().filter(|o| o.id != id) {
        o.satisfaction = (o.satisfaction - 3.0).max(0.0);
    }
    c.staff_rules.asks.retain(|a| a.id != id);
    Ok(until)
}

/// Take a dismissal back (not a resignation).
pub fn withdraw_notice(c: &mut Company, id: u32) -> Result<(), &'static str> {
    match c.staff.iter_mut().find(|e| e.id == id) {
        Some(e) if e.notice_until.is_some() && !e.resigned => {
            e.notice_until = None;
            Ok(())
        }
        Some(_) => Err("That cannot be taken back."),
        None => Err("They do not work here."),
    }
}

/// A raise of `share` (0.05 = 5 %), to the ten euros. A request for a raise is answered by it.
pub fn raise(c: &mut Company, id: u32, share: f64) {
    if let Some(e) = c.staff.iter_mut().find(|e| e.id == id) {
        e.wage = ((e.wage as f64 * (1.0 + share) / 10_00 as f64).round() as Cents) * 10_00;
        e.satisfaction = (e.satisfaction + 100.0 * share * 2.0).min(100.0);
    }
    c.staff_rules.asks.retain(|a| a.id != id || a.share > share + 1e-9);
}

/// What someone costs a month (gross and the employer's share).
pub fn monthly_cost(e: &Employee) -> Cents {
    economy::employer_cost(e.wage)
}

/// Whether someone's wage is booked by the month (the drivers); the others are paid their hours
/// day by day (`roles_day`).
pub fn paid_monthly(e: &Employee) -> bool {
    drives(e)
}

/// What happened to someone tonight (for the day's report).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StaffNote {
    Sick { name: String, until: String },
    Holiday { name: String, until: String },
    Resigned { name: String, until: String },
    Unhappy { name: String },
    Left { name: String },
}

/// The wage for the days of `month` someone was employed (pro rata).
pub fn wage_for_month(e: &Employee, month: &str) -> Cents {
    let Some(first) = dates::parse(&format!("{month}-01")) else { return 0 };
    let (y, m, _) = dates::civil_from_days(first);
    let len = dates::days_in_month(y, m) as i64;
    let last = first + len - 1;
    let from = dates::parse(&e.hired).unwrap_or(first).max(first);
    let to = e.notice_until.as_deref().and_then(dates::parse).unwrap_or(last).min(last);
    let days = (to - from + 1).max(0);
    (economy::employer_cost(e.wage) as f64 * days as f64 / len as f64).round() as Cents
}

/// Whether the inbox told about `name` under `title` in the last `days` days.
fn told_lately(c: &Company, title: &str, name: &str, days: i64) -> bool {
    c.inbox.messages.iter().rev().take(400).any(|m| m.title == title && m.args.iter().any(|(k, v)| k == "name" && v == name) && dates::between(&m.date, &c.date) < days)
}

/// The night after a working day: experience, stress and friendliness for the drivers
/// (`worked`: id, minutes, the end of their last duty and the start of their first), satisfaction
/// for everyone moving towards what their wage, their hours and their stress make of it,
/// illness (more under stress) and holidays, resignations, and those whose notice ends leave
/// (their last wage and, on Hard, a severance pay booked). The inbox hears of long illnesses,
/// of who thinks of leaving and of who resigned.
pub fn after_day(c: &mut Company, worked: &[(u32, i32, i32, i32)], rng: &mut Rng) -> Vec<StaffNote> {
    let r = economy::rules(c.difficulty);
    let today = c.date.clone();
    let mut notes = Vec::new();
    let mut leaving = Vec::new();
    let mut told: Vec<Message> = Vec::new();
    // (how many of the drivers who could drive did: no one in reserve is felt)
    let available = c.staff.iter().filter(|e| drives(e) && e.employed_on(&today) && !e.absent(&today)).count();
    let pressure = if available > 0 { ((worked.len() as f64 / available as f64 - 0.75) * 4.0).clamp(0.0, 1.0) } else { 0.0 };
    let price_index = c.price_index;
    for e in c.staff.iter_mut() {
        let w = if drives(e) { worked.iter().find(|w| w.0 == e.id) } else { None };
        if drives(e) {
            match w {
                Some(&(_, minutes, end, start)) => {
                    e.experience = (e.experience + 0.12).min(100.0);
                    e.week_days += 1;
                    e.days_worked += 1;
                    let short_rest = e.last_end.is_some_and(|p| 1440 + start - p < NIGHT_REST);
                    e.last_end = Some(end);
                    needs(e, &DayOf { worked: true, minutes, night: night_minutes(start, end.max(start)), short_rest, pressure, away: false });
                }
                None => {
                    e.last_end = None;
                    let away = e.absent(&today);
                    needs(e, &DayOf { away, ..Default::default() });
                }
            }
        }
        let market = market_wage(e.role, e.experience, price_index) as f64;
        let overtime = w.is_some_and(|w| w.1 > DAY_TARGET);
        let holiday = e.holiday_until.as_deref().is_some_and(|u| dates::between(&today, u) >= 0);
        let target = (60.0 + 150.0 * (e.wage as f64 / market - 1.0) - if overtime { 8.0 } else { 0.0 } - (e.stress - 35.0).max(0.0) * 0.5 + if holiday { 6.0 } else { 0.0 }).clamp(0.0, 100.0);
        let was = e.satisfaction;
        e.satisfaction = ((e.satisfaction + (target - e.satisfaction) * 0.1) * 10.0).round() / 10.0;
        if e.satisfaction < 35.0 && was >= 35.0 {
            notes.push(StaffNote::Unhappy { name: e.name.clone() });
        }
        if e.satisfaction < 30.0 && was >= 30.0 && e.notice_until.is_none() {
            told.push(Message::new(Kind::Staff, "%{name} thinks of leaving", "%{name} is unhappy here: their pay, their hours or their stress. Under 25 they may hand in their notice. A raise, fewer hours or a few days off may keep them.").arg("name", e.name.clone()));
        }
        let away = e.absent(&dates::add(&today, 1));
        if !away && e.notice_until.is_none() {
            let stressed = 1.0 + (e.stress - 30.0).max(0.0) / 100.0 * 1.5;
            if rng.chance(r.sickness * (1.6 - e.reliability) * stressed) {
                let days = rng.int(1, 7);
                let until = dates::add(&today, days);
                e.sick_until = Some(until.clone());
                notes.push(StaffNote::Sick { name: e.name.clone(), until: until.clone() });
                if days >= 5 {
                    told.push(Message::new(Kind::Staff, "%{name} is ill for a long time", "%{name} is ill for %{n} days. Their work waits or falls to the others.").arg("name", e.name.clone()).arg("n", days.to_string()));
                }
            } else if e.holiday_left >= 5 && rng.chance(0.012) {
                let days = rng.int(5, 10.min(e.holiday_left as i64)) as u32;
                let until = dates::add(&today, days as i64);
                e.holiday_until = Some(until.clone());
                e.holiday_left -= (days * 5).div_ceil(7);
                notes.push(StaffNote::Holiday { name: e.name.clone(), until });
            }
        }
        if e.notice_until.is_none() && e.satisfaction < 25.0 && rng.chance(0.03) {
            let until = dates::add(&today, 14);
            e.notice_until = Some(until.clone());
            e.resigned = true;
            notes.push(StaffNote::Resigned { name: e.name.clone(), until });
            told.push(Message::new(Kind::Staff, "%{name} has resigned", "%{name} has handed in their notice and leaves in two weeks. A resignation cannot be taken back; the week's applicants are on the Staff page.").arg("name", e.name.clone()));
        }
        if e.notice_until.as_deref() == Some(today.as_str()) {
            leaving.push(e.id);
        }
    }
    for m in told {
        let name = m.args.iter().find(|a| a.0 == "name").map(|a| a.1.clone()).unwrap_or_default();
        if !told_lately(c, &m.title, &name, 14) {
            inbox::post(c, m);
        }
    }
    for id in leaving {
        let Some(e) = c.staff.iter().find(|e| e.id == id).cloned() else { continue };
        if paid_monthly(&e) {
            c.book(BookingKind::Wages, -wage_for_month(&e, &dates::month_of(&today)), e.name.clone(), false);
        } else {
            // (paid by the hour: their last day)
            let last = (hourly(e.wage) * STANDARD_DAY as f64 / 60.0 * (1.0 + economy::EMPLOYER_SHARE)).round() as Cents;
            c.book(BookingKind::Wages, -last, e.name.clone(), false);
        }
        if !e.resigned && r.severance_months_per_year > 0.0 {
            let years = dates::years_between(&e.hired, &today);
            c.book(BookingKind::Severance, -(e.wage as f64 * r.severance_months_per_year * years).round() as Cents, e.name.clone(), false);
        }
        c.staff.retain(|x| x.id != id);
        c.staff_rules.asks.retain(|a| a.id != id);
        notes.push(StaffNote::Left { name: e.name });
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::company::model::Difficulty;

    fn block(key: &str, from: i32, to: i32) -> Block {
        Block { key: key.into(), from, to, from_stop: "A".into(), to_stop: "B".into() }
    }

    /// Faces follow the names (Luc): every applicant of a week has a face of their name's
    /// gender, none twice; a company opened gives someone with a face of the other gender a
    /// fitting one; a name that says nothing takes any face.
    #[test]
    fn faces_follow_the_names_gender() {
        assert!(FEMALE_PORTRAITS.windows(2).all(|w| w[0] < w[1]) && FEMALE_PORTRAITS.iter().all(|p| valid(*p)));
        assert_eq!(FIRST.iter().filter(|f| f.1).count(), 20);
        let mut c = found(&Founding { name: "Faces".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash = 10_000_000_00;
        let week = market(&c);
        let mut seen = Vec::new();
        for a in &week {
            let f = female_name(&a.name).expect("a name of the list");
            assert_eq!(female_portrait(a.portrait), f, "{} has portrait {}", a.name, a.portrait);
            assert!(!seen.contains(&a.portrait), "portrait {} twice", a.portrait);
            seen.push(a.portrait);
        }
        // hired, then given a face of the other gender: mended on opening
        let id = hire(&mut c, &week[0]).expect("hired");
        let e = c.staff.iter_mut().find(|e| e.id == id).unwrap();
        e.name = "Anna Becker".into();
        e.portrait = 1;
        assert!(!female_portrait(1));
        migrate(&mut c);
        let e = c.staff.iter().find(|e| e.id == id).unwrap();
        assert!(female_portrait(e.portrait), "{}", e.portrait);
        assert_eq!(female_name("Agency driver"), None);
        // (older companies' names follow too: Mia with a woman's face, Paul with a man's)
        assert_eq!((female_name("Mia Schulz"), female_name("Paul Richter")), (Some(true), Some(false)));
        assert!(suits("Agency driver", 1) && suits("Agency driver", 2));
    }

    #[test]
    fn the_working_time_rules_of_omsi_hub() {
        let morning = block("m", 6 * 60, 10 * 60);
        // overlapping
        assert_eq!(check(&[morning.clone()], &block("x", 9 * 60, 11 * 60), None, None).double.as_deref(), Some("m"));
        // a short pause and another stop: a transfer warning, but allowed
        let mut after = block("a", 10 * 60 + 25, 14 * 60);
        after.from_stop = "C".into();
        let k = check(&[morning.clone()], &after, None, None);
        assert_eq!((k.rest, k.transfer, k.too_long, k.allowed()), (Some(25), Some(25), false, true));
        // too short a rest
        assert!(!check(&[morning.clone()], &block("b", 10 * 60 + 10, 12 * 60), None, None).allowed());
        // a long day: overtime above 8 hours, too long above 10
        let k = check(&[morning.clone()], &block("c", 11 * 60, 15 * 60 + 30), None, None);
        assert_eq!((k.overtime, k.too_long), (30, false));
        assert!(check(&[morning.clone()], &block("d", 11 * 60, 17 * 60 + 30), None, None).too_long);
        // the night: yesterday ended at 22:00, today begins at 6:00 - only 8 hours
        let k = check(&[], &morning, Some(22 * 60), None);
        assert_eq!(k.night_rest, Some(480));
        assert!(!k.allowed());
        assert!(check(&[], &morning, Some(19 * 60), None).allowed());
    }

    #[test]
    fn hiring_dismissing_and_the_wages() {
        let mut c = found(&Founding { name: "Leute".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let market = applicants(&c);
        assert_eq!(market.len(), 6);
        assert!(market.iter().all(|a| (2_500_00..4_500_00).contains(&a.wage) && (21..=63).contains(&a.age)));
        let a = market[0].clone();
        let id = hire(&mut c, &a).unwrap();
        assert_eq!(applicants(&c).len(), 5);
        assert!(hire(&mut c, &a).is_err());
        let e = c.employee(id).unwrap().clone();
        // hired on the 4th of March: the wage for 28 of 31 days
        assert_eq!(wage_for_month(&e, "2024-03"), (economy::employer_cost(e.wage) as f64 * 28.0 / 31.0).round() as Cents);
        assert_eq!(wage_for_month(&e, "2024-04"), economy::employer_cost(e.wage));
        assert_eq!(wage_for_month(&e, "2024-02"), 0);
        // dismissed: four weeks' notice on Realistic
        let until = dismiss(&mut c, id).unwrap();
        assert_eq!(until, "2024-04-01");
        assert!(c.employee(id).unwrap().employed_on("2024-04-01") && !c.employee(id).unwrap().employed_on("2024-04-02"));
        withdraw_notice(&mut c, id).unwrap();
        assert!(c.employee(id).unwrap().notice_until.is_none());
        // the licence and the experience for the big buses
        let mut e = c.employee(id).unwrap().clone();
        e.experience = 10.0;
        assert!(may_drive(&e, BusSize::Articulated) && !qualified(&e, BusSize::Articulated));
        e.licence = Licence::D1;
        assert!(!may_drive(&e, BusSize::Solo) && may_drive(&e, BusSize::Midi));
        // someone who does not drive drives nothing
        e.licence = Licence::D;
        e.role = Role::Mechanic;
        assert!(!may_drive(&e, BusSize::Midi) && !qualified(&e, BusSize::Solo));
    }

    #[test]
    fn someone_whose_notice_ends_leaves_with_the_last_wage() {
        let mut c = found(&Founding { name: "Weg".into(), difficulty: Difficulty::Hard, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let a = applicants(&c)[0].clone();
        let id = hire(&mut c, &a).unwrap();
        c.staff[0].hired = "2022-03-04".into();
        dismiss(&mut c, id).unwrap();
        c.date = c.staff[0].notice_until.clone().unwrap();
        let cash = c.cash;
        let notes = after_day(&mut c, &[], &mut Rng::new(1));
        assert!(c.staff.is_empty());
        assert!(notes.contains(&StaffNote::Left { name: a.name.clone() }));
        // the wage of April's first day, and on Hard half a month's wage per year employed
        let severance = (a.wage as f64 * 0.5 * dates::years_between("2022-03-04", "2024-04-01")).round() as Cents;
        let wage = (economy::employer_cost(a.wage) as f64 / 30.0).round() as Cents;
        assert_eq!(cash - c.cash, severance + wage);
    }

    fn company() -> Company {
        found(&Founding { name: "Rollen".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), ..Default::default() }, "Luc")
    }

    /// Hire `n` of a role (another week's market when this one runs out).
    fn hire_n(c: &mut Company, role: Role, n: usize) -> Vec<u32> {
        let mut ids = Vec::new();
        let (id0, week) = (c.id.clone(), c.taken.clone());
        let mut round = 0;
        while ids.len() < n {
            for a in applicants_of(c, role) {
                if ids.len() < n {
                    ids.push(hire(c, &a).unwrap());
                }
            }
            round += 1;
            c.id = format!("{id0}{round}");
            c.taken = Default::default();
        }
        c.id = id0;
        c.taken = week;
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until) = (None, None);
        }
        ids
    }

    #[test]
    fn every_role_has_its_own_market_and_inspectors_wait_for_the_research() {
        let mut c = company();
        let all = market(&c);
        for role in Role::ALL {
            let mine: Vec<&Applicant> = all.iter().filter(|a| a.role == role).collect();
            assert_eq!(mine.len(), if role == Role::Driver { 6 } else { 3 }, "{role:?}");
            // (wages around the role's market)
            for a in mine {
                let m = market_wage(role, a.experience, 1.0) as f64;
                assert!((a.wage as f64) > m * 0.9 && (a.wage as f64) < m * 1.15, "{role:?} {}", a.wage);
            }
        }
        // the numbers are unique, the drivers' market is as it was
        let mut nos: Vec<u32> = all.iter().map(|a| a.no).collect();
        nos.dedup();
        assert_eq!(nos.len(), all.len());
        assert!(applicants(&c).iter().all(|a| a.role == Role::Driver && a.no < 100));
        // a ticket inspector only after the research
        let inspector = applicants_of(&c, Role::Inspector)[0].clone();
        assert!(hire(&mut c, &inspector).is_err());
        c.research.done.push(super::super::research::keys::TICKET_INSPECTORS.to_string());
        let id = hire(&mut c, &inspector).unwrap();
        let e = c.employee(id).unwrap();
        assert_eq!(e.role, Role::Inspector);
        assert!(!drives(e) && e.types.is_empty());
        assert_eq!(applicants_of(&c, Role::Inspector).len(), 2);
    }

    #[test]
    fn no_two_people_share_a_face() {
        let mut c = company();
        c.research.done.push(super::super::research::keys::TICKET_INSPECTORS.to_string());
        let faces = |c: &Company| -> Vec<u32> { c.staff.iter().map(|e| e.portrait()).chain(market(c).iter().map(|a| a.portrait())).collect() };
        let unique = |v: &[u32]| {
            let mut s = v.to_vec();
            s.sort();
            s.dedup();
            s.len() == v.len()
        };
        let before = market(&c);
        assert!(before.iter().all(|a| valid(a.portrait)) && unique(&faces(&c)));
        // a hire keeps the face they applied with; the others keep theirs
        let a = before[2].clone();
        let id = hire(&mut c, &a).unwrap();
        assert_eq!(c.employee(id).unwrap().portrait, a.portrait);
        let after = market(&c);
        for x in &after {
            assert_eq!(x.portrait, before.iter().find(|b| b.no == x.no).unwrap().portrait, "{}", x.name);
        }
        assert!(unique(&faces(&c)));
        // many weeks of hiring: still no face twice, and new faces before old ones
        for role in Role::ALL {
            hire_n(&mut c, role, 8);
        }
        assert!(unique(&faces(&c)));
        // an old file: nobody has a face, two have the same - all get their own
        let mut old = c.clone();
        for e in old.staff.iter_mut() {
            e.portrait = 0;
        }
        old.staff[1].portrait = 7;
        old.staff[2].portrait = 7;
        old.staff_rules = StaffRules::default();
        migrate(&mut old);
        let held: Vec<u32> = old.staff.iter().map(|e| e.portrait).collect();
        assert!(held.iter().all(|p| valid(*p)) && unique(&held) && held[1] == 7);
        assert!(unique(&faces(&old)));
        assert_eq!(old.staff_rules.portraits_used.len(), old.staff.len());
        // a face comes free when its person leaves, but a face never given comes first
        let gone = old.staff.remove(0).portrait;
        let free = free_portraits(&old, &held_portraits(&old));
        assert!(!free[..PORTRAITS as usize - old.staff_rules.portraits_used.len()].contains(&gone));
        assert!(free.contains(&gone));
    }

    #[test]
    fn an_old_file_opens_with_faces_and_its_buses_as_clean_as_ever() {
        let mut c = company();
        hire_n(&mut c, Role::Driver, 3);
        let bus = super::super::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        super::super::market::buy_new(&mut c, &bus, super::super::market::Payment::Cash, "").unwrap();
        // (a file of before: no roles, stress, friendliness, faces, cleanliness or staff rules)
        let mut v = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("staff_rules");
        for e in v["staff"].as_array_mut().unwrap() {
            for k in ["role", "stress", "friendliness", "portrait"] {
                e.as_object_mut().unwrap().remove(k);
            }
        }
        for b in v["fleet"].as_array_mut().unwrap() {
            b.as_object_mut().unwrap().remove("clean");
        }
        let old: Company = serde_json::from_value(v).unwrap();
        let old = super::super::store::migrate(old);
        assert!(old.staff.iter().all(|e| e.role == Role::Driver && e.stress == 20.0 && e.friendliness == 60.0 && valid(e.portrait)));
        let mut faces: Vec<u32> = old.staff.iter().map(|e| e.portrait).collect();
        faces.sort();
        faces.dedup();
        assert_eq!(faces.len(), old.staff.len());
        assert!(old.fleet.iter().all(|v| v.clean == 75.0));
        assert!(old.staff_rules.follow_prices && old.staff_rules.hours.is_empty() && !old.staff_rules.auto_hire);
        assert_eq!(old.staff_rules.hours_of(Role::Mechanic), default_hours(Role::Mechanic));
        // opened again: nothing changes
        assert_eq!(super::super::store::migrate(old.clone()), old);
    }

    #[test]
    fn more_people_than_faces_reuse_only_then() {
        let mut c = company();
        c.staff_rules.portraits_used = (1..=PORTRAITS).collect();
        // every face given once already: the oldest come back first
        assert_eq!(free_portraits(&c, &[]).first(), Some(&1));
        assert_eq!(free_portraits(&c, &[1, 2]).first(), Some(&3));
        // all held: nothing is free, and an applicant shows one by their number
        let all: Vec<u32> = (1..=PORTRAITS).collect();
        assert!(free_portraits(&c, &all).is_empty());
        let mut a = applicants(&c)[0].clone();
        a.portrait = 0;
        assert!(valid(a.portrait()));
    }

    #[test]
    fn core_hours_and_who_stays_on() {
        let mut c = company();
        let ids = hire_n(&mut c, Role::Mechanic, 4);
        // a Monday: everyone on the rota with a free day elsewhere in the week
        let date = "2024-03-04";
        let at: Vec<u32> = c.staff.iter().filter(|e| at_work(e, date)).map(|e| e.id).collect();
        let (sh, uncovered) = shifts(&c, date, Role::Mechanic);
        assert_eq!(sh.len(), at.len());
        assert!(sh.iter().all(|s| s.minutes() == 480 && s.extra.is_none() && s.night() == 0));
        assert_eq!(uncovered, 0);
        // five days a week each
        let week: Vec<usize> = ids.iter().map(|id| (0..7).filter(|k| on_rota(c.employee(*id).unwrap(), &dates::add(date, *k))).count()).collect();
        assert!(week.iter().all(|n| *n == 5));
        // core hours 06:00-14:00 and one place outside them: the least stressed stay on, at most
        // twelve hours each, the night paid more
        c.research.done.push(super::super::research::keys::SHIFT_SCHEDULE.to_string());
        set_hours(&mut c, Role::Mechanic, Hours { from: 6 * 60, to: 14 * 60, outside: 1 }).unwrap();
        for (k, e) in c.staff.iter_mut().enumerate() {
            e.stress = 50.0 - k as f64;
        }
        let (sh, uncovered) = shifts(&c, date, Role::Mechanic);
        let stayed: Vec<&Shift> = sh.iter().filter(|s| s.extra.is_some()).collect();
        assert!(stayed.iter().all(|s| s.minutes() <= ROLE_DAY_MAX));
        // the least stressed stays first, from the core's end
        let calmest = sh.iter().min_by(|a, b| c.employee(a.id).unwrap().stress.total_cmp(&c.employee(b.id).unwrap().stress)).unwrap();
        assert_eq!(calmest.extra.map(|x| x.0), Some(14 * 60));
        // sixteen hours outside the core, four for each: what is left of them nobody covers
        assert_eq!(uncovered, 960 - 240 * sh.len() as i32);
        assert!(stayed.iter().any(|s| s.night() > 0));
        // a core of twelve hours at most, of an hour at least
        set_hours(&mut c, Role::Cleaner, Hours { from: 18 * 60, to: 10 * 60, outside: 0 }).unwrap();
        assert_eq!(c.staff_rules.hours_of(Role::Cleaner), Hours { from: 18 * 60, to: 6 * 60, outside: 0 });
        set_hours(&mut c, Role::Cleaner, Hours { from: 18 * 60, to: 18 * 60 + 20, outside: 0 }).unwrap();
        assert_eq!(c.staff_rules.hours_of(Role::Cleaner).length(), 60);
        // without the research the hours stay as they are
        let mut d = company();
        assert!(set_hours(&mut d, Role::Mechanic, Hours { from: 0, to: 600, outside: 0 }).is_err());
        assert!(set_hours(&mut c, Role::Driver, Hours { from: 0, to: 600, outside: 0 }).is_err());
        // the night's minutes, also past midnight
        assert_eq!(night_minutes(18 * 60, 22 * 60), 120);
        assert_eq!(night_minutes(22 * 60, 26 * 60), 240);
        assert_eq!(night_minutes(4 * 60, 8 * 60), 120);
        assert_eq!(night_minutes(8 * 60, 16 * 60), 0);
    }

    #[test]
    fn the_roles_are_paid_by_the_hour() {
        let mut c = company();
        hire_n(&mut c, Role::Cleaner, 3);
        let date = c.date.clone();
        let (sh, _) = shifts(&c, &date, Role::Cleaner);
        let month = dates::month_of(&date);
        let before = c.month(&month).get(BookingKind::Wages);
        let mut report = super::super::day::DayReport::default();
        roles_day(&mut c, &date, &mut report);
        let paid = before - c.month(&month).get(BookingKind::Wages);
        // 12:00-20:00, eight hours by day, the employer's share on top
        let want: f64 = sh.iter().map(|s| hourly(c.employee(s.id).unwrap().wage) * 8.0).sum::<f64>() * (1.0 + economy::EMPLOYER_SHARE);
        assert_eq!(paid, want.round() as Cents);
        let last = c.staff_rules.last.clone().unwrap();
        assert_eq!(last.roles.iter().find(|r| r.role == Role::Cleaner).unwrap().at_work as usize, sh.len());
        // the month's end books no monthly wage for them (`paid_monthly`)
        assert!(c.staff.iter().all(|e| !paid_monthly(e)));
        // a short core is paid four hours; the night half as much again
        let s = Shift { id: 1, role: Role::Cleaner, from: 22 * 60, to: 24 * 60, extra: None };
        assert_eq!((s.paid(), s.night()), (MIN_PAID, 120));
    }

    #[test]
    fn stress_rises_with_long_hours_and_falls_on_free_days() {
        let mut e = super::super::plan::agency_driver();
        e.stress = 20.0;
        for _ in 0..10 {
            needs(&mut e, &DayOf { worked: true, minutes: 660, night: 240, short_rest: true, pressure: 1.0, away: false });
        }
        assert!(e.stress > 70.0, "{}", e.stress);
        let friendly = e.friendliness;
        for _ in 0..10 {
            needs(&mut e, &DayOf { worked: true, minutes: 660, night: 240, short_rest: true, pressure: 1.0, away: false });
        }
        assert!(e.friendliness < friendly, "stress makes them less friendly");
        for _ in 0..10 {
            needs(&mut e, &DayOf::default());
        }
        assert!(e.stress < 15.0, "{}", e.stress);
        // the level from experience
        assert_eq!([5.0, 20.0, 50.0, 80.0].map(Grade::of), [Grade::Trainee, Grade::Novice, Grade::Experienced, Grade::Expert]);
    }

    #[test]
    fn mechanics_cleaners_and_inspectors_do_their_work() {
        let mut c = company();
        let bus = super::super::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        c.cash += 5_000_000_00;
        for _ in 0..6 {
            super::super::market::buy_new(&mut c, &bus, super::super::market::Payment::Cash, "").unwrap();
        }
        let date = c.date.clone();
        assert_eq!(workshop_share(&c, &date), 0.0);
        hire_n(&mut c, Role::Mechanic, 3);
        let share = workshop_share(&c, &date);
        assert!(share > 0.0 && share <= WORKSHOP_MAX, "{share}");
        assert!(super::super::training::mechanics(&c, &date) > 0);
        // the cleaners: dirty buses cleaned, the dirtiest first
        for (k, v) in c.fleet.iter_mut().enumerate() {
            v.clean = 50.0 + k as f64;
        }
        let mut quiet = c.clone();
        let mut report = super::super::day::DayReport::default();
        roles_day(&mut quiet, &date, &mut report);
        assert!(quiet.fleet.iter().all(|v| v.clean < 52.0), "nobody cleans: they only get dirtier");
        hire_n(&mut c, Role::Cleaner, 2);
        roles_day(&mut c, &date, &mut report);
        assert!(c.fleet[0].clean > 90.0 && c.staff_rules.last.as_ref().unwrap().cleaned > 0);
        // the inspectors check the busiest line, and its fares rise
        c.lines.push(super::super::model::CompanyLine { name: "Linie5".into(), number: "5".into(), km: 300.0, service_from: Some(0), ..Default::default() });
        c.lines.push(super::super::model::CompanyLine { name: "Linie7".into(), number: "7".into(), km: 100.0, service_from: Some(0), ..Default::default() });
        assert_eq!(fare_gain(&c, &date, "Linie5"), 0.0);
        c.research.done.push(super::super::research::keys::TICKET_INSPECTORS.to_string());
        hire_n(&mut c, Role::Inspector, 1);
        let date = (0..7).map(|k| dates::add(&date, k)).find(|d| c.staff.iter().any(|e| e.role == Role::Inspector && at_work(e, d))).unwrap();
        assert!(fare_gain(&c, &date, "Linie5") > 0.0 && fare_gain(&c, &date, "Linie7") == 0.0);
        assert!(fare_gain(&c, &date, "Linie5") <= INSPECT_GAIN);
    }

    #[test]
    fn raises_are_asked_answered_and_wages_follow_prices() {
        let mut c = company();
        let ids = hire_n(&mut c, Role::Service, 2);
        // underpaid and not happy: one asks, sooner or later
        for e in c.staff.iter_mut() {
            e.wage = 2_000_00;
            e.satisfaction = 40.0;
            e.hired = "2023-01-01".into();
        }
        let mut report = super::super::day::DayReport::default();
        let mut date = c.date.clone();
        for _ in 0..200 {
            roles_day(&mut c, &date, &mut report);
            if !c.staff_rules.asks.is_empty() {
                break;
            }
            date = dates::add(&date, 1);
            c.date = date.clone();
        }
        let ask = c.staff_rules.asks[0].clone();
        assert!(ids.contains(&ask.id) && ask.share >= 0.03 && ask.share <= 0.12);
        assert!(c.inbox.messages.iter().any(|m| m.title == "%{name} asks for a raise"));
        let wage = c.employee(ask.id).unwrap().wage;
        answer_raise(&mut c, ask.id, true).unwrap();
        assert!(c.employee(ask.id).unwrap().wage > wage && c.staff_rules.ask_of(ask.id).is_none());
        assert!(answer_raise(&mut c, ask.id, false).is_err());
        // the month's end: wages rise with the prices
        c.staff_rules.wage_index = 1.0;
        c.price_index = 1.02;
        let w = c.staff[0].wage;
        roles_day(&mut c, "2024-03-31", &mut report);
        assert_eq!(c.staff[0].wage, ((w as f64 * 1.02 / 10_00.0).round() as Cents) * 10_00);
        assert_eq!(c.staff_rules.wage_index, 1.02);
    }

    #[test]
    fn the_automatic_hiring_keeps_the_minimum() {
        let mut c = company();
        c.staff_rules.auto_hire = true;
        c.staff_rules.set_minimum(Role::Cleaner, 2);
        c.staff_rules.set_minimum(Role::Inspector, 1);
        let date = c.date.clone();
        let mut report = super::super::day::DayReport::default();
        roles_day(&mut c, &date, &mut report);
        assert_eq!(c.staff.iter().filter(|e| e.role == Role::Cleaner).count(), 1, "one a night");
        roles_day(&mut c, &date, &mut report);
        assert_eq!(c.staff.iter().filter(|e| e.role == Role::Cleaner).count(), 2);
        roles_day(&mut c, &date, &mut report);
        assert_eq!(c.staff.iter().filter(|e| e.role == Role::Cleaner).count(), 2);
        // (no inspectors without the research)
        assert!(c.staff.iter().all(|e| e.role != Role::Inspector));
        assert!(c.inbox.messages.iter().filter(|m| m.title == "%{name} was hired").count() == 2);
    }
}
