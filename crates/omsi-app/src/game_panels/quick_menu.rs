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

use super::{card, el, note, text, theme, theme_color};
use omsi_plugin::ui::{Align, Anchor, Element, Kind, Panel, Rgba, RowAlign, Weight};
use omsi_ui::tr;

/// OMSI 2's lit tile: a deep orange-brown, the white icon on it.
const LIT: Rgba = Rgba([138, 72, 26, 255]);
pub(crate) const COLUMNS: usize = 5;
pub(crate) const PER_PAGE: usize = 15;
/// Five tiles of 84 and four gaps of 8, with the card's padding round them.
const WIDTH: f32 = 5.0 * 84.0 + 4.0 * 8.0 + 2.0 * 14.0;

/// The tiles: id, label, icon - in OMSI's order, five to a row, fifteen to a page.
pub(crate) const TILES: [(&str, &str, &str); 28] = [
    ("place", "Place a vehicle", "add"),
    ("switch", "Change vehicle", "sync_alt"),
    ("remove", "Remove vehicle", "delete"),
    ("reset", "Reset vehicle", "restart_alt"),
    ("map", "City map", "map"),
    ("duty", "Timetable", "departure_board"),
    ("dest", "Destination", "directions_bus"),
    ("repair", "Repair", "construction"),
    ("refuel", "Refuel", "local_gas_station"),
    ("wash", "Wash", "local_car_wash"),
    ("arrows", "Route arrows", "navigation"),
    ("time", "Time", "schedule"),
    ("weather", "Weather", "partly_cloudy_day"),
    ("controls", "Controls", "sports_esports"),
    ("stats", "Statistics", "receipt_long"),
    // the second page: the game menu's lines
    ("camera", "Camera", "videocam"),
    ("vehicle", "Vehicle options...", "tune"),
    ("world", "World options...", "public"),
    ("options", "Options", "settings"),
    ("tobus", "Back to my bus", "directions_walk"),
    ("save", "Save the situation", "save"),
    ("saveslot", "Save to a slot...", "sd_card"),
    ("shot", "Screenshot", "photo_camera"),
    ("photo", "Photo mode", "visibility"),
    ("skipstop", "Skip the next stop", "stop_request"),
    ("endduty", "End the tour", "sports_score"),
    ("players", "Player list", "group"),
    ("admin", "Administration", "dns"),
];

/// The tiles that are the game menu's lines, offered only when it offers them now.
const MENU_LINES: [&str; 16] = [
    "map", "controls", "camera", "vehicle", "world", "options", "tobus", "save", "saveslot", "shot", "photo", "skipstop", "endduty",
    "players", "admin", "duty",
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
    tr(label).trim_end_matches("...").trim_end_matches('…').to_string()
}

/// The menu: its page and the tile chosen on it (by the mouse or the keys).
#[derive(Debug, Default)]
pub(crate) struct QuickMenu {
    pub(crate) page: usize,
    pub(crate) focus: usize,
    /// Where the cursor was when it last chose a tile: the keys' choice stays until it moves.
    pub(crate) cursor: Option<(f32, f32)>,
}

/// What the tiles show now: which are greyed and which are lit.
pub(crate) struct TileState {
    pub(crate) off: Vec<&'static str>,
    pub(crate) lit: Vec<&'static str>,
}

impl TileState {
    pub(crate) fn of(app: &crate::App) -> TileState {
        let offered: Vec<&'static str> = app.game_menu_items().iter().map(|l| l.0).collect();
        let off = MENU_LINES.iter().copied().filter(|id| !offered.contains(id)).collect();
        let mut lit = Vec::new();
        if app.settings.nav_arrows {
            lit.push("arrows");
        }
        if app.session.duty.is_some() {
            lit.push("duty");
        }
        TileState { off, lit }
    }
}

impl QuickMenu {
    pub(crate) fn panel(&self, app: &crate::App) -> Panel {
        self.panel_of(&TileState::of(app), app.settings.is_light_theme())
    }

    /// The tile chosen: its id.
    pub(crate) fn chosen(&self) -> Option<&'static str> {
        page_tiles(self.page).get(self.focus).map(|t| t.0)
    }

    /// The page shown, with the tiles as `state` has them.
    pub(crate) fn panel_of(&self, state: &TileState, light: bool) -> Panel {
        let muted = theme_color(theme(light).TEXT_DIM);
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
                        let kind = Kind::Tile { text: String::new(), icon: icon.to_string(), focus, off };
                        Element { color: (!off && state.lit.contains(id)).then_some(LIT), ..el(Some(id), kind) }
                    })
                    .collect();
                el(None, Kind::Row { children: row, gap: 8.0, align: RowAlign::Start })
            })
            .collect();
        let (caption, off) = match tiles.get(self.focus) {
            Some((id, label, _)) => (name(label), state.off.contains(id)),
            None => (String::new(), false),
        };
        let mut foot = vec![Element { grow: true, ..text(&caption, 15.0, Weight::Bold) }];
        if off {
            foot[0].color = Some(muted);
            foot.push(Element { color: Some(muted), ..el(None, Kind::Text { text: tr("Currently unavailable").into_owned(), size: 12.0, weight: Weight::Regular, align: Align::Right, wrap: false }) });
        }
        if pages() > 1 {
            let at = format!("{} / {}", self.page + 1, pages());
            foot.push(Element { color: Some(muted), ..el(None, Kind::Text { text: at, size: 12.0, weight: Weight::Regular, align: Align::Right, wrap: false }) });
            foot.push(el(Some("next"), Kind::Button { text: String::new(), icon: Some("chevron_right".into()) }));
        }
        children.push(el(None, Kind::Row { children: foot, gap: 10.0, align: RowAlign::Start }));
        children.push(note(&tr("Alt or Esc closes the menu; the game goes on."), light));
        Panel { padding: 14.0, gap: 8.0, ..card(Anchor::BottomRight, 32.0, 32.0, WIDTH, 215, light, children) }
    }

    /// The next page (after the last, the first), its first tile chosen.
    pub(crate) fn turn(&mut self, by: isize) {
        let n = pages() as isize;
        self.page = (self.page as isize + by).rem_euclid(n) as usize;
        self.focus = self.focus.min(page_tiles(self.page).len().saturating_sub(1));
    }

    /// An arrow key: the choice moves in the grid; past the side of a page, onto the next
    /// page's row (or the one before's), as OMSI's arrow turns it.
    pub(crate) fn step(&mut self, dx: isize, dy: isize) {
        let n = page_tiles(self.page).len() as isize;
        let (r, c) = (self.focus as isize / COLUMNS as isize, self.focus as isize % COLUMNS as isize);
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
        None => Some(QuickMenu { cursor: Some(app.input.cursor), ..Default::default() }),
    };
}

/// A key while the menu is open: the arrows choose, Enter starts the tile chosen, Tab and
/// the page keys turn the page. True when the key was the menu's.
pub(crate) fn key(app: &mut crate::App, code: winit::keyboard::KeyCode) -> bool {
    use winit::keyboard::KeyCode as K;
    let Some(q) = app.menus.game_panels.quick.as_mut() else { return false };
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
                click(app, id);
            }
        }
        _ => return false,
    }
    true
}

/// A tile clicked: what it starts, the menu closing (the page arrow turns the page).
pub(crate) fn click(app: &mut crate::App, id: &str) {
    if id == "next" {
        if let Some(q) = app.menus.game_panels.quick.as_mut() {
            q.turn(1);
        }
        return;
    }
    app.menus.game_panels.quick = None;
    match id {
        "dest" => super::dest_form::open(app),
        "arrows" => {
            crate::game_lists::option_do(app, "nav_arrows", "", crate::game_lists::Move::Next);
        }
        "time" | "weather" => {
            let tab = crate::game_lists::tab_of(app, crate::game_lists::ListKind::World(0), if id == "time" { "Time" } else { "Weather" });
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> crate::App {
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        crate::assemble_app(args, Default::default())
    }

    #[test]
    fn the_tiles_have_icons_and_omsis_first_page() {
        for (id, _, icon) in TILES {
            assert!(omsi_ui::icons::svg(icon).is_some(), "{id}: no icon {icon}");
        }
        let ids: std::collections::HashSet<_> = TILES.iter().map(|t| t.0).collect();
        assert_eq!(ids.len(), TILES.len());
        assert_eq!(pages(), 2);
        assert_eq!(page_tiles(0).len(), 15);
        assert!(omsi_ui::icons::svg("chevron_right").is_some());
    }

    #[test]
    fn every_menu_line_tile_is_a_line_of_the_game_menu() {
        for id in MENU_LINES {
            assert!(TILES.iter().any(|t| t.0 == id), "{id}");
        }
        let all: Vec<&str> = crate::app_impl::GAME_MENU.iter().map(|l| l.0).chain(["tobus", "players", "admin"]).collect();
        for id in MENU_LINES {
            assert!(all.contains(&id), "{id} is no game menu line");
        }
    }

    #[test]
    fn the_arrows_walk_the_grid_and_turn_the_page() {
        let mut q = QuickMenu::default();
        q.step(1, 0);
        assert_eq!((q.page, q.focus), (0, 1));
        q.step(0, 1);
        assert_eq!((q.page, q.focus), (0, 6));
        q.step(0, 1);
        q.step(0, 1);
        assert_eq!(q.focus, 11, "no row under the last");
        q.focus = 14;
        q.step(1, 0);
        // the second page has 13 tiles: its third row ends at 12
        assert_eq!((q.page, q.focus), (1, 10));
        q.step(-1, 0);
        assert_eq!((q.page, q.focus), (0, 14));
        q.focus = 0;
        q.step(-1, 0);
        assert_eq!((q.page, q.focus), (1, 4));
        q.turn(1);
        assert_eq!(q.page, 0);
    }

    #[test]
    fn a_still_cursor_leaves_the_keys_choice() {
        let mut q = QuickMenu { cursor: Some((5.0, 5.0)), ..Default::default() };
        q.hover("remove", (5.0, 5.0));
        assert_eq!(q.focus, 0);
        q.hover("remove", (6.0, 5.0));
        assert_eq!(q.focus, 2);
        q.step(1, 0);
        q.hover("remove", (6.0, 5.0));
        assert_eq!(q.focus, 3);
    }

    #[test]
    fn the_panel_shows_the_chosen_tiles_name_and_the_lit_and_greyed_ones() {
        let q = QuickMenu { focus: 7, ..Default::default() };
        let p = q.panel_of(&TileState { off: vec!["map"], lit: vec!["arrows"] }, false);
        let tiles: Vec<&Element> = p.children.iter().take(3).flat_map(|r| match &r.kind {
            Kind::Row { children, .. } => children.iter().collect::<Vec<_>>(),
            _ => vec![],
        }).collect();
        assert_eq!(tiles.len(), 15);
        let of = |id: &str| tiles.iter().find(|t| t.id.as_deref() == Some(id)).unwrap();
        assert!(matches!(of("repair").kind, Kind::Tile { focus: true, .. }));
        assert!(matches!(of("map").kind, Kind::Tile { off: true, .. }));
        assert!(!of("map").takes_clicks());
        assert_eq!(of("arrows").color, Some(LIT));
        let Kind::Row { children: foot, .. } = &p.children[3].kind else { panic!() };
        assert!(matches!(&foot[0].kind, Kind::Text { text, .. } if text == "Repair"));
        assert!(foot.iter().any(|e| e.id.as_deref() == Some("next")));
    }

    #[test]
    fn keys_open_and_start_tiles() {
        use winit::keyboard::KeyCode as K;
        let mut app = app();
        assert!(!key(&mut app, K::ArrowRight), "no menu, no key");
        toggle(&mut app);
        // to "Time" (row 3, column 2) and Enter: the world window's Time page
        for k in [K::ArrowDown, K::ArrowDown, K::ArrowRight] {
            assert!(key(&mut app, k));
        }
        assert_eq!(app.menus.game_panels.quick.as_ref().and_then(|q| q.chosen()), Some("time"));
        assert!(key(&mut app, K::Enter));
        assert!(app.menus.game_panels.quick.is_none());
        assert!(app.menus.chooser.is_some());
        app.close_list();
        app.close_game_menu();
    }

    #[test]
    fn game_panels_follow_the_light_theme_setting() {
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, Default::default());
        app.settings.ui_theme = "light".into();
        toggle(&mut app);
        app.frame_game_panels(0.0, [0.0, 0.0, 1280.0, 720.0], false);

        let panel = app.menus.game_panels.ui.panels().iter().find(|p| p.id == "quick").unwrap();
        let background = panel.panel.background.unwrap();
        assert!(background.0[0] > 230 && background.0[1] > 230 && background.0[2] > 230, "{background:?}");
    }

    #[test]
    fn a_tile_that_opens_a_list_opens_the_game_menu_under_it() {
        let mut app = app();
        for tile in ["time", "weather", "stats", "controls", "options", "camera"] {
            toggle(&mut app);
            click(&mut app, tile);
            assert!(app.menus.chooser.is_some(), "{tile}: no list");
            assert!(app.menus.game_menu.is_some(), "{tile}: the list gets no input");
            app.close_list();
            app.close_game_menu();
        }
    }

    #[test]
    fn the_page_arrow_turns_the_page_and_keeps_the_menu() {
        let mut app = app();
        toggle(&mut app);
        click(&mut app, "next");
        assert_eq!(app.menus.game_panels.quick.as_ref().map(|q| q.page), Some(1));
        click(&mut app, "next");
        assert_eq!(app.menus.game_panels.quick.as_ref().map(|q| q.page), Some(0));
    }

    #[test]
    fn a_tile_leaves_the_pause_as_it_was() {
        let mut app = app();
        for (paused, stale) in [(true, false), (false, true)] {
            for tile in ["reset", "remove", "repair"] {
                app.paused = paused;
                app.menus.menu_prev_pause = stale;
                toggle(&mut app);
                click(&mut app, tile);
                assert_eq!(app.paused, paused, "{tile}");
            }
        }
    }
}
