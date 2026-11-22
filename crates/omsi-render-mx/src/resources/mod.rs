//! What the scene's draws are made of on the GPU: meshes, textures, materials, and their
//! slots let go of and taken over again.

mod free;
mod materials;
mod meshes;
mod textures;

pub(crate) use free::*;
pub use materials::*;
pub use meshes::*;
pub use textures::*;
