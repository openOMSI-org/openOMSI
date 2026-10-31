//! What `Renderer::build` makes, a feature a module: each makes its pipelines, layouts,
//! buffers and fixed textures and hands them back for the renderer.

pub(crate) mod common;
pub(crate) mod coronas;
pub(crate) mod defaults;
pub(crate) mod enhanced;
pub(crate) mod errors;
pub(crate) mod mip;
pub(crate) mod overlays;
pub(crate) mod parallel;
pub(crate) mod passes;
pub(crate) mod post;
pub(crate) mod scene;
pub(crate) mod shadows;
pub(crate) mod sky;
pub(crate) mod ssao;
pub(crate) mod upscale;

#[cfg(all(test, windows))]
mod dx12_tests;
