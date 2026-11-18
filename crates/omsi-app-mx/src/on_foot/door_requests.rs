use super::*;
use omsi_sim::people::DoorRequest;

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

impl App {
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
                omsi_ui::tr("Stand near a bus entry outside, or an exit inside, to request the door").into_owned(),
                3.0,
            ));
            return;
        };
        if let Some(f) = self.session.on_foot.as_mut() {
            f.door_request = Some(target);
            f.door_request_send = 0.0;
        }
        self.service_msg = Some((
            omsi_ui::tr(if target.exit { "Exit requested at door {}" } else { "Entry requested at door {}" })
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

fn parse_request(text: &str, host: bool) -> Option<DoorRequest> {
    let mut words = text.split_whitespace();
    let kind = words.next()?;
    let id = words.next()?.parse::<u64>().ok()?;
    let bus = match (kind, id) {
        ("player", 0) => BusId::Player,
        ("ai", id) if host && id > 0 && id <= omsi_net::world::MAX_ID as u64 => BusId::Ai(id),
        _ => return None,
    };
    let exit = match words.next()? {
        "entry" => false,
        "exit" => true,
        _ => return None,
    };
    let index = words.next()?.parse::<usize>().ok()?;
    if index > u16::MAX as usize || words.next().is_some() {
        return None;
    }
    Some(DoorRequest { bus, exit, index })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_only_target_an_owners_player_bus_or_the_hosts_ai() {
        assert_eq!(
            parse_request("player 0 entry 2", false),
            Some(DoorRequest {
                bus: BusId::Player,
                exit: false,
                index: 2
            })
        );
        assert_eq!(
            parse_request("ai 7 exit 1", true),
            Some(DoorRequest {
                bus: BusId::Ai(7),
                exit: true,
                index: 1
            })
        );
        assert!(parse_request("ai 7 exit 1", false).is_none());
        for invalid in [
            "player 3 exit 1",
            "ai 0 entry 0",
            "ai 1099511627776 entry 0",
            "player 0 entry 65536",
            "player 0 entry 0 extra",
            "player 0 open 0",
        ] {
            assert!(parse_request(invalid, true).is_none());
        }
    }
}
