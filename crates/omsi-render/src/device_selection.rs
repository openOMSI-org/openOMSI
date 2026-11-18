//! GPU array path selection for downlevel/ANGLE devices during renderer creation.
use super::{camera_layout_entries, material_layout_entries, ArrayPath, ARRAY_PATH};

pub(super) fn select_array_path(adapter: &mx::Adapter, info: &mx::AdapterInfo) {
    // the per-draw arrays as storage buffers where the device reads them in a vertex
    // shader (three there, two lights arrays in a fragment shader), else as textures;
    // OMSI_GPU_ARRAYS=textures|nostorage|lighttextures takes those paths on any device
    let (vertex_storage, fragment_storage) = adapter.storage_buffers_per_stage();
    let path = match omsi_cfg::flags::OMSI_GPU_ARRAYS.var() {
        Some("textures") => ArrayPath::VertexTextures,
        Some("nostorage") => ArrayPath::NoStorage,
        Some("lighttextures") => ArrayPath::LightTextures,
        _ if fragment_storage < 2 => without_storage(adapter),
        _ if vertex_storage < 3 => ArrayPath::VertexTextures,
        // (the draw list and the lamps' grid are arrays of u32, 4 bytes, which ANGLE cannot
        // bind as storage buffers: it failed the shadow pipeline with "a size that is a
        // multiple of 16 bytes", #1857)
        _ if super::is_angle(info) => without_storage(adapter),
        _ => ArrayPath::Storage,
    };
    ARRAY_PATH.store(path as u8, std::sync::atomic::Ordering::Relaxed);
    match path {
        ArrayPath::Storage => {}
        ArrayPath::VertexTextures => log::warn!("{}: no storage buffers in vertex shaders; the scene's arrays are read from textures", info.name),
        ArrayPath::NoStorage => log::warn!("{}: no storage buffers; the scene's arrays are read from textures and the lamps light no pixels of their own", info.name),
        ArrayPath::LightTextures => log::info!("{}: no storage buffers; the scene's arrays and its lamps are read from textures", info.name),
    }
}

fn without_storage(adapter: &mx::Adapter) -> ArrayPath {
    if !cfg!(target_os = "android") && lamps_fit(adapter.texture_units()) { ArrayPath::LightTextures } else { ArrayPath::NoStorage }
}

/// Whether the camera and material groups with the lamps in textures fit the texture units
/// (vertex, fragment, together; `mx::Adapter::texture_units`).
pub(crate) fn lamps_fit((vertex, fragment, together): (u32, u32, u32)) -> bool {
    use crate::gpu::schema::{BindingType, ShaderStages};
    let entries = [camera_layout_entries(ArrayPath::LightTextures, true), material_layout_entries()].concat();
    let textures = |stage: Option<ShaderStages>| {
        entries
            .iter()
            .filter(|e| matches!(e.ty, BindingType::Texture { .. }) && stage.is_none_or(|s| e.visibility.contains(s)))
            .count() as u32
    };
    textures(None) <= together
        && textures(Some(ShaderStages::VERTEX)) <= vertex
        && textures(Some(ShaderStages::FRAGMENT)) <= fragment
}
