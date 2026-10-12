//! The company's research (the rules are `omsi_launcher_lib::company::research`'s), as City Bus
//! Manager's tree: a strip with the cash, the trophies and the rating; the research under way,
//! how far it is and who works on it; and the sections, each with its hairline and its title,
//! a card a topic - its picture on its section's patterned colour, its stages, its price in
//! money and trophies, and what it waits for (a lock), is doing (its progress) or has done (a
//! check mark). An info button says what a topic does.

use super::super::theme::*;
use super::super::ui::{id_of, ButtonKind, Ui};
use super::super::Launcher;
use super::{act, eur, grouped, kit, num};
use glam::Vec2;
use omsi_launcher_lib::company::research::{self as rs, Need, Section, State, Topic};
use omsi_launcher_lib::company::Company;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::cell::Cell;

thread_local! {
    /// Until when (the interface's time) a click on Stop stops the research: the first click
    /// arms it, as stopping loses half the money.
    static STOP_ARMED: Cell<f32> = const { Cell::new(f32::MIN) };
}

/// A card of the tree.
const CARD_W: f32 = 200.0;
const CARD_H: f32 = 372.0;
const GAP: f32 = 14.0;

/// What a click on a card asks.
enum Action {
    Start(&'static str),
    Info(&'static str),
}

/// A section's colour: the ground of its cards' pictures.
fn colour(s: Section) -> Color {
    match s {
        Section::Buses => Color::rgba(36, 150, 140, 1.0),
        Section::Upgrades => Color::rgba(70, 128, 210, 1.0),
        Section::Passengers => Color::rgba(52, 98, 178, 1.0),
        Section::Premises => Color::rgba(52, 150, 82, 1.0),
        Section::Staff => Color::rgba(124, 84, 196, 1.0),
        Section::Workshop => Color::rgba(180, 106, 48, 1.0),
        Section::Area => Color::rgba(30, 134, 172, 1.0),
        Section::Tools => Color::rgba(216, 170, 54, 1.0),
    }
}

/// The small sign a section's pattern repeats.
fn motif(s: Section) -> &'static str {
    match s {
        Section::Buses => "directions_bus",
        Section::Upgrades => "star",
        Section::Passengers => "person",
        Section::Premises => "home",
        Section::Staff => "groups",
        Section::Workshop => "construction",
        Section::Area => "location_on",
        Section::Tools => "add",
    }
}

/// `text` (translated already) broken at spaces into lines of `w` at most.
fn wrap(ui: &Ui, text: &str, w: f32, px: f32, weight: Weight) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if ui.width(&next, px, weight) > w && !line.is_empty() {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// What a need asks, in words.
fn need_text(n: Need) -> String {
    match n {
        Need::Done(key) => {
            let Some(t) = rs::topic(key) else { return key.to_string() };
            let name = omsi_ui::tr(t.title).into_owned();
            if t.stages > 1 {
                omsi_ui::tr("%{name}, stage %{n}").replace("%{name}", &name).replace("%{n}", &rs::stage_of(key).to_string())
            } else {
                name
            }
        }
        Need::Level(lv) => omsi_ui::tr("Company level %{n}").replace("%{n}", &lv.to_string()),
    }
}

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    strip(&mut l.ui, Rect::new(area.x, area.y, area.w, 44.0), &c);
    let bh = banner(l, Rect::new(area.x, area.y + 58.0, area.w, 0.0), &c);
    let top = area.y + 58.0 + bh + 16.0;
    let body = Rect::new(area.x, top, area.w, (area.bottom() - top).max(0.0));
    let mut action = None;
    l.ui.scroll_area("company-research", body, &mut |ui, v| {
        let cols = (((v.w - 12.0 + GAP) / (CARD_W + GAP)).floor() as usize).max(1);
        let mut y = v.y;
        for s in Section::ALL {
            y = head(ui, Rect::new(v.x, y, v.w - 12.0, 30.0), s.label()) + 12.0;
            let topics: Vec<&Topic> = rs::TOPICS.iter().filter(|t| t.section == s).collect();
            for (k, t) in topics.iter().enumerate() {
                let r = Rect::new(v.x + (k % cols) as f32 * (CARD_W + GAP), y + (k / cols) as f32 * (CARD_H + GAP), CARD_W, CARD_H);
                if !ui.rect_visible(r) {
                    continue;
                }
                if let Some(a) = card(ui, r, &c, t) {
                    action = Some(a);
                }
            }
            y += topics.len().div_ceil(cols) as f32 * (CARD_H + GAP) + 22.0;
        }
        y - v.y
    });
    match action {
        Some(Action::Info(key)) => info(l, &c, key),
        Some(Action::Start(key)) => start(l, &c, key),
        None => {}
    }
}

/// The strip on top, as City Bus Manager's: the cash, the trophies, the rating (the
/// authority's view of the company).
fn strip(ui: &mut Ui, r: Rect, c: &Company) {
    let parts = [
        ("payments", eur(c.cash), if c.cash < 0 { DANGER } else { OK }, "Cash"),
        ("emoji_events", grouped(c.trophies as f64), WARN, "Trophies: earned with goals and contracts, spent on research"),
        ("star", format!("{} %", num(c.reputation, 1)), accent_2(), "Rating: how the authority and the passengers see the company"),
    ];
    let px = 18.0;
    let widths: Vec<f32> = parts.iter().map(|p| ui.width(&p.1, px, Weight::Bold) + 34.0).collect();
    let w = widths.iter().sum::<f32>() + 28.0 * (parts.len() as f32 - 1.0) + 36.0;
    let pill = Rect::new(r.center().x - w * 0.5, r.y, w.min(r.w), r.h);
    ui.p().rounded(pill, r.h * 0.5, FIELD);
    ui.p().rounded_border(pill, r.h * 0.5, 1.5, Color::WHITE.alpha(0.22));
    let mut x = pill.x + 18.0;
    for (k, (icon, value, ink, tip)) in parts.iter().enumerate() {
        let cell = Rect::new(x, r.y, widths[k], r.h);
        ui.p().circle(Vec2::new(x + 11.0, r.center().y), 12.0, ink.alpha(0.18));
        ui.icon(icon, Vec2::new(x + 11.0, r.center().y), 16.0, *ink);
        ui.text_in(value, Rect::new(x + 30.0, r.y, widths[k] - 30.0, r.h), px, Weight::Bold, TEXT, Align::Left);
        ui.tooltip(cell, tip);
        x += widths[k] + 28.0;
    }
}

/// The research under way (or none), and who works on it. Returns its height.
fn banner(l: &mut Launcher, r: Rect, c: &Company) -> f32 {
    let h = 74.0;
    let r = Rect::new(r.x, r.y, r.w, h);
    l.ui.card(r);
    let people = rs::researchers(c, &c.date).count();
    let pace = rs::daily_work(c, &c.date);
    let staff = if people == 0 {
        omsi_ui::tr("No researchers at work: a research stands still until you hire researchers on the Staff page.").into_owned()
    } else {
        omsi_ui::tr("Researchers at work: %{n}, %{work} researcher-days of work a day.").replace("%{n}", &people.to_string()).replace("%{work}", &num(pace, 1))
    };
    let staff_ink = if people == 0 { WARN } else { TEXT_SOFT };
    let Some(run) = c.research.running.clone() else {
        l.ui.icon("search", Vec2::new(r.x + 30.0, r.center().y), 24.0, accent_2());
        l.ui.text_in("No research is running: choose one below and start it.", Rect::new(r.x + 56.0, r.y + 12.0, r.w - 76.0, 24.0), kit::BODY, Weight::Bold, TEXT, Align::Left);
        l.ui.text_in(&staff, Rect::new(r.x + 56.0, r.y + 38.0, r.w - 76.0, 22.0), kit::NOTE, Weight::Regular, staff_ink, Align::Left);
        return h;
    };
    let Some(t) = rs::topic(&run.topic) else { return h };
    // its picture, small
    let pic = Rect::new(r.x + 14.0, r.y + 13.0, 48.0, 48.0);
    picture(&mut l.ui, pic, t, false);
    let stop_w = 210.0f32.min(r.w * 0.3);
    let text_w = (r.w - 90.0 - stop_w - 24.0).max(60.0);
    let what = if t.stages > 1 {
        omsi_ui::tr("Researching: %{name}, stage %{stage} of %{stages}").replace("%{name}", &omsi_ui::tr(t.title)).replace("%{stage}", &run.stage.to_string()).replace("%{stages}", &t.stages.to_string())
    } else {
        omsi_ui::tr("Researching: %{name}").replace("%{name}", &omsi_ui::tr(t.title))
    };
    l.ui.text_in(&what, Rect::new(r.x + 76.0, r.y + 10.0, text_w, 22.0), kit::BODY, Weight::Bold, TEXT, Align::Left);
    let left = match rs::days_left(c) {
        Some(d) => omsi_ui::tr("%{share} % done, about %{n} days to go at today's pace").replace("%{share}", &num(run.share() * 100.0, 0)).replace("%{n}", &num(d, 0)),
        None => omsi_ui::tr("%{share} % done: nobody works on it").replace("%{share}", &num(run.share() * 100.0, 0)),
    };
    super::meter(&mut l.ui, Rect::new(r.x + 76.0, r.y + 38.0, text_w, 6.0), run.share(), if people == 0 { WARN } else { accent() });
    l.ui.text_in(&format!("{left}  ·  {staff}"), Rect::new(r.x + 76.0, r.y + 48.0, text_w, 20.0), kit::NOTE, Weight::Regular, staff_ink, Align::Left);
    // stopping it: a second click within a few seconds
    let now = l.ui.time;
    let armed = STOP_ARMED.with(|a| a.get()) > now;
    let b = Rect::new(r.right() - stop_w - 14.0, r.y + 17.0, stop_w, 40.0);
    let label = if armed { "Click again to stop" } else { "Stop the research" };
    if l.ui.button("company-research-stop", b, label, Some("stop_circle"), if armed { ButtonKind::Danger } else { ButtonKind::Normal }) {
        if armed {
            STOP_ARMED.with(|a| a.set(f32::MIN));
            if act(l, rs::stop).is_some() {
                l.state.set_status(omsi_ui::tr("The research was stopped: the trophies and half the money came back.").into_owned(), false);
            }
        } else {
            STOP_ARMED.with(|a| a.set(now + 4.0));
        }
    }
    l.ui.tooltip(b, "Stopping gives back the trophies and half the money; the work done is lost.");
    h
}

/// A section's head: a short hairline, its title, a hairline to the end. Returns its bottom.
fn head(ui: &mut Ui, r: Rect, title: &str) -> f32 {
    let title = omsi_ui::tr(title).into_owned();
    let px = kit::HEAD + 1.0;
    let tw = ui.width(&title, px, Weight::Bold);
    let y = r.center().y;
    ui.p().rect(Rect::new(r.x, y - 0.5, 28.0, 1.0), Color::WHITE.alpha(0.55));
    ui.text_in(&title, Rect::new(r.x + 40.0, r.y, tw + 4.0, r.h), px, Weight::Bold, TEXT, Align::Left);
    let x = r.x + 40.0 + tw + 14.0;
    if x < r.right() {
        ui.p().rect(Rect::new(x, y - 0.5, r.right() - x, 1.0), Color::WHITE.alpha(0.55));
    }
    r.bottom()
}

/// A topic's picture: its section's colour with the section's sign repeated over it, its own
/// icon in the middle - dimmed under a lock when it cannot be started yet.
fn picture(ui: &mut Ui, r: Rect, t: &Topic, locked: bool) {
    let ground = colour(t.section);
    let ground = if locked { ground.darken(0.25) } else { ground };
    ui.p().rounded(r, RADIUS, ground);
    // the pattern: a staggered grid of the section's sign
    let step = (r.w / 4.5).max(12.0);
    let s = (step * 0.42).max(8.0);
    let mut row = 0;
    let mut y = r.y + step * 0.45;
    while y < r.bottom() - s * 0.5 {
        let mut x = r.x + step * if row % 2 == 0 { 0.45 } else { 0.95 };
        while x < r.right() - s * 0.5 {
            ui.icon(motif(t.section), Vec2::new(x, y), s, Color::WHITE.alpha(0.13));
            x += step;
        }
        y += step * 0.86;
        row += 1;
    }
    let c = r.center();
    let big = r.w * 0.42;
    ui.icon(t.icon, c, big, Color::WHITE.alpha(if locked { 0.55 } else { 0.95 }));
    if locked {
        let lr = (r.w * 0.2).max(10.0);
        ui.p().circle(Vec2::new(c.x, c.y + big * 0.18), lr, Color::rgba(230, 232, 238, 0.9));
        ui.icon("lock", Vec2::new(c.x, c.y + big * 0.18), lr * 1.25, ground.darken(0.3));
    }
}

/// A topic's card; returns what was clicked.
fn card(ui: &mut Ui, r: Rect, c: &Company, t: &Topic) -> Option<Action> {
    let state = rs::state(c, t);
    let done = rs::stages_done(&c.research, t);
    let running = c.research.running.as_ref().filter(|x| x.topic == t.key);
    let fill = match state {
        State::Done => Color::rgba(62, 70, 88, 1.0),
        State::Locked => PANEL,
        _ => FIELD,
    };
    ui.p().rounded(r, SHEET_RADIUS, fill);
    let edge = match state {
        State::Running => accent().alpha(0.8),
        State::Open => Color::WHITE.alpha(0.14),
        _ => HAIRLINE,
    };
    ui.p().rounded_border(r, SHEET_RADIUS, if state == State::Running { 1.5 } else { 1.0 }, edge);
    let mut out = None;
    // the title, on up to three lines
    let px = 15.0;
    let title = omsi_ui::tr(t.title).into_owned();
    let lines = wrap(ui, &title, r.w - 20.0, px, Weight::Bold);
    let lh = 19.0;
    let n = lines.len().min(3);
    let ty = r.y + 12.0 + (3 - n) as f32 * lh * 0.5;
    for (k, line) in lines.iter().take(3).enumerate() {
        ui.text_in(line, Rect::new(r.x + 10.0, ty + k as f32 * lh, r.w - 20.0, lh), px, Weight::Bold, if state == State::Locked { TEXT_SOFT } else { TEXT }, Align::Center);
    }
    // the picture
    let side = r.w - 44.0;
    let pic = Rect::new(r.x + 22.0, r.y + 74.0, side, side);
    picture(ui, pic, t, state == State::Locked);
    let mut y = pic.bottom() + 12.0;
    // the stages done (and the running one's part), with the info button beside them
    if state != State::Done {
        let bar = Rect::new(r.x + 22.0, y, side - 30.0, 18.0);
        let part = running.map(|x| x.share()).unwrap_or(0.0);
        let frac = ((done as f64 + part) / t.stages as f64).clamp(0.0, 1.0);
        ui.p().rounded(bar, 4.0, Color::WHITE.alpha(0.82));
        if frac > 0.0 {
            ui.p().rounded(Rect::new(bar.x, bar.y, (bar.w * frac as f32).max(8.0), bar.h), 4.0, accent());
        }
        ui.text_in(&format!("{} / {}", done, t.stages), bar, 12.0, Weight::Bold, Color::rgba(30, 36, 50, 1.0), Align::Center);
        if info_button(ui, &format!("company-research-info-{}", t.key), Vec2::new(bar.right() + 16.0, bar.center().y)) {
            out = Some(Action::Info(t.key));
        }
        y = bar.bottom() + 12.0;
    } else if info_button(ui, &format!("company-research-info-{}", t.key), Vec2::new(r.right() - 22.0, pic.y + 14.0)) {
        out = Some(Action::Info(t.key));
    }
    match state {
        State::Done => {
            let m = Vec2::new(r.center().x, y + 22.0);
            ui.p().circle(m, 20.0, OK);
            ui.icon("check", m, 26.0, Color::WHITE);
            ui.text_in("Researched", Rect::new(r.x + 10.0, y + 48.0, r.w - 20.0, 20.0), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Center);
        }
        State::Running => {
            let run = running.cloned().unwrap_or_default();
            ui.text_in(&omsi_ui::tr("Researching: %{share} %").replace("%{share}", &num(run.share() * 100.0, 0)), Rect::new(r.x + 10.0, y, r.w - 20.0, 22.0), kit::ROWS, Weight::Bold, accent_2(), Align::Center);
            let left = match rs::days_left(c) {
                Some(d) => omsi_ui::tr("about %{n} days to go").replace("%{n}", &num(d, 0)),
                None => omsi_ui::tr("nobody works on it").into_owned(),
            };
            ui.text_in(&left, Rect::new(r.x + 10.0, y + 24.0, r.w - 20.0, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Center);
        }
        State::Locked | State::Open => {
            let k = rs::cost(c, t, done + 1);
            let money = Rect::new(r.x + 10.0, y, r.w - 20.0, 20.0);
            row(ui, money, "payments", &eur(k.money), if c.cash < k.money { DANGER.lighten(0.2) } else { TEXT }, OK);
            let short = k.trophies - c.trophies;
            let trophies = if short > 0 { omsi_ui::tr("%{n} (%{short} missing)").replace("%{n}", &grouped(k.trophies as f64)).replace("%{short}", &grouped(short as f64)) } else { grouped(k.trophies as f64) };
            row(ui, Rect::new(r.x + 10.0, y + 22.0, r.w - 20.0, 20.0), "emoji_events", &trophies, if short > 0 { DANGER.lighten(0.2) } else { TEXT }, WARN);
            let b = Rect::new(r.x + 18.0, r.bottom() - 46.0, r.w - 36.0, 34.0);
            if state == State::Open {
                let kind = if c.research.running.is_some() { ButtonKind::Normal } else { ButtonKind::Primary };
                if ui.button(&format!("company-research-start-{}", t.key), b, "Start research", Some("search"), kind) {
                    out = Some(Action::Start(t.key));
                }
            } else {
                let needs: Vec<String> = rs::unmet(c, t).into_iter().map(need_text).collect();
                let text = omsi_ui::tr("Needs: %{what}").replace("%{what}", &needs.join(", "));
                let lines = wrap(ui, &text, r.w - 44.0, kit::NOTE, Weight::Medium);
                let top = b.y + b.h - lines.len().min(2) as f32 * 18.0;
                ui.icon("lock", Vec2::new(r.x + 20.0, top + 9.0), 14.0, TEXT_DIM);
                for (k, line) in lines.iter().take(2).enumerate() {
                    ui.text_in(line, Rect::new(r.x + 32.0, top + k as f32 * 18.0, r.w - 44.0, 18.0), kit::NOTE, Weight::Medium, TEXT_DIM, Align::Left);
                }
            }
        }
    }
    out
}

/// The round info button of a card: a white disc with an "i" (the info sign, big). Returns
/// whether it was clicked.
fn info_button(ui: &mut Ui, name: &str, c: Vec2) -> bool {
    let r = Rect::new(c.x - 12.0, c.y - 12.0, 24.0, 24.0);
    let (h, _, clicked) = ui.interact(id_of(name), r);
    if h {
        ui.cursor = winit::window::CursorIcon::Pointer;
    }
    ui.p().circle(c, 8.0, Color::rgba(30, 36, 50, 1.0));
    ui.icon("info", c, 24.0, if h { Color::WHITE } else { Color::WHITE.alpha(0.85) });
    ui.tooltip(r, "What it does");
    clicked
}

/// A price's row on a card: its icon in its colour, the amount.
fn row(ui: &mut Ui, r: Rect, icon: &str, text: &str, ink: Color, icon_ink: Color) {
    let px = kit::ROWS + 0.5;
    let w = ui.width(text, px, Weight::Bold) + 24.0;
    let x = r.center().x - w * 0.5;
    ui.icon(icon, Vec2::new(x + 8.0, r.center().y), 16.0, icon_ink);
    ui.text_in(text, Rect::new(x + 22.0, r.y, w - 20.0, r.h), px, Weight::Bold, ink, Align::Left);
}

/// What a topic does, its stages, its price and what it needs, in a popup.
fn info(l: &mut Launcher, c: &Company, key: &str) {
    let Some(t) = rs::topic(key) else { return };
    let done = rs::stages_done(&c.research, t);
    let mut text = omsi_ui::tr(t.info).into_owned();
    text.push_str("\n\n");
    if done >= t.stages {
        text.push_str(&omsi_ui::tr("Researched: it works for the company."));
    } else {
        let k = rs::cost(c, t, done + 1);
        let stage = omsi_ui::tr("Stage %{stage} of %{stages} costs %{money} and %{trophies} trophies, and %{work} researcher-days of work.")
            .replace("%{stage}", &(done + 1).to_string())
            .replace("%{stages}", &t.stages.to_string())
            .replace("%{money}", &eur(k.money))
            .replace("%{trophies}", &grouped(k.trophies as f64))
            .replace("%{work}", &num(k.work, 0));
        text.push_str(&stage);
    }
    let needs: Vec<String> = rs::unmet(c, t).into_iter().map(need_text).collect();
    let unlock = if needs.is_empty() { String::new() } else { omsi_ui::tr("It needs first: %{what}.").replace("%{what}", &needs.join(", ")) };
    kit::show(l, kit::Popup::new(t.icon, t.title, text, unlock, None));
}

/// Start a topic's next stage: what is in the way said first (a research running, the
/// trophies, the cash), else started.
fn start(l: &mut Launcher, c: &Company, key: &'static str) {
    let Some(t) = rs::topic(key) else { return };
    let done = rs::stages_done(&c.research, t);
    let k = rs::cost(c, t, done + 1);
    if c.research.running.is_some() {
        let p = kit::Popup::new("search", "A research is running", omsi_ui::tr("Your researchers work on one research at a time: wait until it is done, or stop it at the top of the page.").into_owned(), "", None);
        kit::show(l, p);
        return;
    }
    if k.trophies > c.trophies {
        let text = omsi_ui::tr("The company has %{have} trophies; this research needs %{need}: %{short} more.").replace("%{have}", &grouped(c.trophies as f64)).replace("%{need}", &grouped(k.trophies as f64)).replace("%{short}", &grouped((k.trophies - c.trophies) as f64));
        kit::show(l, kit::Popup::new("emoji_events", "Not enough trophies", text, omsi_ui::tr("Trophies are earned with goals and contracts: see the Contracts page and the inbox."), None));
        return;
    }
    if k.money > c.cash {
        kit::show(l, kit::no_cash(c, k.money));
        return;
    }
    if let Some(stage) = act(l, |c| rs::start(c, key)) {
        let text = if t.stages > 1 {
            omsi_ui::tr("The research has begun: %{name}, stage %{stage}.").replace("%{name}", &omsi_ui::tr(t.title)).replace("%{stage}", &stage.to_string())
        } else {
            omsi_ui::tr("The research has begun: %{name}.").replace("%{name}", &omsi_ui::tr(t.title))
        };
        l.state.set_status(text, false);
    }
}
