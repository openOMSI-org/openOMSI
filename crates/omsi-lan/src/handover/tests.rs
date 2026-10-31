use super::*;

#[test]
fn empty_or_multiline_target_is_rejected_before_connecting() {
    for target in ["", "   ", "localhost\nother", "\u{0}"] {
        assert!(
            PreparedJoin::connect(target, "player", WorldInfo::default(), &Pose::default())
                .is_err()
        );
    }
}

#[test]
fn a_failed_candidate_keeps_the_active_room_connected() {
    let world = WorldInfo::default();
    let mut host = LanSession::host(0, "host", world.clone(), false).unwrap();
    let addr = format!("127.0.0.1:{}", host.local_addr().unwrap().port());
    let (stop, done) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        while done.try_recv().is_err() {
            host.tick(0.01, &Pose::default());
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let mut active = PreparedJoin::connect_with_wait(
        &addr,
        "active",
        world.clone(),
        &Pose::default(),
        Duration::from_secs(2),
    )
    .unwrap();
    let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let failed = PreparedJoin::connect_with_wait(
        &silent.local_addr().unwrap().to_string(),
        "candidate",
        world,
        &Pose::default(),
        Duration::from_millis(80),
    );
    assert!(failed.is_err());
    active.session.tick(0.02, &Pose::default());
    assert!(active.session.connected);
    assert!(active.session.welcome.is_some());
    drop(active);
    stop.send(()).unwrap();
    worker.join().unwrap();
}

struct RunningHost {
    address: String,
    session: u64,
    peers: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl RunningHost {
    fn start(map: &str) -> Self {
        let mut host = LanSession::host(
            0,
            "host",
            WorldInfo {
                map: map.into(),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let address = format!("127.0.0.1:{}", host.local_addr().unwrap().port());
        let session = host.session;
        let (stop, done) = std::sync::mpsc::channel();
        let peers = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = peers.clone();
        let worker = std::thread::spawn(move || {
            while matches!(done.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)) {
                host.tick(0.01, &Pose::default());
                observed.store(host.peer_count(), std::sync::atomic::Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        Self {
            address,
            session,
            peers,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for RunningHost {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn a_candidate_can_join_a_different_room_without_retargeting_the_old_session() {
    let first = RunningHost::start("maps/First/global.cfg");
    let second = RunningHost::start("maps/Second/global.cfg");
    let current = PreparedJoin::connect(
        &first.address,
        "player",
        WorldInfo::default(),
        &Pose::default(),
    )
    .unwrap();
    let original_host = current.session.host;
    let original_session = current.session.session;
    let candidate = PreparedJoin::connect(
        &second.address,
        "player",
        WorldInfo::default(),
        &Pose::default(),
    )
    .unwrap();
    assert_ne!(candidate.session.session, original_session);
    assert_eq!(
        candidate.session.welcome.as_ref().unwrap().world.map,
        "maps/Second/global.cfg"
    );
    assert_eq!(current.session.host, original_host);
    assert_eq!(current.session.session, original_session);
    assert!(current.session.connected);
}

#[test]
fn a_staged_websocket_does_not_replace_the_active_rooms_gateway() {
    let host = RunningHost::start("maps/Shared/global.cfg");
    let gateway = omsi_net::ws::WsGateway::start(
        "127.0.0.1:0".parse().unwrap(),
        host.address.parse().unwrap(),
        Default::default(),
    )
    .unwrap();
    let url = format!("http://{}", gateway.addr);
    let active = PreparedJoin::connect(&url, "active", WorldInfo::default(), &Pose::default())
        .unwrap()
        .accept();
    let original_url = crate::gateway::joined_url();
    assert!(original_url.is_some());
    let candidate =
        PreparedJoin::connect(&url, "candidate", WorldInfo::default(), &Pose::default()).unwrap();
    assert_eq!(crate::gateway::joined_url(), original_url);
    drop(candidate);
    assert_eq!(crate::gateway::joined_url(), original_url);
    assert!(active.connected);
    drop(active);
    crate::gateway::close_public_gateway();
}

#[test]
fn hosted_room_code_is_rejected_before_sending_a_handshake() {
    let mut host = LanSession::host(0, "host", WorldInfo::default(), false).unwrap();
    let code = host.code().unwrap().encode();
    let sent = host.sent();
    let result = PreparedJoin::connect_for_handover(
        &format!("  {code}  "),
        "host",
        WorldInfo::default(),
        &Pose::default(),
        Some(host.session),
    );
    assert!(
        matches!(result, Err(ref error) if error == "You cannot join the room you are hosting")
    );
    host.tick(0.02, &Pose::default());
    assert_eq!(host.sent(), sent);
    assert_eq!(host.peer_count(), 0);
    assert_eq!(host.role, omsi_net::Role::Host);
}

#[test]
fn hosted_room_is_rejected_after_address_resolution() {
    let host = RunningHost::start("maps/Shared/global.cfg");
    let result = PreparedJoin::connect_for_handover(
        &host.address,
        "host",
        WorldInfo::default(),
        &Pose::default(),
        Some(host.session),
    );
    assert!(
        matches!(result, Err(ref error) if error == "You cannot join the room you are hosting")
    );
    let other_player = PreparedJoin::connect(
        &host.address,
        "other",
        WorldInfo::default(),
        &Pose::default(),
    )
    .unwrap();
    assert_eq!(other_player.session.session, host.session);
    assert!(other_player.session.connected);
}

#[test]
fn hosted_room_is_rejected_through_a_websocket_gateway() {
    let host = RunningHost::start("maps/Shared/global.cfg");
    let gateway = omsi_net::ws::WsGateway::start(
        "127.0.0.1:0".parse().unwrap(),
        host.address.parse().unwrap(),
        Default::default(),
    )
    .unwrap();
    let result = PreparedJoin::connect_for_handover(
        &format!("http://{}", gateway.addr),
        "host",
        WorldInfo::default(),
        &Pose::default(),
        Some(host.session),
    );
    assert!(
        matches!(result, Err(ref error) if error == "You cannot join the room you are hosting")
    );
}

#[test]
fn a_host_can_still_prepare_a_different_room() {
    let current = RunningHost::start("maps/Current/global.cfg");
    let other = RunningHost::start("maps/Other/global.cfg");
    let candidate = PreparedJoin::connect_for_handover(
        &other.address,
        "host",
        WorldInfo::default(),
        &Pose::default(),
        Some(current.session),
    )
    .unwrap();
    assert_eq!(candidate.session.session, other.session);
}

#[test]
fn a_client_can_prepare_its_current_room_without_a_host_exclusion() {
    let host = RunningHost::start("maps/Shared/global.cfg");
    let candidate = PreparedJoin::connect_for_handover(
        &host.address,
        "client",
        WorldInfo::default(),
        &Pose::default(),
        None,
    )
    .unwrap();
    assert_eq!(candidate.session.session, host.session);
}

#[test]
fn dropping_a_websocket_candidate_sends_bye_before_closing_the_bridge() {
    let host = RunningHost::start("maps/Shared/global.cfg");
    let gateway = omsi_net::ws::WsGateway::start(
        "127.0.0.1:0".parse().unwrap(),
        host.address.parse().unwrap(),
        Default::default(),
    )
    .unwrap();
    let candidate = PreparedJoin::connect(
        &format!("http://{}", gateway.addr),
        "player",
        WorldInfo::default(),
        &Pose::default(),
    )
    .unwrap();
    assert_eq!(host.peers.load(std::sync::atomic::Ordering::Relaxed), 1);
    drop(candidate);
    let until = Instant::now() + Duration::from_secs(2);
    while host.peers.load(std::sync::atomic::Ordering::Relaxed) != 0 {
        assert!(
            Instant::now() < until,
            "departed bus remained until the host timeout"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn candidate_vehicle_offers_do_not_replace_the_current_room_offers() {
    let host = RunningHost::start("maps/Shared/global.cfg");
    let gateway = omsi_net::ws::WsGateway::start(
        "127.0.0.1:0".parse().unwrap(),
        host.address.parse().unwrap(),
        omsi_net::ws::ServerInfo {
            vehicles: vec!["Vehicles/Allowed/test.bus".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let current = crate::gateway::server_offers();
    let mut candidate = PreparedJoin::connect_for_handover(
        &format!("http://{}", gateway.addr),
        "player",
        WorldInfo::default(),
        &Pose {
            bus: "Vehicles/Other/test.bus".into(),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while candidate.vehicle_allowed.is_none() && Instant::now() < until {
        candidate.poll_vehicle_offers();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(candidate.vehicle_allowed, Some(false));
    assert_eq!(crate::gateway::server_offers(), current);
}
