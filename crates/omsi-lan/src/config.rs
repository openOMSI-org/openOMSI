//! What LAN play takes from the game's command line, as a value the game fills once
//! (`LanConfig`) and takes back where the session changed it (the host's map, date, time
//! and weather a joining player adopts).

use std::path::PathBuf;

/// The game's choices LAN play reads and, for a joining player, changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LanConfig {
    /// The game's folder.
    pub root: PathBuf,
    /// The map (`maps/<name>/global.cfg`, relative to `root`).
    pub map: String,
    /// The weather file or name (None: the map's own).
    pub weather: Option<String>,
    /// The season chosen (None: by date).
    pub season: Option<String>,
    /// The date (`yyyy-mm-dd`), the day of the year, the time of day (`hh:mm[:ss]`).
    pub date: Option<String>,
    pub day_of_year: Option<i32>,
    pub time: String,
    /// The bus, its paint scheme and the driver figure.
    pub bus: Option<String>,
    pub paint: Option<String>,
    pub driver: Option<String>,
    /// The player's name, hosting on this port, joining this session.
    pub lan_name: String,
    pub lan_host: Option<u16>,
    pub lan_join: Option<String>,
    /// Discovered gateway to try if an Auto UDP join gets no answer.
    pub lan_ws_fallback: Option<String>,
    /// A dedicated server (`--server`).
    pub server: bool,
    /// Fatal verification/download error for the selected server vehicle. Never
    /// continue into the world using a different local version after this fails.
    pub mod_fetch_error: Option<String>,
    /// Where the bus starts and what it drives: entry point, spawn, depot, line, tour, trip.
    pub entry: usize,
    pub spawn: Option<String>,
    pub hof: Option<String>,
    pub line: Option<String>,
    pub tour: Option<String>,
    pub trip: Option<String>,
}

impl LanConfig {
    /// The clock the game starts on: the date (or the day of the year) and the time chosen.
    pub fn start_clock(&self) -> omsi_sim::SimClock {
        omsi_sim::SimClock::starting(self.date.as_deref(), self.day_of_year, &self.time)
    }
}
