//! Hire employees: the week's applicants of every role, a chip for each role with how many
//! apply, and a card for each applicant - their face (one nobody of the company has), who they
//! are, their level, what they ask a month and by the hour against the market, what they bring
//! (a driver's licence and skills, the others' friendliness and reliability) - and Hire. Ticket
//! inspectors wait for their research.

use super::super::super::theme::*;
use super::super::super::ui::{ButtonKind, Ui};
use super::super::super::Launcher;
use super::super::kit;
use super::super::{act, day_label, eur, grade, meter, num};
use super::list::{grade_colour, per_hour};
use super::portraits;
use glam::Vec2;
use omsi_launcher_lib::company::staff::{self, Applicant, Grade};
use omsi_launcher_lib::company::{self as co, Company, Licence, Role};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

/// A card's height.
const CARD_H: f32 = 206.0;

pub(super) fn draw(l: &mut Launcher, area: Rect, c: &Company) {
    let market = staff::market(c);
    let labels: Vec<String> = Role::ALL.iter().map(|r| format!("{}  {}", omsi_ui::tr(staff::plural(*r)), market.iter().filter(|a| a.role == *r).count())).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let chips_h = l.ui.chips_height(area.w, 34.0, &refs);
    let mut pick = l.company.people.hire_role.min(Role::ALL.len() - 1);
    if l.ui.chips("company-hire-roles", Rect::new(area.x, area.y, area.w, 34.0), &mut pick, &refs) {
        l.company.people.hire_role = pick;
    }
    let role = Role::ALL[pick];
    let top = area.y + chips_h + 16.0;
    let foot = Rect::new(area.x, area.bottom() - 30.0, area.w, 30.0);
    let body = Rect::new(area.x, top, area.w, (foot.y - 10.0 - top).max(0.0));
    let t = omsi_ui::tr("New applicants come on %{date}. Every role has its own market: wages around what the market pays for their experience.").replace("%{date}", &day_label(&co::network::next_monday(c)));
    l.ui.text_in(&t, foot, kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    // (ticket inspectors: after their research)
    if let Err(why) = staff::hireable(c, role) {
        l.ui.icon("lock", Vec2::new(body.x + 14.0, body.y + 14.0), 20.0, TEXT_DIM);
        let mut h = l.ui.paragraph(why, Vec2::new(body.x + 34.0, body.y), body.w.min(760.0), kit::BODY, Weight::Regular, TEXT_SOFT);
        // (the easy mode does not research it by itself, and its research is the advanced
        // mode's page: the way there is the advanced mode)
        let go = if co::autopilot::on(c) {
            h += 6.0 + l.ui.paragraph(super::super::NOT_RESEARCHED_BY_ITSELF, Vec2::new(body.x + 34.0, body.y + h + 6.0), body.w.min(760.0), kit::NOTE, Weight::Regular, TEXT_DIM);
            kit::Go::Advanced(super::super::RESEARCH_TAB)
        } else {
            kit::Go::Research
        };
        let bw = kit::Foot::width(&l.ui, go.label(), Some(go.icon()));
        if l.ui.button("company-hire-research", Rect::new(body.x + 34.0, body.y + h + 16.0, bw, 38.0), go.label(), Some(go.icon()), ButtonKind::Primary) {
            super::super::go(l, go);
        }
        return;
    }
    let mine: Vec<Applicant> = market.into_iter().filter(|a| a.role == role).collect();
    if mine.is_empty() {
        l.ui.text_in("Nobody else applies for this work this week.", Rect::new(body.x, body.y, body.w, 26.0), kit::BODY, Weight::Medium, TEXT_SOFT, Align::Left);
        return;
    }
    let faces = portraits::textures(l, mine.iter().map(|a| a.portrait()));
    let price_index = c.price_index;
    let cols = ((body.w / 440.0).floor() as usize).clamp(1, 4);
    let mut hire: Option<usize> = None;
    l.ui.scroll_area("company-hire-cards", body, &mut |ui, v| {
        let gap = 14.0;
        let cw = (v.w - 12.0 - gap * (cols - 1) as f32) / cols as f32;
        for (k, a) in mine.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + gap), v.y + (k / cols) as f32 * (CARD_H + 14.0), cw, CARD_H);
            if !ui.rect_visible(r) {
                continue;
            }
            if card(ui, r, a, faces.get(&a.portrait()).copied(), price_index) {
                hire = Some(k);
            }
        }
        mine.len().div_ceil(cols) as f32 * (CARD_H + 14.0)
    });
    if let Some(k) = hire {
        let a = mine[k].clone();
        if act(l, |c| staff::hire(c, &a)).is_some() {
            l.state.set_status(omsi_ui::tr("%{name} works for %{company} from today.").replace("%{name}", &a.name).replace("%{company}", &c.name), false);
        }
    }
}

/// An applicant's card; returns whether Hire was clicked.
fn card(ui: &mut Ui, r: Rect, a: &Applicant, face: Option<usize>, price_index: f64) -> bool {
    ui.p().rounded(r, RADIUS, PANEL);
    ui.p().rounded_border(r, RADIUS, 1.0, HAIRLINE);
    let pic = Rect::new(r.x + 16.0, r.y + 16.0, 96.0, 96.0);
    portraits::draw(ui, pic, face, &a.name);
    let tx = pic.right() + 16.0;
    let tw = r.right() - tx - 16.0;
    ui.text_in(&a.name, Rect::new(tx, r.y + 14.0, tw, 24.0), kit::HEAD, Weight::Bold, TEXT, Align::Left);
    let who = format!("{}  ·  {}", omsi_ui::tr(a.role.label()), omsi_ui::tr("%{n} years old").replace("%{n}", &a.age.to_string()));
    ui.text_in(&who, Rect::new(tx, r.y + 40.0, tw, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    // the level
    let g = Grade::of(a.experience);
    ui.text_in(g.label(), Rect::new(tx, r.y + 66.0, 110.0, 20.0), kit::ROWS, Weight::Bold, grade_colour(g), Align::Left);
    meter(ui, Rect::new(tx + 112.0, r.y + 72.0, (tw - 112.0).max(20.0), 8.0), a.experience / 100.0, grade_colour(g));
    ui.tooltip(Rect::new(tx, r.y + 62.0, tw, 26.0), &omsi_ui::tr("Experience %{n} of 100: Trainee, Novice, Experienced, Expert").replace("%{n}", &num(a.experience, 0)));
    // what they ask, against the market
    let market = staff::market_wage(a.role, a.experience, price_index);
    let asks = format!("{}  ·  {}", omsi_ui::tr("%{amount} a month").replace("%{amount}", &eur(a.wage)), per_hour(a.wage));
    ui.text_in(&asks, Rect::new(tx, r.y + 92.0, tw, 22.0), kit::ROWS, Weight::Bold, TEXT, Align::Left);
    let vs = a.wage as f64 / market.max(1) as f64 - 1.0;
    let against = if vs.abs() < 0.015 { omsi_ui::tr("as the market pays").into_owned() } else if vs > 0.0 { omsi_ui::tr("%{n} % over the market").replace("%{n}", &format!("{:.0}", vs * 100.0)) } else { omsi_ui::tr("%{n} % under the market").replace("%{n}", &format!("{:.0}", -vs * 100.0)) };
    ui.text_in(&against, Rect::new(tx, r.y + 114.0, tw, 18.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
    // what they bring
    let y = r.y + 140.0;
    if a.role == Role::Driver {
        let lic = super::licence_line(a.licence, &a.endorsements);
        ui.text_in(&lic, Rect::new(r.x + 16.0, y, 90.0, 22.0), kit::ROWS, Weight::Bold, if a.licence == Licence::D1 { WARN } else { TEXT }, Align::Left);
        ui.tooltip(Rect::new(r.x + 16.0, y, 90.0, 22.0), &super::endorsements_tip(a.licence, &a.endorsements));
        let skills = format!("{}  {:.0} · {:.0} · {:.0}", omsi_ui::tr("Skills"), a.skills.driving, a.skills.punctuality, a.skills.service);
        ui.text_in(&skills, Rect::new(r.x + 110.0, y, (r.w - 126.0) * 0.6, 22.0), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Left);
        ui.tooltip(Rect::new(r.x + 110.0, y, (r.w - 126.0) * 0.6, 22.0), "Driving · punctuality · service");
    } else {
        let f = a.friendliness();
        ui.text_in("Friendliness", Rect::new(r.x + 16.0, y, 110.0, 22.0), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Left);
        ui.text_in(&num(f, 0), Rect::new(r.x + 126.0, y, 40.0, 22.0), kit::ROWS, Weight::Bold, grade(f), Align::Left);
    }
    let rel = format!("{}  {:.0} %", omsi_ui::tr("Reliable"), a.reliability * 100.0);
    ui.text_in(&rel, Rect::new(r.x + 16.0, y + 24.0, (r.w - 32.0) * 0.6, 22.0), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Left);
    ui.tooltip(Rect::new(r.x + 16.0, y + 24.0, (r.w - 32.0) * 0.6, 22.0), "How seldom they fall ill or come late");
    let b = Rect::new(r.right() - 136.0, r.bottom() - 52.0, 120.0, 38.0);
    ui.button(&format!("company-hire-{}", a.no), b, "Hire", Some("person_add"), ButtonKind::Primary)
}
