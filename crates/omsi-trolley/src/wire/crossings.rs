//! Intersections between the physical conductors of two trolley spans.

use super::WireNet;
use glam::DVec3;

/// Where two physical trolley wires cross.
#[derive(Debug, Clone, Copy)]
pub struct Crossing {
    /// The spans that cross.
    pub a: u32,
    pub b: u32,
    /// The side of each span (0 right/positive, 1 left/negative).
    pub side_a: u8,
    pub side_b: u8,
    /// Where on each span the crossing is (the length along the span).
    pub sa: f64,
    pub sb: f64,
    /// Where it is in the world.
    pub pos: DVec3,
}

/// How close two wires must be in height to cross rather than one passing above the other (m).
const CROSS_HEIGHT: f64 = 0.25;
/// Wire extent in XY, including the largest allowed mitre and half spacing.
const WIRE_REACH: f64 = 2.0;
/// Ignore conductor contacts this far from a shared node, where joined wires can overlap.
const SHARED_NODE_REACH: f64 = WIRE_REACH + super::JOIN;

/// Where two horizontal segments cross, if they do: the parameters along each.
fn cross_2d(a0: DVec3, a1: DVec3, b0: DVec3, b1: DVec3) -> Option<(f64, f64)> {
    let (r, s) = (a1 - a0, b1 - b0);
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-9 {
        return None;
    }
    let d = b0 - a0;
    let t = (d.x * s.y - d.y * s.x) / den;
    let u = (d.x * r.y - d.y * r.x) / den;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) { Some((t, u)) } else { None }
}

impl WireNet {
    /// Find crossings between the actual left and right conductors, using the span grid to
    /// avoid comparing every pair in a map.
    pub(super) fn build_crossings(&mut self) {
        let boxes: Vec<(DVec3, DVec3)> = self
            .spans
            .iter()
            .map(|s| {
                let mut lo = s.pts[0];
                let mut hi = s.pts[0];
                for p in &s.pts {
                    lo = lo.min(*p);
                    hi = hi.max(*p);
                }
                (lo - DVec3::splat(WIRE_REACH), hi + DVec3::splat(WIRE_REACH))
            })
            .collect();
        let mut crossings = Vec::new();
        let mut on_span: Vec<Vec<(u32, f64)>> = vec![Vec::new(); self.spans.len()];
        for a in 0..self.spans.len() {
            let mut near = Vec::new();
            for cell in self.cells_of(a as u32) {
                let Some(list) = self.grid.get(&cell) else { continue };
                for &b in list {
                    if b as usize > a { near.push(b); }
                }
            }
            near.sort_unstable();
            near.dedup();
            for b in near {
                let (alo, ahi) = boxes[a];
                let (blo, bhi) = boxes[b as usize];
                if ahi.x < blo.x || bhi.x < alo.x || ahi.y < blo.y || bhi.y < alo.y { continue; }
                for (side_a, side_b, sa, sb, pos) in self.wire_intersections(a, b) {
                    if self.spans[a].ends.iter().enumerate().any(|(end_a, node)| {
                        self.spans[b as usize].ends.iter().enumerate().any(|(end_b, other_node)| {
                            node == other_node
                                && sa_from_end(&self.spans[a], end_a, sa) <= SHARED_NODE_REACH
                                && sa_from_end(&self.spans[b as usize], end_b, sb) <= SHARED_NODE_REACH
                        })
                    }) {
                        continue;
                    }
                    let ci = crossings.len() as u32;
                    crossings.push(Crossing { a: a as u32, b, side_a, side_b, sa, sb, pos });
                    on_span[a].push((ci, sa));
                    on_span[b as usize].push((ci, sb));
                }
            }
        }
        self.crossings = crossings;
        self.span_crossings = on_span;
    }

    /// The grid cells a span's physical wires may pass through.
    fn cells_of(&self, span: u32) -> Vec<(i32, i32)> {
        let mut cells = Vec::new();
        for w in self.spans[span as usize].pts.windows(2) {
            let (c0, c1) = (
                super::cell_of(w[0].min(w[1]) - DVec3::splat(WIRE_REACH)),
                super::cell_of(w[0].max(w[1]) + DVec3::splat(WIRE_REACH)),
            );
            for cx in c0.0..=c1.0 {
                for cy in c0.1..=c1.1 {
                    cells.push((cx, cy));
                }
            }
        }
        cells.sort_unstable();
        cells.dedup();
        cells
    }

    /// Every intersection between the two spans' physical wires, excluding span endpoints.
    fn wire_intersections(&self, a: usize, b: u32) -> Vec<(u8, u8, f64, f64, DVec3)> {
        let sa = &self.spans[a];
        let sb = &self.spans[b as usize];
        let mut out = Vec::new();
        for side_a in 0..=1 {
            for side_b in 0..=1 {
                for i in 0..sa.pts.len() - 1 {
                    let (a0, a1) = sa.wire_segment(i, side_a);
                    for j in 0..sb.pts.len() - 1 {
                        let (b0, b1) = sb.wire_segment(j, side_b);
                        let Some((t, u)) = cross_2d(a0, a1, b0, b1) else { continue };
                        let za = a0.z + t * (a1.z - a0.z);
                        let zb = b0.z + u * (b1.z - b0.z);
                        if (za - zb).abs() > CROSS_HEIGHT { continue; }
                        let la = sa.cum[i] + t * sa.pts[i].distance(sa.pts[i + 1]);
                        let lb = sb.cum[j] + u * sb.pts[j].distance(sb.pts[j + 1]);
                        const END_EPS: f64 = 1e-1;
                        if la <= END_EPS || sa.len() - la <= END_EPS || lb <= END_EPS || sb.len() - lb <= END_EPS {
                            continue;
                        }
                        let pos = a0.lerp(a1, t);
                        // Adjacent polyline pieces can describe the same conductor crossing.
                        if out.iter().any(|(as_, bs, x, y, _): &(u8, u8, f64, f64, DVec3)| {
                            *as_ == side_a && *bs == side_b && (*x - la).abs() < 1e-5 && (*y - lb).abs() < 1e-5
                        }) { continue; }
                        out.push((side_a, side_b, la, lb, pos));
                    }
                }
            }
        }
        out
    }

    /// Crossings on a span: crossing id and distance along the span.
    pub fn crossings_of(&self, span: u32) -> &[(u32, f64)] {
        self.span_crossings.get(span as usize).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

#[inline]
fn sa_from_end(span: &super::Span, end: usize, s: f64) -> f64 {
    if end == 0 { s } else { span.len() - s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wires_crossing_at_the_same_height_are_detected() {
        let net = WireNet::parse(
            "[trolley_span]\np -20 0 5.5\np 20 0 5.5\n[trolley_span]\np 0 -20 5.5\np 0 20 5.5\n",
        );
        assert!(!net.crossings.is_empty(), "same-height wires crossing should be recorded");
    }

    #[test]
    fn wire_passing_above_another_is_not_detected_as_a_crossing() {
        let net = WireNet::parse(
            "[trolley_span]\np -20 0 5.5\np 20 0 5.5\n[trolley_span]\np 0 -20 6.5\np 0 20 6.5\n",
        );
        assert!(net.crossings.is_empty(), "wires separated by 1 m should not cross");
    }

    #[test]
    fn a_frog_does_not_create_wire_crossings_at_its_shared_node() {
        let net = WireNet::parse(
            "[trolley_span]\np -50 0 5.5\np 100 0 5.5\n[trolley_span]\np 100 0 5.5\np 400 0 5.5\n[trolley_span]\np 100 0 5.5\np 300 80 5.5\n[trolley_switch]\npos 100 0 5.5\nmode fixed\n",
        );
        assert!(net.crossings.is_empty(), "joined frog spans should not create crossings");
    }

    #[test]
    fn an_angled_joint_does_not_create_wire_crossings() {
        let net = WireNet::parse(
            "[trolley_span]\np -50 0 5.5\np 0 0 5.5\n[trolley_span]\np 0 0 5.5\np 50 50 5.5\n",
        );
        assert!(net.crossings.is_empty(), "joined angled spans should not create crossings");
    }

    #[test]
    fn spans_meeting_at_endpoints_do_not_create_crossings() {
        let net = WireNet::parse(
            "[trolley_span]\np -20 0 5.5\np 0 0 5.5\n[trolley_span]\np 0 0 5.5\np 0 20 5.5\n",
        );
        assert!(net.crossings.is_empty(), "endpoint meetings should not be recorded as crossings");
    }

    #[test]
    fn crossing_in_the_middle_of_a_span_is_still_detected() {
        let net = WireNet::parse(
            "[trolley_span]\np -20 0 5.5\np 20 0 5.5\n[trolley_span]\np 0 -20 5.5\np 0 20 5.5\n",
        );
        assert!(!net.crossings.is_empty(), "interior crossings should still be recorded");
    }

    #[test]
    fn shared_endpoint_does_not_hide_another_interior_crossing() {
        let net = WireNet::parse(
            "[trolley_span]\np 0 0 5.5\np 20 0 5.5\np 20 20 5.5\n[trolley_span]\np 0 0 5.5\np 10 -10 5.5\np 10 10 5.5\n",
        );
        assert!(!net.crossings.is_empty(), "a shared endpoint should not hide a separate interior crossing");
    }
}
