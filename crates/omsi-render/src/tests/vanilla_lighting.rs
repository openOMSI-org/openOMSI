use super::*;

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer")
}

fn wall(renderer: &mut Renderer, scene: &mut Scene, material: usize) {
    let mesh = renderer.add_mesh(
        scene,
        &MeshData {
            wire_wind: Vec::new(),
            positions: vec![
                Vec3::new(-4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, 4.0),
                Vec3::new(-4.0, 4.0, 4.0),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    renderer.add_instance(scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
}

fn centre(renderer: &mut Renderer, scene: &mut Scene, lighting: &Lighting) -> [u8; 3] {
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let rgba = renderer
        .render_to_image(scene, 64, 64, &camera, lighting)
        .unwrap();
    rgba[(32 * 64 + 32) * 4..][..3].try_into().unwrap()
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn vanilla_lighting_lamps_preserve_texture_colours_at_full_light() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let texture = renderer.add_texture(
        &mut scene,
        &omsi_texture::Image {
            width: 1,
            height: 1,
            rgba: vec![64, 128, 192, 255],
            has_alpha: false,
        },
        false,
    );
    let material = renderer.add_material(
        &mut scene,
        Some(texture),
        AlphaMode::Opaque,
        [1.0; 4],
        false,
    );
    wall(&mut renderer, &mut scene, material);
    // Several nearby platform lamps may overlap. Their light must saturate before
    // it multiplies the texture, rather than clipping all its channels to white.
    scene.lights = vec![
        PointLight {
            position: DVec3::new(0.0, 3.0, 0.0),
            radius: 24.0,
            intensity: 1.0,
            ..Default::default()
        };
        4
    ];
    for classic in [true, false] {
        let lighting = Lighting {
            classic,
            sun_intensity: 0.0,
            ambient: Vec3::ZERO,
            secondary: Vec3::ZERO,
            shadows: false,
            fog_density: 0.0,
            night: 1.0,
            ..Default::default()
        };
        let pixel = centre(&mut renderer, &mut scene, &lighting);
        assert!(
            pixel
                .iter()
                .zip([64u8, 128, 192])
                .all(|(a, b)| a.abs_diff(b) <= 2),
            "classic {classic}: a fully lit texture must retain its colours: {pixel:?}"
        );
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn vanilla_lighting_classic_specular_is_not_srgb_encoded_twice() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let material = renderer.add_material_extra(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 0.0, 1.0],
        false,
        None,
        None,
        None,
        None,
        [0.0; 3],
        MaterialExtra {
            ambient: Some([0.0; 3]),
            specular: [0.2, 0.2, 0.2, 1.0],
            ..Default::default()
        },
    );
    wall(&mut renderer, &mut scene, material);
    let lighting = Lighting {
        classic: true,
        sun_dir: -Vec3::Y,
        sun_color: Vec3::splat(0.5),
        sun_intensity: 1.0,
        ambient: Vec3::ZERO,
        secondary: Vec3::ZERO,
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let pixel = centre(&mut renderer, &mut scene, &lighting);
    // D3D's encoded specular term is at most 0.5 * 0.2 = 0.1 (26/255).
    assert!(
        pixel.iter().all(|&c| (18..=27).contains(&c)),
        "encoded highlight: {pixel:?}"
    );
}
