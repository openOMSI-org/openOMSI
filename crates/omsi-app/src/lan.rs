//! LAN play in the game: starting a session from the command line, taking the host's
//! world, what we send about our bus, the other players' buses (drawn and heard), putting
//! our bus where nobody stands, the chat line and the lines the HUD and the launcher show.
//!
//! The transport is `omsi-net`. The game adds what needs the world:
//! - a joining player takes the host's date, time of day, weather and season before its
//!   map is loaded (`adopt_host_world`), and the host's clock keeps it in step afterwards;
//! - the footprints of the host's vehicles, and placing a joining player's bus in front of
//!   or behind whatever stands at its entry point (along the road);
//! - the remote states as vehicles: the other player's vehicle type runs as an AI copy
//!   whose pose comes off the network, fed the pedals, lights, indicators and doors as its
//!   AI scripts expect them, with the model's own lamp, `[visible]`, sound and moving-part
//!   variables pinned to the sender's values (`SyncTable`); its outside sounds (`[sound_ai]`
//!   plus the horn and indicator relay of `[sound]`) play where it stands;
//! - the chat line (V to type, Enter to send) with join and leave notices.
//!
//! While a session runs the game keeps `~/.openomsi/lan/<instance>.json` up to date
//! (role, session code, address, players, warnings), which is where the launcher reads the
//! code to show it with a copy button. `<instance>` is the id the launcher gives the game in
//! `OMSI_INSTANCE`, else the process id.

use crate::scene::{self, World};
use crate::{Args, Player};
use glam::DVec3;
use omsi_lan::remote::lan_now;
use omsi_net::{Footprint, LanEvent, LanSession, PartPose, Pose, Role};
use omsi_render::{Renderer, Scene};
use omsi_script::VarId;
use omsi_sim::traffic::{Lane, LaneKind, Network};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// How long a joining player's game waits for the host's welcome (its world) before the
/// map is loaded, and for the host's list of what stands at the spawn.
pub const WELCOME_WAIT: Duration = Duration::from_secs(3);
/// How long a joining game waits for the host's welcome before it loads a map: a host busy
/// loading a heavy part of a big map answered after the 3 s it used to wait, and the player
/// was left on the map chosen before joining, where nobody ever met them.
pub const HOST_ANSWER_WAIT: Duration = Duration::from_secs(8);
/// The largest vehicle file loaded for another player (a `.bus` is a few hundred KB).
const MAX_VEHICLE_FILE: u64 = 16 << 20;
/// Other players' buses are heard within this distance (m), and fall silent beyond 1.2 times it.
const HEAR_RANGE: f64 = 250.0;
/// A clock further off the host's than this (s) is set; a smaller difference is caught up.
const CLOCK_JUMP: f64 = 2.0;
/// Chat lines shown, and for how long (s).
const CHAT_LINES: usize = 6;

/// Stable random seed shared by all processes in one LAN room.
pub fn population_seed(session: &LanSession) -> u64 {
    session.session ^ 0x4F4D_5349_4C41_4E31
}

// ---------------------------------------------------------------------------------------
// what of a vehicle goes over the network

pub use omsi_lan::sync::{SyncTable, VarTable};
use omsi_lan::sync::var_table;

pub use omsi_lan::spawn::footprint_of;
use omsi_lan::spawn::{along, blocked, heading_gap, obb, BUS_BOX, CAR_BOX, GAP_ACROSS};

/// The sync table of a vehicle type (worked out once per type).
fn sync_table(game: &mut LanGame, v: &omsi_sim::VehicleInstance) -> Arc<SyncTable> {
    let ty = &v.ty;
    game.tables
        .entry(ty.def.path.clone())
        .or_insert_with(|| {
            let parts: Vec<Arc<omsi_sim::VehicleType>> = v.trailers.iter().map(|t| t.ty.clone()).collect();
            let t = Arc::new(SyncTable::new(ty, &parts));
            log::info!(
                "LAN: sync table of {}: {}",
                ty.def.path.display(),
                t.describe()
            );
            t
        })
        .clone()
}

// ---------------------------------------------------------------------------------------
// the game's side of a session

/// Another player's bus as we draw and hear it: their vehicle type run as an AI vehicle
/// whose pose comes off the network instead of a lane.
impl RemoteVehicle {
    /// The figure they are drawn as: the one at their wheel, which their avatar on foot
    /// wears too (`sync_remote_walkers`).
    pub(crate) fn figure_type(&self) -> Option<Arc<omsi_sim::human::HumanType>> {
        self.driver.as_ref().map(|d| d.human_type())
    }
}

pub struct RemoteVehicle {
    vehicle: omsi_sim::VehicleInstance,
    render: scene::VehicleRender,
    trailer_renders: Vec<scene::VehicleRender>,
    pub name: String,
    table: Arc<SyncTable>,
    sounds: Vec<omsi_audio::SoundSet>,
    /// Its `[sound]` heard from inside, while we ride in it (`SyncTable::interior`).
    inside_sounds: Option<omsi_audio::SoundSet>,
    /// Where the latest pose puts the bus and its rear sections, for smoothing between
    /// network updates.
    target: (DVec3, f64),
    rear: Vec<(DVec3, f64)>,
    /// The latest pose's place and when it came (for carrying it on between updates).
    pose_seen: (DVec3, Instant),
    /// Door openings, suspension travel and the sync table's values as drawn (they glide
    /// towards the latest state).
    doors: Vec<f32>,
    suspension: Vec<f32>,
    values: Vec<f32>,
    /// Metres driven, for the wheel rotation (backwards when reversing).
    odometer: f32,
    horn: bool,
    /// The depot file the destination display is set from.
    hof: Option<Arc<omsi_vehicle::Hof>>,
    /// The line / destination shown last.
    shown: (String, String),
    /// Their bus type is not installed here: ours stands in for it.
    pub stand_in: bool,
    pub last: Pose,
    /// The bus file and paint scheme it was made in (`last` is the state drawn, which may be
    /// an interpolated older one).
    made_as: (String, String),
    /// The driver at the wheel (their bus stood empty here), hidden while they walk about.
    driver: Option<crate::driver::DriverFigure>,
    /// The figure (`Pose::figure`) the driver was made for: made again when another comes.
    /// (Made once, it kept the figure guessed before their INFO named theirs - one of the
    /// map's drivers -, and on foot they were a passenger picked by their id.)
    driver_for: Option<String>,
    /// Their states as they came, and the moment of theirs drawn (`omsi_lan::remote`).
    track: omsi_lan::remote::PoseTrack,
    /// Every variable of theirs as it came (`omsi_net::vars`), pinned each frame, and their
    /// strings - for the same vehicle files only (`vars`' hash); and the variables the pose
    /// already carries, which glide instead (lamps, switches, moving parts, doors).
    vars: Option<VarTable>,
    synced: hashbrown::HashMap<u16, f32>,
    synced_strings: hashbrown::HashMap<u16, String>,
    smooth: hashbrown::HashSet<u16>,
}

impl omsi_lan::voice::RemoteBuses for LanGame {
    fn remote_bus(&self, id: u32) -> Option<DVec3> {
        self.remotes.get(&id).map(|r| r.vehicle().position)
    }
}

impl RemoteVehicle {
    /// The LAN player's vehicle as it is drawn here.
    pub fn vehicle(&self) -> &omsi_sim::VehicleInstance {
        &self.vehicle
    }

    /// The same, for the plugins (`omsi.set_other_var`: their next state writes it again).
    pub fn vehicle_mut(&mut self) -> &mut omsi_sim::VehicleInstance {
        &mut self.vehicle
    }
}

// The chat's keys are `chat_toggle` and `chat_open` of keyboard.cfg's [game]
// (`KeyboardCfg::with_game_defaults`: V, and '/' or - where the bus's gear down has '/', as in
// OMSI's own file - the key left of 1). Not Y for them: the stock file gives that
// scan code (21) to `scendes_set_z` unmodified and to `view_toggle_informationdisplay` with
// Ctrl, and a German keyboard's Y is `scendes_set_y`; V (47) is bound to nothing there.

/// The chat: its lines ("Name: text", "* notice"), oldest first, and the line being typed.
#[derive(Default)]
pub struct Chat {
    pub lines: Vec<String>,
    /// The line being typed: '/' or a click on the chat opens it, Enter sends it, Escape
    /// drops it.
    pub typing: Option<String>,
    /// `chat_toggle` (V) hides and shows the chat.
    pub hidden: bool,
    /// The chat is switched off in the settings: no box, no keys.
    pub disabled: bool,
    /// What was typed when the line lost the focus (a click elsewhere): back when it opens.
    draft: String,
    /// Keys pressed while typing: their release is the chat's too.
    swallow: hashbrown::HashSet<KeyCode>,
    /// Why the last line was not sent.
    error: Option<(String, Instant)>,
}

impl Chat {
    fn push(&mut self, line: String) {
        self.lines.push(line);
        let over = self.lines.len().saturating_sub(crate::ui::CHAT_KEEP);
        if over > 0 {
            self.lines.drain(..over);
        }
    }

    /// Why the last line was not sent, for a few seconds.
    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().filter(|(_, at)| at.elapsed().as_secs_f32() < 6.0).map(|(e, _)| e.as_str())
    }

    /// Open the input box (a click on the chat, the '/' key).
    pub fn open(&mut self) {
        if !self.disabled && self.typing.is_none() {
            self.hidden = false;
            self.typing = Some(std::mem::take(&mut self.draft));
        }
    }

    /// A click outside the chat takes the focus from the line (as a text box on a web
    /// page): the keys are the game's again, what was typed waits for the next time.
    pub fn blur(&mut self) {
        if let Some(t) = self.typing.take() {
            self.draft = t;
        }
    }
}

/// A change of the world the host asks for (clients): the game applies it to its clock
/// and weather.
#[derive(Debug, Clone, PartialEq)]
pub enum WorldUpdate {
    /// Set the clock to this day and time.
    Clock {
        year: i32,
        day_of_year: i32,
        time: f64,
    },
    /// Move the clock on (or back) by this many seconds.
    Slew(f64),
    /// Use this weather file (None: the map's default).
    Weather(Option<String>),
    /// Host: the timetable tours the other players drive (line, tour; lower case).
    Tours(hashbrown::HashSet<(String, String)>),
}

/// What the game keeps about a session besides the transport: the other players' buses,
/// the chat, the sync tables and the host's world as far as it was taken over.
#[derive(Default)]
pub struct LanGame {
    pub remotes: hashbrown::HashMap<u32, RemoteVehicle>,
    pub chat: Chat,
    /// The chat's lines (name, text) not yet handed to the plugins (`lan_chat`).
    pub plugin_chat: Vec<(String, String)>,
    /// The shared world: the host's traffic and people (`lan_world`).
    pub world: crate::lan_world::LanWorld,
    /// The password `/admin` was given, until the server's challenge comes (`admin`).
    pub admin_password: Option<String>,
    tables: hashbrown::HashMap<PathBuf, Arc<SyncTable>>,
    /// The welcome whose world was taken over (`LanSession::welcomes`).
    adopted: u32,
    /// Host: the tours the other players drive, as last told the timetable.
    tours: hashbrown::HashSet<(String, String)>,
    /// Clock difference still to catch up (s).
    slew: f64,
    status_t: f32,
    log_t: f32,
    /// Seconds the session has been ticked.
    clock: f32,
    /// How far our clock was off the host's at its last clock message (s).
    last_gap: Option<f64>,
    last_sent: u64,
    last_received: u64,
    /// The host's weather last taken over from its clock messages.
    weather_seen: Option<String>,
    /// Vehicles another player drives that could not be made here, and when that was
    /// tried: tried again only after a while (every frame, a server read a big add-on bus
    /// it could not load over and over and stood still for everybody).
    failed: hashbrown::HashMap<(u32, String), std::time::Instant>,
    /// Our vehicle's variable table (by its program), for `omsi_net::vars`.
    my_vars: Option<(usize, Arc<VarTable>)>,
    vars_log: f32,
}

/// What the frame knows that LAN play needs.
pub struct Frame<'a> {
    pub audio: Option<&'a omsi_audio::AudioEngine>,
    /// Where the listener (camera) is.
    pub listener: Option<DVec3>,
    /// The listener sits in our own bus's cabin: another player's bus is heard through our
    /// own bodywork and glass just as an AI one is (see `SoundSet::set_muffled`).
    pub muffled: bool,
    /// Passengers aboard our bus.
    pub riders: usize,
    /// The game's clock (None: an offscreen client, which keeps the one it started with).
    pub clock: Option<&'a omsi_sim::SimClock>,
    /// The timetable tour we drive, `<line>/<tour>` (the host's timetable leaves it to us).
    pub tour: Option<String>,
    /// We are out of the seat, walking about.
    pub walker: Option<omsi_net::Walker>,
    /// We stand or sit in this player's bus: it is drawn from inside.
    pub inside_of: Option<u32>,
    /// We are holding the bus radio key (map-wide GreenTeaSpeak voice).
    pub radio_keyed: bool,
}

pub(crate) use omsi_lan::data_dir;

pub use omsi_lan::StatusFileGuard;
use omsi_lan::status_path;

fn date_of(clock: &omsi_sim::SimClock) -> String {
    let (d, m) = clock.day_month();
    format!("{:04}-{m:02}-{d:02}", clock.year)
}

/// The name the other players see: `--lan-name` (the launcher passes the driver profile's
/// name), else the personnel file's name, else the computer account's - never a name to
/// type in, and never the bare "Driver" everybody would share.
pub fn player_name(args: &Args) -> String {
    let given = args.lan_name.trim();
    if !given.is_empty() && given != "Driver" {
        return given.to_string();
    }
    if let Some(stem) = args.driver.as_deref().and_then(|d| Path::new(d).file_stem()).map(|s| s.to_string_lossy().to_string()) {
        if !stem.trim().is_empty() && stem != "Driver" {
            return stem;
        }
    }
    // the account's full name on macOS, else its login name
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("id").arg("-F").output() {
        let full = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !full.is_empty() {
            return full;
        }
    }
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| "Driver".into())
}

/// Our world as the other side sees it.
pub fn world_info(args: &Args) -> omsi_net::WorldInfo {
    let clock = crate::start_clock(args);
    omsi_net::WorldInfo {
        map: args.map.replace('\\', "/"),
        date: date_of(&clock),
        time: clock.time,
        weather: args.weather.clone().unwrap_or_default().replace('\\', "/"),
        season: args.season.clone().unwrap_or_default(),
    }
}

pub use omsi_lan::gateway::{close_public_gateway, offers, open_public_gateway, server_offers, take_local_admin, tunnel_url, update_server_info, update_server_players, update_server_world};
use omsi_lan::gateway::{ask_server_offers, ws_came_back, ws_join_target};

/// The buses a joining player may choose, on the gateway's status page (see
/// `omsi_lan::gateway::publish_vehicles`): `only` when a server's list says, else every bus
/// installed under `root`.
pub fn publish_vehicles(root: PathBuf, only: Vec<String>) {
    omsi_lan::gateway::publish_vehicles(only, move || crate::menu::Menu::new(&root, "").vehicles.into_iter().map(|v| v.1).collect());
}

/// Host or join as the command line says (`--lan-host`, `--lan-join`). A session that
/// cannot be started is reported and the game runs on alone. A joining player waits
/// briefly for the host's welcome, so that its map is loaded with the host's world.
pub fn start(args: &Args) -> Option<LanSession> {
    // (every bus offered until the server joined now says otherwise, see `ServerOffers`)
    let offers_session = omsi_lan::gateway::new_offers_session();
    let world = world_info(args);
    let session = match (&args.lan_host, &args.lan_join) {
        (Some(port), _) => {
            // 0 = the default port, or the next free one when another session runs here
            let (p, try_next) = if *port == 0 {
                (omsi_net::DEFAULT_PORT, true)
            } else {
                (*port, false)
            };
            match LanSession::host(p, &player_name(args), world, try_next) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("LAN: cannot host on port {p}: {e}");
                    write_failure(&format!("cannot host on port {p}: {e}"));
                    None
                }
            }
        }
        (None, Some(target)) => {
            // a server's address (https://…, a trycloudflare name): over a WebSocket
            let direct = match omsi_net::ws::ws_url(target) {
                Some(url) => match ws_join_target(&url) {
                    Ok(local) => {
                        ask_server_offers(target, offers_session);
                        local
                    }
                    Err(e) => {
                        log::warn!("LAN: cannot reach '{target}': {e}");
                        write_failure(&format!("cannot reach '{target}': {e}"));
                        return None;
                    }
                },
                // (an address over UDP: a server there answers at its web port too; a code
                // or a search has nobody to ask)
                None => {
                    ask_server_offers(target, offers_session);
                    target.clone()
                }
            };
            match LanSession::join(&direct, &player_name(args), world, Duration::from_secs(3)) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("LAN: cannot join '{target}': {e}");
                    write_failure(&format!("cannot join '{target}': {e}"));
                    None
                }
            }
        }
        _ => None,
    };
    let mut session = session?;
    if let Some(c) = session.code() {
        log::info!(
            "LAN: other players join with the code {}  (or by an address: {})",
            c.encode(),
            host_addresses(c.port)
                .iter()
                .map(|(label, addr, _)| format!("{label} {addr}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for a in omsi_net::addrs::local_addresses() {
            log::info!(
                "LAN: address {} on {} ({}{})",
                a.ip,
                a.interface,
                a.label(),
                if a.kind.reachable() { "" } else { ", not offered: nobody else reaches it" }
            );
        }
    }
    if session.role == Role::Client {
        // the hello names the bus we are about to drive (the state says when it stands)
        let planned = Pose {
            bus: args.bus.clone().unwrap_or_default().replace('\\', "/"),
            ..Default::default()
        };
        let t0 = Instant::now();
        while t0.elapsed() < HOST_ANSWER_WAIT && !session.connected && session.rejected.is_none() {
            session.tick(0.02, &planned);
            std::thread::sleep(Duration::from_millis(20));
        }
        // nobody answered at the addresses of the code: the host's tunnel, if it posted one
        if session.welcome.is_none() && session.rejected.is_none() && args.lan_join.as_deref().map(|t| omsi_net::ws::ws_url(t).is_none()).unwrap_or(false) {
            if let Some(url) = omsi_net::SessionCode::decode(args.lan_join.as_deref().unwrap_or("")).ok().and_then(|c| omsi_net::bridge::lookup_tunnel(c.session)) {
                log::info!("LAN: no answer at the code's addresses; through the host's tunnel {url}");
                if let Some(ws) = omsi_net::ws::ws_url(&url) {
                    match ws_join_target(&ws).and_then(|local| LanSession::join(&local, &player_name(args), world_info(args), Duration::from_secs(3))) {
                        Ok(mut s2) => {
                            let t1 = Instant::now();
                            while t1.elapsed() < WELCOME_WAIT * 2 && !s2.connected && s2.rejected.is_none() {
                                s2.tick(0.02, &planned);
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            session = s2;
                        }
                        Err(e) => log::warn!("LAN: the tunnel {url} did not answer: {e}"),
                    }
                }
            }
        }
        match (&session.welcome, &session.rejected) {
            (Some(_), _) => log::info!("LAN: welcome after {:.2} s", t0.elapsed().as_secs_f32()),
            (None, Some(why)) => log::warn!("LAN: turned away: {why}"),
            (None, None) => log::info!(
                "LAN: no welcome within {:.0} s; the host's world is taken over when it comes",
                HOST_ANSWER_WAIT.as_secs_f32()
            ),
        }
    }
    if session.role == Role::Host && args.server.is_none() {
        // the way in over the internet that always works (see `open_public_gateway`)
        // (its day and time as it starts: a friend joining by the code chooses a duty of that
        // day's timetable)
        let info = omsi_net::ws::ServerInfo { name: format!("{}'s game", player_name(args)), map: args.map.clone(), max_players: 16, version: env!("CARGO_PKG_VERSION").into(), time: args.time.clone(), date: args.date.clone().unwrap_or_default(), ..Default::default() };
        open_public_gateway(&session, info, 0, true);
        publish_vehicles(args.root.clone(), Vec::new());
    }
    write_status(&session, &Default::default(), None);
    Some(session)
}

pub use omsi_lan::LEFT_SERVER;

/// A joining game the server sent or turned away: the reason, once (the game then ends and the
/// launcher shows "Disconnected from the server" with it). None for a host, or while it may play.
pub fn turned_away(lan: &LanSession) -> Option<String> {
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let why = lan.turned_away.clone().filter(|_| lan.role == Role::Client)?;
    if SAID.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    log::warn!("{LEFT_SERVER}{why}");
    Some(why)
}

/// The host's mods (see `lan_mods`): the host serves them on its session's port number
/// (TCP), a joining player fetches what it lacks before its world is made, and says how
/// far it has got in the status the launcher shows.
/// What of our command line decides the mods we share as the host.
fn mod_source(args: &Args) -> crate::lan_mods::ModSource {
    crate::lan_mods::ModSource { root: args.root.clone(), map: args.map.clone(), bus: args.bus.clone(), weather: args.weather.clone() }
}

pub fn share_mods(args: &mut Args, lan: &mut LanSession) {
    match lan.role {
        Role::Host => {
            if let Some(port) = lan.local_addr().map(|a| a.port()) {
                crate::lan_mods::serve(port, lan.session, &mod_source(args));
            }
        }
        Role::Client => {
            let Some(mut host) = lan.host.filter(|_| lan.welcome.is_some()) else {
                log::info!("LAN mods: no host to ask yet (its mods are not fetched)");
                return;
            };
            let note = |lan: &mut LanSession, text: String| {
                lan.warnings.retain(|w| !w.starts_with("Host's mods"));
                lan.warnings.push(text);
                write_status(lan, &Default::default(), None);
            };
            note(lan, "Host's mods: looking what is needed...".into());
            let session = lan.session;
            // the host's TCP port is not always reachable (a router forwards the UDP session
            // only, or we came in over a WebSocket): the files go through its tunnel then
            let joined_url = omsi_lan::gateway::joined_url();
            let direct = joined_url.is_none() && std::net::TcpStream::connect_timeout(&host, Duration::from_secs(3)).is_ok();
            if !direct {
                let base = joined_url.or_else(|| omsi_net::bridge::lookup_tunnel(session).and_then(|u| omsi_net::ws::ws_url(&u)));
                match base.map(|b| format!("{}/tcp", b.strip_suffix("/ws").unwrap_or(&b))) {
                    Some(tcp_url) => match omsi_net::ws::tcp_forward(&tcp_url) {
                        Ok(local) => {
                            log::info!("LAN mods: the host's TCP port is out of reach; through {tcp_url}");
                            host = local;
                        }
                        Err(e) => log::warn!("LAN mods: {e}"),
                    },
                    None => {
                        note(lan, "Host's mods could not be downloaded; playing with what is installed here".into());
                        return;
                    }
                }
            }
            // (in the background: the session is kept alive meanwhile - a join held for
            // minutes was dropped by the host and came in later without the mods)
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            let mut fmap = args.map.clone();
            let worker = std::thread::spawn(move || {
                let mut last = Instant::now() - Duration::from_secs(1);
                let r = crate::lan_mods::fetch(&mut fmap, host, session, &mut |done, total, file| {
                    if last.elapsed() > Duration::from_millis(500) {
                        last = Instant::now();
                        let pct = if total > 0 { done * 100 / total } else { 100 };
                        let _ = tx.send(format!("Host's mods: {pct}% ({:.0} of {:.0} MB) {file}", done as f64 / 1e6, total as f64 / 1e6));
                    }
                });
                (r, fmap)
            });
            let planned = Pose { bus: args.bus.clone().unwrap_or_default().replace('\\', "/"), ..Default::default() };
            while !worker.is_finished() {
                lan.keepalive(0.05, &planned);
                if let Some(line) = rx.try_iter().last() {
                    log::info!("LAN mods: {line}");
                    note(lan, line);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let (result, map) = worker.join().unwrap_or_else(|_| (Err("the download stopped".into()), args.map.clone()));
            args.map = map;
            match result {
                Ok(r) => {
                    let text = if r.fetched > 0 {
                        format!("Host's mods: {} files ({:.0} MB) fetched (kept for the next join)", r.fetched, r.bytes as f64 / 1e6)
                    } else {
                        "Host's mods: everything the host uses is installed here".to_string()
                    };
                    note(lan, text);
                    let mine = world_info(args);
                    lan.set_world(mine);
                }
                Err(e) => {
                    log::warn!("LAN mods: {e}");
                    note(lan, format!("Host's mods could not be fetched: {e}"));
                }
            }
        }
    }
}

/// Do `work` (loading a world, which may take minutes on a big map) while the session is
/// kept on another thread: a host answers the players who join meanwhile, a joining game
/// tells its host that it is still there. A dedicated server loads the whole map before its
/// first frame, and nobody could join it until it was done.
pub fn answering_while<T>(lan: &mut Option<LanSession>, bus: Option<&str>, work: impl FnOnce() -> T) -> T {
    let Some(session) = lan.take() else {
        return work();
    };
    let planned = Pose {
        bus: bus.unwrap_or_default().replace('\\', "/"),
        ..Default::default()
    };
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        let keeper = s.spawn(|| {
            let mut session = session;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                session.keepalive(0.05, &planned);
                std::thread::sleep(Duration::from_millis(50));
            }
            session
        });
        let out = work();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        match keeper.join() {
            Ok(session) => *lan = Some(session),
            Err(_) => log::warn!("LAN: the session stopped while the world loaded"),
        }
        out
    })
}

/// A joining player plays on the host's map when it is installed here, whatever map was
/// chosen before joining. The host's list of mods also brings it, but that list may not
/// come in time: listing a big add-on map (thousands of objects to find and hash) took the
/// host longer than the joining game waited, and the player was left on its own map, where
/// nobody ever met them.
pub fn take_host_map(args: &mut Args, lan: &mut LanSession) {
    if lan.role != Role::Client {
        return;
    }
    let Some(theirs) = lan.welcome.as_ref().map(|w| w.world.map.trim().replace('\\', "/")) else {
        return;
    };
    // (the same map, however its path was written: from its `maps/` folder on - a map
    // chosen as a whole path, or out of an archive, is still the host's one, and taking it
    // for another map dropped the line and tour chosen: everybody drove without a duty)
    let norm = |s: &str| {
        let s = s.trim().replace('\\', "/").to_ascii_lowercase();
        match s.rfind("maps/") {
            Some(k) => s[k..].to_string(),
            None => s,
        }
    };
    if theirs.is_empty() || norm(&theirs) == norm(&args.map) {
        return;
    }
    if crate::lan_mods::refuse_path(&theirs).is_some() || !norm(&theirs).starts_with("maps/") {
        return;
    }
    match omsi_cfg::find_in_roots(&theirs) {
        Some(_) => {
            log::info!("LAN: the session is on the host's map {theirs} (not {}); the duty chosen is tried there", args.map);
            args.map = theirs;
            // (the entry point and depot file chosen were those of the other map; the line and
            // tour are tried on the host's - taken away here, the player drove free without a
            // word, Luc; not there, the game says so: "No duty")
            args.entry = 0;
            args.spawn = None;
            args.hof = None;
            let mine = world_info(args);
            lan.set_world(mine);
        }
        None => {
            let line = format!("the host plays on {theirs}, which is not installed here - install that map to meet the others");
            log::warn!("LAN: {line}");
            if !lan.warnings.contains(&line) {
                lan.warnings.push(line);
            }
        }
    }
}

/// A date `YYYY-MM-DD` as (year, day of year).
fn parse_date(s: &str) -> Option<(i32, i32)> {
    let v: Vec<i32> = s.split('-').filter_map(|x| x.trim().parse().ok()).collect();
    (v.len() == 3).then(|| {
        let mut c = omsi_sim::SimClock::default();
        c.set_date(v[0], v[1], v[2]);
        (c.year, c.day_of_year)
    })
}

/// The host's clock as a `SimClock` now (its date moved on past midnight).
fn host_clock_now(h: &omsi_net::HostClock) -> Option<omsi_sim::SimClock> {
    let (year, day_of_year) = parse_date(&h.world.date)?;
    let mut c = omsi_sim::SimClock {
        year,
        day_of_year,
        time: h.world.time,
        ..Default::default()
    };
    c.advance(h.at.elapsed().as_secs_f32() * h.speed as f32);
    Some(c)
}

/// The host's time of day now as `HH:MM:SS` (a joining player's duty is placed with it).
pub fn host_time_now(lan: &LanSession) -> Option<String> {
    let w = lan.welcome.as_ref()?;
    let c = host_clock_now(&omsi_net::HostClock { world: w.world.clone(), at: w.at, speed: 1.0 })?;
    let t = c.time.rem_euclid(86400.0) as u32;
    Some(format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60))
}

/// A weather file the host uses, if this machine has it (None: the map's default).
fn host_weather(args: &Args, weather: &str) -> Result<Option<String>, String> {
    let w = weather.trim();
    if w.is_empty() {
        return Ok(None);
    }
    // the natural model and the cycle are made on each machine: no file to have
    if crate::weather_model::is_natural(Some(w)) || w == "cycle" || crate::weather_setup::custom_weather(Some(w)).is_some() {
        return Ok(Some(w.to_string()));
    }
    // a METAR report's values: made into a weather here, no file and no sync of our own
    if w.starts_with(crate::weather_setup::REPORT) {
        return if crate::weather_setup::from_report(w).is_some() {
            Ok(Some(w.to_string()))
        } else {
            Err(format!("the host's weather {w} cannot be read here"))
        };
    }
    let path = omsi_cfg::resolve_path(&args.root, w);
    let inside = !w.contains("..") && !w.starts_with('/') && !w.contains(':');
    if inside && omsi_cfg::vfs::is_file(&path) {
        Ok(Some(w.to_string()))
    } else {
        Err(format!("the host's weather {w} is not installed here"))
    }
}

/// Whether the host's weather `host` (from its clock message) is news to a client whose own
/// is `local` and that last took up `seen`. Empty is a weather too: the map's default.
fn runtime_weather_changed(host: &str, local: Option<&str>, seen: Option<&str>) -> bool {
    let norm = |s: &str| s.trim().replace('\\', "/").to_ascii_lowercase();
    let host_norm = norm(host);
    host_norm != norm(local.unwrap_or("")) && seen.is_none_or(|last| norm(last) != host_norm)
}

/// Take the host's world (a joining player, before the map is loaded): the date, the time
/// of day as the host's clock has it now, the weather and the season go into the
/// arguments the world is made from. The map stays ours (a different one is warned about).
pub fn adopt_host_world(args: &mut Args, lan: &mut LanSession, game: &mut LanGame) {
    if lan.role != Role::Client {
        return;
    }
    let Some(w) = lan.welcome.clone() else { return };
    game.adopted = lan.welcomes;
    let hw = &w.world;
    if let Some(c) = host_clock_now(&omsi_net::HostClock {
        world: hw.clone(),
        at: w.at,
        speed: lan.clock_speed,
    }) {
        args.date = Some(date_of(&c));
        args.day_of_year = None;
        args.time = format!(
            "{:02}:{:02}:{:05.2}",
            (c.time / 3600.0) as i32,
            ((c.time % 3600.0) / 60.0) as i32,
            c.time % 60.0
        );
    }
    let mut warnings = Vec::new();
    match host_weather(args, &hw.weather) {
        Ok(wt) => args.weather = wt,
        Err(e) => warnings.push(e),
    }
    args.season = (!hw.season.trim().is_empty()).then(|| hw.season.trim().to_string());
    log::info!(
        "LAN: taking the host's world: {} {} weather {} season {}",
        args.date.as_deref().unwrap_or(""),
        args.time,
        args.weather.as_deref().unwrap_or("(the map's)"),
        args.season.as_deref().unwrap_or("(by date)")
    );
    let mut mine = world_info(args);
    if warnings.is_empty() {
        // compared as the host spelled it
        mine.weather = hw.weather.clone();
    }
    lan.set_world(mine);
    for line in warnings {
        log::warn!("LAN: {line}");
        lan.warnings.push(line);
    }
}

/// The host's world after the map was loaded (a late welcome, a reconnect): the clock is
/// set and the weather changed; the season needs the map loaded again, which is said.
fn adopt_at_runtime(
    args: &Args,
    lan: &mut LanSession,
    game: &mut LanGame,
    world: Option<&World>,
    clock: &omsi_sim::SimClock,
) -> Vec<WorldUpdate> {
    let mut out = Vec::new();
    let Some(w) = lan.welcome.clone() else {
        return out;
    };
    game.adopted = lan.welcomes;
    let hw = w.world.clone();
    if let Some(c) = host_clock_now(&omsi_net::HostClock {
        world: hw.clone(),
        at: w.at,
        speed: lan.clock_speed,
    }) {
        log::info!(
            "LAN: the host's clock: {} {:02}:{:02} (ours was {} {:02}:{:02})",
            date_of(&c),
            (c.time / 3600.0) as i32,
            ((c.time % 3600.0) / 60.0) as i32,
            date_of(clock),
            (clock.time / 3600.0) as i32,
            ((clock.time % 3600.0) / 60.0) as i32
        );
        out.push(WorldUpdate::Clock {
            year: c.year,
            day_of_year: c.day_of_year,
            time: c.time,
        });
    }
    let mut warnings = Vec::new();
    let mut weather = args.weather.clone();
    match host_weather(args, &hw.weather) {
        Ok(wt) => {
            let norm = |s: &Option<String>| {
                s.as_deref()
                    .unwrap_or("")
                    .trim()
                    .replace('\\', "/")
                    .to_ascii_lowercase()
            };
            if norm(&wt) != norm(&args.weather) {
                out.push(WorldUpdate::Weather(wt.clone()));
            }
            weather = wt;
        }
        Err(e) => warnings.push(e),
    }
    // the season is in the textures that are loaded: say so when the host's differs
    if let Some(world) = world {
        let mut theirs = args.clone();
        theirs.date = Some(hw.date.clone());
        theirs.day_of_year = None;
        theirs.season = (!hw.season.trim().is_empty()).then(|| hw.season.trim().to_string());
        theirs.weather = weather.clone();
        let want = crate::season_folder(&theirs, &world.global).1;
        if want != omsi_texture::season_folder() {
            warnings.push(format!("the host's season ({}) differs from the one loaded here ({}) - start the game again to see it", want.as_deref().unwrap_or("summer"), omsi_texture::season_folder().as_deref().unwrap_or("summer")));
        }
    }
    let mut mine = world_info(args);
    mine.weather = if warnings.iter().any(|w| w.contains("weather")) {
        mine.weather
    } else {
        hw.weather.clone()
    };
    lan.set_world(mine);
    for line in warnings {
        log::warn!("LAN: {line}");
        lan.warnings.push(line);
    }
    out
}

/// Days since 1 January of year 0 (for comparing two dates).
fn day_number(year: i32, day_of_year: i32) -> i64 {
    let y = year as i64 - 1;
    let before = if year > 0 {
        365 * y + y / 4 - y / 100 + y / 400 + 366
    } else {
        0
    };
    before + day_of_year as i64
}

/// How far our clock is behind the host's (s, negative: ahead).
fn clock_gap(host: &omsi_sim::SimClock, mine: &omsi_sim::SimClock) -> f64 {
    (day_number(host.year, host.day_of_year) - day_number(mine.year, mine.day_of_year)) as f64
        * 86400.0
        + host.time
        - mine.time
}

fn write_failure(msg: &str) {
    let Some(p) = status_path() else { return };
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let v = serde_json::json!({ "pid": std::process::id(), "role": "none", "error": msg, "updated": now_secs() });
    let _ = std::fs::write(p, serde_json::to_vec_pretty(&v).unwrap_or_default());
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The addresses other players may join this host at, best first: (what network it is,
/// `ip:port`, kind key) - a VPN's ("Hamachi 25.34.223.28:27015") before the LAN's.
pub fn host_addresses(port: u16) -> Vec<(&'static str, String, &'static str)> {
    omsi_net::addrs::joinable_addresses()
        .into_iter()
        .map(|a| (a.label(), format!("{}:{port}", a.ip), a.kind.key()))
        .collect()
}

/// Write the status file the launcher reads (written atomically: a new file renamed over
/// the old one).
fn write_status(lan: &LanSession, game: &LanGame, player: Option<&Player>) {
    let Some(p) = status_path() else { return };
    let players: Vec<serde_json::Value> = lan
        .peers()
        .map(|peer| {
            let d = player.filter(|_| peer.pose.has_vehicle()).map(|pl| relative_position(&pl.vehicle, &peer.pose));
            serde_json::json!({ "id": peer.pose.id, "name": peer.pose.name, "bus": peer.pose.bus, "line": peer.pose.line, "destination": peer.pose.destination, "passengers": peer.pose.passengers, "where": d, "drawn": game.remotes.contains_key(&peer.pose.id) })
        })
        .collect();
    let code = lan.code();
    let v = serde_json::json!({
        "pid": std::process::id(),
        "role": if lan.role == Role::Host { "host" } else { "client" },
        "name": lan.my_name,
        "code": code.as_ref().map(|c| c.encode()),
        "address": code.as_ref().map(|c| format!("{}:{}", c.ip(), c.port)).or_else(|| lan.host.map(|h| h.to_string())),
        "addresses": code.as_ref().map(|c| host_addresses(c.port).into_iter().map(|(label, addr, kind)| serde_json::json!({ "label": label, "address": addr, "kind": kind })).collect::<Vec<_>>()).unwrap_or_default(),
        "trying": (lan.role == Role::Client && lan.host.is_none()).then(|| lan.candidates.iter().map(|a| a.to_string()).collect::<Vec<_>>()),
        "port": lan.local_addr().map(|a| a.port()),
        "session": omsi_net::session_hex(lan.session),
        "tunnel": tunnel_url(),
        "connected": lan.connected,
        "rejected": lan.rejected,
        "warnings": lan.warnings,
        "host_name": lan.welcome.as_ref().map(|w| w.host_name.clone()),
        "map": lan.world.map,
        "players": players,
        "chat": game.chat.lines.iter().rev().take(CHAT_LINES).rev().cloned().collect::<Vec<_>>(),
        "updated": now_secs(),
    });
    // (the disk is written on a thread of its own: a slow disk or a virus scanner held the
    // frame every two seconds)
    static WRITER: std::sync::OnceLock<Option<std::sync::mpsc::Sender<(PathBuf, Vec<u8>)>>> = std::sync::OnceLock::new();
    let writer = WRITER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, Vec<u8>)>();
        std::thread::Builder::new()
            .name("lan status".into())
            .spawn(move || {
                while let Ok(mut job) = rx.recv() {
                    // only the newest of a backlog is worth writing
                    while let Ok(newer) = rx.try_recv() {
                        job = newer;
                    }
                    let (p, bytes) = job;
                    if let Some(dir) = p.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let tmp = p.with_extension("json.tmp");
                    if std::fs::write(&tmp, bytes).is_ok() {
                        let _ = std::fs::rename(&tmp, &p);
                    }
                }
            })
            .ok()
            .map(|_| tx)
    });
    if let Some(tx) = writer {
        let _ = tx.send((p, serde_json::to_vec_pretty(&v).unwrap_or_default()));
    }
}

/// "35 m ahead", "1.2 km behind", "12 m to the left" - where `pose` is seen from our bus.
fn relative_position(me: &omsi_sim::VehicleInstance, pose: &Pose) -> String {
    let d = DVec3::new(pose.x, pose.y, pose.z) - me.position;
    let dist = d.truncate().length();
    let h = me.heading.to_radians();
    let (fwd, right) = (d.x * h.sin() + d.y * h.cos(), d.x * h.cos() - d.y * h.sin());
    let side = if fwd.abs() >= right.abs() {
        if fwd >= 0.0 {
            "ahead"
        } else {
            "behind"
        }
    } else if right >= 0.0 {
        "to the right"
    } else {
        "to the left"
    };
    if dist >= 1000.0 {
        format!("{:.1} km {side}", dist / 1000.0)
    } else {
        format!("{dist:.0} m {side}")
    }
}

/// The other players for the navigator and the city map (#1011, #1080); `my_id` is ours.
pub fn nav_players(game: &LanGame, my_id: u32) -> Vec<crate::navigator::NavPlayer> {
    let bus_of = |id: u32| game.remotes.get(&id).map(|r| (r.vehicle.position, r.vehicle.heading));
    let mut out: Vec<_> = game.remotes.values().filter_map(|r| nav_player(&r.last, &r.name, (r.vehicle.position, r.vehicle.heading), my_id, bus_of)).collect();
    // (always in the same order: the tags of two players close together do not swap)
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A player as the maps show them: with the bus they drive (`bus`: as drawn here), else on
/// foot where they walk - riding in a third player's bus, with that bus (`bus_of`; the
/// walker's own point lags behind it), and not at all in ours, which is our own arrow.
fn nav_player(pose: &Pose, name: &str, bus: (DVec3, f64), my_id: u32, bus_of: impl Fn(u32) -> Option<(DVec3, f64)>) -> Option<crate::navigator::NavPlayer> {
    let (position, heading) = match pose.walker {
        None => bus,
        Some(w) => match w.aboard {
            Some(a) if a.owner == my_id => return None,
            Some(a) => bus_of(a.owner).unwrap_or((DVec3::new(w.x, w.y, w.z), w.heading as f64)),
            None => (DVec3::new(w.x, w.y, w.z), w.heading as f64),
        },
    };
    let name = if name.trim().is_empty() { format!("player {}", pose.id) } else { name.trim().to_string() };
    Some(crate::navigator::NavPlayer { position, heading, name })
}

/// The other players' name tags, as ETS2 has them: the name over the bus's roof and a
/// small line under it (line and destination, how far away), fading out beyond 300 m.
/// Screen positions in physical pixels of a `width` x `height` picture (on a triple-screen
/// rig, of the three panels side by side). `speaks` tells who talks in the voice chat now (by
/// name and id): "speaking" under their name. `on_radio` who keys the bus radio: "radio"
/// instead, and the tag stays visible across the map.
pub fn name_tags(
    game: &LanGame,
    cam: &omsi_render::Camera,
    width: f32,
    height: f32,
    rig: Option<&omsi_render::TripleScreen>,
    speaks: &dyn Fn(&str, u32) -> bool,
    on_radio: &dyn Fn(&str, u32) -> bool,
) -> Vec<((f32, f32), String, String, f32)> {
    let views: Vec<_> = if let Some(rig) = rig {
        rig.views(cam, width as u32, height as u32)
            .iter()
            .map(|v| {
                let vp = v.projection
                    * glam::Mat4::look_to_rh(glam::Vec3::ZERO, v.camera.forward(), v.camera.up());
                (vp, v.viewport[0] as f32, v.viewport[2] as f32)
            })
            .collect()
    } else {
        vec![(
            cam.view_proj(width / height.max(1.0), cam.position),
            0.0,
            width,
        )]
    };
    let mut tags = Vec::new();
    for r in game.remotes.values() {
        let v = &r.vehicle;
        // the roof: the bounding box's top when the bus says, else a bus's height
        let top = v.ty.def.bounding_box.map(|b| (b[5] + b[2] * 0.5) as f64).filter(|t| *t > 1.0).unwrap_or(3.4);
        // (a player out of the seat: the name over their head, none over the bus)
        let p = match r.last.walker {
            Some(w) => DVec3::new(w.x, w.y, w.z + 2.15),
            None => v.position + DVec3::new(0.0, 0.0, top + 0.6),
        };
        let d = (p - cam.position).length();
        let radio = on_radio(&r.name, r.last.id) || r.last.radio_keyed;
        // (a radio key across the map: the tag still shows who is transmitting)
        if d > 450.0 && !radio {
            continue;
        }
        let Some((screen_x, y)) = views.iter().find_map(|(vp, offset, panel_width)| {
            let c = *vp * (p - cam.position).as_vec3().extend(1.0);
            if c.w <= 0.1 {
                return None;
            }
            let (x, y) = (c.x / c.w, c.y / c.w);
            let limit = if rig.is_some() { 1.0 } else { 1.2 };
            (x.abs() <= limit && y.abs() <= 1.2)
                .then_some((offset + (x + 1.0) * 0.5 * panel_width, y))
        }) else {
            continue;
        };
        let name = if r.name.trim().is_empty() {
            format!("player {}", r.last.id)
        } else {
            r.name.trim().to_string()
        };
        let pose = &r.last;
        let mut sub = match (pose.line.trim().is_empty(), pose.destination.trim().is_empty()) {
            (false, false) => format!("{} {}", pose.line.trim(), pose.destination.trim()),
            (true, false) => pose.destination.trim().to_string(),
            (false, true) => format!("line {}", pose.line.trim()),
            _ => String::new(),
        };
        if d > 25.0 {
            let dist = if d >= 1000.0 { format!("{:.1} km", d / 1000.0) } else { format!("{:.0} m", d) };
            sub = if sub.is_empty() { dist } else { format!("{sub} | {dist}") };
        }
        if radio {
            sub = if sub.is_empty() { omsi_ui::tr("radio").into_owned() } else { format!("{} | {sub}", omsi_ui::tr("radio")) };
        } else if speaks(&r.name, r.last.id) {
            sub = if sub.is_empty() { omsi_ui::tr("speaking").into_owned() } else { format!("{} | {sub}", omsi_ui::tr("speaking")) };
        }
        let alpha = if radio { 1.0 } else { (1.0 - ((d as f32 - 300.0) / 150.0)).clamp(0.0, 1.0) };
        tags.push(((screen_x, (1.0 - y) * 0.5 * height), name, sub, alpha));
    }
    tags
}

/// A vehicle file as the other players' games find it: relative to whichever content root
/// holds it (the original installation or the mods folder).
fn content_relative(path: &Path, root: &Path) -> String {
    let mut roots = omsi_cfg::content_roots();
    roots.push(root.to_path_buf());
    crate::lan_world::relative_to_roots(path, &roots).unwrap_or_default()
}

/// The paint scheme `--paint` picked, by name.
pub fn paint_name(args: &Args, ty: &omsi_sim::VehicleType) -> String {
    // (none chosen: the model's own textures, sent as no name)
    let Some(p) = args.paint.as_deref() else {
        return String::new();
    };
    ty.paint_schemes
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(p))
        .or_else(|| {
            p.parse::<usize>()
                .ok()
                .and_then(|i| ty.paint_schemes.get(i))
        })
        .map(|s| s.name.clone())
        .unwrap_or_default()
}


/// Line and destination as our bus shows them: the duty's, or what the IBIS was set to.
fn line_and_destination(p: &Player, duty: Option<(&str, &str)>) -> (String, String) {
    if let Some((l, d)) = duty {
        return (l.to_string(), d.to_string());
    }
    let v = &p.vehicle;
    let line = v
        .var("IBIS_Linie_Complex")
        .filter(|x| *x >= 100.0)
        .map(|x| ((x / 100.0) as i32).to_string())
        .unwrap_or_default();
    let dest = match (v.var("IBIS_TerminusIndex"), v.host.hof.as_ref()) {
        // terminus code 0 is the blank display ("Leerfeld")
        (Some(i), Some(h)) if i >= 0.0 => h
            .termini
            .get(i as usize)
            .filter(|t| t.code != 0)
            .map(|t| {
                // (its sign's first line; a blank one names nothing on the others' side)
                t.strings
                    .iter()
                    .find(|s| !s.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| t.texture_id.clone())
            })
            .unwrap_or_default(),
        _ => String::new(),
    };
    (line, dest.trim().to_string())
}

/// What a vehicle's indicators show: 0 off, 1 left, 2 right, 3 hazard - the indicator
/// switch where the script has one, else the lamps (as they are lit just now).
pub(crate) fn indicator(v: &omsi_sim::VehicleInstance) -> u8 {
    let on = |n: &str| v.var(n).unwrap_or(0.0) > 0.5;
    if on("lights_sw_warnblinker") {
        3
    } else {
        match v.var("lights_sw_blinker") {
            Some(s) if (0.5..2.5).contains(&s) => s.round() as u8,
            _ => match (on("lights_blinker_l"), on("lights_blinker_r")) {
                (true, true) => 3,
                (true, false) => 1,
                (false, true) => 2,
                _ => 0,
            },
        }
    }
}

/// What we send about our own bus.
pub fn my_pose(
    game: &mut LanGame,
    p: Option<&Player>,
    args: &Args,
    duty: Option<(&str, &str)>,
    riders: usize,
) -> Pose {
    let Some(p) = p else { return Pose::default() };
    let v = &p.vehicle;
    let table = sync_table(game, v);
    let val = |n: &str| v.var(n).unwrap_or(0.0);
    let on = |n: &str| val(n) > 0.5;
    let get = |id: VarId| v.state.vars.get(id as usize).copied().unwrap_or(0.0);
    let rpm = table.engine_n.map(get).unwrap_or(0.0);
    let mut flags = omsi_net::FLAG_VEHICLE;
    for (bit, set) in [
        (omsi_net::FLAG_ENGINE, on("engine_on") || rpm > 100.0),
        (
            omsi_net::FLAG_ELECTRICS,
            on("elec_busbar_main") || on("elec_busbar_avail"),
        ),
        (
            omsi_net::FLAG_HORN,
            table.horn.iter().any(|id| get(*id) > 0.5),
        ),
        (omsi_net::FLAG_BRAKE, on("lights_brems")),
        (omsi_net::FLAG_REVERSE, on("lights_rueckfahr")),
        (omsi_net::FLAG_FOG, on("lights_nebelschluss")),
        (
            omsi_net::FLAG_KNEELING,
            ["bremse_kneeling", "vdv_kneel", "ecas_kneel", "kneeling"]
                .iter()
                .any(|n| on(n)),
        ),
        (
            omsi_net::FLAG_WIPERS,
            on("wiperrunning") || on("wiper_running"),
        ),
        (omsi_net::FLAG_STOP_BRAKE, on("bremse_halte")),
    ] {
        if set {
            flags |= bit;
        }
    }
    let head = if on("lights_fern") {
        3
    } else if on("lights_abbl") || on("lights_main") || v.var("Spot_Select").is_some_and(|s| s >= 0.0) {
        // (a mod bus names its lamps its own way; its selected spotlight is the dipped
        // beam every script sets for the renderer - without it the others saw such a bus
        // drive through the night with its headlights off)
        2
    } else if on("lights_stand") {
        1
    } else {
        0
    };
    let blinker = indicator(v);
    let fp = footprint_of(v, BUS_BOX);
    let bb = v.ty.def.bounding_box.unwrap_or(BUS_BOX);
    let h = v.heading.to_radians();
    let box_offset = ((fp.x - v.position.x) * h.sin() + (fp.y - v.position.y) * h.cos()) as f32;
    let (line, destination) = line_and_destination(p, duty);
    let texts = display_texts(v);
    let freetex = freetex_values(v);
    Pose {
        id: 0,
        name: String::new(),
        bus: content_relative(&v.ty.def.path, &args.root),
        // (the scheme the bus wears now: one picked in the game's menu after the start too)
        paint: match v.host.paint_scheme {
            Some(Some(i)) => {
                v.ty.paint_schemes
                    .get(i)
                    .map(|s| s.name.clone())
                    .unwrap_or_default()
            }
            Some(None) => String::new(),
            None => paint_name(args, &v.ty),
        },
        line,
        destination,
        tour: String::new(),
        texts,
        freetex,
        figure: p.driver.as_ref().map(|d| content_relative(&d.human_type().def.path, &args.root)).unwrap_or_default(),
        length: fp.length.max(bb[1]),
        width: bb[0],
        box_offset,
        table: table.hash,
        x: v.position.x,
        y: v.position.y,
        z: v.position.z,
        heading: v.heading as f32,
        pitch: v.pitch,
        bank: v.bank,
        speed_kmh: v.physics.velocity_kmh(),
        steer_deg: v.physics.steer_deg,
        flags,
        head,
        interior: (v.interior_light() * 3.0).round() as u8,
        blinker,
        rpm,
        throttle: v.physics.controls.throttle,
        brake: v.physics.controls.brake,
        passengers: riders as u32,
        doors: table.doors.iter().map(|id| get(*id)).collect(),
        suspension: v
            .physics
            .wheels
            .iter()
            .flat_map(|a| a.iter().map(|w| w.suspension))
            .collect(),
        rear: v
            .trailers
            .iter()
            .map(|t| PartPose {
                x: t.position.x,
                y: t.position.y,
                z: t.position.z,
                heading: t.heading as f32,
            })
            .collect(),
        lamps: table.lamps.iter().map(|(_, id)| get(*id)).collect(),
        switches: table.switches.iter().map(|(_, id)| get(*id)).collect(),
        values: table.values.iter().map(|(_, id)| get(*id)).collect(),
        walker: None,
        sent_ms: 0,
        radio_keyed: false,
    }
}

/// The string variables the vehicle's `[texttexture]` displays show: the model's, in its
/// order, then those of its rear sections' displays that the model has not (a side
/// destination sign on an articulated bus's rear section stood blank for the others).
fn display_vars(v: &omsi_sim::VehicleInstance) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let models = std::iter::once(&v.ty.model).chain(v.trailers.iter().map(|t| &t.ty.model));
    for t in models.flat_map(|m| m.text_textures.iter()) {
        let n = t.variable.trim();
        if !names.iter().any(|x| x.eq_ignore_ascii_case(n)) {
            names.push(n.to_string());
        }
    }
    names.truncate(omsi_net::MAX_TEXTS);
    names
}

/// The strings the vehicle's `[texttexture]` displays show (see [`display_vars`]).
fn display_texts(v: &omsi_sim::VehicleInstance) -> Vec<String> {
    display_vars(v)
        .iter()
        .map(|n| v.ty.program.str_var(n).and_then(|i| v.state.str_vars.get(i as usize)).cloned().unwrap_or_default())
        .collect()
}

/// The `[matl_freetex]` string variables of a vehicle type, sorted by name (both games
/// agree on the order): the picture a slot shows is the file one of them names - a roller
/// blind's number, a sign. Worked out by the sender's scripts alone.
fn freetex_names(ty: &omsi_sim::VehicleType) -> Vec<String> {
    let mut names: Vec<String> = ty
        .model
        .meshes
        .iter()
        .flat_map(|m| m.materials.iter().filter_map(|mat| mat.freetex.as_ref().map(|f| f.1.trim().to_string())))
        .filter(|n| ty.program.str_var(n).is_some())
        .collect();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names.dedup_by_key(|n| n.to_ascii_lowercase());
    names.truncate(omsi_net::MAX_FREETEX);
    names
}

/// What the vehicle's `[matl_freetex]` variables hold now (see [`freetex_names`]).
fn freetex_values(v: &omsi_sim::VehicleInstance) -> Vec<String> {
    freetex_names(&v.ty)
        .iter()
        .map(|n| v.ty.program.str_var(n).and_then(|i| v.state.str_vars.get(i as usize)).cloned().unwrap_or_default())
        .collect()
}

/// Show another player's `[matl_freetex]` pictures on their bus here: their copy's scripts
/// do not run the roller blind, which stood empty (the line number never showed).
fn show_freetex(v: &mut omsi_sim::VehicleInstance, values: &[String]) {
    if values.is_empty() {
        return;
    }
    for (name, value) in freetex_names(&v.ty).iter().zip(values) {
        if let Some(i) = v.ty.program.str_var(name) {
            if let Some(s) = v.state.str_vars.get_mut(i as usize) {
                if s != value {
                    *s = value.clone();
                }
            }
        }
    }
}

/// Show another player's display texts on their bus here (same files, same order).
fn show_display_texts(v: &mut omsi_sim::VehicleInstance, texts: &[String]) {
    if texts.is_empty() {
        return;
    }
    for (name, text) in display_vars(v).iter().zip(texts) {
        if let Some(i) = v.ty.program.str_var(name) {
            if let Some(s) = v.state.str_vars.get_mut(i as usize) {
                if s != text {
                    *s = text.clone();
                }
            }
        }
    }
}

/// The host's own vehicles for its lists: its bus and the AI traffic around.
fn host_footprints(
    p: Option<&Player>,
    traffic: Option<&crate::traffic::Traffic>,
) -> Vec<Footprint> {
    let mut out = Vec::new();
    if let Some(p) = p {
        out.push(footprint_of(&p.vehicle, BUS_BOX));
    }
    if let Some(t) = traffic {
        out.extend(t.cars.iter().map(|c| footprint_of(&c.vehicle, CAR_BOX)));
    }
    out
}


/// Move our bus off anything standing where it spawned: along the road it stands on, the
/// nearest free place behind or in front of the others (behind first at equal distance),
/// or - off the road - along its own heading and then in rows beside it. Returns the
/// distance moved (0 when the spawn was free, None when no free place was found).
pub fn clear_spawn(
    p: &mut Player,
    occupied: &[Footprint],
    world: &World,
    net: Option<&Network>,
) -> Option<f64> {
    let me = footprint_of(&p.vehicle, BUS_BOX);
    if !blocked(&me, occupied) {
        log::info!(
            "LAN: our spawn at ({:.1}, {:.1}) is free ({} vehicle(s) nearby)",
            p.vehicle.position.x,
            p.vehicle.position.y,
            occupied.len()
        );
        return Some(0.0);
    }
    // the origin sits this far from the box centre (along, across)
    let v = &p.vehicle;
    let h0 = v.heading.to_radians();
    let dc = DVec3::new(me.x, me.y, me.z) - v.position;
    let (c_along, c_across) = (
        dc.x * h0.sin() + dc.y * h0.cos(),
        dc.x * h0.cos() - dc.y * h0.sin(),
    );
    let statics = v.collision.clone();
    let free_at = |centre: DVec3, heading: f64| -> Option<DVec3> {
        let fp = Footprint {
            x: centre.x,
            y: centre.y,
            z: centre.z,
            heading: heading as f32,
            length: me.length,
            width: me.width,
        };
        if blocked(&fp, occupied) {
            return None;
        }
        let z = world.ground_height(centre.x, centre.y)?;
        if let Some(cw) = statics.as_ref() {
            let mut b = obb(&fp, false);
            b.z0 = z + 0.3;
            b.z1 = z + 3.0;
            if cw.hit(&b).is_some() {
                return None;
            }
        }
        let h = heading.to_radians();
        Some(DVec3::new(
            centre.x - c_along * h.sin() - c_across * h.cos(),
            centre.y - c_along * h.cos() + c_across * h.sin(),
            z,
        ))
    };
    // the lanes: the traffic network (linked), or the world's own list
    let world_lanes;
    let (lanes, prev): (&[Lane], &[Vec<usize>]) = match net {
        Some(n) if !n.lanes.is_empty() => (&n.lanes, &n.prev),
        _ => {
            world_lanes = world.lanes.lock().clone();
            (&world_lanes, &[])
        }
    };
    let centre = DVec3::new(me.x, me.y, me.z);
    let mut best_lane: Option<(usize, f32, f64, bool)> = None;
    for (i, l) in lanes.iter().enumerate() {
        if l.kind != LaneKind::Street || l.points.len() < 2 {
            continue;
        }
        // a lane cannot reach further from its start than its own length
        if (l.points[0] - centre).truncate().length() > l.length() as f64 + 10.0 {
            continue;
        }
        if let Some((s, d)) = l.nearest_point(centre) {
            if d > 6.0 {
                continue;
            }
            let same_way = heading_gap(l.at(s).1, v.heading as f32) < 60.0;
            // a lane running our way beats a closer one running the other way
            let score = d + if same_way { 0.0 } else { 4.0 };
            if best_lane.map(|b| score < b.2).unwrap_or(true) {
                best_lane = Some((i, s, score, same_way));
            }
        }
    }
    let mut placed: Option<(DVec3, f64, f64)> = None;
    // the lane and the place on it where the bus went (for its rear sections)
    let mut on_lane: Option<(usize, f32, f32)> = None;
    if let Some((li, s, _, same_way)) = best_lane {
        let sign = if same_way { 1.0 } else { -1.0 };
        // how much the road turns under the whole bus there: a straight box on a bend
        // stands with its ends off the road (an articulated bus with its rear section on
        // the verge), so a straight stretch a little further off is taken first
        let reach = me.length * 0.5 + 1.0;
        let bend = |c: f32| -> f32 {
            match (along(lanes, prev, li, s, c + reach), along(lanes, prev, li, s, c - reach)) {
                (Some(a), Some(b)) => heading_gap(a.1, b.1),
                _ => 90.0,
            }
        };
        let mut fallback: Option<(DVec3, f64, f64, f32)> = None;
        'search: for k in 1..=90 {
            let d = k as f32 * 1.5;
            for dd in [-d, d] {
                if let Some((pos, lh)) = along(lanes, prev, li, s, dd * sign) {
                    let heading = if same_way {
                        lh as f64
                    } else {
                        lh as f64 + 180.0
                    }
                    .rem_euclid(360.0);
                    if let Some(origin) = free_at(pos, heading) {
                        if bend(dd * sign) < 12.0 {
                            placed = Some((origin, heading, dd as f64));
                            on_lane = Some((li, s, dd * sign));
                            break 'search;
                        }
                        if fallback.is_none() {
                            fallback = Some((origin, heading, dd as f64, dd * sign));
                        }
                    }
                }
            }
            // a bend-free place more than 60 m away: the nearest free one on the bend
            if d > 60.0 && fallback.is_some() {
                break;
            }
        }
        if placed.is_none() {
            if let Some((o, h, m, at)) = fallback {
                placed = Some((o, h, m));
                on_lane = Some((li, s, at));
            }
        }
    }
    if placed.is_none() {
        // off the road (a depot yard): along our heading, then rows beside it
        let (fwd, right) = (
            DVec3::new(h0.sin(), h0.cos(), 0.0),
            DVec3::new(h0.cos(), -h0.sin(), 0.0),
        );
        'rows: for row in [0.0, 1.0, -1.0, 2.0, -2.0] {
            let side = right * row * (me.width as f64 + GAP_ACROSS + 0.5);
            for k in 0..=40 {
                let d = k as f64 * 1.5;
                for dd in [-d, d] {
                    if let Some(origin) = free_at(centre + side + fwd * dd, v.heading) {
                        placed = Some((origin, v.heading, dd));
                        break 'rows;
                    }
                }
            }
        }
    }
    let Some((origin, heading, moved)) = placed else {
        log::warn!(
            "LAN: no free place found near our spawn at ({:.1}, {:.1}); staying there",
            v.position.x,
            v.position.y
        );
        return None;
    };
    let from = p.vehicle.position;
    p.vehicle.position = origin;
    p.vehicle.heading = heading;
    if let Some(rb) = p.vehicle.rigid.as_mut() {
        rb.place(origin, heading);
    }
    for t in p.vehicle.trailers.iter_mut() {
        t.realign();
    }
    // on the road: every rear section's turning axle on the lane behind its coupling, so
    // that it follows the bend instead of standing straight out onto the verge
    if let (Some((li, s, at)), true) = (on_lane, !p.vehicle.trailers.is_empty()) {
        let back = if heading_gap(lanes[li].at(s).1, heading as f32) < 90.0 { -1.0 } else { 1.0 };
        let (lead_pos, lead_rot) = (p.vehicle.position, p.vehicle.body_rotation());
        let fwd = DVec3::new(heading.to_radians().sin(), heading.to_radians().cos(), 0.0);
        // (the first rear section; a second one lines up behind it as it moves)
        if let Some(t) = p.vehicle.trailers.first_mut() {
            let c = t.coupling_point(lead_pos, lead_rot);
            let len = t.pivot_length() as f64;
            // walk back along the lane until the axle is the part's length from the coupling
            let mut k = 0.0f32;
            while k < 60.0 {
                if let Some((q, _)) = along(lanes, prev, li, s, at + back * k) {
                    let q = DVec3::new(q.x, q.y, c.z);
                    if (q - c).truncate().length() >= len && (q - c).dot(fwd) < 0.0 {
                        let dir = (q - c).truncate().normalize_or_zero();
                        t.place_pivot(c + DVec3::new(dir.x, dir.y, 0.0) * len);
                        break;
                    }
                }
                k += 0.25;
            }
        }
    }
    for _ in 0..3 {
        p.vehicle.update(1.0 / 30.0);
    }
    log::info!(
        "LAN: {} vehicle(s) stand at our spawn; moved our bus {:.1} m {} to ({:.1}, {:.1}, {:.1}) heading {:.0} (from ({:.1}, {:.1}))",
        occupied.iter().filter(|o| (DVec3::new(o.x, o.y, o.z) - from).truncate().length() < 30.0).count(),
        moved.abs(),
        if moved < 0.0 { "back" } else { "forward" },
        origin.x,
        origin.y,
        origin.z,
        heading,
        from.x,
        from.y
    );
    Some(moved)
}

/// A joining player's game, right after its bus was put at the entry point: ask the host
/// what stands there, wait briefly for the answer and move the bus clear.
/// Ask the host what stands at our spawn and wait up to `wait` for the answer (the window
/// waits for none: the frame loop moves the bus when the list comes, see `frame`; an
/// offscreen run has no frames to spare and waits).
pub fn settle_spawn(
    lan: &mut LanSession,
    game: &mut LanGame,
    p: &mut Player,
    args: &Args,
    world: &World,
    net: Option<&Network>,
    wait: Duration,
) {
    if lan.role != Role::Client || lan.spawn_settled {
        return;
    }
    lan.request_near(footprint_of(&p.vehicle, BUS_BOX));
    if wait.is_zero() {
        return;
    }
    let mine = my_pose(game, Some(p), args, None, 0);
    let t0 = Instant::now();
    while t0.elapsed() < wait && lan.near.is_none() && lan.rejected.is_none() {
        lan.tick(0.02, &mine);
        std::thread::sleep(Duration::from_millis(20));
    }
    match lan.near.clone() {
        Some(near) => {
            log::info!("LAN: the host's list of what stands at our spawn after {:.2} s", t0.elapsed().as_secs_f32());
            clear_spawn(p, &near, world, net);
            lan.spawn_settled = true;
        }
        None => log::info!("LAN: the host did not say within {:.0} s what stands at our spawn; the bus is moved when it does (if it has not been driven by then)", wait.as_secs_f32()),
    }
}

/// The vehicle file a remote pose names, on this machine. The path comes off the network:
/// it must be a plain relative `.bus` / `.ovh` path (`omsi_net::vehicle_path`), found under
/// one of our content roots, and a regular file of a sane size - never a device such as
/// `/dev/zero`, never a file elsewhere on the disk.
fn remote_bus_file(args: &Args, bus: &str) -> Result<PathBuf, String> {
    let rel = omsi_net::vehicle_path(bus)
        .ok_or_else(|| "not a vehicle file inside a content folder".to_string())?;
    let mut path = omsi_cfg::resolve_path(&args.root, &rel);
    if !omsi_cfg::vfs::is_file(&path) {
        // the other side's own place for it ("Archives/<pack>.zip/Vehicles/…", a content
        // folder of its own): the same vehicle under any content root of ours
        let lower = rel.to_ascii_lowercase().replace('\\', "/");
        if let Some(k) = lower.find("vehicles/") {
            if let Some((_, p)) = omsi_cfg::find_in_roots(&rel.replace('\\', "/")[k..]) {
                path = p;
            }
        }
    }
    let mut roots = omsi_cfg::content_roots();
    roots.push(args.root.clone());
    if !roots.iter().any(|r| path.starts_with(r)) {
        return Err("not inside a content folder".into());
    }
    // (a file in an archive or among the host's mods has no metadata of its own)
    let len = match omsi_cfg::vfs::file_size(&path) {
        Some(len) => len,
        None => {
            let md = std::fs::metadata(&path).map_err(|e| e.to_string())?;
            if !md.is_file() {
                return Err("not a file".into());
            }
            md.len()
        }
    };
    if len > MAX_VEHICLE_FILE {
        return Err(format!("{len} bytes is too much for a vehicle file"));
    }
    Ok(path)
}

/// Load the type a remote player drives, or a stand-in: ours, or on a server the first of
/// the buses its `vehicles` list allows. A bus the list does not allow is not loaded at all
/// (a player joining with another than the server offers).
fn remote_type(
    args: &Args,
    pose: &Pose,
    player: Option<&Player>,
) -> Option<(Arc<omsi_sim::VehicleType>, bool)> {
    let allowed = crate::server::SERVER_VEHICLES.get().filter(|l| !l.is_empty());
    let loaded = if allowed.is_none_or(|l| crate::server::allows(l, &pose.bus)) {
        remote_bus_file(args, &pose.bus).and_then(|path| omsi_sim::VehicleType::load(&args.root, &path).map_err(|e| e.to_string()))
    } else {
        Err("the server does not offer it".to_string())
    };
    match loaded {
        Ok(t) => Some((Arc::new(t), false)),
        Err(e) => {
            log::warn!("LAN: player {} drives {:?}, which cannot be loaded here ({e}); showing a stand-in", pose.id, pose.bus);
            if let Some(p) = player {
                return Some((p.vehicle.ty.clone(), true));
            }
            let first = allowed.and_then(|l| l.first())?;
            let path = remote_bus_file(args, first).ok()?;
            omsi_sim::VehicleType::load(&args.root, &path).ok().map(|t| (Arc::new(t), true))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn new_remote(
    game: &mut LanGame,
    args: &Args,
    pose: &Pose,
    player: Option<&Player>,
    world: &World,
    r: &Renderer,
    scene: &mut Scene,
    clock: Option<&omsi_sim::SimClock>,
) -> Option<RemoteVehicle> {
    let (ty, stand_in) = remote_type(args, pose, player)?;
    let mut host =
        omsi_sim::VehicleHost::new(clock.cloned().unwrap_or_else(|| crate::start_clock(args)));
    host.font_lib = Some(world.fonts.clone());
    let hof = crate::find_hof(args, world, &ty);
    host.hof = hof.clone();
    let scheme = if pose.paint.is_empty() {
        None
    } else {
        ty.paint_schemes
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(&pose.paint))
    };
    host.paint_scheme = Some(scheme);
    let mut vehicle = omsi_sim::VehicleInstance::new(ty.clone(), host);
    vehicle.ground = None;
    if !ty.model.text_textures.is_empty() {
        vehicle.init_text_textures(&mut world.fonts.lock(), &|p| {
            omsi_texture::decode_file(p)
                .ok()
                .map(|i| (i.width, i.height, i.rgba))
        });
    }
    vehicle.apply_paint_vars(scheme);
    let render = world.add_vehicle_shared(r, scene, &ty, scheme, None);
    // the coupled sections of an articulated bus
    let mut trailer_renders = Vec::new();
    let mut lead = ty.clone();
    let mut lead_rev = false;
    let mut seen = vec![lead.def.path.clone()];
    for _ in 0..crate::spawn::MAX_COUPLED_PARTS {
        let Some((path, rev)) = crate::spawn::next_coupled(&lead.def, lead_rev, true) else {
            break;
        };
        if crate::spawn::chain_has_part(&seen, &path) {
            break;
        }
        match omsi_sim::VehicleType::load(&args.root, &path) {
            Ok(t) => {
                let t = Arc::new(t);
                trailer_renders.push(world.add_vehicle_shared(
                    r,
                    scene,
                    &t,
                    crate::spawn::part_scheme(&ty, scheme, &t),
                    Some(&render),
                ));
                vehicle.attach_trailer_ex(t.clone(), rev);
                seen.push(path);
                lead = t;
                lead_rev = rev;
            }
            Err(e) => {
                log::warn!("LAN: rear section {}: {e}", path.display());
                break;
            }
        }
    }
    // (with its rear sections: theirs are in the table too)
    let table = sync_table(game, &vehicle);
    vehicle.position = DVec3::new(pose.x, pose.y, pose.z);
    vehicle.heading = pose.heading as f64;
    let rear: Vec<(DVec3, f64)> = pose
        .rear
        .iter()
        .map(|q| (DVec3::new(q.x, q.y, q.z), q.heading as f64))
        .collect();
    let matched = pose.table == table.hash;
    log::info!(
        "LAN: drawing player {} '{}' in {}{} at ({:.1}, {:.1}) heading {:.0}{}; {}",
        pose.id,
        pose.name,
        pose.bus,
        if pose.paint.is_empty() {
            String::new()
        } else {
            format!(" ({})", pose.paint)
        },
        pose.x,
        pose.y,
        pose.heading,
        if trailer_renders.is_empty() {
            String::new()
        } else {
            format!(", {} rear section(s)", trailer_renders.len())
        },
        if matched {
            "lamps, switches and sound values follow theirs".to_string()
        } else {
            format!("their vehicle files differ from ours (table {:08X}, ours {:08X}): lights and doors only", pose.table, table.hash)
        }
    );
    Some(RemoteVehicle {
        vehicle,
        render,
        trailer_renders,
        name: pose.name.clone(),
        table,
        sounds: Vec::new(),
        inside_sounds: None,
        target: (DVec3::new(pose.x, pose.y, pose.z), pose.heading as f64),
        rear,
        pose_seen: (DVec3::new(pose.x, pose.y, pose.z), Instant::now()),
        doors: pose.doors.clone(),
        suspension: pose.suspension.clone(),
        values: pose.values.clone(),
        odometer: 0.0,
        horn: false,
        hof,
        shown: (String::new(), String::new()),
        stand_in,
        last: pose.clone(),
        made_as: (pose.bus.clone(), pose.paint.clone()),
        driver: None,
        driver_for: None,
        track: Default::default(),
        vars: None,
        synced: Default::default(),
        synced_strings: Default::default(),
        smooth: Default::default(),
    })
}

impl RemoteVehicle {
    /// Take in the states that came (with when they arrived).
    fn take_samples(&mut self, history: &std::collections::VecDeque<(Instant, Pose)>) {
        self.track.take(history);
    }

    /// Their state as it is drawn now (see `omsi_lan::remote::PoseTrack::interpolated`).
    fn interpolated(&mut self) -> Option<Pose> {
        self.track.interpolated()
    }
}

/// A remote vehicle has gone: its sounds stop and its instances go back to the world.
fn release(
    r: &Renderer,
    scene: &mut Scene,
    world: &World,
    audio: Option<&omsi_audio::AudioEngine>,
    rv: RemoteVehicle,
) {
    if let Some(a) = audio {
        for mut s in rv.sounds {
            s.stop_all(a);
        }
    }
    if let Some(mut d) = rv.driver {
        d.hide(r, scene);
    }
    world.release_vehicle(r, scene, rv.render);
    for t in rv.trailer_renders {
        world.release_vehicle(r, scene, t);
    }
}

/// The rear sections of another game's vehicle where that game has them (`rear`: position
/// and heading of each), each leaning so that it meets the part in front at their joint
/// (`Trailer::set_remote_pose`).
fn place_remote_rear(v: &mut omsi_sim::VehicleInstance, rear: &[(DVec3, f64)]) {
    let mut lead = (v.position, v.body_rotation());
    for (i, cur) in rear.iter().enumerate() {
        let Some(t) = v.trailers.get_mut(i) else { break };
        let c = t.coupling_point(lead.0, lead.1);
        t.set_remote_pose(cur.0, cur.1, c);
        lead = (t.position, t.body_rotation());
    }
}

fn ease_heading(from: f64, to: f64, k: f64) -> f64 {
    let dh = (to - from + 540.0).rem_euclid(360.0) - 180.0;
    if dh.abs() > 90.0 {
        to
    } else {
        from + dh * k
    }
}

/// `cur` glides towards `want` (same length afterwards).
fn glide(cur: &mut Vec<f32>, want: &[f32], k: f32) {
    if cur.len() != want.len() {
        *cur = want.to_vec();
        return;
    }
    for (c, w) in cur.iter_mut().zip(want) {
        *c += (w - *c) * k;
    }
}

/// One frame of another player's bus: the pose glides towards the latest state, the AI
/// scripts run with that state as their inputs, the table's variables are pinned to the
/// sender's values, and the horn is pressed and let go as theirs is.
fn drive_remote(rv: &mut RemoteVehicle, pose: &Pose, dt: f32, exact: bool) {
    rv.name = pose.name.clone();
    rv.vehicle.pitch = pose.pitch;
    rv.vehicle.bank = pose.bank;
    rv.odometer += pose.speed_kmh / 3.6 * dt;
    if exact {
        // interpolated between their states (`RemoteVehicle::interpolated`): where it is
        // drawn is where it is, the doors and the moving parts as they were then
        rv.vehicle.position = DVec3::new(pose.x, pose.y, pose.z);
        rv.vehicle.heading = pose.heading as f64;
        rv.target = (rv.vehicle.position, rv.vehicle.heading);
        rv.pose_seen = (rv.vehicle.position, Instant::now());
        rv.rear = pose.rear.iter().map(|q| (DVec3::new(q.x, q.y, q.z), q.heading as f64)).collect();
        place_remote_rear(&mut rv.vehicle, &rv.rear);
        rv.doors = pose.doors.clone();
        rv.suspension = pose.suspension.clone();
        rv.values = pose.values.clone();
    } else {
        // (an older game's states carry no clock:) where the bus is now, not where it was
        // when the pose left the other game: carried on along its heading at its speed for
        // the time since the pose came, plus the way over the network
        let at = DVec3::new(pose.x, pose.y, pose.z);
        if at != rv.pose_seen.0 {
            rv.pose_seen = (at, Instant::now());
        }
        let age = (rv.pose_seen.1.elapsed().as_secs_f64() + 0.04).min(0.4);
        let h = (pose.heading as f64).to_radians();
        let ahead = DVec3::new(h.sin(), h.cos(), 0.0) * (pose.speed_kmh as f64 / 3.6) * age;
        rv.target = (at + ahead, pose.heading as f64);
        // glide towards it: twenty updates a second, drawn at frame rate
        let k = (dt * 20.0).min(1.0);
        let kd = k as f64;
        let d = rv.target.0 - rv.vehicle.position;
        rv.vehicle.position += if d.length() > 25.0 { d } else { d * kd };
        rv.vehicle.heading = ease_heading(rv.vehicle.heading, rv.target.1, kd);
        // the rear sections where the other game has them
        rv.rear.resize(pose.rear.len(), (DVec3::ZERO, 0.0));
        for (cur, q) in rv.rear.iter_mut().zip(pose.rear.iter()) {
            // (carried on with the bus: a rear section follows the same way)
            let tgt = (DVec3::new(q.x, q.y, q.z) + ahead, q.heading as f64);
            let jump = cur.0 == DVec3::ZERO || (tgt.0 - cur.0).length() > 25.0;
            cur.0 = if jump { tgt.0 } else { cur.0 + (tgt.0 - cur.0) * kd };
            cur.1 = if jump { tgt.1 } else { ease_heading(cur.1, tgt.1, kd) };
        }
        place_remote_rear(&mut rv.vehicle, &rv.rear);
        glide(&mut rv.doors, &pose.doors, k);
        glide(&mut rv.suspension, &pose.suspension, k);
        glide(&mut rv.values, &pose.values, k);
    }
    // the wheels' travel (kneeling, bumps) as theirs
    let mut it = rv.suspension.iter();
    for axle in rv.vehicle.physics.wheels.iter_mut() {
        for w in axle.iter_mut() {
            w.suspension = it.next().copied().unwrap_or(0.0);
        }
    }
    // the horn: pressed and let go as theirs (the scripts fire its sound events)
    let horn = pose.flags & omsi_net::FLAG_HORN != 0;
    if horn != rv.horn {
        rv.horn = horn;
        rv.vehicle.trigger(if horn { "horn" } else { "horn_off" });
    }
    let t = rv.table.clone();
    let matched = pose.table == t.hash && !rv.stand_in;
    let engine = pose.flags & omsi_net::FLAG_ENGINE != 0;
    let electrics = pose.flags & omsi_net::FLAG_ELECTRICS != 0;
    // the AI scripts' inputs: an engine that is off is off, lights as a level (0.5
    // parking lights, 1 dipped, 2 main beam), the saloon lights, the pedals
    let mut inputs: Vec<(VarId, f32)> = Vec::with_capacity(8);
    let active = if engine || (electrics && t.engine_n.is_some()) {
        1.0
    } else {
        -1.0
    };
    inputs.extend(t.ai_engine.map(|id| (id, active)));
    inputs.extend(
        t.ai_light
            .map(|id| (id, [0.0, 0.5, 1.0, 2.0][pose.head.min(3) as usize])),
    );
    inputs.extend(
        t.ai_interior
            .map(|id| (id, (pose.interior > 0) as i32 as f32)),
    );
    inputs.extend(t.throttle.map(|id| (id, pose.throttle)));
    inputs.extend(t.brake.map(|id| (id, pose.brake)));
    // what is pinned to their values: the engine speed, the doors, and - for the same
    // vehicle files - the lamps, switches and sound and moving-part values
    let mut pinned: Vec<(VarId, f32)> =
        Vec::with_capacity(8 + t.lamps.len() + t.switches.len() + t.values.len());
    if !t.values.iter().any(|v| Some(v.1) == t.engine_n) {
        pinned.extend(t.engine_n.map(|id| (id, pose.rpm)));
    }
    pinned.extend(t.doors.iter().zip(&rv.doors).map(|(id, v)| (*id, *v)));
    if matched
        && pose.lamps.len() == t.lamps.len()
        && pose.switches.len() == t.switches.len()
        && rv.values.len() == t.values.len()
    {
        pinned.extend(
            t.lamps
                .iter()
                .zip(&pose.lamps)
                .map(|((_, id), v)| (*id, *v)),
        );
        pinned.extend(
            t.switches
                .iter()
                .zip(&pose.switches)
                .map(|((_, id), v)| (*id, *v)),
        );
        pinned.extend(
            t.values
                .iter()
                .zip(&rv.values)
                .map(|((_, id), v)| (*id, *v)),
        );
    }
    let doors_open = rv.doors.iter().any(|d| *d > 0.2);
    let frame = omsi_sim::vehicle::AiFrame {
        speed: pose.speed_kmh / 3.6,
        odometer: rv.odometer,
        steer_deg: pose.steer_deg,
        blinker: pose.blinker as i32,
        brake: pose.flags & omsi_net::FLAG_BRAKE != 0 || pose.brake > 0.1,
        lights: pose.head >= 2,
        at_station: if doors_open { 1 } else { -1 },
        // Their stop's side is not on the wire: their doors are pinned to the openings
        // they send (see `doors` above), so which side the player's own script opened is
        // already in those values - the frame only runs the AI half of the script.
        at_station_side: 0.0,
        priority_warning: false,
        engine_off: false,
    };
    // and every other variable of theirs, as their scripts have it (`omsi_net::vars`)
    pinned.extend(rv.synced.iter().filter(|(id, _)| !rv.smooth.contains(*id)).map(|(id, v)| (*id as VarId, *v)));
    rv.vehicle.update_ai_with(dt, &frame, &inputs, &pinned);
    for (id, text) in &rv.synced_strings {
        if let Some(s) = rv.vehicle.state.str_vars.get_mut(*id as usize) {
            if s != text {
                s.clone_from(text);
            }
        }
    }
    rv.last = pose.clone();
}

/// Our vehicle's variables to the others, and theirs taken into the copies drawn here.
fn sync_vars(lan: &mut LanSession, game: &mut LanGame, player: Option<&Player>, dt: f32) {
    if omsi_cfg::flags::OMSI_NO_VAR_SYNC.is_set() {
        return;
    }
    if let Some(p) = player {
        let program = &p.vehicle.ty.program;
        let key = Arc::as_ptr(program) as usize;
        if game.my_vars.as_ref().map(|m| m.0) != Some(key) {
            let t = var_table(program);
            log::info!("LAN: {} variables and {} strings of our bus go to the others (table {:08x})", t.floats.len(), t.strings.len(), t.hash);
            game.my_vars = Some((key, Arc::new(t)));
        }
        let t = game.my_vars.as_ref().map(|m| m.1.clone()).expect("just made");
        let vars = &p.vehicle.state.vars;
        let floats: Vec<f32> = t.floats.iter().map(|id| vars.get(*id as usize).copied().unwrap_or(0.0)).collect();
        let strs = &p.vehicle.state.str_vars;
        let strings: Vec<String> = t.strings.iter().map(|id| strs.get(*id as usize).cloned().unwrap_or_default()).collect();
        lan.send_vars(t.hash, &t.floats, &floats, &t.strings, &strings, dt);
        // `OMSI_DEBUG_VAR_SYNC=<variable>`: ours every two seconds, theirs as it came
        if let Some(name) = omsi_cfg::flags::OMSI_DEBUG_VAR_SYNC.var() {
            game.vars_log += dt;
            if game.vars_log > 2.0 {
                game.vars_log = 0.0;
                log::info!("LAN vars: ours {name} = {:?}", p.vehicle.var(&name));
                for (id, rv) in &game.remotes {
                    let theirs = rv.vehicle.ty.program.var(&name).and_then(|k| rv.synced.get(&(k as u16)).copied());
                    log::info!("LAN vars: player {id}'s {name} = {theirs:?} ({} variables, {} strings taken)", rv.synced.len(), rv.synced_strings.len());
                }
            }
        }
    }
    for v in lan.take_vars() {
        let Some(rv) = game.remotes.get_mut(&v.id) else { continue };
        if rv.stand_in {
            continue;
        }
        if rv.vars.is_none() {
            let t = var_table(&rv.vehicle.ty.program);
            let tbl = &rv.table;
            rv.smooth = tbl.lamps.iter().chain(&tbl.switches).chain(&tbl.values).map(|(_, id)| *id as u16).chain(tbl.doors.iter().map(|id| *id as u16)).chain(tbl.engine_n.map(|id| id as u16)).collect();
            rv.vars = Some(t);
        }
        let Some(t) = rv.vars.as_ref() else { continue };
        if t.hash != v.table {
            continue;
        }
        let (nf, ns) = (rv.vehicle.state.vars.len(), rv.vehicle.state.str_vars.len());
        for (id, x) in v.floats {
            if (id as usize) < nf && x.is_finite() {
                rv.synced.insert(id, x);
            }
        }
        for (id, s) in v.strings {
            if (id as usize) < ns {
                rv.synced_strings.insert(id, s);
            }
        }
    }
}

/// The outside sounds of a remote bus: made when it comes into earshot, dropped beyond.
fn sound_remote(
    rv: &mut RemoteVehicle,
    audio: Option<&omsi_audio::AudioEngine>,
    listener: Option<DVec3>,
    muffled: bool,
    inside: bool,
) {
    let fired: Vec<String> = std::mem::take(&mut rv.vehicle.host.fired_triggers);
    let fired_files: Vec<(String, String)> =
        std::mem::take(&mut rv.vehicle.host.fired_file_triggers);
    let (Some(audio), Some(at)) = (audio, listener) else {
        return;
    };
    // Riding in it: its whole `[sound]` heard from inside, as our own bus is heard from its
    // cab - not its outside sounds muffled, which a passenger heard as if standing in the
    // street beside it (the exterior engine, the tyres, the doors from outside).
    let interior = inside.then_some(()).and(rv.table.interior.clone());
    if let Some((cfg, dir)) = interior {
        for mut s in rv.sounds.drain(..) {
            s.stop_all(audio);
        }
        if rv.inside_sounds.is_none() && audio.clips_ready(&omsi_audio::SoundSet::clip_paths(&cfg, &dir)) {
            let number = rv.vehicle.number();
            rv.inside_sounds = Some(omsi_audio::SoundSet::new(audio, &cfg.chosen_for(&number), &dir));
        }
        if let Some(ss) = rv.inside_sounds.as_mut() {
            let xf = rv.vehicle.world_transform();
            let v = &rv.vehicle;
            ss.set_inside(true);
            ss.set_muffled(true);
            ss.set_listener_vehicle(true);
            // its own outside sounds come in through what this bus has open - its
            // `Snd_OutsideVol`, not the walker's own bus's (or 1 without one): riding along
            // the exterior engine drowned the cab (#1759)
            omsi_audio::soundset::set_outside_open(Some(v.var("Snd_OutsideVol").unwrap_or(0.0)));
            ss.update(audio, &|n| v.var(n), &xf, &fired);
            for (t, f) in &fired_files {
                ss.play_file_trigger(audio, t, f, &|n| v.var(n), &xf);
            }
        }
        return;
    }
    if let Some(mut s) = rv.inside_sounds.take() {
        s.stop_all(audio);
    }
    let d = (rv.vehicle.position - at).length();
    if d > HEAR_RANGE * 1.2 {
        for mut s in rv.sounds.drain(..) {
            s.stop_all(audio);
        }
        return;
    }
    if rv.sounds.is_empty() && d < HEAR_RANGE && !rv.table.sounds.is_empty() {
        // the clips are read in the background the first time; silent till then
        let ready = rv
            .table
            .sounds
            .iter()
            .map(|(cfg, dir)| (cfg, dir))
            .chain(rv.table.part_sounds.iter().map(|(_, cfg, dir)| (cfg, dir)))
            .all(|(cfg, dir)| audio.clips_ready(&omsi_audio::SoundSet::clip_paths(cfg, dir)));
        if ready {
            let number = rv.vehicle.number();
            rv.sounds = rv
                .table
                .sounds
                .iter()
                .map(|(cfg, dir)| omsi_audio::SoundSet::new_exterior(audio, &cfg.chosen_for(&number), dir))
                .collect();
            // the rear sections' sounds ride on the first set, each at its section
            if let Some(first) = rv.sounds.first_mut() {
                for (k, cfg, dir) in &rv.table.part_sounds {
                    first.add_part(*k, omsi_audio::SoundSet::new_exterior(audio, &cfg.chosen_for(&number), dir));
                }
            }
        }
    }
    let xf = rv.vehicle.world_transform();
    let v = &rv.vehicle;
    for ss in rv.sounds.iter_mut() {
        ss.set_muffled(muffled);
        ss.update(audio, &|n| v.var(n), &xf, &fired);
        ss.update_parts(audio, &|n| v.var(n), &|i| v.trailers.get(i).map(|t| t.world_transform()), &fired);
        for (t, f) in &fired_files {
            ss.play_file_trigger(audio, t, f, &|n| v.var(n), &xf);
        }
    }
}

/// LAN play, once a frame: send our bus, take in the others', answer joining players,
/// place our bus when the host's list arrives late, keep a drawn and heard vehicle for
/// each player, follow the host's world and clock, and collect the chat. Returns what the
/// game has to change about its world.
#[allow(clippy::too_many_arguments)]
pub fn tick(
    lan: &mut LanSession,
    game: &mut LanGame,
    dt: f32,
    args: &Args,
    mut player: Option<&mut Player>,
    world: Option<&World>,
    renderer: Option<&Renderer>,
    mut scene: Option<&mut Scene>,
    mut traffic: Option<&mut crate::traffic::Traffic>,
    mut humans: Option<&mut crate::humans::Humans>,
    view: &mut crate::view_sync::SimView,
    duty: Option<(&str, &str)>,
    frame: &Frame,
) -> Vec<WorldUpdate> {
    let mut updates = Vec::new();
    if lan.role == Role::Client && ws_came_back() {
        lan.rehello();
    }
    let mut mine = my_pose(game, player.as_deref(), args, duty, frame.riders);
    mine.tour = frame.tour.clone().unwrap_or_default();
    mine.walker = frame.walker;
    // (only while driving: on foot or as a passenger the radio key does nothing)
    mine.radio_keyed = frame.radio_keyed && frame.walker.is_none() && mine.has_vehicle();
    if lan.role == Role::Host {
        // the tours the others drive are theirs, not the timetable's
        let tours: hashbrown::HashSet<(String, String)> = lan
            .peers()
            .filter_map(|p| p.pose.tour.split_once('/'))
            .map(|(l, t)| (l.trim().to_lowercase(), t.trim().to_lowercase()))
            .filter(|(l, t)| !l.is_empty() && !t.is_empty())
            .collect();
        if tours != game.tours {
            game.tours = tours.clone();
            updates.push(WorldUpdate::Tours(tours));
        }
        if let Some(c) = frame.clock {
            lan.set_clock(&date_of(c), c.time);
        }
        if !lan.pending_joins().is_empty() {
            lan.set_local_footprints(host_footprints(player.as_deref(), traffic.as_deref()));
            lan.answer_joins();
        }
    }
    let gone = lan.tick(dt, &mine);
    sync_vars(lan, game, player.as_deref(), dt);
    game.world.tick(
        lan,
        dt,
        args,
        world,
        renderer,
        scene.as_deref_mut(),
        traffic.as_deref_mut(),
        humans.as_deref_mut(),
        view,
        player.as_deref().map(|p| p.vehicle.position),
    );
    // the host's world: taken over at a late welcome, the clock kept in step
    if let (Role::Client, Some(clock)) = (lan.role, frame.clock) {
        if lan.welcomes != game.adopted && lan.welcome.is_some() {
            updates.extend(adopt_at_runtime(args, lan, game, world, clock));
            game.slew = 0.0;
            lan.take_host_clock();
        } else if let Some(h) = lan.take_host_clock() {
            // the host changed its weather (its administration): ours follows
            if runtime_weather_changed(&h.world.weather, args.weather.as_deref(), game.weather_seen.as_deref()) {
                game.weather_seen = Some(h.world.weather.clone());
                if let Ok(wt) = host_weather(args, &h.world.weather) {
                    let shown = if h.world.weather.trim().is_empty() { "the map's default" } else { h.world.weather.as_str() };
                    log::info!("LAN: the host's weather is now {shown}");
                    updates.push(WorldUpdate::Weather(wt));
                }
            }
            if let Some(hc) = host_clock_now(&h) {
                let gap = clock_gap(&hc, clock);
                game.last_gap = Some(gap);
                if gap.abs() > CLOCK_JUMP {
                    log::info!(
                        "LAN: our clock is {gap:+.1} s off the host's; set to {} {:02}:{:02}:{:02}",
                        date_of(&hc),
                        (hc.time / 3600.0) as i32,
                        ((hc.time % 3600.0) / 60.0) as i32,
                        (hc.time % 60.0) as i32
                    );
                    updates.push(WorldUpdate::Clock {
                        year: hc.year,
                        day_of_year: hc.day_of_year,
                        time: hc.time,
                    });
                    game.slew = 0.0;
                } else {
                    game.slew = gap;
                }
            }
        }
        if game.slew.abs() > 1.0e-3 {
            // caught up within a few seconds
            let step = game.slew * (dt as f64 * 0.5).min(1.0);
            game.slew -= step;
            updates.push(WorldUpdate::Slew(step));
        }
    }
    // a list of what stands at our spawn that came after the bus was placed: move it only
    // if it has not been driven
    if lan.role == Role::Client && !lan.spawn_settled {
        if let (Some(near), Some(p), Some(world)) = (lan.near.clone(), player.as_deref_mut(), world)
        {
            if p.vehicle.physics.velocity_kmh().abs() < 1.0 {
                clear_spawn(p, &near, world, traffic.map(|t| &t.net));
            } else {
                log::info!(
                    "LAN: the host's list came after we drove off; our bus stays where it is"
                );
            }
            lan.spawn_settled = true;
        }
    }
    // OMSI_LAN_SAY="30=Hallo;45=Tschüss": chat lines said at these seconds of the session
    // (for tests of games that have no keyboard: offscreen runs)
    game.clock += dt;
    if let Some(script) = omsi_cfg::flags::OMSI_LAN_SAY.var() {
        for item in script.split(';') {
            if let Some((at, text)) = item.split_once('=') {
                if at
                    .trim()
                    .parse::<f32>()
                    .map(|t| t <= game.clock && t > game.clock - dt)
                    .unwrap_or(false)
                {
                    chat_send(lan, game, text);
                }
            }
        }
    }
    for e in lan.take_events() {
        if let LanEvent::Chat { name, text, .. } = &e {
            // (for the plugins' `lan_chat`, a few kept while nobody takes them)
            if game.plugin_chat.len() < 32 {
                game.plugin_chat.push((name.clone(), crate::ui::filter_chat(text)));
            }
        }
        game.chat.push(match e {
            LanEvent::Chat { name, text, .. } => format!("{name}: {}", crate::ui::filter_chat(&text)),
            LanEvent::Notice(n) => format!("* {n}"),
        });
    }
    game.status_t -= dt;
    if game.status_t <= 0.0 {
        write_status(lan, game, player.as_deref());
        game.status_t = 2.0;
        // (a friend's game: its status page says its clock and day as they are now - a friend
        // joining by the code chooses a duty for them)
        if lan.role == Role::Host && args.server.is_none() {
            if let Some(c) = frame.clock {
                let players = lan.peers().filter(|p| p.has_info).count();
                update_server_info(players, &crate::schedule::hhmm(c.time.rem_euclid(86400.0)), "", &date_of(c));
            }
        }
    }
    let (Some(w), Some(r), Some(scene)) = (world, renderer, scene) else {
        return updates;
    };
    for id in gone {
        if let Some(rv) = game.remotes.remove(&id) {
            log::info!("LAN: no longer drawing player {id} '{}'", rv.name);
            release(r, scene, w, frame.audio, rv);
        }
    }
    // a player the session forgot (timed out), or one that has no bus any more, is dropped
    // as well
    let known: Vec<u32> = lan
        .peers()
        .filter(|p| !(p.has_pose && !p.pose.has_vehicle()))
        .map(|p| p.pose.id)
        .collect();
    let stale: Vec<u32> = game
        .remotes
        .keys()
        .copied()
        .filter(|id| !known.contains(id))
        .collect();
    for id in stale {
        if let Some(rv) = game.remotes.remove(&id) {
            release(r, scene, w, frame.audio, rv);
        }
    }
    let poses: Vec<Pose> = lan
        .peers()
        .filter(|p| p.has_pose && p.has_info && p.pose.has_vehicle())
        .map(|p| p.pose.clone())
        .collect();
    for pose in poses {
        // another vehicle than before (the player changed buses), or another paint scheme on
        // it: made again
        if game
            .remotes
            .get(&pose.id)
            .map(|rv| rv.made_as.0 != pose.bus || rv.made_as.1 != pose.paint)
            .unwrap_or(false)
        {
            if let Some(rv) = game.remotes.remove(&pose.id) {
                release(r, scene, w, frame.audio, rv);
            }
        }
        if !game.remotes.contains_key(&pose.id) {
            let key = (pose.id, pose.bus.clone());
            if game.failed.get(&key).is_some_and(|t| t.elapsed().as_secs_f32() < 30.0) {
                continue;
            }
            let Some(rv) = new_remote(
                game,
                args,
                &pose,
                player.as_deref(),
                w,
                r,
                scene,
                frame.clock,
            ) else {
                game.failed.insert(key, std::time::Instant::now());
                continue;
            };
            game.failed.remove(&key);
            game.remotes.insert(pose.id, rv);
        }
        let Some(rv) = game.remotes.get_mut(&pose.id) else {
            continue;
        };
        // line and destination on the displays
        let want = (pose.line.clone(), pose.destination.clone());
        if want != rv.shown && !want.1.is_empty() {
            if let Some(i) = rv.vehicle.ty.program.str_var("Linie") {
                rv.vehicle.state.str_vars[i as usize] = want.0.clone();
            }
            crate::schedule::set_ai_destination(
                &mut rv.vehicle,
                rv.hof.as_deref(),
                &want.0,
                &want.1,
                &[],
            );
            log::info!(
                "LAN: player {} '{}' shows {}{}",
                pose.id,
                pose.name,
                if want.0.is_empty() {
                    String::new()
                } else {
                    format!("line {} ", want.0)
                },
                want.1
            );
            rv.shown = want;
        }
        // between their states as they were sent (older games: gliding towards the newest)
        if let Some(peer) = lan.peers().find(|p| p.pose.id == pose.id) {
            rv.take_samples(&peer.history);
        }
        match rv.interpolated() {
            Some(mut ip) => {
                // (who they are, what their bus shows: the newest state's)
                ip.name = pose.name.clone();
                ip.bus = pose.bus.clone();
                ip.table = pose.table;
                ip.line = pose.line.clone();
                ip.destination = pose.destination.clone();
                ip.texts = pose.texts.clone();
                ip.freetex = pose.freetex.clone();
                drive_remote(rv, &ip, dt, true);
            }
            None => drive_remote(rv, &pose, dt, false),
        }
        show_display_texts(&mut rv.vehicle, &pose.texts);
        show_freetex(&mut rv.vehicle, &pose.freetex);
        let inside = frame.inside_of == Some(pose.id);
        sound_remote(rv, frame.audio, frame.listener, frame.muffled, inside);
    }
    // draw them like AI traffic
    for (id, rv) in game.remotes.iter_mut() {
        if rv.driver_for.as_deref() != Some(rv.last.figure.as_str()) && !rv.stand_in {
            rv.driver_for = Some(rv.last.figure.clone());
            if let Some(mut d) = rv.driver.take() {
                d.hide(r, scene);
            }
            // their own figure at the wheel (a figure picked by their id otherwise)
            rv.driver = crate::driver::DriverFigure::new_named(w, r, scene, &rv.vehicle, omsi_net::human_path(&rv.last.figure).as_deref(), 1000 + *id as u64);
        }
        if let Some(d) = rv.driver.as_mut() {
            d.update(r, scene, &rv.vehicle, &rv.render, dt.max(1.0 / 120.0), rv.last.walker.is_none(), false);
        }
        // drawn as the own bus is: its outside meshes from outside, its inside ones to
        // whoever stands in it (the outside and the AI meshes together fought over the
        // same surfaces - the flicker and the black patches in the saloon), opaque slots
        // never faded by an alpha variable the AI scripts left at 0, the bellows skinned
        let inside = frame.inside_of == Some(*id);
        // OMSI_TRACE_REMOTE=<file.csv>: where each other player's bus is drawn, every frame
        if let Some(path) = omsi_cfg::flags::OMSI_TRACE_REMOTE.var() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                let _ = writeln!(f, "{:.4},{id},{:.3},{:.3},{:.3},{:.2},{}", lan_now(), rv.vehicle.position.x, rv.vehicle.position.y, rv.vehicle.position.z, rv.vehicle.heading, rv.track.has_offset());
            }
        }
        crate::player::sync_vehicle_transforms(r, scene, &mut rv.vehicle, &mut rv.render, &mut rv.trailer_renders, inside);
    }
    debug_log(lan, game, dt, frame);
    updates
}

/// `OMSI_DEBUG_LAN`: every few seconds, what we know of every player, what their bus
/// shows and plays here, and the bytes that went over the network.
fn debug_log(lan: &LanSession, game: &mut LanGame, dt: f32, frame: &Frame) {
    if !omsi_cfg::flags::OMSI_DEBUG_LAN.is_set() {
        return;
    }
    game.log_t -= dt;
    if game.log_t > 0.0 {
        return;
    }
    game.log_t = 4.0;
    let (sent, received) = (lan.sent(), lan.received);
    log::info!(
        "LAN traffic: {:.0} B/s out, {:.0} B/s in{}",
        (sent - game.last_sent) as f32 / 4.0,
        (received - game.last_received) as f32 / 4.0,
        game.last_gap
            .take()
            .map(|g| format!(
                "; our clock was {g:+.2} s off the host's, {:+.2} s still to catch up",
                game.slew
            ))
            .unwrap_or_default()
    );
    game.last_sent = sent;
    game.last_received = received;
    for peer in lan.peers().filter(|p| p.has_pose) {
        let q = &peer.pose;
        log::info!(
            "LAN state of player {} '{}': {} paint '{}' at ({:.2}, {:.2}, {:.3}) pitch {:.2} bank {:.2} wheels {:?} {:.1} km/h steer {:.1} flags {:010b} head {} interior {} blinker {} rpm {:.0} throttle {:.2} brake {:.2} doors {:?} passengers {} line '{}' destination '{}' {} lamps lit of {}, {} rear section(s), table {:08X}",
            q.id,
            q.name,
            q.bus,
            q.paint,
            q.x,
            q.y,
            q.z,
            q.pitch,
            q.bank,
            q.suspension.iter().map(|s| (s * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
            q.speed_kmh,
            q.steer_deg,
            q.flags,
            q.head,
            q.interior,
            q.blinker,
            q.rpm,
            q.throttle,
            q.brake,
            q.doors.iter().map(|d| (d * 10.0).round() / 10.0).collect::<Vec<_>>(),
            q.passengers,
            q.line,
            q.destination,
            q.lamps.iter().filter(|l| **l > 0.5).count(),
            q.lamps.len(),
            q.rear.len(),
            q.table
        );
        if let Some(rv) = game.remotes.get(&q.id) {
            let v = &rv.vehicle;
            // (the cockpit's own lamps left out)
            let lit: Vec<&str> = rv
                .table
                .lamps
                .iter()
                .filter(|(n, id)| {
                    !n.starts_with("cockpit")
                        && !n.starts_with("cp_")
                        && v.state.vars.get(*id as usize).copied().unwrap_or(0.0) > 0.5
                })
                .map(|(n, _)| n.as_str())
                .take(16)
                .collect();
            let steer = v.var("Axle_Steering_0_L").unwrap_or(0.0).to_degrees();
            let values: Vec<String> = rv
                .table
                .values
                .iter()
                .map(|(n, id)| {
                    format!(
                        "{n}={:.2}",
                        v.state.vars.get(*id as usize).copied().unwrap_or(0.0)
                    )
                })
                .collect();
            log::info!(
                "LAN copy of player {}: {} lamps lit {:?}; values {}; door_0 {:.2} engine_n {:.0} wheel {:.0} deg steer {:.1} deg AI_Light {:?} lights_sw_blinker {:?}",
                q.id,
                rv.table.lamps.iter().filter(|(_, id)| v.state.vars.get(*id as usize).copied().unwrap_or(0.0) > 0.5).count(),
                lit,
                values.join(" "),
                rv.table.doors.first().and_then(|id| v.state.vars.get(*id as usize)).copied().unwrap_or(0.0),
                v.var("engine_n").unwrap_or(0.0),
                v.var("Wheel_Rotation_1_L").unwrap_or(0.0).to_degrees().rem_euclid(360.0),
                steer,
                v.var("AI_Light"),
                v.var("lights_sw_blinker")
            );
            if let Some(a) = frame.audio {
                let playing: Vec<String> = rv
                    .sounds
                    .iter()
                    .flat_map(|s| s.playing(a))
                    .filter(|p| p.1 > 1.0e-4)
                    .map(|(f, g, pitch)| {
                        format!(
                            "{} {:.3}x{:.2}",
                            f.rsplit(['\\', '/']).next().unwrap_or(&f),
                            g,
                            pitch
                        )
                    })
                    .take(10)
                    .collect();
                log::info!(
                    "LAN sounds of player {} ({} set(s)): {:?}",
                    q.id,
                    rv.sounds.len(),
                    playing
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// chat

/// A key while LAN play runs: the key bound to `chat_open` ('/' or '`', see
/// `KeyboardCfg::with_game_defaults`) opens the chat line, the one bound to `chat_toggle` (V) hides and shows the chat - `bound` is the `[game]` action of `Inputs/keyboard.cfg` the
/// key makes with the modifiers held. While the line is open every key is the chat's
/// (Enter sends, Escape drops the line, Backspace takes a character back).
/// Returns whether the key was taken. Text arrives through `chat_type`.
pub fn chat_key(
    lan: &mut LanSession,
    game: &mut LanGame,
    code: KeyCode,
    pressed: bool,
    repeat: bool,
    bound: Option<&str>,
) -> bool {
    let chat = &mut game.chat;
    if chat.disabled {
        return false;
    }
    if chat.typing.is_none() {
        let toggle = bound.is_some_and(|a| a.eq_ignore_ascii_case("chat_toggle"));
        let open = bound.is_some_and(|a| a.eq_ignore_ascii_case("chat_open"));
        if pressed && !repeat && (toggle || open) {
            if toggle {
                chat.hidden = !chat.hidden;
            } else {
                chat.open();
            }
            chat.swallow.insert(code);
            return true;
        }
        // the release of the key that opened or sent the line
        return !pressed && chat.swallow.remove(&code);
    }
    if !pressed {
        return chat.swallow.remove(&code);
    }
    chat.swallow.insert(code);
    match code {
        KeyCode::Enter | KeyCode::NumpadEnter => {
            let text = chat.typing.take().unwrap_or_default();
            chat_send(lan, game, &text);
        }
        KeyCode::Escape => chat.typing = None,
        KeyCode::Backspace => {
            if let Some(t) = chat.typing.as_mut() {
                t.pop();
            }
        }
        _ => {}
    }
    true
}

/// The key that opened the chat: its release is the chat's too.
pub fn chat_swallow(game: &mut LanGame, code: KeyCode) {
    game.chat.swallow.insert(code);
}

/// Characters typed into an open chat line.
pub fn chat_type(game: &mut LanGame, text: &str) {
    if let Some(t) = game.chat.typing.as_mut() {
        for c in text.chars().filter(|c| !c.is_control()) {
            if t.chars().count() < omsi_net::MAX_CHAT {
                t.push(c);
            }
        }
    }
}

/// Is the chat line open (the keys are the chat's)?
pub fn chat_open(game: &LanGame) -> bool {
    game.chat.typing.is_some()
}

/// Say `text` to everybody.
pub fn chat_send(lan: &mut LanSession, game: &mut LanGame, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    // `/admin <password>`: a server's administration (see `admin`), never said aloud
    if let Some(pw) = text.trim().strip_prefix("/admin ") {
        crate::admin::request(lan, game, pw.trim());
        game.chat.push("* asked the server for its administration".into());
        return;
    }
    // `/reconnect`: join the host again after a lost connection (no restart of the game)
    if text.trim().eq_ignore_ascii_case("/reconnect") {
        if lan.reconnect() {
            game.chat.push("* reconnecting ...".into());
        } else {
            game.chat.push("* only a joined game can reconnect".into());
        }
        return;
    }
    let text = crate::ui::filter_chat(text.trim());
    match lan.say(&text) {
        Ok(()) => game.chat.error = None,
        Err(e) => {
            log::info!("LAN chat not sent: {e}");
            game.chat.error = Some((e, Instant::now()));
        }
    }
}

// ---------------------------------------------------------------------------------------
// what the HUD shows

/// The HUD's LAN lines: the session and its code, who is connected, what they drive and
/// where they are, what differs between the host's world and ours, and the chat line.
pub fn hud_lines(lan: &LanSession, _game: &LanGame, _player: Option<&Player>) -> Vec<String> {
    // One line: who is there is written above their buses (`name_tags`), what they say is in
    // the chat. The long list of every player's bus, place and doors that stood here filled
    // half the screen in a session of a few players.
    let mut lines = Vec::new();
    let players = lan.peer_count();
    let others = match players {
        0 => "nobody else yet".to_string(),
        1 => "1 other player".to_string(),
        n => format!("{n} other players"),
    };
    match lan.role {
        Role::Host => {
            let c = lan.code();
            // (translated apart from the code, which stays as it is)
            lines.push(format!(
                "{}   {}",
                omsi_ui::tr(&format!("Online: {others}")),
                c.as_ref().map(|c| c.encode()).unwrap_or_default(),
            ));
        }
        Role::Client => {
            if let Some(why) = lan.rejected.as_ref() {
                lines.push(format!("Online: not connected: {why} (chat /reconnect)"));
            } else if lan.connected {
                let name = lan.welcome.as_ref().map(|w| w.host_name.clone()).unwrap_or_default();
                lines.push(format!("Online: in {name}'s game, {others}"));
            } else {
                lines.push("Online: connecting ...".to_string());
            }
        }
    }
    if let Some(w) = lan.warnings.first() {
        lines.push(format!("Online: {w}"));
    }
    // OMSI_DEBUG_LAN: what the HUD shows, every few seconds
    if omsi_cfg::flags::OMSI_DEBUG_LAN.is_set() {
        HUD_LOG.with(|t| {
            let now = Instant::now();
            if t.get()
                .map(|last| now.duration_since(last) > Duration::from_secs(4))
                .unwrap_or(true)
            {
                t.set(Some(now));
                log::info!("LAN HUD:\n    {}", lines.join("\n    "));
            }
        });
    }
    lines
}

thread_local! {
    static HUD_LOG: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
mod tests {
    include!("../../../tools/test-support/original_root.rs");
    use super::*;

    /// The natural weather and the cycle need no file: a client takes them from any host.
    #[test]
    fn a_hosts_natural_weather_or_cycle_is_taken_without_a_file() {
        use clap::Parser;
        let args = crate::cli::Args::parse_from(["openomsi"]);
        for w in ["natural", "Natural", "cycle"] {
            assert_eq!(host_weather(&args, w), Ok(Some(w.to_string())), "{w}");
        }
        assert_eq!(host_weather(&args, ""), Ok(None));
        assert!(host_weather(&args, "weather/none_such.owt").is_err());
    }

    #[test]
    fn runtime_weather_change_detects_custom_and_clearing_to_default() {
        let custom = "custom:vis=5000;br=0.80;wd=90;ws=4.0;t=22.0;rh=70;p=1013;c=2;cb=120;pt=1;pi=80;wet=0.50;snow=0;snowroad=0";
        assert!(runtime_weather_changed(custom, Some("Weather/#CAVOK.owt"), None));
        assert!(!runtime_weather_changed(custom, Some(custom), None));
        assert!(!runtime_weather_changed(custom, Some(&custom.to_ascii_uppercase()), None));
        assert!(!runtime_weather_changed(custom, Some("Weather/#CAVOK.owt"), Some(custom)));
        // empty is a weather too: the host went back to the map's default
        assert!(runtime_weather_changed("", Some(custom), Some(custom)));
        assert!(!runtime_weather_changed("", None, Some(custom)));
    }

    /// The maps show another player with their bus, on foot where they walk, riding in a
    /// third player's bus with that bus, and not at all riding in ours (#1011, #1080).
    #[test]
    fn the_maps_show_a_player_where_they_are() {
        let bus = (DVec3::new(100.0, 200.0, 5.0), 90.0);
        let third = |id: u32| (id == 7).then_some((DVec3::new(-50.0, 10.0, 0.0), 180.0));
        let mut pose = Pose { id: 3, ..Default::default() };
        let p = nav_player(&pose, " Anna ", bus, 2, third).unwrap();
        assert_eq!((p.position, p.heading, p.name.as_str()), (bus.0, 90.0, "Anna"));
        pose.walker = Some(omsi_net::Walker { x: 1.0, y: 2.0, z: 3.0, heading: 45.0, ..Default::default() });
        let p = nav_player(&pose, "", bus, 2, third).unwrap();
        assert_eq!((p.position, p.heading, p.name.as_str()), (DVec3::new(1.0, 2.0, 3.0), 45.0, "player 3"));
        pose.walker.as_mut().unwrap().aboard = Some(omsi_net::Aboard { owner: 7, ..Default::default() });
        assert_eq!(nav_player(&pose, "Anna", bus, 2, third).unwrap().position, DVec3::new(-50.0, 10.0, 0.0));
        pose.walker.as_mut().unwrap().aboard = Some(omsi_net::Aboard { owner: 2, ..Default::default() });
        assert!(nav_player(&pose, "Anna", bus, 2, third).is_none());
    }

    /// What is seen comes before what is heard in the capped values list: the AA-FR Agora
    /// L's sound variables filled it in name order before its roller blind's scroll.
    #[test]
    #[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
    fn the_roller_blind_scroll_is_in_the_sync_table() {
        let root = original_root();
        let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_4d_main.bus");
        require_content(&[&bus]);
        let ty = omsi_sim::VehicleType::load(&root, &bus).expect("Agora L");
        let t = SyncTable::new(&ty, &[]);
        assert!(t.values.len() <= omsi_net::wire::MAX_VALUES);
        for want in ["Rollband_Linie_Trans", "Rollband_Linie_Trans_2"] {
            assert!(t.values.iter().any(|v| v.0.eq_ignore_ascii_case(want)), "no {want}: {}", t.describe());
        }
    }

    /// An articulated bus's rear section is in its sync table: its lamps, displays' switches
    /// and outside sounds (the AA-FR Agora L's rear section stood dark and silent in the
    /// other players' games).
    #[test]
    #[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
    fn the_rear_section_is_in_the_sync_table() {
        let root = original_root();
        let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_main.bus");
        let trail = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_trail.bus");
        require_content(&[&bus, &trail]);
        let ty = omsi_sim::VehicleType::load(&root, &bus).expect("Agora L");
        let part = Arc::new(omsi_sim::VehicleType::load(&root, &trail).expect("Agora L trail"));
        let alone = SyncTable::new(&ty, &[]);
        let whole = SyncTable::new(&ty, &[part]);
        let count = |t: &SyncTable| t.lamps.len() + t.switches.len();
        assert!(count(&whole) > count(&alone), "alone {}, whole {}", alone.describe(), whole.describe());
        assert!(!whole.part_sounds.is_empty(), "no sounds for the rear section: {}", whole.describe());
        assert_ne!(whole.hash, alone.hash);
    }

    #[test]
    fn day_numbers_and_clock_gaps() {
        assert_eq!(day_number(1990, 1) - day_number(1989, 365), 1);
        assert_eq!(day_number(1989, 1) - day_number(1988, 366), 1);
        let a = omsi_sim::SimClock {
            year: 1990,
            day_of_year: 1,
            time: 10.0,
            ..Default::default()
        };
        let b = omsi_sim::SimClock {
            year: 1989,
            day_of_year: 365,
            time: 86390.0,
            ..Default::default()
        };
        assert!((clock_gap(&a, &b) - 20.0).abs() < 1e-9);
        assert!((clock_gap(&b, &a) + 20.0).abs() < 1e-9);
        assert_eq!(parse_date("1989-05-30"), Some((1989, 150)));
        assert_eq!(parse_date("x"), None);
    }

    #[test]
    fn the_hosts_clock_moves_on_past_midnight() {
        let h = omsi_net::HostClock {
            world: omsi_net::WorldInfo {
                date: "1989-12-31".into(),
                time: 86399.5,
                ..Default::default()
            },
            at: Instant::now() - Duration::from_secs(2),
            speed: 1.0,
        };
        let c = host_clock_now(&h).unwrap();
        assert_eq!((c.year, c.day_of_year), (1990, 1));
        assert!((c.time - 1.5).abs() < 0.1, "{}", c.time);
    }
}
