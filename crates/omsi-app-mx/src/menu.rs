//! Start menu: map, vehicle, time, traffic, passengers, schedule - keyboard driven,
//! drawn as HUD text before the world is loaded.

use std::path::{Path, PathBuf};

pub struct Menu {
    pub maps: Vec<(String, String)>,
    pub vehicles: Vec<(String, String)>,
    /// (manufacturer, type) of each of `vehicles`, as OMSI's [friendlyname] gives them.
    pub vehicle_meta: Vec<(String, String)>,
    pub map: usize,
    pub vehicle: usize,
    pub hour: i32,
    pub traffic: usize,
    pub passengers: bool,
    pub schedule: bool,
    pub row: usize,
    pub start: bool,
    /// Situations (name, file); index 0 = none.
    pub situations: Vec<(String, String)>,
    pub situation: usize,
    /// Weather files (name, path); index 0 = the map's default.
    pub weathers: Vec<(String, String)>,
    pub weather: usize,
    /// Day of the year, so winter can be chosen without the command line.
    pub day: i32,
}

impl Menu {
    pub fn new(root: &Path, default_map: &str) -> Menu {
        // the folders of every content root: the installation, the installed mods and the
        // archives read in place
        let merged = |rel: &str| {
            let mut dirs = omsi_cfg::read_dir_merged(rel);
            if dirs.is_empty() {
                dirs = omsi_cfg::vfs::read_dir_paths(&root.join(rel));
            }
            dirs.retain(|d| omsi_cfg::vfs::is_dir(d));
            dirs.sort_by_key(|d| d.file_name().map(|n| n.to_ascii_lowercase()));
            dirs
        };
        let mut maps = Vec::new();
        for d in merged("maps") {
            let cfg = omsi_cfg::resolve_path(&d, "global.cfg");
            if !omsi_cfg::vfs::is_file(&cfg) {
                continue;
            }
            let name = omsi_map::GlobalCfg::load(&cfg).map(|g| g.name.clone()).ok().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| d.file_name().unwrap().to_string_lossy().into_owned());
            maps.push((name, format!("maps/{}/global.cfg", d.file_name().unwrap().to_string_lossy())));
        }
        // every content root's vehicle folders (installed mods too), and of their files only
        // what OMSI offers: those with a [friendlyname] - never an articulated bus's rear
        // section, which comes with its front
        let mut vehicles = Vec::new();
        let mut vehicle_meta = Vec::new();
        for d in merged("Vehicles") {
            // the folder's files over all roots: a repaint installed as a mod brings only
            // textures and must neither hide the installation's bus nor add one of its own
            let folder = d.file_name().unwrap().to_string_lossy().to_string();
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut files: Vec<PathBuf> = Vec::new();
            for dd in omsi_cfg::mirrored_dirs(&d) {
                for f in omsi_cfg::vfs::read_dir_paths(&dd) {
                    if seen.insert(f.file_name().unwrap().to_string_lossy().to_ascii_lowercase()) {
                        files.push(f);
                    }
                }
            }
            files.sort_by_key(|f| f.file_name().unwrap().to_string_lossy().to_ascii_lowercase());
            for (f, v) in omsi_vehicle::vehicle::offered_vehicles(&files) {
                let rel = format!("Vehicles/{}/{}", folder, f.file_name().unwrap().to_string_lossy());
                let stem = f.file_stem().unwrap().to_string_lossy().to_string();
                let name = Some(format!("{} {}", v.manufacturer, v.type_name).trim().to_string()).filter(|n| !n.is_empty()).unwrap_or(stem);
                vehicle_meta.push((v.manufacturer.trim().to_string(), Some(v.type_name.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| name.clone())));
                vehicles.push((name, rel));
            }
        }
        log::info!("menu: {} vehicles offered", vehicles.len());
        let mut situations = vec![("-".to_string(), String::new())];
        // the installation's and the content folder's (where the game saves them)
        let mut files: Vec<PathBuf> = omsi_cfg::read_dir_merged("Situations");
        if files.is_empty() {
            files = std::fs::read_dir(root.join("Situations")).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        }
        files.retain(|p| p.extension().map(|e| e.eq_ignore_ascii_case("osn")).unwrap_or(false));
        files.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
        {
            for f in files {
                let name = omsi_content::Situation::load(&f).map(|s| s.name).ok().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| f.file_stem().unwrap().to_string_lossy().into_owned());
                situations.push((name, format!("Situations/{}", f.file_name().unwrap().to_string_lossy())));
            }
        }
        let mut weathers = vec![("default".to_string(), String::new())];
        if let Ok(rd) = std::fs::read_dir(root.join("Weather")) {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("owt")).unwrap_or(false)).collect();
            files.sort();
            for f in files {
                let name = omsi_ui::tr(&omsi_launcher_lib::weather_name(&f)).into_owned();
                weathers.push((name, format!("Weather/{}", f.file_name().unwrap().to_string_lossy())));
            }
        }
        let map = maps.iter().position(|m| m.1.eq_ignore_ascii_case(default_map)).unwrap_or(0);
        let vehicle = vehicles.iter().position(|v| v.1.to_ascii_lowercase().contains("sd80")).unwrap_or(0);
        Menu { maps, vehicles, vehicle_meta, map, vehicle, hour: 9, traffic: 30, passengers: true, schedule: true, row: 0, start: false, situations, situation: 0, weathers, weather: 0, day: 150 }
    }

    pub fn lines(&self) -> Vec<String> {
        let mark = |r: usize| if self.row == r { ">" } else { " " };
        vec![
            "openOMSI".to_string(),
            String::new(),
            format!("{} Map:        {}", mark(0), self.maps.get(self.map).map(|m| m.0.as_str()).unwrap_or("-")),
            format!("{} Vehicle:    {}", mark(1), self.vehicles.get(self.vehicle).map(|v| v.0.as_str()).unwrap_or("-")),
            format!("{} Time:       {:02}:00", mark(2), self.hour),
            format!("{} Traffic:    {} vehicles", mark(3), self.traffic),
            format!("{} Passengers: {}", mark(4), if self.passengers { "on" } else { "off" }),
            format!("{} Timetable:  {}", mark(5), if self.schedule { "on" } else { "off" }),
            format!("{} Weather:    {}", mark(6), self.weathers.get(self.weather).map(|w| w.0.as_str()).unwrap_or("-")),
            format!("{} Date:       {}  ({})", mark(7), self.date_text(), self.season_text()),
            format!("{} Situation:  {}", mark(8), self.situations.get(self.situation).map(|s| s.0.as_str()).unwrap_or("-")),
            String::new(),
            "Arrow keys choose, Enter starts, Esc quits".to_string(),
        ]
    }

    /// The chosen day of the year as a date in the map's default year.
    pub fn date_text(&self) -> String {
        let months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut d = self.day.max(1);
        for (i, m) in months.iter().enumerate() {
            if d <= *m {
                return format!("{d:02}.{:02}.", i + 1);
            }
            d -= m;
        }
        "31.12.".to_string()
    }

    pub fn season_text(&self) -> &'static str {
        match self.day {
            1..=59 => "winter",
            60..=120 => "spring",
            121..=243 => "summer",
            244..=304 => "autumn",
            _ => "winter",
        }
    }

    /// Handle a key (winit key code name); returns true when consumed.
    pub fn key(&mut self, code: winit::keyboard::KeyCode) {
        use winit::keyboard::KeyCode as K;
        let dir: i32 = match code {
            K::ArrowLeft => -1,
            K::ArrowRight => 1,
            _ => 0,
        };
        match code {
            K::ArrowUp => self.row = (self.row + 8) % 9,
            K::ArrowDown => self.row = (self.row + 1) % 9,
            K::Enter | K::NumpadEnter => self.start = true,
            _ => {}
        }
        if dir != 0 {
            let step = |v: usize, n: usize| -> usize { if n == 0 { 0 } else { ((v as i32 + dir).rem_euclid(n as i32)) as usize } };
            match self.row {
                0 => self.map = step(self.map, self.maps.len()),
                1 => self.vehicle = step(self.vehicle, self.vehicles.len()),
                2 => self.hour = (self.hour + dir).rem_euclid(24),
                3 => self.traffic = (self.traffic as i32 + dir * 10).clamp(0, 200) as usize,
                4 => self.passengers = !self.passengers,
                5 => self.schedule = !self.schedule,
                6 => self.weather = step(self.weather, self.weathers.len()),
                7 => self.day = (self.day + dir * 15).rem_euclid(365).max(1),
                8 => self.situation = step(self.situation, self.situations.len()),
                _ => {}
            }
        }
    }
}