//! Signals, switches, particles, HTML objects and scripted scenery objects.
use super::*;

/// An object whose night textures are lit by a `[NightMapMode]` timetable.
#[derive(Debug, Clone, Copy)]
pub struct NightMode {
    pub inst: usize,
    /// This object's in-use window and darkness threshold (see [`InUse`]).
    pub use_: InUse,
    pub slots: usize,
}

pub use omsi_sim::scenery::InUse;
pub use omsi_map::calendar::DayKind;

/// A scripted object's dynamic texture change: its type, the variants chosen, its mesh
/// instances and the slots' switches.
type TextureUpdate = (Arc<ObjectType>, Vec<usize>, Vec<usize>, Vec<((usize, usize), bool)>);

/// Where a frame's scenery scripts went (seconds), for OMSI_PROFILE: waiting for the script
/// list (the tile loader places objects under the same lock), gathering what the scripts are
/// given, the scripts themselves, applying what they did (text and HTML textures, sounds,
/// materials, transforms), the dynamic textures, the lamps' sounds.
#[derive(Default, Clone, Copy, Debug)]
pub struct ScriptedTimes {
    pub lock: f64,
    pub inputs: f64,
    pub scripts: f64,
    pub apply: f64,
    pub textures: f64,
    pub lamps: f64,
}

/// The render side of the scripted scenery objects of the loaded tiles, by key (their
/// scripts are the simulation's [`omsi_sim::scenery::SceneryState`], which takes the
/// placed objects from `inbox`).
#[derive(Default)]
pub struct ScriptedSet {
    /// In key order (the order the objects were placed in).
    pub views: Vec<ScriptedView>,
    pub inbox: Arc<omsi_sim::scenery::SceneryInbox>,
}

/// The render side of a scripted scenery object (see [`ScriptedObject`] for the fields).
pub struct ScriptedView {
    /// The key its script has in the simulation's list.
    pub key: u64,
    pub map_id: i64,
    pub pos: DVec3,
    pub tile: (i32, i32),
    /// Its script (the names of the variables the view reads).
    pub program: Arc<omsi_script::Program>,
    pub ty: Arc<ObjectType>,
    pub xf: Mat4,
    pub instances: Vec<usize>,
    pub variants: Vec<(usize, usize, MaterialId, MaterialId, String)>,
    pub sounds: Option<omsi_audio::SoundSet>,
    pub texts: Vec<(TextureId, omsi_sim::texttex::TextTextureState)>,
    pub htmls: Vec<(usize, TextureId)>,
    pub alpha_slots: Vec<LampSlots>,
    pub alpha_last: Vec<Vec<f32>>,
    /// Its meshes' transforms and visibility as the last step left them (for the rays of
    /// the mouse).
    pub mesh_xf: Vec<Mat4>,
    pub mesh_visible: Vec<bool>,
    pub script_lamps: Vec<crate::staging::ScriptLamp>,
}

pub(crate) fn view_of(views: &[ScriptedView], key: u64) -> Option<usize> {
    views.binary_search_by_key(&key, |v| v.key).ok()
}

/// Add the glow of the lamps scenery scripts switch within `range` of the camera, each as
/// bright as its variable was at the last step, by day as well as by night.
pub(crate) fn push_script_lamp_glows(views: &[ScriptedView], camera_pos: DVec3, range: f64, out: &mut Vec<omsi_render::Corona>) {
    let near = |v: &&ScriptedView| v.pos.distance(camera_pos) <= range;
    for view in views.iter().filter(near) {
        for lamp in view.script_lamps.iter().filter(|l| l.level > 0.0) {
            let mut glow = lamp.glow;
            glow.brightness *= lamp.level.min(1.0);
            out.push(glow);
        }
    }
}

impl ScriptedSet {
    /// A placed object: its script goes to the simulation (through the inbox), the rest
    /// stays here.
    pub fn push(&mut self, o: ScriptedObject) {
        let ScriptedObject {
            ty,
            pos,
            xf,
            instances,
            inst,
            controller,
            light_index,
            light_parent,
            map_id,
            variants,
            sounds,
            tile,
            var_parent,
            texts,
            arrivals,
            htmls,
            alpha_slots,
            alpha_last,
            mut script_lamps,
        } = o;
        let key = self.inbox.next_key();
        // (as its script's {init} left them, until its first step)
        for lamp in &mut script_lamps {
            lamp.level = inst.var(&lamp.variable).unwrap_or(0.0);
        }
        let (program, mesh_xf, mesh_visible) = (inst.program.clone(), inst.mesh_transforms.clone(), inst.mesh_visible.clone());
        let mut s = omsi_sim::scenery::SceneryObject::new(key, map_id, pos, tile, inst, ty.sco.night_map_mode);
        s.controller = controller;
        s.light_index = light_index;
        s.light_parent = light_parent;
        s.var_parent = var_parent;
        s.arrivals = arrivals;
        s.text_vars = texts.iter().map(|(_, st)| st.def.variable.trim().to_string()).collect();
        s.text_last = vec![None; texts.len()];
        s.has_pages = !htmls.is_empty();
        s.needs_vars = !variants.is_empty() || !alpha_slots.is_empty() || !ty.dynamic_textures.is_empty() || ty.sco.sound.is_some();
        // (lamps it switches read its variables too)
        s.needs_vars |= !script_lamps.is_empty();
        if !ty.sco.scripts.lua.is_empty() {
            let sc = &ty.sco.scripts;
            s.lua = Some(Arc::new(omsi_sim::scenery::ObjectLua {
                files: sc.lua.clone(),
                range: if sc.lua_range > 0.0 { (sc.lua_range as f64).min(omsi_sim::scenery::SCRIPT_RANGE) } else { 250.0 },
                rate: sc.lua_rate,
                type_name: ty.sco.path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            }));
            // (the way its +y axis points: compass degrees, north 0 and east 90)
            s.heading = xf.y_axis.x.atan2(xf.y_axis.y).to_degrees();
        }
        s.switch_dirs = ty.sco.path_switch_dir.clone();
        if s.switch_dirs.iter().any(|d| d.is_some()) {
            s.sco_path = Some(Arc::from(ty.sco.path.as_path()));
        }
        self.inbox.push(omsi_sim::scenery::SceneryChange::Add(s));
        self.views.push(ScriptedView {
            key,
            map_id,
            pos,
            tile,
            program,
            ty,
            xf,
            instances,
            variants,
            sounds,
            texts,
            htmls,
            alpha_slots,
            alpha_last,
            mesh_xf,
            mesh_visible,
            script_lamps,
        });
    }

    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// The view of the object of `key`.
    pub fn get_mut(&mut self, key: u64) -> Option<&mut ScriptedView> {
        let i = view_of(&self.views, key)?;
        Some(&mut self.views[i])
    }

    /// The objects of tile `key` go (their sounds stop now; their scripts at the
    /// simulation's next step).
    pub fn drop_tile(&mut self, key: (i32, i32), audio: Option<&omsi_audio::AudioEngine>) {
        // (the tile's lamps' scripts go with it too, whether or not it had scripted objects)
        self.inbox.push(omsi_sim::scenery::SceneryChange::DropTile(key));
        if !self.views.iter().any(|v| v.tile == key) {
            return;
        }
        self.views.retain_mut(|v| {
            if v.tile != key {
                return true;
            }
            if let (Some(a), Some(mut ss)) = (audio, v.sounds.take()) {
                ss.stop_all(a);
            }
            false
        });
    }
}

impl World {
    /// Light or darken the night textures of the objects with a `[NightMapMode]` timetable
    /// for this hour of the day (0..24).
    pub fn update_night_modes(&self, renderer: &Renderer, scene: &mut Scene, clock: &omsi_sim::SimClock, brightness: f32) {
        let day = self.day_kind(clock);
        for m in self.night_modes.lock().iter() {
            let v = if m.use_.lit(clock.time, day, brightness) { 1.0 } else { 0.0 };
            renderer.set_slot_night(scene, m.inst, &vec![v; m.slots]);
        }
    }

    /// A number plate from `registrations.txt` for a vehicle with `[registration_free]`
    /// (OMSI gives such a vehicle's `ident` a random line of it at the spawn).
    pub fn free_registration(&self, seed: u64) -> Option<String> {
        let list = self.registrations.get_or_init(|| {
            self.chrono_dirs()
                .iter()
                .rev()
                .chain(std::iter::once(&self.map_dir))
                .map(|d| omsi_cfg::resolve_path(d, "registrations.txt"))
                .find(|p| omsi_cfg::vfs::is_file(p))
                .map(|p| omsi_map::ailists::load_list(&p))
                .unwrap_or_default()
        });
        (!list.is_empty()).then(|| list[(seed % list.len() as u64) as usize].clone())
    }

    /// Working day, holiday and school holidays at `clock`'s date, from the map's
    /// `Holidays.txt`.
    pub fn day_kind(&self, clock: &omsi_sim::SimClock) -> DayKind {
        let cal = self.calendar.get_or_init(|| omsi_map::Calendar::load(&self.map_dir.join("Holidays.txt")).unwrap_or_default());
        let date = clock.date_code();
        DayKind { workday: clock.weekday() < 5, holiday: cal.is_holiday(date), school_holiday: cal.in_holiday_range(date) }
    }

    /// Switch the `NightlightA` material variants of static objects.
    pub fn set_lamps(&self, renderer: &Renderer, scene: &mut Scene, on: bool) {
        for (inst, slot, m_on, m_off) in self.night_slots.lock().iter() {
            renderer.set_material(scene, *inst, *slot, if on { *m_on } else { *m_off });
        }
    }

    /// The simulation's scenery objects take what the tile loader placed or took away
    /// since (before anything reads them).
    pub fn scenery_sync(&self, scenery: &mut omsi_sim::scenery::SceneryState) {
        let inbox = self.scripted.lock().inbox.clone();
        scenery.take_inbox(&inbox);
    }

    /// Whether switch object `id` is set to its path `path` (None: no such switch, or the
    /// path has no `[switchdir]`).
    pub fn switch_set_to(&self, scenery: &mut omsi_sim::scenery::SceneryState, id: i64, path: u16) -> Option<bool> {
        self.scenery_sync(scenery);
        scenery.switch_set_to(id, path)
    }

    /// Set the railway signals: `aspects` gives each signal object's `Signal` (see
    /// [`omsi_sim::scenery::SceneryState::set_signals`]), for the routes of the map.
    pub fn set_signals(&self, scenery: &mut omsi_sim::scenery::SceneryState, aspects: &HashMap<i64, f32>) {
        if self.signal_routes.is_empty() {
            return;
        }
        self.scenery_sync(scenery);
        scenery.set_signals(&self.signal_routes, &self.signal_ids, aspects);
    }

    /// The echo at `p`: (reverberation time, how much of it is heard) - full inside an
    /// underpass's box, fading over its edge distance at the sides.
    pub fn reverb_at(&self, p: DVec3) -> (f32, f32) {
        let me = omsi_sim::collision::Obb::point(p, 0.01);
        let mut best = (0.0f32, 0.0f32);
        for (b, time, fade) in self.data.peek().lists.reverb_zones.iter() {
            if p.z < b.z0 - 1.0 || p.z > b.z1 + 1.0 {
                continue;
            }
            let inside = (-me.separation(b)) as f32;
            let mix = (inside / fade.max(0.1)).clamp(0.0, 1.0);
            if mix > best.1 {
                best = (*time, mix);
            }
        }
        best
    }

    /// The `[htmltexture]` page of a scenery object a ray lands on (within `reach` metres).
    /// The nearest triangle of those objects decides, as for the bus's pages: a part of
    /// the object in front of its page takes the click away from it.
    pub fn html_object_hit(&self, origin: DVec3, dir: glam::Vec3, reach: f32) -> Option<PageHit> {
        let scripted = self.scripted.lock();
        let mut best: Option<(f32, Option<PageHit>)> = None;
        for v in scripted.views.iter().filter(|v| !v.htmls.is_empty()) {
            if (v.pos - origin).length() > reach as f64 + 60.0 {
                continue;
            }
            let local = (origin - v.pos).as_vec3();
            for mi in 0..v.instances.len() {
                let Some((data, o3d_mats, overrides)) = v.ty.meshes.get(mi) else { continue };
                if !v.mesh_visible.get(mi).copied().unwrap_or(true) {
                    continue;
                }
                let xf = v.xf * v.mesh_xf.get(mi).copied().unwrap_or(Mat4::IDENTITY);
                let Some(hit) = omsi_geometry::ray_mesh_hit(local, dir, data, &xf) else { continue };
                if hit.t > reach || best.as_ref().is_some_and(|b| b.0 <= hit.t) {
                    continue;
                }
                // the page the hit material slot shows (a slot that shows none is in the way)
                let slot = data.slot_of(hit.index) as usize;
                let page = overrides
                    .iter()
                    .filter(|m| !m.item && omsi_sim::vehicle::override_slot(o3d_mats, m) == Some(slot))
                    .find_map(|m| m.use_script_texture)
                    .map(|n| n.max(0) as usize)
                    .filter(|n| v.htmls.iter().any(|(i, _)| i == n));
                let page = page.map(|page| PageHit {
                    t: hit.t,
                    map_id: v.map_id,
                    page,
                    u: hit.uv.x.clamp(0.0, 1.0),
                    v: hit.uv.y.clamp(0.0, 1.0),
                });
                best = Some((hit.t, page));
            }
        }
        best.and_then(|b| b.1)
    }

    /// A press, release or move on a page of a scenery object (see [`Self::html_object_hit`]).
    /// What the page does (`omsi.setVar`, `omsi.trigger`) reaches the object's script.
    pub fn html_object_pointer(
        &self,
        scenery: &mut omsi_sim::scenery::SceneryState,
        map_id: i64,
        page: usize,
        u: f32,
        v: f32,
        kind: omsi_sim::htmltex::PointerKind,
    ) -> bool {
        self.scenery_sync(scenery);
        scenery.html_pointer(map_id, page, u, v, kind)
    }

    /// Finds the closest scenery object mesh carrying a `[mouseevent]` under a ray.
    pub fn scenery_object_hit(&self, origin: DVec3, dir: glam::Vec3, reach: f32, spread: f32) -> Option<SceneryHit> {
        let scripted = self.scripted.lock();
        let mut best: Option<SceneryHit> = None;
        let right = glam::Vec3::new(-dir.y, dir.x, 0.0).normalize_or_zero();
        let up = dir.cross(right).normalize_or_zero();
        let dirs = if spread > 0.0 {
            vec![
                dir,
                (dir + right * spread).normalize(),
                (dir - right * spread).normalize(),
                (dir + up * spread).normalize(),
                (dir - up * spread).normalize(),
            ]
        } else {
            vec![dir]
        };
        for v in scripted.views.iter().filter(|v| v.ty.has_mouse_events) {
            if (v.pos - origin).length() > reach as f64 + 60.0 {
                continue;
            }
            let local = (origin - v.pos).as_vec3();
            for mi in 0..v.ty.meshes.len() {
                let Some((data, _, _)) = v.ty.meshes.get(mi) else { continue };
                if !v.mesh_visible.get(mi).copied().unwrap_or(true) {
                    continue;
                }
                let Some(&def_idx) = v.ty.mesh_def_index.get(mi) else { continue };
                let Some(event) = v.ty.model.meshes.get(def_idx).and_then(|m| m.mouse_event.as_ref()) else { continue };
                let xf = v.xf * v.mesh_xf.get(mi).copied().unwrap_or(Mat4::IDENTITY);
                for d in &dirs {
                    if let Some(t) = omsi_geometry::ray_mesh(local, *d, data, &xf) {
                        if t <= reach && best.as_ref().map_or(true, |b| t < b.t) {
                            best = Some(SceneryHit {
                                map_id: v.map_id,
                                mesh_index: mi,
                                event: event.clone(),
                                t,
                            });
                            break;
                        }
                    }
                }
            }
        }
        best
    }

    /// Click down on a scenery object's `[mouseevent]` switch or button.
    pub fn scenery_object_click(&self, scenery: &mut omsi_sim::scenery::SceneryState, map_id: i64, event: &str) -> bool {
        self.scenery_sync(scenery);
        scenery.object_click(map_id, event)
    }

    /// Mouse dragged while holding down a scenery object switch.
    pub fn scenery_object_drag(&self, scenery: &mut omsi_sim::scenery::SceneryState, map_id: i64, event: &str, dx: f32, dy: f32) -> bool {
        self.scenery_sync(scenery);
        scenery.object_drag(map_id, event, dx, dy)
    }

    /// Mouse button released from a scenery object switch (`<event>_off`).
    pub fn scenery_object_release(&self, scenery: &mut omsi_sim::scenery::SceneryState, map_id: i64, event: &str) -> bool {
        self.scenery_sync(scenery);
        scenery.object_release(map_id, event)
    }

    /// Mouse wheel notch over a scenery object switch.
    pub fn scenery_object_wheel(&self, scenery: &mut omsi_sim::scenery::SceneryState, map_id: i64, event: &str, amount: f32) -> bool {
        self.scenery_sync(scenery);
        scenery.object_wheel(map_id, event, amount)
    }

    /// The colour the tile's night light map (its own part, see [`own_tile_of_light_map`])
    /// has at `pos` (0..1, bilinear), or `None` where no light map is loaded: the light it
    /// throws on a vehicle standing there (Omsi.exe samples it at the vehicle's place,
    /// 0x61378c, for its ambient light and `Envir_Brightness`).
    pub fn light_map_light_at(&self, pos: DVec3) -> Option<glam::Vec3> {
        let ts = tile_size();
        let key = ((pos.x / ts).floor() as i32, (pos.y / ts).floor() as i32);
        let img = self.light_maps.lock().get(&key).cloned()?;
        let (w, h) = (img.width as usize, img.height as usize);
        if w == 0 || h == 0 || img.rgba.len() < w * h * 4 {
            return None;
        }
        let u = ((pos.x / ts - key.0 as f64) * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let v = ((1.0 - (pos.y / ts - key.1 as f64)) * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (u.floor() as usize, v.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = ((u - x0 as f64) as f32, (v - y0 as f64) as f32);
        let px = |x: usize, y: usize| {
            let i = (y * w + x) * 4;
            glam::Vec3::new(img.rgba[i] as f32, img.rgba[i + 1] as f32, img.rgba[i + 2] as f32) / 255.0
        };
        let top = px(x0, y0).lerp(px(x1, y0), fx);
        let bottom = px(x0, y1).lerp(px(x1, y1), fx);
        Some(top.lerp(bottom, fy))
    }

    /// Fill the light map atlas with the 5x5 tiles around `eye` (when it moved to another
    /// tile or tiles came or went): the splines and `[LightMapMapping]` objects are lit by it
    /// at night as the terrain is.
    pub fn update_light_map_atlas(&self, renderer: &Renderer, eye: DVec3) {
        // (`OMSI_NO_LIGHT_MAP=1`: the tiles' night light maps left out, for an A/B)
        if omsi_cfg::flags::OMSI_NO_LIGHT_MAP.is_set() {
            return;
        }
        let ts = tile_size();
        let centre = ((eye.x / ts).floor() as i32, (eye.y / ts).floor() as i32);
        let generation = self.light_maps_generation.load(std::sync::atomic::Ordering::Relaxed);
        let mut last = self.light_map_atlas.lock();
        if *last == Some((centre, generation)) {
            return;
        }
        *last = Some((centre, generation));
        let n = omsi_render::LM_ATLAS_TILES as i32;
        let maps = self.light_maps.lock();
        for row in 0..n {
            for col in 0..n {
                // column from the west, row from the north
                let key = (centre.0 - n / 2 + col, centre.1 + n / 2 - row);
                renderer.set_light_map_tile((col as u32, row as u32), maps.get(&key).map(|a| a.as_ref()));
            }
        }
        let sw = ((centre.0 - n / 2) as f64 * ts, (centre.1 - n / 2) as f64 * ts);
        renderer.set_light_map_place(sw.0, sw.1, n as f64 * ts);
    }

    /// Throw the points the trains need (`Traffic::switch_requests`): see
    /// [`omsi_sim::scenery::SceneryState::set_switches`].
    pub fn set_switches(&self, scenery: &mut omsi_sim::scenery::SceneryState, requests: &[(i64, u16)]) {
        if requests.is_empty() {
            return;
        }
        self.scenery_sync(scenery);
        scenery.set_switches(requests);
    }

    /// Move the particles of the placed objects within 1.5 km of `center`, their variables
    /// read from the object's script (the fireworks' frequency).
    pub fn update_particles(&self, dt: f32, center: DVec3, scripts: &omsi_sim::scenery::SceneryOutputs) {
        let mut objs = self.particle_objects.lock();
        if objs.is_empty() {
            return;
        }
        // (an environment switch read once per process)
        static DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *DEBUG.get_or_init(|| omsi_cfg::flags::OMSI_DEBUG_PARTICLES.is_set()) {
            let mut near: Vec<(f64, &ParticleObject)> = objs.values().flatten().map(|po| ((po.pos - center).length(), po)).collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (d, po) in near.iter().take(3) {
                log::info!("particle object {} at ({:.1}, {:.1}, {:.1}), {d:.0} m: {} particles", po.map_id, po.pos.x, po.pos.y, po.pos.z, po.set.particles().count());
            }
        }
        // (the scripted object whose variables an emitter reads: the first of its id, as a
        // search would find it, with its variables before this frame's scripts ran)
        for list in objs.values_mut() {
            for po in list.iter_mut() {
                if (po.pos - center).length() > 1500.0 {
                    continue;
                }
                let value = |n: &str| scripts.watched_var(po.map_id, n).unwrap_or(0.0);
                po.set.update(dt, po.pos, po.rot, &value);
            }
        }
    }

    /// The departures for the HTML pages of the player's vehicle: the stop names its pages asked
    /// for go to the boards, and the departures made for them come back into its host.
    pub fn sync_html_departures(&self, host: &mut omsi_sim::host::VehicleHost) {
        if host.html_departure_wants.is_empty() {
            return;
        }
        let mut boards = self.timetable_boards.lock();
        for k in &host.html_departure_wants {
            if !boards.wanted_names.contains(k) {
                boards.wanted_names.push(k.clone());
            }
        }
        if host.html_departures_gen != boards.departures_gen {
            host.html_departures = host
                .html_departure_wants
                .iter()
                .filter_map(|k| boards.departures.get(k).map(|l| (k.clone(), l.clone())))
                .collect();
            host.html_departures_gen = boards.departures_gen;
        }
    }

    /// Run the scripts and animations of the placed objects near `center` and push their
    /// mesh transforms / visibility to the renderer. `phase_of(controller, light)` gives the
    /// light's current state (the `TrafficLightPhase` value) and whether a vehicle is
    /// asking for it (`TrafficLightApproach`).
    pub fn update_scripted(
        &self,
        scenery: &mut omsi_sim::scenery::SceneryState,
        renderer: &Renderer,
        scene: &mut Scene,
        dt: f32,
        center: DVec3,
        brightness: f32,
        phase_of: &dyn Fn(usize, usize) -> (f32, f32),
        audio: Option<&omsi_audio::AudioEngine>,
        muffled: bool,
        times: &mut ScriptedTimes,
    ) -> usize {
        let now = self.script_clock();
        let day = self.day_kind(&now);
        let t = std::time::Instant::now();
        self.scenery_sync(scenery);
        times.lock += t.elapsed().as_secs_f64();
        // The scripts run in the simulation's list (what each is given gathered in order
        // first, then the scripts side by side on the worker threads), and what they did
        // comes back as commands with values, in object order, applied here.
        let mut step = omsi_sim::scenery::StepTimes::default();
        {
            let t = std::time::Instant::now();
            let boards = self.timetable_boards.lock();
            let controllers = self.controller_of_object.lock();
            times.lock += t.elapsed().as_secs_f64();
            let inputs = omsi_sim::scenery::SceneryInputs {
                clock: boards.clock.as_ref(),
                time: now.time,
                day,
                brightness,
                center,
                phase_of,
                controllers: &controllers,
                by_stop: &boards.by_stop,
                departures: &boards.departures,
                departures_gen: boards.departures_gen,
                pages_near: HTML_OBJECT_NEAR,
            };
            scenery.step(dt, &inputs, &mut step);
        }
        times.inputs += step.inputs;
        times.scripts += step.scripts;
        let updated = self.scenery_apply(scenery.outputs(), renderer, scene, dt, center, audio, muffled, times);
        self.lamp_sounds(&mut scenery.lamps, center, audio, muffled, times);
        updated
    }

    /// The scenery scripts' inputs as owned values, for a step on another thread (see
    /// [`omsi_sim::scenery::OwnedSceneryInputs`]): the list takes what the loader handed
    /// over first.
    pub fn scenery_prepare(
        &self,
        scenery: &mut omsi_sim::scenery::SceneryState,
        center: DVec3,
        brightness: f32,
        times: &mut ScriptedTimes,
    ) -> omsi_sim::scenery::OwnedSceneryInputs {
        let now = self.script_clock();
        let day = self.day_kind(&now);
        let t = std::time::Instant::now();
        self.scenery_sync(scenery);
        let boards = self.timetable_boards.lock();
        let controllers = self.controller_of_object.lock();
        times.lock += t.elapsed().as_secs_f64();
        omsi_sim::scenery::OwnedSceneryInputs {
            clock: boards.clock.clone(),
            time: now.time,
            day,
            brightness,
            center,
            controllers: Arc::new(controllers.clone()),
            by_stop: Arc::new(boards.by_stop.clone()),
            departures: Arc::new(boards.departures.clone()),
            departures_gen: boards.departures_gen,
            pages_near: HTML_OBJECT_NEAR,
        }
    }

    /// What a step of the scenery scripts did, onto the render side and the speakers: the
    /// particle emitters, the commands, the boards' wanted stops and the dynamic textures
    /// (then [`Self::lamp_sounds`]). Returns how many objects ran.
    #[allow(clippy::too_many_arguments)]
    pub fn scenery_apply(
        &self,
        out: &mut omsi_sim::scenery::SceneryOutputs,
        renderer: &Renderer,
        scene: &mut Scene,
        dt: f32,
        center: DVec3,
        audio: Option<&omsi_audio::AudioEngine>,
        muffled: bool,
        times: &mut ScriptedTimes,
    ) -> usize {
        let t = std::time::Instant::now();
        // (the emitters read the variables as they were before the scripts ran)
        self.update_particles(dt, center, out);
        let (updated, texture_updates) = self.apply_scripted(renderer, scene, out, audio, muffled);
        {
            // (the boards keep this frame's lists; the old ones are the next step's buffers)
            let mut boards = self.timetable_boards.lock();
            std::mem::swap(&mut boards.wanted, &mut out.wanted);
            std::mem::swap(&mut boards.wanted_names, &mut out.wanted_names);
        }
        times.apply += out.post + t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        self.apply_texture_updates(renderer, scene, texture_updates);
        times.textures += t.elapsed().as_secs_f64();
        updated
    }

    /// The traffic lamps' own sounds (a level crossing's bell): their scripts run with the
    /// light programs (`Traffic::sync`), what they fired is heard here.
    pub fn lamp_sounds(
        &self,
        lamps: &mut omsi_sim::scenery::LampScripts,
        center: DVec3,
        audio: Option<&omsi_audio::AudioEngine>,
        muffled: bool,
        times: &mut ScriptedTimes,
    ) {
        let t = std::time::Instant::now();
        // the lamps' own sounds (a level crossing's bell): their scripts run with the light
        // programs (`Traffic::sync`), what they fired is heard here
        for lamp in self.light_objects.lock().iter() {
            // (only the lamps that have a sound, and near enough to hear: a city map has
            // nearly a thousand lamps, and going through all of them took 1.8 ms a frame)
            let Some(path) = lamp.sound.as_ref() else { continue };
            let Some(script) = lamp.script.and_then(|k| lamps.get_mut(k)) else { continue };
            let dist = (lamp.pos - center).length();
            if dist >= SOUND_REACH_MAX as f64 {
                if let (Some(a), Some(mut ss)) = (audio, lamp.sounds.lock().take()) {
                    ss.stop_all(a);
                }
                // (what it fired out of earshot is not heard later)
                script.host.fired_triggers.clear();
                continue;
            }
            let fired = std::mem::take(&mut script.host.fired_triggers);
            let inst = &*script;
            if let Some(a) = audio {
                let mut sounds = lamp.sounds.lock();
                self.object_sounds(a, &mut sounds, path, dist, muffled, lamp.pos, lamp.xf, &|n| inst.var(n), &fired);
            }
        }
        times.lamps += t.elapsed().as_secs_f64();
    }

    /// What the scenery scripts did, onto the render side: the commands of a step in their
    /// order (sounds stopped first, then per object its texts, pages, sounds, materials,
    /// texture choices and meshes, as values). Returns how many objects ran and the
    /// dynamic texture changes (applied once the list's lock is let go).
    pub fn apply_scripted(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        out: &mut omsi_sim::scenery::SceneryOutputs,
        audio: Option<&omsi_audio::AudioEngine>,
        muffled: bool,
    ) -> (usize, Vec<TextureUpdate>) {
        let mut updated = 0;
        let mut texture_updates: Vec<TextureUpdate> = Vec::new();
        let mut scripted = self.scripted.lock();
        let views = &mut scripted.views;
        let omsi_sim::scenery::SceneryOutputs { cmds, vars, transforms, visible, .. } = out;
        for cmd in cmds.iter_mut() {
            let s = match cmd {
                omsi_sim::scenery::SceneryCmd::Silence(key) => {
                    // (an object whose tile went since has stopped already)
                    if let Some(i) = view_of(views, *key) {
                        if let (Some(a), Some(mut ss)) = (audio, views[i].sounds.take()) {
                            ss.stop_all(a);
                        }
                    }
                    continue;
                }
                omsi_sim::scenery::SceneryCmd::Show(s) => s,
            };
            let Some(i) = view_of(views, s.key) else { continue };
            let v = &mut views[i];
            let (dist, nightlight) = (s.dist, s.nightlight);
            let vars = &vars[s.vars.0 as usize..s.vars.1 as usize];
            let program = v.program.clone();
            let var = |n: &str| omsi_sim::scenery::var_in(&program, vars, n);
            // text textures from the script's strings whenever they change: read only on
            // `Refresh_Strings`, a board whose string was still empty at its first frame
            // stayed blank for good (#367)
            for (k, text) in &s.texts {
                let Some((tex, st)) = v.texts.get_mut(*k) else { continue };
                if st.update(text) {
                    let (w, h) = (st.def.width.max(1) as u32, st.def.height.max(1) as u32);
                    if let Some(rgba) = st.pending.take() {
                        // OMSI_DUMP_SCENERY_TEXT=<dir>: the pictures as drawn
                        if let Some(dir) = omsi_cfg::flags::OMSI_DUMP_SCENERY_TEXT.os() {
                            let path = std::path::Path::new(&dir).join(format!("text_{}.png", v.map_id));
                            log::info!("scenery text of object {}: {text:?} -> {}", v.map_id, path.display());
                            if let Some(img) = image::RgbaImage::from_raw(w, h, rgba.clone()) {
                                let _ = img.save(&path);
                            }
                        }
                        renderer.update_texture_mips(scene, *tex, &Image { width: w, height: h, rgba, has_alpha: true });
                    }
                }
            }
            // [htmltexture] pages: only near the listener (a page is a whole browser frame;
            // the simulation drove them, here come their pictures)
            for (index, w, h, rgba) in s.pages.drain(..) {
                if let Some((_, tex)) = v.htmls.iter().find(|(i, _)| *i == index) {
                    renderer.update_texture(scene, *tex, &Image { width: w, height: h, rgba, has_alpha: true });
                }
            }
            // [sound] of scenery objects: crossing bells, ambient loops
            let fired: Vec<String> = std::mem::take(&mut s.fired);
            // (out of earshot with nothing playing: nothing to do - finding the sound file
            // for each of a city's scripted objects every frame took 1.8 ms)
            let near = dist < SOUND_REACH_MAX as f64 || v.sounds.is_some();
            if let (Some(a), true) = (audio, near) {
                let ty = v.ty.clone();
                let path = ty.sound_path.get_or_init(|| {
                    let rel = ty.sco.sound.as_ref()?;
                    let dir = ty.sco.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                    Some(omsi_cfg::resolve_path(&dir, rel))
                });
                if let Some(path) = path {
                    self.object_sounds(a, &mut v.sounds, path, dist, muffled, v.pos, v.xf, &var, &fired);
                }
            }
            let switch_value = |name: &str| {
                if name.trim().eq_ignore_ascii_case("NightlightA") {
                    nightlight
                } else {
                    name.trim().parse::<f32>().ok().or_else(|| var(name)).unwrap_or(0.0)
                }
            };
            for (inst, slot, base, item, name) in &v.variants {
                renderer.set_material(scene, *inst, *slot, if change_picks_item(switch_value(name)) { *item } else { *base });
            }
            for lamp in &mut v.script_lamps {
                lamp.level = var(&lamp.variable).unwrap_or(0.0);
            }
            if !v.ty.dynamic_textures.is_empty() {
                let selection = scenery_texture_selection(&v.ty, &var);
                let switches = v.variants.iter().map(|(inst, slot, _, _, name)| ((*inst, *slot), change_picks_item(switch_value(name)))).collect();
                texture_updates.push((v.ty.clone(), selection, v.instances.clone(), switches));
            }
            let (xfs, shown) = (&transforms[s.meshes.0 as usize..s.meshes.1 as usize], &visible[s.meshes.0 as usize..s.meshes.1 as usize]);
            for (k, ((inst, xf), &visible)) in v.instances.iter().zip(xfs).zip(shown).enumerate() {
                renderer.set_transform(scene, *inst, v.pos, v.xf * *xf);
                // (the slots its `[alphascale]` variables fade, see `ScriptedObject::alpha_slots`)
                let alpha = v.alpha_slots.get(k).filter(|l| !l.alpha.is_empty()).map(|l| l.values(&|n| n.trim().parse::<f32>().ok().or_else(|| var(n))).0);
                let p = &mut scene.instances[*inst];
                match alpha {
                    Some(a) => {
                        if p.visible != visible || v.alpha_last.get(k) != Some(&a) {
                            renderer.set_params(scene, *inst, &a, visible, &[]);
                            if let Some(last) = v.alpha_last.get_mut(k) {
                                *last = a;
                            }
                        }
                    }
                    None => {
                        if p.visible != visible {
                            renderer.set_params(scene, *inst, &[], visible, &[]);
                        }
                    }
                }
            }
            // (for the mouse's rays)
            v.mesh_xf.clear();
            v.mesh_xf.extend_from_slice(xfs);
            v.mesh_visible.clear();
            v.mesh_visible.extend_from_slice(shown);
            updated += 1;
        }
        (updated, texture_updates)
    }

    /// The dynamic texture changes the scripts asked for this frame. Scenery placement takes
    /// the GPU-cache lock before the script list, so they are applied after the script-list
    /// lock is released, to keep that lock order consistent.
    fn apply_texture_updates(&self, renderer: &Renderer, scene: &mut Scene, texture_updates: Vec<TextureUpdate>) {
    for (ty, selection, instances, switches) in texture_updates {
        let variant = {
            let mut gpu = self.gpu.lock();
            gpu.dynamic_texture_variant(
                renderer,
                scene,
                Arc::as_ptr(&ty) as usize,
                &selection,
                &self.root,
                &HashMap::new(),
            )
        };
        if let Some(rows) = variant {
            for (mi, row) in rows.iter().enumerate() {
                let Some(&mesh_inst) = instances.get(mi) else {
                    continue;
                };
                for (slot, pair) in row.iter().enumerate() {
                    let Some((base, item)) = pair else { continue };
                    // (the last switch of a slot, as a map collected from them kept it)
                    let item_on = switches
                        .iter()
                        .rev()
                        .find(|(k, _)| *k == (mesh_inst, slot))
                        .is_some_and(|&(_, on)| on);
                    renderer.set_material(
                        scene,
                        mesh_inst,
                        slot,
                        if item_on { *item } else { *base },
                    );
                }
            }
        }
    }
    }

    /// Play a scenery object's `[sound]` (the config at `path`) while the listener is within
    /// its reach (`sound_reach`): loaded when it comes near, faded out and stopped when it
    /// goes.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn object_sounds(
        &self,
        a: &omsi_audio::AudioEngine,
        sounds: &mut Option<omsi_audio::SoundSet>,
        path: &Path,
        dist: f64,
        muffled: bool,
        pos: DVec3,
        xf: Mat4,
        var: &dyn Fn(&str) -> Option<f32>,
        fired: &[String],
    ) {
        // read once per file, the clips in the background (see AudioEngine::clips_ready)
        // (looked up before anything is allocated: this runs every frame for every object
        // that sounds within reach)
        let known = self.sound_cfgs.lock().get(path).cloned();
        let cfg = match known {
            Some(c) => c,
            None => self
                .sound_cfgs
                .lock()
                .entry(path.to_path_buf())
                .or_insert_with(|| omsi_vehicle::SoundCfg::load(path).map_err(|e| log::warn!("{e}")).ok().map(Arc::new))
                .clone(),
        };
        let reach = cfg.as_deref().map_or(SOUND_REACH_MIN, sound_reach);
        if dist >= reach as f64 {
            if let Some(mut ss) = sounds.take() {
                ss.stop_all(a);
            }
            return;
        }
        if sounds.is_none() {
            if let Some(cfg) = cfg {
                let sdir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                if a.clips_ready(&omsi_audio::SoundSet::clip_paths(&cfg, &sdir)) {
                    log::info!("scenery sound {}: {} sounds ({:.0} m away)", path.display(), cfg.sounds.len(), dist);
                    let mut ss = omsi_audio::SoundSet::new(a, &cfg, &sdir);
                    ss.master = self.settings.sound_scenery();
                    *sounds = Some(ss);
                }
            }
        }
        if let Some(ss) = sounds.as_mut() {
            // (the last fifth of the reach fades it out, so that it never stops at once)
            ss.master = self.settings.sound_scenery() * ((reach - dist as f32) / (0.2 * reach)).clamp(0.0, 1.0);
            // scenery (a fountain, machinery, a level crossing bell): heard through the
            // player's own bodywork and glass just like any other sound from outside the cabin
            ss.set_muffled(muffled);
            let xf = Mat4::from_translation(pos.as_vec3()) * xf;
            ss.update(a, var, &xf, fired);
        }
    }
}

/// How far a scenery object's sounds carry: Omsi.exe lowers a `[3d]` sound by its range over
/// the distance and never cuts it off (`omsi_audio::distance_gain`). Played within 300 m
/// only, a sound cube's loud sound stopped dead at that line, still well audible (#2534).
/// It is kept now until its loudest sound is 40 dB down (a hundredth), from 300 m up to the
/// distance the scenery scripts run within (`SCRIPT_RANGE`, beyond which their objects fall
/// silent), and faded out over the last fifth of that.
const SOUND_REACH_MIN: f32 = 300.0;
const SOUND_REACH_MAX: f32 = omsi_sim::scenery::SCRIPT_RANGE as f32;
fn sound_reach(cfg: &omsi_vehicle::SoundCfg) -> f32 {
    let range = cfg.sounds.iter().map(|s| s.range).fold(0.0f32, f32::max);
    (range * 100.0).clamp(SOUND_REACH_MIN, SOUND_REACH_MAX)
}
