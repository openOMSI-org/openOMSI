//! The photo mode: the game stands still and a camera of its own flies round the bus - free
//! as a drone or in orbit round it - with a real lens (focal length, aperture, focus, the
//! blades' shape in the bokeh), a shutter that blurs the street past a moving bus, a
//! darkroom (exposure, white balance, tone, colour, film looks, vignetting, grain), a frame
//! of any aspect ratio with composition guides, and the photo saved into `Screenshots`.
//!
//! What is shown is the photo itself, developed as it will be saved: while the camera moves
//! a quick picture, once it stands the pictures of the shot add up frame by frame until the
//! lens and the shutter have been sampled (`render`). The panel is the launcher's toolkit
//! (`panel`).

use std::collections::HashSet;

use glam::{DVec3, Vec3};
use omsi_render::Camera;
use winit::keyboard::KeyCode;
use crate::app_impl::ShiftClockCx;

mod pad;
mod panel;
mod render;

// (the lens and the darkroom are the omsi-photo crate)
use omsi_photo::{develop, exposure, lens};
pub(crate) use develop::Grade;
pub(crate) use omsi_photo::stretch;

/// A request of the photo mode's panel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Request {
    Exit,
    Take,
    Reset,
    HideUi,
    /// The horizon level again (no roll).
    Level,
    /// The clock moved by these seconds (the light of another hour).
    Clock(f64),
    /// The photo just saved shown in the file browser.
    Reveal(std::path::PathBuf),
    /// Focus on what the picture shows at this point (shares of the window: x, y).
    FocusAt(f32, f32),
    /// Focus at the hyperfocal distance (everything from half of it to the horizon sharp).
    Hyperfocal,
}

/// How far the camera may go from the bus: (metres, label).
pub(crate) const RANGES: [(f32, &str); 5] = [(25.0, "25 m"), (50.0, "50 m"), (100.0, "100 m"), (250.0, "250 m"), (f32::INFINITY, "Unlimited")];
/// The frame: (width / height, label); None: the window's.
pub(crate) const ASPECTS: [(Option<f32>, &str); 8] = [
    (None, "Window"),
    (Some(16.0 / 9.0), "16:9"),
    (Some(21.0 / 9.0), "21:9 (cinema)"),
    (Some(3.0 / 2.0), "3:2"),
    (Some(4.0 / 3.0), "4:3"),
    (Some(1.0), "1:1 (square)"),
    (Some(4.0 / 5.0), "4:5 (portrait)"),
    (Some(9.0 / 16.0), "9:16 (story)"),
];
pub(crate) const GRIDS: [&str; 4] = ["Off", "Rule of thirds", "Golden ratio", "Centre"];
/// The f-numbers of the aperture ring.
pub(crate) const FSTOPS: [f32; 11] = [1.2, 1.4, 1.8, 2.0, 2.8, 4.0, 5.6, 8.0, 11.0, 16.0, 22.0];
/// Shutter times (s) with their labels.
pub(crate) const SHUTTERS: [(f32, &str); 8] = [(1.0 / 8.0, "1/8 s"), (1.0 / 15.0, "1/15 s"), (1.0 / 30.0, "1/30 s"), (1.0 / 60.0, "1/60 s"), (1.0 / 125.0, "1/125 s"), (1.0 / 250.0, "1/250 s"), (1.0 / 500.0, "1/500 s"), (1.0 / 1000.0, "1/1000 s")];
pub(crate) const BLADES: [(u32, &str); 4] = [(0, "Round"), (5, "5 blades"), (6, "6 blades"), (8, "8 blades")];
/// Pictures a photo is made of when it has depth of field or motion blur.
pub(crate) const QUALITY: [(u32, &str); 4] = [(16, "Draft (16 samples)"), (32, "Good (32 samples)"), (64, "High (64 samples)"), (128, "Best (128 samples)")];
/// The saved photo's size, times the window's.
pub(crate) const SIZES: [(f32, &str); 2] = [(1.0, "As the window"), (2.0, "Double (supersampled)")];
/// The brightest aperture of the advanced mode's lens.
pub(crate) const LENS_MAX: [f32; 5] = [1.2, 1.4, 1.8, 2.8, 4.0];
/// Exposure bracketing: how many photos.
pub(crate) const BRACKETS: [(u32, &str); 3] = [(1, "Off"), (3, "3 photos"), (5, "5 photos")];
/// How the bus's motion is taken: frozen, panned with (the street blurs) or from a tripod
/// (the bus blurs).
pub(crate) const MOTIONS: [&str; 3] = ["Frozen", "Panning", "Tripod"];

/// What the photo mode is set to (kept for the session).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub orbit: bool,
    /// Vertical field of view (degrees).
    pub fov: f32,
    pub roll: f32,
    pub speed: f32,
    pub range: usize,
    pub grid: usize,
    pub aspect: usize,
    pub dof: bool,
    pub auto_focus: bool,
    pub focus: f32,
    pub fstop: usize,
    pub blades: usize,
    pub motion: bool,
    pub shutter: usize,
    pub quality: usize,
    pub grade: Grade,
    pub size: usize,
    /// The spirit level over the picture.
    pub level: bool,
    /// The bus's motion from a tripod: the bus blurs, not the street.
    pub tripod: bool,
    /// The advanced mode's camera: the sensor, the exposure, the lens's brightest aperture,
    /// the white balance (a preset, the custom kelvin), the picture profile, the lens as it
    /// comes (uncorrected), tilt and shift, how bright burnt-out lights are in the bokeh.
    pub sensor: usize,
    pub exposure: exposure::Exposure,
    pub lens_max: usize,
    pub wb: usize,
    pub kelvin: f32,
    pub profile: usize,
    pub lens_raw: bool,
    pub tilt: [f32; 2],
    /// Shift (mm on the sensor: right, up).
    pub shift: [f32; 2],
    pub highlights: f32,
    pub noise: bool,
    pub bracket: usize,
    pub bracket_step: f32,
    pub png16: bool,
    pub zebra: bool,
    pub peaking: bool,
    pub histogram: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            orbit: false, fov: lens::fov_for(35.0), roll: 0.0, speed: 1.0, range: 1, grid: 0, aspect: 0, dof: false, auto_focus: true, focus: 10.0, fstop: 4, blades: 0, motion: false, shutter: 2, quality: 1, grade: Grade::neutral(), size: 0,
            level: false, tripod: false, sensor: 0, exposure: exposure::Exposure::default(), lens_max: 1, wb: 0, kelvin: 5500.0, profile: 0, lens_raw: false, tilt: [0.0; 2], shift: [0.0; 2], highlights: 0.5, noise: true, bracket: 0, bracket_step: 1.0, png16: false, zebra: false, peaking: false, histogram: true,
        }
    }
}

impl Settings {
    /// The sensor's height (mm): full frame in the simple mode.
    pub(crate) fn sensor_h(&self, pro: bool) -> f32 {
        if pro { exposure::SENSORS[self.sensor.min(exposure::SENSORS.len() - 1)].1 } else { 24.0 }
    }

    /// The real focal length (mm) of the lens on the sensor.
    pub(crate) fn focal(&self, pro: bool) -> f32 {
        lens::focal_on(self.fov, self.sensor_h(pro))
    }
}

/// The photo mode while it is on.
pub(crate) struct Photo {
    pub cam: Camera,
    /// Where the bus stood (the camera's leash, the orbit's centre).
    pub anchor: DVec3,
    /// The bus's velocity (m/s, world) when the game stopped: the rolling shot's.
    pub bus_velocity: Vec3,
    /// The orbit: round the anchor, at this distance, from this side and height.
    orbit: (f32, f32, f32),
    vel: Vec3,
    fov_target: f32,
    pub keys: HashSet<KeyCode>,
    /// The view turned by dragging (the right button anywhere, the left over the picture).
    pub looking: bool,
    pub settings: Settings,
    pub tab: usize,
    pub hide_ui: bool,
    /// The game stood paused before.
    prev_paused: bool,
    /// The view the game had, to return to.
    prev_camera: Option<Camera>,
    /// How far the photo's clock was moved (s): moved back on the way out, so that the
    /// bus's duty goes on at the time it stood at (#2116).
    clock_shift: f64,
    pub(crate) render: render::State,
    /// A word of what was done (the photo saved), and how long it shows.
    pub note: Option<(String, f32)>,
    /// The file the note tells of (a click on it shows it in the file browser).
    pub saved: Option<std::path::PathBuf>,
    pad: pad::PadState,
    /// The advanced mode's camera body (from the launcher's setting).
    pub pro: bool,
    /// The left button pressed over the picture (where, shares of the window) and how far
    /// the view has turned since (degrees): let go without turning it is a click to focus.
    pub press: Option<(f32, f32)>,
    pub dragged: f32,
    /// The light of the scene as the meter reads it (EV at ISO 100, the metering mode's
    /// share included), and the exposure the camera takes with it.
    pub scene_ev: f32,
    pub metered: Option<exposure::Metered>,
    /// Where the focus was set by a click (shares of the window), shown a while.
    pub focus_mark: Option<(f32, f32, f32)>,
    /// On a phone: the settings sheet is open (else only the camera's own buttons show), the
    /// finger's rise or sink (-1 .. 1) and whether the camera is moved fast, and where the
    /// sheet was drawn last (physical pixels: x, y, width, height) for the fingers.
    pub touch_panel: bool,
    pub touch_z: f32,
    pub touch_fast: bool,
    pub panel_rect: Option<[f32; 4]>,
    /// The stick on the screen: right, forward (-1 .. 1).
    pub touch_move: glam::Vec2,
}

impl Photo {
    fn new(cam: Camera, anchor: DVec3, bus_velocity: Vec3, prev_paused: bool, settings: Settings) -> Photo {
        let to = cam.position - anchor;
        let dist = (to.length() as f32).clamp(3.0, 60.0);
        let around = (to.x as f32).atan2(to.y as f32).to_degrees();
        let up = ((to.z as f32) / dist.max(0.1)).clamp(-1.0, 1.0).asin().to_degrees();
        let mut cam = cam;
        cam.fov_deg = settings.fov;
        cam.roll = settings.roll;
        Photo {
            cam,
            anchor,
            bus_velocity,
            orbit: (dist, around, up.clamp(-10.0, 80.0)),
            vel: Vec3::ZERO,
            fov_target: settings.fov,
            keys: HashSet::new(),
            looking: false,
            settings,
            tab: 0,
            hide_ui: false,
            prev_paused,
            prev_camera: Some(cam),
            clock_shift: 0.0,
            render: render::State::default(),
            note: None,
            saved: None,
            pad: pad::PadState::default(),
            pro: false,
            press: None,
            dragged: 0.0,
            scene_ev: 12.0,
            metered: None,
            focus_mark: None,
            touch_panel: false,
            touch_z: 0.0,
            touch_fast: false,
            panel_rect: None,
            touch_move: glam::Vec2::ZERO,
        }
    }

    /// The f-number the photo is taken at: the camera's (the advanced mode's, its automatic
    /// modes deciding) or the ring's.
    pub(crate) fn fstop(&self) -> f32 {
        match (self.pro, self.metered) {
            (true, Some(m)) => m.fstop,
            _ => FSTOPS[self.settings.fstop.min(FSTOPS.len() - 1)],
        }
    }

    /// The shutter time (s).
    pub(crate) fn shutter_time(&self) -> f32 {
        match (self.pro, self.metered) {
            (true, Some(m)) => m.shutter,
            _ => SHUTTERS[self.settings.shutter.min(SHUTTERS.len() - 1)].0,
        }
    }

    /// The advanced mode's exposure for the light now.
    pub(crate) fn meter_now(&mut self) {
        if !self.pro {
            self.metered = None;
            return;
        }
        let s = &self.settings;
        // (the full-frame equivalent: what the field of view is on a 24 mm high sensor)
        let focal_eq = lens::focal_mm(s.fov);
        let max = LENS_MAX[s.lens_max.min(LENS_MAX.len() - 1)];
        let min = if max >= 2.8 { 32.0 } else { 22.0 };
        self.metered = Some(exposure::meter(&s.exposure, self.scene_ev, (max, min), focal_eq));
    }

    /// The camera turned by (dx, dy) degrees (right, down).
    pub(crate) fn look(&mut self, dx: f32, dy: f32) {
        let fine = if self.keys.contains(&KeyCode::AltLeft) || self.keys.contains(&KeyCode::AltRight) { 0.25 } else { 1.0 };
        self.dragged += dx.abs() + dy.abs();
        let (dx, dy) = (dx * fine, dy * fine);
        if self.settings.orbit {
            self.orbit.1 += dx;
            self.orbit.2 = (self.orbit.2 + dy).clamp(-10.0, 85.0);
        } else {
            self.cam.yaw = (self.cam.yaw + dx).rem_euclid(360.0);
            self.cam.pitch = (self.cam.pitch - dy).clamp(-89.0, 89.0);
        }
    }

    /// The lens zoomed by wheel notches (up: longer).
    pub(crate) fn zoom(&mut self, notches: f32) {
        self.fov_target = (self.fov_target * (1.0 - 0.08 * notches)).clamp(lens::fov_for(400.0), lens::fov_for(10.0));
    }

    /// The focus distance the lens is at: the bus (where the camera looks at it) when it
    /// focuses by itself, else the one set.
    pub(crate) fn focus_distance(&self) -> f32 {
        if !self.settings.auto_focus {
            return self.settings.focus;
        }
        let to = (self.anchor + DVec3::new(0.0, 0.0, 1.6) - self.cam.position).as_vec3();
        let along = to.dot(self.cam.forward());
        if along <= 0.3 {
            return self.settings.focus;
        }
        // (the bus's surface where the camera sees its middle, from the shot's depth: its
        // middle itself lay a couple of metres behind the side the camera looks at)
        let tan = (self.cam.fov_deg.to_radians() * 0.5).tan();
        let (w, h) = self.render.depth.as_ref().map(|d| (d.0 as f32, d.1 as f32)).unwrap_or((16.0, 9.0));
        let (u, v) = (to.dot(self.cam.right()) / along / (tan * w / h), to.dot(self.cam.up()) / along / tan);
        let surface = (u.abs() < 0.95 && v.abs() < 0.95).then(|| self.render.depth_at(0.5 + u * 0.5, 0.5 - v * 0.5)).flatten();
        match surface {
            Some(d) if d < along + 0.5 && d > along - 6.0 => d,
            _ => along,
        }
    }

    /// One frame of the camera: the keys and the pad held, smoothed, kept near the bus
    /// and above the ground.
    fn fly(&mut self, dt: f32, ground: impl Fn(f64, f64) -> Option<f64>) {
        let keys = self.keys.clone();
        let k = |c: KeyCode| keys.contains(&c);
        let axis = |plus: bool, minus: bool| (plus as i32 - minus as i32) as f32;
        // W A S D move, Space (or E) rises and Ctrl (or C, Q) sinks, as in a game's free camera
        let up = k(KeyCode::Space) || k(KeyCode::KeyE);
        let down = k(KeyCode::ControlLeft) || k(KeyCode::ControlRight) || k(KeyCode::KeyC) || k(KeyCode::KeyQ);
        let mut m = Vec3::new(axis(k(KeyCode::KeyD), k(KeyCode::KeyA)), axis(k(KeyCode::KeyW), k(KeyCode::KeyS)), axis(up, down));
        m += self.pad.moves();
        m += Vec3::new(self.touch_move.x, self.touch_move.y, self.touch_z);
        m.z *= 0.5;
        let fast = if k(KeyCode::ShiftLeft) || k(KeyCode::ShiftRight) || self.touch_fast { 4.0 } else if k(KeyCode::AltLeft) || k(KeyCode::AltRight) { 0.25 } else { 1.0 };
        // the arrows and the right stick turn the view; Z and X roll it
        let turn = 70.0 * dt * (self.cam.fov_deg / 60.0).clamp(0.15, 1.5);
        let (lx, ly) = (axis(k(KeyCode::ArrowRight), k(KeyCode::ArrowLeft)) + self.pad.look[0], axis(k(KeyCode::ArrowDown), k(KeyCode::ArrowUp)) + self.pad.look[1]);
        if lx != 0.0 || ly != 0.0 {
            self.look(lx * turn, ly * turn);
        }
        let roll = axis(k(KeyCode::KeyX), k(KeyCode::KeyZ)) + self.pad.roll;
        if roll != 0.0 {
            self.settings.roll = (self.settings.roll + roll * 30.0 * dt).clamp(-45.0, 45.0);
        }
        // the lens: + and - (and the pad's d-pad) zoom
        let zoom = axis(k(KeyCode::Equal) || k(KeyCode::NumpadAdd), k(KeyCode::Minus) || k(KeyCode::NumpadSubtract)) + self.pad.zoom;
        if zoom != 0.0 {
            self.zoom(zoom * dt * 6.0);
        }
        let ease = 1.0 - (-dt / 0.10).exp();
        self.settings.fov += (self.fov_target - self.settings.fov) * ease;
        if (self.settings.fov - self.fov_target).abs() < 0.05 {
            self.settings.fov = self.fov_target;
        }
        let speed = 5.0 * self.settings.speed * fast;
        let range = RANGES[self.settings.range.min(RANGES.len() - 1)].0;
        if self.settings.orbit {
            // W and S come closer and back away, A and D go round, Space and Ctrl up and down
            let target = Vec3::new(m.x * 60.0, -m.y * speed, m.z * 40.0);
            self.vel += (target - self.vel) * (1.0 - (-dt / 0.12).exp());
            if self.vel.length() < 0.05 && target == Vec3::ZERO {
                self.vel = Vec3::ZERO;
            }
            self.orbit.0 = (self.orbit.0 + self.vel.y * dt).clamp(2.0, range.min(400.0));
            self.orbit.1 += self.vel.x * dt;
            self.orbit.2 = (self.orbit.2 + self.vel.z * dt).clamp(-10.0, 85.0);
            let (d, a, e) = (self.orbit.0 as f64, (self.orbit.1 as f64).to_radians(), (self.orbit.2 as f64).to_radians());
            let centre = self.anchor + DVec3::new(0.0, 0.0, 1.5);
            self.cam.position = centre + DVec3::new(a.sin() * e.cos(), a.cos() * e.cos(), e.sin()) * d;
            let dir = (centre - self.cam.position).as_vec3().normalize_or(Vec3::Y);
            self.cam.yaw = dir.x.atan2(dir.y).to_degrees();
            self.cam.pitch = dir.z.clamp(-1.0, 1.0).asin().to_degrees();
        } else {
            // a drone: level over the ground whichever way it looks, up and down apart
            let (s, c) = self.cam.yaw.to_radians().sin_cos();
            let fwd = Vec3::new(s, c, 0.0);
            let right = Vec3::new(c, -s, 0.0);
            let target = (right * m.x + fwd * m.y + Vec3::Z * m.z) * speed;
            self.vel += (target - self.vel) * (1.0 - (-dt / 0.15).exp());
            // (stopped once it no longer shows: a velocity easing out for ever moved the camera
            // a fraction of a millimetre a frame, and every such move started the photo again)
            if self.vel.length() < 0.05 && target == Vec3::ZERO {
                self.vel = Vec3::ZERO;
            }
            self.cam.position += (self.vel * dt).as_dvec3();
        }
        // on its leash round the bus
        let off = self.cam.position - self.anchor;
        if range.is_finite() && off.length() > range as f64 {
            self.cam.position = self.anchor + off.normalize() * range as f64;
        }
        // and never under the ground
        if let Some(g) = ground(self.cam.position.x, self.cam.position.y) {
            if self.cam.position.z < g + 0.25 {
                self.cam.position.z = g + 0.25;
                self.vel.z = self.vel.z.max(0.0);
            }
        }
        self.cam.fov_deg = self.settings.fov;
        self.cam.roll = self.settings.roll;
    }

    /// A key of the keyboard while the photo mode is on; the requests it makes.
    fn key(&mut self, code: KeyCode, pressed: bool) -> Option<Request> {
        if !pressed {
            self.keys.remove(&code);
            return None;
        }
        self.keys.insert(code);
        match code {
            KeyCode::Escape => Some(Request::Exit),
            KeyCode::KeyH => Some(Request::HideUi),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::F12 => Some(Request::Take),
            KeyCode::KeyR => Some(Request::Level),
            KeyCode::Tab => {
                let n = panel::tabs(self.pro).len();
                let back = self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
                self.tab = if back { (self.tab + n - 1) % n } else { (self.tab + 1) % n };
                None
            }
            _ => None,
        }
    }
}

impl Photo {
    /// The photo as last developed (w, h, RGBA).
    pub(crate) fn developed(&self) -> Option<(u32, u32, &[u8])> {
        self.render.developed.as_ref().map(|(w, h, px)| (*w, *h, px.as_slice()))
    }

    /// The photo as the window shows it (the live view's aids over it).
    pub(crate) fn shown(&self) -> Option<(u32, u32, &[u8])> {
        let (w, h, px) = self.developed()?;
        Some((w, h, self.render.shown.as_deref().filter(|s| s.len() == px.len()).unwrap_or(px)))
    }
}

/// `OMSI_PHOTO`: settings for a test, `key=value` by commas.
fn test_settings(s: &mut Settings, text: &str) {
    for part in text.split(',') {
        let Some((k, v)) = part.split_once('=') else { continue };
        let x: f32 = v.trim().parse().unwrap_or(0.0);
        let near = |list: &[f32]| list.iter().enumerate().min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs())).map(|e| e.0).unwrap_or(0);
        match k.trim() {
            "orbit" => s.orbit = x != 0.0,
            "focal" => s.fov = lens::fov_for(x),
            "dof" => s.dof = x != 0.0,
            "fstop" => s.fstop = near(&FSTOPS),
            "focus" => {
                s.auto_focus = false;
                s.focus = x;
            }
            "motion" => s.motion = x != 0.0,
            "shutter" => s.shutter = near(&SHUTTERS.map(|t| 1.0 / t.0)),
            "filter" => s.grade.filter = develop::Filter::ALL[(x as usize).min(develop::Filter::ALL.len() - 1)],
            "vignette" => s.grade.vignette = x,
            "grain" => s.grade.grain = x,
            "exposure" => s.grade.exposure = x,
            "aspect" => s.aspect = (x as usize).min(ASPECTS.len() - 1),
            "grid" => s.grid = (x as usize).min(GRIDS.len() - 1),
            "quality" => s.quality = (x as usize).min(QUALITY.len() - 1),
            // the advanced mode's
            "mode" => s.exposure.mode = (x as usize).min(3),
            "iso" => {
                s.exposure.iso = exposure::nearest_iso(x);
                s.exposure.auto_iso = false;
            }
            "t" => s.exposure.shutter = exposure::nearest_shutter(1.0 / x.max(1e-3)),
            "f" => s.exposure.aperture = exposure::nearest_aperture(x),
            "comp" => s.exposure.compensation = x,
            "metering" => s.exposure.metering = (x as usize).min(2),
            "sensor" => s.sensor = (x as usize).min(exposure::SENSORS.len() - 1),
            "kelvin" => {
                s.wb = exposure::WHITE_BALANCE.len() - 1;
                s.kelvin = x;
            }
            "profile" => s.profile = (x as usize).min(exposure::PROFILES.len() - 1),
            "raw" => s.lens_raw = x != 0.0,
            "tilt" => s.tilt[0] = x,
            "swing" => s.tilt[1] = x,
            "shift" => s.shift[1] = x,
            "shiftx" => s.shift[0] = x,
            "tripod" => s.tripod = x != 0.0,
            "highlights" => s.highlights = x,
            "bracket" => s.bracket = (x as usize).min(BRACKETS.len() - 1),
            "png16" => s.png16 = x != 0.0,
            "zebra" => s.zebra = x != 0.0,
            "peaking" => s.peaking = x != 0.0,
            "border" => s.grade.border = develop::Border::ALL[(x as usize).min(develop::Border::ALL.len() - 1)],
            "clarity" => s.grade.clarity = x,
            "noise" => s.noise = x != 0.0,
            "pro" => {}
            "dist" | "around" | "up" | "kmh" | "tab" => {}
            _ => log::warn!("OMSI_PHOTO: no setting {k}"),
        }
    }
}

impl crate::App {
    pub(crate) fn photo_on(&self) -> bool {
        self.photo.is_some()
    }

    /// Into the photo mode (from the pause menu or its key): the game stands still, the
    /// camera starts where the view was.
    pub(crate) fn enter_photo(&mut self) {
        if self.photo.is_some() || self.camera.is_none() {
            return;
        }
        self.release_vehicle_keys();
        let cam = self.camera.unwrap();
        let (anchor, velocity) = match self.player.as_ref() {
            Some(p) => {
                let h = p.vehicle.heading.to_radians();
                let v = p.vehicle.physics.speed;
                (p.vehicle.position, Vec3::new(h.sin() as f32, h.cos() as f32, 0.0) * v)
            }
            None => (cam.position, Vec3::ZERO),
        };
        let mut settings = self.photo_settings.clone().unwrap_or_default();
        if let Some(t) = omsi_cfg::flags::OMSI_PHOTO.var() {
            test_settings(&mut settings, t);
        }
        let mut ph = Photo::new(cam, anchor, velocity, self.paused, settings);
        ph.pro = self.settings.photo_mode == "advanced";
        if let Some(t) = omsi_cfg::flags::OMSI_PHOTO.var() {
            if t.split(',').any(|p| p.trim() == "pro=1") {
                ph.pro = true;
            }
        }
        // (a camera inside the bus starts as it is; the field of view is the photo mode's)
        ph.cam.pitch = cam.pitch;
        // (a test's orbit: OMSI_PHOTO dist, around, up)
        for part in omsi_cfg::flags::OMSI_PHOTO.var().unwrap_or("").split(',') {
            let Some((k, v)) = part.split_once('=') else { continue };
            let x: f32 = v.trim().parse().unwrap_or(0.0);
            match k.trim() {
                "dist" => ph.orbit.0 = x,
                "around" => ph.orbit.1 = x,
                "up" => ph.orbit.2 = x,
                // (the panel's page open, for a test's picture of it)
                "tab" => ph.tab = x as usize,
                // (a moving bus for a rolling shot without driving one)
                "kmh" => {
                    let h = self.player.as_ref().map(|p| p.vehicle.heading.to_radians()).unwrap_or(0.0);
                    ph.bus_velocity = Vec3::new(h.sin() as f32, h.cos() as f32, 0.0) * x / 3.6;
                }
                _ => {}
            }
        }
        self.photo = Some(ph);
        if self.net.lan.is_none() {
            self.paused = true;
        }
        log::info!("photo mode on at {:.1},{:.1},{:.1}, the bus at {:.1} km/h", cam.position.x, cam.position.y, cam.position.z, velocity.length() * 3.6);
    }

    pub(crate) fn photo_request(&mut self, r: Request) {
        photo_request(&mut PhotoCx::of(&mut self.parts()), r);
    }

    /// A key while the photo mode is on: all of them are its own.
    pub(crate) fn photo_key(&mut self, code: KeyCode, pressed: bool, repeat: bool) {
        let Some(ph) = self.photo.as_mut() else { return };
        if repeat {
            return;
        }
        if let Some(r) = ph.key(code, pressed) {
            self.photo_request(r);
        }
    }
}

crate::app::stage_ctx!(
    /// The photo mode's part of the game (the clock it may move included).
    PhotoCx { reads: [input, net, world], writes: [ui, clock, view, menus, traffic, player, gfx] }
);

/// Out of the photo mode: the game as it was.
fn exit_photo(cx: &mut PhotoCx) {
    let Some(ph) = cx.ui.photo.take() else { return };
    *cx.ui.photo_settings = Some(ph.settings.clone());
    *cx.clock.paused = ph.prev_paused;
    if ph.clock_shift != 0.0 {
        crate::app_impl::shift_clock(crate::app::narrow!(cx => ShiftClockCx { reads: [net, input, world], writes: [clock, traffic, player, ui, gfx] }), -ph.clock_shift);
    }
    if let Some(c) = ph.prev_camera {
        *cx.view.camera = Some(c);
    }
    cx.menus.shell.drop_picture();
    cx.menus.shell.opaque = false;
    cx.menus.shell.clear();
    log::info!("photo mode off");
}

/// What the photo mode's keys, pad and panel ask for (`Request`).
pub(crate) fn photo_request(cx: &mut PhotoCx, r: Request) {
    match r {
        Request::Exit => exit_photo(cx),
        Request::Take => {
            if let Some(ph) = cx.ui.photo.as_mut() {
                ph.render.capture = true;
            }
        }
        Request::Reset => {
            if let Some(ph) = cx.ui.photo.as_mut() {
                ph.settings = Settings { fov: ph.settings.fov, orbit: ph.settings.orbit, ..Settings::default() };
                ph.fov_target = ph.settings.fov;
            }
        }
        Request::Level => {
            if let Some(ph) = cx.ui.photo.as_mut() {
                ph.settings.roll = 0.0;
            }
        }
        Request::HideUi => {
            if let Some(ph) = cx.ui.photo.as_mut() {
                ph.hide_ui = !ph.hide_ui;
            }
        }
        Request::Reveal(path) => crate::platform::reveal(&path),
        Request::FocusAt(x, y) => {
            if let Some(ph) = cx.ui.photo.as_mut() {
                match ph.render.depth_at(x, y) {
                    Some(d) => {
                        ph.settings.auto_focus = false;
                        ph.settings.focus = d.clamp(0.3, 5000.0);
                        if !ph.settings.dof {
                            ph.settings.dof = true;
                        }
                        ph.focus_mark = Some((x, y, 1.5));
                    }
                    None => ph.note = Some((omsi_ui::tr("Hold the camera still a moment, then click to focus").into_owned(), 3.0)),
                }
            }
        }
        Request::Hyperfocal => {
            if let Some(ph) = cx.ui.photo.as_mut() {
                let sh = ph.settings.sensor_h(ph.pro);
                let (_, _, hyper) = exposure::dof_limits(ph.settings.focal(ph.pro), ph.fstop(), 10.0, sh);
                ph.settings.auto_focus = false;
                ph.settings.focus = hyper.clamp(0.3, 5000.0);
            }
        }
        Request::Clock(secs) => {
            if cx.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                if let Some(ph) = cx.ui.photo.as_mut() {
                    ph.note = Some((omsi_ui::tr("In a LAN session the host sets the clock").into_owned(), 3.0));
                }
            } else {
                let moved = !crate::app_impl::real_time_locked(cx.common.settings, cx.net);
                crate::app_impl::shift_clock(crate::app::narrow!(cx => ShiftClockCx { reads: [net, input, world], writes: [clock, traffic, player, ui, gfx] }), secs);
                if let Some(ph) = cx.ui.photo.as_mut() {
                    if moved {
                        ph.clock_shift += secs;
                    }
                    ph.render.reset();
                }
            }
        }
    }
}

/// The photo mode's frame before the picture: the pad, the camera flown, the view the
/// game streams the world round, and the panel.
pub(crate) fn photo(mut cx: PhotoCx, dt: f32) {
    if cx.ui.photo.is_none() {
        return;
    }
    let requests = match (cx.ui.photo.as_mut(), cx.input.controllers.as_ref()) {
        (Some(ph), Some(ctl)) => ph.pad.read(ctl),
        (Some(ph), None) => {
            ph.pad = pad::PadState::default();
            Vec::new()
        }
        _ => Vec::new(),
    };
    for r in requests {
        photo_request(&mut cx, r);
    }
    let world = cx.world.world.clone();
    let Some(ph) = cx.ui.photo.as_mut() else { return };
    ph.touch_move = if cx.input.touch.enabled { let s = cx.input.touch.stick_analog(); glam::Vec2::new(s.x, -s.y) } else { glam::Vec2::ZERO };
    ph.fly(dt.min(0.1), |x, y| world.as_ref().and_then(|w| w.ground_height(x, y)));
    if let Some(n) = ph.note.as_mut() {
        n.1 -= dt;
    }
    if let Some(m) = ph.focus_mark.as_mut() {
        m.2 -= dt;
    }
    if ph.focus_mark.is_some_and(|m| m.2 <= 0.0) {
        ph.focus_mark = None;
    }
    if ph.note.as_ref().is_some_and(|n| n.1 <= 0.0) {
        ph.note = None;
        ph.saved = None;
    }
    *cx.view.camera = Some(ph.cam);
    panel::photo_panel(&mut cx, dt);
}
