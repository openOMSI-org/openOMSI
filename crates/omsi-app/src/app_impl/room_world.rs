//! Install a room only after its map can be opened; never exit the game on a load error.

use super::rooms::LoadedRoom;
use super::*;

impl App {
    pub(super) fn install_room(&mut self, mut room: LoadedRoom) -> Result<(), String> {
        if let Some(reason) = room
            .session
            .turned_away
            .as_ref()
            .or(room.session.rejected.as_ref())
        {
            return Err(reason.clone());
        }
        let next_weather = room.args.weather.clone();
        if let Some(opened) = room.opened.take() {
            let Some(renderer) = self.renderer.as_ref() else {
                return Err("The renderer is not ready for a new map".into());
            };
            let mut scene = renderer.new_scene();
            let streaming = !room.args.all && room.args.radius.is_none();
            let mut kept = Some(room.session);
            let loaded = if streaming {
                Ok((opened.0, opened.1))
            } else {
                lan::answering_while(&mut kept, room.args.bus.as_deref(), || {
                    crate::world_load::load_open_world(&room.args, renderer, &mut scene, opened)
                })
            };
            room.session = kept.unwrap();
            let (world, camera) = loaded.map_err(|e| {
                format!(
                    "{} {e:#}",
                    omsi_ui::tr("The room's map could not be loaded:")
                )
            })?;
            self.clear_room_world();
            self.args = room.args;
            self.net.lan = Some(room.session);
            let mut config = self.args.lan_config();
            lan::adopt_host_world(
                &mut config,
                &lan::AppLanHost {
                    args: &self.args,
                    world: None,
                },
                self.net.lan.as_mut().unwrap(),
                &mut self.net.remotes,
            );
            self.args.take_lan_config(&config);
            self.clock = start_clock(&self.args);
            let renderer = self.renderer.take().unwrap();
            self.session.envir = omsi_content::Envir::load(&self.args.root.join("envir.cfg")).ok();
            let (weather, model) = load_weather_run(&self.args);
            self.session.weather = Some(weather);
            self.session.sky.model = model;
            self.session.wetness = self
                .session
                .weather
                .as_ref()
                .map(initial_wetness)
                .unwrap_or(0.0);
            setup_sky(
                &self.args,
                &renderer,
                &mut scene,
                self.session.envir.as_ref(),
                self.session.weather.as_ref(),
                &mut self.session.sky.clouds,
            );
            let world = Arc::new(world);
            if streaming {
                let distance = self
                    .args
                    .view_distance
                    .or_else(settings::view_distance)
                    .unwrap_or(1200.0)
                    .max(omsi_map::tile_size());
                let video = video_memory_limit(&self.settings);
                world.set_fast_texture_loads(true);
                world.set_texture_budget(texture_budget(video));
                world.set_video_memory_limit(video);
                self.gfx.streamer = Some(tiles::Streamer::new(
                    world.clone(),
                    &start_centers(&self.args, &camera, Some(&world)),
                    distance,
                    700.0,
                    crate::release_free_memory,
                ));
                self.world = Some(world);
                self.cam.starting = Some(camera);
            } else {
                self.start_world(world, camera, &renderer, &mut scene);
            }
            self.renderer = Some(renderer);
            self.scene = Some(scene);
        } else {
            self.args = room.args;
            self.net.lan = Some(room.session);
            let mut config = self.args.lan_config();
            lan::adopt_host_world(
                &mut config,
                &lan::AppLanHost {
                    args: &self.args,
                    world: self.world.as_deref(),
                },
                self.net.lan.as_mut().unwrap(),
                &mut self.net.remotes,
            );
            self.args.take_lan_config(&config);
            let clock = start_clock(&self.args);
            self.apply_world_update(lan::WorldUpdate::Clock {
                year: clock.year,
                day_of_year: clock.day_of_year,
                time: clock.time,
            });
            self.change_weather(next_weather, false, 240.0);
            self.enable_room_population();
        }
        self.paused = false;
        self.last = Instant::now();
        Ok(())
    }

    fn enable_room_population(&mut self) {
        if self.session.humans.is_none() {
            let mut humans = humans::Humans::new(&self.args.root, &mut self.gfx.sim_view.people);
            humans.exact_fare = self.settings.exact_fare;
            humans.visual_hailing =
                self.settings.passenger_hailing && self.settings.passenger_animation == "enhanced";
            humans.bad_driving = self.settings.pax_bad_driving;
            humans.boarding = self.settings.boarding.clone();
            humans.stand_chance = self.settings.standing_chance;
            humans.voices = match self.settings.pax_voices.as_str() {
                "off" => 2,
                "tickets" => 1,
                _ => 0,
            };
            if let Some(p) = self.player.as_mut() {
                humans.set_cabin(&mut p.vehicle);
                humans.ticket_key = ticket_key_name(&self.args.root, &p.bindings);
                humans.tickets = p.vehicle.host.tickets.clone();
                if let Some(world) = self.world.as_ref() {
                    if !world.global.money_system.trim().is_empty() {
                        humans.money = Some(money::Money::new(
                            &self.args.root,
                            &world.global.money_system,
                        ));
                    }
                }
            }
            self.session.humans = Some(humans);
        }
        if let Some(humans) = self.session.humans.as_mut() {
            humans.avatar_only = false;
            if let Some(lan) = self.net.lan.as_ref() {
                humans.set_lan_seed(lan::population_seed(lan));
            }
        }
        if let Some(traffic) = self.session.traffic.as_mut() {
            traffic.lights_only = false;
            if let Some(lan) = self.net.lan.as_ref() {
                traffic.set_lan_seed(lan::population_seed(lan));
            }
        }
        if let (Some(schedule), Some(lan)) = (self.session.schedule.as_mut(), self.net.lan.as_ref())
        {
            schedule.sim.vehicle_seed =
                omsi_script::mix_seed(lan::population_seed(lan) ^ 0x746f_7572_5f62_7573);
        }
    }

    fn clear_room_world(&mut self) {
        self.pipeline = Default::default();
        self.gfx.streamer = None;
        self.world = None;
        self.player = None;
        self.camera = None;
        self.session.placed.clear();
        self.session.traffic = None;
        self.session.humans = None;
        self.session.scenery = Default::default();
        self.session.schedule = None;
        self.session.timetable_book = None;
        self.session.duty = None;
        self.session.duty_places = false;
        self.session.on_foot = None;
        self.session.safe_pose = None;
        self.session.journey = None;
        self.session.pumping = None;
        self.session.world_day = None;
        self.session.first_populate = true;
        self.session.populate_t = 0.0;
        self.session.humans_populate_t = 0.0;
        self.session.weather_blend = None;
        self.session.weather_cycle = None;
        self.session.metar_rx = None;
        self.session.metar_once = false;
        self.session.metar_next = 0.0;
        self.session.sky = crate::weather_setup::SkyState {
            look: self.args.session_settings,
            ..Default::default()
        };
        self.session.rain = rain::Rain::new();
        self.session.cabin_air = crate::condensation::CabinAir::new();
        self.session.spray = puddles::Spray::new();
        self.session.spray.graphics = self.args.session_settings.graphics();
        self.sound.ambience = None;
        self.sound.soundscape = None;
        self.sound.voices_waiting.clear();
        if let Some(audio) = self.sound.audio.as_ref() {
            self.sound.radio.stop(audio);
            self.sound.milezero.update(audio, false, 0.0);
        }
        self.integrations.plugin_voices.clear();
        self.sound.audio = None;
        self.sound.foot_in = None;
        self.gfx.sim_view = Default::default();
        self.gfx.frozen_mirrors = None;
        self.gfx.route_arrows = Default::default();
        self.menus.editor = None;
        self.menus.navigator = None;
        self.menus.pending_placement = None;
        self.menus.remote_added.clear();
        self.menus.placing = None;
        self.cam.ego = false;
        self.integrations.object_scripts = Default::default();
        self.fixed_tick = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn app() -> App {
        crate::assemble_app(Args::parse_from(["openomsi"]), Default::default())
    }

    #[test]
    fn joining_reenables_existing_avatar_only_population() {
        let mut app = app();
        let mut humans = humans::Humans::new(&app.args.root, &mut app.gfx.sim_view.people);
        humans.avatar_only = true;
        app.session.humans = Some(humans);
        app.enable_room_population();
        assert!(!app.session.humans.as_ref().unwrap().avatar_only);
    }

    #[test]
    fn same_map_handover_reseeds_traffic_and_timetable_for_each_room() {
        let mut app = app();
        let host = omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap();
        let root = std::env::temp_dir().join(format!(
            "openomsi-room-seeds-{}-{}",
            std::process::id(),
            host.session
        ));
        let map = root.join("maps/test/global.cfg");
        std::fs::create_dir_all(map.parent().unwrap()).unwrap();
        std::fs::write(&map, "[name]\nSeed regression\n").unwrap();
        let world = World::open(&root, &map, 20261031).unwrap();
        app.session.traffic = Some(traffic::Traffic::new(&root, &world, 0).unwrap());
        app.session.schedule = Some(schedule::Schedule::new(&root, &world, &Default::default()));
        app.net.lan = Some(host);
        for id in [17, 43] {
            app.net.lan.as_mut().unwrap().session = id;
            let seed = lan::population_seed(app.net.lan.as_ref().unwrap());
            app.session.traffic.as_mut().unwrap().set_lan_seed(1);
            app.session.schedule.as_mut().unwrap().sim.vehicle_seed = 1;
            app.enable_room_population();
            assert_eq!(app.session.traffic.as_ref().unwrap().rng, seed | 1);
            assert_eq!(
                app.session.schedule.as_ref().unwrap().sim.vehicle_seed,
                omsi_script::mix_seed(seed ^ 0x746f_7572_5f62_7573)
            );
        }
        drop(app);
        drop(world);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changing_maps_discards_plugin_audio_ids() {
        let mut app = app();
        app.integrations.plugin_voices.push((42, true));
        app.clear_room_world();
        assert!(app.integrations.plugin_voices.is_empty());
        assert!(app.sound.audio.is_none());
    }
}
