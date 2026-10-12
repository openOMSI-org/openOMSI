//! The launcher's look, after Omsi-Hub's: the map is the ground - night blue, edge to edge -
//! and the choices float on it as dark sheets. Colour has three jobs and no others: the
//! accent (the player's choice, the logo's orange unless they chose another - see
//! `crate::accent`) is the route and the one action that goes on, yellow is the line number
//! (a printed plate), and red, green and blue are punctuality. Everything else is ground,
//! sheet and ink.
//!
//! The light look (the setting "dark mode" off, as the launcher starts) has no colours of its
//! own in the pages: every page is drawn in the dark look's colours below, and at the frame's
//! end each is carried over to the light look's (`light`, `Lighten`, `Ui::finish`) - so the
//! bus company, the editors, the settings and the dialogs follow the mode without a line of
//! their own. What is drawn for both looks already (the start's glass, the ground) says so
//! (`Ui::as_drawn`) and keeps its colours.

use hashbrown::HashMap;
use omsi_ui::Color;

/// The accent: the route on the map, the step you are on, the chosen row and the main action.
pub fn accent() -> Color {
    crate::accent::base()
}
/// The main action under the mouse; nowhere else.
pub fn accent_deep() -> Color {
    crate::accent::shades().deep
}
/// The line number's plate, and its ink.
pub const LINE: Color = Color::rgba(255, 210, 63, 1.0);
pub const ON_LINE: Color = Color::rgba(26, 20, 0, 1.0);
/// Punctuality: late, on time, early (blue, whatever the accent: it is a meaning).
pub const LATE: Color = Color::rgba(217, 58, 48, 1.0);
pub const ON_TIME: Color = Color::rgba(26, 138, 79, 1.0);
pub const EARLY: Color = Color::rgba(42, 117, 247, 1.0);
/// Early as text on a dark sheet.
pub const EARLY_SOFT: Color = Color::rgba(122, 168, 255, 1.0);

/// Text on the accent: white, or dark ink on a light one.
pub fn on_accent() -> Color {
    crate::accent::shades().on
}
/// The accent as text or a mark on a dark sheet: lighter.
pub fn accent_2() -> Color {
    crate::accent::shades().soft
}
pub const DANGER: Color = LATE;
/// (lighter than `ON_TIME`: it is read as text on a dark sheet)
pub const OK: Color = Color::rgba(52, 178, 110, 1.0);
pub const WARN: Color = Color::rgba(232, 170, 70, 1.0);

/// The sheet's ink, and its one quiet grade (with a softer and a fainter step between).
pub const TEXT: Color = Color::rgba(232, 235, 242, 1.0);
pub const TEXT_SOFT: Color = Color::rgba(196, 202, 214, 1.0);
pub const TEXT_DIM: Color = Color::rgba(149, 157, 176, 1.0);
pub const TEXT_FAINT: Color = Color::rgba(96, 104, 122, 1.0);

/// The ground (the map when there is none to show), the sheets on it, and the fields in them.
pub const GROUND: Color = Color::rgba(9, 12, 24, 1.0);
pub const RAIL: Color = GROUND;
pub const PANEL: Color = Color::rgba(20, 26, 38, 1.0);
pub const FIELD: Color = Color::rgba(27, 34, 49, 1.0);
pub const HOVER: Color = Color::rgba(33, 40, 54, 1.0);
/// A row chosen in a list (the pause menu's rail, `shell::pause`).
pub const SELECTED: Color = Color::rgba(38, 46, 62, 1.0);
/// A sheet's edge and the hairlines inside it.
pub const EDGE: Color = Color::rgba(255, 255, 255, 0.08);
pub const HAIRLINE: Color = Color::rgba(255, 255, 255, 0.09);
/// What lies on the map without being a sheet (its buttons): dark, a little see-through.
pub const ON_MAP: Color = Color::rgba(18, 26, 38, 0.86);

/// The desktop launcher's calm look (`launcher/ui/styles.css`), which the game's own screens
/// over the picture follow (`Ui::flat`): graphite surfaces one step lighter each, no edges.
pub const BG: Color = Color::rgba(18, 18, 18, 1.0);
pub const SIDEBAR: Color = Color::rgba(23, 23, 23, 1.0);
pub const SURFACE: Color = Color::rgba(28, 28, 28, 1.0);
pub const SURFACE_2: Color = Color::rgba(37, 37, 37, 1.0);
pub const SURFACE_3: Color = Color::rgba(47, 47, 47, 1.0);
pub const INK: Color = Color::rgba(237, 237, 237, 1.0);
pub const INK_2: Color = Color::rgba(184, 184, 184, 1.0);
pub const INK_MUTED: Color = Color::rgba(138, 138, 138, 1.0);
pub const ACCENT_HOVER: Color = Color::rgba(240, 173, 69, 1.0);
pub const ACCENT_INK: Color = Color::rgba(26, 18, 6, 1.0);
pub const DANGER_2: Color = Color::rgba(229, 83, 75, 1.0);

/// A control inside a sheet; a sheet; the main action.
pub const RADIUS: f32 = 10.0;
pub const SHEET_RADIUS: f32 = 14.0;
pub const ACTION_RADIUS: f32 = 12.0;
/// Height of a control row.
pub const ROW: f32 = 36.0;
pub const GAP: f32 = 12.0;

// --- the ground ------------------------------------------------------------------------------

/// The dark look's ground under the launcher's pages: a deep night blue.
pub const NIGHT: Color = Color::rgba(10, 20, 46, 1.0);

/// The launcher's ground for the accent `rgb`, and the route drawn on it: in the light look a
/// light ground under the route in the colour itself (the default orange on Luc's cream), in
/// the dark one the night blue with the route a little sunk into it.
pub fn ground_of(rgb: u32, dark: bool) -> (Color, Color) {
    let [r, g, b] = crate::accent::unpack(rgb);
    if dark {
        return (NIGHT, Color::rgba(r, g, b, 1.0).mix(NIGHT, 0.25));
    }
    if rgb == crate::accent::DEFAULT {
        return (Color::rgba(252, 234, 208, 1.0), Color::rgba(247, 138, 18, 1.0));
    }
    let light = |v: u8| (v as f32 + (255.0 - v as f32) * 0.55).round() as u8;
    (Color::rgba(light(r), light(g), light(b), 1.0), Color::rgba(r, g, b, 1.0))
}

/// What the light look's pages stand on where they fill the window (the bus company): the
/// launcher's light ground, lifted towards white as far as the quiet grade of the ink needs to
/// read on it (a blue or a red accent's ground is too deep for small words).
pub fn light_ground(rgb: u32) -> Color {
    let base = ground_of(rgb, false).0;
    let mut t = 0.0;
    let mut c = base;
    while contrast(rgb255(light::TEXT_DIM), rgb255(c)) < 4.6 && t < 1.0 {
        t += 0.02;
        c = base.mix(Color::WHITE, t);
    }
    c
}

/// The ground as the mode has it now: what a new page fades in from (`transition`).
pub fn ground() -> Color {
    if crate::accent::dark() {
        GROUND
    } else {
        light_ground(crate::accent::chosen())
    }
}

// --- the light look --------------------------------------------------------------------------

/// The light look's colours (Luc: "There must also be a dark and a light mode"): the cream
/// and white of the launcher's light glass under its dark words, each the counterpart of the
/// dark look's colour of the same name (the ground is the launcher's own, `light_ground`).
/// Words keep WCAG AA (4.5:1) on every sheet, field and row, as the dark look's do; the faint
/// grade is for what may be missed, as there (3:1).
pub mod light {
    use omsi_ui::Color;

    pub const PANEL: Color = Color::rgba(255, 254, 252, 1.0);
    pub const FIELD: Color = Color::rgba(245, 242, 237, 1.0);
    pub const HOVER: Color = Color::rgba(237, 233, 227, 1.0);
    pub const SELECTED: Color = Color::rgba(229, 224, 217, 1.0);
    /// The ink, as the light glass's (`flow::glass_look`).
    pub const TEXT: Color = Color::rgba(30, 28, 34, 1.0);
    pub const TEXT_SOFT: Color = Color::rgba(70, 66, 72, 1.0);
    pub const TEXT_DIM: Color = Color::rgba(100, 95, 98, 1.0);
    pub const TEXT_FAINT: Color = Color::rgba(134, 128, 126, 1.0);
    /// A see-through white of the dark look (the light under the mouse, an edge, a hairline)
    /// is this ink, see-through: on the light sheet it lies darker, as the white lay lighter.
    pub const INK: Color = Color::rgba(22, 20, 26, 1.0);
}

/// A colour's red, green and blue in 0..255.
fn rgb255(c: Color) -> [f32; 3] {
    [c.0[0] * 255.0, c.0[1] * 255.0, c.0[2] * 255.0]
}

/// WCAG relative luminance of red, green and blue in 0..255.
fn luminance(c: [f32; 3]) -> f32 {
    let lin = |v: f32| {
        let s = (v / 255.0).clamp(0.0, 1.0);
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG contrast ratio (1 to 21).
pub fn contrast(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn mean(c: [f32; 3]) -> f32 {
    (c[0] + c[1] + c[2]) / 3.0
}

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The dark look's greys by how light they are (the mean of their red, green and blue), each
/// with its light counterpart: a grey between two of them goes in proportion. Under the
/// sheets the order turns over - a field, a row under the mouse and a chosen one lie lighter
/// than the sheet on the night, darker than it on the cream - and above them the ink goes from
/// the faint grade to the full, and on to white, whose counterpart is the ink itself.
fn greys(ground: [f32; 3]) -> [(f32, [f32; 3]); 10] {
    let pair = |dark: Color, light: Color| (mean(rgb255(dark)), rgb255(light));
    [
        (mean(rgb255(GROUND)), ground),
        pair(PANEL, light::PANEL),
        pair(FIELD, light::FIELD),
        pair(HOVER, light::HOVER),
        pair(SELECTED, light::SELECTED),
        pair(TEXT_FAINT, light::TEXT_FAINT),
        pair(TEXT_DIM, light::TEXT_DIM),
        pair(TEXT_SOFT, light::TEXT_SOFT),
        pair(TEXT, light::TEXT),
        (255.0, rgb255(light::INK)),
    ]
}

/// The light counterpart of a dark-look grey `m` light (`greys`).
fn grey_at(points: &[(f32, [f32; 3])], m: f32) -> [f32; 3] {
    if m <= points[0].0 {
        return points[0].1;
    }
    for w in points.windows(2) {
        let ((m0, a), (m1, b)) = (w[0], w[1]);
        if m <= m1 {
            let t = (m - m0) / (m1 - m0).max(1e-3);
            return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        }
    }
    points[points.len() - 1].1
}

/// The tiles' sheen (`ui::LIGHT`): a light, which stays one.
const SHEEN: [f32; 3] = [214.0, 228.0, 255.0];
/// How strongly a see-through black (a sheet's shadow) lies on the light ground: in linear
/// light it showed little on the night and a lot on the cream.
const SHADOW: f32 = 0.45;

/// How a see-through white of the dark look lies as see-through ink on the light one: the
/// step it made from the dark sheet (large, blended in linear light) about halved.
fn overlay_alpha(a: f32) -> f32 {
    (0.85 * a.max(0.0).sqrt()).min(0.6)
}

/// Whether `c` (its triangle's most opaque corner `strongest`) is a scrim the light look keeps
/// dark (`to_light`) and strong enough to write light words on: the ground see-through, or
/// black seen as black. (The words drawn on a small one keep their colours: `Ui::to_light`.)
pub fn is_scrim(c: [f32; 4], strongest: f32) -> bool {
    let rgb = [c[0] * 255.0, c[1] * 255.0, c[2] * 255.0];
    let hi = rgb[0].max(rgb[1]).max(rgb[2]);
    if hi <= 6.0 {
        return strongest >= 0.6;
    }
    mean(rgb) <= 20.0 && (0.5..0.98).contains(&strongest)
}

/// The colour `c` (sRGB, straight alpha) of something drawn for the dark look, as the light
/// look draws it. `words`: it is a word's or an icon's (the atlas's sprites); `strongest`: the
/// most opaque corner of its triangle (a fade from white to nothing is one white thing).
///
/// - The dark look's greys (a little blue, as all of them are) go over to the light ones by
///   how light they are (`greys`): the ground to the cream, the sheets to white, the ink to
///   dark ink.
/// - White seen as white (a knob, words on the accent or on a photo) stays; a see-through
///   white - the dark look's way of lifting a surface - is see-through ink.
/// - Black seen as black stays; a see-through one is a shadow, lighter here.
/// - The ground see-through is a scrim (under words on a photo, over a dimmed page): it
///   stays, as does what is darker than the ground (a display, a tyre in a drawing).
/// - A colour stays: the accent, a meaning's red, green, amber and blue, the line's yellow.
///   Words and icons in one are made dark enough to read on the light sheet (4.5:1); dark
///   words lie on a light fill (the accent's ink) and stay.
pub fn to_light(c: [f32; 4], words: bool, strongest: f32, ground: [f32; 3]) -> [f32; 4] {
    let rgb = [c[0] * 255.0, c[1] * 255.0, c[2] * 255.0];
    let (hi, lo) = (rgb[0].max(rgb[1]).max(rgb[2]), rgb[0].min(rgb[1]).min(rgb[2]));
    let m = mean(rgb);
    if (0..3).all(|i| (rgb[i] - SHEEN[i]).abs() < 1.0) {
        return c;
    }
    if lo >= 246.0 {
        if strongest >= 0.5 {
            return c;
        }
        let ink = light::INK.0;
        return [ink[0], ink[1], ink[2], overlay_alpha(c[3])];
    }
    if hi <= 6.0 {
        return if strongest >= 0.6 { c } else { [c[0], c[1], c[2], c[3] * SHADOW] };
    }
    if m <= 20.0 && strongest < 0.98 {
        return c;
    }
    if words && m < 60.0 {
        return c;
    }
    // (how much of a dark-look grey it is: grey, a little blue, no darker than the ground)
    let grey = 1.0 - smooth(38.0, 56.0, hi - lo);
    let cool = smooth(1.0, 5.0, rgb[2] - rgb[0]);
    let deep = smooth(11.0, 13.5, m);
    let w = grey * cool * deep;
    let n = grey_at(&greys(ground), m);
    let mut out = [rgb[0] + (n[0] - rgb[0]) * w, rgb[1] + (n[1] - rgb[1]) * w, rgb[2] + (n[2] - rgb[2]) * w];
    if words && w < 0.5 {
        let sheet = rgb255(light::PANEL);
        let mut k = 0;
        while contrast(out, sheet) < 4.5 && k < 60 {
            out = [out[0] * 0.96, out[1] * 0.96, out[2] * 0.96];
            k += 1;
        }
    }
    [out[0] / 255.0, out[1] / 255.0, out[2] / 255.0, c[3]]
}

/// The light look of a frame's colours (`Ui::finish`): `to_light` for each colour, worked out
/// once and remembered.
pub struct Lighten {
    ground: [f32; 3],
    known: HashMap<([u32; 4], u8), [f32; 4]>,
}

impl Default for Lighten {
    fn default() -> Self {
        Lighten::new()
    }
}

impl Lighten {
    pub fn new() -> Lighten {
        Lighten { ground: rgb255(light_ground(crate::accent::DEFAULT)), known: HashMap::new() }
    }

    /// The light ground (`light_ground`) the dark ground becomes: what was worked out for
    /// another is forgotten.
    pub fn set_ground(&mut self, g: Color) {
        let g = rgb255(g);
        if g != self.ground {
            self.ground = g;
            self.known.clear();
        }
    }

    /// `to_light`, remembered.
    pub fn colour(&mut self, c: [f32; 4], words: bool, strongest: f32) -> [f32; 4] {
        // (what `to_light` asks of `strongest`)
        let class = if strongest < 0.5 { 0 } else if strongest < 0.6 { 1 } else if strongest < 0.98 { 2 } else { 3 };
        let key = (c.map(f32::to_bits), class << 1 | words as u8);
        if let Some(v) = self.known.get(&key) {
            return *v;
        }
        let v = to_light(c, words, strongest, self.ground);
        if self.known.len() > 16_384 {
            self.known.clear();
        }
        self.known.insert(key, v);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(c: [f32; 4]) -> [f32; 3] {
        [c[0] * 255.0, c[1] * 255.0, c[2] * 255.0]
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1.0)
    }

    #[test]
    fn the_light_palette_reads_as_the_dark_one_does() {
        let surfaces = [light::PANEL, light::FIELD, light::HOVER, light::SELECTED];
        for ink in [light::TEXT, light::TEXT_SOFT, light::TEXT_DIM] {
            for s in surfaces {
                let k = contrast(rgb255(ink), rgb255(s));
                assert!(k >= 4.5, "{ink:?} on {s:?}: {k:.2}");
            }
            // (and on the ground of every accent offered)
            for (name, a) in crate::accent::PRESETS {
                let k = contrast(rgb255(ink), rgb255(light_ground(a)));
                assert!(k >= 4.5, "{ink:?} on {name}'s ground: {k:.2}");
            }
        }
        for s in [light::PANEL, light::FIELD, light::HOVER] {
            assert!(contrast(rgb255(light::TEXT_FAINT), rgb255(s)) >= 3.0);
        }
        // the grades go darker in order, the surfaces lighter
        let l = |c: Color| luminance(rgb255(c));
        assert!(l(light::TEXT) < l(light::TEXT_SOFT) && l(light::TEXT_SOFT) < l(light::TEXT_DIM) && l(light::TEXT_DIM) < l(light::TEXT_FAINT));
        assert!(l(light::SELECTED) < l(light::HOVER) && l(light::HOVER) < l(light::FIELD) && l(light::FIELD) < l(light::PANEL));
        // the orange's ground is the launcher's cream itself
        assert_eq!(light_ground(crate::accent::DEFAULT), ground_of(crate::accent::DEFAULT, false).0);
    }

    #[test]
    fn every_dark_colour_becomes_its_light_counterpart() {
        let g = rgb255(light_ground(crate::accent::DEFAULT));
        let pairs = [
            (GROUND, light_ground(crate::accent::DEFAULT)),
            (PANEL, light::PANEL),
            (FIELD, light::FIELD),
            (HOVER, light::HOVER),
            (SELECTED, light::SELECTED),
            (TEXT, light::TEXT),
            (TEXT_SOFT, light::TEXT_SOFT),
            (TEXT_DIM, light::TEXT_DIM),
            (TEXT_FAINT, light::TEXT_FAINT),
        ];
        for (dark, want) in pairs {
            for words in [false, true] {
                // (words in a surface's colour would be ink on a light fill: they stay)
                if words && mean(rgb255(dark)) < 60.0 {
                    assert_eq!(to_light(dark.0, true, 1.0, g), dark.0);
                    continue;
                }
                let got = to_light(dark.0, words, 1.0, g);
                assert!(near(rgb(got), rgb255(want)), "{dark:?} -> {got:?}, not {want:?}");
            }
        }
        // see-through, as it was
        assert_eq!(to_light(PANEL.alpha(0.5).0, false, 0.5, g)[3], 0.5);
        // a mix of two greys lies between their counterparts
        let mid = rgb(to_light(PANEL.mix(HOVER, 0.5).0, false, 1.0, g));
        assert!(mid[0] < 255.0 && mid[0] > rgb255(light::HOVER)[0]);
    }

    #[test]
    fn white_black_and_colours_keep_what_they_mean() {
        let g = rgb255(light_ground(crate::accent::DEFAULT));
        // a see-through white (an edge, the light under the mouse) is see-through ink
        let edge = to_light(EDGE.0, false, EDGE.0[3], g);
        assert!(near(rgb(edge), rgb255(light::INK)) && edge[3] > EDGE.0[3] && edge[3] < 0.6);
        // white seen as white stays: words on the accent, a switch's knob
        assert_eq!(to_light(Color::WHITE.0, true, 1.0, g), Color::WHITE.0);
        let knob = Color::rgba(240, 240, 240, 1.0);
        assert_eq!(to_light(knob.0, false, 1.0, g), knob.0);
        // a fade from white to nothing stays white all the way (its strongest corner is white)
        assert_eq!(to_light(Color::WHITE.alpha(0.0).0, false, 1.0, g), Color::WHITE.alpha(0.0).0);
        // a shadow is lighter, a black thing black
        let shadow = Color::rgba(0, 0, 0, 0.45);
        assert!((to_light(shadow.0, false, 0.45, g)[3] - 0.45 * SHADOW).abs() < 1e-6);
        assert_eq!(to_light(Color::BLACK.0, false, 1.0, g), Color::BLACK.0);
        // a scrim under words on a photo stays dark
        let scrim = Color::rgba(9, 12, 24, 0.85);
        assert_eq!(to_light(scrim.0, false, 0.85, g), scrim.0);
        // the accent, the meanings and the line's plate stay
        for c in [crate::accent::base(), LATE, ON_TIME, EARLY, OK, WARN, LINE, crate::accent::shades().soft] {
            assert_eq!(to_light(c.0, false, 1.0, g), c.0, "{c:?}");
        }
        // the accent's dark ink on it stays dark, as words and as a mark
        let ink = crate::accent::shades().on;
        assert_eq!(to_light(ink.0, true, 1.0, g), ink.0);
        assert_eq!(to_light(ink.0, false, 1.0, g), ink.0);
        assert_eq!(to_light(ON_LINE.0, true, 1.0, g), ON_LINE.0);
        // words in a colour made for the dark sheet read on the light one
        for c in [OK, WARN, EARLY_SOFT, crate::accent::shades().soft, crate::accent::base()] {
            let w = to_light(c.0, true, 1.0, g);
            let k = contrast(rgb(w), rgb255(light::PANEL));
            assert!(k >= 4.5, "{c:?} -> {w:?}: {k:.2}");
            // (darker, the same hue)
            let (a, b) = (rgb255(c), rgb(w));
            assert!(b[0] <= a[0] && b[1] <= a[1] && b[2] <= a[2]);
        }
        // the tiles' sheen stays a light
        assert_eq!(to_light(Color::rgba(214, 228, 255, 0.1).0, false, 0.1, g), Color::rgba(214, 228, 255, 0.1).0);
    }

    #[test]
    fn a_light_ground_follows_the_accent() {
        let mut l = Lighten::new();
        let a = l.colour(GROUND.0, false, 1.0);
        l.set_ground(light_ground(0x2A75F7));
        let b = l.colour(GROUND.0, false, 1.0);
        assert_ne!(a, b);
        assert!(near(rgb(b), rgb255(light_ground(0x2A75F7))));
        // (remembered: the same answer again)
        assert_eq!(l.colour(GROUND.0, false, 1.0), b);
    }
}
