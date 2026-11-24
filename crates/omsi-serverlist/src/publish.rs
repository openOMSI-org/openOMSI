//! A public server's side of the list: register with the service, say it is alive every
//! minute, take itself off the list at the end. The service lists a server only after it
//! has fetched the server's own `GET /status` and found the challenge it gave at
//! registration there - so a server must be reachable from the internet to be listed, and
//! nobody can list an address that is not theirs.
//!
//! Before the first word the server's name, message and icon pass the moderator
//! (`start` refuses a listing the list would not show). An icon goes up only if it was
//! cleared; otherwise the listing has none and the list shows the default one.

use crate::moderation::{IconVerdict, Moderator, listing_refusal};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often a heartbeat goes (the service forgets a server 3 minutes after the last).
pub const EVERY: Duration = Duration::from_secs(60);
/// The largest icon the service takes.
pub const MAX_UPLOAD_ICON: usize = 16 * 1024;

/// What a server says about itself to the list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub name: String,
    pub motd: String,
    pub map: String,
    pub players: usize,
    pub max_players: usize,
    pub version: String,
    pub protocol: u32,
    /// The web port of the server's gateway (the service tries the sender's address with it).
    pub web_port: u16,
    /// The tunnel's address, if the server has one (preferred: it is https).
    pub address: Option<String>,
    /// The icon, only if cleared.
    pub icon: Option<Vec<u8>>,
}

/// One request to the service.
pub trait Transport {
    /// POST `body` as JSON to `path`; the status and the JSON answer.
    fn post(&self, path: &str, body: &Value) -> Result<(u16, Value), String>;
}

/// The service over HTTPS.
pub struct Http {
    pub service: String,
}

impl Transport for Http {
    fn post(&self, path: &str, body: &Value) -> Result<(u16, Value), String> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build();
        let r = agent
            .post(&format!("{}{path}", self.service.trim_end_matches('/')))
            .set("Content-Type", "application/json")
            .send_string(&body.to_string());
        let (status, text) = match r {
            Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
            Err(ureq::Error::Status(s, r)) => (s, r.into_string().unwrap_or_default()),
            Err(e) => return Err(e.to_string()),
        };
        Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
    }
}

/// What a tick found.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// On the list.
    Listed,
    /// Registered, not on the list yet (the service has not reached the server: the reason).
    Waiting(String),
    /// The service refused the listing (the reason); it is not retried at once.
    Refused(String),
    /// The service could not be reached or is busy; tried again later.
    Down(String),
}

struct Registration {
    id: String,
    token: String,
}

pub struct Publisher<T: Transport> {
    t: T,
    reg: Option<Registration>,
}

fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn error_of(v: &Value, status: u16) -> String {
    v.get("error")
        .and_then(Value::as_str)
        .map(|e| crate::moderation::clean(e, 160))
        .unwrap_or_else(|| format!("the service answered {status}"))
}

impl<T: Transport> Publisher<T> {
    pub fn new(t: T) -> Publisher<T> {
        Publisher { t, reg: None }
    }

    /// Whether the service knows this server now.
    pub fn registered(&self) -> bool {
        self.reg.is_some()
    }

    /// Register if needed (`set_challenge` puts the service's challenge on the server's
    /// status page), then send a heartbeat.
    pub fn tick(&mut self, l: &Listing, set_challenge: &mut dyn FnMut(&str)) -> Outcome {
        if self.reg.is_none() {
            let mut body = json!({
                "name": l.name, "motd": l.motd, "map": l.map, "max_players": l.max_players,
                "version": l.version, "protocol": l.protocol, "web_port": l.web_port,
            });
            if let Some(a) = &l.address {
                body["address"] = json!(a);
            }
            if let Some(i) = l.icon.as_ref().filter(|i| i.len() <= MAX_UPLOAD_ICON) {
                body["icon"] = json!(b64(i));
            }
            match self.t.post("/v1/register", &body) {
                Err(e) => return Outcome::Down(e),
                Ok((200, v)) => {
                    let (Some(id), Some(token), Some(ch)) = (
                        v.get("id").and_then(Value::as_str),
                        v.get("token").and_then(Value::as_str),
                        v.get("challenge").and_then(Value::as_str),
                    ) else {
                        return Outcome::Down("the service answered something else".into());
                    };
                    set_challenge(ch);
                    self.reg = Some(Registration {
                        id: id.to_string(),
                        token: token.to_string(),
                    });
                }
                Ok((429, v)) => return Outcome::Down(error_of(&v, 429)),
                Ok((s, v)) if s >= 500 => return Outcome::Down(error_of(&v, s)),
                Ok((s, v)) => return Outcome::Refused(error_of(&v, s)),
            }
        }
        let r = self.reg.as_ref().expect("registered above");
        let body = json!({"id": r.id, "token": r.token, "players": l.players, "map": l.map});
        match self.t.post("/v1/heartbeat", &body) {
            Err(e) => Outcome::Down(e),
            Ok((200, v)) if v.get("verified").and_then(Value::as_bool) == Some(true) => {
                Outcome::Listed
            }
            Ok((200, v)) => Outcome::Waiting(error_of(&v, 200)),
            // (the service forgot us, or a restart changed the token: register again)
            Ok((404 | 403, _)) => {
                self.reg = None;
                Outcome::Down("the service forgot the listing; registering again".into())
            }
            Ok((429, v)) => Outcome::Down(error_of(&v, 429)),
            Ok((s, v)) if s >= 500 => Outcome::Down(error_of(&v, s)),
            Ok((s, v)) => Outcome::Refused(error_of(&v, s)),
        }
    }

    /// Take the server off the list now.
    pub fn unlist(&mut self) {
        if let Some(r) = self.reg.take() {
            let _ = self
                .t
                .post("/v1/unlist", &json!({"id": r.id, "token": r.token}));
        }
    }
}

/// A running publisher; dropping it takes the server off the list.
pub struct Handle {
    stop: Arc<AtomicBool>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Check the listing, and start publishing it. `Err` is the reason the server cannot be
/// public (shown to its host); the icon (a PNG, if any) is uploaded only when it was cleared.
/// `now` gives the listing as it is (players, tunnel address) and `set_challenge` puts the
/// service's challenge on the status page.
pub fn start(
    service: &str,
    first: Listing,
    icon: Option<Vec<u8>>,
    m: Arc<dyn Moderator>,
    now: Arc<dyn Fn() -> Listing + Send + Sync>,
    set_challenge: Arc<dyn Fn(&str) + Send + Sync>,
) -> Result<Handle, String> {
    if let Some(why) = listing_refusal(&*m, &first.name, &first.motd, icon.as_deref()) {
        return Err(why);
    }
    let cleared = icon.filter(|i| m.check_icon(i) == IconVerdict::Cleared);
    if cleared.is_none() {
        log::info!(
            "public list: the icon is not shown in the public list (no image check has cleared it); the default one is used"
        );
    }
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let service = crate::service_url(service);
    std::thread::Builder::new()
        .name("public-list".into())
        .spawn(move || {
            let mut p = Publisher::new(Http { service: service.clone() });
            let mut last = None;
            let mut wait = Duration::ZERO;
            while !flag.load(Ordering::Relaxed) {
                let mut l = now();
                l.icon = cleared.clone();
                let out = p.tick(&l, &mut |c| set_challenge(c));
                if last.as_ref() != Some(&out) {
                    match &out {
                        Outcome::Listed => log::info!("public list: listed at {service}"),
                        Outcome::Waiting(w) => log::warn!("public list: registered, not listed yet: {w} (the service must reach this server's web port or tunnel)"),
                        Outcome::Refused(w) => log::warn!("public list: refused: {w}"),
                        Outcome::Down(w) => log::info!("public list: {w}"),
                    }
                    last = Some(out.clone());
                }
                wait = match out {
                    Outcome::Listed => EVERY,
                    Outcome::Waiting(_) => Duration::from_secs(20),
                    Outcome::Refused(_) => Duration::from_secs(600),
                    Outcome::Down(_) => (wait * 2).clamp(Duration::from_secs(15), Duration::from_secs(300)),
                };
                let until = std::time::Instant::now() + wait;
                while std::time::Instant::now() < until && !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
            p.unlist();
        })
        .map_err(|e| e.to_string())?;
    Ok(Handle { stop })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// A service that answers from a script and remembers what it was sent.
    struct Fake {
        answers: RefCell<VecDeque<Result<(u16, Value), String>>>,
        sent: RefCell<Vec<(String, Value)>>,
    }

    impl Fake {
        fn new(a: Vec<Result<(u16, Value), String>>) -> Fake {
            Fake {
                answers: RefCell::new(a.into()),
                sent: RefCell::new(Vec::new()),
            }
        }
    }

    impl Transport for &Fake {
        fn post(&self, path: &str, body: &Value) -> Result<(u16, Value), String> {
            self.sent
                .borrow_mut()
                .push((path.to_string(), body.clone()));
            self.answers
                .borrow_mut()
                .pop_front()
                .unwrap_or(Err("no more answers".into()))
        }
    }

    fn listing() -> Listing {
        Listing {
            name: "Fine".into(),
            map: "Berlin".into(),
            players: 2,
            max_players: 16,
            version: "0.3".into(),
            protocol: 14,
            web_port: 27025,
            ..Default::default()
        }
    }

    fn registered() -> Result<(u16, Value), String> {
        Ok((
            200,
            json!({"id": "abc", "token": "tok", "challenge": "chal"}),
        ))
    }

    #[test]
    fn registering_puts_the_challenge_on_the_status_page_then_beats() {
        let f = Fake::new(vec![
            registered(),
            Ok((
                200,
                json!({"verified": false, "error": "the status page did not show the challenge"}),
            )),
            Ok((200, json!({"verified": true}))),
        ]);
        let mut p = Publisher::new(&f);
        let mut challenge = String::new();
        assert!(matches!(
            p.tick(&listing(), &mut |c| challenge = c.to_string()),
            Outcome::Waiting(_)
        ));
        assert_eq!(challenge, "chal");
        assert_eq!(
            p.tick(&listing(), &mut |_| panic!("registered once")),
            Outcome::Listed
        );
        let sent = f.sent.borrow();
        assert_eq!(sent[0].0, "/v1/register");
        assert!(sent[0].1.get("icon").is_none() && sent[0].1.get("address").is_none());
        assert_eq!(sent[1].1["token"], "tok");
        assert_eq!(sent[2].0, "/v1/heartbeat");
    }

    #[test]
    fn a_forgotten_listing_registers_again_and_a_refusal_is_reported() {
        let f = Fake::new(vec![
            registered(),
            Ok((404, Value::Null)),
            registered(),
            Ok((200, json!({"verified": true}))),
            Ok((200, Value::Null)),
            Ok((400, json!({"error": "name not allowed"}))),
        ]);
        let mut p = Publisher::new(&f);
        assert!(matches!(p.tick(&listing(), &mut |_| {}), Outcome::Down(_)));
        assert!(!p.registered());
        assert_eq!(p.tick(&listing(), &mut |_| {}), Outcome::Listed);
        p.unlist();
        assert!(!p.registered());
        let mut p = Publisher::new(&f);
        assert_eq!(
            p.tick(&listing(), &mut |_| {}),
            Outcome::Refused("name not allowed".into())
        );
    }

    #[test]
    fn an_icon_goes_up_only_when_small_and_the_tunnel_address_with_it() {
        let f = Fake::new(vec![registered(), Ok((200, json!({"verified": true})))]);
        let mut p = Publisher::new(&f);
        let l = Listing {
            icon: Some(vec![1, 2, 3, 4]),
            address: Some("https://x.trycloudflare.com".into()),
            ..listing()
        };
        p.tick(&l, &mut |_| {});
        assert_eq!(f.sent.borrow()[0].1["icon"], "AQIDBA==");
        assert_eq!(
            f.sent.borrow()[0].1["address"],
            "https://x.trycloudflare.com"
        );
        let f = Fake::new(vec![registered(), Ok((200, json!({"verified": true})))]);
        let big = Listing {
            icon: Some(vec![0; MAX_UPLOAD_ICON + 1]),
            ..listing()
        };
        Publisher::new(&f).tick(&big, &mut |_| {});
        assert!(f.sent.borrow()[0].1.get("icon").is_none());
    }

    #[test]
    fn the_network_down_is_retried_not_refused() {
        let f = Fake::new(vec![Err("offline".into()), Ok((503, Value::Null))]);
        let mut p = Publisher::new(&f);
        assert!(matches!(p.tick(&listing(), &mut |_| {}), Outcome::Down(_)));
        assert!(matches!(p.tick(&listing(), &mut |_| {}), Outcome::Down(_)));
    }

    #[test]
    fn a_flagged_listing_never_starts() {
        struct No;
        impl Moderator for No {
            fn check_text(&self, _: &str) -> crate::Verdict {
                crate::Verdict::Flagged("test")
            }
            fn check_icon(&self, _: &[u8]) -> IconVerdict {
                IconVerdict::Unchecked
            }
        }
        let r = start(
            "",
            listing(),
            None,
            Arc::new(No),
            Arc::new(listing),
            Arc::new(|_| {}),
        );
        assert!(r.err().unwrap().contains("name"));
    }
}
