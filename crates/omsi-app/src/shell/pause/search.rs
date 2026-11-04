//! Presentation-only filtering; actions always retain their original list indices.
use super::*;

#[derive(Default)]
pub(crate) struct State {
    pub page: String,
    pub query: String,
    pub visible: Vec<usize>,
}

pub(super) fn filter_groups(items: &[(&str, &str)], hide_back: bool, query: &str) -> Vec<Vec<usize>> {
    let query = query.trim().to_lowercase();
    let mut groups = settings_groups(items, hide_back);
    if query.is_empty() { return groups; }
    for group in &mut groups {
        group.retain(|&k| {
            if items[k].0 == crate::game_lists::HEADING { return true; }
            let row = parse_row(items[k].1);
            omsi_ui::kit::matches(row.name, &query) || omsi_ui::kit::matches(row.desc, &query)
        });
    }
    groups.retain(|g| g.iter().any(|&k| items[k].0 != crate::game_lists::HEADING));
    groups
}

pub(super) fn toolbar_layout(area: Rect, body: Rect) -> (Rect, f32) {
    if area.w >= 760.0 {
        let width = (area.w * 0.34).clamp(220.0, 360.0);
        (Rect::new(area.right() - width - 44.0, area.y + 4.0, width, 36.0), 0.0)
    } else {
        (Rect::new(body.x, body.y, (body.w - 44.0).max(40.0), 36.0), 48.0)
    }
}

pub(super) fn toolbar(sh: &mut Shell, list: &ListView, area: Rect, body: &mut Rect) {
    if sh.settings_search.page != list.key {
        sh.settings_search = State { page: list.key.clone(), ..Default::default() };
        sh.ui.focus = None;
    }
    let (field, body_offset) = toolbar_layout(area, *body);
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
        if !focused(&self.shell) { crate::platform::text_input(self.window.as_deref(), false); }
        true
    }
}
