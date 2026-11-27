//! Legacy graphical-LAN host publication kept separate from the CPU headless host.
use super::*;

impl LanWorld {
    pub(super) fn host(
        &mut self,
        lan: &mut LanSession,
        dt: f32,
        mut traffic: Option<&mut Traffic>,
        mut humans: Option<&mut Humans>,
        me: Option<DVec3>,
    ) {
        // where the players are: the world is kept around them too
        let players: Vec<(u32, DVec3)> = lan
            .peers()
            .filter(|p| p.has_pose && p.pose.has_vehicle())
            .map(|p| (p.pose.id, DVec3::new(p.pose.x, p.pose.y, p.pose.z)))
            .collect();
        let centers: Vec<DVec3> = players.iter().map(|p| p.1).collect();
        if let Some(t) = traffic.as_deref_mut() {
            t.lan_centers = centers.clone();
            // and what they see: no car may come or go there in front of them either
            // (the list is kept: no allocation a frame)
            t.lan_eyes.clear();
            t.lan_eyes.extend(
                lan.peers()
                    .filter(|p| p.has_pose && p.pose.has_vehicle())
                    .map(|p| {
                        let at = DVec3::new(p.pose.x, p.pose.y, p.pose.z);
                        crate::traffic::Viewer::lan_player(at, p.pose.heading as f64)
                    }),
            );
        }
        if let Some(h) = humans.as_deref_mut() {
            h.lan_centers = centers;
        }
        // waiting people a client's bus takes
        for (id, ids) in lan.take_claims() {
            let granted = humans
                .as_deref_mut()
                .map(|h| h.hand_over(id, &ids))
                .unwrap_or_default();
            let denied: Vec<u32> = ids
                .iter()
                .copied()
                .filter(|i| !granted.contains(i))
                .collect();
            if !granted.is_empty() || !denied.is_empty() {
                log::info!(
                    "LAN: player {id}'s bus takes {} waiting passenger(s) {:?}{}",
                    granted.len(),
                    granted,
                    if denied.is_empty() {
                        String::new()
                    } else {
                        format!("; {denied:?} have boarded another bus")
                    }
                );
            }
            lan.answer_claim(id, &granted, &denied);
            if let Some(v) = self.views.get_mut(&id) {
                for g in &granted {
                    v.sent.remove(&(true, *g));
                }
            }
        }
        // descriptions asked for again (lost)
        let wants = lan.take_wants();
        let t_ref = traffic.as_deref();
        let h_ref = humans.as_deref();
        for (id, refs) in wants {
            for r in refs {
                if let Some(d) = describe(h_ref, r) {
                    lan.send_desc(id, &d);
                }
            }
        }
        self.views
            .retain(|id, _| players.iter().any(|p| p.0 == *id));
        let players_at = players.clone();
        let lan_me_pos = me;
        for (id, at) in players {
            let view = self.views.entry(id).or_default();
            view.acc += dt;
            view.lights_acc += dt;
            for s in view.sent.values_mut() {
                s.1 += dt;
            }
            if view.acc + 1.0e-4 < nw::MOVING_EVERY {
                continue;
            }
            view.acc = (view.acc - nw::MOVING_EVERY).clamp(0.0, nw::MOVING_EVERY);
            let mut g = Gather::default();
            if let Some(t) = t_ref {
                // (the cars themselves go by the table, below; what their scripts need of the
                // host's timetable goes by the car's id there)
                let mut cars = std::collections::HashSet::new();
                for c in &t.cars {
                    if (c.vehicle.position - at).truncate().length() > CAR_RADIUS {
                        continue;
                    }
                    let Some(tid) = t.table.id_of(c.id) else {
                        continue;
                    };
                    cars.insert(tid.raw());
                    let sent = view.context_pages.entry(tid.raw()).or_default();
                    let context = ai_context(&c.vehicle.host);
                    let Some(pages) = context.pages() else {
                        continue;
                    };
                    if !sent.due(MOVING_EVERY_S, c.is_bus(), pages) {
                        continue;
                    }
                    if let Some(packet) = context.packet(
                        omsi_net::PROTOCOL as u8,
                        tid.raw(),
                        lan.stamp_ms(),
                        sent.page,
                    ) {
                        lan.send_ai_context(id, &packet);
                        sent.page = sent.page.wrapping_add(1);
                        sent.sent += 1;
                        sent.age = 0.0;
                    }
                }
                view.context_pages.retain(|k, _| cars.contains(k));
            }
            if let Some(h) = h_ref {
                gather_people(view, h, t_ref.map(|t| &t.table), at, &mut g);
            }
            let Gather {
                mut frame,
                mut seen,
                mut descs,
                mut ids_hash,
            } = g;
            // the riders of our own bus, and the other players' people and riders: everybody
            // sees everybody's passengers
            let mut others: Vec<(PersonState, String)> = Vec::new();
            if let Some(h) = h_ref {
                for p in h.lan_riders() {
                    let Some((_, l, lh, seat)) = p.aboard else {
                        continue;
                    };
                    if p.id > nw::MAX_ID
                        || (lan_me_pos
                            .map(|m| (m - at).truncate().length() > CAR_RADIUS)
                            .unwrap_or(true))
                    {
                        continue;
                    }
                    others.push((
                        PersonState {
                            id: p.id,
                            activity: net_activity(p.activity),
                            place: PersonPlace::Aboard {
                                bus: nw::PLAYER_BUS | lan.my_id,
                                x: l.x,
                                y: l.y,
                                z: l.z,
                                heading: lh as f32,
                                seat: seat.map(|s| s.min(254) as u8),
                            },
                        },
                        Humans::type_file(&p.ty),
                    ));
                }
            }
            for (peer, up) in &self.ups {
                if *peer == id {
                    continue;
                }
                let bus_near = players_at
                    .iter()
                    .any(|(pp, pos)| pp == peer && (*pos - at).truncate().length() < CAR_RADIUS);
                for (pid, track) in &up.tracks {
                    let (Some(local), Some(last), Some(file)) =
                        (up.ids.get(pid), track.samples.last(), up.files.get(pid))
                    else {
                        continue;
                    };
                    let near = match last.v.place {
                        PersonPlace::Foot { x, y, .. } => {
                            (glam::DVec2::new(x, y) - at.truncate()).length() < PERSON_RADIUS
                        }
                        PersonPlace::Aboard { .. } => bus_near,
                    };
                    if !near {
                        continue;
                    }
                    let mut st = last.v;
                    st.id = RELAYED | ((local - (1 << 29)) & 0x3F_FFFF);
                    if let PersonPlace::Foot {
                        ref mut waiting, ..
                    } = st.place
                    {
                        *waiting = None;
                    }
                    others.push((st, file.clone()));
                }
            }
            for (st, file) in others {
                let key = (true, st.id);
                seen.insert(key);
                ids_hash ^= fnv(&(st.id as u64 | 1 << 40).to_le_bytes());
                if !view.described.contains_key(&key) {
                    view.described.insert(key, 0);
                    descs.push(Desc::Person { id: st.id, file });
                }
                let q = person_q(&st);
                let due = match view.sent.get(&key) {
                    None => true,
                    Some((last, age)) => *last != q || *age >= nw::STANDING_EVERY,
                };
                if !due {
                    continue;
                }
                view.sent.insert(key, (q, 0.0));
                frame.people.push(st);
            }
            if view.lights_acc >= nw::LIGHTS_EVERY {
                view.lights_acc = 0.0;
                let keys: Vec<u32> = self
                    .departed
                    .iter()
                    .filter_map(|k| u32::try_from(*k).ok())
                    .collect();
                frame.parked = Some((keys.len() == self.departed.len(), keys));
                if let Some(t) = t_ref {
                    frame.lights = t
                        .light_states(at, LIGHT_RADIUS)
                        .into_iter()
                        .filter(|(o, _, _)| (0..=u32::MAX as i64).contains(o))
                        .map(|(object, time, held)| LightState { object, time, held })
                        .collect();
                }
                frame.points = self.points.around(id);
            }
            // what went out of sight (or away) since the last frame
            let gone: Vec<Key> = view
                .sent
                .keys()
                .filter(|k| !seen.contains(*k))
                .copied()
                .collect();
            for k in gone {
                view.sent.remove(&k);
                view.gone.push((k, 1.0));
            }
            for g in view.gone.iter_mut() {
                g.1 -= nw::MOVING_EVERY;
            }
            view.gone.retain(|g| g.1 > 0.0 && !seen.contains(&g.0));
            frame.gone = view.gone.iter().map(|(k, _)| *k).collect();
            view.last_counts = (seen.iter().filter(|k| k.0).count(), frame.lights.len());
            view.ids_hash = ids_hash;
            for d in &descs {
                lan.send_desc(id, d);
            }
            if let Some(f) = self.trace.as_mut() {
                let ms = lan.stamp_ms();
                for s in t_ref
                    .iter()
                    .flat_map(|t| t.table.iter())
                    .filter(|s| s.row.distance_to([at.x, at.y, at.z]) <= CAR_RADIUS)
                {
                    let [x, y, z] = s.row.position();
                    let _ = writeln!(
                        f,
                        "host,{ms},{id},c,{},{x:.3},{y:.3},{z:.3},{:.2}",
                        s.id.raw(),
                        s.row.heading_deg()
                    );
                }
                for p in &frame.people {
                    if let PersonPlace::Foot {
                        x, y, z, heading, ..
                    } = p.place
                    {
                        let _ = writeln!(
                            f,
                            "host,{ms},{id},p,{},{x:.3},{y:.3},{z:.3},{heading:.2}",
                            p.id
                        );
                    }
                }
            }
            let n = lan.send_world(id, &frame);
            view.bytes += n as u64;
        }
        // the cars: the table, to each player around where it is
        if let Some(t) = t_ref {
            let viewers: Vec<(u32, [f64; 3])> = players_at
                .iter()
                .map(|(id, p)| (*id, [p.x, p.y, p.z]))
                .collect();
            let n = lan.traffic_publish(&t.table, &viewers, dt, TRAFFIC_BYTES_PER_SECOND);
            self.traffic_bytes += n as u64;
        }
        self.report_host_diagnostics(lan, dt, t_ref);
        if let Some(f) = self.trace.as_mut() {
            let _ = f.flush();
        }
    }

    fn report_host_diagnostics(&mut self, lan: &LanSession, dt: f32, t_ref: Option<&Traffic>) {
        self.log_t -= dt;
        if self.log_t <= 0.0 && omsi_cfg::flags::OMSI_DEBUG_LAN.is_set() {
            self.log_t = 4.0;
            for (peer, up) in &self.ups {
                let aboard = up
                    .tracks
                    .values()
                    .filter(|t| {
                        t.samples
                            .last()
                            .map(|s| matches!(s.v.place, PersonPlace::Aboard { .. }))
                            .unwrap_or(false)
                    })
                    .count();
                log::info!(
                    "LAN people of player {peer}: {} heard ({aboard} riding its bus), {} drawn here",
                    up.tracks.len(),
                    up.ids.len()
                );
            }
            for (id, v) in &self.views {
                log::info!(
                    "LAN world → player {id}: {} people, {} light programs in range; ids {:016X}; {:.1} KB/s ({} KB in all)",
                    v.last_counts.0,
                    v.last_counts.1,
                    v.ids_hash,
                    0.0f32.max(v.bytes as f32 / 1024.0 / lan_seconds(lan)),
                    v.bytes / 1024
                );
            }
            let st = lan.traffic_stats();
            log::info!(
                "LAN traffic table: {} cars in the table, {} told to the players; {} datagrams, {:.1} KB/s ({} KB in all), rows {} whole / {} changes, {} deferred by the budget, {} answers",
                t_ref.map(|t| t.table.len()).unwrap_or(0),
                st.tracked,
                st.datagrams,
                self.traffic_bytes as f32 / 1024.0 / lan_seconds(lan),
                self.traffic_bytes / 1024,
                st.full_rows,
                st.delta_rows,
                st.deferred,
                st.acks_heard
            );
        }
    }
}
