//! Multiplayer controls use the same fields and cards as the other in-game forms.

use super::{button, card, divider, el, grow_row, input, note, row, text, theme_color};
use omsi_ui::theme::Palette;
use omsi_plugin::api::Value;
use omsi_plugin::ui::{Anchor, Element, Kind, Panel, RowAlign, Weight};

#[derive(Default)]
pub(crate) struct Multiplayer {
    pub(crate) target: String,
    pub(crate) said: Option<String>,
}

impl Multiplayer {
    pub(super) fn panel(&self, app: &crate::App, pal: Palette) -> Panel {
        let map = app
            .net
            .room
            .ready
            .as_ref()
            .and_then(|r| r.session.welcome.as_ref())
            .map(|w| {
                (
                    w.world.map.as_str(),
                    crate::app_impl::rooms::different_map(&app.args.map, &w.world.map),
                )
            });
        self.panel_of(
            app.net.room.busy(),
            map,
            app.net.lan.as_ref().map(|s| s.role),
            app.net
                .room
                .ready
                .as_ref()
                .and_then(|room| room.vehicle_allowed),
            pal,
        )
    }

    pub(super) fn panel_of(
        &self,
        busy: bool,
        map: Option<(&str, bool)>,
        role: Option<omsi_net::Role>,
        vehicle_allowed: Option<bool>,
        pal: Palette,
    ) -> Panel {
        let tr = omsi_ui::tr;
        let mut heading = text(&tr("Room settings"), 21.0, Weight::Bold);
        heading.color = Some(theme_color(pal.text));
        let description = if role.is_some() { "Manage your current room connection." } else { "Manage your current room or join a multiplayer game." };
        let mut children = vec![heading, note(&tr(description), pal), divider()];
        if role == Some(omsi_net::Role::Host) && (busy || map.is_some()) {
            children.push(note(
                &tr("Leaving or switching rooms closes your hosted room for all other players."),
                pal,
            ));
        }
        if busy {
            children.push(note(
                &tr(self
                    .said
                    .as_deref()
                    .unwrap_or("Connecting to the new room; the current game stays open")),
                pal,
            ));
        } else if let Some((map, different)) = map {
            children.push(note(&format!("{} {map}", tr("Room map:")), pal));
            children.push(note(&tr(if different {
                "This room uses a different map. Joining reloads the world and ends the current drive."
            } else {
                "Joining leaves the current room. Your vehicle and position are kept on this map."
            }), pal));
            if vehicle_allowed == Some(false) {
                children.push(note(&tr("This room does not allow your current vehicle. Joining may disconnect you."), pal));
            }
            children.push(grow_row(vec![
                button("confirm", &tr("Join this room"), Some("check")),
                button("back", &tr("Back"), Some("arrow_back")),
            ]));
        } else {
            if role.is_none() {
                children.push(section_heading(&tr("Join room"), pal));
                let mut entry = vec![input("code", &self.target, &tr("Connection code or server address"), 2048)];
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                entry.push(button("paste", &tr("Paste connection code"), Some("content_paste")));
                children.push(row(entry, RowAlign::Start));
                children.push(row(vec![button("join", &tr("Join room"), Some("login"))], RowAlign::End));
            } else {
                children.push(section_heading(&tr("Current room"), pal));
                if role == Some(omsi_net::Role::Host) {
                    children.push(note(&tr("Leaving closes your hosted room for all other players."), pal));
                }
                let mut actions = Vec::new();
                if role == Some(omsi_net::Role::Client) {
                    actions.push(button("reconnect", &tr("Reconnect to current room"), Some("refresh")));
                }
                actions.push(button("offline", &tr("Leave room and play offline"), Some("logout")));
                let mut actions = if role == Some(omsi_net::Role::Client) { grow_row(actions) } else { row(actions, RowAlign::End) };
                if let Kind::Row { gap, .. } = &mut actions.kind { *gap = 16.0; }
                children.push(actions);
            }
            if let Some(said) = &self.said {
                children.push(note(&tr(said), pal));
            }
        }
        children.push(el(None, Kind::Space { size: 6.0 }));
        children.push(divider());
        let hint_text = if role.is_some() { tr("Closing this window keeps your current room connected.") } else { "".into() };
        let mut hint = note(&hint_text, pal);
        hint.grow = true;
        children.push(row(vec![hint, button("close", &tr("Close"), Some("close"))], RowAlign::Between));
        style_buttons(&mut children, pal);
        let mut panel = card(Anchor::Center, 0.0, 0.0, 620.0, 255, pal, children);
        panel.background = Some(theme_color(pal.panel));
        panel.padding = 24.0;
        panel.gap = 14.0;
        panel.radius = 18.0;
        panel
    }
}

fn section_heading(label: &str, pal: Palette) -> Element {
    Element { color: Some(theme_color(pal.text_soft)), ..text(label, 14.0, Weight::Bold) }
}

fn style_buttons(children: &mut [Element], pal: Palette) {
    for child in children {
        if let Kind::Row { children, .. } = &mut child.kind {
            style_buttons(children, pal);
        } else if matches!(child.kind, Kind::Button { .. }) {
            if child.id.is_none() { continue; }
            child.color = Some(theme_color(match child.id.as_deref() {
                Some("offline") => pal.danger,
                Some("join" | "confirm" | "reconnect") => pal.accent,
                _ => pal.surface_2,
            }));
        }
    }
}

pub(super) fn change(app: &mut crate::App, id: &str, value: &Value) {
    if id == "code" && app.net.lan.is_none() && !app.net.room.busy() && app.net.room.ready.is_none() {
        if let (Some(form), Value::Str(value)) = (app.menus.game_panels.multiplayer.as_mut(), value)
        {
            form.target = value.clone();
        }
    }
}

pub(super) fn click(app: &mut crate::App, id: &str) {
    if id == "close" {
        app.net.room.cancel();
        app.menus.game_panels.multiplayer = None;
        return;
    }
    if app.net.room.busy() {
        return;
    }
    if app.net.lan.is_some() && matches!(id, "paste" | "join" | "code") {
        return;
    }
    match id {
        "paste" =>
        {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                Ok(text) => {
                    if let Some(form) = app.menus.game_panels.multiplayer.as_mut() {
                        form.target = text.trim().chars().take(2048).collect();
                    }
                }
                Err(_) => {
                    if let Some(form) = app.menus.game_panels.multiplayer.as_mut() {
                        form.said = Some("The clipboard could not be read".into());
                    }
                }
            }
        }
        "join" | "code" => {
            if let Some(target) = app
                .menus
                .game_panels
                .multiplayer
                .as_ref()
                .map(|f| f.target.clone())
            {
                app.connect_room(&target);
            }
        }
        "confirm" => app.net.room.accept = true,
        "back" => app.net.room.cancel(),
        "reconnect" => app.reconnect_room(),
        "offline" => app.net.room.leave = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn app() -> crate::App {
        crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default())
    }

    #[test]
    fn room_action_icons_have_visible_assets() {
        for name in ["content_paste", "login"] {
            let mask = omsi_ui::icons::rasterize(name, 24).expect("room action icon must exist");
            assert!(mask.iter().any(|alpha| *alpha > 0), "{name}");
        }
    }

    #[test]
    fn a_single_player_game_offers_the_multiplayer_menu() {
        let app = app();
        assert!(
            app.game_menu_items()
                .iter()
                .any(|line| line.0 == "multiplayer")
        );
    }

    #[test]
    fn the_code_field_uses_latest_edits_without_starting_a_connection() {
        let mut app = app();
        app.open_multiplayer();
        change(&mut app, "code", &Value::Str("new-room".into()));
        assert_eq!(
            app.menus.game_panels.multiplayer.as_ref().unwrap().target,
            "new-room"
        );
        assert!(!app.net.room.busy());
        click(&mut app, "close");
        assert!(app.menus.game_panels.multiplayer.is_none());
    }

    #[test]
    fn confirmation_does_not_replace_the_room_until_the_frame_boundary() {
        let mut app = app();
        app.open_multiplayer();
        click(&mut app, "confirm");
        assert!(app.net.room.accept);
        assert!(app.net.lan.is_none());
        click(&mut app, "back");
        assert!(!app.net.room.accept);
    }

    #[test]
    fn connected_room_hides_join_controls_and_restores_them_after_leaving() {
        let fonts = omsi_ui::Fonts::new();
        for role in [omsi_net::Role::Host, omsi_net::Role::Client] {
            let panel = Multiplayer::default().panel_of(false, None, Some(role), None, Palette::LIGHT);
            let laid = crate::plugin_ui::layout_typing(&panel, &fonts, 1.0, None);
            let shown: Vec<_> = laid.items.iter().filter_map(|item| match item {
                crate::plugin_ui::Item::Text { text, .. } => Some(text.as_str()), _ => None,
            }).collect();
            for label in ["Join room", "Paste connection code", "Connection code or server address"] {
                assert!(!shown.contains(&omsi_ui::tr(label).as_ref()), "{label}");
            }
            let ids: Vec<_> = laid.hits.iter().filter_map(|hit| hit.element.as_deref()).collect();
            for id in ["paste", "code", "join"] { assert!(!ids.contains(&id), "{id}"); }
            assert!(ids.contains(&"offline"));
            assert!(ids.contains(&"close"));
            assert_eq!(ids.contains(&"reconnect"), role == omsi_net::Role::Client);
        }
        let panel = Multiplayer::default().panel_of(false, None, None, None, Palette::LIGHT);
        let laid = crate::plugin_ui::layout_typing(&panel, &fonts, 1.0, None);
        for id in ["paste", "code", "join"] {
            assert!(laid.hits.iter().any(|hit| hit.element.as_deref() == Some(id)), "{id}");
        }
    }

    #[test]
    fn connected_room_rejects_stale_input_and_join_events() {
        let mut app = app();
        app.net.lan = Some(omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap());
        app.open_multiplayer();
        app.menus.game_panels.multiplayer.as_mut().unwrap().target = "original".into();
        change(&mut app, "code", &Value::Str("replacement".into()));
        for id in ["paste", "join", "code"] { click(&mut app, id); }
        assert_eq!(app.menus.game_panels.multiplayer.as_ref().unwrap().target, "original");
        assert!(app.menus.game_panels.multiplayer.as_ref().unwrap().said.is_none());
        assert!(!app.net.room.busy());
        assert!(!app.net.room.accept);
        click(&mut app, "offline");
        assert!(app.net.room.leave);
    }

    #[test]
    fn escape_cancels_the_form_and_pending_confirmation() {
        let mut app = app();
        app.open_multiplayer();
        app.net.room.accept = true;
        assert!(app.close_game_panel());
        assert!(app.menus.game_panels.multiplayer.is_none());
        assert!(!app.net.room.accept);
    }
}
