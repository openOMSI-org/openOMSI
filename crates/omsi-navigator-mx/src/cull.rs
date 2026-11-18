use glam::{Mat4, Vec2, Vec3, Vec4};
use omsi_ui::Vertex;
use std::ops::Range;

const RUN: usize = 768;
const GROUP: usize = 8;
const GAP: u32 = 2 * RUN as u32;
const MAX_DRAWS: usize = 16;

#[derive(Clone, Copy, Debug)]
pub(super) struct Run {
    end: u32,
    lo: Vec3,
    hi: Vec3,
    ext_m: f32,
    ext_px: f32,
}

#[derive(Default)]
pub(super) struct Runs {
    runs: Vec<Run>,
    groups: Vec<Run>,
}

impl Run {
    fn empty(end: u32) -> Run {
        Run { end, lo: Vec3::MAX, hi: Vec3::MIN, ext_m: 0.0, ext_px: 0.0 }
    }

    fn take(mut self, o: &Run) -> Run {
        self.lo = self.lo.min(o.lo);
        self.hi = self.hi.max(o.hi);
        self.ext_m = self.ext_m.max(o.ext_m);
        self.ext_px = self.ext_px.max(o.ext_px);
        self.end = o.end;
        self
    }

    fn seen(&self, planes: &[Vec4; 5], mpp: f32) -> bool {
        let m = self.ext_m.max(self.ext_px * mpp);
        let (lo, hi) = (self.lo - Vec3::new(m, m, 0.0), self.hi + Vec3::new(m, m, 0.0));
        planes.iter().all(|p| {
            let far = Vec3::new(if p.x > 0.0 { hi.x } else { lo.x }, if p.y > 0.0 { hi.y } else { lo.y }, if p.z > 0.0 { hi.z } else { lo.z });
            p.truncate().dot(far) + p.w >= 0.0
        })
    }
}

fn grouped(runs: Vec<Run>) -> Runs {
    let groups = runs.chunks(GROUP).map(|c| c.iter().fold(Run::empty(0), |g, r| g.take(r))).collect();
    Runs { runs, groups }
}

pub(super) fn runs(v: &[Vertex]) -> Runs {
    let mut end = 0u32;
    grouped(
        v.chunks(RUN)
            .map(|c| {
                end += c.len() as u32;
                let mut r = Run::empty(end);
                for x in c {
                    let p = Vec3::from(x.pos);
                    let q = if x.mode[0] > 2.5 { p + Vec3::new(x.uv[0], x.uv[1], 0.0) } else { p };
                    r.lo = r.lo.min(p).min(q);
                    r.hi = r.hi.max(p).max(q);
                    let e = Vec2::from(x.ext).length();
                    r.ext_m = r.ext_m.max(e * x.width[0]);
                    r.ext_px = r.ext_px.max(e * x.width[1]);
                    if x.mode[0] < 0.5 {
                        r.lo = Vec3::splat(-1e9);
                        r.hi = Vec3::splat(1e9);
                    }
                }
                r
            })
            .collect(),
    )
}

pub(super) fn visible(runs: &Runs, view_proj: Mat4, mpp: f32) -> Vec<Range<u32>> {
    let total = runs.runs.last().map_or(0, |r| r.end);
    if omsi_cfg::flags::OMSI_NO_NAV_CULL.is_set() {
        return std::iter::once(0..total).collect();
    }
    let [r0, r1, r2, r3] = [0, 1, 2, 3].map(|i| view_proj.row(i));
    let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2];
    let mut out: Vec<Range<u32>> = Vec::new();
    let mut start = 0;
    for (g, chunk) in runs.groups.iter().zip(runs.runs.chunks(GROUP)) {
        if !g.seen(&planes, mpp) {
            start = g.end;
            continue;
        }
        for r in chunk {
            if r.seen(&planes, mpp) {
                match out.last_mut() {
                    Some(l) if start - l.end <= GAP => l.end = r.end,
                    _ => out.push(start..r.end),
                }
            }
            start = r.end;
        }
    }
    while out.len() > MAX_DRAWS {
        let i = (0..out.len() - 1).min_by_key(|&i| out[i + 1].start - out[i].end).unwrap_or(0);
        out[i].end = out.remove(i + 1).end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(at: Vec2) -> [Vertex; 3] {
        let v = |d: Vec2| Vertex { pos: [at.x + d.x, at.y + d.y, 0.0], ext: [0.0, 1.0], width: [2.0, 3.0], uv: [0.0; 2], color: [1.0; 4], mode: [1.0, 0.0] };
        [v(Vec2::ZERO), v(Vec2::X), v(Vec2::Y)]
    }

    #[test]
    fn only_the_runs_the_camera_sees_are_drawn_and_in_their_order() {
        let mut verts = Vec::new();
        for k in 0..6 {
            let at = if k % 2 == 0 { Vec2::new(0.0, 100.0) } else { Vec2::new(0.0, -2000.0) };
            for _ in 0..RUN / 3 {
                verts.extend(quad(at));
            }
        }
        let rs = runs(&verts);
        assert_eq!(rs.runs.len(), 6);
        let view = Mat4::look_at_rh(Vec3::new(0.0, -50.0, 120.0), Vec3::new(0.0, 60.0, 0.0), Vec3::Z);
        let vp = Mat4::perspective_rh(40f32.to_radians(), 1.25, 1.0, 20000.0) * view;
        let n = RUN as u32;
        assert_eq!(visible(&rs, vp, 0.5), vec![0..n * 5]);
        let far = grouped(rs.runs.iter().map(|r| Run { end: r.end * 4, ..*r }).collect());
        assert_eq!(visible(&far, vp, 0.5), vec![0..4 * n, 8 * n..12 * n, 16 * n..20 * n]);
    }

    #[test]
    fn a_road_just_off_the_edge_is_kept_for_its_width_in_pixels() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, -50.0, 120.0), Vec3::new(0.0, 60.0, 0.0), Vec3::Z);
        let vp = Mat4::perspective_rh(40f32.to_radians(), 1.25, 1.0, 20000.0) * view;
        let edge = (0..400).map(|x| x as f32).find(|&x| (vp * Vec4::new(x, 60.0, 0.0, 1.0)).x > (vp * Vec4::new(x, 60.0, 0.0, 1.0)).w).unwrap();
        let verts = quad(Vec2::new(edge + 1.5, 60.0));
        let rs = runs(&verts);
        assert_eq!(visible(&rs, vp, 0.0), vec![0..3]);
        assert!(visible(&runs(&quad(Vec2::new(edge + 40.0, 60.0))), vp, 0.0).is_empty());
        assert_eq!(visible(&runs(&quad(Vec2::new(edge + 40.0, 60.0))), vp, 20.0), vec![0..3]);
    }

    #[test]
    fn scattered_runs_are_drawn_in_a_few_draws_that_hold_them_all() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, -50.0, 120.0), Vec3::new(0.0, 60.0, 0.0), Vec3::Z);
        let vp = Mat4::perspective_rh(40f32.to_radians(), 1.25, 1.0, 20000.0) * view;
        let mut verts = Vec::new();
        for k in 0..80 {
            let at = if k % 4 == 0 { Vec2::new(0.0, 100.0) } else { Vec2::new(0.0, -2000.0) };
            for _ in 0..RUN / 3 {
                verts.extend(quad(at));
            }
        }
        let n = RUN as u32;
        let seen = visible(&runs(&verts), vp, 0.5);
        assert_eq!(seen.len(), MAX_DRAWS);
        assert!(seen.windows(2).all(|w| w[0].end < w[1].start));
        assert!((0..20).all(|k| seen.iter().any(|r| r.contains(&(4 * k * n)) && r.contains(&((4 * k + 1) * n - 1)))));
    }
}
