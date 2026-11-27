//! Order-preserving broad-phase searches over a short-lived set of world positions.
//! The authoritative simulation and the graphical game share this lightweight
//! utility. It changes candidate discovery, never final distance or ownership rules.
use glam::DVec2;
use hashbrown::HashMap;

const CELL_SIZE: f64 = 256.0;
const GRID_MIN: usize = 96;
const MAX_QUERY_CELLS: i64 = 256;

/// A snapshot of the original list's XY positions, indexed by positions
/// within this snapshot. Rebuild after any car list mutation.
#[derive(Default)]
pub struct SpatialIndex {
    points: Vec<DVec2>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    indexed: bool,
}

fn cell(value: f64) -> i64 {
    (value / CELL_SIZE).floor() as i64
}

impl SpatialIndex {
    pub fn from_positions(points: impl IntoIterator<Item = DVec2>) -> Self {
        let points: Vec<DVec2> = points.into_iter().collect();
        let indexed = points.len() >= GRID_MIN;
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        if indexed {
            for (i, p) in points.iter().enumerate() {
                if p.is_finite() {
                    cells.entry((cell(p.x), cell(p.y))).or_default().push(i);
                }
            }
        }
        Self {
            points,
            cells,
            indexed,
        }
    }

    /// Return indices in the same list order as an exact linear proximity
    /// scan. The supplied output is reused across players, avoiding repeated
    /// allocations. Non-finite positions cannot match.
    pub fn within(&self, center: DVec2, radius: f64, out: &mut Vec<usize>) {
        out.clear();
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
            return;
        }
        let min_x = cell(center.x - radius);
        let max_x = cell(center.x + radius);
        let min_y = cell(center.y - radius);
        let max_y = cell(center.y + radius);
        let cols = max_x.saturating_sub(min_x).saturating_add(1);
        let rows = max_y.saturating_sub(min_y).saturating_add(1);
        let use_grid =
            self.indexed && cols > 0 && rows > 0 && cols.saturating_mul(rows) <= MAX_QUERY_CELLS;
        let r2 = radius * radius;
        if use_grid {
            for x in min_x..=max_x {
                for y in min_y..=max_y {
                    if let Some(list) = self.cells.get(&(x, y)) {
                        for &i in list {
                            if (self.points[i] - center).length_squared() <= r2 {
                                out.push(i);
                            }
                        }
                    }
                }
            }
            // Preserve the original car priority and MAX_CARS_PER_PEER order.
            out.sort_unstable();
        } else {
            for (i, point) in self.points.iter().enumerate() {
                if point.is_finite() && (*point - center).length_squared() <= r2 {
                    out.push(i);
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(points: &[DVec2], at: DVec2, r: f64) -> Vec<usize> {
        points
            .iter()
            .enumerate()
            .filter_map(|(i, p)| (p.is_finite() && (*p - at).length() <= r).then_some(i))
            .collect()
    }

    #[test]
    fn grid_matches_linear_selection_and_original_priority() {
        let mut points: Vec<_> = (0..512)
            .map(|i| {
                DVec2::new(
                    (i as f64 * 19.41).sin() * 1900.0,
                    (i as f64 * 3.37).cos() * 2200.0,
                )
            })
            .collect();
        points[0] = DVec2::new(256.0, -256.0);
        points[1] = DVec2::new(-256.0, -256.0);
        points[2] = DVec2::new(f64::NAN, 0.0);
        let index = SpatialIndex::from_positions(points.iter().copied());
        assert_eq!(index.len(), points.len());
        let mut got = Vec::new();
        for x in -13..=13 {
            for y in -13..=13 {
                let at = DVec2::new(x as f64 * 170.0, y as f64 * 170.0);
                for r in [0.0, 24.0, 260.0, 650.0, 5000.0] {
                    index.within(at, r, &mut got);
                    assert_eq!(got, reference(&points, at, r), "at {at:?} radius {r}");
                }
            }
        }
    }

    #[test]
    fn small_sets_invalid_inputs_and_reused_buffers_are_safe() {
        let index = SpatialIndex::from_positions([
            DVec2::ZERO,
            DVec2::new(2.0, 0.0),
            DVec2::new(f64::INFINITY, 1.0),
        ]);
        let mut out = vec![7, 8, 9];
        index.within(DVec2::ZERO, 2.0, &mut out);
        assert_eq!(out, vec![0, 1]);
        index.within(DVec2::new(f64::NAN, 0.0), 2.0, &mut out);
        assert!(out.is_empty());
        index.within(DVec2::ZERO, -1.0, &mut out);
        assert!(out.is_empty());
    }
}
