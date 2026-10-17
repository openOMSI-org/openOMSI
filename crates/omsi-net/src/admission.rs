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
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, secret), &challenge.to_be_bytes());
    u64::from_be_bytes(tag.as_ref()[..8].try_into().expect("8 bytes"))
}

pub(crate) struct Rebind {
    key: hmac::Key,
    started: Instant,
}

impl Rebind {
    pub fn new() -> Rebind {
        Rebind { key: hmac::Key::new(hmac::HMAC_SHA256, &random_bytes::<32>()), started: Instant::now() }
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

    pub fn proven(&self, nonce: u64, from: SocketAddr, secret: &[u8; 16], challenge: &str, answer: &str) -> bool {
        let (Ok(c), Ok(a)) = (u64::from_str_radix(challenge.trim(), 16), u64::from_str_radix(answer.trim(), 16)) else {
            return false;
        };
        let now = self.bucket();
        let fresh = [Some(now), now.checked_sub(1)].into_iter().flatten().any(|b| self.challenge_at(nonce, from, b) == c);
        fresh && proof(secret, c) == a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LanSession, Pose, WorldInfo, PROTOCOL};
    use std::net::UdpSocket;
    use std::time::{Duration, Instant};

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    fn raw() -> UdpSocket {
        let s = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
        s
    }

    fn knock(s: &UdpSocket, host: &mut LanSession, nonce: Option<u64>) -> Option<String> {
        let port = host.local_addr().unwrap().port();
        let nonce = nonce.map(|n| format!("{n:016x}")).unwrap_or_default();
        let msg = format!("HELLO|{PROTOCOL}|-|p||m|1989-05-30|32400|||{nonce}");
        let mut buf = [0u8; 1500];
        for _ in 0..20 {
            let _ = s.send_to(msg.as_bytes(), ("127.0.0.1", port));
            host.tick(0.02, &Pose::default());
            while let Ok((n, _)) = s.recv_from(&mut buf) {
                let t = String::from_utf8_lossy(&buf[..n]).to_string();
                if t.starts_with("WELCOME|") || t.starts_with("REJECT|") {
                    return Some(t);
                }
            }
        }
        None
    }

    #[test]
    fn a_move_is_proven_only_with_the_secret_and_a_challenge_for_that_address() {
        let r = Rebind::new();
        let secret = random_bytes::<16>();
        let from = addr("198.51.100.4:5000");
        let c = r.challenge(7, from);
        let hex = format!("{c:016x}");
        let good = format!("{:016x}", proof(&secret, c));
        assert!(r.proven(7, from, &secret, &hex, &good));
        assert!(!r.proven(7, from, &random_bytes::<16>(), &hex, &good));
        assert!(!r.proven(7, addr("198.51.100.5:5000"), &secret, &hex, &good));
        assert!(!r.proven(8, from, &secret, &hex, &good));
        assert!(!r.proven(7, from, &secret, &format!("{:016x}", c ^ 1), &format!("{:016x}", proof(&secret, c ^ 1))));
        assert!(!r.proven(7, from, &secret, "", ""));
        let s = random_bytes::<16>();
        assert_eq!(parse_secret(&secret_hex(&s)), Some(s));
        assert_eq!(parse_secret("abc"), None);
    }

    #[test]
    fn a_nonce_overheard_does_not_move_a_player_but_its_own_game_does() {
        let mut host = LanSession::host(28250, "host", WorldInfo::default(), true).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut c = LanSession::join(&format!("127.0.0.1:{port}"), "c", WorldInfo::default(), Duration::from_millis(10)).unwrap();
        c.heartbeat = 0.05;
        let t0 = Instant::now();
        while !host.peers.get(&c.my_id).is_some_and(|p| p.settled) && t0.elapsed() < Duration::from_secs(5) {
            c.tick(0.05, &Pose::default());
            host.tick(0.05, &Pose::default());
            std::thread::sleep(Duration::from_millis(10));
        }
        let id = c.my_id;
        let home = host.peers[&id].addr;
        assert!(home.is_some() && c.rebind_secret.is_some());
        let thief = raw();
        let msg = format!("HELLO|{PROTOCOL}|-|c||m|1989-05-30|32400|||{:016x}", c.nonce);
        let mut buf = [0u8; 1500];
        let mut challenge = None;
        for _ in 0..20 {
            let _ = thief.send_to(msg.as_bytes(), ("127.0.0.1", port));
            host.tick(0.02, &Pose::default());
            if let Ok((n, _)) = thief.recv_from(&mut buf) {
                let t = String::from_utf8_lossy(&buf[..n]).to_string();
                assert!(!t.starts_with("WELCOME|"), "{t}");
                challenge = t.strip_prefix("CHALLENGE|").map(str::to_string);
                if challenge.is_some() {
                    break;
                }
            }
        }
        assert_eq!(host.peers[&id].addr, home);
        let challenge = challenge.expect("challenged");
        let wrong = format!("{msg}|{challenge}|{:016x}", proof(&random_bytes::<16>(), u64::from_str_radix(&challenge, 16).unwrap()));
        let _ = thief.send_to(wrong.as_bytes(), ("127.0.0.1", port));
        for _ in 0..10 {
            host.tick(0.02, &Pose::default());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(host.peers[&id].addr, home);
        let right = format!("{msg}|{challenge}|{:016x}", proof(&c.rebind_secret.unwrap(), u64::from_str_radix(&challenge, 16).unwrap()));
        let _ = thief.send_to(right.as_bytes(), ("127.0.0.1", port));
        let t0 = Instant::now();
        while host.peers[&id].addr == home && t0.elapsed() < Duration::from_secs(2) {
            host.tick(0.02, &Pose::default());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(host.peers[&id].addr, Some(thief.local_addr().unwrap()));
    }

    #[test]
    fn a_game_reconnecting_with_several_addresses_answers_the_hosts_challenge() {
        let mut host = LanSession::host(28260, "host", WorldInfo::default(), true).unwrap();
        let port = host.local_addr().unwrap().port();
        let spare = raw();
        let addrs = vec![addr(&format!("127.0.0.1:{port}")), spare.local_addr().unwrap()];
        let mut c = LanSession::join_addr(addrs, None, "c", WorldInfo::default()).unwrap();
        c.heartbeat = 0.05;
        let t0 = Instant::now();
        while c.rebind_secret.is_none() && t0.elapsed() < Duration::from_secs(5) {
            c.tick(0.05, &Pose::default());
            host.tick(0.05, &Pose::default());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(c.rebind_secret.is_some());
        assert!(c.reconnect());
        assert_eq!(c.host, None);
        let to = addr(&format!("127.0.0.1:{}", c.local_addr().unwrap().port()));
        host.send(b"CHALLENGE|00000000000000ab", to);
        let stranger = raw();
        let _ = stranger.send_to(b"CHALLENGE|00000000000000cd", to);
        let t0 = Instant::now();
        while c.answer.is_none() && t0.elapsed() < Duration::from_secs(2) {
            c.tick(0.0, &Pose::default());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(c.answer, Some(0xab));
    }

    #[test]
    fn a_hello_overheard_before_the_player_settles_does_not_get_the_secret() {
        let mut host = LanSession::host(28270, "host", WorldInfo::default(), true).unwrap();
        let real = raw();
        let first = knock(&real, &mut host, Some(0x4444)).unwrap();
        assert!(parse_secret(first.split('|').nth(11).unwrap_or("")).is_some(), "{first}");
        let thief = raw();
        let stolen = knock(&thief, &mut host, Some(0x4444)).unwrap();
        assert!(stolen.starts_with("WELCOME|"), "{stolen}");
        assert_eq!(parse_secret(stolen.split('|').nth(11).unwrap_or("")), None, "{stolen}");
        let again = knock(&real, &mut host, Some(0x4444)).unwrap();
        assert!(parse_secret(again.split('|').nth(11).unwrap_or("")).is_some(), "{again}");
    }

    #[test]
    fn a_stranger_from_outside_who_names_no_session_is_not_told_it() {
        let mut host = LanSession::host(28280, "host", WorldInfo::default(), true).unwrap();
        let hex = crate::session_hex(host.session);
        assert_eq!(host.welcome_session(addr("203.0.113.5:4000"), false), "-");
        assert_eq!(host.welcome_session(addr("203.0.113.5:4000"), true), hex);
        assert_eq!(host.welcome_session(addr("127.0.0.1:4000"), false), hex);
        assert_eq!(host.welcome_session(addr("192.168.1.20:4000"), false), hex);
        host.tell_strangers_the_session();
        assert_eq!(host.welcome_session(addr("203.0.113.5:4000"), false), hex);
    }

    #[test]
    fn a_ban_holds_the_address_as_well_as_the_nonce() {
        let bans = vec![Ban { nonce: Some(5), ip: Some("203.0.113.9".parse().unwrap()), why: "griefing".into() }];
        assert_eq!(banned(&bans, Some(5), addr("192.0.2.1:1")), Some("griefing"));
        assert_eq!(banned(&bans, Some(6), addr("203.0.113.9:4000")), Some("griefing"));
        assert_eq!(banned(&bans, None, addr("203.0.113.9:4001")), Some("griefing"));
        assert_eq!(banned(&bans, Some(6), addr("192.0.2.1:1")), None);
        assert_eq!(source_ip(addr("127.0.0.1:9")), None);
        let v6 = vec![Ban { nonce: None, ip: source_ip(addr("[2001:db8:5:6::1]:7")), why: "v6".into() }];
        assert_eq!(banned(&v6, Some(1), addr("[2001:db8:5:6:aaaa::2]:9")), Some("v6"));
        assert_eq!(banned(&v6, Some(1), addr("[2001:db8:5:7::1]:9")), None);
        let local = raw();
        let _g = crate::ws::GatewayClient::register(local.local_addr().unwrap(), "127.0.0.1".parse().unwrap());
        assert_eq!(source_ip(local.local_addr().unwrap()), None);
    }

    #[test]
    fn a_banned_player_stays_out_with_a_new_game_and_a_new_connection() {
        let mut host = LanSession::host(28240, "host", WorldInfo::default(), true).unwrap();
        let first = raw();
        let _a = crate::ws::GatewayClient::register(first.local_addr().unwrap(), "198.51.100.4".parse().unwrap());
        let welcome = knock(&first, &mut host, Some(0x1111)).unwrap();
        let id: u32 = welcome.split('|').nth(2).unwrap().parse().unwrap();
        host.kick(id, "griefing", true);
        let again = raw();
        let _b = crate::ws::GatewayClient::register(again.local_addr().unwrap(), "198.51.100.4".parse().unwrap());
        assert!(knock(&again, &mut host, Some(0x2222)).unwrap().contains("griefing"));
        let elsewhere = raw();
        let _c = crate::ws::GatewayClient::register(elsewhere.local_addr().unwrap(), "203.0.113.7".parse().unwrap());
        assert!(knock(&elsewhere, &mut host, None).unwrap().starts_with("REJECT|"));
        let fresh = raw();
        let _d = crate::ws::GatewayClient::register(fresh.local_addr().unwrap(), "203.0.113.7".parse().unwrap());
        assert!(knock(&fresh, &mut host, Some(0x3333)).unwrap().starts_with("WELCOME|"));
    }

    /// Two players behind the web gateway count as two sources (their own addresses), not
    /// as the gateway's one loopback; a socket of this machine counts as none.
    #[test]
    fn gateway_players_are_their_own_source() {
        let (a, b) = (addr("127.0.0.1:40101"), addr("127.0.0.1:40102"));
        let _a = crate::ws::GatewayClient::register(a, "198.51.100.1".parse().unwrap());
        let _b = crate::ws::GatewayClient::register(b, "198.51.100.2".parse().unwrap());
        assert_eq!(source_ip(a), Some("198.51.100.1".parse().unwrap()));
        assert_eq!(source_ip(b), Some("198.51.100.2".parse().unwrap()));
        assert_eq!(source_ip(addr("127.0.0.1:40103")), None);
        assert_eq!(source_ip(addr("[2001:db8:1:2:3:4:5:6]:9")), Some("2001:db8:1:2::".parse().unwrap()));
    }
}
