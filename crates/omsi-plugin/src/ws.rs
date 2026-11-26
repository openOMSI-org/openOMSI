//! `omsi.ws`: the read-only WebSocket feeds a plugin may listen to (#3098).
//!
//! A plugin cannot open sockets of its own (its `send` only reaches programs on this
//! computer), so a plugin that wants live data from an online service had to lean on a helper
//! program beside the game. These feeds replace it, with limits in the spirit of `send`:
//!
//! * `wss://` only - no plain `ws://`, no port of its own, no user name or password in the
//!   address, no literal IP address, no redirects; the host must be one the player allowed;
//! * the connection runs on its own thread, so the game never waits for the network; the
//!   messages are queued (a few, the oldest goes first) and a broken feed opens again by
//!   itself after a pause;
//! * a plugin holds at most [`SLOTS`] feeds, each with its own address and queue.
//!
//! Nothing is ever sent: the plugin drains what the server pushed.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Most feeds one plugin may hold at the same time.
pub const SLOTS: usize = 3;
/// Longest message kept (bytes); a longer one is dropped.
pub const MAX_MESSAGE: usize = 64 * 1024;
/// Most messages a feed keeps waiting for the plugin (the oldest goes first).
pub const MAX_QUEUE: usize = 32;
/// How long a broken feed waits before it opens again.
pub const RETRY: Duration = Duration::from_secs(5);
/// Longest address a plugin may give.
const MAX_URL: usize = 512;
/// The file the player edits to allow hosts, in the plugins folder.
pub const HOSTS_FILE: &str = "websocket-hosts.txt";

/// A feed address that passed the checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The address as it will be opened.
    pub url: String,
    /// Its host, lower case.
    pub host: String,
}

/// Whether `host` is an IP address the URL parser could read as a literal, including the
/// decimal, hexadecimal and octal forms of IPv4. A player's allow-list cannot make one legal.
fn literal_host(host: &str) -> bool {
    if host.parse::<std::net::Ipv4Addr>().is_ok() || host.parse::<std::net::Ipv6Addr>().is_ok() {
        return true;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return false;
    }
    parts.iter().all(|p| {
        p.chars().all(|c| c.is_ascii_digit())
            || p.strip_prefix("0x")
                .is_some_and(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
            || (p.len() > 1
                && p.starts_with('0')
                && p[1..].chars().all(|c| ('0'..='7').contains(&c)))
    }) && parts
        .iter()
        .any(|p| p.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// Check `url` against the player's allow-list. The error says what is wrong (the plugin
/// shows it to whoever wrote the plugin), the same way `send` refuses a port.
pub fn endpoint(url: &str, allow: &[String]) -> Result<Endpoint, String> {
    let u = url.trim();
    if u.is_empty() {
        return Err("an address is needed".into());
    }
    if u.len() > MAX_URL {
        return Err(format!("an address is at most {MAX_URL} characters"));
    }
    if u.chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return Err("an address cannot hold spaces, control characters or backslashes".into());
    }
    let lower = u.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("wss://")
        .ok_or("only wss:// addresses (a plain ws:// is not allowed)")?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, _) = rest.split_at(authority_end);
    if authority.is_empty() {
        return Err("the address has no host".into());
    }
    if authority.contains('@') {
        return Err("a user name or password in the address is not allowed".into());
    }
    // (a port of its own: the game never lets a plugin choose one)
    if authority.contains(':') {
        let bracketed = authority.starts_with('[');
        if !bracketed || authority.split("]:").count() > 2 {
            return Err("a port in the address is not allowed".into());
        }
        return Err("a literal address is not allowed, give the host name".into());
    }
    let host = authority.trim_end_matches('.').to_string();
    if host.is_empty() {
        return Err("the address has no host".into());
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return Err(format!("\"{host}\" is not a host name"));
    }
    if literal_host(&host) {
        return Err(format!(
            "\"{host}\" is a literal address, give the host name"
        ));
    }
    if allow.is_empty() {
        return Err(format!(
            "no host is allowed yet: the player adds them to {HOSTS_FILE} in the plugins folder"
        ));
    }
    if !allow.iter().any(|a| a.eq_ignore_ascii_case(&host)) {
        return Err(format!(
            "\"{host}\" is not on the player's list of allowed hosts ({HOSTS_FILE})"
        ));
    }
    Ok(Endpoint {
        url: u.to_string(),
        host,
    })
}

/// The hosts of a `websocket-hosts.txt`: one per line, `#` starts a comment, case does not
/// matter, at most [`SLOTS`] are kept (the file is the player's, the game only reads it).
pub fn hosts_from(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || out.len() >= SLOTS {
            continue;
        }
        // (a whole address is accepted as well as a bare host: wss://host/path)
        let line = line
            .strip_prefix("wss://")
            .or_else(|| line.strip_prefix("WSS://"))
            .unwrap_or(line);
        let host = line
            .split(['/', '?', ':'])
            .next()
            .unwrap_or("")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if !out.iter().any(|h| h == &host) {
            out.push(host);
        }
    }
    out
}

/// The hosts the player allowed, read from the plugins folder at start-up (see
/// [`load_hosts`]). Empty until then, which allows nothing.
static HOSTS: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Read `websocket-hosts.txt` from the plugins folder `dir` (`#` comments, one host per
/// line, at most [`SLOTS`]) and keep it. A missing file allows nothing.
pub fn load_hosts(dir: &std::path::Path) {
    let text = std::fs::read_to_string(dir.join(HOSTS_FILE)).unwrap_or_default();
    let hosts = hosts_from(&text);
    if !hosts.is_empty() {
        log::info!(
            "ws feeds: {} host(s) allowed: {}",
            hosts.len(),
            hosts.join(", ")
        );
    }
    *HOSTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(hosts);
}

/// The hosts the player allowed (nothing before [`load_hosts`] ran, in the tests as well).
pub fn hosts() -> Vec<String> {
    HOSTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// What one read of a feed gave.
pub enum Next {
    /// A text message the server pushed.
    Text(String),
    /// Nothing yet.
    Idle,
    /// The connection is over (closed by the server or broken).
    Closed,
}

/// A live connection. The plugin never writes to it.
pub trait Link: Send {
    fn next(&mut self) -> Next;
}

/// How a feed gets its connection (the tests bring their own).
pub type Connector = Arc<dyn Fn(&str) -> Result<Box<dyn Link>, String> + Send + Sync>;

/// One feed: its queue, its state and the flag that stops its thread.
struct Feed {
    queue: Arc<Mutex<VecDeque<String>>>,
    state: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The feeds one plugin holds.
pub struct Feeds {
    slots: [Option<Feed>; SLOTS],
    connector: Connector,
}

impl Default for Feeds {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Feeds {
    fn drop(&mut self) {
        for slot in self.slots.iter_mut().flatten() {
            slot.stop.store(true, Ordering::Relaxed);
        }
    }
}

impl Feeds {
    /// Feeds over real WebSocket connections.
    pub fn new() -> Feeds {
        Feeds::with_connector(Arc::new(|url| live(url)))
    }

    /// Feeds whose connections come from `connector` (the tests).
    pub fn with_connector(connector: Connector) -> Feeds {
        Feeds {
            slots: Default::default(),
            connector,
        }
    }

    /// Open `url` (checked against `allow`) in the first free slot; gives the slot number.
    pub fn open_with(&mut self, url: &str, allow: &[String]) -> Result<usize, String> {
        let end = endpoint(url, allow)?;
        let slot = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or_else(|| format!("a plugin holds at most {SLOTS} feeds; close one first"))?;
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let state = Arc::new(Mutex::new(String::from("connecting")));
        let stop = Arc::new(AtomicBool::new(false));
        let (q, s, st) = (queue.clone(), state.clone(), stop.clone());
        let connector = self.connector.clone();
        let url = end.url.clone();
        std::thread::Builder::new()
            .name(format!("plugin ws {slot}"))
            .spawn(move || run(url, connector, q, s, st))
            .map_err(|e| e.to_string())?;
        self.slots[slot] = Some(Feed { queue, state, stop });
        Ok(slot)
    }

    /// Drain what arrived in slot `slot` (oldest first).
    pub fn poll(&mut self, slot: usize) -> Result<Vec<String>, String> {
        let feed = self.slot(slot)?;
        let mut q = feed.queue.lock().unwrap_or_else(|e| e.into_inner());
        Ok(q.drain(..).collect())
    }

    /// What slot `slot` is doing: `connecting`, `open`, `reconnecting`, `failed`, `closed`.
    pub fn state(&self, slot: usize) -> Result<String, String> {
        let feed = self.slot(slot)?;
        Ok(feed.state.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    /// Close slot `slot` and free it.
    pub fn close(&mut self, slot: usize) -> Result<(), String> {
        let feed = self.slot(slot)?;
        feed.stop.store(true, Ordering::Relaxed);
        self.slots[slot] = None;
        Ok(())
    }

    fn slot(&self, slot: usize) -> Result<&Feed, String> {
        if slot >= SLOTS {
            return Err(format!("the slot is 0-{}", SLOTS - 1));
        }
        self.slots[slot]
            .as_ref()
            .ok_or_else(|| format!("no feed in slot {slot}"))
    }
}

fn set_state(state: &Mutex<String>, s: &str) {
    let mut g = state.lock().unwrap_or_else(|e| e.into_inner());
    g.clear();
    g.push_str(s);
}

fn push(queue: &Mutex<VecDeque<String>>, text: String) {
    let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
    while q.len() >= MAX_QUEUE {
        q.pop_front();
    }
    q.push_back(text);
}

/// One feed's thread: connect, drain, and when the connection ends wait and connect again.
fn run(
    url: String,
    connector: Connector,
    queue: Arc<Mutex<VecDeque<String>>>,
    state: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        set_state(&state, "connecting");
        match connector(&url) {
            Ok(mut link) => {
                set_state(&state, "open");
                loop {
                    if stop.load(Ordering::Relaxed) {
                        set_state(&state, "closed");
                        return;
                    }
                    match link.next() {
                        Next::Text(t) => {
                            if t.len() <= MAX_MESSAGE {
                                push(&queue, t);
                            }
                        }
                        Next::Idle => std::thread::sleep(Duration::from_millis(20)),
                        Next::Closed => break,
                    }
                }
                set_state(&state, "reconnecting");
            }
            Err(e) => {
                log::warn!("ws feed {url}: {e}");
                set_state(&state, "failed");
            }
        }
        let until = Instant::now() + RETRY;
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    set_state(&state, "closed");
}

/// A live feed over `tungstenite`, with the short read timeout the loop above needs.
fn live(url: &str) -> Result<Box<dyn Link>, String> {
    use tungstenite::stream::MaybeTlsStream;
    let (tx, rx) = std::sync::mpsc::channel();
    let u = url.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(
            tungstenite::client::connect(&u)
                .map(|(ws, _)| ws)
                .map_err(|e| format!("{u}: {e}")),
        );
    });
    let mut ws = rx
        .recv_timeout(Duration::from_secs(12))
        .map_err(|_| format!("{url}: no answer within 12 s"))??;
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(Duration::from_millis(20))),
        MaybeTlsStream::Rustls(s) => s
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(20))),
        _ => Ok(()),
    }
    .map_err(|e| e.to_string())?;
    Ok(Box::new(Live { ws }))
}

struct Live {
    ws: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
}

impl Link for Live {
    fn next(&mut self) -> Next {
        use tungstenite::{Error, Message};
        match self.ws.read() {
            Ok(Message::Text(t)) => Next::Text(t.to_string()),
            Ok(Message::Close(_)) => Next::Closed,
            Ok(_) => Next::Idle,
            Err(Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Next::Idle
            }
            Err(Error::ConnectionClosed) | Err(Error::AlreadyClosed) => Next::Closed,
            Err(e) => {
                log::warn!("ws feed: {e}");
                Next::Closed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn hosts(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_wss_hosts_the_player_allowed_are_opened() {
        let allow = hosts(&["feeds.example.org"]);
        assert_eq!(
            endpoint("wss://feeds.example.org/live", &allow)
                .unwrap()
                .host,
            "feeds.example.org"
        );
        // the scheme is not free
        assert!(
            endpoint("ws://feeds.example.org/live", &allow)
                .unwrap_err()
                .contains("wss://")
        );
        assert!(endpoint("https://feeds.example.org/live", &allow).is_err());
        // nor is the host
        assert!(endpoint("wss://evil.example.org/live", &allow).is_err());
        assert!(
            endpoint("wss://example.org/live", &allow).is_err(),
            "a subdomain is not the host"
        );
        // nor a port, a password, a literal address
        assert!(
            endpoint("wss://feeds.example.org:8443/live", &allow)
                .unwrap_err()
                .contains("port")
        );
        assert!(
            endpoint("wss://user:pw@feeds.example.org/live", &allow)
                .unwrap_err()
                .contains("password")
        );
        // a literal address stays refused even when the player lists it
        let literals = hosts(&["127.0.0.1", "2130706433", "0x7f000001", "0177.0.0.1", "::1"]);
        for url in [
            "wss://127.0.0.1/live",
            "wss://2130706433/live",
            "wss://0x7f000001/live",
            "wss://0177.0.0.1/live",
            "wss://[::1]/live",
        ] {
            let err = endpoint(url, &literals).unwrap_err();
            assert!(
                err.contains("literal address"),
                "{url} was refused, but as {err}"
            );
        }
        // and a broken address is a broken address
        assert!(endpoint("", &allow).is_err());
        assert!(endpoint("wss://", &allow).is_err());
        assert!(endpoint("wss://feeds example.org/live", &allow).is_err());
        assert!(
            endpoint(
                &format!("wss://feeds.example.org/{}", "a".repeat(MAX_URL)),
                &allow
            )
            .is_err()
        );
        // the list may be empty: then nothing opens
        assert!(
            endpoint("wss://feeds.example.org/live", &[])
                .unwrap_err()
                .contains(HOSTS_FILE)
        );
        // case does not matter, and a trailing dot is the same host
        assert!(endpoint("WSS://FEEDS.Example.ORG/x", &allow).is_ok());
        assert_eq!(
            endpoint("wss://feeds.example.org./x", &allow).unwrap().host,
            "feeds.example.org"
        );
    }

    #[test]
    fn the_players_file_names_at_most_three_hosts() {
        let text = "# my feeds\nfeeds.example.org   # the position feed\n\nwss://second.example.org/live\nFEEDS.example.org\nthird.example.org\nfourth.example.org\n";
        assert_eq!(
            hosts_from(text),
            hosts(&[
                "feeds.example.org",
                "second.example.org",
                "third.example.org"
            ])
        );
        assert!(hosts_from("").is_empty());
        assert!(hosts_from("# nothing but a comment\n").is_empty());
    }

    /// A link that gives back the script and then says the connection is over.
    struct Script(Vec<Option<String>>, usize);

    impl Link for Script {
        fn next(&mut self) -> Next {
            let i = self.0.get(self.1).cloned();
            self.1 += 1;
            match i {
                Some(Some(t)) => Next::Text(t),
                Some(None) => Next::Idle,
                None => Next::Closed,
            }
        }
    }

    fn scripted(script: Vec<Option<String>>) -> (Connector, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        let c: Connector = Arc::new(move |_url: &str| {
            let _ = tx.send(());
            Ok(Box::new(Script(script.clone(), 0)) as Box<dyn Link>)
        });
        (c, rx)
    }

    fn wait_state(feeds: &Feeds, slot: usize, want: &str) {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if feeds.state(slot).unwrap() == want {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "slot {slot} never became {want}: {}",
            feeds.state(slot).unwrap()
        );
    }

    #[test]
    fn a_feed_queues_what_arrives_and_a_plugin_drains_it() {
        let (connector, _opened) = scripted(vec![
            Some("one".into()),
            Some("two".into()),
            None,
            Some("three".into()),
        ]);
        let mut feeds = Feeds::with_connector(connector);
        let allow = hosts(&["feeds.example.org"]);
        let slot = feeds
            .open_with("wss://feeds.example.org/live", &allow)
            .unwrap();
        assert_eq!(slot, 0);
        let until = Instant::now() + Duration::from_secs(5);
        let mut got: Vec<String> = Vec::new();
        while Instant::now() < until && got.len() < 3 {
            got.extend(feeds.poll(slot).unwrap());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            got,
            vec!["one", "two", "three"],
            "in the order the server pushed them"
        );
        feeds.close(slot).unwrap();
        assert!(feeds.poll(0).is_err(), "a closed slot has no feed");
    }

    #[test]
    fn a_queue_keeps_the_newest_messages_only() {
        let many: Vec<Option<String>> = (0..MAX_QUEUE + 8)
            .map(|i| Some(format!("{i}")))
            .chain(std::iter::once(None))
            .collect();
        let (connector, _opened) = scripted(many);
        let mut feeds = Feeds::with_connector(connector);
        let slot = feeds
            .open_with(
                "wss://feeds.example.org/live",
                &hosts(&["feeds.example.org"]),
            )
            .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        let mut got: Vec<String> = Vec::new();
        // (wait until nothing arrives for a moment, so the whole script has gone through)
        while Instant::now() < until {
            got.extend(feeds.poll(slot).unwrap());
            if got.len() >= MAX_QUEUE {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(got.len() <= MAX_QUEUE, "{} messages kept", got.len());
        assert_eq!(
            got.last().map(String::as_str),
            Some(&format!("{}", MAX_QUEUE + 7)[..]),
            "the newest arrived"
        );
        feeds.close(slot).unwrap();
    }

    #[test]
    fn a_plugin_holds_three_feeds_and_a_fourth_is_refused() {
        let (connector, _opened) = scripted(vec![None]);
        let mut feeds = Feeds::with_connector(connector);
        let allow = hosts(&["a.example.org", "b.example.org", "c.example.org"]);
        for (i, h) in ["a", "b", "c"].iter().enumerate() {
            assert_eq!(
                feeds
                    .open_with(&format!("wss://{h}.example.org/x"), &allow)
                    .unwrap(),
                i
            );
        }
        assert!(
            feeds
                .open_with("wss://a.example.org/y", &allow)
                .unwrap_err()
                .contains("at most 3")
        );
        feeds.close(1).unwrap();
        assert_eq!(
            feeds.open_with("wss://b.example.org/y", &allow).unwrap(),
            1,
            "a closed slot is free again"
        );
    }

    #[test]
    fn a_closed_feed_frees_its_slot() {
        let (connector, _opened) = scripted(vec![None]);
        let mut feeds = Feeds::with_connector(connector);
        let slot = feeds
            .open_with(
                "wss://feeds.example.org/live",
                &hosts(&["feeds.example.org"]),
            )
            .unwrap();
        assert!(feeds.state(slot).is_ok());
        feeds.close(slot).unwrap();
        assert!(feeds.state(slot).is_err(), "a closed slot has no feed");
        assert!(feeds.poll(slot).is_err());
        assert!(feeds.close(slot).is_err());
        assert!(feeds.close(SLOTS).unwrap_err().contains("0-2"));
    }

    #[test]
    fn a_feed_that_cannot_connect_says_so_and_tries_again() {
        let (tx, rx) = mpsc::channel();
        let c: Connector = Arc::new(move |_url: &str| {
            let _ = tx.send(());
            Err("no route to host".to_string())
        });
        let mut feeds = Feeds::with_connector(c);
        let slot = feeds
            .open_with(
                "wss://feeds.example.org/live",
                &hosts(&["feeds.example.org"]),
            )
            .unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        wait_state(&feeds, slot, "failed");
        // and after the pause it is at it again
        let mut tries = 1;
        let until = Instant::now() + RETRY + Duration::from_secs(3);
        while Instant::now() < until {
            if rx.recv_timeout(Duration::from_millis(200)).is_ok() {
                tries += 1;
                break;
            }
        }
        assert!(tries >= 2, "the feed opened again after its pause");
        feeds.close(slot).unwrap();
    }
}
