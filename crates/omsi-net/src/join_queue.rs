//! Bounded, fair multiplayer join admission.
//! Separate from the packet parser to respect the project's file/function budgets.

use super::*;

/// A loading player heard from less than this long ago holds a loading slot; one silent
/// longer (its game closed or stuck) no longer keeps the others waiting - it is still timed
/// out after `LOAD_TIMEOUT`.
pub const LOADING_ALIVE: Duration = Duration::from_secs(15);

impl LanSession {
    /// Only the address currently being contacted (or an advertised rendezvous
    /// candidate before an address wins) may answer a joining player.
    pub(super) fn accepts_host_source(&self, from: SocketAddr) -> bool {
        self.host == Some(from)
            || (self.host.is_none() && self.candidates.contains(&from))
    }

    pub(super) fn on_waiting_slot(&mut self, from: SocketAddr, starting: bool) {
        if !self.waiting_for_slot {
            if starting {
                log::info!("LAN: {from} is still loading its world; retrying automatically");
                self.events.push(LanEvent::Notice("the server is still loading its world; waiting for it".into()));
            } else {
                log::info!("LAN: {from} is admitting other players; retrying automatically");
                self.events.push(LanEvent::Notice(
                    "the host is admitting other players; waiting for a loading slot".into(),
                ));
            }
        }
        self.waiting_for_slot = true;
    }

    /// A dedicated server loading its world (`true`): new players are told to wait - one let
    /// in then met an empty world and was told of none of its vehicles - and come in once it
    /// is ready (`false`). Players already here stay.
    pub fn set_starting(&mut self, on: bool) {
        if self.starting && !on {
            log::info!("LAN: the world is ready: letting players in");
        }
        self.starting = on;
    }

    /// This machine's address as the internet sees it (STUN, or the router's forwarding),
    /// once the bridge has found it out.
    pub fn public_addr(&self) -> Option<SocketAddr> {
        self.bridge.as_ref().and_then(|b| b.public())
    }

    /// The players loading now that hold a loading slot: those heard from lately.
    pub(super) fn loading_now(&self) -> usize {
        let now = Instant::now();
        self.peers.values().filter(|p| !p.has_pose && now.duration_since(p.last_seen) < LOADING_ALIVE).count()
    }

    /// Returns true if the incoming HELLO must retry later.
    /// Stale entries expire after six seconds; new entries have a bounded FIFO.
    pub(super) fn defer_new_join(&mut self, from: SocketAddr, supports_wait: bool) -> bool {
        let now = Instant::now();
        if self.starting {
            if supports_wait {
                self.send(format!("WAIT|{PROTOCOL}|2|starting").as_bytes(), from);
            } else {
                self.reject(from, "the server is still loading its world - try again in a minute");
            }
            return true;
        }
        self.waiting_joins.retain(|(_, seen)| now.duration_since(*seen) < Duration::from_secs(6));
        let queued = self.waiting_joins.iter().position(|(addr, _)| *addr == from);
        if let Some(i) = queued {
            self.waiting_joins[i].1 = now;
        } else if self.loading_now() >= MAX_JOINING
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
        let active_loading = self.loading_now();
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
