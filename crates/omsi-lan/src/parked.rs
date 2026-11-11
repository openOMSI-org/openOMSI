//! A dedicated server's parked buses: the buses its players sent it (`mods::vehicles`) stand
//! in its world when nobody drives them, where their last driver left them, and any player
//! walks up to one and takes it. The server keeps the list and the places (`parked.json`
//! beside the store, so they stand there after a restart too) and says who may take which;
//! the game of the player who took one drives it as its own bus, and the others see it as
//! that player's. A free one is told to the players as a player's parked bus is
//! (`omsi_net::LanSession::set_parked`).
//!
//! A game asks with the command `park-take <bus> <op>` and hears `park-grant <bus> <op>` or
//! `park-deny <bus> <op>`; once it drives the bus it says `park-took <bus> <op>` until it
//! hears `park-yours <bus> <op>` - only then is the bus it came in left for the others.
//! `<op>` is the game's own number of that taking: an answer to an earlier asking, or a
//! word of an asking the player gave up for another, is told apart by it on both sides.

use omsi_net::{is_fingerprint, is_parked, LanSession, Pose, FIRST_PARKED_ID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A player granted a bus has this long to appear in it; then it is free again.
const GRANT_WAIT: Duration = Duration::from_secs(120);
/// The list is written this long after it changed at the earliest (and the places of the
/// buses being driven are noted as often: a server that stops dead loses no more).
const SAVE_EVERY: Duration = Duration::from_secs(10);
/// While a bus is kept for a player, a bus of the same files its game shows this near to it
/// may be either of the two: the one it came in is not followed there.
const TAKEN_WITHIN: f64 = 25.0;
/// A player takes a bus it stands at, not one across the map.
const REACH: f64 = 40.0;

/// One bus: which (its fingerprint among the kept ones), drawn how, standing where.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParkedBus {
    pub id: u32,
    pub fp: String,
    pub bus: String,
    #[serde(default)]
    pub paint: String,
    pub length: f32,
    pub width: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub box_offset: f32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub heading: f32,
    #[serde(default)]
    pub pitch: f32,
    #[serde(default)]
    pub bank: f32,
    /// The player who has it (not kept: after a restart every bus stands free).
    #[serde(skip)]
    driver: Option<Driver>,
}

/// Who has a bus. A bus is free (no `Driver`), kept for a player who asked for it
/// (`drives` false: it stands there for everybody to see until that player's game shows it,
/// or `until` passes, or the player goes), or driven (`drives`).
#[derive(Debug, Clone, PartialEq)]
struct Driver {
    player: u32,
    drives: bool,
    until: Instant,
    /// The player's number of this taking (`park-take <bus> <op>`).
    op: u32,
    /// The bus the player had when it asked, as its game showed it: left for the others only
    /// once the player sits in this one - until then it is still that player's, whatever
    /// became of the answer or of the bus it was to load.
    came_in: Option<Pose>,
}

impl ParkedBus {
    fn of(id: u32, p: &Pose) -> Self {
        let mut b = ParkedBus {
            id,
            fp: p.content.clone(),
            bus: p.bus.clone(),
            paint: p.paint.clone(),
            length: p.length,
            width: p.width,
            height: p.height,
            box_offset: p.box_offset,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            heading: 0.0,
            pitch: 0.0,
            bank: 0.0,
            driver: None,
        };
        b.stand(p);
        b
    }

    /// Left where `p` shows it.
    fn stand(&mut self, p: &Pose) {
        (self.x, self.y, self.z) = (p.x, p.y, p.z);
        (self.heading, self.pitch, self.bank) = (p.heading, p.pitch, p.bank);
    }

    fn pose(&self) -> Pose {
        Pose {
            id: self.id,
            bus: self.bus.clone(),
            paint: self.paint.clone(),
            content: self.fp.clone(),
            length: self.length,
            width: self.width,
            height: self.height,
            box_offset: self.box_offset,
            x: self.x,
            y: self.y,
            z: self.z,
            heading: self.heading,
            pitch: self.pitch,
            bank: self.bank,
            flags: omsi_net::FLAG_VEHICLE,
            ..Default::default()
        }
    }

    /// Is this the bus `bus` in the paint `paint`, or the very bus `fp`? Two players'
    /// copies of a bus differ in a file or two and so in their fingerprints: for the players
    /// they are the same bus in the same paint, and one of them stands in the world.
    fn is(&self, fp: &str, bus: &str, paint: &str) -> bool {
        self.fp == fp || (self.bus.to_lowercase() == bus.to_lowercase() && self.paint.to_lowercase() == paint.to_lowercase())
    }

    fn away(&self, x: f64, y: f64) -> f64 {
        (self.x - x).hypot(self.y - y)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    map: String,
    buses: Vec<ParkedBus>,
}

/// A place a bus may be put back to (a map's entry point): x, y, z and the heading (degrees).
pub type Spot = ([f64; 3], f32);

pub struct Parked {
    file: PathBuf,
    map: String,
    /// How many buses stand in the world at most (the numbers there are for them).
    max: usize,
    buses: Vec<ParkedBus>,
    /// The bus each player's game showed last (players in a bus this server keeps).
    seen: HashMap<u32, Pose>,
    /// Each player's latest asking (its `<op>`, counted up by its game): one numbered
    /// before it that comes late changes nothing.
    asked: HashMap<u32, u32>,
    /// Is the bus with this fingerprint kept whole, so that every player can have it?
    kept: fn(&str) -> bool,
    changed: bool,
    unsaved: Option<Instant>,
    noted: Instant,
}

fn kept_whole(fp: &str) -> bool {
    crate::mods::vehicles::manifest(fp).is_some() && crate::mods::vehicles::withheld_of(fp).is_empty()
}

impl Parked {
    /// The buses as `file` has them. Its places are for the map they were left on: on
    /// another map the buses are put to `spots` (the map's entry points), side by side.
    pub fn open(file: &Path, map: &str, max: usize, spots: &[Spot]) -> Self {
        Self::open_with(file, map, max, spots, kept_whole)
    }

    fn open_with(file: &Path, map: &str, max: usize, spots: &[Spot], kept: fn(&str) -> bool) -> Self {
        let max = max.min((u16::MAX as u32 - FIRST_PARKED_ID) as usize + 1);
        let saved: Saved = std::fs::read(file).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        let mut buses: Vec<ParkedBus> = Vec::new();
        for b in saved.buses {
            // (a bus the server keeps no longer - `uploads_keep_days` - stands there no more)
            let good = is_parked(b.id) && is_fingerprint(&b.fp) && omsi_net::vehicle_path(&b.bus).is_some() && [b.x, b.y, b.z].iter().all(|v| v.is_finite());
            if good && kept(&b.fp) && buses.len() < max && !buses.iter().any(|o| o.id == b.id || o.is(&b.fp, &b.bus, &b.paint)) {
                buses.push(b);
            }
        }
        let mut p = Parked { file: file.to_path_buf(), map: map.to_string(), max, buses, seen: HashMap::new(), asked: HashMap::new(), kept, changed: true, unsaved: None, noted: Instant::now() };
        if !saved.map.eq_ignore_ascii_case(map) && !p.buses.is_empty() {
            log::info!("parked buses: {} left on {} are put to this map's entry points", p.buses.len(), saved.map);
            p.to_spots(spots, 0);
        }
        log::info!("parked buses: {} stand in the world ({} at most)", p.buses.len(), p.max);
        p
    }

    /// The buses the server keeps that stand nowhere yet (kept before it parked any, or sent
    /// by a player who left before this was on) are put to the entry points: `described` says
    /// how such a bus is told (`describe`), None for one that cannot be.
    fn park_kept(&mut self, fps: &[String], spots: &[Spot], described: impl Fn(&str) -> Option<Pose>) {
        let first = self.buses.len();
        for fp in fps {
            if self.buses.len() >= self.max || spots.is_empty() || self.buses.iter().any(|b| b.fp == *fp) || !(self.kept)(fp) {
                continue;
            }
            let (Some(id), Some(pose)) = ((FIRST_PARKED_ID..=u16::MAX as u32).find(|id| !self.buses.iter().any(|b| b.id == *id)), described(fp)) else { continue };
            // (the same bus in the same paint, sent by another player: one is enough)
            if !self.buses.iter().any(|b| b.is(fp, &pose.bus, &pose.paint)) {
                self.buses.push(ParkedBus::of(id, &pose));
            }
        }
        if self.buses.len() > first {
            log::info!("parked buses: {} the server keeps are put to the map's entry points", self.buses.len() - first);
            self.to_spots(spots, first);
        }
    }

    /// The free buses from the `from`-th on to the entry points: one at each, the next ones
    /// beside them.
    pub fn to_spots(&mut self, spots: &[Spot], from: usize) {
        // (a map without one: they stay where they are)
        if spots.is_empty() {
            return;
        }
        for (k, b) in self.buses.iter_mut().enumerate().skip(from).filter(|(_, b)| b.driver.is_none()) {
            let ([x, y, z], heading) = spots[k % spots.len()];
            // (to the right of the one before it, a bus's width and a door's swing apart)
            let side = (k / spots.len()) as f64 * 4.5;
            let h = (heading as f64).to_radians();
            (b.x, b.y, b.z) = (x + h.cos() * side, y - h.sin() * side, z);
            (b.heading, b.pitch, b.bank) = (heading, 0.0, 0.0);
        }
        self.touched();
    }

    /// Every free bus down on the ground where it stands (`ground`: the floor at x, y nearest
    /// the height given, where the world knows one). The buses put to an entry point stand in
    /// a row at that point's height, and a yard is not level: without this the ones further
    /// along the row stood in the ground, or over it.
    fn seat(&mut self, ground: &dyn Fn(f64, f64, f64) -> Option<f64>) {
        let mut moved = 0;
        for b in self.buses.iter_mut().filter(|b| b.driver.is_none()) {
            if let Some(z) = ground(b.x, b.y, b.z).filter(|z| z.is_finite() && (z - b.z).abs() > 0.05) {
                b.z = z;
                moved += 1;
            }
        }
        if moved > 0 {
            log::info!("parked buses: {moved} put down on the ground where they stand");
            self.touched();
        }
    }

    pub fn buses(&self) -> &[ParkedBus] {
        &self.buses
    }

    fn touched(&mut self) {
        self.changed = true;
        self.unsaved.get_or_insert_with(Instant::now);
    }

    /// The bus `player` left (`pose`: as its game showed it last) stands there from now on:
    /// the parked bus it had taken, or its own when the server keeps that one.
    fn left(&mut self, player: u32, pose: &Pose) {
        if let Some(b) = self.buses.iter_mut().find(|b| b.driver.as_ref().is_some_and(|d| d.player == player && d.drives)) {
            b.stand(pose);
            b.driver = None;
            log::info!("parked buses: player {player} left {} ({}) at {:.0}, {:.0}", b.bus, b.id, b.x, b.y);
            return self.touched();
        }
        // (one of each bus and paint: a second player's copy of it is not another bus here)
        if self.buses.len() >= self.max || self.buses.iter().any(|b| b.is(&pose.content, &pose.bus, &pose.paint)) || !(self.kept)(&pose.content) {
            return;
        }
        let Some(id) = (FIRST_PARKED_ID..=u16::MAX as u32).find(|id| !self.buses.iter().any(|b| b.id == *id)) else { return };
        log::info!("parked buses: player {player}'s {} stays at {:.0}, {:.0} as bus {id}", pose.bus, pose.x, pose.y);
        self.buses.push(ParkedBus::of(id, pose));
        self.touched();
    }

    /// A frame of the server: who drives what now, the free buses told to the players, the
    /// list written when it changed.
    pub fn tick(&mut self, lan: &mut LanSession, dt: f32) {
        let now: HashMap<u32, Pose> = lan
            .peers()
            .filter(|p| p.has_pose && p.has_info && p.pose.has_vehicle() && is_fingerprint(&p.pose.content) && !is_parked(p.pose.id))
            .map(|p| (p.pose.id, p.pose.clone()))
            .collect();
        // the buses kept for a player: its own when its game says it drives one (`took`), free
        // again when it does not come. Meanwhile the bus it came in is followed where it goes:
        // a player whose answer was lost drives on, and leaves it where it is then
        let online: Vec<u32> = lan.peers().map(|p| p.pose.id).collect();
        // (a player who comes again counts its askings anew)
        self.asked.retain(|player, _| online.contains(player));
        for k in 0..self.buses.len() {
            let Some(mut d) = self.buses[k].driver.clone().filter(|d| !d.drives) else { continue };
            let (b, id) = (&self.buses[k], self.buses[k].id);
            let shown = now.get(&d.player).filter(|p| d.came_in.as_ref().is_some_and(|c| c.content == p.content));
            if let Some(p) = shown.filter(|p| !(p.content == b.fp && b.away(p.x, p.y) < TAKEN_WITHIN)) {
                d.came_in = Some(p.clone());
                self.buses[k].driver = Some(d.clone());
            }
            if !online.contains(&d.player) || Instant::now() > d.until {
                log::info!("parked buses: player {} did not take bus {id}: free again", d.player);
                self.buses[k].driver = None;
                self.changed = true;
                // (a player who went meanwhile left the bus it came in, as every player who goes)
                if let (false, Some(pose)) = (online.contains(&d.player), &d.came_in) {
                    self.left(d.player, pose);
                }
            }
        }
        // (a player a bus is kept for is between two buses: the one its game shows is the one
        // it came in, held in `Driver::came_in` until it is known which of the two it drives)
        let between = |me: &Self, player: u32| me.buses.iter().any(|b| b.driver.as_ref().is_some_and(|d| d.player == player && !d.drives));
        // players who went, or changed buses: the bus they had stands where it was
        let before = std::mem::take(&mut self.seen);
        for (player, pose) in &before {
            if now.get(player).is_none_or(|p| p.content != pose.content) && !between(self, *player) {
                self.left(*player, pose);
            }
        }
        self.seen = now.into_iter().filter(|(player, _)| !between(self, *player)).collect();
        if self.noted.elapsed() > SAVE_EVERY {
            self.noted = Instant::now();
            for b in &mut self.buses {
                let Some(p) = b.driver.as_ref().filter(|d| d.drives).and_then(|d| self.seen.get(&d.player)) else { continue };
                if b.away(p.x, p.y) > 1.0 {
                    b.stand(p);
                    self.unsaved.get_or_insert_with(Instant::now);
                }
            }
            // (and the server goes on keeping them, however long nobody drives them)
            crate::mods::vehicles::in_use(self.buses.iter().map(|b| b.fp.as_str()));
        }
        if std::mem::take(&mut self.changed) {
            // (a bus granted stands there until its player's game shows it: that game makes
            // its own from the one it draws, and a player who never does loses nothing)
            lan.set_parked(self.buses.iter().filter(|b| b.driver.as_ref().is_none_or(|d| !d.drives)).map(ParkedBus::pose).collect());
        }
        lan.tell_parked(dt);
        if self.unsaved.is_some_and(|t| t.elapsed() > SAVE_EVERY) {
            self.save();
        }
    }

    /// `player`'s game says it drives bus `id` now: the bus kept for it is its own (and the
    /// one it came in stands where it was left, for the others), when its game shows that bus.
    /// Said again by a player who has it already: yes again.
    fn took(&mut self, lan: &LanSession, player: u32, id: u32, op: u32) -> bool {
        let Some(k) = self.buses.iter().position(|b| b.id == id && b.driver.as_ref().is_some_and(|d| d.player == player && d.op == op)) else { return false };
        let d = self.buses[k].driver.clone().expect("found by its driver");
        if d.drives {
            return true;
        }
        if !lan.peers().any(|p| p.pose.id == player && p.pose.has_vehicle() && p.pose.content == self.buses[k].fp) {
            return false;
        }
        if let Some(pose) = &d.came_in {
            self.left(player, pose);
        }
        self.buses[k].driver = Some(Driver { drives: true, came_in: None, ..d });
        self.changed = true;
        true
    }

    /// A player's command, when it is about a parked bus (`park-take <number>`,
    /// `park-took <number>`): answered.
    pub fn command(&mut self, lan: &mut LanSession, from: u32, text: &str) -> bool {
        // (`<bus> <op>`; a word that names no bus is nobody's)
        let named = |rest: &str| {
            let mut w = rest.split_whitespace();
            Some((w.next()?.parse::<u32>().ok()?, w.next().and_then(|o| o.parse::<u32>().ok()).unwrap_or(0)))
        };
        if let Some(rest) = text.strip_prefix("park-took ") {
            // (no answer to one that is not so yet: the game says it again)
            if let Some((id, op)) = named(rest).filter(|(id, op)| self.took(lan, from, *id, *op)) {
                lan.command(from, &format!("park-yours {id} {op}"));
            }
            return true;
        }
        let Some(rest) = text.strip_prefix("park-take ") else { return false };
        // (an asking numbered before the player's latest came late - the player has asked
        // again since: it changes no bus and gets no word, the game has left it)
        let late = |me: &Self, op: u32| me.asked.get(&from).is_some_and(|newest| (op.wrapping_sub(*newest) as i32) < 0);
        if let Some((id, op)) = named(rest).filter(|(_, op)| !late(self, *op)) {
            self.asked.insert(from, op);
            let granted = self.take(lan, from, id, op);
            lan.command(from, &format!("{} {id} {op}", if granted { "park-grant" } else { "park-deny" }));
        }
        true
    }

    /// `player` asks for bus `id`: it is kept for that player when it is free and the player is
    /// at it. The bus the player came in stays its own until its game shows it in this one.
    fn take(&mut self, lan: &LanSession, player: u32, id: u32, op: u32) -> bool {
        let Some(peer) = lan.peers().find(|p| p.pose.id == player && p.has_pose) else { return false };
        let at = peer.pose.walker.map(|w| (w.x, w.y)).unwrap_or((peer.pose.x, peer.pose.y));
        let Some(k) = self.buses.iter().position(|b| b.id == id) else { return false };
        // (asked again - the answer may have been lost on its way: the same answer, and the
        // time to take it anew, under the number of this asking)
        if let Some(d) = self.buses[k].driver.as_mut().filter(|d| d.player == player && !d.drives) {
            (d.until, d.op) = (Instant::now() + GRANT_WAIT, op);
            return true;
        }
        if self.buses[k].driver.is_some() || self.buses[k].away(at.0, at.1) >= REACH {
            return false;
        }
        // (one bus is kept for a player at a time: another it asked for before is free again)
        let mut came_in = self.seen.remove(&player);
        for b in &mut self.buses {
            if b.driver.as_ref().is_some_and(|d| d.player == player && !d.drives) {
                came_in = came_in.or(b.driver.take().and_then(|d| d.came_in));
            }
        }
        let b = &mut self.buses[k];
        b.driver = Some(Driver { player, drives: false, until: Instant::now() + GRANT_WAIT, op, came_in });
        log::info!("parked buses: {} ({id}) is kept for player {player}", b.bus);
        self.changed = true;
        true
    }

    /// The administration puts the free bus `id` down at `x`, `y` facing `heading` (degrees),
    /// on the ground there (`ground`: the floor at a place nearest a height - the one the bus
    /// stood at - when the world knows one): a bus left where no bus gets out again, or in
    /// the way, is parked where it belongs.
    fn put(&mut self, id: u32, x: f64, y: f64, heading: f32, ground: &dyn Fn(f64, f64, f64) -> Option<f64>) -> Result<(), &'static str> {
        if ![x, y, heading as f64].iter().all(|v| v.is_finite()) {
            return Err("not a place");
        }
        let b = self.buses.iter_mut().find(|b| b.id == id).ok_or("no such bus")?;
        if b.driver.is_some() {
            return Err("a player has it");
        }
        (b.x, b.y, b.z) = (x, y, ground(x, y, b.z).filter(|z| z.is_finite()).unwrap_or(b.z));
        (b.heading, b.pitch, b.bank) = (heading.rem_euclid(360.0), 0.0, 0.0);
        self.touched();
        Ok(())
    }

    /// The list to its file now (as the server stops; `tick` writes it as it changes).
    pub fn save(&mut self) {
        let saved = Saved { map: self.map.clone(), buses: self.buses.clone() };
        let tmp = self.file.with_extension("json.tmp");
        let written = serde_json::to_vec_pretty(&saved).map_err(|e| e.to_string()).and_then(|d| std::fs::write(&tmp, d).and_then(|_| std::fs::rename(&tmp, &self.file)).map_err(|e| e.to_string()));
        match written {
            Ok(()) => self.unsaved = None,
            // (what changed is still to be written: tried again in a while)
            Err(e) => {
                log::warn!("parked buses: {} not written: {e}", self.file.display());
                self.unsaved = Some(Instant::now());
            }
        }
    }
}

/// How many buses a dedicated server parks (`parked_buses` of its server.cfg; 0: none).
static ALLOWED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The dedicated server's parked buses, once its world is there.
static SERVER: std::sync::Mutex<Option<Parked>> = std::sync::Mutex::new(None);

fn server() -> std::sync::MutexGuard<'static, Option<Parked>> {
    SERVER.lock().unwrap_or_else(|e| e.into_inner())
}

/// server.cfg: up to `max` of the buses the players sent stand in the world (0: none).
pub fn allow(max: usize) {
    ALLOWED.store(max, std::sync::atomic::Ordering::Relaxed);
}

/// A kept bus as it is told while it stands parked: read from its kept files for its size
/// and for the paint it was sent in (the one whose pictures are among its files).
fn describe(root: &Path, fp: &str) -> Option<Pose> {
    let m = crate::mods::vehicles::manifest(fp)?;
    let ty = omsi_sim::VehicleType::load(root, &crate::mods::vehicles::bus_file(fp)?).ok()?;
    let came = |file: &str| {
        let file = file.replace('\\', "/").to_lowercase();
        m.entries.iter().any(|e| e.path.to_lowercase().ends_with(&file))
    };
    let paint = ty.paint_schemes.iter().find(|s| !s.textures.is_empty() && s.textures.iter().all(|t| came(&t.1))).map(|s| s.name.clone()).unwrap_or_default();
    let bb = ty.def.bounding_box.unwrap_or(crate::spawn::BUS_BOX);
    Some(Pose { content: fp.to_string(), bus: m.bus, paint, length: bb[1], width: bb[0], height: bb[2], box_offset: bb[4], ..Default::default() })
}

/// The server's world is loaded (`map`, `spots`: its entry points): its parked buses stand
/// in it from now on - where they were left, the ones never left anywhere at the entry
/// points - each on the ground there (`ground`: the floor at x, y nearest a height).
pub fn start(root: &Path, map: &str, spots: &[Spot], ground: &dyn Fn(f64, f64, f64) -> Option<f64>) {
    let max = ALLOWED.load(std::sync::atomic::Ordering::Relaxed);
    let Some(file) = crate::data_dir().filter(|_| max > 0).map(|d| d.join("lan-store").join("parked.json")) else { return };
    let mut parked = Parked::open(&file, map, max, spots);
    parked.park_kept(&crate::mods::vehicles::kept_fingerprints(), spots, |fp| describe(root, fp));
    parked.seat(ground);
    *server() = Some(parked);
}

/// A player's command to the server, when it is about a parked bus.
pub fn server_command(lan: &mut LanSession, from: u32, text: &str) -> bool {
    server().as_mut().is_some_and(|p| p.command(lan, from, text))
}

/// The buses standing free in the server's world now (for its map of who is where).
pub fn server_free() -> Vec<Pose> {
    server().as_ref().map(|p| p.buses.iter().filter(|b| b.driver.is_none()).map(ParkedBus::pose).collect()).unwrap_or_default()
}

/// A command of the server's administration (its web panel, a tool on its machine), when it
/// is about the parked buses: `parked move <number> <x> <y> <heading>` puts a free one down
/// there, on the ground (`ground`: the floor at a place nearest a height, where the world is
/// loaded).
pub fn server_admin(text: &str, ground: &dyn Fn(f64, f64, f64) -> Option<f64>) -> bool {
    let Some(rest) = text.trim().strip_prefix("parked ") else { return false };
    let words: Vec<&str> = rest.split_whitespace().collect();
    let done = match (server().as_mut(), words.as_slice()) {
        (None, _) => Err("this server parks no buses (parked_buses = 0)"),
        (Some(p), ["move", id, x, y, heading]) => match (id.parse(), x.parse::<f64>(), y.parse::<f64>(), heading.parse::<f32>()) {
            (Ok(id), Ok(x), Ok(y), Ok(heading)) => p.put(id, x, y, heading, ground),
            _ => Err("parked move <number> <x> <y> <heading>"),
        },
        _ => Err("parked move <number> <x> <y> <heading>"),
    };
    match done {
        Ok(()) => log::info!("parked buses: the administration: {rest}"),
        Err(why) => log::warn!("parked buses: the administration's '{rest}' was not done: {why}"),
    }
    true
}

/// A frame of the server (see `Parked::tick`).
pub fn server_tick(lan: &mut LanSession, dt: f32) {
    if let Some(p) = server().as_mut() {
        p.tick(lan, dt);
    }
}

#[cfg(test)]
mod tests;
