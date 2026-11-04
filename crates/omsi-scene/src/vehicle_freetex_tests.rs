use super::*;
use glam::{Vec2, Vec3};

struct Fixture(PathBuf);

impl Fixture {
    fn world() -> (Self, World) {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "openomsi-display-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("global.cfg"), "[name]\nDisplay cache test\n").unwrap();
        let world = World::open(&path, &path.join("global.cfg"), 20261101).unwrap();
        (Self(path), world)
    }

    fn picture(&self, name: &str, side: u32) -> PathBuf {
        let path = self.0.join(name);
        image::RgbaImage::from_pixel(side, side, image::Rgba([220, 50, 30, 255]))
            .save(&path)
            .unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let (Ok(path), Ok(root)) = (self.0.canonicalize(), std::env::temp_dir().canonicalize()) {
            if path.starts_with(&root) && path != root {
                std::fs::remove_dir_all(path).unwrap();
            }
        }
    }
}

fn renderer() -> Renderer {
    renderer_on(false)
}

fn renderer_on(gpu: bool) -> Renderer {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    if !gpu {
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    }
    let instance = wgpu::Instance::new(descriptor);
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        omsi_render::RenderOptions {
            msaa: if gpu { 4 } else { 1 },
            ssao: false,
            shadow_size: 1024,
            ..Default::default()
        },
    ))
    .unwrap()
}

fn slot(world: &World, renderer: &Renderer, scene: &mut Scene) -> VariantSlot {
    let material = renderer.add_material(scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let look = Look {
        alpha: AlphaMode::Opaque,
        color: [1.0; 4],
        emissive: [0.0; 3],
        unlit: true,
        lit: false,
        diffuse: None,
        transmap: None,
        night: None,
        lightmap: None,
        envmap: None,
        extra: Default::default(),
        dyn_tex: Default::default(),
    };
    let mut free = world.free_texture_slot("display".into(), None, true, 1, &[]);
    free.is_player = true;
    VariantSlot {
        mesh: 0,
        slot: 0,
        base: material,
        item: material,
        more: Vec::new(),
        var: String::new(),
        more_vars: Vec::new(),
        entries: Vec::new(),
        tex_var: String::new(),
        free: vec![free],
        free_built: HashMap::new(),
        free_lights: None,
        spec: SlotSpec {
            base: look,
            item: None,
            more: Vec::new(),
        },
        base_tex: None,
        entry_tex: Vec::new(),
        lights: None,
    }
}

fn show(
    slot: &mut VariantSlot,
    renderer: &Renderer,
    scene: &mut Scene,
    path: PathBuf,
) -> TextureId {
    let name = path.to_string_lossy().to_string();
    let f = &mut slot.free[0];
    let id = match f.pictures.get(&name).copied().flatten() {
        Some(id) => id,
        None => f.load_picture(renderer, scene, path).unwrap(),
    };
    f.pictures.insert(name.clone(), Some(id));
    f.current = Some(name.clone());
    f.image = Some(id);
    let material = renderer.add_material(scene, Some(id), AlphaMode::Opaque, [1.0; 4], true);
    slot.base = material;
    slot.item = material;
    slot.free_built.insert(
        name,
        FreeBuilt {
            base: Some(material),
            ..Default::default()
        },
    );
    slot.trim_free_textures(renderer, scene);
    id
}

#[test]
fn display_frames_are_released_and_slots_reused_over_repeated_sequences() {
    let (fixture, world) = Fixture::world();
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    let mut slot = slot(&world, &renderer, &mut scene);
    let paths: Vec<_> = (0..128)
        .map(|n| fixture.picture(&format!("frame{n}.png"), 32))
        .collect();
    let before = scene.textures.len();
    for _ in 0..4 {
        for path in &paths {
            let id = show(&mut slot, &renderer, &mut scene, path.clone());
            assert_eq!(scene.texture_size_of(id), Some((32, 32)));
            assert!(scene.materials[slot.base].uses_texture(id));
            assert!(slot.free[0].pictures.len() <= KEEP_PICTURES);
            assert!(world.vehicle_textures.lock().len() <= KEEP_PICTURES);
            assert!(world.player_textures.lock().len() <= KEEP_PICTURES);
            assert!(scene.textures.len() <= before + KEEP_PICTURES + 1);
        }
    }
}

#[test]
fn alternating_large_display_pictures_stay_cached_until_the_current_picture_shrinks() {
    let (fixture, world) = Fixture::world();
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    let mut slot = slot(&world, &renderer, &mut scene);
    let first_path = fixture.picture("first.png", 2048);
    let second_path = fixture.picture("second.png", 2048);
    let first = show(&mut slot, &renderer, &mut scene, first_path.clone());
    assert!(scene.texture_bytes_of(first) >= KEEP_BYTES);
    let second = show(&mut slot, &renderer, &mut scene, second_path.clone());
    assert_eq!(slot.free[0].pictures.len(), 2);
    assert_ne!(first, second);
    for _ in 0..4 {
        assert_eq!(
            show(&mut slot, &renderer, &mut scene, first_path.clone()),
            first
        );
        assert_eq!(
            show(&mut slot, &renderer, &mut scene, second_path.clone()),
            second
        );
        assert_eq!(world.vehicle_textures.lock().len(), 2);
    }
    let current = show(
        &mut slot,
        &renderer,
        &mut scene,
        fixture.picture("small.png", 32),
    );
    assert_eq!(slot.free[0].pictures.len(), 1);
    assert_eq!(scene.texture_bytes_of(first), 0);
    assert_eq!(scene.texture_bytes_of(second), 0);
    assert_eq!(scene.texture_size_of(current), Some((32, 32)));
}

#[test]
fn byte_limit_preserves_the_current_large_picture_and_other_owners() {
    let (fixture, world) = Fixture::world();
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    let mut slot = slot(&world, &renderer, &mut scene);
    let old_path = fixture.picture("shared.png", 2048);
    let old = show(&mut slot, &renderer, &mut scene, old_path.clone());
    assert_eq!(scene.texture_size_of(old), Some((2048, 2048)));
    let mut other = world.free_texture_slot("other".into(), None, true, 1, &[]);
    assert_eq!(
        other.load_picture(&renderer, &mut scene, old_path.clone()),
        Some(old)
    );
    let new = show(
        &mut slot,
        &renderer,
        &mut scene,
        fixture.picture("current.png", 2048),
    );
    let current = show(
        &mut slot,
        &renderer,
        &mut scene,
        fixture.picture("next.png", 2048),
    );
    assert_eq!(slot.free[0].pictures.len(), 1);
    assert_eq!(world.vehicle_textures.lock()[&old_path].1, 1);
    assert_eq!(scene.texture_size_of(old), Some((2048, 2048)));
    assert_eq!(scene.texture_bytes_of(new), 0);
    assert_eq!(scene.texture_size_of(current), Some((2048, 2048)));
    other.trim(&renderer, &mut scene, &[]);
    assert!(!world.vehicle_textures.lock().contains_key(&old_path));
    assert!(!world.player_textures.lock().contains(&old_path));
    assert_eq!(scene.texture_bytes_of(old), 0);
    assert!(world.freetex_reloads.snapshot(old).is_none());
}

#[test]
fn live_item_materials_and_aliases_survive_cache_eviction() {
    let (fixture, world) = Fixture::world();
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    let mut slot = slot(&world, &renderer, &mut scene);
    let path = fixture.picture("item.png", 32);
    let old = show(&mut slot, &renderer, &mut scene, path.clone());
    let alias = slot.free[0]
        .load_picture(&renderer, &mut scene, path.clone())
        .unwrap();
    slot.free[0].pictures.insert("alias".into(), Some(alias));
    slot.more.push(slot.base);
    for n in 0..64 {
        show(
            &mut slot,
            &renderer,
            &mut scene,
            fixture.picture(&format!("next{n}.png"), 32),
        );
    }
    assert_eq!(world.vehicle_textures.lock()[&path], (old, 2));
    assert_eq!(scene.texture_size_of(old), Some((32, 32)));
    slot.more.clear();
    for n in 64..100 {
        show(
            &mut slot,
            &renderer,
            &mut scene,
            fixture.picture(&format!("next{n}.png"), 32),
        );
    }
    assert!(!world.vehicle_textures.lock().contains_key(&path));
    assert!(!world.player_textures.lock().contains(&path));
}

#[test]
fn missing_and_remote_pictures_do_not_release_borrowed_textures() {
    let (_fixture, world) = Fixture::world();
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    let mut slot = slot(&world, &renderer, &mut scene);
    let data = TextureData::from_image(omsi_texture::Image {
        width: 4,
        height: 4,
        rgba: vec![255; 64],
        has_alpha: false,
    });
    let remote = renderer.add_texture_data(&mut scene, &data);
    slot.free[0].remote = Some(Some(remote));
    slot.free[0].image = Some(remote);
    slot.free[0].current = Some("remote".into());
    slot.free[0].pictures.insert("remote".into(), Some(remote));
    for n in 0..100 {
        slot.free[0].pictures.insert(format!("missing{n}"), None);
    }
    slot.trim_free_textures(&renderer, &mut scene);
    assert_eq!(slot.free[0].pictures.len(), 1);
    assert_eq!(scene.texture_size_of(remote), Some((4, 4)));
}

#[test]
#[ignore = "requires a graphics adapter; renders a synthetic passenger display sequence"]
fn display_playback_keeps_pixels_and_msaa_with_bounded_gpu_storage() {
    let (fixture, world) = Fixture::world();
    let mut renderer = renderer_on(true);
    let mut scene = renderer.new_scene();
    let mut slot = slot(&world, &renderer, &mut scene);
    let mesh = renderer.add_mesh(
        &mut scene,
        &omsi_geometry::MeshData {
            positions: vec![
                Vec3::new(-4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, 4.0),
                Vec3::new(-4.0, 4.0, 4.0),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    let instance = renderer.add_instance(
        &mut scene,
        mesh,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![slot.base],
    );
    let camera = omsi_render::Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let lighting = omsi_render::Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let mut peak = 0;
    for frame in 0..256 {
        let path = fixture.0.join(format!("frame{frame}.png"));
        let color = if frame % 2 == 0 {
            [220, 30, 20, 255]
        } else {
            [20, 30, 220, 255]
        };
        image::RgbaImage::from_pixel(720, 405, image::Rgba(color))
            .save(&path)
            .unwrap();
        let id = show(&mut slot, &renderer, &mut scene, path);
        renderer.set_material(&mut scene, instance, 0, slot.base);
        let rgba = renderer
            .render_to_image(&mut scene, 64, 64, &camera, &lighting)
            .unwrap();
        let pixel = &rgba[(32 * 64 + 32) * 4..][..3];
        let (bright, dark) = if frame % 2 == 0 { (0, 2) } else { (2, 0) };
        assert!(
            pixel[bright] > 180 && pixel[dark] < 80,
            "frame {frame}: {pixel:?}"
        );
        assert_eq!(scene.texture_size_of(id), Some((720, 405)));
        assert_eq!(renderer.options.msaa, 4);
        assert!(!renderer.take_out_of_memory());
        let bytes = renderer.texture_bytes(&scene);
        peak = peak.max(bytes);
        assert!(bytes <= KEEP_BYTES, "frame {frame}: {bytes} bytes retained");
    }
    println!("256 display frames: peak scene textures {peak} bytes; 4x MSAA retained");
}
