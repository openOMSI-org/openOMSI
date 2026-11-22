//! Per-owner GPU surfaces for LAN freetex images. Never enters the file texture cache.
use super::*;
use omsi_lan::freetex::{Candidate, Frame, State, Worker};
use omsi_render::TextureId;
use std::collections::BTreeMap;

type Slot = (usize, String);

#[derive(Default)]
pub(super) struct Pixels {
    worker: Option<Worker>,
    local: Frame,
    pending: BTreeMap<(u32, u16), omsi_lan::freetex::Received>,
    next_generation: u64,
    capture_at: Option<std::time::Instant>,
    session: Option<(u64, u32)>,
    candidates: Vec<Candidate>,
    lookup_at: Option<std::time::Instant>,
}

#[derive(Default)]
pub(super) struct RemotePixels {
    schema: Option<u32>,
    generation: u64,
    welcome: Option<u32>,
    table: u32,
    slots: Vec<Slot>,
    textures: BTreeMap<u16, TextureId>,
    bytes: BTreeMap<u16, usize>,
}

fn slots(render: &scene::VehicleRender, parts: &[scene::VehicleRender]) -> Vec<Slot> {
    let mut slots: Vec<_> = std::iter::once(render)
        .chain(parts)
        .enumerate()
        .flat_map(|(section, r)| {
            r.variants.iter().flat_map(move |v| {
                v.free
                    .iter()
                    .map(move |f| (section, f.var.to_ascii_lowercase()))
            })
        })
        .collect();
    slots.sort();
    slots.dedup();
    slots.truncate(omsi_lan::freetex::MAX_SLOTS);
    slots
}

#[cfg(test)]
fn table(bus: &str, slots: &[Slot]) -> u32 {
    layout_table(0x811c9dc5, bus, slots)
}

fn layout_table(mut hash: u32, bus: &str, slots: &[Slot]) -> u32 {
    for b in bus
        .replace('\\', "/")
        .to_ascii_lowercase()
        .bytes()
        .chain([0])
        .chain(slots.iter().flat_map(|(section, name)| {
            (*section as u32)
                .to_le_bytes()
                .into_iter()
                .chain(name.bytes())
                .chain([0])
        }))
    {
        hash = (hash ^ b as u32).wrapping_mul(0x01000193);
    }
    hash
}

fn override_slot(v: &mut RemoteView, slot: &Slot, id: Option<TextureId>) {
    let r = if slot.0 == 0 {
        Some(&mut v.render)
    } else {
        v.trailer_renders.get_mut(slot.0 - 1)
    };
    if let Some(r) = r {
        for f in r
            .variants
            .iter_mut()
            .flat_map(|v| &mut v.free)
            .filter(|f| f.var.eq_ignore_ascii_case(&slot.1))
        {
            if f.remote != Some(id) {
                f.remote = Some(id);
                f.remote_key = format!("@network:{id:?}");
            }
        }
    }
}

impl LanView {
    pub(crate) fn stop_freetex(&mut self) {
        self.pixels = Pixels::default();
        for view in self.remotes.values_mut() {
            view.pixels.welcome = None;
        }
    }
    pub(crate) fn capture_freetex(
        &mut self,
        player: Option<&crate::player::Player>,
        root: &std::path::Path,
    ) {
        let Some(p) = player else {
            self.pixels.local = Frame::default();
            return;
        };
        let now = std::time::Instant::now();
        if self
            .pixels
            .capture_at
            .is_some_and(|t| now.duration_since(t).as_millis() < 100)
        {
            return;
        }
        self.pixels.capture_at = Some(now);
        let slots = slots(&p.render, &p.trailer_renders);
        let bus = omsi_lan::game::content_relative(&p.vehicle.ty.def.path, root);
        let images: Vec<_> = slots
            .iter()
            .map(|(section, name)| {
                let r = if *section == 0 {
                    &p.render
                } else {
                    &p.trailer_renders[*section - 1]
                };
                r.variants
                    .iter()
                    .flat_map(|v| &v.free)
                    .filter(|f| f.var.eq_ignore_ascii_case(name))
                    .find_map(|f| f.image.and_then(|id| f.reloads.snapshot(id)))
            })
            .collect();
        let paths = slots
            .iter()
            .map(|(section, name)| {
                let r = if *section == 0 {
                    &p.render
                } else {
                    &p.trailer_renders[*section - 1]
                };
                r.variants
                    .iter()
                    .flat_map(|v| &v.free)
                    .filter(|f| f.var.eq_ignore_ascii_case(name))
                    .find_map(|f| f.image.and_then(|id| f.reloads.source_path(id)))
            })
            .collect();
        let schema =
            omsi_lan::script_texture::script_texture_table(&bus, &p.vehicle.ty.program, &[]);
        self.pixels.local = Frame {
            table: layout_table(schema, "", &slots),
            images,
            paths,
        };
    }

    pub(crate) fn sync_freetex(
        &mut self,
        lan: &mut omsi_net::LanSession,
        game: &LanGame,
        r: &Renderer,
        scene: &mut Scene,
        root: &std::path::Path,
    ) {
        let session = (lan.session, lan.my_id);
        if self.pixels.session != Some(session) {
            self.pixels.session = Some(session);
            self.pixels.worker = None;
            self.pixels.pending.clear();
            for view in self.remotes.values_mut() {
                view.pixels.welcome = None;
            }
        }
        if self.pixels.worker.is_none() {
            match Worker::new() {
                Ok(worker) => self.pixels.worker = Some(worker),
                Err(e) => {
                    log::warn!("LAN freetex worker: {e}");
                    return;
                }
            }
        }
        let mut subscriptions = BTreeMap::new();
        let now = std::time::Instant::now();
        let lookup = self
            .pixels
            .lookup_at
            .is_none_or(|t| now.duration_since(t).as_millis() >= 100);
        if lookup {
            self.pixels.candidates.clear();
            self.pixels.lookup_at = Some(now);
        }
        for (&id, v) in &mut self.remotes {
            let Some(rv) = game
                .remotes
                .get(&id)
                .filter(|rv| !rv.stand_in && !rv.fallback)
            else {
                continue;
            };
            let Some(schema) = rv.freetex_subscription_table() else {
                if v.pixels.welcome.take().is_some() {
                    for slot in v.pixels.slots.clone() {
                        override_slot(v, &slot, None);
                    }
                    v.pixels.generation = 0;
                }
                continue;
            };
            if v.pixels.welcome != Some(lan.welcomes) || v.pixels.schema != Some(schema) {
                self.pixels.next_generation = self.pixels.next_generation.wrapping_add(1);
                v.pixels.generation = self.pixels.next_generation;
                v.pixels.welcome = Some(lan.welcomes);
                v.pixels.slots = slots(&v.render, &v.trailer_renders);
                v.pixels.table = layout_table(schema, "", &v.pixels.slots);
                v.pixels.schema = Some(schema);
                for slot in v.pixels.slots.clone() {
                    override_slot(v, &slot, None);
                }
            }
            if !v.pixels.slots.is_empty() {
                subscriptions.insert(id, (v.pixels.table, v.pixels.generation));
            }
            if lookup {
                for (section, name) in &v.pixels.slots {
                    let r = if *section == 0 {
                        &v.render
                    } else {
                        &v.trailer_renders[*section - 1]
                    };
                    if let Some(f) = r
                        .variants
                        .iter()
                        .flat_map(|v| &v.free)
                        .find(|f| f.var.eq_ignore_ascii_case(name))
                    {
                        let candidate = Candidate {
                            name: rv.vehicle().str_var(name).to_owned(),
                            dirs: f.dirs.clone(),
                            root: root.to_path_buf(),
                        };
                        if self.pixels.candidates.len() < omsi_lan::freetex::MAX_SLOTS
                            && !self.pixels.candidates.contains(&candidate)
                        {
                            self.pixels.candidates.push(candidate);
                        }
                    }
                }
            }
        }
        let output = self.pixels.worker.as_ref().unwrap().pump(
            State {
                id: lan.my_id,
                local: self.pixels.local.clone(),
                subscriptions,
                peers: lan.peers().map(|p| p.pose.id).collect(),
                candidates: self.pixels.candidates.clone(),
            },
            lan.take_freetex_packets(),
        );
        for p in output.packets {
            lan.send_freetex_packet(&p);
        }
        for image in output.images {
            self.pixels
                .pending
                .insert((image.incoming.owner, image.incoming.index), image);
        }
        self.pixels.pending.retain(|(owner, _), i| {
            self.remotes
                .get(owner)
                .is_some_and(|v| v.pixels.generation == i.generation)
        });
        // Upload at most one complete image per frame. Incomplete transfers stay invisible.
        let Some((_, received)) = self.pixels.pending.pop_first() else {
            return;
        };
        let incoming = received.incoming;
        let retained_bytes: usize = self
            .remotes
            .values()
            .flat_map(|v| v.pixels.bytes.values())
            .sum();
        let Some(v) = self.remotes.get_mut(&incoming.owner) else {
            return;
        };
        if v.pixels.generation != received.generation || v.pixels.table != incoming.table {
            return;
        }
        let Some(slot) = v.pixels.slots.get(incoming.index as usize).cloned() else {
            return;
        };
        let Some(data) = received.data else {
            override_slot(v, &slot, None);
            return;
        };
        if data.width > r.device.limits().max_texture_dimension_2d
            || data.height > r.device.limits().max_texture_dimension_2d
        {
            return;
        }
        let bytes = data.gpu_bytes() as usize;
        let old_bytes = v.pixels.bytes.get(&incoming.index).copied().unwrap_or(0);
        if retained_bytes - old_bytes + bytes > 64 * 1024 * 1024 {
            return;
        }
        let id = if let Some(&id) = v.pixels.textures.get(&incoming.index) {
            if r.refresh_texture(scene, id, &data) {
                r.rebind_textures(scene, &[id]);
            }
            id
        } else {
            let id = r.add_texture_data(scene, &data);
            v.pixels.textures.insert(incoming.index, id);
            id
        };
        v.pixels.bytes.insert(incoming.index, bytes);
        override_slot(v, &slot, Some(id));
    }
}

pub(super) fn prepare(v: &mut RemoteView) {
    for r in std::iter::once(&mut v.render).chain(&mut v.trailer_renders) {
        for f in r.variants.iter_mut().flat_map(|v| &mut v.free) {
            f.remote = Some(None);
            f.remote_key = "@network:None".into();
        }
    }
}

pub(super) fn release(r: &Renderer, scene: &mut Scene, pixels: RemotePixels) {
    for id in pixels.textures.into_values() {
        r.free_texture(scene, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_ignores_local_paths_but_distinguishes_sections_and_variables() {
        let slots = vec![(0, "display".into()), (1, "display".into())];
        assert_eq!(
            table("Vehicles/Bus/main.bus", &slots),
            table("vehicles\\bus\\MAIN.bus", &slots)
        );
        assert_ne!(table("bus.bus", &slots), table("bus.bus", &slots[..1]));
        assert_ne!(
            table("bus.bus", &slots),
            table("bus.bus", &[(0, "other".into())])
        );
    }
}
