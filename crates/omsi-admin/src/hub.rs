//! The hub between the server's threads: the web gateway's (which ask, and read what the
//! server last published) and the server's own loop (which does, on its tick). Requests wait
//! in a short queue - a full one refuses more, a stale one is refused when the loop comes to
//! it - and every operation, whatever way it came, has a record of what became of it.

use crate::audit::{Audit, Entry};
use crate::auth::Auth;
use crate::op::{Op, Target};
use crate::perm::{Perm, Role};
use crate::players::Player;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Requests waiting for the loop at most.
const QUEUE: usize = 64;
/// Records kept.
const RECORDS: usize = 200;
/// A request the loop has not come to in this long is refused (the server was stuck).
const STALE: Duration = Duration::from_secs(30);

/// Where an operation came from, for its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A player's game (the Administration menu), this player.
    Game(u32),
    /// The web panel or the API.
    Web,
    /// `POST /admin` from the server's own machine.
    Local,
}

/// Who asks.
#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    /// As the records name them.
    pub label: String,
    pub role: Role,
    pub origin: Origin,
}

/// What became of an operation. `Queued`: taken, the loop has not come to it; `Applied`: the
/// loop did it (a player sent away is gone from the session, the clock reads the new time);
/// `Pending`: the loop did its part, the rest is another game's (a word sent to a player's
/// game, which shows it); `Rejected`: not done, with the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Applied,
    Pending,
    Rejected,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Applied => "applied",
            Status::Pending => "pending",
            Status::Rejected => "rejected",
        }
    }

    /// The word a player's game shows for it (English: the key of its translations).
    pub fn label(self) -> &'static str {
        match self {
            Status::Applied => "Applied",
            Status::Pending => "Sent",
            Status::Rejected => "Refused",
            Status::Queued => "Waiting",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        [Status::Queued, Status::Applied, Status::Pending, Status::Rejected].into_iter().find(|x| x.name() == s)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub status: Status,
    pub message: String,
}

impl Outcome {
    pub fn applied(m: impl Into<String>) -> Outcome {
        Outcome { status: Status::Applied, message: m.into() }
    }
    pub fn pending(m: impl Into<String>) -> Outcome {
        Outcome { status: Status::Pending, message: m.into() }
    }
    pub fn rejected(m: impl Into<String>) -> Outcome {
        Outcome { status: Status::Rejected, message: m.into() }
    }
}

/// An operation's record.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OpRecord {
    pub id: u64,
    pub actor: String,
    pub op: String,
    pub summary: String,
    pub target: Option<Target>,
    /// Who the player was when it was asked (`Name (#3)`): kept, as they may be gone since.
    pub target_name: String,
    pub status: Status,
    pub message: String,
    /// Seconds since 1970, when it was asked.
    pub at: u64,
}

/// A request for the loop.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: u64,
    pub actor: Actor,
    pub op: Op,
    pub asked: Instant,
}

impl Job {
    /// Waited too long for the loop.
    pub fn stale(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.asked) > STALE
    }
}

/// The server as the admins see it, published by the loop (about once a second).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ServerState {
    pub name: String,
    pub map: String,
    pub version: String,
    pub players: usize,
    pub max_players: usize,
    /// `HH:MM` and seconds of the day.
    pub time: String,
    pub clock: f64,
    pub clock_speed: f64,
    /// The clock follows the machine's (`real_time`): no admin moves it or its speed.
    pub real_time: bool,
    pub weather: String,
    /// The weather follows this airport's METAR (`metar_sync`): no admin changes it.
    pub metar: Option<String>,
    pub traffic: usize,
    pub traffic_max: usize,
    pub timetable: bool,
    pub passengers: bool,
    pub passenger_density: f32,
    pub passenger_density_max: f32,
    pub waiting: usize,
    pub walking: usize,
    pub aboard: usize,
    pub cars: usize,
    pub buses: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub server: ServerState,
    pub players: Vec<Player>,
    /// When it was published (None: never yet).
    pub at: Option<Instant>,
}

pub struct Hub {
    auth: Mutex<Auth>,
    audit: Mutex<Audit>,
    snapshot: Mutex<Arc<Snapshot>>,
    queue: Mutex<VecDeque<Job>>,
    records: Mutex<VecDeque<OpRecord>>,
    next: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Hub {
    pub fn new(auth: Auth, audit: Audit) -> Hub {
        Hub { auth: Mutex::new(auth), audit: Mutex::new(audit), snapshot: Mutex::new(Arc::new(Snapshot::default())), queue: Mutex::new(VecDeque::new()), records: Mutex::new(VecDeque::new()), next: AtomicU64::new(1) }
    }

    pub fn auth(&self) -> MutexGuard<'_, Auth> {
        lock(&self.auth)
    }

    /// Note something that is no operation (a login, a logout, a refused door).
    pub fn note(&self, actor: &str, role: Option<Role>, action: &str, result: &str, detail: &str) {
        lock(&self.audit).record(actor, role, action, "", result, detail);
    }

    /// The audit entries after `seq`.
    pub fn audit_since(&self, seq: u64, max: usize) -> Vec<Entry> {
        lock(&self.audit).since(seq, max)
    }

    fn keep(&self, r: OpRecord) {
        let mut v = lock(&self.records);
        if v.len() >= RECORDS {
            v.pop_front();
        }
        v.push_back(r);
    }

    fn target_label(&self, t: Option<Target>) -> String {
        let Some(t) = t else { return String::new() };
        let snap = self.snapshot();
        snap.players.iter().find(|p| p.id == t.id && p.conn == t.conn).map(|p| p.label()).unwrap_or_else(|| format!("player {}", t.id))
    }

    /// Start a record for an operation the loop does at once (one from a player's game or
    /// the machine's own tool): its number, for `finish`. Its permission was checked by the
    /// caller, as `submit` checks it.
    pub fn begin(&self, actor: &Actor, op: &str, summary: &str, target: Option<Target>, target_name: &str) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.keep(OpRecord { id, actor: actor.label.clone(), op: op.into(), summary: crate::op::clean(summary, 200), target, target_name: crate::op::clean(target_name, 80), status: Status::Queued, message: String::new(), at: crate::unix_now() });
        id
    }

    /// A request from the web: checked against the role, queued for the loop - its record
    /// (`queued`), or why it is refused at once.
    pub fn submit(&self, actor: Actor, op: Op) -> Result<OpRecord, String> {
        let perm = op.perm();
        if !actor.role.allows(perm) {
            let why = format!("a {} may not do this ({} needs the '{}' permission)", actor.role.name(), op.verb(), perm_name(perm));
            lock(&self.audit).record(&actor.label, Some(actor.role), op.verb(), &self.target_label(op.target()), "refused", &why);
            return Err(why);
        }
        let mut q = lock(&self.queue);
        if q.len() >= QUEUE {
            return Err("the server has too many requests waiting: try again in a moment".into());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let target_name = self.target_label(op.target());
        let rec = OpRecord { id, actor: actor.label.clone(), op: op.verb().into(), summary: op.summary(), target: op.target(), target_name, status: Status::Queued, message: "waiting for the server's next step".into(), at: crate::unix_now() };
        lock(&self.audit).record(&actor.label, Some(actor.role), op.verb(), &rec.target_name, "queued", &rec.summary);
        q.push_back(Job { id, actor, op, asked: Instant::now() });
        drop(q);
        self.keep(rec.clone());
        Ok(rec)
    }

    /// The loop takes at most `max` requests (the rest wait for its next step).
    pub fn take(&self, max: usize) -> Vec<Job> {
        let mut q = lock(&self.queue);
        let n = q.len().min(max);
        q.drain(..n).collect()
    }

    /// What became of operation `id` (the loop's word; a record no longer kept is still in
    /// the audit log).
    pub fn finish(&self, id: u64, actor: &Actor, outcome: &Outcome) {
        let mut target = String::new();
        let mut summary = String::new();
        let mut op = String::new();
        if let Some(r) = lock(&self.records).iter_mut().find(|r| r.id == id) {
            r.status = outcome.status;
            r.message = crate::op::clean(&outcome.message, 200);
            target = r.target_name.clone();
            summary = r.summary.clone();
            op = r.op.clone();
        }
        let detail = if outcome.message.is_empty() { summary } else { format!("{summary}: {}", outcome.message) };
        lock(&self.audit).record(&actor.label, Some(actor.role), &op, &target, outcome.status.name(), &detail);
    }

    pub fn record(&self, id: u64) -> Option<OpRecord> {
        lock(&self.records).iter().find(|r| r.id == id).cloned()
    }

    /// The records after `id`, oldest first, at most `max`.
    pub fn records_since(&self, id: u64, max: usize) -> Vec<OpRecord> {
        let v = lock(&self.records);
        let mut out: Vec<OpRecord> = v.iter().filter(|r| r.id > id).cloned().collect();
        if out.len() > max {
            out.drain(..out.len() - max);
        }
        out
    }

    /// The last records, newest last (also those of earlier ids still changing: an event
    /// list shows the latest state of each).
    pub fn recent(&self, max: usize) -> Vec<OpRecord> {
        let v = lock(&self.records);
        v.iter().skip(v.len().saturating_sub(max)).cloned().collect()
    }

    pub fn publish(&self, snap: Snapshot) {
        *lock(&self.snapshot) = Arc::new(snap);
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        lock(&self.snapshot).clone()
    }
}

fn perm_name(p: Perm) -> &'static str {
    match p {
        Perm::View => "view",
        Perm::Moderate => "moderate",
        Perm::World => "world",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthCfg, Passwords};
    use crate::op::WeatherOp;

    fn hub() -> Hub {
        Hub::new(Auth::new(Passwords { admin: "admin-secret".into(), moderator: "mod-secret".into() }, AuthCfg::default()), Audit::new(None, 0))
    }

    fn actor(role: Role) -> Actor {
        Actor { label: format!("web#1 ({})", role.name()), role, origin: Origin::Web }
    }

    /// The role decides, whatever the panel showed: a moderator's world operation is refused
    /// and noted, and never reaches the loop.
    #[test]
    fn a_request_needs_its_permission() {
        let h = hub();
        let err = h.submit(actor(Role::Moderator), Op::Weather(WeatherOp::Next)).unwrap_err();
        assert!(err.contains("moderator"), "{err}");
        assert!(h.take(10).is_empty());
        assert_eq!(h.audit_since(0, 10).last().map(|e| e.result.clone()), Some("refused".into()));
        let rec = h.submit(actor(Role::Moderator), Op::Say("hi".into())).unwrap();
        assert_eq!(rec.status, Status::Queued);
        let rec2 = h.submit(actor(Role::Admin), Op::Weather(WeatherOp::Next)).unwrap();
        let jobs = h.take(10);
        assert_eq!(jobs.iter().map(|j| j.id).collect::<Vec<_>>(), vec![rec.id, rec2.id]);
    }

    /// Taken is not done: the record says queued until the loop says what became of it.
    #[test]
    fn a_record_follows_its_operation() {
        let h = hub();
        let a = actor(Role::Admin);
        let rec = h.submit(a.clone(), Op::Say("hi".into())).unwrap();
        assert_eq!(h.record(rec.id).unwrap().status, Status::Queued);
        let job = h.take(1).remove(0);
        h.finish(job.id, &a, &Outcome::rejected("nobody to hear it"));
        let r = h.record(rec.id).unwrap();
        assert_eq!((r.status, r.message.as_str()), (Status::Rejected, "nobody to hear it"));
        assert_eq!(h.records_since(0, 10).len(), 1);
        assert_eq!(h.records_since(rec.id, 10).len(), 0);
        // the in-game way: begun and finished by the loop at once
        let g = Actor { label: "player 2 'x'".into(), role: Role::Admin, origin: Origin::Game(2) };
        let id = h.begin(&g, "clock", "clock 08:30", None, "");
        h.finish(id, &g, &Outcome::applied(""));
        assert_eq!(h.record(id).unwrap().status, Status::Applied);
        let audit = h.audit_since(0, 10);
        assert_eq!(audit.iter().map(|e| e.result.as_str()).collect::<Vec<_>>(), vec!["queued", "rejected", "applied"]);
    }

    #[test]
    fn the_queue_is_short() {
        let h = hub();
        for _ in 0..QUEUE {
            h.submit(actor(Role::Admin), Op::Say("x".into())).unwrap();
        }
        assert!(h.submit(actor(Role::Admin), Op::Say("x".into())).is_err());
        assert_eq!(h.take(16).len(), 16);
        assert!(h.submit(actor(Role::Admin), Op::Say("x".into())).is_ok());
        let job = h.take(1).remove(0);
        assert!(!job.stale(Instant::now()));
        assert!(job.stale(Instant::now() + STALE + Duration::from_secs(1)));
    }

    #[test]
    fn records_are_few() {
        let h = hub();
        let a = actor(Role::Admin);
        for _ in 0..(RECORDS + 5) {
            let id = h.begin(&a, "say", "say: x", None, "");
            h.finish(id, &a, &Outcome::applied(""));
        }
        assert_eq!(h.recent(1000).len(), RECORDS);
    }
}
