//! Autodesk FBX (binary and ASCII, every version the tools write), read with ufbx: every
//! mesh of the scene in metres and in its rest pose, with its materials' colour and colour
//! texture (a file or packed into the FBX), its skin (bone names and weights, the skeleton's
//! rest positions) and its blend shapes.

use crate::import::{embedded_texture, from_rh, texture_name, Builder};
use crate::{Bone, BoneWeight, Joint, Material, Mesh, Morph, O3dError};
use glam::{Mat3, Quat, Vec2, Vec3};
use std::collections::HashMap;
use std::path::Path;

fn err(e: impl std::fmt::Display) -> O3dError {
    O3dError::Import(format!("FBX: {e}"))
}

/// An ASCII FBX starts with a comment line or a `FBXHeaderExtension` block.
pub(crate) fn looks_like_ascii_fbx(b: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&b[..b.len().min(512)]);
    head.contains("FBX") || head.contains("FBXHeaderExtension")
}

fn v3(v: ufbx::Vec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn rot_rh(m: &ufbx::Matrix) -> Quat {
    let c = |x: f64, y: f64, z: f64| Vec3::new(x as f32, y as f32, -(z as f32)).normalize_or_zero();
    let (x, y) = (c(m.m00, m.m10, m.m20), c(m.m01, m.m11, m.m21));
    let z = x.cross(y).normalize_or_zero();
    Quat::from_mat3(&Mat3::from_cols(x, z.cross(x), z)).normalize()
}

fn pos_rh(m: &ufbx::Matrix) -> Vec3 {
    from_rh(Vec3::new(m.m03 as f32, m.m13 as f32, m.m23 as f32))
}

fn load_opts() -> ufbx::LoadOpts<'static> {
    ufbx::LoadOpts {
        target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        space_conversion: ufbx::SpaceConversion::ModifyGeometry,
        generate_missing_normals: true,
        ..Default::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Clip {
    pub fps: f32,
    pub names: Vec<String>,
    pub frames: Vec<Vec<(Vec3, Quat)>>,
}

pub fn parse_fbx_clip(bytes: &[u8]) -> Result<Clip, O3dError> {
    let scene = ufbx::load_memory(bytes, load_opts()).map_err(|e| err(e.description.as_ref() as &str))?;
    let stack = scene.anim_stacks.iter().next().ok_or_else(|| err("no animation"))?;
    let fps = if scene.settings.frames_per_second > 1.0 { scene.settings.frames_per_second } else { 30.0 };
    let joints: Vec<usize> = (0..scene.nodes.len()).filter(|&i| scene.nodes[i].bone.is_some()).collect();
    if joints.is_empty() {
        return Err(err("no skeleton"));
    }
    let names = joints.iter().map(|&i| (scene.nodes[i].element.name.as_ref() as &str).to_string()).collect();
    let n = (((stack.time_end - stack.time_begin) * fps).round() as usize).clamp(1, 3_000);
    let mut frames = Vec::with_capacity(n);
    for f in 0..n {
        let t = stack.time_begin + f as f64 / fps;
        let s = ufbx::evaluate_scene(&scene, &stack.anim, t, ufbx::EvaluateOpts::default()).map_err(|e| err(e.description.as_ref() as &str))?;
        frames.push(joints.iter().map(|&i| (pos_rh(&s.nodes[i].node_to_world), rot_rh(&s.nodes[i].node_to_world))).collect());
    }
    Ok(Clip { fps: fps as f32, names, frames })
}

pub fn load_clip(path: &Path) -> Result<Clip, O3dError> {
    let bytes = std::fs::read(path).map_err(|e| err(format!("{}: {e}", path.display())))?;
    parse_fbx_clip(&bytes)
}

/// Read an FBX file's bytes (`path` names it: textures are looked for beside it).
pub fn parse_fbx(bytes: &[u8], path: &Path) -> Result<Mesh, O3dError> {
    let scene = ufbx::load_memory(bytes, load_opts()).map_err(|e| err(e.description.as_ref() as &str))?;
    let mut b = Builder::default();
    // materials by ufbx element id
    let mut mat_index: HashMap<u32, u16> = HashMap::new();
    let mut material = |mat: Option<&ufbx::Material>, b: &mut Builder| -> u16 {
        let id = mat.map(|m| m.element.element_id).unwrap_or(u32::MAX);
        if let Some(i) = mat_index.get(&id) {
            return *i;
        }
        let m = match mat {
            None => Material { diffuse: [0.8, 0.8, 0.8, 1.0], ..Default::default() },
            Some(m) => {
                let (p, f) = (&m.pbr, &m.fbx);
                let tex = p.base_color.texture.as_ref().or(f.diffuse_color.texture.as_ref());
                let c = if f.diffuse_color.has_value { f.diffuse_color.value_vec4 } else { p.base_color.value_vec4 };
                // (with a texture its colour alone counts, as in the tools: Max writes a grey
                // diffuse beside the map and means the map)
                let diffuse = if tex.is_some() { [1.0, 1.0, 1.0, 1.0] } else { [c.x as f32, c.y as f32, c.z as f32, 1.0] };
                let e = f.emission_color.value_vec4;
                let ef = if f.emission_factor.has_value { f.emission_factor.value_vec4.x as f32 } else { 1.0 };
                Material {
                    diffuse,
                    specular: [0.0; 3],
                    emissive: if f.emission_color.has_value { [e.x as f32 * ef, e.y as f32 * ef, e.z as f32 * ef] } else { [0.0; 3] },
                    specular_power: 1.0,
                    texture: tex.map(|t| texture_of(path, t)).unwrap_or_default(),
                }
            }
        };
        b.mesh.materials.push(m);
        let i = b.mesh.materials.len() as u16 - 1;
        mat_index.insert(id, i);
        i
    };
    let mut bones: HashMap<String, Vec<BoneWeight>> = HashMap::new();
    let mut bone_order: Vec<String> = Vec::new();
    let mut morphs: Vec<Morph> = Vec::new();
    // rest positions of the bones the skins name (the bind pose, where the skin was made)
    let mut bind: HashMap<u32, (Vec3, Quat)> = HashMap::new();
    let mut tri = Vec::new();
    for node in scene.nodes.iter() {
        let Some(mesh) = node.mesh.as_ref() else { continue };
        let g = &node.geometry_to_world;
        let nmat = ufbx::matrix_for_normals(g);
        // one vertex per distinct (control point, normal, uv) of the face corners - by value:
        // the exporters write a normal and a uv of their own for every corner
        let mut corner_vertex: HashMap<(u32, [u32; 5]), u32> = HashMap::new();
        // the vertices made of each control point (for skin weights and blend shapes)
        let mut of_point: Vec<Vec<u32>> = vec![Vec::new(); mesh.num_vertices];
        let mut out_of = |corner: usize, b: &mut Builder| -> u32 {
            let point = mesh.vertex_indices[corner];
            let n = if mesh.vertex_normal.exists { v3(mesh.vertex_normal[corner]) } else { Vec3::Y };
            // (FBX's v runs up the picture, Direct3D's down)
            let uv = if mesh.vertex_uv.exists {
                let t = mesh.vertex_uv[corner];
                Vec2::new(t.x as f32, 1.0 - t.y as f32)
            } else {
                Vec2::ZERO
            };
            let key = (point, [n.x.to_bits(), n.y.to_bits(), n.z.to_bits(), uv.x.to_bits(), uv.y.to_bits()]);
            if let Some(v) = corner_vertex.get(&key) {
                return *v;
            }
            let p = v3(ufbx::transform_position(g, mesh.vertex_position[corner]));
            let n = v3(ufbx::transform_direction(&nmat, ufbx::Vec3 { x: n.x as f64, y: n.y as f64, z: n.z as f64 }));
            let v = b.vertex(p, n, uv);
            corner_vertex.insert(key, v);
            if let Some(list) = of_point.get_mut(point as usize) {
                list.push(v);
            }
            v
        };
        tri.resize(mesh.max_face_triangles * 3, 0u32);
        for (fi, face) in mesh.faces.iter().enumerate() {
            let n = mesh.triangulate_face(&mut tri, *face) as usize;
            let mi = mesh.face_material.get(fi).copied().unwrap_or(0) as usize;
            let mat = node.materials.get(mi).or_else(|| mesh.materials.get(mi)).map(|r| &**r);
            let mat = material(mat, &mut b);
            for t in tri[..n * 3].chunks_exact(3) {
                let [a, c, d] = [t[0], t[1], t[2]].map(|i| out_of(i as usize, &mut b));
                b.triangle(a, c, d, mat);
            }
        }
        for skin in mesh.skin_deformers.iter() {
            for cl in skin.clusters.iter() {
                let Some(bone) = cl.bone_node.as_ref() else { continue };
                let name = (bone.element.name.as_ref() as &str).to_string();
                let m = &cl.bind_to_world;
                bind.insert(bone.element.element_id, (pos_rh(m), rot_rh(m)));
                if !bones.contains_key(&name) {
                    bone_order.push(name.clone());
                }
                let list = bones.entry(name).or_default();
                for (pt, w) in cl.vertices.iter().zip(cl.weights.iter()) {
                    if *w <= 0.0 {
                        continue;
                    }
                    for v in of_point.get(*pt as usize).into_iter().flatten() {
                        list.push(BoneWeight { vertex: *v, weight: *w as f32 });
                    }
                }
            }
        }
        for bd in mesh.blend_deformers.iter() {
            for ch in bd.channels.iter() {
                // the shape at full weight (the last keyframe of an in-between series)
                let Some(shape) = ch.keyframes.iter().last().map(|k| &k.shape).or(ch.target_shape.as_ref()) else { continue };
                let full = (ch.element.name.as_ref() as &str).to_string();
                // (Maya and Max write `blendShape1.Smile`: the channel's own name is the part after the dot)
                let name = full.rsplit('.').next().unwrap_or(&full).to_string();
                let mut offsets = Vec::new();
                for (k, pt) in shape.offset_vertices.iter().enumerate() {
                    let dp = shape.position_offsets.get(k).map(|v| v3(ufbx::transform_direction(g, *v))).unwrap_or(Vec3::ZERO);
                    let dn = shape.normal_offsets.get(k).map(|v| v3(ufbx::transform_direction(&nmat, *v))).unwrap_or(Vec3::ZERO);
                    if dp.length_squared() < 1e-14 && dn.length_squared() < 1e-14 {
                        continue;
                    }
                    for v in of_point.get(*pt as usize).into_iter().flatten() {
                        offsets.push((*v, from_rh(dp), from_rh(dn)));
                    }
                }
                if !offsets.is_empty() {
                    match morphs.iter_mut().find(|m| m.name == name) {
                        Some(m) => m.offsets.extend(offsets),
                        None => morphs.push(Morph { name, offsets }),
                    }
                }
            }
        }
    }
    if b.mesh.triangles.is_empty() {
        return Err(err("no meshes in the file"));
    }
    // the skeleton: the bone nodes, in the file's order, with parents among them
    let is_joint = |n: &ufbx::Node| n.bone.is_some() || bind.contains_key(&n.element.element_id);
    let ids: Vec<u32> = scene.nodes.iter().filter(|n| is_joint(n)).map(|n| n.element.element_id).collect();
    for n in scene.nodes.iter().filter(|n| is_joint(n)) {
        let id = n.element.element_id;
        let w = &n.node_to_world;
        let (position, rotation) = bind.get(&id).copied().unwrap_or((pos_rh(w), rot_rh(w)));
        let parent = n.parent.as_ref().and_then(|p| ids.iter().position(|i| *i == p.element.element_id));
        b.mesh.joints.push(Joint { name: (n.element.name.as_ref() as &str).to_string(), parent, position, rotation });
    }
    b.mesh.bones = bone_order.into_iter().map(|name| Bone { weights: bones.remove(&name).unwrap_or_default(), name }).collect();
    b.mesh.morphs = morphs;
    Ok(b.finish())
}

/// The texture name of an FBX texture: its packed picture, or its file.
fn texture_of(model: &Path, t: &ufbx::Texture) -> String {
    let content: &[u8] = t.content.as_ref();
    if !content.is_empty() {
        return embedded_texture(model, t.element.name.as_ref() as &str, content);
    }
    let files = [t.relative_filename.as_ref() as &str, t.filename.as_ref(), t.absolute_filename.as_ref()];
    let file = files.iter().find(|f| !f.is_empty()).copied().unwrap_or("");
    texture_name(model, file)
}
