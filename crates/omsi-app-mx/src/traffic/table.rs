//! The traffic table the simulation writes (see `omsi_net::traffic_table`): after each step of
//! the traffic, every car as one row. The LAN host's clients are told from it
//! (`lan_world`); nothing in the simulation reads it, so it is only written while the game
//! hosts a LAN session.

use super::*;
use omsi_net::traffic_table::{Kind, Look, Meta, Row};

impl Traffic {
    /// Write every car of the simulation into the table; the cars no longer there are over.
    /// Not for a client, whose cars are the host's rows (`lan_world`).
    pub(crate) fn write_table(&mut self) {
        if self.sim.mirror || !self.table_on {
            if !self.table.is_empty() {
                self.table.clear();
            }
            return;
        }
        let root = &self.table_root;
        let night = self.sim.night;
        // (a bus's sign changes once in a while: the table's copy is looked at once a second)
        let refresh = self.sim.time - self.table_meta_t >= 1.0;
        if refresh {
            self.table_meta_t = self.sim.time;
        }
        self.table.begin(self.sim.time as f64);
        for c in &self.sim.cars {
            let v = &c.vehicle;
            let kind = if c.is_bus() {
                Kind::Bus
            } else if c.is_rail() {
                Kind::Rail
            } else {
                Kind::Car
            };
            let at_station = if c.at_station() {
                1
            } else if !v.station_released() {
                -1
            } else {
                0
            };
            let row = Row::from_state(
                [v.position.x, v.position.y, v.position.z],
                v.heading,
                v.pitch as f64,
                v.bank as f64,
                c.state.speed as f64,
                c.body.steer as f64,
                c.state.blinker.clamp(0, 3) as u8,
                c.state.braking,
                night,
                at_station,
            );
            let look = || Look {
                file: omsi_lan::mirror::relative_file(&v.ty.def.path, root),
                // (the paint scheme it is drawn with: a host's cars are all its own, never a
                // LAN mirror's, whose `scheme` is unset)
                scheme: c.scheme.map(|s| s.min(254) as u8),
                paint: c.scheme.and_then(|s| v.ty.paint_schemes.get(s)).map(|p| p.name.clone()).unwrap_or_default(),
            };
            let meta = || if c.is_bus() { crate::lan_world::car_meta(v) } else { Meta::default() };
            let Some(id) = self.table.upsert(c.id, kind, look, meta, row) else { continue };
            if c.is_bus() && refresh {
                self.table.set_meta(id, &crate::lan_world::car_meta(v));
            }
        }
        for id in self.table.finish() {
            log::debug!("traffic table: row {id} is over");
        }
    }
}
