//! The colours a plugin gives the map in its panel that fills the navigator's frame (the
//! `map` element's `colors`), over the light or dark palette the player chose. The other
//! pictures (a screen in the cockpit, the remote cockpit, the city map) keep the palette and
//! the navigator's ground: colours are built into the ground, so a frame map whose colours
//! change it has a ground of its own, built again only when they change.

use super::build::Job;
use super::style::{self, Palette};
use super::{Color, Navigator, Roads};
use std::hash::{Hash, Hasher};

/// A frame map's colours, each in its name's place in `style::NAMES` (None: the palette's).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameColors([Option<Color>; style::NAMES.len()]);

impl Default for FrameColors {
    fn default() -> FrameColors {
        FrameColors([None; style::NAMES.len()])
    }
}

/// Colours by the palette's names; a name the palette lacks is left out.
impl<'a> FromIterator<(&'a str, Color)> for FrameColors {
    fn from_iter<I: IntoIterator<Item = (&'a str, Color)>>(colors: I) -> FrameColors {
        let mut m = FrameColors::default();
        for (name, c) in colors {
            if let Some(i) = style::NAMES.iter().position(|n| *n == name) {
                m.0[i] = Some(c);
            }
        }
        m
    }
}

impl FrameColors {
    /// The colours given, by name.
    fn given(&self) -> impl Iterator<Item = (&'static str, Color)> + '_ {
        style::NAMES
            .into_iter()
            .zip(self.0)
            .filter_map(|(name, c)| Some((name, c?)))
    }

    /// `pal` with these colours.
    pub(super) fn over(&self, pal: Palette) -> Palette {
        self.given().fold(pal, |p, (name, c)| p.with(name, c))
    }
}

/// The colours drawn anew in every picture, over the ground and not in it: the land under
/// the map and the navigator's cards.
const OVER_THE_GROUND: [&str; 4] = ["land", "card", "card_text", "card_dim"];

/// A frame map's own ground, built with its key while its colours change the ground, and the
/// build on its way.
#[derive(Default)]
pub(super) struct FrameGround {
    pub roads: Option<Roads>,
    pub job: Option<Job<(u64, usize, f64)>>,
}

/// The ground a picture of the panel draws on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GroundChoice {
    /// The frame map's own (`FrameGround`), not the navigator's.
    pub own: bool,
    /// Its key: the navigator's `look_key`, or the frame map's.
    pub key: u64,
    /// The palette it is built in: the player's for the navigator's ground, whichever picture
    /// builds it; the frame map's colours over it for that map's own.
    pub pal: Palette,
}

impl GroundChoice {
    /// Where its new builds go on the graphics card.
    pub(super) fn buffers(&self) -> [usize; 4] {
        if self.own { FRAME_BUFFERS } else { BUFFERS }
    }
}

/// Where the navigator's ground is on the graphics card: the ground, the houses' shadows, the
/// houses standing and flat.
const BUFFERS: [usize; 4] = [0, 12, 8, 13];
/// Where a frame map's own ground is, beside the navigator's (which stays there for the other
/// pictures).
const FRAME_BUFFERS: [usize; 4] = [19, 20, 21, 22];

/// The roads a picture draws and where they are on the graphics card: a frame map's own
/// ground (`own`) once it is built, else - and until then - the navigator's.
pub(super) fn ground_drawn<'a>(
    roads: &'a Option<Roads>,
    frame: &'a Option<FrameGround>,
    own: bool,
) -> (Option<&'a Roads>, [usize; 4]) {
    match frame.as_ref().and_then(|g| g.roads.as_ref()) {
        Some(r) if own => (Some(r), FRAME_BUFFERS),
        _ => (roads.as_ref(), BUFFERS),
    }
}

/// The cache of the route's mesh a picture uses: by the history it shows and the ground it
/// lies on (each its own anchor), so that pictures drawn in turn keep each other's mesh.
pub(super) fn route_slot(history: bool, own_ground: bool) -> usize {
    usize::from(history) + 2 * usize::from(own_ground)
}

impl Navigator {
    /// The colours of the frame panel's map in a picture of this look: a picture with the
    /// plugins' panels (the navigator on the window) while that map is shown; None for the
    /// others, and while there is none.
    pub(super) fn frame_colors(&self, look: &super::panel::PanelLook) -> Option<FrameColors> {
        self.framed
            .filter(|_| look.plugins)
            .flatten()
            .map(|(_, _, colors)| colors)
    }

    /// The palette of a picture whose map has these colours: the player's, light or dark,
    /// with them.
    pub(super) fn picture_palette(&self, colors: Option<&FrameColors>) -> Palette {
        let pal = *style::palette(self.light);
        colors.map_or(pal, |c| c.over(pal))
    }

    /// The panel ground's key under a frame map with these colours: the navigator's
    /// `look_key` while they leave the ground as the player's palette has it (none, the
    /// palette's own, or only the land's and the cards'), else one that changes with them - so
    /// that the ground is built again once when they change, and for nothing else of them.
    pub(super) fn frame_look_key(&self, colors: Option<&FrameColors>) -> u64 {
        let pal = *style::palette(self.light);
        let mut changed = colors
            .into_iter()
            .flat_map(FrameColors::given)
            .filter(|(name, c)| !OVER_THE_GROUND.contains(name) && pal.color(name) != Some(*c))
            .peekable();
        if changed.peek().is_none() {
            return self.look_key();
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.look_key().hash(&mut h);
        for (name, c) in changed {
            (name, c.0.map(f32::to_bits)).hash(&mut h);
        }
        h.finish()
    }

    /// The ground of a picture whose map has these colours: a frame map's own while they
    /// change the ground; else the navigator's, which every other picture draws on.
    pub(super) fn ground_for(&self, colors: Option<&FrameColors>) -> GroundChoice {
        let key = self.frame_look_key(colors);
        let own = key != self.look_key();
        GroundChoice {
            own,
            key,
            pal: self.picture_palette(colors.filter(|_| own)),
        }
    }

    /// Lets the frame map's own ground go once no frame map wants it: the frame panel's map
    /// went, or its colours leave the ground as the navigator's again. (Kept while the
    /// navigator only fades out and in, so that its map is there again at once.)
    pub(super) fn drop_frame_ground(&mut self) {
        let colors = self
            .frame_panel
            .and_then(|p| p.map)
            .and(self.frame_look)
            .map(|l| l.colors);
        if !self.ground_for(colors.as_ref()).own {
            self.let_frame_ground_go();
        }
    }

    /// Lets the frame map's own ground go, and its room on the graphics card.
    pub(super) fn let_frame_ground_go(&mut self) {
        if self.frame_ground.take().is_some()
            && let Some(gpu) = self.gpu.as_mut()
        {
            FRAME_BUFFERS
                .into_iter()
                .for_each(|id| gpu.drop_vertices(id));
        }
    }
}
