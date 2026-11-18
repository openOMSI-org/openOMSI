//! The shadow a vehicle's headlamps cast (Enhanced): the lamps on the vehicle's front that
//! light the road ahead, taken together as one source between them, get one of the shadow
//! map tiles (`LampShadow`, under the far map) - a perspective view from there along the
//! road. What stands in the beams - a car ahead, a pole, a passenger at the stop - throws its
//! shadow forward along the road instead of being lit through. The player's headlamps are
//! the first of them; the nearest cars' are given the others (`group_spots`).
//!
//! One map for the pair, not one a lamp: two maps would take two tiles, for shadows that
//! differ only by the lamps' spacing at the edges.
//! The vehicle's own parts are no casters for it: the map's eye sits a few centimetres
//! ahead of the lamps' face, and a bumper or a towing eye standing out in front of it
//! would have darkened the whole road.

use super::*;

/// The map's field of view (degrees, square) and how far it looks down from the level: the
/// low beam lights the road from under the bumper (some 60 degrees down at the kerb a
/// metre ahead) to its cut-off at the lamps' horizon, and out to the sides some 45 degrees
/// (`headlamp` in lamp_air.wgsl).
const HEADLAMP_SHADOW_FOV: f32 = 110.0;
const HEADLAMP_SHADOW_PITCH: f32 = 22.0;
/// How far the map reaches at most (m): a full beam is declared to reach some hundreds of
/// metres, but past this its light on a caster's shadow is lost among the street's own.
pub(crate) const HEADLAMP_SHADOW_REACH: f32 = 90.0;
/// How far round the vehicle's `[boundingbox]` a lamp still counts as one of its own (m):
/// the headlamps' light is placed just in front of the vehicle's face (lights.rs).
const OWN_LAMP_MARGIN: f32 = 0.6;

/// The player's vehicle's box (`Lighting::inside`) relative to the render origin, for the
/// tests of the lamps and casters that belong to it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VehicleBox {
    origin: Vec3,
    sin: f32,
    cos: f32,
    half: Vec3,
    centre: Vec3,
}

impl VehicleBox {
    pub(crate) fn new((origin, heading, bb): &(DVec3, f64, [f32; 6]), ro: DVec3) -> VehicleBox {
        let (sin, cos) = (*heading as f32).to_radians().sin_cos();
        VehicleBox { origin: (*origin - ro).as_vec3(), sin, cos, half: Vec3::new(bb[0], bb[1], bb[2]) * 0.5, centre: Vec3::new(bb[3], bb[4], bb[5]) }
    }

    /// Whether a point (render-origin relative) lies in the box grown by `margin`.
    pub(crate) fn contains(&self, p: Vec3, margin: f32) -> bool {
        let d = p - self.origin;
        let l = Vec3::new(d.x * self.cos - d.y * self.sin, d.x * self.sin + d.y * self.cos, d.z) - self.centre;
        l.abs().cmplt(self.half + Vec3::splat(margin)).all()
    }
}

/// Whether a light is one of the player's headlamps that share its shadow map: an enhanced
/// light drawn as a road lamp's beam (`PointLight::beam`) or by a picture of one aimed along
/// the road (`PointLight::cookie`), switched on, at the vehicle.
pub(crate) fn own_headlamp(l: &PointLight, p: Vec3, own: &VehicleBox) -> bool {
    let road = l.beam != 0.0 || (l.cookie.is_some() && l.direction.normalize_or_zero().z.abs() < 0.5);
    road && l.mode == LightMode::Enhanced && l.intensity > 0.0 && own.contains(p, OWN_LAMP_MARGIN)
}

/// The headlamps' shadow map for the frame: from the middle of the lamps (`lamps`: their
/// place, direction and reach), looking along their mean direction on the level - none
/// when no lamp lights the road ahead, or when their light ends before the camera's
/// surroundings begin (`cam_rel`).
pub(crate) fn headlamp_shadow(lamps: &[(Vec3, Vec3, f32)], own: VehicleBox, cam_rel: Vec3) -> Option<LampShadow> {
    if lamps.is_empty() {
        return None;
    }
    let n = lamps.len() as f32;
    let position = lamps.iter().map(|l| l.0).sum::<Vec3>() / n;
    let ahead = lamps.iter().map(|l| l.1).sum::<Vec3>();
    let look = Vec3::new(ahead.x, ahead.y, 0.0).normalize_or_zero();
    if look == Vec3::ZERO {
        return None;
    }
    let range = lamps.iter().map(|l| l.2).fold(0.0f32, f32::max).min(HEADLAMP_SHADOW_REACH);
    if range <= 1.0 || (position - cam_rel).length() > range + LAMP_SHADOW_REACH {
        return None;
    }
    // (the index the shaders compare is the tile's own code: every one of the lamps marked
    // as the player's reads this map, see `lamp_shadow_at`)
    Some(LampShadow { position, range, look: Some(look), own: Some(own), ..LampShadow::new(VEHICLE_SLOTS.start) })
}

impl LampShadow {
    /// The headlamps' view and projection: along `look`, pitched down to take in the road
    /// from the bumper out, depth 0..1 over the reach.
    pub(crate) fn headlamp_view_proj(&self, look: Vec3) -> Mat4 {
        let (s, c) = HEADLAMP_SHADOW_PITCH.to_radians().sin_cos();
        let dir = look * c - Vec3::Z * s;
        let proj = Mat4::perspective_rh(HEADLAMP_SHADOW_FOV.to_radians(), 1.0, 0.1, self.range.max(1.0));
        proj * Mat4::look_to_rh(self.position, dir, Vec3::Z)
    }

    /// Whether a caster (bounding sphere `c`, `r`, render-origin relative) may stand in this
    /// map's light: under a street lamp's head within its reach, or in front of the
    /// headlamps within theirs and their view - not a part of the vehicle they belong to.
    pub(crate) fn reaches(&self, c: Vec3, r: f32) -> bool {
        let d = c - self.position;
        if d.length() >= self.range + r {
            return false;
        }
        // (a saloon lamp's map holds what stands in the vehicle, under the lamp)
        if let Some(cabin) = &self.cabin {
            return c.z - r < self.position.z && cabin.vehicle.contains(c, 0.3);
        }
        if let Some(aim) = self.aim {
            // (a spot's cone: its field's half-diagonal, a caster's sphere on top)
            let along = d.dot(aim);
            let half = (self.fov * 0.5).to_radians().tan() * std::f32::consts::SQRT_2;
            return along > -r && (d - aim * along).length() < along.max(0.0) * half + r * 1.8;
        }
        match self.look {
            None => c.z - r < self.position.z,
            Some(look) => {
                if self.own.is_some_and(|b| b.contains(c, 0.05)) {
                    return false;
                }
                // (inside the view's cone: the field's half-diagonal, its sphere's width on top)
                let along = d.dot(look);
                let half = (HEADLAMP_SHADOW_FOV * 0.5).to_radians().tan() * std::f32::consts::SQRT_2;
                along > -r && (d - look * along).length() < along.max(0.0) * half + r * 1.8
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus() -> VehicleBox {
        // a 12 m bus facing north (+y) at the origin, its box from 0 to 3 m high
        VehicleBox::new(&(DVec3::ZERO, 0.0, [2.5, 12.0, 3.0, 0.0, 0.0, 1.5]), DVec3::ZERO)
    }

    /// The pair's map looks along the road from between the lamps, and a car ahead stands in
    /// it while the bus's own front does not.
    #[test]
    fn the_headlamps_shadow_what_is_ahead_but_not_their_own_bus() {
        let lamps = [(Vec3::new(-0.9, 6.05, 0.8), Vec3::new(0.0, 1.0, -0.05), 60.0), (Vec3::new(0.9, 6.05, 0.8), Vec3::new(0.0, 1.0, -0.05), 60.0)];
        let m = headlamp_shadow(&lamps, bus(), Vec3::new(0.0, -15.0, 3.0)).expect("a map");
        assert_eq!(m.slot, VEHICLE_SLOTS.start);
        assert!((m.position - Vec3::new(0.0, 6.05, 0.8)).length() < 1e-4);
        // a car 15 m ahead, one beside the road behind the bus, the bus's bumper
        assert!(m.reaches(Vec3::new(0.5, 21.0, 0.7), 2.2));
        assert!(!m.reaches(Vec3::new(4.0, -10.0, 0.7), 2.2));
        assert!(!m.reaches(Vec3::new(0.0, 5.9, 0.5), 0.6));
        // the car's place lands inside the map, in front of its eye
        let q = m.view_proj().project_point3(Vec3::new(0.5, 21.0, 0.7));
        assert!(q.x.abs() < 1.0 && q.y.abs() < 1.0 && q.z > 0.0 && q.z < 1.0, "{q:?}");
        // the road a metre in front of the bumper is in it too
        let q = m.view_proj().project_point3(Vec3::new(0.0, 7.0, 0.0));
        assert!(q.x.abs() < 1.0 && q.y.abs() < 1.0, "{q:?}");
    }

    /// Lamps of another vehicle are not the player's, and none switched on makes no map.
    #[test]
    fn only_the_players_lit_headlamps_count() {
        let b = bus();
        let lamp = |at: DVec3, intensity: f32| PointLight { position: at, radius: 60.0, intensity, beam: 1.0, direction: Vec3::Y, mode: LightMode::Enhanced, ..Default::default() };
        let at = DVec3::new(0.9, 6.05, 0.8);
        assert!(own_headlamp(&lamp(at, 1800.0), at.as_vec3(), &b));
        assert!(!own_headlamp(&lamp(at, 0.0), at.as_vec3(), &b));
        let other = DVec3::new(0.9, 30.0, 0.8);
        assert!(!own_headlamp(&lamp(other, 1800.0), other.as_vec3(), &b));
        assert!(headlamp_shadow(&[], b, Vec3::ZERO).is_none());
    }
}
