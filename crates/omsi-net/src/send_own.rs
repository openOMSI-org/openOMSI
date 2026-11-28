use super::*;

impl LanSession {
    pub(super) fn send_own(&mut self, dt: f32, mine: &Pose) {
        let mut p = mine.clone();
        p.id = self.my_id;
        p.name = self.my_name.clone();
        if p.bus.is_empty() {
            p.flags &= !FLAG_VEHICLE;
        }
        // (the clock is left out of the comparison: an unchanged state is still unchanged)
        p.sent_ms = 0;
        let data = wire::encode_state(&p, PROTOCOL as u8, self.seq);
        let body = &data[wire::STATE_HEADER..];
        if body == self.last_state.as_slice() {
            self.unchanged += dt;
        } else {
            self.unchanged = 0.0;
        }
        // a heartbeat without a vehicle, the full rate while anything changes, less when not
        let interval = if p.flags & FLAG_VEHICLE == 0 {
            self.heartbeat
        } else if self.unchanged > 1.0 {
            1.0 / IDLE_RATE_HZ
        } else {
            1.0 / RATE_HZ
        };
        // (the remainder is kept, so the rate holds on average whatever the frame times)
        if self.send_acc + 1.0e-4 >= interval {
            self.send_acc = (self.send_acc - interval).clamp(0.0, interval);
            self.last_state = body.to_vec();
            self.seq = self.seq.wrapping_add(1);
            // stamped with the moment of the frame it shows (see `Pose::sent_ms`)
            p.sent_ms = self.stamp_ms().max(1);
            let data = wire::encode_state(&p, PROTOCOL as u8, self.seq);
            match self.role {
                Role::Host => self.broadcast(&data, None),
                Role::Client => {
                    if let Some(h) = self.host {
                        self.send(&data, h);
                    }
                }
            }
        }
        let info = p.encode_info();
        // Sent when it changes, but no more often than INFO_MIN_GAP: a roller blind turning
        // through its numbers or a pilot screen changes the `[matl_freetex]` pictures many
        // times a second, and the host took ten messages a second and dropped the rest.
        if (info != self.last_info && self.info_acc >= INFO_MIN_GAP) || self.info_acc >= INFO_EVERY
        {
            self.info_acc = 0.0;
            match self.role {
                Role::Host => self.broadcast(info.as_bytes(), None),
                Role::Client => {
                    if let Some(h) = self.host {
                        self.send(info.as_bytes(), h);
                    }
                }
            }
            self.last_info = info;
        }
        if self.role == Role::Host && self.clock_acc >= CLOCK_EVERY {
            self.clock_acc = 0.0;
            self.broadcast(
                format!("CLOCK|{}|{}", self.world.fields(), self.clock_speed).as_bytes(),
                None,
            );
        }
        if let (Role::Client, Some(at), None, Some(h)) =
            (self.role, self.place, self.near.as_ref(), self.host)
        {
            if self.place_acc >= 1.0 {
                self.place_acc = 0.0;
                self.send(
                    format!(
                        "PLACE|{}|{:.2}|{:.2}|{:.2}|{:.1}|{:.1}|{:.1}",
                        self.my_id, at.x, at.y, at.z, at.heading, at.length, at.width
                    )
                    .as_bytes(),
                    h,
                );
            }
        }
    }
}
