//! Exercise FXC's optimizer, which WGSL validation and HLSL generation cannot test.
use crate::*;

#[test]
#[ignore = "requires a DX12 adapter and the Windows FXC compiler"]
fn enhanced_scene_compiles_with_optimized_fxc() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::DX12;
    // Debug instance flags disable FXC optimization and hide the stack overflow.
    descriptor.flags = wgpu::InstanceFlags::empty();
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
    let instance = wgpu::Instance::new(descriptor);
    let adapter =
        pollster::block_on(instance.request_adapter(&Default::default())).expect("DX12 adapter");
    let (device, _) =
        pollster::block_on(adapter.request_device(&Default::default())).expect("DX12 device");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("enhanced FXC regression"),
        source: wgpu::ShaderSource::Wgsl(
            shaders::scene_shader_for(false, ArrayPath::Storage).into(),
        ),
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
    let buffers = [wgpu::VertexBufferLayout {
        array_stride: 32,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let target = Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Rgba16Float,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    });
    let _ = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("optimized enhanced FXC regression"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &buffers,
            compilation_options: Default::default(),
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_enhanced"),
            targets: &[target.clone(), target],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
}
