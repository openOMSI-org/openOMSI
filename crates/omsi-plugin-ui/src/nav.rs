//! The panels' elements in the navigator's look (`omsi.ui` version 4): boxes - a tinted card
//! with a stripe, for a note or an advice - and stop lists, the stops on a rail with their
//! times, as the navigator's duty board drew them; and a bar's parts and a badge's weight.

use super::{color, ink_on, Item, Layout, ACCENT, BADGE_H, BADGE_PAD, BADGE_PX};
use glam::Vec2;
use omsi_plugin::ui::{Element, Kind, Rgba, Stop, StopState};
use omsi_ui::{Color, Rect, Weight};

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
