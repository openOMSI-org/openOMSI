//! The contracts the company takes on: goals with a reward, service contracts (the buses'
//! internet, ...), and what they pay or cost day by day.
//!
//! Goals (City Bus Manager's assignments): every week a few are on offer - complete so many
//! trips, carry so many passengers, raise the company's rating, a week without a dropped trip,
//! punctuality, a line on time, days in profit - sized to what the company runs, each with a
//! reward in trophies (`Company::trophies`, research's currency) and money. Taken on, a goal
//! counts the company's real days (`day::DayReport`) until it is reached or its time runs out;
//! up to `RUNNING_MAX` run at once. The trophies the goals earned in all climb a bar of
//! milestones, each paying a bonus.
//!
//! Service contracts: the buses' internet (once WLAN is researched): one of four providers,
//! so many GB a day at a price a day, changes from tomorrow, today's data topped up when it
//! runs low - the passengers' use grows with the day's passengers, and a connection that runs
//! out annoys them (the reputation). A fixed diesel price from a supplier, against the market's
//! events (`inbox::fuel_price_factor`). An outside cleaning company: clean buses please the
//! passengers.

use super::clock::Level;
use super::dates;
use super::day::DayReport;
use super::inbox::{self, Kind, Message};
use super::model::{BookingKind, Cents, Company, DayRecord, Difficulty};
use super::research;
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// How many goals run at once at most, and how many are on offer each week.
pub const RUNNING_MAX: usize = 3;
pub const OFFERED: usize = 4;
/// How many finished goals are kept to look back on.
const FINISHED_KEPT: usize = 30;
/// The trophies the goals earned in all at which a milestone is reached (then every 2,000
/// more), and the bonus it pays: this many euros for each of them.
pub const MILESTONES: [i64; 10] = [50, 100, 175, 250, 350, 500, 750, 1_000, 1_500, 2_000];
const MILESTONE_EUROS: f64 = 40.0;

// --- goals ----------------------------------------------------------------------------------------

/// What a goal asks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Aim {
    /// Trips run (not dropped), the player's own counted too.
    Trips { n: u32 },
    Passengers { n: u32 },
    Km { km: u32 },
    /// The company's rating (its reputation in stars, `rating`) at least this.
    Rating { stars: f64 },
    /// Days in a row without a dropped trip.
    NoDrops { days: u32 },
    /// Days in a row with at least `pct` % of the trips on time.
    OnTime { pct: u32, days: u32 },
    /// Days in a row on which the line `line` (its number) dropped nothing and ran at most one
    /// trip in ten late.
    Line { line: String, days: u32 },
    /// Days closed in profit.
    Profit { days: u32 },
}

/// What a goal's progress counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Count,
    Km,
    Stars,
    Days,
}

impl Aim {
    /// Its title (English, with `%{name}` places) and what goes in them.
    pub fn title(&self) -> (&'static str, Vec<(String, String)>) {
        let a = |k: &str, v: String| (k.to_string(), v);
        match self {
            Aim::Trips { n } => ("Complete %{count} trips successfully", vec![a("count", n.to_string())]),
            Aim::Passengers { n } => ("Carry %{count} passengers", vec![a("count", n.to_string())]),
            Aim::Km { km } => ("Drive %{count} km on the lines", vec![a("count", km.to_string())]),
            Aim::Rating { stars } => ("Improve your company rating to %{stars} stars", vec![a("stars", format!("{stars:.1}"))]),
            Aim::NoDrops { days } => ("No dropped trips for %{days} days in a row", vec![a("days", days.to_string())]),
            Aim::OnTime { pct, days } => ("At least %{pct} % of the trips on time for %{days} days in a row", vec![a("pct", pct.to_string()), a("days", days.to_string())]),
            Aim::Line { line, days } => ("Run line %{n} on time for %{days} days in a row", vec![a("n", line.clone()), a("days", days.to_string())]),
            Aim::Profit { days } => ("Close %{days} days in profit", vec![a("days", days.to_string())]),
        }
    }

    /// What counts, in a sentence (English).
    pub fn rule(&self) -> &'static str {
        match self {
            Aim::Trips { .. } => "Every trip run counts, yours too; a dropped one does not.",
            Aim::Passengers { .. } => "Every passenger on your lines counts.",
            Aim::Km { .. } => "Every kilometre your buses run on the lines counts.",
            Aim::Rating { .. } => "Your rating is your reputation in stars: trips on time and happy passengers raise it.",
            Aim::NoDrops { .. } => "A day with a dropped trip starts the count again.",
            Aim::OnTime { .. } => "A day with fewer trips on time starts the count again.",
            Aim::Line { .. } => "A day on which the line drops a trip or runs more than one in ten late starts the count again.",
            Aim::Profit { .. } => "Every day that ends with a profit counts.",
        }
    }

    /// The figure to reach.
    pub fn target(&self) -> f64 {
        match self {
            Aim::Trips { n } | Aim::Passengers { n } => *n as f64,
            Aim::Km { km } => *km as f64,
            Aim::Rating { stars } => *stars,
            Aim::NoDrops { days } | Aim::OnTime { days, .. } | Aim::Line { days, .. } | Aim::Profit { days } => *days as f64,
        }
    }

    pub fn unit(&self) -> Unit {
        match self {
            Aim::Trips { .. } | Aim::Passengers { .. } => Unit::Count,
            Aim::Km { .. } => Unit::Km,
            Aim::Rating { .. } => Unit::Stars,
            _ => Unit::Days,
        }
    }

    /// Counted in days in a row: a day that misses starts it again.
    pub fn in_a_row(&self) -> bool {
        matches!(self, Aim::NoDrops { .. } | Aim::OnTime { .. } | Aim::Line { .. })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Offered,
    Running,
    Done,
    Failed,
}

/// A goal: on offer until a day, taken on, reached or missed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Goal {
    pub id: u32,
    pub aim: Aim,
    pub trophies: i64,
    pub reward: Cents,
    /// The last day it may be taken on.
    pub offered_until: String,
    /// How many days it runs once taken on.
    pub days: i64,
    #[serde(default)]
    pub state: State,
    #[serde(default)]
    pub taken: Option<String>,
    /// Its last day.
    #[serde(default)]
    pub deadline: Option<String>,
    /// How far it is (of `Aim::target`): a count, the stars, the days (in a row).
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub ended: Option<String>,
    /// The player gave it up.
    #[serde(default)]
    pub given_up: bool,
}

impl Goal {
    /// How much of it is done, 0 to 1.
    pub fn share(&self) -> f64 {
        let t = self.aim.target();
        if t <= 0.0 {
            1.0
        } else {
            (self.progress / t).clamp(0.0, 1.0)
        }
    }

    /// Days left after `today` until its deadline (0: today is its last day).
    pub fn days_left(&self, today: &str) -> Option<i64> {
        self.deadline.as_deref().map(|d| dates::between(today, d))
    }
}

/// The company's rating: its reputation in stars, 0 to 5 with one decimal.
pub fn rating(c: &Company) -> f64 {
    (c.reputation.clamp(0.0, 100.0) / 2.0).round() / 10.0
}

/// The `k`th milestone of the trophies (from 0).
pub fn milestone_at(k: usize) -> i64 {
    match MILESTONES.get(k) {
        Some(m) => *m,
        None => MILESTONES[MILESTONES.len() - 1] + 2_000 * (k + 1 - MILESTONES.len()) as i64,
    }
}

/// The bonus the milestone `at` pays (whole euros).
pub fn milestone_bonus(c: &Company, at: i64) -> Cents {
    euros(at as f64 * MILESTONE_EUROS * 100.0 * c.price_index)
}

/// The milestones of the trophies the goals earned in all: (the last reached, the next).
pub fn milestone(c: &Company) -> (i64, i64) {
    let mut k = 0;
    while c.contracts.earned >= milestone_at(k) {
        k += 1;
    }
    (if k == 0 { 0 } else { milestone_at(k - 1) }, milestone_at(k))
}

// --- service contracts ------------------------------------------------------------------------------

/// An internet provider for the buses' WLAN: what the passengers think of its network, and its
/// prices (cents a GB a day at founding prices, and a GB topped up for the day).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Provider {
    pub name: &'static str,
    pub satisfaction: u32,
    pub per_gb: f64,
    pub top_up: f64,
}

/// The providers (made-up names).
pub const PROVIDERS: [Provider; 4] = [
    Provider { name: "Pennywave", satisfaction: 42, per_gb: 450.0, top_up: 700.0 },
    Provider { name: "Mitralink", satisfaction: 66, per_gb: 650.0, top_up: 950.0 },
    Provider { name: "Corvanet", satisfaction: 85, per_gb: 800.0, top_up: 1_150.0 },
    Provider { name: "Altisky", satisfaction: 98, per_gb: 1_400.0, top_up: 1_800.0 },
];
/// The GB a day a contract may have, and its steps.
pub const GB_MIN: u32 = 2;
pub const GB_MAX: u32 = 250;
pub const GB_STEP: u32 = 2;
/// What a passenger uses of the WLAN on average (GB).
pub const GB_PER_PASSENGER: f64 = 0.012;

/// The buses' internet: from whom, how much a day, since when.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Internet {
    pub provider: usize,
    pub gb: u32,
    pub since: String,
}

/// A change of the internet contract, from tomorrow.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NetChange {
    Take { provider: usize, gb: u32 },
    Cancel,
}

/// The internet's data: what was topped up for a day, and what the passengers used on the last
/// day closed and how much more than there was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Data {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub topped: f64,
    #[serde(default)]
    pub used: f64,
    #[serde(default)]
    pub short: f64,
}

/// A fixed diesel price from a supplier (cents a litre), from a day until a day.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FuelDeal {
    pub price: f64,
    pub from: String,
    pub until: String,
    /// Its end was told ahead.
    #[serde(default)]
    pub warned: bool,
}

/// The terms a supplier fixes the diesel's price for (days), and its margin over the market's
/// price of the day it is signed.
pub const FUEL_TERMS: [(i64, f64); 3] = [(30, 1.03), (90, 1.05), (180, 1.07)];

/// An outside company cleaning the buses every night, from a day (until a day, once ended).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cleaning {
    pub since: String,
    #[serde(default)]
    pub until: Option<String>,
}

/// What the cleaning company asks a bus a day (cents, at founding prices).
pub const CLEANING_PER_BUS: f64 = 1_800.0;

/// The contracts on offer, taken and done.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Contracts {
    /// The goals on offer, running and finished (the last `FINISHED_KEPT`).
    #[serde(default)]
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub next_id: u32,
    /// The Monday (a day number) whose goals are on offer.
    #[serde(default)]
    pub week: i64,
    /// The trophies the goals earned in all (research spends `Company::trophies`; the
    /// milestones count these).
    #[serde(default)]
    pub earned: i64,
    /// How many of the milestones were reached (and paid).
    #[serde(default)]
    pub milestones: u32,
    #[serde(default)]
    pub internet: Option<Internet>,
    #[serde(default)]
    pub internet_next: Option<NetChange>,
    #[serde(default)]
    pub data: Data,
    #[serde(default)]
    pub fuel: Option<FuelDeal>,
    #[serde(default)]
    pub cleaning: Option<Cleaning>,
}

/// Whole euros of `cents`.
fn euros(cents: f64) -> Cents {
    ((cents / 100.0).round() as Cents) * 100
}

/// What `gb` a day of `provider` cost a day now.
pub fn net_price(c: &Company, provider: usize, gb: u32) -> Cents {
    let p = PROVIDERS[provider.min(PROVIDERS.len() - 1)];
    euros(p.per_gb * gb as f64 * c.price_index)
}

/// What `gb` topped up for today cost now.
pub fn top_up_price(c: &Company, provider: usize, gb: u32) -> Cents {
    let p = PROVIDERS[provider.min(PROVIDERS.len() - 1)];
    euros(p.top_up * gb as f64 * c.price_index)
}

/// The internet contract that runs today (a new one runs from tomorrow).
pub fn internet_today(c: &Company) -> Option<&Internet> {
    c.contracts.internet.as_ref().filter(|n| n.since.as_str() <= c.date.as_str())
}

/// Today's data: what the contract gives and what was topped up, and what the passengers used
/// so far as far as yesterday tells (their use through the day).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataToday {
    pub gb: f64,
    pub topped: f64,
    pub used: f64,
}

impl DataToday {
    pub fn left(&self) -> f64 {
        (self.gb + self.topped - self.used).max(0.0)
    }
}

pub fn data_today(c: &Company) -> Option<DataToday> {
    let net = internet_today(c)?;
    let topped = if c.contracts.data.date == c.date { c.contracts.data.topped } else { 0.0 };
    // (the use of the last day closed - else as many passengers as the last day had - spread
    // over the hours the buses run, 5:00 to midnight)
    let expected = if c.contracts.data.used > 0.0 { c.contracts.data.used } else { c.history.last().map(|d| d.passengers as f64 * GB_PER_PASSENGER).unwrap_or(0.0) };
    let through = ((c.clock.minute as f64 - 300.0) / 1_140.0).clamp(0.0, 1.0);
    Some(DataToday { gb: net.gb as f64, topped, used: expected * through })
}

/// Take an internet contract (or change it) from tomorrow: `gb` a day from `provider`.
pub fn take_internet(c: &mut Company, provider: usize, gb: u32) -> Result<(), &'static str> {
    if !c.research.has(research::keys::WLAN) {
        return Err("Research WLAN on the buses first.");
    }
    if provider >= PROVIDERS.len() || !(GB_MIN..=GB_MAX).contains(&gb) {
        return Err("There is no such offer.");
    }
    let same = c.contracts.internet.as_ref().is_some_and(|n| n.provider == provider && n.gb == gb);
    c.contracts.internet_next = if same { None } else { Some(NetChange::Take { provider, gb }) };
    Ok(())
}

/// End the internet contract after today (or let a change waiting for tomorrow go).
pub fn cancel_internet(c: &mut Company) {
    c.contracts.internet_next = c.contracts.internet.is_some().then_some(NetChange::Cancel);
}

/// Top today's data up by `gb`: paid at once. Returns the price.
pub fn top_up(c: &mut Company, gb: u32) -> Result<Cents, &'static str> {
    let Some(net) = internet_today(c).cloned() else { return Err("The buses have no internet contract today.") };
    let price = top_up_price(c, net.provider, gb);
    if c.cash < price {
        return Err("Not enough cash.");
    }
    if c.contracts.data.date != c.date {
        c.contracts.data.date = c.date.clone();
        c.contracts.data.topped = 0.0;
    }
    c.contracts.data.topped += gb as f64;
    c.book(BookingKind::Services, -price, format!("Internet on the buses: {} GB topped up ({})", gb, PROVIDERS[net.provider.min(PROVIDERS.len() - 1)].name), false);
    Ok(price)
}

/// The fixed price a supplier asks for the `term`th of `FUEL_TERMS` (cents a litre), signed
/// today: the market's price of the day - its events too - and the supplier's margin.
pub fn fuel_offer(c: &Company, term: usize) -> (f64, String) {
    let (days, margin) = FUEL_TERMS[term.min(FUEL_TERMS.len() - 1)];
    let market = inbox::usual_diesel(c) * inbox::market_fuel_factor(c, &c.date);
    let from = dates::add(&c.date, 1);
    ((market * margin * 10.0).round() / 10.0, dates::add(&from, days - 1))
}

/// Sign a fixed diesel price for the `term`th of `FUEL_TERMS`, from tomorrow. It cannot be
/// ended early.
pub fn sign_fuel(c: &mut Company, term: usize) -> Result<(), &'static str> {
    if c.contracts.fuel.as_ref().is_some_and(|d| d.until.as_str() >= c.date.as_str()) {
        return Err("A fixed diesel price runs already.");
    }
    let (price, until) = fuel_offer(c, term);
    c.contracts.fuel = Some(FuelDeal { price, from: dates::add(&c.date, 1), until: until.clone(), warned: false });
    let m = Message::new(Kind::Contract, "Fixed diesel price", "From tomorrow until %{until} the company pays %{price} a litre of diesel, whatever the market does.").arg("until", until).arg("price", format!("{price:.1}"));
    inbox::post(c, m);
    Ok(())
}

/// The diesel's price on `date` against the usual, when a fixed price runs then.
pub fn fixed_fuel(c: &Company, date: &str) -> Option<f64> {
    let d = c.contracts.fuel.as_ref().filter(|d| d.from.as_str() <= date && date <= d.until.as_str())?;
    Some(d.price / inbox::usual_diesel(c).max(1.0))
}

/// What the cleaning company asks a day now, for the buses the company has today.
pub fn cleaning_price(c: &Company) -> Cents {
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    euros(CLEANING_PER_BUS * c.price_index) * buses as Cents
}

/// The cleaning company runs on `date`.
pub fn cleaning_on(c: &Company, date: &str) -> bool {
    c.contracts.cleaning.as_ref().is_some_and(|k| k.since.as_str() <= date && k.until.as_deref().is_none_or(|u| date <= u))
}

/// Sign the cleaning company, from tomorrow.
pub fn sign_cleaning(c: &mut Company) -> Result<(), &'static str> {
    if c.contracts.cleaning.as_ref().is_some_and(|k| k.until.is_none()) {
        return Err("The cleaning company works for you already.");
    }
    c.contracts.cleaning = Some(Cleaning { since: dates::add(&c.date, 1), until: None });
    Ok(())
}

/// End the cleaning company's contract after today (one not started yet goes at once).
pub fn cancel_cleaning(c: &mut Company) {
    let today = c.date.clone();
    match c.contracts.cleaning.as_mut() {
        Some(k) if k.since.as_str() > today.as_str() => c.contracts.cleaning = None,
        Some(k) => k.until = Some(today),
        None => {}
    }
}

// --- the goals on offer ---------------------------------------------------------------------------

/// A figure rounded to a number that reads well (5, 10, 50 or 100).
fn nice(x: f64) -> u32 {
    let step = if x < 100.0 {
        5.0
    } else if x < 1_000.0 {
        10.0
    } else if x < 10_000.0 {
        50.0
    } else {
        100.0
    };
    ((x / step).round() * step).max(step) as u32
}

/// The goals of the week of `date`, drawn from the company and the week and sized to what the
/// company ran on its last days (or, a new one, to its lines).
fn draw(c: &Company, date: &str) -> Vec<Goal> {
    let week = dates::week_of(date);
    let mut rng = Rng::of(&[&c.id, "goals"], week);
    let recent: Vec<&DayRecord> = c.history.iter().rev().take(14).collect();
    let avg = |f: &dyn Fn(&DayRecord) -> f64| if recent.is_empty() { None } else { Some(recent.iter().map(|d| f(d)).sum::<f64>() / recent.len() as f64) };
    let tours: f64 = c.lines.iter().map(|l| l.tours as f64).sum();
    let trips = avg(&|d| d.trips.saturating_sub(d.dropped_trips) as f64).unwrap_or(tours * 6.0).max(20.0);
    let pax = avg(&|d| d.passengers as f64).unwrap_or(trips * 15.0).max(150.0);
    let km = avg(&|d| d.km).unwrap_or(trips * 8.0).max(100.0);
    let stars = rating(c);
    let lines: Vec<String> = {
        let now = super::clock::now(c);
        let mut v: Vec<String> = c.lines.iter().filter(|l| super::network::in_service(l, now)).map(|l| l.number.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut kinds: Vec<u8> = vec![0, 1, 2, 4, 5, 7];
    if stars < 4.8 {
        kinds.push(3);
    }
    if !lines.is_empty() {
        kinds.push(6);
    }
    let money = match c.difficulty {
        Difficulty::Easy => 1.25,
        Difficulty::Realistic => 1.0,
        Difficulty::Hard => 0.8,
    };
    let until = dates::fmt(week + 6);
    let mut out = Vec::new();
    while out.len() < OFFERED && !kinds.is_empty() {
        let k = kinds.remove((rng.next_u64() % kinds.len() as u64) as usize);
        let (aim, days, trophies) = match k {
            0 => {
                let d = rng.int(7, 14);
                (Aim::Trips { n: nice(trips * d as f64 * rng.range(0.85, 1.0)) }, d, rng.int(15, 25))
            }
            1 => {
                let d = rng.int(7, 14);
                (Aim::Passengers { n: nice(pax * d as f64 * rng.range(0.85, 1.0)) }, d, rng.int(15, 25))
            }
            2 => {
                let d = rng.int(7, 14);
                (Aim::Km { km: nice(km * d as f64 * rng.range(0.85, 1.0)) }, d, rng.int(10, 20))
            }
            3 => {
                let target = ((stars + rng.range(0.2, 0.5)).min(5.0) * 10.0).round() / 10.0;
                (Aim::Rating { stars: target }, rng.int(14, 28), rng.int(10, 20))
            }
            4 => {
                let d = rng.int(5, 7);
                (Aim::NoDrops { days: d as u32 }, d * 2, rng.int(20, 30))
            }
            5 => {
                let pct = [85u32, 90, 95].into_iter().filter(|p| *p as f64 <= c.punctuality + 5.0).max().unwrap_or(85);
                let d = rng.int(5, 7);
                (Aim::OnTime { pct, days: d as u32 }, d * 2, rng.int(20, 30))
            }
            6 => {
                let line = rng.pick(&lines).cloned().unwrap_or_default();
                let d = rng.int(5, 7);
                (Aim::Line { line, days: d as u32 }, d * 2, rng.int(10, 15))
            }
            _ => {
                let d = rng.int(5, 10);
                (Aim::Profit { days: d as u32 }, d * 2, rng.int(15, 25))
            }
        };
        let reward = ((trophies as f64 * 400_00.0 * rng.range(0.85, 1.2) * c.price_index * money) / 10_00.0).round() as Cents * 10_00;
        out.push(Goal { id: 0, aim, trophies, reward, offered_until: until.clone(), days, state: State::Offered, taken: None, deadline: None, progress: 0.0, ended: None, given_up: false });
    }
    out
}

/// The goals on offer for the week of `date`: a new week brings new ones (those not taken go)
/// and says so in the inbox.
pub fn refresh(c: &mut Company, date: &str) {
    let week = dates::week_of(date);
    if c.contracts.week == week {
        return;
    }
    c.contracts.week = week;
    c.contracts.goals.retain(|g| g.state != State::Offered);
    let offers = draw(c, date);
    let n = offers.len();
    for mut g in offers {
        c.contracts.next_id += 1;
        g.id = c.contracts.next_id;
        c.contracts.goals.push(g);
    }
    // (the easy mode takes them on by itself, at night: `autopilot`)
    let (title, text) = if super::autopilot::on(c) { NEW_GOALS_EASY } else { NEW_GOALS };
    let m = Message::new(Kind::Contract, title, text).dated(date).arg("count", n.to_string()).arg("until", dates::fmt(week + 6));
    inbox::post(c, m);
}

/// A goal's message: its title with `text` under it, and its progress.
fn goal_message(g: &Goal, text: &str, level: Level, date: &str) -> Message {
    let (title, args) = g.aim.title();
    let mut m = Message::new(Kind::Goal, title, text).level(level).dated(date).goal(g.id);
    m.args = args;
    m.arg("trophies", g.trophies.to_string()).arg("reward", g.reward.to_string())
}

/// Take a goal on offer on: it runs from today for its days.
pub fn take(c: &mut Company, id: u32) -> Result<(), &'static str> {
    let today = c.date.clone();
    let stars = rating(c);
    let running = c.contracts.goals.iter().filter(|g| g.state == State::Running).count();
    let Some(g) = c.contracts.goals.iter_mut().find(|g| g.id == id) else { return Err("There is no such goal.") };
    if g.state != State::Offered {
        return Err("This goal is taken already.");
    }
    if g.offered_until.as_str() < today.as_str() {
        return Err("This offer has run out.");
    }
    if running >= RUNNING_MAX {
        return Err("Three goals run already: reach one or give one up first.");
    }
    g.state = State::Running;
    g.taken = Some(today.clone());
    let deadline = dates::add(&today, g.days - 1);
    g.deadline = Some(deadline.clone());
    g.progress = if matches!(g.aim, Aim::Rating { .. }) { stars } else { 0.0 };
    let m = goal_message(g, TAKEN, Level::Info, &today).arg("date", deadline);
    inbox::post(c, m);
    Ok(())
}

/// Give a running goal up: it is missed, without its reward.
pub fn give_up(c: &mut Company, id: u32) -> Result<(), &'static str> {
    let today = c.date.clone();
    let Some(g) = c.contracts.goals.iter_mut().find(|g| g.id == id && g.state == State::Running) else { return Err("There is no such goal.") };
    g.state = State::Failed;
    g.given_up = true;
    g.ended = Some(today.clone());
    let m = goal_message(g, GIVEN_UP, Level::Minor, &today);
    inbox::post(c, m);
    Ok(())
}

/// The day's result so far: what running the lines brought today, less what it cost.
fn day_result(c: &Company, date: &str) -> Cents {
    c.ledger.iter().rev().take_while(|b| b.date == date).filter(|b| !b.kind.is_capital()).map(|b| b.amount).sum()
}

/// The running goals' day: their progress by the day's figures, those reached paid, those whose
/// time ran out missed, and the milestones of the trophies reached.
fn goals_day(c: &mut Company, date: &str, report: &DayReport) {
    let result = day_result(c, date);
    let stars = rating(c);
    let ran = report.trips + report.measured;
    let lines: Vec<String> = c.lines.iter().map(|l| l.number.clone()).collect();
    let mut told: Vec<Message> = Vec::new();
    let mut paid: Vec<(i64, Cents, String)> = Vec::new();
    for g in c.contracts.goals.iter_mut().filter(|g| g.state == State::Running) {
        let row = |p: &mut f64, ok: bool| *p = if ok { *p + 1.0 } else { 0.0 };
        match &g.aim {
            Aim::Trips { .. } => g.progress += (report.trips.saturating_sub(report.dropped) + report.measured) as f64,
            Aim::Passengers { .. } => g.progress += report.passengers as f64,
            Aim::Km { .. } => g.progress += report.km.max(0.0).round(),
            Aim::Rating { .. } => g.progress = stars,
            Aim::NoDrops { .. } => {
                if ran > 0 {
                    row(&mut g.progress, report.dropped == 0);
                }
            }
            Aim::OnTime { pct, .. } => {
                if let Some(p) = report.punctuality {
                    row(&mut g.progress, p + 1e-9 >= *pct as f64);
                }
            }
            Aim::Line { line, .. } => match report.lines.iter().find(|l| l.number.eq_ignore_ascii_case(line)) {
                Some(l) if l.trips > 0 => row(&mut g.progress, l.dropped == 0 && l.late * 10 <= l.trips),
                _ if !lines.iter().any(|n| n.eq_ignore_ascii_case(line)) => g.deadline = Some(date.to_string()),
                _ => {}
            },
            Aim::Profit { .. } => {
                if result >= 0 && ran > 0 {
                    g.progress += 1.0;
                }
            }
        }
        let target = g.aim.target();
        if g.progress + 1e-9 >= target {
            g.state = State::Done;
            g.ended = Some(date.to_string());
            let (title, args) = g.aim.title();
            paid.push((g.trophies, g.reward, inbox::fill(title, &args)));
            told.push(goal_message(g, REACHED, Level::Good, date));
        } else if g.deadline.as_deref().is_some_and(|d| d <= date) {
            g.state = State::Failed;
            g.ended = Some(date.to_string());
            told.push(goal_message(g, MISSED, Level::Warn, date));
        } else if g.days_left(date) == Some(2) {
            told.push(goal_message(g, SOON, Level::Warn, date).key(&format!("goal-{}-soon", g.id)));
        }
    }
    for (trophies, reward, what) in paid {
        c.trophies += trophies;
        c.contracts.earned += trophies;
        c.book(BookingKind::Rewards, reward, format!("Goal reached: {what}"), false);
    }
    for m in told {
        inbox::post(c, m);
    }
    // the milestones the trophies reached, each with its bonus
    while c.contracts.earned >= milestone_at(c.contracts.milestones as usize) {
        let at = milestone_at(c.contracts.milestones as usize);
        c.contracts.milestones += 1;
        let bonus = milestone_bonus(c, at);
        c.book(BookingKind::Rewards, bonus, format!("Trophy milestone {at}"), false);
        let m = Message::new(Kind::Goal, MILESTONE.0, MILESTONE.1).level(Level::Good).dated(date).arg("count", at.to_string()).arg("reward", bonus.to_string());
        inbox::post(c, m);
    }
    // (the finished ones kept to look back on)
    let finished: Vec<u32> = c.contracts.goals.iter().filter(|g| matches!(g.state, State::Done | State::Failed)).map(|g| g.id).collect();
    if finished.len() > FINISHED_KEPT {
        let gone = &finished[..finished.len() - FINISHED_KEPT];
        c.contracts.goals.retain(|g| !gone.contains(&g.id));
    }
}

// --- the day ------------------------------------------------------------------------------------

/// The reputation moved by `change` (the day's report with it).
fn move_reputation(c: &mut Company, report: &mut DayReport, change: f64) {
    let before = c.reputation;
    c.reputation = (c.reputation + change).clamp(0.0, 100.0);
    report.reputation = c.reputation;
    report.reputation_change += c.reputation - before;
}

/// The service contracts' day: what they cost, the passengers' internet and whether its data
/// lasted, the clean buses, a fixed diesel price ending.
fn services_day(c: &mut Company, date: &str, report: &mut DayReport) {
    // the internet
    if let Some(net) = c.contracts.internet.clone().filter(|n| n.since.as_str() <= date) {
        let p = PROVIDERS[net.provider.min(PROVIDERS.len() - 1)];
        let cost = net_price(c, net.provider, net.gb);
        c.book(BookingKind::Services, -cost, format!("Internet on the buses: {} ({} GB a day)", p.name, net.gb), false);
        let topped = if c.contracts.data.date == date { c.contracts.data.topped } else { 0.0 };
        let used = report.passengers as f64 * GB_PER_PASSENGER;
        let short = (used - net.gb as f64 - topped).max(0.0);
        let missed = if used > 0.0 { short / used } else { 0.0 };
        // (a good network that lasts the day pleases the passengers; one that runs out annoys
        // them)
        if report.passengers > 0 {
            let change = 0.25 * (p.satisfaction as f64 / 100.0 - 0.35) * (1.0 - missed) - 0.6 * missed;
            move_reputation(c, report, change);
        }
        c.contracts.data = Data { date: date.to_string(), topped, used, short };
        if short >= 0.5 {
            let m = Message::new(Kind::Contract, NET_SHORT.0, NET_SHORT.1).level(Level::Warn).dated(date).key("internet-short").arg("used", format!("{used:.0}")).arg("short", format!("{short:.0}")).arg("gb", net.gb.to_string()).arg("name", p.name);
            inbox::post(c, m);
        }
    }
    // the cleaning company
    if cleaning_on(c, date) {
        let price = cleaning_price(c);
        let buses = c.fleet.iter().filter(|v| v.held_on(date)).count();
        c.book(BookingKind::Services, -price, format!("Cleaning company ({buses} buses)"), false);
        if report.tours > 0 {
            move_reputation(c, report, 0.06);
        }
        if c.contracts.cleaning.as_ref().and_then(|k| k.until.as_deref()) == Some(date) {
            c.contracts.cleaning = None;
            inbox::post(c, Message::new(Kind::Contract, CLEANING_ENDED.0, CLEANING_ENDED.1).level(Level::Minor).dated(&dates::add(date, 1)));
        }
    }
    // the fixed diesel price: its end told ahead, and when it came
    if let Some(d) = c.contracts.fuel.clone() {
        if d.until.as_str() <= date {
            c.contracts.fuel = None;
            inbox::post(c, Message::new(Kind::Contract, FUEL_ENDED.0, FUEL_ENDED.1).level(Level::Info).dated(&dates::add(date, 1)));
        } else if !d.warned && dates::between(date, &d.until) <= 3 {
            if let Some(x) = c.contracts.fuel.as_mut() {
                x.warned = true;
            }
            let m = Message::new(Kind::Contract, FUEL_ENDING.0, FUEL_ENDING.1).level(Level::Info).dated(date).arg("until", d.until.clone()).arg("price", format!("{:.1}", d.price));
            inbox::post(c, m);
        }
    }
}

/// The internet contract's change waiting for tomorrow made, and told (dated tomorrow).
fn internet_tomorrow(c: &mut Company, tomorrow: &str) {
    match c.contracts.internet_next.take() {
        Some(NetChange::Take { provider, gb }) => {
            c.contracts.internet = Some(Internet { provider, gb, since: tomorrow.to_string() });
            let price = net_price(c, provider, gb);
            let m = Message::new(Kind::Contract, NET_TAKEN.0, NET_TAKEN.1).dated(tomorrow).arg("name", PROVIDERS[provider.min(PROVIDERS.len() - 1)].name).arg("gb", gb.to_string()).arg("amount", price.to_string());
            inbox::post(c, m);
        }
        Some(NetChange::Cancel) => {
            c.contracts.internet = None;
            inbox::post(c, Message::new(Kind::Contract, NET_ENDED.0, NET_ENDED.1).level(Level::Minor).dated(tomorrow));
        }
        None => {}
    }
}

/// The contracts' day (`day::close_day`): what the service contracts cost and do, the goals'
/// progress, rewards and ends, tomorrow's changes, and a new week's goals.
pub fn day(c: &mut Company, date: &str, report: &mut DayReport) {
    refresh(c, date);
    services_day(c, date, report);
    goals_day(c, date, report);
    let tomorrow = dates::add(date, 1);
    internet_tomorrow(c, &tomorrow);
    refresh(c, &tomorrow);
}

const NEW_GOALS: (&str, &str) = ("New goals on offer", "%{count} new goals are on offer until %{until}, each with trophies and money as its reward. Take on up to three at once on the Contracts page.");
const NEW_GOALS_EASY: (&str, &str) = ("New goals on offer", "%{count} new goals are on offer until %{until}, each with trophies and money as its reward: the company takes them on by itself, up to three at once.");
const TAKEN: &str = "Goal taken on: reach it by %{date} for %{trophies} trophies and %{reward}.";
const GIVEN_UP: &str = "You gave this goal up.";
const REACHED: &str = "Goal reached: you earned %{trophies} trophies and %{reward}.";
const MISSED: &str = "Goal missed: its time ran out.";
const SOON: &str = "Two days are left to reach this goal.";
const MILESTONE: (&str, &str) = ("Trophy milestone reached", "Your goals have earned %{count} trophies in all: a bonus of %{reward}.");
const NET_SHORT: (&str, &str) = ("The buses' internet ran out", "Your passengers used %{used} GB, %{short} GB more than the %{gb} GB a day of the contract with %{name}: they were annoyed. Take more data a day, or top it up when it runs low.");
const NET_TAKEN: (&str, &str) = ("Internet contract", "From today the buses' WLAN runs on %{name}'s network: %{gb} GB a day for %{amount} a day.");
const NET_ENDED: (&str, &str) = ("Internet contract ended", "From today the buses have no internet contract: the WLAN is off.");
const CLEANING_ENDED: (&str, &str) = ("Cleaning contract ended", "The cleaning company no longer cleans your buses.");
const FUEL_ENDING: (&str, &str) = ("Fixed diesel price ends soon", "Your fixed price of %{price} a litre ends on %{until}; after it the company pays the market's price again.");
const FUEL_ENDED: (&str, &str) = ("Fixed diesel price ended", "The supplier's fixed price has ended: the company pays the market's price for diesel again.");

/// Every English text the contracts write (titles, texts, refusals), for the translations'
/// check.
pub fn texts() -> Vec<&'static str> {
    let aims = [
        Aim::Trips { n: 1 },
        Aim::Passengers { n: 1 },
        Aim::Km { km: 1 },
        Aim::Rating { stars: 1.0 },
        Aim::NoDrops { days: 1 },
        Aim::OnTime { pct: 1, days: 1 },
        Aim::Line { line: String::new(), days: 1 },
        Aim::Profit { days: 1 },
    ];
    let mut out: Vec<&'static str> = aims.iter().flat_map(|a| [a.title().0, a.rule()]).collect();
    out.extend([TAKEN, GIVEN_UP, REACHED, MISSED, SOON, "Fixed diesel price", "From tomorrow until %{until} the company pays %{price} a litre of diesel, whatever the market does."]);
    for (a, b) in [NEW_GOALS, NEW_GOALS_EASY, MILESTONE, NET_SHORT, NET_TAKEN, NET_ENDED, CLEANING_ENDED, FUEL_ENDING, FUEL_ENDED] {
        out.extend([a, b]);
    }
    out.extend([
        "Research WLAN on the buses first.",
        "There is no such offer.",
        "The buses have no internet contract today.",
        "A fixed diesel price runs already.",
        "The cleaning company works for you already.",
        "There is no such goal.",
        "This goal is taken already.",
        "This offer has run out.",
        "Three goals run already: reach one or give one up first.",
    ]);
    out
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::company::day::LineDay;
    use crate::company::model::{CompanyLine, DayRecord};

    fn company() -> Company {
        let mut c = found(&Founding { name: "Ziel".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), added: c.date.clone(), service_from: Some(0), tours: 4, ..Default::default() });
        c
    }

    /// A closed day's figures: trips (and dropped, late), passengers, punctuality.
    fn report(date: &str, trips: u32, dropped: u32, late: u32, passengers: u32) -> DayReport {
        let run = trips - dropped;
        DayReport {
            date: date.into(),
            tours: 4,
            covered: 4,
            trips,
            dropped,
            late,
            passengers,
            km: run as f64 * 8.0,
            punctuality: (run > 0).then(|| (run - late) as f64 / run as f64 * 100.0),
            lines: vec![LineDay { line: "Linie5".into(), number: "5".into(), trips, dropped, late, passengers, ..Default::default() }],
            ..Default::default()
        }
    }

    /// The contracts' day closed for the company's day, and the date moved on.
    fn close(c: &mut Company, r: &mut DayReport) {
        let date = c.date.clone();
        day(c, &date, r);
        c.date = dates::add(&date, 1);
    }

    #[test]
    fn a_week_brings_its_goals_the_same_for_the_same_company() {
        let mut a = company();
        let mut b = company();
        refresh(&mut a, "2024-03-04");
        refresh(&mut b, "2024-03-06");
        let offered = |c: &Company| c.contracts.goals.iter().filter(|g| g.state == State::Offered).cloned().collect::<Vec<_>>();
        assert_eq!(offered(&a).len(), OFFERED);
        assert_eq!(offered(&a), offered(&b));
        assert!(offered(&a).iter().all(|g| g.offered_until == "2024-03-10" && g.trophies >= 10 && g.reward > 0 && g.aim.target() > 0.0));
        // the same week again: nothing new; told once
        refresh(&mut a, "2024-03-07");
        assert_eq!(offered(&a).len(), OFFERED);
        assert_eq!(a.inbox.messages.iter().filter(|m| m.title == NEW_GOALS.0).count(), 1);
        // the next week: others, the old offers gone
        refresh(&mut a, "2024-03-11");
        assert!(offered(&a).iter().all(|g| g.offered_until == "2024-03-17"));
        assert_eq!(offered(&a).len(), OFFERED);
        // the easy mode is told the company takes them on by itself
        let text = |c: &Company| c.inbox.messages.iter().rev().find(|m| m.title == NEW_GOALS.0).map(|m| m.text.clone());
        assert_eq!(text(&a).as_deref(), Some(NEW_GOALS.1));
        let mut e = company();
        e.mode = super::super::Mode::Easy;
        refresh(&mut e, "2024-03-04");
        assert_eq!(text(&e).as_deref(), Some(NEW_GOALS_EASY.1));
    }

    #[test]
    fn goals_are_sized_to_what_the_company_runs() {
        let mut small = company();
        let mut big = company();
        for k in 0..14 {
            let date = dates::add("2024-02-19", k);
            small.history.push(DayRecord { date: date.clone(), trips: 30, passengers: 400, km: 250.0, ..Default::default() });
            big.history.push(DayRecord { date, trips: 600, passengers: 9_000, km: 5_000.0, ..Default::default() });
        }
        refresh(&mut small, "2024-03-04");
        refresh(&mut big, "2024-03-04");
        let per_day = |c: &Company| c.contracts.goals.iter().find_map(|g| match g.aim {
            Aim::Trips { n } => Some(n as f64 / g.days as f64),
            Aim::Passengers { n } => Some(n as f64 / 15.0 / g.days as f64),
            Aim::Km { km } => Some(km as f64 / 8.0 / g.days as f64),
            _ => None,
        });
        // (the same draws: the same kinds of goal, at sizes of their own)
        if let (Some(s), Some(b)) = (per_day(&small), per_day(&big)) {
            assert!(b > s * 10.0, "{s} {b}");
        }
        assert_eq!(nice(1_372.0), 1_350);
        assert_eq!(nice(87.0), 85);
        assert_eq!(nice(1.0), 5);
    }

    #[test]
    fn a_goal_taken_on_counts_the_days_and_pays_its_reward() {
        let mut c = company();
        let today = c.date.clone();
        refresh(&mut c, &today);
        c.contracts.goals.retain(|g| g.state != State::Offered);
        let push = |c: &mut Company, aim: Aim, days: i64| {
            c.contracts.next_id += 1;
            let id = c.contracts.next_id;
            c.contracts.goals.push(Goal { id, aim, trophies: 20, reward: 8_000_00, offered_until: "2024-03-10".into(), days, state: State::Offered, taken: None, deadline: None, progress: 0.0, ended: None, given_up: false });
            id
        };
        let trips = push(&mut c, Aim::Trips { n: 100 }, 7);
        let row = push(&mut c, Aim::NoDrops { days: 3 }, 6);
        let line = push(&mut c, Aim::Line { line: "5".into(), days: 2 }, 4);
        let fourth = push(&mut c, Aim::Profit { days: 2 }, 4);
        for id in [trips, row, line] {
            take(&mut c, id).unwrap();
        }
        assert_eq!(take(&mut c, fourth), Err("Three goals run already: reach one or give one up first."));
        assert_eq!(take(&mut c, trips), Err("This goal is taken already."));
        let cash = c.cash;
        // day 1: 40 trips, none dropped; day 2: one dropped (the row starts again); then clean days
        close(&mut c, &mut report("2024-03-04", 40, 0, 2, 600));
        close(&mut c, &mut report("2024-03-05", 40, 1, 2, 600));
        let g = |c: &Company, id: u32| c.contracts.goals.iter().find(|g| g.id == id).cloned().unwrap();
        assert_eq!((g(&c, trips).progress, g(&c, row).progress, g(&c, line).progress), (79.0, 0.0, 0.0));
        close(&mut c, &mut report("2024-03-06", 40, 0, 2, 600));
        // the trips are reached: its trophies and money, and the inbox says so
        assert_eq!(g(&c, trips).state, State::Done);
        assert_eq!((c.trophies, c.contracts.earned), (20, 20));
        assert_eq!(c.cash - cash, 8_000_00);
        assert!(c.inbox.messages.iter().any(|m| m.goal == Some(trips) && m.text == REACHED && m.level == Level::Good));
        assert_eq!(g(&c, line).state, State::Running);
        close(&mut c, &mut report("2024-03-07", 40, 0, 2, 600));
        assert_eq!(g(&c, line).state, State::Done);
        // the row of three days: reached on the fifth day; with 40 trophies the first milestone is not
        close(&mut c, &mut report("2024-03-08", 40, 0, 2, 600));
        assert_eq!(g(&c, row).state, State::Done);
        assert_eq!(c.contracts.milestones, 1, "50 trophies reached");
        assert!(c.inbox.messages.iter().any(|m| m.title == MILESTONE.0));
        assert_eq!(milestone(&c), (50, 100));
    }

    #[test]
    fn a_goal_whose_time_runs_out_is_missed() {
        let mut c = company();
        let today = c.date.clone();
        refresh(&mut c, &today);
        let id = c.contracts.goals[0].id;
        c.contracts.goals[0].aim = Aim::Passengers { n: 1_000_000 };
        c.contracts.goals[0].days = 2;
        take(&mut c, id).unwrap();
        close(&mut c, &mut report("2024-03-04", 40, 0, 0, 600));
        close(&mut c, &mut report("2024-03-05", 40, 0, 0, 600));
        let g = c.contracts.goals.iter().find(|g| g.id == id).unwrap();
        assert_eq!((g.state, g.progress), (State::Failed, 1_200.0));
        assert!(c.inbox.messages.iter().any(|m| m.goal == Some(id) && m.text == MISSED));
        assert_eq!(c.trophies, 0);
        // a goal given up
        let other = c.contracts.goals.iter().find(|g| g.state == State::Offered).map(|g| g.id).unwrap();
        take(&mut c, other).unwrap();
        give_up(&mut c, other).unwrap();
        assert!(c.contracts.goals.iter().any(|g| g.id == other && g.given_up && g.state == State::Failed));
    }

    #[test]
    fn the_internet_costs_by_the_day_and_annoys_when_it_runs_out() {
        let mut c = company();
        assert_eq!(take_internet(&mut c, 2, 40), Err("Research WLAN on the buses first."));
        c.research.done.push(research::keys::WLAN.into());
        take_internet(&mut c, 2, 40).unwrap();
        // from tomorrow: today nothing is booked
        assert!(internet_today(&c).is_none() && top_up(&mut c, 10).is_err());
        let services = |c: &Company| c.month("2024-03").get(BookingKind::Services);
        close(&mut c, &mut report("2024-03-04", 40, 0, 0, 3_000));
        assert_eq!(services(&c), 0);
        assert_eq!(internet_today(&c).map(|n| n.gb), Some(40));
        assert!(c.inbox.messages.iter().any(|m| m.title == NET_TAKEN.0 && m.date == "2024-03-05"));
        // 3,000 passengers use 36 GB: enough; a good network pleases them
        let rep = c.reputation;
        let mut r = report("2024-03-05", 40, 0, 0, 3_000);
        close(&mut c, &mut r);
        assert_eq!(services(&c), -net_price(&c, 2, 40));
        assert!(c.reputation > rep && r.reputation_change > 0.0);
        // 6,000 use 72 GB: short, they are annoyed - unless it was topped up
        let rep = c.reputation;
        let mut busy = c.clone();
        close(&mut busy, &mut report("2024-03-06", 40, 0, 0, 6_000));
        assert!(busy.reputation < rep);
        assert!(busy.contracts.data.short > 30.0);
        assert!(busy.inbox.messages.iter().any(|m| m.title == NET_SHORT.0));
        let price = top_up(&mut c, 40).unwrap();
        assert_eq!(price, top_up_price(&c, 2, 40));
        close(&mut c, &mut report("2024-03-06", 40, 0, 0, 6_000));
        assert!(c.reputation > rep && c.contracts.data.short == 0.0);
        // cancelled: from tomorrow none
        cancel_internet(&mut c);
        close(&mut c, &mut report("2024-03-07", 40, 0, 0, 3_000));
        assert!(c.contracts.internet.is_none());
    }

    #[test]
    fn a_fixed_diesel_price_holds_against_the_market() {
        let mut c = company();
        let usual = inbox::usual_diesel(&c);
        // a shortage is coming: the price fixed today is the market's of today, and a margin
        c.inbox.events.push(inbox::Event { id: 1, key: "fuel_shortage".into(), from: "2024-03-07".into(), until: "2024-03-10".into(), fuel: 1.4, ..Default::default() });
        let (price, until) = fuel_offer(&c, 0);
        assert!((price - usual * 1.03).abs() < 0.1, "{price}");
        assert_eq!(until, "2024-04-03");
        sign_fuel(&mut c, 0).unwrap();
        assert_eq!(sign_fuel(&mut c, 1), Err("A fixed diesel price runs already."));
        assert_eq!(inbox::fuel_price_factor(&c, "2024-03-04"), 1.0);
        assert!((inbox::fuel_price_factor(&c, "2024-03-08") - 1.03).abs() < 0.01);
        assert!((inbox::market_fuel_factor(&c, "2024-03-08") - 1.4).abs() < 1e-9);
        // its end told ahead and when it came
        c.date = "2024-03-31".into();
        close(&mut c, &mut DayReport::default());
        assert!(c.inbox.messages.iter().any(|m| m.title == FUEL_ENDING.0));
        c.date = "2024-04-03".into();
        close(&mut c, &mut DayReport::default());
        assert!(c.contracts.fuel.is_none() && c.inbox.messages.iter().any(|m| m.title == FUEL_ENDED.0));
    }

    #[test]
    fn the_cleaning_company_is_paid_by_the_bus() {
        let mut c = company();
        let bus = super::super::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        c.cash += 1_000_000_00;
        super::super::market::buy_new(&mut c, &bus, super::super::market::Payment::Cash, "").unwrap();
        super::super::market::buy_new(&mut c, &bus, super::super::market::Payment::Cash, "").unwrap();
        sign_cleaning(&mut c).unwrap();
        assert!(sign_cleaning(&mut c).is_err());
        close(&mut c, &mut report("2024-03-04", 40, 0, 0, 600));
        assert_eq!(c.month("2024-03").get(BookingKind::Services), 0);
        let rep = c.reputation;
        close(&mut c, &mut report("2024-03-05", 40, 0, 0, 600));
        assert_eq!(c.month("2024-03").get(BookingKind::Services), -2 * 18_00);
        assert!(c.reputation > rep);
        cancel_cleaning(&mut c);
        close(&mut c, &mut report("2024-03-06", 40, 0, 0, 600));
        assert!(c.contracts.cleaning.is_none());
        close(&mut c, &mut report("2024-03-07", 40, 0, 0, 600));
        assert_eq!(c.month("2024-03").get(BookingKind::Services), -4 * 18_00);
    }

    #[test]
    fn an_older_file_loads_its_contracts() {
        // (the first round wrote an empty object)
        let k: Contracts = serde_json::from_str("{}").unwrap();
        assert_eq!(k, Contracts::default());
        let g: Goal = serde_json::from_str(r#"{"id":1,"aim":{"kind":"trips","n":50},"trophies":10,"reward":100,"offered_until":"2024-03-10","days":7}"#).unwrap();
        assert_eq!((g.state, g.progress), (State::Offered, 0.0));
    }
}
