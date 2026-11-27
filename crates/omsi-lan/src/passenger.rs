use omsi_net::Footprint;

const MARGIN: f64 = 1.5;

pub fn inside(f: &Footprint, local: [f32; 3]) -> bool {
    let (across, along) = (local[0] as f64, local[1] as f64);
    across.abs() <= f.width as f64 * 0.5 + MARGIN && along.abs() <= f.length as f64 + MARGIN
}

pub fn passenger_trigger(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_suffix("_off").unwrap_or(&n);
    n.starts_with("int_") || n.contains("haltewunsch") || n.contains("kinderwagenwunsch")
}

/// May player `from` work switch `ev` of our bus `bus` (`lan`: our session)? Only a passenger
/// aboard our bus, inside it, and only a switch passengers work (`passenger_trigger`).
pub fn may_work(
    lan: &omsi_net::LanSession,
    from: u32,
    bus: Option<&omsi_sim::VehicleInstance>,
    ev: &str,
) -> bool {
    aboard_inside(lan, from, bus) && passenger_trigger(ev)
}

/// Is player `from` aboard our bus `bus` and inside it - riding in it, not standing beside it?
/// (What a passenger works, and what the conductor of the bus serves, hangs on it: #2859.)
pub fn aboard_inside(
    lan: &omsi_net::LanSession,
    from: u32,
    bus: Option<&omsi_sim::VehicleInstance>,
) -> bool {
    let walker = lan
        .peers()
        .find(|p| p.pose.id == from)
        .and_then(|p| p.pose.walker);
    let aboard = walker
        .and_then(|w| w.aboard)
        .filter(|a| a.owner == lan.my_id);
    let inside = match (aboard, bus) {
        (Some(a), Some(v)) => inside(
            &crate::spawn::footprint_of(v, crate::spawn::BUS_BOX),
            a.local,
        ),
        _ => false,
    };
    aboard.is_some() && inside
}

/// Select an identifiable passenger only when exactly one authenticated player is
/// aboard this bus. Multiple passengers require a separate explicit designation UI.
pub fn sole_aboard(lan: &omsi_net::LanSession, bus: &omsi_sim::VehicleInstance) -> Option<u32> {
    let footprint = crate::spawn::footprint_of(bus, crate::spawn::BUS_BOX);
    let mut aboard = lan.peers().filter(|peer| {
        peer.has_info
            && peer.has_pose
            && peer
                .pose
                .walker
                .and_then(|walker| walker.aboard)
                .is_some_and(|a| a.owner == lan.my_id && inside(&footprint, a.local))
    });
    let candidate = aboard.next()?.pose.id;
    (candidate != 0 && aboard.next().is_none()).then_some(candidate)
}

/// The owner validates an authenticated designated assistant before applying
/// cooperative cab actions. Passenger bell permissions remain unchanged.
pub fn may_work_cabin(
    lan: &omsi_net::LanSession,
    from: u32,
    bus: Option<&omsi_sim::VehicleInstance>,
    assistant: Option<u32>,
    action: &crate::coop_permissions::CabinAction,
) -> bool {
    use crate::coop_permissions::{CabinRequest, CoopRoles, matches_assistant, may_operate_cabin};
    let Some(bus) = bus else { return false };
    let aboard = lan
        .peers()
        .find(|peer| peer.pose.id == from && peer.has_info && peer.has_pose)
        .and_then(|peer| peer.pose.walker)
        .and_then(|walker| walker.aboard)
        .filter(|aboard| aboard.owner == lan.my_id);
    let inside_bus = aboard.is_some_and(|aboard| {
        inside(
            &crate::spawn::footprint_of(bus, crate::spawn::BUS_BOX),
            aboard.local,
        )
    });
    let assigned = matches_assistant(assistant, from);
    let req = CabinRequest {
        same_session: lan.connected || lan.role == omsi_net::Role::Host,
        owner_consented: assigned,
        passenger_inside_bus: inside_bus,
        speed_kmh: bus.physics.speed * 3.6,
    };
    may_operate_cabin(
        CoopRoles {
            conductor: assigned,
            ..Default::default()
        },
        req,
        action,
    )
}

/// Whether a pending boarding grant should be returned after the owner's bus
/// leaves the stop. This is pure LAN policy, independent of game rendering.
pub fn left_boarding_stop(bus: Option<(glam::DVec3, f64)>, stop: Option<glam::DVec3>) -> bool {
    match (bus, stop) {
        (Some((position, speed)), Some(point)) => {
            let distance = (position - point).truncate().length();
            distance > 75.0 || (distance > 25.0 && speed.abs() > 2.0)
        }
        (None, _) => true,
        _ => false,
    }
}

/// Resend physical BOARD/RETURN reports until the authoritative host ACKs;
/// a dropped UDP acknowledgement must not lose a rider.
pub fn resend_until_ack(
    lan: &mut omsi_net::LanSession,
    pending: &mut hashbrown::HashMap<u32, Option<std::time::Instant>>,
    now: std::time::Instant,
    boarded: bool,
) {
    let due: Vec<u32> = pending
        .iter()
        .filter(|(_, last)| {
            last.is_none_or(|at| now.duration_since(at) >= std::time::Duration::from_secs(1))
        })
        .map(|(id, _)| *id)
        .collect();
    if due.is_empty() {
        return;
    }
    if boarded {
        lan.confirm_boarded(&due);
    } else {
        lan.return_unboarded(&due);
    }
    for id in due {
        pending.insert(id, Some(now));
    }
}

/// Coalesce bursty passenger requests into one bounded UDP CLAIM message.
/// Individual frames can discover waiting people in several groups, while the
/// host also rate-limits INFO and PAXBOARD/RETURN messages from this peer.
#[derive(Default)]
pub struct ClaimQueue {
    pending: std::collections::BTreeSet<u32>,
    /// Claims already transmitted; hold them until the caller's own retry window.
    recently_sent: std::collections::BTreeMap<u32, std::time::Instant>,
    /// Keep the visual waiting person while a delayed GRANT may still arrive.
    awaiting_answer: std::collections::BTreeMap<u32, std::time::Instant>,
    last_sent: Option<std::time::Instant>,
}

impl ClaimQueue {
    const INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
    const MAX_BATCH: usize = 64;
    const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(8);
    const ANSWER_WINDOW: std::time::Duration = std::time::Duration::from_secs(15);

    /// Clear claims belonging to the previous host/session.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.recently_sent.clear();
        self.awaiting_answer.clear();
        self.last_sent = None;
    }

    fn take_due(
        &mut self,
        requested: impl IntoIterator<Item = u32>,
        now: std::time::Instant,
    ) -> Vec<u32> {
        // A caller can rediscover the same stop on the next frame. Do not
        // reinsert already sent IDs ahead of yet-unsent claims. Let genuinely
        // lost UDP claims be retried after the normal stop retry timeout.
        self.recently_sent
            .retain(|_, sent| now.saturating_duration_since(*sent) < Self::RETRY_AFTER);
        self.awaiting_answer
            .retain(|_, sent| now.saturating_duration_since(*sent) < Self::ANSWER_WINDOW);
        self.pending.extend(
            requested
                .into_iter()
                .filter(|id| !self.recently_sent.contains_key(id)),
        );
        if self.pending.is_empty()
            || self
                .last_sent
                .is_some_and(|last| now.saturating_duration_since(last) < Self::INTERVAL)
        {
            return Vec::new();
        }
        let due: Vec<u32> = self.pending.iter().take(Self::MAX_BATCH).copied().collect();
        for id in &due {
            self.pending.remove(id);
            self.recently_sent.insert(*id, now);
            self.awaiting_answer.insert(*id, now);
        }
        self.last_sent = Some(now);
        due
    }

    /// A GONE for a just-claimed person may arrive before its UDP GRANT.
    /// Hold the mirrored waiting person only until its bounded retry window.
    pub fn is_awaiting(&self, id: u32) -> bool {
        self.pending.contains(&id)
            || self.awaiting_answer.get(&id).is_some_and(|since| {
                std::time::Instant::now().saturating_duration_since(*since) < Self::ANSWER_WINDOW
            })
    }

    pub fn mark_answered(&mut self, id: u32) {
        self.pending.remove(&id);
        self.recently_sent.remove(&id);
        self.awaiting_answer.remove(&id);
    }

    pub fn send_due(
        &mut self,
        lan: &omsi_net::LanSession,
        requested: impl IntoIterator<Item = u32>,
        now: std::time::Instant,
    ) -> usize {
        let due = self.take_due(requested, now);
        if !due.is_empty() {
            lan.claim(&due);
        }
        due.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passenger_is_inside_the_bus_it_claims() {
        let bus = Footprint {
            x: 100.0,
            y: 50.0,
            z: 0.0,
            heading: 90.0,
            length: 12.0,
            width: 2.5,
        };
        assert!(inside(&bus, [0.0, 0.0, 1.0]));
        assert!(inside(&bus, [0.8, -5.5, 1.0]));
        assert!(!inside(&bus, [5.0, 0.0, 1.0]));
        assert!(!inside(&bus, [0.0, 200.0, 1.0]));
    }

    #[test]
    fn claims_are_deduplicated_and_sent_at_most_twice_per_second() {
        let mut queue = ClaimQueue::default();
        let now = std::time::Instant::now();
        let first = queue.take_due(0..100, now);
        assert_eq!(first.len(), 64);
        assert_eq!(first[0], 0);
        assert_eq!(first[63], 63);
        // A second group found in the same frame must not flood the
        // authenticated host's 10-message-per-second bucket.
        assert!(
            queue
                .take_due(50..150, now + std::time::Duration::from_millis(200))
                .is_empty()
        );
        let second = queue.take_due(
            std::iter::empty(),
            now + std::time::Duration::from_millis(500),
        );
        assert_eq!(second, (64..128).collect::<Vec<_>>());
        let third = queue.take_due(
            std::iter::empty(),
            now + std::time::Duration::from_millis(1_000),
        );
        assert_eq!(third, (128..150).collect::<Vec<_>>());
        assert!(
            queue
                .take_due([50, 51, 64], now + std::time::Duration::from_secs(2))
                .is_empty()
        );
        let retried = queue.take_due([50, 51], now + std::time::Duration::from_secs(9));
        assert_eq!(retried, vec![50, 51]);
        assert!(queue.is_awaiting(50));
        queue.mark_answered(50);
        assert!(!queue.is_awaiting(50));
        queue.clear();
        assert!(
            queue
                .take_due(
                    std::iter::empty(),
                    now + std::time::Duration::from_millis(1_500),
                )
                .is_empty()
        );
    }

    #[test]
    fn boarding_retries_accept_the_client_hashbrown_pending_ledger() {
        // Compile-time regression for the exact HashMap implementation used
        // by omsi-app::lan_world, which differs from std::collections::HashMap.
        fn accepts_client_ledger(_: &mut hashbrown::HashMap<u32, Option<std::time::Instant>>) {}
        let mut pending = hashbrown::HashMap::new();
        pending.insert(7, None);
        accepts_client_ledger(&mut pending);
    }

    #[test]
    fn boarding_grant_returns_only_after_bus_leaves_a_stop() {
        let stop = Some(glam::DVec3::ZERO);
        assert!(!left_boarding_stop(Some((glam::DVec3::ZERO, 0.0)), stop));
        assert!(!left_boarding_stop(
            Some((glam::DVec3::new(26.0, 0.0, 0.0), 1.0)),
            stop
        ));
        assert!(left_boarding_stop(
            Some((glam::DVec3::new(26.0, 0.0, 0.0), 3.0)),
            stop
        ));
        assert!(left_boarding_stop(
            Some((glam::DVec3::new(80.0, 0.0, 0.0), 0.0)),
            stop
        ));
        assert!(left_boarding_stop(None, stop));
        assert!(!left_boarding_stop(Some((glam::DVec3::ZERO, 0.0)), None));
    }

    #[test]
    fn only_a_passengers_controls_are_worked_from_another_game() {
        assert!(passenger_trigger("int_haltewunsch"));
        assert!(passenger_trigger("Haltewunsch_off"));
        assert!(passenger_trigger("int_kinderwagenwunsch"));
        assert!(!passenger_trigger("bus_doorfront0"));
        assert!(!passenger_trigger("handbrake"));
        assert!(!passenger_trigger("engine_start"));
    }
}
