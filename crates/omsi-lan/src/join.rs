//! Initial join and the Auto selector's already-discovered WebSocket fallback.

use omsi_net::{LanSession, Pose, WorldInfo};
use std::time::{Duration, Instant};

fn wait_for_welcome(session: &mut LanSession, planned: &Pose, wait: Duration) {
    let started = Instant::now();
    while started.elapsed() < wait && !session.connected && session.rejected.is_none() {
        session.tick(0.02, planned);
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// An explicit UDP selection passes no fallback. Never switch transport after a
/// welcome or a server rejection, and never rediscover a gateway during this join.
pub(crate) fn connect(
    target: &str,
    direct: &str,
    fallback: Option<&str>,
    name: &str,
    world: WorldInfo,
    planned: &Pose,
    wait: Duration,
) -> Result<LanSession, String> {
    let initial =
        LanSession::join(direct, name, world.clone(), Duration::from_secs(3)).map(|mut session| {
            wait_for_welcome(&mut session, planned, wait);
            session
        });
    let needs_fallback = match &initial {
        Ok(session) => {
            !session.connected && session.welcome.is_none() && session.turned_away.is_none()
        }
        Err(_) => true,
    };
    let Some(gateway) =
        fallback.filter(|_| needs_fallback && omsi_net::ws::ws_url(target).is_none())
    else {
        return initial;
    };
    let Some(url) = omsi_net::ws::ws_url(gateway) else {
        log::warn!("LAN: invalid WebSocket fallback gateway '{gateway}'");
        return initial;
    };
    log::info!("LAN: UDP did not answer; trying the discovered gateway {gateway}");
    match crate::gateway::ws_join_target(&url)
        .and_then(|local| LanSession::join(&local, name, world, Duration::from_secs(3)))
    {
        Ok(mut session) => {
            wait_for_welcome(&mut session, planned, wait);
            Ok(session)
        }
        Err(error) => {
            log::warn!("LAN: the discovered gateway {gateway} did not answer: {error}");
            initial
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
    use std::sync::atomic::Ordering;

    #[test]
    fn only_an_unanswered_auto_join_uses_the_discovered_gateway() {
        let world = WorldInfo::default();
        let planned = Pose::default();
        let mut host = LanSession::host(0, "host", world.clone(), false).unwrap();
        let at = SocketAddr::from((Ipv4Addr::LOCALHOST, host.local_addr().unwrap().port()));
        let gateway =
            omsi_net::ws::WsGateway::start("127.0.0.1:0".parse().unwrap(), at, Default::default())
                .unwrap();
        let fallback = format!("http://{}", gateway.addr);
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let worker = std::thread::spawn(move || {
            while matches!(
                stopped.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ) {
                host.tick(0.01, &Pose::default());
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let silent_at = silent.local_addr().unwrap().to_string();

        // Explicit UDP has no fallback even though a working gateway exists.
        let forced = connect(
            &silent_at,
            &silent_at,
            None,
            "forced",
            world.clone(),
            &planned,
            Duration::from_millis(100),
        )
        .unwrap();
        assert!(!forced.connected);
        assert_eq!(gateway.connected.load(Ordering::Relaxed), 0);

        // A web target has already become a local UDP bridge. Even if it is
        // unanswered and a fallback was supplied, never switch gateways.
        let web = connect(
            &fallback,
            &silent_at,
            Some(&fallback),
            "web",
            world.clone(),
            &planned,
            Duration::from_millis(100),
        )
        .unwrap();
        assert!(!web.connected);
        assert_eq!(gateway.connected.load(Ordering::Relaxed), 0);

        // A successful UDP join does not open a WebSocket.
        let direct = connect(
            &at.to_string(),
            &at.to_string(),
            Some(&fallback),
            "direct",
            world.clone(),
            &planned,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(direct.connected);
        assert_eq!(gateway.connected.load(Ordering::Relaxed), 0);

        // An explicit refusal is final, even when WS could reach another server.
        let refusing = UdpSocket::bind("127.0.0.1:0").unwrap();
        refusing
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let refusing_at = refusing.local_addr().unwrap().to_string();
        let refusal = std::thread::spawn(move || {
            let (_, from) = refusing.recv_from(&mut [0u8; 4096]).unwrap();
            refusing
                .send_to(
                    format!("REJECT|{}|server full", omsi_net::PROTOCOL).as_bytes(),
                    from,
                )
                .unwrap();
        });
        let rejected = connect(
            &refusing_at,
            &refusing_at,
            Some(&fallback),
            "rejected",
            world.clone(),
            &planned,
            Duration::from_secs(2),
        )
        .unwrap();
        refusal.join().unwrap();
        assert_eq!(rejected.turned_away.as_deref(), Some("server full"));
        assert_eq!(gateway.connected.load(Ordering::Relaxed), 0);

        // Use the exact gateway on its arbitrary port, rather than deriving a
        // gateway address from the silent UDP endpoint.
        let fallback_join = connect(
            &silent_at,
            &silent_at,
            Some(&fallback),
            "auto",
            world,
            &planned,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(fallback_join.connected);
        assert_eq!(gateway.connected.load(Ordering::Relaxed), 1);
        drop(fallback_join);
        crate::gateway::close_public_gateway();
        drop(stop);
        worker.join().unwrap();
    }
}
