//! The vehicles' road dirt for the enhanced picture (openOMSI, #1386, the `procedural_dirt`
//! setting): each frame the vehicles nearest the camera hand the renderer what dirt they
//! carry (`omsi_sim::vehicle::RoadDirt`) and the shape to lay it out on, their box and their
//! wheels (`omsi_render::VehicleDirt`). An articulated bus's rear sections carry the dirt
//! of their front section.

use glam::{DVec3, Vec3};
use omsi_render::{DIRT_VEHICLES, Renderer, VehicleDirt};
use omsi_sim::VehicleInstance;
use omsi_sim::vehicle::RoadDirt;
use std::sync::atomic::Ordering;

/// The shape of one body (`def`) at `origin` turned by `rotation` (model frame to world),
/// carrying `dirt`.
fn body(
    def: &omsi_vehicle::Vehicle,
    origin: DVec3,
    rotation: glam::Mat4,
    dirt: &RoadDirt,
) -> VehicleDirt {
    let mut axles: Vec<&omsi_vehicle::Axle> = def.axles.iter().collect();
    axles.sort_by(|a, b| b.long.total_cmp(&a.long));
    let (front, rear) = (
        axles.first().map_or(3.0, |a| a.long),
        axles.last().map_or(-3.0, |a| a.long),
    );
    let half_track = axles
        .iter()
        .map(|a| a.max_width / 2.0)
        .fold(0.0f32, f32::max)
        .max(0.8);
    // the `.bus`'s `[procedural_dirt_box]` or `[boundingbox]` (sizes, then the middle);
    // without one, round the axles
    let (min, max) = match def.procedural_dirt_box.or(def.bounding_box) {
        Some(b) if b[..3].iter().all(|s| *s > 0.3) => {
            let (s, c) = (Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5]));
            (c - s * 0.5, c + s * 0.5)
        }
        _ => (
            Vec3::new(-half_track - 0.15, rear - 2.5, 0.0),
            Vec3::new(half_track + 0.15, front + 2.5, 3.0),
        ),
    };
    let mut ys = [0.0; 4];
    for (k, a) in axles.iter().take(4).enumerate() {
        ys[k] = a.long;
    }
    VehicleDirt {
        origin,
        right: rotation.transform_vector3(Vec3::X),
        forward: rotation.transform_vector3(Vec3::Y),
        min,
        max,
        axles: ys,
        axle_count: axles.len().min(4) as u32,
        wheel_radius: axles.first().map_or(0.5, |a| a.wheel_diameter / 2.0),
        half_track,
        spray: dirt.spray,
        film: dirt.film,
        dust: dirt.dust,
        wet: dirt.wet,
        slush: dirt.slush,
        mud: dirt.mud,
    }
}

/// `OMSI_ROAD_DIRT=spray,film,dust,wet[,slush,mud]`: the dirt every vehicle carries, for
/// pictures of it without the hour of driving it takes.
fn forced() -> Option<RoadDirt> {
    let v = omsi_cfg::flags::OMSI_ROAD_DIRT.var()?;
    let x: Vec<f32> = v
        .split(',')
        .map(|s| s.trim().parse().unwrap_or(0.0))
        .collect();
    let at = |k: usize| x.get(k).copied().unwrap_or(0.0).clamp(0.0, 1.0);
    Some(RoadDirt {
        spray: at(0),
        film: at(1),
        dust: at(2),
        wet: at(3),
        slush: at(4),
        mud: at(5),
    })
}

/// The bodies of `v` and its rear sections, with its dirt.
fn bodies(v: &VehicleInstance) -> impl Iterator<Item = VehicleDirt> + '_ {
    let d = forced().unwrap_or(v.road_dirt);
    // (a body whose file says `[procedural_dirt_off]` keeps its own dirt instead)
    std::iter::once(body(&v.ty.def, v.position, v.body_rotation(), &d))
        .filter(|_| !v.ty.def.procedural_dirt_off)
        .chain(
            v.trailers
                .iter()
                .filter(|t| !t.ty.def.procedural_dirt_off)
                .map(move |t| body(&t.ty.def, t.position, t.body_rotation(), &d)),
        )
}

/// One frame: the dirt of the `vehicles` nearest `eye` with any on them, or none (`on`: the
/// setting). The road dirt is drawn by the enhanced picture alone: without one the vehicles
/// keep their own dirt overlays (`omsi_sim::vehicle::road_dirt::set_procedural_dirt`).
/// The picture holds `DIRT_VEHICLES` bodies; the vehicles are given them whole, nearest
/// first, and each is told whether it got them (`VehicleInstance::road_dirt_drawn`): one
/// that did not keeps its own dirt overlays, a clean one has nothing to lay on.
pub fn frame(r: &Renderer, on: bool, eye: DVec3, vehicles: &[(u64, &VehicleInstance)]) {
    let on = on && !r.options.no_enhanced;
    omsi_sim::vehicle::road_dirt::set_procedural_dirt(on);
    if !on {
        for (_, v) in vehicles {
            v.road_dirt_drawn.store(false, Ordering::Relaxed);
        }
        r.set_vehicle_dirt(&[]);
        return;
    }
    let forced = forced().is_some();
    let mut dirty: Vec<(f64, &VehicleInstance)> = Vec::new();
    for (_, v) in vehicles {
        if v.road_dirt.clean() && !forced {
            v.road_dirt_drawn.store(true, Ordering::Relaxed);
        } else {
            dirty.push((v.position.distance_squared(eye), v));
        }
    }
    dirty.sort_by(|a, b| a.0.total_cmp(&b.0));
    let list = allot(dirty.into_iter().map(|(_, v)| v), DIRT_VEHICLES, |v| {
        bodies(v).filter(VehicleDirt::any).collect()
    });
    r.set_vehicle_dirt(&list);
}

/// The bodies of `vehicles` (nearest first) that fit in `room`, whole vehicles only, each
/// told whether it got its place.
fn allot<'a>(
    vehicles: impl Iterator<Item = &'a VehicleInstance>,
    room: usize,
    bodies_of: impl Fn(&VehicleInstance) -> Vec<VehicleDirt>,
) -> Vec<VehicleDirt> {
    let mut list: Vec<VehicleDirt> = Vec::new();
    for v in vehicles {
        let b = bodies_of(v);
        let fits = list.len() + b.len() <= room;
        if fits {
            list.extend(b);
        }
        v.road_dirt_drawn.store(fits, Ordering::Relaxed);
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_laid_out_by_its_box_and_axles() {
        let mut def = omsi_vehicle::Vehicle::default();
        def.bounding_box = Some([2.5, 12.0, 3.0, 0.0, -1.0, 1.5]);
        for long in [0.0, 5.5] {
            def.axles.push(omsi_vehicle::Axle {
                long,
                max_width: 2.1,
                wheel_diameter: 1.0,
                ..Default::default()
            });
        }
        let dirt = RoadDirt {
            spray: 0.5,
            ..Default::default()
        };
        let b = body(&def, DVec3::new(1.0, 2.0, 3.0), glam::Mat4::IDENTITY, &dirt);
        assert_eq!(b.min, Vec3::new(-1.25, -7.0, 0.0));
        assert_eq!(b.max, Vec3::new(1.25, 5.0, 3.0));
        assert_eq!(b.axle_count, 2);
        assert_eq!(b.axles[..2], [5.5, 0.0]);
        assert_eq!(b.wheel_radius, 0.5);
        assert_eq!(b.spray, 0.5);
        assert_eq!(b.forward, Vec3::Y);
    }
}
