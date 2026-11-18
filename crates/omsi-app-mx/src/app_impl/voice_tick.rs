//! The voice chat of a LAN session, once a frame, and the talk keys.

use super::*;

impl App {
    /// The voice chat (`voice_chat`), once a frame: alive while the settings have it on (the
    /// game that hosts or joins a session, not a dedicated server, which only relays), its
    /// keys, packets and sound handled there.
    pub(super) fn tick_voice(&mut self, dt: f32) {
        let wanted = self.settings.voice_enabled && self.args.server.is_none() && self.net.lan.is_some();
        // a game that hosts relays voice by its own range, whether it talks itself or not
        if self.args.server.is_none() {
            let range = self.settings.voice_range;
            if let Some(l) = self.net.lan.as_mut().filter(|l| l.role == omsi_net::Role::Host) {
                let p = l.voice_policy();
                if (p.range - range).abs() > 0.01 {
                    l.set_voice_policy(p.with_range(range));
                }
            }
        }
        if !wanted {
            // (what comes in is of no use to a game that does not listen: not kept)
            if let Some(l) = self.net.lan.as_mut() {
                l.take_voice();
            }
            if let Some(mut v) = self.sound.voice.take() {
                v.stop(self.sound.audio.as_ref());
            }
            return;
        }
        if self.sound.voice.is_none() {
            // (the first time on this machine: the system asks for the microphone now)
            crate::platform::request_microphone();
            let first = voice_first_time();
            self.sound.voice = Some(crate::voice_chat::VoiceChat::new(first));
        }
        let held = (self.voice_key_held("voice_nearby"), self.voice_key_held("voice_radio"));
        let my_bus = self.player.as_ref().map(|p| p.vehicle.position);
        let listener = self.camera.as_ref().map(|c| c.position);
        let (Some(lan), Some(chat)) = (self.net.lan.as_mut(), self.sound.voice.as_mut()) else { return };
        let my_inside = if self.cam.in_cab { Some(lan.my_id) } else { self.net.inside_remote };
        chat.tick(dt, &self.settings, lan, &self.net.remotes, self.sound.audio.as_ref(), listener, my_bus, my_inside, held, &self.sound.voice_mutes);
    }

    /// Is the bindable talk key `action` (`voice_nearby`, `voice_radio` in Controls) held right
    /// now? A keyboard chord, a controller button bound to the same action, or the touch
    /// screen's button (held while down).
    pub(super) fn voice_key_held(&self, action: &str) -> bool {
        let radio = action.eq_ignore_ascii_case("voice_radio");
        if if radio { self.input.pad_voice_radio } else { self.input.pad_voice_nearby } {
            return true;
        }
        if self.input.touch.voice_held[radio as usize] {
            return true;
        }
        // (a letter typed in the chat or a menu is not a talk key)
        if self.net.remotes.chat.typing.is_some() || self.menus.game_menu.is_some() || self.menus.chooser.is_some() {
            return false;
        }
        let held = |a: KeyCode, b: KeyCode| self.input.keys.contains(&a) || self.input.keys.contains(&b);
        let chord = omsi_content::input::chord(
            held(KeyCode::ShiftLeft, KeyCode::ShiftRight),
            held(KeyCode::ControlLeft, KeyCode::ControlRight),
            held(KeyCode::AltLeft, KeyCode::AltRight),
        );
        self.input.game_keys.iter().any(|b| {
            b.action.eq_ignore_ascii_case(action)
                && b.scan_code != 0
                && b.matches(chord)
                && self.input.keys.iter().any(|k| crate::keys::dik_code(*k) == Some(b.scan_code))
        })
    }

    /// The talk key being down: the other players' name tags show the radio from it.
    pub(super) fn voice_radio_keyed(&self) -> bool {
        self.sound.voice.as_ref().is_some_and(|v| v.transmitting() == Some(omsi_voice::Channel::Radio))
    }
}

/// True once per machine: the marker beside the settings is written, so the system's
/// microphone question comes when voice chat is first switched on, not again.
fn voice_first_time() -> bool {
    let Some(dir) = crate::settings::Settings::path().and_then(|p| p.parent().map(|d| d.to_path_buf())) else { return false };
    let marker = dir.join("voice-microphone-asked");
    if marker.exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&marker, "The system was asked for the microphone when voice chat was first switched on.\n");
    true
}
