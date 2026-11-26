//! Tests of the wipers' water film on the glass.

use super::*;

#[test]
fn the_panes_are_repainted_in_turn_not_all_in_one_frame() {
    for (fps, every) in [
        (144.0, 1.0 / 30.0),
        (60.0, 1.0 / 30.0),
        (30.0, 1.0 / 15.0),
        (60.0, 1.0 / 15.0),
        (240.0, 1.0 / 15.0),
    ] {
        let count = 12;
        let mut phases: Vec<f32> = (0..count).map(|k| first_paint_phase(k, count)).collect();
        let mut paints = vec![0; count];
        let mut most = 0;
        for frame in 0..(fps as usize * 4) {
            let mut now = 0;
            for (phase, painted) in phases.iter_mut().zip(&mut paints) {
                if repaint_due(phase, 1.0 / fps, every) {
                    *painted += 1;
                    now += 1;
                }
            }
            if frame > 0 {
                most = most.max(now);
            }
        }
        let fair = (count as f32 / (fps * every)).ceil() as usize + 1;
        assert!(most <= fair, "{fps} fps: {most} panes in one frame");
        let rate = (4.0 / every) as i32;
        assert!(
            paints.iter().all(|&n| (n - rate).abs() <= 2),
            "{fps} fps: {paints:?}"
        );
    }
}

#[test]
fn sweep_edges_have_partial_coverage_without_a_shared_diagonal_seam() {
    let a = Vec3::ZERO;
    let b = Vec3::Z;
    let c = Vec3::X + Vec3::Z;
    let d = Vec3::X;
    let triangles = [
        SweepTriangle::new(a, b, c, false).unwrap(),
        SweepTriangle::new(a, c, d, false).unwrap(),
    ];
    let cell = (Vec3::X + Vec3::Z) / SIZE as f32;
    let coverage = |p| {
        triangles
            .iter()
            .map(|t| t.coverage(p, t.edge_filter(cell)))
            .sum::<f32>()
            .min(1.0)
    };
    // Move by fractions of a mask cell across the outer edge.
    for fraction in [-0.5, -0.25, 0.0, 0.25, 0.5] {
        let p = Vec3::new(fraction / SIZE as f32, 0.03, 0.5);
        assert!((coverage(p) - (fraction + 0.5)).abs() < 1e-5);
    }
    for t in [0.25, 0.5, 0.75] {
        assert!((coverage(Vec3::new(t, 0.03, t)) - 1.0).abs() < 1e-5);
    }
    assert_eq!(coverage(Vec3::new(0.5, 0.3, 0.5)), 0.0);
}

#[test]
fn offset_glass_keeps_projected_endpoints_and_catches_crossing_runoff() {
    let sweep = SweepTriangle::new(
        Vec3::ZERO,
        Vec3::new(0.0, 0.04, 1.0),
        Vec3::new(0.1, 0.04, 1.0),
        false,
    )
    .unwrap();
    assert!(sweep.contains(Vec3::new(0.045, 0.1, 0.5)));
    assert!(!sweep.contains(Vec3::new(0.09, 0.1, 0.5)));
    assert!(sweep.crosses(Vec3::new(0.045, 0.1, 1.1), Vec3::new(0.045, 0.1, 0.2)));
    assert!(!sweep.crosses(Vec3::new(0.2, 0.1, 1.1), Vec3::new(0.2, 0.1, 0.2)));
    assert!(!sweep.contains(Vec3::new(0.045, 0.4, 0.5)));
    assert!(!sweep.crosses(Vec3::NAN, Vec3::ZERO));
}

#[test]
fn a_wipe_leaves_brief_sheen_then_rewets_gradually_without_a_frame_rate_dependency() {
    assert!(advance_wetness(WIPED_FILM, 0.1, 0.1) < 0.0);
    let after_two_seconds = advance_wetness(WIPED_FILM, 0.1, 2.0);
    assert!(after_two_seconds > 0.1 && after_two_seconds < 0.2);
    let mut stepped = WIPED_FILM;
    for _ in 0..120 {
        stepped = advance_wetness(stepped, 0.1, 1.0 / 60.0);
    }
    assert!((stepped - after_two_seconds).abs() < 1e-5);
    assert_eq!(advance_wetness(0.0, -0.025, 2.0), 0.0);
    assert_eq!(advance_wetness(WIPED_FILM, -0.025, 2.0), 0.0);
    assert_eq!(encode_wetness(0.0), 8);
    assert!(encode_wetness(WIPED_FILM) < 8);
    assert_eq!(encode_wetness(2.0), 255);
}

#[test]
fn collected_water_moves_down_or_with_airflow_without_moving_pinned_beads() {
    let points: Vec<_> = (0..SIZE * SIZE)
        .map(|i| {
            Vec3::new(
                (i % SIZE) as f32 / SIZE as f32,
                0.0,
                (i / SIZE) as f32 / SIZE as f32,
            )
        })
        .collect();
    let source = SIZE * 64 + 64;
    let mut scratch = vec![0.0; SIZE * SIZE];
    for (velocity, neighbour) in [(-Vec3::Z, source - SIZE), (Vec3::X, source + 1)] {
        let mut wet = vec![0.5; SIZE * SIZE];
        wet[source] = 1.6;
        let before = wet.iter().sum::<f32>();
        drain_water(
            &mut wet,
            &points,
            [0.0, 0.0, 1.0, 1.0],
            velocity,
            0.5 / SIZE as f32,
            &mut scratch,
        );
        assert!((wet[source] - 1.3).abs() < 1e-5);
        assert!((wet[neighbour] - 0.8).abs() < 1e-5);
        assert!((wet.iter().sum::<f32>() - before).abs() < 0.01);
    }
    let mut wet = vec![0.5; SIZE * SIZE];
    drain_water(
        &mut wet,
        &points,
        [0.0, 0.0, 1.0, 1.0],
        -Vec3::Z,
        10.0,
        &mut scratch,
    );
    assert!(wet.iter().all(|w| *w == 0.5));
    wet[0] = 1.6;
    let before = wet.iter().sum::<f32>();
    drain_water(
        &mut wet,
        &points,
        [0.0, 0.0, 1.0, 1.0],
        -Vec3::Z,
        10.0,
        &mut scratch,
    );
    assert!(wet.iter().sum::<f32>() < before);
    assert!(wet.iter().all(|w| w.is_finite() && *w >= 0.0 && *w <= 2.0));
}

#[test]
fn combined_mesh_contact_follows_the_blade_instead_of_the_arm() {
    let mut points = Vec::new();
    for i in 0..120 {
        let t = i as f32 / 119.0;
        points.push(Vec3::new(t, 0.0, 0.5));
        points.push(Vec3::new(t, 0.01, 0.5));
        points.push(Vec3::new(0.5 + t * 0.5, 0.02, 0.5 - t * 0.4));
    }
    let ends = combined_blade_ends(&points).unwrap();
    assert!(ends[0].distance(ends[1]) > 0.98);
    assert!(ends.iter().all(|p| (p.z - 0.5).abs() < 0.01));
}

#[test]
fn only_the_blades_traversed_strip_is_cleared_in_both_directions() {
    let start = [Vec3::new(0.0, 0.03, 0.0), Vec3::new(0.0, 0.03, 1.0)];
    let end = start.map(|p| p + Vec3::X * 0.4);
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::X + Vec3::Z],
        indices: vec![0, 1, 2, 1, 3, 2],
        ranges: vec![(0, 6, 0)],
        ..Default::default()
    };
    let (points, bounds) = film_points(&data, 0).unwrap();
    for (from, to) in [(start, end), (end, start)] {
        let mut wet = vec![1.0; SIZE * SIZE];
        assert!(wipe(&mut wet, &points, bounds, from, to));
        for (&p, &wet) in points.iter().zip(&wet) {
            if p.x < 0.4 - 0.5 / SIZE as f32 {
                assert!(wet <= 0.004, "{p:?}: {wet}");
            } else if p.x > 0.435 || (to == start && p.x > 0.4 + 0.5 / SIZE as f32) {
                assert_eq!(wet, 1.0, "{p:?}");
            }
        }
        if to == end {
            assert!(points.iter().zip(&wet).any(|(p, w)| p.x > 0.4 && *w > 1.0));
        }
    }
    let mut wet = vec![1.0; SIZE * SIZE];
    wipe(
        &mut wet,
        &points,
        bounds,
        end,
        start.map(|p| p + Vec3::X * 0.2),
    );
    assert!(points.iter().zip(&wet).any(|(p, w)| p.x < 0.2 && *w > 1.0));
    assert!(wet.iter().all(|w| *w <= 2.0));
    let mut dry = vec![0.0; SIZE * SIZE];
    assert!(!wipe(&mut dry, &points, bounds, start, end));
    assert!(dry.iter().all(|w| *w == 0.0));
    let mut wet = vec![0.6; SIZE * SIZE];
    assert!(!wipe(&mut wet, &points, bounds, start, start));
    assert!(wet.iter().all(|w| *w == 0.6));
    let off_glass = start.map(|p| p - Vec3::Y * 0.3);
    wipe(
        &mut wet,
        &points,
        bounds,
        off_glass,
        off_glass.map(|p| p + Vec3::X * 0.4),
    );
    assert!(wet.iter().all(|w| *w == 0.6));
}

#[test]
fn pooled_water_becomes_runoff_and_stays_in_the_bank_if_capacity_is_full() {
    let bounds = [0.0, 0.0, 1.0, 1.0];
    let mut points = vec![Vec3::NAN; SIZE * SIZE];
    let i = SIZE * 64 + 64;
    points[i] = Vec3::new(0.5, 0.0, 0.5);
    let mut wet = vec![0.0; SIZE * SIZE];
    let mut drops = Drops::new(Vec2::ONE, 0.0, 1);
    wet[i] = 1.8;
    release_runoff(&mut wet, &points, bounds, &mut drops, -Vec2::Y * 0.12);
    assert_eq!(wet[i], WIPED_FILM);
    assert_eq!(drops.drops.len(), 1);
    assert!((drops.drops[0].water - 0.8 * 20000.0 / (SIZE * SIZE) as f32).abs() < 1e-5);
    assert!(drops.drops[0].velocity.y < 0.0);
    // Fill all 128 slots with drops away from this bank: excess must remain there.
    let mut full = Drops::new(Vec2::splat(2.0), 1.0, 2);
    let extra = Drops::new(Vec2::splat(2.0), 1.0, 3);
    let remaining = 128 - full.drops.len();
    full.drops.extend(extra.drops.into_iter().take(remaining));
    for drop in &mut full.drops {
        drop.pos = Vec2::ZERO;
    }
    wet[i] = 1.8;
    release_runoff(&mut wet, &points, bounds, &mut full, -Vec2::Y * 0.12);
    assert_eq!(wet[i], 1.8);
}

#[test]
fn fresh_rain_is_carried_and_leaves_film_on_subsequent_strokes() {
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::X + Vec3::Z],
        indices: vec![0, 1, 2, 1, 3, 2],
        ranges: vec![(0, 6, 0)],
        ..Default::default()
    };
    let (points, bounds) = film_points(&data, 0).unwrap();
    let low = [Vec3::new(0.0, 0.03, 0.2), Vec3::new(1.0, 0.03, 0.2)];
    let high = low.map(|p| p + Vec3::Z * 0.4);
    let mut wet = vec![0.0; SIZE * SIZE];
    for (from, to) in [(low, high), (high, low), (low, high)] {
        // Fresh arrivals after a previously cleared pane: much less than saturation.
        wet.fill(0.03);
        assert!(wipe(&mut wet, &points, bounds, from, to));
        assert!(
            points
                .iter()
                .zip(&wet)
                .any(|(p, w)| { (0.25..0.55).contains(&p.z) && *w == WIPED_FILM })
        );
        let direction = (to[0].z - from[0].z).signum();
        assert!(
            points
                .iter()
                .zip(&wet)
                .any(|(p, w)| { (p.z - to[0].z) * direction > 0.0 && *w > 1.0 })
        );
    }
}

#[test]
fn masks_use_geometry_when_texture_coordinates_repeat() {
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::X + Vec3::Z],
        indices: vec![0, 1, 2, 1, 3, 2],
        ranges: vec![(0, 6, 0)],
        uvs: vec![Vec2::ZERO; 4],
        ..Default::default()
    };
    let (points, bounds) = film_points(&data, 0).unwrap();
    assert_eq!(bounds, [0.0, 0.0, 1.0, 1.0]);
    assert!(points.iter().all(|p| p.is_finite()));
    assert!(points[0].x < 0.01 && points[SIZE * SIZE - 1].x > 0.99);
    assert!(film_points(&data, 1).is_none());
}

#[test]
fn side_panes_use_longitudinal_coordinates_and_keep_the_map_in_the_cab() {
    let data = MeshData {
        positions: vec![
            Vec3::ZERO,
            Vec3::Y * 10.0,
            Vec3::Z,
            Vec3::Y * 10.0 + Vec3::Z,
        ],
        indices: vec![0, 1, 2, 1, 3, 2],
        ranges: vec![(0, 6, 0)],
        ..Default::default()
    };
    let (points, bounds) = film_points_in_cab(&data, 0, Some(Vec3::Y * 8.0)).unwrap();
    assert!(bounds[2] < 0.0);
    assert!(
        points
            .iter()
            .all(|p| p.is_finite() && p.y > 6.2 && p.y < 9.8)
    );
    let p = points[SIZE * SIZE / 2];
    assert_eq!(pane_depth(p, bounds), 0.0);
    assert!((pane_pixel(p, bounds).y - (SIZE / 2) as f32 - 0.5).abs() < 1e-4);
}

#[test]
fn diagonal_blade_contact_uses_its_long_axis() {
    let axis = Vec3::new(0.6, 0.0, 0.8);
    let points = [
        axis * -0.4,
        axis * 0.4,
        axis * -0.4 + Vec3::Y * 0.02,
        axis * 0.4 + Vec3::Y * 0.02,
    ];
    let ends = blade_ends(&points).unwrap();
    assert!((ends[0].distance(ends[1]) - 0.8).abs() < 1e-4);
    assert!(ends.iter().all(|p| (p.y - 0.01).abs() < 1e-4));
}
