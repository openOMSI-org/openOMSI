//! The window's events: winit's `ApplicationHandler` for `App`.

/// Mirror pictures drawn per second at most, all mirrors together (see the redraw).
const MIRROR_RATE: f32 = 75.0;
/// The least a mirror is redrawn a second (see the mirrors in `window_event`).
const MIRROR_MIN_HZ: f32 = 8.0;
/// The most a mirror in the picture is redrawn a second, with the real-time reflections
/// economical (`mirror_refresh=eco`) and full (the default).
const MIRROR_MAX_HZ_ECO: f32 = 15.0;
const MIRROR_MAX_HZ_FULL: f32 = 30.0;
/// With no real-time reflections (`mirror_refresh=off`) a bus's mirrors are drawn once when
/// it is taken over and once more this many seconds later.
const MIRROR_FREEZE_REDRAW: f32 = 2.0;

/// Consume the VR redraw budget without updating a mirror twice in one frame.
/// Negative rates request every mirror each frame; zero freezes immediately.
fn vr_mirror_updates(budget: &mut f32, dt: f32, rate: f32, mirrors: usize) -> usize {
    if mirrors == 0 || rate == 0.0 {
        *budget = 0.0;
        return 0;
    }
    if rate < 0.0 {
        *budget = 0.0;
        return mirrors;
    }
    // Keep only a frame's worth of work after a stall, with the fractional
    // credit carried forward for rates below the game's frame rate.
    *budget = (*budget + dt.clamp(0.0, 0.1) * rate).min(mirrors as f32 + 0.5);
    let updates = (budget.floor() as usize).min(mirrors);
    *budget -= updates as f32;
    updates
}

/// The window's mirror redraws this frame, `rate` pictures a second (at least `min_rate`)
/// and at most `most` a frame: a steady count. With the credit spent as it came, frames
/// drew 0, 1 or 2 mirrors (each a few milliseconds) as the frame times wobbled, and two
/// after a slow frame. Now the count follows the frame time averaged over half a second:
/// as many a frame as the rate asks for (rounded, every frame alike), or with fewer than
/// one a frame due, one every so many frames, the fraction carried on and never two at
/// once. `avg_dt`: the average kept between frames (0 at first).
fn mirror_updates(budget: &mut f32, avg_dt: &mut f32, dt: f32, rate: f32, min_rate: f32, most: usize) -> usize {
    let dt = dt.clamp(0.0, 0.1);
    *avg_dt = if *avg_dt <= 0.0 { dt } else { *avg_dt + (dt - *avg_dt) * (dt / 0.5).min(1.0) };
    let per_frame = rate.max(0.0) * *avg_dt;
    if most == 0 || per_frame <= 0.0 {
        *budget = 0.0;
        return 0;
    }
    if per_frame >= 1.0 {
        *budget = 0.0;
        let least = (min_rate.min(rate) * *avg_dt - 0.05).ceil().max(1.0) as usize;
        return (per_frame.round() as usize).max(least).clamp(1, most);
    }
    *budget = (*budget + per_frame).min(1.0 + per_frame);
    if *budget >= 1.0 {
        *budget -= 1.0;
        1
    } else {
        0
    }
}

fn render_scale_step(fps: f32, slow_frame_wait_share: f32) -> f32 {
    // (three levels, far apart, and a wide band between going down and up again: every
    // step makes the picture's targets anew - hundreds of MB with MSAA and HDR - and a
    // scale that went up and down by 5 % every two seconds stuttered at each change and
    // filled the card's memory with the old ones until the driver gave up)
    if fps < 40.0 && slow_frame_wait_share >= 0.4 {
        -0.15
    } else if fps > 58.0 || slow_frame_wait_share < 0.2 {
        0.15
    } else {
        0.0
    }
}

use super::*;

mod frame;
pub(crate) use frame::steps;
pub(crate) use frame::stages::WINDOW_FRAME;
#[cfg(test)]
pub(crate) use frame::stages::{WindowStage, FRAME_AFTER, FRAME_BEFORE, FRAME_TICK};

/// How fast a stick turns the head, fully pushed (degrees a second, see `Analog::look`).
const LOOK_STICK_DEG_S: f32 = 120.0;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.resumed_impl(event_loop);
    }

    /// A phone put the app into the background: its window's surface goes (made again on
    /// `resumed`), the fingers and the held keys are let go.
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.gfx.surface = None;
        self.input.touch.drop_gpu();
        self.input_lost();
        self.save_last_situation();
        // (a phone's app in the background is often ended without `exiting`)
        omsi_render::pipeline_cache::save();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.process_permission_event(event_loop, &event) { return; }
        match event {
            WindowEvent::CloseRequested => {
                self.finish_vr_nav_edit();
                self.finish_session();
                crate::platform::exit(event_loop);
            }
            WindowEvent::Resized(size) => {
                if let (Some(s), Some(r)) = (self.gfx.surface.as_mut(), self.renderer.as_ref()) {
                    s.resize(r, size.width, size.height);
                }
                // (minimised, Windows makes the window 0 x 0)
                let hidden = size.width == 0 || size.height == 0;
                if hidden && !self.gfx.window_hidden {
                    self.input_lost();
                }
                self.gfx.window_hidden = hidden;
            }
            // (minimised or covered entirely, macOS and Wayland: the focus goes with it, and a
            // window merely covered by another may still be the one the player drives with)
            WindowEvent::Occluded(hidden) => self.gfx.window_hidden = hidden,
            WindowEvent::Focused(true) => {
                self.input.window_focused = true;
                if let Some(ctl) = self.input.controllers.as_mut() {
                    ctl.set_focus(true);
                }
                self.input_back();
            }
            WindowEvent::Focused(false) => {
                self.finish_vr_nav_edit();
                self.input.window_focused = false;
                if let Some(ctl) = self.input.controllers.as_mut() {
                    ctl.set_focus(self.settings.ctrl_background);
                }
                #[cfg(windows)]
                {
                    self.xr.vr_cursor_physical = None;
                    self.xr.vr_cursor_warp_pending = None;
                }
                // No key-up reaches us for whatever was held when focus left (alt-tab, a
                // click outside the window, an OS dialog popping up): without this, a held
                // modifier got "stuck" and made the next plain key press look like it was
                // held with that modifier - Shift got stuck this way once, and a plain `W`
                // (throttle in the wasd preset) was then read as Shift+W, OMSI's own wiper
                // key, toggling the wipers on every press instead of driving. The vehicle's
                // own keys, a door button, a switch held with the mouse and the mouse
                // steering's throttle went on as well, with the window minimised.
                self.input_lost();
            }
            // nothing the keyboard or the mouse does reaches the game while the window is
            // in the background (a wheel turned over a window behind another one zoomed;
            // the keys a system sends again for what is still held when the focus comes
            // back count only when pressed anew)
            WindowEvent::KeyboardInput { is_synthetic, ref event, .. } if self.input.input_away || (is_synthetic && event.state == ElementState::Pressed) => {}
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } if self.input.input_away => {}
            // a background test window driven by an input script takes no real mouse (the
            // person at the computer moving theirs over it clicked through the test)
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::CursorMoved { .. } if scripted_window() => {}
            WindowEvent::ModifiersChanged(modifiers) => {
                for code in input_script::release_inactive_modifiers(&mut self.input.keys, modifiers.state()) {
                    self.on_key(event_loop, code, false, false);
                }
            }
            // (a modifier's key-up that `ModifiersChanged` let go of already: Windows tells
            // the new modifier state before the key-up itself, and the release ran twice)
            WindowEvent::KeyboardInput { ref event, .. }
                if event.state == ElementState::Released
                    && matches!(event.physical_key, PhysicalKey::Code(c) if input_script::is_modifier(c) && !self.input.keys.contains(&c)) => {}
            WindowEvent::KeyboardInput { event, .. } => {
                // a plugin's text field being typed into takes the keys pressed (their
                // releases go on, so that nothing held stays held)
                if event.state == ElementState::Pressed {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        if self.integrations.plugin_typing_key(code, event.text.as_deref()) || self.devtools_typing(code, event.text.as_deref()) {
                            return;
                        }
                        // the game's panels: the field typed into, Esc closing the one on top
                        if self.game_panel_key(code, event.text.as_deref()) {
                            return;
                        }
                    }
                }
                if event.state == ElementState::Pressed && self.menus.menu_edit_icao {
                    if let Some(text)=event.text.as_deref(){ self.icao_edit_text(text); }
                }
                if event.state == ElementState::Pressed && self.menus.menu_edit_number {
                    if let Some(text)=event.text.as_deref(){ self.number_edit_text(text); }
                }
                if event.state == ElementState::Pressed && self.menus.menu_edit_ident {
                    if let Some(text)=event.text.as_deref(){ self.ident_edit_text(text); }
                }
                // Route numbers are free display text in OMSI. Take the text produced by
                // the keyboard layout (rather than only the physical key) so '-', shifted
                // symbols and non-US layouts reach the destination display unchanged.
                if event.state == ElementState::Pressed
                    && self.menus.menu_edit.is_some()
                    && !self.menus.menu_edit_icao
                    && (self.menus.menu_edit_search || matches!(self.menus.list_kind, Some(crate::game_lists::ListKind::RouteNumbers)))
                {
                    if let Some(text) = event.text.as_deref() {
                        if text.chars().any(|c| !c.is_control()) {
                            self.route_edit_text(text);
                            return;
                        }
                    }
                }
                // a MileZero song request being typed takes the keyboard
                if self.milezero_typing(&event) { return; }
                // '/' opens the chat's input box wherever the keyboard has it (the key
                // itself is then swallowed by the chat) - but not Numpad ÷, OMSI's stock
                // front door key (keyboard.cfg `bus_doorfront0 181`)
                // (only while `chat_open` is on its own key: one the player moved it to is
                // the only one, #130)
                if event.state == ElementState::Pressed
                    && event.text.as_deref() == Some("/")
                    && event.physical_key != PhysicalKey::Code(KeyCode::NumpadDivide)
                    && self.input.game_keys.iter().any(|b| b.action.eq_ignore_ascii_case("chat_open") && b.scan_code == 53 && b.chord() == 0)
                    && self.net.lan.is_some()
                    && !lan::chat_open(&self.net.remotes)
                {
                    self.net.remotes.chat.open();
                    if let PhysicalKey::Code(code) = event.physical_key {
                        lan::chat_swallow(&mut self.net.remotes, crate::app_impl::lan_tick::chat_key_of(code));
                    }
                    return;
                }
                // what is typed into an open LAN chat line (the key itself goes on to on_key)
                if let (Some(text), true, true) = (
                    event.text.as_deref(),
                    event.state == ElementState::Pressed,
                    lan::chat_open(&self.net.remotes),
                ) {
                    lan::chat_type(&mut self.net.remotes, text);
                }
                // (a release is never a repeat, and on Windows the second of Num4 and Left
                // held together is pressed, not repeated: `keys::key_repeat`, #1830)
                let pressed = event.state == ElementState::Pressed;
                let repeat = match event.physical_key {
                    PhysicalKey::Code(code) => crate::keys::key_repeat(code, pressed, event.repeat, cfg!(windows), |k| self.input.keys.contains(&k)),
                    _ => event.repeat && pressed,
                };
                // (the Lua plugins' `key` event; a key held down repeats nothing)
                if let (PhysicalKey::Code(code), false) = (event.physical_key, repeat) {
                    if self.integrations.plugin_keys.len() < 64 {
                        self.integrations.plugin_keys.push((format!("{code:?}"), pressed));
                    }
                }
                // a phone's back key is Escape (the game menu, out of the city map ...)
                let physical = match event.physical_key {
                    PhysicalKey::Code(KeyCode::BrowserBack) => PhysicalKey::Code(KeyCode::Escape),
                    k => k,
                };
                if let PhysicalKey::Code(code) = physical {
                    self.on_key(event_loop, code, pressed, repeat);
                }
            }
            // In VR right-click zooms; with mouse steering it first releases the steering.
            // On the desktop a right-drag zooms, as in OMSI (`on_right`).
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Right,
                ..
            } => {
                if let Some(edit) = self.xr.vr_nav_edit.as_mut() {
                    edit.rotating = state == ElementState::Pressed;
                    return;
                }
                if self.menus.navigator.as_ref().map(|n| n.map_open()).unwrap_or(false) {
                    return;
                }
                if self.vr_active() {
                    #[cfg(windows)]
                    if state == ElementState::Pressed && self.menus.game_menu.is_none()
                        && self.menus.chooser.is_none() {
                        if self.input.mouse_drive {
                            self.set_mouse_drive(false);
                            self.service_msg = Some(("Mouse steering off".into(), 3.0));
                        } else {
                            self.xr.vr_zoom_active = !self.xr.vr_zoom_active;
                        }
                    }
                } else {
                    self.on_right(state == ElementState::Pressed);
                }
            }
            // the mouse's side buttons (Mouse 4 and 5): keys of their own for keyboard.cfg (#2281)
            WindowEvent::MouseInput { state, button: button @ (winit::event::MouseButton::Back | winit::event::MouseButton::Forward), .. } => {
                let scan = if button == winit::event::MouseButton::Back { crate::keys::MOUSE_4 } else { crate::keys::MOUSE_5 };
                self.mouse_side_button(scan, state == ElementState::Pressed);
            }
            // (the middle button - the wheel pressed - turns the view as well: OMSI's pan)
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Middle,
                ..
            } => {
                if self.xr.vr_nav_edit.is_some() { return; }
                if self.menus.navigator.as_ref().map(|n| n.map_open()).unwrap_or(false) {
                    return;
                }
                self.input.mouse_look = state == ElementState::Pressed;
                self.input.mmb_held = state == ElementState::Pressed;
                self.sync_look_hold();
                self.sync_mouse_grab();
                self.update_hover();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.wheel(amount);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.xr.vr_nav_edit.is_some() { return; }
                // (both physical pixels)
                if let Some((x, y)) = self.input.cursor_hidden {
                    if (position.x as f32 - x).abs() + (position.y as f32 - y).abs() > 8.0 {
                        self.input.cursor_hidden = None;
                        if let Some(win) = self.window.as_ref() {
                            win.set_cursor_visible(true);
                        }
                    }
                }
                // (the on-screen controls on a computer, `OMSI_TOUCH=1`: the mouse is a
                // finger on them - from #202)
                if self.input.touch.enabled {
                    self.finger_move(0, glam::Vec2::new(position.x as f32, position.y as f32));
                }
                #[cfg(windows)]
                let vr_cockpit = self.xr.vr.is_some() && self.menus.game_menu.is_none()
                    && matches!(self.view.as_str(), "driver" | "pax");
                #[cfg(not(windows))]
                let vr_cockpit = false;
                if vr_cockpit && !self.input.mouse_look && !self.input.mouse_drive {
                    #[cfg(windows)]
                    self.on_vr_cursor_moved(position.x as f32, position.y as f32);
                } else {
                    self.on_mouse_moved(position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                // while the plugins' panels have the mouse its clicks are theirs, none the bus's
                if self.integrations.plugin_focus() {
                    self.plugin_click(state == ElementState::Pressed);
                    return;
                }
                self.alt_tap_spoilt();
                // ... and the game's own panels' (the quick menu, the forms, the editor)
                if self.game_panel_click(state == ElementState::Pressed) {
                    return;
                }
                // ... and those on a plugin's panel that takes them without the mouse (`grab`:
                // a card clicked away or kept), with the release - under the game's own
                // panels, as the plugins' panels are drawn
                if self.plugin_grab(state == ElementState::Pressed) {
                    return;
                }
                if self.input.touch.enabled {
                    let p = glam::Vec2::new(self.input.cursor.0, self.input.cursor.1);
                    if state == ElementState::Pressed {
                        self.finger_down(event_loop, 0, p);
                    } else {
                        self.finger_up(event_loop, 0, p, false);
                    }
                } else {
                    let pressed = state == ElementState::Pressed;
                    self.input.buttons_held.0 = pressed;
                    // the right button already down (looking round): both held zoom, and
                    // the click works nothing in the cab
                    if pressed && self.input.buttons_held.1 && self.start_both_drag() {
                        return;
                    }
                    // (the right button still held goes on zooming by itself, unless with
                    // `[altView]` it turns the view)
                    if !pressed && self.input.both_drag.is_some() && !(self.input.buttons_held.1 && self.right_zooms()) {
                        self.input.both_drag = None;
                        self.input.mouse_look = self.input.buttons_held.1;
                        self.sync_look_hold();
                        self.update_hover();
                    }
                    self.left_button(event_loop, pressed);
                }
            }
            // a finger (a phone; see touch.rs)
            WindowEvent::Touch(t) => self.on_touch(event_loop, t),
            // (while the renderer is still being built, the window's start goes on instead)
            WindowEvent::RedrawRequested => {
                if !self.drive_window_start(event_loop) {
                    self.frame(event_loop);
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if matches!(&event, DeviceEvent::Added | DeviceEvent::Removed) {
            if let Some(controllers) = self.input.controllers.as_ref() {
                controllers.refresh_devices();
            }
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            if scripted_window() {
                return;
            }
            if self.xr.vr_nav_edit.is_some() {
                if self.input.window_focused { self.vr_nav_drag(delta.0 as f32, delta.1 as f32); }
                return;
            }
            // (the cursor locked for looking round: the mouse's own movement turns the view
            // of the bus and the photo camera, in points on macOS - logical pixels)
            if self.input.window_focused && self.look_raw(delta.0 as f32, delta.1 as f32) {
                return;
            }
            // (in a view of the bus the cursor's own way turns it: move_cursor)
            if self.input.mouse_look {
                if !self.cursor_looks() {
                    if self.view == "outside" {
                        // F3 chase orbits at its own gain, not the head's.
                        self.sync_view_look();
                        let (y, p) = crate::input_script::chase_orbit_step(
                            self.cam.look.0,
                            self.cam.look.1,
                            delta.0 as f32,
                            delta.1 as f32,
                        );
                        self.cam.look.0 = y;
                        self.cam.look.1 = p;
                    } else {
                        let k = 0.15 * self.settings.look_sens;
                        self.look_by(delta.0 as f32 * k, delta.1 as f32 * k);
                    }
                }
            } else if self.input.mouse_grab.mode == Some(crate::app_impl::GrabMode::Locked) {
                // (the locked cursor's raw movement, in points on macOS: window pixels)
                let s = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
                self.steer_by(delta.0 as f32 * s, delta.1 as f32 * s);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // the server sent us away (kick, ban): the game ends, the launcher says why
        if self.net.lan.as_ref().and_then(crate::lan::turned_away).is_some() {
            self.finish_session();
            crate::platform::exit(event_loop);
            return;
        }
        crate::game_lists::flush_settings(false);
        // The window hidden until its first picture goes on from here: Windows sends no paint
        // message to a hidden window, so a redraw asked for never came and it was never shown.
        if self.starts_hidden() {
            self.drive_window_start(event_loop);
            return;
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The only user event: a quit signal arrived (see quit.rs).
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        if let Some(sig) = quit::requested() {
            log::info!("{} received: ending the session", quit::signal_name(sig));
            self.finish_session();
            crate::platform::exit(event_loop);
        }
    }

    /// Every way out ends here (Escape, the window's close button, Cmd+Q, --exit-after, a
    /// quit signal): the session is written and the LAN peers hear that we left, before
    /// anything else is torn down (Cmd+Q ends the process without returning from the loop).
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // (OMSI_PROFILE: the summary when the player closes the game, not only after --exit-after)
        self.log_profile_summary();
        crate::game_lists::flush_settings(true);
        self.abort_optimiser();
        self.finish_session();
        // (with the pipelines made since the start: puddles, Enhanced+)
        omsi_render::pipeline_cache::save();
        // ("playing now" ends with the game)
        self.integrations.presence = None;
        if let Some(lan) = self.net.lan.take() {
            // dropping the session says goodbye (BYE) to the host or the players
            drop(lan);
            log::info!("LAN: left the session");
        }
        // the tunnel's cloudflared and the WebSocket gateway go with the game (kept in a
        // static, which Rust never drops: cloudflared outlived every session, holding the
        // port and a public tunnel open)
        crate::lan::close_public_gateway();
        // and the launcher's LAN status file goes (Cmd+Q never returns to main's guard)
        drop(lan::StatusFileGuard);
        log::info!("game ends");
    }
}

impl App {
    /// The window's size in pixels, while the mirror panels can be worked (in the cab, no menu).
    pub(crate) fn mirror_hud_size(&self) -> Option<(f32, f32)> {
        mirror_hud_size(&self.settings, &self.cam, &self.menus, &self.gfx)
    }

    /// The mouse wheel (or a pinch of two fingers): `amount` notches, up positive.
    pub(crate) fn wheel(&mut self, amount: f32) {
        if self.has_process_permission_prompt() { self.shell.wheel(amount); return; }
        let sim = self.player_to_drawn_pose();
        self.wheel_now(amount);
        self.player_back_to_sim_pose(sim);
    }

    fn wheel_now(&mut self, amount: f32) {
        // the photo mode: the lens zooms, or its panel scrolls under the mouse
        if self.photo_on() {
            if self.shell.over_ui {
                self.shell.wheel(amount);
            } else if let Some(ph) = self.photo.as_mut() {
                ph.zoom(amount);
            }
            return;
        }
        if self.xr.vr_nav_edit.is_some() { self.vr_nav_scroll(amount); return; }
        if self.devtools_wheel(amount) {
            return;
        }
        // over a mirror panel the wheel resizes it (Shift: wider or narrower)
        if let Some(size) = self.mirror_hud_size() {
            let shift =
                self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight);
            if self
                .gfx.mirror_hud
                .wheel(amount, shift, self.hud_cursor(), size)
            {
                return;
            }
        }
        // the object editor: the wheel turns (Shift: lifts) the object
        if self.menus.game_menu.is_none() && self.editor_wheel(amount) {
            return;
        }
        // placing a vehicle: the wheel turns it
        if self.menus.placing.is_some() && self.menus.game_menu.is_none() {
            self.placing_wheel(amount);
            return;
        }
        // the game menu and its lists scroll with the wheel
        if self.menus.game_menu.is_some() {
            self.menu_wheel(amount);
            return;
        }
        // the city map takes the wheel while it is open
        if let Some(n) = self.menus.navigator.as_mut().filter(|n| n.map_open()) {
            n.map_wheel(amount, self.input.cursor.0, self.input.cursor.1);
            return;
        }
        // the wheel over the chat (or while typing) scrolls its history
        if let Some(ui) = self.ui.as_mut() {
            if self.net.lan.is_some() && (ui.chat.hovered || lan::chat_open(&self.net.remotes)) {
                // Ctrl + the wheel makes the chat larger or smaller (kept for the next game)
                if self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight) {
                    let to = ((self.settings.chat_size + amount.signum() * 0.1) * 10.0).round() / 10.0;
                    self.settings.chat_size = to.clamp(0.5, 3.0);
                    crate::game_lists::remember_setting("chat_size", &self.settings.chat_size.to_string());
                    return;
                }
                ui.chat.wheel(self.net.remotes.chat.lines.len(), amount);
                return;
            }
        }
        // The wheel over a cockpit switch turns it: the same <event>_drag the
        // original fires while the mouse is dragged, with the notch as the
        // movement. Knobs, the sun blind and the ignition key are far easier to
        // set that way than by holding the button down and moving the mouse.
        // (not while the wheel is zooming: the zoom moved the switches under the cursor, and
        // the wheel's next notch turned one instead of zooming on, #1224)
        let zooming = wheel_goes_on(self.input.wheel_zoomed_at, std::time::Instant::now());
        if self.menus.hover.is_some() && self.view != "free" && !zooming {
            let ray = self.camera.as_ref().zip(self.gfx.surface.as_ref())
                .map(|(cam, s)| self.cockpit_cursor_ray(cam, (s.config.width, s.config.height)));
            if let (Some(p), Some((o, d, spread))) = (
                self.player.as_mut(),
                ray,
            ) {
                p.occlude_controls = self.view == "outside";
                if p.pick(o, d, spread).is_some() {
                    // a notch is worth a good push of the mouse: the scripts divide
                    // the movement by 10 (the ignition key), 200 (the parking brake)
                    // or 500 (the driver's window), so a few pixels would do nothing
                    p.wheel(o, d, spread, -amount * 40.0);
                    return;
                }
            }
            if let (Some(w), Some((o, d, spread))) = (self.world.as_ref(), self.cursor_ray_now()) {
                let blocked = self.player.as_ref().and_then(|p| p.opaque_body_hit(o, d));
                if let Some(hit) = w.scenery_object_hit(o, d, crate::input_script::SCENERY_OBJECT_REACH, spread).filter(|h| blocked.map_or(true, |t| t >= h.t)) {
                    w.scenery_object_wheel(&mut self.session.scenery, hit.map_id, &hit.event, -amount * 40.0);
                    return;
                }
            }
        }
        let ctrl = self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight);
        if matches!(self.view.as_str(), "driver" | "pax") && self.player.is_some() {
            self.input.wheel_zoomed_at = Some(std::time::Instant::now());
        }
        if self.view == "outside" && self.player.is_some() && ctrl {
            // Ctrl+wheel: the outside camera stays where it is and narrows its field of view
            // (a telephoto; OMSI's own zoom there only moves the camera, as the wheel does)
            self.zoom_by(amount);
        } else if self.view == "outside" && self.player.is_some() {
            self.cam.orbit = (self.cam.orbit - amount * 1.5).clamp(ORBIT_MIN, ORBIT_MAX);
        } else if matches!(self.view.as_str(), "driver" | "pax") && self.player.is_some() {
            // inside the bus the wheel zooms, as in OMSI (the camera itself stays in the seat)
            self.zoom_by(amount);
        } else if matches!(self.view.as_str(), "free" | "foot") && !ctrl {
            // the free camera and on foot: the wheel zooms too (Ctrl+wheel moves the free
            // camera on, as the wheel alone did)
            self.zoom_by(amount);
        } else if let Some(cam) = self.camera.as_mut() {
            let f = cam.forward();
            cam.position += (f * amount * 4.0).as_dvec3();
        }
    }

    /// The left mouse button (or a finger's tap) where the cursor is.
    pub(crate) fn left_button(&mut self, event_loop: &ActiveEventLoop, pressed: bool) {
        if self.has_process_permission_prompt() { self.shell.button(pressed); return; }
        if let Some(edit) = self.xr.vr_nav_edit.as_mut() { edit.moving = pressed; return; }
        // the pause menu (and the photo mode) of the launcher's toolkit take the clicks
        if self.shell_takes_mouse() {
            if pressed {
                self.menus.menu_kbd = false;
            }
            self.shell.button(pressed);
            // (the left button dragged over the photo turns the photo camera)
            let over = self.shell.over_ui;
            let size = self.gfx.surface.as_ref().map(|s| (s.config.width as f32, s.config.height as f32)).unwrap_or((1.0, 1.0));
            let at = (self.input.cursor.0 / size.0.max(1.0), self.input.cursor.1 / size.1.max(1.0));
            let mut focus = None;
            if let Some(ph) = self.photo.as_mut() {
                ph.looking = pressed && !over;
                // (a click on the picture that does not turn the view focuses there)
                if pressed && !over {
                    ph.press = Some(at);
                    ph.dragged = 0.0;
                } else if !pressed {
                    if let Some(p) = ph.press.take() {
                        if ph.dragged < 0.3 {
                            focus = Some(p);
                        }
                    }
                }
            }
            if let Some((x, y)) = focus {
                self.photo_request(crate::photo::Request::FocusAt(x, y));
            }
            self.sync_look_hold();
            return;
        }
        // the developer tools' panel takes the clicks on it
        if self.devtools_click(pressed) {
            return;
        }
        // a mirror panel is dragged with the left button (a release always ends a drag)
        if let Some(size) = self
            .mirror_hud_size()
            .or_else(|| (!pressed).then(|| self.hud_size()))
        {
            if self.gfx.mirror_hud.press(pressed, self.hud_cursor(), size) {
                return;
            }
        }
        let state = if pressed { ElementState::Pressed } else { ElementState::Released };
        // placing a vehicle: a click sets it down
        if self.menus.placing.is_some() && self.menus.game_menu.is_none() {
            if state == ElementState::Pressed {
                self.placing_click();
            }
            return;
        }
        // the game menu takes the clicks while it is open
        if self.menus.game_menu.is_some() {
            if state == ElementState::Pressed {
                // (a tap or a click: only what is under the finger or the mouse is lit)
                self.menus.menu_kbd = false;
            }
            // Releasing the mouse button finishes scrollbar dragging.
            if state == ElementState::Released {
                self.menus.menu_drag = None;
                self.menus.dd_scroll_drag = None;
                self.menus.pane_scroll_drag = None;
                if self.menus.menu_scroll_drag {
                    self.menus.menu_scroll_drag = false;
                    self.menus.menu_top = self.menus.menu_top.map(f32::round);
                }
                return;
            }

            // an open drop-down takes the click: an entry is chosen, anywhere else closes it
            if self.menus.dropdown.is_some() {
                let inside = |r: &[f32; 4]| self.input.cursor.0 >= r[0] && self.input.cursor.0 <= r[2] && self.input.cursor.1 >= r[1] && self.input.cursor.1 <= r[3];
                let hit = self.ui.as_ref().and_then(|u| u.dd_rects.iter().position(|r| inside(r)).map(|i| i + u.dd_top));
                // its scroll bar is dragged (a press on the track beside the thumb takes the
                // thumb there by its middle); before, the press closed the list (#794)
                if let Some((track, thumb)) = self.ui.as_ref().and_then(|u| u.dd_scroll).filter(|_| hit.is_none()) {
                    let bar = [thumb[0], track[1], thumb[2], track[3]];
                    if inside(&bar) {
                        let grab = if inside(&thumb) { self.input.cursor.1 - thumb[1] } else { (thumb[3] - thumb[1]) * 0.5 };
                        self.menus.dd_scroll_drag = Some(grab);
                        self.drag_dropdown(self.input.cursor.1);
                        return;
                    }
                }
                match hit {
                    Some(i) => self.dropdown_pick(i),
                    None => self.menus.dropdown = None,
                }
                return;
            }

            // Pressing the mouse button on the scrollbar thumb starts dragging.
            if state == ElementState::Pressed {
                if let Some(thumb) = self
                    .ui
                    .as_ref()
                    .and_then(|u| u.menu_scroll_thumb)
                {
                    if self.input.cursor.0 >= thumb[0]
                        && self.input.cursor.0 <= thumb[2]
                        && self.input.cursor.1 >= thumb[1]
                        && self.input.cursor.1 <= thumb[3]
                    {
                        self.menus.menu_scroll_drag = true;
                        return;
                    }
                }

                // The sidebar of a settings window: a page, or the way back.
                if self.menus.chooser.is_some() {
                    let side = self.ui.as_ref().and_then(|u| {
                        u.menu_side.iter().position(|r| {
                            self.input.cursor.0 >= r[0]
                                && self.input.cursor.0 <= r[2]
                                && self.input.cursor.1 >= r[1]
                                && self.input.cursor.1 <= r[3]
                        })
                    });
                    if let Some(i) = side {
                        self.settings_side_click(i);
                        return;
                    }
                }

                // The timetable beside a line's tours: a stop to start from, or the button.
                if self.menus.chooser.is_some() {
                    // its scroll bar is dragged (a press on the track beside the thumb takes
                    // the thumb there by its middle)
                    if let Some((track, thumb, _, _)) = self.ui.as_ref().and_then(|u| u.menu_pane_scroll) {
                        let inside = |r: &[f32; 4]| self.input.cursor.0 >= r[0] && self.input.cursor.0 <= r[2] && self.input.cursor.1 >= r[1] && self.input.cursor.1 <= r[3];
                        if inside(&[thumb[0], track[1], thumb[2], track[3]]) {
                            let grab = if inside(&thumb) { self.input.cursor.1 - thumb[1] } else { (thumb[3] - thumb[1]) * 0.5 };
                            self.menus.pane_scroll_drag = Some(grab);
                            self.drag_pane(self.input.cursor.1);
                            return;
                        }
                    }
                    let pane = self.ui.as_ref().and_then(|u| {
                        let inside = |r: &[f32; 4]| self.input.cursor.0 >= r[0] && self.input.cursor.0 <= r[2] && self.input.cursor.1 >= r[1] && self.input.cursor.1 <= r[3];
                        if u.menu_pane_go.as_ref().is_some_and(inside) {
                            return Some(usize::MAX);
                        }
                        if let Some(j) = u.menu_time.iter().position(inside) {
                            return Some(usize::MAX - 1 - j);
                        }
                        u.menu_pane.iter().position(inside).map(|i| i + u.menu_pane_start)
                    });
                    if let Some(i) = pane {
                        self.tour_pane_click(i);
                        return;
                    }
                }

                // Otherwise check whether a menu row was clicked.
                let hit = self.ui.as_ref().and_then(|u| {
                    u.menu_rects.iter().position(|r| {
                        self.input.cursor.0 >= r[0]
                            && self.input.cursor.0 <= r[2]
                            && self.input.cursor.1 >= r[1]
                            && self.input.cursor.1 <= r[3]
                    })
                });

                if let Some(row) = hit {
                    // (a click on a slider or a stepper sets the value there)
                    let k = row
                        + self
                        .ui
                        .as_ref()
                        .map(|u| u.menu_start)
                        .unwrap_or(0);
                    let ctl = self.ui.as_ref().and_then(|u| u.menu_ctl.get(row).copied().flatten());

                    // (a greyed-out line cannot be clicked)
                    if self.menu_item_off(k) {
                        return;
                    }

                    if let Some(c) = ctl {
                        if self.menus.chooser.is_some() && self.input.cursor.0 >= c[0] && self.input.cursor.0 <= c[2] {
                            let fx = ((self.input.cursor.0 - c[0]) / (c[2] - c[0]).max(1.0)).clamp(0.0, 1.0);
                            self.menus.chooser = Some(k);
                            // (a slider is held: it follows the cursor till the button is let go)
                            if self.list_click(k, fx) {
                                self.menus.menu_drag = Some(k);
                            }
                            return;
                        }
                    }

                    if self.menus.chooser.is_none() {
                        self.menus.game_menu = Some(k);
                    }

                    // (a click on a tour shows its stops: the trip starts with the button)
                    if matches!(self.menus.list_kind, Some(crate::game_lists::ListKind::Tours(..))) && crate::game_lists::tour_at(self, k).is_some() {
                        self.menus.chooser = Some(k);
                        if let Some(crate::game_lists::ListKind::Tours(line, _)) = self.menus.list_kind.clone() {
                            self.menus.list_kind = Some(crate::game_lists::ListKind::Tours(line, None));
                        }
                        return;
                    }

                    // on the arrows round a line's value: one step down or up; elsewhere on
                    // the line as Enter
                    let arrows = self.ui.as_ref().and_then(|u| u.menu_arrows.get(row).copied().flatten());
                    match arrows {
                        Some([from, to, _]) if self.input.cursor.0 >= from && self.input.cursor.0 < to => self.chooser_adjust(k, "-"),
                        Some([_, _, plus]) if self.input.cursor.0 >= plus => self.chooser_adjust(k, "+"),
                        _ => self.menu_choose(event_loop, k),
                    }
                }
            }

            return;
        }
        self.on_left(state == ElementState::Pressed)
    }
}

/// OMSI's timetable window: the current trip's stops with their times, the ones served
/// greyed, the next one marked.
fn timetable_rows(duty: Option<&crate::schedule::PlayerDuty>, delay: Option<f64>) -> Option<crate::ui::TimetableView> {
    let d = duty?;
    let trip = d.trips.get(d.trip_index)?;
    let hm = |t: f64| format!("{:02}:{:02}", ((t / 3600.0) as i64).rem_euclid(24), ((t % 3600.0) / 60.0) as i64);
    let line = if trip.line.trim().is_empty() { d.line.trim() } else { trip.line.trim() };
    let meta = format!("{} {}  -  {} {}/{}", omsi_ui::tr("Tour"), d.tour.trim(), omsi_ui::tr("Trip"), d.trip_index + 1, d.trips.len());
    // (the first stop is left, the last arrived at; the others both, as the launcher's trips)
    let stops: Vec<(usize, &_)> = trip.stops.iter().enumerate().filter(|(_, s)| s.stops).collect();
    let (first, last) = (stops.first().map(|s| s.0), stops.last().map(|s| s.0));
    let rows = stops
        .iter()
        .map(|&(k, s)| crate::ui::TimetableRow {
            name: s.name.trim().to_string(),
            arr: if Some(k) == first { String::new() } else { hm(s.arr) },
            dep: if Some(k) == last { String::new() } else { hm(s.dep) },
            expected: if k < d.next_stop { String::new() } else { hm((if Some(k) == first { s.dep } else { s.arr } + delay.unwrap_or(0.0)).rem_euclid(86400.0)) },
            state: if k < d.next_stop { 0 } else if k == d.next_stop { 1 } else { 2 },
        })
        .collect();
    // the trip after this one
    let next = d.trips.get(d.trip_index + 1).map(|n| (format!("{} {}", if n.line.trim().is_empty() { d.line.trim() } else { n.line.trim() }, n.terminus.trim()), hm(n.departure)));
    Some(crate::ui::TimetableView { line: line.to_string(), terminus: trip.terminus.trim().to_string(), meta, delay: delay.unwrap_or(0.0), rows, next })
}

/// The outside air from the weather and the cabin air the vehicle scripts/engine maintain.
/// OMSI exposes both to every bus as Weather_Temperature and Cabinair_Temp.
pub(crate) fn vehicle_temperatures(p: &Player) -> (f32, f32) {
    let outside = p.vehicle.host.temperature;
    let inside = p
        .vehicle
        .var("Cabinair_Temp")
        .filter(|v| v.is_finite())
        .unwrap_or_else(|| outside.clamp(18.0, 25.0));
    (outside, inside)
}

#[derive(Default)]
struct InfoLineData {
    time: String,
    speed: Option<String>,
    speed_limit: Option<String>,
    stop_requested: bool,
    route: Option<(String, String)>,
    next_stop: Option<String>,
    delay: Option<String>,
    distance: Option<String>,
    odometer: Option<String>,
    temperatures: Option<(String, String)>,
    passengers: Option<(&'static str, String)>,
    fuel: Option<f32>,
}

fn info_bar(data: InfoLineData) -> crate::ui::InfoBar {
    use crate::ui::InfoItemStyle;
    let item = crate::ui::InfoItem::new;
    let mut info = crate::ui::InfoBar {
        primary: vec![item("Time", data.time, InfoItemStyle::Clock)],
        secondary: Vec::new(),
    };
    if let Some((line, terminus)) = data.route {
        let mut route = item("Route", terminus, InfoItemStyle::Route);
        route.badge = Some(line);
        info.primary.insert(0, route);
    }
    if let Some(value) = data.speed {
        info.primary.push(item("Speed", value, InfoItemStyle::Speed));
    }
    if let Some(value) = data.speed_limit {
        info.primary.push(item("Speed limit", value, InfoItemStyle::SpeedLimit));
    }
    if data.stop_requested {
        info.primary.push(item("Stop requested", String::new(), InfoItemStyle::Status));
    }
    if let Some(value) = data.next_stop {
        info.primary.push(item("Next stop", value, InfoItemStyle::NextStop));
    }
    if let Some(value) = data.delay {
        info.primary.push(item("Delay", value, InfoItemStyle::Delay));
    }
    if let Some(value) = data.distance {
        info.secondary.push(item("Distance", value, InfoItemStyle::Detail));
    }
    if let Some(value) = data.odometer {
        info.secondary.push(item("Odometer", value, InfoItemStyle::Detail));
    }
    if let Some((outside, cabin)) = data.temperatures {
        info.secondary.push(item("Outside / cabin", format!("{outside} / {cabin}"), InfoItemStyle::Detail));
    }
    if let Some((label, value)) = data.passengers {
        info.secondary.push(item(label, value, InfoItemStyle::Detail));
    }
    if let Some(progress) = data.fuel {
        let mut fuel = item("Fuel", format!("{:.0}%", progress * 100.0), InfoItemStyle::Fuel);
        fuel.progress = Some(progress);
        info.secondary.push(fuel);
    }
    info
}

/// The information bar's driving details and vehicle data, kept as translated labels and live values.
fn info_line(clock: &omsi_sim::SimClock, player: Option<&Player>, duty: Option<&crate::schedule::PlayerDuty>, passengers: Option<usize>, metres: f64, speed_limit: Option<f32>) -> crate::ui::InfoBar {
    let t = clock.time;
    let mut data = InfoLineData { time: format!("{:02}:{:02}:{:02}", ((t / 3600.0) as i64).rem_euclid(24), ((t % 3600.0) / 60.0) as i64, (t % 60.0) as i64), ..Default::default() };
    if let Some(p) = player {
        data.speed = Some(crate::units::Units::now().speed_text(p.vehicle.physics.velocity_kmh().abs()));
        // the road's speed limit, as the navigator's top bar has it (#2476: its sign there is small)
        data.speed_limit = speed_limit.map(|v| {
            let u = crate::units::Units::now();
            format!("{:.0}", u.speed_limit(v))
        });
        // a stop asked for, as OMSI's Shift+Z overlay says (#1767: seen from outside too)
        data.stop_requested = crate::navigator::stop_requested(&p.vehicle);
        data.distance = Some(distance_driven(metres));
        // the bus's whole mileage, as OMSI's Shift+Z overlay reads it ("Mileometer", the
        // `kmcounter_*` the cockpit shows, whole kilometres and the metres of the fraction)
        data.odometer = p.vehicle.var("kmcounter_km").filter(|v| v.is_finite()).map(|km| odometer_value(km as f64 + p.vehicle.var("kmcounter_m").unwrap_or(0.0) as f64 / 1000.0));
        let (outside, inside) = vehicle_temperatures(p);
        let u = crate::units::Units::now();
        data.temperatures = Some((u.temperature(outside), u.temperature(inside)));
        // how many are aboard right now, out of the bus's places (None: the passengers are
        // switched off for this drive, so there is nothing to count)
        data.passengers = passengers.map(|n| (if n == 1 { "Passenger" } else { "Passengers" }, passenger_count(n, passenger_capacity(&p.vehicle))));
        // the tank as the bus's script says it (OMSI's RL_TankContent: tank_percent)
        data.fuel = p.vehicle.var("tank_percent").filter(|v| v.is_finite()).map(|tank| tank.clamp(0.0, 1.0));
        if let Some(d) = duty {
            if let Some(trip) = d.trips.get(d.trip_index) {
                let line = if trip.line.trim().is_empty() { d.line.trim() } else { trip.line.trim() };
                data.route = Some((line.to_string(), trip.terminus.trim().to_string()));
                data.next_stop = trip.stops.get(d.next_stop).map(|s| s.name.trim().to_string());
                let delay = p.vehicle.host.tt_delay;
                data.delay = Some(format!("{}{}:{:02}", if delay < 0.0 { "-" } else { "+" }, (delay.abs() / 60.0) as i64, (delay.abs() % 60.0) as i64));
            }
        }
    }
    info_bar(data)
}

/// The kilometres driven this session (`Career::metres`), to a hundred metres.
fn distance_driven(metres: f64) -> String {
    crate::units::Units::now().long_distance(metres.max(0.0) / 1000.0, 1)
}

/// The passenger places (`[passpos]`) in the player's bus and any coupled sections: nobody
/// boards once they are all taken (None: no passenger cabin to count).
fn passenger_capacity(vehicle: &omsi_sim::VehicleInstance) -> Option<usize> {
    let sections =
        std::iter::once(&vehicle.ty).chain(vehicle.trailers.iter().map(|trailer| &trailer.ty));
    let mut capacity = 0;
    let mut found_cabin = false;
    for ty in sections {
        if let Some(cabin) = crate::driver::cabin_of(&ty.def) {
            capacity += cabin.pass_positions.len();
            found_cabin = true;
        }
    }
    found_cabin.then_some(capacity)
}

fn odometer_value(km: f64) -> String {
    crate::units::Units::now().long_distance(km.max(0.0), 1)
}

fn passenger_count(n: usize, capacity: Option<usize>) -> String {
    capacity.map_or_else(|| n.to_string(), |capacity| format!("{n}/{capacity}"))
}

#[cfg(test)]
mod governor_tests {
    use super::render_scale_step;

    #[test]
    fn cpu_stutters_do_not_reduce_picture_quality() {
        assert!(render_scale_step(35.0, 0.1) > 0.0);
        assert!(render_scale_step(35.0, 0.6) < 0.0);
        assert!(render_scale_step(60.0, 0.6) > 0.0);
    }
}

#[cfg(test)]
mod info_tests {
    use super::{distance_driven, odometer_value, passenger_count};

    #[test]
    fn the_distance_driven_is_written_in_kilometres() {
        assert_eq!(distance_driven(0.0), "0.0 km");
        assert_eq!(distance_driven(12_345.0), "12.3 km");
    }

    #[test]
    fn the_odometer_reads_kilometres_and_tenths() {
        assert_eq!(odometer_value(75_556.639), "75556.6 km");
    }

    #[test]
    fn passenger_count_stays_separate_from_its_localized_label() {
        assert_eq!(passenger_count(0, None), "0");
        assert_eq!(passenger_count(1, None), "1");
        assert_eq!(passenger_count(23, None), "23");
    }

    #[test]
    fn passenger_count_includes_the_bus_capacity() {
        assert_eq!(passenger_count(4, Some(65)), "4/65");
    }

    #[test]
    fn the_information_bar_separates_driving_details_from_vehicle_data() {
        let info = super::info_bar(super::InfoLineData {
            time: "09:00:00".into(),
            speed: Some("48 km/h".into()),
            speed_limit: Some("50".into()),
            stop_requested: true,
            route: Some(("76".into(), "Central".into())),
            next_stop: Some("Market".into()),
            delay: Some("+2:14".into()),
            distance: Some("12.3 km".into()),
            odometer: Some("75556.6 km".into()),
            temperatures: Some(("15 °C".into(), "20 °C".into())),
            passengers: Some(("Passengers", "4/65".into())),
            fuel: Some(0.8),
            ..Default::default()
        });
        assert_eq!(
            info.primary.iter().map(|item| (item.label, item.value.as_str())).collect::<Vec<_>>(),
            [
                ("Route", "Central"),
                ("Time", "09:00:00"),
                ("Speed", "48 km/h"),
                ("Speed limit", "50"),
                ("Stop requested", ""),
                ("Next stop", "Market"),
                ("Delay", "+2:14"),
            ]
        );
        assert_eq!(
            info.secondary.iter().map(|item| (item.label, item.value.as_str())).collect::<Vec<_>>(),
            [
                ("Distance", "12.3 km"),
                ("Odometer", "75556.6 km"),
                ("Outside / cabin", "15 °C / 20 °C"),
                ("Passengers", "4/65"),
                ("Fuel", "80%"),
            ]
        );
        assert_eq!(
            info.primary.iter().map(|item| item.style).collect::<Vec<_>>(),
            [
                crate::ui::InfoItemStyle::Route,
                crate::ui::InfoItemStyle::Clock,
                crate::ui::InfoItemStyle::Speed,
                crate::ui::InfoItemStyle::SpeedLimit,
                crate::ui::InfoItemStyle::Status,
                crate::ui::InfoItemStyle::NextStop,
                crate::ui::InfoItemStyle::Delay,
            ]
        );
        assert!(info.secondary.iter().take(4).all(|item| item.style == crate::ui::InfoItemStyle::Detail));
        assert_eq!(info.secondary[4].style, crate::ui::InfoItemStyle::Fuel);
        assert_eq!(info.primary[0].badge.as_deref(), Some("76"));
        assert_eq!(info.secondary[4].progress, Some(0.8));

        let bare = super::info_bar(super::InfoLineData { time: "09:00:00".into(), ..Default::default() });
        assert_eq!(bare.primary.iter().map(|item| item.label).collect::<Vec<_>>(), ["Time"]);
        assert!(bare.secondary.is_empty());
    }
}

#[cfg(test)]
mod vr_mirror_tests {
    use super::{mirror_updates, vr_mirror_updates};

    /// The window's mirrors: the same count every frame once the frame time has settled,
    /// never two after a slow frame below one a frame, and the rate kept.
    #[test]
    fn window_mirrors_take_a_steady_count() {
        for (fps, rate, want) in [(140.0f32, 75.0f32, None), (60.0, 75.0, Some(1)), (30.0, 75.0, Some(2)), (240.0, 75.0, None)] {
            let (mut budget, mut avg) = (1.0, 0.0);
            let mut total = 0;
            for n in 0..(fps as usize * 4) {
                // (frame times wobbling by a tenth)
                let dt = (1.0 + if n % 3 == 0 { 0.1 } else { -0.05 }) / fps;
                let k = mirror_updates(&mut budget, &mut avg, dt, rate, 40.0, 2);
                assert!(k <= 1 || want == Some(2), "fps {fps}: {k}");
                if let Some(w) = want.filter(|_| n > fps as usize) {
                    assert_eq!(k, w, "fps {fps} frame {n}");
                }
                total += k;
            }
            let per_s = total as f32 / 4.0;
            assert!(per_s >= 40.0 && per_s <= rate.max(fps) + 1.0, "fps {fps}: {per_s} a second");
            if fps > rate {
                assert!((per_s - rate).abs() < 2.0, "fps {fps}: {per_s} a second");
            }
        }
        // a slow frame below one a frame: one mirror, not a burst
        let (mut budget, mut avg) = (0.0, 1.0 / 140.0);
        assert!(mirror_updates(&mut budget, &mut avg, 0.1, 75.0, 40.0, 2) <= 2);
        assert_eq!(mirror_updates(&mut budget, &mut avg, 0.0, 0.0, 40.0, 2), 0);
    }

    #[test]
    fn every_frame_updates_all_mirrors_even_at_low_game_fps() {
        let mut budget = 0.75;
        for dt in [1.0 / 90.0, 1.0 / 30.0, 0.5] {
            assert_eq!(vr_mirror_updates(&mut budget, dt, -1.0, 8), 8);
            assert_eq!(budget, 0.0);
        }
    }

    #[test]
    fn a_high_budget_is_not_limited_to_two_mirrors_per_frame() {
        let mut budget = 0.0;
        assert_eq!(vr_mirror_updates(&mut budget, 1.0 / 60.0, 240.0, 4), 4);
        assert_eq!(vr_mirror_updates(&mut budget, 0.5, 360.0, 4), 4);
        assert!(budget <= 0.5);
    }

    #[test]
    fn fractional_credit_preserves_the_selected_total_rate() {
        for fps in [30, 60, 90] {
            let mut budget = 0.0;
            let updates: usize = (0..fps * 10).map(|_| vr_mirror_updates(&mut budget, 1.0 / fps as f32, 16.0, 4)).sum();
            assert!((159..=160).contains(&updates), "fps={fps}: {updates}");
        }
    }

    #[test]
    fn off_and_no_mirrors_discard_old_credit() {
        let mut budget = 2.5;
        assert_eq!(vr_mirror_updates(&mut budget, 0.1, 0.0, 4), 0);
        assert_eq!(budget, 0.0);
        assert_eq!(vr_mirror_updates(&mut budget, 0.1, -1.0, 0), 0);
        assert_eq!(vr_mirror_updates(&mut budget, 0.1, 360.0, 0), 0);
        assert_eq!(budget, 0.0);
    }
}

/// How long after a notch of the mouse wheel that zoomed the next one still counts as the same
/// turn of the wheel (a turn sends its notches some 20-100 ms apart).
const WHEEL_TURN: std::time::Duration = std::time::Duration::from_millis(350);

/// Whether a notch of the wheel at `now` goes on zooming, the last one having zoomed at
/// `zoomed` (#1224).
fn wheel_goes_on(zoomed: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    zoomed.is_some_and(|t| now.saturating_duration_since(t) < WHEEL_TURN)
}

#[cfg(test)]
mod wheel_turn_tests {
    /// A turn of the wheel that zooms goes on zooming over a switch; a notch a while later
    /// over the switch turns the switch (#1224).
    #[test]
    fn a_turn_of_the_wheel_zooms_on() {
        let t = std::time::Instant::now();
        let ms = std::time::Duration::from_millis;
        assert!(!super::wheel_goes_on(None, t));
        assert!(super::wheel_goes_on(Some(t), t + ms(80)));
        assert!(!super::wheel_goes_on(Some(t), t + ms(600)));
    }
}

/// A test window in the background driven by an input script (`OMSI_BACKGROUND` with
/// `OMSI_INPUT`): its mouse is the script's alone.
fn scripted_window() -> bool {
    omsi_cfg::flags::OMSI_BACKGROUND.is_set() && omsi_cfg::flags::OMSI_INPUT.var().is_some()
}

/// [`App::mirror_hud_size`] on the parts it reads.
pub(crate) fn mirror_hud_size(settings: &settings::Settings, cam: &ViewState, menus: &MenuState, gfx: &GfxState) -> Option<(f32, f32)> {
    if !cam.in_cab || menus.game_menu.is_some() || !gfx.mirror_hud.editing() {
        return None;
    }
    Some(crate::app_impl::hud_size(settings, gfx))
}
