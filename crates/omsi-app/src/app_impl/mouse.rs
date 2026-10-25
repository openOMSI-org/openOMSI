//! The mouse: the cursor, the buttons, dragging switches, the pages on scenery objects.

use super::*;

impl App {
    /// The cursor moved to (x, y) in physical pixels - from the window or an `OMSI_INPUT` script.
    /// The cursor moved, and the switch under it is named at once (touch input and
    /// `OMSI_INPUT` scripts read `hover` right after).
    pub(crate) fn on_cursor(&mut self, x: f32, y: f32) {
        // (while the mouse steers its movement goes to the steering point first)
        let Some((x, y)) = self.steer_cursor_event(x, y) else { return };
        if self.move_cursor(x, y) {
            self.update_hover();
        }
    }

    /// The right mouse button (or both) held in a view of the bus: start OMSI's mouse zoom
    /// (false when there is nothing to zoom - a menu, the city map, on foot).
    pub(crate) fn start_both_drag(&mut self) -> bool {
        if self.menus.game_menu.is_some() || self.player.is_none() || self.menus.navigator.as_ref().is_some_and(|n| n.map_open()) {
            return false;
        }
        let value = match self.view.as_str() {
            "outside" => self.cam.orbit,
            "driver" | "pax" => *self.cam.view_zoom.get(&self.view).unwrap_or(&1.0),
            _ => return false,
        };
        self.input.both_drag = Some((self.input.cursor.1, value));
        self.input.mouse_look = false;
        self.update_hover();
        true
    }

    /// Whether the mouse steering (when on) steers in the view shown: every view of the
    /// player's bus, the map camera (F4) included, but not walking.
    pub(crate) fn mouse_steers_in_view(&self) -> bool {
        self.player.is_some() && (matches!(self.view.as_str(), "driver" | "outside" | "pax") || (self.view == "free" && !self.cam.ego))
    }

    /// Looking round with the mouse goes by the cursor's way in the window (a view of the
    /// bus); on foot and with the free camera it keeps the raw mouse movement.
    pub(crate) fn cursor_looks(&self) -> bool {
        self.input.mouse_look && self.player.is_some() && !matches!(self.view.as_str(), "foot" | "free")
    }

    /// The right button alone zooms, as in Omsi.exe (TForm_main.Panel1MouseMove 0x82c5f8:
    /// ssRight without `[altView]`, or Shift+right with it); otherwise it turns the view.
    pub(crate) fn right_zooms(&self) -> bool {
        !self.settings.alt_view || self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight)
    }

    /// The right mouse button on the desktop: held, it zooms (the outside camera's distance,
    /// the view in the bus), or with OMSI's `[altView]` turns the view; the middle button
    /// turns it in any case. Where there is nothing to zoom it turns the view.
    pub(crate) fn on_right(&mut self, pressed: bool) {
        if let Some(ph) = self.photo.as_mut() {
            ph.looking = pressed;
            self.sync_look_hold();
            return;
        }
        self.input.buttons_held.1 = pressed;
        // the left button already down on nothing it works: both held zoom
        if pressed && self.input.buttons_held.0 && !self.input.dragging && self.start_both_drag() {
            return;
        }
        // (a switch held with the left button keeps the mouse: looking round
        // took the cursor's movement away from it, and the drag stopped)
        if pressed && self.input.dragging {
            return;
        }
        if !pressed {
            self.input.both_drag = None;
        }
        // a right click lets go of the mouse steering as in OMSI (#162) when the player
        // wants it so; otherwise the right button looks round and the wheel and pedals stay
        // where the mouse left them (it went off with every look round, and with every
        // look round in the pause)
        if pressed && self.input.mouse_drive && self.menus.game_menu.is_none() && self.settings.mouse_right_off && !self.paused {
            self.set_mouse_drive(false);
            self.service_msg = Some(("Mouse steering off".into(), 3.0));
        }
        if pressed && self.right_zooms() && self.start_both_drag() {
            self.sync_look_hold();
            return;
        }
        // The wheel and the pedals keep their own point, `mouse_grab.at`, while the view turns.
        // Where the cursor is held while the mouse steers (the steering cross shows the point)
        // it is not put back where the look began - that was the cursor's jump as the button
        // came up - its movement only counts afresh from where it is. Where the free cursor
        // itself shows the point (`mouse_hold` off) it goes back there, so the two agree.
        if self.input.mouse_drive && self.menus.game_menu.is_none() && self.input.look_lock.is_none() {
            if pressed {
                self.input.steer_cursor = Some(self.input.cursor);
            } else if let Some((x, y)) = self.input.steer_cursor.take() {
                if self.settings.mouse_hold {
                    self.input.mouse_grab.pause();
                } else {
                    self.input.cursor = (x, y);
                    if let Some(win) = self.window.as_ref() {
                        let _ = win.set_cursor_position(winit::dpi::PhysicalPosition::new(x as f64, y as f64));
                    }
                }
            }
        }
        self.input.mouse_look = pressed;
        // (looking round locks the cursor where it stands, or lets the steering's hold go
        // where it cannot, and holds it again after)
        self.sync_look_hold();
        self.sync_mouse_grab();
        // (the cursor shows it at once, not with the next look at what is under it)
        self.update_hover();
    }

    /// The mouse's own movement (dx, dy) while looking round with the cursor locked
    /// (`sync_look_hold`): in logical pixels (points on macOS), at the cursor's rate - the
    /// field of view over 78.75 per pixel, as Omsi.exe turns the view by the cursor's way.
    /// False when the look does not go by it (the raw look of the free camera and on foot).
    pub(crate) fn look_raw(&mut self, dx: f32, dy: f32) -> bool {
        if self.input.look_lock.is_none() {
            return false;
        }
        if let Some(ph) = self.photo.as_mut() {
            if ph.looking {
                let k = look_deg_per_px(ph.cam.fov_deg) * self.settings.look_sens;
                ph.look(dx * k, dy * k);
            }
            return true;
        }
        if !self.cursor_looks() {
            return false;
        }
        let fov = self.camera.as_ref().map(|c| c.fov_deg).unwrap_or(60.0);
        let k = look_deg_per_px(fov) * self.settings.look_sens;
        self.look_by(dx * k, dy * k);
        true
    }

    pub(crate) fn on_mouse_moved(&mut self, x: f32, y: f32) {
        // (while the mouse steers its movement goes to the steering point first)
        let Some((x, y)) = self.steer_cursor_event(x, y) else { return };
        self.cursor_moved_to(x, y);
    }

    /// The cursor's new place in the window, as the window reported it or as the mouse
    /// steering's point stands in it.
    pub(super) fn cursor_moved_to(&mut self, x: f32, y: f32) {
        // a plugin's slider or panel being dragged follows the cursor
        if self.integrations.plugin_drag_move(x, y) || self.game_panel_drag(x) {
            self.input.cursor = (x, y);
            return;
        }
        // a mirror panel being dragged follows the cursor (nothing else of the cursor's
        // work is done meanwhile, and outside a drag none of it is touched)
        if self.gfx.mirror_hud.dragging() {
            if let Some(size) = self.gfx.surface.as_ref().map(|_| self.hud_size()) {
                let origin_x = self.input.cursor.0 - self.hud_cursor().0;
                if self.gfx.mirror_hud.moved((x - origin_x, y), size) {
                    self.input.cursor = (x, y);
                    return;
                }
            }
        }
        if self.move_cursor(x, y) {
            self.html_move();
        }
    }

    fn html_move(&mut self) {
        let sim = self.player_to_drawn_pose();
        self.html_move_now();
        self.player_back_to_sim_pose(sim);
    }

    fn html_move_now(&mut self) {
        if let Some((id, page, ..)) = self.input.html_object_pressed {
            let Some((o, d, _)) = self.cursor_ray_now() else { return };
            let Some(w) = self.world.clone() else { return };
            if let Some(h) = w.html_object_hit(o, d, HTML_OBJECT_REACH).filter(|h| h.map_id == id && h.page == page) {
                w.html_object_pointer(&mut self.session.scenery, id, page, h.u, h.v, omsi_sim::htmltex::PointerKind::Move);
                self.input.html_object_pressed = Some((id, page, h.u, h.v));
            }
            return;
        }
        let Some((page, ..)) = self.input.html_pressed else { return };
        let Some((o, d, _)) = self.cursor_ray_now() else { return };
        let Some(p) = self.player.as_mut() else { return };
        if let Some((pg, u, v)) = p.html_hit(o, d, &self.integrations.plugin_pointer).filter(|h| h.0 == page) {
            p.html_pointer(pg, u, v, omsi_sim::htmltex::PointerKind::Move, &mut self.integrations.plugin_pointer);
            self.input.html_pressed = Some((pg, u, v));
        }
    }

    #[cfg(windows)]
    pub(crate) fn reset_vr_pointer(&mut self) {
        if let Some(vr) = self.xr.vr.as_mut() {
            vr.recenter_pointer();
        }
        self.xr.vr_cursor_physical = None;
        self.xr.vr_cursor_warp_pending = None;
    }

    #[cfg(windows)]
    pub(crate) fn on_vr_cursor_moved(&mut self, x: f32, y: f32) {
        if let Some(target) = self.xr.vr_cursor_warp_pending.take() {
            // CursorMoved from set_cursor_position is not hand movement.
            self.xr.vr_cursor_physical = Some((x, y));
            if (x - target.0).abs() < 3.0 && (y - target.1).abs() < 3.0 {
                return;
            }
            // A real move arrived first; use the next event as the new baseline.
            return;
        }
        if let Some(previous) = self.xr.vr_cursor_physical {
            let (dx, dy) = (x - previous.0, y - previous.1);
            self.input.cursor.0 += dx;
            self.input.cursor.1 += dy;
            self.accumulate_cockpit_drag(dx, dy);
        }
        self.xr.vr_cursor_physical = Some((x, y));
        self.html_move();
        let Some((width, height)) = self.gfx.surface.as_ref().map(|s|
            (s.config.width as f32, s.config.height as f32)) else { return };
        if self.input.window_focused && !self.input.mouse_look
            && (x < 12.0 || x > width - 12.0 || y < 12.0 || y > height - 12.0) {
            let center = (width * 0.5, height * 0.5);
            if self.window.as_ref().is_some_and(|window| window.set_cursor_position(
                winit::dpi::PhysicalPosition::new(center.0 as f64, center.1 as f64)).is_ok()) {
                self.xr.vr_cursor_physical = Some(center);
                self.xr.vr_cursor_warp_pending = Some(center);
            }
        }
    }

    #[cfg(windows)]
    pub(crate) fn poll_vr_cursor_position(&mut self) {
        if self.xr.vr_nav_edit.is_some() { return; }
        let cockpit = self.xr.vr.is_some() && self.menus.game_menu.is_none()
            && self.menus.chooser.is_none() && !self.input.mouse_drive
            && matches!(self.view.as_str(), "driver" | "pax");
        if !cockpit {
            self.xr.vr_cursor_physical = None;
            self.xr.vr_cursor_warp_pending = None;
            return;
        }
        if !self.input.window_focused || self.input.mouse_look { return; }
        let Some(window) = self.window.as_ref() else { return };
        let Ok(client_origin) = window.inner_position() else { return };
        let mut point = windows::Win32::Foundation::POINT::default();
        if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) }.is_ok() {
            self.on_vr_cursor_moved((point.x - client_origin.x) as f32,
                                    (point.y - client_origin.y) as f32);
        }
    }

    /// Take the cursor's new place; false when the move was someone else's (the object
    /// editor's drag, the city map) and no switch is to be named.
    fn move_cursor(&mut self, x: f32, y: f32) -> bool {
        let last = self.input.cursor;
        self.shell.pointer(x, y);
        // the photo camera turned by a drag (and nothing else of the game's under the mouse)
        if let Some(ph) = self.photo.as_mut() {
            self.input.cursor = (x, y);
            if ph.looking && self.input.look_lock.is_none() {
                let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0).max(0.1);
                let k = look_deg_per_px(ph.cam.fov_deg) * self.settings.look_sens;
                ph.look((x - last.0) / scale * k, (y - last.1) / scale * k);
            }
            return false;
        }
        self.input.cursor = (x, y);
        if !self.input.buttons_held.0 {
            self.menus.hud_resize = None;
        }
        if let Some((k, size0, at, w0)) = self.menus.hud_resize {
            let grow = ((x - at[0]) + (y - at[1])) * 0.5;
            self.settings.hud_sizes[k] = crate::ui::window_size((size0 * (w0 + grow) / w0 * 20.0).round() / 20.0);
            return true;
        }
        // the navigator held by the mouse follows it
        if let Some(n) = self.menus.navigator.as_mut() {
            if n.panel_move(x, y) {
                return true;
            }
        }
        // (the mouse has taken over from the keyboard: only what is under it is lit)
        if self.menus.game_menu.is_some() && (x, y) != last {
            self.menus.menu_kbd = false;
        }
        // a slider held with the mouse button follows the cursor (while the menu is open)
        if self.menus.menu_drag.is_some() && self.menus.game_menu.is_none() {
            self.menus.menu_drag = None;
        }
        if let Some(k) = self.menus.menu_drag {
            let c = self.ui.as_ref().and_then(|u| k.checked_sub(u.menu_start).and_then(|i| u.menu_ctl.get(i).copied().flatten()));
            match c {
                Some(c) => {
                    let fx = ((x - c[0]) / (c[2] - c[0]).max(1.0)).clamp(0.0, 1.0);
                    self.list_click(k, fx);
                }
                None => self.menus.menu_drag = None,
            }
            return false;
        }
        if let Some((y0, v0)) = self.input.both_drag {
            // a hand on the zoom cancels an eased Space return.
            self.cam.f1_reset = None;
            // (0x82c5f8: outside, the distance at the press times 1 + the way up over 500
            // pixels; in the bus the field of view at the press plus the way up over 500
            // pixels times the camera's own, which is also its widest (+0x31c, 0x7edde4):
            // moving up widens the view as it backs the outside camera away)
            if self.view == "outside" {
                let k = (1.0 + (y0 - y) / 500.0).max(0.05);
                self.cam.orbit = (v0 * k).clamp(ORBIT_MIN, ORBIT_MAX);
            } else if self.settings.precision_zoom {
                // precision zoom from the press anchor (drag down zooms in):
                // the FOV-multiplier curve instead of the linear way, same
                // floor. Past the authored field of view it stays linear.
                let intent = if self.view == "driver" { ZOOM_INTENT_F1 } else { ZOOM_INTENT };
                let dy = y - y0;
                let m = if v0 > 1.0 {
                    (v0 - dy / 500.0).clamp(0.2, v0.max(1.0))
                } else {
                    precision_zoom_step(v0, dy, intent).clamp(0.2, 1.0)
                };
                self.cam.view_zoom.insert(self.view.clone(), m);
            } else {
                self.cam.view_zoom.insert(self.view.clone(), (v0 + (y0 - y) / 500.0).clamp(0.2, 1.0_f32.max(v0)));
            }
            return false;
        }
        if self.menus.pane_scroll_drag.is_some() {
            if self.menus.chooser.is_none() || self.menus.game_menu.is_none() {
                self.menus.pane_scroll_drag = None;
            } else {
                self.drag_pane(y);
                return false;
            }
        }
        if self.menus.dd_scroll_drag.is_some() {
            if self.menus.dropdown.is_none() || self.menus.game_menu.is_none() {
                self.menus.dd_scroll_drag = None;
            } else {
                self.drag_dropdown(y);
                return false;
            }
        }
        if self.menus.menu_scroll_drag {
            let Some(ui) = self.ui.as_ref() else {
                self.menus.menu_scroll_drag = false;
                return true;
            };

            if let (Some(track), Some(thumb)) =
                (ui.menu_scroll_track, ui.menu_scroll_thumb)
            {
                let track_h = (track[3] - track[1]).max(1.0);
                let thumb_h = (thumb[3] - thumb[1]).max(1.0);
                let travel = (track_h - thumb_h).max(1.0);

                let max_top =
                    (self.menu_len() as f32 - ui.menu_rows as f32).max(0.0);

                if max_top > 0.0 {
                    let delta = (y - last.1) / travel * max_top;

                    self.menus.menu_top = Some(
                        (self.menus.menu_top.unwrap_or(ui.menu_start as f32) + delta)
                            .clamp(0.0, max_top),
                    );
                }
            }

            return false;
        }
        // an object dragged in the object editor follows
        if self.menus.editor_drag {
            self.editor_drag_frame();
            return false;
        }
        // while the city map is open the mouse is the map's
        if let Some(n) = self.menus.navigator.as_mut().filter(|n| n.map_open()) {
            n.map_move(x, y);
            return false;
        }
        // looking round in a view of the bus follows the cursor, as Omsi.exe turns it
        // (0x82c5f8: yaw and pitch at the press plus the cursor's way times fov / 78.75):
        // raw device deltas are no window pixels (a tablet, a remote desktop or a VM
        // reports positions there and spun the view) and did not follow the zoom
        if self.cursor_looks() && self.input.look_lock.is_none() {
            let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0).max(0.1);
            let fov = self.camera.as_ref().map(|c| c.fov_deg).unwrap_or(60.0);
            let k = look_deg_per_px(fov) * self.settings.look_sens;
            self.look_by((x - last.0) / scale * k, (y - last.1) / scale * k);
        }
        // Dragging a switch reads the movement in screen pixels - take it from the
        // cursor itself rather than from the raw device delta, which is not in the
        // window's pixels (and on this Mac is not always delivered at all): that is
        // why the parking brake could not be pulled with the mouse. The movement is
        // collected here and handed to the script once a frame (`App::drag_frame`).
        if self.input.dragging {
            self.accumulate_cockpit_drag(
                self.input.cursor.0 - last.0,
                self.input.cursor.1 - last.1,
            );
        }
        true
    }

    /// Both desktop and VR mouse paths deliver logical pixels to the same held
    /// control. Head movement and cursor-recentering events must not enter here.
    fn accumulate_cockpit_drag(&mut self, dx: f32, dy: f32) {
        if self.input.dragging {
            let scale = self
                .window
                .as_ref()
                .map(|w| w.scale_factor() as f32)
                .unwrap_or(1.0)
                .max(0.1);
            self.input.drag_delta.0 += dx / scale;
            self.input.drag_delta.1 += dy / scale;
        }
    }

    pub(crate) fn on_left(&mut self, pressed: bool) {
        let sim = self.player_to_drawn_pose();
        self.left_now(pressed);
        self.player_back_to_sim_pose(sim);
    }

    fn left_now(&mut self, pressed: bool) {
        if self.xr.vr_nav_edit.is_some() { return; }
        // the object editor: the mouse picks and drags
        if self.menus.game_menu.is_none() && self.editor_mouse(pressed) {
            return;
        }
        // the MileZero card: its buttons, and a press on it goes no further
        if pressed && self.settings.milezero {
            let (x, y) = self.input.cursor;
            let at = |r: Option<[f32; 4]>| r.is_some_and(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]);
            let (play, request, card) = self.ui.as_ref().map_or((false, false, false), |u| (at(u.mz.play), at(u.mz.request), at(u.mz.rect)));
            if play {
                self.sound.milezero.want_play = !self.sound.milezero.want_play;
                return;
            } else if request {
                self.sound.milezero.typing = Some(String::new());
                return;
            } else if card {
                return;
            }
        }
        // a trolleybus's pole in the mouse's hand: let go, or taken (see `trolley_mouse`)
        if pressed && self.menus.game_menu.is_none() && self.trolley_mouse_click() {
            return;
        }
        // the city map: a click on the navigator opens it; while it is open the mouse is
        // the map's (a click outside closes it)
        let (x, y) = self.input.cursor;
        if !pressed {
            if let Some((k, ..)) = self.menus.hud_resize.take() {
                crate::game_lists::remember_setting(crate::ui::SIZE_KEYS[k], &format!("{:.2}", self.settings.hud_sizes[k]));
                return;
            }
        }
        if pressed && self.menus.game_menu.is_none() {
            if let Some((k, w0, shown)) = self.ui.as_ref().and_then(|u| u.grip_at(x, y)) {
                self.settings.hud_sizes[k] = shown;
                self.menus.hud_resize = Some((k, shown, [x, y], w0));
                return;
            }
        }
        let vr_active = self.vr_active();
        if let Some(n) = self.menus.navigator.as_mut() {
            if n.map_open() {
                let ctrl = self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight);
                if pressed && ctrl {
                    // Ctrl+click: the bus to the street nearest that point, as OMSI's map
                    // window places vehicles
                    if let Some(at) = n.map_point(x, y) {
                        if std::mem::take(&mut self.menus.teleport_pick) {
                            n.toggle_map();
                        }
                        self.place_bus_at(at);
                    }
                } else if pressed {
                    n.map_press(x, y);
                } else if n.map_release() {
                    if !self.menus.teleport_pick {
                        n.map_destination(x, y);
                        return;
                    }
                    // after Esc → Move the bus the map is dragged to the place as ever, and
                    // the click that lets go where it was pressed puts the bus there (the
                    // press used to put it at once, so only what the map showed could be
                    // chosen)
                    if let Some(at) = n.map_point(x, y) {
                        self.menus.teleport_pick = false;
                        n.toggle_map();
                        self.place_bus_at(at);
                    }
                }
                return;
            }
            // (a click opens the city map, a drag moves the navigator: #940)
            if pressed && !vr_active && n.over_panel(x, y) {
                n.panel_press(x, y);
                return;
            }
            if !pressed {
                match n.panel_release() {
                    Some(false) => {
                        n.toggle_map();
                        return;
                    }
                    Some(true) => {
                        let at = n.placement();
                        self.settings.navigator_corner = at.clone();
                        crate::game_lists::remember_setting("navigator_corner", &at);
                        // (and its size, when it was pulled at its corner)
                        if (n.size - self.settings.nav_size).abs() > 1e-3 {
                            self.settings.nav_size = n.size;
                            crate::game_lists::remember_setting("nav_size", &format!("{:.2}", n.size));
                        }
                        return;
                    }
                    None => {}
                }
            }
        }
        // the map camera (F4): Ctrl+click on the ground puts the bus on the street nearest
        // that point, as Ctrl+click on the city map does - OMSI's map view moves the vehicle
        // to a place clicked as well (#1039). A rail vehicle stays on its track.
        let ctrl = self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight);
        if pressed && ctrl && self.view == "free" && self.menus.game_menu.is_none() && self.player.is_some() {
            if self.player.as_ref().is_some_and(|p| crate::rail_drive::is_rail(&p.vehicle.ty.def)) {
                self.service_msg = Some(("A rail vehicle cannot be moved off its track".into(), 3.0));
                return;
            }
            let hit = self
                .cursor_ray_now()
                .zip(self.world.clone())
                .and_then(|((o, d, _), w)| crate::placing::ground_hit(&w, o, d.as_dvec3(), 2000.0));
            match hit {
                Some(at) => self.place_bus_at(at.truncate()),
                None => self.service_msg = Some(("Ctrl+click on the ground to move the bus there".into(), 3.0)),
            }
            return;
        }
        // a click on the chat opens its input box (and is the chat's, not the cockpit's)
        if pressed && self.net.lan.is_some() && self.settings.chat {
            if self.ui.as_ref().map(|u| u.chat.hovered).unwrap_or(false) {
                self.net.remotes.chat.open();
                return;
            }
            // a click anywhere else leaves the line and goes on to the game
            self.net.remotes.chat.blur();
        }
        // in another player's bus: a passenger, whose clicks work nothing of it (they
        // went to the driver's game, which worked its switches for them)
        if self.view == "foot" && self.net.inside_remote.is_some() {
            return;
        }
        // a page (`[htmltexture]`) on a scenery object: pressed and released like the bus's own
        if self.html_object_click(pressed) {
            return;
        }
        if self.scenery_object_click_event(pressed) {
            return;
        }
        // on foot: the own bus's switches, doors and flaps from inside it or standing by it
        if self.view == "foot" && !self.foot_reaches_bus() {
            return;
        }
        #[cfg(windows)]
        if self.xr.vr.is_some() && self.input.mouse_drive && self.menus.game_menu.is_none()
            && matches!(self.view.as_str(), "driver" | "pax") {
            if !pressed {
                if let Some(player) = self.player.as_mut() { player.release(); }
                self.input.dragging = false;
            }
            return;
        }
        let precise_release = self.vr_active() && matches!(self.view.as_str(), "driver" | "pax");
        let ray = self.camera.as_ref().zip(self.gfx.surface.as_ref())
            .map(|(cam, s)| self.cockpit_cursor_ray(cam, (s.config.width, s.config.height)));
        if let (Some(p), Some((o, d, spread))) = (
            self.player.as_mut(),
            ray,
        ) {
            let (dx, dy) = std::mem::take(&mut self.input.drag_delta);
            p.occlude_controls = self.view == "outside";
            if pressed {
                if let Some((page, u, v)) = p.html_hit(o, d, &self.integrations.plugin_pointer) {
                    p.release();
                    p.html_pointer(page, u, v, omsi_sim::htmltex::PointerKind::Down, &mut self.integrations.plugin_pointer);
                    self.input.html_pressed = Some((page, u, v));
                    self.input.dragging = false;
                    return;
                }
                // a tear-off ticket block: a ticket of its type torn off for the passenger
                if let Some(n) = self.session.humans.as_ref().and(self.gfx.sim_view.people.ticket_blocks.as_ref()).and_then(|b| b.hit(o, d, &p.vehicle)) {
                    log::info!("ticket block {n}: a ticket torn off");
                    p.vehicle.set_engine_var("GivenTicket", n as f32);
                    self.input.dragging = false;
                    return;
                }
                // a coin of the change on the tray: picked up again
                if let Some((m, k)) = self.session.humans.as_mut().and_then(|h| h.sim.money.as_mut()).and_then(|m| crate::money::change_hit(m, o, d, &p.vehicle).map(|k| (m, k))) {
                    log::info!("a coin of the change picked up again");
                    m.take_coin(k);
                    self.input.dragging = false;
                    return;
                }
                self.input.dragging = if precise_release { p.click_precise(o, d, spread) } else { p
                    .click(o, d, spread) }.is_some();
            } else {
                if let Some((page, u, v)) = self.input.html_pressed.take() {
                    let (u, v) = p.html_hit(o, d, &self.integrations.plugin_pointer).filter(|h| h.0 == page).map_or((u, v), |h| (h.1, h.2));
                    p.html_pointer(page, u, v, omsi_sim::htmltex::PointerKind::Up, &mut self.integrations.plugin_pointer);
                    self.input.dragging = false;
                    return;
                }
                // CursorMoved and the release can arrive between redraws. Deliver the
                // last movement before `_off`, so a short adjustment is not lost or
                // mistaken for a stationary click on a drag-only control.
                if self.input.dragging && (dx != 0.0 || dy != 0.0) {
                    p.drag(dx, dy);
                }
                if self.input.dragging && self.input.buttons_held.1 {
                    p.release_keeping();
                } else if precise_release {
                    p.release_precise();
                } else {
                    p.release();
                }
                self.input.dragging = false;
            }
        }
        // the map camera (F4): a click on the ground that worked nothing of the bus asks
        // whether to move the bus there (`game_panels::move_here`)
        if pressed && self.view == "free" && !self.cam.ego && !self.input.dragging && self.menus.game_menu.is_none() && !self.mouse_steering_now() {
            if self.player.as_ref().is_some_and(|p| crate::rail_drive::is_rail(&p.vehicle.ty.def)) {
                return;
            }
            let hit = self.cursor_ray_now().zip(self.world.clone()).and_then(|((o, d, _), w)| crate::placing::ground_hit(&w, o, d.as_dvec3(), 2000.0));
            if let Some(at) = hit {
                crate::game_panels::ask_move(self, at);
            }
        }
    }

    /// A click on a page of a scenery object (`[htmltexture]` in its model). True when the
    /// click was the page's: the press lands on it, the release goes to it wherever the
    /// pointer is by then.
    fn html_object_click(&mut self, pressed: bool) -> bool {
        let Some(w) = self.world.clone() else { return false };
        if !pressed {
            let Some((id, page, u, v)) = self.input.html_object_pressed.take() else { return false };
            let (u, v) = self
                .cursor_ray_now()
                .and_then(|(o, d, _)| w.html_object_hit(o, d, HTML_OBJECT_REACH))
                .filter(|h| h.map_id == id && h.page == page)
                .map_or((u, v), |h| (h.u, h.v));
            w.html_object_pointer(&mut self.session.scenery, id, page, u, v, omsi_sim::htmltex::PointerKind::Up);
            self.input.dragging = false;
            return true;
        }
        // (driving with the VR pointer: the clicks are the bus's)
        #[cfg(windows)]
        if self.xr.vr.is_some() && self.input.mouse_drive && self.menus.game_menu.is_none() && matches!(self.view.as_str(), "driver" | "pax") {
            return false;
        }
        let Some((o, d, _)) = self.cursor_ray_now() else { return false };
        let Some(h) = w.html_object_hit(o, d, HTML_OBJECT_REACH) else { return false };
        // the bus in front of the page (a switch, a window, its own page) takes the click
        if self.player.as_ref().and_then(|p| p.body_hit(o, d)).is_some_and(|t| t < h.t) {
            return false;
        }
        if let Some(p) = self.player.as_mut() {
            p.release();
        }
        w.html_object_pointer(&mut self.session.scenery, h.map_id, h.page, h.u, h.v, omsi_sim::htmltex::PointerKind::Down);
        self.input.html_object_pressed = Some((h.map_id, h.page, h.u, h.v));
        self.input.dragging = false;
        true
    }

    /// A left click (or let go) over a scenery object carrying a `[mouseevent]`.
    fn scenery_object_click_event(&mut self, pressed: bool) -> bool {
        let Some(w) = self.world.as_ref() else { return false };
        if !pressed {
            if let Some((map_id, ev)) = self.input.pressed_scenery_object.take() {
                w.scenery_object_release(&mut self.session.scenery, map_id, &ev);
                self.input.dragging = false;
                return true;
            }
            return false;
        }
        let Some((o, d, spread)) = self.cursor_ray_now() else { return false };
        let veh_has_control = if self.view == "foot" && !self.foot_reaches_bus() {
            false
        } else {
            self.player.as_ref().is_some_and(|p| {
                p.pick(o, d, spread).is_some() || p.pick_trailer(o, d, spread).is_some()
            })
        };
        if veh_has_control {
            return false;
        }
        let Some(hit) = w.scenery_object_hit(o, d, crate::input_script::SCENERY_OBJECT_REACH, spread) else { return false };
        if self.player.as_ref().and_then(|p| p.opaque_body_hit(o, d)).is_some_and(|t| t < hit.t) {
            return false;
        }
        if let Some(p) = self.player.as_mut() {
            p.release();
        }
        self.input.drag_delta = (0.0, 0.0);
        w.scenery_object_click(&mut self.session.scenery, hit.map_id, &hit.event);
        crate::plugin_io::object::clicked(self, hit.map_id, &hit.event);
        self.input.pressed_scenery_object = Some((hit.map_id, hit.event));
        self.input.dragging = true;
        true
    }

    /// The ray under the cursor now (see [`Self::cockpit_cursor_ray`]).
    pub(crate) fn cursor_ray_now(&self) -> Option<(glam::DVec3, glam::Vec3, f32)> {
        let (cam, s) = self.camera.as_ref().zip(self.gfx.surface.as_ref())?;
        Some(self.cockpit_cursor_ray(cam, (s.config.width, s.config.height)))
    }

    /// A switch held with the mouse: OMSI runs its `<event>_drag` trigger every frame the
    /// button is down, with this frame's movement in `mouse_x` / `mouse_y` - 0 while the
    /// hand keeps still. The scripts rely on that: the EN92 cash desk takes its swing speed
    /// from the last two positions, and fired only on movement it kept the speed of the last
    /// small move through a pause and swung shut when let go; the door scripts set their
    /// push once per trigger.
    ///
    /// The redraw path may inline this after `Player::tick` (field borrow of `player`); keep
    /// the helper for any call site that does not already hold `self.player`.
    #[allow(dead_code)] // inlined in `app_events` redraw while `player` is borrowed
    pub(crate) fn drag_frame(&mut self) {
        if !self.input.dragging {
            return;
        }
        let (dx, dy) = std::mem::take(&mut self.input.drag_delta);
        if let Some((map_id, ref ev)) = self.input.pressed_scenery_object {
            if let Some(w) = self.world.as_ref() {
                w.scenery_object_drag(&mut self.session.scenery, map_id, ev, dx, dy);
            }
        } else if let Some(p) = self.player.as_mut() {
            p.drag(dx, dy);
        }
    }

    /// On foot, the own bus is within reach: inside it, or standing by it (a hand's reach
    /// round its body; a click still has to hit one of its meshes).
    pub(crate) fn foot_reaches_bus(&self) -> bool {
        if self.foot_bus() == Some(crate::humans::BusId::Player) {
            return true;
        }
        match (self.player.as_ref(), self.camera.as_ref()) {
            // (every part of an articulated bus: a door button of the rear section is in
            // reach standing by that section, however far the front one is - #715)
            (Some(p), Some(c)) => {
                let v = &p.vehicle;
                std::iter::once((v.position, v.heading, v.ty.def.bounding_box))
                    .chain(v.trailers.iter().map(|t| (t.position, t.heading, t.ty.def.bounding_box)))
                    .any(|(at, heading, bb)| part_in_reach(c.position, at, heading, bb))
            }
            _ => false,
        }
    }
}

/// How far (m) a click reaches a page (`[htmltexture]`) on a scenery object.
const HTML_OBJECT_REACH: f32 = 4.0;

/// Degrees the view turns per (logical) pixel of the cursor's way while looking round:
/// Omsi.exe's fov / 78.75 (TForm_main.Panel1MouseMove 0x82c5f8).
impl App {
    /// A finger dragged over the photo (physical pixels): the photo camera turns.
    pub(crate) fn photo_look_touch(&mut self, dx: f32, dy: f32) {
        let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0).max(0.1);
        let k = self.photo.as_ref().map(|ph| look_deg_per_px(ph.cam.fov_deg)).unwrap_or(0.0) * self.settings.look_sens * 1.6;
        if let Some(ph) = self.photo.as_mut() {
            ph.look(dx / scale * k, dy / scale * k);
        }
    }
}

fn look_deg_per_px(fov_deg: f32) -> f32 {
    fov_deg / 78.75
}

#[cfg(test)]
mod look_tests {
    #[test]
    fn a_cursor_way_of_78_75_px_turns_by_the_field_of_view() {
        assert!((78.75 * super::look_deg_per_px(60.0) - 60.0).abs() < 1e-4);
    }
}

/// A part of a vehicle (its origin, heading in degrees and `[boundingbox]`) is in reach of
/// a person standing at `eye`: within 3 m of the box's half length round its centre.
fn part_in_reach(eye: glam::DVec3, at: glam::DVec3, heading: f64, bb: Option<[f32; 6]>) -> bool {
    let bb = bb.unwrap_or([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
    let h = heading.to_radians();
    let (fwd, right) = (glam::DVec2::new(h.sin(), h.cos()), glam::DVec2::new(h.cos(), -h.sin()));
    let centre = at + (right * bb[3] as f64 + fwd * bb[4] as f64).extend(bb[5] as f64);
    let reach = (bb[0].max(bb[1]) as f64) * 0.5 + 3.0;
    (eye - centre).length() < reach
}

#[cfg(test)]
mod reach_tests {
    use super::part_in_reach;
    use glam::DVec3;

    /// Standing by the rear section of an articulated bus, 17 m behind the front part's
    /// origin: out of the front part's reach, in the rear one's.
    #[test]
    fn the_rear_section_is_reached_by_its_own_box() {
        let front = [2.5, 11.0, 3.0, 0.0, -3.0, 1.5];
        let rear = [2.5, 7.0, 3.0, 0.0, -3.5, 1.5];
        let eye = DVec3::new(2.0, -17.0, 1.7);
        assert!(!part_in_reach(eye, DVec3::ZERO, 0.0, Some(front)));
        assert!(part_in_reach(eye, DVec3::new(0.0, -12.0, 0.0), 0.0, Some(rear)));
        // (the box's centre turns with the part: heading 180, the rear is ahead)
        assert!(part_in_reach(DVec3::new(-2.0, 17.0, 1.7), DVec3::new(0.0, 12.0, 0.0), 180.0, Some(rear)));
    }
}

impl App {
    /// A key pressed while a MileZero song request is being typed: true when it was taken.
    pub(crate) fn milezero_typing(&mut self, event: &winit::event::KeyEvent) -> bool {
        use winit::keyboard::{Key, NamedKey};
        let mz = &mut self.sound.milezero;
        if event.state != ElementState::Pressed || mz.typing.is_none() {
            return false;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => mz.submit(),
            Key::Named(NamedKey::Escape) => mz.typing = None,
            Key::Named(NamedKey::Backspace) => {
                if let Some(t) = mz.typing.as_mut() {
                    t.pop();
                }
            }
            _ => {
                if let (Some(t), Some(text)) = (mz.typing.as_mut(), event.text.as_deref()) {
                    if t.chars().count() < 200 {
                        t.extend(text.chars().filter(|c| !c.is_control()));
                    }
                }
            }
        }
        true
    }
}

#[cfg(all(test, windows))]
mod vr_drag_tests {
    use super::*;
    use clap::Parser;

    fn app() -> App {
        crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default())
    }

    #[test]
    fn headset_mouse_moves_reach_the_held_control() {
        let mut app = app();
        app.input.cursor = (400.0, 300.0);
        app.input.dragging = true;
        app.xr.vr_cursor_physical = Some((100.0, 100.0));
        app.on_vr_cursor_moved(102.0, 97.0);
        assert_eq!(app.input.cursor, (402.0, 297.0));
        assert_eq!(app.input.drag_delta, (2.0, -3.0));
        // Multiple moves before the frame are accumulated, never replaced.
        app.on_vr_cursor_moved(103.0, 98.0);
        assert_eq!(app.input.drag_delta, (3.0, -2.0));
    }

    #[test]
    fn cursor_warp_does_not_turn_the_control_or_move_the_virtual_pointer() {
        let mut app = app();
        app.input.cursor = (400.0, 300.0);
        app.input.dragging = true;
        app.xr.vr_cursor_physical = Some((990.0, 700.0));
        app.xr.vr_cursor_warp_pending = Some((500.0, 400.0));
        app.on_vr_cursor_moved(500.0, 400.0);
        assert_eq!(app.input.cursor, (400.0, 300.0));
        assert_eq!(app.input.drag_delta, (0.0, 0.0));
        app.on_vr_cursor_moved(503.0, 398.0);
        assert_eq!(app.input.drag_delta, (3.0, -2.0));
        app.input.dragging = false;
        app.on_vr_cursor_moved(506.0, 399.0);
        assert_eq!(app.input.drag_delta, (3.0, -2.0));
    }
}
