//! What the launcher's model asks of the program it runs in: the game program (omsi-app)
//! installs a `Host` once, before the model is made (`set`). Everything here is what only the
//! game has - the interface's language and machine translation, the previous run's crash on a
//! phone, the city map read the way the navigator reads it - so the model needs none of the
//! game's modules.
//!
//! Without a host (the model's own tests) every call answers as if nothing were there.

use glam::{DVec3, Vec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The game program's side of the launcher's model (see the module).
pub trait Host: Send + Sync {
    /// Show the interface in `code` (the settings' ENG / DEU / FRA / RUS).
    fn ui_language(&self, _code: &str) {}
    /// Machine translation of the interface on or off, and texts it must leave alone (names
    /// of maps, buses, lines).
    fn mt_enable(&self, _on: bool) {}
    fn mt_protect(&self, _texts: &mut dyn Iterator<Item = &str>) {}
    fn tr(&self, text: &str) -> String {
        text.to_string()
    }
    /// A phone: what the previous run said last and the end of its log, when it crashed.
    fn previous_run_crash(&self) -> Option<(String, String)> {
        None
    }
    /// A map's roads, read the way the game's city map reads them (the navigator's).
    fn city_map(&self, _root: &Path, _tiles: &[(i32, i32, PathBuf)], _chrono: &[PathBuf]) -> CityMap {
        CityMap::default()
    }
    /// The map's own layers (ground, water, paved surfaces, houses: `omsi_scene::mapgen`), read
    /// the way the navigator reads them, from the game's cache when it has them.
    fn map_layers(&self, _root: &Path, _map_dir: &Path, _tiles: &[(i32, i32, PathBuf)], _chrono: &[PathBuf]) -> Option<omsi_map::layers::Layers> {
        None
    }
    /// The game's city map colours (RGBA, 0 - 1): road casing, road, main road, route.
    fn city_map_colors(&self) -> [[f32; 4]; 4] {
        let c = |r: u8, g: u8, b: u8, a: f32| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a];
        [c(30, 30, 30, 0.9), c(92, 92, 92, 1.0), c(112, 112, 112, 1.0), c(214, 48, 40, 1.0)]
    }
    /// A polyline thinned to `tol` metres (the game's own map simplification).
    fn simplify(&self, pts: &[Vec3], _tol: f32) -> Vec<Vec3> {
        pts.to_vec()
    }
}

/// A carriageway of the city map: points in world metres.
pub struct MapRoad {
    pub points: Vec<DVec3>,
    pub width: f32,
    /// A road cars drive far along: drawn brighter.
    pub main: bool,
}

/// A map as the city map draws it (see `Host::city_map`).
#[derive(Default)]
pub struct CityMap {
    pub roads: Vec<MapRoad>,
    /// The linked network behind the roads (a trip's road pieces are lanes of it).
    pub net: omsi_sim::traffic::Network,
    /// Every placed object's world place by id.
    pub places: HashMap<i64, DVec3>,
}

// (an install-once hook: the program's side, the same for every launcher window)
static HOST: OnceLock<Box<dyn Host>> = OnceLock::new();

/// Install the program's side of the launcher (once; later calls are ignored).
pub fn set(host: Box<dyn Host>) {
    let _ = HOST.set(host);
}

struct NoHost;
impl Host for NoHost {}

/// The installed host (or one that has nothing).
pub fn host() -> &'static dyn Host {
    static NONE: NoHost = NoHost;
    HOST.get().map(|h| h.as_ref()).unwrap_or(&NONE)
}
