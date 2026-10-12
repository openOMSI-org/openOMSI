//! The work schedule (City Bus Manager's, Luc's wish) and the staff's settings. The schedule:
//! for each role besides the drivers its core hours - from and to, with arrows - when all of
//! them who work that day are there, and how many stay on outside them (the least stressed, in
//! turns); the night supplement and the four hours paid at least are said, and what the hours
//! give today: who is at work, the hours paid, the wages, and what the role's work comes to.
//! Setting the hours asks for the research of the shift schedule. The settings: the automatic
//! hiring of a minimum for each role, wages that follow the prices, and small raises granted
//! without asking.

use super::super::super::theme::*;
use super::super::super::ui::ButtonKind;
use super::super::super::Launcher;
use super::super::kit;
use super::super::{act, eur, num};
use super::list::shift_text;
use glam::Vec2;
use omsi_launcher_lib::company::research::keys;
use omsi_launcher_lib::company::staff::{self, Hours};
use omsi_launcher_lib::company::{Company, Role};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

/// Between the core hours' from and to (no words: not translated).
const DASH: &str = "–";

pub(super) fn schedule(l: &mut Launcher, area: Rect, c: &Company) {
    let labels: Vec<String> = staff::OTHERS.iter().map(|r| format!("{}  {}", omsi_ui::tr(staff::plural(*r)), c.staff.iter().filter(|e| e.role == *r).count())).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let chips_h = l.ui.chips_height(area.w, 34.0, &refs);
    let mut pick = l.company.people.hours_role.min(staff::OTHERS.len() - 1);
    if l.ui.chips("company-hours-roles", Rect::new(area.x, area.y, area.w, 34.0), &mut pick, &refs) {
        l.company.people.hours_role = pick;
    }
    let role = staff::OTHERS[pick];
    let open = c.research.has(keys::SHIFT_SCHEDULE);
    let h = c.staff_rules.hours_of(role);
    let top = area.y + chips_h + 24.0;
    let left = Rect::new(area.x, top, (area.w * 0.56).min(760.0), (area.bottom() - top).max(0.0));
    let cx = left.center().x;
    let mut y = left.y;
    // the core hours
    l.ui.text_in("Core hours", Rect::new(left.x, y, left.w, 28.0), kit::TITLE, Weight::Bold, TEXT, Align::Center);
    y += 44.0;
    let (mut from, mut to) = (h.from, h.to);
    let fw = 150.0;
    let fr = Rect::new(cx - fw - 30.0, y, fw, 52.0);
    let tr_ = Rect::new(cx + 30.0, y, fw, 52.0);
    if open {
        let a = l.ui.time_field(&format!("company-hours-from-{pick}"), fr, &mut from);
        let b = l.ui.time_field(&format!("company-hours-to-{pick}"), tr_, &mut to);
        if (a || b) && from != to {
            act(l, |c| staff::set_hours(c, role, Hours { from, to, outside: h.outside }));
        }
    } else {
        for (r, m) in [(fr, h.from), (tr_, h.to)] {
            l.ui.p().rounded(r, RADIUS, FIELD);
            l.ui.text_in(&omsi_launcher_lib::company::clock::hhmm(m as i64), r, 22.0, Weight::Bold, TEXT_SOFT, Align::Center);
        }
    }
    l.ui.text_in(DASH, Rect::new(cx - 30.0, y, 60.0, 52.0), 22.0, Weight::Bold, TEXT_DIM, Align::Center);
    y += 66.0;
    l.ui.text_in("During the core hours all of them who work that day are there.", Rect::new(left.x, y, left.w, 22.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Center);
    y += 46.0;
    // who stays on outside them
    l.ui.text_in("Present outside the core hours", Rect::new(left.x, y, left.w, 28.0), kit::TITLE, Weight::Bold, TEXT, Align::Center);
    y += 40.0;
    let have = c.staff.iter().filter(|e| e.role == role && e.notice_until.is_none()).count() as u32;
    let most = have.max(h.outside).max(1);
    let sr = Rect::new(left.x + left.w * 0.2, y, left.w * 0.6, 36.0);
    if open {
        let mut v = h.outside as f32;
        if l.ui.slider(&format!("company-hours-outside-{pick}"), sr, &mut v, 0.0, most as f32, 1.0, "", &|v| format!("{v:.0}")) {
            let outside = v.round().max(0.0) as u32;
            act(l, |c| staff::set_hours(c, role, Hours { outside, ..h }));
        }
    } else {
        l.ui.text_in(&h.outside.to_string(), sr, 22.0, Weight::Bold, TEXT_SOFT, Align::Center);
    }
    y += 50.0;
    for (k, t) in [
        "The least stressed stay on after the core hours, in turns: nobody works more than 12 hours a day.",
        "Between 20:00 and 06:00 they are paid 50 % more (night supplement).",
        "Everyone is paid for at least 4 hours on a day they work: core hours shorter than that are not worth it.",
        "Each works five days a week; their days off follow one another round the week.",
    ]
    .iter()
    .enumerate()
    {
        let ph = l.ui.paragraph(t, Vec2::new(left.x + 20.0, y), left.w - 40.0, kit::ROWS, Weight::Regular, if k == 1 { TEXT } else { TEXT_SOFT });
        y += ph + 10.0;
    }
    if !open {
        y += 8.0;
        l.ui.icon("lock", Vec2::new(left.x + 30.0, y + 12.0), 18.0, WARN);
        let mut ph = l.ui.paragraph("The working hours can be set once the company has researched the shift schedule.", Vec2::new(left.x + 48.0, y), left.w - 68.0, kit::ROWS, Weight::Medium, WARN);
        // (the easy mode does not research it by itself: the way there is the advanced mode)
        let go = if omsi_launcher_lib::company::autopilot::on(c) {
            ph += 6.0 + l.ui.paragraph(super::super::NOT_RESEARCHED_BY_ITSELF, Vec2::new(left.x + 48.0, y + ph + 6.0), left.w - 68.0, kit::NOTE, Weight::Regular, TEXT_DIM);
            kit::Go::Advanced(super::super::RESEARCH_TAB)
        } else {
            kit::Go::Research
        };
        let bw = kit::Foot::width(&l.ui, go.label(), Some(go.icon()));
        if l.ui.button("company-hours-research", Rect::new(left.x + 48.0, y + ph + 10.0, bw, 36.0), go.label(), Some(go.icon()), ButtonKind::Normal) {
            super::super::go(l, go);
        }
    }

    // today, for this role
    let right = Rect::new(left.right() + 30.0, top, (area.right() - left.right() - 30.0).max(0.0), (area.bottom() - top).max(0.0));
    if right.w < 260.0 {
        return;
    }
    let card = Rect::new(right.x, right.y, right.w, 360.0f32.min(right.h));
    let inner = super::super::section(&mut l.ui, card, "Today");
    let (shifts, uncovered) = staff::shifts(c, &c.date, role);
    let all = c.staff.iter().filter(|e| e.role == role && e.employed_on(&c.date)).count();
    let paid: f64 = shifts.iter().map(|s| s.paid() as f64 / 60.0).sum();
    let night: f64 = shifts.iter().map(|s| s.night() as f64 / 60.0).sum();
    let wages: f64 = shifts.iter().filter_map(|s| c.employee(s.id).map(|e| staff::hourly(e.wage) * (s.paid() as f64 + staff::NIGHT_SUPPLEMENT * s.night() as f64) / 60.0)).sum::<f64>() * (1.0 + omsi_launcher_lib::company::economy::EMPLOYER_SHARE);
    let asked = h.outside as i32 * (1440 - h.length());
    let mut rows: Vec<(String, String)> = vec![
        (omsi_ui::tr("At work").into_owned(), format!("{} / {}", shifts.len(), all)),
        (omsi_ui::tr("Hours paid").into_owned(), format!("{} h", num(paid, 1))),
        (omsi_ui::tr("Of them at night").into_owned(), format!("{} h", num(night, 1))),
        (omsi_ui::tr("Wages today").into_owned(), eur(wages.round() as i64)),
    ];
    if asked > 0 {
        rows.push((omsi_ui::tr("Outside the core hours covered").into_owned(), format!("{:.0} %", 100.0 * (asked - uncovered) as f64 / asked as f64)));
    }
    let last = c.staff_rules.last.clone().unwrap_or_default();
    let what = match role {
        Role::Mechanic => Some((omsi_ui::tr("In-house maintenance and repairs").into_owned(), format!("{:.0} % (≤ {:.0} %)", 100.0 * staff::workshop_share(c, &c.date), 100.0 * staff::WORKSHOP_MAX))),
        Role::Cleaner if !last.date.is_empty() => Some((omsi_ui::tr("Buses cleaned last night").into_owned(), format!("{}  ·  {:.0} %", last.cleaned, last.clean))),
        Role::Service if !last.date.is_empty() => Some((omsi_ui::tr("Complaints answered yesterday").into_owned(), format!("{} / {}", last.handled, last.complaints))),
        Role::Inspector => {
            let lines: Vec<String> = staff::inspected(c, &c.date).iter().filter_map(|x| c.lines.iter().find(|l| l.name == x.0).map(|l| l.number.clone())).collect();
            Some((omsi_ui::tr("Lines checked today").into_owned(), if lines.is_empty() { "–".into() } else { lines.join(", ") }))
        }
        _ => None,
    };
    rows.extend(what);
    let mut yy = inner.y;
    for (label, value) in rows {
        l.ui.text_in(&label, Rect::new(inner.x, yy, inner.w * 0.65, 24.0), kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Left);
        l.ui.text_in(&value, Rect::new(inner.x + inner.w * 0.4, yy, inner.w * 0.6, 24.0), kit::ROWS, Weight::Bold, TEXT, Align::Right);
        yy += 30.0;
    }
    // who works when
    let list = Rect::new(right.x, card.bottom() + 16.0, right.w, (right.bottom() - card.bottom() - 16.0).max(0.0));
    if list.h > 60.0 && !shifts.is_empty() {
        kit::caps(&mut l.ui, Rect::new(list.x, list.y, list.w, 16.0), "Hours of each");
        let names: Vec<(String, String, bool)> = shifts.iter().map(|s| (c.employee(s.id).map(|e| e.name.clone()).unwrap_or_default(), shift_text(s), s.extra.is_some())).collect();
        let rows_r = Rect::new(list.x, list.y + 26.0, list.w, list.h - 26.0);
        l.ui.scroll_area("company-hours-each", rows_r, &mut |ui, v| {
            for (k, (name, hours, stays)) in names.iter().enumerate() {
                let r = Rect::new(v.x, v.y + k as f32 * 28.0, v.w - 12.0, 26.0);
                if !ui.rect_visible(r) {
                    continue;
                }
                ui.text_in(name, Rect::new(r.x, r.y, r.w * 0.5, r.h), kit::ROWS, Weight::Medium, TEXT, Align::Left);
                ui.text_in(hours, Rect::new(r.x + r.w * 0.4, r.y, r.w * 0.6, r.h), kit::ROWS, Weight::Regular, if *stays { WARN } else { TEXT_SOFT }, Align::Right);
            }
            names.len() as f32 * 28.0
        });
    }
}

pub(super) fn settings(l: &mut Launcher, area: Rect, c: &Company) {
    let w = area.w.min(760.0);
    let mut y = area.y;
    let rules = c.staff_rules.clone();
    // the automatic hiring
    let mut auto = rules.auto_hire;
    if l.ui.toggle("company-staff-auto-hire", Rect::new(area.x, y, w, 36.0), &mut auto, "Hire automatically to keep a minimum of each role") {
        act(l, |c| {
            c.staff_rules.auto_hire = auto;
            Ok(())
        });
    }
    y += 40.0;
    let ph = l.ui.paragraph("At night the best applicant of the week is hired for a role that has fewer than its minimum - one a night for each role, while the cash allows.", Vec2::new(area.x, y), w, kit::NOTE, Weight::Regular, TEXT_DIM);
    y += ph + 12.0;
    let mut change: Option<(Role, u32)> = None;
    for role in Role::ALL {
        let r = Rect::new(area.x + 20.0, y, w - 20.0, 34.0);
        let n = rules.minimum_of(role);
        let have = c.staff.iter().filter(|e| e.role == role && e.notice_until.is_none()).count();
        l.ui.icon(staff::icon(role), Vec2::new(r.x + 10.0, r.center().y), 16.0, if rules.auto_hire { accent_2() } else { TEXT_FAINT });
        l.ui.text_in(staff::plural(role), Rect::new(r.x + 28.0, r.y, r.w * 0.45, r.h), kit::ROWS, Weight::Medium, if rules.auto_hire { TEXT } else { TEXT_DIM }, Align::Left);
        let x = r.x + r.w * 0.5;
        if l.ui.icon_button(&format!("company-staff-min-down-{role:?}"), Vec2::new(x + 14.0, r.center().y), 15.0, "remove", "Fewer") && n > 0 {
            change = Some((role, n - 1));
        }
        l.ui.text_in(&n.to_string(), Rect::new(x + 30.0, r.y, 44.0, r.h), kit::HEAD, Weight::Bold, TEXT, Align::Center);
        if l.ui.icon_button(&format!("company-staff-min-up-{role:?}"), Vec2::new(x + 90.0, r.center().y), 15.0, "add", "More") && n < 99 {
            change = Some((role, n + 1));
        }
        let t = omsi_ui::tr("%{n} now").replace("%{n}", &have.to_string());
        l.ui.text_in(&t, Rect::new(x + 120.0, r.y, r.right() - x - 120.0, r.h), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
        y += 38.0;
    }
    if let Some((role, n)) = change {
        act(l, |c| {
            c.staff_rules.set_minimum(role, n);
            Ok(())
        });
    }
    y += 18.0;
    l.ui.p().rect(Rect::new(area.x, y, w, 1.0), HAIRLINE);
    y += 18.0;
    // the wages and the prices
    let mut follow = rules.follow_prices;
    if l.ui.toggle("company-staff-follow-prices", Rect::new(area.x, y, w, 36.0), &mut follow, "Wages rise with the prices at every month's end") {
        act(l, |c| {
            c.staff_rules.follow_prices = follow;
            Ok(())
        });
    }
    y += 40.0;
    let ph = l.ui.paragraph("Satisfaction follows a wage against what the market pays, and the market rises with the prices: without this, everyone earns a little less each month.", Vec2::new(area.x, y), w, kit::NOTE, Weight::Regular, TEXT_DIM);
    y += ph + 18.0;
    let mut grant = rules.grant_small;
    if l.ui.toggle("company-staff-grant-small", Rect::new(area.x, y, w, 36.0), &mut grant, "Grant requests for a raise of up to 5 % without asking") {
        act(l, |c| {
            c.staff_rules.grant_small = grant;
            Ok(())
        });
    }
    y += 40.0;
    l.ui.paragraph("Someone who earns less than the market and is not satisfied asks for a raise now and then. Larger requests wait for your answer on their card; no answer within a week is a no.", Vec2::new(area.x, y), w, kit::NOTE, Weight::Regular, TEXT_DIM);
}
