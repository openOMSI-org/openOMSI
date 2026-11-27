//! App adapters and compatibility re-exports for the pause UI crate.

#[cfg(test)]
#[path = "pause_legacy_tests.rs"]
mod legacy;

#[cfg(test)]
pub(crate) use legacy::*;

#[cfg(not(test))]
pub(crate) use omsi_pause_ui::Summary;

#[cfg(not(test))]
pub(crate) use omsi_pause_ui::pause::{navigation, quick_actions};

#[cfg(not(test))]
pub(crate) mod search {
    use crate::shell::Shell;

    pub(crate) type State = omsi_settings_ui::SearchState;

    pub(crate) fn focused(shell: &Shell) -> bool {
        shell.ui.focus.is_some_and(|id| {
            id == omsi_ui::kit::id_of("pause-settings-search")
                || id == omsi_ui::kit::id_of("pause-online-search")
        })
    }
}

#[cfg(not(test))]
pub(crate) mod online_ui {
    pub(crate) use omsi_multiplayer_ui::View;

    pub(crate) fn from_app(app: &crate::App) -> View {
        crate::app_impl::pause_online::from_app(app)
    }
}
