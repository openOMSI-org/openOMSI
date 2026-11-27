use super::*;

impl LanSession {
    pub(super) fn receive_text(
        &mut self,
        parts: &[&str],
        from: SocketAddr,
        n: usize,
        text: &str,
        gone: &mut Vec<u32>,
    ) {
        match (parts[0], self.role) {
            ("DISCOVER", Role::Host) if addrs::is_local_peer(from) => {
                let msg = format!(
                    "HERE|{PROTOCOL}|{}|{}|{}|{}",
                    self.my_name,
                    session_hex(self.session),
                    clean_text(&self.world.map, 260),
                    self.peers.len() + 1
                );
                self.send(msg.as_bytes(), from);
            }
            ("HELLO", Role::Host) => self.on_hello(&parts, from),
            // Ignore unexpected addresses: they must not impersonate a host or
            // end an in-progress join by injecting a fabricated welcome.
            ("WELCOME", Role::Client) if self.accepts_host_source(from) => {
                self.on_welcome(&parts, from)
            }
            ("CHALLENGE", Role::Client)
                if self.host == Some(from)
                    || (self.host.is_none() && self.candidates.contains(&from)) =>
            {
                self.on_challenge(&parts)
            }
            ("REALM", Role::Client)
                if self.host == Some(from)
                    || (self.host.is_none() && self.candidates.contains(&from)) =>
            {
                self.on_realm(&parts)
            }
            // Temporary server-side capacity limits are not permanent rejections.
            ("WAIT", Role::Client)
                if !self.connected
                    && field(&parts, 1).parse::<u32>().ok() == Some(PROTOCOL)
                    && self.accepts_host_source(from) =>
            {
                self.on_waiting_slot(from, field(&parts, 3) == "starting");
            }
            ("REJECT", Role::Client) if self.accepts_host_source(from) => {
                let reason = clean_text(field(&parts, 2), 300);
                // Different candidate IPs may belong to different computers. Keep
                // trying those, but a refusal from the one machine whose ports we
                // are scanning is final, just like an explicit-port refusal.
                if self.host.is_none()
                    && self
                        .candidates
                        .iter()
                        .any(|candidate| candidate.ip() != from.ip())
                {
                    if self.other_reject.is_none() {
                        log::info!(
                            "LAN: {from} turned us away ({reason}); still trying the host's other addresses"
                        );
                    }
                    self.other_reject = Some(format!("{from}: {reason}"));
                    return;
                }
                if self.rejected.is_none() {
                    log::warn!("LAN: the host turned us away: {reason}");
                    self.events.push(LanEvent::Notice(format!(
                        "the host turned us away: {reason}"
                    )));
                }
                // (the game ends on a door closed before it ever played there, or on a ban;
                // a reconnect refused for now - the session full, say - leaves it playing
                // on to `/reconnect` later)
                if self.welcomes == 0 || reason.contains("sent you away") {
                    self.turned_away = Some(reason.clone());
                }
                self.rejected = Some(reason);
                self.connected = false;
            }
            ("INFO", _) => self.on_info(&parts, from),
            ("PLACE", Role::Host) => self.on_place(&parts, from),
            ("NEAR", Role::Client) => {
                if field(&parts, 1).parse::<u32>().ok() == Some(self.my_id) && self.place.is_some()
                {
                    self.near = Some(
                        field(&parts, 2)
                            .split(';')
                            .filter_map(Footprint::decode)
                            .take(MAX_FOOTPRINTS)
                            .collect(),
                    );
                }
            }
            ("CLOCK", Role::Client) => {
                let world = WorldInfo::from_fields(&parts, 1);
                // (after the world's fields: how fast the host's clock runs)
                let speed = field(&parts, 6)
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.0 && *v <= 1000.0)
                    .unwrap_or(1.0);
                self.clock_speed = speed;
                if !world.date.is_empty() {
                    self.host_clock = Some(HostClock {
                        world,
                        at: Instant::now(),
                        speed,
                    });
                }
            }
            ("CHAT", Role::Host) => self.on_chat(&parts, from),
            ("CMD", _) => self.on_command(&parts, from),
            ("KICK", Role::Client) => {
                let reason = clean_text(field(&parts, 1), 200);
                log::warn!("LAN: the host sent us away: {reason}");
                self.events.push(LanEvent::Notice(format!(
                    "the host sent you away: {reason}"
                )));
                self.rejected = Some(format!("the host sent you away: {reason}"));
                self.turned_away = Some(reason);
                self.connected = false;
            }
            ("PAXRETURN", Role::Host) | ("PAXBOARD", Role::Host) => {
                self.on_pax_message(&parts, from)
            }
            ("WANT", Role::Host) | ("CLAIM", Role::Host) => {
                let Some(id) = field(&parts, 1).parse::<u32>().ok() else {
                    return;
                };
                if self
                    .checked_peer(id, from, MESSAGE_RATE, PeerTraffic::Message)
                    .is_none()
                {
                    return;
                }
                if parts[0] == "WANT" {
                    let list = world::EntityRef::parse_list(field(&parts, 2));
                    if !list.is_empty() && self.wants.len() < 64 {
                        self.wants.push((id, list));
                    }
                } else {
                    let list: Vec<u32> = field(&parts, 2)
                        .split(',')
                        .filter_map(|x| x.trim().parse().ok())
                        .take(64)
                        .collect();
                    if !list.is_empty() && self.claims.len() < 64 {
                        self.claims.push((id, list));
                    }
                }
            }
            ("DESC", Role::Host) => {
                let Some(id) = self
                    .peers
                    .iter()
                    .find(|(_, p)| p.addr == Some(from))
                    .map(|(id, _)| *id)
                else {
                    return;
                };
                if self
                    .checked_peer(id, from, MESSAGE_RATE, PeerTraffic::Message)
                    .is_none()
                {
                    return;
                }
                if let Some(d @ world::Desc::Person { .. }) = world::Desc::decode(&parts) {
                    if self.descs_up.len() < 256 {
                        self.descs_up.push((id, d));
                    }
                }
            }
            ("DESC", Role::Client) => {
                if let Some(d) = world::Desc::decode(&parts) {
                    self.world_received += n as u64;
                    if self.descs.len() < 1024 {
                        self.descs.push(d);
                    }
                }
            }
            ("PAXACK", Role::Client) => self.on_pax_ack(&parts, from),
            ("GRANT", Role::Client) | ("DENY", Role::Client) => {
                let list: Vec<u32> = field(&parts, 1)
                    .split(',')
                    .filter_map(|x| x.trim().parse().ok())
                    .take(64)
                    .collect();
                if !list.is_empty() {
                    self.grants.push((list, parts[0] == "GRANT"));
                }
            }
            ("SAY", Role::Client) => {
                let (Some(id), name, text) = (
                    field(&parts, 1).parse::<u32>().ok(),
                    clean_text(field(&parts, 2), MAX_NAME),
                    clean_text(field(&parts, 3), MAX_CHAT),
                ) else {
                    return;
                };
                if !text.is_empty() {
                    log::info!("LAN chat <{name}> {text}");
                    self.events.push(LanEvent::Chat {
                        id,
                        name,
                        text,
                        mine: false,
                    });
                }
            }
            ("NOTE", Role::Client) => {
                let text = clean_text(field(&parts, 1), 200);
                if !text.is_empty() {
                    log::info!("LAN: {text}");
                    self.events.push(LanEvent::Notice(text));
                }
            }
            ("BYE", _) => self.on_bye(&parts, from, text, gone),
            ("HELLO", Role::Client) | ("DISCOVER", Role::Client) => {}
            // an older game that got past the hello somehow (or sends its poses blindly)
            ("POSE", Role::Host) if !self.peers.values().any(|p| p.addr == Some(from)) => {
                self.reject(from, &format!("the host runs LAN protocol {PROTOCOL}, your game an older one - both players need the same version of the game"));
            }
            _ => {}
        }
    }
}
