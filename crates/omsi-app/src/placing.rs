//! Placing a vehicle with the mouse (Esc menu, Place a vehicle...): the vehicle chosen
//! stands where the cursor points on the ground and follows it, sitting on the surface there
//! (a slope, a kerb); the wheel (or Q / E) turns it, R turns it round, a left click sets it
//! down and Escape takes it away again. Where it would stand in another vehicle, the click
//! is refused and the HUD says why.

use crate::App;
use glam::DVec3;
use omsi_sim::collision::Obb;
use winit::keyboard::KeyCode;

pub(crate) struct Placing {
    /// The placed vehicle being put down (`Player::uid`).
    pub uid: u64,
    pub heading: f64,
    /// Where it stands now, if the cursor points at the ground.
    pub at: Option<DVec3>,
    /// Standing in another vehicle: not to be set down there.
    pub blocked: bool,
}

/// Where the ray from `o` along `d` first meets the ground (the terrain, the streets), up
/// to `max` metres away.
pub(crate) fn ground_hit(w: &crate::scene::World, o: DVec3, d: DVec3, max: f64) -> Option<DVec3> {
    let above = |t: f64| -> Option<bool> {
        let p = o + d * t;
        w.ground_height(p.x, p.y).map(|g| p.z > g)
    };
    let mut t = 0.5;
    let mut last = 0.0;
    while t < max {
        if above(t) == Some(false) {
            // (between the last point above the ground and this one below it)
            let (mut a, mut b) = (last, t);
            for _ in 0..24 {
                let m = (a + b) * 0.5;
                if above(m).unwrap_or(true) {
                    a = m;
                } else {
                    b = m;
                }
            }
            let p = o + d * b;
            return w.ground_height(p.x, p.y).map(|g| DVec3::new(p.x, p.y, g));
        }
        last = t;
        t += (t * 0.01).max(0.25);
    }
    None
}

/// Vehicle `v` put at `at` facing `heading`, standing still.
pub(crate) fn put_vehicle(v: &mut omsi_sim::VehicleInstance, at: DVec3, heading: f64) {
    v.position = at;
    v.heading = heading;
    if let Some(rb) = v.rigid.as_mut() {
        rb.place(at, heading);
    }
    for t in v.trailers.iter_mut() {
        t.realign();
    }
}

/// The vehicle just placed (`uid`) follows the mouse until it is set down.
pub(crate) fn begin_placing(menus: &mut crate::app::MenuState, service_msg: &mut Option<(String, f32)>, uid: u64, heading: f64) {
    menus.placing = Some(Placing { uid, heading, at: None, blocked: false });
    *service_msg = Some(("Placing: point at the ground, mouse wheel or Q / E turns it, R turns it round, click sets it down, Esc takes it away".into(), 30.0));
}

impl App {
    /// Every frame while placing: the vehicle where the cursor points.
    pub(crate) fn placing_frame(&mut self) {
        let Some(pl) = self.menus.placing.as_ref() else { return };
        let (uid, heading) = (pl.uid, pl.heading);
        let Some(k) = self.session.placed.iter().position(|q| q.uid == uid) else {
            self.menus.placing = None;
            return;
        };
        let (Some(w), Some(cam), Some(s)) = (self.world.clone(), self.camera.as_ref(), self.gfx.surface.as_ref()) else { return };
        let (o, d) = self.world_cursor_ray(cam, (s.config.width, s.config.height));
        let hit = ground_hit(&w, o, d.as_dvec3(), 400.0);
        // in another vehicle (the own bus, the traffic, another placed one)?
        let blocked = hit
            .map(|at| {
                let bb = self.session.placed[k].vehicle.ty.def.bounding_box.unwrap_or([2.5, 11.0, 3.0, 0.0, 0.0, 1.5]);
                let me = Obb::from_box(bb, at, heading);
                let mut others: Vec<Obb> = Vec::new();
                let mut add = |v: &omsi_sim::VehicleInstance| {
                    if (v.position.truncate() - at.truncate()).length() < 40.0 {
                        if let Some(b) = v.ty.def.bounding_box {
                            others.push(Obb::from_box(b, v.position, v.heading));
                        }
                    }
                };
                if let Some(p) = self.player.as_ref() {
                    add(&p.vehicle);
                }
                for (j, q) in self.session.placed.iter().enumerate() {
                    if j != k {
                        add(&q.vehicle);
                    }
                }
                if let Some(t) = self.session.traffic.as_ref() {
                    for c in &t.cars {
                        add(&c.vehicle);
                    }
                }
                others.iter().any(|o| me.overlaps_plan(o))
            })
            .unwrap_or(false);
        if let Some(at) = hit {
            put_vehicle(&mut self.session.placed[k].vehicle, at, heading);
        }
        if let Some(pl) = self.menus.placing.as_mut() {
            pl.at = hit;
            pl.blocked = blocked;
        }
        let msg = match (hit, blocked) {
            (None, _) => "Placing: point at the ground (wheel / Q / E turn, R turns round, Esc takes it away)",
            (Some(_), true) => "Placing: it would stand in another vehicle here",
            (Some(_), false) => "Placing: click to set it down (wheel / Q / E turn, R turns round, Esc takes it away)",
        };
        self.service_msg = Some((msg.into(), 0.5));
    }

    /// The mouse wheel while placing: the vehicle turns (7.5° a notch).
    pub(crate) fn placing_wheel(&mut self, amount: f32) {
        if let Some(pl) = self.menus.placing.as_mut() {
            pl.heading = (pl.heading + amount as f64 * 7.5).rem_euclid(360.0);
        }
    }

    /// A left click while placing: set down where it stands (true: the click was taken).
    pub(crate) fn placing_click(&mut self) -> bool {
        let Some(pl) = self.menus.placing.as_ref() else { return false };
        match (pl.at, pl.blocked) {
            (Some(_), false) => {
                let uid = pl.uid;
                self.menus.placing = None;
                if let Some(q) = self.session.placed.iter_mut().find(|q| q.uid == uid) {
                    // settled onto its wheels where it was put
                    for _ in 0..3 {
                        q.vehicle.update(1.0 / 30.0);
                    }
                    let name = format!("{} {}", q.vehicle.ty.def.manufacturer, q.vehicle.ty.def.type_name);
                    let how = if self.player.is_some() { "Esc menu: Drive the next vehicle" } else { "G at its driver's door" };
                    self.service_msg = Some((format!("Placed: {} ({how} to drive it)", name.trim()), 5.0));
                }
            }
            (Some(_), true) => self.service_msg = Some(("It would stand in another vehicle here".into(), 2.0)),
            (None, _) => self.service_msg = Some(("Point at the ground to set it down".into(), 2.0)),
        }
        true
    }

    /// Keys while placing; true when the key was taken.
    pub(crate) fn placing_key(&mut self, code: KeyCode, pressed: bool) -> bool {
        if self.menus.placing.is_none() {
            return false;
        }
        match code {
            KeyCode::Escape => {
                if pressed {
                    self.placing_cancel();
                }
                true
            }
            KeyCode::KeyR | KeyCode::KeyQ | KeyCode::KeyE => {
                if pressed {
                    let turn = match code {
                        KeyCode::KeyR => 180.0,
                        KeyCode::KeyQ => -15.0,
                        _ => 15.0,
                    };
                    if let Some(pl) = self.menus.placing.as_mut() {
                        pl.heading = (pl.heading + turn).rem_euclid(360.0);
                    }
                }
                true
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                if pressed {
                    self.placing_click();
                }
                true
            }
            _ => false,
        }
    }

    /// Every vehicle one placed goes (Esc menu: Remove the placed vehicles), their riders
    /// stepping out where they are.
    pub(crate) fn remove_placed_vehicles(&mut self) {
        self.menus.placing = None;
        let n = self.session.placed.len();
        for mut q in std::mem::take(&mut self.session.placed) {
            if let (Some(a), Some(mut ss)) = (self.sound.audio.as_ref(), q.sounds.take()) {
                ss.stop_all(a);
            }
            if let (Some(w), Some(r), Some(scene)) = (self.world.clone(), self.renderer.as_ref(), self.scene.as_mut()) {
                if let Some(h) = self.session.humans.as_mut() {
                    h.evict(crate::humans::BusId::Ai(crate::humans::placed_bus_id(q.uid)), &w);
                }
                if let Some(mut d) = q.driver.take() {
                    d.hide(r, scene);
                }
                if let Some(mut tv) = q.trolley_view.take() {
                    tv.release(r, scene);
                }
                w.release_vehicle(r, scene, q.render);
                for t in q.trailer_renders {
                    w.release_vehicle(r, scene, t);
                }
            }
        }
        self.service_msg = Some((omsi_ui::i18n::tr_fill("{n} placed vehicle(s) removed", &[("{n}", &n.to_string())]), 3.0));
    }

    /// Escape while placing: the vehicle goes again.
    pub(crate) fn placing_cancel(&mut self) {
        let Some(pl) = self.menus.placing.take() else { return };
        let Some(k) = self.session.placed.iter().position(|q| q.uid == pl.uid) else { return };
        let mut q = self.session.placed.remove(k);
        if let (Some(a), Some(mut ss)) = (self.sound.audio.as_ref(), q.sounds.take()) {
            ss.stop_all(a);
        }
        if let (Some(w), Some(r), Some(scene)) = (self.world.clone(), self.renderer.as_ref(), self.scene.as_mut()) {
            if let Some(mut d) = q.driver.take() {
                d.hide(r, scene);
            }
            if let Some(mut tv) = q.trolley_view.take() {
                tv.release(r, scene);
            }
            w.release_vehicle(r, scene, q.render);
            for t in q.trailer_renders {
                w.release_vehicle(r, scene, t);
            }
        }
        self.service_msg = Some(("Placing given up".into(), 2.0));
    }
}
