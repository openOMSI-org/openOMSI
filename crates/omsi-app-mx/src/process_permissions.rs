//! Trusted game UI for native helper requests. Plugin panels cannot approve these.
use omsi_plugin::api::process::{ProcessDecision, ProcessPrompt};
mod dialog;
use dialog::draw;
pub(crate) use dialog::DialogState;

#[cfg(test)]
mod tests;

impl crate::App {
    /// Modal input is handled before plugin panels, bus controls, photo mode or VR edits.
    pub(crate) fn process_permission_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: &winit::event::WindowEvent,
    ) -> bool {
        use winit::event::{ElementState, MouseButton, WindowEvent};
        if !self.has_process_permission_prompt() {
            return false;
        }
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                    self.on_key(
                        event_loop,
                        code,
                        event.state == ElementState::Pressed,
                        event.repeat,
                    );
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if *button == MouseButton::Left {
                    self.shell.button(*state == ElementState::Pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.cursor = (position.x as f32, position.y as f32);
                self.shell.pointer(position.x as f32, position.y as f32);
            }
            _ => return false,
        }
        true
    }
    pub(crate) fn has_process_permission_prompt(&self) -> bool {
        self.shell.process_prompt.is_some()
            || self
                .integrations
                .plugins
                .as_ref()
                .and_then(|p| p.hub.process_permissions.as_ref())
                .is_some_and(|p| p.has_pending())
    }
    pub(crate) fn process_permission_prompt(&self) -> Option<ProcessPrompt> {
        self.integrations
            .plugins
            .as_ref()?
            .hub
            .process_permissions
            .as_ref()?
            .prompt()
    }
    pub(crate) fn answer_process_permission(&mut self, id: u64, decision: ProcessDecision) {
        if let Some(permissions) = self
            .integrations
            .plugins
            .as_ref()
            .and_then(|p| p.hub.process_permissions.as_ref())
        {
            permissions.respond(id, decision);
        }
    }
    pub(crate) fn frame_process_permission(&mut self, dt: f32) -> bool {
        let Some(prompt) = self.process_permission_prompt() else {
            if self.shell.process_prompt.take().is_some() {
                self.shell.process_dialog = Default::default();
                self.shell.clear();
                self.sync_mouse_grab();
            }
            return false;
        };
        let Some(surface) = self.gfx.surface.as_ref() else {
            return false;
        };
        let (w, h) = (surface.config.width as f32, surface.config.height as f32);
        if self.shell.process_prompt != Some(prompt.id) {
            self.shell.process_dialog = Default::default();
            self.shell.process_prompt = Some(prompt.id);
            // A click/key intended for the bus must never turn into consent as it opens.
            self.shell.ui.discard_input();
            self.shell.ui.dismiss_popups();
            self.shell.ui.input.down = false;
            self.shell.ui.active = None;
            self.release_vehicle_keys();
            self.input.keys.clear();
            self.input.mouse_look = false;
            self.input.mmb_held = false;
            if let Some(photo) = self.photo.as_mut() {
                photo.looking = false;
            }
            self.sync_look_hold();
            self.sync_mouse_grab();
            if let Some(window) = &self.window {
                window.set_cursor_visible(true);
            }
        }
        let dpi = self
            .window
            .as_ref()
            .map(|w| w.scale_factor() as f32)
            .unwrap_or(1.0);
        let scale = dpi
            * crate::ui::size_factor(
                h,
                dpi,
                self.settings.ui_scale,
                self.settings.ui_scale_window,
            );
        self.shell.begin(w, h, scale, dt);
        self.shell.opaque = false;
        if let Some(decision) = draw(&mut self.shell.ui, &prompt, &mut self.shell.process_dialog) {
            self.shell
                .actions
                .push(crate::shell::Action::ProcessPermission(prompt.id, decision));
        }
        self.shell.finish();
        true
    }
}
