//! The Administration menu of a game that administers a dedicated server from afar (see
//! `omsi_lan::admin::panel`): its pages in the game menu's lists, the server asked at the menu's
//! pace while it is open, and its answers shown as they come - the line picked kept by what it
//! does, not by where it was (a player's line follows the player, whatever the list does).

use crate::game_lists::ListKind;
use crate::App;
use omsi_lan::admin::panel::{AdminMenu, Panel};
use omsi_lan::commands::Command;
use std::time::Instant;

/// This game administers a dedicated server (not its own session): the server's menu.
pub(crate) fn remote(app: &App) -> bool {
    app.net.is_admin && app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client)
}

fn admin_kind(k: Option<&ListKind>) -> bool {
    matches!(k, Some(ListKind::Admin | ListKind::AdminPlayers | ListKind::AdminPlayer(_) | ListKind::AdminConfirm(..)))
}

/// The lines of a page of the server's menu.
pub(crate) fn items(app: &App, kind: &ListKind) -> Vec<(String, String)> {
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    let p = &app.net.admin_panel;
    match kind {
        ListKind::Admin => p.main_lines(Instant::now(), &tr),
        ListKind::AdminPlayers => p.player_lines(&tr),
        ListKind::AdminPlayer(t) => p.one_player_lines(*t, &tr),
        ListKind::AdminConfirm(verb, t) => p.confirm_lines(verb, *t, &tr),
        _ => Vec::new(),
    }
}

/// A line of the server's menu picked: the page to show next (None: back to the game menu).
pub(crate) fn run(app: &mut App, kind: &ListKind, action: &str) -> Option<ListKind> {
    match AdminMenu::parse(action) {
        AdminMenu::Send(line) => {
            if let Some(l) = app.net.lan.as_mut() {
                l.command(1, &format!("admin {line}"));
            }
            app.net.admin_panel.soon();
            app.service_msg = Some((format!("{}: {line}", omsi_ui::tr("Sent to the server")), 2.0));
            // (after sending a player away: back to the players, where the answer shows)
            Some(if matches!(kind, ListKind::AdminConfirm(..)) { ListKind::AdminPlayers } else { kind.clone() })
        }
        AdminMenu::Players => Some(ListKind::AdminPlayers),
        AdminMenu::Player(t) => Some(ListKind::AdminPlayer(t)),
        AdminMenu::Confirm(verb, t) => Some(ListKind::AdminConfirm(verb, t)),
        AdminMenu::Classic => Some(ListKind::AdminClassic),
        AdminMenu::Logout => {
            if let Some(l) = app.net.lan.as_mut() {
                l.command(1, "admin-logout");
            }
            app.net.is_admin = false;
            app.net.admin_panel = Panel::default();
            None
        }
        AdminMenu::Nothing => Some(kind.clone()),
    }
}

/// Each frame: while one of the server menu's pages is open, ask what it shows (the panel
/// keeps the pace: every 3 s at most, one question at a time).
pub(crate) fn tick(app: &mut App) {
    if !remote(app) || !admin_kind(app.menus.list_kind.as_ref()) {
        return;
    }
    let now = Instant::now();
    let was_silent = app.net.admin_panel.silent(now);
    let asks = app.net.admin_panel.due(now);
    if let Some(l) = app.net.lan.as_mut() {
        for a in asks {
            l.command(1, a);
        }
    }
    // (the server stopped answering: the menu says so)
    if app.net.admin_panel.silent(now) != was_silent {
        refresh_keeping(app);
    }
}

/// A server's answer for its admin: kept for the menu, the open page shown again with it, and
/// what became of an operation said at once wherever the player is.
pub(crate) fn take(app: &mut App, c: &Command) {
    if let Command::AdminResult { status, message, .. } = c {
        log::info!("LAN: the server: {} {message}", status.name());
        app.service_msg = Some((format!("{}: {message}", omsi_ui::tr(status.label())), 4.0));
    }
    if app.net.admin_panel.take(c) && admin_kind(app.menus.list_kind.as_ref()) {
        refresh_keeping(app);
    }
}

/// The open page made again, on the line that does what the chosen one did.
fn refresh_keeping(app: &mut App) {
    let chosen = app.menus.chooser.and_then(|k| app.menus.admin_list.as_ref()?.get(k).map(|l| l.1.clone()));
    app.refresh_list();
    if let (Some(action), Some(list)) = (chosen, app.menus.admin_list.as_ref()) {
        match list.iter().position(|l| l.1 == action) {
            Some(k) => app.menus.chooser = Some(k),
            // (a player who left: not the next one in their place)
            None if action.starts_with("pick ") => app.menus.chooser = Some(0),
            None => {}
        }
    }
}
