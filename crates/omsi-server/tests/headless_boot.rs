//! End-to-end native binary smoke test without a window, GPU or OMSI assets.
//! The synthetic content is used solely inside CI and is not redistributed.

use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// server.cfg deliberately clamps port=0 to 1. Pick an actual unprivileged
/// free TCP port for the host and gateway instead.
fn free_port() -> u16 {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap().port()
}

#[test]
fn native_server_boots_a_cpu_map_and_stops_after_bounded_ticks() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("openomsi-headless-{}-{nanos}", std::process::id()));
    let map = root.join("Maps/Fixture");
    write(
        &map.join("global.cfg"),
        "[name]\nHeadless Fixture\n[map]\n0\n0\ntile_0_0.map\n",
    );
    // Tile format v10: detail level, definition, object ID, unlinked flag,
    // x/height/y, heading, length, radius, grades, cant, texture offset.
    write(
        &map.join("tile_0_0.map"),
        "[version]\n10\n[spline]\n0\nSplines/smoke.sli\n101\n-1\n10\n0\n10\n0\n40\n0\n0\n0\n0\n0\n0\n\n[object]\n0\nSceneryobjects\\Generic\\entrypoint_bus.sco\n7\n6.5\n225.5\n0.2\n90\n1\nCI Spawn\n",
    );
    write(
        &root.join("Splines/smoke.sli"),
        "[length]\n40\n[texture]\nroad.bmp\n[profile]\n0\n[profilepnt]\n-1.5\n0\n0\n1\n[profilepnt]\n1.5\n0\n1\n1\n[path]\n0\n0\n0\n3\n0\n",
    );
    // No [entrypoints] section: the old server discovers this .sco marker.
    write(
        &root.join("Sceneryobjects/Generic/entrypoint_bus.sco"),
        "[entrypoint]\n[onlyeditor]\n",
    );
    let cfg = root.join("server-test.cfg");
    let port = free_port();
    let mut web_port = free_port();
    while web_port == port {
        web_port = free_port();
    }
    write(
        &cfg,
        &format!(
            "name = CI Headless Fixture\nmap = Maps/Fixture/global.cfg\ndate = 1989-05-30\ntime = 09:00\ntraffic = 0\ntimetable = 0\npassengers = 0\nport = {port}\nweb_port = {web_port}\nmax_players = 2\ntunnel = 0\npublic_server = 0\npublic_address = off\nreal_time = 0\nmetar_sync = 0\naccept_uploads = 0\nshare_maps = 0\nparked_buses = 0\nvoice = 0\nadmin_web = off\n"
        ),
    );
    let result = Command::new(env!("CARGO_BIN_EXE_omsi-server"))
        .arg(&root)
        .arg(&cfg)
        .arg("--experimental-free-roam")
        .arg("--ticks")
        .arg("3")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("start standalone headless executable");
    assert!(
        result.status.success(),
        "native host failed with code {:?}\nstdout:\n{}\nstderr:\n{}",
        result.status.code(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("entry points: 1 from the tiles' entry objects"),
        "headless executable failed to discover the legacy entrypoint: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("stopped cleanly after 3 ticks"),
        "headless host did not finish its bounded simulation: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // Run the real native executable with AI, passenger and Chrono scheduler
    // enabled against an isolated asset-free fixture. It must not require a
    // renderer, GPU or a graphical client merely to advance its first ticks.
    let experimental = fs::read_to_string(&cfg)
        .unwrap()
        .replace("traffic = 0", "traffic = 4")
        .replace("timetable = 0", "timetable = 1")
        .replace("passengers = 0", "passengers = 1");
    fs::write(&cfg, experimental).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_omsi-server"))
        .arg(&root)
        .arg(&cfg)
        .arg("--experimental-simulation")
        .arg("--ticks")
        .arg("65")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .spawn()
        .expect("start isolated CPU-simulation server");
    // Exercise the actual stdin -> queued command -> simulation-tick path,
    // rather than only the parser unit test.
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(b"/reload\n")
        .expect("send local reload command");
    let simulated = child
        .wait_with_output()
        .expect("wait for native simulation");
    assert!(
        simulated.status.success(),
        "CPU simulation failed with code {:?}\\nstdout:\\n{}\\nstderr:\\n{}",
        simulated.status.code(),
        String::from_utf8_lossy(&simulated.stdout),
        String::from_utf8_lossy(&simulated.stderr)
    );
    let diagnostic = String::from_utf8_lossy(&simulated.stderr);
    assert!(diagnostic.contains("CPU SIMULATION"), "{diagnostic}");
    assert!(
        diagnostic.contains("server console: /reload: no changes"),
        "local /reload was not executed through the active simulation: {diagnostic}"
    );
    assert!(
        diagnostic.contains("stopped cleanly after 65 ticks"),
        "{diagnostic}"
    );
    let _ = fs::remove_dir_all(&root);
}
