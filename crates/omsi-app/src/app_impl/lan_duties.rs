//! LAN play (host): how far the other players have got on their duties.

use omsi_net::{LanSession, Role};
use omsi_sim::people::PeopleSim;
use omsi_sim::timetable_run::ScheduleSim;

/// A player's game tells the host which tour it drives, not which stop it makes for: each
/// stop their bus has stood at and left goes to the timetable, so that their trip brings
/// nobody there any more (`ScheduleSim::lan_bus_left`).
pub(crate) fn lan_tour_progress(
    lan: &LanSession,
    people: &mut PeopleSim,
    timetable: &mut ScheduleSim,
) {
    if lan.role != Role::Host {
        return;
    }
    for p in lan.peers() {
        let left = people.remote_stops_left(p.pose.id);
        if let Some((line, tour)) = p.pose.tour.split_once('/') {
            for stop in left {
                timetable.lan_bus_left(line.trim(), tour.trim(), stop);
            }
        }
    }
}
