//! What a player typed to join: a session code, an address, a port, a computer's name.

use super::*;

/// Where a joining player wants to go.
#[derive(Debug, Clone, PartialEq)]
pub enum JoinTarget {
    /// Look for a host on the local network.
    Discover,
    /// These addresses (tried all at once, the first that answers is taken); `session` is
    /// set when the player gave a session code.
    Direct {
        addrs: Vec<SocketAddr>,
        session: Option<u64>,
        protocol: Option<u8>,
    },
}

/// What the text of a join field means, without looking any names up (the launcher checks
/// the field as it is typed): a description, or what is wrong with it.
pub fn describe_join(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty()
        || t.eq_ignore_ascii_case("auto")
        || t.eq_ignore_ascii_case("discover")
        || t.eq_ignore_ascii_case("search")
    {
        return Ok("search the local network for a hosted session".into());
    }
    if let Some(url) = ws::ws_url(t) {
        return Ok(format!("a server on the internet, reached over a WebSocket ({url})"));
    }
    if looks_like_code(t) {
        let c = SessionCode::decode(t)?;
        if c.protocol as u32 != PROTOCOL {
            return Err(format!("this code was made by a game with LAN protocol {}, this one speaks {PROTOCOL} - both players need the same version", c.protocol));
        }
        let at: Vec<String> = c
            .ips
            .iter()
            .map(|ip| format!("{ip}:{} ({})", c.port, addrs::classify(*ip, "").label()))
            .collect();
        return Ok(format!(
            "session {} hosted at {}",
            session_hex(c.session),
            at.join(" or ")
        ));
    }
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return match t.parse::<u32>() {
            Ok(p) if (1..=65535).contains(&p) => {
                Ok(format!("a session hosted on this computer, port {p}"))
            }
            _ => Err(format!("the number {t} is not a port (1 to 65535)")),
        };
    }
    if let Ok(a) = t.parse::<SocketAddr>() {
        return Ok(format!("the host at {a}"));
    }
    if let Ok(ip) = t.parse::<std::net::IpAddr>() {
        return Ok(format!("the host at {ip}, port {DEFAULT_PORT}"));
    }
    let (host, port) = match t.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(p) if p > 0 => (h, p),
            _ => {
                return Err(format!(
                    "the part after ':' must be a port (1 to 65535), not '{p}'"
                ))
            }
        },
        None => (t, DEFAULT_PORT),
    };
    let name_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && host.bytes().any(|b| b.is_ascii_alphabetic());
    if name_ok
        && (host.contains('.')
            || host.eq_ignore_ascii_case("localhost")
            || !host
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
    {
        return Ok(format!("the computer named {host}, port {port}"));
    }
    Err(format!("the entry '{t}' is neither a session code (OMSI-XXXX-…), an address (192.168.1.20 or 192.168.1.20:27015), a port, nor empty (search)"))
}

/// Read what a player typed into the join field: a session code, `ip`, `ip:port`,
/// `host:port`, a bare port (a host on this machine), or nothing / `auto` (search).
pub fn parse_join(text: &str) -> Result<JoinTarget, String> {
    let t = text.trim();
    if t.is_empty()
        || t.eq_ignore_ascii_case("auto")
        || t.eq_ignore_ascii_case("discover")
        || t.eq_ignore_ascii_case("search")
    {
        return Ok(JoinTarget::Discover);
    }
    if looks_like_code(t) {
        let c = SessionCode::decode(t)?;
        return Ok(JoinTarget::Direct {
            addrs: c.addrs(),
            session: Some(c.session),
            protocol: Some(c.protocol),
        });
    }
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return match t.parse::<u32>() {
            Ok(p) if (1..=65535).contains(&p) => Ok(JoinTarget::Direct {
                addrs: vec![SocketAddr::from((Ipv4Addr::LOCALHOST, p as u16))],
                session: None,
                protocol: None,
            }),
            _ => Err(format!("the number {t} is not a port (1 to 65535)")),
        };
    }
    if let Ok(a) = t.parse::<SocketAddr>() {
        return Ok(JoinTarget::Direct {
            addrs: vec![a],
            session: None,
            protocol: None,
        });
    }
    if let Ok(ip) = t.parse::<std::net::IpAddr>() {
        return Ok(JoinTarget::Direct {
            addrs: vec![SocketAddr::new(ip, DEFAULT_PORT)],
            session: None,
            protocol: None,
        });
    }
    // a host name, with or without a port
    let (host, port) = match t.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(p) if p > 0 => (h, p),
            _ => {
                return Err(format!(
                    "the entry '{t}': the part after ':' must be a port (1 to 65535)"
                ))
            }
        },
        None => (t, DEFAULT_PORT),
    };
    // (a name of digits only would be read as a numeric address by the resolver: "27015:27015")
    if host.is_empty()
        || host.contains(char::is_whitespace)
        || host.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return Err(format!("the entry '{t}' is neither a session code (OMSI-…), an address (192.168.1.20 or 192.168.1.20:27015) nor a port"));
    }
    match (host, port).to_socket_addrs() {
        Ok(mut it) => match it.find(|a| a.is_ipv4()) {
            Some(a) => Ok(JoinTarget::Direct { addrs: vec![a], session: None, protocol: None }),
            None => Err(format!("the computer {host} has no IPv4 address")),
        },
        Err(e) => Err(format!("the entry '{t}' is neither a session code (OMSI-…), an address nor a port, and no computer of that name was found ({e})")),
    }
}
