//! Reserve the host's state and content transports before announcing the session.

use super::{
    LanSession, PORT_RANGE, PROTOCOL, Role, WorldInfo, bridge, hostkey, local_addrs,
};
use std::io::{self, Read};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

#[cfg(windows)]
mod windows;

pub(super) fn bind(port: u16) -> io::Result<(UdpSocket, Option<TcpListener>)> {
    if omsi_cfg::flags::OMSI_NO_LAN_MODS.is_set() {
        return Ok((udp_socket(port)?, None));
    }
    // Let TCP choose an ephemeral port first; Windows excludes different ranges
    // for each transport. The same chosen port must still be reserved for UDP.
    let listener = tcp_listener(port)
        .map_err(|e| io::Error::new(e.kind(), format!("TCP port {port}: {e}")))?;
    let port = listener.local_addr()?.port();
    let udp =
        udp_socket(port).map_err(|e| io::Error::new(e.kind(), format!("UDP port {port}: {e}")))?;
    verify_loopback(&listener).map_err(probe_error)?;
    Ok((udp, Some(listener)))
}

#[cfg(not(windows))]
fn udp_socket(port: u16) -> io::Result<UdpSocket> {
    UdpSocket::bind(("0.0.0.0", port))
}

#[cfg(windows)]
fn udp_socket(port: u16) -> io::Result<UdpSocket> {
    windows::udp_socket(port)
}

#[cfg(not(windows))]
fn tcp_listener(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(("0.0.0.0", port))
}

#[cfg(windows)]
fn tcp_listener(port: u16) -> io::Result<TcpListener> {
    windows::tcp_listener(port)
}

/// A wildcard bind alone can succeed while a more specific listener receives loopback.
/// Check the gateway's actual destination, without sending a protocol to another service.
fn verify_loopback(listener: &TcpListener) -> io::Result<()> {
    let port = listener.local_addr()?.port();
    let target = SocketAddr::from(([127, 0, 0, 1], port));
    let probe = TcpStream::connect_timeout(&target, Duration::from_millis(250))?;
    let source = probe.local_addr()?;
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_millis(250);
    loop {
        match listener.accept() {
            Ok((mut stream, peer)) if peer == source => {
                // Close the probe first so the host port does not enter TIME_WAIT.
                probe.shutdown(Shutdown::Write)?;
                stream.set_read_timeout(Some(Duration::from_millis(250)))?;
                stream.read(&mut [0])?;
                listener.set_nonblocking(false)?;
                return Ok(());
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("TCP {target} does not reach this session's listener"),
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

// A refused connection or failed handshake means this candidate did not reach
// our listener. Retry another port, but do not hide socket resource exhaustion.
fn probe_error(error: io::Error) -> io::Error {
    match error.kind() {
        io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::NotConnected
        | io::ErrorKind::TimedOut
        | io::ErrorKind::WouldBlock
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::BrokenPipe => io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("TCP loopback probe failed: {error}"),
        ),
        _ => error,
    }
}

// TCP and UDP have separate exclusions. Repeated bind(0) can keep choosing
// adjacent TCP ports inside one UDP exclusion, so spread retries across the
// dynamic range instead of exhausting the retry budget in the same block.
fn candidate(port: u16, offset: u16, seed: u16) -> Option<u16> {
    if port != 0 {
        return port.checked_add(offset);
    }
    if offset == 0 {
        return Some(0);
    }
    let stride = (16384 / PORT_RANGE) | 1;
    Some(49152 + seed.wrapping_add(offset.wrapping_mul(stride)) % 16384)
}

impl LanSession {
    /// Host on a port reserved for both UDP state and TCP content transfers.
    /// With `try_next`, a bounded range is tried when either transport is occupied.
    pub fn host(
        port: u16,
        name: &str,
        world: WorldInfo,
        try_next: bool,
    ) -> std::io::Result<LanSession> {
        let mut last_err = None;
        let tries = if try_next || port == 0 { PORT_RANGE } else { 1 };
        let seed = super::random_session_id() as u16;
        for offset in 0..tries {
            let Some(candidate) = candidate(port, offset, seed) else {
                break;
            };
            match bind(candidate) {
                Ok((socket, listener)) => {
                    let p = socket.local_addr()?.port();
                    socket.set_nonblocking(true)?;
                    socket.set_broadcast(true)?;
                    let mut s = LanSession::new(socket, Role::Host, name, world);
                    s.host_mods = listener;
                    let key = std::sync::Arc::new(
                        hostkey::HostKey::generate()
                            .ok_or_else(|| std::io::Error::other("no key for the session"))?,
                    );
                    s.session = key.session();
                    s.bridge = bridge::Bridge::start(
                        true,
                        s.session,
                        Some(key.clone()),
                        local_addrs(p),
                        p,
                    );
                    s.host_key = Some(key);
                    // Logs are shared in support bundles: keep join codes and session
                    // identity in the multiplayer UI, not in persistent diagnostics.
                    log::info!("LAN: hosting on UDP/TCP port {p} (protocol {PROTOCOL})");
                    if port != 0 && p != port {
                        log::info!("LAN: UDP/TCP port {port} is unavailable; using {p}");
                    }
                    return Ok(s);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    log::info!("LAN: cannot reserve host port {candidate}: {e}");
                    last_err = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.unwrap_or_else(|| std::io::Error::other("no host port available")))
    }

    /// Transfer the reserved TCP listener without releasing its port between services.
    pub fn take_mods_listener(&mut self) -> Option<std::net::TcpListener> {
        self.host_mods.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_connection_failures_allow_the_next_candidate() {
        for kind in [
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::NotConnected,
            io::ErrorKind::TimedOut,
            io::ErrorKind::WouldBlock,
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::BrokenPipe,
        ] {
            assert_eq!(
                probe_error(io::Error::from(kind)).kind(),
                io::ErrorKind::AddrInUse
            );
        }
        for kind in [io::ErrorKind::OutOfMemory, io::ErrorKind::Other] {
            assert_eq!(probe_error(io::Error::from(kind)).kind(), kind);
        }
    }

    #[test]
    fn ephemeral_retries_spread_beyond_one_exclusion_block() {
        for seed in [0, 55230, u16::MAX] {
            assert_eq!(candidate(0, 0, seed), Some(0));
            let ports: Vec<_> = (1..PORT_RANGE)
                .map(|offset| candidate(0, offset, seed).unwrap())
                .collect();
            assert!(ports.iter().all(|&port| port >= 49152));
            assert!(ports.windows(2).all(|p| p[0].abs_diff(p[1]) > 1000));
            let unique: std::collections::BTreeSet<_> = ports.iter().collect();
            assert_eq!(unique.len(), ports.len());
        }
        assert_eq!(candidate(27015, 1, 0), Some(27016));
        assert_eq!(candidate(u16::MAX, 0, 0), Some(u16::MAX));
        assert_eq!(candidate(u16::MAX, 1, 0), None);
    }
}
