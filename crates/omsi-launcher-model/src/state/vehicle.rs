//! Vehicle selection defaults: depot and fleet number.
use super::*;

impl State {
    /// The depot file a bus uses on the chosen map: the map's own when the bus has it (or
    /// has none: the game borrows it), else the bus's first.
    /// The depot file for the chosen bus: the one the map's own buses use on the chosen date
    /// (the chrono scenarios change it: Berlin's 1994 depot has line 137 where 1986's had
    /// 92), which the bus has, else its first.
    pub fn default_hof(&self) -> String {
        let line = if self.choice.free {
            None
        } else {
            self.choice.line.as_deref()
        };
        core::default_hof(self.map(), self.bus(), &self.choice.date, line)
    }

    /// Pick once when the bus changes; both launcher layouts use this stored choice.
    pub(super) fn random_bus_number(&self) -> String {
        self.bus()
            .filter(|v| !v.numbers.is_empty())
            .map(|v| {
                let index = (omsi_net::random_session_id() % v.numbers.len() as u64) as usize;
                v.numbers[index].0.clone()
            })
            .unwrap_or_default()
    }
}

impl State {
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
}
