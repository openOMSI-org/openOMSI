//! The company's inbox (City Bus Manager's; the rules are `company::inbox`'): what happens to
//! the company and around it as cards, the newest first - the world's events with the bar of
//! their days and how far into them the company is, the goals with their progress, the staff,
//! the fleet, the workshop, the lines, the money. Filtered by kind, the unread ones marked and
//! counted; a card opened beside the list says all it has, what an event does and where a
//! thing is mended. Under the list, the trophies' bar to the next milestone.

use super::super::theme::*;
use super::super::ui::{ButtonKind, Ui};
use super::super::Launcher;
use super::contracts::{goal_progress, reward, trophy_bar};
use super::{act, day_label, eur, eur_cents, grouped, kit, month_label, num, section};
use glam::Vec2;
use omsi_launcher_lib::company::clock::Level;
use omsi_launcher_lib::company::inbox::{Event, Kind, Message};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::cell::RefCell;

/// Which messages the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Filter {
    #[default]
    All,
    Unread,
    Of(Kind),
}

impl Filter {
    fn shows(self, m: &Message) -> bool {
        match self {
            Filter::All => true,
            Filter::Unread => !m.read,
            Filter::Of(k) => m.kind == k,
        }
    }

    /// The kinds "mark all read" and "delete read" act on (empty: all).
    fn kinds(self) -> Vec<Kind> {
        match self {
            Filter::Of(k) => vec![k],
            _ => Vec::new(),
        }
    }
}

/// What the page keeps: the filter and the message open, for which company.
#[derive(Default)]
struct View {
    company: String,
    filter: Filter,
    open: Option<u64>,
}

thread_local! {
    static VIEW: RefCell<View> = RefCell::new(View::default());
}

/// The inbox and the contracts brought up to date before they are looked at: the welcome said
/// (an older company's events looked for), the week's goals drawn.
pub(super) fn ensure(l: &mut Launcher) {
    let Some(c) = l.company.company.as_ref() else { return };
    if c.inbox.welcomed && c.contracts.week == co::dates::week_of(&c.date) {
        return;
    }
    act(l, |c| {
        co::inbox::welcome(c);
        let date = c.date.clone();
        co::contracts::refresh(c, &date);
        Ok(())
    });
}

/// A place's value as the page writes it, by its name (`company::inbox::Message`).
fn value(name: &str, v: &str) -> String {
    match name {
        "amount" | "reward" | "cost" | "charge" => v.parse::<i64>().map(eur).unwrap_or_else(|_| v.to_string()),
        "price" | "price_after" => v.parse::<f64>().map(eur_cents).unwrap_or_else(|_| v.to_string()),
        "date" | "from" | "until" => day_label(v),
        "month" => month_label(v),
        "count" => v.parse::<f64>().map(grouped).unwrap_or_else(|_| v.to_string()),
        "stars" => v.parse::<f64>().map(|x| num(x, 1)).unwrap_or_else(|_| v.to_string()),
        "what" | "why" | "area" | "job" | "role" => omsi_ui::tr(v).into_owned(),
        _ => v.to_string(),
    }
}

/// An English text translated, its `%{name}` places filled as the page writes them.
pub(super) fn words(text: &str, args: &[(String, String)]) -> String {
    let mut t = omsi_ui::tr(text).into_owned();
    for (k, v) in args {
        t = t.replace(&format!("%{{{k}}}"), &value(k, v));
    }
    t
}

/// A wide bar with its words in its middle (City Bus Manager's): its track, the part done in
/// `c`.
pub(super) fn wide_bar(ui: &mut Ui, r: Rect, frac: f64, c: Color, label: &str) {
    ui.p().rounded(r, 6.0, FIELD);
    let f = frac.clamp(0.0, 1.0) as f32;
    if f > 0.0 {
        ui.p().rounded(Rect::new(r.x, r.y, (r.w * f).clamp(12.0f32.min(r.w), r.w), r.h), 6.0, c.alpha(0.7));
    }
    ui.text_in(label, r.pad(8.0, 0.0), kit::NOTE, Weight::Bold, TEXT, Align::Center);
}

/// A button's width for its words in the interface's language (and its icon).
pub(super) fn button_w(ui: &Ui, label: &str, icon: Option<&str>) -> f32 {
    kit::Foot::width(ui, &omsi_ui::tr(label), icon)
}

/// "Fri 8 Mar".
fn short_day(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (_, m, day) = co::dates::civil_from_days(d);
    format!("{} {} {}", omsi_ui::tr(super::WEEKDAYS[co::dates::weekday(d) as usize]), day, omsi_ui::tr(super::MONTHS[(m as usize).clamp(1, 12) - 1]))
}

/// The days `from` to `until`: how far into them the company is (0 to 1) and their words
/// ("Fri 8 Mar - Mon 11 Mar").
fn days_of(c: &Company, from: &str, until: &str) -> (f64, String) {
    let days = (co::dates::between(from, until) + 1).max(1) as f64;
    let into = co::dates::between(from, &c.date) as f64 + c.clock.minute as f64 / 1_440.0;
    let label = if from == until { short_day(from) } else { format!("{} – {}", short_day(from), short_day(until)) };
    ((into / days).clamp(0.0, 1.0), label)
}

/// A message's icon: its event's, else its kind's.
fn icon_of(c: &Company, m: &Message) -> &'static str {
    if let Some(e) = m.event.and_then(|id| c.inbox.event(id)) {
        return event_icon(&e.key);
    }
    kind_icon(m.kind)
}

pub(super) fn kind_icon(k: Kind) -> &'static str {
    match k {
        Kind::Notice => "info",
        Kind::Event => "event",
        Kind::Staff => "groups",
        Kind::Fleet => "directions_bus",
        Kind::Workshop => "construction",
        Kind::Research => "hub",
        Kind::Contract => "description",
        Kind::Goal => "emoji_events",
        Kind::Finance => "payments",
        Kind::Lines => "route",
    }
}

fn event_icon(key: &str) -> &'static str {
    match key {
        "fuel_shortage" | "fuel_drop" => "water_drop",
        "environment_days" => "air",
        "fair" => "public",
        "football" => "sports_score",
        "christmas_market" => "stars",
        "rail_strike" => "commute",
        "town_festival" => "campaign",
        "concert" => "headphones",
        "marathon" => "directions_walk",
        "tourists" => "photo_camera",
        "school_holidays" => "today",
        "heat_wave" => "wb_sunny",
        "storm" => "thunderstorm",
        "snow" => "weather_snowy",
        "roadworks" => "construction",
        "diversion" => "alt_route",
        _ => "event",
    }
}

/// A level's colour (the ordinary run of things in the accent's).
fn colour_of(level: Level) -> Color {
    match level {
        Level::Minor | Level::Info => accent_2(),
        other => super::clock::colour(other),
    }
}

/// What a message shows as its bar: (how far, its colour, its words).
fn bar_of(c: &Company, m: &Message) -> Option<(f64, Color, String)> {
    if let Some(g) = m.goal.and_then(|id| c.contracts.goals.iter().find(|g| g.id == id)) {
        let colour = match g.state {
            co::contracts::State::Done => OK,
            co::contracts::State::Failed => TEXT_FAINT,
            _ => accent_2(),
        };
        return Some((g.share(), colour, goal_progress(g)));
    }
    if let (Some(from), Some(until)) = (m.from.as_deref(), m.until.as_deref()) {
        let (frac, label) = days_of(c, from, until);
        return Some((frac, accent_2(), label));
    }
    m.progress.map(|(done, of)| ((done / of.max(1e-9)).clamp(0.0, 1.0), accent_2(), format!("{} / {}", grouped(done), grouped(of))))
}

/// A card's height in a list `w` wide.
fn card_height(ui: &Ui, w: f32, text: &str, bar: bool, goal: bool) -> f32 {
    let ph = ui.paragraph_height(text, w - 62.0 - 16.0, kit::ROWS, Weight::Regular);
    14.0 + 24.0 + 6.0 + ph + if goal { 6.0 + 22.0 } else { 0.0 } + if bar { 10.0 + 24.0 } else { 0.0 } + 14.0
}

/// A message's card: its icon, title, day, text, reward and bar; returns (clicked, deleted).
fn card(ui: &mut Ui, r: Rect, c: &Company, m: &Message, text: &str, open: bool) -> (bool, bool) {
    // (a sheet of its own under the mouse's light: in the light look white, where a see-through
    // white lift became a muddy patch darker than the ground)
    ui.p().rounded(r, RADIUS, PANEL);
    let clicked = ui.row(&format!("company-inbox-{}", m.id), r, false);
    let colour = colour_of(m.level);
    if open {
        ui.p().rounded(r, RADIUS, accent_2().alpha(0.12));
    }
    ui.p().rounded_border(r, RADIUS, 1.0, if open { accent_2().alpha(0.7) } else { HAIRLINE });
    if !m.read {
        ui.p().rounded(Rect::new(r.x, r.y + 8.0, 4.0, r.h - 16.0), 2.0, accent());
    }
    let x = r.x + 62.0;
    let w = r.w - 62.0 - 16.0;
    let mut y = r.y + 14.0;
    ui.p().circle(Vec2::new(r.x + 31.0, y + 16.0), 19.0, colour.alpha(0.16));
    ui.icon(icon_of(c, m), Vec2::new(r.x + 31.0, y + 16.0), 22.0, colour);
    let date = short_day(&m.date);
    let dw = ui.width(&date, kit::NOTE, Weight::Medium) + 40.0;
    let title = words(&m.title, &m.args);
    ui.text_in(&title, Rect::new(x, y, w - dw, 24.0), kit::HEAD - 0.5, if m.read { Weight::Medium } else { Weight::Bold }, TEXT, Align::Left);
    ui.text_in(&date, Rect::new(x + w - dw, y, dw - 30.0, 24.0), kit::NOTE, Weight::Medium, TEXT_DIM, Align::Right);
    let delete = ui.icon_button(&format!("company-inbox-delete-{}", m.id), Vec2::new(r.right() - 22.0, y + 12.0), 12.0, "delete", "Delete");
    y += 24.0 + 6.0;
    y += ui.paragraph(text, Vec2::new(x, y), w, kit::ROWS, Weight::Regular, if m.read { TEXT_SOFT } else { TEXT });
    let goal = m.goal.and_then(|id| c.contracts.goals.iter().find(|g| g.id == id));
    if let Some(g) = goal {
        reward(ui, Vec2::new(x, y + 6.0), g.trophies, g.reward);
        y += 6.0 + 22.0;
    }
    if let Some((frac, col, label)) = bar_of(c, m) {
        wide_bar(ui, Rect::new(x, y + 10.0, w, 24.0), frac, col, &label);
    }
    (clicked, delete)
}

pub fn draw(l: &mut Launcher, area: Rect) {
    ensure(l);
    let Some(c) = l.company.company.clone() else { return };
    let (mut filter, mut open) = VIEW.with(|v| {
        let mut v = v.borrow_mut();
        if v.company != c.id {
            *v = View { company: c.id.clone(), ..Default::default() };
        }
        (v.filter, v.open)
    });
    let gap = 14.0;
    // the filters: all, unread, and the kinds there are messages of, each with its count
    let msgs = &c.inbox.messages;
    let mut filters: Vec<(Filter, String)> = vec![(Filter::All, format!("{}  {}", omsi_ui::tr("All"), msgs.len())), (Filter::Unread, format!("{}  {}", omsi_ui::tr("Unread"), c.inbox.unread()))];
    for k in Kind::ALL {
        let n = msgs.iter().filter(|m| m.kind == k).count();
        if n > 0 {
            filters.push((Filter::Of(k), format!("{}  {}", omsi_ui::tr(k.label()), n)));
        }
    }
    if !filters.iter().any(|f| f.0 == filter) {
        filter = Filter::All;
    }
    let refs: Vec<&str> = filters.iter().map(|f| f.1.as_str()).collect();
    let mut chosen = filters.iter().position(|f| f.0 == filter).unwrap_or(0);
    let w_mark = button_w(&l.ui, "Mark all read", Some("check_circle"));
    let w_delete = button_w(&l.ui, "Delete read", Some("delete"));
    let tools_w = w_mark + 8.0 + w_delete;
    let chips = Rect::new(area.x, area.y, (area.w - tools_w - gap).max(200.0), 34.0);
    if l.ui.chips("company-inbox-filter", chips, &mut chosen, &refs) {
        filter = filters[chosen].0;
    }
    let chips_h = l.ui.chips_height(chips.w, 34.0, &refs);
    let mut mark_all = false;
    let mut delete_read = false;
    let tools = Rect::new(area.right() - tools_w, area.y, tools_w, 34.0);
    if l.ui.button("company-inbox-mark-all", Rect::new(tools.x, tools.y, w_mark, 34.0), "Mark all read", Some("check_circle"), ButtonKind::Normal) {
        mark_all = true;
    }
    let dr = Rect::new(tools.x + w_mark + 8.0, tools.y, w_delete, 34.0);
    if l.ui.button("company-inbox-delete-read", dr, "Delete read", Some("delete"), ButtonKind::Ghost) {
        delete_read = true;
    }
    l.ui.tooltip(dr, "Delete the messages already read (of the kind shown)");
    // the list, and the trophies' bar under it
    let top = area.y + chips_h + gap;
    let lw = ((area.w - gap) * 0.56).max(360.0).min(area.w);
    let strip_h = 58.0;
    let list = Rect::new(area.x, top, lw, (area.bottom() - top - strip_h - gap).max(120.0));
    let shown: Vec<Message> = msgs.iter().rev().filter(|m| filter.shows(m)).cloned().collect();
    let mut clicked: Option<u64> = None;
    let mut deleted: Option<u64> = None;
    if shown.is_empty() {
        let mid = list.center();
        l.ui.icon("check_circle", Vec2::new(mid.x, mid.y - 40.0), 40.0, TEXT_FAINT);
        let t = if filter == Filter::Unread { "Everything is read." } else { "No messages yet. What happens to your company and around it comes in here as its days go by." };
        l.ui.text_in(t, Rect::new(list.x, mid.y - 6.0, list.w, 24.0), kit::BODY, Weight::Medium, TEXT_SOFT, Align::Center);
    }
    let texts: Vec<String> = shown.iter().map(|m| words(&m.text, &m.args)).collect();
    l.ui.scroll_area("company-inbox-list", list, &mut |ui: &mut Ui, v: Rect| {
        let mut y = v.y;
        let w = v.w - 10.0;
        for (m, text) in shown.iter().zip(texts.iter()) {
            let goal = m.goal.is_some_and(|id| c.contracts.goals.iter().any(|g| g.id == id));
            let h = card_height(ui, w, text, bar_of(&c, m).is_some(), goal);
            let r = Rect::new(v.x, y, w, h);
            if ui.rect_visible(r) {
                let (k, d) = card(ui, r, &c, m, text, open == Some(m.id));
                if d {
                    deleted = Some(m.id);
                } else if k {
                    clicked = Some(m.id);
                }
            }
            y += h + 10.0;
        }
        y - v.y
    });
    let strip = Rect::new(area.x, list.bottom() + gap, lw, strip_h);
    l.ui.card(strip);
    kit::caps(&mut l.ui, Rect::new(strip.x + 16.0, strip.y + 8.0, 200.0, 16.0), "Trophies");
    trophy_bar(&mut l.ui, Rect::new(strip.x + 12.0, strip.y + 22.0, strip.w - 24.0, 32.0), &c);
    // the message open
    let detail = Rect::new(area.x + lw + gap, top, (area.w - lw - gap).max(0.0), area.bottom() - top);
    let mut go: Option<usize> = None;
    let mut unread: Option<u64> = None;
    if detail.w > 200.0 {
        let m = open.and_then(|id| c.inbox.get(id)).cloned();
        let (g, u, d) = details(l, detail, &c, m.as_ref());
        go = g;
        unread = u;
        if d.is_some() {
            deleted = d;
        }
    }
    // what was done
    if let Some(id) = clicked {
        open = Some(id);
        if c.inbox.get(id).is_some_and(|m| !m.read) {
            act(l, |c| {
                c.inbox.mark(id, true);
                Ok(())
            });
        }
    }
    if let Some(id) = deleted {
        if open == Some(id) {
            open = None;
        }
        act(l, |c| {
            c.inbox.delete(id);
            Ok(())
        });
    }
    if let Some(id) = unread {
        act(l, |c| {
            c.inbox.mark(id, false);
            Ok(())
        });
    }
    let kinds = filter.kinds();
    if mark_all {
        act(l, |c| {
            c.inbox.mark_all(&kinds);
            Ok(())
        });
    }
    if delete_read {
        act(l, |c| {
            c.inbox.delete_read(&kinds);
            Ok(())
        });
        if open.is_some_and(|id| l.company.company.as_ref().is_some_and(|c| c.inbox.get(id).is_none())) {
            open = None;
        }
    }
    VIEW.with(|v| {
        let mut v = v.borrow_mut();
        (v.filter, v.open) = (filter, open);
    });
    if let Some(tab) = go {
        l.company.tab = tab;
    }
}

/// Where a message is mended in the company's mode: the tab and the button's words. The easy
/// mode goes only to the pages it shows - the workshop's news to the fleet, while what the
/// company sees to by itself there (the research, the contracts and goals, the planning) has
/// no button.
fn page_for(m: &Message, mode: co::Mode) -> Option<(usize, &'static str, &'static str)> {
    let page = page_of(m);
    if mode == co::Mode::Advanced {
        return page;
    }
    match m.kind {
        Kind::Workshop => Some((1, "To the fleet", "directions_bus")),
        _ => page.filter(|p| super::pages_of(mode).contains(&p.0)),
    }
}

/// Where a message of `kind` is mended: the tab and the button's words.
fn page_of(m: &Message) -> Option<(usize, &'static str, &'static str)> {
    match m.kind {
        Kind::Staff => Some((2, "To the staff", "groups")),
        Kind::Fleet => Some((1, "To the fleet", "directions_bus")),
        Kind::Workshop => Some((super::WORKSHOP_TAB, "To the workshop", "construction")),
        Kind::Research => Some((super::RESEARCH_TAB, "To the research", "hub")),
        Kind::Contract | Kind::Goal => Some((super::CONTRACTS_TAB, "To the contracts", "description")),
        Kind::Finance => Some((4, "Show the finances", "payments")),
        Kind::Lines => Some((3, "To the lines", "route")),
        Kind::Event => Some((5, "Open the planning", "event")),
        Kind::Notice => None,
    }
}

/// What an event does, in lines of words and their colours.
fn effects(e: &Event) -> Vec<(String, Color)> {
    let pct = |f: f64| format!("{:+.0} %", (f - 1.0) * 100.0);
    let mut out = Vec::new();
    if (e.fuel - 1.0).abs() > 0.001 {
        out.push((omsi_ui::tr("Diesel price: %{pct}").replace("%{pct}", &pct(e.fuel)), if e.fuel > 1.0 { WARN } else { OK }));
    }
    if (e.demand - 1.0).abs() > 0.001 {
        let t = if e.line.is_some() { "Passengers on the line: %{pct}" } else { "Passengers: %{pct}" };
        out.push((omsi_ui::tr(t).replace("%{pct}", &pct(e.demand)), if e.demand > 1.0 { OK } else { WARN }));
    }
    if e.late > 0.0 {
        let t = if e.line.is_some() { "Trips on the line late more often: %{pct}" } else { "Trips late more often: %{pct}" };
        out.push((omsi_ui::tr(t).replace("%{pct}", &format!("+{:.0} %", e.late * 100.0)), WARN));
    }
    out
}

/// The message open, beside the list: all it says, an event's effects and days, a goal's
/// progress, and its buttons. Returns (a tab to go to, a message to mark unread, one to delete).
fn details(l: &mut Launcher, r: Rect, c: &Company, m: Option<&Message>) -> (Option<usize>, Option<u64>, Option<u64>) {
    let inner = section(&mut l.ui, r, "Message");
    let Some(m) = m else {
        let mid = inner.center();
        l.ui.icon("receipt_long", Vec2::new(mid.x, mid.y - 30.0), 36.0, TEXT_FAINT);
        l.ui.text_in("Click a message to read it here.", Rect::new(inner.x, mid.y, inner.w, 24.0), kit::BODY, Weight::Medium, TEXT_DIM, Align::Center);
        return (None, None, None);
    };
    let colour = colour_of(m.level);
    let mut y = inner.y;
    let tw = kit::tag(&mut l.ui, Vec2::new(inner.x, y), &omsi_ui::tr(m.kind.label()), colour);
    let _ = tw;
    l.ui.text_in(&day_label(&m.date), Rect::new(inner.x, y, inner.w, 22.0), kit::NOTE, Weight::Medium, TEXT_DIM, Align::Right);
    y += 34.0;
    let title = words(&m.title, &m.args);
    y += l.ui.paragraph(&title, Vec2::new(inner.x, y), inner.w, kit::TITLE, Weight::Bold, TEXT) + 10.0;
    let text = words(&m.text, &m.args);
    y += l.ui.paragraph(&text, Vec2::new(inner.x, y), inner.w, kit::BODY, Weight::Regular, TEXT_SOFT) + 16.0;
    // an event: what it does, and its days
    if let Some(e) = m.event.and_then(|id| c.inbox.event(id)).cloned() {
        kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "What it does");
        y += 24.0;
        for (t, col) in effects(&e) {
            l.ui.p().circle(Vec2::new(inner.x + 5.0, y + 11.0), 4.0, col);
            l.ui.text_in(&t, Rect::new(inner.x + 18.0, y, inner.w - 18.0, 22.0), kit::ROWS, Weight::Medium, TEXT, Align::Left);
            y += 26.0;
        }
        let status = if c.date.as_str() < e.from.as_str() {
            omsi_ui::tr("Starts in %{n} days").replace("%{n}", &co::dates::between(&c.date, &e.from).to_string())
        } else if c.date.as_str() <= e.until.as_str() {
            omsi_ui::tr("Running: day %{a} of %{b}").replace("%{a}", &(co::dates::between(&e.from, &c.date) + 1).to_string()).replace("%{b}", &e.days().to_string())
        } else {
            omsi_ui::tr("Over").into_owned()
        };
        l.ui.text_in(&status, Rect::new(inner.x, y, inner.w, 22.0), kit::ROWS, Weight::Bold, accent_2(), Align::Left);
        y += 30.0;
        y += day_strip(&mut l.ui, Rect::new(inner.x, y, inner.w, 46.0), c, &e.from, &e.until) + 14.0;
    } else if let (Some(from), Some(until)) = (m.from.as_deref(), m.until.as_deref()) {
        y += day_strip(&mut l.ui, Rect::new(inner.x, y, inner.w, 46.0), c, from, until) + 14.0;
    }
    // a goal: its progress, its reward, its rule, how long it has
    if let Some(g) = m.goal.and_then(|id| c.contracts.goals.iter().find(|g| g.id == id)).cloned() {
        if let Some((frac, col, label)) = bar_of(c, m) {
            wide_bar(&mut l.ui, Rect::new(inner.x, y, inner.w, 28.0), frac, col, &label);
            y += 38.0;
        }
        reward(&mut l.ui, Vec2::new(inner.x, y), g.trophies, g.reward);
        y += 30.0;
        y += l.ui.paragraph(g.aim.rule(), Vec2::new(inner.x, y), inner.w, kit::NOTE + 0.5, Weight::Regular, TEXT_DIM) + 8.0;
        let state = match g.state {
            co::contracts::State::Offered => omsi_ui::tr("On offer until %{date}").replace("%{date}", &day_label(&g.offered_until)),
            co::contracts::State::Running => omsi_ui::tr("Running until %{date}").replace("%{date}", &day_label(g.deadline.as_deref().unwrap_or(""))),
            co::contracts::State::Done => omsi_ui::tr("Reached").into_owned(),
            co::contracts::State::Failed => omsi_ui::tr("Missed").into_owned(),
        };
        l.ui.text_in(&state, Rect::new(inner.x, y, inner.w, 22.0), kit::ROWS, Weight::Bold, TEXT_SOFT, Align::Left);
    } else if let Some((frac, col, label)) = m.progress.and_then(|_| bar_of(c, m)) {
        wide_bar(&mut l.ui, Rect::new(inner.x, y, inner.w, 28.0), frac, col, &label);
    }
    let _ = y;
    // the buttons
    let by = inner.bottom() - 36.0;
    let mut x = inner.x;
    let mut go = None;
    if let Some((tab, label, icon)) = page_for(m, c.mode) {
        let w = button_w(&l.ui, label, Some(icon));
        if l.ui.button("company-inbox-go", Rect::new(x, by, w, 36.0), label, Some(icon), ButtonKind::Primary) {
            go = Some(tab);
        }
        x += w + 10.0;
    }
    let mut unread = None;
    let w = button_w(&l.ui, "Mark as unread", None);
    if m.read && l.ui.button("company-inbox-unread", Rect::new(x, by, w, 36.0), "Mark as unread", None, ButtonKind::Normal) {
        unread = Some(m.id);
    }
    let mut delete = None;
    let w = button_w(&l.ui, "Delete", Some("delete"));
    if l.ui.button("company-inbox-delete", Rect::new(inner.right() - w, by, w, 36.0), "Delete", Some("delete"), ButtonKind::Danger) {
        delete = Some(m.id);
    }
    (go, unread, delete)
}

/// The days `from` to `until` as cells side by side (at most two weeks shown), the days gone
/// filled, today marked; returns its height.
fn day_strip(ui: &mut Ui, r: Rect, c: &Company, from: &str, until: &str) -> f32 {
    let Some(a) = co::dates::parse(from) else { return 0.0 };
    let n = (co::dates::between(from, until) + 1).clamp(1, 14);
    let today = co::dates::parse(&c.date).unwrap_or(a);
    let gap = 4.0;
    let cw = ((r.w - gap * (n - 1) as f32) / n as f32).min(90.0);
    for k in 0..n {
        let d = a + k;
        let cell = Rect::new(r.x + k as f32 * (cw + gap), r.y, cw, r.h);
        let (fill, ink) = if d < today {
            (accent_2().alpha(0.55), TEXT)
        } else if d == today {
            (accent().alpha(0.9), on_accent())
        } else {
            (FIELD, TEXT_SOFT)
        };
        ui.p().rounded(cell, 6.0, fill);
        let (_, _, day) = co::dates::civil_from_days(d);
        ui.text_in(super::WEEKDAYS[co::dates::weekday(d) as usize], Rect::new(cell.x, cell.y + 4.0, cell.w, 18.0), kit::NOTE - 1.0, Weight::Bold, ink, Align::Center);
        ui.text_in(&day.to_string(), Rect::new(cell.x, cell.y + 22.0, cell.w, 20.0), kit::ROWS, Weight::Medium, ink, Align::Center);
        if d == today {
            ui.tooltip(cell, "Today");
        }
    }
    r.h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_filled_as_the_page_writes_it() {
        let args = vec![("n".to_string(), "101".to_string()), ("date".to_string(), "2024-03-06".to_string()), ("amount".to_string(), "120000".to_string())];
        assert_eq!(words("Bus %{n} broke down on its tour: in the workshop until %{date}, repairs %{amount}.", &args), "Bus 101 broke down on its tour: in the workshop until Wed 6 Mar 2024, repairs €1,200.");
        assert_eq!(value("price", "145.0"), "€1.45");
        assert_eq!(value("count", "1370"), "1,370");
        assert_eq!(value("stars", "3.8"), "3.8");
        assert_eq!(value("n", "1001"), "1001");
        assert_eq!(short_day("2024-03-08"), "Fri 8 Mar");
    }

    #[test]
    fn every_text_of_the_inbox_and_the_contracts_is_translated() {
        // (the core's texts are no literals of the pages: `locale_check` cannot see them)
        let texts: Vec<&str> = co::inbox::texts().into_iter().chain(co::contracts::texts()).collect();
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

    #[test]
    fn the_easy_mode_goes_only_to_its_own_pages() {
        let kinds = [Kind::Staff, Kind::Fleet, Kind::Workshop, Kind::Research, Kind::Contract, Kind::Goal, Kind::Finance, Kind::Lines, Kind::Event, Kind::Notice];
        for kind in kinds {
            let m = Message::new(kind, "t", "x");
            // the advanced mode: every page as before
            assert_eq!(page_for(&m, co::Mode::Advanced), page_of(&m), "{kind:?}");
            // the easy mode: a page it shows, or no button
            if let Some((tab, _, _)) = page_for(&m, co::Mode::Easy) {
                assert!(super::super::pages_of(co::Mode::Easy).contains(&tab), "{kind:?} -> {tab}");
            }
        }
        assert_eq!(page_for(&Message::new(Kind::Workshop, "t", "x"), co::Mode::Easy).map(|p| p.0), Some(1));
        assert_eq!(page_for(&Message::new(Kind::Staff, "t", "x"), co::Mode::Easy).map(|p| p.0), Some(2));
        for kind in [Kind::Research, Kind::Contract, Kind::Goal, Kind::Event] {
            assert_eq!(page_for(&Message::new(kind, "t", "x"), co::Mode::Easy), None, "{kind:?}");
        }
    }

    #[test]
    fn a_filter_shows_its_messages() {
        let mut m = Message::new(Kind::Staff, "Ill", "x");
        assert!(Filter::All.shows(&m) && Filter::Unread.shows(&m) && Filter::Of(Kind::Staff).shows(&m));
        m.read = true;
        assert!(!Filter::Unread.shows(&m) && !Filter::Of(Kind::Fleet).shows(&m));
        assert_eq!(Filter::Of(Kind::Staff).kinds(), vec![Kind::Staff]);
        assert!(Filter::Unread.kinds().is_empty());
    }
}
