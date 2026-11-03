//! Collision damage on a vehicle instance: cracks over its broken glass and dents in its
//! body meshes (copies of its own), from the hits the simulation recorded.
use crate::scene::*;
use omsi_render::{Renderer, Scene};

fn closest_point_on_triangle(
    p: glam::Vec3,
    a: glam::Vec3,
    b: glam::Vec3,
    c: glam::Vec3,
) -> glam::Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
fn point_in_triangle_2d(p: glam::Vec2, tri: [glam::Vec2; 3]) -> bool {
    let cross = |a: glam::Vec2, b: glam::Vec2, c: glam::Vec2| (b - a).perp_dot(c - a);
    let (area, a, b, c) = (
        cross(tri[0], tri[1], tri[2]),
        cross(tri[0], tri[1], p),
        cross(tri[1], tri[2], p),
        cross(tri[2], tri[0], p),
    );
    area.abs() > 1e-8 && (a * area >= -1e-6 && b * area >= -1e-6 && c * area >= -1e-6)
}

fn clip_polygon_to_triangle(
    polygon: &[glam::Vec2],
    triangle: [glam::Vec2; 3],
) -> Vec<glam::Vec2> {
    let orientation = (triangle[1] - triangle[0])
        .perp_dot(triangle[2] - triangle[0])
        .signum();
    if orientation == 0.0 {
        return Vec::new();
    }
    let mut clipped = polygon.to_vec();
    for edge in 0..3 {
        if clipped.is_empty() {
            break;
        }
        let edge_start = triangle[edge];
        let edge_end = triangle[(edge + 1) % 3];
        let signed_distance = |point: glam::Vec2| {
            (edge_end - edge_start).perp_dot(point - edge_start) * orientation
        };
        let input = std::mem::take(&mut clipped);
        let mut previous = *input.last().expect("non-empty polygon");
        let mut previous_distance = signed_distance(previous);
        for current in input {
            let current_distance = signed_distance(current);
            let previous_inside = previous_distance >= -1e-6;
            let current_inside = current_distance >= -1e-6;
            if previous_inside != current_inside {
                let denominator = previous_distance - current_distance;
                if denominator.abs() > 1e-8 {
                    let t = (previous_distance / denominator).clamp(0.0, 1.0);
                    clipped.push(previous.lerp(current, t));
                }
            }
            if current_inside {
                clipped.push(current);
            }
            previous = current;
            previous_distance = current_distance;
        }
    }
    clipped
}

#[allow(clippy::too_many_arguments)]
fn push_clipped_crack_segment(
    mesh: &mut omsi_geometry::MeshData,
    start: glam::Vec3,
    end: glam::Vec3,
    axis_x: glam::Vec3,
    axis_y: glam::Vec3,
    impact: glam::Vec3,
    triangles: &[[glam::Vec2; 3]],
    normal: glam::Vec3,
    width: f32,
    slot: u32,
    normal_offset: f32,
) {
    let project = |point: glam::Vec3| {
        glam::Vec2::new((point - impact).dot(axis_x), (point - impact).dot(axis_y))
    };
    let (a, b) = (project(start), project(end));
    let side = (b - a).perp().normalize_or_zero() * (width * 0.5);
    if side.length_squared() < 1e-10 {
        return;
    }
    let strip = [a + side, b + side, b - side, a - side];
    for triangle in triangles {
        let polygon = clip_polygon_to_triangle(&strip, *triangle);
        if polygon.len() < 3 {
            continue;
        }
        let area = polygon
            .iter()
            .enumerate()
            .map(|(i, p)| p.perp_dot(polygon[(i + 1) % polygon.len()]))
            .sum::<f32>()
            .abs();
        if area < 1e-10 {
            continue;
        }
        let base = mesh.positions.len() as u32;
        for point in &polygon {
            mesh.positions.push(
                impact + axis_x * point.x + axis_y * point.y + normal * normal_offset,
            );
            mesh.normals.push(normal);
            mesh.uvs.push(glam::Vec2::ZERO);
        }
        let first = mesh.indices.len() as u32;
        for i in 1..polygon.len() - 1 {
            mesh.indices.extend([base, base + i as u32, base + i as u32 + 1]);
        }
        mesh.ranges
            .push((first, mesh.indices.len() as u32 - first, slot));
    }
}

fn glass_crack_mesh(
    pane: &omsi_geometry::MeshData,
    mesh_index: usize,
    impact: glam::Vec3,
    energy: f32,
) -> omsi_geometry::MeshData {
    let mut nearest: Option<(f32, glam::Vec3, glam::Vec3)> = None;
    for tri in pane.indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let (Some(&a), Some(&b), Some(&c)) = (
            pane.positions.get(a),
            pane.positions.get(b),
            pane.positions.get(c),
        ) else {
            continue;
        };
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let closest = closest_point_on_triangle(impact, a, b, c);
        let distance = closest.distance_squared(impact);
        if normal.length_squared() > 0.5
            && nearest.map(|n| distance < n.0).unwrap_or(true)
        {
            nearest = Some((distance, closest, normal));
        }
    }
    let Some((_, plane_point, normal)) = nearest else {
        return omsi_geometry::MeshData::default();
    };
    let impact = plane_point;
    let reference = if normal.z.abs() < 0.9 { glam::Vec3::Z } else { glam::Vec3::Y };
    let axis_x = normal.cross(reference).normalize_or_zero();
    let axis_y = normal.cross(axis_x).normalize_or_zero();
    let pane_triangles = pane
        .indices
        .chunks_exact(3)
        .filter_map(|indices| {
            let [a, b, c] = [indices[0] as usize, indices[1] as usize, indices[2] as usize];
            let (a, b, c) = (
                *pane.positions.get(a)?,
                *pane.positions.get(b)?,
                *pane.positions.get(c)?,
            );
            let tri_normal = (b - a).cross(c - a).normalize_or_zero();
            if tri_normal.dot(normal).abs() < 0.95
                || [a, b, c].iter().any(|p| (*p - impact).dot(normal).abs() > 0.015)
            {
                return None;
            }
            Some([
                glam::Vec2::new((a - impact).dot(axis_x), (a - impact).dot(axis_y)),
                glam::Vec2::new((b - impact).dot(axis_x), (b - impact).dot(axis_y)),
                glam::Vec2::new((c - impact).dot(axis_x), (c - impact).dot(axis_y)),
            ])
        })
        .collect::<Vec<_>>();
    if pane_triangles.is_empty() {
        return omsi_geometry::MeshData::default();
    }
    let pane_radius = pane_triangles
        .iter()
        .flat_map(|tri| tri.iter())
        .map(|p| p.length())
        .fold(0.0f32, f32::max)
        .min(0.65);
    let radius = pane_radius.min((energy / 20_000.0).sqrt() * 0.2);
    if radius <= 0.01 {
        return omsi_geometry::MeshData::default();
    }

    let mut mesh = omsi_geometry::MeshData {
        one_sided: false,
        ..Default::default()
    };
    let mut seed = (mesh_index as u32).wrapping_add(0x9e37_79b9);
    let mut rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    let rays = 8;
    for ray in 0..rays {
        let angle = std::f32::consts::TAU * (ray as f32 / rays as f32) + (rand() - 0.5) * 0.09;
        let distance = radius * (0.55 + rand() * 0.35);
        let mut points = Vec::new();
        for segment in 0..3 {
            let t = (segment + 1) as f32 / 3.0;
            let wobble = (rand() - 0.5) * radius * 0.035;
            let at = impact
                + axis_x * (angle.cos() * distance * t + angle.sin() * wobble)
                + axis_y * (angle.sin() * distance * t - angle.cos() * wobble);
            points.push(at);
        }
        let mut prev = impact;
        for (part, next) in points.iter().copied().enumerate() {
            push_clipped_crack_segment(
                &mut mesh,
                prev,
                next,
                axis_x,
                axis_y,
                impact,
                &pane_triangles,
                normal,
                0.0018,
                0,
                0.00035,
            );
            push_clipped_crack_segment(
                &mut mesh,
                prev,
                next,
                axis_x,
                axis_y,
                impact,
                &pane_triangles,
                normal,
                0.00055,
                1,
                0.00055,
            );
            if part == 1 && ray % 3 == 0 {
                let branch_angle = angle + if rand() > 0.5 { 0.8 } else { -0.8 };
                let branch_end = next
                    + axis_x * (branch_angle.cos() * radius * 0.18)
                    + axis_y * (branch_angle.sin() * radius * 0.18);
                push_clipped_crack_segment(
                    &mut mesh,
                    next,
                    branch_end,
                    axis_x,
                    axis_y,
                    impact,
                    &pane_triangles,
                    normal,
                    0.0015,
                    0,
                    0.00035,
                );
                push_clipped_crack_segment(
                    &mut mesh,
                    next,
                    branch_end,
                    axis_x,
                    axis_y,
                    impact,
                    &pane_triangles,
                    normal,
                    0.00045,
                    1,
                    0.00055,
                );
            }
            prev = next;
        }
    }
    mesh
}

pub fn sync_vehicle_damage(
    renderer: &Renderer,
    scene: &mut Scene,
    vehicle: &mut omsi_sim::VehicleInstance,
    render: &mut VehicleRender,
) {
    place_cracks(renderer, scene, render, vehicle.position, |m| vehicle.mesh_local_transform(m));
    for glass in vehicle.take_broken_glass() {
        let Some(transform) = vehicle.mesh_transforms.get(glass.mesh) else { continue };
        let impact = transform.inverse().transform_point3(glass.point);
        let local = vehicle.mesh_local_transform(glass.mesh);
        add_crack(renderer, scene, render, &vehicle.ty, vehicle.position, glass.mesh, impact, glass.energy, local);
    }
    for dent in vehicle.take_damage_dents() {
        add_dent(renderer, scene, render, &vehicle.ty, &dent);
    }
}

/// The glass cracks a vehicle has, where its panes are (`local`: a mesh's transform
/// relative to `position`) and shown with them.
pub(crate) fn place_cracks(renderer: &Renderer, scene: &mut Scene, render: &VehicleRender, position: glam::DVec3, local: impl Fn(usize) -> glam::Mat4) {
    for &(mesh, instance, _) in &render.glass_cracks {
        renderer.set_transform(scene, instance, position, local(mesh));
        // shown with the pane it lies on: a bus's inner and outer window meshes (one for the
        // cab view, one for outside) both crack, and drawn always, both crack stars showed
        let visible = render.instances.get(mesh).is_none_or(|&i| scene.instances[i].visible);
        // (a crack has its two materials: their alpha is copied onto the stack, not into a
        // fresh vector for every crack of every car each frame)
        let own = &scene.instances[instance].slot_alpha;
        let mut buf = [1.0f32; 4];
        if own.len() <= buf.len() {
            let n = own.len();
            buf[..n].copy_from_slice(own);
            renderer.set_params(scene, instance, &buf[..n], visible, &[]);
        } else {
            let alpha = own.clone();
            renderer.set_params(scene, instance, &alpha, visible, &[]);
        }
    }
}

/// A pane of mesh `mesh` cracked at `impact` (in the mesh's frame) with `energy`: its crack
/// overlay made (`local`: the mesh's transform relative to `position`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_crack(
    renderer: &Renderer,
    scene: &mut Scene,
    render: &mut VehicleRender,
    ty: &omsi_sim::VehicleType,
    position: glam::DVec3,
    mesh: usize,
    impact: glam::Vec3,
    energy: f32,
    local: glam::Mat4,
) {
    let Some(pane) = ty.mesh_data(mesh) else { return };
    let data = glass_crack_mesh(&pane, mesh, impact, energy);
    if data.indices.is_empty() {
        return;
    }
    // (lit, as the pane they lie on: a crack shows by the light it scatters, and drawn
    // unlit its pale lines glowed at night)
    let dark = renderer.add_material(
        scene,
        None,
        omsi_render::AlphaMode::Blend,
        [0.025, 0.095, 0.13, 1.0],
        false,
    );
    let bright = renderer.add_material(
        scene,
        None,
        omsi_render::AlphaMode::Blend,
        [0.72, 0.94, 1.0, 1.0],
        false,
    );
    render.own_materials.extend([dark, bright]);
    let id = renderer.add_mesh(scene, &data);
    let instance = renderer.add_instance(scene, id, position, local, vec![dark, bright]);
    render.glass_cracks.push((mesh, instance, id));
}

pub(crate) fn carry_damage(renderer: &Renderer, scene: &mut Scene, from: &mut VehicleRender, to: &mut VehicleRender) {
    for (mesh, id, data) in std::mem::take(&mut from.damaged) {
        match to.instances.get(mesh) {
            Some(&instance) => {
                renderer.set_instance_mesh(scene, instance, id);
                to.damaged.push((mesh, id, data));
            }
            None => from.damaged.push((mesh, id, data)),
        }
    }
    for crack in std::mem::take(&mut from.glass_cracks) {
        let materials = &scene.instances[crack.1].materials;
        from.own_materials.retain(|m| !materials.contains(m));
        to.own_materials.extend(materials.iter().copied());
        to.glass_cracks.push(crack);
    }
}

/// A dent taken: the mesh it is in pushed in round its point (a copy of the mesh of this
/// vehicle's own, made at its first dent).
pub(crate) fn add_dent(renderer: &Renderer, scene: &mut Scene, render: &mut VehicleRender, ty: &omsi_sim::VehicleType, dent: &omsi_sim::vehicle::VehicleDent) {
    let Some(vm) = ty.meshes.get(dent.mesh) else { return };
    if !vm.skin.is_empty() {
        return;
    }
    let owned = render.damaged.iter().position(|(i, _, _)| *i == dent.mesh);
    let (id, data) = if let Some(k) = owned {
        let (_, id, data) = &mut render.damaged[k];
        (*id, data)
    } else {
        let Some(&instance) = render.instances.get(dent.mesh) else { return };
        // (as the first upload had it: the glass keeps its snow's coordinates in metres)
        let Some(data) = omsi_scene::gpu_mesh_of(ty, dent.mesh) else { return };
        if data.positions.is_empty() {
            return;
        }
        let id = renderer.add_mesh(scene, &data);
        renderer.set_instance_mesh(scene, instance, id);
        render.damaged.push((dent.mesh, id, data));
        let (_, id, data) = render.damaged.last_mut().expect("just inserted damage mesh");
        (*id, data)
    };

    let radius = 0.55 + dent.depth;
    let mut changed = false;
    for p in &mut data.positions {
        let distance = p.distance(dent.point);
        if distance < radius {
            let weight = (1.0 - distance / radius).powi(2);
            *p += dent.push * (dent.depth * weight);
            changed = true;
        }
    }
    if changed {
        omsi_geometry::compute_normals_d3d(data);
        renderer.update_mesh(scene, id, &data.positions, &data.normals, &data.uvs);
    }
}

#[cfg(test)]
mod vehicle_damage_tests {
    use super::*;

    #[test]
    fn glass_cracks_are_colored_overlay_geometry_without_pane_holes() {
        let pane = omsi_geometry::MeshData {
            positions: vec![
                glam::Vec3::new(0.0, -1.0, 0.0),
                glam::Vec3::new(1.0, 1.0, 0.0),
                glam::Vec3::new(-1.0, 1.0, 0.0),
            ],
            normals: vec![glam::Vec3::Z; 3],
            uvs: vec![glam::Vec2::ZERO; 3],
            ranges: vec![(0, 3, 0)],
            indices: vec![0, 1, 2],
            one_sided: true,
        };
        let impact = glam::Vec3::new(0.15, -0.6, 0.0);
        let cracks = glass_crack_mesh(&pane, 7, impact, 20_000.0);
        assert!(!cracks.indices.is_empty());
        assert_eq!(cracks.indices.len() % 3, 0);
        assert!(cracks.ranges.iter().all(|(_, count, slot)| *count >= 3 && *count % 3 == 0 && *slot <= 1));
        assert!(cracks.ranges.iter().any(|(_, _, slot)| *slot == 0));
        assert!(cracks.ranges.iter().any(|(_, _, slot)| *slot == 1));
        assert!(cracks.positions.iter().all(|p| p.is_finite()));
        let pane_triangle = [
            glam::Vec2::new(0.0, -1.0),
            glam::Vec2::new(1.0, 1.0),
            glam::Vec2::new(-1.0, 1.0),
        ];
        assert!(cracks.positions.iter().all(|p| {
            p.z >= 0.0
                && p.z <= 0.001
                && point_in_triangle_2d(glam::Vec2::new(p.x, p.y), pane_triangle)
        }));
        assert!(cracks.indices.len() < 1200, "{} vertices", cracks.indices.len());
        let severe = glass_crack_mesh(&pane, 7, impact, 320_000.0);
        let radius = |mesh: &omsi_geometry::MeshData| {
            mesh.positions
                .iter()
                .map(|p| glam::Vec2::new(p.x - impact.x, p.y - impact.y).length())
                .fold(0.0f32, f32::max)
        };
        assert!(radius(&severe) > radius(&cracks));
        assert_eq!(pane.positions.len(), 3);
        assert_eq!(pane.indices.len(), 3);
    }
}
