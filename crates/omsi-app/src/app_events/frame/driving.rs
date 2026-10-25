//! The player's bus in the window's frame: its step, the view from it, the sound round the
//! camera.

use super::*;

impl App {
    /// The cursor's aim into the cab.
    pub(super) fn frame_player_hover(&mut self) {
        let __th = Instant::now();
        self.trolley_mouse_feed();
        // (the cursor's aim into the cab: again when the cursor or the view
        // turned, else every few frames for switches that moved under it - a ray
        // through every cockpit mesh every frame was a tenth of the frame)
        #[cfg(windows)]
        let headset_camera = self
            .xr
            .vr
            .as_ref()
            .and_then(|vr| vr.navigator_edit_camera());
        #[cfg(not(windows))]
        let headset_camera: Option<omsi_render::Camera> = None;
        let key = headset_camera.as_ref().or(self.camera.as_ref()).map(|c| (self.input.cursor.0.round() as i32, self.input.cursor.1.round() as i32, (c.yaw * 4.0).round() as i32, (c.pitch * 4.0).round() as i32));
        // (the cab sways with the suspension: a view that only turned waits a few frames)
        let cursor_moved = key.map(|k| (k.0, k.1)) != self.menus.hover_key.map(|k| (k.0, k.1));
        if cursor_moved || (key != self.menus.hover_key && self.perf.total_frames % 6 == 0) || self.perf.total_frames % 12 == 0 {
            self.menus.hover_key = key;
            self.update_hover();
        }
        self.perf.profile.add(crate::profile_keys::ProfKey::PlayerHover, __th.elapsed().as_secs_f64());
    }
}

crate::app::stage_ctx!(
    /// The player's bus's step: its part of the game.
    PlayerStepCx { reads: [sound, menus], writes: [player, gfx, world, input, clock, ui, view, integrations, traffic] }
);

/// The bus's step (scripts, physics, the head) and the situation's other vehicles: the
/// `player_tick` stage's, once per tick.
pub(super) fn player_step(cx: PlayerStepCx, dt: f32) {
    let (Some(p), Some(r), Some(scene)) = (
        cx.player.player.as_mut(),
        cx.gfx.renderer.as_ref(),
        cx.gfx.scene.as_mut(),
    ) else {
        return;
    };
    // (while the tile under the bus is being read again - the weather turned
    // to snow and every tile came back with the winter textures - there is
    // no ground under it: it is held where it is rather than falling through
    // the world and being put back somewhere in the sky)
    // (the tile's wheel surfaces - its roads, bridges and yards - not its
    // terrain alone: the terrain comes first while the tile is placed, and a
    // parked bus taken over after a restart fell through its yard onto the
    // ground below before the surfaces came, as Omsi.exe holds it, #1279)
    let ground_here = cx.world.world.as_ref().is_none_or(|w| ground_under(w, p.vehicle.position));
    // `--triggers name@secs`: at that time of the simulation, as the offscreen run fires them
    // (the same tick at any frame rate)
    if !*cx.clock.paused {
        static TIMED: std::sync::OnceLock<Vec<(String, f32)>> = std::sync::OnceLock::new();
        let timed = TIMED.get_or_init(|| crate::cli::parse_triggers(cx.common.args).into_iter().filter(|(_, at)| *at > 0.0).collect());
        let (now, _) = cx.clock.fixed_tick.sim_time();
        for (name, at) in timed {
            if (*at as f64) > now - dt as f64 && (*at as f64) <= now {
                p.fire_timed(name);
            }
        }
    }
    if let Some((v, crashes)) = cx.input.cruise {
        if p.vehicle.crashes != crashes {
            log::info!("cruise: crashed at {:.1}, {:.1}, {:.1} - let go", p.vehicle.position.x, p.vehicle.position.y, p.vehicle.position.z);
            cx.input.cruise = None;
        } else {
            p.vehicle.set_speed(v);
        }
    }
    if !*cx.clock.paused && ground_here {
        // the hands on a trolleybus's ropes
        if p.vehicle.trolley.is_some() {
            let keys = &cx.input.keys;
            let eye = cx.view.camera.as_ref().map(|c| (c.position, c.forward()));
            let hint = crate::trolley_hands::apply(&mut p.vehicle, &crate::trolley_hands::Hands { keys: &|k| keys.contains(&k), view: cx.view.view.as_str(), eye });
            if let (Some(h), None) = (hint, cx.ui.service_msg.as_ref()) {
                *cx.ui.service_msg = Some((h.into(), 2.5));
            }
        }
        // (the developer tools' held variables: before the scripts and after them)
        crate::devtools::apply_holds(cx.ui.devtools.as_deref(), &mut p.vehicle);
        p.tick(
            dt,
            cx.sound.audio.as_ref(),
            cx.view.cam.in_cab,
            !matches!(cx.view.view.as_str(), "free" | "foot"),
        );
        crate::devtools::apply_holds(cx.ui.devtools.as_deref(), &mut p.vehicle);
        if let Some(g) = p.vehicle.trolley.as_deref_mut() {
            if let Some(n) = g.rig.take_notes().pop() {
                *cx.ui.service_msg = Some((n, 3.0));
            }
        }
        crate::plugins::plugin_impacts(&p.vehicle, &mut **cx.integrations);
        steps::deliver_player_impacts(p, cx.traffic.traffic.as_mut());
        // After scripts: zero-movement `_drag` for a held switch. Running this
        // *before* `tick` cleared Aachen ibox momentary flags (incl. digit 0 /
        // `ibox_taste_D11`) before the frame could act when the click path had
        // not already consumed them (#744).
        if cx.input.dragging {
            let (dx, dy) = std::mem::take(&mut cx.input.drag_delta);
            if let Some((map_id, ref ev)) = cx.input.pressed_scenery_object {
                if let Some(w) = cx.world.world.as_ref() {
                    w.scenery_object_drag(&mut *cx.world.scenery, map_id, ev, dx, dy);
                }
            } else {
                p.drag(dx, dy);
            }
        }
        // (not in the headset: the player's own head moves there, and a head
        // thrown about by the bus on top of it made the whole cab sway and
        // shift before the eyes)
        #[cfg(windows)]
        let vr_on = cx.gfx.xr.vr.is_some();
        #[cfg(not(windows))]
        let vr_on = false;
        p.move_head(dt, cx.common.settings.head_movement && !vr_on);
        // (a head that is doing nothing still breathes and shifts its weight:
        // the sway goes on the head and the view while the bus waits, never
        // into the springs above. Nothing of it while a headset or a real
        // head tracker moves the head - that head is not a still one)
        let idle = if vr_on || (cx.common.settings.head_tracking && cx.input.headtrack.is_some()) { 0.0 } else { cx.common.settings.head_idle };
        // (at a standstill, as the setting says: it fades out over the first
        // few km/h as the bus pulls away and comes back when it stands - it
        // swayed on the road as well, #1325)
        let idle = idle * crate::head_idle::standstill(p.vehicle.physics.velocity_kmh().abs());
        // (a switch under the cursor is a hand reaching for it, and a view that
        // goes on sliding under the pointer is a view that misses what it was
        // reaching for. Held, not reset: the camera stays where it is, which is
        // where any camera is while the player is busy with something)
        let reaching = idle > 0.0 && (cx.menus.menus.hover.is_some() || cx.menus.menus.hover_hand);
        if !cx.view.cam.head_idle_hold.step(dt, reaching) {
            p.move_head_idle(dt, idle, cx.common.settings.head_idle_pace);
        }
        if let Some(w) = cx.world.world.as_ref() {
            crate::rail_drive::frame(p, cx.traffic.traffic.as_ref().map(|t| &t.net), w, &mut *cx.world.scenery, dt);
        }
    } else if cx.input.dragging {
        // paused / no ground: still deliver held-switch `_drag` (was unconditional before)
        let (dx, dy) = std::mem::take(&mut cx.input.drag_delta);
        if let Some((map_id, ref ev)) = cx.input.pressed_scenery_object {
            if let Some(w) = cx.world.world.as_ref() {
                w.scenery_object_drag(&mut *cx.world.scenery, map_id, ev, dx, dy);
            }
        } else {
            p.drag(dx, dy);
        }
    }
    // a script that set the time of day (`(S.S.Time)`) moves the game's clock
    if let Some(t) = p.vehicle.host.time_written.take() {
        *cx.clock.pending_time = Some(t);
    }
    // the situation's further vehicles stand and run their scripts, and the
    // player's bus meets them
    let mut placed_boxes = Vec::new();
    for q in cx.player.placed.iter_mut() {
        // (held where it stands while its tile is not loaded, as the driven bus is:
        // with the player far off the tile goes, and the bus fell through the world
        // and was saved there, #2132)
        if !*cx.clock.paused && cx.world.world.as_ref().is_none_or(|w| ground_under(w, q.vehicle.position)) {
            if let Some(w) = cx.world.world.as_ref() {
                reground(w, &mut q.vehicle);
            }
            q.vehicle.update(dt);
        }
        // (drawn as the frame's last tick leaves it)
        if cx.clock.fixed_tick.view_due() {
            q.sync_transforms(r, scene, false, cx.world.world.as_ref().map_or(2, |w| w.settings.rain_quality()));
        }
        let f = crate::lan::footprint_of(&q.vehicle, [2.5, 11.5, 3.0, 0.0, 0.0, 1.5]);
        placed_boxes.push(omsi_sim::collision::Obb {
            center: glam::DVec2::new(f.x, f.y),
            half: glam::DVec2::new(f.width as f64 * 0.5, f.length as f64 * 0.5),
            heading: (f.heading as f64).to_radians(),
            z0: f.z,
            z1: f.z + 3.0,
            velocity: glam::DVec2::ZERO,
            mass: 12_000.0,
            pole: None,
            id: -1,
        });
    }
    if !cx.player.placed.is_empty() {
        // (the traffic writes the list afresh every frame; without it, this does)
        if cx.traffic.traffic.is_none() {
            p.vehicle.dynamic_boxes.clear();
        }
        p.vehicle.dynamic_boxes.extend(placed_boxes);
    }
    if let Some(w) = cx.world.world.as_ref() {
        lay_down_poles(w, r, scene, &mut p.vehicle);
    }
    static EVERY: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    if let Some(every) = *EVERY.get_or_init(|| {
        omsi_cfg::flags::OMSI_DEBUG_PHYSICS.parse::<f32>()
            .filter(|v| *v > 0.0)
    }) {
        static LAST: std::sync::atomic::AtomicU32 =
            std::sync::atomic::AtomicU32::new(u32::MAX);
        // (the simulation's own time: the same ticks at any frame rate)
        let t = cx.clock.fixed_tick.sim_time().0 as f32;
        let bucket = (t / every) as u32;
        if LAST.swap(bucket, std::sync::atomic::Ordering::Relaxed) != bucket {
            log_physics(&p.vehicle, t);
        }
    }
}

crate::app::stage_ctx!(
    /// The player's bus into the scene: its part of the game.
    PlayerToSceneCx { reads: [view, world, people, ui], writes: [player, gfx, perf] }
);

/// The bus and its driver into the scene.
pub(super) fn player_to_scene(cx: PlayerToSceneCx, dt: f32) {
    let (Some(p), Some(r), Some(scene)) = (
        cx.player.player.as_mut(),
        cx.gfx.renderer.as_ref(),
        cx.gfx.scene.as_mut(),
    ) else {
        return;
    };
    let inside = cx.view.cam.in_cab;
    if let Some(tv) = p.trolley_view.as_mut() {
        tv.update(r, scene, dt, &mut p.vehicle);
    }
    p.panto_fx.update(dt, &mut p.vehicle);
    let glass = p.sync_transforms(r, scene, inside, cx.world.world.as_ref().map_or(2, |w| w.settings.rain_quality()));
    cx.perf.profile.add(crate::profile_keys::ProfKey::PlayerGlass, glass);
    crate::scene::sync_vehicle_damage(r, scene, &mut p.vehicle, &mut p.render);
    // from the driver's seat the figure stays in the mirrors
    // (from the driver's seat only the mirrors show him - while the camera is at his eyes:
    // the photo mode keeps the driver's view while its camera flies off, and taken from
    // there every photo showed the bus without its driver)
    // (out of the seat: nobody at the wheel)
    let at_eyes = *cx.view.view == "driver" && (cx.ui.photo.is_none() || cx.view.camera.as_ref().is_none_or(|c| (c.position - crate::camera_util::driver_eye(p)).length() < 0.5));
    p.sync_driver_hands(r, scene, dt, cx.common.settings.driver && cx.people.on_foot.is_none(), at_eyes, cx.common.settings.hands_in_cab);
}

crate::app::stage_ctx!(
    /// The audio stage's part of the game.
    AudioCx { reads: [world, clock], writes: [player, sound, view, gfx, ui] }
);

/// The field of view without a bus, the sound's listener, the placed vehicles without a
/// bus of one's own, and the bus radio.
pub(super) fn audio(cx: AudioCx, dt: f32) {
    // on foot (or the free camera) without a bus of one's own: the field of view
    // setting and the wheel's zoom, as with one - only the player's frame applied
    // them, so after removing the bus the wheel zoomed nothing (#837)
    if cx.player.player.is_none() && matches!(cx.view.view.as_str(), "free" | "foot") {
        if let Some(cam) = cx.view.camera.as_mut() {
            let base = if cx.common.settings.fov >= 20.0 { cx.common.settings.fov.min(120.0) } else { 60.0 };
            cam.fov_deg = (base * cx.view.cam.view_zoom.get(cx.view.view.as_str()).copied().unwrap_or(1.0)).clamp(8.0, 120.0);
        }
    }
    if let Some(a) = cx.sound.audio.as_ref() {
        a.follow_device();
    }
    if let (Some(a), Some(cam)) = (cx.sound.audio.as_ref(), cx.view.camera.as_ref()) {
        let (reverb_time, reverb_mix) = cx.world.world.as_ref().map(|w| w.reverb_at(cam.position)).unwrap_or((0.0, 0.0));
        a.set_listener(omsi_audio::Listener {
            position: cam.position.as_vec3(),
            forward: cam.forward(),
            right: cam.right(),
            // (silent while paused: the engine's loops would go on)
            // (the settings' volume: it had been 0.6 whatever the slider said)
            master: if *cx.clock.paused { 0.0 } else { cx.common.settings.volume.clamp(0.0, 1.0) },
            reverb_time,
            reverb_mix,
        });
    }
    // on foot without a bus of one's own: the vehicles one placed still stand, run
    // their scripts and are drawn where they are (the player's frame did it)
    if let (None, Some(r), Some(scene)) = (cx.player.player.as_ref(), cx.gfx.renderer.as_ref(), cx.gfx.scene.as_mut()) {
        for q in cx.player.placed.iter_mut() {
            if !*cx.clock.paused && cx.world.world.as_ref().is_none_or(|w| ground_under(w, q.vehicle.position)) {
                if let Some(w) = cx.world.world.as_ref() {
                    reground(w, &mut q.vehicle);
                }
                q.vehicle.update(dt);
            }
            q.sync_transforms(r, scene, false, cx.world.world.as_ref().map_or(2, |w| w.settings.rain_quality()));
        }
    }
    if let Some(a) = cx.sound.audio.as_ref() {
        cx.sound.milezero.update(a, cx.common.settings.milezero, cx.common.settings.volume);
        match cx.player.player.as_mut() {
            Some(p) => {
                let inside = cx.view.cam.in_cab;
                p.trolley_sound.update(a, &mut p.vehicle, &[], *cx.clock.paused);
                cx.sound.radio.set_map(&cx.common.args.root, &cx.common.args.map);
                cx.sound.radio.fx = cx.common.settings.radio_fx;
                if let Some(m) = cx.sound.radio.update(a, &p.vehicle, inside) {
                    *cx.ui.service_msg = Some((m, 6.0));
                }
                // (a radio whose display is a text of its script shows the station)
                p.vehicle.radio_text = cx.sound.radio.display_text();
                p.vehicle.radio_frequency = cx.sound.radio.frequency(p.vehicle.position.x, p.vehicle.position.y);
                (p.vehicle.host.radio_name, p.vehicle.host.radio_song) = cx.sound.radio.now_playing();
            }
            None => cx.sound.radio.stop(a),
        }
    }
}

/// Whether the wheel surfaces of the tile under `at` are loaded (roads, bridges, yards - not
/// the terrain alone): a vehicle there has ground to stand on.
fn ground_under(w: &crate::scene::World, at: glam::DVec3) -> bool {
    let k = ((at.x / omsi_map::tile_size()).floor() as i32, (at.y / omsi_map::tile_size()).floor() as i32);
    w.data.peek().ground.surfaces.contains_key(&k)
}

/// A placed vehicle found under the ground (one saved after it had fallen through a tile that
/// was not loaded, #2132) is put back on the highest ground there, as `admin::guard_fall`
/// does for the driven bus.
fn reground(w: &crate::scene::World, v: &mut omsi_sim::VehicleInstance) {
    let at = v.position;
    if w.data.peek().drive_probe(at.x, at.y, at.z + 1.5).below.is_some() {
        return;
    }
    let Some(g) = w.walk_height(at.x, at.y) else { return };
    if at.z >= g - 8.0 {
        return;
    }
    log::warn!("a placed vehicle lay under the ground at ({:.1}, {:.1}, {:.1}): put back on it at {g:.1}", at.x, at.y, at.z);
    let origin = glam::DVec3::new(at.x, at.y, g);
    v.position = origin;
    if let Some(rb) = v.rigid.as_mut() {
        rb.place(origin, v.heading);
    }
    for t in v.trailers.iter_mut() {
        t.realign();
    }
}
