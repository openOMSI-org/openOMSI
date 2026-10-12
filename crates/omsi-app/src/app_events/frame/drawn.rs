use super::*;
use crate::on_foot::AVATAR_KEY;
use omsi_sim::people::{remote_bus_player, BusId};

impl App {
    pub(super) fn frame_drawn(&mut self) {
        let sim = self.player_to_drawn_pose();
        let behind = self.drawn_behind();
        if let (Some(r), Some(scene)) = (self.renderer.as_ref(), self.scene.as_mut()) {
            if let Some(t) = self.session.traffic.as_mut() {
                t.sim.show_between_at(behind);
                t.follow_drawn(&self.gfx.sim_view.traffic, r, scene);
            }
            let (traffic, player, remotes) = (self.session.traffic.as_ref(), self.player.as_ref(), &self.net.remotes.remotes);
            let vehicle_of = |b: BusId| match b {
                BusId::Player => player.map(|p| &p.vehicle),
                BusId::Ai(id) => match remote_bus_player(id) {
                    Some(who) => remotes.get(&who).map(|rv| rv.vehicle()),
                    None => traffic.and_then(|t| t.sim.cars.iter().find(|c| c.id == id)).map(|c| &c.vehicle),
                },
            };
            let avatar = self.session.humans.as_ref().and_then(|h| h.avatars.get(&AVATAR_KEY).copied());
            let moved = self.gfx.sim_view.people.follow_buses(vehicle_of, avatar, r, scene);
            if let (Some(h), Some(w), Some(p)) = (self.session.humans.as_mut(), self.world.as_ref(), self.player.as_ref()) {
                h.sync_money(&mut self.gfx.sim_view.people, w, r, scene, &p.vehicle);
            }
            if let (Some(eye), Some(cam), Some(d)) = (self.session.on_foot.as_ref().and_then(|f| f.eye_aboard()), self.camera.as_mut(), moved) {
                cam.position = eye + d;
            }
            if let Some(t) = self.session.traffic.as_mut() {
                t.sim.show_actual();
            }
        }
        self.player_back_to_sim_pose(sim);
    }
}
