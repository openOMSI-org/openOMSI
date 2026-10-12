//! RosterFlow's Gantt (`rosterflow`): the day as Optibus's scheduling module draws it. A row
//! per bus's block (Vehicles) or per driver's duty (Drivers): a checkbox, a red "!" when the
//! row breaks a rule, its ID; then on the hour axis its trips as saturated chips in the line's
//! colour, pointed the way they run (right outbound, left inbound), and every event in
//! Optibus's symbols - deadheads as small dark boxes (an orange dot where the time is
//! estimated), pull-outs and pull-ins as grey-blue boxes, the pre- and post-trip checks and the
//! sign-on and sign-off as T-bars, the reliefs as arrows (down: the driver gets off on
//! arrival, up: the next one gets on at departure, both ways: handed over in the layover, a
//! grey line while the bus is attended), splits as a line with "Split", travel as an amber
//! box with W, P or R, custom work as a white box, linked trips striped, a trip moved off its
//! timetable tour with a dot.
//!
//! A click on a chip chooses it and opens its card; Shift and a click widen the choice along
//! the row; a double click (or MOVE, or a drag) takes it up, and the rows it may go to show
//! drop zones rated with stars, blue where nothing breaks, red where a rule does, with the
//! cost they make. A right click opens the menu (Move, Move to new row, Unassign, Swap, Swap
//! from here, View in timeplan, Delete ...). A gap between two trips at a terminus or the
//! depot is clicked to cut a duty there; the relief arrows are clicked to join it again.

use super::super::super::ui::{id_of, Key, Ui};
use super::super::super::Launcher;
use super::rf_brand::{arrow_chip, arrow_points};
use super::rosterflow::{self as shell, block_row, duty_row, edit, Cached, Card, Drop, Hand, Menu, Pal, RowId, Stat, Zone};
use glam::Vec2;
use omsi_launcher_lib::company::network::{PlannedTrip, TourOfDay};
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, EvKind, ReliefTiming, Shaped};
use omsi_launcher_lib::company::studio::{self as st, Target};
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// A row's pitch, a chip's height, the ID column, a stats column, the axis band.
pub(super) const ROW: f32 = 27.0;
pub(super) const CHIP: f32 = 18.0;
pub(super) const ID_W: f32 = 96.0;
pub(super) const STAT_W: f32 = 64.0;
pub(super) const AXIS: f32 = 28.0;

/// A line's colour (`co::rosterflow::line_colour`).
pub(super) fn line_colour(c: &Company, line: &str) -> Color {
    let [r, g, b] = rf::line_colour(c, line);
    Color::rgba(r, g, b, 1.0)
}

/// Ink on a chip of colour `c`: dark on light colours, white on dark.
pub(super) fn ink_on(c: Color) -> Color {
    super::super::super::ownlines::ink_on(c)
}

/// What a row shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Kind {
    Block(usize),
    Duty(usize),
}

/// A row as laid out: what, its ID, pinned or not.
#[derive(Clone, Debug)]
pub(super) struct Row {
    pub kind: Kind,
    pub id: RowId,
    pub pinned: bool,
}

/// The ID a bus's block goes by: its line's number and its name ("1/02").
pub(super) fn block_label(t: &TourOfDay) -> String {
    let tour = t.tour.trim();
    let tour = if tour.len() == 1 && tour.chars().all(|c| c.is_ascii_digit()) { format!("0{tour}") } else { tour.to_string() };
    format!("{}/{}", t.number.trim(), tour)
}

/// Whether a trip runs outbound: from where the line's first trip of the day leaves.
pub(super) fn outbound(tours: &[TourOfDay], line: &str, x: &PlannedTrip) -> bool {
    let home = tours.iter().filter(|t| t.line.eq_ignore_ascii_case(line)).flat_map(|t| t.trips.iter()).filter(|y| y.counts()).min_by_key(|y| y.dep).map(|y| y.from.trim().to_string());
    home.is_none_or(|h| x.from.trim().eq_ignore_ascii_case(&h))
}

/// The day's first and last minute, to whole hours.
pub(super) fn day_span(board: &Board) -> (i32, i32) {
    let t0 = board.tours.iter().map(|t| t.from()).chain(board.shaped.iter().map(|s| s.start)).min().unwrap_or(300) - 20;
    let t1 = board.tours.iter().map(|t| t.to()).chain(board.shaped.iter().map(|s| s.end)).max().unwrap_or(1380) + 20;
    let t0 = t0.div_euclid(60) * 60;
    let t1 = (t1 + 59).div_euclid(60) * 60;
    (t0, t1.max(t0 + 180))
}

/// The value of a stats column for a row (None: nothing to show), and its words.
pub(super) fn stat_of(c: &Company, board: &Board, kind: Kind, s: Stat) -> Option<(f64, String)> {
    let hm = rf::hm;
    match kind {
        Kind::Block(i) => {
            let t = board.tours.get(i)?;
            Some(match s {
                Stat::Start => (t.from() as f64, rf::clock(t.from())),
                Stat::End => (t.to() as f64, rf::clock(t.to())),
                Stat::Spread => ((t.to() - t.from()) as f64, hm(t.to() - t.from())),
                Stat::Driving => {
                    let d: i32 = t.trips.iter().map(|x| x.arr - x.dep).sum();
                    (d as f64, hm(d))
                }
                Stat::Km => (t.km(), format!("{:.0}", t.km())),
                Stat::Trips => {
                    let n = t.trips.iter().filter(|x| x.counts()).count();
                    (n as f64, n.to_string())
                }
                Stat::Paid | Stat::Type => return None,
            })
        }
        Kind::Duty(d) => {
            let x = board.shaped.get(d)?;
            Some(match s {
                Stat::Start => (x.start as f64, rf::clock(x.start)),
                Stat::End => (x.end as f64, rf::clock(x.end)),
                Stat::Spread => (x.spread() as f64, hm(x.spread())),
                Stat::Paid => (x.paid as f64, hm(x.paid)),
                Stat::Driving => (x.driving as f64, hm(x.driving)),
                Stat::Km => {
                    let km: f64 = x.spans.iter().map(|sp| piece_trips(board, sp.tour, sp.duty).iter().map(|t| t.km).sum::<f64>()).sum();
                    (km, format!("{km:.0}"))
                }
                Stat::Trips => {
                    let n: usize = x.spans.iter().map(|sp| piece_trips(board, sp.tour, sp.duty).iter().filter(|t| t.counts()).count()).sum();
                    (n as f64, n.to_string())
                }
                Stat::Type => (x.kind.map(|k| k as f64).unwrap_or(99.0), omsi_ui::tr(x.kind_name(&c.planning.flow.prefs)).into_owned()),
            })
        }
    }
}

/// The trips of piece `duty` of block `tour`.
pub(super) fn piece_trips(board: &Board, tour: usize, duty: usize) -> &[PlannedTrip] {
    let Some(t) = board.tours.get(tour) else { return &[] };
    let r = co::network::duties_of(t).get(duty).cloned().unwrap_or(0..0);
    t.trips.get(r).unwrap_or(&[])
}

/// The rows as shown: pinned first, then (filtered) by line and start, or as a stats column
/// sorts them.
pub(super) fn rows(l: &Launcher, k: &Cached, board: &Board, drivers: bool) -> Vec<Row> {
    let s = &l.company.planning.rf;
    let c = &k.view;
    let f = &s.filter;
    let issue_rows: Option<Vec<(rf::Scope, usize)>> = f.issues.as_ref().map(|keys| board.issues.iter().filter(|i| keys.contains(&i.key)).filter_map(|i| i.row.map(|r| (i.scope, r))).collect());
    let mut out: Vec<Row> = Vec::new();
    if drivers {
        for (d, x) in board.shaped.iter().enumerate() {
            if let Some(ir) = &issue_rows {
                if !ir.contains(&(rf::Scope::Duty, d)) {
                    continue;
                }
            }
            if f.locked_only && !x.locked {
                continue;
            }
            if !f.hidden.is_empty() && !x.spans.is_empty() && x.spans.iter().all(|sp| f.hidden.iter().any(|h| h.eq_ignore_ascii_case(&board.tours[sp.tour].line))) {
                continue;
            }
            if x.end < f.from || x.start > f.to {
                continue;
            }
            out.push(Row { kind: Kind::Duty(d), id: duty_row(x), pinned: false });
        }
    } else {
        let line_at = |l: &str| c.lines.iter().position(|x| x.name.eq_ignore_ascii_case(l)).unwrap_or(usize::MAX);
        let mut idx: Vec<usize> = (0..board.tours.len()).collect();
        idx.sort_by_key(|&i| (line_at(&board.tours[i].line), board.tours[i].from(), i));
        for i in idx {
            let t = &board.tours[i];
            if let Some(ir) = &issue_rows {
                if !ir.contains(&(rf::Scope::Vehicle, i)) {
                    continue;
                }
            }
            if f.hidden.iter().any(|h| h.eq_ignore_ascii_case(&t.line)) {
                continue;
            }
            if f.locked_only && !rf::block_locked(c, board.weekday, &t.line, &t.tour) {
                continue;
            }
            if t.to() < f.from || t.from() > f.to {
                continue;
            }
            if f.dir != 0 && !t.trips.iter().any(|x| x.counts() && outbound(&board.tours, &t.line, x) == (f.dir == 1)) {
                continue;
            }
            out.push(Row { kind: Kind::Block(i), id: block_row(t), pinned: false });
        }
    }
    if let Some((stat, asc)) = s.sort {
        out.sort_by(|a, b| {
            let va = stat_of(c, board, a.kind, stat).map(|v| v.0).unwrap_or(f64::MAX);
            let vb = stat_of(c, board, b.kind, stat).map(|v| v.0).unwrap_or(f64::MAX);
            let o = va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal);
            if asc {
                o
            } else {
                o.reverse()
            }
        });
    }
    // (the pinned on top, in the order pinned)
    let mut top: Vec<Row> = Vec::new();
    for id in &s.pinned {
        if let Some(i) = out.iter().position(|r| r.id == *id) {
            let mut r = out.remove(i);
            r.pinned = true;
            top.push(r);
        }
    }
    top.extend(out);
    top
}

pub(super) fn shown_rows(l: &Launcher, k: &Cached, board: &Board, drivers: bool) -> usize {
    rows(l, k, board, drivers).len()
}

// --- drawing -----------------------------------------------------------------------------------

/// A trip's chip: the line's colour, pointed the way it runs, its words as the width allows;
/// `outline` (chosen white, an issue red), `stripes` linked, `dot` moved off its tour.
#[allow(clippy::too_many_arguments)]
pub(super) fn chip(ui: &mut Ui, r: Rect, colour: Color, right: bool, label: &str, outline: Option<Color>, stripes: bool, dot: bool, alpha: f32) {
    let fill = colour.alpha(alpha);
    arrow_chip(ui, r, fill, right);
    if stripes {
        ui.push_clip(r, 3.0);
        let mut x = r.x - r.h;
        while x < r.right() {
            ui.p().convex(&[Vec2::new(x, r.bottom()), Vec2::new(x + 3.0, r.bottom()), Vec2::new(x + 3.0 + r.h, r.y), Vec2::new(x + r.h, r.y)], Color::rgba(0, 0, 0, 0.22 * alpha));
            x += 8.0;
        }
        ui.pop_clip();
    }
    if let Some(o) = outline {
        ui.p().stroke(&arrow_points(r.inset(-1.0), right), 1.6, o);
    }
    if dot {
        let x = if right { r.x + 5.0 } else { r.right() - 5.0 };
        ui.p().circle(Vec2::new(x, r.center().y), 2.6, Color::rgba(10, 12, 16, alpha));
    }
    let px = 11.0;
    let tip = (r.h * 0.45).min(r.w * 0.45);
    let room = r.w - tip - 4.0;
    if !label.is_empty() && room > 8.0 {
        let ink = ink_on(colour).alpha(alpha);
        let text_r = if right { Rect::new(r.x + 2.0, r.y, room, r.h) } else { Rect::new(r.x + tip + 2.0, r.y, room, r.h) };
        if ui.width(label, px, Weight::Bold) <= room {
            ui.text_as_is(label, Vec2::new(text_r.center().x, r.center().y + 4.0), px, Weight::Bold, ink, Align::Center);
        } else if room > 18.0 {
            // (cut, as Optibus does: "2...")
            let mut s: String = label.chars().take(((room / 8.0) as usize).max(1)).collect();
            s.push('…');
            ui.text_as_is(&s, Vec2::new(text_r.center().x, r.center().y + 4.0), px, Weight::Bold, ink, Align::Center);
        }
    }
}

/// A deadhead: a small dark rounded box (an orange dot where its time is estimated).
pub(super) fn deadhead(ui: &mut Ui, p: &Pal, a: f32, b: f32, cy: f32, estimated: bool) {
    let w = (b - a).max(9.0);
    let r = Rect::new(a + ((b - a) - w).min(0.0) * 0.5, cy - CHIP * 0.42, w, CHIP * 0.84);
    ui.p().rounded(r, 3.0, p.dead);
    ui.p().rounded_border(r, 3.0, 1.0, Color::rgba(255, 255, 255, 0.10));
    if estimated {
        ui.p().circle(r.center(), 3.0, p.warn);
    }
}

/// A pull-out or pull-in: a grey-blue box (an orange dot where the company adds it itself).
pub(super) fn pull(ui: &mut Ui, p: &Pal, a: f32, b: f32, cy: f32, generated: bool) {
    let w = (b - a).max(12.0);
    let r = Rect::new(if b - a < 12.0 { b - 12.0 } else { a }, cy - CHIP * 0.5, w, CHIP);
    ui.p().rounded(r, 3.0, p.pull);
    if generated {
        ui.p().circle(r.center(), 3.0, p.warn);
    }
}

/// A T-bar: the pre- or post-trip check, a sign-on or a sign-off (`left`: its bar on the left).
pub(super) fn t_bar(ui: &mut Ui, p: &Pal, a: f32, b: f32, cy: f32, left: bool) {
    let col = if p.light { p.dim } else { Color::rgba(200, 206, 214, 1.0) };
    let a = a.min(b - 6.0);
    ui.p().rect(Rect::new(a, cy - 0.75, b - a, 1.5), col);
    let x = if left { a } else { b - 1.5 };
    ui.p().rect(Rect::new(x, cy - 7.0, 1.5, 14.0), col);
}

/// A relief arrow: down (the driver gets off), up (gets on), or both (handed over).
pub(super) fn relief_arrow(ui: &mut Ui, p: &Pal, x: f32, cy: f32, up: bool, down: bool) {
    let col = if p.light { p.text } else { Color::WHITE };
    let h = CHIP * 0.62;
    ui.p().rect(Rect::new(x - 0.6, cy - h, 1.2, h * 2.0), col);
    if down {
        ui.p().convex(&[Vec2::new(x - 3.5, cy + h - 4.0), Vec2::new(x + 3.5, cy + h - 4.0), Vec2::new(x, cy + h + 1.0)], col);
    }
    if up {
        ui.p().convex(&[Vec2::new(x - 3.5, cy - h + 4.0), Vec2::new(x, cy - h - 1.0), Vec2::new(x + 3.5, cy - h + 4.0)], col);
    }
}

/// The words on a trip's chip as the Layers ask: the line, the times, the stops.
pub(super) fn chip_label(t: &TourOfDay, x: &PlannedTrip, layers: u8) -> String {
    match layers {
        1 => format!("{:02} > {:02}", x.dep.rem_euclid(60), x.arr.rem_euclid(60)),
        2 => format!("{} > {}", co::rosterpdf::stop_letters(&x.from), co::rosterpdf::stop_letters(x.end())),
        _ => {
            if x.line.trim().is_empty() {
                t.number.trim().to_string()
            } else {
                x.line.trim().to_string()
            }
        }
    }
}

/// What the player did in the Gantt this frame (done once it is drawn).
enum Do {
    Choose { tour: usize, trip: usize, row: RowId, shift: bool, at: Vec2 },
    Pick(Hand, bool),
    DropZone(usize),
    Menu(Menu),
    Card(Card),
    Tick(RowId),
    TickAll(Vec<RowId>),
    Lock(Kind),
    Pin(RowId),
    Swap(usize),
    Cut(usize, usize),
    Uncut(usize, usize),
    Duty(RowId),
    StackChoose(usize, Vec2),
    Cancel,
    Spare,
}

/// The Gantt in `r`: the axis, the stack (when there is one), the rows, the new row, what is
/// carried; the clicks done once it is drawn.
pub(super) fn gantt(l: &mut Launcher, r: Rect, k: &Cached, board: &Board, p: &Pal, drivers: bool, read_only: bool) {
    let c: &Company = &k.view;
    let rows_now = rows(l, k, board, drivers);
    let s = &l.company.planning.rf;
    let stats = s.stats.clone();
    let sort = s.sort;
    let layers = s.layers;
    let hand = s.hand.clone();
    let zones = s.zones.clone();
    let sel = s.sel.clone();
    let sel_row = s.sel_row.clone();
    let ticked = s.ticked.clone();
    let reveal = s.reveal.clone();
    let dragging = s.dragging;
    let wd = board.weekday;
    let prefs = c.planning.flow.prefs.clone();
    let ui = &mut l.ui;
    ui.p().rect(r, p.grid);
    // the columns: the IDs, the stats, the hours
    let lw = ID_W + stats.len() as f32 * STAT_W;
    let gx = r.x + lw + 6.0;
    let gw = (r.w - lw - 18.0).max(60.0);
    let (t0, t1) = day_span(board);
    let total = (t1 - t0) as f32;
    let st_ = &mut l.company.planning.rf;
    let over = r.contains(ui.input.mouse) && !ui.over_popup();
    let stack_rows = if !drivers && (!board.stack.is_empty() || matches!(hand, Some(Hand::Trips { .. }))) { stack_lanes(board).max(1) } else { 0 };
    let stack_h = if stack_rows > 0 { stack_rows as f32 * ROW + 26.0 } else { 0.0 };
    // (zoomed in: the bar along the day under the rows)
    let hbar_h = if st_.zoom > 1.001 { super::rf_scroll::BAR } else { 0.0 };
    let body = Rect::new(r.x, r.y + AXIS + stack_h, r.w, (r.h - AXIS - stack_h - hbar_h).max(0.0));
    let content_h = (rows_now.len() as f32 + 2.0) * ROW;
    // the zoom (Ctrl and the wheel), along the day (Shift and the wheel), up and down (the wheel)
    if over && ui.input.ctrl && ui.input.wheel.y.abs() > 0.0 {
        let at = t0 as f32 + st_.pan as f32 + (ui.input.mouse.x - gx) / (gw / (total / st_.zoom));
        st_.zoom = (st_.zoom * if ui.input.wheel.y > 0.0 { 1.25 } else { 0.8 }).clamp(1.0, 8.0);
        let ppm = gw / (total / st_.zoom);
        st_.pan = (at - t0 as f32 - (ui.input.mouse.x - gx) / ppm).round() as i32;
        ui.input.wheel = Vec2::ZERO;
    } else if over && (ui.input.shift || ui.input.wheel.x.abs() > 0.0) && ui.input.wheel.length() > 0.0 {
        let d = if ui.input.wheel.x.abs() > 0.0 { -ui.input.wheel.x } else { -ui.input.wheel.y };
        st_.pan += (d * total / st_.zoom / 12.0).round() as i32;
        ui.input.wheel = Vec2::ZERO;
    } else if over && ui.input.wheel.y.abs() > 0.0 {
        st_.scroll -= ui.input.wheel.y * ROW * 2.0;
        ui.input.wheel = Vec2::ZERO;
    }
    let visible = total / st_.zoom;
    st_.pan = st_.pan.clamp(0, (total - visible).max(0.0) as i32);
    st_.scroll = st_.scroll.clamp(0.0, (content_h - body.h).max(0.0));
    // (a row asked for: scrolled into view)
    if let Some(id) = &reveal {
        if let Some(i) = rows_now.iter().position(|x| x.id == *id) {
            let y = i as f32 * ROW;
            if y < st_.scroll || y + ROW > st_.scroll + body.h {
                st_.scroll = (y - body.h * 0.3).clamp(0.0, (content_h - body.h).max(0.0));
            }
        }
        st_.reveal = None;
    }
    let (pan, scroll) = (st_.pan, st_.scroll);
    let ppm = gw / visible;
    let x_of = move |m: i32| gx + (m - t0 - pan) as f32 * ppm;
    let mouse = ui.input.mouse;
    let released = ui.input.released;
    let mut todo: Vec<Do> = Vec::new();
    let mut hovered_zone: Option<usize> = None;
    // the hour bands, every other one lighter, under everything
    ui.push_clip(Rect::new(gx, r.y, gw, r.h), 0.0);
    for h in (t0 / 60)..(t1 / 60) {
        if h % 2 == 1 {
            let (a, b) = (x_of(h * 60), x_of(h * 60 + 60));
            ui.p().rect(Rect::new(a, r.y + AXIS, b - a, r.h - AXIS), p.band);
        }
    }
    ui.pop_clip();
    // the axis: the hours ("04:00"), the header of the IDs and the stats
    ui.p().rect(Rect::new(r.x, r.y, r.w, AXIS), p.axis);
    ui.p().rect(Rect::new(r.x, r.y + AXIS - 1.0, r.w, 1.0), p.line);
    ui.p().rect(Rect::new(r.x, r.y + AXIS, lw, r.h - AXIS), p.labels);
    let every = if ppm * 60.0 >= 50.0 { 1 } else if ppm * 60.0 >= 25.0 { 2 } else { 3 };
    ui.push_clip(Rect::new(gx - 24.0, r.y, gw + 30.0, AXIS), 0.0);
    for h in (t0 / 60..=t1 / 60).filter(|h| h % every == 0) {
        let x = x_of(h * 60);
        ui.text_as_is(&format!("{:02}:00", h.rem_euclid(24)), Vec2::new(x, r.y + AXIS * 0.5 + 4.0), 11.0, Weight::Medium, p.dim, Align::Center);
    }
    ui.pop_clip();
    // the header's checkbox (all rows); "ID", or the ticked rows' count
    let all_ids: Vec<RowId> = rows_now.iter().map(|x| x.id.clone()).collect();
    let all_ticked = !all_ids.is_empty() && all_ids.iter().all(|id| ticked.contains(id));
    if checkbox(ui, p, "rf-tick-all", Vec2::new(r.x + 14.0, r.y + AXIS * 0.5), all_ticked) {
        todo.push(Do::TickAll(if all_ticked { Vec::new() } else { all_ids.clone() }));
    }
    if ticked.is_empty() || read_only {
        ui.text_as_is("ID", Vec2::new(r.x + 40.0, r.y + AXIS * 0.5 + 4.0), 11.0, Weight::Bold, p.dim, Align::Left);
    }
    let mut sort_click: Option<Stat> = None;
    for (k2, stat) in stats.iter().enumerate() {
        let hr = Rect::new(r.x + ID_W + k2 as f32 * STAT_W, r.y, STAT_W, AXIS);
        let (h, _, click) = ui.interact(id_of(&format!("rf-stat-head-{k2}")), hr);
        let on = sort.is_some_and(|s| s.0 == *stat);
        ui.text_in(stat.label(), hr.pad(4.0, 0.0), 10.0, Weight::Bold, if on || h { p.accent } else { p.dim }, Align::Center);
        ui.tooltip(hr, "Click to sort by it, again the other way, a third time back");
        if click {
            sort_click = Some(*stat);
        }
    }
    // the stack (Optibus's edit mode: the trips no bus runs, above the schedule)
    if stack_rows > 0 {
        let sr = Rect::new(r.x, r.y + AXIS, r.w, stack_h);
        if !board.stack.is_empty() {
            super::super::super::tour::anchor("rf-stack", sr);
        }
        ui.p().rect(sr, p.labels.mix(p.grid, 0.5));
        ui.p().rect(Rect::new(sr.x, sr.bottom() - 3.0, sr.w, 3.0), p.line);
        ui.icon("inventory_2", Vec2::new(r.x + 16.0, sr.y + 13.0), 15.0, p.dim);
        let words = omsi_ui::tr("Stack: unassigned trips").to_uppercase();
        ui.text_as_is(&words, Vec2::new(r.x + 30.0, sr.y + 17.0), 10.5, Weight::Bold, p.dim, Align::Left);
        ui.push_clip(Rect::new(gx - 4.0, sr.y, gw + 8.0, sr.h), 0.0);
        let lanes = lanes_of(board);
        for (k2, (line, x)) in board.stack.iter().enumerate() {
            let lane = lanes.get(k2).copied().unwrap_or(0).min(3);
            let cy = sr.y + 22.0 + lane as f32 * ROW + ROW * 0.5;
            let (a, b) = (x_of(x.dep), x_of(x.arr));
            let cr = Rect::new(a + 1.0, cy - CHIP * 0.5, (b - a - 2.0).max(6.0), CHIP);
            let colour = line_colour(c, line);
            let taken = hand == Some(Hand::Stack(k2));
            let (h, _, click) = ui.interact(id_of(&format!("rf-stack-{k2}")), cr);
            chip(ui, cr, colour, outbound(&board.tours, line, x), x.line.trim(), if h || taken { Some(Color::WHITE) } else { None }, false, false, if taken { 0.45 } else { 1.0 });
            if click && !read_only && hand.is_none() {
                todo.push(Do::StackChoose(k2, mouse));
            }
        }
        // (where trips taken up go back onto the stack)
        if let (Some(Hand::Trips { .. }), Some(zs)) = (&hand, &zones) {
            if let Some(zi) = zs.iter().position(|z| z.drop == Drop::Stack) {
                let zr = Rect::new(gx, sr.y + 22.0, gw, sr.h - 26.0);
                let on = zr.contains(mouse);
                ui.p().rounded_border(zr.inset(2.0), 4.0, 1.4, if on { p.accent } else { p.accent.alpha(0.45) });
                if board.stack.is_empty() {
                    ui.text_in("Drop here to unassign: the trips wait on the stack", zr, 12.0, Weight::Medium, p.accent, Align::Center);
                }
                let (_, _, click) = ui.interact(id_of("rf-zone-stack"), zr);
                if on {
                    hovered_zone = Some(zi);
                }
                if click || (on && released && dragging) {
                    todo.push(Do::DropZone(zi));
                }
            }
        }
        ui.pop_clip();
    }
    // the rows
    ui.push_clip(body, 0.0);
    let mut y = body.y - scroll;
    let mut lit: Option<(i32, i32)> = None;
    for row in rows_now.iter() {
        let rr = Rect::new(r.x, y, r.w, ROW);
        y += ROW;
        if !ui.rect_visible(rr) {
            continue;
        }
        let cy = rr.center().y;
        // (the tour's row: the first one wholly in view)
        if rr.y >= body.y && rr.bottom() <= body.bottom() {
            super::super::super::tour::anchor("rf-row", Rect::new(rr.x, rr.y, rr.w - 12.0, rr.h));
            super::super::super::tour::anchor("rf-row-id", Rect::new(rr.x + 4.0, rr.y, ID_W - 8.0, rr.h));
        }
        let row_on = sel_row.as_ref() == Some(&row.id);
        let rh = rr.contains(mouse) && body.contains(mouse);
        if row_on || rh {
            ui.p().rect(Rect::new(gx - 6.0, rr.y, gw + 12.0, rr.h), p.text.alpha(if row_on { 0.06 } else { 0.03 }));
        }
        if row.pinned {
            ui.p().rect(Rect::new(r.x, rr.bottom() - 1.0, r.w, 1.0), p.accent.alpha(0.35));
        }
        // the ID column: the checkbox, the red "!", the ID; the lock and the pin on hover
        let (issues_hard, issues_any, locked, label, tip) = match row.kind {
            Kind::Block(i) => {
                let t = &board.tours[i];
                let any: Vec<&rf::Issue> = board.block_issues(i).filter(|x| !rf::dismissed(c, wd, &x.key)).collect();
                let tip = any.iter().map(|x| rf::issue_text(x, &|s| omsi_ui::tr(s).into_owned())).collect::<Vec<_>>().join("\n");
                (any.iter().any(|x| x.hard), !any.is_empty(), rf::block_locked(c, wd, &t.line, &t.tour), block_label(t), tip)
            }
            Kind::Duty(d) => {
                let x = &board.shaped[d];
                let any: Vec<&rf::Issue> = x.issues.iter().map(|&i| &board.issues[i]).filter(|i| !rf::dismissed(c, wd, &i.key)).collect();
                let tip = any.iter().map(|x| rf::issue_text(x, &|s| omsi_ui::tr(s).into_owned())).collect::<Vec<_>>().join("\n");
                (any.iter().any(|x| x.hard), !any.is_empty(), x.locked, x.label(d), tip)
            }
        };
        let tick = ticked.contains(&row.id);
        if checkbox(ui, p, &format!("rf-tick-{:?}", row.id), Vec2::new(r.x + 14.0, cy), tick) {
            todo.push(Do::Tick(row.id.clone()));
        }
        if issues_any {
            let col = if issues_hard { p.err } else { p.warn };
            ui.text_as_is("!", Vec2::new(r.x + 26.0, cy + 5.0), 14.0, Weight::Black, col, Align::Left);
            ui.tooltip(Rect::new(r.x + 23.0, rr.y, 11.0, rr.h), &tip);
        }
        let idr = Rect::new(r.x + 36.0, rr.y, ID_W - 60.0, rr.h);
        let (idh, _, id_click) = ui.interact(id_of(&format!("rf-id-{:?}", row.id)), idr);
        let id_col = if row_on { p.accent } else if idh { p.text } else { p.text.alpha(0.88) };
        if let Kind::Block(i) = row.kind {
            let colour = line_colour(c, &board.tours[i].line);
            ui.swatch(Rect::new(r.x + 36.0, cy - 6.0, 3.0, 12.0), 1.0, colour);
        }
        ui.text_as_is(&label, Vec2::new(r.x + 43.0, cy + 4.5), 12.5, Weight::Bold, id_col, Align::Left);
        if id_click && !read_only {
            match row.kind {
                Kind::Block(i) => todo.push(Do::Card(Card::Vehicle { tour: i, at: Vec2::new(idr.right(), cy) })),
                Kind::Duty(_) => todo.push(Do::Duty(row.id.clone())),
            }
        }
        // (the lock: shown when locked, else on hover; the pin on hover)
        if (locked || rh) && !read_only {
            let lc = Vec2::new(r.x + ID_W - 30.0, cy);
            let lr = Rect::new(lc.x - 9.0, cy - 9.0, 18.0, 18.0);
            let (lh, _, lclick) = ui.interact(id_of(&format!("rf-lock-{:?}", row.id)), lr);
            ui.icon(if locked { "lock" } else { "livery_unlock" }, lc, 14.0, if locked { p.accent } else if lh { p.text } else { p.faint });
            ui.tooltip(lr, if locked { "Locked: Optimise leaves it and no edit changes it. Click to unlock" } else { "Lock it: Optimise leaves it as it is" });
            if lclick {
                todo.push(Do::Lock(row.kind));
            }
        }
        if (row.pinned || rh) && !read_only {
            let pc = Vec2::new(r.x + ID_W - 12.0, cy);
            let pr = Rect::new(pc.x - 9.0, cy - 9.0, 18.0, 18.0);
            let (ph, _, pclick) = ui.interact(id_of(&format!("rf-pin-{:?}", row.id)), pr);
            ui.icon("location_on", pc, 14.0, if row.pinned { p.accent } else if ph { p.text } else { p.faint });
            ui.tooltip(pr, if row.pinned { "Pinned to the top. Click to unpin" } else { "Pin it to the top (P)" });
            if pclick {
                todo.push(Do::Pin(row.id.clone()));
            }
        }
        for (k2, stat) in stats.iter().enumerate() {
            if let Some((_, words)) = stat_of(c, board, row.kind, *stat) {
                let sr = Rect::new(r.x + ID_W + k2 as f32 * STAT_W, rr.y, STAT_W, rr.h);
                ui.text_as_is(&words, Vec2::new(sr.center().x, cy + 4.0), 11.5, Weight::Medium, p.text.alpha(0.85), Align::Center);
            }
        }
        // the swap's target: a click on a bus of the line
        if let (Some(Hand::Swap { tour, .. }), Kind::Block(i)) = (&hand, row.kind) {
            if i != *tour && board.tours[i].line.eq_ignore_ascii_case(&board.tours[*tour].line) {
                let zr = Rect::new(gx, rr.y + 1.0, gw, rr.h - 2.0);
                let on = zr.contains(mouse);
                ui.p().rounded_border(zr, 3.0, 1.2, if on { p.accent } else { p.accent.alpha(0.3) });
                let (_, _, click) = ui.interact(id_of(&format!("rf-swap-{i}")), zr);
                if click {
                    todo.push(Do::Swap(i));
                }
            }
        }
        ui.push_clip(Rect::new(gx - 4.0, rr.y, gw + 8.0, rr.h), 0.0);
        match row.kind {
            Kind::Block(i) => block_row_draw(ui, p, c, board, i, &row.id, cy, &x_of, &prefs, &hand, &sel, layers, read_only, mouse, &mut todo, &mut lit),
            Kind::Duty(d) => duty_row_draw(ui, p, c, board, d, &row.id, cy, &x_of, &prefs, &hand, &sel, layers.label, read_only, mouse, &mut todo, &mut lit),
        }
        // the drop zones in this row
        if let (Some(zs), Some((a, b, right))) = (&zones, hand_span(board, hand.as_ref())) {
            for (zi, z) in zs.iter().enumerate() {
                let here = match (z.drop, row.kind) {
                    (Drop::Block(bi), Kind::Block(i)) => bi == i,
                    (Drop::Duty(d), Kind::Duty(e)) => d == e,
                    _ => false,
                };
                if !here {
                    continue;
                }
                let zr = Rect::new(x_of(a) + 1.0, cy - CHIP * 0.5 - 1.0, (x_of(b) - x_of(a) - 2.0).max(30.0), CHIP + 2.0);
                let on = zr.contains(mouse);
                if on {
                    hovered_zone = Some(zi);
                }
                draw_zone(ui, p, zr, z, right, on);
                let (_, _, click) = ui.interact(id_of(&format!("rf-zone-{zi}")), zr);
                if click || (on && released && dragging) {
                    todo.push(Do::DropZone(zi));
                }
            }
        }
        ui.pop_clip();
    }
    // the new row: a new bus or a new duty
    if !read_only {
        let rr = Rect::new(r.x, y + 4.0, r.w, ROW);
        if ui.rect_visible(rr) {
            if rr.bottom() <= body.bottom() {
                super::super::super::tour::anchor("rf-new-row", Rect::new(rr.x, rr.y, (gx - r.x + gw.min(560.0)).min(rr.w - 12.0), rr.h));
            }
            let label = if drivers { "New duty" } else { "New vehicle" };
            ui.icon("add", Vec2::new(r.x + 22.0, rr.center().y), 15.0, p.dim);
            ui.text_in(label, Rect::new(r.x + 36.0, rr.y, (gx - r.x - 40.0).max(60.0), rr.h), 12.0, Weight::Medium, p.dim, Align::Left);
            let want = if drivers { Drop::NewDuty } else { Drop::New };
            let zone = zones.as_ref().and_then(|zs| zs.iter().position(|z| z.drop == want));
            match (zone, hand_span(board, hand.as_ref())) {
                (Some(zi), Some((a, b, right))) => {
                    let z = &zones.as_ref().unwrap()[zi];
                    let zr = Rect::new(x_of(a) + 1.0, rr.center().y - CHIP * 0.5 - 1.0, (x_of(b) - x_of(a) - 2.0).max(30.0), CHIP + 2.0);
                    let on = zr.contains(mouse);
                    if on {
                        hovered_zone = Some(zi);
                    }
                    ui.push_clip(Rect::new(gx - 4.0, rr.y, gw + 8.0, rr.h), 0.0);
                    draw_zone(ui, p, zr, z, right, on);
                    ui.pop_clip();
                    let (_, _, click) = ui.interact(id_of("rf-zone-new"), zr);
                    if click || (on && released && dragging) {
                        todo.push(Do::DropZone(zi));
                    }
                }
                _ if drivers && hand.is_none() => {
                    // (a spare duty from scratch: a click on the new row)
                    let lr = Rect::new(gx, rr.y + 2.0, gw.min(560.0), rr.h - 4.0);
                    let (h, _, click) = ui.interact(id_of("rf-new-duty"), lr);
                    ui.p().rounded_border(lr, 3.0, 1.0, if h { p.accent } else { p.line });
                    ui.text_in("Click for a spare duty of custom work, or drop a piece of work here to begin a duty with it", lr.pad(10.0, 0.0), 11.5, Weight::Regular, if h { p.text } else { p.faint }, Align::Left);
                    if click {
                        todo.push(Do::Spare);
                    }
                }
                _ => {}
            }
        }
    }
    ui.pop_clip();
    // the time chosen or under the mouse: a column down the chart (Optibus's highlight)
    if let Some((a, b)) = lit {
        let (xa, xb) = (x_of(a).max(gx), x_of(b).min(gx + gw));
        if xb > xa {
            ui.p().rect(Rect::new(xa, r.y + AXIS, xb - xa, r.h - AXIS), p.text.alpha(0.04));
            ui.p().rect(Rect::new(xa, r.y, xb - xa, AXIS), p.accent.alpha(0.18));
        }
    }
    // what a drag carries, at the mouse
    if dragging {
        if let Some((a, b, right)) = hand_span(board, hand.as_ref()) {
            let w = (x_of(b) - x_of(a)).max(20.0);
            let cr = Rect::new(mouse.x - w * 0.5, mouse.y - CHIP * 0.5, w, CHIP);
            let colour = hand.as_ref().map(|h| hand_colour(c, board, h)).unwrap_or(Color::WHITE);
            ui.p().shadow(cr, 4.0, 10.0, Color::rgba(0, 0, 0, 0.45));
            chip(ui, cr, colour, right, "", Some(Color::WHITE), false, false, 0.92);
        }
    }
    // the scrollbars: down the rows, and along the day once zoomed in (dragged, or a click on
    // the track beside the thumb moves a page)
    let look = super::rf_scroll::look(p);
    let vbar = Rect::new(r.right() - super::rf_scroll::BAR, body.y, super::rf_scroll::BAR, body.h);
    let to_scroll = super::rf_scroll::bar(ui, &look, "rf-gantt-v", vbar, true, scroll, body.h, content_h);
    let mut to_pan = None;
    if hbar_h > 0.0 {
        let strip = Rect::new(r.x, body.bottom(), r.w, hbar_h);
        ui.p().rect(strip, p.grid);
        ui.p().rect(Rect::new(r.x, strip.y, lw, strip.h), p.labels);
        ui.p().rect(Rect::new(r.x, strip.y, r.w, 1.0), p.line);
        to_pan = super::rf_scroll::bar(ui, &look, "rf-gantt-h", Rect::new(gx, strip.y, gw, strip.h), false, pan as f32, visible, total);
    }
    // the hand: pressed, carried once it moved, let go
    let input = ui.input.clone();
    let s = &mut l.company.planning.rf;
    if let Some(v) = to_scroll {
        s.scroll = v;
    }
    if let Some(v) = to_pan {
        s.pan = v.round() as i32;
    }
    s.zone_hover = hovered_zone;
    if let Some(st) = sort_click {
        s.sort = match s.sort {
            Some((x, true)) if x == st => Some((st, false)),
            Some((x, false)) if x == st => None,
            _ => Some((st, true)),
        };
    }
    if let Some((at, h)) = s.press.clone() {
        if !input.down {
            s.press = None;
        } else if (input.mouse - at).length() > 7.0 {
            s.press = None;
            todo.push(Do::Pick(h, true));
        }
    }
    if s.dragging && (input.released || !input.down) && !todo.iter().any(|d| matches!(d, Do::DropZone(_))) {
        todo.push(Do::Cancel);
    }
    apply(l, k, board, drivers, todo);
}

/// A row's checkbox; returns whether it was clicked.
fn checkbox(ui: &mut Ui, p: &Pal, name: &str, c: Vec2, on: bool) -> bool {
    let r = Rect::new(c.x - 6.5, c.y - 6.5, 13.0, 13.0);
    let (h, _, click) = ui.interact(id_of(name), r.inset(-3.0));
    if on {
        ui.p().rounded(r, 2.0, p.accent);
        ui.icon("check", c, 11.0, Color::WHITE);
    } else {
        ui.p().rounded_border(r, 2.0, 1.3, if h { p.text } else { p.dim });
    }
    click
}

/// The time a hand holds (its first departure, its last arrival) and which way it points.
pub(super) fn hand_span(board: &Board, h: Option<&Hand>) -> Option<(i32, i32, bool)> {
    match h? {
        Hand::Trips { tour, trips } | Hand::Swap { tour, trips, .. } => {
            let t = board.tours.get(*tour)?;
            let xs: Vec<&PlannedTrip> = trips.iter().filter_map(|&k| t.trips.get(k)).collect();
            let right = xs.first().is_none_or(|x| outbound(&board.tours, &t.line, x));
            Some((xs.iter().map(|x| x.dep).min()?, xs.iter().map(|x| x.arr).max()?, right))
        }
        Hand::Stack(k) => {
            let (line, x) = board.stack.get(*k)?;
            Some((x.dep, x.arr, outbound(&board.tours, line, x)))
        }
        Hand::Piece { tour, duty } => {
            let xs = piece_trips(board, *tour, *duty);
            Some((xs.first()?.dep, xs.iter().map(|x| x.arr).max()?, true))
        }
    }
}

fn hand_colour(c: &Company, board: &Board, h: &Hand) -> Color {
    match h {
        Hand::Trips { tour, .. } | Hand::Swap { tour, .. } | Hand::Piece { tour, .. } => board.tours.get(*tour).map(|t| line_colour(c, &t.line)).unwrap_or(Color::WHITE),
        Hand::Stack(k) => board.stack.get(*k).map(|s| line_colour(c, &s.0)).unwrap_or(Color::WHITE),
    }
}

/// A drop zone: an outlined slot, blue where nothing breaks and red where a rule does, its
/// stars inside.
pub(super) fn draw_zone(ui: &mut Ui, p: &Pal, r: Rect, z: &Zone, right: bool, on: bool) {
    let col = if z.clean { p.accent } else { p.err };
    let pts = arrow_points(r, right);
    ui.p().convex(&pts[..5], if on { col.alpha(0.30) } else { p.grid.alpha(0.9) });
    ui.p().stroke(&pts, if on { 2.2 } else { 1.6 }, col);
    let cx = r.center().x;
    ui.text_as_is(&z.stars.to_string(), Vec2::new(cx - 6.0, r.center().y + 4.5), 11.5, Weight::Bold, p.text, Align::Center);
    ui.icon("star", Vec2::new(cx + 6.0, r.center().y), 12.0, p.accent);
}

/// The lanes the stack's trips lie in (no two at once in one lane).
fn lanes_of(board: &Board) -> Vec<usize> {
    let mut ends: Vec<i32> = Vec::new();
    board
        .stack
        .iter()
        .map(|(_, x)| match ends.iter().position(|e| *e <= x.dep) {
            Some(k) => {
                ends[k] = x.arr;
                k
            }
            None => {
                ends.push(x.arr);
                ends.len() - 1
            }
        })
        .collect()
}

fn stack_lanes(board: &Board) -> usize {
    lanes_of(board).into_iter().max().map(|m| m + 1).unwrap_or(0).min(4)
}

/// A bus's block in its row.
#[allow(clippy::too_many_arguments)]
fn block_row_draw(ui: &mut Ui, p: &Pal, c: &Company, board: &Board, i: usize, row: &RowId, cy: f32, x_of: &dyn Fn(i32) -> f32, prefs: &rf::Prefs, hand: &Option<Hand>, sel: &[(usize, usize)], layers: shell::Layers, read_only: bool, mouse: Vec2, todo: &mut Vec<Do>, lit: &mut Option<(i32, i32)>) {
    let t = &board.tours[i];
    let colour = line_colour(c, &t.line);
    let n = t.trips.len();
    if n == 0 {
        return;
    }
    let wd = board.weekday;
    let pieces = co::network::duties_of(t);
    let issue_deps: Vec<i32> = board.block_issues(i).filter(|x| x.text == rf::I_LAYOVER).filter_map(|x| x.key.rsplit('/').next().and_then(|d| d.parse().ok())).collect();
    let line_trips: Vec<&PlannedTrip> = board.tours.iter().filter(|o| o.line.eq_ignore_ascii_case(&t.line)).flat_map(|o| o.trips.iter()).collect();
    // the pre-trip check, and a pull-out the timetable has none of (the company's estimate)
    let first = &t.trips[0];
    if !first.empty {
        pull(ui, p, x_of(first.dep - 8), x_of(first.dep) - 2.0, cy, true);
        t_bar(ui, p, x_of(first.dep - 8 - prefs.pre_trip), x_of(first.dep - 8) - 2.0, cy, true);
    } else {
        t_bar(ui, p, x_of(first.dep - prefs.pre_trip), x_of(first.dep) - 1.0, cy, true);
    }
    for (j, x) in t.trips.iter().enumerate() {
        let a = x_of(x.dep);
        let b = x_of(x.arr).max(a + 4.0);
        // the gap before it: a deadhead where the places differ, the layover's minutes
        if j > 0 {
            let prev = &t.trips[j - 1];
            if !prev.end().trim().eq_ignore_ascii_case(x.from.trim()) && !prev.empty && !x.empty {
                let m = st::travel(&line_trips, prev.end(), &x.from);
                deadhead(ui, p, x_of(prev.arr) + 1.0, x_of(prev.arr + m) - 1.0, cy, true);
            } else if layers.breaks && x.dep - prev.arr > 0 && x_of(x.dep) - x_of(prev.arr) > 18.0 {
                ui.text_as_is(&format!("{}m", x.dep - prev.arr), Vec2::new((x_of(prev.arr) + a) * 0.5, cy + 3.5), 9.5, Weight::Medium, p.dim, Align::Center);
            }
        }
        if x.empty || !x.counts() {
            if x.empty && (j == 0 || j + 1 == n) {
                pull(ui, p, a + 1.0, b - 1.0, cy, false);
            } else {
                deadhead(ui, p, a + 1.0, b - 1.0, cy, false);
            }
            continue;
        }
        let cr = Rect::new(a + 1.0, cy - CHIP * 0.5, (b - a - 2.0).max(5.0), CHIP);
        let right = outbound(&board.tours, &t.line, x);
        let chosen = sel.contains(&(i, j));
        let taken = match hand {
            Some(Hand::Trips { tour, trips }) | Some(Hand::Swap { tour, trips, .. }) => *tour == i && trips.contains(&j),
            _ => false,
        };
        let linked = !rf::linked(c, wd, &t.line, &st::TripRef::of(x)).is_empty();
        let moved = layers.moved && !x.origin.trim().is_empty() && x.origin.trim() != t.tour.trim();
        let problem = issue_deps.contains(&x.dep);
        let (h, _, _) = ui.interact(id_of(&format!("rf-trip-{i}-{j}")), cr);
        let outline = if chosen || (h && hand.is_none()) { Some(Color::WHITE) } else if problem { Some(p.err) } else { None };
        if chosen || (h && hand.is_none()) {
            *lit = Some((x.dep, x.arr));
        }
        chip(ui, cr, colour, right, &chip_label(t, x, layers.label), outline, linked, moved, if taken { 0.35 } else { 1.0 });
        if ui.rect_visible(cr) && cr.w > 24.0 {
            super::super::super::tour::anchor("rf-chip", cr);
        }
        if read_only || hand.is_some() {
            continue;
        }
        if h && ui.input.pressed {
            todo.push(Do::Choose { tour: i, trip: j, row: row.clone(), shift: ui.input.shift, at: mouse });
        }
        if h && ui.input.right_pressed {
            todo.push(Do::Menu(Menu { at: mouse, tour: i, trip: j, duty_row: None, piece: None, custom: None }));
        }
    }
    // the post-trip check, and a pull-in the timetable has none of
    let last = &t.trips[n - 1];
    if !last.empty {
        pull(ui, p, x_of(last.arr) + 2.0, x_of(last.arr + 8), cy, true);
        t_bar(ui, p, x_of(last.arr + 8) + 2.0, x_of(last.arr + 8 + prefs.post_trip), cy, false);
    } else {
        t_bar(ui, p, x_of(last.arr) + 1.0, x_of(last.arr + prefs.post_trip), cy, false);
    }
    // the reliefs: arrows where a driver gets off and on, a grey line while it is attended; a
    // click there joins the duties again
    for r in pieces.iter().skip(1) {
        let (pv, nx) = (&t.trips[r.start - 1], &t.trips[r.start]);
        let (xa, xb) = (x_of(pv.arr), x_of(nx.dep));
        match prefs.relief {
            ReliefTiming::ArrivalDeparture => {
                relief_arrow(ui, p, xa + 1.5, cy, false, true);
                relief_arrow(ui, p, xb - 1.5, cy, true, false);
            }
            _ => {
                let at = x_of(rf::relief_at(prefs, pv.arr, nx.dep, true));
                ui.p().rect(Rect::new(xa, cy - 0.5, (xb - xa).max(0.0), 1.0), p.grey);
                relief_arrow(ui, p, at, cy, true, true);
            }
        }
        if !read_only && hand.is_none() {
            let gr = Rect::new(xa - 3.0, cy - CHIP * 0.6, (xb - xa).max(4.0) + 6.0, CHIP * 1.2);
            if ui.rect_visible(gr) {
                super::super::super::tour::anchor("rf-relief", gr);
            }
            let (h, _, click) = ui.interact(id_of(&format!("rf-relief-{i}-{}", r.start)), gr);
            if h {
                ui.p().rounded_border(gr, 3.0, 1.0, p.accent);
                ui.tooltip(gr, &omsi_ui::tr("A driver takes over here, at %{stop}. Click to join the two duties.").replace("%{stop}", nx.from.trim()));
            }
            if click {
                todo.push(Do::Uncut(i, r.start));
            }
        }
    }
    // the gaps where a duty can be cut (on hover: where the bus stands at a relief point)
    if !read_only && hand.is_none() {
        for at in 1..n {
            if pieces.iter().any(|r| r.start == at) {
                continue;
            }
            let (pv, nx) = (&t.trips[at - 1], &t.trips[at]);
            let (xa, xb) = (x_of(pv.arr), x_of(nx.dep));
            if xb - xa < 3.0 {
                continue;
            }
            let gr = Rect::new(xa, cy - CHIP * 0.5, xb - xa, CHIP);
            if !gr.contains(mouse) {
                continue;
            }
            let (h, _, click) = ui.interact(id_of(&format!("rf-gap-{i}-{at}")), gr);
            if !h {
                continue;
            }
            match st::can_cut(c, &board.tours, i, at) {
                Ok(()) => {
                    let mx = (xa + xb) * 0.5;
                    ui.p().rect(Rect::new(mx - 1.0, cy - CHIP * 0.6, 2.0, CHIP * 1.2), p.accent);
                    ui.tooltip(gr, &omsi_ui::tr("Click to cut the duty here: a driver takes over at %{stop}.").replace("%{stop}", nx.from.trim()));
                    if click {
                        todo.push(Do::Cut(i, at));
                    }
                }
                Err(why) => ui.tooltip(gr, why),
            }
        }
    }
}

/// A driver's duty in its row.
#[allow(clippy::too_many_arguments)]
fn duty_row_draw(ui: &mut Ui, p: &Pal, c: &Company, board: &Board, d: usize, row: &RowId, cy: f32, x_of: &dyn Fn(i32) -> f32, prefs: &rf::Prefs, hand: &Option<Hand>, sel: &[(usize, usize)], layer: u8, read_only: bool, mouse: Vec2, todo: &mut Vec<Do>, lit: &mut Option<(i32, i32)>) {
    let s: &Shaped = &board.shaped[d];
    // the splits: a thin line across, "Split" in the middle
    let line_col = if p.light { p.dim } else { Color::rgba(220, 224, 230, 0.9) };
    for (a, b) in &s.splits {
        let (xa, xb) = (x_of(*a), x_of(*b));
        ui.p().rect(Rect::new(xa, cy - 0.5, (xb - xa).max(0.0), 1.0), line_col);
        if xb - xa > 50.0 {
            let w = ui.width(&omsi_ui::tr("Split"), 10.0, Weight::Medium) + 10.0;
            let lr = Rect::new((xa + xb) * 0.5 - w * 0.5, cy - 7.0, w, 14.0);
            ui.p().rect(lr, p.grid);
            ui.text_in("Split", lr, 10.0, Weight::Medium, line_col, Align::Center);
        }
    }
    // the events: sign-on and sign-off, travel, custom work
    for (k, e) in s.events.iter().enumerate() {
        let (a, b) = (x_of(e.from), x_of(e.to));
        let er = Rect::new(a, cy - CHIP * 0.5, (b - a).max(8.0), CHIP);
        match e.kind {
            EvKind::SignOn => t_bar(ui, p, a, b, cy, true),
            EvKind::SignOff => t_bar(ui, p, a, b, cy, false),
            EvKind::Travel(m) => {
                let br = Rect::new(a + 1.0, cy - CHIP * 0.45, (b - a - 2.0).max(14.0), CHIP * 0.9);
                ui.p().rounded(br, 3.0, p.dead);
                ui.p().rounded_border(br, 3.0, 1.3, p.warn);
                ui.text_as_is(m.letter(), Vec2::new(br.center().x, cy + 4.0), 10.5, Weight::Bold, p.warn, Align::Center);
            }
            EvKind::Custom(ci) => {
                let br = Rect::new(a + 1.0, cy - CHIP * 0.5, (b - a - 2.0).max(10.0), CHIP);
                ui.p().rounded(br, 4.0, Color::rgba(238, 240, 244, 1.0));
                if p.light {
                    ui.p().rounded_border(br, 4.0, 1.0, p.dim);
                }
                let kind = s.custom.get(ci).map(|x| omsi_ui::tr(&x.kind).into_owned()).unwrap_or_default();
                if ui.width(&kind, 10.5, Weight::Bold) < br.w - 4.0 {
                    ui.text_as_is(&kind, Vec2::new(br.center().x, cy + 4.0), 10.5, Weight::Bold, Color::rgba(30, 36, 46, 1.0), Align::Center);
                }
            }
        }
        if !read_only && hand.is_none() && er.contains(mouse) {
            let (h, _, _) = ui.interact(id_of(&format!("rf-ev-{d}-{k}")), er);
            if h && ui.input.pressed {
                todo.push(Do::Card(Card::Event { row: d, ev: k, at: mouse }));
            }
            if let (true, true, EvKind::Custom(ci)) = (h, ui.input.right_pressed, e.kind) {
                todo.push(Do::Menu(Menu { at: mouse, tour: usize::MAX, trip: usize::MAX, duty_row: Some(d), piece: None, custom: Some((d, ci)) }));
            }
        }
    }
    // the pieces' trips
    for sp in &s.spans {
        let t = &board.tours[sp.tour];
        let colour = line_colour(c, &t.line);
        let r = co::network::duties_of(t).get(sp.duty).cloned().unwrap_or(0..0);
        let n = t.trips.len();
        let taken = *hand == Some(Hand::Piece { tour: sp.tour, duty: sp.duty });
        // attended: a grey line from where the driver takes the bus to its departure
        if let Some(f) = t.trips.get(r.start) {
            if r.start > 0 {
                if sp.from < f.dep - 1 {
                    ui.p().rect(Rect::new(x_of(sp.from), cy - 0.5, x_of(f.dep) - x_of(sp.from), 1.0), p.grey);
                }
                let both = !matches!(prefs.relief, ReliefTiming::ArrivalDeparture | ReliefTiming::OnDeparture);
                relief_arrow(ui, p, x_of(sp.from) + 1.5, cy, true, both);
            }
        }
        if let Some(l) = r.end.checked_sub(1).and_then(|e| t.trips.get(e)) {
            if r.end < n {
                if sp.to > l.arr + 1 {
                    ui.p().rect(Rect::new(x_of(l.arr), cy - 0.5, x_of(sp.to) - x_of(l.arr), 1.0), p.grey);
                }
                relief_arrow(ui, p, x_of(sp.to) - 1.5, cy, false, true);
            }
        }
        for j in r.clone() {
            let x = &t.trips[j];
            let a = x_of(x.dep);
            let b = x_of(x.arr).max(a + 4.0);
            if j > r.start {
                let prev = &t.trips[j - 1];
                if !prev.end().trim().eq_ignore_ascii_case(x.from.trim()) && !prev.empty && !x.empty {
                    deadhead(ui, p, x_of(prev.arr) + 1.0, x_of(prev.arr + st::DEADHEAD_MIN) - 1.0, cy, true);
                }
            }
            if x.empty || !x.counts() {
                if x.empty && (j == 0 || j + 1 == n) {
                    pull(ui, p, a + 1.0, b - 1.0, cy, false);
                } else {
                    deadhead(ui, p, a + 1.0, b - 1.0, cy, false);
                }
                continue;
            }
            let cr = Rect::new(a + 1.0, cy - CHIP * 0.5, (b - a - 2.0).max(5.0), CHIP);
            let chosen = sel.contains(&(sp.tour, j));
            let (h, _, _) = ui.interact(id_of(&format!("rf-dtrip-{d}-{}-{j}", sp.tour)), cr);
            if chosen || (h && hand.is_none()) {
                *lit = Some((x.dep, x.arr));
            }
            let outline = if chosen || (h && hand.is_none()) { Some(Color::WHITE) } else { None };
            chip(ui, cr, colour, outbound(&board.tours, &t.line, x), &chip_label(t, x, layer), outline, false, false, if taken { 0.35 } else { 1.0 });
            if ui.rect_visible(cr) && cr.w > 24.0 {
                super::super::super::tour::anchor("rf-chip", cr);
            }
            if read_only || hand.is_some() {
                continue;
            }
            if h && ui.input.pressed {
                todo.push(Do::Choose { tour: sp.tour, trip: j, row: row.clone(), shift: ui.input.shift, at: mouse });
            }
            if h && ui.input.right_pressed {
                todo.push(Do::Menu(Menu { at: mouse, tour: sp.tour, trip: j, duty_row: Some(d), piece: Some((sp.tour, sp.duty)), custom: None }));
            }
        }
    }
}

// --- what was done ---------------------------------------------------------------------------------

/// The trips of block `tour` from `a` to `b` (both in), with passengers.
fn range_of(board: &Board, tour: usize, a: usize, b: usize) -> Vec<usize> {
    let Some(t) = board.tours.get(tour) else { return Vec::new() };
    (a.min(b)..=a.max(b)).filter(|&k| t.trips.get(k).is_some_and(|x| x.counts())).collect()
}

fn apply(l: &mut Launcher, k: &Cached, board: &Board, drivers: bool, todo: Vec<Do>) {
    let wd = board.weekday;
    let now = l.ui.time;
    if todo.iter().any(|d| matches!(d, Do::Choose { .. } | Do::Menu(_) | Do::Card(_) | Do::Duty(_) | Do::StackChoose(..))) {
        l.company.planning.rf.opened_at = now;
    }
    for d in todo {
        match d {
            Do::Choose { tour, trip, row, shift, at } => {
                let s = &mut l.company.planning.rf;
                let key = format!("{tour}/{trip}");
                let double = s.last_click.as_ref().is_some_and(|(k2, t)| *k2 == key && now - t < 0.4);
                s.last_click = Some((key, now));
                s.menu = None;
                if shift && !drivers && s.sel_row.as_ref() == Some(&row) && s.sel.first().is_some_and(|f| f.0 == tour) {
                    let first = s.sel[0].1;
                    s.sel = range_of(board, tour, first, trip).into_iter().map(|x| (tour, x)).collect();
                    s.card = Some(Card::Selection { at });
                } else {
                    s.sel = vec![(tour, trip)];
                    s.sel_row = Some(row);
                    s.card = Some(Card::Trip { tour, trip, at, assign: false });
                }
                // (pressed: taken up if the mouse moves on; clicked twice: taken up)
                let hand = if drivers {
                    let duty = co::network::duties_of(&board.tours[tour]).iter().position(|r| r.contains(&trip)).unwrap_or(0);
                    Hand::Piece { tour, duty }
                } else {
                    Hand::Trips { tour, trips: s.sel.iter().filter(|x| x.0 == tour).map(|x| x.1).collect() }
                };
                if double {
                    pick(l, k, board, hand, false);
                } else {
                    l.company.planning.rf.press = Some((at, hand));
                }
            }
            Do::Pick(h, dragging) => pick(l, k, board, h, dragging),
            Do::DropZone(zi) => drop_at(l, board, zi),
            Do::Cancel => {
                let s = &mut l.company.planning.rf;
                s.hand = None;
                s.zones = None;
                s.dragging = false;
            }
            Do::Menu(m) => {
                let s = &mut l.company.planning.rf;
                s.card = None;
                if m.tour != usize::MAX && !s.sel.contains(&(m.tour, m.trip)) {
                    s.sel = vec![(m.tour, m.trip)];
                }
                s.menu = Some(m);
            }
            Do::Card(cd) => {
                let s = &mut l.company.planning.rf;
                s.menu = None;
                s.card = Some(cd);
            }
            Do::Spare => {
                edit(l, omsi_ui::tr("Spare duty added").into_owned(), |c| {
                    rf::add_spare(c, wd, rf::CUSTOM_KINDS[0], 810, 1290);
                    Ok(())
                });
            }
            Do::Tick(id) => {
                let s = &mut l.company.planning.rf;
                if let Some(i) = s.ticked.iter().position(|x| *x == id) {
                    s.ticked.remove(i);
                } else {
                    s.ticked.push(id);
                }
            }
            Do::TickAll(ids) => l.company.planning.rf.ticked = ids,
            Do::Lock(kind) => toggle_lock(l, board, kind),
            Do::Pin(id) => {
                let s = &mut l.company.planning.rf;
                if let Some(i) = s.pinned.iter().position(|x| *x == id) {
                    s.pinned.remove(i);
                } else {
                    s.pinned.insert(0, id);
                }
            }
            Do::Swap(to) => {
                if let Some(Hand::Swap { tour, trips, how }) = l.company.planning.rf.hand.clone() {
                    let tours = board.tours.clone();
                    let words = omsi_ui::tr("Trips swapped between %{a} and %{b}").replace("%{a}", &block_label(&tours[tour])).replace("%{b}", &block_label(&tours[to]));
                    if !edit(l, words, |c| st::swap(c, wd, &tours, tour, &trips, to, how)) {
                        l.company.planning.rf.hand = None;
                    }
                }
            }
            Do::Cut(t, at) => {
                let tours = board.tours.clone();
                let words = omsi_ui::tr("Duty cut on %{v} at %{time}").replace("%{v}", &block_label(&tours[t])).replace("%{time}", &rf::clock(tours[t].trips[at].dep));
                edit(l, words, |c| st::cut(c, wd, &tours, t, at));
            }
            Do::Uncut(t, at) => {
                let tours = board.tours.clone();
                let words = omsi_ui::tr("Duties joined on %{v} at %{time}").replace("%{v}", &block_label(&tours[t])).replace("%{time}", &rf::clock(tours[t].trips[at].dep));
                edit(l, words, |c| st::uncut(c, wd, &tours, t, at));
            }
            Do::Duty(id) => {
                let s = &mut l.company.planning.rf;
                s.panel = shell::Panel::Duty(id.clone());
                s.duty_scroll = 0.0;
                s.sel_row = Some(id);
                s.card = None;
            }
            Do::StackChoose(k2, at) => {
                pick(l, k, board, Hand::Stack(k2), false);
                let s = &mut l.company.planning.rf;
                s.card = Some(Card::Stack { k: k2, at });
                s.sel.clear();
            }
        }
    }
}

/// Lock or unlock a bus's block or a duty.
pub(super) fn toggle_lock(l: &mut Launcher, board: &Board, kind: Kind) {
    let wd = board.weekday;
    match kind {
        Kind::Block(i) => {
            let Some(t) = board.tours.get(i) else { return };
            let (line, tour, label) = (t.line.clone(), t.tour.clone(), block_label(t));
            let on = !l.company.company.as_ref().map(rf::view).is_some_and(|v| rf::block_locked(&v, wd, &line, &tour));
            let words = if on { omsi_ui::tr("Vehicle %{v} locked") } else { omsi_ui::tr("Vehicle %{v} unlocked") }.replace("%{v}", &label);
            edit(l, words, |c| {
                rf::set_block_lock(c, wd, &line, &tour, on);
                Ok(())
            });
        }
        Kind::Duty(d) => {
            let Some(s) = board.shaped.get(d) else { return };
            let label = s.label(d);
            let on = !s.locked;
            let words = if on { omsi_ui::tr("Duty %{d} locked") } else { omsi_ui::tr("Duty %{d} unlocked") }.replace("%{d}", &label);
            match (&s.key, s.spare) {
                (Some(key), _) => {
                    let key = key.clone();
                    edit(l, words, |c| {
                        rf::set_duty_lock(c, wd, &key, on);
                        Ok(())
                    });
                }
                (None, Some(id)) => {
                    edit(l, words, |c| {
                        if let Some(sp) = c.planning.flow.sheet.spares.iter_mut().find(|x| x.weekday == wd && x.id == id) {
                            sp.locked = on;
                        }
                        Ok(())
                    });
                }
                _ => {}
            }
        }
    }
}

/// Take something up: its drop zones are worked out (each rated by what it costs).
pub(super) fn pick(l: &mut Launcher, k: &Cached, board: &Board, h: Hand, dragging: bool) {
    // (a locked vehicle or duty is not taken up: said at once)
    let c = &k.view;
    let wd = board.weekday;
    let refused = match &h {
        Hand::Trips { tour, .. } | Hand::Swap { tour, .. } => board.tours.get(*tour).is_some_and(|t| rf::block_locked(c, wd, &t.line, &t.tour)).then_some(st::LOCKED),
        Hand::Piece { tour, duty } => rf::piece_locked(c, wd, &board.tours, *tour, *duty).then_some(st::DUTY_LOCKED),
        Hand::Stack(_) => None,
    };
    if let Some(why) = refused {
        l.state.set_status(omsi_ui::tr(why).into_owned(), true);
        return;
    }
    let zones = if matches!(h, Hand::Swap { .. }) { None } else { Some(zones_for(k, board, &h)) };
    let s = &mut l.company.planning.rf;
    s.press = None;
    s.hand = Some(h);
    s.zones = zones;
    s.dragging = dragging;
    s.menu = None;
    {
        s.card = None;
    }
}

/// Where what is in hand may go, each rated: the cost it makes against the day's (blue where
/// it breaks no rule more), the cheapest five stars.
pub(super) fn zones_for(k: &Cached, board: &Board, h: &Hand) -> Vec<Zone> {
    let c: &Company = &k.view;
    let wd = board.weekday;
    let tours = &board.tours;
    let base = board.figures.total() + board.figures.penalties;
    let hard_now = board.issues.iter().filter(|i| i.hard).count();
    let mut out: Vec<Zone> = Vec::new();
    let mut probe_with = |drop: Drop, f: &dyn Fn(&mut Company) -> Result<(), &'static str>| {
        let mut probe = c.clone();
        if f(&mut probe).is_ok() {
            let b = rf::board(&probe, &k.lines, &k.date);
            let hard = b.issues.iter().filter(|i| i.hard).count();
            out.push(Zone { drop, ok: Ok(()), stars: 0, delta: b.figures.total() + b.figures.penalties - base, clean: hard <= hard_now });
        }
    };
    match h {
        Hand::Trips { tour, trips } => {
            let line = tours[*tour].line.clone();
            for (i, t) in tours.iter().enumerate() {
                if i == *tour || !t.line.eq_ignore_ascii_case(&line) || st::plan_move_many(c, wd, tours, *tour, trips, Target::Block(i)).is_err() {
                    continue;
                }
                probe_with(Drop::Block(i), &|p| st::move_trips(p, wd, tours, *tour, trips, Target::Block(i)));
            }
            probe_with(Drop::New, &|p| st::move_trips(p, wd, tours, *tour, trips, Target::New));
            probe_with(Drop::Stack, &|p| st::unassign(p, wd, tours, *tour, trips));
        }
        Hand::Stack(sk) => {
            let Some((line, x)) = board.stack.get(*sk) else { return out };
            for (i, t) in tours.iter().enumerate() {
                if !t.line.eq_ignore_ascii_case(line) || st::plan_assign(c, wd, tours, line, x, Target::Block(i)).is_err() {
                    continue;
                }
                probe_with(Drop::Block(i), &|p| st::assign(p, wd, tours, line, x, Target::Block(i)));
            }
            probe_with(Drop::New, &|p| st::assign(p, wd, tours, line, x, Target::New));
        }
        Hand::Piece { tour, duty } => {
            let mine = st::duty_of(&board.duties, *tour, *duty);
            for (d, s) in board.shaped.iter().enumerate() {
                let Some(di) = s.duty else { continue };
                if Some(di) == mine {
                    continue;
                }
                let Some(first) = board.duties[di].pieces.first() else { continue };
                let a = (first.tour, first.duty);
                let mut test = c.clone();
                let _ = st::detach(&mut test, wd, tours, (*tour, *duty));
                if st::can_join(&test, wd, tours, a, (*tour, *duty)).is_err() {
                    continue;
                }
                probe_with(Drop::Duty(d), &|p| {
                    let _ = st::detach(p, wd, tours, (*tour, *duty));
                    st::join(p, wd, tours, a, (*tour, *duty))
                });
            }
            if mine.is_some_and(|m| board.duties[m].pieces.len() > 1) {
                probe_with(Drop::NewDuty, &|p| st::detach(p, wd, tours, (*tour, *duty)));
            }
        }
        Hand::Swap { .. } => {}
    }
    rate(&mut out);
    out
}

/// The stars: the cheapest five, the dearest one; a zone that breaks a rule two at most.
pub(super) fn rate(zones: &mut [Zone]) {
    let mut deltas: Vec<i64> = zones.iter().map(|z| z.delta).collect();
    deltas.sort_unstable();
    deltas.dedup();
    let n = deltas.len().max(1);
    for z in zones.iter_mut() {
        let rank = deltas.iter().position(|d| *d == z.delta).unwrap_or(0);
        z.stars = if n <= 1 { 5 } else { (5.0 - (rank as f32 * 4.0 / (n - 1) as f32).round()) as u8 };
        if !z.clean {
            z.stars = z.stars.min(2);
        }
    }
}

/// Drop what is in hand at zone `zi`.
pub(super) fn drop_at(l: &mut Launcher, board: &Board, zi: usize) {
    let s = &l.company.planning.rf;
    let (Some(h), Some(z)) = (s.hand.clone(), s.zones.as_ref().and_then(|zs| zs.get(zi)).cloned()) else { return };
    let wd = board.weekday;
    let tours = board.tours.clone();
    match (h, z.drop) {
        (Hand::Trips { tour, trips }, Drop::Block(i)) => {
            let words = omsi_ui::tr("%{n} trips moved from %{a} to %{b}").replace("%{n}", &trips.len().to_string()).replace("%{a}", &block_label(&tours[tour])).replace("%{b}", &block_label(&tours[i]));
            edit(l, words, |c| st::move_trips(c, wd, &tours, tour, &trips, Target::Block(i)));
        }
        (Hand::Trips { tour, trips }, Drop::New) => {
            let words = omsi_ui::tr("%{n} trips moved from %{a} to a new vehicle").replace("%{n}", &trips.len().to_string()).replace("%{a}", &block_label(&tours[tour]));
            edit(l, words, |c| st::move_trips(c, wd, &tours, tour, &trips, Target::New));
        }
        (Hand::Trips { tour, trips }, Drop::Stack) => {
            let words = omsi_ui::tr("%{n} trips of %{a} unassigned to the stack").replace("%{n}", &trips.len().to_string()).replace("%{a}", &block_label(&tours[tour]));
            edit(l, words, |c| st::unassign(c, wd, &tours, tour, &trips));
        }
        (Hand::Stack(sk), drop) => {
            if let Some((line, x)) = board.stack.get(sk).cloned() {
                let to = match drop {
                    Drop::Block(i) => Target::Block(i),
                    _ => Target::New,
                };
                let words = omsi_ui::tr("Trip of %{time} assigned from the stack").replace("%{time}", &rf::clock(x.dep));
                edit(l, words, |c| st::assign(c, wd, &tours, &line, &x, to));
            }
        }
        (Hand::Piece { tour, duty }, Drop::Duty(d)) => {
            if let Some(first) = board.shaped.get(d).and_then(|s| s.duty).and_then(|di| board.duties[di].pieces.first().copied()) {
                let words = omsi_ui::tr("Piece of %{v} joined into duty %{d}").replace("%{v}", &block_label(&tours[tour])).replace("%{d}", &board.shaped[d].label(d));
                edit(l, words, |c| {
                    let _ = st::detach(c, wd, &tours, (tour, duty));
                    st::join(c, wd, &tours, (first.tour, first.duty), (tour, duty))
                });
            }
        }
        (Hand::Piece { tour, duty }, Drop::NewDuty) => {
            let words = omsi_ui::tr("Piece of %{v} made a duty of its own").replace("%{v}", &block_label(&tours[tour]));
            edit(l, words, |c| st::detach(c, wd, &tours, (tour, duty)));
        }
        _ => {}
    }
    l.company.planning.rf.settle();
}

/// The arrow keys: along the row to the next or previous trip (Shift: the choice widens), up
/// and down to the nearest trip of the row above or below.
pub(super) fn arrow_key(l: &mut Launcher, k: &Cached, board: &Board, drivers: bool, key: Key, shift: bool) {
    let rows_now = rows(l, k, board, drivers);
    let s = &mut l.company.planning.rf;
    let Some(&(tour, trip)) = s.sel.last() else { return };
    let Some(ri) = rows_now.iter().position(|r| Some(&r.id) == s.sel_row.as_ref()) else { return };
    // the chips of a row, in order: (block, trip)
    let chips_of = |row: &Row| -> Vec<(usize, usize)> {
        match row.kind {
            Kind::Block(i) => (0..board.tours[i].trips.len()).filter(|&j| board.tours[i].trips[j].counts()).map(|j| (i, j)).collect(),
            Kind::Duty(d) => board.shaped[d].spans.iter().flat_map(|sp| co::network::duties_of(&board.tours[sp.tour]).get(sp.duty).cloned().unwrap_or(0..0).filter(|&j| board.tours[sp.tour].trips[j].counts()).map(move |j| (sp.tour, j))).collect(),
        }
    };
    let here = chips_of(&rows_now[ri]);
    let at = here.iter().position(|x| *x == (tour, trip)).unwrap_or(0);
    let Some(dep) = board.tours.get(tour).and_then(|t| t.trips.get(trip)).map(|x| x.dep) else { return };
    match key {
        Key::Left | Key::Right => {
            let next = if key == Key::Left { at.checked_sub(1) } else { Some(at + 1).filter(|n| *n < here.len()) };
            if let Some(n) = next {
                if shift && !drivers && here[n].0 == tour {
                    if !s.sel.contains(&here[n]) {
                        s.sel.push(here[n]);
                        s.sel.sort_unstable();
                    }
                } else {
                    s.sel = vec![here[n]];
                }
                s.card = None;
            }
        }
        Key::Up | Key::Down => {
            let other = if key == Key::Up { ri.checked_sub(1) } else { Some(ri + 1).filter(|n| *n < rows_now.len()) };
            if let Some(o) = other {
                let there = chips_of(&rows_now[o]);
                if let Some(best) = there.iter().min_by_key(|(t2, j2)| (board.tours[*t2].trips[*j2].dep - dep).abs()) {
                    s.sel = vec![*best];
                    s.sel_row = Some(rows_now[o].id.clone());
                    s.reveal = Some(rows_now[o].id.clone());
                    s.card = None;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_named_by_its_line_and_tour_and_trips_point_their_way() {
        let trip = |a: &str, b: &str, dep: i32| PlannedTrip { line: "4".into(), from: a.into(), to: b.into(), stops: 6, dep, arr: dep + 30, ..Default::default() };
        let t = TourOfDay { line: "Linie4".into(), number: "4".into(), tour: "1".into(), trips: vec![trip("Nord", "Süd", 360), trip("Süd", "Nord", 400)], ..Default::default() };
        assert_eq!(block_label(&t), "4/01");
        let tours = vec![t.clone()];
        assert!(outbound(&tours, "Linie4", &t.trips[0]));
        assert!(!outbound(&tours, "Linie4", &t.trips[1]));
        assert_eq!(chip_label(&t, &t.trips[0], 1), "00 > 30");
        assert_eq!(chip_label(&t, &t.trips[0], 2), "No > Sü");
        let b = Board { tours, ..Default::default() };
        let (a, z) = day_span(&b);
        assert!(a <= 340 && z >= 450 && a % 60 == 0 && z % 60 == 0);
    }

    #[test]
    fn every_text_of_rosterflow_and_the_studio_is_translated() {
        // (the core's texts are no literals of the pages: `locale_check` cannot see them)
        let places = |s: &str| {
            let mut v: Vec<String> = s.match_indices("%{").filter_map(|(i, _)| s[i..].find('}').map(|j| s[i..i + j + 1].to_string())).collect();
            v.sort();
            v
        };
        let mut all: Vec<&str> = rf::texts();
        all.extend(st::texts());
        all.extend(Stat::ALL.iter().map(|s| s.label()));
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for t in &all {
                let Some(x) = crate::_rust_i18n_try_translate(lang, t) else { panic!("{lang}: no translation of {t:?}") };
                assert_eq!(places(&x), places(t), "{lang}: {t:?} -> {x:?}");
            }
        }
    }

    #[test]
    fn the_cheapest_zones_get_five_stars_and_one_that_breaks_a_rule_two_at_most() {
        let z = |delta: i64, clean: bool| Zone { drop: Drop::New, ok: Ok(()), stars: 0, delta, clean };
        let mut zs = vec![z(-500, true), z(0, true), z(800, true), z(-500, false)];
        rate(&mut zs);
        assert_eq!(zs.iter().map(|z| z.stars).collect::<Vec<_>>(), vec![5, 3, 1, 2]);
    }
}
