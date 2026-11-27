//! OMSI 2's accident (manual 2.11, "First Aid"), the game's part: the messages, the hazard
//! lights the police ask for, "Call the police", and in LAN play the messages between the
//! client and the host, whose traffic is the one simulated (`omsi_sim::ai_traffic::accident`).

use crate::traffic::Traffic;
use omsi_net::traffic_table::TrafficId;
use omsi_net::{LanSession, Role};
use omsi_sim::ai_traffic::accident::{AccidentMsg, police_may_be_called};

impl crate::App {
    /// Once a frame: the hazard lights, the accidents to tell of (the personnel file's third
    /// `[crashs]` count is leaving the scene) and whether the menu offers the police.
    pub(crate) fn accident_frame(&mut self, dt: f32) {
        if let Some(p) = self.player.as_ref() {
            let lit = |n: &str| p.vehicle.var(n).is_some_and(|v| v > 0.5);
            let both = lit("lights_blinker_l") && lit("lights_blinker_r");
            self.menus.game_panels.hazard_watch.step(dt, both);
        }
        let Some(t) = self.session.traffic.as_mut() else {
            self.menus.game_panels.accident_open = false;
            return;
        };
        let (new, fled) = (t.take_accidents(), t.take_fled());
        self.menus.game_panels.accident_open = t.accident_open();
        let msg = if fled > 0 {
            self.session.career.crashes[2] += fled as i32;
            "You left the scene of the accident without calling the police"
        } else if new > 0 {
            "Accident! Stop, switch on the hazard lights and call the police"
        } else {
            return;
        };
        self.service_msg = Some((msg.into(), 8.0));
    }

    pub(crate) fn call_police(&mut self) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let Some(t) = self.session.traffic.as_mut() else {
            return;
        };
        let watch = &self.menus.game_panels.hazard_watch;
        let switch = p.vehicle.var("lights_sw_warnblinker");
        let msg = if !t.accident_open() {
            "No accident to report"
        } else if !police_may_be_called(switch, p.vehicle.physics.velocity_kmh(), watch) {
            "Stop and switch on the hazard lights first"
        } else {
            let n = t.clear_accidents();
            self.menus.game_panels.accident_open = false;
            log::info!("the police took up the accident: {n} vehicles back on their way");
            "The police have taken up the accident"
        };
        self.service_msg = Some((msg.into(), 6.0));
    }
}

/// LAN, once a frame of `dt` seconds: what a client has for the host, and what the host has
/// for each client.
pub(crate) fn exchange(lan: &mut LanSession, traffic: Option<&mut Traffic>, dt: f32) {
    let Some(t) = traffic else {
        return;
    };
    let out: Vec<(u32, AccidentMsg)> = match lan.role {
        Role::Client => t
            .sim
            .accident_to_host(dt)
            .into_iter()
            .map(|m| (1, m))
            .collect(),
        Role::Host => {
            let players: Vec<u32> = lan.peers().map(|p| p.pose.id).collect();
            t.sim.accident_to_players(&players, dt)
        }
    };
    for (to, msg) in out {
        if let Some(text) = msg.command() {
            lan.command(to, &text);
        }
    }
}

/// LAN: a command of player `from`'s that is one of the accident's, carried out and
/// answered; false for any other.
pub(crate) fn on_command(
    lan: Option<&mut LanSession>,
    traffic: Option<&mut Traffic>,
    from: u32,
    text: &str,
) -> bool {
    let Some(mut msg) = AccidentMsg::parse(text, from == 1) else {
        return false;
    };
    let (Some(lan), Some(t)) = (lan, traffic) else {
        return true;
    };
    let answer = if lan.role == Role::Host {
        if let AccidentMsg::Hit { hit, .. } = &mut msg {
            // the client names the car by its traffic id; one the host no longer has is none
            let key = u32::try_from(-2 - hit.obstacle_id)
                .ok()
                .and_then(TrafficId::from_raw)
                .and_then(|id| t.table.get(id));
            hit.obstacle_id = key.map_or(-1, |slot| -2 - slot.key as i64);
        }
        t.sim.accident_from_player(from, msg)
    } else {
        t.sim.accident_from_host(msg)
    };
    if let Some(text) = answer.and_then(|a| a.command()) {
        lan.command(from, &text);
    }
    true
}
