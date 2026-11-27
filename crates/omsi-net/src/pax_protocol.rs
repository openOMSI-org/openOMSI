//! Passenger claim completion and safe cancellation on top of the LAN UDP
//! session. This is deliberately separate from the session's core wire reader
//! so the established size budget stays enforceable.

use super::*;

impl LanSession {
    /// Return people who were granted but never boarded. The sender must have
    /// removed its local unboarded copies before calling this. It retries until
    /// the server acknowledges each ID; UDP delivery is not guaranteed.
    pub fn return_unboarded(&self, people: &[u32]) {
        if let (Role::Client, Some(host), true) = (self.role, self.host, self.connected) {
            for chunk in people.chunks(64) {
                let ids: Vec<String> = chunk.iter().map(u32::to_string).collect();
                self.send(
                    format!("PAXRETURN|{}|{}", self.my_id, ids.join(",")).as_bytes(),
                    host,
                );
            }
        }
    }

    /// Confirm that granted people have actually entered the player's bus.
    /// This is separate from GRANT and from the aggregate Pose.passengers.
    pub fn confirm_boarded(&self, people: &[u32]) {
        if let (Role::Client, Some(host), true) = (self.role, self.host, self.connected) {
            for chunk in people.chunks(64) {
                let ids: Vec<String> = chunk.iter().map(u32::to_string).collect();
                self.send(
                    format!("PAXBOARD|{}|{}", self.my_id, ids.join(",")).as_bytes(),
                    host,
                );
            }
        }
    }

    /// Answer a client's return request; duplicates are safe and acknowledged.
    pub fn acknowledge_returns(&self, player: u32, people: &[u32]) {
        let Some(to) = self.peer_addr(player).filter(|_| self.role == Role::Host) else {
            return;
        };
        for chunk in people.chunks(64).filter(|chunk| !chunk.is_empty()) {
            let ids: Vec<String> = chunk.iter().map(u32::to_string).collect();
            self.send(format!("PAXACK|{}", ids.join(",")).as_bytes(), to);
        }
    }

    pub fn take_pax_returns(&mut self) -> Vec<(u32, Vec<u32>)> {
        std::mem::take(&mut self.pax_returns)
    }

    pub fn take_pax_boarded(&mut self) -> Vec<(u32, Vec<u32>)> {
        std::mem::take(&mut self.pax_boarded)
    }

    pub fn take_pax_acks(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pax_acks)
    }

    /// Ask the host for the people waiting at a stop who are about to board our bus
    /// (client): it answers with `GRANT` for those it hands over (they leave its world and
    /// become ours) and `DENY` for the others (they boarded another bus meanwhile).
    pub fn claim(&self, people: &[u32]) {
        if let (Role::Client, Some(h), true) = (self.role, self.host, self.connected) {
            for chunk in people.chunks(64) {
                let ids: Vec<String> = chunk.iter().map(|i| i.to_string()).collect();
                self.send(
                    format!("CLAIM|{}|{}", self.my_id, ids.join(",")).as_bytes(),
                    h,
                );
            }
        }
    }

    /// Answer a player's claim (host).
    pub fn answer_claim(&self, id: u32, granted: &[u32], denied: &[u32]) {
        let Some(to) = self.peer_addr(id) else {
            return;
        };
        for (word, list) in [("GRANT", granted), ("DENY", denied)] {
            for chunk in list.chunks(64).filter(|c| !c.is_empty()) {
                let ids: Vec<String> = chunk.iter().map(|i| i.to_string()).collect();
                self.send(format!("{word}|{}", ids.join(",")).as_bytes(), to);
            }
        }
    }

    pub(super) fn on_pax_message(&mut self, parts: &[&str], from: SocketAddr) {
        let Some(id) = field(parts, 1).parse::<u32>().ok() else {
            return;
        };
        if self
            .checked_peer(id, from, MESSAGE_RATE, PeerTraffic::Message)
            .is_none()
        {
            return;
        }
        let ids: Vec<u32> = field(parts, 2)
            .split(',')
            .filter_map(|value| value.trim().parse().ok())
            .take(64)
            .collect();
        if ids.is_empty() {
            return;
        }
        if parts[0] == "PAXRETURN" {
            if self.pax_returns.len() < 64 {
                self.pax_returns.push((id, ids));
            }
        } else if self.pax_boarded.len() < 64 {
            self.pax_boarded.push((id, ids));
        }
    }

    pub(super) fn on_pax_ack(&mut self, parts: &[&str], from: SocketAddr) {
        // A random UDP sender must never cancel an outstanding client retry.
        if Some(from) != self.host {
            return;
        }
        let ids: Vec<u32> = field(parts, 1)
            .split(',')
            .filter_map(|value| value.trim().parse().ok())
            .take(64)
            .collect();
        // Bound accumulated ACK memory even if the host sends multiple pages.
        let remain = 1024_usize.saturating_sub(self.pax_acks.len());
        self.pax_acks.extend(ids.into_iter().take(remain));
    }
}
