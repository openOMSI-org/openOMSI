//! A plain `.o3d` written: version 1 (16-bit counts) while the mesh fits, version 3 (32-bit
//! counts, and 32-bit triangle indices beyond 65535 vertices) past that - the files OMSI's
//! converter wrote; a pivot matrix and bones when the mesh has them. The layout is
//! `docs/FORMATS.md`'s: sections `17` vertices, `49` triangles, `26` materials, `79` matrix,
//! `54` bones, in that order.

#[derive(Debug, Clone, PartialEq)]
pub struct O3dMaterial {
    pub diffuse: [f32; 4],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub power: f32,
    /// The texture's file name, in the model's texture folder.
    pub texture: String,
}

impl Default for O3dMaterial {
    fn default() -> Self {
        O3dMaterial { diffuse: [1.0; 4], specular: [0.0; 3], emissive: [0.0; 3], power: 0.0, texture: String::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct O3dMesh {
    /// `x y z nx ny nz u v` per vertex, in OMSI's mesh frame (X right, Y up, Z forward).
    pub vertices: Vec<[f32; 8]>,
    /// Three vertex indices and the material.
    pub triangles: Vec<([u32; 3], u16)>,
    pub materials: Vec<O3dMaterial>,
    /// The pivot matrix (`origin_from_mesh`), 16 numbers row-major, when wanted.
    pub matrix: Option<[f32; 16]>,
    /// Bones: name and (vertex, weight) pairs.
    pub bones: Vec<(String, Vec<(u32, f32)>)>,
}

/// The version the file gets: 1 while every count fits in 16 bits and there are no bones,
/// 3 otherwise.
pub fn version_of(m: &O3dMesh) -> u8 {
    if m.vertices.len() > 0xFFFF || m.triangles.len() > 0xFFFF || !m.bones.is_empty() {
        3
    } else {
        1
    }
}

/// The file's bytes.
pub fn write(m: &O3dMesh) -> Vec<u8> {
    let version = version_of(m);
    let long = m.vertices.len() > 0xFFFF;
    let mut b: Vec<u8> = Vec::with_capacity(16 + m.vertices.len() * 32 + m.triangles.len() * 14);
    b.extend_from_slice(&[0x84, 0x19, version]);
    if version >= 3 {
        b.push(u8::from(long));
    }
    let count = |b: &mut Vec<u8>, n: usize| {
        if version >= 3 {
            b.extend_from_slice(&(n as u32).to_le_bytes());
        } else {
            b.extend_from_slice(&(n as u16).to_le_bytes());
        }
    };
    let index = |b: &mut Vec<u8>, i: u32| {
        if long {
            b.extend_from_slice(&i.to_le_bytes());
        } else {
            b.extend_from_slice(&(i as u16).to_le_bytes());
        }
    };
    let f = |b: &mut Vec<u8>, v: f32| b.extend_from_slice(&v.to_le_bytes());
    let str8 = |b: &mut Vec<u8>, s: &str| {
        let (enc, _, _) = encoding_rs::WINDOWS_1252.encode(s);
        let n = enc.len().min(255);
        b.push(n as u8);
        b.extend_from_slice(&enc[..n]);
    };
    b.push(0x17);
    count(&mut b, m.vertices.len());
    for v in &m.vertices {
        for x in v {
            f(&mut b, *x);
        }
    }
    b.push(0x49);
    count(&mut b, m.triangles.len());
    for (idx, mat) in &m.triangles {
        for i in idx {
            index(&mut b, *i);
        }
        b.extend_from_slice(&mat.to_le_bytes());
    }
    b.push(0x26);
    b.extend_from_slice(&(m.materials.len().min(0xFFFF) as u16).to_le_bytes());
    for mat in m.materials.iter().take(0xFFFF) {
        for x in mat.diffuse.iter().chain(&mat.specular).chain(&mat.emissive) {
            f(&mut b, *x);
        }
        f(&mut b, mat.power);
        str8(&mut b, &mat.texture);
    }
    if let Some(mx) = &m.matrix {
        b.push(0x79);
        for x in mx {
            f(&mut b, *x);
        }
    }
    if !m.bones.is_empty() {
        b.push(0x54);
        b.extend_from_slice(&(m.bones.len().min(0xFFFF) as u16).to_le_bytes());
        for (name, weights) in m.bones.iter().take(0xFFFF) {
            str8(&mut b, name);
            b.extend_from_slice(&(weights.len().min(0xFFFF) as u16).to_le_bytes());
            for (v, w) in weights.iter().take(0xFFFF) {
                index(&mut b, *v);
                f(&mut b, *w);
            }
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_mesh_is_a_version_1_file_the_reader_takes_back() {
        let m = O3dMesh {
            vertices: vec![[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0], [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0]],
            triangles: vec![([0, 1, 2], 1)],
            materials: vec![O3dMaterial::default(), O3dMaterial { diffuse: [0.5, 0.6, 0.7, 1.0], specular: [0.1, 0.1, 0.1], emissive: [0.0; 3], power: 8.0, texture: "Straße.bmp".into() }],
            matrix: None,
            bones: Vec::new(),
        };
        let b = write(&m);
        assert_eq!(&b[..3], &[0x84, 0x19, 1]);
        assert_eq!(b[3], 0x17);
        assert_eq!(u16::from_le_bytes([b[4], b[5]]), 3);
        let back = omsi_o3d::parse_o3d(&b).unwrap();
        assert_eq!(back.version, 1);
        assert_eq!(back.vertices.len(), 3);
        assert_eq!(back.vertices[2].uv, glam::Vec2::new(0.0, 1.0));
        assert_eq!(back.triangles[0].indices, [0, 1, 2]);
        assert_eq!(back.triangles[0].material, 1);
        assert_eq!(back.materials[1].texture, "Straße.bmp");
        assert_eq!(back.materials[1].specular_power, 8.0);
        assert!(!back.has_transform);
    }

    #[test]
    fn a_big_mesh_or_bones_make_a_version_3_file() {
        let mut m = O3dMesh::default();
        m.vertices = vec![[0.0; 8]; 70_000];
        m.triangles = vec![([0, 69_999, 1], 0)];
        m.materials.push(O3dMaterial::default());
        assert_eq!(version_of(&m), 3);
        let back = omsi_o3d::parse_o3d(&write(&m)).unwrap();
        assert_eq!(back.vertices.len(), 70_000);
        assert_eq!(back.triangles[0].indices, [0, 69_999, 1]);
        // bones and a pivot too
        let m = O3dMesh {
            vertices: vec![[0.0; 8]; 2],
            triangles: vec![],
            materials: vec![O3dMaterial::default()],
            matrix: Some([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.5, 2.5, 3.5, 1.0]),
            bones: vec![("Bone.001".into(), vec![(1, 0.75)])],
        };
        let back = omsi_o3d::parse_o3d(&write(&m)).unwrap();
        assert_eq!(back.version, 3);
        assert!(back.has_transform);
        assert_eq!(back.transform.w_axis.x, 1.5);
        assert_eq!(back.bones[0].name, "Bone.001");
        assert_eq!(back.bones[0].weights[0].vertex, 1);
    }
}
