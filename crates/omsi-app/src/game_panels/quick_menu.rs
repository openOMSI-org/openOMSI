//! OMSI 2's quick menu: Alt tapped alone opens a block of tiles at the bottom right of the
//! picture, the game going on behind it (Omsi.exe's `Tform_menu3`, 0x686480: a grid of 68
//! pixel tiles, each starting what the old toolbar's buttons did - 0x686e3c). Its tiles here
//! are those of OMSI's that the game has: OMSI's place, swap, remove and reset of the bus,
//! the timetable, the destination, the repair, the pump and the wash, the route arrows, the
//! time and the weather in one, and the statistics; the AI driving the player's bus, the
//! breakdown call and the input devices' tile are left out. The statistics tile shows the last
//! trip's summary again, or the personnel file when there is none.

use super::{card, el, note};
use omsi_plugin::ui::{Anchor, Element, Kind, Panel, Rgba, RowAlign};
use omsi_ui::tr;

const ACCENT: Rgba = Rgba([232, 160, 48, 255]);
const COLUMNS: usize = 6;

fn tile(id: &str, label: &str, icon: &str) -> Element {
    el(Some(id), Kind::Tile { text: tr(label).into_owned(), icon: icon.to_string() })
}

/// The tiles: id, label, icon - in OMSI's order, six to a row.
pub(crate) const TILES: [(&str, &str, &str); 12] = [
    ("place", "Place a vehicle", "add"),
    ("switch", "Change vehicle", "sync_alt"),
    ("remove", "Remove vehicle", "delete"),
    ("reset", "Reset vehicle", "restart_alt"),
    ("duty", "Timetable", "departure_board"),
    ("dest", "Destination", "directions_bus"),
    ("repair", "Repair", "construction"),
    ("refuel", "Refuel", "local_gas_station"),
    ("wash", "Wash", "local_car_wash"),
    ("arrows", "Route arrows", "navigation"),
    ("world", "Time & weather", "partly_cloudy_day"),
    ("stats", "Statistics", "receipt_long"),
];

pub(crate) struct QuickMenu;

impl QuickMenu {
    pub(crate) fn panel(&self, app: &crate::App) -> Panel {
        self.panel_of(app.settings.nav_arrows)
    }

    /// The menu with the route arrows on (their tile lit) or off.
    pub(crate) fn panel_of(&self, arrows: bool) -> Panel {
        let mut children: Vec<Element> = TILES
            .chunks(COLUMNS)
            .map(|line| {
                let tiles = line.iter().map(|(id, label, icon)| Element { grow: true, color: (*id == "arrows" && arrows).then_some(ACCENT), ..tile(id, label, icon) }).collect();
                el(None, Kind::Row { children: tiles, gap: 6.0, align: RowAlign::Start })
            })
            .collect();
        children.push(note(&tr("Alt or Esc closes the menu; the game goes on.")));
        Panel { padding: 10.0, gap: 6.0, ..card(Anchor::BottomRight, 32.0, 32.0, 560.0, 150, children) }
    }
}

/// Alt tapped: the menu opens, or closes when it is open.
pub(crate) fn toggle(app: &mut crate::App) {
    let gp = &mut app.menus.game_panels;
    gp.quick = match gp.quick.take() {
        Some(_) => None,
        None => Some(QuickMenu),
    };
}

/// A tile clicked: what it starts, the menu closing.
pub(crate) fn click(app: &mut crate::App, id: &str) {
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
        "world" => {
            let tab = crate::game_lists::tab_of(app, crate::game_lists::ListKind::World(0), "Time");
            app.open_list(crate::game_lists::ListKind::World(tab));
        }
        "stats" => match app.session.journey.as_ref().and_then(|j| j.last_summary()) {
            Some(s) => app.menus.game_panels.trip = Some(super::trip_summary::TripPanel::new(s)),
            None => app.open_list(crate::game_lists::ListKind::Drivers),
        },
        _ => {}
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
}
