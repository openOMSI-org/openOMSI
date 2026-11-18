//! What each wheel of the player's bus rolls on, told to its scripts as Omsi.exe does
//! (`Axle_SurfaceID_<axle>_L/_R`: the `[surface]` id of the road's or the ground's texture
//! under the wheel, see `World::surface_under`), and to the body's dirt (`host.offroad`).

/// How much a wheel on a `[surface]` throws up of the ground (0 a road): dirt (mud when
/// wet) the whole, grass 0.6, gravel 0.4 - the ids 3, 4 and 5.
pub fn offroad_of(surface: u8) -> f32 {
    match surface {
        3 => 1.0,
        4 => 0.6,
        5 => 0.4,
        _ => 0.0,
    }
}

/// Set the bus's `Axle_SurfaceID_` variables for this frame.
pub fn tell_scripts(w: &crate::World, v: &mut omsi_sim::VehicleInstance) {
    let Some(rb) = v.rigid.as_ref() else { return };
    let rot = v.body_rotation();
    let found: Vec<(String, u8)> = rb
        .wheels
        .iter()
        .enumerate()
        .map(|(k, wh)| {
            let hub = v.position + rot.transform_vector3(wh.attach).as_dvec3();
            let contact = glam::DVec3::new(hub.x, hub.y, if wh.ground_seen { wh.ground_z } else { hub.z - wh.radius as f64 });
            let axle = rb.wheel_axle.get(k).copied().unwrap_or(0);
            let side = if wh.attach.x < 0.0 { "L" } else { "R" };
            (format!("Axle_SurfaceID_{axle}_{side}"), w.surface_under(contact).unwrap_or(0))
        })
        .collect();
    if !found.is_empty() {
        v.host.offroad = found.iter().map(|(_, id)| offroad_of(*id)).sum::<f32>() / found.len() as f32;
    }
    for (name, id) in found {
        v.set_var(&name, id as f32);
    }
}

#[cfg(test)]
mod tests {
    /// Only the ground off the road soils the bus: dirt most, gravel least.
    #[test]
    fn the_ground_off_the_road_soils_the_bus() {
        assert_eq!([0, 1, 2, 6].map(super::offroad_of), [0.0; 4]);
        assert!(super::offroad_of(3) > super::offroad_of(4) && super::offroad_of(4) > super::offroad_of(5) && super::offroad_of(5) > 0.0);
    }
}
