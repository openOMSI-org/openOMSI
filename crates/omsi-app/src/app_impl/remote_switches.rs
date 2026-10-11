//! Working the switches of another player's bus in multiplayer.
//!
//! Another player's bus is drawn here from the states its own game sends: its scripts do not
//! run in our game, so a click on one of its buttons cannot do anything *here*. What the
//! click can do is tell the game that owns the bus which button was pressed - the same
//! `CMD` channel the host's administration uses - and that game works the switch as if its
//! player had clicked it. The result (lamps, doors, displays, switch positions) comes back
//! with the bus's next states like everything else the bus does.
//!
//! The words of the channel (one `CMD` text each, at most `MAX_CHAT` long):
//!
//! ```text
//! press <event>              the button went down (`Player::press_event`)
//! drag <event> <dx> <dy>     the mouse moved with it held (every ~120 ms, also 0 0 to say
//!                            that it is still held)
//! release <event>            the button came up (`Player::release_event`)
//! trigger <event>            older games: a plain trigger, nothing held
//! ```
//!
//! The owner's game takes these only
//! * from a player who is out of the seat and at the bus (aboard it, or within a hand's
//!   reach of its body - the same reach the owner has on foot), and
//! * for a name that is a `[mouseevent]` of that very bus: never an arbitrary script trigger.
//!
//! A button that was pressed is let go again when the other player stops saying that it is
//! held (a lost `release`, a game that was closed) or walks away from the bus.

use super::*;
use glam::DVec3;
use std::time::{Duration, Instant};

/// How long a switch stays held without the other player saying so.
const HOLD_TIMEOUT: Duration = Duration::from_secs(4);
/// The shortest time between two `drag` texts of a held switch (the channel takes ten
/// messages a second).
const DRAG_EVERY: Duration = Duration::from_millis(120);
/// A held switch that does not move says so this often.
const KEEPALIVE: Duration = Duration::from_millis(500);
/// The most a single `drag` text moves (logical pixels), whatever it says.
const MAX_DRAG: f32 = 200.0;

/// A switch of our bus that another player holds down.
pub(crate) struct RemoteHold {
    from: u32,
    ev: String,
    /// What `Player::press_event` answered (the script reacts to the press itself).
    plain: bool,
    /// The mouse's way while it was held.
    moved: f32,
    last: Instant,
}

/// A switch of another player's bus that we hold down.
pub(crate) struct HeldRemote {
    owner: u32,
    ev: String,
    /// The cursor as it was when the last `drag` was worked out.
    cursor: (f32, f32),
    /// Movement not yet sent (logical pixels).
    pending: (f32, f32),
    sent: Instant,
}

/// Is a person at `eye` within a hand's reach of the vehicle (any of its sections)?
fn bus_in_reach(eye: DVec3, v: &omsi_sim::VehicleInstance) -> bool {
    std::iter::once((v.position, v.heading, v.ty.def.bounding_box))
        .chain(v.trailers.iter().map(|t| (t.position, t.heading, t.ty.def.bounding_box)))
        .any(|(at, heading, bb)| super::mouse::part_in_reach(eye, at, heading, bb))
}

/// A switch name that can travel in a `CMD` text.
fn sendable(ev: &str) -> bool {
    !ev.is_empty() && ev.len() <= 64 && !ev.chars().any(|c| c.is_whitespace() || c == '|' || c.is_control())
}

/// `press`, `drag`, `release` or `trigger`, the switch and the numbers after it - or None
/// for a text that is none of them.
fn parse(text: &str) -> Option<(&str, &str, Vec<f32>)> {
    let mut words = text.split_whitespace();
    let verb = words.next()?;
    if !matches!(verb, "press" | "drag" | "release" | "trigger") {
        return None;
    }
    let ev = words.next()?;
    let nums: Vec<f32> = words.map(|w| w.parse::<f32>()).collect::<Result<_, _>>().ok()?;
    Some((verb, ev, nums))
}

impl App {
    /// The text of a `CMD` was one for a switch of our bus (see the module's words)?
    pub(crate) fn lan_switch_command(&mut self, from: u32, text: &str) -> bool {
        let Some((verb, ev, nums)) = parse(text) else { return false };
        if !self.settings.bus_control {
            log::info!("LAN: player {from} asked for switch {ev} of our bus: not allowed by the setting 'Other players may work my bus'");
            return true;
        }
        if !self.peer_may_operate(from) {
            log::info!("LAN: player {from} asked for switch {ev} of our bus from out of its reach: ignored");
            return true;
        }
        let Some(p) = self.player.as_mut() else { return true };
        if !p.has_mouse_event(ev) {
            log::info!("LAN: player {from} asked for {ev}, which is no switch of our bus: ignored");
            return true;
        }
        let holds = &mut self.net.remote_holds;
        match verb {
            "trigger" => {
                log::info!("LAN: player {from} works {ev} in our bus");
                p.vehicle.trigger(ev);
            }
            "press" => {
                // (a release that never came)
                if let Some(i) = holds.iter().position(|h| h.from == from) {
                    let h = holds.remove(i);
                    p.release_event(&h.ev, h.plain, h.moved);
                }
                log::info!("LAN: player {from} presses {ev} in our bus");
                let plain = p.press_event(ev);
                holds.push(RemoteHold { from, ev: ev.to_string(), plain, moved: 0.0, last: Instant::now() });
            }
            "drag" => {
                if let Some(h) = holds.iter_mut().find(|h| h.from == from && h.ev.eq_ignore_ascii_case(ev)) {
                    let dx = nums.first().copied().filter(|v| v.is_finite()).unwrap_or(0.0).clamp(-MAX_DRAG, MAX_DRAG);
                    let dy = nums.get(1).copied().filter(|v| v.is_finite()).unwrap_or(0.0).clamp(-MAX_DRAG, MAX_DRAG);
                    h.moved += dx.abs() + dy.abs();
                    h.last = Instant::now();
                    let name = h.ev.clone();
                    p.drag_event(&name, dx, dy);
                }
            }
            "release" => {
                if let Some(i) = holds.iter().position(|h| h.from == from && h.ev.eq_ignore_ascii_case(ev)) {
                    let h = holds.remove(i);
                    log::info!("LAN: player {from} lets go of {} in our bus", h.ev);
                    p.release_event(&h.ev, h.plain, h.moved);
                }
            }
            _ => {}
        }
        true
    }

    /// May player `from` work our bus's switches now: out of the seat, and aboard our bus
    /// or within reach of it?
    fn peer_may_operate(&self, from: u32) -> bool {
        let Some(lan) = self.net.lan.as_ref() else { return false };
        let Some(w) = lan.peers().find(|p| p.pose.id == from).and_then(|p| p.pose.walker) else {
            return false;
        };
        if w.aboard.is_some_and(|a| a.owner == lan.my_id) {
            return true;
        }
        let Some(p) = self.player.as_ref() else { return false };
        bus_in_reach(DVec3::new(w.x, w.y, w.z + 1.5), &p.vehicle)
    }

    /// Every frame: the switches others hold in our bus are let go of when they stopped
    /// saying so or walked away; and the switch we hold in another's bus is told about.
    pub(crate) fn tick_remote_switches(&mut self) {
        if self.net.remote_holds.is_empty() && self.net.remote_held.is_none() {
            return;
        }
        let gone: Vec<usize> = (0..self.net.remote_holds.len())
            .filter(|&i| {
                let h = &self.net.remote_holds[i];
                h.last.elapsed() > HOLD_TIMEOUT || !self.peer_may_operate(h.from)
            })
            .collect();
        for i in gone.into_iter().rev() {
            let h = self.net.remote_holds.remove(i);
            log::info!("LAN: player {} no longer holds {} in our bus", h.from, h.ev);
            if let Some(p) = self.player.as_mut() {
                p.release_event(&h.ev, h.plain, h.moved);
            }
        }
        self.remote_drag_frame();
    }

    /// The left button went down on a switch of another player's bus within reach (a
    /// passenger in it, or standing by it): tell its game. True when the click was one.
    pub(crate) fn remote_press(&mut self) -> bool {
        if self.view != "foot" || self.net.lan.is_none() {
            return false;
        }
        let Some((o, d, spread)) = self.cursor_ray_now() else { return false };
        let Some(eye) = self.camera.as_ref().map(|c| c.position) else { return false };
        // our own bus's switch under the pointer is ours to work (it has the click)
        if self.player.as_ref().is_some_and(|p| p.pick(o, d, spread).is_some() || p.pick_trailer(o, d, spread).is_some()) {
            return false;
        }
        let mut best: Option<(f64, u32, String)> = None;
        for (&id, rv) in self.net.remotes.remotes.iter() {
            // (a bus we only show a stand-in of has none of the switches it really has)
            if rv.stand_in {
                continue;
            }
            let v = rv.vehicle();
            if !bus_in_reach(eye, v) {
                continue;
            }
            if let Some(ev) = crate::player::pick_event_in(v, o, d, spread) {
                let dist = (v.position - eye).length();
                if best.as_ref().is_none_or(|b| dist < b.0) {
                    best = Some((dist, id, ev));
                }
            }
        }
        let Some((_, owner, ev)) = best else {
            log::info!("LAN: click on foot: no switch of another player's bus under the pointer (in reach)");
            return false;
        };
        if !sendable(&ev) {
            return false;
        }
        log::info!("LAN: pressing switch {ev} of player {owner}'s bus");
        if let Some(lan) = self.net.lan.as_mut() {
            lan.command(owner, &format!("press {ev}"));
        }
        self.net.remote_held = Some(HeldRemote {
            owner,
            ev,
            cursor: self.input.cursor,
            pending: (0.0, 0.0),
            sent: Instant::now(),
        });
        true
    }

    /// The left button came up: let go of the switch we held in another player's bus.
    pub(crate) fn remote_release(&mut self) -> bool {
        let Some(h) = self.net.remote_held.take() else { return false };
        if let Some(lan) = self.net.lan.as_mut() {
            // (what moved since the last text goes first, as a short adjustment must not be lost)
            if h.pending != (0.0, 0.0) {
                lan.command(h.owner, &format!("drag {} {:.1} {:.1}", h.ev, h.pending.0, h.pending.1));
            }
            lan.command(h.owner, &format!("release {}", h.ev));
        }
        true
    }

    /// While the switch is held: the cursor's way since last time, sent a few times a second
    /// (and a `0 0` now and then, so the owner knows that it is still held).
    fn remote_drag_frame(&mut self) {
        let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0).max(0.1);
        let cursor = self.input.cursor;
        let Some(h) = self.net.remote_held.as_mut() else { return };
        h.pending.0 += (cursor.0 - h.cursor.0) / scale;
        h.pending.1 += (cursor.1 - h.cursor.1) / scale;
        h.cursor = cursor;
        let moving = h.pending != (0.0, 0.0);
        let due = if moving { h.sent.elapsed() >= DRAG_EVERY } else { h.sent.elapsed() >= KEEPALIVE };
        if !due {
            return;
        }
        let (dx, dy) = std::mem::take(&mut h.pending);
        h.sent = Instant::now();
        let text = format!("drag {} {:.1} {:.1}", h.ev, dx, dy);
        let owner = h.owner;
        if let Some(lan) = self.net.lan.as_mut() {
            lan.command(owner, &text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, sendable};

    #[test]
    fn the_words_of_the_channel_are_read() {
        assert_eq!(parse("press cp_Licht"), Some(("press", "cp_Licht", vec![])));
        assert_eq!(parse("drag cp_Drehschalter -3.5 12.0"), Some(("drag", "cp_Drehschalter", vec![-3.5, 12.0])));
        assert_eq!(parse("release cp_Licht"), Some(("release", "cp_Licht", vec![])));
        assert_eq!(parse("trigger cp_Licht"), Some(("trigger", "cp_Licht", vec![])));
    }

    #[test]
    fn other_texts_and_broken_numbers_are_not_switch_commands() {
        assert_eq!(parse("voice?"), None);
        assert_eq!(parse("admin kick 2"), None);
        assert_eq!(parse("press"), None);
        assert_eq!(parse(""), None);
        // (a number that is none: the text is dropped, not worked with 0)
        assert_eq!(parse("drag cp_x a b"), None);
    }

    #[test]
    fn only_plain_switch_names_are_sent() {
        assert!(sendable("cp_Licht_Innen"));
        assert!(!sendable(""));
        assert!(!sendable("two words"));
        assert!(!sendable("a|b"));
        assert!(!sendable(&"x".repeat(65)));
    }
}
