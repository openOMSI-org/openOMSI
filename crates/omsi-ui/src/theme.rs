//! The launcher's look: flat and dark (neutral greys, no tint), one amber accent used
//! sparingly - after the calm menus of Euro Truck Simulator 2.
//!
//! The game's screens read their colours from the palette in force ([`th`]): openOMSI's own,
//! dark ([`Palette::OPENOMSI`]) or light ([`Palette::LIGHT`]) as the player chose
//! ([`palette`]), and over it the colours of an interface theme the player chose
//! (`Themes/<id>/theme.json`, docs/THEMES.md: [`Palette::with_colors`]), put in force with
//! [`set`]. A theme may also make the panels frosted glass over the game's picture ([`Glass`]).

use std::cell::Cell;
use std::sync::RwLock;

use crate::Color;

/// Panels of frosted glass: the game's picture shows through them blurred, under a tint of
/// the panel's own colour, with a light edge and a sheen at the top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    /// How much of the panel's colour lies over the blurred picture (0 to 1).
    pub opacity: f32,
    /// How far the picture is blurred, in points (the radius of the blur).
    pub blur: f32,
    /// The opacity of the light edge round a panel (0 to 1).
    pub edge: f32,
    /// The opacity of the sheen at the top of a panel (0 to 1).
    pub highlight: f32,
}

impl Glass {
    pub const DEFAULT: Glass = Glass { opacity: 0.6, blur: 24.0, edge: 0.16, highlight: 0.10 };
}

/// The colours of the interface, and the corners of its panels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub accent: Color,
    pub accent_2: Color,
    pub danger: Color,
    pub ok: Color,
    pub warn: Color,
    pub text: Color,
    pub text_soft: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    /// Rail, panels, fields.
    pub rail: Color,
    pub panel: Color,
    pub field: Color,
    pub hover: Color,
    pub selected: Color,
    pub edge: Color,
    /// The desktop launcher's calm look (`launcher/ui/styles.css`), which the game's own
    /// screens over the picture follow (`Ui::flat`): graphite surfaces one step lighter each,
    /// no edges.
    pub bg: Color,
    pub sidebar: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub surface_3: Color,
    pub ink: Color,
    pub ink_2: Color,
    pub ink_muted: Color,
    pub accent_hover: Color,
    pub accent_ink: Color,
    pub danger_2: Color,
    /// The corners of a panel (points).
    pub radius: f32,
    /// Panels of frosted glass rather than solid ones.
    pub glass: Option<Glass>,
}

impl Palette {
    pub const OPENOMSI: Palette = Palette {
        accent: Color::rgba(232, 160, 48, 1.0),
        accent_2: Color::rgba(96, 160, 232, 1.0),
        danger: Color::rgba(222, 78, 68, 1.0),
        ok: Color::rgba(104, 190, 118, 1.0),
        warn: Color::rgba(232, 170, 70, 1.0),
        text: Color::rgba(236, 236, 236, 1.0),
        text_soft: Color::rgba(200, 200, 200, 1.0),
        text_dim: Color::rgba(142, 142, 142, 1.0),
        text_faint: Color::rgba(96, 96, 96, 1.0),
        rail: Color::rgba(18, 18, 18, 1.0),
        panel: Color::rgba(22, 22, 22, 1.0),
        field: Color::rgba(31, 31, 31, 1.0),
        hover: Color::rgba(38, 38, 38, 1.0),
        selected: Color::rgba(44, 44, 44, 1.0),
        edge: Color::rgba(255, 255, 255, 0.06),
        bg: Color::rgba(18, 18, 18, 1.0),
        sidebar: Color::rgba(23, 23, 23, 1.0),
        surface: Color::rgba(28, 28, 28, 1.0),
        surface_2: Color::rgba(37, 37, 37, 1.0),
        surface_3: Color::rgba(47, 47, 47, 1.0),
        ink: Color::rgba(237, 237, 237, 1.0),
        ink_2: Color::rgba(184, 184, 184, 1.0),
        ink_muted: Color::rgba(138, 138, 138, 1.0),
        accent_hover: Color::rgba(240, 173, 69, 1.0),
        accent_ink: Color::rgba(26, 18, 6, 1.0),
        danger_2: Color::rgba(229, 83, 75, 1.0),
        radius: 8.0,
        glass: None,
    };

    /// openOMSI's light look (the setting `ui_theme` "light").
    pub const LIGHT: Palette = Palette {
        accent: Color::rgba(185, 120, 14, 1.0),
        accent_2: Color::rgba(52, 116, 184, 1.0),
        danger: Color::rgba(188, 54, 48, 1.0),
        ok: Color::rgba(42, 132, 62, 1.0),
        warn: Color::rgba(180, 112, 18, 1.0),
        text: Color::rgba(32, 32, 32, 1.0),
        text_soft: Color::rgba(72, 72, 72, 1.0),
        text_dim: Color::rgba(104, 104, 104, 1.0),
        text_faint: Color::rgba(136, 136, 136, 1.0),
        rail: Color::rgba(239, 239, 239, 1.0),
        panel: Color::rgba(252, 252, 252, 1.0),
        field: Color::rgba(246, 246, 246, 1.0),
        hover: Color::rgba(234, 234, 234, 1.0),
        selected: Color::rgba(224, 224, 224, 1.0),
        edge: Color::rgba(0, 0, 0, 0.12),
        bg: Color::rgba(247, 247, 247, 1.0),
        sidebar: Color::rgba(239, 239, 239, 1.0),
        surface: Color::rgba(255, 255, 255, 1.0),
        surface_2: Color::rgba(242, 242, 242, 1.0),
        surface_3: Color::rgba(230, 230, 230, 1.0),
        ink: Color::rgba(28, 28, 28, 1.0),
        ink_2: Color::rgba(76, 76, 76, 1.0),
        ink_muted: Color::rgba(112, 112, 112, 1.0),
        accent_hover: Color::rgba(204, 137, 22, 1.0),
        accent_ink: Color::rgba(30, 20, 4, 1.0),
        danger_2: Color::rgba(203, 62, 55, 1.0),
        radius: 8.0,
        glass: None,
    };

    /// The colour of `name` (the field's name, as a theme file has it), to set it.
    pub fn color_mut(&mut self, name: &str) -> Option<&mut Color> {
        Some(match name {
            "accent" => &mut self.accent,
            "accent_2" => &mut self.accent_2,
            "danger" => &mut self.danger,
            "ok" => &mut self.ok,
            "warn" => &mut self.warn,
            "text" => &mut self.text,
            "text_soft" => &mut self.text_soft,
            "text_dim" => &mut self.text_dim,
            "text_faint" => &mut self.text_faint,
            "rail" => &mut self.rail,
            "panel" => &mut self.panel,
            "field" => &mut self.field,
            "hover" => &mut self.hover,
            "selected" => &mut self.selected,
            "edge" => &mut self.edge,
            "bg" | "background" => &mut self.bg,
            "sidebar" => &mut self.sidebar,
            "surface" => &mut self.surface,
            "surface_2" => &mut self.surface_2,
            "surface_3" => &mut self.surface_3,
            "ink" => &mut self.ink,
            "ink_2" => &mut self.ink_2,
            "ink_muted" => &mut self.ink_muted,
            "accent_hover" => &mut self.accent_hover,
            "accent_ink" => &mut self.accent_ink,
            "danger_2" => &mut self.danger_2,
            _ => return None,
        })
    }

    /// A palette with the colours `given` (by name) on `base` (openOMSI's dark or light look,
    /// [`palette`]); a colour the theme leaves out follows the one it goes with there (the
    /// panels' greys the surfaces', the texts the inks', the darker red the red), so that a
    /// theme of a few colours is whole. Names it does not know are returned.
    pub fn with_colors<'a>(base: Palette, given: impl IntoIterator<Item = (&'a str, Color)>) -> (Palette, Vec<&'a str>) {
        let mut p = base;
        let mut set: Vec<&str> = Vec::new();
        let mut unknown = Vec::new();
        for (name, c) in given {
            match p.color_mut(name) {
                Some(slot) => {
                    *slot = c;
                    set.push(if name == "background" { "bg" } else { name });
                }
                None => unknown.push(name),
            }
        }
        // (what follows what when it was not given itself)
        const FOLLOWS: [(&str, &str); 13] = [
            ("rail", "bg"),
            ("sidebar", "bg"),
            ("panel", "surface"),
            ("field", "surface_2"),
            ("hover", "surface_3"),
            ("selected", "surface_3"),
            ("text", "ink"),
            ("text_soft", "ink_2"),
            ("text_dim", "ink_muted"),
            ("text_faint", "ink_muted"),
            ("danger_2", "danger"),
            ("accent_hover", "accent"),
            ("warn", "accent"),
        ];
        for (to, from) in FOLLOWS {
            if !set.contains(&to) && set.contains(&from) {
                let mut c = *p.color_mut(from).unwrap();
                match to {
                    "accent_hover" => c = c.lighten(0.12),
                    "text_faint" => c = c.darken(0.3),
                    _ => {}
                }
                *p.color_mut(to).unwrap() = c;
            }
        }
        (p, unknown)
    }
}

/// openOMSI's own look, light or dark.
pub fn palette(light: bool) -> Palette {
    if light { Palette::LIGHT } else { Palette::OPENOMSI }
}

static PALETTE: RwLock<Palette> = RwLock::new(Palette::OPENOMSI);

thread_local! {
    /// A palette for this thread alone (`with_palette`: the tests of a theme).
    static LOCAL: Cell<Option<Palette>> = const { Cell::new(None) };
}

/// The palette in force.
pub fn th() -> Palette {
    LOCAL.with(|l| l.get()).unwrap_or_else(|| *PALETTE.read().unwrap_or_else(|e| e.into_inner()))
}

/// A corner of `r` as openOMSI draws it, moved by as much as the palette's `radius` differs from
/// openOMSI's: a theme's rounder (or squarer) corners for every surface, the openOMSI look's own
/// unchanged.
pub fn corner(r: f32) -> f32 {
    (r + th().radius - Palette::OPENOMSI.radius).max(0.0)
}

/// Puts `p` in force for every screen drawn after.
pub fn set(p: Palette) {
    *PALETTE.write().unwrap_or_else(|e| e.into_inner()) = p;
}

/// `f` with `p` in force on this thread only.
pub fn with_palette<R>(p: Palette, f: impl FnOnce() -> R) -> R {
    let before = LOCAL.with(|l| l.replace(Some(p)));
    let out = f();
    LOCAL.with(|l| l.set(before));
    out
}

/// Height of a control row.
pub const ROW: f32 = 36.0;
pub const GAP: f32 = 12.0;
/// Width of the left rail (points).
pub const RAIL_W: f32 = 236.0;

/// How strongly the interface's backgrounds are drawn for the opacity setting
/// (the game's `ui_opacity`): 1 at its default of 85 %, as designed; lower, the picture shows
/// through them (never less than 0.3), higher a little darker. The texts stay solid.
pub fn backdrop(opacity: f32) -> f32 {
    (opacity / 0.85).clamp(0.3, 1.3)
}

/// A colour of a theme file: `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().strip_prefix('#')?;
    if !h.is_ascii() {
        return None;
    }
    let digit = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
    let (r, g, b, a) = match h.len() {
        3 => (digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17, 255),
        6 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, 255),
        8 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, digit(6, 2)?),
        _ => return None,
    };
    Some(Color::rgba(r, g, b, a as f32 / 255.0))
}

/// A colour as the 8-bit RGBA the game's own overlays take.
pub fn rgba8(c: Color) -> [u8; 4] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [q(c.0[0]), q(c.0[1]), q(c.0[2]), q(c.0[3])]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The opacity's default is the design; below it the backgrounds fade, never quite away.
    #[test]
    fn backgrounds_follow_the_opacity_setting() {
        assert_eq!(backdrop(0.85), 1.0);
        assert!((backdrop(0.425) - 0.5).abs() < 1e-6);
        assert_eq!(backdrop(0.2), 0.3);
        assert!(backdrop(1.0) > 1.0);
    }

    #[test]
    fn a_theme_colour_is_read_from_hex() {
        assert_eq!(parse_hex("#f58620"), Some(Color::rgba(0xf5, 0x86, 0x20, 1.0)));
        assert_eq!(parse_hex(" #fff "), Some(Color::rgba(255, 255, 255, 1.0)));
        assert_eq!(parse_hex("#0a142e80"), Some(Color::rgba(10, 20, 46, 128.0 / 255.0)));
        for bad in ["f58620", "#f5862", "#ggg", "", "#", "#ééé"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
        assert_eq!(rgba8(Color::rgba(245, 134, 32, 1.0)), [245, 134, 32, 255]);
    }

    /// A theme of a few colours is whole: what it leaves out follows what it gives, the rest
    /// stays openOMSI's.
    #[test]
    fn a_colour_left_out_follows_the_one_it_goes_with() {
        let navy = Color::rgba(10, 20, 46, 1.0);
        let orange = Color::rgba(245, 134, 32, 1.0);
        let (p, unknown) = Palette::with_colors(Palette::OPENOMSI, [("background", navy), ("accent", orange), ("ink", Color::WHITE), ("sparkle", navy)]);
        assert_eq!(unknown, ["sparkle"]);
        assert_eq!((p.bg, p.rail, p.sidebar), (navy, navy, navy));
        assert_eq!((p.accent, p.warn), (orange, orange));
        assert_ne!(p.accent_hover, orange, "a lighter orange under the mouse");
        assert_eq!(p.text, Color::WHITE);
        assert_eq!(p.surface, Palette::OPENOMSI.surface);
        // (given itself, a colour stays as given)
        let (p, _) = Palette::with_colors(Palette::OPENOMSI, [("background", navy), ("sidebar", orange)]);
        assert_eq!((p.rail, p.sidebar), (navy, orange));
    }

    /// The light look is a palette of its own, with openOMSI's corners and no glass; a theme's
    /// colours go over the look chosen, what it leaves out stays that look's.
    #[test]
    fn a_theme_goes_over_the_light_or_the_dark_look() {
        assert_eq!(palette(false), Palette::OPENOMSI);
        assert_eq!(palette(true), Palette::LIGHT);
        assert_ne!(Palette::LIGHT.bg, Palette::OPENOMSI.bg);
        assert_ne!(Palette::LIGHT.text, Palette::OPENOMSI.text);
        assert_eq!((Palette::LIGHT.radius, Palette::LIGHT.glass), (Palette::OPENOMSI.radius, None));
        let orange = Color::rgba(245, 134, 32, 1.0);
        let (p, _) = Palette::with_colors(palette(true), [("accent", orange)]);
        assert_eq!((p.accent, p.warn), (orange, orange));
        assert_eq!((p.bg, p.text, p.surface), (Palette::LIGHT.bg, Palette::LIGHT.text, Palette::LIGHT.surface));
    }

    /// A palette set for one thread is that thread's: the others keep the one in force.
    #[test]
    fn a_palette_for_a_thread_stays_on_it() {
        let red = Palette { accent: Color::rgba(255, 0, 0, 1.0), ..Palette::OPENOMSI };
        with_palette(red, || {
            assert_eq!(th().accent, red.accent);
            let other = std::thread::spawn(|| th().accent).join().unwrap();
            assert_eq!(other, Palette::OPENOMSI.accent);
        });
        assert_eq!(th().accent, Palette::OPENOMSI.accent);
    }
}
