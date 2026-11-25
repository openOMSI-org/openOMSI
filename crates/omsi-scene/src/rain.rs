//! Precipitation: the rain falling round the camera - what it is (how hard, how big its
//! drops) and how the eye moves through it; the renderer works every drop out on the GPU
//! (rain.wgsl) - and the picture the film on the glass wears in a snow weather.

use glam::{DVec3, Vec3};
use omsi_render::{Rainfall, Scene};

/// The rain rate (mm/h) of OMSI's precipitation strength (0..1 of its 255): a drizzle at
/// the bottom, a cloudburst (60 mm/h) at the top.
pub fn rain_rate_mm_h(rate: f32) -> f32 {
    0.3 + 60.0 * rate.clamp(0.0, 1.0).powi(2)
}

/// The median drop diameter (mm) of rain falling at `r` mm/h: Marshall and Palmer's drop
/// sizes (1948), `N(D) = 8000 e^(-ΛD)` with `Λ = 4.1 R^-0.21` per mm, whose median volume
/// diameter is 3.67 / Λ.
pub fn median_drop_mm(r: f32) -> f32 {
    3.67 / (4.1 * r.max(0.01).powf(-0.21))
}

/// The speed (m/s) a raindrop of `d` mm falls at in still air at sea level (Atlas, Srivastava
/// and Sekhon 1973): a drizzle drifts down at 2-4 m/s, the big drops of a cloudburst at 7-8.
/// (rain.wgsl gives every drop its own.)
pub fn terminal_speed(d: f32) -> f32 {
    (9.65 - 10.3 * (-0.6 * d.max(0.0)).exp()).max(0.5)
}

/// The rain's quality of a `rain_quality` setting (`high`, `medium`, `low`): 2 high, 1
/// medium, 0 low.
pub fn quality_of(q: &str) -> u8 {
    match q {
        "low" => 0,
        "medium" => 1,
        _ => 2,
    }
}

/// The weather's wind (m/s, world): the air the rain, the snow and the glass's water move
/// in - Omsi.exe's own vector, blowing from the direction (see
/// `omsi_weather::conditions::wind_vector`).
pub fn weather_wind(weather: &omsi_content::weather::Weather) -> Vec3 {
    omsi_weather::conditions::wind_vector(weather)
}

/// How much of the ground and of what faces up the snow covers (0..1) after `secs` more of
/// this weather (`kind` and `rate` as `Rain::set` takes them, `temp` °C), from `start`. A
/// heavy snowfall lays a few centimetres an hour - a white cover in a quarter of an hour, a
/// light one in an hour or two; flakes landing above +2 °C melt where they land. Without
/// snow falling it lies on below zero, thaws in the warmth (half an hour at +5 °C) and
/// faster in the rain.
pub fn snow_cover(kind: i32, rate: f32, temp: f32, secs: f32, start: f32) -> f32 {
    let rate = rate.clamp(0.0, 1.0);
    let change = if kind == 2 && rate > 0.0 && temp < 2.0 {
        (0.05 + rate) / 900.0
    } else {
        -(temp.max(0.0) / 9000.0 + if kind == 1 { rate / 600.0 } else { 0.0 })
    };
    (start + change * secs).clamp(0.0, 1.0)
}

pub struct Rain {
    kind: i32,
    rate: f32,
    quality: u8,
    /// How thick the drops are (mm, their median volume diameter), from the rain's rate.
    drop_mm: f32,
    /// The eye's velocity (m/s), smoothed over a few frames.
    eye: Vec3,
    /// The weather's temperature (°C) and the snow lying from the snowfalls so far (see
    /// [`snow_cover`]; `None` before the first weather).
    temp: f32,
    cover: Option<f32>,
}

/// The boxes a vehicle keeps the weather out of: its own `[boundingbox]` and each coupled
/// part's (an articulated bus's rear section is a part of its own, with its own box:
/// without it, it rained in the rear saloon, #777).
pub fn vehicle_boxes(v: &omsi_sim::VehicleInstance) -> Vec<(DVec3, f64, [f32; 6])> {
    let front = v.ty.def.bounding_box.map(|bb| (v.position, v.heading, bb));
    let parts = v.trailers.iter().filter_map(|t| {
        // a part coupled the other way round stands turned about its own origin
        let heading = if t.reversed {
            t.heading + 180.0
        } else {
            t.heading
        };
        t.ty.def.bounding_box.map(|bb| (t.position, heading, bb))
    });
    front.into_iter().chain(parts).collect()
}

impl Rain {
    pub fn new() -> Rain {
        Rain {
            kind: 0,
            rate: 0.0,
            quality: 2,
            drop_mm: 2.0,
            eye: Vec3::ZERO,
            temp: 10.0,
            cover: None,
        }
    }

    /// `kind`: 0 none, 1 rain, 2 snow; `rate` 0..1; `quality` the rain's (see [`quality_of`]).
    pub fn set(&mut self, kind: i32, rate: f32, quality: u8) {
        self.kind = kind;
        self.rate = rate.clamp(0.0, 1.0);
        self.quality = quality;
        // the drops of this rain: small and slow in a drizzle, big and fast in a downpour
        // (they all looked the same whatever the rain, #1412)
        self.drop_mm = median_drop_mm(rain_rate_mm_h(self.rate));
    }

    /// The weather's temperature (°C), for the snow lying.
    pub fn set_temperature(&mut self, temp: f32) {
        self.temp = temp;
    }

    /// How much of what faces up the snow covers now (0..1, see [`snow_cover`]).
    pub fn snow_cover(&self) -> f32 {
        self.cover.unwrap_or(0.0)
    }

    /// Hand the renderer this frame's rain (`Scene::rainfall`; snow is drawn by the renderer
    /// from the weather: render snow.wgsl, `Lighting::snowfall`). `carried`: the velocity of
    /// the vehicle the camera rides with (m/s; zero for a free camera or on foot). The
    /// streaks slant with that alone - not with the camera's own way round the bus or a turn
    /// of the head: taken from the camera's motion, the whole rain swung and stretched with
    /// every move of the mouse, which nothing else in the picture does.
    pub fn tick(&mut self, dt: f32, carried: Vec3, wind: Vec3, scene: &mut Scene) {
        if dt > 0.0 {
            self.eye += (carried - self.eye) * (dt / 0.1).min(1.0);
        }
        let start = self.cover.unwrap_or(0.0);
        self.cover = Some(snow_cover(self.kind, self.rate, self.temp, dt, start));
        let raining = self.kind == 1 && self.rate > 0.0;
        scene.rainfall = Rainfall {
            rate: if raining { self.rate } else { 0.0 },
            drop_mm: self.drop_mm,
            eye_velocity: self.eye,
            wind,
            quality: self.quality,
        };
    }
}

/// The picture the windscreen and window layers wear while it snows.
///
/// Every bus carries the same rain film: a mesh in front of the glass textured with
/// `regen.tga` (running drops) and faded in by `[alphascale] Rain_Window_*_Wetness`, which
/// `rain.osc` fills from `PrecipRate` - and `rain.osc` never asks what is falling, so in a
/// snowstorm the original shows raindrops on every pane, in the cab and along the saloon.
/// OMSI's own way out is the seasonal texture folder (`texture\WinterSnow\`), which no stock
/// vehicle fills in, so openOMSI builds the winter picture itself: the snow the wiped
/// panes of the cab wear (render `snow_layer`, at full cover) baked into a tile of a
/// metre of glass (`pane_uv`) for every other pane - the saloon's windows, the doors, the
/// small tilt windows - so that all the glass of a bus holds the same snow. The tile joins
/// its own edges: the cells of its noise are a whole number to the metre and wrap.
pub fn snow_on_glass(_root: &std::path::Path) -> omsi_texture::Image {
    // (made once: the same picture at every reload of a vehicle or a scene)
    static MADE: std::sync::OnceLock<omsi_texture::Image> = std::sync::OnceLock::new();
    MADE.get_or_init(make_snow_on_glass).clone()
}

fn make_snow_on_glass() -> omsi_texture::Image {
    const SIDE: usize = 512;
    // (the shader's hash, on the integer cell: `rain_hash` in 08_rain_glass.wgsl)
    let hash = |x: i32, y: i32| -> [f32; 2] {
        let mut a = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
        a = (a ^ (a >> 16)).wrapping_mul(0x7feb_352d);
        a = (a ^ (a >> 15)).wrapping_mul(0x846c_a68b);
        a ^= a >> 16;
        let mut b = a ^ 0x68bc_21eb;
        b = (b ^ (b >> 16)).wrapping_mul(0x7feb_352d);
        b = (b ^ (b >> 15)).wrapping_mul(0x846c_a68b);
        b ^= b >> 16;
        [a as f32 / 4_294_967_296.0, b as f32 / 4_294_967_296.0]
    };
    let smooth = |lo: f32, hi: f32, x: f32| {
        let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // smooth value noise with `n` cells to the tile, wrapping
    let noise = |u: f32, v: f32, n: i32, shift: i32| -> f32 {
        let (x, y) = (u * n as f32, v * n as f32);
        let (ix, iy) = (x.floor(), y.floor());
        let (fx, fy) = (x - ix, y - iy);
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let at = |dx: i32, dy: i32| {
            hash(
                (ix as i32 + dx).rem_euclid(n) + shift,
                (iy as i32 + dy).rem_euclid(n) + shift,
            )[0]
        };
        let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
        (a + (b - a) * sx) + ((c + (d - c) * sx) - (a + (b - a) * sx)) * sy
    };
    // cells to the tile: the shader's 1.1 cm and 2 cm crystals, 4.5 cm and 1.2 cm lumps
    const CRYSTALS: [i32; 2] = [90, 50];
    let mut rgba = vec![255u8; SIDE * SIDE * 4];
    for py in 0..SIDE {
        for px in 0..SIDE {
            let (u, v) = (
                (px as f32 + 0.5) / SIDE as f32,
                (py as f32 + 0.5) / SIDE as f32,
            );
            let mut flakes = 0.0f32;
            for (layer, n) in CRYSTALS.iter().enumerate() {
                let (x, y) = (u * *n as f32, v * *n as f32);
                let (cx, cy) = (x.floor() as i32, y.floor() as i32);
                let h = hash(
                    cx.rem_euclid(*n) + layer as i32 * 37,
                    cy.rem_euclid(*n) + layer as i32 * 37,
                );
                let h2 = hash(
                    cx.rem_euclid(*n) + layer as i32 * 37 + 91,
                    cy.rem_euclid(*n) + layer as i32 * 37 + 13,
                );
                let (ox, oy) = (cx as f32 + 0.25 + 0.5 * h[0], cy as f32 + 0.25 + 0.5 * h[1]);
                let r = 0.16 + 0.2 * h2[0];
                // (a crystal shows once the cover passes its own threshold: all of the
                // larger layer's and most of the fine one's, at full cover)
                let shown = if h2[1] <= if layer == 1 { 0.85 } else { 1.15 } {
                    1.0
                } else {
                    0.0
                };
                let edge = 1.0 - smooth(r * 0.55, r, ((x - ox).powi(2) + (y - oy).powi(2)).sqrt());
                flakes = flakes.max(shown * edge * (0.6 + 0.4 * h[0]));
            }
            let lumps = noise(u, v, 22, 0) * 0.65 + noise(u, v, 83, 7) * 0.35;
            let c = 0.7 + 0.6 * lumps;
            let sheet = smooth(0.5, 1.2, c) * 0.75 + smooth(1.0, 1.7, c) * 0.22;
            let t = smooth(1.0, 1.8, c);
            let white = [0.9 + 0.07 * t, 0.92 + 0.06 * t, 0.96 + 0.04 * t];
            let alpha = (flakes * 0.92).max(sheet).clamp(0.0, 0.97);
            let p = (py * SIDE + px) * 4;
            for k in 0..3 {
                rgba[p + k] = (white[k] * 255.0) as u8;
            }
            rgba[p + 3] = (alpha * 255.0) as u8;
        }
    }
    omsi_texture::Image {
        width: SIDE as u32,
        height: SIDE as u32,
        rgba,
        has_alpha: true,
    }
}

/// The snow-on-glass picture (`snow_on_glass`) on the GPU, with its mip chain: one level
/// alone, its specks of a pixel or two were point-sampled across a whole windscreen and
/// glittered with every move of the head (#946).
pub fn add_snow_on_glass(
    renderer: &omsi_render::Renderer,
    scene: &mut omsi_render::Scene,
    root: &std::path::Path,
) -> omsi_render::TextureId {
    renderer.add_texture(scene, &snow_on_glass(root), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The snow on the glass goes up with its mip levels, and it is white throughout (a
    /// level made smaller stays white, only less opaque).
    #[test]
    fn snow_on_glass_has_mip_levels_and_stays_white() {
        let img = super::snow_on_glass(std::path::Path::new("/nonexistent"));
        assert!(
            img.rgba
                .chunks_exact(4)
                .all(|p| p[..3].iter().all(|c| *c >= 224))
        );
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let renderer = pollster::block_on(omsi_render::Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            omsi_render::RenderOptions {
                msaa: 1,
                shadow_size: 1024,
                ..Default::default()
            },
        ))
        .expect("noop renderer");
        let mut scene = renderer.new_scene();
        let id =
            super::add_snow_on_glass(&renderer, &mut scene, std::path::Path::new("/nonexistent"));
        assert_eq!(renderer.texture_levels(&scene, id), Some((512, 512, 10)));
    }

    /// The renderer is handed the eye's way through the rain (the streaks are the drops' way
    /// against it), no rain in a dry or a snow weather, and no speed of a jump of the camera.
    #[test]
    fn the_rainfall_follows_the_eye_and_the_weather() {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let renderer = pollster::block_on(omsi_render::Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            omsi_render::RenderOptions {
                msaa: 1,
                shadow_size: 1024,
                ..Default::default()
            },
        ))
        .expect("noop renderer");
        let mut scene = renderer.new_scene();
        let mut rain = Rain::new();
        rain.set(1, 1.0, 2);
        for _ in 0..60 {
            rain.tick(0.05, Vec3::X * 10.0, Vec3::X * 3.0, &mut scene);
        }
        let r = scene.rainfall;
        assert_eq!(r.rate, 1.0);
        assert!(
            (r.eye_velocity - Vec3::X * 10.0).length() < 0.1,
            "{:?}",
            r.eye_velocity
        );
        assert_eq!(r.wind, Vec3::X * 3.0);
        rain.set(2, 1.0, 2);
        rain.tick(0.05, Vec3::ZERO, Vec3::ZERO, &mut scene);
        assert_eq!(scene.rainfall.rate, 0.0);
        rain.set(0, 1.0, 2);
        rain.tick(0.05, Vec3::ZERO, Vec3::ZERO, &mut scene);
        assert_eq!(scene.rainfall.rate, 0.0);
    }

    /// A drizzle falls slower and in smaller drops than a downpour (#1412), as measured.
    #[test]
    fn a_downpour_falls_faster_in_bigger_drops_than_a_drizzle() {
        let drizzle = median_drop_mm(rain_rate_mm_h(0.05));
        let downpour = median_drop_mm(rain_rate_mm_h(1.0));
        assert!(drizzle > 0.5 && drizzle < 1.0, "{drizzle}");
        assert!(downpour > 2.0 && downpour < 2.5, "{downpour}");
        assert!(terminal_speed(drizzle) > 2.0 && terminal_speed(drizzle) < 4.5);
        assert!(terminal_speed(downpour) > 6.5 && terminal_speed(downpour) < 8.0);
        let mut rain = Rain::new();
        rain.set(1, 0.05, 2);
        let small = rain.drop_mm;
        rain.set(1, 1.0, 2);
        assert!(rain.drop_mm > small + 1.0);
    }

    /// Snow lies on in the cold, builds up in a snowfall (faster in a heavy one), melts as
    /// it lands in the warmth and thaws without a snowfall above zero.
    #[test]
    fn the_snow_cover_builds_up_and_thaws() {
        let heavy = snow_cover(2, 1.0, -3.0, 600.0, 0.0);
        let light = snow_cover(2, 0.1, -3.0, 600.0, 0.0);
        assert!(
            heavy > 0.6 && heavy < 0.8 && light < heavy / 3.0,
            "{heavy} {light}"
        );
        assert_eq!(snow_cover(2, 1.0, -3.0, 2000.0, 0.0), 1.0);
        assert_eq!(snow_cover(2, 1.0, 5.0, 600.0, 0.0), 0.0);
        assert_eq!(snow_cover(0, 0.0, -5.0, 3600.0, 0.8), 0.8);
        let thaw = snow_cover(0, 0.0, 5.0, 900.0, 1.0);
        assert!(thaw > 0.4 && thaw < 0.6, "{thaw}");
        assert!(snow_cover(1, 0.5, 5.0, 900.0, 1.0) < thaw);
    }

    /// The picture is a tile (a metre of any pane, `pane_uv::TILE`): a milky layer with
    /// clear gaps, as much snow in the middle as at its edges, and its edges join: the step
    /// across the tile's seam is no bigger than the step between any two neighbours.
    #[test]
    fn snow_on_glass_is_an_even_tile() {
        let img = super::snow_on_glass(std::path::Path::new("/nonexistent"));
        assert_eq!((img.width, img.height), (512, 512));
        let alpha_of = |x: usize, y: usize| img.rgba[(y * 512 + x) * 4 + 3] as f32;
        let (mut middle, mut rim, mut n_middle, mut n_rim) = (0f32, 0f32, 0f32, 0f32);
        let (mut seam, mut inner) = (0f32, 0f32);
        for y in 0..512 {
            seam += (alpha_of(0, y) - alpha_of(511, y)).abs();
            for x in 0..512 {
                inner += (alpha_of(x, y) - alpha_of((x + 1) % 512, y)).abs() / 512.0;
                let d = ((x as f32 - 255.5).abs()).max((y as f32 - 255.5).abs()) / 255.5;
                if d < 0.35 {
                    middle += alpha_of(x, y);
                    n_middle += 1.0;
                } else if d > 0.8 {
                    rim += alpha_of(x, y);
                    n_rim += 1.0;
                }
            }
        }
        let mean = img.rgba.chunks_exact(4).map(|p| p[3] as f32).sum::<f32>() / (512.0 * 512.0);
        assert!(
            mean > 40.0 && mean < 190.0,
            "the glass is clear or opaque (mean alpha {mean})"
        );
        let (a, b) = (rim / n_rim, middle / n_middle);
        assert!(
            a < b * 1.3 && b < a * 1.3,
            "the tile is not even: rim {a:.1}, middle {b:.1}"
        );
        assert!(
            seam < inner * 2.0,
            "the seam shows: {seam:.0} against {inner:.0}"
        );
    }
}
