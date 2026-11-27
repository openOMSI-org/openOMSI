//! Optional quiet conversations between seated passengers, using installed OMSI voice packs.
//! No BCS integration, no new recordings, and no changes to passenger tasks or tickets.
//! Inspired by Mayconrib808/OpenOmsi-BBS (MIT) and adapted for the current PeopleSim.
use super::*;

const PAUSE: f64 = 42.0;
const REPLY_AFTER: f64 = 2.4;
const NEAR: f64 = 3.5;
const MAX_RIDERS: usize = 64;

fn can_converse(a_bus: BusId, b_bus: BusId, a: DVec3, b: DVec3) -> bool {
    a_bus == b_bus && a.is_finite() && b.is_finite() && (a - b).length_squared() <= NEAR * NEAR
}

fn is_seated_local(p: &Person, bus: BusId) -> bool {
    !p.remote
        && p.puppet.is_none()
        && matches!(&p.state, State::Pax(x) if x.inside == Some(bus) && x.task == Task::SittingInBus)
}

fn chat_due(now: f64, last: f64, last_voice: f64) -> bool {
    now.is_finite() && now >= 0.0 && now - last >= PAUSE && now - last_voice >= CHAT_PAUSE
}

impl PeopleSim {
    fn ambient_voice(&mut self, i: usize, response: bool) -> bool {
        let samples: &[&str] = if response {
            &[
                "Chat_2",
                "SmallTalk_2",
                "Talk_2",
                "Hello_2",
                "GoodMorning_1",
                "Hello_1",
            ]
        } else {
            &[
                "Chat_1",
                "SmallTalk_1",
                "Talk_1",
                "Hello_1",
                "GoodEvening_1",
            ]
        };
        for name in samples {
            let Some(path) = self.voice_file(i, name) else {
                continue;
            };
            if self
                .voice_said
                .get(&path)
                .is_some_and(|t| self.time >= *t && self.time - *t < CHAT_PAUSE)
            {
                continue;
            }
            let p = &self.people[i];
            self.voice_lines.push(VoiceLine {
                position: p.position + DVec3::new(0.0, 0.0, 1.6),
                speaker: p.id,
                path: path.clone(),
            });
            self.voice_said.insert(path, self.time);
            self.last_chat = self.time;
            return true;
        }
        false
    }

    /// Sparse, local audio only: never play a duplicate on LAN mirrors or servers.
    /// At most 64 seated passengers are considered every 42 simulated seconds.
    pub(crate) fn tick_ambient_talk(&mut self) {
        if !self.ambient_talk || self.mirror || self.players_only || self.voices != 0 {
            self.ambient_talk_reply = None;
            return;
        }
        if let Some((id, when)) = self.ambient_talk_reply {
            if self.time < when {
                return;
            }
            self.ambient_talk_reply = None;
            if let Some(i) = self.people.iter().position(|p| {
                p.id == id && matches!(p.place, Place::Bus(bus, _) if is_seated_local(p, bus))
            }) {
                self.ambient_voice(i, true);
            }
            return;
        }
        if !chat_due(self.time, self.ambient_talk_at, self.last_chat) {
            return;
        }
        self.ambient_talk_at = self.time;
        let seated: Vec<(usize, BusId)> = self
            .people
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.place {
                Place::Bus(bus, _) if is_seated_local(p, bus) => Some((i, bus)),
                _ => None,
            })
            .take(MAX_RIDERS)
            .collect();
        if seated.len() < 2 {
            return;
        }
        // Cosmetic selection does not advance the RNG for passenger destinations.
        let slot = (self.time / PAUSE).floor() as usize;
        let (speaker, bus) =
            seated[slot.wrapping_mul(17).wrapping_add(seated.len()) % seated.len()];
        let nearby: Vec<usize> = seated
            .iter()
            .filter_map(|&(i, other)| {
                (i != speaker
                    && can_converse(
                        bus,
                        other,
                        self.people[speaker].position,
                        self.people[i].position,
                    ))
                .then_some(i)
            })
            .collect();
        if nearby.is_empty() {
            return;
        }
        let listener = nearby[slot.wrapping_mul(13) % nearby.len()];
        if self.ambient_voice(speaker, false) {
            self.ambient_talk_reply = Some((self.people[listener].id, self.time + REPLY_AFTER));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limits_speech_and_rejects_invalid_time() {
        assert!(!chat_due(0.0, 0.0, -1e9));
        assert!(!chat_due(50.0, 10.0, -1e9));
        assert!(!chat_due(50.0, 0.0, 45.0));
        assert!(chat_due(50.0, 0.0, 20.0));
        assert!(!chat_due(f64::NAN, 0.0, 0.0));
    }

    #[test]
    fn requires_close_passengers_on_the_same_bus() {
        let p = DVec3::new(12.0, 0.0, 1.0);
        assert!(can_converse(
            BusId::Player,
            BusId::Player,
            p,
            DVec3::new(14.0, 0.0, 1.0)
        ));
        assert!(!can_converse(BusId::Ai(3), BusId::Ai(4), p, p));
        assert!(!can_converse(BusId::Player, BusId::Ai(3), p, p));
        assert!(!can_converse(
            BusId::Player,
            BusId::Player,
            p,
            DVec3::new(40.0, 0.0, 1.0)
        ));
        assert!(!can_converse(
            BusId::Player,
            BusId::Player,
            p,
            DVec3::new(f64::NAN, 0.0, 1.0)
        ));
    }
}
