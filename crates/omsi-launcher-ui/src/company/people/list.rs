//! My employees (City Bus Manager's staff list, Luc's wish): chips for the roles with how many
//! of each there are, a sort and its direction, and a card for everyone - their face, role, name
//! and wage by the hour, what they are doing now (from today's plan and their role's hours), and
//! their level, satisfaction, stress and friendliness as bars. A card opens the person: their
//! details and needs, their pay and a raise or the one they asked for, a dismissal, and the
//! courses of their work.

use super::super::super::theme::*;
use super::super::super::ui::{ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::kit;
use super::super::{act, day_label, eur, eur_cents, grade, meter, num, Confirm, Dialog};
use super::portraits;
use glam::Vec2;
use omsi_launcher_lib::company::day::Plan;
use omsi_launcher_lib::company::staff::{self, Grade};
use omsi_launcher_lib::company::training::{self, CourseKind};
use omsi_launcher_lib::company::{self as co, Company, Employee, Role};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// The ways the list is sorted.
const SORTS: [&str; 6] = ["Name", "Role", "Wage", "Level", "Satisfaction", "Stress"];

/// A card's height.
const CARD_H: f32 = 116.0;

/// The colour of a level.
pub(super) fn grade_colour(g: Grade) -> Color {
    match g {
        Grade::Trainee => DANGER.lighten(0.15),
        Grade::Novice => WARN,
        Grade::Experienced => EARLY_SOFT,
        Grade::Expert => OK,
    }
}

/// "06:00 - 14:00" of minutes of the day.
pub(super) fn span(from: i32, to: i32) -> String {
    format!("{} - {}", co::clock::hhmm(from as i64), co::clock::hhmm(to as i64))
}

/// Someone's hours of a shift: the core, and when they stay on.
pub(super) fn shift_text(s: &staff::Shift) -> String {
    match s.extra {
        Some((a, b)) if a == s.to => span(s.from, b),
        Some((a, b)) => format!("{}  ·  {}", span(s.from, s.to), span(a, b)),
        None => span(s.from, s.to),
    }
}

/// An hourly wage: "€18.70 / hour".
pub(super) fn per_hour(wage: co::Cents) -> String {
    omsi_ui::tr("%{amount} / hour").replace("%{amount}", &eur_cents(staff::hourly(wage)))
}

/// What someone is doing now (the company's clock): two lines and their colour - driving a bus
/// on line 5 from 04:35 to 12:40, at work by their role's hours, resting, ill, on holiday, on a
/// course.
pub(super) fn doing(c: &Company, plan: Option<&Plan>, e: &Employee) -> (String, String, Color) {
    let today = c.date.as_str();
    let now = c.clock.minute as i32;
    let until = |u: &Option<String>| u.clone().filter(|u| co::dates::between(today, u) >= 0);
    let until_text = |u: &str| omsi_ui::tr("until %{date}").replace("%{date}", &day_label(u));
    if let Some(u) = until(&e.sick_until) {
        return (omsi_ui::tr("Ill").into_owned(), until_text(&u), WARN);
    }
    if let Some(u) = until(&e.holiday_until) {
        return (omsi_ui::tr("On holiday").into_owned(), until_text(&u), TEXT_SOFT);
    }
    if let Some(u) = until(&e.training_until) {
        return (omsi_ui::tr("On a course").into_owned(), until_text(&u), accent_2());
    }
    if staff::drives(e) {
        let mut duties: Vec<(i32, i32, String)> = plan
            .map(|p| p.tours.iter().filter(|t| !t.tour.unplanned).flat_map(|t| t.duties.iter().filter(|d| d.driver == Some(e.id)).map(move |d| (d.from, d.to, t.tour.number.clone()))).collect())
            .unwrap_or_default();
        duties.sort_by_key(|d| d.0);
        let (Some(first), Some(last)) = (duties.first(), duties.last()) else {
            let off = e.week_days >= staff::WEEK_DAYS;
            return (omsi_ui::tr(if off { "Days off" } else { "Free today" }).into_owned(), String::new(), TEXT_SOFT);
        };
        let day = span(first.0, last.1);
        return match duties.iter().find(|d| d.0 <= now && now < d.1) {
            Some(d) => (omsi_ui::tr("Driving a bus").into_owned(), format!("{}  ·  {}", omsi_ui::tr("Line %{n}").replace("%{n}", &d.2), span(d.0, d.1)), OK),
            None if now < first.0 => (omsi_ui::tr("Drives today").into_owned(), day, TEXT),
            None if now >= last.1 => (omsi_ui::tr("Resting").into_owned(), day, TEXT_SOFT),
            None => (omsi_ui::tr("On a break").into_owned(), day, TEXT_SOFT),
        };
    }
    let Some(s) = staff::shift_of(c, today, e) else {
        let free = !staff::on_rota(e, today);
        return (omsi_ui::tr(if free { "Day off" } else { "Resting" }).into_owned(), String::new(), TEXT_SOFT);
    };
    let hours = shift_text(&s);
    if s.at(now) {
        let detail = match e.role {
            Role::Inspector => staff::inspected(c, today).iter().find(|x| x.2.contains(&e.id)).and_then(|x| c.lines.iter().find(|l| l.name == x.0)).map(|l| omsi_ui::tr("Line %{n}").replace("%{n}", &l.number)).unwrap_or(hours),
            _ => hours,
        };
        (omsi_ui::tr(staff::activity(e.role)).into_owned(), detail, OK)
    } else if now < s.from {
        (omsi_ui::tr("Starts at %{time}").replace("%{time}", &co::clock::hhmm(s.from as i64)), hours, TEXT)
    } else {
        (omsi_ui::tr("Resting").into_owned(), hours, TEXT_SOFT)
    }
}

/// A need's bar with its name before it ("Stress ▬▬▬"): the colour from green to red.
fn need(ui: &mut Ui, r: Rect, label: &str, value: f64, good_high: bool, bold: bool) {
    let lw = (r.w * 0.42).min(130.0);
    let (px, weight, ink) = if bold { (15.0, Weight::Bold, TEXT) } else { (kit::NOTE, Weight::Regular, TEXT_SOFT) };
    ui.text_in(label, Rect::new(r.x, r.y, lw - 10.0, r.h), px, weight, ink, Align::Right);
    let c = if good_high { grade(value) } else { grade(100.0 - value) };
    meter(ui, Rect::new(r.x + lw, r.center().y - 5.0, r.w - lw, 10.0), value / 100.0, c);
}

/// The four bars of someone: their level, satisfaction, stress and friendliness.
fn needs(ui: &mut Ui, r: Rect, e: &Employee) {
    let g = Grade::of(e.experience);
    let row = r.h / 4.0;
    let lw = (r.w * 0.42).min(130.0);
    ui.text_in(g.label(), Rect::new(r.x, r.y, lw - 10.0, row), 16.0, Weight::Bold, TEXT, Align::Right);
    meter(ui, Rect::new(r.x + lw, r.y + row * 0.5 - 5.0, r.w - lw, 10.0), e.experience / 100.0, grade_colour(g));
    ui.tooltip(Rect::new(r.x, r.y, r.w, row), &omsi_ui::tr("Experience %{n} of 100: Trainee, Novice, Experienced, Expert").replace("%{n}", &num(e.experience, 0)));
    need(ui, Rect::new(r.x, r.y + row, r.w, row), "Satisfaction", e.satisfaction, true, false);
    need(ui, Rect::new(r.x, r.y + 2.0 * row, r.w, row), "Stress", e.stress, false, false);
    need(ui, Rect::new(r.x, r.y + 3.0 * row, r.w, row), "Friendliness", e.friendliness, true, false);
}

/// Whether `e` is shown under the chip `filter` (0: all).
fn shown(filter: usize, e: &Employee) -> bool {
    filter == 0 || Role::ALL.get(filter - 1) == Some(&e.role)
}

fn sort(list: &mut [Employee], by: usize, descending: bool) {
    list.sort_by(|a, b| {
        let o = match by {
            1 => Role::ALL.iter().position(|r| *r == a.role).cmp(&Role::ALL.iter().position(|r| *r == b.role)).then(a.name.cmp(&b.name)),
            2 => a.wage.cmp(&b.wage),
            3 => a.experience.total_cmp(&b.experience),
            4 => a.satisfaction.total_cmp(&b.satisfaction),
            5 => a.stress.total_cmp(&b.stress),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        };
        if descending {
            o.reverse()
        } else {
            o
        }
    });
}

pub(super) fn draw(l: &mut Launcher, area: Rect, c: &Company) {
    if let Some(id) = l.company.people.open {
        if c.employee(id).is_some() {
            person(l, area, c, id);
            return;
        }
        l.company.people.open = None;
    }
    if c.staff.is_empty() {
        let h = l.ui.paragraph("Nobody works here yet. Every duty of a tour needs a driver; mechanics, cleaners, service employees and ticket inspectors help the company in their own ways. Hire some among the week's applicants.", Vec2::new(area.x, area.y + 4.0), area.w.min(860.0), kit::BODY, Weight::Regular, TEXT_SOFT);
        let bw = kit::Foot::width(&l.ui, "To the applicants", Some("groups"));
        if l.ui.button("company-to-applicants", Rect::new(area.x, area.y + h + 18.0, bw, 40.0), "To the applicants", Some("groups"), ButtonKind::Primary) {
            l.company.people.tab = 1;
        }
        return;
    }
    // the chips: all, and every role with how many there are
    let mut labels = vec![format!("{}  {}", omsi_ui::tr("All"), c.staff.len())];
    labels.extend(Role::ALL.iter().map(|r| format!("{}  {}", omsi_ui::tr(staff::plural(*r)), c.staff.iter().filter(|e| e.role == *r).count())));
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let sort_w = 260.0f32.min(area.w * 0.3);
    let chips_w = (area.w - sort_w - 24.0).max(200.0);
    let chips_h = l.ui.chips_height(chips_w, 34.0, &refs);
    let mut filter = l.company.people.filter.min(Role::ALL.len());
    if l.ui.chips("company-staff-roles", Rect::new(area.x, area.y, chips_w, 34.0), &mut filter, &refs) {
        l.company.people.filter = filter;
    }
    // the sort and its direction
    let options: Vec<String> = SORTS.iter().map(|s| omsi_ui::tr(s).into_owned()).collect();
    let mut by = l.company.people.sort.min(SORTS.len() - 1);
    let sr = Rect::new(area.right() - sort_w, area.y, sort_w - 44.0, 34.0);
    l.ui.text_in("Sort by", Rect::new(sr.x - 90.0, sr.y, 82.0, sr.h), kit::NOTE, Weight::Medium, TEXT_DIM, Align::Right);
    if l.ui.select("company-staff-sort", sr, &mut by, &options) {
        l.company.people.sort = by;
    }
    let desc = l.company.people.descending;
    if l.ui.icon_button("company-staff-sort-dir", Vec2::new(sr.right() + 24.0, sr.center().y), 17.0, if desc { "arrow_downward" } else { "arrow_upward" }, if desc { "Descending" } else { "Ascending" }) {
        l.company.people.descending = !desc;
    }
    let mut list: Vec<Employee> = c.staff.iter().filter(|e| shown(filter, e)).cloned().collect();
    sort(&mut list, by, desc);
    let plan = l.company.plan.clone();
    let now: Vec<(String, String, Color)> = list.iter().map(|e| doing(c, plan.as_ref(), e)).collect();
    let faces = portraits::textures(l, list.iter().map(|e| e.portrait()));
    let asks: Vec<u32> = c.staff_rules.asks.iter().map(|a| a.id).collect();
    let top = area.y + chips_h + 16.0;
    let rows = Rect::new(area.x, top, area.w, (area.bottom() - top).max(0.0));
    let cols = if rows.w >= 1500.0 { 2 } else { 1 };
    let mut opened: Option<u32> = None;
    l.ui.scroll_area("company-staff-cards", rows, &mut |ui, v| {
        let gap = 12.0;
        let cw = (v.w - 12.0 - gap * (cols - 1) as f32) / cols as f32;
        for (k, e) in list.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + gap), v.y + (k / cols) as f32 * (CARD_H + 10.0), cw, CARD_H);
            if !ui.rect_visible(r) {
                continue;
            }
            ui.p().rounded(r, RADIUS, PANEL);
            if ui.row(&format!("company-staff-card-{}", e.id), r, false) {
                opened = Some(e.id);
            }
            ui.p().rounded_border(r, RADIUS, 1.0, HAIRLINE);
            card(ui, r, e, faces.get(&e.portrait()).copied(), &now[k], asks.contains(&e.id));
        }
        list.len().div_ceil(cols) as f32 * (CARD_H + 10.0)
    });
    if let Some(id) = opened {
        l.company.people.open = Some(id);
    }
}

/// A card's insides: the face, who they are and their wage, what they are doing, the bars.
fn card(ui: &mut Ui, r: Rect, e: &Employee, face: Option<usize>, now: &(String, String, Color), asks: bool) {
    let pic = Rect::new(r.x + 14.0, r.y + 14.0, r.h - 28.0, r.h - 28.0);
    portraits::draw(ui, pic, face, &e.name);
    let tx = pic.right() + 18.0;
    let bars_w = (r.w * 0.3).clamp(220.0, 330.0);
    let text_w = ((r.w - (tx - r.x) - bars_w - 30.0) * 0.45).max(120.0);
    let mut x = tx;
    ui.icon(staff::icon(e.role), Vec2::new(x + 9.0, r.y + 26.0), 16.0, accent_2());
    x += 24.0;
    ui.text_in(e.role.label(), Rect::new(x, r.y + 14.0, text_w - 24.0, 24.0), 16.0, Weight::Bold, TEXT, Align::Left);
    ui.text_in(&e.name, Rect::new(tx, r.y + 42.0, text_w, 24.0), 15.0, Weight::Medium, TEXT_SOFT, Align::Left);
    ui.text_in(&per_hour(e.wage), Rect::new(tx, r.y + 70.0, text_w, 26.0), 16.0, Weight::Bold, TEXT, Align::Left);
    // (what calls for an answer)
    if e.notice_until.is_some() {
        let t = omsi_ui::tr(if e.resigned { "Resigned" } else { "Leaving" });
        kit::tag(ui, Vec2::new(tx + text_w * 0.62, r.y + 72.0), &t, DANGER.lighten(0.25));
    } else if asks {
        kit::tag(ui, Vec2::new(tx + text_w * 0.62, r.y + 72.0), &omsi_ui::tr("Asks for a raise"), WARN);
    }
    // what they are doing now
    let mid = Rect::new(tx + text_w + 10.0, r.y, (r.right() - bars_w - 30.0 - (tx + text_w + 10.0)).max(60.0), r.h);
    ui.text_in(&now.0, Rect::new(mid.x, mid.center().y - 26.0, mid.w, 26.0), 16.0, Weight::Medium, now.2, Align::Center);
    if !now.1.is_empty() {
        ui.text_in(&now.1, Rect::new(mid.x, mid.center().y + 2.0, mid.w, 24.0), 15.0, Weight::Regular, TEXT_SOFT, Align::Center);
    }
    needs(ui, Rect::new(r.right() - bars_w - 18.0, r.y + 10.0, bars_w, r.h - 20.0), e);
}

/// Someone opened: who they are and what they do, their needs, their pay and a raise (or the one
/// they asked for), a dismissal, and the courses of their work.
fn person(l: &mut Launcher, area: Rect, c: &Company, id: u32) {
    let Some(e) = c.employee(id).cloned() else { return };
    let back_w = kit::Foot::width(&l.ui, "All employees", Some("chevron_left"));
    if l.ui.button("company-staff-back", Rect::new(area.x, area.y, back_w, 36.0), "All employees", Some("chevron_left"), ButtonKind::Normal) || l.ui.input.keys.contains(&super::super::super::ui::Key::Escape) {
        l.company.people.open = None;
        return;
    }
    let body = Rect::new(area.x, area.y + 52.0, area.w, (area.h - 52.0).max(0.0));
    // who they are
    let left = Rect::new(body.x, body.y, 320.0f32.min(body.w * 0.3), body.h);
    l.ui.card(left);
    let side = (left.w - 60.0).min(240.0);
    let pic = Rect::new(left.x + (left.w - side) * 0.5, left.y + 24.0, side, side);
    let face = portraits::texture(l, e.portrait());
    portraits::draw(&mut l.ui, pic, face, &e.name);
    let mut y = pic.bottom() + 16.0;
    l.ui.text_in(&e.name, Rect::new(left.x + 20.0, y, left.w - 40.0, 28.0), kit::TITLE, Weight::Bold, TEXT, Align::Center);
    y += 30.0;
    l.ui.text_in(e.role.label(), Rect::new(left.x + 20.0, y, left.w - 40.0, 22.0), kit::HEAD - 1.0, Weight::Medium, accent_2(), Align::Center);
    y += 26.0;
    let since = omsi_ui::tr("%{age}, since %{date}").replace("%{age}", &e.age.to_string()).replace("%{date}", &day_label(&e.hired));
    l.ui.text_in(&since, Rect::new(left.x + 20.0, y, left.w - 40.0, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Center);
    y += 34.0;
    let plan = l.company.plan.clone();
    let (a, b, col) = doing(c, plan.as_ref(), &e);
    l.ui.text_in(&a, Rect::new(left.x + 20.0, y, left.w - 40.0, 24.0), kit::HEAD, Weight::Bold, col, Align::Center);
    if !b.is_empty() {
        l.ui.text_in(&b, Rect::new(left.x + 20.0, y + 26.0, left.w - 40.0, 22.0), kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Center);
    }
    y += 60.0;
    if let Some(u) = e.notice_until.as_deref() {
        let t = omsi_ui::tr(if e.resigned { "Resigned: leaves after %{date}" } else { "Leaves after %{date}" }).replace("%{date}", &day_label(u));
        l.ui.text_in(&t, Rect::new(left.x + 20.0, y, left.w - 40.0, 22.0), kit::ROWS, Weight::Medium, DANGER.lighten(0.25), Align::Center);
    }

    // the needs, the pay, the work, the courses
    let rx = left.right() + 18.0;
    let rw = (body.right() - rx).max(200.0);
    let half = (rw - 18.0) / 2.0;
    let needs_r = Rect::new(rx, body.y, half, 300.0);
    let inner = super::super::section(&mut l.ui, needs_r, "Their needs");
    let g = Grade::of(e.experience);
    let rows: [(String, f64, bool, &str); 4] = [
        (omsi_ui::tr(g.label()).into_owned(), e.experience, true, "Experience grows with every day worked."),
        (omsi_ui::tr("Satisfaction").into_owned(), e.satisfaction, true, "Their pay against the market, their stress and their holidays. Under 25 they may leave."),
        (omsi_ui::tr("Stress").into_owned(), e.stress, false, "Overtime, nights, short rests and too few colleagues raise it; days off bring it down."),
        (omsi_ui::tr("Friendliness").into_owned(), e.friendliness, true, "Grows with experience and the customer service course, falls with stress. The passengers feel it."),
    ];
    let mut yy = inner.y;
    for (k, (label, value, good, note)) in rows.iter().enumerate() {
        let c2 = if k == 0 { grade_colour(g) } else if *good { grade(*value) } else { grade(100.0 - value) };
        l.ui.text_in(label, Rect::new(inner.x, yy, inner.w * 0.5, 22.0), kit::ROWS, Weight::Bold, TEXT, Align::Left);
        l.ui.text_in(&num(*value, 0), Rect::new(inner.x, yy, inner.w, 22.0), kit::ROWS, Weight::Bold, c2, Align::Right);
        meter(&mut l.ui, Rect::new(inner.x, yy + 26.0, inner.w, 8.0), value / 100.0, c2);
        l.ui.text_in(note, Rect::new(inner.x, yy + 36.0, inner.w, 18.0), kit::NOTE - 1.0, Weight::Regular, TEXT_DIM, Align::Left);
        yy += 62.0;
    }

    // the pay
    let pay_r = Rect::new(rx + half + 18.0, body.y, half, 300.0);
    let inner = super::super::section(&mut l.ui, pay_r, "Pay");
    let market = staff::market_wage(e.role, e.experience, c.price_index);
    let lines: [(&str, String); 4] = [
        ("A month", eur(e.wage)),
        ("By the hour", per_hour(e.wage)),
        ("The market pays", eur(market)),
        ("Costs a month", eur(staff::monthly_cost(&e))),
    ];
    let mut yy = inner.y;
    for (label, value) in lines {
        l.ui.text_in(label, Rect::new(inner.x, yy, inner.w * 0.55, 24.0), kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Left);
        l.ui.text_in(&value, Rect::new(inner.x + inner.w * 0.4, yy, inner.w * 0.6, 24.0), kit::ROWS, Weight::Bold, TEXT, Align::Right);
        yy += 28.0;
    }
    if !staff::drives(&e) {
        l.ui.text_in("Paid by the hour: the night 50 % more, at least four hours a day worked.", Rect::new(inner.x, yy, inner.w, 20.0), kit::NOTE - 1.0, Weight::Regular, TEXT_DIM, Align::Left);
    }
    yy += 28.0;
    let mut action: Option<u8> = None;
    if let Some(ask) = c.staff_rules.ask_of(e.id).cloned() {
        let t = omsi_ui::tr("Asks for %{n} % more, until %{date}").replace("%{n}", &format!("{:.0}", ask.share * 100.0)).replace("%{date}", &day_label(&ask.until));
        l.ui.text_in(&t, Rect::new(inner.x, yy, inner.w, 22.0), kit::ROWS, Weight::Bold, WARN, Align::Left);
        yy += 30.0;
        let bw = (inner.w - 10.0) / 2.0;
        if l.ui.button("company-staff-grant", Rect::new(inner.x, yy, bw, 36.0), "Grant the raise", Some("check"), ButtonKind::Primary) {
            action = Some(3);
        }
        if l.ui.button("company-staff-refuse", Rect::new(inner.x + bw + 10.0, yy, bw, 36.0), "Refuse", Some("close"), ButtonKind::Normal) {
            action = Some(4);
        }
    } else {
        let bw = (inner.w - 10.0) / 2.0;
        if l.ui.button("company-staff-raise", Rect::new(inner.x, yy, bw, 36.0), "A raise of 5 %", Some("trending_up"), ButtonKind::Normal) {
            action = Some(0);
        }
        if e.notice_until.is_some() {
            if !e.resigned && l.ui.button("company-staff-keep", Rect::new(inner.x + bw + 10.0, yy, bw, 36.0), "Take the dismissal back", Some("restart_alt"), ButtonKind::Normal) {
                action = Some(2);
            }
        } else if l.ui.button("company-staff-dismiss", Rect::new(inner.x + bw + 10.0, yy, bw, 36.0), "Dismiss", Some("logout"), ButtonKind::Danger) {
            action = Some(1);
        }
    }

    // the work
    let work_r = Rect::new(rx, body.y + 318.0, half, (body.h - 318.0).max(120.0));
    let inner = super::super::section(&mut l.ui, work_r, "Work");
    let mut facts: Vec<(String, String)> = vec![
        (omsi_ui::tr("Reliable").into_owned(), format!("{:.0} %", e.reliability * 100.0)),
        (omsi_ui::tr("Days worked").into_owned(), e.days_worked.to_string()),
        (omsi_ui::tr("Holiday days left").into_owned(), e.holiday_left.to_string()),
    ];
    if staff::drives(&e) {
        facts.insert(0, (omsi_ui::tr("Licence").into_owned(), super::licence_line(e.licence, &e.endorsements)));
        facts.insert(1, (omsi_ui::tr("Driving · punctuality · service").into_owned(), format!("{:.0} · {:.0} · {:.0}", e.skills.driving, e.skills.punctuality, e.skills.service)));
        facts.insert(2, (omsi_ui::tr("Bus types").into_owned(), e.types.len().to_string()));
    } else {
        let hours = staff::shift_of(c, &c.date, &e).map(|s| shift_text(&s)).unwrap_or_else(|| omsi_ui::tr("Not at work").into_owned());
        facts.insert(0, (omsi_ui::tr("Today").into_owned(), hours));
        facts.insert(1, (omsi_ui::tr("Service skill").into_owned(), format!("{:.0}", e.skills.service)));
    }
    let mut yy = inner.y;
    for (label, value) in &facts {
        if yy + 26.0 > inner.bottom() {
            break;
        }
        l.ui.text_in(label, Rect::new(inner.x, yy, inner.w * 0.6, 24.0), kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Left);
        l.ui.text_in(value, Rect::new(inner.x + inner.w * 0.4, yy, inner.w * 0.6, 24.0), kit::ROWS, Weight::Bold, TEXT, Align::Right);
        yy += 28.0;
    }

    // the courses of their work
    let courses_r = Rect::new(rx + half + 18.0, body.y + 318.0, half, (body.h - 318.0).max(120.0));
    let inner = super::super::section(&mut l.ui, courses_r, "Courses");
    let mut book: Option<CourseKind> = None;
    let mut lock: Option<CourseKind> = None;
    let mut yy = inner.y;
    for kind in staff::courses_for(e.role).iter().copied() {
        if yy + 40.0 > inner.bottom() {
            break;
        }
        let row = Rect::new(inner.x, yy, inner.w, 38.0);
        l.ui.icon(kind.icon(), Vec2::new(row.x + 10.0, row.center().y), 16.0, accent_2());
        l.ui.text_in(kind.label(), Rect::new(row.x + 28.0, row.y, row.w * 0.5, row.h), kit::ROWS, Weight::Bold, TEXT, Align::Left);
        l.ui.tooltip(Rect::new(row.x, row.y, row.w * 0.55, row.h), &omsi_ui::tr(kind.effect()));
        let cell = Rect::new(row.right() - 170.0, row.y + 2.0, 170.0, row.h - 4.0);
        if training::trained(c, Some(e.id), kind) {
            l.ui.icon("check_circle", Vec2::new(cell.right() - 12.0, cell.center().y), 17.0, OK);
        } else if let Some(x) = training::course_of(c, Some(e.id), kind).filter(|x| !x.done) {
            let t = omsi_ui::tr("Until %{date}").replace("%{date}", &day_label(&x.until));
            l.ui.text_in(&t, cell, kit::NOTE, Weight::Medium, accent_2(), Align::Right);
        } else if !co::levels::unlocked(c, kind.spec().feature) {
            let t = omsi_ui::tr("From level %{n}").replace("%{n}", &kind.spec().feature.level().to_string());
            if l.ui.button(&format!("company-staff-course-lock-{}", kind.label()), cell, &t, Some("lock"), ButtonKind::Ghost) {
                lock = Some(kind);
            }
        } else {
            let label = format!("{}  ·  {}", omsi_ui::tr("Book"), eur(training::cost_of(c, kind)));
            if l.ui.button(&format!("company-staff-course-{}", kind.label()), cell, &label, Some("add"), ButtonKind::Ghost) {
                book = Some(kind);
            }
        }
        yy += 42.0;
    }
    if staff::drives(&e) && yy + 24.0 <= inner.bottom() {
        l.ui.text_in("Licences and bus types: the Licences & training tab.", Rect::new(inner.x, yy, inner.w, 22.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
    }

    if let Some(kind) = lock {
        kit::show(l, kit::locked(c, kind.spec().feature));
    }
    if let Some(kind) = book {
        if let Some(until) = act(l, |c| training::enrol(c, kind, Some(id))) {
            l.state.set_status(omsi_ui::tr("Booked: %{course}, until %{date}.").replace("%{course}", &omsi_ui::tr(kind.label())).replace("%{date}", &day_label(&until)), false);
        }
    }
    match action {
        Some(0) => {
            act(l, |c| {
                staff::raise(c, id, 0.05);
                Ok(())
            });
        }
        Some(1) => l.company.dialog = Some(Dialog::Confirm { what: Confirm::Dismiss(id) }),
        Some(2) => {
            act(l, |c| staff::withdraw_notice(c, id));
        }
        Some(3) => {
            act(l, |c| staff::answer_raise(c, id, true));
        }
        Some(4) => {
            act(l, |c| staff::answer_raise(c, id, false));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Difficulty, Founding};

    #[test]
    fn the_list_is_sorted_and_filtered_by_role() {
        let mut c = found(&Founding { name: "Liste".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        for role in [Role::Driver, Role::Cleaner, Role::Mechanic] {
            let a = staff::applicants_of(&c, role)[0].clone();
            staff::hire(&mut c, &a).unwrap();
        }
        let mut list = c.staff.clone();
        sort(&mut list, 1, false);
        assert_eq!(list.iter().map(|e| e.role).collect::<Vec<_>>(), vec![Role::Driver, Role::Mechanic, Role::Cleaner]);
        sort(&mut list, 2, true);
        assert!(list.windows(2).all(|w| w[0].wage >= w[1].wage));
        assert!(c.staff.iter().all(|e| shown(0, e)));
        assert_eq!(c.staff.iter().filter(|e| shown(2, e)).count(), 1);
        // what they do now: off duty without a plan, the others by their hours
        let (driver, _, _) = doing(&c, None, &c.staff[0]);
        assert_eq!(driver, omsi_ui::tr("Free today"));
        assert_eq!(span(4 * 60 + 35, 12 * 60 + 40), "04:35 - 12:40");
    }

    /// The string literals in `src` right after `before`: up to `n` of them, one after the other
    /// with a comma between (an occurrence not followed by a literal gives none).
    fn literals_after<'a>(src: &'a str, before: &str, n: usize) -> Vec<&'a str> {
        let mut out = Vec::new();
        for (at, _) in src.match_indices(before) {
            let mut rest = &src[at + before.len()..];
            for k in 0..n {
                rest = rest.trim_start();
                if k > 0 {
                    let Some(r) = rest.strip_prefix(',') else { break };
                    rest = r.trim_start();
                }
                let Some(r) = rest.strip_prefix('"') else { break };
                let Some(len) = r.find('"') else { break };
                out.push(&r[..len]);
                rest = &r[len + 1..];
            }
        }
        out
    }

    #[test]
    fn what_the_staff_pages_and_the_inbox_show_is_translated() {
        let mut keys: Vec<&str> = Role::ALL.iter().flat_map(|r| [staff::plural(*r), staff::activity(*r), r.label()]).collect();
        keys.extend(Grade::ALL.iter().map(|g| g.label()));
        keys.extend(SORTS);
        keys.extend(super::super::TABS);
        // (handed to the controls in variables: the checks of the sources do not see them)
        keys.extend([
            "Their needs",
            "Pay",
            "Work",
            "Courses",
            "Today",
            "Hours of each",
            "Experience grows with every day worked.",
            "Their pay against the market, their stress and their holidays. Under 25 they may leave.",
            "Overtime, nights, short rests and too few colleagues raise it; days off bring it down.",
            "Grows with experience and the customer service course, falls with stress. The passengers feel it.",
            "A month",
            "By the hour",
            "The market pays",
            "Costs a month",
            "Descending",
            "Ascending",
            "Resigned",
            "Leaving",
        ]);
        // what the inbox is told of the staff, and the refusals of the staff's rules
        let src = include_str!("../../../../omsi-launcher-core/src/company/staff.rs");
        let src = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
        let told = literals_after(src, "Message::new(Kind::Staff, ", 2);
        assert!(told.len() >= 16, "{told:?}");
        keys.extend(told);
        for role in ["Role::Mechanic => (", "Role::Cleaner => (", "Role::Service => ("] {
            keys.extend(literals_after(src, role, 2));
        }
        keys.extend(literals_after(src, "Err(", 1));
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in &keys {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }
}
