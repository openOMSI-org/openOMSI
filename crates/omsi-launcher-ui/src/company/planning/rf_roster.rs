//! RosterFlow's Roster tab: the planning's weekly roster drawn as the rest of RosterFlow is
//! (Luc: "make these bars in the same style as the rest of RosterFlow"). Under the title bar
//! the next seven days as RosterFlow's tabs, each with whether it is covered; the toolbar with
//! what the roster repeats and the tools (repeat, clear the day, fill the roster); then the
//! day's tours as RosterFlow's Gantt - its axis and hour bands, its compact rows with the ID
//! column (the tour, its bus) - grouped by line under a heading that folds the line away and
//! opens its timetable. A tour's duties lie on the hours in the colour of who drives them;
//! what is open reads as RosterFlow's issues: an outlined chip in the warning colour with its
//! icon (red where a rule stands in the way), not a pale box. Beside the chart the day, a
//! duty or a tour, with the drivers and buses to give by tapping or dragging - as before.
//!
//! Everything is drawn in RosterFlow's own palette (`Pal`, dark or light) as the Vehicles and
//! Drivers views are; the roster's rules (`fit_of`, `problem_colour` ...) keep the launcher's
//! colours and `ink` takes them over.

use super::rf_gantt::{line_colour, AXIS};
use super::rf_scroll::{self, BAR};
use super::rosterflow::{caps, pal, rf_button, rf_icon, Pal};
use super::*;

/// The day tabs' band, the toolbar, a line's heading, the ID column (the tour and its bus),
/// the side panel's heading.
const DAY_H: f32 = 50.0;
const TOOL_H: f32 = 40.0;
const HEAD: f32 = 34.0;
const LEFT: f32 = 236.0;
const SIDE_HEAD: f32 = 44.0;

/// A colour of the launcher's theme (what the roster's rules give: `fit_of`, `bus_fit`,
/// `problem_colour`) as RosterFlow's palette has it.
fn ink(q: &Pal, c: Color) -> Color {
    if c == WARN {
        q.warn
    } else if c == DANGER || c == DANGER.lighten(0.2) {
        q.err
    } else if c == OK {
        q.ok
    } else if c == TEXT {
        q.text
    } else if c == TEXT_SOFT {
        q.text.alpha(0.82)
    } else if c == TEXT_DIM {
        q.dim
    } else if c == TEXT_FAINT {
        q.faint
    } else if c == accent() {
        q.accent
    } else {
        c
    }
}

/// What is open, as RosterFlow's issues: an outlined chip in the warning (or error) colour
/// with its icon, its words as the room allows.
fn issue_chip(ui: &mut Ui, r: Rect, col: Color, icon: &str, words: &str, hover: bool) {
    ui.p().rounded(r, 3.0, col.alpha(if hover { 0.22 } else { 0.13 }));
    ui.p().rounded_border(r, 3.0, 1.2, col);
    let ih = (r.h * 0.62).clamp(11.0, 14.0);
    if r.w >= ih + 8.0 {
        ui.icon(icon, Vec2::new(r.x + 4.0 + ih * 0.5, r.center().y), ih, col);
    }
    let tx = r.x + ih + 9.0;
    if r.right() - tx > 16.0 && !words.is_empty() {
        ui.text_in(words, Rect::new(tx, r.y, r.right() - tx - 5.0, r.h), 11.5, Weight::Bold, col, Align::Left);
    }
}

/// The width an issue's tag takes (`issue_chip` with its words).
fn tag_w(ui: &Ui, words: &str) -> f32 {
    ui.width(words, 11.5, Weight::Bold) + 32.0
}

/// A line's plate: its number on its colour.
fn plate(ui: &mut Ui, c: &Company, line: &str, number: &str, x: f32, cy: f32) -> f32 {
    let colour = line_colour(c, line);
    let w = (ui.width(number, 12.0, Weight::Bold) + 16.0).max(30.0);
    let r = Rect::new(x, cy - 10.0, w, 20.0);
    ui.p().rounded(r, 3.0, colour);
    ui.text_as_is(number, Vec2::new(r.center().x, cy + 4.5), 12.0, Weight::Bold, super::rf_gantt::ink_on(colour), Align::Center);
    w
}

/// A quiet button with an icon (the line's timetable from a duty or a tour).
fn link_button(ui: &mut Ui, q: &Pal, name: &str, r: Rect, icon: &str, label: &str) -> bool {
    let (h, _, clicked) = ui.interact(id_of(name), r);
    ui.p().rounded(r, 3.0, if h { q.accent.alpha(0.12) } else { q.text.alpha(0.03) });
    ui.p().rounded_border(r, 3.0, 1.0, if h { q.accent } else { q.text.alpha(0.16) });
    ui.icon(icon, Vec2::new(r.x + 18.0, r.center().y), 16.0, q.accent);
    ui.text_in(label, Rect::new(r.x + 34.0, r.y, r.w - 42.0, r.h), 13.0, Weight::Medium, if h { q.text } else { q.accent }, Align::Left);
    clicked
}

// --- the page ------------------------------------------------------------------------------------

/// The Roster tab in `area` (under RosterFlow's title bar).
pub(super) fn page(l: &mut Launcher, area: Rect, c: &Company, date: &str) {
    let q = pal(&l.ui);
    l.ui.p().rect(area, q.bg);
    let day_r = Rect::new(area.x, area.y, area.w, DAY_H);
    super::super::super::tour::anchor("rf-roster-week", day_r);
    day_tabs(l, day_r, c, &q);
    let tools_r = Rect::new(area.x, day_r.bottom(), area.w, TOOL_H);
    super::super::super::tour::anchor("rf-roster-tools", tools_r);
    tools(l, tools_r, c, date, &q);
    let body = Rect::new(area.x, tools_r.bottom(), area.w, (area.bottom() - tools_r.bottom()).max(0.0));
    let plan = plan_of(l, c, date);
    let side_w = (body.w * 0.27).clamp(300.0, 420.0);
    let chart = Rect::new(body.x, body.y, (body.w - side_w).max(200.0), body.h);
    let side = Rect::new(chart.right(), body.y, side_w, body.h);
    super::super::super::tour::anchor("rf-roster-chart", chart);
    if matches!(plan, Some(Ok(_))) {
        super::super::super::tour::anchor("rf-roster-side", side);
    }
    match plan {
        None => {
            l.ui.p().rect(body, q.grid);
            l.ui.text_in("Reading the timetable…", body.pad(24.0, 0.0), 13.0, Weight::Medium, q.dim, Align::Center);
        }
        Some(Err(e)) => {
            l.ui.p().rect(body, q.grid);
            l.ui.paragraph(&e, Vec2::new(body.x + 24.0, body.y + 24.0), (body.w - 48.0).max(0.0), 13.0, Weight::Regular, q.err);
        }
        Some(Ok(p)) => {
            let hits = gantt(l, chart, c, &p, &q);
            // (pressed on a duty or a tour's bus in the chart: taken up once the mouse moves)
            let m = l.ui.input.mouse;
            if l.ui.input.pressed && l.company.planning.drag.is_none() && l.company.planning.carry.is_none() {
                if let Some(h) = hits.iter().find(|h| h.r.contains(m)) {
                    l.company.planning.press = Some((m, h.tour, h.duty));
                }
            }
            if let Some((at, ti, duty)) = l.company.planning.press {
                if !l.ui.input.down {
                    l.company.planning.press = None;
                } else if (m - at).length() > 6.0 {
                    l.company.planning.press = None;
                    l.company.planning.carry = Some((ti, duty));
                    l.company.planning.carry_from = at;
                    // (the list to drop it on: the day's, not a duty's own)
                    l.company.planning.sel = None;
                }
            }
            l.company.planning.targets.clear();
            side_panel(l, side, c, &p, &q);
            // a duty or a tour carried to a driver or a bus of the list and let go
            if let Some((ti, duty)) = l.company.planning.carry {
                if l.ui.input.released {
                    l.company.planning.carry = None;
                    let target = l.company.planning.targets.iter().find(|t| t.0.contains(m)).map(|t| t.1);
                    match (target, duty) {
                        (Some(Arm::Driver(w)), Some(k)) => give_driver(l, &p, ti, k, Some(w)),
                        (Some(Arm::Bus(id)), _) => give_bus(l, &p, ti, Some(id)),
                        _ => {}
                    }
                } else if l.ui.input.down {
                    if (m - l.company.planning.carry_from).length() > 6.0 {
                        carried(&mut l.ui, &q, &p, ti, duty);
                    }
                } else {
                    l.company.planning.carry = None;
                }
            }
            // a driver or a bus dragged here and let go
            if l.ui.input.released {
                if let Some(arm) = l.company.planning.drag.take() {
                    let at = l.ui.input.mouse;
                    if let Some(h) = hits.iter().find(|h| h.r.contains(at)) {
                        give(l, &p, arm, h);
                    }
                }
            } else if l.ui.input.down {
                if let Some(arm) = l.company.planning.drag {
                    if !side.contains(l.ui.input.mouse) {
                        dragged(&mut l.ui, &q, c, arm);
                    }
                }
            } else {
                l.company.planning.drag = None;
            }
        }
    }
}

/// The duty or tour being carried, at the mouse: a chip in RosterFlow's blue.
fn carried(ui: &mut Ui, q: &Pal, p: &DayPlan, ti: usize, duty: Option<usize>) {
    let Some(t) = p.tours.get(ti) else { return };
    let text = line_title(t, duty);
    let m = ui.input.mouse;
    let w = (ui.width(&text, 12.5, Weight::Bold) + 22.0).min(360.0);
    let r = Rect::new(m.x + 10.0, m.y + 6.0, w, 24.0);
    ui.p().shadow(r, 4.0, 10.0, Color::rgba(0, 0, 0, 0.4));
    ui.p().rounded(r, 3.0, q.accent);
    ui.text_in(&text, r, 12.5, Weight::Bold, Color::WHITE, Align::Center);
}

/// The driver or bus being dragged, at the mouse.
fn dragged(ui: &mut Ui, q: &Pal, c: &Company, arm: Arm) {
    let (text, fill, ink_c) = match arm {
        Arm::Driver(Who::Staff(id)) => (first_name(c, id), driver_colour(c, id), INK),
        Arm::Driver(Who::Player) => (omsi_ui::tr("You").into_owned(), q.accent, Color::WHITE),
        Arm::Driver(Who::Agency) => (omsi_ui::tr("Agency").into_owned(), q.grey, Color::WHITE),
        Arm::Bus(id) => (c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default(), q.tool, q.text),
    };
    let m = ui.input.mouse;
    let w = ui.width(&text, 12.5, Weight::Bold) + 22.0;
    let r = Rect::new(m.x + 10.0, m.y + 6.0, w, 24.0);
    ui.p().shadow(r, 4.0, 10.0, Color::rgba(0, 0, 0, 0.4));
    ui.p().rounded(r, 3.0, fill);
    ui.text_in(&text, r, 12.5, Weight::Bold, ink_c, Align::Center);
}

/// The next seven days as RosterFlow's tabs: the weekday in small capitals, the date, a dot
/// for whether the day is covered; the day shown underlined in blue.
fn day_tabs(l: &mut Launcher, r: Rect, c: &Company, q: &Pal) {
    l.ui.p().rect(r, q.bar);
    l.ui.p().rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), q.line);
    let w = r.w / 7.0;
    for k in 0..7usize {
        let date = co::dates::add(&c.date, k as i64);
        let cell = Rect::new(r.x + k as f32 * w, r.y, w, r.h);
        let on = l.company.planning.day == k;
        let (h, _, click) = l.ui.interact(id_of(&format!("plan-day-{k}")), cell);
        if on {
            l.ui.p().rect(cell, q.accent.alpha(0.07));
            l.ui.p().rect(Rect::new(cell.x, cell.bottom() - 3.0, cell.w, 3.0), q.accent);
        } else if h {
            l.ui.p().rect(cell, q.text.alpha(0.04));
        }
        if k > 0 {
            l.ui.p().rect(Rect::new(cell.x, cell.y + 12.0, 1.0, cell.h - 24.0), q.line);
        }
        if click && !on {
            let v = &mut l.company.planning;
            v.day = k;
            v.sel = None;
            v.clear_armed = false;
        }
        let Some(d) = co::dates::parse(&date) else { continue };
        let (_, m, dd) = co::dates::civil_from_days(d);
        let wd = super::super::WEEKDAYS[co::dates::weekday(d) as usize];
        caps(&mut l.ui, Rect::new(cell.x + 16.0, cell.y + 8.0, cell.w - 40.0, 16.0), if k == 0 { "Company day" } else { wd }, if on { q.accent } else { q.dim }, Align::Left);
        let big = format!("{} {} {}", omsi_ui::tr(wd), dd, omsi_ui::tr(super::super::MONTHS[(m as usize).clamp(1, 12) - 1]));
        l.ui.text_in(&big, Rect::new(cell.x + 16.0, cell.y + 24.0, cell.w - 28.0, 20.0), 13.5, Weight::Bold, if on || h { q.text } else { q.text.alpha(0.86) }, Align::Left);
        let status = plan_of(l, c, &date).and_then(|p| p.ok()).map(|p| p.counts());
        if let Some((tours, _, open, no_bus)) = status {
            let col = if tours == 0 { q.faint } else if open + no_bus == 0 { q.ok } else { q.warn };
            l.ui.p().circle(Vec2::new(cell.right() - 16.0, cell.y + 15.0), 4.0, col);
            let tip = if tours == 0 { omsi_ui::tr("No tours this day").into_owned() } else if open + no_bus == 0 { omsi_ui::tr("Every tour planned").into_owned() } else { omsi_ui::tr("%{n} open").replace("%{n}", &(open + no_bus).to_string()) };
            l.ui.tooltip(cell, &tip);
        }
    }
}

/// The toolbar: what the roster is (or what the fleet lacks), and the tools - repeat, clear,
/// fill - as RosterFlow's buttons.
fn tools(l: &mut Launcher, r: Rect, c: &Company, date: &str, q: &Pal) {
    l.ui.p().rect(r, q.tool);
    let wd = pl::weekday_of(date);
    let bw = |ui: &Ui, label: &str| ui.width(&omsi_ui::tr(label).to_uppercase(), 12.5, Weight::Bold) + 36.0;
    let mut x = r.right() - 4.0;
    let fw = bw(&l.ui, "Fill the roster");
    x -= fw;
    let fill_r = Rect::new(x, r.y + 4.0, fw, r.h - 8.0);
    let fill = rf_button(&mut l.ui, q, "plan-fill", fill_r, "Fill the roster", true, true);
    l.ui.tooltip(fill_r, "Fix in the roster what the dispatcher would take: free buses, and free drivers by the working-time rules. Only what is planned runs.");
    let clear_label = if l.company.planning.clear_armed { "Press again" } else { "Clear the day" };
    let cw = bw(&l.ui, "Clear the day").max(bw(&l.ui, "Press again"));
    x -= cw + 6.0;
    let clear_r = Rect::new(x, r.y + 4.0, cw, r.h - 8.0);
    let clear = rf_button(&mut l.ui, q, "plan-clear", clear_r, clear_label, false, true);
    if l.company.planning.clear_armed {
        l.ui.p().rounded_border(clear_r, 2.0, 1.5, q.err);
    }
    l.ui.tooltip(clear_r, "Take every bus and driver of this weekday out of the roster");
    let (repeat_label, to): (&str, Vec<u8>) = if wd < 5 { ("Repeat Mon–Fri", (0..5).collect()) } else { ("Repeat on the weekend", vec![5, 6]) };
    let rw = bw(&l.ui, repeat_label);
    x -= rw + 6.0;
    let repeat_r = Rect::new(x, r.y + 4.0, rw, r.h - 8.0);
    let repeat = rf_button(&mut l.ui, q, "plan-repeat", repeat_r, repeat_label, false, true);
    l.ui.tooltip(repeat_r, "This weekday's roster on the other days too (theirs is replaced)");
    // (a fleet without the buses the tours ask for at their busiest: said instead)
    let lack = l.company.planning.days.iter().find(|d| d.0 == c.map && d.1 == date).and_then(|d| d.2.as_ref().ok()).map(|lines| co::ownline::shortfall(c, &co::network::tours_of_day(c, lines, date))).unwrap_or_default();
    let (text, colour, icon) = match lack.first() {
        Some(s) => (
            omsi_ui::tr("At the busiest the tours ask for %{n} × %{bus} (or bigger); the fleet has %{have}.")
                .replace("%{n}", &s.needed.to_string())
                .replace("%{bus}", &omsi_ui::tr(co::BusKind { size: s.size, drive: co::Drive::Diesel }.label()))
                .replace("%{have}", &s.have.to_string()),
            q.warn,
            "warning",
        ),
        None => (omsi_ui::tr("%{day}: the roster of this weekday repeats every week.").replace("%{day}", &day_label(date)), q.text.alpha(0.86), "event"),
    };
    let cy = r.center().y;
    l.ui.icon(icon, Vec2::new(r.x + 18.0, cy), 16.0, colour);
    l.ui.text_in(&text, Rect::new(r.x + 34.0, r.y, (x - r.x - 46.0).max(0.0), r.h), 13.0, Weight::Medium, colour, Align::Left);
    l.ui.tooltip(Rect::new(r.x, r.y, (x - r.x - 12.0).max(0.0), r.h), &text);
    // (each tool says what it did - or, when it could do nothing, why, in a popup)
    let day = weekday_name(wd);
    if fill {
        if let Some(lines) = l.company.planning.days.iter().find(|d| d.0 == c.map && d.1 == date).and_then(|d| d.2.as_ref().ok()).cloned() {
            let d = date.to_string();
            if let Some(f) = act(l, |c| {
                let tours = co::network::tours_of_day(c, &lines, &d);
                Ok(pl::fill_day_told(c, &d, tours))
            }) {
                let open = open_text(&f);
                if f.duties + f.buses > 0 {
                    let done = omsi_ui::tr("Filled: %{d} duties given to drivers, %{b} buses given.").replace("%{d}", &f.duties.to_string()).replace("%{b}", &f.buses.to_string());
                    l.state.set_status(if open.is_empty() { done } else { format!("{done} {open}") }, false);
                } else if open.is_empty() {
                    let text = omsi_ui::tr("Every tour of %{day} has its bus and its drivers already.").replace("%{day}", &day);
                    kit::show(l, kit::Popup::new("event", "Nothing to fill", text, "", None));
                } else {
                    let go = if f.unqualified > 0 { kit::Go::Licences } else if f.busy > 0 { kit::Go::Hire } else { kit::Go::Dealer };
                    let unlock = omsi_ui::tr("Hire drivers, train them for the buses on the Staff page, or get more buses.");
                    kit::show(l, kit::Popup::new("event", "Nothing could be filled", open, unlock, Some(go)));
                }
            }
        }
    }
    if clear {
        if l.company.planning.clear_armed {
            l.company.planning.clear_armed = false;
            if let Some(gone) = act(l, |c| Ok(pl::clear_day(c, wd))) {
                if gone.is_empty() {
                    let text = omsi_ui::tr("The roster of %{day} is empty already.").replace("%{day}", &day);
                    kit::show(l, kit::Popup::new("delete", "Nothing to clear", text, "", None));
                } else {
                    let t = omsi_ui::tr("The day is cleared: %{d} duties and %{b} buses taken off the roster of %{day}.").replace("%{d}", &gone.duties.to_string()).replace("%{b}", &gone.buses.to_string()).replace("%{day}", &day);
                    l.state.set_status(t, false);
                }
            }
        } else {
            l.company.planning.clear_armed = true;
        }
    }
    if repeat {
        if let Some(days) = act(l, |c| Ok(pl::copy_day(c, wd, &to))) {
            if days.is_empty() {
                let text = omsi_ui::tr("The roster of %{day} is empty: there is nothing to repeat. Fill it first (\"Fill the roster\" does it for you), then repeat it on the other days.").replace("%{day}", &day);
                kit::show(l, kit::Popup::new("content_copy", "Nothing to repeat", text, "", None));
            } else {
                let key = if days.len() == 1 { "The roster of %{day} is now on %{days} too." } else { "The roster of %{day} is now on %{days} too (%{n} days)." };
                let t = omsi_ui::tr(key).replace("%{day}", &day).replace("%{days}", &weekdays_text(&days)).replace("%{n}", &days.len().to_string());
                l.state.set_status(t, false);
            }
        }
    }
}

// --- the chart -----------------------------------------------------------------------------------

/// The day's tours as RosterFlow's Gantt, grouped by line - each line's heading folds it away
/// and back, and opens its timetable - the tour and its bus in the ID column, its duties on
/// the hours. Returns where the duties and buses are.
fn gantt(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan, q: &Pal) -> Vec<Hit> {
    l.ui.p().rect(r, q.grid);
    if p.tours.is_empty() && c.lines.is_empty() {
        l.ui.paragraph("No tours on this day: the company runs no line yet, or its lines do not run on this day.", Vec2::new(r.x + 24.0, r.y + 24.0), (r.w - 48.0).max(0.0), 13.0, Weight::Regular, q.dim);
        return Vec::new();
    }
    let t0 = p.tours.iter().map(|t| t.tour.from()).min().unwrap_or(300).div_euclid(60) * 60;
    let t1 = (p.tours.iter().map(|t| t.tour.to()).max().unwrap_or(1440) + 59).div_euclid(60) * 60;
    let span = (t1 - t0).max(60) as f32;
    let gx = r.x + LEFT + 8.0;
    let gw = (r.w - LEFT - 8.0 - BAR - 8.0).max(60.0);
    let ppm = gw / span;
    let x_of = move |m: i32| gx + (m - t0) as f32 * ppm;
    let hours: Vec<i32> = (t0 / 60..t1 / 60).collect();
    // the hour bands, every other one lighter, under everything; the ID column
    l.ui.p().rect(Rect::new(r.x, r.y + AXIS, LEFT, r.h - AXIS), q.labels);
    l.ui.push_clip(Rect::new(gx, r.y, gw, r.h), 0.0);
    for &h in &hours {
        if h % 2 == 1 {
            let (a, b) = (x_of(h * 60), x_of(h * 60 + 60));
            l.ui.p().rect(Rect::new(a, r.y + AXIS, b - a, r.h - AXIS), q.band);
        }
    }
    l.ui.pop_clip();
    // the axis: the hours ("05:00")
    l.ui.p().rect(Rect::new(r.x, r.y, r.w, AXIS), q.axis);
    l.ui.p().rect(Rect::new(r.x, r.y + AXIS - 1.0, r.w, 1.0), q.line);
    let every = if ppm * 60.0 >= 50.0 { 1 } else if ppm * 60.0 >= 25.0 { 2 } else { 3 };
    l.ui.push_clip(Rect::new(gx - 24.0, r.y, gw + 30.0, AXIS), 0.0);
    for h in (t0 / 60..=t1 / 60).filter(|h| h % every == 0) {
        l.ui.text_as_is(&format!("{:02}:00", h.rem_euclid(24)), Vec2::new(x_of(h * 60), r.y + AXIS * 0.5 + 4.0), 11.0, Weight::Medium, q.dim, Align::Center);
    }
    l.ui.pop_clip();
    // the lines in the company's order, each with its tours (folded: its heading alone; a line
    // without tours this day its heading too, to open its timetable - a Sunday given trips)
    let mut groups: Vec<(Option<co::CompanyLine>, Vec<usize>)> = c.lines.iter().map(|x| (Some(x.clone()), Vec::new())).collect();
    for (i, t) in p.tours.iter().enumerate() {
        match groups.iter_mut().find(|g| g.0.as_ref().is_some_and(|x| x.name.eq_ignore_ascii_case(&t.tour.line))) {
            Some(g) => g.1.push(i),
            None => groups.push((None, vec![i])),
        }
    }
    let names: Vec<String> = groups.iter().map(|g| g.0.as_ref().map(|cl| cl.name.clone()).unwrap_or_else(|| p.tours[g.1[0]].tour.line.clone())).collect();
    let mut folded: Vec<bool> = names.iter().map(|n| pl::is_collapsed(c, n)).collect();
    // (a line to show, from the Lines page: unfolded)
    let focus = l.company.planning.focus.take();
    let mut unfold: Option<String> = None;
    if let Some(k) = focus.as_ref().and_then(|f| names.iter().position(|n| n.eq_ignore_ascii_case(f))) {
        if folded[k] {
            folded[k] = false;
            unfold = Some(names[k].clone());
        }
    }
    // over the ID column in the axis: what is in hand, or "Collapse all" / "Expand all"
    let armed = l.company.planning.arm;
    let mut fold_all: Option<bool> = None;
    let head_r = Rect::new(r.x, r.y, LEFT, AXIS);
    if let Some(a) = armed {
        let who = match a {
            Arm::Driver(w) => who_name(c, Some(w)),
            Arm::Bus(id) => c.vehicle(id).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default(),
        };
        let hint = match a {
            Arm::Driver(_) => omsi_ui::tr("Tap a duty to give it to %{who} (Esc: stop)."),
            Arm::Bus(_) => omsi_ui::tr("Tap a tour's bus to give it %{who} (Esc: stop)."),
        }
        .replace("%{who}", &who);
        l.ui.text_in(&hint, head_r.pad(12.0, 0.0), 11.5, Weight::Bold, q.accent, Align::Left);
        l.ui.tooltip(head_r, &hint);
    } else if groups.iter().any(|g| !g.1.is_empty()) {
        // every line folded, or every one open
        let all = groups.iter().zip(&folded).filter(|(g, _)| !g.1.is_empty()).all(|(_, f)| *f);
        let (label, icon) = if all { ("Expand all", "unfold_more") } else { ("Collapse all", "unfold_less") };
        let w = ui_w(&l.ui, label) + 34.0;
        let b = Rect::new(r.x + 6.0, r.y + 3.0, w, AXIS - 6.0);
        let (h, _, click) = l.ui.interact(id_of("plan-fold-all"), b);
        let col = if h { q.text } else { q.dim };
        l.ui.icon(icon, Vec2::new(b.x + 12.0, b.center().y), 15.0, col);
        caps(&mut l.ui, Rect::new(b.x + 24.0, b.y, b.w - 24.0, b.h), label, col, Align::Left);
        l.ui.tooltip(b, if all { "Show every line's tours" } else { "Fold every line to its heading: what it has planned, and what is open" });
        if click {
            fold_all = Some(!all);
        }
    }
    let rows = Rect::new(r.x, r.y + AXIS, r.w, (r.h - AXIS).max(0.0));
    // the rows' height: the room left by the headings, shared by the tours shown
    let shown: usize = groups.iter().zip(&folded).filter(|(_, f)| !**f).map(|(g, _)| g.1.len()).sum();
    let room = rows.h - 8.0 - groups.len() as f32 * HEAD;
    let pitch = row_pitch(room, shown);
    let sel = l.company.planning.sel.clone();
    let mut hits: Vec<Hit> = Vec::new();
    let mut clicked: Option<(usize, Option<usize>)> = None;
    let mut start: Option<String> = None;
    let mut stop: Option<String> = None;
    let mut toggle: Option<String> = None;
    let mut timetable: Option<(String, Option<String>)> = None;
    let mut focus_y: Option<f32> = None;
    let now = l.ui.time;
    let mut taps: Vec<(String, usize)> = Vec::new();
    let look = rf_scroll::look(q);
    let q = *q;
    rf_scroll::area(&mut l.ui, &look, "plan-gantt", rows, &mut |ui, v| {
        let mut y = v.y;
        let mouse = ui.input.mouse;
        for (gi, (line, idx)) in groups.iter().enumerate() {
            let name = &names[gi];
            let hr = Rect::new(v.x, y, v.w - BAR, HEAD);
            if focus.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(name)) {
                focus_y = Some(y - v.y);
            }
            let none = idx.is_empty();
            let idle = !none && idx.iter().all(|&i| p.tours[i].tour.unplanned);
            if ui.rect_visible(hr) {
                line_heading(ui, &q, c, p, hr, line.as_ref(), name, idx, folded[gi] || none, none, idle, &mut toggle, &mut timetable, &mut start, &mut stop);
            }
            y += HEAD;
            if folded[gi] {
                continue;
            }
            for &ti in idx {
                let t = &p.tours[ti];
                let row = Rect::new(v.x, y, v.w - BAR, pitch);
                y += pitch;
                if !ui.rect_visible(row) {
                    continue;
                }
                let cy = row.center().y;
                let tour_on = sel == Some(Sel::Tour(t.tour.line.clone(), t.tour.tour.clone()));
                let row_hover = row.contains(mouse) && rows.contains(mouse);
                if t.tour.unplanned {
                    // (a line not in service: planned here, not run)
                    ui.p().rect(Rect::new(gx - 6.0, row.y, gw + 12.0, row.h), q.text.alpha(0.025));
                }
                if tour_on || row_hover {
                    ui.p().rect(Rect::new(gx - 6.0, row.y, gw + 12.0, row.h), q.text.alpha(if tour_on { 0.06 } else { 0.03 }));
                }
                // the ID column: the line's colour, the tour, its bus
                ui.swatch(Rect::new(row.x + 14.0, cy - 6.0, 3.0, 12.0), 1.0, line_colour(c, &t.tour.line));
                let id_col = if t.tour.unplanned { q.dim } else if tour_on { q.accent } else { q.text.alpha(0.9) };
                ui.text_in(&format!("{} {}", omsi_ui::tr("Tour"), t.tour.tour), Rect::new(row.x + 22.0, row.y, 76.0, row.h), 12.5, Weight::Bold, id_col, Align::Left);
                let chip_h = (pitch - 8.0).clamp(18.0, 24.0);
                let chip = Rect::new(row.x + 100.0, cy - chip_h * 0.5, LEFT - 100.0 - 10.0, chip_h);
                let (h, _, click) = ui.interact(id_of(&format!("plan-bus-{ti}")), chip);
                if click {
                    clicked = Some((ti, None));
                    taps.push((format!("bus-{ti}"), ti));
                }
                hits.push(Hit { r: chip, tour: ti, duty: None });
                bus_chip(ui, &q, c, t, chip, h, tour_on);
                // the duties on the hours
                let bar_h = (pitch - 8.0).clamp(18.0, 26.0);
                ui.push_clip(Rect::new(gx - 4.0, row.y, gw + 8.0, row.h), 0.0);
                for (k, d) in t.duties.iter().enumerate() {
                    let br = Rect::new(x_of(d.from), cy - bar_h * 0.5, ((d.to - d.from) as f32 * ppm).max(6.0), bar_h);
                    let (h, _, click) = ui.interact(id_of(&format!("plan-duty-{ti}-{k}")), br);
                    if click {
                        clicked = Some((ti, Some(k)));
                        taps.push((format!("duty-{ti}-{k}"), ti));
                    }
                    hits.push(Hit { r: br, tour: ti, duty: Some(k) });
                    let chosen = sel == Some(Sel::Duty(t.tour.line.clone(), t.tour.tour.clone(), k));
                    duty_chip(ui, &q, c, p, ti, k, br, ppm, &x_of, h, chosen);
                }
                ui.pop_clip();
            }
        }
        y - v.y + 8.0
    });
    if let Some(fy) = focus_y {
        l.ui.scroll_to("plan-gantt", fy, 40.0, rows.h);
    }
    if let Some(name) = start {
        l.company.dialog = Some(Dialog::Service { line: name, when: 0, date: c.date.clone(), gaps: false });
    }
    if let Some(name) = stop {
        act(l, |c| co::network::stop_service(c, &name));
    }
    // folded, unfolded: kept with the company
    if let Some(name) = unfold {
        act(l, |c| {
            pl::set_collapsed(c, &name, false);
            Ok(())
        });
    }
    if let Some(name) = toggle {
        let to = !pl::is_collapsed(c, &name);
        act(l, |c| {
            pl::set_collapsed(c, &name, to);
            Ok(())
        });
    }
    if let Some(all) = fold_all {
        act(l, |c| {
            pl::set_all_collapsed(c, &names, all);
            Ok(())
        });
    }
    // a bus or a duty tapped twice: the line's timetable, that tour marked
    let twice = taps.into_iter().find(|(key, _)| second_tap(&mut l.company.planning, key.clone(), now)).map(|(_, ti)| ti);
    if let Some(ti) = twice {
        let t = &p.tours[ti];
        timetable = Some((t.tour.line.clone(), Some(t.tour.tour.clone())));
    }
    if let Some((line, tour)) = timetable {
        plan_edit::open(l, &line, &p.date, tour);
        return hits;
    }
    if let Some((ti, duty)) = clicked {
        let t = &p.tours[ti];
        match (l.company.planning.arm, duty) {
            (Some(Arm::Driver(w)), Some(k)) if !t.by_player => give_driver(l, p, ti, k, Some(w)),
            (Some(Arm::Bus(id)), _) if !t.by_player => give_bus(l, p, ti, Some(id)),
            (_, Some(k)) => {
                let s = Sel::Duty(t.tour.line.clone(), t.tour.tour.clone(), k);
                let v = &mut l.company.planning;
                v.sel = if v.sel.as_ref() == Some(&s) { None } else { Some(s) };
            }
            (_, None) => {
                let s = Sel::Tour(t.tour.line.clone(), t.tour.tour.clone());
                let v = &mut l.company.planning;
                v.sel = if v.sel.as_ref() == Some(&s) { None } else { Some(s) };
            }
        }
    }
    hits
}

/// The width of small capitals' words.
fn ui_w(ui: &Ui, label: &str) -> f32 {
    ui.width(&omsi_ui::tr(label).to_uppercase(), 11.0, Weight::Bold)
}

/// A line's heading: the fold, the line's plate and name (a tap folds it too), what is open as
/// issue tags, its timetable, and its service - not yet (the button that starts it), or since
/// when, with the tours covered.
#[allow(clippy::too_many_arguments)]
fn line_heading(ui: &mut Ui, q: &Pal, c: &Company, p: &DayPlan, hr: Rect, line: Option<&co::CompanyLine>, name: &String, idx: &[usize], is_folded: bool, none: bool, idle: bool, toggle: &mut Option<String>, timetable: &mut Option<(String, Option<String>)>, start: &mut Option<String>, stop: &mut Option<String>) {
    let cy = hr.center().y;
    ui.p().rect(hr, q.labels.mix(q.grid, 0.4));
    ui.p().rect(Rect::new(hr.x, hr.y, hr.w + BAR, 1.0), q.line);
    ui.p().rect(Rect::new(hr.x, hr.bottom() - 1.0, hr.w + BAR, 1.0), q.line);
    // the fold
    if !none {
        let fr = Rect::new(hr.x + 4.0, cy - 12.0, 24.0, 24.0);
        let (h, _, click) = ui.interact(id_of(&format!("plan-fold-{name}")), fr);
        ui.icon(if is_folded { "chevron_right" } else { "expand_more" }, fr.center(), 18.0, if h { q.text } else { q.dim });
        ui.tooltip(fr, if is_folded { "Expand the line: its tours" } else { "Collapse the line to its heading" });
        if click {
            *toggle = Some(name.clone());
        }
    }
    let mut x = hr.x + 32.0;
    let covered = idx.iter().filter(|&&i| p.tours[i].covered()).count();
    let state_w = 360.0f32.min(hr.w * 0.36);
    // the line's timetable, changed here (`plan_edit`)
    let tt_label = omsi_ui::tr("Timetable").into_owned();
    let tt_w = 26.0 + ui.width(&tt_label, 13.0, Weight::Medium) + 12.0;
    let tt = Rect::new(hr.right() - state_w - 12.0 - tt_w, hr.y + 4.0, tt_w, hr.h - 8.0);
    let (th, _, tclick) = ui.interact(id_of(&format!("plan-tt-{name}")), tt);
    if th {
        ui.p().rounded(tt, 3.0, q.text.alpha(0.06));
    }
    let tcol = if th { q.text } else { q.accent };
    ui.icon("departure_board", Vec2::new(tt.x + 13.0, cy), 16.0, tcol);
    ui.text_in(&tt_label, Rect::new(tt.x + 26.0, tt.y, tt.w - 28.0, tt.h), 13.0, Weight::Medium, tcol, Align::Left);
    ui.tooltip(tt, "The line's trips and timetable: see them and change them here");
    if tclick {
        *timetable = Some((name.clone(), None));
    }
    // what is open on the line's tours: duties without a driver, tours without a bus
    let open: usize = idx.iter().map(|&i| &p.tours[i]).filter(|t| !t.by_player && t.bus.is_some()).map(|t| t.duties.iter().filter(|d| d.open()).count()).sum();
    let no_bus = idx.iter().map(|&i| &p.tours[i]).filter(|t| !t.by_player && t.bus.is_none()).count();
    let mut tags: Vec<(String, &str)> = Vec::new();
    if no_bus > 0 {
        tags.push((omsi_ui::tr("%{n} without a bus").replace("%{n}", &no_bus.to_string()), "directions_bus"));
    }
    if open > 0 {
        tags.push((omsi_ui::tr("%{n} without a driver").replace("%{n}", &open.to_string()), "person"));
    }
    let tag_col = if idle { q.dim } else { q.warn };
    let tags_w: f32 = tags.iter().map(|t| tag_w(ui, &t.0) + 6.0).sum();
    let name_right = tt.x - 10.0;
    let mut tx = name_right;
    // (the tags only where the name keeps room to be read)
    let with_tags = name_right - tags_w - (x + 60.0) >= 140.0;
    if with_tags {
        for (t, icon) in tags.iter().rev() {
            let w = tag_w(ui, t);
            tx -= w;
            issue_chip(ui, Rect::new(tx, cy - 10.0, w, 20.0), tag_col, icon, t, false);
            tx -= 6.0;
        }
    }
    let Some(cl) = line else {
        ui.text_in(name, Rect::new(x, hr.y, (tx - x - 8.0).max(40.0), hr.h), 13.0, Weight::Bold, q.text, Align::Left);
        return;
    };
    x += plate(ui, c, &cl.name, &cl.number, x, cy) + 10.0;
    let caption = if cl.caption.is_empty() { cl.name.clone() } else { cl.caption.clone() };
    let cap = Rect::new(x, hr.y, (tx - x - 8.0).max(40.0), hr.h);
    ui.text_in(&caption, cap, 13.0, Weight::Bold, q.text, Align::Left);
    let (_, _, tap) = ui.interact(id_of(&format!("plan-fold-name-{name}")), Rect::new(hr.x + 30.0, hr.y, cap.right() - hr.x - 30.0, hr.h));
    if tap && !none {
        *toggle = Some(name.clone());
    }
    if !with_tags && !tags.is_empty() {
        ui.tooltip(cap, &tags.iter().map(|t| t.0.clone()).collect::<Vec<_>>().join("  ·  "));
    }
    // its service: not yet (the button that starts it), or since when
    let sr = Rect::new(hr.right() - state_w, hr.y + 4.0, state_w - 6.0, hr.h - 8.0);
    if none {
        ui.text_in(&omsi_ui::tr("No tours this day"), sr, 12.0, Weight::Medium, q.dim, Align::Right);
    } else if cl.service_from.is_none() || idle {
        let label = omsi_ui::tr("Start service…").into_owned();
        let bw = ui.width(&label.to_uppercase(), 12.5, Weight::Bold) + 30.0;
        let b = Rect::new(sr.right() - bw, sr.y, bw, sr.h);
        if rf_button(ui, q, &format!("plan-start-{}", cl.name), b, &label, true, true) {
            *start = Some(cl.name.clone());
        }
        ui.tooltip(b, "Its tours run from the moment you choose, as they are planned; what is not covered then is dropped with the contract's penalty");
        let note = omsi_ui::tr("Not in service · %{c} of %{t} tours planned").replace("%{c}", &covered.to_string()).replace("%{t}", &idx.len().to_string());
        ui.text_in(&note, Rect::new(sr.x, sr.y, sr.w - bw - 10.0, sr.h), 12.0, Weight::Medium, q.dim, Align::Right);
    } else {
        let since = cl.service_from.map(|s| if s <= 0 { String::new() } else { format!("{} {}", day_label(&co::clock::date_of(s)), co::clock::hhmm(s)) }).unwrap_or_default();
        let text = if since.is_empty() { omsi_ui::tr("In service").into_owned() } else { omsi_ui::tr("In service since %{when}").replace("%{when}", &since) };
        let cov = omsi_ui::tr("%{c} of %{t} tours covered").replace("%{c}", &covered.to_string()).replace("%{t}", &idx.len().to_string());
        let all = covered == idx.len();
        ui.text_in(&format!("{text}  ·  {cov}"), Rect::new(sr.x, sr.y, sr.w - 30.0, sr.h), 12.0, Weight::Medium, if all { q.ok } else { q.warn }, Align::Right);
        if rf_icon(ui, q, &format!("plan-stop-{}", cl.name), Vec2::new(sr.right() - 10.0, cy), "pause", "Take the line out of service", false, true) {
            *stop = Some(cl.name.clone());
        }
    }
}

/// A tour's bus in the ID column: the bus (grey when only the dispatcher's suggestion), a
/// rental, or - none - RosterFlow's issue chip.
fn bus_chip(ui: &mut Ui, q: &Pal, c: &Company, t: &DayTour, chip: Rect, h: bool, on: bool) {
    if t.by_player {
        ui.text_in(&omsi_ui::tr("You drove it."), chip, 12.0, Weight::Bold, q.accent, Align::Left);
        return;
    }
    let tip = omsi_ui::tr("The tour's bus: tap a bus in the list, then here - or drag it here. Tap it twice: the line's timetable").into_owned();
    match t.bus {
        None => {
            let col = ink(q, problem_colour(t.bus_problem));
            let icon = if t.bus_problem.is_some_and(|x| x != Problem::Unassigned) { "error" } else { "warning" };
            issue_chip(ui, chip, col, icon, &omsi_ui::tr("No bus"), h);
            let why = t.bus_problem.map(|x| omsi_ui::tr(x.label()).into_owned()).unwrap_or_default();
            ui.tooltip(chip, &if why.is_empty() { tip } else { format!("{why}\n{tip}") });
        }
        Some(bus) => {
            let (text, suggested) = match bus {
                BusOf::Own(id) => (c.vehicle(id).map(|x| format!("{}  {}", x.number, x.name)).unwrap_or_default(), t.bus_from == Source::Auto),
                BusOf::Rental => (omsi_ui::tr("Rental bus").into_owned(), false),
            };
            let fill = if suggested { q.tool.alpha(0.5) } else { q.tool };
            ui.p().rounded(chip, 3.0, if h { fill.mix(q.text, 0.08) } else { fill });
            if let Some(pb) = t.bus_problem {
                ui.p().rounded_border(chip, 3.0, 1.2, ink(q, problem_colour(Some(pb))));
            }
            let col = if suggested { q.dim } else { q.text };
            ui.icon("directions_bus", Vec2::new(chip.x + 11.0, chip.center().y), 13.0, col);
            ui.text_in(&text, Rect::new(chip.x + 22.0, chip.y, chip.w - 26.0, chip.h), 12.0, Weight::Bold, col, Align::Left);
            ui.tooltip(chip, &tip);
        }
    }
    if on {
        ui.p().rounded_border(chip.inset(-1.5), 4.0, 1.6, q.accent);
    }
}

/// A duty on the hours: in the colour of who drives it (paler when only the dispatcher's
/// suggestion, outlined orange when chosen for today alone), or - nobody - RosterFlow's issue
/// chip with what is wrong and its times.
#[allow(clippy::too_many_arguments)]
fn duty_chip(ui: &mut Ui, q: &Pal, c: &Company, p: &DayPlan, ti: usize, k: usize, br: Rect, ppm: f32, x_of: &dyn Fn(i32) -> f32, h: bool, chosen: bool) {
    let t = &p.tours[ti];
    let d = &t.duties[k];
    let times = format!("{} – {}", hhmm(d.from), hhmm(d.to));
    let open = d.who.is_none() && !t.by_player;
    // (a tour without its bus does not run: its drivers' duties paler)
    let faint = if t.bus.is_none() && !t.by_player { 0.5 } else { 1.0 };
    if open {
        let col = ink(q, problem_colour(d.problem));
        let icon = if d.problem.is_some_and(|x| x != Problem::Unassigned) { "error" } else { "warning" };
        let words = problem_text(c, p, ti, k, d.problem);
        let full = format!("{words}  {times}");
        let say = if ui.width(&full, 11.5, Weight::Bold) + 36.0 <= br.w { full } else { words };
        issue_chip(ui, br, col, icon, &say, h);
    } else {
        let (fill, ink_c, label, short) = if t.by_player {
            (q.accent.alpha(0.6), Color::WHITE, omsi_ui::tr("You").into_owned(), String::new())
        } else {
            match d.who {
                Some(Who::Staff(id)) => {
                    let col = driver_colour(c, id);
                    let full = c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
                    (if d.from_ == Source::Auto { col.alpha(0.6) } else { col }, INK, first_name(c, id), initials(&full))
                }
                Some(Who::Player) => (q.accent, Color::WHITE, omsi_ui::tr("You").into_owned(), String::new()),
                Some(Who::Agency) => (q.grey, Color::WHITE, omsi_ui::tr("Agency").into_owned(), String::new()),
                None => (q.grey, Color::WHITE, String::new(), String::new()),
            }
        };
        ui.p().rounded(br, 3.0, fill.alpha(faint * if h { 0.85 } else { 1.0 }));
        if matches!(d.from_, Source::Dispatcher | Source::Central) {
            ui.p().rounded_border(br, 3.0, 1.5, q.warn);
        }
        // its depot and empty runs, shaded: run with it, without passengers
        let shade = (br.h * 0.2).clamp(3.0, 5.0);
        for x in t.tour.trips[d.start..d.end].iter().filter(|x| x.empty) {
            let a = x_of(x.dep.max(d.from));
            let b = x_of(x.arr.min(d.to)).max(a + 2.0);
            ui.p().rect(Rect::new(a, br.bottom() - shade, b - a, shade), Color::rgba(0, 0, 0, 0.3));
        }
        let px = 11.5;
        let full = format!("{label}  {times}");
        if let Some(words) = fit_words(ui, br.w, px, &full, &label).or_else(|| fit_words(ui, br.w, px, &short, &short)) {
            ui.text_in(words, Rect::new(br.x + 6.0, br.y, br.w - 10.0, br.h), px, Weight::Bold, ink_c.alpha(faint.max(0.7)), Align::Left);
        }
    }
    // a late driver's first minutes
    if let Some(late) = d.late {
        let lw = ((late.until.min(d.to) - d.from) as f32 * ppm).max(3.0);
        let col = if late.cover.is_some() { q.dim } else { q.err };
        ui.p().rounded(Rect::new(br.x, br.y, lw, br.h), 3.0, col.alpha(0.6));
    }
    if !d.warn.is_empty() {
        ui.p().circle(Vec2::new(br.right() - 5.0, br.y + 5.0), 3.0, q.warn);
    }
    if chosen {
        ui.p().rounded_border(br.inset(-2.0), 4.0, 2.0, q.text);
    }
    let tip = format!("{times}  ·  {}", who_name(c, if t.by_player { Some(Who::Player) } else { d.who }));
    ui.tooltip(br, &tip);
}

// --- the side ------------------------------------------------------------------------------------

fn side_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan, q: &Pal) {
    let sel = l.company.planning.sel.clone();
    let find = |line: &str, tour: &str| p.tours.iter().position(|t| t.tour.line == line && t.tour.tour == tour);
    match sel {
        Some(Sel::Duty(line, tour, k)) => match find(&line, &tour).filter(|&i| k < p.tours[i].duties.len()) {
            Some(ti) => duty_panel(l, r, c, p, ti, k, q),
            None => day_panel(l, r, c, p, q),
        },
        Some(Sel::Tour(line, tour)) => match find(&line, &tour) {
            Some(ti) => tour_panel(l, r, c, p, ti, q),
            None => day_panel(l, r, c, p, q),
        },
        None => day_panel(l, r, c, p, q),
    }
}

/// The side panel's sheet and heading; returns the room under it.
fn panel(ui: &mut Ui, q: &Pal, r: Rect, title: &str) -> Rect {
    ui.p().rect(r, q.sheet);
    ui.p().rect(Rect::new(r.x, r.y, 1.0, r.h), q.text.alpha(0.1));
    let head = Rect::new(r.x, r.y, r.w, SIDE_HEAD);
    ui.p().rect(head, q.axis);
    ui.p().rect(Rect::new(r.x, head.bottom() - 1.0, r.w, 1.0), q.line);
    caps(ui, Rect::new(r.x + 16.0, r.y, r.w - 60.0, SIDE_HEAD), title, q.dim, Align::Left);
    Rect::new(r.x + 14.0, head.bottom() + 12.0, r.w - 16.0, (r.h - SIDE_HEAD - 16.0).max(0.0))
}

/// A list row with a name, what it says on the right, and a swatch; returns (clicked,
/// pressed on it).
#[allow(clippy::too_many_arguments)]
fn pick_row(ui: &mut Ui, q: &Pal, name: &str, r: Rect, on: bool, swatch: Option<Color>, text: &str, right: &str, right_c: Color) -> (bool, bool) {
    let pressed = ui.hover(r) && ui.input.pressed;
    let (h, _, clicked) = ui.interact(id_of(name), r);
    if on {
        ui.p().rounded(r, 3.0, q.accent.alpha(0.16));
        ui.p().rect(Rect::new(r.x, r.y + 5.0, 3.0, r.h - 10.0), q.accent);
    } else if h {
        ui.p().rounded(r, 3.0, q.text.alpha(0.05));
    }
    let mut x = r.x + 12.0;
    if let Some(s) = swatch {
        ui.p().rounded(Rect::new(x, r.y + r.h * 0.5 - 5.0, 10.0, 10.0), 2.0, s);
        x += 18.0;
    }
    let rw = ui.width(right, 12.0, Weight::Regular).min(r.w * 0.5);
    ui.text_in(text, Rect::new(x, r.y, r.right() - x - rw - 16.0, r.h), 13.0, Weight::Medium, q.text, Align::Left);
    ui.text_in(right, Rect::new(r.right() - rw - 10.0, r.y, rw, r.h), 12.0, Weight::Regular, if on { q.text } else { right_c }, Align::Right);
    (clicked, pressed)
}

/// Nothing chosen: the day in short, what fell out, and the drivers and buses to give.
fn day_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan, q: &Pal) {
    let inner = panel(&mut l.ui, q, r, if p.today { "The company's day" } else { "This day" });
    let (tours, covered, open, no_bus) = p.counts();
    let rows = p.open_rows();
    let arm = l.company.planning.arm;
    // what the morning brought, and what is open
    let mut morning: Vec<(String, Color)> = Vec::new();
    for d in &p.disruptions {
        match *d {
            Disruption::Late { employee, minutes } => morning.push((omsi_ui::tr("%{name} comes %{n} minutes late.").replace("%{name}", &who_name(c, Some(Who::Staff(employee)))).replace("%{n}", &minutes.to_string()), q.warn)),
            Disruption::Breakdown { vehicle, cost } => morning.push((omsi_ui::tr("Bus %{n} did not start: towed to the workshop (%{amount}).").replace("%{n}", &c.vehicle(vehicle).map(|v| v.number.clone()).unwrap_or_default()).replace("%{amount}", &eur(cost)), q.err)),
        }
    }
    // (the drivers: only they drive)
    let staff: Vec<(u32, String, Color, String, Color)> = c
        .staff
        .iter()
        .filter(|e| co::staff::drives(e))
        .map(|e| {
            let mins: i32 = p.duties_of(Who::Staff(e.id)).iter().map(|&(i, k)| p.tours[i].duties[k].to - p.tours[i].duties[k].from).sum();
            let (state, col) = if !e.employed_on(&p.date) {
                (omsi_ui::tr("Not employed then").into_owned(), q.faint)
            } else if e.sick_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
                (omsi_ui::tr("Ill").into_owned(), q.err)
            } else if e.holiday_until.as_deref().is_some_and(|u| co::dates::between(&p.date, u) >= 0) {
                (omsi_ui::tr("On holiday").into_owned(), q.dim)
            } else if mins == 0 {
                (omsi_ui::tr("Free").into_owned(), q.dim)
            } else {
                (length(mins), if mins > co::staff::DAY_TARGET { q.warn } else { q.text.alpha(0.82) })
            };
            (e.id, e.name.clone(), driver_colour(c, e.id), state, col)
        })
        .collect();
    let buses: Vec<(u32, String, String, Color)> = c
        .fleet
        .iter()
        .map(|v| {
            let n = p.tours.iter().filter(|t| t.bus == Some(BusOf::Own(v.id))).count();
            let (state, col) = if !v.held_on(&p.date) {
                (omsi_ui::tr("Gone back then").into_owned(), q.faint)
            } else if v.in_workshop(&p.date) || p.disruptions.iter().any(|d| matches!(d, Disruption::Breakdown { vehicle, .. } if *vehicle == v.id)) {
                (omsi_ui::tr("In the workshop").into_owned(), q.err)
            } else if n == 0 {
                (omsi_ui::tr("Free").into_owned(), q.dim)
            } else {
                (omsi_ui::tr("%{n} tours").replace("%{n}", &n.to_string()), q.text.alpha(0.82))
            };
            (v.id, format!("{}  {}", v.number, v.name), state, col)
        })
        .collect();
    let open_rows: Vec<(usize, Option<usize>, String, String, Color, bool)> = rows
        .iter()
        .map(|o| {
            let t = &p.tours[o.tour];
            let what = match o.duty {
                _ if o.late => omsi_ui::tr("Late start").into_owned(),
                Some(k) => problem_text(c, p, o.tour, k, o.problem),
                None => omsi_ui::tr(o.problem.map(Problem::label).unwrap_or("No driver")).into_owned(),
            };
            let fill = if o.filled {
                match (o.duty, o.late) {
                    (None, _) => match t.bus {
                        Some(BusOf::Rental) => omsi_ui::tr("Rental bus").into_owned(),
                        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default(),
                        None => String::new(),
                    },
                    (Some(k), true) => t.duties[k].late.and_then(|x| x.cover).map(|id| who_name(c, Some(Who::Staff(id)))).unwrap_or_default(),
                    (Some(k), false) => who_name(c, t.duties[k].who),
                }
            } else {
                omsi_ui::tr("Trips dropped").into_owned()
            };
            let hard = o.problem.is_some_and(|x| x != Problem::Unassigned);
            (o.tour, o.duty, format!("{}  ·  {}", line_title(t, o.duty), what), fill, if o.filled { q.text.alpha(0.82) } else { ink(q, problem_colour(o.problem)) }, hard)
        })
        .collect();
    let mut tapped: Option<Arm> = None;
    let mut pressed: Option<Arm> = None;
    let mut open_pick: Option<(usize, Option<usize>)> = None;
    let mut open_press: Option<(usize, Option<usize>)> = None;
    let mut targets: Vec<(Rect, Arm)> = Vec::new();
    // (a duty or tour carried: the drivers and buses it could go to light up under the mouse)
    let carrying = l.company.planning.carry.map(|c| c.1.is_some());
    let clip = inner;
    let summary = omsi_ui::tr("%{c} of %{t} tours covered").replace("%{c}", &covered.to_string()).replace("%{t}", &tours.to_string());
    let gaps = if open + no_bus == 0 { omsi_ui::tr("Nothing open.").into_owned() } else { omsi_ui::tr("%{d} duties without a driver, %{b} tours without a bus.").replace("%{d}", &open.to_string()).replace("%{b}", &no_bus.to_string()) };
    let look = rf_scroll::look(q);
    let q = *q;
    rf_scroll::area(&mut l.ui, &look, "plan-side", inner, &mut |ui, v| {
        let w = v.w - BAR - 2.0;
        let mut y = v.y;
        ui.text_in(&summary, Rect::new(v.x, y, w, 22.0), 16.0, Weight::Bold, q.text, Align::Left);
        y += 26.0;
        let ok = open + no_bus == 0;
        ui.icon(if ok { "check_circle" } else { "warning" }, Vec2::new(v.x + 7.0, y + 10.0), 14.0, if ok { q.ok } else { q.warn });
        ui.text_in(&gaps, Rect::new(v.x + 20.0, y, w - 20.0, 20.0), 12.5, Weight::Medium, if ok { q.ok } else { q.warn }, Align::Left);
        y += 34.0;
        let head = |ui: &mut Ui, y: f32, t: &str| caps(ui, Rect::new(v.x, y, w, 16.0), t, q.dim, Align::Left);
        if !morning.is_empty() {
            head(ui, y, "This morning");
            y += 22.0;
            for (t, col) in &morning {
                ui.p().circle(Vec2::new(v.x + 4.0, y + 8.0), 3.0, *col);
                let h = ui.paragraph(t, Vec2::new(v.x + 14.0, y), w - 16.0, 12.5, Weight::Regular, q.text.alpha(0.82));
                y += h.max(18.0) + 6.0;
            }
            y += 8.0;
        }
        if !open_rows.is_empty() {
            head(ui, y, "Open duties");
            y += 22.0;
            for (k, (ti, duty, title, fill, col, hard)) in open_rows.iter().enumerate() {
                let rr = Rect::new(v.x, y, w, 46.0);
                if ui.hover(rr) && ui.input.pressed {
                    open_press = Some((*ti, *duty));
                }
                let (h, _, click) = ui.interact(id_of(&format!("plan-open-{k}")), rr);
                if click {
                    open_pick = Some((*ti, *duty));
                }
                if h {
                    ui.p().rounded(rr, 3.0, q.text.alpha(0.05));
                }
                ui.p().rect(Rect::new(rr.x, rr.y + 6.0, 3.0, rr.h - 12.0), *col);
                ui.icon(if *hard { "error" } else { "warning" }, Vec2::new(rr.x + 16.0, rr.y + 14.0), 14.0, *col);
                ui.icon("open_with", Vec2::new(rr.right() - 14.0, rr.center().y), 15.0, if h { q.text } else { q.faint });
                ui.text_in(title, Rect::new(rr.x + 28.0, rr.y + 4.0, rr.w - 50.0, 20.0), 12.5, Weight::Bold, q.text, Align::Left);
                ui.text_in(&format!("→ {fill}"), Rect::new(rr.x + 28.0, rr.y + 23.0, rr.w - 50.0, 18.0), 12.0, Weight::Regular, *col, Align::Left);
                y += 48.0;
            }
            y += 8.0;
        }
        head(ui, y, "Drivers");
        y += 20.0;
        ui.text_in(&omsi_ui::tr("Tap one, then a duty - or drag it there; or drag a duty here."), Rect::new(v.x, y, w, 18.0), 12.0, Weight::Regular, q.dim, Align::Left);
        y += 24.0;
        let me = Rect::new(v.x, y, w, 30.0);
        let mut target = |ui: &mut Ui, r: Rect, a: Arm| {
            let fits = match (carrying, a) {
                (Some(true), _) => true,
                (Some(false), Arm::Bus(_)) => true,
                _ => false,
            };
            if fits {
                let (x0, y0) = (r.x.max(clip.x), r.y.max(clip.y));
                let shown = Rect::new(x0, y0, r.right().min(clip.right()) - x0, r.bottom().min(clip.bottom()) - y0);
                if shown.w > 0.0 && shown.h > 0.0 {
                    targets.push((shown, a));
                    if ui.hover(r) {
                        ui.p().rounded_border(r, 3.0, 2.0, q.accent);
                    }
                }
            }
        };
        target(ui, me, Arm::Driver(Who::Player));
        let (cl, pr) = pick_row(ui, &q, "plan-me", me, arm == Some(Arm::Driver(Who::Player)), Some(q.accent), &omsi_ui::tr("You"), &omsi_ui::tr("Your own duties"), q.dim);
        if cl {
            tapped = Some(Arm::Driver(Who::Player));
        }
        if pr {
            pressed = Some(Arm::Driver(Who::Player));
        }
        y += 32.0;
        for (id, name, col, state, state_c) in &staff {
            let rr = Rect::new(v.x, y, w, 30.0);
            let a = Arm::Driver(Who::Staff(*id));
            target(ui, rr, a);
            let (cl, pr) = pick_row(ui, &q, &format!("plan-driver-{id}"), rr, arm == Some(a), Some(*col), name, state, *state_c);
            if cl {
                tapped = Some(a);
            }
            if pr {
                pressed = Some(a);
            }
            y += 32.0;
        }
        if staff.is_empty() {
            ui.text_in(&omsi_ui::tr("Nobody on the payroll yet."), Rect::new(v.x, y, w, 20.0), 12.5, Weight::Regular, q.dim, Align::Left);
            y += 22.0;
        }
        y += 10.0;
        head(ui, y, "Buses");
        y += 22.0;
        for (id, name, state, state_c) in &buses {
            let rr = Rect::new(v.x, y, w, 30.0);
            let a = Arm::Bus(*id);
            target(ui, rr, a);
            let (cl, pr) = pick_row(ui, &q, &format!("plan-bus-pick-{id}"), rr, arm == Some(a), None, name, state, *state_c);
            if cl {
                tapped = Some(a);
            }
            if pr {
                pressed = Some(a);
            }
            y += 32.0;
        }
        if buses.is_empty() {
            ui.text_in(&omsi_ui::tr("No bus in the fleet yet."), Rect::new(v.x, y, w, 20.0), 12.5, Weight::Regular, q.dim, Align::Left);
            y += 22.0;
        }
        y - v.y + 8.0
    });
    let mouse = l.ui.input.mouse;
    let view = &mut l.company.planning;
    view.targets = targets;
    if let Some(a) = pressed {
        view.drag = Some(a);
    }
    if let Some(o) = open_press {
        view.carry = Some(o);
        view.carry_from = mouse;
    }
    if let Some(a) = tapped {
        view.arm = if view.arm == Some(a) { None } else { Some(a) };
        view.drag = None;
    }
    if let Some((ti, duty)) = open_pick {
        // (a tap, not a carry)
        view.carry = None;
        let t = &p.tours[ti];
        view.sel = Some(match duty {
            Some(k) => Sel::Duty(t.tour.line.clone(), t.tour.tour.clone(), k),
            None => Sel::Tour(t.tour.line.clone(), t.tour.tour.clone()),
        });
    }
}

/// The way from a duty or a tour to its line's trips and timetable (`plan_edit`).
fn timetable_button(l: &mut Launcher, q: &Pal, r: Rect, name: &str) -> bool {
    let pressed = link_button(&mut l.ui, q, name, r, "departure_board", "The line's trips and timetable");
    l.ui.tooltip(r, "See the line's departures and the tours they make, and change them");
    pressed
}

/// The panel's close (back to the day).
fn close(l: &mut Launcher, q: &Pal, r: Rect) -> bool {
    if rf_icon(&mut l.ui, q, "plan-sel-close", Vec2::new(r.right() - 24.0, r.y + SIDE_HEAD * 0.5), "close", "Back to the day", false, true) {
        l.company.planning.sel = None;
        return true;
    }
    false
}

/// Facts of a duty or a tour, a line each (broken where long); returns the height.
fn facts_draw(ui: &mut Ui, facts: &[(String, Color)], x: f32, y: f32, w: f32) -> f32 {
    let mut yy = y;
    for (s, col) in facts {
        let h = ui.paragraph(s, Vec2::new(x, yy), w, 12.5, Weight::Regular, *col);
        yy += h.max(18.0) + 4.0;
    }
    yy - y
}

/// A duty chosen: its times, who drives it and why, who else could; on the company's day
/// what to do when it is open, and "Drive this duty".
fn duty_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan, ti: usize, k: usize, q: &Pal) {
    let t = p.tours[ti].clone();
    let d = t.duties[k].clone();
    let inner = panel(&mut l.ui, q, r, "Duty");
    if close(l, q, r) {
        return;
    }
    let key = pl::duty_key(&t.tour.line, &t.tour.tour, k);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let late_key = pl::late_key(&t.tour.line, &t.tour.tour, k);
    let late_fill = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == late_key);
    let first = &t.tour.trips[d.start];
    let last = &t.tour.trips[d.end - 1];
    let mut facts: Vec<(String, Color)> = vec![
        (format!("{} – {}  ·  {}  ·  {} {}", hhmm(d.from), hhmm(d.to), length(d.to - d.from), d.end - d.start, omsi_ui::tr("trips")), q.text.alpha(0.86)),
        (format!("{} → {}", first.from, last.to), q.dim),
    ];
    let who = if t.by_player { omsi_ui::tr("You drove it.").into_owned() } else { format!("{}: {}", omsi_ui::tr("Driven by"), who_name(c, d.who)) };
    facts.push((who, q.text));
    if !source_text(d.from_).is_empty() && d.who.is_some() {
        facts.push((omsi_ui::tr(source_text(d.from_)).into_owned(), q.dim));
    }
    if let Some(pb) = d.problem {
        facts.push((format!("{}: {}", omsi_ui::tr("Open because"), omsi_ui::tr(pb.label())), ink(q, problem_colour(Some(pb)))));
    }
    for w in &d.warn {
        let s = match w {
            Warn::Overtime(m) => omsi_ui::tr("Overtime %{t}").replace("%{t}", &length(*m)),
            Warn::Transfer(m) => omsi_ui::tr("%{n} min to change stops").replace("%{n}", &m.to_string()),
            Warn::Experience => omsi_ui::tr("Little experience for this bus").into_owned(),
        };
        facts.push((s, q.warn));
    }
    if let Some(late) = d.late {
        let s = match late.cover {
            Some(id) => omsi_ui::tr("The driver is late until %{t}: %{name} drives the first trips.").replace("%{t}", &hhmm(late.until)).replace("%{name}", &who_name(c, Some(Who::Staff(id)))),
            None => omsi_ui::tr("The driver is late until %{t}: the trips before are dropped.").replace("%{t}", &hhmm(late.until)),
        };
        facts.push((s, if late.cover.is_some() { q.warn } else { q.err }));
    }
    let mut y = inner.y;
    l.ui.swatch(Rect::new(inner.x, y + 5.0, 3.0, 14.0), 1.0, line_colour(c, &t.tour.line));
    l.ui.text_in(&line_title(&t, Some(k)), Rect::new(inner.x + 10.0, y, inner.w - 14.0, 24.0), 15.0, Weight::Bold, q.text, Align::Left);
    y += 32.0;
    if timetable_button(l, q, Rect::new(inner.x, y, inner.w - 4.0, 32.0), "plan-duty-tt") {
        plan_edit::open(l, &t.tour.line, &p.date, Some(t.tour.tour.clone()));
        return;
    }
    y += 42.0;
    y += facts_draw(&mut l.ui, &facts, inner.x, y, inner.w - 6.0);
    y += 8.0;
    // on the company's day: drive it, and what to do while it is open
    let foot_h = if p.today && !t.by_player { 50.0 } else { 0.0 };
    if p.today && !t.by_player {
        let b = Rect::new(inner.x, inner.bottom() - 38.0, inner.w - 4.0, 36.0);
        if rf_button(&mut l.ui, q, "plan-drive", b, "Drive this duty", true, true) {
            drive(l, c, p, ti, k);
            return;
        }
    }
    let mut options: Vec<(String, Option<Fill>, bool)> = Vec::new();
    if p.today && !t.by_player {
        if d.problem.is_some() || d.who.is_none() {
            options.push((omsi_ui::tr("An agency driver for today").into_owned(), Some(Fill::Agency), true));
            options.push((omsi_ui::tr("Drop its trips today").into_owned(), Some(Fill::Drop), true));
            if fill_now {
                options.push((omsi_ui::tr("Leave it to the central").into_owned(), None, true));
            }
        }
        if d.late.is_some() {
            options.push((omsi_ui::tr("Drop the first trips").into_owned(), Some(Fill::Drop), false));
            if late_fill {
                options.push((omsi_ui::tr("Let the central cover the first trips").into_owned(), None, false));
            }
        }
    }
    let cands: Vec<(Option<Who>, String, String, Color, Option<Color>)> = {
        let mut v: Vec<(Option<Who>, String, String, Color, Option<Color>)> = Vec::new();
        v.push((Some(Who::Player), omsi_ui::tr("You").into_owned(), omsi_ui::tr("Drive it yourself").into_owned(), q.dim, Some(q.accent)));
        let mut people: Vec<(bool, u32, String, String, Color)> = c
            .staff
            .iter()
            .filter(|e| co::staff::drives(e))
            .map(|e| {
                let (s, col, ok) = fit_of(c, p, ti, k, e.id);
                (ok, e.id, e.name.clone(), s, ink(q, col))
            })
            .collect();
        people.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)));
        for (_, id, name, s, col) in people {
            v.push((Some(Who::Staff(id)), name, s, col, Some(driver_colour(c, id))));
        }
        v.push((None, omsi_ui::tr("Nobody").into_owned(), String::new(), q.dim, None));
        v
    };
    let list = Rect::new(inner.x, y, inner.w, (inner.bottom() - foot_h - y).max(0.0));
    let current = if t.by_player { Some(Who::Player) } else { d.who };
    let mut pick: Option<Option<Who>> = None;
    let mut opt: Option<(Option<Fill>, bool)> = None;
    let look = rf_scroll::look(q);
    let q = *q;
    rf_scroll::area(&mut l.ui, &look, "plan-duty-side", list, &mut |ui, v| {
        let w = v.w - BAR - 2.0;
        let mut yy = v.y;
        if !options.is_empty() {
            caps(ui, Rect::new(v.x, yy, w, 16.0), "Today", q.dim, Align::Left);
            yy += 22.0;
            for (n, (label, fill, whole)) in options.iter().enumerate() {
                let rr = Rect::new(v.x, yy, w, 30.0);
                if pick_row(ui, &q, &format!("plan-opt-{n}"), rr, false, None, label, "", q.dim).0 {
                    opt = Some((*fill, *whole));
                }
                yy += 32.0;
            }
            yy += 10.0;
        }
        caps(ui, Rect::new(v.x, yy, w, 16.0), "Who drives it", q.dim, Align::Left);
        yy += 22.0;
        for (n, (who, name, s, col, sw)) in cands.iter().enumerate() {
            let rr = Rect::new(v.x, yy, w, 30.0);
            if pick_row(ui, &q, &format!("plan-cand-{n}"), rr, *who == current && who.is_some(), *sw, name, s, *col).0 {
                pick = Some(*who);
            }
            yy += 32.0;
        }
        yy - v.y + 8.0
    });
    if let Some(w) = pick {
        if !t.by_player {
            give_driver(l, p, ti, k, w);
        }
    }
    if let Some((fill, whole)) = opt {
        set_fill(l, if whole { key } else { late_key }, fill);
    }
}

/// A tour chosen: its bus and which others could run it; on the company's day a rental bus
/// or dropping it.
fn tour_panel(l: &mut Launcher, r: Rect, c: &Company, p: &DayPlan, ti: usize, q: &Pal) {
    let t = p.tours[ti].clone();
    let inner = panel(&mut l.ui, q, r, "Tour");
    if close(l, q, r) {
        return;
    }
    let mut y = inner.y;
    l.ui.swatch(Rect::new(inner.x, y + 5.0, 3.0, 14.0), 1.0, line_colour(c, &t.tour.line));
    l.ui.text_in(&line_title(&t, None), Rect::new(inner.x + 10.0, y, inner.w - 14.0, 24.0), 15.0, Weight::Bold, q.text, Align::Left);
    y += 32.0;
    if timetable_button(l, q, Rect::new(inner.x, y, inner.w - 4.0, 32.0), "plan-tour-tt") {
        plan_edit::open(l, &t.tour.line, &p.date, Some(t.tour.tour.clone()));
        return;
    }
    y += 42.0;
    let mut facts: Vec<(String, Color)> = vec![(format!("{} – {}  ·  {} km  ·  {} {}", hhmm(t.tour.from()), hhmm(t.tour.to()), t.tour.km().round(), t.duties.len(), omsi_ui::tr("duties")), q.text.alpha(0.86))];
    let bus = match t.bus {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default(),
        Some(BusOf::Rental) => omsi_ui::tr("Rental bus").into_owned(),
        None => omsi_ui::tr("No bus").into_owned(),
    };
    facts.push((format!("{}: {}", omsi_ui::tr("Bus"), bus), if t.bus.is_none() && !t.by_player { q.warn } else { q.text }));
    if t.bus.is_some() && !source_text(t.bus_from).is_empty() {
        facts.push((omsi_ui::tr(source_text(t.bus_from)).into_owned(), q.dim));
    }
    if let Some(pb) = t.bus_problem {
        facts.push((format!("{}: {}", omsi_ui::tr("Open because"), omsi_ui::tr(pb.label())), ink(q, problem_colour(Some(pb)))));
    }
    if let Some(w) = t.tour.wants() {
        facts.push((omsi_ui::tr("The tour asks for: %{kind}").replace("%{kind}", &omsi_ui::tr(co::BusKind { size: w, drive: co::Drive::Diesel }.label())), q.dim));
    }
    y += facts_draw(&mut l.ui, &facts, inner.x, y, inner.w - 6.0);
    y += 8.0;
    let key = pl::tour_key(&t.tour.line, &t.tour.tour);
    let fill_now = c.planning.date == c.date && c.planning.fills.iter().any(|f| f.key == key);
    let mut options: Vec<(String, Option<Fill>)> = Vec::new();
    if p.today && !t.by_player && (t.bus.is_none() || t.bus_problem.is_some()) {
        let rent = co::economy::rent_per_day(co::BusKind { size: t.tour.wants().unwrap_or_default(), drive: co::Drive::Diesel }, &co::economy::rules(c.difficulty), c.price_index);
        options.push((omsi_ui::tr("Rent a bus for today (%{amount})").replace("%{amount}", &eur(rent)), Some(Fill::Rental)));
        options.push((omsi_ui::tr("Drop the tour today").into_owned(), Some(Fill::Drop)));
        if fill_now {
            options.push((omsi_ui::tr("Leave it to the central").into_owned(), None));
        }
    }
    let mut cands: Vec<(bool, Option<u32>, String, String, Color)> = c
        .fleet
        .iter()
        .map(|v| {
            let (s, col, ok) = bus_fit(c, p, ti, v.id);
            (ok, Some(v.id), format!("{}  {}", v.number, v.name), s, ink(q, col))
        })
        .collect();
    cands.sort_by(|a, b| b.0.cmp(&a.0));
    cands.push((true, None, omsi_ui::tr("No bus").into_owned(), String::new(), q.dim));
    let current = match t.bus {
        Some(BusOf::Own(id)) => Some(id),
        _ => None,
    };
    let list = Rect::new(inner.x, y, inner.w, (inner.bottom() - y).max(0.0));
    let mut pick: Option<Option<u32>> = None;
    let mut opt: Option<Option<Fill>> = None;
    let by_player = t.by_player;
    let look = rf_scroll::look(q);
    let q = *q;
    rf_scroll::area(&mut l.ui, &look, "plan-tour-side", list, &mut |ui, v| {
        let w = v.w - BAR - 2.0;
        let mut yy = v.y;
        if !options.is_empty() {
            caps(ui, Rect::new(v.x, yy, w, 16.0), "Today", q.dim, Align::Left);
            yy += 22.0;
            for (n, (label, fill)) in options.iter().enumerate() {
                let rr = Rect::new(v.x, yy, w, 30.0);
                if pick_row(ui, &q, &format!("plan-topt-{n}"), rr, false, None, label, "", q.dim).0 {
                    opt = Some(*fill);
                }
                yy += 32.0;
            }
            yy += 10.0;
        }
        if !by_player {
            caps(ui, Rect::new(v.x, yy, w, 16.0), "Which bus runs it", q.dim, Align::Left);
            yy += 22.0;
            for (n, (_, id, name, s, col)) in cands.iter().enumerate() {
                let rr = Rect::new(v.x, yy, w, 30.0);
                if pick_row(ui, &q, &format!("plan-bcand-{n}"), rr, id.is_some() && *id == current, None, name, s, *col).0 {
                    pick = Some(*id);
                }
                yy += 32.0;
            }
        }
        yy - v.y + 8.0
    });
    if let Some(b) = pick {
        give_bus(l, p, ti, b);
    }
    if let Some(fill) = opt {
        set_fill(l, key, fill);
    }
}
