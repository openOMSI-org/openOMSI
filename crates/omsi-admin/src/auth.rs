//! The admins' sessions for the web panel and the API: a password gives a role and a token
//! (random, 256 bits, shown once; the server keeps only its SHA-256), which ends after a while
//! without use, after a longest life, at logout, or when the passwords change. Wrong passwords
//! lock the door for their source, and for everybody when they come from many at once.
//!
//! The token goes in the `Authorization: Bearer` header only: never in a cookie (so no other
//! site's page can make a browser send it), never in an address, never in a log.

use crate::perm::{Perm, Role};
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// The passwords of `server.cfg` (empty: none). A moderator's equal to the admin's gives the
/// admin's role.
#[derive(Clone, Default)]
pub struct Passwords {
    pub admin: String,
    pub moderator: String,
}

impl std::fmt::Debug for Passwords {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // (never the passwords themselves)
        write!(
            f,
            "Passwords {{ admin: {}, moderator: {} }}",
            !self.admin.is_empty(),
            !self.moderator.is_empty()
        )
    }
}

/// How long sessions live and how hard the door locks.
#[derive(Debug, Clone)]
pub struct AuthCfg {
    /// A session not used for this long ends.
    pub idle: Duration,
    /// No session lives longer.
    pub max_age: Duration,
    /// Sessions at once (the oldest ends for a new one).
    pub max_sessions: usize,
    /// Wrong passwords from one source within `window` that lock it.
    pub per_source: usize,
    /// Wrong passwords from all sources within `window` that lock the door for everybody.
    pub overall: usize,
    pub window: Duration,
}

impl Default for AuthCfg {
    fn default() -> AuthCfg {
        AuthCfg {
            idle: Duration::from_secs(30 * 60),
            max_age: Duration::from_secs(12 * 3600),
            max_sessions: 16,
            per_source: 5,
            overall: 30,
            window: Duration::from_secs(120),
        }
    }
}

struct Session {
    role: Role,
    actor: String,
    created: Instant,
    last: Instant,
}

/// A session as the panels are told of it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionInfo {
    pub role: Role,
    pub permissions: Vec<Perm>,
    /// Who the records name (`web#<n> (<address>)`).
    pub actor: String,
    /// Seconds until it ends unless used.
    pub expires_in: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoginError {
    /// No password set for this way in.
    Disabled,
    /// Too many wrong passwords lately: again in `wait` seconds; `first`: the first knock on
    /// this lock (the one to note - not every knock, which would fill the records).
    Locked {
        wait: u64,
        first: bool,
    },
    Wrong,
    /// Every session is taken by a role above the one asked for.
    Full,
}

impl LoginError {
    pub fn message(&self) -> String {
        match self {
            LoginError::Disabled => {
                "the server has no admin password: its administration is off".into()
            }
            LoginError::Locked { wait, .. } => {
                format!("too many wrong passwords: try again in {wait} s")
            }
            LoginError::Wrong => "wrong password".into(),
            LoginError::Full => {
                "every session is taken by an admin: try again when one ends".into()
            }
        }
    }
}

pub struct Auth {
    pw: Passwords,
    cfg: AuthCfg,
    sessions: HashMap<[u8; 32], Session>,
    fails: HashMap<IpAddr, Vec<Instant>>,
    all_fails: Vec<Instant>,
    /// The locks already told of: whose (None: everybody's) and the wrong password that began it.
    told: HashMap<Option<IpAddr>, Instant>,
    /// Each source's locks so far and until when the last holds: a source that keeps
    /// guessing waits twice as long each time (an hour at most).
    penalty: HashMap<IpAddr, (u32, Instant)>,
    counter: u64,
}

fn digest(s: &str) -> [u8; 32] {
    let d = ring::digest::digest(&ring::digest::SHA256, s.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(d.as_ref());
    out
}

/// Equal in time independent of where they differ (on digests: of equal length).
fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// The source a wrong password counts against: an IPv6 address by its /64 (a v4 one mapped
/// into v6 as the v4 one), an IPv4 one as it is.
fn bucket(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

fn token() -> Option<String> {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 32];
    ring::rand::SystemRandom::new().fill(&mut b).ok()?;
    Some(b.iter().map(|x| format!("{x:02x}")).collect())
}

impl Auth {
    pub fn new(pw: Passwords, cfg: AuthCfg) -> Auth {
        Auth {
            pw,
            cfg,
            sessions: HashMap::new(),
            fails: HashMap::new(),
            all_fails: Vec::new(),
            told: HashMap::new(),
            penalty: HashMap::new(),
            counter: 0,
        }
    }

    /// Whether any password is set.
    pub fn enabled(&self) -> bool {
        !self.pw.admin.is_empty() || !self.pw.moderator.is_empty()
    }

    /// The role a password gives (both compared, whatever matches first).
    pub fn role_of(&self, password: &str) -> Option<Role> {
        if password.is_empty() {
            return None;
        }
        let given = digest(password);
        let admin = !self.pw.admin.is_empty() && same(&given, &digest(&self.pw.admin));
        let moderator = !self.pw.moderator.is_empty() && same(&given, &digest(&self.pw.moderator));
        if admin {
            Some(Role::Admin)
        } else if moderator {
            Some(Role::Moderator)
        } else {
            None
        }
    }

    /// The door's lock for `source`: seconds until it opens again, and which lock (whose, and
    /// the wrong password that began it). The lock for everybody leaves a request straight
    /// from this machine alone (`direct`: its own peer, through no proxy): a flood from afar
    /// cannot keep the operator out of their own panel.
    fn locked(
        &mut self,
        source: IpAddr,
        direct: bool,
        now: Instant,
    ) -> Option<(u64, Option<IpAddr>, Instant)> {
        let window = self.cfg.window;
        let keep = |v: &mut Vec<Instant>| v.retain(|t| now.saturating_duration_since(*t) < window);
        keep(&mut self.all_fails);
        self.fails.retain(|_, v| {
            keep(v);
            !v.is_empty()
        });
        let wait = |v: &[Instant], whose: Option<IpAddr>| {
            v.first().map(|t| {
                (
                    window
                        .saturating_sub(now.saturating_duration_since(*t))
                        .as_secs()
                        .max(1),
                    whose,
                    *t,
                )
            })
        };
        if self.all_fails.len() >= self.cfg.overall && !direct {
            return wait(&self.all_fails, None);
        }
        if let Some(&(_, until)) = self.penalty.get(&source).filter(|(_, until)| *until > now) {
            return Some((
                until.saturating_duration_since(now).as_secs().max(1),
                Some(source),
                until,
            ));
        }
        match self.fails.get(&source) {
            Some(v) if v.len() >= self.cfg.per_source => wait(v, Some(source)),
            _ => None,
        }
    }

    /// A password for a session: its token (to give the client once) and what it may do.
    pub fn login(
        &mut self,
        password: &str,
        source: IpAddr,
        now: Instant,
    ) -> Result<(String, SessionInfo), LoginError> {
        self.login_from(password, source, false, now)
    }

    /// `login`, for a request that came straight from this machine (`direct`) or not.
    pub fn login_from(
        &mut self,
        password: &str,
        source: IpAddr,
        direct: bool,
        now: Instant,
    ) -> Result<(String, SessionInfo), LoginError> {
        if !self.enabled() {
            return Err(LoginError::Disabled);
        }
        // (the counts by source: an IPv6 address by its /64, which one client changes at will)
        let key = bucket(source);
        if let Some((wait, whose, began)) = self.locked(key, direct, now) {
            if self.told.len() >= 1024 {
                self.told.clear();
            }
            let first = self.told.insert(whose, began) != Some(began);
            return Err(LoginError::Locked { wait, first });
        }
        let Some(role) = self.role_of(password) else {
            self.all_fails.push(now);
            // (at most so many sources remembered: a flood of addresses forgets the oldest)
            if self.fails.len() >= 1024 && !self.fails.contains_key(&key) {
                if let Some(k) = self.fails.keys().next().copied() {
                    self.fails.remove(&k);
                }
            }
            let tries = self.fails.entry(key).or_default();
            tries.push(now);
            if tries.len() >= self.cfg.per_source {
                // locked: for the window the first time, twice as long each time after
                self.fails.remove(&key);
                if self.penalty.len() >= 1024 && !self.penalty.contains_key(&key) {
                    self.penalty.retain(|_, (_, until)| *until > now);
                }
                let strikes = self.penalty.get(&key).map_or(0, |p| p.0).saturating_add(1);
                let hold = self
                    .cfg
                    .window
                    .saturating_mul(1 << (strikes - 1).min(10))
                    .min(Duration::from_secs(3600));
                self.penalty.insert(key, (strikes, now + hold));
            }
            return Err(LoginError::Wrong);
        };
        self.penalty.remove(&key);
        self.prune(now);
        while self.sessions.len() >= self.cfg.max_sessions {
            // (the least used of this role or a lower one: a moderator's logins never end an
            // admin's session)
            let oldest = self
                .sessions
                .iter()
                .filter(|(_, s)| s.role == role || role == Role::Admin)
                .min_by_key(|(_, s)| s.last)
                .map(|(k, _)| *k);
            match oldest {
                Some(k) => self.sessions.remove(&k),
                None => return Err(LoginError::Full),
            };
        }
        let token = token().ok_or(LoginError::Disabled)?;
        self.counter += 1;
        let actor = format!("web#{} ({source})", self.counter);
        self.sessions.insert(
            digest(&token),
            Session {
                role,
                actor,
                created: now,
                last: now,
            },
        );
        let info = self.info(&digest(&token), now).expect("just made");
        Ok((token, info))
    }

    fn info(&self, key: &[u8; 32], now: Instant) -> Option<SessionInfo> {
        let s = self.sessions.get(key)?;
        let idle_left = self
            .cfg
            .idle
            .saturating_sub(now.saturating_duration_since(s.last));
        let age_left = self
            .cfg
            .max_age
            .saturating_sub(now.saturating_duration_since(s.created));
        Some(SessionInfo {
            role: s.role,
            permissions: s.role.perms(),
            actor: s.actor.clone(),
            expires_in: idle_left.min(age_left).as_secs(),
        })
    }

    fn prune(&mut self, now: Instant) {
        let (idle, max_age) = (self.cfg.idle, self.cfg.max_age);
        self.sessions.retain(|_, s| {
            now.saturating_duration_since(s.last) < idle
                && now.saturating_duration_since(s.created) < max_age
        });
    }

    /// The session of a token, if it still lives (its use keeps it alive).
    pub fn check(&mut self, token: &str, now: Instant) -> Option<SessionInfo> {
        if token.is_empty() || token.len() > 128 {
            return None;
        }
        self.prune(now);
        let key = digest(token);
        self.sessions.get_mut(&key)?.last = now;
        self.info(&key, now)
    }

    /// The session of a token ends. Whether there was one.
    pub fn logout(&mut self, token: &str) -> bool {
        self.sessions.remove(&digest(token)).is_some()
    }

    /// Every session ends (the passwords changed, the server says so).
    pub fn revoke_all(&mut self) {
        self.sessions.clear();
    }

    /// New passwords: every session ends.
    pub fn set_passwords(&mut self, pw: Passwords) {
        self.pw = pw;
        self.revoke_all();
    }

    pub fn sessions(&self) -> usize {
        self.sessions.len()
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
