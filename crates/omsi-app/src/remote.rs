use super::*;
use omsi_remote::cockpit::{Cockpit, Tick};
use omsi_remote::Command;
use omsi_sim::htmltex::HtmlRequest;

pub(crate) type RemoteCockpit = Cockpit;

pub(crate) fn lan_ip() -> String {
    use omsi_net::addrs::AddrKind;
    let addrs = omsi_net::addrs::joinable_addresses();
    let best = addrs.iter().find(|a| a.kind == AddrKind::Lan).or_else(|| addrs.iter().find(|a| !a.kind.is_vpn())).or(addrs.first());
    best.map(|a| a.ip.to_string()).unwrap_or_else(|| "127.0.0.1".into())
}

pub(crate) fn dir() -> Option<std::path::PathBuf> {
    crate::settings::Settings::path().and_then(|p| p.parent().map(|d| d.join("remote")))
}

impl App {
    pub(crate) fn remote_description(&self) -> String {
        match self.integrations.remote.server.as_ref() {
            Some(r) => format!("Open {} on a phone or tablet of the same network", format!("http://{}:{}", lan_ip(), r.port())),
            None if self.integrations.remote.failed() => format!("Port {} is taken by another program", self.settings.remote_cockpit_port),
            None => "The bus's switches, its destination sign and a dashboard on a phone or tablet of the same network".into(),
        }
    }

    fn remote_switch(&mut self) {
        let wanted = self.settings.remote_cockpit && self.args.server.is_none();
        let port = self.settings.remote_cockpit_port;
        if self.integrations.remote.must_stop(wanted, port) {
            for c in self.integrations.remote.stop() {
                self.remote_command(c);
            }
            if let Some(n) = self.menus.navigator.as_mut() {
                n.remote = false;
                n.forget_remote_area();
            }
        }
        match wanted {
            true => self.integrations.remote.start(port, dir),
            false => self.integrations.remote.forget_failure(),
        }
    }

    pub(crate) fn remote_input(&mut self) {
        let Some(server) = self.integrations.remote.server.as_ref() else { return };
        for c in server.take_commands() {
            self.remote_command(c);
        }
        let r = &mut self.integrations.remote;
        let Some(server) = r.server.as_ref() else { return };
        let drive = server.drive();
        r.phone_steers &= drive.is_some();
        if let (Some(d), Some(p), false) = (drive, self.player.as_mut(), self.paused) {
            let a = &mut p.analog;
            r.phone_steers = omsi_remote::cockpit::phone_steers(r.phone_steers, d.held, a.steering.is_some() || p.axes.left_key || p.axes.right_key);
            if r.phone_steers {
                a.steering = Some(d.steering);
            }
            a.stick = false;
            a.throttle = Some(a.throttle.unwrap_or(0.0).max(d.throttle));
            a.brake = Some(a.brake.unwrap_or(0.0).max(d.brake));
        }
    }

    fn remote_command(&mut self, c: Command) {
        if let Command::Game(name) = &c {
            if omsi_input::actions::is_game_action(name) {
                self.game_action(name);
            }
            return;
        }
        if let Command::Indicator(v @ 1..=3) = c {
            self.blinker(v);
            return;
        }
        if let (Command::Action { name, down }, Some(p)) = (&c, self.player.as_mut()) {
            if let Some(fire) = self.integrations.remote.direct_doors(name, *down, &p.vehicle) {
                for t in fire {
                    p.action(&t, *down);
                }
                return;
            }
        }
        let duty = self.session.duty.as_ref().map(|d| d.trip_for_ibis());
        let Some(p) = self.player.as_mut() else { return };
        let request = match c {
            Command::Action { name, down } => {
                match omsi_sim::engine_action(&name) {
                    Some(a) => p.axes.set(a, down),
                    None => {
                        p.action(&name, down);
                    }
                }
                return;
            }
            Command::StartUp => {
                self.service_msg = Some((p.start_up(), 6.0));
                if let Some((trip, stop)) = duty {
                    p.set_duty_destination(trip, stop);
                }
                return;
            }
            Command::SaloonLights => {
                self.service_msg = Some((p.toggle_saloon_lights(), 3.0));
                return;
            }
            Command::Route(i) => HtmlRequest::SetRoute(i),
            Command::Line(l) => HtmlRequest::SetLine(l),
            Command::Destination(i) => HtmlRequest::SetDestination(i),
            Command::ClearLine => HtmlRequest::ClearLine,
            Command::NextStop(i) => HtmlRequest::SetNextStop(i),
            Command::Indicator(_) => {
                p.blinker_key_state = 0;
                p.action("blinker_off", true);
                p.action("blinker_off", false);
                return;
            }
            Command::Game(_) => return,
        };
        if p.vehicle.host.html_requests.len() < 32 {
            p.vehicle.host.html_requests.push(request);
        }
    }

    pub(crate) fn frame_remote(&mut self, dt: f32) {
        self.remote_switch();
        let r = &mut self.integrations.remote;
        let Some(server) = r.server.as_mut() else { return };
        server.set_pairing(self.menus.game_panels.remote.is_some());
        if server.take_paired() && self.menus.game_panels.remote.take().is_some() {
            self.service_msg = Some((omsi_ui::tr("Phone or tablet connected").into_owned(), 4.0));
        }
        let lang = omsi_sim::vehicle_api::locale();
        if r.lang != lang {
            let texts: Vec<(&str, String)> = omsi_remote::TEXTS.iter().map(|k| (*k, omsi_ui::tr(k).into_owned())).collect();
            server.set_texts(&lang, &texts);
            r.lang = lang;
        }
        let watched = server.clients() > 0;
        if let Some(n) = self.menus.navigator.as_mut() {
            n.remote = watched;
        }
        if !watched {
            return;
        }
        r.nav_t -= dt;
        if let (Some(n), Some(p), true) = (self.menus.navigator.as_mut(), self.player.as_ref(), r.nav_t <= 0.0) {
            r.nav_t = 0.4;
            if let Some((anchor, roads)) = n.remote_roads(p.vehicle.position) {
                server.publish("roads", omsi_remote::nav::roads_message(anchor, &roads));
            }
            if let Some(i) = n.remote_info() {
                let turn = i.turn.map(|(direction, angle, metres, street)| omsi_remote::nav::Turn { direction, angle, metres, street });
                let info = omsi_remote::nav::NavInfo { anchor: i.anchor, route: i.route, turn, stop: i.stop, stops: i.stops, street: i.street, limit: i.limit, on_route: i.on_route };
                server.publish("nav", omsi_remote::nav::nav_message(&info));
            }
        }
        let p = self.player.as_ref();
        let tick = Tick { dt, root: &self.args.root, paused: self.paused, doors_idle: p.is_some_and(|p| p.door_buttons.is_empty()), action_aliases: crate::player::ACTION_ALIASES };
        let describe = |v: &omsi_sim::VehicleInstance| {
            let def = &v.ty.def;
            let short = omsi_launcher_lib::vehicle_type_label(&def.type_name, &def.path);
            (omsi_launcher_lib::display_bus_name(&format!("{} {short}", def.manufacturer)), p.map(|p| p.bound_actions()).unwrap_or_default())
        };
        let clock = &self.clock;
        r.frame(tick, p.map(|p| &p.vehicle), describe, || omsi_sim::vehicle_api::environment(clock, &omsi_sim::vehicle_api::locale()));
    }
}
