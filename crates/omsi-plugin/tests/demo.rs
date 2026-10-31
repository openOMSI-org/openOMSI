//! The demo plugin (crates/omsi-plugin/demo) driven in this process and through
//! `omsi-plugin-host`, the way the game drives a plugin.
use omsi_plugin::{HostConfig, Plugin, PluginIo, Plugins, Remote};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn demo_library() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let release = !cfg!(debug_assertions);
    let mut cmd = std::process::Command::new(cargo);
    cmd.args(["build", "-q", "-p", "omsi-demo-plugin", "--manifest-path"]).arg(root.join("Cargo.toml"));
    if release {
        cmd.arg("--release");
    }
    assert!(cmd.status().unwrap().success(), "building the demo plugin");
    let name = format!("{}omsi_demo_plugin{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX);
    // the test binary is target/<profile>/deps/…; the library sits in target/<profile>
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().join(name)
}

/// A plugins folder with the demo as `Demo\demo.dll`, listed in `Demo\demo.opl`.
fn plugins_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omsi-plugin-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Demo")).unwrap();
    std::fs::copy(demo_library(), dir.join("Demo/demo.dll")).unwrap();
    std::fs::write(
        dir.join("Demo/demo.opl"),
        "[dll]\r\ndemo\\DEMO.dll\r\n\r\n[varlist]\r\n2\r\nthrottle\r\nmissing_var\r\n\r\n[stringvarlist]\r\n1\r\nnote\r\n\r\n[systemvarlist]\r\n1\r\nTime\r\n\r\n[triggers]\r\n1\r\nhorn\r\n\r\n[scripttexturelist]\r\n3\r\n2\r\n0\r\n9\r\n",
    )
    .unwrap();
    dir
}

#[derive(Default)]
struct Game {
    time: f32,
    vars: HashMap<String, f32>,
    strings: HashMap<String, String>,
    fired: Vec<(String, bool)>,
    /// The vehicle's script textures: (width, height, RGBA).
    textures: Vec<(u32, u32, Vec<u8>)>,
    /// The passengers of this frame and the tickets the plugin changed: (person, change, by).
    asks: Vec<omsi_plugin::TicketAsk>,
    changed: Vec<(u32, omsi_plugin::TicketChange, String)>,
}

impl PluginIo for Game {
    fn system(&mut self, name: &str) -> Option<f32> {
        (name == "Time").then_some(self.time)
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        true
    }
    fn var(&mut self, name: &str) -> Option<f32> {
        self.vars.get(name).copied()
    }
    fn set_var(&mut self, name: &str, v: f32) {
        self.vars.insert(name.into(), v);
    }
    fn string(&mut self, name: &str) -> Option<String> {
        self.strings.get(name).cloned()
    }
    fn set_string(&mut self, name: &str, s: &str) {
        self.strings.insert(name.into(), s.into());
    }
    fn fire(&mut self, trigger: &str, down: bool) {
        self.fired.push((trigger.into(), down));
    }
    fn script_texture_size(&self, index: usize) -> Option<(u32, u32)> {
        self.textures.get(index).map(|t| (t.0, t.1))
    }
    fn set_script_texture(&mut self, index: usize, width: u32, height: u32, rgba: &[u8]) -> bool {
        match self.textures.get_mut(index) {
            Some(t) if (t.0, t.1) == (width, height) => {
                t.2.copy_from_slice(rgba);
                true
            }
            _ => false,
        }
    }
    fn ticket_asks(&self) -> Vec<omsi_plugin::TicketAsk> {
        self.asks.clone()
    }
    fn people_set_ticket(&mut self, id: u32, change: &omsi_plugin::TicketChange, by: &str) -> Result<(), String> {
        self.changed.push((id, change.clone(), by.to_string()));
        Ok(())
    }
}

fn run(plugins: &mut Plugins) -> Game {
    let mut g = Game::default();
    g.vars.insert("throttle".into(), 0.25);
    g.strings.insert("note".into(), "................".into());
    // [scripttexturelist] lists 2 (entry 0: painted), 0 (entry 1: never written) and 9
    // (entry 2: the vehicle has no such texture, skipped)
    g.textures = vec![(4, 4, vec![0; 64]), (2, 2, vec![0; 16]), (8, 2, vec![0; 64])];
    for t in [0.0, 1.0, 1.5, 2.0] {
        g.time = t;
        // a press in the middle of texture 2 at 1 s (and one on texture 0, which the
        // demo's entry 1 ignores)
        if t == 1.0 {
            plugins.pointer = vec![(2, omsi_plugin::POINTER_DOWN, 0.5, 0.5), (0, omsi_plugin::POINTER_DOWN, 0.1, 0.1)];
            // and two passengers boarding: one rides from stop 2 to 5 of the trip, one is
            // a child; a third rides where the trip does not go (left as it is)
            let ask = |person: u32, age: f32, dest: Option<usize>| omsi_plugin::TicketAsk { person, age, kind: omsi_plugin::TicketKind::Buy, ticket: 1, name: "Full fare".into(), price: 20.0, stop_index: Some(1), destination: Some("Nový Bor".into()), destination_index: dest, ..Default::default() };
            g.asks = vec![ask(5, 40.0, Some(4)), ask(6, 8.0, Some(4)), ask(7, 40.0, None)];
        }
        plugins.frame(&mut g);
        g.asks.clear();
    }
    g
}

/// The tickets the demo changed: the ride priced by its stops, the child's ticket gone.
fn check_tickets(g: &Game) {
    let ride = omsi_plugin::TicketChange { price: Some(30.0), name: Some("Nový Bor, 3 stops".into()), voice: Some("Zone_3".into()), ..Default::default() };
    let child = omsi_plugin::TicketChange { kind: Some(omsi_plugin::TicketKind::None), ..Default::default() };
    assert_eq!(g.changed, [(5, ride, "DEMO".to_string()), (6, child, "DEMO".to_string())]);
}

/// What the demo drew after four frames: the third texture (number 2 of the vehicle)
/// painted, the first pixel's alpha counting the four calls through the kept buffer; the
/// other untouched.
fn check_textures(g: &Game) {
    let (w, h, px) = &g.textures[2];
    assert_eq!((*w, *h), (8, 2));
    assert_eq!(&px[..4], &[0, 0, 7, 4]);
    // pixel (5, 1)
    let i = (8 + 5) * 4;
    assert_eq!(&px[i..i + 4], &[5, 1, 7, 255]);
    // the press reached the plugin in the texture's pixels: (0.5, 0.5) of 8 x 2 is (4, 1)
    assert_eq!(&px[4..8], &[4, 1, omsi_plugin::POINTER_DOWN, 1]);
    assert!(g.textures[0].2.iter().all(|b| *b == 0));
}

#[test]
fn in_process() {
    let dir = plugins_dir("local");
    let mut plugins = Plugins::load(&[dir.clone()], &HostConfig::default());
    assert_eq!(plugins.loaded.len(), 1);
    let g = run(&mut plugins);
    // doubled every frame: 0.25 · 2⁴
    assert_eq!(g.vars["throttle"], 4.0);
    // written into the buffer as long as the text it held
    assert_eq!(g.strings["note"], "seen at 2");
    // the system variable reached the plugin before its trigger was asked: down at 1 s
    // (odd), still down at 1.5 s, up at 2 s
    assert_eq!(g.fired, [("horn".to_string(), true), ("horn".to_string(), false)]);
    check_textures(&g);
    check_tickets(&g);
    plugins.finalize();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn through_the_host() {
    let dir = plugins_dir("remote");
    let dll = dir.join("Demo/demo.dll");
    let host = Path::new(env!("CARGO_BIN_EXE_omsi-plugin-host"));
    let r = Remote::spawn(None, host, &dll, &dir).expect("host starts");
    assert!(r.procs().variable && r.procs().trigger && r.procs().system && r.procs().string && r.procs().texture && r.procs().pointer && r.procs().ticket);
    drop(r);
    // the same frames through a plugin whose library will not load here: force the host
    let hosts = HostConfig { host32: Some(host.to_path_buf()), runner: None };
    std::fs::write(dir.join("Demo/demo.opl"), "[dll]\r\nDemo\\notalibrary.dll\r\n").unwrap();
    std::fs::write(dir.join("Demo/notalibrary.dll"), b"MZ not really").unwrap();
    let err = Plugin::load(&dir.join("Demo/demo.opl"), &dir, &hosts).err().expect("a file that is no library fails");
    assert!(err.contains("could not load") || err.contains("host") || err.contains("Wine"), "{err}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn host_frames() {
    let dir = plugins_dir("frames");
    let host = Path::new(env!("CARGO_BIN_EXE_omsi-plugin-host"));
    let mut r = Remote::spawn(None, host, &dir.join("Demo/demo.dll"), &dir).unwrap();
    let tickets = vec![
        omsi_plugin::TicketCall { person: 1, age: 40.0, stop: "Střížkov".into(), destination: "Nový Bor".into(), stop_number: 2, destination_number: 5, kind: 2, ticket: 1, price: 20.0, name: "Full fare".into() },
        omsi_plugin::TicketCall { person: 2, age: 9.0, kind: 2, ticket: 2, ..Default::default() },
        omsi_plugin::TicketCall { person: 3, age: 40.0, kind: 2, ticket: 1, ..Default::default() },
    ];
    let f = omsi_plugin::Frame { system: vec![(0, 3.0)], vars: vec![(0, 1.5)], strings: vec![(0, "0123456789ab".into())], triggers: vec![0], textures: vec![(0, 3, 2), (1, 2, 2)], pointer: vec![(0, omsi_plugin::POINTER_UP, 2.0, 1.0)], tickets };
    let reply = r.frame(&f).unwrap().expect("the host answers");
    assert_eq!(reply.system, [None]);
    assert_eq!(reply.vars, [Some(3.0)]);
    assert_eq!(reply.strings, [Some("seen at 3".to_string())]);
    assert_eq!(reply.triggers_active, [true]);
    assert_eq!(reply.textures.len(), 2);
    let px = reply.textures[0].as_ref().expect("entry 0 is written");
    assert_eq!(px.len(), 3 * 2 * 4);
    assert_eq!(&px[..4], &[0, 0, 7, 1]);
    // the release went through the host before the picture was drawn
    assert_eq!(&px[4..8], &[2, 1, omsi_plugin::POINTER_UP, 1]);
    assert_eq!(reply.textures[1], None);
    // the passengers' tickets went through the host and back
    let full = omsi_plugin::TicketAnswer { kind: 2, ticket: 1, price: 30.0, name: "Nový Bor, 3 stops".into(), voice: "Zone_3".into() };
    assert_eq!(reply.tickets, [Some(full), Some(omsi_plugin::TicketAnswer { kind: 0, ticket: 2, ..Default::default() }), None]);
    // the host keeps the buffer: the second frame counts on from the first
    let reply = r.frame(&f).unwrap().expect("the host answers");
    assert_eq!(reply.textures[0].as_ref().unwrap()[3], 2);
    r.finalize();
    let _ = std::fs::remove_dir_all(dir);
}

/// A plugin busy in its own code (SuperRadio waiting on its station) holds the game only a
/// moment: the frame goes on without it, its answer is taken whole when it comes - the horn
/// it pressed in that frame too - and a host stuck for good is ended at once at the end.
#[test]
fn host_busy_in_its_own_code() {
    let dir = plugins_dir("busy");
    let host = Path::new(env!("CARGO_BIN_EXE_omsi-plugin-host"));
    let mut r = Remote::spawn(None, host, &dir.join("Demo/demo.dll"), &dir).unwrap();
    // (Time 1: the horn is down)
    let busy = omsi_plugin::Frame { system: vec![(0, 1.0)], vars: vec![(0, -1.5)], triggers: vec![0], ..Default::default() };
    let asked = std::time::Instant::now();
    assert!(r.frame(&busy).unwrap().is_none(), "the game does not wait for it");
    assert!(asked.elapsed() < std::time::Duration::from_millis(1200));
    assert!(r.owes());
    let late = loop {
        if let Some(reply) = r.late().unwrap() {
            break reply;
        }
        assert!(asked.elapsed() < std::time::Duration::from_secs(10), "the late answer comes");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(late.vars, [Some(-3.0)]);
    assert_eq!(late.triggers_active, [true]);
    assert!(!r.owes());
    // then it answers as before: at once, or (asked with the short wait of a plugin that
    // was late) as a late answer a moment after
    let quick = omsi_plugin::Frame { system: vec![(0, 2.0)], vars: vec![(0, 1.5)], triggers: vec![0], ..Default::default() };
    let reply = match r.frame(&quick).unwrap() {
        Some(reply) => reply,
        None => loop {
            if let Some(reply) = r.late().unwrap() {
                break reply;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        },
    };
    assert_eq!(reply.vars, [Some(3.0)]);
    assert_eq!(reply.triggers_active, [false]);
    // answered within the short wait, it is waited for in full again
    let reply = loop {
        if let Some(reply) = r.frame(&quick).unwrap() {
            break reply;
        }
        while r.late().unwrap().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    assert_eq!(reply.vars, [Some(3.0)]);
    // stuck for good: the end does not wait for it
    let stuck = omsi_plugin::Frame { vars: vec![(0, -60.0)], ..Default::default() };
    assert!(r.frame(&stuck).unwrap().is_none());
    let ending = std::time::Instant::now();
    r.finalize();
    assert!(ending.elapsed() < std::time::Duration::from_secs(1), "finalize waited {:?}", ending.elapsed());
    let _ = std::fs::remove_dir_all(dir);
}

/// The real thing: the demo built as a 32-bit Windows DLL, run by the 32-bit host under
/// Wine. Needs `OMSI_TEST_WINE_DIR` = the folder holding `omsi-plugin-host.exe` and
/// `omsi_demo_plugin.dll` built for i686-pc-windows-gnu (see docs/PLUGINS.md), and Wine.
#[test]
fn windows_dll_under_wine() {
    let Some(dir) = omsi_cfg::flags::OMSI_TEST_WINE_DIR.live_os().map(PathBuf::from) else { return };
    let hosts = HostConfig::detect();
    let wine = hosts.runner.clone().or_else(|| cfg!(windows).then(PathBuf::new));
    let runner = if cfg!(windows) { None } else { Some(wine.expect("wine on the path")) };
    let mut r = Remote::spawn(runner.as_deref(), &dir.join("omsi-plugin-host.exe"), &dir.join("omsi_demo_plugin.dll"), &dir).expect("the 32-bit host starts");
    let f = omsi_plugin::Frame { system: vec![(0, 5.0)], vars: vec![(0, 0.75)], strings: vec![(0, "abcdefghijk".into())], triggers: vec![0], textures: vec![(0, 2, 2)], ..Default::default() };
    let reply = r.frame(&f).unwrap().expect("the host answers");
    assert_eq!(reply.vars, [Some(1.5)]);
    assert_eq!(reply.textures[0].as_ref().map(|p| p[..4].to_vec()), Some(vec![0, 0, 7, 1]));
    assert_eq!(reply.strings, [Some("seen at 5".to_string())]);
    assert_eq!(reply.triggers_active, [true]);
    r.finalize();
}
