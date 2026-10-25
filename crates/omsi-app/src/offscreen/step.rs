//! The stages of the offscreen run's step (a tick of the fixed step, 1/60 s; their order is
//! `stages::OFFSCREEN_STEP`): the server's work, the traffic, the player's bus, the people, the
//! LAN, the spray and the snapshots due.

use super::*;
use crate::view_sync::{self, ViewSync};

impl Offscreen<'_> {
    /// One step, the `i`-th (false: a server was told to stop).
    pub(super) fn step(&mut self, i: usize) -> Result<bool> {
        let mut step = stages::Step { run: self, i };
        Ok(crate::frame_stages::run(&mut step, stages::OFFSCREEN_STEP)? == crate::frame_stages::Flow::Go)
    }

    /// The ground under the vehicles (`OMSI_GROUND_GAP`).
    pub(super) fn ground_gap_step(&mut self, t_s: f32) {
        if let Some(g) = self.ground_gap.as_mut() {
            g.frame(&self.world.data, t_s, self.player.as_ref().map(|p| &p.vehicle), self.traffic.as_deref());
        }
    }

    /// The clock of this moment (`t_s` into the run; a dedicated server's own), and what the
    /// bus's scripts are told of it, as the window's `frame_weather` has them.
    pub(super) fn clock_step(&mut self, t_s: f32) {
        let mut clock = start_clock(self.args);
        if self.server {
            clock.advance((self.srv_clock + self.srv_admin.shift) as f32);
        } else {
            clock.advance(t_s + self.service_seconds as f32);
        }
        self.run_clock = clock;
        let daylight = omsi_sim::Daylight::compute(&self.run_clock, self.envir.as_ref());
        if let Some(p) = self.player.as_mut() {
            steps::tell_surroundings(p, Some(&self.world), &daylight, Some(&self.weather), self.wetness);
        }
    }

    /// A dedicated server's clock, its METAR weather, its administration and its status
    /// (false: it was told to stop).
    pub(super) fn server_step(&mut self, i: usize) -> bool {
        let (args, dt) = (self.args, self.dt);
        let Self {
            ref mut srv_clock,
            ref mut srv_admin,
            ref srv_metar,
            ref mut srv_metar_due,
            ref mut srv_metar_rx,
            ref mut srv_weather_name,
            ref mut lan_off,
            ref remotes_off,
            ref mut traffic,
            ref world,
            ref renderer,
            ref mut scene,
            ref mut sim_view,
            ref humans_off,
            ref mut real_time,
            ref settings,
            ..
        } = *self;
        *srv_clock += dt as f64 * lan_off.as_ref().map(|l| l.clock_speed).unwrap_or(1.0);
        // a server on the real time (server.cfg): its clock reads this machine's
        if i.is_multiple_of(super::steps_in(1.0)) && crate::real_time::server_real() {
            if let Some(n) = crate::real_time::now() {
                let have = (parse_time(&args.time) + *srv_clock + srv_admin.shift).rem_euclid(86400.0);
                let off = (n.secs - have + 43_200.0).rem_euclid(86_400.0) - 43_200.0;
                if off.abs() > 0.5 {
                    srv_admin.shift += off;
                }
            }
        }
        if let (Some(icao), Some(l)) = (srv_metar.as_ref(), lan_off.as_mut()) {
            if srv_metar_rx.is_some() {
                let got = srv_metar_rx.as_ref().map(|rx| rx.try_recv());
                match got {
                    Some(Ok(report)) => {
                        *srv_metar_rx = None;
                        // (a failed download is tried again in a minute)
                        let wait = if report.is_some() { 600 } else { 60 };
                        *srv_metar_due = std::time::Instant::now() + std::time::Duration::from_secs(wait);
                        if let Some(w) = report {
                            if let Some(wire) = crate::weather_setup::report_wire(&w) {
                                if wire != l.weather() {
                                    log::info!("server: weather now the METAR report of {icao}: {wire}");
                                    l.set_weather(&wire);
                                }
                                *srv_weather_name = w.name.clone();
                            }
                        }
                    }
                    Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                        *srv_metar_rx = None;
                        *srv_metar_due = std::time::Instant::now() + std::time::Duration::from_secs(60);
                    }
                    _ => {}
                }
            } else if std::time::Instant::now() >= *srv_metar_due {
                let (tx, rx) = std::sync::mpsc::channel();
                *srv_metar_rx = Some(rx);
                let icao = icao.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::weather_setup::try_metar(&icao));
                });
            }
        }
        if let Some(l) = lan_off.as_mut() {
            let positions = |id: u32| remotes_off.remotes.get(&id).map(|r| (r.vehicle().position, r.vehicle().heading));
            srv_admin.prune(l);
            omsi_lan::bans::sync_host(l);
            // the administration, whatever way it came (see `omsi_lan::admin::exec`): the
            // players' games, a tool on this machine (POST /admin, checked by the gateway: an
            // admin of its own), the web panel's requests
            let hub = omsi_lan::admin::hub().map(|h| &**h);
            let rules = crate::admin::server_rules(srv_metar.is_some());
            for (from, text) in l.take_commands() {
                omsi_lan::admin::server_command(l, from, &text, srv_admin, &positions, &rules, hub);
            }
            for text in crate::lan::take_local_admin() {
                srv_admin.admins.insert(crate::admin::LOCAL_ADMIN, omsi_admin::Role::Admin);
                omsi_lan::admin::server_command(l, crate::admin::LOCAL_ADMIN, &format!("admin {text}"), srv_admin, &positions, &rules, hub);
            }
            if let Some(h) = hub {
                for job in h.take(16) {
                    omsi_lan::admin::run_job(l, srv_admin, job, &positions, &rules, h);
                }
            }
            // and their part on the server's own world
            let jobs = srv_admin.take_world();
            if !jobs.is_empty() {
                let resync = |t: &mut crate::traffic::Traffic| view_sync::apply_traffic(t, &mut sim_view.traffic, world, renderer, scene);
                let mut cars = traffic.as_mut().map(|t| AdminTraffic(t, resync));
                let mut w = omsi_lan::admin::world::ServerWorld { lan: l, shift: &mut srv_admin.shift, passenger_density: &mut srv_admin.passenger_density, clock: parse_time(&args.time) + *srv_clock, traffic: cars.as_mut().map(|c| c as &mut dyn omsi_lan::admin::world::ServerTraffic), traffic_max: crate::game_lists::TRAFFIC_MAX };
                for job in jobs {
                    let out = omsi_lan::admin::world::apply(&mut w, &job.action);
                    omsi_lan::admin::report(w.lan, hub, &job.actor, job.record, &out);
                }
            }
        }
        if quit::requested().is_some() {
            log::info!("server: stopping");
            return false;
        }
        if i.is_multiple_of(super::steps_in(1.0)) {
            if let Some(l) = lan_off.as_mut() {
                crate::server::enforce_vehicles(l);
                crate::server::tick_status(l, parse_time(&args.time) + *srv_clock + srv_admin.shift, srv_weather_name.as_str());
                // the shared world, counted for GET /status
                let (cars, buses, dormant, parked) = traffic.as_ref().map(|t| t.counts()).unwrap_or_default();
                let (walking, waiting, aboard) = humans_off.as_ref().map(|h| h.counts()).unwrap_or_default();
                let target = traffic.as_ref().map(|t| t.target).unwrap_or(0);
                let (passengers, passenger_density) = (humans_off.is_some(), srv_admin.passenger_density.unwrap_or(settings.pax_density));
                crate::lan::update_server_world(omsi_net::ws::WorldCounts { cars, buses, dormant, parked, walking, waiting, aboard, traffic: target, passengers, passenger_density });
                // and for the admins' panels
                if let Some(h) = omsi_lan::admin::hub() {
                    let time = parse_time(&args.time) + *srv_clock + srv_admin.shift;
                    let state = crate::server::admin_state(time, l.clock_speed, srv_weather_name, srv_metar.as_deref(), omsi_net::ws::WorldCounts { cars, buses, dormant, parked, walking, waiting, aboard, traffic: target, passengers, passenger_density });
                    omsi_lan::admin::publish(h, l, srv_admin, state);
                }
            }
        }
        if lan_off.is_none() {
            std::thread::sleep(real_time.wait(Instant::now(), dt));
        }
        true
    }

    /// The AI traffic's step (and with it the timetable's departures).
    pub(super) fn traffic_step(&mut self, i: usize, t_s: f32) -> Result<()> {
        let (args, out, w, h, dt, center) = (self.args, self.out, self.w, self.h, self.dt, self.center);
        let Self {
            ref mut traffic,
            ref mut schedule,
            ref mut player,
            ref camera,
            ref settings,
            ref world,
            ref mut scenery,
            ref mut renderer,
            ref mut scene,
            ref mut sim_view,
            ref humans_off,
            ref remotes_off,
            ref run_clock,
            ref envir,
            ref weather,
            ref wetness,
            ref sky,
            ..
        } = *self;
        if let Some(t) = traffic.as_mut() {
            // the camera the snapshots are taken with is the player's eye for the traffic
            let view_cam = eye_camera(args, Some(&*t), player.as_ref(), camera);
            traffic_inputs(
                t,
                Some(&view_cam),
                w as f64 / h.max(1) as f64,
                triple_extent(settings, &view_cam, w, h),
                weather.fog.0 as f64,
                run_clock,
                humans_off.as_ref(),
                player.as_ref(),
                &renderer.options,
            );
            let daylight = omsi_sim::Daylight::compute(run_clock, envir.as_ref());
            steps::set_ai_environment(t, run_clock, daylight, Some(weather), *wetness, false, settings.auto_clutch);
            if i.is_multiple_of(super::steps_in(2.0)) {
                // (following a car, the population goes with the camera)
                let pc = if args.follow.is_some() { view_cam.position } else { center };
                t.populate(world, pc);
                view_sync::apply_traffic(t, &mut sim_view.traffic, world, renderer, scene);
                // (what the window does every frame: cars that parked become parked objects,
                // released vehicles go back to the world)
                view_sync::sync(ViewSync::traffic(t, scenery), sim_view, world, renderer, scene);
                // `OMSI_POPULATION_SHOTS=1` (with OMSI_DEBUG_POPULATION): a picture from the
                // viewer whenever a car was put inside its frustum (behind something), with
                // where on the picture it stands - to see that it really is hidden
                let framed = std::mem::take(&mut t.framed_spawns);
                if !framed.is_empty() && omsi_cfg::flags::OMSI_POPULATION_SHOTS.is_set() {
                    view_sync::sync(ViewSync::traffic(t, scenery), sim_view, world, renderer, scene);
                    if let Some(p) = player.as_mut() {
                        pose_player(p, renderer, scene, args, settings);
                    }
                    let daylight = omsi_sim::Daylight::compute(run_clock, envir.as_ref());
                    let lighting = weather_lighting(
                        &daylight,
                        weather,
                        cloud_drift_at(weather, run_clock.time),
                        0.0,
                        0.0,
                        settings.shadows,
                        sky,
                    );
                    let pixels =
                        renderer.render_to_image(scene, w, h, &view_cam, &lighting)?;
                    let path = out.with_file_name(format!(
                        "{}_spawn_{t_s:.0}.png",
                        out.file_stem().and_then(|s| s.to_str()).unwrap_or("snap")
                    ));
                    image::save_buffer(&path, &pixels, w, h, image::ColorType::Rgba8)?;
                    let f = view_cam.forward();
                    let r = view_cam.right();
                    let u = r.cross(f);
                    let tan_y = (view_cam.fov_deg * 0.5).to_radians().tan();
                    let tan_x = tan_y * w as f32 / h as f32;
                    for (id, pos) in framed {
                        let rel =
                            (pos - view_cam.position).as_vec3() + glam::Vec3::new(0.0, 0.0, 0.8);
                        let z = rel.dot(f);
                        let px = w as f32 * 0.5 * (1.0 + rel.dot(r) / (z * tan_x));
                        let py = h as f32 * 0.5 * (1.0 - rel.dot(u) / (z * tan_y));
                        log::info!("population shot {}: car {id} appeared at pixel ({px:.0}, {py:.0}), {z:.0} m ahead", path.display());
                    }
                }
            }
            if let Some(s) = schedule.as_mut() {
                // at start, pick up trips that left within the last 20 minutes (a few per
                // frame until they are all out, as the window does)
                if i.is_multiple_of(super::steps_in(2.0)) || s.pending() > 0 {
                    // no timetable vehicle is put into the player's bus or a LAN player's
                    steps::set_keep_clear(t, player.as_ref(), remotes_off);
                    steps::schedule_tick(s, world, t, &mut sim_view.traffic, renderer, scene, i == 0);
                }
            }
            // the AI's lights by the time of day, and by day in gloomy weather
            let daylight = omsi_sim::Daylight::compute(run_clock, envir.as_ref());
            steps::set_ai_daylight(t, daylight, steps::gloomy_weather(Some(weather)));
            let rail = player.as_ref().and_then(|p| p.rail.as_ref()).map(|r| (r.lane, r.along));
            let capture = self.lan_off.as_ref().is_some_and(|lan| lan.role == omsi_net::Role::Host);
            for c in &mut t.sim.cars { c.vehicle.host.capture_sound_triggers = capture; }
            t.table_on = capture;
            steps::traffic_tick(t, world, scenery, dt, false, player.as_ref(), remotes_off, &[], rail);
            steps::traffic_boxes(t, player.as_mut(), settings.collision_vehicles);
        }
        Ok(())
    }

    /// The player's bus starting up.
    pub(super) fn startup_step(&mut self) {
        let dt = self.dt;
        if let Some(player) = self.player.as_mut() {
            player.tick_startup(dt);
        }
    }

    /// The player's duty and the personnel file.
    pub(super) fn duty_step(&mut self) {
        let (args, dt) = (self.args, self.dt);
        let announce = self.settings.auto_announce;
        let Self {
            ref mut player,
            ref mut duty,
            ref world,
            ref mut career,
            ref mut journey,
            ref humans_off,
            ref run_clock,
            ..
        } = *self;
        let Some(player) = player.as_mut() else { return };
        if let Some(d) = duty.as_mut() {
            // (no plugins in an offscreen run: what they would be told goes to the log)
            let mut events = Vec::new();
            steps::duty_step(
                d,
                player,
                world,
                career,
                journey,
                &args.root,
                run_clock.time,
                Some(run_clock),
                true,
                Some(&mut events),
                announce,
            );
            for e in events {
                log::info!("plugin event: {e:?}");
            }
        }
        let riders = humans_off.as_ref().map(|h| h.riding()).unwrap_or(0);
        let crash = steps::career_step(career, player, riders, duty.is_some(), dt, true);
        if crash > 0.0 {
            log::warn!("crash: {:.0} kJ", crash / 1000.0);
        }
        for j in career.take_jolts() {
            log::info!("plugin event: jolt {j:?}");
        }
    }

    /// The `--drive` test's step while it runs.
    pub(super) fn drive_stage(&mut self, i: usize) {
        if self.player.is_some() && i < self.drive_frames {
            self.drive_step(i, i as f32 * self.dt);
        }
    }

    /// One step of `--drive`: the pedals and the wheel as the test has them, the bus's step,
    /// and what the run logs of it.
    fn drive_step(&mut self, i: usize, t_s: f32) {
        let (dt, drive_v0) = (self.dt, self.drive_v0);
        let mut controls = self.drive_controls(t_s);
        let Some(player) = self.player.as_mut() else { return };
        player.tick_auto_shift(dt, controls.throttle, controls.brake);
        player.auto_clutch_bite(controls.throttle);
        controls.clutch = controls.clutch.max(player.axes.clutch);
        player.vehicle.set_controls(controls);
        if i == 0 {
            if let Some(v0) = drive_v0 {
                player.vehicle.set_speed(v0 / 3.6);
            }
        }
        if let Some(spec) = omsi_cfg::flags::OMSI_TROLLEY_HANDS.var() {
            crate::trolley_hands::scripted(&mut player.vehicle, spec, t_s as f64);
        }
        player.vehicle.update(dt);
        if let Some(tv) = player.trolley_view.as_mut() {
            tv.update(&self.renderer, &mut self.scene, dt, &mut player.vehicle);
        }
        player.panto_fx.update(dt, &mut player.vehicle);
        let (vpos, vkmh) = (player.vehicle.position, player.vehicle.physics.velocity_kmh());
        if let Some(g) = player.vehicle.trolley.as_deref_mut() {
            for n in g.rig.take_notes() {
                log::info!("trolley t={t_s:.2}: {n}");
            }
            if let Some(every) = omsi_cfg::flags::OMSI_DEBUG_TROLLEY.parse::<f32>().filter(|e| *e > 0.0) {
                if i.is_multiple_of(super::steps_in(every as f64)) {
                    let poles: Vec<String> = g.rig.poles.iter().map(|p| format!("{:?}/{:.1}deg/{:.0}N{}", p.mode, p.theta.to_degrees(), p.force, if p.closed { "" } else { "/open" })).collect();
                    log::info!(
                        "trolley t={t_s:.2}: at ({:.1}, {:.1}) {:.1} km/h, {:.0} V {:.0} A, notch {}/{}, force {:.0} N, brake {:.0} N, circuit {}, poles [{}], battery {:.2}",
                        vpos.x, vpos.y, vkmh, g.rig.volts, g.rig.amps, g.rig.out.notch, g.rig.out.notches, g.rig.out.wheel_force, g.rig.out.mech_brake, g.rig.circuit, poles.join(", "), g.rig.out.battery_soc
                    );
                }
            }
        }
        steps::deliver_player_impacts(player, self.traffic.as_mut());
        self.drive_diagnostics(i, t_s);
    }

    /// The `--drive` test's pedals and wheel at `t_s`: the timed triggers, `--setvar`'s
    /// steering and braking, `OMSI_DRIVE_PROFILE`, `OMSI_AUTOPILOT`.
    fn drive_controls(&mut self, t_s: f32) -> omsi_sim::Controls {
        let (args, dt) = (self.args, self.dt);
        let Self {
            ref mut player,
            ref timed,
            ref drive_profile,
            ref traffic,
            ..
        } = *self;
        let Some(player) = player.as_mut() else { return omsi_sim::Controls::default() };
        for (name, at) in timed {
            if *at > t_s - dt && *at <= t_s {
                player.fire_timed(name);
            }
        }
        player.axes.clutch = (player.axes.clutch - 0.7 * dt).max(0.0);
        // --drive test profile: full throttle, steering from --setvar drive_steer, brake after drive_brake_at
        let steer = args
            .setvar
            .as_ref()
            .and_then(|s| {
                s.split(',').find_map(|kv| {
                    kv.strip_prefix("drive_steer=")
                        .and_then(|v| v.parse::<f32>().ok())
                })
            })
            .unwrap_or(0.0);
        let brake_at = args
            .setvar
            .as_ref()
            .and_then(|s| {
                s.split(',').find_map(|kv| {
                    kv.strip_prefix("drive_brake_at=")
                        .and_then(|v| v.parse::<f32>().ok())
                })
            })
            .unwrap_or(f32::MAX);
        let throttle_from = args
            .setvar
            .as_ref()
            .and_then(|s| {
                s.split(',').find_map(|kv| {
                    kv.strip_prefix("drive_throttle_from=")
                        .and_then(|v| v.parse::<f32>().ok())
                })
            })
            .unwrap_or(0.0);
        // `drive_brake_until=S`: the foot on the brake until S, as a driver holds it
        // to select a gear (the Citaro's ZF/Voith D button wants the brake pressed)
        let brake_until = args
            .setvar
            .as_ref()
            .and_then(|s| {
                s.split(',').find_map(|kv| {
                    kv.strip_prefix("drive_brake_until=")
                        .and_then(|v| v.parse::<f32>().ok())
                })
            })
            .unwrap_or(0.0);
        let braking = t_s >= brake_at || t_s < brake_until;
        let throttle = if braking || t_s < throttle_from {
            0.0
        } else {
            1.0
        };
        let mut controls = omsi_sim::Controls {
            throttle,
            brake: if braking { 1.0 } else { 0.0 },
            steering: steer,
            ..Default::default()
        };
        if let Some(step) = drive_profile.iter().rev().find(|s| s[0] <= t_s) {
            controls = omsi_sim::Controls {
                throttle: step[1],
                brake: step[2],
                steering: step[3],
                ..Default::default()
            };
        }
        // OMSI_AUTOPILOT=<km/h>: the player's bus follows the road network's lanes at
        // that speed (a steering wheel on a pure-pursuit point 12 m ahead, a throttle
        // and brake on the speed) - to drive it round a map's roundabouts and bends
        // and see where it falls through or leaves the road; each lane taken is the
        // straightest on
        if let (Some(kmh), Some(net)) = (omsi_cfg::flags::OMSI_AUTOPILOT.parse::<f32>(), traffic.as_ref().map(|t| &t.net)) {
            let v = &player.vehicle;
            let h = v.heading.to_radians();
            let fwd = DVec3::new(h.sin(), h.cos(), 0.0);
            let probe = v.position + fwd * 3.0;
            if let Some((mut lane, mut s, _)) = net.nearest_lane(probe, omsi_sim::traffic::LaneKind::Street) {
                // (the lane that runs our way)
                let lh = net.lanes[lane].at(s).1 as f64;
                let dh = (lh - v.heading + 540.0).rem_euclid(360.0) - 180.0;
                if dh.abs() > 100.0 {
                    if let Some((l2, s2, _)) = (0..net.lanes.len()).filter(|&k| net.lanes[k].kind == omsi_sim::traffic::LaneKind::Street).filter_map(|k| net.lanes[k].nearest_point(probe).map(|(s, d)| (k, s, d))).filter(|(k, s, d)| *d < 6.0 && ((net.lanes[*k].at(*s).1 as f64 - v.heading + 540.0).rem_euclid(360.0) - 180.0).abs() < 80.0).min_by(|a, b| a.2.total_cmp(&b.2)) {
                        lane = l2;
                        s = s2;
                    }
                }
                let mut ahead = 12.0f32;
                loop {
                    let len = net.lanes[lane].length();
                    if s + ahead <= len || net.lanes[lane].next.is_empty() {
                        s = (s + ahead).min(len);
                        break;
                    }
                    ahead -= len - s;
                    let here = net.lanes[lane].at(len).1;
                    lane = *net.lanes[lane].next.iter().min_by(|a, b| {
                        let da = (net.lanes[**a].at(0.0).1 - here + 540.0).rem_euclid(360.0) - 180.0;
                        let db = (net.lanes[**b].at(0.0).1 - here + 540.0).rem_euclid(360.0) - 180.0;
                        da.abs().total_cmp(&db.abs())
                    }).unwrap();
                    s = 0.0;
                }
                let target = net.lanes[lane].at(s).0;
                let d = (target - v.position).truncate();
                let want = d.x.atan2(d.y).to_degrees();
                let alpha = ((want - v.heading + 540.0).rem_euclid(360.0) - 180.0) as f32;
                let speed = v.physics.velocity_kmh();
                controls.steering = (alpha / 30.0).clamp(-1.0, 1.0);
                // (with an OMSI_DRIVE_PROFILE the pedals are the profile's: the autopilot only steers)
                if drive_profile.is_empty() {
                    controls.throttle = ((kmh - speed) / 10.0).clamp(0.0, 1.0);
                    controls.brake = ((speed - kmh - 3.0) / 10.0).clamp(0.0, 1.0);
                }
            }
        }
        controls
    }

    /// What the run logs of the bus as it drives (`OMSI_DEBUG_VARS`, `OMSI_SUSP_TRACE`,
    /// `OMSI_WHEEL_TRACE`, the HUD's reasons, the drive's extremes, `OMSI_DEBUG_PHYSICS`,
    /// `OMSI_TRACE_VARS`), the joint held, the rail line, the poles laid down.
    fn drive_diagnostics(&mut self, i: usize, t_s: f32) {
        let (dt, physics_log) = (self.dt, self.physics_log);
        let Self {
            ref mut player,
            ref world,
            ref mut scenery,
            ref traffic,
            ref settings,
            ref snapshot_times,
            ref renderer,
            ref mut scene,
            ref mut wheel_worst,
            ref mut last_reasons,
            ..
        } = *self;
        let Some(player) = player.as_mut() else { return };
        // OMSI_DEBUG_VARS with OMSI_DEBUG_VARS_EVERY=<s>: the variables through the drive
        if let (Some(list), Some(every)) = (omsi_cfg::flags::OMSI_DEBUG_VARS.var(), omsi_cfg::flags::OMSI_DEBUG_VARS_EVERY.parse::<f32>()) {
            if (t_s / every).floor() != ((t_s - dt) / every).floor() {
                let vals: Vec<String> = list.split(',').map(str::trim).map(|v| format!("{v}={:.2}", player.vehicle.var(v).unwrap_or(f32::NAN))).collect();
                log::info!("t={t_s:.1}: {}", vals.join(" "));
            }
        }
        // OMSI_JOINT_ANGLE=degrees: the rear section held at that angle to the front
        // one (the joint and its bellows seen bent, without driving a curve)
        if let Some(a) = omsi_cfg::flags::OMSI_JOINT_ANGLE.var().and_then(|v| v.trim().parse::<f64>().ok()) {
            let v = &mut player.vehicle;
            let (pos, rot, heading) = (v.position, v.body_rotation(), v.heading);
            if let Some(t) = v.trailers.first_mut() {
                let c = t.coupling_point(pos, rot);
                let h = (heading + a).to_radians();
                t.place_pivot(c - DVec3::new(h.sin(), h.cos(), 0.0) * t.pivot_length() as f64);
            }
        }
        crate::rail_drive::frame(player, traffic.as_ref().map(|t| &t.net), world, scenery, dt);
        // (the autopilot's log: where the bus is against the ground under it, twice a
        // second, and at once when the ground is not under it any more)
        if omsi_cfg::flags::OMSI_AUTOPILOT.is_set() {
            let at = player.vehicle.position;
            let under = world.data.peek().drive_probe(at.x, at.y, at.z + 1.5).below;
            let lost = under.is_none_or(|g| at.z < g - 0.6);
            if i.is_multiple_of(super::steps_in(0.5)) || lost {
                log::info!("autopilot t={t_s:.1} at ({:.1}, {:.1}, {:.2}) heading {:.0} {:.0} km/h, ground under {:?}{}", at.x, at.y, at.z, player.vehicle.heading, player.vehicle.physics.velocity_kmh(), under.map(|g| (g * 100.0).round() / 100.0), if lost { " FELL" } else { "" });
            }
        }
        // the driver's hands follow the wheel frame by frame (as in the window), so
        // that the snapshots show them where the hand-over-hand has got to
        if settings.driver && !snapshot_times.is_empty() {
            player.sync_driver(renderer, scene, dt, true, false);
        }
        // OMSI_SUSP_TRACE=<csv>: every frame, the body's height and vertical speed and
        // each wheel's travel, load and the ground under it (bumps and hops)
        if let Some(path) = omsi_cfg::flags::OMSI_SUSP_TRACE.var() {
            use std::io::Write;
            static TRACE: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);
            let mut f = TRACE.lock().unwrap_or_else(|e| e.into_inner());
            if f.is_none() {
                *f = std::fs::File::create(&path).ok();
                if let Some(f) = f.as_mut() {
                    let _ = writeln!(f, "t,x,y,z,vz,kmh,wheel,compression,rate,load,ground_z,on_ground");
                }
            }
            if let (Some(f), Some(rb)) = (f.as_mut(), player.vehicle.rigid.as_ref()) {
                for (k, w) in rb.wheels.iter().enumerate() {
                    let _ = writeln!(
                        f,
                        "{:.4},{:.2},{:.2},{:.4},{:.3},{:.1},{k},{:.4},{:.3},{:.0},{:.4},{}",
                        t_s, rb.position.x, rb.position.y, rb.position.z, rb.velocity.z,
                        player.vehicle.physics.velocity_kmh(), w.compression, w.compression_rate, w.load, w.ground_z, w.on_ground as u8
                    );
                }
            }
        }
        // OMSI_WHEEL_TRACE: the deepest a drawn tyre goes into the road (or floats
        // over it), once a second while driving
        if omsi_cfg::flags::OMSI_WHEEL_TRACE.is_set() {
            let lows = tyre_lows(&player.vehicle, world);
            if let Some((p, d)) = lows.iter().min_by(|a, b| a.1.total_cmp(&b.1)) {
                *wheel_worst = match *wheel_worst {
                    Some((_, w)) if w <= *d => *wheel_worst,
                    _ => Some((*p, *d)),
                };
            }
            if (t_s % 1.0) < dt {
                if let Some((p, d)) = wheel_worst.take() {
                    log::info!("wheel trace t={t_s:.0}: deepest tyre {d:+.3} m at ({:.1}, {:.1}, {:.2}), {:.0} km/h", p.x, p.y, p.z, player.vehicle.physics.velocity_kmh());
                }
            }
        }
        // what the window's HUD would say about a bus that does not move
        // (once per reason: the numbers in a line change all the time)
        if i.is_multiple_of(super::steps_in(1.0)) {
            let why = standing_reasons(&player.vehicle, &|a| crate::diagnostics::rebound_key(&player.bindings, a));
            let key = |l: &String| l.split('(').next().unwrap_or_default().to_string();
            for line in why
                .iter()
                .filter(|l| !last_reasons.iter().any(|o| key(o) == key(l)))
            {
                log::info!("HUD at {t_s:.1} s (not moving): {line}");
            }
            *last_reasons = why;
        }
        lay_down_poles(world, renderer, scene, &mut player.vehicle);
        // the worst the bus did on the way (a bus on end, flying, through the ground)
        {
            let v = &player.vehicle;
            let g = world.ground_height(v.position.x, v.position.y).map(|g| v.position.z - g).unwrap_or(0.0);
            let e = &mut self.drive_extremes;
            if v.pitch.abs() > e.0.abs() {
                e.0 = v.pitch;
                e.4 = (v.position, t_s);
            }
            if v.bank.abs() > e.1.abs() {
                e.1 = v.bank;
            }
            e.2 = e.2.max(g);
            e.3 = e.3.min(g);
        }
        if physics_log > 0.0
            && (t_s / physics_log).floor() != ((t_s + dt) / physics_log).floor()
        {
            log_physics(&player.vehicle, t_s + dt);
        }
        // `OMSI_TRACE_VARS=a,b,$c`: the listed variables every half second of the run
        // (a leading `$` reads a string variable) - how a start-up sequence unfolds
        if i.is_multiple_of(super::steps_in(0.5)) {
            if let Some(list) = omsi_cfg::flags::OMSI_TRACE_VARS.var() {
                let vals: Vec<String> = list
                    .split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| match v.strip_prefix('$') {
                        Some(s) => format!("{v}={:?}", player.vehicle.str_var(s)),
                        None => format!(
                            "{v}={}",
                            player
                                .vehicle
                                .var(v)
                                .map(|x| format!("{x:.3}"))
                                .unwrap_or_else(|| "-".into())
                        ),
                    })
                    .collect();
                log::info!("t={t_s:.1} {}", vals.join(" "));
            }
        }
    }

    /// The people: their step, and what it means for the buses and the personnel file.
    pub(super) fn humans_step(&mut self, i: usize) {
        let (args, w, h, dt, view_aspect) = (self.args, self.w, self.h, self.dt, self.view_aspect);
        let Self {
            ref mut humans_off,
            ref world,
            ref settings,
            ref srv_admin,
            ref run_clock,
            ref remotes_off,
            ref mut player,
            ref mut traffic,
            ref camera,
            ref duty,
            ref renderer,
            ref mut scene,
            ref mut sim_view,
            ref mut career,
            ..
        } = *self;
        let size = (w, h);
        if let Some(h) = humans_off.as_mut() {
            // keep density and time_of_day up to date every tick, as app_events.rs does
            // (stop_target = enter_mean * density; without this it stays at the startup
            // value and the new formula returns 0 for the whole session when the map has
            // a low hourly density at the start time)
            // (a dedicated server's own passenger density, which its admins may change)
            let density = srv_admin.passenger_density.unwrap_or(settings.pax_density);
            steps::humans_by_hour(h, world, run_clock.time, density, duty.as_ref());
            // populate stops near every LAN player every 2 seconds, as app_events.rs
            // does every 2 s near the local player.  At startup `center` is ZERO (no
            // player bus on a headless server), so stops on the actual map - which can
            // be thousands of metres away - fall outside the 600 m filter in
            // `populate_with` and are never seeded without this loop.
            if i.is_multiple_of(super::steps_in(2.0)) {
                let player_centers: Vec<glam::DVec3> = remotes_off
                    .remotes
                    .values()
                    .map(|r| r.vehicle().position)
                    .chain(player.as_ref().map(|p| p.vehicle.position))
                    .collect();
                for c in &player_centers {
                    h.populate(world, *c);
                    view_sync::apply_people(h, &mut sim_view.people, world, renderer, scene);
                }
                // also update which stops the LAN players are near
                h.lan_centers = player_centers;
            }
            // what the passengers must not be seen appearing in front of
            let eye_cam = eye_camera(args, traffic.as_ref(), player.as_ref(), camera);
            h.eye = Some(humans::Eye::of(&eye_cam, view_aspect).widened(triple_extent(settings, &eye_cam, size.0, size.1)));
            h.set_remote_buses(remotes_off.remotes.iter().map(|(id, r)| (*id, r.vehicle())));
            h.set_duty(duty.as_ref());
            h.set_player_next_stop(duty.as_ref().and_then(|d| d.trip().stops.get(d.next_stop)));
            let took = steps::tick_humans(h, &mut sim_view.people, dt, world, player.as_mut(), traffic.as_mut(), renderer, scene);
            if let Some(p) = player.as_mut() {
                if took {
                    p.vehicle.set_var("GivenTicket", -1.0);
                }
                p.vehicle.host.humans_on_path_link = h.path_link_counts();
                p.vehicle.host.humans_on_seat = h.seat_counts();
            }
            if sim_view.people.tracing() {
                // OMSI_TRACE_PAX wants every frame as a window would draw it
                view_sync::sync(ViewSync::people(h, None, eye_cam.position), sim_view, world, renderer, scene);
            }
            if let Some(m) = h.take_message() {
                log::info!("HUD: {m}");
            }
            if let Some(p) = player.as_mut() {
                h.write_pax_vars(&mut p.vehicle);
            }
            if let Some(p) = player.as_ref() {
                let hurt = steps::people_in_career(career, h, p, settings.collision_pedestrians);
                if hurt > 0 {
                    log::warn!("{hurt} pedestrian(s) knocked down");
                }
            }
            for (name, price) in h.take_sales() {
                log::info!("plugin event: ticket_sold {} {price}", name.trim());
            }
            if std::mem::take(&mut h.stop_request) {
                if let Some(p) = player.as_mut() {
                    p.vehicle.trigger("int_haltewunsch");
                }
            }
        }
    }

    /// The LAN session's step (with the clock a host tells the others).
    pub(super) fn lan_step(&mut self) {
        let (args, dt) = (self.args, self.dt);
        let Self {
            ref mut lan_off,
            ref camera,
            ref mut player,
            ref lan_audio,
            ref run_clock,
            ref duty,
            ref mut remotes_off,
            ref mut lan_world_off,
            ref world,
            ref renderer,
            ref mut scene,
            ref mut sim_view,
            ref mut traffic,
            ref mut humans_off,
            ref mut schedule,
            ref mut real_time,
            ..
        } = *self;
        if let Some(l) = lan_off.as_mut() {
            let listener = if args.cam.is_some() {
                Some(camera.position)
            } else {
                player.as_ref().map(|p| p.vehicle.position)
            };
            if let (Some(a), Some(at)) = (lan_audio.as_ref(), listener) {
                a.set_listener(omsi_audio::Listener {
                    position: at.as_vec3(),
                    ..Default::default()
                });
            }
            // a host tells the others its clock (a client here keeps the one it started with;
            // a server's runs at its speed, moved by its admins)
            let clock = (l.role == omsi_net::Role::Host).then_some(run_clock);
            let frame = lan::LanInput {
                listener,
                muffled: false,
                riders: humans_off.as_ref().map(|h| h.riding()).unwrap_or(0),
                clock,
                tour: duty.as_ref().map(|d| format!("{}/{}", d.line, d.tour)),
                walker: None,
                inside_of: None,
                radio_keyed: false,
                figure: player.as_ref().and_then(|p| p.driver.as_ref()).map(|d| d.human_type().def.path.clone()),
                drawn: true,
            };
            let config = args.lan_config();
            let host = lan::AppLanHost { args, world: Some(world) };
            if l.role == omsi_net::Role::Host {
                if let Some(t) = traffic.as_mut() {
                    for c in &mut t.sim.cars {
                        if let Some(id) = t.table.id_of(c.id) { remotes_off.publish_vehicle_sounds(l, &mut c.vehicle, id.raw() as u64, &config); }
                    }
                }
            }
            sim_view.lan.capture_freetex(player.as_ref(), &args.root);
            let sent = {
                let traffic = traffic.as_ref();
                let cars = || traffic.map(|t| t.cars.iter().map(|c| lan::car_footprint(&c.vehicle)).collect()).unwrap_or_default();
                lan::tick_send(l, remotes_off, dt, &config, player.as_mut().map(|p| &mut p.vehicle), &cars, None, &frame)
            };
            lan_world_off.tick(
                l,
                dt,
                &config,
                Some(world),
                Some(renderer),
                Some(scene),
                traffic.as_mut(),
                humans_off.as_mut(),
                sim_view,
                player.as_ref().map(|p| p.vehicle.position),
            );
            if let Some(t) = traffic.as_mut().filter(|t| t.sim.mirror) {
                let ready: Vec<_> = t.sim.cars.iter().filter(|c| t.has_sound_set(c.id)).map(|c| c.id).collect();
                for c in &mut t.sim.cars {
                    if ready.contains(&c.id) { remotes_off.apply_ai_sounds(&mut c.vehicle, c.id, &config.root); }
                }
            }
            let updates = lan::tick_receive(
                l,
                remotes_off,
                sent,
                dt,
                &config,
                &host,
                player.as_mut().map(|p| &mut p.vehicle),
                traffic.as_ref().map(|t| &t.net),
                &frame,
            );
            crate::view_sync::apply_lan(remotes_off, &mut sim_view.lan, world, renderer, scene, lan_audio.as_ref(), &frame, dt);
            sim_view.lan.sync_freetex(l, remotes_off, renderer, scene, &args.root);
            for u in updates {
                if let (lan::WorldUpdate::Tours(tours), Some(s)) = (u, schedule.as_mut()) {
                    s.set_lan_tours(tours);
                }
            }
            // the other games run in real time (see `RealTime`)
            std::thread::sleep(real_time.wait(Instant::now(), dt));
        }
    }

    /// The cabin air, the tyres' spray and the roads' wetness, frame by frame as the window's
    /// `frame_lights` and `frame_lighting` have them.
    pub(super) fn surroundings_step(&mut self) {
        let (args, dt) = (self.args, self.dt);
        let Self {
            ref mut spray,
            ref mut cabin_air,
            ref mut wetness,
            ref traffic,
            ref player,
            ref camera,
            ref remotes_off,
            ref world,
            ref weather,
            ref humans_off,
            ..
        } = *self;
        if let Some(p) = player.as_ref() {
            steps::cabin_air_step(cabin_air, dt, p, weather, humans_off.as_ref());
            if omsi_cfg::flags::OMSI_CABIN_LOG.is_set() {
                static FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let n = FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n % ((20.0 / dt.max(1e-3)) as u32).max(1) == 0 {
                    let t = n as f32 * dt;
                    log::info!("cabin t={t:.0}s air {:.1} C vapour {:.2} g/m3 film (screen, sides, rear) {:.2} {:.2} {:.2} g/m2 defrost {:.2} scripted {:?}", cabin_air.temp, cabin_air.vapour, cabin_air.film[0], cabin_air.film[1], cabin_air.film[2], cabin_air.defrost, p.vehicle.scripted_cabin_air());
                }
            }
        }
        // the tyres' spray, frame by frame as the window throws it (the camera that matters
        // for its detail: the followed car's, else the player's bus): the puddles and the wet
        // asphalt the renderer draws (none under snow, OMSI_WETNESS as the picture takes it).
        // A dedicated server's picture (a 64x36 offscreen one) is never looked at: no spray
        // to throw for every AI car in the rain
        let spray_wet = puddles::road_wetness(*wetness, weather.snow);
        let snowing = weather.snow || weather.snow_on_road || precip_of(weather).0 == 2;
        omsi_sim::particles::set_road_water((!snowing).then(|| puddles::road_water_hook(world, spray_wet)));
        let server = crate::server::SERVER_MODE.load(std::sync::atomic::Ordering::Relaxed);
        if !server && (spray_wet > 0.0 || !spray.is_empty()) && !omsi_cfg::flags::OMSI_NO_SPRAY.is_set() {
            let eye = traffic
                .as_ref()
                .and_then(|t| follow_id(args, Some(t)).and_then(|id| follow_camera(Some(t), id)))
                .map(|c| c.position)
                .or(player.as_ref().filter(|_| args.cam.is_none()).map(|p| p.vehicle.position))
                .unwrap_or(camera.position);
            steps::throw_spray(spray, dt, player.as_ref(), traffic.as_ref(), remotes_off, eye, steps::spray_wind(weather), world, spray_wet, world.settings.rain_quality());
        }
        *wetness = crate::weather_setup::road_wetness(precip_of(weather).1, dt as f64, *wetness);
    }

    /// A snapshot due at `t_s` (`--snapshots`).
    pub(super) fn snapshot_step(&mut self, t_s: f32) -> Result<()> {
        let (args, out, w, h, dt) = (self.args, self.out, self.w, self.h, self.dt);
        // mid-run snapshots (relative to the first overtake with --follow auto)
        let traffic = self.traffic.as_ref();
        let auto_base = match args.follow.as_deref() {
            Some("auto") => traffic.and_then(|t| t.last_overtaker).map(|o| o.1),
            Some("turn") => traffic.and_then(|t| t.first_turner).map(|o| o.1),
            Some("red") => traffic.and_then(|t| t.first_red).map(|o| o.1),
            Some("yield") => traffic.and_then(|t| t.first_yield).map(|o| o.1),
            Some("pass") => traffic.and_then(|t| t.first_passer).map(|o| o.1),
            _ => Some(0.0),
        };
        if let (Some(&ts), Some(base)) = (self.snapshot_times.first(), auto_base) {
            if t_s + dt > ts + base {
                self.snapshot_times.remove(0);
                let (pixels, cam) = self.picture_now()?;
                let path = out.with_file_name(format!(
                    "{}_{ts:.1}.png",
                    out.file_stem().and_then(|s| s.to_str()).unwrap_or("snap")
                ));
                image::save_buffer(&path, &pixels, w, h, image::ColorType::Rgba8)?;
                if omsi_cfg::flags::OMSI_BLEND_AB.is_set() {
                    // the same moment with the blended draws in the old order (by origin
                    // distance only), for a before/after picture of the draw order
                    self.renderer.blend_by_origin = true;
                    let lighting = self.lighting_now(&cam);
                    let old = self.renderer.render_to_image(&mut self.scene, w, h, &cam, &lighting)?;
                    self.renderer.blend_by_origin = false;
                    image::save_buffer(
                        path.with_extension("old.png"),
                        &old,
                        w,
                        h,
                        image::ColorType::Rgba8,
                    )?;
                }
                log::info!(
                    "snapshot at {ts:.1} s -> {} (camera ({:.1}, {:.1}, {:.1}) yaw {:.0})",
                    path.display(),
                    cam.position.x,
                    cam.position.y,
                    cam.position.z,
                    cam.yaw
                );
            }
        }
        Ok(())
    }
}

/// The server's AI traffic for its admins' operations (`omsi_lan::admin::world`), its look
/// brought up to date after cars were taken off the road.
struct AdminTraffic<'a, F: FnMut(&mut crate::traffic::Traffic)>(&'a mut crate::traffic::Traffic, F);

impl<F: FnMut(&mut crate::traffic::Traffic)> omsi_lan::admin::world::ServerTraffic for AdminTraffic<'_, F> {
    fn set_density(&mut self, n: usize) {
        self.0.target = n;
    }
    fn clear_cars(&mut self) -> usize {
        let ids: Vec<u64> = self.0.cars.iter().filter(|c| !c.is_bus()).map(|c| c.id).collect();
        for id in &ids {
            self.0.remove_car(*id);
        }
        (self.1)(self.0);
        ids.len()
    }
}
