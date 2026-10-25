//! The AI traffic's step in the window's frame.

use super::*;

crate::app::stage_ctx!(
    /// The AI traffic's part of the game.
    TrafficCx { reads: [view, clock, weather, people, net, sound], writes: [traffic, world, player, gfx, perf] }
);

/// The AI traffic: what it needs to know, the timetable's departures, its step, its
/// sound and its pictures.
pub(super) fn traffic(cx: TrafficCx, dt: f32) {
    // (with a triple screen, all of its three panels are in sight)
    let sight = cx.view.camera.as_ref().zip(cx.gfx.gfx.surface.as_ref())
        .and_then(|(c, s)| crate::app_impl::sight_extent(cx.common.settings, cx.view, c, (s.config.width, s.config.height)));
    if let (Some(t), Some(w), Some(r), Some(scene)) = (
        cx.traffic.traffic.as_mut(),
        cx.world.world.as_ref(),
        cx.gfx.renderer.as_ref(),
        cx.gfx.scene.as_mut(),
    ) {
        let center = cx.player.player
            .as_ref()
            .map(|p| p.vehicle.position)
            .or(cx.view.camera.as_ref().map(|c| c.position))
            .unwrap_or(DVec3::ZERO);
        let aspect = cx.gfx.gfx.surface
            .as_ref()
            .map(|s| s.config.width as f64 / s.config.height.max(1) as f64)
            .unwrap_or(16.0 / 9.0);
        let fog = cx.weather.weather
            .as_ref()
            .map(|w| w.fog.0 as f64)
            .unwrap_or(50000.0);
        traffic_inputs(
            t,
            cx.view.camera.as_ref(),
            aspect,
            sight,
            fog,
            &*cx.clock.clock,
            cx.people.humans.as_ref(),
            cx.player.player.as_ref(),
            &r.options,
        );
        let daylight = omsi_sim::Daylight::compute(&*cx.clock.clock, cx.weather.envir.as_ref());
        steps::set_ai_environment(t, cx.clock.clock, daylight, cx.weather.weather.as_ref(), *cx.weather.wetness, *cx.clock.paused, cx.common.settings.auto_clutch);
        // The population every 2 s and the timetable's departures on their own clock
        // (every 2 s, every 0.1 s while a backlog of departures waits), never both in
        // one frame: each is a heavy frame of its own. (Together on one clock, a backlog
        // ran the whole population ten times a second; the people's own look-round
        // keeps off these frames too, `traffic_heavy_frame`.)
        *cx.traffic.populate_t -= dt;
        let first = *cx.traffic.first_populate;
        let mut heavy = false;
        if *cx.traffic.populate_t <= 0.0 && !*cx.clock.paused {
            *cx.traffic.populate_t = 2.0;
            heavy = true;
            let __t5 = Instant::now();
            let view = cx.view.camera.as_ref().map(|c| c.forward().as_dvec3());
            t.populate_seen(w, center, view);
            view_sync::apply_traffic(t, &mut cx.gfx.gfx.sim_view.traffic, w, r, scene);
            cx.perf.profile.add(crate::profile_keys::ProfKey::TrafficPopulate, __t5.elapsed().as_secs_f64());
        }
        if let Some(s) = cx.traffic.schedule.as_mut() {
            if let Some(blocks) = s.poll_transit() {
                t.sim.set_blocks(blocks);
            }
            s.tick_t -= dt;
            // (the first look goes with the first population: the trips under way are put
            // out at the start)
            if (first || (s.tick_t <= 0.0 && !heavy)) && !*cx.clock.paused {
                s.tick_t = if s.pending() > 0 { 0.1 } else { 2.0 };
                heavy = true;
                steps::set_keep_clear(t, cx.player.player.as_ref(), &cx.net.remotes);
                let __t6 = Instant::now();
                steps::schedule_tick(s, w, t, &mut cx.gfx.gfx.sim_view.traffic, r, scene, first);
                cx.perf.profile.add(crate::profile_keys::ProfKey::TrafficSchedule, __t6.elapsed().as_secs_f64());
            }
        }
        if heavy {
            *cx.traffic.first_populate = false;
            *cx.traffic.traffic_heavy_frame = true;
        }
        // the AI's lights (and a bus's saloon lamps, which its scripts switch with
        // them): by the time of day, and by day in fog, rain, snow or under a
        // closed cloud cover as drivers do
        let gloomy = steps::gloomy_weather(cx.weather.weather.as_ref());
        // Omsi switches the AI's lights on below a light value of 0.75, before
        // the street lamps (0.6), and off after them in the morning
        let daylight = omsi_sim::Daylight::compute(&*cx.clock.clock, cx.weather.envir.as_ref());
        steps::set_ai_daylight(t, daylight, gloomy);
        let __t2 = Instant::now();
        let rail = cx.player.player.as_ref().and_then(|p| p.rail.as_ref()).map(|r| (r.lane, r.along));
        // (OMSI_SIM_THREAD: the step itself is made on the sim thread while the picture is
        // drawn, see `sim_thread`; here the rest of it, on the traffic the last one left)
        let step_dt = if !*cx.clock.paused && cx.traffic.pipeline.takes_traffic(t) {
            t.scripts_due = true;
            cx.traffic.pipeline.owe_traffic(dt, cx.player.player.as_ref().map(crate::traffic_link::player_outline));
            0.0
        } else {
            // (the cars' scripts and animations in the frame's last tick only: their
            // bodies move on in every step, see `TrafficSim::scripts_due`)
            t.scripts_due = cx.clock.fixed_tick.view_due();
            dt
        };
        let capture = cx.net.lan.as_ref().is_some_and(|lan| lan.role == omsi_net::Role::Host);
        for c in &mut t.sim.cars { c.vehicle.host.capture_sound_triggers = capture; }
        t.table_on = capture;
        let signals = steps::traffic_tick(t, w, &mut *cx.world.scenery, step_dt, *cx.clock.paused, cx.player.player.as_ref(), &cx.net.remotes, &cx.player.placed, rail);
        cx.perf.profile.add(crate::profile_keys::ProfKey::TrafficTickSignals, signals);
        cx.perf.profile.add(crate::profile_keys::ProfKey::TrafficTick, __t2.elapsed().as_secs_f64());
        for (k, v) in [crate::profile_keys::ProfKey::TrafficTickLanes, crate::profile_keys::ProfKey::TrafficTickPlan, crate::profile_keys::ProfKey::TrafficTickAi]
            .into_iter()
            .zip(t.frame_split)
        {
            cx.perf.profile.add(k, v);
        }
        // the options' [no_collision_vehToVeh]: the bus drives through the traffic
        steps::traffic_boxes(t, cx.player.player.as_mut(), cx.common.settings.collision_vehicles);
        // (the sounds and the picture only in the frame's last tick: the triggers the cars
        // fire wait in them until then, and only the last tick's state is drawn)
        let view_due = cx.clock.fixed_tick.view_due();
        let __t3 = Instant::now();
        if let Some(a) = cx.sound.audio.as_ref().filter(|_| view_due) {
            let street = cx.weather.weather
                .as_ref()
                .map(|w| street_condition(w, *cx.weather.wetness))
                .unwrap_or(0.0);
            // (a walker in the doorway: muffled by as much as the eyes are in, see `on_foot`)
            let muffled = cx.view.cam.in_cab || cx.sound.foot_in.is_some_and(|w| w > 0.0);
            // heard round the camera (the ear), not round the player's bus: a
            // free camera following an AI bus lost its sound 250 m from the bus
            let ear = cx.view.camera.as_ref().map(|c| c.position).unwrap_or(center);
            // (riding in an AI bus on foot: that bus is heard from inside, #1286)
            let riding = cx.people.on_foot.as_ref().and_then(|f| match f.inside {
                Some((omsi_sim::people::BusId::Ai(id), _)) => Some(id),
                _ => None,
            });
            t.update_audio(a, ear, street, muffled || riding.is_some(), riding, cx.common.settings.vol_ai);
        }
        cx.perf.profile.add(crate::profile_keys::ProfKey::TrafficAudio, __t3.elapsed().as_secs_f64());
        let __t4 = Instant::now();
        t.camera = cx.view.camera.as_ref().map(|c| c.position);
        // the view sync of the traffic: before the player and the people move, as the
        // cars that parked leave the traffic here (see `view_sync`)
        view_sync::sync(ViewSync::traffic(t, &mut *cx.world.scenery).world_only(!view_due), &mut cx.gfx.gfx.sim_view, w, r, scene);
        cx.perf.profile.add(crate::profile_keys::ProfKey::TrafficSync, __t4.elapsed().as_secs_f64());
    }
}
