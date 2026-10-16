//! The LAN, the people and the player's duty in the window's frame.

use super::*;

impl App {
    /// The LAN session (a stage of its own: a joining player's bus is loaded here, and that
    /// frame was counted as the people's).
    pub(super) fn frame_lan(&mut self, dt: f32) {
        self.tick_lan(dt);
    }

    /// The player's duty, the personnel file's step, the placing of vehicles, the host's map
    /// edits and `--on-foot`.
    pub(super) fn frame_duty(&mut self, dt: f32) {
        if self.session.duty.is_none() {
            self.session.career.no_trip();
        }
        if let (Some(d), Some(p), Some(w), false) = (
            self.session.duty.as_mut(),
            self.player.as_mut(),
            self.world.as_ref(),
            self.paused,
        ) {
            steps::duty_step(
                d,
                p,
                w,
                &mut self.session.career,
                &mut self.session.journey,
                &self.args.root,
                self.clock.time,
                Some(&self.clock),
                true,
                Some(&mut self.integrations.plugin_events),
            );
        }
        let crash = if let Some(p) = self.player.as_mut() {
            let riders = self.session.humans.as_ref().map(|h| h.riding()).unwrap_or(0);
            // (a frame after the session was written must not start another one)
            let tick = !self.exiting && !self.paused;
            steps::career_step(&mut self.session.career, p, riders, self.session.duty.is_some(), dt, tick)
        } else {
            0.0
        };
        if crash > 0.0 {
            let speed = self.player.as_ref().map(|p| p.vehicle.physics.velocity_kmh().abs()).unwrap_or(0.0);
            let (at_fault, reason) = self.accident_fault_assessment(speed);
            self.session.accident_review = Some((1.5, at_fault));
            self.session.accident_reason = Some(reason);
            self.service_msg = Some((omsi_ui::tr("Accident: the expert is checking who is at fault…").into_owned(), 2.0));
            use omsi_plugin::InfoValue::Num;
            let args = vec![Num(crash as f64 / 1000.0), Num(speed as f64)];
            crate::plugins::queue_event(&mut self.integrations.plugin_events, "crash", args);
        }
        self.check_red_light();
        let missed_stops = self.session.humans.as_mut().map(|h| h.take_missed_player_stops());
        if let Some((alighting, boarding)) = missed_stops {
            for _ in 0..alighting {
                self.session.career.wallet_change(-omsi_career::economy::MISSED_ALIGHTING_FINE, "missed alighting: a passenger requested this stop but the bus drove past it");
            }
            for _ in 0..boarding {
                self.session.career.wallet_change(-omsi_career::economy::MISSED_BOARDING_FINE, "missed boarding: a waiting passenger requested this stop but the bus drove past it");
            }
        }
        let compliments = self.session.humans.as_mut().map(|h| h.take_passenger_compliments()).unwrap_or(0);
        for _ in 0..compliments {
            self.session.career.wallet_change(omsi_career::economy::PASSENGER_COMPLIMENT_REWARD, "passenger compliment: greeting or thanks after a satisfactory service");
        }
        self.check_road_offences(dt);
        let verdict = if let Some((left, at_fault)) = self.session.accident_review.as_mut() {
            *left -= dt;
            (*left <= 0.0).then_some(*at_fault)
        } else { None };
        if let Some(fault) = verdict {
            let reason = self.session.accident_reason.take().unwrap_or_else(|| "the collision circumstances were inconclusive".into());
            self.session.career.wallet_change_detail(
                if fault { -omsi_career::economy::CRASH_FINE } else { omsi_career::economy::ACCIDENT_EXONERATION },
                format!("{}: {}", if fault { "accident: you are responsible" } else { "accident: you are not responsible" }, reason),
            );
            self.session.accident_review = None;
        }
        for j in self.session.career.take_jolts() {
            use crate::plugins::num_f32;
            let args = vec![num_f32(j.along), num_f32(j.across), num_f32(j.speed_kmh), omsi_plugin::InfoValue::Num(j.riders as f64)];
            crate::plugins::queue_event(&mut self.integrations.plugin_events, "jolt", args);
        }
        if !self.paused {
            crate::admin::guard_fall(self, dt);
        }
        if let Some((amount, balance, reason)) = self.session.career.take_wallet_event() {
            self.service_msg = Some((
                format!(
                    "O-Bucks {amount:+.2}  •  {} • {} {balance:.2}",
                    omsi_ui::tr(&reason),
                    omsi_ui::tr("balance"),
                ),
                4.0,
            ));
        }
        self.placing_frame();
        // the host sends every edit of the map again now and then (players join)
        if self.net.lan.as_ref().map(|l| l.role == omsi_net::Role::Host).unwrap_or(false) {
            self.menus.editor_sync_t -= dt;
            if self.menus.editor_sync_t <= 0.0 {
                self.menus.editor_sync_t = 10.0;
                self.editor_broadcast(true);
            }
        }
        // --on-foot: the bus the start put down goes, the player stands beside it
        if self.args.on_foot && self.world.is_some() {
            self.args.on_foot = false;
            if self.player.is_some() {
                self.remove_driven_vehicle();
            } else if let Some(c) = self.camera.as_ref() {
                // (no bus came: where the camera stands, on the ground)
                let p = c.position;
                let z = self.world.as_ref().and_then(|w| w.walk_height(p.x, p.y)).unwrap_or(p.z - 1.7);
                let yaw = c.yaw as f64;
                self.start_on_foot(glam::DVec3::new(p.x, p.y, z), yaw);
            }
            self.service_msg = Some(("On foot: Esc menu, Place a vehicle..., then G at its driver's door to drive it".into(), 8.0));
        }
    }
}

crate::app::stage_ctx!(
    /// The stage after the people's part of the game.
    AfterHumansCx { reads: [clock], writes: [people, view, player, ui, integrations] }
);

/// After the people: the walker aboard a bus where the bus is this frame, and the fuel pump
/// or bus wash running.
pub(super) fn after_humans(cx: AfterHumansCx, dt: f32) {
    if let Some(f) = cx.people.on_foot.as_mut() {
        f.after_humans(cx.people.humans.as_ref(), cx.view.camera.as_mut());
    }
    if !*cx.clock.paused {
        cx.player.tick_service(dt, cx.ui.service_msg, &mut cx.integrations.plugin_events);
    }
}

crate::app::stage_ctx!(
    /// The people's part of the game.
    HumansCx { reads: [view, clock, net, duty, world], writes: [people, traffic, player, gfx, ui] }
);

/// The people at the stops and in the buses, and their view sync.
pub(super) fn humans(cx: HumansCx, dt: f32) {
    let sight = cx.view.camera.as_ref().zip(cx.gfx.gfx.surface.as_ref())
        .and_then(|(c, s)| crate::app_impl::sight_extent(cx.common.settings, cx.view, c, (s.config.width, s.config.height)));
    if let (Some(h), Some(w), Some(r), Some(scene)) = (
        cx.people.humans.as_mut(),
        cx.world.world.as_ref(),
        cx.gfx.renderer.as_ref(),
        cx.gfx.scene.as_mut(),
    ) {
        // (where the camera is, as Omsi.exe fills the stops round the tile it shows - at the
        // bus's height, not up in the sky of the map view. Kept at the player's bus, the stops
        // an AI bus was watched at from the free camera stood empty: nobody got on or off
        // after its first stop, #2319)
        let bus = cx.player.player.as_ref().map(|p| p.vehicle.position);
        let center = match (cx.view.camera.as_ref().map(|c| c.position), bus) {
            (Some(c), Some(b)) => DVec3::new(c.x, c.y, b.z),
            (Some(c), None) => c,
            (None, Some(b)) => b,
            (None, None) => DVec3::ZERO,
        };
        // (the trips due at the stops soon: once a game minute, #1415)
        if let (Some(s), Some(t)) = (cx.traffic.schedule.as_ref(), cx.traffic.traffic.as_ref()) {
            if (t.day_time - h.due_at).abs() >= 60.0 {
                let t0 = std::time::Instant::now();
                h.due_dests = Some(s.due_destinations(t.day_time));
                h.due_at = t.day_time;
                if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                    h.stage_stats.add("due destinations", t0.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }
        if h.stop_targets.is_none() {
            h.stop_targets = cx.traffic.schedule.as_ref().map(|s| s.stop_targets());
            h.stop_names = cx.traffic.schedule.as_ref().map(|s| s.stop_names());
            h.stop_target_objects = cx.traffic.schedule.as_ref().map(|s| s.stop_target_objects());
            if let Some(t) = &h.stop_targets {
                log::info!("people: {} bus stops with timetable targets", t.len());
            }
        }
        // (whom the player's bus takes on: by the duty, or in free drive by its terminus)
        h.set_duty(cx.duty.duty.as_ref());
        // (the riders leave a bus the driver has walked away from)
        h.driver_away = cx.people.on_foot.as_ref().is_some_and(|f| {
            let own = Some(crate::humans::BusId::Player);
            f.seat.map(|s| s.0) != own && f.inside.map(|i| i.0) != own
        });
        // (OMSI's `AIPassFactor`, the passengers setting in per cent)
        steps::humans_by_hour(h, w, cx.clock.clock.time, cx.common.settings.pax_density, cx.duty.duty.as_ref());
        // The periodic fillings of the map each get a frame of their own: the traffic's
        // (`traffic_heavy_frame`, set in its stage before this one), the strollers and
        // stops of the people's step (every second, in `tick_inner`) and this one (every
        // two seconds). Whichever finds its frame taken waits for the next; together in
        // one frame they were one long frame every two seconds. Not the first filling:
        // the timetable buses standing at their stops when the session starts look for
        // the people there in the frame the traffic was put out, and a frame later they
        // had passed the stop with nobody on it.
        let settled = h.started;
        let traffic_heavy = std::mem::take(&mut *cx.traffic.traffic_heavy_frame) && settled;
        h.defer_populate = traffic_heavy;
        let step_dt = if *cx.clock.paused { 0.0 } else { dt };
        let step_populates = settled && !traffic_heavy && cx.traffic.traffic.is_some() && h.stroll_timer - step_dt <= 0.0;
        *cx.people.humans_populate_t -= dt;
        if *cx.people.humans_populate_t <= 0.0 && !*cx.clock.paused && !traffic_heavy && !step_populates {
            *cx.people.humans_populate_t = 2.0;
            let t0 = std::time::Instant::now();
            h.populate(w, center);
            view_sync::apply_people(h, &mut cx.gfx.gfx.sim_view.people, w, r, scene);
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                h.stage_stats.add("app populate", t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
        if let (Some(cam), Some(s)) = (cx.view.camera.as_ref(), cx.gfx.gfx.surface.as_ref()) {
            h.eye = Some(humans::Eye::of(
                cam,
                s.config.width as f32 / s.config.height.max(1) as f32,
            ).widened(sight));
        }
        // (the other LAN players' buses, for their riders to sit in)
        h.set_remote_buses(cx.net.remotes.remotes.iter().map(|(id, r)| (*id, r.vehicle())));
        // (and the vehicles the player placed and left, with their riders)
        h.set_placed_buses(cx.player.placed.iter().map(|q| (q.uid, &q.vehicle)));
        h.set_player_next_stop(
            cx.duty.duty
                .as_ref()
                .and_then(|d| d.trip().stops.get(d.next_stop)),
        );
        let step_dt = if *cx.clock.paused { 0.0 } else { dt };
        // (OMSI_SIM_THREAD: what the people's step made during the last picture is taken
        // now, and their next step goes there with this frame's, see `sim_thread`)
        let mut took = false;
        if let Some(back) = cx.traffic.pipeline.take_people_stepped() {
            let started = Instant::now().checked_sub(std::time::Duration::from_secs_f64(back.secs)).unwrap_or_else(Instant::now);
            steps::humans_after_step(h, &mut cx.gfx.gfx.sim_view.people, back.dt, w, cx.player.player.as_mut(), cx.traffic.traffic.as_mut(), r, scene, started);
            took = back.took;
        }
        if cx.traffic.pipeline.takes_people(cx.traffic.traffic.as_ref().map(|t| &t.sim), h) {
            let bus = cx.player.player.as_ref().map(|p| h.player_bus_view(&p.vehicle));
            cx.traffic.pipeline.owe_people(step_dt, w.data(), bus);
        } else {
            took |= steps::tick_humans(
                h,
                &mut cx.gfx.gfx.sim_view.people,
                step_dt,
                w,
                cx.player.player.as_mut(),
                cx.traffic.traffic.as_mut(),
                r,
                scene,
            );
        }
        if let Some(m) = h.take_message() {
            *cx.ui.service_msg = Some((m, 6.0));
        }
        if let Some(dialog_name) = h.take_dialog() {
            let t = passenger_dialog(&dialog_name);
            // One question in roughly 10 passenger conversations, while still
            // avoiding a new dialog when another question is already active.
            // It is deliberately based on a conversation rather
            // than a frame, so a busy stop cannot spam the driver.
            *cx.people.passenger_dialogs += 1;
            if *cx.people.passenger_dialogs % 10 == 0 && cx.people.passenger_question.is_none() {
                let duty = cx.duty.duty.as_ref();
                let trip = duty.and_then(|d| d.trips.get(d.trip_index));
                let line = trip
                    .map(|t| t.line.trim())
                    .filter(|l| !l.is_empty())
                    .or_else(|| duty.map(|d| d.line.trim()).filter(|l| !l.is_empty()))
                    .unwrap_or("?")
                    .to_string();
                let terminus = trip.map(|t| t.terminus.trim()).filter(|s| !s.is_empty()).unwrap_or("le terminus").to_string();
                let next_index = duty.map(|d| d.next_stop).unwrap_or(0);
                let next_stop = trip.and_then(|t| t.stops.get(next_index)).map(|s| s.name.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "le prochain arrêt".into());
                let following_stop = trip.and_then(|t| t.stops.get(next_index + 1)).map(|s| s.name.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| terminus.clone());
                let first_stop = trip.and_then(|t| t.stops.iter().find(|s| s.stops)).map(|s| s.name.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "le premier arrêt".into());
                let correspondence = trip
                    .and_then(|t| t.stops.get(next_index))
                    .and_then(|s| h.stop_target_objects.as_ref()?.get(&s.object_id))
                    .and_then(|targets| targets.keys().find(|name| name.as_str() != next_stop.as_str()).cloned())
                    .unwrap_or_else(|| following_stop.clone());
                let kind = *cx.people.passenger_dialogs % 8;
                let tr = |key: &str| omsi_ui::tr(key).into_owned();
                let (question, correct_text, wrong_a, wrong_b) = match kind {
                    0 => (format!("{} {terminus} ?", tr("Vous allez à")), format!("Oui, la ligne {line} va jusqu'à {terminus}."), format!("Non, elle s'arrête à {next_stop}."), format!("Oui, mais descendez à {first_stop}.")),
                    1 => (format!("Quel est le prochain arrêt après {next_stop} ?"), format!("Après {next_stop}, c'est {following_stop}."), format!("C'est {terminus}."), format!("C'est {first_stop}.")),
                    2 => (format!("Où descendre pour aller vers {terminus} ?"), format!("Descendez au terminus {terminus}."), format!("Descendez à {next_stop}."), format!("Descendez à {first_stop}.")),
                    3 => (format!("La ligne {line} dessert-elle bien {terminus} ?"), format!("Oui, {line} dessert {terminus}."), format!("Non, cette ligne s'arrête à {next_stop}."), format!("Oui, mais uniquement dans l'autre sens.")),
                    4 => (format!("À quel arrêt puis-je changer pour poursuivre vers {correspondence} ?"), format!("Descendez à {correspondence} pour votre correspondance."), format!("Descendez à {terminus}, il n'y a pas de correspondance avant."), format!("Descendez à {next_stop}, c'est le terminus.")),
                    5 => (format!("Puis-je prendre la ligne {line} pour rejoindre {next_stop} ?"), format!("Oui, descendez à {next_stop}."), format!("Non, cette ligne ne dessert que {first_stop}."), format!("Oui, mais seulement après {terminus}.")),
                    6 => (format!("Dans quel sens roule le bus de la ligne {line} ?"), format!("Vers {terminus}."), format!("Vers {first_stop}."), format!("Il ne dessert aucun arrêt aujourd'hui.")),
                    _ => (format!("Pour la correspondance à {correspondence}, dois-je descendre avant le terminus ?"), format!("Oui, descendez à {correspondence}, avant {terminus}."), format!("Non, restez jusqu'à {terminus}."), format!("Oui, descendez dès {next_stop}, c'est le terminus.")),
                };
                let correct = (*cx.people.passenger_dialogs as usize) % 3;
                // Vary the two distractors as well as the position of the correct
                // answer, without adding another RNG to the simulation thread.
                let mut wrongs = [wrong_a, wrong_b];
                if *cx.people.passenger_dialogs % 2 == 0 {
                    wrongs.swap(0, 1);
                }
                let mut options = [wrongs[0].clone(), wrongs[1].clone(), correct_text];
                options.swap(correct, 2);
                *cx.people.passenger_question = Some(crate::app::PassengerQuestion {
                    question: question.clone(), options, correct, selected: 0, left: 30.0, closing: false,
                });
                passenger_tts(&question);
                *cx.ui.service_msg = Some((omsi_ui::tr("A passenger has a question: use the wheel, then click an answer").into_owned(), 8.0));
            } else if let Some(t) = t {
                *cx.ui.service_msg = Some((t, 5.0));
            }
        }
        if let Some(p) = cx.player.player.as_mut() {
            if took {
                p.vehicle.set_var("GivenTicket", -1.0);
            }
            h.give_ticket = std::mem::take(&mut p.give_ticket);
            h.give_change_all = std::mem::take(&mut p.give_change);
            if std::mem::take(&mut p.take_change) {
                h.take_change_tray();
            }
            if std::mem::take(&mut h.stop_request) {
                // a passenger's request is the vehicle trigger Omsi.exe fires
                // (0x62e42c), not the cab's stop button `door_haltewunsch`,
                // whose switch and brake sounds some buses play
                p.vehicle.trigger("int_haltewunsch");
            }
            h.write_pax_vars(&mut p.vehicle);
            p.vehicle.host.humans_on_path_link = h.path_link_counts();
            p.vehicle.host.humans_on_seat = h.seat_counts();
            let coins: Vec<usize> = std::mem::take(&mut p.vehicle.host.change_coins);
            h.give_change(w, r, scene, &coins);
        }
        // the view sync of the people: the coins and ticket blocks of the player's bus,
        // then everybody's pose (see `view_sync`), in the frame's last tick only
        if cx.clock.fixed_tick.view_due() {
            let bus = cx.player.player.as_ref().map(|p| &p.vehicle);
            view_sync::sync(ViewSync::people(h, bus, center), &mut cx.gfx.gfx.sim_view, w, r, scene);
        }
    }
}

impl App {
    pub(crate) fn passenger_question_wheel(&mut self, amount: f32) -> bool {
        let Some(q) = self.session.passenger_question.as_mut() else { return false };
        if q.closing { return true; }
        if amount.abs() < f32::EPSILON { return true; }
        q.selected = (q.selected as i32 - amount.signum() as i32).rem_euclid(3) as usize;
        true
    }

    pub(crate) fn passenger_question_click(&mut self, pressed: bool) -> bool {
        if !pressed { return self.session.passenger_question.is_some(); }
        let Some(surface) = self.gfx.surface.as_ref() else { return false };
        let (w, h) = (surface.config.width as f32, surface.config.height as f32);
        let (x, y) = self.input.cursor;
        if !(w * 0.25..=w * 0.75).contains(&x) || !(h * 0.30..=h * 0.66).contains(&y) { return false; }
        let panel_top = h * 0.30;
        let panel_height = h * (0.66 - 0.30);
        let row_height = panel_height * 0.27 / 3.0;
        let first_row = panel_top + panel_height * 0.40;
        if y < first_row || y >= first_row + row_height * 3.0 { return true; }
        let selected = (((y - first_row) / row_height).floor() as usize).min(2);
        let Some(q) = self.session.passenger_question.take() else { return false };
        if q.closing {
            self.session.passenger_question = Some(q);
            return true;
        }
        let right = selected == q.correct;
        let amount = if right { omsi_career::economy::PASSENGER_ANSWER_REWARD } else { -omsi_career::economy::PASSENGER_ANSWER_FINE };
        let _change = self.session.career.wallet_change(
            amount,
            if right { "passenger question: selected the correct route answer" } else { "passenger question: selected an incorrect route answer" },
        );
        let mut closing = q;
        closing.selected = selected;
        closing.left = 0.45;
        closing.closing = true;
        self.session.passenger_question = Some(closing);
        true
    }
}

impl App {
    /// Explain the automatic accident verdict instead of presenting an unexplained
    /// debit or reward. This is conservative when no other vehicle is close enough.
    fn accident_fault_assessment(&self, player_speed_kmh: f32) -> (bool, String) {
        let Some(player) = self.player.as_ref() else {
            return (true, "the player's bus could not be identified after the collision".into());
        };
        let Some(traffic) = self.session.traffic.as_ref() else {
            return (
                player_speed_kmh > 2.0,
                if player_speed_kmh > 2.0 {
                    format!("no other vehicle was detected; the bus hit an obstacle while moving at {player_speed_kmh:.1} km/h")
                } else {
                    "no other vehicle was detected and the contact occurred at very low speed".into()
                },
            );
        };
        let Some((player_lane, player_s, _)) = traffic.net.nearest_lane_near(player.vehicle.position, omsi_sim::traffic::LaneKind::Street) else {
            return (player_speed_kmh > 2.0, "the bus was not on a known traffic lane when the collision occurred".into());
        };
        let Some((car, other_lane, other_s, distance)) = traffic.cars.iter().filter(|car| !car.gone).filter_map(|car| {
            let (lane, s, _) = traffic.net.nearest_lane_near(car.vehicle.position, omsi_sim::traffic::LaneKind::Street)?;
            let d = (car.vehicle.position - player.vehicle.position).truncate().length();
            (d < 14.0).then_some((car, lane, s, d))
        }).min_by(|a, b| a.3.total_cmp(&b.3)) else {
            return (
                player_speed_kmh > 2.0,
                if player_speed_kmh > 2.0 {
                    format!("no other vehicle was within 14 m; the bus hit a fixed obstacle while moving at {player_speed_kmh:.1} km/h")
                } else {
                    "no other vehicle was within 14 m and the contact happened at very low speed".into()
                },
            );
        };
        let priority_conflict = traffic.net.crossings[player_lane].iter().any(|crossing| {
            crossing.other == other_lane
                && traffic.net.must_yield(player_lane, other_lane)
                && (-4.0..=12.0).contains(&(crossing.at - player_s))
                && (-4.0..=12.0).contains(&(crossing.other_at - other_s))
        });
        if priority_conflict {
            return (true, format!("the other vehicle had priority at the crossing ({distance:.1} m away), but the bus entered its path"));
        }
        if player_speed_kmh > car.state.speed * 3.6 + 8.0 {
            return (true, format!("the bus approached the other vehicle too quickly ({player_speed_kmh:.1} km/h versus {:.1} km/h)", car.state.speed * 3.6));
        }
        (false, format!("the other vehicle was already in the collision area ({distance:.1} m away) and no player priority violation was detected"))
    }

    fn check_red_light(&mut self) {
        let candidate = {
            let (Some(t), Some(p)) = (self.session.traffic.as_ref(), self.player.as_ref()) else { return };
            let speed = p.vehicle.physics.velocity_kmh().abs() / 3.6;
            let mut found = None;
            for (lane, distance) in t.lanes_ahead_of(p.vehicle.position, p.vehicle.heading, 18.0).into_iter().skip(1) {
                let Some((controller, light)) = t.net.lanes[lane].traffic_light else { continue };
                let Some((state, _)) = t.light_state(controller, light) else { continue };
                use omsi_sim::traffic::{Aspect, TrafficLightController};
                let red = matches!(TrafficLightController::aspect(state), Aspect::Red | Aspect::RedYellow);
                found = Some((controller, light, distance, red, speed));
                break;
            }
            found
        };
        let watch = self.session.red_light_watch;
        match (watch, candidate) {
            (None, Some((controller, light, distance, true, _))) if distance <= 14.0 => {
                self.session.red_light_watch = Some(crate::app::RedLightWatch { controller, light });
            }
            (Some(w), Some((controller, light, _, true, _))) if w.controller == controller && w.light == light => {}
            (Some(w), Some((controller, light, _, false, _))) if w.controller == controller && w.light == light => {
                self.session.red_light_watch = None;
            }
            (Some(w), None) => {
                let speed = self.player.as_ref().map(|p| p.vehicle.physics.velocity_kmh().abs() / 3.6).unwrap_or(0.0);
                self.session.red_light_watch = None;
                if speed > 1.5 {
                    self.session.career.wallet_change(-omsi_career::economy::TRAFFIC_OFFENCE_FINE, "red light offence: the bus crossed a signal after it was red");
                }
            }
            (Some(_), Some(_)) => self.session.red_light_watch = None,
            _ => {}
        }
    }

    /// Road-rule checks use a short cooldown and require the bus to be moving on
    /// the offending approach, so broad turns and parking manoeuvres stay harmless.
    fn check_road_offences(&mut self, dt: f32) {
        self.session.road_offence_cool = (self.session.road_offence_cool - dt).max(0.0);
        self.session.priority_offence_cool = (self.session.priority_offence_cool - dt).max(0.0);
        let (wrong_way, priority_refusal, traffic_to_yield) = {
            let (Some(t), Some(p)) = (self.session.traffic.as_ref(), self.player.as_ref()) else {
                self.session.wrong_way_watch = (self.session.wrong_way_watch - dt * 2.0).max(0.0);
                return;
            };
            // Use the signed forward speed. Reversing, turning around, and moving slowly
            // across a wide junction must not be reported as driving the wrong way.
            let speed = p.vehicle.physics.speed.max(0.0);
            let Some((lane, s, distance)) = t.net.nearest_lane_near(p.vehicle.position, omsi_sim::traffic::LaneKind::Street) else { return };
            let heading = t.net.lanes[lane].at(s).1 as f64;
            let heading_error = (p.vehicle.heading - heading + 540.0).rem_euclid(360.0) - 180.0;
            let has_forward_lane = t.net.lane_along(
                p.vehicle.position,
                p.vehicle.heading,
                omsi_sim::traffic::LaneKind::Street,
                3.0,
                70.0,
            ).is_some();
            let wrong = speed > 2.0
                && distance < 2.5
                && heading_error.abs() > 135.0
                && !has_forward_lane;
            // Check both vehicles against the same crossing, not merely against
            // neighbouring lanes. This prevents unrelated traffic at a junction
            // from being treated as a priority conflict.
            let priority_conflict = t.net.crossings[lane].iter().any(|crossing| {
                let player_gap = crossing.at - s;
                t.net.must_yield(lane, crossing.other)
                    && (-2.0..=12.0).contains(&player_gap)
                    && t.cars.iter().any(|car| {
                        let Some((other, car_s, _)) = t.net.nearest_lane_near(car.vehicle.position, omsi_sim::traffic::LaneKind::Street) else { return false };
                        let car_gap = crossing.other_at - car_s;
                        other == crossing.other
                            && (-3.0..=12.0).contains(&car_gap)
                            && car.state.speed > 1.5
                    })
            });
            let priority_refusal = priority_conflict && speed > 3.0;
            let traffic_to_yield = priority_conflict
                && speed < 1.0
                && self.session.red_light_watch.is_none();
            (wrong, priority_refusal, traffic_to_yield)
        };
        if wrong_way {
            self.session.wrong_way_watch += dt;
        } else {
            self.session.wrong_way_watch = (self.session.wrong_way_watch - dt * 2.0).max(0.0);
        }
        if self.session.wrong_way_watch >= 1.0 && self.session.road_offence_cool <= 0.0 {
            self.session.road_offence_cool = 8.0;
            self.session.wrong_way_watch = 0.0;
            self.session.career.wallet_change(-omsi_career::economy::WRONG_WAY_FINE, "wrong-way offence: the bus travelled forward against the street lane direction");
        }
        if priority_refusal && self.session.priority_offence_cool <= 0.0 {
            self.session.priority_offence_cool = 8.0;
            self.session.career.wallet_change(-omsi_career::economy::PRIORITY_OFFENCE_FINE, "priority offence: the bus entered a crossing while a priority vehicle was approaching");
        }
        if self.session.courtesy_watch > 0.0 {
            let stopped = self.player.as_ref().is_some_and(|p| p.vehicle.physics.velocity_kmh().abs() < 3.0);
            if traffic_to_yield && stopped {
                self.session.courtesy_watch = (self.session.courtesy_watch + dt).min(2.0);
            } else if !traffic_to_yield {
                self.session.courtesy_watch -= dt;
                if self.session.courtesy_watch <= 0.0 && stopped {
                    self.session.career.wallet_change(omsi_career::economy::COURTESY_REWARD, "courtesy reward: the bus waited and let a priority vehicle clear the crossing");
                    self.session.courtesy_watch = -8.0;
                }
            } else if !stopped {
                self.session.courtesy_watch = 0.0;
            }
        } else if self.session.courtesy_watch < 0.0 {
            self.session.courtesy_watch = (self.session.courtesy_watch + dt).min(0.0);
        } else if traffic_to_yield {
            self.session.courtesy_watch = 1.5;
        }
    }
}

/// Best-effort TTS. The game remains fully playable when no TTS provider is installed.
fn passenger_tts(text: &str) {
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &format!("Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak('{}')", text.replace('\'', "''"))]).spawn(); }
    #[cfg(not(target_os = "windows"))]
    { let _ = std::process::Command::new("espeak-ng").args(["-v", "fr", text]).spawn().or_else(|_| std::process::Command::new("espeak").args(["-v", "fr", text]).spawn()); }
}

/// The player on foot, and the other players walking about.
pub(super) fn on_foot(mut cx: crate::on_foot::OnFootCx, dt: f32) {
    let dt = if *cx.clock.paused { 0.0 } else { dt };
    crate::on_foot::tick(&mut cx, dt);
    crate::on_foot::remote_walkers(&mut cx);
}

/// OMSI's hint for what a passenger has just said (`TooCold_1`: "Passengers complain about
/// the low temperature in your bus!"), from the game's language files
/// (`Languages/<LANG>_basic*.olf`, `RL_PassDlg_<name>`): Omsi.exe shows them as the riders
/// speak. None for what has no hint (a greeting).
fn passenger_dialog(name: &str) -> Option<String> {
    static TEXTS: std::sync::OnceLock<Option<omsi_content::language::Language>> = std::sync::OnceLock::new();
    let texts = TEXTS.get_or_init(|| {
        let lang = crate::settings::Settings::load().language;
        let load = |lang: &str| {
            let mut out: Option<omsi_content::language::Language> = None;
            for dir in omsi_cfg::content_dirs("Languages") {
                for file in ["basic", "basic_103", "basic_200"] {
                    if let Ok(l) = omsi_content::language::Language::load(&dir.join(format!("{lang}_{file}.olf"))) {
                        match out.as_mut() {
                            Some(o) => o.merge(l),
                            None => out = Some(l),
                        }
                    }
                }
            }
            out
        };
        load(&lang).or_else(|| load("ENG"))
    });
    let t = texts.as_ref()?.strings.get(&format!("RL_PassDlg_{name}"))?.trim().trim_matches('"').replace("\"\"", "\"");
    (!t.is_empty()).then_some(t)
}
