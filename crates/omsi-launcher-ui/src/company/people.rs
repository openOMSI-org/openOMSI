//! The staff and the labour market, as City Bus Manager's staff pages (Luc's wish): six kinds
//! of employees - bus drivers, mechanics, researchers, cleaners, service employees and ticket
//! inspectors - each with a face of their own and their needs. Its tabs: My employees, a card
//! for everyone with what they do now and their level, satisfaction, stress and friendliness,
//! and a person opened with a raise, a dismissal and their courses (`list`); Hire employees, the
//! week's applicants of every role (`hire`); Licences & training, who of the drivers may drive
//! which bus, and the courses that open more (`co::licences`); Work schedule, the core hours of
//! the roles besides the drivers and who stays on after them; and Settings, the automatic
//! hiring, wages that follow the prices and small raises granted without asking (`hours`).

mod hire;
mod hours;
mod list;
mod portraits;

use super::super::theme::*;
use super::super::ui::ButtonKind;
use super::super::Launcher;
use super::kit;
use super::{act, day_label, eur};
use glam::Vec2;
use omsi_launcher_lib::company::licences::{self, Endorsement, Lack};
use omsi_launcher_lib::company::staff;
use omsi_launcher_lib::company::training::{self, CourseKind};
use omsi_launcher_lib::company::{self as co, Company, Employee, Licence};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

#[derive(Default)]
pub struct PeopleView {
    tab: usize,
    /// The training matrix's group (licences, bus types, courses) and its first column shown
    /// (more columns than fit: paged).
    group: usize,
    first: usize,
    /// My employees: the role shown (0: all, else `Role::ALL[k - 1]`), the sort (`list::SORTS`)
    /// and its direction, and the person opened.
    filter: usize,
    sort: usize,
    descending: bool,
    open: Option<u32>,
    /// The role of Hire employees (`Role::ALL`), and of the work schedule (`staff::OTHERS`).
    hire_role: usize,
    hours_role: usize,
}

/// The tabs of the Staff page.
const TABS: [&str; 5] = ["My employees", "Hire employees", "Licences & training", "Work schedule", "Settings"];

/// The driver applicants (a popup's "Hire drivers").
pub(super) fn to_applicants(l: &mut Launcher) {
    l.company.people.tab = 1;
    l.company.people.hire_role = 0;
}

/// The staff's courses (a day report's "Train the drivers").
pub(super) fn to_courses(l: &mut Launcher) {
    l.company.tab = 2;
    l.company.people.tab = 2;
    l.company.people.group = 2;
    l.company.people.first = 0;
}

/// The type trainings of the fleet's models (a bus card's "Train drivers").
pub(super) fn to_training(l: &mut Launcher) {
    l.company.tab = 2;
    l.company.people.tab = 2;
    l.company.people.group = 1;
    l.company.people.first = 0;
}

/// What a driver lacks for a bus, in words.
pub(super) fn lack_text(c: &Company, x: &Lack) -> String {
    match x {
        Lack::Licence => omsi_ui::tr("No licence for this bus").into_owned(),
        Lack::Endorsement(e) => omsi_ui::tr(e.lacking()).into_owned(),
        Lack::Type(k) => omsi_ui::tr("No type training for the %{bus}").replace("%{bus}", &licences::type_name(c, k)),
    }
}

/// A bus joined the fleet: how many drivers may drive it, when not all.
pub(super) fn drivers_note(c: &Company, id: u32) -> Option<String> {
    let v = c.vehicle(id)?;
    let (q, m) = licences::qualified_drivers(c, v.kind, &v.bus);
    (m > 0 && q < m).then(|| omsi_ui::tr("%{q} of %{m} drivers may drive it: train more on the Staff page.").replace("%{q}", &q.to_string()).replace("%{m}", &m.to_string()))
}

/// The licence and the endorsements: "D", "D · G E".
fn licence_line(l: Licence, e: &[Endorsement]) -> String {
    let mut s = licence_text(l).to_string();
    if !e.is_empty() {
        s.push_str(" · ");
        s.push_str(&e.iter().map(|x| x.short()).collect::<Vec<_>>().join(" "));
    }
    s
}

/// The licence and the endorsements spelt out (a tooltip).
fn endorsements_tip(l: Licence, e: &[Endorsement]) -> String {
    let mut v = vec![omsi_ui::tr(if l == Licence::D1 { "D1: only midibuses" } else { "D: every bus" }).into_owned()];
    v.extend(e.iter().map(|x| format!("{}: {}", x.short(), omsi_ui::tr(x.label()))));
    v.join("\n")
}

fn licence_text(l: Licence) -> &'static str {
    match l {
        Licence::D => "D",
        Licence::D1 => "D1",
    }
}

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let labels: Vec<String> = TABS.iter().enumerate().map(|(k, t)| if k == 0 { format!("{} ({})", omsi_ui::tr(t), c.staff.len()) } else { omsi_ui::tr(t).into_owned() }).collect();
    // (the easy mode: the employees and the applicants - the courses, the hours and the
    // automatic hiring are the company's own there; their tabs are kept for a way that leads
    // to them and for the advanced mode)
    let shown = shown_tabs(co::autopilot::on(&c), l.company.people.tab);
    let refs: Vec<&str> = labels.iter().take(shown).map(String::as_str).collect();
    let tabs_w = if shown == TABS.len() { 900.0f32 } else { 420.0 }.min(area.w);
    let mut tab = l.company.people.tab.min(TABS.len() - 1);
    if l.ui.segmented("company-staff-tabs", Rect::new(area.x, area.y, tabs_w, 40.0), &mut tab, &refs) {
        l.company.people.tab = tab;
        l.company.people.open = None;
    }
    // what the staff costs and what today asks
    if area.w - tabs_w > 320.0 {
        let wages: i64 = c.staff.iter().map(staff::monthly_cost).sum();
        let duties: usize = l.company.plan.as_ref().map(|p| p.tours.iter().filter(|t| !t.by_player && !t.live && !t.tour.unplanned).map(|t| t.duties.len()).sum()).unwrap_or(0);
        let info = omsi_ui::tr("Wages %{amount} a month with the employer's share  ·  %{n} duties today").replace("%{amount}", &eur(wages)).replace("%{n}", &duties.to_string());
        l.ui.text_in(&info, Rect::new(area.x + tabs_w + 20.0, area.y, area.w - tabs_w - 20.0, 40.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Right);
    }
    let body = Rect::new(area.x, area.y + 58.0, area.w, (area.h - 58.0).max(0.0));
    match l.company.people.tab {
        1 => hire::draw(l, body, &c),
        2 => {
            training(l, Rect::new(body.x, body.y, body.w, (body.h - 44.0).max(0.0)), &c);
            let foot = Rect::new(area.x, area.bottom() - 30.0, area.w, 30.0);
            l.ui.text_in("A course begins today and the driver is away for its days. New drivers learn the fleet's models when they start; a model new to the company needs its type training first.", foot, kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
        }
        3 => hours::schedule(l, body, &c),
        4 => hours::settings(l, body, &c),
        _ => list::draw(l, body, &c),
    }
}

/// How many of the Staff page's tabs show: all of them, or in the easy mode the first two -
/// unless the tab open is one of the others (a way led there).
fn shown_tabs(easy: bool, tab: usize) -> usize {
    if easy && tab < 2 {
        2
    } else {
        TABS.len()
    }
}

/// A column of the training matrix: a licence or endorsement, a model's type training, or one
/// of the staff's courses (`training::CourseKind::STAFF`).
#[derive(Clone, PartialEq)]
enum Col {
    Licence(CourseKind),
    Type(String),
    Course(CourseKind),
}

impl Col {
    fn kind(&self) -> CourseKind {
        match self {
            Col::Licence(k) | Col::Course(k) => *k,
            Col::Type(_) => CourseKind::TypeTraining,
        }
    }

    fn subject(&self) -> &str {
        match self {
            Col::Type(t) => t,
            _ => "",
        }
    }

    fn title(&self, c: &Company) -> String {
        match self {
            Col::Type(t) => licences::type_name(c, t),
            _ => omsi_ui::tr(self.kind().label()).into_owned(),
        }
    }

    fn cost(&self, c: &Company) -> co::Cents {
        match self {
            Col::Course(k) => training::cost_of(c, *k),
            _ => licences::course_cost(c, self.kind()),
        }
    }

    fn has(&self, c: &Company, e: &Employee) -> bool {
        match self {
            Col::Course(k) => training::trained(c, Some(e.id), *k),
            _ => licences::has(e, self.kind(), self.subject()),
        }
    }

    fn booked<'a>(&self, c: &'a Company, e: &Employee) -> Option<&'a training::Course> {
        match self {
            Col::Course(k) => training::course_of(c, Some(e.id), *k).filter(|x| !x.done),
            _ => licences::booked(c, e.id, self.kind(), self.subject()),
        }
    }
}

/// The columns of a group of the matrix: 0 the licences, 1 the fleet's models, 2 the courses.
fn columns(c: &Company, group: usize) -> Vec<Col> {
    match group {
        0 => CourseKind::LICENCES.iter().map(|k| Col::Licence(*k)).collect(),
        1 => licences::fleet_types(c).into_iter().map(Col::Type).collect(),
        _ => CourseKind::STAFF.iter().map(|k| Col::Course(*k)).collect(),
    }
}

/// A course's cost and days.
fn course_terms(cost: co::Cents, k: CourseKind) -> String {
    let days = k.spec().days;
    let d = if days == 1 { omsi_ui::tr("1 day").into_owned() } else { omsi_ui::tr("%{n} days").replace("%{n}", &days.to_string()) };
    format!("{}  ·  {}", eur(cost), d)
}

/// The licences, the type trainings and the courses: a row for each driver, a column for
/// each licence, each model of the fleet or each course; booked per driver, or for all who
/// lack it.
fn training(l: &mut Launcher, area: Rect, c: &Company) {
    if !c.staff.iter().any(staff::drives) {
        l.ui.paragraph("Nobody works here yet. Every duty of a tour needs a driver: hire some among the week's applicants.", Vec2::new(area.x, area.y + 4.0), area.w.min(860.0), kit::BODY, Weight::Regular, TEXT_SOFT);
        return;
    }
    let groups = [omsi_ui::tr("Licences").into_owned(), omsi_ui::tr("Bus types").into_owned(), omsi_ui::tr("Courses").into_owned()];
    let refs: Vec<&str> = groups.iter().map(String::as_str).collect();
    let mut group = l.company.people.group;
    if l.ui.segmented("company-training-groups", Rect::new(area.x, area.y, 420.0f32.min(area.w), 34.0), &mut group, &refs) {
        l.company.people.group = group;
        l.company.people.first = 0;
    }
    let area = Rect::new(area.x, area.y + 50.0, area.w, (area.h - 50.0).max(0.0));
    let all = columns(c, l.company.people.group);
    if all.is_empty() {
        l.ui.text_in("The fleet has no buses yet.", Rect::new(area.x, area.y, area.w, 24.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Left);
        return;
    }
    let name_w = (area.w * 0.2).clamp(170.0, 240.0);
    let fit = (((area.w - name_w - 12.0) / 150.0).floor() as usize).max(1);
    let per = fit.min(all.len());
    let first = l.company.people.first.min(all.len() - per);
    let shown: Vec<Col> = all[first..first + per].to_vec();
    let col_w = (area.w - name_w - 12.0) / per as f32;
    // (the drivers who stay: licences, bus types and the drivers' courses are theirs)
    let people: Vec<Employee> = c.staff.iter().filter(|e| e.notice_until.is_none() && staff::drives(e)).cloned().collect();
    let mut book: Option<(Col, Option<u32>)> = None;
    let mut lock: Option<CourseKind> = None;
    // the heads: the course, what it costs, how many have it, and "Train all"
    let head_h = 100.0;
    if all.len() > per {
        let t = omsi_ui::tr("%{a}-%{b} of %{n}").replace("%{a}", &(first + 1).to_string()).replace("%{b}", &(first + per).to_string()).replace("%{n}", &all.len().to_string());
        l.ui.text_in(&t, Rect::new(area.x + 4.0, area.y + 60.0, name_w - 90.0, 24.0), kit::NOTE, Weight::Medium, TEXT_DIM, Align::Left);
        if first > 0 && l.ui.icon_button("company-training-prev", Vec2::new(area.x + name_w - 64.0, area.y + 72.0), 15.0, "chevron_left", "Earlier columns") {
            l.company.people.first = first.saturating_sub(per);
        }
        if first + per < all.len() && l.ui.icon_button("company-training-next", Vec2::new(area.x + name_w - 28.0, area.y + 72.0), 15.0, "chevron_right", "More columns") {
            l.company.people.first = (first + per).min(all.len() - per);
        }
    }
    kit::caps(&mut l.ui, Rect::new(area.x + 4.0, area.y, name_w - 12.0, 16.0), "Driver");
    let g = l.company.people.group;
    for (k, col) in shown.iter().enumerate() {
        let kind = col.kind();
        let x = area.x + name_w + k as f32 * col_w;
        let open = co::levels::unlocked(c, kind.spec().feature);
        let icon = if matches!(col, Col::Type(_)) { "directions_bus" } else { kind.icon() };
        l.ui.icon(icon, Vec2::new(x + 9.0, area.y + 9.0), 15.0, if open { accent_2() } else { TEXT_FAINT });
        l.ui.text_in(&col.title(c), Rect::new(x + 22.0, area.y, col_w - 28.0, 18.0), 14.0, Weight::Bold, TEXT, Align::Left);
        let terms = course_terms(col.cost(c), kind);
        let under = if matches!(col, Col::Type(_)) { format!("{}  ·  {terms}", omsi_ui::tr("Type training")) } else { terms };
        l.ui.text_in(&under, Rect::new(x + 4.0, area.y + 20.0, col_w - 10.0, 16.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
        let have = people.iter().filter(|e| col.has(c, e)).count();
        let count = omsi_ui::tr("%{n} of %{m} have it").replace("%{n}", &have.to_string()).replace("%{m}", &people.len().to_string());
        l.ui.text_in(&count, Rect::new(x + 4.0, area.y + 38.0, col_w - 10.0, 16.0), kit::NOTE, Weight::Medium, if have == people.len() { OK } else { TEXT_SOFT }, Align::Left);
        l.ui.tooltip(Rect::new(x, area.y, col_w, 56.0), &omsi_ui::tr(kind.effect()));
        let b = Rect::new(x + 2.0, area.y + 62.0, (col_w - 12.0).min(140.0), 28.0);
        if !open {
            let t = omsi_ui::tr("From level %{n}").replace("%{n}", &kind.spec().feature.level().to_string());
            if l.ui.button(&format!("company-train-lock-{g}-{k}-{first}"), b, &t, Some("lock"), ButtonKind::Ghost) {
                lock = Some(kind);
            }
        } else if have < people.len() && l.ui.button(&format!("company-train-all-{g}-{k}-{first}"), b, "Train all", Some("groups"), ButtonKind::Normal) {
            book = Some((col.clone(), None));
        }
    }
    l.ui.p().rect(Rect::new(area.x, area.y + head_h, area.w, 1.0), HAIRLINE);
    let rows = Rect::new(area.x, area.y + head_h + 6.0, area.w, (area.h - head_h - 6.0).max(0.0));
    let cc = c.clone();
    l.ui.scroll_area("company-licences", rows, &mut |ui, v| {
        let rh = 40.0;
        for (i, e) in people.iter().enumerate() {
            let row = Rect::new(v.x, v.y + i as f32 * rh, v.w - 12.0, rh - 2.0);
            if !ui.rect_visible(row) {
                continue;
            }
            ui.text_in(&e.name, Rect::new(row.x + 4.0, row.y, name_w - 70.0, row.h), kit::ROWS, Weight::Bold, TEXT, Align::Left);
            ui.text_in(&licence_line(e.licence, &e.endorsements), Rect::new(row.x + name_w - 66.0, row.y, 60.0, row.h), kit::NOTE, Weight::Medium, TEXT_SOFT, Align::Left);
            let away = e.absent(&cc.date);
            for (k, col) in shown.iter().enumerate() {
                let kind = col.kind();
                let cell = Rect::new(row.x + name_w + k as f32 * col_w, row.y + 4.0, col_w - 10.0, row.h - 8.0);
                if col.has(&cc, e) {
                    ui.icon("check_circle", Vec2::new(cell.x + 10.0, cell.center().y), 15.0, OK);
                    continue;
                }
                match col.booked(&cc, e) {
                    Some(x) => {
                        ui.icon("schedule", Vec2::new(cell.x + 10.0, cell.center().y), 15.0, accent_2());
                        let t = omsi_ui::tr("Until %{date}").replace("%{date}", &day_label(&x.until));
                        ui.text_in(&t, Rect::new(cell.x + 24.0, cell.y, cell.w - 24.0, cell.h), kit::NOTE, Weight::Medium, accent_2(), Align::Left);
                    }
                    None if !co::levels::unlocked(&cc, kind.spec().feature) => {
                        ui.icon("lock", Vec2::new(cell.x + 10.0, cell.center().y), 14.0, TEXT_FAINT);
                    }
                    None if e.licence == Licence::D1 && matches!(kind, CourseKind::ArticulatedLicence | CourseKind::DoubleDeckerLicence) => {
                        ui.text_in("D licence first", Rect::new(cell.x + 4.0, cell.y, cell.w, cell.h), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
                    }
                    None if away => {
                        ui.text_in("Away", Rect::new(cell.x + 4.0, cell.y, cell.w, cell.h), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
                    }
                    None => {
                        if ui.button(&format!("company-train-{}-{g}-{k}-{first}", e.id), Rect::new(cell.x, cell.y, cell.w.min(110.0), cell.h), "Book", Some("add"), ButtonKind::Ghost) {
                            book = Some((col.clone(), Some(e.id)));
                        }
                    }
                }
            }
            ui.p().rect(Rect::new(row.x, row.bottom(), row.w, 1.0), HAIRLINE);
        }
        people.len() as f32 * rh
    });
    if let Some(kind) = lock {
        kit::show(l, kit::locked(c, kind.spec().feature));
    }
    let Some((col, who)) = book else { return };
    let kind = col.kind();
    let course = if matches!(col, Col::Type(_)) { format!("{} {}", omsi_ui::tr("Type training"), col.title(c)) } else { col.title(c) };
    let subject = col.subject().to_string();
    let staff_course = matches!(col, Col::Course(_));
    match who {
        Some(id) => {
            let done = if staff_course { act(l, |c| training::enrol(c, kind, Some(id))) } else { act(l, |c| licences::enrol(c, id, kind, &subject)) };
            if let Some(until) = done {
                l.state.set_status(omsi_ui::tr("Booked: %{course}, until %{date}.").replace("%{course}", &course).replace("%{date}", &day_label(&until)), false);
            }
        }
        None => {
            let cost = col.cost(c);
            if c.cash < cost {
                kit::show(l, kit::no_cash(c, cost));
                return;
            }
            let done = if staff_course { act(l, |c| Ok(training::enrol_all(c, kind))) } else { act(l, |c| Ok(licences::enrol_all(c, kind, &subject))) };
            let Some((n, spent)) = done else { return };
            if n == 0 {
                kit::show(l, kit::Popup::new("groups", "Nobody could be booked", omsi_ui::tr("Who lacks it is away today, on another course or leaving the company - or needs the D licence first."), "", None));
            } else {
                let t = omsi_ui::tr("Booked for %{n} drivers: %{course}, %{amount} together.").replace("%{n}", &n.to_string()).replace("%{course}", &course).replace("%{amount}", &eur(spent));
                l.state.set_status(t, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::dealer::Extra;
    use omsi_launcher_lib::company::plan::Problem;

    #[test]
    fn the_licences_are_translated() {
        let mut keys: Vec<&str> = Endorsement::ALL.iter().flat_map(|e| [e.label(), e.lacking()]).collect();
        let courses = CourseKind::LICENCES.iter().chain([CourseKind::TypeTraining].iter());
        keys.extend(courses.flat_map(|k| [k.label(), k.effect()]));
        keys.extend([Problem::NoEndorsement.label(), Problem::NoTypeTraining.label(), Extra::Introduction.label(), "They need the D licence first.", "Which bus is the training for?"]);
        // (the staff's courses, and what opens the defensive driving)
        keys.extend(CourseKind::STAFF.iter().flat_map(|k| [k.label(), k.effect()]));
        keys.extend([co::levels::Feature::SafetyCourse.label(), "Licences", "Bus types", "Courses", "Train the drivers", "Bus %{bus} had an accident yesterday: damage %{amount}"]);
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in &keys {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }
}
