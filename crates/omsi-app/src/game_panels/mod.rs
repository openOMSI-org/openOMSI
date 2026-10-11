//! The game's own panels over the picture, made of the plugins' cards (`omsi.ui`'s elements,
//! laid out and drawn by `plugin_ui`): OMSI 2's quick menu behind a tap of Alt, the trip's
//! summary at its end, the destination form, the view editor and the question whether to
//! move the bus where the map camera was clicked.
//!
//! They have a `UiState` of their own - the plugins' may not be there at all, and theirs
//! are the plugins' - with one owner, the game. Each frame the clicks and the changes of
//! the last are worked off, the open panels are set anew (a panel that stays the same is
//! not laid out again) and drawn. While one of them is open the mouse is theirs: a click
//! on a panel goes to it, a click beside the quick menu closes it.

use omsi_plugin::ui::{Align, Element, Kind, Panel, Rgba, RowAlign, UiState, Weight};

mod dest_form;
mod move_here;
pub(crate) use dest_form::open as open_dest_form;
pub(crate) use move_here::ask as ask_move;
pub(crate) mod quick_menu;
pub(crate) mod trip_summary;
pub(crate) mod view_editor;
#[cfg(test)]
mod preview;

/// The panels' owner in their `UiState` (there is no other).
const GAME: u64 = 1;

/// The panels and what they show.
#[derive(Default)]
pub(crate) struct GamePanels {
    ui: UiState,
    draw: crate::plugin_ui::PluginPanels,
    /// A slider the left button holds: its panel, id and track on the window (physical
    /// pixels).
    slider: Option<(String, String, [f32; 2])>,
    /// Alt is down and nothing else has been pressed with it: let go, it opens the quick menu.
    alt_alone: bool,
    pub(crate) quick: Option<quick_menu::QuickMenu>,
    pub(crate) trip: Option<trip_summary::TripPanel>,
    pub(crate) dest: Option<dest_form::DestForm>,
    pub(crate) view: Option<view_editor::ViewEditor>,
    /// Where the map camera's click asks to move the bus to.
    pub(crate) move_to: Option<glam::DVec3>,
}

impl GamePanels {
    /// Whether any panel is open.
    pub(crate) fn any(&self) -> bool {
        self.quick.is_some() || self.trip.is_some() || self.dest.is_some() || self.view.is_some() || self.move_to.is_some()
    }

    /// Whether the panels have the mouse.
    pub(crate) fn focused(&self) -> bool {
        self.ui.focused()
    }
}

// --- the elements --------------------------------------------------------------------------

pub(super) fn el(id: Option<&str>, kind: Kind) -> Element {
    Element { id: id.map(str::to_string), color: None, grow: false, clickable: false, kind }
}

pub(super) fn text(t: &str, size: f32, weight: Weight) -> Element {
    el(None, Kind::Text { text: t.to_string(), size, weight, align: Align::Left, wrap: true })
}

pub(super) fn title(t: &str) -> Element {
    text(t, 17.0, Weight::Bold)
}

pub(super) fn note(t: &str) -> Element {
    Element { color: Some(Rgba([170, 170, 170, 255])), ..text(t, 12.0, Weight::Regular) }
}

pub(super) fn button(id: &str, t: &str, icon: Option<&str>) -> Element {
    el(Some(id), Kind::Button { text: t.to_string(), icon: icon.map(str::to_string) })
}

pub(super) fn row(children: Vec<Element>, align: RowAlign) -> Element {
    el(None, Kind::Row { children, gap: 8.0, align })
}

/// A row of children that share its width alike.
pub(super) fn grow_row(children: Vec<Element>) -> Element {
    row(children.into_iter().map(|c| Element { grow: true, ..c }).collect(), RowAlign::Start)
}

/// A labelled slider: the name and its value in words to the left, the slider to the right.
pub(super) fn slider_row(id: &str, name: &str, shown: &str, value: f32, range: (f32, f32), step: f32) -> Element {
    row(
        vec![
            Element { grow: false, ..text(name, 13.0, Weight::Regular) },
            Element { grow: true, ..el(None, Kind::Text { text: shown.to_string(), size: 13.0, weight: Weight::Medium, align: Align::Right, wrap: false }) },
            Element { grow: false, ..el(Some(id), Kind::Slider { value, min: range.0, max: range.1, step }) },
        ],
        RowAlign::Start,
    )
}

pub(super) fn input(id: &str, value: &str, placeholder: &str, max: usize) -> Element {
    Element { grow: true, ..el(Some(id), Kind::Input { text: value.to_string(), placeholder: placeholder.to_string(), max }) }
}

pub(super) fn divider() -> Element {
    el(None, Kind::Divider)
}

/// The game's card: its dark colour, a little see-through over the picture (`alpha`).
pub(super) fn card(anchor: omsi_plugin::ui::Anchor, x: f32, y: f32, width: f32, alpha: u8, children: Vec<Element>) -> Panel {
    Panel { anchor, x, y, width, padding: 14.0, gap: 8.0, background: Some(Rgba([14, 16, 20, alpha])), radius: 12.0, accent: None, visible: true, clickable: false, draggable: false, children }
}

// --- the frame -----------------------------------------------------------------------------

impl crate::App {
    /// Work off what was clicked and changed, set the open panels and draw them.
    pub(crate) fn frame_game_panels(&mut self, dt: f32, hud: [f32; 4], vr_active: bool) {
        let clicks = self.menus.game_panels.ui.take_clicks(GAME);
        for c in clicks {
            let el = c.element.as_deref().unwrap_or("");
            match c.panel.as_str() {
                "quick" => quick_menu::click(self, el),
                "trip" => trip_summary::click(self, el),
                "dest" => dest_form::click(self, el),
                "view" => view_editor::click(self, el),
                "move" => move_here::click(self, el),
                _ => {}
            }
        }
        let changes = self.menus.game_panels.ui.take_changes(GAME);
        for c in changes {
            match c.panel.as_str() {
                "view" => view_editor::change(self, &c.element, &c.value),
                "dest" => dest_form::change(self, &c.element, &c.value),
                _ => {}
            }
        }
        // A completed trip's summary waits until the bus stops: the panel takes the mouse.
        let speed = self.player.as_ref().map_or(f32::INFINITY, |p| p.vehicle.physics.velocity_kmh());
        let still = if self.paused { 0.0 } else { dt };
        if let Some(j) = self.session.journey.as_mut() {
            if self.settings.trip_summary {
                if let Some(s) = j.take_summary_when_stopped(speed, still) {
                    self.menus.game_panels.trip = Some(trip_summary::TripPanel::new(s));
                }
            } else {
                // Disabled summaries are consumed, so enabling the setting later cannot
                // open a report from an earlier trip. The quick menu still has it.
                j.take_summary();
            }
        }
        let panels: Vec<(&str, Option<Panel>)> = vec![
            ("view", self.menus.game_panels.view.as_ref().map(|v| v.panel(self))),
            ("trip", self.menus.game_panels.trip.as_ref().map(|t| t.panel())),
            ("dest", self.menus.game_panels.dest.as_ref().map(|d| d.panel())),
            ("quick", self.menus.game_panels.quick.as_ref().map(|q| q.panel(self))),
            ("move", self.menus.game_panels.move_to.map(move_here::panel)),
        ];
        let gp = &mut self.menus.game_panels;
        for (id, p) in panels {
            match p {
                Some(p) => {
                    let _ = gp.ui.set(GAME, id, p);
                }
                None => {
                    gp.ui.remove(GAME, id);
                }
            }
        }
        let (Some(r), Some(scene)) = (self.renderer.as_ref(), self.scene.as_mut()) else { return };
        let dpi = self.window.as_ref().map_or(1.0, |w| w.scale_factor() as f32);
        let mut size = crate::ui::size_factor(hud[3], dpi, self.settings.ui_scale, self.settings.ui_scale_window);
        // (the trip's summary shrinks to fit a small screen - a phone - rather than spill off it)
        if let Some((w, h)) = self.menus.game_panels.trip.as_ref().map(|t| t.extent()) {
            size = size.min(hud[2] * 0.96 / (w * dpi)).min(hud[3] * 0.96 / (h * dpi));
        }
        let map_open = self.menus.navigator.as_ref().is_some_and(|n| n.map_open());
        let hidden = vr_active || self.menus.game_menu.is_some() || self.menus.chooser.is_some() || map_open || self.photo.is_some();
        let frame = crate::plugin_ui::PanelsFrame {
            hud,
            scale: dpi * size,
            hidden,
            cursor: self.input.cursor,
            dt,
            backdrop: crate::ui::backdrop(self.settings.ui_opacity),
            navigator: None,
        };
        let gp = &mut self.menus.game_panels;
        gp.draw.frame(r, scene, &mut gp.ui, &frame);
        // (the mouse is the panels' while one is open and shown)
        let open = gp.any();
        if open != gp.ui.focused() {
            gp.ui.set_focus(GAME, open);
        }
        if !gp.ui.focused() {
            gp.slider = None;
        }
    }

    /// Whether the game's panels have the mouse.
    pub(crate) fn game_panels_mouse(&self) -> bool {
        self.menus.game_panels.focused()
    }

    /// The left button while the panels have the mouse: a press on a button goes to it, a
    /// slider takes the place pressed and follows the cursor, a field starts taking the keys.
    /// True when the click was the panels' (the bus does not get it). A press beside the
    /// panels closes the quick menu, and goes on to the bus while only the view editor is
    /// open (its mirrors and switches are to be tried with it).
    pub(crate) fn game_panel_click(&mut self, pressed: bool) -> bool {
        let gp = &mut self.menus.game_panels;
        if !gp.ui.focused() {
            return false;
        }
        if !pressed {
            return gp.slider.take().is_some();
        }
        let (x, y) = self.input.cursor;
        let press = gp.draw.press_at(&gp.ui, x, y);
        if gp.ui.typing().is_some() && !press.as_ref().is_some_and(|p| gp.ui.typing().is_some_and(|t| t.panel == p.panel && p.element.as_deref() == Some(t.element.as_str()))) {
            gp.ui.stop_typing(false);
        }
        let Some(pr) = press else {
            if gp.quick.take().is_some() {
                return true;
            }
            return gp.trip.is_some() || gp.dest.is_some() || gp.move_to.is_some();
        };
        match (pr.kind, pr.element.as_deref()) {
            (crate::plugin_ui::HitKind::Slider, Some(el)) => {
                gp.ui.set_control(pr.owner, &pr.panel, el, pr.fraction);
                gp.slider = Some((pr.panel.clone(), el.to_string(), pr.span));
            }
            (crate::plugin_ui::HitKind::Tabs, Some(el)) => gp.ui.set_control(pr.owner, &pr.panel, el, pr.fraction),
            _ if pr.hit => gp.ui.click(pr.owner, &pr.panel, pr.element.as_deref()),
            _ => {}
        }
        true
    }

    /// The cursor moved: a slider held follows it. True when one did.
    pub(crate) fn game_panel_drag(&mut self, x: f32) -> bool {
        let gp = &mut self.menus.game_panels;
        let Some((panel, el, track)) = gp.slider.clone() else { return false };
        let f = ((x - track[0]) / (track[1] - track[0]).max(1.0)).clamp(0.0, 1.0);
        gp.ui.set_control(GAME, &panel, &el, f);
        true
    }

    /// A key while the panels are open: typed into the field being typed into (Enter takes
    /// it, Esc leaves it), Esc closes the panel on top. True when the key was theirs.
    pub(crate) fn game_panel_key(&mut self, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
        use winit::keyboard::KeyCode;
        let gp = &mut self.menus.game_panels;
        if gp.ui.typing().is_some() {
            match code {
                KeyCode::Enter | KeyCode::NumpadEnter => gp.ui.stop_typing(true),
                KeyCode::Escape => gp.ui.stop_typing(false),
                KeyCode::Backspace => gp.ui.type_text(None),
                _ => {
                    if let Some(t) = text.filter(|t| t.chars().any(|c| !c.is_control())) {
                        gp.ui.type_text(Some(t));
                    }
                }
            }
            return true;
        }
        if code == KeyCode::Escape {
            return self.close_game_panel();
        }
        false
    }

    /// Esc: the panel on top closes (the quick menu, the question, the form, the summary,
    /// the view editor - which keeps what was set without saving it). True when one did.
    pub(crate) fn close_game_panel(&mut self) -> bool {
        let gp = &mut self.menus.game_panels;
        if gp.quick.take().is_some() || gp.move_to.take().is_some() || gp.dest.take().is_some() || gp.trip.take().is_some() {
            return true;
        }
        if gp.view.is_some() {
            view_editor::close(self, false);
            return true;
        }
        false
    }

    /// Alt pressed and let go with nothing else between: OMSI 2's quick menu opens (or
    /// closes). Every key comes through here, pressed and released, before its own work.
    pub(crate) fn alt_tap(&mut self, code: winit::keyboard::KeyCode, pressed: bool, repeat: bool) {
        use winit::keyboard::KeyCode;
        let alt = matches!(code, KeyCode::AltLeft | KeyCode::AltRight);
        let gp = &mut self.menus.game_panels;
        match (alt, pressed) {
            (true, true) if !repeat => gp.alt_alone = true,
            (true, false) if std::mem::take(&mut gp.alt_alone) => {
                if self.menus.game_menu.is_none() && self.menus.chooser.is_none() && self.photo.is_none() && !self.vr_active() {
                    quick_menu::toggle(self);
                }
            }
            (false, _) if pressed => gp.alt_alone = false,
            _ => {}
        }
    }

    /// A mouse button went down: Alt held with it is no tap.
    pub(crate) fn alt_tap_spoilt(&mut self) {
        self.menus.game_panels.alt_alone = false;
    }
}
