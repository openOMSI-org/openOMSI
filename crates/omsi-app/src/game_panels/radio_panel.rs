use super::{button, card, grow_row, note, text, title};
use omsi_plugin::ui::{Anchor, Panel, Weight};
use omsi_ui::theme::Palette;
use omsi_ui::tr;

#[derive(Default)]
pub(crate) struct RadioPanel {
    said: Option<String>,
}

impl RadioPanel {
    pub(crate) fn panel(&self, app: &crate::App, pal: Palette) -> Panel {
        let radio = &app.sound.radio;
        let (station, song) = radio.now_playing();
        let playing = if station.is_empty() { tr("The radio is off").into_owned() } else if song.is_empty() { station } else { format!("{station} - {song}") };
        let mut children = vec![
            title(&tr("Radio")),
            text(&playing, 14.0, Weight::Medium),
            note(&tr("Volume {}").replace("{}", &format!("{:.0} %", radio.volume() * 100.0)), pal),
            grow_row(vec![
                button("power", &tr("On / off"), Some("power_settings_new")),
                button("previous", &tr("Previous"), Some("chevron_left")),
                button("next", &tr("Next"), Some("chevron_right")),
            ]),
            grow_row(vec![
                button("down", &tr("Quieter"), Some("remove")),
                button("up", &tr("Louder"), Some("add")),
                button("close", &tr("Close"), Some("close")),
            ]),
        ];
        if let Some(said) = &self.said {
            children.push(note(said, pal));
        }
        card(Anchor::Center, 0.0, 0.0, 420.0, 236, pal, children)
    }
}

pub(crate) fn open(app: &mut crate::App) {
    app.menus.game_panels.radio = Some(RadioPanel::default());
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    let msg = match id {
        "power" => app.sound.radio.toggle_power(app.player.as_ref().map(|p| &p.vehicle)),
        "previous" => app.sound.radio.previous_station(),
        "next" => app.sound.radio.next_station(),
        "up" | "down" => {
            app.sound.radio.step_volume(id == "up");
            if let Some(r) = app.menus.game_panels.radio.as_mut() {
                r.said = None;
            }
            return;
        }
        "close" => {
            app.menus.game_panels.radio = None;
            return;
        }
        _ => return,
    };
    if let Some(r) = app.menus.game_panels.radio.as_mut() {
        r.said = Some(msg);
    }
}
