//! A visible AI bus becomes the player's existing, moving vehicle.

use crate::App;
use crate::humans::{BusId, placed_bus_id};
use omsi_sim::VehicleType;
use omsi_sim::ai_traffic::bus_service::Phase;
use omsi_sim::ai_traffic::control::Removal;
use omsi_ui::tr;
use std::sync::Arc;

fn player_type(root: &std::path::Path, old: &Arc<VehicleType>) -> Result<Arc<VehicleType>, String> {
    let ty = Arc::new(
        VehicleType::load(root, &old.def.path)
            .map_err(|e| format!("Could not load the driver's resources: {e:#}"))?,
    );
    if ty.program.var_names != old.program.var_names
        || ty.program.str_var_names != old.program.str_var_names
        || ty.meshes.len() != old.meshes.len()
    {
        return Err("The vehicle resources changed; reload the map before taking it over".into());
    }
    Ok(ty)
}

impl App {
    pub(super) fn take_ai_bus(&mut self, id: u64) -> Result<(), String> {
        if self.view != "free" || self.cam.ego {
            return Err("AI buses can only be taken over in F4 free view".into());
        }
        if self.net.lan.is_some() {
            return Err("Vehicle handover is only available in single-player".into());
        }
        let world = self.world.clone().ok_or("The world is not ready")?;
        let car = self
            .session
            .traffic
            .as_ref()
            .and_then(|t| t.cars.iter().find(|c| c.id == id))
            .ok_or("The selected AI bus is no longer available")?;
        if !super::road_bus(&car.vehicle.ty) {
            return Err("This AI vehicle cannot be driven by a player".into());
        }
        // Loading is fallible, so finish it before any ownership or timetable changes.
        let ty = player_type(&self.args.root, &car.vehicle.ty)?;
        let parts: Vec<_> = car
            .vehicle
            .trailers
            .iter()
            .map(|t| player_type(&self.args.root, &t.ty))
            .collect::<Result<_, _>>()?;
        let scheme = car.scheme;
        let next_stop = car.vehicle.host.tt_busstop_index.max(0) as usize;
        let at_stop = car
            .bus
            .as_ref()
            .is_some_and(|b| matches!(b.phase, Phase::Boarding | Phase::Waiting | Phase::Closing));
        let delay = car.vehicle.host.tt_delay as f64;
        let gone = car.gone;
        if self.renderer.is_none() || self.scene.is_none() {
            return Err("The scene is not ready".into());
        }
        let now = self.session.traffic.as_ref().unwrap().day_time;
        let reservation = self
            .session
            .schedule
            .as_ref()
            .and_then(|s| s.duty_reservation());
        let mut duty = if !gone {
            match self.session.schedule.as_mut() {
                Some(s) => s.take_car_duty(&*world.data(), id, now)?,
                None => None,
            }
        } else {
            None
        };
        let traffic = self
            .session
            .traffic
            .as_mut()
            .expect("the selected car belongs to traffic");
        let car = traffic
            .sim
            .take_off_id(id, Removal::TakenOver)
            .expect("preflight kept the selected car");
        traffic.drop_sounds(id);
        if let Some(s) = self.session.schedule.as_mut() {
            s.forget_car(id);
            s.forget_route(id);
        }
        let renderer = self.renderer.as_ref().unwrap();
        let scene = self.scene.as_mut().unwrap();
        traffic
            .events
            .push(omsi_sim::view_events::TrafficEvent::Released { id });
        crate::view_sync::apply_traffic(
            traffic,
            &mut self.gfx.sim_view.traffic,
            &world,
            renderer,
            scene,
        );
        let mut vehicle = car.vehicle;
        vehicle.ty = ty.clone();
        for (part, ty) in vehicle.trailers.iter_mut().zip(parts) {
            part.ty = ty;
        }
        vehicle.physics.speed = car.state.speed;
        vehicle.rigid = None;
        vehicle.collision = self
            .settings
            .collision_objects
            .then(|| world.data.peek().lists.collision.clone());
        vehicle.wheel_walls = self.settings.collision_objects;
        vehicle.contact = Some(Arc::new(crate::scene::LiveGround(world.data.clone())));
        vehicle.host.auto_clutch = if self.settings.auto_clutch { 1.0 } else { 0.0 };
        vehicle.set_cab_wheel_rotation(self.settings.cab_wheel_rotation);
        if self.args.physics != "simple" {
            vehicle.enable_rigid_body();
        }
        if let Some(d) = duty.as_mut() {
            d.resume_from_ai(next_stop, at_stop, delay, vehicle.position);
        }
        // Replacement GPU textures must receive the unchanged destination strings too.
        for texture in vehicle.text_textures.iter_mut().chain(
            vehicle.trailers.iter_mut().flat_map(|part| part.text_textures.iter_mut()),
        ) {
            texture.invalidate_upload();
        }
        let render = world.add_vehicle(renderer, scene, &ty, scheme);
        let trailer_renders = vehicle
            .trailers
            .iter()
            .map(|t| {
                world.add_vehicle_part(
                    renderer,
                    scene,
                    &t.ty,
                    scheme.filter(|i| *i < t.ty.paint_schemes.len()),
                    &render,
                )
            })
            .collect();
        let bindings =
            omsi_content::KeyboardCfg::load(&crate::startup::keyboard_cfg(&self.args.root))
                .map(|k| k.with_game_defaults().vehicles)
                .unwrap_or_default();
        let mut p =
            crate::spawn::new_player(vehicle, render, trailer_renders, bindings, false, &ty);
        p.driver = crate::driver::DriverFigure::new(&world, renderer, scene, &p.vehicle, 0);
        let previous = self.player.take();
        if let Some(mut old) = previous {
            old.parked_duty = Some(super::SavedDuty {
                duty: self.session.duty.take(),
                reservation,
            });
            old.axes.release_all();
            old.release();
            if let (Some(audio), Some(mut sounds)) = (self.sound.audio.as_ref(), old.sounds.take())
            {
                sounds.stop_all(audio);
            }
            if let Some(driver) = old.driver.as_mut() {
                driver.hide(renderer, scene);
            }
            if let Some(h) = self.session.humans.as_mut() {
                h.remap_bus(BusId::Player, BusId::Ai(placed_bus_id(old.uid)));
            }
            self.session.placed.push(old);
        }
        if let Some(h) = self.session.humans.as_mut() {
            h.avatar_remove(crate::on_foot::AVATAR_KEY);
            h.remap_bus(BusId::Ai(id), BusId::Player);
            h.rebind_player_cabin(&mut p.vehicle);
        }
        if let Some(audio) = self.sound.audio.as_ref() {
            p.load_sounds(audio);
        }
        self.session.on_foot = None;
        self.session.safe_pose = None;
        self.session.safe_age = 0.0;
        self.session.pumping = None;
        self.session.duty_places = false;
        self.session.journey = None;
        self.args.line = duty.as_ref().map(|d| d.line.clone());
        self.args.tour = duty.as_ref().map(|d| d.tour.clone());
        if duty.is_none() {
            if let Some(s) = self.session.schedule.as_mut() {
                s.restore_duty_reservation(None);
            }
        }
        self.session.duty = duty;
        self.view = "driver".into();
        self.cam.ego = false;
        self.cam.look = (0.0, 0.0);
        if let Some(camera) = self.camera.as_ref() {
            self.camera = Some(p.camera("driver", camera));
        }
        self.player = Some(p);
        self.sync_retained_reservations();
        self.input.dragging = false;
        self.service_msg = Some((
            tr("You are now driving the selected AI bus").into_owned(),
            5.0,
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn takeover_initializes_the_player_driver_like_normal_spawning() {
        let source = include_str!("take.rs").split("#[cfg(test)]").next().unwrap();
        let initialized = source.split("crate::spawn::new_player(").nth(1).unwrap();
        assert!(initialized.contains("p.driver = crate::driver::DriverFigure::new("));
    }

    #[test]
    fn retaining_a_bus_does_not_discard_its_driver_render_handle() {
        let source = include_str!("take.rs").split("#[cfg(test)]").next().unwrap();
        assert!(!source.contains("old.driver.take()"));
        assert!(source.contains("old.driver.as_mut()"));
    }
}
