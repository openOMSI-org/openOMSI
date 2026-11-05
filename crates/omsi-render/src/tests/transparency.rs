use super::*;

/// A scene of a green wall behind two quads in one model, the near one an invisible
/// `[matl_alpha] 2` pane (alpha 0 everywhere, depth written) drawn first: `second` is the
/// far quad's material, `ordered` whether the model is drawn in model order. Returns the
/// middle pixel for the classic, the Vanilla+ and the Enhanced picture at MSAA 1 and 4.
fn behind_an_invisible_pane(second: impl Fn(&mut Renderer, &mut Scene) -> MaterialId, ordered: bool, split: bool) -> Vec<([u8; 3], String)> {
    invisible_pane_scene(second, ordered, split, false)
}

fn invisible_pane_scene(second: impl Fn(&mut Renderer, &mut Scene) -> MaterialId, ordered: bool, split: bool, second_in_front: bool) -> Vec<([u8; 3], String)> {
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut out = Vec::new();
    for (msaa, ssao) in [(1, false), (4, false), (1, true), (4, true)] {
        let mut renderer = pollster::block_on(Renderer::new_with(
            &adapter,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa,
                ssao,
                shadow_size: 1024,
                fxaa: false,
                render_scale: 1.0,
                ..Default::default()
            },
        ))
        .expect("test renderer");
        let mut scene = renderer.new_scene();
        let clear = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 1, height: 1, rgba: vec![255, 255, 255, 0], has_alpha: true }, false);
        let near = renderer.add_material(&mut scene, Some(clear), AlphaMode::Blend, [1.0; 4], true);
        let far = second(&mut renderer, &mut scene);
        let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
        let quad = |y: f32, half: f32| [Vec3::new(-half, y, -half), Vec3::new(half, y, -half), Vec3::new(half, y, half), Vec3::new(-half, y, half)];
        let wall = renderer.add_mesh(
            &mut scene,
            &MeshData {
                positions: quad(8.0, 20.0).to_vec(),
                normals: vec![-Vec3::Y; 4],
                uvs: vec![glam::Vec2::ZERO; 4],
                ranges: vec![(0, 6, 0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                one_sided: false,
            },
        );
        renderer.add_instance(&mut scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
        let (pane_at, second_at) = if second_in_front { ((4.0, 8.0), (2.0, 4.0)) } else { ((2.0, 4.0), (4.0, 8.0)) };
        if split {
            for (y, half, material) in [(pane_at.0, pane_at.1, near), (second_at.0, second_at.1, far)] {
                let layer = renderer.add_mesh(
                    &mut scene,
                    &MeshData {
                        positions: quad(y, half).to_vec(),
                        normals: vec![-Vec3::Y; 4],
                        uvs: vec![glam::Vec2::ZERO; 4],
                        ranges: vec![(0, 6, 0)],
                        indices: vec![0, 1, 2, 0, 2, 3],
                        one_sided: false,
                    },
                );
                let i = renderer.add_instance(&mut scene, layer, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
                renderer.set_ordered(&mut scene, i, ordered);
            }
        } else {
            let layers = renderer.add_mesh(
                &mut scene,
                &MeshData {
                    positions: [quad(pane_at.0, pane_at.1), quad(second_at.0, second_at.1)].concat(),
                    normals: vec![-Vec3::Y; 8],
                    uvs: vec![glam::Vec2::ZERO; 8],
                    ranges: vec![(0, 6, 0), (6, 6, 1)],
                    indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
                    one_sided: false,
                },
            );
            let i = renderer.add_instance(&mut scene, layers, DVec3::ZERO, Mat4::IDENTITY, vec![near, far]);
            renderer.set_ordered(&mut scene, i, ordered);
        }
        let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
        for (classic, enhanced) in [(true, false), (false, false), (false, true)] {
            let lighting = Lighting { classic, enhanced, shadows: false, fog_density: 0.0, ..Default::default() };
            let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
            let c = &rgba[(32 * 64 + 32) * 4..][..3];
            out.push(([c[0], c[1], c[2]], format!("classic {classic}, enhanced {enhanced}, MSAA {msaa}, SSAO {ssao}")));
        }
    }
    out
}

/// Omsi.exe draws `[matl_alpha] 2` with alpha blending, the alpha test off and the depth
/// written (0x7fd6c4: ALPHATESTENABLE 0, ZWRITEENABLE unless `[matl_noZwrite]`): a pane
/// invisible all over still hides what its model draws behind it later. Content uses it
/// as a mask - a scroll box whose lines vanish under an invisible cover (#2059).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn an_invisible_pane_hides_what_its_model_draws_behind_it_later() {
    // a blended layer behind it, in the order the model lists them
    let blue = |r: &mut Renderer, s: &mut Scene| {
        let t = r.add_texture(s, &omsi_texture::Image { width: 1, height: 1, rgba: vec![0, 0, 255, 128], has_alpha: true }, false);
        r.add_material(s, Some(t), AlphaMode::Blend, [1.0; 4], true)
    };
    for (c, case) in behind_an_invisible_pane(blue, false, false) {
        assert!(c[1] > 100 && c[2] < 40, "the blue layer showed through the invisible pane: {c:?}; {case}");
    }
    // the scroll box: an opaque disc behind the cover, the model drawn in its order
    let red = |r: &mut Renderer, s: &mut Scene| r.add_material(s, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    for (c, case) in behind_an_invisible_pane(red, true, false) {
        assert!(c[1] > 100 && c[0] < 40, "the disc showed through the invisible cover: {c:?}; {case}");
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_part_in_front_of_an_earlier_invisible_pane_stays_seen() {
    let red = |r: &mut Renderer, s: &mut Scene| r.add_material(s, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    for split in [false, true] {
        for (c, case) in invisible_pane_scene(red, true, split, true) {
            assert!(c[0] > 100 && c[1] < 40, "the part in front of the pane was lost: {c:?}; {case}, split {split}");
        }
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn an_invisible_cover_in_its_own_mesh_hides_the_sun_blind_and_not_the_street() {
    let blind = |r: &mut Renderer, s: &mut Scene| r.add_material(s, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    for (c, case) in behind_an_invisible_pane(blind, true, true) {
        assert!(c[1] > 100 && c[0] < 40, "the sun blind or the clear colour showed through the cover: {c:?}; {case}");
    }
}

#[test]
fn oversized_colour_texture_keeps_its_silhouette_when_fitted() {
    let side = 2048u32;
    let rgba = (0..side * side)
        .flat_map(|i| {
            if i % side % 2 == 0 && i / side % 2 == 0 {
                [40, 80, 120, 255]
            } else {
                [255, 255, 255, 0]
            }
        })
        .collect();
    let data = omsi_texture::TextureData {
        width: side,
        height: side,
        format: omsi_texture::PixelFormat::Rgba8,
        levels: vec![rgba],
        has_alpha: true,
        gpu_mips: true,
    };
    let small = fit_texture(&data, 1024).unwrap();
    assert_eq!((small.width, small.height), (1024, 1024));
    assert!(small.levels[0].chunks_exact(4).all(|p| {
        p[..3]
            .iter()
            .zip([40u8, 80, 120])
            .all(|(a, b)| a.abs_diff(b) <= 1)
            && p[3] == 64
    }));
}
