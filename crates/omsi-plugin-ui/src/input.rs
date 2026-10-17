//! The mouse and the keyboard on the plugins' panels while they have the mouse: clicks,
//! sliders and tabs set where they are pressed, sliders and draggable panels followed while
//! the button is held, and the text typed into a field.

use super::{hit_at, HitKind, Key, PluginPanels};
use glam::Vec2;
use omsi_plugin::ui::UiState;
use omsi_ui::Rect;

/// What the left button holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    /// A slider: its plugin, panel and id, and its track on the window (physical pixels).
    Slider { owner: u64, panel: String, element: String, track: [f32; 2] },
    /// A panel moved by its free parts: where the cursor was (physical pixels).
    Panel { owner: u64, panel: String, last: Vec2 },
}

/// A press on the panels.
#[derive(Debug, Clone, PartialEq)]
pub struct Press {
    pub owner: u64,
    pub panel: String,
    pub element: Option<String>,
    pub kind: HitKind,
    /// Where across the hit it is, 0..1 (a slider's value, the tab).
    pub fraction: f32,
    /// The hit's left and right edges on the window (physical pixels).
    pub span: [f32; 2],
    /// It is on no clickable part of a panel that can be dragged.
    pub drag: bool,
    /// It is on something that takes clicks (the panel itself when it is clickable).
    pub hit: bool,
}

/// A panel's place moved by where the player dragged it, kept on the screen.
pub fn moved(at: Vec2, by: (f32, f32), w: f32, h: f32, screen: Vec2) -> Vec2 {
    Vec2::new((at.x + by.0).clamp(0.0, (screen.x - w).max(0.0)), (at.y + by.1).clamp(0.0, (screen.y - h).max(0.0)))
}

impl PluginPanels {
    /// What a press at `(x, y)` (physical pixels on the window) is on: the topmost panel's
    /// part there.
    pub fn press_at(&self, ui: &UiState, x: f32, y: f32) -> Option<Press> {
        for e in ui.panels().iter().rev().filter(|e| e.panel.visible) {
            let Some(card) = self.cards.get(&Key::Panel(e.owner, e.id.clone())) else { continue };
            if card.seen != self.frame {
                continue;
            }
            // (each card at its own scale: the navigator's for the panel in its board, which
            // stays where the navigator puts it)
            let k = card.scale.max(0.25);
            let draggable = e.panel.draggable && e.panel.anchor != omsi_plugin::ui::Anchor::Navigator;
            let local = (Vec2::new(x, y) - card.origin) / k;
            if !Rect::new(0.0, 0.0, card.laid.w, card.laid.h).contains(local) {
                continue;
            }
            let hit = hit_at(&card.laid, local).map(|i| &card.laid.hits[i]);
            // (a clickable panel's own hit, the whole card, does not keep it from being dragged)
            let on_part = hit.is_some_and(|h| h.element.is_some());
            return Some(match hit {
                Some(h) => Press {
                    owner: e.owner,
                    panel: e.id.clone(),
                    element: h.element.clone(),
                    kind: h.kind,
                    fraction: ((local.x - h.r.x) / h.r.w.max(1.0)).clamp(0.0, 1.0),
                    span: [card.origin.x + h.r.x * k, card.origin.x + h.r.right() * k],
                    drag: draggable && !on_part,
                    hit: true,
                },
                None => Press { owner: e.owner, panel: e.id.clone(), element: None, kind: HitKind::Click, fraction: 0.0, span: [0.0, 0.0], drag: draggable, hit: false },
            });
        }
        None
    }
}
