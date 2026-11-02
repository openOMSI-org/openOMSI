//! Bounded, fair multiplayer join admission.
//! Separate from the packet parser to respect the project's file/function budgets.

use super::*;

impl LanSession {
    /// Only the address currently being contacted (or an advertised rendezvous
    /// candidate before an address wins) may answer a joining player.
    pub(super) fn accepts_host_source(&self, from: SocketAddr) -> bool {
        self.host == Some(from)
            || (self.host.is_none() && self.candidates.contains(&from))
    }

    pub(super) fn on_waiting_slot(&mut self, from: SocketAddr) {
        if !self.waiting_for_slot {
            log::info!("LAN: {from} is admitting other players; retrying automatically");
            self.events.push(LanEvent::Notice(
                "the host is admitting other players; waiting for a loading slot".into(),
            ));
        }
        self.waiting_for_slot = true;
    }

    /// Returns true if the incoming HELLO must retry later.
    /// Stale entries expire after six seconds; new entries have a bounded FIFO.
    pub(super) fn defer_new_join(&mut self, from: SocketAddr, supports_wait: bool) -> bool {
        let now = Instant::now();
        self.waiting_joins.retain(|(_, seen)| now.duration_since(*seen) < Duration::from_secs(6));
        let queued = self.waiting_joins.iter().position(|(addr, _)| *addr == from);
        if let Some(i) = queued {
            self.waiting_joins[i].1 = now;
        } else if self.peers.values().filter(|p| !p.has_pose).count() >= MAX_JOINING
            || !self.waiting_joins.is_empty()
        {
            if !supports_wait {
                self.reject(from, "the host is admitting other players - try again in a minute");
                return true;
            }
            if self.waiting_joins.iter().filter(|(a, _)| a.ip() == from.ip()).count() >= 32 {
                self.reject(from, "too many players from this address are waiting - retry later");
                return true;
            }
            if self.waiting_joins.len() >= MAX_PEERS {
                self.reject(from, "the host has too many players waiting to join - retry later");
                return true;
            }
            self.waiting_joins.push_back((from, now));
        }
        let active_loading = self.peers.values().filter(|p| !p.has_pose).count();
        if active_loading >= MAX_JOINING
            || self.waiting_joins.front().is_some_and(|(addr, _)| *addr != from)
        {
            self.send(format!("WAIT|{PROTOCOL}|2").as_bytes(), from);
            return true;
        }
        if self.waiting_joins.front().is_some_and(|(addr, _)| *addr == from) {
            self.waiting_joins.pop_front();
        }
        false
    }
}
