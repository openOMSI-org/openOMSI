//! From a `.x` file to the meshes to write, by the converter's settings - the eight
//! switches of OMSI's "X to O3D Converter 1.0", under their names, with its defaults, and
//! doing what it does (checked against the stock objects that ship both files):
//!
//! * the vertices of the `.o3d` are the corners of the faces, in the order the faces list
//!   them: a corner is its vertex's position and texture coordinate with the normal
//!   `MeshNormals` gives that corner, so a vertex two faces light differently is written
//!   twice; *Opt. Vertices* folds corners that are the same in every number into one (off,
//!   every corner is a vertex of its own);
//! * the meshes come in the order of their frames' names (the original keeps the frames in
//!   a sorted list), the file's own order between meshes of one name;
//! * every position goes through the matrices of the frames the mesh sits in, the mesh's
//!   own first, then its parents' (a normal as a vector through the same matrix, made unit
//!   length again after it); *Use Submesh Local Pos. as Global Pos.* skips the matrices;
//! * *X-File: z up* swaps Y and Z after that (the file's up is Z, OMSI's is Y) and, as the
//!   swap mirrors, writes every triangle the other way round (a mirroring frame matrix does
//!   not: the original leaves the winding alone there);
//! * *All Normals Up* sets every normal to straight up;
//! * *Opt. Materials* drops the materials no face uses (the list as the exporter wrote it,
//!   in its order, less those); *Remove Specular* zeroes the specular colour (the power
//!   stays);
//! * *Separate Files* writes one `.o3d` a mesh (`<file>_<mesh>`), else all the meshes of
//!   the file go into one, the vertices folded across them.
//!
//! Beyond the original: skin weights become bones (off unless asked), the frame matrix may
//! be kept as the pivot (`origin_from_mesh`), and a mesh without normals may get flat ones
//! (off, they are zeros, as the original wrote them).

use crate::o3d::{O3dMaterial, O3dMesh};
use crate::xfile::{Frame, MaterialSlot, XFile, XMaterial, XMesh};
use glam::{Mat3, Mat4, Vec3};
use std::collections::HashMap;

/// The converter's settings, named as its switches are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// *X-File: z up*: the file's Z is up; swapped to OMSI's Y up.
    pub z_up: bool,
    /// *Separate Files*: one `.o3d` a mesh.
    pub separate_files: bool,
    /// *Show Details of X*: the log tells each frame, mesh and material.
    pub show_details: bool,
    /// *Remove Specular*: the materials' specular colour zeroed.
    pub remove_specular: bool,
    /// *Opt. Materials*: the materials no face uses dropped.
    pub opt_materials: bool,
    /// *Opt. Vertices*: corners that are the same folded into one vertex.
    pub opt_vertices: bool,
    /// *All Normals Up*: every normal straight up.
    pub all_normals_up: bool,
    /// *Use Submesh Local Pos. as Global Pos.*: the frame matrices left out.
    pub local_as_global: bool,
    /// Skin weights written as bones (the original converter dropped them).
    pub bones: bool,
    /// The mesh's frame matrix kept as the file's pivot (`origin_from_mesh`).
    pub pivot: bool,
    /// A mesh without normals gets flat ones from its faces (off: zeros, as the original).
    pub flat_normals: bool,
}

impl Default for Options {
    /// The original's defaults: Z up, remove specular, optimise materials and vertices.
    fn default() -> Self {
        Options { z_up: true, separate_files: false, show_details: false, remove_specular: true, opt_materials: true, opt_vertices: true, all_normals_up: false, local_as_global: false, bones: false, pivot: false, flat_normals: false }
    }
}

/// One file to write: its name (without `.o3d`), the mesh, and what the conversion did.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub name: String,
    pub mesh: O3dMesh,
    pub log: Vec<String>,
}

/// A mesh of the file with the matrix of the frames around it.
struct Placed<'a> {
    mesh: &'a XMesh,
    frame: String,
    xform: Mat4,
}

fn collect<'a>(f: &'a Frame, parent: Mat4, out: &mut Vec<Placed<'a>>) {
    let local = f.matrix.map(|m| Mat4::from_cols_array(&m)).unwrap_or(Mat4::IDENTITY);
    // (D3D's row-vector matrix read column by column is glam's column-vector one: the mesh's
    // own frame applies first, its parents' after, as `v · child · parent`)
    let xform = parent * local;
    for m in &f.meshes {
        out.push(Placed { mesh: m, frame: f.name.clone(), xform });
    }
    for c in &f.children {
        collect(c, xform, out);
    }
}

fn material_of(slot: &MaterialSlot, named: &HashMap<String, XMaterial>, log: &mut Vec<String>) -> XMaterial {
    match slot {
        MaterialSlot::Inline(m) => m.clone(),
        MaterialSlot::Reference(name) => named.get(name).cloned().unwrap_or_else(|| {
            log.push(format!("material {name} is referred to but nowhere in the file: white"));
            XMaterial::default()
        }),
    }
}

/// The texture as the `.o3d` names it: the file name alone, the folders dropped.
fn texture_name(t: &str) -> String {
    t.rsplit(['\\', '/']).next().unwrap_or(t).trim().to_string()
}

/// A name fit for a file: letters, digits, `_`, `-`, `.`.
fn file_safe(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') { c } else { '_' }).collect()
}

/// An output being built: the file and the corners it holds already (for the folding).
struct Builder {
    out: Output,
    seen: HashMap<[u32; 8], u32>,
}

impl Builder {
    fn new(name: String) -> Builder {
        Builder { out: Output { name, mesh: O3dMesh::default(), log: Vec::new() }, seen: HashMap::new() }
    }

    /// The vertex for a corner: the one already there that is the same (when folding), else
    /// a new one.
    fn corner(&mut self, v: [f32; 8], fold: bool) -> u32 {
        if fold {
            let key = v.map(f32::to_bits);
            if let Some(&i) = self.seen.get(&key) {
                return i;
            }
            let i = self.out.mesh.vertices.len() as u32;
            self.seen.insert(key, i);
            self.out.mesh.vertices.push(v);
            i
        } else {
            self.out.mesh.vertices.push(v);
            (self.out.mesh.vertices.len() - 1) as u32
        }
    }
}

/// The meshes of `x` converted; `stem` names the output (`<stem>` as one file, or
/// `<stem>_<mesh>` each).
pub fn convert(x: &XFile, stem: &str, o: &Options) -> Vec<Output> {
    let mut placed = Vec::new();
    collect(&x.root, Mat4::IDENTITY, &mut placed);
    placed.sort_by_key(|p| p.frame.to_lowercase());
    if placed.is_empty() {
        return vec![Output { name: stem.to_string(), mesh: O3dMesh { materials: vec![O3dMaterial::default()], ..Default::default() }, log: vec!["no mesh in the file".into()] }];
    }
    let mut outputs: Vec<Output> = Vec::new();
    let mut current: Option<Builder> = None;
    let mut used_names: HashMap<String, usize> = HashMap::new();
    for (k, p) in placed.iter().enumerate() {
        if o.separate_files || current.is_none() {
            if let Some(done) = current.take() {
                outputs.push(finish(done.out, o));
            }
            let name = if o.separate_files {
                let base = if p.mesh.name.is_empty() { if p.frame.is_empty() { format!("mesh{}", k + 1) } else { p.frame.clone() } } else { p.mesh.name.clone() };
                let base = format!("{stem}_{}", file_safe(&base));
                let n = used_names.entry(base.clone()).or_insert(0);
                *n += 1;
                if *n > 1 { format!("{base}_{n}") } else { base }
            } else {
                stem.to_string()
            };
            current = Some(Builder::new(name));
        }
        let b = current.as_mut().expect("an output");
        add_mesh(b, p, x, o);
    }
    if let Some(done) = current.take() {
        outputs.push(finish(done.out, o));
    }
    outputs
}

fn add_mesh(b: &mut Builder, p: &Placed, x: &XFile, o: &Options) {
    let m = p.mesh;
    if o.show_details {
        b.out.log.push(format!(
            "mesh {} in frame {}: {} vertices, {} faces, {} normals, {} texture coordinates, {} materials, {} bones",
            if m.name.is_empty() { "(unnamed)" } else { &m.name },
            if p.frame.is_empty() { "(none)" } else { &p.frame },
            m.positions.len(),
            m.faces.len(),
            m.normals.len(),
            m.uvs.len(),
            m.materials.len(),
            m.skin.len()
        ));
        if !o.local_as_global && p.xform != Mat4::IDENTITY {
            let t = p.xform.w_axis;
            b.out.log.push(format!("  frame matrix applied (translation {:.3} {:.3} {:.3})", t.x, t.y, t.z));
        }
    }
    let xform = if o.local_as_global { Mat4::IDENTITY } else { p.xform };
    let linear = Mat3::from_mat4(xform);
    let nv = m.positions.len();
    // a mesh without normals gets flat ones from its faces when asked (the original wrote zeros)
    let flat: Vec<Vec3> = if o.flat_normals && m.normals.is_empty() && !m.faces.is_empty() {
        b.out.log.push("no normals in the file: flat ones from the faces".into());
        m.faces
            .iter()
            .map(|f| {
                let p = |i: usize| f.get(i).and_then(|v| m.positions.get(*v as usize)).map(|v| Vec3::from_array(*v));
                match (p(0), p(1), p(2)) {
                    (Some(a), Some(c), Some(d)) => (c - a).cross(d - a).normalize_or_zero(),
                    _ => Vec3::ZERO,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let place = |pos: [f32; 3], n: Vec3, uv: [f32; 2]| -> [f32; 8] {
        let mut pt = xform.transform_point3(Vec3::from_array(pos));
        let mut n = linear * n;
        if o.z_up {
            pt = Vec3::new(pt.x, pt.z, pt.y);
            n = Vec3::new(n.x, n.z, n.y);
        }
        if o.all_normals_up {
            n = Vec3::Y;
        }
        let n = n.normalize_or_zero();
        [pt.x, pt.y, pt.z, n.x, n.y, n.z, uv[0], uv[1]]
    };
    // the materials of the list (a mesh without one gets white)
    let base_m = b.out.mesh.materials.len();
    let mut mats: Vec<O3dMaterial> = m
        .materials
        .iter()
        .map(|slot| {
            let xm = material_of(slot, &x.named_materials, &mut b.out.log);
            O3dMaterial {
                diffuse: xm.diffuse,
                specular: if o.remove_specular { [0.0; 3] } else { xm.specular },
                emissive: xm.emissive,
                power: xm.power,
                texture: texture_name(&xm.texture),
            }
        })
        .collect();
    if mats.is_empty() {
        mats.push(O3dMaterial::default());
    }
    let last_mat = (mats.len() - 1) as u32;
    // the corners, face by face, as vertices; a face's corners beyond its first three as a
    // fan (the original's own vertex order: a corner the first time a face names it)
    let mut vertex_of: Vec<Vec<u32>> = Vec::with_capacity(nv);
    vertex_of.resize_with(nv, Vec::new);
    let mut skipped = 0usize;
    for (fi, f) in m.faces.iter().enumerate() {
        if f.len() < 3 || f.iter().any(|v| (*v as usize) >= nv) {
            skipped += 1;
            continue;
        }
        let mat = (base_m as u32 + m.face_materials.get(fi).copied().unwrap_or(0).min(last_mat)).min(0xFFFF) as u16;
        let corners: Vec<u32> = f
            .iter()
            .enumerate()
            .map(|(k, vi)| {
                let n = if m.normals.is_empty() {
                    flat.get(fi).copied().unwrap_or(Vec3::ZERO)
                } else {
                    m.normal_faces.get(fi).and_then(|nf| nf.get(k)).and_then(|ni| m.normals.get(*ni as usize)).map(|n| Vec3::from_array(*n)).unwrap_or(Vec3::ZERO)
                };
                let v = place(m.positions[*vi as usize], n, m.uvs.get(*vi as usize).copied().unwrap_or([0.0, 0.0]));
                let i = b.corner(v, o.opt_vertices);
                vertex_of[*vi as usize].push(i);
                i
            })
            .collect();
        for k in 1..corners.len() - 1 {
            let (c1, c2) = if o.z_up { (corners[k + 1], corners[k]) } else { (corners[k], corners[k + 1]) };
            b.out.mesh.triangles.push(([corners[0], c1, c2], mat));
        }
    }
    if skipped > 0 {
        b.out.log.push(format!("{skipped} faces left out (fewer than three corners, or a vertex the mesh does not have)"));
    }
    b.out.mesh.materials.extend(mats);
    if o.bones {
        for s in &m.skin {
            let mut weights: Vec<(u32, f32)> = Vec::new();
            for (i, w) in s.indices.iter().zip(&s.weights) {
                if let Some(vs) = vertex_of.get(*i as usize) {
                    let mut vs = vs.clone();
                    vs.sort_unstable();
                    vs.dedup();
                    for v in vs {
                        weights.push((v, *w));
                    }
                }
            }
            if !weights.is_empty() {
                b.out.mesh.bones.push((s.bone.clone(), weights));
            }
        }
    } else if !m.skin.is_empty() && o.show_details {
        b.out.log.push(format!("  {} skin weights left out (bones are off)", m.skin.len()));
    }
    if o.pivot && b.out.mesh.matrix.is_none() && p.xform != Mat4::IDENTITY {
        b.out.mesh.matrix = Some(p.xform.to_cols_array());
    }
}

/// The unused materials dropped, and the summary line.
fn finish(mut out: Output, o: &Options) -> Output {
    let nm0 = out.mesh.materials.len();
    if o.opt_materials && !out.mesh.triangles.is_empty() {
        let mut used = vec![false; nm0];
        for t in &out.mesh.triangles {
            if let Some(u) = used.get_mut(t.1 as usize) {
                *u = true;
            }
        }
        let mut keep: Vec<O3dMaterial> = Vec::new();
        let mut map: Vec<u16> = Vec::with_capacity(nm0);
        for (m, u) in out.mesh.materials.iter().zip(&used) {
            map.push(keep.len() as u16);
            if *u {
                keep.push(m.clone());
            }
        }
        for t in &mut out.mesh.triangles {
            t.1 = map.get(t.1 as usize).copied().unwrap_or(0);
        }
        out.mesh.materials = keep;
    }
    let m = &out.mesh;
    out.log.push(format!(
        "{}.o3d: {} vertices, {} triangles, {} materials{}{}, version {}",
        out.name,
        m.vertices.len(),
        m.triangles.len(),
        m.materials.len(),
        if m.materials.len() != nm0 { format!(" (of {nm0})") } else { String::new() },
        if m.bones.is_empty() { String::new() } else { format!(", {} bones", m.bones.len()) },
        crate::o3d::version_of(m)
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfile::parse;

    const BOX: &str = "xof 0303txt 0032\n\
Material Red { 1.0;0.0;0.0;1.0;; 8.0; 0.5;0.5;0.5;; 0.0;0.0;0.0;; TextureFilename { \"tex\\\\red.bmp\"; } }\n\
Frame Root {\n\
  FrameTransformMatrix { 1,0,0,0, 0,0,1,0, 0,1,0,0, 0,0,0,1;; }\n\
  Frame Box {\n\
    FrameTransformMatrix { 2,0,0,0, 0,2,0,0, 0,0,2,0, 0.5,1.5,2.5,1;; }\n\
    Mesh Cube {\n\
      4;\n 0;0;0;,\n 1;0;0;,\n 1;1;0;,\n 0;1;0;;\n\
      1;\n 4;0,3,2,1;;\n\
      MeshNormals { 2; 0;0;1;, 1;0;0;; 1; 4;0,0,1,1;; }\n\
      MeshTextureCoords { 4; 0;0;, 1;0;, 1;1;, 0;1;; }\n\
      MeshMaterialList { 2; 1; 1;; Material { 1;1;1;1;; 0; 0;0;0;; 0;0;0;; } { Red } }\n\
      SkinWeights { \"Bone.001\"; 2; 0, 3; 1.0, 0.5; 1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1;; }\n\
    }\n\
  }\n\
  Frame Twin { Mesh Twin { 3; 0;0;0;, 1;0;0;, 0;0;1;; 1; 3;0,1,2;; MeshMaterialList { 1; 1; 0;; { Red } } } }\n\
}\n";

    #[test]
    fn the_corners_make_the_vertices_in_face_order_through_the_frames() {
        let x = parse(BOX.as_bytes()).unwrap();
        // as the original with every switch at its default, but the file's own up kept
        let o = Options { z_up: false, ..Default::default() };
        let out = convert(&x, "box", &o);
        assert_eq!(out.len(), 1);
        let m = &out[0].mesh;
        // the quad's corners 0,3,2,1 become vertices 0..3 in that order, as two triangles
        assert_eq!(m.triangles[0], ([0, 1, 2], 0));
        assert_eq!(m.triangles[1], ([0, 2, 3], 0));
        // vertex 1 is the file's vertex 3 (0,1,0): scaled by 2 and moved (0.5,3.5,2.5), then Root swaps Y and Z
        let v = m.vertices[1];
        assert_eq!(&v[..3], &[0.5, 2.5, 3.5]);
        // its corner's normal (0,0,1) through Box is (0,0,2), through Root (0,2,0), unit again
        assert_eq!(&v[3..6], &[0.0, 1.0, 0.0]);
        assert_eq!(&v[6..], &[0.0, 1.0]);
        // the third corner's normal is the other one (1,0,0) -> (2,0,0) -> unit (1,0,0)
        assert_eq!(&m.vertices[2][3..6], &[1.0, 0.0, 0.0]);
        // the inline white that no face uses is dropped; the twin frame's Red stays its own
        assert_eq!(m.materials.len(), 2);
        assert_eq!(m.materials[0].texture, "red.bmp");
        assert_eq!(m.materials[0].specular, [0.0; 3]);
        assert_eq!(m.materials[0].power, 8.0);
        assert_eq!(m.triangles[2].1, 1);
        // the twin's corner at the origin has normal (0,0,0): not the cube's corner, so 7 vertices
        assert_eq!(m.vertices.len(), 7);
        assert!(m.bones.is_empty(), "no bones unless asked");
        assert!(out[0].log.last().unwrap().contains("7 vertices, 3 triangles, 2 materials (of 3), version 1"));
    }

    #[test]
    fn z_up_swaps_after_the_frames() {
        let x = parse(BOX.as_bytes()).unwrap();
        let out = convert(&x, "box", &Options::default());
        let v = out[0].mesh.vertices[1];
        assert_eq!(&v[..3], &[0.5, 3.5, 2.5]);
        assert_eq!(&v[3..6], &[0.0, 0.0, 1.0]);
        assert_eq!(out[0].mesh.triangles[0], ([0, 2, 1], 0), "turned round by the swap");
    }

    #[test]
    fn the_other_switches() {
        let x = parse(BOX.as_bytes()).unwrap();
        let o = Options { separate_files: true, local_as_global: true, all_normals_up: true, remove_specular: false, opt_materials: false, opt_vertices: false, bones: true, pivot: true, show_details: true, ..Default::default() };
        let out = convert(&x, "box", &o);
        assert_eq!(out.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["box_Cube", "box_Twin"]);
        let m = &out[0].mesh;
        // local positions, only Z up applied; every normal straight up
        assert_eq!(&m.vertices[1][..3], &[0.0, 0.0, 1.0]);
        assert_eq!(&m.vertices[1][3..6], &[0.0, 1.0, 0.0]);
        assert_eq!(m.materials.len(), 2);
        assert_eq!(m.materials[1].specular, [0.5; 3]);
        // the bones follow the corners: the file's vertex 0 is corner 0, vertex 3 is corner 1
        assert_eq!(m.bones, vec![("Bone.001".to_string(), vec![(0, 1.0), (1, 0.5)])]);
        assert!(m.matrix.is_some(), "the frame matrix kept as the pivot");
        assert!(out[0].log.iter().any(|l| l.starts_with("mesh Cube in frame Box")));
        assert_eq!(out[1].mesh.materials.len(), 1);
    }

    #[test]
    fn folding_keeps_the_first_of_equal_corners_and_a_vertex_lit_two_ways_is_two() {
        let x = parse("xof 0303txt 0032\nMesh { 4; 0;0;0;, 1;0;0;, 0;0;0;, 1;0;0;; 2; 3;0,1,2;, 3;2,3,1;; MeshNormals { 2; 0;0;1;, 0;1;0;; 2; 3;0,0,0;, 3;1,1,1;; } SkinWeights { \"b\"; 1; 1; 1.0; 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1;; } }\n".as_bytes()).unwrap();
        let out = convert(&x, "w", &Options { bones: true, ..Default::default() });
        let m = &out[0].mesh;
        // face 0: corners 0,1,2 with normal 0 -> vertices 0,1,0 (2 is the same as 0);
        // face 1: corners 2,3,1 with normal 1 -> new vertices 2,3 and 3 again (lit the other
        // way; 1 and 3 are the same place). Z up turns the triangles round.
        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.triangles.iter().map(|t| t.0).collect::<Vec<_>>(), vec![[0, 0, 1], [2, 3, 3]]);
        // the file's vertex 1 became vertices 1 and 3
        assert_eq!(m.bones[0].1, vec![(1, 1.0), (3, 1.0)]);
        assert_eq!(m.materials.len(), 1, "a mesh without a material list gets white");
        // without folding every corner is a vertex
        let out = convert(&x, "w", &Options { opt_vertices: false, ..Default::default() });
        assert_eq!(out[0].mesh.vertices.len(), 6);
    }

    #[test]
    fn a_file_without_a_mesh_still_gives_a_file() {
        let x = parse("xof 0303txt 0032\nFrame Empty { }\n".as_bytes()).unwrap();
        let out = convert(&x, "e", &Options::default());
        assert_eq!(out[0].name, "e");
        assert!(out[0].mesh.vertices.is_empty());
    }
}
