//! Exercise FXC's optimizer, which WGSL validation and HLSL generation cannot test.
use crate::*;

#[test]
#[ignore = "requires a DX12 adapter and the Windows FXC compiler"]
fn enhanced_scene_compiles_with_optimized_fxc() {
    // FXC, optimizing (debug shaders disable FXC's optimizer and hide the stack overflow)
    // SAFETY: set before mX reads them, on the test's own thread
    unsafe {
        std::env::set_var("MX_DX12_COMPILER", "fxc");
        std::env::set_var("MX_DX12_SHADER_DEBUG", "0");
    }
    let instance = mx::Instance::with_backends(mx::Backends::DX12);
    let adapter = instance.default_adapter().expect("DX12 adapter");
    let (device, _) = adapter
        .request_device(&Default::default())
        .expect("DX12 device");
    let shader = device
        .create_shader_module(&mx::ShaderModuleDescriptor {
            label: Some("enhanced FXC regression"),
            source: &*(shaders::scene_shader_for(false, ArrayPath::Storage)),
        })
        .expect("mX: a shader");
    let attributes = mx::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
    let buffers = [mx::VertexBufferLayout {
        array_stride: 32,
        step_mode: mx::VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let target = Some(mx::ColorTargetState {
        format: mx::TextureFormat::Rgba16Float,
        blend: None,
        write_mask: mx::ColorWrites::ALL,
    });
    let _ = device
        .create_render_pipeline(&mx::RenderPipelineDescriptor {
            label: Some("optimized enhanced FXC regression"),
            vertex: mx::VertexState {
                constants: &[],
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &buffers,
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(mx::FragmentState {
                constants: &[],
                module: &shader,
                entry_point: Some("fs_enhanced"),
                targets: &[target.clone(), target],
            }),
        })
        .expect("mX: a render pipeline");
}
