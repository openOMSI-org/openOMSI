//! Multiplayer controls use the same fields and cards as the other in-game forms.

use super::{button, card, grow_row, input, note, title};
use omsi_plugin::api::Value;
use omsi_plugin::ui::{Anchor, Panel};

#[derive(Default)]
pub(crate) struct Multiplayer {
    pub(crate) target: String,
    pub(crate) said: Option<String>,
}

impl Multiplayer {
    pub(super) fn panel(&self, app: &crate::App, light: bool) -> Panel {
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
            light,
        )
    }

    pub(super) fn panel_of(
        &self,
        busy: bool,
        map: Option<(&str, bool)>,
        role: Option<omsi_net::Role>,
        vehicle_allowed: Option<bool>,
        light: bool,
    ) -> Panel {
        let tr = omsi_ui::tr;
        let mut heading = title(&tr("Multiplayer"));
        heading.color = Some(super::theme_color(super::theme(light).TEXT));
        let mut children = vec![heading];
        if role == Some(omsi_net::Role::Host) {
            children.push(note(
                &tr("Leaving or switching rooms closes your hosted room for all other players."),
                light,
            ));
        }
        if busy {
            children.push(note(
                &tr(self
                    .said
                    .as_deref()
                    .unwrap_or("Connecting to the new room; the current game stays open")),
                light,
            ));
        } else if let Some((map, different)) = map {
            children.push(note(&format!("{} {map}", tr("Room map:")), light));
            children.push(note(&tr(if different {
                "This room uses a different map. Joining reloads the world and ends the current drive."
            } else {
                "Joining leaves the current room. Your vehicle and position are kept on this map."
            }), light));
            if vehicle_allowed == Some(false) {
                children.push(note(&tr("This room does not allow your current vehicle. Joining may disconnect you."), light));
            }
            children.push(grow_row(vec![
                button("confirm", &tr("Join this room"), Some("check")),
                button("back", &tr("Back"), Some("arrow_back")),
            ]));
        } else {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            children.push(grow_row(vec![button(
                "paste",
                &tr("Paste connection code"),
                Some("content_paste"),
            )]));
            children.push(input(
                "code",
                &self.target,
                &tr("Connection code or server address"),
                2048,
            ));
            children.push(grow_row(vec![button(
                "join",
                &tr("Join another room"),
                Some("login"),
            )]));
            if role.is_some() {
                if role == Some(omsi_net::Role::Client) {
                    children.push(grow_row(vec![button(
                        "reconnect",
                        &tr("Reconnect to current room"),
                        Some("refresh"),
                    )]));
                }
                children.push(grow_row(vec![button(
                    "offline",
                    &tr("Leave room and play offline"),
                    Some("logout"),
                )]));
            }
            if let Some(said) = &self.said {
                children.push(note(&tr(said), light));
            }
        }
        children.push(grow_row(vec![button("close", &tr("Close"), Some("close"))]));
        card(Anchor::Center, 0.0, 0.0, 460.0, 245, light, children)
    }
}

pub(super) fn change(app: &mut crate::App, id: &str, value: &Value) {
    if id == "code" && !app.net.room.busy() && app.net.room.ready.is_none() {
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
    fn escape_cancels_the_form_and_pending_confirmation() {
        let mut app = app();
        app.open_multiplayer();
        app.net.room.accept = true;
        assert!(app.close_game_panel());
        assert!(app.menus.game_panels.multiplayer.is_none());
        assert!(!app.net.room.accept);
    }
}
