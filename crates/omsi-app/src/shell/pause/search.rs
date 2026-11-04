//! Search input integration for the pause settings pages.
use super::*;
pub(crate) type State = omsi_settings_ui::SearchState;

pub(super) fn toolbar(sh: &mut Shell, list: &ListView, area: Rect, body: &mut Rect) {
    if sh.settings_search.page != list.key {
        sh.settings_search = State { page: list.key.clone(), ..Default::default() };
        sh.ui.focus = None;
    }
    let (field, body_offset) = omsi_settings_ui::toolbar_layout(area, *body);
    let changed = sh.ui.text_input("pause-settings-search", field, &mut sh.settings_search.query, &omsi_ui::tr("Search settings"), Some("search"));
    let clear = sh.ui.icon_button("pause-settings-search-clear", Vec2::new(field.right() + 22.0, field.center().y), 18.0, "close", "Clear search");
    if clear { sh.settings_search.query.clear(); sh.ui.focus = None; }
    if changed || clear {
        let id = id_of(&format!("pause-list-{}", list.key));
        sh.ui.scroll.insert(id, 0.0);
        sh.ui.scroll.insert(id ^ 0xabc, 0.0);
        sh.last_sel = None;
    }
    *body = Rect::new(body.x, body.y + body_offset, body.w, (body.h - body_offset).max(0.0));
}

pub(crate) fn focused(sh: &Shell) -> bool {
    sh.ui.focus == Some(id_of("pause-settings-search"))
}

pub(crate) fn key(sh: &mut Shell, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
    use omsi_ui::kit::Key;
    use winit::keyboard::KeyCode as K;
    if !focused(sh) { return false; }
    let key = match code {
        K::ArrowLeft => Some(Key::Left), K::ArrowRight => Some(Key::Right),
        K::Home => Some(Key::Home), K::End => Some(Key::End),
        K::Backspace => Some(Key::Backspace), K::Delete => Some(Key::Delete),
        K::Enter | K::NumpadEnter | K::Tab | K::Escape => { sh.ui.focus = None; return true; }
        K::KeyA if sh.ui.input.ctrl => Some(Key::SelectAll),
        _ => None,
    };
    if let Some(key) = key { sh.ui.input.keys.push(key); }
    else if !sh.ui.input.ctrl && !sh.ui.input.alt {
        if let Some(text) = text { sh.ui.input.text.extend(text.chars().filter(|c| !c.is_control())); }
    }
    true
}

impl crate::app::App {
    pub(crate) fn settings_search_key(&mut self, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
        use winit::keyboard::KeyCode as K;
        if self.menus.game_menu.is_none() || self.menus.chooser.is_none() || !focused(&self.shell) { return false; }
        self.shell.ui.input.shift = self.input.keys.contains(&K::ShiftLeft) || self.input.keys.contains(&K::ShiftRight);
        self.shell.ui.input.ctrl = self.input.keys.contains(&K::ControlLeft) || self.input.keys.contains(&K::ControlRight);
        self.shell.ui.input.alt = self.input.keys.contains(&K::AltLeft) || self.input.keys.contains(&K::AltRight);
        key(&mut self.shell, code, text);
        if !focused(&self.shell) { crate::platform::phone_keyboard(self.window.as_deref(), false); }
        true
    }
}
