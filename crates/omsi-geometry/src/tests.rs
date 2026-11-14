use super::*;

#[test]
fn static_mesh_merge_preserves_geometry_uvs_and_material_order() {
    let a = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z],
        normals: vec![Vec3::Z; 4],
        uvs: vec![Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE],
        indices: vec![0, 1, 2, 1, 2, 3, 0, 2, 3],
        ranges: vec![(0, 3, 0), (3, 3, 1), (6, 3, 0)],
        one_sided: true,
    };
    let mut b = a.clone();
    for p in &mut b.positions { *p += Vec3::splat(20.0); }
    let merged = MeshData::merge_static(&[&a, &b]);
    assert_eq!(merged.positions, [a.positions.clone(), b.positions.clone()].concat());
    assert_eq!(merged.normals, [a.normals.clone(), b.normals.clone()].concat());
    assert_eq!(merged.uvs, [a.uvs.clone(), b.uvs.clone()].concat());
    assert_eq!(merged.indices, vec![0, 1, 2, 4, 5, 6, 1, 2, 3, 5, 6, 7, 0, 2, 3, 4, 6, 7]);
    assert_eq!(merged.ranges, vec![(0, 6, 0), (6, 6, 1), (12, 6, 0)]);
    assert!(merged.one_sided);
    // Same-slot adjacent profiles need only one draw and keep their triangle order.
    b.ranges = vec![(0, 3, 0), (3, 3, 0), (6, 3, 1)];
    let merged = MeshData::merge_static(&[&b, &b]);
    assert_eq!(merged.ranges, vec![(0, 12, 0), (12, 6, 1)]);
    assert_eq!(merged.indices.len(), b.indices.len() * 2);
}

/// A map object turned, pitched and banked a good deal at once stands as Omsi.exe puts
/// it: `v · RotationX(pitch) · RotationZ(bank) · RotationY(heading)` in Direct3D's
/// left-handed frame (x right, y up, z forward), read in the world's (x, z, y).
#[test]
fn map_angles_compose_as_omsi_does() {
    let d3d = |v: [f32; 3], h: f32, p: f32, b: f32| -> Vec3 {
        let (h, p, b) = (h.to_radians(), p.to_radians(), b.to_radians());
        let rx = |v: [f32; 3]| [v[0], v[1] * p.cos() - v[2] * p.sin(), v[1] * p.sin() + v[2] * p.cos()];
        let rz = |v: [f32; 3]| [v[0] * b.cos() - v[1] * b.sin(), v[0] * b.sin() + v[1] * b.cos(), v[2]];
        let ry = |v: [f32; 3]| [v[0] * h.cos() + v[2] * h.sin(), v[1], -v[0] * h.sin() + v[2] * h.cos()];
        let w = ry(rz(rx(v)));
        Vec3::new(w[0], w[2], w[1])
    };
    let (h, p, b) = (40.0, 35.0, -50.0);
    let r = object_rotation(map_rotation([h as f64, p as f64, b as f64]));
    for (lh, rh) in [([0.0, 0.0, 1.0], Vec3::Y), ([1.0, 0.0, 0.0], Vec3::X), ([0.0, 1.0, 0.0], Vec3::Z)] {
        let want = d3d(lh, h, p, b);
        let got = r.transform_vector3(rh);
        assert!((want - got).length() < 1e-4, "{lh:?}: {got:?} != {want:?}");
    }
}

/// A map object pitched and banked by the file's positive angles: the nose goes down
/// and the right side up, as in Direct3D's frame the file keeps them in.
#[test]
fn map_pitch_lowers_the_nose_and_bank_raises_the_right() {
    let r = object_rotation(map_rotation([0.0, 30.0, 0.0]));
    let forward = r.transform_vector3(Vec3::Y);
    assert!(forward.z < -0.4, "{forward:?}");
    let r = object_rotation(map_rotation([0.0, 0.0, 30.0]));
    let up = r.transform_vector3(Vec3::Z);
    assert!(up.x < -0.4, "{up:?}");
    // the heading stays as the file has it (clockwise from north)
    let r = object_rotation(map_rotation([90.0, 0.0, 0.0]));
    assert!(r.transform_vector3(Vec3::Y).x > 0.99);
}

/// Two road segments whose seam vertices came out 3 mm apart: a wheel's point in
/// the sliver between them stands on the road (it fell through to the ground under it),
/// while a point a few centimetres past the road's edge still does not.
#[test]
fn a_wheel_does_not_fall_through_a_seam() {
    let mut g = DriveGrid::default();
    let quad = |g: &mut DriveGrid, y0: f32, y1: f32| {
        g.push([Vec3::new(0.0, y0, 1.0), Vec3::new(8.0, y0, 1.0), Vec3::new(0.0, y1, 1.0)]);
        g.push([Vec3::new(8.0, y0, 1.0), Vec3::new(8.0, y1, 1.0), Vec3::new(0.0, y1, 1.0)]);
    };
    quad(&mut g, 0.0, 10.0);
    quad(&mut g, 10.003, 20.0);
    g.build(300.0);
    assert_eq!(g.probe(4.0, 10.0015, 2.0).below, Some(1.0));
    assert_eq!(g.surface_below(4.0, 10.0015, 2.0).map(|(z, _)| z), Some(1.0));
    assert_eq!(g.probe(8.03, 5.0, 2.0).below, None);
    assert_eq!(g.probe(4.0, 20.03, 2.0).below, None);
}

#[test]
fn thin_triangle_queries_stay_on_the_finite_face() {
    // An acute, sloping tip beside a road. Offset edge half-planes used to
    // accept (13.8, 9.24), 1.8 m past the tip, and invent a height of 1.52 m.
    let triangle = [Vec3::new(10.0, 10.0, 0.0), Vec3::new(12.0, 9.6, 0.8), Vec3::new(10.0, 9.999, 0.0)];
    for reversed in [false, true] {
        let mut face = triangle;
        if reversed { face.swap(1, 2); }
        for ridge in [false, true] {
            let mut grid = DriveGrid::default();
            assert!(grid.push_kind(face, ridge));
            grid.build(300.0);
            // The tip straddles a grid bucket boundary at x = 12: nearby
            // points must still find it, but their heights stay on the tip.
            for (x, y, expected) in [
                (11.0, 9.8, Some(0.4)),
                (12.002, 9.5996, Some(0.8)),
                (12.01, 9.598, None),
                (12.004, 9.596, None), // inside the expanded AABB, outside the radius
                (13.8, 9.24, None),
            ] {
                let near = grid.probe(x, y, 0.5);
                let wall = grid.probe_walls(x, y, 1.0);
                let surface = grid.surface_below(x, y, 1.0).map(|(z, _)| z);
                let mut heights = [0.0; 4];
                let mut top = None;
                let count = grid.heights(x, y, &mut heights, &mut top);
                let actual = if ridge { wall.below } else { near.below.or(near.above) };
                match expected {
                    Some(z) => {
                        assert!((actual.unwrap() - z).abs() < 1e-4, "({x}, {y}): {actual:?}");
                        assert!((0.0..=0.8).contains(&actual.unwrap()));
                        if ridge {
                            assert_eq!(near, Probe::default());
                            assert_eq!(count, 0);
                            assert_eq!(surface, None);
                            assert_eq!(top, actual);
                        } else {
                            assert_eq!(wall, Probe::default());
                            assert_eq!(count, 1);
                            assert_eq!(heights[0], actual.unwrap());
                            assert_eq!(surface, actual);
                            assert_eq!(top, None);
                            assert_eq!(near.below.is_some(), z <= 0.5);
                        }
                    }
                    None => {
                        assert_eq!(near, Probe::default(), "({x}, {y})");
                        assert_eq!(wall, Probe::default());
                        assert_eq!(surface, None);
                        assert_eq!(count, 0);
                        assert_eq!(top, None);
                    }
                }
            }
        }
    }
}

/// A kerb: road at 0, pavement at 0.15 from x = 10 on, a bridge deck at 6 m over it all.
#[test]
fn drive_grid_probes_the_face_under_the_axle() {
    let mut g = DriveGrid::default();
    let quad = |g: &mut DriveGrid, x0: f32, x1: f32, z: f32| {
        g.push([Vec3::new(x0, 0.0, z), Vec3::new(x1, 0.0, z), Vec3::new(x0, 20.0, z)]);
        g.push([Vec3::new(x1, 0.0, z), Vec3::new(x1, 20.0, z), Vec3::new(x0, 20.0, z)]);
    };
    quad(&mut g, 0.0, 10.0, 0.0);
    quad(&mut g, 10.0, 20.0, 0.15);
    quad(&mut g, 0.0, 20.0, 6.0);
    // the kerb face itself is a wall and never a place to stand
    g.push([Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 0.0), Vec3::new(10.0, 0.0, 0.15)]);
    g.build(300.0);
    assert_eq!(g.tris.len(), 6);
    let p = g.probe(5.0, 5.0, 0.5);
    assert_eq!(p, Probe { below: Some(0.0), above: Some(6.0) });
    let p = g.probe(12.0, 5.0, 0.5);
    assert_eq!(p.below, Some(0.15));
    // a probe that starts under the pavement's top sees it as a step above
    let p = g.probe(12.0, 5.0, 0.1);
    assert_eq!(p, Probe { below: None, above: Some(0.15) });
    // on the shared edge of two triangles
    assert_eq!(g.probe(5.0, 10.0, 0.5).below, Some(0.0));
    assert_eq!(g.probe(250.0, 250.0, 0.5), Probe::default());
}

#[test]
fn reflection_surface_uses_the_nearby_face_and_its_grade() {
    let mut grid = DriveGrid::default();
    let plane = |height: f32| [Vec3::new(0.0, 0.0, height),
        Vec3::new(20.0, 0.0, height + 2.0), Vec3::new(0.0, 20.0, height - 1.0)];
    // Reversed authoring winding must still give an upward normal.
    let mut road = plane(12.0);
    road.swap(1, 2);
    grid.push(road);
    grid.push(plane(20.0)); // bridge deck above the vehicle
    grid.push_kind(plane(12.3), true); // wall top is not a reflecting road
    grid.build(300.0);
    let (height, normal) = grid.surface_below(4.0, 5.0, 13.0).unwrap();
    assert!((height - 12.15).abs() < 1e-5);
    assert!(normal.distance(Vec3::new(-0.1, 0.05, 1.0).normalize()) < 1e-5);
    assert!(grid.surface_below(4.0, 5.0, 11.0).is_none());
    assert!(grid.surface_below(290.0, 290.0, 30.0).is_none());
}

#[test]
fn surf_maps_lift_the_faces_drawn_with_their_texture() {
    // red 0, 1, 0.5 and 1 along u: down 2 cm, up 2 cm, level, up 2 cm
    let rgba: Vec<u8> = [0u8, 255, 128, 255].iter().flat_map(|&r| [r, 0, 0, 255]).collect();
    let map = std::sync::Arc::new(HeightMap::from_rgba(4, 1, &rgba).unwrap());
    assert!((map.lift(Vec2::new(0.0, 0.3)) + 0.02).abs() < 1e-6);
    assert!((map.lift(Vec2::new(0.25, 0.0)) - 0.02).abs() < 1e-6);
    assert!(map.lift(Vec2::new(0.5, 0.0)).abs() < 1e-3);
    // tiled: 1.25 is 0.25; past the last texel's start the position stops there and
    // the second last texel (0.5) is read, not the next tile's first one
    assert!((map.lift(Vec2::new(1.25, 0.0)) - 0.02).abs() < 1e-6);
    assert!(map.lift(Vec2::new(0.875, 0.0)).abs() < 1e-3);
    // a spline's two faces: the one in the slot of a texture with a map is lifted, the
    // other one not; u runs 0..1 over 10 m in x
    let mesh = MeshData {
        positions: vec![
            Vec3::new(0.0, 0.0, 1.0), Vec3::new(10.0, 0.0, 1.0), Vec3::new(0.0, 10.0, 1.0),
            Vec3::new(20.0, 0.0, 1.0), Vec3::new(30.0, 0.0, 1.0), Vec3::new(20.0, 10.0, 1.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0),
            Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
        ranges: vec![(0, 3, 1), (3, 3, 0)],
        ..Default::default()
    };
    let surf = SurfFaces::of(&mesh, &[None, Some(map)]).unwrap();
    assert!(SurfFaces::of(&mesh, &[None, None]).is_none());
    // (the drive mesh keeps only the shape, as the tile loader stages it)
    let shape = MeshData { positions: mesh.positions.clone(), indices: mesh.indices.clone(), ..Default::default() };
    let mut ts = TileSurface::new(64);
    ts.add_spline_drive(&shape, Some(&surf), DVec3::ZERO, 0, 0);
    ts.finish();
    let at = |x: f32| ts.drive.probe(x, 1.0, 2.0).below.unwrap();
    assert!((at(2.5) - 1.02).abs() < 1e-4);
    assert!((at(0.1) - 0.98).abs() < 2e-3);
    assert!((at(22.5) - 1.0).abs() < 1e-6);
}

#[test]
fn declared_terrain_holes_cut_deep_ground_without_cutting_surroundings() {
    let side = tile_size() as f32;
    let (lo, hi, middle) = (side * 0.25, side * 0.75, side * 0.5);
    let cutter = MeshData {
        positions: vec![
            Vec3::new(lo, lo, -12.0), Vec3::new(hi, lo, -12.0),
            Vec3::new(hi, hi, -12.0), Vec3::new(lo, hi, -12.0),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Default::default()
    };
    let mut object_hole = TileSurface::new(64);
    object_hole.rasterize_hole(&cutter, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);

    // An aligned spline's generated outline has the same explicit-hole semantics.
    let def = strip(side * 0.25, 0.0);
    let curve = SplineCurve {
        start: DVec3::new(middle as f64, lo as f64, -12.0),
        ..plain_curve((hi - lo) as f64, 0.0)
    };
    let mut spline_hole = TileSurface::new(64);
    for ring in spline_hole_outlines(&def, &curve, false, 1) {
        assert!(!outline_crosses_itself(&ring));
        spline_hole.add_outline(&ring, 0, 0);
    }
    for hole in [&object_hole, &spline_hole] {
        assert!(hole.cuts_anything(&|_, _| 2.0, 0.12));
        assert!(hole.cut_at(middle, middle, 2.0, 0.12));
        assert!(!hole.cut_at(side * 0.1, middle, 2.0, 0.12));
        let mask = hole.mask_image(&|_, _| 2.0, 0.12);
        assert_eq!(mask[hole.texel(middle, middle) * 4 + 3], 0);
        assert_eq!(mask[hole.texel(side * 0.1, middle) * 4 + 3], 255);
    }

    // Ordinary deep road geometry still does not request an excavation, even when
    // the optional automatic road-cut heuristic is enabled.
    let mut road = TileSurface::new(64);
    road.rasterize_kind(&cutter, &Mat4::IDENTITY, DVec3::ZERO, 0, 0, true);
    assert!(!road.cuts_anything(&|_, _| 2.0, 0.12));
    assert!(!road.cut_at(middle, middle, 2.0, 0.12));
}

#[test]
fn a_terrain_hole_is_cut_along_its_rim_not_by_texel() {
    let side = tile_size() as f32;
    let cell = side / 64.0;
    // a cutter from texel 16 to the middle of texel 40 in x, 16..48 in y
    let (x0, x1, y0, y1) = (16.0 * cell, 40.5 * cell, 16.0 * cell, 48.0 * cell);
    let cutter = MeshData {
        positions: vec![Vec3::new(x0, y0, 0.0), Vec3::new(x1, y0, 0.0), Vec3::new(x1, y1, 0.0), Vec3::new(x0, y1, 0.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Default::default()
    };
    let mut ts = TileSurface::new(64);
    ts.rasterize_hole(&cutter, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
    let mask = ts.mask_image(&|_, _| 0.0, 0.12);
    let a = |i: usize, j: usize| mask[(j * 64 + i) * 4 + 3];
    // inside up to the rim's texels: no ground left (the old one-texel erosion kept the
    // rim texels' ground, a strip of grass along the carriageway's edge)
    assert_eq!(a(16, 30), 0);
    assert_eq!(a(39, 30), 0);
    assert_eq!(a(30, 47), 0);
    // the texel the rim halves keeps half its ground, the one beyond it all
    assert!((120..=135).contains(&a(40, 30)), "{}", a(40, 30));
    assert_eq!(a(41, 30), 255);
    assert_eq!(a(15, 30), 255);
    // on a fine raster the ground is kept for `HOLE_KEEP` inside the rim: the texel
    // along the rim keeps a little, the next one none
    let cell = side / 512.0;
    let (x0, x1, y0, y1) = (100.0 * cell, 300.0 * cell, 100.0 * cell, 300.0 * cell);
    let cutter = MeshData {
        positions: vec![Vec3::new(x0, y0, 0.0), Vec3::new(x1, y0, 0.0), Vec3::new(x1, y1, 0.0), Vec3::new(x0, y1, 0.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Default::default()
    };
    let mut ts = TileSurface::new(512);
    ts.rasterize_hole(&cutter, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
    let mask = ts.mask_image(&|_, _| 0.0, 0.12);
    let a = |i: usize, j: usize| mask[(j * 512 + i) * 4 + 3];
    let kept = ((HOLE_KEEP / (cell / 4.0)) + 0.5).floor().min(4.0) as u32;
    assert_eq!(a(100, 200) as u32, 255 * (4 * kept) / 16, "cell {cell}");
    assert_eq!(a(299, 200) as u32, 255 * (4 * kept) / 16);
    assert_eq!(a(102, 200), 0);
    assert_eq!(a(200, 200), 0);
}

#[test]
fn surface_raster_keeps_its_layers() {
    let mut ts = TileSurface::new(512);
    let cell = tile_size() as f32 / 512.0;
    let quad = |z: f32| {
        let mut m = MeshData::default();
        for (x, y) in [(10.0f32, 10.0f32), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)] {
            m.positions.push(Vec3::new(x, y, z));
        }
        m.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        m
    };
    // a plate at 2 m, a road at 1 m under part of it, a hole cutter elsewhere
    ts.rasterize_kind(&quad(2.0), &Mat4::IDENTITY, DVec3::ZERO, 0, 0, false);
    ts.rasterize_kind(&quad(1.0), &Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)), DVec3::ZERO, 0, 0, true);
    ts.rasterize_hole(&quad(0.5), &Mat4::from_translation(Vec3::new(100.0, 100.0, 0.0)), DVec3::ZERO, 0, 0);
    ts.finish();
    let k = |x: f32, y: f32| ts.texel(x, y);
    assert_eq!(ts.sample(12.0, 20.0), Some(2.0));
    assert_eq!(ts.sample_road(12.0, 20.0), None);
    assert_eq!(ts.sample(25.0, 20.0), Some(2.0));
    assert_eq!(ts.sample_road(25.0, 20.0), Some(1.0));
    assert_eq!(ts.low_height(k(25.0, 20.0)), 1.0);
    assert_eq!(ts.low_height(k(12.0, 20.0)), 2.0);
    assert_eq!(ts.sample(33.0, 20.0), Some(1.0));
    assert_eq!(ts.sample(200.0, 200.0), None);
    assert_eq!(ts.hole_height(k(115.0, 115.0)), Some(0.5));
    assert_eq!(ts.hole_height(k(12.0, 20.0)), None);
    assert!(ts.cut_at(115.0, 115.0, 0.0, 0.12));
    assert!(!ts.cut_at(200.0, 200.0, 0.0, 0.12));
    // a surface alone takes no ground away (only the map's holes do, as in Omsi.exe)
    // unless OMSI_ROAD_CUT asks for it
    assert_eq!(ts.cut_at(12.0, 20.0, 1.95, 0.12), road_cut());
    assert!(!ts.cut_at(12.0, 20.0, 1.5, 0.12));
    // an aligned spline's outline cuts exactly along it, whatever the heights
    ts.add_outline(&[DVec2::new(50.0, 50.0), DVec2::new(60.0, 50.0), DVec2::new(60.0, 51.0), DVec2::new(50.0, 51.0)], 0, 0);
    assert!(ts.cut_at(55.0, 50.5, 40.0, 0.12));
    assert!(!ts.cut_at(55.0, 51.3, 0.0, 0.12));
    let mask = ts.mask_image(&|_, _| 0.0, 0.12);
    let n = ts.size;
    let a = |x: f32, y: f32| mask[((y / tile_size() as f32 * n as f32) as usize * n + (x / tile_size() as f32 * n as f32) as usize) * 4 + 3];
    assert!(a(55.0, 50.5) < 128 && a(55.0, 53.0) == 255);
    // only the touched blocks hold memory: a few kilobytes, not the 5 MB of a dense raster
    assert!(ts.heap_bytes().0 < 200_000, "{}", ts.heap_bytes().0);
    let _ = cell;
}

#[test]
fn height_profile_mesh_follows_the_segments() {
    let mut def = Spline::default();
    def.height_profiles.push(omsi_scenery::sli::HeightProfile { x0: -4.5, x1: 4.5, z0: 0.1, z1: 0.1 });
    def.height_profiles.push(omsi_scenery::sli::HeightProfile { x0: 4.5, x1: 7.5, z0: 0.25, z1: 0.25 });
    def.height_profiles.push(omsi_scenery::sli::HeightProfile { x0: -1.25, x1: -1.25, z0: 0.25, z1: 0.25 });
    let c = SplineCurve { start: DVec3::new(10.0, 10.0, 30.0), heading_deg: 0.0, length: 20.0, radius: 0.0, grad_start: 0.0, grad_end: 0.0, delta_h: None, cant_start: 0.0, cant_end: 0.0, skew_start: 0.0, skew_end: 0.0, tex_offset: 0.0, seed: 0, half_cant_width: DEFAULT_HALF_CANT_WIDTH };
    let m = build_height_profile_mesh(&def, &c, false, DVec3::ZERO);
    let mut g = DriveGrid::default();
    for t in m.indices.chunks_exact(3) {
        g.push([m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]]);
    }
    g.build(300.0);
    assert_eq!(g.tris.len(), 8, "the zero-width segment is no surface (two cross-sections every 10 m)");
    assert!((g.probe(10.0, 15.0, 31.0).below.unwrap() - 30.1).abs() < 1e-4);
    assert!((g.probe(16.0, 15.0, 31.0).below.unwrap() - 30.25).abs() < 1e-4);
    // mirrored, the pavement is on the left
    let m = build_height_profile_mesh(&def, &c, true, DVec3::ZERO);
    let lo = m.positions.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    assert!((lo - 2.5).abs() < 1e-4, "{lo}");
}

#[test]
fn terrain_height_follows_the_drawn_triangles() {
    // one cell raised at its lower right corner only: the drawn surface is a flat
    // triangle over half the cell, the bilinear patch is not
    let mut t = Terrain::flat();
    let n = t.samples();
    t.heights.iter_mut().for_each(|h| *h = 0.0);
    t.heights[1] = 1.0; // (ix 1, iy 0)
    let cell = tile_size() as f32 / t.cells as f32;
    let _ = n;
    // the diagonal runs from (0,0) to (1,1): on it the raised corner is not felt
    let h = terrain_height(&t, cell * 0.5, cell * 0.5);
    assert!(h.abs() < 1e-4, "{h}");
    // in the lower triangle, (0,0)-(1,0)-(1,1), the corner pulls linearly
    let h = terrain_height(&t, cell * 0.75, cell * 0.25);
    assert!((h - 0.5).abs() < 1e-4, "{h}");
    // the upper triangle, (0,0)-(1,1)-(0,1), does not reach it
    let h = terrain_height(&t, cell * 0.25, cell * 0.75);
    assert!(h.abs() < 1e-4, "{h}");
    // bilinear would give 0.5625 at (0.75, 0.25)
    assert!((t.sample_bilinear(cell * 0.75, cell * 0.25) - 0.5625).abs() < 1e-4);
}

#[test]
fn straight_and_arc() {
    let c = SplineCurve { start: DVec3::ZERO, heading_deg: 90.0, length: 10.0, radius: 0.0, grad_start: 0.0, grad_end: 0.0, delta_h: None, cant_start: 0.0, cant_end: 0.0, skew_start: 0.0, skew_end: 0.0, tex_offset: 0.0, seed: 0, half_cant_width: DEFAULT_HALF_CANT_WIDTH };
    let e = c.end_point();
    assert!((e.x - 10.0).abs() < 1e-9 && e.y.abs() < 1e-9);
    // quarter circle to the right from heading 0 with radius 10 ends at (10, 10), heading 90
    let c = SplineCurve { start: DVec3::ZERO, heading_deg: 0.0, length: std::f64::consts::FRAC_PI_2 * 10.0, radius: 10.0, grad_start: 0.0, grad_end: 0.0, delta_h: None, cant_start: 0.0, cant_end: 0.0, skew_start: 0.0, skew_end: 0.0, tex_offset: 0.0, seed: 0, half_cant_width: DEFAULT_HALF_CANT_WIDTH };
    let e = c.end_point();
    assert!((e.x - 10.0).abs() < 1e-9 && (e.y - 10.0).abs() < 1e-9, "{e:?}");
    assert!((c.heading_at(c.length) - 90.0).abs() < 1e-9);
}

fn plain_curve(length: f64, radius: f64) -> SplineCurve {
    SplineCurve { start: DVec3::ZERO, heading_deg: 0.0, length, radius, grad_start: 0.0, grad_end: 0.0, delta_h: None, cant_start: 0.0, cant_end: 0.0, skew_start: 0.0, skew_end: 0.0, tex_offset: 0.0, seed: 0, half_cant_width: DEFAULT_HALF_CANT_WIDTH }
}

fn strip(half: f32, v_scale: f32) -> Spline {
    let mut def = Spline::default();
    def.textures.push(Default::default());
    def.profiles.push(omsi_scenery::sli::SplineProfile {
        texture: 0,
        points: vec![
            omsi_scenery::sli::SplineProfilePoint { x: -half, u: 0.0, v_scale, ..Default::default() },
            omsi_scenery::sli::SplineProfilePoint { x: half, u: 1.0, v_scale, ..Default::default() },
        ],
    });
    def
}

/// Omsi.exe counts a cross-section every half degree of turn, but never more than its
/// buffers hold: an island's nose (radius 2, a quarter turn) is round, a road of many
/// profiles stays at a few dozen.
#[test]
fn stations_every_half_degree_within_the_buffer() {
    let nose = plain_curve(std::f64::consts::FRAC_PI_2 * 2.0, 2.0);
    let def = strip(2.0, 0.1);
    assert_eq!(spline_station_count(&def, &nose), 180);
    let m = build_spline_mesh(&def, &nose, false, DVec3::ZERO);
    assert_eq!(m.positions.len(), 181 * 2);
    // 30 profile segments: 3870 / 180 indices = 21 cross-sections at most
    let mut road = Spline::default();
    for k in 0..30 {
        road.profiles.push(omsi_scenery::sli::SplineProfile {
            texture: 0,
            points: vec![
                omsi_scenery::sli::SplineProfilePoint { x: k as f32, ..Default::default() },
                omsi_scenery::sli::SplineProfilePoint { x: k as f32 + 1.0, ..Default::default() },
            ],
        });
    }
    assert_eq!(spline_station_count(&road, &plain_curve(50.0, 30.0)), 21);
    // straight and level: one every 10 m
    assert_eq!(spline_station_count(&def, &plain_curve(95.0, 0.0)), 9);
    assert_eq!(spline_station_count(&def, &plain_curve(4.0, 0.0)), 1);
}

/// The skew shifts every cross-section, running from its start value to its end value.
#[test]
fn skew_runs_along_the_spline() {
    let def = strip(2.0, 0.0);
    let c = SplineCurve { skew_end: 1.0, ..plain_curve(20.0, 0.0) };
    let m = build_spline_mesh(&def, &c, false, DVec3::ZERO);
    // two cross-sections: at 0, 10 and 20 m; the right edge (x = 2) at the end is 2 m on
    let right_end = m.positions[5];
    assert!((right_end.y - 22.0).abs() < 1e-4 && (right_end.x - 2.0).abs() < 1e-4, "{right_end:?}");
    let left_end = m.positions[4];
    assert!((left_end.y - 18.0).abs() < 1e-4, "{left_end:?}");
    // half way the skew is a half
    assert!((m.positions[3].y - 11.0).abs() < 1e-4, "{:?}", m.positions[3]);
    // the drivable surface follows
    let mut def = def;
    def.height_profiles.push(omsi_scenery::sli::HeightProfile { x0: -2.0, x1: 2.0, z0: 0.0, z1: 0.0 });
    let hp = build_height_profile_mesh(&def, &c, false, DVec3::ZERO);
    assert!((hp.positions[5].y - 22.0).abs() < 1e-4);
}

/// A chain's textures run on across its joints: the next spline starts where the
/// last one's v ended (the map's texture offset); a mirrored spline turns v round.
#[test]
fn textures_run_on_along_a_chain() {
    let def = strip(2.0, 0.37);
    let a = plain_curve(13.0, 0.0);
    let b = SplineCurve { start: a.end_point(), tex_offset: 13.0, ..plain_curve(9.0, 0.0) };
    let ma = build_spline_mesh(&def, &a, false, DVec3::ZERO);
    let mb = build_spline_mesh(&def, &b, false, DVec3::ZERO);
    let end_a = ma.uvs[ma.uvs.len() - 1].y;
    let start_b = mb.uvs[1].y;
    assert!((end_a - start_b).rem_euclid(1.0).min(1.0 - (end_a - start_b).rem_euclid(1.0)) < 1e-4, "{end_a} {start_b}");
    let mm = build_spline_mesh(&def, &a, true, DVec3::ZERO);
    assert!((mm.uvs[mm.uvs.len() - 1].y + end_a).abs() < 1e-5);
    // [scaleTexByLength]: 0..v_scale over the whole spline
    let mut def = def;
    def.textures[0].scale_by_length = true;
    let m = build_spline_mesh(&def, &plain_curve(55.0, 0.0), false, DVec3::ZERO);
    assert!((m.uvs[m.uvs.len() - 1].y - 0.37).abs() < 1e-6);
}

/// Stock `str_2spur_8m_DDR_Bahnstr.sli`: 16 parts of a 512x4096 texture, 5 m each; the
/// parts follow on letter by letter and the spline ends on the first letter again.
#[test]
fn patchwork_parts_join_up() {
    let mut def = strip(3.6, 0.1);
    def.textures[0].patchwork = Some(omsi_scenery::sli::PatchworkChain {
        segment_length: 5.0,
        chain: "AAAABBAACAAADAAAA".into(),
        weights: "1110001111100155".into(),
        invertable: "0000000000000000".into(),
    });
    for seed in [1u32, 4190, 3215067, 123456789] {
        let c = SplineCurve { seed, ..plain_curve(47.3, 0.0) };
        let mut n = spline_station_count(&def, &c);
        let pw = patchwork(&def, &c, &mut n).unwrap();
        assert_eq!(pw.order.len(), 10);
        assert_eq!(n % 10, 0);
        let chain = b"AAAABBAACAAADAAAA";
        let mut state = b'A';
        for &p in &pw.order {
            assert!(p > 0 && (p as usize) <= 16);
            let k = p as usize - 1;
            assert_eq!(chain[k], state, "seed {seed}: {:?}", pw.order);
            assert!(b"1110001111100155"[k] != b'0');
            state = chain[k + 1];
        }
        assert_eq!(state, b'A');
        // the same spline always gets the same order
        let mut n2 = spline_station_count(&def, &c);
        assert_eq!(patchwork(&def, &c, &mut n2).unwrap().order, pw.order);
        // each stretch shows 1/16 of the texture
        let m = build_spline_mesh(&def, &c, false, DVec3::ZERO);
        let (v0, v1) = (m.uvs[0].y, m.uvs[2].y);
        let p = pw.order[0] as f32 - 1.0;
        assert!((v0 - p / 16.0).abs() < 1e-6 && (v1 - (p / 16.0 + 1.0 / 16.0 / pw.per as f32)).abs() < 1e-6, "{v0} {v1}");
    }
}

#[test]
fn spline_h_height() {
    // the Ahlheim underpass ramp: 73.86 m, leaves at 6.19 %, arrives level, 5.85 m up
    let c = SplineCurve { start: DVec3::new(0.0, 0.0, -5.79), heading_deg: 0.0, length: 73.86, radius: 0.0, grad_start: 6.19, grad_end: 0.0, delta_h: Some(5.85), cant_start: 0.0, cant_end: 0.0, skew_start: 0.0, skew_end: 0.0, tex_offset: 0.0, seed: 0, half_cant_width: DEFAULT_HALF_CANT_WIDTH };
    assert!((c.height_at(0.0) + 5.79).abs() < 1e-9);
    assert!((c.height_at(c.length) - 0.06).abs() < 1e-9, "{}", c.height_at(c.length));
    assert!((c.slope_at(0.0) - 0.0619).abs() < 1e-9);
    assert!(c.slope_at(c.length).abs() < 1e-9);
    // the slope is the derivative of the height
    let s = 30.0;
    let d = (c.height_at(s + 1e-4) - c.height_at(s - 1e-4)) / 2e-4;
    assert!((d - c.slope_at(s)).abs() < 1e-6, "{d} {}", c.slope_at(s));
    // a plain spline keeps its gradient parabola
    let p = SplineCurve { delta_h: None, ..c };
    assert!((p.height_at(p.length) - (-5.79 + 73.86 * 0.0619 / 2.0)).abs() < 1e-9);
}

#[test]
fn a_backwards_quad_with_an_unmirrored_matrix_can_keep_its_winding() {
    let v = |x: f32, y: f32| omsi_o3d::Vertex { position: Vec3::new(x, y, 1.0), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::ZERO };
    let o3d = omsi_o3d::Mesh { vertices: vec![v(0.0, 0.0), v(0.0, 1.0), v(1.0, 0.0), v(1.0, 1.0)], triangles: vec![omsi_o3d::Triangle { indices: [0, 1, 2], material: 0 }, omsi_o3d::Triangle { indices: [2, 1, 3], material: 0 }], materials: vec![omsi_o3d::Material::default()], transform: glam::Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0)), has_transform: true, ..Default::default() };
    assert_eq!(positive_det_faces_forward(&o3d), Some(false));
    assert!(turns_round(&o3d));
    // the same faces from a file without a matrix (an `.x`, an old `.o3d`) or with the
    // identity: drawn as wound, as Omsi.exe draws every mesh (#874)
    let plain = omsi_o3d::Mesh { has_transform: false, ..o3d.clone() };
    assert!(!turns_round(&plain));
    assert_eq!(mesh_from_o3d(&plain).indices[..3], [0, 1, 2]);
    let identity = omsi_o3d::Mesh { transform: glam::Mat4::IDENTITY, ..o3d.clone() };
    assert!(!turns_round(&identity));
    assert_eq!(mesh_from_o3d(&o3d).indices[..3], [0, 2, 1]);
    let mut kept = mesh_from_o3d_turning(&o3d, false);
    assert_eq!(kept.indices[..3], [0, 1, 2]);
    reverse_winding(&mut kept);
    assert_eq!(kept.indices, mesh_from_o3d(&o3d).indices);
}

#[test]
fn the_exporters_quarter_turn_matrix_does_not_turn_a_mesh_round() {
    let v = |x: f32, y: f32| omsi_o3d::Vertex { position: Vec3::new(x, y, 1.0), normal: Vec3::new(0.0, 1.0, 1.0), uv: Vec2::ZERO };
    let quarter_turn = glam::Mat4::from_cols(glam::Vec4::X, glam::Vec4::new(0.0, 0.0, -1.0, 0.0), glam::Vec4::new(0.0, 1.0, 0.0, 0.0), glam::Vec4::new(0.0, 0.0251, 0.0, 1.0));
    let o3d = omsi_o3d::Mesh {
        vertices: vec![v(0.0, 0.0), v(0.0, 1.0), v(1.0, 0.0), v(1.0, 1.0)],
        triangles: vec![omsi_o3d::Triangle { indices: [0, 1, 2], material: 0 }, omsi_o3d::Triangle { indices: [2, 1, 3], material: 0 }],
        materials: vec![omsi_o3d::Material::default()],
        transform: quarter_turn,
        has_transform: true,
        ..Default::default()
    };
    assert_eq!(positive_det_faces_forward(&o3d), Some(false));
    assert!(!turns_round(&o3d));
    assert_eq!(mesh_from_o3d(&o3d).indices[..3], [0, 1, 2]);
}

#[test]
fn inverted_wheel_mesh_turns_round() {
    let v = |x: f32, y: f32| omsi_o3d::Vertex { position: Vec3::new(x, y, 1.0), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::ZERO };
    let o3d = omsi_o3d::Mesh {
        vertices: vec![v(0.0, 0.0), v(0.0, 1.0), v(1.0, 0.0), v(1.0, 1.0)],
        triangles: vec![omsi_o3d::Triangle { indices: [0, 1, 2], material: 0 }, omsi_o3d::Triangle { indices: [2, 1, 3], material: 0 }],
        materials: vec![omsi_o3d::Material::default()],
        transform: glam::Mat4::IDENTITY,
        has_transform: true,
        ..Default::default()
    };
    assert!(mesh_faces_against_normals(&o3d));
    assert!(!turns_round(&o3d));
    let wheel = mesh_from_o3d_wheel(&o3d);
    assert_eq!(wheel.indices[..3], [0, 2, 1]);
}

#[test]
fn cone_filtered_rays_hit_what_the_whole_mesh_does() {
    let mut seed = 7u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let mut positions = Vec::new();
    for _ in 0..1500 {
        positions.push(Vec3::new(rnd() * 2.0, 3.0 + rnd() * 2.0, rnd() * 2.0));
    }
    let mesh = MeshData { indices: (0..1500).collect(), positions, ..Default::default() };
    let xf = Mat4::from_rotation_z(0.3) * Mat4::from_translation(Vec3::new(0.2, 0.0, 0.1));
    let o = Vec3::new(0.1, -1.0, 0.05);
    let axis = Vec3::new(0.05, 1.0, 0.02).normalize();
    let spread = 0.02;
    let tris = cone_triangles(o, axis, spread * 2.0 + 1e-4, &mesh, &xf);
    assert!(tris.len() < 500);
    let right = Vec3::new(-axis.y, axis.x, 0.0).normalize();
    let up = axis.cross(right).normalize();
    for ring in 0..=2 {
        for k in 0..(8 * ring).max(1) {
            let a = k as f32 / (8 * ring).max(1) as f32 * std::f32::consts::TAU;
            let r = spread * ring as f32;
            let d = (axis + right * (a.cos() * r) + up * (a.sin() * r)).normalize();
            assert_eq!(ray_triangles(o, d, &mesh, &xf, &tris), ray_mesh(o, d, &mesh, &xf));
        }
    }
}

#[test]
fn o3d_front_faces_arrive_clockwise() {
    // A triangle a viewer at the origin looking along +z sees from its front in the
    // file's Direct3D frame (x right, y up, z forward): clockwise there, the normal
    // pointing back at the viewer.
    let v = |x: f32, y: f32, z: f32| omsi_o3d::Vertex { position: Vec3::new(x, y, z), normal: Vec3::new(0.0, 0.0, -1.0), uv: Vec2::ZERO };
    let o3d = omsi_o3d::Mesh { vertices: vec![v(0.0, 0.0, 1.0), v(0.0, 1.0, 1.0), v(1.0, 0.0, 1.0)], triangles: vec![omsi_o3d::Triangle { indices: [0, 1, 2], material: 0 }], materials: vec![omsi_o3d::Material::default()], ..Default::default() };
    let m = mesh_from_o3d(&o3d);
    assert!(m.one_sided);
    // the same viewer in the world frame looks along +y with z up: the screen shows
    // world x to the right and world z upwards
    let p: Vec<(f32, f32)> = m.indices.iter().map(|i| (m.positions[*i as usize].x, m.positions[*i as usize].z)).collect();
    let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
    assert!(area < 0.0, "front face must be clockwise on the screen (the renderer's front face), area {area}");
    assert_eq!(m.normals[0], Vec3::new(0.0, -1.0, 0.0));
}

#[test]
fn d3d_normals_face_the_front() {
    // the front-facing triangle of `o3d_front_faces_arrive_clockwise`, its file normals
    // pointing away: recomputed, they point back at the viewer as D3DX makes them
    let v = |x: f32, y: f32, z: f32| omsi_o3d::Vertex { position: Vec3::new(x, y, z), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::ZERO };
    let o3d = omsi_o3d::Mesh { vertices: vec![v(0.0, 0.0, 1.0), v(0.0, 1.0, 1.0), v(1.0, 0.0, 1.0)], triangles: vec![omsi_o3d::Triangle { indices: [0, 1, 2], material: 0 }], materials: vec![omsi_o3d::Material::default()], ..Default::default() };
    let mut m = mesh_from_o3d(&o3d);
    assert_eq!(m.normals[0], Vec3::new(0.0, 1.0, 0.0));
    compute_normals_d3d(&mut m);
    assert!(m.normals.iter().all(|n| (*n - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-6), "{:?}", m.normals);
}
