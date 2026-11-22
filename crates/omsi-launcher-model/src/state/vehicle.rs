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
