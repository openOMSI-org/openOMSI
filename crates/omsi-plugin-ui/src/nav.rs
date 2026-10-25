//! The panels' elements in the navigator's look (`omsi.ui` version 4): boxes - a tinted card
//! with a stripe, for a note or an advice - and stop lists, the stops on a rail with their
//! times, as the navigator's duty board drew them; and a bar's parts and a badge's weight.
//! The navigator's map (`omsi.ui` version 6) is room the navigator fills: it is noted where
//! it is ([`Laid::map`]), and nothing is drawn there.

use super::{color, ink_on, Item, Layout, ACCENT, BADGE_H, BADGE_PAD, BADGE_PX};
use glam::Vec2;
use omsi_plugin::ui::{Element, Kind, Panel, Rgba, Stop, StopState};
use omsi_ui::{Color, Rect, Weight};

/// The panel for the navigator as it was laid out, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Docked {
    /// Its height (0 before it is laid out).
    pub height: f32,
    /// It fills the navigator's whole frame (`frame = true`), not the board under the map.
    pub frame: bool,
    /// Where the navigator's map goes in it (a `map` element), from the panel's top.
    pub map: Option<DockedMap>,
    /// The colour of the navigator's card under a panel in the whole frame (its `background`;
    /// None: the navigator's own).
    pub background: Option<Color>,
}

/// A panel as it is laid out for the navigator: as wide as it, flat (no accent, no corners)
/// and see-through over the navigator's card. One in the board paints its `background` over
/// the card; one in the whole frame has the navigator paint the card in it, rounded as the
/// card is (a square of its own would stand out at the corners).
pub(crate) fn in_navigator(p: &Panel, width: f32) -> Panel {
    let background = if p.frame { None } else { p.background };
    Panel { width, accent: None, radius: 0.0, background: Some(background.unwrap_or(Rgba([0, 0, 0, 0]))), ..p.clone() }
}

/// The navigator's map in a panel that fills its frame: where it goes (physical pixels, from
/// the panel's top left: x, y, width, height) and whether the navigator's cards go over it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockedMap {
    pub rect: [f32; 4],
    pub cards: bool,
}

/// A box's room round what is in it (the stripe's side, the others), and its stripe.
const BOX_PAD: (f32, f32) = (12.0, 8.0);
const BOX_Y: f32 = 7.0;
const BOX_STRIPE: f32 = 3.0;
const BOX_R: f32 = 6.0;
const BOX_TINT: f32 = 0.14;
/// A stop's row at size 13 (it grows with the size), the rail's place and the names'.
const STOP_ROW: f32 = 22.0;
const RAIL_X: f32 = 6.0;
const NAME_X: f32 = 19.0;
/// The rail where the bus has been, and the texts by where the stop is.
const RAIL_DONE: Color = Color::rgba(74, 84, 98, 1.0);
const TEXT_NOW: Color = Color::rgba(255, 255, 255, 1.0);
const TEXT_SOFT: Color = Color::rgba(196, 202, 214, 1.0);
const TEXT_DIM: Color = Color::rgba(149, 157, 176, 1.0);
const TEXT_FAINT: Color = Color::rgba(110, 118, 136, 1.0);
const TAG_FILL: Color = Color::rgba(255, 255, 255, 0.10);
/// The navigator's own map: this share of its width high.
const MAP_SHARE: f32 = 0.8;

impl Layout<'_> {
    pub(crate) fn nav_natural(&self, e: &Element) -> f32 {
        match &e.kind {
            Kind::Box { children, .. } => children.iter().map(|c| self.natural(c)).fold(0.0, f32::max) + BOX_STRIPE + BOX_PAD.0 + BOX_PAD.1,
            Kind::Stops { stops, size } => {
                let k = size / 13.0;
                stops.iter().map(|s| NAME_X * k + self.width(&s.name, *size, Weight::Medium) + 2.0 * self.width("00:00", size * 12.0 / 13.0, Weight::Bold) + 16.0 * k).fold(0.0, f32::max).ceil()
            }
            _ => 0.0,
        }
    }

    pub(crate) fn nav_height(&self, e: &Element, w: f32) -> f32 {
        match &e.kind {
            Kind::Box { children, gap } => {
                let inner = (w - BOX_STRIPE - BOX_PAD.0 - BOX_PAD.1).max(1.0);
                let h: f32 = children.iter().map(|c| self.height(c, inner, false)).sum();
                h + gap * children.len().saturating_sub(1) as f32 + 2.0 * BOX_Y
            }
            Kind::Stops { stops, size } => stops.len() as f32 * STOP_ROW * size / 13.0,
            // (as high as the navigator's own map unless it says: 0.8 times the width)
            Kind::Map { height, .. } if *height > 0.0 => *height,
            Kind::Map { .. } => (w * MAP_SHARE).round(),
            _ => 0.0,
        }
    }

    pub(crate) fn nav_emit(&mut self, e: &Element, r: Rect) {
        match &e.kind {
            Kind::Box { children, gap } => {
                let tint = e.color.map(color).unwrap_or(ACCENT);
                self.rounded(r, BOX_R, tint.alpha(BOX_TINT), None);
                self.rounded(Rect::new(r.x, r.y, BOX_STRIPE, r.h), BOX_STRIPE * 0.5, tint, None);
                let x = r.x + BOX_STRIPE + BOX_PAD.0;
                let w = (r.right() - BOX_PAD.1 - x).max(1.0);
                let mut y = r.y + BOX_Y;
                for (i, c) in children.iter().enumerate() {
                    if i > 0 {
                        y += gap;
                    }
                    let h = self.height(c, w, false);
                    self.emit(c, Rect::new(x, y, w, h), false);
                    y += h;
                }
            }
            Kind::Stops { stops, size } => {
                let rail = e.color.map(color).unwrap_or(ACCENT);
                let row = STOP_ROW * size / 13.0;
                for (i, s) in stops.iter().enumerate() {
                    let rr = Rect::new(r.x, r.y + i as f32 * row, r.w, row);
                    self.stop(s, rr, *size, rail, i > 0, i + 1 < stops.len());
                }
            }
            Kind::Map { cards, .. } => {
                if self.out.map.is_none() {
                    self.out.map = Some((r, *cards));
                }
            }
            _ => {}
        }
    }

    /// A bar's parts on its `track` (`segments`): side by side, a hair apart, each as wide as
    /// the bar is high at least, so that it shows as a round part of its own, and the rest of
    /// the width shared by their values - none reaches into the next. False when there are none
    /// to draw (the bar shows its `value` then).
    pub(crate) fn bar_parts(&mut self, track: Rect, segments: &[(f32, Rgba)]) -> bool {
        let parts: Vec<&(f32, Rgba)> = segments.iter().filter(|s| s.0 > 0.0).collect();
        let total: f32 = parts.iter().map(|s| s.0).sum();
        if total <= 0.0 {
            return false;
        }
        let height = track.h;
        let least = height.min(track.w / parts.len() as f32);
        let spare = (track.w - least * parts.len() as f32).max(0.0);
        let mut x = track.x;
        for (i, (v, c)) in parts.iter().enumerate() {
            let w = least + spare * v / total;
            let drawn = if i + 1 == parts.len() { track.right() - x } else { w - 1.0 };
            self.rounded(Rect::new(x, track.y, drawn.max(0.0), height), (height * 0.5).min(drawn * 0.5).max(0.0), color(*c), None);
            x += w;
        }
        true
    }

    /// A badge: its text in its weight on a rounded fill, `BADGE_H` high, in the middle of `r`.
    pub(crate) fn badge(&mut self, e: &Element, r: Rect, text: &str, text_color: Option<Rgba>, wt: Weight) {
        let w = self.natural(e).min(r.w);
        let b = Rect::new(r.x, r.y + (r.h - BADGE_H) * 0.5, w, BADGE_H);
        let fill = e.color.map(color).unwrap_or(ACCENT);
        self.rounded(b, BADGE_H * 0.5, fill, None);
        let t = self.fit(text, BADGE_PX, wt, w - 2.0 * BADGE_PAD);
        let tw = self.width(&t, BADGE_PX, wt);
        let y = self.baseline(b.y, b.h, BADGE_PX, wt);
        self.text(t, BADGE_PX, wt, Vec2::new(b.x + (b.w - tw) * 0.5, y), text_color.map(color).unwrap_or(ink_on(fill)));
    }

    /// A circle (a rounded square as wide as it is high).
    fn dot(&mut self, at: Vec2, radius: f32, c: Color) {
        self.rounded(Rect::new(at.x - radius, at.y - radius, 2.0 * radius, 2.0 * radius), radius, c, None);
    }

    /// One stop on the rail: its pin with the rail through it (grey where the bus has been),
    /// its name, and on the right its time, before it when the bus will be there, and its
    /// tag.
    fn stop(&mut self, s: &Stop, r: Rect, size: f32, rail: Color, above: bool, below: bool) {
        let k = size / 13.0;
        let (x, cy) = (r.x + RAIL_X * k, r.y + r.h * 0.5);
        let line = if s.state == StopState::Done { RAIL_DONE } else { rail };
        if above {
            self.out.items.push(Item::Rect { r: Rect::new(x - 1.5 * k, r.y, 3.0 * k, r.h * 0.5), color: line });
        }
        if below {
            self.out.items.push(Item::Rect { r: Rect::new(x - 1.5 * k, cy, 3.0 * k, r.h * 0.5), color: line });
        }
        let center = Vec2::new(x, cy);
        match s.state {
            StopState::Done => self.dot(center, 4.0 * k, RAIL_DONE),
            StopState::Now => {
                self.dot(center, 9.0 * k, rail.alpha(0.25));
                self.dot(center, 6.0 * k, rail);
            }
            StopState::Ahead => {
                self.dot(center, 4.5 * k, rail);
                self.dot(center, 2.5 * k, Color::rgba(20, 26, 38, 1.0));
            }
        }
        // the times from the right: the planned one, and before it the expected one
        let tpx = 12.0 * k;
        let mut right = r.right();
        let lh = r.h;
        if !s.time.is_empty() {
            let (wt, c) = match s.state {
                StopState::Done => (Weight::Medium, TEXT_FAINT),
                StopState::Now => (Weight::Bold, rail),
                StopState::Ahead => (Weight::Medium, TEXT_DIM),
            };
            let tw = self.width(&s.time, tpx, wt);
            let base = self.baseline(r.y, lh, tpx, wt);
            self.text(s.time.clone(), tpx, wt, Vec2::new(right - tw, base), c);
            right -= tw + 8.0 * k;
        }
        if !s.expected.is_empty() {
            let ew = self.width(&s.expected, tpx, Weight::Bold);
            let base = self.baseline(r.y, lh, tpx, Weight::Bold);
            self.text(s.expected.clone(), tpx, Weight::Bold, Vec2::new(right - ew, base), s.expected_color.map(color).unwrap_or(TEXT_SOFT));
            right -= ew + 8.0 * k;
        }
        if !s.tag.is_empty() {
            let px = 10.0 * k;
            let w = self.width(&s.tag, px, Weight::Medium) + 10.0 * k;
            let t = Rect::new(right - w, cy - 8.0 * k, w, 16.0 * k);
            self.rounded(t, 4.0 * k, TAG_FILL, None);
            let base = self.baseline(t.y, t.h, px, Weight::Medium);
            self.text(s.tag.clone(), px, Weight::Medium, Vec2::new(t.x + 5.0 * k, base), TEXT_DIM);
            right -= w + 8.0 * k;
        }
        let (px, wt, c) = match s.state {
            StopState::Done => (12.5 * k, Weight::Regular, TEXT_FAINT),
            StopState::Now => (14.0 * k, Weight::Bold, TEXT_NOW),
            StopState::Ahead => (13.0 * k, Weight::Medium, TEXT_SOFT),
        };
        let nx = r.x + NAME_X * k;
        let name = self.fit(&s.name, px, wt, (right - nx).max(0.0));
        let base = self.baseline(r.y, lh, px, wt);
        self.text(name, px, wt, Vec2::new(nx, base), c);
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use omsi_plugin::ui::panel_from_source;

    fn panel(src: &str) -> Panel {
        panel_from_source(src).unwrap()
    }

    /// A panel that fills the navigator's frame says where its map goes: the room of its
    /// first `map` element (the navigator's own map's height unless it says), drawn empty;
    /// cut off, as much of it as is left.
    /// A panel in the navigator is laid out see-through; one in the whole frame leaves its
    /// background to the navigator's card (rounded as the card is), one in the board paints it.
    #[test]
    fn the_whole_frame_s_background_is_the_navigators_card() {
        let frame = panel(r##"{ anchor = "navigator", frame = true, background = "#141A26", accent = "#FF0000", radius = 8 }"##);
        let p = super::in_navigator(&frame, 360.0);
        assert_eq!((p.width, p.background, p.accent, p.radius), (360.0, Some(Rgba([0, 0, 0, 0])), None, 0.0));
        let board = panel(r##"{ anchor = "navigator", background = "#141A26" }"##);
        assert_eq!(super::in_navigator(&board, 360.0).background, Some(Rgba([0x14, 0x1A, 0x26, 255])));
    }

    #[test]
    fn a_map_element_is_room_for_the_navigators_map() {
        let fonts = Fonts::new();
        let p = panel(r#"{ anchor = "navigator", frame = true, width = 360, padding = 10, gap = 6, children = {
            { type = "text", text = "Line 5", size = 14 },
            { type = "map", cards = false },
            { type = "map", height = 40 },
            { type = "button", id = "go", text = "Go" },
        } }"#);
        let mut l = layout(&p, &fonts, 1.0);
        let (r, cards) = l.map.expect("the map");
        assert!(!cards);
        assert_eq!((r.x, r.w, r.h), (10.0, 340.0, (340.0 * 0.8f32).round()));
        assert!((r.y - (10.0 + 14.0 * LINE + 6.0)).abs() < 0.01, "{r:?}");
        let empty = |l: &Laid| l.items.iter().all(|i| match i {
            Item::Rect { r: x, .. } | Item::Rounded { r: x, .. } => x.right() <= r.x || x.x >= r.right() || x.bottom() <= r.y || x.y >= r.bottom(),
            _ => true,
        });
        assert!(empty(&l), "nothing is drawn where the map goes");
        assert_eq!(l.h, (10.0 + 14.0 * LINE + 6.0 + r.h + 6.0 + 40.0 + 6.0 + BUTTON_H + 10.0).ceil());
        // (a map with a height of its own, elsewhere: room of that height)
        let q = panel(r#"{ children = { { type = "map", height = 50 } } }"#);
        assert_eq!(layout(&q, &fonts, 1.0).map.unwrap().0.h, 50.0);
        // cut off in the middle of the map: the part that is left
        cut(&mut l, r.y + 30.0);
        assert_eq!(l.map.unwrap().0.h, 30.0);
        cut(&mut l, r.y);
        assert!(l.map.is_none());
    }
}
