//! A dense map loses detail uniformly, not everything drawn after the first full layer.
//! Exact vertices are kept until the budget is exceeded. Then all triangles, old and new,
//! are composited into a fixed grid covering the build's area. Rows of equal cells become
//! quads. A finer, screen-scaled grid is used when its actual runs fit; the coarse grid
//! remains the worst-case fallback. No GPU or readback needed.

use super::Vertex;
use glam::Vec2;

pub(super) struct GroundBudget {
    limit: usize,
    reach: f32,
    mpp: f32,
    grid: Option<Grid>,
    coarse_side: usize,
}

impl GroundBudget {
    pub fn new(limit: usize, reach: f32, mpp: f32) -> Self {
        assert!(limit >= 6, "a ground budget must hold one quad");
        Self {
            limit,
            reach: if reach.is_finite() && reach > 0.0 {
                reach
            } else {
                1.0
            },
            mpp: if mpp.is_finite() { mpp.max(0.0) } else { 0.0 },
            grid: None,
            coarse_side: Grid::side_for(limit),
        }
    }

    pub fn reduced(&self) -> bool {
        self.grid.is_some()
    }

    pub fn clear(&mut self) {
        self.grid = None;
    }

    pub fn push(&mut self, exact: &mut Vec<Vertex>, vertices: &[Vertex]) {
        debug_assert_eq!(vertices.len() % 3, 0);
        for t in vertices.chunks_exact(3) {
            if self.visible(t) {
                self.push_visible(exact, t);
            }
        }
    }

    fn visible(&self, t: &[Vertex]) -> bool {
        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for v in t {
            // Pixel-sized extrusion depends on perspective depth, which this painter does
            // not know. Keep it conservatively for the existing GPU visibility culling.
            if v.width[1] != 0.0 || v.mode[0] != 1.0 {
                return true;
            }
            let p = Vec2::new(v.pos[0], v.pos[1]) + Vec2::from(v.ext) * v.width[0];
            if !p.is_finite() {
                return true;
            }
            lo = lo.min(p);
            hi = hi.max(p);
        }
        // A triangle crossing the build area stays intact, even if all its corners are out.
        lo.x <= self.reach && lo.y <= self.reach && hi.x >= -self.reach && hi.y >= -self.reach
    }

    fn push_visible(&mut self, exact: &mut Vec<Vertex>, vertices: &[Vertex]) {
        if self.grid.is_none() && exact.len().saturating_add(vertices.len()) <= self.limit {
            exact.extend_from_slice(vertices);
            return;
        }
        let starting = self.grid.is_none();
        if starting {
            // One raster only: at most 16 MiB of colors and 4 MiB of draw-order tags.
            // Integer scale factors let the bounded fallback be derived without rerasterizing.
            let wanted = if self.mpp > 0.0 {
                (2.0 * self.reach / self.mpp).ceil() as usize
            } else {
                0
            };
            let factor = wanted
                .div_ceil(self.coarse_side)
                .clamp(1, 4.min(1024 / self.coarse_side));
            self.grid = Some(Grid::with_side(
                self.coarse_side * factor,
                self.reach,
                self.mpp,
            ));
        }
        let grid = self.grid.as_mut().unwrap();
        for t in exact.chunks_exact(3).chain(vertices.chunks_exact(3)) {
            grid.triangle(t);
        }
        if starting {
            *exact = Vec::new();
        }
    }

    pub fn finish(self, exact: Vec<Vertex>) -> Vec<Vertex> {
        match self.grid {
            Some(grid) => {
                drop(exact);
                if grid.side > self.coarse_side && grid.vertex_count() > self.limit {
                    grid.coarsened(self.coarse_side).vertices()
                } else {
                    grid.vertices()
                }
            }
            None => exact,
        }
    }
}

struct Grid {
    side: usize,
    reach: f32,
    mpp: f32,
    /// Linear, premultiplied RGBA; transparent cells have no ground to draw.
    cells: Vec<[f32; 4]>,
    /// Last triangle touching each cell, so reduction retains late, narrow road markings.
    stamps: Vec<u32>,
    stamp: u32,
    /// Land classes and road passes repeat one colour over many thousands of triangles.
    last_color: Option<([[f32; 4]; 3], [f32; 4])>,
}

impl Grid {
    #[cfg(test)]
    fn new(limit: usize, reach: f32, mpp: f32) -> Self {
        Self::with_side(Self::side_for(limit), reach, mpp)
    }

    fn side_for(limit: usize) -> usize {
        ((limit / 6) as f64).sqrt().floor().clamp(1.0, 512.0) as usize
    }

    fn coarsened(mut self, side: usize) -> Self {
        debug_assert_eq!(self.side % side, 0);
        let scale = self.side / side;
        // Keep the latest painted sample instead of averaging a narrow late road into
        // its background. Colors are already composited; do not blend alpha a second time.
        for y in 0..side {
            for x in 0..side {
                let mut sample = y * scale * self.side + x * scale;
                for dy in 0..scale {
                    for dx in 0..scale {
                        let i = (y * scale + dy) * self.side + x * scale + dx;
                        if self.stamps[i] > self.stamps[sample] {
                            sample = i;
                        }
                    }
                }
                // Destination precedes unread source blocks; no second grid is allocated.
                self.cells[y * side + x] = self.cells[sample];
                self.stamps[y * side + x] = self.stamps[sample];
            }
        }
        self.cells.truncate(side * side);
        self.stamps.truncate(side * side);
        self.side = side;
        self
    }

    fn with_side(side: usize, reach: f32, mpp: f32) -> Self {
        Self {
            side,
            reach,
            mpp,
            cells: vec![[0.0; 4]; side * side],
            stamps: vec![0; side * side],
            stamp: 0,
            last_color: None,
        }
    }

    fn triangle(&mut self, t: &[Vertex]) {
        let p: [Vec2; 3] = std::array::from_fn(|i| {
            let v = t[i];
            let at = Vec2::new(v.pos[0], v.pos[1])
                + Vec2::from(v.ext) * v.width[0].max(v.width[1] * self.mpp);
            (at / self.reach + Vec2::ONE) * (self.side as f32 * 0.5)
        });
        if p.iter().any(|p| !p.is_finite())
            || t.iter().any(|v| v.color.iter().any(|c| !c.is_finite()))
        {
            return;
        }
        if (p[1] - p[0]).perp_dot(p[2] - p[0]).abs() < 1e-10 {
            return;
        }
        let low = p
            .iter()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let high = p
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .clamp(0.0, self.side as f32) as usize;
        let key = [t[0].color, t[1].color, t[2].color];
        let color = self
            .last_color
            .filter(|(old, _)| *old == key)
            .map(|(_, color)| color)
            .unwrap_or_else(|| {
                std::array::from_fn(|k| {
                    t.iter()
                        .map(|v| {
                            if k == 3 {
                                v.color[k].clamp(0.0, 1.0)
                            } else {
                                linear(v.color[k])
                            }
                        })
                        .sum::<f32>()
                        / 3.0
                })
            });
        self.last_color = Some((key, color));
        if color[3] == 0.0 {
            return;
        }
        self.stamp = self.stamp.saturating_add(1);
        // Conservative coverage keeps a narrow road even when no cell centre falls in it.
        // Clip each triangle to a row's slab before finding its x span, not its whole box.
        for y in low..high {
            let Some((a, b)) = row_span(p, y as f32, (y + 1) as f32) else {
                continue;
            };
            let a = a.floor().clamp(0.0, self.side as f32) as usize;
            let b = b.ceil().clamp(0.0, self.side as f32) as usize;
            self.stamps[y * self.side + a..y * self.side + b].fill(self.stamp);
            let cells = &mut self.cells[y * self.side + a..y * self.side + b];
            if color[3] == 1.0 {
                cells.fill(color);
                continue;
            }
            for dst in cells {
                for k in 0..3 {
                    dst[k] = color[k] * color[3] + dst[k] * (1.0 - color[3]);
                }
                dst[3] = color[3] + dst[3] * (1.0 - color[3]);
            }
        }
    }

    fn vertex_count(&self) -> usize {
        self.cells
            .chunks_exact(self.side)
            .map(|row| {
                row.iter()
                    .enumerate()
                    .filter(|(x, color)| color[3] > 0.0 && (*x == 0 || **color != row[*x - 1]))
                    .count()
            })
            .sum::<usize>()
            * 6
    }

    fn vertices(self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let coordinate = |i: usize| (2.0 * (i as f32 / self.side as f32) - 1.0) * self.reach;
        for y in 0..self.side {
            let mut x = 0;
            while x < self.side {
                let color = self.cells[y * self.side + x];
                let first = x;
                x += 1;
                while x < self.side && self.cells[y * self.side + x] == color {
                    x += 1;
                }
                if color[3] <= 0.0 {
                    continue;
                }
                let color = [
                    srgb(color[0] / color[3]),
                    srgb(color[1] / color[3]),
                    srgb(color[2] / color[3]),
                    color[3],
                ];
                let v = |x: usize, y: usize| Vertex {
                    pos: [coordinate(x), coordinate(y), 0.0],
                    color,
                    mode: [1.0, 0.0],
                    ..Default::default()
                };
                let (a, b, c, d) = (v(first, y), v(x, y), v(x, y + 1), v(first, y + 1));
                out.extend([a, b, c, a, c, d]);
            }
        }
        out
    }
}

/// The x extent of a triangle intersecting the horizontal strip [low, high].
fn row_span(p: [Vec2; 3], low: f32, high: f32) -> Option<(f32, f32)> {
    let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut take = |x: f32| {
        left = left.min(x);
        right = right.max(x);
    };
    for i in 0..3 {
        let (a, b) = (p[i], p[(i + 1) % 3]);
        if a.y >= low && a.y <= high {
            take(a.x);
        }
        for y in [low, high] {
            if (a.y < y && b.y >= y) || (b.y < y && a.y >= y) {
                take(a.x + (b.x - a.x) * ((y - a.y) / (b.y - a.y)));
            }
        }
    }
    (left <= right).then_some((left, right))
}

fn linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests;
