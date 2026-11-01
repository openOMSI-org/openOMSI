//! Room handovers are staged: connect privately, confirm, then load the accepted room.

use super::*;
use omsi_lan::handover::PreparedJoin;
use std::sync::mpsc::{self, Receiver, TryRecvError};

type OpenedWorld = (World, Camera, DVec3);

pub(crate) struct LoadedRoom {
    pub(super) session: omsi_net::LanSession,
    pub(super) args: Args,
    pub(super) opened: Option<OpenedWorld>,
}

#[derive(Default)]
pub(crate) struct RoomSwitch {
    connecting: Option<Receiver<Result<PreparedJoin, String>>>,
    loading: Option<Receiver<Result<LoadedRoom, String>>>,
    pub(crate) ready: Option<PreparedJoin>,
    pub(crate) accept: bool,
    pub(crate) leave: bool,
}

impl RoomSwitch {
    pub(crate) fn busy(&self) -> bool {
        self.connecting.is_some() || self.loading.is_some()
    }

    pub(crate) fn cancel(&mut self) {
        self.connecting = None;
        self.ready = None;
        self.accept = false;
    }
}

pub(crate) fn different_map(current: &str, host: &str) -> bool {
    let normal = |path: &str| {
        let path = path.trim().replace('\\', "/").to_ascii_lowercase();
        path.rfind("maps/")
            .map(|i| path[i..].to_string())
            .unwrap_or(path)
    };
    !host.trim().is_empty() && normal(current) != normal(host)
}

impl App {
    pub(crate) fn open_multiplayer(&mut self) {
        self.close_game_menu();
        self.menus.game_panels.multiplayer = Some(Default::default());
    }

    fn room_notice(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.service_msg = Some((message.clone(), 8.0));
        if let Some(form) = self.menus.game_panels.multiplayer.as_mut() {
            form.said = Some(message);
        }
    }

    pub(crate) fn connect_room(&mut self, target: &str) {
        if self.net.room.busy() {
            return;
        }
        self.net.room.ready = None;
        let target = target.trim().to_string();
        let config = self.args.lan_config();
        let name = lan::player_name(&config);
        let world = lan::world_info(&config);
        let planned = omsi_net::Pose {
            bus: self
                .player
                .as_ref()
                .map(|p| p.vehicle.ty.def.path.to_string_lossy().replace('\\', "/"))
                .or(config.bus)
                .unwrap_or_default(),
            ..Default::default()
        };
        let hosted_session = self
            .net
            .lan
            .as_ref()
            .filter(|session| session.role == omsi_net::Role::Host)
            .map(|session| session.session);
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new()
            .name("room-connect".into())
            .spawn(move || {
                let result = PreparedJoin::connect_for_handover(
                    &target,
                    &name,
                    world,
                    &planned,
                    hosted_session,
                );
                let _ = tx.send(result);
            }) {
            Ok(_) => {
                self.net.room.connecting = Some(rx);
                self.room_notice("Connecting to the new room; the current game stays open");
            }
            Err(error) => self.room_notice(error.to_string()),
        }
    }

    pub(crate) fn reconnect_room(&mut self) {
        let reconnected = self.net.lan.as_mut().is_some_and(|lan| lan.reconnect());
        self.room_notice(if reconnected {
            "Reconnecting to the current room"
        } else {
            "There is no joined room to reconnect to"
        });
    }

    /// Called before simulation and drawing, so a map never changes midway through a frame.
    pub(crate) fn tick_room_switch(&mut self, elapsed: f32) {
        if self.net.room.leave && !self.net.room.busy() {
            self.net.room.leave = false;
            self.net.room.cancel();
            self.leave_room();
            self.room_notice("Left the room; continuing offline");
        }
        if let Some(rx) = self.net.room.connecting.take() {
            match rx.try_recv() {
                Ok(Ok(room)) => {
                    self.net.room.ready = Some(room);
                    self.room_notice("Room found. Confirm to leave the current room and join it.");
                }
                Ok(Err(error)) => self.room_notice(error),
                Err(TryRecvError::Empty) => self.net.room.connecting = Some(rx),
                Err(TryRecvError::Disconnected) => self.room_notice(
                    "The connection attempt stopped; the current game has not been changed",
                ),
            }
        }
        let refused = self.net.room.ready.as_mut().and_then(|room| {
            room.session.keepalive(elapsed, &omsi_net::Pose::default());
            room.poll_vehicle_offers();
            room.session
                .turned_away
                .as_ref()
                .or(room.session.rejected.as_ref())
                .cloned()
        });
        if let Some(reason) = refused {
            self.net.room.cancel();
            self.room_notice(reason);
        }
        if std::mem::take(&mut self.net.room.accept) {
            if let Some(room) = self.net.room.ready.take() {
                self.load_room(room);
            }
        }
        if let Some(rx) = self.net.room.loading.take() {
            match rx.try_recv() {
                Ok(Ok(room)) => match self.install_room(room) {
                    Ok(()) => self.room_notice("Joined the new room"),
                    Err(error) => self.failed_room(error),
                },
                Ok(Err(error)) => self.failed_room(error),
                Err(TryRecvError::Empty) => self.net.room.loading = Some(rx),
                Err(TryRecvError::Disconnected) => {
                    self.failed_room("The room could not be loaded; continuing offline".into())
                }
            }
        }
    }

    fn failed_room(&mut self, error: String) {
        lan::close_public_gateway();
        omsi_lan::gateway::new_offers_session();
        omsi_cfg::vfs::record_reads(false);
        self.room_notice(error);
    }

    fn load_room(&mut self, room: PreparedJoin) {
        let mut args = self.args.clone();
        args.lan_host = None;
        args.lan_join = Some(room.target().to_string());
        args.lan_ws_fallback = None;
        if let Some(p) = self.player.as_ref() {
            args.bus = Some(p.vehicle.ty.def.path.to_string_lossy().replace('\\', "/"));
        }
        let reload = room
            .session
            .welcome
            .as_ref()
            .is_some_and(|w| different_map(&args.map, &w.world.map));
        self.leave_room();
        self.room_notice("Loading the new room; the current game stays open");
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new()
            .name("room-load".into())
            .spawn(move || {
                let _ = tx.send(prepare_room(room, args, reload));
            }) {
            Ok(_) => self.net.room.loading = Some(rx),
            Err(error) => self.failed_room(error.to_string()),
        }
    }

    /// Release every room-owned system before another room may reuse player/entity ids.
    pub(crate) fn leave_room(&mut self) {
        self.net.mods_server = None;
        if let Some(mut voice) = self.sound.voice.take() {
            voice.stop(self.sound.audio.as_ref());
        }
        if let (Some(w), Some(r), Some(scene)) = (
            self.world.as_deref(),
            self.renderer.as_ref(),
            self.scene.as_mut(),
        ) {
            self.gfx
                .sim_view
                .lan
                .clear(w, r, scene, self.sound.audio.as_ref());
            self.net.lan_world.leave(
                w,
                r,
                scene,
                self.session.traffic.as_mut(),
                self.session.humans.as_mut(),
                &mut self.gfx.sim_view,
            );
        }
        if let Some(h) = self.session.humans.as_mut() {
            for (id, _) in self.net.remote_walkers.drain(..) {
                h.avatar_remove(crate::on_foot::REMOTE_KEY + id);
            }
        }
        self.net.remote_walkers.clear();
        self.net.inside_remote = None;
        self.net.is_admin = false;
        self.net.admin_panel = Default::default();
        self.net.remotes = Default::default();
        self.net.lan_world = Default::default();
        self.net.lan = None;
        lan::close_public_gateway();
        omsi_lan::gateway::new_offers_session();
        omsi_cfg::vfs::record_reads(false);
        self.args.lan_host = None;
        self.args.lan_join = None;
        self.args.lan_ws_fallback = None;
        if let Some(traffic) = self.session.traffic.as_mut() {
            traffic.lights_only = !(self.args.traffic > 0
                || self.args.schedule
                || crate::rail_drive::args_rail(&self.args));
        }
        if let Some(humans) = self.session.humans.as_mut() {
            humans.avatar_only = !self.args.passengers;
        }
        if let Some(player) = self.player.as_mut() {
            for texture in &mut player.vehicle.host.script_textures {
                texture.disable_network_snapshot();
            }
            player.vehicle.host.capture_sound_triggers = false;
            player.vehicle.host.network_sound_triggers.clear();
        }
        if let Some(s) = self.session.schedule.as_mut() {
            s.set_lan_tours(Default::default());
        }
    }
}

fn prepare_room(room: PreparedJoin, mut args: Args, reload: bool) -> Result<LoadedRoom, String> {
    let mut session = room.accept();
    let mut config = args.lan_config();
    let _mods_server = lan::share_mods(&mut config, &mut session);
    lan::take_host_map(&mut config, &mut session);
    if session
        .welcome
        .as_ref()
        .is_some_and(|w| different_map(&config.map, &w.world.map))
    {
        return Err("The room's map is not installed; install it and try again".into());
    }
    if reload {
        args.cam = None;
        args.situation_others.clear();
        args.situation_vars.clear();
        args.situation_strvars.clear();
    }
    lan::adopt_host_world(
        &mut config,
        &lan::AppLanHost {
            args: &args,
            world: None,
        },
        &mut session,
        &mut Default::default(),
    );
    args.take_lan_config(&config);
    let mut kept = Some(session);
    let opened = if reload {
        Some(
            lan::answering_while(&mut kept, args.bus.as_deref(), || {
                crate::world_load::open_world(&args)
            })
            .map_err(|e| {
                format!(
                    "{} {e:#}",
                    omsi_ui::tr("The room's map could not be opened:")
                )
            })?,
        )
    } else {
        None
    };
    Ok(LoadedRoom {
        session: kept.unwrap(),
        args,
        opened,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_comparison_ignores_root_separator_and_case() {
        assert!(!different_map(
            "C:/OMSI/maps/Grundorf/global.cfg",
            "maps\\grundorf\\global.cfg"
        ));
        assert!(!different_map("maps/A/global.cfg", ""));
        assert!(different_map("maps/A/global.cfg", "maps/B/global.cfg"));
    }

    #[test]
    fn a_host_cannot_join_its_own_room() {
        use clap::Parser;
        let mut app = crate::assemble_app(Args::parse_from(["openomsi"]), Default::default());
        let host = omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap();
        let id = host.session;
        let address = host.local_addr();
        let code = host.code().unwrap().encode();
        app.net.lan = Some(host);
        app.open_multiplayer();
        app.connect_room(&code);
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while app.net.room.busy() && Instant::now() < deadline {
            app.tick_room_switch(0.02);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!app.net.room.busy());
        assert!(app.net.room.ready.is_none());
        let host = app.net.lan.as_ref().unwrap();
        assert_eq!(host.role, omsi_net::Role::Host);
        assert_eq!(host.session, id);
        assert_eq!(host.local_addr(), address);
        assert_eq!(host.peer_count(), 0);
        assert_eq!(
            app.menus
                .game_panels
                .multiplayer
                .as_ref()
                .unwrap()
                .said
                .as_deref(),
            Some("You cannot join the room you are hosting")
        );
    }

    #[test]
    fn leaving_host_stops_its_mods_server() {
        let mut app = crate::assemble_app(Args::parse_from(["openomsi"]), Default::default());
        let mut host = omsi_net::LanSession::host(0, "host", Default::default(), false).unwrap();
        let address = format!("127.0.0.1:{}", host.local_addr().unwrap().port());
        app.net.mods_server = lan::share_mods(&mut app.args.lan_config(), &mut host);
        assert!(app.net.mods_server.is_some());
        app.net.lan = Some(host);
        app.leave_room();
        assert!(app.net.lan.is_none());
        assert!(app.net.mods_server.is_none());
        assert!(std::net::TcpListener::bind(&address).is_ok());
    }

    #[test]
    fn rejected_candidate_keeps_the_current_room_and_role() {
        let mut app = crate::assemble_app(Args::parse_from(["openomsi"]), Default::default());
        let current = omsi_net::LanSession::host(0, "current", Default::default(), false).unwrap();
        let current_id = current.session;
        app.net.lan = Some(current);
        let mut host = omsi_net::LanSession::host(0, "next", Default::default(), false).unwrap();
        let address = format!("127.0.0.1:{}", host.local_addr().unwrap().port());
        let (stop, done) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            while matches!(done.try_recv(), Err(TryRecvError::Empty)) {
                host.tick(0.01, &Default::default());
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        });
        let candidate =
            PreparedJoin::connect(&address, "player", Default::default(), &Default::default())
                .unwrap();
        let sent = candidate.session.sent();
        app.net.room.ready = Some(candidate);
        app.tick_room_switch(2.5);
        assert!(app.net.room.ready.as_ref().unwrap().session.sent() > sent);
        app.net.room.ready.as_mut().unwrap().session.turned_away = Some("candidate closed".into());
        app.net.room.accept = true;
        app.tick_room_switch(0.02);
        assert_eq!(app.net.lan.as_ref().map(|s| s.session), Some(current_id));
        assert_eq!(app.net.lan.as_ref().unwrap().role, omsi_net::Role::Host);
        assert!(!app.net.room.busy());
        assert!(app.net.room.ready.is_none());
        stop.send(()).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn cancel_drops_only_uncommitted_candidates() {
        let mut state = RoomSwitch {
            accept: true,
            ..Default::default()
        };
        let (tx, rx) = mpsc::channel();
        state.connecting = Some(rx);
        state.cancel();
        assert!(!state.busy());
        assert!(!state.accept);
        assert!(tx.send(Err("cancelled".into())).is_err());
    }
}
