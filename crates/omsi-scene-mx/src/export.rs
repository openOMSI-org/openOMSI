//! `--export-glb`: write the player's vehicle as a binary glTF file, textures included,
//! for the launcher's 3D preview (three.js reads it straight away). The meshes go out in
//! their resting pose with the vehicle's paint scheme applied.

use anyhow::{Context, Result};
use glam::Vec3;
use omsi_sim::{VehicleInstance, VehicleType};
use std::collections::HashMap;
use std::path::Path;

struct Prim {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    material: usize,
    name: String,
}

/// Write `out` from the vehicle as it stands now. `scheme` is the paint scheme index.
pub fn export_glb(root: &Path, vt: &VehicleType, vehicle: &VehicleInstance, scheme: Option<usize>, out: &Path) -> Result<()> {
    let textures = omsi_texture::TextureCache::new();
    let dirs = vt.texture_dirs(root);
    let (subst, _scheme_dir): (hashbrown::HashMap<String, String>, Option<std::path::PathBuf>) = match scheme {
        Some(i) => vt.scheme_substitutions(i, omsi_texture::find_texture),
        None => (vt.default_substitutions(root), None),
    };
    // (the scheme's pictures are named in full by the substitutions; its folder is no
    // search path for the model's other textures)
    let dirs_all: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
    // textures by (lower-case) file name → (png bytes, has alpha)
    let mut images: Vec<(Vec<u8>, bool)> = Vec::new();
    let mut image_index: HashMap<String, Option<usize>> = HashMap::new();
    let mut materials: Vec<(Option<usize>, bool, [f32; 4])> = Vec::new(); // (image, blended, colour)
    let mut material_names: Vec<String> = Vec::new();
    let mut material_index: HashMap<String, usize> = HashMap::new();
    let mut prims: Vec<Prim> = Vec::new();
    let mut skipped = 0;
    // the vehicle itself, then every coupled part (the rear of an articulated bus) at its
    // offset from the front section
    let mut parts: Vec<(&VehicleType, Vec<(glam::Mat4, &omsi_sim::vehicle::MeshProps, usize)>)> = Vec::new();
    parts.push((vt, (0..vt.meshes.len()).map(|i| (vehicle.mesh_local_transform(i), &vehicle.mesh_props[i], i)).collect()));
    for t in &vehicle.trailers {
        let offset = glam::Mat4::from_translation((t.position - vehicle.position).as_vec3());
        parts.push((&t.ty, (0..t.ty.meshes.len()).map(|i| (offset * t.mesh_local_transform(i), &t.mesh_props[i], i)).collect()));
    }
    for (vt, meshes) in &parts {
    for (xf, props, i) in meshes.iter().copied() {
        let vm = &vt.meshes[i];
        let def = &vt.model.meshes[vm.def_index];
        // resting pose: what an outside observer sees
        if !props.visible || (def.viewpoint != 0 && def.viewpoint & 1 == 0) {
            skipped += 1;
            continue;
        }
        let normal_xf = xf.inverse().transpose();
        for (first, count, slot) in &vm.data.ranges {
            let (first, count, slot) = (*first as usize, *count as usize, *slot as usize);
            if count == 0 {
                continue;
            }
            // the dirt film and other [matl_alphascale] overlays are faded out on a clean
            // bus: leave them out rather than showing them opaque
            if props.slot_alpha.get(slot).copied().unwrap_or(1.0) < 0.05 {
                continue;
            }
            let mat = vm.materials.get(slot);
            let tex_name = mat.map(|m| m.texture.trim().to_string()).unwrap_or_default();
            let tex_name = subst.get(&tex_name.to_ascii_lowercase()).cloned().unwrap_or(tex_name);
            // OMSI reads the diffuse alpha only as an alpha test ([matl_alpha] 1) or a
            // blend ([matl_alpha] 2); otherwise it is the reflection mask and the surface
            // is opaque - treated as a cutout, the NL202's body (alpha 0.15) vanished and
            // left the dark interior and dirt layers showing through
            let alpha_mode = vm.overrides.iter().filter(|o| o.texture.eq_ignore_ascii_case(&tex_name) || o.texture.eq_ignore_ascii_case(mat.map(|m| m.texture.as_str()).unwrap_or(""))).map(|o| o.alpha).max().unwrap_or(0);
            let alpha_override = alpha_mode >= 2;
            let key = format!("{}|{}", tex_name.to_ascii_lowercase(), alpha_mode);
            let material = match material_index.get(&key) {
                Some(m) => *m,
                None => {
                    // `null.bmp` is the exporters' "no texture": the material colour alone
                    let img = if crate::is_null_texture(&tex_name) {
                        None
                    } else {
                        match image_index.get(&tex_name.to_ascii_lowercase()) {
                            Some(v) => *v,
                            None => {
                                let found = textures.get(&tex_name, &dirs_all).and_then(|img| {
                                    let mut png = Vec::new();
                                    let enc = image::codecs::png::PngEncoder::new(&mut png);
                                    use image::ImageEncoder;
                                    enc.write_image(&img.rgba, img.width, img.height, image::ExtendedColorType::Rgba8).ok()?;
                                    images.push((png, img.has_alpha));
                                    Some(images.len() - 1)
                                });
                                image_index.insert(tex_name.to_ascii_lowercase(), found);
                                found
                            }
                        }
                    };
                    let has_alpha = img.map(|k| images[k].1).unwrap_or(false);
                    let colour = mat.map(|m| m.diffuse).unwrap_or([1.0; 4]);
                    // with a texture its alpha alone counts (D3D's default stage), so the
                    // material's own alpha (0.7 on the Citaro's door leaves, 0 on its glass)
                    // does not make a surface see-through
                    let alpha = if img.is_some() { 1.0 } else { colour[3] };
                    materials.push((img, (has_alpha || img.is_none()) && alpha_override, [colour[0], colour[1], colour[2], if has_alpha && alpha_mode == 1 { -1.0 } else { alpha }]));
                    material_names.push(format!("{} (slot alpha {:.2})", tex_name, props.slot_alpha.get(slot).copied().unwrap_or(1.0)));
                    material_index.insert(key, materials.len() - 1);
                    materials.len() - 1
                }
            };
            // gather the vertices this range uses, re-indexed
            let mut remap: HashMap<u32, u32> = HashMap::new();
            let mut prim = Prim { positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(), indices: Vec::with_capacity(count), material, name: format!("{} [{slot}] a={:.2}", def.file, props.slot_alpha.get(slot).copied().unwrap_or(1.0)) };
            for &vi in &vm.data.indices[first..first + count] {
                let ni = match remap.get(&vi) {
                    Some(n) => *n,
                    None => {
                        let p = xf.transform_point3(vm.data.positions[vi as usize]);
                        let n = normal_xf.transform_vector3(vm.data.normals.get(vi as usize).copied().unwrap_or(Vec3::Z)).normalize_or_zero();
                        let uv = vm.data.uvs.get(vi as usize).copied().unwrap_or_default();
                        // glTF is y up, right-handed: x right, y up, z towards the viewer;
                        // the model frame is x right, y forward, z up
                        prim.positions.push([p.x, p.z, -p.y]);
                        prim.normals.push([n.x, n.z, -n.y]);
                        prim.uvs.push([uv.x, uv.y]);
                        let k = prim.positions.len() as u32 - 1;
                        remap.insert(vi, k);
                        k
                    }
                };
                prim.indices.push(ni);
            }
            // glTF's front faces are counter-clockwise; OMSI's arrive clockwise in the
            // model frame (see `omsi_geometry::mesh_from_o3d`)
            for tri in prim.indices.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
            prims.push(prim);
        }
    }
    }
    log::info!("glb: {} primitives, {} materials, {} textures ({skipped} meshes hidden)", prims.len(), materials.len(), images.len());
    // --- pack the binary chunk
    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut push_view = |bin: &mut Vec<u8>, bytes: &[u8], target: Option<u32>| -> usize {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let offset = bin.len();
        bin.extend_from_slice(bytes);
        let mut v = serde_json::json!({ "buffer": 0, "byteOffset": offset, "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = serde_json::json!(t);
        }
        buffer_views.push(v);
        buffer_views.len() - 1
    };
    for p in &prims {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for q in &p.positions {
            for c in 0..3 {
                lo[c] = lo[c].min(q[c]);
                hi[c] = hi[c].max(q[c]);
            }
        }
        let pv = push_view(&mut bin, bytemuck_cast(&p.positions), Some(34962));
        accessors.push(serde_json::json!({ "bufferView": pv, "componentType": 5126, "count": p.positions.len(), "type": "VEC3", "min": lo, "max": hi }));
        let a_pos = accessors.len() - 1;
        let nv = push_view(&mut bin, bytemuck_cast(&p.normals), Some(34962));
        accessors.push(serde_json::json!({ "bufferView": nv, "componentType": 5126, "count": p.normals.len(), "type": "VEC3" }));
        let a_nrm = accessors.len() - 1;
        let uv = push_view(&mut bin, bytemuck_cast2(&p.uvs), Some(34962));
        accessors.push(serde_json::json!({ "bufferView": uv, "componentType": 5126, "count": p.uvs.len(), "type": "VEC2" }));
        let a_uv = accessors.len() - 1;
        let iv = push_view(&mut bin, bytemuck_u32(&p.indices), Some(34963));
        accessors.push(serde_json::json!({ "bufferView": iv, "componentType": 5125, "count": p.indices.len(), "type": "SCALAR" }));
        let a_idx = accessors.len() - 1;
        meshes.push(serde_json::json!({ "name": p.name, "primitives": [{ "attributes": { "POSITION": a_pos, "NORMAL": a_nrm, "TEXCOORD_0": a_uv }, "indices": a_idx, "material": p.material }] }));
    }
    let mut gltf_images = Vec::new();
    for (png, _) in &images {
        let v = push_view(&mut bin, png, None);
        gltf_images.push(serde_json::json!({ "bufferView": v, "mimeType": "image/png" }));
    }
    let gltf_textures: Vec<serde_json::Value> = (0..images.len()).map(|i| serde_json::json!({ "source": i, "sampler": 0 })).collect();
    let gltf_materials: Vec<serde_json::Value> = materials
        .iter()
        .enumerate()
        .map(|(mi, (img, blended, colour))| {
            let mut m = serde_json::json!({
                "name": material_names.get(mi).cloned().unwrap_or_default(),
                "pbrMetallicRoughness": { "baseColorFactor": [colour[0], colour[1], colour[2], colour[3].abs().max(0.05)], "metallicFactor": 0.0, "roughnessFactor": 0.75 },
                // OMSI draws one side only: inside-out click spots (the SD202's front flap
                // over its right headlight) stay invisible
                "doubleSided": false,
                "alphaMode": if *blended { "BLEND" } else if colour[3] < 0.0 { "MASK" } else { "OPAQUE" },
                "alphaCutoff": 0.5
            });
            if let Some(k) = img {
                m["pbrMetallicRoughness"]["baseColorTexture"] = serde_json::json!({ "index": k });
            }
            m
        })
        .collect();
    let nodes: Vec<serde_json::Value> = (0..meshes.len()).map(|i| serde_json::json!({ "mesh": i })).collect();
    let json = serde_json::json!({
        "asset": { "version": "2.0", "generator": "openOMSI" },
        "scene": 0,
        "scenes": [{ "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": gltf_materials,
        "textures": gltf_textures,
        "images": gltf_images,
        "samplers": [{ "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497 }],
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{ "byteLength": bin.len() }]
    });
    let mut json_bytes = serde_json::to_vec(&json)?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut file: Vec<u8> = Vec::with_capacity(total);
    file.extend_from_slice(b"glTF");
    file.extend_from_slice(&2u32.to_le_bytes());
    file.extend_from_slice(&(total as u32).to_le_bytes());
    file.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    file.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
    file.extend_from_slice(&json_bytes);
    file.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    file.extend_from_slice(&0x004E4942u32.to_le_bytes());
    file.extend_from_slice(&bin);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    std::fs::write(out, &file).with_context(|| format!("writing {}", out.display()))?;
    log::info!("wrote {} ({:.1} MB)", out.display(), file.len() as f64 / 1e6);
    Ok(())
}

fn bytemuck_cast(v: &[[f32; 3]]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 12) }
}
fn bytemuck_cast2(v: &[[f32; 2]]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 8) }
}
fn bytemuck_u32(v: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 4) }
}
