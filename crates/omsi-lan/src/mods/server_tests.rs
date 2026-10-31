use super::*;

fn hosting() -> (Option<Server>, SocketAddr) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let key = Arc::new(omsi_net::hostkey::HostKey::generate().unwrap());
    let source = ModSource {
        root: std::env::temp_dir().join("openomsi-no-test-content"),
        map: String::new(),
        bus: None,
        weather: None,
    };
    (serve(listener, key, &source), address)
}

#[test]
fn dropping_room_server_releases_its_listening_port() {
    let (server, address) = hosting();
    drop(server);
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if TcpListener::bind(address).is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the old room still serves its TCP port"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn dropping_room_server_disconnects_existing_file_clients() {
    let (server, address) = hosting();
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while server
        .as_ref()
        .unwrap()
        .state
        .connections
        .lock()
        .unwrap()
        .is_empty()
    {
        assert!(Instant::now() < deadline, "file client was not accepted");
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(server);
    match client.read(&mut [0]) {
        Ok(0) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        result => panic!("the old room kept a file connection alive: {result:?}"),
    }
}
