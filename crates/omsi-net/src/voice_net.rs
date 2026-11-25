//! Voice chat on the session's transport: opaque frames, relayed by the host.
//!
//! The datagram (`VOICE`, session-bound like the sound and variable ones, so it carries the
//! sender's secret):
//!
//! ```text
//! 0      MAGIC (0xBC)
//! 1      PROTOCOL
//! 2      channel: 0 nearby (heard within the range), 1 radio (the whole session)
//! 3      flags: bit 0 = the last frame of a transmission (the key was let go)
//! 4..8   the speaker's player id (u32 LE)
//! 8..10  frame number (u16 LE, counts up by one per frame)
//! 10..   the encoded audio (at most `MAX_PAYLOAD` bytes; may be empty on an end frame)
//! ```
//!
//! A client sends its frames to the host only. The host checks that the datagram comes from
//! the player it names and within the rate it may send, drops it when the session has no
//! voice, the channel is off, or the player is muted by an administrator, and passes it on
//! unchanged to the players it concerns: on the radio everybody, nearby those within the
//! range of the speaker (with a margin: the listeners work out the loudness themselves).
//! The host never decodes anything and needs no audio device, so a dedicated server relays
//! voice as it relays everything else.

use super::*;

pub const MAGIC: u8 = 0xBC;
pub const HEADER: usize = 10;
/// The longest encoded frame the relay takes (an Opus frame of speech is some 40 to 120
/// bytes; this leaves room for the highest bit rates and no more).
pub const MAX_PAYLOAD: usize = 400;
/// Frames a player may send: 50 a second are 20 ms frames, a little more is allowed for
/// jitter, and a burst of two seconds' worth for a connection that stalled and caught up.
pub(crate) const VOICE_RATE: (f32, f32) = (60.0, 120.0);
/// Received frames waiting for the game (a game that does not tick drops the oldest).
const QUEUE: usize = 256;
/// Metres added to the range when the host decides who may hear a speaker: the listeners'
/// own idea of where the speaker is (their drawn, smoothed bus) differs a little.
const RANGE_MARGIN: f64 = 12.0;

pub const NEARBY: u8 = 0;
pub const RADIO: u8 = 1;
pub const END: u8 = 1;

/// What the session allows (the host's choice; a dedicated server's `server.cfg`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoicePolicy {
    /// Voice chat at all.
    pub enabled: bool,
    /// The session-wide radio channel.
    pub radio: bool,
    /// How far a player is heard on the nearby channel (m).
    pub range: f32,
}

pub const MIN_RANGE: f32 = 5.0;
pub const MAX_RANGE: f32 = 1000.0;
pub const DEFAULT_RANGE: f32 = 75.0;

impl Default for VoicePolicy {
    fn default() -> Self {
        VoicePolicy {
            enabled: true,
            radio: true,
            range: DEFAULT_RANGE,
        }
    }
}

impl VoicePolicy {
    pub fn with_range(mut self, range: f32) -> Self {
        self.range = if range.is_finite() {
            range.clamp(MIN_RANGE, MAX_RANGE)
        } else {
            DEFAULT_RANGE
        };
        self
    }

    /// The host's answer to `voice?`: `voice <on> <radio> <range>`.
    pub fn command(&self) -> String {
        format!(
            "voice {} {} {}",
            self.enabled as u8, self.radio as u8, self.range
        )
    }

    /// The host's answer as a joining game reads it.
    pub fn parse_command(text: &str) -> Option<VoicePolicy> {
        let mut f = text.strip_prefix("voice ")?.split_whitespace();
        let flag = |s: Option<&str>| s.map(|s| s == "1");
        let enabled = flag(f.next())?;
        let radio = flag(f.next())?;
        let range = f.next()?.parse::<f32>().ok().filter(|r| r.is_finite())?;
        Some(VoicePolicy {
            enabled,
            radio,
            range: range.clamp(MIN_RANGE, MAX_RANGE),
        })
    }

    /// Whether `channel` is carried at all.
    pub fn allows(&self, channel: u8) -> bool {
        self.enabled && (channel == NEARBY || channel == RADIO && self.radio)
    }
}

/// One frame of someone's voice as the game takes it in.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceIn {
    pub from: u32,
    pub channel: u8,
    pub seq: u16,
    pub end: bool,
    pub payload: Vec<u8>,
}

/// A datagram of voice.
pub fn encode(proto: u8, channel: u8, end: bool, from: u32, seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(HEADER + payload.len());
    v.extend([MAGIC, proto, channel, end as u8 * END]);
    v.extend(from.to_le_bytes());
    v.extend(seq.to_le_bytes());
    v.extend(payload);
    v
}

/// A datagram of voice taken apart, or None for one that does not hold together (wrong
/// version, a channel or flag that does not exist, too long, no sender).
pub fn decode(data: &[u8], proto: u8) -> Option<VoiceIn> {
    if data.len() < HEADER
        || data.len() > HEADER + MAX_PAYLOAD
        || data[0] != MAGIC
        || data[1] != proto
    {
        return None;
    }
    if data[2] > RADIO || data[3] & !END != 0 {
        return None;
    }
    let from = u32::from_le_bytes(data[4..8].try_into().ok()?);
    if from == 0 {
        return None;
    }
    let end = data[3] & END != 0;
    if data.len() == HEADER && !end {
        return None;
    }
    Some(VoiceIn {
        from,
        channel: data[2],
        seq: u16::from_le_bytes(data[8..10].try_into().ok()?),
        end,
        payload: data[HEADER..].to_vec(),
    })
}

type Place = [f64; 3];

/// Where a player is for the purpose of hearing: on foot where they walk, in a bus where it
/// is; None for a player who is nowhere (a dedicated server's own place).
pub fn place_of(p: &Pose) -> Option<Place> {
    match p.walker {
        Some(w) => Some([w.x, w.y, w.z]),
        None if p.has_vehicle() => Some([p.x, p.y, p.z]),
        None => None,
    }
}

/// Who gets a frame of `channel` from the player at `from`: `others` are the other players
/// (id and place, where known). Nearby, those within the range (plus a margin) - a player
/// whose place is unknown is not reached; on the radio, all of them.
pub fn recipients(
    policy: &VoicePolicy,
    channel: u8,
    from: Option<Place>,
    others: &[(u32, Option<Place>)],
) -> Vec<u32> {
    if !policy.allows(channel) {
        return Vec::new();
    }
    let reach = policy.range as f64 + RANGE_MARGIN;
    others
        .iter()
        .filter(|(_, at)| {
            channel == RADIO
                || match (from, at) {
                    (Some(a), Some(b)) => {
                        (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>() <= reach * reach
                    }
                    _ => false,
                }
        })
        .map(|(id, _)| *id)
        .collect()
}

impl LanSession {
    /// What the session allows (the host's; a client has what the host told it).
    pub fn voice_policy(&self) -> VoicePolicy {
        self.voice_policy
    }

    pub fn set_voice_policy(&mut self, policy: VoicePolicy) {
        self.voice_policy = policy;
    }

    /// An administrator silences a player's voice (host); the mute ends with the player's
    /// leaving.
    pub fn set_voice_muted(&mut self, id: u32, muted: bool) {
        if muted {
            self.voice_muted.insert(id);
        } else {
            self.voice_muted.remove(&id);
        }
    }

    /// Host: player `id` asked for the session's voice rules, so it has voice chat on and is
    /// passed the frames (a game with it off is not, whatever it is near).
    pub fn set_voice_listener(&mut self, id: u32) {
        if self.peers.contains_key(&id) {
            self.voice_listeners.insert(id);
        }
    }

    pub fn voice_muted(&self, id: u32) -> bool {
        self.voice_muted.contains(&id)
    }

    /// Send a frame of our voice. The host passes it to those it reaches, a client sends it
    /// to the host to do so.
    pub fn voice_send(&mut self, channel: u8, end: bool, seq: u16, payload: &[u8]) {
        if !self.connected || payload.len() > MAX_PAYLOAD || channel > RADIO {
            return;
        }
        let data = encode(PROTOCOL as u8, channel, end, self.my_id, seq, payload);
        match self.role {
            Role::Host => {
                if self.voice_muted.contains(&self.my_id) {
                    return;
                }
                let at = self.own_place;
                self.relay_voice(&data, channel, self.my_id, at);
            }
            Role::Client => {
                if let Some(h) = self.host {
                    self.send(&data, h);
                }
            }
        }
    }

    /// The frames that came in since the last call, oldest first.
    pub fn take_voice(&mut self) -> Vec<VoiceIn> {
        std::mem::take(&mut self.voice_in)
    }

    pub(super) fn on_voice(&mut self, data: &[u8], from: SocketAddr) {
        let Some(v) = decode(data, PROTOCOL as u8) else {
            return;
        };
        if v.from == self.my_id {
            return;
        }
        match self.role {
            Role::Host => {
                if self
                    .checked_peer(v.from, from, VOICE_RATE, PeerTraffic::Voice)
                    .is_none()
                {
                    return;
                }
                if self.voice_muted.contains(&v.from) || !self.voice_policy.allows(v.channel) {
                    return;
                }
                let at = self.peers.get(&v.from).and_then(|p| place_of(&p.pose));
                self.relay_voice(data, v.channel, v.from, at);
                // (the host is a player too - unless it is a dedicated server, which is nowhere)
                if self.own_place.is_some()
                    && !recipients(
                        &self.voice_policy,
                        v.channel,
                        at,
                        &[(self.my_id, self.own_place)],
                    )
                    .is_empty()
                {
                    self.push_voice(v);
                }
            }
            Role::Client => self.push_voice(v),
        }
    }

    fn push_voice(&mut self, v: VoiceIn) {
        if self.voice_in.len() >= QUEUE {
            self.voice_in.remove(0);
        }
        self.voice_in.push(v);
    }

    /// Host: pass the datagram on to the players the speaker reaches (not to the speaker).
    fn relay_voice(&self, data: &[u8], channel: u8, speaker: u32, at: Option<Place>) {
        let others: Vec<(u32, Option<Place>)> = self
            .peers
            .iter()
            .filter(|(id, p)| **id != speaker && p.has_pose && self.voice_listeners.contains(id))
            .map(|(id, p)| (*id, place_of(&p.pose)))
            .collect();
        for id in recipients(&self.voice_policy, channel, at, &others) {
            if let Some(a) = self.peers.get(&id).and_then(|p| p.addr) {
                self.send(data, a);
            }
        }
        // (a host's own frame is not queued for itself here; see `voice_send`)
    }

    /// Per tick: forget the mutes of players who left, remember our own place.
    pub(super) fn voice_tick(&mut self, mine: &Pose) {
        self.own_place = place_of(mine);
        if !self.voice_listeners.is_empty() {
            let peers = &self.peers;
            self.voice_listeners.retain(|id| peers.contains_key(id));
        }
        if !self.voice_muted.is_empty() {
            let peers = &self.peers;
            let me = self.my_id;
            self.voice_muted
                .retain(|id| *id == me || peers.contains_key(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(range: f32, radio: bool) -> VoicePolicy {
        VoicePolicy {
            enabled: true,
            radio,
            range,
        }
    }

    #[test]
    fn a_datagram_round_trips_and_malformed_ones_are_refused() {
        let d = encode(15, NEARBY, false, 7, 65535, &[1, 2, 3]);
        assert_eq!(
            decode(&d, 15),
            Some(VoiceIn {
                from: 7,
                channel: 0,
                seq: 65535,
                end: false,
                payload: vec![1, 2, 3]
            })
        );
        // an end frame may carry nothing, an ordinary one may not
        assert!(decode(&encode(15, RADIO, true, 7, 1, &[]), 15).is_some());
        assert!(decode(&encode(15, RADIO, false, 7, 1, &[]), 15).is_none());
        assert!(decode(&d, 14).is_none());
        assert!(decode(&encode(15, NEARBY, false, 0, 1, &[1]), 15).is_none());
        let mut bad = d.clone();
        bad[2] = 2;
        assert!(decode(&bad, 15).is_none());
        bad = d.clone();
        bad[3] = 0x80;
        assert!(decode(&bad, 15).is_none());
        assert!(decode(&encode(15, NEARBY, false, 7, 1, &[0; MAX_PAYLOAD]), 15).is_some());
        assert!(decode(&encode(15, NEARBY, false, 7, 1, &[0; MAX_PAYLOAD + 1]), 15).is_none());
        assert!(decode(&d[..9], 15).is_none());
    }

    #[test]
    fn nearby_reaches_those_in_range_and_the_radio_everybody() {
        let others = [
            (2, Some([10.0, 0.0, 0.0])),
            (3, Some([200.0, 0.0, 0.0])),
            (4, None),
        ];
        let from = Some([0.0, 0.0, 0.0]);
        let p = policy(50.0, true);
        assert_eq!(recipients(&p, NEARBY, from, &others), vec![2]);
        assert_eq!(recipients(&p, RADIO, from, &others), vec![2, 3, 4]);
        // the margin covers a few metres of disagreement about where somebody is
        assert_eq!(
            recipients(
                &p,
                NEARBY,
                from,
                &[(5, Some([55.0, 0.0, 0.0])), (6, Some([70.0, 0.0, 0.0]))]
            ),
            vec![5]
        );
        // nobody knows where the speaker is: nobody nearby hears them
        assert!(recipients(&p, NEARBY, None, &others).is_empty());
    }

    #[test]
    fn the_session_can_switch_the_radio_or_everything_off() {
        let others = [(2, Some([1.0, 0.0, 0.0]))];
        let from = Some([0.0, 0.0, 0.0]);
        assert!(recipients(&policy(50.0, false), RADIO, from, &others).is_empty());
        assert_eq!(
            recipients(&policy(50.0, false), NEARBY, from, &others),
            vec![2]
        );
        let off = VoicePolicy {
            enabled: false,
            ..policy(50.0, true)
        };
        assert!(recipients(&off, NEARBY, from, &others).is_empty());
        assert!(recipients(&off, RADIO, from, &others).is_empty());
    }

    #[test]
    fn the_policy_travels_as_a_command() {
        let p = VoicePolicy {
            enabled: true,
            radio: false,
            range: 120.0,
        };
        assert_eq!(VoicePolicy::parse_command(&p.command()), Some(p));
        assert_eq!(
            VoicePolicy::parse_command("voice 1 1 99999"),
            Some(VoicePolicy {
                enabled: true,
                radio: true,
                range: MAX_RANGE
            })
        );
        assert_eq!(VoicePolicy::parse_command("voice 1 1 nan"), None);
        assert_eq!(VoicePolicy::parse_command("voice x"), None);
        assert_eq!(VoicePolicy::parse_command("trigger x"), None);
        assert_eq!(
            VoicePolicy::default().with_range(f32::NAN).range,
            DEFAULT_RANGE
        );
        assert_eq!(VoicePolicy::default().with_range(1.0).range, MIN_RANGE);
    }
}
