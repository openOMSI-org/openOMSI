use super::*;

#[derive(Debug, Clone, Copy)]
struct Case {
    enabled: bool,
    night_alpha: Option<u8>,
    night_rgb: [u8; 3],
    transmap: bool,
    declared_mask: bool,
    mode: AlphaMode,
    diffuse_alpha: u8,
    visible: bool,
}

impl Default for Case {
    fn default() -> Self {
        Self {
            enabled: true,
            night_alpha: Some(255),
            night_rgb: [32, 64, 255],
            transmap: false,
            declared_mask: false,
            mode: AlphaMode::Blend,
            diffuse_alpha: 0,
            visible: true,
        }
    }
}

/// A transparent unpowered lamp, with its entire lit picture in the night map.
/// Exercise the actual shader and blend state, including a separately supplied
/// transparency mask, rather than only checking the material's flag.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_clear_lamp_uses_its_active_night_maps_coverage() {
    check_night_coverage(1);
}

/// Build the production scene variants and actually draw/resolve at 4x MSAA.
/// A successful renderer with a silent 1x fallback must fail this regression.
#[test]
#[ignore = "requires a graphics adapter with 4x MSAA; run with --ignored on a GPU host"]
fn a_clear_lamp_uses_its_active_night_maps_coverage_at_4x_msaa() {
    check_night_coverage(4);
}

fn check_night_coverage(samples: u32) {
    let _ = env_logger::try_init();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: samples,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer");
    let info = renderer
        .adapter
        .as_ref()
        .expect("hardware adapter")
        .get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    assert_eq!(renderer.options.msaa, samples, "MSAA fallback: {info:?}");
    eprintln!(
        "night coverage: {} {:?}, requested/actual {samples}x MSAA",
        info.name, info.backend
    );
    for (classic, enhanced) in [(true, false), (false, false), (false, true)] {
        for case in [
            Case::default(),
            // RGB eligibility is covered by the scene-side alpha guard tests.
            // Here exercise fractional coverage and the background it must preserve.
            Case {
                night_alpha: Some(128),
                ..Default::default()
            },
            Case {
                enabled: false,
                visible: false,
                ..Default::default()
            },
            Case {
                night_rgb: [255; 3],
                ..Default::default()
            },
            Case {
                night_alpha: Some(0),
                visible: false,
                ..Default::default()
            },
            Case {
                transmap: true,
                visible: false,
                ..Default::default()
            },
            Case {
                night_alpha: None,
                visible: false,
                ..Default::default()
            },
            // Ordinary diffuse coverage must survive a clear night map.
            Case {
                enabled: false,
                diffuse_alpha: 255,
                night_alpha: Some(0),
                ..Default::default()
            },
            // Even a mistakenly set flag cannot turn alpha-tested artwork into a clear lamp.
            Case {
                mode: AlphaMode::Test,
                diffuse_alpha: 255,
                night_alpha: Some(0),
                ..Default::default()
            },
            Case {
                mode: AlphaMode::Opaque,
                night_alpha: Some(0),
                ..Default::default()
            },
            // A declared but missing mask keeps the existing opaque fallback.
            Case {
                declared_mask: true,
                ..Default::default()
            },
        ] {
            let mut scene = renderer.new_scene();
            let texture = |r: &mut Renderer, s: &mut Scene, rgba: [u8; 4], has_alpha| {
                r.add_texture(
                    s,
                    &omsi_texture::Image {
                        width: 1,
                        height: 1,
                        rgba: rgba.to_vec(),
                        has_alpha,
                    },
                    false,
                )
            };
            let clear = texture(&mut renderer, &mut scene, [0; 4], true);
            let diffuse = texture(
                &mut renderer,
                &mut scene,
                [0, 0, 0, case.diffuse_alpha],
                true,
            );
            let night = case.night_alpha.map(|a| {
                texture(
                    &mut renderer,
                    &mut scene,
                    [case.night_rgb[0], case.night_rgb[1], case.night_rgb[2], a],
                    true,
                )
            });
            let lamp = renderer.add_material_extra(
                &mut scene,
                Some(diffuse),
                case.mode,
                [1.0; 4],
                false,
                case.transmap.then_some((clear, true)),
                night,
                None,
                None,
                [0.0; 3],
                MaterialExtra {
                    night_switched: true,
                    night_alpha: case.enabled,
                    transmap_declared: case.declared_mask,
                    ..Default::default()
                },
            );
            let flags = scene.materials[lamp].uniform.params2[3] as u32;
            assert_eq!(
                flags & 16 != 0,
                case.enabled
                    && case.mode == AlphaMode::Blend
                    && night.is_some()
                    && !case.transmap
                    && !case.declared_mask,
                "{case:?}"
            );
            let wall = renderer.add_material(
                &mut scene,
                None,
                AlphaMode::Opaque,
                [1.0, 0.0, 0.0, 1.0],
                true,
            );
            for (y, mat) in [(6.0, wall), (4.0, lamp)] {
                let mesh = renderer.add_mesh(
                    &mut scene,
                    &MeshData {
                        positions: vec![
                            Vec3::new(-2.0, y, -2.0),
                            Vec3::new(2.0, y, -2.0),
                            Vec3::new(2.0, y, 2.0),
                            Vec3::new(-2.0, y, 2.0),
                        ],
                        normals: vec![-Vec3::Y; 4],
                        uvs: vec![glam::Vec2::splat(0.5); 4],
                        indices: vec![0, 1, 2, 0, 2, 3],
                        ranges: vec![(0, 6, 0)],
                        one_sided: false,
                    },
                );
                renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat]);
            }
            let camera = Camera {
                position: DVec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 100.0,
            };
            let lighting = Lighting {
                classic,
                enhanced,
                night: 0.0,
                shadows: false,
                fog_density: 0.0,
                ..Default::default()
            };
            let rgba = renderer
                .render_to_image(&mut scene, 64, 64, &camera, &lighting)
                .unwrap();
            assert_eq!(
                renderer.options.msaa, samples,
                "MSAA changed during rendering"
            );
            assert!(
                renderer.device_lost().is_none(),
                "device lost during rendering"
            );
            assert!(
                !renderer
                    .errors
                    .gpu_error
                    .load(std::sync::atomic::Ordering::Relaxed),
                "GPU error"
            );
            let targets = renderer
                .targets
                .msaa_targets
                .get(&(64, 64))
                .expect("scene depth target");
            assert_eq!(targets.1.texture().sample_count(), samples);
            if enhanced {
                let hdr = renderer
                    .targets
                    .hdr_targets
                    .get(&(64, 64))
                    .expect("HDR target");
                if samples > 1 {
                    assert_eq!(
                        hdr.msaa_view
                            .as_ref()
                            .expect("multisampled HDR colour")
                            .texture()
                            .sample_count(),
                        samples
                    );
                }
            } else if samples > 1 {
                assert_eq!(
                    targets
                        .0
                        .as_ref()
                        .expect("multisampled scene colour")
                        .texture()
                        .sample_count(),
                    samples
                );
            }
            let p = &rgba[(32 * 64 + 32) * 4..][..3];
            eprintln!(
                "night coverage rendered: {samples}x classic={classic} enhanced={enhanced} {case:?}: {p:?}"
            );
            if case.night_alpha == Some(128) {
                assert!(
                    p[0] > 96,
                    "fractional coverage must retain the red background: {p:?}"
                );
            }
            assert_eq!(
                p[2] > 96,
                case.visible,
                "classic={classic} enhanced={enhanced} {case:?}: {p:?}"
            );
            if case.visible && case.night_rgb == [255; 3] {
                assert!(
                    p.iter().all(|&channel| channel > 96)
                        && p.iter().max().unwrap() - p.iter().min().unwrap() <= 8,
                    "white artwork: classic={classic} enhanced={enhanced}: {p:?}"
                );
            }
        }
    }
}
