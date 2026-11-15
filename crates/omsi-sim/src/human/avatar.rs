//! A human type made from a rigged avatar of another tool (an FBX or glTF with a 3ds Max
//! Biped skeleton: the Microsoft Rocketbox avatars), drawn in place of OMSI's people when
//! the player wants it. It becomes an ordinary [`HumanType`] - the same rig, the same
//! posing, OMSI's animation or the procedural one - so that nothing downstream knows:
//!
//! - the model is turned to face +y (the avatars face the file's +z) and its arms are
//!   brought from the A-pose it was made in to the T-pose OMSI's people stand in (the
//!   arm's skin follows by its own weights);
//! - the Biped's bones are given to OMSI's thirteen (spine, clavicles and neck to the torso,
//!   calf, foot and toes to the shin, hand and fingers to the hand, the face to the head);
//! - the `[links]` joints, height and seat height are measured from the skeleton;
//! - its blend shapes become a face (see [`super::face`]).

use super::face::FaceRig;
use super::rig::{fit_leg_joints, influences, split_feet, HumanMesh, HumanType, Joints, Rig};
use super::*;
use glam::Quat;

/// Who an avatar is, as the pack lists it.
#[derive(Debug, Clone, Default)]
pub struct AvatarInfo {
    pub id: String,
    pub female: bool,
    pub child: bool,
}

/// OMSI's bone id for a Biped bone, by name; `in_head`: the bone hangs off the head (the
/// face's bones).
fn omsi_bone(name: &str, in_head: bool) -> i32 {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_prefix("bip01").or_else(|| n.strip_prefix("bip001")).unwrap_or(&n).trim();
    let side = |l: i32, r: i32| -> Option<i32> {
        if n.starts_with("l ") {
            Some(l)
        } else if n.starts_with("r ") {
            Some(r)
        } else {
            None
        }
    };
    let part = n.get(2..).unwrap_or("");
    if in_head || n == "head" {
        return BONE_HEAD;
    }
    if part.starts_with("thigh") {
        if let Some(b) = side(BONE_OS_L, BONE_OS_R) {
            return b;
        }
    }
    if part.starts_with("calf") || part.starts_with("foot") || part.starts_with("toe") {
        if let Some(b) = side(BONE_US_L, BONE_US_R) {
            return b;
        }
    }
    if part.starts_with("upperarm") {
        if let Some(b) = side(BONE_OA_L, BONE_OA_R) {
            return b;
        }
    }
    if part.starts_with("forearm") {
        if let Some(b) = side(BONE_UA_L, BONE_UA_R) {
            return b;
        }
    }
    if part.starts_with("hand") || part.starts_with("finger") {
        if let Some(b) = side(BONE_HAND_L, BONE_HAND_R) {
            return b;
        }
    }
    if n.is_empty() || n.starts_with("pelvis") {
        return BONE_HIP;
    }
    // spine, clavicles, neck, and whatever else the torso carries
    BONE_MAIN
}

/// The arm chain of one side (`l` or `r`): upper arm, forearm, hand, fingers.
fn in_arm(name: &str, side: char) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_prefix("bip01 ").or_else(|| n.strip_prefix("bip001 ")).unwrap_or(&n);
    let Some(rest) = n.strip_prefix(side).and_then(|r| r.strip_prefix(' ')) else { return false };
    ["upperarm", "forearm", "hand", "finger"].iter().any(|p| rest.starts_with(p))
}

impl HumanType {
    /// Load an avatar (`.fbx`, `.glb`) as a human type.
    pub fn load_avatar(path: &Path, info: &AvatarInfo) -> Result<HumanType> {
        let mut m = omsi_o3d::load_mesh(path).with_context(|| format!("loading {}", path.display()))?;
        if m.joints.is_empty() || m.bones.is_empty() {
            anyhow::bail!("{}: no skeleton", path.display());
        }
        // a half turn about the vertical: the avatar faces the file's +z, OMSI's people +y
        let turn = |v: Vec3| Vec3::new(-v.x, v.y, -v.z);
        for v in &mut m.vertices {
            v.position = turn(v.position);
            v.normal = turn(v.normal);
        }
        for j in &mut m.joints {
            j.position = turn(j.position);
        }
        // the mesh frame's y and z swapped, as `mesh_from_o3d` does
        let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
        let joint = |name: &str| -> Option<Vec3> {
            m.joints.iter().find(|j| j.name.to_ascii_lowercase().replace("bip001", "bip01") == name).map(|j| swap(j.position))
        };
        let mut data = mesh_from_o3d(&m);
        // --- A-pose to T-pose: each arm turned about its shoulder until it points sideways
        let mut arm_t = [Quat::IDENTITY; 2];
        let mut joints_t: Vec<(String, Vec3)> = m.joints.iter().map(|j| (j.name.to_ascii_lowercase().replace("bip001", "bip01"), swap(j.position))).collect();
        for (side, sign) in [('l', -1.0f32), ('r', 1.0)] {
            let (Some(sh), Some(hand)) = (joint(&format!("bip01 {side} upperarm")), joint(&format!("bip01 {side} hand"))) else { continue };
            let dir = (hand - sh).normalize_or_zero();
            if dir == Vec3::ZERO {
                continue;
            }
            let q = Quat::from_rotation_arc(dir, Vec3::X * sign);
            arm_t[(side == 'r') as usize] = q;
            let mut w = vec![0.0f32; data.positions.len()];
            for b in m.bones.iter().filter(|b| in_arm(&b.name, side)) {
                for bw in &b.weights {
                    if let Some(x) = w.get_mut(bw.vertex as usize) {
                        *x += bw.weight;
                    }
                }
            }
            for (i, wi) in w.iter().enumerate() {
                let wi = wi.clamp(0.0, 1.0);
                if wi <= 0.0 {
                    continue;
                }
                let p = data.positions[i];
                let turned = sh + q * (p - sh);
                data.positions[i] = p + (turned - p) * wi;
                let n = data.normals[i];
                data.normals[i] = (n + (q * n - n) * wi).normalize_or_zero();
            }
            for (name, pos) in joints_t.iter_mut() {
                if in_arm(name, side) {
                    *pos = sh + q * (*pos - sh);
                }
            }
        }
        let jt = |name: &str| joints_t.iter().find(|(n, _)| n == name).map(|(_, p)| *p);
        // --- the bones as OMSI's
        let head_ix = m.joints.iter().position(|j| j.name.to_ascii_lowercase().ends_with(" head"));
        let in_head = |name: &str| -> bool {
            let Some(h) = head_ix else { return false };
            let mut k = m.joints.iter().position(|j| j.name == name);
            // (the head itself counts as the head's)
            let mut first = true;
            while let Some(i) = k {
                if i == h {
                    return !first;
                }
                first = false;
                k = m.joints[i].parent;
            }
            false
        };
        let bones: Vec<(i32, Vec<(u32, f32)>)> = m.bones.iter().map(|b| (omsi_bone(&b.name, in_head(&b.name)), b.weights.iter().map(|w| (w.vertex, w.weight)).collect())).collect();
        let skin = influences(&data, &bones);
        // hair, eyelashes: alpha-tested
        let alpha = m.materials.iter().map(|mat| if mat.texture.to_ascii_lowercase().contains("opacity") || mat.texture.to_ascii_lowercase().contains("hair") { 1 } else { 0 }).collect();
        // --- the face: only the blend shapes it uses are kept, in the model frame
        let face = FaceRig::from_shapes(m.morphs.iter().map(|s| s.name.as_str()));
        let (face, morphs) = match face {
            Some(rig) => {
                let mut kept = Vec::new();
                let mut remap = rig.clone();
                for (slot, s) in rig.shape.iter().enumerate() {
                    let Some(s) = s else { continue };
                    let src = &m.morphs[*s as usize];
                    // (a shape already kept for another channel is shared)
                    let k = match kept.iter().position(|k: &omsi_o3d::Morph| k.name == src.name) {
                        Some(k) => k,
                        None => {
                            kept.push(omsi_o3d::Morph {
                                name: src.name.clone(),
                                offsets: src.offsets.iter().filter(|o| o.1.length_squared() > 1e-10).map(|(v, p, n)| (*v, swap(turn(*p)), swap(turn(*n)))).collect(),
                            });
                            kept.len() - 1
                        }
                    };
                    remap.shape[slot] = Some(k as u16);
                }
                (Some(remap), kept)
            }
            None => (None, Vec::new()),
        };
        let mesh = HumanMesh { data, materials: m.materials.clone(), bones, skin, alpha, morphs };
        // --- the `[links]` and sizes from the skeleton (right side, model frame)
        let height = mesh.data.positions.iter().map(|p| p.z).fold(0.0f32, f32::max).max(0.8);
        let scale = height / 1.75;
        let hip = jt("bip01 r thigh").unwrap_or(Vec3::new(0.09, 0.0, 0.53 * height));
        let knee = jt("bip01 r calf").unwrap_or(hip - Vec3::Z * 0.42 * scale);
        let waist = jt("bip01 spine1").or(jt("bip01 spine")).unwrap_or(Vec3::new(0.0, 0.0, 0.66 * height));
        let shoulder = jt("bip01 r upperarm").unwrap_or(Vec3::new(0.17 * scale, 0.0, 0.81 * height));
        let elbow = jt("bip01 r forearm").unwrap_or(shoulder + Vec3::X * 0.27 * scale);
        let hand = jt("bip01 r hand").unwrap_or(elbow + Vec3::X * 0.25 * scale);
        let neck = jt("bip01 head").or(jt("bip01 neck")).unwrap_or(Vec3::new(0.0, 0.0, 0.87 * height));
        // the finger tips: the right hand's farthest point along the arm
        let mut finger = hand + Vec3::X * 0.18 * scale;
        let rh = slot_of(BONE_HAND_R).unwrap_or(HAND[1]);
        let tip = mesh.skin.iter().enumerate().filter(|(_, inf)| (0..inf.n as usize).any(|k| inf.slot[k] as usize == rh && inf.weight[k] > 0.5)).map(|(i, _)| mesh.data.positions[i]).max_by(|a, b| a.x.total_cmp(&b.x));
        if let Some(t) = tip {
            if t.x > hand.x + 0.08 * scale {
                finger = Vec3::new(t.x, hand.y, hand.z);
            }
        }
        let links = vec![
            hip.x, hip.y, hip.z, knee.x, knee.y, knee.z, waist.y, waist.z, shoulder.x, shoulder.y, shoulder.z, elbow.x, elbow.y, elbow.z, neck.y, neck.z, hand.x, hand.y, hand.z, finger.x, finger.y, finger.z,
        ];
        // (as OMSI's people: the seat 8-9 cm under the hip joint, strides and swing by kind)
        let walk_param = if info.child {
            [1.0, 70.0, 0.7, 0.5, 0.0]
        } else if info.female {
            [1.2, 83.0, 0.5, 2.0, 0.2]
        } else {
            [1.4, 66.0, 1.0, 1.0, 0.0]
        };
        let def = Human {
            path: path.to_path_buf(),
            model: String::new(),
            seat_height: hip.z - 0.085 * scale,
            feet_dist: if info.female { 0.1 } else { 0.04 },
            height,
            links,
            voice: if info.female { "F1" } else { "M1" }.into(),
            walk_param,
            mass: if info.child { 0.04 } else { 0.07 + 0.01 * (!info.female) as u8 as f32 },
            age: info.child.then_some(10),
        };
        let mut meshes = vec![mesh];
        let mut joints = Joints::from_links(&def.links);
        fit_leg_joints(&mut joints, &meshes, path);
        let rig = Rig::measure(&def, &joints, &meshes);
        for m in meshes.iter_mut() {
            split_feet(m, &rig);
        }
        let model_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        Ok(HumanType {
            omsi: crate::human_omsi::OmsiRig::new(&def),
            joints,
            rig,
            def,
            model: omsi_model::Model::default(),
            model_dir: model_dir.clone(),
            meshes,
            variants: Vec::new(),
            extra_dirs: vec![model_dir],
            face,
            biped: path.extension().is_some_and(|e| e.eq_ignore_ascii_case("fbx")).then(|| super::clips::Biped::from_joints(&m.joints, arm_t)).flatten().map(Box::new),
        })
    }

    /// The face's blend shapes applied to the rest pose of mesh `k` with these channel
    /// weights (see [`super::face::Face`]): positions and normals for [`skin_from`]; None
    /// for a type without a face or a face at rest.
    pub fn face_rest(&self, k: usize, weights: &[f32; super::face::CHANNELS], out: &mut (Vec<Vec3>, Vec<Vec3>)) -> bool {
        let (Some(face), Some(m)) = (self.face.as_ref(), self.meshes.get(k)) else { return false };
        if k != 0 || weights.iter().all(|w| *w < 0.003) {
            return false;
        }
        out.0.clear();
        out.0.extend_from_slice(&m.data.positions);
        out.1.clear();
        out.1.extend_from_slice(&m.data.normals);
        // (a shape two channels share counts once, with the larger weight)
        let mut per_shape = vec![0.0f32; m.morphs.len()];
        for (c, s) in face.shape.iter().enumerate() {
            if let Some(s) = s {
                if let Some(w) = per_shape.get_mut(*s as usize) {
                    *w = w.max(weights[c]);
                }
            }
        }
        for (s, w) in per_shape.iter().enumerate() {
            if *w < 0.003 {
                continue;
            }
            for (v, dp, dn) in &m.morphs[s].offsets {
                let v = *v as usize;
                if v < out.0.len() {
                    out.0[v] += *dp * *w;
                    out.1[v] += *dn * *w;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biped_bones_become_omsi_bones() {
        assert_eq!(omsi_bone("Bip01 L Thigh", false), BONE_OS_L);
        assert_eq!(omsi_bone("Bip01 R Calf", false), BONE_US_R);
        assert_eq!(omsi_bone("Bip01 R Toe0", false), BONE_US_R);
        assert_eq!(omsi_bone("Bip01 L UpperArm", false), BONE_OA_L);
        assert_eq!(omsi_bone("Bip01 R Forearm", false), BONE_UA_R);
        assert_eq!(omsi_bone("Bip01 L Finger21", false), BONE_HAND_L);
        assert_eq!(omsi_bone("Bip01 Pelvis", false), BONE_HIP);
        assert_eq!(omsi_bone("Bip01 Spine2", false), BONE_MAIN);
        assert_eq!(omsi_bone("Bip01 L Clavicle", false), BONE_MAIN);
        assert_eq!(omsi_bone("Bip01 Head", false), BONE_HEAD);
        assert_eq!(omsi_bone("Bip01 LEyeBlinkTop", true), BONE_HEAD);
        assert!(in_arm("Bip01 R Finger02", 'r') && !in_arm("Bip01 R Clavicle", 'r') && !in_arm("Bip01 L Hand", 'r'));
    }

    /// A Rocketbox avatar (`OMSI_TEST_AVATAR=<…_facial.fbx>`): T-posed, measured, with a face.
    #[test]
    #[ignore]
    fn a_rocketbox_avatar_loads() {
        let Ok(p) = std::env::var("OMSI_TEST_AVATAR") else { return };
        let t = HumanType::load_avatar(Path::new(&p), &AvatarInfo { id: "test".into(), female: true, child: false }).unwrap();
        let r = &t.rig;
        eprintln!("height {:.2} hip {:?} shoulder {:?} wrist {:?} sole {:.3} seat {:.2}", t.def.height, r.hip[1], r.shoulder[1], r.wrist[1], r.sole, t.def.seat_height);
        // arms out sideways at shoulder height
        assert!((r.wrist[1].z - r.shoulder[1].z).abs() < 0.05, "{:?} {:?}", r.wrist[1], r.shoulder[1]);
        assert!(r.wrist[1].x > 0.45 && r.wrist[0].x < -0.45);
        // the right side is +x, the toes point +y
        assert!(r.hip[1].x > 0.0);
        assert!(r.ball_joint(1).y > r.ankle[1].y);
        assert!(t.face.is_some());
        let w = [0.5f32; super::super::face::CHANNELS];
        let mut out = (Vec::new(), Vec::new());
        assert!(t.face_rest(0, &w, &mut out));
        let moved = out.0.iter().zip(&t.meshes[0].data.positions).filter(|(a, b)| (**a - **b).length() > 1e-4).count();
        assert!(moved > 100, "{moved}");
    }
}
