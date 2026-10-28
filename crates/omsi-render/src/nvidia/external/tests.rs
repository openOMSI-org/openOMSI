//! Transport fixtures never load NVIDIA and are not neural execution evidence.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "openomsi-nr-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(directory.join("host.ready"), "openomsi-streamline-host/1\n").unwrap();
        Self(directory)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn header() -> Header {
    Header {
        kind: REQUEST,
        session: 0,
        frame: 0,
        generation: 1,
        width: 2,
        height: 3,
        flags: 7,
        payload: 102,
        stages: 0,
        result: 0,
        camera: [0.; 80],
    }
}

#[test]
fn capture_sequence_is_exclusive_replayable_and_never_neural_evidence() {
    let parent = Directory::new();
    let path = parent.0.join("capture");
    assert!(capture::Capture::new(&path, 0).is_err());
    assert!(!path.exists());
    let mut capture = capture::Capture::new(&path, 2).unwrap();
    assert!(capture::Capture::new(&path, 2).is_err());
    let mut input = header();
    input.flags = 5;
    assert!(!capture.write(input.clone(), &[0; 102]).unwrap());
    assert!(capture.write(input, &[1; 102]).unwrap());
    let mut one = fs::File::open(path.join("00000001.omsi-nr")).unwrap();
    let a = Header::read(&mut one).unwrap();
    let b = Header::read(fs::File::open(path.join("00000002.omsi-nr")).unwrap()).unwrap();
    assert_eq!((a.frame, b.frame), (1, 2));
    assert_eq!(a.session, b.session);
    assert_eq!(a.flags & 2, 2);
    assert_eq!(b.flags & 2, 0);
    assert_eq!((a.stages, b.stages), (0, 0));
    assert_eq!(
        fs::metadata(path.join("00000001.omsi-nr")).unwrap().len(),
        486
    );
    assert!(
        fs::read_to_string(path.join("capture.json"))
            .unwrap()
            .contains("\"neural_evaluation\":false")
    );
    assert!(capture.write(header(), &[0; 102]).is_err());
}

/// Only a transport fixture. COMPLETE here simulates a receipt for parser tests;
/// it never establishes genuine runtime execution and is never compiled into a game.
fn reply(directory: &Path, stages: u32, stale: bool) -> std::thread::JoinHandle<()> {
    let directory = directory.to_owned();
    std::thread::spawn(move || {
        let start = Instant::now();
        loop {
            if let Some(path) = fs::read_dir(&directory)
                .unwrap()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "request"))
            {
                let mut input = fs::File::open(&path).unwrap();
                let mut h = Header::read(&mut input).unwrap();
                let mut data = Vec::new();
                input.read_to_end(&mut data).unwrap();
                assert_eq!(data.len() as u64, h.payload);
                if h.width == 32 {
                    assert_ne!(
                        &data[(6 * 32 + 6) * 4..][..3],
                        &[255, 0, 0],
                        "HUD entered the inference input"
                    );
                    let i = 32 * 32 * 12 + (16 * 32 + 16) * 4;
                    let depth = f32::from_le_bytes(data[i..i + 4].try_into().unwrap());
                    assert!(
                        depth > 0. && depth <= 1.,
                        "captured geometry depth must be reverse Z"
                    );
                }
                h.kind = RESPONSE;
                h.payload = h.width as u64 * h.height as u64 * 4;
                h.stages = stages;
                if stale {
                    h.generation += 1;
                }
                let pending = path.with_extension("testpending");
                let mut f = fs::File::create(&pending).unwrap();
                h.write(&mut f).unwrap();
                f.write_all(&vec![42; h.payload as usize]).unwrap();
                drop(f);
                fs::rename(pending, path.with_extension("response")).unwrap();
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(1));
        }
    })
}

#[test]
fn missing_or_unknown_host_and_invalid_timeout_do_not_attach() {
    let d = Directory::new();
    fs::remove_file(d.0.join("host.ready")).unwrap();
    assert!(Session::connect(&d.0, Duration::from_secs(1)).is_err());
    fs::write(d.0.join("host.ready"), "unrelated-copy-tool\n").unwrap();
    assert!(Session::connect(&d.0, Duration::from_secs(1)).is_err());
    assert!(!d.0.join("client.lock").exists());
    assert!(Session::connect(&d.0, Duration::ZERO).is_err());
    assert!(Session::connect(&d.0, Duration::from_secs(31)).is_err());
}

#[test]
fn actual_file_exchange_lock_ownership_and_cleanup() {
    let d = Directory::new();
    let mut session = Session::connect(&d.0, Duration::from_secs(2)).unwrap();
    assert!(Session::connect(&d.0, Duration::from_secs(2)).is_err());
    let peer = reply(&d.0, COMPLETE, false);
    let mut diagnostic = Diagnostics::default();
    assert_eq!(
        session
            .exchange(header(), &[0; 102], &mut diagnostic)
            .unwrap(),
        vec![42; 24]
    );
    peer.join().unwrap();
    assert_eq!(diagnostic.evaluations, 1);
    assert_eq!(diagnostic.compositions, 0);
    assert_ne!(diagnostic.state, State::Active);
    assert!(
        !fs::read_dir(&d.0).unwrap().filter_map(Result::ok).any(|e| e
            .path()
            .extension()
            .is_some_and(|e| e == "request" || e == "response"))
    );
    drop(session);
    assert!(!d.0.join("client.lock").exists());
}

#[test]
fn stale_copy_timeout_and_disconnect_preserve_fallback() {
    for (stages, stale) in [(LOADED, false), (COMPLETE, true)] {
        let d = Directory::new();
        let mut s = Session::connect(&d.0, Duration::from_secs(2)).unwrap();
        let peer = reply(&d.0, stages, stale);
        let mut diagnostic = Diagnostics::default();
        assert!(s.exchange(header(), &[0; 102], &mut diagnostic).is_err());
        peer.join().unwrap();
        assert_eq!(diagnostic.evaluations, 0);
        assert_eq!(diagnostic.compositions, 0);
    }
    let d = Directory::new();
    let mut s = Session::connect(&d.0, Duration::from_millis(10)).unwrap();
    assert!(
        s.exchange(header(), &[0; 102], &mut Diagnostics::default())
            .is_err()
    );
    let mut a = ExternalAdapter {
        session: Some(s),
        ..Default::default()
    };
    a.fail("host closed");
    assert!(!a.enabled());
    assert_eq!(a.diagnostics.state, State::EvaluationFailed);
    assert!(!d.0.join("client.lock").exists());
}

#[test]
fn formats_motion_direction_depth_and_nonfinite_input() {
    let motion = [0x3c00u16, 0xc000u16, 0x7c00u16, 0]; // (+1,-2), (+inf,0)
    let planes = [
        vec![3, 2, 1, 255, 6, 5, 4, 255],
        motion.into_iter().flat_map(u16::to_le_bytes).collect(),
        [0.25f32, 0.75]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect(),
        vec![255; 2],
    ];
    let (packed, color, validity) =
        resources::payload(planes, wgpu::TextureFormat::Bgra8UnormSrgb).unwrap();
    assert_eq!(color, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    assert_eq!(validity, [255, 0]);
    let float = |i| f32::from_le_bytes(packed[i..i + 4].try_into().unwrap());
    assert_eq!(float(8), 1.);
    assert_eq!(float(12), -2.);
    assert_eq!(float(16), 0.);
    assert_eq!(float(24), 0.25);
    assert_eq!(float(28), 0.75);
    assert_eq!(packed.len(), 34);
}

#[test]
fn streamline_row_vector_constants_rebase_large_world_origin() {
    use glam::{DVec3, Mat4, Vec4};
    let ro = DVec3::new(9_000_000., -8_000_000., 0.);
    let camera = crate::Camera {
        position: ro + DVec3::new(3., 4., 2.),
        yaw: 21.,
        pitch: 5.,
        roll: 0.,
        fov_deg: 60.,
        near: 0.1,
        far: 5000.,
    };
    let vp = camera.view_proj(2., ro);
    let old = Mat4::from_translation(glam::Vec3::new(0.1, -0.2, 0.)) * vp;
    let values = resources::camera_constants(&camera, ro, vp, old, (20, 10));
    let clip = Vec4::new(0.2, -0.1, 0.7, 1.);
    let expected = old * vp.inverse() * clip;
    // Multiply a row vector by SL's row-major matrix.
    let got = Vec4::from_array(std::array::from_fn(|j| {
        (0..4).map(|i| clip[i] * values[32 + i * 4 + j]).sum()
    }));
    assert!((got - expected).abs().max_element() < 1e-5);
    assert_eq!(&values[64..67], &[3., 4., 2.]);
    assert_eq!(&values[76..78], &[0.1, 5000.]);
    let vector = |i| glam::Vec3::from_slice(&values[i..i + 3]);
    let to_world = Mat4::from_cols(
        vector(70).extend(0.),
        vector(67).extend(0.),
        vector(73).extend(0.),
        vector(64).extend(1.),
    );
    assert!((vector(73).cross(vector(70)) - vector(67)).length() < 1e-5);
    let projected = Mat4::from_cols_array(values[..16].try_into().unwrap()) * to_world.inverse();
    assert!(
        (projected - vp)
            .to_cols_array()
            .iter()
            .all(|v| v.abs() < 1e-5)
    );
}

#[test]
#[ignore = "requires cmake-built target/dlss5-community/protocol-build/neural-protocol-test"]
fn rust_cpp_protocol_interoperability() {
    let d = Directory::new();
    let path = d.0.join("frame.bin");
    let mut f = fs::File::create(&path).unwrap();
    let mut h = header();
    h.session = 13;
    h.frame = 6;
    h.generation = 2;
    h.write(&mut f).unwrap();
    f.write_all(&[0; 102]).unwrap();
    drop(f);
    let tool = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/dlss5-community/protocol-build/neural-protocol-test");
    assert!(
        std::process::Command::new(tool)
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn composition_shader_validates() {
    let source = include_str!("composite.wgsl");
    let module = naga::front::wgsl::parse_str(source).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
#[ignore = "requires native GPU; synthetic IPC peer tests composition/fallback, never NVIDIA inference"]
fn external_gpu_transport_fallback_and_synthetic_composition() {
    use crate::*;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut r = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            preview_only: true,
            msaa: 1,
            ssao: false,
            fxaa: false,
            no_enhanced: true,
            shadow_size: 512,
            ..Default::default()
        },
    ))
    .unwrap();
    eprintln!("external transport GPU: {:?}", r.device.adapter_info());
    let mut scene = r.new_scene();
    let mesh = r.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                glam::Vec3::new(-2., 5., -2.),
                glam::Vec3::new(2., 5., -2.),
                glam::Vec3::new(2., 5., 2.),
                glam::Vec3::new(-2., 5., 2.),
            ],
            normals: vec![-glam::Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    let material = r.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.2, 0.6, 0.3, 1.],
        true,
    );
    r.add_instance(
        &mut scene,
        mesh,
        glam::DVec3::ZERO,
        glam::Mat4::IDENTITY,
        vec![material],
    );
    let red = r.add_texture(
        &mut scene,
        &omsi_texture::Image {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
            has_alpha: false,
        },
        false,
    );
    scene.overlays.push((red, [0., 0., 12., 12.]));
    let camera = Camera {
        position: glam::DVec3::ZERO,
        yaw: 0.,
        pitch: 0.,
        roll: 0.,
        fov_deg: 60.,
        near: 0.1,
        far: 1000.,
    };
    let target = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("external test presentation"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    r.render_inner(
        &mut scene,
        &view,
        32,
        32,
        &camera,
        &Lighting::default(),
        true,
        None,
        None,
        false,
    );
    let before = r.read_texture(&target, wgpu::TextureAspect::All).unwrap();
    assert_eq!(&before[(6 * 32 + 6) * 4..][..4], &[255, 0, 0, 255]);
    let d = Directory::new();
    r.neural_external = ExternalAdapter {
        session: Some(Session::connect(&d.0, Duration::from_secs(5)).unwrap()),
        environment_checked: true,
        ..Default::default()
    };
    let peer = reply(&d.0, LOADED, false);
    r.render_inner(
        &mut scene,
        &view,
        32,
        32,
        &camera,
        &Lighting::default(),
        true,
        None,
        None,
        false,
    );
    peer.join().unwrap();
    let after = r.read_texture(&target, wgpu::TextureAspect::All).unwrap();
    assert_eq!(before, after);
    assert_eq!(r.external_neural_diagnostics().evaluations, 0);
    assert_eq!(
        r.external_neural_diagnostics().state,
        State::EvaluationFailed
    );
    assert_eq!(r.external_neural_diagnostics().compositions, 0);
    assert!(
        r.neural_external.resources.is_none(),
        "failed adapter must release optional allocations"
    );
    r.render_inner(
        &mut scene,
        &view,
        32,
        32,
        &camera,
        &Lighting::default(),
        true,
        None,
        None,
        false,
    );
    assert_eq!(
        after,
        r.read_texture(&target, wgpu::TextureAspect::All).unwrap()
    );
    r.disable_external_neural();
    assert_eq!(r.external_neural_diagnostics().state, State::Disabled);
    // A synthetic COMPLETE receipt exercises the OUTPUT branch with a constant test
    // image. These counters are fixture assertions, never real NVIDIA evidence.
    let successful = Directory::new();
    r.neural_external = ExternalAdapter {
        session: Some(Session::connect(&successful.0, Duration::from_secs(5)).unwrap()),
        environment_checked: true,
        ..Default::default()
    };
    for _ in 0..2 {
        let peer = reply(&successful.0, COMPLETE, false);
        r.render_inner(
            &mut scene,
            &view,
            32,
            32,
            &camera,
            &Lighting::default(),
            true,
            None,
            None,
            false,
        );
        peer.join().unwrap();
    }
    let composed = r.read_texture(&target, wgpu::TextureAspect::All).unwrap();
    assert_eq!(&composed[(16 * 32 + 16) * 4..][..4], &[42, 42, 42, 255]);
    assert_eq!(&composed[(6 * 32 + 6) * 4..][..4], &[255, 0, 0, 255]);
    assert_eq!(
        &composed[(1 * 32 + 30) * 4..][..4],
        &before[(1 * 32 + 30) * 4..][..4],
        "invalid sky must retain original color"
    );
    assert_eq!(r.external_neural_diagnostics().evaluations, 2);
    assert_eq!(r.external_neural_diagnostics().compositions, 1);
    assert_eq!(r.external_neural_diagnostics().state, State::Active);
    let small = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("resized neural fixture"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let peer = reply(&successful.0, COMPLETE, false);
    r.render_inner(
        &mut scene,
        &small.create_view(&Default::default()),
        16,
        16,
        &camera,
        &Lighting::default(),
        true,
        None,
        None,
        false,
    );
    peer.join().unwrap();
    assert_eq!(r.external_neural_diagnostics().evaluations, 3);
    assert_eq!(
        r.external_neural_diagnostics().compositions,
        1,
        "resize must reset validity before using output"
    );
    assert_eq!(
        r.external_neural_diagnostics().state,
        State::ReadyForTransport
    );
    r.disable_external_neural();
    r.render_inner(
        &mut scene,
        &view,
        32,
        32,
        &camera,
        &Lighting::default(),
        true,
        None,
        None,
        false,
    );
    assert_eq!(
        r.read_texture(&target, wgpu::TextureAspect::All).unwrap(),
        before
    );
    let captures = Directory::new();
    let output = captures.0.join("real-inputs");
    let evaluations = r.external_neural_diagnostics().evaluations;
    r.request_neural_capture(&output, 2).unwrap();
    for _ in 0..2 {
        r.render(&mut scene, &view, 32, 32, &camera, &Lighting::default());
    }
    assert_eq!(
        r.external_neural_diagnostics().state,
        State::CaptureComplete
    );
    assert_eq!(r.external_neural_diagnostics().captures, 2);
    assert_eq!(r.external_neural_diagnostics().evaluations, evaluations);
    let mut file = fs::File::open(output.join("00000002.omsi-nr")).unwrap();
    let header = Header::read(&mut file).unwrap();
    assert_eq!(
        header.flags & 2,
        0,
        "consecutive capture must retain history"
    );
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes[32 * 32 * 16 + 16 * 32 + 16], 255);
    assert_ne!(
        &bytes[(6 * 32 + 6) * 4..][..3],
        &[255, 0, 0],
        "capture must exclude HUD"
    );
    assert_eq!(
        r.read_texture(&target, wgpu::TextureAspect::All).unwrap(),
        before
    );
}
