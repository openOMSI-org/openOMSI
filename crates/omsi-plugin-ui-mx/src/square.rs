//! A tile of its icon alone, as OMSI 2's quick menu has them (`Kind::Tile` with no text):
//! a square, the icon white on grey or on the tile's own colour, ringed in the accent when it
//! has the keys (`focus`), greyed and taking no clicks when it is `off`.

use super::{color, Hit, HitKind, Item, Layout, INK};
use omsi_plugin::ui::Element;
use omsi_ui::{Color, Rect};

/// How wide the square is; the icon is half of it.
pub(crate) const SQUARE: f32 = 84.0;
const SQUARE_R: f32 = 10.0;
const RING: f32 = 2.0;
// (solid greys: see-through ink blends in linear light and stays bright)
const OFF_INK: Color = Color::rgba(84, 86, 92, 1.0);
const OFF_TILE: Color = Color::rgba(46, 48, 54, 0.3);

impl Layout<'_> {
    /// A square of its icon alone, white on grey or on its own colour (lit, as OMSI 2's
    /// orange tiles), ringed in the accent when it has the keys, greyed when it is off.
    pub(crate) fn square_tile(&mut self, e: &Element, r: Rect, icon: &str, focus: bool, off: bool) {
        let side = r.w.min(r.h);
        let b = Rect::new(r.x, r.y + (r.h - side) * 0.5, r.w, side);
        if focus {
            let ring = Rect::new(b.x - RING, b.y - RING, b.w + 2.0 * RING, b.h + 2.0 * RING);
            self.rounded(ring, SQUARE_R + RING, self.colors.accent, None);
        }
        let hit = (!off).then(|| {
            self.out.hits.push(Hit { r: b, element: e.id.clone(), kind: HitKind::Click });
            self.out.hits.len() - 1
        });
        let fill = if off { if self.colors.ink == INK { OFF_TILE } else { self.colors.tile.alpha(0.3) } } else { e.color.map(color).unwrap_or(self.colors.tile) };
        // (inside the ring a thin dark edge keeps the tile apart from it; solid, or the
        // ring would show through the see-through tile)
        if focus {
            let inner = Rect::new(b.x + 1.0, b.y + 1.0, b.w - 2.0, b.h - 2.0);
            self.rounded(b, SQUARE_R, Color([self.colors.card.0[0], self.colors.card.0[1], self.colors.card.0[2], 1.0]), None);
            self.rounded(inner, SQUARE_R - 1.0, fill, hit);
        } else {
            self.rounded(b, SQUARE_R, fill, hit);
        }
        // (a solid grey: see-through ink blends in linear light and stays bright)
        let ink = if off { if self.colors.ink == INK { OFF_INK } else { self.colors.muted } } else if e.color.is_some() { INK } else { self.colors.ink };
        self.out.items.push(Item::Icon {
            name: icon.to_string(),
            center: b.center(),
            size: (side * 0.5).round(),
            color: ink,
        });
    }
}
