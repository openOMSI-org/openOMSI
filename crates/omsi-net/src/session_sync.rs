//! Script textures, sound events and variable synchronization for a session.
use super::*;

impl LanSession {
    pub(super) fn on_image_packet(&mut self, data: &[u8], from: SocketAddr) -> bool {
        match data.first() {
            Some(&scripttex::MAGIC) => self.on_script_texture(data, from),
            Some(&freetex::MAGIC) => self.on_freetex(data, from),
            _ => return false,
        }
        true
    }

    /// Released surfaces of our own vehicle. The transport retains an acknowledged
    /// baseline per subscriber and sends only pixel changes after initialization.
    pub fn set_script_textures(&mut self, table: u32, images: Vec<Option<scripttex::Image>>) {
        self.script_textures.set_local(table, images);
    }

    pub fn clear_script_textures(&mut self) {
        self.script_textures.clear_local();
    }

    pub fn subscribe_script_textures(&mut self, owner: u32, table: u32) -> u64 {
        self.script_textures.subscribe(owner, table)
    }

    pub fn unsubscribe_script_textures(&mut self, owner: u32) {
        self.script_textures.unsubscribe(owner);
    }

    pub fn take_script_textures(&mut self) -> Vec<scripttex::Incoming> {
        self.script_textures.take_incoming()
    }

    pub fn publish_sound(&mut self, event: sounds::Event) {
        if !self.connected || (event.ai != 0 && self.role != Role::Host) { return; }
        let peers: Vec<u32> = self.peers.keys().copied().collect();
        self.sounds.publish(&peers, event);
    }

    pub fn take_sounds(&mut self) -> Vec<sounds::Incoming> {
        self.sounds.take_incoming().into_iter().filter(|s| s.event.ai == 0 || s.owner == 1).collect()
    }

    pub(super) fn route_sound(&self, data: &[u8]) {
        let Some(a) = sounds::address(data, PROTOCOL as u8) else { return; };
        match self.role {
            Role::Host => if let Some(to) = self.peers.get(&a.to).and_then(|p| p.addr) { self.send(data, to); },
            Role::Client => if let Some(host) = self.host { self.send(data, host); },
        }
    }

    pub(super) fn on_sound(&mut self, data: &[u8], from: SocketAddr) {
        let Some(a) = sounds::address(data, PROTOCOL as u8) else { return; };
        if a.from == self.my_id || a.from == 0 || a.to == 0 { return; }
        match self.role {
            Role::Host => {
                if self.checked_peer(a.from, from, SOUND_RATE, PeerTraffic::Sounds).is_none() { return; }
                if a.to != self.my_id { self.route_sound(data); return; }
            }
            Role::Client => if a.to != self.my_id || !self.peers.contains_key(&a.from) { return; },
        }
        if let Some(reply) = self.sounds.receive(PROTOCOL as u8, self.my_id, data) { self.route_sound(&reply); }
    }

    pub(super) fn route_script_texture(&self, data: &[u8]) {
        let Some(a) = scripttex::address(data, PROTOCOL as u8) else { return };
        match self.role {
            Role::Host => {
                if let Some(to) = self.peers.get(&a.to).and_then(|p| p.addr) {
                    self.send(data, to);
                }
            }
            Role::Client => {
                if let Some(host) = self.host { self.send(data, host); }
            }
        }
    }

    pub(super) fn on_script_texture(&mut self, data: &[u8], from: SocketAddr) {
        let Some(a) = scripttex::address(data, PROTOCOL as u8) else { return };
        if a.from == self.my_id || a.from == 0 || a.to == 0 { return; }
        match self.role {
            Role::Host => {
                if self.checked_peer(a.from, from, TEXTURE_RATE, PeerTraffic::Textures).is_none() { return; }
                if a.to != self.my_id {
                    self.route_script_texture(data);
                    return;
                }
            }
            Role::Client => {
                if a.to != self.my_id || !self.peers.contains_key(&a.from) { return; }
            }
        }
        for reply in self.script_textures.receive(PROTOCOL as u8, self.my_id, data) {
            self.route_script_texture(&reply);
        }
    }

    /// Every script variable of our vehicle (see `vars.rs`): `float_ids[k]` holds
    /// `floats[k]`, the same for the strings, `table` names them (a hash of their names). Called
    /// every frame; it sends what is due.
    #[allow(clippy::too_many_arguments)]
    pub fn send_vars(&mut self, table: u32, float_ids: &[u16], floats: &[f32], string_ids: &[u16], strings: &[String], dt: f32) {
        self.var_sender.subscribers(&self.peers.keys().copied().collect::<Vec<_>>());
        let msgs = self.var_sender.tick(PROTOCOL as u8, self.my_id, table, float_ids, floats, string_ids, strings, dt);
        for m in msgs {
            match self.role {
                Role::Host => self.broadcast(&m, None),
                Role::Client => {
                    if let Some(h) = self.host {
                        self.send(&m, h);
                    }
                }
            }
        }
    }

    /// Model animation variables, sampled on the same clock as the vehicle pose.
    pub fn send_animation_vars(&mut self, table: u32, values: &[(u16, f32)], dt: f32) {
        let sent_ms = self.stamp_ms();
        for packet in self.animation_sender.tick(PROTOCOL as u8, self.my_id, table, sent_ms, values, dt) {
            match self.role {
                Role::Host => self.broadcast(&packet, None),
                Role::Client => { if let Some(host) = self.host { self.send(&packet, host); } }
            }
        }
    }

    /// Repeat the name manifest using independently recovered string fragments.
    pub fn send_var_schema(&mut self, table: u32, schema: &str, dt: f32) {
        self.schema_sender.subscribers(&self.peers.keys().copied().collect::<Vec<_>>());
        let texts = [schema.to_owned()];
        for mut packet in self.schema_sender.tick(PROTOCOL as u8, self.my_id, table, &[], &[], &[0], &texts, dt) {
            packet[0] = var_schema::MAGIC;
            match self.role {
                Role::Host => self.broadcast(&packet, None),
                Role::Client => { if let Some(host) = self.host { self.send(&packet, host); } }
            }
        }
    }

    pub fn take_var_schemas(&mut self) -> Vec<vars::VarsIn> { std::mem::take(&mut self.schemas_in) }

    /// Host-only AI script context pages, alongside the shared world poses.
    pub fn send_ai_context(&self, to: u32, data: &[u8]) {
        if self.role != Role::Host || data.len() > MAX_DATAGRAM { return; }
        if let Some(addr) = self.peers.get(&to).and_then(|p| p.addr) { self.send(data, addr); }
    }

    pub fn take_ai_contexts(&mut self) -> Vec<Vec<u8>> { std::mem::take(&mut self.ai_context_in) }

    /// The others' vehicle variables that came since the last call, oldest first.
    pub fn take_vars(&mut self) -> Vec<vars::VarsIn> {
        std::mem::take(&mut self.vars_in)
    }

    pub(super) fn on_vars(&mut self, data: &[u8], from: SocketAddr) {
        self.on_variable_packet(data, from, false);
    }

    pub(super) fn on_variable_packet(&mut self, data: &[u8], from: SocketAddr, schema: bool) {
        let Some(v) = vars::decode(data, PROTOCOL as u8) else { return };
        if v.id == self.my_id || v.id == 0 {
            return;
        }
        match self.role {
            // (as a state: only a player we welcomed, from its address, within its rate)
            Role::Host => {
                let (rate, traffic) = if !schema && v.sent_ms != 0 {
                    (ANIMATION_RATE, PeerTraffic::Animations)
                } else {
                    (VAR_RATE, PeerTraffic::Variables)
                };
                if self.checked_peer(v.id, from, rate, traffic).is_none() {
                    return;
                }
                if schema {
                    let mut packet = data.to_vec();
                    packet[0] = var_schema::MAGIC;
                    self.broadcast(&packet, Some(v.id));
                } else {
                    self.broadcast(data, Some(v.id));
                }
            }
            Role::Client => {
                if !self.peers.contains_key(&v.id) {
                    return;
                }
            }
        }
        let receiver = if schema { &mut self.schema_receiver } else { &mut self.var_receiver };
        let Some(v) = receiver.receive(data, PROTOCOL as u8) else { return };
        if let Some((index, version)) = receiver.completed_fragment(data) {
            let mut ack = vec![0xEB, PROTOCOL as u8];
            ack.extend_from_slice(&self.my_id.to_le_bytes());
            ack.extend_from_slice(&v.id.to_le_bytes());
            ack.extend_from_slice(&v.table.to_le_bytes());
            ack.extend_from_slice(&index.to_le_bytes());
            ack.extend_from_slice(&version.to_le_bytes());
            ack.push(schema as u8);
            self.route_string_ack(&ack, v.id);
        }
        if schema {
            if self.schemas_in.len() < 128 && !v.strings.is_empty() { self.schemas_in.push(v); }
            return;
        }
        // (a game that stopped reading does not pile them up without end)
        if self.vars_in.len() < 1024 {
            self.vars_in.push(v);
        }
    }

}

/// Bind versioned traffic to the freshly issued connection secret. Relays unwrap and
/// rewrap for the destination, so old sessions cannot poison a retained peer's state.
pub(super) fn session_bound(data: &[u8]) -> bool {
    data.first().is_some_and(|b| [sounds::MAGIC, voice_net::MAGIC, vars::VARS_MAGIC, var_schema::MAGIC, 0xEB].contains(b))
}

impl LanSession {
    pub(super) fn reset_peer_sync(&mut self, id: u32) {
        let keep: Vec<_> = self.peers.keys().copied().filter(|p| *p != id).collect();
        self.sounds.retain_peers(&keep);
        self.var_receiver.retain_peers(&keep);
        self.schema_receiver.retain_peers(&keep);
        self.vars_in.retain(|v| v.id != id);
        self.schemas_in.retain(|v| v.id != id);
        if self.role == Role::Client {
            self.var_sender = vars::VarSender::default();
            self.schema_sender = vars::VarSender::default();
            self.animation_sender = vars::AnimationSender::default();
        }
    }
}

impl LanSession {
    fn route_string_ack(&self, data: &[u8], to: u32) {
        let addr = match self.role {
            Role::Host => self.peers.get(&to).and_then(|p| p.addr),
            Role::Client => self.host,
        };
        if let Some(addr) = addr { self.send(data, addr); }
    }

    pub(super) fn on_string_ack(&mut self, data: &[u8], from: SocketAddr) {
        if data.len() != 25 || data[1] != PROTOCOL as u8 || data[24] > 1 { return; }
        let sender = u32::from_le_bytes(data[2..6].try_into().unwrap());
        let to = u32::from_le_bytes(data[6..10].try_into().unwrap());
        if self.role == Role::Host && self.checked_peer(sender, from, VAR_RATE, PeerTraffic::Variables).is_none() { return; }
        if to != self.my_id {
            if self.role == Role::Host { self.route_string_ack(data, to); }
            return;
        }
        let table = u32::from_le_bytes(data[10..14].try_into().unwrap());
        let index = u16::from_le_bytes(data[14..16].try_into().unwrap());
        let version = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let tx = if data[24] == 1 { &mut self.schema_sender } else { &mut self.var_sender };
        tx.acknowledge(sender, table, index, version);
    }
}

impl LanSession {
    pub(super) fn send(&self, data: &[u8], to: SocketAddr) {
        let mut framed = Vec::new();
        if session_sync::session_bound(data) {
            let secret = match self.role {
                Role::Client => self.rebind_secret,
                Role::Host => self.peers.values().find(|p| p.addr == Some(to)).map(|p| p.secret),
            };
            let Some(secret) = secret else { return; };
            framed.push(0xEA);
            framed.extend_from_slice(&secret);
            framed.extend_from_slice(data);
        }
        let data = if framed.is_empty() { data } else { &framed };
        if self.socket.send_to(data, to).is_ok() {
            self.sent.set(self.sent.get() + data.len() as u64);
        }
    }

}

impl LanSession {
    pub(super) fn welcome_session(&self, to: SocketAddr, named: bool) -> String {
        if named || self.strangers_told || addrs::is_local_peer(to) {
            session_hex(self.session)
        } else {
            "-".to_string()
        }
    }

    pub(super) fn send_welcome(&self, id: u32, to: SocketAddr, named: bool) {
        let session = self.welcome_session(to, named);
        let msg = format!(
            "WELCOME|{PROTOCOL}|{id}|{}|{}|{}|{}",
            session,
            self.my_name,
            self.world.fields(),
            self.peers.len() + 1
        );
        let msg = match self.peers.get(&id) {
            Some(p) if p.home == Some(to) => format!("{msg}|{}|{:016x}", admission::secret_hex(&p.secret), p.nonce.unwrap_or(0)),
            _ => msg,
        };
        self.send(msg.as_bytes(), to);
    }

}
