//! Artificial road and scenery fixtures.
use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        // A counter besides the clock: the clock ticks in microseconds on macOS, and two
        // tests running side by side made the same folder in the same tick, wrote over each
        // other's files and deleted them under each other.
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "openomsi-crossing-field-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// .x coordinates are x across, z up, y along. Use separate stations so the
// uncovered entrance remains flat rather than interpolating a distant field lift.
fn strip(stations: &[(f32, f32)]) -> String {
    let vertices: Vec<_> = stations
        .iter()
        .flat_map(|&(y, z)| [-3.0, 3.0].map(|x| format!("{x};{z};{y};")))
        .collect();
    let faces: Vec<_> = (0..stations.len() - 1)
        .flat_map(|i| {
            let a = 2 * i;
            [
                format!("3;{a},{},{a_plus};", a + 2, a_plus = a + 1),
                format!("3;{},{},{};", a + 1, a + 2, a + 3),
            ]
        })
        .collect();
    format!(
        "xof 0303txt 0032\nMesh strip {{\n{};\n{};\n{};\n{};\n}}\n",
        vertices.len(),
        vertices.join(",\n"),
        faces.len(),
        faces.join(",\n")
    )
}

fn staged(path: PathBuf, ot: Arc<ObjectType>, pose: Pose) -> StagedTile {
    StagedTile {
        tx: 0,
        ty: 0,
        origin: DVec3::ZERO,
        path,
        base_terrain: Terrain::flat(),
        align: Vec::new(),
        hole_rims: Vec::new(),
        water: None,
        bakes_light_map: false,
        splines: Vec::new(),
        meshes: Mutex::new(Some(Vec::new())),
        drive: Vec::new(),
        lanes: Mutex::new(Vec::new()),
        street_points: Vec::new(),
        sound_lines: Vec::new(),
        objects: vec![StagedObject {
            ot,
            id: 1,
            place: Placement::Pose(pose),
            rules: Vec::new(),
            extra: Vec::new(),
            lamp_parent: None,
            parked: false,
            map_object: true,
            instance: 0,
            key: 1,
        }],
        anchors: Vec::new(),
        counts: LoadStats::default(),
        resolved: std::sync::OnceLock::new(),
    }
}

#[test]
fn aligned_ditch_preparation_keeps_a_solid_floor_without_an_alpha_hole() {
    let fixture = Fixture::new();
    fixture.write(
        "global.cfg",
        "[name]\nAligned ditch\n[map]\n0\n0\ntile_0_0.map\n",
    );
    fixture.write("ditch.sli", "[heightprofile]\n2.92\n5.35\n0.10\n0.04\n[texture]\ngras1.bmp\n[profile]\n0\n[profilepnt]\n2.92\n0.10\n0.7\n0.167\n[profilepnt]\n4.23\n-0.47\n0.7\n0.167\n[profilepnt]\n5.35\n0.04\n0.2\n0.167\n");
    for flag in [
        "[spline_terrain_align]".to_string(),
        "[spline_terrain_align_2]\n4".to_string(),
    ] {
        fixture.write("tile_0_0.map", &format!("[version]\n14\n[spline]\n0\nditch.sli\n1\n0\n0\n40\n0\n40\n0\n20\n0\n0\n0\n0\n0\n0\n0\n0\n\n{flag}\n"));
        let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
        let (prepared, _) = world.prepare_tiles(&[(0, 0, fixture.0.join("tile_0_0.map"))]);
        assert_eq!(prepared.len(), 1);
        let p = &prepared[0];
        assert!(
            !p.splines.is_empty(),
            "the fixture spline must actually be loaded"
        );
        assert!(
            p.cut.is_none(),
            "an aligned ditch must not punch a raster hole in its solid backing"
        );
        assert!(
            p.hole_walls.is_empty(),
            "the shoulders are ground geometry, not vertical walls"
        );
        // Inject a fitting error at the production result handler. The original
        // outline must still cut the saved ground, including a neighboring tile.
        let cache = world.staged.lock();
        let q = cache.get(&(0, 0)).unwrap();
        assert!(!q.hole_rims.is_empty());
        for (key, origin) in [
            ((0, 0), DVec3::ZERO),
            ((0, 1), DVec3::new(0.0, tile_size(), 0.0)),
        ] {
            let mut ts = TileSurface::new(64);
            let mut rims = Vec::new();
            let fallback = crate::cut_terrain::finish_spline_fit(
                Err("injected triangulation error"),
                &q.base_terrain,
                &mut ts,
                &mut rims,
                &[q],
                key,
                origin,
            );
            assert_eq!(
                fallback.positions,
                build_terrain_mesh(&q.base_terrain).positions
            );
            assert_eq!(rims.len(), q.hole_rims.len());
            assert_eq!(rims[0][0], q.hole_rims[0][0] - origin);
            if key == (0, 0) {
                assert!(
                    ts.cut_at(44.23, 50.0, 0.0, 0.03),
                    "failure must retain the original ditch hole"
                );
            }
        }
        drop(cache);
        let terrain = p.terrain.as_ref().unwrap();
        assert!(field_height(terrain, 44.23, 50.0).unwrap() < -0.47);
        for y in [39.5, 39.75, 40.0, 50.0, 60.0, 60.25, 60.5] {
            assert!(
                field_height(terrain, 44.23, y).is_some(),
                "the extended end cannot expose the sky"
            );
        }
    }
}

#[test]
fn aligned_ditch_shoulder_does_not_cover_the_adjacent_road() {
    check_adjacent_asphalt(true);
}

#[test]
fn aligned_ditch_shoulder_respects_asphalt_without_vehicle_paths() {
    check_adjacent_asphalt(false);
}

fn check_adjacent_asphalt(has_vehicle_paths: bool) {
    let fixture = Fixture::new();
    fixture.write(
        "global.cfg",
        "[name]\nDitch next to road\n[map]\n0\n0\ntile_0_0.map\n",
    );
    fixture.write("ditch.sli", "[heightprofile]\n2.92\n5.35\n0.10\n0.04\n[texture]\ngras1.bmp\n[profile]\n0\n[profilepnt]\n2.92\n0.10\n0.7\n0.167\n[profilepnt]\n4.23\n-0.47\n0.7\n0.167\n[profilepnt]\n5.35\n0.04\n0.2\n0.167\n");
    let mut asphalt = "[heightprofile]\n-3\n2.92\n0\n0\n[texture]\nasphalt.bmp\n[profile]\n0\n[profilepnt]\n-3\n0\n0\n1\n[profilepnt]\n2.92\n0\n1\n1\n".to_string();
    if has_vehicle_paths {
        asphalt.push_str("[path]\n0\n0\n0\n3\n0\n");
    }
    fixture.write("road.sli", &asphalt);
    let mut tile = "[version]\n14\n".to_string();
    for (id, file, z, flag) in [
        (1, "ditch.sli", 0.18, "[spline_terrain_align_2]\n4\n"),
        (2, "road.sli", 0.03, ""),
    ] {
        tile.push_str(&format!("[spline]\n0\n{file}\n{id}\n0\n0\n40\n{z}\n40\n0\n20\n0\n0\n0\n0\n0\n0\n0\n0\n\n{flag}\n"));
    }
    fixture.write("tile_0_0.map", &tile);
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
    let (prepared, _) = world.prepare_tiles(&[(0, 0, fixture.0.join("tile_0_0.map"))]);
    assert_eq!(prepared.len(), 1);
    let p = &prepared[0];
    assert!(
        p.cut.is_none(),
        "road protection must not create an alpha hole"
    );
    let terrain = p.terrain.as_ref().unwrap();
    for y in [40.1, 45.0, 50.0, 55.0, 59.9] {
        for x in [41.0, 42.0, 42.8, 42.91] {
            assert!(
                field_height(terrain, x, y).unwrap() <= 0.028,
                "the real road surface at 3 cm must remain visible at {x}, {y}"
            );
        }
    }
    assert!(
        field_height(terrain, 44.23, 50.0).unwrap() < -0.29,
        "the ditch floor must remain underneath its graphical profile"
    );
}

#[test]
fn streaming_overlapping_ditches_at_negative_tiles_keeps_the_entire_batch() {
    let fixture = Fixture::new();
    let keys = [(-28, 6), (-29, 6), (-28, 7)];
    let mut global = "[name]\nStreaming ditch regression\n".to_string();
    let mut tiles = Vec::new();
    for (tx, ty) in keys {
        let name = format!("tile_{tx}_{ty}.map");
        global.push_str(&format!("[map]\n{tx}\n{ty}\n{name}\n"));
        let mut tile = "[version]\n14\n".to_string();
        for (i, heading, dx, dy) in [
            (1, 7.123, 0.0, 0.0),
            (2, 7.123, 0.00000317, -0.00000197),
            (3, 24.25, 0.0, 0.0),
        ] {
            tile.push_str(&format!("[spline]\n0\nditch.sli\n{i}\n0\n0\n{}\n0\n{}\n{heading}\n20\n31.319\n0\n0\n0\n0\n0\n0\n0\nmirror\n\n[spline_terrain_align]\n", 39.731 + dx, 40.127 + dy));
        }
        fixture.write(&name, &tile);
        assert_eq!(
            omsi_map::Tile::load(&fixture.0.join(&name))
                .unwrap()
                .splines
                .len(),
            3
        );
        tiles.push((tx, ty, fixture.0.join(name)));
    }
    fixture.write("global.cfg", &global);
    fixture.write("ditch.sli", "[heightprofile]\n2.92\n5.35\n0.10\n0.04\n[texture]\ngras1.bmp\n[profile]\n0\n[profilepnt]\n2.92\n0.10\n0.7\n0.167\n[profilepnt]\n4.23\n-0.47\n0.7\n0.167\n[profilepnt]\n5.35\n0.04\n0.2\n0.167\n");
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
    let (prepared, _) = world.prepare_tiles(&tiles);
    assert_eq!(
        prepared.len(),
        tiles.len(),
        "no tile in the streaming batch may be discarded"
    );
    for p in prepared {
        assert!(
            !p.splines.is_empty(),
            "the three source splines may be merged into a render batch"
        );
        assert!(!p.terrain.as_ref().unwrap().indices.is_empty());
        assert!(p.cut.is_none());
    }
}

#[test]
fn uncovered_crossing_entrance_preserves_render_and_wheel_heights() {
    for field_sign in [-1.0, 1.0] {
        // Also retain an intentional 20 cm step; deformation must not stitch it away.
        for authored_step in [0.0, 0.2] {
            let fixture = Fixture::new();
            fixture.write("global.cfg", "[name]\nArtificial junction\n");
            fixture.write("junction.sco", "[surface]\n[absheight]\n[mesh]\njunction.x\n[crossing_heightdeformation]\nfield.x\n");
            fixture.write(
                "junction.x",
                &strip(&[0.0, 2.0, 4.0, 6.0, 8.0].map(|y| (y, 0.1 + authored_step))),
            );
            fixture.write(
                "field.x",
                &strip(&[(4.0, field_sign * 0.4), (8.0, field_sign * 0.8)]),
            );
            let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
            let ot = world
                .object_type("junction.sco")
                .expect("artificial junction loads");
            let field = ot.deform.as_ref().unwrap();
            assert_eq!(field_height(field, 0.0, 0.0), None);

            for elevation in [0.0, 20.0] {
                for heading in [0.0, 37.0] {
                    fixture.write("tile_0_0.map", &format!(
                        "[version]\n14\n[object]\n0\njunction.sco\n1\n100\n100\n{elevation}\n{heading}\n0\n0\n0\n\n"));
                    let tile = omsi_map::Tile::load(&fixture.0.join("tile_0_0.map")).unwrap();
                    let object = &tile.objects[0];
                    let pose = Pose {
                        pos: DVec3::from(object.pos),
                        rot: object_rotation(omsi_geometry::map_rotation(object.rot)),
                    };
                    let src: HashMap<_, _> = [(
                        (0, 0),
                        Arc::new(staged(fixture.0.join("tile_0_0.map"), ot.clone(), pose)),
                    )]
                    .into_iter()
                    .collect();
                    let warped = world.warp_crossings(&src[&(0, 0)], &src);
                    let mesh = &warped.get(&0).expect("covered stations are deformed")[0];
                    for (before, after) in ot.meshes[0].0.positions.iter().zip(&mesh.positions) {
                        assert_eq!(after.truncate(), before.truncate());
                        let expected = if before.y < 4.0 {
                            before.z
                        } else {
                            before.z + field_sign * before.y * 0.1
                        };
                        assert!(
                            (after.z - expected).abs() < 1e-6,
                            "vertex {before:?}: {after:?}, expected {expected}"
                        );
                        if before.y < 4.0 {
                            assert_eq!(
                                after.z.to_bits(),
                                before.z.to_bits(),
                                "uncovered vertices remain exact"
                            );
                        }
                    }
                    assert!(mesh
                        .normals
                        .iter()
                        .all(|n| n.is_finite() && (n.length() - 1.0).abs() < 1e-5));

                    let road = omsi_geometry::mesh_from_o3d(
                        &omsi_o3d::xfile::parse_x(strip(&[(-5.0, 0.1), (0.0, 0.1)]).as_bytes())
                            .unwrap(),
                    );
                    let mut surface = TileSurface::new(128);
                    surface.add_drive_mesh(&road, &pose.rot, pose.pos, 0, 0);
                    surface.add_drive_mesh(mesh, &pose.rot, pose.pos, 0, 0);
                    surface.finish();
                    // The production wheel query, across the seam at both tyre tracks.
                    for lateral in [-1.5, 1.5] {
                        let sample = |y| {
                            let p = pose.pos
                                + pose
                                    .rot
                                    .transform_point3(glam::Vec3::new(lateral, y, 0.0))
                                    .as_dvec3();
                            probe_tile(Some(&surface), None, (0, 0), p.x, p.y, elevation + 2.0)
                                .below
                                .unwrap()
                        };
                        let approach = sample(-0.05);
                        let entrance = sample(0.05);
                        assert!((approach - (elevation + 0.1)).abs() < 1e-5);
                        assert!(
                            (entrance - approach - authored_step as f64).abs() < 1e-5,
                            "wheel step {approach} -> {entrance}, authored {authored_step}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_crossing_entirely_outside_its_field_keeps_its_original_mesh() {
    let fixture = Fixture::new();
    fixture.write("global.cfg", "[name]\nArtificial uncovered junction\n");
    fixture.write(
        "junction.sco",
        "[surface]\n[mesh]\njunction.x\n[crossing_heightdeformation]\nfield.x\n",
    );
    fixture.write("junction.x", &strip(&[(0.0, 0.1), (2.0, 0.1)]));
    fixture.write("field.x", &strip(&[(4.0, 0.4), (8.0, 0.8)]));
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
    let ot = world.object_type("junction.sco").unwrap();
    let src: HashMap<_, _> = [(
        (0, 0),
        Arc::new(staged(
            fixture.0.join("tile_0_0.map"),
            ot.clone(),
            Pose {
                pos: DVec3::new(100.0, 100.0, 20.0),
                rot: Mat4::IDENTITY,
            },
        )),
    )]
    .into_iter()
    .collect();
    assert!(world.warp_crossings(&src[&(0, 0)], &src).is_empty());
    assert!(ot.meshes[0].0.positions.iter().all(|p| p.z == 0.1));
}

#[test]
fn ai_lane_heights_are_independent_of_visual_vertex_coverage() {
    for covered_vertices in [false, true] {
        for field_sign in [-1.0, 0.0, 1.0] {
            let fixture = Fixture::new();
            fixture.write(
                "global.cfg",
                "[name]\nArtificial AI junction\n[map]\n0\n0\ntile_0_0.map\n",
            );
            fixture.write(
                "tile_0_0.map",
                "[version]\n14\n[object]\n0\njunction.sco\n1\n100\n100\n20\n37\n0\n0\n0\n\n",
            );
            fixture.write("junction.sco", "[surface]\n[absheight]\n[mesh]\njunction.x\n[crossing_heightdeformation]\nfield.x\n[path]\n0\n0\n0.1\n0\n0\n10\n0\n0\n0\n3\n0\n0\n");
            // The coarse variant spans the field with triangles but has no vertex
            // inside it. AI path samples at y=4 and y=6 are covered in both variants.
            let stations = if covered_vertices {
                vec![(0.0, 0.1), (4.0, 0.1), (6.0, 0.1), (10.0, 0.1)]
            } else {
                vec![(0.0, 0.1), (10.0, 0.1)]
            };
            fixture.write("junction.x", &strip(&stations));
            fixture.write(
                "field.x",
                &strip(&[(4.0, field_sign * 0.4), (6.0, field_sign * 0.6)]),
            );
            let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
            let (prepared, _) = world.prepare_tiles(&[(0, 0, fixture.0.join("tile_0_0.map"))]);
            assert_eq!(prepared.len(), 1);
            assert_eq!(prepared[0].objects.len(), 1);
            assert_eq!(
                prepared[0].objects[0].warped.is_some(),
                covered_vertices && field_sign != 0.0
            );

            let lanes = world.traffic_inbox.peek(omsi_sim::world_data::TrafficArrivals::lanes_of);
            assert_eq!(lanes.len(), 1, "the map's artificial [path] is placed");
            let lane = &lanes[0];
            assert_eq!(lane.points.len(), 6);
            let rotation = object_rotation([37.0, 0.0, 0.0]);
            for (i, point) in lane.points.iter().enumerate() {
                let y = i as f32 * 2.0;
                let expected_xy = DVec3::new(100.0, 100.0, 20.0)
                    + rotation
                        .transform_point3(glam::Vec3::new(0.0, y, 0.1))
                        .as_dvec3();
                assert!((point.truncate() - expected_xy.truncate()).length() < 1e-5);
                let offset = if (4.0..=6.0).contains(&y) {
                    field_sign * y * 0.1
                } else {
                    0.0
                };
                assert!((point.z - (20.1 + offset as f64)).abs() < 1e-5,
                    "covered_vertices={covered_vertices}, field_sign={field_sign}, y={y}: lane height {}", point.z);
            }
        }
    }
}

// A scenery backdrop can cover several distant areas in separate meshes. Splitting
// one object into parts must not change the source tiles from which it is visible.
#[test]
fn split_backdrop_keeps_whole_object_visibility_bounds() {
    let fixture = Fixture::new();
    fixture.write("global.cfg", "[name]\nArtificial background\n");
    fixture.write(
        "split.sco",
        "[rendertype]\nsurface\n[absheight]\n[mesh]\nsouth.x\n[mesh]\nnorth.x\n",
    );
    fixture.write(
        "single.sco",
        "[rendertype]\nsurface\n[absheight]\n[mesh]\nwhole.x\n",
    );
    fixture.write("south.x", &strip(&[(-1400.0, 0.0), (-1000.0, 0.0)]));
    fixture.write("north.x", &strip(&[(300.0, 0.2), (1100.0, 0.2)]));
    fixture.write(
        "whole.x",
        &strip(&[(-1400.0, 0.0), (-1000.0, 0.0), (300.0, 0.2), (1100.0, 0.2)]),
    );
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
    let split = world.object_type("split.sco").unwrap();
    let single = world.object_type("single.sco").unwrap();
    assert_eq!(split.meshes.len(), 2);
    let tile = (-3, 4);
    let ts = tile_size();
    let expected = Some([-4.0 * ts, 3.0 * ts, -1.0 * ts, 6.0 * ts]);
    for heading in [0.0, 37.0, 90.0] {
        let xf = object_rotation([heading, 0.0, 0.0]);
        let pos = DVec3::new(-770.0, 1450.0, 20.0);
        assert_eq!(stand_in_area(&single, &xf, pos, tile), expected);
        assert_eq!(stand_in_area(&split, &xf, pos, tile), expected,
            "splitting a large backdrop into meshes must preserve its visibility at heading {heading}");
        // Placement scale still controls the footprint, just as for a single mesh.
        let small = xf * Mat4::from_scale(glam::Vec3::splat(0.5));
        assert_eq!(stand_in_area(&split, &small, pos, tile), None);
    }
}

#[test]
fn ordinary_split_object_keeps_full_view_distance() {
    let fixture = Fixture::new();
    fixture.write("global.cfg", "[name]\nArtificial road structure\n");
    fixture.write(
        "road.sco",
        "[surface]\n[absheight]\n[mesh]\nwest.x\n[mesh]\neast.x\n",
    );
    // Even a sizeable road structure remains below the existing whole-object cutoff.
    fixture.write("west.x", &strip(&[(-750.0, 0.1), (0.0, 0.1)]));
    fixture.write("east.x", &strip(&[(0.0, 0.1), (750.0, 0.1)]));
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
    let road = world.object_type("road.sco").unwrap();
    for heading in [0.0, 37.0, 90.0] {
        assert_eq!(
            stand_in_area(
                &road,
                &object_rotation([heading, 0.0, 0.0]),
                DVec3::ZERO,
                (0, 0)
            ),
            None
        );
    }
}

// A backdrop card can start at its placement and run far out from there (HafenCity's
// `3_BG_niederbaum`, 1.6 km of the far bank of the Elbe). Its materials, drawn without
// the depth test or depth writes, tell it from an ordinary long model of the same size.
#[test]
fn backdrop_card_from_its_origin_is_a_stand_in() {
    let fixture = Fixture::new();
    fixture.write("global.cfg", "[name]
Artificial harbour
");
    fixture.write("card.x", &strip(&[(0.0, 0.0), (800.0, 30.0), (1600.0, 0.0)]));
    fixture.write(
        "backdrop.sco",
        "[mesh]
card.x
[matl]
harbour.dds
0
[matl_alpha]
2
[matl_noZcheck]
",
    );
    fixture.write("pier.sco", "[mesh]
card.x
[matl]
harbour.dds
0
");
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261008).unwrap();
    let backdrop = world.object_type("backdrop.sco").unwrap();
    let pier = world.object_type("pier.sco").unwrap();
    let tile = (-6, -4);
    let ts = tile_size();
    let expected = Some([-7.0 * ts, -5.0 * ts, -4.0 * ts, -2.0 * ts]);
    for heading in [0.0, 37.0, 90.0] {
        let xf = object_rotation([heading, 0.0, 0.0]);
        let pos = DVec3::new(-1515.0, -914.0, 0.0);
        assert_eq!(stand_in_area(&backdrop, &xf, pos, tile), expected,
            "a backdrop card at heading {heading} keeps to the tiles around its own");
        assert_eq!(stand_in_area(&pier, &xf, pos, tile), None,
            "an ordinary model of the same size keeps the far view at heading {heading}");
    }
}

#[test]
fn aligned_overhead_only_spline_keeps_saved_ground() {
    let fixture = Fixture::new();
    fixture.write(
        "global.cfg",
        "[name]\nOverhead regression\n[map]\n0\n0\ntile_0_0.map\n",
    );
    fixture.write("wire.sli", "[texture]\nwire.bmp\n[profile]\n0\n[profilepnt]\n-0.1\n5.5\n0\n1\n[profilepnt]\n0.1\n5.5\n1\n1\n");
    fixture.write("tile_0_0.map", "[version]\n14\n[spline]\n0\nwire.sli\n1\n0\n0\n40\n0\n40\n0\n20\n0\n0\n0\n0\n0\n0\n0\n0\n\n[spline_terrain_align]\n");
    let mut saved = Terrain::flat();
    for y in 0..saved.samples() {
        for x in 0..saved.samples() {
            let i = y * saved.samples() + x;
            saved.heights[i] = x as f32 * 0.013 + y as f32 * 0.021 + (x * y) as f32 * 0.0001;
        }
    }
    std::fs::write(fixture.0.join("tile_0_0.map.terrain"), saved.to_bytes()).unwrap();
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
    let (prepared, _) = world.prepare_tiles(&[(0, 0, fixture.0.join("tile_0_0.map"))]);
    assert_eq!(prepared.len(), 1);
    let cache = world.staged.lock();
    let staged = cache.get(&(0, 0)).unwrap();
    assert_eq!(staged.splines.len(), 1, "fixture must load a real spline");
    assert!(!staged.align.is_empty(), "fixture must be aligned");
    assert!(!staged.splines[0].cuts_terrain);
    assert!(staged.splines[0].terrain_profile.is_none());
    assert_eq!(
        staged.base_terrain, saved,
        "use the authored, non-flat terrain"
    );
    let expected = build_terrain_mesh(&saved);
    let actual = prepared[0].terrain.as_ref().unwrap();
    assert_eq!(actual.positions, expected.positions);
    assert_eq!(actual.indices, expected.indices);
    assert_eq!(actual.normals, expected.normals);
    assert!(prepared[0].cut.is_none());
    assert!(prepared[0].hole_walls.is_empty());
}
