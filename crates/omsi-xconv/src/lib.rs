//! DirectX `.x` meshes converted to OMSI's `.o3d`, the way OMSI's own "X to O3D Converter"
//! does it (the same eight settings, the same output), for many files at once.
//!
//! * [`xfile`] reads a `.x` file - text or binary - as its frames, meshes and materials;
//! * [`convert`] turns that into the meshes to write, by the converter's settings
//!   ([`Options`]): the frame matrices applied, Z up turned to Y up, normals up, vertices and
//!   materials optimised, the specular removed, one file or one a mesh;
//! * [`o3d`] writes a plain (unscrambled) `.o3d`: version 1 while the counts fit in 16 bits,
//!   version 3 with 32-bit counts beyond that, as the files the original tool wrote.
//!
//! The file format is the public one of `docs/FORMATS.md`; nothing here reads or writes the
//! scrambled vertices of version 4+ files.

pub mod convert;
pub mod o3d;
pub mod xfile;

pub use convert::{convert, Options, Output};
pub use o3d::{O3dMaterial, O3dMesh};
pub use xfile::{parse, XFile};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a .x file (no `xof` header)")]
    NotX,
    #[error("a compressed .x file ({0}): save it uncompressed")]
    Compressed(String),
    #[error("the .x file is damaged: {0} (token {1})")]
    Syntax(String, usize),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// One `.x` file read, converted and written beside it (or into `out_dir`): the files
/// written, with each one's log.
pub fn convert_file(path: &std::path::Path, out_dir: Option<&std::path::Path>, opts: &Options) -> Result<Vec<(std::path::PathBuf, Vec<String>)>, Error> {
    let bytes = std::fs::read(path)?;
    let x = parse(&bytes)?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "mesh".into());
    let dir = out_dir.map(|d| d.to_path_buf()).or_else(|| path.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    let mut written = Vec::new();
    for out in convert(&x, &stem, opts) {
        let target = dir.join(format!("{}.o3d", out.name));
        std::fs::write(&target, o3d::write(&out.mesh))?;
        written.push((target, out.log));
    }
    Ok(written)
}
