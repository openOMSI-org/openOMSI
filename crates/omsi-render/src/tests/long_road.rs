use super::*;

/// A marking lying flat on a long road keeps its place at every distance: the road's two
/// huge triangles reach from the eye to the horizon, and their depth must be the plane's
/// whatever each pixel's own distance (#3085: markings went and came back as the bus drove
/// towards them, where the pull of the road stopped growing with the distance).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_marking_on_a_long_road_stays_seen_at_every_distance() {
    let instance = mx::Instance::new();
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(mx::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let quad = |x0: f32, x1: f32, y0: f32, y1: f32| MeshData {
        positions: vec![Vec3::new(x0, y0, 0.0), Vec3::new(x1, y0, 0.0), Vec3::new(x1, y1, 0.0), Vec3::new(x0, y1, 0.0)],
        normals: vec![Vec3::Z; 4], uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)], indices: vec![0, 1, 2, 0, 2, 3], one_sided: false,
    };
    // (a straight road's mesh has pieces of hundreds of metres: the triangles that reach from
    // the eye's feet to far away, where the pull of the road is not one plane any more if it
    // stops growing with the distance)
    let piece = 200.0f32;
    let road_mesh = {
        let n = (600.0 / piece) as usize;
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for k in 0..n {
            let (y0, y1) = (0.5 + k as f32 * piece, 0.5 + (k + 1) as f32 * piece);
            let b = positions.len() as u32;
            positions.extend([Vec3::new(-4.0, y0, 0.0), Vec3::new(4.0, y0, 0.0), Vec3::new(4.0, y1, 0.0), Vec3::new(-4.0, y1, 0.0)]);
            indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        let len = indices.len() as u32;
        renderer.add_mesh(&mut scene, &MeshData { normals: vec![Vec3::Z; positions.len()], uvs: vec![glam::Vec2::ZERO; positions.len()], positions, ranges: vec![(0, len, 0)], indices, one_sided: false })
    };
    let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
    let white = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 1.0, 1.0, 1.0], true);
    let road = renderer.add_surface_instance(&mut scene, road_mesh, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    scene.instances[road].render_phase = RenderPhase::Spline;
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let mut lost = Vec::new();
    for height in [1.2f32, 2.5, 3.5] {
        for d in [4.0f32, 6.0, 9.0, 12.0, 15.0, 19.0, 24.0, 30.0, 40.0, 55.0, 75.0, 100.0] {
            // (a quarter of its distance long: several pixels from a grazing eye)
            let mark_mesh = renderer.add_mesh(&mut scene, &quad(-0.4, 0.4, d * 0.875, d * 1.125));
            let mark = renderer.add_surface_instance(&mut scene, mark_mesh, DVec3::ZERO, Mat4::IDENTITY, vec![white]);
            scene.instances[mark].render_phase = RenderPhase::Spline;
            scene.instances[mark].decal = true;
            scene.dirty = true;
            let pitch = -(height / d).atan().to_degrees();
            let camera = Camera { position: DVec3::new(0.0, 0.0, height as f64), yaw: 0.0, pitch, roll: 0.0, fov_deg: 2.0, near: 0.1, far: 2000.0 };
            let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
            let pixel = &rgba[(32 * 64 + 32) * 4..][..3];
            if !(pixel[0] > 200 && pixel[2] > 200) {
                lost.push((height, d, pixel.to_vec()));
            }
            scene.instances[mark].visible = false;
        }
    }
    assert!(lost.is_empty(), "the marking was lost at (height, distance, pixel): {lost:?}");
}
