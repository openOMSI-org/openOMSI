//! The contracts the company can take on, and those it has (City Bus Manager's assignments and
//! service contracts; the rules are `company::contracts`'): the week's goals with their rewards
//! in trophies and money, those running with their progress, those finished; the trophies'
//! bar to the next milestone; the buses' internet from one of four providers once WLAN is
//! researched - its GB a day, its price a day, today's data and its top-up -, a supplier's
//! fixed diesel price, and an outside cleaning company.

use super::super::theme::*;
use super::super::ui::{ButtonKind, Ui};
use super::super::Launcher;
use super::inbox::{button_w, wide_bar, words};
use super::{act, day_label, eur, eur_cents, grouped, kit, num, section};
use glam::Vec2;
use omsi_launcher_lib::company::contracts::{self as ct, Goal, State, Unit, PROVIDERS};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::cell::RefCell;

/// What the page keeps: the part shown, the GB a day each provider's slider stands at, for
/// which company.
struct View {
    company: String,
    part: usize,
    gb: [f32; 4],
}

thread_local! {
    static VIEW: RefCell<Option<View>> = const { RefCell::new(None) };
}

/// The page's view of `c` (a new one for another company).
fn view_of(c: &Company) -> (usize, [f32; 4]) {
    VIEW.with(|v| {
        let mut v = v.borrow_mut();
        if v.as_ref().is_none_or(|x| x.company != c.id) {
            let mut gb = [40.0; 4];
            if let Some(n) = c.contracts.internet.as_ref() {
                gb[n.provider.min(3)] = n.gb as f32;
            }
            *v = Some(View { company: c.id.clone(), part: 0, gb });
        }
        v.as_ref().map(|x| (x.part, x.gb)).unwrap_or((0, [40.0; 4]))
    })
}

fn keep(part: usize, gb: [f32; 4]) {
    VIEW.with(|v| {
        if let Some(x) = v.borrow_mut().as_mut() {
            (x.part, x.gb) = (part, gb);
        }
    });
}

/// A goal's progress in words: "1,343 / 1,370", "3.6 / 3.8 stars", "3 / 7 days".
pub(super) fn goal_progress(g: &Goal) -> String {
    let (p, t) = (g.progress, g.aim.target());
    match g.aim.unit() {
        Unit::Count => format!("{} / {}", grouped(p), grouped(t)),
        Unit::Km => format!("{} / {} km", grouped(p), grouped(t)),
        Unit::Stars => omsi_ui::tr("%{a} / %{b} stars").replace("%{a}", &num(p, 1)).replace("%{b}", &num(t, 1)),
        Unit::Days => omsi_ui::tr("%{a} / %{b} days").replace("%{a}", &num(p, 0)).replace("%{b}", &num(t, 0)),
    }
}

/// A goal's title in the interface's language.
pub(super) fn goal_title(g: &Goal) -> String {
    let (t, args) = g.aim.title();
    words(t, &args)
}

/// A goal's reward: its trophies and its money, side by side from `at`; returns the width.
pub(super) fn reward(ui: &mut Ui, at: Vec2, trophies: i64, money: co::Cents) -> f32 {
    ui.icon("emoji_events", Vec2::new(at.x + 9.0, at.y + 10.0), 18.0, WARN);
    let t = trophies.to_string();
    let w1 = ui.text(&t, Vec2::new(at.x + 22.0, at.y + 15.0), kit::ROWS, Weight::Bold, TEXT, Align::Left);
    let x = at.x + 22.0 + w1 + 16.0;
    ui.icon("payments", Vec2::new(x + 9.0, at.y + 10.0), 18.0, OK);
    let w2 = ui.text(&eur(money), Vec2::new(x + 22.0, at.y + 15.0), kit::ROWS, Weight::Bold, TEXT, Align::Left);
    x + 22.0 + w2 - at.x
}

/// The trophies' bar: what the goals earned in all against the next milestone, and its bonus.
pub(super) fn trophy_bar(ui: &mut Ui, r: Rect, c: &Company) {
    let (last, next) = ct::milestone(c);
    let earned = c.contracts.earned;
    let frac = if next > last { (earned - last) as f64 / (next - last) as f64 } else { 1.0 };
    ui.icon("emoji_events", Vec2::new(r.x + 14.0, r.center().y), 24.0, WARN);
    let bar = Rect::new(r.x + 34.0, r.y + (r.h - 26.0) * 0.5, (r.w - 34.0 - 40.0).max(40.0), 26.0);
    wide_bar(ui, bar, frac, OK, &format!("{} / {}", grouped(earned as f64), grouped(next as f64)));
    ui.icon("military_tech", Vec2::new(r.right() - 16.0, r.center().y), 26.0, TEXT_SOFT);
    let bonus = ct::milestone_bonus(c, next);
    let tip = omsi_ui::tr("The trophies your goals earned in all. At %{count} the company reaches its next milestone: a bonus of %{amount}.").replace("%{count}", &grouped(next as f64)).replace("%{amount}", &eur(bonus));
    ui.tooltip(r, &tip);
}

/// The company's rating as five stars, filled as far as it goes, from `at` (each `size`).
pub(super) fn stars(ui: &mut Ui, at: Vec2, size: f32, rating: f64) {
    for k in 0..5 {
        let c = Vec2::new(at.x + size * 0.5 + k as f32 * (size + 2.0), at.y + size * 0.5);
        let fill = (rating - k as f64).clamp(0.0, 1.0);
        ui.icon("star", c, size, if fill >= 0.75 { WARN } else if fill >= 0.25 { WARN.alpha(0.35) } else { TEXT_FAINT });
    }
}

pub fn draw(l: &mut Launcher, area: Rect) {
    super::inbox::ensure(l);
    let Some(c) = l.company.company.clone() else { return };
    let (mut part, mut gb) = view_of(&c);
    let gap = 14.0;
    // the figures: the trophies to spend, the rating, the goals running, the trophies' bar
    let fw = (area.w - 3.0 * gap) / 4.0;
    let fh = kit::FIGURE_H;
    let running = c.contracts.goals.iter().filter(|g| g.state == State::Running).count();
    let r0 = Rect::new(area.x, area.y, fw, fh);
    super::figure(&mut l.ui, r0, "Trophies", &grouped(c.trophies as f64), &omsi_ui::tr("to spend on research"), WARN);
    l.ui.tooltip(r0, "Trophies are earned with goals; research costs trophies as well as money.");
    let rating = ct::rating(&c);
    let r1 = Rect::new(area.x + fw + gap, area.y, fw, fh);
    super::figure(&mut l.ui, r1, "Rating", &format!("{} / 5", num(rating, 1)), "", super::grade(c.reputation + 15.0));
    stars(&mut l.ui, Vec2::new(r1.x + 16.0, r1.y + 68.0), 18.0, rating);
    l.ui.tooltip(r1, "The company's rating is its reputation in stars: trips on time and happy passengers raise it.");
    let r2 = Rect::new(area.x + 2.0 * (fw + gap), area.y, fw, fh);
    super::figure(&mut l.ui, r2, "Goals running", &format!("{running} / {}", ct::RUNNING_MAX), &omsi_ui::tr("at most three at once"), TEXT);
    let r3 = Rect::new(area.x + 3.0 * (fw + gap), area.y, fw, fh);
    l.ui.card(r3);
    kit::caps(&mut l.ui, Rect::new(r3.x + 16.0, r3.y + 13.0, r3.w - 32.0, 16.0), "Trophy milestone");
    trophy_bar(&mut l.ui, Rect::new(r3.x + 12.0, r3.y + 40.0, r3.w - 24.0, 44.0), &c);
    // the part
    let y = area.y + fh + gap;
    let parts = [omsi_ui::tr("Goals").into_owned(), omsi_ui::tr("Service contracts").into_owned()];
    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    l.ui.segmented("company-contracts-part", Rect::new(area.x, y, 420.0f32.min(area.w), 38.0), &mut part, &refs);
    let body = Rect::new(area.x, y + 38.0 + gap, area.w, (area.bottom() - y - 38.0 - gap).max(120.0));
    if part == 0 {
        goals(l, body, &c);
    } else {
        services(l, body, &c, &mut gb);
    }
    keep(part, gb);
}

// --- the goals ------------------------------------------------------------------------------------

/// The goals: on offer, running, finished - three columns.
fn goals(l: &mut Launcher, r: Rect, c: &Company) {
    let gap = 14.0;
    let w1 = (r.w - 2.0 * gap) * 0.36;
    let w3 = r.w - 2.0 * gap - 2.0 * w1;
    let offered: Vec<Goal> = c.contracts.goals.iter().filter(|g| g.state == State::Offered).cloned().collect();
    let running: Vec<Goal> = c.contracts.goals.iter().filter(|g| g.state == State::Running).cloned().collect();
    let mut finished: Vec<Goal> = c.contracts.goals.iter().filter(|g| matches!(g.state, State::Done | State::Failed)).cloned().collect();
    finished.reverse();
    let mut take: Option<u32> = None;
    let mut give_up: Option<u32> = None;
    // on offer
    let a = Rect::new(r.x, r.y, w1, r.h);
    let inner = section(&mut l.ui, a, "On offer this week");
    if let Some(g) = offered.first() {
        let until = omsi_ui::tr("until %{date}").replace("%{date}", &day_label(&g.offered_until));
        l.ui.text_in(&until, Rect::new(a.x + 18.0, a.y + 12.0, a.w - 36.0, 20.0), kit::NOTE, Weight::Medium, TEXT_DIM, Align::Right);
    }
    if offered.is_empty() {
        l.ui.paragraph("Every goal of this week is taken. New ones come on Monday.", Vec2::new(inner.x, inner.y), inner.w, kit::BODY, Weight::Regular, TEXT_SOFT);
    }
    let full = running.len() >= ct::RUNNING_MAX;
    l.ui.scroll_area("company-goals-offered", inner, &mut |ui: &mut Ui, v: Rect| {
        let mut y = v.y;
        for g in &offered {
            let title = goal_title(g);
            let th = ui.paragraph_height(&title, v.w - 28.0, kit::HEAD, Weight::Bold);
            let rule = omsi_ui::tr(g.aim.rule()).into_owned();
            let rh = ui.paragraph_height(&rule, v.w - 28.0, kit::NOTE, Weight::Regular);
            let h = 14.0 + th + 4.0 + rh + 12.0 + 22.0 + 14.0 + 34.0 + 14.0;
            let card = Rect::new(v.x, y, v.w - 8.0, h);
            if ui.rect_visible(card) {
                ui.p().rounded(card, RADIUS, Color::WHITE.alpha(0.03));
                ui.p().rounded_border(card, RADIUS, 1.0, HAIRLINE);
                let mut yy = card.y + 14.0;
                yy += ui.paragraph(&title, Vec2::new(card.x + 14.0, yy), card.w - 28.0, kit::HEAD, Weight::Bold, TEXT) + 4.0;
                yy += ui.paragraph(&rule, Vec2::new(card.x + 14.0, yy), card.w - 28.0, kit::NOTE, Weight::Regular, TEXT_DIM) + 12.0;
                reward(ui, Vec2::new(card.x + 14.0, yy), g.trophies, g.reward);
                let days = omsi_ui::tr("%{n} days to reach it").replace("%{n}", &g.days.to_string());
                ui.text_in(&days, Rect::new(card.x + 14.0, yy, card.w - 28.0, 22.0), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Right);
                yy += 22.0 + 14.0;
                let bw = button_w(ui, "Take it on", Some("flag"));
                let b = Rect::new(card.right() - 14.0 - bw, yy, bw, 34.0);
                if ui.button(&format!("company-goal-take-{}", g.id), b, "Take it on", Some("flag"), if full { ButtonKind::Normal } else { ButtonKind::Primary }) {
                    take = Some(g.id);
                }
            }
            y += h + 10.0;
        }
        y - v.y
    });
    // running
    let b = Rect::new(r.x + w1 + gap, r.y, w1, r.h);
    let inner = section(&mut l.ui, b, "Running");
    if running.is_empty() {
        l.ui.paragraph("No goal runs now. Take on up to three of those on offer: each counts your company's days from today until it is reached or its time runs out.", Vec2::new(inner.x, inner.y), inner.w, kit::BODY, Weight::Regular, TEXT_SOFT);
    }
    let today = c.date.clone();
    l.ui.scroll_area("company-goals-running", inner, &mut |ui: &mut Ui, v: Rect| {
        let mut y = v.y;
        for g in &running {
            let title = goal_title(g);
            let th = ui.paragraph_height(&title, v.w - 28.0, kit::HEAD, Weight::Bold);
            let h = 14.0 + th + 10.0 + 26.0 + 10.0 + 22.0 + 8.0 + 22.0 + 12.0 + 32.0 + 14.0;
            let card = Rect::new(v.x, y, v.w - 8.0, h);
            if ui.rect_visible(card) {
                ui.p().rounded(card, RADIUS, Color::WHITE.alpha(0.03));
                ui.p().rounded_border(card, RADIUS, 1.0, accent_2().alpha(0.4));
                let mut yy = card.y + 14.0;
                yy += ui.paragraph(&title, Vec2::new(card.x + 14.0, yy), card.w - 28.0, kit::HEAD, Weight::Bold, TEXT) + 10.0;
                let left = g.days_left(&today).unwrap_or(0);
                let colour = if g.share() >= 1.0 { OK } else if left <= 2 { WARN } else { accent_2() };
                wide_bar(ui, Rect::new(card.x + 14.0, yy, card.w - 28.0, 26.0), g.share(), colour, &goal_progress(g));
                yy += 26.0 + 10.0;
                reward(ui, Vec2::new(card.x + 14.0, yy), g.trophies, g.reward);
                yy += 22.0 + 8.0;
                let when = match left {
                    0 => omsi_ui::tr("Today is its last day").into_owned(),
                    1 => omsi_ui::tr("Until tomorrow").into_owned(),
                    n => omsi_ui::tr("Until %{date}: %{n} days left").replace("%{date}", &day_label(g.deadline.as_deref().unwrap_or(""))).replace("%{n}", &n.to_string()),
                };
                ui.text_in(&when, Rect::new(card.x + 14.0, yy, card.w - 28.0, 22.0), kit::NOTE, Weight::Medium, if left <= 2 { WARN } else { TEXT_SOFT }, Align::Left);
                yy += 22.0 + 12.0;
                let bw = button_w(ui, "Give up", None);
                ui.text_in(g.aim.rule(), Rect::new(card.x + 14.0, yy, card.w - 38.0 - bw, 32.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
                ui.tooltip(Rect::new(card.x + 14.0, yy, card.w - 38.0 - bw, 32.0), g.aim.rule());
                if ui.button(&format!("company-goal-give-up-{}", g.id), Rect::new(card.right() - 14.0 - bw, yy, bw, 32.0), "Give up", None, ButtonKind::Ghost) {
                    give_up = Some(g.id);
                }
            }
            y += h + 10.0;
        }
        y - v.y
    });
    // finished
    let f = Rect::new(r.x + 2.0 * (w1 + gap), r.y, w3, r.h);
    let inner = section(&mut l.ui, f, "Finished");
    if finished.is_empty() {
        l.ui.paragraph("The goals you reach or miss are listed here.", Vec2::new(inner.x, inner.y), inner.w, kit::BODY, Weight::Regular, TEXT_SOFT);
    }
    l.ui.scroll_area("company-goals-finished", inner, &mut |ui: &mut Ui, v: Rect| {
        let mut y = v.y;
        for g in &finished {
            let title = goal_title(g);
            let th = ui.paragraph_height(&title, v.w - 44.0, kit::ROWS, Weight::Medium);
            let h = th + 4.0 + 20.0 + 12.0;
            let done = g.state == State::Done;
            ui.icon(if done { "check_circle" } else { "error" }, Vec2::new(v.x + 11.0, y + 11.0), 20.0, if done { OK } else { TEXT_DIM });
            ui.paragraph(&title, Vec2::new(v.x + 30.0, y), v.w - 44.0, kit::ROWS, Weight::Medium, if done { TEXT } else { TEXT_SOFT });
            let ended = day_label(g.ended.as_deref().unwrap_or(""));
            let what = if done {
                omsi_ui::tr("Reached on %{date}: %{trophies} trophies, %{amount}").replace("%{date}", &ended).replace("%{trophies}", &g.trophies.to_string()).replace("%{amount}", &eur(g.reward))
            } else if g.given_up {
                omsi_ui::tr("Given up on %{date}").replace("%{date}", &ended)
            } else {
                omsi_ui::tr("Missed on %{date}").replace("%{date}", &ended)
            };
            ui.text_in(&what, Rect::new(v.x + 30.0, y + th + 2.0, v.w - 44.0, 20.0), kit::NOTE, Weight::Regular, if done { OK } else { TEXT_DIM }, Align::Left);
            ui.p().rect(Rect::new(v.x, y + h - 6.0, v.w - 8.0, 1.0), HAIRLINE);
            y += h;
        }
        y - v.y
    });
    if let Some(id) = take {
        if act(l, |c| ct::take(c, id)).is_some() {
            l.state.set_status(omsi_ui::tr("Goal taken on: it counts from today.").into_owned(), false);
        }
    }
    if let Some(id) = give_up {
        act(l, |c| ct::give_up(c, id));
    }
}

// --- the service contracts --------------------------------------------------------------------------

fn services(l: &mut Launcher, r: Rect, c: &Company, gb: &mut [f32; 4]) {
    let gap = 14.0;
    let lw = (r.w - gap) * 0.64;
    let rw = r.w - gap - lw;
    internet(l, Rect::new(r.x, r.y, lw, r.h), c, gb);
    let fh = (r.h - gap) * 0.55;
    fuel(l, Rect::new(r.x + lw + gap, r.y, rw, fh), c);
    cleaning(l, Rect::new(r.x + lw + gap, r.y + fh + gap, rw, r.h - fh - gap), c);
}

/// The buses' internet: the four providers side by side, the one the company has with today's
/// data and its top-up.
fn internet(l: &mut Launcher, r: Rect, c: &Company, gb: &mut [f32; 4]) {
    let inner = section(&mut l.ui, r, "Internet on the buses");
    if !c.research.has(co::research::keys::WLAN) {
        let mid = inner.center();
        l.ui.icon("wifi", Vec2::new(mid.x, mid.y - 70.0), 44.0, TEXT_FAINT);
        let t = "The buses have no WLAN yet. Research it, and the internet providers make their offers here: your passengers like to be online, and a connection that lasts the day raises your reputation.";
        let w = inner.w.min(560.0);
        let h = l.ui.paragraph_height(t, w, kit::BODY, Weight::Regular);
        l.ui.paragraph(t, Vec2::new(mid.x - w * 0.5, mid.y - 30.0), w, kit::BODY, Weight::Regular, TEXT_SOFT);
        let bw = button_w(&l.ui, "To the research", Some("hub"));
        if l.ui.button("company-net-research", Rect::new(mid.x - bw * 0.5, mid.y - 20.0 + h, bw, 38.0), "To the research", Some("hub"), ButtonKind::Primary) {
            l.company.tab = super::RESEARCH_TAB;
        }
        return;
    }
    let active = ct::internet_today(c).cloned();
    let next = c.contracts.internet_next.clone();
    // what the passengers used on the last day closed
    let used = c.contracts.data.used;
    let head = if used > 0.0 {
        omsi_ui::tr("Used yesterday: %{n} GB").replace("%{n}", &num(used, 1))
    } else {
        let pax = c.history.last().map(|d| d.passengers as f64).unwrap_or(0.0);
        omsi_ui::tr("Your passengers would use about %{n} GB a day").replace("%{n}", &num(pax * ct::GB_PER_PASSENGER, 0))
    };
    l.ui.icon("info", Vec2::new(inner.x + 9.0, inner.y + 10.0), 16.0, TEXT_DIM);
    l.ui.text_in(&head, Rect::new(inner.x + 24.0, inner.y, inner.w - 24.0, 20.0), kit::ROWS, Weight::Medium, TEXT_SOFT, Align::Left);
    let n = PROVIDERS.len() as f32;
    let cg = 12.0;
    let cw = (inner.w - (n - 1.0) * cg) / n;
    let top = inner.y + 32.0;
    let ch = (inner.bottom() - top).max(300.0);
    let mut chosen: Option<(usize, u32)> = None;
    let mut cancel = false;
    let mut top_up: Option<u32> = None;
    for (k, p) in PROVIDERS.iter().enumerate() {
        let card = Rect::new(inner.x + k as f32 * (cw + cg), top, cw, ch);
        let mine = active.as_ref().is_some_and(|a| a.provider == k);
        let coming = matches!(next, Some(ct::NetChange::Take { provider, .. }) if provider == k);
        l.ui.p().rounded(card, RADIUS, if mine { accent_2().alpha(0.08) } else { Color::WHITE.alpha(0.03) });
        l.ui.p().rounded_border(card, RADIUS, 1.0, if mine { accent_2().alpha(0.6) } else { HAIRLINE });
        let x = card.x + 14.0;
        let w = card.w - 28.0;
        let mut y = card.y + 14.0;
        l.ui.icon("wifi", Vec2::new(card.center().x, y + 14.0), 28.0, if mine { accent_2() } else { TEXT_SOFT });
        y += 36.0;
        l.ui.text_in(p.name, Rect::new(x, y, w, 26.0), kit::HEAD + 1.0, Weight::Bold, TEXT, Align::Center);
        y += 30.0;
        let sat = omsi_ui::tr("%{pct} % customer satisfaction").replace("%{pct}", &p.satisfaction.to_string());
        l.ui.text_in(&sat, Rect::new(x, y, w, 20.0), kit::NOTE, Weight::Medium, super::grade(p.satisfaction as f64), Align::Center);
        y += 24.0;
        super::meter(&mut l.ui, Rect::new(x, y, w, 5.0), p.satisfaction as f64 / 100.0, super::grade(p.satisfaction as f64));
        y += 16.0;
        let per = omsi_ui::tr("%{amount} a GB a day").replace("%{amount}", &eur_cents(p.per_gb * c.price_index));
        l.ui.text_in(&per, Rect::new(x, y, w, 20.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Center);
        y += 30.0;
        // today's data, on the contract that runs
        if let (true, Some(d)) = (mine, ct::data_today(c)) {
            let all = (d.gb + d.topped).max(0.1);
            let left = d.left();
            let colour = if left / all > 0.3 { OK } else if left > 0.0 { WARN } else { DANGER };
            wide_bar(&mut l.ui, Rect::new(x, y, w, 24.0), left / all, colour, &omsi_ui::tr("%{n} GB left today").replace("%{n}", &num(left, 1)));
            y += 32.0;
            let more = ((d.gb * 0.5 / 5.0).round() * 5.0).max(5.0) as u32;
            let label = omsi_ui::tr("+%{n} GB for %{amount}").replace("%{n}", &more.to_string()).replace("%{amount}", &eur(ct::top_up_price(c, k, more)));
            if l.ui.button(&format!("company-net-top-up-{k}"), Rect::new(x, y, w, 32.0), &label, None, ButtonKind::Normal) {
                top_up = Some(more);
            }
            l.ui.tooltip(Rect::new(x, y, w, 32.0), "More data for today only, paid at once");
            y += 44.0;
        }
        // the GB a day (from tomorrow)
        let mut v = gb[k];
        l.ui.slider(&format!("company-net-gb-{k}"), Rect::new(x, y, w, 30.0), &mut v, ct::GB_MIN as f32, ct::GB_MAX as f32, ct::GB_STEP as f32, "", &|v| format!("{v:.0} GB"));
        gb[k] = v;
        l.ui.tooltip(Rect::new(x, y, w, 30.0), "The GB a day the contract gives the buses' WLAN");
        y += 38.0;
        let gbs = v.round() as u32;
        l.ui.text_in(&omsi_ui::tr("%{amount} a day").replace("%{amount}", &eur(ct::net_price(c, k, gbs))), Rect::new(x, y, w, 30.0), 22.0, Weight::Bold, TEXT, Align::Center);
        y += 34.0;
        l.ui.text_in("Changes are active from tomorrow", Rect::new(x, y, w, 18.0), kit::NOTE - 1.0, Weight::Regular, TEXT_DIM, Align::Center);
        y += 26.0;
        // what is coming, and the buttons
        let mut state = String::new();
        if coming {
            if let Some(ct::NetChange::Take { gb: g, .. }) = &next {
                state = omsi_ui::tr("From tomorrow: %{n} GB a day").replace("%{n}", &g.to_string());
            }
        } else if mine && next == Some(ct::NetChange::Cancel) {
            state = omsi_ui::tr("Ends tonight").into_owned();
        } else if mine {
            state = omsi_ui::tr("Your contract: %{n} GB a day").replace("%{n}", &active.as_ref().map(|a| a.gb).unwrap_or(0).to_string());
        }
        if !state.is_empty() {
            l.ui.text_in(&state, Rect::new(x, y, w, 20.0), kit::NOTE, Weight::Bold, if coming { accent_2() } else if mine { OK } else { WARN }, Align::Center);
        }
        let by = card.bottom() - 14.0 - 34.0;
        let current = mine && active.as_ref().is_some_and(|a| a.gb == gbs) && !matches!(next, Some(ct::NetChange::Take { .. }) | Some(ct::NetChange::Cancel));
        if mine {
            let half = (w - 8.0) * 0.5;
            if !current && l.ui.button(&format!("company-net-change-{k}"), Rect::new(x, by, half, 34.0), if next == Some(ct::NetChange::Cancel) { "Keep it" } else { "Change" }, None, ButtonKind::Primary) {
                chosen = Some((k, gbs));
            }
            if next != Some(ct::NetChange::Cancel) && l.ui.button(&format!("company-net-cancel-{k}"), Rect::new(x + half + 8.0, by, half, 34.0), "Cancel", None, ButtonKind::Danger) {
                cancel = true;
            }
        } else if !coming || next.as_ref().is_some_and(|n| matches!(n, ct::NetChange::Take { gb: g, .. } if *g != gbs)) {
            if l.ui.button(&format!("company-net-accept-{k}"), Rect::new(x, by, w, 34.0), "Accept", None, ButtonKind::Primary) {
                chosen = Some((k, gbs));
            }
        }
    }
    if let Some((k, g)) = chosen {
        if act(l, |c| ct::take_internet(c, k, g)).is_some() {
            l.state.set_status(omsi_ui::tr("The internet contract changes from tomorrow.").into_owned(), false);
        }
    }
    if cancel {
        act(l, |c| {
            ct::cancel_internet(c);
            Ok(())
        });
    }
    if let Some(g) = top_up {
        if let Some(price) = act(l, |c| ct::top_up(c, g)) {
            l.state.set_status(omsi_ui::tr("Topped up: %{n} GB for %{amount}.").replace("%{n}", &g.to_string()).replace("%{amount}", &eur(price)), false);
        }
    }
}

/// A supplier's fixed diesel price: the market's today, the terms offered, the one running.
fn fuel(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Diesel supplier");
    let usual = co::inbox::usual_diesel(c);
    let market = co::inbox::market_fuel_factor(c, &c.date);
    let mut y = inner.y;
    l.ui.icon("water_drop", Vec2::new(inner.x + 10.0, y + 13.0), 20.0, TEXT_SOFT);
    let today = omsi_ui::tr("Diesel today: %{price} a litre").replace("%{price}", &eur_cents(usual * market));
    l.ui.text_in(&today, Rect::new(inner.x + 28.0, y, inner.w - 28.0, 26.0), kit::HEAD, Weight::Bold, TEXT, Align::Left);
    y += 28.0;
    if (market - 1.0).abs() > 0.005 {
        let t = omsi_ui::tr("%{pct} against the usual %{price}, by the market's events").replace("%{pct}", &format!("{:+.0} %", (market - 1.0) * 100.0)).replace("%{price}", &eur_cents(usual));
        l.ui.text_in(&t, Rect::new(inner.x + 28.0, y, inner.w - 28.0, 20.0), kit::NOTE, Weight::Medium, if market > 1.0 { WARN } else { OK }, Align::Left);
        y += 24.0;
    }
    y += 8.0;
    let running = c.contracts.fuel.clone();
    match running {
        Some(d) => {
            let started = d.from.as_str() <= c.date.as_str();
            let t = if started { "Fixed at %{price} a litre until %{date}." } else { "Fixed at %{price} a litre from tomorrow until %{date}." };
            let t = omsi_ui::tr(t).replace("%{price}", &eur_cents(d.price)).replace("%{date}", &day_label(&d.until));
            y += l.ui.paragraph(&t, Vec2::new(inner.x, y), inner.w, kit::BODY, Weight::Medium, OK) + 6.0;
            l.ui.paragraph("A fixed price runs until its last day: it cannot be ended early.", Vec2::new(inner.x, y), inner.w, kit::NOTE, Weight::Regular, TEXT_DIM);
        }
        None => {
            y += l.ui.paragraph("Fix the price for a while: from tomorrow the company pays the same, whatever the market does. Sign before a price rise the inbox tells of - but a fixed price also stays when diesel gets cheaper.", Vec2::new(inner.x, y), inner.w, kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT) + 10.0;
            let mut sign: Option<usize> = None;
            for k in 0..ct::FUEL_TERMS.len() {
                let (days, _) = ct::FUEL_TERMS[k];
                let (price, until) = ct::fuel_offer(c, k);
                let row = Rect::new(inner.x, y, inner.w, 36.0);
                if !l.ui.rect_visible(row) || row.bottom() > inner.bottom() + 4.0 {
                    break;
                }
                let t = omsi_ui::tr("%{n} days: %{price} a litre, until %{date}").replace("%{n}", &days.to_string()).replace("%{price}", &eur_cents(price)).replace("%{date}", &day_label(&until));
                let bw = button_w(&l.ui, "Fix it", None);
                l.ui.text_in(&t, Rect::new(row.x, row.y, row.w - bw - 10.0, row.h), kit::ROWS, Weight::Regular, TEXT, Align::Left);
                if l.ui.button(&format!("company-fuel-sign-{k}"), Rect::new(row.right() - bw, row.y + 1.0, bw, 34.0), "Fix it", None, ButtonKind::Normal) {
                    sign = Some(k);
                }
                y += 42.0;
            }
            if let Some(k) = sign {
                if act(l, |c| ct::sign_fuel(c, k)).is_some() {
                    l.state.set_status(omsi_ui::tr("The diesel's price is fixed from tomorrow.").into_owned(), false);
                }
            }
        }
    }
}

/// The outside cleaning company: what it asks, what it does, signed or ended.
fn cleaning(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Cleaning company");
    let buses = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
    let per_bus = (ct::CLEANING_PER_BUS * c.price_index / 100.0).round() as i64 * 100;
    let mut y = inner.y;
    let t = omsi_ui::tr("An outside company cleans your buses every night: %{amount} a bus, %{total} a day for your %{n} buses. Clean buses please the passengers: the reputation rises a little every day they run.").replace("%{amount}", &eur(per_bus)).replace("%{total}", &eur(ct::cleaning_price(c))).replace("%{n}", &buses.to_string());
    y += l.ui.paragraph(&t, Vec2::new(inner.x, y), inner.w, kit::NOTE + 0.5, Weight::Regular, TEXT_SOFT) + 10.0;
    let k = c.contracts.cleaning.clone();
    let (state, colour) = match &k {
        Some(k) if k.since.as_str() > c.date.as_str() => (omsi_ui::tr("Starts tomorrow").into_owned(), accent_2()),
        Some(k) if k.until.as_deref() == Some(c.date.as_str()) => (omsi_ui::tr("Ends tonight").into_owned(), WARN),
        Some(k) => (omsi_ui::tr("Cleaning your buses since %{date}").replace("%{date}", &day_label(&k.since)), OK),
        None => (String::new(), TEXT),
    };
    if !state.is_empty() {
        l.ui.text_in(&state, Rect::new(inner.x, y, inner.w, 22.0), kit::ROWS, Weight::Bold, colour, Align::Left);
        y += 30.0;
    }
    let by = (inner.bottom() - 36.0).max(y);
    let open = k.as_ref().is_some_and(|k| k.until.is_none());
    if open {
        if l.ui.button("company-cleaning-end", Rect::new(inner.x, by, button_w(&l.ui, "End the contract", None), 34.0), "End the contract", None, ButtonKind::Danger) {
            act(l, |c| {
                ct::cancel_cleaning(c);
                Ok(())
            });
        }
    } else if l.ui.button("company-cleaning-sign", Rect::new(inner.x, by, button_w(&l.ui, "Sign the contract", Some("check_circle")), 34.0), "Sign the contract", Some("check_circle"), ButtonKind::Primary) {
        act(l, ct::sign_cleaning);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::contracts::Aim;

    fn goal(aim: Aim, progress: f64) -> Goal {
        Goal { id: 1, aim, trophies: 20, reward: 14_381_00, offered_until: "2024-03-10".into(), days: 7, state: State::Running, taken: None, deadline: None, progress, ended: None, given_up: false }
    }

    #[test]
    fn a_goals_progress_is_said_in_its_unit() {
        assert_eq!(goal_progress(&goal(Aim::Trips { n: 1_370 }, 1_343.0)), "1,343 / 1,370");
        assert_eq!(goal_progress(&goal(Aim::Rating { stars: 3.8 }, 3.6)), "3.6 / 3.8 stars");
        assert_eq!(goal_progress(&goal(Aim::NoDrops { days: 7 }, 3.0)), "3 / 7 days");
        assert_eq!(goal_title(&goal(Aim::Trips { n: 1_370 }, 0.0)), "Complete 1,370 trips successfully");
        assert_eq!(goal_title(&goal(Aim::Line { line: "5".into(), days: 5 }, 0.0)), "Run line 5 on time for 5 days in a row");
    }
}
