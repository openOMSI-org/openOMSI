//! Coupling and uncoupling a trailer by hand.

use super::*;

impl App {
    /// Couple a standing vehicle to the back of the one driven: its front coupling within
    /// 2.5 m of the train's rear coupling, facing the same way.
    pub(crate) fn couple(&mut self) {
        let (Some(w), Some(r), Some(scene), Some(p)) = (self.world.clone(), self.renderer.as_ref(), self.scene.as_mut(), self.player.as_mut()) else { return };
        // the train's rear coupling in the world
        let rear = match p.vehicle.trailers.last() {
            Some(t) => {
                let c = if t.reversed { t.ty.def.coupling_front.as_ref() } else { t.ty.def.coupling_back.as_ref() };
                c.map(|c| (t.world_transform().transform_point3(glam::Vec3::from(c.pos)), t.heading))
            }
            None => p.vehicle.ty.def.coupling_back.as_ref().map(|c| (p.vehicle.world_transform().transform_point3(glam::Vec3::from(c.pos)), p.vehicle.heading)),
        };
        let Some((rear, rear_heading)) = rear else {
            self.service_msg = Some(("This vehicle has no coupling at its back".into(), 4.0));
            return;
        };
        let found = self.session.placed.iter().position(|q| {
            let Some(c) = q.vehicle.ty.def.coupling_front.as_ref() else { return false };
            let front = q.vehicle.world_transform().transform_point3(glam::Vec3::from(c.pos));
            let dh = ((q.vehicle.heading - rear_heading + 540.0).rem_euclid(360.0) - 180.0).abs();
            (front - rear).truncate().length() < 2.5 && dh < 35.0
        });
        let Some(k) = found else {
            self.service_msg = Some(("Nothing to couple: back up to a trailer's coupling (within 2.5 m, in line)".into(), 4.0));
            return;
        };
        let mut q = self.session.placed.remove(k);
        let ty = q.vehicle.ty.clone();
        if let Some(mut tv) = q.trolley_view.take() {
            tv.release(r, scene);
        }
        if let (Some(a), Some(mut ss)) = (self.sound.audio.as_ref(), q.sounds) {
            ss.stop_all(a);
        }
        w.release_vehicle(r, scene, q.render);
        for tr in q.trailer_renders {
            w.release_vehicle(r, scene, tr);
        }
        p.vehicle.attach_trailer_ex(ty.clone(), false);
        p.trailer_renders.push(w.add_vehicle_part(r, scene, &ty, None, &p.render));
        p.hand_coupled += 1;
        self.service_msg = Some((omsi_ui::i18n::tr_fill("Coupled: {vehicle}", &[("{vehicle}", &format!("{} {}", ty.def.manufacturer, ty.def.type_name))]), 3.0));
    }

    /// Uncouple the last part coupled by hand: it stays where it is, a vehicle of its own.
    pub(crate) fn uncouple(&mut self) {
        let (Some(w), Some(r), Some(scene)) = (self.world.clone(), self.renderer.as_ref(), self.scene.as_mut()) else { return };
        let Some(p) = self.player.as_mut() else { return };
        if p.hand_coupled == 0 {
            self.service_msg = Some(("Nothing coupled by hand (an articulated bus's rear section stays)".into(), 4.0));
            return;
        }
        let Some(t) = p.vehicle.detach_last_trailer() else { return };
        if let Some(tr) = p.trailer_renders.pop() {
            w.release_vehicle(r, scene, tr);
        }
        p.hand_coupled -= 1;
        let one = Args {
            bus: Some(t.ty.def.path.to_string_lossy().to_string()),
            spawn: Some(format!("{},{},{},{}", t.position.x, t.position.y, t.heading, t.position.z)),
            situation_vars: Vec::new(),
            situation_strvars: Vec::new(),
            situation_odometer_km: None,
            situation_others: Vec::new(),
            line: None,
            tour: None,
            trip: None,
            autostart: false,
            paint: None,
            keep_fuel: true,
            ..self.args.clone()
        };
        match spawn_player(&one, &w, r, scene) {
            Ok(Some(q)) => {
                self.session.placed.push(q);
                self.service_msg = Some(("Uncoupled".into(), 3.0));
            }
            Ok(None) => {}
            Err(e) => log::warn!("uncoupled part: {e:#}"),
        }
    }
}
