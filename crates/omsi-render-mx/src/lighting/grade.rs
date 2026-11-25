//! The Enhanced picture's colour grade: what a camera and an eye do to the light of the
//! moment beyond its exposure - how much of a sunset's red and a blue hour's blue they
//! discount (white balance), how much colour a grey day, a fog or a low sun leaves or gives
//! (saturation), what the shadows of a night keep of the sky (lift) and how a low sun's
//! highlights run warm. One table over the sun's height, bent by the weather; post.wgsl
//! `grade_scene` / `grade_display` apply it around the tone curve.
//!
//! `OMSI_TONEMAP=natural|agx|neutral` picks the tone curve (AgX by default), and
//! `OMSI_GRADE=0` leaves the grade neutral (for comparisons).

use crate::view::Lighting;
use glam::Vec3;

/// The grade of the moment (post.wgsl `PostParams.f/g/h`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColorGrade {
    /// Gains on the scene light (white balance), luminance-neutral.
    pub white: Vec3,
    /// Saturation about each pixel's luminance (1 = as it is).
    pub saturation: f32,
    /// What black is lifted to (display terms, 0..~0.03).
    pub lift: Vec3,
    /// The highlights' tint (display terms, applied towards white).
    pub highlights: Vec3,
}

impl ColorGrade {
    pub(crate) const NEUTRAL: ColorGrade = ColorGrade {
        white: Vec3::ONE,
        saturation: 1.0,
        lift: Vec3::ZERO,
        highlights: Vec3::ONE,
    };

    fn lerp(&self, o: &ColorGrade, t: f32) -> ColorGrade {
        ColorGrade {
            white: self.white.lerp(o.white, t),
            saturation: self.saturation + (o.saturation - self.saturation) * t,
            lift: self.lift.lerp(o.lift, t),
            highlights: self.highlights.lerp(o.highlights, t),
        }
    }

    /// The three vec4 the post uniform carries (f, g, h).
    pub(crate) fn uniform(&self, tone_curve: f32) -> [[f32; 4]; 3] {
        [
            [self.white.x, self.white.y, self.white.z, self.saturation],
            [self.lift.x, self.lift.y, self.lift.z, tone_curve],
            [self.highlights.x, self.highlights.y, self.highlights.z, 0.0],
        ]
    }
}

/// Keys of the grade over the sun's altitude (degrees). Between them it is blended.
fn key(white: [f32; 3], saturation: f32, lift: [f32; 3], highlights: [f32; 3]) -> ColorGrade {
    let w = Vec3::from(white);
    // (white balance gains are kept luminance-neutral: the exposure is the light model's)
    let l = 0.2126 * w.x + 0.7152 * w.y + 0.0722 * w.z;
    ColorGrade {
        white: w / l,
        saturation,
        lift: Vec3::from(lift),
        highlights: Vec3::from(highlights),
    }
}

fn by_altitude(alt: f32) -> ColorGrade {
    // (deg, grade): a clear day's neutral camera, a warm golden hour that keeps its colour,
    // a dusk whose pink and violet the eye takes most of out (the magenta that every lit wall
    // took at sunset), a blue hour a little cool, a night whose dark keeps the sky's blue
    let keys: [(f32, ColorGrade); 7] = [
        (
            -18.0,
            key(
                [0.96, 1.0, 1.06],
                0.82,
                [0.010, 0.015, 0.026],
                [1.0, 1.0, 1.0],
            ),
        ),
        (
            -9.0,
            key(
                [0.96, 1.0, 1.05],
                0.86,
                [0.008, 0.012, 0.022],
                [1.0, 1.0, 1.0],
            ),
        ),
        (
            -4.0,
            key(
                [0.97, 1.03, 1.02],
                0.92,
                [0.003, 0.005, 0.011],
                [1.0, 1.0, 0.99],
            ),
        ),
        (
            0.5,
            key(
                [1.02, 1.02, 0.94],
                1.10,
                [0.002, 0.002, 0.004],
                [1.05, 1.0, 0.90],
            ),
        ),
        (
            6.0,
            key(
                [1.06, 1.0, 0.90],
                1.18,
                [0.001, 0.001, 0.002],
                [1.06, 1.0, 0.90],
            ),
        ),
        (
            20.0,
            key([1.02, 1.0, 0.97], 1.13, [0.0, 0.0, 0.0], [1.02, 1.0, 0.97]),
        ),
        (
            45.0,
            key([1.0, 1.0, 1.0], 1.12, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        ),
    ];
    if alt <= keys[0].0 {
        return keys[0].1;
    }
    for w in keys.windows(2) {
        let (a0, g0) = w[0];
        let (a1, g1) = w[1];
        if alt <= a1 {
            let t = ((alt - a0) / (a1 - a0)).clamp(0.0, 1.0);
            // (smooth through the keys: no kink in a sunrise played at speed)
            let t = t * t * (3.0 - 2.0 * t);
            return g0.lerp(&g1, t);
        }
    }
    keys[keys.len() - 1].1
}

/// Tests that measure geometry by pixel values (a reflection's strength, a shadow's ratio)
/// render with the old natural curve and no grade (`neutral_for_tests`).
static NEUTRAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
pub(crate) struct NeutralGrade;

#[cfg(test)]
impl Drop for NeutralGrade {
    fn drop(&mut self) {
        NEUTRAL.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The natural curve and no grade until the guard is dropped.
#[cfg(test)]
pub(crate) fn neutral_for_tests() -> NeutralGrade {
    NEUTRAL.store(true, std::sync::atomic::Ordering::Relaxed);
    NeutralGrade
}

/// The grade for the light of the moment.
pub(crate) fn color_grade(l: &Lighting) -> ColorGrade {
    if !grade_on() || NEUTRAL.load(std::sync::atomic::Ordering::Relaxed) {
        return ColorGrade::NEUTRAL;
    }
    let alt = l
        .sun_dir
        .normalize_or_zero()
        .z
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
    let mut g = by_altitude(alt);
    let day = crate::atmosphere::smoothstep(-6.0, 4.0, alt);
    // a grey sky: colour goes out of everything, the light a little cooler, and a low sun's
    // warmth is behind the deck
    let oc = l.overcast.clamp(0.0, 1.0);
    let rain = l.rain.clamp(0.0, 1.0);
    let fog = crate::atmosphere::smoothstep(0.0015, 0.012, l.fog_density);
    let snow = l.snowfall.clamp(0.0, 1.0);
    let grey = (oc * 0.8 + rain * 0.4 + fog * 0.6).min(1.0);
    g.saturation *= 1.0 - 0.18 * grey * day - 0.08 * fog;
    let cool = Vec3::new(0.97, 1.0, 1.04);
    let warm_out = Vec3::ONE.lerp(cool, (oc * 0.6 + snow * 0.4).min(1.0) * day);
    g.white = (g.white.lerp(Vec3::ONE, grey * 0.6) * warm_out).max(Vec3::splat(0.5));
    let lw = 0.2126 * g.white.x + 0.7152 * g.white.y + 0.0722 * g.white.z;
    g.white /= lw;
    g.highlights = g.highlights.lerp(Vec3::ONE, grey);
    // a fog's or a downpour's grey veil lifts the dark a little more and evenly (the air's
    // own light), not towards blue
    let veil = (fog * 0.6 + rain * 0.25) * (0.4 + 0.6 * day);
    g.lift = g.lift * (1.0 - 0.5 * grey) + Vec3::splat(0.012 * veil);
    g
}

/// The tone curve's number for post.wgsl `tone_curve` (0 natural, 1 AgX, 2 PBR Neutral).
pub(crate) fn tone_curve_choice() -> f32 {
    if NEUTRAL.load(std::sync::atomic::Ordering::Relaxed) {
        return 0.0;
    }
    static C: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *C.get_or_init(
        || match std::env::var("OMSI_TONEMAP").ok().as_deref().map(str::trim) {
            Some("natural") | Some("0") => 0.0,
            Some("neutral") | Some("pbr") | Some("2") => 2.0,
            _ => 1.0,
        },
    )
}

fn grade_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("OMSI_GRADE").map_or(true, |v| v.trim() != "0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_balance_keeps_the_luminance() {
        for alt in [-20.0f32, -10.0, -5.0, -1.0, 0.0, 3.0, 10.0, 30.0, 60.0] {
            let g = by_altitude(alt);
            let l = 0.2126 * g.white.x + 0.7152 * g.white.y + 0.0722 * g.white.z;
            assert!((l - 1.0).abs() < 1e-3, "{alt}: {l}");
        }
    }

    #[test]
    fn a_low_sun_is_warm_and_a_night_cool() {
        let gold = by_altitude(5.0);
        assert!(gold.white.x > gold.white.z);
        let night = by_altitude(-15.0);
        assert!(night.white.z > night.white.x);
        assert!(night.lift.z > night.lift.x);
        assert!(by_altitude(40.0).lift.max_element() < 1e-3);
    }
}
