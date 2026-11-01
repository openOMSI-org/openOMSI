//! The navigator's board as the plugins see it (`ui.dock`): where it is and whose panel is in it,
//! as the game tells it every frame.

use super::UiState;

/// The navigator's board as the plugins see it (`ui.dock`), in the panel's pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dock {
    pub width: f32,
    /// The most height there is room for.
    pub room: f32,
    /// The height the panel in it came to (more than `room`: it is cut off there; 0 before
    /// it is laid out).
    pub height: f32,
    /// The plugin whose panel is in it.
    pub owner: Option<u64>,
    /// It is the navigator's whole frame (a panel with `frame = true` is in it), not the board
    /// under its map.
    pub frame: bool,
}

impl UiState {
    /// The game says where the navigator's board is and whose panel is in it, or that it
    /// shows none.
    pub fn set_dock(&mut self, dock: Option<Dock>) {
        self.dock = dock;
    }

    /// `omsi.ui.dock()`: the navigator's board, None while it shows none.
    pub fn dock(&self) -> Option<Dock> {
        self.dock
    }
}
