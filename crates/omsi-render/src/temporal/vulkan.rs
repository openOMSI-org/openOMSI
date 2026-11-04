//! What both upscalers need of Vulkan beside wgpu: its device and queue, command buffers of our
//! own run on that queue, and each texture's image (and view, and memory, for Streamline).
//!
//! As on Direct3D 12 (`d3d12.rs`) the upscaler records into a command buffer of ours, put on
//! wgpu's queue after the submit that drew the scene and before the one that draws the HUD. On
//! Vulkan the queue's order alone does not make one submit's writes visible to the next: a full
//! memory barrier opens and closes every buffer of ours, and the vendors' own barriers move each
//! image from the layout wgpu left it in and back.

use anyhow::{anyhow, Context, Result};
use ash::vk;

#[derive(Clone)]
pub(super) struct VkGpu {
    pub(super) device: ash::Device,
    pub(super) instance: vk::Instance,
    pub(super) physical: vk::PhysicalDevice,
    pub(super) get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    pub(super) family: u32,
    pub(super) queue_index: u32,
}

impl VkGpu {
    pub(super) fn of(device: &wgpu::Device) -> Option<VkGpu> {
        let hal = unsafe { device.as_hal::<wgpu_hal::api::Vulkan>() }?;
        let instance = hal.shared_instance().raw_instance();
        Some(VkGpu {
            device: hal.raw_device().clone(),
            instance: instance.handle(),
            physical: hal.raw_physical_device(),
            get_device_proc_addr: instance.fp_v1_0().get_device_proc_addr,
            family: hal.queue_family_index(),
            queue_index: hal.queue_index(),
        })
    }
}

pub(super) fn queue(queue: &wgpu::Queue) -> Result<vk::Queue> {
    let hal = unsafe { queue.as_hal::<wgpu_hal::api::Vulkan>() }.ok_or_else(|| anyhow!("no Vulkan queue under wgpu's"))?;
    Ok(hal.as_raw())
}

pub(super) fn image(t: &wgpu::Texture) -> Result<vk::Image> {
    let hal = unsafe { t.as_hal::<wgpu_hal::api::Vulkan>() }.ok_or_else(|| anyhow!("no Vulkan image under an upscaler's texture"))?;
    Ok(unsafe { hal.raw_handle() })
}

pub(super) fn image_and_memory(t: &wgpu::Texture) -> Result<(vk::Image, vk::DeviceMemory)> {
    let hal = unsafe { t.as_hal::<wgpu_hal::api::Vulkan>() }.ok_or_else(|| anyhow!("no Vulkan image under an upscaler's texture"))?;
    let memory = match unsafe { hal.memory() } {
        wgpu_hal::vulkan::TextureMemory::Allocation(a) => unsafe { a.memory() },
        wgpu_hal::vulkan::TextureMemory::Dedicated(m) => *m,
        wgpu_hal::vulkan::TextureMemory::External => vk::DeviceMemory::null(),
    };
    Ok((unsafe { hal.raw_handle() }, memory))
}

pub(super) fn view(v: &wgpu::TextureView) -> Result<vk::ImageView> {
    let hal = unsafe { v.as_hal::<wgpu_hal::api::Vulkan>() }.ok_or_else(|| anyhow!("no Vulkan image view under an upscaler's view"))?;
    Ok(unsafe { hal.raw_handle() })
}

pub(super) fn format(f: wgpu::TextureFormat) -> vk::Format {
    use wgpu::TextureFormat as Tf;
    match f {
        Tf::Bgra8UnormSrgb => vk::Format::B8G8R8A8_SRGB,
        Tf::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
        Tf::Rgba8UnormSrgb => vk::Format::R8G8B8A8_SRGB,
        Tf::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        Tf::Rgb10a2Unorm => vk::Format::A2B10G10R10_UNORM_PACK32,
        Tf::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        Tf::Rg16Float => vk::Format::R16G16_SFLOAT,
        Tf::Rg32Float => vk::Format::R32G32_SFLOAT,
        Tf::R16Float => vk::Format::R16_SFLOAT,
        Tf::Depth32Float => vk::Format::D32_SFLOAT,
        _ => vk::Format::UNDEFINED,
    }
}

pub(super) fn usage(u: wgpu::TextureUsages, depth: bool) -> vk::ImageUsageFlags {
    let mut out = vk::ImageUsageFlags::empty();
    if u.contains(wgpu::TextureUsages::COPY_SRC) {
        out |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
    if u.contains(wgpu::TextureUsages::COPY_DST) {
        out |= vk::ImageUsageFlags::TRANSFER_DST;
    }
    if u.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
        out |= vk::ImageUsageFlags::SAMPLED;
    }
    if u.contains(wgpu::TextureUsages::STORAGE_BINDING) {
        out |= vk::ImageUsageFlags::STORAGE;
    }
    if u.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        out |= if depth { vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT } else { vk::ImageUsageFlags::COLOR_ATTACHMENT };
    }
    out
}

struct Slot {
    buffer: vk::CommandBuffer,
    fence: vk::Fence,
    pending: bool,
}

pub(super) struct VkRing {
    device: ash::Device,
    pool: vk::CommandPool,
    slots: Vec<Slot>,
    next: usize,
    open: Option<usize>,
}

impl VkRing {
    pub(super) fn new(gpu: &VkGpu) -> Result<VkRing> {
        let device = gpu.device.clone();
        let pool = unsafe {
            device.create_command_pool(&vk::CommandPoolCreateInfo::default().queue_family_index(gpu.family).flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER), None)
        }
        .context("creating a command pool")?;
        let made = (|| -> Result<Vec<Slot>> {
            let buffers = unsafe { device.allocate_command_buffers(&vk::CommandBufferAllocateInfo::default().command_pool(pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(3)) }.context("allocating command buffers")?;
            let mut slots = Vec::new();
            for buffer in buffers {
                let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }.context("creating a fence")?;
                slots.push(Slot { buffer, fence, pending: false });
            }
            Ok(slots)
        })();
        match made {
            Ok(slots) => Ok(VkRing { device, pool, slots, next: 0, open: None }),
            Err(e) => {
                unsafe { device.destroy_command_pool(pool, None) };
                Err(e)
            }
        }
    }

    fn full_barrier(&self, buffer: vk::CommandBuffer) {
        let all = vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE;
        let barrier = vk::MemoryBarrier::default().src_access_mask(all).dst_access_mask(all);
        unsafe { self.device.cmd_pipeline_barrier(buffer, vk::PipelineStageFlags::ALL_COMMANDS, vk::PipelineStageFlags::ALL_COMMANDS, vk::DependencyFlags::empty(), &[barrier], &[], &[]) };
    }

    pub(super) fn begin(&mut self) -> Result<vk::CommandBuffer> {
        let i = self.next;
        let slot = &mut self.slots[i];
        if slot.pending {
            unsafe { self.device.wait_for_fences(&[slot.fence], true, u64::MAX) }.context("waiting for a command buffer")?;
            unsafe { self.device.reset_fences(&[slot.fence]) }.context("resetting a fence")?;
            slot.pending = false;
        }
        let buffer = slot.buffer;
        unsafe {
            self.device.reset_command_buffer(buffer, vk::CommandBufferResetFlags::empty())?;
            self.device.begin_command_buffer(buffer, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;
        }
        self.full_barrier(buffer);
        self.open = Some(i);
        Ok(buffer)
    }

    pub(super) fn submit(&mut self, queue: vk::Queue) -> Result<()> {
        let i = self.open.take().ok_or_else(|| anyhow!("no command buffer open"))?;
        self.next = (i + 1) % self.slots.len();
        let slot = &mut self.slots[i];
        let buffer = slot.buffer;
        let all = vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE;
        let barrier = vk::MemoryBarrier::default().src_access_mask(all).dst_access_mask(all);
        unsafe {
            self.device.cmd_pipeline_barrier(buffer, vk::PipelineStageFlags::ALL_COMMANDS, vk::PipelineStageFlags::ALL_COMMANDS, vk::DependencyFlags::empty(), &[barrier], &[], &[]);
            self.device.end_command_buffer(buffer)?;
            let buffers = [buffer];
            self.device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&buffers)], slot.fence).context("submitting to wgpu's queue")?;
        }
        slot.pending = true;
        Ok(())
    }

    pub(super) fn wait(&mut self) {
        let fences: Vec<vk::Fence> = self.slots.iter().filter(|s| s.pending).map(|s| s.fence).collect();
        if !fences.is_empty() && unsafe { self.device.wait_for_fences(&fences, true, u64::MAX) }.is_ok() {
            let _ = unsafe { self.device.reset_fences(&fences) };
            for s in &mut self.slots {
                s.pending = false;
            }
        }
    }
}

impl Drop for VkRing {
    fn drop(&mut self) {
        self.wait();
        unsafe {
            for s in &self.slots {
                self.device.destroy_fence(s.fence, None);
            }
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}
