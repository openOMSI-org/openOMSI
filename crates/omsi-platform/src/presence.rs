//! "Playing now": while a game session runs it tells the project's presence service (a
//! Cloudflare Worker, `services/presence/`) every few minutes that one more openBus is being
//! played, and says goodbye when it ends. The website and the README show the count.
//!
//! What goes out is a random id made for this session alone (not kept anywhere, a new one
//! every start), the game's version and the kind of system (windows, macos, linux,
//! android) - no name, no map, nothing of the computer. The service keeps an id for 25
//! minutes after its last word and stores no addresses.
//!
//! Setting `presence` (on by default; the launcher's Settings → General: "Count me on the
//! website's 'playing now'"). `OMSI_NO_PRESENCE=1` switches it off, `OMSI_PRESENCE_URL`
//! points it at another service (tests).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The project's presence service (`services/presence/`, deployed with wrangler).
// TODO(openBus): placeholder host, point this at your own presence service.
pub const SERVICE: &str = "https://presence.openbus.example";
/// How often a running game says it is still there (the service forgets one after 25 min).
/// (Every three minutes, a few hundred players used up the free plan's 100 000 requests a
/// day by the evening, and the counter answered nobody until midnight UTC.)
const EVERY: Duration = Duration::from_secs(600);
/// How long a game stays quiet when the service says it has too many requests (429).
const BACK_OFF: Duration = Duration::from_secs(1800);

/// The presence of this session: dropping it says goodbye.
pub struct Presence {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// A fresh random id: two hashes of the clock by the process's own random hasher keys.
fn session_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let keys = std::collections::hash_map::RandomState::new();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let half = |salt: u64| {
        let mut h = keys.build_hasher();
        h.write_u128(now);
        h.write_u32(std::process::id());
        h.write_u64(salt);
        h.finish()
    };
    format!("{:016x}{:016x}", half(1), half(2))
}

/// The system, as the service counts it.
fn system() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Whether the player lets the game be counted (`presence`, on unless switched off).
fn allowed() -> bool {
    if omsi_cfg::flags::OMSI_NO_PRESENCE.is_set() {
        return false;
    }
    let text = std::fs::read_to_string(omsi_launcher_lib::data_dir().join("settings.cfg")).ok();
    omsi_launcher_lib::settings_from_text(text.as_deref()).get("presence").and_then(|v| v.as_bool()).unwrap_or(true)
}

/// What a post came to.
#[derive(PartialEq, Clone, Copy)]
enum Sent {
    Ok,
    /// the service is over its request limit: wait long before asking again
    Limited,
    Failed,
}

fn post(agent: &ureq::Agent, url: &str, body: &serde_json::Value) -> Sent {
    match agent.post(url).set("Content-Type", "application/json").send_string(&body.to_string()) {
        Ok(_) => Sent::Ok,
        Err(ureq::Error::Status(429, _)) => Sent::Limited,
        Err(_) => Sent::Failed,
    }
}

/// A session's own offset into the ping period (0..60 s, from its id): the games started
/// together after an update do not all ping in the same second.
fn jitter(id: &str) -> Duration {
    let n = u64::from_str_radix(&id[..8.min(id.len())], 16).unwrap_or(0);
    Duration::from_secs(n % 60)
}

impl Presence {
    /// Start saying "playing" (in the background); None when the player does not want it.
    pub fn start() -> Option<Presence> {
        if !allowed() {
            log::info!("presence: not counted on the website (setting presence=0)");
            return None;
        }
        let base = omsi_cfg::flags::OMSI_PRESENCE_URL.var().map(str::to_string).unwrap_or_else(|| SERVICE.to_string());
        let base = base.trim_end_matches('/').to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("presence".into())
            .spawn(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(Duration::from_secs(10))
                    .timeout(Duration::from_secs(15))
                    .user_agent(&format!("openBus/{}", crate::updater::current_version()))
                    .build();
                let id = session_id();
                let hello = serde_json::json!({ "id": id, "v": crate::updater::current_version(), "os": system() });
                let mut said = false;
                let mut first = true;
                while !flag.load(Ordering::Relaxed) {
                    let sent = post(&agent, &format!("{base}/ping"), &hello);
                    let ok = sent == Sent::Ok;
                    if ok != said || first {
                        log::info!("presence: {}", match sent {
                            Sent::Ok => "counted as playing",
                            Sent::Limited => "the service is over its request limit (asking again in half an hour)",
                            Sent::Failed => "the service did not answer (tried again later)",
                        });
                    }
                    said = ok;
                    let wait = if sent == Sent::Limited { BACK_OFF } else if first { EVERY + jitter(&id) } else { EVERY };
                    first = false;
                    // (a second at a time: the goodbye must not wait ten minutes)
                    let mut slept = Duration::ZERO;
                    while slept < wait && !flag.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_secs(1));
                        slept += Duration::from_secs(1);
                    }
                }
                if said {
                    post(&agent, &format!("{base}/bye"), &serde_json::json!({ "id": id }));
                }
            })
            .ok()?;
        Some(Presence { stop, thread: Some(thread) })
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // (the goodbye has a few seconds; a game that ends does not hang on the network)
        if let Some(t) = self.thread.take() {
            let until = std::time::Instant::now() + Duration::from_secs(3);
            while !t.is_finished() && std::time::Instant::now() < until {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_session_id_is_32_hex_digits_and_new_each_time() {
        let (a, b) = (super::session_id(), super::session_id());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    /// Against a service that runs (`OMSI_PRESENCE_URL=http://127.0.0.1:8787` with
    /// `npx wrangler dev` in services/presence): counted while it runs, gone after.
    #[test]
    #[ignore = "needs the presence service running at OMSI_PRESENCE_URL"]
    fn a_session_is_counted_while_it_runs() {
        let base = omsi_cfg::flags::OMSI_PRESENCE_URL.live_var().expect("OMSI_PRESENCE_URL");
        let count = || ureq::get(&format!("{base}/players")).call().unwrap().into_string().map(|t| serde_json::from_str::<serde_json::Value>(&t).unwrap()).unwrap()["players"].as_u64().unwrap();
        let before = count();
        let p = super::Presence::start().expect("presence allowed");
        std::thread::sleep(std::time::Duration::from_secs(2));
        // (the counter's answer is cached for two minutes)
        std::thread::sleep(std::time::Duration::from_secs(121));
        assert_eq!(count(), before + 1);
        drop(p);
        std::thread::sleep(std::time::Duration::from_secs(121));
        assert_eq!(count(), before);
    }
}
