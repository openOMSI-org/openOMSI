//! Exercise real query/readback ownership with the production per-view entry points.
use super::*;

#[test]
#[ignore = "requires a graphics adapter and OMSI_GPU_TIMERS=1; run with --ignored"]
fn xr_diagnostics_keep_views_separate_and_collect_each_readback_once() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut r = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            ray_tracing: false,
            shadow_size: 1024,
            ..Default::default()
        },
    ))
    .expect("test renderer");
    assert!(
        r.gpu_timers.iter().all(Option::is_some),
        "enable OMSI_GPU_TIMERS on a timestamp-capable adapter"
    );
    r.profiling = true;
    let mut scene = r.new_scene();
    let mesh = r.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                Vec3::new(-1.0, 5.0, -1.0),
                Vec3::new(1.0, 5.0, -1.0),
                Vec3::new(0.0, 5.0, 1.0),
            ],
            normals: vec![-Vec3::Y; 3],
            uvs: vec![glam::Vec2::ZERO; 3],
            indices: vec![0, 1, 2],
            ranges: vec![(0, 3, 0)],
            one_sided: false,
        },
    );
    let material = r.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    r.add_instance(
        &mut scene,
        mesh,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![material],
    );
    let texture = r.add_render_texture(&mut scene, 64, 64);
    let target = scene.textures[texture].view.clone();
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 1000.0,
    };
    let projection = Mat4::perspective_rh(90f32.to_radians(), 1.0, camera.far, camera.near);
    let lighting = Lighting {
        enhanced: false,
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    r.render_to_texture(&mut scene, texture, &camera, &lighting, 1.0);
    r.render(&mut scene, &target, 64, 64, &camera, &lighting);
    for eye in 0..2 {
        r.render_xr_eye(
            &mut scene,
            &target,
            64,
            64,
            &camera,
            &lighting,
            projection,
            eye == 1,
        );
    }
    assert_eq!(r.pictures.get(), [1, 1, 1, 1]);
    for _ in 0..3 {
        r.collect_gpu_timers();
        r.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
    }
    r.collect_gpu_timers();
    for (i, timer) in r.gpu_timers.iter().enumerate() {
        let timer = timer.as_ref().unwrap();
        assert!(
            !timer.waiting && !timer.unresolved,
            "view {i} readback must finish"
        );
        assert_eq!(timer.totals.get("(all passes)").map(|v| v.1), Some(1));
        assert!(
            !timer.intervals.is_empty(),
            "view {i} has its own begin/end intervals"
        );
    }
    let summary = r.gpu_pass_times();
    for prefix in ["mirrors: ", "left eye: ", "right eye: "] {
        assert!(
            summary
                .iter()
                .any(|(label, _, _)| label.starts_with(prefix))
        );
    }
    assert_eq!(r.counts.borrow().get("left eye pictures"), Some(&1.0));
    assert_eq!(r.counts.borrow().get("right eye pictures"), Some(&1.0));
    assert_eq!(r.counts.borrow().get("mirror pictures"), Some(&1.0));
    r.collect_gpu_timers();
    assert_eq!(
        r.gpu_pass_times(),
        summary,
        "completed samples must not be counted twice"
    );
    r.render_xr_eye(
        &mut scene, &target, 64, 64, &camera, &lighting, projection, false,
    );
    assert_eq!(r.pictures.get(), [1, 1, 2, 1]);
    for _ in 0..3 {
        r.collect_gpu_timers();
        r.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
    }
    r.collect_gpu_timers();
    assert_eq!(
        r.gpu_timers[2].as_ref().unwrap().totals["(all passes)"].1,
        2
    );
    assert_eq!(
        r.gpu_timers[3].as_ref().unwrap().totals["(all passes)"].1,
        1
    );
}
