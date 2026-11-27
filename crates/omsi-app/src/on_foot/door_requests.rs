use super::*;
use omsi_lan::door_requests::parse_request;

const REACH: f64 = 3.2;
const SEND_EVERY: f32 = 0.2;
const LEASE: f32 = 1.0;

/// Use the cabin position when aboard: the bus may have moved since the walker tick.
fn location(f: &OnFoot, h: &Humans) -> (DVec3, Option<BusId>) {
    let inside = f.seat.map(|s| s.0).or(f.inside.map(|i| i.0));
    let local = f
        .seat
        .and_then(|(bus, seat)| h.seat_stand(bus, seat))
        .or(f.inside.map(|i| i.1));
    let at = inside
        .zip(local)
        .and_then(|(bus, local)| {
            h.last_buses
                .iter()
                .find(|b| b.id == bus)
                .map(|b| b.world(local))
        })
        .unwrap_or(f.pos);
    (at, inside)
}

/// Built-in conductor's two-tone bell. It is not tied to vehicle script events:
/// the signal is available on modded buses that lack a dedicated door-close gong.
fn coop_departure_clip() -> std::sync::Arc<omsi_audio::Clip> {
    static CLIP: std::sync::OnceLock<std::sync::Arc<omsi_audio::Clip>> = std::sync::OnceLock::new();
    CLIP.get_or_init(|| {
        let sample_rate = 48_000u32;
        let chirp = (sample_rate as usize * 14) / 100;
        let pause = (sample_rate as usize * 8) / 100;
        let mut samples = vec![0i16; chirp * 2 + pause];
        for (start, hz) in [(0, 780.0f32), (chirp + pause, 990.0)] {
            for i in 0..chirp {
                let fade = (i.min(chirp - i - 1) as f32 / 360.0).min(1.0);
                let angle = std::f32::consts::TAU * hz * i as f32 / sample_rate as f32;
                samples[start + i] = (angle.sin() * fade * 10_000.0) as i16;
            }
        }
        std::sync::Arc::new(omsi_audio::Clip {
            sample_rate,
            channels: 1,
            samples,
        })
    })
    .clone()
}

impl App {
    pub(crate) fn play_coop_departure_signal(&self) {
        if let Some(audio) = self.sound.audio.as_ref() {
            audio.play(
                coop_departure_clip(),
                omsi_audio::VoiceParams {
                    gain: 0.65,
                    ..Default::default()
                },
            );
        }
    }

    pub(crate) fn request_coop_departure_signal(&mut self) {
        if self.view == "foot" && self.net.inside_remote.is_some() {
            self.request_coop_cabin(omsi_lan::coop_permissions::CabinAction::DepartureSignal);
        }
    }

    pub(crate) fn remote_cabin_owner(&self) -> Option<u32> {
        let BusId::Ai(id) = self.foot_bus()? else {
            return None;
        };
        crate::humans::remote_bus_player(id)
    }

    pub(super) fn request_coop_cabin(&mut self, action: omsi_lan::coop_permissions::CabinAction) {
        let Some(owner) = self.remote_cabin_owner() else {
            return;
        };
        let Some(lan) = self.net.lan.as_mut() else {
            return;
        };
        if owner == lan.my_id {
            return;
        }
        lan.command(owner, &action.command());
    }

    /// Forward a physical key only in the assistant's explicit keyboard mode.
    /// The owning client resolves its own bindings and enforces cabin-only access.
    pub(crate) fn coop_keyboard_key(
        &mut self,
        code: winit::keyboard::KeyCode,
        pressed: bool,
        repeat: bool,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) -> bool {
        if !self.net.coop_keyboard_mode || self.view != "foot" || self.net.inside_remote.is_none() {
            return false;
        }
        if pressed && !repeat {
            if let Some(scan) = omsi_input::keys::dik_code(code) {
                let modifier = omsi_content::input::chord(shift, ctrl, alt)
                    & !omsi_input::keys::own_modifier_bit(code);
                self.request_coop_cabin(omsi_lan::coop_permissions::CabinAction::Key(
                    scan, modifier,
                ));
            }
        }
        true
    }

    pub(crate) fn coop_fare_key(&mut self, ctrl: bool, shift: bool) -> bool {
        use omsi_lan::coop_permissions::{CabinAction, FareAction};
        if self.view != "foot" || self.net.inside_remote.is_none() || (ctrl && shift) {
            return false;
        }
        let fare = match (ctrl, shift) {
            (true, _) => FareAction::GiveChange,
            (_, true) => FareAction::CollectPayment,
            _ => FareAction::IssueTicket,
        };
        self.request_coop_cabin(CabinAction::Fare(fare));
        true
    }

    pub(crate) fn apply_coop_fare(&mut self, action: omsi_lan::coop_permissions::FareAction) {
        use omsi_lan::coop_permissions::FareAction;
        let Some((p, people)) = self.player.as_mut().zip(self.session.humans.as_mut()) else {
            return;
        };
        match action {
            FareAction::IssueTicket if people.desk_busy.is_some() && people.paid.is_some() => {
                p.give_ticket = true;
            }
            FareAction::GiveChange
                if people.change_due.is_some_and(|owed| owed > 0.0)
                    && people
                        .money
                        .as_ref()
                        .is_some_and(|money| money.change_count() == 0) =>
            {
                p.give_change = true;
            }
            FareAction::CollectPayment => {
                if let Some(money) = people.money.as_mut() {
                    money.clear(false);
                }
            }
            _ => {}
        }
    }

    /// A short press uses the switch. A drag gives registered rotary/sliding
    /// controls one bounded adjustment once the mouse is released.
    pub(crate) fn click_remote_cabin(&mut self, pressed: bool) {
        use omsi_lan::coop_permissions::{cabin_action_for_model, cabin_adjustment_for_model};
        if pressed {
            self.net.coop_cab_press = self
                .remote_cabin_switch()
                .zip(self.remote_cabin_owner())
                .map(|((_, event), owner)| (owner, event.to_owned(), self.input.cursor));
            return;
        }
        let Some((owner, event, start)) = self.net.coop_cab_press.take() else {
            return;
        };
        let (dx, dy) = (self.input.cursor.0 - start.0, self.input.cursor.1 - start.1);
        let movement = if dx.abs() > dy.abs() { dx } else { dy };
        let Some(vehicle) = self
            .remote_cabin_owner()
            .filter(|&id| id == owner)
            .and_then(|id| self.net.remotes.remotes.get(&id))
            .map(|remote| remote.vehicle())
        else {
            return;
        };
        let adjusted = (movement.abs() >= 4.0)
            .then(|| cabin_adjustment_for_model(&vehicle.ty, &event, movement.signum() as i8))
            .flatten();
        if let Some(action) = adjusted.or_else(|| cabin_action_for_model(&vehicle.ty, &event)) {
            self.request_coop_cabin(action);
        }
    }

    fn remote_cabin_switch(&self) -> Option<(&omsi_sim::VehicleInstance, &str)> {
        let owner = self.remote_cabin_owner()?;
        let (camera, surface) = self.camera.as_ref().zip(self.gfx.surface.as_ref())?;
        let (origin, direction, spread) =
            self.cockpit_cursor_ray(camera, (surface.config.width, surface.config.height));
        let vehicle = self.net.remotes.remotes.get(&owner)?.vehicle();
        let (mesh, _) = crate::player::pick_in(
            vehicle,
            origin,
            direction,
            spread,
            crate::player::Reach(REACH as f32),
            false,
        )?;
        let event = vehicle
            .ty
            .model
            .meshes
            .get(vehicle.ty.meshes[mesh].def_index)?
            .mouse_event
            .as_deref()?;
        Some((vehicle, event))
    }

    pub(crate) fn request_passenger_door(&mut self) {
        let target = self
            .session
            .on_foot
            .as_ref()
            .zip(self.session.humans.as_ref())
            .and_then(|(f, h)| {
                if f.transit.is_some() {
                    return None;
                }
                let (at, inside) = location(f, h);
                h.nearest_door_request(at, inside, REACH)
            });
        let Some(target) = target else {
            self.service_msg = Some((
                omsi_ui::tr(
                    "Stand near a bus entry outside, or an exit inside, to request the door",
                )
                .into_owned(),
                3.0,
            ));
            return;
        };
        if let Some(f) = self.session.on_foot.as_mut() {
            f.door_request = Some(target);
            f.door_request_send = 0.0;
        }
        self.service_msg = Some((
            omsi_ui::tr(if target.exit {
                "Exit requested at door {}"
            } else {
                "Entry requested at door {}"
            })
            .replacen("{}", &(target.index + 1).to_string(), 1),
            3.0,
        ));
    }

    pub(crate) fn tick_passenger_door_request(&mut self, dt: f32) {
        let target = self
            .session
            .on_foot
            .as_ref()
            .zip(self.session.humans.as_ref())
            .and_then(|(f, h)| {
                let request = f.door_request?;
                let (at, inside) = location(f, h);
                (f.transit.is_none()
                    && h.door_request_near(request, at, inside, REACH)
                    && !h.requested_door_open(request))
                .then_some(request)
            });
        let Some(target) = target else {
            if let Some(h) = self.session.humans.as_mut() {
                h.clear_manual_door_request(0);
            }
            if let Some(f) = self.session.on_foot.as_mut() {
                f.door_request = None;
            }
            return;
        };
        let remote = match (target.bus, self.net.lan.as_ref()) {
            (BusId::Ai(id), Some(lan)) => crate::humans::remote_bus_player(id)
                .map(|owner| (owner, "player", 0))
                .or_else(|| {
                    (lan.role == omsi_net::Role::Client && id <= omsi_net::world::MAX_ID as u64)
                        .then_some((1, "ai", id))
                }),
            _ => None,
        };
        if let Some((owner, kind, id)) = remote {
            if let Some(h) = self.session.humans.as_mut() {
                h.clear_manual_door_request(0);
            }
            let Some(f) = self.session.on_foot.as_mut() else {
                return;
            };
            f.door_request_send -= dt.max(0.0);
            if f.door_request_send <= 0.0 {
                f.door_request_send = SEND_EVERY;
                if let Some(lan) = self.net.lan.as_mut() {
                    lan.command(
                        owner,
                        &format!(
                            "pax_request {kind} {id} {} {}",
                            if target.exit { "exit" } else { "entry" },
                            target.index
                        ),
                    );
                }
            }
        } else if let Some(h) = self.session.humans.as_mut() {
            h.set_manual_door_request(0, target, LEASE);
        }
    }

    /// CMD already authenticates and routes the source. Only the vehicle's simulation
    /// owner applies a request, after checking the sender's latest walker position.
    pub(crate) fn receive_passenger_door_request(&mut self, from: u32, text: &str) {
        let Some(lan) = self.net.lan.as_ref() else {
            return;
        };
        let Some(walker) = lan
            .peers()
            .find(|p| p.pose.id == from)
            .and_then(|p| p.pose.walker)
        else {
            return;
        };
        let Some(request) = parse_request(text, lan.role == omsi_net::Role::Host) else {
            return;
        };
        let Some(h) = self.session.humans.as_mut() else {
            return;
        };
        let mut at = DVec3::new(walker.x, walker.y, walker.z);
        let inside = if request.exit {
            match request.bus {
                BusId::Player => {
                    let Some(aboard) = walker.aboard.filter(|a| a.owner == lan.my_id) else {
                        return;
                    };
                    let Some(bus) = h.last_buses.iter().find(|b| b.id == BusId::Player) else {
                        return;
                    };
                    at = bus.world(glam::Vec3::from_array(aboard.local));
                }
                // AI riders' walker poses are world positions; AI buses are host-owned.
                BusId::Ai(_) if walker.aboard.is_none() => {}
                _ => return,
            }
            Some(request.bus)
        } else {
            if walker.aboard.is_some() {
                return;
            }
            None
        };
        // Allow one metre for the delay between the remote avatar and bus snapshots.
        if h.door_request_near(request, at, inside, REACH + 1.0) && !h.requested_door_open(request)
        {
            h.set_manual_door_request(from, request, LEASE);
        }
    }
}
