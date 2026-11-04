pub mod cockpit;
mod json;
mod layouts;
pub mod nav;
mod pairing;
mod queue;
mod server;
mod session;
pub mod sign;
mod doors;
pub use doors::{doorways, tried_doors};
mod skins;
mod texts;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

pub use skins::extract as extract_skins;
pub use texts::TEXTS;

use omsi_sim::vehicle_api::ApiValue;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Action { name: String, down: bool },
    Game(String),
    Indicator(u8),
    StartUp,
    SaloonLights,
    Route(usize),
    Line(String),
    Destination(usize),
    ClearLine,
    NextStop(usize),
}

#[derive(Clone)]
pub struct Vehicle {
    pub key: String,
    pub name: String,
    pub folder: PathBuf,
    pub actions: Vec<String>,
    pub aliases: Vec<(String, String)>,
    pub depot: Option<ApiValue>,
    pub doors: Vec<Vec<usize>>,
}

#[derive(Default)]
pub(crate) struct Feed {
    pub texts: Arc<String>,
    pub texts_seq: u64,
    pub vehicle_msg: Arc<String>,
    pub vehicle_seq: u64,
    pub state: Arc<serde_json::Value>,
    pub state_seq: u64,
    pub channels: Vec<(&'static str, Arc<String>, u64)>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) struct Shared {
    pub pin: Mutex<String>,
    pub pairing_open: AtomicBool,
    pub generation: AtomicU64,
    pub store: layouts::Store,
    pub devices: Mutex<pairing::Devices>,
    pub feed: Mutex<Feed>,
    pub vehicle: Mutex<Option<Arc<Vehicle>>>,
    pub clients: AtomicUsize,
    pub commands: Mutex<queue::Queue>,
    pub held: Mutex<HashMap<u64, Vec<String>>>,
    pub sessions: AtomicU64,
    pub paired: AtomicU64,
    pub stop: AtomicBool,
    pub skins: Mutex<HashMap<String, Arc<Vec<u8>>>>,
    pub sign: Mutex<Option<Arc<Vec<u8>>>>,
    pub sign_seq: AtomicU64,
    pub drive: Mutex<Option<Drive>>,
    pub unpaired: Mutex<HashMap<std::net::IpAddr, usize>>,
    pub trusted: Mutex<std::collections::HashSet<std::net::IpAddr>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drive {
    pub session: u64,
    pub at: std::time::Instant,
    pub steering: f32,
    pub throttle: f32,
    pub brake: f32,
    pub held: bool,
}

pub(crate) const DRIVE_FRESH: std::time::Duration = std::time::Duration::from_millis(250);
pub(crate) const STEER_FRESH: std::time::Duration = std::time::Duration::from_millis(1500);

impl Shared {
    pub(crate) fn feed(&self) -> MutexGuard<'_, Feed> {
        lock(&self.feed)
    }

    pub(crate) fn rebuild_vehicle(&self) {
        let mut skins: Vec<String> = lock(&self.skins).keys().cloned().collect();
        skins.sort();
        let vehicle = lock(&self.vehicle).clone();
        let msg = Arc::new(json::vehicle_message(vehicle.as_deref(), &self.store, &skins));
        let mut f = self.feed();
        f.vehicle_msg = msg;
        f.vehicle_seq += 1;
    }

    pub(crate) fn new_pin(&self) {
        *lock(&self.pin) = pairing::new_pin();
    }

    pub(crate) fn send(&self, c: Command) -> bool {
        lock(&self.commands).push(c)
    }
}

pub struct Remote {
    shared: Arc<Shared>,
    port: u16,
    last_state: serde_json::Value,
}

impl Remote {
    pub fn start(port: u16, dir: PathBuf) -> std::io::Result<Remote> {
        let shared = Arc::new(Shared {
            pin: Mutex::new(pairing::new_pin()),
            pairing_open: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            store: layouts::Store::new(dir.join("layouts")),
            devices: Mutex::new(pairing::Devices::load(dir.join("devices.txt"))),
            feed: Mutex::new(Feed::default()),
            vehicle: Mutex::new(None),
            clients: AtomicUsize::new(0),
            commands: Mutex::new(queue::Queue::default()),
            held: Mutex::new(HashMap::new()),
            sessions: AtomicU64::new(0),
            paired: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            skins: Mutex::new(HashMap::new()),
            sign: Mutex::new(None),
            sign_seq: AtomicU64::new(0),
            drive: Mutex::new(None),
            unpaired: Mutex::new(HashMap::new()),
            trusted: Mutex::default(),
        });
        shared.rebuild_vehicle();
        let port = server::start(port, shared.clone())?;
        log::info!("remote cockpit: listening on port {port}");
        Ok(Remote { shared, port, last_state: serde_json::Value::Null })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn pin(&self) -> String {
        lock(&self.shared.pin).clone()
    }

    pub fn set_pairing(&self, open: bool) {
        self.shared.pairing_open.store(open, Ordering::Relaxed);
    }

    pub fn take_paired(&self) -> bool {
        self.shared.paired.swap(0, Ordering::Relaxed) > 0
    }

    pub fn clients(&self) -> usize {
        self.shared.clients.load(Ordering::Relaxed)
    }

    pub fn forget_devices(&self) {
        lock(&self.shared.devices).forget_all();
        lock(&self.shared.trusted).clear();
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
        self.shared.new_pin();
    }

    pub fn holds_a_door(&self) -> bool {
        lock(&self.shared.held).values().flatten().any(|n| n.to_ascii_lowercase().starts_with("door"))
    }

    pub fn release_held(&self) {
        for name in self.release_all() {
            self.shared.send(Command::Action { name, down: false });
        }
    }

    pub fn release_all(&self) -> Vec<String> {
        let mut names: Vec<String> = lock(&self.shared.held).drain().flat_map(|(_, v)| v).collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn set_texts(&self, lang: &str, texts: &[(&str, String)]) {
        let msg = json::texts_message(lang, texts);
        let mut f = self.shared.feed();
        if *f.texts != msg {
            f.texts = Arc::new(msg);
            f.texts_seq += 1;
        }
    }

    pub fn set_vehicle(&self, vehicle: Option<Vehicle>) {
        lock(&self.shared.skins).clear();
        self.set_sign(None);
        *lock(&self.shared.vehicle) = vehicle.map(Arc::new);
        self.shared.rebuild_vehicle();
    }

    pub fn set_doors(&self, doors: Vec<Vec<usize>>, aliases: Vec<(String, String)>) {
        let mut slot = lock(&self.shared.vehicle);
        let Some(old) = slot.as_deref() else { return };
        *slot = Some(Arc::new(Vehicle { doors, aliases, ..old.clone() }));
        drop(slot);
        self.shared.rebuild_vehicle();
    }

    pub fn set_sign(&self, png: Option<Vec<u8>>) {
        *lock(&self.shared.sign) = png.map(Arc::new);
        self.shared.sign_seq.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_skins(&self, skins: Vec<(String, Vec<u8>)>) {
        *lock(&self.shared.skins) = skins.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        self.shared.rebuild_vehicle();
    }

    pub fn set_state(&mut self, vehicle: Option<&ApiValue>, env: &ApiValue, paused: bool) {
        let state = json::state(vehicle, env, paused);
        if state == self.last_state {
            return;
        }
        self.last_state = state.clone();
        let mut f = self.shared.feed();
        f.state = Arc::new(state);
        f.state_seq += 1;
    }

    pub fn publish(&self, channel: &'static str, message: String) {
        let mut f = self.shared.feed();
        match f.channels.iter_mut().find(|c| c.0 == channel) {
            Some(c) if *c.1 == message => {}
            Some(c) => {
                c.1 = Arc::new(message);
                c.2 += 1;
            }
            None => f.channels.push((channel, Arc::new(message), 1)),
        }
    }

    pub fn take_commands(&self) -> Vec<Command> {
        lock(&self.shared.commands).take(queue::PER_FRAME)
    }

    pub fn take_all_commands(&self) -> Vec<Command> {
        lock(&self.shared.commands).take(usize::MAX)
    }

    pub fn drive(&self) -> Option<Drive> {
        let mut d = lock(&self.shared.drive);
        if d.is_some_and(|x| x.at.elapsed() > STEER_FRESH) {
            *d = None;
        }
        d.map(|x| match x.at.elapsed() > DRIVE_FRESH {
            true => Drive { throttle: 0.0, brake: 0.0, held: false, ..x },
            false => x,
        })
    }
}

pub fn pairing_url(ip: &str, port: u16, pin: &str) -> String {
    format!("http://{ip}:{port}/#pin={pin}")
}

pub fn qr(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let n = code.size();
    let cells = (0..n).flat_map(|y| (0..n).map(move |x| (x, y))).map(|(x, y)| code.get_module(x, y)).collect();
    Some((n as usize, cells))
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests;
