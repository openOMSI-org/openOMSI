//! The window's main-pass bundles are kept from one frame to the next part by part
//! (`record_main_bundles`). A part kept although what it draws changed would show last
//! frame's draws: each case here changes one thing, checks which parts were recorded again,
//! and compares the picture with one drawn without any kept bundle - on the same renderer
//! with its bundles dropped, and on a second renderer drawing the same scene for the first
//! time.

use super::*;

const SIDE: usize = 34;
const SIZE: u32 = 160;

fn renderer(instance: &wgpu::Instance) -> Renderer {
    pollster::block_on(Renderer::new_with(
        instance,
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

/// What the cases change, in order (each case keeps the ones before it).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    Nothing,
    Move,
    Add,
    Remove,
    Texture,
    Material,
    Hide,
    Lod,
    Mirror,
    MirrorSteady,
}

const CASES: [Change; 12] = [
    Change::Nothing,
    Change::Move,
    Change::Add,
    Change::Remove,
    Change::Texture,
    Change::Material,
    Change::Hide,
    Change::Lod,
    Change::Nothing,
    Change::Mirror,
    Change::Nothing,
    Change::MirrorSteady,
];

/// A wall of small quads, each with a material of its own (so a batch each, and the main
/// pass in four bundles), one of them textured.
struct Wall {
    scene: Scene,
    mesh: MeshId,
    texture: TextureId,
    spare: MaterialId,
    mirror: TextureId,
}

fn wall(r: &Renderer) -> Wall {
    let mut scene = r.new_scene();
    let h = 0.42;
    let mesh = r.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                Vec3::new(-h, 0.0, -h),
                Vec3::new(h, 0.0, -h),
                Vec3::new(h, 0.0, h),
                Vec3::new(-h, 0.0, h),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            ranges: vec![(0, 6, 0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            one_sided: false,
        },
    );
    let texture = r.add_texture(
        &mut scene,
        &omsi_texture::Image {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
            has_alpha: false,
        },
        false,
    );
    for k in 0..SIDE * SIDE {
        let (x, z) = (
            (k % SIDE) as f64 - SIDE as f64 / 2.0,
            (k / SIDE) as f64 - SIDE as f64 / 2.0,
        );
        let c = [
            (k % 7) as f32 / 6.0,
            (k % 11) as f32 / 10.0,
            (k % 13) as f32 / 12.0,
            1.0,
        ];
        let material = if k == SIDE * SIDE / 2 {
            r.add_material(&mut scene, Some(texture), AlphaMode::Opaque, [1.0; 4], true)
        } else {
            r.add_material(&mut scene, None, AlphaMode::Opaque, c, true)
        };
        r.add_instance(
            &mut scene,
            mesh,
            DVec3::new(x, 22.0, z),
            Mat4::IDENTITY,
            vec![material],
        );
    }
    let spare = r.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 1.0, 0.0, 1.0],
        true,
    );
    let mirror = r.add_render_texture(&mut scene, 64, 64);
    Wall {
        scene,
        mesh,
        texture,
        spare,
        mirror,
    }
}

/// Make `change` to the wall (the same on either renderer).
fn apply(r: &mut Renderer, w: &mut Wall, change: Change, camera: &Camera, lighting: &Lighting) {
    let s = &mut w.scene;
    match change {
        Change::Nothing => {}
        Change::Move => {
            let o = s.instances[5].origin;
            r.set_transform(s, 5, o + DVec3::new(0.0, 0.0, 0.5), Mat4::IDENTITY);
        }
        Change::Add => {
            let m = r.add_material(s, None, AlphaMode::Opaque, [0.0, 1.0, 1.0, 1.0], true);
            r.add_instance(
                s,
                w.mesh,
                DVec3::new(0.0, 21.0, 0.3),
                Mat4::IDENTITY,
                vec![m],
            );
        }
        Change::Remove => r.remove_instance(s, 3),
        Change::Texture => {
            let green = omsi_texture::Image {
                width: 1,
                height: 1,
                rgba: vec![0, 255, 0, 255],
                has_alpha: false,
            };
            r.replace_texture(s, w.texture, &omsi_texture::TextureData::from_image(green));
            r.rebind_textures(s, &[w.texture]);
        }
        Change::Material => r.set_material(s, 10, 0, w.spare),
        Change::Hide => {
            s.instances[20].visible = false;
            s.dirty = true;
        }
        Change::Lod => r.set_lod_range(s, 30, 1e9, f32::MAX),
        // a mirror's picture in between (it keeps no bundles of the window's)
        Change::Mirror => r.render_to_texture(s, w.mirror, camera, lighting, 1.0),
        // as in the game: every window frame follows a mirror's
        Change::MirrorSteady => {
            r.render_to_texture(s, w.mirror, camera, lighting, 1.0);
            r.render_to_image(s, SIZE, SIZE, camera, lighting).unwrap();
            r.render_to_texture(s, w.mirror, camera, lighting, 1.0);
        }
    }
}

fn picture(r: &mut Renderer, w: &mut Wall, camera: &Camera, lighting: &Lighting) -> Vec<u8> {
    r.render_to_image(&mut w.scene, SIZE, SIZE, camera, lighting)
        .unwrap()
}

fn settle(r: &mut Renderer, w: &mut Wall, camera: &Camera, lighting: &Lighting, parts: usize) {
    picture(r, w, camera, lighting);
    assert_eq!(
        r.history.bundle_stats.get(),
        (parts, 0),
        "the first repeat records again to keep what it binds"
    );
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn kept_bundles_draw_what_a_fresh_recording_draws() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut kept = renderer(&instance);
    let mut fresh = renderer(&instance);
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 200.0,
    };
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let mut w = wall(&kept);
    // frame A: everything recorded
    let mut before = picture(&mut kept, &mut w, &camera, &lighting);
    let (recorded, reused) = kept.history.bundle_stats.get();
    assert_eq!(reused, 0);
    assert!(
        recorded >= 4,
        "the wall should take four bundles or more, got {recorded}"
    );
    let parts = recorded;
    settle(&mut kept, &mut w, &camera, &lighting, parts);
    for (n, &change) in CASES.iter().enumerate() {
        apply(&mut kept, &mut w, change, &camera, &lighting);
        let now = picture(&mut kept, &mut w, &camera, &lighting);
        let (recorded, reused) = kept.history.bundle_stats.get();
        assert_eq!(recorded + reused, parts, "{change:?}: parts");
        match change {
            // the same draws (a move is in the instances' buffer, not in the bundles)
            Change::Nothing | Change::Move | Change::Mirror | Change::MirrorSteady => {
                assert_eq!(recorded, 0, "{change:?}: nothing to record")
            }
            // one draw more or less, or another texture or material in one: its part and
            // those after it again, the ones before it kept
            _ => assert!(
                recorded >= 1,
                "{change:?}: a changed part must be recorded again"
            ),
        }
        if matches!(change, Change::Add | Change::Texture) {
            assert!(
                reused >= 1,
                "{change:?}: the parts before the change should be kept"
            );
        }
        if matches!(
            change,
            Change::Move
                | Change::Add
                | Change::Remove
                | Change::Texture
                | Change::Material
                | Change::Hide
                | Change::Lod
        ) {
            assert_ne!(now, before, "{change:?}: the change should show");
        }
        // the same frame recorded from scratch on this renderer
        *kept.history.main_bundles.borrow_mut() = None;
        let again = picture(&mut kept, &mut w, &camera, &lighting);
        assert_eq!(kept.history.bundle_stats.get().1, 0);
        assert!(
            now == again,
            "{change:?}: kept bundles drew something else than recorded ones"
        );
        settle(&mut kept, &mut w, &camera, &lighting, parts);
        // and the same scene drawn by another renderer for the first time
        let mut other = wall(&fresh);
        for &c in &CASES[..=n] {
            apply(&mut fresh, &mut other, c, &camera, &lighting);
        }
        *fresh.history.main_bundles.borrow_mut() = None;
        let first = picture(&mut fresh, &mut other, &camera, &lighting);
        assert!(
            now == first,
            "{change:?}: kept bundles drew something else than a fresh renderer"
        );
        before = now;
    }
    // another pass mode is never served the bundles of the last one
    let enhanced = Lighting {
        enhanced: true,
        ..lighting.clone()
    };
    picture(&mut kept, &mut w, &camera, &enhanced);
    assert_eq!(
        kept.history.bundle_stats.get().1,
        0,
        "the enhanced pass must not reuse the plain pass's bundles"
    );
    picture(&mut kept, &mut w, &camera, &lighting);
    assert_eq!(kept.history.bundle_stats.get().1, 0, "and back");
}
