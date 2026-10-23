//! The official server by name: a player types `openomsi` (the launcher lists it as
//! "openOMSI | Official Server") and is joined to wherever that server is reached now.
//!
//! The server is reached through a Cloudflare quick tunnel, whose address changes whenever
//! it starts again. It posts that address to a fixed topic of the public relay (`bridge`'s
//! ntfy.sh) every few minutes, signed with the official server's Ed25519 key; a game asking
//! for `openomsi` takes the newest post whose signature checks out with the public key
//! below and which is recent. Anybody can post to the topic, nobody else can sign: a forged
//! address is never taken.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What a player types for the official server.
pub const ALIAS: &str = "openomsi";
/// Its name in the launcher's list.
pub const NAME: &str = "openOMSI | Official Server";
/// The relay topic its address is posted under.
const TOPIC: &str = "openomsi-official-server-7f3a9c";
const RELAY: &str = "https://ntfy.sh";
/// The official server's public key (Ed25519).
const PUBLIC_KEY: [u8; 32] = [
    0x3d, 0xda, 0x37, 0xfc, 0x2b, 0x53, 0xcc, 0x9f, 0x38, 0x36, 0x8f, 0xc4, 0xbd, 0x88, 0x2b, 0x1e, 0x09, 0xc1, 0xcf, 0x9b, 0xd4, 0xc5, 0xd4, 0xf4, 0x2a, 0xfe, 0x98, 0x2b, 0x65, 0xa7, 0xd1, 0x2c,
];
/// A post older than this is not taken (the server posts every five minutes).
const FRESH: Duration = Duration::from_secs(40 * 60);

/// Is `target` the official server's name?
pub fn is_alias(target: &str) -> bool {
    target.trim().eq_ignore_ascii_case(ALIAS)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// The text a post signs: the address and the time it was posted.
fn signed_text(url: &str, at: u64) -> String {
    format!("OFFICIAL {url} {at}")
}

/// The address of a post, when its signature is the official key's and it is at most
/// `FRESH` old at `now`.
fn verify(msg: &str, key: &[u8], now: u64) -> Option<(String, u64)> {
    let (text, sig) = msg.trim().rsplit_once(" #")?;
    let mut parts = text.split(' ');
    if parts.next()? != "OFFICIAL" {
        return None;
    }
    let url = parts.next()?.to_string();
    let at: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !url.starts_with("https://") || url.len() > 300 {
        return None;
    }
    let sig = unhex(sig)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key).verify(text.as_bytes(), &sig).ok()?;
    (at <= now + 300 && now.saturating_sub(at) <= FRESH.as_secs()).then_some((url, at))
}

/// Where the official server is reached now (its `https://` address), from the relay.
pub fn resolve() -> Result<String, String> {
    if PUBLIC_KEY == [0; 32] {
        return Err("this build does not know the official server".into());
    }
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).user_agent("openOMSI").build();
    let body = agent
        .get(&format!("{RELAY}/{TOPIC}/json?poll=1&since=1h"))
        .call()
        .map_err(|e| format!("the official server's address could not be read ({e})"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let t = now();
    body.lines()
        .filter_map(|l| crate::bridge::json_field(l, "message"))
        .filter_map(|m| verify(&m, &PUBLIC_KEY, t))
        .max_by_key(|(_, at)| *at)
        .map(|(url, _)| url)
        .ok_or_else(|| "the official server is not online right now".into())
}

/// `target` as an address the game connects to: the official server's current one for its
/// name, else `target` itself.
pub fn resolve_target(target: &str) -> Result<String, String> {
    if is_alias(target) {
        resolve()
    } else {
        Ok(target.to_string())
    }
}

/// The official server: post where it is reached (`url`, its tunnel), signed with its key
/// (a PKCS#8 Ed25519 key, `OMSI_OFFICIAL_KEY` names the file). Call every few minutes.
pub fn announce(url: &str, pkcs8: &[u8]) -> Result<(), String> {
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8).map_err(|e| format!("the official key: {e}"))?;
    let text = signed_text(url, now());
    let sig = pair.sign(text.as_bytes());
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).user_agent("openOMSI").build();
    agent
        .post(&format!("{RELAY}/{TOPIC}"))
        .set("Cache", "yes")
        .send_string(&format!("{text} #{}", hex(sig.as_ref())))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// A new official key: (PKCS#8 document, public key as hex).
pub fn generate_key() -> Result<(Vec<u8>, String), String> {
    use ring::signature::KeyPair;
    let rng = ring::rand::SystemRandom::new();
    let doc = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).map_err(|e| e.to_string())?;
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(doc.as_ref()).map_err(|e| e.to_string())?;
    Ok((doc.as_ref().to_vec(), hex(pair.public_key().as_ref())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;

    #[test]
    fn only_the_key_s_recent_posts_count() {
        let (doc, _) = generate_key().unwrap();
        let pair = ring::signature::Ed25519KeyPair::from_pkcs8(&doc).unwrap();
        let key = pair.public_key().as_ref().to_vec();
        let post = |url: &str, at: u64| {
            let text = signed_text(url, at);
            format!("{text} #{}", hex(pair.sign(text.as_bytes()).as_ref()))
        };
        let t = 1_800_000_000;
        assert_eq!(verify(&post("https://a.trycloudflare.com", t - 60), &key, t).map(|x| x.0).as_deref(), Some("https://a.trycloudflare.com"));
        // too old, forged, tampered
        assert!(verify(&post("https://a.trycloudflare.com", t - 3 * 3600), &key, t).is_none());
        let (other, _) = generate_key().unwrap();
        let other = ring::signature::Ed25519KeyPair::from_pkcs8(&other).unwrap();
        let text = signed_text("https://evil.example", t);
        assert!(verify(&format!("{text} #{}", hex(other.sign(text.as_bytes()).as_ref())), &key, t).is_none());
        let good = post("https://a.trycloudflare.com", t);
        assert!(verify(&good.replace("https://a.", "https://b."), &key, t).is_none());
        assert!(is_alias(" OpenOMSI "));
    }
}
