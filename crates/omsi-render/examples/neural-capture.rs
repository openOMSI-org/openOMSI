//! Real renderer input export, never a generated neural output.
use anyhow::{Context, Result, ensure};
use glam::{DVec3, Mat4, Vec2, Vec3};
use omsi_geometry::MeshData;
use omsi_render::*;
use std::path::Path;

fn main() -> Result<()> {
    env_logger::init();
    let mut arguments = std::env::args_os().skip(1);
    let directory = arguments
        .next()
        .context("usage: neural-capture <new-capture-directory> [width height]")?;
    let width = arguments
        .next()
        .map(|v| v.to_string_lossy().parse::<u32>())
        .transpose()?
        .unwrap_or(256);
    let height = arguments
        .next()
        .map(|v| v.to_string_lossy().parse::<u32>())
        .transpose()?
        .unwrap_or(width);
    ensure!(
        arguments.next().is_none() && width > 0 && height > 0 && width <= 3840 && height <= 2160,
        "expected dimensions within 1..3840 x 1..2160"
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
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
    ))?;
    let mut scene = renderer.new_scene();
    let mesh = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                Vec3::new(-2., 5., -2.),
                Vec3::new(2., 5., -2.),
                Vec3::new(2., 5., 2.),
                Vec3::new(-2., 5., 2.),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![Vec2::ZERO; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    let material = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.2, 0.6, 0.3, 1.],
        true,
    );
    let object = renderer.add_instance(
        &mut scene,
        mesh,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![material],
    );
    let mut camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.,
        pitch: 0.,
        roll: 0.,
        fov_deg: 60.,
        near: 0.1,
        far: 1000.,
    };
    println!("Capture GPU: {:?}", renderer.device.adapter_info());
    let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("neural reference main view"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    // Use the main-view API: render_to_image deliberately resets exposure/history
    // for independent stills and therefore cannot capture a temporal sequence.
    renderer.render(
        &mut scene,
        &view,
        width,
        height,
        &camera,
        &Lighting::default(),
    );
    renderer.request_neural_capture(Path::new(&directory), 4)?;
    for index in 0..4 {
        if index == 2 {
            camera.position.x += 0.04;
            camera.yaw += 0.5;
        }
        if index == 3 {
            renderer.set_transform(&mut scene, object, DVec3::new(0.03, 0., 0.), Mat4::IDENTITY);
        }
        renderer.render(
            &mut scene,
            &view,
            width,
            height,
            &camera,
            &Lighting::default(),
        );
    }
    let evidence = renderer.external_neural_diagnostics();
    ensure!(
        evidence.captures == 4 && evidence.evaluations == 0 && evidence.compositions == 0,
        "capture incomplete: {evidence:?}"
    );
    println!(
        "Exported 4 real GPU-rendered inputs to {}; no NVIDIA inference.",
        Path::new(&directory).display()
    );
    Ok(())
}
