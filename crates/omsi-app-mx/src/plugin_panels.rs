//! The mouse and the keyboard on the plugins' panels, the app's side: whether the panels
//! have the mouse, and the clicks, drags and typing handed to them (what a press is on and
//! how a drag moves is `omsi_plugin_ui`'s).

use crate::plugin_ui::{input::{Drag, Press}, HitKind};
use glam::Vec2;
use omsi_plugin::ui::UiState;

impl crate::App {
    /// A press of the left button while the panels do not have the mouse, on a panel that
    /// takes clicks without it (`grab`), and the release of such a press: the panel's, as a
    /// click with the mouse would be. True when it was (the cab gets nothing of it).
    pub(crate) fn plugin_grab(&mut self, pressed: bool) -> bool {
        if !pressed {
            if !std::mem::take(&mut self.integrations.plugin_panels.grabbed) {
                return false;
            }
            self.plugin_click(false);
            return true;
        }
        let Some(p) = self.integrations.plugins.as_ref() else { return false };
        // (not while the mouse turns the head: there is no cursor to click with)
        if self.input.mouse_look {
            return false;
        }
        let (x, y) = self.input.cursor;
        if !self.integrations.plugin_panels.grabs_at(&p.ui.borrow(), x, y) {
            return false;
        }
        self.integrations.plugin_panels.grabbed = true;
        self.plugin_press(true, true);
        true
    }

    /// The left button while the panels have the mouse: a press on a clickable part goes to
    /// its plugin as `ui_click`, a slider or tabs take the place pressed, a draggable panel
    /// follows the cursor from its free parts; the bus gets none of it.
    pub(crate) fn plugin_click(&mut self, pressed: bool) {
        self.plugin_press(pressed, false);
    }

    /// [`App::plugin_click`]; `grab`: a press on a panel that takes it without the mouse,
    /// which clicks its buttons, boxes, sliders and tabs, but neither starts typing into a
    /// field nor drags the panel (the driver did not ask the panels for the mouse).
    fn plugin_press(&mut self, pressed: bool, grab: bool) {
        let Some(p) = self.integrations.plugins.as_ref() else {
            return;
        };
        if !pressed {
            self.integrations.plugin_panels.drag = None;
            return;
        }
        let (x, y) = self.input.cursor;
        let press = self.integrations.plugin_panels.press_at(&p.ui.borrow(), x, y);
        let mut ui = p.ui.borrow_mut();
        // (a press anywhere but on the field typed into ends the typing)
        let on_field = |pr: &Option<Press>, ui: &UiState| matches!((pr, ui.typing()), (Some(pr), Some(t)) if pr.owner == t.owner && pr.panel == t.panel && pr.element.as_deref() == Some(t.element.as_str()));
        if ui.typing().is_some() && !on_field(&press, &ui) {
            ui.stop_typing(false);
        }
        let Some(pr) = press else { return };
        if grab && pr.element.as_deref().is_some_and(|el| ui.is_field(pr.owner, &pr.panel, el)) {
            return;
        }
        match (pr.kind, pr.element.as_deref()) {
            (HitKind::Slider, Some(el)) => {
                ui.set_control(pr.owner, &pr.panel, el, pr.fraction);
                self.integrations.plugin_panels.drag = Some(Drag::Slider { owner: pr.owner, panel: pr.panel.clone(), element: el.to_string(), track: pr.span });
            }
            (HitKind::Tabs, Some(el)) => ui.set_control(pr.owner, &pr.panel, el, pr.fraction),
            _ if pr.drag && !grab => {
                self.integrations.plugin_panels.drag = Some(Drag::Panel { owner: pr.owner, panel: pr.panel.clone(), last: Vec2::new(x, y) });
            }
            _ if pr.hit => ui.click(pr.owner, &pr.panel, pr.element.as_deref()),
            _ => {}
        }
    }
}

impl crate::app::Integrations {
    /// A key while the player types into a plugin's text field: the text goes to the field,
    /// Backspace rubs out, Enter hands it to the plugin and Esc leaves it. True when the
    /// key was the field's (the bus does not get it).
    pub(crate) fn plugin_typing_key(&mut self, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
        use winit::keyboard::KeyCode;
        let Some(p) = self.plugins.as_ref() else { return false };
        let mut ui = p.ui.borrow_mut();
        if ui.typing().is_none() {
            return false;
        }
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => ui.stop_typing(true),
            KeyCode::Escape => ui.stop_typing(false),
            KeyCode::Backspace => ui.type_text(None),
            _ => {
                if let Some(t) = text.filter(|t| t.chars().any(|c| !c.is_control())) {
                    ui.type_text(Some(t));
                }
            }
        }
        true
    }

    /// The cursor moved while a slider or a panel is dragged: it follows. True when it did
    /// (the cursor's other work is left out then).
    pub(crate) fn plugin_drag_move(&mut self, x: f32, y: f32) -> bool {
        let Some(drag) = self.plugin_panels.drag.clone() else { return false };
        let Some(p) = self.plugins.as_ref() else { return false };
        let mut ui = p.ui.borrow_mut();
        if !ui.focused() {
            self.plugin_panels.drag = None;
            return false;
        }
        match drag {
            Drag::Slider { owner, panel, element, track } => {
                let f = ((x - track[0]) / (track[1] - track[0]).max(1.0)).clamp(0.0, 1.0);
                ui.set_control(owner, &panel, &element, f);
            }
            Drag::Panel { owner, panel, last } => {
                let k = self.plugin_panels.scale.max(0.25);
                ui.drag(owner, &panel, (x - last.x) / k, (y - last.y) / k);
                self.plugin_panels.drag = Some(Drag::Panel { owner, panel, last: Vec2::new(x, y) });
            }
        }
        true
    }

    /// Esc while the panels have the mouse: typing in a field ends first, then the mouse goes
    /// back to the bus (and not on to the menu). True when it did one of them.
    pub(crate) fn release_plugin_focus(&mut self) -> bool {
        let Some(p) = self.plugins.as_ref().filter(|p| p.ui.borrow().focused()) else {
            return false;
        };
        let mut ui = p.ui.borrow_mut();
        if ui.typing().is_some() {
            ui.stop_typing(false);
        } else {
            ui.release_focus();
        }
        true
    }

    /// Whether the plugins' panels have the mouse (`omsi.ui.focus`).
    pub(crate) fn plugin_focus(&self) -> bool {
        crate::plugin_ui::focused(&self.plugins)
    }
}
