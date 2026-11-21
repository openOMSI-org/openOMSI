//! The IBIS and the destination: typing a trip in, the HTML displays' requests.

use super::*;

impl Player {
    /// Type on into the IBIS; when the typing could not be done, the IBIS variables are
    /// written directly.
    pub(crate) fn tick_ibis(&mut self, dt: f32) {
        let Some((mut typist, line, terminus, stops)) = self.ibis_typist.take() else {
            return;
        };
        if typist.tick(&mut self.vehicle, dt) {
            self.ibis_typist = Some((typist, line, terminus, stops));
            return;
        }
        match typist.outcome() {
            Some(Ok(what)) => log::info!("IBIS set to line {line} terminus {terminus}: {what}"),
            other => {
                log::info!(
                    "IBIS: {}; line {line} terminus {terminus} set directly",
                    other
                        .and_then(|o| o.as_ref().err())
                        .cloned()
                        .unwrap_or_default()
                );
                let hof = self.vehicle.host.hof.clone();
                let stops: Vec<&str> = stops.iter().map(String::as_str).collect();
                schedule::set_player_destination_directly(
                    &mut self.vehicle,
                    hof.as_deref(),
                    &line,
                    &terminus,
                    &stops,
                );
            }
        }
    }

    /// A page moved the duty to stop `stop` of `trip` (`omsi.setNextStop`): the IBIS follows,
    /// forwards or backwards, by `IBIS_busstop` / `ibox_busstop` (its keys only count up, so
    /// the typist cannot do this). Without a route in the IBIS there is nothing to move.
    pub(crate) fn ibis_to_stop(&mut self, trip: &schedule::PlannedTrip, stop: usize) -> bool {
        let Some(hof) = self.vehicle.host.hof.clone() else {
            return false;
        };
        let Some(route) = self
            .vehicle
            .var("IBIS_RouteIndex")
            .or_else(|| self.vehicle.var("ibox_routenindex"))
            .filter(|r| *r >= 0.0)
        else {
            return false;
        };
        if self.vehicle.var("IBIS_busstop").is_none() && self.vehicle.var("ibox_busstop").is_none()
        {
            return false;
        }
        let Some(name) = trip.stops.get(stop).map(|s| s.name.clone()) else {
            return false;
        };
        if let Some(i) = schedule::ibis_stop_index(&hof, route.round() as usize, &name, stop) {
            let mut applied = false;
            for var in ["IBIS_busstop", "ibox_busstop"] {
                if self.vehicle.var(var).is_some() {
                    self.vehicle.set_var(var, i as f32);
                    applied = true;
                }
            }
            if self.vehicle.var("ibox_TTBusstopIndexLAST").is_some() {
                self.vehicle.set_var("ibox_TTBusstopIndexLAST", stop as f32);
                applied = true;
            }
            applied
        } else {
            false
        }
    }

    /// The game types the duty into the IBIS by itself: no plugin leaves it to the driver.
    pub(crate) fn ibis_auto(&self) -> bool {
        self.ibis_holds.is_empty()
    }

    /// Whether an auto-start or IBIS typing is actively setting up the trip (#2267).
    pub(crate) fn ibis_busy(&self) -> bool {
        self.startup.is_some() || self.ibis_typist.is_some() || self.ibis_duty.is_some()
    }

    /// Update the IBIS destination and next stop according to the duty schedule (#2267).
    pub(crate) fn update_duty_ibis(&mut self, d: &mut schedule::PlayerDuty) {
        if d.take_trip_change() {
            self.duty_trip_pending = true;
            self.ibis_shown = None;
        }
        if self.duty_trip_pending && self.duty_typed && self.ibis_auto() {
            let (trip, stop) = d.trip_for_ibis();
            self.set_duty_destination(trip, stop);
            self.duty_trip_pending = false;
        }
        let auto_ibis = self.duty_typed && self.ibis_auto();
        if omsi_sim::timetable_run::PlayerDuty::ibis_stop_stale(
            self.ibis_shown,
            d.trip_for_ibis().1,
            auto_ibis,
            self.ibis_auto_stop,
        ) && !self.ibis_busy()
        {
            let (trip, stop) = d.trip_for_ibis();
            if self.ibis_to_stop(trip, stop) {
                self.ibis_shown = Some(stop);
            }
        }
    }

    /// Put the duty's trip on the IBIS, at the stop the bus is at: now, or when the running
    /// auto-start is done. Not while a plugin leaves the IBIS to the driver
    /// (`duty.set_auto_ibis`): the driver types line, route and destination.
    pub(crate) fn set_duty_destination(&mut self, trip: &schedule::PlannedTrip, stop: usize) {
        if !duty_selected(&mut self.duty_typed, &self.ibis_holds) {
            log::info!(
                "IBIS: line {} to {} left for the driver to type",
                trip.line.trim(),
                trip.terminus.trim()
            );
            return;
        }
        let stops: Vec<String> = trip.stops.iter().map(|s| s.name.clone()).collect();
        let name = trip
            .stops
            .get(stop)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        if self.startup.is_some() {
            self.ibis_duty = Some((
                trip.line.clone(),
                trip.terminus.clone(),
                stops,
                (stop, name),
            ));
        } else {
            self.type_destination(&trip.line, &trip.terminus, &stops, (stop, &name));
        }
    }

    pub(crate) fn type_destination(
        &mut self,
        line: &str,
        terminus: &str,
        stops: &[String],
        stop: (usize, &str),
    ) {
        if let Some((mut old, ..)) = self.ibis_typist.take() {
            old.abandon(&mut self.vehicle);
        }
        // (the trip's destination, not one picked by hand before)
        self.blind_pick = None;
        let hof = self.vehicle.host.hof.clone();
        // the keys a driver reaches: the cockpit's clickable switches and the keyboard's
        let mut keys: hashbrown::HashSet<String> = self
            .vehicle
            .ty
            .model
            .meshes
            .iter()
            .filter_map(|m| m.mouse_event.as_ref())
            .map(|e| e.trim().to_ascii_lowercase())
            .collect();
        keys.extend(
            self.bound_actions()
                .iter()
                .map(|a| a.trim().to_ascii_lowercase()),
        );
        let operable = |name: &str| keys.contains(&name.trim().to_ascii_lowercase());
        let names: Vec<&str> = stops.iter().map(String::as_str).collect();
        match schedule::player_ibis(
            &mut self.vehicle,
            hof.as_deref(),
            line,
            terminus,
            &names,
            Some(stop),
            &operable,
            self.ibis_background,
        ) {
            Some(typist) => {
                self.ibis_typist = Some((
                    typist,
                    line.to_string(),
                    terminus.to_string(),
                    stops.to_vec(),
                ))
            }
            None => log::info!("IBIS: nothing to type for line {line} terminus {terminus}"),
        }
    }

    /// What the bus's HTML pages asked of the IBIS (`omsi.setRoute`, `omsi.setLine`,
    /// `omsi.setDestination`, `omsi.setNextStop`): a route is typed as a driver does (line, route, destination),
    /// a destination alone is written to the sign.
    pub(crate) fn apply_html_requests(&mut self) {
        let requests = self.vehicle.take_html_requests();
        if requests.is_empty() {
            return;
        }
        let (stops, requests): (Vec<_>, Vec<_>) = requests
            .into_iter()
            .partition(|r| matches!(r, omsi_sim::htmltex::HtmlRequest::SetNextStop(_)));
        if let Some(omsi_sim::htmltex::HtmlRequest::SetNextStop(i)) = stops.last() {
            log::info!("HTML page: setNextStop({i}) received");
            self.html_next_stop = Some(*i);
        }
        if requests.is_empty() {
            return;
        }
        let Some(hof) = self.vehicle.host.hof.clone() else {
            log::info!("HTML page: the bus has no depot file, {requests:?} ignored");
            return;
        };
        for req in requests {
            match req {
                omsi_sim::htmltex::HtmlRequest::SetRoute(i) => self.set_route_from_page(&hof, i),
                omsi_sim::htmltex::HtmlRequest::SetLine(line) => {
                    let wanted = line.trim();
                    let digits: String =
                        wanted.chars().take_while(|c| c.is_ascii_digit()).collect();
                    let found = hof
                        .info_trips
                        .iter()
                        .position(|t| route_line(t).eq_ignore_ascii_case(wanted))
                        .or_else(|| {
                            hof.info_trips
                                .iter()
                                .position(|t| !digits.is_empty() && route_line(t) == digits)
                        });
                    match found {
                        Some(i) => self.set_route_from_page(&hof, i),
                        None => {
                            log::info!("HTML page: depot file {} has no line '{wanted}'", hof.name)
                        }
                    }
                }
                omsi_sim::htmltex::HtmlRequest::SetNextStop(_) => {}
                omsi_sim::htmltex::HtmlRequest::ClearLine => {
                    if let Some((mut old, ..)) = self.ibis_typist.take() {
                        old.abandon(&mut self.vehicle);
                    }
                    for n in ["IBIS_LinieKurs", "IBIS_Linie_Complex", "IBIS_Linie_Suffix"] {
                        self.vehicle.set_var(n, 0.0);
                    }
                    self.vehicle.set_var("IBIS_RouteIndex", -1.0);
                    if let Some(i) = self.vehicle.ty.program.str_var("IBIS_Complex_Line") {
                        self.vehicle.state.str_vars[i as usize] = "     ".into();
                    }
                    if let Some(i) = self.vehicle.ty.program.str_var("SetLineTo") {
                        self.vehicle.state.str_vars[i as usize] = String::new();
                    }
                    log::info!("HTML page: line cleared");
                }
                omsi_sim::htmltex::HtmlRequest::SetDestination(ti) => {
                    let Some(term) = hof.termini.get(ti) else {
                        log::info!("HTML page: depot file {} has no destination {ti}", hof.name);
                        continue;
                    };
                    let keep = self.vehicle.host.tt_line.trim().is_empty();
                    let line = {
                        let l = self.vehicle.host.tt_line.trim().to_string();
                        if !l.is_empty() {
                            l
                        } else {
                            let c = self.vehicle.str_var("IBIS_Complex_Line").trim().to_string();
                            if !c.is_empty() {
                                c
                            } else {
                                self.vehicle
                                    .var("IBIS_LinieKurs")
                                    .filter(|n| *n > 0.5)
                                    .map(|n| format!("{}", n.round() as i64))
                                    .unwrap_or_default()
                            }
                        }
                    };
                    let wanted = if term.texture_id.trim().is_empty() {
                        term.strings.first().cloned().unwrap_or_default()
                    } else {
                        term.texture_id.clone()
                    };
                    log::info!("HTML page: destination {ti} '{wanted}' on line '{line}'");
                    if let Some((mut old, ..)) = self.ibis_typist.take() {
                        old.abandon(&mut self.vehicle);
                    }
                    self.set_destination_by_hand(&hof, &line, ti, keep);
                }
            }
        }
    }

    /// Row `ti` of the depot file on the destination display, set by hand (the destination
    /// list, a bus page): on the IBIS now, on a roller blind once it can be turned
    /// (`schedule::turn_roller_blind`).
    pub(crate) fn set_destination_by_hand(
        &mut self,
        hof: &omsi_vehicle::hof::Hof,
        line: &str,
        ti: usize,
        keep_shown: bool,
    ) {
        self.set_destination_by_hand_with_display(hof, line, None, ti, keep_shown);
    }

    pub(crate) fn set_destination_by_hand_with_display(
        &mut self,
        hof: &omsi_vehicle::hof::Hof,
        line: &str,
        display: Option<&str>,
        ti: usize,
        keep_shown: bool,
    ) {
        let blind = self
            .blind_pick
            .as_ref()
            .map(|p| p.line.clone())
            .unwrap_or_else(|| self.vehicle.str_var("SetLineTo"));
        let shown = display
            .map(str::to_string)
            .or_else(|| crate::game_lists::kept_blind_line(&blind, keep_shown));
        self.blind_pick = schedule::set_player_destination_with_display(
            &mut self.vehicle,
            hof,
            line,
            display,
            ti,
            &[],
        );
        if let (Some(p), Some(shown)) = (self.blind_pick.as_mut(), shown) {
            p.line = shown;
        }
        schedule::turn_roller_blind(
            &mut self.vehicle,
            &mut self.blind_pick,
            self.startup.is_some(),
        );
        if let Some(p) = &self.blind_pick {
            log::info!(
                "roller blind: destination {} kept until the bus is switched on",
                p.row
            );
        }
    }

    /// Type route `i` of the depot file (`omsi.depot.routes[i]`) into the IBIS.
    fn set_route_from_page(&mut self, hof: &omsi_vehicle::hof::Hof, i: usize) {
        let Some(t) = hof.info_trips.get(i) else {
            log::info!("HTML page: depot file {} has no route {i}", hof.name);
            return;
        };
        let code = omsi_cfg::parse_i32(&t.route);
        let Some(term) = hof.termini.iter().find(|x| x.code == code) else {
            log::info!(
                "HTML page: route {i} ({}) leads to destination code {code}, which the depot file lacks",
                t.name.trim()
            );
            return;
        };
        // (the route's own stops, so that this route is typed and not another of the line to
        // the same terminus)
        let stops: Vec<String> = hof
            .info_busstop_lists
            .get(i)
            .map(|l| l.iter().map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();
        let name = stops.first().cloned().unwrap_or_default();
        let wanted = if term.texture_id.trim().is_empty() {
            term.strings.first().cloned().unwrap_or_default()
        } else {
            term.texture_id.clone()
        };
        let line = route_line(t);
        log::info!(
            "HTML page: route {i} '{}' (line '{line}' (file: '{}') to '{wanted}')",
            t.name.trim(),
            t.line.trim()
        );
        self.type_destination(&line, &wanted, &stops, (0, &name));
    }
}

/// A duty's trip is put on the IBIS: the duty is remembered as typed (`Player::duty_typed`,
/// its next trips are typed too) even while a plugin holds the IBIS (`holds`), so that the
/// trips after the last hold is let go are typed again. Whether the game types this one now.
fn duty_selected(duty_typed: &mut bool, holds: &std::collections::BTreeSet<u64>) -> bool {
    *duty_typed = true;
    holds.is_empty()
}

#[cfg(test)]
mod hold_tests {
    use super::duty_selected;

    /// Two plugins hold the IBIS, a duty is chosen (Shift+U), both let go: the next trip is
    /// typed by the game again, as `steps.rs` types a new trip only when `duty_typed` (review
    /// of #2877).
    #[test]
    fn a_duty_chosen_while_held_is_typed_from_the_next_trip_after_the_last_hold_goes() {
        let mut typed = false;
        let mut holds = std::collections::BTreeSet::new();
        holds.insert(1);
        holds.insert(2);
        assert!(
            !duty_selected(&mut typed, &holds),
            "not typed while a plugin holds the IBIS"
        );
        assert!(typed, "the duty is remembered all the same");
        holds.remove(&1);
        assert!(
            !(typed && duty_selected(&mut typed, &holds)),
            "the other plugin still holds it"
        );
        holds.remove(&2);
        assert!(
            typed && duty_selected(&mut typed, &holds),
            "the next trip is typed again"
        );
    }

    /// A trip change arrives while a plugin holds the IBIS: the change is not applied immediately,
    /// but retried once the hold is released.
    #[test]
    fn a_trip_change_while_held_is_retried_after_hold_released() {
        let mut holds = std::collections::BTreeSet::new();
        holds.insert(42);
        let mut duty_typed = true;
        let mut duty_trip_pending = true;

        assert!(!duty_selected(&mut duty_typed, &holds));
        assert!(duty_trip_pending);

        holds.remove(&42);
        assert!(duty_selected(&mut duty_typed, &holds));
        duty_trip_pending = false;
        assert!(!duty_trip_pending);
    }

    /// A trip and route change defers marking the IBIS stop shown while startup or typist is running,
    /// and invalidates `ibis_shown` across trip transition (#2267).
    #[test]
    fn deferred_trip_and_route_transition_waits_for_startup_and_typist() {
        use omsi_sim::timetable_run::{PlannedStop, PlannedTrip, PlayerDuty, StopDir};
        use omsi_vehicle::Hof;

        let stop = |name: &str| PlannedStop {
            object_id: 1,
            name: name.into(),
            arr: 0.0,
            dep: 0.0,
            position: None,
            dir: StopDir::Forward,
            stops: true,
            closed: false,
        };
        let trip1 = PlannedTrip {
            name: "Trip 1".into(),
            line: "100".into(),
            terminus: "Terminus A".into(),
            departure: 0.0,
            end: 600.0,
            stops: vec![stop("Stop 1"), stop("Stop 2")],
        };
        let trip2 = PlannedTrip {
            name: "Trip 2".into(),
            line: "100".into(),
            terminus: "Terminus B".into(),
            departure: 700.0,
            end: 1300.0,
            stops: vec![stop("Stop 2"), stop("Stop 3")],
        };
        let mut duty = PlayerDuty::new("100".into(), "1".into(), vec![trip1, trip2], 0, 0, true);

        let mut v =
            crate::schedule::tests::script_test_vehicle("", "IBIS_RouteIndex\nIBIS_busstop\n", "");
        v.set_var("IBIS_RouteIndex", 0.0);
        v.set_var("IBIS_busstop", 0.0);
        v.host.hof = Some(std::sync::Arc::new(Hof {
            info_busstop_lists: vec![
                vec!["Stop 1".into(), "Stop 2".into()],
                vec!["Stop 2".into(), "Stop 3".into()],
            ],
            ..Default::default()
        }));

        let mut player = crate::spawn::test_player(v, vec![]);
        player.duty_typed = true;
        player.ibis_auto_stop = true;

        // Initial trip update: stop 0 is typed and shown
        player.update_duty_ibis(&mut duty);
        assert_eq!(player.ibis_shown, Some(0));
        assert_eq!(player.vehicle.var("IBIS_busstop"), Some(0.0));

        // Advance duty to next stop 1 on trip 1
        duty.next_stop = 1;
        player.update_duty_ibis(&mut duty);
        assert_eq!(player.ibis_shown, Some(1));
        assert_eq!(player.vehicle.var("IBIS_busstop"), Some(1.0));

        // Advance to trip 2 and make the player busy with auto-start / typist
        duty.start_at(1, 0);
        player.ibis_duty = Some((
            "100".into(),
            "Terminus B".into(),
            vec!["Stop 2".into(), "Stop 3".into()],
            (0, "Stop 2".into()),
        ));
        assert!(player.ibis_busy());

        // While busy, update_duty_ibis resets shown stop but defers setting the new stop
        player.update_duty_ibis(&mut duty);
        assert_eq!(
            player.ibis_shown, None,
            "deferred during transition while busy"
        );
        assert_eq!(
            player.vehicle.var("IBIS_busstop"),
            Some(1.0),
            "retains old stop while transition is busy"
        );

        // When typing completes, update_duty_ibis applies the new trip's stop
        player.ibis_duty = None;
        assert!(!player.ibis_busy());
        player.vehicle.set_var("IBIS_RouteIndex", 1.0);
        player.update_duty_ibis(&mut duty);
        assert_eq!(
            player.ibis_shown,
            Some(0),
            "shown stop updated after typing completes"
        );
        assert_eq!(player.vehicle.var("IBIS_busstop"), Some(0.0));
    }
}
