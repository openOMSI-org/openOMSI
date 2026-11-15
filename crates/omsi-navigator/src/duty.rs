//! Convert the player's duty into upcoming and completed navigator stops.

use super::*;

/// The duty as the navigator shows it: line, terminus, the stops from the next one on,
/// and the current trip (a key that changes with it, and its name).
#[allow(clippy::type_complexity)]
pub fn duty_parts(duty: Option<&omsi_sim::timetable_run::PlayerDuty>) -> (Option<String>, Option<String>, Vec<NavStop>, Option<(String, String)>) {
    let Some(d) = duty else { return (None, None, Vec::new(), None) };
    let Some(trip) = d.trips.get(d.trip_index) else { return (None, None, Vec::new(), None) };
    let line = if trip.line.trim().is_empty() { d.line.trim() } else { trip.line.trim() };
    let stops = trip
        .stops
        .iter()
        .skip(d.next_stop)
        .filter(|s| s.stops)
        .map(|s| NavStop { object_id: s.object_id, position: s.position.unwrap_or(DVec3::ZERO), name: s.name.clone(), arrival: s.arr, closed: s.closed.clone() })
        .collect();
    (Some(line.to_string()), Some(trip.terminus.clone()), stops, Some((format!("{}/{}", d.trip_index, trip.name), trip.name.clone())))
}

impl Navigator {
    /// Stops already passed on the current duty trip, kept separately from guidance stops.
    pub fn set_completed_stops(&mut self, duty: Option<&omsi_sim::timetable_run::PlayerDuty>) {
        self.completed_stops.clear();
        self.trip_completed = duty.is_some_and(|d| d.trip_done());
        let Some(d) = duty else { return };
        let Some(trip) = d.trips.get(d.trip_index) else { return };
        self.completed_stops = trip.stops.iter().enumerate()
            .filter(|(i, s)| s.stops && (*i < d.next_stop || self.trip_completed))
            .map(|(_, s)| NavStop { object_id: s.object_id, position: s.position.unwrap_or(DVec3::ZERO), name: s.name.clone(), arrival: s.arr, closed: s.closed.clone() })
            .collect();
    }
}
