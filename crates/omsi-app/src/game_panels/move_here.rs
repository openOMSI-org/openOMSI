//! The map camera (F4): a click on the ground asks whether to move the bus there, and puts
//! it there - wherever that is, a car park, a yard, a field, not only on the nearest
//! street as Ctrl+click does - facing where the map camera looks, on the ground found there.

use super::{button, card, grow_row, note, title};
use omsi_plugin::ui::{Anchor, Panel};
use omsi_ui::theme::Palette;

pub(crate) fn panel(at: glam::DVec3, pal: Palette) -> Panel {
    card(
        Anchor::Center,
        0.0,
        0.0,
        360.0,
        236,
        pal,
        vec![
            title("Move the vehicle here?"),
            note(&format!("x {:.0}  y {:.0}", at.x, at.y), pal),
            grow_row(vec![button("yes", "Move here", Some("near_me")), button("no", "Cancel", Some("close"))]),
        ],
    )
}

/// A click of the map camera on the ground: the question.
pub(crate) fn ask(app: &mut crate::App, at: glam::DVec3) {
    app.menus.game_panels.move_to = Some(at);
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    let Some(at) = app.menus.game_panels.move_to.take() else { return };
    if id != "yes" {
        return;
    }
    let Some(heading) = app.camera.as_ref().map(|c| c.yaw as f64).or_else(|| app.player.as_ref().map(|p| p.vehicle.heading)) else { return };
    if crate::admin::request_teleport(app, at, heading) {
        app.service_msg = Some((omsi_ui::tr("The bus stands where the map was clicked").into_owned(), 3.0));
        app.service_event("teleport", "player", None);
    }
}
