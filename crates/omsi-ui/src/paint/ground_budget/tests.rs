use super::*;
use crate::paint::{Color, Painter};
use glam::Vec3;

fn quad(p: &mut Painter, lo: Vec2, hi: Vec2, color: Color) {
    p.world_poly(
        &[
            lo.extend(0.0),
            Vec3::new(hi.x, lo.y, 0.0),
            hi.extend(0.0),
            Vec3::new(lo.x, hi.y, 0.0),
        ],
        color,
    );
}

fn covers(verts: &[Vertex], point: Vec2, color: Color) -> bool {
    verts.chunks_exact(3).any(|t| {
        if t[0]
            .color
            .iter()
            .zip(color.0)
            .any(|(a, b)| (*a - b).abs() > 1e-5)
        {
            return false;
        }
        let p: [Vec2; 3] = std::array::from_fn(|i| Vec2::new(t[i].pos[0], t[i].pos[1]));
        let signs = [0, 1, 2].map(|i| (p[(i + 1) % 3] - p[i]).perp_dot(point - p[i]));
        signs.iter().all(|s| *s >= -1e-5) || signs.iter().all(|s| *s <= 1e-5)
    })
}

#[test]
fn a_ground_that_fits_is_byte_for_byte_the_original_geometry() {
    let (mut exact, mut bounded) = (Painter::new(), Painter::with_ground_budget(120, 10.0, 0.25));
    for p in [&mut exact, &mut bounded] {
        quad(p, Vec2::splat(-8.0), Vec2::splat(8.0), Color::hex(0x304020));
        p.ribbon(
            &[Vec3::new(-7.0, -2.0, 0.0), Vec3::new(6.0, 2.0, 0.0)],
            1.0,
            2.0,
            Color::WHITE,
            true,
        );
    }
    assert!(!bounded.ground_reduced());
    assert_eq!(bounded.finish_ground(), exact.verts);
}

#[test]
fn a_full_land_layer_keeps_late_roads_at_both_ends_of_the_map() {
    let green = Color::hex(0x408020);
    let mut p = Painter::with_ground_budget(96, 8.0, 0.1);
    // The old first-come budget filled here and discarded every later road.
    for _ in 0..16 {
        quad(&mut p, Vec2::splat(-8.0), Vec2::splat(8.0), green);
    }
    for x in [-6.0, 6.0] {
        p.ribbon(
            &[Vec3::new(x, -7.0, 0.0), Vec3::new(x, 7.0, 0.0)],
            0.2,
            0.0,
            Color::WHITE,
            false,
        );
    }
    assert!(p.ground_reduced());
    assert!(
        !p.at_vertex_limit(),
        "a coarsened map must still accept later layers"
    );
    let verts = p.finish_ground();
    assert!(verts.len() <= 96);
    for x in [-6.0, 6.0] {
        assert!(covers(&verts, Vec2::new(x, 0.0), Color::WHITE));
    }
    assert!(
        covers(&verts, Vec2::ZERO, green),
        "land between the roads remains"
    );
}

#[test]
fn even_a_checkerboard_never_exceeds_the_vertex_budget() {
    for limit in [6, 17, 96, 260_000] {
        let mut grid = Grid::new(limit, 500.0, 1.0);
        for y in 0..grid.side {
            for x in 0..grid.side {
                grid.cells[y * grid.side + x] = if (x + y) % 2 == 0 {
                    [1.0; 4]
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                };
            }
        }
        let verts = grid.vertices();
        assert!(verts.len() <= limit, "{} > {limit}", verts.len());
        assert_eq!(verts.len() % 6, 0);
        assert!(
            verts
                .iter()
                .all(|v| v.pos.iter().all(|q| q.is_finite() && q.abs() <= 500.0))
        );
    }
}

#[test]
fn winding_clipping_and_nonfinite_input_do_not_drop_a_late_layer() {
    let mut p = Painter::with_ground_budget(24, 10.0, 1.0);
    for _ in 0..5 {
        quad(&mut p, Vec2::splat(-1e6), Vec2::splat(1e6), Color::BLACK);
    }
    p.world_poly(&[Vec3::splat(f32::NAN), Vec3::X, Vec3::Y], Color::WHITE);
    p.world_poly(
        &[
            Vec3::new(-10.0, -10.0, 0.0),
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::new(10.0, -10.0, 0.0),
        ],
        Color::WHITE,
    );
    let verts = p.finish_ground();
    assert!(verts.len() <= 24);
    assert!(covers(&verts, Vec2::ZERO, Color::WHITE));
    assert!(verts.iter().all(|v| v.pos.iter().all(|x| x.is_finite())));
}

#[test]
fn clearing_a_reduced_painter_starts_an_exact_ground_again() {
    let mut p = Painter::with_ground_budget(6, 10.0, 1.0);
    for _ in 0..2 {
        quad(&mut p, Vec2::ZERO, Vec2::ONE, Color::WHITE);
    }
    assert!(p.ground_reduced());
    p.clear();
    quad(&mut p, Vec2::ZERO, Vec2::ONE, Color::BLACK);
    assert!(!p.ground_reduced());
    assert_eq!(p.finish_ground().len(), 6);
}
