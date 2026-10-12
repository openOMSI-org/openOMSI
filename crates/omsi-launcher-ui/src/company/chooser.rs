//! The bus company's start (Luc: "a start screen for the bus company mode with a play mode,
//! singleplayer, multiplayer, and then per mode the companies the player has ... every time
//! people click the bus company they must be able to select their company, a bit like their
//! profile in the main menu"): first how to play - alone, or with others on a server (shown,
//! not open yet: it comes later) - then the driver's companies as tiles, the way the main
//! menu shows its drivers, and a tile to found a new one.
//!
//! It opens every time the bus company is entered from elsewhere (`choose`), not when its own
//! editors (lines, depots, liveries) give the company back. A company is deleted here too:
//! the bin in its tile's corner, asked once more (Luc: easy to delete on the start).

use super::super::theme::*;
use super::super::ui::{id_of, ButtonKind};
use super::super::Launcher;
use super::{company_mark, eur, kit, HOME_TAB};
use glam::Vec2;
use omsi_launcher_lib::company::Company;
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

/// Where the start is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Step {
    /// How to play: singleplayer or multiplayer.
    #[default]
    Mode,
    /// The companies of the mode chosen.
    Companies,
}

/// The start's state while it is open.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Chooser {
    pub step: Step,
    /// The company (its id) whose deleting is being asked.
    pub delete: Option<String>,
}

/// Open the start: the bus company was entered (a playing clock stops while it is open).
pub fn choose(l: &mut Launcher) {
    super::clock::play(l, 0.0);
    l.company.chooser = Some(Chooser::default());
}

/// Back to the start's companies from an open company (its pages' "Other company"): the
/// companies are chosen and founded there only (Luc).
pub(super) fn to_companies(l: &mut Launcher) {
    super::clock::play(l, 0.0);
    l.company.chooser = Some(Chooser { step: Step::Companies, delete: None });
}

/// What a click on the start did.
#[derive(Clone, Debug, PartialEq)]
enum Pick {
    Single,
    Back,
    Open(String),
    New,
    /// Its bin was clicked: the deleting asked.
    Delete(String),
}

/// The start in the company's content area.
pub(super) fn draw(l: &mut Launcher, area: Rect) {
    let step = l.company.chooser.as_ref().map(|c| c.step).unwrap_or_default();
    let list: Vec<Company> = l.company.companies.clone().unwrap_or_default();
    // (while the deleting is asked, the start under the question takes nothing)
    let asking = l.company.chooser.as_ref().and_then(|c| c.delete.clone());
    let held = asking.is_some().then(|| super::mask(&mut l.ui));
    let pick = match step {
        Step::Mode => modes(l, area, list.len()),
        Step::Companies => companies(l, area, &list),
    };
    if let Some(i) = held {
        l.ui.input = i;
    }
    if let Some(id) = asking {
        ask_delete(l, &id, &list);
        return;
    }
    match pick {
        Some(Pick::Single) => {
            if let Some(c) = l.company.chooser.as_mut() {
                c.step = Step::Companies;
            }
        }
        Some(Pick::Back) => {
            if let Some(c) = l.company.chooser.as_mut() {
                c.step = Step::Mode;
            }
        }
        Some(Pick::Open(id)) => {
            if let Some(c) = list.iter().find(|c| c.id == id).cloned() {
                super::switch_company(l, Some(c));
                l.company.chooser = None;
                l.company.tab = HOME_TAB;
                l.company.home_cat = 0;
            }
        }
        // (the founding; its Cancel comes back here, its company opens on its start screen)
        Some(Pick::New) => l.company.wizard = Some(super::wizard::Wizard::new(l)),
        Some(Pick::Delete(id)) => {
            if let Some(c) = l.company.chooser.as_mut() {
                c.delete = Some(id);
            }
        }
        None => {}
    }
}

/// The head over a step: its title and what it is about, centred over `w`.
fn head(l: &mut Launcher, area: Rect, title: &str, text: &str) -> f32 {
    l.ui.text_in(title, Rect::new(area.x, area.y, area.w, 34.0), 26.0, Weight::Bold, TEXT, Align::Center);
    l.ui.text_in(text, Rect::new(area.x, area.y + 40.0, area.w, 22.0), kit::BODY, Weight::Regular, TEXT_SOFT, Align::Center);
    area.y + 92.0
}

/// How to play: two cards side by side, the multiplayer one shown but not open yet.
fn modes(l: &mut Launcher, area: Rect, n: usize) -> Option<Pick> {
    let (cw, ch, gap) = (380.0f32.min((area.w - 24.0) / 2.0), 250.0, 24.0);
    let total_h = 92.0 + ch;
    let top = area.y + ((area.h - total_h) * 0.4).max(0.0);
    let y = head(l, Rect::new(area.x, top, area.w, 70.0), "How do you want to play?", "Your own company in a time of its own, or one run together with others.");
    let x0 = area.x + (area.w - (2.0 * cw + gap)) * 0.5;
    let single = Rect::new(x0, y, cw, ch);
    let multi = Rect::new(x0 + cw + gap, y, cw, ch);
    let count = if n == 1 { omsi_ui::tr("1 company on this computer").into_owned() } else { omsi_ui::tr("%{n} companies on this computer").replace("%{n}", &n.to_string()) };
    let mut out = None;
    if card(l, "company-mode-single", single, "person", "Singleplayer", "Found and run your own companies, in a time of their own that runs while you play.", &count, true) {
        out = Some(Pick::Single);
    }
    card(l, "company-mode-multi", multi, "groups", "Multiplayer", "Run a company together on a server: an owner, planners and drivers who take its duties.", &omsi_ui::tr("Coming later"), false);
    l.ui.tooltip(multi, "Multiplayer companies come in a later version.");
    out
}

/// A card of a play mode: its icon, name, words and a line at its foot; `open` false: faint,
/// a "coming later" mark in its corner, and no click. Returns whether it was clicked.
#[allow(clippy::too_many_arguments)]
fn card(l: &mut Launcher, id: &str, base: Rect, icon: &str, title: &str, text: &str, foot: &str, open: bool) -> bool {
    let t = l.ui.tile(id_of(id), base, SHEET_RADIUS);
    let r = if open { t.r } else { base };
    let hover = if open { t.hover } else { 0.0 };
    if open {
        l.ui.tile_shadow(&t);
    }
    let fade = if open { 1.0 } else { 0.55 };
    l.ui.p().rounded(r, SHEET_RADIUS, PANEL.mix(HOVER, hover));
    if open {
        l.ui.tile_light(&t, 0.05);
        l.ui.tile_edge(&t, 1.0, EDGE);
    } else {
        l.ui.p().rounded_border(r, SHEET_RADIUS, 1.0, EDGE);
    }
    let mark = Rect::new(r.x + 26.0, r.y + 26.0, 56.0, 56.0);
    l.ui.p().rounded(mark, RADIUS, accent().alpha(0.14 * fade).mix(accent(), hover));
    l.ui.icon(icon, mark.center(), 30.0, accent_2().alpha(fade).mix(on_accent(), hover));
    if !open {
        // (the mark that it is not open yet, in its corner)
        let words = omsi_ui::tr("Coming later");
        let bw = l.ui.width(&words, 12.0, Weight::Bold) + 22.0;
        let b = Rect::new(r.right() - 22.0 - bw, r.y + 22.0, bw, 26.0);
        l.ui.p().rounded(b, 13.0, WARN.alpha(0.18));
        l.ui.text_in(&words, b, 12.0, Weight::Bold, WARN, Align::Center);
    }
    l.ui.text_in(title, Rect::new(r.x + 26.0, mark.bottom() + 18.0, r.w - 52.0, 30.0), 22.0, Weight::Bold, TEXT.alpha(fade), Align::Left);
    l.ui.paragraph(text, Vec2::new(r.x + 26.0, mark.bottom() + 54.0), r.w - 52.0, kit::BODY, Weight::Regular, TEXT_SOFT.alpha(fade));
    l.ui.text_in(foot, Rect::new(r.x + 26.0, r.bottom() - 42.0, r.w - 52.0, 20.0), kit::NOTE, Weight::Medium, if open { accent_2() } else { TEXT_DIM }, Align::Left);
    open && t.clicked
}

/// The companies as tiles (the main menu's drivers' way), a tile to found a new one after
/// them, and the way back to the play modes.
fn companies(l: &mut Launcher, area: Rect, list: &[Company]) -> Option<Pick> {
    let mut out = None;
    let back = Rect::new(area.x, area.y, 180.0, 38.0);
    if l.ui.button("company-chooser-back", back, "Play mode", Some("chevron_left"), ButtonKind::Normal) {
        out = Some(Pick::Back);
    }
    let (tw, th, gap) = (300.0, 176.0, 16.0);
    let count = list.len() + 1;
    let room = Rect::new(area.x, back.bottom() + 12.0, area.w, (area.h - back.h - 12.0).max(0.0));
    let per_row = (((room.w + gap) / (tw + gap)).floor() as usize).clamp(1, 4);
    let rows = count.div_ceil(per_row);
    let grid_h = rows as f32 * (th + gap) - gap;
    let title = if list.is_empty() { "Found your first company" } else { "Your companies" };
    let text = if list.is_empty() { "You have no company yet: give it a name, colours, a map and a depot." } else { "Choose the company to open, or found a new one." };
    let top = room.y + ((room.h - 92.0 - grid_h) * 0.35).max(0.0);
    let y0 = head(l, Rect::new(room.x, top, room.w, 70.0), title, text);
    // (more than the window holds: the tiles scroll)
    let grid_r = Rect::new(room.x, y0, room.w, (room.bottom() - y0).max(0.0));
    let mut tiles = |l: &mut Launcher, y: f32| {
        for k in 0..count {
            let (row, col) = (k / per_row, k % per_row);
            let in_row = (count - row * per_row).min(per_row);
            let row_w = in_row as f32 * (tw + gap) - gap;
            let base = Rect::new(room.x + (room.w - row_w) * 0.5 + col as f32 * (tw + gap), y + row as f32 * (th + gap), tw, th);
            match list.get(k) {
                Some(c) => match company_tile(l, base, c) {
                    Some(true) => out = Some(Pick::Open(c.id.clone())),
                    Some(false) => out = Some(Pick::Delete(c.id.clone())),
                    None => {}
                },
                None => {
                    if new_tile(l, base) {
                        out = Some(Pick::New);
                    }
                }
            }
        }
    };
    if grid_h <= grid_r.h + 0.5 {
        tiles(l, y0);
    } else {
        let off = l.ui.scroll.get(&id_of("company-chooser")).copied().unwrap_or(0.0);
        l.ui.push_clip(grid_r, 0.0);
        tiles(l, y0 - off);
        l.ui.pop_clip();
        l.ui.scroll_keep("company-chooser", grid_r, grid_h + 20.0);
    }
    out
}

/// A company's tile: its mark with how it is run beside it (its mode, the corner kept free),
/// its name, where it is at home, its day and its cash; under the mouse a bin in its corner.
/// Some(true): opened, Some(false): its bin clicked.
fn company_tile(l: &mut Launcher, base: Rect, c: &Company) -> Option<bool> {
    let t = l.ui.tile(id_of(&format!("company-chooser-{}", c.id)), base, SHEET_RADIUS);
    let r = t.r;
    l.ui.tile_shadow(&t);
    l.ui.p().rounded(r, SHEET_RADIUS, PANEL.mix(HOVER, t.hover));
    l.ui.tile_light(&t, 0.05);
    l.ui.tile_edge(&t, 1.0, EDGE.mix(accent(), t.hover * 0.6));
    let mark = Rect::new(r.x + 20.0, r.y + 22.0, 56.0, 56.0);
    company_mark(l, mark, c);
    // (the mode at the mark's top, its clock's line under it: the two side by side overlapped)
    let line = super::clock::tile_line(c);
    let pill_y = if line.is_some() { mark.y + 2.0 } else { mark.center().y - 11.0 };
    let pw = super::mode_pill_w(&l.ui, c.mode).min((r.right() - 56.0 - mark.right() - 14.0).max(0.0));
    super::mode_pill(&mut l.ui, "", Rect::new(mark.right() + 14.0, pill_y, pw, 22.0), c.mode, false);
    let w = r.w - 40.0;
    l.ui.text_in(&c.name, Rect::new(r.x + 20.0, mark.bottom() + 14.0, w, 22.0), 16.0, Weight::Bold, TEXT, Align::Left);
    let map = if c.map_name.is_empty() { super::super::state::short_map(&c.map) } else { c.map_name.clone() };
    let home = if c.depot.is_empty() { map } else { format!("{map}  ·  {}", c.depot) };
    l.ui.text_in(&home, Rect::new(r.x + 20.0, mark.bottom() + 40.0, w, 18.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    let day = format!("{}  ·  {}", super::day_label(&c.date), eur(c.cash));
    l.ui.text_in(&day, Rect::new(r.x + 20.0, mark.bottom() + 60.0, w, 18.0), kit::NOTE, Weight::Medium, if c.cash < 0 { DANGER } else { TEXT_DIM }, Align::Left);
    // (beside the mark, its clock against the real one: what waits to be caught up, or was)
    if let Some((t, colour)) = line {
        let cy = mark.y + 42.0;
        l.ui.p().circle(Vec2::new(mark.right() + 20.0, cy), 3.5, colour);
        l.ui.text_in(&t, Rect::new(mark.right() + 30.0, cy - 10.0, (r.right() - 20.0 - mark.right() - 30.0).max(0.0), 20.0), kit::NOTE, Weight::Medium, colour, Align::Left);
    }
    // (under the mouse: the bin in the corner - as the main menu's drivers have it)
    let bin = Vec2::new(r.right() - 24.0, r.y + 24.0);
    let on_bin = l.ui.input.mouse.distance(bin) <= 15.0;
    if t.hover > 0.3 && l.ui.icon_button(&format!("company-chooser-delete-{}", c.id), bin, 14.0, "delete", "Delete this company") {
        return Some(false);
    }
    (t.clicked && !on_bin).then_some(true)
}

/// The question whether to delete the company `id` for good (its buses, staff, lines, money
/// and history; the driver's own record stays).
fn ask_delete(l: &mut Launcher, id: &str, list: &[Company]) {
    let name = list.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_default();
    let title = omsi_ui::tr("Delete %{name}?").replace("%{name}", &name);
    let text = omsi_ui::tr("The company is deleted for good: its buses, staff, lines, money and history. Your service record as a driver stays. This cannot be undone.");
    let h = 70.0 + l.ui.paragraph_height(&text, 520.0 - 56.0, kit::BODY, Weight::Regular) + 24.0 + kit::BUTTON_H + 24.0;
    let f = kit::frame(l, 560.0, h.max(220.0), "warning", &title);
    l.ui.paragraph(&text, Vec2::new(f.body.x, f.body.y), f.body.w, kit::BODY, Weight::Regular, TEXT_SOFT);
    let mut foot = kit::Foot::new(&f);
    let yes = foot.right(l, "company-chooser-delete-yes", "Delete for good", None, ButtonKind::Danger);
    let no = foot.right(l, "company-chooser-delete-no", "Cancel", None, ButtonKind::Normal) || f.close || l.ui.input.keys.contains(&super::super::ui::Key::Escape);
    if yes || no {
        if let Some(c) = l.company.chooser.as_mut() {
            c.delete = None;
        }
    }
    if yes {
        super::delete_company(l, id);
    }
}

/// The tile to found a new company.
fn new_tile(l: &mut Launcher, base: Rect) -> bool {
    let t = l.ui.tile(id_of("company-chooser-new"), base, SHEET_RADIUS);
    let r = t.r;
    l.ui.tile_shadow(&t);
    l.ui.p().rounded(r, SHEET_RADIUS, PANEL.mix(HOVER, t.hover).alpha(0.7));
    l.ui.tile_light(&t, 0.05);
    l.ui.tile_edge(&t, 1.0, accent().alpha(0.35 + 0.5 * t.hover));
    let c = Vec2::new(r.center().x, r.y + 64.0);
    l.ui.p().circle(c, 26.0, accent().alpha(0.16).mix(accent(), t.hover));
    l.ui.icon("add", c, 28.0, accent_2().mix(on_accent(), t.hover));
    l.ui.text_in("New company", Rect::new(r.x + 16.0, r.y + 102.0, r.w - 32.0, 22.0), 16.0, Weight::Bold, TEXT, Align::Center);
    l.ui.text_in("Its name, colours, map and depot", Rect::new(r.x + 16.0, r.y + 128.0, r.w - 32.0, 18.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Center);
    t.clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start opens on the play modes.
    #[test]
    fn it_opens_on_the_play_modes() {
        assert_eq!(Chooser::default().step, Step::Mode);
    }
}
