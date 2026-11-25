//! Skinning: the meshes deformed by the bones, and the hands curled round a bar.

use super::*;

/// The bone transforms of [`skin`] from Omsi.exe's thirteen bones
/// ([`crate::human_omsi::OmsiAnim::bones`]): the feet and toes, which the original does not
/// have, go with the shins they were split off.
pub fn slots_from_omsi(b: &[Affine3A; crate::human_omsi::BONES]) -> [Affine3A; SLOTS] {
    let mut out = [Affine3A::IDENTITY; SLOTS];
    out[..crate::human_omsi::BONES].copy_from_slice(b);
    for side in 0..2 {
        out[FOOT[side]] = b[SHIN[side]];
        out[TOE[side]] = b[SHIN[side]];
    }
    out
}

/// Deform `mesh` with the bone transforms (linear blend skinning).
pub fn skin(
    mesh: &HumanMesh,
    bones: &[Affine3A; SLOTS],
    out_pos: &mut Vec<Vec3>,
    out_nrm: &mut Vec<Vec3>,
) {
    let n = mesh.data.positions.len();
    out_pos.clear();
    out_nrm.clear();
    out_pos.reserve(n);
    out_nrm.reserve(n);
    for (i, inf) in mesh.skin.iter().enumerate().take(n) {
        let p = Vec3A::from(mesh.data.positions[i]);
        let nr = Vec3A::from(mesh.data.normals[i]);
        if inf.n <= 1 {
            let m = &bones[inf.slot[0] as usize];
            out_pos.push(m.transform_point3a(p).into());
            out_nrm.push(m.transform_vector3a(nr).into());
            continue;
        }
        let mut mat = Mat3A::ZERO;
        let mut tr = Vec3A::ZERO;
        for k in 0..inf.n as usize {
            let m = &bones[inf.slot[k] as usize];
            let w = inf.weight[k];
            mat += m.matrix3 * w;
            tr += m.translation * w;
        }
        out_pos.push((mat * p + tr).into());
        out_nrm.push((mat * nr).normalize_or_zero().into());
    }
}

/// The hands of a human type closed round a bar of `radius` (m; a steering wheel's rim):
/// every vertex of a hand beyond the knuckles is bent round an axis across the palm, as far
/// round as its distance from the knuckles reaches, so that the fingers wrap the bar from
/// the back of the hand to the palm. The rest pose stays: the hands lie palm down along the
/// arms, the thumbs (which sit before the knuckles) are left as they are. Returns positions
/// and normals per mesh, for [`skin_from`].
pub fn curl_hands(ty: &HumanType, radius: f32) -> Vec<(Vec<Vec3>, Vec<Vec3>)> {
    let rig = &ty.rig;
    let hand_len = (ty.joints.finger - ty.joints.hand)
        .length()
        .clamp(0.12, 0.3);
    let mut out: Vec<(Vec<Vec3>, Vec<Vec3>)> = ty
        .meshes
        .iter()
        .map(|m| (m.data.positions.clone(), m.data.normals.clone()))
        .collect();
    for side in 0..2 {
        let slot = HAND[side] as u8;
        let w = rig.wrist[side];
        let u = (rig.wrist[side] - rig.elbow[side]).normalize_or(Vec3::X * SIDE[side]);
        let p = -Vec3::Z;
        let kd = hand_len * 0.58;
        let owned = |inf: &Influence| {
            (0..inf.n as usize).any(|k| inf.slot[k] == slot && inf.weight[k] > 0.5)
        };
        // the fingers' middle plane: their average height
        let (mut zs, mut n) = (0.0f32, 0.0f32);
        for m in &ty.meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                let v = m.data.positions[i];
                if owned(inf) && (v - w).dot(u) > kd {
                    zs += v.z;
                    n += 1.0;
                }
            }
        }
        if n < 3.0 {
            continue;
        }
        let mut knuckle = w + u * kd;
        knuckle.z = zs / n;
        let centre = knuckle + p * radius;
        let axis = u.cross(p).normalize_or(Vec3::Y);
        // the fingers' spread (the rest hand's fingers are apart) and where the thumb begins:
        // across the hand, the thumb's side positive
        let side_sign = SIDE[side];
        let (mut c_sum, mut c_n, mut c_hi) = (0.0f32, 0.0f32, f32::MIN);
        for m in &ty.meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                let rel = m.data.positions[i] - knuckle;
                if owned(inf) && rel.dot(u) > 0.01 {
                    let c = rel.dot(axis) * side_sign;
                    c_sum += c;
                    c_n += 1.0;
                    c_hi = c_hi.max(c);
                }
            }
        }
        let c_mid = if c_n > 0.0 { c_sum / c_n } else { 0.0 };
        let scale = hand_len / 0.19;
        let thumb_root = c_hi - 0.015 * scale;
        const MAX_TURN: f32 = 3.2;
        // a fist's fingers lie together; the thumb swings in under the bar
        const SQUEEZE: f32 = 0.55;
        const THUMB_TURN: f32 = 55.0;
        for (k, m) in ty.meshes.iter().enumerate() {
            for (i, inf) in m.skin.iter().enumerate() {
                if !owned(inf) {
                    continue;
                }
                let rel = m.data.positions[i] - knuckle;
                let s = rel.dot(u);
                let h = rel.dot(p);
                let c = rel.dot(axis) * side_sign;
                let thumb = smoothstep(thumb_root, thumb_root + 0.02 * scale, c)
                    * (1.0 - smoothstep(-0.005 * scale, 0.012 * scale, s));
                if thumb > 0.0 {
                    let t = (THUMB_TURN * thumb).to_radians();
                    let (dc, dh) = (c - thumb_root, h);
                    let (c2, h2) = (
                        thumb_root + dc * t.cos() - dh * t.sin(),
                        dc * t.sin() + dh * t.cos(),
                    );
                    out[k].0[i] = knuckle + u * s + p * h2 + axis * (c2 * side_sign);
                    // (axis = u x p: a turn about u takes axis towards -p)
                    out[k].1[i] = Quat::from_axis_angle(u, -t * side_sign) * m.data.normals[i];
                    continue;
                }
                if s <= 0.0 {
                    continue;
                }
                let squeeze = 1.0 - (1.0 - SQUEEZE) * smoothstep(0.0, 0.03 * scale, s);
                let c = c_mid + (c - c_mid) * squeeze;
                let across = axis * (c * side_sign);
                let phi = (s / radius).min(MAX_TURN);
                let extra = (s - MAX_TURN * radius).max(0.0);
                let r = (radius - h).max(0.003);
                let tangent = u * phi.cos() + p * phi.sin();
                out[k].0[i] =
                    centre - p * (r * phi.cos()) + u * (r * phi.sin()) + tangent * extra + across;
                out[k].1[i] = Quat::from_axis_angle(axis, phi) * m.data.normals[i];
            }
        }
    }
    out
}

/// Where the fingers that [`curl_hands`] closed round a bar of `radius` hold it: the bar's
/// centre in the rest frame (left, right), for a caller that wants the bar exactly there.
pub fn grip_centres(ty: &HumanType, radius: f32) -> [Option<Vec3>; 2] {
    let rig = &ty.rig;
    let hand_len = (ty.joints.finger - ty.joints.hand)
        .length()
        .clamp(0.12, 0.3);
    [0, 1].map(|side| {
        let slot = HAND[side] as u8;
        let w = rig.wrist[side];
        let u = (rig.wrist[side] - rig.elbow[side]).normalize_or(Vec3::X * SIDE[side]);
        let kd = hand_len * 0.58;
        let owned = |inf: &Influence| {
            (0..inf.n as usize).any(|k| inf.slot[k] == slot && inf.weight[k] > 0.5)
        };
        let (mut zs, mut n) = (0.0f32, 0.0f32);
        for m in &ty.meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                let v = m.data.positions[i];
                if owned(inf) && (v - w).dot(u) > kd {
                    zs += v.z;
                    n += 1.0;
                }
            }
        }
        if n < 3.0 {
            return None;
        }
        let mut knuckle = w + u * kd;
        knuckle.z = zs / n;
        Some(knuckle - Vec3::Z * radius)
    })
}

/// The bone slot of a hand (0 left, 1 right) in [`Posed::bones`].
pub fn hand_slot(side: usize) -> usize {
    HAND[side.min(1)]
}

/// [`skin`] of `mesh` with its positions and normals replaced (see [`curl_hands`]).
pub fn skin_from(
    mesh: &HumanMesh,
    rest: &(Vec<Vec3>, Vec<Vec3>),
    bones: &[Affine3A; SLOTS],
    out_pos: &mut Vec<Vec3>,
    out_nrm: &mut Vec<Vec3>,
) {
    let n = mesh.data.positions.len().min(rest.0.len());
    out_pos.clear();
    out_nrm.clear();
    for (i, inf) in mesh.skin.iter().enumerate().take(n) {
        let p = Vec3A::from(rest.0[i]);
        let nr = Vec3A::from(rest.1[i]);
        let mut mat = Mat3A::ZERO;
        let mut tr = Vec3A::ZERO;
        for k in 0..inf.n.max(1) as usize {
            let m = &bones[inf.slot[k] as usize];
            let w = if inf.n <= 1 { 1.0 } else { inf.weight[k] };
            mat += m.matrix3 * w;
            tr += m.translation * w;
        }
        out_pos.push((mat * p + tr).into());
        out_nrm.push((mat * nr).normalize_or_zero().into());
    }
}
