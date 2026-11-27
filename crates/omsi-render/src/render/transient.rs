//! Checked transient GPU allocations for captures and readbacks.

use crate::Renderer;

impl Renderer {
    /// Do not draw with an invalid render target if the GPU ran out of memory.
    /// Checked texture creation also reports OOM to the normal recovery path.
    pub fn try_create_transient_texture(
        &self,
        desc: &wgpu::TextureDescriptor<'_>,
    ) -> Option<wgpu::Texture> {
        crate::resources::try_create_texture(&self.device, desc)
    }

    /// Readback buffers need the same error handling as scene mesh buffers.
    pub fn try_create_transient_buffer(
        &self,
        desc: &wgpu::BufferDescriptor<'_>,
    ) -> Option<wgpu::Buffer> {
        crate::resources::try_create_buffer(&self.device, desc)
    }
}
