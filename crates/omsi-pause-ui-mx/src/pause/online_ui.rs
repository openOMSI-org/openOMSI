//! Session snapshots and action routing for the independent multiplayer panels.
use super::*;
pub(crate) use omsi_multiplayer_ui::View;

fn with_panel(sh: &mut Shell, draw: impl FnOnce(&mut omsi_multiplayer_ui::Panel<'_>)) {
    let mut actions = Vec::new();
    let ui = &mut sh.ui;
    let state = &mut sh.state.pause_online;
    draw(&mut omsi_multiplayer_ui::Panel {
        ui,
        state,
        actions: &mut actions,
    });
    sh.actions
        .extend(actions.into_iter().map(|action| match action {
            omsi_multiplayer_ui::Action::Choose(index) => Action::Choose(index),
            omsi_multiplayer_ui::Action::OnlineAction(tab, command) => {
                Action::OnlineAction(tab, command)
            }
        }));
}

pub(super) fn page(sh: &mut Shell, v: &PauseView, area: Rect) {
    let room_actions: Vec<_> = v
        .rail
        .iter()
        .enumerate()
        .filter(|(_, line)| !v.disabled.contains(&line.0))
        .map(|(index, line)| (line.0, index))
        .collect();
    let view = omsi_multiplayer_ui::PageView {
        room_actions: &room_actions,
        online: &v.online,
    };
    with_panel(sh, |panel| omsi_multiplayer_ui::page(panel, &view, area));
}

pub(super) fn management_modal(sh: &mut Shell, v: &View) {
    with_panel(sh, |panel| omsi_multiplayer_ui::management_modal(panel, v));
}
pub(super) fn confirmation(sh: &mut Shell) {
    with_panel(sh, omsi_multiplayer_ui::confirmation);
}
