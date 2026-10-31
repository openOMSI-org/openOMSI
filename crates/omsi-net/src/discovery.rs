//! Discovery covers the same bounded port range that hosts reserve.

use super::{PORT_RANGE, addrs::LocalAddr};
use std::net::{Ipv4Addr, SocketAddr};

pub(super) fn targets(
    port: u16,
    addresses: impl IntoIterator<Item = LocalAddr>,
) -> Vec<SocketAddr> {
    // Include each interface's subnet broadcast, including VPNs and platforms
    // that do not route the limited broadcast to every interface.
    let mut ips = vec![Ipv4Addr::BROADCAST, Ipv4Addr::LOCALHOST];
    for address in addresses {
        let broadcast = address.broadcast.unwrap_or_else(|| {
            let o = address.ip.octets();
            Ipv4Addr::new(o[0], o[1], o[2], 255)
        });
        if !ips.contains(&broadcast) {
            ips.push(broadcast);
        }
    }
    ips.into_iter()
        .flat_map(|ip| {
            (0..PORT_RANGE).filter_map(move |offset| {
                port.checked_add(offset)
                    .map(|port| SocketAddr::from((ip, port)))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_PORT, addrs::AddrKind};

    #[test]
    fn every_discovery_address_covers_the_host_range() {
        let address = LocalAddr {
            ip: Ipv4Addr::new(192, 168, 1, 20),
            interface: "test".into(),
            kind: AddrKind::Lan,
            broadcast: Some(Ipv4Addr::new(192, 168, 3, 255)),
        };
        let fallback = LocalAddr {
            broadcast: None,
            ..address.clone()
        };
        let targets = targets(DEFAULT_PORT, [address.clone(), address, fallback]);
        assert_eq!(targets.len(), 4 * usize::from(PORT_RANGE));
        for ip in [
            Ipv4Addr::BROADCAST,
            Ipv4Addr::LOCALHOST,
            Ipv4Addr::new(192, 168, 3, 255),
            Ipv4Addr::new(192, 168, 1, 255),
        ] {
            for port in DEFAULT_PORT..DEFAULT_PORT + PORT_RANGE {
                assert!(
                    targets.contains(&SocketAddr::from((ip, port))),
                    "{ip}:{port}"
                );
            }
        }
    }

    #[test]
    fn discovery_includes_the_highest_port_without_wrapping() {
        let targets = targets(u16::MAX, []);
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|target| target.port() == u16::MAX));
    }
}
