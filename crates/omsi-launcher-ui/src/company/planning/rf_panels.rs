//! What lies over RosterFlow's Gantt: the cards a click opens (a trip, a vehicle, an event,
//! the stack's trip, a choice of several, a drop zone's ranking and cost difference), the
//! right-click menu, the batch toolbar of the ticked rows, the Validation panel (open and
//! dismissed issues, Filter Issues, Dismiss Issues), the duty panel (its type, lock, times,
//! paid time, signing, pieces, travel, breaks and custom work), and the small windows of the
//! toolbar, the title bar and the status bar (history, Stats, Layers, Filter, zoom, print,
//! the day, a line's colour, go to, the optimisation settings, a histogram).

use super::super::super::ui::{id_of, Key, Ui};
use super::super::super::Launcher;
use super::super::{day_label, eur};
use super::rf_gantt::{self as gantt, block_label, line_colour, Kind};
use super::rosterflow::{self as shell, rf_button, caps, edit, plain, Cached, Card, Drop, Filter, Hand, Pal, Panel, RowId, Stat};
use glam::Vec2;
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, Custom, EvKind, Scope, Travel};
use omsi_launcher_lib::company::studio::{self as st, Swap, Target};
use omsi_launcher_lib::company::{self as co, Cents, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

// --- small controls in RosterFlow's look -------------------------------------------------------

/// A switch with its words on the left.
pub(super) fn switch(ui: &mut Ui, p: &Pal, name: &str, r: Rect, on: bool, label: &str) -> bool {
    let (h, _, click) = ui.interact(id_of(name), r);
    let track = Rect::new(r.right() - 34.0, r.center().y - 9.0, 34.0, 18.0);
    ui.p().rounded(track, 9.0, if on { p.accent } else { p.faint.alpha(0.6) });
    let kx = if on { track.right() - 9.0 } else { track.x + 9.0 };
    ui.p().circle(Vec2::new(kx, track.center().y), 6.5, Color::WHITE);
    ui.text_in(label, Rect::new(r.x, r.y, r.w - 44.0, r.h), 13.0, Weight::Medium, if h { p.text } else { p.text.alpha(0.88) }, Align::Left);
    click
}

/// A checkbox with its words after it.
pub(super) fn check_row(ui: &mut Ui, p: &Pal, name: &str, r: Rect, on: bool, label: &str) -> bool {
    let (h, _, click) = ui.interact(id_of(name), r);
    let b = Rect::new(r.x, r.center().y - 7.0, 14.0, 14.0);
    if on {
        ui.p().rounded(b, 2.0, p.accent);
        ui.icon("check", b.center(), 12.0, Color::WHITE);
    } else {
        ui.p().rounded_border(b, 2.0, 1.3, if h { p.text } else { p.dim });
    }
    ui.text_in(label, Rect::new(r.x + 22.0, r.y, r.w - 22.0, r.h), 13.0, Weight::Medium, if h { p.text } else { p.text.alpha(0.88) }, Align::Left);
    click
}

/// A row of choices, one on.
pub(super) fn seg(ui: &mut Ui, p: &Pal, name: &str, r: Rect, at: usize, labels: &[&str]) -> Option<usize> {
    let w = r.w / labels.len().max(1) as f32;
    let mut out = None;
    ui.p().rounded_border(r, 3.0, 1.0, p.dim.alpha(0.6));
    for (k, label) in labels.iter().enumerate() {
        let cell = Rect::new(r.x + k as f32 * w, r.y, w, r.h);
        let (h, _, click) = ui.interact(id_of(&format!("{name}-{k}")), cell);
        if k == at {
            ui.p().rounded(cell.inset(1.0), 2.0, p.accent);
        } else if h {
            ui.p().rect(cell.inset(1.0), p.text.alpha(0.06));
        }
        let col = if k == at { Color::WHITE } else { p.text };
        let t = omsi_ui::tr(label).to_uppercase();
        ui.text_in(&t, cell.pad(4.0, 0.0), 11.0, Weight::Bold, col, Align::Center);
        if click && k != at {
            out = Some(k);
        }
    }
    out
}

/// A value with minus and plus beside it; `fmt` its words. Returns the new value when changed.
#[allow(clippy::too_many_arguments)]
pub(super) fn stepper(ui: &mut Ui, p: &Pal, name: &str, r: Rect, value: i32, step: i32, min: i32, max: i32, fmt: &dyn Fn(i32) -> String) -> Option<i32> {
    ui.p().rounded(r, 3.0, p.field);
    ui.p().rounded_border(r, 3.0, 1.0, p.line);
    let minus = Rect::new(r.x, r.y, 26.0, r.h);
    let plus = Rect::new(r.right() - 26.0, r.y, 26.0, r.h);
    let (hm, _, cm) = ui.interact(id_of(&format!("{name}-minus")), minus);
    let (hp, _, cp) = ui.interact(id_of(&format!("{name}-plus")), plus);
    ui.icon("remove", minus.center(), 15.0, if hm { p.accent } else { p.dim });
    ui.icon("add", plus.center(), 15.0, if hp { p.accent } else { p.dim });
    ui.text_as_is(&fmt(value), Vec2::new(r.center().x, r.center().y + 4.5), 13.0, Weight::Bold, p.text, Align::Center);
    // (the wheel over it turns it)
    let mut v = value;
    if r.contains(ui.input.mouse) && ui.input.wheel.y.abs() > 0.0 {
        v += ui.input.wheel.y.signum() as i32 * step;
        ui.input.wheel = Vec2::ZERO;
    }
    if cm {
        v -= step;
    }
    if cp {
        v += step;
    }
    let v = v.clamp(min, max);
    (v != value).then_some(v)
}

/// A line of a menu; returns whether it was clicked.
fn menu_item(ui: &mut Ui, p: &Pal, name: &str, r: Rect, label: &str, enabled: bool, tip: &str) -> bool {
    let (h, _, click) = ui.interact(id_of(name), r);
    if h && enabled {
        ui.p().rect(r, p.accent.alpha(0.16));
    }
    ui.text_in(label, Rect::new(r.x + 14.0, r.y, r.w - 20.0, r.h), 13.5, Weight::Medium, if enabled { p.menu_text } else { p.menu_text.alpha(0.4) }, Align::Left);
    if !tip.is_empty() {
        ui.tooltip(r, tip);
    }
    click && enabled
}

/// A sheet over the page: its shadow, its body, kept for the mouse (the Gantt under it gets
/// none where it lies).
fn sheet(l: &mut Launcher, r: Rect, fill: Color) {
    l.ui.p().shadow(r, 6.0, 18.0, Color::rgba(0, 0, 0, 0.35));
    l.ui.p().rounded(r, 4.0, fill);
    l.company.planning.rf.over.push(r);
}

/// A card's header: navy, its words white.
fn card_head(ui: &mut Ui, p: &Pal, r: Rect, colour: Color) {
    ui.push_clip(r, 0.0);
    ui.p().rounded(Rect::new(r.x, r.y, r.w, r.h + 6.0), 4.0, colour);
    ui.pop_clip();
    let _ = p;
}

/// A card's field: its caption in grey capitals, its value after it.
fn field(ui: &mut Ui, p: &Pal, x: f32, y: f32, w: f32, caption: &str, value: &str) {
    let t = omsi_ui::tr(caption).to_uppercase();
    ui.text_as_is(&format!("{t}:"), Vec2::new(x, y + 4.5), 11.0, Weight::Bold, p.card_dim, Align::Left);
    ui.text_in(value, Rect::new(x + 92.0, y - 8.0, w - 92.0, 18.0), 13.0, Weight::Medium, p.card_text, Align::Left);
}

/// "07:05 ── 45 min, 12.3 km ──> 07:50", the times in grey pills.
fn time_line(ui: &mut Ui, p: &Pal, x: f32, y: f32, w: f32, from: i32, to: i32, mid: &str) {
    let pill = |ui: &mut Ui, x: f32, t: i32| {
        let r = Rect::new(x, y - 10.0, 52.0, 20.0);
        ui.p().rounded(r, 10.0, p.card_pill);
        ui.text_as_is(&rf::clock(t), Vec2::new(r.center().x, y + 4.5), 12.0, Weight::Bold, p.card_text, Align::Center);
    };
    pill(ui, x, from);
    pill(ui, x + w - 52.0, to);
    let (a, b) = (x + 58.0, x + w - 58.0);
    ui.p().rect(Rect::new(a, y - 0.5, b - a, 1.0), p.card_dim);
    ui.p().convex(&[Vec2::new(b - 6.0, y - 4.0), Vec2::new(b, y), Vec2::new(b - 6.0, y + 4.0)], p.card_dim);
    let mw = ui.width(mid, 11.0, Weight::Medium) + 10.0;
    let mr = Rect::new((a + b) * 0.5 - mw * 0.5, y - 8.0, mw, 16.0);
    ui.p().rect(mr, p.card);
    ui.text_as_is(mid, Vec2::new(mr.center().x, y + 4.0), 11.0, Weight::Medium, p.card_dim, Align::Center);
}

/// A card placed by `at` (under it and to the right, kept in `room`).
fn place(at: Vec2, w: f32, h: f32, room: Rect) -> Rect {
    let x = (at.x - 30.0).clamp(room.x + 6.0, (room.right() - w - 6.0).max(room.x + 6.0));
    let y = if at.y + 16.0 + h < room.bottom() { at.y + 16.0 } else { (at.y - h - 16.0).max(room.y + 6.0) };
    Rect::new(x, y, w, h)
}

/// The duty a piece is in, as its row in the board and its label.
fn duty_of_piece(board: &Board, tour: usize, duty: usize) -> Option<(usize, String)> {
    let di = st::duty_of(&board.duties, tour, duty)?;
    let row = board.shaped.iter().position(|s| s.duty == Some(di))?;
    Some((row, board.shaped[row].label(row)))
}

fn piece_of(board: &Board, tour: usize, trip: usize) -> usize {
    co::network::duties_of(&board.tours[tour]).iter().position(|r| r.contains(&trip)).unwrap_or(0)
}

// --- the overlays -------------------------------------------------------------------------------

/// Everything over the Gantt this frame, the last drawn.
#[allow(clippy::too_many_arguments)]
pub(super) fn overlays(l: &mut Launcher, page: Rect, gantt_r: Rect, status: Rect, k: &Cached, board: &Board, p: &Pal, drivers: bool, date: &str) {
    l.company.planning.rf.over.clear();
    let fresh = l.company.planning.rf.opened_at == l.ui.time;
    let pressed = l.ui.input.pressed || l.ui.input.right_pressed;
    let mouse = l.ui.input.mouse;
    if !l.company.planning.rf.ticked.is_empty() {
        batch(l, gantt_r, board, p, drivers);
    }
    let panel = l.company.planning.rf.panel.clone();
    let mut keep: Vec<Rect> = Vec::new();
    match &panel {
        Panel::Issues(scope) => keep.push(issues(l, status, k, board, p, *scope, drivers)),
        Panel::Duty(id) => keep.push(duty_panel(l, gantt_r, k, board, p, id)),
        Panel::History => keep.push(history(l, page, p)),
        Panel::Stats => keep.push(stats(l, page, gantt_r, p, drivers)),
        Panel::Layers => keep.push(layers(l, gantt_r, p)),
        Panel::Filter => keep.push(filter(l, page, k, p)),
        Panel::Zoom => keep.push(zoom(l, status, p)),
        Panel::Print => keep.push(print_menu(l, page, k, board, p, date)),
        Panel::Day => keep.push(day_menu(l, page, k, p)),
        Panel::Colour(line) => keep.push(colour(l, page, k, p, line)),
        Panel::GoTo(text) => keep.push(go_to(l, page, k, board, p, text, drivers)),
        Panel::Optimise => keep.push(opt_settings(l, page, k, p)),
        Panel::Histogram(which) => keep.push(histogram_window(l, page, board, p, *which)),
        Panel::None => {}
    }
    if let Some(z) = l.company.planning.rf.zone_hover {
        zone_card(l, page, k, board, p, z);
    }
    let card = l.company.planning.rf.card.clone();
    if let Some(cd) = card {
        if let Some(r) = card_draw(l, page, k, board, p, &cd, drivers) {
            keep.push(r);
        }
    }
    let menu = l.company.planning.rf.menu.clone();
    if let Some(m) = menu {
        let r = menu_draw(l, page, k, board, p, &m, drivers);
        keep.push(r);
    }
    // a click elsewhere closes the card, the menu and the small windows (not the Validation
    // panel's or the duty panel's: they stay open while the Gantt is worked)
    if pressed && !fresh && !keep.iter().any(|r| r.contains(mouse)) {
        let s = &mut l.company.planning.rf;
        if s.menu.take().is_none() {
            s.card = None;
        }
        if !matches!(s.panel, Panel::Issues(_) | Panel::Duty(_) | Panel::Filter | Panel::Optimise | Panel::Histogram(_)) && !page.contains(mouse).then_some(()).is_none() {
            s.panel = Panel::None;
        }
    }
}

// --- the batch toolbar ------------------------------------------------------------------------

/// The ticked rows' tools over the Gantt's header: lock, unlock, unassign to the stack, and
/// how many are ticked.
fn batch(l: &mut Launcher, g: Rect, board: &Board, p: &Pal, drivers: bool) {
    let ui = &mut l.ui;
    let n = l.company.planning.rf.ticked.len();
    let words = omsi_ui::tr("%{n} rows selected").replace("%{n}", &n.to_string());
    let w = 34.0 * 3.0 + ui.width(&words, 12.5, Weight::Bold) + 60.0;
    let r = Rect::new(g.x + 28.0, g.y + 2.0, w, gantt::AXIS - 4.0);
    ui.p().rounded(r, 3.0, p.tool);
    l.company.planning.rf.over.push(r);
    let cy = r.center().y;
    let lock = shell::rf_icon(&mut l.ui, p, "rf-batch-lock", Vec2::new(r.x + 16.0, cy), "lock", "Lock the rows ticked", false, true);
    let unlock = shell::rf_icon(&mut l.ui, p, "rf-batch-unlock", Vec2::new(r.x + 48.0, cy), "livery_unlock", "Unlock the rows ticked", false, true);
    let stack = shell::rf_icon(&mut l.ui, p, "rf-batch-stack", Vec2::new(r.x + 80.0, cy), "inventory_2", "Unassign the ticked vehicles' trips to the stack", false, !drivers);
    let ui = &mut l.ui;
    ui.text_as_is(&words, Vec2::new(r.x + 104.0, cy + 4.5), 12.5, Weight::Bold, p.text, Align::Left);
    let xr = Rect::new(r.right() - 26.0, r.y, 24.0, r.h);
    let (h, _, close) = ui.interact(id_of("rf-batch-close"), xr);
    ui.icon("close", xr.center(), 15.0, if h { p.err } else { p.text });
    let ticked = l.company.planning.rf.ticked.clone();
    let wd = board.weekday;
    if close {
        l.company.planning.rf.ticked.clear();
    }
    if lock || unlock {
        let on = lock;
        let tours = board.tours.clone();
        let shaped = board.shaped.clone();
        let words = if on { omsi_ui::tr("%{n} rows locked") } else { omsi_ui::tr("%{n} rows unlocked") }.replace("%{n}", &ticked.len().to_string());
        edit(l, words, |c| {
            for id in &ticked {
                match id {
                    RowId::Block(line, tour) => rf::set_block_lock(c, wd, line, tour, on),
                    RowId::Duty(_) => {
                        if let Some(s) = shaped.iter().find(|s| shell::duty_row(s) == *id) {
                            match (&s.key, s.spare) {
                                (Some(key), _) => rf::set_duty_lock(c, wd, key, on),
                                (None, Some(sid)) => {
                                    if let Some(sp) = c.planning.flow.sheet.spares.iter_mut().find(|x| x.weekday == wd && x.id == sid) {
                                        sp.locked = on;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            let _ = &tours;
            Ok(())
        });
        l.company.planning.rf.ticked = ticked.clone();
    }
    if stack {
        let tours = board.tours.clone();
        let words = omsi_ui::tr("%{n} vehicles unassigned to the stack").replace("%{n}", &ticked.len().to_string());
        edit(l, words, |c| {
            // (one bus at a time, each on the day as it then is)
            for id in &ticked {
                if let RowId::Block(line, tour) = id {
                    let Some(i) = tours.iter().position(|t| t.line == *line && t.tour == *tour) else { continue };
                    let trips: Vec<usize> = (0..tours[i].trips.len()).filter(|&j| tours[i].trips[j].counts() && !tours[i].trips[j].origin.trim().is_empty()).collect();
                    let mut lay_c = c.clone();
                    if st::unassign(&mut lay_c, wd, &tours, i, &trips).is_ok() {
                        // (the stack of every line kept; the blocks of the others as they were)
                        *c = lay_c;
                    }
                }
            }
            Ok(())
        });
        l.company.planning.rf.ticked.clear();
    }
}

// --- the cards -----------------------------------------------------------------------------------

/// The card open (a trip, a vehicle, an event, the stack's trip, the choice); returns where it
/// lies.
fn card_draw(l: &mut Launcher, page: Rect, k: &Cached, board: &Board, p: &Pal, cd: &Card, drivers: bool) -> Option<Rect> {
    let c: &Company = &k.view;
    let wd = board.weekday;
    match cd.clone() {
        Card::Trip { tour, trip, at, assign } => {
            let t = board.tours.get(tour)?;
            let x = t.trips.get(trip)?.clone();
            let candidates: Vec<usize> = if assign { (0..board.tours.len()).filter(|&i| i != tour && board.tours[i].line.eq_ignore_ascii_case(&t.line) && st::plan_move(c, wd, &board.tours, tour, trip, Target::Block(i)).is_ok()).collect() } else { Vec::new() };
            let h = 300.0 + if assign { 26.0 * (candidates.len() + 1) as f32 + 10.0 } else { 0.0 };
            let r = place(at, 410.0, h, page);
            sheet(l, r, p.card);
            let head = Rect::new(r.x, r.y, r.w, 42.0);
            card_head(&mut l.ui, p, head, p.card_head);
            let ui = &mut l.ui;
            let piece = piece_of(board, tour, trip);
            let duty = duty_of_piece(board, tour, piece);
            let mut go_duty = None;
            if let Some((row, label)) = &duty {
                ui.icon("person", Vec2::new(r.x + 20.0, head.center().y), 18.0, Color::WHITE);
                let w = ui.text_as_is(label, Vec2::new(r.x + 36.0, head.center().y + 5.0), 15.0, Weight::Bold, Color::WHITE, Align::Left);
                ui.p().rect(Rect::new(r.x + 36.0, head.center().y + 8.0, w, 1.0), Color::WHITE.alpha(0.7));
                let (_, _, click) = ui.interact(id_of("rf-card-duty"), Rect::new(r.x + 10.0, head.y, w + 30.0, head.h));
                if click {
                    go_duty = Some(*row);
                }
            }
            let vlabel = block_label(t);
            let vw = ui.width(&vlabel, 15.0, Weight::Bold);
            let vx = r.right() - 40.0 - vw;
            ui.icon("directions_bus", Vec2::new(vx - 14.0, head.center().y), 18.0, Color::WHITE);
            let (vh, _, vclick) = ui.interact(id_of("rf-card-vehicle"), Rect::new(vx - 26.0, head.y, vw + 30.0, head.h));
            ui.text_as_is(&vlabel, Vec2::new(vx, head.center().y + 5.0), 15.0, Weight::Bold, Color::WHITE, Align::Left);
            if vh {
                ui.p().rect(Rect::new(vx, head.center().y + 8.0, vw, 1.0), Color::WHITE);
            }
            ui.tooltip(Rect::new(vx - 26.0, head.y, vw + 30.0, head.h), "Assign the trip to another vehicle");
            let (bh, _, bclick) = ui.interact(id_of("rf-card-colour"), Rect::new(r.right() - 34.0, head.y + 6.0, 28.0, 30.0));
            ui.icon("palette", Vec2::new(r.right() - 20.0, head.center().y), 18.0, if bh { p.warn } else { Color::WHITE });
            ui.tooltip(Rect::new(r.right() - 34.0, head.y + 6.0, 28.0, 30.0), "The line's colour");
            // the body
            let col = r.w * 0.5;
            let mut y = head.bottom() + 22.0;
            let cl = c.lines.iter().find(|x2| x2.name.eq_ignore_ascii_case(&t.line));
            field(ui, p, r.x + 18.0, y, col, "Route", &format!("{}  {}", t.number, cl.map(|x2| x2.caption.clone()).unwrap_or_default()));
            field(ui, p, r.x + col + 8.0, y, col - 26.0, "Dir", if gantt::outbound(&board.tours, &t.line, &x) { "A" } else { "B" });
            y += 26.0;
            field(ui, p, r.x + 18.0, y, col, "Trip ID", &x.name);
            field(ui, p, r.x + col + 8.0, y, col - 26.0, "Service ID", &omsi_ui::tr(shell::DAY_NAMES[wd as usize % 7]));
            y += 26.0;
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "Timetable", &omsi_ui::tr("Tour %{t}").replace("%{t}", x.origin.trim()));
            y += 18.0;
            ui.p().rect(Rect::new(r.x + 16.0, y, r.w - 32.0, 1.0), p.card_pill);
            y += 20.0;
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "From", &x.from);
            y += 26.0;
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "To", x.end());
            y += 18.0;
            ui.p().rect(Rect::new(r.x + 16.0, y, r.w - 32.0, 1.0), p.card_pill);
            y += 22.0;
            let t_cap = omsi_ui::tr("Time").to_uppercase();
            ui.text_as_is(&format!("{t_cap}:"), Vec2::new(r.x + 18.0, y + 4.5), 11.0, Weight::Bold, p.card_dim, Align::Left);
            let mid = format!("{} min, {:.1} km", x.arr - x.dep, x.km);
            time_line(ui, p, r.x + 76.0, y, r.w - 94.0, x.dep, x.arr, &mid);
            y += 20.0;
            // the quick assign list
            let mut assign_to: Option<Option<usize>> = None;
            if assign {
                ui.p().rect(Rect::new(r.x + 16.0, y, r.w - 32.0, 1.0), p.card_pill);
                y += 6.0;
                for (n, &i) in candidates.iter().enumerate() {
                    let rr = Rect::new(r.x + 14.0, y + n as f32 * 26.0, r.w - 28.0, 24.0);
                    let (h, _, click) = ui.interact(id_of(&format!("rf-assign-{i}")), rr);
                    if h {
                        ui.p().rect(rr, p.accent.alpha(0.14));
                    }
                    ui.icon("directions_bus", Vec2::new(rr.x + 12.0, rr.center().y), 15.0, p.card_dim);
                    ui.text_as_is(&block_label(&board.tours[i]), Vec2::new(rr.x + 26.0, rr.center().y + 4.5), 13.0, Weight::Bold, p.card_text, Align::Left);
                    if click {
                        assign_to = Some(Some(i));
                    }
                }
                let rr = Rect::new(r.x + 14.0, y + candidates.len() as f32 * 26.0, r.w - 28.0, 24.0);
                let (h, _, click) = ui.interact(id_of("rf-assign-new"), rr);
                if h {
                    ui.p().rect(rr, p.accent.alpha(0.14));
                }
                ui.icon("add", Vec2::new(rr.x + 12.0, rr.center().y), 15.0, p.accent);
                ui.text_in("New vehicle", Rect::new(rr.x + 26.0, rr.y, rr.w - 26.0, rr.h), 13.0, Weight::Bold, p.accent, Align::Left);
                if click {
                    assign_to = Some(None);
                }
            }
            // the footer: unassign, link, lock; MOVE and the timeplan
            let fy = r.bottom() - 44.0;
            ui.p().rect(Rect::new(r.x + 16.0, fy - 6.0, r.w - 32.0, 1.0), p.card_pill);
            let un = card_icon(ui, p, "rf-card-unassign", Vec2::new(r.x + 30.0, fy + 18.0), "inventory_2", "Unassign: the trip waits on the stack", !drivers);
            let linked = !rf::linked(c, wd, &t.line, &st::TripRef::of(&x)).is_empty();
            let link = card_icon(ui, p, "rf-card-link", Vec2::new(r.x + 66.0, fy + 18.0), "link", if linked { "Unlink: the trips need no longer stay together" } else { "Link it with the trips chosen (Shift and a click)" }, linked);
            let tp = rf_button(ui, p, "rf-card-timeplan", Rect::new(r.right() - 238.0, fy + 4.0, 110.0, 30.0), "Timeplan", false, true);
            let mv = rf_button(ui, p, "rf-card-move", Rect::new(r.right() - 120.0, fy + 4.0, 104.0, 30.0), "Move", true, true);
            // what was clicked
            let line = t.line.clone();
            let tours = board.tours.clone();
            if let Some(row) = go_duty {
                let id = shell::duty_row(&board.shaped[row]);
                let s = &mut l.company.planning.rf;
                l.company.planning.view = shell::DRIVERS;
                s.panel = Panel::Duty(id.clone());
                s.reveal = Some(id.clone());
                s.sel_row = Some(id);
                s.card = None;
            }
            if vclick && !drivers {
                if let Some(Card::Trip { assign, .. }) = l.company.planning.rf.card.as_mut() {
                    *assign = !*assign;
                }
            }
            if bclick {
                l.company.planning.rf.panel = Panel::Colour(line.clone());
            }
            if let Some(to) = assign_to {
                let target = to.map(Target::Block).unwrap_or(Target::New);
                let words = match to {
                    Some(i) => omsi_ui::tr("%{n} trips moved from %{a} to %{b}").replace("%{n}", "1").replace("%{a}", &block_label(&tours[tour])).replace("%{b}", &block_label(&tours[i])),
                    None => omsi_ui::tr("%{n} trips moved from %{a} to a new vehicle").replace("%{n}", "1").replace("%{a}", &block_label(&tours[tour])),
                };
                edit(l, words, |c| st::move_trip(c, wd, &tours, tour, trip, target));
            }
            if un {
                let words = omsi_ui::tr("%{n} trips of %{a} unassigned to the stack").replace("%{n}", "1").replace("%{a}", &block_label(&tours[tour]));
                edit(l, words, |c| st::unassign(c, wd, &tours, tour, &[trip]));
            }
            if link {
                link_or_unlink(l, board, tour, trip, linked);
            }
            if tp {
                l.company.planning.rf.timeplan_line = Some(line);
                l.company.planning.view = shell::TIMEPLAN;
                l.company.planning.rf.settle();
            }
            if mv {
                let h = if drivers { Hand::Piece { tour, duty: piece } } else { Hand::Trips { tour, trips: vec![trip] } };
                gantt::pick(l, k, board, h, false);
                l.company.planning.rf.card = None;
            }
            Some(r)
        }
        Card::Vehicle { tour, at } => {
            let t = board.tours.get(tour)?.clone();
            let r = place(at, 380.0, 236.0, page);
            sheet(l, r, p.card);
            let head = Rect::new(r.x, r.y, r.w, 42.0);
            card_head(&mut l.ui, p, head, p.warn.darken(0.05));
            let ui = &mut l.ui;
            let number_line = omsi_ui::tr("Line %{n}").replace("%{n}", &t.number);
            ui.icon("directions_bus", Vec2::new(r.x + 20.0, head.center().y), 18.0, Color::WHITE);
            ui.text_as_is(&format!("{}  /  {}", block_label(&t), number_line), Vec2::new(r.x + 38.0, head.center().y + 5.0), 15.0, Weight::Bold, Color::WHITE, Align::Left);
            let mut y = head.bottom() + 22.0;
            let first = t.trips.first().cloned().unwrap_or_default();
            let last = t.trips.last().cloned().unwrap_or_default();
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "From", &first.from);
            y += 26.0;
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "To", last.end());
            y += 30.0;
            let mid = format!("{}, {:.1} km", rf::hm(t.to() - t.from()), t.km());
            time_line(ui, p, r.x + 18.0, y, r.w - 36.0, t.from(), t.to(), &mid);
            y += 28.0;
            let n_trips = t.trips.iter().filter(|x| x.counts()).count();
            let n_duties = co::network::duties_of(&t).len();
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "Trips", &format!("{n_trips}  ·  {} {n_duties}", omsi_ui::tr("duties")));
            let fy = r.bottom() - 44.0;
            ui.p().rect(Rect::new(r.x + 16.0, fy - 6.0, r.w - 32.0, 1.0), p.card_pill);
            let locked = rf::block_locked(c, wd, &t.line, &t.tour);
            let lock = card_icon(ui, p, "rf-vcard-lock", Vec2::new(r.x + 30.0, fy + 18.0), if locked { "lock" } else { "livery_unlock" }, if locked { "Unlock the vehicle" } else { "Lock the vehicle: Optimise leaves it" }, locked);
            let paint = card_icon(ui, p, "rf-vcard-colour", Vec2::new(r.x + 66.0, fy + 18.0), "palette", "The line's colour", false);
            let crew = rf_button(ui, p, "rf-vcard-crew", Rect::new(r.right() - 196.0, fy + 4.0, 180.0, 30.0), "View in crew schedule", false, true);
            if lock {
                gantt::toggle_lock(l, board, Kind::Block(tour));
            }
            if paint {
                l.company.planning.rf.panel = Panel::Colour(t.line.clone());
            }
            if crew {
                if let Some((row, _)) = duty_of_piece(board, tour, 0) {
                    let id = shell::duty_row(&board.shaped[row]);
                    l.company.planning.view = shell::DRIVERS;
                    let s = &mut l.company.planning.rf;
                    s.settle();
                    s.reveal = Some(id.clone());
                    s.sel_row = Some(id);
                }
            }
            Some(r)
        }
        Card::Event { row, ev, at } => {
            let s = board.shaped.get(row)?.clone();
            let e = *s.events.get(ev)?;
            let r = place(at, 330.0, 190.0, page);
            sheet(l, r, p.card);
            let head = Rect::new(r.x, r.y, r.w, 40.0);
            card_head(&mut l.ui, p, head, p.card_head);
            let ui = &mut l.ui;
            let kind = match e.kind {
                EvKind::SignOn => omsi_ui::tr("Sign-on").into_owned(),
                EvKind::SignOff => omsi_ui::tr("Sign-off").into_owned(),
                EvKind::Travel(m) => format!("{} ({})", omsi_ui::tr("Travel"), omsi_ui::tr(m.label())),
                EvKind::Custom(ci) => s.custom.get(ci).map(|x| omsi_ui::tr(&x.kind).into_owned()).unwrap_or_default(),
            };
            ui.icon("person", Vec2::new(r.x + 20.0, head.center().y), 17.0, Color::WHITE);
            ui.text_as_is(&format!("{}  ·  {}", s.label(row), kind), Vec2::new(r.x + 36.0, head.center().y + 5.0), 14.0, Weight::Bold, Color::WHITE, Align::Left);
            let mut y = head.bottom() + 24.0;
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "Type", &kind);
            y += 30.0;
            time_line(ui, p, r.x + 18.0, y, r.w - 36.0, e.from, e.to, &format!("{} min", e.to - e.from));
            y += 28.0;
            let mut pick_mode = None;
            let mut delete = false;
            if let EvKind::Travel(m) = e.kind {
                let at_k = Travel::ALL.iter().position(|x| *x == m).unwrap_or(0);
                pick_mode = seg(ui, p, "rf-ev-mode", Rect::new(r.x + 18.0, y, r.w - 36.0, 28.0), at_k, &Travel::LABELS);
            }
            if let EvKind::Custom(_) = e.kind {
                delete = rf_button(ui, p, "rf-ev-delete", Rect::new(r.x + 18.0, y, 120.0, 28.0), "Delete", false, true);
            }
            let wd2 = wd;
            if let Some(mk) = pick_mode {
                // (the gap: the piece before the travel)
                let gap = s.spans.iter().rposition(|sp| sp.to <= e.from).unwrap_or(0) as u32;
                if let Some(key) = s.key.clone() {
                    let words = omsi_ui::tr("Travel of duty %{d} by %{mode}").replace("%{d}", &s.label(row)).replace("%{mode}", &omsi_ui::tr(Travel::ALL[mk].label()));
                    edit(l, words, |c| {
                        let n = rf::note_mut(c, wd2, &key);
                        n.travel.retain(|x| x.0 != gap);
                        n.travel.push((gap, Travel::ALL[mk]));
                        Ok(())
                    });
                }
            }
            if delete {
                if let EvKind::Custom(ci) = e.kind {
                    delete_custom(l, &s, row, ci, wd2);
                }
            }
            Some(r)
        }
        Card::Stack { k: sk, at } => {
            let (line, x) = board.stack.get(sk)?.clone();
            let r = place(at, 360.0, 170.0, page);
            sheet(l, r, p.card);
            let head = Rect::new(r.x, r.y, r.w, 40.0);
            card_head(&mut l.ui, p, head, p.card_head);
            let ui = &mut l.ui;
            ui.icon("inventory_2", Vec2::new(r.x + 20.0, head.center().y), 17.0, Color::WHITE);
            let words = omsi_ui::tr("Unassigned trip").into_owned();
            ui.text_in(&words, Rect::new(r.x + 36.0, head.y, r.w - 50.0, head.h), 14.0, Weight::Bold, Color::WHITE, Align::Left);
            let mut y = head.bottom() + 22.0;
            let number = c.lines.iter().find(|l2| l2.name.eq_ignore_ascii_case(&line)).map(|l2| l2.number.clone()).unwrap_or_default();
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "Route", &number);
            y += 26.0;
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "From", &format!("{} → {}", x.from, x.end()));
            y += 30.0;
            time_line(ui, p, r.x + 18.0, y, r.w - 36.0, x.dep, x.arr, &format!("{} min", x.arr - x.dep));
            y += 24.0;
            ui.text_in("Click a drop zone to assign it to a vehicle", Rect::new(r.x + 18.0, y, r.w - 36.0, 18.0), 11.5, Weight::Regular, p.card_dim, Align::Left);
            Some(r)
        }
        Card::Selection { at } => {
            let sel = l.company.planning.rf.sel.clone();
            let &(tour, _) = sel.first()?;
            let t = board.tours.get(tour)?;
            let xs: Vec<&co::network::PlannedTrip> = sel.iter().filter_map(|&(a, b)| board.tours.get(a).and_then(|t2| t2.trips.get(b))).collect();
            let from = xs.iter().map(|x| x.dep).min()?;
            let to = xs.iter().map(|x| x.arr).max()?;
            let driving: i32 = xs.iter().map(|x| x.arr - x.dep).sum();
            let r = place(at, 380.0, 196.0, page);
            sheet(l, r, p.card);
            let head = Rect::new(r.x, r.y, r.w, 40.0);
            card_head(&mut l.ui, p, head, p.card_head);
            let ui = &mut l.ui;
            ui.icon("directions_bus", Vec2::new(r.x + 20.0, head.center().y), 17.0, Color::WHITE);
            let words = omsi_ui::tr("%{n} trips chosen").replace("%{n}", &xs.len().to_string());
            ui.text_as_is(&format!("{}  ·  {}", block_label(t), words), Vec2::new(r.x + 36.0, head.center().y + 5.0), 14.0, Weight::Bold, Color::WHITE, Align::Left);
            let mut y = head.bottom() + 22.0;
            field(ui, p, r.x + 18.0, y, r.w - 36.0, "Driving time", &rf::hm(driving));
            y += 30.0;
            time_line(ui, p, r.x + 18.0, y, r.w - 36.0, from, to, &format!("{} {}", omsi_ui::tr("span"), rf::hm(to - from)));
            let fy = r.bottom() - 44.0;
            ui.p().rect(Rect::new(r.x + 16.0, fy - 6.0, r.w - 32.0, 1.0), p.card_pill);
            let link = rf_button(ui, p, "rf-sel-link", Rect::new(r.x + 16.0, fy + 4.0, 100.0, 30.0), "Link", false, !drivers && xs.len() > 1);
            let mvw = format!("{} {}", omsi_ui::tr("Move"), xs.len());
            let mv = rf_button(ui, p, "rf-sel-move", Rect::new(r.right() - 140.0, fy + 4.0, 124.0, 30.0), &mvw, true, !drivers);
            let trips: Vec<usize> = sel.iter().filter(|s| s.0 == tour).map(|s| s.1).collect();
            if link {
                let tours = board.tours.clone();
                let words = omsi_ui::tr("%{n} trips linked on %{v}").replace("%{n}", &trips.len().to_string()).replace("%{v}", &block_label(&tours[tour]));
                edit(l, words, |c| rf::link(c, wd, &tours, tour, &trips));
            }
            if mv {
                gantt::pick(l, k, board, Hand::Trips { tour, trips }, false);
                l.company.planning.rf.card = None;
            }
            Some(r)
        }

    }
}

/// A round icon button on a card.
fn card_icon(ui: &mut Ui, p: &Pal, name: &str, c: Vec2, icon: &str, tip: &str, on: bool) -> bool {
    let r = Rect::new(c.x - 15.0, c.y - 15.0, 30.0, 30.0);
    let (h, _, click) = ui.interact(id_of(name), r);
    ui.p().rounded(r, 4.0, if on { p.accent } else if h { p.card_dim } else { p.card_dim.alpha(0.75) });
    ui.icon(icon, c, 17.0, Color::WHITE);
    ui.tooltip(r, tip);
    click
}

/// Link the trip with the trips chosen, or take its link apart.
fn link_or_unlink(l: &mut Launcher, board: &Board, tour: usize, trip: usize, linked: bool) {
    let wd = board.weekday;
    let tours = board.tours.clone();
    if linked {
        let words = omsi_ui::tr("Link on %{v} taken apart").replace("%{v}", &block_label(&tours[tour]));
        edit(l, words, |c| {
            rf::unlink(c, wd, &tours, tour, trip);
            Ok(())
        });
    } else {
        let mut trips: Vec<usize> = l.company.planning.rf.sel.iter().filter(|s| s.0 == tour).map(|s| s.1).collect();
        if !trips.contains(&trip) {
            trips.push(trip);
        }
        let words = omsi_ui::tr("%{n} trips linked on %{v}").replace("%{n}", &trips.len().to_string()).replace("%{v}", &block_label(&tours[tour]));
        edit(l, words, |c| rf::link(c, wd, &tours, tour, &trips));
    }
}

/// Delete custom work `ci` of duty `s` (a spare duty without any left goes).
fn delete_custom(l: &mut Launcher, s: &rf::Shaped, row: usize, ci: usize, wd: u8) {
    let words = omsi_ui::tr("Custom work deleted from duty %{d}").replace("%{d}", &s.label(row));
    let (key, spare) = (s.key.clone(), s.spare);
    edit(l, words, |c| {
        match (key, spare) {
            (Some(key), _) => {
                let n = rf::note_mut(c, wd, &key);
                if ci < n.custom.len() {
                    n.custom.remove(ci);
                }
                rf::tidy_notes(c);
            }
            (None, Some(id)) => {
                if let Some(sp) = c.planning.flow.sheet.spares.iter_mut().find(|x| x.weekday == wd && x.id == id) {
                    if ci < sp.custom.len() {
                        sp.custom.remove(ci);
                    }
                }
                c.planning.flow.sheet.spares.retain(|x| !(x.weekday == wd && x.custom.is_empty()));
            }
            _ => {}
        }
        Ok(())
    });
}

/// The ranking of the drop zone under the mouse (Optibus's recommendation tooltip): its stars,
/// the cost difference, and whether it breaks a rule.
fn zone_card(l: &mut Launcher, page: Rect, k: &Cached, board: &Board, p: &Pal, zi: usize) {
    let Some(z) = l.company.planning.rf.zones.as_ref().and_then(|zs| zs.get(zi)).cloned() else { return };
    let _ = k;
    let at = l.ui.input.mouse;
    let r = place(at, 330.0, 150.0, page);
    l.ui.p().shadow(r, 6.0, 18.0, Color::rgba(0, 0, 0, 0.35));
    l.ui.p().rounded(r, 4.0, p.card);
    let ui = &mut l.ui;
    let head = Rect::new(r.x, r.y, r.w, 38.0);
    card_head(ui, p, head, p.card_head);
    let target = match z.drop {
        Drop::Block(i) => board.tours.get(i).map(block_label).unwrap_or_default(),
        Drop::Duty(d) => board.shaped.get(d).map(|s| s.label(d)).unwrap_or_default(),
        Drop::New => omsi_ui::tr("New vehicle").into_owned(),
        Drop::NewDuty => omsi_ui::tr("New duty").into_owned(),
        Drop::Stack => omsi_ui::tr("Stack").into_owned(),
    };
    ui.icon(if matches!(z.drop, Drop::Duty(_) | Drop::NewDuty) { "person" } else { "directions_bus" }, Vec2::new(r.x + 20.0, head.center().y), 17.0, Color::WHITE);
    ui.text_as_is(&target, Vec2::new(r.x + 36.0, head.center().y + 5.0), 14.0, Weight::Bold, Color::WHITE, Align::Left);
    let half = (r.w - 36.0) * 0.5;
    let a = Rect::new(r.x + 12.0, head.bottom() + 10.0, half, 60.0);
    let b = Rect::new(a.right() + 12.0, a.y, half, 60.0);
    ui.p().rounded(a, 3.0, p.card_pill.alpha(0.6));
    ui.p().rounded(b, 3.0, p.card_pill.alpha(0.6));
    ui.text_in("Ranking", Rect::new(a.x, a.y + 4.0, a.w, 20.0), 12.0, Weight::Medium, p.card_dim, Align::Center);
    for s in 0..5 {
        let c = Vec2::new(a.center().x - 40.0 + s as f32 * 20.0, a.y + 40.0);
        ui.icon("star", c, 18.0, if (s as u8) < z.stars { p.accent } else { p.card_dim.alpha(0.35) });
    }
    ui.text_in("Cost difference", Rect::new(b.x, b.y + 4.0, b.w, 20.0), 12.0, Weight::Medium, p.card_dim, Align::Center);
    let sign = if z.delta > 0 { "+ " } else if z.delta < 0 { "- " } else { "" };
    let words = format!("{sign}{}", eur(z.delta.abs()));
    ui.text_as_is(&words, Vec2::new(b.center().x, b.y + 46.0), 17.0, Weight::Bold, if z.delta > 0 { p.err } else { p.ok }, Align::Center);
    let note = if z.clean { "Breaks no rule: click to drop it here" } else { "Breaks a rule: it can still be dropped here" };
    ui.text_in(note, Rect::new(r.x + 14.0, r.bottom() - 30.0, r.w - 28.0, 22.0), 12.0, Weight::Medium, if z.clean { p.card_text } else { p.err }, Align::Left);
}

// --- the right-click menu -------------------------------------------------------------------

fn menu_draw(l: &mut Launcher, page: Rect, k: &Cached, board: &Board, p: &Pal, m: &shell::Menu, drivers: bool) -> Rect {
    let c: &Company = &k.view;
    let wd = board.weekday;
    // the items: (label, what, enabled, tip)
    let mut items: Vec<(&str, u8, bool, &str)> = Vec::new();
    if let Some((row, _ci)) = m.custom {
        items.push(("Edit", 20, true, ""));
        items.push(("Delete", 21, true, ""));
        let _ = row;
    } else if drivers {
        let piece_locked = m.piece.is_some_and(|(t, d)| rf::piece_locked(c, wd, &board.tours, t, d));
        items.push(("Move", 1, !piece_locked, ""));
        items.push(("Move to new row", 2, !piece_locked && m.duty_row.and_then(|r| board.shaped.get(r)).is_some_and(|s| s.spans.len() > 1), "The piece a duty of its own"));
        items.push(("Lock duty", 9, true, ""));
        items.push(("Add custom work", 22, true, "Spare, office, union or training time in the duty"));
        items.push(("—", 0, false, ""));
        items.push(("View in vehicle schedule", 23, true, ""));
        items.push(("View in timeplan", 7, true, ""));
        items.push(("Delete", 8, false, "Trips are deleted in the timetable"));
    } else {
        let t = &board.tours[m.tour];
        let locked = rf::block_locked(c, wd, &t.line, &t.tour);
        let linked = t.trips.get(m.trip).is_some_and(|x| !rf::linked(c, wd, &t.line, &st::TripRef::of(x)).is_empty());
        items.push(("Move", 1, !locked, ""));
        items.push(("Move to new row", 2, !locked, ""));
        items.push(("Unassign", 3, !locked, "The trip waits on the stack"));
        items.push(("Swap", 4, !locked, "Swap the trips chosen with what another bus runs over their time: click the bus next"));
        items.push(("Swap from here", 5, !locked, "Swap everything from this trip on with another bus's day from then on: click the bus next"));
        items.push((if linked { "Unlink" } else { "Link" }, 6, !locked, ""));
        items.push((if locked { "Unlock vehicle" } else { "Lock vehicle" }, 9, true, ""));
        items.push(("Line colour…", 10, true, ""));
        items.push(("—", 0, false, ""));
        items.push(("View in timeplan", 7, true, ""));
        items.push(("Delete", 8, false, "Trips are deleted in the timetable"));
    }
    let h = items.iter().map(|i| if i.0 == "—" { 9.0 } else { 30.0 }).sum::<f32>() + 10.0;
    let r = Rect::new(m.at.x.min(page.right() - 230.0), if m.at.y + h < page.bottom() { m.at.y } else { m.at.y - h }, 220.0, h);
    sheet(l, r, p.menu);
    let mut y = r.y + 5.0;
    let mut chosen = None;
    for (label, what, enabled, tip) in items {
        if label == "—" {
            l.ui.p().rect(Rect::new(r.x + 10.0, y + 4.0, r.w - 20.0, 1.0), p.menu_text.alpha(0.15));
            y += 9.0;
            continue;
        }
        if menu_item(&mut l.ui, p, &format!("rf-menu-{what}"), Rect::new(r.x, y, r.w, 30.0), label, enabled, tip) {
            chosen = Some(what);
        }
        y += 30.0;
    }
    let Some(what) = chosen else { return r };
    l.company.planning.rf.menu = None;
    let tours = board.tours.clone();
    let trips: Vec<usize> = {
        let sel = &l.company.planning.rf.sel;
        let mine: Vec<usize> = sel.iter().filter(|s| s.0 == m.tour).map(|s| s.1).collect();
        if mine.is_empty() {
            vec![m.trip]
        } else {
            mine
        }
    };
    match what {
        1 => {
            let h = match m.piece {
                Some((t, d)) => Hand::Piece { tour: t, duty: d },
                None => Hand::Trips { tour: m.tour, trips },
            };
            gantt::pick(l, k, board, h, false);
        }
        2 => match m.piece {
            Some((t, d)) => {
                let words = omsi_ui::tr("Piece of %{v} made a duty of its own").replace("%{v}", &block_label(&tours[t]));
                edit(l, words, |c| st::detach(c, wd, &tours, (t, d)));
            }
            None => {
                let words = omsi_ui::tr("%{n} trips moved from %{a} to a new vehicle").replace("%{n}", &trips.len().to_string()).replace("%{a}", &block_label(&tours[m.tour]));
                edit(l, words, |c| st::move_trips(c, wd, &tours, m.tour, &trips, Target::New));
            }
        },
        3 => {
            let words = omsi_ui::tr("%{n} trips of %{a} unassigned to the stack").replace("%{n}", &trips.len().to_string()).replace("%{a}", &block_label(&tours[m.tour]));
            edit(l, words, |c| st::unassign(c, wd, &tours, m.tour, &trips));
        }
        4 | 5 => {
            let how = if what == 4 { Swap::Span } else { Swap::FromHere };
            let s = &mut l.company.planning.rf;
            s.hand = Some(Hand::Swap { tour: m.tour, trips, how });
            s.zones = None;
            l.state.set_status(omsi_ui::tr("Click the bus to swap with (Escape: no swap).").into_owned(), false);
        }
        6 => {
            let linked = tours[m.tour].trips.get(m.trip).is_some_and(|x| !rf::linked(c, wd, &tours[m.tour].line, &st::TripRef::of(x)).is_empty());
            link_or_unlink(l, board, m.tour, m.trip, linked);
        }
        7 => {
            let line = if m.tour < tours.len() { Some(tours[m.tour].line.clone()) } else { None };
            l.company.planning.rf.timeplan_line = line;
            l.company.planning.view = shell::TIMEPLAN;
            l.company.planning.rf.settle();
        }
        9 => match m.duty_row {
            Some(d) => gantt::toggle_lock(l, board, Kind::Duty(d)),
            None => gantt::toggle_lock(l, board, Kind::Block(m.tour)),
        },
        10 => l.company.planning.rf.panel = Panel::Colour(tours[m.tour].line.clone()),
        20 => {
            if let Some((row, _)) = m.custom {
                let id = shell::duty_row(&board.shaped[row]);
                l.company.planning.rf.panel = Panel::Duty(id);
            }
        }
        21 => {
            if let Some((row, ci)) = m.custom {
                let s = board.shaped[row].clone();
                delete_custom(l, &s, row, ci, wd);
            }
        }
        22 => {
            if let Some(row) = m.duty_row {
                let id = shell::duty_row(&board.shaped[row]);
                l.company.planning.rf.panel = Panel::Duty(id);
            }
        }
        23 => {
            let id = shell::block_row(&tours[m.tour]);
            l.company.planning.view = shell::VEHICLES;
            let s = &mut l.company.planning.rf;
            s.settle();
            s.reveal = Some(id.clone());
            s.sel_row = Some(id);
        }
        _ => {}
    }
    r
}

// --- the Validation panel --------------------------------------------------------------------------

fn scope_title(s: Scope) -> &'static str {
    match s {
        Scope::Global => "Global Issues",
        Scope::Duty => "Duty Issues",
        Scope::Vehicle => "Vehicle Issues",
    }
}

/// The row an issue is about, in words ("Duty 1003", "Vehicle 1/02").
fn issue_row_words(board: &Board, i: &rf::Issue) -> String {
    match (i.scope, i.row) {
        (Scope::Duty, Some(r)) => board.shaped.get(r).map(|s| format!("{} {}", omsi_ui::tr("Duty"), s.label(r))).unwrap_or_default(),
        (Scope::Vehicle, Some(r)) => board.tours.get(r).map(|t| format!("{} {}", omsi_ui::tr("Vehicle"), block_label(t))).unwrap_or_default(),
        _ => String::new(),
    }
}

/// Optibus's Validation panel above the status bar: the issues of a kind, open or dismissed,
/// grouped where several say the same; ticked ones filtered to or dismissed.
fn issues(l: &mut Launcher, status: Rect, k: &Cached, board: &Board, p: &Pal, scope: Scope, drivers: bool) -> Rect {
    let c: &Company = &k.view;
    let wd = board.weekday;
    let dismissed_tab = l.company.planning.rf.dismissed_tab;
    let list = board.open(c, scope, dismissed_tab);
    let w = 470.0;
    let h = 470.0f32.min(status.y - 120.0);
    let r = Rect::new(status.right() - w - 150.0, status.y - h - 8.0, w, h);
    super::super::super::tour::anchor("rf-issues-panel", r);
    sheet(l, r, p.sheet);
    let ui = &mut l.ui;
    let head = Rect::new(r.x, r.y, r.w, 44.0);
    ui.push_clip(head, 0.0);
    ui.p().rounded(Rect::new(head.x, head.y, head.w, head.h + 6.0), 4.0, p.accent);
    ui.pop_clip();
    ui.text_in(scope_title(scope), Rect::new(r.x + 18.0, head.y, r.w - 60.0, head.h), 17.0, Weight::Medium, Color::WHITE, Align::Left);
    let xr = Rect::new(r.right() - 40.0, head.y + 7.0, 30.0, 30.0);
    let (xh, _, close) = ui.interact(id_of("rf-issues-close"), xr);
    ui.icon("close", xr.center(), 20.0, if xh { p.text } else { Color::WHITE });
    // the tabs
    let mut tab_pick = None;
    for (k2, label) in ["Open issues", "Dismissed issues"].iter().enumerate() {
        let tr = Rect::new(r.x + 18.0 + k2 as f32 * 170.0, head.bottom() + 6.0, 160.0, 34.0);
        let on = (k2 == 1) == dismissed_tab;
        let (th, _, click) = ui.interact(id_of(&format!("rf-issues-tab-{k2}")), tr);
        caps(ui, tr, label, if on { p.text } else if th { p.text.alpha(0.8) } else { p.dim }, Align::Left);
        if click && !on {
            tab_pick = Some(k2 == 1);
        }
    }
    ui.p().rect(Rect::new(r.x + 18.0, head.bottom() + 40.0, r.w - 36.0, 1.0), p.line);
    ui.p().rect(Rect::new(r.x + 18.0 + if dismissed_tab { 170.0 } else { 0.0 }, head.bottom() + 38.0, 110.0, 2.0), p.accent);
    // select all
    let ticks = l.company.planning.rf.issue_ticks.clone();
    let keys: Vec<String> = list.iter().map(|&i| board.issues[i].key.clone()).collect();
    let all_on = !keys.is_empty() && keys.iter().all(|k2| ticks.contains(k2));
    let mut y = head.bottom() + 52.0;
    let sa = omsi_ui::tr("Select all (%{n})").replace("%{n}", &list.len().to_string());
    let mut toggle_all = false;
    if check_row(ui, p, "rf-issues-all", Rect::new(r.x + 18.0, y, r.w - 36.0, 28.0), all_on, &sa) {
        toggle_all = true;
    }
    y += 34.0;
    // the list, grouped by what they say
    let mut groups: Vec<(&'static str, Vec<usize>)> = Vec::new();
    for &i in &list {
        let t = board.issues[i].text;
        match groups.iter_mut().find(|g| g.0 == t) {
            Some(g) => g.1.push(i),
            None => groups.push((t, vec![i])),
        }
    }
    let open_groups = l.company.planning.rf.groups_open.clone();
    let foot = r.bottom() - 54.0;
    ui.push_clip(Rect::new(r.x, y, r.w, foot - y), 0.0);
    let mut flips: Vec<Vec<String>> = Vec::new();
    let mut reveal: Option<RowId> = None;
    let mut fold: Option<&'static str> = None;
    if list.is_empty() {
        ui.text_in(if dismissed_tab { "No dismissed issues." } else { "No issues: every rule is kept." }, Rect::new(r.x + 18.0, y, r.w - 36.0, 26.0), 13.0, Weight::Medium, p.dim, Align::Left);
    }
    for (text, members) in &groups {
        let several = members.len() > 2;
        let open = open_groups.contains(text);
        let shown: Vec<usize> = if several && !open { Vec::new() } else { members.clone() };
        if several {
            let gr = Rect::new(r.x + 18.0, y, r.w - 36.0, 30.0);
            let gkeys: Vec<String> = members.iter().map(|&i| board.issues[i].key.clone()).collect();
            let gon = gkeys.iter().all(|k2| ticks.contains(k2));
            let b = Rect::new(gr.x, gr.center().y - 7.0, 14.0, 14.0);
            let (bh, _, bclick) = ui.interact(id_of(&format!("rf-group-tick-{text}")), b.inset(-3.0));
            if gon {
                ui.p().rounded(b, 2.0, p.accent);
                ui.icon("check", b.center(), 12.0, Color::WHITE);
            } else {
                ui.p().rounded_border(b, 2.0, 1.3, if bh { p.text } else { p.dim });
            }
            if bclick {
                flips.push(gkeys);
            }
            let badge = Rect::new(gr.x + 24.0, gr.center().y - 9.0, 22.0, 18.0);
            ui.p().rounded(badge, 9.0, p.dim);
            ui.text_as_is(&members.len().to_string(), Vec2::new(badge.center().x, gr.center().y + 4.0), 11.0, Weight::Bold, p.sheet, Align::Center);
            let words = omsi_ui::tr(rf::issue_title(text)).into_owned();
            ui.text_in(&words, Rect::new(gr.x + 54.0, gr.y, gr.w - 90.0, gr.h), 13.0, Weight::Bold, p.text, Align::Left);
            let (ch, _, cclick) = ui.interact(id_of(&format!("rf-group-open-{text}")), Rect::new(gr.right() - 30.0, gr.y, 30.0, gr.h));
            ui.icon(if open { "expand_less" } else { "expand_more" }, Vec2::new(gr.right() - 14.0, gr.center().y), 18.0, if ch { p.text } else { p.dim });
            if cclick {
                fold = Some(text);
            }
            y += 34.0;
        }
        for &i in &shown {
            let is = &board.issues[i];
            let words = rf::issue_text(is, &|s| omsi_ui::tr(s).into_owned());
            let row = issue_row_words(board, is);
            let full = if row.is_empty() { words } else { format!("{words} — {row}") };
            let x0 = r.x + 18.0 + if several { 24.0 } else { 0.0 };
            let tw = r.right() - 18.0 - x0 - 26.0;
            let lh = ui.paragraph_height(&full, tw, 12.5, Weight::Medium).max(20.0);
            let ir = Rect::new(x0, y, r.right() - 18.0 - x0, lh + 10.0);
            let on = ticks.contains(&is.key);
            let b = Rect::new(ir.x, ir.y + 6.0, 14.0, 14.0);
            let (bh, _, bclick) = ui.interact(id_of(&format!("rf-issue-tick-{}", is.key)), b.inset(-3.0));
            if on {
                ui.p().rounded(b, 2.0, p.accent);
                ui.icon("check", b.center(), 12.0, Color::WHITE);
            } else {
                ui.p().rounded_border(b, 2.0, 1.3, if bh { p.text } else { p.dim });
            }
            if bclick {
                flips.push(vec![is.key.clone()]);
            }
            let tr_ = Rect::new(ir.x + 24.0, ir.y, tw, ir.h);
            let (th, _, tclick) = ui.interact(id_of(&format!("rf-issue-go-{}", is.key)), tr_);
            ui.p().circle(Vec2::new(ir.x + 24.0 - 5.0, ir.y + 13.0), 0.1, p.dim);
            ui.paragraph(&full, Vec2::new(tr_.x, tr_.y + 1.0), tw, 12.5, Weight::Medium, if th { p.accent } else if is.hard { p.text } else { p.text.alpha(0.85) });
            if tclick {
                reveal = match (is.scope, is.row) {
                    (Scope::Duty, Some(row)) => board.shaped.get(row).map(shell::duty_row),
                    (Scope::Vehicle, Some(row)) => board.tours.get(row).map(shell::block_row),
                    _ => None,
                };
            }
            y += ir.h + 2.0;
        }
    }
    ui.pop_clip();
    // the footer: Filter Issues, Dismiss (or Restore) Issues
    ui.p().rect(Rect::new(r.x, foot, r.w, 1.0), p.line);
    let any = !ticks.is_empty();
    let filt = rf_button(ui, p, "rf-issues-filter", Rect::new(r.x + 18.0, foot + 12.0, 190.0, 32.0), "Filter issues", false, any && scope != Scope::Global);
    let dis = rf_button(ui, p, "rf-issues-dismiss", Rect::new(r.right() - 208.0, foot + 12.0, 190.0, 32.0), if dismissed_tab { "Restore issues" } else { "Dismiss issues" }, true, any);
    let s = &mut l.company.planning.rf;
    if close {
        s.panel = Panel::None;
    }
    if let Some(t) = tab_pick {
        s.dismissed_tab = t;
        s.issue_ticks.clear();
    }
    if toggle_all {
        s.issue_ticks = if all_on { Vec::new() } else { keys.clone() };
    }
    for f in flips {
        let on = f.iter().all(|k2| s.issue_ticks.contains(k2));
        s.issue_ticks.retain(|k2| !f.contains(k2));
        if !on {
            s.issue_ticks.extend(f);
        }
    }
    if let Some(t) = fold {
        if let Some(i) = s.groups_open.iter().position(|x| *x == t) {
            s.groups_open.remove(i);
        } else {
            s.groups_open.push(t);
        }
    }
    if let Some(id) = reveal {
        if matches!(id, RowId::Duty(_)) != drivers {
            l.company.planning.view = if drivers { shell::VEHICLES } else { shell::DRIVERS };
        }
        let s = &mut l.company.planning.rf;
        s.reveal = Some(id.clone());
        s.sel_row = Some(id);
    }
    if filt {
        let s = &mut l.company.planning.rf;
        s.filter.issues = Some(ticks.clone());
        s.panel = Panel::None;
        if (scope == Scope::Duty) != drivers {
            l.company.planning.view = if scope == Scope::Duty { shell::DRIVERS } else { shell::VEHICLES };
        }
    }
    if dis {
        let words = if dismissed_tab { omsi_ui::tr("%{n} issues restored") } else { omsi_ui::tr("%{n} issues dismissed") }.replace("%{n}", &ticks.len().to_string());
        let on = !dismissed_tab;
        edit(l, words, |c| {
            for key in &ticks {
                rf::set_dismissed(c, wd, key, on);
            }
            Ok(())
        });
        l.company.planning.rf.panel = Panel::Issues(scope);
        l.company.planning.rf.issue_ticks.clear();
    }
    r
}

// --- the duty panel ---------------------------------------------------------------------------------

/// A duty by hand: its type, lock, times and paid time, signing, its pieces (taken out one by
/// one), the way between them, its breaks, its custom work (added and deleted), its issues.
fn duty_panel(l: &mut Launcher, g: Rect, k: &Cached, board: &Board, p: &Pal, id: &RowId) -> Rect {
    let c: &Company = &k.view;
    let wd = board.weekday;
    let w = 400.0;
    let r = Rect::new(g.right() - w - 10.0, g.y + 8.0, w, g.h - 16.0);
    let Some(row) = board.shaped.iter().position(|s| shell::duty_row(s) == *id) else {
        l.company.planning.rf.panel = Panel::None;
        return r;
    };
    let s = board.shaped[row].clone();
    super::super::super::tour::anchor("rf-duty-panel", r);
    sheet(l, r, p.sheet);
    let prefs = c.planning.flow.prefs.clone();
    let ui = &mut l.ui;
    let head = Rect::new(r.x, r.y, r.w, 48.0);
    ui.push_clip(head, 0.0);
    ui.p().rounded(Rect::new(head.x, head.y, head.w, head.h + 6.0), 4.0, p.warn.darken(0.05));
    ui.pop_clip();
    ui.icon("person", Vec2::new(r.x + 20.0, head.center().y), 19.0, Color::WHITE);
    let kind = omsi_ui::tr(s.kind_name(&prefs)).into_owned();
    ui.text_as_is(&format!("{}  /  {}", s.label(row), kind), Vec2::new(r.x + 38.0, head.center().y + 5.5), 17.0, Weight::Bold, Color::WHITE, Align::Left);
    let xr = Rect::new(r.right() - 38.0, head.y + 9.0, 30.0, 30.0);
    let (xh, _, close) = ui.interact(id_of("rf-duty-close"), xr);
    ui.icon("close", xr.center(), 20.0, if xh { p.text } else { Color::WHITE });
    let lr = Rect::new(r.right() - 72.0, head.y + 9.0, 30.0, 30.0);
    super::super::super::tour::anchor("rf-duty-lock", lr);
    let (lh, _, lock) = ui.interact(id_of("rf-duty-lock"), lr);
    ui.icon(if s.locked { "lock" } else { "livery_unlock" }, lr.center(), 19.0, if lh || s.locked { Color::WHITE } else { Color::WHITE.alpha(0.7) });
    ui.tooltip(lr, if s.locked { "Locked: Optimise leaves it. Click to unlock" } else { "Lock the duty: Optimise leaves it" });
    let scroll = l.company.planning.rf.duty_scroll;
    let ui = &mut l.ui;
    ui.push_clip(Rect::new(r.x, head.bottom(), r.w, r.h - head.h), 0.0);
    let mut y = head.bottom() + 14.0 - scroll;
    let x0 = r.x + 18.0;
    let iw = r.w - 36.0;
    // the type: automatic, or set by hand
    caps(ui, Rect::new(x0, y, iw, 16.0), "Duty type", p.dim, Align::Left);
    y += 20.0;
    let mut names: Vec<String> = vec![omsi_ui::tr("Automatic").into_owned()];
    names.extend(prefs.duty_types.iter().map(|t| omsi_ui::tr(&t.name).into_owned()));
    names.push(omsi_ui::tr(rf::OTHER).into_owned());
    let mut at = if s.kind_set { s.kind.map(|k2| k2 + 1).unwrap_or(names.len() - 1) } else { 0 };
    let picked_type = plain(ui, |ui| ui.select("rf-duty-type", Rect::new(x0, y, iw, 32.0), &mut at, &names));
    y += 44.0;
    // the times
    let times: [(&str, String); 8] = [
        ("Sign-on", rf::clock(s.start)),
        ("Sign-off", rf::clock(s.end)),
        ("Spread", rf::hm(s.spread())),
        ("Work time", rf::hm(s.work)),
        ("Driving time", rf::hm(s.driving)),
        ("Platform time", rf::hm(s.platform)),
        ("Splits", if s.splits.is_empty() { "-".into() } else { s.splits.iter().map(|x| format!("{}–{}", rf::clock(x.0), rf::clock(x.1))).collect::<Vec<_>>().join(", ") }),
        ("Changeovers", s.changeovers.to_string()),
    ];
    for (n, (label, value)) in times.iter().enumerate() {
        let cx = x0 + (n % 2) as f32 * (iw * 0.5);
        let cy = y + (n / 2) as f32 * 22.0;
        let t = omsi_ui::tr(label).into_owned();
        ui.text_in(&t, Rect::new(cx, cy, iw * 0.25, 20.0), 12.0, Weight::Medium, p.dim, Align::Left);
        ui.text_as_is(value, Vec2::new(cx + iw * 0.25, cy + 14.0), 12.5, Weight::Bold, p.text, Align::Left);
    }
    y += 4.0 * 22.0 + 10.0;
    // the paid time (set by hand or not), the signing
    let note = s.key.as_ref().and_then(|key| c.planning.flow.sheet.notes.iter().find(|n| n.weekday == wd && n.key.is(key))).cloned();
    let spare = s.spare.and_then(|id| c.planning.flow.sheet.spares.iter().find(|x| x.weekday == wd && x.id == id)).cloned();
    let paid_set = note.as_ref().and_then(|n| n.paid).or(spare.as_ref().and_then(|x| x.paid));
    ui.text_in("Paid time", Rect::new(x0, y, iw * 0.3, 30.0), 12.5, Weight::Medium, p.dim, Align::Left);
    let new_paid = stepper(ui, p, "rf-duty-paid", Rect::new(x0 + iw * 0.3, y, 130.0, 30.0), s.paid, 5, 0, 1440, &rf::hm);
    let auto = paid_set.is_some() && rf_button(ui, p, "rf-duty-paid-auto", Rect::new(x0 + iw * 0.3 + 140.0, y + 1.0, 100.0, 28.0), "Automatic", false, true);
    y += 38.0;
    let mut sign: Option<(bool, i32)> = None;
    if s.key.is_some() {
        let son = note.as_ref().and_then(|n| n.sign_on).unwrap_or(prefs.sign_on);
        let soff = note.as_ref().and_then(|n| n.sign_off).unwrap_or(prefs.sign_off);
        ui.text_in("Sign-on", Rect::new(x0, y, iw * 0.3, 30.0), 12.5, Weight::Medium, p.dim, Align::Left);
        if let Some(v) = stepper(ui, p, "rf-duty-son", Rect::new(x0 + iw * 0.3, y, 110.0, 30.0), son, 1, 0, 60, &|m| format!("{m} min")) {
            sign = Some((true, v));
        }
        y += 38.0;
        ui.text_in("Sign-off", Rect::new(x0, y, iw * 0.3, 30.0), 12.5, Weight::Medium, p.dim, Align::Left);
        if let Some(v) = stepper(ui, p, "rf-duty-soff", Rect::new(x0 + iw * 0.3, y, 110.0, 30.0), soff, 1, 0, 60, &|m| format!("{m} min")) {
            sign = Some((false, v));
        }
        y += 40.0;
    }
    // the pieces, the way between them
    caps(ui, Rect::new(x0, y, iw, 16.0), "Pieces of work", p.dim, Align::Left);
    y += 22.0;
    let mut take_out: Option<(usize, usize)> = None;
    let mut travel_pick: Option<(u32, Travel)> = None;
    for (n, sp) in s.spans.iter().enumerate() {
        let t = &board.tours[sp.tour];
        let colour = line_colour(c, &t.line);
        let rr = Rect::new(x0, y, iw, 28.0);
        ui.p().rounded(rr, 3.0, p.field);
        ui.swatch(Rect::new(rr.x + 6.0, rr.y + 6.0, 26.0, 16.0), 3.0, colour);
        ui.text_as_is(&t.number, Vec2::new(rr.x + 19.0, rr.y + 18.5), 10.5, Weight::Black, gantt::ink_on(colour), Align::Center);
        let trips = gantt::piece_trips(board, sp.tour, sp.duty);
        let (a, b) = (trips.first().map(|x| x.from.clone()).unwrap_or_default(), trips.last().map(|x| x.end().to_string()).unwrap_or_default());
        let words = format!("{}  {}–{}  {} → {}", block_label(t), rf::clock(sp.from), rf::clock(sp.to), a, b);
        ui.text_in(&words, Rect::new(rr.x + 40.0, rr.y, rr.w - 72.0, rr.h), 12.0, Weight::Medium, p.text, Align::Left);
        if s.spans.len() > 1 {
            let xr2 = Rect::new(rr.right() - 28.0, rr.y + 2.0, 24.0, 24.0);
            let (h2, _, c2) = ui.interact(id_of(&format!("rf-duty-out-{n}")), xr2);
            ui.icon("close", xr2.center(), 15.0, if h2 { p.err } else { p.dim });
            ui.tooltip(xr2, "Take the piece out: a duty of its own");
            if c2 {
                take_out = Some((sp.tour, sp.duty));
            }
        }
        y += 32.0;
        // (between this piece and the next: the way, where the places differ)
        if let Some(next) = s.spans.get(n + 1) {
            let here_end = trips.last().map(|x| x.end().trim().to_string()).unwrap_or_default();
            let next_start = gantt::piece_trips(board, next.tour, next.duty).first().map(|x| x.from.trim().to_string()).unwrap_or_default();
            if !here_end.eq_ignore_ascii_case(&next_start) {
                let mode = note.as_ref().and_then(|nn| nn.travel.iter().find(|x| x.0 as usize == n).map(|x| x.1)).unwrap_or(prefs.travel);
                let at_k = Travel::ALL.iter().position(|x| *x == mode).unwrap_or(0);
                ui.icon("directions_walk", Vec2::new(x0 + 10.0, y + 13.0), 15.0, p.warn);
                if let Some(mk) = seg(ui, p, &format!("rf-duty-travel-{n}"), Rect::new(x0 + 24.0, y, iw - 24.0, 26.0), at_k, &Travel::LABELS) {
                    travel_pick = Some((n as u32, Travel::ALL[mk]));
                }
                y += 32.0;
            } else {
                let gap = next.from - sp.to;
                let words = if gap >= prefs.split_min { omsi_ui::tr("Split of %{t}").replace("%{t}", &rf::hm(gap)) } else { omsi_ui::tr("Break of %{t}").replace("%{t}", &rf::hm(gap)) };
                ui.text_in(&words, Rect::new(x0 + 24.0, y - 2.0, iw - 24.0, 20.0), 11.5, Weight::Medium, p.dim, Align::Left);
                y += 20.0;
            }
        }
    }
    // the breaks
    let breaks: Vec<(i32, i32)> = rf::breaks_of(&prefs, &board.tours, &s).into_iter().filter(|b| b.1 - b.0 >= prefs.break_min).collect();
    if !breaks.is_empty() {
        let words = format!("{}: {}", omsi_ui::tr("Breaks"), breaks.iter().take(6).map(|b| format!("{} ({})", rf::clock(b.0), rf::hm(b.1 - b.0))).collect::<Vec<_>>().join(", "));
        let h = ui.paragraph(&words, Vec2::new(x0, y), iw, 11.5, Weight::Regular, p.dim);
        y += h + 6.0;
    }
    // custom work: the list, and one to add
    caps(ui, Rect::new(x0, y, iw, 16.0), "Custom work", p.dim, Align::Left);
    y += 22.0;
    let mut delete: Option<usize> = None;
    for (n, cw) in s.custom.iter().enumerate() {
        let words = format!("{}  {}–{}", omsi_ui::tr(&cw.kind), rf::clock(cw.from), rf::clock(cw.to));
        ui.text_in(&words, Rect::new(x0, y, iw - 30.0, 24.0), 12.0, Weight::Medium, p.text, Align::Left);
        let dr = Rect::new(x0 + iw - 24.0, y, 24.0, 24.0);
        let (dh, _, dc) = ui.interact(id_of(&format!("rf-duty-cw-del-{n}")), dr);
        ui.icon("delete", dr.center(), 15.0, if dh { p.err } else { p.dim });
        if dc {
            delete = Some(n);
        }
        y += 26.0;
    }
    let add_state = l.company.planning.rf.custom_draft.clone();
    let ui = &mut l.ui;
    let (mut kind_k, mut from, mut to) = add_state;
    let kinds: Vec<String> = rf::CUSTOM_KINDS.iter().map(|x| omsi_ui::tr(x).into_owned()).collect();
    plain(ui, |ui| ui.select("rf-duty-cw-kind", Rect::new(x0, y, 120.0, 30.0), &mut kind_k, &kinds));
    if let Some(v) = stepper(ui, p, "rf-duty-cw-from", Rect::new(x0 + 126.0, y, 100.0, 30.0), from, 15, 0, 1800, &rf::clock) {
        from = v;
        to = to.max(from + 15);
    }
    if let Some(v) = stepper(ui, p, "rf-duty-cw-to", Rect::new(x0 + 232.0, y, 100.0, 30.0), to, 15, from + 15, 1800, &rf::clock) {
        to = v;
    }
    let add = rf_button(ui, p, "rf-duty-cw-add", Rect::new(x0 + iw - 54.0, y + 1.0, 54.0, 28.0), "Add", true, true);
    l.company.planning.rf.custom_draft = (kind_k, from, to);
    y += 40.0;
    // its issues
    let ui = &mut l.ui;
    if !s.issues.is_empty() {
        caps(ui, Rect::new(x0, y, iw, 16.0), "Issues", p.dim, Align::Left);
        y += 20.0;
        for &i in &s.issues {
            let is = &board.issues[i];
            let words = rf::issue_text(is, &|t| omsi_ui::tr(t).into_owned());
            ui.icon("error", Vec2::new(x0 + 7.0, y + 9.0), 13.0, if is.hard { p.err } else { p.warn });
            let h = ui.paragraph(&words, Vec2::new(x0 + 20.0, y), iw - 20.0, 11.5, Weight::Medium, p.text);
            y += h + 4.0;
        }
        y += 6.0;
    }
    // a spare duty can go altogether
    let mut remove_spare = false;
    if s.spare.is_some() {
        remove_spare = rf_button(ui, p, "rf-duty-remove", Rect::new(x0, y, 160.0, 30.0), "Delete duty", false, true);
        y += 40.0;
    }
    ui.pop_clip();
    // (the wheel scrolls what does not fit)
    let room = r.bottom() - (r.y + 48.0);
    let content = y + l.company.planning.rf.duty_scroll - (r.y + 48.0);
    if r.contains(l.ui.input.mouse) && l.ui.input.wheel.y.abs() > 0.0 {
        l.company.planning.rf.duty_scroll -= l.ui.input.wheel.y * 40.0;
        l.ui.input.wheel = Vec2::ZERO;
    }
    l.company.planning.rf.duty_scroll = l.company.planning.rf.duty_scroll.clamp(0.0, (content - room + 10.0).max(0.0));
    // what was done
    let label = s.label(row);
    let key = s.key.clone();
    let spare_id = s.spare;
    let set_note = |l: &mut Launcher, words: String, f: &dyn Fn(&mut rf::DutyNote)| {
        let key = key.clone();
        edit(l, words, |c| {
            match (&key, spare_id) {
                (Some(key), _) => {
                    f(rf::note_mut(c, wd, key));
                    rf::tidy_notes(c);
                }
                (None, Some(id)) => {
                    if let Some(sp) = c.planning.flow.sheet.spares.iter_mut().find(|x| x.weekday == wd && x.id == id) {
                        let mut n = rf::DutyNote { kind: sp.kind.clone(), paid: sp.paid, custom: sp.custom.clone(), locked: sp.locked, ..Default::default() };
                        f(&mut n);
                        sp.kind = n.kind;
                        sp.paid = n.paid;
                        sp.custom = n.custom;
                        sp.locked = n.locked;
                    }
                }
                _ => {}
            }
            Ok(())
        });
    };
    if close {
        l.company.planning.rf.panel = Panel::None;
    }
    if lock {
        gantt::toggle_lock(l, board, Kind::Duty(row));
        l.company.planning.rf.panel = Panel::Duty(id.clone());
    }
    let types = prefs.duty_types.clone();
    let keep_open = |l: &mut Launcher| l.company.planning.rf.panel = Panel::Duty(id.clone());
    if picked_type {
        let kind = match at {
            0 => None,
            n if n <= types.len() => Some(types[n - 1].name.clone()),
            _ => Some(rf::OTHER.to_string()),
        };
        let words = omsi_ui::tr("Type of duty %{d} set").replace("%{d}", &label);
        set_note(l, words, &|n| n.kind = kind.clone());
        keep_open(l);
    }
    if let Some(v) = new_paid {
        let words = omsi_ui::tr("Paid time of duty %{d} set to %{t}").replace("%{d}", &label).replace("%{t}", &rf::hm(v));
        set_note(l, words, &|n| n.paid = Some(v));
        keep_open(l);
    }
    if auto {
        let words = omsi_ui::tr("Paid time of duty %{d} automatic").replace("%{d}", &label);
        set_note(l, words, &|n| n.paid = None);
        keep_open(l);
    }
    if let Some((on, v)) = sign {
        let words = omsi_ui::tr("Signing of duty %{d} set").replace("%{d}", &label);
        set_note(l, words, &|n| if on { n.sign_on = Some(v) } else { n.sign_off = Some(v) });
        keep_open(l);
    }
    if let Some((gap, mode)) = travel_pick {
        let words = omsi_ui::tr("Travel of duty %{d} by %{mode}").replace("%{d}", &label).replace("%{mode}", &omsi_ui::tr(mode.label()));
        set_note(l, words, &|n| {
            n.travel.retain(|x| x.0 != gap);
            n.travel.push((gap, mode));
        });
        keep_open(l);
    }
    if let Some(ci) = delete {
        delete_custom(l, &s, row, ci, wd);
        keep_open(l);
    }
    if add {
        let words = omsi_ui::tr("Custom work added to duty %{d}").replace("%{d}", &label);
        let kind = rf::CUSTOM_KINDS[kind_k.min(rf::CUSTOM_KINDS.len() - 1)].to_string();
        set_note(l, words, &|n| n.custom.push(Custom { kind: kind.clone(), from, to, place: String::new() }));
        keep_open(l);
    }
    if let Some(pc) = take_out {
        let tours = board.tours.clone();
        let words = omsi_ui::tr("Piece of %{v} made a duty of its own").replace("%{v}", &block_label(&tours[pc.0]));
        edit(l, words, |c| st::detach(c, wd, &tours, pc));
        keep_open(l);
    }
    if remove_spare {
        if let Some(id) = spare_id {
            let words = omsi_ui::tr("Spare duty %{d} deleted").replace("%{d}", &label);
            edit(l, words, |c| {
                rf::remove_spare(c, wd, id);
                Ok(())
            });
            l.company.planning.rf.panel = Panel::None;
        }
    }
    r
}

// --- the small windows ------------------------------------------------------------------------------

/// The schedule's history under its icon: every change, the latest first; a click goes back
/// to before it.
fn history(l: &mut Launcher, page: Rect, p: &Pal) -> Rect {
    let undo: Vec<String> = l.company.planning.rf.undo.iter().map(|u| u.1.clone()).collect();
    let n = undo.len();
    let h = (n.min(14) as f32 + 1.0) * 30.0 + 46.0;
    let r = Rect::new(page.right() - 760.0, page.y + 4.0, 380.0, h);
    sheet(l, r, p.menu);
    let ui = &mut l.ui;
    caps(ui, Rect::new(r.x + 14.0, r.y + 8.0, r.w - 28.0, 24.0), "Schedule history", p.menu_text, Align::Left);
    let mut y = r.y + 38.0;
    ui.text_in("Now", Rect::new(r.x + 14.0, y, r.w - 28.0, 28.0), 13.0, Weight::Bold, p.accent, Align::Left);
    y += 30.0;
    let mut back = None;
    for (k, label) in undo.iter().enumerate().rev().take(14) {
        if menu_item(ui, p, &format!("rf-hist-{k}"), Rect::new(r.x, y, r.w, 30.0), label, true, "Go back to before this change") {
            back = Some(k);
        }
        y += 30.0;
    }
    if let Some(k) = back {
        shell::back_to(l, k);
        l.company.planning.rf.panel = Panel::None;
    }
    r
}

/// Stats: the columns of figures beside the IDs.
fn stats(l: &mut Launcher, page: Rect, g: Rect, p: &Pal, drivers: bool) -> Rect {
    let options: Vec<Stat> = Stat::ALL.iter().copied().filter(|s| drivers || !matches!(s, Stat::Paid | Stat::Type)).collect();
    let h = options.len() as f32 * 30.0 + 90.0;
    let r = Rect::new(g.x + 250.0, g.y - 2.0, 260.0, h);
    let _ = page;
    sheet(l, r, p.menu);
    let mut on = l.company.planning.rf.stats.clone();
    let mut y = r.y + 12.0;
    let mut changed = false;
    for s in &options {
        let has = on.contains(s);
        if check_row(&mut l.ui, &Pal { text: p.menu_text, dim: p.menu_text.alpha(0.6), ..*p }, &format!("rf-stat-{s:?}"), Rect::new(r.x + 16.0, y, r.w - 32.0, 28.0), has, s.label()) {
            if has {
                on.retain(|x| x != s);
            } else {
                on.push(*s);
            }
            changed = true;
        }
        y += 30.0;
    }
    let reset = rf_button(&mut l.ui, p, "rf-stats-reset", Rect::new(r.x + 16.0, r.bottom() - 44.0, 100.0, 30.0), "Reset", false, true);
    let done = rf_button(&mut l.ui, p, "rf-stats-apply", Rect::new(r.right() - 116.0, r.bottom() - 44.0, 100.0, 30.0), "Done", true, true);
    let s = &mut l.company.planning.rf;
    if changed {
        s.stats = on;
    }
    if reset {
        s.stats.clear();
        s.sort = None;
    }
    if done {
        s.panel = Panel::None;
    }
    r
}

/// Layers: what the trips' chips say, and the layovers' minutes.
fn layers(l: &mut Launcher, g: Rect, p: &Pal) -> Rect {
    let r = Rect::new(g.x + 330.0, g.y - 2.0, 330.0, 230.0);
    sheet(l, r, p.menu);
    let mp = Pal { text: p.menu_text, dim: p.menu_text.alpha(0.6), ..*p };
    let ly = l.company.planning.rf.layers;
    let ui = &mut l.ui;
    caps(ui, Rect::new(r.x + 16.0, r.y + 10.0, r.w - 32.0, 18.0), "Trips", mp.dim, Align::Left);
    let mut label = ly.label;
    for (k, words) in ["Route", "Start and end time", "Start and end stops"].iter().enumerate() {
        let rr = Rect::new(r.x + 16.0, r.y + 32.0 + k as f32 * 30.0, r.w - 32.0, 28.0);
        if check_row(ui, &mp, &format!("rf-layer-{k}"), rr, ly.label == k as u8, words) {
            label = k as u8;
        }
    }
    caps(ui, Rect::new(r.x + 16.0, r.y + 128.0, r.w - 32.0, 18.0), "Between trips", mp.dim, Align::Left);
    let mut breaks = ly.breaks;
    if check_row(ui, &mp, "rf-layer-breaks", Rect::new(r.x + 16.0, r.y + 150.0, r.w - 32.0, 28.0), ly.breaks, "Break duration") {
        breaks = !breaks;
    }
    let mut moved = ly.moved;
    if check_row(&mut l.ui, &mp, "rf-layer-moved", Rect::new(r.x + 16.0, r.y + 184.0, r.w - 32.0, 28.0), ly.moved, "Mark trips off their timetable tour") {
        moved = !moved;
    }
    l.company.planning.rf.layers = shell::Layers { label, breaks, moved };
    r
}

/// Filter: the lines, the direction, the departure times, locked only.
fn filter(l: &mut Launcher, page: Rect, k: &Cached, p: &Pal) -> Rect {
    let c: &Company = &k.view;
    let mut f = l.company.planning.rf.filter_edit.clone().unwrap_or_default();
    let lines: Vec<(String, String)> = c.lines.iter().map(|x| (x.name.clone(), x.number.clone())).collect();
    let rows = lines.len().div_ceil(3);
    let h = 300.0 + rows as f32 * 30.0;
    let r = Rect::new(page.center().x - 290.0, page.y + 70.0, 580.0, h);
    // (a dim over the page)
    l.ui.p().rect(page, Color::rgba(0, 0, 0, 0.35));
    sheet(l, r, p.sheet);
    l.company.planning.rf.over.push(page);
    let ui = &mut l.ui;
    let head = Rect::new(r.x, r.y, r.w, 44.0);
    ui.push_clip(head, 0.0);
    ui.p().rounded(Rect::new(head.x, head.y, head.w, head.h + 6.0), 4.0, p.accent);
    ui.pop_clip();
    ui.text_in("Filter", Rect::new(r.x + 18.0, r.y, 200.0, 44.0), 17.0, Weight::Medium, Color::WHITE, Align::Left);
    let mut y = head.bottom() + 16.0;
    caps(ui, Rect::new(r.x + 18.0, y, 300.0, 16.0), "Vehicles", p.accent, Align::Left);
    y += 24.0;
    let cw = (r.w - 36.0) / 3.0;
    for (n, (name, number)) in lines.iter().enumerate() {
        let rr = Rect::new(r.x + 18.0 + (n % 3) as f32 * cw, y + (n / 3) as f32 * 30.0, cw - 8.0, 28.0);
        let on = !f.hidden.iter().any(|h2| h2.eq_ignore_ascii_case(name));
        let words = omsi_ui::tr("Line %{n}").replace("%{n}", number);
        if check_row(ui, p, &format!("rf-filter-line-{n}"), rr, on, &words) {
            if on {
                f.hidden.push(name.clone());
            } else {
                f.hidden.retain(|h2| !h2.eq_ignore_ascii_case(name));
            }
        }
    }
    y += rows as f32 * 30.0 + 6.0;
    if check_row(ui, p, "rf-filter-locked", Rect::new(r.x + 18.0, y, 300.0, 28.0), f.locked_only, "Locked only") {
        f.locked_only = !f.locked_only;
    }
    y += 40.0;
    caps(ui, Rect::new(r.x + 18.0, y, 300.0, 16.0), "Trips", p.accent, Align::Left);
    y += 24.0;
    if let Some(d) = seg(ui, p, "rf-filter-dir", Rect::new(r.x + 18.0, y, 330.0, 28.0), f.dir as usize, &["Both ways", "Outbound", "Inbound"]) {
        f.dir = d as u8;
    }
    y += 40.0;
    ui.text_in("Departure from", Rect::new(r.x + 18.0, y, 130.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Left);
    if let Some(v) = stepper(ui, p, "rf-filter-from", Rect::new(r.x + 150.0, y, 110.0, 30.0), f.from, 30, 0, 1800, &rf::clock) {
        f.from = v;
    }
    ui.text_in("to", Rect::new(r.x + 270.0, y, 30.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Center);
    if let Some(v) = stepper(ui, p, "rf-filter-to", Rect::new(r.x + 304.0, y, 110.0, 30.0), f.to.min(1800), 30, 0, 1800, &|v| if v >= 1800 { "—".to_string() } else { rf::clock(v) }) {
        f.to = if v >= 1800 { 2880 } else { v };
    }
    let clear = rf_button(ui, p, "rf-filter-clear", Rect::new(r.x + 18.0, r.bottom() - 46.0, 120.0, 32.0), "Clear all", false, true);
    let cancel = rf_button(ui, p, "rf-filter-cancel", Rect::new(r.right() - 284.0, r.bottom() - 46.0, 116.0, 32.0), "Cancel", false, true);
    let apply = rf_button(ui, p, "rf-filter-apply", Rect::new(r.right() - 160.0, r.bottom() - 46.0, 142.0, 32.0), "Apply filter", true, true);
    let s = &mut l.company.planning.rf;
    if clear {
        f = Filter::default();
    }
    s.filter_edit = Some(f.clone());
    if cancel {
        s.panel = Panel::None;
    }
    if apply {
        let issues = s.filter.issues.take();
        s.filter = Filter { issues, ..f };
        s.panel = Panel::None;
    }
    r
}

/// The zoom's choices above it.
fn zoom(l: &mut Launcher, status: Rect, p: &Pal) -> Rect {
    let r = Rect::new(status.right() - 120.0, status.y - 6.0 * 30.0 - 14.0, 110.0, 6.0 * 30.0 + 10.0);
    sheet(l, r, p.menu);
    let mut pick = None;
    for (k, z) in [100.0f32, 150.0, 200.0, 300.0, 400.0, 600.0].iter().enumerate() {
        if menu_item(&mut l.ui, p, &format!("rf-zoom-{k}"), Rect::new(r.x, r.y + 5.0 + k as f32 * 30.0, r.w, 30.0), &format!("{z:.0}%"), true, "") {
            pick = Some(z / 100.0);
        }
    }
    if let Some(z) = pick {
        let s = &mut l.company.planning.rf;
        s.zoom = z;
        s.panel = Panel::None;
    }
    r
}

/// Print: the bus graph or the duty graph as a PDF file.
fn print_menu(l: &mut Launcher, page: Rect, k: &Cached, board: &Board, p: &Pal, date: &str) -> Rect {
    let r = Rect::new(page.x + 280.0, page.y + 4.0, 260.0, 74.0);
    sheet(l, r, p.menu);
    let bus = menu_item(&mut l.ui, p, "rf-print-bus", Rect::new(r.x, r.y + 6.0, r.w, 30.0), "Bus graph (PDF)", true, "Every vehicle's day on the hours, as Optibus prints it");
    let duty = menu_item(&mut l.ui, p, "rf-print-duty", Rect::new(r.x, r.y + 38.0, r.w, 30.0), "Duty graph (PDF)", true, "Every driver's duty on the hours");
    if bus || duty {
        l.company.planning.rf.panel = Panel::None;
        super::rf_print::print(l, k, board, date, bus);
    }
    r
}

/// The day: the next seven.
fn day_menu(l: &mut Launcher, page: Rect, k: &Cached, p: &Pal) -> Rect {
    let r = Rect::new(page.right() - 560.0, page.y + 4.0, 230.0, 7.0 * 30.0 + 10.0);
    sheet(l, r, p.menu);
    let today = l.company.company.as_ref().map(|c| c.date.clone()).unwrap_or_else(|| k.view.date.clone());
    let mut pick = None;
    for d in 0..7usize {
        let date = co::dates::add(&today, d as i64);
        let words = if d == 0 { format!("{}  ·  {}", day_label(&date), omsi_ui::tr("Company day")) } else { day_label(&date) };
        let on = l.company.planning.day == d;
        let rr = Rect::new(r.x, r.y + 5.0 + d as f32 * 30.0, r.w, 30.0);
        if on {
            l.ui.p().rect(rr, p.accent.alpha(0.14));
        }
        if menu_item(&mut l.ui, p, &format!("rf-day-{d}"), rr, &words, true, "") {
            pick = Some(d);
        }
    }
    if let Some(d) = pick {
        l.company.planning.day = d;
        let s = &mut l.company.planning.rf;
        s.panel = Panel::None;
        s.settle();
    }
    r
}

/// A line's colour: the palette's swatches and a picker for any other; used for its chips, its
/// plates and the map.
fn colour(l: &mut Launcher, page: Rect, k: &Cached, p: &Pal, line: &str) -> Rect {
    let c: &Company = &k.view;
    let r = Rect::new(page.center().x - 170.0, page.y + 120.0, 340.0, 330.0);
    super::super::super::tour::anchor("rf-colour", r);
    sheet(l, r, p.sheet);
    let number = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line)).map(|x| x.number.clone()).unwrap_or_default();
    let now = rf::line_colour(c, line);
    let ui = &mut l.ui;
    let title = omsi_ui::tr("Colour of line %{n}").replace("%{n}", &number);
    ui.text_in(&title, Rect::new(r.x + 18.0, r.y + 10.0, r.w - 60.0, 30.0), 15.0, Weight::Bold, p.text, Align::Left);
    let xr = Rect::new(r.right() - 38.0, r.y + 10.0, 28.0, 28.0);
    let (xh, _, close) = ui.interact(id_of("rf-colour-close"), xr);
    ui.icon("close", xr.center(), 18.0, if xh { p.err } else { p.dim });
    let mut pick: Option<Option<[u8; 3]>> = None;
    for (n, sw) in rf::PALETTE.iter().enumerate() {
        let sr = Rect::new(r.x + 18.0 + (n % 10) as f32 * 30.0, r.y + 48.0, 24.0, 24.0);
        let (h, _, click) = ui.interact(id_of(&format!("rf-swatch-{n}")), sr);
        ui.swatch(sr, 4.0, Color::rgba(sw[0], sw[1], sw[2], 1.0));
        if *sw == now || h {
            ui.p().rounded_border(sr.inset(-2.0), 5.0, 1.6, p.text);
        }
        if click {
            pick = Some(Some(*sw));
        }
    }
    let cur = crate::accent::pack(now);
    if let Some(v) = plain(ui, |ui| super::super::super::accent_pick::picker(ui, &format!("rf-picker-{line}"), Rect::new(r.x + 18.0, r.y + 84.0, r.w - 36.0, 0.0), cur)) {
        pick = Some(Some(crate::accent::unpack(v)));
    }
    let auto = rf_button(ui, p, "rf-colour-auto", Rect::new(r.x + 18.0, r.bottom() - 44.0, 150.0, 30.0), "Automatic", false, true);
    if auto {
        pick = Some(None);
    }
    if close {
        l.company.planning.rf.panel = Panel::None;
    }
    if let Some(colour) = pick {
        // (the line's colour is the company's, not the draft's: plates and the map take it)
        let line2 = line.to_string();
        if super::super::act(l, |c| Ok(rf::set_line_colour(c, &line2, colour))) == Some(true) {
            l.company.planning.rf.cache = None;
        }
    }
    r
}

/// G: go to a vehicle or a duty by its ID.
fn go_to(l: &mut Launcher, page: Rect, k: &Cached, board: &Board, p: &Pal, text: &str, drivers: bool) -> Rect {
    let r = Rect::new(page.center().x - 180.0, page.y + 100.0, 360.0, 92.0);
    sheet(l, r, p.sheet);
    let _ = k;
    let mut t = text.to_string();
    let ui = &mut l.ui;
    caps(ui, Rect::new(r.x + 16.0, r.y + 8.0, r.w - 32.0, 18.0), if drivers { "Go to duty" } else { "Go to vehicle" }, p.dim, Align::Left);
    let field = Rect::new(r.x + 16.0, r.y + 34.0, r.w - 32.0, 36.0);
    plain(ui, |ui| {
        ui.focus = Some(id_of("rf-goto-field"));
        ui.text_input("rf-goto-field", field, &mut t, if drivers { "1003" } else { "1/02" }, Some("search"));
    });
    let enter = ui.input.keys.contains(&Key::Enter);
    let esc = ui.input.keys.contains(&Key::Escape);
    if enter {
        let want = t.trim().to_lowercase();
        let found: Option<RowId> = if drivers {
            board.shaped.iter().enumerate().find(|(n, s)| s.label(*n).to_lowercase() == want).map(|(_, s)| shell::duty_row(s))
        } else {
            board.tours.iter().find(|tt| block_label(tt).to_lowercase() == want || block_label(tt).to_lowercase().ends_with(&format!("/{want}")) || tt.tour.trim().to_lowercase() == want).map(shell::block_row)
        };
        match found {
            Some(id) => {
                let s = &mut l.company.planning.rf;
                s.reveal = Some(id.clone());
                s.sel_row = Some(id.clone());
                s.pinned.retain(|x| *x != id);
                s.panel = Panel::None;
            }
            None => l.state.set_status(omsi_ui::tr("No vehicle or duty with that ID.").into_owned(), true),
        }
        l.ui.focus = None;
        return r;
    }
    if esc {
        l.company.planning.rf.panel = Panel::None;
        l.ui.focus = None;
        return r;
    }
    l.company.planning.rf.panel = Panel::GoTo(t);
    r
}

/// The optimisation settings: what Optimise makes, and Optimise itself.
fn opt_settings(l: &mut Launcher, page: Rect, k: &Cached, p: &Pal) -> Rect {
    let r = Rect::new(page.right() - 440.0, page.y + 60.0, 420.0, 250.0);
    sheet(l, r, p.sheet);
    let prefs = k.view.planning.flow.prefs.clone();
    let ui = &mut l.ui;
    let head = Rect::new(r.x, r.y, r.w, 44.0);
    ui.push_clip(head, 0.0);
    ui.p().rounded(Rect::new(head.x, head.y, head.w, head.h + 6.0), 4.0, p.accent);
    ui.pop_clip();
    ui.text_in("Optimisation settings", Rect::new(r.x + 18.0, r.y, r.w - 60.0, 44.0), 16.0, Weight::Medium, Color::WHITE, Align::Left);
    let mut y = head.bottom() + 16.0;
    let veh = switch(ui, p, "rf-opt-veh", Rect::new(r.x + 18.0, y, r.w - 36.0, 30.0), prefs.optimise_vehicles, "Optimise vehicles (the blocks)");
    y += 38.0;
    let dut = switch(ui, p, "rf-opt-duty", Rect::new(r.x + 18.0, y, r.w - 36.0, 30.0), prefs.optimise_duties, "Optimise duties (cut and paired)");
    y += 38.0;
    ui.paragraph("Locked vehicles and duties are kept; the preferences say what may be and what it costs.", Vec2::new(r.x + 18.0, y), r.w - 36.0, 12.0, Weight::Regular, p.dim);
    let restore = rf_button(ui, p, "rf-opt-restore", Rect::new(r.x + 18.0, r.bottom() - 46.0, 150.0, 32.0), "Restore defaults", false, true);
    let close = rf_button(ui, p, "rf-opt-close", Rect::new(r.right() - 254.0, r.bottom() - 46.0, 110.0, 32.0), "Close", false, true);
    let go = rf_button(ui, p, "rf-opt-go", Rect::new(r.right() - 136.0, r.bottom() - 46.0, 118.0, 32.0), "Optimize", true, true);
    if veh || dut || restore {
        super::super::act(l, |c| {
            let f = &mut c.planning.flow.prefs;
            if veh {
                f.optimise_vehicles = !f.optimise_vehicles;
            }
            if dut {
                f.optimise_duties = !f.optimise_duties;
            }
            if restore {
                f.optimise_vehicles = true;
                f.optimise_duties = true;
            }
            Ok(())
        });
    }
    if close {
        l.company.planning.rf.panel = Panel::None;
    }
    if go {
        l.company.planning.rf.panel = Panel::None;
        shell::optimise(l, k);
    }
    r
}

/// A histogram large: its bars with counts and shares.
fn histogram_window(l: &mut Launcher, page: Rect, board: &Board, p: &Pal, which: u8) -> Rect {
    let f = &board.figures;
    let bars: Vec<(String, usize)> = if which == 0 { f.types.iter().map(|t| (omsi_ui::tr(&t.0).into_owned(), t.1)).collect() } else { rf::PAID_BANDS.iter().zip(f.paid_bands.iter()).map(|(b, n)| (format!("{b} h"), *n)).collect() };
    let r = Rect::new(page.center().x - 300.0, page.y + 80.0, 600.0, 360.0);
    l.ui.p().rect(page, Color::rgba(0, 0, 0, 0.35));
    sheet(l, r, p.sheet);
    l.company.planning.rf.over.push(page);
    let ui = &mut l.ui;
    let title = if which == 0 { "Duty types" } else { "Duties paid time" };
    caps(ui, Rect::new(r.x + 18.0, r.y + 12.0, r.w - 60.0, 22.0), title, p.text, Align::Left);
    let xr = Rect::new(r.right() - 38.0, r.y + 8.0, 28.0, 28.0);
    let (xh, _, close) = ui.interact(id_of("rf-hist-close"), xr);
    ui.icon("close", xr.center(), 18.0, if xh { p.err } else { p.dim });
    let total: usize = bars.iter().map(|b| b.1).sum::<usize>().max(1);
    let most = bars.iter().map(|b| b.1).max().unwrap_or(1).max(1);
    let n = bars.len().max(1) as f32;
    let bw = ((r.w - 60.0) / n).min(90.0);
    let x0 = r.center().x - bw * n * 0.5;
    let base = r.bottom() - 50.0;
    for (k, (label, v)) in bars.iter().enumerate() {
        let bh = *v as f32 / most as f32 * (r.h - 130.0);
        let b = Rect::new(x0 + k as f32 * bw + 10.0, base - bh, bw - 20.0, bh.max(1.0));
        ui.p().rect(b, p.accent);
        ui.text_as_is(&format!("{v} ({:.0}%)", *v as f64 * 100.0 / total as f64), Vec2::new(b.center().x, b.y - 6.0), 12.0, Weight::Bold, p.text, Align::Center);
        ui.text_in(label, Rect::new(b.x - 8.0, base + 6.0, b.w + 16.0, 18.0), 12.0, Weight::Medium, p.dim, Align::Center);
    }
    if close {
        l.company.planning.rf.panel = Panel::None;
    }
    r
}

/// (Costs as words for the zone card: `eur` with its sign.)
#[allow(dead_code)]
fn signed(v: Cents) -> String {
    if v >= 0 {
        format!("+{}", eur(v))
    } else {
        format!("-{}", eur(-v))
    }
}
