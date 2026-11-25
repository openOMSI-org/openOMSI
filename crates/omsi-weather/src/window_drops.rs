//! A bounded population of collector drops. Fine spray stays procedural; these
//! drops retain water and momentum, merge on contact and leave the pane at its edges.

use glam::Vec2;

const LIMIT: usize = 128;
const RAIN_LIMIT: usize = LIMIT - 4; // leave room for runoff from the wiper banks
const MAP: usize = 512;
const TRAIL: usize = 4;
const BAND: usize = 32;
// Enlarge the visible footprint without changing the water volume, pinning or runoff.
// Millimetre-sized caps otherwise fade into subpixel specks from the driver's seat (#3019).
// The enlargement follows the rain: a drizzle's drops are small beads, a downpour's are the
// full size (#3076).
const DROP_SCALE: f32 = 2.0;
const DRIZZLE_SCALE: f32 = 1.2;
/// The widest a drop is drawn (m) before the scale, and how much wider a heavy rain's are.
const DROP_CAP: f32 = 0.0022;
const HEAVY_CAP: f32 = 0.7;

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How drops are drawn at a rain rate (0 .. 1 and more with the air stream): their
/// enlargement and their widest radius.
fn look(rain: f32) -> [f32; 2] {
    let r = rain.clamp(0.0, 1.0);
    [
        DRIZZLE_SCALE + (DROP_SCALE - DRIZZLE_SCALE) * smooth(0.1, 0.8, r),
        DROP_CAP * (1.0 + HEAVY_CAP * smooth(0.5, 1.0, r)),
    ]
}

#[derive(Clone)]
pub struct Drop {
    pub pos: Vec2,
    pub previous: Vec2,
    pub velocity: Vec2,
    // Volume in cubic millimetres, with the common cap-shape factor omitted.
    pub water: f32,
    radius: f32,
    tail: [Vec2; TRAIL],
}

impl Drop {
    fn new(pos: Vec2, water: f32) -> Self {
        Self {
            pos,
            previous: pos,
            water,
            radius: water.cbrt() * 0.001,
            velocity: Vec2::ZERO,
            tail: [pos; TRAIL],
        }
    }

    fn radius(&self) -> f32 {
        self.radius
    }

    fn paint_radius(&self, look: [f32; 2]) -> f32 {
        self.radius().min(look[1]) * look[0]
    }

    pub fn displace(&mut self, pos: Vec2, velocity: Vec2) {
        self.pos = pos;
        self.previous = pos;
        self.velocity = velocity;
        // A blade removes the old trail too; it must not reappear behind the blade.
        self.tail.fill(pos);
    }
}

pub struct Drops {
    pub drops: Vec<Drop>,
    size: Vec2,
    grid: [usize; 2],
    heads: Vec<i32>,
    next: [i32; LIMIT],
    seed: u32,
    arrivals: f32,
    pub canvas: Canvas,
}

pub struct Canvas {
    /// The drops' enlargement and widest radius (see `look`).
    look: [f32; 2],
    size: Vec2,
    dirty: Vec<usize>,
    bands: [[u16; 2]; MAP / BAND],
    pub image: omsi_texture::Image,
}

impl Drops {
    pub fn new(size: Vec2, wet: f32, seed: u32) -> Self {
        let grid = [
            (size.x / 0.02).ceil() as usize,
            (size.y / 0.02).ceil() as usize,
        ];
        let mut this = Self {
            drops: Vec::with_capacity(LIMIT),
            size,
            grid,
            heads: vec![-1; grid[0] * grid[1]],
            next: [-1; LIMIT],
            seed,
            arrivals: 0.0,
            canvas: Canvas::new(size),
        };
        // Match an already wet script state without making all the drops the same age.
        // Larger visible caps need fewer collectors to obstruct the view.
        for _ in 0..((wet * size.element_product() * 30.0) as usize).min(RAIN_LIMIT) {
            let pos = Vec2::new(this.random(), this.random()) * size;
            let water = 0.4 + this.random().powi(3) * 14.0;
            this.drops.push(Drop::new(pos, water));
        }
        this
    }

    /// Transfer pooled water into a nearby drop, or start a bounded new runner.
    pub fn feed(&mut self, pos: Vec2, water: f32, velocity: Vec2) -> bool {
        if let Some(drop) = self
            .drops
            .iter_mut()
            .find(|d| d.pos.distance_squared(pos) < 0.015 * 0.015)
        {
            let sum = drop.water + water;
            drop.velocity = (drop.velocity * drop.water + velocity * water) / sum;
            drop.water = sum;
            drop.radius = sum.cbrt() * 0.001;
        } else if self.drops.len() < LIMIT {
            let mut drop = Drop::new(pos, water);
            drop.velocity = velocity;
            self.drops.push(drop);
        } else {
            return false;
        }
        true
    }

    pub fn random(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) as f32 / 16777216.0
    }

    fn cell(&self, pos: Vec2) -> [usize; 2] {
        let p = pos / self.size;
        [
            ((p.x * self.grid[0] as f32) as usize).min(self.grid[0] - 1),
            ((p.y * self.grid[1] as f32) as usize).min(self.grid[1] - 1),
        ]
    }

    /// Surface resistance decreases with size; drag grows with relative airspeed
    /// squared. The coefficients approximate ordinary glass, not a CFD solver.
    pub fn advance(
        &mut self,
        dt: f32,
        rain: f32,
        gravity: Vec2,
        air: Vec2,
        spread: f32,
        wetness: impl Fn(Vec2) -> f32,
    ) {
        let steps = (dt / (1.0 / 60.0)).ceil().max(1.0) as usize;
        let step = dt / steps as f32;
        self.canvas.look = look(rain);
        for drop in &mut self.drops {
            drop.previous = drop.pos;
        }
        for _ in 0..steps {
            self.arrivals += step * rain * self.size.element_product() * 12.0;
            while self.arrivals >= 1.0 {
                self.arrivals -= 1.0;
                let pos = Vec2::new(self.random(), self.random()) * self.size;
                // Freshly wiped glass needs time to collect visible drops again.
                if self.drops.len() < RAIN_LIMIT && wetness(pos) > 0.06 {
                    // (raindrops are bigger the harder it rains)
                    let water = (0.12 + self.random() * 0.5) * (1.0 + 1.5 * rain.clamp(0.0, 1.0));
                    self.drops.push(Drop::new(pos, water));
                }
            }
            for drop in &mut self.drops {
                let wet = wetness(drop.pos).clamp(0.0, 1.0);
                let r = (drop.radius() * 1000.0).max(0.4);
                let outward = (drop.pos.x / self.size.x * 2.0 - 1.0) * spread;
                let force = gravity + (air + Vec2::new(outward, 0.0)) / r;
                let strength = force.length();
                // (a downpour's impacts shake the drops loose: the heavier, the less they hold)
                let pinning = 1.7 / (r * r) * (1.0 - 0.3 * smooth(0.3, 1.0, rain));
                let drive = force.normalize_or_zero() * (strength - pinning).max(0.0) * 0.4;
                drop.velocity = (drop.velocity + drive * step) / (1.0 + step * 4.0);
                let previous = drop.pos;
                drop.pos += drop.velocity * step;
                let distance = previous.distance(drop.pos);
                // Rain feeds pinned drops; a runner also collects the fine water in its path.
                drop.water += step * rain * 0.65 + distance * drop.radius() * wet * 12000.0;
                if rain == 0.0 {
                    drop.water = (drop.water - step * 0.015).max(0.0);
                }
                drop.radius = drop.water.cbrt() * 0.001;
                if drop.pos.distance_squared(drop.tail[0]) > 0.08 * 0.08 {
                    drop.tail.copy_within(0..TRAIL - 1, 1);
                    drop.tail[0] = previous;
                }
            }
            self.drops.retain(|d| {
                d.water > 0.01
                    && d.pos.cmpge(Vec2::ZERO).all()
                    && d.pos.cmplt(self.size).all()
                    && wetness(d.pos).is_finite()
            });
            self.merge(step);
        }
    }

    fn merge(&mut self, step: f32) {
        self.heads.fill(-1);
        let padding = self
            .drops
            .iter()
            .map(|d| d.radius() + d.velocity.length() * step)
            .fold(0.0, f32::max);
        for i in 0..self.drops.len() {
            let drop = &self.drops[i];
            let start = drop.pos - drop.velocity * step;
            let reach = Vec2::splat(drop.radius() + padding);
            let lo = self.cell(start.min(drop.pos) - reach);
            let hi = self.cell(start.max(drop.pos) + reach);
            // Query only cells touched by this step, including the other drops' motion.
            for y in lo[1]..=hi[1] {
                for x in lo[0]..=hi[0] {
                    let mut j = self.heads[y * self.grid[0] + x];
                    while j >= 0 {
                        let k = j as usize;
                        let (a, b) = self.drops.split_at_mut(i);
                        let (other, drop) = (&mut a[k], &mut b[0]);
                        let contact = drop.radius() + other.radius();
                        let motion = (drop.velocity - other.velocity) * step;
                        let start = drop.pos - other.pos - motion;
                        let t = (-start.dot(motion) / motion.length_squared().max(1e-12))
                            .clamp(0.0, 1.0);
                        if other.water > 0.0
                            && (start + motion * t).length_squared() < contact * contact
                        {
                            let sum = drop.water + other.water;
                            if other.velocity.length_squared() > drop.velocity.length_squared() {
                                drop.tail = other.tail;
                            }
                            drop.previous =
                                (drop.previous * drop.water + other.previous * other.water) / sum;
                            drop.pos = (drop.pos * drop.water + other.pos * other.water) / sum;
                            drop.velocity =
                                (drop.velocity * drop.water + other.velocity * other.water) / sum;
                            drop.water = sum;
                            drop.radius = sum.cbrt() * 0.001;
                            other.water = 0.0;
                        }
                        j = self.next[k];
                    }
                }
            }
            let [x, y] = self.cell(self.drops[i].pos);
            let head = &mut self.heads[y * self.grid[0] + x];
            self.next[i] = *head;
            *head = i as i32;
        }
        self.drops.retain(|d| d.water > 0.0);
    }

    pub fn paint(&mut self) -> bool {
        self.canvas.paint(&self.drops)
    }
}

impl Canvas {
    fn new(size: Vec2) -> Self {
        let mut image = omsi_texture::Image {
            width: MAP as u32,
            height: MAP as u32,
            rgba: vec![0; MAP * MAP * 4],
            has_alpha: true,
        };
        for pixel in image.rgba.chunks_exact_mut(4) {
            pixel[..2].fill(128);
        }
        Self {
            look: [DROP_SCALE, DROP_CAP],
            size,
            dirty: Vec::with_capacity(MAP * MAP / 8),
            bands: [[u16::MAX, 0]; MAP / BAND],
            image,
        }
    }

    pub fn take(&mut self) -> Canvas {
        let empty = Self {
            look: self.look,
            size: self.size,
            dirty: Vec::new(),
            bands: [[u16::MAX, 0]; MAP / BAND],
            image: omsi_texture::Image {
                width: 0,
                height: 0,
                rgba: Vec::new(),
                has_alpha: true,
            },
        };
        std::mem::replace(self, empty)
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.size);
    }

    pub fn changed(&self) -> impl Iterator<Item = [u32; 4]> + '_ {
        self.bands
            .iter()
            .enumerate()
            .filter(|(_, b)| b[0] <= b[1])
            .map(|(k, b)| {
                [
                    b[0] as u32,
                    (k * BAND) as u32,
                    (b[1] - b[0] + 1) as u32,
                    BAND as u32,
                ]
            })
    }

    fn mark(&mut self, i: usize) {
        let band = &mut self.bands[i / MAP / BAND];
        let x = (i % MAP) as u16;
        band[0] = band[0].min(x);
        band[1] = band[1].max(x);
    }

    /// RG is the cap slope, B a cleared runner track, A water coverage. Reuses the
    /// material's bump binding. Clear only touched pixels, not the entire map.
    pub fn paint(&mut self, drops: &[Drop]) -> bool {
        let changed = !self.dirty.is_empty() || !drops.is_empty();
        self.bands = [[u16::MAX, 0]; MAP / BAND];
        for k in 0..self.dirty.len() {
            let i = self.dirty[k];
            self.image.rgba[i * 4..i * 4 + 4].copy_from_slice(&[128, 128, 0, 0]);
            self.mark(i);
        }
        self.dirty.clear();
        for drop in drops {
            let pos = drop.pos;
            let radius = drop.paint_radius(self.look);
            let tail = drop.tail;
            // A short, narrowing filament is distinct from the rounded moving head.
            if drop.velocity.length_squared() > 0.0001 {
                let mut start = pos;
                for (k, &end) in tail.iter().enumerate() {
                    let fade = 1.0 - k as f32 / TRAIL as f32;
                    if start.distance_squared(end) > 1e-10 {
                        self.stamp(start, end, radius * fade, true);
                    }
                    start = end;
                }
            }
            self.stamp(pos, pos, radius, false);
        }
        changed
    }

    fn stamp(&mut self, a: Vec2, b: Vec2, radius: f32, trail: bool) {
        let scale = Vec2::splat(MAP as f32) / self.size;
        let aa = (self.size / MAP as f32).max_element() * 0.6;
        let lo = ((a.min(b) - Vec2::splat(radius + aa)) * scale)
            .floor()
            .max(Vec2::ZERO);
        let hi = ((a.max(b) + Vec2::splat(radius + aa)) * scale)
            .ceil()
            .min(Vec2::splat((MAP - 1) as f32));
        let edge = b - a;
        let inverse_length = edge.length_squared().max(1e-10).recip();
        let water_radius = if trail { radius * 0.25 } else { radius };
        let reach = (radius + aa) * (radius + aa);
        for y in lo.y as usize..=hi.y as usize {
            // Clip a diagonal trail per row instead of visiting its mostly empty rectangle.
            let (left, right) = if edge.y.abs() > 1e-8 {
                let centre = (y as f32 + 0.5) / scale.y - a.y;
                let t0 = ((centre - radius - aa) / edge.y).clamp(0.0, 1.0);
                let t1 = ((centre + radius + aa) / edge.y).clamp(0.0, 1.0);
                let x0 = a.x + edge.x * t0;
                let x1 = a.x + edge.x * t1;
                (
                    ((x0.min(x1) - radius - aa) * scale.x).floor().max(lo.x),
                    ((x0.max(x1) + radius + aa) * scale.x).ceil().min(hi.x),
                )
            } else {
                (lo.x, hi.x)
            };
            for x in left as usize..=right as usize {
                let pos = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / scale;
                let t = ((pos - a).dot(edge) * inverse_length).clamp(0.0, 1.0);
                let offset = pos - a.lerp(b, t);
                let distance_squared = offset.length_squared();
                if distance_squared >= reach {
                    continue;
                }
                let distance = distance_squared.sqrt();
                let coverage = ((radius + aa - distance) / (2.0 * aa)).clamp(0.0, 1.0)
                    * (radius / aa).min(1.0);
                // The collected channel is as wide as the head; only the water
                // filament left in its centre is thin.
                let track = if trail {
                    (coverage * 255.0 + 0.5) as u8
                } else {
                    0
                };
                let water_coverage = ((water_radius + aa - distance) / (2.0 * aa)).clamp(0.0, 1.0)
                    * (water_radius / aa).min(1.0);
                let alpha = (water_coverage * 255.0 + 0.5) as u8;
                if alpha == 0 && track == 0 {
                    continue;
                }
                let i = y * MAP + x;
                self.mark(i);
                let pixel = &mut self.image.rgba[i * 4..i * 4 + 4];
                if pixel[2] == 0 && pixel[3] == 0 {
                    self.dirty.push(i);
                }
                if trail {
                    pixel[2] = pixel[2].max(track);
                }
                if pixel[3] < alpha {
                    let slope = (offset / water_radius.max(1e-5)).clamp_length_max(1.0) * 0.5;
                    pixel[0] = (128.5 + slope.x * 127.0) as u8;
                    pixel[1] = (128.5 + slope.y * 127.0) as u8;
                    pixel[3] = alpha;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint_every_pixel(drops: &mut Drops) {
        for i in drops.canvas.dirty.drain(..) {
            drops.canvas.image.rgba[i * 4..i * 4 + 4].copy_from_slice(&[128, 128, 0, 0]);
        }
        for i in 0..drops.drops.len() {
            let drop = &drops.drops[i];
            let (pos, radius, tail) = (drop.pos, drop.paint_radius(drops.canvas.look), drop.tail);
            if drop.velocity.length_squared() > 0.0001 {
                let mut start = pos;
                for (k, &end) in tail.iter().enumerate() {
                    if start.distance_squared(end) > 1e-10 {
                        stamp_every_pixel(
                            drops,
                            start,
                            end,
                            radius * (1.0 - k as f32 / TRAIL as f32),
                            true,
                        );
                    }
                    start = end;
                }
            }
            stamp_every_pixel(drops, pos, pos, radius, false);
        }
    }

    fn stamp_every_pixel(drops: &mut Drops, a: Vec2, b: Vec2, radius: f32, trail: bool) {
        let scale = Vec2::splat(MAP as f32) / drops.size;
        let aa = (drops.size / MAP as f32).max_element() * 0.6;
        let lo = ((a.min(b) - Vec2::splat(radius + aa)) * scale)
            .floor()
            .max(Vec2::ZERO);
        let hi = ((a.max(b) + Vec2::splat(radius + aa)) * scale)
            .ceil()
            .min(Vec2::splat((MAP - 1) as f32));
        let edge = b - a;
        let inverse_length = edge.length_squared().max(1e-10).recip();
        let water_radius = if trail { radius * 0.25 } else { radius };
        for y in lo.y as usize..=hi.y as usize {
            for x in lo.x as usize..=hi.x as usize {
                let pos = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / scale;
                let t = ((pos - a).dot(edge) * inverse_length).clamp(0.0, 1.0);
                let offset = pos - a.lerp(b, t);
                let distance = offset.length();
                let coverage = ((radius + aa - distance) / (2.0 * aa)).clamp(0.0, 1.0)
                    * (radius / aa).min(1.0);
                let track = if trail {
                    (coverage * 255.0 + 0.5) as u8
                } else {
                    0
                };
                let water_coverage = ((water_radius + aa - distance) / (2.0 * aa)).clamp(0.0, 1.0)
                    * (water_radius / aa).min(1.0);
                let alpha = (water_coverage * 255.0 + 0.5) as u8;
                if alpha == 0 && track == 0 {
                    continue;
                }
                let i = y * MAP + x;
                let pixel = &mut drops.canvas.image.rgba[i * 4..i * 4 + 4];
                if pixel[2] == 0 && pixel[3] == 0 {
                    drops.canvas.dirty.push(i);
                }
                if trail {
                    pixel[2] = pixel[2].max(track);
                }
                if pixel[3] < alpha {
                    let slope = (offset / water_radius.max(1e-5)).clamp_length_max(1.0) * 0.5;
                    pixel[0] = (128.5 + slope.x * 127.0) as u8;
                    pixel[1] = (128.5 + slope.y * 127.0) as u8;
                    pixel[3] = alpha;
                }
            }
        }
    }

    #[test]
    fn visible_caps_cover_more_glass_without_changing_their_water() {
        let drop = Drop::new(Vec2::splat(0.5), 8.0);
        let mut physical = Canvas::new(Vec2::ONE);
        physical.stamp(drop.pos, drop.pos, drop.radius(), false);
        let mut visible = Canvas::new(Vec2::ONE);
        visible.paint(std::slice::from_ref(&drop));
        let coverage = |canvas: &Canvas| {
            canvas
                .image
                .rgba
                .chunks_exact(4)
                .map(|p| p[3] as u32)
                .sum::<u32>()
        };
        assert!(coverage(&visible) > coverage(&physical) * 3);
        assert_eq!(drop.water, 8.0);
        assert!((drop.radius() - 0.002).abs() < 1e-6);
        // Wiping the last drop also removes its enlarged footprint.
        assert!(visible.paint(&[]));
        assert!(visible.image.rgba.chunks_exact(4).all(|p| p[3] == 0));
        assert!(!visible.paint(&[]));
    }

    #[test]
    fn larger_caps_use_fewer_collectors_and_still_rewet_the_pane() {
        assert_eq!(Drops::new(Vec2::ONE, 1.0, 7).drops.len(), 30);
        let mut drops = Drops::new(Vec2::ONE, 0.0, 7);
        drops.advance(1.0, 1.0, Vec2::ZERO, Vec2::ZERO, 0.0, |_| 1.0);
        assert!((10..=12).contains(&drops.drops.len()));
        let mut wiped = Drops::new(Vec2::ONE, 0.0, 7);
        wiped.advance(1.0, 1.0, Vec2::ZERO, Vec2::ZERO, 0.0, |_| -0.04);
        assert!(wiped.drops.is_empty());
    }

    #[test]
    fn painted_drops_match_a_drop_painted_over_its_whole_square() {
        for (size, seed) in [
            (Vec2::new(2.4, 1.2), 7u32),
            (Vec2::new(0.6, 0.9), 11),
            (Vec2::new(1.1, 0.5), 3),
        ] {
            let mut fast = Drops::new(size, 0.7, seed);
            let mut whole = Drops::new(size, 0.7, seed);
            for step in 0..400 {
                for drops in [&mut fast, &mut whole] {
                    drops.advance(1.0 / 60.0, 1.0, -Vec2::Y, Vec2::new(0.4, 0.1), 0.2, |_| 1.0);
                }
                if step % 2 == 0 {
                    fast.paint();
                    paint_every_pixel(&mut whole);
                    assert!(
                        fast.canvas.image.rgba == whole.canvas.image.rgba,
                        "seed {seed} step {step}"
                    );
                    assert_eq!(
                        fast.canvas.dirty, whole.canvas.dirty,
                        "seed {seed} step {step}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_repainted_pixel_lies_in_a_changed_band() {
        for (size, seed) in [(Vec2::new(2.4, 1.2), 7u32), (Vec2::new(0.6, 0.9), 11)] {
            let mut drops = Drops::new(size, 0.7, seed);
            for step in 0..300 {
                drops.advance(1.0 / 60.0, 1.0, -Vec2::Y, Vec2::new(0.4, 0.1), 0.2, |_| 1.0);
                if step % 2 == 1 {
                    continue;
                }
                let before = drops.canvas.image.rgba.clone();
                drops.paint();
                let bands: Vec<_> = drops.canvas.changed().collect();
                for (i, (a, b)) in before
                    .chunks_exact(4)
                    .zip(drops.canvas.image.rgba.chunks_exact(4))
                    .enumerate()
                {
                    if a != b {
                        let (x, y) = ((i % MAP) as u32, (i / MAP) as u32);
                        assert!(
                            bands.iter().any(|r| x >= r[0]
                                && x < r[0] + r[2]
                                && y >= r[1]
                                && y < r[1] + r[3]),
                            "seed {seed} step {step} pixel {x},{y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_long_update_keeps_all_elapsed_motion_and_empty_glass_needs_no_upload() {
        let mut whole = Drops::new(Vec2::ONE, 0.0, 1);
        let mut split = Drops::new(Vec2::ONE, 0.0, 1);
        assert!(!whole.paint());
        for pop in [&mut whole, &mut split] {
            pop.drops.push(Drop::new(Vec2::splat(0.5), 27.0));
        }
        whole.advance(1.0, 0.0, -Vec2::Y, Vec2::ZERO, 0.0, |_| 1.0);
        for _ in 0..60 {
            split.advance(1.0 / 60.0, 0.0, -Vec2::Y, Vec2::ZERO, 0.0, |_| 1.0);
        }
        assert!(whole.drops[0].pos.distance(split.drops[0].pos) < 1e-5);
        assert!(whole.drops[0].pos.y < 0.48);
        assert!(whole.paint());
        whole.drops.clear();
        assert!(whole.paint());
        assert!(!whole.paint());
    }

    #[test]
    fn coalescence_conserves_volume_and_momentum() {
        let mut drops = Drops::new(Vec2::ONE, 0.0, 1);
        let mut a = Drop::new(Vec2::splat(0.5), 8.0);
        a.velocity = Vec2::X;
        drops.drops.push(a);
        drops.drops.push(Drop::new(Vec2::new(0.501, 0.5), 1.0));
        drops.merge(0.0);
        assert_eq!(drops.drops.len(), 1);
        assert_eq!(drops.drops[0].water, 9.0);
        assert!((drops.drops[0].velocity.x - 8.0 / 9.0).abs() < 1e-6);
    }

    #[test]
    fn a_runner_collects_crossed_beads_without_swallowing_near_misses() {
        let mut drops = Drops::new(Vec2::ONE, 0.0, 1);
        drops.drops.push(Drop::new(Vec2::new(0.5, 0.5), 1.0));
        drops.drops.push(Drop::new(Vec2::new(0.515, 0.5), 1.0));
        let mut runner = Drop::new(Vec2::new(0.5, 0.49), 8.0);
        runner.velocity = -Vec2::Y;
        drops.drops.push(runner);
        drops.merge(0.02);
        assert_eq!(drops.drops.len(), 2);
        let moving = drops.drops.iter().find(|d| d.velocity.y < 0.0).unwrap();
        assert_eq!(moving.water, 9.0);
        assert!((moving.velocity.y + 8.0 / 9.0).abs() < 1e-6);
        assert_eq!(drops.drops.iter().map(|d| d.water).sum::<f32>(), 10.0);
    }

    #[test]
    fn a_runner_clears_a_wider_channel_than_its_thin_water_filament() {
        let mut drops = Drops::new(Vec2::ONE, 0.0, 1);
        drops
            .canvas
            .stamp(Vec2::new(0.5, 0.3), Vec2::new(0.5, 0.7), 0.0022, true);
        assert!(
            drops
                .canvas
                .image
                .rgba
                .chunks_exact(4)
                .any(|p| p[2] > 150 && p[3] < 100)
        );
    }

    #[test]
    fn clipping_trails_keeps_the_whole_capsule_in_both_directions() {
        for (a, b) in [
            (Vec2::new(0.2, 0.3), Vec2::new(0.8, 0.7)),
            (Vec2::new(0.8, 0.7), Vec2::new(0.2, 0.3)),
            (Vec2::new(0.2, 0.3), Vec2::new(0.8, 0.3)),
            (Vec2::new(0.2, 0.3), Vec2::new(0.2, 0.7)),
        ] {
            let mut drops = Drops::new(Vec2::ONE, 0.0, 1);
            drops.canvas.stamp(a, b, 0.0022, true);
            for y in 0..MAP {
                for x in 0..MAP {
                    let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / MAP as f32;
                    let edge = b - a;
                    let t = ((p - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
                    if p.distance(a + edge * t) < 0.0022 {
                        assert!(drops.canvas.image.rgba[(y * MAP + x) * 4 + 2] > 0);
                    }
                }
            }
        }
    }

    #[test]
    fn small_drops_pin_large_drops_fall_and_air_can_reverse_them() {
        let mut drops = Drops::new(Vec2::ONE, 0.0, 1);
        drops.drops.push(Drop::new(Vec2::new(0.25, 0.5), 0.5));
        drops.drops.push(Drop::new(Vec2::new(0.75, 0.5), 27.0));
        drops.advance(0.2, 0.0, -Vec2::Y, Vec2::ZERO, 0.0, |_| 1.0);
        assert_eq!(drops.drops[0].pos.y, 0.5);
        assert!(drops.drops[1].pos.y < 0.5);
        drops.drops[1].velocity = Vec2::ZERO;
        drops.advance(0.2, 0.0, -Vec2::Y, Vec2::Y * 9.0, 0.0, |_| 1.0);
        assert!(drops.drops[1].velocity.y > 0.0);
    }

    #[test]
    fn rain_keeps_arriving_and_large_water_leaves_the_pane() {
        let mut drops = Drops::new(Vec2::ONE, 1.0, 123);
        let original = drops.drops[0].pos;
        for _ in 0..600 {
            drops.advance(0.05, 1.0, -Vec2::Y, Vec2::ZERO, 0.0, |_| 1.0);
        }
        assert!(drops.drops.len() <= LIMIT);
        assert!(
            drops
                .drops
                .iter()
                .all(|d| d.pos.is_finite() && d.water.is_finite())
        );
        assert!(!drops.drops.iter().any(|d| d.pos == original));
        assert!(drops.drops.iter().any(|d| d.velocity.y < -0.02));
        assert!(drops.arrivals < 1.0);
        drops.paint();
        assert!(drops.canvas.image.rgba.chunks_exact(4).any(|p| p[3] > 0));
    }
}
