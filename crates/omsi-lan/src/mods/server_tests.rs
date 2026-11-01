use super::*;

fn hosting() -> (Option<Server>, SocketAddr) {
    hosting_with_accept(TcpListener::accept)
}

fn hosting_with_accept(
    accept: impl FnMut(&TcpListener) -> std::io::Result<(TcpStream, SocketAddr)> + Send + 'static,
) -> (Option<Server>, SocketAddr) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let key = Arc::new(omsi_net::hostkey::HostKey::generate().unwrap());
    let source = ModSource {
        root: std::env::temp_dir().join("openomsi-no-test-content"),
        map: String::new(),
        bus: None,
        weather: None,
    };
    (serve_with_accept(listener, key, &source, accept), address)
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

#[test]
fn accept_errors_back_off_and_keep_the_room_server_alive() {
    use std::io::{Error, ErrorKind};

    let mut errors = [
        Error::from(ErrorKind::ConnectionAborted),
        Error::from(ErrorKind::ConnectionReset),
        Error::from(ErrorKind::Interrupted),
        Error::other("temporary file handle exhaustion"),
    ]
    .into_iter();
    let mut failed_at: Option<Instant> = None;
    let (server, address) = hosting_with_accept(move |listener| {
        if let Some(at) = failed_at.take() {
            assert!(
                at.elapsed() >= Duration::from_millis(20),
                "each failed accept must back off before retrying"
            );
        }
        match errors.next() {
            Some(error) => {
                failed_at = Some(Instant::now());
                Err(error)
            }
            None => listener.accept(),
        }
    });
    let server = server.unwrap();
    let _client = TcpStream::connect(address).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.state.connections.lock().unwrap().is_empty() {
        assert!(
            !server.worker.as_ref().unwrap().is_finished(),
            "the room server stopped after a transient accept error"
        );
        assert!(
            Instant::now() < deadline,
            "the room server did not accept a new client"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
