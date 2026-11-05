//! The plugins' cables (`omsi.cable`) as scene meshes: each one a thin tube of a few sides
//! round its points, lit and shaded as any mesh of the scene. Only the cables that changed
//! are made again (their vertices rewritten when the shape keeps its size), and the meshes of
//! those taken away are kept hidden for the next ones laid; one tied to the bus only follows
//! the bus's drawn pose.

use std::collections::HashMap;

use glam::{DVec3, Mat4, Vec2, Vec3};
use omsi_render::{AlphaMode, MaterialId, MeshId, Renderer, Scene};
use omsi_sim::VehicleInstance;

/// Sides of a cable's tube: six read as round at the thickness of a wire.
const SIDES: usize = 6;

/// The most vertices the meshes kept for reuse may hold together (a plugin's budget of
/// points, `omsi_plugin`'s `MAX_PLUGIN_POINTS`): past it the oldest kept mesh is freed, its
/// instance kept hidden for a mesh made anew.
const SPARE_VERTS: usize = 65_536 * SIDES;

/// A cable to draw: a row of points, in the map's frame or (`on_bus`) in the player's bus's
/// model frame (x right, y forward, z up).
#[derive(Debug, Clone, PartialEq)]
pub struct Cable {
    pub points: Vec<[f64; 3]>,
    /// The tube's radius (m).
    pub radius: f32,
    /// Linear RGB and alpha, 0 to 1.
    pub color: [f32; 4],
    pub on_bus: bool,
}

/// A cable's key: its plugin's and the plugin's own id for it.
pub type CableKey = (u64, String);

struct Drawn {
    mesh: MeshId,
    instance: usize,
    /// The vertices the mesh was made with (its index buffer holds that many).
    verts: usize,
    cable: Cable,
}

/// What the scene holds of the plugins' cables.
#[derive(Default)]
pub struct PluginCables {
    /// The scene the ids belong to (`Scene::serial`).
    scene: u64,
    drawn: HashMap<CableKey, Drawn>,
    /// Meshes of cables taken away, hidden, for the next cables laid, oldest first, and the
    /// vertices they hold.
    spare: Vec<Drawn>,
    spare_verts: usize,
    /// Hidden instances whose spare mesh was freed, for the next cables laid.
    idle: Vec<usize>,
    materials: Palette,
    /// Slots of the scene's meshes and materials freed here: the next made take them over
    /// (`recycle_*`), so a cable that changes its colour or its number of points each frame
    /// does not grow the scene's lists.
    free_meshes: Vec<MeshId>,
    free_materials: Vec<MaterialId>,
}

/// One material per colour drawn now, freed when the last cable of that colour goes or takes
/// another: a plugin that fades or blinks a cable would otherwise add one each frame for good.
#[derive(Default)]
struct Palette {
    by: HashMap<[u32; 4], (MaterialId, usize)>,
}

impl Palette {
    /// The material of `color` for one more cable (`make` makes it when there is none yet).
    fn take(&mut self, color: [f32; 4], make: impl FnOnce() -> MaterialId) -> MaterialId {
        let e = self.by.entry(color.map(f32::to_bits)).or_insert_with(|| (make(), 0));
        e.1 += 1;
        e.0
    }

    /// One cable fewer of `color`: its material when that was the last, to be freed.
    fn give_back(&mut self, color: [f32; 4]) -> Option<MaterialId> {
        let key = color.map(f32::to_bits);
        let e = self.by.get_mut(&key)?;
        e.1 -= 1;
        if e.1 == 0 { self.by.remove(&key).map(|(m, _)| m) } else { None }
    }
}

impl PluginCables {
    /// Brings the scene up to the plugins' cables. `changes(all)` gives the cables changed
    /// since it was last asked, each with what it is now (`None`: taken away), or every cable
    /// when `all` (the scene is a new one). `bus` is the player's bus as drawn now, for the
    /// cables tied to it.
    pub fn sync(
        &mut self,
        changes: impl FnOnce(bool) -> Vec<(CableKey, Option<Cable>)>,
        bus: Option<&VehicleInstance>,
        r: &Renderer,
        scene: &mut Scene,
    ) {
        let all = self.scene != scene.serial;
        if all {
            // (another scene: what was made lived in the old one; everything is laid anew)
            self.drawn.clear();
            self.spare.clear();
            self.spare_verts = 0;
            self.idle.clear();
            self.materials = Palette::default();
            self.free_meshes.clear();
            self.free_materials.clear();
            self.scene = scene.serial;
        }
        for (key, cable) in changes(all) {
            match cable {
                Some(c) => self.lay(key, c, r, scene),
                None => {
                    if let Some(d) = self.drawn.remove(&key) {
                        // (kept for the next cable rather than left in the scene for good)
                        r.set_params(scene, d.instance, &[], false, &[]);
                        self.release(d.cable.color, r, scene);
                        self.spare_verts += d.verts;
                        self.spare.push(d);
                        while self.spare_verts > SPARE_VERTS && !self.spare.is_empty() {
                            let old = self.spare.remove(0);
                            self.spare_verts -= old.verts;
                            self.free_mesh(old.mesh, r, scene);
                            self.idle.push(old.instance);
                        }
                    }
                }
            }
        }
        let pose = bus.map(|v| (v.position, v.body_rotation()));
        for d in self.drawn.values().filter(|d| d.cable.on_bus) {
            match pose {
                Some((pos, rot)) => {
                    r.set_transform(scene, d.instance, pos, rot);
                    r.set_params(scene, d.instance, &[], true, &[]);
                }
                None => r.set_params(scene, d.instance, &[], false, &[]),
            }
        }
    }

    fn lay(&mut self, key: CableKey, cable: Cable, r: &Renderer, scene: &mut Scene) {
        if cable.points.len() < 2 || self.drawn.get(&key).is_some_and(|d| d.cable == cable) {
            return;
        }
        // (a world cable's vertices are kept near its first point, for the f32 they are
        // stored in; a bus cable's are in the bus's frame already)
        let origin = if cable.on_bus { DVec3::ZERO } else { DVec3::from_array(cable.points[0]) };
        let data = tube(&cable.points, origin, cable.radius);
        let material = self.material(cable.color, r, scene);
        let own = self.drawn.remove(&key);
        if let Some(d) = &own {
            // (after the new colour was taken: the same colour is not freed and made again)
            self.release(d.cable.color, r, scene);
        }
        let reused = own.or_else(|| {
            let same = self.spare.iter().position(|s| s.verts == data.positions.len());
            let d = same.or(self.spare.len().checked_sub(1)).map(|i| self.spare.remove(i))?;
            self.spare_verts -= d.verts;
            Some(d)
        });
        let d = match reused {
            Some(mut d) => {
                if d.verts == data.positions.len() {
                    r.update_mesh(scene, d.mesh, &data.positions, &data.normals, &data.uvs);
                } else {
                    self.free_mesh(d.mesh, r, scene);
                    let mesh = self.add_mesh(&data, r, scene);
                    r.set_instance_mesh(scene, d.instance, mesh);
                    d.mesh = mesh;
                    d.verts = data.positions.len();
                }
                r.set_material(scene, d.instance, 0, material);
                r.set_transform(scene, d.instance, origin, Mat4::IDENTITY);
                r.set_params(scene, d.instance, &[], true, &[]);
                d.cable = cable;
                d
            }
            None => {
                let mesh = self.add_mesh(&data, r, scene);
                let instance = match self.idle.pop() {
                    Some(i) => {
                        r.set_instance_mesh(scene, i, mesh);
                        r.set_material(scene, i, 0, material);
                        r.set_transform(scene, i, origin, Mat4::IDENTITY);
                        r.set_params(scene, i, &[], true, &[]);
                        i
                    }
                    None => r.add_instance(scene, mesh, origin, Mat4::IDENTITY, vec![material]),
                };
                Drawn { mesh, instance, verts: data.positions.len(), cable }
            }
        };
        self.drawn.insert(key, d);
    }

    fn material(&mut self, color: [f32; 4], r: &Renderer, scene: &mut Scene) -> MaterialId {
        let free = &mut self.free_materials;
        self.materials.take(color, || {
            let alpha = if color[3] < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque };
            let new = r.add_material(scene, None, alpha, color, false);
            match free.pop() {
                Some(into) => {
                    let id = r.recycle_material(scene, new, into);
                    if id == new {
                        free.push(into);
                    }
                    id
                }
                None => new,
            }
        })
    }

    fn release(&mut self, color: [f32; 4], r: &Renderer, scene: &mut Scene) {
        if let Some(m) = self.materials.give_back(color) {
            r.free_material(scene, m);
            self.free_materials.push(m);
        }
    }

    fn add_mesh(&mut self, data: &omsi_geometry::MeshData, r: &Renderer, scene: &mut Scene) -> MeshId {
        let new = r.add_mesh(scene, data);
        match self.free_meshes.pop() {
            Some(into) => {
                let id = r.recycle_mesh(scene, new, into);
                if id == new {
                    self.free_meshes.push(into);
                }
                id
            }
            None => new,
        }
    }

    fn free_mesh(&mut self, mesh: MeshId, r: &Renderer, scene: &mut Scene) {
        r.free_mesh(scene, mesh);
        self.free_meshes.push(mesh);
    }
}

/// A tube of `SIDES` sides round the points, `radius` thick, its vertices relative to
/// `origin`. Each ring stands square to the line there, turned along it without
/// twisting (parallel transport), so a long wire's faces do not spiral.
fn tube(points: &[[f64; 3]], origin: DVec3, radius: f32) -> omsi_geometry::MeshData {
    let pts: Vec<Vec3> = points.iter().map(|p| (DVec3::from_array(*p) - origin).as_vec3()).collect();
    let n = pts.len();
    let tangent = |i: usize| -> Vec3 {
        let a = pts[i.saturating_sub(1)];
        let b = pts[(i + 1).min(n - 1)];
        (b - a).try_normalize().unwrap_or(Vec3::X)
    };
    let mut positions = Vec::with_capacity(n * SIDES);
    let mut normals = Vec::with_capacity(n * SIDES);
    let t0 = tangent(0);
    let mut side = if t0.z.abs() < 0.9 { t0.cross(Vec3::Z) } else { t0.cross(Vec3::X) }.normalize();
    let mut prev_t = t0;
    for (i, p) in pts.iter().enumerate() {
        let t = tangent(i);
        let axis = prev_t.cross(t);
        if axis.length_squared() > 1e-12 {
            let angle = prev_t.dot(t).clamp(-1.0, 1.0).acos();
            side = glam::Quat::from_axis_angle(axis.normalize(), angle) * side;
        }
        side = (side - t * side.dot(t)).try_normalize().unwrap_or(side);
        let up = t.cross(side);
        prev_t = t;
        for k in 0..SIDES {
            let a = k as f32 / SIDES as f32 * std::f32::consts::TAU;
            let dir = side * a.cos() + up * a.sin();
            positions.push(*p + dir * radius);
            normals.push(dir);
        }
    }
    let mut indices = Vec::with_capacity((n - 1) * SIDES * 6);
    for i in 0..n - 1 {
        let (a, b) = ((i * SIDES) as u32, ((i + 1) * SIDES) as u32);
        for k in 0..SIDES as u32 {
            let k1 = (k + 1) % SIDES as u32;
            indices.extend_from_slice(&[a + k, b + k, b + k1, a + k, b + k1, a + k1]);
        }
    }
    let uvs = vec![Vec2::ZERO; positions.len()];
    let count = indices.len() as u32;
    omsi_geometry::MeshData { positions, normals, uvs, ranges: vec![(0, count, 0)], indices, one_sided: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tube_has_a_ring_a_point_and_its_faces_stand_round_the_line() {
        let pts = [[100.0, 200.0, 6.0], [110.0, 200.0, 5.8], [120.0, 200.0, 6.0]];
        let origin = DVec3::from_array(pts[0]);
        let m = tube(&pts, origin, 0.01);
        assert_eq!(m.positions.len(), 3 * SIDES);
        assert_eq!(m.indices.len(), 2 * SIDES * 6);
        // every vertex lies the radius away from its point, its normal pointing out
        for (i, p) in m.positions.iter().enumerate() {
            let c = (DVec3::from_array(pts[i / SIDES]) - origin).as_vec3();
            assert!(((*p - c).length() - 0.01).abs() < 1e-4);
            assert!((*p - c).normalize().dot(m.normals[i]) > 0.999);
        }
    }

    #[test]
    fn a_colour_is_made_once_and_freed_with_its_last_cable() {
        let mut p = Palette::default();
        let mut made = 0;
        let red = [1.0, 0.0, 0.0, 1.0];
        assert_eq!(p.take(red, || { made += 1; 7 }), 7);
        assert_eq!(p.take(red, || { made += 1; 8 }), 7);
        assert_eq!(made, 1);
        assert_eq!(p.give_back(red), None);
        assert_eq!(p.give_back(red), Some(7));
        // a cable blinking through colours leaves none behind
        for i in 0..100 {
            let c = [i as f32 / 100.0, 0.0, 0.0, 1.0];
            p.take(c, || i);
            assert_eq!(p.give_back(c), Some(i));
        }
        assert!(p.by.is_empty());
        assert_eq!(p.give_back(red), None);
    }

    fn renderer() -> Renderer {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            omsi_render::RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, ..Default::default() },
        ))
        .unwrap()
    }

    #[test]
    fn a_cable_blinking_and_growing_every_frame_keeps_the_scene_its_size() {
        let r = renderer();
        let mut scene = r.new_scene();
        let mut cables = PluginCables::default();
        let key = |id: &str| (1u64, id.to_string());
        let cable = |i: usize| Cable {
            points: (0..2 + i % 7).map(|k| [k as f64, 0.0, 6.0]).collect(),
            radius: 0.01,
            color: [(i % 50) as f32 / 50.0, 0.0, 0.0, 1.0],
            on_bus: false,
        };
        let sizes = |s: &Scene| (s.meshes.len(), s.materials.len(), s.instances.len());
        let mut steady = None;
        for i in 0..300 {
            // a cable that takes a new colour and a new number of points each frame, and one
            // laid and taken away again and again
            let mut frame = vec![(key("wire"), Some(cable(i)))];
            frame.push((key("rope"), if i % 2 == 0 { Some(cable(i + 3)) } else { None }));
            cables.sync(|_| frame, None, &r, &mut scene);
            if i == 4 {
                steady = Some(sizes(&scene));
            }
        }
        assert_eq!(Some(sizes(&scene)), steady, "the scene's meshes, materials and instances");
    }

    #[test]
    fn a_wire_straight_up_still_gets_a_tube() {
        let m = tube(&[[0.0, 0.0, 0.0], [0.0, 0.0, 5.0]], DVec3::ZERO, 0.02);
        assert!(m.positions.iter().all(|p| p.is_finite()));
        assert!(m.normals.iter().all(|n| (n.length() - 1.0).abs() < 1e-4));
    }
}
