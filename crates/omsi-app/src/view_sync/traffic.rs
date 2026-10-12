//! What the traffic shows: the GPU side of the AI vehicles, their drivers and the
//! traffic lamps.
//!
//! The renders live in `TrafficView` (`SimView::traffic`, apart from the `Traffic`). The
//! traffic's own calls that put a car on the road or take one off (population, timetable
//! departures, trains, the LAN mirror, its step) do not touch them: they write down
//! `TrafficEvent`s (`Traffic::events`), which [`apply_traffic`] replays in their order -
//! right after the call that made them, so that the renderer sees the same calls in the
//! same order as when those calls made the renders themselves. Everything else - parked
//! cars, lamps, drivers, the cars' transforms, materials and script textures - is brought
//! up to date by the view sync (`view_sync::sync`, `Traffic::sync`).

use crate::scene::{VehicleRender, World};
use crate::traffic::Traffic;
use hashbrown::HashMap;
use omsi_sim::traffic::TrafficLightController;
use omsi_sim::view_events::{CarLooks, FrameSnapshot, LookLimits, PartFrame, TrafficEvent};
use omsi_sim::VehicleType;
use std::sync::Arc;
use omsi_render::{Renderer, Scene};
use omsi_texture::Image;

pub(super) const SCRIPT_UPLOAD_BUDGET: usize = 4 << 20;

/// Within this distance (m) of the camera a timetable bus has its driver at the wheel.
pub(super) const DRIVER_NEAR: f64 = 70.0;

/// How near an articulated AI bus has to be for its bellows to be reshaped as its joint
/// turns (m): the fold of the bend is a few centimetres, which is a screen pixel or more
/// within this range - farther out it is not worth a mesh update every frame it steers.
pub(super) const SKIN_DISTANCE: f64 = 200.0;

/// What an AI car looks like on the screen: the render of its body and those of its
/// coupled parts (trailers, rear sections, the cars of a train), in their order.
pub(crate) struct CarRender {
    pub(crate) body: VehicleRender,
    pub(crate) trailers: Vec<VehicleRender>,
    /// The car's string variables as the snapshots sent them (`CarLooks::str_changes`).
    pub(crate) strs: Vec<String>,
}

/// The GPU side of the traffic, kept apart from the simulation: the cars' renders by car
/// id, and the drivers of the timetable buses.
#[derive(Default)]
pub(crate) struct TrafficView {
    cars: HashMap<u64, CarRender>,
    /// Renders of cars that have gone, given back at the next `sync`.
    released: Vec<VehicleRender>,
    /// Renders put aside while their car is made anew under its id (`TrafficEvent::Detached`).
    detached: Vec<(u64, CarRender)>,
    /// The drivers at the wheel of the timetable buses near the camera, by car id (see
    /// `driver.rs`; made within `DRIVER_NEAR` m of the camera, let go beyond twice that).
    drivers: HashMap<u64, crate::driver::DriverFigure>,
    /// Figures let go by their bus, hidden, for the next one (their GPU meshes stay).
    driver_pool: Vec<crate::driver::DriverFigure>,
}

impl TrafficView {
    /// The renders of the car `id` that has just been put on the road.
    pub(crate) fn insert(&mut self, id: u64, render: CarRender) {
        if let Some(old) = self.cars.insert(id, render) {
            log::warn!("traffic: two cars with id {id}; the renders of the first are let go");
            self.released.push(old.body);
            self.released.extend(old.trailers);
        }
    }

    /// Car `id` has gone: its renders go back to the world at the next sync.
    pub(crate) fn retire(&mut self, id: u64) {
        if let Some(r) = self.cars.remove(&id) {
            self.released.push(r.body);
            self.released.extend(r.trailers);
        }
    }

    /// Car `id` has gone: its renders go back to the world now.
    pub(crate) fn release_car(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene, id: u64) {
        if let Some(r) = self.cars.remove(&id) {
            release_car_render(world, renderer, scene, r);
        }
    }

    /// Couple another part of type `t` to car `id`'s picture.
    pub(crate) fn add_trailer(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene, id: u64, t: &Arc<VehicleType>) {
        if let Some(r) = self.cars.get_mut(&id) {
            r.trailers
                .push(world.add_vehicle_shared(renderer, scene, t, None, Some(&r.body)));
        }
    }

    /// Let the renders of car `id`'s coupled parts go now (it gets others).
    pub(crate) fn release_trailers(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene, id: u64) {
        if let Some(r) = self.cars.get_mut(&id) {
            for t in r.trailers.drain(..) {
                world.release_vehicle(renderer, scene, t);
            }
        }
    }

    /// The renderer instances of the cars, by car id in its order, each car's body first
    /// (`OMSI_SCENE_DIGEST`).
    pub(crate) fn instances(&self) -> Vec<(u64, usize)> {
        let mut ids: Vec<u64> = self.cars.keys().copied().collect();
        ids.sort_unstable();
        let mut out = Vec::new();
        for id in ids {
            let r = &self.cars[&id];
            for part in std::iter::once(&r.body).chain(&r.trailers) {
                out.extend(part.instances.iter().map(|i| (id, *i)));
            }
        }
        out
    }

    /// Follow one thing the traffic did (see [`TrafficEvent`]).
    pub(crate) fn apply(&mut self, event: TrafficEvent, world: &World, renderer: &Renderer, scene: &mut Scene) {
        match event {
            TrafficEvent::Spawned { id, ty, scheme, trailers } => {
                let body = world.add_vehicle_shared(renderer, scene, &ty, scheme, None);
                let trailers = trailers
                    .iter()
                    .map(|(t, scheme)| world.add_vehicle_shared(renderer, scene, t, *scheme, Some(&body)))
                    .collect();
                self.insert(id, CarRender { body, trailers, strs: Vec::new() });
            }
            TrafficEvent::Released { id } => self.release_car(world, renderer, scene, id),
            TrafficEvent::Retired { id } => self.retire(id),
            TrafficEvent::Detached { id } => {
                if let Some(r) = self.cars.remove(&id) {
                    self.detached.push((id, r));
                }
            }
            TrafficEvent::DetachedReleased { id } => {
                if let Some(k) = self.detached.iter().position(|(d, _)| *d == id) {
                    let (_, r) = self.detached.swap_remove(k);
                    release_car_render(world, renderer, scene, r);
                }
            }
            TrafficEvent::TrailerAdded { id, ty } => self.add_trailer(world, renderer, scene, id, &ty),
            TrafficEvent::TrailersReleased { id } => self.release_trailers(world, renderer, scene, id),
            TrafficEvent::ParkedDeparted { instances } => crate::scene::show_parked_instances(renderer, scene, &instances, false),
            TrafficEvent::ParkedReturned { instances } => crate::scene::show_parked_instances(renderer, scene, &instances, true),
        }
    }
}

/// Bring the renderer up to what the traffic's calls did since the last time: replay
/// `Traffic::events` in their order (see [`TrafficEvent`]). Called right after each call
/// that can put cars on the road or take them off, where those calls used to make the
/// renderer calls themselves.
pub(crate) fn apply_traffic(t: &mut Traffic, view: &mut TrafficView, world: &World, renderer: &Renderer, scene: &mut Scene) {
    if t.events.is_empty() {
        return;
    }
    for e in std::mem::take(&mut t.events) {
        view.apply(e, world, renderer, scene);
    }
}

/// Give the renders of a car back to the world, its body first.
pub(crate) fn release_car_render(world: &World, renderer: &Renderer, scene: &mut Scene, r: CarRender) {
    for r in std::iter::once(r.body).chain(r.trailers) {
        world.release_vehicle(renderer, scene, r);
    }
}

impl Traffic {
    pub fn precache_random(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene) {
        let t0 = std::time::Instant::now();
        let sets = self.random_sets();
        for chunk in sets.chunks(3) {
            world.prefetch_vehicle_sets(renderer, chunk);
            for (ty, scheme) in chunk {
                world.precache_vehicle(renderer, scene, ty, *scheme);
            }
        }
        world.forget_prefetched();
        for (ty, _) in &sets {
            self.prime_pull_out_room(ty, false);
        }
        log::info!("traffic: {} vehicle/paint sets of the random traffic read and uploaded in {:.1} s", sets.len(), t0.elapsed().as_secs_f32());
    }

    /// The drivers of the timetable buses near the camera: made when a bus comes within
    /// `DRIVER_NEAR`, posed every sync, let go when it is twice that far or gone. They read
    /// their buses from the snapshot (`TrafficSim::snapshot_drivers`).
    pub(super) fn sync_drivers(&mut self, view: &mut TrafficView, world: &World, renderer: &Renderer, scene: &mut Scene) {
        self.sim.snapshot_drivers(&mut self.snapshot, DRIVER_NEAR * 2.0);
        let snap = &self.snapshot;
        let Some(eye) = snap.drivers_eye() else { return };
        let dt = snap.drivers_dt().max(1.0 / 120.0);
        let mut keep: Vec<u64> = Vec::new();
        for bus in snap.driver_buses() {
            let car = crate::driver::SnapshotBus { snap, bus };
            let d = (bus.position - eye).length();
            keep.push(bus.id);
            if !view.drivers.contains_key(&bus.id) {
                if d > DRIVER_NEAR {
                    continue;
                }
                let figure = match view.driver_pool.pop() {
                    Some(mut f) => {
                        if f.attach(&car) {
                            Some(f)
                        } else {
                            view.driver_pool.push(f);
                            None
                        }
                    }
                    None => crate::driver::DriverFigure::new(world, renderer, scene, &car, bus.id),
                };
                match figure {
                    Some(f) => {
                        view.drivers.insert(bus.id, f);
                    }
                    None => continue,
                }
            }
            if let (Some(f), Some(r)) = (view.drivers.get_mut(&bus.id), view.cars.get(&bus.id)) {
                f.update(renderer, scene, &car, &r.body, dt, true, false);
            }
        }
        let gone: Vec<u64> = view.drivers.keys().copied().filter(|id| !keep.contains(id)).collect();
        for id in gone {
            if let Some(mut f) = view.drivers.remove(&id) {
                f.hide(renderer, scene);
                view.driver_pool.push(f);
            }
        }
    }

    pub(super) fn sync(&mut self, view: &mut TrafficView, world: &World, scenery: &mut omsi_sim::scenery::SceneryState, renderer: &Renderer, scene: &mut Scene) {
        // cars that have parked (the simulation takes them off): the parked object stands
        // in their place from now on
        self.settle_parked(world);
        apply_traffic(self, view, world, renderer, scene);
        for r in std::mem::take(&mut view.released) {
            world.release_vehicle(renderer, scene, r);
        }
        // (drawn between the traffic's last two steps, where this frame falls: see
        // `TrafficSim::advance`; back where they stand once drawn)
        self.sim.show_between();
        // where the cars and their meshes are drawn, as data (`FrameSnapshot`)
        self.sim.snapshot_into(&mut self.snapshot);
        self.sync_drivers(view, world, renderer, scene);
        self.sync_lamps(world, scenery, renderer, scene);
        self.sync_cars(view, world, renderer, scene);
        self.sim.show_actual();
    }

    pub(crate) fn follow_drawn(&self, view: &TrafficView, renderer: &Renderer, scene: &mut Scene) {
        let snap = &self.snapshot;
        if snap.cars.is_empty() {
            return;
        }
        let mut index: Option<HashMap<u64, usize>> = None;
        for (k, frame) in snap.cars.iter().enumerate() {
            let Some(looks) = snap.looks(frame).filter(|_| frame.shown) else { continue };
            let Some(r) = view.cars.get(&frame.id) else { continue };
            let i = match self.sim.cars.get(k) {
                Some(c) if c.id == frame.id => k,
                _ => match index.get_or_insert_with(|| self.sim.cars.iter().enumerate().map(|(i, c)| (c.id, i)).collect()).get(&frame.id) {
                    Some(&i) => i,
                    None => continue,
                },
            };
            let v = &self.sim.cars[i].vehicle;
            let parts = snap.parts(frame);
            if r.body.hidden || parts.len() != 1 + v.trailers.len() {
                continue;
            }
            let now = std::iter::once((v.position, v.body_rotation())).chain(v.trailers.iter().map(|t| (t.position, t.body_rotation())));
            for (j, ((part, (position, rot)), render)) in parts.iter().zip(now).zip(std::iter::once(&r.body).chain(&r.trailers)).enumerate() {
                let turn = rot * part.rotation.inverse();
                let meshes = snap.meshes(part);
                for (inst, m) in render.instances.iter().zip(meshes) {
                    renderer.set_transform(scene, *inst, position, turn * m.local);
                }
                let lamps = snap.lamps(looks, j);
                crate::scene::sync_interior_lamps(renderer, scene, &part.ty, position, rot, |li| lamps.get(li).copied().unwrap_or(false), render);
                if j == 0 {
                    let local = |m: usize| meshes.get(m).map_or(glam::Mat4::IDENTITY, |f| turn * f.local);
                    crate::view_sync::vehicle_damage::place_cracks(renderer, scene, render, position, local);
                }
            }
            if let Some(d) = view.drivers.get(&frame.id) {
                d.follow(renderer, scene, v.position, v.body_rotation());
            }
        }
    }

    /// The traffic light lamps (see `sync`).
    fn sync_lamps(&mut self, world: &World, scenery: &mut omsi_sim::scenery::SceneryState, renderer: &Renderer, scene: &mut Scene) {
        // (the lamps' scripts are the simulation's: it takes the lamps placed since first)
        world.scenery_sync(scenery);
        let lamps = &mut scenery.lamps;
        // traffic light lamps: the lamp's script (or the stock rules) turns the state of its
        // light into the `[visible] red|yellow|green 1` meshes and the coronas, and moves
        // what it animates (a barrier arm); it runs on the time since the last sync (an
        // offscreen run syncs only for its pictures)
        let frame_dt = std::mem::take(&mut self.sim.lamp_dt);
        let near = self.sim.viewer.map(|v| v.pos);
        let debug_lamps = omsi_cfg::flags::OMSI_DEBUG_LAMPS.is_set();
        // Far lamps take turns: each one every `LAMP_TURNS`th frame, its script run on the
        // time it was owed. All of them in 1.2 km, every frame, one after the other on the
        // frame's thread, were a good part of the traffic's frame; a lamp 300 m off that
        // changes a frame or three late shows nobody anything (a moving barrier always runs).
        self.lamp_turn = self.lamp_turn.wrapping_add(1);
        if self.lamp_owed.len() > 8192 {
            self.lamp_owed.clear();
        }
        for lamp in world.light_objects.lock().iter_mut() {
            if debug_lamps && lamp.animated {
                log::info!("moving lamp at ({:.1}, {:.1}), {:.0} m from the viewer", lamp.pos.x, lamp.pos.y, near.map(|p| (lamp.pos - p).length()).unwrap_or(0.0));
            }
            let dist = near.map(|p| (lamp.pos - p).length()).unwrap_or(0.0);
            if dist > 1200.0 {
                continue;
            }
            let key = lamp.script.map(|k| k as usize).unwrap_or(lamp.parent as usize ^ lamp.index);
            if dist > LAMP_NEAR && !lamp.animated && !lamp_turn_now(key, self.lamp_turn) {
                if lamp.script.is_some() {
                    *self.lamp_owed.entry(key).or_default() += frame_dt;
                }
                continue;
            }
            let dt = frame_dt + self.lamp_owed.remove(&key).unwrap_or(0.0);
            let (state, request) = match self
                .controller_of_object
                .get(&lamp.parent)
                .and_then(|&c| self.sim.lights.get(c))
            {
                Some(ctl) if lamp.any_light && ctl.lights.len() > 1 => {
                    // the most open of the crossing's lights (see `LightObject::any_light`)
                    let open = |s: i32| match s {
                        6..=8 => 3,
                        3..=5 => 2,
                        9..=11 => 1,
                        0..=2 => 0,
                        _ => -1,
                    };
                    let li = (0..ctl.lights.len())
                        .max_by_key(|&i| open(ctl.state(i)))
                        .unwrap_or(0);
                    (ctl.state(li), ctl.request.iter().any(|r| *r))
                }
                Some(ctl) => (
                    ctl.state(lamp.index),
                    ctl.request.get(lamp.index).copied().unwrap_or(false),
                ),
                // a lamp that names no crossing, or one whose crossing has no program,
                // reads the engine's dummy (see `UNLINKED_PHASE`): red, as in OMSI
                None => (omsi_sim::traffic::UNLINKED_PHASE, false),
            };
            let (r, y, g) = TrafficLightController::lamps(state);
            if let Some(s) = lamp.script.and_then(|k| lamps.get_mut(k)) {
                let vars = omsi_sim::scenery::SceneryVars {
                    nightlight: self.sim.night as i32 as f32,
                    in_use: 1.0,
                    traffic_light_phase: state as f32,
                    traffic_light_approach: request as i32 as f32,
                    switch: None,
                };
                s.update(dt, &vars);
                // `OMSI_DEBUG_LAMPS`: where the moving lamps are (barriers) and how far their
                // meshes are turned, each time the lamps are updated
                if lamp.animated && debug_lamps {
                    let turn = s
                        .mesh_transforms
                        .iter()
                        .map(|m| {
                            let (_, r, _) = m.to_scale_rotation_translation();
                            r.to_axis_angle().1.to_degrees()
                        })
                        .fold(0.0f32, f32::max);
                    log::info!("lamp at ({:.1}, {:.1}): light state {:?} (crossing {:?}, light {}{}), meshes turned up to {turn:.0} deg", lamp.pos.x, lamp.pos.y, vars.traffic_light_phase, self.sim.controller_of_object.get(&lamp.parent), lamp.index, if lamp.any_light { ", any" } else { "" });
                }
                if lamp.animated {
                    for (i, (inst, _)) in lamp.instances.iter().enumerate() {
                        if let Some(m) = s.mesh_transforms.get(i) {
                            renderer.set_transform(scene, *inst, lamp.pos, lamp.xf * *m);
                        }
                    }
                    // the lights go with their meshes (a barrier's lamps rise with its arm)
                    for (c, (mi, local, dir)) in lamp.coronas.iter_mut().zip(&lamp.corona_mesh) {
                        if let Some(m) = s.mesh_transforms.get(*mi) {
                            let xf = lamp.xf * *m;
                            c.0.position = lamp.pos + xf.transform_point3(*local).as_dvec3();
                            if *dir != glam::Vec3::ZERO {
                                c.0.direction = xf.transform_vector3(*dir).normalize_or_zero();
                            }
                        }
                    }
                }
            }
            if !lamp.texts.is_empty() {
                if let Some(s) = lamp.script.and_then(|k| lamps.get_mut(k)) {
                    let _ = s.take_refresh_strings();
                    for (tex, st) in lamp.texts.iter_mut() {
                        let text = s.str_var(st.def.variable.trim()).to_string();
                        if st.update(&text) {
                            if let Some(rgba) = st.pending.take() {
                                let (w, h) = (st.def.width.max(1) as u32, st.def.height.max(1) as u32);
                                renderer.update_texture_mips(
                                    scene,
                                    *tex,
                                    &omsi_texture::Image { width: w, height: h, rgba, has_alpha: true },
                                );
                            }
                        }
                    }
                }
            }
            let raw_value = |lamp: &crate::scene::LightObject, var: &str| -> f32 {
                // Custom signals can shift phases or blink the standard channels (Numazu
                // pedestrian lamps). Use their script outputs whenever they are available.
                let scripted = lamp.script.and_then(|k| lamps.get(k)).and_then(|s| {
                    // A failed/missing script can still have a varlist of zeroes. Keep
                    // stock fallback behaviour if it has no runnable frame block.
                    if s.program.frame.is_empty() { None } else { s.var(var) }
                });
                crate::scene::traffic_lamp_value(
                    var,
                    scripted,
                    crate::scene::standard_traffic_lamp(var, r, y, g, request),
                )
            };
            // A scripted lamp can leave both standard channels on at once. A road signal
            // must never show green together with red; keep the stop aspect in that case.
            let red_on = raw_value(lamp, "red") >= 0.5;
            let value = |lamp: &crate::scene::LightObject, var: &str| -> f32 {
                let green = matches!(var.trim().to_ascii_lowercase().as_str(), "green" | "gruen" | "grün");
                let v = raw_value(lamp, var);
                if green && red_on { 0.0 } else { v }
            };
            if !lamp.animated {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (state, request).hash(&mut h);
                if let Some(s) = lamp.script.and_then(|k| lamps.get(k)) {
                    for v in &s.state.vars {
                        v.to_bits().hash(&mut h);
                    }
                }
                let sig = h.finish();
                if lamp.shown == Some(sig) {
                    continue;
                }
                lamp.shown = Some(sig);
            }
            // Traffic lamps do not enter World's ordinary scripted-object update path.
            // Switch their materials here too, so [matl_item] nightmaps light the LEDs.
            for (inst, slot, base, item, var) in &lamp.variants {
                renderer.set_material(
                    scene,
                    *inst,
                    *slot,
                    if crate::scene::change_picks_item(value(lamp, var)) { *item } else { *base },
                );
            }
            for k in 0..lamp.coronas.len() {
                let v = value(lamp, &lamp.coronas[k].1);
                lamp.lit[k] = v;
            }
            for (k, (inst, cond)) in lamp.instances.iter().enumerate() {
                let visible = match cond {
                    Some((var, want)) => (value(lamp, var) - want).abs() < 0.5,
                    None => true,
                };
                // lenses switched by their material instead (`[alphascale]` and
                // `[matl_lightmap]` on the lamp's variables, #826)
                match lamp.slots.get(k).filter(|s| !s.is_empty()) {
                    Some(slots) => {
                        let known = |v: &str| -> Option<f32> {
                            let scripted = lamp.script.and_then(|k| lamps.get(k)).and_then(|s| {
                                if s.program.frame.is_empty() { None } else { s.var(v) }
                            });
                            let raw = scripted
                                .or_else(|| v.trim().parse::<f32>().ok())
                                .or_else(|| crate::scene::standard_traffic_lamp(v, r, y, g, request));
                            let green = matches!(v.trim().to_ascii_lowercase().as_str(), "green" | "gruen" | "grün");
                            raw.map(|value| if green && red_on { 0.0 } else { value })
                        };
                        let (alpha, light) = slots.values(&known);
                        renderer.set_params(scene, *inst, &alpha, visible, &[]);
                        renderer.set_slot_light(scene, *inst, &light);
                    }
                    None => renderer.set_params(scene, *inst, &[], visible, &[]),
                }
            }
        }
    }

    /// The AI vehicles' pictures: where they stand, their materials, their script
    /// textures (see `sync`). What the simulation does for them (their pages and text
    /// textures run, the uploads, cracks and dents taken, the bellows reshaped) is taken into
    /// the snapshot first (`TrafficSim::snapshot_looks`); the renderer is then given the
    /// same calls in the same order as when it was done car by car here.
    fn sync_cars(&mut self, view: &mut TrafficView, world: &World, renderer: &Renderer, scene: &mut Scene) {
        // `Envir_Brightness`, which Omsi.exe sets for every road vehicle as for the
        // player's: the stock buses fade their windows by it at night (left at the engine's
        // default of 1, an AI bus under the street lamps kept its daytime brown glass)
        if let Some(d) = self.sim.daylight {
            for c in self.sim.cars.iter_mut().filter(|c| c.vehicle.ai_visuals) {
                let b = d.envir_brightness(world.light_map_light_at(c.vehicle.position));
                c.vehicle.set_var("Envir_Brightness", b);
            }
        }
        // A far car's script textures (its destination sign) stay as they are drawn: OMSI
        // shows them at any distance its model level has them. (They were stood in for by
        // their mean colour beyond 50 m, and every timetable bus coming up the street had
        // a blank sign until it was almost there.) What a far car's scripts redraw goes to
        // the GPU at most every half second, a slice of the cars per frame.
        let limits = LookLimits { displays_far: crate::scene::DISPLAYS_FAR, skin_distance: SKIN_DISTANCE, upload_budget: SCRIPT_UPLOAD_BUDGET };
        self.sim.snapshot_looks(&mut self.snapshot, &mut self.looks, &limits);
        // one buffer for the per-mesh alpha of every car: a fresh copy for each mesh of each
        // car was thousands of small allocations a frame in a busy street
        let mut alpha: Vec<f32> = Vec::new();
        let snap = &self.snapshot;
        for frame in &snap.cars {
            let Some(r) = view.cars.get_mut(&frame.id) else { continue };
            // out of sight (`tick` decided): hidden once, then left alone until it comes
            // into view again - its many per-mesh updates were a third of this stage
            let Some(looks) = snap.looks(frame).filter(|_| frame.shown) else {
                if !r.body.hidden {
                    r.body.hidden = true;
                    for inst in r.body.instances.iter().chain(r.trailers.iter().flat_map(|r| r.instances.iter())) {
                        renderer.set_params(scene, *inst, &[], false, &[]);
                    }
                }
                continue;
            };
            r.body.hidden = false;
            show_looks(renderer, scene, snap, snap.parts(frame), looks, r);
            // where the body and its coupled parts stand and how their meshes are drawn: from
            // the snapshot (the body first, then the parts, as their renders go)
            for (part, r) in snap.parts(frame).iter().zip(std::iter::once(&r.body).chain(r.trailers.iter())) {
                pose_part(renderer, scene, snap, part, r, &mut alpha);
            }
        }
    }
}

/// Bring car `r`'s renders to its looks in the snapshot (`parts`: its body, then its coupled
/// parts): interior lamps, pictures, switched materials, cracks and dents, bellows - in the
/// order the view sync gave the renderer these calls when it read the car itself.
fn show_looks(renderer: &Renderer, scene: &mut Scene, snap: &FrameSnapshot, parts: &[PartFrame], looks: &CarLooks, r: &mut CarRender) {
    let Some(body) = parts.first() else { return };
    for (i, s) in &looks.str_changes {
        let i = *i as usize;
        if r.strs.len() <= i {
            r.strs.resize(i + 1, String::new());
        }
        r.strs[i].clone_from(s);
    }
    let vars = crate::scene::CarVars { ty: &looks.ty, index: &looks.var_index, vars: snap.vars(looks), strs: &r.strs };
    let text = |renderer: &Renderer, scene: &mut Scene, render: &VehicleRender, part: u32| {
        for (_, i, width, height, rgba) in looks.text_uploads.iter().filter(|u| u.0 == part) {
            if let Some(Some(tex)) = render.text_textures.get(*i) {
                renderer.update_texture_mips(scene, *tex, &Image { width: *width, height: *height, rgba: rgba.clone(), has_alpha: true });
            }
        }
    };
    // the body: its interior lamps, its text and script textures, its materials, its damage
    let render = &mut r.body;
    let lamps = snap.lamps(looks, 0);
    crate::scene::sync_interior_lamps(renderer, scene, &looks.ty, body.position, body.rotation, |li| lamps.get(li).copied().unwrap_or(false), render);
    text(renderer, scene, render, 0);
    let mut rebound = Vec::new();
    for u in &looks.script_uploads {
        let Some(Some(tex)) = render.script_textures.get(u.index) else { continue };
        let img = Image { width: u.width, height: u.height, rgba: u.rgba.clone(), has_alpha: true };
        if u.mipmaps {
            if renderer.update_texture_mips(scene, *tex, &img) {
                rebound.push(*tex);
            }
        } else {
            renderer.update_texture(scene, *tex, &img);
        }
    }
    renderer.rebind_textures(scene, &rebound);
    crate::scene::sync_materials(renderer, scene, &vars, render);
    let meshes = snap.meshes(body);
    let local = |m: usize| meshes.get(m).map_or(glam::Mat4::IDENTITY, |f| f.local);
    crate::view_sync::vehicle_damage::place_cracks(renderer, scene, render, body.position, local);
    for (mesh, impact, energy, local) in &looks.cracks {
        crate::view_sync::vehicle_damage::add_crack(renderer, scene, render, &looks.ty, body.position, *mesh, *impact, *energy, *local);
    }
    for dent in &looks.dents {
        crate::view_sync::vehicle_damage::add_dent(renderer, scene, render, &looks.ty, dent);
    }
    // a coupled part runs no scripts of its own: its plates, its displays and its
    // switched materials follow the leading vehicle's, as the player's own rear
    // sections do (without this an AI bus's rear section kept the blank textures and
    // the unswitched materials it was built with)
    for (k, (part, render)) in parts.iter().skip(1).zip(r.trailers.iter_mut()).enumerate() {
        let lamps = snap.lamps(looks, k + 1);
        crate::scene::sync_interior_lamps(renderer, scene, &part.ty, part.position, part.rotation, |li| lamps.get(li).copied().unwrap_or(false), render);
        crate::scene::sync_materials(renderer, scene, &vars, render);
        text(renderer, scene, render, k as u32 + 1);
    }
    // an articulated AI bus (timetable or random traffic) bends its bellows like the
    // player's while it is near enough for the fold to show (`SKIN_DISTANCE`; farther out its
    // shape just stays as it was, which nobody can tell from still following the road)
    for (part, mesh, pos, nrm) in &looks.skins {
        let render = if *part == 0 { Some(&r.body) } else { r.trailers.get(*part as usize - 1) };
        let (Some(render), Some(ty)) = (render, parts.get(*part as usize).map(|p| &p.ty)) else { continue };
        if let Some((_, id, _)) = render.skinned.iter().find(|(i, _, _)| i == mesh) {
            renderer.update_mesh(scene, *id, pos, nrm, &ty.meshes[*mesh].data.uvs);
        }
    }
}

/// Bring one vehicle part's instances (`r`) to `part` of the snapshot: their transforms and
/// per-slot values. `alpha`: a buffer for the slots' alpha, kept by the caller.
fn pose_part(renderer: &Renderer, scene: &mut Scene, snap: &FrameSnapshot, part: &PartFrame, r: &VehicleRender, alpha: &mut Vec<f32>) {
    let meshes = snap.meshes(part);
    for (inst, m) in r.instances.iter().zip(meshes) {
        renderer.set_transform(scene, *inst, part.position, m.local);
        // AI vehicles do not run every cockpit/material script that the player
        // vehicle runs.  Some models consequently leave an `[alphascale]`
        // variable at zero; applying it to an opaque body makes the traffic bus
        // translucent and reveals the interior through its panels.  Opaque slots
        // are never allowed to be faded by a dynamic alpha value; windows and
        // explicitly alpha-tested/blended slots retain their authored behavior.
        alpha.clear();
        alpha.extend_from_slice(snap.slot_alpha(m));
        for (slot, mat) in scene.instances[*inst].materials.iter().enumerate() {
            if scene
                .materials
                .get(*mat)
                .is_some_and(|m| m.alpha == omsi_render::AlphaMode::Opaque)
            {
                if let Some(a) = alpha.get_mut(slot) {
                    *a = 1.0;
                }
            }
        }
        renderer.set_params(scene, *inst, alpha, m.visible, snap.slot_uv(m));
        renderer.set_slot_light(scene, *inst, snap.slot_light(m));
        renderer.set_slot_night(scene, *inst, snap.slot_night(m));
        renderer.set_interior(scene, *inst, m.interior);
    }
}

/// Lamps further than this from the viewer (m) are updated in turns (see `sync_lamps`).
const LAMP_NEAR: f64 = 300.0;
/// A far lamp is updated every this many frames.
const LAMP_TURNS: u32 = 4;

/// Is it the turn of the far lamp `key` in frame `turn`? (Spread by the key, so that every
/// frame updates about the same share of them.)
fn lamp_turn_now(key: usize, turn: u32) -> bool {
    let spread = (key as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 60;
    (turn.wrapping_add(spread as u32)) % LAMP_TURNS == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A far lamp gets its turn once in every `LAMP_TURNS` frames, and the lamps' turns are
    /// spread over those frames.
    #[test]
    fn far_lamps_take_turns() {
        let mut per_frame = [0usize; LAMP_TURNS as usize];
        for key in (0..4000usize).map(|k| 0x1000_0000 + k * 48) {
            let due: Vec<u32> = (0..LAMP_TURNS).filter(|&t| lamp_turn_now(key, t)).collect();
            assert_eq!(due.len(), 1, "{key:#x}");
            per_frame[due[0] as usize] += 1;
        }
        assert!(per_frame.iter().all(|&n| n > 700), "{per_frame:?}");
    }
}
