//! OMSI 2's quick menu: Alt tapped alone opens a block of tiles at the bottom right of the
//! picture, the game going on behind it (Omsi.exe's `Tform_menu3`, 0x686480: a grid of 68
//! pixel tiles, each starting what the old toolbar's buttons did - 0x686e3c). Its tiles here
//! are those of OMSI's that the game has: OMSI's place, swap, remove and reset of the bus,
//! the timetable, the destination, the repair, the pump and the wash, the route arrows, the
//! time, the weather and the statistics; the AI driving the player's bus, the breakdown
//! call and the input devices' tile are left out. A tile with more than one thing to do
//! (refuel or wash, the trip's summary or the personnel file) opens a row of its own.

use super::{button, card, grow_row, note};
use omsi_plugin::ui::{Anchor, Panel};

/// The tiles: id, label, icon - in OMSI's order, four to a row.
pub(crate) const TILES: [(&str, &str, &str); 12] = [
    ("place", "Place a vehicle", "add"),
    ("switch", "Change vehicle", "sync_alt"),
    ("remove", "Remove vehicle", "delete"),
    ("reset", "Reset vehicle", "restart_alt"),
    ("duty", "Timetable", "departure_board"),
    ("dest", "Destination", "directions_bus"),
    ("repair", "Repair", "construction"),
    ("station", "Refuel / wash", "water_drop"),
    ("arrows", "Route arrows", "navigation"),
    ("time", "Time", "schedule"),
    ("weather", "Weather", "partly_cloudy_day"),
    ("stats", "Statistics", "receipt_long"),
];

/// The open quick menu and the tile whose own row is open.
#[derive(Default)]
pub(crate) struct QuickMenu {
    pub(super) sub: Option<&'static str>,
}

impl QuickMenu {
    pub(crate) fn panel(&self, app: &crate::App) -> Panel {
        self.panel_of(app.settings.nav_arrows, app.session.journey.as_ref().is_some_and(|j| j.has_summary()))
    }

    /// The menu with the route arrows on or off, and a trip summary to show again or none.
    pub(crate) fn panel_of(&self, arrows: bool, summary: bool) -> Panel {
        let mut children = Vec::new();
        for four in TILES.chunks(4) {
            children.push(grow_row(four.iter().map(|(id, label, icon)| super::Element { color: self.lit(arrows, id), ..button(id, label, Some(icon)) }).collect()));
        }
        match self.sub {
            Some("station") => children.push(grow_row(vec![button("refuel", "Refuel", Some("water_drop")), button("wash", "Wash", Some("water_drop"))])),
            Some("stats") => {
                let mut row = vec![button("career", "Personnel file", Some("badge"))];
                if summary {
                    row.insert(0, button("summary", "Trip summary", Some("receipt_long")));
                }
                children.push(grow_row(row));
            }
            _ => {}
        }
        children.push(note("Alt or Esc closes the menu; the game goes on."));
        // (bottom right, over the picture's corner as OMSI's)
        card(Anchor::BottomRight, -16.0, -16.0, 680.0, 228, children)
    }

    /// A tile that is on (the route arrows) or open (its own row) is drawn lit.
    fn lit(&self, arrows: bool, id: &str) -> Option<omsi_plugin::ui::Rgba> {
        let on = match id {
            "arrows" => arrows,
            other => self.sub == Some(other),
        };
        on.then_some(omsi_plugin::ui::Rgba([232, 160, 48, 255]))
    }
}

/// Alt tapped: the menu opens, or closes when it is open.
pub(crate) fn toggle(app: &mut crate::App) {
    let gp = &mut app.menus.game_panels;
    gp.quick = match gp.quick.take() {
        Some(_) => None,
        None => Some(QuickMenu::default()),
    };
}

/// A tile clicked: what it starts. The menu closes, unless the tile opened a row of its own.
pub(crate) fn click(app: &mut crate::App, id: &str) {
    let toggle_sub = |app: &mut crate::App, sub: &'static str| {
        if let Some(q) = app.menus.game_panels.quick.as_mut() {
            q.sub = if q.sub == Some(sub) { None } else { Some(sub) };
        }
    };
    match id {
        "station" => return toggle_sub(app, "station"),
        "stats" => return toggle_sub(app, "stats"),
        _ => {}
    }
    app.menus.game_panels.quick = None;
    match id {
        "place" | "switch" | "remove" | "reset" | "repair" | "refuel" | "wash" => {
            // (the game menu's own actions; those that open a list open the menu with it)
            if !app.page_action(id) {
                log::warn!("quick menu: no action {id}");
            }
        }
        "duty" => app.open_list(crate::game_lists::ListKind::Lines),
        "dest" => super::dest_form::open(app),
        "arrows" => {
            crate::game_lists::option_do(app, "nav_arrows", "", crate::game_lists::Move::Next);
        }
        "time" => {
            let tab = crate::game_lists::tab_of(app, crate::game_lists::ListKind::World(0), "Time");
            app.open_list(crate::game_lists::ListKind::World(tab));
        }
        "weather" => {
            let tab = crate::game_lists::tab_of(app, crate::game_lists::ListKind::World(0), "Weather");
            app.open_list(crate::game_lists::ListKind::World(tab));
        }
        "summary" => {
            if let Some(s) = app.session.journey.as_ref().and_then(|j| j.last_summary()) {
                app.menus.game_panels.trip = Some(super::trip_summary::TripPanel::new(s));
            }
        }
        "career" => app.open_list(crate::game_lists::ListKind::Drivers),
        _ => {}
    }
    if app.menus.chooser.is_some() && app.menus.game_menu.is_none() {
        app.open_game_menu();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_tiles_with_icons_the_game_has() {
        assert_eq!(TILES.len(), 12);
        for (id, _, icon) in TILES {
            assert!(omsi_ui::icons::svg(icon).is_some(), "{id}: no icon {icon}");
        }
        let ids: std::collections::HashSet<_> = TILES.iter().map(|t| t.0).collect();
        assert_eq!(ids.len(), 12);
    }

    #[test]
    fn a_tile_that_opens_a_list_opens_the_game_menu_under_it() {
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, Default::default());
        for tile in ["time", "weather", "duty", "career"] {
            toggle(&mut app);
            click(&mut app, tile);
            assert!(app.menus.chooser.is_some(), "{tile}: no list");
            assert!(app.menus.game_menu.is_some(), "{tile}: the list gets no input");
            app.close_list();
            app.close_game_menu();
        }
    }
}
