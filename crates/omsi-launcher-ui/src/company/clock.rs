//! The company's clock on its pages: the company's now in the sheet's head with the steps to
//! simulate (+1 h, "Simulate to tomorrow", the others in the time dialog), the play mode that
//! runs the clock while the page is open (×60, ×600), the decisions a simulation waits for,
//! and the feed of what happened (the rules are `omsi_launcher_lib::company::clock`'s).
//!
//! A step within the company's day runs at once, on the page's own copy of the day's
//! timetable; a step past midnight closes days and runs on a thread of its own, as "Close the
//! day" did. While the game runs on the company's map its clock leads: the steps are not
//! offered, the company's day goes along with the game's time and what the game reports of
//! the company's buses is told as it comes.
//!
//! The company's time runs with the real clock (Luc: "a real-time clock that runs and is
//! always in step with the real clock, so that progress - besides fast-forwarding - also
//! happens when openOMSI is not running"): while its pages are open the clock goes at least as
//! the real one does (×1, "Stop" no more), and a company found behind its course - opened
//! again after a while - catches up on a thread under a cover, with a summary of what happened
//! while the player was away. Fast-forwarding moves it ahead of the real clock, the bar says by
//! how much. Not while the game leads: a duty never jumps.

use super::super::theme::*;
use super::super::ui::{ButtonKind, Ui};
use super::super::Launcher;
use super::kit::{self, Foot};
use super::{changed, data, day_label, eur, spawn, Dialog, Msg};
use glam::Vec2;
use omsi_launcher_lib::company::clock::{self as ck, Ask, Away, Choice, FeedItem, Level, Run, Step, DAY};
use omsi_launcher_lib::company::store::Disk;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

/// A catch-up come back from its thread: the company caught up and what it came to.
type CaughtUp = Result<(Company, Option<Away>), String>;

/// How long the real clock's steps wait after one failed (the timetable could not be read):
/// not tried again - and said again - every frame.
const HOLD_SECS: u64 = 300;

pub struct ClockView {
    /// Playing: company minutes per second of ours (0: the clock stands, or with the real
    /// clock goes as the real one does).
    pub(super) speed: f64,
    acc: f64,
    last: Option<Instant>,
    /// Changes of the clock not saved yet (a playing clock is saved now and then).
    dirty: Option<Instant>,
    /// The game: when the clock last followed it, and how many of its live events were told,
    /// by company and day (a company opened again tells none twice).
    follow_at: Option<Instant>,
    live_seen: HashMap<(String, String), usize>,
    /// The time dialog's "until": a minute of the day.
    pub(super) until: f32,
    /// A catch-up on its thread: how many minutes it catches up (the cover says it), and its
    /// way back.
    catching: Option<i64>,
    caught_tx: Sender<CaughtUp>,
    caught_rx: Receiver<CaughtUp>,
    /// What happened while the player was away: shown over the pages until it is closed.
    pub(super) away: Option<Away>,
    /// A step of the real clock failed: the next waits until then.
    held: Option<Instant>,
}

impl Default for ClockView {
    fn default() -> Self {
        let (caught_tx, caught_rx) = channel();
        ClockView { speed: 0.0, acc: 0.0, last: None, dirty: None, follow_at: None, live_seen: HashMap::new(), until: 16.0 * 60.0, catching: None, caught_tx, caught_rx, away: None, held: None }
    }
}

impl ClockView {
    /// A simulation failed: the real clock's steps wait a while before they try again.
    pub(super) fn hold(&mut self) {
        self.held = Some(Instant::now());
    }

    fn holding(&self) -> bool {
        self.held.is_some_and(|t| t.elapsed().as_secs() < HOLD_SECS)
    }
}

/// The real time now, in Unix seconds.
fn real_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The company's clock runs with the real one (its settings).
fn realtime(c: &Company) -> bool {
    c.clock.real.on
}

/// A span of minutes as the pages write it, its two largest parts: "2 d 4 h", "31 d",
/// "4 h 10 min", "3 h", "12 min".
pub(super) fn span_text(minutes: i64) -> String {
    let (d, h, m) = ck::span(minutes);
    let t = match (d, h, m) {
        (0, 0, _) => omsi_ui::tr("%{m} min"),
        (0, _, 0) => omsi_ui::tr("%{h} h"),
        (0, _, _) => omsi_ui::tr("%{h} h %{m} min"),
        (_, 0, _) => omsi_ui::tr("%{d} d"),
        _ => omsi_ui::tr("%{d} d %{h} h"),
    };
    t.replace("%{d}", &d.to_string()).replace("%{h}", &h.to_string()).replace("%{m}", &m.to_string())
}

/// The game runs on the company's map: its clock leads (the minute of its day). A game on
/// another map, or a test drive at the dealer's, runs outside the company's time
/// (`map::time_of_day`).
fn game_minute(l: &Launcher, c: &Company) -> Option<i64> {
    let (secs, in_game) = super::map::time_of_day(l, c);
    in_game.then_some((secs / 60.0).floor() as i64)
}

/// The game leads the open company's clock now.
fn game_leads(l: &Launcher) -> bool {
    l.company.company.as_ref().is_some_and(|c| game_minute(l, c).is_some())
}

/// A step may be taken now (no other running, no game leading).
pub(super) fn may_step(l: &Launcher) -> bool {
    !l.company.closing && !game_leads(l) && l.company.company.as_ref().is_some_and(|c| c.clock.ask.is_none())
}

/// Simulate to `to` (the clock's minutes): within the company's day at once, past its midnight
/// on a thread. `quick`: nothing waits, the dispatcher decides.
pub(super) fn simulate(l: &mut Launcher, to: i64, quick: bool) {
    let Some(c) = l.company.company.clone() else { return };
    if l.company.closing || game_minute(l, &c).is_some() {
        return;
    }
    let midnight = (ck::now(&c).div_euclid(DAY) + 1) * DAY;
    let lines = l.company.today.as_ref().filter(|t| t.map == c.map && t.date == c.date && t.error.is_none()).map(|t| t.lines.clone());
    if to < midnight {
        if let Some(lines) = lines {
            let mut c = c;
            let d = data();
            let mut w = Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), lines);
            match ck::advance(&mut c, to, &mut w, quick) {
                Ok(run) => adopt(l, c, run),
                Err(e) => {
                    l.company.clock.speed = 0.0;
                    l.company.clock.hold();
                    l.state.set_status(e, true);
                }
            }
            return;
        }
    }
    l.company.closing = true;
    l.company.clock.dirty = None;
    spawn(&l.company.tx, move || {
        let d = data();
        let mut c = c;
        let w = lines.map(|x| Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), x));
        match co::store::simulate(&d, &mut c, to, quick, w) {
            Ok(run) => {
                // (stepped ahead of the real clock: the lead kept, as the company is saved)
                if ck::keep_lead(&mut c, real_now()) {
                    if let Err(e) = co::store::save(&d, &c) {
                        log::warn!("company: saving the clock's lead: {e:#}");
                    }
                }
                Msg::Simulated(Ok((c, run)))
            }
            Err(e) => Msg::Simulated(Err(format!("{e:#}"))),
        }
    });
}

/// The company after a simulation: its days' reports shown, the decision it waits for asked,
/// saved (a running clock's quiet minutes now and then). Stepped ahead of the real clock, its
/// lead is kept.
pub(super) fn adopt(l: &mut Launcher, mut c: Company, run: Run) {
    ck::keep_lead(&mut c, real_now());
    let quiet = run.reports.is_empty() && !run.stopped && c.clock.feed.iter().rev().take(run.told).all(|f| f.level == Level::Minor);
    let running = l.company.clock.speed > 0.0 || realtime(&c);
    l.company.company = Some(c);
    if !run.reports.is_empty() {
        l.company.reports = Some(run.reports);
    }
    if quiet && running {
        let view = &mut l.company;
        if let (Some(c), Some(list)) = (view.company.as_ref(), view.companies.as_mut()) {
            if let Some(x) = list.iter_mut().find(|x| x.id == c.id) {
                *x = c.clone();
            }
        }
        if view.clock.dirty.is_none() {
            view.clock.dirty = Some(Instant::now());
        }
        return;
    }
    l.company.clock.dirty = None;
    changed(l);
}

/// Save what a playing clock left unsaved.
fn flush(l: &mut Launcher) {
    if l.company.clock.dirty.take().is_some() {
        changed(l);
    }
}

/// Every frame on the company's pages: the clock with the real one (in step from the first
/// time it is opened; behind it, caught up), the playing clock, and the game's clock leading.
pub fn tick(l: &mut Launcher, modal: bool) {
    let now = Instant::now();
    let dt = l.company.clock.last.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0).min(0.5);
    l.company.clock.last = Some(now);
    caught_up(l);
    let Some(c) = l.company.company.clone() else { return };
    let real = real_now();
    // the game leads (and the company goes along with it: no catch-up in the middle of a duty)
    if game_minute(l, &c).is_some() {
        l.company.clock.speed = 0.0;
        if l.company.clock.follow_at.is_none_or(|t| t.elapsed().as_secs_f32() >= 2.0) {
            l.company.clock.follow_at = Some(now);
            follow(l, &c);
        }
        return;
    }
    let live = realtime(&c) && !l.company.clock.holding();
    let running = l.company.clock.speed > 0.0 || realtime(&c);
    if l.company.clock.dirty.is_some_and(|t| t.elapsed().as_secs_f32() >= 8.0) || (!running && l.company.clock.dirty.is_some()) {
        flush(l);
    }
    if l.company.closing {
        return;
    }
    // never in step yet (a new company, an older file, the setting just turned on): from now
    if live && c.clock.real.synced.is_none() {
        if let Some(x) = l.company.company.as_mut() {
            ck::sync(x, real);
        }
        changed(l);
        return;
    }
    // behind the real clock for a while (opened again, a decision that waited): caught up at
    // once, the dispatcher deciding - also over a decision left waiting, not over a dialog
    let other = l.company.dialog.is_some() || l.company.reports.is_some() || l.company.popup.is_some() || l.company.clock.away.is_some();
    if live && !other && ck::behind(&c, real) >= ck::AWAY_TOLD {
        catch_up(l, c, real);
        return;
    }
    if modal {
        return;
    }
    if c.clock.ask.is_some() {
        l.company.clock.speed = 0.0;
        return;
    }
    if l.company.clock.speed <= 0.0 {
        // ×1: in step with the real clock (a step within the day once its timetable is read)
        let to = ck::course(&c, real).unwrap_or(i64::MIN);
        if live && to > ck::now(&c) {
            let midnight = (ck::now(&c).div_euclid(DAY) + 1) * DAY;
            let known = l.company.today.as_ref().is_some_and(|t| t.map == c.map && t.date == c.date && t.error.is_none());
            if known || to >= midnight {
                simulate(l, to, false);
            }
        }
        return;
    }
    l.ui.keep_moving();
    let view = &mut l.company.clock;
    view.acc += dt * view.speed;
    if view.acc < 1.0 {
        return;
    }
    let steps = view.acc.floor() as i64;
    view.acc -= steps as f64;
    simulate(l, ck::now(&c) + steps, false);
}

/// The company caught up with the real clock on a thread, under a cover (`ck::catch_up`).
fn catch_up(l: &mut Launcher, c: Company, real: i64) {
    l.company.clock.speed = 0.0;
    l.company.clock.catching = Some(ck::behind(&c, real).min(ck::CATCH_UP_MAX));
    l.company.closing = true;
    // (the thread saves the company with what the running clock left unsaved: no older copy
    // saved over it meanwhile)
    l.company.clock.dirty = None;
    let lines = l.company.today.as_ref().filter(|t| t.map == c.map && t.date == c.date && t.error.is_none()).map(|t| t.lines.clone());
    let tx = l.company.clock.caught_tx.clone();
    std::thread::spawn(move || {
        let d = data();
        let mut c = c;
        let w = lines.map(|x| Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), x));
        let r = co::store::catch_up(&d, &mut c, real, w).map(|away| (c, away)).map_err(|e| format!("{e:#}"));
        let _ = tx.send(r);
    });
}

/// A catch-up came back: the company taken over (saved already), what happened while the
/// player was away shown.
fn caught_up(l: &mut Launcher) {
    while let Ok(r) = l.company.clock.caught_rx.try_recv() {
        l.company.closing = false;
        l.company.clock.catching = None;
        match r {
            Ok((c, away)) => {
                let view = &mut l.company;
                if let Some(list) = view.companies.as_mut() {
                    if let Some(x) = list.iter_mut().find(|x| x.id == c.id) {
                        *x = c.clone();
                    }
                }
                // (the company still open - nothing else can be while the cover is up)
                if view.company.as_ref().is_some_and(|x| x.id == c.id) {
                    view.company = Some(c);
                    view.plan = None;
                    view.clock.away = away.filter(|a| a.caught.to - a.caught.from >= ck::AWAY_TOLD);
                }
            }
            Err(e) => {
                l.company.clock.hold();
                l.state.set_status(e, true);
            }
        }
    }
}

/// What the cover over the pages says while the company's time is simulated.
pub(super) fn cover_text(l: &Launcher) -> String {
    match l.company.clock.catching {
        Some(m) => omsi_ui::tr("Catching up %{span} of the company's time…").replace("%{span}", &span_text(m)),
        None => omsi_ui::tr("Simulating the company's time…").into_owned(),
    }
}

/// The game's minute taken over, and what it reported told; the company's course against the
/// real clock goes on from the game's time.
fn follow(l: &mut Launcher, c: &Company) {
    let Some(minute) = game_minute(l, c) else { return };
    let mut c = c.clone();
    let key = (c.id.clone(), c.date.clone());
    // (the company's days before are closed: their events are booked)
    let seen = &mut l.company.clock.live_seen;
    seen.retain(|k, _| k.0 != key.0 || k.1 == key.1);
    let already = seen.get(&key).copied().unwrap_or(0);
    let d = data();
    let live = co::store::peek_live(&d, &c.id, already);
    let mut told = 0;
    if !live.is_empty() {
        l.company.clock.live_seen.insert(key, already + live.len());
        told += ck::tell_live(&mut c, &live);
    }
    let lines = l.company.today.as_ref().filter(|t| t.map == c.map && t.date == c.date && t.error.is_none()).map(|t| t.lines.clone());
    if let Some(lines) = lines {
        let mut w = Disk::new(&d).knowing(&c.map.clone(), &c.date.clone(), lines);
        match ck::follow(&mut c, minute, &mut w) {
            Ok(run) => told += run.told,
            Err(e) => log::warn!("company: following the game's clock: {e}"),
        }
    }
    let synced = ck::with_game(&mut c, real_now());
    if told > 0 || synced || ck::now(&c) != ck::now(l.company.company.as_ref().unwrap_or(&c)) {
        l.company.company = Some(c);
        changed(l);
    }
}

/// The company's now as the head shows it.
pub(super) fn now_label(c: &Company) -> String {
    format!("{}  ·  {}", day_label(&c.date), ck::hhmm(ck::now(c)))
}

/// The head: the company's day and its time, the play buttons, +1 h, "Simulate to tomorrow"
/// and the time dialog. Returns where it begins.
pub fn head(l: &mut Launcher, r: Rect) -> f32 {
    let Some(c) = l.company.company.clone() else { return r.right() };
    let in_game = game_leads(l);
    let ok = may_step(l);
    let gap = 8.0;
    let menu = Rect::new(r.right() - 42.0, r.y, 42.0, r.h);
    let tomorrow_w = Foot::width(&l.ui, "Simulate to tomorrow", Some("nights_stay"));
    let tomorrow = Rect::new(menu.x - gap - tomorrow_w, r.y, tomorrow_w, r.h);
    let hour = Rect::new(tomorrow.x - gap - 70.0, r.y, 70.0, r.h);
    let mut x = hour.x;
    if l.ui.button("company-time", menu, "", Some("schedule"), ButtonKind::Normal) && !l.company.closing {
        l.company.dialog = Some(Dialog::Time);
    }
    l.ui.tooltip(menu, "More steps, the clock's speed and the dispatcher");
    if l.ui.button("company-tomorrow", tomorrow, "Simulate to tomorrow", Some("nights_stay"), ButtonKind::Primary) {
        if ok {
            let to = ck::target(&c, Step::Midnight);
            simulate(l, to, false);
        } else {
            busy(l);
        }
    }
    l.ui.tooltip(tomorrow, if in_game { "The game leads the company's clock while it runs" } else { "Simulate the rest of the day: the tours run, what happens is told, and the day is closed at midnight" });
    if l.ui.button("company-hour", hour, "+1 h", None, ButtonKind::Normal) {
        if ok {
            let to = ck::target(&c, Step::Minutes(60));
            simulate(l, to, false);
        } else {
            busy(l);
        }
    }
    l.ui.tooltip(hour, "Simulate one hour of the company's time");
    // the play buttons, where there is room
    if r.w >= 640.0 {
        for (k, (label, speed)) in [("×600", 600.0 / 60.0), ("×60", 1.0)].into_iter().enumerate() {
            let w = if k == 0 { 70.0 } else { 62.0 };
            let b = Rect::new(x - gap - w, r.y, w, r.h);
            x = b.x;
            let on = (l.company.clock.speed - speed).abs() < 1e-6;
            if l.ui.button(&format!("company-play-{k}"), b, label, None, if on { ButtonKind::Primary } else { ButtonKind::Ghost }) {
                if ok || on {
                    play(l, if on { 0.0 } else { speed });
                } else {
                    busy(l);
                }
            }
            let tip = match (on, k) {
                (true, _) if realtime(&c) => "Back to the real clock's pace",
                (true, _) => "Stop the clock",
                (false, 0) => "Run the company's clock at ten minutes a second while this page is open",
                (false, _) => "Run the company's clock at a minute a second while this page is open",
            };
            l.ui.tooltip(b, tip);
        }
    }
    // the day and the time; with the real clock its live dot, and how far fast-forwarding
    // moved the company ahead of it
    let date = day_label(&c.date);
    let time = ck::hhmm(ck::now(&c));
    let label = omsi_ui::tr(if in_game { "Company time · the game's" } else { "Company time" }).to_uppercase();
    let live = realtime(&c) && !in_game;
    let lead = if live { c.clock.real.lead } else { 0 };
    let lead_t = format!("+{}", span_text(lead));
    let tw = l.ui.width(&time, 22.0, Weight::Bold);
    let lw = l.ui.width(&label, 11.5, Weight::Bold);
    let lead_w = if lead > 0 { l.ui.width(&lead_t, 11.5, Weight::Bold) + 8.0 } else { 0.0 };
    let top_w = lw + if live { 14.0 + lead_w } else { 0.0 };
    let dw = (l.ui.width(&date, 14.0, Weight::Medium) + tw + 12.0).max(top_w) + 8.0;
    let dx = x - 16.0 - dw;
    if dx < r.x {
        return x;
    }
    l.ui.text_in(&label, Rect::new(dx, r.y - 3.0, dw, 15.0), 11.5, Weight::Bold, if in_game { OK } else { TEXT_DIM }, Align::Right);
    if live {
        let dot = Vec2::new(dx + dw - lw - 9.0, r.y + 4.5);
        l.ui.p().circle(dot, 3.5, OK);
        if lead > 0 {
            l.ui.text_in(&lead_t, Rect::new(dx, r.y - 3.0, (dot.x - 7.0 - dx).max(0.0), 15.0), 11.5, Weight::Bold, accent_2(), Align::Right);
        }
    }
    l.ui.text_in(&time, Rect::new(dx, r.y + 12.0, dw, 28.0), 22.0, Weight::Bold, if l.company.clock.speed > 0.0 { accent_2() } else { TEXT }, Align::Right);
    l.ui.text_in(&date, Rect::new(dx, r.y + 14.0, dw - tw - 12.0, 26.0), 14.0, Weight::Medium, TEXT_SOFT, Align::Right);
    if !in_game {
        l.ui.tooltip(Rect::new(dx, r.y - 3.0, dw, r.h + 3.0), &realtime_tip(&c));
    }
    dx
}

/// What the clock's head says of the real clock under the mouse.
fn realtime_tip(c: &Company) -> String {
    if !realtime(c) {
        return omsi_ui::tr("The company's time stands still while it is closed: the real clock is off in the company's settings.").into_owned();
    }
    let mut t = omsi_ui::tr("In step with the real clock: the company's time goes on while openOMSI is closed, and is caught up when you open the company.").into_owned();
    if c.clock.real.lead > 0 {
        t.push('\n');
        t.push_str(&omsi_ui::tr("Fast-forwarded: %{span} ahead of the real clock.").replace("%{span}", &span_text(c.clock.real.lead)));
    }
    t
}

/// Why the clock cannot be stepped now, in a popup.
pub(super) fn busy(l: &mut Launcher) {
    let p = if game_leads(l) {
        kit::Popup::new("sports_esports", "The game leads the clock", omsi_ui::tr("The game runs on the company's map: the company's day goes along with the game's time."), omsi_ui::tr("Simulating is offered again when the game is closed."), None)
    } else if l.company.company.as_ref().is_some_and(|c| c.clock.ask.is_some()) {
        kit::Popup::new("help", "A decision waits", omsi_ui::tr("The company's clock waits for your answer first."), "", None)
    } else {
        kit::Popup::new("timer", "The clock is running", omsi_ui::tr("The company's time is being simulated: wait a moment."), "", None)
    };
    kit::show(l, p);
}

/// The clock runs (company minutes a second) or stands.
pub(super) fn play(l: &mut Launcher, speed: f64) {
    l.company.clock.speed = speed;
    l.company.clock.acc = 0.0;
    l.company.clock.last = Some(Instant::now());
    if speed == 0.0 {
        flush(l);
    }
}

/// The time dialog: every step, "until", the clock's speed, the dispatcher, the quick close.
pub fn time_dialog(l: &mut Launcher) {
    let Some(c) = l.company.company.clone() else {
        l.company.dialog = None;
        return;
    };
    let title = omsi_ui::tr("Company time: %{now}").replace("%{now}", &now_label(&c));
    let in_game = game_leads(l);
    let f = kit::frame(l, 780.0, if in_game { 680.0 } else { 620.0 }, "schedule", &title);
    let inner = f.body;
    let ok = may_step(l);
    let mut y = inner.y;
    if in_game {
        y += l.ui.paragraph("The game runs on the company's map: its clock leads, and the company's day goes along with it. Simulating is offered again when the game is closed.", Vec2::new(inner.x, y), inner.w, kit::BODY, Weight::Regular, WARN) + 12.0;
    }
    let heading = |l: &mut Launcher, y: f32, t: &str| {
        kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), t);
    };
    heading(l, y, "Simulate");
    y += 26.0;
    let steps: [(&str, Step); 9] = [
        ("+15 min", Step::Minutes(15)),
        ("+1 h", Step::Minutes(60)),
        ("+6 h", Step::Minutes(360)),
        ("To the morning", Step::Morning),
        ("To tomorrow", Step::Midnight),
        ("+1 day", Step::Days(1)),
        ("+2 days", Step::Days(2)),
        ("+3 days", Step::Days(3)),
        ("+7 days", Step::Days(7)),
    ];
    let cols = 5;
    let bw = (inner.w - (cols - 1) as f32 * 8.0) / cols as f32;
    let mut chosen: Option<Step> = None;
    for (k, (label, step)) in steps.iter().enumerate() {
        let b = Rect::new(inner.x + (k % cols) as f32 * (bw + 8.0), y + (k / cols) as f32 * 46.0, bw, 38.0);
        if l.ui.button(&format!("company-step-{k}"), b, label, None, ButtonKind::Normal) {
            if ok {
                chosen = Some(*step);
            } else {
                busy(l);
            }
        }
        let to = ck::target(&c, *step);
        l.ui.tooltip(b, &format!("{}  {}", day_label(&ck::date_of(to)), ck::hhmm(to)));
    }
    y += 2.0 * 46.0 + 10.0;
    // until a time
    let mut m = l.company.clock.until;
    let fmt = |v: f32| ck::hhmm(v as i64);
    let label = omsi_ui::tr("Simulate until %{time}").replace("%{time}", &ck::hhmm(m as i64));
    let gw = Foot::width(&l.ui, &label, Some("schedule")) + 10.0;
    l.ui.slider("company-until", Rect::new(inner.x, y, inner.w - gw - 14.0, 40.0), &mut m, 0.0, (DAY - 15) as f32, 15.0, "Until", &fmt);
    l.company.clock.until = m;
    if l.ui.button("company-until-go", Rect::new(inner.right() - gw, y, gw, 40.0), &label, Some("schedule"), ButtonKind::Normal) {
        if ok {
            chosen = Some(Step::Until(m as i64));
        } else {
            busy(l);
        }
    }
    y += 40.0 + 24.0;
    heading(l, y, "Run the clock");
    y += 26.0;
    // (with the real clock the slowest is the real one's pace: ×1, in step with it)
    let real = realtime(&c);
    let speeds = [(if real { "×1" } else { "Stop" }, 0.0), ("×60", 1.0), ("×600", 10.0)];
    let sw = 110.0;
    for (k, (label, speed)) in speeds.iter().enumerate() {
        let on = (l.company.clock.speed - speed).abs() < 1e-6;
        let b = Rect::new(inner.x + k as f32 * (sw + 8.0), y, sw, 38.0);
        if l.ui.button(&format!("company-speed-{k}"), b, label, None, if on { ButtonKind::Primary } else { ButtonKind::Normal }) {
            if ok || *speed == 0.0 {
                play(l, *speed);
            } else {
                busy(l);
            }
        }
        if real && k == 0 {
            l.ui.tooltip(b, "In step with the real clock");
        }
    }
    let say = if real { "×1: in step with the real clock. ×60: a minute a second; it stops for what needs you." } else { "×60: a minute a second; it stops for what needs you." };
    l.ui.paragraph(say, Vec2::new(inner.x + 3.0 * (sw + 8.0) + 10.0, y + 2.0), inner.w - 3.0 * (sw + 8.0) - 10.0, kit::NOTE, Weight::Regular, TEXT_SOFT);
    y += 38.0 + 12.0;
    // the real clock: whether the company runs with it, and its lead
    let words = realtime_tip(&c).replace('\n', " ");
    l.ui.icon(if real { "schedule" } else { "pause_circle" }, Vec2::new(inner.x + 9.0, y + 10.0), 17.0, if real { OK } else { TEXT_DIM });
    y += l.ui.paragraph(&words, Vec2::new(inner.x + 26.0, y), inner.w - 26.0, kit::NOTE, Weight::Regular, TEXT_SOFT).max(20.0) + 16.0;
    heading(l, y, "Decisions");
    y += 26.0;
    let mut auto = c.clock.dispatcher;
    if l.ui.toggle("company-dispatcher", Rect::new(inner.x, y, inner.w, ROW), &mut auto, "The dispatcher decides breakdowns (a rental bus while the cash allows)") {
        if let Some(c) = l.company.company.as_mut() {
            c.clock.dispatcher = auto;
        }
        changed(l);
    }
    y += ROW + 8.0;
    let _ = y;
    // the quick close, and done
    let mut foot = Foot::new(&f);
    if foot.left(l, "company-quick-close", "Close the day quickly", Some("check_circle"), ButtonKind::Normal) {
        if ok {
            let to = ck::target(&c, Step::Midnight);
            l.company.dialog = None;
            simulate(l, to, true);
            return;
        }
        busy(l);
    }
    l.ui.tooltip(Rect::new(f.foot.x, f.foot.y, 260.0, f.foot.h), "The rest of the day at once: nothing stops, the dispatcher decides");
    if foot.right(l, "company-time-done", "Close", None, ButtonKind::Primary) || f.close {
        l.company.dialog = None;
    }
    if let Some(step) = chosen {
        l.company.dialog = None;
        let to = ck::target(&c, step);
        simulate(l, to, false);
    }
}

/// A feed line's words, with its places filled.
pub(super) fn text(f: &FeedItem) -> String {
    let mut t = omsi_ui::tr(&f.text).into_owned();
    for (k, v) in &f.args {
        let v = match k.as_str() {
            "amount" => v.parse::<i64>().map(eur).unwrap_or_else(|_| v.clone()),
            "date" => day_label(v),
            "month" => super::month_label(v),
            "why" | "job" | "area" => omsi_ui::tr(v).into_owned(),
            "bus" if v == "rental" => omsi_ui::tr("rental bus").into_owned(),
            _ => v.clone(),
        };
        t = t.replace(&format!("%{{{k}}}"), &v);
    }
    t
}

pub(super) fn colour(level: Level) -> Color {
    match level {
        Level::Minor => TEXT_DIM,
        Level::Info => TEXT_SOFT,
        Level::Good => OK,
        Level::Warn => WARN,
        Level::Bad => DANGER.lighten(0.25),
    }
}

/// The feed: what happened, the newest first (the ordinary run of the day only when asked).
pub fn feed(l: &mut Launcher, r: Rect, c: &Company, minor: &mut bool) {
    let inner = super::section(&mut l.ui, r, "The company's day");
    let tw = 170.0;
    let mut m = *minor;
    let tr = Rect::new(inner.right() - tw, r.y + 9.0, tw, 26.0);
    if l.ui.toggle("company-feed-minor", tr, &mut m, "All tours") {
        *minor = m;
    }
    l.ui.tooltip(tr, "Show every tour leaving and coming back too");
    let items: Vec<FeedItem> = c.clock.feed.iter().rev().filter(|f| *minor || f.level != Level::Minor).take(120).cloned().collect();
    if items.is_empty() && c.clock.feed.iter().any(|f| f.at >= ck::moment(&c.date, 0)) {
        l.ui.paragraph("The day runs as planned so far. \"All tours\" shows every tour leaving and coming back.", Vec2::new(inner.x, inner.y), inner.w, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    if items.is_empty() {
        l.ui.paragraph("Nothing happened yet. Simulate the company's time at the top: by the hour, to the morning, to tomorrow or days at once; what happens is told here.", Vec2::new(inner.x, inner.y), inner.w, kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    let today = c.date.clone();
    l.ui.scroll_area("company-feed", inner, &mut |ui: &mut Ui, v: Rect| {
        let mut y = v.y;
        let mut day = String::new();
        for f in &items {
            let d = ck::date_of(f.at);
            if d != day {
                day = d.clone();
                if d != today {
                    ui.text_in(&day_label(&d).to_uppercase(), Rect::new(v.x, y + 6.0, v.w, 16.0), kit::CAPS, Weight::Bold, TEXT_DIM, Align::Left);
                    y += 28.0;
                }
            }
            let t = text(f);
            ui.text_in(&ck::hhmm(f.at), Rect::new(v.x, y, 52.0, 21.0), kit::ROWS, Weight::Bold, TEXT_DIM, Align::Left);
            ui.p().circle(Vec2::new(v.x + 62.0, y + 10.5), 4.0, colour(f.level));
            let h = ui.paragraph(&t, Vec2::new(v.x + 76.0, y), v.w - 86.0, kit::ROWS, Weight::Regular, if f.level == Level::Minor { TEXT_DIM } else { TEXT });
            y += h.max(21.0) + 8.0;
        }
        y - v.y + 6.0
    });
}

/// What the simulation waits for: a breakdown (a rental bus or the trips dropped), a tender
/// opened or the player's bid beaten (to the auction, or on), the morning's disruptions.
pub fn ask_dialog(l: &mut Launcher) {
    let Some(c) = l.company.company.clone() else { return };
    let Some(ask) = c.clock.ask.clone() else { return };
    let when = ck::hhmm(ck::now(&c));
    let mut answer: Option<(Option<Choice>, bool)> = None;
    match &ask {
        Ask::Breakdown { index } => {
            let Some(b) = c.clock.today.as_ref().and_then(|s| s.breaks.get(*index)).cloned() else {
                answer = Some((None, true));
                return finish(l, answer);
            };
            let title = omsi_ui::tr("%{time}  Bus %{bus} broke down").replace("%{time}", &when).replace("%{bus}", &b.bus);
            let cost = ck::rental_cost(&c, *index);
            let back = ck::hhmm(ck::moment(&c.date, (b.at + ck::RENTAL_MINUTES) as i64));
            let t = omsi_ui::tr("It was on line %{n}; it goes to the workshop. A rental bus can take over its trips from %{time} for %{amount}; else its trips are dropped for the rest of the day, with the contract's penalty.").replace("%{n}", &b.line).replace("%{time}", &back).replace("%{amount}", &eur(cost));
            let th = l.ui.paragraph_height(&t, 640.0 - 56.0, kit::BODY, Weight::Regular);
            // (a decision the clock waits for: no cross, both answers lead on)
            let f = kit::frame_with(l, 640.0, 70.0 + th + 20.0 + ROW + 24.0 + kit::BUTTON_H + 26.0, "warning", &title, false);
            l.ui.paragraph(&t, Vec2::new(f.body.x, f.body.y), f.body.w, kit::BODY, Weight::Regular, TEXT_SOFT);
            let mut auto = c.clock.dispatcher;
            if l.ui.toggle("company-ask-dispatcher", Rect::new(f.body.x, f.body.y + th + 16.0, f.body.w, ROW), &mut auto, "From now on the dispatcher decides") {
                if let Some(c) = l.company.company.as_mut() {
                    c.clock.dispatcher = auto;
                }
            }
            let mut foot = Foot::new(&f);
            let label = omsi_ui::tr("Rental bus, %{amount}").replace("%{amount}", &eur(cost));
            if foot.right(l, "company-ask-rental", &label, Some("directions_bus"), ButtonKind::Primary) {
                answer = Some((Some(Choice::Rental), true));
            }
            if foot.right(l, "company-ask-drop", "Drop its trips", None, ButtonKind::Normal) {
                answer = Some((Some(Choice::Drop), true));
            }
        }
        Ask::Tender { id, outbid } => {
            let Some(t) = c.concessions.tenders.iter().find(|t| t.id == *id).cloned() else {
                return finish(l, Some((None, true)));
            };
            let title = if *outbid { omsi_ui::tr("%{time}  Your bid for line %{n} is beaten") } else { omsi_ui::tr("%{time}  Line %{n} is out to tender") }.replace("%{time}", &when).replace("%{n}", &t.number);
            let left = (t.closes_at - ck::now(&c)).max(0);
            let mut s = omsi_ui::tr("Bids until %{time} (%{left} from now). The least bid now: %{amount}; buying it outright: %{buy}.").replace("%{time}", &ck::hhmm(t.closes_at)).replace("%{left}", &format!("{}:{:02} h", left / 60, left % 60)).replace("%{amount}", &eur(co::concessions::min_bid(&c, &t))).replace("%{buy}", &eur(t.buy_out()));
            if *outbid {
                let placed = co::concessions::bids(&c, &t, ck::now(&c));
                if let Some(p) = placed.last() {
                    s = format!("{}  {}", omsi_ui::tr("%{who} bids %{amount}.").replace("%{who}", &co::concessions::bidder_name(&c, &t, p.who)).replace("%{amount}", &eur(p.amount)), s);
                }
            }
            let th = l.ui.paragraph_height(&s, 640.0 - 56.0, kit::BODY, Weight::Regular);
            let f = kit::frame(l, 640.0, 70.0 + th + 24.0 + kit::BUTTON_H + 26.0, "payments", &title);
            l.ui.paragraph(&s, Vec2::new(f.body.x, f.body.y), f.body.w, kit::BODY, Weight::Regular, TEXT_SOFT);
            let mut foot = Foot::new(&f);
            // (a question left from the advanced mode: the easy mode has no auctions to go to)
            if super::shown(l, super::CONCESSIONS_TAB) && foot.right(l, "company-ask-auction", "To the auction", Some("payments"), ButtonKind::Primary) {
                l.company.tab = super::CONCESSIONS_TAB;
                l.company.tenders.selected = Some(*id);
                answer = Some((None, false));
            }
            if foot.right(l, "company-ask-on", if *outbid { "Let it go" } else { "Go on" }, None, ButtonKind::Normal) {
                answer = Some((None, true));
            }
            if f.close && answer.is_none() {
                answer = Some((None, false));
            }
        }
        Ask::Morning { count } => {
            let title = omsi_ui::tr("%{time}  The morning").replace("%{time}", &when);
            let t = omsi_ui::tr("%{n} things fell out this morning: drivers late, buses that did not start. The central fills them as it can; the planning shows them and lets you choose.").replace("%{n}", &count.to_string());
            let th = l.ui.paragraph_height(&t, 640.0 - 56.0, kit::BODY, Weight::Regular);
            let f = kit::frame(l, 640.0, 70.0 + th + 24.0 + kit::BUTTON_H + 26.0, "wb_twilight", &title);
            l.ui.paragraph(&t, Vec2::new(f.body.x, f.body.y), f.body.w, kit::BODY, Weight::Regular, TEXT_SOFT);
            let mut foot = Foot::new(&f);
            if super::shown(l, 5) && foot.right(l, "company-ask-plan", "Open the planning", Some("event"), ButtonKind::Primary) {
                l.company.tab = 5;
                answer = Some((None, false));
            }
            if foot.right(l, "company-ask-on", "Go on", None, ButtonKind::Normal) {
                answer = Some((None, true));
            }
            if f.close && answer.is_none() {
                answer = Some((None, false));
            }
        }
        Ask::Notice { text, args } => {
            let item = FeedItem { at: ck::now(&c), text: text.clone(), args: args.clone(), level: Level::Bad };
            let t = super::clock::text(&item);
            let th = l.ui.paragraph_height(&t, 600.0 - 56.0, kit::BODY, Weight::Regular);
            let f = kit::frame(l, 600.0, 70.0 + th + 24.0 + kit::BUTTON_H + 26.0, "info", &when);
            l.ui.paragraph(&t, Vec2::new(f.body.x, f.body.y), f.body.w, kit::BODY, Weight::Regular, TEXT_SOFT);
            let mut foot = Foot::new(&f);
            if foot.right(l, "company-ask-ok", "Go on", None, ButtonKind::Primary) {
                answer = Some((None, true));
            }
            if f.close && answer.is_none() {
                answer = Some((None, false));
            }
        }
    }
    finish(l, answer);
}

/// The answer given: applied, and the simulation goes on where it was going (`on`), unless the
/// clock plays (it goes on by itself) or the player went to look.
fn finish(l: &mut Launcher, answer: Option<(Option<Choice>, bool)>) {
    let Some((choice, on)) = answer else { return };
    let Some(c) = l.company.company.as_mut() else { return };
    let mut run = Run::default();
    let to = ck::answer(c, choice, &mut run);
    changed(l);
    if on && l.company.clock.speed == 0.0 {
        if let Some(to) = to {
            simulate(l, to, false);
        }
    }
}

// --- the real clock on the pages ------------------------------------------------------------------

/// What happened while the player was away (a catch-up of `ck::AWAY_TOLD` or more): how long
/// the company's time ran on, the days closed and their money, the cash, and what of note
/// happened on the way, the newest first; the days' report a click away.
pub fn away_dialog(l: &mut Launcher) {
    let Some(a) = l.company.clock.away.clone() else { return };
    let Some(c) = l.company.company.clone() else {
        l.company.clock.away = None;
        return;
    };
    let k = &a.caught;
    let at = |m: i64| format!("{}  {}", day_label(&ck::date_of(m)), ck::hhmm(m));
    let ran = omsi_ui::tr("The company's time ran on with the real clock: %{span}, from %{from} to %{until}. The dispatcher decided what came up.").replace("%{span}", &span_text(k.to - k.from)).replace("%{from}", &at(k.from)).replace("%{until}", &at(k.to));
    let n = (ck::CATCH_UP_MAX / DAY).to_string();
    let skipped = (k.skipped > 0).then(|| omsi_ui::tr("You were away for longer than %{n} days: the company caught up %{n} days, and the rest was skipped.").replace("%{n}", &n));
    // (as high as what it says: a quiet absence is a small dialog)
    let (w, body_w) = (860.0, 860.0 - 56.0);
    let ran_h = l.ui.paragraph_height(&ran, body_w, kit::BODY, Weight::Regular) + 10.0;
    let skip_h = skipped.as_ref().map_or(0.0, |t| l.ui.paragraph_height(t, body_w - 28.0, kit::BODY, Weight::Regular) + 10.0);
    let list_h = if a.notable.is_empty() { 40.0 } else { (a.notable.len() as f32 * 34.0 + 10.0).min(420.0) };
    let h = 70.0 + ran_h + skip_h + 6.0 + kit::FIGURE_H + 20.0 + 26.0 + list_h + 18.0 + kit::BUTTON_H + 24.0;
    let f = kit::frame(l, w, h, "history", &omsi_ui::tr("While you were away"));
    let inner = f.body;
    let mut y = inner.y;
    y += l.ui.paragraph(&ran, Vec2::new(inner.x, y), inner.w, kit::BODY, Weight::Regular, TEXT) + 10.0;
    if let Some(t) = &skipped {
        l.ui.icon("warning", Vec2::new(inner.x + 9.0, y + 10.0), 18.0, WARN);
        y += l.ui.paragraph(t, Vec2::new(inner.x + 28.0, y), inner.w - 28.0, kit::BODY, Weight::Regular, WARN) + 10.0;
    }
    y += 6.0;
    // the figures: the days closed, their result, the cash
    let gap = 12.0;
    let fw = (inner.w - 2.0 * gap) / 3.0;
    super::figure(&mut l.ui, Rect::new(inner.x, y, fw, kit::FIGURE_H), "Days closed", &k.days.to_string(), "", TEXT);
    let (res, res_c) = kit::signed(k.result);
    super::figure(&mut l.ui, Rect::new(inner.x + fw + gap, y, fw, kit::FIGURE_H), "Result", &res, "", if k.days == 0 { TEXT_DIM } else { res_c });
    let change = omsi_ui::tr("%{amount} while you were away").replace("%{amount}", &kit::signed(c.cash - a.cash_before).0);
    super::figure(&mut l.ui, Rect::new(inner.x + 2.0 * (fw + gap), y, fw, kit::FIGURE_H), "Cash", &eur(c.cash), &change, if c.cash < 0 { DANGER.lighten(0.2) } else { TEXT });
    y += kit::FIGURE_H + 20.0;
    // what of note happened, the newest first
    kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "What happened");
    y += 26.0;
    let list = Rect::new(inner.x, y, inner.w, (inner.bottom() - y).max(0.0));
    if a.notable.is_empty() {
        l.ui.paragraph("Nothing out of the ordinary: the tours ran as planned. The feed and the inbox have every line.", Vec2::new(list.x, list.y), list.w, kit::BODY, Weight::Regular, TEXT_SOFT);
    } else {
        let items: Vec<FeedItem> = a.notable.iter().rev().cloned().collect();
        l.ui.scroll_area("company-away", list, &mut |ui: &mut Ui, v: Rect| {
            let mut yy = v.y;
            for f in &items {
                let when = format!("{}  {}", day_label(&ck::date_of(f.at)), ck::hhmm(f.at));
                ui.text_in(&when, Rect::new(v.x, yy, 170.0, 21.0), kit::ROWS, Weight::Bold, TEXT_DIM, Align::Left);
                ui.p().circle(Vec2::new(v.x + 182.0, yy + 10.5), 4.0, colour(f.level));
                let h = ui.paragraph(&text(f), Vec2::new(v.x + 196.0, yy), v.w - 206.0, kit::ROWS, Weight::Regular, TEXT);
                yy += h.max(21.0) + 8.0;
            }
            yy - v.y + 6.0
        });
    }
    let mut foot = Foot::new(&f);
    let report = !a.reports.is_empty() && foot.left(l, "company-away-report", "The days' report", Some("receipt_long"), ButtonKind::Normal);
    let done = foot.right(l, "company-away-ok", "Go on", None, ButtonKind::Primary);
    if report {
        l.company.reports = Some(a.reports.clone());
    }
    if report || done || f.close || l.ui.input.keys.contains(&super::super::ui::Key::Enter) {
        l.company.clock.away = None;
    }
}

/// The company's settings' switch of the real clock: whether its time runs with the real one,
/// also while openOMSI is closed. Returns the height it took.
pub(super) fn realtime_switch(l: &mut Launcher, r: Rect) -> f32 {
    let Some(mut on) = l.company.company.as_ref().map(realtime) else { return 0.0 };
    if l.ui.toggle("company-settings-realtime", Rect::new(r.x, r.y, r.w, ROW), &mut on, "Run with the real clock, also while openOMSI is closed") {
        super::act(l, |c| {
            ck::set_realtime(c, on);
            Ok(())
        });
    }
    let say = if on { "When you open the company, the time since is simulated, a month at most. Fast-forwarding moves it ahead of the real clock." } else { "Off: the company's time stands still while it is closed." };
    let h = l.ui.paragraph(say, Vec2::new(r.x, r.y + ROW + 4.0), r.w, kit::NOTE, Weight::Regular, TEXT_SOFT);
    ROW + 4.0 + h + 14.0
}

/// What the start's tile says of a company's real clock: the time waiting to be caught up
/// when it is opened, else the last absence caught up ("Caught up 3 d 4 h"), else that it
/// runs in step. None: it does not run with the real clock.
pub(super) fn tile_line(c: &Company) -> Option<(String, Color)> {
    if !realtime(c) {
        return None;
    }
    let behind = ck::behind(c, real_now());
    if behind >= ck::AWAY_TOLD {
        return Some((omsi_ui::tr("%{span} to catch up").replace("%{span}", &span_text(behind)), accent_2()));
    }
    match &c.clock.real.caught {
        Some(k) => Some((omsi_ui::tr("Caught up %{span}").replace("%{span}", &span_text(k.to - k.from)), OK)),
        None => Some((omsi_ui::tr("In step with the real clock").into_owned(), TEXT_DIM)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_is_written_with_its_two_largest_parts() {
        assert_eq!(span_text(2 * DAY + 4 * 60 + 7), "2 d 4 h");
        assert_eq!(span_text(31 * DAY + 7), "31 d");
        assert_eq!(span_text(4 * 60 + 10), "4 h 10 min");
        assert_eq!(span_text(3 * 60), "3 h");
        assert_eq!(span_text(12), "12 min");
        assert_eq!(span_text(-3), "0 min");
    }

    #[test]
    fn the_real_clock_is_translated() {
        // (the core's inbox texts, and the pages' texts chosen in a `match` or an `if`, which
        // `locale_check` cannot see)
        let mut texts: Vec<&str> = ck::texts();
        texts.extend([
            "Back to the real clock's pace",
            "×1: in step with the real clock. ×60: a minute a second; it stops for what needs you.",
            "When you open the company, the time since is simulated, a month at most. Fast-forwarding moves it ahead of the real clock.",
            "Off: the company's time stands still while it is closed.",
        ]);
        let places = |s: &str| {
            let mut v: Vec<String> = s.match_indices("%{").filter_map(|(i, _)| s[i..].find('}').map(|j| s[i..i + j + 1].to_string())).collect();
            v.sort();
            v
        };
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for t in &texts {
                let Some(x) = crate::_rust_i18n_try_translate(lang, t) else { panic!("{lang}: no translation of {t:?}") };
                assert_eq!(places(&x), places(t), "{lang}: {t:?} -> {x:?}");
            }
        }
    }
}
