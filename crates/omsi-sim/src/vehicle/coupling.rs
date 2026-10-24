//! Where coupled parts join: the body's ends, its declared couplings and joints.

use super::*;

/// A negative `[boogies]` swaps the rail body's ends relative to its path frame. This is
/// independent of the consist's coupling flags.
/// Keep `heading` in the direction of travel and turn the whole body,
/// including its lights and sound sources, rather than just its meshes.
pub fn body_reversed(def: &Vehicle, reversed: bool) -> bool {
    reversed ^ def.boogies.is_some_and(|b| b < 0.0)
}

/// Model heading in the world, from the path heading and the absolute consist flag.
pub fn body_heading(def: &Vehicle, heading: f64, reversed: bool) -> f64 {
    if body_reversed(def, reversed) {
        heading + 180.0
    } else {
        heading
    }
}

pub(super) fn body_rotation(heading: f64, pitch: f32, bank: f32, reversed: bool) -> Mat4 {
    let (h, p, b) = if reversed {
        (heading + 180.0, -pitch, -bank)
    } else {
        (heading, pitch, bank)
    };
    Mat4::from_quat(
        Quat::from_rotation_z((-h).to_radians() as f32)
            * Quat::from_rotation_x(p.to_radians())
            * Quat::from_rotation_y(b.to_radians()),
    )
}

pub(super) fn declared_coupling(def: &Vehicle, front: bool) -> Vec3 {
    let coupling = if front {
        &def.coupling_front
    } else {
        &def.coupling_back
    };
    let p = coupling
        .as_ref()
        .map(|c| Vec3::from(c.pos))
        // (a bogie frame names one coupling only and hangs by its origin at the other, see
        // `is_bogie_frame`)
        .unwrap_or(if is_bogie_frame(def) { Vec3::ZERO } else { Vec3::new(0.0, if front { 4.0 } else { -4.0 }, 0.3) });
    // Rail couplings are declared in the path frame. Express them in the model frame
    // so body_rotation can turn a negative-bogie body without moving its joints.
    if body_reversed(def, false) {
        Vec3::new(-p.x, -p.y, p.z)
    } else {
        p
    }
}

/// A rail part that is a bogie frame as some mods make them (the Tatra T6B5's `_trail.bus`):
/// no `[boogies]`, one axle at most, and the one coupling of the two it needs named - the other is its
/// own origin. The coupling of the car it hangs from says where it stands, as it does for a
/// car with `[boogies]`.
pub(super) fn is_bogie_frame(def: &Vehicle) -> bool {
    def.is_rail() && def.boogies.is_none() && def.axles.len() <= 1 && (def.coupling_front.is_some() != def.coupling_back.is_some())
}

/// A rail vehicle that says where its couplings are (or where its bogies are, which sets them):
/// joined as it says, not by the ends of its body.
fn declares_couplings(def: &Vehicle) -> bool {
    def.boogies.is_some() || is_bogie_frame(def)
}

/// Where the two parts meet, in their respective model frames. Bogie-defined trains
/// use declared couplings, as Omsi.exe does, even when a mod's
/// joints do not match its bodies: the CR200J's inset rear coupling causes overlap
/// in vanilla too.
/// Other rail definitions retain body-end placement; road vehicles retain their
/// declared joint.
pub fn coupling_offsets(
    lead: &VehicleType,
    lead_reversed: bool,
    car: &VehicleType,
    car_reversed: bool,
) -> Option<(f32, f32)> {
    if !car.def.is_rail() {
        return None;
    }
    // the part in front joins by the coupling of its own frame that faces the one behind it -
    // its rear coupling, which is its front coupling where it is itself turned round; the
    // one behind joins by its front coupling, which is its rear where it is turned round.
    // The two read opposite ends because the two face opposite ways.
    let lead_rear = if declares_couplings(&lead.def) {
        declared_coupling(&lead.def, lead_reversed).y
    } else {
        lead.model_box()
            .map(|(lo, hi)| if lead_reversed { hi.y } else { lo.y })?
    };
    let car_front = if declares_couplings(&car.def) {
        declared_coupling(&car.def, !car_reversed).y
    } else {
        car.model_box()
            .map(|(lo, hi)| if car_reversed { lo.y } else { hi.y })?
    };
    Some((lead_rear, car_front))
}

// (A declared joint is kept however far it stands from the body. The rail parts are laid
// along the track by these distances alone (`VehicleInstance::retrail`), and the stock
// Berlin S-Bahn (BR481) hangs its bogie frames 34 m ahead of the joint behind its last car,
// which puts them under the cars they carry: put at the end of the body instead, they ran
// behind the train (#2069).)

/// `(lead's rear, car's front)` in each body's model frame, including default joints.
pub fn coupling_points(
    lead: &VehicleType,
    lead_reversed: bool,
    car: &VehicleType,
    car_reversed: bool,
) -> (Vec3, Vec3) {
    let mut back = declared_coupling(&lead.def, lead_reversed);
    let mut front = declared_coupling(&car.def, !car_reversed);
    if let Some((b, f)) = coupling_offsets(lead, lead_reversed, car, car_reversed) {
        back.y = b;
        front.y = f;
    }
    // Between bogie-defined cars only distance along the track joins them;
    // lateral position and height come from the track, not a road joint.
    if lead.def.boogies.is_some() && car.def.boogies.is_some() {
        back = Vec3::new(0.0, back.y, 0.0);
        front = Vec3::new(0.0, front.y, 0.0);
    }
    (back, front)
}

/// Where a car of a consist stands, for a caller that lays the cars along one heading with
/// no turn of its own (`Traffic::blocked`): the world position and heading of the car whose
/// body front sits at the joint [`coupling_offsets`] named.
///
/// Both `lead_reversed` and `reversed` are absolute body orientations, including the
/// intrinsic turn of negative bogies, and `heading` is the consist's path heading.
/// A car's body sits along the consist's heading turned round when it is itself turned
/// round, so its heading is derived from `reversed` alone and never from the car in front
/// of it: two cars turned round in a row would otherwise come out turned twice and lie on
/// top of each other.
///
/// `back` is the leading car's body end in the leading car's frame and `front` this car's
/// body front in its own, so the joint lies `back` along the lead's heading (turned round
/// when the *lead* is) and the car's origin steps back from it by `front` along the car's
/// own heading. A caller that turns the part around itself (`TrailerPart::body_rotation`)
/// needs none of this.
pub fn coupling_placement(
    lead_origin: DVec3,
    heading: f64,
    lead_reversed: bool,
    back: f32,
    reversed: bool,
    front: f32,
) -> (DVec3, f64) {
    let turned = |base: f64, rev: bool| if rev { base + 180.0 } else { base };
    let lead_h = turned(heading, lead_reversed);
    let lh = lead_h.to_radians();
    let joint = lead_origin + DVec3::new(lh.sin(), lh.cos(), 0.0) * back as f64;
    let car_h = turned(heading, reversed);
    let ch = car_h.to_radians();
    let origin = joint - DVec3::new(ch.sin(), ch.cos(), 0.0) * front as f64;
    (origin, car_h)
}
