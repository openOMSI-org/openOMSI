//! glTF 2.0 (`.gltf` with its `.bin` and pictures beside it, or one `.glb`): every mesh of
//! the default scene, moved by its nodes, with its base colour, its colour texture (a file
//! beside it or packed into the file), its skin and its morph targets.

use crate::import::{base64, embedded_texture, from_rh, percent_decode, texture_name, Builder};
use crate::{Bone, BoneWeight, Joint, Material, Mesh, Morph, O3dError};
use glam::{Mat4, Vec2, Vec3};
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn err(e: impl std::fmt::Display) -> O3dError {
    O3dError::Import(format!("glTF: {e}"))
}

/// Read a `.glb` or a `.gltf` (`path` names it: external buffers and pictures are next to it).
pub fn parse_glb_or_gltf(bytes: &[u8], path: &Path) -> Result<Mesh, O3dError> {
    let gltf = ::gltf::Gltf::from_slice(bytes).map_err(err)?;
    let dir = path.parent().unwrap_or(Path::new(""));
    // the buffers: the GLB's own chunk, data: URIs, files beside the model
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for b in gltf.buffers() {
        let data = match b.source() {
            ::gltf::buffer::Source::Bin => gltf.blob.clone().ok_or_else(|| err("the GLB has no binary chunk"))?,
            ::gltf::buffer::Source::Uri(uri) => read_uri(dir, uri)?,
        };
        if data.len() < b.length() {
            return Err(err(format!("buffer {} holds {} of {} bytes", b.index(), data.len(), b.length())));
        }
        buffers.push(data);
    }
    // the pictures as texture names
    let mut images: Vec<String> = Vec::new();
    for img in gltf.images() {
        let name = match img.source() {
            ::gltf::image::Source::View { view, .. } => {
                let buf = &buffers[view.buffer().index()];
                let bytes = buf.get(view.offset()..view.offset() + view.length()).ok_or_else(|| err("an image view lies outside its buffer"))?;
                embedded_texture(path, &format!("image{}", img.index()), bytes)
            }
            ::gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => {
                let bytes = read_uri(dir, uri)?;
                embedded_texture(path, &format!("image{}", img.index()), &bytes)
            }
            ::gltf::image::Source::Uri { uri, .. } => texture_name(path, &percent_decode(uri)),
        };
        images.push(name);
    }
    let mut b = Builder::default();
    for m in gltf.materials() {
        let pbr = m.pbr_metallic_roughness();
        let texture = pbr.base_color_texture().and_then(|t| images.get(t.texture().source().index()).cloned()).unwrap_or_default();
        let e = m.emissive_factor();
        b.mesh.materials.push(Material { diffuse: pbr.base_color_factor(), specular: [0.0; 3], emissive: e, specular_power: 1.0, texture });
    }
    let default_material = b.mesh.materials.len() as u16;
    let mut used_default = false;
    // node transforms of the scene
    let roots: Vec<::gltf::Node> = match gltf.default_scene().or_else(|| gltf.scenes().next()) {
        Some(s) => s.nodes().collect(),
        None => gltf.nodes().collect(),
    };
    let (world, parent, order) = walk(roots);
    let mut missing_normals: Vec<bool> = Vec::new();
    let mut bones: HashMap<String, Vec<BoneWeight>> = HashMap::new();
    let mut bone_order: Vec<String> = Vec::new();
    let mut morphs: Vec<Morph> = Vec::new();
    let mut joint_rest: HashMap<usize, Vec3> = HashMap::new();
    for node in &order {
        let Some(mesh) = node.mesh() else { continue };
        let skin = node.skin();
        // a skinned mesh is drawn where its joints put it (the node's own transform does
        // not count); a plain one where its node is
        let xf = if skin.is_some() { Mat4::IDENTITY } else { world[&node.index()] };
        let nxf = Mat4::from_mat3(glam::Mat3::from_mat4(xf).inverse().transpose());
        let joints: Vec<::gltf::Node> = skin.as_ref().map(|s| s.joints().collect()).unwrap_or_default();
        if let Some(s) = &skin {
            let r = s.reader(|buf| buffers.get(buf.index()).map(|d| d.as_slice()));
            if let Some(ibm) = r.read_inverse_bind_matrices() {
                for (j, m) in joints.iter().zip(ibm) {
                    let bind = Mat4::from_cols_array_2d(&m).inverse();
                    joint_rest.insert(j.index(), bind.w_axis.truncate());
                }
            }
        }
        let targets: Vec<String> = mesh
            .extras()
            .as_ref()
            .and_then(|e| serde_json::from_str::<serde_json::Value>(e.get()).ok())
            .and_then(|v| v.get("targetNames").and_then(|t| t.as_array()).map(|a| a.iter().map(|s| s.as_str().unwrap_or("").to_string()).collect()))
            .unwrap_or_default();
        for prim in mesh.primitives() {
            if prim.mode() != ::gltf::mesh::Mode::Triangles {
                continue;
            }
            let r = prim.reader(|buf| buffers.get(buf.index()).map(|d| d.as_slice()));
            let Some(pos) = r.read_positions() else { continue };
            let pos: Vec<[f32; 3]> = pos.collect();
            let nor: Vec<[f32; 3]> = r.read_normals().map(|n| n.collect()).unwrap_or_default();
            let uv: Vec<[f32; 2]> = r.read_tex_coords(0).map(|t| t.into_f32().collect()).unwrap_or_default();
            let idx: Vec<u32> = r.read_indices().map(|i| i.into_u32().collect()).unwrap_or_else(|| (0..pos.len() as u32).collect());
            let material = match prim.material().index() {
                Some(i) => i as u16,
                None => {
                    used_default = true;
                    default_material
                }
            };
            let base = b.mesh.vertices.len() as u32;
            for (i, p) in pos.iter().enumerate() {
                let p = xf.transform_point3(Vec3::from(*p));
                let n = nor.get(i).map(|n| nxf.transform_vector3(Vec3::from(*n))).unwrap_or(Vec3::Y);
                // (glTF's v already runs down the picture, as Direct3D's)
                b.vertex(p, n, uv.get(i).map(|t| Vec2::from(*t)).unwrap_or(Vec2::ZERO));
                missing_normals.push(nor.get(i).is_none());
            }
            for t in idx.chunks_exact(3) {
                b.triangle(base.saturating_add(t[0]), base.saturating_add(t[1]), base.saturating_add(t[2]), material);
            }
            // skin weights, per joint name
            if let (Some(js), Some(ws)) = (r.read_joints(0), r.read_weights(0)) {
                for (v, (j, w)) in js.into_u16().zip(ws.into_f32()).take(pos.len()).enumerate() {
                    for k in 0..4 {
                        if w[k] <= 0.0 {
                            continue;
                        }
                        let Some(node) = joints.get(j[k] as usize) else { continue };
                        let name = node.name().map(str::to_string).unwrap_or_else(|| format!("joint{}", node.index()));
                        if !bones.contains_key(&name) {
                            bone_order.push(name.clone());
                        }
                        bones.entry(name).or_default().push(BoneWeight { vertex: base + v as u32, weight: w[k] });
                    }
                }
            }
            // morph targets (by the mesh's `targetNames` where it has them)
            for (k, (dp, dn, _)) in r.read_morph_targets().enumerate() {
                let dp: Vec<[f32; 3]> = dp.map(|i| i.collect()).unwrap_or_default();
                let dn: Vec<[f32; 3]> = dn.map(|i| i.collect()).unwrap_or_default();
                let name = targets.get(k).cloned().unwrap_or_else(|| format!("morph{k}"));
                let at = match morphs.iter().position(|m| m.name == name) {
                    Some(i) => i,
                    None => {
                        morphs.push(Morph { name, offsets: Vec::new() });
                        morphs.len() - 1
                    }
                };
                for v in 0..pos.len() {
                    let p = dp.get(v).map(|d| Vec3::from(*d)).unwrap_or(Vec3::ZERO);
                    let n = dn.get(v).map(|d| Vec3::from(*d)).unwrap_or(Vec3::ZERO);
                    if p.length_squared() > 1e-14 || n.length_squared() > 1e-14 {
                        let (p, n) = (xf.transform_vector3(p), nxf.transform_vector3(n));
                        morphs[at].offsets.push((base + v as u32, from_rh(p), from_rh(n)));
                    }
                }
            }
        }
    }
    if used_default || b.mesh.materials.is_empty() {
        b.mesh.materials.push(Material { diffuse: [0.8, 0.8, 0.8, 1.0], ..Default::default() });
    }
    b.fill_normals(&missing_normals);
    // the skeleton: every node that is a joint of a skin, with its rest position
    let skin_joints: Vec<usize> = gltf.skins().flat_map(|s| s.joints().map(|j| j.index()).collect::<Vec<_>>()).collect();
    for n in gltf.nodes().filter(|n| skin_joints.contains(&n.index())) {
        let position = joint_rest.get(&n.index()).copied().or_else(|| world.get(&n.index()).map(|w| w.w_axis.truncate())).unwrap_or(Vec3::ZERO);
        b.mesh.joints.push(Joint { name: n.name().map(str::to_string).unwrap_or_else(|| format!("joint{}", n.index())), parent: None, position: from_rh(position), rotation: glam::Quat::IDENTITY });
    }
    let index_of: HashMap<usize, usize> = gltf.nodes().filter(|n| skin_joints.contains(&n.index())).enumerate().map(|(k, n)| (n.index(), k)).collect();
    for (k, n) in gltf.nodes().filter(|n| skin_joints.contains(&n.index())).enumerate() {
        b.mesh.joints[k].parent = parent.get(&n.index()).and_then(|p| index_of.get(p)).copied();
    }
    b.mesh.bones = bone_order.into_iter().map(|name| Bone { weights: bones.remove(&name).unwrap_or_default(), name }).collect();
    b.mesh.morphs = morphs;
    if b.mesh.triangles.is_empty() {
        return Err(err("no triangles in the file"));
    }
    Ok(b.finish())
}

type Walk<'a> = (HashMap<usize, Mat4>, HashMap<usize, usize>, Vec<::gltf::Node<'a>>);

fn walk(roots: Vec<::gltf::Node<'_>>) -> Walk<'_> {
    let mut world: HashMap<usize, Mat4> = HashMap::new();
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut stack: Vec<(::gltf::Node, Mat4, Option<usize>)> = roots.into_iter().map(|n| (n, Mat4::IDENTITY, None)).collect();
    let mut order: Vec<::gltf::Node> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    while let Some((node, up, from)) = stack.pop() {
        if !seen.insert(node.index()) {
            continue;
        }
        if let Some(from) = from {
            parent.insert(node.index(), from);
        }
        let w = up * Mat4::from_cols_array_2d(&node.transform().matrix());
        world.insert(node.index(), w);
        for c in node.children() {
            stack.push((c, w, Some(node.index())));
        }
        order.push(node);
    }
    (world, parent, order)
}

/// The bytes a buffer or picture URI names: a `data:` URI's own, or a file beside the model.
fn read_uri(dir: &Path, uri: &str) -> Result<Vec<u8>, O3dError> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (_, payload) = rest.split_once(";base64,").ok_or_else(|| err("a data URI that is not base64"))?;
        return base64(payload).ok_or_else(|| err("a damaged data URI"));
    }
    let rel = percent_decode(uri);
    let p = omsi_cfg::resolve_path(dir, &rel);
    omsi_cfg::vfs::read(&p).map_err(|e| err(format!("{}: {e}", p.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-triangle glTF with its buffer as a data URI and a picture beside it.
    fn triangle_gltf(dir: &Path) -> std::path::PathBuf {
        let mut bin: Vec<u8> = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for _ in 0..3 {
            for c in [0.0f32, 0.0, 1.0] {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for t in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for c in t {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        let b64 = {
            const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in bin.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for k in 0..4 {
                    if k <= c.len() {
                        s.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
                    } else {
                        s.push('=');
                    }
                }
            }
            s
        };
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("tex.png"), b"\x89PNG").unwrap();
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[0,0,-2]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0.5,0.5,1],"baseColorTexture":{{"index":0}}}}}}],
            "textures":[{{"source":0}}],"images":[{{"uri":"tex.png"}}],
            "buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{b64}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},
            {{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}}]}}"#,
            len = bin.len()
        );
        let p = dir.join("tri.gltf");
        std::fs::write(&p, json).unwrap();
        p
    }

    #[test]
    fn a_node_that_lists_itself_or_shares_children_is_walked_once() {
        let json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"children":[0,1,1]},{"children":[0,1]}]}"#;
        let e = parse_glb_or_gltf(json, Path::new("loop.gltf")).unwrap_err();
        assert!(e.to_string().contains("no triangles"), "{e}");
    }

    #[test]
    fn joint_parents_never_loop_back() {
        let json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"children":[0,1]},{"children":[0,1,2]},{}]}"#;
        let g = ::gltf::Gltf::from_slice(json).unwrap();
        let (_, parent, order) = walk(g.scenes().next().unwrap().nodes().collect());
        assert_eq!(order.len(), 3);
        for start in 0..3 {
            let mut at = start;
            let mut steps = 0;
            while let Some(&p) = parent.get(&at) {
                at = p;
                steps += 1;
                assert!(steps <= 3, "a loop from node {start}");
            }
        }
        assert_eq!(parent.get(&0), None);
        assert_eq!(parent.get(&1), Some(&0));
        assert_eq!(parent.get(&2), Some(&1));
    }

    #[test]
    fn a_gltf_comes_in_moved_by_its_node_with_its_texture() {
        let dir = std::env::temp_dir().join(format!("omsi-o3d-gltf-{}", std::process::id()));
        let p = triangle_gltf(&dir);
        let m = crate::load_mesh(&p).unwrap();
        assert_eq!(m.vertices.len(), 3);
        // translated by -2 along the file's z, which is +2 along Direct3D's z
        assert!((m.vertices[1].position - Vec3::new(1.0, 0.0, 2.0)).length() < 1e-6, "{:?}", m.vertices[1].position);
        assert_eq!(m.triangles[0].indices, [0, 2, 1]);
        assert_eq!(m.materials[0].diffuse, [1.0, 0.5, 0.5, 1.0]);
        assert_eq!(m.materials[0].texture, "tex.png");
        assert_eq!(omsi_cfg::model_texture("TEX.PNG"), Some(dir.join("tex.png")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
