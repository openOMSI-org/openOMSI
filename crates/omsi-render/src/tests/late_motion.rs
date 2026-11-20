use super::*;

/// The temporal upscaler these tests ask for: Apple's on a Mac, which has no other, AMD's elsewhere.
const UPSCALER: TemporalUpscaler = if cfg!(target_os = "macos") {
    TemporalUpscaler::MetalFx
} else {
    TemporalUpscaler::Fsr3
};

/// A cab's parts drawn after its first pane that writes depth lay their motion in the late
/// pass (`DrawPlan::masked`), up to `BEHIND_GLASS` behind what the main pass left only under
/// glass: a dial 3 cm behind its gauge's glass gives the gauge its motion, and a part hidden
/// 3 cm behind an opaque panel leaves the panel's motion alone.
#[test]
#[ignore = "requires a graphics adapter and AMD's DLLs beside the test binary; run with --ignored on a GPU host"]
fn late_motion_lays_through_glass_only() {
    let (w, h) = (256, 128);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let temporal = Some(Temporal {
        upscaler: UPSCALER,
        quality: TemporalQuality::Native,
    });
    let options = RenderOptions {
        msaa: 1,
        ssao: false,
        shadow_size: 512,
        fxaa: false,
        render_scale: 1.0,
        temporal,
        ..Default::default()
    };
    let mut r = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        options,
    ))
    .expect("test renderer");
    let mut scene = r.new_scene();
    // (upright quads facing the camera, `y` metres ahead of it)
    let quad = |x0: f32, x1: f32, y: f32| MeshData {
        positions: vec![
            Vec3::new(x0, y, 0.6),
            Vec3::new(x1, y, 0.6),
            Vec3::new(x1, y, 2.4),
            Vec3::new(x0, y, 2.4),
        ],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        ranges: vec![(0, 6, 0)],
        ..Default::default()
    };
    let grey = r.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.5, 0.5, 0.5, 1.0],
        true,
    );
    let glass = r.add_material(
        &mut scene,
        None,
        AlphaMode::Blend,
        [0.8, 0.9, 1.0, 0.3],
        true,
    );
    // one model in its own order: a window first, then the gauge (its dial, then its glass),
    // then the panel and the part hidden behind it
    let parts = [
        (quad(-1.6, -1.4, 3.0), glass),
        (quad(-1.2, -0.1, 3.03), grey),
        (quad(-1.2, -0.1, 3.0), glass),
        (quad(0.1, 1.2, 3.0), grey),
        (quad(0.1, 1.2, 3.03), grey),
    ];
    let mut ids = Vec::new();
    for (mesh, mat) in parts {
        let mesh = r.add_mesh(&mut scene, &mesh);
        let i = r.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat]);
        scene.instances[i].ordered = true;
        ids.push(i);
    }
    let (dial, hidden) = (ids[1], ids[4]);
    let camera = Camera {
        position: DVec3::new(0.0, 0.0, 1.5),
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 60.0,
        near: 0.1,
        far: 200.0,
    };
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let tex = r.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    for _ in 0..2 {
        r.render_window(&mut scene, &view, w, h, &camera, &lighting);
    }
    if r.temporal.is_none() {
        return;
    }
    // the dial and the hidden part move up 40 cm, the panel and the panes stay
    let up = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.4));
    r.set_transform(&mut scene, dial, DVec3::ZERO, up);
    r.set_transform(&mut scene, hidden, DVec3::ZERO, up);
    r.render_window(&mut scene, &view, w, h, &camera, &lighting);
    let t = r
        .temporal
        .as_ref()
        .and_then(|d| d.targets.as_ref())
        .expect("the upscaler's targets");
    let motion = r.read_texture(&t.motion, wgpu::TextureAspect::All).unwrap();
    let (rw, rh) = (t.motion.width() as usize, t.motion.height() as usize);
    let at = |fx: f32| {
        let i = ((rh / 2) * rw + (fx * rw as f32) as usize) * 4;
        let (x, y) = (half(&motion[i..]), half(&motion[i + 2..]));
        (x * x + y * y).sqrt()
    };
    let (gauge, panel) = (at(0.4), at(0.6));
    assert!(
        gauge > 5.0,
        "the dial behind the gauge's glass gives it its motion: {gauge} pixels"
    );
    assert!(
        panel < 1.0,
        "the part hidden behind the panel left its motion there: {panel} pixels"
    );
}

/// A half float (`Rg16Float`'s channels), little-endian.
fn half(b: &[u8]) -> f32 {
    let h = u16::from_le_bytes([b[0], b[1]]);
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let (exp, frac) = (((h >> 10) & 0x1f) as i32, (h & 0x3ff) as f32);
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    }
}
