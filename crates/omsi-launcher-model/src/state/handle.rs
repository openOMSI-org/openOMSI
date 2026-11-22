use super::*;
use std::time::Instant;

impl State {
    pub(super) fn handle_content_early(
        &mut self,
        mut maps: Vec<core::MapInfo>,
        mut weathers: Vec<core::WeatherInfo>,
    ) {
        if !self.content_first {
            return;
        }
        host().mt_protect(
            &mut maps
                .iter()
                .flat_map(|m| [m.name.as_str(), m.friendly.as_str()]),
        );
        host().mt_protect(&mut weathers.iter().map(|w| w.name.as_str()));
        self.maps = maps;
        self.weathers = weathers;
        self.pick_map();
        self.load_lines();
        self.set_status(
            format!("Reading the buses... ({} maps)", self.maps.len()),
            false,
        );
    }

    pub(super) fn handle_vehicles_read(
        &mut self,
        batch: Vec<core::VehicleInfo>,
        done: usize,
        total: usize,
    ) {
        if !self.content_first {
            return;
        }
        host().mt_protect(
            &mut batch
                .iter()
                .flat_map(|v| {
                    [
                        v.name.as_str(),
                        v.manufacturer.as_str(),
                        v.type_name.as_str(),
                    ]
                })
                .chain(
                    batch
                        .iter()
                        .flat_map(|v| v.paints.iter().map(|p| p.as_str())),
                ),
        );
        self.vehicles.extend(batch);
        self.set_status(
            format!(
                "Reading the vehicle folders: {done} of {total} ({maps} maps, {buses} buses)",
                maps = self.maps.len(),
                buses = self.vehicles.len()
            ),
            false,
        );
    }

    pub(super) fn handle_content_ok(
        &mut self,
        mut maps: Vec<core::MapInfo>,
        mut vehicles: Vec<core::VehicleInfo>,
        mut weathers: Vec<core::WeatherInfo>,
    ) {
        // (names of things, not the interface: never machine-translated)
        host().mt_protect(
            &mut maps
                .iter()
                .flat_map(|m| [m.name.as_str(), m.friendly.as_str()]),
        );
        host().mt_protect(
            &mut vehicles
                .iter()
                .flat_map(|v| {
                    [
                        v.name.as_str(),
                        v.manufacturer.as_str(),
                        v.type_name.as_str(),
                    ]
                })
                .chain(
                    vehicles
                        .iter()
                        .flat_map(|v| v.paints.iter().map(|p| p.as_str())),
                ),
        );
        host().mt_protect(&mut weathers.iter().map(|w| w.name.as_str()));
        let known: std::collections::HashSet<String> = self
            .maps
            .iter()
            .map(|m| m.file.clone())
            .chain(self.vehicles.iter().map(|v| v.file.clone()))
            .chain(self.weathers.iter().map(|w| w.file.clone()))
            .collect();
        if !known.is_empty() {
            for f in maps
                .iter()
                .map(|m| &m.file)
                .chain(vehicles.iter().map(|v| &v.file))
                .chain(weathers.iter().map(|w| &w.file))
            {
                if !known.contains(f) {
                    self.fresh.insert(f.clone(), Instant::now());
                }
            }
        }
        self.set_status(
            format!(
                "Content read: {} maps, {} buses, {} weathers",
                maps.len(),
                vehicles.len(),
                weathers.len()
            ),
            false,
        );
        self.maps = maps;
        self.vehicles = vehicles;
        self.weathers = weathers;
        self.loading_content = false;
        // what was chosen last time, where it still exists
        if !self.vehicles.iter().any(|v| v.file == self.choice.bus) {
            if let Some(v) = self.vehicles.first() {
                self.choice.bus = v.file.clone();
                self.choice.paint.clear();
                self.choice.number = self.random_bus_number();
                self.choice.hof = self.default_hof();
            }
        }
        self.pick_map();
        // (the first reading asked for the lines with the maps already)
        if !std::mem::take(&mut self.content_first)
            || self.lines_for != (self.choice.map.clone(), self.choice.date.clone())
        {
            self.load_lines();
        }
        self.load_args();
        self.load_ibis();
        self.content_done();
    }

    pub(super) fn handle_lines(
        &mut self,
        map: String,
        date: String,
        lines: Result<Vec<core::LineInfo>, String>,
    ) {
        if let Ok(ls) = lines.as_ref() {
            host().mt_protect(
                &mut ls
                    .iter()
                    .flat_map(|l| {
                        l.termini
                            .iter()
                            .map(|t| t.as_str())
                            .chain([l.name.as_str()])
                    })
                    .chain(
                        ls.iter()
                            .flat_map(|l| l.tours.iter().map(|t| t.number.as_str())),
                    ),
            );
        }
        if (map, date) != self.lines_for {
            return;
        }
        self.loading_lines = false;
        match lines {
            Ok(l) => {
                self.lines = l;
                let (n, date) = (self.lines.len(), self.choice.date.clone());
                let mut note = String::new();
                if let Some(line) = self.choice.line.clone() {
                    match self.lines.iter().find(|x| x.name == line) {
                        None => {
                            note = format!(
                                "Timetable of {date}: {n} lines - line {line} does not run that day"
                            );
                            self.choice.line = None;
                            self.choice.tour = None;
                        }
                        Some(l) => {
                            if let Some(t) = &self.choice.tour {
                                match l.tour(t) {
                                    None => self.choice.tour = None,
                                    Some(t) => {
                                        if !t.runs {
                                            note = format!(
                                                "Timetable of {date}: {n} lines - tour {} of line {line} does not run that day ({})",
                                                t.number, t.days
                                            );
                                        }
                                        self.choice.tour = Some(t.id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                let err = !note.is_empty();
                self.set_status(
                    if err {
                        note
                    } else {
                        format!("Timetable of {date}: {n} lines")
                    },
                    err,
                );
            }
            Err(e) => {
                self.lines.clear();
                self.set_status(e, true);
            }
        }
        self.load_ibis();
    }

    pub(super) fn handle_poll(&mut self, r: Result<core::InboxPoll, String>) {
        self.polling = false;
        match r {
            Ok(p) => {
                for s in &p.started {
                    core::log_to_file(&format!("inbox: installing {s}"));
                }
                if !p.started.is_empty() {
                    self.set_status(
                        format!(
                            "Installing {} from the Mods folder...",
                            p.started
                                .iter()
                                .map(|x| x.rsplit('/').next().unwrap_or(x))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        false,
                    );
                }
                let mut installed = false;
                for j in &p.jobs {
                    let was = self
                        .jobs
                        .iter()
                        .find(|x| x.id == j.id)
                        .map(|x| x.finished.is_some())
                        .unwrap_or(false);
                    if j.finished.is_some() && !was && self.stamp.is_some() {
                        if j.state == "done" {
                            installed = true;
                            self.set_status(j.message.clone(), false);
                        } else if j.state == "failed" {
                            self.set_status(format!("{}: {}", j.name, j.message), true);
                        }
                    }
                }
                self.jobs = p.jobs;
                // a game that was running and is not any more: did it end on an error?
                for old in self.instances.iter().filter(|i| i.running) {
                    let still = p.instances.iter().any(|n| n.pid == old.pid && n.running);
                    if !still && !self.stopping.contains(&old.pid) {
                        if let Some(why) = disconnect_of(std::path::Path::new(&old.log)) {
                            core::log_to_file(&format!(
                                "game {} was sent away by its server: {why}",
                                old.pid
                            ));
                            self.disconnected = Some(core::i18n::tr_said(&why));
                        } else if let Some(c) = crash_of(std::path::Path::new(&old.log)) {
                            core::log_to_file(&format!(
                                "game {} ended on an error: {}",
                                old.pid, c.0
                            ));
                            self.crash = Some(c);
                        }
                    }
                }
                self.instances = p.instances;
                // the game started from here is in the list: whether it runs is known
                // (one that ended at once left the launcher blank until the 15 s were out)
                if game_listed(self.launched_pid, &self.instances) {
                    self.launch_hold = None;
                    self.launched_pid = None;
                }
                for i in &self.instances {
                    if !i.running {
                        self.stopping.remove(&i.pid);
                    }
                }
                // (while the lists are read the stamp moves by itself - the cache gains
                // the folders each bus depends on - and every poll started the whole
                // reading over on top of the one going: a big installation never
                // finished. A change then is taken up when the reading is done.)
                let changed = self.stamp.as_ref().map(|s| *s != p.stamp).unwrap_or(false);
                self.stamp = if self.loading_content {
                    None
                } else {
                    Some(p.stamp)
                };
                if changed || installed {
                    if self.loading_content {
                        self.reload_content = true;
                    } else {
                        self.load_content();
                    }
                    self.load_mods();
                }
                for pid in self.open_logs.clone() {
                    self.log_tail(pid);
                }
            }
            Err(e) => core::log_to_file(&format!("poll: {e}")),
        }
    }
}
