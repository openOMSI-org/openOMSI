//! OMSI 2's quick menu: Alt tapped alone opens a block of tiles at the bottom right of the
//! picture, the game going on behind it (Omsi.exe's `Tform_menu3`, 0x686480: a grid of square
//! tiles, each starting what the old toolbar's buttons did - 0x686e3c). As there, the tiles
//! are icons alone, five to a row and three rows to a page, lit when what they stand for is
//! on (the route arrows, the tour being driven); an arrow at the bottom right turns the page.
//! The name of the tile chosen - under the mouse or by the arrow keys - is written under them.
//!
//! The first page has OMSI's tiles in OMSI's places, those the game has: place, swap, remove
//! and reset the bus, the timetable, the destination, the repair, the pump and the wash, the
//! route arrows, the time, the weather, the controls and the statistics; the city map stands
//! where OMSI's AI driver was. The second has the game menu's own lines (the camera, the
//! options, saving, screenshots...). A tile whose line the game menu does not offer now is
//! greyed and takes no clicks. The statistics tile shows the last trip's summary again, or
//! the personnel file when there is none.

use super::{card, el, note, text, theme_color};
use omsi_plugin::ui::{Align, Anchor, Element, Kind, Panel, Rgba, RowAlign, Weight};
use omsi_ui::theme::Palette;
use omsi_ui::tr;

/// OMSI 2's lit tile: a deep orange-brown, the white icon on it.
const LIT: Rgba = Rgba([138, 72, 26, 255]);
pub(crate) const COLUMNS: usize = 5;
pub(crate) const PER_PAGE: usize = 15;
/// Five tiles of 84 and four gaps of 4, with the card's padding round them.
const WIDTH: f32 = 5.0 * 84.0 + 4.0 * 4.0 + 2.0 * 10.0;

/// The tiles: id, label, icon - in OMSI's order, five to a row, fifteen to a page.
pub(crate) const TILES: [(&str, &str, &str); 30] = [
    ("place", "Place a vehicle", "omsi_place"),
    ("switch", "Change vehicle", "omsi_switch"),
    ("remove", "Remove vehicle", "omsi_remove"),
    ("reset", "Reset vehicle", "omsi_reset"),
    ("map", "City map", "omsi_map"),
    ("duty", "Timetable", "omsi_duty"),
    ("dest", "Destination", "omsi_dest"),
    ("repair", "Repair", "omsi_repair"),
    ("refuel", "Refuel", "omsi_refuel"),
    ("wash", "Wash", "omsi_wash"),
    ("arrows", "Dynamic route arrows", "omsi_arrows"),
    ("time", "Time", "omsi_time"),
    ("weather", "Weather", "omsi_weather"),
    ("controls", "Controls", "omsi_controls"),
    ("stats", "Statistics", "omsi_stats"),
    // the second page: the game menu's lines
    ("camera", "Camera", "omsi_camera"),
    ("view_editor", "Seat, view and mirrors", "omsi_view_editor"),
    ("vehicle", "Vehicle options...", "omsi_vehicle"),
    ("world", "World options...", "omsi_world"),
    ("options", "Options", "omsi_options"),
    ("tobus", "Back to my bus", "omsi_tobus"),
    ("save", "Save the situation", "omsi_save"),
    ("saveslot", "Save to a slot...", "omsi_saveslot"),
    ("shot", "Screenshot", "omsi_shot"),
    ("photo", "Photo mode", "omsi_photo"),
    ("skipstop", "Skip the next stop", "omsi_skipstop"),
    ("endduty", "End the tour", "omsi_endduty"),
    ("players", "Player list", "omsi_players"),
    ("admin", "Administration", "omsi_admin"),
    ("radio", "Radio", "omsi_radio"),
];

/// The tiles that are the game menu's lines, offered only when it offers them now.
const MENU_LINES: [&str; 16] = [
    "map", "controls", "camera", "vehicle", "world", "options", "tobus", "save", "saveslot",
    "shot", "photo", "skipstop", "endduty", "players", "admin", "duty",
];

pub(crate) fn pages() -> usize {
    TILES.len().div_ceil(PER_PAGE)
}

/// The tiles of page `page`.
fn page_tiles(page: usize) -> &'static [(&'static str, &'static str, &'static str)] {
    let start = (page * PER_PAGE).min(TILES.len());
    &TILES[start..(start + PER_PAGE).min(TILES.len())]
}

/// A label as a name: without the dots of a menu line that opens a window.
fn name(label: &str) -> String {
    tr(label)
        .trim_end_matches("...")
        .trim_end_matches('…')
        .to_string()
}

/// The menu: its page and the tile chosen on it (by the mouse or the keys).
#[derive(Debug, Default)]
pub(crate) struct QuickMenu {
    pub(crate) page: usize,
    pub(crate) focus: usize,
    /// Where the cursor was when it last chose a tile: the keys' choice stays until it moves.
    pub(crate) cursor: Option<(f32, f32)>,
    pub(crate) mouse_kept: bool,
}

/// What the tiles show now: which are greyed and which are lit.
pub(crate) struct TileState {
    pub(crate) off: Vec<&'static str>,
    pub(crate) lit: Vec<&'static str>,
}

impl TileState {
    pub(crate) fn of(app: &crate::App) -> TileState {
        let offered: Vec<&'static str> = app.game_menu_items().iter().map(|l| l.0).collect();
        let mut off: Vec<&'static str> = MENU_LINES
            .iter()
            .copied()
            .filter(|id| !offered.contains(id))
            .collect();
        let mut lit = Vec::new();
        if app.settings.nav_dynamic_arrows {
            lit.push("arrows");
        }
        if app.session.duty.is_some() {
            lit.push("duty");
        }
        if app.player.is_none() || app.session.on_foot.is_some() || app.vr_active() {
            off.push("view_editor");
        }
        TileState { off, lit }
    }
}

impl QuickMenu {
    pub(crate) fn panel(&self, app: &crate::App) -> Panel {
        self.panel_of(
            &TileState::of(app),
            crate::themes::palette(app.settings.is_light_theme()),
        )
    }

    /// The tile chosen: its id.
    pub(crate) fn chosen(&self) -> Option<&'static str> {
        page_tiles(self.page).get(self.focus).map(|t| t.0)
    }

    /// The page shown, with the tiles as `state` has them.
    pub(crate) fn panel_of(&self, state: &TileState, pal: Palette) -> Panel {
        let muted = theme_color(pal.text_dim);
        let tiles = page_tiles(self.page);
        let mut children: Vec<Element> = tiles
            .chunks(COLUMNS)
            .enumerate()
            .map(|(r, line)| {
                let row = line
                    .iter()
                    .enumerate()
                    .map(|(c, (id, _, icon))| {
                        let focus = r * COLUMNS + c == self.focus;
                        let off = state.off.contains(id);
                        // (no text: a square of the icon alone, its name under the grid)
                        let kind = Kind::Tile {
                            text: String::new(),
                            icon: icon.to_string(),
                            focus,
                            off,
                        };
                        Element {
                            color: (!off && state.lit.contains(id)).then_some(LIT),
                            ..el(Some(id), kind)
                        }
                    })
                    .collect();
                el(
                    None,
                    Kind::Row {
                        children: row,
                        gap: 4.0,
                        align: RowAlign::Start,
                    },
                )
            })
            .collect();
        let (caption, off) = match tiles.get(self.focus) {
            Some((id, label, _)) => (name(label), state.off.contains(id)),
            None => (String::new(), false),
        };
        let mut foot = vec![Element {
            grow: true,
            ..text(&caption, 15.0, Weight::Bold)
        }];
        if off {
            foot[0].color = Some(muted);
            foot.push(Element {
                color: Some(muted),
                ..el(
                    None,
                    Kind::Text {
                        text: tr("Currently unavailable").into_owned(),
                        size: 12.0,
                        weight: Weight::Regular,
                        align: Align::Right,
                        wrap: false,
                    },
                )
            });
        }
        if pages() > 1 {
            let at = format!("{} / {}", self.page + 1, pages());
            foot.push(Element {
                color: Some(muted),
                ..el(
                    None,
                    Kind::Text {
                        text: at,
                        size: 12.0,
                        weight: Weight::Regular,
                        align: Align::Right,
                        wrap: false,
                    },
                )
            });
            foot.push(el(
                Some("next"),
                Kind::Button {
                    text: String::new(),
                    icon: Some("chevron_right".into()),
                },
            ));
        }
        children.push(el(
            None,
            Kind::Row {
                children: foot,
                gap: 10.0,
                align: RowAlign::Start,
            },
        ));
        children.push(note(
            &tr("Alt or Esc closes the menu; the game goes on."),
            pal,
        ));
        Panel {
            padding: 10.0,
            gap: 4.0,
            radius: 3.0,
            ..card(Anchor::BottomRight, 32.0, 32.0, WIDTH, 230, pal, children)
        }
    }

    /// The next page (after the last, the first), its first tile chosen.
    pub(crate) fn turn(&mut self, by: isize) {
        let n = pages() as isize;
        self.page = (self.page as isize + by).rem_euclid(n) as usize;
        self.focus = self
            .focus
            .min(page_tiles(self.page).len().saturating_sub(1));
    }

    /// An arrow key: the choice moves in the grid; past the side of a page, onto the next
    /// page's row (or the one before's), as OMSI's arrow turns it.
    pub(crate) fn step(&mut self, dx: isize, dy: isize) {
        let n = page_tiles(self.page).len() as isize;
        let (r, c) = (
            self.focus as isize / COLUMNS as isize,
            self.focus as isize % COLUMNS as isize,
        );
        if dy != 0 {
            let to = (r + dy) * COLUMNS as isize + c;
            if (0..n).contains(&to) {
                self.focus = to as usize;
            }
            return;
        }
        let c2 = c + dx;
        let to = r * COLUMNS as isize + c2;
        if (0..COLUMNS as isize).contains(&c2) && to < n {
            self.focus = to as usize;
            return;
        }
        if pages() < 2 {
            return;
        }
        self.turn(dx.signum());
        let n = page_tiles(self.page).len();
        let row = (r as usize * COLUMNS).min(n.saturating_sub(1) / COLUMNS * COLUMNS);
        let col = if dx > 0 { 0 } else { COLUMNS - 1 };
        self.focus = (row + col).min(n.saturating_sub(1));
    }

    /// A tile under the cursor at `cursor`: chosen, unless the cursor has not moved since
    /// (the keys' choice stays then).
    pub(crate) fn hover(&mut self, id: &str, cursor: (f32, f32)) {
        if self.cursor == Some(cursor) {
            return;
        }
        if let Some(i) = page_tiles(self.page).iter().position(|t| t.0 == id) {
            self.focus = i;
            self.cursor = Some(cursor);
        }
    }
}

/// Alt tapped: the menu opens (on its first page), or closes when it is open.
pub(crate) fn toggle(app: &mut crate::App) {
    let gp = &mut app.menus.game_panels;
    gp.quick = match gp.quick.take() {
        Some(_) => None,
        None => Some(QuickMenu {
            cursor: Some(app.input.cursor),
            ..Default::default()
        }),
    };
}

/// A key while the menu is open: the arrows choose, Enter starts the tile chosen, Tab and
/// the page keys turn the page. True when the key was the menu's.
pub(crate) fn key(app: &mut crate::App, code: winit::keyboard::KeyCode) -> bool {
    use winit::keyboard::KeyCode as K;
    let Some(q) = app
        .menus
        .game_panels
        .quick
        .as_mut()
        .filter(|q| !q.mouse_kept)
    else {
        return false;
    };
    match code {
        K::ArrowLeft => q.step(-1, 0),
        K::ArrowRight => q.step(1, 0),
        K::ArrowUp => q.step(0, -1),
        K::ArrowDown => q.step(0, 1),
        K::Tab | K::PageDown => q.turn(1),
        K::PageUp => q.turn(-1),
        K::Enter | K::NumpadEnter => {
            let chosen = q.chosen();
            if let Some(id) = chosen.filter(|id| !TileState::of(app).off.contains(id)) {
                start(app, id, false);
            }
        }
        _ => return false,
    }
    true
}

/// A tile clicked: what it starts, the menu closing (the page arrow turns the page).
pub(crate) fn click(app: &mut crate::App, id: &str) {
    start(app, id, true);
}

fn start(app: &mut crate::App, id: &str, mouse: bool) {
    if id == "next" {
        if let Some(q) = app.menus.game_panels.quick.as_mut() {
            q.turn(1);
        }
        return;
    }
    let kept = app.menus.game_panels.quick.take();
    match id {
        "dest" => super::dest_form::open(app),
        "radio" => super::radio_panel::open(app),
        "arrows" => {
            crate::game_lists::option_do(
                app,
                "nav_dynamic_arrows",
                "",
                crate::game_lists::Move::Next,
            );
        }
        "time" | "weather" => {
            let tab = crate::game_lists::tab_of(
                app,
                crate::game_lists::ListKind::World(0),
                if id == "time" { "Time" } else { "Weather" },
            );
            app.open_list(crate::game_lists::ListKind::World(tab));
        }
        "stats" => match app.session.journey.as_ref().and_then(|j| j.last_summary()) {
            Some(s) => app.menus.game_panels.trip = Some(super::trip_summary::TripPanel::new(s)),
            None => app.open_list(crate::game_lists::ListKind::Drivers),
        },
        // (the game menu's own lines and actions; those that open a list open the menu with it)
        other => {
            if !app.game_menu_action(other) {
                log::warn!("quick menu: no action {other}");
            }
        }
    }
    if app.menus.chooser.is_some() && app.menus.game_menu.is_none() {
        app.open_game_menu();
    }
    let elsewhere = app.menus.game_menu.is_some()
        || app.menus.chooser.is_some()
        || app.photo.is_some()
        || app.menus.navigator.as_ref().is_some_and(|n| n.map_open())
        || app.menus.game_panels.any();
    if !elsewhere {
        app.menus.game_panels.quick = kept.map(|q| QuickMenu {
            mouse_kept: q.mouse_kept || mouse,
            ..q
        });
    }
}

#[cfg(test)]
#[path = "quick_menu_tests.rs"]
mod tests;
