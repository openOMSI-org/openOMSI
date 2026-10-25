//! The clock, the weather, the lights, the rain and the scenery's scripts in the window's
//! frame.

use super::*;

impl App {
    /// The METAR sync, the clock and the weather; the street lamps and the lit windows by the
    /// daylight, which the bus's scripts are told. The frame's daylight.
    pub(super) fn frame_weather(&mut self, dt: f32) -> omsi_sim::Daylight {
        // (the METAR sync: the report's weather, in real time)
        self.tick_metar(dt);
        self.tick_tomorrow();
        if !self.paused {
            // (the time speed: the settings', or the session's in LAN play)
            let speed = self.time_speed();
            self.clock.advance(dt * speed as f32);
            // (the real-time sync: the device's date and time, whatever the speed was)
            self.sync_real_time();
            if let Some(t) = self.session.traffic.as_mut() {
                t.time_scale = speed;
            }
            self.tick_weather(dt * speed as f32);
        } else if self.session.weather_blend.is_some() {
            // (a preset picked in the paused menu: the change goes over in real time)
            self.tick_weather(dt);
        }
        let daylight = omsi_sim::Daylight::compute(&self.clock, self.session.envir.as_ref());
        // (the avatars' faces yawn late in the evening)
        crate::view_sync::people::set_hour((self.clock.time / 3600.0).rem_euclid(24.0) as f32);
        let lamps = self.session.lamps_on != Some(daylight.lamps_on);
        self.session.lamps_on = Some(daylight.lamps_on);
        // the lit windows of the houses by their [NightMapMode] timetable (once a
        // second: tiles come and go, and the hours pass)
        let night_modes = self.perf.total_frames % 60 == 0;
        if let (Some(w), Some(r), Some(scene)) = (
            self.world.as_ref(),
            self.renderer.as_ref(),
            self.scene.as_mut(),
        ) {
            steps::world_lamps(w, r, scene, &self.clock, &daylight, lamps, night_modes);
        }
        if night_modes {
            self.follow_date();
        }
        if let Some(p) = self.player.as_mut() {
            steps::tell_surroundings(p, self.world.as_deref(), &daylight, self.session.weather.as_ref(), self.session.wetness);
        }
        daylight
    }
}

crate::app::stage_ctx!(
    /// The lights' stage's part of the game: the lamps, the rain and the spray, the sounds of the world around.
    LightsCx { reads: [clock, world, net, view, menus], writes: [perf, sound, weather, people, gfx, traffic, player] }
);

/// The lights (the lamps' cones, the light maps, every light that shines), the rain and
/// the snow, the cabin air, the tyres' spray and the sounds of the street.
pub(super) fn lights(cx: LightsCx, dt: f32, daylight: omsi_sim::Daylight, behind: f32) {
    // the lamps' cones in fog and falling rain or snow, and new light pictures
    let enhanced = cx.gfx.renderer.as_ref().is_some_and(|r| !r.options.no_enhanced);
    let cone = lights::ConeWeather { enhanced, ..cx.weather.weather.as_ref().map(|wt| lights::ConeWeather::of(wt, daylight.night)).unwrap_or_default() };
    if let Some(r) = cx.gfx.renderer.as_mut() {
        lights::upload_corona_textures(r);
    }
    // the tile light maps around the camera, for the roads' night light
    let __ta = Instant::now();
    if let (Some(w), Some(r), Some(cam)) = (cx.world.world.as_ref(), cx.gfx.renderer.as_ref(), cx.view.camera.as_ref()) {
        w.update_light_map_atlas(r, cam.position);
    }
    cx.perf.profile.add(crate::profile_keys::ProfKey::LightsAtlas, __ta.elapsed().as_secs_f64());
    if let (Some(w), Some(scene), Some(cam)) = (
        cx.world.world.as_ref(),
        cx.gfx.scene.as_mut(),
        cx.view.camera.as_ref(),
    ) {
        // (the cars' lamps where their meshes are drawn: between the traffic's last two
        // steps, as the view sync puts them - at the step's own pose a car's glow ran up to a
        // step ahead of its body, in and out of it at speed, and the lamps seemed to flicker)
        if let Some(t) = cx.traffic.traffic.as_mut() {
            t.sim.show_between_at(behind);
        }
        let vehicles = steps::light_vehicles(cx.player.player.as_ref(), cx.traffic.traffic.as_ref(), &cx.net.remotes);
        let __tc = Instant::now();
        lights::collect(
            w, scene, &daylight, cam.position, &vehicles, cone,
            lights::VehicleSmoke::from_setting(&cx.common.settings.vehicle_smoke),
            cx.player.player.is_some(),
        );
        drop(vehicles);
        if let Some(p) = cx.player.player.as_ref() {
            if let Some(tv) = p.trolley_view.as_ref() {
                tv.push_effects(scene, &p.vehicle);
            }
            p.panto_fx.push_effects(scene);
        }
        cx.perf.profile.add(crate::profile_keys::ProfKey::LightsCollect, __tc.elapsed().as_secs_f64());
        // the object editor's pick: a magenta glow over it
        if let Some(id) = cx.menus.menus.editor.as_ref().and_then(|e| e.selected) {
            let at = w.edit_objects.lock().get(&id).map(|o| o.pos);
            let moved = w.object_edits.lock().get(&id).map(|e| e.moved).unwrap_or_default();
            if let Some(p) = at {
                scene.coronas.push(omsi_render::Corona {
                    position: p + moved + glam::DVec3::Z * 3.0,
                    size: 0.6,
                    color: [1.0, 0.1, 0.9],
                    brightness: 2.0,
                    ..Default::default()
                });
            }
        }
        // (set below while there is weather; none without)
        omsi_sim::particles::set_road_water(None);
        if let Some(wt) = &cx.weather.weather {
            let (kind, rate) = precip_of(wt);
            let rain_quality = crate::rain::quality_of(&cx.common.settings.rain_quality);
            w.settings.set_rain_quality(rain_quality);
            cx.weather.rain.set(kind, rate, rain_quality);
            cx.weather.rain.set_temperature(wt.temp.0);
            // [wind] direction (deg) speed (m/s)
            let wind = crate::rain::weather_wind(wt);
            let spray_wind = steps::spray_wind(wt);
            let __tr = Instant::now();
            // (the bus the camera rides with: in it or following it, not a free camera)
            let riding = matches!(cx.view.view.as_str(), "driver" | "pax" | "outside");
            let carried = cx.player.player.as_ref().filter(|_| riding).map(|p| crate::lights::vehicle_velocity(&p.vehicle)).unwrap_or_default();
            cx.weather.rain.tick(if *cx.clock.paused { 0.0 } else { dt }, carried, wind, scene);
            // the player's bus's cabin air and the condensation on its glass
            if let Some(p) = cx.player.player.as_ref() {
                steps::cabin_air_step(&mut cx.weather.cabin_air, if *cx.clock.paused { 0.0 } else { dt }, p, wt, cx.people.humans.as_ref());
            }
            cx.perf.profile.add(crate::profile_keys::ProfKey::LightsRain, __tr.elapsed().as_secs_f64());
            // what every vehicle's tyres throw up from the water on the road: the
            // puddles and the wet asphalt the renderer draws (the same wetness:
            // none under snow, OMSI_WETNESS as the picture takes it)
            let wetness = puddles::road_wetness(*cx.weather.wetness, wt.snow);
            // (the vehicles' own wheel spray goes with the same water: only through puddles -
            // in snow it keeps the scripts' own slush)
            if !(wt.snow || wt.snow_on_road || kind == 2) {
                omsi_sim::particles::set_road_water(Some(puddles::road_water_hook(w, wetness)));
            }
            if (wetness > 0.0 || !cx.weather.spray.is_empty()) && !omsi_cfg::flags::OMSI_NO_SPRAY.is_set() {
                let __ts = Instant::now();
                steps::throw_spray(
                    &mut cx.weather.spray,
                    if *cx.clock.paused { 0.0 } else { dt },
                    cx.player.player.as_ref(),
                    cx.traffic.traffic.as_ref(),
                    &cx.net.remotes,
                    cam.position,
                    spray_wind,
                    w,
                    wetness,
                    rain_quality,
                );
                cx.weather.spray.sprites(cam.position, &mut scene.smoke);
                cx.perf.profile.add(crate::profile_keys::ProfKey::LightsSpray, __ts.elapsed().as_secs_f64());
            }
            // the rain heard in the street and the footsteps on the pavement
            if let (Some(amb), Some(a)) = (cx.sound.ambience.as_mut(), cx.sound.audio.as_ref())
            {
                let steps = cx.people.humans
                    .as_mut()
                    .map(|h| h.take_footfalls())
                    .unwrap_or_default();
                // what the passengers say, where they stand. A clip heard for the first
                // time is read in the background and the line spoken as soon as it is
                // there (read here, in the frame, it stalled the picture); one still not
                // read after two seconds is let go.
                let said = cx.people.humans.as_mut().map(|h| h.take_voice_lines()).unwrap_or_default();
                {
                    let waiting = &mut cx.sound.voices_waiting;
                    waiting.extend(said.into_iter().map(|line| (line, Instant::now())));
                    waiting.retain(|(line, since)| {
                        if !a.clips_ready(std::slice::from_ref(&line.path)) {
                            return since.elapsed() < std::time::Duration::from_secs(2);
                        }
                        if let Some(clip) = a.load_clip(&line.path) {
                            // (an avatar's lips move with it)
                            crate::view_sync::people::speak(line.speaker, &clip);
                            a.play(
                                clip,
                                omsi_audio::mixer::VoiceParams {
                                    gain: 1.0,
                                    pitch: 1.0,
                                    looping: false,
                                    position: Some(line.position.as_vec3()),
                                    doppler: true,
                                    range: 3.0,
                                    lowpass_hz: 0.0,
                                    important: false,
                                },
                            );
                        }
                        false
                    });
                }
                let inside = cx.view.cam.in_cab;
                let __tm = Instant::now();
                // openOMSI's ambience; with its recordings the street's rain is its own
                let ours = if let Some(s) = cx.sound.soundscape.as_mut() {
                    s.enabled = cx.common.settings.ambient;
                    s.volume = cx.common.settings.vol_ambient;
                    let people = cx.people.humans.as_ref().map(|h| h.people.iter().filter(|p| (p.position - cam.position).length() < 40.0).count() as u32).unwrap_or(0);
                    let open = cx.player.player.as_ref().and_then(|p| p.vehicle.var("Snd_OutsideVol")).unwrap_or(0.0);
                    // the flashes the picture showed: their thunder comes at the speed of sound
                    if let Some(r) = cx.gfx.renderer.as_mut() {
                        for (delay, distance, dir) in r.take_strikes() {
                            s.thunder(delay, distance, dir, cam.position);
                        }
                    }
                    s.update(
                        a,
                        crate::soundscape::Moment {
                            world: cx.world.world.as_deref(),
                            weather: Some(wt),
                            clock: &*cx.clock.clock,
                            sun: daylight.altitude_deg,
                            wetness,
                            ear: cam.position,
                            inside,
                            open,
                            people,
                            paused: *cx.clock.paused,
                            dt,
                        },
                    );
                    s.active()
                } else {
                    false
                };
                cx.perf.profile.add(crate::profile_keys::ProfKey::LightsSoundscape, __tm.elapsed().as_secs_f64());
                amb.update(
                    a,
                    dt,
                    if ours { (0, 0.0) } else { (kind, rate) },
                    inside,
                    street_condition(wt, *cx.weather.wetness),
                    cam.position,
                    &steps,
                );
                cx.perf.profile.add(crate::profile_keys::ProfKey::LightsAmbience, __tm.elapsed().as_secs_f64());
                if let Some(every) = debug_sound_every() {
                    static LAST: std::sync::atomic::AtomicU32 =
                        std::sync::atomic::AtomicU32::new(u32::MAX);
                    let bucket = (cx.clock.clock.time / every as f64) as u32;
                    if LAST.swap(bucket, std::sync::atomic::Ordering::Relaxed) != bucket
                    {
                        log::info!("sound: environment - {} (precip {kind} {rate:.2}, StreetCond {:.2}, {} voices)", amb.last, street_condition(wt, *cx.weather.wetness), a.voice_count());
                        if let Some(sc) = cx.sound.soundscape.as_mut() {
                            let heard = sc.take_heard();
                            log::info!("sound: ambience - {}; heard {heard}", sc.last);
                        }
                    }
                }
            }
        }
        if let Some(t) = cx.traffic.traffic.as_mut() {
            t.sim.show_actual();
        }
    }
}

crate::app::stage_ctx!(
    /// The scripted stage's part of the game: the boards, the route arrows, the scenery's scripts.
    ScriptedCx { reads: [clock, duty, view, sound], writes: [world, traffic, player, gfx, perf] }
);

/// The departure boards, the map's route arrows and the scenery's scripts.
pub(super) fn scripted(cx: ScriptedCx, dt: f32, daylight: omsi_sim::Daylight) {
    if let (Some(w), Some(r), Some(scene), Some(cam)) = (
        cx.world.world.as_ref(),
        cx.gfx.renderer.as_ref(),
        cx.gfx.scene.as_mut(),
        cx.view.camera.as_ref(),
    ) {
        let traffic = cx.traffic.traffic.as_ref();
        let phase = |c: usize, li: usize| {
            traffic.map(|t| t.light_vars(c, li)).unwrap_or((omsi_sim::traffic::UNLINKED_PHASE as f32, 0.0))
        };
        let __tb = Instant::now();
        if let Some(p) = cx.player.player.as_mut() {
            w.sync_html_departures(&mut p.vehicle.host);
        }
        steps::departure_boards(
            cx.traffic.schedule.as_mut(),
            w,
            traffic,
            cx.duty.duty.as_ref(),
            &*cx.clock.clock,
        );
        cx.perf.profile.add(crate::profile_keys::ProfKey::ScriptedBoards, __tb.elapsed().as_secs_f64());
        // the map's own route arrows, with OMSI 2's route arrows
        w.show_help_arrows(r, scene, cx.common.settings.nav_arrows);
        let mut times = omsi_scene::ScriptedTimes::default();
        // (OMSI_SIM_THREAD: what the scripts made during the last picture is shown now,
        // and their next step is made with this one's, see `sim_thread`)
        if cx.traffic.pipeline.take_scenery_stepped() {
            w.scenery_apply(cx.world.scenery.outputs(), r, scene, dt, cam.position, cx.sound.audio.as_ref(), cx.view.cam.in_cab, &mut times);
        }
        if cx.traffic.pipeline.takes_scenery(traffic.map(|t| &t.sim)) {
            let inputs = w.scenery_prepare(&mut *cx.world.scenery, cam.position, daylight.brightness, &mut times);
            cx.traffic.pipeline.owe_scenery(move |s, phase_of| {
                s.step(dt, &inputs.as_inputs(phase_of), &mut omsi_sim::scenery::StepTimes::default());
            });
            // (the lamps' scripts stay here: their sounds every frame, as update_scripted plays them)
            w.lamp_sounds(&mut cx.world.scenery.lamps, cam.position, cx.sound.audio.as_ref(), cx.view.cam.in_cab, &mut times);
        } else {
            w.update_scripted(
                &mut *cx.world.scenery,
                r,
                scene,
                dt,
                cam.position,
                daylight.brightness,
                &phase,
                cx.sound.audio.as_ref(),
                cx.view.cam.in_cab,
                &mut times,
            );
        }
        if omsi_cfg::flags::OMSI_PROFILE.is_set() {
            use crate::profile_keys::ProfKey as K;
            for (k, v) in [
                (K::ScriptedLock, times.lock),
                (K::ScriptedInputs, times.inputs),
                (K::ScriptedScripts, times.scripts),
                (K::ScriptedApply, times.apply),
                (K::ScriptedTextures, times.textures),
                (K::ScriptedLamps, times.lamps),
            ] {
                cx.perf.profile.add(k, v);
            }
        }
    }
}
