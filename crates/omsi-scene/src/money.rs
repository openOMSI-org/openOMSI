//! Money on the cash desk, drawn: the coins a passenger pays with and the change the driver
//! hands out (which coins, and where each lies, is omsi-sim's `people::money`).

use crate::World;
use glam::{DVec3, Mat4, Vec3};
use omsi_geometry::mesh_from_o3d;
use omsi_render::{AlphaMode, MaterialId, MeshId, Renderer, Scene};
use omsi_sim::VehicleInstance;
use std::path::{Path, PathBuf};

pub use omsi_sim::people::Money;

struct DeskTicket {
    instance: usize,
    ticket_type: usize,
    local: Mat4,
    parent: Option<String>,
    layer: u32,
}

/// The mesh and materials of a coin or note, made the first time it is put down.
fn mesh(m: &mut Money, world: &World, renderer: &Renderer, scene: &mut Scene, coin: usize) -> Option<(MeshId, Vec<MaterialId>)> {
    if let Some(c) = m.meshes.get(&coin) {
        return Some(c.clone());
    }
    let c = m.currency.as_ref()?;
    let n = c.coins.len();
    let file = if coin < n { c.coins.get(coin)?.0.clone() } else { c.bills.get(coin - n)?.0.clone() };
    let o3d = omsi_o3d::load_mesh(&omsi_cfg::resolve_path(&m.dir, &file)).map_err(|e| log::warn!("{e}")).ok()?;
    let dirs = [m.dir.clone(), omsi_cfg::resolve_path(&world.root, "Texture")];
    let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
    let mats: Vec<MaterialId> = o3d
        .materials
        .iter()
        .map(|mat| {
            let tex = omsi_texture::find_texture(&mat.texture, &dirs_ref).and_then(|p| world.textures.get_gpu_fast(&p)).map(|(img, _)| renderer.add_texture_data(scene, &img));
            renderer.add_material(scene, tex, AlphaMode::Opaque, [1.0; 4], false)
        })
        .collect();
    let id = renderer.add_mesh(scene, &mesh_from_o3d(&o3d));
    m.meshes.insert(coin, (id, mats.clone()));
    Some((id, mats))
}

/// Put coins on a point of the cabin as Omsi.exe does (sub_7e7e08): each one flat at the
/// point's height, somewhere in the variation rectangle and turned at random about the
/// vertical. Nothing is stacked: the coins had been raised 3 mm per coin already lying
/// there, so a driver clicking change built an endless tower on the tray.
/// A point of `[ticket_sale_money_point_2]` / `[ticket_sale_change_point_2]` names the
/// mesh (`parent`, its `[mesh_ident]`) the money lies on: it moves with that mesh - the
/// change tray in a cash desk swinging with the driver's door (#1468).
#[allow(clippy::too_many_arguments)]
pub fn place(m: &mut Money, world: &World, renderer: &Renderer, scene: &mut Scene, coins: &[usize], point: Vec3, var: [f32; 2], change: bool, parent: Option<&str>) {
    for coin in coins {
        let Some((id, mats)) = mesh(m, world, renderer, scene, *coin) else { continue };
        let local = Money::coin_place(point, var, [m.rand_f(), m.rand_f(), m.rand_f()]);
        let inst = renderer.add_instance(scene, id, DVec3::ZERO, Mat4::IDENTITY, mats);
        m.placed.push((inst, local, *coin, change, parent.map(str::to_string)));
    }
}

/// Show the coins lying in the bus where they lie now; the ones taken away hidden.
pub fn sync(m: &mut Money, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance) {
    for inst in m.hidden.drain(..) {
        renderer.set_params(scene, inst, &[], false, &[]);
    }
    let rot = bus.body_rotation();
    for (inst, local, _, _, parent) in &m.placed {
        let on = parent.as_deref().map(|n| mesh_of(bus, n)).unwrap_or(Mat4::IDENTITY);
        renderer.set_transform(scene, *inst, bus.position, rot * on * *local);
    }
}

/// The transform of the bus's mesh named `name` (its `[mesh_ident]`) in the bus frame.
fn mesh_of(bus: &VehicleInstance, name: &str) -> Mat4 {
    let defs = &bus.ty.model.meshes;
    bus.ty
        .meshes
        .iter()
        .rposition(|m| defs.get(m.def_index).and_then(|d| d.mesh_ident.as_deref()).is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim())))
        .and_then(|i| bus.mesh_transforms.get(i).copied())
        .unwrap_or(Mat4::IDENTITY)
}

/// The coin of the change lying on the tray OR coin/bill on the desk a ray from `origin` along `dir` points at
/// (its place in `Money::placed`), the nearest: within 2 cm of a coin's middle, as near
/// as a coin is wide. A click on it picks it up again, as in Omsi.exe (#1354, #2399: the
/// coins could not be clicked).
pub fn money_hit(
    m: &Money,
    origin: DVec3,
    dir: Vec3,
    bus: &VehicleInstance,
) -> Option<usize> {
    let rot = bus.body_rotation();
    let o = (origin - bus.position).as_vec3();
    let dir = dir.normalize_or_zero();

    m.placed
        .iter()
        .enumerate()
        .filter_map(|(k, (_, local, _, _, parent))| {
            let on = parent
                .as_deref()
                .map(|n| mesh_of(bus, n))
                .unwrap_or(Mat4::IDENTITY);

            let c = (rot * on * *local).transform_point3(Vec3::ZERO);
            let t = (c - o).dot(dir);

            (t > 0.0 && t < 3.0 && (o + dir * t).distance(c) < 0.02)
                .then_some((k, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|h| h.0)
}

/// The tear-off ticket blocks of a bus without a ticket printer: the ticket pack's
/// `Ticket_<n>_block.o3d` hung on the bus's `[new_attachment]` point n (the stock SD's
/// "ticket block attach points"), moving with the bus. A click on block n tears off a
/// ticket of type n - `GivenTicket` n, as a printer's script hands one over (#1413).
pub struct TicketBlocks {
    /// The bus type and the pack they were made for.
    pub made_for: (PathBuf, PathBuf),
    /// (instance, place in the bus frame, ticket type, the mesh for clicks)
    blocks: Vec<(usize, Mat4, usize, omsi_geometry::MeshData)>,
    /// Individual ticket meshes, by ticket type.
    single_meshes: Vec<(usize, MeshId, Vec<MaterialId>, omsi_geometry::MeshData)>,
    /// Tickets waiting to be placed on the desk.
    pending_tickets: Vec<usize>,
    /// Individual tickets currently on the desk.
    desk_tickets: Vec<DeskTicket>,
    /// Ticket instances waiting to be removed by a mouse click.
    pending_ticket_removals: Vec<usize>,
}

impl TicketBlocks {
    /// Allow photo shutter samples to move the blocks with the bus.
    pub fn instances(&self) -> impl Iterator<Item = usize> + '_ {
        self.blocks.iter().map(|b| b.0).chain(self.desk_tickets.iter().map(|t| t.instance))
    }

    pub fn new(world: &World, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance, pack: &Path) -> TicketBlocks {
        let dir = pack.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut blocks = Vec::new();
        let mut single_meshes = Vec::new();
        for (n, a) in bus.ty.def.attachments.iter().enumerate() {
            let file = dir.join(format!("Ticket_{n}_block.o3d"));
            if !omsi_cfg::vfs::is_file(&file) {
                continue;
            }
            let Ok(m) = omsi_o3d::load_mesh(&file).map_err(|e| log::warn!("{e}")) else { continue };
            let dirs = [dir.clone(), omsi_cfg::resolve_path(&world.root, "Texture")];
            let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
            let mats: Vec<MaterialId> = m
                .materials
                .iter()
                .map(|mat| {
                    let tex = omsi_texture::find_texture(&mat.texture, &dirs_ref).and_then(|p| world.textures.get_gpu_fast(&p)).map(|(img, _)| renderer.add_texture_data(scene, &img));
                    renderer.add_material(scene, tex, AlphaMode::Test, [1.0; 4], false)
                })
                .collect();
            let data = mesh_from_o3d(&m);
            let id = renderer.add_mesh(scene, &data);
            let inst = renderer.add_instance(scene, id, DVec3::ZERO, Mat4::IDENTITY, mats);
            let local = omsi_world::map_index::attachment_matrix(&omsi_scenery::sco::Attachment { ops: a.ops.clone() });
            blocks.push((inst, local, n, data));

            let single_file = [
                dir.join(format!("ticket_{n}_single.o3d")),
                dir.join(format!("Ticket_{n}_single.o3d")),
            ]
            .into_iter()
            .find(|file| omsi_cfg::vfs::is_file(file));

            if let Some(file) = single_file {
                if let Ok(single) = omsi_o3d::load_mesh(&file).map_err(|e| log::warn!("{e}"))
                {
                    let mats: Vec<MaterialId> = single
                        .materials
                        .iter()
                        .map(|mat| {
                            let tex = omsi_texture::find_texture(
                                &mat.texture,
                                &dirs_ref,
                            )
                            .and_then(|p| world.textures.get_gpu_fast(&p))
                            .map(|(img, _)| renderer.add_texture_data(scene, &img));

                            renderer.add_material(scene, tex, AlphaMode::Test, [1.0; 4], false)
                        })
                        .collect();

                    let data = mesh_from_o3d(&single);
                    let mesh = renderer.add_mesh(scene, &data);
                    single_meshes.push((n, mesh, mats, data));
                    log::info!("Loaded individual ticket model: {}", file.display());
                }
            }
        }
        if !blocks.is_empty() {
            log::info!("ticket blocks: {} of {} on the bus", blocks.len(), dir.display());
        }
        log::info!("Ticket blocks: {}, individual ticket models: {}", blocks.len(), single_meshes.len());
        TicketBlocks { made_for: (bus.ty.def.path.clone(), pack.to_path_buf()), blocks, single_meshes, pending_tickets: Vec::new(), desk_tickets: Vec::new(), pending_ticket_removals: Vec::new(), }
    }

    pub fn sync(&self, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance) {
        let rot = bus.body_rotation();
        // Existing ticket blocks
        for (inst, local, _, _) in &self.blocks {
            renderer.set_transform(scene, *inst, bus.position, rot * *local);
        }
        // Individual tickets on the desk
        for ticket in &self.desk_tickets {
            let on = ticket.parent.as_deref().map(|n| mesh_of(bus, n)).unwrap_or(Mat4::IDENTITY);

            renderer.set_transform(
                scene,
                ticket.instance,
                bus.position,
                rot * on * ticket.local,
            );
        }
    }

    /// Take the blocks out of the picture for good: the bus they hung in is gone, or another
    /// set replaces them. Left in the scene they stood where that bus last was - in the air
    /// over the road, with no bus round them (#2340, #2399).
    pub fn remove(&self, renderer: &Renderer, scene: &mut Scene) {
        for (inst, ..) in &self.blocks {
            renderer.remove_instance(scene, *inst);
        }
        for ticket in &self.desk_tickets {
            renderer.remove_instance(scene, ticket.instance);
        }
    }

    /// Where each block is drawn (its ticket type and the middle of its mesh, from the bus's
    /// position in world axes): for tests that aim the mouse at them.
    pub fn centres(&self, bus: &VehicleInstance) -> Vec<(usize, Vec3)> {
        let rot = bus.body_rotation();
        self.blocks
            .iter()
            .filter(|b| !b.3.positions.is_empty())
            .map(|(_, local, n, data)| {
                let c = data.positions.iter().copied().sum::<Vec3>() / data.positions.len() as f32;
                (*n, (rot * *local).transform_point3(c))
            })
            .collect()
    }

    /// The ticket type of the block a ray from `origin` along `dir` hits first.
    pub fn hit(&self, origin: DVec3, dir: Vec3, bus: &VehicleInstance) -> Option<usize> {
        let o = (origin - bus.position).as_vec3();
        let rot = bus.body_rotation();
        self.blocks.iter().filter_map(|(_, local, n, data)| omsi_geometry::ray_mesh(o, dir, data, &(rot * *local)).map(|t| (t, *n))).min_by(|a, b| a.0.total_cmp(&b.0)).map(|x| x.1)
    }

    /// Queue a ticket to be placed on the desk.
    pub fn request_ticket(&mut self, n: usize) {
        self.pending_tickets.push(n);
    }

    /// Take the next pending ticket request.
    pub fn take_ticket_request(&mut self) -> Option<usize> {
        if self.pending_tickets.is_empty() {
            return None;
        }

        Some(self.pending_tickets.remove(0))
    }

    /// Place an individual ticket on the desk.
    /// Existing tickets are kept.
    pub fn place_ticket(&mut self, n: usize, renderer: &Renderer, scene: &mut Scene, point: Vec3, var: [f32; 2], parent: Option<&str>, r: [f32; 3]) -> bool {
        let Some((_, mesh, mats, _)) = self.single_meshes.iter().find(|(ticket, _, _, _)| *ticket == n)
        else {
            log::warn!("No individual ticket model found for ticket type {n}");
            return false;
        };

        // Give each ticket a slightly different height to prevent z-fighting.
        let layer = self.desk_tickets.iter().map(|ticket| ticket.layer).max().map_or(0, |max| max.saturating_add(1));
        let layered_point = point + Vec3::new(0.0, 0.0, layer as f32 * 0.001);
        let local = Money::coin_place(layered_point, var, r);
        let instance = renderer.add_instance(scene, *mesh, DVec3::ZERO, Mat4::IDENTITY, mats.clone());
        self.desk_tickets.push(DeskTicket { instance, ticket_type: n, local, parent: parent.map(str::to_owned), layer });

        true
    }

    /// Remove one ticket of the requested type when the passenger takes it.
    pub fn take_ticket( &mut self, ticket_type: usize, renderer: &Renderer, scene: &mut Scene,
    ) -> bool {
        let Some(index) = self.desk_tickets.iter().position(|ticket| ticket.ticket_type == ticket_type)
        else {
            return false;
        };

        let ticket = self.desk_tickets.remove(index);
        renderer.remove_instance(scene, ticket.instance);

        true
    }

    /// The individual ticket hit by a ray from the mouse.
    pub fn hit_desk_ticket(&self, origin: DVec3, dir: Vec3, bus: &VehicleInstance) -> Option<usize> {
        let o = (origin - bus.position).as_vec3();
        let rot = bus.body_rotation();

        self.desk_tickets
            .iter()
            .filter_map(|ticket| {
                let data = self.single_meshes.iter().find(|(n, _, _, _)| *n == ticket.ticket_type).map(|(_, _, _, data)| data)?;
                let on = ticket.parent.as_deref().map(|n| mesh_of(bus, n)).unwrap_or(Mat4::IDENTITY);
                let transform = rot * on * ticket.local;

                omsi_geometry::ray_mesh(o, dir, data, &transform).map(|distance| (distance, ticket.instance))
            }).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, instance)| instance)
    }

    pub fn request_remove_ticket(&mut self, instance: usize) {
        self.pending_ticket_removals.push(instance);
    }

    pub fn take_ticket_removal_request(&mut self) -> Option<usize> {
        if self.pending_ticket_removals.is_empty() {
            return None;
        }

        Some(self.pending_ticket_removals.remove(0))
    }

    pub fn remove_ticket_instance(&mut self, instance: usize, renderer: &Renderer, scene: &mut Scene) -> bool {
        let Some(index) = self.desk_tickets.iter().position(|ticket| ticket.instance == instance)
        else {
            return false;
        };

        let ticket = self.desk_tickets.remove(index);
        renderer.remove_instance(scene, ticket.instance);

        true
    }
}
