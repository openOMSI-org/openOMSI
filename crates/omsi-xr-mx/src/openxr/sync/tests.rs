//! Real DX12 queue tests without an OpenXR runtime/headset. The native image
//! starts in RT exactly as xrWaitSwapchainImage promises, then mX imports it.

use super::*;
use omsi_render::{RenderOptions, Renderer};
use std::sync::mpsc;
use std::time::Duration;
use windows::Win32::Graphics::Dxgi::Common::*;

fn renderer() -> Renderer {
    let instance = mx::Instance::with_backends(mx::Backends::DX12);
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(mx::TextureFormat::Rgba8Unorm),
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

fn image(renderer: &Renderer) -> (ID3D12Resource, mx::Texture) {
    let device = renderer.device.as_dx12().unwrap();
    let mut resource: Option<ID3D12Resource> = None;
    unsafe {
        device
            .raw()
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
                // (as xrWaitSwapchainImage hands a colour image over)
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                None,
                &mut resource,
            )
            .unwrap();
    }
    let resource = resource.unwrap();
    let desc = mx::TextureDescriptor {
        label: Some("mock OpenXR image"),
        size: mx::Extent3d {
            width: 4,
            height: 1,
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: mx::TextureDimension::D2,
        format: mx::TextureFormat::Rgba8Unorm,
        usage: mx::TextureUsages::RENDER_ATTACHMENT
            | mx::TextureUsages::TEXTURE_BINDING
            | mx::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    // SAFETY: the resource made above, of this device, described as it is
    let texture = mx::Texture::Dx12(unsafe { device.texture_from_raw(resource.clone(), &desc) });
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
    let (_resource, texture) = image(&renderer);
    let mut sync = SwapchainSync::new(&renderer.device, &renderer.queue, 1);
    let fence: ID3D12Fence = unsafe {
        renderer
            .device
            .as_dx12()
            .unwrap()
            .raw()
            .CreateFence(0, D3D12_FENCE_FLAG_NONE)
    }
    .unwrap();
    let gate = Gate(fence);
    unsafe { renderer.queue.as_dx12().unwrap().raw().Wait(&gate.0, 1) }.unwrap();
    // App's renderer field precedes XR: the guard must own graphics handles
    // even after the application's Renderer has been destroyed.
    drop(renderer);
    let (prepared_tx, prepared_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        sync.prepare_image(0, &texture).unwrap();
        sync.release_image(0).unwrap();
        prepared_tx.send(()).unwrap();
        // No rendering submission: Drop must still wait for the handover lists.
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
    assert!(prepared.is_ok(), "the handover blocked on GPU completion");
    assert!(
        completed_while_blocked.is_err(),
        "shutdown freed in-flight command objects"
    );
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
}

#[test]
#[ignore = "requires a DX12 graphics adapter and DXC runtime DLLs"]
fn the_handover_does_not_wait_but_shutdown_waits_for_its_commands() {
    gated_shutdown(false);
}

#[test]
#[ignore = "requires a DX12 graphics adapter and DXC runtime DLLs"]
fn explicit_finish_waits_after_the_renderer_is_dropped() {
    gated_shutdown(true);
}

fn pass(encoder: &mut mx::CommandEncoder, view: &mx::TextureView, load: mx::LoadOp<mx::Color>) {
    drop(encoder.begin_render_pass(&mx::RenderPassDescriptor {
        label: Some("mock XR eye"),
        color_attachments: &[Some(mx::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: mx::Operations {
                load,
                store: mx::StoreOp::Store,
            },
        })],
        ..Default::default()
    }));
}

fn assert_no_d3d_errors(renderer: &Renderer) {
    let Ok(info) = renderer
        .device
        .as_dx12()
        .unwrap()
        .raw()
        .cast::<ID3D12InfoQueue>()
    else {
        eprintln!("D3D12 debug layer unavailable; pixel/fence checks still run");
        return;
    };
    let mut errors = Vec::new();
    unsafe {
        for index in 0..info.GetNumStoredMessagesAllowedByRetrievalFilter() {
            let mut bytes: usize = 0;
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

const RED: mx::Color = mx::Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const GREEN: mx::Color = mx::Color {
    r: 0.0,
    g: 1.0,
    b: 0.0,
    a: 1.0,
};

fn roundtrip(desktop_mirror: bool) {
    let renderer = renderer();
    let images: Vec<_> = (0..3).map(|_| image(&renderer)).collect();
    let mut sync = SwapchainSync::new(&renderer.device, &renderer.queue, images.len());
    let mirror = super::super::Mirror::new(&renderer);
    let desktop = renderer
        .device
        .create_texture(&mx::TextureDescriptor {
            label: Some("mock desktop"),
            size: mx::Extent3d {
                width: 4,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: mx::TextureDimension::D2,
            format: renderer.format(),
            usage: mx::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .expect("mX: a texture")
        .create_view(&Default::default())
        .expect("mX: a texture view");
    let readbacks: Vec<_> = (0..3)
        .map(|_| {
            renderer
                .device
                .create_buffer(&mx::BufferDescriptor {
                    label: Some("mock runtime stereo readback"),
                    size: 512,
                    usage: mx::BufferUsages::COPY_DST | mx::BufferUsages::MAP_READ,
                })
                .expect("mX: a buffer")
        })
        .collect();
    // Cycle swapchain images repeatedly, with no CPU wait between frames: wait, draw both eyes
    // (and the desktop mirror from the left one), read both layers as the runtime would, release.
    let mut lists = vec![None; images.len()];
    for frame in 0..48 {
        let index = frame % images.len();
        let (_, texture) = &images[index];
        sync.prepare_image(index, texture).unwrap();
        // (each image's handover lists are recorded once and kept)
        let list = sync.images[index].as_ref().unwrap().to_mx.as_raw();
        assert_eq!(*lists[index].get_or_insert(list), list);
        let eyes = [0, 1].map(|eye| {
            texture
                .create_view(&mx::TextureViewDescriptor {
                    dimension: Some(mx::TextureViewDimension::D2),
                    base_array_layer: eye,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
                .expect("mX: a texture view")
        });
        let mut encoder = renderer.device.create_command_encoder(None);
        let colors = if frame % 2 == 0 {
            [RED, GREEN]
        } else {
            [GREEN, RED]
        };
        pass(&mut encoder, &eyes[0], mx::LoadOp::Clear(colors[0]));
        pass(&mut encoder, &eyes[1], mx::LoadOp::Clear(colors[1]));
        renderer.queue.submit([encoder.finish()]);
        if desktop_mirror {
            mirror.draw(&renderer, &eyes[0], &desktop, (4, 1), (4, 1));
        }
        let mut encoder = renderer.device.create_command_encoder(None);
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            mx::TexelCopyBufferInfo {
                buffer: &readbacks[index],
                layout: mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            texture.size(),
        );
        renderer.queue.submit([encoder.finish()]);
        sync.release_image(index).unwrap();
    }
    // Teardown is the sole CPU completion wait in this simulated frame loop.
    drop(sync);
    renderer.device.wait_idle().unwrap();
    for (index, buffer) in readbacks.iter().enumerate() {
        let (left, right) = if (45 + index) % 2 == 0 {
            ([255, 0, 0, 255], [0, 255, 0, 255])
        } else {
            ([0, 255, 0, 255], [255, 0, 0, 255])
        };
        buffer
            .map_read_now(0..buffer.size(), |pixels| {
                assert_eq!(&pixels[0..4], &left, "left eye/runtime ordering");
                assert_eq!(&pixels[256..260], &right, "right eye/runtime ordering");
            })
            .unwrap();
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
