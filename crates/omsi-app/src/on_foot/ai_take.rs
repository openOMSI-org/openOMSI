//! Taking over a bus of the AI traffic (#3974), as OMSI 2 lets the player: G at the driver's
//! door of a bus that stands. The bus is read in full (an AI type has no meshes for a cab
//! that is driven) on a worker and put where the AI bus stood, in its paint and with its
//! depot file; the AI bus leaves the traffic, and the player's own bus, if any, stays where it
//! is, among the vehicles to switch to.

use super::*;

/// The fastest an AI bus may still roll to be taken over (m/s): one that stands at a stop or
/// a light.
const STANDING: f32 = 0.5;

impl App {
    /// The AI bus whose driver's door is in reach of `pos`, standing.
    pub(super) fn ai_cab_near(&mut self, pos: DVec3) -> Option<u64> {
        // (a LAN client draws the host's traffic: none of it is its own to take)
        if self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
            return None;
        }
        let t = self.session.traffic.as_ref()?;
        let h = self.session.humans.as_mut()?;
        let mut best: Option<(u64, f64)> = None;
        for c in t.cars.iter().filter(|c| !c.gone && c.is_bus() && c.state.speed.abs() < STANDING) {
            let Some(door) = h.vehicle_driver_door(&c.vehicle) else { continue };
            let d = (door - pos).truncate().length();
            if d < DOOR_REACH && best.is_none_or(|b| d < b.1) {
                best = Some((c.id, d));
            }
        }
        best.map(|b| b.0)
    }

    /// Take over the AI bus `id`: read it as a player's bus where it stands, and take it off
    /// the road.
    pub(super) fn take_ai_bus(&mut self, id: u64) {
        if self.menus.pending_placement.is_some() {
            self.service_msg = Some(("A vehicle is still loading".into(), 4.0));
            return;
        }
        let (Some(w), Some(r)) = (self.world.clone(), self.renderer.as_ref()) else { return };
        let Some(c) = self.session.traffic.as_ref().and_then(|t| t.cars.iter().find(|c| c.id == id)) else { return };
        let v = &c.vehicle;
        let bus = omsi_lan::game::content_relative(&v.ty.def.path, &self.args.root);
        let paint = v.host.paint_scheme.flatten().and_then(|i| v.ty.paint_schemes.get(i)).map(|s| s.name.clone()).filter(|p| !p.is_empty());
        let hof = v.host.hof.as_ref().map(|h| h.path.to_string_lossy().into_owned());
        let (at, heading) = (v.position, v.heading);
        let args = crate::app_impl::placement_args(&self.args, bus.clone(), format!("{},{},{},{}", at.x, at.y, heading, at.z), paint, hof);
        let prefetch = w.placement_prefetch(r);
        let worker_args = args.clone();
        let (tx, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new().name("ai-takeover".into()).spawn(move || {
            let result = crate::spawn::PreparedPlayer::load(&worker_args).map(|mut prepared| {
                prepared.prefetch(prefetch, worker_args.paint.as_deref());
                prepared
            });
            let _ = tx.send(result);
        });
        if let Err(e) = worker {
            self.service_msg = Some((format!("Could not place {bus} ({e})"), 5.0));
            return;
        }
        // the AI bus leaves the road (and its timetable, and its passengers their seats)
        if let (Some(t), Some(r), Some(sc)) = (self.session.traffic.as_mut(), self.renderer.as_ref(), self.scene.as_mut()) {
            if t.remove_car(id) {
                crate::view_sync::apply_traffic(t, &mut self.gfx.sim_view.traffic, &w, r, sc);
            }
        }
        if let Some(s) = self.session.schedule.as_mut() {
            s.sim.forget_car(id);
        }
        if let Some(h) = self.session.humans.as_mut() {
            h.evict(BusId::Ai(id), &w);
        }
        log::info!("taking over AI bus {id}: {bus} at ({:.1}, {:.1})", at.x, at.y);
        self.menus.pending_placement = Some(crate::spawn::PendingPlacement { receiver, world: w, args, name: bus, replace: None, reload: false, heading, taken: None, drive: true });
        self.service_msg = Some((omsi_ui::tr("Taking over the bus...").into_owned(), 4.0));
    }
}
