use omsi_net::Footprint;

const MARGIN: f64 = 1.5;

pub fn inside(f: &Footprint, local: [f32; 3]) -> bool {
    let (across, along) = (local[0] as f64, local[1] as f64);
    across.abs() <= f.width as f64 * 0.5 + MARGIN && along.abs() <= f.length as f64 + MARGIN
}

pub fn passenger_trigger(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_suffix("_off").unwrap_or(&n);
    n.starts_with("int_") || n.contains("haltewunsch") || n.contains("kinderwagenwunsch")
}

/// May player `from` work switch `ev` of our bus `bus` (`lan`: our session)? Only a passenger
/// aboard our bus, inside it, and only a switch passengers work (`passenger_trigger`).
pub fn may_work(
    lan: &omsi_net::LanSession,
    from: u32,
    bus: Option<&omsi_sim::VehicleInstance>,
    ev: &str,
) -> bool {
    let walker = lan
        .peers()
        .find(|p| p.pose.id == from)
        .and_then(|p| p.pose.walker);
    let aboard = walker
        .and_then(|w| w.aboard)
        .filter(|a| a.owner == lan.my_id);
    let inside = match (aboard, bus) {
        (Some(a), Some(v)) => inside(
            &crate::spawn::footprint_of(v, crate::spawn::BUS_BOX),
            a.local,
        ),
        _ => false,
    };
    aboard.is_some() && inside && passenger_trigger(ev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passenger_is_inside_the_bus_it_claims() {
        let bus = Footprint {
            x: 100.0,
            y: 50.0,
            z: 0.0,
            heading: 90.0,
            length: 12.0,
            width: 2.5,
        };
        assert!(inside(&bus, [0.0, 0.0, 1.0]));
        assert!(inside(&bus, [0.8, -5.5, 1.0]));
        assert!(!inside(&bus, [5.0, 0.0, 1.0]));
        assert!(!inside(&bus, [0.0, 200.0, 1.0]));
    }

    #[test]
    fn only_a_passengers_controls_are_worked_from_another_game() {
        assert!(passenger_trigger("int_haltewunsch"));
        assert!(passenger_trigger("Haltewunsch_off"));
        assert!(passenger_trigger("int_kinderwagenwunsch"));
        assert!(!passenger_trigger("bus_doorfront0"));
        assert!(!passenger_trigger("handbrake"));
        assert!(!passenger_trigger("engine_start"));
    }
}
