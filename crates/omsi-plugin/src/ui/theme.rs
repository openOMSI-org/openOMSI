//! The interface themes the game has (`ui.themes`, `ui.theme`), and a plugin's wish to use one
//! (`ui.set_theme`): the game tells the plugins its themes before it loads them and each frame,
//! and takes a wish after their frame (it becomes the player's setting, as if chosen in
//! Options › Interface).

use super::UiState;
use std::cell::RefCell;

thread_local! {
    /// The game's themes and the one in use, for the plugins it loads next: their main chunk
    /// already sees them (`ui.themes`, `ui.theme`, `ui.set_theme`), before the first frame.
    static FOR_LOAD: RefCell<(Vec<ThemeInfo>, String)> = const { RefCell::new((Vec::new(), String::new())) };
}

/// The game's themes and the one in use ("" openOMSI's own look), told before it loads the
/// plugins on this thread.
pub fn themes_for_load(themes: Vec<ThemeInfo>, now: &str) {
    FOR_LOAD.with(|f| *f.borrow_mut() = (themes, now.to_string()));
}

/// What the game told before it loads the plugins (none: no themes yet).
pub(super) fn for_load() -> (Vec<ThemeInfo>, String) {
    FOR_LOAD.with(|f| f.borrow().clone())
}

/// One of the game's interface themes: its id (the setting's value; "" openOMSI's own look)
/// and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
}

impl UiState {
    /// The game's themes and the one in use (`ui.themes`, `ui.theme`).
    pub fn set_themes(&mut self, themes: Vec<ThemeInfo>, now: &str) {
        self.themes = themes;
        if self.theme_now != now {
            self.theme_now = now.to_string();
        }
    }

    pub fn themes(&self) -> &[ThemeInfo] {
        &self.themes
    }

    /// The id of the theme in use ("" openOMSI's own look).
    pub fn theme(&self) -> &str {
        &self.theme_now
    }

    /// A plugin asks for theme `id` ("" or "openomsi": openOMSI's own look). Refused when the
    /// game has no theme of that id.
    pub fn ask_theme(&mut self, id: &str) -> Result<(), String> {
        let id = id.trim();
        let id = if id.eq_ignore_ascii_case("openomsi") { "" } else { id };
        let Some(t) = self.themes.iter().find(|t| t.id.eq_ignore_ascii_case(id)) else {
            return Err(format!("no theme \"{id}\" (ui.themes() lists them)"));
        };
        self.theme_ask = Some(t.id.clone());
        Ok(())
    }

    /// The theme a plugin asked for since the game last looked.
    pub fn take_theme_ask(&mut self) -> Option<String> {
        self.theme_ask.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_asks_for_a_theme_the_game_has() {
        let mut ui = UiState::default();
        assert!(ui.ask_theme("omsihub").is_err(), "the game told of none yet");
        let themes = vec![ThemeInfo { id: String::new(), name: "openOMSI".into() }, ThemeInfo { id: "OmsiHub".into(), name: "Omsi-Hub".into() }];
        ui.set_themes(themes.clone(), "");
        assert_eq!((ui.themes(), ui.theme()), (&themes[..], ""));
        // (any case; the game's spelling is kept)
        ui.ask_theme(" omsihub ").unwrap();
        assert_eq!(ui.take_theme_ask().as_deref(), Some("OmsiHub"));
        assert_eq!(ui.take_theme_ask(), None);
        // (openOMSI's own look, by either name)
        ui.ask_theme("openOMSI").unwrap();
        assert_eq!(ui.take_theme_ask().as_deref(), Some(""));
        assert!(ui.ask_theme("sparkle").unwrap_err().contains("sparkle"));
        assert_eq!(ui.take_theme_ask(), None);
    }

    /// A plugin's main chunk, run as the game loads it, already knows the themes.
    #[test]
    fn the_themes_are_known_as_the_plugins_load() {
        let themes = vec![ThemeInfo { id: String::new(), name: "openOMSI".into() }, ThemeInfo { id: "omsihub".into(), name: "Omsi-Hub".into() }];
        themes_for_load(themes.clone(), "");
        let mut ui = UiState::default();
        assert_eq!((ui.themes(), ui.theme()), (&themes[..], ""));
        ui.ask_theme("omsihub").unwrap();
        assert_eq!(ui.take_theme_ask().as_deref(), Some("omsihub"));
        themes_for_load(Vec::new(), "");
    }
}
