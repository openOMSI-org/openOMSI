//! What the model formats of other tools (glTF / GLB, FBX, Wavefront OBJ) share on their
//! way into a [`Mesh`]: they are right-handed with y up, where OMSI's mesh files keep
//! Direct3D's left-handed frame (x right, y up, z forward). A modeller exporting for OMSI
//! builds in Blender with the vehicle's front along +y; Blender's glTF / FBX / OBJ export
//! turns that into -z, so z is turned round (and the winding with it): the model comes in
//! facing the way it faced in the modeller, as openOMSI's own `--export-glb` writes it out.

use crate::{Material, Mesh, Triangle, Vertex};
use glam::{Vec2, Vec3};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A point or direction of a right-handed, y-up file in the mesh frame.
pub(crate) fn from_rh(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, -v.z)
}

/// Collects the vertices and triangles of a model given in a right-handed frame.
#[derive(Default)]
pub(crate) struct Builder {
    pub mesh: Mesh,
}

impl Builder {
    /// A vertex in the file's frame; returns its index.
    pub fn vertex(&mut self, position: Vec3, normal: Vec3, uv: Vec2) -> u32 {
        self.mesh.vertices.push(Vertex {
            position: from_rh(position),
            normal: from_rh(normal).normalize_or_zero(),
            uv,
        });
        self.mesh.vertices.len() as u32 - 1
    }

    /// A triangle wound counter-clockwise in the file's frame (as glTF, FBX and OBJ wind
    /// their front faces): clockwise once z has turned round, as Direct3D draws it.
    pub fn triangle(&mut self, a: u32, b: u32, c: u32, material: u16) {
        self.mesh.triangles.push(Triangle {
            indices: [a, c, b],
            material,
        });
    }

    /// Normals for the vertices that came without any (an OBJ without `vn`, a glTF
    /// primitive without NORMAL): the area-weighted sum of the faces round each vertex.
    pub fn fill_normals(&mut self, missing: &[bool]) {
        if !missing.iter().any(|m| *m) {
            return;
        }
        self.mesh.drop_bad_triangles();
        let mut acc = vec![Vec3::ZERO; self.mesh.vertices.len()];
        for t in &self.mesh.triangles {
            let [a, b, c] = t.indices.map(|i| self.mesh.vertices[i as usize].position);
            // (the stored order is the file's turned round, and so is the frame: the plain
            // cross product of the stored edges points out of the front)
            let n = (b - a).cross(c - a);
            for i in t.indices {
                acc[i as usize] += n;
            }
        }
        for (i, v) in self.mesh.vertices.iter_mut().enumerate() {
            if missing.get(i).copied().unwrap_or(false) {
                v.normal = acc[i].normalize_or(Vec3::Y);
            }
        }
    }

    pub fn finish(mut self) -> Mesh {
        if self.mesh.materials.is_empty() {
            self.mesh.materials.push(Material::default());
        }
        self.mesh.transform = glam::Mat4::IDENTITY;
        self.mesh.drop_bad_triangles();
        self.mesh
    }
}

/// The texture name a material of a model file gives for a picture file named `file`
/// (as the file has it: relative to the model, absolute on the author's machine, with `\`
/// or `/`): the bare file name, looked for in the usual texture folders as any OMSI
/// texture is, and else the picture beside the model (see
/// `omsi_cfg::register_model_texture`).
pub(crate) fn texture_name(model: &Path, file: &str) -> String {
    let file = file.trim().replace('\\', "/");
    let dir = model.parent().unwrap_or(Path::new(""));
    if file.is_empty() {
        return String::new();
    }
    let rel = Path::new(&file);
    let mut tries: Vec<PathBuf> = Vec::new();
    let bare = file.rsplit('/').next().unwrap_or(&file).to_string();
    if bare.contains(':') || bare.trim_matches(['.', ' ']).is_empty() {
        return bare;
    }
    if !rel.is_absolute() && !file.contains(':') {
        tries.push(omsi_cfg::resolve_path(dir, &file));
    }
    tries.push(dir.join(&bare));
    for sub in ["texture", "textures", "Texture", "Textures"] {
        tries.push(dir.join(sub).join(&bare));
    }
    if let Some(t) = tries.into_iter().find(|t| omsi_cfg::vfs::is_file(t)) {
        omsi_cfg::register_model_texture(&bare, t);
    }
    bare
}

/// A picture packed into the model file, put into memory beside it (see
/// `omsi_cfg::vfs::put_memory_file`); returns the texture name that finds it.
pub(crate) fn embedded_texture(model: &Path, key: &str, bytes: &[u8]) -> String {
    let ext = picture_extension(bytes);
    let safe: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let file = format!(
        "{}.{safe}.{ext}",
        model
            .file_name()
            .map(|f| f.to_string_lossy())
            .unwrap_or_default()
    );
    let path = model.parent().unwrap_or(Path::new("")).join(&file);
    omsi_cfg::vfs::put_memory_file(path.clone(), Arc::from(bytes));
    omsi_cfg::register_model_texture(&file, path);
    file
}

/// The file extension that fits a picture's content (what the texture loader goes by
/// when a picture has no signature it knows).
fn picture_extension(b: &[u8]) -> &'static str {
    if b.starts_with(b"\x89PNG") {
        "png"
    } else if b.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else if b.starts_with(b"DDS ") {
        "dds"
    } else if b.starts_with(b"BM") {
        "bmp"
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "webp"
    } else if b.starts_with(b"\xABKTX 20") {
        "ktx2"
    } else if b.starts_with(b"GIF8") {
        "gif"
    } else if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        "tif"
    } else {
        "tga"
    }
}

/// Decode standard base64 (a `data:` URI's payload); None on a character outside it.
pub(crate) fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// `%20` and the like in a URI.
pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_percent_decode() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("AAEC").unwrap(), [0, 1, 2]);
        assert!(base64("a*b").is_none());
        assert_eq!(percent_decode("my%20tex.png"), "my tex.png");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn a_model_picture_named_outside_the_content_folder_is_not_taken() {
        let dir = std::env::temp_dir().join(format!("omsi-o3d-model-tex-{}", std::process::id()));
        let root = dir.join("content");
        let model = root.join("Vehicles/Bus/model");
        let outside = dir.join("outside");
        for d in [&model, &root.join("Vehicles/Bus/pics"), &outside] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(root.join("Vehicles/Bus/pics/inside-pic.png"), b"x").unwrap();
        std::fs::write(outside.join("outside-pic.png"), b"x").unwrap();
        omsi_cfg::add_content_root(root.clone());
        let m = model.join("bus.gltf");
        assert_eq!(texture_name(&m, "../pics/inside-pic.png"), "inside-pic.png");
        assert!(omsi_cfg::model_texture("inside-pic.png").is_some());
        assert_eq!(
            texture_name(&m, "../../../../../../../../outside/outside-pic.png"),
            "outside-pic.png"
        );
        assert_eq!(omsi_cfg::model_texture("outside-pic.png"), None);
        assert_eq!(texture_name(&m, "C:outside-pic.png"), "C:outside-pic.png");
        omsi_cfg::remove_content_root(&root);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn normals_are_made_without_the_triangles_that_name_missing_vertices() {
        let mut b = Builder::default();
        let i = [Vec3::ZERO, Vec3::X, Vec3::Y].map(|p| b.vertex(p, Vec3::Z, Vec2::ZERO));
        b.triangle(i[0], i[1], i[2], 0);
        b.triangle(0, 1, 500, 0);
        b.fill_normals(&[true, true, true]);
        assert_eq!(b.mesh.triangles.len(), 1);
        assert!((b.mesh.vertices[1].normal - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);
    }

    #[test]
    fn a_right_handed_triangle_keeps_its_front() {
        // counter-clockwise seen from +z (towards the viewer) in the file
        let mut b = Builder::default();
        let i = [Vec3::ZERO, Vec3::X, Vec3::Y].map(|p| b.vertex(p, Vec3::Z, Vec2::ZERO));
        b.triangle(i[0], i[1], i[2], 0);
        let mut m = b.finish();
        assert_eq!(m.triangles[0].indices, [0, 2, 1]);
        assert_eq!(m.vertices[0].normal, Vec3::new(0.0, 0.0, -1.0));
        // normals made for it point the same way as the file's
        let mut b = Builder {
            mesh: std::mem::take(&mut m),
        };
        b.fill_normals(&[true, true, true]);
        assert!((b.mesh.vertices[1].normal - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);
    }
}
