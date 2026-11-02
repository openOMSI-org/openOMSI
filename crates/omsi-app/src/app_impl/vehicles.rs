//! Placing, swapping, reloading and switching the vehicles in the world.

use super::*;

impl App {
    /// Put the vehicle file `bus` down beside the camera or the bus driven, in `paint` (a
    /// scheme's name; None: at random) with the depot file `hof` (None: the map's).
    /// The driven vehicle read again from its files and put where it stands (#728).
    pub(crate) fn reload_driven_vehicle(&mut self) {
        let Some(p) = self.player.as_ref() else {
            self.service_msg = Some(("There is no vehicle to reload: you are on foot".into(), 3.0));
            return;
        };
        // (the file under its content root, as the vehicle lists name it: a whole path was
        // taken for one under the game's folder)
        let file = &p.vehicle.ty.def.path;
        let bus = omsi_cfg::content_roots().iter().chain(std::iter::once(&self.args.root)).find_map(|r| file.strip_prefix(r).ok()).unwrap_or(file).to_string_lossy().replace('\\', "/");
        let paint = p.vehicle.host.paint_scheme.flatten().and_then(|i| p.vehicle.ty.paint_schemes.get(i)).map(|s| s.name.clone());
        // (the depot file by its file name, as `find_hof` looks for it)
        let hof = p.vehicle.host.hof.as_ref().and_then(|h| h.path.file_stem().map(|s| s.to_string_lossy().to_string()).or_else(|| Some(h.name.clone())));
        let idle = self.menus.pending_placement.is_none();
        self.menus.swap_pending = true;
        self.place_vehicle(&bus, paint, hof);
        // (said when it is in place, see `placement`)
        if let Some(p) = self.menus.pending_placement.as_mut().filter(|_| idle) {
            p.reload = true;
        }
    }

    pub(crate) fn settle_depot_switch(&mut self, quitting: bool) {
        if quitting {
            self.menus.pending_placement = None;
        }
        let Some(sw) = &self.session.depot_prev else { return };
        let swapped_in = self.menus.swapped_in;
        let now = self.player.as_ref().map(|p| (p.uid, swapped_in == Some(p.uid) && self.args.bus.as_deref().is_some_and(|b| crate::depot_bus::is_this_bus(&p.vehicle, b))));
        let Some(landed) = crate::depot_bus::switch_landed(self.menus.pending_placement.is_some(), Some(sw.old_uid), now) else { return };
        let Some(sw) = self.session.depot_prev.take() else { return };
        if landed {
            if let Some((old, outing, wear)) = sw.record {
                crate::depot_bus::record_wear(&old, &outing, wear);
            }
            self.session.depot_base = sw.base;
        } else {
            log::warn!("depot bus {:?}: not driven, back to {:?}", self.args.depot, sw.prev.0);
            (self.args.depot, self.args.bus) = sw.prev;
        }
    }

    pub(crate) fn take_depot_bus(&mut self, id: &str) {
        self.settle_depot_switch(false);
        let Some(b) = crate::depot_bus::listed().into_iter().find(|b| b.id == id) else {
            self.service_msg = Some(("That bus is no longer in the depot".into(), 3.0));
            return;
        };
        let before = (self.args.number.clone(), self.args.plate.clone(), self.args.depot.clone(), self.args.bus.clone());
        let driven = self.player.as_ref().map(|p| p.uid);
        let record = match (before.2.clone(), self.player.as_ref()) {
            (Some(old), Some(p)) if crate::depot_bus::drives(&p.vehicle.ty.def.path.to_string_lossy(), crate::depot_bus::bus_of(&old).as_deref()) => {
                Some((old, crate::depot_bus::outing_since(&self.session.career, &self.args, &self.session.depot_base), crate::depot_bus::wear(&p.vehicle)))
            }
            _ => None,
        };
        self.args.number = Some(b.number.clone()).filter(|n| !n.trim().is_empty());
        self.args.plate = Some(b.plate.clone()).filter(|n| !n.trim().is_empty());
        self.args.depot = Some(b.id.clone());
        self.args.bus = Some(b.bus.clone());
        let idle = self.menus.pending_placement.is_none();
        if idle {
            self.menus.swapped_in = None;
        }
        self.menus.swap_pending = driven.is_some();
        self.place_vehicle(&b.bus, Some(b.paint.clone()).filter(|p| !p.trim().is_empty()), None);
        (self.args.number, self.args.plate) = (before.0, before.1);
        let started = idle && self.menus.pending_placement.is_some();
        let Some(old_uid) = driven.filter(|_| started) else {
            (self.args.depot, self.args.bus) = (before.2, before.3);
            return;
        };
        let c = &self.session.career;
        let base = omsi_launcher_lib::depot::Outing { metres: c.metres, seconds: c.seconds, stops: c.stops[0], tickets: c.tickets.0, takings: c.tickets.1, crashes: c.crashes[0], ..Default::default() };
        self.session.depot_prev = Some(crate::depot_bus::DepotSwitch { prev: (before.2, before.3), old_uid, record, base });
    }

    /// The vehicle is read and its meshes and textures made on a worker thread; the next
    /// frames go on and `placement` puts it into the world when it is ready.
    pub(crate) fn place_vehicle(&mut self, bus: &str, paint: Option<String>, hof: Option<String>) {
        if self.menus.pending_placement.is_some() {
            self.menus.swap_pending = false;
            self.service_msg = Some(("A vehicle is still loading".into(), 4.0));
            return;
        }
        // (in the driven vehicle's place, see `swap_pending`)
        let swap = std::mem::take(&mut self.menus.swap_pending) && self.player.is_some();
        let name = self.menus.vehicle_list.iter().find(|v| v.1 == bus).map(|v| v.0.clone()).unwrap_or_else(|| bus.to_string());
        // (a server's own buses only - its `vehicles` list, #1183 - whoever asks: the lists,
        // a plugin, the input script)
        if crate::lan::server_offers().is_some_and(|o| !crate::lan::offers(&o, bus)) {
            self.service_msg = Some((omsi_ui::i18n::tr_fill("The server does not offer {name}", &[("{name}", &name)]), 4.0));
            return;
        }
        let bus = bus.to_string();
        let (Some(w), Some(r), Some(cam)) = (self.world.clone(), self.renderer.as_ref(), self.camera.as_ref()) else { return };
        let (x, y, heading) = match (self.view.as_str(), self.player.as_ref()) {
            (_, Some(p)) if swap => (p.vehicle.position.x, p.vehicle.position.y, p.vehicle.heading),
            ("free", _) | (_, None) => {
                let f = cam.forward();
                let flat = glam::DVec2::new(f.x as f64, f.y as f64).normalize_or_zero();
                let at = cam.position.truncate() + flat * 15.0;
                (at.x, at.y, cam.yaw as f64)
            }
            (_, Some(p)) => {
                let h = p.vehicle.heading.to_radians();
                let right = glam::DVec2::new(h.cos(), -h.sin());
                let at = p.vehicle.position.truncate() + right * 5.0;
                (at.x, at.y, p.vehicle.heading)
            }
        };
        let one = Args {
            bus: Some(bus.clone()),
            spawn: Some(format!("{x},{y},{heading}")),
            // a bus placed now is a new one, also in a session started from a situation
            // (a full tank if wanted, none of the situation's script pictures)
            situation: None,
            situation_vars: Vec::new(),
            situation_strvars: Vec::new(),
            situation_odometer_km: None,
            situation_others: Vec::new(),
            line: None,
            tour: None,
            trip: None,
            autostart: false,
            paint,
            hof: hof.or(self.args.hof.clone()),
            ..self.args.clone()
        };
        let prefetch = w.placement_prefetch(r);
        let worker_args = one.clone();
        let (tx, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new().name("vehicle-placement".into()).spawn(move || {
            let start = Instant::now();
            let result = crate::spawn::PreparedPlayer::load(&worker_args).map(|mut prepared| {
                prepared.prefetch(prefetch, worker_args.paint.as_deref());
                prepared
            });
            log::info!("vehicle placement: read in {:.3} s", start.elapsed().as_secs_f64());
            let _ = tx.send(result);
        });
        match worker {
            Ok(_) => {
                log::info!("placing {bus} at ({x:.1}, {y:.1}){}", if swap { " in the driven vehicle's place" } else { "" });
                self.menus.pending_placement = Some(crate::spawn::PendingPlacement {
                    receiver,
                    world: w,
                    args: one,
                    name,
                    replace: self.player.as_ref().filter(|_| swap).map(|p| p.uid),
                    reload: false,
                    heading,
                });
                self.service_msg = Some(("Loading vehicle...".into(), 4.0));
            }
            Err(e) => self.service_msg = Some((format!("Could not place {name} ({e})"), 5.0)),
        }
    }

    /// Drive another of the vehicles standing in the world (see [`switch_vehicle`]).
    pub(crate) fn switch_vehicle(&mut self) {
        SwitchCx::of(&mut self.parts()).switch_vehicle();
    }

    /// Take the wheel of the `k`-th placed vehicle (`game_lists::ListKind::SwitchTo`): the
    /// others keep their turn after it.
    pub(crate) fn switch_vehicle_to(&mut self, k: usize) {
        if k < self.session.placed.len() {
            self.session.placed.rotate_left(k);
        }
        self.switch_vehicle();
    }

    pub(crate) fn walk_to(&mut self, at: glam::DVec2) {
        if self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
            self.service_msg = Some((omsi_ui::tr("In a LAN session the city map does not move you").into_owned(), 3.0));
            return;
        }
        let top = glam::DVec3::new(at.x, at.y, 3000.0);
        let street_height = || {
            let p = glam::DVec3::new(at.x, at.y, 0.0);
            let nets = [self.session.traffic.as_ref().map(|t| &t.net), self.menus.navigator.as_ref().and_then(|n| n.map_net())];
            nets.into_iter().flatten().find_map(|net| net.nearest_lane(p, omsi_sim::traffic::LaneKind::Street).filter(|(_, _, d)| *d <= 300.0).map(|(l, s, _)| net.lanes[l].at(s).0.z))
        };
        let ground = self.world.clone().and_then(|w| crate::placing::ground_hit(&w, top, glam::DVec3::NEG_Z, 6000.0)).or_else(|| street_height().map(|z| glam::DVec3::new(at.x, at.y, z)));
        let Some(g) = ground else {
            self.service_msg = Some((omsi_ui::tr("No ground to stand on there").into_owned(), 3.0));
            return;
        };
        if self.session.on_foot.is_none() && self.player.is_none() {
            self.service_msg = Some((omsi_ui::tr("There is no bus of your own to get up from").into_owned(), 3.0));
            return;
        }
        if self.session.on_foot.is_none() {
            self.get_up();
        }
        let Some(f) = self.session.on_foot.as_mut() else { return };
        f.pos = g;
        f.vel = glam::DVec2::ZERO;
        (f.lift, f.vz) = (0.0, 0.0);
        (f.seat, f.inside, f.transit, f.arrive) = (None, None, None, None);
        (f.footing, f.bus_turn) = (None, None);
        self.service_msg = Some((omsi_ui::tr("You are on foot where the map was clicked").into_owned(), 3.0));
    }

    /// Put the bus on the street nearest the world point `at` (the city map's Ctrl+click),
    /// facing along it.
    pub(crate) fn place_bus_at(&mut self, at: glam::DVec2) {
        self.place_bus_facing(at, None);
    }

    pub(crate) fn place_bus_facing(&mut self, at: glam::DVec2, facing: Option<f64>) {
        let p = glam::DVec3::new(at.x, at.y, 0.0);
        // the traffic's lanes (the tiles loaded around the bus), else the navigator's of the
        // whole map: a street far off on a big map was "no street" until the bus had been
        // flown there (#235). (the height of the point does not matter: the nearest by the
        // ground plan)
        let nets = [self.session.traffic.as_ref().map(|t| &t.net), self.menus.navigator.as_ref().and_then(|n| n.map_net())];
        let Some((net, (lane, s, _))) = nets
            .into_iter()
            .flatten()
            .find_map(|net| {
                let along = facing.and_then(|h| net.lane_along(p, h, omsi_sim::traffic::LaneKind::Street, 25.0, 75.0));
                along.or_else(|| net.nearest_lane(p, omsi_sim::traffic::LaneKind::Street)).filter(|(_, _, d)| *d <= 300.0).map(|f| (net, f))
            })
        else {
            self.service_msg = Some(("No street near that point".into(), 3.0));
            return;
        };
        let l = &net.lanes[lane];
        let (pos, heading) = l.at(s);
        let heading = heading as f64;
        if crate::admin::request_teleport(self, pos, heading) {
            self.service_msg = Some((omsi_ui::tr("The bus stands where the map was clicked").into_owned(), 3.0));
            self.service_event("teleport", "player", None);
        }
    }
}

crate::app::stage_ctx!(
    /// The placement stage's part of the game: a vehicle read on a worker put into the world.
    PlacementCx { reads: [world, sound], writes: [menus, player, people, view, ui, gfx] }
);

crate::app::stage_ctx!(
    /// Driving the next placed vehicle: its part of the game.
    SwitchCx { reads: [sound], writes: [player, people, view, ui] }
);

impl SwitchCx<'_, '_> {
    fn switch_vehicle(self) {
        switch_vehicle(self.player, self.people, self.view, self.ui, self.sound, self.common.settings);
    }
}

/// `q` (just spawned where the driven vehicle stands) becomes the one driven, and the
/// one driven until now goes, with whoever rode in it (#728).
fn replace_driven_vehicle(q: Player, cx: &mut PlacementCx) {
    let uid = q.uid;
    cx.player.placed.insert(0, q);
    switch_vehicle(cx.player, cx.people, cx.view, cx.ui, cx.sound, cx.common.settings);
    if !cx.player.player.as_ref().is_some_and(|p| p.uid == uid) {
        return;
    }
    let Some(mut old) = cx.player.placed.pop() else { return };
    if let (Some(a), Some(mut ss)) = (cx.sound.audio.as_ref(), old.sounds.take()) {
        ss.stop_all(a);
    }
    if let (Some(w), Some(r), Some(scene)) = (cx.world.world.clone(), cx.gfx.renderer.as_ref(), cx.gfx.scene.as_mut()) {
        if let Some(h) = cx.people.humans.as_mut() {
            h.evict(crate::humans::BusId::Ai(crate::humans::placed_bus_id(old.uid)), &w);
        }
        if let Some(mut d) = old.driver.take() {
            d.hide(r, scene);
        }
        w.release_vehicle(r, scene, old.render);
        for t in old.trailer_renders {
            w.release_vehicle(r, scene, t);
        }
    }
}

/// The vehicle `place_vehicle` reads, put into the world once it is ready (each frame,
/// paused or not; nothing here waits for the worker). It is dropped when another map was
/// loaded meanwhile or the vehicle it was to replace is no longer driven; a worker that
/// failed leaves everything as it was.
pub(crate) fn placement(mut cx: PlacementCx) {
    use std::sync::mpsc::TryRecvError;
    let Some(mut pending) = cx.menus.menus.pending_placement.take() else { return };
    let prepared = match pending.receiver.try_recv() {
        Err(TryRecvError::Empty) => {
            cx.menus.menus.pending_placement = Some(pending);
            return;
        }
        Err(TryRecvError::Disconnected) => {
            log::warn!("placing {}: the loading stopped", pending.name);
            *cx.ui.service_msg = Some(("Vehicle loading stopped unexpectedly".into(), 5.0));
            return;
        }
        Ok(Err(e)) => {
            log::warn!("placing {}: {e:#}", pending.name);
            *cx.ui.service_msg = Some((format!("Could not place {} ({e:#})", pending.name), 5.0));
            return;
        }
        Ok(Ok(prepared)) => prepared,
    };
    if cx.world.world.as_ref().is_none_or(|w| !Arc::ptr_eq(w, &pending.world)) {
        return;
    }
    if let Some(uid) = pending.replace {
        let Some(p) = cx.player.player.as_ref().filter(|p| p.uid == uid) else { return };
        // (where the bus driven stands now: it may have moved while the other was read)
        pending.args.spawn = Some(format!("{},{},{}", p.vehicle.position.x, p.vehicle.position.y, p.vehicle.heading));
    }
    let (Some(r), Some(scene)) = (cx.gfx.renderer.as_ref(), cx.gfx.scene.as_mut()) else { return };
    let start = Instant::now();
    let result = crate::spawn::spawn_player_prepared(&pending.args, &pending.world, r, scene, prepared);
    log::info!("vehicle placement: put into the world in {:.3} s", start.elapsed().as_secs_f64());
    match result {
        Ok(Some(q)) if pending.replace.is_some() => {
            let name = format!("{} {}", q.vehicle.ty.def.manufacturer, q.vehicle.ty.def.type_name);
            cx.menus.menus.swapped_in = Some(q.uid);
            replace_driven_vehicle(q, &mut cx);
            *cx.ui.service_msg = Some(if pending.reload {
                (format!("Reloaded {} from its files", name.trim()), 4.0)
            } else {
                ("Vehicle loaded".into(), 4.0)
            });
        }
        Ok(Some(q)) => {
            let uid = q.uid;
            cx.player.placed.push(q);
            // (then put down with the mouse, where the player wants it)
            crate::placing::begin_placing(cx.menus.menus, cx.ui.service_msg, uid, pending.heading);
            *cx.ui.service_msg = Some(("Vehicle loaded".into(), 4.0));
        }
        Ok(None) => {}
        Err(e) => *cx.ui.service_msg = Some((format!("Could not place {} ({e:#})", pending.name), 5.0)),
    }
}

/// Drive another of the vehicles standing in the world (a situation's): the one driven
/// now stays where it is with everything as it was, and its sounds go to the next one.
fn switch_vehicle(
    player: &mut crate::app::PlayerPart,
    people: &mut crate::app::PeoplePart,
    view: &mut crate::app::ViewPart,
    ui: &mut crate::app::UiPart,
    sound: &SoundState,
    settings: &settings::Settings,
) {
    if player.placed.is_empty() {
        *ui.service_msg = Some(("There is no other vehicle to drive".into(), 3.0));
        return;
    }
    let Some(mut now) = player.player.take() else {
        // on foot without a bus: the first placed one's wheel
        crate::on_foot::take_placed(0, player, people, view, ui, sound, settings);
        return;
    };
    if let (Some(a), Some(mut ss)) = (sound.audio.as_ref(), now.sounds.take()) {
        ss.stop_all(a);
    }
    let mut next = player.placed.remove(0);
    if let Some(a) = sound.audio.as_ref() {
        next.load_sounds(a);
    }
    // the riders stay in the bus left; the people know the new one's cabin
    if let Some(h) = people.humans.as_mut() {
        h.player_bus_swapped(now.uid, next.uid, &mut next.vehicle);
    }
    next.vehicle.host.auto_clutch = if settings.auto_clutch { 1.0 } else { 0.0 };
    player.placed.push(now);
    let name = format!("{} {}", next.vehicle.ty.def.manufacturer, next.vehicle.ty.def.type_name);
    if let Some(cam) = view.camera.as_ref() {
        *view.camera = Some(next.camera(view.view, cam));
    }
    *player.player = Some(next);
    view.cam.look = (0.0, 0.0);
    *ui.service_msg = Some((omsi_ui::i18n::tr_fill("Now driving: {name}", &[("{name}", &name.trim())]), 4.0));
}
