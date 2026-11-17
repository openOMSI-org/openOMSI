//! OMSI 2's accident in LAN play: the host's traffic is the simulated one, so a client's hits
//! and its call for the police go to the host, and the host tells each client about the
//! accidents it caused (`omsi_sim::ai_traffic::accident`).

use crate::traffic::Traffic;
use omsi_lan::commands::{Command, accident_command, crash_command};
use omsi_net::{LanSession, Role};

/// Once a frame: a client sends the host its hits and its call for the police; the host
/// tells each client of its accidents when anything changed.
pub(crate) fn exchange(lan: &mut LanSession, traffic: Option<&mut Traffic>) {
    let Some(t) = traffic else {
        return;
    };
    if lan.role == Role::Client {
        let (hits, police) = t.sim.take_for_host();
        for hit in hits {
            let Some(car) = car_of(hit.obstacle_id) else {
                continue;
            };
            let text = crash_command(car, hit.point, hit.push, hit.speed, hit.energy, hit.mass);
            lan.command(1, &text);
        }
        if police {
            lan.command(1, "police");
        }
    } else if lan.role == Role::Host {
        let players: Vec<u32> = lan.peers().map(|p| p.pose.id).collect();
        for player in players {
            if let Some((open, new, fled)) = t.sim.accident_note(player) {
                lan.command(player, &accident_command(open, new, fled));
            }
        }
    }
}

/// A client's mirrored car is the host's traffic id (`-2 - id` as an obstacle).
fn car_of(obstacle_id: i64) -> Option<u32> {
    u32::try_from(-2 - obstacle_id).ok()
}

/// One of the accident's commands, carried out; false for any other command.
pub(crate) fn on_command(
    traffic: Option<&mut Traffic>,
    host: bool,
    from: u32,
    command: &Command,
) -> bool {
    let t = match command {
        Command::Crash { .. } | Command::Police | Command::Accident { .. } => traffic,
        _ => return false,
    };
    let Some(t) = t else {
        return true;
    };
    match *command {
        Command::Crash {
            car,
            point,
            push,
            speed,
            energy,
            mass,
        } if host => {
            let key = omsi_net::traffic_table::TrafficId::from_raw(car)
                .and_then(|id| t.table.get(id))
                .map(|slot| slot.key);
            if let Some(key) = key {
                let hit = omsi_sim::vehicle::DynamicImpact {
                    obstacle_id: -2 - key as i64,
                    point,
                    push,
                    speed,
                    energy,
                    mass,
                };
                t.sim.player_impacts_by(vec![hit], from);
            }
        }
        Command::Police if host => {
            let n = t.sim.clear_accidents_by(from);
            log::info!("LAN: player {from} called the police: {n} vehicles back on their way");
        }
        Command::Accident { open, new, fled } if !host => t.sim.accident_from_host(open, new, fled),
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::car_of;

    #[test]
    fn a_mirrored_car_is_named_by_its_traffic_id() {
        assert_eq!(car_of(-2 - 65537), Some(65537));
        assert_eq!(car_of(-1), None);
        assert_eq!(car_of(5), None);
    }
}
