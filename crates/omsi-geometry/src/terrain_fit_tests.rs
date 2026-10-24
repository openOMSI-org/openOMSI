use super::*;
use spade::FloatTriangulation;
use std::collections::HashMap;

#[test]
fn shoulder_preserves_a_cliff_that_rises_away_from_an_already_aligned_lip() {
    let mut field = Field::new(60);
    field.add_rim(
        [DVec3::new(40.0, 40.0, 0.0), DVec3::new(40.0, 60.0, 0.0)],
        None,
        DVec2::ZERO,
        5.0,
    );
    // A bank curves upward beside the ditch. Extrapolating the flat lip's
    // absolute plane must not cut a new shelf into this original hillside.
    let terrain = |p: DVec2| 0.4 * (p.x - 40.0).max(0.0).powi(2);
    for x in [41.0, 42.0, 43.0, 44.0] {
        let p = DVec2::new(x, 50.0);
        assert!((field.height(p, terrain(p), &terrain) - terrain(p)).abs() < 0.01);
    }
}

#[test]
fn shoulder_preserves_a_changing_longitudinal_hill_slope() {
    let mut field = Field::new(60);
    field.add_rim(
        [DVec3::new(40.0, 40.0, 0.0), DVec3::new(40.0, 60.0, 4.0)],
        None,
        DVec2::new(0.0, 0.2),
        5.0,
    );
    let terrain = |p: DVec2| 0.2 * (p.y - 40.0) + 0.25 * (p.x - 40.0).powi(2);
    for y in [42.0, 48.0, 55.0] {
        let p = DVec2::new(42.0, y);
        assert!((field.height(p, terrain(p), &terrain) - terrain(p)).abs() < 0.01);
    }
}

#[test]
fn curved_shoulder_preserves_the_bank_outside_the_bend() {
    let mut field = Field::new(60);
    let center = DVec2::splat(100.0);
    let arc = |angle: f64| center + DVec2::new(angle.cos(), angle.sin()) * 40.0;
    for i in 0..90 {
        let a = arc((i as f64).to_radians());
        let b = arc((i as f64 + 1.0).to_radians());
        field.add_rim([a.extend(0.0), b.extend(0.0)], None, DVec2::ZERO, 5.0);
    }
    let terrain = |p: DVec2| 0.25 * (p.distance(center) - 40.0).powi(2);
    for angle in [15.0_f64, 30.0, 45.0, 60.0, 75.0] {
        let angle = angle.to_radians();
        let p = center + DVec2::new(angle.cos(), angle.sin()) * 42.0;
        assert!((field.height(p, terrain(p), &terrain) - terrain(p)).abs() < 0.002);
    }
}

fn test_profile(
    def: &Spline,
    curve: &SplineCurve,
    mirror: bool,
    mode: u8,
    origin: DVec3,
) -> SplineTerrainProfile {
    let rims = spline_hole_rims(def, curve, mirror, mode);
    build_spline_terrain_profile(def, curve, mirror, mode, &rims, origin)
}

fn ditch() -> Spline {
    // Synthetic cross-section with the reported lip spacing and depth. The
    // intermediate shape is authored for this test, not copied from a map asset.
    let points = [
        (2.92, 0.10),
        (3.45, 0.075),
        (3.82, -0.16),
        (4.12, -0.44),
        (4.23, -0.47),
        (4.34, -0.44),
        (4.76, -0.12),
        (5.14, 0.035),
        (5.35, 0.04),
    ];
    let mut text =
        "[heightprofile]\n2.92\n5.35\n0.10\n0.04\n[texture]\ngras1.bmp\n[profile]\n0\n".to_string();
    for (x, z) in points {
        text.push_str(&format!("[profilepnt]\n{x}\n{z}\n0.7\n0.167\n"));
    }
    Spline::parse(&omsi_cfg::CfgFile::from_str("ditch.sli", &text))
}

fn ground() -> Terrain {
    Terrain {
        cells: 60,
        heights: vec![0.0; 61 * 61],
    }
}

fn curve(pos: [f64; 3], heading: f64, radius: f64) -> SplineCurve {
    SplineCurve::from_map(
        &MapSpline {
            pos,
            heading,
            length: 20.0,
            radius,
            ..Default::default()
        },
        DVec2::ZERO,
    )
}

fn sample(mesh: &MeshData, x: f64, y: f64) -> f64 {
    let p = DVec2::new(x, y);
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[t[i] as usize].as_dvec3());
        let (u, v, d) = ((b - a).truncate(), (c - a).truncate(), p - a.truncate());
        let det = u.perp_dot(v);
        if det.abs() < 1e-10 {
            continue;
        }
        let (s, t) = (d.perp_dot(v) / det, u.perp_dot(d) / det);
        if s >= -1e-7 && t >= -1e-7 && s + t <= 1.0 + 1e-7 {
            return a.z + s * (b.z - a.z) + t * (c.z - a.z);
        }
    }
    panic!("open ground at ({x}, {y})");
}

fn sample_normal(mesh: &MeshData, x: f64, y: f64) -> Vec3 {
    let p = DVec2::new(x, y);
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[t[i] as usize].truncate().as_dvec2());
        let (u, v, d) = (b - a, c - a, p - a);
        let det = u.perp_dot(v);
        if det.abs() < 1e-12 {
            continue;
        }
        let (s, tv) = (d.perp_dot(v) / det, u.perp_dot(d) / det);
        if s >= -1e-7 && tv >= -1e-7 && s + tv <= 1.0 + 1e-7 {
            return (mesh.normals[t[0] as usize] * (1.0 - s - tv) as f32
                + mesh.normals[t[1] as usize] * s as f32
                + mesh.normals[t[2] as usize] * tv as f32)
                .normalize();
        }
    }
    panic!("missing terrain normal at {p:?}");
}

fn closed_tile(mesh: &MeshData) {
    let mut edges = HashMap::<(u32, u32), usize>::new();
    let mut area = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let p = [0, 1, 2].map(|i| mesh.positions[t[i] as usize].truncate().as_dvec2());
        assert!(
            (p[1] - p[0]).perp_dot(p[2] - p[0]) > 0.0,
            "a rendered triangle must not collapse or invert: {p:?}"
        );
        area += (p[1] - p[0]).perp_dot(p[2] - p[0]).abs() * 0.5;
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    assert!(
        (area - tile_size().powi(2)).abs() < 0.01,
        "the whole tile must remain covered: {area}"
    );
    for ((a, b), n) in edges {
        assert!(n == 1 || n == 2);
        if n == 1 {
            let [a, b] = [a, b].map(|i| mesh.positions[i as usize]);
            assert!(
                [0, 1]
                    .into_iter()
                    .any(|axis| (a[axis].abs() < 1e-5 && b[axis].abs() < 1e-5)
                        || ((a[axis] - tile_size() as f32).abs() < 1e-5
                            && (b[axis] - tile_size() as f32).abs() < 1e-5)),
                "unpaired interior edge: {a:?} -> {b:?}"
            );
        }
    }
    assert!(mesh.positions.iter().all(|p| p.is_finite()));
    assert!(mesh.normals.iter().all(|p| p.is_finite()));
}

#[test]
fn reported_ditch_has_a_floor_smooth_shoulders_and_closed_extended_ends() {
    let def = ditch();
    let ground = ground();
    let saved = ground.heights.clone();
    for mirror in [false, true] {
        for mode in 1..=4 {
            let c = curve([40.0, 40.0, 0.0], 0.0, 0.0);
            let profile = test_profile(&def, &c, mirror, mode, DVec3::ZERO);
            let mesh = build_fitted_terrain_mesh(&ground, &[(&profile, DVec3::ZERO)]);
            closed_tile(&mesh);
            let x = |offset: f64| 40.0 + if mirror { -offset } else { offset };
            assert!(
                sample(&mesh, x(4.23), 50.0) < -0.47,
                "the ground must follow the trough, not the flat heightprofile"
            );
            assert!((sample(&mesh, x(2.95), 50.0) - 0.097).abs() < 0.002);
            let shoulder = sample(&mesh, x(2.45), 50.0);
            assert!(
                shoulder > 0.0 && shoulder < 0.097,
                "blend into the lip: {shoulder}"
            );
            assert!(sample(&mesh, x(-3.0), 50.0).abs() < 1e-7);
            let visible = build_spline_mesh(&def, &c, mirror, DVec3::ZERO);
            for i in 1..100 {
                let px = x(2.92 + (5.35 - 2.92) * i as f64 / 100.0);
                assert!(
                    sample(&mesh, px, 50.0) <= sample(&visible, px, 50.0) + 0.002,
                    "the backing must not bury any part of the drawn ditch"
                );
            }
            for y in [39.25, 39.75, 40.0, 59.75, 60.0, 60.25, 60.75] {
                assert!(sample(&mesh, x(4.23), y).is_finite());
            }
        }
    }
    assert_eq!(
        ground.heights, saved,
        "no terrain file or collision grid is rewritten"
    );
}

#[test]
fn curved_sloping_canted_and_overlapping_profiles_keep_the_tile_closed() {
    let def = ditch();
    let mut c = curve([40.0, 40.0, 0.0], 30.0, 35.0);
    c.grad_start = 5.0;
    c.grad_end = -3.0;
    c.cant_start = 7.0;
    c.cant_end = -5.0;
    let other = curve([35.0, 50.0, -0.1], 90.0, 0.0);
    let a = test_profile(&def, &c, false, 1, DVec3::ZERO);
    let b = test_profile(&def, &other, true, 4, DVec3::ZERO);
    let m = build_fitted_terrain_mesh(&ground(), &[(&a, DVec3::ZERO), (&b, DVec3::ZERO)]);
    closed_tile(&m);
    let reversed = build_fitted_terrain_mesh(&ground(), &[(&b, DVec3::ZERO), (&a, DVec3::ZERO)]);
    closed_tile(&reversed);
    for y in 40..60 {
        for x in 35..60 {
            assert!(
                (sample(&m, x as f64 + 0.1, y as f64 + 0.2)
                    - sample(&reversed, x as f64 + 0.1, y as f64 + 0.2))
                .abs()
                    < 0.002
            );
        }
    }
}

#[test]
fn trough_and_blended_shoulders_match_across_tile_seams() {
    let c = curve([tile_size() - 10.0, 40.0, 0.0], 73.0, 0.0);
    let profile = test_profile(&ditch(), &c, false, 1, DVec3::ZERO);
    let graded = |origin_x: f64| {
        let mut t = ground();
        for y in 0..=t.cells {
            for x in 0..=t.cells {
                let (px, py) = (
                    origin_x + x as f64 * tile_size() / t.cells as f64,
                    y as f64 * tile_size() / t.cells as f64,
                );
                t.heights[y * (t.cells + 1) + x] = ((px * 0.02).sin() + py * py * 0.001) as f32;
            }
        }
        t
    };
    let (left_ground, right_ground) = (graded(0.0), graded(tile_size()));
    let left = build_fitted_terrain_mesh_with_neighbors(
        &left_ground,
        &[(&profile, DVec3::ZERO)],
        &[(&right_ground, DVec2::new(tile_size(), 0.0))],
    );
    let right = build_fitted_terrain_mesh_with_neighbors(
        &right_ground,
        &[(&profile, DVec3::new(-tile_size(), 0.0, 0.0))],
        &[(&left_ground, DVec2::new(-tile_size(), 0.0))],
    );
    closed_tile(&left);
    closed_tile(&right);
    let seam_stations = |mesh: &MeshData, x: f32| {
        let mut stations: Vec<_> = mesh
            .positions
            .iter()
            .filter(|p| (p.x - x).abs() < 1e-5)
            .map(|p| (p.y as f64 * 10000.0).round() as i64)
            .collect();
        stations.sort_unstable();
        stations.dedup();
        stations
    };
    assert_eq!(
        seam_stations(&left, tile_size() as f32),
        seam_stations(&right, 0.0),
        "adaptive refinement must keep matching seam vertices"
    );
    for i in 0..100 {
        let y = 33.0 + i as f64 * 0.1;
        let (a, b) = (sample(&left, tile_size(), y), sample(&right, 0.0, y));
        assert!((a - b).abs() < 0.0001, "tile seam at {y}: {a} != {b}");
        let (na, nb) = (
            sample_normal(&left, tile_size(), y),
            sample_normal(&right, 0.0, y),
        );
        assert!(
            na.distance(nb) < 0.03,
            "terrain lighting must join across tiles at {y}: {na:?} != {nb:?}"
        );
    }
}

#[test]
fn saved_terrain_triangles_are_preserved_away_from_the_ditch() {
    let mut t = ground();
    for y in 0..=t.cells {
        for x in 0..=t.cells {
            t.heights[y * (t.cells + 1) + x] =
                ((x as f32 * 0.17).sin() * (y as f32 * 0.23).cos()) * 5.0;
        }
    }
    let c = curve([40.0, 40.0, 0.0], 0.0, 0.0);
    let p = test_profile(&ditch(), &c, false, 1, DVec3::ZERO);
    let m = build_fitted_terrain_mesh(&t, &[(&p, DVec3::ZERO)]);
    closed_tile(&m);
    for y in (100..290).step_by(19) {
        for x in (100..290).step_by(17) {
            let p = DVec2::new(x as f64 + 0.123, y as f64 + 0.456);
            assert!(
                (sample(&m, p.x, p.y) - super::super::terrain_walls::height(&t, p, tile_size()))
                    .abs()
                    < 0.00001,
                "the original diagonals and heights must survive outside the local blend"
            );
        }
    }
}

#[test]
fn streamed_nearly_coincident_profiles_do_not_abort_the_tile() {
    let def = ditch();
    for i in 0..48 {
        let c = curve(
            [39.731 + i as f64 * 0.013, 40.127, 0.0],
            7.123 + i as f64 * 1.713,
            31.319 + i as f64 * 0.327,
        );
        let a = test_profile(&def, &c, i % 2 == 0, 1 + i % 4, DVec3::ZERO);
        let mut other = c;
        other.heading_deg += 17.127;
        let b = test_profile(&def, &other, false, 1, DVec3::ZERO);
        let m = build_fitted_terrain_mesh(
            &ground(),
            &[
                (&a, DVec3::ZERO),
                (&a, DVec3::new(0.00000317, -0.00000197, 0.0)),
                (&b, DVec3::ZERO),
            ],
        );
        closed_tile(&m);
        let x = if i % 2 == 0 { -4.23 } else { 4.23 };
        let p = c.with_sli(&def).offset_point(10.0, x, -0.47);
        assert!(
            sample(&m, p.x, p.y) <= p.z + 0.01,
            "fixture {i}: avoiding the split panic must not bury the ditch"
        );
    }
}

#[test]
fn steep_hill_transition_does_not_step_when_the_nearest_rim_changes() {
    let mut terrain = ground();
    for y in 0..=terrain.cells {
        for x in 0..=terrain.cells {
            terrain.heights[y * (terrain.cells + 1) + x] = x as f32 * 5.0 * 3.0;
        }
    }
    let mut field = Field::new(terrain.cells);
    // Two verges follow the same steep ground, both 20 cm below it. Switching
    // to the nearest verge's absolute height used to make a ridge between them.
    field.add_rim(
        [DVec3::new(40.0, 40.0, 119.8), DVec3::new(40.0, 60.0, 119.8)],
        None,
        DVec2::new(3.0, 0.0),
        1.0,
    );
    field.add_rim(
        [DVec3::new(41.0, 40.0, 122.8), DVec3::new(41.0, 60.0, 122.8)],
        None,
        DVec2::new(3.0, 0.0),
        1.0,
    );
    let a = DVec2::new(40.5 - 0.0001, 50.0);
    let b = DVec2::new(40.5 + 0.0001, 50.0);
    let terrain_at = |p| super::super::terrain_walls::height(&terrain, p, tile_size());
    let ha = field.height(a, a.x * 3.0, &terrain_at);
    let hb = field.height(b, b.x * 3.0, &terrain_at);
    assert!(
        (hb - ha).abs() < 0.001,
        "nearest-edge switch creates a step on the slope: {ha} -> {hb}"
    );
    assert!(
        (ha - (a.x * 3.0 - 0.1)).abs() < 0.001,
        "retain the hill's gradient while blending the displacement"
    );
}

#[test]
fn steep_hillside_joins_the_ditch_over_the_terrain_sampling_distance() {
    let mut terrain = ground();
    for y in 0..=terrain.cells {
        for x in 0..=terrain.cells {
            terrain.heights[y * (terrain.cells + 1) + x] =
                (24.0 + (220.0 - x as f64 * 5.0) * 0.5) as f32;
        }
    }
    let c = curve([220.0, 190.0, 24.0], 0.0, 0.0);
    let profile = test_profile(&ditch(), &c, true, 1, DVec3::ZERO);
    let mesh = build_fitted_terrain_mesh(&terrain, &[(&profile, DVec3::ZERO)]);
    closed_tile(&mesh);
    let mut previous = sample(&mesh, 205.0, 200.0);
    for i in 1..=96 {
        let x = 205.0 + i as f64 * 0.1;
        let h = sample(&mesh, x, 200.0);
        assert!(
            (h - previous).abs() < 0.2,
            "the hillside must not become a narrow grass wall at {x}: {previous} -> {h}"
        );
        previous = h;
    }
    assert!((sample(&mesh, 205.0, 200.0) - 31.5).abs() < 1e-5);
}

#[test]
fn curved_rim_has_no_step_when_two_projected_samples_meet_at_a_station() {
    let mut field = Field::new(60);
    field.add_rim(
        [DVec3::new(40.0, 40.0, 10.0), DVec3::new(40.0, 50.0, 10.0)],
        None,
        DVec2::ZERO,
        5.0,
    );
    field.add_rim(
        [DVec3::new(40.0, 50.0, 10.0), DVec3::new(39.0, 60.0, 10.0)],
        None,
        DVec2::ZERO,
        5.0,
    );
    field.add_rim(
        [DVec3::new(43.0, 40.0, 20.0), DVec3::new(43.0, 60.0, 20.0)],
        None,
        DVec2::ZERO,
        5.0,
    );
    let a = field.height(DVec2::new(41.0, 50.0 - 0.0001), 30.0, &|_| 30.0);
    let b = field.height(DVec2::new(41.0, 50.0 + 0.0001), 30.0, &|_| 30.0);
    assert!(
        (a - b).abs() < 0.001,
        "rim station creates a ledge: {a} -> {b}"
    );
}

#[test]
fn ditch_ends_close_locally_without_a_long_bowl_in_the_saved_ground() {
    for mirror in [false, true] {
        for mode in 1..=4 {
            let c = curve([40.0, 40.0, 0.0], 0.0, 0.0);
            let profile = test_profile(&ditch(), &c, mirror, mode, DVec3::ZERO);
            let mesh = build_fitted_terrain_mesh(&ground(), &[(&profile, DVec3::ZERO)]);
            let x = 40.0 + if mirror { -4.23 } else { 4.23 };
            for y in [37.5, 38.0, 62.0, 62.5, 63.0] {
                assert!(
                    sample(&mesh, x, y).abs() < 0.002,
                    "the ditch must not dig a bowl beyond its end: mode {mode}, y={y}, height={}",
                    sample(&mesh, x, y)
                );
            }
            closed_tile(&mesh);
            let tip = if mode & 1 == 0 { 60.0 } else { 60.5 };
            let mut previous = sample(&mesh, x, tip + 0.01);
            assert!(previous < -0.4);
            for i in 1..=40 {
                let h = sample(&mesh, x, tip + 0.01 + i as f64 * 0.05);
                assert!(
                    h >= previous - 0.002 && h <= 0.002,
                    "end cap must return smoothly to the ground: mode {mode}, mirror {mirror}, tip {tip}, step {i}, {previous} -> {h}"
                );
                assert!(
                    (h - previous).abs() < 0.1,
                    "end cap must not form a vertical step"
                );
                previous = h;
            }
        }
    }
}

#[test]
fn curved_sloping_end_caps_preserve_saved_ground_beyond_both_terminals() {
    for mirror in [false, true] {
        for mode in 1..=4 {
            let mut c = curve([100.0, 100.0, 0.0], 37.0, -45.0);
            c.grad_start = 2.0;
            c.grad_end = 4.0;
            let profile = test_profile(&ditch(), &c, mirror, mode, DVec3::ZERO);
            let mesh = build_fitted_terrain_mesh(&ground(), &[(&profile, DVec3::ZERO)]);
            closed_tile(&mesh);
            for s in [-3.0, c.length + 3.0] {
                for x in [2.0, 4.23, 6.0] {
                    let p = c.offset_point(s, if mirror { -x } else { x }, 0.0);
                    assert!(
                        sample(&mesh, p.x, p.y).abs() < 0.002,
                        "curved terminal must not deform distant ground: {p:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn transition_slope_does_not_jump_at_a_nearest_rim_bisector() {
    let mut field = Field::new(60);
    field.add_rim(
        [DVec3::new(40.0, 40.0, -0.2), DVec3::new(40.0, 60.0, -0.2)],
        None,
        DVec2::ZERO,
        1.0,
    );
    field.add_rim(
        [DVec3::new(41.0, 40.0, -0.2), DVec3::new(41.0, 60.0, -0.2)],
        None,
        DVec2::ZERO,
        1.0,
    );
    let h = |x| field.height(DVec2::new(x, 50.0), 0.0, &|_| 0.0);
    let d = 0.0001;
    let left = (h(40.5) - h(40.5 - d)) / d;
    let right = (h(40.5 + d) - h(40.5)) / d;
    assert!(
        (left - right).abs() < 0.002,
        "continuous heights must also have continuous slopes: {left} -> {right}"
    );
}

#[test]
fn backing_follows_the_graphical_profile_at_its_lip_instead_of_a_coarse_v() {
    let profile = test_profile(
        &ditch(),
        &curve([40.0, 40.0, 0.0], 0.0, 0.0),
        false,
        1,
        DVec3::ZERO,
    );
    let h = |x| sample(&profile.mesh, x, 50.0);
    let slope = (h(43.0) - h(42.96)) / 0.04;
    let expected = (0.075 - 0.1) / (3.45 - 2.92);
    assert!(
        (slope - expected).abs() < 0.002,
        "the backing lip must match the visible profile's tangent: {slope} vs {expected}"
    );
}

#[test]
fn transition_matches_the_rim_slope_and_returns_to_the_saved_ground_slope() {
    let mut field = Field::new(60);
    let [a, b, c, d] = [
        DVec3::new(40.0, 40.0, 0.1),
        DVec3::new(42.0, 40.0, 0.02),
        DVec3::new(42.0, 60.0, 0.02),
        DVec3::new(40.0, 60.0, 0.1),
    ];
    field.add_triangle([a, b, c]);
    field.add_triangle([a, c, d]);
    field.add_rim([a, d], None, DVec2::new(-0.04, 0.0), 1.0);
    field.add_rim([b, c], None, DVec2::new(-0.04, 0.0), 1.0);
    let h = |x| field.height(DVec2::new(x, 50.0), 0.0, &|_| 0.0);
    let e = 0.0001;
    for x in [39.0, 40.0] {
        let left = (h(x) - h(x - e)) / e;
        let right = (h(x + e) - h(x)) / e;
        assert!(
            (left - right).abs() < 0.002,
            "slope discontinuity at {x}: {left} -> {right}"
        );
    }
    assert!(((h(40.0) - h(40.0 - e)) / e + 0.04).abs() < 0.002);
}

#[test]
fn road_surfaces_bound_the_shoulder_in_straights_bends_and_hills() {
    for (heading, radius, grade) in [(0.0, 0.0, 0.0), (37.0, -45.0, 4.0)] {
        let mut c = curve([100.0, 100.0, 0.18], heading, radius);
        c.grad_start = grade;
        c.grad_end = grade;
        let profile = test_profile(&ditch(), &c, false, 4, DVec3::ZERO);
        let mut road = MeshData::default();
        for i in 0..=40 {
            for x in [-3.0, 2.92] {
                let p = c.offset_point(i as f64 * c.length / 40.0, x, -0.15);
                road.positions.push(p.as_vec3());
            }
            if i > 0 {
                let a = ((i - 1) * 2) as u32;
                road.indices.extend([a, a + 2, a + 1, a + 1, a + 2, a + 3]);
            }
        }
        let unfitted = build_fitted_terrain_mesh(&ground(), &[(&profile, DVec3::ZERO)]);
        let fitted = build_fitted_terrain_mesh_with_roads(
            &ground(),
            &[(&profile, DVec3::ZERO)],
            &[],
            &[(&road, DVec3::ZERO)],
        );
        closed_tile(&fitted);
        let mut was_over_road = false;
        for t in road.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|i| road.positions[t[i] as usize].as_dvec3());
            // Probe near the ditch-facing edge as well as within the carriageway.
            for weights in [[0.1, 0.1, 0.8], [0.1, 0.8, 0.1], [0.3, 0.3, 0.4]] {
                let p = a * weights[0] + b * weights[1] + c * weights[2];
                was_over_road |= sample(&unfitted, p.x, p.y) > p.z + 0.01;
                assert!(
                    sample(&fitted, p.x, p.y) <= p.z - 0.002,
                    "ground must stay below the real road triangles: {p:?}"
                );
            }
        }
        assert!(
            was_over_road,
            "the fixture must reproduce the terrain spilling onto the road"
        );
    }
}

#[test]
fn road_constraint_keeps_ground_under_a_bridge_and_blends_outside_its_rim() {
    let mut road = Field::new(60);
    let [a, b, c, d] = [
        DVec3::new(40.0, 40.0, 0.0),
        DVec3::new(42.0, 40.0, 0.0),
        DVec3::new(42.0, 60.0, 0.0),
        DVec3::new(40.0, 60.0, 0.0),
    ];
    road.add_triangle([a, b, c]);
    road.add_triangle([a, c, d]);
    road.add_rim([b, c], None, DVec2::ZERO, 0.5);
    assert_eq!(road.road_limit(DVec2::new(41.0, 50.0), -5.0), -5.0);
    let h = |x| road.road_limit(DVec2::new(x, 50.0), 0.2);
    let e = 0.0001;
    for x in [42.0, 42.5] {
        assert!(((h(x) - h(x - e)) / e - (h(x + e) - h(x)) / e).abs() < 0.002);
    }
    assert_eq!(h(43.0), 0.2);
    let mut field = Field::new(60);
    field.add_triangle([
        a + DVec3::Z * 11.0,
        b + DVec3::Z * 11.0,
        c + DVec3::Z * 11.0,
    ]);
    field.roads = Some(Box::new(road));
    assert!(
        field.height(DVec2::new(41.0, 45.0), 10.0, &|_| 10.0) > 9.99,
        "new fill must be limited without excavating the original roof over an underpass"
    );
}

/// Synthetic load/mesh-size comparison; no original map content is required.
#[test]
#[ignore = "opt-in release performance measurement"]
fn spline_heavy_terrain_cost() {
    let terrain = Terrain {
        cells: 128,
        heights: vec![1.0; 129 * 129],
    };
    let mut c = curve([140.0, 40.0, 0.0], 0.0, 0.0);
    c.length = 200.0;
    let profile = test_profile(&ditch(), &c, false, 1, DVec3::ZERO);
    let mut roads = Vec::new();
    for lane in 0..14 {
        let mut mesh = MeshData::default();
        let x = 10.0 + lane as f32 * 20.0;
        for station in 0..=600 {
            let y = station as f32 * 0.5;
            mesh.positions
                .extend([Vec3::new(x, y, 1.1), Vec3::new(x + 6.0, y, 1.1)]);
            if station < 600 {
                let a = station * 2;
                mesh.indices.extend([a, a + 1, a + 3, a, a + 3, a + 2]);
            }
        }
        roads.push(mesh);
    }
    let roads: Vec<_> = roads.iter().map(|m| (m, DVec3::ZERO)).collect();
    let uneven = Terrain {
        cells: 128,
        heights: (0..129)
            .flat_map(|y| {
                (0..129).map(move |x| {
                    (1.0 + 0.15 * (x as f64 * 0.13).sin() * (y as f64 * 0.11).cos()) as f32
                })
            })
            .collect(),
    };
    for (kind, terrain) in [("flat", terrain), ("uneven", uneven)] {
        for run in 0..4 {
            let start = std::time::Instant::now();
            let mesh = build_terrain_mesh(&terrain);
            println!(
                "SAVED_COST,{kind},{run},{:.3},{},{}",
                start.elapsed().as_secs_f64() * 1000.0,
                mesh.positions.len(),
                mesh.indices.len() / 3
            );
            std::hint::black_box(mesh);
        }
        for run in 0..4 {
            let start = std::time::Instant::now();
            let mesh = try_build_fitted_terrain_mesh_with_roads(
                &terrain,
                &[(&profile, DVec3::ZERO)],
                &[],
                &roads,
            )
            .unwrap();
            println!(
                "FIT_COST,{kind},{run},{:.3},{},{}",
                start.elapsed().as_secs_f64() * 1000.0,
                mesh.positions.len(),
                mesh.indices.len() / 3
            );
        }
    }
}

#[test]
fn distant_roads_do_not_refine_terrain_or_change_the_local_fit() {
    let terrain = ground();
    let c = curve([40.0, 40.0, 0.0], 0.0, 0.0);
    let profile = test_profile(&ditch(), &c, false, 1, DVec3::ZERO);
    let expected = build_fitted_terrain_mesh(&terrain, &[(&profile, DVec3::ZERO)]);
    let road = MeshData {
        positions: vec![
            Vec3::new(240.0, 20.0, 1.0),
            Vec3::new(248.0, 20.0, 1.0),
            Vec3::new(240.0, 280.0, 1.0),
            Vec3::new(248.0, 280.0, 1.0),
        ],
        indices: vec![0, 1, 3, 0, 3, 2],
        ..Default::default()
    };
    let actual = build_fitted_terrain_mesh_with_roads(
        &terrain,
        &[(&profile, DVec3::ZERO)],
        &[],
        &[(&road, DVec3::ZERO)],
    );
    assert_eq!(actual.positions.len(), expected.positions.len());
    assert_eq!(actual.indices, expected.indices);
    for (i, (actual_position, expected_position)) in
        actual.positions.iter().zip(&expected.positions).enumerate()
    {
        assert_eq!(actual_position.truncate(), expected_position.truncate());
        if actual_position.x < 150.0 {
            assert_eq!(actual_position, expected_position);
            assert_eq!(actual.normals[i], expected.normals[i]);
        }
        assert!(actual.normals[i].is_finite());
    }
    // A road above the saved ground must not refine a flat, untouched region.
    assert_eq!(sample(&actual, 244.0, 100.0), 0.0);
    // Clearance beneath a road below the saved terrain still uses the old grid;
    // it must not introduce road-station sampling outside the fitted region.
    let mut low_road = road.clone();
    for p in &mut low_road.positions {
        p.z = -1.0;
    }
    let low = build_fitted_terrain_mesh_with_roads(
        &terrain,
        &[(&profile, DVec3::ZERO)],
        &[],
        &[(&low_road, DVec3::ZERO)],
    );
    let cell = tile_size() / terrain.cells as f64;
    for p in low.positions.iter().filter(|p| p.x > 150.0) {
        assert!((p.x as f64 / cell - (p.x as f64 / cell).round()).abs() < 1e-5);
        assert!((p.y as f64 / cell - (p.y as f64 / cell).round()).abs() < 1e-5);
    }
    assert!((sample(&low, 244.0, 100.0) + 0.003).abs() < 1e-6);
}

#[test]
fn adaptive_mesh_preserves_saved_terrain_and_the_full_tile_border() {
    for uneven in [false, true] {
        let cells = 16usize;
        let terrain = Terrain {
            cells,
            heights: (0..=cells)
                .flat_map(|y| {
                    (0..=cells).map(move |x| {
                        if uneven {
                            (x / 3) as f32 + (y as f32 - 8.0).powi(2) * 0.1 + (x * y) as f32 * 0.02
                        } else {
                            1.0
                        }
                    })
                })
                .collect(),
        };
        let profile = test_profile(
            &ditch(),
            &curve([40.0, 40.0, 0.0], 0.0, 0.0),
            false,
            1,
            DVec3::ZERO,
        );
        let mesh = build_fitted_terrain_mesh(&terrain, &[(&profile, DVec3::ZERO)]);
        let side = tile_size();
        let cell = side / cells as f64;
        for y in 8..cells {
            for x in 8..cells {
                for offset in [
                    DVec2::splat(0.25),
                    DVec2::new(0.75, 0.25),
                    DVec2::splat(0.75),
                ] {
                    let p = (DVec2::new(x as f64, y as f64) + offset) * cell;
                    let expected = super::super::terrain_walls::height(&terrain, p, side);
                    assert!(
                        (sample(&mesh, p.x, p.y) - expected).abs() < 1e-5,
                        "saved terrain changed at {p}"
                    );
                }
            }
        }
        for i in 0..=cells {
            for p in [
                DVec2::new(i as f64 * cell, 0.0),
                DVec2::new(i as f64 * cell, side),
                DVec2::new(0.0, i as f64 * cell),
                DVec2::new(side, i as f64 * cell),
            ] {
                assert!(
                    mesh.positions
                        .iter()
                        .any(|q| q.truncate().as_dvec2().distance(p) < 1e-5),
                    "missing border point {p}"
                );
            }
        }
    }
}

#[test]
fn coplanar_road_station_density_does_not_inflate_fitted_terrain() {
    let terrain = ground();
    let profile = test_profile(
        &ditch(),
        &curve([40.0, 40.0, 0.0], 0.0, 0.0),
        false,
        1,
        DVec3::ZERO,
    );
    let road = |stations: usize| {
        let mut mesh = MeshData::default();
        for i in 0..=stations {
            let y = 20.0 + 100.0 * i as f32 / stations as f32;
            mesh.positions
                .extend([Vec3::new(42.0, y, 0.05), Vec3::new(48.0, y, 0.05)]);
            if i < stations {
                let a = (i * 2) as u32;
                mesh.indices.extend([a, a + 1, a + 3, a, a + 3, a + 2]);
            }
        }
        mesh
    };
    let coarse_road = road(1);
    let dense_road = road(200);
    let fit = |road: &MeshData| {
        build_fitted_terrain_mesh_with_roads(
            &terrain,
            &[(&profile, DVec3::ZERO)],
            &[],
            &[(road, DVec3::ZERO)],
        )
    };
    let coarse = fit(&coarse_road);
    let dense = fit(&dense_road);
    assert!(
        dense.positions.len() <= coarse.positions.len() + 64,
        "{} vs {}",
        dense.positions.len(),
        coarse.positions.len()
    );
    for y in (35..66).step_by(2) {
        for x in (35..53).step_by(2) {
            assert!(
                (sample(&dense, x as f64, y as f64) - sample(&coarse, x as f64, y as f64)).abs()
                    < 0.002
            );
        }
    }
}

#[test]
fn adaptive_refinement_bounds_height_error_without_retaining_every_probe() {
    let height = |p: DVec2| 0.04 * (p.x * 0.3).sin() + (p.x - 8.0).max(0.0) * 0.01;
    let mut cdt = Cdt::new();
    for p in [
        DVec2::ZERO,
        DVec2::new(20.0, 0.0),
        DVec2::new(20.0, 2.0),
        DVec2::new(0.0, 2.0),
    ] {
        insert(&mut cdt, p).unwrap();
    }
    let probes: Vec<_> = (0..=40)
        .flat_map(|x| (0..=4).map(move |y| DVec2::new(x as f64 * 0.5, y as f64 * 0.5)))
        .collect();
    sampling::refine(&mut cdt, probes.clone(), height, |_, _| 0.0005).unwrap();
    assert!(cdt.num_vertices() < probes.len());
    for p in probes {
        let actual = cdt
            .barycentric()
            .interpolate(
                |v| {
                    let q = v.position();
                    height(DVec2::new(q.x, q.y))
                },
                Point2::new(p.x, p.y),
            )
            .unwrap();
        assert!((actual - height(p)).abs() <= 0.0005 + 1e-10);
    }
}

#[test]
fn coplanar_cleanup_keeps_duplicate_surface_boundaries_and_true_folds() {
    let a = DVec3::ZERO;
    let b = DVec3::X;
    let c = DVec3::new(1.0, 1.0, 0.0);
    let d = DVec3::Y;
    let contains = |edges: &Vec<[DVec2; 2]>, u: DVec3, v: DVec3| {
        edges.iter().any(|edge| {
            *edge == [u.truncate(), v.truncate()] || *edge == [v.truncate(), u.truncate()]
        })
    };
    let planar = [[a, b, c], [a, c, d]];
    let edges = sampling::creases(planar.iter());
    assert!(!contains(&edges, a, c));
    for triangles in [
        vec![planar[0], planar[1]],
        vec![planar[0], planar[1], planar[0], planar[1]],
    ] {
        let edges = sampling::creases(triangles.iter());
        for [u, v] in [[a, b], [b, c], [c, d], [d, a]] {
            assert!(contains(&edges, u, v));
        }
    }
    let folded = [[a, b, c], [a, c, d + DVec3::Z]];
    assert!(contains(&sampling::creases(folded.iter()), a, c));
}

#[test]
fn malformed_profile_is_an_error_instead_of_aborting_a_tile_batch() {
    let profile = SplineTerrainProfile {
        mesh: MeshData {
            indices: vec![0, 1, 2],
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        try_build_fitted_terrain_mesh_with_roads(&ground(), &[(&profile, DVec3::ZERO)], &[], &[]),
        Err(TerrainFitError::Panicked)
    ));
}

#[test]
fn excessive_terminal_depth_returns_a_recoverable_error() {
    let c = curve([40.0, 40.0, -1000.0], 0.0, 0.0);
    let profile = test_profile(&ditch(), &c, false, 1, DVec3::ZERO);
    assert!(matches!(
        try_build_fitted_terrain_mesh_with_roads(&ground(), &[(&profile, DVec3::ZERO)], &[], &[]),
        Err(TerrainFitError::Complexity)
    ));
}

#[test]
fn terminal_fade_preserves_the_lateral_bank_shape() {
    let mut field = Field::new(60);
    field.add_rim(
        [DVec3::new(40.0, 40.0, -0.5), DVec3::new(40.0, 60.0, -0.5)],
        Some([
            End {
                point: DVec2::new(40.0, 40.0),
                outward: -DVec2::Y,
                width: 1.0,
            },
            End {
                point: DVec2::new(40.0, 60.0),
                outward: DVec2::Y,
                width: 1.0,
            },
        ]),
        DVec2::ZERO,
        4.0,
    );
    for x in [40.6, 41.0, 42.0, 43.0] {
        let side = field.height(DVec2::new(x, 60.0), 0.0, &|_| 0.0);
        let half = field.height(DVec2::new(x, 60.5), 0.0, &|_| 0.0);
        assert!((half - side * 0.5).abs() < 1e-10);
        let end = field.height(DVec2::new(x, 61.0), 0.0, &|_| 0.0);
        let near_end = field.height(DVec2::new(x, 60.999), 0.0, &|_| 0.0);
        assert_eq!(end, 0.0);
        assert!(near_end.abs() < 1e-8, "end tangent must join saved ground");
    }
}

#[test]
fn diagonal_constraint_grid_visits_are_linear_and_include_shared_boundaries() {
    let cover = |a, b| super::super::terrain_constraints::covered_bins([a, b], 300.0, 128);
    let diagonal = cover(DVec2::ZERO, DVec2::splat(300.0));
    assert!(diagonal.len() < 4 * 128);
    let reversed = cover(DVec2::splat(300.0), DVec2::ZERO);
    assert_eq!(diagonal, reversed);
    let horizontal = cover(DVec2::new(0.0, 150.0), DVec2::new(300.0, 150.0));
    let vertical = cover(DVec2::new(150.0, 0.0), DVec2::new(150.0, 300.0));
    // Every segment sharing the grid corner must be compared in its floor cell.
    assert!(horizontal.contains(&(64 * 128 + 64)));
    assert!(vertical.contains(&(64 * 128 + 64)));
    assert_eq!(horizontal.len(), 128);
    assert_eq!(vertical.len(), 128);
    for cell in [63 * 128 + 63, 63 * 128 + 64, 64 * 128 + 63, 64 * 128 + 64] {
        assert!(diagonal.contains(&cell));
    }
}

#[test]
fn a_ditch_ending_short_of_the_border_still_moves_both_sides_alike() {
    // A ditch parallel to the border whose outer lip is 1 to 3 m short of it lies in the
    // left tile only: the right tile holds none of its triangles, yet must meet the left.
    for end in [1.0, 2.0, 3.0] {
        let c = curve([tile_size() - 5.35 - end, 30.0, 0.0], 0.0, 0.0);
        let profile = test_profile(&ditch(), &c, false, 1, DVec3::ZERO);
        let graded = |origin_x: f64| {
            let mut t = ground();
            for y in 0..=t.cells {
                for x in 0..=t.cells {
                    let px = origin_x + x as f64 * tile_size() / t.cells as f64;
                    t.heights[y * (t.cells + 1) + x] = (px * 0.02).sin() as f32;
                }
            }
            t
        };
        let (left_ground, right_ground) = (graded(0.0), graded(tile_size()));
        let left = build_fitted_terrain_mesh_with_neighbors(
            &left_ground,
            &[(&profile, DVec3::ZERO)],
            &[(&right_ground, DVec2::new(tile_size(), 0.0))],
        );
        let right = build_fitted_terrain_mesh_with_neighbors(
            &right_ground,
            &[(&profile, DVec3::new(-tile_size(), 0.0, 0.0))],
            &[(&left_ground, DVec2::new(-tile_size(), 0.0))],
        );
        closed_tile(&left);
        closed_tile(&right);
        for i in 0..200 {
            let y = 28.0 + i as f64 * 0.1;
            let (a, b) = (sample(&left, tile_size(), y), sample(&right, 0.0, y));
            assert!((a - b).abs() < 0.0001, "end {end} m before the border, y {y}: {a} != {b}");
        }
    }
}

#[test]
fn a_spline_in_a_cutting_is_left_to_the_hole_but_a_trough_is_not() {
    let c = curve([40.0, 40.0, 0.0], 0.0, 0.0);
    let profile = test_profile(&ditch(), &c, false, 1, DVec3::ZERO);
    let mut hill = ground();
    assert!(!profile_in_cutting(&profile, DVec3::ZERO, &hill, &[]), "flat ground");
    for v in hill.heights.iter_mut() {
        *v = 0.5;
    }
    assert!(!profile_in_cutting(&profile, DVec3::ZERO, &hill, &[]), "a lip 0.5 m under the hill");
    for v in hill.heights.iter_mut() {
        *v = 5.0;
    }
    assert!(profile_in_cutting(&profile, DVec3::ZERO, &hill, &[]), "a 5 m cutting");
}
