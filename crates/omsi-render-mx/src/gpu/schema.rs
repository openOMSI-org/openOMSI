//! What the scene's bind groups hold, as data: the bindings the scene shaders declare, with
//! their stages and kinds. mX needs no layouts; these descriptions are kept for what the
//! renderer decides from them (the texture units a GL pipeline takes, see `lamps_fit` and
//! `sixteen_texture_units`, and the tests that hold shaders and bindings to each other).

use std::num::NonZeroU64;

bitflags::bitflags! {
    /// The stages a binding is read in.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub(crate) struct ShaderStages: u32 {
        const VERTEX = 1;
        const FRAGMENT = 2;
        const COMPUTE = 4;
        const VERTEX_FRAGMENT = Self::VERTEX.bits() | Self::FRAGMENT.bits();
    }
}

pub(crate) type BufferSize = NonZeroU64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferBindingType {
    Uniform,
    Storage { read_only: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SamplerBindingType {
    Filtering,
    NonFiltering,
    Comparison,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageTextureAccess {
    WriteOnly,
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingType {
    Buffer {
        ty: BufferBindingType,
        has_dynamic_offset: bool,
        min_binding_size: Option<BufferSize>,
    },
    Sampler(SamplerBindingType),
    Texture {
        sample_type: mx::TextureSampleType,
        view_dimension: mx::TextureViewDimension,
        multisampled: bool,
    },
    StorageTexture {
        access: StorageTextureAccess,
        format: mx::TextureFormat,
        view_dimension: mx::TextureViewDimension,
    },
}

/// One binding of a group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BindGroupLayoutEntry {
    pub binding: u32,
    pub visibility: ShaderStages,
    pub ty: BindingType,
    pub count: Option<std::num::NonZeroU32>,
}
