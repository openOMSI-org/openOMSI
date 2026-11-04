use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const LOCK_AFTER: usize = 5;
const LOCK_ALL_AFTER: usize = 30;
const LOCK_WINDOW: Duration = Duration::from_secs(120);
const MAX_DEVICES: usize = 32;

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    SystemRandom::new().fill(&mut b).expect("the system's random numbers");
    b
}

pub(crate) fn new_pin() -> String {
    let n = u32::from_le_bytes(random_bytes::<4>()) % 1_000_000;
    format!("{n:06}")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

pub(crate) fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut d = (a.len() != b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        d |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    d == 0
}

fn network(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() => IpAddr::V6((u128::from(v6) & !((1u128 << 64) - 1)).into()),
        IpAddr::V6(v6) => IpAddr::V4(v6.to_ipv4_mapped().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED)),
        v4 => v4,
    }
}

pub(crate) struct Devices {
    path: PathBuf,
    known: Vec<(String, String)>,
    failures: Vec<(IpAddr, Instant)>,
}

pub(crate) enum Pairing {
    Paired(String),
    Wrong { renew: bool },
    Locked,
}

impl Devices {
    pub(crate) fn load(path: PathBuf) -> Devices {
        let known = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (h, name) = l.split_once('\t').unwrap_or((l, ""));
                (h.len() == 64 && h.bytes().all(|c| c.is_ascii_hexdigit())).then(|| (h.to_string(), name.to_string()))
            })
            .collect();
        Devices { path, known, failures: Vec::new() }
    }

    fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self.known.iter().map(|(h, n)| format!("{h}\t{n}\n")).collect();
        if let Err(e) = std::fs::write(&self.path, text) {
            log::warn!("remote cockpit: cannot write {}: {e}", self.path.display());
        }
    }

    pub(crate) fn knows(&self, token: &str) -> bool {
        let h = hash(token);
        self.known.iter().fold(false, |found, (k, _)| found | same(k, &h))
    }

    fn locked(&mut self, from: IpAddr) -> bool {
        let from = network(from);
        self.failures.retain(|(_, t)| t.elapsed() < LOCK_WINDOW);
        self.failures.len() >= LOCK_ALL_AFTER || self.failures.iter().filter(|(ip, _)| *ip == from).count() >= LOCK_AFTER
    }

    pub(crate) fn pair(&mut self, pin: &str, wanted: &str, name: &str, from: IpAddr) -> Pairing {
        if self.locked(from) {
            return Pairing::Locked;
        }
        if !same(pin, wanted.trim()) {
            self.failures.push((network(from), Instant::now()));
            return Pairing::Wrong { renew: self.failures.len() == LOCK_ALL_AFTER };
        }
        let token = hex(&random_bytes::<32>());
        let name: String = name.chars().filter(|c| !c.is_control()).take(48).collect();
        self.known.push((hash(&token), name));
        if self.known.len() > MAX_DEVICES {
            self.known.remove(0);
        }
        self.save();
        Pairing::Paired(token)
    }

    pub(crate) fn forget_all(&mut self) {
        self.known.clear();
        self.save();
    }
}
