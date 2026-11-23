//! The panels' elements in the navigator's look (`omsi.ui` version 4): boxes - a tinted card
//! with a stripe, for a note or an advice - and stop lists, the stops on a rail with their
//! times, as the navigator's duty board drew them; and a bar's parts and a badge's weight.
//! The navigator's map (`omsi.ui` version 6) is room the navigator fills: it is noted where
//! it is ([`Laid::map`]), and nothing is drawn there but the plugin's own cards over it
//! (`overlay`, version 8), cut off at its edges.

use super::{BADGE_H, BADGE_PAD, BADGE_PX, Item, Layout, color, ink_on};
use glam::Vec2;
use omsi_plugin::ui::{Element, Kind, MapCard, Panel, Rgba, Stop, StopState};
use omsi_ui::{Color, Fonts, Rect, Weight};

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
    Panel {
        width,
        accent: None,
        radius: 0.0,
        background: Some(background.unwrap_or(Rgba([0, 0, 0, 0]))),
        ..p.clone()
    }
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
/// The navigator's own map: this share of its width high.
const MAP_SHARE: f32 = 0.8;

/// What an item covers, near enough to tell whether it lies in a rectangle: a text from
/// its baseline up by its size and down by a quarter of it.
fn covers(fonts: &Fonts, i: &Item) -> Rect {
    match i {
        Item::Rounded { r, .. } | Item::Rect { r, .. } | Item::Image { r, .. } => *r,
        Item::Text {
            text,
            px,
            weight,
            at,
            ..
        } => Rect::new(
            at.x,
            at.y - px,
            fonts.width_as_is(text, *px, *weight),
            px * 1.25,
        ),
        Item::Icon { center, size, .. } => {
            Rect::new(center.x - size * 0.5, center.y - size * 0.5, *size, *size)
        }
        Item::Line { a, b, .. } => Rect::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        ),
        Item::Poly { pts, .. } => {
            let lo = pts.iter().fold(Vec2::splat(f32::MAX), |m, p| m.min(*p));
            let hi = pts.iter().fold(Vec2::splat(f32::MIN), |m, p| m.max(*p));
            Rect::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y)
        }
    }
}

impl Layout<'_> {
    pub(crate) fn nav_natural(&self, e: &Element) -> f32 {
        match &e.kind {
            Kind::Box { children, .. } => {
                children.iter().map(|c| self.natural(c)).fold(0.0, f32::max)
                    + BOX_STRIPE
                    + BOX_PAD.0
                    + BOX_PAD.1
            }
            Kind::Stops { stops, size } => {
                let k = size / 13.0;
                stops
                    .iter()
                    .map(|s| {
                        NAME_X * k
                            + self.width(&s.name, *size, Weight::Medium)
                            + 2.0 * self.width("00:00", size * 12.0 / 13.0, Weight::Bold)
                            + 16.0 * k
                    })
                    .fold(0.0, f32::max)
                    .ceil()
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
                let tint = e.color.map(color).unwrap_or(self.colors.accent);
                self.rounded(r, BOX_R, tint.alpha(BOX_TINT), None);
                self.rounded(
                    Rect::new(r.x, r.y, BOX_STRIPE, r.h),
                    BOX_STRIPE * 0.5,
                    tint,
                    None,
                );
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
                let rail = e.color.map(color).unwrap_or(self.colors.accent);
                let row = STOP_ROW * size / 13.0;
                for (i, s) in stops.iter().enumerate() {
                    let rr = Rect::new(r.x, r.y + i as f32 * row, r.w, row);
                    self.stop(s, rr, *size, rail, i > 0, i + 1 < stops.len());
                }
            }
            Kind::Map { cards, overlay, .. } => {
                if self.out.map.is_none() {
                    self.out.map = Some((r, *cards));
                }
                for c in overlay {
                    self.map_card(c, r);
                }
            }
            _ => {}
        }
    }

    fn map_card(&mut self, c: &MapCard, map: Rect) {
        let room = map.pad(c.x, c.y);
        let natural = c
            .children
            .iter()
            .map(|e| self.natural(e))
            .fold(0.0, f32::max);
        let w = c
            .width
            .unwrap_or(natural + 2.0 * c.padding)
            .min(room.w)
            .max(1.0);
        let inner = (w - 2.0 * c.padding).max(1.0);
        let heights: Vec<f32> = c
            .children
            .iter()
            .map(|e| self.height(e, inner, false))
            .collect();
        let gaps = c.gap * c.children.len().saturating_sub(1) as f32;
        let h = heights.iter().sum::<f32>() + gaps + 2.0 * c.padding;
        let (ax, ay) = c.place.factors();
        let card = Rect::new(
            room.x + ax * (room.w - w),
            room.y + (ay * (room.h - h)).max(0.0),
            w,
            h,
        );
        let from = (self.out.items.len(), self.out.hits.len());
        let fill = c.background.map(color).unwrap_or(self.colors.card);
        self.rounded(card, c.radius.min(w * 0.5).min(h * 0.5), fill, None);
        let mut y = card.y + c.padding;
        for (i, (e, eh)) in c.children.iter().zip(heights).enumerate() {
            if i > 0 {
                y += c.gap;
            }
            self.emit(e, Rect::new(card.x + c.padding, y, inner, eh), false);
            y += eh;
        }
        self.clip(from, map);
    }

    // Keep hit indices stable because hover shapes refer to them.
    fn clip(&mut self, (items, hits): (usize, usize), r: Rect) {
        let cut = |b: Rect| {
            let (x, y) = (b.x.clamp(r.x, r.right()), b.y.clamp(r.y, r.bottom()));
            Rect::new(
                x,
                y,
                (b.right().min(r.right()) - x).max(0.0),
                (b.bottom().min(r.bottom()) - y).max(0.0),
            )
        };
        let inside = |b: Rect| {
            b.x >= r.x && b.y >= r.y && b.right() <= r.right() && b.bottom() <= r.bottom()
        };
        let laid = self.out.items.split_off(items);
        let kept: Vec<Item> = laid
            .into_iter()
            .filter_map(|i| match i {
                Item::Rounded {
                    r: b,
                    radius,
                    color,
                    hot,
                } => {
                    let b = cut(b);
                    (b.w > 0.0 && b.h > 0.0).then(|| Item::Rounded {
                        r: b,
                        radius: radius.min(b.w * 0.5).min(b.h * 0.5),
                        color,
                        hot,
                    })
                }
                Item::Rect { r: b, color } => {
                    let b = cut(b);
                    (b.w > 0.0 && b.h > 0.0).then_some(Item::Rect { r: b, color })
                }
                i => inside(covers(self.fonts, &i)).then_some(i),
            })
            .collect();
        self.out.items.extend(kept);
        for h in &mut self.out.hits[hits..] {
            h.r = cut(h.r);
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
            let drawn = if i + 1 == parts.len() {
                track.right() - x
            } else {
                w - 1.0
            };
            self.rounded(
                Rect::new(x, track.y, drawn.max(0.0), height),
                (height * 0.5).min(drawn * 0.5).max(0.0),
                color(*c),
                None,
            );
            x += w;
        }
        true
    }

    /// A badge: its text in its weight on a rounded fill, `BADGE_H` high, in the middle of `r`.
    pub(crate) fn badge(
        &mut self,
        e: &Element,
        r: Rect,
        text: &str,
        text_color: Option<Rgba>,
        wt: Weight,
    ) {
        let w = self.natural(e).min(r.w);
        let b = Rect::new(r.x, r.y + (r.h - BADGE_H) * 0.5, w, BADGE_H);
        let fill = e.color.map(color).unwrap_or(self.colors.accent);
        self.rounded(b, BADGE_H * 0.5, fill, None);
        let t = self.fit(text, BADGE_PX, wt, w - 2.0 * BADGE_PAD);
        let tw = self.width(&t, BADGE_PX, wt);
        let y = self.baseline(b.y, b.h, BADGE_PX, wt);
        self.text(
            t,
            BADGE_PX,
            wt,
            Vec2::new(b.x + (b.w - tw) * 0.5, y),
            text_color.map(color).unwrap_or(ink_on(fill, self.colors)),
        );
    }

    /// A circle (a rounded square as wide as it is high).
    fn dot(&mut self, at: Vec2, radius: f32, c: Color) {
        self.rounded(
            Rect::new(at.x - radius, at.y - radius, 2.0 * radius, 2.0 * radius),
            radius,
            c,
            None,
        );
    }

    /// One stop on the rail: its pin with the rail through it (grey where the bus has been),
    /// its name, and on the right its time, before it when the bus will be there, and its
    /// tag.
    fn stop(&mut self, s: &Stop, r: Rect, size: f32, rail: Color, above: bool, below: bool) {
        let k = size / 13.0;
        let (x, cy) = (r.x + RAIL_X * k, r.y + r.h * 0.5);
        let line = if s.state == StopState::Done {
            self.colors.rail_done
        } else {
            rail
        };
        if above {
            self.out.items.push(Item::Rect {
                r: Rect::new(x - 1.5 * k, r.y, 3.0 * k, r.h * 0.5),
                color: line,
            });
        }
        if below {
            self.out.items.push(Item::Rect {
                r: Rect::new(x - 1.5 * k, cy, 3.0 * k, r.h * 0.5),
                color: line,
            });
        }
        let center = Vec2::new(x, cy);
        match s.state {
            StopState::Done => self.dot(center, 4.0 * k, self.colors.rail_done),
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
                StopState::Done => (Weight::Medium, self.colors.faint),
                StopState::Now => (Weight::Bold, rail),
                StopState::Ahead => (Weight::Medium, self.colors.muted),
            };
            let tw = self.width(&s.time, tpx, wt);
            let base = self.baseline(r.y, lh, tpx, wt);
            self.text(s.time.clone(), tpx, wt, Vec2::new(right - tw, base), c);
            right -= tw + 8.0 * k;
        }
        if !s.expected.is_empty() {
            let ew = self.width(&s.expected, tpx, Weight::Bold);
            let base = self.baseline(r.y, lh, tpx, Weight::Bold);
            self.text(
                s.expected.clone(),
                tpx,
                Weight::Bold,
                Vec2::new(right - ew, base),
                s.expected_color.map(color).unwrap_or(self.colors.soft),
            );
            right -= ew + 8.0 * k;
        }
        if !s.tag.is_empty() {
            let px = 10.0 * k;
            let w = self.width(&s.tag, px, Weight::Medium) + 10.0 * k;
            let t = Rect::new(right - w, cy - 8.0 * k, w, 16.0 * k);
            self.rounded(t, 4.0 * k, self.colors.tag_fill, None);
            let base = self.baseline(t.y, t.h, px, Weight::Medium);
            self.text(
                s.tag.clone(),
                px,
                Weight::Medium,
                Vec2::new(t.x + 5.0 * k, base),
                self.colors.muted,
            );
            right -= w + 8.0 * k;
        }
        let (px, wt, c) = match s.state {
            StopState::Done => (12.5 * k, Weight::Regular, self.colors.faint),
            StopState::Now => (14.0 * k, Weight::Bold, self.colors.ink),
            StopState::Ahead => (13.0 * k, Weight::Medium, self.colors.soft),
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
        let frame = panel(
            r##"{ anchor = "navigator", frame = true, background = "#141A26", accent = "#FF0000", radius = 8 }"##,
        );
        let p = super::in_navigator(&frame, 360.0);
        assert_eq!(
            (p.width, p.background, p.accent, p.radius),
            (360.0, Some(Rgba([0, 0, 0, 0])), None, 0.0)
        );
        let board = panel(r##"{ anchor = "navigator", background = "#141A26" }"##);
        assert_eq!(
            super::in_navigator(&board, 360.0).background,
            Some(Rgba([0x14, 0x1A, 0x26, 255]))
        );
    }

    #[test]
    fn a_map_element_is_room_for_the_navigators_map() {
        let fonts = Fonts::new();
        let p = panel(
            r#"{ anchor = "navigator", frame = true, width = 360, padding = 10, gap = 6, children = {
            { type = "text", text = "Line 5", size = 14 },
            { type = "map", cards = false },
            { type = "map", height = 40 },
            { type = "button", id = "go", text = "Go" },
        } }"#,
        );
        let mut l = layout(&p, &fonts, 1.0);
        let (r, cards) = l.map.expect("the map");
        assert!(!cards);
        assert_eq!((r.x, r.w, r.h), (10.0, 340.0, (340.0 * 0.8f32).round()));
        assert!((r.y - (10.0 + 14.0 * LINE + 6.0)).abs() < 0.01, "{r:?}");
        let empty = |l: &Laid| {
            l.items.iter().all(|i| match i {
                Item::Rect { r: x, .. } | Item::Rounded { r: x, .. } => {
                    x.right() <= r.x || x.x >= r.right() || x.bottom() <= r.y || x.y >= r.bottom()
                }
                _ => true,
            })
        };
        assert!(empty(&l), "nothing is drawn where the map goes");
        assert_eq!(
            l.h,
            (10.0 + 14.0 * LINE + 6.0 + r.h + 6.0 + 40.0 + 6.0 + BUTTON_H + 10.0).ceil()
        );
        // (a map with a height of its own, elsewhere: room of that height)
        let q = panel(r#"{ children = { { type = "map", height = 50 } } }"#);
        assert_eq!(layout(&q, &fonts, 1.0).map.unwrap().0.h, 50.0);
        // cut off in the middle of the map: the part that is left
        cut(&mut l, r.y + 30.0);
        assert_eq!(l.map.unwrap().0.h, 30.0);
        cut(&mut l, r.y);
        assert!(l.map.is_none());
    }

    fn within(r: Rect, room: Rect) -> bool {
        r.x >= room.x - 0.01
            && r.y >= room.y - 0.01
            && r.right() <= room.right() + 0.01
            && r.bottom() <= room.bottom() + 0.01
    }

    /// A map 300 x 240, 40 from the top of a panel that fills the navigator's frame, with a
    /// card of `card`'s keys over it.
    fn map_with(card: &str) -> Panel {
        panel(&format!(
            r#"{{ anchor = "navigator", frame = true, width = 300, padding = 0, gap = 0, children = {{
            {{ type = "space", size = 40 }},
            {{ type = "map", height = 240, cards = false, overlay = {{ {card} }} }},
        }} }}"#
        ))
    }

    /// Version 8: a card of the plugin's own lies on the map, at its place there, kept from
    /// the map's edges; its shapes and texts are the panel's, drawn over the map.
    #[test]
    fn an_overlay_card_lies_on_the_map() {
        let fonts = Fonts::new();
        let l = layout(
            &map_with(
                r#"{ place = "top_right", width = 100, children = { { type = "text", text = "Markt" }, { type = "badge", text = "3 min" } } }"#,
            ),
            &fonts,
            1.0,
        );
        let (map, cards) = l.map.expect("the map");
        assert_eq!((map, cards), (Rect::new(0.0, 40.0, 300.0, 240.0), false));
        assert!(l.items.len() >= 4, "{:?}", l.items);
        let card = Rect::new(300.0 - 8.0 - 100.0, 40.0 + 8.0, 100.0, 240.0);
        for i in &l.items {
            assert!(within(super::covers(&fonts, i), card), "{i:?}");
        }
        // (the card behind what is in it: the game's card colour, its own corners)
        assert!(
            matches!(l.items[0], Item::Rounded { r, radius: 10.0, .. } if r.x == 192.0 && r.y == 48.0 && r.w == 100.0)
        );
        // a card as wide as what is in it, in the middle of the bottom
        let l = layout(
            &map_with(
                r#"{ place = "bottom", children = { { type = "text", text = "62 km/h" } } }"#,
            ),
            &fonts,
            1.0,
        );
        let Item::Rounded { r, .. } = l.items[0] else {
            panic!("{:?}", l.items)
        };
        let w = fonts.width_as_is("62 km/h", 14.0, Weight::Regular).ceil() + 20.0;
        assert!(
            (r.w - w).abs() < 0.01 && (r.x - (300.0 - w) * 0.5).abs() < 0.01,
            "{r:?}"
        );
        assert!((r.bottom() - (40.0 + 240.0 - 8.0)).abs() < 0.01, "{r:?}");
        // a map elsewhere is empty room: the cards are laid out in it all the same
        let q = panel(
            r#"{ width = 200, padding = 0, children = { { type = "map", height = 100, overlay = { { place = "center", children = { { type = "text", text = "x" } } } } } } }"#,
        );
        let l = layout(&q, &fonts, 1.0);
        assert!(
            !l.items.is_empty()
                && l.items
                    .iter()
                    .all(|i| within(super::covers(&fonts, i), Rect::new(0.0, 0.0, 200.0, 100.0)))
        );
    }

    /// A card larger than the map is cut to it: no wider than the map less its offsets, and
    /// what is below the map's bottom is not drawn and takes no click.
    #[test]
    fn an_overlay_card_stays_inside_the_map() {
        let fonts = Fonts::new();
        let buttons = (1..=12)
            .map(|i| format!(r#"{{ type = "button", id = "b{i}", text = "Button {i}" }}"#))
            .collect::<Vec<_>>()
            .join(", ");
        let l = layout(
            &map_with(&format!(
                r#"{{ place = "bottom_left", width = 500, children = {{ {{ type = "text", text = "{}" }}, {buttons} }} }}"#,
                "word ".repeat(60)
            )),
            &fonts,
            1.0,
        );
        let map = Rect::new(0.0, 40.0, 300.0, 240.0);
        assert!(l.items.len() > 4);
        for i in &l.items {
            assert!(within(super::covers(&fonts, i), map), "{i:?}");
        }
        for h in &l.hits {
            assert!(within(h.r, map), "{h:?}");
        }
        let Item::Rounded { r, .. } = l.items[0] else {
            panic!("{:?}", l.items)
        };
        assert_eq!(
            (r.x, r.y, r.w, r.bottom()),
            (8.0, 48.0, 300.0 - 16.0, 280.0)
        );
        // (the buttons that do not fit are not drawn and take no click)
        assert!(l.hits.iter().any(|h| h.r.h == 0.0));
        assert_eq!(
            l.hits.iter().filter(|h| h.r.h > 0.0).count(),
            l.items
                .iter()
                .filter(|i| matches!(i, Item::Rounded { hot: Some(_), .. }))
                .count()
        );
    }

    /// What in a card takes a click takes it as in the panel: the topmost hit there.
    #[test]
    fn an_overlay_button_clicks() {
        let fonts = Fonts::new();
        let l = layout(
            &map_with(
                r#"{ place = "bottom_right", children = { { type = "button", id = "next", text = "Next stop" } } }"#,
            ),
            &fonts,
            1.0,
        );
        let i = l
            .hits
            .iter()
            .position(|h| h.element.as_deref() == Some("next"))
            .expect("the button's hit");
        let r = l.hits[i].r;
        assert!(
            within(r, Rect::new(0.0, 40.0, 300.0, 240.0)) && r.h == BUTTON_H,
            "{r:?}"
        );
        assert_eq!(hit_at(&l, r.center()), Some(i));
        assert_eq!(l.hits[i].kind, HitKind::Click);
        // (on the map beside the card: none, the press is the navigator's)
        assert_eq!(hit_at(&l, Vec2::new(20.0, 60.0)), None);
    }

    /// A map without cards, or with an empty list of them, is laid out as it always was.
    #[test]
    fn a_map_without_overlay_lays_out_as_before() {
        let fonts = Fonts::new();
        let src = |overlay: &str| {
            panel(&format!(
                r#"{{ anchor = "navigator", frame = true, width = 360, padding = 10, gap = 6, children = {{
                {{ type = "text", text = "Line 5", size = 14 }},
                {{ type = "map", cards = false {overlay} }},
                {{ type = "map", height = 40 }},
                {{ type = "button", id = "go", text = "Go" }},
            }} }}"#
            ))
        };
        let before = layout(&src(""), &fonts, 1.0);
        let empty = layout(&src(", overlay = {}"), &fonts, 1.0);
        assert_eq!(
            (before.w, before.h, before.map, &before.items, &before.hits),
            (empty.w, empty.h, empty.map, &empty.items, &empty.hits)
        );
        assert_eq!(
            before.map.unwrap().0,
            Rect::new(
                10.0,
                10.0 + 14.0 * LINE + 6.0,
                340.0,
                (340.0 * 0.8f32).round()
            )
        );
        assert_eq!(
            before.h,
            (10.0
                + 14.0 * LINE
                + 6.0
                + (340.0 * 0.8f32).round()
                + 6.0
                + 40.0
                + 6.0
                + BUTTON_H
                + 10.0)
                .ceil()
        );
    }

    /// A card taller than the map is cut off at the map's bottom edge: what shows of it takes
    /// its clicks, and a press just outside what shows - below the edge, where the card would
    /// go on if it were not cut off, or just beside it - is no part of the plugin's: there the
    /// press stays the navigator's (its map dragged, the city map opened).
    #[test]
    fn a_card_cut_off_at_the_map_s_edge_takes_no_click_past_it() {
        let fonts = Fonts::new();
        let buttons = (1..=12)
            .map(|i| format!(r#"{{ type = "button", id = "b{i}", text = "Button {i}" }}"#))
            .collect::<Vec<_>>()
            .join(", ");
        // (a map 300 x 240, 40 from the panel's top, with room of the panel's below it; the
        // card 120 wide at 8, 8 in it: its buttons 34 high, 6 apart, from 48 + 10 down)
        let src = format!(
            r#"{{ anchor = "navigator", frame = true, grab = true, width = 300, padding = 0, gap = 0, children = {{
            {{ type = "space", size = 40 }},
            {{ type = "map", height = 240, cards = false, overlay = {{
                {{ place = "top_left", x = 8, y = 8, width = 120, padding = 10, gap = 6, children = {{ {buttons} }} }},
            }} }},
            {{ type = "space", size = 60 }},
        }} }}"#
        );
        let p = panel(&src);
        let l = layout(&p, &fonts, 1.0);
        let map = Rect::new(0.0, 40.0, 300.0, 240.0);
        assert_eq!(l.map.map(|m| m.0), Some(map));
        assert!(l.h >= 340.0, "the panel goes on below the map: {}", l.h);
        // (the card's background: from its top to the map's bottom, not to its own 542)
        let Item::Rounded { r, .. } = l.items[0] else {
            panic!("{:?}", l.items)
        };
        assert_eq!(
            (r.x, r.y, r.w, r.bottom()),
            (8.0, 48.0, 120.0, map.bottom())
        );
        let element = |p: Vec2| hit_at(&l, p).and_then(|i| l.hits[i].element.clone());
        let hit = |id: &str| {
            l.hits
                .iter()
                .find(|h| h.element.as_deref() == Some(id))
                .map(|h| h.r)
                .unwrap()
        };
        // a button wholly on the map takes its click
        assert_eq!(hit("b1"), Rect::new(18.0, 58.0, 100.0, BUTTON_H));
        assert_eq!(element(hit("b1").center()).as_deref(), Some("b1"));
        // the button across the edge: its part on the map takes it, up to the edge
        assert_eq!(hit("b6"), Rect::new(18.0, 258.0, 100.0, 22.0));
        assert_eq!(element(Vec2::new(68.0, 270.0)).as_deref(), Some("b6"));
        assert_eq!(element(Vec2::new(68.0, 279.5)).as_deref(), Some("b6"));
        // just below the edge, where the rest of it and the next button would be: none
        assert_eq!(element(Vec2::new(68.0, 280.5)), None);
        assert_eq!(element(Vec2::new(68.0, 291.0)), None);
        assert_eq!(element(Vec2::new(68.0, 315.0)), None);
        assert_eq!(hit("b7").h, 0.0);
        // just beside the card, and beside the button on the card's own background: none
        let y = hit("b1").center().y;
        for x in [7.5, 17.5, 118.5, 128.5] {
            assert_eq!(element(Vec2::new(x, y)), None, "{x}");
        }
        // as the game asks it, without the mouse: the panel takes the press on what shows of
        // its buttons, and leaves it to the navigator everywhere else
        let mut panels = PluginPanels {
            scale: 1.0,
            ..Default::default()
        };
        let mut ui = UiState::default();
        ui.set(1, "phone", p.clone()).unwrap();
        let laid = Laid { flat: true, ..l };
        let card = Card {
            revision: 0,
            scale: 1.0,
            backdrop: 1.0,
            pal: Palette::OPENBUS,
            laid,
            tex: None,
            drawn: None,
            origin: Vec2::ZERO,
            y: None,
            seen: panels.frame,
            full_h: 0.0,
            room: f32::INFINITY,
        };
        panels.cards.insert(Key::Panel(1, "phone".into()), card);
        assert!(panels.grabs_at(&ui, 68.0, y));
        assert!(panels.grabs_at(&ui, 68.0, 279.5));
        for (x, y) in [
            (68.0, 280.5),
            (68.0, 315.0),
            (128.5, y),
            (118.5, y),
            (150.0, 150.0),
        ] {
            assert!(!panels.grabs_at(&ui, x, y), "{x}, {y}");
        }
    }
}
