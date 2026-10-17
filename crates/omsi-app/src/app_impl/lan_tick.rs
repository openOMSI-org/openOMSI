//! LAN play once a frame, and what the host's world and the other players' games ask of ours.

use super::*;

impl App {
    /// LAN play, once a frame: send our bus, take in the others', and keep a drawn
    /// vehicle for each of them.
    pub(crate) fn tick_lan(&mut self, dt: f32) {
        let walker = self.walker_pose();
        let radio_keyed = self.voice_radio_held();
        let Some(lan) = self.net.lan.as_mut() else {
            // (the session is over: the plugin is told so)
            self.sound.voice = None;
            if let Some(p) = self.player.as_mut() {
                for texture in &mut p.vehicle.host.script_textures {
                    texture.disable_network_snapshot();
                }
                p.vehicle.host.capture_sound_triggers = false;
                p.vehicle.host.network_sound_triggers.clear();
            }
            return;
        };
        let duty = self
            .session.duty
            .as_ref()
            .map(|d| &d.trips[d.trip_index])
            .map(|t| (t.line.as_str(), t.terminus.as_str()));
        let frame = lan::LanInput {
            listener: self.camera.as_ref().map(|c| c.position),
            muffled: self.cam.in_cab || self.net.inside_remote.is_some(),
            riders: self.session.humans.as_ref().map(|h| h.riding()).unwrap_or(0),
            clock: Some(&self.clock),
            tour: self.session.duty.as_ref().map(|d| format!("{}/{}", d.line, d.tour)),
            walker,
            inside_of: self.net.inside_remote,
            radio_keyed,
            figure: self.player.as_ref().and_then(|p| p.driver.as_ref()).map(|d| d.human_type().def.path.clone()),
            drawn: self.world.is_some() && self.renderer.is_some() && self.scene.is_some(),
        };
        let config = self.args.lan_config();
        let host = AppLanHost { args: &self.args, world: self.world.as_deref() };
        if lan.role == omsi_net::Role::Host {
            if let Some(t) = self.session.traffic.as_mut() {
                for c in &mut t.sim.cars {
                    self.net.remotes.publish_vehicle_sounds(lan, &mut c.vehicle, c.id, &config);
                }
            }
        }
        let sent = {
            let traffic = self.session.traffic.as_ref();
            let cars = || traffic.map(|t| t.cars.iter().map(|c| lan::car_footprint(&c.vehicle)).collect()).unwrap_or_default();
            lan::tick_send(lan, &mut self.net.remotes, dt, &config, self.player.as_mut().map(|p| &mut p.vehicle), &cars, duty, &frame)
        };
        self.net.lan_world.tick(
            lan,
            dt,
            &config,
            self.world.as_deref(),
            self.renderer.as_ref(),
            self.scene.as_mut(),
            self.session.traffic.as_mut(),
            self.session.humans.as_mut(),
            &mut self.gfx.sim_view,
            self.player.as_ref().map(|p| p.vehicle.position),
        );
        if let Some(t) = self.session.traffic.as_mut().filter(|t| t.sim.mirror) {
            let ready: Vec<_> = t.sim.cars.iter().filter(|c| t.has_sound_set(c.id)).map(|c| c.id).collect();
            for c in &mut t.sim.cars {
                if ready.contains(&c.id) { self.net.remotes.apply_ai_sounds(&mut c.vehicle, c.id, &config.root); }
            }
        }
        let updates = lan::tick_receive(
            lan,
            &mut self.net.remotes,
            sent,
            dt,
            &config,
            &host,
            self.player.as_mut().map(|p| &mut p.vehicle),
            self.session.traffic.as_ref().map(|t| &t.net),
            &frame,
        );
        if let (Some(w), Some(r), Some(scene)) = (self.world.as_deref(), self.renderer.as_ref(), self.scene.as_mut()) {
            crate::view_sync::apply_lan(&mut self.net.remotes, &mut self.gfx.sim_view.lan, w, r, scene, self.sound.audio.as_ref(), &frame, dt);
        }
        for u in updates {
            self.apply_world_update(u);
        }
        let cmds = self.net.lan.as_mut().map(|l| l.take_commands()).unwrap_or_default();
        for (from, text) in cmds {
            self.lan_command(from, &text);
        }
        self.tick_voice(dt);
    }

    /// A command another player's game sent ours (`LanSession::command`): read by the session
    /// (`omsi_lan::commands::Command`), carried out here.
    pub(crate) fn lan_command(&mut self, from: u32, text: &str) {
        use omsi_lan::commands::Command;
        if let Some(request) = text.strip_prefix("pax_request ") {
            self.receive_passenger_door_request(from, request);
            return;
        }
        // a plugin's message to the same plugin here (`lan.send`)
        if self.plugin_lan_message(from, text) {
            return;
        }
        let Some(command) = Command::parse(from, text) else {
            log::info!("LAN: command '{text}' from player {from} not taken");
            return;
        };
        match command {
            // a switch worked by a passenger of ours: only by one who is in our bus
            Command::Trigger(ev) => {
                let may = self.net.lan.as_ref().is_some_and(|l| omsi_lan::passenger::may_work(l, from, self.player.as_ref().map(|p| &p.vehicle), &ev));
                match self.player.as_mut().filter(|_| may) {
                    Some(p) => {
                        log::info!("LAN: player {from} works {ev} in our bus");
                        p.vehicle.trigger(&ev);
                    }
                    None => log::info!("LAN: player {from} asked for switch {ev} of our bus from outside it, or for one only the driver works: ignored"),
                }
            }
            // the voice server of the session (`voice`): asked of the host, told by it
            Command::VoiceAsked => {
                if let Some(l) = self.net.lan.as_mut().filter(|l| l.role == omsi_net::Role::Host) {
                    l.command(from, &crate::voice::VoiceServer::command(crate::voice::hosted().as_ref()));
                }
            }
            Command::Voice(server) => {
                if let Some(v) = self.sound.voice.as_mut() {
                    v.set_server(server);
                }
            }
            command => crate::admin::carry_out(self, from, command),
        }
    }

    /// The host's world as LAN play asks for it: its clock (set or caught up with) and its
    /// weather, for everything that keeps a clock of its own.
    pub(crate) fn apply_world_update(&mut self, u: lan::WorldUpdate) {
        match u {
            lan::WorldUpdate::Clock {
                year,
                day_of_year,
                time,
            } => {
                self.clock.year = year;
                self.clock.day_of_year = day_of_year;
                self.clock.time = time;
                if let Some(t) = self.session.traffic.as_mut() {
                    t.day_time = time;
                }
            }
            lan::WorldUpdate::Slew(s) => {
                self.clock.time = (self.clock.time + s).clamp(0.0, 86399.999);
                if let Some(t) = self.session.traffic.as_mut() {
                    t.day_time += s;
                }
            }
            lan::WorldUpdate::Weather(w) => {
                log::info!(
                    "LAN: the host's weather: {}",
                    w.as_deref().unwrap_or("the map's default")
                );
                // (coming over to it as the host does, not at a stroke - the streets stay
                // as wet as they are and dry or wet with it)
                self.change_weather(w, false, 240.0);
            }
            lan::WorldUpdate::Tours(tours) => {
                if let Some(s) = self.session.schedule.as_mut() {
                    s.set_lan_tours(tours);
                }
            }
        }
        if let Some(p) = self.player.as_mut() {
            p.vehicle.host.clock = self.clock.clone();
        }
    }
}

/// A key as LAN play's chat line takes it (see `lan::ChatKey`).
pub(crate) fn chat_key_of(code: winit::keyboard::KeyCode) -> lan::ChatKey {
    use std::hash::{Hash, Hasher};
    use winit::keyboard::KeyCode;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut h);
    let role = match code {
        KeyCode::Enter | KeyCode::NumpadEnter => lan::ChatRole::Send,
        KeyCode::Escape => lan::ChatRole::Cancel,
        KeyCode::Backspace => lan::ChatRole::Erase,
        _ => lan::ChatRole::Other,
    };
    lan::ChatKey { id: h.finish(), role }
}

impl crate::Args {
    /// What LAN play takes from the command line (see `omsi_lan::config::LanConfig`).
    pub(crate) fn lan_config(&self) -> omsi_lan::config::LanConfig {
        omsi_lan::config::LanConfig {
            root: self.root.clone(),
            map: self.map.clone(),
            weather: self.weather.clone(),
            season: self.season.clone(),
            date: self.date.clone(),
            day_of_year: self.day_of_year,
            time: self.time.clone(),
            bus: self.bus.clone(),
            paint: self.paint.clone(),
            driver: self.driver.clone(),
            lan_name: self.lan_name.clone(),
            lan_host: self.lan_host,
            lan_join: self.lan_join.clone(),
            server: self.server.is_some(),
            entry: self.entry,
            spawn: self.spawn.clone(),
            hof: self.hof.clone(),
            line: self.line.clone(),
            tour: self.tour.clone(),
            trip: self.trip.clone(),
        }
    }

    /// Take back what LAN play changed (a joining player's map, date, time and weather, and
    /// the spawn choices a new map drops).
    pub(crate) fn take_lan_config(&mut self, c: &omsi_lan::config::LanConfig) {
        self.map = c.map.clone();
        self.weather = c.weather.clone();
        self.season = c.season.clone();
        self.date = c.date.clone();
        self.day_of_year = c.day_of_year;
        self.time = c.time.clone();
        self.entry = c.entry;
        self.spawn = c.spawn.clone();
        self.hof = c.hof.clone();
        self.line = c.line.clone();
        self.tour = c.tour.clone();
        self.trip = c.trip.clone();
    }
}

/// What LAN play asks of the game (see `omsi_lan::host::LanHost`): its arguments as they
/// are now and the world, when one is loaded.
pub(crate) struct AppLanHost<'a> {
    pub(crate) args: &'a crate::Args,
    pub(crate) world: Option<&'a crate::scene::World>,
}

impl omsi_lan::host::LanHost for AppLanHost<'_> {
    fn world(&self) -> Option<std::sync::Arc<omsi_sim::world_data::WorldData>> {
        self.world.map(|w| w.data())
    }

    fn world_lanes(&self) -> Vec<omsi_sim::traffic::Lane> {
        self.world.map(|w| w.traffic_inbox.peek(omsi_sim::world_data::TrafficArrivals::lanes_of)).unwrap_or_default()
    }

    fn remote_vehicle(
        &self,
        args: &omsi_lan::config::LanConfig,
        ty: std::sync::Arc<omsi_sim::VehicleType>,
        scheme: Option<usize>,
        clock: omsi_sim::SimClock,
        fonts: Option<omsi_lan::mods::vehicles::SharedFonts>,
    ) -> (omsi_sim::VehicleInstance, Option<std::sync::Arc<omsi_vehicle::Hof>>) {
        let world = self.world.expect("another player's vehicle is made with a world loaded");
        let mut host = omsi_sim::VehicleHost::new(clock);
        let fonts = fonts.unwrap_or_else(|| world.fonts.clone());
        host.font_lib = Some(fonts.clone());
        let hof = self.depot_of(args, &ty);
        host.hof = hof.clone();
        host.paint_scheme = Some(scheme);
        host.remote_script_textures = true;
        host.remote_sound_triggers = true;
        let mut vehicle = omsi_sim::VehicleInstance::new(ty.clone(), host);
        vehicle.ground = None;
        if !ty.model.text_textures.is_empty() {
            vehicle.init_text_textures(&mut fonts.lock(), &|p| {
                omsi_texture::decode_file(p)
                    .ok()
                    .map(|i| (i.width, i.height, i.rgba))
            });
        }
        vehicle.apply_paint_vars(scheme);
        // the coupled sections of an articulated bus (drawn by the view as they are attached)
        let mut lead = ty.clone();
        let mut lead_rev = false;
        let mut seen = vec![lead.def.path.clone()];
        for _ in 0..crate::spawn::MAX_COUPLED_PARTS {
            let Some((path, rev)) = crate::spawn::next_coupled(&lead.def, lead_rev, true) else {
                break;
            };
            if crate::spawn::chain_has_part(&seen, &path) {
                break;
            }
            match omsi_sim::VehicleType::load(&args.root, &path) {
                Ok(t) => {
                    let t = std::sync::Arc::new(t);
                    vehicle.attach_trailer_ex(t.clone(), rev);
                    seen.push(path);
                    lead = t;
                    lead_rev = rev;
                }
                Err(e) => {
                    log::warn!("LAN: rear section {}: {e}", path.display());
                    break;
                }
            }
        }
        (vehicle, hof)
    }

    fn set_destination(&self, v: &mut omsi_sim::VehicleInstance, hof: Option<&omsi_vehicle::Hof>, line: &str, destination: &str) {
        crate::schedule::set_ai_destination(v, hof, line, destination, &[]);
    }

    fn version(&self) -> String {
        omsi_platform::VERSION.into()
    }

    fn publish_vehicles(&self, only: Vec<String>) {
        crate::lan::publish_vehicles(self.args.root.clone(), only);
    }

    fn season_folders(&self, cfg: &omsi_lan::config::LanConfig) -> Option<(Option<String>, Option<String>)> {
        let world = self.world?;
        let mut theirs = self.args.clone();
        theirs.take_lan_config(cfg);
        Some((crate::season_folder(&theirs, &world.global).1, omsi_texture::season_folder()))
    }

    fn weather_without_file(&self, w: &str) -> bool {
        crate::weather_model::is_natural(Some(w)) || w == "cycle" || crate::weather_setup::custom_weather(Some(w)).is_some()
    }

    fn weather_report(&self, w: &str) -> Option<bool> {
        w.starts_with(crate::weather_setup::REPORT).then(|| crate::weather_setup::from_report(w).is_some())
    }

    fn depot_of(&self, cfg: &omsi_lan::config::LanConfig, ty: &omsi_sim::VehicleType) -> Option<std::sync::Arc<omsi_vehicle::Hof>> {
        crate::find_hof_named(cfg.hof.as_deref(), self.world?, ty)
    }
}
