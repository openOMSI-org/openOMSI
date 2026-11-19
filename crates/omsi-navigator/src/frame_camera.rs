//! The camera a plugin gives the map in its panel that fills the navigator's frame (the `map`
//! element's `camera`): how far it is, whether north is up, how far ahead of the bus it
//! looks and how it tilts. Each key left out is as the player's settings make it. The other
//! pictures (a screen in the cockpit, the remote cockpit, the city map) keep the navigator's
//! own camera and ground: a camera that wants another ground (another tilt, or farther than
//! the navigator's zoom goes) has the frame map build one of its own.

use super::build::Job;
use super::{MAX_ZOOM, Navigator, PITCH, Roads};

/// A frame map's camera, as the plugin gave it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameCamera {
    pub distance: Option<CameraDistance>,
    /// True: north up; false: the bus's heading up.
    pub north: Option<bool>,
    /// The share of the distance the view looks ahead of the bus (0: the bus in the middle).
    pub ahead: Option<f32>,
    pub tilt: Option<CameraTilt>,
}

/// The camera's distance from where it looks, in metres, as the navigator's own zoom is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraDistance {
    /// Always this.
    Fixed(f32),
    /// The navigator's zoom (out with the speed, in for the next turn) held between these.
    Bounds { min: f32, max: f32 },
}

/// How the map tilts: the houses standing in 3D, flat, or the classic 2D straight from above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraTilt {
    ThreeD,
    Flat,
    Top,
}

impl FrameCamera {
    /// The farthest its distance lets the camera go (None: as far as the navigator's own):
    /// the ground is built for it, so that it is not built again as the zoom eases out.
    pub(super) fn farthest(&self) -> Option<f64> {
        match self.distance? {
            CameraDistance::Fixed(d) => Some(d as f64),
            CameraDistance::Bounds { max, .. } => Some(max as f64),
        }
    }
}

/// What a tilt makes of the player's settings: whether the houses stand in 3D and whether the
/// map is the classic 2D (no houses, no surfaces). None: the settings as they are.
pub(super) fn tilt_view(tilt: Option<CameraTilt>, show_3d: bool, classic_2d: bool) -> (bool, bool) {
    match tilt {
        None => (show_3d, classic_2d),
        Some(CameraTilt::ThreeD) => (true, false),
        Some(CameraTilt::Flat) => (false, false),
        Some(CameraTilt::Top) => (false, true),
    }
}

/// The camera's pitch (degrees), as the navigator has always chosen it: the classic 2D
/// straight down, the houses in 3D at 52°, flat at 60°.
pub(super) fn pitch_for(tilt: Option<CameraTilt>, show_3d: bool, classic_2d: bool) -> f64 {
    match tilt_view(tilt, show_3d, classic_2d) {
        (_, true) => 90.0,
        (true, false) => PITCH,
        (false, false) => PITCH + 8.0,
    }
}

/// A frame map's own ground, built with its key while its camera wants another ground than
/// the navigator's, and the build on its way.
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
    /// Built as the classic 2D (no houses, no surfaces).
    pub classic_2d: bool,
    /// The farthest the picture's camera goes (metres): the ground reaches that far.
    pub most: f64,
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
    /// The camera of the frame panel's map shown in the last frame (None: no such map, or
    /// one without a camera).
    pub(super) fn frame_camera(&self) -> Option<FrameCamera> {
        self.framed.flatten().and_then(|(_, _, camera)| camera)
    }

    /// The ground of a picture whose map has this camera (`map_scale`: its own zoom's share):
    /// a frame map's own when its camera changes whether the ground is the classic 2D, or
    /// goes farther than the navigator's zoom; else the navigator's, which every other picture
    /// draws on.
    pub(super) fn ground_for(&self, camera: Option<&FrameCamera>, map_scale: f64) -> GroundChoice {
        let key = self.frame_look_key(camera);
        let farthest = camera.and_then(|c| c.farthest()).unwrap_or(0.0);
        GroundChoice {
            own: key != self.look_key() || farthest > MAX_ZOOM,
            key,
            classic_2d: self.ground_2d(camera),
            most: (MAX_ZOOM / map_scale).max(farthest),
        }
    }

    /// Lets the frame map's own ground go once no frame map wants it: the frame panel's map
    /// went, or its camera wants the navigator's ground again. (Kept while the navigator only
    /// fades out and in, so that its map is there again at once.)
    pub(super) fn drop_frame_ground(&mut self) {
        let camera = self
            .frame_panel
            .and_then(|p| p.map)
            .and(self.frame_look)
            .and_then(|l| l.camera);
        if !self.ground_for(camera.as_ref(), 1.0).own {
            self.let_frame_ground_go();
        }
    }

    /// Lets the frame map's own ground go, and its room on the graphics card.
    pub(super) fn let_frame_ground_go(&mut self) {
        if self.frame_ground.take().is_some() {
            if let Some(gpu) = self.gpu.as_mut() {
                FRAME_BUFFERS
                    .into_iter()
                    .for_each(|id| gpu.drop_vertices(id));
            }
        }
    }
}
