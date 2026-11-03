use super::*;
use omsi_map::layers::{LAND_GRASS, Layers};

fn parts(anchor: DVec2, crowded: bool) -> GroundParts {
    let mut layers = Layers::empty(anchor);
    let q = |x, y| glam::Vec2::new(x, y);
    layers.land[LAND_GRASS].extend([
        q(-400.0, -400.0),
        q(400.0, -400.0),
        q(400.0, 400.0),
        q(-400.0, -400.0),
        q(400.0, 400.0),
        q(-400.0, 400.0),
    ]);
    if crowded {
        for _ in 0..MAX_GROUND_VERTS / 3 {
            layers.land[LAND_GRASS].extend([q(-100.0, -100.0), q(-99.9, -100.0), q(-100.0, -99.9)]);
        }
    }
    let roads = [-250.0, 250.0]
        .map(|x| {
            (
                MapRoad {
                    points: [-300.0, 300.0]
                        .map(|y| DVec3::new(anchor.x + x, anchor.y + y, 0.0))
                        .to_vec(),
                    width: 4.0,
                    main: false,
                },
                false,
            )
        })
        .into();
    GroundParts {
        roads: Arc::new(roads),
        net: None,
        md: Some(Arc::new(MapData::new(Arc::new(layers)))),
        surface: None,
        contours: None,
        house_reach: None,
    }
}

fn look() -> GroundLook {
    GroundLook {
        surfaces: false,
        trees: false,
        dashes: false,
        tol: 0.25,
    }
}

fn color_at(verts: &[Vertex], at: glam::Vec2) -> Option<[f32; 4]> {
    // Resolve road extrusion at the test build scale; the last covering triangle is visible.
    verts
        .chunks_exact(3)
        .filter(|t| {
            let p: [glam::Vec2; 3] = std::array::from_fn(|i| {
                let v = t[i];
                glam::Vec2::new(v.pos[0], v.pos[1])
                    + glam::Vec2::from(v.ext) * v.width[0].max(v.width[1] * 0.5)
            });
            if (p[1] - p[0]).perp_dot(p[2] - p[0]).abs() < 1e-10 {
                return false;
            }
            let cross = [0, 1, 2].map(|i| (p[(i + 1) % 3] - p[i]).perp_dot(at - p[i]));
            cross.iter().all(|s| *s >= -1e-4) || cross.iter().all(|s| *s <= 1e-4)
        })
        .last()
        .map(|t| t[0].color)
}

#[test]
fn dense_land_keeps_roads_and_background_after_a_pan() {
    let pal = style::palette(false);
    for anchor in [DVec2::ZERO, DVec2::new(17000.0, -23000.0)] {
        let built = build(
            &parts(anchor, true),
            anchor,
            500.0,
            pal,
            look(),
            false,
            None,
            true,
            0.5,
        );
        let ground = &built.vertices()[0];
        assert!(ground.len() <= MAX_GROUND_VERTS);
        assert_eq!(ground.len() % 3, 0);
        for (x, color) in [(-250.0, pal.road), (0.0, pal.grass), (250.0, pal.road)] {
            let got =
                color_at(ground, glam::Vec2::new(x, 0.0)).expect("the whole view is still drawn");
            assert!(
                got.iter().zip(color.0).all(|(a, b)| (*a - b).abs() < 1e-5),
                "x {x}: {got:?} != {:?}",
                color.0
            );
        }
        let bytes = built.counts.iter().sum::<usize>() * std::mem::size_of::<Vertex>();
        assert!(bytes <= (MAX_GROUND_VERTS + 3 * MAX_HOUSE_VERTS) * std::mem::size_of::<Vertex>());
    }
}

#[test]
fn ordinary_maps_keep_the_exact_ground_and_launcher_geometry() {
    let parts = parts(DVec2::ZERO, false);
    let pal = style::palette(false);
    let exact = build(
        &parts,
        DVec2::ZERO,
        500.0,
        pal,
        look(),
        false,
        None,
        false,
        0.5,
    );
    let capped = build(
        &parts,
        DVec2::ZERO,
        500.0,
        pal,
        look(),
        false,
        None,
        true,
        0.5,
    );
    assert_eq!(capped.vertices(), exact.vertices());
}

#[test]
fn the_launchers_map_is_not_cut_by_the_gpu_budget() {
    let map = MapGround {
        parts: parts(DVec2::ZERO, true),
    };
    let built = map.build(DVec2::ZERO, 500.0, 3.0, false);
    assert!(
        built.counts[0] > MAX_GROUND_VERTS,
        "launcher remains uncapped as in #3301"
    );
    assert!(
        built.vertices()[0]
            .iter()
            .any(|v| v.color == style::palette(false).road.0)
    );
}

#[test]
fn dense_background_keeps_the_original_road_vertices_and_markings() {
    use omsi_map::layers::{PAINT_WHITE, PAVE_INNER};
    for anchor in [DVec2::ZERO, DVec2::new(17000.0, -23000.0)] {
        let mut parts = parts(anchor, true);
        let mut layers = (*parts.md.as_ref().unwrap().layers).clone();
        layers.paved[PAVE_INNER].extend([
            glam::Vec2::new(0.13, 0.27),
            glam::Vec2::new(5.19, 0.27),
            glam::Vec2::new(0.13, 10.31),
        ]);
        layers.paint[PAINT_WHITE].extend([
            glam::Vec2::new(1.13, 1.27),
            glam::Vec2::new(1.29, 1.27),
            glam::Vec2::new(1.13, 8.31),
        ]);
        let mut road_layers = layers.clone();
        road_layers.land.iter_mut().for_each(Vec::clear);
        let mut roads_only = parts.clone();
        roads_only.md = Some(Arc::new(MapData::new(Arc::new(road_layers))));
        parts.md = Some(Arc::new(MapData::new(Arc::new(layers))));
        let look = GroundLook {
            surfaces: true,
            ..look()
        };
        let pal = style::palette(false);
        let expected = build(
            &roads_only,
            anchor,
            500.0,
            pal,
            look,
            true,
            None,
            false,
            0.1,
        );
        let actual = build(&parts, anchor, 500.0, pal, look, true, None, true, 0.1);
        assert!(actual.counts[0] <= MAX_GROUND_VERTS);
        assert!(
            actual.vertices()[0].ends_with(&expected.vertices()[0]),
            "dense land must not rasterize road edges or subpixel lane markings"
        );
    }
}

#[test]
fn ground_cells_keep_large_triangles_crossing_the_view_from_distant_centres() {
    use omsi_map::layers::PAVE_INNER;
    let mut layers = Layers::empty(DVec2::ZERO);
    let triangle = [
        glam::Vec2::new(-5000.0, -5000.0),
        glam::Vec2::new(15000.0, -5000.0),
        glam::Vec2::new(-5000.0, 15000.0),
    ];
    layers.land[LAND_GRASS].extend(triangle);
    layers.paved[PAVE_INNER].extend(triangle);
    let parts = GroundParts {
        roads: Arc::new(Vec::new()),
        net: None,
        md: Some(Arc::new(MapData::new(Arc::new(layers)))),
        surface: None,
        contours: None,
        house_reach: None,
    };
    let built = build(
        &parts,
        DVec2::ZERO,
        20.0,
        style::palette(false),
        look(),
        true,
        None,
        true,
        0.1,
    );
    assert_eq!(
        built.counts[0], 6,
        "large triangles must not vanish because their indexing cell is outside the view"
    );
}

#[test]
#[ignore = "reads a local map cache selected with NAVIGATOR_TEST_LAYERS and NAVIGATOR_TEST_CENTRES"]
fn measure_local_cached_map_ground_coverage() {
    use std::io::Read;
    let path = std::env::var("NAVIGATOR_TEST_LAYERS").expect("local map cache path");
    let mut header = [0u8; 16];
    std::fs::File::open(&path)
        .unwrap()
        .read_exact(&mut header)
        .unwrap();
    let key = u64::from_le_bytes(header[8..16].try_into().unwrap());
    let layers = omsi_map::layers::Layers::open(std::path::Path::new(&path), key)
        .expect("compatible map cache");
    let md = Arc::new(MapData::new(Arc::new(layers)));
    let centres = std::env::var("NAVIGATOR_TEST_CENTRES").expect("world centres as x,y;x,y");
    let pal = style::palette(false);
    let look = GroundLook {
        surfaces: true,
        trees: true,
        dashes: true,
        tol: 0.0,
    };
    for centre in centres.split(';') {
        let (x, y) = centre.split_once(',').unwrap();
        let anchor = DVec2::new(x.parse().unwrap(), y.parse().unwrap());
        let mut old = Painter::new();
        md.build_paved(&mut old, anchor, anchor, 640.0, pal, look);
        let old_count = old.verts.len();
        drop(old);
        let reach = super::view::padded(super::view::panel_reach(120.0, 400.0, 300.0).unwrap());
        let mut roads = Painter::with_ground_budget(MAX_GROUND_VERTS - 6, reach as f32, 0.3);
        let start = std::time::Instant::now();
        md.build_paved(&mut roads, anchor, anchor, reach, pal, look);
        assert!(
            !roads.ground_reduced(),
            "the local visible roads must keep original triangles"
        );
        let road_count = roads.finish_ground().len();
        let mut land =
            Painter::with_ground_budget(MAX_GROUND_VERTS - road_count, reach as f32, 0.3);
        md.build_land(&mut land, anchor, anchor, reach, pal, look);
        assert!(!land.ground_reduced(), "the local background must fit too");
        let land_count = land.finish_ground().len();
        assert!(road_count + land_count <= MAX_GROUND_VERTS);
        eprintln!(
            "local cache centre {anchor}, reach {reach:.1} m: 640 m candidates with crossing guard {old_count}, clipped roads {road_count}, land {land_count}, CPU {:?}; both layers exact",
            start.elapsed()
        );
    }
}

#[test]
fn a_road_crossing_the_view_is_kept_when_both_endpoints_are_outside() {
    let mut parts = parts(DVec2::ZERO, false);
    parts.md = None;
    parts.roads = Arc::new(vec![(
        MapRoad {
            points: vec![DVec3::new(-1000.0, 0.0, 0.0), DVec3::new(1000.0, 0.0, 0.0)],
            width: 4.0,
            main: false,
        },
        false,
    )]);
    let built = build(
        &parts,
        DVec2::ZERO,
        20.0,
        style::palette(false),
        look(),
        false,
        None,
        true,
        0.1,
    );
    assert!(
        built.counts[0] > 0,
        "a crossing segment is visible even when neither endpoint is in range"
    );
}

#[test]
fn taller_buildings_keep_their_conservative_scope_when_the_ground_view_shrinks() {
    let mut layers = Layers::empty(DVec2::ZERO);
    layers.buildings.push(omsi_map::layers::Building {
        outline: vec![
            glam::Vec2::new(490.0, 0.0),
            glam::Vec2::new(510.0, 0.0),
            glam::Vec2::new(510.0, 20.0),
            glam::Vec2::new(490.0, 20.0),
        ],
        roof: vec![[0, 1, 2], [0, 2, 3]],
        height: 150.0,
        ..Default::default()
    });
    let mut parts = parts(DVec2::ZERO, false);
    parts.md = Some(Arc::new(MapData::new(Arc::new(layers))));
    parts.house_reach = Some(640.0);
    let built = build(
        &parts,
        DVec2::ZERO,
        20.0,
        style::palette(false),
        look(),
        false,
        Some(0),
        true,
        0.1,
    );
    assert!(
        built.counts[2] > 0,
        "ground clipping must not shrink the standing buildings' coverage"
    );
}
