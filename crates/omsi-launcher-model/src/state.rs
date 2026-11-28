//! What the launcher knows and has chosen, and the work it has running in the background
//! (reading the content lists and timetables, polling the running games and installs).
//!
//! Everything slow runs on a thread of its own and comes back as a [`Msg`]; the window
//! drains them at the start of each frame, so it never waits.

mod messages;
mod vehicle;

#[cfg(test)]
mod tests;

use crate::host::host;
use omsi_launcher_lib as core;
pub use omsi_launcher_lib::crash::{CRASH_TAIL_GAP, crash_of, disconnect_of};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

/// Results of background work.
pub enum Msg {
    Content(
        Result<
            (
                Vec<core::MapInfo>,
                Vec<core::VehicleInfo>,
                Vec<core::WeatherInfo>,
            ),
            String,
        >,
    ),
    /// The first reading of the content: the maps and weathers (quick to read), before the
    /// buses.
    ContentEarly {
        maps: Vec<core::MapInfo>,
        weathers: Vec<core::WeatherInfo>,
    },
    /// The first reading of the content: the buses of the next few folders, how many
    /// folders are read and how many there are.
    VehiclesRead {
        batch: Vec<core::VehicleInfo>,
        done: usize,
        total: usize,
    },
    Lines {
        map: String,
        date: String,
        lines: Result<Vec<core::LineInfo>, String>,
    },
    Poll(Result<core::Poll, String>),
    Profile(Result<core::Profile, String>),
    Profiles(Vec<String>),
    Ibis {
        key: String,
        info: Result<core::IbisInfo, String>,
    },
    Args(Result<Vec<String>, String>),
    Launched(Result<core::Launched, String>),
    Stopped {
        pid: u32,
        result: Result<bool, String>,
    },
    LogTail {
        pid: u32,
        lines: Vec<String>,
    },
    Mods(Result<core::ModsStatus, String>),
    /// A mod switched on or off, or removed (its name), or why not.
    ModChanged {
        result: Result<String, String>,
        done: &'static str,
    },
    ModInfo(Result<core::install::SourceInfo, String>),
    Installed(Result<core::install::Progress, String>),
    Join(serde_json::Value),
    Server {
        address: String,
        info: Result<omsi_net::ws::ServerInfo, String>,
    },
    /// A background job stopped on an error of its own (a panic): whatever it was loading
    /// is not coming.
    Crashed(String),
}

/// A server in the Multiplayer page's list (`~/.openomsi/servers.json`), as the player
/// added it: its address (`https://….trycloudflare.com`, `http://host:port`) and a name of
/// their own (empty: the server's).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
    pub protocol: JoinProto,
}

/// How the Multiplayer page joins a server.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JoinProto {
    /// Straight to the game port over UDP (gateway status still required).
    Udp,
    /// Through the server's web gateway (a WebSocket).
    WebSocket,
    /// UDP for bare addresses with an explicit game port, falling back to the discovered
    /// WebSocket gateway; a port that answered status uses WebSocket immediately.
    #[default]
    #[serde(other)]
    Auto,
}

/// Select the gameplay address only after a gateway has answered. A bare explicit
/// port in Auto names the game port unless that exact HTTP endpoint answered status.
fn server_join_address(
    address: &str,
    proto: JoinProto,
    info: Option<&omsi_net::ws::ServerInfo>,
) -> Result<(String, JoinProto), &'static str> {
    let address = address.trim();
    let is_web = omsi_net::ws::ws_url(address).is_some() || omsi_net::official::is_alias(address);
    let explicit_port = address.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty()
            && (!host.contains(':') || (host.starts_with('[') && host.ends_with(']')))
            && !host.contains('/')
            && !host.contains(char::is_whitespace)
            && !port.is_empty()
            && port.bytes().all(|b| b.is_ascii_digit())
            && port.parse::<u16>().is_ok()
    });
    let proto = match proto {
        JoinProto::Auto
            if !is_web
                && explicit_port
                && !info.is_some_and(|i| i.reached_at == format!("http://{address}")) =>
        {
            JoinProto::Udp
        }
        JoinProto::Auto => JoinProto::WebSocket,
        p => p,
    };
    if proto == JoinProto::Udp && is_web {
        return Err("UDP needs the game's address (host or host:port), not a web link");
    }
    let info =
        info.ok_or("The server has not answered yet (is its address right? is it running?)")?;
    let target = if proto == JoinProto::WebSocket && !is_web && !info.reached_at.is_empty() {
        info.reached_at.clone()
    } else {
        address.to_string()
    };
    Ok((target, proto))
}

/// A code host's first responsive status endpoint, including its tunnel.
fn host_status(code: &str) -> Result<omsi_net::ws::ServerInfo, String> {
    omsi_net::ws::query_session(code)
}

/// The list as saved, with the official server first when it is not in it.
fn with_official(mut list: Vec<ServerEntry>) -> Vec<ServerEntry> {
    if !list
        .iter()
        .any(|s| omsi_net::official::is_alias(&s.address))
    {
        list.insert(
            0,
            ServerEntry {
                name: omsi_net::official::NAME.into(),
                address: omsi_net::official::ALIAS.into(),
                protocol: JoinProto::Auto,
            },
        );
    }
    list
}

fn servers_path() -> std::path::PathBuf {
    core::data_dir().join("servers.json")
}

/// The duty as it is remembered between launches (`~/.openomsi/launcher-duty.json`).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Choice {
    pub bus: String,
    pub paint: String,
    /// The number plate the player typed for the bus (empty: as the content says).
    pub plate: String,
    /// The fleet number picked from the bus's `[number]` list (empty: no selection yet).
    pub number: String,
    /// The variables the player picked for this bus - its onboard computer (the IBIS unit or
    /// ticket printer its repaints name), a door type: name to the value a paint gives it
    /// (empty: the paint's own, `--setvar`, #4077).
    pub setvars: std::collections::BTreeMap<String, String>,
    pub hof: String,
    /// The depot file was chosen by hand (else it follows the map and the date).
    pub hof_manual: bool,
    pub map: String,
    /// The start: the entry point's place in the map's list, or -1 = automatic (nearest to
    /// the duty's first stop by road).
    pub entry: i32,
    pub line: Option<String>,
    /// The launcher tour ID (legacy choices contain the displayed number).
    pub tour: Option<String>,
    pub free: bool,
    /// Minutes of the day.
    pub time: i32,
    pub start_trip: Option<(String, String, usize, i32)>,
    pub date: String,
    /// "auto", spring, summer, autumn, winter.
    pub season: String,
    /// The season's phase: early, mid, late (with a season chosen).
    pub phase: String,
    /// The player's own date from before a season moved it ("By date" gives it back).
    pub own_date: Option<String>,
    pub weather: String,
    pub traffic: f32,
    pub passengers: bool,
    pub schedule: bool,
    pub autostart: bool,
    pub comfortable_cabin_on_start: bool,
    /// Start on foot (no bus of one's own until one is placed).
    pub on_foot: bool,
    /// off, host, join
    pub lan_mode: String,
    pub lan_addr: String,
    /// Discovered gateway to try if an Auto UDP join gets no answer.
    pub lan_ws_fallback: Option<String>,
    #[serde(default)]
    pub session_buses: String,
    /// 2: `entry` may be -1 (automatic); older files had a fixed entry point there.
    pub version: u32,
}

impl Default for Choice {
    fn default() -> Self {
        Choice {
            bus: String::new(),
            paint: String::new(),
            plate: String::new(),
            number: String::new(),
            setvars: Default::default(),
            hof: String::new(),
            hof_manual: false,
            map: String::new(),
            entry: -1,
            line: None,
            tour: None,
            free: false,
            time: 9 * 60,
            start_trip: None,
            date: "1989-05-30".into(),
            season: "auto".into(),
            phase: "mid".into(),
            own_date: None,
            weather: String::new(),
            traffic: 30.0,
            passengers: true,
            schedule: true,
            autostart: false,
            comfortable_cabin_on_start: false,
            on_foot: false,
            lan_mode: "off".into(),
            lan_addr: String::new(),
            lan_ws_fallback: None,
            session_buses: String::new(),
            version: 2,
        }
    }
}

fn choice_path() -> std::path::PathBuf {
    core::data_dir().join("launcher-duty.json")
}

impl Choice {
    pub fn load() -> Choice {
        let c: Choice = std::fs::read_to_string(choice_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self::fresh(c)
    }

    /// A duty read back from the file, as a new launcher starts with it.
    fn fresh(mut c: Choice) -> Choice {
        // A server is joined on purpose, in the launcher's own session (Multiplayer page): a
        // join left in the file made every later start connect to it, with no sign of it on
        // the Drive page ("Leave Server" is only there for a server joined since the launch).
        if c.lan_mode == "join" {
            c.lan_mode = "off".into();
            c.lan_ws_fallback = None;
        }
        if c.version < 2 {
            // the start was always the map's first entry point: now it is automatic
            c.entry = -1;
            c.version = 2;
        }
        // (older launchers took a vehicle line of a broken ailists.cfg for the map's depot)
        if c.hof.to_ascii_lowercase().contains(".bus")
            || c.hof.to_ascii_lowercase().contains(".ovh")
        {
            c.hof.clear();
        }
        // a season chosen: the date in its phase, as the game will have it (older launchers
        // kept the day of the month; the date may have followed the computer's since)
        if let Some(d) = super::season::launcher_date(&c.season, &c.phase, &c.date, &c.map, true) {
            c.date = d;
        }
        c
    }
    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(choice_path(), t);
        }
    }
}

pub struct State {
    pub config: core::Config,
    pub maps: Vec<core::MapInfo>,
    pub vehicles: Vec<core::VehicleInfo>,
    pub weathers: Vec<core::WeatherInfo>,
    pub lines: Vec<core::LineInfo>,
    pub lines_for: (String, String),
    pub loading_content: bool,
    /// The lists are filled while they are read (the first reading: nothing to show yet).
    pub content_first: bool,
    /// The content changed while it was being read: read it again when that is done.
    pub reload_content: bool,
    pub loading_lines: bool,
    pub choice: Choice,
    pub choice_dirty: f32,
    /// Map, whether it has a `laststn.osn`, when that was looked up.
    pub last_sit: Option<(String, Vec<core::SavedSituation>, std::time::Instant)>,
    /// Which of them "Continue" starts (0: the newest, the last situation when there is one).
    pub save_pick: usize,
    pub profiles: Vec<String>,
    pub profile: Option<core::Profile>,
    pub settings: serde_json::Value,
    pub settings_dirty: f32,
    /// `settings.cfg` as last read or written here: a game changes it too (its Options
    /// in the pause menu), and the launcher's copy from before must not be written back
    /// over that.
    settings_file: Option<String>,
    pub keybindings: serde_json::Value,
    pub keybindings_error: String,
    pub instances: Vec<core::Instance>,
    pub queued_launch: Option<core::Duty>,
    /// Start was pressed: the graphics device stays given up until the list of games has the
    /// game started (its process, once it is known), 15 s at most.
    pub launch_hold: Option<std::time::Instant>,
    launched_pid: Option<u32>,
    /// A game started from here ended on an error: what it said, and the end of its log
    /// (see `crash_of`), for the dialog that asks to report it.
    pub crash: Option<(String, String)>,
    /// Windows: the desktop launcher could not open for want of WebView2 and opened this one
    /// instead (`OPENOMSI_NO_WEBVIEW`): the dialog that offers to install it, with how the
    /// install goes ("", "busy", "done" or the error).
    pub webview: Option<std::sync::Arc<std::sync::Mutex<String>>>,
    /// A game started from here was sent away by its server (kicked, banned) or turned away
    /// at the door: the server's message, for the "Disconnected from the server" dialog.
    pub disconnected: Option<String>,
    pub jobs: Vec<core::install::Progress>,
    pub mods: Option<core::ModsStatus>,
    pub mods_asked: bool,
    /// The mod being switched or removed (its id).
    pub mod_busy: Option<String>,
    /// Installs finished when the Mods page last asked for the mods.
    pub mods_jobs_seen: usize,
    pub mod_info: Option<Result<core::install::SourceInfo, String>>,
    pub mod_path: String,
    pub mod_mode: usize,
    pub ibis: Option<(String, Result<core::IbisInfo, String>)>,
    pub cmdline: String,
    pub join: (bool, String),
    pub join_checked: String,
    pub logs: std::collections::HashMap<u32, Vec<String>>,
    pub open_logs: std::collections::HashSet<u32>,
    pub stopping: std::collections::HashSet<u32>,
    /// Content that appeared while the launcher was open (marked NEW for a while).
    pub fresh: std::collections::HashMap<String, Instant>,
    pub status: (String, bool, Instant),
    stamp: Option<String>,
    poll_t: f32,
    polling: bool,
    pub second_armed: Option<Instant>,
    /// Multiplayer: the saved servers, what each said last (and when it was asked), and the
    /// one the Drive page is joined to now (its address).
    pub servers: Vec<ServerEntry>,
    pub server_info:
        std::collections::HashMap<String, (Instant, Result<omsi_net::ws::ServerInfo, String>)>,
    pub server_asked: std::collections::HashMap<String, Instant>,
    pub joined_server: Option<String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl State {
    pub fn new() -> State {
        let (tx, rx) = channel();
        let config = core::load_config();
        let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
        host().ui_language(
            settings
                .get("language")
                .and_then(|x| x.as_str())
                .unwrap_or("ENG"),
        );
        host().mt_enable(
            settings
                .get("machine_translation")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
        );
        let keybindings = core::get_keybindings().unwrap_or(serde_json::Value::Null);
        let mut choice = Choice::load();
        // (at the real time, a trip picked in an earlier session has most likely left)
        if settings
            .get("use_real_time")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            choice.start_trip = None;
        }
        let mut s = State {
            config,
            maps: Vec::new(),
            vehicles: Vec::new(),
            weathers: Vec::new(),
            lines: Vec::new(),
            lines_for: (String::new(), String::new()),
            loading_content: false,
            content_first: false,
            reload_content: false,
            loading_lines: false,
            choice,
            choice_dirty: 0.0,
            last_sit: None,
            save_pick: 0,
            profiles: Vec::new(),
            profile: None,
            settings,
            settings_dirty: 0.0,
            settings_file: read_settings_file(),
            keybindings,
            keybindings_error: String::new(),
            instances: Vec::new(),
            queued_launch: None,
            launch_hold: None,
            launched_pid: None,
            crash: None,
            webview: (cfg!(windows) && std::env::var_os("OPENOMSI_NO_WEBVIEW").is_some())
                .then(|| std::sync::Arc::new(std::sync::Mutex::new(String::new()))),
            disconnected: None,
            jobs: Vec::new(),
            mods: None,
            mods_asked: false,
            mod_busy: None,
            mods_jobs_seen: 0,
            mod_info: None,
            mod_path: String::new(),
            mod_mode: 0,
            ibis: None,
            cmdline: String::new(),
            join: (true, String::new()),
            join_checked: "\u{0}".into(),
            logs: Default::default(),
            open_logs: Default::default(),
            stopping: Default::default(),
            fresh: Default::default(),
            status: (String::new(), false, Instant::now()),
            stamp: None,
            poll_t: 0.0,
            polling: false,
            second_armed: None,
            servers: with_official(
                std::fs::read(servers_path())
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or_default(),
            ),
            server_info: Default::default(),
            server_asked: Default::default(),
            joined_server: None,
            tx,
            rx,
        };
        s.load_content();
        s.load_profiles();
        s.poll_now();
        // (a phone runs the game in the launcher's process: a crash took both, and the
        // launcher learns of it from the previous run's log)
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            s.crash = host().previous_run_crash();
        }
        s
    }

    pub fn set_status(&mut self, text: impl Into<String>, err: bool) {
        let t = text.into();
        if err {
            core::log_to_file(&format!("ERROR {t}"));
        }
        self.status = (host().tr(&t), err, Instant::now());
    }

    /// A game is about to start, starting (not in the list of games yet) or running.
    pub fn in_game(&self) -> bool {
        self.queued_launch.is_some()
            || self
                .launch_hold
                .is_some_and(|t| t.elapsed().as_secs_f32() < 15.0)
            || self.instances.iter().any(|i| i.running)
    }

    pub fn instances_ready(&self) -> bool {
        self.stamp.is_some()
    }

    pub fn spawn_launch(&mut self, d: core::Duty) {
        self.launch_hold = Some(std::time::Instant::now());
        self.launched_pid = None;
        self.spawn(move || Msg::Launched(core::launch(&d).map_err(|e| format!("{e:#}"))));
    }

    pub(crate) fn spawn(&self, f: impl FnOnce() -> Msg + Send + 'static) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            // (a job that panics - an odd file of some mod - sent nothing, and the page
            // it was loading for said "loading" for ever: it says what went wrong instead)
            let m = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|e| {
                let why = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "an unknown error".into());
                Msg::Crashed(why)
            });
            let _ = tx.send(m);
        });
    }

    pub fn load_content(&mut self) {
        self.loading_content = true;
        // the first reading shows what it has read as it goes: a big installation's buses
        // (thousands of folders) took minutes, with nothing on the page all that time
        self.content_first = self.maps.is_empty() && self.vehicles.is_empty();
        self.set_status("Reading the OMSI folder...", false);
        let tx = self.tx.clone();
        self.spawn(move || {
            let r = (|| -> anyhow::Result<_> {
                let maps = core::list_maps()?;
                let weathers = core::list_weather()?;
                let _ = tx.send(Msg::ContentEarly {
                    maps: maps.clone(),
                    weathers: weathers.clone(),
                });
                let vehicles = core::list_vehicles_progress(|batch, done, total| {
                    let _ = tx.send(Msg::VehiclesRead {
                        batch: batch.to_vec(),
                        done,
                        total,
                    });
                })?;
                Ok((maps, vehicles, weathers))
            })();
            Msg::Content(r.map_err(|e| format!("{e:#}")))
        });
    }

    /// A reading of the lists ended: the next poll's stamp is the new reference, and a change
    /// that came while reading is read now.
    fn content_done(&mut self) {
        self.stamp = None;
        if std::mem::take(&mut self.reload_content) {
            self.load_content();
        }
    }

    /// The map chosen last time where it still exists, else the one OMSI 2 had last, else
    /// the first.
    fn pick_map(&mut self) {
        if !self.maps.iter().any(|m| m.file == self.choice.map) {
            let last = core::omsi_options(std::path::Path::new(&self.config.root))
                .and_then(|o| o.last_map);
            if let Some(m) = last
                .and_then(|l| self.maps.iter().find(|m| m.file.eq_ignore_ascii_case(&l)))
                .or(self.maps.first())
            {
                self.choice.map = m.file.clone();
                self.choice.entry = -1;
            }
        }
    }

    pub fn load_lines(&mut self) {
        let (map, date) = (self.choice.map.clone(), self.choice.date.clone());
        if map.is_empty() {
            return;
        }
        // (every change of the date comes here: the depot file of that date, unless one was
        // chosen by hand)
        let hof = self.default_hof();
        if hof != self.choice.hof && !self.choice.hof_manual {
            log::info!("depot file for {date}: {hof}");
            self.choice.hof = hof;
        }
        self.loading_lines = true;
        self.lines_for = (map.clone(), date.clone());
        self.spawn(move || {
            let lines = core::list_lines(&map, &date).map_err(|e| format!("{e:#}"));
            Msg::Lines { map, date, lines }
        });
    }

    pub fn load_profiles(&mut self) {
        self.spawn(|| Msg::Profiles(core::list_profiles().unwrap_or_default()));
        let name = self.config.profile.clone();
        self.spawn(move || Msg::Profile(core::get_profile(&name).map_err(|e| format!("{e:#}"))));
    }

    pub fn load_profile(&mut self) {
        let name = self.config.profile.clone();
        self.spawn(move || Msg::Profile(core::get_profile(&name).map_err(|e| format!("{e:#}"))));
    }

    pub fn load_ibis(&mut self) {
        let (Some(line), false) = (self.choice.line.clone(), self.choice.free) else {
            self.ibis = None;
            return;
        };
        let (bus, hof) = (self.choice.bus.clone(), self.choice.hof.clone());
        let key = format!("{bus}|{hof}|{line}");
        if self.ibis.as_ref().map(|i| i.0 == key).unwrap_or(false) {
            return;
        }
        self.spawn(move || Msg::Ibis {
            key,
            info: core::ibis_info(&bus, &hof, &line, "", &[]).map_err(|e| format!("{e:#}")),
        });
    }

    pub fn load_args(&mut self) {
        let d = self.duty();
        self.spawn(move || Msg::Args(core::duty_args(&d).map_err(|e| format!("{e:#}"))));
    }

    pub fn load_mods(&mut self) {
        self.mods_asked = true;
        self.spawn(|| Msg::Mods(core::mods_status().map_err(|e| format!("{e:#}"))));
    }

    /// Switch the mod `id` on or off, in the background (its folders move).
    pub fn mod_toggle(&mut self, id: String, on: bool) {
        self.mod_busy = Some(id.clone());
        self.spawn(move || Msg::ModChanged {
            result: core::mod_set_enabled(&id, on).map_err(|e| format!("{e:#}")),
            done: if on {
                "The mod {name} is switched on"
            } else {
                "The mod {name} is switched off"
            },
        });
    }

    /// Delete the mod `id`, in the background.
    pub fn mod_remove(&mut self, id: String) {
        self.mod_busy = Some(id.clone());
        self.spawn(move || Msg::ModChanged {
            result: core::mod_remove(&id).map_err(|e| format!("{e:#}")),
            done: "The mod {name} is deleted",
        });
    }

    pub fn check_join(&mut self) {
        if self.join_checked == self.choice.lan_addr {
            return;
        }
        self.join_checked = self.choice.lan_addr.clone();
        let t = self.choice.lan_addr.clone();
        self.spawn(move || Msg::Join(core::check_join(&t)));
        // the host's status (its buses): at the code's addresses, else through its tunnel
        if omsi_net::looks_like_code(&self.choice.lan_addr) {
            let code = self.choice.lan_addr.clone();
            self.spawn(move || Msg::Server {
                info: host_status(&code),
                address: code,
            });
        }
    }

    /// The buses the host or server offers (None: any, it has not said).
    pub fn host_vehicles(&self) -> Option<Vec<String>> {
        if self.choice.lan_mode != "join" {
            return None;
        }
        let key = self
            .joined_server
            .clone()
            .unwrap_or_else(|| self.choice.lan_addr.clone());
        let list = self
            .server_info
            .get(&key)?
            .1
            .as_ref()
            .ok()?
            .vehicles
            .clone();
        (!list.is_empty()).then_some(list)
    }

    /// Keep the server list on disk.
    pub fn save_servers(&self) {
        let _ = std::fs::create_dir_all(core::data_dir());
        let _ = std::fs::write(
            servers_path(),
            serde_json::to_vec_pretty(&self.servers).unwrap_or_default(),
        );
    }

    /// Ask a server about itself (its status and icon), at most every `every` seconds.
    pub fn ask_server(&mut self, address: &str, every: f32) {
        if self
            .server_asked
            .get(address)
            .map(|t| t.elapsed().as_secs_f32() < every)
            .unwrap_or(false)
        {
            return;
        }
        self.server_asked
            .insert(address.to_string(), Instant::now());
        let a = address.to_string();
        self.spawn(move || Msg::Server {
            info: omsi_net::ws::query(&a, true),
            address: a,
        });
    }

    /// The Drive page joins `address`: the server's map is the map, the session is joined.
    /// `proto` says how: over UDP straight to the game port, over the server's web gateway
    /// (a WebSocket), or `Auto` (UDP for bare game ports; WebSocket for responding web ports).
    /// Every selection requires a successful gateway status response; Auto UDP keeps
    /// that gateway as a fallback if the initial handshake gets no answer.
    pub fn join_server(&mut self, address: &str, proto: JoinProto) {
        let info = self
            .server_info
            .get(address)
            .cloned()
            .and_then(|(_, r)| r.ok());
        let auto = proto == JoinProto::Auto;
        let (lan_addr, proto) = match server_join_address(address, proto, info.as_ref()) {
            Ok(join) => join,
            Err(message) => {
                self.set_status(message, true);
                return;
            }
        };
        if let Some(info) = info.as_ref() {
            // A map not installed here comes with the server's mods when the game joins
            // (`lan_mods`), so this is a notice, not a refusal.
            // The Drive page needs a map of this installation chosen, so the choice stays
            // as it is then: `duty()` starts the game on the server's map anyway.
            if !self.maps.is_empty()
                && !self
                    .maps
                    .iter()
                    .any(|m| m.file.eq_ignore_ascii_case(&info.map))
            {
                self.set_status(format!("The server plays {}, which is not installed here: it is fetched from the server on joining.", info.map), false);
            } else {
                self.choice.map = info.map.clone();
            }
        }
        self.choice.lan_mode = "join".into();
        self.choice.lan_addr = lan_addr;
        self.choice.lan_ws_fallback = info
            .as_ref()
            .filter(|_| auto && proto == JoinProto::Udp)
            .map(|i| i.reached_at.clone())
            .filter(|url| !url.is_empty());
        self.joined_server = Some(address.to_string());
        let name = info
            .as_ref()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| address.to_string());
        self.join = (
            true,
            core::i18n::tr("the server {name}").replace("{name}", &name),
        );
        self.join_checked = address.to_string();
        self.touched();
        let joined = if proto == JoinProto::Udp {
            "Joined {} over UDP - choose your bus and duty, then Start the duty"
        } else {
            "Joined {} - choose your bus and duty, then Start the duty"
        };
        self.set_status(joined.replace("{}", &name), false);
    }

    /// Back to playing alone (the Drive page's "Leave Server").
    pub fn leave_server(&mut self) {
        self.joined_server = None;
        self.choice.lan_mode = "off".into();
        self.choice.lan_ws_fallback = None;
        self.touched();
    }

    pub fn poll_now(&mut self) {
        if self.polling {
            return;
        }
        self.polling = true;
        self.spawn(|| Msg::Poll(core::poll().map_err(|e| format!("{e:#}"))));
    }

    pub fn log_tail(&mut self, pid: u32) {
        self.spawn(move || Msg::LogTail {
            pid,
            lines: core::log_tail(pid, 80).unwrap_or_default(),
        });
    }

    pub fn stop(&mut self, pid: u32) {
        self.stopping.insert(pid);
        self.spawn(move || Msg::Stopped {
            pid,
            result: core::stop_instance(pid).map_err(|e| format!("{e:#}")),
        });
    }

    pub fn install(&mut self, path: String) {
        let mode = ["auto", "extract", "inplace"][self.mod_mode.min(2)].to_string();
        self.mod_path = path.clone();
        let p2 = path.clone();
        self.spawn(move || {
            Msg::ModInfo(core::inspect_mod(std::path::Path::new(&p2)).map_err(|e| format!("{e:#}")))
        });
        self.spawn(move || {
            Msg::Installed(
                core::start_install(std::path::Path::new(&path), &mode)
                    .map_err(|e| format!("{e:#}")),
            )
        });
    }

    pub fn launch(&mut self) {
        if !self.save_pending_settings() {
            return;
        }
        if !omsi_cfg::missing_original_essentials(std::path::Path::new(&self.config.root))
            .is_empty()
        {
            self.set_status(
                "A session needs the original OMSI 2: choose its folder under Setup first.",
                true,
            );
            return;
        }
        let d = self.duty();
        self.set_status("Starting the game...", false);
        self.queued_launch = Some(d);
    }

    /// The duty as the backend takes it.
    pub fn duty(&self) -> core::Duty {
        let c = &self.choice;
        let lan = match c.lan_mode.as_str() {
            "host" => "host".to_string(),
            "join" => format!("join:{}", c.lan_addr.trim()),
            _ => "off".to_string(),
        };
        // Joining: the host's map, as its status gives it (a code's host is asked when the
        // code is typed). The game takes it from the host's welcome too, but only when that
        // comes before the map is loaded: through a tunnel it came later, the game started
        // on the map chosen here, and the players never met ("the host drives on X10 Berlin,
        // you on Berlin-Spandau"). A map not installed here comes with the host's mods.
        let host_map = (c.lan_mode == "join")
            .then(|| {
                self.joined_server
                    .clone()
                    .unwrap_or_else(|| c.lan_addr.clone())
            })
            .and_then(|k| {
                self.server_info
                    .get(&k)
                    .and_then(|x| x.1.as_ref().ok())
                    .map(|i| i.map.trim().replace('\\', "/"))
            })
            .filter(|m| m.to_ascii_lowercase().contains("maps/"));
        core::Duty {
            map: host_map.unwrap_or_else(|| c.map.clone()),
            bus: c.bus.clone(),
            paint: Some(c.paint.clone()).filter(|p| !p.is_empty()),
            plate: Some(c.plate.clone()).filter(|p| !p.trim().is_empty()),
            number: Some(c.number.clone()).filter(|n| !n.trim().is_empty()),
            setvars: Some(
                c.setvars
                    .iter()
                    .filter(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty())
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )
            .filter(|m: &std::collections::BTreeMap<String, String>| !m.is_empty()),
            hof: Some(if c.hof_manual {
                c.hof.clone()
            } else {
                self.default_hof()
            })
            .filter(|p| !p.is_empty()),
            entry: Some(c.entry),
            line: if c.free { None } else { c.line.clone() },
            tour: if c.free {
                None
            } else {
                self.tour().map(|t| t.number.clone())
            },
            trip: if c.free {
                None
            } else {
                self.picked_trip().map(|i| i.to_string())
            },
            whole_tour: !c.free && self.picked_trip().is_some(),
            time: format!("{:02}:{:02}", c.time / 60, c.time % 60),
            date: Some(c.date.clone()),
            weather: Some(c.weather.clone()).filter(|w| !w.is_empty()),
            traffic: Some(c.traffic.round() as u32),
            passengers: Some(c.passengers),
            schedule: Some(c.schedule),
            autostart: Some(c.autostart),
            comfortable_cabin_on_start: Some(c.comfortable_cabin_on_start),
            on_foot: Some(c.on_foot),
            profile: Some(self.config.profile.clone()).filter(|p| !p.is_empty()),
            lan: Some(lan),
            lan_ws_fallback: c.lan_ws_fallback.clone().filter(|_| {
                c.lan_mode == "join" && self.joined_server.as_deref() == Some(c.lan_addr.as_str())
            }),
            // The native launcher has no server-fleet consent dialog yet. Never
            // silently authorize a download from this entry point.
            allow_server_fleet: false,
            session_buses: Some(c.session_buses.clone())
                .filter(|b| c.lan_mode == "host" && !b.trim().is_empty())
                .map(|b| b.trim().to_string()),
            lan_name: None,
            season: Some(c.season.clone()).filter(|s| s != "auto").map(|s| {
                super::season::launcher_choice(&s, &c.phase)
                    .map(|x| x.word())
                    .unwrap_or(s)
            }),
            tutorial: None,
            situation: None,
            depot: None,
        }
    }

    /// The situations to continue on the chosen map: the last one and the save slots
    /// (looked up at most every two seconds: the page asks every frame).
    pub fn saved_situations(&mut self) -> &[core::SavedSituation] {
        let fresh = self
            .last_sit
            .as_ref()
            .is_some_and(|(m, _, t)| *m == self.choice.map && t.elapsed().as_secs_f32() < 2.0);
        if !fresh {
            if self
                .last_sit
                .as_ref()
                .is_some_and(|(m, _, _)| *m != self.choice.map)
            {
                self.save_pick = 0;
            }
            let list = core::saved_situations(&self.choice.map);
            self.save_pick = self.save_pick.min(list.len().saturating_sub(1));
            self.last_sit = Some((self.choice.map.clone(), list, std::time::Instant::now()));
        }
        self.last_sit
            .as_ref()
            .map(|x| x.1.as_slice())
            .unwrap_or(&[])
    }

    /// Whether a situation to continue lies on the chosen map.
    pub fn has_last_situation(&mut self) -> bool {
        !self.saved_situations().is_empty()
    }

    /// Continue the situation chosen of the map's (`laststn.osn`, or a save slot, #341).
    pub fn launch_last_situation(&mut self) {
        if !self.save_pending_settings() {
            return;
        }
        let pick = self.save_pick;
        let list = self.saved_situations();
        let Some(file) = list
            .get(pick)
            .or_else(|| list.first())
            .map(|s| s.file.clone())
        else {
            self.set_status("No situation left on this map yet", true);
            return;
        };
        let mut d = self.duty();
        d.situation = Some(file.to_string_lossy().to_string());
        d.lan = Some("off".into());
        self.set_status("Continuing where you left off...", false);
        self.queued_launch = Some(d);
    }

    /// Start one of OMSI's tutorials (1..4).
    pub fn launch_tutorial(&mut self, n: usize) {
        if !self.save_pending_settings() {
            return;
        }
        let mut d = self.duty();
        d.tutorial = Some(n);
        d.lan = Some("off".into());
        self.set_status("Starting the tutorial...", false);
        self.queued_launch = Some(d);
    }

    /// Settings a game changed while it ran: taken over, unless the launcher's own changes
    /// wait to be saved (those win, as the later ones).
    fn reload_changed_settings(&mut self) {
        if self.settings_dirty > 0.0 {
            return;
        }
        let now = read_settings_file();
        if now.is_some() && now != self.settings_file {
            if let Ok(v) = core::get_settings() {
                self.settings = v;
            }
            self.settings_file = now;
        }
    }

    fn save_pending_settings(&mut self) -> bool {
        if self.settings_dirty <= 0.0 {
            return true;
        }
        match core::save_settings(&self.settings) {
            Ok(()) => {
                self.settings_dirty = 0.0;
                self.settings_file = read_settings_file();
                true
            }
            Err(e) => {
                self.set_status(format!("Could not save settings ({e:#})"), true);
                false
            }
        }
    }

    /// Something of the duty changed: remember it (soon) and refresh what depends on it.
    pub fn touched(&mut self) {
        self.choice_dirty = 0.4;
    }

    /// Work done each frame: results of background work, the regular poll, saving.
    fn follow_clock(&mut self) {
        let on = |k: &str| {
            self.settings
                .get(k)
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        let (time, date, year) = (
            on("use_real_time"),
            on("use_real_date"),
            on("use_real_year"),
        );
        // (a season chosen sets the date: its phase's, see `season_chosen`)
        let date = date && self.choice.season == "auto";
        if !time && !date {
            return;
        }
        let Some((y, mo, d, h, m)) = core::local_now() else {
            return;
        };
        if time {
            self.choice.time = h * 60 + m;
        }
        if date {
            let y = if year {
                y
            } else {
                self.choice
                    .date
                    .get(..4)
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(y)
            };
            let today = format!("{y:04}-{mo:02}-{d:02}");
            if self.choice.date != today {
                self.choice.date = today;
                self.load_lines();
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        while let Ok(m) = self.rx.try_recv() {
            self.handle(m);
        }
        self.follow_clock();
        self.poll_t -= dt;
        if self.poll_t <= 0.0 {
            self.poll_t = 2.5;
            self.poll_now();
            self.reload_changed_settings();
        }
        if self.choice_dirty > 0.0 {
            self.choice_dirty -= dt;
            if self.choice_dirty <= 0.0 {
                self.choice.save();
                self.load_args();
                self.load_ibis();
            }
        }
        if self.settings_dirty > 0.0 {
            self.settings_dirty -= dt;
            if self.settings_dirty <= 0.0 {
                match core::save_settings(&self.settings) {
                    Ok(()) => {
                        self.settings_file = read_settings_file();
                        self.set_status("Settings saved.", false)
                    }
                    Err(e) => self.set_status(format!("{e:#}"), true),
                }
            }
        }
        if self.choice.lan_mode == "join" {
            self.check_join();
        }
        let now = Instant::now();
        self.fresh
            .retain(|_, t| now.duration_since(*t).as_secs() < 600);
    }

    // --- helpers for the pages -------------------------------------------------------------

    pub fn bus(&self) -> Option<&core::VehicleInfo> {
        self.vehicles.iter().find(|v| v.file == self.choice.bus)
    }
    pub fn map(&self) -> Option<&core::MapInfo> {
        self.maps.iter().find(|m| m.file == self.choice.map)
    }
    pub fn line(&self) -> Option<&core::LineInfo> {
        let l = self.choice.line.as_ref()?;
        self.lines.iter().find(|x| &x.name == l)
    }
    pub fn tour(&self) -> Option<&core::TourInfo> {
        let t = self.choice.tour.as_ref()?;
        self.line()?.tour(t)
    }

    pub fn select_bus(&mut self, file: &str) {
        if self.choice.bus == file {
            return;
        }
        self.choice.bus = file.to_string();
        self.choice.paint.clear();
        self.choice.number = self.random_bus_number();
        self.choice.setvars.clear();
        // (a hand-picked depot file stays when the new bus has one of that name)
        let keep = self.choice.hof_manual
            && self.bus().is_some_and(|v| {
                v.hofs
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(&self.choice.hof))
            });
        if !keep {
            self.choice.hof_manual = false;
            self.choice.hof = self.default_hof();
        }
        self.touched();
    }

    pub fn select_map(&mut self, file: &str) {
        if self.choice.map == file {
            return;
        }
        self.choice.map = file.to_string();
        self.choice.entry = -1;
        self.choice.line = None;
        self.choice.tour = None;
        self.choice.hof = self.default_hof();
        self.lines.clear();
        self.load_lines();
        self.touched();
    }

    /// The season the chosen date (or the override) means.
    pub fn season(&self) -> &str {
        if self.choice.season != "auto" {
            return &self.choice.season;
        }
        let m: u32 = self
            .choice
            .date
            .get(5..7)
            .and_then(|x| x.parse().ok())
            .unwrap_or(5);
        match m {
            12 | 1 | 2 => "winter",
            3..=5 => "spring",
            6..=8 => "summer",
            _ => "autumn",
        }
    }

    /// Weather presets that make sense in the season (no summer shower in the snow).
    pub fn weather_fits(&self, w: &core::WeatherInfo) -> bool {
        let rain = w.precip.starts_with("rain");
        match self.season() {
            "winter" => w.snow || if rain { w.temp <= 10.0 } else { w.temp <= 16.0 },
            s => {
                if w.snow || w.temp <= 0.0 {
                    return false;
                }
                s != "summer" || w.temp >= 8.0
            }
        }
    }

    /// The trip of the tour a start at the chosen time begins with: the next to leave (one
    /// that left a minute or two ago still counts), else the tour's last.
    pub fn first_trip(&self) -> Option<usize> {
        let t = self.tour()?;
        if let Some(i) = self.picked_trip() {
            if let Some(k) = t.trips.iter().position(|x| x.index == i) {
                return Some(k);
            }
        }
        let now = self.choice.time as f64 * 60.0;
        trip_index_at(t, now)
    }

    pub fn picked_trip(&self) -> Option<usize> {
        picked_trip_of(&self.choice)
    }

    /// The start time follows the computer's clock (`use_real_time`).
    pub fn real_time(&self) -> bool {
        self.settings
            .get("use_real_time")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Start the tour at trip `index` (leaving at `departure`): at its departure, or at the
    /// real time, which then stays and the bus waits for the trip.
    pub fn pick_trip(&mut self, index: usize, departure: f64) {
        let (Some(line), Some(tour)) = (self.choice.line.clone(), self.choice.tour.clone()) else {
            return;
        };
        if !self.real_time() {
            self.choice.time = (departure / 60.0).floor() as i32;
        }
        self.choice.start_trip = Some((line, tour, index, self.choice.time));
        self.touched();
    }
}

/// The trip picked to start the duty with: kept while its line and tour stay chosen. The start
/// time may move a few minutes earlier so the driver can board and set up (#4334); the fourth
/// field of `start_trip` is only the time the trip was picked at, not a condition.
fn picked_trip_of(c: &Choice) -> Option<usize> {
    let (line, tour, index, _) = c.start_trip.as_ref()?;
    (c.line.as_ref() == Some(line) && c.tour.as_ref() == Some(tour)).then_some(*index)
}

/// Whether the game started from here (its process `pid`) is in a list of games.
fn game_listed(pid: Option<u32>, instances: &[core::Instance]) -> bool {
    pid.is_some_and(|p| instances.iter().any(|i| i.pid == p))
}

/// The trip a tour starts with at `now`, shared by the route preview and the launch choice.
pub(super) fn trip_index_at(tour: &core::TourInfo, now: f64) -> Option<usize> {
    tour.trips
        .iter()
        .position(|x| x.departure >= now - 120.0)
        .or(if tour.trips.is_empty() {
            None
        } else {
            Some(tour.trips.len() - 1)
        })
}

pub fn hhmm(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as i64;
    format!("{:02}:{:02}", (s / 3600) % 24, (s % 3600) / 60)
}

pub fn fmt_bytes(b: u64) -> String {
    let b = b as f64;
    if b >= (1u64 << 30) as f64 {
        format!("{:.1} GB", b / (1u64 << 30) as f64)
    } else if b >= (1u64 << 20) as f64 {
        format!("{:.0} MB", b / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", (b / 1024.0).max(1.0))
    }
}

pub fn short_map(m: &str) -> String {
    m.trim_start_matches("maps/")
        .trim_end_matches("/global.cfg")
        .to_string()
}

/// Why `root` is not an OMSI 2 to play on, said so that the player knows what to choose.
pub fn root_problem(root: &str) -> String {
    let root = root.trim();
    let p = std::path::Path::new(root);
    let missing = omsi_cfg::missing_original_essentials(p);
    if root.is_empty() {
        "The original OMSI 2 was not found automatically: choose its folder (the one with Omsi.exe, maps and Vehicles in it) under Setup and press Save.".to_string()
    } else if !p.exists() {
        format!(
            "The folder {root} does not exist: choose the folder of the original OMSI 2 (with Omsi.exe, maps and Vehicles in it) under Setup."
        )
    } else if missing.iter().any(|m| m.contains("content folder"))
        || p.join("openomsi.exe").exists()
        || p.join("openomsi").is_file()
    {
        format!(
            "The folder {root} is openOMSI's own folder, not OMSI 2's: choose the folder of the original game (with Omsi.exe in it) under Setup."
        )
    } else if missing.is_empty() {
        String::new()
    } else {
        format!(
            "The folder {root} is not a complete OMSI 2 - it lacks {}. openOMSI plays on the original's stock content: choose the folder of a complete installation under Setup.",
            missing
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
#[cfg(test)]
mod choice_tests;
#[cfg(test)]
mod launch_tests;

fn read_settings_file() -> Option<String> {
    std::fs::read_to_string(core::data_dir().join("settings.cfg")).ok()
}
