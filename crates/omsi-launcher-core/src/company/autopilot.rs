//! The easy mode (Luc: "make an easy mode and an advanced mode for the bus company mode too
//! ... easy you must think up yourself, what is suitable, so that players get a choice"): for a
//! player who wants to run buses without managing every detail, the company does its routine
//! by itself - what a careful player of the advanced mode would see to every day.
//!
//! - *The morning* (`morning`: as the company enters its day, and on the launcher's page when
//!   today's plan is made): its lines are put into service as soon as they are the company's,
//!   a concession put out again is renewed at the asking price, and the day's roster is filled
//!   anew with the buses and drivers that are there - the player's own duties kept.
//! - *The night* (`night`: at the end of the day's close): drivers hired for the duties the
//!   day had and the other roles to a sensible minimum for the fleet, requests for a raise
//!   granted, drivers trained for the fleet's buses, worn buses sent to the workshop, goals
//!   taken on, research started on a few clear upgrades, parking built before the yard is
//!   full.
//! - *Always*: the stores order their spare parts (`parts::day`), the dispatcher decides a
//!   breakdown and the morning's gaps and no tender stops the clock (`clock`), the dealer sells
//!   quickly (`set_mode`), and a map line's concession is taken at the asking price instead of
//!   its auction (`concessions::take_line`).
//!
//! It spends with care: never below a month of wages and a little more (`cushion`). Nothing of
//! it is lost when the company goes advanced - the roster, the people, the research and the
//! buildings are the company's as they were made, and every page shows them. What it did
//! lately is kept (`Autopilot::log`) for the start screen.

use super::concessions::{self as cn, Event};
use super::contracts;
use super::dates;
use super::dealer::BuyingMode;
use super::depot::{self, Area, JobKind};
use super::licences::{self, Endorsement};
use super::model::{Cents, Company, Licence, Mode, Role};
use super::network;
use super::plan::{self, Who};
use super::research;
use super::staff;
use super::training::CourseKind;
use crate::LineInfo;
use serde::{Deserialize, Serialize};

/// What the easy mode keeps.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Autopilot {
    /// What the company did by itself lately, the newest last (`LOG_KEPT` of them).
    #[serde(default)]
    pub log: Vec<Deed>,
    /// The drivers the duties of a day ask for (`network::drivers_for`), as the last morning or
    /// night counted them.
    #[serde(default)]
    pub drivers: u32,
    /// How the dealer sold before the easy mode made it the quick buy: given back when the
    /// company goes advanced.
    #[serde(default)]
    pub buying: Option<BuyingMode>,
}

/// Something the company did by itself: its day, its words (English, a text with `%{name}`
/// places, translated when shown) and what fills them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Deed {
    pub date: String,
    pub text: String,
    #[serde(default)]
    pub args: Vec<(String, String)>,
}

/// How many deeds are kept.
pub const LOG_KEPT: usize = 40;
/// How many drivers a night hires at most, and people of each other role.
pub const DRIVERS_A_NIGHT: usize = 3;
/// A bus this worn (its condition) goes to the workshop to be repaired.
pub const WORN: f64 = 55.0;

// --- what it says ----------------------------------------------------------------------------------

pub const LINE_STARTED: &str = "Line %{n} is in service: its tours run from now on.";
pub const ROSTER: &str = "The day's roster is filled: %{d} duties and %{b} buses given.";
pub const RENEWED: &str = "The concession for line %{n} is renewed at the asking price, %{amount}, until %{date}.";
pub const NOT_RENEWED: &str = "The concession for line %{n} could not be renewed: the cash does not cover the asking price of %{amount}.";
pub const HIRED: &str = "Hired: %{name} (%{role}).";
pub const RAISE: &str = "%{name} asked for more pay and got it.";
pub const TYPE_TRAINING: &str = "%{name} learns to drive the %{bus}.";
pub const COURSE: &str = "%{name} is booked on a course: %{what}.";
pub const REPAIR: &str = "Bus %{n} goes to the workshop to be repaired: %{amount}.";
pub const GOAL: &str = "A goal is taken on: %{trophies} trophies and %{reward} when it is reached.";
pub const RESEARCH: &str = "Research started: %{what}.";
pub const PARKING: &str = "More parking spaces are being built: %{amount}.";
pub const PARTS: &str = "Spare parts ordered for the stores: %{amount}.";

/// Every text the easy mode writes, for the translations' check.
pub fn texts() -> Vec<&'static str> {
    vec![LINE_STARTED, ROSTER, RENEWED, NOT_RENEWED, HIRED, RAISE, TYPE_TRAINING, COURSE, REPAIR, GOAL, RESEARCH, PARKING, PARTS]
}

// --- the mode ------------------------------------------------------------------------------------------

/// Whether the company is run in the easy mode.
pub fn on(c: &Company) -> bool {
    c.mode == Mode::Easy
}

/// Run the company in `mode` from now on. The easy mode buys quickly (the buying chosen is
/// kept, and given back when it goes advanced again) and leaves to the dispatcher what waits
/// for an answer; going advanced changes nothing else - everything the easy mode made stays.
pub fn set_mode(c: &mut Company, mode: Mode) {
    if c.mode == mode {
        return;
    }
    match mode {
        Mode::Easy => {
            c.autopilot.buying = Some(c.dealer.mode);
            c.dealer.mode = BuyingMode::Simple;
            if c.clock.ask.is_some() {
                let mut run = super::clock::Run::default();
                super::clock::answer(c, None, &mut run);
            }
        }
        Mode::Advanced => {
            if let Some(b) = c.autopilot.buying.take() {
                c.dealer.mode = b;
            }
        }
    }
    c.mode = mode;
}

/// What the company did by itself on `date` (the newest last).
pub fn deeds_of<'a>(c: &'a Company, date: &'a str) -> impl Iterator<Item = &'a Deed> + 'a {
    c.autopilot.log.iter().filter(move |d| d.date == date)
}

/// Note a deed: once a day each (the same words again on the same day take the place of the
/// first: the morning is made again whenever today's plan is).
fn note(c: &mut Company, date: &str, text: &str, args: Vec<(&str, String)>) {
    let args: Vec<(String, String)> = args.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    // (the roster's deed is the day's last count of it)
    let same = |d: &Deed| d.date == date && d.text == text && (text == ROSTER || d.args == args);
    c.autopilot.log.retain(|d| !same(d));
    c.autopilot.log.push(Deed { date: date.to_string(), text: text.to_string(), args });
    if c.autopilot.log.len() > LOG_KEPT {
        let extra = c.autopilot.log.len() - LOG_KEPT;
        c.autopilot.log.drain(..extra);
    }
}

/// What the company keeps back: a month of its people's wages and €5,000 (at today's prices).
pub fn cushion(c: &Company) -> Cents {
    c.staff.iter().filter(|e| e.notice_until.is_none()).map(staff::monthly_cost).sum::<Cents>() + (5_000_00.0 * c.price_index.max(1.0)).round() as Cents
}

/// Whether the company can spend `cost` and keep its cushion.
fn can_spend(c: &Company, cost: Cents) -> bool {
    c.cash - cost >= cushion(c)
}

// --- the morning -----------------------------------------------------------------------------------

/// The company's morning in the easy mode (`lines`: the timetable of `date`): its lines in
/// service, the concessions put out again renewed, the day's roster filled anew and the drivers
/// its duties ask for counted. Returns whether anything changed.
pub fn morning(c: &mut Company, date: &str, lines: &[LineInfo]) -> bool {
    let before = (c.planning.week.clone(), c.lines.iter().map(|l| l.service_from).collect::<Vec<_>>(), c.concessions.held.len(), c.cash, c.autopilot.drivers);
    renew(c, date);
    start_lines(c, date);
    let tours = network::tours_of_day(c, lines, date);
    let duties: usize = tours.iter().filter(|t| !t.unplanned).map(|t| network::duties_of(t).len()).sum();
    c.autopilot.drivers = network::drivers_for(duties) as u32;
    // the weekday's roster made anew with what is there today (the player's own duties kept)
    let wd = plan::weekday_of(date);
    for r in c.planning.week.iter_mut().filter(|r| r.weekday == wd) {
        r.bus = None;
        for d in r.duties.iter_mut() {
            if *d != Some(Who::Player) {
                *d = None;
            }
        }
        while r.duties.last() == Some(&None) {
            r.duties.pop();
        }
    }
    c.planning.week.retain(|r| r.bus.is_some() || r.duties.iter().any(Option::is_some));
    let f = plan::fill_day_told(c, date, tours);
    if f.duties + f.buses > 0 {
        note(c, date, ROSTER, vec![("d", f.duties.to_string()), ("b", f.buses.to_string())]);
    }
    let after = (c.planning.week.clone(), c.lines.iter().map(|l| l.service_from).collect::<Vec<_>>(), c.concessions.held.len(), c.cash, c.autopilot.drivers);
    before != after
}

/// Every line of the company not in service yet put into service from now (one started
/// for a later moment keeps it).
pub fn start_lines(c: &mut Company, date: &str) {
    let now = super::clock::now(c);
    let waiting: Vec<(String, String)> = c.lines.iter().filter(|l| l.service_from.is_none()).map(|l| (l.name.clone(), l.number.clone())).collect();
    for (name, number) in waiting {
        if network::start_service(c, &name, now).is_ok() {
            note(c, date, LINE_STARTED, vec![("n", number)]);
        }
    }
}

/// The renewals of the company's concessions taken at the asking price, before their auction.
fn renew(c: &mut Company, date: &str) {
    let due: Vec<u32> = c.concessions.tenders.iter().filter(|t| t.open() && t.renewal && cn::of_line(c, &t.line).is_some()).map(|t| t.id).collect();
    for id in due {
        let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { continue };
        let price = cn::asking_price(c, &t);
        match cn::take_at_asking(c, id) {
            Ok((Event::Won { number, until }, paid)) => note(c, date, RENEWED, vec![("n", number), ("amount", paid.to_string()), ("date", until)]),
            Ok(_) => {}
            Err(_) => note(c, date, NOT_RENEWED, vec![("n", t.number.clone()), ("amount", price.to_string())]),
        }
    }
}

// --- the night -------------------------------------------------------------------------------------

/// The company's night in the easy mode, at the end of the day `date`'s close (`duties`: the
/// day's duties the company's drivers had): what it books is the day's.
pub fn night(c: &mut Company, date: &str, duties: usize) {
    let tomorrow = dates::add(date, 1);
    // (the drivers the day's duties asked for, or its morning counted)
    c.autopilot.drivers = c.autopilot.drivers.max(network::drivers_for(duties) as u32);
    raises(c, date);
    hire(c, date, &tomorrow);
    train(c, date);
    repairs(c, date, &tomorrow);
    goals(c, date);
    research_next(c, date);
    parking(c, date);
    // (what the stores ordered by themselves tonight: `parts::day`)
    let parts: Cents = c.stores.orders.iter().filter(|o| o.auto && o.ordered == date).map(|o| o.cost).sum();
    if parts > 0 {
        note(c, date, PARTS, vec![("amount", parts.to_string())]);
    }
}

/// Every request for a raise granted.
fn raises(c: &mut Company, date: &str) {
    let asks: Vec<u32> = c.staff_rules.asks.iter().map(|a| a.id).collect();
    for id in asks {
        let name = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
        if staff::answer_raise(c, id, true).is_ok() {
            note(c, date, RAISE, vec![("name", name)]);
        }
    }
}

/// The people a role should have in the easy mode: the drivers the day's duties ask for, and
/// the others by the size of the fleet - a mechanic from three buses (one for every eight), a
/// cleaner from five (one for every twelve), a researcher from six, a service employee from
/// eight (one for every twenty) and, once they may be hired, a ticket inspector from ten.
pub fn minimum(c: &Company, role: Role) -> usize {
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    let from = |least: usize, per: usize| if buses >= least { (buses / per).max(1) } else { 0 };
    match role {
        Role::Driver => c.autopilot.drivers as usize,
        Role::Mechanic => from(3, 8),
        Role::Cleaner => from(5, 12),
        Role::Researcher => from(6, 25),
        Role::Service => from(8, 20),
        Role::Inspector => {
            if staff::hireable(c, Role::Inspector).is_ok() {
                from(10, 20)
            } else {
                0
            }
        }
    }
}

/// The roles under their minimum take the week's best applicants: up to `DRIVERS_A_NIGHT`
/// drivers a night, one of every other role.
fn hire(c: &mut Company, date: &str, tomorrow: &str) {
    for role in Role::ALL {
        let want = minimum(c, role);
        let most = if role == Role::Driver { DRIVERS_A_NIGHT } else { 1 };
        for _ in 0..most {
            if staff::staying(c, role, tomorrow) >= want {
                break;
            }
            let Some(name) = staff::hire_best(c, role) else { break };
            // (the cushion counts them from now on)
            note(c, date, HIRED, vec![("name", name), ("role", role.label().to_string())]);
        }
    }
}

/// The drivers trained for the fleet's buses: of every model family half of them at least,
/// and of the endorsements the fleet asks for twice as many drivers as such buses - two a night
/// each, the cash allowing.
fn train(c: &mut Company, date: &str) {
    let today = c.date.clone();
    let free = |c: &Company| -> Vec<(u32, f64)> {
        let mut v: Vec<(u32, f64)> = c
            .staff
            .iter()
            .filter(|e| staff::drives(e) && e.employed_on(&today) && e.notice_until.is_none() && !e.absent(&today))
            .map(|e| (e.id, e.experience))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    };
    let drivers = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(&today) && e.notice_until.is_none()).count();
    if drivers == 0 {
        return;
    }
    // the model families
    for key in licences::fleet_types(c) {
        let with = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(&today) && e.types.contains(&key)).count();
        let booked = c.progress.courses.iter().filter(|x| !x.done && x.kind == CourseKind::TypeTraining && x.subject == key).count();
        let mut short = drivers.div_ceil(2).saturating_sub(with + booked).min(2);
        for (id, _) in free(c) {
            if short == 0 {
                break;
            }
            let lacks = c.employee(id).is_some_and(|e| !e.types.contains(&key));
            if !lacks || !can_spend(c, licences::course_cost(c, CourseKind::TypeTraining)) {
                continue;
            }
            let name = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
            if licences::enrol(c, id, CourseKind::TypeTraining, &key).is_ok() {
                note(c, date, TYPE_TRAINING, vec![("name", name), ("bus", licences::type_name(c, &key))]);
                short -= 1;
            }
        }
    }
    // the endorsements
    for x in Endorsement::ALL {
        let buses = c.fleet.iter().filter(|v| v.held_on(&today) && licences::needed(v.kind).contains(&x)).count();
        if buses == 0 {
            continue;
        }
        let kind = x.course();
        let with = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(&today) && e.endorsements.contains(&x)).count();
        let booked = c.progress.courses.iter().filter(|k| !k.done && k.kind == kind).count();
        let mut short = (2 * buses).min(drivers).saturating_sub(with + booked).min(2);
        for (id, _) in free(c) {
            if short == 0 {
                break;
            }
            let fits = c.employee(id).is_some_and(|e| !e.endorsements.contains(&x) && (e.licence == Licence::D || x == Endorsement::Electric));
            if !fits || !can_spend(c, licences::course_cost(c, kind)) {
                continue;
            }
            let name = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
            if licences::enrol(c, id, kind, "").is_ok() {
                note(c, date, COURSE, vec![("name", name), ("what", x.label().to_string())]);
                short -= 1;
            }
        }
    }
}

/// Worn buses sent to the workshop to be repaired - one at a time for every ten buses, so the
/// tours keep theirs.
fn repairs(c: &mut Company, date: &str, tomorrow: &str) {
    let held = c.fleet.iter().filter(|v| v.held_on(tomorrow)).count();
    let at_once = (held / 10).max(1);
    let mut busy = c.site.jobs.iter().filter(|j| j.kind == JobKind::Repair).count();
    let mut worn: Vec<(u32, f64, String)> = c
        .fleet
        .iter()
        .filter(|v| v.held_on(tomorrow) && v.condition < WORN && !v.in_workshop(tomorrow) && c.site.job_of(v.id).is_none())
        .map(|v| (v.id, v.condition, v.number.clone()))
        .collect();
    worn.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (id, _, number) in worn {
        if busy >= at_once {
            break;
        }
        let cost = depot::job_cost(c, id, JobKind::Repair);
        if !can_spend(c, cost) {
            break;
        }
        if depot::order(c, id, JobKind::Repair).is_ok() {
            note(c, date, REPAIR, vec![("n", number), ("amount", cost.to_string())]);
            busy += 1;
        }
    }
}

/// The goals on offer taken on while fewer than `contracts::RUNNING_MAX` run, the richest in
/// trophies first (a goal missed costs nothing).
fn goals(c: &mut Company, date: &str) {
    let mut offered: Vec<(u32, i64, Cents)> = c.contracts.goals.iter().filter(|g| g.state == contracts::State::Offered && g.offered_until.as_str() >= date).map(|g| (g.id, g.trophies, g.reward)).collect();
    offered.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
    for (id, trophies, reward) in offered {
        let running = c.contracts.goals.iter().filter(|g| g.state == contracts::State::Running).count();
        if running >= contracts::RUNNING_MAX {
            break;
        }
        if contracts::take(c, id).is_ok() {
            note(c, date, GOAL, vec![("trophies", trophies.to_string()), ("reward", reward.to_string())]);
        }
    }
}

/// The few clear upgrades the easy mode researches, in its order: cheaper running first, then
/// room, bigger buses, and what the passengers notice.
pub const UPGRADES: [&str; 14] = [
    "fuel",
    "wear",
    "premises",
    research::keys::ARTICULATED,
    "air_conditioning",
    "passenger_info",
    "stops",
    "dirt",
    "auto_cleaning",
    "parts",
    "repairs",
    "ticket_machines",
    "food",
    research::keys::DOUBLE_DECKERS,
];

/// The next upgrade started when nothing is researched and a researcher works here: the first
/// of `UPGRADES` that is open, whose trophies the company has and whose money it can spend
/// twice over.
fn research_next(c: &mut Company, date: &str) {
    if c.research.running.is_some() || staff::staying(c, Role::Researcher, date) == 0 {
        return;
    }
    for key in UPGRADES {
        let Some(t) = research::TOPICS.iter().find(|t| t.key == key) else { continue };
        if research::state(c, t) != research::State::Open {
            continue;
        }
        let k = research::cost(c, t, research::stages_done(&c.research, t) + 1);
        if c.trophies < k.trophies || !can_spend(c, 2 * k.money) {
            continue;
        }
        if research::start(c, key).is_ok() {
            note(c, date, RESEARCH, vec![("what", t.title.to_string())]);
            return;
        }
    }
}

/// More parking spaces built before the yard is full: when the buses held and on order come
/// within two of its places.
fn parking(c: &mut Company, date: &str) {
    if c.site.works_on(Area::Parking).is_some() {
        return;
    }
    let held = c.fleet.iter().filter(|v| v.held_on(date)).count();
    let ordered: usize = c.dealer.orders.iter().map(|o| o.contract.count as usize).sum();
    if held + ordered + 2 < depot::places(c) {
        return;
    }
    let Some((cost, _)) = depot::next_cost(c, Area::Parking) else { return };
    if can_spend(c, cost) && depot::build(c, Area::Parking).is_ok() {
        note(c, date, PARKING, vec![("amount", cost.to_string())]);
    }
}

/// A map line's concession taken at the asking price (the easy mode's way to a line: no
/// auction): the line is the company's from tomorrow, and its morning puts it into service.
pub fn take_line(c: &mut Company, l: &LineInfo) -> Result<(Event, Cents), &'static str> {
    cn::take_line(c, l)
}

#[cfg(test)]
mod tests {
    use super::super::clock::{self, World};
    use super::super::concessions::tests::line;
    use super::super::day;
    use super::super::market::{self, MarketBus, Payment};
    use super::super::model::Difficulty;
    use super::super::{found, store, Founding};
    use super::*;
    use crate::company::day::DayReport;

    fn company(mode: Mode, d: Difficulty, buses: usize, drivers: usize) -> (Company, Vec<LineInfo>) {
        let lines = vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true)];
        let mut c = found(&Founding { name: "Leicht".into(), difficulty: d, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), mode, ..Default::default() }, "Luc");
        c.cash += 5_000_000_00;
        network::add_line(&mut c, &lines[0], None).unwrap();
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..buses {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        let id = c.id.clone();
        let today = c.date.clone();
        while staff::staying(&c, Role::Driver, &today) < drivers {
            for a in staff::applicants(&c) {
                if staff::staying(&c, Role::Driver, &today) < drivers && a.licence == Licence::D {
                    staff::hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id;
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
        }
        (c, lines)
    }

    /// The timetable of every day the same; the night as the store runs it.
    struct Fake(Vec<LineInfo>);

    impl World for Fake {
        fn lines(&mut self, _: &Company, _: &str) -> Result<Vec<LineInfo>, String> {
            Ok(self.0.clone())
        }

        fn close_day(&mut self, c: &mut Company, lines: &[LineInfo]) -> Result<DayReport, String> {
            network::refresh_lines(c, lines);
            let date = c.date.clone();
            let tours = network::tours_of_day(c, lines, &date);
            let r = day::close_day(c, tours, &[]);
            Ok(depot::after_close(c, r, lines))
        }
    }

    #[test]
    fn the_mode_is_saved_and_an_older_file_is_advanced() {
        let (easy, _) = company(Mode::Easy, Difficulty::Realistic, 0, 0);
        assert_eq!(easy.mode, Mode::Easy);
        let dir = std::env::temp_dir().join(format!("openomsi-autopilot-mode-{}", std::process::id()));
        store::save(&dir, &easy).unwrap();
        assert_eq!(store::load(&dir, &easy.id).unwrap().mode, Mode::Easy);
        // a file from before the modes: no mode, no autopilot
        let mut v = serde_json::to_value(&easy).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("mode");
        o.remove("autopilot");
        std::fs::write(store::path_of(&dir, &easy.id), serde_json::to_string(&v).unwrap()).unwrap();
        let old = store::load(&dir, &easy.id).unwrap();
        assert_eq!((old.mode, old.autopilot.clone()), (Mode::Advanced, Autopilot::default()));
        let _ = std::fs::remove_dir_all(&dir);
        // (a company founded without a choice is advanced, as before)
        assert_eq!(found(&Founding::default(), "x").mode, Mode::Advanced);
    }

    #[test]
    fn the_easy_mode_buys_quickly_and_the_advanced_mode_gets_its_buying_back() {
        let f = Founding { name: "Kauf".into(), buying: BuyingMode::Advanced, mode: Mode::Easy, ..Default::default() };
        let mut c = found(&f, "Luc");
        assert_eq!((c.dealer.mode, c.autopilot.buying), (BuyingMode::Simple, Some(BuyingMode::Advanced)));
        set_mode(&mut c, Mode::Advanced);
        assert_eq!((c.mode, c.dealer.mode, c.autopilot.buying), (Mode::Advanced, BuyingMode::Advanced, None));
        // (and back: nothing else moves)
        let before = c.clone();
        set_mode(&mut c, Mode::Easy);
        set_mode(&mut c, Mode::Advanced);
        assert_eq!(c, before);
    }

    #[test]
    fn the_morning_puts_the_lines_in_service_and_fills_the_roster() {
        let (mut c, lines) = company(Mode::Easy, Difficulty::Realistic, 3, 6);
        assert!(c.lines[0].service_from.is_none());
        let date = c.date.clone();
        assert!(morning(&mut c, &date, &lines));
        assert!(network::in_service(&c.lines[0], clock::now(&c)));
        // every tour of the day has its bus and its driver
        let wd = plan::weekday_of(&date);
        let r = plan::rostered(&c, wd);
        assert_eq!((r.buses, r.duties), (3, 3), "{:?}", c.planning.week);
        assert!(deeds_of(&c, &date).any(|d| d.text == LINE_STARTED));
        assert!(deeds_of(&c, &date).any(|d| d.text == ROSTER));
        assert_eq!(c.autopilot.drivers, network::drivers_for(3) as u32);
        // the player's own duty is kept, the rest made anew; made again, nothing changes
        let tour = c.planning.week.iter().find(|r| r.weekday == wd).unwrap().clone();
        plan::set_driver(&mut c, wd, &tour.line, &tour.tour, 0, Some(Who::Player));
        morning(&mut c, &date, &lines);
        assert_eq!(plan::roster(&c, wd, &tour.line, &tour.tour).unwrap().duties[0], Some(Who::Player));
        assert!(!morning(&mut c, &date, &lines), "the same morning again");
        assert_eq!(deeds_of(&c, &date).filter(|d| d.text == ROSTER).count(), 1);
    }

    #[test]
    fn a_closed_day_fills_the_roster_hires_orders_parts_and_sees_to_the_fleet() {
        let (mut c, lines) = company(Mode::Easy, Difficulty::Realistic, 8, 2);
        // (the stores empty and not ordering by themselves, a bus worn out)
        c.stores.stock.clear();
        c.stores.auto = false;
        c.fleet[0].condition = 30.0;
        let mut w = Fake(lines);
        let to = clock::moment(&super::super::dates::add(&c.date, 1), 0);
        let run = clock::advance(&mut c, to, &mut w, false).unwrap();
        assert_eq!(run.reports.len(), 1, "the easy mode asks nothing: {:?}", c.clock.ask);
        let date = run.reports[0].date.clone();
        // its morning put the line into service and filled the roster
        assert!(network::in_service(&c.lines[0], clock::now(&c)));
        assert!(run.reports[0].tours > 0 && run.reports[0].covered > 0, "{:?}", run.reports[0]);
        // the night: drivers for the duties, and for eight buses a mechanic, a cleaner, a
        // researcher and a service employee
        let deeds: Vec<&Deed> = deeds_of(&c, &date).collect();
        let today = c.date.clone();
        assert!(staff::staying(&c, Role::Driver, &today) > 2, "{deeds:?}");
        for role in [Role::Mechanic, Role::Cleaner, Role::Researcher, Role::Service] {
            assert_eq!(staff::staying(&c, role, &today), 1, "{role:?}");
        }
        assert!(deeds.iter().any(|d| d.text == HIRED));
        // the stores ordered what ran short, the worn bus has its repair, goals run, research too
        assert!(c.stores.orders.iter().any(|o| o.auto), "parts");
        assert!(deeds.iter().any(|d| d.text == PARTS));
        let worn = c.fleet[0].id;
        assert!(c.site.jobs.iter().any(|j| j.vehicle == worn && j.kind == JobKind::Repair), "{:?}", c.site.jobs);
        assert!(c.contracts.goals.iter().any(|g| g.state == contracts::State::Running), "goals");
        assert!(c.research.running.as_ref().is_some_and(|r| r.topic == "fuel"), "{:?}", c.research.running);
        // and it kept its cushion
        assert!(c.cash >= cushion(&c));
    }

    #[test]
    fn the_advanced_mode_leaves_it_all_to_the_player() {
        let (mut c, lines) = company(Mode::Advanced, Difficulty::Realistic, 8, 2);
        c.fleet[0].condition = 30.0;
        let staff_before = c.staff.len();
        let date = c.date.clone();
        let tours = network::tours_of_day(&c, &lines, &date);
        day::close_day(&mut c, tours, &[]);
        assert!(c.lines[0].service_from.is_none() && c.planning.week.is_empty());
        assert_eq!(c.staff.len(), staff_before);
        assert!(c.site.jobs.is_empty() && c.research.running.is_none() && c.autopilot.log.is_empty());
        assert!(!c.contracts.goals.iter().any(|g| g.state == contracts::State::Running));
        // (and its clock stops for what it is asked)
        let mut w = Fake(lines);
        let to = clock::moment(&super::super::dates::add(&c.date, 3), 0);
        assert!(clock::advance(&mut c, to, &mut w, false).unwrap().stopped);
    }

    #[test]
    fn a_line_is_taken_at_the_asking_price_and_runs_from_tomorrow() {
        let (mut c, lines) = company(Mode::Easy, Difficulty::Realistic, 3, 6);
        let price = cn::asking_price_of(&c, &lines[1]);
        let cash = c.cash;
        let (ev, paid) = take_line(&mut c, &lines[1]).unwrap();
        assert_eq!(paid, price);
        assert_eq!(c.cash, cash - price);
        assert!(matches!(ev, Event::Won { .. }));
        let h = cn::of_line(&c, "Linie7").unwrap();
        assert_eq!(h.from, super::super::dates::add(&c.date, 1));
        assert!(cn::open_tender(&c, "Linie7").is_none(), "no auction left open");
        // the night takes the line on, the next morning puts it into service
        let date = c.date.clone();
        c.date = super::super::dates::add(&date, 1);
        cn::after_day(&mut c, &date, &[], &lines);
        let next = c.date.clone();
        morning(&mut c, &next, &lines);
        let l7 = c.lines.iter().find(|l| l.name == "Linie7").unwrap();
        assert!(network::in_service(l7, clock::now(&c)));
        // without the cash nothing changes
        let (mut poor, lines) = company(Mode::Easy, Difficulty::Realistic, 0, 0);
        poor.cash = 100_00;
        let before = poor.clone();
        assert_eq!(take_line(&mut poor, &lines[1]).err(), Some("Not enough cash."));
        assert_eq!(poor, before);
    }

    #[test]
    fn a_concession_put_out_again_is_renewed_at_the_asking_price() {
        let (mut c, lines) = company(Mode::Easy, Difficulty::Realistic, 3, 6);
        cn::ensure(&mut c);
        let until = super::super::dates::add(&c.date, 20);
        c.concessions.held[0].until = until.clone();
        cn::refresh(&mut c, &lines);
        assert!(c.concessions.tenders.iter().any(|t| t.renewal && t.open()));
        let date = c.date.clone();
        let cash = c.cash;
        morning(&mut c, &date, &lines);
        let h = cn::of_line(&c, "Linie5").unwrap();
        assert!(super::super::dates::between(&until, &h.until) > 300, "{} -> {}", until, h.until);
        assert!(c.cash < cash && !c.concessions.tenders.iter().any(|t| t.renewal && t.open()));
        assert!(deeds_of(&c, &date).any(|d| d.text == RENEWED));
    }

    #[test]
    fn the_minimum_staff_grows_with_the_fleet() {
        let (mut c, _) = company(Mode::Easy, Difficulty::Realistic, 0, 0);
        assert!(Role::ALL.iter().all(|r| minimum(&c, *r) == 0));
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..3 {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        assert_eq!((minimum(&c, Role::Mechanic), minimum(&c, Role::Cleaner), minimum(&c, Role::Researcher)), (1, 0, 0));
        c.autopilot.drivers = 7;
        assert_eq!(minimum(&c, Role::Driver), 7);
        // (no ticket inspectors before they are researched)
        assert_eq!(minimum(&c, Role::Inspector), 0);
    }

    #[test]
    fn a_deed_is_noted_once_a_day() {
        let (mut c, _) = company(Mode::Easy, Difficulty::Realistic, 0, 0);
        note(&mut c, "2024-03-04", ROSTER, vec![("d", "3".into()), ("b", "1".into())]);
        note(&mut c, "2024-03-04", ROSTER, vec![("d", "4".into()), ("b", "2".into())]);
        note(&mut c, "2024-03-04", HIRED, vec![("name", "A".into()), ("role", "Mechanic".into())]);
        note(&mut c, "2024-03-04", HIRED, vec![("name", "B".into()), ("role", "Mechanic".into())]);
        note(&mut c, "2024-03-05", ROSTER, vec![("d", "4".into()), ("b", "2".into())]);
        assert_eq!(c.autopilot.log.len(), 4);
        assert_eq!(deeds_of(&c, "2024-03-04").find(|d| d.text == ROSTER).unwrap().args[0].1, "4");
        for k in 0..LOG_KEPT {
            note(&mut c, "2024-03-06", HIRED, vec![("name", k.to_string())]);
        }
        assert_eq!(c.autopilot.log.len(), LOG_KEPT);
    }
}
