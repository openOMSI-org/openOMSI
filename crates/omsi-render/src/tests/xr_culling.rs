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
