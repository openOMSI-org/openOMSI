//! Real ANGLE/D3D11 shader compilation on Windows' WARP software device. Ignored in the
//! workspace suite because it requires the packaged x64 DLLs; release.yml runs it after
//! downloading and checking them. WARP does not reproduce a particular GPU's driver.

#![cfg(windows)]

#[test]
#[ignore = "requires Windows and the packaged ANGLE DLLs"]
fn angle_d3d11_compiles_and_draws_on_warp() {
    // Flags and the remembered fallback are process-global. Each combination gets a
    // fresh process; a native abort in one case must not hide the others' results.
    if let Ok(case) = std::env::var("ANGLE_STARTUP_CASE") {
        let (basic, no_enhanced) = case.split_once(':').expect("test case");
        let basic = basic == "true";
        let no_enhanced = no_enhanced == "true";
        std::thread::Builder::new()
            .name("ANGLE WARP renderer".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || run_case(basic, no_enhanced))
            .expect("renderer thread")
            .join()
            .expect("renderer thread failed");
        return;
    }
    let mut failures = Vec::new();
    for basic in [false, true] {
        for no_enhanced in [true, false] {
            let case = format!("{basic}:{no_enhanced}");
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args([
                "--exact",
                "angle_d3d11_compiles_and_draws_on_warp",
                "--ignored",
                "--nocapture",
            ]);
            command.env("ANGLE_STARTUP_CASE", &case);
            if basic {
                command.env("OMSI_BASIC_PIPELINES", "1");
            } else {
                command.env_remove("OMSI_BASIC_PIPELINES");
            }
            let status = command.status().expect("ANGLE regression process");
            if !status.success() {
                failures.push(format!("{case}: {status}"));
            }
        }
    }
    assert!(failures.is_empty(), "ANGLE startup failures: {failures:?}");
}

fn run_case(basic: bool, no_enhanced: bool) {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    let directory = std::env::var("ANGLE_TEST_LIBRARY_DIRECTORY").expect("ANGLE DLL directory");
    let directory = std::fs::canonicalize(directory).expect("ANGLE DLL directory exists");
    for dll in ["libEGL.dll", "libGLESv2.dll"] {
        assert!(directory.join(dll).is_file(), "missing {dll}");
    }
    // ANGLE's libEGL from that folder, on WARP (mX's GL reads both from the environment)
    // SAFETY: set before mX loads EGL, on the test's own thread
    unsafe {
        std::env::set_var("MX_EGL_LIBRARY", directory.join("libEGL.dll"));
        std::env::set_var("MX_ANGLE_DEVICE", "warp");
    }
    let instance = omsi_render::angle::instance();
    let adapters = instance.enumerate_adapters();
    assert!(!adapters.is_empty(), "ANGLE/WARP did not expose an adapter");
    for adapter in &adapters {
        let info = adapter.info();
        eprintln!("ANGLE WARP adapter: {info:?}");
        assert_eq!(info.backend, mx::Backend::Gl);
        assert!(info.name.contains("ANGLE"), "expected ANGLE: {info:?}");
        assert_eq!(info.device_type, mx::DeviceType::Cpu, "expected WARP");
    }
    eprintln!("ANGLE WARP: basic={basic}, no_enhanced={no_enhanced}");
    let mut renderer = pollster::block_on(omsi_render::Renderer::new_with(
        &instance,
        None,
        Some(mx::TextureFormat::Rgba8UnormSrgb),
        omsi_render::RenderOptions {
            msaa: 1,
            shadow_size: 256,
            no_enhanced,
            ..Default::default()
        },
    ))
    .expect("ANGLE renderer");
    if !basic {
        assert!(
            !omsi_render::basic_pipelines(),
            "full pipeline set silently fell back to basic"
        );
    }
    let scope = renderer.watch_errors();
    let mut scene = renderer.new_scene();
    let camera = omsi_render::Camera {
        position: glam::DVec3::new(0.0, -35.0, 30.0),
        yaw: 0.0,
        pitch: -40.0,
        roll: 0.0,
        fov_deg: 60.0,
        near: 0.1,
        far: 1000.0,
    };
    let pixels = renderer
        .render_to_image(
            &mut scene,
            16,
            16,
            &camera,
            &omsi_render::Lighting::default(),
        )
        .expect("ANGLE first frame and readback");
    assert_eq!(pixels.len(), 16 * 16 * 4);
    assert!(scope.pop().is_none());
    assert!(renderer.device_lost().is_none());
}
