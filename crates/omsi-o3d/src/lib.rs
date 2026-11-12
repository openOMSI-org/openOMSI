//! OMSI mesh files.
//!
//! * `.o3d` - OMSI's own binary format (unit `mc_o3dfiles`), see `docs/FORMATS.md`.
//! * `.x`   - DirectX text meshes, still used by a handful of stock objects and the helper meshes.
//! * `.glb` / `.gltf`, `.fbx`, `.obj` - the formats of today's modelling tools and asset
//!   libraries (see [`import`]): openOMSI reads them wherever OMSI reads a mesh, with their
//!   textures (embedded ones too), skins and blend shapes.
//!
//! All of them produce the same [`Mesh`]. Coordinates are OMSI's: X right, Y forward, Z up, metres.

use glam::{Mat4, Vec2, Vec3};
use std::path::Path;

pub mod fbx;
pub mod gltf;
pub mod import;
pub mod obj;
pub mod xfile;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    pub indices: [u32; 3],
    pub material: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub diffuse: [f32; 4],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub specular_power: f32,
    /// Texture file name, relative to the model's texture directory. Empty = untextured.
    pub texture: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoneWeight {
    pub vertex: u32,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub weights: Vec<BoneWeight>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
    /// Object transform. Identity when the file has none.
    pub transform: Mat4,
    /// The file gave the transform (an `.o3d` matrix section); an `.x` file or an `.o3d`
    /// without one has the identity, which says nothing about how it was exported.
    pub has_transform: bool,
    pub bones: Vec<Bone>,
    pub version: u8,
    /// The skeleton of a skinned import (glTF, FBX): every joint with its parent and its
    /// rest position in the mesh frame. An `.o3d` names its bones only.
    pub joints: Vec<Joint>,
    /// Blend shapes (morph targets) of an import: offsets of some vertices.
    pub morphs: Vec<Morph>,
}

/// A joint of a skeleton (see [`Mesh::joints`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    pub position: Vec3,
    pub rotation: glam::Quat,
}

/// A blend shape: what it moves and how far at full weight, in the mesh frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Morph {
    pub name: String,
    /// (vertex, position offset, normal offset)
    pub offsets: Vec<(u32, Vec3, Vec3)>,
}

impl Default for Material {
    fn default() -> Self {
        Self { diffuse: [1.0; 4], specular: [0.0; 3], emissive: [0.0; 3], specular_power: 1.0, texture: String::new() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum O3dError {
    #[error("not an o3d file (bad magic)")]
    BadMagic,
    #[error("unexpected end of file at offset {0}")]
    Eof(usize),
    #[error("unknown section tag 0x{tag:02x} at offset {offset}")]
    BadTag { tag: u8, offset: usize },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(".x parse error: {0}")]
    XFile(String),
    #[error("{0}")]
    Import(String),
}

struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn need(&self, n: usize) -> Result<(), O3dError> {
        if self.p + n > self.b.len() {
            Err(O3dError::Eof(self.p))
        } else {
            Ok(())
        }
    }
    fn u8(&mut self) -> Result<u8, O3dError> {
        self.need(1)?;
        let v = self.b[self.p];
        self.p += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, O3dError> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.b[self.p], self.b[self.p + 1]]);
        self.p += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, O3dError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.b[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        Ok(v)
    }
    fn f32(&mut self) -> Result<f32, O3dError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn str8(&mut self) -> Result<String, O3dError> {
        let n = self.u8()? as usize;
        self.need(n)?;
        let (s, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&self.b[self.p..self.p + n]);
        self.p += n;
        Ok(s.into_owned())
    }
}

/// Parse an `.o3d` file from memory.
pub fn parse_o3d(bytes: &[u8]) -> Result<Mesh, O3dError> {
    let mut c = Cur { b: bytes, p: 0 };
    if c.u8()? != 0x84 || c.u8()? != 0x19 {
        return Err(O3dError::BadMagic);
    }
    let version = c.u8()?;
    let mut long_indices = false;
    let mut alt = false;
    if version >= 3 {
        let flags = c.u8()?;
        long_indices = flags & 1 != 0;
        alt = flags & 2 != 0;
    }
    let mut key = 0u32;
    if version >= 4 {
        key = c.u32()?;
    }
    // Files of version 4+ carry a key; unless it is 0xFFFFFFFF the vertex records are
    // scrambled (see `unscramble`). The original additionally refuses keys it does not know;
    // that check is deliberately not reproduced.
    let scrambled = version >= 4 && key != 0xFFFF_FFFF;
    let mut state: u16 = if scrambled {
        let k = key as i64 + (version as i64 - 4) + if alt { 0x17D } else { 0 };
        (k.rem_euclid(0xFDE8)) as u16
    } else {
        0
    };
    let wide_count = version >= 3;
    let mut mesh = Mesh { transform: Mat4::IDENTITY, version, ..Default::default() };

    while c.p < bytes.len() {
        let tag = c.u8()?;
        match tag {
            0x17 => {
                let n = if wide_count { c.u32()? as usize } else { c.u16()? as usize };
                c.need(n * 32)?;
                mesh.vertices.reserve(n);
                let n16 = (n as i64 % 0xFDE8) as u32;
                let mut fb: u32 = 0;
                for _ in 0..n {
                    let mut position = Vec3::new(c.f32()?, c.f32()?, c.f32()?);
                    let mut normal = Vec3::new(c.f32()?, c.f32()?, c.f32()?);
                    let mut uv = Vec2::new(c.f32()?, c.f32()?);
                    if scrambled {
                        if key == 0 {
                            state = if alt { 0x130 } else { 0 };
                        }
                        state = ((state as u32 * n16 + n16 * fb) % 8000) as u16;
                        fb = frac_byte(position);
                        unscramble(state, &mut position, &mut normal, &mut uv);
                    }
                    mesh.vertices.push(Vertex { position, normal, uv });
                }
            }
            0x49 => {
                let n = if wide_count { c.u32()? as usize } else { c.u16()? as usize };
                let isz = if long_indices { 4 } else { 2 };
                c.need(n * (3 * isz + 2))?;
                mesh.triangles.reserve(n);
                for _ in 0..n {
                    let mut idx = [0u32; 3];
                    for i in idx.iter_mut() {
                        *i = if long_indices { c.u32()? } else { c.u16()? as u32 };
                    }
                    let material = c.u16()?;
                    mesh.triangles.push(Triangle { indices: idx, material });
                }
            }
            0x26 => {
                let n = c.u16()? as usize;
                for _ in 0..n {
                    let diffuse = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
                    let specular = [c.f32()?, c.f32()?, c.f32()?];
                    let emissive = [c.f32()?, c.f32()?, c.f32()?];
                    let specular_power = c.f32()?;
                    let texture = c.str8()?;
                    mesh.materials.push(Material { diffuse, specular, emissive, specular_power, texture });
                }
            }
            0x79 => {
                let mut m = [0f32; 16];
                for v in m.iter_mut() {
                    *v = c.f32()?;
                }
                // File stores rows; row 3 is the translation (D3D convention, row vectors).
                mesh.transform = Mat4::from_cols_array(&m);
                mesh.has_transform = true;
            }
            0x54 => {
                let n = c.u16()? as usize;
                for _ in 0..n {
                    let name = c.str8()?;
                    let w = c.u16()? as usize;
                    let mut weights = Vec::with_capacity(w);
                    for _ in 0..w {
                        let vertex = if long_indices { c.u32()? } else { c.u16()? as u32 };
                        let weight = c.f32()?;
                        weights.push(BoneWeight { vertex, weight });
                    }
                    mesh.bones.push(Bone { name, weights });
                }
            }
            // any other byte is passed over, one at a time, as the exe's loader does
            // (`mc_o3dfile`: the tag `case` has no else, the loop reads the next byte).
            // Protected v7 files (flags 3) have three junk bytes inside the transform and
            // a few more after it; refusing them lost the whole body of the MB Sprinter
            // 412D XLWB and 49 other meshes of that pack.
            _ => {}
        }
    }
    mesh.drop_bad_triangles();
    Ok(mesh)
}

/// Mixing byte derived from the raw (still scrambled) position of a vertex; feeds the state
/// of the *next* vertex.
fn frac_byte(p: Vec3) -> u32 {
    let f = |x: f32| (x as f64) - (x as f64).trunc();
    let prod = (f(p.x) * f(p.y) * f(p.z)).abs() * 600.0;
    (prod.trunc() as i64).rem_euclid(256) as u32
}

/// Undo the per-vertex scrambling of version 4+ files.
fn unscramble(s: u16, p: &mut Vec3, n: &mut Vec3, uv: &mut Vec2) {
    if s < 1000 {
        std::mem::swap(&mut p.x, &mut p.y);
    } else if s < 3000 {
        std::mem::swap(&mut p.x, &mut p.z);
    } else if s > 7000 {
        std::mem::swap(&mut p.y, &mut p.z);
    }
    if s & 3 == 0 {
        n.x = -n.x;
    }
    if s % 6 == 0 {
        n.y = -n.y;
    }
    if s % 7 == 0 {
        n.z = -n.z;
    }
    // The `s > 6500` branch can never run (every such state already took `s > 4500`), so the
    // x/z normal swap never happens. The exe has the same dead test in both directions: the
    // loader (`mc_o3dfile`, 0x56c74c) and the writer (0x56af20) check `s > 6500` only inside
    // `s <= 4500`. Kept as is so files decode exactly as OMSI decodes them.
    if s < 600 {
        std::mem::swap(&mut n.y, &mut n.z);
    } else if s > 4500 {
        std::mem::swap(&mut n.x, &mut n.y);
    } else if s > 6500 {
        std::mem::swap(&mut n.x, &mut n.z);
    }
    if s % 5 == 0 {
        let m = (s % 100) as f32;
        uv.x -= m * m / 10000.0;
    }
    if s % 3 == 0 {
        let m = (s % 50) as f32;
        uv.y -= m * m / 2500.0;
    }
}

/// Load a mesh by file name, choosing the parser by content (the formats' own signatures)
/// and else by extension: a model renamed by hand reads as what it is.
pub fn load_mesh(path: &Path) -> Result<Mesh, O3dError> {
    let bytes = omsi_cfg::vfs::read(path)?;
    parse_mesh(&bytes, path)
}

/// [`load_mesh`] of a file's bytes; `path` names it (relative textures are looked for
/// beside it, embedded ones are put there in memory).
pub fn parse_mesh(bytes: &[u8], path: &Path) -> Result<Mesh, O3dError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if bytes.starts_with(b"glTF") {
        return gltf::parse_glb_or_gltf(bytes, path);
    }
    if bytes.starts_with(b"Kaydara FBX Binary") || (ext == "fbx" && fbx::looks_like_ascii_fbx(bytes)) {
        return fbx::parse_fbx(bytes, path);
    }
    if bytes.starts_with(b"xof ") {
        return xfile::parse_x(bytes);
    }
    if bytes.first() == Some(&0x84) && bytes.get(1) == Some(&0x19) {
        return parse_o3d(bytes);
    }
    match ext.as_str() {
        "x" => xfile::parse_x(bytes),
        "gltf" | "glb" => gltf::parse_glb_or_gltf(bytes, path),
        "fbx" => fbx::parse_fbx(bytes, path),
        "obj" => obj::parse_obj(bytes, path),
        _ => parse_o3d(bytes),
    }
}

/// The mesh file formats openOMSI reads, by extension.
pub const MESH_EXTENSIONS: [&str; 6] = ["o3d", "x", "glb", "gltf", "fbx", "obj"];

impl Mesh {
    /// Leave out triangles naming a vertex the file does not have (a damaged or hand-made
    /// file; every user of the mesh indexes the vertices with them).
    pub fn drop_bad_triangles(&mut self) {
        let n = self.vertices.len() as u32;
        let before = self.triangles.len();
        self.triangles.retain(|t| t.indices.iter().all(|&i| i < n));
        if self.triangles.len() != before {
            log::warn!("mesh: {} triangles name vertices beyond the {n} there are; left out", before - self.triangles.len());
        }
    }

    /// Axis-aligned bounds of the untransformed vertices.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for v in &self.vertices {
            lo = lo.min(v.position);
            hi = hi.max(v.position);
        }
        if self.vertices.is_empty() {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            (lo, hi)
        }
    }

    /// Translation part of the object transform (D3D row-vector convention: 4th row).
    pub fn origin(&self) -> Vec3 {
        let r = self.transform.col(3);
        // `from_cols_array` interpreted the 16 floats column-wise; the file is row-major with
        // the translation in elements 12..15, which land in column 3. Good enough for the
        // translation; full transform users should call `transform_row_major()`.
        Vec3::new(r.x, r.y, r.z)
    }

    /// The transform as a column-vector matrix usable with glam (`M * v`).
    pub fn transform_row_major(&self) -> Mat4 {
        self.transform.transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Little-endian byte writer for hand-made `.o3d` files.
    #[derive(Default)]
    struct W(Vec<u8>);

    impl W {
        fn u8(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
        fn u16(&mut self, v: u16) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn u32(&mut self, v: u32) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn f32(&mut self, v: f32) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn str8(&mut self, s: &str) -> &mut Self {
            self.u8(s.len() as u8);
            self.0.extend_from_slice(s.as_bytes());
            self
        }
        /// A count: u16 before version 3, u32 from version 3 on.
        fn count(&mut self, version: u8, n: usize) -> &mut Self {
            if version >= 3 {
                self.u32(n as u32)
            } else {
                self.u16(n as u16)
            }
        }
        fn vertex(&mut self, v: &Vertex) -> &mut Self {
            for f in [v.position.x, v.position.y, v.position.z, v.normal.x, v.normal.y, v.normal.z, v.uv.x, v.uv.y] {
                self.f32(f);
            }
            self
        }
    }

    /// Header: magic, version, flags (v3+), key (v4+).
    fn header(version: u8, flags: u8, key: u32) -> W {
        let mut w = W::default();
        w.u8(0x84).u8(0x19).u8(version);
        if version >= 3 {
            w.u8(flags);
        }
        if version >= 4 {
            w.u32(key);
        }
        w
    }

    fn sample_vertices() -> Vec<Vertex> {
        vec![
            Vertex { position: Vec3::new(1.25, -2.5, 3.75), normal: Vec3::new(0.0, 0.6, 0.8), uv: Vec2::new(0.1, 0.9) },
            Vertex { position: Vec3::new(10.3, 4.7, -0.9), normal: Vec3::new(1.0, 0.0, 0.0), uv: Vec2::new(0.5, 0.25) },
            Vertex { position: Vec3::new(-7.11, 0.33, 2.2), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::new(0.75, 0.0) },
        ]
    }

    /// One triangle, one material, one transform, one bone: the whole body of a simple file.
    fn body(w: &mut W, version: u8, long_indices: bool, vertices: &[Vertex]) {
        w.u8(0x17).count(version, vertices.len());
        for v in vertices {
            w.vertex(v);
        }
        w.u8(0x49).count(version, 1);
        for i in [0u32, 1, 2] {
            if long_indices {
                w.u32(i);
            } else {
                w.u16(i as u16);
            }
        }
        w.u16(0);
        w.u8(0x26).u16(1);
        for f in [0.5, 0.6, 0.7, 1.0, 0.1, 0.2, 0.3, 0.0, 0.0, 0.0, 12.0] {
            w.f32(f);
        }
        w.str8("bus.bmp");
        w.u8(0x79);
        for i in 0..16 {
            w.f32(if i % 5 == 0 { 1.0 } else if i == 12 { 4.0 } else { 0.0 });
        }
        w.u8(0x54).u16(1).str8("bone").u16(1);
        if long_indices {
            w.u32(2);
        } else {
            w.u16(2);
        }
        w.f32(0.5);
    }

    fn check_body(m: &Mesh, vertices: &[Vertex]) {
        assert_eq!(m.vertices, vertices);
        assert_eq!(m.triangles, vec![Triangle { indices: [0, 1, 2], material: 0 }]);
        assert_eq!(m.materials.len(), 1);
        assert_eq!(m.materials[0].diffuse, [0.5, 0.6, 0.7, 1.0]);
        assert_eq!(m.materials[0].specular, [0.1, 0.2, 0.3]);
        assert_eq!(m.materials[0].specular_power, 12.0);
        assert_eq!(m.materials[0].texture, "bus.bmp");
        assert!(m.has_transform);
        assert_eq!(m.origin(), Vec3::new(4.0, 0.0, 0.0));
        assert_eq!(m.bones, vec![Bone { name: "bone".into(), weights: vec![BoneWeight { vertex: 2, weight: 0.5 }] }]);
    }

    #[test]
    fn header_versions_1_to_3() {
        for version in [1u8, 2] {
            let mut w = header(version, 0, 0);
            body(&mut w, version, false, &sample_vertices());
            let m = parse_o3d(&w.0).unwrap();
            assert_eq!(m.version, version);
            check_body(&m, &sample_vertices());
        }
        // version 3: flags byte, u32 counts; flag 1 = 32-bit indices
        for long in [false, true] {
            let mut w = header(3, long as u8, 0);
            body(&mut w, 3, long, &sample_vertices());
            check_body(&parse_o3d(&w.0).unwrap(), &sample_vertices());
        }
    }

    #[test]
    fn version_4_with_open_key_is_not_scrambled() {
        for version in [4u8, 5, 7] {
            let mut w = header(version, 0, 0xFFFF_FFFF);
            body(&mut w, version, false, &sample_vertices());
            let m = parse_o3d(&w.0).unwrap();
            assert_eq!(m.version, version);
            check_body(&m, &sample_vertices());
        }
    }

    #[test]
    fn empty_body_and_missing_transform() {
        let m = parse_o3d(&header(3, 0, 0).0).unwrap();
        assert!(m.vertices.is_empty() && m.triangles.is_empty());
        assert!(!m.has_transform);
        assert_eq!(m.transform, Mat4::IDENTITY);
        assert_eq!(m.bounds(), (Vec3::ZERO, Vec3::ZERO));
    }

    #[test]
    fn bad_magic() {
        assert!(matches!(parse_o3d(&[0x84, 0x18, 3, 0]), Err(O3dError::BadMagic)));
        assert!(matches!(parse_o3d(b"xof 0303txt"), Err(O3dError::BadMagic)));
    }

    #[test]
    fn truncated_files_are_errors() {
        // header cut short
        assert!(matches!(parse_o3d(&[]), Err(O3dError::Eof(0))));
        assert!(matches!(parse_o3d(&[0x84, 0x19]), Err(O3dError::Eof(2))));
        assert!(matches!(parse_o3d(&[0x84, 0x19, 4, 0, 0xFF, 0xFF]), Err(O3dError::Eof(4))));
        // every cut inside a section is an error, never a panic or a silently shorter section;
        // only a cut right before a section tag leaves a (shorter) valid file
        let mut w = header(3, 0, 0);
        body(&mut w, 3, false, &sample_vertices());
        let full = w.0;
        let header_len = 4;
        let mut errors = 0;
        for cut in header_len + 1..full.len() {
            match parse_o3d(&full[..cut]) {
                Err(O3dError::Eof(at)) => {
                    assert!(at <= cut, "cut {cut}: eof at {at}");
                    errors += 1;
                }
                Ok(_) => assert!(matches!(full[cut], 0x49 | 0x26 | 0x79 | 0x54), "cut {cut} inside a section parsed"),
                Err(e) => panic!("cut {cut}: expected Eof, got {e:?}"),
            }
        }
        assert_eq!(errors, full.len() - header_len - 1 - 4);
        assert!(parse_o3d(&full).is_ok());
    }

    #[test]
    fn vertex_count_beyond_the_file_is_an_error() {
        let mut w = header(3, 0, 0);
        w.u8(0x17).u32(1000).vertex(&sample_vertices()[0]);
        assert!(matches!(parse_o3d(&w.0), Err(O3dError::Eof(_))));
    }

    #[test]
    fn unknown_bytes_are_skipped_and_bad_triangles_dropped() {
        let mut w = header(3, 0, 0);
        w.u8(0x00).u8(0xAB);
        w.u8(0x17).u32(1).vertex(&sample_vertices()[0]);
        w.u8(0x33);
        w.u8(0x49).u32(2).u16(0).u16(0).u16(0).u16(0).u16(0).u16(5).u16(0).u16(0);
        let m = parse_o3d(&w.0).unwrap();
        assert_eq!(m.vertices.len(), 1);
        assert_eq!(m.triangles, vec![Triangle { indices: [0, 0, 0], material: 0 }]);
    }

    #[test]
    fn material_name_is_windows_1252() {
        let mut w = header(3, 0, 0);
        w.u8(0x26).u16(1);
        for _ in 0..11 {
            w.f32(0.0);
        }
        w.u8(5).0.extend_from_slice(b"T\xfcr.x");
        assert_eq!(parse_o3d(&w.0).unwrap().materials[0].texture, "T\u{fc}r.x");
    }

    /// The inverse of `unscramble`, as the exe's writer (0x56af20) does it: swap the
    /// position, swap the normal, then negate; the texture coordinates are moved up.
    fn scramble(s: u16, p: &mut Vec3, n: &mut Vec3, uv: &mut Vec2) {
        if s < 1000 {
            std::mem::swap(&mut p.x, &mut p.y);
        } else if s < 3000 {
            std::mem::swap(&mut p.x, &mut p.z);
        } else if s > 7000 {
            std::mem::swap(&mut p.y, &mut p.z);
        }
        if s < 600 {
            std::mem::swap(&mut n.y, &mut n.z);
        } else if s > 4500 {
            std::mem::swap(&mut n.x, &mut n.y);
        }
        if s & 3 == 0 {
            n.x = -n.x;
        }
        if s.is_multiple_of(6) {
            n.y = -n.y;
        }
        if s.is_multiple_of(7) {
            n.z = -n.z;
        }
        if s.is_multiple_of(5) {
            let m = (s % 100) as f32;
            uv.x += m * m / 10000.0;
        }
        if s.is_multiple_of(3) {
            let m = (s % 50) as f32;
            uv.y += m * m / 2500.0;
        }
    }

    /// Write vertices the way a scrambled file stores them, running the same state machine
    /// as the parser (the state of each vertex mixes in the stored position of the one before).
    fn scrambled_file(version: u8, flags: u8, key: u32, vertices: &[Vertex]) -> Vec<u8> {
        let alt = flags & 2 != 0;
        let mut w = header(version, flags, key);
        w.u8(0x17).u32(vertices.len() as u32);
        let k = key as i64 + (version as i64 - 4) + if alt { 0x17D } else { 0 };
        let mut state = k.rem_euclid(0xFDE8) as u16;
        let n16 = (vertices.len() as i64 % 0xFDE8) as u32;
        let mut fb = 0u32;
        for v in vertices {
            if key == 0 {
                state = if alt { 0x130 } else { 0 };
            }
            state = ((state as u32 * n16 + n16 * fb) % 8000) as u16;
            let mut s = *v;
            scramble(state, &mut s.position, &mut s.normal, &mut s.uv);
            fb = frac_byte(s.position);
            w.vertex(&s);
        }
        w.0
    }

    fn assert_close(a: &[Vertex], b: &[Vertex]) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.position, y.position);
            assert_eq!(x.normal, y.normal);
            assert!((x.uv - y.uv).abs().max_element() < 1e-5, "{:?} vs {:?}", x.uv, y.uv);
        }
    }

    #[test]
    fn scrambled_v4_v5_round_trip() {
        let verts = sample_vertices();
        for (version, flags, key) in [(4u8, 0u8, 0u32), (4, 2, 0), (4, 0, 12345), (5, 0, 777), (5, 3, 0xDEAD), (6, 2, 40000)] {
            let bytes = scrambled_file(version, flags, key, &verts);
            let m = parse_o3d(&bytes).unwrap();
            assert_close(&m.vertices, &verts);
        }
    }

    #[test]
    fn scrambled_file_differs_from_plain() {
        // key 0 restarts the state at every vertex; the first one gets state 0 -> x/y position swap
        let verts = sample_vertices();
        let bytes = scrambled_file(4, 0, 0, &verts);
        let mut plain = header(4, 0, 0);
        plain.u8(0x17).u32(3);
        for v in &verts {
            plain.vertex(v);
        }
        assert_ne!(bytes, plain.0);
        // reading the plain records as scrambled ones changes them
        assert_ne!(parse_o3d(&plain.0).unwrap().vertices, verts);
    }

    #[test]
    fn unscramble_position_swaps() {
        let run = |s: u16| {
            let mut p = Vec3::new(1.0, 2.0, 3.0);
            let mut n = Vec3::ZERO;
            let mut uv = Vec2::ZERO;
            unscramble(s, &mut p, &mut n, &mut uv);
            p
        };
        assert_eq!(run(999), Vec3::new(2.0, 1.0, 3.0));
        assert_eq!(run(1000), Vec3::new(3.0, 2.0, 1.0));
        assert_eq!(run(2999), Vec3::new(3.0, 2.0, 1.0));
        assert_eq!(run(3000), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(run(7000), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(run(7001), Vec3::new(1.0, 3.0, 2.0));
    }

    #[test]
    fn unscramble_normal_negation_and_swaps() {
        let run = |s: u16| {
            let mut p = Vec3::ZERO;
            let mut n = Vec3::new(1.0, 2.0, 3.0);
            let mut uv = Vec2::ZERO;
            unscramble(s, &mut p, &mut n, &mut uv);
            n
        };
        // 601: no negation (601 = 601), between 600 and 4500: no swap
        assert_eq!(run(601), Vec3::new(1.0, 2.0, 3.0));
        // 4: x negated (s & 3 == 0), below 600: y/z swapped after the negation
        assert_eq!(run(4), Vec3::new(-1.0, 3.0, 2.0));
        // 12: x (4|12) and y (6|12) negated, then y/z swap
        assert_eq!(run(12), Vec3::new(-1.0, 3.0, -2.0));
        // 4502: above 4500, x/y swapped
        assert_eq!(run(4502), Vec3::new(2.0, 1.0, 3.0));
        // 4500 itself is not above 4500 (and 4500 = 4*1125 = 6*750: x, y negated)
        assert_eq!(run(4500), Vec3::new(-1.0, -2.0, 3.0));
    }

    /// The `s > 6500` branch of `unscramble` is dead in OMSI too: states above 6500 take the
    /// `s > 4500` x/y swap, and no state ever swaps the normal's x and z.
    #[test]
    fn unscramble_dead_6500_branch_matches_exe() {
        for s in [6501u16, 6999, 7001, 7999] {
            let mut p = Vec3::ZERO;
            let mut n = Vec3::new(1.0, 2.0, 3.0);
            let mut uv = Vec2::ZERO;
            unscramble(s, &mut p, &mut n, &mut uv);
            let sign = |neg: bool| if neg { -1.0 } else { 1.0 };
            let neg = Vec3::new(sign(s & 3 == 0), sign(s % 6 == 0), sign(s % 7 == 0));
            let negated = Vec3::new(1.0, 2.0, 3.0) * neg;
            assert_eq!(n, Vec3::new(negated.y, negated.x, negated.z), "state {s}");
        }
        for s in 0..8000u16 {
            let mut p = Vec3::ZERO;
            let mut n = Vec3::new(1.0, 2.0, 4.0);
            let mut uv = Vec2::ZERO;
            unscramble(s, &mut p, &mut n, &mut uv);
            assert_ne!(n.x.abs(), 4.0, "state {s} swapped the normal's x and z");
        }
    }

    #[test]
    fn unscramble_uv_offsets() {
        let run = |s: u16| {
            let mut uv = Vec2::new(1.0, 1.0);
            unscramble(s, &mut Vec3::default(), &mut Vec3::default(), &mut uv);
            uv
        };
        // 1: neither multiple of 5 nor of 3
        assert_eq!(run(1), Vec2::new(1.0, 1.0));
        // 10: u -= 10^2 / 10000
        assert!((run(10) - Vec2::new(0.99, 1.0)).abs().max_element() < 1e-6);
        // 30: u -= 30^2/10000 = 0.09, v -= 30^2/2500 = 0.36
        assert!((run(30) - Vec2::new(0.91, 0.64)).abs().max_element() < 1e-6);
        // 1002: v -= (1002 % 50)^2 / 2500 = 4/2500
        assert!((run(1002) - Vec2::new(1.0, 1.0 - 4.0 / 2500.0)).abs().max_element() < 1e-6);
    }

    #[test]
    fn frac_byte_mixes_fractional_parts() {
        assert_eq!(frac_byte(Vec3::new(1.0, 2.0, 3.0)), 0);
        // 0.5 * 0.5 * 0.5 * 600 = 75
        assert_eq!(frac_byte(Vec3::new(1.5, -2.5, 3.5)), 75);
    }
}
