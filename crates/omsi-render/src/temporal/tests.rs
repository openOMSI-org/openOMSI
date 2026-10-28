use super::*;

#[test]
fn motion_and_debug_shaders_validate_without_a_gpu() {
    for source in [motion_source(), include_str!("debug.wgsl").to_string()] {
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    }
}

struct Fixture {
    renderer: Renderer,
    scene: Scene,
    target: wgpu::Texture,
    camera: Camera,
    object: usize,
}

impl Fixture {
    fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let renderer = pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                temporal_debug: TemporalDebug::Motion,
                msaa: 1,
                ssao: false,
                fxaa: false,
                shadow_size: 512,
                no_enhanced: true,
                preview_only: true,
                ..Default::default()
            },
        ));
        let mut renderer = renderer.expect("graphics adapter for temporal test");
        eprintln!("temporal GPU adapter: {:?}", renderer.device.adapter_info());
        renderer.mesh_pages = true;
        assert!(matches!(
            renderer.device.adapter_info().backend,
            wgpu::Backend::Vulkan | wgpu::Backend::Dx12 | wgpu::Backend::Metal
        ));
        let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("temporal test colour"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut scene = renderer.new_scene();
        let mesh = renderer.add_mesh(
            &mut scene,
            &MeshData {
                positions: vec![
                    Vec3::new(-2.0, 5.0, -2.0),
                    Vec3::new(2.0, 5.0, -2.0),
                    Vec3::new(2.0, 5.0, 2.0),
                    Vec3::new(-2.0, 5.0, 2.0),
                ],
                normals: vec![-Vec3::Y; 4],
                uvs: vec![glam::Vec2::ZERO; 4],
                indices: vec![0, 1, 2, 0, 2, 3],
                ranges: vec![(0, 6, 0)],
                one_sided: false,
            },
        );
        let material = renderer.add_material(
            &mut scene,
            None,
            AlphaMode::Opaque,
            [0.3, 0.5, 0.2, 1.0],
            true,
        );
        let object = renderer.add_instance(
            &mut scene,
            mesh,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![material],
        );
        let camera = Camera {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 1000.0,
        };
        Self {
            renderer,
            scene,
            target,
            camera,
            object,
        }
    }

    fn frame(&mut self) {
        let target = self.target.create_view(&Default::default());
        self.renderer.render(
            &mut self.scene,
            &target,
            self.target.width(),
            self.target.height(),
            &self.camera,
            &Lighting {
                shadows: false,
                fog_density: 0.0,
                ..Default::default()
            },
        );
    }

    fn centre(&self) -> ([f32; 2], u8) {
        let resources = self.renderer.temporal_buffers().expect("temporal targets");
        let motion = self
            .renderer
            .read_texture(resources.motion, wgpu::TextureAspect::All)
            .unwrap();
        let valid = self
            .renderer
            .read_texture(resources.validity, wgpu::TextureAspect::All)
            .unwrap();
        let index = (resources.size.1 / 2 * resources.size.0 + resources.size.0 / 2) as usize;
        let half = |at| {
            let h = u16::from_le_bytes([motion[at], motion[at + 1]]);
            let sign = if h & 0x8000 == 0 { 1.0 } else { -1.0 };
            let exp = ((h >> 10) & 31) as i32;
            let fraction = (h & 1023) as f32 / 1024.0;
            if exp == 0 {
                sign * fraction * 2.0f32.powi(-14)
            } else {
                sign * (1.0 + fraction) * 2.0f32.powi(exp - 15)
            }
        };
        ([half(index * 4), half(index * 4 + 2)], valid[index])
    }
}

#[test]
#[ignore = "requires a graphics adapter; run explicitly on a GPU host"]
fn temporal_gpu_static_moving_cut_and_disable() {
    let mut f = Fixture::new();
    f.frame();
    assert_eq!(f.centre().1, 0, "first frame has no history");
    f.frame();
    let (v, valid) = f.centre();
    assert_eq!(valid, 255);
    assert!(
        v[0].abs() < 0.01 && v[1].abs() < 0.01,
        "static motion {v:?}"
    );
    f.renderer.set_transform(
        &mut f.scene,
        f.object,
        DVec3::new(0.5, 0.0, 0.0),
        Mat4::IDENTITY,
    );
    f.frame();
    let (v, valid) = f.centre();
    assert_eq!(valid, 255);
    assert!(
        (v[0] + 5.54256).abs() < 0.02 && v[1].abs() < 0.01,
        "moving object motion {v:?}"
    );
    let mirror_camera = Camera {
        yaw: 90.0,
        ..f.camera
    };
    let view = f.target.create_view(&Default::default());
    f.renderer.render_inner(
        &mut f.scene,
        &view,
        64,
        64,
        &mirror_camera,
        &Lighting {
            shadows: false,
            ..Default::default()
        },
        false,
        None,
        None,
        false,
    );
    f.frame();
    assert_eq!(
        f.centre().1,
        255,
        "a mirror must not reset main-view history"
    );
    assert!(
        f.centre().0.iter().all(|v| v.abs() < 0.01),
        "a mirror must not replace the previous main camera"
    );
    f.renderer.reset_temporal_history(&mut f.scene);
    f.frame();
    assert_eq!(f.centre().1, 0);
    f.frame();
    assert_eq!(f.centre().1, 255);
    f.camera.fov_deg = 70.0;
    f.frame();
    assert_eq!(f.centre().1, 0, "projection change resets history");
    let red = f.renderer.add_texture(
        &mut f.scene,
        &omsi_texture::Image {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
            has_alpha: false,
        },
        false,
    );
    f.scene.overlays.push((red, [0.0, 0.0, 12.0, 12.0]));
    f.frame();
    let picture = f
        .renderer
        .read_texture(&f.target, wgpu::TextureAspect::All)
        .unwrap();
    assert_eq!(
        &picture[(6 * 64 + 6) * 4..][..4],
        &[255, 0, 0, 255],
        "HUD is recomposited over diagnostics"
    );
    f.renderer.options.temporal_debug = TemporalDebug::Off;
    f.frame();
    assert!(f.renderer.temporal_buffers().is_none() && f.renderer.temporal.is_none());
}

#[test]
#[ignore = "requires a graphics adapter; run explicitly on a GPU host"]
fn temporal_gpu_rebase_recycle_mesh_pose_and_resize() {
    let mut f = Fixture::new();
    f.frame();
    f.frame();
    f.renderer
        .set_render_origin(&mut f.scene, DVec3::new(100.0, 0.0, 0.0));
    f.frame();
    let (v, valid) = f.centre();
    assert_eq!(valid, 255);
    assert!(
        v[0].abs() < 0.01 && v[1].abs() < 0.01,
        "floating origin {v:?}"
    );
    // A changed skinned pose is not mislabeled as stationary; old vertices are needed
    // before this path can feed NVIDIA NR.
    f.scene.meshes[f.scene.instances[f.object].mesh].pose += 1;
    f.frame();
    assert_eq!(f.centre().1, 0);
    f.frame();
    assert_eq!(f.centre().1, 255);
    let mesh = f.scene.instances[f.object].mesh;
    let materials = f.scene.instances[f.object].materials.clone();
    let new = f
        .renderer
        .add_instance(&mut f.scene, mesh, DVec3::ZERO, Mat4::IDENTITY, materials);
    f.object = f.renderer.recycle_instance(&mut f.scene, new, f.object);
    f.frame();
    assert_eq!(f.centre().1, 0, "recycled slot is a different object");
    f.target = f.renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("resized temporal test"),
        size: wgpu::Extent3d {
            width: 96,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    f.frame();
    assert_eq!(f.renderer.temporal_buffers().unwrap().size, (96, 64));
    assert_eq!(f.centre().1, 0, "resize resets history");
}

#[test]
#[ignore = "requires a graphics adapter; run explicitly on a GPU host"]
fn temporal_gpu_cpu_skinned_vertices_reproject() {
    let mut f = Fixture::new();
    let local = [
        Vec3::new(-2.0, 5.0, -2.0),
        Vec3::new(2.0, 5.0, -2.0),
        Vec3::new(2.0, 5.0, 2.0),
        Vec3::new(-2.0, 5.0, 2.0),
    ];
    let normals = [-Vec3::Y; 4];
    let uvs = [glam::Vec2::ZERO; 4];
    let mesh = f.renderer.add_mesh(
        &mut f.scene,
        &MeshData {
            positions: local.to_vec(),
            normals: normals.to_vec(),
            uvs: uvs.to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    assert!(
        f.scene.meshes[mesh].base_vertex > 0,
        "exercise page-relative previous-vertex indexing"
    );
    f.renderer.set_instance_mesh(&mut f.scene, f.object, mesh);
    f.renderer
        .update_mesh(&mut f.scene, mesh, &local, &normals, &uvs);
    f.frame();
    let changed = local.map(|p| p + Vec3::new(0.25, 0.0, 0.0));
    f.renderer
        .update_mesh(&mut f.scene, mesh, &changed, &normals, &uvs);
    f.frame();
    let (motion, valid) = f.centre();
    assert_eq!(valid, 255, "the old CPU-skinned pose was captured");
    assert!(
        (motion[0] + 2.77128).abs() < 0.02 && motion[1].abs() < 0.01,
        "skinned vertex motion {motion:?}"
    );
}
