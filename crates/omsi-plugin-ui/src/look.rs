//! The look of the plugins' panels and notifications, from the palette in force
//! (`omsi_ui::theme::th`): openOMSI's dark look as it always was, its light look or an
//! interface theme's from their colours.

use omsi_ui::theme::Palette;
use omsi_ui::Color;

/// openOMSI's dark look: the card under a notification or a panel, its near black.
const CARD: Color = Color::rgba(14, 16, 20, 225.0 / 255.0);
pub(crate) const INK: Color = Color::rgba(236, 236, 236, 1.0);
pub(crate) const DARK_INK: Color = Color::rgba(18, 14, 8, 1.0);
// (solid greys, the menus' own: the pipeline blends in linear light, where a tenth of white
// over the dark card came out a light grey)
const TRACK: Color = Color::rgba(62, 62, 62, 1.0);
const DIVIDER: Color = Color::rgba(52, 52, 52, 1.0);
const BUTTON: Color = Color::rgba(44, 44, 44, 1.0);
const TILE: Color = Color::rgba(46, 48, 54, 0.6);
pub(crate) const SHADOW: Color = Color::rgba(0, 0, 0, 0.35);

/// The colours a card is laid out in.
#[derive(Clone, Copy)]
pub(crate) struct ThemeColors {
    pub card: Color,
    pub ink: Color,
    pub dark_ink: Color,
    pub accent: Color,
    pub track: Color,
    pub divider: Color,
    pub button: Color,
    pub tile: Color,
    pub muted: Color,
    pub soft: Color,
    pub faint: Color,
    pub rail_done: Color,
    pub tag_fill: Color,
}

impl ThemeColors {
    /// The colours of the palette `p`: openOMSI's dark look its own greys, any other (the
    /// light look, a theme's) its surfaces and inks.
    pub fn of(p: Palette) -> Self {
        if p == Palette::OPENOMSI {
            return Self {
                card: CARD,
                ink: INK,
                dark_ink: DARK_INK,
                accent: p.accent,
                track: TRACK,
                divider: DIVIDER,
                button: BUTTON,
                tile: TILE,
                muted: Color::rgba(142, 142, 142, 1.0),
                soft: Color::rgba(196, 202, 214, 1.0),
                faint: Color::rgba(110, 118, 136, 1.0),
                rail_done: Color::rgba(74, 84, 98, 1.0),
                tag_fill: Color::rgba(255, 255, 255, 0.10),
            };
        }
        Self {
            card: p.panel.alpha(CARD.0[3]),
            ink: p.ink,
            dark_ink: DARK_INK,
            accent: p.accent,
            track: p.surface_3,
            divider: p.edge,
            button: p.surface_2,
            tile: p.surface_3,
            muted: p.text_dim,
            soft: p.ink_2,
            faint: p.text_faint,
            rail_done: p.surface_3,
            tag_fill: p.edge,
        }
    }
}
