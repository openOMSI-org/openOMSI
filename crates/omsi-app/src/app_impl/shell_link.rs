//! The game's side of the screens drawn with the launcher's toolkit (`shell`): what the
//! pause menu shows of the game, and what its clicks do.

use super::*;
use crate::game_lists::ListKind;
use crate::shell::pause::{ListView, PauseView};
use crate::shell::Action;

/// The line of the pause menu's rail that list `kind` was opened from (it stays lit while
/// the list is open, as the launcher's page in its rail).
pub(crate) fn rail_of(kind: &ListKind, camera_tab: bool) -> Option<&'static str> {
    Some(match kind {
        ListKind::Options(_) if camera_tab => "camera",
        ListKind::Options(_) => "options",
        ListKind::Controls
        | ListKind::Keyboard(_)
        | ListKind::ControllerDevices(_)
        | ListKind::Controller(..)
        | ListKind::ControllerAxis(..)
        | ListKind::ControllerButtons(_)
        | ListKind::ControllerButtonSettings(..)
        | ListKind::ControllerButton(..)
        | ListKind::ControllerCapture(_)
        | ListKind::Events => "controls",
        ListKind::World(_) => "world",
        ListKind::SaveSlots => "saveslot",
        ListKind::PlayerList | ListKind::PlayerPage(..) | ListKind::PlayerDo(..) | ListKind::BanList => "players",
        ListKind::Lines | ListKind::Tours(..) => "duty",
        ListKind::Admin | ListKind::AdminPlayers | ListKind::AdminPlayer(_) | ListKind::AdminConfirm(..) | ListKind::AdminClassic => "admin",
        _ => "vehicle",
    })
}

impl App {
    /// The screens of the launcher's toolkit take the mouse: the pause menu is open (not in
    /// a headset, whose menu is a card in the scene), or the photo mode is on.
    pub(crate) fn shell_takes_mouse(&self) -> bool {
        self.has_process_permission_prompt() || (!self.vr_active() && self.menus.game_menu.is_some()) || self.photo.is_some()
    }

    /// The pause menu drawn with the launcher's toolkit, over the picture (nothing when it is
    /// closed or a headset shows its own).
    pub(crate) fn frame_shell(&mut self, dt: f32) {
        if self.frame_process_permission(dt) { return; }
        let Some(s) = self.gfx.surface.as_ref() else {
            self.shell.clear();
            return;
        };
        // (the photo mode draws its own panel, `photo::panel`)
        if self.photo.is_some() {
            return;
        }
        // the developer tools, while the pause menu is closed
        if self.devtools.is_some() && self.menus.game_menu.is_none() {
            self.frame_devtools(dt);
            return;
        }
        if !self.shell_takes_mouse() {
            if self.shell.showing() {
                self.shell.clear();
            }
            return;
        }
        let (w, h) = (s.config.width as f32, s.config.height as f32);
        let dpi = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
        let scale = dpi * ui::size_factor(h, dpi, self.settings.ui_scale, self.settings.ui_scale_window);
        self.fix_pause_focus();
        let rail = self.game_menu_items();
        let tabs = match self.menus.list_kind.as_ref() {
            Some(k) if self.menus.chooser.is_some() => crate::game_lists::page_titles(self, k),
            _ => None,
        };
        let chooser_list = self.menus.admin_list.as_ref().unwrap_or(&self.menus.vehicle_list);
        let (kind, head, preview) = crate::game_lists::menu_extras(self.menus.list_kind.as_ref(), self.menus.admin_list.as_deref(), self.menus.chooser, crate::duty_book::timetable(self), self.clock.time);
        let camera_tab = matches!(self.menus.list_kind, Some(ListKind::Options(t)) if Some(t) == tabs.as_ref().and_then(|(titles, _)| titles.iter().position(|x| x == omsi_ui::tr("Camera").as_ref())));
        let list = self.menus.chooser.map(|sel| ListView {
            kind,
            head,
            tabs: tabs.clone(),
            items: chooser_list.iter().map(|(name, action)| (action.as_str(), name.as_str())).collect(),
            sel,
            preview: preview.as_ref(),
            pane_first: self.menus.pane_scroll.filter(|p| Some(p.0) == self.menus.chooser).map(|p| p.1),
            dropdown: self.menus.dropdown.as_ref().map(|d| ui::DropdownView { row: d.row, items: d.items.iter().map(|x| x.0.as_str()).collect(), sel: d.sel, top: d.top, current: d.current }),
            key: format!("{:?}", self.menus.list_kind).chars().filter(|c| c.is_alphanumeric()).collect(),
        });
        let view = PauseView {
            paused: self.paused,
            quick_actions: &self.settings.pause_quick_actions,
            rail: &rail,
            rail_sel: self.menus.game_menu,
            disabled: self.menu_disabled_ids(),
            rail_open: self.menus.list_kind.as_ref().and_then(|k| rail_of(k, camera_tab)).filter(|_| self.menus.chooser.is_some()),
            list,
            kbd: self.menus.menu_kbd,
            keys: !crate::platform::touch_controls(),
            summary: self.pause_summary(),
        };
        self.shell.begin(w, h, scale, dt);
        crate::shell::pause::draw(&mut self.shell, &view);
        self.shell.finish();
    }

    /// What the pause menu's sidebar shows of the session: the bus and the driver.
    fn pause_summary(&self) -> crate::shell::pause::Summary {
        let bus = self.player.as_ref().map(|p| format!("{} {}", p.vehicle.ty.def.manufacturer.trim(), p.vehicle.ty.def.type_name.trim()).trim().to_string()).unwrap_or_default();
        crate::shell::pause::Summary { bus, driver: self.args.lan_name.trim().to_string() }
    }

    /// What the clicks on the screens asked for, done (at the start of a frame, with the
    /// event loop at hand).
    pub(crate) fn apply_shell_actions(&mut self, event_loop: &ActiveEventLoop) {
        let actions = std::mem::take(&mut self.shell.actions);
        for a in actions {
            log::info!("screens: {a:?}");
            // (the mouse has taken over from the keyboard)
            self.menus.menu_kbd = false;
            match a {
                Action::ProcessPermission(id, decision) => { self.answer_process_permission(id, decision); }
                Action::Choose(k) => {
                    if self.menus.game_menu.is_none() {
                        continue;
                    }
                    if self.menus.chooser.is_some() {
                        self.menus.chooser = Some(k);
                        // (a click on a tour shows its stops: the trip starts with the button)
                        if matches!(self.menus.list_kind, Some(ListKind::Tours(..))) && crate::game_lists::tour_at(self, k).is_some() {
                            if let Some(ListKind::Tours(line, _)) = self.menus.list_kind.clone() {
                                self.menus.list_kind = Some(ListKind::Tours(line, None));
                            }
                            continue;
                        }
                    } else {
                        self.menus.game_menu = Some(k);
                    }
                    self.menu_choose(event_loop, k);
                }
                Action::PauseCategory(category) => {
                    if self.menus.game_menu.is_some() {
                        self.switch_pause_category(category);
                    }
                }
                Action::Rail(k) => {
                    self.close_list();
                    if k != usize::MAX && self.menus.game_menu.is_some() {
                        self.menus.game_menu = Some(k);
                        self.menu_choose(event_loop, k);
                    }
                }
                Action::QuickActions(edit) => self.edit_quick_actions(edit),
                Action::Control(k, fx) => {
                    if self.menus.chooser.is_some() {
                        self.menus.chooser = Some(k);
                        self.list_click(k, fx);
                    }
                }
                Action::Side(i) => self.settings_side_click(i),
                Action::Pane(i) => self.tour_pane_click(i),
                Action::Dropdown(i) => self.dropdown_pick(i),
                Action::CloseDropdown => self.menus.dropdown = None,
                Action::Photo(r) => self.photo_request(r),
            }
        }
    }
}

#[cfg(test)]
mod pause_navigation_tests {
    use super::*;

    #[test]
    fn pause_navigation_player_lists_keep_the_online_category() {
        assert_eq!(rail_of(&ListKind::PlayerList, false), Some("players"));
        assert_eq!(rail_of(&ListKind::BanList, false), Some("players"));
        assert_eq!(rail_of(&ListKind::SaveSlots, false), Some("saveslot"));
    }
}
