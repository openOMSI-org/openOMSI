//! The map camera (F4): a click on the ground asks whether to move the bus there, and puts
//! it there - wherever that is, a car park, a yard, a field, not only on the nearest
//! street as Ctrl+click does - facing where the camera looks, on the ground found there.

use super::{button, card, grow_row, note, title};
use omsi_plugin::ui::{Anchor, Panel};

pub(crate) fn panel(at: glam::DVec3, light: bool) -> Panel {
    card(
        Anchor::Center,
        0.0,
        0.0,
        360.0,
        236,
        light,
        vec![
            title("Move the vehicle here?"),
            note(&format!("x {:.0}  y {:.0}", at.x, at.y), light),
            grow_row(vec![button("yes", "Move here", Some("near_me")), button("no", "Cancel", Some("close"))]),
        ],
    )
}

/// A click of the map camera on the ground: the question.
pub(crate) fn ask(app: &mut crate::App, at: glam::DVec3) {
    app.menus.game_panels.move_to = (app.player.is_some()
        && !app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client))
        .then_some(at);
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    let Some(at) = app.menus.game_panels.move_to.take() else { return };
    if id != "yes" || app.player.is_none() {
        return;
    }
    if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
        app.service_msg = Some(("In a LAN session only the host moves vehicles on the map".into(), 4.0));
        return;
    }
    // facing where the map camera looks (its yaw is a heading), else the way it faced
    let Some(heading) = app.camera.as_ref().map(|c| c.yaw as f64).or_else(|| app.player.as_ref().map(|p| p.vehicle.heading)) else { return };
    crate::admin::teleport(app, at, heading);
    app.service_msg = Some(("The bus stands where the map was clicked".into(), 3.0));
    app.service_event("teleport", "player", None);
}

#[cfg(test)]
mod tests;
