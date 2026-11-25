//! The software cursor (#1247): the shapes the game draws itself when the player asks it to.
//!
//! Moving the hardware cursor under Windows makes the compositor move the plane the game's
//! picture is on; with G-SYNC and no MPO plane to fall back on, that stops G-SYNC. The way out
//! is to leave the hardware cursor alone - hidden - and draw an arrow of our own into the
//! frame. This module is the drawing: a few polygons per cursor shape, the hotspot at `at`, in
//! pixels, scaled by the interface's scale.
//!
//! Each shape is a list of **convex** parts with the hotspot at the origin, growing to +x and
//! +y: the painter fills a convex polygon with a triangle fan, so a notched arrow is two
//! pieces rather than one concave outline.

use glam::Vec2;

use crate::paint::{Color, Painter};

/// The arrow's head: from the tip to the two wings.
const ARROW_HEAD: &[[f32; 2]] = &[[0.00, 0.00], [0.02, 0.78], [0.30, 0.52]];
/// The arrow's tail, cut in where the head's wing is.
const ARROW_TAIL: &[[f32; 2]] = &[[0.14, 0.54], [0.30, 0.54], [0.44, 0.98], [0.26, 1.00]];
/// A pointing hand: a palm with the index finger up (a convex outline about a hand's shape).
const HAND: &[[f32; 2]] = &[
    [0.34, 0.00],
    [0.55, 0.10],
    [0.68, 0.30],
    [0.78, 0.54],
    [0.70, 0.80],
    [0.48, 0.98],
    [0.24, 0.92],
    [0.10, 0.68],
    [0.14, 0.40],
    [0.20, 0.16],
];
/// The crosshair: two bars.
const CROSS_V: &[[f32; 2]] = &[[0.42, 0.00], [0.58, 0.00], [0.58, 1.00], [0.42, 1.00]];
const CROSS_H: &[[f32; 2]] = &[[0.00, 0.42], [1.00, 0.42], [1.00, 0.58], [0.00, 0.58]];
/// The resize cursor: a double arrow up and down.
const MOVE_TOP: &[[f32; 2]] = &[[0.50, 0.00], [0.80, 0.30], [0.20, 0.30]];
const MOVE_BOTTOM: &[[f32; 2]] = &[[0.50, 1.00], [0.20, 0.70], [0.80, 0.70]];
const MOVE_SHAFT: &[[f32; 2]] = &[[0.42, 0.28], [0.58, 0.28], [0.58, 0.72], [0.42, 0.72]];
const MOVE_LEFT: &[[f32; 2]] = &[[0.00, 0.50], [0.30, 0.20], [0.30, 0.80]];
const MOVE_RIGHT: &[[f32; 2]] = &[[1.00, 0.50], [0.70, 0.20], [0.70, 0.80]];
const MOVE_HSHAFT: &[[f32; 2]] = &[[0.28, 0.42], [0.72, 0.42], [0.72, 0.58], [0.28, 0.58]];
/// A closed hand: a fist.
const GRAB: &[[f32; 2]] = &[
    [0.50, 0.06],
    [0.78, 0.18],
    [0.92, 0.46],
    [0.82, 0.78],
    [0.54, 0.96],
    [0.26, 0.88],
    [0.08, 0.58],
    [0.16, 0.26],
];

/// The convex parts of cursor `kind` (the kinds `App::set_cursor_kind` uses), the arrow for
/// anything else.
pub fn outline(kind: u8) -> &'static [&'static [[f32; 2]]] {
    match kind {
        1 => &[HAND],
        2 => &[CROSS_V, CROSS_H],
        3 => &[
            MOVE_TOP,
            MOVE_BOTTOM,
            MOVE_LEFT,
            MOVE_RIGHT,
            MOVE_SHAFT,
            MOVE_HSHAFT,
        ],
        4 => &[MOVE_TOP, MOVE_BOTTOM, MOVE_SHAFT],
        5 => &[GRAB],
        _ => &[ARROW_HEAD, ARROW_TAIL],
    }
}

/// The size of the cursor in pixels for an interface `scale`: about 20 px at a scale of 1 (the
/// size Windows draws at 100 %), and never so small or so large a screen cannot show it.
pub fn size(scale: f32) -> f32 {
    (20.0 * scale).clamp(8.0, 96.0)
}

/// The cursor's shape in pixels around `at`, at `scale`, each part shrunk towards the shape's
/// own centre by `grow` (1 = the shape itself, a little less = the white inside of the border).
pub fn parts(kind: u8, at: Vec2, scale: f32, grow: f32) -> Vec<Vec<Vec2>> {
    let size = size(scale);
    let shape = outline(kind);
    // (the centre of the whole shape, so the border is even on every side)
    let (mut cx, mut cy, mut n) = (0.0f32, 0.0f32, 0.0f32);
    for part in shape {
        for p in *part {
            cx += p[0];
            cy += p[1];
            n += 1.0;
        }
    }
    let (cx, cy) = (cx / n, cy / n);
    shape
        .iter()
        .map(|part| {
            part.iter()
                .map(|p| at + Vec2::new(cx + (p[0] - cx) * grow, cy + (p[1] - cy) * grow) * size)
                .collect()
        })
        .collect()
}

/// Draw the cursor `kind` at `at` (pixels) at `scale` into `p`: a dark outline with a white
/// body inside it, so it reads on a light road and on a dark one.
pub fn paint(p: &mut Painter, kind: u8, at: Vec2, scale: f32) {
    for part in parts(kind, at, scale, 1.0) {
        p.convex(&part, Color::BLACK);
    }
    for part in parts(kind, at, scale, 0.66) {
        p.convex(&part, Color::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every point of every part of `kind`, at `grow`.
    fn points(kind: u8, at: Vec2, scale: f32, grow: f32) -> Vec<Vec2> {
        parts(kind, at, scale, grow).into_iter().flatten().collect()
    }

    #[test]
    fn every_kind_is_made_of_polygons_that_fit_their_box() {
        for kind in 0..=5u8 {
            let shape = outline(kind);
            assert!(!shape.is_empty(), "kind {kind} has parts");
            for part in shape {
                assert!(part.len() >= 3, "kind {kind}: a polygon, not a line");
                for p in *part {
                    assert!(
                        (-0.01..=1.01).contains(&p[0]) && (-0.01..=1.01).contains(&p[1]),
                        "kind {kind}: {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_part_is_convex() {
        // (the painter fills a fan: a concave part would come out with a piece missing)
        fn cross(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
            (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
        }
        for kind in 0..=5u8 {
            for part in outline(kind) {
                let mut sign = 0.0f32;
                for i in 0..part.len() {
                    let (a, b, c) = (
                        part[i],
                        part[(i + 1) % part.len()],
                        part[(i + 2) % part.len()],
                    );
                    let z = cross(a, b, c);
                    if z.abs() < 1e-6 {
                        continue;
                    }
                    if sign == 0.0 {
                        sign = z.signum();
                    } else {
                        assert_eq!(
                            z.signum(),
                            sign,
                            "kind {kind}: part {part:?} turns the same way everywhere"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_shapes_differ_from_one_another() {
        for a in 0..=5u8 {
            for b in (a + 1)..=4u8 {
                assert_ne!(outline(a), outline(b), "kinds {a} and {b}");
            }
        }
    }

    #[test]
    fn the_hotspot_is_where_the_click_is() {
        let at = Vec2::new(400.0, 300.0);
        // (the arrow's tip is the hotspot)
        assert!(points(0, at, 1.0, 1.0)[0].distance(at) < 1e-3);
        // (every shape is drawn within a cursor's size of it)
        for kind in 0..=5u8 {
            for p in points(kind, at, 1.0, 1.0) {
                assert!(
                    p.distance(at) <= size(1.0) * 1.5,
                    "kind {kind}: {p:?} is off the hotspot"
                );
            }
        }
    }

    #[test]
    fn the_size_follows_the_interface_scale() {
        let at = Vec2::ZERO;
        let span = |scale: f32| {
            points(0, at, scale, 1.0)
                .iter()
                .fold(0.0f32, |m, p| m.max(p.length()))
        };
        assert!(
            (span(2.0) - span(1.0) * 2.0).abs() < 0.5,
            "twice the scale, twice the size"
        );
        assert!(
            size(0.0) >= 8.0 && size(100.0) <= 96.0,
            "a cursor a screen can still show"
        );
    }

    #[test]
    fn the_white_body_lies_inside_the_dark_border() {
        let at = Vec2::new(10.0, 10.0);
        let border = parts(0, at, 1.0, 1.0);
        let body = parts(0, at, 1.0, 0.66);
        // (the centre of the whole shape, as `parts` shrinks towards it)
        let all: Vec<Vec2> = border.iter().flatten().copied().collect();
        let c = all.iter().fold(Vec2::ZERO, |a, p| a + *p) / all.len() as f32;
        for (b, w) in border.iter().zip(&body) {
            for (bp, wp) in b.iter().zip(w) {
                assert!(
                    wp.distance(c) < bp.distance(c),
                    "the body is inside the border: {wp:?} / {bp:?}"
                );
            }
        }
        let widest = |ps: &[Vec<Vec2>]| -> f32 {
            ps.iter()
                .flatten()
                .fold(0.0f32, |m, p| m.max(p.distance(at)))
        };
        assert!(
            widest(&border) - widest(&body) > 1.0,
            "and the border is visible: {} px",
            widest(&border) - widest(&body)
        );
    }
}
