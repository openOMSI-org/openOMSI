//! This machine's addresses, and which of them another player can reach.
//!
//! A computer has many IPv4 addresses and most of them are no use to a friend: the loopback,
//! the self-assigned 169.254 ones, the bridges of virtual machines and containers. What
//! matters is the address on the network both players share: the home LAN when they sit in
//! the same house, and a VPN's address when they play over the internet - Hamachi hands out
//! 25.x.x.x, Radmin VPN 26.x.x.x, Tailscale 100.64.0.0/10, ZeroTier a range of the network's
//! own choosing on an interface named after it. The session code carries the best few
//! (`SessionCode::ips`), and the joining game tries all of them at once.
//!
//! The list comes from `getifaddrs` on macOS and Linux and from `ipconfig` on Windows (read
//! as bytes: a localised Windows names its adapters in its own language, but "IPv4" and the
//! numbers stay ASCII). It is kept for a few seconds, since the status file and the HUD ask
//! for it all the time.

use std::net::Ipv4Addr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What an address is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddrKind {
    Hamachi,
    Radmin,
    Tailscale,
    ZeroTier,
    /// Another VPN (a tunnel interface with a private address).
    Vpn,
    /// The local network (a private address on an ordinary interface).
    Lan,
    /// An address straight on the internet (no router in between).
    Public,
    /// A bridge of virtual machines or containers: nobody else reaches it.
    Virtual,
    /// 169.254.x.x: the interface got no address from anybody.
    LinkLocal,
}

impl AddrKind {
    /// The name players know it by.
    pub fn label(self) -> &'static str {
        match self {
            AddrKind::Hamachi => "Hamachi",
            AddrKind::Radmin => "Radmin VPN",
            AddrKind::Tailscale => "Tailscale",
            AddrKind::ZeroTier => "ZeroTier",
            AddrKind::Vpn => "VPN",
            AddrKind::Lan => "LAN",
            AddrKind::Public => "internet",
            AddrKind::Virtual => "virtual machines",
            AddrKind::LinkLocal => "link-local",
        }
    }

    /// Short machine-readable name (status file, launcher).
    pub fn key(self) -> &'static str {
        match self {
            AddrKind::Hamachi => "hamachi",
            AddrKind::Radmin => "radmin",
            AddrKind::Tailscale => "tailscale",
            AddrKind::ZeroTier => "zerotier",
            AddrKind::Vpn => "vpn",
            AddrKind::Lan => "lan",
            AddrKind::Public => "public",
            AddrKind::Virtual => "virtual",
            AddrKind::LinkLocal => "link-local",
        }
    }

    /// A VPN players set up to play together over the internet.
    pub fn is_vpn(self) -> bool {
        matches!(
            self,
            AddrKind::Hamachi
                | AddrKind::Radmin
                | AddrKind::Tailscale
                | AddrKind::ZeroTier
                | AddrKind::Vpn
        )
    }

    /// Whether another computer can reach this address at all.
    pub fn reachable(self) -> bool {
        !matches!(self, AddrKind::Virtual | AddrKind::LinkLocal)
    }

    /// Order in the code and in the lists: the gaming VPNs first (the reason they are
    /// installed at all is playing together), then the LAN, then the rest.
    fn rank(self) -> u8 {
        match self {
            AddrKind::Hamachi | AddrKind::Radmin | AddrKind::ZeroTier => 0,
            AddrKind::Tailscale => 1,
            AddrKind::Vpn => 2,
            AddrKind::Lan => 3,
            AddrKind::Public => 4,
            AddrKind::Virtual => 5,
            AddrKind::LinkLocal => 6,
        }
    }
}

/// An IPv4 address of this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAddr {
    pub ip: Ipv4Addr,
    /// The interface (adapter) it belongs to, as the system names it.
    pub interface: String,
    pub kind: AddrKind,
    /// The subnet's broadcast address, where the interface has one (discovery).
    pub broadcast: Option<Ipv4Addr>,
}

impl LocalAddr {
    pub fn label(&self) -> &'static str {
        self.kind.label()
    }
}

fn is_private(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168)
}

/// What an address is, from the address itself and the name of its interface.
pub fn classify(ip: Ipv4Addr, interface: &str) -> AddrKind {
    let [a, b, _, _] = ip.octets();
    let name = interface.to_ascii_lowercase();
    let has = |s: &str| name.contains(s);
    if ip.is_link_local() {
        return AddrKind::LinkLocal;
    }
    // the gaming VPNs by their address ranges (Hamachi's 25/8 and Radmin's 26/8 are
    // borrowed public blocks nobody else puts on a local interface) or by the adapter's name
    if a == 25 || has("hamachi") || name.starts_with("ham") {
        return AddrKind::Hamachi;
    }
    if a == 26 || has("radmin") {
        return AddrKind::Radmin;
    }
    if has("zerotier") || name.starts_with("zt") || name.starts_with("feth") {
        return AddrKind::ZeroTier;
    }
    if (a == 100 && (64..128).contains(&b)) || has("tailscale") {
        return AddrKind::Tailscale;
    }
    // bridges of virtual machines and containers (Docker, VirtualBox, VMware, Parallels,
    // Hyper-V/WSL, libvirt, macOS's own VM bridges)
    let virtual_names = [
        "docker", "br-", "veth", "vboxnet", "virtualbox", "vmnet", "vmware", "vethernet",
        "hyper-v", "virbr", "lxc", "lxd", "podman", "cni", "flannel", "bridge1", "vnic",
        "parallels", "wsl",
    ];
    if virtual_names.iter().any(|v| name.starts_with(v) || has(v)) {
        return AddrKind::Virtual;
    }
    let tunnel = ["utun", "tun", "tap", "wg", "ppp", "ipsec", "wireguard", "openvpn", "vpn"];
    if tunnel.iter().any(|t| name.starts_with(t) || has(t)) {
        return AddrKind::Vpn;
    }
    if is_private(ip) {
        AddrKind::Lan
    } else {
        AddrKind::Public
    }
}

static CACHE: Mutex<Option<(Instant, Vec<LocalAddr>)>> = Mutex::new(None);
/// How long a list of addresses is believed (s).
const CACHE_FOR: Duration = Duration::from_secs(10);

/// Every IPv4 address of an interface that is up, except the loopback: the best first (see
/// `AddrKind::rank`; among equals the one the system routes the internet through).
/// `OMSI_LAN_IP=a.b.c.d[,e.f.g.h]` puts those first (a machine the detection gets wrong).
pub fn local_addresses() -> Vec<LocalAddr> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, list)) = cache.as_ref() {
        if at.elapsed() < CACHE_FOR {
            return list.clone();
        }
    }
    let list = read_addresses();
    *cache = Some((Instant::now(), list.clone()));
    list
}

fn read_addresses() -> Vec<LocalAddr> {
    let mut list: Vec<LocalAddr> = system_addresses()
        .into_iter()
        .filter(|a| !a.ip.is_loopback() && !a.ip.is_unspecified())
        .collect();
    // the one the default route goes out of wins among addresses of the same kind (a laptop
    // on wifi with a dead ethernet port that still holds an address)
    let routed = default_route_ip();
    list.sort_by_key(|a| (a.kind.rank(), Some(a.ip) != routed, a.interface.clone()));
    list.dedup_by_key(|a| a.ip);
    if let Ok(v) = omsi_cfg::flags::OMSI_LAN_IP.live_var() {
        let forced: Vec<Ipv4Addr> = v
            .split([',', ' ', ';'])
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        for ip in forced.into_iter().rev() {
            list.retain(|a| a.ip != ip);
            list.insert(
                0,
                LocalAddr {
                    ip,
                    interface: "OMSI_LAN_IP".into(),
                    kind: classify(ip, ""),
                    broadcast: None,
                },
            );
        }
    }
    list
}

/// The address the system would send internet traffic from (a UDP "connect" sends nothing).
fn default_route_ip() -> Option<Ipv4Addr> {
    let s = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()? {
        std::net::SocketAddr::V4(a) if !a.ip().is_unspecified() => Some(*a.ip()),
        _ => None,
    }
}

#[cfg(unix)]
fn system_addresses() -> Vec<LocalAddr> {
    let mut out = Vec::new();
    // SAFETY: getifaddrs hands out a linked list we only read and give back with freeifaddrs
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return out;
        }
        let mut p = head;
        while !p.is_null() {
            let ifa = &*p;
            p = ifa.ifa_next;
            if ifa.ifa_addr.is_null() || (*ifa.ifa_addr).sa_family as i32 != libc::AF_INET {
                continue;
            }
            let flags = ifa.ifa_flags as i32;
            if flags & libc::IFF_UP == 0 || flags & libc::IFF_RUNNING == 0 {
                continue;
            }
            let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
            let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
            let name = std::ffi::CStr::from_ptr(ifa.ifa_name)
                .to_string_lossy()
                .into_owned();
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
            let bcast = ifa.ifa_dstaddr;
            #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd")))]
            let bcast = ifa.ifa_ifu;
            let broadcast = (flags & libc::IFF_BROADCAST != 0 && !bcast.is_null())
                .then(|| {
                    let b = &*(bcast as *const libc::sockaddr_in);
                    Ipv4Addr::from(u32::from_be(b.sin_addr.s_addr))
                })
                .filter(|b| !b.is_unspecified());
            out.push(LocalAddr {
                ip,
                kind: classify(ip, &name),
                interface: name,
                broadcast,
            });
        }
        libc::freeifaddrs(head);
    }
    out
}

#[cfg(windows)]
fn system_addresses() -> Vec<LocalAddr> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: no console flashes up over the game
    let out = std::process::Command::new("ipconfig")
        .creation_flags(0x0800_0000)
        .output();
    match out {
        Ok(o) => parse_ipconfig(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => Vec::new(),
    }
}

#[cfg(not(any(unix, windows)))]
fn system_addresses() -> Vec<LocalAddr> {
    Vec::new()
}

/// The adapters and their IPv4 addresses in the output of Windows' `ipconfig`: an adapter
/// starts at an unindented line ending in ':' ("Ethernet adapter Hamachi:", "Адаптер
/// Ethernet Hamachi:"), its addresses are the indented lines naming "IPv4" whose value is an
/// address ("IPv4 Address. . . : 25.1.2.3(Preferred)"). A disconnected adapter lists none.
pub fn parse_ipconfig(text: &str) -> Vec<LocalAddr> {
    let mut out = Vec::new();
    let mut adapter = String::new();
    let mut mask: Option<Ipv4Addr> = None;
    for line in text.lines() {
        let t = line.trim_end();
        if t.is_empty() {
            continue;
        }
        if !line.starts_with(char::is_whitespace) {
            adapter = t.trim_end_matches(':').trim().to_string();
            // "Ethernet adapter Hamachi" → "Hamachi" (the part after the adapter kind; the
            // whole line is kept for the classification)
            continue;
        }
        let value = t.rsplit(':').next().unwrap_or("").trim();
        let first = value
            .split(|c: char| !(c.is_ascii_digit() || c == '.'))
            .next()
            .unwrap_or("");
        let Ok(ip) = first.parse::<Ipv4Addr>() else {
            continue;
        };
        if t.contains("IPv4") {
            mask = None;
            out.push(LocalAddr {
                ip,
                kind: classify(ip, &adapter),
                interface: adapter.clone(),
                broadcast: None,
            });
        } else if out.last().map(|a| a.interface == adapter).unwrap_or(false)
            && mask.is_none()
            && ip.octets()[0] == 255
        {
            // the subnet mask follows the address: the broadcast from both
            mask = Some(ip);
            if let Some(a) = out.last_mut() {
                let b = u32::from(a.ip) | !u32::from(ip);
                a.broadcast = Some(Ipv4Addr::from(b));
            }
        }
    }
    out
}

pub fn is_local_peer(addr: std::net::SocketAddr) -> bool {
    let std::net::SocketAddr::V4(a) = addr else { return false };
    let ip = *a.ip();
    ip.is_loopback() || classify(ip, "") != AddrKind::Public || local_addresses().iter().any(|l| l.ip.octets()[..3] == ip.octets()[..3])
}

/// The addresses another player may join at, best first: those of `local_addresses` a
/// friend can reach (no link-local, no virtual machine bridges).
pub fn joinable_addresses() -> Vec<LocalAddr> {
    local_addresses()
        .into_iter()
        .filter(|a| a.kind.reachable())
        .collect()
}

/// A dedicated server's direct address (its web port reached without the tunnel), as it
/// finds it by itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DirectChoice {
    /// An internet address of this machine's own: told as it is.
    Own(Ipv4Addr),
    /// The one the internet sees behind a router: told if the server reaches itself there.
    Seen(Ipv4Addr),
}

impl DirectChoice {
    pub fn ip(self) -> Ipv4Addr {
        match self {
            DirectChoice::Own(ip) | DirectChoice::Seen(ip) => ip,
        }
    }
}

/// Which address to tell: this machine's own internet address, else the one the internet
/// sees (to be tried first), else none yet. An address in a carrier's NAT range
/// (100.64.0.0/10) is never one.
pub fn direct_choice(own: Option<Ipv4Addr>, seen: Option<Ipv4Addr>) -> Option<DirectChoice> {
    let cgnat = |ip: &Ipv4Addr| ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]);
    let own = own.filter(|ip| !cgnat(ip));
    own.map(DirectChoice::Own).or(seen.filter(|ip| !ip.is_private() && !ip.is_loopback() && !cgnat(ip)).map(DirectChoice::Seen))
}

/// The web port of a dedicated server whose direct address is found by itself (`auto_direct`).
static WEB_PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
/// Found, or being tried: not again.
static DIRECT_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A dedicated server finds its direct address by itself, on `web_port` (`public_address` left
/// empty): a game joining through the tunnel tries it first.
pub fn auto_direct(web_port: u16) {
    let _ = WEB_PORT.set(web_port);
}

/// Called as the status is told: once the address can be known, it is, once, and handed to
/// `tell`. A machine with an internet address of its own (on one of its adapters) is reached
/// there. One behind a router is reached at the address the internet sees (STUN) only if the
/// router forwards the web port - and never behind a carrier's NAT (CGNAT), where that address
/// is the carrier's. So that one is told only if the server reaches itself there; a router that
/// cannot loop back loses the shortcut, but nobody is sent to an address that does not answer.
pub fn find_direct(lan: &crate::LanSession, tell: fn(&str)) {
    use std::sync::atomic::Ordering;
    let Some(&port) = WEB_PORT.get() else { return };
    if DIRECT_STARTED.load(Ordering::Relaxed) {
        return;
    }
    let own = local_addresses().into_iter().find(|a| a.kind == AddrKind::Public).map(|a| a.ip);
    let seen = lan.public_addr().and_then(|a| match a.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        std::net::IpAddr::V6(_) => None,
    });
    let Some(choice) = direct_choice(own, seen) else { return };
    DIRECT_STARTED.store(true, Ordering::Relaxed);
    let addr = std::net::SocketAddr::from((choice.ip(), port));
    if let DirectChoice::Own(_) = choice {
        log::info!("server: players joining through the tunnel try this machine's address {addr} first (public_address = off: never)");
        tell(&addr.to_string());
        return;
    }
    std::thread::spawn(move || {
        if crate::ws::reachable(&format!("http://{addr}"), Duration::from_secs(3)) {
            log::info!("server: reached itself at {addr}: players joining through the tunnel try it first (public_address = off: never)");
            tell(&addr.to_string());
        } else {
            log::info!("server: not reached at {addr}, the address the internet sees (no forwarding of the web port, or a carrier's NAT): players join through the tunnel; public_address in server.cfg can name another");
        }
    });
}

#[cfg(test)]
mod direct_tests {
    use super::*;

    /// An internet address of the machine's own is told at once; behind a router the one the
    /// internet sees is only tried (the server must reach itself there); an address in a
    /// carrier's NAT range is none; nothing known yet, nothing told.
    #[test]
    fn the_machines_own_address_first_the_seen_one_only_tried() {
        let own = Ipv4Addr::new(203, 0, 113, 7);
        let seen = Ipv4Addr::new(198, 51, 100, 9);
        assert_eq!(direct_choice(Some(own), Some(seen)), Some(DirectChoice::Own(own)));
        assert_eq!(direct_choice(None, Some(seen)), Some(DirectChoice::Seen(seen)));
        assert_eq!(direct_choice(Some(Ipv4Addr::new(100, 72, 1, 2)), Some(seen)), Some(DirectChoice::Seen(seen)), "a CGNAT adapter is no internet address");
        assert_eq!(direct_choice(None, Some(Ipv4Addr::new(100, 72, 1, 2))), None);
        assert_eq!(direct_choice(None, Some(Ipv4Addr::new(192, 168, 1, 5))), None);
        assert_eq!(direct_choice(None, None), None);
    }
}
