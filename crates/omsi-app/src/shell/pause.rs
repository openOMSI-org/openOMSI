//! The pause menu as a page of the launcher: the launcher's rail on the left with the
//! menu's lines as its pages, the picture of the game beside it, and a list or settings
//! window opened from the rail shown as a launcher page right of it - a title, the tabs as
//! the launcher's segmented bar, the rows in panels of two columns with the launcher's
//! fields, switches and sliders.
//!
//! What a row is comes from `game_lists` as before (`name\u{1f}kind\u{1f}value\u{1f}
//! description\u{1f}fraction`, see `ui::MenuKind::Options`); what a click does goes back to
//! the game as an [`Action`].

use glam::Vec2;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

use super::{Action, Shell};
use omsi_ui::theme::*;
use omsi_ui::kit::{id_of, ButtonKind, Ui};
use crate::ui::{DropdownView, MenuKind, Preview};

/// The page's background behind a list or settings window: the launcher's, solid.
const PAGE: Color = BG;
/// ... and on the home page, where nothing but the sidebar is drawn: the paused game stays
/// in view, dimmed.
const HOME_VEIL: Color = Color([BG.0[0], BG.0[1], BG.0[2], 0.72]);
/// A list's row.
const LIST_ROW: f32 = 40.0;

/// An open list or settings window, as the game has it.
pub(crate) struct ListView<'a> {
    pub kind: MenuKind,
    pub head: Option<(String, String)>,
    pub tabs: Option<(Vec<String>, usize)>,
    /// (what it does, what it shows)
    pub items: Vec<(&'a str, &'a str)>,
    pub sel: usize,
    pub preview: Option<&'a Preview>,
    pub pane_first: Option<usize>,
    pub dropdown: Option<DropdownView<'a>>,
    /// Names the list for its scroll position (its kind and page).
    pub key: String,
}

/// What the pause menu shows.
pub(crate) struct PauseView<'a> {
    pub paused: bool,
    /// The game menu's lines.
    pub rail: &'a [(&'static str, &'static str)],
    /// The line the keyboard is on (the game menu's, while no list is open).
    pub rail_sel: Option<usize>,
    /// The line of the rail the open list belongs to.
    pub rail_open: Option<&'static str>,
    pub list: Option<ListView<'a>>,
    /// The keyboard chose last: its line is shown lit.
    pub kbd: bool,
    /// Key hints (a keyboard is there).
    pub keys: bool,
    /// What the home page sums up of the session.
    pub summary: Summary,
}

/// The session as the pause menu's sidebar shows it: the bus and the driver.
#[derive(Default)]
pub(crate) struct Summary {
    pub bus: String,
    pub driver: String,
}

/// The Material icon of a line of the game menu (the launcher's icon set).
pub(crate) fn rail_icon(id: &str) -> &'static str {
    match id {
        "resume" => "play_arrow",
        "tobus" => "directions_bus",
        "options" => "settings",
        "controls" => "sports_esports",
        "camera" => "videocam",
        "photo" | "shot" => "photo_camera",
        "vehicle" => "airport_shuttle",
        "world" => "partly_cloudy_day",
        "map" => "map",
        "duty" => "departure_board",
        "skipstop" => "near_me",
        "endduty" => "sports_score",
        "save" => "save",
        "saveslot" => "add",
        "load" => "history",
        "copycode" => "content_copy",
        "admin" => "key",
        "quit" => "logout",
        _ => "chevron_right",
    }
}

/// A label without the "..." that says it opens more.
fn plain(label: &str) -> (&str, bool) {
    let t = label.trim_end();
    match t.strip_suffix("...").or_else(|| t.strip_suffix('…')) {
        Some(x) => (x.trim_end(), true),
        None => (t, false),
    }
}

pub(crate) fn draw(sh: &mut Shell, v: &PauseView) {
    let size = sh.ui.size;
    // the launcher's window: its sidebar and a page beside it - the home page with the
    // session's summary, or the list or settings window opened from the sidebar
    let page = Rect::new(RAIL_W, 0.0, (size.x - RAIL_W).max(0.0), size.y);
    sh.ui.solid(page);
    match v.list.as_ref() {
        Some(list) => {
            sh.ui.p().rect(page, PAGE);
            list_page(sh, v, list, page);
        }
        // (the home page: the game, dimmed - the sidebar holds every action)
        None => sh.ui.p().rect(page, HOME_VEIL),
    }
    rail(sh, v);
}



/// The launcher's sidebar with the game menu's lines as its items: the logo, the name and
/// the version on top, the items (the open one on the highest grey, its icon in the
/// accent), room between their groups, the driver at the bottom.
fn rail(sh: &mut Shell, v: &PauseView) {
    let size = sh.ui.size;
    let r = Rect::new(0.0, 0.0, RAIL_W, size.y);
    let logo = sh.logo_tex;
    let ui = &mut sh.ui;
    ui.solid(r);
    ui.p().rect(r, SIDEBAR);
    // the brand
    if let Some(tex) = logo {
        ui.image(Rect::new(18.0, 16.0, 32.0, 32.0), tex, 7.0);
    }
    ui.text("openOMSI", Vec2::new(58.0, 31.0), 17.0, Weight::Bold, INK, Align::Left);
    let state = omsi_ui::tr(if v.paused { "Paused" } else { "Menu" }).into_owned();
    ui.text(&format!("{}  -  {state}", crate::startup::VERSION), Vec2::new(58.0, 46.0), 11.5, Weight::Medium, INK_MUTED, Align::Left);
    // the items
    let lines: Vec<(usize, &str, &str)> = v.rail.iter().enumerate().map(|(k, l)| (k, l.0, l.1)).collect();
    let top = 70.0;
    let foot = 12.0 + 56.0 + 10.0;
    let area = Rect::new(0.0, top, RAIL_W, (size.y - top - foot).max(42.0));
    let any_hover = ui.hover(area);
    let mut picked: Option<usize> = None;
    let mut sel_y: Option<f32> = None;
    ui.scroll_area("pause-rail", area, &mut |ui, view| {
        let mut y = view.y;
        for &(k, id, label) in &lines {
            // room between the groups (the everyday lines, saving, the way out), as the
            // launcher's `nav-sep`
            if matches!(id, "save" | "admin" | "quit") {
                y += 10.0;
            }
            let row = Rect::new(12.0, y, RAIL_W - 24.0, 38.0);
            let (h, _, clicked) = ui.interact(id_of(&format!("pause-{id}")), row);
            if clicked {
                picked = Some(k);
            }
            let open = v.rail_open == Some(id);
            let kbd = v.kbd && !any_hover && v.list.is_none() && v.rail_sel == Some(k);
            if kbd {
                sel_y = Some(y - view.y);
            }
            let danger = id == "quit";
            let t = ui.anim(id_of(&format!("pause-hl-{id}")), if h || kbd { 1.0 } else { 0.0 }, 0.06);
            if open {
                ui.p().rounded(row, 6.0, SURFACE_3);
            } else if t > 0.01 {
                ui.p().rounded(row, 6.0, if danger { DANGER_2.alpha(0.14 * t) } else { SURFACE_2.alpha(t) });
            }
            let c = if danger {
                DANGER_2.mix(Color::rgba(255, 176, 166, 1.0), t)
            } else if open {
                INK
            } else {
                INK_2.mix(INK, t)
            };
            let icon_c = if open || (kbd && !danger) { ACCENT } else { c };
            let translated = omsi_ui::tr(label);
            let (text, _) = plain(&translated);
            let (_, more) = plain(label);
            ui.icon(rail_icon(id), Vec2::new(row.x + 19.5, row.center().y), 19.0, icon_c);
            ui.text_in(text, Rect::new(row.x + 41.0, row.y, row.w - 64.0, row.h), 14.0, Weight::Medium, c, Align::Left);
            if more {
                ui.icon("chevron_right", Vec2::new(row.right() - 14.0, row.center().y), 16.0, c.alpha(0.5 + 0.5 * t));
            } else if id == "resume" && v.keys {
                ui.text_in("Esc", Rect::new(row.right() - 44.0, row.y, 34.0, row.h), 11.5, Weight::Medium, INK_MUTED, Align::Right);
            }
            y += 40.0;
        }
        y - view.y + 8.0
    });
    if let (Some(y), true) = (sel_y, v.kbd) {
        sh.ui.scroll_to("pause-rail", y, 38.0, area.h);
    }
    // the driver, at the bottom: the launcher's driver chip
    let ui = &mut sh.ui;
    let chip = Rect::new(12.0, size.y - 12.0 - 56.0, RAIL_W - 24.0, 56.0);
    ui.p().rounded(chip, 8.0, SURFACE);
    let av = Vec2::new(chip.x + 10.0 + 17.0, chip.center().y);
    ui.p().circle(av, 17.0, SURFACE_3);
    let initial = v.summary.driver.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    ui.text_in(&initial, Rect::new(av.x - 17.0, av.y - 17.0, 34.0, 34.0), 14.0, Weight::Bold, INK_2, Align::Center);
    ui.text_in(&v.summary.driver, Rect::new(chip.x + 54.0, chip.y + 9.0, chip.w - 64.0, 20.0), 13.5, Weight::Bold, INK, Align::Left);
    ui.text_in(&v.summary.bus, Rect::new(chip.x + 54.0, chip.y + 28.0, chip.w - 64.0, 18.0), 12.0, Weight::Regular, INK_MUTED, Align::Left);
    if let Some(k) = picked {
        sh.actions.push(if v.list.is_some() { Action::Rail(k) } else { Action::Choose(k) });
    }
}

/// The launcher's tabs: the titles in a row, the chosen one in the text colour over an amber
/// line, the others muted. Returns the one clicked.
fn tabs(ui: &mut Ui, at: Vec2, titles: &[String], chosen: usize, max_w: f32) -> Option<usize> {
    let mut x = at.x;
    let mut out = None;
    for (i, t) in titles.iter().enumerate() {
        let w = ui.width(&omsi_ui::tr(t), 14.0, Weight::Medium) + 4.0;
        if x + w > at.x + max_w {
            break;
        }
        let r = Rect::new(x, at.y, w, 38.0);
        let (h, _, clicked) = ui.interact(id_of(&format!("pause-tab-{i}")), r);
        let on = i == chosen;
        ui.text_in(t, r, 14.0, Weight::Medium, if on || h { INK } else { INK_MUTED }, Align::Center);
        if on {
            ui.p().rect(Rect::new(r.x, r.bottom() - 2.0, r.w, 2.0), ACCENT);
        }
        if clicked && !on {
            out = Some(i);
        }
        x += w + 22.0;
    }
    out
}

/// An open list or settings window as a launcher page right of the rail.
fn list_page(sh: &mut Shell, v: &PauseView, list: &ListView, page: Rect) {
    let margin = 32.0;
    let w = (page.w - margin * 2.0).min(1400.0);
    let area = Rect::new(page.x + margin + ((page.w - margin * 2.0) - w).max(0.0) * 0.5, 28.0, w, page.h - 28.0 - 24.0);
    // the title, with the way back before it
    let (title, sub) = list.head.clone().unwrap_or_else(|| (omsi_ui::tr("Options").into_owned(), String::new()));
    let back_k = list.items.iter().position(|(id, label)| *id == "back" && plain(label).0 == omsi_ui::tr("Back").as_ref());
    let back = Rect::new(area.x, area.y, 36.0, 36.0);
    if sh.ui.icon_button("pause-back", back.center(), 18.0, "arrow_back", "Back") {
        sh.actions.push(match (&list.tabs, back_k) {
            (Some((t, _)), _) => Action::Side(t.len()),
            (None, Some(k)) => Action::Choose(k),
            (None, None) => Action::Rail(usize::MAX),
        });
    }
    let tx = area.x + 46.0;
    sh.ui.text(&title, Vec2::new(tx, area.y + 26.0), 26.0, Weight::Bold, INK, Align::Left);
    let sub = if sub.is_empty() { list_hint(list.kind) } else { sub };
    if !sub.is_empty() {
        sh.ui.text_in(&sub, Rect::new(tx, area.y + 36.0, area.w - 46.0, 20.0), 13.0, Weight::Regular, INK_MUTED, Align::Left);
    }
    let mut body = Rect::new(area.x, area.y + 70.0, area.w, (area.h - 70.0).max(40.0));
    // the pages of a settings window: the launcher's tabs
    if let Some((titles, at)) = list.tabs.as_ref().filter(|(t, _)| t.len() > 1) {
        if let Some(tab) = tabs(&mut sh.ui, Vec2::new(body.x, body.y - 6.0), titles, *at, body.w) {
            sh.actions.push(Action::Side(tab));
        }
        let below = body.y - 6.0 + 38.0 + 20.0;
        body = Rect::new(body.x, below, body.w, (body.bottom() - below).max(40.0));
    }
    // an open drop-down takes the mouse where it lies (the rows under it do not answer)
    let dd_rect = list.dropdown.as_ref().and_then(|d| sh.dd_anchor.map(|a| dropdown_rect(a, d.items.len(), sh.ui.size)));
    let mouse = sh.ui.input.mouse;
    let saved = (sh.ui.input.pressed, sh.ui.input.released);
    if let Some(r) = dd_rect {
        if r.contains(mouse) || sh.ui.input.pressed {
            sh.ui.input.pressed = false;
            sh.ui.input.released = false;
        }
    }
    let mut anchor: Option<Rect> = None;
    match list.kind {
        MenuKind::Options => settings_rows(sh, v, list, body, &mut anchor),
        MenuKind::Lines | MenuKind::Tours => timetable_lists(sh, v, list, body),
        _ => plain_list(sh, v, list, body),
    }
    (sh.ui.input.pressed, sh.ui.input.released) = saved;
    sh.dd_anchor = anchor;
    if let (Some(d), Some(a)) = (list.dropdown.as_ref(), anchor) {
        dropdown(sh, d, a, v.kbd);
    }
}

/// What a list is for, under its title, when the game gives no line of its own.
fn list_hint(kind: MenuKind) -> String {
    let t = match kind {
        MenuKind::Options => "Every change takes effect at once.",
        MenuKind::Lines => "Choose a line of the map's timetable.",
        MenuKind::Tours => "Choose a tour and the stop to start from.",
        _ => "",
    };
    omsi_ui::tr(t).into_owned()
}

/// A row of a settings window, taken apart.
struct Row<'a> {
    name: &'a str,
    kind: &'a str,
    value: &'a str,
    desc: &'a str,
    frac: Option<f32>,
}

fn parse_row(label: &str) -> Row<'_> {
    let mut p = label.split('\u{1f}');
    let name = p.next().unwrap_or("");
    let kind = p.next().unwrap_or("a");
    let value = p.next().unwrap_or("");
    let desc = p.next().unwrap_or("");
    let frac = p.next().and_then(|x| x.parse().ok());
    Row { name, kind, value, desc, frac }
}

/// How high a row of a settings window is: two lines when it has a description.
fn row_h(id: &str, label: &str) -> f32 {
    if id == crate::game_lists::HEADING {
        return 34.0;
    }
    if parse_row(label).desc.is_empty() {
        ROW + 4.0
    } else {
        52.0
    }
}

/// The rows of a settings window in the launcher's panels: two columns side by side where
/// the page is wide enough, the headings of the list as the panels' section titles.
fn settings_rows(sh: &mut Shell, v: &PauseView, list: &ListView, body: Rect, anchor: &mut Option<Rect>) {
    let items = &list.items;
    // (the way back is the arrow before the title)
    let shown: Vec<usize> = (0..items.len()).filter(|&k| !(items[k].0 == "back" && list.tabs.is_none() && plain(parse_row(items[k].1).name).0 == omsi_ui::tr("Back").as_ref())).collect();
    let two = body.w >= 900.0 && shown.len() > 6;
    // (split where the heights balance, at a heading if one is near)
    let total: f32 = shown.iter().map(|&k| row_h(items[k].0, items[k].1)).sum();
    let mut split = shown.len();
    if two {
        let mut acc = 0.0;
        for (n, &k) in shown.iter().enumerate() {
            if acc >= total * 0.5 {
                split = n;
                // (a heading ends up first in the right column, not last in the left)
                if n > 0 && items[shown[n - 1]].0 == crate::game_lists::HEADING {
                    split = n - 1;
                }
                break;
            }
            acc += row_h(items[k].0, items[k].1);
        }
    }
    let cols: Vec<&[usize]> = if two { vec![&shown[..split], &shown[split..]] } else { vec![&shown[..]] };
    let any_hover = sh.ui.hover(body);
    let key = format!("pause-list-{}", list.key);
    let mut acts: Vec<Action> = Vec::new();
    let mut sel_y: Option<(f32, f32)> = None;
    let mut dd_anchor: Option<Rect> = None;
    let dd_row = list.dropdown.as_ref().map(|d| d.row);
    sh.ui.scroll_area(&key, body, &mut |ui, view| {
        let gap = GAP * 2.0;
        let cw = if cols.len() == 2 { (view.w - 8.0 - gap) / 2.0 } else { (view.w - 8.0).min(760.0) };
        let mut tallest = 0.0f32;
        for (c, rows) in cols.iter().enumerate() {
            let x = view.x + c as f32 * (cw + gap);
            let h: f32 = rows.iter().map(|&k| row_h(items[k].0, items[k].1)).sum::<f32>() + 28.0;
            let panel = Rect::new(x, view.y, cw, h.max(60.0));
            ui.panel(panel);
            let inner = Rect::new(x + 18.0, view.y + 14.0, cw - 36.0, h - 28.0);
            let mut y = inner.y;
            for &k in rows.iter() {
                let (id, label) = items[k];
                let rh = row_h(id, label);
                if id == crate::game_lists::HEADING {
                    // (a group's title as the launcher's card title)
                    let t = parse_row(label).name;
                    ui.text_in(t, Rect::new(inner.x, y + 4.0, inner.w, 28.0), 15.0, Weight::Bold, INK, Align::Left);
                    y += rh;
                    continue;
                }
                // a light line between the rows, as between the launcher's
                if rows.first() != Some(&k) && items.get(k.wrapping_sub(1)).is_some_and(|(pid, _)| *pid != crate::game_lists::HEADING) {
                    ui.p().rect(Rect::new(inner.x, y - 2.0, inner.w, 1.0), Color::rgba(255, 255, 255, 0.045));
                }
                let r = Rect::new(inner.x - 8.0, y, inner.w + 16.0, rh - 4.0);
                let kbd = v.kbd && !any_hover && k == list.sel;
                if kbd {
                    sel_y = Some((y - view.y, rh));
                }
                if let Some(a) = settings_row(ui, k, label, r, kbd, dd_row == Some(k), &mut dd_anchor) {
                    acts.push(a);
                }
                y += rh;
            }
            tallest = tallest.max(h);
        }
        tallest + 8.0
    });
    if let Some((y, h)) = sel_y {
        if sh.last_sel != Some((id_of(&key), list.sel)) {
            sh.ui.scroll_to(&key, y, h, body.h);
        }
    }
    sh.last_sel = Some((id_of(&key), list.sel));
    *anchor = dd_anchor;
    sh.actions.extend(acts);
}

/// One row of a settings window: its name (and description) on the left, its control on
/// the right as the launcher draws that control.
fn settings_row(ui: &mut Ui, k: usize, label: &str, r: Rect, kbd: bool, dd_open: bool, dd_anchor: &mut Option<Rect>) -> Option<Action> {
    let row = parse_row(label);
    let id = id_of(&format!("pause-row-{k}-{}", row.name));
    let hov = ui.hover(r);
    let t = ui.anim(id ^ 0x51, if hov || kbd { 1.0 } else { 0.0 }, 0.06);
    if t > 0.01 {
        // (half a step lighter than the panel: the fields in the row stay apart from it)
        ui.p().rounded(r, 6.0, SURFACE.mix(SURFACE_2, 0.5).alpha(t));
    }
    if kbd {
        ui.p().rounded(Rect::new(r.x, r.y + 8.0, 2.0, r.h - 16.0), 1.0, ACCENT);
    }
    let lx = r.x + 8.0;
    let ctl_w = (r.w * 0.5).min(300.0);
    let ctl = Rect::new(r.right() - 8.0 - ctl_w, r.center().y - 17.0, ctl_w, 34.0);
    let name_w = match row.kind {
        "s" | "i" => r.w - 80.0,
        _ => ctl.x - lx - 12.0,
    };
    let ink = TEXT_SOFT.mix(TEXT, t);
    if row.desc.is_empty() {
        ui.text_in(row.name, Rect::new(lx, r.y, name_w, r.h), 13.0, Weight::Regular, ink, Align::Left);
    } else {
        ui.text_in(row.name, Rect::new(lx, r.y + 6.0, name_w, 20.0), 13.0, Weight::Medium, ink, Align::Left);
        ui.text_in(row.desc, Rect::new(lx, r.y + 25.0, name_w, 17.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
        // (cut short by a narrow column: the whole of it under the mouse)
        if ui.width(row.desc, 11.5, Weight::Regular) > name_w {
            ui.tooltip(Rect::new(lx, r.y, name_w, r.h), row.desc);
        }
    }
    match row.kind {
        // a switch: the whole row flips it, as the launcher's
        "s" => {
            let mut on = row.value == "on";
            let sw = Rect::new(r.right() - 8.0 - 60.0, r.y, 60.0, r.h);
            let flipped = ui.toggle(&format!("pause-sw-{k}-{}", row.name), sw, &mut on, "");
            let (_, _, clicked) = ui.interact(id, Rect::new(r.x, r.y, r.w - 68.0, r.h));
            (flipped || clicked).then_some(Action::Choose(k))
        }
        // a slider, the row's value text beside it
        "v" => {
            let mut f = row.frac.unwrap_or(0.0).clamp(0.0, 1.0);
            let value = row.value.to_string();
            let changed = ui.slider(&format!("pause-sl-{k}-{}", row.name), ctl, &mut f, 0.0, 1.0, 0.0, "", &|_| value.clone());
            changed.then_some(Action::Control(k, f))
        }
        // a stepper: the value between the two arrows of a field
        "c" => {
            ui.p().rounded(ctl, 6.0, SURFACE_2);
            ui.text_in(row.value, ctl.pad(36.0, 0.0), 13.0, Weight::Regular, INK, Align::Center);
            let left = Rect::new(ctl.x, ctl.y, 34.0, ctl.h);
            let right = Rect::new(ctl.right() - 34.0, ctl.y, 34.0, ctl.h);
            let mut out = None;
            for (half, icon, fx) in [(left, "chevron_left", 0.0), (right, "chevron_right", 1.0)] {
                let (h, _, clicked) = ui.interact(id_of(&format!("pause-st-{k}-{icon}")), half);
                if h {
                    ui.p().rounded(half.inset(3.0), 5.0, SURFACE_3);
                }
                ui.icon(icon, half.center(), 18.0, if h { ACCENT } else { TEXT_DIM });
                if clicked {
                    out = Some(Action::Control(k, fx));
                }
            }
            out
        }
        // opens a list or a drop-down: the launcher's select, closed
        "o" => {
            let (h, _, clicked) = ui.interact(id, r);
            let a = ui.anim(id ^ 0x77, if h || dd_open { 1.0 } else { 0.0 }, 0.08);
            // (open: a step lighter, as under the mouse; no ring round it)
            ui.p().rounded(ctl, 6.0, SURFACE_2.mix(SURFACE_3, a));
            // (without a value it opens a list of its own: a way on, not a drop-down)
            let text = if row.value.is_empty() { omsi_ui::tr("Change").into_owned() } else { row.value.to_string() };
            ui.text_in(&text, Rect::new(ctl.x + 12.0, ctl.y, ctl.w - 40.0, ctl.h), 13.0, Weight::Regular, if row.value.is_empty() { TEXT_SOFT.mix(TEXT, a) } else { TEXT }, Align::Left);
            let icon = match (row.value.is_empty(), dd_open) {
                (true, _) => "chevron_right",
                (false, true) => "expand_less",
                (false, false) => "expand_more",
            };
            ui.icon(icon, Vec2::new(ctl.right() - 18.0, ctl.center().y), 18.0, if row.value.is_empty() { TEXT_DIM.mix(ACCENT, a) } else { TEXT_DIM });
            if dd_open {
                *dd_anchor = Some(ctl);
            }
            clicked.then_some(Action::Choose(k))
        }
        // information only
        "i" => {
            ui.text_in(row.value, Rect::new(r.x + r.w * 0.4, r.y, r.w * 0.6 - 8.0, r.h), 13.0, Weight::Medium, TEXT, Align::Right);
            None
        }
        // a value being typed
        "E" => {
            ui.p().rounded(ctl, 6.0, SURFACE_3);
            let caret = if (ui.time * 2.0) as i64 % 2 == 0 { "|" } else { "" };
            ui.text_in(&format!("{}{caret}", row.value), Rect::new(ctl.x + 12.0, ctl.y, ctl.w - 24.0, ctl.h), 13.0, Weight::Medium, TEXT, Align::Left);
            None
        }
        // a button with its text as the value
        _ => {
            let text = if row.value.is_empty() { omsi_ui::tr("Open").into_owned() } else { row.value.to_string() };
            let bw = (ui.width(&text, 13.0, Weight::Medium) + 36.0).clamp(96.0, ctl_w);
            let b = Rect::new(r.right() - 8.0 - bw, ctl.y, bw, ctl.h);
            let pressed = ui.button(&format!("pause-bt-{k}-{}", row.name), b, &text, None, ButtonKind::Normal);
            let (_, _, clicked) = ui.interact(id, Rect::new(r.x, r.y, b.x - r.x - 6.0, r.h));
            (pressed || clicked).then_some(Action::Choose(k))
        }
    }
}

/// Where the open drop-down's list lies under (or over) its field `a`.
fn dropdown_rect(a: Rect, n: usize, size: Vec2) -> Rect {
    let h = (n as f32 * 34.0 + 8.0).min(300.0);
    let below = size.y - a.bottom() - 12.0;
    let y = if below >= h || below >= a.y - 12.0 { a.bottom() + 4.0 } else { a.y - 4.0 - h };
    Rect::new(a.x, y, a.w, h.min(below.max(a.y - 12.0)))
}

/// The open drop-down as the launcher's: a list over everything, the value in force ticked.
fn dropdown(sh: &mut Shell, d: &DropdownView, a: Rect, kbd: bool) {
    let rr = dropdown_rect(a, d.items.len(), sh.ui.size);
    let ui = &mut sh.ui;
    // (over the scroll area's clip: a layer of its own over the whole window)
    ui.push_clip(Rect::new(0.0, 0.0, ui.size.x, ui.size.y), 0.0);
    ui.p().rounded(rr.inset(-2.0), 10.0, Color::rgba(0, 0, 0, 0.35));
    ui.p().rounded(rr, 8.0, SURFACE_2);
    let row = 34.0;
    let id = id_of("pause-dropdown");
    let content = d.items.len() as f32 * row + 8.0;
    let max = (content - rr.h).max(0.0);
    let mut scroll = ui.scroll.get(&id).copied().unwrap_or(d.top as f32 * row).clamp(0.0, max);
    if ui.hover(rr) && ui.input.wheel.y.abs() > 0.0 {
        scroll = (scroll - ui.input.wheel.y * 40.0).clamp(0.0, max);
        ui.input.wheel = Vec2::ZERO;
    }
    if kbd {
        let y = d.sel as f32 * row;
        if y < scroll {
            scroll = y;
        } else if y + row > scroll + rr.h - 8.0 {
            scroll = y + row - rr.h + 8.0;
        }
    }
    ui.scroll.insert(id, scroll);
    ui.push_clip(rr.inset(4.0), 8.0);
    let mut picked = None;
    for (i, item) in d.items.iter().enumerate() {
        let y = rr.y + 4.0 + i as f32 * row - scroll;
        if y + row < rr.y || y > rr.bottom() {
            continue;
        }
        let cell = Rect::new(rr.x + 4.0, y, rr.w - 8.0, row);
        let (h, _, clicked) = ui.interact(id_of(&format!("pause-dd-{i}")), cell);
        if d.current == Some(i) {
            ui.p().rounded(cell, 5.0, SURFACE_3);
            ui.icon("check", Vec2::new(cell.right() - 16.0, cell.center().y), 15.0, ACCENT);
        } else if h || (kbd && d.sel == i) {
            ui.p().rounded(cell, 5.0, SURFACE_3.alpha(0.6));
        }
        ui.text_in(item, Rect::new(cell.x + 10.0, cell.y, cell.w - 36.0, cell.h), 13.0, Weight::Regular, TEXT, Align::Left);
        if clicked {
            picked = Some(i);
        }
    }
    ui.pop_clip();
    if max > 0.0 {
        let bar_h = (rr.h * rr.h / content).max(24.0);
        let y = rr.y + (rr.h - bar_h) * (scroll / max);
        ui.p().rounded(Rect::new(rr.right() - 5.0, y, 3.0, bar_h), 1.5, Color::WHITE.alpha(0.3));
    }
    ui.pop_clip();
    if rr.contains(ui.input.mouse) {
        ui.over_ui = true;
    }
    match picked {
        Some(i) => sh.actions.push(Action::Dropdown(i)),
        // (a click anywhere else closes it, and does nothing else)
        None if ui.input.pressed && !rr.contains(ui.input.mouse) && !a.contains(ui.input.mouse) => sh.actions.push(Action::CloseDropdown),
        None => {}
    }
}

/// The lines of a list (drivers, fleet numbers, destinations, buttons, events ...): the
/// launcher's list rows in one panel.
fn plain_list(sh: &mut Shell, v: &PauseView, list: &ListView, body: Rect) {
    let w = body.w.min(720.0);
    let panel = Rect::new(body.x, body.y, w, body.h);
    sh.ui.panel(panel);
    let inner = Rect::new(panel.x + 10.0, panel.y + 10.0, panel.w - 20.0, panel.h - 20.0);
    list_rows(sh, v, list, inner, false);
}

/// The rows of a list in `r`, scrolled; `tours`: the chosen one stays marked.
fn list_rows(sh: &mut Shell, v: &PauseView, list: &ListView, r: Rect, keep_marked: bool) {
    let items = &list.items;
    let back_txt = omsi_ui::tr("Back").into_owned();
    let shown: Vec<usize> = (0..items.len()).filter(|&k| !(items[k].0 == "back" && plain(items[k].1).0 == back_txt)).collect();
    // (a list of codes and names - the destinations - in two columns)
    let code_w = shown.iter().filter_map(|&k| code_and_name(items[k].1)).map(|(c, _)| sh.ui.width(c, 13.0, Weight::Medium)).fold(0.0f32, f32::max);
    let any_hover = sh.ui.hover(r);
    let key = format!("pause-list-{}", list.key);
    let mut acts = Vec::new();
    let mut sel_y = None;
    let line_pre = format!("{} ", omsi_ui::tr("Line"));
    sh.ui.scroll_area(&key, r, &mut |ui, view| {
        let mut y = view.y;
        for &k in &shown {
            let (id, label) = items[k];
            if id == crate::game_lists::HEADING {
                ui.heading(Rect::new(view.x + 10.0, y + 6.0, view.w - 20.0, 28.0), label, None);
                y += 34.0;
                continue;
            }
            let row = Rect::new(view.x, y, view.w - 8.0, LIST_ROW - 4.0);
            let kbd = v.kbd && !any_hover && k == list.sel;
            if kbd {
                sel_y = Some(y - view.y);
            }
            let marked = (keep_marked && k == list.sel) || kbd;
            if ui.row(&format!("pause-lr-{}-{k}", list.key), row, marked) {
                acts.push(Action::Choose(k));
            }
            let h = ui.hover(row);
            let ink = if marked || h { TEXT } else { TEXT_SOFT };
            let cy = row.center().y;
            if id == "search" {
                let field = row.inset(2.0);
                ui.p().rounded(field, 6.0, SURFACE_2);
                ui.icon("search", Vec2::new(field.x + 16.0, cy), 16.0, TEXT_DIM);
                ui.text_in(label, Rect::new(field.x + 34.0, field.y, field.w - 44.0, field.h), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
            } else if let Some((name, info)) = label.strip_prefix(line_pre.as_str()).and_then(|rest| rest.rsplit_once("  (")).map(|(n, t)| (n, t.trim_end_matches(')'))) {
                // a line of the timetable: its number on a sign, as a line sign on a bus
                let sw = (ui.width(name, 14.0, Weight::Bold) + 22.0).max(48.0);
                let sign = Rect::new(row.x + 10.0, cy - 13.0, sw, 26.0);
                ui.p().rounded(sign, 6.0, ACCENT.mix(Color::rgba(150, 104, 30, 1.0), if marked || h { 0.0 } else { 0.3 }));
                ui.text_in(name, sign, 14.0, Weight::Bold, Color::rgba(18, 14, 8, 1.0), Align::Center);
                ui.text_in(info, Rect::new(sign.right() + 12.0, row.y, row.right() - sign.right() - 40.0, row.h), 13.0, Weight::Regular, ink, Align::Left);
                ui.icon("chevron_right", Vec2::new(row.right() - 16.0, cy), 16.0, TEXT_DIM);
            } else if let Some((code, name)) = code_and_name(label).filter(|_| code_w > 0.0) {
                ui.text_in(code, Rect::new(row.x + 12.0, row.y, code_w, row.h), 13.0, Weight::Medium, ACCENT, Align::Right);
                ui.text_in(name, Rect::new(row.x + 24.0 + code_w, row.y, row.w - code_w - 36.0, row.h), 13.0, Weight::Regular, ink, Align::Left);
            } else {
                let (text, more) = plain(label);
                // (a row of a settings list kind without its own window: its value at the right)
                let row_parts = parse_row(text);
                ui.text_in(row_parts.name, Rect::new(row.x + 12.0, row.y, row.w - 60.0, row.h), 13.0, Weight::Regular, ink, Align::Left);
                if !row_parts.value.is_empty() && row_parts.kind != "a" {
                    ui.text_in(row_parts.value, Rect::new(row.x + row.w * 0.5, row.y, row.w * 0.5 - 36.0, row.h), 12.5, Weight::Medium, TEXT_DIM, Align::Right);
                }
                if more {
                    ui.icon("chevron_right", Vec2::new(row.right() - 16.0, cy), 16.0, TEXT_DIM);
                }
            }
            y += LIST_ROW;
        }
        y - view.y + 4.0
    });
    if let Some(y) = sel_y {
        if sh.last_sel != Some((id_of(&key), list.sel)) {
            sh.ui.scroll_to(&key, y, LIST_ROW, r.h);
        }
    }
    sh.last_sel = Some((id_of(&key), list.sel));
    sh.actions.extend(acts);
}

/// "{code:>3}  {name}": a destination's code and name.
fn code_and_name(label: &str) -> Option<(&str, &str)> {
    let (c, n) = label.trim_start().split_once("  ")?;
    (!c.is_empty() && c.len() <= 5 && c.chars().all(|ch| ch.is_ascii_digit())).then_some((c, n.trim_start()))
}

/// The lines or a line's tours, with the timetable of the chosen one beside them.
fn timetable_lists(sh: &mut Shell, v: &PauseView, list: &ListView, body: Rect) {
    // (narrow - a phone: the timetable with its Start under the list instead of beside it;
    // without it a tour could be marked but not started)
    if let (Some(p), true) = (list.preview, body.w < 760.0) {
        let lh = (body.h * 0.38).max(120.0);
        let top = Rect::new(body.x, body.y, body.w, lh);
        sh.ui.panel(top);
        list_rows(sh, v, list, Rect::new(top.x + 10.0, top.y + 10.0, top.w - 20.0, top.h - 20.0), list.kind == MenuKind::Tours);
        preview(sh, p, list, Rect::new(body.x, top.bottom() + GAP, body.w, body.h - lh - GAP));
        return;
    }
    let pane_w = if list.preview.is_some() { (body.w * 0.46).min(440.0) } else { 0.0 };
    let lw = (body.w - pane_w - if pane_w > 0.0 { GAP * 2.0 } else { 0.0 }).min(620.0);
    let left = Rect::new(body.x, body.y, lw, body.h);
    sh.ui.panel(left);
    list_rows(sh, v, list, Rect::new(left.x + 10.0, left.y + 10.0, left.w - 20.0, left.h - 20.0), list.kind == MenuKind::Tours);
    if let Some(p) = list.preview {
        preview(sh, p, list, Rect::new(left.right() + GAP * 2.0, body.y, pane_w, body.h));
    }
}

/// The timetable beside the lines or tours: its title, the facts, the time with arrows,
/// the stops (one chosen to start from) and the button that starts the trip.
fn preview(sh: &mut Shell, p: &Preview, list: &ListView, r: Rect) {
    let mut acts = Vec::new();
    let ui = &mut sh.ui;
    ui.panel(r);
    let inner = Rect::new(r.x + 18.0, r.y + 16.0, r.w - 36.0, r.h - 32.0);
    ui.text_in(&p.title, Rect::new(inner.x, inner.y, inner.w, 24.0), 16.0, Weight::Bold, TEXT, Align::Left);
    ui.text_in(&p.meta, Rect::new(inner.x, inner.y + 26.0, inner.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
    let mut top = inner.y + 54.0;
    top += 6.0;
    if let Some(time) = p.time.as_ref() {
        let bh = 34.0;
        let prev = Rect::new(inner.x, top, 44.0, bh);
        let next = Rect::new(inner.right() - 44.0, top, 44.0, bh);
        // (the trip before: usize::MAX - 1, the next: usize::MAX - 2)
        for (b, icon, code) in [(prev, "arrow_back", usize::MAX - 1), (next, "arrow_forward", usize::MAX - 2)] {
            if ui.button(&format!("pause-trip-{icon}"), b, "", Some(icon), ButtonKind::Normal) {
                acts.push(Action::Pane(code));
            }
        }
        ui.text_in(time, Rect::new(prev.right() + 8.0, top, next.x - prev.right() - 16.0, bh), 18.0, Weight::Bold, ACCENT, Align::Center);
        top += bh + 10.0;
    }
    let lh = 28.0;
    match (p.chosen, p.button.as_ref()) {
        (Some(chosen), Some(button)) => {
            let go = Rect::new(inner.x, inner.bottom() - 44.0, inner.w, 44.0);
            let rows_r = Rect::new(inner.x - 8.0, top, inner.w + 16.0, (go.y - 10.0 - top).max(lh));
            let key = format!("pause-pane-{}", list.key);
            let mut first_view: Option<f32> = None;
            ui.scroll_area(&key, rows_r, &mut |ui, view| {
                for (i, (what, when)) in p.rows.iter().enumerate() {
                    let row = Rect::new(view.x, view.y + i as f32 * lh, view.w - 8.0, lh - 2.0);
                    if ui.row(&format!("pause-stop-{i}"), row, i == chosen) {
                        acts.push(Action::Pane(i));
                    }
                    if i == chosen {
                        first_view = Some(i as f32 * lh);
                    }
                    ui.text_in(when, Rect::new(row.right() - 70.0, row.y, 60.0, row.h), 12.5, Weight::Medium, ACCENT, Align::Right);
                    ui.text_in(what, Rect::new(row.x + 10.0, row.y, row.w - 90.0, row.h), 12.5, Weight::Regular, if i == chosen { TEXT } else { TEXT_SOFT }, Align::Left);
                }
                p.rows.len() as f32 * lh
            });
            if let (Some(y), None) = (first_view, list.pane_first) {
                if sh.last_sel.map(|s| s.1) != Some(chosen) {
                    sh.ui.scroll_to(&key, y, lh, rows_r.h);
                }
            }
            if sh.ui.button("pause-go", go, button, Some("play_arrow"), ButtonKind::Primary) {
                acts.push(Action::Pane(usize::MAX));
            }
        }
        _ => {
            let rows_r = Rect::new(inner.x, top, inner.w, inner.bottom() - top);
            let tp = format!("{} ", omsi_ui::tr("Tour"));
            let key = format!("pause-pane-{}", list.key);
            ui.scroll_area(&key, rows_r, &mut |ui, view| {
                for (i, (what, when)) in p.rows.iter().enumerate() {
                    let y = view.y + i as f32 * lh;
                    ui.text_in(when, Rect::new(view.right() - 78.0, y, 70.0, lh), 12.5, Weight::Medium, ACCENT, Align::Right);
                    match what.strip_prefix(tp.as_str()) {
                        Some(rest) => {
                            let (num, dest) = rest.split_once("  ›  ").unwrap_or((rest, ""));
                            let tw = (ui.width(num.trim(), 12.5, Weight::Bold) + 16.0).max(34.0);
                            let tile = Rect::new(view.x, y + 3.0, tw, lh - 6.0);
                            ui.p().rounded(tile, 5.0, ACCENT.alpha(0.16));
                            ui.text_in(num.trim(), tile, 12.5, Weight::Bold, ACCENT, Align::Center);
                            ui.text_in(dest, Rect::new(tile.right() + 10.0, y, view.w - tw - 100.0, lh), 12.5, Weight::Regular, TEXT_SOFT, Align::Left);
                        }
                        None => {
                            ui.text_in(what, Rect::new(view.x, y, view.w - 90.0, lh), 12.5, Weight::Regular, TEXT_SOFT, Align::Left);
                        }
                    }
                }
                p.rows.len() as f32 * lh
            });
        }
    }
    sh.actions.extend(acts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_that_opens_more_loses_its_dots() {
        assert_eq!(plain("Options..."), ("Options", true));
        assert_eq!(plain("Resume"), ("Resume", false));
        assert_eq!(plain("Fleet number..."), ("Fleet number", true));
    }

    #[test]
    fn a_settings_row_is_taken_apart() {
        let r = parse_row("Clouds\u{1f}s\u{1f}on\u{1f}Draws the clouds\u{1f}0.5");
        assert_eq!((r.name, r.kind, r.value, r.desc, r.frac), ("Clouds", "s", "on", "Draws the clouds", Some(0.5)));
        let r = parse_row("Plain");
        assert_eq!((r.name, r.kind, r.value), ("Plain", "a", ""));
    }

    #[test]
    fn destinations_are_codes_and_names() {
        assert_eq!(code_and_name(" 12  Hauptbahnhof"), Some(("12", "Hauptbahnhof")));
        assert_eq!(code_and_name("Line 5  (3 tours)"), None);
    }

    #[test]
    fn a_dropdown_opens_upwards_where_there_is_no_room_below() {
        let size = Vec2::new(1440.0, 880.0);
        let low = dropdown_rect(Rect::new(500.0, 800.0, 300.0, 34.0), 6, size);
        assert!(low.bottom() <= 800.0, "{low:?}");
        let high = dropdown_rect(Rect::new(500.0, 100.0, 300.0, 34.0), 6, size);
        assert!(high.y >= 134.0, "{high:?}");
    }
}
