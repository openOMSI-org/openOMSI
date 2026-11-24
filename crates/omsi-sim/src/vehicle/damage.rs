//! Collision damage of a vehicle: the hits on moving vehicles handed to the traffic, the
//! hits an AI car takes (its collision event, a dent in its body), and the glass broken.

use super::*;

#[derive(Debug, Clone, Copy)]
pub struct DynamicImpact {
    pub obstacle_id: i64,
    pub point: DVec3,
    pub push: DVec3,
    pub speed: f32,
    pub energy: f32,
    /// The striking vehicle's mass (kg): what the struck car's recoil is shared with.
    pub mass: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct VehicleDent {
    pub mesh: usize,
    pub point: Vec3,
    pub push: Vec3,
    pub depth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrokenGlass {
    pub mesh: usize,
    pub point: Vec3,
    pub energy: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImpactZone {
    Front,
    Rear,
    Left,
    Right,
    Centre,
}

pub(super) fn impact_zone(point: Vec3, bb: [f32; 6]) -> ImpactZone {
    let (x, y) = (point.x - bb[3], point.y - bb[4]);
    if y >= bb[1] * 0.25 {
        ImpactZone::Front
    } else if y <= -bb[1] * 0.25 {
        ImpactZone::Rear
    } else if x < 0.0 {
        ImpactZone::Left
    } else if x > 0.0 {
        ImpactZone::Right
    } else {
        ImpactZone::Centre
    }
}

impl VehicleInstance {
    /// Drain collisions against moving vehicles so the traffic system can deliver each hit
    /// to its AI vehicle after the player's physics step.
    pub fn take_dynamic_impacts(&mut self) -> Vec<DynamicImpact> {
        std::mem::take(&mut self.dynamic_impacts)
    }

    /// Apply a collision to an AI vehicle. Its own scripts receive the usual OMSI collision
    /// event, and the nearest body mesh gets a localized, persistent dent.
    pub fn receive_dynamic_impact(&mut self, point: Vec3, push: Vec3, speed: f32, energy: f32) {
        if !point.is_finite()
            || !push.is_finite()
            || !speed.is_finite()
            || !energy.is_finite()
            || energy <= 0.0
        {
            return;
        }

        let bb = self
            .ty
            .def
            .bounding_box
            .unwrap_or([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        let zone = impact_zone(point, bb);
        let zone_id = match zone {
            ImpactZone::Front => 1.0,
            ImpactZone::Rear => 2.0,
            ImpactZone::Left => 3.0,
            ImpactZone::Right => 4.0,
            ImpactZone::Centre => 5.0,
        };
        self.set_var("AI_CollisionZone", zone_id);
        self.set_var("AI_CollisionSpeed", speed);
        self.host.coll_pos = point.to_array();
        self.host.coll_energy += energy / 1000.0;
        self.collided = true;
        self.last_crash += energy;
        self.last_impact = energy;
        self.crashes += 1;
        self.break_glass_for_impact(point, speed, energy);

        let mut nearest: Option<(f32, usize)> = None;
        for (i, mesh) in self.ty.meshes.iter().enumerate() {
            let Some(transform) = self.mesh_transforms.get(i) else {
                continue;
            };
            if !mesh.skin.is_empty()
                || is_shadow_mesh(&self.ty, i)
                || is_glass_mesh(&self.ty, mesh)
                // (an empty mesh has no box; not `mesh_bounds`, which an AI-loaded type -
                // every AI car - leaves at zero: no AI car ever got a dent)
                || self.ty.mesh_boxes.get(i).is_none_or(|(lo, hi)| lo == hi)
            {
                continue;
            }
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            let Some(&(mesh_lo, mesh_hi)) = self.ty.mesh_boxes.get(i) else {
                continue;
            };
            for x in [mesh_lo.x, mesh_hi.x] {
                for y in [mesh_lo.y, mesh_hi.y] {
                    for z in [mesh_lo.z, mesh_hi.z] {
                        let p = transform.transform_point3(Vec3::new(x, y, z));
                        lo = lo.min(p);
                        hi = hi.max(p);
                    }
                }
            }
            let closest = point.clamp(lo, hi);
            let distance = point.distance_squared(closest);
            if nearest.map(|n| distance < n.0).unwrap_or(true) {
                nearest = Some((distance, i));
            }
        }
        if let Some((_, i)) = nearest {
            let mut direction = push.normalize_or_zero();
            if direction == Vec3::ZERO {
                direction = match zone {
                    ImpactZone::Front => -Vec3::Y,
                    ImpactZone::Rear => Vec3::Y,
                    ImpactZone::Left => Vec3::X,
                    ImpactZone::Right => -Vec3::X,
                    ImpactZone::Centre => Vec3::Z,
                };
            }
            let depth = ((energy / 100_000.0).sqrt() * 0.12).clamp(0.06, 0.4);
            let transform = self.mesh_transforms[i];
            self.damage_dents.push(VehicleDent {
                mesh: i,
                point: transform.inverse().transform_point3(point),
                // (into the body, the way the blow pushed: turned round, the rear of a car
                // run into from behind bulged out towards the bus)
                push: transform
                    .inverse()
                    .transform_vector3(direction)
                    .normalize_or_zero(),
                depth,
            });
        }
    }

    pub(super) fn break_glass_for_impact(&mut self, point: Vec3, speed: f32, energy: f32) {
        if speed < 8.0 || energy < 20_000.0 {
            return;
        }
        let influence_radius = (0.45 + (energy / 100_000.0).sqrt() * 0.45).clamp(0.6, 1.5);
        let nearby = self
            .ty
            .meshes
            .iter()
            .enumerate()
            .filter_map(|(i, mesh)| {
                if self.glass_broken.get(i).copied().unwrap_or(true)
                    || !is_glass_mesh(&self.ty, mesh)
                {
                    return None;
                }
                let &(mesh_min, mesh_max) = self.ty.mesh_boxes.get(i)?;
                let transform = *self.mesh_transforms.get(i)?;
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for x in [mesh_min.x, mesh_max.x] {
                    for y in [mesh_min.y, mesh_max.y] {
                        for z in [mesh_min.z, mesh_max.z] {
                            let p = transform.transform_point3(Vec3::new(x, y, z));
                            lo = lo.min(p);
                            hi = hi.max(p);
                        }
                    }
                }
                let distance = point.distance_squared(point.clamp(lo, hi));
                (distance <= influence_radius * influence_radius).then_some((i, distance))
            })
            .collect::<Vec<_>>();
        if nearby.is_empty() {
            return;
        }
        for (i, _) in nearby {
            self.glass_broken[i] = true;
            self.broken_glass.push(BrokenGlass {
                mesh: i,
                point,
                energy,
            });
        }
    }

    /// Take the glass panes broken by an accident for per-instance visual shattering.
    pub fn take_broken_glass(&mut self) -> Vec<BrokenGlass> {
        std::mem::take(&mut self.broken_glass)
    }

    /// Drain new dents so the app can apply them to this AI instance's private render meshes.
    pub fn take_damage_dents(&mut self) -> Vec<VehicleDent> {
        std::mem::take(&mut self.damage_dents)
    }
}

pub(super) fn is_glass_mesh(ty: &VehicleType, mesh: &VehicleMesh) -> bool {
    let Some(def) = ty.model.meshes.get(mesh.def_index) else {
        return false;
    };
    let looks_like_glass = |text: &str| {
        let text = text.to_ascii_lowercase();
        [
            "glass",
            "glas",
            "fenster",
            "scheibe",
            "window",
            "windshield",
            "windscreen",
        ]
        .iter()
        .any(|word| text.contains(word))
    };
    if def
        .mesh_ident
        .as_deref()
        .is_some_and(|name| looks_like_glass(name))
        || looks_like_glass(&def.file)
        || looks_like_glass(&mesh.file.to_string_lossy())
    {
        return true;
    }
    if mesh.materials.len().max(def.materials.len()) > 1 {
        return false;
    }
    mesh.materials.iter().any(|m| looks_like_glass(&m.texture))
        || def.materials.iter().any(|m| looks_like_glass(&m.texture))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::tests::coupling_test_type;

    #[test]
    fn ai_impact_points_are_classified_by_body_zone() {
        let bb = [2.0, 4.0, 2.0, 0.0, 0.0, 1.0];
        assert_eq!(impact_zone(Vec3::new(0.0, 1.1, 0.5), bb), ImpactZone::Front);
        assert_eq!(impact_zone(Vec3::new(0.0, -1.1, 0.5), bb), ImpactZone::Rear);
        assert_eq!(impact_zone(Vec3::new(-0.8, 0.0, 0.5), bb), ImpactZone::Left);
        assert_eq!(impact_zone(Vec3::new(0.8, 0.0, 0.5), bb), ImpactZone::Right);
        assert_eq!(
            impact_zone(Vec3::new(0.0, 0.0, 0.5), bb),
            ImpactZone::Centre
        );
    }

    #[test]
    fn ai_dynamic_impact_records_script_collision_state() {
        let mut v = VehicleInstance::new(
            coupling_test_type(None),
            VehicleHost::new(Default::default()),
        );
        v.receive_dynamic_impact(Vec3::new(-0.7, 0.0, 0.6), Vec3::X, 4.0, 20_000.0);
        assert!(v.collided);
        assert_eq!(v.host.coll_pos, [-0.7, 0.0, 0.6]);
        assert_eq!(v.host.coll_energy, 20.0);
        assert_eq!(v.last_impact, 20_000.0);
        assert_eq!(v.crashes, 1);
    }

    /// An AI car (its type loaded without CPU meshes: no bounding spheres) run into from
    /// behind gets a dent in its body, pushed in the way the blow went.
    #[test]
    fn an_ai_car_hit_from_behind_is_dented_inwards() {
        let mut ty = coupling_test_type(None);
        let ty_mut = Arc::get_mut(&mut ty).unwrap();
        ty_mut.model.meshes.push(omsi_model::MeshDef {
            file: "body.o3d".into(),
            ..Default::default()
        });
        ty_mut.meshes.push(VehicleMesh {
            def_index: 0,
            data: MeshData::default(),
            file: PathBuf::from("body.o3d"),
            materials: Vec::new(),
            overrides: Vec::new(),
            pivot: Mat4::IDENTITY,
            viewpoint: 0,
            skin: Vec::new(),
            keep_winding: false,
        });
        ty_mut.mesh_bounds = vec![(Vec3::ZERO, 0.0)];
        let mut v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        v.receive_dynamic_impact(Vec3::new(0.0, -6.0, 0.6), Vec3::Y, 10.0, 50_000.0);
        let dents = v.take_damage_dents();
        assert_eq!(dents.len(), 1, "{dents:?}");
        assert!(dents[0].push.y > 0.99, "{:?}", dents[0]);
    }

    #[test]
    fn a_severe_accident_marks_named_glass_meshes_broken_once() {
        let mut ty = coupling_test_type(None);
        let ty_mut = Arc::get_mut(&mut ty).unwrap();
        ty_mut.mesh_boxes.clear();
        for (file, x) in [
            ("windshield.o3d", 0.0),
            ("side_window.o3d", 5.0),
            ("rear_window.o3d", 1.2),
        ] {
            let def_index = ty_mut.model.meshes.len();
            ty_mut.model.meshes.push(omsi_model::MeshDef {
                file: file.into(),
                ..Default::default()
            });
            ty_mut.meshes.push(VehicleMesh {
                def_index,
                data: MeshData::default(),
                file: PathBuf::from(file),
                materials: Vec::new(),
                overrides: Vec::new(),
                pivot: Mat4::IDENTITY,
                viewpoint: 0,
                skin: Vec::new(),
                keep_winding: false,
            });
            ty_mut
                .mesh_boxes
                .push((Vec3::new(x - 0.5, -0.5, 1.0), Vec3::new(x + 0.5, 0.5, 2.0)));
        }
        let mut v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        let impact = Vec3::new(0.0, 0.0, 1.5);
        v.break_glass_for_impact(impact, 7.9, 100_000.0);
        assert!(v.take_broken_glass().is_empty());
        v.break_glass_for_impact(impact, 8.0, 20_000.0);
        assert_eq!(
            v.take_broken_glass().as_slice(),
            &[BrokenGlass {
                mesh: 0,
                point: impact,
                energy: 20_000.0,
            }]
        );
        v.break_glass_for_impact(impact, 12.0, 80_000.0);
        assert_eq!(
            v.take_broken_glass().as_slice(),
            &[BrokenGlass {
                mesh: 2,
                point: impact,
                energy: 80_000.0,
            }]
        );
    }

    #[test]
    fn impact_with_a_fixed_object_breaks_only_the_nearby_window() {
        let mut ty = coupling_test_type(None);
        let ty_mut = Arc::get_mut(&mut ty).unwrap();
        ty_mut.def.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 0.0, 1.0]);
        ty_mut.def.rolling_resistance = 0.0;
        ty_mut.model.meshes.clear();
        ty_mut.meshes.clear();
        ty_mut.mesh_boxes.clear();
        for (file, x, y) in [
            ("front_window.o3d", 0.0, 1.0),
            ("side_window.o3d", 5.0, 0.0),
        ] {
            let def_index = ty_mut.model.meshes.len();
            ty_mut.model.meshes.push(omsi_model::MeshDef {
                file: file.into(),
                ..Default::default()
            });
            ty_mut.meshes.push(VehicleMesh {
                def_index,
                data: MeshData::default(),
                file: PathBuf::from(file),
                materials: Vec::new(),
                overrides: Vec::new(),
                pivot: Mat4::IDENTITY,
                viewpoint: 0,
                skin: Vec::new(),
                keep_winding: false,
            });
            ty_mut.mesh_boxes.push((
                Vec3::new(x - 0.4, y - 0.1, 1.0),
                Vec3::new(x + 0.4, y + 0.1, 2.0),
            ));
        }
        let mut bus = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        bus.physics.mass_kg = 10_000.0;
        bus.dynamic_boxes.push(crate::collision::Obb::from_box(
            [1.0, 0.4, 2.0, 0.0, 0.0, 1.0],
            DVec3::new(0.0, 1.4, 0.0),
            0.0,
        ));
        bus.set_speed(10.0);

        bus.step_physics(0.1);

        let broken = bus.take_broken_glass();
        assert_eq!(broken.len(), 1, "{broken:?}");
        assert_eq!(broken[0].mesh, 0);
        assert_eq!(broken[0].energy, 500_000.0);
        assert!(broken[0].point.is_finite());
    }
}
