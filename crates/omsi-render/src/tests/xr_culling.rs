//! Exercise the XR render entry point without a headset. Deliberately disjoint
//! views make one eye overwriting the other's visibility or LOD history observable.

use super::*;

const SIZE: u32 = 64;
const DISTANCE: f64 = 10.9;

struct StereoScene {
    renderer: Renderer,
    scene: Scene,
    target: wgpu::TextureView,
    lighting: Lighting,
}

impl StereoScene {
    fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let renderer = pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa: 1,
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

    fn eye(&mut self, eye: usize, retreat: f64) {
        let camera = Camera {
            position: DVec3::new(if eye == 0 { retreat } else { -retreat }, 0.0, 0.0),
            yaw: if eye == 0 { -90.0 } else { 90.0 },
            ..camera()
        };
        let projection =
            Mat4::perspective_rh(camera.fov_deg.to_radians(), 1.0, camera.far, camera.near);
        self.renderer.render_xr_eye(
            &mut self.scene,
            &self.target,
            SIZE,
            SIZE,
            &camera,
            &self.lighting,
            projection,
            eye == 1,
        );
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

    fn pair(&mut self, retreat: f64) {
        let cameras = std::array::from_fn(|eye| Camera {
            position: DVec3::new(if eye == 0 { retreat } else { -retreat }, 0.0, 0.0),
            yaw: if eye == 0 { -90.0 } else { 90.0 },
            ..camera()
        });
        let projections =
            cameras.map(|c| Mat4::perspective_rh(c.fov_deg.to_radians(), 1.0, c.far, c.near));
        self.renderer.render_xr_stereo(
            &mut self.scene,
            [&self.target, &self.target],
            (SIZE, SIZE),
            &cameras,
            &self.lighting,
            &projections,
        );
    }
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
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_culling_keeps_each_eye_and_the_desktop_independent() {
    let mut s = StereoScene::new();
    s.desktop();
    let desktop_drawn = s.renderer.history.cull_drawn.borrow().clone();
    let desktop_sizes = s.renderer.history.object_sizes.borrow().clone();
    let desktop_scratch = s.renderer.history.object_sizes_scratch.borrow().clone();
    assert_eq!(desktop_drawn, vec![1 << 2]);
    assert_eq!(desktop_sizes.len(), 1);

    for order in [[0, 1], [1, 0], [0, 1]] {
        for eye in order {
            let other = &s.renderer.history.xr_culling[1 - eye];
            let other_drawn = other.drawn.clone();
            let other_sizes = other.sizes.clone();
            let other_scratch = other.scratch.clone();
            s.eye(eye, 0.0);
            let own = &s.renderer.history.xr_culling[eye];
            assert_eq!(own.drawn, vec![1 << eye]);
            assert_eq!(own.sizes.len(), 1);
            let other = &s.renderer.history.xr_culling[1 - eye];
            assert_eq!(other.drawn, other_drawn);
            assert_eq!(other.sizes, other_sizes);
            assert_eq!(other.scratch, other_scratch);
            assert_eq!(*s.renderer.history.cull_drawn.borrow(), desktop_drawn);
            assert_eq!(*s.renderer.history.object_sizes.borrow(), desktop_sizes);
            assert_eq!(
                *s.renderer.history.object_sizes_scratch.borrow(),
                desktop_scratch
            );
        }
        // Desktop rendering between headset frames must not change either eye.
        s.desktop();
        for eye in 0..2 {
            assert_eq!(s.renderer.history.xr_culling[eye].drawn, vec![1 << eye]);
            assert_eq!(s.renderer.history.xr_culling[eye].sizes.len(), 1);
        }
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_culling_retains_each_eyes_distance_hysteresis() {
    let mut s = StereoScene::new();
    s.renderer.options.max_obj_dist = 10.0;
    for eye in [0, 1] {
        s.eye(eye, 0.0);
        assert_eq!(s.renderer.history.xr_culling[eye].drawn, vec![1 << eye]);
    }
    // At 11.3 m, a radius-1 object is past the 11 m initial cutoff, but
    // remains inside the 11.5 m cutoff of an eye that drew it last frame.
    for order in [[1, 0], [0, 1]] {
        for eye in order {
            s.eye(eye, 0.4);
            assert_eq!(s.renderer.history.xr_culling[eye].drawn, vec![1 << eye]);
        }
    }
    // A genuinely new view at this distance must still reject the object.
    s.renderer.history.xr_culling = Default::default();
    for eye in [0, 1] {
        s.eye(eye, 0.4);
        assert_eq!(s.renderer.history.xr_culling[eye].drawn, vec![0]);
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_culling_retains_each_eyes_lod_size_hysteresis() {
    let mut s = StereoScene::new();
    let size = |distance: f64| 2.0 / (distance as f32 * 90f32.to_radians());
    let cutoff = (size(DISTANCE) + size(DISTANCE + 0.4)) * 0.5;
    for eye in [0, 1] {
        s.renderer
            .set_lod_range(&mut s.scene, eye, cutoff, f32::MAX);
        s.eye(eye, 0.0);
        assert_eq!(s.renderer.history.xr_culling[eye].drawn, vec![1 << eye]);
    }
    let original_sizes = [
        s.renderer.history.xr_culling[0].sizes.clone(),
        s.renderer.history.xr_culling[1].sizes.clone(),
    ];
    // A 3.5% size change stays within the existing 6% LOD deadband. The
    // other eye's disjoint view must not discard the saved object size.
    for order in [[1, 0], [0, 1]] {
        for eye in order {
            s.eye(eye, 0.4);
            assert_eq!(s.renderer.history.xr_culling[eye].drawn, vec![1 << eye]);
            assert_eq!(
                s.renderer.history.xr_culling[eye].sizes,
                original_sizes[eye]
            );
        }
    }
    s.renderer.history.xr_culling = Default::default();
    for eye in [0, 1] {
        s.eye(eye, 0.4);
        assert_eq!(s.renderer.history.xr_culling[eye].drawn, vec![0]);
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_stereo_matches_independent_eyes_after_scene_updates() {
    let mut independent = StereoScene::new();
    let mut stereo = StereoScene::new();
    for s in [&mut independent, &mut stereo] {
        s.desktop();
        s.renderer.options.max_obj_dist = 10.0;
    }
    let desktop = stereo.renderer.history.cull_drawn.borrow().clone();
    let desktop_bundles = stereo.renderer.history.bundle_stats.get();
    for retreat in [0.0, 0.4, 0.4] {
        independent.eye(0, retreat);
        independent.eye(1, retreat);
        stereo.pair(retreat);
        for eye in 0..2 {
            let a = &independent.renderer.history.xr_culling[eye];
            let b = &stereo.renderer.history.xr_culling[eye];
            assert_eq!(a.drawn, b.drawn);
            assert_eq!(a.sizes, b.sizes);
            assert!(
                b.bundles.is_some(),
                "eye {eye} keeps its own recorded batches"
            );
        }
        assert_eq!(*stereo.renderer.history.cull_drawn.borrow(), desktop);
        assert_eq!(stereo.renderer.history.bundle_stats.get(), desktop_bundles);
    }
    for s in [&mut independent, &mut stereo] {
        // Previously rejected by the broad phase: changes must be visible next
        // frame, including a transform scale and moving the render origin.
        s.renderer.set_transform(
            &mut s.scene,
            2,
            DVec3::new(-5.0, 0.0, 0.0),
            Mat4::from_scale(Vec3::splat(2.0)),
        );
        s.renderer
            .set_render_origin(&mut s.scene, DVec3::new(100.0, 0.0, 0.0));
    }
    independent.eye(0, 0.0);
    independent.eye(1, 0.0);
    stereo.pair(0.0);
    for eye in 0..2 {
        assert_eq!(
            independent.renderer.history.xr_culling[eye].drawn,
            stereo.renderer.history.xr_culling[eye].drawn
        );
    }
    assert!(stereo.renderer.history.xr_culling[0].drawn[0] & (1 << 2) != 0);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_stereo_pixels_match_separate_eye_rendering() {
    let mut s = StereoScene::new();
    let readback_target = |renderer: &Renderer| {
        renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stereo pixel regression"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let reference = readback_target(&s.renderer);
    s.target = reference.create_view(&Default::default());
    let green = s.renderer.add_material(
        &mut s.scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    s.scene.instances[1].materials[0] = green;
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
    let mut expected = Vec::new();
    for eye in 0..2 {
        s.eye(eye, 0.0);
        expected.push(
            s.renderer
                .read_texture(&reference, wgpu::TextureAspect::All)
                .unwrap(),
        );
    }
    assert_ne!(
        expected[0], expected[1],
        "the views must have distinguishable pixels"
    );
    let textures = [readback_target(&s.renderer), readback_target(&s.renderer)];
    let targets = textures
        .each_ref()
        .map(|t| t.create_view(&Default::default()));
    s.renderer.render_xr_stereo(
        &mut s.scene,
        [&targets[0], &targets[1]],
        (SIZE, SIZE),
        &cameras,
        &s.lighting,
        &projections,
    );
    for eye in 0..2 {
        let pixels = s
            .renderer
            .read_texture(&textures[eye], wgpu::TextureAspect::All)
            .unwrap();
        assert_eq!(
            pixels, expected[eye],
            "eye {eye} keeps its own projection and draw order"
        );
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_subpixel_ai_meshes_leave_other_eye_and_player_detail_intact() {
    let mut s = StereoScene::new();
    let lit = s.renderer.add_material(
        &mut s.scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        false,
    );
    s.scene.instances[0].materials[0] = lit;
    s.scene.instances[0].min_mesh_pixels = 5.0;
    // The other eye's small unlit lamp must remain visible for its glow.
    s.scene.instances[1].min_mesh_pixels = 5.0;
    s.pair(0.0);
    assert_eq!(s.renderer.history.xr_culling[0].drawn, vec![0]);
    assert_eq!(s.renderer.history.xr_culling[1].drawn, vec![1 << 1]);
    // No cutoff on the player's/ordinary instances; desktop ignores it too.
    s.scene.instances[0].min_mesh_pixels = 0.0;
    s.pair(0.0);
    assert_eq!(s.renderer.history.xr_culling[0].drawn, vec![1]);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_stereo_broad_phase_keeps_asymmetric_eye_only_candidates() {
    let mut s = StereoScene::new();
    for (i, position) in [
        DVec3::new(-15.0, 10.0, 0.0),
        DVec3::new(15.0, 10.0, 0.0),
        DVec3::new(0.0, 10.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        s.renderer
            .set_transform(&mut s.scene, i, position, Mat4::IDENTITY);
    }
    let cameras = [camera(); 2];
    let mut wide = Mat4::perspective_rh(90f32.to_radians(), 1.0, 100.0, 0.1);
    wide.z_axis.x = 1.0;
    let projections = [
        wide,
        Mat4::perspective_rh(30f32.to_radians(), 1.0, 100.0, 0.1),
    ];
    let mut expected = Vec::new();
    for eye in 0..2 {
        s.renderer.render_xr_eye(
            &mut s.scene,
            &s.target,
            SIZE,
            SIZE,
            &cameras[eye],
            &s.lighting,
            projections[eye],
            eye == 1,
        );
        expected.push(s.renderer.history.xr_culling[eye].drawn.clone());
    }
    assert_ne!(expected[0], expected[1]);
    s.renderer.history.xr_culling = Default::default();
    s.renderer.render_xr_stereo(
        &mut s.scene,
        [&s.target, &s.target],
        (SIZE, SIZE),
        &cameras,
        &s.lighting,
        &projections,
    );
    for eye in 0..2 {
        assert_eq!(s.renderer.history.xr_culling[eye].drawn, expected[eye]);
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn xr_authored_vehicle_lod_levels_are_selected_per_eye() {
    let mut s = StereoScene::new();
    for (i, (lo, hi)) in [(0.15, f32::MAX), (0.05, 0.15), (0.0, 0.05)]
        .into_iter()
        .enumerate()
    {
        s.renderer
            .set_transform(&mut s.scene, i, DVec3::new(0.0, 5.0, 0.0), Mat4::IDENTITY);
        s.renderer
            .set_object_culling(&mut s.scene, i, 1.0, 1.0, false);
        s.renderer.set_lod_range(&mut s.scene, i, lo, hi);
    }
    let mut cameras = [
        camera(),
        Camera {
            position: DVec3::new(0.0, -40.0, 0.0),
            ..camera()
        },
    ];
    let projections =
        cameras.map(|c| Mat4::perspective_rh(c.fov_deg.to_radians(), 1.0, c.far, c.near));
    s.renderer.render_xr_stereo(
        &mut s.scene,
        [&s.target, &s.target],
        (SIZE, SIZE),
        &cameras,
        &s.lighting,
        &projections,
    );
    assert_eq!(s.renderer.history.xr_culling[0].drawn, vec![1]);
    assert_eq!(s.renderer.history.xr_culling[1].drawn, vec![4]);
    cameras[1].position.y = -10.0;
    s.renderer.render_xr_stereo(
        &mut s.scene,
        [&s.target, &s.target],
        (SIZE, SIZE),
        &cameras,
        &s.lighting,
        &projections,
    );
    assert_eq!(s.renderer.history.xr_culling[0].drawn, vec![1]);
    assert_eq!(s.renderer.history.xr_culling[1].drawn, vec![2]);
}
