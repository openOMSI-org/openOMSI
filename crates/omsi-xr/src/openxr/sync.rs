//! The swapchain images' states between OpenXR and mX, on the one D3D12 queue they share.
//!
//! OpenXR hands a color image over in `RENDER_TARGET` at `xrWaitSwapchainImage` and takes it
//! back in `RENDER_TARGET` at `xrReleaseSwapchainImage`; mX keeps every texture in its home
//! state between its commands (`ALL_SHADER_RESOURCE` for one shaders read, else `COMMON`). Two
//! command lists per image, recorded once, move it from one to the other on mX's queue: after
//! the wait and before the release. D3D12 queue order does the rest; the CPU waits only before
//! the swapchain and the session are destroyed.
//!
//! OpenXR's D3D12 image-state contract:
//! https://registry.khronos.org/OpenXR/specs/1.1/html/xrspec.html#khr_d3d12_enable-state
//! xrDestroySwapchain separately requires completed graphics commands.

use anyhow::{Result, anyhow};
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

/// An image's two transitions: from OpenXR's state to mX's, and back.
struct Handover {
    _allocator: ID3D12CommandAllocator,
    to_mx: ID3D12CommandList,
    to_xr: ID3D12CommandList,
}

/// The lists of every image, kept for the session (never reset while the GPU may run them).
pub(super) struct SwapchainSync {
    device: mx::Device,
    queue: mx::Queue,
    images: Vec<Option<Handover>>,
    finished: bool,
}

impl SwapchainSync {
    pub(super) fn new(device: &mx::Device, queue: &mx::Queue, images: usize) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            images: (0..images).map(|_| None).collect(),
            finished: false,
        }
    }

    /// After `xrWaitSwapchainImage`: image `index` (`texture`, imported into mX) from
    /// `RENDER_TARGET` to mX's home state, before mX's commands on it.
    pub(super) fn prepare_image(&mut self, index: usize, texture: &mx::Texture) -> Result<()> {
        self.finished = false;
        if self.images[index].is_none() {
            self.images[index] = Some(record(&self.device, texture)?);
        }
        let h = self.images[index].as_ref().expect("made above");
        self.execute(&h.to_mx)
    }

    /// Before `xrReleaseSwapchainImage`: image `index` back to `RENDER_TARGET`, after mX's
    /// commands on it (all submitted by now).
    pub(super) fn release_image(&mut self, index: usize) -> Result<()> {
        let h = self.images[index]
            .as_ref()
            .ok_or_else(|| anyhow!("an OpenXR image released before it was prepared"))?;
        self.execute(&h.to_xr)
    }

    fn execute(&self, list: &ID3D12CommandList) -> Result<()> {
        let queue = self
            .queue
            .as_dx12()
            .ok_or_else(|| anyhow!("OpenXR lost the DX12 command queue"))?;
        unsafe { queue.raw().ExecuteCommandLists(&[Some(list.clone())]) };
        Ok(())
    }

    /// Vr calls this before destroying XR objects, retaining this guard's device
    /// and queue until afterward. Drop also covers early exits in standalone use.
    pub(super) fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        // (an empty submission signals mX's fence after every list above on the same queue;
        // this wait must finish BEFORE the OpenXR swapchain and session are destroyed)
        let submitted = self.queue.submit([]);
        if let Err(error) = self.device.wait(submitted) {
            log::error!("OpenXR shutdown GPU wait failed: {error}");
        }
    }
}

impl Drop for SwapchainSync {
    fn drop(&mut self) {
        self.finish();
    }
}

/// `texture`'s two transitions, recorded and closed.
fn record(device: &mx::Device, texture: &mx::Texture) -> Result<Handover> {
    let device = device
        .as_dx12()
        .ok_or_else(|| anyhow!("OpenXR lost the DX12 graphics device"))?;
    let texture = texture
        .as_dx12()
        .ok_or_else(|| anyhow!("an OpenXR image that is not mX's DX12 texture"))?;
    let home = texture.rest_state();
    let allocator: ID3D12CommandAllocator = unsafe {
        device
            .raw()
            .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
    }?;
    let list = |before: D3D12_RESOURCE_STATES,
                after: D3D12_RESOURCE_STATES|
     -> Result<ID3D12CommandList> {
        let list: ID3D12GraphicsCommandList = unsafe {
            device.raw().CreateCommandList(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                &allocator,
                None::<&ID3D12PipelineState>,
            )
        }?;
        if before != after {
            let mut barrier = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: std::mem::ManuallyDrop::new(Some(texture.raw().clone())),
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: before,
                        StateAfter: after,
                    }),
                },
            };
            unsafe {
                list.ResourceBarrier(std::slice::from_ref(&barrier));
                std::mem::ManuallyDrop::drop(&mut (*barrier.Anonymous.Transition).pResource);
            }
        }
        unsafe { list.Close() }?;
        Ok(list.cast()?)
    };
    let to_mx = list(D3D12_RESOURCE_STATE_RENDER_TARGET, home)?;
    let to_xr = list(home, D3D12_RESOURCE_STATE_RENDER_TARGET)?;
    Ok(Handover {
        _allocator: allocator,
        to_mx,
        to_xr,
    })
}

#[cfg(test)]
mod tests;
