//! The LAN, the people and the player's duty in the window's frame.

use super::*;

impl App {
    /// The LAN session (a stage of its own: a joining player's bus is loaded here, and that
    /// frame was counted as the people's).
    pub(super) fn frame_lan(&mut self, dt: f32) {
        self.tick_lan(dt);
    }

    /// The player's duty, the personnel file's step, the placing of vehicles, the host's map
    /// edits and `--on-foot`.
    pub(super) fn frame_duty(&mut self, dt: f32) {
        if self.session.duty.is_none() {
            self.session.career.no_trip();
        }
        let announce = self.settings.auto_announce;
        if let (Some(d), Some(p), Some(w), false) = (
            self.session.duty.as_mut(),
            self.player.as_mut(),
            self.world.as_ref(),
            self.paused,
        ) {
            steps::duty_step(
                d,
                p,
                w,
                &mut self.session.career,
                &mut self.session.journey,
                &self.args.root,
                self.clock.time,
                Some(&self.clock),
                true,
                Some(&mut self.integrations.plugin_events),
                announce,
            );
        }
        if let Some(p) = self.player.as_mut() {
            let riders = self.session.humans.as_ref().map(|h| h.riding()).unwrap_or(0);
            // (a frame after the session was written must not start another one)
            let tick = !self.exiting && !self.paused;
            let crash = steps::career_step(&mut self.session.career, p, riders, self.session.duty.is_some(), dt, tick);
            if crash > 0.0 {
                self.service_msg = Some((format!("Crash: {:.0} kJ", crash / 1000.0), 6.0));
                use omsi_plugin::InfoValue::Num;
                let args = vec![Num(crash as f64 / 1000.0), Num(p.vehicle.physics.velocity_kmh().abs() as f64)];
                crate::plugins::queue_event(&mut self.integrations.plugin_events, "crash", args);
            }
        }
        for j in self.session.career.take_jolts() {
            use crate::plugins::num_f32;
            let args = vec![num_f32(j.along), num_f32(j.across), num_f32(j.speed_kmh), omsi_plugin::InfoValue::Num(j.riders as f64)];
            crate::plugins::queue_event(&mut self.integrations.plugin_events, "jolt", args);
        }
        if !self.paused {
            crate::admin::guard_fall(self, dt);
        }
        self.placing_frame();
        // the host sends every edit of the map again now and then (players join)
        if self.net.lan.as_ref().map(|l| l.role == omsi_net::Role::Host).unwrap_or(false) {
            self.menus.editor_sync_t -= dt;
            if self.menus.editor_sync_t <= 0.0 {
                self.menus.editor_sync_t = 10.0;
                self.editor_broadcast(true);
            }
        }
        // --on-foot: the bus the start put down goes, the player stands beside it
        if self.args.on_foot && self.world.is_some() {
            self.args.on_foot = false;
            if self.player.is_some() {
                self.remove_driven_vehicle();
            } else if let Some(c) = self.camera.as_ref() {
                // (no bus came: where the camera stands, on the ground)
                let p = c.position;
                let z = self.world.as_ref().and_then(|w| w.walk_height(p.x, p.y)).unwrap_or(p.z - 1.7);
                let yaw = c.yaw as f64;
                self.start_on_foot(glam::DVec3::new(p.x, p.y, z), yaw);
            }
            self.service_msg = Some(("On foot: Esc menu, Place a vehicle..., then G at its driver's door to drive it".into(), 8.0));
        }
    }
}

crate::app::stage_ctx!(
    /// The stage after the people's part of the game.
    AfterHumansCx { reads: [clock], writes: [people, view, player, ui, integrations] }
);

/// After the people: the walker aboard a bus where the bus is this frame, and the fuel pump
/// or bus wash running.
pub(super) fn after_humans(cx: AfterHumansCx, dt: f32) {
    if let Some(f) = cx.people.on_foot.as_mut() {
        f.after_humans(cx.people.humans.as_ref(), cx.view.camera.as_mut());
    }
    if !*cx.clock.paused {
        cx.player.tick_service(dt, cx.ui.service_msg, &mut cx.integrations.plugin_events);
    }
}

crate::app::stage_ctx!(
    /// The people's part of the game.
    HumansCx { reads: [view, clock, net, duty, world], writes: [people, traffic, player, gfx, ui] }
);

/// The people at the stops and in the buses, and their view sync.
pub(super) fn humans(cx: HumansCx, dt: f32) {
    let sight = cx.view.camera.as_ref().zip(cx.gfx.gfx.surface.as_ref())
        .and_then(|(c, s)| crate::app_impl::sight_extent(cx.common.settings, cx.view, c, (s.config.width, s.config.height)));
    if let (Some(h), Some(w), Some(r), Some(scene)) = (
        cx.people.humans.as_mut(),
        cx.world.world.as_ref(),
        cx.gfx.renderer.as_ref(),
        cx.gfx.scene.as_mut(),
    ) {
        // (where the camera is, as Omsi.exe fills the stops round the tile it shows - at the
        // bus's height, not up in the sky of the map view. Kept at the player's bus, the stops
        // an AI bus was watched at from the free camera stood empty: nobody got on or off
        // after its first stop, #2319)
        let bus = cx.player.player.as_ref().map(|p| p.vehicle.position);
        let center = match (cx.view.camera.as_ref().map(|c| c.position), bus) {
            (Some(c), Some(b)) => DVec3::new(c.x, c.y, b.z),
            (Some(c), None) => c,
            (None, Some(b)) => b,
            (None, None) => DVec3::ZERO,
        };
        // (the trips due at the stops soon: once a game minute, #1415)
        if let (Some(s), Some(t)) = (cx.traffic.schedule.as_ref(), cx.traffic.traffic.as_ref()) {
            if (t.day_time - h.due_at).abs() >= 60.0 {
                let t0 = std::time::Instant::now();
                h.due_dests = Some(s.due_destinations(t.day_time));
                h.due_at = t.day_time;
                if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                    h.stage_stats.add("due destinations", t0.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }
        if h.stop_targets.is_none() {
            h.stop_targets = cx.traffic.schedule.as_ref().map(|s| s.stop_targets());
            h.stop_names = cx.traffic.schedule.as_ref().map(|s| s.stop_names());
            h.stop_target_objects = cx.traffic.schedule.as_ref().map(|s| s.stop_target_objects());
            if let Some(t) = &h.stop_targets {
                log::info!("people: {} bus stops with timetable targets", t.len());
            }
        }
        // (whom the player's bus takes on: by the duty, or in free drive by its terminus)
        h.set_duty(cx.duty.duty.as_ref());
        // (the riders leave a bus the driver has walked away from)
        h.driver_away = cx.people.on_foot.as_ref().is_some_and(|f| {
            let own = Some(crate::humans::BusId::Player);
            f.seat.map(|s| s.0) != own && f.inside.map(|i| i.0) != own
        });
        // (OMSI's `AIPassFactor`, the passengers setting in per cent)
        steps::humans_by_hour(h, w, cx.clock.clock.time, cx.common.settings.pax_density, cx.duty.duty.as_ref());
        // The periodic fillings of the map each get a frame of their own: the traffic's
        // (`traffic_heavy_frame`, set in its stage before this one), the strollers and
        // stops of the people's step (every second, in `tick_inner`) and this one (every
        // two seconds). Whichever finds its frame taken waits for the next; together in
        // one frame they were one long frame every two seconds. Not the first filling:
        // the timetable buses standing at their stops when the session starts look for
        // the people there in the frame the traffic was put out, and a frame later they
        // had passed the stop with nobody on it.
        let settled = h.started;
        let traffic_heavy = std::mem::take(&mut *cx.traffic.traffic_heavy_frame) && settled;
        h.defer_populate = traffic_heavy;
        let step_dt = if *cx.clock.paused { 0.0 } else { dt };
        let step_populates = settled && !traffic_heavy && cx.traffic.traffic.is_some() && h.stroll_timer - step_dt <= 0.0;
        *cx.people.humans_populate_t -= dt;
        if *cx.people.humans_populate_t <= 0.0 && !*cx.clock.paused && !traffic_heavy && !step_populates {
            *cx.people.humans_populate_t = 2.0;
            let t0 = std::time::Instant::now();
            h.populate(w, center);
            view_sync::apply_people(h, &mut cx.gfx.gfx.sim_view.people, w, r, scene);
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                h.stage_stats.add("app populate", t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
        if let (Some(cam), Some(s)) = (cx.view.camera.as_ref(), cx.gfx.gfx.surface.as_ref()) {
            h.eye = Some(humans::Eye::of(
                cam,
                s.config.width as f32 / s.config.height.max(1) as f32,
            ).widened(sight));
        }
        // (the other LAN players' buses, for their riders to sit in)
        h.set_remote_buses(cx.net.remotes.remotes.iter().map(|(id, r)| (*id, r.vehicle())));
        // (and the vehicles the player placed and left, with their riders)
        h.set_placed_buses(cx.player.placed.iter().map(|q| (q.uid, &q.vehicle)));
        h.set_player_next_stop(
            cx.duty.duty
                .as_ref()
                .and_then(|d| d.trip().stops.get(d.next_stop)),
        );
        let step_dt = if *cx.clock.paused { 0.0 } else { dt };
        // (OMSI_SIM_THREAD: what the people's step made during the last picture is taken
        // now, and their next step goes there with this frame's, see `sim_thread`)
        let mut took = false;
        if let Some(back) = cx.traffic.pipeline.take_people_stepped() {
            let started = Instant::now().checked_sub(std::time::Duration::from_secs_f64(back.secs)).unwrap_or_else(Instant::now);
            steps::humans_after_step(h, &mut cx.gfx.gfx.sim_view.people, back.dt, w, cx.player.player.as_mut(), cx.traffic.traffic.as_mut(), r, scene, started);
            took = back.took;
        }
        if cx.traffic.pipeline.takes_people(cx.traffic.traffic.as_ref().map(|t| &t.sim), h) {
            let bus = cx.player.player.as_ref().map(|p| h.player_bus_view(&p.vehicle));
            cx.traffic.pipeline.owe_people(step_dt, w.data(), bus);
        } else {
            took |= steps::tick_humans(
                h,
                &mut cx.gfx.gfx.sim_view.people,
                step_dt,
                w,
                cx.player.player.as_mut(),
                cx.traffic.traffic.as_mut(),
                r,
                scene,
            );
        }
        if let Some(m) = h.take_message() {
            *cx.ui.service_msg = Some((m, 6.0));
        }
        if let Some(t) = h.take_dialog().and_then(|n| passenger_dialog(&n)) {
            *cx.ui.service_msg = Some((t, 5.0));
        }
        if let Some(p) = cx.player.player.as_mut() {
            if took {
                p.vehicle.set_var("GivenTicket", -1.0);
            }
            h.give_ticket = std::mem::take(&mut p.give_ticket);
            if std::mem::take(&mut p.take_change) {
                h.take_change_tray();
            }
            if std::mem::take(&mut h.stop_request) {
                // a passenger's request is the vehicle trigger Omsi.exe fires
                // (0x62e42c), not the cab's stop button `door_haltewunsch`,
                // whose switch and brake sounds some buses play
                p.vehicle.trigger("int_haltewunsch");
            }
            h.write_pax_vars(&mut p.vehicle);
            p.vehicle.host.humans_on_path_link = h.path_link_counts();
            p.vehicle.host.humans_on_seat = h.seat_counts();
            let mut coins: Vec<usize> = std::mem::take(&mut p.vehicle.host.change_coins);
            if std::mem::take(&mut p.give_change) {
                let owed = h.change_due.unwrap_or(0.0);
                if let Some(m) = h.money.as_mut() {
                    let given = m.value_of(&coins);
                    coins.extend(m.change_left_coins(owed - given));
                }
            }
            h.give_change(w, r, scene, &coins);
        }
        // the view sync of the people: the coins and ticket blocks of the player's bus,
        // then everybody's pose (see `view_sync`), in the frame's last tick only
        if cx.clock.fixed_tick.view_due() {
            let bus = cx.player.player.as_ref().map(|p| &p.vehicle);
            view_sync::sync(ViewSync::people(h, bus, center), &mut cx.gfx.gfx.sim_view, w, r, scene);
        }
    }
}

/// The player on foot, and the other players walking about.
pub(super) fn on_foot(mut cx: crate::on_foot::OnFootCx, dt: f32) {
    let dt = if *cx.clock.paused { 0.0 } else { dt };
    crate::on_foot::tick(&mut cx, dt);
    crate::on_foot::remote_walkers(&mut cx);
}

/// OMSI's hint for what a passenger has just said (`TooCold_1`: "Passengers complain about
/// the low temperature in your bus!"), from the game's language files
/// (`Languages/<LANG>_basic*.olf`, `RL_PassDlg_<name>`): Omsi.exe shows them as the riders
/// speak. None for what has no hint (a greeting).
fn passenger_dialog(name: &str) -> Option<String> {
    static TEXTS: std::sync::OnceLock<Option<omsi_content::language::Language>> = std::sync::OnceLock::new();
    let texts = TEXTS.get_or_init(|| {
        let lang = crate::settings::Settings::load().language;
        let load = |lang: &str| {
            let mut out: Option<omsi_content::language::Language> = None;
            for dir in omsi_cfg::content_dirs("Languages") {
                for file in ["basic", "basic_103", "basic_200"] {
                    if let Ok(l) = omsi_content::language::Language::load(&dir.join(format!("{lang}_{file}.olf"))) {
                        match out.as_mut() {
                            Some(o) => o.merge(l),
                            None => out = Some(l),
                        }
                    }
                }
            }
            out
        };
        load(&lang).or_else(|| load("ENG"))
    });
    let t = texts.as_ref()?.strings.get(&format!("RL_PassDlg_{name}"))?.trim().trim_matches('"').replace("\"\"", "\"");
    (!t.is_empty()).then_some(t)
}

