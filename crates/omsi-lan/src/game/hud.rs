//! What the HUD shows for LAN play.

use super::LanGame;
use omsi_net::{LanSession, Role};
use std::time::{Duration, Instant};

/// The HUD's LAN lines: the session and its code, who is connected, what they drive and
/// where they are, what differs between the host's world and ours, and the chat line.
/// `tr` translates a line into the game's language.
pub fn hud_lines(lan: &LanSession, tr: &dyn Fn(&str) -> String) -> Vec<String> {
    // One line: who is there is written above their buses (`name_tags`), what they say is in
    // the chat. The long list of every player's bus, place and doors that stood here filled
    // half the screen in a session of a few players.
    let mut lines = Vec::new();
    let players = lan.peer_count();
    let others = match players {
        0 => tr("nobody else yet"),
        1 => tr("1 other player"),
        n => tr("{n} other players").replace("{n}", &n.to_string()),
    };
    match lan.role {
        Role::Host => {
            let c = lan.code();
            // (translated apart from the code, which stays as it is)
            lines.push(format!(
                "{}   {}",
                match players {
                    0 => tr("Online: nobody else yet"),
                    1 => tr("Online: 1 other player"),
                    n => tr("Online: {n} other players").replace("{n}", &n.to_string()),
                },
                c.as_ref().map(|c| c.encode()).unwrap_or_default(),
            ));
        }
        Role::Client => {
            if let Some(why) = lan.rejected.as_ref() {
                lines.push(
                    tr("Online: not connected: {reason} (chat /reconnect)")
                        .replace("{reason}", &tr(why)),
                );
            } else if lan.connected {
                let name = lan
                    .welcome
                    .as_ref()
                    .map(|w| w.host_name.clone())
                    .unwrap_or_default();
                // Substitute the name last so a player's text is never treated as a placeholder.
                lines.push(
                    tr("Online: in {host}'s game, {others}")
                        .replace("{others}", &others)
                        .replace("{host}", &name),
                );
            } else {
                lines.push(tr("Online: connecting ..."));
            }
        }
    }
    if let Some(w) = lan.warnings.first() {
        let w = match w.strip_prefix("Host's mods could not be fetched: ") {
            Some(e) => tr("Host's mods could not be fetched: {e}").replace("{e}", &tr(e)),
            None => tr(w),
        };
        lines.push(tr("Online: {warning}").replace("{warning}", &w));
    }
    // OMSI_DEBUG_LAN: what the HUD shows, every few seconds
    if omsi_cfg::flags::OMSI_DEBUG_LAN.is_set() {
        HUD_LOG.with(|t| {
            let now = Instant::now();
            if t.get()
                .map(|last| now.duration_since(last) > Duration::from_secs(4))
                .unwrap_or(true)
            {
                t.set(Some(now));
                log::info!("LAN HUD:\n    {}", lines.join("\n    "));
            }
        });
    }
    lines
}

thread_local! {
    static HUD_LOG: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// `OMSI_DEBUG_LAN`: every few seconds, what we know of every player, what their bus
/// shows and plays here, and the bytes that went over the network.
pub(super) fn debug_log(lan: &LanSession, game: &mut LanGame, dt: f32) {
    if !omsi_cfg::flags::OMSI_DEBUG_LAN.is_set() {
        return;
    }
    game.log_t -= dt;
    if game.log_t > 0.0 {
        return;
    }
    game.log_t = 4.0;
    let (sent, received) = (lan.sent(), lan.received);
    log::info!(
        "LAN traffic: {:.0} B/s out, {:.0} B/s in{}",
        (sent - game.last_sent) as f32 / 4.0,
        (received - game.last_received) as f32 / 4.0,
        game.last_gap
            .take()
            .map(|g| format!(
                "; our clock was {g:+.2} s off the host's, {:+.2} s still to catch up",
                game.slew
            ))
            .unwrap_or_default()
    );
    game.last_sent = sent;
    game.last_received = received;
    for peer in lan.peers().filter(|p| p.has_pose) {
        let q = &peer.pose;
        log::info!(
            "LAN state of player {} '{}': {} paint '{}' at ({:.2}, {:.2}, {:.3}) pitch {:.2} bank {:.2} wheels {:?} {:.1} km/h steer {:.1} flags {:010b} head {} interior {} blinker {} rpm {:.0} throttle {:.2} brake {:.2} doors {:?} passengers {} line '{}' destination '{}' {} lamps lit of {}, {} rear section(s), table {:08X}",
            q.id,
            q.name,
            q.bus,
            q.paint,
            q.x,
            q.y,
            q.z,
            q.pitch,
            q.bank,
            q.suspension
                .iter()
                .map(|s| (s * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>(),
            q.speed_kmh,
            q.steer_deg,
            q.flags,
            q.head,
            q.interior,
            q.blinker,
            q.rpm,
            q.throttle,
            q.brake,
            q.doors
                .iter()
                .map(|d| (d * 10.0).round() / 10.0)
                .collect::<Vec<_>>(),
            q.passengers,
            q.line,
            q.destination,
            q.lamps.iter().filter(|l| **l > 0.5).count(),
            q.lamps.len(),
            q.rear.len(),
            q.table
        );
        if let Some(rv) = game.remotes.get(&q.id) {
            let v = &rv.vehicle;
            // (the cockpit's own lamps left out)
            let lit: Vec<&str> = rv
                .table
                .lamps
                .iter()
                .filter(|(n, id)| {
                    !n.starts_with("cockpit")
                        && !n.starts_with("cp_")
                        && v.state.vars.get(*id as usize).copied().unwrap_or(0.0) > 0.5
                })
                .map(|(n, _)| n.as_str())
                .take(16)
                .collect();
            let steer = v.var("Axle_Steering_0_L").unwrap_or(0.0).to_degrees();
            let values: Vec<String> = rv
                .table
                .values
                .iter()
                .map(|(n, id)| {
                    format!(
                        "{n}={:.2}",
                        v.state.vars.get(*id as usize).copied().unwrap_or(0.0)
                    )
                })
                .collect();
            log::info!(
                "LAN copy of player {}: {} lamps lit {:?}; values {}; door_0 {:.2} engine_n {:.0} wheel {:.0} deg steer {:.1} deg AI_Light {:?} lights_sw_blinker {:?}",
                q.id,
                rv.table
                    .lamps
                    .iter()
                    .filter(|(_, id)| v.state.vars.get(*id as usize).copied().unwrap_or(0.0) > 0.5)
                    .count(),
                lit,
                values.join(" "),
                rv.table
                    .doors
                    .first()
                    .and_then(|id| v.state.vars.get(*id as usize))
                    .copied()
                    .unwrap_or(0.0),
                v.var("engine_n").unwrap_or(0.0),
                v.var("Wheel_Rotation_1_L")
                    .unwrap_or(0.0)
                    .to_degrees()
                    .rem_euclid(360.0),
                steer,
                v.var("AI_Light"),
                v.var("lights_sw_blinker")
            );
            // (what it plays is the view's to tell: `view_sync::lan`)
            game.drawn.debug_sounds.push(q.id);
        }
    }
}
