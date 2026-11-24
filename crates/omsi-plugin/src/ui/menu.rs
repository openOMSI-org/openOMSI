use super::{MAX_CLICKS, UiState, check_id};

/// Most lines one plugin puts in the pause menu.
pub const MAX_MENU_ITEMS: usize = 3;
/// Longest label of such a line, in characters.
pub const MAX_MENU_LABEL: usize = 40;

/// A line a plugin put in the game's pause menu (`ui.set_menu_item`): choosing it closes the
/// menu and sends the plugin `menu_item`.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub owner: u64,
    pub id: String,
    pub label: String,
    /// A Material Symbols name of the game's icons (None: the game's own for plugins).
    pub icon: Option<String>,
}

impl UiState {
    /// Put a line in the pause menu, or change the plugin's line `id`.
    pub fn set_menu_item(
        &mut self,
        owner: u64,
        id: &str,
        label: &str,
        icon: Option<String>,
    ) -> Result<(), String> {
        check_id("the line's id", id)?;
        // (one line of text: a line break or another control character is a space)
        let label = label
            .split(|c: char| c.is_control() || c.is_whitespace())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let label = label.as_str();
        if label.is_empty() {
            return Err("the label is empty".into());
        }
        if label.chars().count() > MAX_MENU_LABEL {
            return Err(format!(
                "the label is longer than {MAX_MENU_LABEL} characters"
            ));
        }
        let item = MenuItem {
            owner,
            id: id.to_string(),
            label: label.to_string(),
            icon: icon.filter(|i| !i.trim().is_empty()),
        };
        match self
            .menu
            .iter()
            .position(|m| m.owner == owner && m.id == id)
        {
            Some(i) => self.menu[i] = item,
            None if self.menu.iter().filter(|m| m.owner == owner).count() >= MAX_MENU_ITEMS => {
                return Err(format!(
                    "a plugin has {MAX_MENU_ITEMS} lines in the menu at most"
                ));
            }
            None => self.menu.push(item),
        }
        Ok(())
    }

    /// Take a plugin's line out of the pause menu; whether it had one.
    pub fn remove_menu_item(&mut self, owner: u64, id: &str) -> bool {
        let n = self.menu.len();
        self.menu.retain(|m| !(m.owner == owner && m.id == id));
        self.menu.len() != n
    }

    /// Every plugin's lines in the pause menu.
    pub fn menu_items(&self) -> &[MenuItem] {
        &self.menu
    }

    /// The player chose a line of the pause menu: its plugin hears it in its next frame.
    pub fn pick_menu_item(&mut self, owner: u64, id: &str) {
        if self.menu_picks.len() >= MAX_CLICKS {
            self.menu_picks.remove(0);
        }
        self.menu_picks.push((owner, id.to_string()));
    }

    /// The lines of the pause menu chosen for a plugin.
    pub fn take_menu_picks(&mut self, owner: u64) -> Vec<String> {
        if self.menu_picks.is_empty() {
            return Vec::new();
        }
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.menu_picks)
            .into_iter()
            .partition(|m| m.0 == owner);
        self.menu_picks = rest;
        mine.into_iter().map(|m| m.1).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_lines_belong_to_their_plugin() {
        let mut ui = UiState::default();
        for i in 0..MAX_MENU_ITEMS {
            ui.set_menu_item(1, &format!("m{i}"), "A line", None)
                .unwrap();
        }
        assert!(
            ui.set_menu_item(1, "one too many", "A line", None)
                .unwrap_err()
                .contains("3 lines")
        );
        // changing one is no new line, another plugin has its own
        ui.set_menu_item(1, "m0", "  Re\nnamed\t ", Some(" ".into()))
            .unwrap();
        ui.set_menu_item(2, "m0", "Other", Some("map".into()))
            .unwrap();
        assert_eq!(
            ui.menu_items()[0],
            MenuItem {
                owner: 1,
                id: "m0".into(),
                label: "Re named".into(),
                icon: None
            }
        );
        assert!(
            ui.set_menu_item(2, "long", &"x".repeat(MAX_MENU_LABEL + 1), None)
                .is_err()
        );
        // a choice goes to its plugin only, once
        ui.pick_menu_item(2, "m0");
        assert!(ui.take_menu_picks(1).is_empty());
        assert_eq!(ui.take_menu_picks(2), ["m0"]);
        assert!(ui.take_menu_picks(2).is_empty());
        ui.remove_owner(1);
        assert_eq!(ui.menu_items().len(), 1);
    }
}
