//! Giving the existing bus to AI and leaving the driver safely on foot.

use crate::App;
use crate::humans::BusId;
use crate::view_sync::traffic::CarRender;
use omsi_ui::tr;

impl App {
    pub(super) fn give_bus_to_ai(&mut self) -> Result<(), String> {
        if let Some(reason) = self.give_ai_reason() { return Err(reason.into()); }
        let mut plan = super::route::prepare(self)?;
        let outside = self.handover_exit().ok_or("There is no safe place outside the bus to hand it over")?;
        let heading = self.session.on_foot.as_ref().map(|f| f.yaw as f64)
            .unwrap_or_else(|| self.player.as_ref().unwrap().vehicle.heading);
        let service = plan.departure.and_then(|i| self.session.schedule.as_ref().map(|schedule| {
            let trip = schedule.trip_of(i);
            (trip.terminus.clone(), omsi_sim::timetable_run::trip_stations(trip).last().copied(), schedule.special_stops(i))
        }));
        let renderer = self.renderer.as_ref().ok_or("The renderer is not ready")?;
        let scene = self.scene.as_mut().ok_or("The scene is not ready")?;
        let mut p = self.player.take().expect("preflight kept the owned bus");
        p.axes.release_all();
        p.release();
        let held: Vec<_> = p.held_keys.keys().copied().collect();
        for scan in held { p.key(scan, 0, false); }
        p.door_release();
        if let (Some(audio), Some(mut sounds)) = (self.sound.audio.as_ref(), p.sounds.take()) { sounds.stop_all(audio); }
        if let Some(mut driver) = p.driver.take() { driver.hide(renderer, scene); }
        let delay = p.vehicle.host.tt_delay;
        if let Some(bus) = plan.bus.as_mut() {
            bus.timetable.install(&mut p.vehicle.host, bus.stops.first());
            p.vehicle.host.tt_delay = delay;
            p.vehicle.set_var("schedule_active", 1.0);
        }
        let scheme = p.vehicle.host.paint_scheme.flatten();
        let traffic = self.session.traffic.as_mut().expect("preflight kept traffic");
        let id = traffic.sim.adopt_vehicle(p.vehicle, plan.lane, plan.s, plan.bus);
        let car = traffic.cars.last_mut().expect("the bus was appended");
        car.scheme = scheme;
        if let (Some(bus), Some((terminus, last_stop, (always, early, holds)))) = (car.bus.as_mut(), service) {
            bus.terminus = terminus;
            bus.last_stop = last_stop;
            bus.always = always;
            bus.serve_early = early;
            bus.holds = holds;
            bus.delay = delay as f64;
        }
        if plan.departure.is_none() {
            let ci = traffic.cars.len() - 1;
            traffic.release(ci);
        }
        self.gfx.sim_view.traffic.insert(id, CarRender { body: p.render, trailers: p.trailer_renders, strs: Vec::new() });
        if let Some(departure) = plan.departure {
            self.session.schedule.as_mut().expect("preflight kept the timetable").handover_to_ai(id, departure);
        }
        self.session.duty = None;
        self.session.duty_places = false;
        self.session.journey = None;
        self.args.line = None;
        self.args.tour = None;
        self.session.safe_pose = None;
        self.session.safe_age = 0.0;
        self.session.pumping = None;
        if let Some(h) = self.session.humans.as_mut() { h.remap_bus(BusId::Player, BusId::Ai(id)); }
        // No AI step happens before the pedestrian is outside the bus.
        self.start_on_foot(outside, heading);
        self.cam.ego = false;
        self.input.dragging = false;
        self.service_msg = Some((tr("AI is driving the bus. You are now on foot; F4 selects an AI bus to take over.").into_owned(), 7.0));
        Ok(())
    }
}
