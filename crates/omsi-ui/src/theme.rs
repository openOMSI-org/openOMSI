//! The launcher's look: flat and dark (neutral greys, no tint), one amber accent used
//! sparingly - after the calm menus of Euro Truck Simulator 2.

use crate::Color;

pub const ACCENT: Color = Color::rgba(232, 160, 48, 1.0);
pub const ACCENT_2: Color = Color::rgba(96, 160, 232, 1.0);
pub const DANGER: Color = Color::rgba(222, 78, 68, 1.0);
pub const OK: Color = Color::rgba(104, 190, 118, 1.0);
pub const WARN: Color = Color::rgba(232, 170, 70, 1.0);

pub const TEXT: Color = Color::rgba(236, 236, 236, 1.0);
pub const TEXT_SOFT: Color = Color::rgba(200, 200, 200, 1.0);
pub const TEXT_DIM: Color = Color::rgba(142, 142, 142, 1.0);
pub const TEXT_FAINT: Color = Color::rgba(96, 96, 96, 1.0);

/// Rail, panels, fields.
pub const RAIL: Color = Color::rgba(18, 18, 18, 1.0);
pub const PANEL: Color = Color::rgba(22, 22, 22, 1.0);
pub const FIELD: Color = Color::rgba(31, 31, 31, 1.0);
pub const HOVER: Color = Color::rgba(38, 38, 38, 1.0);
pub const SELECTED: Color = Color::rgba(44, 44, 44, 1.0);
pub const EDGE: Color = Color::rgba(255, 255, 255, 0.06);

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

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(non_snake_case)]
pub struct Theme {
    pub ACCENT: Color,
    pub ACCENT_2: Color,
    pub DANGER: Color,
    pub OK: Color,
    pub WARN: Color,
    pub TEXT: Color,
    pub TEXT_SOFT: Color,
    pub TEXT_DIM: Color,
    pub TEXT_FAINT: Color,
    pub RAIL: Color,
    pub PANEL: Color,
    pub FIELD: Color,
    pub HOVER: Color,
    pub SELECTED: Color,
    pub EDGE: Color,
    pub BG: Color,
    pub SIDEBAR: Color,
    pub SURFACE: Color,
    pub SURFACE_2: Color,
    pub SURFACE_3: Color,
    pub INK: Color,
    pub INK_2: Color,
    pub INK_MUTED: Color,
    pub ACCENT_HOVER: Color,
    pub ACCENT_INK: Color,
    pub DANGER_2: Color,
}

pub const DARK: Theme = Theme {
    ACCENT,
    ACCENT_2,
    DANGER,
    OK,
    WARN,
    TEXT,
    TEXT_SOFT,
    TEXT_DIM,
    TEXT_FAINT,
    RAIL,
    PANEL,
    FIELD,
    HOVER,
    SELECTED,
    EDGE,
    BG,
    SIDEBAR,
    SURFACE,
    SURFACE_2,
    SURFACE_3,
    INK,
    INK_2,
    INK_MUTED,
    ACCENT_HOVER,
    ACCENT_INK,
    DANGER_2,
};

pub const LIGHT: Theme = Theme {
    ACCENT: Color::rgba(185, 120, 14, 1.0),
    ACCENT_2: Color::rgba(52, 116, 184, 1.0),
    DANGER: Color::rgba(188, 54, 48, 1.0),
    OK: Color::rgba(42, 132, 62, 1.0),
    WARN: Color::rgba(180, 112, 18, 1.0),
    TEXT: Color::rgba(32, 32, 32, 1.0),
    TEXT_SOFT: Color::rgba(72, 72, 72, 1.0),
    TEXT_DIM: Color::rgba(104, 104, 104, 1.0),
    TEXT_FAINT: Color::rgba(136, 136, 136, 1.0),
    RAIL: Color::rgba(239, 239, 239, 1.0),
    PANEL: Color::rgba(252, 252, 252, 1.0),
    FIELD: Color::rgba(246, 246, 246, 1.0),
    HOVER: Color::rgba(234, 234, 234, 1.0),
    SELECTED: Color::rgba(224, 224, 224, 1.0),
    EDGE: Color::rgba(0, 0, 0, 0.12),
    BG: Color::rgba(247, 247, 247, 1.0),
    SIDEBAR: Color::rgba(239, 239, 239, 1.0),
    SURFACE: Color::rgba(255, 255, 255, 1.0),
    SURFACE_2: Color::rgba(242, 242, 242, 1.0),
    SURFACE_3: Color::rgba(230, 230, 230, 1.0),
    INK: Color::rgba(28, 28, 28, 1.0),
    INK_2: Color::rgba(76, 76, 76, 1.0),
    INK_MUTED: Color::rgba(112, 112, 112, 1.0),
    ACCENT_HOVER: Color::rgba(204, 137, 22, 1.0),
    ACCENT_INK: Color::rgba(30, 20, 4, 1.0),
    DANGER_2: Color::rgba(203, 62, 55, 1.0),
};

pub fn palette(light: bool) -> Theme {
    if light { LIGHT } else { DARK }
}

pub const RADIUS: f32 = 8.0;
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
    fn dark_and_light_palettes_are_distinct_but_share_opacity_rules() {
        let dark = palette(false);
        let light = palette(true);

        assert_eq!(dark, DARK);
        assert_eq!(light, LIGHT);
        assert_ne!(dark, light);
        assert_ne!(dark.BG, light.BG);
        assert_ne!(dark.TEXT, light.TEXT);
        assert_eq!(backdrop(0.425), 0.5);
        assert_eq!(backdrop(0.85), 1.0);
    }
}
