//! The route's look on the map of a plugin's panel that fills the navigator's frame (the `map`
//! element's `route`): its colours, its least width, its arrows, the faint route through the
//! houses, the jams' colours and the part already driven. Only that map takes it; the cockpit
//! screens, the remote picture and the navigator without such a panel draw the route as ever,
//! and keep a cached mesh of their own (`route_slot`), so that pictures drawn in turn do not
//! build each other's route again at every draw.

use super::*;
use style::Palette;

/// The route's look a frame map asks for (each None: the navigator's own).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameRoute {
    pub color: Option<Color>,
    pub edge: Option<Color>,
    /// The part already driven, drawn only when it has a colour.
    pub driven: Option<Color>,
    /// The least width in pixels.
    pub width: Option<f32>,
    pub arrows: Option<bool>,
    pub ghost: Option<bool>,
    pub jams: Option<bool>,
}

/// The cache slot of the route's mesh: one for the route ahead and one with the route already
/// driven (`history`, a cockpit screen's setting), and both again for a frame map's own look.
pub(super) fn route_slot(history: bool, look: Option<FrameRoute>) -> usize {
    usize::from(history) + 2 * usize::from(look.is_some())
}

/// How the route is drawn near `anchor` (as far as `reach`): the lane's width less 0.6 m, at
/// least 5 pixels or the look's width, with arrows every 32 m for 900 m unless it has none.
pub(super) fn route_style(look: Option<&FrameRoute>, anchor: DVec2, reach: f64) -> RouteStyle {
    RouteStyle {
        extra_m: -0.6,
        min_px: look.and_then(|l| l.width).unwrap_or(5.0),
        arrows: look
            .and_then(|l| l.arrows)
            .unwrap_or(true)
            .then_some((32.0, 900.0)),
        max_len: 12_000.0,
        near: Some((anchor, reach)),
    }
}

/// The palette with the look's route colours in it (the route is drawn from the palette).
pub(super) fn route_palette(pal: &Palette, look: Option<&FrameRoute>) -> Palette {
    let mut pal = *pal;
    if let Some(l) = look {
        pal.route = l.color.unwrap_or(pal.route);
        pal.route_edge = l.edge.unwrap_or(pal.route_edge);
    }
    pal
}

/// The faint route through the houses is drawn.
pub(super) fn ghost_shown(look: Option<&FrameRoute>) -> bool {
    look.and_then(|l| l.ghost).unwrap_or(true)
}

/// The route takes the jams' colours where the traffic is slow.
pub(super) fn jams_shown(look: Option<&FrameRoute>) -> bool {
    look.and_then(|l| l.jams).unwrap_or(true)
}

/// The lane `id` of `net` comes within `reach` of `anchor`.
pub(super) fn lane_near(net: &Network, id: usize, anchor: DVec2, reach: f64) -> bool {
    net.lanes.get(id).is_some_and(|l| {
        l.points.windows(2).any(|p| {
            let a = p[0].truncate();
            let d = p[1].truncate() - a;
            let t = ((anchor - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
            (a + d * t - anchor).length() <= reach
        })
    })
}

/// The part of the route already driven near `anchor`, as wide as the route and in `colour`:
/// the lanes behind the bus and its own lane up to `s`, under the route ahead.
pub(super) fn driven_route(
    p: &mut Painter,
    net: &Network,
    (lanes, s): (&[usize], f32),
    (anchor, reach): (DVec2, f64),
    style: &RouteStyle,
    colour: Color,
) {
    let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
    let Some((&now, behind)) = lanes.split_last() else {
        return;
    };
    for (k, &l) in behind.iter().chain([&now]).enumerate() {
        let Some(lane) = net
            .lanes
            .get(l)
            .filter(|_| lane_near(net, l, anchor, reach))
        else {
            continue;
        };
        let to = if k == behind.len() { s } else { lane.length() };
        let mut pts: Vec<Vec3> = lane
            .points
            .iter()
            .zip(&lane.dist)
            .filter(|(_, d)| **d < to)
            .map(|(q, _)| rel(*q))
            .collect();
        pts.push(rel(lane.at(to.min(lane.length())).0));
        let w = lane.width.max(2.6) + style.extra_m;
        p.ribbon(&pts, w, style.min_px, colour, true);
    }
}
