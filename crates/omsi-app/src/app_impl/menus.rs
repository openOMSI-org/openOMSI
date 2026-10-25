//! The game menu (Esc): its lines, its keys, the mouse wheel on it and what its lines do.

use super::*;
use std::borrow::Cow;

impl App {
    /// Open the game menu: the simulation pauses (not in a LAN session, which runs on
    /// for the other players).
    pub(crate) fn open_game_menu(&mut self) {
        self.menus.menu_prev_pause = self.paused;
        // the menu takes the keys, their key-ups too: what is held now is let go here, or a
        // steering key let go in the menu went on turning the wheel to full lock once the
        // menu closed (a throttle key went on accelerating, a door button stayed pressed)
        self.release_vehicle_keys();
        // Mouse releases are consumed by the menu too. Finish any pending physical
        // drag before letting go. VR stops precisely; desktop keeps its endpoint fallback.
        let (dx, dy) = std::mem::take(&mut self.input.drag_delta);
        let vr_active = self.vr_active();
        if let Some(player) = self.player.as_mut() {
            if self.input.dragging && (dx != 0.0 || dy != 0.0) {
                player.drag(dx, dy);
            }
            player.release_for_menu(vr_active);
        }
        self.input.dragging = false;
        self.input.buttons_held = (false, false);
        self.input.both_drag = None;
        if self.net.lan.is_none() {
            self.paused = true;
        }
        self.shell.pause_category = Default::default();
        self.shell.pause_hidden = true;
        self.shell.quick_edit = false;
        self.menus.game_menu = Some(0);
        self.menus.menu_top = None;
        self.menus.menu_kbd = true;
        self.menus.menu_drag = None;
    }

    pub(crate) fn close_game_menu(&mut self) {
        self.shell.quick_edit = false;
        self.menus.key_capture = None;
        if self.menus.menu_edit_number {
            if let Some(w)=self.window.as_ref(){w.set_ime_allowed(false);}
            self.menus.menu_edit_number=false; self.menus.menu_edit=None;
        }
        if self.menus.menu_edit_ident {
            if let Some(w)=self.window.as_ref(){w.set_ime_allowed(false);}
            self.menus.menu_edit_ident=false; self.menus.menu_edit=None;
        }
        if self.menus.menu_edit_icao {
            if let Some(w)=self.window.as_ref(){w.set_ime_allowed(false);}
            self.menus.menu_edit_icao=false; self.menus.menu_edit=None;
        }
        let was_open = self.menus.game_menu.take().is_some();
        self.menus.menu_top = None;
        if was_open {
            self.paused = self.menus.menu_prev_pause;
        }
    }

    pub(crate) fn menu_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode) {
        self.menus.menu_kbd = true;
        if self.menus.key_capture.is_some() {
            match code {
                KeyCode::ShiftLeft | KeyCode::ShiftRight | KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::AltLeft | KeyCode::AltRight => {}
                KeyCode::Escape => self.cancel_key_capture(),
                KeyCode::Delete | KeyCode::Backspace => self.apply_key_capture(None, 0),
                _ => match crate::keys::dik_code(code) {
                    Some(scan) => {
                        let chord = omsi_content::input::chord(
                            self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight),
                            self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight),
                            self.input.keys.contains(&KeyCode::AltLeft) || self.input.keys.contains(&KeyCode::AltRight),
                        );
                        self.apply_key_capture(Some(scan), chord);
                    }
                    None => self.service_msg = Some(("That key has no DirectInput scan code".into(), 3.0)),
                },
            }
            return;
        }
        if self.menus.chooser.is_some() {
            self.chooser_key(code);
            return;
        }
        if self.quick_editor_key(code) { return; }
        let sel = self.menus.game_menu.unwrap_or(0);
        let modified = self.input.keys.iter().any(|key| {
            matches!(*key, KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::AltLeft | KeyCode::AltRight | KeyCode::ShiftLeft | KeyCode::ShiftRight)
        });
        self.menus.menu_top = None;
        if code == KeyCode::KeyE && !modified && !self.vr_active() && self.shell.pause_category == crate::shell::pause::navigation::Category::Home {
            self.shell.quick_edit = true;
            self.shell.quick_focus = 0;
            self.shell.last_sel = None;
            return;
        }
        match code {
            // P changes only the simulation state, even while a menu is open.
            KeyCode::KeyP if !modified => self.toggle_pause(),
            KeyCode::Escape => self.close_game_menu(),
            KeyCode::ArrowUp | KeyCode::KeyW => self.menus.game_menu = Some(self.pause_menu_step(sel, false)),
            KeyCode::ArrowDown | KeyCode::KeyS => self.menus.game_menu = Some(self.pause_menu_step(sel, true)),
            KeyCode::ArrowLeft | KeyCode::KeyA if !self.vr_active() => self.step_pause_category(false),
            KeyCode::ArrowRight | KeyCode::KeyD if !self.vr_active() => self.step_pause_category(true),
            KeyCode::Tab if !self.vr_active() => {
                let backwards = self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight);
                self.step_pause_category(!backwards);
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => self.menu_choose(event_loop, sel),
            _ => {}
        }
    }

    /// The category is only a view of the permission-filtered menu, never a second action list.
    pub(crate) fn switch_pause_category(&mut self, category: crate::shell::pause::navigation::Category) {
        let menu = self.game_menu_items();
        let ids: Vec<_> = menu.iter().map(|line| line.0).collect();
        if !crate::shell::pause::navigation::Category::available(&ids).contains(&category) {
            return;
        }
        self.close_list();
        self.shell.pause_category = category;
        self.shell.pause_hidden = false;
        self.shell.quick_edit = false;
        self.menus.game_menu = Some(crate::shell::pause::quick_actions::indices(category, &menu, &self.settings.pause_quick_actions).first().copied().unwrap_or(0));
        self.shell.last_sel = None;
    }

    fn step_pause_category(&mut self, forward: bool) {
        let menu = self.game_menu_items();
        let ids: Vec<_> = menu.iter().map(|line| line.0).collect();
        let categories = crate::shell::pause::navigation::Category::available(&ids);
        let current = categories.iter().position(|c| *c == self.shell.pause_category).unwrap_or(0);
        let next = if forward { (current + 1) % categories.len() } else { (current + categories.len() - 1) % categories.len() };
        self.switch_pause_category(categories[next]);
    }

    fn pause_menu_step(&self, from: usize, down: bool) -> usize {
        let menu = self.game_menu_items();
        if self.vr_active() {
            return self.menu_step(from, menu.len(), down);
        }
        let ids: Vec<_> = menu.iter().map(|line| line.0).collect();
        let mut indices: Vec<_> = ids.iter().position(|id| *id == "resume").into_iter().collect();
        indices.extend(crate::shell::pause::quick_actions::indices(self.shell.pause_category, &menu, &self.settings.pause_quick_actions));
        indices.extend(ids.iter().position(|id| *id == "quit"));
        indices.retain(|k| !self.menu_item_off(*k));
        crate::shell::pause::navigation::step(&indices, from, down).unwrap_or(from)
    }

    /// The open drop-down's scroll bar held with the mouse at height `y`: the first entry
    /// shown follows the thumb.
    pub(crate) fn drag_dropdown(&mut self, y: f32) {
        let (Some(grab), Some(ui)) = (self.menus.dd_scroll_drag, self.ui.as_ref()) else { return };
        let Some((track, thumb)) = ui.dd_scroll else { return };
        let rows = ui.dd_rows.max(1);
        let Some(d) = self.menus.dropdown.as_mut() else { return };
        d.top = dropdown_top_at(y - grab, track, thumb[3] - thumb[1], d.items.len(), rows);
    }

    /// The scroll bar of the timetable beside the tours held with the mouse at height `y`:
    /// the first stop shown follows the thumb.
    pub(crate) fn drag_pane(&mut self, y: f32) {
        let (Some(grab), Some(k)) = (self.menus.pane_scroll_drag, self.menus.chooser) else { return };
        let Some((track, thumb, n, fit)) = self.ui.as_ref().and_then(|u| u.menu_pane_scroll) else { return };
        self.menus.pane_scroll = Some((k, dropdown_top_at(y - grab, track, thumb[3] - thumb[1], n, fit)));
    }

    /// The mouse wheel over the game menu: the chosen line moves (the menu scrolls with it),
    /// in a list the same; no wrapping round.
    pub(crate) fn menu_wheel(&mut self, amount: f32) {
        // (the pause menu of the launcher's toolkit scrolls what is under the mouse itself)
        if self.shell_takes_mouse() {
            self.menus.menu_kbd = false;
            self.shell.wheel(amount);
            return;
        }
        // (an open drop-down scrolls, not the window under it)
        if self.menus.dropdown.is_some() {
            self.menus.wheel_acc += amount;
            let steps = self.menus.wheel_acc.trunc() as i64;
            if steps == 0 {
                return;
            }
            self.menus.wheel_acc -= steps as f32;
            let rows = self.ui.as_ref().map(|u| u.dd_rows).unwrap_or(8);
            if let Some(d) = self.menus.dropdown.as_mut() {
                let max = d.items.len().saturating_sub(rows) as i64;
                d.top = (d.top as i64 - steps).clamp(0, max) as usize;
            }
            return;
        }
        self.menus.wheel_acc += amount;
        let steps = self.menus.wheel_acc.trunc() as i64;
        if steps == 0 {
            return;
        }
        self.menus.wheel_acc -= steps as f32;
        // the wheel over the timetable beside the tours scrolls its stops
        if let (Some(u), Some(k)) = (self.ui.as_ref(), self.menus.chooser) {
            let (x, y) = self.input.cursor;
            if u.menu_pane_box.is_some_and(|r| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]) {
                let first = (u.menu_pane_start as i64 - steps).max(0) as usize;
                self.menus.pane_scroll = Some((k, first));
                return;
            }
        }
        // the list scrolls under the mouse; what is chosen stays chosen (the wheel used to
        // walk the highlight up and down the lines)
        let n = self.menu_len() as f32;
        let (start, rows) = self.ui.as_ref().map(|u| (u.menu_start as f32, u.menu_rows as f32)).unwrap_or((0.0, n));
        let top = (self.menus.menu_top.unwrap_or(start) - steps as f32).clamp(0.0, (n - rows).max(0.0));
        self.menus.menu_top = Some(top);
    }

    /// How many lines the menu shows now (the chooser's list, else the game menu's).
    pub(crate) fn menu_len(&self) -> usize {
        match self.menus.chooser {
            Some(_) => self.menus.admin_list.as_ref().unwrap_or(&self.menus.vehicle_list).len(),
            None => self.game_menu_items().len(),
        }
    }

    /// Do what line `k` of the game menu says.
    pub(crate) fn menu_choose(&mut self, event_loop: &ActiveEventLoop, k: usize) {
        // (a click in an open list keeps the scroll where it is: no jump to the line)
        self.menus.menu_top = if self.menus.chooser.is_some() { self.ui.as_ref().map(|u| u.menu_start as f32) } else { None };
        if self.menus.chooser.is_some() {
            self.chooser_pick(k);
            return;
        }
        // (a greyed-out line does nothing)
        if self.menu_item_off(k) {
            return;
        }
        let Some(id) = self.game_menu_items().get(k).map(|m| m.0) else { return };
        match id {
            "resume" => self.close_game_menu(),
            "options" => self.open_list(crate::game_lists::ListKind::Options(0)),
            "controls" => self.open_list(crate::game_lists::ListKind::Controls),
            "camera" => {
                let tab = crate::game_lists::tab_of(self, crate::game_lists::ListKind::Options(0), "Camera");
                self.open_list(crate::game_lists::ListKind::Options(tab));
            }
            "vehicle" => self.open_list(crate::game_lists::ListKind::Vehicle(0)),
            "world" => self.open_list(crate::game_lists::ListKind::World(0)),
            "copycode" => {
                self.close_game_menu();
                self.copy_server_code();
            }
            "admin" => self.open_list(crate::game_lists::ListKind::Admin),
            "players" => self.open_list(crate::game_lists::ListKind::PlayerList),
            "duty" => self.open_list(crate::game_lists::ListKind::Lines),
            "map" => {
                self.close_game_menu();
                if let Some(n) = self.menus.navigator.as_mut() {
                    if !n.map_open() {
                        n.toggle_map();
                    }
                }
            }
            "save" => {
                self.quick_save();
                self.close_game_menu();
            }
            // (a new slot, or one of the map's slots to save over: #2556)
            "saveslot" => self.open_list(crate::game_lists::ListKind::SaveSlots),
            "shot" => {
                self.close_game_menu();
                self.take_screenshot();
            }
            "photo" => {
                self.close_game_menu();
                self.enter_photo();
            }
            "skipstop" => {
                self.close_game_menu();
                self.skip_next_stop();
            }
            // the route ends here: free drive, as the list of lines has it
            "endduty" => {
                crate::game_lists::end_duty(self);
                self.close_game_menu();
            }
            "tobus" => {
                self.close_game_menu();
                self.back_to_bus();
            }
            "load" => {
                self.close_game_menu();
                if self.load_quicksave() {
                    self.finish_session();
                    crate::platform::exit(event_loop);
                }
            }
            "quit" => {
                self.menus.game_menu = None;
                self.finish_session();
                crate::platform::exit(event_loop);
            }
            "plugin" => self.choose_plugin_line(k),
            // (the rest are the lines of the vehicle and world pages)
            other => {
                self.page_action(other);
            }
        }
    }

    /// The actions of the vehicle and world pages (and of what the plugins and the input
    /// script ask of the menu by name). False when `id` is none of them.
    pub(crate) fn page_action(&mut self, id: &str) -> bool {
        match id {
            "swap" | "place" => {
                // (a plain "Place a vehicle" puts one beside; "Swap" in the driven one's place)
                self.menus.swap_pending = id == "swap" && self.player.is_some();
                if self.menus.vehicle_list.is_empty() {
                    let menu = crate::menu::Menu::new(&self.args.root, &self.args.map);
                    self.menus.vehicle_meta = menu.vehicles.iter().zip(menu.vehicle_meta).map(|(v, meta)| (v.1.clone(), meta)).collect();
                    self.menus.vehicle_list = menu.vehicles;
                    // (alphabetical)
                    self.menus.vehicle_list.sort_by_key(|v| v.0.to_lowercase());
                    crate::mt::protect(self.menus.vehicle_list.iter().map(|v| v.0.as_str()));
                }
                if self.menus.vehicle_list.is_empty() {
                    self.service_msg = Some(("No vehicles found".into(), 3.0));
                } else if crate::lan::server_offers().is_some_and(|o| !self.menus.vehicle_list.iter().any(|v| crate::lan::offers(&o, &v.1))) {
                    self.service_msg = Some(("The server offers none of the vehicles installed here".into(), 4.0));
                } else {
                    // (as the launcher's bus step: the manufacturer, then the type)
                    self.open_list(crate::game_lists::ListKind::PlaceMaker);
                }
            }
            "couple" => {
                self.close_game_menu();
                self.couple();
            }
            "uncouple" => {
                self.close_game_menu();
                self.uncouple();
            }
            "tobus" => {
                self.close_game_menu();
                self.back_to_bus();
            }
            "remove" => {
                self.close_game_menu();
                self.remove_driven_vehicle();
            }
            "reload" => {
                self.close_game_menu();
                self.reload_driven_vehicle();
            }
            "clearplaced" => {
                self.close_game_menu();
                self.remove_placed_vehicles();
            }
            "getout" => {
                self.close_game_menu();
                self.get_up();
            }
            "reset" => {
                self.close_game_menu();
                if let Some(p) = self.player.as_ref() {
                    let (at, heading) = (p.vehicle.position, p.vehicle.heading);
                    crate::admin::teleport(self, at, heading);
                    self.service_msg = Some(("The vehicle stands on its wheels again".into(), 3.0));
                    self.service_event("reset", self.menu_by(), None);
                }
            }
            "teleport" => {
                self.close_game_menu();
                if let Some(n) = self.menus.navigator.as_mut() {
                    if !n.map_open() {
                        n.toggle_map();
                    }
                    self.menus.teleport_pick = true;
                    self.service_msg = Some(("Click a street on the map: the bus is put there".into(), 6.0));
                }
            }
            // (the game's own panels, by name for the plugins and the input script)
            "view_editor" => crate::game_panels::view_editor::open(self),
            "mirror_hud_edit" => {
                self.close_game_menu();
                self.view = "driver".into();
                if let Some(p) = self.player.as_ref() {
                    let msg = self.gfx.mirror_hud.start_edit(p);
                    self.service_msg = Some((msg, 6.0));
                }
            }
            "dest_form" => crate::game_panels::open_dest_form(self),
            "quick_menu" => crate::game_panels::quick_menu::toggle(self),
            "driver" => self.open_list(crate::game_lists::ListKind::Drivers),
            "number" => self.open_list(crate::game_lists::ListKind::Numbers),
            "dest" => self.open_list(crate::game_lists::ListKind::Destinations),
            "hof" => self.open_list(crate::game_lists::ListKind::Hofs),
            "tplist" => self.open_list(crate::game_lists::ListKind::Spots),
            "editor" => {
                self.close_game_menu();
                self.toggle_editor();
            }
            "timetable" => {
                self.menus.timetable = !self.menus.timetable;
                self.close_game_menu();
            }
            "info" => {
                self.set_info_bar(!self.menus.info_bar);
                self.close_game_menu();
            }
            "refuel" | "wash" | "repair" => {
                self.close_game_menu();
                self.run_service(id, self.menu_by());
            }
            "weather" => {
                self.close_game_menu();
                self.next_weather();
            }
            "metar_once" => self.load_metar_once(),
            "metar_refresh" => self.refresh_metar_now(),
            "weather_custom" => self.current_weather_as_custom(),
            "switch" => {
                self.close_game_menu();
                self.switch_vehicle();
            }
            "switchto" => self.open_list(crate::game_lists::ListKind::SwitchTo),
            "later" | "earlier" | "later10" | "earlier10" => {
                self.close_game_menu();
                if self.net.lan.as_ref().map(|l| l.role == omsi_net::Role::Client).unwrap_or(false) {
                    self.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
                } else {
                    self.shift_clock(match id {
                        "later" => 3600.0,
                        "earlier" => -3600.0,
                        "later10" => 600.0,
                        _ => -600.0,
                    });
                }
            }
            _ => return false,
        }
        true
    }
}

/// The game menu on a server (`--lan-join https://…`): the world's clock and weather are the
/// server's, and the way out leaves the server.
pub(crate) const SERVER_GAME_MENU: [(&str, &str); 9] = [
    ("resume", "Resume"),
    ("options", "Options..."),
    ("camera", "Camera..."),
    ("vehicle", "Vehicle options..."),
    ("world", "World options..."),
    ("map", "City map"),
    ("shot", "Screenshot"),
    ("photo", "Photo mode"),
    ("quit", "Leave the server"),
];

/// Whether the session was started on a server (`--lan-join https://…`).
pub(crate) fn on_server(args: &crate::Args) -> bool {
    args.lan_join.as_deref().map(|t| omsi_net::ws::ws_url(t).is_some()).unwrap_or(false)
}

/// The game menu's lines for a session with these arguments.
pub(crate) fn game_menu_for(args: &crate::Args) -> &'static [(&'static str, &'static str)] {
    if on_server(args) {
        &SERVER_GAME_MENU
    } else {
        &GAME_MENU
    }
}

/// A line of the game menu: what it does, its label, and for a plugin's line (`"plugin"`)
/// whose and which it is, so that a click on it goes to that very line.
pub(crate) type MenuLine = (&'static str, Cow<'static, str>, Option<omsi_plugin::ui::MenuItem>);

impl crate::App {
    /// The game menu's lines for this session: back to the own bus while walking about,
    /// the administration for a host and a server's admin, the plugins' lines
    /// (`ui.set_menu_item`, each a `"plugin"` line). What can be set is behind "Options",
    /// "Vehicle options" and "World options".
    pub(crate) fn game_menu_items(&self) -> Vec<MenuLine> {
        let mut v: Vec<MenuLine> = game_menu_for(&self.args).iter().map(|&(id, label)| (id, Cow::Borrowed(label), None)).collect();
        let mut at = 1;
        if self.session.on_foot.is_some() && self.player.is_some() {
            v.insert(at, ("tobus", "Back to my bus".into(), None));
            at += 1;
        }
        // without a bus of one's own: no line to drive
        if self.player.is_none() {
            v.retain(|x| x.0 != "duty");
        }
        // ending the route is offered only while there is one, skipping a stop while its
        // trip still has one to come
        if self.session.duty.is_none() {
            v.retain(|x| x.0 != "endduty");
        }
        if !self.session.duty.as_ref().is_some_and(|d| d.stop_to_skip()) {
            v.retain(|x| x.0 != "skipstop");
        }
        if self.menus.navigator.is_none() {
            v.retain(|x| x.0 != "map");
        }
        let host = self.net.lan.as_ref().map(|l| l.role == omsi_net::Role::Host).unwrap_or(false);
        // the server code: a line to copy it, right under "World options" (only in a LAN session or on a server)
        if self.net.lan.is_some() || on_server(&self.args) {
            if let Some(w) = v.iter().position(|x| x.0 == "world") {
                v.insert(w + 1, ("copycode", "Copy server code".into(), None));
                at = at.max(w + 2);
            }
        }
        // everybody in a multiplayer session may see who is there
        if self.net.lan.is_some() {
            let before_quit = v.iter().position(|x| x.0 == "quit").unwrap_or(v.len()).max(at);
            v.insert(before_quit, ("players", "Player list...".into(), None));
        }
        if host || self.net.is_admin {
            let before_quit = v.iter().position(|x| x.0 == "quit").unwrap_or(v.len()).max(at);
            v.insert(before_quit, ("admin", "Administration...".into(), None));
        }
        // the plugins' lines, a group of their own before saving (or the way out)
        let lines = self.plugin_menu_lines();
        let before = v.iter().position(|x| matches!(x.0, "save" | "admin" | "quit")).unwrap_or(v.len()).max(at);
        v.splice(before..before, lines.into_iter().map(|m| ("plugin", Cow::Owned(m.label.clone()), Some(m))));
        v
    }

    /// The plugins' lines of the game menu (`ui.set_menu_item`), in their order there.
    pub(crate) fn plugin_menu_lines(&self) -> Vec<omsi_plugin::ui::MenuItem> {
        self.integrations.plugins.as_ref().map(|p| p.ui.borrow().menu_items().to_vec()).unwrap_or_default()
    }

    /// The player chose the game menu's line `k`, a plugin's: the menu closes, and that
    /// line's plugin hears `menu_item` with its id in its next frame.
    fn choose_plugin_line(&mut self, k: usize) {
        let line = self.game_menu_items().into_iter().nth(k).and_then(|m| m.2);
        self.close_game_menu();
        if let (Some(m), Some(p)) = (line, self.integrations.plugins.as_ref()) {
            p.ui.borrow_mut().pick_menu_item(m.owner, &m.id);
        }
    }

    /// Put the session's server code on the clipboard.
    pub(crate) fn copy_server_code(&mut self) {
        // (the host's code; a player or a server's join code or address as it was entered)
        let Some(code) = self.net.lan.as_ref().and_then(|l| l.code()).map(|c| c.encode()).or_else(|| self.args.lan_join.clone()).filter(|c| !c.trim().is_empty()) else {
            self.service_msg = Some(("No server code: not in a LAN session or on a server".into(), 3.0));
            return;
        };
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            thread_local! {
                // (kept alive: on X11 the text is gone when the clipboard is dropped)
                static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> = const { std::cell::RefCell::new(None) };
            }
            let ok = CLIPBOARD.with(|c| {
                let mut c = c.borrow_mut();
                if c.is_none() {
                    *c = arboard::Clipboard::new().ok();
                }
                c.as_mut().is_some_and(|cb| cb.set_text(code.clone()).is_ok())
            });
            self.service_msg = Some(if ok { ("Server code copied".into(), 3.0) } else { (format!("{}: {code}", omsi_ui::tr("Server code")), 8.0) });
        }
        #[cfg(target_os = "ios")]
        {
            let ok = crate::apple::set_clipboard(&code);
            self.service_msg = Some(if ok { ("Server code copied".into(), 3.0) } else { (format!("{}: {code}", omsi_ui::tr("Server code")), 8.0) });
        }
        #[cfg(target_os = "android")]
        {
            self.service_msg = Some((format!("{}: {code}", omsi_ui::tr("Server code")), 8.0));
        }
    }

    /// The ids of the game menu's lines that are greyed out and cannot be chosen now.
    pub(crate) fn menu_disabled_ids(&self) -> &'static [&'static str] {
        &[]
    }

    /// Whether line `k` of the game menu is greyed out.
    pub(crate) fn menu_item_off(&self, k: usize) -> bool {
        self.menus.chooser.is_none() && self.game_menu_items().get(k).is_some_and(|m| self.menu_disabled_ids().contains(&m.0))
    }

    /// The next line up or down from `from` that can be chosen (round the ends; the greyed-out
    /// lines are skipped).
    fn menu_step(&self, from: usize, n: usize, down: bool) -> usize {
        let mut k = from;
        for _ in 0..n {
            k = if down { (k + 1) % n } else { (k + n - 1) % n };
            if !self.menu_item_off(k) {
                return k;
            }
        }
        from
    }
}

/// The lines of the game menu: (what, label). What can be set is on the pages behind
/// "Options", "Vehicle options" and "World options" (see `game_lists`).
pub(crate) const GAME_MENU: [(&str, &str); 16] = [
    ("resume", "Resume"),
    ("options", "Options..."),
    ("controls", "Controls..."),
    // (the driver's view - seat, field of view, head movement - straight from the pause
    // menu: it is what is changed most while driving, #908)
    ("camera", "Camera..."),
    ("vehicle", "Vehicle options..."),
    ("world", "World options..."),
    ("map", "City map"),
    ("duty", "Line and tour..."),
    ("skipstop", "Skip the next stop"),
    ("endduty", "End the tour"),
    ("save", "Save the situation"),
    ("saveslot", "Save to a slot..."),
    ("load", "Load the quicksave"),
    ("shot", "Screenshot"),
    ("photo", "Photo mode"),
    ("quit", "End the session"),
];
