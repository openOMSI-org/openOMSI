//! GPU regression for the roof map shared by the rain and snow vertex shaders.

use super::*;

fn center_depth(renderer: &Renderer) -> f32 {
    let rgba = renderer
        .read_texture(&renderer.shadows.precip.color_texture, wgpu::TextureAspect::All)
        .expect("read the precipitation roof map");
    let at = ((PRECIP_TEXELS / 2 * PRECIP_TEXELS + PRECIP_TEXELS / 2) * 4) as usize;
    let bits = u32::from(rgba[at]) | (u32::from(rgba[at + 1]) << 8) | (u32::from(rgba[at + 2]) << 16);
    bits as f32 / 16_777_215.0
}

/// Run the very same `under_roof` WGSL function used by rain and snow in a
/// vertex shader, at one point below and one above the roof.
fn probe_occlusion(renderer: &Renderer, matrix: Mat4) -> [u8; 2] {
    let columns = matrix.to_cols_array_2d();
    let matrix_source = columns
        .iter()
        .map(|c| format!("vec4<f32>({:.9}, {:.9}, {:.9}, {:.9})", c[0], c[1], c[2], c[3]))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "{}\n{}",
        include_str!("../precip_common.wgsl"),
        r#"
struct RoofProbeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) blocked: f32,
};
@vertex fn roof_test_vs(@builtin(vertex_index) i: u32, @builtin(instance_index) k: u32) -> RoofProbeOut {
    let matrix = mat4x4<f32>(__MATRIX__);
    let point = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, 13.0), k == 1u);
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: RoofProbeOut;
    out.clip = vec4<f32>(-1.0 + f32(k) + p.x * 0.5, p.y * 2.0 - 1.0, 0.0, 1.0);
    out.blocked = select(0.0, 1.0, under_roof(point, matrix, 1.0));
    return out;
}
@fragment fn roof_test_fs(in: RoofProbeOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.blocked, 0.0, 0.0, 1.0);
}
"#
        .replace("__MATRIX__", &matrix_source)
    );
    let device = &renderer.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("roof occlusion probe"), source: wgpu::ShaderSource::Wgsl(roof_as_float(source).into()) });
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("roof probe camera"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("roof probe camera"),
        layout: &camera_layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: renderer.camera_buf.as_entire_binding() }],
    });
    let roof_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("roof probe texture"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        }],
    });
    let roof_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("roof probe texture"),
        layout: &roof_layout,
        entries: &[wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&renderer.shadows.precip.color_view) }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("roof probe"),
        bind_group_layouts: &[Some(&camera_layout), Some(&roof_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("roof probe"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("roof_test_vs"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("roof_test_fs"),
            targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("roof probe result"),
        size: wgpu::Extent3d { width: 2, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("roof probe"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &camera_group, &[]);
        pass.set_bind_group(1, &roof_group, &[]);
        pass.draw(0..3, 0..2);
    }
    renderer.queue.submit(Some(encoder.finish()));
    let rgba = renderer.read_texture(&target, wgpu::TextureAspect::All).expect("read roof probe");
    [rgba[0], rgba[4]]
}

#[test]
#[ignore = "requires a graphics adapter; CI runs it on ANGLE/CGL and native Metal"]
fn roof_occlusion_gpu() {
    let requested = std::env::var("OPENOMSI_ROOF_TEST_BACKEND").unwrap_or_else(|_| "gl".to_owned());
    let backends = match requested.as_str() {
        "angle" | "gl" => wgpu::Backends::GL,
        "metal" => wgpu::Backends::METAL,
        "vulkan" => wgpu::Backends::VULKAN,
        _ => panic!("unknown roof test backend: {requested}"),
    };
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = backends;
    let instance = if requested == "angle" { crate::angle::instance(descriptor) } else { wgpu::Instance::new(descriptor) };
    let adapters = pollster::block_on(instance.enumerate_adapters(backends));
    let adapter = adapters.first().expect("roof test graphics adapter");
    eprintln!("roof test adapter: {:?}", adapter.get_info());
    let mut renderer = pollster::block_on(Renderer::new_on(
        adapter.clone(),
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 256, preview_only: true, ..Default::default() },
    ))
    .expect("roof test renderer");
    assert!(renderer.pass.rain_fall_pipeline.is_some(), "rain pipeline");
    assert!(renderer.pass.snow_pipeline.is_some(), "snow pipeline");
    let mut scene = renderer.new_scene();
    let camera = Camera { position: DVec3::new(0.0, 0.0, 1.0), yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let mut lighting = Lighting { shadows: false, ..Default::default() };
    scene.rainfall.rate = 0.1;
    scene.rainfall.drop_mm = 2.0;
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).expect("rain without a roof");
    assert_eq!(center_depth(&renderer), 1.0, "an empty roof map clears to far depth");

    let roof = MeshData {
        positions: vec![Vec3::new(-12.0, -12.0, 12.0), Vec3::new(12.0, -12.0, 12.0), Vec3::new(12.0, 12.0, 12.0), Vec3::new(-12.0, 12.0, 12.0)],
        normals: vec![Vec3::Z; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        ranges: vec![(0, 6, 0)],
        one_sided: false,
    };
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.3, 0.3, 0.3, 1.0], true);
    let mesh = renderer.add_mesh(&mut scene, &roof);
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).expect("rain under a roof");
    let matrix = renderer.shadows.precip.matrix.get().expect("rain drew the roof map");
    let depth = center_depth(&renderer);
    let expected = matrix.project_point3(Vec3::new(0.0, 0.0, 12.0)).z;
    assert!((depth - expected).abs() < 0.0002, "roof depth {depth}, expected {expected}");
    assert_eq!(probe_occlusion(&renderer, matrix), [255, 0], "rain and snow must reject a point below the roof, but not one above it");

    scene.rainfall.rate = 0.0;
    lighting.snowfall = 0.2;
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).expect("snow under a roof");
    let snow_matrix = renderer.shadows.precip.matrix.get().expect("snow drew the roof map");
    assert!((center_depth(&renderer) - snow_matrix.project_point3(Vec3::new(0.0, 0.0, 12.0)).z).abs() < 0.0002);
    assert_eq!(probe_occlusion(&renderer, snow_matrix), [255, 0]);
    assert!(renderer.device_lost().is_none());
}
