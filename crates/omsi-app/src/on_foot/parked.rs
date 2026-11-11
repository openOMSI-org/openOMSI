//! Taking one of a server's parked buses (`omsi_lan::parked`): G at its driver's door.
//!
//! One taking is one transfer (`ParkedTransfer`). Every message of it names the bus and the
//! transfer's own number, so an answer to an earlier asking that comes late is none of it.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Asked,
    Granted,
    Took,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParkedTransfer {
    bus: u32,
    op: u32,
    stage: Stage,
    since: f32,
    took_for: f32,
}

/// A transfer the server does not answer, or whose bus never came into the world, is given
/// up after this long (the server frees the bus by itself, `omsi_lan::parked`).
const GIVEN_UP_AFTER: f32 = 30.0;
/// A bus taken that the server never says is ours (it keeps a bus for 120 s) is said no
/// more after this long.
const UNHEARD_AFTER: f32 = 150.0;

#[derive(Debug, PartialEq)]
enum Heard {
    Other,
    /// Of another transfer (an earlier asking, answered late), or out of turn: left aside.
    Stale,
    Grant,
    Deny,
    Yours,
}

impl ParkedTransfer {
    fn ask(bus: u32) -> Self {
        // (numbers no earlier transfer of this game had: counted on from the clock at its start)
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1);
        let _ = NEXT.compare_exchange(0, seed | 1, std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed);
        let op = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ParkedTransfer { bus, op, stage: Stage::Asked, since: 0.0, took_for: 0.0 }
    }

    pub(crate) fn took(&mut self, bus: u32) {
        if self.bus == bus && self.stage == Stage::Granted {
            (self.stage, self.since) = (Stage::Took, f32::MAX);
        }
    }

    /// Past the asking no other transfer starts: giving this one up would leave the server
    /// without the word that we drive the bus, and it would give it to the next player.
    fn holds(&self) -> bool {
        self.stage != Stage::Asked
    }

    fn hear(&self, text: &str) -> Heard {
        let mut words = text.split_whitespace();
        let (Some(verb), bus, op) = (words.next(), words.next(), words.next()) else { return Heard::Other };
        let heard = match verb {
            "park-grant" => Heard::Grant,
            "park-deny" => Heard::Deny,
            "park-yours" => Heard::Yours,
            _ => return Heard::Other,
        };
        let ours = bus.and_then(|b| b.parse().ok()) == Some(self.bus) && op.and_then(|o| o.parse().ok()) == Some(self.op);
        let due = match heard {
            Heard::Yours => self.stage == Stage::Took,
            _ => self.stage == Stage::Asked,
        };
        if ours && due { heard } else { Heard::Stale }
    }
}

impl App {
    pub(super) fn parked_cab_near(&mut self, pos: DVec3) -> Option<u32> {
        let h = self.session.humans.as_mut()?;
        let mut best: Option<(u32, f64)> = None;
        // (one that is drawn as itself: a box standing in for it has no door and no wheel)
        for (id, rv) in self.net.remotes.remotes.iter().filter(|(id, rv)| omsi_net::is_parked(**id) && !rv.stand_in && !rv.fallback) {
            let v = rv.vehicle();
            let Some(door) = h.vehicle_driver_door(v).or_else(|| h.driver_stand(v).and_then(|l| h.vehicle_cabin_world(v, l))) else { continue };
            let dist = (door - pos).truncate().length();
            if dist < DOOR_REACH + 0.6 && best.is_none_or(|b| dist < b.1) {
                best = Some((*id, dist));
            }
        }
        best.map(|b| b.0)
    }

    pub(super) fn ask_for_parked(&mut self, id: u32) {
        if self.menus.pending_placement.is_some() {
            self.service_msg = Some(("A vehicle is still loading".into(), 4.0));
            return;
        }
        if self.menus.parked_transfer.as_ref().is_some_and(ParkedTransfer::holds) {
            self.service_msg = Some((omsi_ui::tr("The server is still confirming the bus you took").into_owned(), 4.0));
            return;
        }
        let Some(l) = self.net.lan.as_mut() else { return };
        let t = ParkedTransfer::ask(id);
        l.command(1, &format!("park-take {} {}", t.bus, t.op));
        self.menus.parked_transfer = Some(t);
    }

    /// A command may be lost on its way: said every second until the server answers.
    pub(crate) fn tell_took_parked(&mut self, dt: f32) {
        let Some(t) = self.menus.parked_transfer.as_mut() else { return };
        let Some(l) = self.net.lan.as_mut() else {
            self.menus.parked_transfer = None;
            return;
        };
        match t.stage {
            Stage::Took if t.took_for > UNHEARD_AFTER => self.menus.parked_transfer = None,
            Stage::Took if t.since >= 1.0 => {
                (t.since, t.took_for) = (0.0, t.took_for + dt);
                l.command(1, &format!("park-took {} {}", t.bus, t.op));
            }
            Stage::Took => (t.since, t.took_for) = (t.since + dt, t.took_for + dt),
            _ => {
                t.since += dt;
                if t.since > GIVEN_UP_AFTER {
                    self.menus.parked_transfer = None;
                }
            }
        }
    }

    pub(crate) fn parked_answer(&mut self, text: &str) -> bool {
        let Some(t) = self.menus.parked_transfer.as_mut() else { return text.starts_with("park-") };
        let id = t.bus;
        match t.hear(text) {
            Heard::Other => return false,
            Heard::Stale => return true,
            Heard::Yours => {
                self.menus.parked_transfer = None;
                return true;
            }
            Heard::Deny => {
                self.menus.parked_transfer = None;
                self.service_msg = Some((omsi_ui::tr("Somebody else has just taken this bus").into_owned(), 4.0));
                return true;
            }
            Heard::Grant => (t.stage, t.since) = (Stage::Granted, 0.0),
        }
        let (Some(w), Some(r), Some(rv)) = (self.world.clone(), self.renderer.as_ref(), self.net.remotes.remotes.get(&id)) else { return true };
        let v = rv.vehicle();
        // what the server tells of it (`rv.last` is the state drawn: it has no bus and no paint)
        let told = self.net.lan.as_ref().and_then(|l| l.peers().find(|p| p.pose.id == id)).map(|p| (p.pose.bus.clone(), p.pose.paint.clone()));
        let bus = told.as_ref().map(|t| t.0.clone()).filter(|b| !b.is_empty()).unwrap_or_else(|| omsi_lan::game::content_relative(&v.ty.def.path, &self.args.root));
        let (at, heading) = (v.position, v.heading);
        // in the paint it is drawn in here (a kept bus has the files of that paint alone: in
        // "the model's own textures" it stood there without its paint and with blind windows)
        let drawn = v.host.paint_scheme.flatten().and_then(|i| v.ty.paint_schemes.get(i)).map(|s| s.name.clone());
        let paint = drawn.or(told.map(|t| t.1)).filter(|p| !p.is_empty());
        let args = crate::app_impl::placement_args(&self.args, bus.clone(), format!("{},{},{},{}", at.x, at.y, heading, at.z), paint, None);
        let mut prepared = crate::spawn::PreparedPlayer::of_type(&self.args.root, v.ty.clone(), v.host.font_lib.clone());
        prepared.prefetch(w.placement_prefetch(r), args.paint.as_deref());
        let (tx, receiver) = std::sync::mpsc::channel();
        let _ = tx.send(Ok(prepared));
        let replace = self.player.as_ref().map(|p| p.uid);
        self.menus.pending_placement = Some(crate::spawn::PendingPlacement { receiver, world: w, args, name: bus, replace, reload: false, heading, taken: Some(id) });
        true
    }
}

#[cfg(test)]
mod tests;
