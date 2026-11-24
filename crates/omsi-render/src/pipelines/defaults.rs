//! The material samplers, the camera uniform and the textures a material without its
//! own falls back on.

use super::common::*;
use crate::*;

pub(crate) struct Defaults {
    pub sampler: wgpu::Sampler,
    pub clamp_sampler: wgpu::Sampler,
    pub mirror_sampler: wgpu::Sampler,
    pub camera_buf: wgpu::Buffer,
    pub white_texture: GpuTexture,
    pub black_texture: GpuTexture,
    pub flat_normal_texture: GpuTexture,
}

pub(crate) fn build(device: &wgpu::Device, queue: &wgpu::Queue, anisotropy: u16) -> Defaults {
    let sampler = texture_sampler(device, wgpu::AddressMode::Repeat, anisotropy);
    // [matl_texadress_clamp]: a number plate is a small quad whose texture must not
    // repeat beyond its edge - repeated, the plate text tiled the whole rear of the bus
    let clamp_sampler = texture_sampler(device, wgpu::AddressMode::ClampToEdge, anisotropy);
    let mirror_sampler = texture_sampler(device, wgpu::AddressMode::MirrorRepeat, anisotropy);
    let camera_buf = uniform_buffer(
        device,
        "camera",
        std::mem::size_of::<CameraUniform>() as u64,
    );
    let white = omsi_texture::Image::solid([255, 255, 255, 255]);
    let white_texture = upload_texture(device, queue, &white, false);
    let black_texture = upload_texture(
        device,
        queue,
        &omsi_texture::Image::solid([0, 0, 0, 255]),
        false,
    );
    let flat_normal_texture = upload_texture(
        device,
        queue,
        &omsi_texture::Image::solid([128, 128, 255, 255]),
        false,
    );
    Defaults {
        sampler,
        clamp_sampler,
        mirror_sampler,
        camera_buf,
        white_texture,
        black_texture,
        flat_normal_texture,
    }
}
