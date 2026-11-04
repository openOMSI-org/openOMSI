use crate::{doorways, tried_doors, Command, Remote, Vehicle};
use omsi_sim::vehicle_api::ApiValue;
use omsi_sim::{VehicleInstance, VehicleType};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

const PUBLISH_EVERY: f32 = 1.0 / 30.0;

type Skins = Receiver<(String, Vec<(String, Vec<u8>)>)>;
type Ways = (Vec<Vec<usize>>, Vec<Vec<String>>);

enum Found {
    Doors(Ways, Option<crate::sign::Sign>),
    Tried(Vec<(Vec<String>, Vec<usize>)>),
}

type Doors = Receiver<(String, Found)>;
pub type ActionAliases<'a> = &'a [(&'a str, &'a [&'a str])];

#[derive(Default)]
pub struct Cockpit {
    pub server: Option<Remote>,
    failed_port: Option<u16>,
    publish_t: f32,
    vehicle: String,
    pub lang: String,
    skins: Option<Skins>,
    tried: Option<Doors>,
    sign: Option<crate::sign::Sign>,
    sign_t: f32,
    pub nav_t: f32,
    pub phone_steers: bool,
    door_triggers: Vec<Vec<String>>,
    door_ways: Vec<Vec<usize>>,
    all_fired: Vec<String>,
}

pub struct Tick<'a> {
    pub dt: f32,
    pub root: &'a Path,
    pub paused: bool,
    pub doors_idle: bool,
    pub action_aliases: ActionAliases<'a>,
}

fn aliases(v: &VehicleInstance, buttons: &[Vec<String>], action_aliases: ActionAliases) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let keys = omsi_sim::door_keys::door_keys(&v.ty);
    let doors = if buttons.is_empty() { &keys[..] } else { buttons };
    for (n, group) in doors.iter().enumerate() {
        for t in group.iter().flat_map(|g| g.split('|')) {
            out.push((format!("door_{}", n + 1), t.to_string()));
            out.push(("doors_all".into(), t.to_string()));
        }
    }
    for (action, names) in action_aliases {
        for n in *names {
            out.push((action.to_string(), n.to_string()));
        }
    }
    out
}

fn doors(ty: &VehicleType, trailers: &[Arc<VehicleType>]) -> Ways {
    let mut ways = doorways(ty);
    for t in trailers {
        ways.extend(doorways(t));
    }
    let keys = omsi_sim::door_keys::door_keys(ty).len();
    let fronts: Vec<String> = (0..ways.len()).map(|k| format!("bus_doorfront{k}")).collect();
    let direct = keys < ways.len() && fronts.iter().all(|f| ty.program.trigger(f).is_some());
    match (direct, keys == 0 || ways.len() == keys) {
        (true, _) => (ways, fronts.into_iter().map(|f| vec![f]).collect()),
        (false, true) => (ways, Vec::new()),
        _ => (Vec::new(), Vec::new()),
    }
}

fn find_doors(v: &VehicleInstance, root: &Path, key: &str) -> Doors {
    let (tx, rx) = channel();
    let (ty, root, key) = (v.ty.clone(), root.to_path_buf(), key.to_string());
    let trailers: Vec<Arc<VehicleType>> = v.trailers.iter().map(|t| t.ty.clone()).collect();
    let _ = std::thread::Builder::new().name("remote cockpit doors".into()).spawn(move || {
        let _ = tx.send((key.clone(), Found::Doors(doors(&ty, &trailers), crate::sign::Sign::find(&ty, &root))));
        let tried = tried_doors(ty.clone(), &doorways(&ty));
        log::info!("remote cockpit: door buttons {:?}", tried.iter().map(|(t, l)| format!("{} = {l:?}", t.join(" + "))).collect::<Vec<_>>());
        let _ = tx.send((key, Found::Tried(tried)));
    });
    rx
}

fn start_skins(v: &VehicleInstance, root: &Path, key: &str) -> Skins {
    let (tx, rx) = channel();
    let (ty, root, key) = (v.ty.clone(), root.to_path_buf(), key.to_string());
    let scheme = v.host.paint_scheme.flatten();
    let _ = std::thread::Builder::new().name("remote cockpit buttons".into()).spawn(move || {
        let skins = crate::extract_skins(&ty, &root, scheme);
        log::info!("remote cockpit: {} pictures of the bus's own buttons", skins.len());
        let _ = tx.send((key, skins));
    });
    rx
}

fn vehicle_key(v: &VehicleInstance) -> String {
    let path = &v.ty.def.path;
    let folder = path.parent().and_then(|d| d.file_name()).map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let hof = v.host.hof.as_ref().map(|h| h.name.trim().to_string()).unwrap_or_default();
    format!("{folder}/{file}#{hof}")
}

fn vehicle(v: &VehicleInstance, name: String, bound: Vec<String>, doors: Vec<Vec<usize>>, aliases: Vec<(String, String)>) -> Vehicle {
    let def = &v.ty.def;
    let mut actions = v.ty.program.trigger_names();
    actions.extend(bound);
    actions.sort_by_key(|a| a.to_ascii_lowercase());
    actions.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let key = vehicle_key(v);
    Vehicle {
        key: key.split('#').next().unwrap_or("").to_string(),
        name,
        folder: def.path.parent().map(Path::to_path_buf).unwrap_or_default(),
        actions,
        aliases,
        depot: v.host.hof.as_deref().map(omsi_sim::vehicle_api::depot),
        doors,
    }
}

impl Cockpit {
    pub fn failed(&self) -> bool {
        self.failed_port.is_some()
    }

    pub fn forget_failure(&mut self) {
        self.failed_port = None;
    }

    pub fn must_stop(&self, wanted: bool, port: u16) -> bool {
        !wanted || self.server.as_ref().is_some_and(|s| s.port() != port)
    }

    pub fn stop(&mut self) -> Vec<Command> {
        self.phone_steers = false;
        let Some(server) = self.server.take() else { return Vec::new() };
        let mut left: Vec<Command> = server.take_all_commands().into_iter().filter(|c| matches!(c, Command::Action { down: false, .. })).collect();
        left.extend(server.release_all().into_iter().map(|name| Command::Action { name, down: false }));
        left
    }

    pub fn start(&mut self, port: u16, dir: impl FnOnce() -> Option<PathBuf>) {
        if self.server.is_some() || self.failed_port == Some(port) {
            return;
        }
        let Some(dir) = dir() else { return };
        match Remote::start(port, dir) {
            Ok(s) => {
                self.server = Some(s);
                self.failed_port = None;
                self.vehicle.clear();
                self.lang.clear();
            }
            Err(e) => {
                log::warn!("remote cockpit: port {port}: {e}");
                self.failed_port = Some(port);
            }
        }
    }

    pub fn direct_doors(&mut self, name: &str, down: bool, v: &VehicleInstance) -> Option<Vec<String>> {
        if self.door_triggers.is_empty() {
            return None;
        }
        let name = name.to_ascii_lowercase();
        if name == "doors_all" {
            if !down {
                return Some(std::mem::take(&mut self.all_fired));
            }
            if !self.all_fired.is_empty() {
                return Some(Vec::new());
            }
            let open: Vec<bool> = self.door_ways.iter().map(|w| w.iter().any(|l| v.var(&format!("door_{l}")).is_some_and(|x| x > 0.05))).collect();
            self.all_fired = all_doors(&self.door_triggers, &open);
            return Some(self.all_fired.clone());
        }
        let n: usize = name.strip_prefix("door_")?.parse().ok()?;
        self.door_triggers.get(n.checked_sub(1)?).cloned()
    }

    pub fn frame(&mut self, t: Tick, v: Option<&VehicleInstance>, describe: impl FnOnce(&VehicleInstance) -> (String, Vec<String>), env: impl FnOnce() -> ApiValue) {
        let Some(server) = self.server.as_mut() else { return };
        let key = v.map(vehicle_key).unwrap_or_default();
        if self.vehicle != key {
            server.release_held();
            self.phone_steers = false;
            self.all_fired.clear();
            (self.door_ways, self.door_triggers) = Default::default();
            server.set_vehicle(v.map(|v| {
                let (name, bound) = describe(v);
                vehicle(v, name, bound, Vec::new(), aliases(v, &[], t.action_aliases))
            }));
            self.skins = v.map(|v| start_skins(v, t.root, &key));
            self.tried = v.map(|v| find_doors(v, t.root, &key));
            self.sign = None;
            self.vehicle = key;
        }
        let ready = !server.holds_a_door() && t.doors_idle;
        while let (true, Some(Ok((for_key, found))), Some(v)) = (ready, self.tried.as_ref().map(|rx| rx.try_recv()), v) {
            if for_key != self.vehicle {
                continue;
            }
            match found {
                Found::Doors(ways, sign) => {
                    (self.door_ways, self.door_triggers) = ways;
                    self.sign = sign;
                }
                Found::Tried(tried) => {
                    self.tried = None;
                    if !tried.is_empty() {
                        (self.door_triggers, self.door_ways) = tried.into_iter().unzip();
                    }
                }
            }
            server.set_doors(self.door_ways.clone(), aliases(v, &self.door_triggers, t.action_aliases));
        }
        if let Some(Ok((for_key, skins))) = self.skins.as_ref().map(|rx| rx.try_recv()) {
            if for_key == self.vehicle {
                server.set_skins(skins);
            }
            self.skins = None;
        }
        self.sign_t -= t.dt;
        if let (Some(sign), Some(v), true) = (self.sign.as_mut(), v, self.sign_t <= 0.0) {
            self.sign_t = 0.25;
            if let Some(png) = sign.render(v) {
                server.set_sign(png);
            }
        }
        self.publish_t -= t.dt;
        if self.publish_t > 0.0 {
            return;
        }
        self.publish_t = PUBLISH_EVERY;
        match v {
            Some(v) => server.set_state(Some(&v.html_api_snapshot()), &v.html_env_snapshot(), t.paused),
            None => server.set_state(None, &env(), t.paused),
        }
    }
}

pub fn phone_steers(before: bool, held: bool, local: bool) -> bool {
    held || (before && !local)
}

pub(crate) fn all_doors(triggers: &[Vec<String>], open: &[bool]) -> Vec<String> {
    let any = open.contains(&true);
    triggers.iter().zip(open).filter(|(_, o)| !any || **o).flat_map(|(t, _)| t.clone()).filter(|t| t != "bus_dooraft").collect()
}

pub fn qr_png(url: &str, dir: &Path) -> Option<PathBuf> {
    const MODULE_PX: u32 = 8;
    let name = format!("qr-{:08x}.png", url.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619)));
    let path = dir.join(name);
    if path.is_file() {
        return Some(path);
    }
    for old in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if old.file_name().to_string_lossy().starts_with("qr-") {
            let _ = std::fs::remove_file(old.path());
        }
    }
    let (n, cells) = crate::qr(url)?;
    let side = (n as u32 + 8) * MODULE_PX;
    let img = image::GrayImage::from_fn(side, side, |x, y| {
        let (cx, cy) = ((x / MODULE_PX) as i64 - 4, (y / MODULE_PX) as i64 - 4);
        let dark = cx >= 0 && cy >= 0 && (cx as usize) < n && (cy as usize) < n && cells[cy as usize * n + cx as usize];
        image::Luma([if dark { 0 } else { 255 }])
    });
    std::fs::create_dir_all(dir).ok()?;
    img.save(&path).ok()?;
    Some(path)
}
