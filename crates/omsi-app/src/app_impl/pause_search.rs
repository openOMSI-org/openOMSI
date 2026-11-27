//! App-owned keyboard integration for pause settings search.

use super::*;

impl App {
    pub(crate) fn settings_search_key(
        &mut self,
        code: winit::keyboard::KeyCode,
        text: Option<&str>,
    ) -> bool {
        use winit::keyboard::KeyCode as K;
        if self.menus.game_menu.is_none() || !crate::shell::pause::search::focused(&self.shell) {
            return false;
        }
        self.shell.ui.input.shift =
            self.input.keys.contains(&K::ShiftLeft) || self.input.keys.contains(&K::ShiftRight);
        self.shell.ui.input.ctrl =
            self.input.keys.contains(&K::ControlLeft) || self.input.keys.contains(&K::ControlRight);
        self.shell.ui.input.alt =
            self.input.keys.contains(&K::AltLeft) || self.input.keys.contains(&K::AltRight);
        let field = if self.shell.ui.focus == Some(omsi_ui::kit::id_of("pause-online-search")) {
            "pause-online-search"
        } else {
            "pause-settings-search"
        };
        omsi_settings_ui::search_key(&mut self.shell.ui, field, code, text);
        if !crate::shell::pause::search::focused(&self.shell) {
            crate::platform::phone_keyboard(self.window.as_deref(), false);
        }
        true
    }
}
