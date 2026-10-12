//! The bus company's pages (the rules are `omsi_launcher_lib::company`'s): founding a
//! company, its overview, its fleet and the vehicle market, its staff and the labour market,
//! its lines and how today's tours are covered, its finances, and its clock: the company's
//! time simulated by the hour or the day ("Simulate to tomorrow"), with the days' reports. Reached from the start's fifth tile, as Omsi-Hub's bus company is from the
//! tile beside its ways to drive.
//!
//! Calm, as Omsi-Hub's company pages are: sections with a hairline, figures in big type,
//! nothing moves but what is under the mouse. What takes time - reading the buses for the
//! market, the timetable of the company's day, closing a day - runs on a thread of its own.

mod adverts;
mod bank;
mod career;
mod chooser;
mod clock;
mod concessions;
mod contracts;
mod dealer;
mod depot;
mod easy_home;
mod fares;
mod fleet;
mod home;
mod inbox;
mod kit;
mod lines;
mod map;
mod money;
mod overview;
mod people;
mod planning;
mod repair_game;
mod research;
mod settings;
mod signing;
pub(super) mod tutorial;
mod wizard;
mod workshop;

use super::theme::*;
use super::ui::{ButtonKind, Input, Key, Ui};
use super::{Launcher, Page};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::day::{DayReport, Note, Plan};
use omsi_launcher_lib::company::market::MarketBus;
use omsi_launcher_lib::company::{self as co, Cents, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::sync::mpsc::{channel, Receiver, Sender};

enum Msg {
    Companies(String, Vec<Company>),
    Market(usize, Vec<MarketBus>),
    /// The timetable of a day, read at a revision of it (`CompanyView::timetable`).
    Lines { map: String, date: String, revision: u64, result: Result<Vec<core::LineInfo>, String> },
    Own(String, Vec<core::lines::OwnLine>),
    /// A simulation past midnight came back (`clock::simulate`).
    Simulated(Result<(Company, co::clock::Run), String>),
}

/// The timetable of the company's day.
pub(super) struct Today {
    map: String,
    date: String,
    lines: Vec<core::LineInfo>,
    error: Option<String>,
}

pub(super) const TABS: [&str; 14] = ["Overview", "Fleet", "Staff", "Lines", "Finances", "Planning", "Career", "Depot", "Concessions", "Map", "Research", "Workshop", "Contracts", "Inbox"];
/// The tabs of the depot, the concession market and the fleet map (see `TABS`).
pub(super) const DEPOT_TAB: usize = 7;
pub(super) const CONCESSIONS_TAB: usize = 8;
pub(super) const MAP_TAB: usize = 9;
/// The research, the workshop's stores, the contracts and the inbox.
pub(super) const RESEARCH_TAB: usize = 10;
pub(super) const WORKSHOP_TAB: usize = 11;
pub(super) const CONTRACTS_TAB: usize = 12;
pub(super) const INBOX_TAB: usize = 13;
/// The company's start screen (`home`): no page of `TABS` but where the company opens, with a
/// tile for every page; the strip under the bar and Escape lead back to it.
pub(super) const HOME_TAB: usize = 100;
/// The pages' icons, in the order of `TABS`.
pub(super) const ICONS: [&str; 14] = ["signal_cellular_alt", "directions_bus", "groups", "route", "payments", "calendar_month", "military_tech", "garage", "public", "map", "search", "construction", "description", "chat"];
/// The pages in the strip under the bar (`nav`), grouped as the start screen groups them: at a
/// glance, operations, people, business, the company.
const RAIL: [&[usize]; 5] = [&[0, INBOX_TAB], &[5, 3, MAP_TAB, 1], &[2], &[4, CONCESSIONS_TAB, CONTRACTS_TAB, 6], &[DEPOT_TAB, WORKSHOP_TAB, RESEARCH_TAB]];
/// The easy mode's pages (Luc: "a somewhat different layout per mode; advanced has all the
/// functions"): what a player who runs buses without every detail looks at - the overview and
/// the inbox, the lines, the map and the fleet, the staff, the money and the career with the
/// player's own duties. The planning, the depot, the concessions, the research, the workshop
/// and the contracts run by themselves (`co::autopilot`) and keep their pages for the advanced
/// mode, where nothing of them is lost.
const EASY_RAIL: [&[usize]; 4] = [&[0, INBOX_TAB], &[3, MAP_TAB, 1], &[2], &[4, 6]];

/// The strip's pages of a mode, grouped.
fn rail(mode: co::Mode) -> &'static [&'static [usize]] {
    match mode {
        co::Mode::Easy => &EASY_RAIL,
        co::Mode::Advanced => &RAIL,
    }
}

/// The pages a mode shows, in the strip's order.
pub(super) fn pages_of(mode: co::Mode) -> Vec<usize> {
    rail(mode).iter().flat_map(|g| g.iter().copied()).collect()
}

/// Whether the open company is run in the easy mode.
pub(super) fn easy(l: &Launcher) -> bool {
    l.company.company.as_ref().is_some_and(co::autopilot::on)
}

/// The page an `OMSI_LAUNCHER_PAGE` of "bus company:<part>" opens: the start screen for
/// "home" or "start", else the page of that number (the last one past them).
/// The bus company entered from elsewhere: its start opens, the play mode first (`chooser`).
pub(super) fn begin(l: &mut Launcher) {
    chooser::choose(l);
}

pub(super) fn tab_of(part: &str) -> Option<usize> {
    let p = part.trim();
    if p.eq_ignore_ascii_case("home") || p.eq_ignore_ascii_case("start") {
        return Some(HOME_TAB);
    }
    p.parse::<usize>().ok().map(|n| n.min(TABS.len() - 1))
}

/// A page's name as the strip under the bar says it (the start screen's own).
fn page_name(tab: usize) -> &'static str {
    TABS.get(tab).copied().unwrap_or("Start screen")
}

/// Whether Escape goes back to the start screen this frame: on a page (not the start screen
/// itself), with nothing lying over it, no text field typed in, and nothing the page lets go
/// of first (a driver or a duty in hand on the planning).
fn escape_home(tab: usize, escape: bool, modal: bool, typing: bool, holding: bool) -> bool {
    escape && tab != HOME_TAB && !modal && !typing && !holding
}

/// A dialog over the company's pages.
pub(super) enum Dialog {
    /// A bus of the dealer leased (0) or rented (1).
    New { bus: MarketBus, how: usize, days: f32, livery: usize },
    /// One of the dealer's sheets (`dealer::Sheet`: a bus, an offer, a talk, a contract).
    Dealer,
    /// A bus of the fleet: its livery, a service, selling or giving it back.
    Vehicle { id: u32, livery: usize },
    /// Something that cannot be undone.
    Confirm { what: Confirm },
    /// The company's time: every step, the clock's speed, the dispatcher.
    Time,
    /// A line's fare: its single ticket within its band, and what it does to passengers and
    /// fares (`fares`).
    Fare { line: String, fare: f32 },
    /// Taking a line on (or applying for it): what it needs against what the company has.
    AddLine { name: String },
    /// Putting a line into service from a moment on (`planning`).
    Service { line: String, when: usize, date: String, gaps: bool },
    /// The company's settings: its date (`settings`).
    Settings { date: String },
}

#[derive(Clone, Debug)]
pub(super) enum Confirm {
    Sell(u32),
    Dismiss(u32),
    RemoveLine(String),
    /// The company (by its id) deleted for good.
    DeleteCompany(String),
}

pub struct CompanyView {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// The driver's companies (None: not read yet), and for which driver.
    companies: Option<Vec<Company>>,
    companies_for: Option<String>,
    pub(super) company: Option<Company>,
    pub(super) tab: usize,
    /// The start screen's category (its section) shown (`home::draw`).
    pub(super) home_cat: usize,
    /// The bus company's start while it is open: the play mode, then the company to open
    /// (`chooser`).
    pub(super) chooser: Option<chooser::Chooser>,
    pub(super) wizard: Option<wizard::Wizard>,
    /// The market's buses with their kinds, for how many installed buses, and how far the
    /// reading is.
    pub(super) market: Option<Vec<MarketBus>>,
    market_for: Option<usize>,
    market_busy: bool,
    pub(super) today: Option<Today>,
    today_asked: Option<(String, String)>,
    pub(super) own: Vec<core::lines::OwnLine>,
    own_for: Option<String>,
    /// Today's plan (None: to be made again).
    pub(super) plan: Option<Plan>,
    closing: bool,
    pub(super) reports: Option<Vec<DayReport>>,
    pub(super) dialog: Option<Dialog>,
    pub(super) fleet: fleet::FleetView,
    pub(super) people: people::PeopleView,
    pub(super) lines: lines::LinesView,
    pub(super) money: money::MoneyView,
    pub(super) planning: planning::PlanningView,
    /// Raised whenever the company changed (the planning keeps its plans until then).
    pub(super) generation: u64,
    pub(super) career: career::CareerView,
    pub(super) depot: depot::DepotView,
    pub(super) tenders: concessions::TendersView,
    pub(super) map: map::FleetMap,
    pub(super) clock: clock::ClockView,
    /// The feed shows the ordinary run of the day too.
    pub(super) feed_minor: bool,
    /// Something cannot be done: said over everything (`kit::draw_popup`).
    popup: Option<kit::Popup>,
    /// Raised whenever the map's timetable was written (the line editor): every page reads it
    /// again (`reload_timetable`).
    pub(super) timetable: u64,
    /// The day report's line opened to its money.
    report_line: Option<String>,
    /// For whom the company's tour was looked at this session (`tutorial::frame`).
    tutorial_for: Option<String>,
    /// Lines whose changed timetable took effect: their tours planned anew once the
    /// timetable is read again (`refill`).
    refill: Vec<String>,
    /// Changes of own lines whose timetable could not be written (the company, the line
    /// editor's id, the company's day): they keep waiting, tried again the next day or when
    /// the timetable is written (`changes_due`).
    unwritten: Vec<(String, u64, String)>,
}

impl Default for CompanyView {
    fn default() -> Self {
        let (tx, rx) = channel();
        CompanyView {
            tx,
            rx,
            companies: None,
            companies_for: None,
            company: None,
            tab: HOME_TAB,
            home_cat: 0,
            chooser: None,
            wizard: None,
            market: None,
            market_for: None,
            market_busy: false,
            today: None,
            today_asked: None,
            own: Vec::new(),
            own_for: None,
            plan: None,
            closing: false,
            reports: None,
            dialog: None,
            fleet: Default::default(),
            people: Default::default(),
            lines: Default::default(),
            money: Default::default(),
            planning: Default::default(),
            generation: 0,
            career: Default::default(),
            depot: Default::default(),
            tenders: Default::default(),
            map: Default::default(),
            clock: Default::default(),
            feed_minor: false,
            popup: None,
            timetable: 0,
            report_line: None,
            tutorial_for: None,
            refill: Vec::new(),
            unwritten: Vec::new(),
        }
    }
}

fn spawn(tx: &Sender<Msg>, f: impl FnOnce() -> Msg + Send + 'static) {
    let tx = tx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
}

/// The data folder (`~/.openomsi`).
fn data() -> std::path::PathBuf {
    core::data_dir()
}

// --- money and dates as the pages show them -------------------------------------------------

/// How the interface's language writes a number: its thousands separator, its decimal mark,
/// and where the euro sign goes. Every amount, kilometre and count of the company's pages
/// (the dealer, the contracts, the bank, the concessions, the depot, the phone's tab) is
/// written through `eur`, `eur_cents`, `grouped` and `num`, which ask this.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NumberStyle {
    thousands: char,
    decimal: char,
    /// "€17,968" (en), "€ 17.968" (nl) or "17.968 €".
    euro: EuroAt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EuroAt {
    Before,
    BeforeSpaced,
    After,
}

/// The narrow no-break space French, Russian, Ukrainian and Polish group thousands with.
const NARROW: char = '\u{202F}';

fn style(lang: &str) -> NumberStyle {
    let (thousands, decimal, euro) = match lang {
        "" | "en" => (',', '.', EuroAt::Before),
        "nl" => ('.', ',', EuroAt::BeforeSpaced),
        "de" | "it" | "es" | "pt" | "pt-pt" | "tr" | "da" | "id" => ('.', ',', EuroAt::After),
        _ => (NARROW, ',', EuroAt::After),
    };
    NumberStyle { thousands, decimal, euro }
}

/// `x` rounded to `places` decimals, its thousands grouped, as `lang` writes it.
fn num_in(x: f64, places: usize, lang: &str) -> String {
    let st = style(lang);
    let x = if x.is_finite() { x } else { 0.0 };
    let text = format!("{:.*}", places, x.abs());
    let (whole, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut out = String::new();
    // (-0 is 0)
    if x < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(st.thousands);
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push(st.decimal);
        out.push_str(frac);
    }
    out
}

/// An amount with its euro sign where `lang` puts it (the minus before all).
fn with_euro(n: String, lang: &str) -> String {
    let (sign, n) = match n.strip_prefix('-') {
        Some(rest) => ("-", rest.to_string()),
        None => ("", n),
    };
    match style(lang).euro {
        EuroAt::Before => format!("{sign}€{n}"),
        EuroAt::BeforeSpaced => format!("{sign}€ {n}"),
        EuroAt::After => format!("{sign}{n} €"),
    }
}

fn eur_in(c: Cents, lang: &str) -> String {
    with_euro(num_in((c as f64 / 100.0).round(), 0, lang), lang)
}

fn eur_cents_in(c: f64, lang: &str) -> String {
    with_euro(num_in(c / 100.0, 2, lang), lang)
}

/// Whole euros, grouped as the language groups them ("€1,234,567", "€ 1.234.567",
/// "1.234.567 €", "1 234 567 €").
pub(super) fn eur(c: Cents) -> String {
    eur_in(c, &omsi_ui::i18n::language())
}

/// Euros with their cents (a fare, a price per kilometre): "€17,968.50", "€ 17.968,50".
pub(super) fn eur_cents(c: f64) -> String {
    eur_cents_in(c, &omsi_ui::i18n::language())
}

/// A whole number grouped like money (kilometres, passengers).
pub(super) fn grouped(n: f64) -> String {
    num_in(n.round(), 0, &omsi_ui::i18n::language())
}

/// A number with `places` decimals as the language writes it ("4.5", "4,5").
pub(super) fn num(x: f64, places: usize) -> String {
    num_in(x, places, &omsi_ui::i18n::language())
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Tue 5 Mar 1989".
pub(super) fn day_label(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (y, m, day) = co::dates::civil_from_days(d);
    format!("{} {} {} {}", omsi_ui::tr(WEEKDAYS[co::dates::weekday(d) as usize]), day, omsi_ui::tr(MONTHS[(m as usize).clamp(1, 12) - 1]), y)
}

/// "Mar 1989" of a `YYYY-MM`.
pub(super) fn month_label(month: &str) -> String {
    let y = month.get(..4).unwrap_or("");
    let m: usize = month.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    format!("{} {}", omsi_ui::tr(MONTHS[m.clamp(1, 12) - 1]), y)
}

// --- shared pieces ----------------------------------------------------------------------------

/// A section: its hairline box and its name in capitals; returns the room inside.
pub(super) fn section(ui: &mut Ui, r: Rect, title: &str) -> Rect {
    ui.card(r);
    kit::caps(ui, Rect::new(r.x + 18.0, r.y + 14.0, r.w - 36.0, 16.0), title);
    Rect::new(r.x + 18.0, r.y + 44.0, r.w - 36.0, (r.h - 58.0).max(0.0))
}

/// A figure: its name, the value in big type and a line under it (`kit::FIGURE_H` high).
pub(super) fn figure(ui: &mut Ui, r: Rect, label: &str, value: &str, under: &str, c: Color) {
    ui.card(r);
    kit::caps(ui, Rect::new(r.x + 16.0, r.y + 13.0, r.w - 32.0, 16.0), label);
    let px = if r.w < 170.0 { 22.0 } else { 27.0 };
    ui.text_in(value, Rect::new(r.x + 16.0, r.y + 34.0, r.w - 32.0, 34.0), px, Weight::Bold, c, Align::Left);
    if !under.is_empty() {
        ui.text_in(under, Rect::new(r.x + 16.0, r.y + 70.0, r.w - 32.0, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    }
}

/// A thin bar of 0..1.
pub(super) fn meter(ui: &mut Ui, r: Rect, frac: f64, c: Color) {
    ui.p().rounded(r, r.h * 0.5, HAIRLINE);
    let w = (r.w * frac.clamp(0.0, 1.0) as f32).max(r.h);
    ui.p().rounded(Rect::new(r.x, r.y, w, r.h), r.h * 0.5, c);
}

/// The colour of a 0-100 condition or satisfaction.
pub(super) fn grade(v: f64) -> Color {
    if v >= 65.0 {
        OK
    } else if v >= 40.0 {
        WARN
    } else {
        DANGER
    }
}

/// The company's mark: its short name on its colours.
pub(super) fn monogram(ui: &mut Ui, r: Rect, c: &Company) {
    let main = super::ownlines::colour_of(&c.colours[0]);
    let second = super::ownlines::colour_of(&c.colours[1]);
    // (the company's own colours and their ink, as they are in either look)
    ui.as_drawn();
    ui.p().rounded(r, RADIUS, main);
    ui.p().rounded(Rect::new(r.x, r.bottom() - r.h * 0.22, r.w, r.h * 0.22), RADIUS * 0.5, second);
    ui.p().rect(Rect::new(r.x, r.bottom() - r.h * 0.22, r.w, r.h * 0.11), second);
    ui.text_in(&c.short, Rect::new(r.x, r.y, r.w, r.h * 0.8), (r.h * 0.34).min(18.0), Weight::Black, super::ownlines::ink_on(main), Align::Center);
    ui.end_as_drawn();
}

/// The company's mark: its logo picture when it has one (fitted into the square, on white),
/// else its monogram. (The picture was chosen and kept, but only the monogram was drawn:
/// Luc saw nothing change.)
pub(super) fn company_mark(l: &mut Launcher, r: Rect, c: &Company) {
    match c.logo.as_deref().and_then(|p| logo_texture(l, p)) {
        Some(tex) => {
            // (on white in either look: the light one would make the white its dark ink)
            l.ui.swatch(r, RADIUS, Color::WHITE);
            l.ui.image(r.inset(3.0), tex, (RADIUS - 2.0).max(0.0));
        }
        None => monogram(&mut l.ui, r, c),
    }
}

thread_local! {
    /// Logo pictures that could not be read (not tried again every frame).
    static LOGO_UNREAD: std::cell::RefCell<std::collections::HashSet<String>> = Default::default();
}

/// The texture of the logo picture at `path`: read the first time it is asked for - at most
/// 256 pixels, square, the picture in its middle on a clear ground - and uploaded with the
/// launcher's other pictures (`Launcher::icons`) a frame later. None until then, and for a
/// file that cannot be read.
fn logo_texture(l: &mut Launcher, path: &str) -> Option<usize> {
    let key = format!("logo:{path}");
    if let Some(t) = l.icons.get(&key) {
        return Some(*t);
    }
    if l.icons_pending.iter().any(|p| p.0 == key) || LOGO_UNREAD.with(|u| u.borrow().contains(&key)) {
        return None;
    }
    match image::open(path) {
        Ok(img) => {
            let img = img.thumbnail(256, 256).to_rgba8();
            let side = img.width().max(img.height()).max(1);
            let mut square = image::RgbaImage::new(side, side);
            image::imageops::overlay(&mut square, &img, ((side - img.width()) / 2) as i64, ((side - img.height()) / 2) as i64);
            l.icons_pending.push((key, square));
        }
        Err(_) => LOGO_UNREAD.with(|u| {
            u.borrow_mut().insert(key);
        }),
    }
    None
}

/// A plain text line, cut to fit.
pub(super) fn line(ui: &mut Ui, r: Rect, text: &str, px: f32, c: Color) {
    ui.text_in(text, r, px, Weight::Regular, c, Align::Left);
}

/// A mode's icon: the easy mode's company that runs by itself, the advanced mode's controls.
pub(super) fn mode_icon(mode: co::Mode) -> &'static str {
    match mode {
        co::Mode::Easy => "autorenew",
        co::Mode::Advanced => "tune",
    }
}

/// How wide a mode's pill is.
pub(super) fn mode_pill_w(ui: &Ui, mode: co::Mode) -> f32 {
    ui.width(&omsi_ui::tr(mode.label()), 12.0, Weight::Bold) + 40.0
}

/// A company's mode as a small pill (its icon and name; the easy mode in the accent), which a
/// `click` one answers under the mouse. Returns whether it was clicked.
pub(super) fn mode_pill(ui: &mut Ui, id: &str, r: Rect, mode: co::Mode, click: bool) -> bool {
    let (hover, clicked) = if click {
        let (h, _, c) = ui.interact(super::ui::id_of(id), r);
        (h, c)
    } else {
        (false, false)
    };
    let easy = mode == co::Mode::Easy;
    let (fill, ink) = if easy { (accent().alpha(if hover { 0.28 } else { 0.18 }), accent_2()) } else { (if hover { HOVER } else { FIELD }, TEXT_SOFT) };
    ui.p().rounded(r, r.h * 0.5, fill);
    if !easy {
        ui.p().rounded_border(r, r.h * 0.5, 1.0, HAIRLINE);
    }
    ui.icon(mode_icon(mode), Vec2::new(r.x + 14.0, r.center().y), 13.0, ink);
    ui.text_in(mode.label(), Rect::new(r.x + 25.0, r.y, r.w - 30.0, r.h), 12.0, Weight::Bold, ink, Align::Left);
    if click {
        ui.tooltip(r, "How much of the company you run yourself: change it in the company's settings");
    }
    clicked
}

// --- the page --------------------------------------------------------------------------------

/// What the page asks for in the background, and what came.
fn work(l: &mut Launcher) {
    let profile = l.state.config.profile.clone();
    let view = &mut l.company;
    while let Ok(m) = view.rx.try_recv() {
        match m {
            Msg::Companies(p, list) => {
                if p == profile {
                    // (the one open stays open; else the first)
                    let keep = view.company.as_ref().map(|c| c.id.clone());
                    view.company = keep.and_then(|id| list.iter().find(|c| c.id == id).cloned()).or_else(|| list.first().cloned());
                    view.companies = Some(list);
                    view.plan = None;
                }
            }
            Msg::Market(n, list) => {
                view.market = Some(list);
                view.market_for = Some(n);
                view.market_busy = false;
            }
            Msg::Lines { map, date, revision, result } => {
                // (read before the timetable was written again: the old one, or half of the
                // new - the read asked for since is on its way)
                if revision != view.timetable {
                    continue;
                }
                let (lines, error) = match result {
                    Ok(l) => (l, None),
                    Err(e) => (Vec::new(), Some(e)),
                };
                view.today = Some(Today { map, date, lines, error });
                view.plan = None;
            }
            Msg::Own(map, own) => {
                if view.own_for.as_deref() == Some(map.as_str()) {
                    view.own = own;
                }
            }
            Msg::Simulated(r) => {
                view.closing = false;
                match r {
                    Ok((c, run)) => {
                        if let Some(list) = view.companies.as_mut() {
                            if let Some(x) = list.iter_mut().find(|x| x.id == c.id) {
                                *x = c.clone();
                            }
                        }
                        view.company = Some(c);
                        if !run.reports.is_empty() {
                            view.reports = Some(run.reports);
                        }
                        view.plan = None;
                    }
                    Err(e) => {
                        view.clock.speed = 0.0;
                        view.clock.hold();
                        l.state.set_status(e, true);
                    }
                }
            }
        }
    }
    let view = &mut l.company;
    if view.companies_for.as_deref() != Some(profile.as_str()) {
        view.companies_for = Some(profile.clone());
        view.companies = None;
        view.company = None;
        let p = profile.clone();
        spawn(&view.tx, move || Msg::Companies(p.clone(), co::store::list(&data(), &p)));
    }
    // the timetable of the company's day, and the player's own lines of its map
    if let Some(c) = view.company.as_ref() {
        let key = (c.map.clone(), c.date.clone());
        if view.today_asked.as_ref() != Some(&key) {
            view.today_asked = Some(key.clone());
            let (map, date) = key;
            let revision = view.timetable;
            spawn(&view.tx, move || {
                let result = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"));
                Msg::Lines { map, date, revision, result }
            });
        }
        if view.own_for.as_deref() != Some(c.map.as_str()) {
            view.own_for = Some(c.map.clone());
            view.own.clear();
            let map = c.map.clone();
            spawn(&view.tx, move || Msg::Own(map.clone(), core::lines::own_lines_of_map(&map)));
        }
    }
    // a line's change waiting for its day: its day has come; and its tours planned anew
    changes_due(l);
    refill(l);
    // the easy mode's morning before today's plan is made (a line taken on runs at once, the
    // roster takes a bus bought or a driver hired): saved when it changed anything - not while
    // a simulation has the company on its thread
    if l.company.plan.is_none() && !l.company.closing && easy_morning(&mut l.company) {
        changed(l);
    }
    // today's plan (the roster's, see `planning`): every tour of the day, those of lines not in
    // service marked; and the company's day for the game - only what is in service runs on the
    // map with the company's buses while the player drives
    let view = &mut l.company;
    if view.plan.is_none() {
        view.generation += 1;
        if let (Some(c), Some(t)) = (view.company.as_ref(), view.today.as_ref()) {
            if t.map == c.map && t.date == c.date {
                let tours = co::network::tours_of_day(c, &t.lines, &c.date);
                let day = co::plan::day_plan(c, &c.date, tours, &[], &[], false);
                view.plan = Some(day.to_plan());
                if let Err(e) = co::plan::save_live_plan(&data(), &co::plan::live_plan(c, &day)) {
                    log::warn!("company: the day's plan for the game: {e:#}");
                }
            }
        }
    }
}

/// The easy mode's morning on the company's day as the page has it (`co::autopilot::morning`,
/// with the timetable read for its day). Returns whether it changed the company.
fn easy_morning(view: &mut CompanyView) -> bool {
    let (Some(c), Some(t)) = (view.company.as_mut(), view.today.as_ref()) else { return false };
    if !co::autopilot::on(c) || t.map != c.map || t.date != c.date || t.error.is_some() {
        return false;
    }
    let date = c.date.clone();
    co::autopilot::morning(c, &date, &t.lines)
}

/// A change of an own line waiting for its day (`ownline::Pending`): on that day the map's
/// timetable and the company's line take it, and the tours it changed are planned anew once
/// the timetable is read again (`refill`). A change whose timetable could not be written keeps
/// waiting (the map's timetable and the company's line stay as one).
fn changes_due(l: &mut Launcher) {
    let Some(c) = l.company.company.as_ref() else { return };
    let (company, date) = (c.id.clone(), c.date.clone());
    let unwritten = &l.company.unwritten;
    let due: Vec<u64> = co::ownline::due(c).into_iter().filter(|id| !unwritten.iter().any(|u| u.0 == company && u.1 == *id && u.2 == date)).collect();
    if due.is_empty() {
        return;
    }
    let map = c.map.clone();
    let mut failed = Vec::new();
    let mut wrote = false;
    for id in due {
        if let Err(e) = super::lineeditor::take_effect_in_timetable(l, &map, id) {
            l.state.set_status(format!("{}: {e}", omsi_ui::tr("The timetable files could not be written")), true);
            failed.push((company.clone(), id, date.clone()));
            continue;
        }
        wrote = true;
        if let Some(Some((name, _))) = act(l, |c| Ok(co::ownline::take_effect(c, id))) {
            l.company.refill.push(name);
        }
    }
    if wrote {
        reload_timetable(l);
        // (the day's timetable as it is now, before the tours are planned)
        l.company.today = None;
    }
    // (kept after `reload_timetable` let the earlier ones go: these wait for tomorrow, or for
    // the line editor to write the timetable)
    l.company.unwritten.retain(|u| u.2 == date);
    l.company.unwritten.extend(failed);
}

/// The tours of lines whose change took effect, planned anew on the company's day (as "Fill
/// the roster" does, for them only), and said.
fn refill(l: &mut Launcher) {
    if l.company.refill.is_empty() {
        return;
    }
    let (Some(c), Some(t)) = (l.company.company.as_ref(), l.company.today.as_ref()) else { return };
    // (a timetable that could not be read plans nothing: the tours wait for one that can)
    if t.map != c.map || t.date != c.date || t.error.is_some() {
        return;
    }
    let tours = co::network::tours_of_day(c, &t.lines, &c.date);
    let date = c.date.clone();
    for name in std::mem::take(&mut l.company.refill) {
        let number = l.company.company.as_ref().and_then(|c| c.lines.iter().find(|x| x.name == name)).map(|x| x.number.clone()).unwrap_or_default();
        let ours = tours.clone();
        let Some(f) = act(l, |c| Ok(co::plan::fill_line(c, &date, ours, &name))) else { continue };
        let mut text = omsi_ui::tr("Line %{n} runs its new timetable from today: %{d} duties and %{b} buses given anew.").replace("%{n}", &number).replace("%{d}", &f.duties.to_string()).replace("%{b}", &f.buses.to_string());
        let open = planning::open_text(&f);
        if !open.is_empty() {
            text = format!("{text} {open}");
        }
        l.state.set_status(text, false);
    }
    l.company.plan = None;
}

/// The market's buses (read once, again when the installed buses changed).
pub(super) fn ask_market(l: &mut Launcher) {
    let n = l.state.vehicles.len();
    let view = &mut l.company;
    if view.market_busy || view.market_for == Some(n) {
        return;
    }
    view.market_busy = true;
    view.market_for = Some(n);
    let vehicles = l.state.vehicles.clone();
    spawn(&view.tx, move || Msg::Market(vehicles.len(), co::market::market_of(&vehicles)));
}

/// The market is being read.
pub(super) fn market_busy(l: &Launcher) -> bool {
    l.company.market_busy
}

/// After a change made on a page: saved, and today's plan made again.
pub(super) fn changed(l: &mut Launcher) {
    let view = &mut l.company;
    view.plan = None;
    if let Some(c) = view.company.as_ref() {
        if let Some(list) = view.companies.as_mut() {
            match list.iter_mut().find(|x| x.id == c.id) {
                Some(x) => *x = c.clone(),
                None => list.push(c.clone()),
            }
        }
        if let Err(e) = co::store::save(&data(), c) {
            l.state.set_status(format!("{e:#}"), true);
        }
    }
}

/// The lines of the company's map in the timetable of its day (None: not read yet).
pub(super) fn map_lines(view: &CompanyView) -> Option<&[core::LineInfo]> {
    let c = view.company.as_ref()?;
    view.today.as_ref().filter(|t| t.map == c.map && t.error.is_none()).map(|t| t.lines.as_slice())
}

/// The timetable of the company's day and its map's own lines read again (the line editor
/// wrote the map's timetable) - by every page: the Lines page, the planning's week, the
/// concessions' weeks; and a change whose timetable could not be written tried again.
pub(super) fn reload_timetable(l: &mut Launcher) {
    let view = &mut l.company;
    view.today_asked = None;
    view.own_for = None;
    view.plan = None;
    view.timetable += 1;
    view.unwritten.clear();
}

/// Do something to the company; its refusal is said in a popup that says why and what opens
/// it (on another page - the line editor's - in the status line).
pub(super) fn act<T>(l: &mut Launcher, f: impl FnOnce(&mut Company) -> Result<T, &'static str>) -> Option<T> {
    let c = l.company.company.as_mut()?;
    match f(c) {
        Ok(v) => {
            changed(l);
            Some(v)
        }
        Err(e) => {
            if l.page == Page::Company {
                kit::refuse(l, e);
            } else {
                l.state.set_status(omsi_ui::tr(e).into_owned(), true);
            }
            None
        }
    }
}

/// What mends something in the easy mode: the planning, the depot and the research are the
/// company's own there (None: no button - a popup that needs one of them offers the advanced
/// mode itself, `Go::Advanced`), the staff's courses are the career's.
fn easy_go(g: kit::Go) -> Option<kit::Go> {
    use kit::Go;
    match g {
        Go::Planning | Go::Licences | Go::Depot | Go::Research => None,
        Go::Training => Some(Go::Courses),
        g => Some(g),
    }
}

/// Said under a lock the easy mode does not open by itself (a research it does not start).
pub(super) const NOT_RESEARCHED_BY_ITSELF: &str = "The easy mode does not research this by itself: switch to the advanced mode and start it on its Research page.";

/// Whether the open company's mode shows the page `tab` (the start screen always).
pub(super) fn shown(l: &Launcher, tab: usize) -> bool {
    tab == HOME_TAB || l.company.company.as_ref().is_none_or(|c| pages_of(c.mode).contains(&tab))
}

/// Where a popup's second button goes (in the easy mode, a way to the planning goes to the
/// player's own duties).
fn go(l: &mut Launcher, g: kit::Go) {
    use kit::Go;
    l.company.dialog = None;
    l.company.reports = None;
    let g = if easy(l) && g == Go::Planning {
        l.company.tab = 6;
        l.company.career.part = career::DUTIES;
        return;
    } else if easy(l) {
        // (a page the easy mode hides is not opened: the company sees to that by itself)
        match easy_go(g) {
            Some(g) => g,
            None => return,
        }
    } else {
        g
    };
    match g {
        Go::Progress => {
            l.company.tab = 6;
            l.company.career.part = career::PROGRESS;
        }
        Go::Depot => l.company.tab = DEPOT_TAB,
        Go::Loan => bank::open_loan(l),
        Go::Finances => l.company.tab = 4,
        Go::Courses => {
            l.company.tab = 6;
            l.company.career.part = career::TRAINING;
        }
        Go::Planning => l.company.tab = 5,
        Go::Lines => l.company.tab = 3,
        Go::Dealer => {
            l.company.tab = 1;
            l.company.fleet.tab = 1;
        }
        Go::Hire => {
            l.company.tab = 2;
            people::to_applicants(l);
        }
        Go::Training => people::to_courses(l),
        Go::Licences => people::to_training(l),
        Go::Research => l.company.tab = RESEARCH_TAB,
        Go::Advanced(tab) => {
            settings::set_mode(l, co::Mode::Advanced);
            l.company.tab = tab;
        }
    }
}

/// The line editor working for the company: its page under the company's popup when one is
/// up (a size of bus the level does not open yet, a change refused), the page without input.
/// The popup keeps only its OK there: a way to another page would leave the line's changes
/// behind.
pub fn over_line_editor(l: &mut Launcher, draw: impl FnOnce(&mut Launcher)) {
    if l.company.popup.is_none() {
        draw(l);
        return;
    }
    if let Some(p) = l.company.popup.as_mut() {
        p.go = None;
    }
    let i = mask(&mut l.ui);
    draw(l);
    l.ui.input = i;
    kit::draw_popup(l);
}

/// A size of bus the line editor offers the company: refused with the level's popup while
/// the company's level does not open it.
pub fn size_locked(l: &mut Launcher, size: co::BusSize) {
    let refused = l.company.company.as_ref().and_then(|c| co::market::size_allowed(c, size).err());
    if let Some(reason) = refused {
        kit::refuse(l, reason);
    }
}

/// The sizes of bus the company's level does not open yet (an articulated bus, a
/// double-decker).
pub fn sizes_locked(l: &Launcher) -> Vec<co::BusSize> {
    let Some(c) = l.company.company.as_ref() else { return Vec::new() };
    [co::BusSize::Articulated, co::BusSize::Double].into_iter().filter(|s| co::market::size_allowed(c, *s).is_err()).collect()
}

/// The input taken away (a dialog or a popup lies over what is drawn now); returns it.
pub(super) fn mask(ui: &mut Ui) -> Input {
    let i = ui.input.clone();
    ui.input.mouse = Vec2::new(-1e4, -1e4);
    ui.input.pressed = false;
    ui.input.released = false;
    ui.input.wheel = Vec2::ZERO;
    ui.input.keys.clear();
    ui.input.text.clear();
    i
}

/// Something lies over the pages: a dialog, a report, the clock's question, a popup.
fn modal(l: &Launcher) -> bool {
    let asking = l.company.company.as_ref().is_some_and(|c| c.clock.ask.is_some());
    l.company.dialog.is_some() || l.company.reports.is_some() || l.company.closing || asking || l.company.popup.is_some() || l.company.clock.away.is_some()
}

/// What lies over the pages, drawn last: the dialog (with no input while a popup is over it),
/// then the popup.
fn overlays(l: &mut Launcher) {
    let asking = l.company.company.as_ref().is_some_and(|c| c.clock.ask.is_some());
    let under = if l.company.popup.is_some() { Some(mask(&mut l.ui)) } else { None };
    if l.company.closing {
        closing_cover(l);
    } else if l.company.clock.away.is_some() {
        clock::away_dialog(l);
    } else if l.company.reports.is_some() {
        report_dialog(l);
    } else if asking {
        clock::ask_dialog(l);
    } else {
        dialog(l);
    }
    if let Some(i) = under {
        l.ui.input = i;
        kit::draw_popup(l);
    }
}

/// The company's page on a phone (in the phone's frame): the founding wizard, or the company
/// with its strip and tabs.
pub fn draw(l: &mut Launcher, area: Rect) {
    l.ui.widget_px = kit::CONTROL;
    work(l);
    depot::tick(l);
    dealer::tick(l);
    if l.company.companies.is_none() {
        l.ui.text_in("Reading your companies…", Rect::new(area.x, area.y, area.w, 30.0), kit::BODY, Weight::Medium, TEXT_DIM, Align::Left);
        return;
    }
    if l.company.chooser.is_some() && l.company.wizard.is_none() {
        chooser::draw(l, area);
        return;
    }
    if l.company.wizard.is_some() || l.company.company.is_none() {
        if l.company.wizard.is_none() {
            l.company.wizard = Some(wizard::Wizard::new(l));
        }
        wizard::draw(l, area);
        kit::draw_popup(l);
        return;
    }
    let modal = modal(l);
    clock::tick(l, modal);
    let saved = modal.then(|| mask(&mut l.ui));
    let esc = escape_home(l.company.tab, l.ui.input.keys.contains(&Key::Escape), modal, l.ui.focus.is_some(), l.company.planning.holds_choice());
    let body = strip(l, area);
    pages(l, body);
    back_home(l, esc);
    if let Some(i) = saved {
        l.ui.input = i;
        overlays(l);
    }
}

/// The company's screen on a desktop: the whole window its own, as an application's - its
/// bar across the top (the company, its clock and time controls, its settings, the way back),
/// the tabs under it and the pages filling the rest edge to edge.
pub fn screen(l: &mut Launcher) {
    l.ui.widget_px = kit::CONTROL;
    work(l);
    depot::tick(l);
    dealer::tick(l);
    let size = l.ui.size;
    let full = Rect::new(0.0, 0.0, size.x, size.y);
    l.ui.solid(full);
    l.ui.p().rect(full, GROUND);
    // (the start - the play mode, the company - before the company's pages; the founding
    // started from it comes back to it when cancelled)
    let choosing = l.company.companies.is_some() && l.company.chooser.is_some() && l.company.wizard.is_none();
    let founding = l.company.companies.is_some() && !choosing && (l.company.wizard.is_some() || l.company.company.is_none());
    let modal = !founding && !choosing && l.company.companies.is_some() && modal(l);
    if l.company.companies.is_some() && !founding && !choosing {
        clock::tick(l, modal);
    }
    let saved = (modal || (founding && l.company.popup.is_some())).then(|| mask(&mut l.ui));
    // the bar
    let bar = Rect::new(0.0, 0.0, size.x, 72.0);
    l.ui.p().rect(bar, PANEL);
    l.ui.p().rect(Rect::new(0.0, bar.bottom() - 1.0, size.x, 1.0), EDGE);
    let m = 24.0;
    let back = Rect::new(size.x - m - 112.0, 16.0, 112.0, 40.0);
    if l.ui.button("company-back", back, "Back", Some("chevron_left"), ButtonKind::Normal) {
        leave(l);
        l.go(Page::Drive);
    }
    let mut right = back.x - 12.0;
    // the "?": the company's tour, from where it was left (see `tutorial`)
    if l.company.companies.is_some() && !choosing {
        let help = Rect::new(right - 40.0, 16.0, 40.0, 40.0);
        super::tour::anchor("company-help", help);
        if l.ui.button("company-help", help, "", Some("help"), ButtonKind::Normal) {
            tutorial::ask(l);
        }
        l.ui.tooltip(help, "A tour of the bus company");
        right = help.x - 8.0;
    }
    // the sun and moon: the light or the dark look (`theme_switch`)
    right = super::theme_switch::company_button(l, right);
    let company = l.company.company.clone().filter(|_| !founding && !choosing);
    if company.is_some() {
        let gear = Rect::new(right - 40.0, 16.0, 40.0, 40.0);
        if l.ui.button("company-settings", gear, "", Some("settings"), ButtonKind::Normal) {
            let date = l.company.company.as_ref().map(|c| c.date.clone()).unwrap_or_default();
            l.company.dialog = Some(Dialog::Settings { date });
        }
        l.ui.tooltip(gear, "The company's settings: its name, its date, how it buys");
        right = gear.x - 16.0;
        let clock_r = Rect::new(size.x * 0.36, 16.0, (right - size.x * 0.36).max(0.0), 40.0);
        let left = clock::head(l, clock_r);
        super::tour::anchor("company-clock", Rect::new(left, clock_r.y, (clock_r.right() - left).max(0.0), clock_r.h));
        right = left - 16.0;
    }
    match &company {
        Some(c) => {
            // the company's mark and name, and under the name what it has: its cash, its
            // trophies and its reputation (City Bus Manager's top bar; where it is at home is
            // said in the strip under the bar on the start screen, and on the mark)
            let mark = Rect::new(m, 14.0, 44.0, 44.0);
            company_mark(l, mark, c);
            let tw = (right - mark.right() - 16.0).max(60.0);
            // (the name on one line, its mode after it - a click opens the settings, where it
            // is changed - and the pill of cash, trophies and reputation beside them, Luc's style
            // for the top bar)
            let pw = mode_pill_w(&l.ui, c.mode);
            let name_w = (l.ui.width(&c.name, 19.0, Weight::Bold) + 4.0).min((tw * 0.36).max(40.0));
            let name_r = Rect::new(mark.right() + 14.0, 23.0, name_w, 26.0);
            l.ui.text_in(&c.name, name_r, 19.0, Weight::Bold, TEXT, Align::Left);
            l.ui.tooltip(Rect::new(mark.x, mark.y, name_r.right() - mark.x, mark.h), &home_of(c));
            let pill = Rect::new(name_r.right() + 10.0, 25.0, pw, 22.0);
            let mut px = name_r.right() + 20.0;
            if pill.right() <= right {
                super::tour::anchor("company-mode", pill);
                if mode_pill(&mut l.ui, "company-mode-pill", pill, c.mode, true) {
                    let date = c.date.clone();
                    l.company.dialog = Some(Dialog::Settings { date });
                }
                px = pill.right() + 16.0;
            }
            status(l, Rect::new(px, 16.0, (right - px).max(0.0), 40.0), c);
        }
        None => {
            l.ui.icon("garage", Vec2::new(m + 14.0, 36.0), 26.0, accent_2());
            l.ui.text_in("Bus company", Rect::new(m + 40.0, 12.0, right - m - 40.0, 26.0), 19.0, Weight::Bold, TEXT, Align::Left);
            l.ui.text_in("Your own transport company: buses, people and lines, in a time of its own.", Rect::new(m + 40.0, 38.0, right - m - 40.0, 20.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
        }
    }
    let content = Rect::new(m, bar.bottom() + 16.0, size.x - 2.0 * m, (size.y - bar.bottom() - 16.0 - 40.0).max(100.0));
    if l.company.companies.is_none() {
        l.ui.text_in("Reading your companies…", Rect::new(content.x, content.y, content.w, 30.0), kit::BODY, Weight::Medium, TEXT_DIM, Align::Left);
    } else if choosing {
        chooser::draw(l, content);
    } else if founding {
        if l.company.wizard.is_none() {
            l.company.wizard = Some(wizard::Wizard::new(l));
        }
        super::tour::anchor("company-wizard", content);
        wizard::draw(l, content);
    } else if let Some(c) = company.as_ref() {
        // the way round across (the start screen and the page one is on, every page a click
        // away), the page under it; Escape goes back to the start screen
        let esc = escape_home(l.company.tab, l.ui.input.keys.contains(&Key::Escape), modal, l.ui.focus.is_some(), l.company.planning.holds_choice());
        nav(l, Rect::new(content.x, content.y, content.w, 42.0), c);
        let body = Rect::new(content.x, content.y + 58.0, content.w, (content.h - 58.0).max(0.0));
        super::tour::anchor("company-page", body);
        pages(l, body);
        back_home(l, esc);
    }
    // the first time: the company's welcome and tour (`tutorial`) - once a company is open
    if saved.is_none() && !choosing {
        tutorial::frame(l);
    }
    if let Some(i) = saved {
        l.ui.input = i;
        if founding {
            kit::draw_popup(l);
        } else {
            overlays(l);
        }
    }
}

/// The tab's page (the start screen's tiles for `HOME_TAB`).
fn pages(l: &mut Launcher, body: Rect) {
    // (RosterFlow plays its opening when the planning is opened again)
    if l.company.tab != 5 {
        l.company.planning.away();
    }
    match l.company.tab {
        HOME_TAB => home::draw(l, body),
        0 => overview::draw(l, body),
        1 => fleet::draw(l, body),
        2 => people::draw(l, body),
        3 => lines::draw(l, body),
        4 => money::draw(l, body),
        5 => planning::draw(l, body),
        6 => career::draw(l, body),
        DEPOT_TAB => depot::draw(l, body),
        CONCESSIONS_TAB => concessions::draw(l, body),
        MAP_TAB => map::draw(l, body),
        RESEARCH_TAB => research::draw(l, body),
        WORKSHOP_TAB => workshop::draw(l, body),
        CONTRACTS_TAB => contracts::draw(l, body),
        INBOX_TAB => inbox::draw(l, body),
        // (a page that is no more: the start screen)
        _ => home::draw(l, body),
    }
}

/// Escape was pressed on a page (`escape_home`, decided before the page was drawn): back to
/// the start screen - unless the page put something over itself meanwhile.
fn back_home(l: &mut Launcher, esc: bool) {
    if esc && !modal(l) {
        l.company.tab = HOME_TAB;
    }
}

/// Where the company is at home: its map, its depot and how hard its economy is.
fn home_of(c: &Company) -> String {
    let map = if c.map_name.is_empty() { super::state::short_map(&c.map) } else { c.map_name.clone() };
    format!("{}  ·  {}  ·  {}", map, c.depot, omsi_ui::tr(c.difficulty.label()))
}

/// What the company has, under its name: its cash, its trophies and its reputation, each a
/// click from the page about it (the finances, the research the trophies pay for, the
/// overview). Returns where it ends.
fn status(l: &mut Launcher, r: Rect, c: &Company) -> f32 {
    // (each value after its icon on a round mark of its colour: the cash green, the trophies
    // gold, the reputation blue - in one pill with a light edge, Luc's style)
    let items: [(&str, String, Color, Color, &str, usize); 3] = [
        ("payments", eur(c.cash), OK, if c.cash >= 0 { TEXT } else { DANGER.lighten(0.2) }, "Cash: the money the company has now", 4),
        // (the easy mode spends them by itself, and its research is the advanced mode's page:
        // no click there)
        ("emoji_events", grouped(c.trophies as f64), WARN, TEXT, if co::autopilot::on(c) { "Trophies: earned with goals, spent by the company on research by itself" } else { "Trophies: earned with goals and contracts, spent on research" }, RESEARCH_TAB),
        ("star", format!("{} %", num(c.reputation.clamp(0.0, 100.0), 1)), EARLY_SOFT, TEXT, "Reputation: what passengers and the authority think of the company", 0),
    ];
    let h = r.h;
    let big = h >= 34.0;
    let (px, d) = (if big { 14.5 } else { 12.5 }, (h - if big { 12.0 } else { 6.0 }).max(14.0));
    let (pad, gap) = (if big { 12.0 } else { 6.0 }, if big { 22.0 } else { 12.0 });
    let widths: Vec<f32> = items.iter().map(|it| d + 8.0 + l.ui.width(&it.1, px, Weight::Bold)).collect();
    let total = |n: usize| 2.0 * pad + widths[..n].iter().sum::<f32>() + gap * (n.max(1) - 1) as f32;
    // (as many as the room holds)
    let mut n = items.len();
    while n > 0 && total(n) > r.w + 0.5 {
        n -= 1;
    }
    if n == 0 {
        return r.x;
    }
    let pill = Rect::new(r.x, r.y, total(n), h);
    super::tour::anchor("company-status", pill);
    l.ui.p().rounded(pill, h * 0.5, PANEL.mix(HOVER, 0.6));
    l.ui.p().rounded_border(pill, h * 0.5, 1.5, Color::WHITE.alpha(0.3));
    let mut x = pill.x + pad;
    for ((icon, value, icon_c, text_c, tip, tab), w) in items.into_iter().zip(widths).take(n) {
        let hit = Rect::new(x - gap * 0.5 + 2.0, pill.y + 3.0, w + gap - 4.0, h - 6.0);
        let (hover, _, clicked) = l.ui.interact(super::ui::id_of(&format!("company-status-{icon}")), hit);
        let (hover, clicked) = if pages_of(c.mode).contains(&tab) { (hover, clicked) } else { (false, false) };
        if hover {
            l.ui.p().rounded(hit, hit.h * 0.5, Color::WHITE.alpha(0.06));
        }
        let at = Vec2::new(x + d * 0.5, pill.center().y);
        l.ui.p().circle(at, d * 0.5, icon_c.alpha(0.32));
        l.ui.icon(icon, at, d * 0.56, icon_c.lighten(0.25));
        l.ui.text_in(&value, Rect::new(x + d + 8.0, pill.y, w - d - 8.0 + 2.0, h), px, Weight::Bold, text_c, Align::Left);
        l.ui.tooltip(hit, tip);
        if clicked {
            l.company.tab = tab;
        }
        x += w + gap;
    }
    pill.right()
}

/// The way round the company's pages, under its bar: the start screen's button with the page
/// one is on after it (or, on the start screen, where the company is at home), and on the
/// right every page as an icon - grouped as the start screen groups them, a badge where
/// something waits - a click from wherever one is.
fn nav(l: &mut Launcher, r: Rect, c: &Company) {
    super::tour::anchor("company-nav", r);
    let marks = home::Marks::of(c, l.company.plan.as_ref());
    let home = l.company.tab == HOME_TAB;
    // the pages, from the right
    let side = 38.0;
    let gap = 4.0;
    let group_gap = 17.0;
    // (the mode's pages: the easy mode has fewer)
    let groups = self::rail(c.mode);
    let count: usize = groups.iter().map(|g| g.len()).sum();
    let rail_w = count as f32 * side + (count - groups.len()) as f32 * gap + (groups.len() - 1) as f32 * group_gap;
    let x0 = r.right() - rail_w;
    let rail = Rect::new(x0, r.y + (r.h - side) * 0.5, rail_w, side);
    super::tour::anchor("company-tabs", rail);
    let mut x = x0;
    for (g, group) in groups.iter().enumerate() {
        if g > 0 {
            l.ui.p().rect(Rect::new(x - group_gap * 0.5 - 0.5, rail.y + 9.0, 1.0, side - 18.0), HAIRLINE);
        }
        for &tab in group.iter() {
            let b = Rect::new(x, rail.y, side, side);
            let (h, _, clicked) = l.ui.interact(super::ui::id_of(&format!("company-rail-{tab}")), b);
            let on = l.company.tab == tab;
            let t = l.ui.anim(super::ui::id_of(&format!("company-rail-{tab}")) ^ 0x5a, if h { 1.0 } else { 0.0 }, 0.08);
            if on {
                l.ui.p().rounded(b, RADIUS, accent());
            } else if t > 0.01 {
                l.ui.p().rounded(b, RADIUS, HOVER.alpha(t));
            }
            l.ui.icon(ICONS[tab], b.center(), 19.0, if on { on_accent() } else { TEXT_DIM.mix(TEXT, t) });
            let mut tip = omsi_ui::tr(TABS[tab]).into_owned();
            if let Some((n, colour, why)) = marks.badge(tab) {
                let d = Rect::new(b.right() - 15.0, b.y - 3.0, (l.ui.width(&n, 10.5, Weight::Bold) + 8.0).max(16.0), 16.0);
                l.ui.p().rounded(d.inset(-1.5), 9.5, GROUND);
                l.ui.p().rounded(d, 8.0, colour);
                let ink = if colour == accent() { on_accent() } else if colour == WARN { ON_LINE } else { Color::WHITE };
                l.ui.text_in(&n, d, 10.5, Weight::Bold, ink, Align::Center);
                tip = format!("{tip}\n{why}");
            }
            l.ui.tooltip(b, &tip);
            if clicked {
                l.company.tab = tab;
            }
            x = b.right() + gap;
        }
        x += group_gap - gap;
    }
    // the start screen's button, and where one is
    let label = omsi_ui::tr("Start screen");
    let bw = (l.ui.width(&label, kit::CONTROL, Weight::Bold) + 58.0).max(120.0);
    let b = Rect::new(r.x, r.y + 1.0, bw, r.h - 2.0);
    if l.ui.button("company-home", b, "Start screen", Some("apps"), if home { ButtonKind::Primary } else { ButtonKind::Normal }) && !home {
        l.company.tab = HOME_TAB;
    }
    l.ui.tooltip(b, "Every part of the company as a tile (Escape)");
    let room = (x0 - 20.0 - b.right() - 14.0).max(0.0);
    if home {
        l.ui.text_in(&home_of(c), Rect::new(b.right() + 18.0, r.y, room, r.h), kit::BODY, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let tab = l.company.tab;
    l.ui.icon("chevron_right", Vec2::new(b.right() + 16.0, r.center().y), 18.0, TEXT_DIM);
    if room > 60.0 {
        l.ui.icon(ICONS.get(tab).copied().unwrap_or("apps"), Vec2::new(b.right() + 42.0, r.center().y), 20.0, accent_2());
        l.ui.text_in(page_name(tab), Rect::new(b.right() + 58.0, r.y, (room - 48.0).max(0.0), r.h), 18.0, Weight::Bold, TEXT, Align::Left);
    }
}

/// The company's strip on a phone: its mark, its name and what it has (cash, trophies,
/// reputation); on a page the way back to the start screen with the page's name, which opens
/// the list of every page; and the company's clock. Returns the room under it.
fn strip(l: &mut Launcher, area: Rect) -> Rect {
    let Some(c) = l.company.company.clone() else { return area };
    let mark = Rect::new(area.x, area.y, 44.0, 44.0);
    company_mark(l, mark, &c);
    l.ui.text_in(&c.name, Rect::new(mark.right() + 14.0, area.y - 2.0, area.w - 60.0, 24.0), 18.0, Weight::Bold, TEXT, Align::Left);
    status(l, Rect::new(mark.right() + 14.0, area.y + 24.0, area.w - 60.0, 22.0), &c);
    let mut y = area.y + 56.0;
    if l.company.tab != HOME_TAB {
        // the way back, and every page in a list beside it
        let label = omsi_ui::tr("Start screen");
        let bw = (l.ui.width(&label, kit::CONTROL, Weight::Bold) + 52.0).min(area.w * 0.45);
        let back = Rect::new(area.x, y, bw, ROW + 4.0);
        super::tour::anchor("company-nav", Rect::new(area.x, y, area.w, ROW + 4.0));
        if l.ui.button("company-home", back, "Start screen", Some("chevron_left"), ButtonKind::Normal) {
            l.company.tab = HOME_TAB;
        }
        // (the mode's pages, and the one open when it is none of them)
        let mut tabs = pages_of(c.mode);
        if !tabs.contains(&l.company.tab) && l.company.tab < TABS.len() {
            tabs.push(l.company.tab);
        }
        let names: Vec<String> = tabs.iter().map(|t| omsi_ui::tr(TABS[*t]).into_owned()).collect();
        let mut k = tabs.iter().position(|t| *t == l.company.tab).unwrap_or(0);
        if l.ui.select("company-pages", Rect::new(back.right() + 10.0, y, (area.w - bw - 10.0).max(80.0), ROW + 4.0), &mut k, &names) {
            l.company.tab = tabs[k.min(tabs.len() - 1)];
        }
        y = back.bottom() + 10.0;
    }
    // the company's clock: its steps and the time dialog (the desktop has them in its bar;
    // the phone has no sheet's head to keep them in)
    let head = Rect::new(area.x, y, area.w, 40.0);
    clock::head(l, head);
    Rect::new(area.x, head.bottom() + 14.0, area.w, (area.bottom() - head.bottom() - 14.0).max(0.0))
}

/// What the page keeps in its sheet's head (the desktop's company screen draws its own bar).
pub fn head_tools(l: &mut Launcher, r: Rect) -> f32 {
    if l.company.company.is_none() || l.company.wizard.is_some() {
        return r.right();
    }
    clock::head(l, r)
}

/// The company's pages are left (their way back, a duty to drive): a playing clock stops - it
/// runs only while they are open - and what it left unsaved is saved.
pub fn leave(l: &mut Launcher) {
    clock::play(l, 0.0);
}

/// Another of the driver's companies opened (None: none is left, the founding follows): a
/// playing clock stops and saves the one open first, and what the pages kept of it - the
/// planning's choices, a workshop task, the bus, line or tender picked, an advert armed, the
/// month shown, a line's tours waiting to be planned anew - is let go.
pub(super) fn switch_company(l: &mut Launcher, to: Option<Company>) {
    clock::play(l, 0.0);
    // (a duty of the company left is no duty of the one opened)
    if l.state.choice.company.as_ref().is_some_and(|d| to.as_ref().is_none_or(|c| c.id != d.company)) {
        l.state.end_company_duty();
    }
    let view = &mut l.company;
    view.company = to;
    view.chooser = None;
    view.plan = None;
    view.refill.clear();
    view.planning = planning::PlanningView::default();
    view.career.game = None;
    view.fleet.selected = None;
    view.fleet.adverts = Default::default();
    view.depot = Default::default();
    view.tenders = Default::default();
    view.lines = Default::default();
    view.money = Default::default();
}

fn closing_cover(l: &mut Launcher) {
    let size = l.ui.size;
    let full = Rect::new(0.0, 0.0, size.x, size.y);
    l.ui.solid(full);
    l.ui.p().rect(full, Color::rgba(0, 0, 0, 0.45));
    // (wide enough for a catch-up's span: `clock::cover_text`)
    let w = (l.ui.width(&clock::cover_text(l), kit::HEAD, Weight::Bold) + 60.0).clamp(380.0, (size.x - 32.0).max(380.0));
    let r = Rect::new((size.x - w) * 0.5, (size.y - 110.0) * 0.5, w, 110.0);
    l.ui.panel(r);
    l.ui.text_in(&clock::cover_text(l), Rect::new(r.x, r.y + 24.0, r.w, 28.0), kit::HEAD, Weight::Bold, TEXT, Align::Center);
    l.ui.progress(Rect::new(r.x + 40.0, r.y + 72.0, r.w - 80.0, 6.0), 1.0, true);
}

fn dialog(l: &mut Launcher) {
    match &l.company.dialog {
        Some(Dialog::New { .. }) | Some(Dialog::Vehicle { .. }) => fleet::dialog(l),
        Some(Dialog::Dealer) => dealer::dialog(l),
        Some(Dialog::Confirm { what }) => {
            let what = what.clone();
            confirm_dialog(l, what);
        }
        Some(Dialog::Time) => clock::time_dialog(l),
        Some(Dialog::Fare { .. }) => fares::dialog(l),
        Some(Dialog::AddLine { .. }) => lines::add_dialog(l),
        Some(Dialog::Service { .. }) => planning::service_dialog(l),
        Some(Dialog::Settings { .. }) => settings::dialog(l),
        None => {}
    }
}

fn confirm_dialog(l: &mut Launcher, what: Confirm) {
    let (title, text, button) = {
        let Some(c) = l.company.company.as_ref() else { return };
        match &what {
            Confirm::Sell(id) => {
                let Some(v) = c.vehicle(*id) else {
                    l.company.dialog = None;
                    return;
                };
                let amount = co::market::sale_offer(c, v);
                let text = match v.tenure {
                    co::Tenure::Owned { .. } => omsi_ui::tr("A dealer pays %{amount} for %{bus}.").replace("%{amount}", &eur(amount)).replace("%{bus}", &format!("{} {}", v.number, v.name)),
                    co::Tenure::Leased { .. } => omsi_ui::tr("Giving the leased bus back early costs %{amount} (three monthly rates).").replace("%{amount}", &eur(-amount)),
                    co::Tenure::Rented { .. } => omsi_ui::tr("The rented bus goes back today; the days rented are paid.").into_owned(),
                };
                let button = if matches!(v.tenure, co::Tenure::Owned { .. }) { "Sell" } else { "Give back" };
                (format!("{} {}", v.number, v.name), text, button)
            }
            Confirm::Dismiss(id) => {
                let Some(e) = c.employee(*id) else {
                    l.company.dialog = None;
                    return;
                };
                let r = co::economy::rules(c.difficulty);
                let mut text = omsi_ui::tr("%{name} works %{days} more days (the notice) and then leaves.").replace("%{name}", &e.name).replace("%{days}", &r.notice_days.to_string());
                if r.severance_months_per_year > 0.0 {
                    text.push(' ');
                    text.push_str(&omsi_ui::tr("They are paid half a month's wage for every year they worked here."));
                }
                (e.name.clone(), text, "Dismiss")
            }
            Confirm::RemoveLine(name) => {
                let number = c.lines.iter().find(|x| &x.name == name).map(|x| x.number.clone()).unwrap_or_default();
                (omsi_ui::tr("Line %{n}").replace("%{n}", &number), omsi_ui::tr("The company stops running this line from today. Its buses and drivers stay.").into_owned(), "Stop running it")
            }
            Confirm::DeleteCompany(id) => {
                let name = l.company.companies.as_ref().and_then(|cs| cs.iter().find(|x| &x.id == id)).map(|x| x.name.clone()).unwrap_or_else(|| c.name.clone());
                (
                    omsi_ui::tr("Delete %{name}?").replace("%{name}", &name),
                    omsi_ui::tr("The company is deleted for good: its buses, staff, lines, money and history. Your service record as a driver stays. This cannot be undone.").into_owned(),
                    "Delete for good",
                )
            }
        }
    };
    let h = 70.0 + l.ui.paragraph_height(&text, 520.0 - 56.0, kit::BODY, Weight::Regular) + 24.0 + kit::BUTTON_H + 24.0;
    let f = kit::frame(l, 560.0, h.max(220.0), "warning", &title);
    l.ui.paragraph(&text, Vec2::new(f.body.x, f.body.y), f.body.w, kit::BODY, Weight::Regular, TEXT_SOFT);
    let mut foot = kit::Foot::new(&f);
    let yes = foot.right(l, "company-confirm-yes", button, None, ButtonKind::Danger);
    if foot.right(l, "company-confirm-no", "Cancel", None, ButtonKind::Normal) || f.close {
        l.company.dialog = None;
        return;
    }
    if yes {
        l.company.dialog = None;
        match what {
            Confirm::Sell(id) => {
                if act(l, |c| co::market::sell(c, id)).is_some() {
                    l.company.fleet.selected = None;
                }
            }
            Confirm::Dismiss(id) => {
                if let Some(until) = act(l, |c| co::staff::dismiss(c, id)) {
                    l.state.set_status(omsi_ui::tr("Their last day is %{date}.").replace("%{date}", &day_label(&until)), false);
                }
            }
            Confirm::RemoveLine(name) => {
                act(l, |c| {
                    co::network::remove_line(c, &name);
                    Ok(())
                });
            }
            Confirm::DeleteCompany(id) => delete_company(l, &id),
        }
    }
}

/// Delete a company for good (Luc: companies can be deleted) - its file and its live file:
/// the start's companies follow (the founding there when none is left). Not while its day is
/// being simulated (the simulation would write it back).
fn delete_company(l: &mut Launcher, id: &str) {
    if l.company.closing {
        l.state.set_status(omsi_ui::tr("Wait until the company's day is simulated.").into_owned(), true);
        return;
    }
    let name = l.company.companies.as_ref().and_then(|cs| cs.iter().find(|x| x.id == id)).map(|x| x.name.clone()).unwrap_or_default();
    let open = l.company.company.as_ref().is_none_or(|c| c.id == id);
    if open {
        // (stopped before the file goes: what a playing clock left unsaved would write it back)
        clock::play(l, 0.0);
    }
    if let Err(e) = co::store::delete(&data(), id) {
        l.state.set_status(format!("{e:#}"), true);
        return;
    }
    if let Some(list) = l.company.companies.as_mut() {
        list.retain(|x| x.id != id);
    }
    if open {
        // (the next company is chosen on the start, as every company is)
        switch_company(l, None);
        chooser::to_companies(l);
    }
    let view = &mut l.company;
    view.plan = None;
    view.reports = None;
    view.planning = planning::PlanningView::default();
    l.state.set_status(omsi_ui::tr("%{name} is deleted.").replace("%{name}", &name), false);
}

/// A note of the day's report as the company's mode says it: in the easy mode a line won runs
/// by itself, with no planning to do first.
fn note_for(n: &Note, easy: bool) -> (String, Color, Option<kit::Go>) {
    match n {
        Note::Won { number, until } if easy => (omsi_ui::tr("The concession for line %{n} is yours until %{date}: the line runs by itself from its first day.").replace("%{n}", number).replace("%{date}", &day_label(until)), OK, None),
        _ => note_text(n),
    }
}

/// What a note of the day's report says, its colour, and where it is mended.
fn note_text(n: &Note) -> (String, Color, Option<kit::Go>) {
    match n {
        Note::Breakdown { number, until, cost } => (
            omsi_ui::tr("Bus %{n} broke down on its tour: in the workshop until %{date}, repairs %{amount}.").replace("%{n}", number).replace("%{date}", &day_label(until)).replace("%{amount}", &eur(*cost)),
            DANGER.lighten(0.25),
            None,
        ),
        Note::Service { number } => (omsi_ui::tr("Bus %{n} is due for its service: in the workshop tomorrow.").replace("%{n}", number), TEXT_SOFT, None),
        Note::Returned { number, name } => (omsi_ui::tr("Bus %{n} (%{name}) went back: its lease or rental ended.").replace("%{n}", number).replace("%{name}", name), TEXT_SOFT, Some(kit::Go::Dealer)),
        Note::LoanPaid { purpose } => (omsi_ui::tr("A loan is paid off: %{what}.").replace("%{what}", purpose), OK, None),
        Note::Month { month, result } => (
            omsi_ui::tr("%{month} is closed: wages, leases, insurance, the depot and loan rates are booked. The month's result: %{amount}.").replace("%{month}", &month_label(month)).replace("%{amount}", &eur(*result)),
            if *result >= 0 { OK } else { WARN },
            None,
        ),
        Note::Built { area } => (omsi_ui::tr("The depot's building work is done: %{what}.").replace("%{what}", &omsi_ui::tr(area)), OK, None),
        Note::BayWait { number } => (omsi_ui::tr("Bus %{n} waits for a free workshop bay.").replace("%{n}", number), WARN, Some(kit::Go::Depot)),
        Note::Won { number, until } => (omsi_ui::tr("The concession for line %{n} is yours until %{date}. Plan its tours and start its service.").replace("%{n}", number).replace("%{date}", &day_label(until)), OK, Some(kit::Go::Planning)),
        Note::Lost { number, winner } => (omsi_ui::tr("%{who} won the tender for line %{n}.").replace("%{n}", number).replace("%{who}", winner), WARN, None),
        Note::Ended { number } => (omsi_ui::tr("The concession for line %{n} has ended: the line is no longer yours.").replace("%{n}", number), DANGER.lighten(0.25), None),
        Note::NotStarted { number, days, charge } => (
            omsi_ui::tr("Line %{n} is still not in service, %{days} days after it was taken on: the authority charged %{amount}. Plan its tours and start its service.").replace("%{n}", number).replace("%{days}", &days.to_string()).replace("%{amount}", &eur(*charge)),
            DANGER.lighten(0.25),
            Some(kit::Go::Planning),
        ),
        Note::Accident { number, cost } => (
            omsi_ui::tr("Bus %{n} had an accident on its tour: damage %{amount}.").replace("%{n}", number).replace("%{amount}", &eur(*cost)),
            DANGER.lighten(0.25),
            Some(kit::Go::Training),
        ),
    }
}

fn staff_text(n: &co::staff::StaffNote) -> (String, Color, Option<kit::Go>) {
    use co::staff::StaffNote as S;
    match n {
        S::Sick { name, until } => (omsi_ui::tr("%{name} is ill until %{date}.").replace("%{name}", name).replace("%{date}", &day_label(until)), WARN, Some(kit::Go::Planning)),
        S::Holiday { name, until } => (omsi_ui::tr("%{name} is on holiday until %{date}.").replace("%{name}", name).replace("%{date}", &day_label(until)), TEXT_SOFT, None),
        S::Resigned { name, until } => (omsi_ui::tr("%{name} has resigned; their last day is %{date}.").replace("%{name}", name).replace("%{date}", &day_label(until)), DANGER.lighten(0.25), Some(kit::Go::Hire)),
        S::Unhappy { name } => (omsi_ui::tr("%{name} is unhappy with their pay or their hours.").replace("%{name}", name), WARN, None),
        S::Left { name } => (omsi_ui::tr("%{name} has left the company.").replace("%{name}", name), TEXT_SOFT, Some(kit::Go::Hire)),
    }
}

/// A line of what happened: its words, colour, and the buttons that mend it.
struct Fact {
    text: String,
    colour: Color,
    fixes: Vec<kit::Go>,
}

/// The report of the day (or days) just closed: every amount said with what it is, the lines
/// with their money, and what happened with its cause and what mends it.
fn report_dialog(l: &mut Launcher) {
    let Some(reports) = l.company.reports.clone() else { return };
    let Some(last) = reports.last().cloned() else {
        l.company.reports = None;
        return;
    };
    let Some(c) = l.company.company.clone() else { return };
    let first = reports.first().cloned().unwrap_or_default();
    let title = if reports.len() > 1 { format!("{} – {}", day_label(&first.date), day_label(&last.date)) } else { day_label(&last.date) };
    let f = kit::frame(l, 1000.0, 860.0, "receipt_long", &title);
    let inner = f.body;
    let sum = |f: &dyn Fn(&DayReport) -> i64| reports.iter().map(f).sum::<i64>();
    let result = sum(&|r| r.result);
    let income = sum(&|r| r.income);
    let expenses = sum(&|r| r.expenses);
    let tours = sum(&|r| r.tours as i64);
    let covered = sum(&|r| r.covered as i64);
    let trips = sum(&|r| r.trips as i64);
    let dropped = sum(&|r| r.dropped as i64);
    let late = sum(&|r| r.late as i64);
    let pax = sum(&|r| r.passengers as i64);
    let km: f64 = reports.iter().map(|r| r.km).sum();
    let measured = sum(&|r| r.measured as i64);
    let penalties = sum(&|r| r.penalties);
    let uncovered = sum(&|r| r.uncovered as i64);
    let short_buses = sum(&|r| r.short_buses as i64);
    let short_drivers = sum(&|r| r.short_drivers as i64);
    let rep: f64 = reports.iter().map(|r| r.reputation_change).sum();
    // the money of these days by kind (the books of their dates)
    let mut kinds: Vec<(co::BookingKind, i64)> = Vec::new();
    for b in c.ledger.iter().filter(|b| b.date >= first.date && b.date <= last.date && !b.kind.is_capital()) {
        match kinds.iter_mut().find(|k| k.0 == b.kind) {
            Some(k) => k.1 += b.amount,
            None => kinds.push((b.kind, b.amount)),
        }
    }
    kinds.retain(|k| k.1 != 0);
    kinds.sort_by_key(|k| std::cmp::Reverse(k.1));
    // the figures
    let gap = 12.0;
    let fw = (inner.w - 3.0 * gap) / 4.0;
    let fy = inner.y;
    let (inc, inc_c) = kit::signed(income);
    let (out, _) = kit::signed(-expenses);
    let res_r = Rect::new(inner.x, fy, fw, kit::FIGURE_H);
    figure(&mut l.ui, res_r, "Result", &kit::signed(result).0, &format!("{} {}  ·  {} {}", inc, omsi_ui::tr("in"), out, omsi_ui::tr("out")), if result >= 0 { OK } else { DANGER.lighten(0.2) });
    let _ = inc_c;
    let tip = kinds.iter().map(|(k, a)| format!("{}: {}", omsi_ui::tr(k.label()), kit::signed(*a).0)).collect::<Vec<_>>().join("\n");
    l.ui.tooltip(res_r, &tip);
    figure(&mut l.ui, Rect::new(inner.x + fw + gap, fy, fw, kit::FIGURE_H), "Tours run", &format!("{covered} / {tours}"), &omsi_ui::tr("%{n} trips dropped").replace("%{n}", &dropped.to_string()), if covered == tours { OK } else { WARN });
    let punct = if trips - dropped > 0 { format!("{:.0} %", 100.0 * (trips - dropped - late).max(0) as f64 / (trips - dropped) as f64) } else { "–".into() };
    figure(&mut l.ui, Rect::new(inner.x + 2.0 * (fw + gap), fy, fw, kit::FIGURE_H), "On time", &punct, &omsi_ui::tr("%{n} trips late").replace("%{n}", &late.to_string()), TEXT);
    figure(&mut l.ui, Rect::new(inner.x + 3.0 * (fw + gap), fy, fw, kit::FIGURE_H), "Passengers", &grouped(pax as f64), &format!("{} km", grouped(km.round())), TEXT);
    // what happened
    let mut facts: Vec<Fact> = Vec::new();
    if uncovered > 0 || dropped > 0 {
        let mut fixes = Vec::new();
        if short_buses > 0 {
            fixes.push(kit::Go::Dealer);
        }
        if short_drivers > 0 {
            fixes.push(kit::Go::Hire);
        }
        fixes.push(kit::Go::Planning);
        let text = if uncovered > 0 {
            omsi_ui::tr("%{trips} trips dropped: %{tours} tours were not covered - %{buses} without a bus, %{duties} duties without a driver.").replace("%{trips}", &dropped.to_string()).replace("%{tours}", &uncovered.to_string()).replace("%{buses}", &short_buses.to_string()).replace("%{duties}", &short_drivers.to_string())
        } else {
            omsi_ui::tr("%{trips} trips dropped: a bus broke down, or a driver came late.").replace("%{trips}", &dropped.to_string())
        };
        facts.push(Fact { text, colour: DANGER.lighten(0.25), fixes });
    }
    if penalties > 0 {
        facts.push(Fact { text: omsi_ui::tr("Contract penalties %{amount}: the authority charges for every trip dropped or late, and for a concession not started.").replace("%{amount}", &kit::signed(-penalties).0), colour: WARN, fixes: vec![kit::Go::Planning] });
    }
    if measured > 0 {
        facts.push(Fact { text: omsi_ui::tr("You drove %{n} trips yourself: booked as measured.").replace("%{n}", &measured.to_string()), colour: accent_2(), fixes: Vec::new() });
    }
    let crowded = sum(&|r| r.crowded as i64);
    if crowded > 0 {
        let left = sum(&|r| r.left_behind as i64);
        facts.push(Fact { text: omsi_ui::tr("%{n} trips of your own lines were full; %{p} passengers were left at the stop. Bigger buses help.").replace("%{n}", &crowded.to_string()).replace("%{p}", &left.to_string()), colour: WARN, fixes: vec![kit::Go::Dealer] });
    }
    // (what else befell the trips: `co::incidents`, and the courses that help)
    let fines = sum(&|r| r.incidents.fines as i64);
    if fines > 0 {
        let cost = sum(&|r| r.incidents.fine_cost);
        let t = omsi_ui::tr("Your drivers were fined %{n} times in traffic: %{amount}. Defensive driving halves it.").replace("%{n}", &fines.to_string()).replace("%{amount}", &eur(cost));
        facts.push(Fact { text: t, colour: WARN, fixes: vec![kit::Go::Training] });
    }
    let complaints = sum(&|r| r.incidents.complaints as i64);
    if complaints > 0 {
        let t = omsi_ui::tr("%{n} passengers complained about their trip. The customer service course halves the complaints.").replace("%{n}", &complaints.to_string());
        facts.push(Fact { text: t, colour: WARN, fixes: vec![kit::Go::Training] });
    }
    let ill = sum(&|r| r.incidents.taken_ill as i64);
    if ill > 0 {
        let helped = sum(&|r| r.incidents.helped as i64);
        let t = omsi_ui::tr("%{n} passengers were taken ill or hurt on board; a first-aider was at the wheel for %{m} of them.").replace("%{n}", &ill.to_string()).replace("%{m}", &helped.to_string());
        facts.push(Fact { text: t, colour: if helped == ill { TEXT_SOFT } else { WARN }, fixes: if helped == ill { Vec::new() } else { vec![kit::Go::Training] } });
    }
    if rep.abs() >= 0.05 {
        let t = if rep > 0.0 { "Your reputation rose to %{n}: trips on time and passengers carried." } else { "Your reputation fell to %{n}: trips dropped and late cost it." };
        facts.push(Fact { text: omsi_ui::tr(t).replace("%{n}", &num(last.reputation, 1)), colour: if rep > 0.0 { OK } else { WARN }, fixes: Vec::new() });
    }
    let easy = co::autopilot::on(&c);
    for r in &reports {
        for (text, colour, go) in r.notes.iter().map(|n| note_for(n, easy)).chain(r.staff.iter().map(staff_text)) {
            facts.push(Fact { text, colour, fixes: go.into_iter().collect() });
        }
    }
    if tours == 0 {
        let unplanned = reports.iter().flat_map(|r| r.lines.iter()).any(|x| x.unplanned);
        let text = if unplanned && easy {
            omsi_ui::tr("No tours ran: the company's lines were not in service yet. They go into service by themselves the next morning.")
        } else if unplanned {
            omsi_ui::tr("No tours ran: the company's lines are not in service yet. Plan their tours and start their service on the Planning page.")
        } else { omsi_ui::tr("No tours ran: add lines on the Lines page, and buy buses and hire drivers for them.") };
        facts.insert(0, Fact { text: text.into_owned(), colour: TEXT_SOFT, fixes: vec![if unplanned { kit::Go::Planning } else { kit::Go::Lines }] });
    }
    // (the easy mode mends what the company does not see to itself)
    if easy {
        for f in facts.iter_mut() {
            f.fixes = f.fixes.iter().filter_map(|g| easy_go(*g)).collect();
        }
    }
    // the lines, each with its money
    #[derive(Clone, Default)]
    struct LineSum {
        line: String,
        number: String,
        tours: u32,
        covered: u32,
        dropped: u32,
        trips: u32,
        km: f64,
        passengers: u32,
        fares: i64,
        compensation: i64,
        running: i64,
        penalty: i64,
        unplanned: bool,
    }
    let mut lines_by: Vec<LineSum> = Vec::new();
    for r in &reports {
        for ld in &r.lines {
            let i = match lines_by.iter().position(|x| x.line == ld.line) {
                Some(i) => i,
                None => {
                    lines_by.push(LineSum { line: ld.line.clone(), number: ld.number.clone(), unplanned: true, ..Default::default() });
                    lines_by.len() - 1
                }
            };
            let x = &mut lines_by[i];
            x.tours += ld.tours;
            x.covered += ld.covered;
            x.dropped += ld.dropped;
            x.trips += ld.trips;
            x.km += ld.km;
            x.passengers += ld.passengers;
            // (an older report kept the revenue alone)
            if ld.fares == 0 && ld.compensation == 0 {
                x.fares += ld.revenue;
            } else {
                x.fares += ld.fares;
                x.compensation += ld.compensation;
            }
            x.running += ld.running;
            x.penalty += ld.penalty;
            x.unplanned &= ld.unplanned;
        }
    }
    let open = l.company.report_line.clone();
    let list = Rect::new(inner.x, fy + kit::FIGURE_H + 18.0, inner.w, inner.bottom() - fy - kit::FIGURE_H - 18.0);
    let mut toggle: Option<String> = None;
    let mut fix: Option<kit::Go> = None;
    let lines_of = c.lines.clone();
    l.ui.scroll_area("company-report", list, &mut |ui, v| {
        let mut yy = v.y;
        // the money
        if !kinds.is_empty() {
            kit::caps(ui, Rect::new(v.x, yy, v.w, 16.0), "Money");
            yy += 24.0;
            let cw = (v.w - 24.0) / 2.0;
            for (k, (kind, amount)) in kinds.iter().enumerate() {
                let x = v.x + (k % 2) as f32 * (cw + 24.0);
                let y = yy + (k / 2) as f32 * 26.0;
                ui.text_in(&omsi_ui::tr(kind.label()), Rect::new(x, y, cw * 0.6, 24.0), kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Left);
                let (t, col) = kit::signed(*amount);
                ui.text_in(&t, Rect::new(x + cw * 0.4, y, cw * 0.6, 24.0), kit::ROWS, Weight::Bold, col, Align::Right);
                ui.p().rect(Rect::new(x, y + 24.0, cw, 1.0), HAIRLINE);
            }
            yy += kinds.len().div_ceil(2) as f32 * 26.0 + 18.0;
        }
        if !lines_by.is_empty() {
            kit::caps(ui, Rect::new(v.x, yy, v.w, 16.0), "Lines");
            ui.text_in(&omsi_ui::tr("Click a line for its money"), Rect::new(v.x, yy, v.w, 16.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Right);
            yy += 24.0;
            for x in &lines_by {
                let row = Rect::new(v.x, yy, v.w, 40.0);
                let is_open = open.as_deref() == Some(x.line.as_str());
                if ui.row(&format!("company-report-line-{}", x.line), row, false) {
                    toggle = Some(x.line.clone());
                }
                let w = match lines_of.iter().find(|cl| cl.name == x.line) {
                    Some(cl) => line_plate(ui, Vec2::new(row.x + 8.0, row.y + 8.0), cl, 24.0),
                    None => plate(ui, Vec2::new(row.x + 8.0, row.y + 8.0), &x.number, 24.0),
                };
                let text = if x.unplanned && x.tours == 0 {
                    omsi_ui::tr("Not in service: its tours did not run").into_owned()
                } else {
                    omsi_ui::tr("%{c} of %{t} tours  ·  %{d} trips dropped  ·  %{p} passengers").replace("%{c}", &x.covered.to_string()).replace("%{t}", &x.tours.to_string()).replace("%{d}", &x.dropped.to_string()).replace("%{p}", &grouped(x.passengers as f64))
                };
                line(ui, Rect::new(row.x + w + 22.0, row.y, row.w * 0.55, row.h), &text, kit::ROWS, TEXT_SOFT);
                let net = x.fares + x.compensation - x.running - x.penalty;
                let (t, col) = kit::signed(net);
                ui.text_in(&format!("{t} {}", omsi_ui::tr("for the line")), Rect::new(row.right() - 300.0, row.y, 270.0, row.h), kit::ROWS, Weight::Bold, col, Align::Right);
                ui.icon(if is_open { "expand_less" } else { "expand_more" }, Vec2::new(row.right() - 14.0, row.center().y), 18.0, TEXT_DIM);
                yy += 44.0;
                if is_open {
                    let per_km = if x.km > 0.0 { x.compensation as f64 / x.km } else { 0.0 };
                    let rows: [(String, i64); 4] = [
                        (omsi_ui::tr("Fares: %{p} passengers").replace("%{p}", &grouped(x.passengers as f64)), x.fares),
                        (omsi_ui::tr("The authority's payment: %{km} km at %{rate} a km").replace("%{km}", &grouped(x.km)).replace("%{rate}", &eur_cents(per_km)), x.compensation),
                        (omsi_ui::tr("Running costs: fuel or power, maintenance").into_owned(), -x.running),
                        (omsi_ui::tr("Penalties: trips dropped and late").into_owned(), -x.penalty),
                    ];
                    for (label, amount) in rows {
                        ui.text_in(&label, Rect::new(v.x + 52.0, yy, v.w * 0.6, 24.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
                        let (t, col) = kit::signed(amount);
                        ui.text_in(&t, Rect::new(v.right() - 330.0, yy, 300.0, 24.0), kit::NOTE, Weight::Medium, col, Align::Right);
                        yy += 26.0;
                    }
                    ui.text_in(&omsi_ui::tr("Wages, insurance, leases and the depot are booked for the whole company at the month's end."), Rect::new(v.x + 52.0, yy, v.w - 80.0, 22.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
                    yy += 32.0;
                }
            }
            yy += 10.0;
        }
        if !facts.is_empty() {
            kit::caps(ui, Rect::new(v.x, yy, v.w, 16.0), "What happened");
            yy += 26.0;
            for (k, ft) in facts.iter().enumerate() {
                ui.p().circle(Vec2::new(v.x + 5.0, yy + 10.0), 4.0, ft.colour);
                let tw = v.w - 24.0 - if ft.fixes.is_empty() { 0.0 } else { 0.0 };
                let h = ui.paragraph(&ft.text, Vec2::new(v.x + 20.0, yy), tw, kit::BODY - 0.5, Weight::Regular, TEXT);
                yy += h.max(20.0) + 6.0;
                if !ft.fixes.is_empty() {
                    let mut x = v.x + 20.0;
                    for (n, g) in ft.fixes.iter().enumerate() {
                        let bw = kit::Foot::width(ui, g.label(), Some(g.icon()));
                        if ui.button(&format!("company-report-fix-{k}-{n}"), Rect::new(x, yy, bw, 34.0), g.label(), Some(g.icon()), ButtonKind::Normal) {
                            fix = Some(*g);
                        }
                        x += bw + 8.0;
                    }
                    yy += 44.0;
                }
                yy += 4.0;
            }
        }
        yy - v.y + 8.0
    });
    if let Some(t) = toggle {
        l.company.report_line = if open.as_deref() == Some(t.as_str()) { None } else { Some(t) };
    }
    let mut foot = kit::Foot::new(&f);
    let done = foot.right(l, "company-report-ok", "Close the report", None, ButtonKind::Primary);
    if done || f.close || l.ui.input.keys.contains(&Key::Enter) {
        l.company.reports = None;
    }
    if let Some(g) = fix {
        go(l, g);
    }
}

/// A line number's plate: a company line's colour, or the yellow of the timetable's lines.
pub(super) fn plate(ui: &mut Ui, at: Vec2, number: &str, h: f32) -> f32 {
    let px = h * 0.6;
    let w = (ui.width(number, px, Weight::Black) + h * 0.7).max(h * 1.8);
    let r = Rect::new(at.x, at.y, w, h);
    ui.p().rounded(r, 5.0f32.min(h * 0.25), LINE);
    ui.text_in(number, r, px, Weight::Black, ON_LINE, Align::Center);
    w
}

/// A company line's plate (its own colour when it has one).
pub(super) fn line_plate(ui: &mut Ui, at: Vec2, l: &co::CompanyLine, h: f32) -> f32 {
    if l.colour.trim().is_empty() {
        plate(ui, at, &l.number, h)
    } else {
        super::ownlines::plate(ui, at, &l.number, &l.colour, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_grouped_as_the_language_groups_it() {
        assert_eq!(eur_in(123_456_789, ""), "€1,234,568");
        assert_eq!(eur_in(-5_000_00, ""), "-€5,000");
        assert_eq!(eur_in(0, "en"), "€0");
        assert_eq!(eur_in(123_456_789, "nl"), "€ 1.234.568");
        assert_eq!(eur_in(123_456_789, "de"), "1.234.568 €");
        assert_eq!(eur_in(123_456_789, "fr"), "1\u{202F}234\u{202F}568 €");
        assert_eq!(day_label("nonsense"), "nonsense");
    }

    #[test]
    fn every_language_writes_its_numbers_its_own_way() {
        // (whole euros, euros and cents, a number with a decimal, kilometres)
        let n = "\u{202F}";
        let cases: [(&str, &str, &str, &str, &str); 8] = [
            ("en", "€17,968", "€17,968.50", "4.5", "1,234,567"),
            ("", "€17,968", "€17,968.50", "4.5", "1,234,567"),
            ("nl", "€ 17.968", "€ 17.968,50", "4,5", "1.234.567"),
            ("de", "17.968 €", "17.968,50 €", "4,5", "1.234.567"),
            ("fr", "17 968 €", "17 968,50 €", "4,5", "1 234 567"),
            ("ru", "17 968 €", "17 968,50 €", "4,5", "1 234 567"),
            ("uk", "17 968 €", "17 968,50 €", "4,5", "1 234 567"),
            ("pl", "17 968 €", "17 968,50 €", "4,5", "1 234 567"),
        ];
        for (lang, whole, cents, dec, km) in cases {
            // (the cases write the narrow space as a plain one, for reading)
            let fix = |s: &str| if style(lang).thousands == NARROW { s.replace(' ', n).replacen(&format!("{n}€"), " €", 1) } else { s.to_string() };
            assert_eq!(eur_in(17_968_00, lang), fix(whole), "{lang}");
            assert_eq!(eur_cents_in(17_968_50.0, lang), fix(cents), "{lang}");
            assert_eq!(num_in(4.5, 1, lang), dec, "{lang}");
            assert_eq!(num_in(1_234_567.0, 0, lang), fix(km), "{lang}");
        }
        // small amounts, negative ones, rounding
        assert_eq!(eur_in(90_00, "en"), "€90");
        assert_eq!(eur_in(-1_234_00, "nl"), "-€ 1.234");
        assert_eq!(eur_in(-1_234_00, "de"), "-1.234 €");
        assert_eq!(eur_cents_in(-50.0, "en"), "-€0.50");
        assert_eq!(num_in(-0.04, 1, "nl"), "0,0");
        assert_eq!(num_in(999.96, 1, "en"), "1,000.0");
        assert_eq!(num_in(f64::NAN, 0, "en"), "0");
    }

    #[test]
    fn the_company_opens_on_its_start_screen_and_a_shortcut_lands_anywhere() {
        assert_eq!(CompanyView::default().tab, HOME_TAB);
        assert_eq!(tab_of("home"), Some(HOME_TAB));
        assert_eq!(tab_of(" Start "), Some(HOME_TAB));
        assert_eq!(tab_of("7"), Some(DEPOT_TAB));
        assert_eq!(tab_of("99"), Some(TABS.len() - 1));
        assert_eq!(tab_of("fleet"), None);
        // (the start screen is no page of the tabs, and every page has its icon)
        assert!(HOME_TAB >= TABS.len());
        assert_eq!(ICONS.len(), TABS.len());
        assert_eq!(page_name(HOME_TAB), "Start screen");
        assert_eq!(page_name(INBOX_TAB), "Inbox");
        // the strip under the bar has every page once
        let mut all: Vec<usize> = RAIL.iter().flat_map(|g| g.iter().copied()).collect();
        all.sort_unstable();
        assert_eq!(all, (0..TABS.len()).collect::<Vec<_>>());
    }

    #[test]
    fn each_mode_shows_its_own_pages() {
        // the advanced mode: every page, as the strip has always had them
        assert_eq!(pages_of(co::Mode::Advanced), RAIL.iter().flat_map(|g| g.iter().copied()).collect::<Vec<_>>());
        // the easy mode: what a player who runs buses looks at; what runs by itself is left out
        let easy = pages_of(co::Mode::Easy);
        assert_eq!(easy, vec![0, INBOX_TAB, 3, MAP_TAB, 1, 2, 4, 6]);
        for hidden in [5, DEPOT_TAB, CONCESSIONS_TAB, RESEARCH_TAB, WORKSHOP_TAB, CONTRACTS_TAB] {
            assert!(!easy.contains(&hidden), "{}", TABS[hidden]);
        }
        // a way that mends something in the easy mode never goes to the planning
        use kit::Go;
        for g in [Go::Progress, Go::Depot, Go::Loan, Go::Finances, Go::Courses, Go::Planning, Go::Lines, Go::Dealer, Go::Hire, Go::Training, Go::Licences] {
            assert_ne!(easy_go(g), Some(Go::Planning), "{g:?}");
        }
        assert_eq!((easy_go(Go::Planning), easy_go(Go::Training), easy_go(Go::Dealer)), (None, Some(Go::Courses), Some(Go::Dealer)));
        // nor to the depot or the research, which it builds and starts by itself; a popup that
        // needs them offers the advanced mode instead
        assert_eq!((easy_go(Go::Depot), easy_go(Go::Research), easy_go(Go::Licences)), (None, None, None));
        assert_eq!(easy_go(Go::Advanced(DEPOT_TAB)), Some(Go::Advanced(DEPOT_TAB)));
    }

    #[test]
    fn escape_goes_back_to_the_start_screen_only_when_nothing_else_wants_it() {
        assert!(escape_home(3, true, false, false, false));
        // not pressed, already there, a dialog over the page, a text field, the planning's hand
        assert!(!escape_home(3, false, false, false, false));
        assert!(!escape_home(HOME_TAB, true, false, false, false));
        assert!(!escape_home(3, true, true, false, false));
        assert!(!escape_home(3, true, false, true, false));
        assert!(!escape_home(5, true, false, false, true));
    }
}
