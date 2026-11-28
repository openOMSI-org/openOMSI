//! Only show traffic-light countdowns when the actual simulated signal has
//! an unambiguous path and fixed program. No speculative time-to-green.

use glam::DVec3;
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::traffic::{Aspect, LaneKind, TrafficLightController};

fn predictable(ctl: &TrafficLightController, light: usize) -> Option<u32> {
    // Requests, holds and conditional jumps can change the next green time.
    if ctl.held || !ctl.stops.is_empty() {
        return None;
    }
    if !matches!(
        TrafficLightController::aspect(ctl.state(light)),
        Aspect::Red
    ) {
        return None;
    }
    let wait = ctl.time_until_go(light)?;
    (wait.is_finite() && wait > 0.0 && wait <= 300.0).then(|| wait.ceil() as u32)
}

/// A red signal directly ahead on the single unambiguous path out of the
/// bus's lane. Without the host's signal state, never invent a countdown.
pub(crate) fn green_in(traffic: &TrafficSim, bus: DVec3, heading: f64) -> Option<u32> {
    let (lane_id, s, distance) = traffic.net.nearest_lane_near(bus, LaneKind::Street)?;
    let lane = traffic.net.lanes.get(lane_id)?;
    if distance > 3.0 || lane.length() <= 0.0 {
        return None;
    }
    let path_heading = lane.at(s).1 as f64;
    let angle = (path_heading - heading + 540.0).rem_euclid(360.0) - 180.0;
    if angle.abs() > 35.0 || lane.length() - s > 80.0 {
        return None;
    }
    // On a branch the driver's selected turn is unknown. Every outgoing lane
    // must lead to the same controlled light, or this display stays hidden.
    let mut targets = lane
        .next
        .iter()
        .filter_map(|next| traffic.net.lanes.get(*next).map(|l| l.traffic_light));
    let signal = targets.next()??;
    if targets.any(|other| other != Some(signal)) {
        return None;
    }
    predictable(traffic.lights.get(signal.0)?, signal.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_sim::traffic::LightStop;

    #[test]
    fn fixed_red_light_reports_time_until_green() {
        let mut light = TrafficLightController::new(vec![vec![(0, 10.0), (6, 15.0)]], 25.0);
        light.time = 5.0;
        assert_eq!(predictable(&light, 0), Some(5));
        light.time = 12.0;
        assert_eq!(predictable(&light, 0), None);
    }

    #[test]
    fn demand_actuated_lights_do_not_guess_a_countdown() {
        let mut light = TrafficLightController::new(vec![vec![(0, 10.0), (6, 15.0)]], 25.0);
        light.stops.push(LightStop {
            light: 0,
            time: 5.0,
            if_request: false,
            jump_to: None,
        });
        assert_eq!(predictable(&light, 0), None);
        light.stops.clear();
        light.held = true;
        assert_eq!(predictable(&light, 0), None);
    }
}
