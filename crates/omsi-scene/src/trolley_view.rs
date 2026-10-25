//! Trolleybus gear on screen: the overhead wires of a map, a bus's poles and ropes, and the
//! arcs where its current breaks (sparks, the flash they throw on the wire and the roof).
//! What they do comes from `omsi_trolley`; this only draws it.

use glam::{DVec3, Mat4, Vec2, Vec3};
use omsi_geometry::MeshData;
use omsi_render::{AlphaMode, Corona, MaterialId, MeshId, PointLight, Renderer, Scene};
use omsi_sim::VehicleInstance;
use omsi_trolley::{PoleMode, Spark, SparkKind, WireNet};
use std::sync::Arc;

/// Radius of a contact wire as it is drawn (m; the real one is 6 to 7 mm).
const WIRE_RADIUS: f32 = 0.0085;
/// Wires of a chunk lie in one 160 m cell.
const CHUNK: f64 = 160.0;

/// A tube of `radius` along `pts`, appended to `m` (the points are local).
fn tube(m: &mut MeshData, pts: &[Vec3], radius: f32, sides: usize) {
    if pts.len() < 2 {
        return;
    }
    let base = m.positions.len() as u32;
    let first = m.indices.len() as u32;
    for (i, p) in pts.iter().enumerate() {
        let t = if i + 1 < pts.len() { (pts[i + 1] - *p).normalize_or(Vec3::Y) } else { (*p - pts[i - 1]).normalize_or(Vec3::Y) };
        let up = if t.z.abs() > 0.95 { Vec3::X } else { Vec3::Z };
        let a = t.cross(up).normalize_or(Vec3::X);
        let b = t.cross(a).normalize_or(Vec3::Z);
        for s in 0..sides {
            let ang = s as f32 / sides as f32 * std::f32::consts::TAU;
            let n = a * ang.cos() + b * ang.sin();
            m.positions.push(*p + n * radius);
            m.normals.push(n);
            m.uvs.push(Vec2::new(i as f32, s as f32 / sides as f32));
        }
    }
    for i in 0..pts.len() as u32 - 1 {
        for s in 0..sides as u32 {
            let (a, b) = (base + i * sides as u32 + s, base + i * sides as u32 + (s + 1) % sides as u32);
            let (c, d) = (a + sides as u32, b + sides as u32);
            m.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    let count = m.indices.len() as u32 - first;
    match m.ranges.last_mut() {
        Some(r) if r.0 + r.1 == first && r.2 == 0 => r.1 += count,
        _ => m.ranges.push((first, count, 0)),
    }
}

fn new_mesh() -> MeshData {
    MeshData { one_sided: false, ..Default::default() }
}

/// A switch as it is drawn: the frog's body and the tongue that lies along the way it is set
/// to, and a lamp beside it (green straight, amber to the branch).
struct FrogDraw {
    tongue: usize,
    pos: DVec3,
    straight: Vec3,
    branch: Vec3,
    branch_set: bool,
}

/// What shows one pole of the player's bus.
struct PoleDraw {
    boom: usize,
    rope: usize,
}

struct Particle {
    pos: DVec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    heat: f32,
}

struct Flash {
    pos: DVec3,
    age: f32,
    energy: f32,
}

pub struct TrolleyView {
    net: Arc<WireNet>,
    chunk_count: usize,
    frogs: Vec<FrogDraw>,
    poles: Vec<PoleDraw>,
    mat_pole: Option<MaterialId>,
    mat_rope: Option<MaterialId>,
    boom_meshes: Vec<MeshId>,
    unit_rope: Option<MeshId>,
    particles: Vec<Particle>,
    flashes: Vec<Flash>,
    rng: u64,
    /// What this view put in the scene itself (frogs, insulators, poles and ropes): taken out
    /// again by `release`. The wires lie in the scene once for all the buses of a map.
    owned: Vec<usize>,
    /// Sparks the view has taken from the bus's rig since it began (for logs and tests).
    pub sparks_seen: u32,
}

impl TrolleyView {
    pub fn new(renderer: &Renderer, scene: &mut Scene, net: Arc<WireNet>) -> TrolleyView {
        let mat_wire = renderer.add_material(scene, None, AlphaMode::Opaque, [0.2, 0.17, 0.15, 1.0], false);
        let mut cells: hashbrown::HashMap<(i32, i32), Vec<usize>> = hashbrown::HashMap::new();
        for (i, s) in net.spans.iter().enumerate() {
            let mid = s.pts[s.pts.len() / 2];
            cells.entry(((mid.x / CHUNK).floor() as i32, (mid.y / CHUNK).floor() as i32)).or_default().push(i);
        }
        let mut chunk_count = 0usize;
        // (the wires are the map's, not the bus's: drawn by the first bus of the map, left in the
        // scene for the others and for after it has gone)
        let key = Arc::as_ptr(&net) as usize;
        let drawn = scene.trolley_wire_drawn.contains(&key);
        if !drawn && !net.drawn_by_map {
            scene.trolley_wire_drawn.push(key);
        }
        let mut owned: Vec<usize> = Vec::new();
        for (_, spans) in cells.into_iter().filter(|_| !net.drawn_by_map && !drawn) {
            let centre = net.spans[spans[0]].pts[0];
            let mut m = new_mesh();
            for si in spans {
                let s = &net.spans[si];
                for side in 0..2u8 {
                    let h = s.spacing * 0.5 * if side == 0 { 1.0 } else { -1.0 };
                    let pts: Vec<Vec3> = s.pts.iter().zip(&s.mit).map(|(p, mt)| (*p + *mt * h as f64 - centre).as_vec3()).collect();
                    tube(&mut m, &pts, WIRE_RADIUS, 5);
                }
            }
            if m.indices.is_empty() {
                continue;
            }
            renderer.begin_object(scene);
            let mesh = renderer.add_mesh(scene, &m);
            let instance = renderer.add_instance(scene, mesh, centre, Mat4::IDENTITY, vec![mat_wire]);
            scene.instances[instance].casts_shadow = false;
            chunk_count += 1;
        }
        // the frogs and the insulators
        let mut frogs = Vec::new();
        let mat_body = renderer.add_material(scene, None, AlphaMode::Opaque, [0.1, 0.1, 0.11, 1.0], false);
        let mat_tongue = renderer.add_material(scene, None, AlphaMode::Opaque, [0.75, 0.55, 0.12, 1.0], false);
        let mat_ins = renderer.add_material(scene, None, AlphaMode::Opaque, [0.62, 0.6, 0.55, 1.0], false);
        for sw in &net.switches {
            let node = &net.nodes[sw.node as usize];
            let toe = net.end_dir(sw.toe);
            let (straight, branch) = (net.end_dir(sw.straight), net.end_dir(sw.branch));
            let f = -toe;
            let mut body = new_mesh();
            for side in [-0.3f32, 0.3] {
                let r = Vec3::new(f.y as f32, -f.x as f32, 0.0) * side;
                let a = r - f.as_vec3() * 0.3;
                let b = r + f.as_vec3() * 0.3;
                tube(&mut body, &[a, b], 0.045, 8);
                // the hanger of the box
                tube(&mut body, &[(a + b) * 0.5, (a + b) * 0.5 + Vec3::Z * 0.35], 0.012, 4);
            }
            tube(&mut body, &[Vec3::new(f.y as f32, -f.x as f32, 0.0) * -0.3 + Vec3::Z * 0.35, Vec3::new(f.y as f32, -f.x as f32, 0.0) * 0.3 + Vec3::Z * 0.35], 0.03, 6);
            renderer.begin_object(scene);
            let bm = renderer.add_mesh(scene, &body);
            owned.push(renderer.add_instance(scene, bm, node.pos, Mat4::IDENTITY, vec![mat_body]));
            let mut tg = new_mesh();
            tube(&mut tg, &[Vec3::ZERO, Vec3::Y * 1.8], 0.022, 6);
            renderer.begin_object(scene);
            let tm = renderer.add_mesh(scene, &tg);
            let tongue = renderer.add_instance(scene, tm, node.pos + DVec3::Z * 0.2, Mat4::IDENTITY, vec![mat_tongue]);
            owned.push(tongue);
            frogs.push(FrogDraw { tongue, pos: node.pos, straight: straight.as_vec3(), branch: branch.as_vec3(), branch_set: !net.switch_branch(frogs.len()) });
        }
        for node in net.nodes.iter().filter(|n| n.insulator > 0.0 && n.switch.is_none()) {
            let Some(&(si, e)) = node.ends.first() else { continue };
            let dir = net.end_dir((si, e));
            let spacing = net.spans[si as usize].spacing;
            let mut m = new_mesh();
            for side in [-0.5f32, 0.5] {
                let r = Vec3::new(dir.y as f32, -dir.x as f32, 0.0) * spacing * side;
                let half = (node.insulator * 0.5).max(0.25);
                tube(&mut m, &[r - dir.as_vec3() * half, r + dir.as_vec3() * half], 0.028, 6);
            }
            renderer.begin_object(scene);
            let im = renderer.add_mesh(scene, &m);
            owned.push(renderer.add_instance(scene, im, node.pos, Mat4::IDENTITY, vec![mat_ins]));
        }
        log::info!("trolley wires: {} spans in {} chunks, {} frogs", net.spans.len(), chunk_count, frogs.len());
        TrolleyView { net, chunk_count, frogs, poles: Vec::new(), mat_pole: None, mat_rope: None, boom_meshes: Vec::new(), unit_rope: None, particles: Vec::new(), flashes: Vec::new(), rng: 0x2545_f491_4f6c_dd1d, owned, sparks_seen: 0 }
    }

    /// The bus is gone: its poles, ropes, frogs and insulators leave the scene (the wires of
    /// the map stay: they are the map's).
    pub fn release(&mut self, renderer: &Renderer, scene: &mut Scene) {
        for i in self.owned.drain(..) {
            renderer.remove_instance(scene, i);
        }
        self.poles.clear();
        self.frogs.clear();
        self.particles.clear();
        self.flashes.clear();
    }

    pub fn net(&self) -> &Arc<WireNet> {
        &self.net
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        ((self.rng >> 40) as f32) / (1u32 << 24) as f32
    }

    fn build_poles(&mut self, renderer: &Renderer, scene: &mut Scene, vehicle: &VehicleInstance) {
        let Some(g) = vehicle.trolley.as_deref() else { return };
        let mat_pole = renderer.add_material(scene, None, AlphaMode::Opaque, [0.16, 0.17, 0.18, 1.0], false);
        let mat_rope = renderer.add_material(scene, None, AlphaMode::Opaque, [0.55, 0.5, 0.4, 1.0], false);
        self.mat_pole = Some(mat_pole);
        self.mat_rope = Some(mat_rope);
        let mut rope = new_mesh();
        tube(&mut rope, &[Vec3::ZERO, Vec3::Y], 0.007, 4);
        renderer.begin_object(scene);
        self.unit_rope = Some(renderer.add_mesh(scene, &rope));
        for p in &g.rig.poles {
            let l = p.def.length;
            let mut m = new_mesh();
            tube(&mut m, &[Vec3::ZERO, Vec3::Y * 0.4], 0.055, 8);
            tube(&mut m, &[Vec3::Y * 0.4, Vec3::Y * l], 0.04, 8);
            // the shoe and its head
            tube(&mut m, &[Vec3::Y * (l - 0.05), Vec3::Y * (l + 0.16)], 0.05, 6);
            renderer.begin_object(scene);
            let mesh = renderer.add_mesh(scene, &m);
            self.boom_meshes.push(mesh);
            let draw = p.def.visible;
            let boom = renderer.add_instance(scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat_pole]);
            let ropei = renderer.add_instance(scene, self.unit_rope.unwrap(), DVec3::ZERO, Mat4::IDENTITY, vec![mat_rope]);
            scene.instances[boom].visible = draw;
            scene.instances[ropei].visible = draw;
            scene.instances[boom].casts_shadow = true;
            self.owned.extend([boom, ropei]);
            self.poles.push(PoleDraw { boom, rope: ropei });
        }
    }

    /// Per frame: the poles and ropes of the bus where they are now, and the arcs it has
    /// struck since the last frame.
    pub fn update(&mut self, renderer: &Renderer, scene: &mut Scene, dt: f32, vehicle: &mut VehicleInstance) {
        if self.poles.is_empty() && vehicle.trolley.is_some() {
            self.build_poles(renderer, scene, vehicle);
        }
        // the tongues lie along the way the frogs are set to
        for (i, f) in self.frogs.iter_mut().enumerate() {
            let branch = self.net.switch_branch(i);
            if branch != f.branch_set {
                f.branch_set = branch;
                let d = if branch { f.branch } else { f.straight };
                let y = Vec3::new(d.x, d.y, -0.04).normalize();
                let x = y.cross(Vec3::Z).normalize_or(Vec3::X);
                let z = x.cross(y);
                renderer.set_transform(scene, f.tongue, f.pos + DVec3::Z * 0.2, Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), glam::Vec4::W));
            }
        }
        let (pos, rot, bbox) = vehicle.trolley_frame();
        let rear = bbox.map_or(-5.5, |b| b[4] - b[1].abs() * 0.5);
        let legacy = vehicle.trolley.as_deref().is_some_and(|g| g.rig.def.legacy.is_some());
        let sparks = {
            let Some(g) = vehicle.trolley.as_deref_mut() else { return };
            for (k, p) in g.rig.poles.iter().enumerate() {
                let Some(d) = self.poles.get(k) else { continue };
                if !p.def.visible {
                    continue;
                }
                // (a bus whose model has folded poles of its own shows them while ours are down)
                let shown = !(legacy && p.mode == PoleMode::Down);
                // (a plain trolleybus's own raised pole mesh is turned by the pole's angles: the
                // drawn boom is for the buses that have none)
                scene.instances[d.boom].visible = shown && !legacy;
                scene.instances[d.rope].visible = shown;
                let base = pos + rot.transform_point3(p.def.base).as_dvec3();
                let tip_local = Vec3::new(p.psi.sin() as f32 * p.theta.cos() as f32, -(p.psi.cos() as f32) * p.theta.cos() as f32, p.theta.sin() as f32) * p.def.length;
                let dir_w = rot.transform_vector3(tip_local.normalize_or(Vec3::NEG_Y));
                let y = dir_w;
                let mut z = Vec3::Z - y * y.z;
                if z.length() < 0.05 {
                    z = rot.transform_vector3(Vec3::Y);
                }
                let z = z.normalize_or(Vec3::Z);
                let x = y.cross(z).normalize_or(Vec3::X);
                let m = Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), glam::Vec4::W);
                renderer.set_transform(scene, d.boom, base, m);
                // the rope from the tip down to the handle at the back
                let tip_w = base + (dir_w * p.def.length).as_dvec3();
                let handle = pos + rot.transform_point3(Vec3::new(p.def.base.x.signum() * 0.45, rear + 0.1, 0.9)).as_dvec3();
                let v = (handle - tip_w).as_vec3();
                let len = v.length().max(0.05);
                let ry = v / len;
                let rz = {
                    let t = Vec3::Z - ry * ry.z;
                    if t.length() < 0.05 { Vec3::X } else { t.normalize() }
                };
                let rx = ry.cross(rz).normalize_or(Vec3::X);
                let rm = Mat4::from_cols((rx).extend(0.0), (ry * len).extend(0.0), rz.extend(0.0), glam::Vec4::W);
                renderer.set_transform(scene, d.rope, tip_w, rm);
            }
            g.rig.take_sparks()
        };
        for s in sparks {
            self.add_spark(s);
        }
        self.advance(dt);
    }

    fn add_spark(&mut self, s: Spark) {
        self.sparks_seen += 1;
        if omsi_cfg::flags::OMSI_DEBUG_TROLLEY.is_set() {
            log::info!("trolley spark {:?}: {:.0} J, {:.0} A at ({:.1}, {:.1}, {:.2})", s.kind, s.energy, s.amps, s.pos.x, s.pos.y, s.pos.z);
        }
        let n = ((s.energy.sqrt() * 1.1) as usize).clamp(2, 40);
        let (speed, heat0) = match s.kind {
            SparkKind::Dewire => (7.0, 1.0),
            SparkKind::Insulator => (5.5, 1.0),
            SparkKind::Break => (4.5, 0.9),
            SparkKind::Frog => (3.5, 0.7),
            SparkKind::Make => (3.0, 0.6),
        };
        for _ in 0..n {
            let (a, b, c) = (self.rand(), self.rand(), self.rand());
            let az = a * std::f32::consts::TAU;
            let up = b * 0.9 - 0.25;
            let sp = speed * (0.35 + 0.65 * c);
            let vel = Vec3::new(az.cos() * (1.0 - up.abs() * 0.5), az.sin() * (1.0 - up.abs() * 0.5), up) * sp;
            let life = 0.12 + 0.35 * self.rand();
            let size = 0.03 + 0.035 * self.rand();
            self.particles.push(Particle { pos: s.pos, vel, age: 0.0, life, size, heat: heat0 });
        }
        if self.particles.len() > 600 {
            let extra = self.particles.len() - 600;
            self.particles.drain(0..extra);
        }
        self.flashes.push(Flash { pos: s.pos, age: 0.0, energy: s.energy });
    }

    fn advance(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            p.vel.z -= 9.81 * 0.9 * dt;
            p.vel *= (1.0 - 1.2 * dt).max(0.0);
            p.pos += (p.vel * dt).as_dvec3();
        }
        self.particles.retain(|p| p.age < p.life);
        for f in &mut self.flashes {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < 0.18);
    }

    /// The arcs' glare and light, into this frame's lists (after the lamps were collected).
    pub fn push_effects(&self, scene: &mut Scene, vehicle: &VehicleInstance) {
        if omsi_cfg::flags::OMSI_DEBUG_TROLLEY.is_set() && (!self.particles.is_empty() || !self.flashes.is_empty()) {
            log::info!("trolley effects: {} sparks, {} flashes", self.particles.len(), self.flashes.len());
        }
        // the frogs' lamps: green with the tongue straight, amber to the branch
        for f in &self.frogs {
            let (color, d) = if f.branch_set { ([1.0, 0.65, 0.1], f.branch) } else { ([0.2, 1.0, 0.3], f.straight) };
            let _ = d;
            scene.coronas.push(Corona { position: f.pos + DVec3::new(0.0, 0.0, 0.75), size: 0.18, color, brightness: 1.2, direction: Vec3::ZERO, cone_cos: -1.0, ..Default::default() });
        }
        for p in &self.particles {
            let t = p.age / p.life;
            // white-hot, then orange, fading
            let heat = p.heat * (1.0 - t);
            let color = [1.0, 0.55 + 0.4 * heat, 0.25 + 0.65 * heat * heat];
            scene.coronas.push(Corona { position: p.pos, size: p.size * (1.0 + 2.0 * (1.0 - t)), color, brightness: 2.5 * (1.0 - t * t), direction: Vec3::ZERO, cone_cos: -1.0, ..Default::default() });
        }
        for f in &self.flashes {
            let t = f.age / 0.18;
            let k = (1.0 - t) * (1.0 - t);
            let size = 0.12 + f.energy.sqrt() * 0.04;
            scene.coronas.push(Corona { position: f.pos, size, color: [0.8, 0.88, 1.0], brightness: 3.0 * k, direction: Vec3::ZERO, cone_cos: -1.0, ..Default::default() });
            scene.lights.push(PointLight { position: f.pos, radius: 6.0 + f.energy.sqrt() * 0.9, color: [0.7, 0.82, 1.0], intensity: (f.energy.sqrt() / 7.0).min(4.0) * k, flash: true, ..Default::default() });
        }
        // an open contact carrying current burns: a flickering glow at the shoe
        if let Some(g) = vehicle.trolley.as_deref() {
            if g.rig.arc > 0.0 {
                for p in g.rig.poles.iter().filter(|p| p.mode == PoleMode::OnWire && !p.closed) {
                    let flick = 0.6 + 0.4 * ((self.rng >> 5) as f32 % 1.0);
                    scene.coronas.push(Corona { position: p.tip, size: 0.08 + 0.1 * g.rig.arc, color: [0.8, 0.88, 1.0], brightness: 2.5 * flick, direction: Vec3::ZERO, cone_cos: -1.0, ..Default::default() });
                }
            }
        }
    }

    /// Chunks of wire and their centres, for tests.
    pub fn chunk_count(&self) -> usize {
        self.chunk_count
    }

    pub fn pole_instances(&self) -> Vec<usize> {
        self.poles.iter().map(|p| p.boom).collect()
    }
}
