//! D3D12 queue ordering handles frame handoff; CPU completion is only needed
//! before destroying native command objects and the OpenXR swapchain.
//!
//! OpenXR's D3D12 image-state contract hands off access on the bound queue:
//! https://registry.khronos.org/OpenXR/specs/1.1/html/xrspec.html#khr_d3d12_enable-state
//! xrDestroySwapchain separately requires completed graphics commands.

use anyhow::{Result, anyhow};
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

/// One immutable first-use command list per image, retained for the session.
/// This bounded storage avoids retiring/resetting an allocator still on the GPU.
pub(super) struct SwapchainSync {
    device: wgpu::Device,
    queue: wgpu::Queue,
    initialized: Vec<Option<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)>>,
    finished: bool,
}

impl SwapchainSync {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue, images: usize) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            initialized: vec![None; images],
            finished: false,
        }
    }

    /// Call only after OpenXR has granted write access on the bound queue.
    /// The imported wgpu tracker starts in COMMON; actual XR images start in RT.
    pub(super) fn prepare_image(&mut self, index: usize, resource: &ID3D12Resource) -> Result<()> {
        self.finished = false;
        if self.initialized[index].is_none() {
            self.initialized[index] =
                Some(transition_first_use(&self.device, &self.queue, resource)?);
        }
        Ok(())
    }

    /// Vr calls this before destroying XR objects, retaining this guard's device
    /// and queue until afterward. Drop also covers early exits in standalone use.
    pub(super) fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        // Include raw first-use barriers even if rendering exits before its first
        // wgpu submit. An empty submission signals wgpu's fence on the same queue.
        // This wait must finish BEFORE destroying the OpenXR swapchain/session.
        let submitted = self.queue.submit([]);
        if let Err(error) = self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        }) {
            log::error!("OpenXR shutdown GPU wait failed: {error}");
        }
    }
}

impl Drop for SwapchainSync {
    fn drop(&mut self) {
        self.finish();
    }
}

fn transition_first_use(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resource: &ID3D12Resource,
) -> Result<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)> {
    let device = unsafe { device.as_hal::<wgpu_hal::api::Dx12>() }
        .ok_or_else(|| anyhow!("OpenXR lost the DX12 graphics device"))?;
    let queue = unsafe { queue.as_hal::<wgpu_hal::api::Dx12>() }
        .ok_or_else(|| anyhow!("OpenXR lost the DX12 command queue"))?;
    // Submit on wgpu's own queue so its subsequent COMMON -> RENDER_TARGET
    // transition executes after this barrier. Keep the command objects alive
    // until the swapchain's shutdown wait completes.
    let allocator: ID3D12CommandAllocator = unsafe {
        device
            .raw_device()
            .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
    }?;
    let list: ID3D12GraphicsCommandList = unsafe {
        device.raw_device().CreateCommandList(
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            &allocator,
            None::<&ID3D12PipelineState>,
        )
    }?;
    let mut barrier = D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: std::mem::ManuallyDrop::new(Some(resource.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: D3D12_RESOURCE_STATE_RENDER_TARGET,
                StateAfter: D3D12_RESOURCE_STATE_COMMON,
            }),
        },
    };
    unsafe {
        list.ResourceBarrier(std::slice::from_ref(&barrier));
        std::mem::ManuallyDrop::drop(&mut (*barrier.Anonymous.Transition).pResource);
        list.Close()?;
        let command_list: ID3D12CommandList = list.cast()?;
        queue.as_raw().ExecuteCommandLists(&[Some(command_list)]);
    }
    Ok((allocator, list))
}

#[cfg(test)]
mod tests;
