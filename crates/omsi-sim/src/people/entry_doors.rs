//! Which doors of the player's bus passengers may board through (#3124).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EntryDoors {
    #[default]
    AsBus,
    First,
    FirstTwo,
    Last,
}

impl EntryDoors {
    pub fn from_setting(mode: &str) -> EntryDoors {
        let m = mode
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-', '/'], "_");
        match m.as_str() {
            "first" | "front" | "front_door" | "first_door" => EntryDoors::First,
            "first_two" | "first2" | "front_two" | "front2" | "both" => EntryDoors::FirstTwo,
            "last" | "rear" | "back" | "rear_door" | "last_door" | "back_door" => EntryDoors::Last,
            _ => EntryDoors::AsBus,
        }
    }

    pub fn writing(&self) -> &'static str {
        match self {
            EntryDoors::AsBus => "as_bus",
            EntryDoors::First => "first",
            EntryDoors::FirstTwo => "first_two",
            EntryDoors::Last => "last",
        }
    }

    pub fn allows(&self, k: usize, count: usize) -> bool {
        match self {
            EntryDoors::AsBus => true,
            EntryDoors::First => k == 0,
            EntryDoors::FirstTwo => k < 2,
            EntryDoors::Last => count > 0 && k + 1 == count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_spell_the_door_sets() {
        for as_bus in ["as_bus", "bus", "vehicle", "omsi", "", "whatever"] {
            assert_eq!(
                EntryDoors::from_setting(as_bus),
                EntryDoors::AsBus,
                "{as_bus:?}"
            );
        }
        for front in ["first", "front", "FRONT", "front door", "front_door"] {
            assert_eq!(
                EntryDoors::from_setting(front),
                EntryDoors::First,
                "{front:?}"
            );
        }
        for two in ["first_two", "first2", "front_two", "both"] {
            assert_eq!(
                EntryDoors::from_setting(two),
                EntryDoors::FirstTwo,
                "{two:?}"
            );
        }
        for rear in ["last", "rear", "back", "rear door"] {
            assert_eq!(EntryDoors::from_setting(rear), EntryDoors::Last, "{rear:?}");
        }
        assert_eq!(EntryDoors::default(), EntryDoors::AsBus);
        for d in [
            EntryDoors::AsBus,
            EntryDoors::First,
            EntryDoors::FirstTwo,
            EntryDoors::Last,
        ] {
            assert_eq!(EntryDoors::from_setting(d.writing()), d);
        }
    }

    #[test]
    fn which_entries_a_door_set_lets_through() {
        let three = |d: EntryDoors| (0..3).filter(|k| d.allows(*k, 3)).collect::<Vec<_>>();
        assert_eq!(three(EntryDoors::AsBus), vec![0, 1, 2]);
        assert_eq!(three(EntryDoors::First), vec![0]);
        assert_eq!(three(EntryDoors::FirstTwo), vec![0, 1]);
        assert_eq!(three(EntryDoors::Last), vec![2]);
        for d in [EntryDoors::First, EntryDoors::FirstTwo, EntryDoors::Last] {
            assert_eq!(
                (0..1).filter(|k| d.allows(*k, 1)).collect::<Vec<_>>(),
                vec![0],
                "{d:?}"
            );
        }
        assert_eq!(
            (0..2)
                .filter(|k| EntryDoors::Last.allows(*k, 2))
                .collect::<Vec<_>>(),
            vec![1]
        );
        for d in [EntryDoors::First, EntryDoors::FirstTwo, EntryDoors::Last] {
            assert!((0..0).filter(|k| d.allows(*k, 0)).count() == 0, "{d:?}");
        }
    }
}
