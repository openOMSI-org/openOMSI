//! Real DX12 queue tests without an OpenXR runtime/headset. The native image
//! starts in RT exactly as xrWaitSwapchainImage promises, then wgpu imports it.

use super::*;
use omsi_render::{RenderOptions, Renderer};
use std::sync::mpsc;
use std::time::Duration;
use windows::Win32::Graphics::Dxgi::Common::*;

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        flags: wgpu::InstanceFlags::DEBUG | wgpu::InstanceFlags::VALIDATION,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8Unorm),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            ..Default::default()
        },
    ))
    .expect("DX12 test renderer")
}

fn image(renderer: &Renderer) -> (ID3D12Resource, wgpu::Texture) {
    let device = unsafe { renderer.device.as_hal::<wgpu_hal::api::Dx12>() }.unwrap();
    let mut resource: Option<ID3D12Resource> = None;
    unsafe {
        device
            .raw_device()
            .CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                    Width: 4,
                    Height: 1,
                    DepthOrArraySize: 2,
                    MipLevels: 1,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
                    Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                None,
                &mut resource,
            )
            .unwrap();
    }
    let resource = resource.unwrap();
    let size = wgpu::Extent3d {
        width: 4,
        height: 1,
        depth_or_array_layers: 2,
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let hal = unsafe {
        wgpu_hal::dx12::Device::texture_from_raw(
            resource.clone(),
            format,
            wgpu::TextureDimension::D2,
            size,
            1,
            1,
        )
    };
    let texture = unsafe {
        renderer
            .device
            .create_texture_from_hal::<wgpu_hal::api::Dx12>(
                hal,
                &wgpu::TextureDescriptor {
                    label: Some("mock OpenXR image"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                },
            )
    };
    (resource, texture)
}

/// Always unblock the GPU, including when an assertion unwinds the test.
struct Gate(ID3D12Fence);
impl Drop for Gate {
    fn drop(&mut self) {
        unsafe {
            self.0.Signal(1).unwrap();
        }
    }
}

fn gated_shutdown(explicit_finish: bool) {
    let renderer = renderer();
    let (resource, _texture) = image(&renderer);
    let mut sync = SwapchainSync::new(&renderer.device, &renderer.queue, 1);
    let device = unsafe { renderer.device.as_hal::<wgpu_hal::api::Dx12>() }.unwrap();
    let fence: ID3D12Fence =
        unsafe { device.raw_device().CreateFence(0, D3D12_FENCE_FLAG_NONE) }.unwrap();
    let gate = Gate(fence);
    let queue = unsafe { renderer.queue.as_hal::<wgpu_hal::api::Dx12>() }.unwrap();
    unsafe { queue.as_raw().Wait(&gate.0, 1) }.unwrap();
    drop(queue);
    drop(device);
    // App's renderer field precedes XR: the guard must own graphics handles
    // even after the application's Renderer has been destroyed.
    drop(renderer);
    let (prepared_tx, prepared_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        sync.prepare_image(0, &resource).unwrap();
        prepared_tx.send(()).unwrap();
        // No wgpu rendering submission: Drop must still fence the raw barrier.
        if explicit_finish {
            sync.finish();
        }
        drop(sync);
        done_tx.send(()).unwrap();
    });
    let prepared = prepared_rx.recv_timeout(Duration::from_secs(5));
    let completed_while_blocked = done_rx.recv_timeout(Duration::from_millis(100));
    drop(gate);
    worker.join().unwrap();
    assert!(prepared.is_ok(), "first use blocked on GPU completion");
    assert!(
        completed_while_blocked.is_err(),
        "shutdown freed in-flight command objects"
    );
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
}

#[test]
#[ignore = "requires a DX12 graphics adapter and DXC runtime DLLs"]
fn first_use_does_not_wait_but_shutdown_waits_for_raw_commands() {
    gated_shutdown(false);
}

#[test]
#[ignore = "requires a DX12 graphics adapter and DXC runtime DLLs"]
fn explicit_finish_waits_after_the_renderer_is_dropped() {
    gated_shutdown(true);
}

fn pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) {
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("mock XR eye/restore RT"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    }));
}

fn assert_no_d3d_errors(renderer: &Renderer) {
    let device = unsafe { renderer.device.as_hal::<wgpu_hal::api::Dx12>() }.unwrap();
    let Ok(info) = device.raw_device().cast::<ID3D12InfoQueue>() else {
        eprintln!("D3D12 debug layer unavailable; pixel/fence checks still run");
        return;
    };
    let mut errors = Vec::new();
    unsafe {
        for index in 0..info.GetNumStoredMessagesAllowedByRetrievalFilter() {
            let mut bytes = 0;
            info.GetMessage(index, None, &mut bytes).unwrap();
            let mut storage = vec![0u64; bytes.div_ceil(8)];
            let message = storage.as_mut_ptr().cast::<D3D12_MESSAGE>();
            info.GetMessage(index, Some(message), &mut bytes).unwrap();
            if (*message).Severity == D3D12_MESSAGE_SEVERITY_ERROR
                || (*message).Severity == D3D12_MESSAGE_SEVERITY_CORRUPTION
            {
                let description = std::ffi::CStr::from_ptr((*message).pDescription.cast());
                errors.push(description.to_string_lossy().into_owned());
            }
        }
    }
    assert!(errors.is_empty(), "D3D12 validation errors: {errors:#?}");
}

fn roundtrip(desktop_mirror: bool) {
    let renderer = renderer();
    let images: Vec<_> = (0..3).map(|_| image(&renderer)).collect();
    let mut sync = SwapchainSync::new(&renderer.device, &renderer.queue, images.len());
    let mirror = super::super::Mirror::new(&renderer);
    let desktop = renderer
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("mock desktop"),
            size: wgpu::Extent3d {
                width: 4,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let readbacks: Vec<_> = (0..3)
        .map(|_| {
            renderer.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mock runtime stereo readback"),
                size: 512,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        })
        .collect();
    // Cycle swapchain images repeatedly, with no CPU wait between frames. Reading
    // both layers on the same queue models the runtime consumer after release.
    for frame in 0..48 {
        let index = frame % images.len();
        let (resource, texture) = &images[index];
        sync.prepare_image(index, resource).unwrap();
        let first_list = sync.initialized[index].as_ref().unwrap().1.as_raw();
        sync.prepare_image(index, resource).unwrap();
        assert_eq!(
            first_list,
            sync.initialized[index].as_ref().unwrap().1.as_raw()
        );
        let eyes = [0, 1].map(|eye| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: eye,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        let colors = if frame % 2 == 0 {
            [wgpu::Color::RED, wgpu::Color::GREEN]
        } else {
            [wgpu::Color::GREEN, wgpu::Color::RED]
        };
        pass(&mut encoder, &eyes[0], wgpu::LoadOp::Clear(colors[0]));
        pass(&mut encoder, &eyes[1], wgpu::LoadOp::Clear(colors[1]));
        renderer.queue.submit([encoder.finish()]);
        if desktop_mirror {
            // Exercise the production sampling + restore-to-RT implementation.
            mirror.draw(&renderer, &eyes[0], &desktop, (4, 1), (4, 1));
        }
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readbacks[index],
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            texture.size(),
        );
        for eye in &eyes {
            pass(&mut encoder, eye, wgpu::LoadOp::Load);
        }
        renderer.queue.submit([encoder.finish()]);
    }
    assert_eq!(
        sync.initialized
            .iter()
            .filter(|slot| slot.is_some())
            .count(),
        3
    );
    // Teardown is the sole CPU completion wait in this simulated frame loop.
    drop(sync);
    for (index, buffer) in readbacks.iter().enumerate() {
        let (tx, rx) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = buffer.slice(..).get_mapped_range();
        let (left, right) = if (45 + index) % 2 == 0 {
            ([255, 0, 0, 255], [0, 255, 0, 255])
        } else {
            ([0, 255, 0, 255], [255, 0, 0, 255])
        };
        assert_eq!(&pixels[0..4], &left, "left eye/runtime ordering");
        assert_eq!(&pixels[256..260], &right, "right eye/runtime ordering");
        drop(pixels);
        buffer.unmap();
    }
    assert_no_d3d_errors(&renderer);
}

#[test]
#[ignore = "requires a DX12 graphics adapter and DXC runtime DLLs"]
fn repeated_stereo_handoff_without_cpu_wait() {
    roundtrip(false);
}

#[test]
#[ignore = "requires a DX12 graphics adapter and DXC runtime DLLs"]
fn repeated_stereo_handoff_with_desktop_mirror() {
    roundtrip(true);
}
