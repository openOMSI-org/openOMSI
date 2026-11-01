//! Session-local identities of buses handed to AI, independent of timetable ownership.

#[derive(Default)]
pub struct Tracker {
    buses: Vec<(u64, String)>,
}

impl Tracker {
    pub fn record(&mut self, id: u64, name: String) {
        if let Some(bus) = self.buses.iter_mut().find(|bus| bus.0 == id) {
            bus.1 = name;
        } else {
            self.buses.push((id, name));
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = (u64, &str)> {
        self.buses.iter().map(|(id, name)| (*id, name.as_str()))
    }

    pub fn contains(&self, id: u64) -> bool {
        self.buses.iter().any(|bus| bus.0 == id)
    }

    pub fn remove(&mut self, id: u64) {
        self.buses.retain(|bus| bus.0 != id);
    }

    pub fn clear(&mut self) {
        self.buses.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.buses.is_empty()
    }
}

pub fn locate<T>(id: u64, vehicles: impl IntoIterator<Item = (u64, T)>) -> Option<T> {
    vehicles
        .into_iter()
        .find_map(|(key, vehicle)| (key == id).then_some(vehicle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_handover_updates_one_identity() {
        let mut buses = Tracker::default();
        buses.record(7, "First".into());
        buses.record(7, "Updated".into());
        assert_eq!(buses.entries().collect::<Vec<_>>(), vec![(7, "Updated")]);
    }

    #[test]
    fn taking_one_bus_keeps_the_others() {
        let mut buses = Tracker::default();
        buses.record(7, "A".into());
        buses.record(9, "B".into());
        buses.remove(7);
        assert!(!buses.contains(7));
        assert!(buses.contains(9));
        buses.clear();
        assert!(buses.is_empty());
    }

    #[test]
    fn lookup_uses_stable_ids_not_traffic_indices() {
        assert_eq!(locate(7, [(9, "B"), (7, "A")]), Some("A"));
        assert_eq!(locate(7, [(7, "A"), (9, "B")]), Some("A"));
    }

    #[test]
    fn disappeared_bus_cannot_resolve_to_a_different_bus() {
        assert_eq!(locate(7, [(9, "B")]), None);
        assert_eq!(locate::<&str>(7, []), None);
    }
}
