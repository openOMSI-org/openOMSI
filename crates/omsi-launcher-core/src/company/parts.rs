//! The workshop's stores: the spare parts the company keeps (engines, lights, brake pads,
//! tyres, ...), what repairs take from them, and what it orders.
//!
//! As City Bus Manager's spare parts shop: ten parts, each with its price (less with the
//! research into cheaper parts, `research::parts_factor`) and its stock. What is ordered comes
//! at 16:00 for free - that day when ordered before, else the next - or at once for a fee;
//! the clock brings it at its hour (`on_minute`), a day closed without the clock at its close
//! (`day`). Every delivery is told in the inbox.
//!
//! A bus that breaks down needs a part (the bigger the damage the bigger the part: a light, a
//! V-belt, a tyre set, the engine), a service its oil and now and then brake pads, a V-belt or
//! tyres. What the stores have is fitted - and what it was worth comes off the repair's or the
//! maintenance's cost, the stores having bought it already; what they lack keeps the bus in
//! the workshop until it comes (told in the inbox). The company's own mechanics
//! (`Role::Mechanic` at work) do the repairs in their hours - an outside garage takes the days
//! it takes - and the research into repairs makes them quicker. The stores order what runs
//! short by themselves (for the free delivery) unless the player switches that off; an older
//! company finds a first stock in them.

use super::clock::{self, FeedItem, Level};
use super::dates;
use super::day::{DayReport, Note};
use super::depot::JobKind;
use super::inbox::{self, Kind, Message};
use super::market;
use super::model::{BookingKind, BusSize, Cents, Company, Difficulty, Employee, Role, Vehicle};
use super::research;
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// A spare part.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Engine,
    Light,
    Indicator,
    Wiper,
    AirConditioner,
    Router,
    BrakePads,
    TyreSet,
    OilCanister,
    VBelt,
}

impl Part {
    pub const ALL: [Part; 10] = [Part::Engine, Part::Light, Part::Indicator, Part::Wiper, Part::AirConditioner, Part::Router, Part::BrakePads, Part::TyreSet, Part::OilCanister, Part::VBelt];

    /// Its name (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Part::Engine => "Engine",
            Part::Light => "Headlight",
            Part::Indicator => "Indicator",
            Part::Wiper => "Wiper",
            Part::AirConditioner => "Air conditioner",
            Part::Router => "Router",
            Part::BrakePads => "Brake pads",
            Part::TyreSet => "Tyre set",
            Part::OilCanister => "Oil canister",
            Part::VBelt => "V-belt",
        }
    }

    /// The launcher's icon for it.
    pub fn icon(self) -> &'static str {
        match self {
            Part::Engine => "settings",
            Part::Light => "flashlight_on",
            Part::Indicator => "highlight",
            Part::Wiper => "rainy",
            Part::AirConditioner => "ac_unit",
            Part::Router => "wifi",
            Part::BrakePads => "stop_circle",
            Part::TyreSet => "autorenew",
            Part::OilCanister => "water_drop",
            Part::VBelt => "sync_alt",
        }
    }

    /// What it costs at the garage (net, Realistic, at founding).
    pub fn list(self) -> Cents {
        match self {
            Part::Engine => 3_500_00,
            Part::Light => 150_00,
            Part::Indicator => 40_00,
            Part::Wiper => 45_00,
            Part::AirConditioner => 1_400_00,
            Part::Router => 300_00,
            Part::BrakePads => 180_00,
            Part::TyreSet => 1_200_00,
            Part::OilCanister => 110_00,
            Part::VBelt => 70_00,
        }
    }

    /// The workshop hours fitting it takes.
    pub fn hours(self) -> f64 {
        match self {
            Part::Engine => 20.0,
            Part::AirConditioner => 6.0,
            Part::BrakePads => 3.0,
            Part::Router | Part::TyreSet | Part::VBelt => 2.0,
            Part::Light | Part::Indicator | Part::Wiper | Part::OilCanister => 1.0,
        }
    }

    /// What the stores keep of it for every twelve buses.
    pub fn keep(self) -> u32 {
        match self {
            Part::Engine | Part::AirConditioner | Part::Router => 1,
            Part::TyreSet | Part::VBelt => 2,
            Part::Light | Part::Indicator | Part::Wiper | Part::BrakePads => 3,
            Part::OilCanister => 4,
        }
    }

    /// A bus waiting for it, as the inbox says (`%{bus}`: its number).
    pub fn missing_text(self) -> &'static str {
        match self {
            Part::Engine => "Bus %{bus} waits in the workshop for an engine: there is none in the stores.",
            Part::Light => "Bus %{bus} waits in the workshop for a headlight: there is none in the stores.",
            Part::Indicator => "Bus %{bus} waits in the workshop for an indicator: there is none in the stores.",
            Part::Wiper => "Bus %{bus} waits in the workshop for a wiper: there is none in the stores.",
            Part::AirConditioner => "Bus %{bus} waits in the workshop for an air conditioner: there is none in the stores.",
            Part::Router => "Bus %{bus} waits in the workshop for a router: there is none in the stores.",
            Part::BrakePads => "Bus %{bus} waits in the workshop for brake pads: there are none in the stores.",
            Part::TyreSet => "Bus %{bus} waits in the workshop for a tyre set: there is none in the stores.",
            Part::OilCanister => "Bus %{bus} waits in the workshop for an oil canister: there is none in the stores.",
            Part::VBelt => "Bus %{bus} waits in the workshop for a V-belt: there is none in the stores.",
        }
    }
}

/// An order on its way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Order {
    pub id: u32,
    pub parts: Vec<(Part, u32)>,
    /// The company's day it was ordered, and when it comes (the clock's minutes,
    /// `clock::moment`).
    pub ordered: String,
    pub due: i64,
    pub cost: Cents,
    /// Ordered by the stores themselves (what ran short).
    #[serde(default)]
    pub auto: bool,
}

impl Order {
    /// How many parts it brings.
    pub fn count(&self) -> u32 {
        self.parts.iter().map(|x| x.1).sum()
    }
}

/// A bus in the workshop that waits for parts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Waiting {
    pub vehicle: u32,
    pub number: String,
    /// The parts still missing.
    pub parts: Vec<Part>,
    /// The day it began to wait.
    pub since: String,
    /// The workshop days the work takes once they are there.
    pub days: i64,
    /// A breakdown's repair (else a service).
    #[serde(default)]
    pub repair: bool,
    /// What the parts fitted may still take off the repair's cost.
    #[serde(default)]
    pub credit: Cents,
}

/// The spare parts in store and on order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stores {
    /// What is in store, by part.
    #[serde(default)]
    pub stock: Vec<(Part, u32)>,
    #[serde(default)]
    pub orders: Vec<Order>,
    #[serde(default)]
    pub waiting: Vec<Waiting>,
    /// The stores order what runs short by themselves (for the free delivery).
    #[serde(default = "yes")]
    pub auto: bool,
    /// The first stock was put in (`open`).
    #[serde(default)]
    pub opened: bool,
    #[serde(default)]
    pub next_order: u32,
}

fn yes() -> bool {
    true
}

impl Default for Stores {
    fn default() -> Self {
        Stores { stock: Vec::new(), orders: Vec::new(), waiting: Vec::new(), auto: true, opened: false, next_order: 0 }
    }
}

impl Stores {
    /// A new company's stores: the first stock of a small fleet (`open`).
    pub fn first() -> Stores {
        Stores { stock: Part::ALL.iter().map(|p| (*p, p.keep())).collect(), opened: true, ..Default::default() }
    }

    /// How many of a part are in store.
    pub fn count(&self, p: Part) -> u32 {
        self.stock.iter().find(|x| x.0 == p).map(|x| x.1).unwrap_or(0)
    }

    pub fn add(&mut self, p: Part, n: u32) {
        match self.stock.iter_mut().find(|x| x.0 == p) {
            Some(x) => x.1 += n,
            None => self.stock.push((p, n)),
        }
    }

    /// Take one out of store (false: there is none).
    pub fn take(&mut self, p: Part) -> bool {
        match self.stock.iter_mut().find(|x| x.0 == p && x.1 > 0) {
            Some(x) => {
                x.1 -= 1;
                true
            }
            None => false,
        }
    }

    /// How many of a part are on their way.
    pub fn on_order(&self, p: Part) -> u32 {
        self.orders.iter().flat_map(|o| o.parts.iter()).filter(|x| x.0 == p).map(|x| x.1).sum()
    }

    /// How many of a part the buses waiting need.
    pub fn needed(&self, p: Part) -> u32 {
        self.waiting.iter().flat_map(|w| w.parts.iter()).filter(|x| **x == p).count() as u32
    }

    /// The bus `vehicle` waits for parts.
    pub fn waits(&self, vehicle: u32) -> Option<&Waiting> {
        self.waiting.iter().find(|w| w.vehicle == vehicle)
    }
}

// --- prices and deliveries -------------------------------------------------------------------------

/// When the free delivery comes: 16:00.
pub const DELIVERY: i64 = 16 * 60;
/// What the immediate delivery costs (Realistic, at founding).
pub const EXPRESS: Cents = 250_00;

/// Parts' prices against Realistic.
fn price_factor(d: Difficulty) -> f64 {
    match d {
        Difficulty::Easy => 0.85,
        Difficulty::Realistic => 1.0,
        Difficulty::Hard => 1.1,
    }
}

/// What a part costs at the garage now (its list price).
pub fn list_price(c: &Company, p: Part) -> Cents {
    (p.list() as f64 * price_factor(c.difficulty) * c.price_index.max(0.0)).round() as Cents
}

/// What a part costs the stores now: the list price less the research's discount.
pub fn price(c: &Company, p: Part) -> Cents {
    (list_price(c, p) as f64 * research::parts_factor(c)).round() as Cents
}

/// What the immediate delivery costs now.
pub fn express_fee(c: &Company) -> Cents {
    (EXPRESS as f64 * price_factor(c.difficulty) * c.price_index.max(0.0)).round() as Cents
}

/// When what is ordered now comes for free: at 16:00 today when ordered before it, else
/// tomorrow at 16:00.
pub fn free_delivery(c: &Company) -> i64 {
    let day = if (c.clock.minute as i64) < DELIVERY { c.date.clone() } else { dates::add(&c.date, 1) };
    clock::moment(&day, DELIVERY)
}

/// What an order of `parts` costs now (and the fee, for the immediate delivery).
pub fn order_cost(c: &Company, parts: &[(Part, u32)], express: bool) -> Cents {
    parts.iter().map(|(p, n)| price(c, *p) * *n as Cents).sum::<Cents>() + if express { express_fee(c) } else { 0 }
}

/// Order parts: paid now (booked as spare parts), in store at 16:00 (`free_delivery`) or, for
/// the fee, at once. Returns what it cost.
pub fn order(c: &mut Company, parts: &[(Part, u32)], express: bool) -> Result<Cents, &'static str> {
    open(c);
    let parts: Vec<(Part, u32)> = parts.iter().copied().filter(|x| x.1 > 0).collect();
    if parts.is_empty() {
        return Err("Choose the parts to order first.");
    }
    let cost = order_cost(c, &parts, express);
    if c.cash < cost {
        return Err("Not enough cash.");
    }
    let n: u32 = parts.iter().map(|x| x.1).sum();
    let text = if express { format!("Spare parts ({n}), delivered at once") } else { format!("Spare parts ({n})") };
    c.book(BookingKind::Parts, -cost, text, false);
    if express {
        for (p, k) in &parts {
            c.stores.add(*p, *k);
        }
        let today = c.date.clone();
        fit_waiting(c, &today);
    } else {
        c.stores.next_order += 1;
        let o = Order { id: c.stores.next_order, parts, ordered: c.date.clone(), due: free_delivery(c), cost, auto: false };
        c.stores.orders.push(o);
    }
    Ok(cost)
}

/// The orders due by `now` come into the stores (told in the inbox), and the buses waiting for
/// them get their parts. Returns those delivered.
fn deliver(c: &mut Company, now: i64) -> Vec<Order> {
    if !c.stores.orders.iter().any(|o| o.due <= now) {
        return Vec::new();
    }
    let (due, rest): (Vec<Order>, Vec<Order>) = std::mem::take(&mut c.stores.orders).into_iter().partition(|o| o.due <= now);
    c.stores.orders = rest;
    for o in &due {
        for (p, n) in &o.parts {
            c.stores.add(*p, *n);
        }
    }
    let n: u32 = due.iter().map(Order::count).sum();
    inbox::post(c, Message::new(Kind::Workshop, "Spare parts delivered", "%{n} spare parts came into the workshop's stores.").arg("n", n.to_string()));
    fit_waiting(c, &clock::date_of(now));
    due
}

/// The stores' hour (`clock::on_minute`): what is due comes in, said in the feed.
pub fn on_minute(c: &mut Company, now: i64) -> Vec<FeedItem> {
    let n: u32 = deliver(c, now).iter().map(Order::count).sum();
    if n == 0 {
        return Vec::new();
    }
    vec![FeedItem { at: now, text: "Spare parts delivered: %{n}".into(), args: vec![("n".into(), n.to_string())], level: Level::Info }]
}

/// What the stores keep of a part for the fleet: its `keep` for every twelve buses.
pub fn keep_of(c: &Company, p: Part) -> u32 {
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count() as u32;
    p.keep() * buses.div_ceil(12).max(1)
}

/// The first stock (once): what the stores keep for the fleet. A new company's first day and
/// an older company's first load put it in (`store::migrate`).
pub fn open(c: &mut Company) {
    if c.stores.opened {
        return;
    }
    c.stores.opened = true;
    for p in Part::ALL {
        let (keep, have) = (keep_of(c, p), c.stores.count(p));
        if have < keep {
            c.stores.add(p, keep - have);
        }
    }
}

/// What runs short ordered for the free delivery of tomorrow: what the waiting buses need, and
/// the stock up to what the stores keep when it fell under half of it.
fn reorder(c: &mut Company, date: &str) {
    let mut want = Vec::new();
    for p in Part::ALL {
        let keep = keep_of(c, p);
        let have = c.stores.count(p) + c.stores.on_order(p);
        let need = c.stores.needed(p);
        if have < need + keep.div_ceil(2) {
            want.push((p, need + keep - have));
        }
    }
    if want.is_empty() {
        return;
    }
    let cost = order_cost(c, &want, false);
    if c.cash < cost {
        return;
    }
    let n: u32 = want.iter().map(|x| x.1).sum();
    c.book(BookingKind::Parts, -cost, format!("Spare parts ({n}), ordered by the stores"), false);
    c.stores.next_order += 1;
    let o = Order { id: c.stores.next_order, parts: want, ordered: date.to_string(), due: clock::moment(&dates::add(date, 1), DELIVERY), cost, auto: true };
    c.stores.orders.push(o);
}

// --- the workshop's work ---------------------------------------------------------------------------

/// A mechanic's working day, hours.
pub const HOURS: f64 = 8.0;

/// The company's mechanics at work on `date`.
pub fn mechanics<'a>(c: &'a Company, date: &'a str) -> impl Iterator<Item = &'a Employee> + 'a {
    c.staff.iter().filter(move |e| e.role == Role::Mechanic && e.employed_on(date) && !e.absent(date))
}

/// How fast a mechanic works against the usual: the experienced and calm ones quicker.
pub fn speed(e: &Employee) -> f64 {
    (0.6 + 0.8 * e.experience.clamp(0.0, 100.0) / 100.0) * (1.1 - 0.4 * e.stress.clamp(0.0, 100.0) / 100.0)
}

/// The hours the mechanics work on `date`.
pub fn workshop_hours(c: &Company, date: &str) -> f64 {
    mechanics(c, date).map(|e| HOURS * speed(e)).sum()
}

/// The workshop days a repair with `part` takes: in the mechanics' `hours` for it (none: an
/// outside garage's `garage_days`), less with the research into repairs, a day less at an
/// external depot (`research::auto_repair`); one at least.
pub fn repair_days(c: &Company, part: Part, garage_days: i64, hours: f64) -> i64 {
    let f = research::repair_factor(c);
    let days = if hours > 0.0 { (part.hours() * f / hours).ceil() as i64 } else { (garage_days as f64 * f).round() as i64 };
    let days = days.max(1);
    if research::auto_repair(c) {
        (days - 1).max(1)
    } else {
        days
    }
}

/// The part a breakdown of `cost` needs (the bigger the damage the bigger the part; drawn from
/// the company, the bus and the day).
pub fn breakdown_part(c: &Company, v: &Vehicle, date: &str, cost: Cents) -> Part {
    let size = match v.kind.size {
        BusSize::Midi => 0.8,
        BusSize::Solo => 1.0,
        BusSize::Articulated | BusSize::Double => 1.35,
    };
    let euros = cost as f64 / 100.0 / (size * c.price_index.max(0.01));
    let mut rng = Rng::of(&[&c.id, "part", &v.id.to_string()], dates::parse(date).unwrap_or(0));
    let pick = |rng: &mut Rng, of: &[Part]| *rng.pick(of).unwrap_or(&of[0]);
    if euros >= 4_200.0 {
        Part::Engine
    } else if euros >= 3_000.0 {
        pick(&mut rng, &[Part::AirConditioner, Part::TyreSet])
    } else if euros >= 1_800.0 {
        pick(&mut rng, &[Part::BrakePads, Part::VBelt, Part::Router])
    } else {
        pick(&mut rng, &[Part::Light, Part::Indicator, Part::Wiper])
    }
}

/// The parts a service takes: its oil always, brake pads every second, a V-belt every third,
/// a tyre set every fourth (counted by the bus's kilometres).
pub fn service_parts(v: &Vehicle) -> Vec<Part> {
    let n = (v.km / market::SERVICE_KM).round().max(1.0) as i64;
    let mut out = vec![Part::OilCanister];
    if n % 2 == 0 {
        out.push(Part::BrakePads);
    }
    if n % 3 == 0 {
        out.push(Part::VBelt);
    }
    if n % 4 == 0 {
        out.push(Part::TyreSet);
    }
    out
}

/// A part fitted from the stores: what it was worth comes off the repair's cost (at most
/// `credit`; None: a service's maintenance, all of it). Returns what came off.
fn fitted(c: &mut Company, p: Part, number: &str, credit: Option<Cents>) -> Cents {
    let worth = list_price(c, p);
    match credit {
        Some(room) => {
            let off = worth.min(room.max(0));
            c.book(BookingKind::Repair, off, format!("Spare part from the stores: {} ({number})", p.label()), false);
            off
        }
        None => {
            c.book(BookingKind::Maintenance, worth, format!("Spare part from the stores: {} ({number})", p.label()), false);
            worth
        }
    }
}

/// A bus that broke down today (a day report's note): the part it needs from the stores, and
/// the days its repair takes (`hours`: the mechanics' for it). Returns its last day in the
/// workshop (None: the dealer repairs it under his warranty, with his parts).
fn repair(c: &mut Company, date: &str, number: &str, until: &str, cost: Cents, hours: f64) -> Option<String> {
    if cost <= 0 {
        return None;
    }
    let v = c.fleet.iter().find(|v| v.number == number && v.held_on(date))?.clone();
    let part = breakdown_part(c, &v, date, cost);
    let days = repair_days(c, part, dates::between(date, until).max(1), hours);
    let room = cost * 9 / 10;
    let until = if c.stores.take(part) {
        fitted(c, part, number, Some(room));
        dates::add(date, days)
    } else {
        c.stores.waiting.push(Waiting { vehicle: v.id, number: number.to_string(), parts: vec![part], since: date.to_string(), days, repair: true, credit: room });
        inbox::post(c, Message::new(Kind::Workshop, "A bus waits for a spare part", part.missing_text()).arg("bus", number));
        dates::add(date, 1)
    };
    if let Some(x) = c.fleet.iter_mut().find(|x| x.id == v.id) {
        x.workshop_until = Some(until.clone());
    }
    Some(until)
}

/// A bus the night sends to its service: its parts from the stores; what they lack keeps it
/// in the workshop.
fn service(c: &mut Company, date: &str, number: &str) {
    let Some(v) = c.fleet.iter().find(|v| v.number == number).cloned() else { return };
    let mut missing = Vec::new();
    for p in service_parts(&v) {
        if c.stores.take(p) {
            fitted(c, p, number, None);
        } else {
            missing.push(p);
        }
    }
    if let Some(first) = missing.first() {
        inbox::post(c, Message::new(Kind::Workshop, "A bus waits for a spare part", first.missing_text()).arg("bus", number));
        c.stores.waiting.push(Waiting { vehicle: v.id, number: number.to_string(), parts: missing, since: date.to_string(), days: 1, repair: false, credit: 0 });
    }
}

/// The buses waiting get what has come: those with all their parts are worked on from `day`
/// (told in the inbox); the others keep waiting. Those no longer in the fleet go.
fn fit_waiting(c: &mut Company, day: &str) {
    let fleet = &c.fleet;
    c.stores.waiting.retain(|w| fleet.iter().any(|v| v.id == w.vehicle));
    for i in 0..c.stores.waiting.len() {
        let mut w = c.stores.waiting[i].clone();
        let mut left = Vec::new();
        for p in std::mem::take(&mut w.parts) {
            if c.stores.take(p) {
                let off = fitted(c, p, &w.number, w.repair.then_some(w.credit));
                if w.repair {
                    w.credit -= off;
                }
            } else {
                left.push(p);
            }
        }
        w.parts = left;
        c.stores.waiting[i] = w;
    }
    let (ready, still): (Vec<Waiting>, Vec<Waiting>) = std::mem::take(&mut c.stores.waiting).into_iter().partition(|w| w.parts.is_empty());
    c.stores.waiting = still;
    for w in ready {
        if let Some(v) = c.fleet.iter_mut().find(|v| v.id == w.vehicle) {
            v.workshop_until = Some(dates::add(day, w.days.max(1)));
        }
        inbox::post(c, Message::new(Kind::Workshop, "The spare part came", "The spare part for bus %{bus} came: the workshop is on it.").arg("bus", w.number.clone()));
    }
}

/// The buses still waiting stay in the workshop tomorrow too.
fn hold(c: &mut Company, date: &str) {
    let tomorrow = dates::add(date, 1);
    let ids: Vec<u32> = c.stores.waiting.iter().map(|w| w.vehicle).collect();
    for v in c.fleet.iter_mut().filter(|v| ids.contains(&v.id)) {
        if v.workshop_until.as_deref().is_none_or(|u| dates::between(u, &tomorrow) > 0) {
            v.workshop_until = Some(tomorrow.clone());
        }
    }
}

/// The stores' day (`day::close_day`): deliveries in, what the day's repairs and services took
/// (the breakdowns' days set by the mechanics), the buses waiting kept in, what runs short
/// ordered.
pub fn day(c: &mut Company, date: &str, report: &mut DayReport) {
    open(c);
    deliver(c, clock::moment(date, clock::DAY - 1));
    // the breakdowns: the mechanics' hours shared between them
    let broken: Vec<(String, String, Cents)> = report
        .notes
        .iter()
        .filter_map(|n| match n {
            Note::Breakdown { number, until, cost } => Some((number.clone(), until.clone(), *cost)),
            _ => None,
        })
        .collect();
    let hours = if broken.is_empty() { 0.0 } else { workshop_hours(c, date) / broken.len() as f64 };
    for (number, until, cost) in broken {
        let Some(until) = repair(c, date, &number, &until, cost, hours) else { continue };
        for n in report.notes.iter_mut() {
            if let Note::Breakdown { number: x, until: u, .. } = n {
                if *x == number {
                    *u = until.clone();
                }
            }
        }
    }
    // the services
    let serviced: Vec<String> = report.notes.iter().filter_map(|n| if let Note::Service { number } = n { Some(number.clone()) } else { None }).collect();
    for number in serviced {
        service(c, date, &number);
    }
    hold(c, date);
    // (the easy mode's stores always order by themselves, whatever the advanced mode's switch)
    if c.stores.auto || super::autopilot::on(c) {
        reorder(c, date);
    }
}

// --- the depot's jobs ------------------------------------------------------------------------------

/// The workshop days a job of the depot takes: fewer with the research into repairs.
pub fn job_days(c: &Company, kind: JobKind) -> i64 {
    if kind == JobKind::Paint {
        return kind.days();
    }
    ((kind.days() as f64 * research::repair_factor(c)).round() as i64).max(1)
}

/// A job of the depot's workshop started (`depot::start_jobs`, `paid` what was booked for it):
/// the parts it takes from the stores, what they were worth coming off its cost (a service's
/// off the maintenance). What the stores lack the garage brings with the job; a repair under
/// the dealer's warranty takes the dealer's parts.
pub fn job_started(c: &mut Company, vehicle: u32, kind: JobKind, paid: Cents) {
    let Some(v) = c.vehicle(vehicle).cloned() else { return };
    let parts = match kind {
        JobKind::Service => service_parts(&v),
        JobKind::Repair => vec![Part::BrakePads, Part::VBelt, Part::Light],
        JobKind::Overhaul => vec![Part::Engine, Part::TyreSet, Part::BrakePads, Part::OilCanister, Part::VBelt],
        JobKind::Paint => Vec::new(),
    };
    if kind != JobKind::Service && paid <= 0 {
        return;
    }
    let mut room = paid * 9 / 10;
    for p in parts {
        if !c.stores.take(p) {
            continue;
        }
        if kind == JobKind::Service {
            fitted(c, p, &v.number, None);
        } else {
            room -= fitted(c, p, &v.number, Some(room));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::market::{MarketBus, Payment};
    use super::super::{found, Founding};
    use super::*;

    fn company() -> Company {
        let mut c = found(&Founding { name: "Lager".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..3 {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        c
    }

    /// A mechanic at work (`exp` experience, `stress`).
    fn mechanic(c: &mut Company, exp: f64, stress: f64) -> u32 {
        let mut a = super::super::staff::applicants(c)[0].clone();
        a.no = 900 + c.staff.len() as u32;
        let id = super::super::staff::hire(c, &a).unwrap();
        let e = c.staff.iter_mut().find(|e| e.id == id).unwrap();
        (e.role, e.experience, e.stress, e.sick_until, e.holiday_until) = (Role::Mechanic, exp, stress, None, None);
        id
    }

    /// A day's report with a breakdown of bus `number` (in the workshop for `days`, `cost`).
    fn broke(c: &Company, number: &str, days: i64, cost: Cents) -> DayReport {
        DayReport { date: c.date.clone(), notes: vec![Note::Breakdown { number: number.into(), until: dates::add(&c.date, days), cost }], ..Default::default() }
    }

    /// The bus broke down as `broke` says (as the day's close sets it).
    fn break_bus(c: &mut Company, number: &str, days: i64) {
        let until = dates::add(&c.date, days);
        c.fleet.iter_mut().find(|v| v.number == number).unwrap().workshop_until = Some(until);
    }

    #[test]
    fn parts_are_ordered_for_16_00_or_at_once() {
        let mut c = company();
        c.stores.auto = false;
        let cash = c.cash;
        // in the morning: today at 16:00, for nothing but the parts
        c.clock.minute = 9 * 60;
        let cost = order(&mut c, &[(Part::Light, 4), (Part::Engine, 0)], false).unwrap();
        assert_eq!(cost, 4 * 150_00);
        assert_eq!(c.cash, cash - cost);
        assert_eq!(c.month(&dates::month_of(&c.date)).get(BookingKind::Parts), -cost);
        assert_eq!(c.stores.orders[0].due, clock::moment(&c.date, DELIVERY));
        // the first stock is there, the order not yet
        let first = keep_of(&c, Part::Light);
        assert_eq!((c.stores.count(Part::Light), c.stores.on_order(Part::Light)), (first, 4));
        // at 16:00 it comes: in store, told in the inbox and the feed
        let today = c.date.clone();
        assert!(on_minute(&mut c, clock::moment(&today, 15 * 60)).is_empty());
        let feed = on_minute(&mut c, clock::moment(&today, 16 * 60));
        assert_eq!(feed.len(), 1);
        assert_eq!(c.stores.count(Part::Light), first + 4);
        assert!(c.stores.orders.is_empty() && c.inbox.messages.last().is_some_and(|m| m.kind == Kind::Workshop && m.args == vec![("n".to_string(), "4".to_string())]));
        // in the evening: tomorrow at 16:00
        c.clock.minute = 17 * 60;
        order(&mut c, &[(Part::Wiper, 1)], false).unwrap();
        assert_eq!(c.stores.orders[0].due, clock::moment(&dates::add(&c.date, 1), DELIVERY));
        // at once, for the fee
        let before = (c.cash, c.stores.count(Part::TyreSet));
        let cost = order(&mut c, &[(Part::TyreSet, 2)], true).unwrap();
        assert_eq!(cost, 2 * 1_200_00 + EXPRESS);
        assert_eq!((c.cash, c.stores.count(Part::TyreSet)), (before.0 - cost, before.1 + 2));
        // nothing chosen, or too little cash
        assert_eq!(order(&mut c, &[(Part::Light, 0)], false), Err("Choose the parts to order first."));
        c.cash = 10_00;
        assert_eq!(order(&mut c, &[(Part::Engine, 1)], false), Err("Not enough cash."));
        // the research's discount, and the difficulty
        c.research.done.push("parts_1".into());
        assert_eq!(price(&c, Part::Engine), 3_150_00);
        c.difficulty = Difficulty::Hard;
        assert!(list_price(&c, Part::Engine) > 3_500_00);
    }

    #[test]
    fn a_breakdown_takes_its_part_from_the_stores() {
        let mut c = company();
        c.stores.auto = false;
        open(&mut c);
        let number = c.fleet[0].number.clone();
        // a big repair needs the engine: fitted, its worth off the repair's cost
        let part = breakdown_part(&c, &c.fleet[0].clone(), &c.date.clone(), 5_000_00);
        assert_eq!(part, Part::Engine);
        assert_eq!(breakdown_part(&c, &c.fleet[0].clone(), &c.date.clone(), 1_000_00).hours(), 1.0);
        let engines = c.stores.count(Part::Engine);
        break_bus(&mut c, &number, 2);
        let mut r = broke(&c, &number, 2, 5_000_00);
        let date = c.date.clone();
        day(&mut c, &date, &mut r);
        assert_eq!(c.stores.count(Part::Engine), engines - 1);
        assert_eq!(c.month(&dates::month_of(&date)).get(BookingKind::Repair), 3_500_00);
        // (no mechanics: the garage's two days)
        assert_eq!(c.fleet[0].workshop_until.as_deref(), Some(dates::add(&date, 2).as_str()));
        // none left: the bus waits, told in the inbox; the delivery repairs it
        let mut c2 = c.clone();
        c2.fleet[0].workshop_until = None;
        c2.date = dates::add(&date, 1);
        let d2 = c2.date.clone();
        break_bus(&mut c2, &number, 1);
        let mut r = broke(&c2, &number, 1, 5_000_00);
        day(&mut c2, &d2, &mut r);
        assert!(c2.stores.waits(c2.fleet[0].id).is_some());
        assert!(c2.inbox.messages.last().is_some_and(|m| m.text.contains("for an engine") && m.args == vec![("bus".to_string(), number.clone())]));
        assert!(c2.fleet[0].in_workshop(&dates::add(&d2, 1)));
        // it stays in while nothing comes
        let d3 = dates::add(&d2, 1);
        c2.date = d3.clone();
        day(&mut c2, &d3, &mut DayReport::default());
        assert!(c2.fleet[0].in_workshop(&dates::add(&d3, 1)));
        // an engine at once: fitted, back after its repair
        order(&mut c2, &[(Part::Engine, 1)], true).unwrap();
        assert!(c2.stores.waiting.is_empty());
        assert_eq!(c2.stores.count(Part::Engine), 0);
        assert!(c2.inbox.messages.last().is_some_and(|m| m.title == "The spare part came"));
        let back = c2.fleet[0].workshop_until.clone().unwrap();
        assert_eq!(back, dates::add(&d3, 1));
        // under the dealer's warranty: no part of ours
        let mut c3 = c.clone();
        let before = c3.stores.count(Part::Light) + c3.stores.count(Part::Indicator) + c3.stores.count(Part::Wiper);
        let mut r = broke(&c3, &number, 1, 0);
        day(&mut c3, &date, &mut r);
        assert_eq!(c3.stores.count(Part::Light) + c3.stores.count(Part::Indicator) + c3.stores.count(Part::Wiper), before);
    }

    #[test]
    fn the_mechanics_and_the_research_make_repairs_quicker() {
        let mut c = company();
        // the garage's days, a day less with the research into repairs once rounded
        assert_eq!(repair_days(&c, Part::Engine, 3, 0.0), 3);
        c.research.done = vec!["repairs_1".into(), "repairs_2".into(), "repairs_3".into()];
        assert_eq!(repair_days(&c, Part::Engine, 3, 0.0), 2);
        c.research.done.clear();
        // own mechanics: the engine's twenty hours in their hours
        let date = c.date.clone();
        mechanic(&mut c, 50.0, 25.0);
        let h = workshop_hours(&c, &date);
        assert!((h - HOURS * 1.0 * 1.0).abs() < 1e-9, "{h}");
        assert_eq!(repair_days(&c, Part::Engine, 1, h), 3);
        assert_eq!(repair_days(&c, Part::Light, 3, h), 1);
        mechanic(&mut c, 50.0, 25.0);
        mechanic(&mut c, 50.0, 25.0);
        assert_eq!(repair_days(&c, Part::Engine, 1, workshop_hours(&c, &date)), 1);
        // an ill mechanic does not work
        c.staff[0].sick_until = Some(date.clone());
        assert_eq!(mechanics(&c, &date).count(), 2);
        // at an external depot a day sooner
        c.research.done = vec!["premises_1".into(), "external_depots".into(), "auto_repair".into()];
        assert_eq!(repair_days(&c, Part::Engine, 3, 0.0), 2);
        // the depot's jobs too
        c.research.done = vec!["repairs_1".into(), "repairs_2".into(), "repairs_3".into()];
        assert_eq!((job_days(&c, JobKind::Overhaul), job_days(&c, JobKind::Repair), job_days(&c, JobKind::Service)), (4, 1, 1));
    }

    #[test]
    fn a_service_takes_its_oil_and_the_stores_order_what_runs_short() {
        let mut c = company();
        open(&mut c);
        let v = c.fleet[0].clone();
        c.fleet[0].km = 60_100.0;
        assert_eq!(service_parts(&c.fleet[0]), vec![Part::OilCanister, Part::BrakePads]);
        let oil = c.stores.count(Part::OilCanister);
        let date = c.date.clone();
        let mut r = DayReport { date: date.clone(), notes: vec![Note::Service { number: v.number.clone() }], ..Default::default() };
        day(&mut c, &date, &mut r);
        assert_eq!(c.stores.count(Part::OilCanister), oil - 1);
        // (their worth off the maintenance)
        assert_eq!(c.month(&dates::month_of(&date)).get(BookingKind::Maintenance), 110_00 + 180_00);
        // the stock fell under half of what is kept: the stores order it for tomorrow
        c.stores.stock.retain(|x| x.0 != Part::Wiper);
        let d2 = dates::add(&date, 1);
        c.date = d2.clone();
        day(&mut c, &d2, &mut DayReport::default());
        let o = c.stores.orders.last().unwrap();
        assert!(o.auto && o.parts.contains(&(Part::Wiper, keep_of(&c, Part::Wiper))) && o.due == clock::moment(&dates::add(&d2, 1), DELIVERY));
        // switched off: nothing
        let mut off = c.clone();
        off.stores.orders.clear();
        off.stores.auto = false;
        day(&mut off, &d2, &mut DayReport::default());
        assert!(off.stores.orders.is_empty());
        // the next day's close brings it
        let d3 = dates::add(&d2, 1);
        c.date = d3.clone();
        day(&mut c, &d3, &mut DayReport::default());
        assert_eq!(c.stores.count(Part::Wiper), keep_of(&c, Part::Wiper));
    }

    /// The world of the clock without a timetable: the night as the store runs it.
    struct Quiet;

    impl clock::World for Quiet {
        fn lines(&mut self, _: &Company, _: &str) -> Result<Vec<crate::LineInfo>, String> {
            Ok(Vec::new())
        }

        fn close_day(&mut self, c: &mut Company, lines: &[crate::LineInfo]) -> Result<DayReport, String> {
            let r = super::super::day::close_day(c, Vec::new(), &[]);
            Ok(super::super::depot::after_close(c, r, lines))
        }
    }

    #[test]
    fn the_clock_brings_an_order_at_16_00() {
        let mut c = company();
        c.stores.auto = false;
        // ordered at nine: in store when the clock passes four, told in the feed and the inbox
        clock::advance(&mut c, clock::moment("2024-03-04", 9 * 60), &mut Quiet, true).unwrap();
        let before = c.stores.count(Part::Router);
        order(&mut c, &[(Part::Router, 2)], false).unwrap();
        clock::advance(&mut c, clock::moment("2024-03-04", 15 * 60 + 30), &mut Quiet, true).unwrap();
        assert_eq!(c.stores.count(Part::Router), before);
        let run = clock::advance(&mut c, clock::moment("2024-03-04", 17 * 60), &mut Quiet, true).unwrap();
        assert_eq!(c.stores.count(Part::Router), before + 2);
        assert!(run.told > 0 && c.clock.feed.iter().any(|f| f.text == "Spare parts delivered: %{n}" && f.at == clock::moment("2024-03-04", DELIVERY)));
        assert!(c.inbox.messages.iter().any(|m| m.title == "Spare parts delivered"));
        // ordered after four: tomorrow, through the night's close
        order(&mut c, &[(Part::Router, 1)], false).unwrap();
        clock::advance(&mut c, clock::moment("2024-03-05", 12 * 60), &mut Quiet, true).unwrap();
        assert_eq!(c.stores.count(Part::Router), before + 2);
        clock::advance(&mut c, clock::moment("2024-03-05", 16 * 60), &mut Quiet, true).unwrap();
        assert_eq!(c.stores.count(Part::Router), before + 3);
    }

    #[test]
    fn a_job_of_the_depot_takes_parts_and_an_older_company_gets_a_stock() {
        let mut c = company();
        open(&mut c);
        let id = c.fleet[0].id;
        let engines = c.stores.count(Part::Engine);
        job_started(&mut c, id, JobKind::Overhaul, 16_800_00);
        assert_eq!(c.stores.count(Part::Engine), engines - 1);
        assert!(c.month(&dates::month_of(&c.date)).get(BookingKind::Repair) > 3_500_00);
        // an older file's stores: switched on, opened once with what the fleet needs
        let old: Stores = serde_json::from_str("{}").unwrap();
        assert!(old.auto && !old.opened && old.stock.is_empty());
        let mut d = company();
        d.stores = old;
        let d = super::super::store::migrate(d);
        assert!(d.stores.opened && Part::ALL.iter().all(|p| d.stores.count(*p) == p.keep()));
    }
}
