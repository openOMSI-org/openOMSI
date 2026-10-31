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
    // Fallback quads have no extrusion; the last covering triangle is the displayed one.
    verts
        .chunks_exact(3)
        .filter(|t| {
            let p: [glam::Vec2; 3] =
                std::array::from_fn(|i| glam::Vec2::new(t[i].pos[0], t[i].pos[1]));
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
