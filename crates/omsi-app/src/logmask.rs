//! The log without the players' internet addresses: a game hosting a session and a dedicated
//! server log who joins and from where, and `game.log` / `server.log` are kept, sent with
//! bug reports and passed around. A public address keeps its network (`203.0.113.x`,
//! `2001:db8:85a3::x`), which is what tells a connection problem apart; this machine's and
//! the local network's own addresses are left whole. A dedicated server's `log_addresses = 1`
//! keeps them all whole (an owner who needs them for an abuse report).

use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicBool, Ordering};

static WHOLE: AtomicBool = AtomicBool::new(false);

/// `server.cfg`'s `log_addresses`: the addresses logged whole from now on.
pub(crate) fn keep_whole(whole: bool) {
    WHOLE.store(whole, Ordering::Relaxed);
}

/// The logger of the game: env_logger's, every line it is given with the public addresses cut.
pub(crate) fn init() {
    let inner = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
    let max = inner.filter();
    if log::set_boxed_logger(Box::new(Masked(inner))).is_ok() {
        log::set_max_level(max);
    }
}

struct Masked(env_logger::Logger);

impl log::Log for Masked {
    fn enabled(&self, m: &log::Metadata) -> bool {
        self.0.enabled(m)
    }

    fn log(&self, r: &log::Record) {
        if !self.0.matches(r) {
            return;
        }
        // (a line without arguments has no address put in it)
        if WHOLE.load(Ordering::Relaxed) || r.args().as_str().is_some() {
            return self.0.log(r);
        }
        let text = r.args().to_string();
        match mask(&text) {
            Cow::Borrowed(_) => self.0.log(r),
            Cow::Owned(m) => self.0.log(
                &log::Record::builder()
                    .args(format_args!("{m}"))
                    .level(r.level())
                    .target(r.target())
                    .module_path(r.module_path())
                    .file(r.file())
                    .line(r.line())
                    .build(),
            ),
        }
    }

    fn flush(&self) {
        self.0.flush()
    }
}

/// `text` with every public IPv4 and IPv6 address cut to its network.
pub(crate) fn mask(text: &str) -> Cow<'_, str> {
    let addressy = |c: char| c.is_ascii_hexdigit() || c == '.' || c == ':';
    let mut out: Option<String> = None;
    let mut at = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        let c = bytes[i] as char;
        // (a run starts where a word does not: "v1.2.3.4" is no address)
        let starts = addressy(c) && (i == 0 || !(bytes[i - 1] as char).is_alphanumeric() && bytes[i - 1] != b'.' && bytes[i - 1] != b':');
        if !starts {
            i += 1;
            continue;
        }
        let end = text[i..].find(|c: char| !addressy(c)).map_or(text.len(), |n| i + n);
        let run = &text[i..end];
        if let Some((len, masked)) = address(run) {
            let o = out.get_or_insert_with(|| String::with_capacity(text.len()));
            o.push_str(&text[at..i]);
            o.push_str(&masked);
            at = i + len;
        }
        i = end.max(i + 1);
    }
    match out {
        None => Cow::Borrowed(text),
        Some(mut o) => {
            o.push_str(&text[at..]);
            Cow::Owned(o)
        }
    }
}

/// The address at the start of `run` (a run of hex digits, dots and colons) and how it is
/// logged; None when it is no public address. A port after an IPv4 address (`:27015`) and a
/// sentence's full stop stay outside it.
fn address(run: &str) -> Option<(usize, String)> {
    if run.matches(':').count() >= 2 {
        // (a whole IPv6 address; "08:00:12" and the like do not parse)
        let a = run.trim_end_matches(['.', ':']);
        if let Ok(ip) = a.parse::<Ipv6Addr>() {
            return masked(IpAddr::V6(ip)).map(|m| (a.len(), m));
        }
    }
    let v4 = run.split(':').next().unwrap_or("").trim_end_matches('.');
    let ip = v4.parse::<Ipv4Addr>().ok()?;
    masked(IpAddr::V4(ip)).map(|m| (v4.len(), m))
}

fn masked(ip: IpAddr) -> Option<String> {
    match ip {
        IpAddr::V4(v) => {
            let local = v.is_loopback() || v.is_private() || v.is_link_local() || v.is_unspecified() || v.is_broadcast();
            (!local).then(|| {
                let [a, b, c, _] = v.octets();
                format!("{a}.{b}.{c}.x")
            })
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return masked(IpAddr::V4(v4)).map(|m| format!("::ffff:{m}"));
            }
            let s = v.segments();
            // (loopback, unspecified, unique local fc00::/7, link-local fe80::/10)
            let local = v.is_loopback() || v.is_unspecified() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80;
            (!local).then(|| format!("{:x}:{:x}:{:x}::x", s[0], s[1], s[2]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn public_addresses_keep_their_network_only() {
        let cases = [
            ("LAN: player 3 now talks from 203.0.113.77:51234", "LAN: player 3 now talks from 203.0.113.x:51234"),
            ("this computer is 198.51.100.9 on the internet (STUN).", "this computer is 198.51.100.x on the internet (STUN)."),
            ("gateway: player WebSocket from [2001:db8:85a3:1:2:3:4:5]:443 on 0.0.0.0:27025", "gateway: player WebSocket from [2001:db8:85a3::x]:443 on 0.0.0.0:27025"),
            ("peer ::ffff:203.0.113.5 joined", "peer ::ffff:203.0.113.x joined"),
            ("from 203.0.113.1, 203.0.113.2", "from 203.0.113.x, 203.0.113.x"),
        ];
        for (line, want) in cases {
            assert_eq!(mask(line), want);
        }
    }

    #[test]
    fn local_addresses_times_and_versions_stay_as_they_are() {
        for line in [
            "web administration at http://127.0.0.1:27025/admin/",
            "LAN: host at 192.168.1.20:27015 and 10.0.0.5, 172.16.4.1, 169.254.3.3, [::1]:80, fe80::1, fd00::2",
            "game time 08:00, at 12:34:56, build 0.2.50, Windows 10.0.22631, driver 31.0.15.5244",
            "version v1.2.3.4 and 1.2.3.4.5, mac aa:bb:cc:dd:ee:ff, tile 3.5",
            "no address at all",
        ] {
            assert!(matches!(mask(line), std::borrow::Cow::Borrowed(_)), "{line}");
        }
    }
}
