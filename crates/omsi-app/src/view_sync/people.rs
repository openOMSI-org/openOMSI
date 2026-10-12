//! The people as the renderer sees them: their meshes and instances, the GPU side of the
//! human types, the posing and skinning, and the coins on the cash desk.
//!
//! The simulation does not touch the renderer. What it does that the renderer has to
//! follow - somebody appearing, somebody going, coins put on the desk - it writes down as
//! `BodyOp`s (`PeopleSim::bodies`), and the view replays them in the order they happened
//! ([`apply_people`]), right after the call that made them: the renderer sees the very
//! calls, in the very order, it saw when the simulation made them itself.
//!
//! The replay follows every `Humans` call that can make or remove people (`tick` - in
//! `steps::tick_humans` -, `populate`, `avatar`, `mirror_add`, `seed_riders`), with the
//! people's `PeopleView` (`SimView::people`, apart from the `Humans`); the coins handed out,
//! the ticket blocks and the posing are the view sync's (`view_sync::sync`).

use crate::humans::Humans;
use crate::scene::World;
use glam::{DVec3, Mat4, Vec3};
use hashbrown::HashMap;
use omsi_render::{AlphaMode, MaterialId, MeshId, Renderer, Scene};
use omsi_sim::human::{skin, HumanType};
use omsi_sim::people::cabin::DrawnBus;
use omsi_sim::people::{BodyOp, BusId, Place};
use omsi_sim::view_events::{PeopleFrame, PersonFrame};
use omsi_sim::VehicleInstance;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod animation;

/// About how long one person's pose and skinning takes (s), and the work worth a helper
/// of the pool (see `omsi_cfg::par`).
const POSE_SECS: f64 = 25e-6;
const POSE_PER_HELPER: f64 = 0.5e-3;

/// The renderer's side of the people, kept apart from the simulation (`view_sync::SimView`):
/// it starts afresh with every `Humans` (`PeopleView::new`, by `Humans::new`).
#[derive(Default)]
pub(crate) struct PeopleView {
    pub(crate) hidden: Vec<usize>,
    /// GPU side of the human types, shared by everyone of a type: textures by file and the
    /// materials of every (type, mesh) - each person used to upload its own copies - and
    /// the meshes and instances of the people who have gone, taken over by the next person
    /// of the same type (the skinned vertices are rewritten anyway). Without that every
    /// passenger who ever appeared kept a mesh, its textures and materials on the GPU.
    pub(crate) gpu_textures: HashMap<PathBuf, Option<omsi_render::TextureId>>,
    /// Per (type, clothing variant, mesh): its materials, and the meshes and instances of
    /// people who have gone, kept for the next person dressed alike.
    pub(crate) gpu_materials: HashMap<(usize, usize, usize), Vec<MaterialId>>,
    pub(crate) spare: HashMap<(usize, usize, usize), Vec<(MeshId, usize)>>,
    /// Everybody's meshes, instances and pose as drawn, by person id (the simulation's
    /// people hold none of it: `BodyOp::Spawn` makes the meshes here, `Retire` lets them go,
    /// `Renamed` follows a new id).
    pub(crate) people: HashMap<u32, PersonView>,
    /// What the posing reads of the simulation, taken at each sync (its buffer kept).
    pub(crate) frame: PeopleFrame,
    pub(crate) sync_frame: u32,
    /// Animation choice for this session, refreshed on Android in-process launches.
    pub(crate) enhanced_poses: bool,
    /// Simulation time of the last `sync`.
    pub(crate) last_sync: f64,
    /// Frames synced, people posed and skinned, the time that took and the part of it spent
    /// uploading (ms), in total.
    pub(crate) pose_stats: (u32, usize, f64, f64),
    /// Who is posed this frame, per person (kept from frame to frame).
    due: Vec<bool>,
    /// `OMSI_TRACE_PAX=<csv>`: every person near the eye, every frame (see `sync`).
    pub(crate) trace: Option<std::io::BufWriter<std::fs::File>>,
    /// The tear-off ticket blocks of the player's bus (`money::TicketBlocks`).
    pub(crate) ticket_blocks: Option<crate::money::TicketBlocks>,
}

impl PeopleView {
    pub(crate) fn new() -> PeopleView {
        PeopleView {
            hidden: Vec::new(),
            gpu_textures: HashMap::new(),
            gpu_materials: HashMap::new(),
            spare: HashMap::new(),
            people: HashMap::new(),
            frame: PeopleFrame::default(),
            sync_frame: 0,
            enhanced_poses: animation::enabled(),
            last_sync: 0.0,
            pose_stats: (0, 0, 0.0, 0.0),
            due: Vec::new(),
            trace: omsi_cfg::flags::OMSI_TRACE_PAX.var().and_then(|f| std::fs::File::create(f).ok()).map(|f| {
                use std::io::Write;
                let mut w = std::io::BufWriter::new(f);
                let _ = writeln!(w, "t,id,state,ground,posed,x,y,z,heading,lx,ly,lz,rx,ry,rz,vx,vy");
                w
            }),
            ticket_blocks: None,
        }
    }

    /// The renderer instances of the people, by person id in its order
    /// (`OMSI_SCENE_DIGEST`).
    pub(crate) fn instances(&self) -> Vec<(u64, usize)> {
        let mut ids: Vec<u32> = self.people.keys().copied().collect();
        ids.sort_unstable();
        ids.iter().flat_map(|id| self.people[id].meshes.iter().map(|m| (*id as u64, m.1))).collect()
    }

    /// `OMSI_TRACE_PAX` is writing a trace.
    pub(crate) fn tracing(&self) -> bool {
        self.trace.is_some()
    }
}

/// Bring the renderer up to what the people's calls did since the last time (see
/// [`BodyOp`]): called right after each call that can make or remove people, where those
/// calls used to show them themselves.
pub(crate) fn apply_people(h: &mut Humans, view: &mut PeopleView, world: &World, renderer: &Renderer, scene: &mut Scene) {
    h.show_bodies(view, world, renderer, scene);
}

impl Humans {
    /// Bring the renderer up to what the simulation did (see [`BodyOp`]).
    pub(crate) fn show_bodies(&mut self, view: &mut PeopleView, world: &World, renderer: &Renderer, scene: &mut Scene) {
        self.replay_bodies(view, Some(world), renderer, scene);
    }

    /// Bring the renderer up to the people who went since (the frame's `sync`; whatever
    /// made people or coins has shown them already).
    pub(super) fn catch_up_bodies(&mut self, view: &mut PeopleView, renderer: &Renderer, scene: &mut Scene) {
        self.replay_bodies(view, None, renderer, scene);
    }

    /// Replay the ops in order: up to the first that needs the world when there is none.
    fn replay_bodies(&mut self, view: &mut PeopleView, world: Option<&World>, renderer: &Renderer, scene: &mut Scene) {
        if self.sim.bodies.ops.is_empty() {
            return;
        }
        let ops = std::mem::take(&mut self.sim.bodies.ops);
        let mut ops = ops.into_iter();
        while let Some(op) = ops.next() {
            let world = match world {
                Some(w) => w,
                None if op.needs_world() => {
                    // (cannot happen: whatever makes such ops shows them right after it)
                    log::warn!("people: {} body changes left for later", ops.len() + 1);
                    self.sim.bodies.ops = std::iter::once(op).chain(ops).collect();
                    return;
                }
                None => {
                    match op {
                        BodyOp::Retire { id, tkey, variant } => view.retire_meshes(id, tkey, variant),
                        BodyOp::Renamed { from, to } => view.rename(from, to),
                        _ => unreachable!(),
                    }
                    continue;
                }
            };
            match op {
                BodyOp::Spawn { id, ty, variant, position } => {
                    let meshes = view.make_meshes(world, renderer, scene, &ty, variant, position);
                    view.people.entry(id).or_default().meshes = meshes;
                }
                BodyOp::Retire { id, tkey, variant } => view.retire_meshes(id, tkey, variant),
                BodyOp::Renamed { from, to } => view.rename(from, to),
                BodyOp::Coins { coins, point, var, change, parent } => {
                    if let Some(m) = self.sim.money.as_mut() {
                        crate::money::place(m, world, renderer, scene, &coins, point, var, change, parent.as_deref());
                    }
                }
            }
        }
    }
}

impl PeopleView {
    /// Person `id` has gone: their meshes hidden and kept for the next person of the type
    /// (`tkey`) dressed alike (`variant`).
    fn retire_meshes(&mut self, id: u32, tkey: usize, variant: usize) {
        let meshes = self.people.remove(&id).map(|p| p.meshes).unwrap_or_default();
        for (mi, m) in meshes.iter().enumerate() {
            self.hidden.push(m.1);
            self.spare.entry((tkey, variant, mi)).or_default().push(*m);
        }
    }

    /// Person `from` is called `to` from now on.
    fn rename(&mut self, from: u32, to: u32) {
        if let Some(m) = self.people.remove(&from) {
            self.people.insert(to, m);
        }
    }

    /// The meshes and instances of somebody new: those of somebody gone dressed alike, else
    /// made afresh (with the type's materials, made once per type and clothes).
    fn make_meshes(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene, ty: &Arc<HumanType>, variant: usize, position: DVec3) -> Vec<(MeshId, usize)> {
        let tkey = Arc::as_ptr(ty) as usize;
        let mut meshes = Vec::new();
        for (mi, hm) in ty.meshes.iter().enumerate() {
            let key = (tkey, variant, mi);
            // somebody of this type has gone: their mesh and instance
            if let Some((id, inst)) = self.spare.get_mut(&key).and_then(|v| v.pop()) {
                self.hidden.retain(|h| *h != inst);
                renderer.set_transform(scene, inst, position, Mat4::IDENTITY);
                renderer.set_params(scene, inst, &[], true, &[]);
                meshes.push((id, inst));
                continue;
            }
            if !self.gpu_materials.contains_key(&key) {
                let dirs = ty.texture_dirs(&world.root);
                let mut mats = Vec::new();
                for (k, m) in hm.materials.iter().enumerate() {
                    // the variant's texture from its own folder first, else the default
                    let (name, first) = ty.variant_texture(&m.texture, variant);
                    let mut look: Vec<&Path> = first.into_iter().collect();
                    look.extend(dirs.iter().map(|p| p.as_path()));
                    let found = omsi_texture::find_texture(name, &look)
                        .or_else(|| omsi_texture::find_texture(&m.texture, &look));
                    if found.is_none() && !m.texture.trim().is_empty() {
                        log::warn!(
                            "human {}: texture {} not found",
                            ty.def.path.display(),
                            m.texture
                        );
                    }
                    let tex = match found {
                        Some(path) => match self.gpu_textures.get(&path) {
                            Some(t) => *t,
                            None => {
                                let t = world
                                    .textures
                                    .get_gpu_fast(&path)
                                    .map(|(img, _)| renderer.add_texture_data(scene, &img));
                                world.textures.release(&path);
                                self.gpu_textures.insert(path, t);
                                t
                            }
                        },
                        None => None,
                    };
                    let alpha = match hm.alpha.get(k).copied().unwrap_or(0) {
                        1 => AlphaMode::Test,
                        2 => AlphaMode::Blend,
                        _ => AlphaMode::Opaque,
                    };
                    mats.push(renderer.add_material(scene, tex, alpha, [1.0; 4], false));
                }
                self.gpu_materials.insert(key, mats);
            }
            let mats = self.gpu_materials[&key].clone();
            let id = renderer.add_mesh(scene, &hm.data);
            let inst = renderer.add_instance(scene, id, position, Mat4::IDENTITY, mats);
            meshes.push((id, inst));
        }
        meshes
    }
}

impl Humans {
    /// Coins the driver handed out (from the host's GiveChangeCoin list) onto the change point.
    pub fn give_change(
        &mut self,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
        coins: &[usize],
    ) {
        if coins.is_empty() {
            return;
        }
        let point = self.sim.player_cabin.as_ref().and_then(|c| {
            c.data
                .change_points
                .first()
                .or(c.data.money_points.first())
                .cloned()
        });
        if let (Some(m), Some(pt)) = (self.sim.money.as_mut(), point) {
            crate::money::place(
                m,
                world,
                renderer,
                scene,
                coins,
                Vec3::from(pt.pos),
                pt.var,
                true,
                pt.parent.as_deref(),
            );
        }
    }

    pub(crate) fn sync_money(&mut self, view: &mut PeopleView, world: &World, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance) {
        if let Some(m) = self.sim.money.as_mut() {
            crate::money::sync(m, renderer, scene, bus);
        }
        if let Some(pack) = self.sim.tickets.as_ref().map(|t| &t.path) {
            // (compared in place: both paths were copied for it every frame)
            let fresh = view.ticket_blocks.as_ref().is_none_or(|b| b.made_for.0 != bus.ty.def.path || b.made_for.1 != *pack);
            if fresh && !bus.ty.def.attachments.is_empty() {
                view.ticket_blocks = Some(crate::money::TicketBlocks::new(world, renderer, scene, bus, pack));
            }
        }
        if let Some(b) = view.ticket_blocks.as_ref() {
            b.sync(renderer, scene, bus);
        }
    }

    /// Skin the people due for a new pose and push transforms to the renderer: the people
    /// who went since first, then what the posing reads of them is taken into the view's
    /// `PeopleFrame` and posed from it ([`PeopleView::pose`]).
    pub(super) fn sync(&mut self, view: &mut PeopleView, world: &World, renderer: &Renderer, scene: &mut Scene, camera: DVec3) {
        self.catch_up_bodies(view, renderer, scene);
        self.sim.snapshot_into(&mut view.frame);
        view.pose(world, renderer, scene, camera);
    }
}

impl PeopleView {
    pub(crate) fn follow_buses<'a>(&self, vehicle_of: impl Fn(BusId) -> Option<&'a VehicleInstance>, avatar: Option<u32>, renderer: &Renderer, scene: &mut Scene) -> Option<DVec3> {
        let mut drawn: HashMap<BusId, Option<(usize, DrawnBus)>> = HashMap::new();
        let mut moved = None;
        for p in &self.frame.people {
            let Place::Bus(b, l) = p.place else { continue };
            let found = drawn.entry(b).or_insert_with(|| {
                let k = self.frame.buses.iter().position(|x| x.id == b)?;
                vehicle_of(b).map(|v| (k, DrawnBus::of(v, &self.frame.buses[k].cabin)))
            });
            let Some((k, d)) = found.as_ref() else { continue };
            let (at, heading, tilt) = self.frame.buses[*k].follow(d, l, p.position, p.heading, p.tilt);
            if avatar == Some(p.id) {
                moved = Some(at - p.position);
            }
            let Some(st) = self.people.get(&p.id) else { continue };
            let xf = tilt * Mat4::from_rotation_z((-heading).to_radians() as f32);
            for (_, inst) in &st.meshes {
                renderer.set_transform(scene, *inst, at, xf);
            }
        }
        moved
    }
}

/// One person as the renderer has them: their meshes and instances, and their pose as
/// drawn (none of it is the simulation's).
#[derive(Default)]
pub(crate) struct PersonView {
    pub(crate) meshes: Vec<(MeshId, usize)>,
    /// Skinned positions and normals, per mesh.
    skins: Vec<(Vec<Vec3>, Vec<Vec3>)>,
    /// The bones the skins were made with, and whether this frame's pose changed them
    /// (somebody standing still keeps the mesh of the frame before: skinning and uploading
    /// thirty waiting people every frame took 2 ms of the frame at a bus station).
    skin_bones: Option<[glam::Affine3A; omsi_sim::human::SLOTS]>,
    pose_changed: bool,
    /// Whether this person has ever been posed (an unposed model is the file's T-pose).
    skinned: bool,
    /// Frames since the last pose and where the person stood then (the
    /// feet of a mesh posed a frame ago stay on the floor when it is drawn there).
    since_posed: u32,
    posed_at: (DVec3, f64),
    /// The interior light as drawn: it follows the bus's over a moment (stepping through
    /// the door, people lit up and went dark again from one frame to the next).
    lit: f32,
    /// Ankles of the last pose (model frame), for `OMSI_TRACE_PAX`.
    pub(super) ankles: [Vec3; 2],
    /// The earlier IK/foot-planted animation, allocated only for the enhanced poses.
    pub(super) procedural: Option<omsi_sim::human::Pose>,
}

impl PeopleView {
    /// Pose and skin the people of `self.frame` that are due, and push their transforms to
    /// the renderer. Near people are posed every frame, far ones every few frames and people
    /// out of view rarely; the posing and skinning run in parallel. Reads nothing of the
    /// simulation but the frame.
    pub(crate) fn pose(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene, camera: DVec3) {
        for inst in self.hidden.drain(..) {
            renderer.set_params(scene, inst, &[], false, &[]);
        }
        let started = std::time::Instant::now();
        self.sync_frame = self.sync_frame.wrapping_add(1);
        let frame = std::mem::take(&mut self.frame);
        let eye = frame.eye;
        let from = eye.map(|e| e.pos).unwrap_or(camera);
        // synced only now and then (offscreen snapshots): everybody is posed afresh
        let all = frame.time - self.last_sync > 0.12;
        let sdt = (frame.time - self.last_sync).clamp(0.0, 0.5) as f32;
        self.last_sync = frame.time;
        // everybody's pose as drawn, in the frame's order (put back below)
        let mut states: Vec<PersonView> = frame.people.iter().map(|p| self.people.remove(&p.id).unwrap_or_default()).collect();
        let mut due = std::mem::take(&mut self.due);
        due.clear();
        for (k, (p, st)) in frame.people.iter().zip(states.iter_mut()).enumerate() {
            st.since_posed = st.since_posed.saturating_add(1);
            let d = p.position + DVec3::Z * 0.9 - from;
            let dist = d.length();
            let visible = match eye {
                Some(e) => dist < 4.0 || d.dot(e.fwd) / dist.max(1e-3) > e.cos_half - 0.15,
                None => true,
            };
            // everybody the eye can make out is posed every frame: a pose every other
            // frame at 12-30 m moved walkers in steps and made planted feet shiver
            // (within 30 m everybody, seen or not: the mirrors show the people behind the
            // bus, who were posed every twelfth frame and moved in jerks there)
            let every = if dist < 30.0 {
                1
            } else if !visible {
                12
            } else if dist < 45.0 {
                1
            } else if dist < 90.0 {
                2
            } else if dist < 160.0 {
                3
            } else {
                6
            };
            let every = if p.vel.length_squared() < 1e-4 && dist > 20.0 { every * 2 } else { every };
            // spread the far ones over the frames
            let turn = (self.sync_frame + k as u32) % every == 0;
            due.push(
                !st.skinned
                    || all
                    || (st.since_posed >= every && (turn || st.since_posed >= 2 * every)),
            );
        }
        let n_due = due.iter().filter(|d| **d).count();
        let enhanced = self.enhanced_poses;
        let buses: HashMap<_, _> = if enhanced {
            frame.buses.iter().map(|b| (b.id, b)).collect()
        } else {
            HashMap::new()
        };
        let pose_one = |(p, st): &mut (&PersonFrame, &mut PersonView)| {
            // Preserve the OMSI-original pose unless the enhanced A/B mode is requested.
            let bones = if enhanced && !p.puppet {
                let dt = (sdt * st.since_posed.max(1) as f32).min(0.25);
                animation::bones(p, st, dt, world, &buses)
            } else {
                omsi_sim::human::slots_from_omsi(&p.anim.bones(&p.ty.omsi))
            };
            let ty = &p.ty;
            let PersonView { skins, skin_bones, pose_changed, .. } = &mut **st;
            *pose_changed = false;
            if bones.iter().any(|b| !b.is_finite()) && !skins.is_empty() {
                // keep the last good mesh (the rest pose would be the file's T-pose)
                return;
            }
            // (the same bones as the mesh was made with: nothing to skin or upload)
            if skins.len() == ty.meshes.len() && skin_bones.as_ref().is_some_and(|b| b.iter().zip(&bones).all(|(a, c)| a.abs_diff_eq(*c, 1e-6))) {
                return;
            }
            skins.resize_with(ty.meshes.len(), Default::default);
            for (k, m) in ty.meshes.iter().enumerate() {
                let (pos, nrm) = &mut skins[k];
                skin(m, &bones, pos, nrm);
            }
            *skin_bones = Some(bones);
            *pose_changed = true;
        };
        // Posed on this thread, with workers of the pool joining in only when there is work
        // for them (`frame_each`): handed to the pool whole (a par_iter from this thread) the
        // frame waited for its busy workers - Spandau, 9 people a frame: 2.4 ms against ~0.2
        // ms posed here, and up to 39 ms in the frames where the workers were busy.
        let mut due_people: Vec<(&PersonFrame, &mut PersonView)> =
            frame.people.iter().zip(states.iter_mut()).zip(&due).filter(|(_, go)| **go).map(|(pair, _)| pair).collect();
        let helpers = omsi_cfg::par::helpers_for(due_people.len() as f64 * POSE_SECS, POSE_PER_HELPER);
        omsi_cfg::par::frame_each(&mut due_people, helpers, |pair| pose_one(pair));
        drop(due_people);
        let upload = std::time::Instant::now();
        for ((p, st), &go) in frame.people.iter().zip(states.iter_mut()).zip(&due) {
            if go {
                if st.pose_changed || !st.skinned {
                    for (k, (id, _)) in st.meshes.iter().enumerate() {
                        if let Some((pos, nrm)) = st.skins.get(k) {
                            renderer.update_mesh(scene, *id, pos, nrm, &p.ty.meshes[k].data.uvs);
                        }
                    }
                }
                st.skinned = true;
                st.since_posed = 0;
                st.posed_at = (p.position, p.heading);
            }
            // riders go with their bus; on the ground a mesh not posed this frame goes on
            // with the body too (left where it was posed, a far walker moved in jerks -
            // its feet slide a few centimetres instead, which nobody sees at that distance)
            let (at, heading) = match p.place {
                Place::Ground if go => st.posed_at,
                _ => (p.position, p.heading),
            };
            // (riders with the tilt of their floor)
            let tilt = if matches!(p.place, Place::Bus(..)) { p.tilt } else { Mat4::IDENTITY };
            let xf = tilt * Mat4::from_rotation_z((-heading).to_radians() as f32);
            let lit_to = if matches!(p.place, Place::Bus(..)) { p.interior } else { 0.0 };
            st.lit += (lit_to - st.lit) * (sdt / 0.4).min(1.0);
            for (_, inst) in &st.meshes {
                renderer.set_transform(scene, *inst, at, xf);
                renderer.set_interior(scene, *inst, st.lit * 0.5);
            }
            if p.hidden.is_some() && omsi_cfg::flags::OMSI_DEBUG_FOOT.is_set() && self.sync_frame % 30 == 0 {
                log::info!("avatar drawn at ({:.2}, {:.2}, {:.2}) heading {:.0} place {:?} go {}", at.x, at.y, at.z, heading, matches!(p.place, Place::Ground), go);
            }
            if let Some(hide) = p.hidden {
                // (the first-person view: the avatar's own body out of the picture; set
                // every frame, the posing would show it again)
                for (_, inst) in &st.meshes {
                    renderer.set_params(scene, *inst, &[], !hide, &[]);
                }
            }
            if let Some(t) = self.trace.as_mut() {
                // OMSI_TRACE_PAX: where the mesh is drawn and where its ankles are, per frame
                if (at - from).length() < 40.0 {
                    use std::io::Write;
                    let a = |k: usize| at + (xf.transform_vector3(st.ankles[k])).as_dvec3();
                    let (l, r) = (a(0), a(1));
                    let _ = writeln!(
                        t,
                        "{:.4},{},{},{},{},{:.4},{:.4},{:.4},{:.2},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.3},{:.3}",
                        frame.time,
                        p.id,
                        p.state,
                        matches!(p.place, Place::Ground) as u8,
                        go as u8,
                        at.x,
                        at.y,
                        at.z,
                        heading,
                        l.x,
                        l.y,
                        l.z,
                        r.x,
                        r.y,
                        r.z,
                        p.vel.x,
                        p.vel.y
                    );
                }
            }
        }
        // OMSI_CHECK_TPOSE=1: everybody drawn with the arms out (the file's rest pose): the
        // skinned mesh wider than 1.3 m from hand to hand
        if omsi_cfg::flags::OMSI_CHECK_TPOSE.is_set() {
            for (p, st) in frame.people.iter().zip(&states) {
                let [who, pax] = p.debug.clone().unwrap_or_default();
                let Some((pos, _)) = st.skins.first() else {
                    log::info!("t-pose? {who}: never skinned");
                    continue;
                };
                let (lo, hi) = pos.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v.x), hi.max(v.x)));
                if hi - lo > 1.3 {
                    log::info!("t-pose: {who} width {:.2} skinned {} {pax}", hi - lo, st.skinned);
                }
            }
        }
        drop(buses);
        for (p, st) in frame.people.iter().zip(states) {
            self.people.insert(p.id, st);
        }
        self.frame = frame;
        self.due = due;
        self.pose_stats.0 += 1;
        self.pose_stats.1 += n_due;
        self.pose_stats.2 += started.elapsed().as_secs_f64() * 1000.0;
        self.pose_stats.3 += upload.elapsed().as_secs_f64() * 1000.0;
    }
}
