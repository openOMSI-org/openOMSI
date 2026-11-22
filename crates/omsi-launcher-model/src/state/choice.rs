use omsi_launcher_lib as core;
use serde::{Deserialize, Serialize};

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
    /// The buses a session we host drives (#1464): what its players may drive, `;`-separated,
    /// bus files or their folders. Empty: every bus.
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
    pub(crate) fn fresh(mut c: Choice) -> Choice {
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
        if let Some(d) =
            super::super::season::launcher_date(&c.season, &c.phase, &c.date, &c.map, true)
        {
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
