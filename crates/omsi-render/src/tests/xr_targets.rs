//! Compare headset attachment layouts on a real GPU without a headset.

use super::*;

const SIZE: u32 = 64;
const DISTANCE: f64 = 10.9;

struct EyeTargetsScene {
    renderer: Renderer,
    scene: Scene,
    target: wgpu::TextureView,
    lighting: Lighting,
}

impl EyeTargetsScene {
    fn with_options(msaa: u32, ray_tracing: bool) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let renderer = pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa,
                ray_tracing,
                ssao: false,
                shadow_size: 1024,
                fxaa: false,
                render_scale: 1.0,
                min_obj_size: 0.0,
                ..Default::default()
            },
        ))
        .expect("test renderer");
        let mut scene = renderer.new_scene();
        let mesh = renderer.add_mesh(
            &mut scene,
            &MeshData {
                positions: vec![
                    Vec3::new(0.0, -0.5, -0.5),
                    Vec3::new(0.0, 0.5, -0.5),
                    Vec3::new(0.0, 0.5, 0.5),
                    Vec3::new(0.0, -0.5, 0.5),
                ],
                normals: vec![Vec3::X; 4],
                uvs: vec![glam::Vec2::ZERO; 4],
                ranges: vec![(0, 6, 0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                one_sided: false,
            },
        );
        let material = renderer.add_material(
            &mut scene,
            None,
            AlphaMode::Opaque,
            [1.0, 0.0, 0.0, 1.0],
            true,
        );
        // One object per eye, and another for the desktop's forward view.
        for position in [
            DVec3::new(-DISTANCE, 0.0, 0.0),
            DVec3::new(DISTANCE, 0.0, 0.0),
            DVec3::new(0.0, DISTANCE, 0.0),
        ] {
            let id =
                renderer.add_instance(&mut scene, mesh, position, Mat4::IDENTITY, vec![material]);
            renderer.set_object_culling(&mut scene, id, 1.0, 1.0, false);
        }
        let target = renderer.add_render_texture(&mut scene, SIZE, SIZE);
        let target = scene.textures[target].view.clone();
        let lighting = Lighting {
            shadows: false,
            fog_density: 0.0,
            ..Default::default()
        };
        Self {
            renderer,
            scene,
            target,
            lighting,
        }
    }

    fn desktop(&mut self) {
        self.renderer.render(
            &mut self.scene,
            &self.target,
            SIZE,
            SIZE,
            &camera(),
            &self.lighting,
        );
    }
}

/// Exercise both attachment layouts on the same ray-capable renderer. Switching
/// back through a desktop-sized target must upgrade its surfaces and invalidate
/// render targets when their attachment layout changes, without changing eye pixels.
#[test]
#[ignore = "requires a ray-query graphics adapter; run with --ignored on a GPU host"]
fn xr_enhanced_plus_omits_unused_ray_surfaces_and_preserves_pixels() {
    for msaa in [1, 4] {
        let mut s = EyeTargetsScene::with_options(msaa, true);
        assert!(
            s.renderer.options.ray_tracing,
            "this regression requires native ray queries"
        );
        assert!(s.renderer.hdr_eye_pass.is_none());
        assert!(s.renderer.prepass_eye_pipelines.is_none());
        s.desktop();
        assert!(
            s.renderer.hdr_eye_pass.is_none(),
            "desktop must not compile headset pipelines"
        );
        s.renderer.prepare_xr_eye_pipelines();
        assert!(s.renderer.hdr_eye_pass.is_some());
        assert!(s.renderer.prepass_eye_pipelines.is_some());
        assert!(s.renderer.eye_pipeline_kit.is_none());
        s.renderer.prepare_xr_eye_pipelines(); // VR re-entry reuses the first set.
        s.lighting.enhanced = true;
        s.renderer.instant_exposure = true;
        let green = s.renderer.add_material(
            &mut s.scene,
            None,
            AlphaMode::Opaque,
            [0.0, 1.0, 0.0, 1.0],
            true,
        );
        s.scene.instances[1].materials[0] = green;
        // Include translucent material/ordering, not only opaque quads.
        let glass = s.renderer.add_material_extra(
            &mut s.scene,
            None,
            AlphaMode::Blend,
            [0.3, 0.5, 0.7, 0.2],
            false,
            None,
            None,
            None,
            None,
            [0.0; 3],
            MaterialExtra {
                glass: true,
                no_z_write: true,
                ..Default::default()
            },
        );
        let mesh = s.scene.instances[0].mesh;
        s.renderer.add_instance(
            &mut s.scene,
            mesh,
            DVec3::new(-8.0, 0.0, 0.0),
            Mat4::IDENTITY,
            vec![glass],
        );
        let textures = std::array::from_fn::<_, 2, _>(|_| {
            s.renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("enhanced XR attachment regression"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: s.renderer.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        let targets = textures
            .each_ref()
            .map(|t| t.create_view(&Default::default()));
        let cameras = [
            Camera {
                yaw: -90.0,
                ..camera()
            },
            Camera {
                yaw: 90.0,
                ..camera()
            },
        ];
        let projections =
            cameras.map(|c| Mat4::perspective_rh(c.fov_deg.to_radians(), 1.0, c.far, c.near));
        let draw = |s: &mut EyeTargetsScene| {
            for eye in 0..2 {
                s.renderer.render_xr_eye(
                    &mut s.scene,
                    &targets[eye],
                    SIZE,
                    SIZE,
                    &cameras[eye],
                    &s.lighting,
                    projections[eye],
                    eye == 1,
                );
            }
            textures.each_ref().map(|t| {
                s.renderer
                    .read_texture(t, wgpu::TextureAspect::All)
                    .unwrap()
            })
        };
        for _ in 0..3 {
            draw(&mut s);
        }
        assert!(s.renderer.targets.hdr_targets[&(SIZE, SIZE)].gbuf.is_none());
        let expected = draw(&mut s);
        assert_ne!(expected[0], expected[1]);
        // The same-sized desktop view still has its original Enhanced+ targets.
        s.desktop();
        assert!(s.renderer.targets.hdr_targets[&(SIZE, SIZE)].gbuf.is_some());
        let optimized = s.renderer.hdr_eye_pass.take();
        for _ in 0..3 {
            draw(&mut s);
        }
        let legacy = draw(&mut s);
        for eye in 0..2 {
            let error = expected[eye]
                .iter()
                .zip(&legacy[eye])
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                error <= 2,
                "MSAA {msaa}, eye {eye}: colour/mask mismatch {error}"
            );
        }
        s.renderer.hdr_eye_pass = optimized;
        let restored = draw(&mut s);
        for eye in 0..2 {
            let error = legacy[eye]
                .iter()
                .zip(&restored[eye])
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                error <= 2,
                "MSAA {msaa}, eye {eye}: restored colour pipeline mismatch {error}"
            );
        }
        // Force the rain-film pass through the colour-only eye layout after a
        // desktop view has upgraded the same cached target to four attachments.
        // XR normally has no overlays, so exercise this mode combination directly.
        let rain = s.renderer.add_material_extra(
            &mut s.scene,
            None,
            AlphaMode::Blend,
            [0.2, 0.3, 0.4, 0.2],
            false,
            None,
            None,
            None,
            None,
            [0.0; 3],
            MaterialExtra {
                rain_film: true,
                glass: true,
                no_z_write: true,
                ..Default::default()
            },
        );
        for x in [-8.0, 8.0] {
            s.renderer.add_instance(
                &mut s.scene,
                mesh,
                DVec3::new(x, 0.0, 0.0),
                Mat4::IDENTITY,
                vec![rain],
            );
        }
        s.lighting.rain = 1.0;
        s.desktop();
        for eye in 0..2 {
            check_rain_eye(
                &mut s,
                &targets[eye],
                &cameras[eye],
                projections[eye],
                eye == 1,
            );
        }
        assert!(s.renderer.device_lost().is_none());
    }
}

fn check_rain_eye(
    s: &mut EyeTargetsScene,
    target: &wgpu::TextureView,
    camera: &Camera,
    projection: Mat4,
    second_eye: bool,
) {
    use crate::passes::{FrameArgs, FrameEnv, StageClock};
    let error_scope = s
        .renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let mut clock = StageClock::new(false);
    let args = FrameArgs {
        target,
        width: SIZE,
        height: SIZE,
        camera,
        lighting: &s.lighting,
        with_overlays: false,
        exclude_texture: None,
        projection: Some(projection),
        second_eye,
    };
    let mut frame = s
        .renderer
        .begin_frame(&mut s.scene, FrameEnv::read(), args, &mut clock);
    assert!(frame.eye_color);
    assert!(s.renderer.targets.hdr_targets[&(SIZE, SIZE)].gbuf.is_some());
    frame.glass_on = true;
    let shadows = s.renderer.plan_shadow_cache(&s.scene, &frame);
    s.renderer
        .frame_uniforms(&s.scene, &frame, &shadows, &mut clock);
    let visible = s.renderer.cull_view(&s.scene, &frame, &mut clock);
    let mut plan = s
        .renderer
        .batch_draws(&s.scene, &frame, &visible, Vec::new());
    plan.split_rain(&s.scene, true);
    assert!(
        !plan.rain_batches.is_empty(),
        "regression must draw a rain-film batch"
    );
    s.renderer.upload_draw_list(&mut s.scene, &plan.list);
    let mut encoder = s
        .renderer
        .device
        .create_command_encoder(&Default::default());
    s.renderer
        .encode_rain_film(&mut encoder, &s.scene, &frame, &plan.rain_batches, false);
    s.renderer.queue.submit([encoder.finish()]);
    s.renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    let error = pollster::block_on(error_scope.pop());
    assert!(
        error.is_none(),
        "rain-film eye attachment validation: {error:?}"
    );
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_eye_pipeline_preparation_without_enhanced_plus_is_a_noop() {
    let mut s = EyeTargetsScene::with_options(1, false);
    assert!(s.renderer.eye_pipeline_kit.is_none());
    s.renderer.prepare_xr_eye_pipelines();
    assert!(s.renderer.hdr_eye_pass.is_none());
    assert!(s.renderer.prepass_eye_pipelines.is_none());
}

fn camera() -> Camera {
    Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    }
}

#[test]
#[ignore = "requires a ray-query graphics adapter; run with --ignored on a GPU host"]
fn xr_enhanced_plus_targets_work_with_upstream_fsr1() {
    for msaa in [1, 4] {
        let mut s = EyeTargetsScene::with_options(msaa, true);
        assert!(s.renderer.set_upscaler(true, 0.5, 1.0));
        assert_eq!(s.renderer.start_xr(0.75), 1.0);
        let scope = s
            .renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        s.renderer.prepare_xr_eye_pipelines();
        s.lighting.enhanced = true;
        let draw = |s: &mut EyeTargetsScene| {
            for (eye, yaw) in [-90.0, 90.0].into_iter().enumerate() {
                let camera = Camera { yaw, ..camera() };
                let projection =
                    Mat4::perspective_rh(camera.fov_deg.to_radians(), 1.0, camera.far, camera.near);
                s.renderer.render_xr_eye(
                    &mut s.scene,
                    &s.target,
                    SIZE,
                    SIZE,
                    &camera,
                    &s.lighting,
                    projection,
                    eye == 1,
                );
            }
        };
        draw(&mut s);
        let scaled = SIZE * 3 / 4;
        assert!(
            s.renderer.targets.hdr_targets[&(scaled, scaled)]
                .gbuf
                .is_none()
        );
        // A desktop render of the same internal size upgrades the shared cache;
        // both FSR eye submissions must still use the colour-only layout.
        s.renderer.render(
            &mut s.scene,
            &s.target,
            scaled,
            scaled,
            &camera(),
            &s.lighting,
        );
        assert!(
            s.renderer.targets.hdr_targets[&(scaled, scaled)]
                .gbuf
                .is_some()
        );
        draw(&mut s);
        s.renderer
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        assert!(pollster::block_on(scope.pop()).is_none());
        assert!(s.renderer.device_lost().is_none());
    }
}
