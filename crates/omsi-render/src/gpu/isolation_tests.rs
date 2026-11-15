use super::*;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn auxiliary_adapter_state_does_not_change_the_game() {
    let before = GL_BACKEND.load(Relaxed);
    let budget = ADAPTER_TEXTURE_MB.load(Relaxed);
    with_auxiliary_renderer(|| {
        GL_BACKEND.store(!before, Relaxed);
        assert_eq!(GL_BACKEND.load(Relaxed), !before);
        assert!(auxiliary_context().is_some());
        std::thread::scope(|scope| {
            let context = auxiliary_context();
            scope.spawn(move || with_auxiliary_context(context, || {
                assert_eq!(GL_BACKEND.load(Relaxed), !before);
                AMD_VULKAN.store(true, Relaxed);
            })).join().unwrap();
        });
        assert!(AMD_VULKAN.load(Relaxed));
        let rt = crate::RT_GBUF.load(Relaxed);
        crate::RT_GBUF.store(!rt, Relaxed);
        crate::RT_BUFFERS.store(true, Relaxed);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                assert!(!crate::RT_GBUF.load(Relaxed));
                assert!(!crate::RT_BUFFERS.load(Relaxed));
            }).join().unwrap();
        });
        ARRAY_PATH.store(3, Relaxed);
        assert!(matches!(array_path(), ArrayPath::LightTextures));
        BASIC_PIPELINES.store(true, Relaxed);
        assert!(basic_pipelines());
        std::thread::scope(|scope| {
            scope.spawn(|| {
                assert_eq!(GL_BACKEND.load(Relaxed), before);
                assert!(auxiliary_context().is_none());
            }).join().unwrap();
        });
    });
    assert_eq!(GL_BACKEND.load(Relaxed), before);
    assert_eq!(ADAPTER_TEXTURE_MB.load(Relaxed), budget);
    assert!(auxiliary_context().is_none());
}

#[test]
fn auxiliary_adapter_state_is_restored_after_failure() {
    let before = ceiling();
    let result = std::panic::catch_unwind(|| with_auxiliary_renderer(|| {
        update_ceiling(|c| *c = None);
        assert!(ceiling().is_none());
        panic!("failed preview device");
    }));
    assert!(result.is_err());
    assert_eq!(ceiling(), before);
    assert!(auxiliary_context().is_none());
}

#[test]
fn auxiliary_adapter_state_device_open_does_not_publish_metadata() {
    let gpu_options = omsi_texture::gpu_options();
    let texture = ADAPTER_TEXTURE_MB.load(Relaxed);
    let vram = ADAPTER_VRAM_MB.load(Relaxed);
    let info = ADAPTER_INFO.lock().unwrap_or_else(|e| e.into_inner()).clone();
    with_auxiliary_renderer(|| {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        let opened = pollster::block_on(Renderer::open_on(adapter, None, None, RenderOptions::default())).unwrap();
        let name = "auxiliary-fallback-must-not-be-written";
        let before = fallback_path().and_then(|p| std::fs::read(p).ok());
        fallback_store(name, Some((1, true)));
        assert_eq!(fallback_path().and_then(|p| std::fs::read(p).ok()), before);
        assert!(ceiling().is_some());
        assert_eq!(omsi_texture::gpu_options(), gpu_options);
        assert!(fallback_load("auxiliary must not load fallbacks").is_none());
        assert_eq!(ADAPTER_TEXTURE_MB.load(Relaxed), texture);
        assert_eq!(ADAPTER_VRAM_MB.load(Relaxed), vram);
        assert_eq!(*ADAPTER_INFO.lock().unwrap_or_else(|e| e.into_inner()), info);
        let jobs: Vec<crate::pipelines::parallel::Job<bool>> = (0..16)
            .map(|_| -> crate::pipelines::parallel::Job<bool> { Box::new(|| auxiliary_context().is_some()) })
            .collect();
        assert!(crate::pipelines::parallel::compile_all(opened.device(), jobs).into_iter().all(|local| local));
        let renderer = pollster::block_on(opened.finish()).unwrap();
        assert_eq!(renderer.adapter().unwrap().get_info().backend, wgpu::Backend::Noop);
        assert_eq!(omsi_texture::gpu_options(), gpu_options);
        assert_eq!(ADAPTER_TEXTURE_MB.load(Relaxed), texture);
    });
    assert_eq!(ADAPTER_TEXTURE_MB.load(Relaxed), texture);
    assert_eq!(ADAPTER_VRAM_MB.load(Relaxed), vram);
    assert_eq!(*ADAPTER_INFO.lock().unwrap_or_else(|e| e.into_inner()), info);
}

#[test]
fn auxiliary_adapter_state_preserves_enhanced_plus_parent() {
    with_auxiliary_renderer(|| {
        crate::RT_GBUF.store(true, Relaxed);
        crate::RT_BUFFERS.store(true, Relaxed);
        with_auxiliary_renderer(|| {
            crate::RT_GBUF.store(false, Relaxed);
            crate::RT_BUFFERS.store(false, Relaxed);
            assert!(!crate::rt_gbuf());
        });
        assert!(crate::rt_gbuf());
        assert!(crate::RT_BUFFERS.load(Relaxed));
    });
}

#[test]
fn mesh_pages_use_their_device_not_another_renderers_rt_state() {
    with_auxiliary_renderer(|| {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        crate::RT_BUFFERS.store(true, Relaxed);
        let context = auxiliary_context();
        std::thread::scope(|scope| {
            scope.spawn(move || with_auxiliary_context(context, || {
                assert!(crate::MeshPage::new(&device, 4096, 4096).is_some(),
                    "a non-RT preview device must not request BLAS input buffers");
            })).join().unwrap();
        });
    });
}

#[test]
fn mesh_blas_usage_respects_fallback_and_device_features() {
    with_auxiliary_renderer(|| {
        for enabled in [false, true] {
            crate::RT_BUFFERS.store(enabled, Relaxed);
            for capable in [false, true] {
                let features = if capable { wgpu::Features::EXPERIMENTAL_RAY_QUERY } else { wgpu::Features::empty() };
                let usage = crate::mesh_blas_usage(features);
                assert_eq!(usage.contains(wgpu::BufferUsages::BLAS_INPUT), enabled && capable);
                assert_eq!(usage.contains(wgpu::BufferUsages::STORAGE), enabled && capable);
            }
        }
    });
}
