//! The object editor (`crate::editor`) as the app works it: on and off, keys, mouse, wheel.

use super::*;

impl App {
    /// A key while the game menu is open.
    /// The object editor on or off; on, it starts with the free camera where the view is.
    pub(crate) fn toggle_editor(&mut self) {
        // (in a LAN session the host edits the map for everybody: its edits go to the
        // others' games, a client's would stay its own)
        if self.menus.editor.is_none() && self.net.lan.as_ref().map(|l| l.role == omsi_net::Role::Client).unwrap_or(false) {
            self.service_msg = Some(("In a LAN session only the host edits the map".into(), 3.0));
            return;
        }
        if self.menus.editor.take().is_some() {
            self.menus.editor_drag = false;
            self.service_msg = Some(("Object editor off (unsaved changes stay until the end of the session)".into(), 3.0));
            return;
        }
        let ed = crate::editor::Editor::default();
        let msg = self.world.as_ref().map(|w| ed.describe(w)).unwrap_or_default();
        self.menus.editor = Some(ed);
        let help = "click picks, drag moves, wheel turns (Shift: height), Delete, C copy, V variant, Backspace undo, PgUp/PgDn/F ground, [ ] brush, Ctrl+S save, Esc leave";
        self.service_msg = Some((omsi_ui::i18n::said(format!("{msg} - {}", omsi_ui::tr(help)), format!("{} - {help}", omsi_ui::i18n::english(&msg))), 10.0));
    }

    /// A key while the object editor is on; true when it was the editor's.
    pub(crate) fn editor_key(&mut self, code: KeyCode) -> bool {
        let shift = self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight);
        let ctrl = self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight);
        let Some(cam) = self.camera.as_ref() else { return false };
        let (eye, fwd, yaw) = (cam.position, cam.forward(), cam.yaw as f64);
        let Some(action) = action_for(code, shift, ctrl, yaw) else { return false };
        let Some(world) = self.world.clone() else { return false };
        let msg = match action {
            crate::editor::Action::Leave => {
                self.toggle_editor();
                return true;
            }
            crate::editor::Action::Pick => {
                let ed = self.menus.editor.as_mut().unwrap();
                ed.pick(&world, eye, fwd);
                ed.describe(&world)
            }
            crate::editor::Action::NextPick => {
                let ed = self.menus.editor.as_mut().unwrap();
                ed.next_pick();
                ed.describe(&world)
            }
            crate::editor::Action::Save => {
                let content = crate::startup::content_dir();
                let ed = self.menus.editor.as_ref().unwrap();
                match content.map(|c| ed.save(&world, &self.args.map, &c, &self.args.root)) {
                    Some(Ok(files)) if files.is_empty() => "Nothing to save".to_string(),
                    Some(Ok(files)) => format!("Saved {} tile(s) to the content folder ({})", files.len(), files.iter().filter_map(|f| f.file_name()).map(|n| n.to_string_lossy()).collect::<Vec<_>>().join(", ")),
                    Some(Err(e)) => format!("Not saved ({e})"),
                    None => "Not saved: no content folder".to_string(),
                }
            }
            a @ (crate::editor::Action::Ground(_) | crate::editor::Action::Flatten | crate::editor::Action::Brush(_)) => {
                let at = crate::editor::Editor::aim(&world, eye, fwd);
                let (msg, tiles) = self.menus.editor.as_mut().unwrap().ground(&world, at, &a);
                if !tiles.is_empty() {
                    log::info!("map editor: ground of tiles {tiles:?} at {at:?}");
                    world.forget_staged(&tiles);
                    if let Some(st) = self.gfx.streamer.as_mut() {
                        st.reload(Some(&tiles));
                    }
                }
                msg
            }
            a => {
                let (Some(r), Some(scene)) = (self.renderer.as_ref(), self.scene.as_mut()) else { return true };
                match self.menus.editor.as_mut().unwrap().apply(&world, r, scene, &a) {
                    Some(m) => m,
                    None => "Pick an object first (Enter)".to_string(),
                }
            }
        };
        log::info!("object editor: {msg}");
        self.service_msg = Some((msg, 5.0));
        self.editor_broadcast(false);
        true
    }

    /// The host's edits to the other players' games (`all`: every edit of the session,
    /// sent again every ten seconds for the ones who joined since).
    pub(crate) fn editor_broadcast(&mut self, all: bool) {
        let (Some(ed), Some(w)) = (self.menus.editor.as_ref(), self.world.as_ref()) else {
            if all {
                // (edits stay after the editor is left: sent from the world's list)
                if let (Some(w), Some(l)) = (self.world.as_ref(), self.net.lan.as_mut()) {
                    if l.role == omsi_net::Role::Host {
                        let lines = crate::editor::Editor::default().sync_lines(w, &self.args.root, true);
                        let ids: Vec<u32> = l.peers().map(|p| p.pose.id).filter(|id| *id != l.my_id).collect();
                        for line in &lines {
                            for id in &ids {
                                l.command(*id, line);
                            }
                        }
                    }
                }
            }
            return;
        };
        let Some(l) = self.net.lan.as_mut() else { return };
        if l.role != omsi_net::Role::Host {
            return;
        }
        let lines = ed.sync_lines(w, &self.args.root, all);
        let ids: Vec<u32> = l.peers().map(|p| p.pose.id).filter(|id| *id != l.my_id).collect();
        for line in &lines {
            if line.len() > omsi_net::MAX_CHAT {
                log::warn!("object editor: '{line}' is too long to send");
                continue;
            }
            for id in &ids {
                l.command(*id, line);
            }
        }
    }

    /// The mouse in the object editor: a click picks what is under the cursor (and starts
    /// dragging it), a drag moves it over the ground; true when the editor took it.
    pub(crate) fn editor_mouse(&mut self, pressed: bool) -> bool {
        if self.menus.editor.is_none() {
            return false;
        }
        if !pressed {
            if self.menus.editor_drag {
                self.menus.editor_drag = false;
                self.editor_broadcast(false);
            }
            return true;
        }
        let (Some(cam), Some(s), Some(world)) = (self.camera.as_ref(), self.gfx.surface.as_ref(), self.world.clone()) else { return true };
        let (o, d) = self.world_cursor_ray(cam, (s.config.width, s.config.height));
        let ed = self.menus.editor.as_mut().unwrap();
        // (the copy being edited stays the one dragged while it is under the cursor)
        let on_added = ed.editing_added.and_then(|k| ed.added.get(k)).map(|a| {
            let p = a.base + a.moved - o;
            let along = p.dot(d.as_dvec3());
            along > 0.0 && (p - d.as_dvec3() * along).length() < 2.5
        }).unwrap_or(false);
        if !on_added {
            ed.pick(&world, o, d);
        }
        self.menus.editor_drag = on_added || ed.selected.is_some();
        let msg = ed.describe(&world);
        self.service_msg = Some((msg, 5.0));
        true
    }

    /// The cursor moved while an object is dragged.
    pub(crate) fn editor_drag_frame(&mut self) {
        if !self.menus.editor_drag {
            return;
        }
        let (Some(cam), Some(s), Some(world)) = (self.camera.as_ref(), self.gfx.surface.as_ref(), self.world.clone()) else { return };
        let (o, d) = self.world_cursor_ray(cam, (s.config.width, s.config.height));
        let Some(hit) = crate::placing::ground_hit(&world, o, d.as_dvec3(), 400.0) else { return };
        let (Some(r), Some(scene), Some(ed)) = (self.renderer.as_ref(), self.scene.as_mut(), self.menus.editor.as_mut()) else { return };
        if let Some(m) = ed.drag_to(&world, r, scene, hit) {
            self.service_msg = Some((m, 3.0));
        }
    }

    /// The wheel in the object editor: the object turns (5° a notch), with Shift it rises.
    pub(crate) fn editor_wheel(&mut self, amount: f32) -> bool {
        if self.menus.editor.is_none() {
            return false;
        }
        let shift = self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight);
        let action = if shift { crate::editor::Action::Move(glam::DVec3::Z * 0.1 * amount as f64) } else { crate::editor::Action::Turn(5.0 * amount as f64) };
        let (Some(world), Some(r), Some(scene)) = (self.world.clone(), self.renderer.as_ref(), self.scene.as_mut()) else { return true };
        if let Some(m) = self.menus.editor.as_mut().unwrap().apply(&world, r, scene, &action) {
            self.service_msg = Some((m, 3.0));
            self.editor_broadcast(false);
        }
        true
    }
}

/// The editor's key for `code` (with Shift for the fine steps), as the camera faces `yaw`
/// (degrees clockwise from north).
pub fn action_for(code: winit::keyboard::KeyCode, shift: bool, ctrl: bool, yaw: f64) -> Option<crate::editor::Action> {
    use crate::editor::Action;
    use winit::keyboard::KeyCode as K;
    use glam::DVec3;
    let step = if shift { 0.05 } else { 0.5 };
    let turn = if shift { 0.5 } else { 5.0 };
    let (s, c) = yaw.to_radians().sin_cos();
    let fwd = DVec3::new(s, c, 0.0) * step;
    let right = DVec3::new(c, -s, 0.0) * step;
    Some(match code {
        K::Enter | K::NumpadEnter => Action::Pick,
        K::KeyC if !ctrl => Action::Copy,
        K::KeyV if !ctrl => Action::Variant,
        K::Tab => Action::NextPick,
        K::KeyI => Action::Move(fwd),
        K::KeyK => Action::Move(-fwd),
        K::KeyL => Action::Move(right),
        K::KeyJ => Action::Move(-right),
        K::KeyO => Action::Move(DVec3::Z * step),
        K::KeyU => Action::Move(-DVec3::Z * step),
        K::KeyM => Action::Turn(turn),
        K::KeyN => Action::Turn(-turn),
        K::Delete => Action::Delete,
        K::Backspace => Action::Undo,
        K::KeyS if ctrl => Action::Save,
        K::PageUp => Action::Ground(if shift { 0.05 } else { 0.25 }),
        K::PageDown => Action::Ground(if shift { -0.05 } else { -0.25 }),
        K::KeyF if !ctrl => Action::Flatten,
        K::BracketRight => Action::Brush(1.25),
        K::BracketLeft => Action::Brush(0.8),
        K::Escape => Action::Leave,
        _ => return None,
    })
}
