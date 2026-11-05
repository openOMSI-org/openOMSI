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

fn pause_kind(kind: Option<&ListKind>, original: crate::ui::MenuKind, remote_admin: bool) -> crate::ui::MenuKind {
    match kind {
        Some(ListKind::AdminClassic) => crate::ui::MenuKind::Sections,
        Some(ListKind::Admin) if !remote_admin => crate::ui::MenuKind::Sections,
        _ => original,
    }
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
        if self.shell.pause_category == crate::shell::pause::navigation::Category::Online {
            if matches!(self.menus.list_kind, Some(ListKind::PlayerList | ListKind::BanList)) {
                if matches!(self.menus.list_kind, Some(ListKind::BanList)) { self.shell.pause_online.tab = 2; }
                self.close_list();
            }
        }
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
        let remote_admin = crate::admin_panel::remote(self);
        let kind = pause_kind(self.menus.list_kind.as_ref(), kind, remote_admin);
        let badge = match self.menus.list_kind.as_ref() {
            Some(ListKind::PlayerList) => Some(crate::player_list::count(self)),
            Some(ListKind::AdminPlayers) if remote_admin => Some(self.net.admin_panel.total),
            _ => None,
        };
        let camera_tab = matches!(self.menus.list_kind, Some(ListKind::Options(t)) if Some(t) == tabs.as_ref().and_then(|(titles, _)| titles.iter().position(|x| x == omsi_ui::tr("Camera").as_ref())));
        let list = self.menus.chooser.map(|sel| ListView {
            kind,
            head,
            badge,
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
            online: crate::shell::pause::online_ui::from_app(self),
        };
        crate::themes::sync(&self.settings.ui_theme_pack, self.settings.is_light_theme());
        self.shell.begin(w, h, scale, dt);
        let search_focused = crate::shell::pause::search::focused(&self.shell);
        crate::shell::pause::draw(&mut self.shell, &view);
        self.shell.finish();
        let focused_now = crate::shell::pause::search::focused(&self.shell);
        if search_focused != focused_now {
            crate::platform::phone_keyboard(self.window.as_deref(), focused_now);
        }
    }

    fn apply_online_action(&mut self, tab: u8, action: &str) {
        if self.menus.game_menu.is_none() || self.net.lan.is_none() || self.shell.pause_category != crate::shell::pause::navigation::Category::Online { return; }
        if tab == 0 {
            if let Some((target, stage)) = self.shell.pause_online.management.clone() {
                if action == format!("pick {}", target.tag()) && stage.is_none() {
                    if crate::player_list::items(self, &ListKind::PlayerList).iter().any(|row| row.1 == action) {
                        crate::player_list::run(self, &ListKind::PlayerList, action);
                    }
                    return;
                }
                let kind = stage.clone().map_or(ListKind::PlayerPage(target), |verb| {
                    ListKind::PlayerDo(verb, target)
                });
                if !crate::player_list::items(self, &kind)
                    .iter()
                    .any(|row| row.1 == action && row.1 != "noop")
                {
                    return;
                }
                let next = crate::player_list::run(self, &kind, action);
                if matches!(next, Some(ListKind::PlayerList)) && stage.is_some() {
                    self.shell.pause_online.management = None;
                }
                if crate::admin_panel::remote(self) {
                    self.net.admin_panel.soon();
                }
                return;
            }
        }
        if tab == 1 {
            let allowed = if crate::admin_panel::remote(self) {
                self.net.admin_panel.main_lines(std::time::Instant::now(), &|text| text.to_string()).iter().any(|row| row.1.strip_prefix("send ") == Some(action))
            } else { crate::admin::items(self).iter().any(|row| row.1 == action) };
            if crate::player_list::rights(self) == Some(omsi_admin::Role::Admin) && allowed {
                crate::admin::run(self, action);
                if crate::admin_panel::remote(self) { self.net.admin_panel.soon(); }
            }
        } else if tab == 0 || tab == 2 {
            let kind = if tab == 2 { ListKind::BanList } else { ListKind::PlayerList };
            if !crate::player_list::items(self, &kind).iter().any(|row| row.1 == action && row.1 != "noop") { return; }
            if let Some(next) = crate::player_list::run(self, &kind, action) {
                if !matches!(next, ListKind::PlayerList | ListKind::BanList) { self.open_list(next); }
            }
        }
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
                        if category == self.shell.pause_category && !self.shell.pause_hidden && self.menus.list_kind.is_none() {
                            self.shell.pause_hidden = true;
                            self.shell.quick_edit = false;
                        } else {
                            self.switch_pause_category(category);
                        }
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
                Action::OnlineAction(tab, action) => {
                    self.apply_online_action(tab, &action);
                }
                Action::Control(k, fx) => {
                    if self.menus.chooser.is_some() {
                        self.menus.chooser = Some(k);
                        self.list_click(k, fx);
                    }
                }
                Action::Date(date) => self.choose_calendar_date(&date),
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

    #[test]
    fn local_admin_pages_use_sections_without_changing_remote_admin_pages() {
        use crate::ui::MenuKind;
        assert_eq!(pause_kind(Some(&ListKind::Admin), MenuKind::List, false), MenuKind::Sections);
        assert_eq!(pause_kind(Some(&ListKind::Admin), MenuKind::List, true), MenuKind::List);
        assert_eq!(pause_kind(Some(&ListKind::AdminClassic), MenuKind::List, true), MenuKind::Sections);
    }
}

#[cfg(test)]
mod online_action_tests {
    use super::*;
    #[test]
    fn online_room_switch_opens_the_existing_form_without_leaving_the_session() {
        use clap::Parser;
        let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
        app.net.lan = Some(omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap());
        app.menus.game_menu = Some(0);
        app.shell.pause_category = crate::shell::pause::navigation::Category::Online;
        assert!(app.game_menu_action("multiplayer"));
        assert!(app.menus.game_panels.multiplayer.is_some());
        assert!(app.menus.game_menu.is_none());
        assert!(app.net.lan.is_some());
    }

    #[test]
    fn online_world_action_changes_the_session_without_opening_a_list() {
        use clap::Parser;
        let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
        app.net.lan = Some(omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap());
        app.menus.game_menu = Some(0);
        app.shell.pause_category = crate::shell::pause::navigation::Category::Online;
        app.apply_online_action(1, "speed 4");
        assert_eq!(app.net.lan.as_ref().unwrap().clock_speed, 4.0);
        assert!(app.menus.list_kind.is_none() && app.menus.chooser.is_none());
        app.apply_online_action(1, "speed 999");
        assert_eq!(app.net.lan.as_ref().unwrap().clock_speed, 4.0);
        app.menus.game_menu = None;
        app.apply_online_action(1, "speed 8");
        assert_eq!(app.net.lan.as_ref().unwrap().clock_speed, 4.0);
    }

    #[test]
    fn online_quick_player_list_uses_the_shared_page() {
        use clap::Parser;
        let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
        app.net.lan = Some(omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap());
        app.menus.game_menu = Some(0);
        app.shell.pause_category = crate::shell::pause::navigation::Category::Home;
        app.shell.pause_hidden = false;
        assert!(app.game_menu_action("players"));
        assert_eq!(app.shell.pause_category, crate::shell::pause::navigation::Category::Online);
        assert_eq!(app.shell.pause_online.tab, 0);
        assert!(!app.shell.pause_hidden);
        assert!(app.menus.list_kind.is_none() && app.menus.chooser.is_none());
        assert_eq!(crate::shell::pause::online_ui::from_app(&app).players.len(), 1);
    }

    #[test]
    fn online_player_modal_rejects_stale_targets_and_unlisted_actions() {
        use clap::Parser;
        let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
        app.net.lan = Some(omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap());
        app.menus.game_menu = Some(0);
        app.shell.pause_category = crate::shell::pause::navigation::Category::Online;
        app.shell.pause_online.management = Some((omsi_admin::Target { id: 2, conn: 999 }, None));
        let before = app.settings.voice_all_off;
        app.apply_online_action(0, "vall");
        assert_eq!(app.settings.voice_all_off, before);
        app.apply_online_action(0, "run speed 8");
        assert_eq!(app.net.lan.as_ref().unwrap().clock_speed, 1.0);
        assert!(crate::shell::pause::online_ui::from_app(&app).management.is_none());
        assert!(app.menus.list_kind.is_none());
    }

    #[test]
    fn online_quick_multiplayer_opens_the_online_page_and_keeps_the_session() {
        use clap::Parser;
        let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
        app.net.lan = Some(omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap());
        app.menus.game_menu = Some(0);
        app.shell.pause_category = crate::shell::pause::navigation::Category::Home;
        app.shell.pause_hidden = false;
        app.shell.pause_online.tab = 2;
        assert!(app.game_menu_action("multiplayer"));
        assert_eq!(app.shell.pause_category, crate::shell::pause::navigation::Category::Online);
        assert_eq!(app.shell.pause_online.tab, 0);
        assert!(!app.shell.pause_hidden);
        assert!(app.menus.game_panels.multiplayer.is_none());
        assert!(app.menus.game_menu.is_some());
        assert!(app.net.lan.is_some());
        assert!(app.menus.list_kind.is_none());
    }

    #[test]
    fn online_multiplayer_without_a_session_still_opens_the_connection_form() {
        use clap::Parser;
        let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
        app.menus.game_menu = Some(0);
        assert!(app.game_menu_action("multiplayer"));
        assert!(app.menus.game_panels.multiplayer.is_some());
        assert!(app.net.lan.is_none());
    }

}
