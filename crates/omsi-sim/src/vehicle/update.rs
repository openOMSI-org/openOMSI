//! One frame of a vehicle: physics, scripts and the engine's own variables, for the player's
//! vehicle and for AI copies, and the coupled parts following.

use super::*;

impl VehicleInstance {
    /// `Driver_Seat_VertTransl`: the sprung driver's seat, as OMSI moves it:
    /// the body's vertical acceleration and its roll and pitch at the seat's place (the
    /// driver's camera) push a spring (3000) and damper (2000) on 150 kg; ±10 cm at most.
    /// (Omsi.exe 0x7e2110 keeps it at +0x510, hands it to the scripts at 0x7cffb8 and lifts
    /// the driver's camera by the same amount, 0x7e24b2: see [`Self::driver_seat_travel`];
    /// worked out for a model whose scripts do not read it as well.)
    pub(super) fn update_driver_seat(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        // (the lever is the first driver camera, whichever one is chosen: 0x7e21a0 and
        // 0x7e21c7 read Cameras_driver[0]'s position for the roll's and the pitch's share)
        let seat = self.ty.def.cameras_driver.first().map(|c| c.pos).unwrap_or([0.0; 3]);
        let (acc_z, omega) = match &self.rigid {
            Some(rb) => (rb.accel_body.z - 9.81, rb.omega),
            None => (0.0, Vec3::ZERO),
        };
        let d_omega = omega - self.seat.2;
        self.seat.2 = omega;
        // (the rolls' and pitches' change moves the seat by its lever: y forward rolls
        // about x, x across pitches about y)
        let push = -acc_z * dt + d_omega.y * seat[0] - d_omega.x * seat[1];
        let k = (1000.0 / (15.0 * dt * 1000.0)).min(1.0);
        let (mut x, mut v) = (self.seat.0, self.seat.1);
        v += push + ((-x * 3000.0 - v * 2000.0) / 150.0) * k * dt;
        x += v * dt;
        if x.abs() > 0.1 {
            x = x.clamp(-0.1, 0.1);
            v = 0.0;
        }
        self.seat = (x, v, omega);
        if let Some(id) = self.program_ids().driver_seat {
            self.state.vars[id as usize] = x;
        }
    }

    /// How far the sprung driver's seat stands above its rest (m, ±0.1): the driver's eye
    /// rides on it, as the seat mesh does with `Driver_Seat_VertTransl` (#2550).
    pub fn driver_seat_travel(&self) -> f32 {
        self.seat.0
    }

    /// Dirt and spray. OMSI writes three engine variables every frame and the bus scripts
    /// turn them into what you see: `Dirt_Norm` is how dirty the body is (the `[alphascale]`
    /// of the dirt overlay), `DirtRate` how fast the windscreen is soiling right now
    /// (`dirt.osc` adds it up into `Dirt_Wiped`, which the wipers clear), and `PrecipRate`
    /// is signed, so rain wets the glass in `rain.osc` and dry weather dries it again.
    pub(super) fn update_dirt(&mut self, dt: f32) {
        let rain = self.host.precip_rate.clamp(0.0, 1.0);
        let speed = (self.physics.velocity_kmh().abs() / 50.0).min(2.0);
        // a dry duty adds a few percent, a wet one soils the bus in a couple of hours
        // a dry duty of a couple of hours shows; a wet one soils the bus in half an hour
        // Off the road the wheels throw up the ground itself: earth and grass churned up
        // under the tyres spatter the sides, dust off dry ground, mud off wet - a few minutes
        // across a field leave the bus brown (`host.offroad`, the wheels' share off the road)
        let off = self.host.offroad.clamp(0.0, 1.0) * (2.5e-3 + 6.0e-3 * rain.max(self.host.street_cond.clamp(0.0, 1.0)));
        self.dirt = (self.dirt + dt * speed * (8.0e-5 + 1.2e-3 * rain + off)).clamp(0.0, 1.0);
        self.set_engine_ev(Ev::DirtNorm, self.dirt);
        // spray off a wet road, dust off a dry one
        self.set_engine_ev(Ev::DirtRate, speed * (1.5e-4 + 6.0e-3 * rain));
        // rain soaks the glass in a few seconds; without it the film dries in about a minute.
        // Snow builds the film up the same way: `rain.osc` only ever adds `PrecipRate *
        // Timegap` and clamps at 1 - it never asks what is falling - so in the original the
        // glass gets as covered in a snowfall as in a shower and the wipers clear it. (The
        // film wears snow crystals then, see `rain::snow_on_glass`.) Held to a fifth for a
        // "haze", the panes stayed clear in the thickest snowfall and the wipers had
        // nothing to do (#883).
        let rate = if rain > 0.0 { rain * 0.25 } else { -0.02 };
        self.set_engine_ev(Ev::PrecipRate, rate);
        // the state of the road, for the tyre sounds and the wheel spray
        self.set_engine_ev(Ev::StreetCond, self.host.street_cond);
        // what is coupled to this vehicle (the train scripts light their ends by it: a head
        // lamp only where nothing is coupled, the tail lamps at the last car)
        self.set_engine_ev(Ev::TrainFrontCoupling, 0.0);
        self.set_engine_ev(Ev::TrainBackCoupling, if self.trailers.is_empty() { 0.0 } else { 1.0 });
        self.set_engine_ev(Ev::TrainMeReverse, 0.0);
        // and for the tyres' grip
        let mu = road_grip(self.host.street_cond, self.host.temperature);
        if let Some(rb) = self.rigid.as_mut() {
            rb.friction = mu;
        }
    }

    /// The engine's own vehicle variables that OMSI binds to fields of the vehicle
    /// (`TRoadVehicle.virtual_00` registers them, the original points each at its field, and
    /// the original keeps the fields up to date): the odometer (`kmcounter_km` whole
    /// kilometres, `kmcounter_m` the metres of the fractions), the people aboard, whether a
    /// timetable is driven, the precipitation type, and the cabin air - kept within reach of
    /// the weather (never more than 10 °C from the air outside and pulled towards 18..25 °C
    /// as a heated/ventilated bus is; the absolute humidity follows the outside air).
    pub(super) fn update_engine_vars(&mut self, dt: f32) {
        self.update_driver_seat(dt);
        // `[kmcounter_init] year km`: in service since that year, so many kilometres a year -
        // the odometer starts at what that comes to on the day driven (it stood at 0 on
        // every bus that has one, #305), a little different from bus to bus of the kind.
        // Omsi.exe 0x7D1E42: Random(100)/10 + 8 + max(0, years) * km * (1 + 0.2 *
        // (Random(100) - 50) / 50), and 1980 / 60000 km a year without the keyword
        // (TRoadVehicle.LoadFromFile's defaults).
        if !self.km_started {
            self.km_started = true;
            if self.host.km_base == 0.0 {
                let (year, per_year) = self.ty.def.km_counter_init.unwrap_or((1980, 60000.0));
                let years = ((self.host.clock.year - year) as f64 + self.host.clock.day_of_year as f64 / 365.0).max(0.0);
                let seed = (std::ptr::addr_of!(self.host) as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40;
                let (r1, r2) = ((seed % 100) as f64, ((seed / 100) % 100) as f64);
                self.host.km_base = r1 / 10.0 + 8.0 + years * per_year as f64 * (1.0 + 0.2 * (r2 - 50.0) / 50.0);
            }
        }
        // (the sum is split, not the parts: 0.7 km + 0.5 km is 1 km 200 m, not 0 km 1200 m)
        let total = self.odometer_km();
        self.set_engine_ev(Ev::KmCounterKm, total.trunc() as f32);
        self.set_engine_ev(Ev::KmCounterM, (total.fract() * 1000.0) as f32);
        self.set_engine_ev(Ev::HumansCount, self.host.humans_count);
        self.set_engine_ev(Ev::ScheduleActive, self.host.schedule_active);
        self.set_engine_ev(Ev::PrecipType, self.host.precip_type);
        // A bus whose scripts keep their own cabin air (`heizung.osc` of the stock buses and
        // their descendants: they integrate the temperature from heat flows) is left to
        // them; only the relative humidity comes from their temperature and humidity. An
        // engine model on top of theirs broke that balance - the Sprinter 312D's cabin
        // went to infinity within a second, and its engine with it.
        if self.program_ids().owns_cabin_temp {
            if let (Some(t), Some(hum)) = (self.ev(Ev::CabinTemp), self.ev(Ev::CabinAbsHum)) {
                if !self.program_ids().owns_cabin_rel_hum && t.is_finite() && t > -237.0 {
                    self.set_engine_ev(Ev::CabinRelHum, relative_humidity(t, hum));
                }
            }
            return;
        }
        let out = self.host.temperature;
        let (mut t, mut hum, last) = self.cabin_air.unwrap_or((out.clamp(18.0, 25.0), 9.0, f32::NAN));
        // a script of the bus's own wrote the cabin temperature: that is what it is now
        if let Some(cur) = self.ev(Ev::CabinTemp) {
            if !last.is_nan() && (cur - last).abs() > 1e-4 {
                t = cur;
            }
        }
        let lower = (out + 10.0).min(18.0).max(out - 10.0);
        let upper = (out + 20.0).min(out.max(25.0)).max(lower);
        let target = t.clamp(lower, upper);
        t += (target - t) * (dt / 120.0).min(1.0);
        let out_hum = if self.host.abs_humidity > 0.0 { self.host.abs_humidity } else { 9.0 };
        hum += (out_hum - hum) * (dt / 300.0).min(1.0);
        // relative humidity: against the saturation (Magnus) at the cabin temperature

        self.set_engine_ev(Ev::CabinTemp, t);
        self.set_engine_ev(Ev::CabinAbsHum, hum);
        self.set_engine_ev(Ev::CabinRelHum, relative_humidity(t, hum));
        self.cabin_air = Some((t, hum, t));
    }

    /// Change cabin temperature once without changing heater controls or coolant.
    /// Shared-script rear sections read the leading state; independently scripted cars
    /// need their own temperature and humidity updated too.
    pub fn set_cabin_temperature(&mut self, temperature: f32) {
        if !temperature.is_finite() {
            return;
        }
        let humidity = self
            .ev(Ev::CabinAbsHum)
            .filter(|h| h.is_finite() && *h >= 0.0)
            .unwrap_or_else(|| self.cabin_air.map_or(9.0, |(_, h, _)| h));
        self.set_engine_ev(Ev::CabinTemp, temperature);
        self.set_engine_ev(Ev::CabinAbsHum, humidity);
        self.set_engine_ev(Ev::CabinRelHum, relative_humidity(temperature, humidity));
        self.cabin_air = Some((temperature, humidity, temperature));
        for part in &mut self.trailers {
            if let Some(run) = part.run.as_mut() {
                let humidity = run
                    .names
                    .get("cabinair_abshum")
                    .map(|&id| run.state.vars[id as usize])
                    .filter(|h| h.is_finite() && *h >= 0.0)
                    .unwrap_or(humidity);
                for (name, value) in [
                    ("cabinair_temp", temperature),
                    ("cabinair_relhum", relative_humidity(temperature, humidity)),
                ] {
                    if let Some(&id) = run.names.get(name) {
                        run.state.vars[id as usize] = value;
                    }
                }
            }
        }
    }

    /// Run one simulation frame: physics, scripts, then animations.
    pub fn update(&mut self, dt: f32) {
        self.host.clock.advance(dt);
        self.step_physics(dt);
        self.update_ground_probe();
        self.run_collision_trigger();
        self.update_dirt(dt);
        // (signed, as Omsi.exe 0x7e5163 adds it: reversing takes it back)
        self.driven_km += (self.physics.velocity_kmh() as f64 / 3600.0) * dt as f64;
        self.update_engine_vars(dt);
        crate::timetable_run::apply_pending_destination(self);
        self.apply_scheme_vars_each_frame();
        self.run_control_cables();
        let p = self.ty.program.clone();
        self.vm.run_frame(&p, &mut self.state, &mut self.host);
        // and once more after them: what is drawn is the scheme's (see the method)
        self.apply_scheme_vars_each_frame();
        self.show_radio_text();
        self.clear_pax_requests();
        self.update_visuals(dt);
    }

    pub(super) fn run_collision_trigger(&mut self) {
        if std::mem::take(&mut self.collided) {
            self.trigger("collision");
            self.host.coll_energy = 0.0;
        }
    }

    /// The passengers' door requests are pulses: Omsi.exe clears all eight of each kind
    /// (here all [`PAX_DOORS`]) after the vehicle's scripts ran (0x7d6214) and the
    /// passengers set them again every frame. Kept, a timetable bus that drove out of the
    /// passengers' reach kept its last request, and its automatic door never shut.
    pub(super) fn clear_pax_requests(&mut self) {
        for &id in &self.v_pax_req {
            self.state.vars[id as usize] = 0.0;
        }
    }

    pub(super) fn run_control_cables(&mut self) {
        for &(write, read) in &self.v_cables {
            self.state.vars[read as usize] = self.state.vars[write as usize];
        }
    }

    /// Run the scripts' frame once with no time passing (no physics, no clock): what a
    /// switch just pressed makes of the variables the scripts derive from it.
    pub fn update_scripts_only(&mut self, _dt: f32) {
        crate::timetable_run::apply_pending_destination(self);
        let p = self.ty.program.clone();
        let gap = self.host.clock.timegap;
        self.host.clock.timegap = 0.0;
        self.run_control_cables();
        self.vm.run_frame(&p, &mut self.state, &mut self.host);
        self.apply_scheme_vars_each_frame();
        self.host.clock.timegap = gap;
    }

    /// Let the scripts measure the ground under a point of the vehicle
    /// (`GetHeightAbovePoint`): the probe captures this frame's pose and ground sampler.
    /// The answer is how high the point stands *over* the ground, positive with room below:
    /// the Solaris' kneeling sensor stops lowering the body "as soon as it touches the
    /// kerb" when it reads under 5 cm and lets the level control vent the right-hand
    /// bellows only above that, and the NL/NG lift may drop by what it reads (up to
    /// 0.3 m). Read the other way round (ground minus point) the sensor sat at 0, the
    /// right-hand bellows of an Urbino never let their air out and the bus leant 1.7° to
    /// the left all the way. The ground is what lies under the point (with the world's
    /// faces, looked for from half a metre above it, so a kerb a little higher still
    /// counts): the plain sampler gives the highest face there, which under a petrol
    /// station's canopy is its roof - the point stood metres "under the ground", the NL/NG
    /// lift could not go down at all and the arcsine of the ramp's angle turned it on its
    /// side and then away (#2205).
    pub(super) fn update_ground_probe(&mut self) {
        if self.ground.is_none() && self.contact.is_none() {
            return;
        }
        let rot = self.body_rotation();
        let pos = self.position;
        let g = self.ground.clone();
        let contact = self.contact.clone();
        self.host.ground_probe = Some(std::sync::Arc::new(move |x, y, z| {
            let w = pos + rot.transform_point3(glam::Vec3::new(x, y, z)).as_dvec3();
            let h = match &contact {
                Some(c) => c.probe(w.x, w.y, w.z + 0.5).below,
                None => g.as_ref().and_then(|g| g(w.x, w.y)),
            };
            h.map_or(0.0, |h| (w.z - h) as f32)
        }));
    }

    /// One frame of an AI-controlled vehicle: the pose is set by the traffic system
    /// (`ai_motion::AiBody`, which also leaves the accelerations and each wheel's travel in
    /// `physics`), the scripts see the motion through `Velocity` and the `AI_*` variables and
    /// run their `{frame_ai}` blocks (falling back to `{frame}` when a script has none).
    pub fn update_ai(&mut self, dt: f32, ai: &AiFrame) {
        self.update_ai_with(dt, ai, &[], &[]);
    }

    /// `update_ai` for the copy of a vehicle another game drives (LAN play): `inputs` are
    /// written after the `AI_*` variables (so they may override `AI_Engine` or `AI_Light`)
    /// and before the scripts run; `pinned` are written before and once more after the
    /// scripts, so the animations, lamps and sounds show the other game's values whatever
    /// this copy's AI scripts made of them. The odometer may run backwards (reversing).
    pub fn update_ai_with(
        &mut self,
        dt: f32,
        ai: &AiFrame,
        inputs: &[(omsi_script::VarId, f32)],
        pinned: &[(omsi_script::VarId, f32)],
    ) {
        self.update_ai_with_strings(dt, ai, inputs, pinned, &[]);
    }

    /// Pin owner strings before script execution and again before visual updates.
    pub fn update_ai_with_strings(
        &mut self,
        dt: f32,
        ai: &AiFrame,
        inputs: &[(omsi_script::VarId, f32)],
        pinned: &[(omsi_script::VarId, f32)],
        strings: &[(omsi_script::StrVarId, &str)],
    ) {
        if self.host.external_ai_clock {
            self.host.clock.timegap = dt;
        } else {
            self.host.clock.advance(dt);
        }
        self.set_engine_ev(Ev::ScheduleActive, self.host.schedule_active);
        self.physics.speed = ai.speed;
        self.physics.steer_deg = ai.steer_deg;
        let v_kmh = ai.speed * 3.6;
        self.put(self.v_velocity, v_kmh);
        self.put(self.v_velocity_ground, v_kmh);
        // AI motion bypasses step_physics, which normally hands the pedals to scripts.
        let controls = self.physics.controls;
        self.put(self.v_throttle, controls.throttle);
        self.put(self.v_brake, controls.brake);
        self.put(self.v_clutch, controls.clutch);
        // how far the vehicle rolled since the last frame (a reset or a jump counts as none;
        // backwards only for a vehicle that reverses)
        let ds = ai.odometer - self.ai_odometer;
        self.ai_odometer = ai.odometer;
        let ds = if (0.0..50.0).contains(&ds) || (ai.speed < 0.0 && ds > -50.0 && ds < 0.0) {
            ds
        } else {
            0.0
        };
        // `steer_deg` is the front wheel angle of a bicycle model turning about the
        // `[rot_pnt_long]` line: every axle ahead of that line points at the common turning
        // centre, and every wheel rolls as far as its own track through the bend is long
        let (rot, wheelbase) = crate::ai_motion::rotation_point(&self.ty.def);
        let k = ai.steer_deg.to_radians().tan() / wheelbase;
        for ai_idx in 0..self.v_wheels.len() {
            let axle = self.v_wheels[ai_idx];
            for (si, w) in axle.iter().enumerate() {
                let Some(ws) = self.physics.wheels.get_mut(ai_idx).map(|a| &mut a[si]) else {
                    continue;
                };
                let arm = ws.long - rot;
                let across = 1.0 - ws.lat * k;
                // (the axle's angle, the same for both sides, as Omsi.exe hands it to the
                // scripts: see `RigidBody::axle_steer`)
                let steer = if arm > 0.5 {
                    (arm * k).atan().clamp(-1.2, 1.2)
                } else {
                    0.0
                };
                let travel = (across * across + (arm * k) * (arm * k)).sqrt();
                let r = ws.radius.max(0.05);
                ws.rotation_deg =
                    (ws.rotation_deg + (ds * travel / r).to_degrees()).rem_euclid(360.0);
                ws.rpm = ai.speed * travel / (std::f32::consts::TAU * r) * 60.0;
                let (rotation, rpm, suspension) =
                    (ws.rotation_deg.to_radians(), ws.rpm, ws.suspension);
                self.put(w[0], rotation);
                self.put(w[1], rpm);
                self.put(w[2], steer);
                // the travel `AiBody` works out on its springs, about the static sag of an AI
                // copy (`ai_rest_offset`, already in it)
                self.put(w[3], suspension);
            }
        }
        let n_wheel = self
            .physics
            .wheels
            .iter()
            .find(|w| w[0].driven)
            .or(self.physics.wheels.first())
            .map(|w| w[0].rpm)
            .unwrap_or(0.0);
        self.put(self.v_n_wheel, n_wheel);
        let a = self.physics.accel;
        self.physics.a_trans = a;
        self.put(self.v_accel[0], a.x);
        self.put(self.v_accel[1], a.y);
        self.put(self.v_accel[2], a.z);
        // `AI` is the engine's "this vehicle is driven by the AI" flag; the police and
        // ambulance script sounds the siren whenever `AI` is 0 and the car rolls, which is
        // why every spawn came with an ambulance nobody could see
        // `AI_Scheduled_AtStation`: 1 while the bus serves a stop (the door scripts open the
        // doors), then -1 when it wants to leave - the stock `door_X10_AI.osc` closes the
        // doors on -1, releases the stop brake and only then writes 0, which is the signal
        // that the bus may move (`VehicleInstance::station_released`). Writing 0 straight
        // away left every timetable bus driving off with its doors open. A frame that says
        // nothing (0) keeps telling a script that has not answered yet to close.
        let station = match ai.at_station {
            1 => 1.0,
            -1 => -1.0,
            _ => match self.ev(Ev::AiAtStation) {
                Some(v) if v.abs() > 0.5 => -1.0,
                _ => 0.0,
            },
        };
        for (ev, v) in [
            (Ev::Ai, 1.0),
            (Ev::AiBlinkerL, matches!(ai.blinker, 1 | 3) as i32 as f32),
            (Ev::AiBlinkerR, matches!(ai.blinker, 2 | 3) as i32 as f32),
            (Ev::AiBrakelight, ai.brake as i32 as f32),
            (Ev::AiLight, ai.lights as i32 as f32),
            // (the engine's field +0x638: an AI bus lights its saloon when it drives with
            // its lights on - the LiAZ's `lights_AI` switches both saloon circuits on it)
            (Ev::AiInteriorlight, ai.lights as i32 as f32),
            (Ev::AiEngine, if ai.engine_off { -1.0 } else { 1.0 }),
            (Ev::AiAtStation, station),
            // Which side's doors: OMSI hands the stop's side to the script, and a vehicle
            // with doors on both sides opens only the platform's (the BRT stops in
            // Urumqi61 lie left, the ordinary ones right). Off a stop it is 0 (OMSI's
            // default), so a script that reads it there does the same as ever.
            (Ev::AiAtStationSide, ai.at_station_side),
            (Ev::PriorityWarning, ai.priority_warning as i32 as f32),
        ] {
            self.set_ev(ev, v);
        }
        // (the paint scheme's [setvar]s first: what another game sends of its own bus, pinned
        // below, must win over them - its v_font 7 for the Cyrillic font stood reset to the
        // scheme's 0 here, and its display read Latin in the other players' games)
        self.apply_scheme_vars_each_frame();
        self.run_control_cables();
        for &(id, v) in inputs.iter().chain(pinned) {
            self.put(Some(id), v);
        }
        for (id, value) in strings {
            if let Some(s) = self.state.str_vars.get_mut(*id as usize) {
                if s.as_str() != *value {
                    s.clear();
                    s.push_str(value);
                }
            }
        }
        self.run_collision_trigger();
        let p = self.ty.program.clone();
        crate::timetable_run::apply_pending_destination(self);
        if p.frame_ai.is_empty() {
            self.vm.run_frame(&p, &mut self.state, &mut self.host);
        } else {
            self.vm.run_frame_ai(&p, &mut self.state, &mut self.host);
        }
        self.clear_pax_requests();
        // (the scheme's [setvar]s after the scripts as well, and what another game pinned
        // after those: see `apply_scheme_vars_each_frame`)
        self.apply_scheme_vars_each_frame();
        for &(id, v) in pinned {
            self.put(Some(id), v);
        }
        for (id, value) in strings {
            if let Some(s) = self.state.str_vars.get_mut(*id as usize) {
                if s.as_str() != *value {
                    s.clear();
                    s.push_str(value);
                }
            }
        }
        if self.ai_visuals {
            let dt = dt + std::mem::take(&mut self.ai_visuals_missed);
            self.update_visuals(dt);
        } else {
            // the coupled parts follow all the same (other cars keep clear of them)
            if !std::mem::take(&mut self.trailers_stepped) {
                self.update_trailers(dt);
            }
            self.ai_visuals_missed = (self.ai_visuals_missed + dt).min(10.0);
        }
    }

    /// Has the bus's script finished leaving its stop (doors shut, stop brake off)? True
    /// for vehicles whose scripts do not take part in that handshake.
    pub fn station_released(&self) -> bool {
        self.ev(Ev::AiAtStation)
            .map(|v| v > -0.5)
            .unwrap_or(true)
    }

    /// An AI vehicle's coupled parts moved on with its body in every step of the traffic,
    /// while its scripts and animations run once a frame (`TrafficSim::scripts_due`): an
    /// articulated bus's rear section followed only the frame's last step, and below 50
    /// fps it stood a step or more behind the front, the joint open between them.
    pub fn step_ai_trailers(&mut self, dt: f32) {
        self.update_trailers(dt);
        self.trailers_stepped = true;
    }

    /// Coupled vehicles follow and animate from this vehicle's variables.
    pub(super) fn update_trailers(&mut self, dt: f32) {
        if self.ty.def.is_rail() && self.trailers.iter().any(|t| t.track.is_some()) {
            return;
        }
        let mut trailers = std::mem::take(&mut self.trailers);
        super::car_run::set_couplings(&mut trailers);
        let mut lead: Option<(DVec3, Mat4, f64)> = None;
        let mut turn = self.turning();
        for t in &mut trailers {
            t.lead_turn = turn;
            t.update(self, dt, lead);
            lead = Some((t.position, t.body_rotation(), t.heading));
            turn = (t.kappa, t.axle_long);
        }
        self.trailers = trailers;
    }

    /// The curvature (1/m, positive to the right) the vehicle turns on and where its turning
    /// line lies along it (m): the steering's, on the `[rot_pnt_long]` line every axle is
    /// pointed at (`RigidBody::steer_axles`), or an AI vehicle's bicycle model's.
    pub(super) fn turning(&self) -> (f32, f32) {
        if let Some(rb) = &self.rigid {
            return (rb.kappa, rb.rot_pnt_long);
        }
        let (rot, wheelbase) = crate::ai_motion::rotation_point(&self.ty.def);
        (self.physics.steer_deg.to_radians().tan() / wheelbase, rot)
    }

    /// Put the coupled parts on a track: `at_behind(d)` is the point of the track `d` metres
    /// behind the vehicle's origin (negative: ahead of it; None: not known, the part follows
    /// as it does on a road).
    pub fn retrail(&mut self, dt: f32, at_behind: &dyn Fn(f64) -> Option<DVec3>) {
        // a single railcar has nothing coupled behind it (Thüringer Wald's AI trains, #2259)
        if self.trailers.is_empty() {
            return;
        }
        let mut trailers = std::mem::take(&mut self.trailers);
        super::car_run::set_couplings(&mut trailers);
        let mut lead: Option<(DVec3, Mat4, f64)> = None;
        let mut turn = self.turning();
        if self.ty.def.is_rail() {
            // A train's cars as Omsi.exe lays them: one after the other along the track, each
            // part's origin as far behind the joint in front of it as its declared front
            // coupling says, and the part standing on the track with both of its bogies. The
            // couplings are distances along the track and nothing more - the stock Berlin
            // S-Bahn (BR481) couples its bogie frames behind the last car with
            // `[coupling_front] 0 -34.35`, which puts each one back under the car it belongs
            // to. Towed at the coupling like a road trailer instead, with its axle at the
            // sum of distances of a frame mixed up with the car in front of it (the lead's
            // rear coupling taken for the part's own) and a bogie half as far out as the file
            // says, the axle's track point fell short of the coupling or ahead of it: the
            // car stood up at the steepest pitch allowed and turned about on the track
            // (#533, #2069, #2415).
            // (a coupling's y in the frame of the travel: the model's, turned with the body)
            let travel = |turned: bool, y: f32| if turned { -y as f64 } else { y as f64 };
            // how far the joint in front of the part lies behind this vehicle's origin
            let mut joint = -travel(body_reversed(&self.ty.def, false), trailers[0].coupling_back.y);
            for i in 0..trailers.len() {
                // (the part's rear coupling is the one the next part hangs on, in this part's frame)
                let rear = trailers.get(i + 1).map(|n| n.coupling_back.y);
                let t = &mut trailers[i];
                let turned = body_reversed(&t.ty.def, t.reversed);
                let origin = joint + travel(turned, t.coupling_front.y);
                let (y_front, y_rear) = t.rail_ends();
                if omsi_cfg::flags::OMSI_DEBUG_TRAILERS.is_set() {
                    static SHOWN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                    if SHOWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 6 {
                        log::info!("rail part {i}: origin {origin:.2} m behind the lead's, joint before it {joint:.2}, running ends {y_front:.2} / {y_rear:.2}, front coupling {:.2}, back coupling {:.2} (declared: front {:?}, back {:?}, {} axles, boogies {:?})", t.coupling_front.y, t.coupling_back.y, t.ty.def.coupling_front.as_ref().map(|c| c.pos), t.ty.def.coupling_back.as_ref().map(|c| c.pos), t.ty.def.axles.len(), t.ty.def.boogies);
                    }
                }
                match (at_behind(origin - y_front as f64), at_behind(origin - y_rear as f64)) {
                    (Some(front), Some(back)) => t.place_on_rails(self, dt, lead, (front, y_front), (back, y_rear)),
                    _ => t.update(self, dt, lead),
                }
                if let Some(y) = rear {
                    joint = origin - travel(turned, y);
                }
                lead = Some((t.position, t.body_rotation(), t.heading));
            }
        } else {
            let (mut prev_c, mut b_prev) = (self.position, 0.0f64);
            for t in &mut trailers {
                let (lp, lr) = lead.map(|(p, r, _)| (p, r)).unwrap_or((self.position, self.body_rotation()));
                let c = t.coupling_point(lp, lr);
                let b_c = b_prev + (c - prev_c).truncate().length();
                if let Some(p) = at_behind(b_c + t.pivot_length() as f64) {
                    t.place_on_track(p);
                }
                b_prev = b_c;
                prev_c = c;
                t.lead_turn = turn;
                t.update(self, dt, lead);
                lead = Some((t.position, t.body_rotation(), t.heading));
                turn = (t.kappa, t.axle_long);
            }
        }
        self.trailers = trailers;
    }
}

/// OMSI's frames, a thirtieth of a second (`[maxFPS]` 30 in its options.cfg and every option
/// preset but one): `A_Trans_*` is the body's velocity change over one of them (0x7d5124),
/// and the stock rattle scripts (`klappern.osc`: `Klappern_Vol` follows how much |A_Trans|
/// changes from one frame to the next) were tuned on that. Taken over this game's frames -
/// 60 to 150 a second - the change from frame to frame was a half to a fifth of OMSI's for
/// the same jolt, and the buses kept quiet on rough roads (#772, #886). The value is the
/// mean over each thirtieth, held until the next one is complete.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct OmsiFrames {
    pub(super) sum: Vec3,
    pub(super) t: f32,
    pub(super) out: Vec3,
}

impl OmsiFrames {
    const FRAME: f32 = 1.0 / 30.0;

    pub(super) fn push(&mut self, a: Vec3, dt: f32) -> Vec3 {
        if !(dt > 0.0) || !a.is_finite() {
            return self.out;
        }
        // (a frame as long as OMSI's or longer is one of OMSI's)
        if dt >= Self::FRAME * 0.99 {
            *self = OmsiFrames { out: a, ..Default::default() };
            return a;
        }
        self.sum += a * dt;
        self.t += dt;
        if self.t >= Self::FRAME * 0.99 {
            self.out = self.sum / self.t;
            self.sum = Vec3::ZERO;
            self.t = 0.0;
        }
        self.out
    }
}

/// The body-frame acceleration the scripts see as `A_Trans_*` (Omsi.exe 0x7d5124: the
/// velocity's change over the frame, rotated into the body): `accel_body`, the specific force
/// an accelerometer would read, less gravity's share in the body frame.
pub(super) fn scripts_acceleration(accel_body: Vec3, orientation: Quat) -> Vec3 {
    accel_body - orientation.inverse().mul_vec3(Vec3::new(0.0, 0.0, 9.81))
}
