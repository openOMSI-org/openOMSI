//! Exercise the actual renderer-free executable with two independent LAN
//! clients over UDP. Unlike in-process HostTransport tests, this catches
//! bootstrap, socket lifetime and cross-process protocol regressions.
//! Synthetic fixture only: it does not prove bus-script/gameplay parity.

use omsi_net::{LanSession, Pose, WorldInfo};
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("file parent")).unwrap();
    fs::write(path, content).unwrap();
}

fn unused_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct FixtureProcess {
    root: PathBuf,
    process: Child,
}

impl Drop for FixtureProcess {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn start_fixture(max_players: usize) -> (FixtureProcess, SocketAddr) {
    start_fixture_mode(max_players, false)
}

fn start_fixture_mode(max_players: usize, simulation: bool) -> (FixtureProcess, SocketAddr) {
    start_fixture_with_chrono(max_players, simulation, false)
}

fn start_fixture_with_chrono(
    max_players: usize,
    simulation: bool,
    chrono: bool,
) -> (FixtureProcess, SocketAddr) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openomsi-native-udp-{}-{nanos}",
        std::process::id()
    ));
    let map = root.join("Maps/Fixture");
    write(
        &map.join("global.cfg"),
        "[name]\nHeadless Fixture\n[map]\n0\n0\ntile_0_0.map\n",
    );
    write(
        &map.join("tile_0_0.map"),
        "[version]\n10\n[spline]\n0\nSplines/smoke.sli\n101\n-1\n10\n0\n10\n0\n40\n0\n0\n0\n0\n0\n0\n",
    );
    write(
        &root.join("Splines/smoke.sli"),
        "[length]\n40\n[texture]\nroad.bmp\n[profile]\n0\n[profilepnt]\n-1.5\n0\n0\n1\n[profilepnt]\n1.5\n0\n1\n1\n[path]\n0\n0\n0\n3\n0\n",
    );
    if chrono {
        write(
            &map.join("Chrono/NextDay/Chrono.cfg"),
            "[startdate]\n19890531\n",
        );
    }
    let port = unused_port();
    let mut web_port = unused_port();
    while web_port == port {
        web_port = unused_port();
    }
    let cfg = root.join("server-test.cfg");
    let passengers = u8::from(simulation);
    let time = if chrono { "23:59" } else { "09:00" };
    let time_speed = if chrono { 12 } else { 1 };
    write(
        &cfg,
        &format!(
            "name = Native UDP Test\nmap = Maps/Fixture/global.cfg\ndate = 1989-05-30\ntime = {time}\ntime_speed = {time_speed}\ntraffic = 0\ntimetable = 0\npassengers = {passengers}\nport = {port}\nweb_port = {web_port}\nmax_players = {max_players}\ntunnel = 0\npublic_server = 0\npublic_address = off\nreal_time = 0\nmetar_sync = 0\naccept_uploads = 0\nshare_maps = 0\nparked_buses = 0\nvoice = 0\nadmin_web = off\n"
        ),
    );
    let process = Command::new(env!("CARGO_BIN_EXE_omsi-server"))
        .arg(&root)
        .arg(&cfg)
        .arg(if simulation {
            "--experimental-simulation"
        } else {
            "--experimental-free-roam"
        })
        .arg("--ticks")
        .arg("1800")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(
            fs::File::create(root.join("headless.log")).expect("fixture log"),
        ))
        .spawn()
        .expect("start actual native headless server");
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    (FixtureProcess { root, process }, addr)
}

fn driver(name: &str, x: f64) -> Pose {
    Pose {
        name: name.into(),
        bus: format!("Vehicles/{name}.bus"),
        flags: omsi_net::FLAG_VEHICLE,
        x,
        y: 10.0,
        ..Default::default()
    }
}

#[test]
fn chrono_midnight_rebuild_keeps_two_connected_clients_and_manifest() {
    let (mut server, addr) = start_fixture_with_chrono(2, true, true);
    let world = WorldInfo {
        map: "Maps/Fixture/global.cfg".into(),
        date: "1989-05-30".into(),
        time: 23.0 * 3600.0 + 59.0 * 60.0,
        ..Default::default()
    };
    let mut alpha = LanSession::join_addr(vec![addr], None, "alpha", world.clone()).unwrap();
    let mut beta = LanSession::join_addr(vec![addr], None, "beta", world.clone()).unwrap();
    let pa = driver("alpha", 101.0);
    let pb = driver("beta", 202.0);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut joined_before = false;
    let mut migrated = false;
    let mut relayed_after = false;

    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("native host exited during Chrono midnight: {status}");
        }
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if alpha.connected && beta.connected && !migrated {
            joined_before = true;
        }
        let log = fs::read_to_string(server.root.join("headless.log")).unwrap_or_default();
        if log.contains("native Chrono: installed date 19890531") {
            migrated = true;
        }
        if migrated
            && alpha.connected
            && beta.connected
            && alpha.peers().any(|p| p.has_info && p.pose.name == "beta")
            && beta.peers().any(|p| p.has_info && p.pose.name == "alpha")
        {
            relayed_after = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        joined_before,
        "the two clients must connect before midnight"
    );
    assert!(
        migrated,
        "Chrono.cfg transition must rebuild CPU map on May 31"
    );
    assert!(
        relayed_after,
        "connected clients must still exchange poses after the Chrono swap"
    );

    let mut installed_map = world.map;
    let report = omsi_lan::mods::fetch_selected(
        &server.root,
        &mut installed_map,
        addr,
        alpha.session,
        None,
        None,
        &mut |_, _, _| {},
    )
    .expect("authenticated mod manifest must survive automatic Chrono");
    assert_eq!(report.fetched, 0);
    assert_eq!(report.bytes, 0);
    alpha.leave();
    beta.leave();
}

#[test]
fn native_restart_rebuilds_the_cpu_world_without_disconnecting_lan_clients() {
    let (mut server, addr) = start_fixture_mode(2, true);
    let world = WorldInfo {
        map: "Maps/Fixture/global.cfg".into(),
        date: "1989-05-30".into(),
        time: 9.0 * 3600.0,
        ..Default::default()
    };
    let mut alpha = LanSession::join_addr(vec![addr], None, "alpha", world.clone()).unwrap();
    let mut beta = LanSession::join_addr(vec![addr], None, "beta", world.clone()).unwrap();
    let pa = driver("alpha", 101.0);
    let pb = driver("beta", 202.0);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut joined = false;
    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("native host stopped before initial join: {status}");
        }
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if alpha.connected && beta.connected {
            joined = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(joined, "both clients must join before restarting");
    let path = server.root.join("server-test.cfg");
    let current = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        current.replace("passengers = 1", "passengers = 0\nai_max_humans = 2000"),
    )
    .unwrap();
    server
        .process
        .stdin
        .as_mut()
        .expect("local console pipe")
        .write_all(b"/restart\n")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut restarted = false;
    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("native host exited during in-session restart: {status}");
        }
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if fs::read_to_string(server.root.join("headless.log"))
            .unwrap_or_default()
            .contains("/restart completed")
        {
            restarted = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        restarted,
        "console restart was not consumed by the CPU tick"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut relayed = false;
    while Instant::now() < deadline {
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if alpha.connected
            && beta.connected
            && alpha.peers().any(|p| p.has_info && p.pose.name == "beta")
            && beta.peers().any(|p| p.has_info && p.pose.name == "alpha")
        {
            relayed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        relayed,
        "restart must preserve joined clients and pose relay"
    );
    let mut installed_map = world.map;
    let report = omsi_lan::mods::fetch_selected(
        &server.root,
        &mut installed_map,
        addr,
        alpha.session,
        None,
        None,
        &mut |_, _, _| {},
    )
    .expect("authenticated TCP manifest must survive CPU restart");
    assert_eq!(report.fetched, 0);
    assert_eq!(report.bytes, 0);
    alpha.leave();
    beta.leave();
}

#[test]
fn native_binary_relays_two_clients_without_a_host_vehicle() {
    let (mut server, addr) = start_fixture(2);
    let world = WorldInfo {
        map: "Maps/Fixture/global.cfg".into(),
        date: "1989-05-30".into(),
        time: 9.0 * 3600.0,
        ..Default::default()
    };
    let mut alpha = LanSession::join_addr(vec![addr], None, "alpha", world.clone()).unwrap();
    let mut beta = LanSession::join_addr(vec![addr], None, "beta", world.clone()).unwrap();
    let pa = driver("alpha", 101.0);
    let pb = driver("beta", 202.0);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut relayed = false;
    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("native server exited before relay handshake: {status}");
        }
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if alpha.connected
            && beta.connected
            && alpha
                .peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "beta")
            && beta
                .peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "alpha")
        {
            relayed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        relayed,
        "two LAN sessions did not exchange real poses via the separate native process"
    );
    // The graphical join also uses an authenticated TCP manifest AFTER
    // WELCOME, even with local map and vehicle files. UDP-only relay tests
    // could pass while real games blocked forever on this missing service.
    let mut installed_map = world.map.clone();
    let report = omsi_lan::mods::fetch_selected(
        &server.root,
        &mut installed_map,
        addr,
        alpha.session,
        None,
        None,
        &mut |_, _, _| {},
    )
    .expect("native host must answer an authenticated empty mod manifest");
    assert_eq!(
        report.fetched, 0,
        "test server must not share private files"
    );
    assert_eq!(report.bytes, 0);
    assert_eq!(installed_map, world.map);
    // The dedicated process never publishes a player-controlled bus of its
    // own; each client should see only the other connected driver.
    assert_eq!(
        alpha.peers().filter(|p| p.has_info && p.has_pose).count(),
        1
    );
    assert_eq!(beta.peers().filter(|p| p.has_info && p.has_pose).count(), 1);
    // A departed client must free the configured slot in the actual binary,
    // and no stale vehicle may survive into the next driver's session.
    beta.leave();
    drop(beta);
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut cleared = false;
    while Instant::now() < deadline {
        alpha.tick(0.05, &pa);
        if !alpha.peers().any(|p| p.pose.name == "beta") {
            cleared = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(cleared, "native process retained a departed driver");
    let mut charlie = LanSession::join_addr(vec![addr], None, "charlie", world).unwrap();
    let pc = driver("charlie", 303.0);
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut replaced = false;
    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("native server exited before reconnect: {status}");
        }
        alpha.tick(0.05, &pa);
        charlie.tick(0.05, &pc);
        if charlie.connected
            && alpha
                .peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "charlie")
            && charlie
                .peers()
                .any(|p| p.has_info && p.has_pose && p.pose.name == "alpha")
        {
            replaced = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        replaced,
        "native process did not reuse the released player slot"
    );
    assert!(!alpha.peers().any(|p| p.pose.name == "beta"));
    alpha.leave();
    charlie.leave();
}

#[test]
fn native_binary_relays_three_drivers_and_updated_vehicle_state() {
    let (mut server, addr) = start_fixture(3);
    let world = WorldInfo {
        map: "Maps/Fixture/global.cfg".into(),
        date: "1989-05-30".into(),
        time: 9.0 * 3600.0,
        ..Default::default()
    };
    let mut alpha = LanSession::join_addr(vec![addr], None, "alpha", world.clone()).unwrap();
    let mut beta = LanSession::join_addr(vec![addr], None, "beta", world.clone()).unwrap();

    let mut pa = driver("alpha", 101.0);
    pa.paint = "Test blue".into();
    pa.line = "76".into();
    pa.destination = "Grundorf".into();
    pa.doors = vec![1.0, 0.0];
    let pb = driver("beta", 202.0);
    let pc = driver("charlie", 303.0);

    let sees = |client: &LanSession, name: &str, x: f64| {
        client.peers().any(|peer| {
            peer.has_info
                && peer.has_pose
                && peer.pose.name == name
                && (peer.pose.x - x).abs() < 0.5
        })
    };

    // Connect the first two drivers before a late third client joins. This
    // covers the snapshot path used when players join an ongoing session.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut pair_ready = false;
    while Instant::now() < deadline {
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if alpha.connected
            && beta.connected
            && sees(&alpha, "beta", 202.0)
            && sees(&beta, "alpha", 101.0)
        {
            pair_ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        pair_ready,
        "native host did not connect the first two drivers"
    );
    let mut charlie = LanSession::join_addr(vec![addr], None, "charlie", world).unwrap();

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut exchanged = false;
    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("native server exited before three-client exchange: {status}");
        }
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        charlie.tick(0.05, &pc);
        if alpha.connected
            && beta.connected
            && charlie.connected
            && sees(&alpha, "beta", 202.0)
            && sees(&alpha, "charlie", 303.0)
            && sees(&beta, "alpha", 101.0)
            && sees(&beta, "charlie", 303.0)
            && sees(&charlie, "alpha", 101.0)
            && sees(&charlie, "beta", 202.0)
        {
            exchanged = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        exchanged,
        "three independent clients must see both remote drivers through the native executable"
    );
    for client in [&alpha, &beta, &charlie] {
        assert_eq!(
            client.peers().filter(|p| p.has_info && p.has_pose).count(),
            2,
            "the dedicated host must not inject a phantom fourth vehicle"
        );
    }

    // Changing a driven bus is more than a static handshake: test both the
    // high-frequency pose/door state and the slower INFO metadata path.
    pa.x = 151.0;
    pa.doors = vec![0.0, 1.0];
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut updated = false;
    while Instant::now() < deadline {
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        charlie.tick(0.05, &pc);
        let has_updated_bus = |client: &LanSession| {
            client.peers().any(|peer| {
                peer.has_info
                    && peer.has_pose
                    && peer.pose.name == "alpha"
                    && peer.pose.paint == "Test blue"
                    && peer.pose.line == "76"
                    && peer.pose.destination == "Grundorf"
                    && (peer.pose.x - 151.0).abs() < 0.5
                    && peer.pose.doors.len() == 2
                    && peer.pose.doors[1] > 0.9
            })
        };
        if has_updated_bus(&beta) && has_updated_bus(&charlie) {
            updated = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        updated,
        "native host must relay live movement, door state, paint and destination to both observers"
    );

    alpha.leave();
    beta.leave();
    charlie.leave();
}

#[test]
fn simulated_native_host_relays_client_owned_passengers_end_to_end() {
    use omsi_net::world::{self, Activity, Desc, PersonPlace, PersonState, WorldFrame};

    let (mut server, addr) = start_fixture_mode(2, true);
    let info = WorldInfo {
        map: "Maps/Fixture/global.cfg".into(),
        date: "1989-05-30".into(),
        time: 9.0 * 3600.0,
        ..Default::default()
    };
    let mut alpha = LanSession::join_addr(vec![addr], None, "alpha", info.clone()).unwrap();
    let mut beta = LanSession::join_addr(vec![addr], None, "beta", info).unwrap();
    let pa = driver("alpha", 10.0);
    let pb = driver("beta", 12.0);
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if let Some(status) = server.process.try_wait().unwrap() {
            panic!("experimental server exited before clients joined: {status}");
        }
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if alpha.connected && beta.connected {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        alpha.connected && beta.connected,
        "two clients must join simulation mode"
    );

    let person = PersonState {
        id: 7,
        activity: Activity::Walk,
        place: PersonPlace::Foot {
            x: 11.0,
            y: 10.0,
            z: 0.0,
            heading: 0.0,
            speed: 1.0,
            waiting: None,
        },
    };
    let mut relayed_id = None;
    let mut described = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut n = 0;
    while Instant::now() < deadline {
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        if n % 10 == 0 {
            alpha.send_desc_up(&Desc::Person {
                id: person.id,
                file: "Humans/Example.hum".into(),
            });
        }
        alpha.send_world_up(&WorldFrame {
            people: vec![person],
            ..Default::default()
        });
        for desc in beta.take_descs() {
            let Desc::Person { id, file } = desc;
            if id >= 0xC0_0000 && file == "Humans/Example.hum" {
                described = true;
            }
        }
        for (_, frame) in beta.take_world() {
            if let Some(state) = frame.people.into_iter().find(|p| p.id >= 0xC0_0000) {
                assert_eq!(state.activity, Activity::Walk);
                assert!(state.id <= world::MAX_ID);
                relayed_id = Some(state.id);
            }
        }
        if relayed_id.is_some() && described {
            break;
        }
        n += 1;
        std::thread::sleep(Duration::from_millis(10));
    }
    let id = relayed_id.expect("another client must receive relayed rider through the executable");
    assert!(described, "receiver must also get the model description");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut removed = false;
    while Instant::now() < deadline {
        alpha.tick(0.05, &pa);
        beta.tick(0.05, &pb);
        alpha.send_world_up(&WorldFrame {
            gone: vec![(true, person.id)],
            ..Default::default()
        });
        for (_, frame) in beta.take_world() {
            if frame.gone.contains(&(true, id)) {
                removed = true;
                break;
            }
        }
        if removed {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        removed,
        "removing a client-owned rider must reach the other client"
    );
    alpha.leave();
    beta.leave();
}
