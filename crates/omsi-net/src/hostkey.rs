use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct HostKey {
    pair: Ed25519KeyPair,
}

impl HostKey {
    pub fn generate() -> Option<HostKey> {
        let rng = ring::rand::SystemRandom::new();
        for _ in 0..8 {
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).ok()?;
            let key = HostKey { pair: Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).ok()? };
            if key.session() != 0 {
                return Some(key);
            }
        }
        None
    }

    pub fn public(&self) -> &[u8] {
        self.pair.public_key().as_ref()
    }

    pub fn session(&self) -> u64 {
        session_of(self.public())
    }

    pub fn sign(&self, purpose: &str, msg: &[u8]) -> Vec<u8> {
        self.pair.sign(&framed(purpose, msg)).as_ref().to_vec()
    }

    pub fn seal(&self, purpose: &str, text: &str) -> String {
        format!("{text} #{}.{}", hex(self.public()), hex(&self.sign(purpose, text.as_bytes())))
    }
}

impl std::fmt::Debug for HostKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HostKey({})", crate::session_hex(self.session()))
    }
}

pub fn session_of(public: &[u8]) -> u64 {
    use sha2::{Digest, Sha256};
    let h = Sha256::new().chain_update(b"omsi2rw-host-key").chain_update(public).finalize();
    u64::from_be_bytes([0, 0, h[0], h[1], h[2], h[3], h[4], h[5]])
}

fn framed(purpose: &str, msg: &[u8]) -> Vec<u8> {
    let mut v = b"openBus host ".to_vec();
    v.extend_from_slice(purpose.as_bytes());
    v.push(0);
    v.extend_from_slice(msg);
    v
}

pub fn verify(session: u64, public: &[u8], purpose: &str, msg: &[u8], sig: &[u8]) -> bool {
    session != 0 && public.len() == 32 && session_of(public) == session && UnparsedPublicKey::new(&ED25519, public).verify(&framed(purpose, msg), sig).is_ok()
}

pub fn opened<'a>(session: u64, purpose: &str, post: &'a str) -> Option<&'a str> {
    let (text, seal) = post.trim().rsplit_once(" #")?;
    let (public, sig) = seal.trim().split_once('.')?;
    verify(session, &unhex(public)?, purpose, text.as_bytes(), &unhex(sig)?).then_some(text)
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub const MAX_AGE: u64 = 24 * 3600;
const MAX_AHEAD: u64 = 12 * 3600;

pub fn fresh(at: u64, now: u64, max_age: u64) -> bool {
    at <= now + MAX_AHEAD && now.saturating_sub(at) <= max_age
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_text_opens_only_for_the_session_the_key_names() {
        let key = HostKey::generate().unwrap();
        let other = HostKey::generate().unwrap();
        let post = key.seal("relay", "W 100 https://a.trycloudflare.com");
        assert_eq!(opened(key.session(), "relay", &post), Some("W 100 https://a.trycloudflare.com"));
        assert_eq!(opened(other.session(), "relay", &post), None);
        assert_eq!(opened(key.session(), "mods", &post), None);
        assert_eq!(opened(key.session(), "relay", &post.replace("a.try", "b.try")), None);
        let forged = other.seal("relay", "W 100 https://evil.example");
        assert_eq!(opened(key.session(), "relay", &forged), None);
        let swapped = format!("W 100 https://evil.example #{}", post.rsplit_once(" #").unwrap().1);
        assert_eq!(opened(key.session(), "relay", &swapped), None);
        assert_eq!(opened(key.session(), "relay", "W 100 https://evil.example"), None);
    }

    #[test]
    fn a_session_id_is_the_fingerprint_of_its_key() {
        let key = HostKey::generate().unwrap();
        assert_eq!(key.session(), session_of(key.public()));
        assert!(key.session() != 0 && key.session() <= 0xFFFF_FFFF_FFFF);
        assert!(!verify(key.session() ^ 1, key.public(), "mods", b"x", &key.sign("mods", b"x")));
        assert!(verify(key.session(), key.public(), "mods", b"x", &key.sign("mods", b"x")));
    }

    #[test]
    fn old_and_future_posts_are_not_fresh() {
        let now = 100_000;
        assert!(fresh(now, now, 60));
        assert!(fresh(now + 2 * 3600, now, MAX_AGE));
        assert!(fresh(now + 12 * 3600, now, MAX_AGE));
        assert!(!fresh(now + 13 * 3600, now, MAX_AGE));
        assert!(fresh(now - 23 * 3600, now, MAX_AGE));
        assert!(!fresh(now - 25 * 3600, now, MAX_AGE));
        assert!(!fresh(now - 61, now, 60));
    }
}
