//! Shortcut edits change only saved presentation IDs, never the permission-filtered actions.
use super::*;
use crate::shell::pause::quick_actions::{self, Edit};

impl App {
    pub(crate) fn edit_quick_actions(&mut self, edit: Edit) {
        let menu = self.game_menu_items();
        // A stale or forged UI request must not add an action absent from this session.
        if let Edit::Toggle(id) = &edit {
            if !quick_actions::selected(&self.settings.pause_quick_actions).contains(id)
                && !menu.iter().any(|line| line.0 == id && line.2.is_none() && !matches!(line.0, "resume" | "quit" | "plugin")) { return; }
        }
        let old_entries = quick_actions::entries(&menu, &self.settings.pause_quick_actions);
        let focus = old_entries.get(self.shell.quick_focus).cloned();
        self.settings.pause_quick_actions = quick_actions::change(&self.settings.pause_quick_actions, &edit);
        crate::game_lists::remember_setting("pause_quick_actions", &self.settings.pause_quick_actions);
        crate::game_lists::flush_settings(true);
        let entries = quick_actions::entries(&menu, &self.settings.pause_quick_actions);
        self.shell.quick_focus = focus.and_then(|id| entries.iter().position(|e| *e == id)).unwrap_or(0);
        self.shell.last_sel = None;
    }

    pub(super) fn quick_editor_key(&mut self, code: KeyCode) -> bool {
        if self.vr_active() { return false; }
        if !self.shell.quick_edit { return false; }
        if self.input.keys.iter().any(|key| matches!(key, KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::AltLeft | KeyCode::AltRight)) { return true; }
        let entries = quick_actions::entries(&self.game_menu_items(), &self.settings.pause_quick_actions);
        let n = entries.len();
        self.shell.quick_focus = self.shell.quick_focus.min(n.saturating_sub(1));
        let at = self.shell.quick_focus;
        match code {
            KeyCode::Escape | KeyCode::KeyE => self.shell.quick_edit = false,
            KeyCode::ArrowUp | KeyCode::KeyW if n > 0 => self.shell.quick_focus = (at + n - 1) % n,
            KeyCode::ArrowDown | KeyCode::KeyS if n > 0 => self.shell.quick_focus = (at + 1) % n,
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if n > 0 => self.edit_quick_actions(Edit::Toggle(entries[at].clone())),
            KeyCode::ArrowLeft | KeyCode::KeyA if n > 0 => self.edit_quick_actions(Edit::Move(entries[at].clone(), false)),
            KeyCode::ArrowRight | KeyCode::KeyD if n > 0 => self.edit_quick_actions(Edit::Move(entries[at].clone(), true)),
            KeyCode::KeyR => self.edit_quick_actions(Edit::Reset),
            _ => {},
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_actions_editor_keys_do_not_execute_or_close_the_game_menu() {
        // A headless fixture avoids initializing Steam's process-global native client.
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi", "--server", "test-server.cfg"]);
        let mut app = crate::assemble_app(args, Default::default());
        app.open_game_menu();
        app.shell.quick_edit = true;
        assert!(app.quick_editor_key(KeyCode::ArrowDown));
        assert_eq!(app.shell.quick_focus, 1);
        assert!(app.quick_editor_key(KeyCode::ArrowUp));
        assert_eq!(app.shell.quick_focus, 0);
        assert!(app.quick_editor_key(KeyCode::Escape));
        assert!(!app.shell.quick_edit);
        assert!(app.menus.game_menu.is_some());
        assert!(app.shell.actions.is_empty());
    }
}
