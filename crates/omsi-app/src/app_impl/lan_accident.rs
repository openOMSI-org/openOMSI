//! OMSI 2's accident in LAN play: the host's traffic is the simulated one, so a client's hits
//! and its call for the police go to the host, and the host tells each client about the
//! accidents it caused. Each message is sent again until it is answered
//! (`omsi_sim::ai_traffic::accident`).

use crate::traffic::Traffic;
use omsi_lan::commands::{Command, accident_command, crash_command};
use omsi_net::traffic_table::TrafficId;
use omsi_net::{LanSession, Role};
use omsi_sim::ai_traffic::accident::AccidentMsg;

/// Once a frame of `dt` seconds: what a client has for the host, and what the host has for
/// each client.
pub(crate) fn exchange(lan: &mut LanSession, traffic: Option<&mut Traffic>, dt: f32) {
    let Some(t) = traffic else {
        return;
    };
    if lan.role == Role::Client {
        for msg in t.sim.accident_to_host(dt) {
            if let Some(text) = text_of(msg) {
                lan.command(1, &text);
            }
        }
    } else if lan.role == Role::Host {
        let players: Vec<u32> = lan.peers().map(|p| p.pose.id).collect();
        for (player, msg) in t.sim.accident_to_players(&players, dt) {
            if let Some(text) = text_of(msg) {
                lan.command(player, &text);
            }
        }
    }
}

/// One of the accident's commands from player `from`, carried out and answered; false for
/// any other command.
pub(crate) fn on_command(
    lan: Option<&mut LanSession>,
    traffic: Option<&mut Traffic>,
    from: u32,
    command: &Command,
) -> bool {
    let Some(msg) = msg_of(command, traffic.as_deref()) else {
        return false;
    };
    let (Some(lan), Some(t)) = (lan, traffic) else {
        return true;
    };
    let answer = if lan.role == Role::Host {
        t.sim.accident_from_player(from, msg)
    } else {
        t.sim.accident_from_host(msg)
    };
    if let Some(text) = answer.and_then(text_of) {
        lan.command(from, &text);
    }
    true
}

/// A client's mirrored car is the host's traffic id (`-2 - id` as an obstacle).
fn car_of(obstacle_id: i64) -> Option<u32> {
    u32::try_from(-2 - obstacle_id).ok()
}

fn text_of(msg: AccidentMsg) -> Option<String> {
    Some(match msg {
        AccidentMsg::Hit { seq, hit } => {
            let car = car_of(hit.obstacle_id)?;
            crash_command(
                seq, car, hit.point, hit.push, hit.speed, hit.energy, hit.mass,
            )
        }
        AccidentMsg::HitSeen { seq } => format!("crash-seen {seq}"),
        AccidentMsg::Police { seq } => format!("police {seq}"),
        AccidentMsg::PoliceSeen { seq } => format!("police-seen {seq}"),
        AccidentMsg::State {
            seq,
            open,
            new,
            fled,
        } => accident_command(seq, open, new, fled),
        AccidentMsg::StateSeen { seq } => format!("accident-seen {seq}"),
    })
}

/// The message a command carries; a hit's car named as the host's traffic knows it (one
/// it no longer has names none, and is answered all the same).
fn msg_of(command: &Command, traffic: Option<&Traffic>) -> Option<AccidentMsg> {
    Some(match *command {
        Command::Crash {
            seq,
            car,
            point,
            push,
            speed,
            energy,
            mass,
        } => {
            let key = TrafficId::from_raw(car)
                .zip(traffic)
                .and_then(|(id, t)| t.table.get(id))
                .map(|slot| -2 - slot.key as i64);
            let hit = omsi_sim::vehicle::DynamicImpact {
                obstacle_id: key.unwrap_or(-1),
                point,
                push,
                speed,
                energy,
                mass,
            };
            AccidentMsg::Hit { seq, hit }
        }
        Command::CrashSeen(seq) => AccidentMsg::HitSeen { seq },
        Command::Police(seq) => AccidentMsg::Police { seq },
        Command::PoliceSeen(seq) => AccidentMsg::PoliceSeen { seq },
        Command::Accident {
            seq,
            open,
            new,
            fled,
        } => AccidentMsg::State {
            seq,
            open,
            new,
            fled,
        },
        Command::AccidentSeen(seq) => AccidentMsg::StateSeen { seq },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mirrored_car_is_named_by_its_traffic_id() {
        assert_eq!(car_of(-2 - 65537), Some(65537));
        assert_eq!(car_of(-1), None);
        assert_eq!(car_of(5), None);
    }

    /// Every message goes over the wire and comes back as itself.
    #[test]
    fn the_messages_read_back_from_their_commands() {
        let hit = omsi_sim::vehicle::DynamicImpact {
            obstacle_id: -1,
            point: glam::DVec3::new(10.5, -4.25, 2.0),
            push: glam::DVec3::new(1.0, 0.0, 0.0),
            speed: 8.0,
            energy: 60000.0,
            mass: 12000.0,
        };
        for msg in [
            AccidentMsg::HitSeen { seq: 3 },
            AccidentMsg::Police { seq: 4 },
            AccidentMsg::PoliceSeen { seq: 4 },
            AccidentMsg::State {
                seq: 9,
                open: true,
                new: 2,
                fled: 1,
            },
            AccidentMsg::StateSeen { seq: 9 },
        ] {
            let text = text_of(msg).unwrap();
            assert_eq!(
                msg_of(&Command::parse(1, &text).unwrap(), None),
                Some(msg),
                "{text}"
            );
        }
        assert_eq!(text_of(AccidentMsg::Hit { seq: 1, hit }), None, "no car");
        let named = omsi_sim::vehicle::DynamicImpact {
            obstacle_id: -2 - 65537,
            ..hit
        };
        let text = text_of(AccidentMsg::Hit { seq: 1, hit: named }).unwrap();
        // (no traffic table here: the car is one the host does not have)
        assert_eq!(
            msg_of(&Command::parse(3, &text).unwrap(), None),
            Some(AccidentMsg::Hit { seq: 1, hit })
        );
    }
}
