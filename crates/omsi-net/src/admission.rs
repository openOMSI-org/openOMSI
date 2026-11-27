use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};

const BUCKET: Duration = Duration::from_secs(30);

pub(crate) struct Ban {
    pub nonce: Option<u64>,
    pub ip: Option<IpAddr>,
    pub why: String,
}

/// Who a datagram comes from, as bans and locks count it: the player behind the web
/// gateway (not the gateway's own loopback socket), an IPv6 address by its /64; none for
/// this machine.
pub fn source_ip(from: SocketAddr) -> Option<IpAddr> {
    let ip = crate::ws::gateway_client(from).unwrap_or(from.ip());
    (!ip.is_loopback() && !ip.is_unspecified()).then(|| crate::ws::source_key(ip))
}

pub(crate) fn banned(bans: &[Ban], nonce: Option<u64>, from: SocketAddr) -> Option<&str> {
    let ip = source_ip(from);
    bans.iter()
        .find(|b| (b.nonce.is_some() && b.nonce == nonce) || (b.ip.is_some() && b.ip == ip))
        .map(|b| b.why.as_str())
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    if SystemRandom::new().fill(&mut out).is_err() {
        for chunk in out.chunks_mut(6) {
            let r = crate::random_session_id().to_le_bytes();
            chunk.copy_from_slice(&r[..chunk.len()]);
        }
    }
    out
}

pub(crate) fn secret_hex(secret: &[u8; 16]) -> String {
    secret.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn parse_secret(hex: &str) -> Option<[u8; 16]> {
    let hex = hex.trim();
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

pub(crate) fn proof(secret: &[u8; 16], challenge: u64) -> u64 {
    let tag = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, secret),
        &challenge.to_be_bytes(),
    );
    u64::from_be_bytes(tag.as_ref()[..8].try_into().expect("8 bytes"))
}

pub(crate) struct Rebind {
    key: hmac::Key,
    started: Instant,
}

impl Rebind {
    pub fn new() -> Rebind {
        Rebind {
            key: hmac::Key::new(hmac::HMAC_SHA256, &random_bytes::<32>()),
            started: Instant::now(),
        }
    }

    fn challenge_at(&self, nonce: u64, from: SocketAddr, bucket: u64) -> u64 {
        let mut ctx = hmac::Context::with_key(&self.key);
        ctx.update(&nonce.to_be_bytes());
        ctx.update(from.to_string().as_bytes());
        ctx.update(&bucket.to_be_bytes());
        u64::from_be_bytes(ctx.sign().as_ref()[..8].try_into().expect("8 bytes"))
    }

    fn bucket(&self) -> u64 {
        self.started.elapsed().as_secs() / BUCKET.as_secs()
    }

    pub fn challenge(&self, nonce: u64, from: SocketAddr) -> u64 {
        self.challenge_at(nonce, from, self.bucket())
    }

    pub fn proven(
        &self,
        nonce: u64,
        from: SocketAddr,
        secret: &[u8; 16],
        challenge: &str,
        answer: &str,
    ) -> bool {
        let (Ok(c), Ok(a)) = (
            u64::from_str_radix(challenge.trim(), 16),
            u64::from_str_radix(answer.trim(), 16),
        ) else {
            return false;
        };
        let now = self.bucket();
        let fresh = [Some(now), now.checked_sub(1)]
            .into_iter()
            .flatten()
            .any(|b| self.challenge_at(nonce, from, b) == c);
        fresh && proof(secret, c) == a
    }
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
