//! The easy mode's start screen (Luc: "a somewhat different layout per mode ... easy you must
//! think up yourself, what is suitable"): no category tabs, but everything the easy mode has on
//! one screen - a short list of what to do next (a line, buses, drivers, letting the time run,
//! a duty of one's own), each with the button that does it; under it what the company saw to
//! by itself (`co::autopilot`'s log); beside them the eight pages as large tiles, from the
//! money to the inbox; and under the tiles a few ways - the dealer, a new line, a loan, the
//! last day's report, the settings, another company.
//!
//! On a computer the list stands on the left and the tiles four by two on the right, kept to a
//! readable size in the middle; a low window scrolls. A phone has it all under each other.

use super::super::theme::*;
use super::super::ui::{ButtonKind, Ui};
use super::super::Launcher;
use super::home::{self, count_text, n_of, Marks, Tile, To, GAP, MAX_TILE_H, MAX_W, MIN_TILE_H, MORE_GAP, MORE_H, MORE_TOP};
use super::{kit, INBOX_TAB, MAP_TAB};
use glam::Vec2;
use omsi_launcher_lib::company::day::Plan;
use omsi_launcher_lib::company::plan::Who;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

// --- what to do next -------------------------------------------------------------------------------

/// A step of "What to do next".
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Step {
    /// Its name (English, translated when shown) and what it is about now (translated, its
    /// numbers filled).
    pub title: &'static str,
    pub text: String,
    pub done: bool,
    /// Where its button goes, its words and icon (None: nothing to click - the clock is at the
    /// top).
    pub go: Option<(To, &'static str, &'static str)>,
}

/// The duties the player drives himself each week (the roster's).
fn my_duties(c: &Company) -> usize {
    c.planning.week.iter().map(|r| r.duties.iter().filter(|d| **d == Some(Who::Player)).count()).sum()
}

/// What a new easy company does first, and how far this one is: a line, buses for its tours,
/// drivers for its duties, the time running, and a duty of the player's own.
pub(super) fn steps(c: &Company, plan: Option<&Plan>) -> Vec<Step> {
    let today = c.date.as_str();
    let buses = c.fleet.iter().filter(|v| v.held_on(today)).count();
    let drivers = c.staff.iter().filter(|e| co::staff::drives(e) && e.employed_on(today)).count();
    let (no_bus, no_driver) = plan.map(|p| p.short_of()).unwrap_or((0, 0));
    let line = Step {
        title: "Take on a line",
        text: if c.lines.is_empty() { omsi_ui::tr("Choose one of the map's lines, or draw a line of your own.").into_owned() } else { count_text(c.lines.len(), "1 line", "%{n} lines") },
        done: !c.lines.is_empty(),
        go: Some((To::Tab(3), "To the lines", "route")),
    };
    let bus = Step {
        title: "Get your buses",
        text: if buses == 0 {
            omsi_ui::tr("Buy, lease or rent buses at the dealer: one for every tour on the road at once.").into_owned()
        } else if no_bus > 0 {
            n_of("%{n} tours of today have no bus: get more buses.", no_bus)
        } else {
            count_text(buses, "1 bus", "%{n} buses")
        },
        done: buses > 0 && no_bus == 0,
        go: Some((To::Dealer, "To the dealer", "airport_shuttle")),
    };
    let staff = Step {
        title: "Drivers for the duties",
        text: if drivers == 0 {
            omsi_ui::tr("Hire drivers, or let the company hire them tonight for the duties of its lines.").into_owned()
        } else if no_driver > 0 {
            n_of("%{n} duties of today have no driver: the company hires more tonight, or hire them now.", no_driver)
        } else {
            n_of("%{n} drivers: every duty has its driver.", drivers)
        },
        done: drivers > 0 && no_driver == 0,
        go: Some((To::Hire, "Hire drivers", "person")),
    };
    let time = Step {
        title: "Let the time run",
        text: if c.history.is_empty() { omsi_ui::tr("Start the company's clock at the top, or simulate to tomorrow: the tours run and the money comes in.").into_owned() } else { omsi_ui::tr("The days run: every morning the company fills its roster by itself.").into_owned() },
        done: !c.history.is_empty(),
        go: None,
    };
    let mine = my_duties(c);
    let drive = Step {
        title: "Drive a duty yourself",
        text: if mine > 0 { count_text(mine, "You drive 1 duty a week.", "You drive %{n} duties a week.") } else { omsi_ui::tr("Take one of the company's duties and drive it in the game.").into_owned() },
        done: mine > 0,
        go: Some((To::MyDuties, "Choose a duty", "play_arrow")),
    };
    vec![line, bus, staff, time, drive]
}

// --- the tiles -------------------------------------------------------------------------------------

/// The easy mode's tiles: its eight pages, the player's own duties among them as "Drive".
pub(super) fn tiles(c: &Company, plan: Option<&Plan>, marks: &Marks) -> Vec<Tile> {
    let all = home::sections(c, plan, marks);
    let pick = |to: To| all.iter().flat_map(|s| s.tiles.iter()).find(|t| t.to == to).cloned();
    let mine = my_duties(c);
    let drive = Tile::new(To::MyDuties, "Drive", "play_arrow")
        .line(if mine > 0 { count_text(mine, "1 duty of yours a week", "%{n} duties of yours a week") } else { omsi_ui::tr("Take a duty and drive it").into_owned() }, if mine > 0 { TEXT_SOFT } else { accent_2() })
        .line(omsi_ui::tr("Your own duties in the game"), TEXT_DIM);
    let mut out: Vec<Tile> = [pick(To::Tab(0)), pick(To::Tab(3)), pick(To::Tab(1)), pick(To::Tab(2)), Some(drive), pick(To::Tab(4)), pick(To::Tab(MAP_TAB)), pick(To::Tab(INBOX_TAB))].into_iter().flatten().collect();
    for t in out.iter_mut() {
        t.big = false;
    }
    out
}

/// The ways under the tiles: (where, words, icon).
pub(super) fn ways(c: &Company) -> Vec<(To, &'static str, &'static str)> {
    let mut out = vec![(To::Dealer, "Dealer", "airport_shuttle"), (To::NewLine, "Make a new line", "alt_route"), (To::Loan, "Take a loan", "receipt_long")];
    if c.last_report.is_some() {
        out.push((To::Report, "Show the report", "description"));
    }
    out.push((To::Settings, "Settings", "settings"));
    out.push((To::Chooser, "Other company", "sync_alt"));
    out
}

// --- the layout ------------------------------------------------------------------------------------

/// How wide the list on the left is at least and at most.
const SIDE_MIN: f32 = 300.0;
const SIDE_MAX: f32 = 420.0;
/// The tiles' columns where they have room, and else.
const COLS: usize = 4;
const COLS_NARROW: usize = 2;

/// The easy start screen laid out in its area on a computer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Layout {
    /// The list on the left (what to do next, what the company did).
    pub side: Rect,
    /// Each tile's place.
    pub tiles: Vec<Rect>,
    pub tile_h: f32,
    /// Where the ways under the tiles begin, and how wide they may run.
    pub more: Rect,
    /// All of it.
    pub height: f32,
}

/// Where everything lies in `area`: the list on the left, `n` tiles in rows of four (two where
/// there is less room) beside it sharing the height between a tile's least and most, and
/// `more_rows` rows of ways under them; all of it a little higher than the middle.
pub(super) fn layout(area: Rect, n: usize, more_rows: usize) -> Layout {
    let w = area.w.min(MAX_W);
    let x0 = (area.x + (area.w - w) * 0.5).floor();
    let side_w = (w * 0.32).clamp(SIDE_MIN, SIDE_MAX).floor();
    let tx = x0 + side_w + GAP;
    let tw = w - side_w - GAP;
    let cols = if tw >= 700.0 { COLS } else { COLS_NARROW }.min(n.max(1));
    let rows = n.div_ceil(cols).max(1);
    let more_h = if more_rows == 0 { 0.0 } else { MORE_TOP + more_rows as f32 * MORE_H + (more_rows as f32 - 1.0) * MORE_GAP };
    let fixed = (rows as f32 - 1.0) * GAP + more_h;
    let tile_h = ((area.h - fixed) / rows as f32).clamp(MIN_TILE_H, MAX_TILE_H).floor();
    let height = fixed + rows as f32 * tile_h;
    let top = area.y + ((area.h - height) * 0.42).max(0.0).floor();
    let col_w = ((tw - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let tiles = (0..n).map(|k| Rect::new(tx + (k % cols) as f32 * (col_w + GAP), top + (k / cols) as f32 * (tile_h + GAP), col_w, tile_h)).collect();
    let more = Rect::new(tx, top + rows as f32 * tile_h + (rows as f32 - 1.0) * GAP + if more_rows == 0 { 0.0 } else { MORE_TOP }, tw, (more_h - MORE_TOP).max(0.0));
    Layout { side: Rect::new(x0, top, side_w, height), tiles, tile_h, more, height }
}

// --- drawing --------------------------------------------------------------------------------------

/// The easy start screen in the company's page area.
pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let plan = l.company.plan.clone();
    let marks = Marks::of(&c, plan.as_ref());
    let list = tiles(&c, plan.as_ref(), &marks);
    let ways = ways(&c);
    let todo = steps(&c, plan.as_ref());
    let deeds = deeds(&c);
    let phone = super::super::mobile::mobile() || area.w < 760.0;
    let mut go: Option<To> = None;
    if phone {
        l.ui.scroll_area("company-easy-home", area, &mut |ui, v| phone_screen(ui, v, &todo, &deeds, &list, &ways, &mut go));
    } else {
        let tw = area.w.min(MAX_W) - (area.w.min(MAX_W) * 0.32).clamp(SIDE_MIN, SIDE_MAX).floor() - GAP;
        let (_, rows) = home::ways_laid(&l.ui, tw, &ways);
        let g = layout(area, list.len(), rows);
        if g.height <= area.h + 1.0 {
            desk_screen(&mut l.ui, &g, &todo, &deeds, &list, &ways, &mut go);
        } else {
            // (a window lower than the least tiles need: the whole of it scrolls)
            l.ui.scroll_area("company-easy-home", area, &mut |ui, v| {
                let g = layout(Rect::new(v.x, v.y, v.w, 0.0), list.len(), rows);
                desk_screen(ui, &g, &todo, &deeds, &list, &ways, &mut go);
                g.height + 8.0
            });
        }
    }
    if let Some(to) = go {
        home::open(l, to);
    }
}

/// What the company saw to by itself lately, the newest first: (its words, its day).
fn deeds(c: &Company) -> Vec<(String, String)> {
    c.autopilot.log.iter().rev().map(|d| (super::inbox::words(&d.text, &d.args), super::overview::short_date(&d.date))).collect()
}

/// A computer's easy start: the list on the left, the tiles and the ways on the right.
fn desk_screen(ui: &mut Ui, g: &Layout, todo: &[Step], deeds: &[(String, String)], list: &[Tile], ways: &[(To, &'static str, &'static str)], go: &mut Option<To>) {
    super::super::tour::anchor("company-tiles", Rect::new(g.side.x, g.side.y, g.more.right() - g.side.x, g.height));
    let steps_h = steps_height(ui, g.side.w, todo).min(g.side.h);
    steps_card(ui, Rect::new(g.side.x, g.side.y, g.side.w, steps_h), todo, go);
    let rest = g.side.h - steps_h - GAP;
    if rest >= 110.0 {
        deeds_card(ui, Rect::new(g.side.x, g.side.y + steps_h + GAP, g.side.w, rest), deeds);
    }
    // (the tiles and the ways under them: the tour's stop about them)
    super::super::tour::anchor("company-easy-tiles", Rect::new(g.more.x, g.side.y, g.more.w, g.height));
    for (t, r) in list.iter().zip(&g.tiles) {
        if home::tile(ui, *r, t) {
            *go = Some(t.to);
        }
    }
    let (at, _) = home::ways_laid(ui, g.more.w, ways);
    for ((to, label, icon), (row, x)) in ways.iter().zip(at) {
        let r = Rect::new(g.more.x + x, g.more.y + row as f32 * (MORE_H + MORE_GAP), home::way_w(ui, label), MORE_H);
        if home::way(ui, r, label, icon) {
            *go = Some(*to);
        }
    }
}

/// A phone's easy start: the list, the tiles two by two, the ways, what the company did.
/// Returns the height used.
fn phone_screen(ui: &mut Ui, v: Rect, todo: &[Step], deeds: &[(String, String)], list: &[Tile], ways: &[(To, &'static str, &'static str)], go: &mut Option<To>) -> f32 {
    let gap = 10.0;
    let mut y = v.y;
    super::super::tour::anchor("company-tiles", v);
    let h = steps_height(ui, v.w, todo);
    steps_card(ui, Rect::new(v.x, y, v.w, h), todo, go);
    y += h + gap;
    let tw = ((v.w - gap) / 2.0).floor();
    let th = 104.0;
    super::super::tour::anchor("company-easy-tiles", Rect::new(v.x, y, v.w, list.len().div_ceil(2) as f32 * (th + gap) - gap));
    for (k, t) in list.iter().enumerate() {
        let r = Rect::new(v.x + (k % 2) as f32 * (tw + gap), y + (k / 2) as f32 * (th + gap), tw, th);
        if home::tile(ui, r, t) {
            *go = Some(t.to);
        }
    }
    y += list.len().div_ceil(2) as f32 * (th + gap) + 6.0;
    let (at, rows) = home::ways_laid(ui, v.w, ways);
    for ((to, label, icon), (row, x)) in ways.iter().zip(at) {
        let r = Rect::new(v.x + x, y + row as f32 * (MORE_H + MORE_GAP), home::way_w(ui, label), MORE_H);
        if home::way(ui, r, label, icon) {
            *go = Some(*to);
        }
    }
    y += rows as f32 * (MORE_H + MORE_GAP) + 8.0;
    deeds_card(ui, Rect::new(v.x, y, v.w, 260.0), deeds);
    y += 260.0;
    y - v.y + 8.0
}

/// The list's card: its margin, its title's room, a done or later step's height, the gap
/// between steps.
const PAD: f32 = 18.0;
const HEAD_H: f32 = 44.0;
const ROW_H: f32 = 46.0;
const STEP_GAP: f32 = 6.0;

/// The step to do now: the first not done.
fn current(todo: &[Step]) -> Option<usize> {
    todo.iter().position(|s| !s.done)
}

/// How high the current step is drawn in a card `w` wide: its title, its words in full, its
/// button.
fn current_height(ui: &Ui, w: f32, s: &Step) -> f32 {
    let text_w = w - 2.0 * PAD - 40.0;
    24.0 + ui.paragraph_height(&s.text, text_w, kit::NOTE, Weight::Regular) + if s.go.is_some() { 12.0 + 36.0 } else { 0.0 } + 10.0
}

/// The list's height in a card `w` wide.
fn steps_height(ui: &Ui, w: f32, todo: &[Step]) -> f32 {
    let now = current(todo);
    let rows: f32 = todo.iter().enumerate().map(|(k, s)| if Some(k) == now { current_height(ui, w, s) } else { ROW_H }).sum::<f32>() + STEP_GAP * (todo.len() as f32 - 1.0).max(0.0);
    let all_done = if now.is_none() { 70.0 } else { 0.0 };
    HEAD_H + rows + all_done + PAD
}

/// "What to do next": every step with its mark - done, the one to do now with its words and
/// its button, those after it faint.
fn steps_card(ui: &mut Ui, r: Rect, todo: &[Step], go: &mut Option<To>) {
    super::super::tour::anchor("company-easy-steps", r);
    ui.card(r);
    kit::caps(ui, Rect::new(r.x + PAD, r.y + 14.0, r.w - 2.0 * PAD, 16.0), "What to do next");
    let now = current(todo);
    let mut y = r.y + HEAD_H;
    let x = r.x + PAD;
    let text_x = x + 40.0;
    let text_w = r.w - 2.0 * PAD - 40.0;
    for (k, s) in todo.iter().enumerate() {
        let is_now = Some(k) == now;
        let h = if is_now { current_height(ui, r.w, s) } else { ROW_H };
        if y + 20.0 > r.bottom() {
            break;
        }
        // its mark: a check, its number in the accent, or its number faint
        let mid = Vec2::new(x + 14.0, y + 13.0);
        if s.done {
            ui.icon("check_circle", mid, 24.0, OK);
        } else if is_now {
            ui.p().circle(mid, 13.0, accent());
            ui.text_in(&(k + 1).to_string(), Rect::new(mid.x - 13.0, mid.y - 13.0, 26.0, 26.0), 13.0, Weight::Bold, on_accent(), Align::Center);
        } else {
            ui.p().circle(mid, 13.0, HAIRLINE);
            ui.p().circle(mid, 11.5, PANEL);
            ui.text_in(&(k + 1).to_string(), Rect::new(mid.x - 13.0, mid.y - 13.0, 26.0, 26.0), 12.5, Weight::Bold, TEXT_DIM, Align::Center);
        }
        let title_c = if s.done { TEXT_SOFT } else if is_now { TEXT } else { TEXT_DIM };
        ui.text_in(s.title, Rect::new(text_x, y, text_w, 26.0), kit::BODY, Weight::Bold, title_c, Align::Left);
        if is_now {
            let ph = ui.paragraph(&s.text, Vec2::new(text_x, y + 26.0), text_w, kit::NOTE, Weight::Regular, TEXT_SOFT);
            if let Some((to, label, icon)) = s.go {
                let bw = kit::Foot::width(ui, label, Some(icon)).min(text_w);
                if ui.button(&format!("company-easy-step-{k}"), Rect::new(text_x, y + 26.0 + ph + 12.0, bw, 36.0), label, Some(icon), ButtonKind::Primary) {
                    *go = Some(to);
                }
            }
        } else {
            ui.text_in(&s.text, Rect::new(text_x, y + 24.0, text_w, 18.0), kit::NOTE, Weight::Regular, if s.done { TEXT_DIM } else { TEXT_FAINT }, Align::Left);
            ui.tooltip(Rect::new(x, y, r.w - 2.0 * PAD, h), &s.text);
            // (a step done or to come is a click away too)
            if let Some((to, _, _)) = s.go {
                if ui.row(&format!("company-easy-step-row-{k}"), Rect::new(x - 8.0, y - 2.0, r.w - 2.0 * PAD + 16.0, h), false) {
                    *go = Some(to);
                }
            }
        }
        y += h + STEP_GAP;
    }
    if now.is_none() && y + 50.0 <= r.bottom() {
        ui.p().rect(Rect::new(x, y + 4.0, r.w - 2.0 * PAD, 1.0), HAIRLINE);
        ui.paragraph("Your company runs. Grow it when you like: another line, more buses, a duty of your own now and then.", Vec2::new(x, y + 16.0), r.w - 2.0 * PAD, kit::NOTE, Weight::Regular, TEXT_SOFT);
    }
}

/// What the company saw to by itself, the newest first, as many as the card holds; without any
/// yet, what it will see to.
fn deeds_card(ui: &mut Ui, r: Rect, deeds: &[(String, String)]) {
    super::super::tour::anchor("company-easy-deeds", r);
    ui.card(r);
    kit::caps(ui, Rect::new(r.x + PAD, r.y + 14.0, r.w - 2.0 * PAD, 16.0), "Done by the company itself");
    let x = r.x + PAD;
    let w = r.w - 2.0 * PAD;
    let mut y = r.y + HEAD_H;
    if deeds.is_empty() {
        ui.paragraph("Every morning and every night the company sees to its routine: the roster, spare parts, repairs, staff and research. What it did is told here.", Vec2::new(x, y), w, kit::NOTE, Weight::Regular, TEXT_SOFT);
        return;
    }
    let row = 26.0;
    for (text, day) in deeds {
        if y + row > r.bottom() - 10.0 {
            break;
        }
        ui.icon("autorenew", Vec2::new(x + 8.0, y + row * 0.5), 14.0, accent_2());
        let dw = ui.width(day, 12.0, Weight::Regular) + 6.0;
        let tr = Rect::new(x + 24.0, y, (w - 24.0 - dw - 6.0).max(0.0), row);
        ui.text_in(text, tr, kit::NOTE, Weight::Regular, TEXT, Align::Left);
        ui.text_in(day, Rect::new(x + w - dw, y, dw, row), 12.0, Weight::Regular, TEXT_DIM, Align::Right);
        ui.tooltip(tr, text);
        y += row;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Difficulty, Founding, Mode};

    fn company() -> Company {
        found(&Founding { name: "Leicht".into(), difficulty: Difficulty::Realistic, date: "2024-03-04".into(), mode: Mode::Easy, ..Default::default() }, "Luc")
    }

    #[test]
    fn the_easy_start_has_its_eight_pages_and_the_drive() {
        let c = company();
        let list = tiles(&c, None, &Marks::of(&c, None));
        let tos: Vec<To> = list.iter().map(|t| t.to).collect();
        assert_eq!(tos, vec![To::Tab(0), To::Tab(3), To::Tab(1), To::Tab(2), To::MyDuties, To::Tab(4), To::Tab(MAP_TAB), To::Tab(INBOX_TAB)]);
        // every page a tile goes to is one of the easy mode's, and no tile is big
        for t in &list {
            if let To::Tab(tab) = t.to {
                assert!(super::super::pages_of(Mode::Easy).contains(&tab), "{tab}");
            }
            assert!(!t.big && !t.lines.is_empty());
        }
        // the ways go elsewhere than the tiles, none to a page the easy mode leaves out
        for (to, _, _) in ways(&c) {
            assert!(!matches!(to, To::Tab(_)) && to != To::Rankings && to != To::Licences, "{to:?}");
        }
    }

    #[test]
    fn every_text_of_the_easy_mode_is_translated() {
        // (what `locale_check` cannot see: the core's texts, those handed on in a step, a tile
        // or a way, and those chosen in an `if` or a `match` of the pages)
        let c = company();
        let mut texts: Vec<&str> = co::autopilot::texts();
        for m in Mode::ALL {
            texts.extend([m.label(), m.info()]);
        }
        for s in steps(&c, None) {
            texts.push(s.title);
            texts.extend(s.go.map(|g| g.1));
        }
        texts.extend(tiles(&c, None, &Marks::of(&c, None)).iter().map(|t| t.name));
        texts.extend(ways(&c).iter().map(|w| w.1));
        texts.extend([
            "%{n} tours of today have no bus: get more buses.",
            "%{n} duties of today have no driver: the company hires more tonight, or hire them now.",
            "%{n} drivers: every duty has its driver.",
            "The company runs in the easy mode now: it sees to its routine by itself.",
            "The company runs in the advanced mode now: every page and every function is open.",
            "The company runs no line yet: take one on, and its duties are here.",
            "The company's lines run no tours in the coming week.",
            "Every duty of the coming week is yours already.",
            "Every tour in service has a bus and its drivers.",
            "Short for this line: %{b} buses and %{d} drivers. Get the buses at the dealer; the company hires the drivers by itself, at night.",
            "What the line asks of the company on a day like today, against what it has. In the easy mode it runs at once: the company fills its roster every morning.",
            "What the line asks of the company on a day like today, against what it has. Its concession is taken at the asking price, without an auction: the line is the company's from tomorrow and runs by itself.",
        ]);
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
    fn a_new_company_is_told_its_first_step_and_a_running_one_is_done() {
        let mut c = company();
        let s = steps(&c, None);
        assert_eq!(s.len(), 5);
        assert_eq!(current(&s), Some(0));
        assert_eq!(s[0].go.map(|g| g.0), Some(To::Tab(3)));
        assert!(s.iter().all(|x| !x.done));
        // a line, a bus, a driver, a closed day, a duty of his own: every step done
        c.lines.push(co::CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
        c.cash = 10_000_000_00;
        let bus = co::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        co::market::buy_new(&mut c, &bus, co::market::Payment::Cash, "").unwrap();
        let a = co::staff::applicants(&c).into_iter().find(|a| a.licence == co::Licence::D).unwrap();
        co::staff::hire(&mut c, &a).unwrap();
        assert_eq!(current(&steps(&c, Some(&Plan::default()))), Some(3), "the time next");
        c.history.push(Default::default());
        co::plan::set_driver(&mut c, 0, "Linie5", "1", 0, Some(Who::Player));
        let s = steps(&c, Some(&Plan::default()));
        assert!(s.iter().all(|x| x.done), "{s:?}");
        assert_eq!(current(&s), None);
    }

    #[test]
    fn the_easy_start_fits_from_a_small_window_to_a_large_one() {
        let c = company();
        let list = tiles(&c, None, &Marks::of(&c, None));
        for (w, h, rows) in [(1280.0, 720.0, 2), (1440.0, 900.0, 2), (1920.0, 1080.0, 1), (2560.0, 1440.0, 1)] {
            let area = Rect::new(24.0, 146.0, w - 48.0, h - 186.0);
            let g = layout(area, list.len(), rows);
            assert!(g.height <= area.h + 0.5, "{w}x{h}: {} > {}", g.height, area.h);
            assert_eq!(g.tiles.len(), list.len());
            for (i, a) in g.tiles.iter().enumerate() {
                assert!(a.x >= g.side.right() && a.right() <= area.right() + 0.5, "{w}x{h}: {a:?}");
                assert!(a.bottom() <= g.more.y + 0.5, "{w}x{h}: {a:?} over the ways");
                for b in &g.tiles[i + 1..] {
                    let apart = a.right() <= b.x + 0.5 || b.right() <= a.x + 0.5 || a.bottom() <= b.y + 0.5 || b.bottom() <= a.y + 0.5;
                    assert!(apart, "{w}x{h}: {a:?} and {b:?} meet");
                }
            }
            // four columns of two rows on a computer, the list beside them
            assert_eq!(g.tiles[4].y, g.tiles[0].y + g.tile_h + GAP, "{w}x{h}");
            assert!(g.side.x >= area.x && g.side.w >= SIDE_MIN);
        }
    }

    #[test]
    fn a_step_clicked_goes_where_it_says() {
        let c = company();
        let todo = steps(&c, None);
        let mut ui = Ui::new();
        let mut go = None;
        let r = Rect::new(24.0, 146.0, 380.0, 500.0);
        let frame = |ui: &mut Ui, go: &mut Option<To>| {
            ui.begin(Vec2::new(1280.0, 720.0), 1.0, 1.0 / 60.0);
            steps_card(ui, r, &todo, go);
        };
        frame(&mut ui, &mut go);
        let b = ui.drawn[&super::super::super::ui::id_of("company-easy-step-0")];
        ui.input.mouse = b.center();
        ui.input.pressed = true;
        ui.input.down = true;
        frame(&mut ui, &mut go);
        ui.input.pressed = false;
        ui.input.down = false;
        ui.input.released = true;
        frame(&mut ui, &mut go);
        assert_eq!(go, Some(To::Tab(3)));
    }
}
