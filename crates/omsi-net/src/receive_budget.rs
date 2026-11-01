//! UDP receive processing budget, kept separate from the session protocol handlers.
//! The per-tick limit protects the simulation thread without changing the wire protocol.
use super::{Duration, Instant, LanSession};

pub(super) const RECEIVE_PACKET_LIMIT: usize = 1024;
pub(super) const RECEIVE_TIME_BUDGET: Duration = Duration::from_millis(4);
const RECEIVE_DIAGNOSTIC_INTERVAL: Duration = Duration::from_secs(30);

pub(super) fn receive_budget_reached(packets: usize, elapsed: Duration) -> bool {
    packets >= RECEIVE_PACKET_LIMIT || elapsed >= RECEIVE_TIME_BUDGET
}

impl LanSession {
    /// True when processing must yield to the simulation; queued UDP packets wait for
    /// a later tick. Report repeated saturation at most once per 30 seconds.
    pub(super) fn receive_budget_yield(&mut self, packets: usize, started: Instant) -> bool {
        let elapsed = started.elapsed();
        if !receive_budget_reached(packets, elapsed) {
            return false;
        }
        self.recv_budget_hits = self.recv_budget_hits.saturating_add(1);
        if self.recv_budget_last_log.is_none_or(|at| at.elapsed() >= RECEIVE_DIAGNOSTIC_INTERVAL) {
            log::warn!(
                "LAN: receive budget reached {} time(s) since last report; last batch {} datagrams in {:.2} ms (limit {} / {} ms); queued datagrams deferred to next tick",
                self.recv_budget_hits, packets, elapsed.as_secs_f64() * 1000.0,
                RECEIVE_PACKET_LIMIT, RECEIVE_TIME_BUDGET.as_millis()
            );
            self.recv_budget_last_log = Some(Instant::now());
            self.recv_budget_hits = 0;
        }
        true
    }
}
