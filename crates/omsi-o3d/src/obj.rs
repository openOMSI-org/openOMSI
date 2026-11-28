//! Wavefront OBJ with its `.mtl` materials: positions, normals and texture coordinates
//! (negative indices, `v/vt/vn`, `v//vn`, polygons of any size), groups ignored, materials
//! by `usemtl` with their `Kd`, `d`/`Tr` and `map_Kd`.

use crate::import::{Builder, texture_name};
use crate::{Material, Mesh, O3dError};
use glam::{Vec2, Vec3};
use std::collections::HashMap;
use std::path::Path;

fn err(e: impl std::fmt::Display) -> O3dError {
    O3dError::Import(format!("OBJ: {e}"))
}

fn floats<'a>(it: impl Iterator<Item = &'a str>) -> Vec<f32> {
    it.filter_map(|s| s.parse::<f32>().ok()).collect()
}

/// Read an OBJ file's bytes (`path` names it: the `.mtl` and pictures are beside it).
pub fn parse_obj(bytes: &[u8], path: &Path) -> Result<Mesh, O3dError> {
    let text = String::from_utf8_lossy(bytes);
    let dir = path.parent().unwrap_or(Path::new(""));
    let (mut pos, mut nor, mut uvs): (Vec<Vec3>, Vec<Vec3>, Vec<Vec2>) =
        (Vec::new(), Vec::new(), Vec::new());
    let mut b = Builder::default();
    let mut library: HashMap<String, Material> = HashMap::new();
    let mut slots: HashMap<String, u16> = HashMap::new();
    let mut current: Option<u16> = None;
    let mut corner_vertex: HashMap<(usize, usize, usize), u32> = HashMap::new();
    let mut missing: Vec<bool> = Vec::new();
    // a line may go on in the next one after a `\`
    let mut joined = String::new();
    for raw in text.lines() {
        if let Some(stripped) = raw.strip_suffix('\\') {
            joined.push_str(stripped);
            joined.push(' ');
            continue;
        }
        joined.push_str(raw);
        let line = std::mem::take(&mut joined);
        let line = line.split('#').next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let f = floats(it);
                if f.len() >= 3 {
                    pos.push(Vec3::new(f[0], f[1], f[2]));
                }
            }
            Some("vn") => {
                let f = floats(it);
                if f.len() >= 3 {
                    nor.push(Vec3::new(f[0], f[1], f[2]));
                }
            }
            Some("vt") => {
                let f = floats(it);
                // (OBJ's v runs up the picture, Direct3D's down)
                uvs.push(Vec2::new(
                    f.first().copied().unwrap_or(0.0),
                    1.0 - f.get(1).copied().unwrap_or(0.0),
                ));
            }
            Some("mtllib") => {
                let file = line["mtllib".len()..].trim();
                let p = omsi_cfg::resolve_path(dir, file);
                if let Ok(t) = omsi_cfg::vfs::read(&p) {
                    read_mtl(&String::from_utf8_lossy(&t), &p, &mut library);
                } else {
                    log::warn!(
                        "{}: material library {} not found",
                        path.display(),
                        p.display()
                    );
                }
            }
            Some("usemtl") => {
                let name = line["usemtl".len()..].trim().to_string();
                let next = b.mesh.materials.len() as u16;
                let slot = *slots.entry(name.clone()).or_insert_with(|| {
                    b.mesh
                        .materials
                        .push(library.get(&name).cloned().unwrap_or_default());
                    next
                });
                current = Some(slot);
            }
            Some("f") => {
                let material = *current.get_or_insert_with(|| {
                    b.mesh.materials.push(Material::default());
                    b.mesh.materials.len() as u16 - 1
                });
                let mut corners: Vec<u32> = Vec::new();
                for c in it {
                    let mut parts = c.split('/');
                    // 1-based, negative counts back from the end
                    let index = |s: Option<&str>, len: usize| -> Option<usize> {
                        let v: i64 = s.filter(|s| !s.is_empty())?.parse().ok()?;
                        let i = if v < 0 { len as i64 + v } else { v - 1 };
                        (i >= 0 && (i as usize) < len).then_some(i as usize)
                    };
                    let Some(pi) = index(parts.next(), pos.len()) else {
                        return Err(err(format!(
                            "a face names a vertex the file does not have: {c}"
                        )));
                    };
                    let ti = index(parts.next(), uvs.len());
                    let ni = index(parts.next(), nor.len());
                    let key = (pi, ti.unwrap_or(usize::MAX), ni.unwrap_or(usize::MAX));
                    let v = match corner_vertex.get(&key) {
                        Some(v) => *v,
                        None => {
                            let v = b.vertex(
                                pos[pi],
                                ni.map(|i| nor[i]).unwrap_or(Vec3::Y),
                                ti.map(|i| uvs[i]).unwrap_or(Vec2::ZERO),
                            );
                            missing.push(ni.is_none());
                            corner_vertex.insert(key, v);
                            v
                        }
                    };
                    corners.push(v);
                }
                for k in 1..corners.len().saturating_sub(1) {
                    b.triangle(corners[0], corners[k], corners[k + 1], material);
                }
            }
            _ => {}
        }
    }
    if b.mesh.triangles.is_empty() {
        return Err(err("no faces in the file"));
    }
    b.fill_normals(&missing);
    Ok(b.finish())
}

/// The materials of a `.mtl` file into `out` by name.
fn read_mtl(text: &str, mtl: &Path, out: &mut HashMap<String, Material>) {
    let mut name: Option<String> = None;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        let Some(key) = it.next() else { continue };
        if key == "newmtl" {
            let n = line["newmtl".len()..].trim().to_string();
            out.insert(n.clone(), Material::default());
            name = Some(n);
            continue;
        }
        let Some(m) = name.as_ref().and_then(|n| out.get_mut(n)) else {
            continue;
        };
        match key {
            "Kd" => {
                let f = floats(it);
                if f.len() >= 3 {
                    m.diffuse = [f[0], f[1], f[2], m.diffuse[3]];
                }
            }
            "Ks" => {
                let f = floats(it);
                if f.len() >= 3 {
                    m.specular = [f[0], f[1], f[2]];
                }
            }
            "Ke" => {
                let f = floats(it);
                if f.len() >= 3 {
                    m.emissive = [f[0], f[1], f[2]];
                }
            }
            "Ns" => m.specular_power = floats(it).first().copied().unwrap_or(1.0),
            "d" => m.diffuse[3] = floats(it).first().copied().unwrap_or(1.0),
            "Tr" => m.diffuse[3] = 1.0 - floats(it).first().copied().unwrap_or(0.0),
            "map_Kd" => {
                // the file name is the last word (options like `-s 1 1 1` go before it)
                let file = line.split_whitespace().last().unwrap_or("");
                m.texture = texture_name(mtl, file);
            }
            _ => {}
        }
    }
    // (a material with a texture shows its picture as it is)
    for m in out.values_mut() {
        if !m.texture.is_empty() {
            m.diffuse = [1.0, 1.0, 1.0, m.diffuse[3]];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quad_with_a_material_and_negative_indices() {
        let dir = std::env::temp_dir().join(format!("omsi-o3d-obj-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("m.mtl"),
            "newmtl red\nKd 1 0 0\nnewmtl tex\nKd 0.5 0.5 0.5\nmap_Kd -s 1 1 1 wall.png\n",
        )
        .unwrap();
        std::fs::write(dir.join("wall.png"), b"\x89PNG").unwrap();
        let obj = "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nusemtl tex\nf -4/1 -3/2 -2/3 -1/4\nusemtl red\nf 1 2 3\n";
        let p = dir.join("q.obj");
        std::fs::write(&p, obj).unwrap();
        let m = crate::load_mesh(&p).unwrap();
        assert_eq!(m.triangles.len(), 3);
        assert_eq!(m.materials.len(), 2);
        assert_eq!(m.materials[0].texture, "wall.png");
        assert_eq!(
            omsi_cfg::model_texture("WALL.PNG"),
            Some(dir.join("wall.png"))
        );
        assert_eq!(m.materials[1].diffuse, [1.0, 0.0, 0.0, 1.0]);
        // v runs down; the normals were made (facing the file's +z, Direct3D's -z)
        assert_eq!(m.vertices[2].uv, Vec2::new(1.0, 0.0));
        assert!(
            (m.vertices[0].normal - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5,
            "{:?}",
            m.vertices[0].normal
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
