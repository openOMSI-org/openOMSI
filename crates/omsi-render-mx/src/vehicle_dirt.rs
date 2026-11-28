//! The road dirt on the vehicles near the camera (openOMSI, #1386, the `procedural_dirt`
//! setting): what each has gathered where (`omsi_sim::vehicle::RoadDirt`) and the shape it
//! is laid out on - its box and its wheels - which the enhanced picture's scene shader
//! (enhanced/03_surface.wgsl `road_dirt`) lays over the vehicle's outside. It goes in the
//! frame's enhanced uniform: the vehicles' meshes need nothing of their own, and every bus
//! takes it, however it was made.

use crate::Renderer;
use glam::{DVec3, Vec3};

/// How many bodies carry their road dirt in a frame (the nearest the camera, whole vehicles
/// only, as `omsi_scene::road_dirt` lists them); the vehicles past them keep their own dirt
/// overlays.
pub const DIRT_VEHICLES: usize = 64;
/// The uniform's vectors per vehicle (see `VehicleDirt::pack`).
pub(crate) const DIRT_ROWS: usize = 8;

/// One vehicle's road dirt as the picture lays it out.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VehicleDirt {
    /// Its model origin in the world, and the model's x (right) and y (forward) axes there.
    pub origin: DVec3,
    pub right: Vec3,
    pub forward: Vec3,
    /// Its outside in the model frame (the `.bus`'s `[boundingbox]`).
    pub min: Vec3,
    pub max: Vec3,
    /// Its axles along the model's y (front first, up to four), the wheels' radius and how
    /// far out from the middle they stand.
    pub axles: [f32; 4],
    pub axle_count: u32,
    pub wheel_radius: f32,
    pub half_track: f32,
    /// How much of each kind lies on it, 0..1: wheel spray, road film, dust; how wet it is;
    /// the shares of slush and salt and of mud in it.
    pub spray: f32,
    pub film: f32,
    pub dust: f32,
    pub wet: f32,
    pub slush: f32,
    pub mud: f32,
}

impl VehicleDirt {
    /// The uniform's vectors for this vehicle, relative to the render origin `ro`: origin
    /// (w the axles), right (w the wheel radius), forward (w the half track), the box's
    /// min and max, the axles, the amounts (spray, film, dust, wet) and the kind (slush,
    /// mud).
    pub(crate) fn pack(&self, ro: DVec3) -> [[f32; 4]; DIRT_ROWS] {
        let o = (self.origin - ro).as_vec3();
        let r = self.right.normalize_or_zero();
        let f = self.forward.normalize_or_zero();
        [
            [o.x, o.y, o.z, self.axle_count.min(4) as f32],
            [r.x, r.y, r.z, self.wheel_radius.max(0.1)],
            [f.x, f.y, f.z, self.half_track.max(0.3)],
            self.min.extend(0.0).to_array(),
            self.max.extend(0.0).to_array(),
            self.axles,
            [self.spray, self.film, self.dust, self.wet].map(|x| x.clamp(0.0, 1.0)),
            [self.slush, self.mud, 0.0, 0.0].map(|x| x.clamp(0.0, 1.0)),
        ]
    }

    /// Whether there is anything to draw.
    pub fn any(&self) -> bool {
        self.spray + self.film + self.dust > 1e-3
    }
}

/// The uniform's fields for `list` (the first `DIRT_VEHICLES` with dirt on them): how many,
/// and their vectors.
pub(crate) fn pack_all(
    list: &[VehicleDirt],
    ro: DVec3,
) -> ([f32; 4], [[f32; 4]; DIRT_VEHICLES * DIRT_ROWS]) {
    let mut rows = [[0.0; 4]; DIRT_VEHICLES * DIRT_ROWS];
    let mut n = 0;
    for v in list.iter().filter(|v| v.any()).take(DIRT_VEHICLES) {
        rows[n * DIRT_ROWS..(n + 1) * DIRT_ROWS].copy_from_slice(&v.pack(ro));
        n += 1;
    }
    ([n as f32, 0.0, 0.0, 0.0], rows)
}

impl Renderer {
    /// The vehicles' road dirt for the next frames (nearest the camera first); empty: none
    /// drawn (the setting off, or the classic picture).
    pub fn set_vehicle_dirt(&self, list: &[VehicleDirt]) {
        let mut d = self.vehicle_dirt.borrow_mut();
        d.clear();
        d.extend_from_slice(list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dirty_vehicles_up_to_the_limit() {
        let dirty = VehicleDirt {
            spray: 0.5,
            origin: DVec3::new(100.0, 50.0, 2.0),
            right: Vec3::X,
            forward: Vec3::Y,
            ..Default::default()
        };
        let clean = VehicleDirt::default();
        // (a clean one, left out, and more dirty ones than the uniform holds)
        let list: Vec<VehicleDirt> = std::iter::once(clean)
            .chain(std::iter::repeat_n(dirty, DIRT_VEHICLES + 2))
            .collect();
        let (n, rows) = pack_all(&list, DVec3::new(100.0, 0.0, 0.0));
        assert_eq!(n[0], DIRT_VEHICLES as f32);
        assert_eq!(rows[0][..3], [0.0, 50.0, 2.0]);
        assert_eq!(rows[6][0], 0.5);
        assert_eq!(pack_all(&[clean], DVec3::ZERO).0[0], 0.0);
    }
}
