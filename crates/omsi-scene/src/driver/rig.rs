//! Finding what the driver's hands take in the bus's model: the steering wheel (its mesh,
//! axis, centre and rim) and the gear lever (its knob and foot).

use super::*;

/// The foot of the lever and where a hand holds its knob: the knob is the far end of the mesh
/// from `pivot` (the lever's turning axis; without one, the lowest part of the mesh), a little
/// below the very tip; the lever's axis is the line from the foot to the knob.
pub(super) fn lever_knob(positions: &[Vec3], pivot: Option<Vec3>) -> Option<(Vec3, Vec3)> {
    if positions.len() < 8 {
        return None;
    }
    let base = pivot.unwrap_or_else(|| {
        let mut zs: Vec<f32> = positions.iter().map(|p| p.z).collect();
        zs.sort_by(|a, b| a.total_cmp(b));
        let z = zs[zs.len() / 10];
        let low: Vec<&Vec3> = positions.iter().filter(|p| p.z <= z).collect();
        low.iter().fold(Vec3::ZERO, |s, p| s + **p) / low.len().max(1) as f32
    });
    let max = positions.iter().map(|p| (*p - base).length()).fold(0.0f32, f32::max);
    let (tip, axis) = if max >= 0.06 {
        let far: Vec<&Vec3> = positions.iter().filter(|p| (**p - base).length() >= max * 0.9).collect();
        let tip = far.iter().fold(Vec3::ZERO, |s, p| s + **p) / far.len().max(1) as f32;
        (tip, (tip - base).normalize_or(Vec3::Z))
    } else {
        // The pivot sits in the knob itself: the knob is the top of the mesh.
        let mut zs: Vec<f32> = positions.iter().map(|p| p.z).collect();
        zs.sort_by(|a, b| a.total_cmp(b));
        let z = zs[(zs.len() - 1) * 9 / 10];
        let top: Vec<&Vec3> = positions.iter().filter(|p| p.z >= z).collect();
        (top.iter().fold(Vec3::ZERO, |s, p| s + **p) / top.len().max(1) as f32, Vec3::Z)
    };
    if !tip.is_finite() || !axis.is_finite() {
        return None;
    }
    Some((tip - axis * 0.02, axis))
}

/// The gear lever of a manual bus: the mesh near the seat that a gear/shift variable animates
/// (or, failing that, one whose file name says it is a gear lever), the one best named and
/// nearest winning. `OMSI_DRIVER_SHIFTER=<part of a variable or file name>` forces the pick.
/// The hand that works it is the one on the lever's side of the seat (the lever stands right
/// of a left-hand-drive driver and left of a right-hand-drive one).
pub(super) fn find_shifter(v: &impl DriverCar, hip: Vec3, heading: f32) -> Option<Shifter> {
    const STRONG: &[&str] = &[
        "gearlever",
        "gear_lever",
        "gearshift",
        "gear_shift",
        "shiftlever",
        "shift_lever",
        "shifter",
        "gearstick",
        "gear_stick",
        "schalthebel",
        "schaltknueppel",
        "schaltung",
        "antriebshebel",
        "antriebhebel",
        "antrieb_hebel",
    ];
    // "Antrieb" is what OMSI models usually call the gearbox / its lever.
    const WEAK: &[&str] = &["antrieb", "gear", "shift", "schalt", "getriebe"];
    const NOT: &[&str] = &["light", "lamp", "display", "indic", "sound", "retard", "park", "door", "wiper", "text", "warn", "oil", "temp", "rpm", "tacho", "taster", "button", "btn"];
    let forced = omsi_cfg::flags::OMSI_DRIVER_SHIFTER
        .var()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    // OMSI_DRIVER_SHIFTER=off: no lever, both hands on the wheel
    if matches!(forced.as_deref(), Some("off") | Some("none") | Some("0")) {
        return None;
    }
    // How well a variable name says "gear lever": 0 not at all .. 3 forced.
    let grade = |name: &str| -> u8 {
        let n = name.to_ascii_lowercase();
        if let Some(f) = &forced {
            return if n.contains(f.as_str()) { 3 } else { 0 };
        }
        if NOT.iter().any(|w| n.contains(w)) {
            0
        } else if STRONG.iter().any(|w| n.contains(w)) {
            2
        } else if WEAK.iter().any(|w| n.contains(w)) {
            1
        } else {
            0
        }
    };
    let positions_of = |i: usize| -> Vec<Vec3> {
        let vm = &v.ty().meshes[i];
        let have: &[Vec3] = &vm.data.positions;
        if have.len() >= 12 {
            have.to_vec()
        } else {
            omsi_o3d::load_mesh(&vm.file)
                .ok()
                .map(|m| omsi_geometry::mesh_from_o3d(&m).positions)
                .unwrap_or_default()
        }
    };
    let shoulder = hip + Vec3::Z * 0.5;
    let mut best: Option<(f32, usize, Vec<String>, Vec3, Vec3)> = None;
    for (i, vm) in v.ty().meshes.iter().enumerate() {
        let def = &v.ty().model.meshes[vm.def_index];
        let mut grade_of_mesh = 0u8;
        let mut pivot: Option<Vec3> = None;
        let mut vars: Vec<String> = Vec::new();
        for a in &def.animations {
            let g = grade(&a.variable);
            if g == 0 {
                continue;
            }
            grade_of_mesh = grade_of_mesh.max(g);
            if pivot.is_none() {
                let origin = omsi_sim::anim::origin_matrix(&a.origins, vm.pivot);
                pivot = Some(origin.transform_point3(Vec3::ZERO));
            }
            if !vars.contains(&a.variable) {
                vars.push(a.variable.clone());
            }
        }
        if grade_of_mesh == 0 {
            // No variable says so: the mesh's own file name may.
            let file = format!("{:?}", vm.file).to_ascii_lowercase();
            let by_file = match &forced {
                Some(f) => file.contains(f.as_str()),
                None => STRONG.iter().any(|w| file.contains(w)) || file.contains("antrieb"),
            };
            if !by_file {
                continue;
            }
            grade_of_mesh = 1;
        }
        let positions = positions_of(i);
        if positions.len() < 8 {
            continue;
        }
        let centroid = positions.iter().fold(Vec3::ZERO, |s, p| s + *p) / positions.len() as f32;
        let dist = (centroid - hip).length();
        if dist > 1.4 {
            continue;
        }
        let Some((grab, axis)) = lever_knob(&positions, pivot) else { continue };
        if (grab - shoulder).length() > LEVER_REACH {
            continue;
        }
        let score = grade_of_mesh as f32 * 2.0 - dist;
        if best.as_ref().map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, i, vars, grab, axis));
        }
    }
    let (_, mesh, mut vars, grab, axis) = best?;
    for n in GEAR_VARS {
        if v.var(n).is_some() && !vars.iter().any(|x| x == n) {
            vars.push(n.to_string());
        }
    }
    let clutch = CLUTCH_VARS.iter().find(|n| v.var(n).is_some()).map(|n| n.to_string());
    let h = heading.to_radians();
    let d = grab - hip;
    let side = d.x * h.cos() - d.y * h.sin();
    let hand = if side >= 0.0 { 1 } else { 0 };
    Some(Shifter { mesh, vars, clutch, grab, axis, hand })
}

/// The steering wheel: the mesh turned by `Axle_Steering_*` with the largest factor (a
/// road wheel turns by the steering angle itself, a steering wheel by 15-20 times it) that
/// lies within arm's reach of the seat.
pub(super) fn find_wheel(v: &impl DriverCar, hip: Vec3) -> Option<Wheel> {
    let mut best: Option<(f32, usize, usize, Mat4, String, f32)> = None;
    for (i, vm) in v.ty().meshes.iter().enumerate() {
        let def = &v.ty().model.meshes[vm.def_index];
        for (j, a) in def.animations.iter().enumerate() {
            if !a.variable.to_ascii_lowercase().starts_with("axle_steering")
                || a.factor.abs() < 200.0
                || a.kind != Some(omsi_model::AnimKind::Rot)
            {
                continue;
            }
            let origin = omsi_sim::anim::origin_matrix(&a.origins, vm.pivot);
            let c = origin.transform_point3(Vec3::ZERO);
            if (c - hip).length() > 1.2 {
                continue;
            }
            if best.as_ref().map(|b| a.factor.abs() > b.0).unwrap_or(true) {
                best = Some((a.factor.abs(), i, j, origin, a.variable.clone(), a.factor));
            }
        }
    }
    let (_, mesh, anim, origin, var, factor) = best?;
    let origin_point = origin.transform_point3(Vec3::ZERO);
    let centre = origin_point;
    let mut axis = origin.transform_vector3(Vec3::X).normalize_or_zero();
    // the axis points at the driver: at his shoulders, not his hips - a bus's wheel lies
    // nearly flat at the height of the hips, whose direction then says nothing (the SD202's
    // axis came out pointing down and the hands held the air under the rim); a wheel that
    // lies anywhere near flat faces up
    let shoulders = hip + Vec3::Z * 0.5;
    if (axis.z.abs() > 0.4 && axis.z < 0.0) || (axis.z.abs() <= 0.4 && axis.dot(shoulders - centre) < 0.0) {
        axis = -axis;
    }
    let mut up = (Vec3::Z - axis * axis.dot(Vec3::Z)).normalize_or_zero();
    if up.length_squared() < 0.5 {
        up = (Vec3::Y - axis * axis.dot(Vec3::Y)).normalize_or_zero();
    }
    // right as the driver sees it: facing the wheel along -axis
    let right = up.cross(axis).normalize_or_zero();
    let right = if right.x < 0.0 { -right } else { right };
    // the rim: the ring the farthest vertices form round the axis (a spoke or the hub is
    // nearer; a few stray vertices are left out). An AI copy of a bus keeps no vertices on
    // the CPU: its wheel is read from the file (a guessed size left the AI drivers' hands
    // floating over the rim).
    let vm = &v.ty().meshes[mesh];
    let loaded;
    let positions: &[Vec3] = if vm.data.positions.len() >= 12 {
        &vm.data.positions
    } else {
        loaded = omsi_o3d::load_mesh(&vm.file)
            .ok()
            .map(|m| omsi_geometry::mesh_from_o3d(&m).positions)
            .unwrap_or_default();
        &loaded
    };
    let radius_of = |p: &Vec3| {
        let d = *p - centre;
        (d - axis * axis.dot(d)).length()
    };
    let mut radii: Vec<f32> = positions.iter().map(radius_of).collect();
    radii.sort_by(|a, b| a.total_cmp(b));
    // (an AI copy of a bus keeps no vertices on the CPU: a bus wheel's size then)
    let rim = if radii.len() < 12 { 0.26 } else { radii[(radii.len() as f32 * 0.93) as usize] };
    // The rim's cross-section, from the ring of its vertices (a spoke or the hub is nearer
    // the axis): the middle of the tube across and along the axis, and how thick it is. The
    // animation's origin may be anywhere on the axis, the foot of the column as often as the
    // hub; and a guessed tube - a fixed 93 % of the outer radius, 8 mm over the ring's mean
    // height - put the fingers' fist 2-3 cm beside the SD202's rim, closed round the air.
    let pct = |v: &mut Vec<f32>, f: f32| {
        v.sort_by(|a, b| a.total_cmp(b));
        v[((v.len() - 1) as f32 * f) as usize]
    };
    let ring: Vec<&Vec3> = positions.iter().filter(|p| radius_of(p) > rim * 0.8).collect();
    let (radius, along, tube) = if ring.len() >= 8 {
        let mut rs: Vec<f32> = ring.iter().map(|p| radius_of(p)).collect();
        let mut zs: Vec<f32> = ring.iter().map(|p| axis.dot(**p - origin_point)).collect();
        let (r0, r1) = (pct(&mut rs, 0.02), pct(&mut rs, 0.98));
        let (z0, z1) = (pct(&mut zs, 0.02), pct(&mut zs, 0.98));
        ((r0 + r1) * 0.5, (z0 + z1) * 0.5, ((r1 - r0).max(z1 - z0) * 0.5).clamp(0.01, 0.03))
    } else {
        (rim * 0.93, 0.0, 0.017)
    };
    let centre = origin_point + axis * along.clamp(-0.3, 0.3);
    let radius = radius.clamp(0.14, 0.32);
    log::debug!("driver: steering wheel rim {radius:.3} m round the axis, tube {tube:.3} m thick (radius)");
    Some(Wheel { mesh, anim, var, factor, axis, centre, up, right, radius, tube })
}
