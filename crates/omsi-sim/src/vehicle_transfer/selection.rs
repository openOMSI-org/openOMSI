//! Stable vehicle identities and the nearest visible-body selection policy.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Ai(u64),
    Placed(u64),
}

pub fn nearest(
    hits: impl IntoIterator<Item = (Option<Target>, f64, bool)>,
) -> Option<(Target, f64)> {
    let (target, distance, driveable) = hits.into_iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
    match (target, driveable) {
        (Some(target), true) => Some((target, distance)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_original_bus_can_be_selected_again() {
        assert_eq!(
            nearest([
                (Some(Target::Ai(7)), 30.0, true),
                (Some(Target::Placed(7)), 10.0, true)
            ]),
            Some((Target::Placed(7), 10.0))
        );
    }

    #[test]
    fn ai_and_owned_bus_ids_have_separate_namespaces() {
        assert_ne!(Target::Ai(7), Target::Placed(7));
        assert_eq!(
            nearest([
                (Some(Target::Placed(7)), 30.0, true),
                (Some(Target::Ai(7)), 10.0, true)
            ]),
            Some((Target::Ai(7), 10.0))
        );
    }

    #[test]
    fn current_and_undriveable_vehicles_block_selection() {
        for blocker in [None, Some(Target::Ai(2)), Some(Target::Placed(2))] {
            assert_eq!(
                nearest([(Some(Target::Ai(1)), 30.0, true), (blocker, 10.0, false)]),
                None
            );
        }
        assert_eq!(nearest([]), None);
    }
}
