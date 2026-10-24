//! Budgeted headset mirror refresh. Newly visible mirrors get a current image;
//! hidden mirrors share one background update per second in total.

#[derive(Default)]
pub(crate) struct Schedule {
    bus: Option<u64>,
    visible: Vec<usize>,
    count: usize,
    turn: usize,
    hidden_turn: usize,
    background: f32,
}

impl Schedule {
    pub(crate) fn select(
        &mut self,
        bus: u64,
        count: usize,
        visible: &[usize],
        updates: usize,
        dt: f32,
        enabled: bool,
    ) -> Vec<usize> {
        let reset = self.bus != Some(bus) || self.count != count;
        let mut due: Vec<usize> = if reset {
            (0..count).collect()
        } else {
            visible
                .iter()
                .copied()
                .filter(|i| *i < count && !self.visible.contains(i))
                .collect()
        };
        self.bus = Some(bus);
        self.count = count;
        self.visible = visible.iter().copied().filter(|i| *i < count).collect();
        // Initial images and newly visible mirrors take priority over regular
        // turns; they also count against this frame's available redraw budget.
        if enabled {
            self.background = (self.background + dt.clamp(0.0, 0.25)).min(1.0);
            if self.background >= 1.0 && due.len() < updates && count > self.visible.len() {
                for _ in 0..count {
                    let i = self.hidden_turn % count;
                    self.hidden_turn = self.hidden_turn.wrapping_add(1);
                    if !self.visible.contains(&i) {
                        if !due.contains(&i) {
                            due.push(i);
                        }
                        self.background = 0.0;
                        break;
                    }
                }
            }
            for _ in 0..self.visible.len() {
                if due.len() >= updates.min(count) {
                    break;
                }
                let i = self.visible[self.turn % self.visible.len()];
                self.turn = self.turn.wrapping_add(1);
                if !due.contains(&i) {
                    due.push(i);
                }
            }
        } else if !reset {
            // A zero refresh rate explicitly freezes existing images.
            due.clear();
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_all_then_prioritizes_newly_visible_mirrors() {
        let mut s = Schedule::default();
        assert_eq!(s.select(1, 4, &[0], 0, 0.0, true), vec![0, 1, 2, 3]);
        assert!(s.select(1, 4, &[0], 0, 0.0, true).is_empty());
        assert_eq!(s.select(1, 4, &[0, 3], 0, 0.0, true), vec![3]);
        assert_eq!(s.select(2, 4, &[0], 0, 0.0, true), vec![0, 1, 2, 3]);
    }

    #[test]
    fn hidden_mirrors_share_one_background_turn_per_second() {
        let mut s = Schedule::default();
        s.select(1, 3, &[0], 0, 0.0, true);
        for _ in 0..3 {
            assert!(s.select(1, 3, &[0], 0, 0.25, true).is_empty());
        }
        assert_eq!(s.select(1, 3, &[0], 1, 0.25, true), vec![1]);
        for _ in 0..3 {
            assert!(s.select(1, 3, &[0], 0, 0.25, true).is_empty());
        }
        assert_eq!(s.select(1, 3, &[0], 1, 0.25, true), vec![2]);
    }

    #[test]
    fn visible_turns_are_bounded_and_zero_rate_freezes() {
        let mut s = Schedule::default();
        s.select(1, 3, &[0, 2], 0, 0.0, true);
        let a = s.select(1, 3, &[0, 2], 1, 0.0, true);
        let b = s.select(1, 3, &[0, 2], 1, 0.0, true);
        assert_eq!((a, b), (vec![0], vec![2]));
        assert!(s.select(1, 3, &[1], 3, 1.0, false).is_empty());
        assert_eq!(s.select(1, 0, &[], 3, 1.0, true), Vec::<usize>::new());
    }
}
