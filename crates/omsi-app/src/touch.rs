//! The game driven by fingers: a steering wheel (or the phone's tilt), the pedals, the
//! gearbox, the doors, the brakes, the indicators and the horn on the screen, a panel with
//! the rest of the cab, and the cameras. What no control takes is the cockpit's and the
//! camera's: a tap works the switch under the finger as a click does, a drag on a switch
//! turns it, a drag elsewhere looks round (or turns the outside camera), two fingers zoom.
//! While the game menu, a list or the city map is open every finger is a mouse.
//!
//! The controls are drawn with `omsi-ui` over the game's picture (Material Symbols icons),
//! in physical pixels scaled by the screen's density. The pedals and the wheel feed the
//! same analog inputs a game controller does (`Player::analog`), the buttons fire the
//! actions of `Inputs/keyboard.cfg` (or the keys that stand for them), so every bus - stock
//! or mod - is driven by them as it is by the keyboard.

use super::*;
use omsi_ui::paint::Align;
use omsi_ui::{Atlas, Color, Draw, Fonts, Gpu, Layer, Painter, Rect, Weight};
use winit::event::{Touch as Finger, TouchPhase};

mod paint;
#[cfg(test)]
mod tests;

/// What a control does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Btn {
    Menu,
    Pause,
    Camera,
    LookReset,
    Map,
    Timetable,
    Panel,
    Hide,
    /// The n-th door, front to back (`Shift + n`).
    Door(usize),
    /// One direct OMSI trigger group (0, 1, or combined 2+3).
    DirectDoor(usize),
    StopBrake,
    ParkingBrake,
    /// A gear action (`automatic_D`, `kw_s_plus` ...) and its letter.
    Gear(&'static str, &'static str),
    BlinkLeft,
    BlinkRight,
    Hazard,
    Horn,
    // the cab panel
    Battery,
    Engine,
    AutoStart,
    Headlights,
    HighBeam,
    Wipers,
    SaloonLights,
    Ticket,
    /// The Q key, held: the bus's announcement or microphone, whatever its keys put there
    /// (#2107: a phone had no way to play the stop announcement).
    Announce,
    Tilt,
    Info,
    Navigator,
    InteriorCam,
    Screenshot,
    CloseMenu,
    /// On foot: down on the knees and up again (C).
    Kneel,
    /// The voice chat's talk keys, held: nearby, and the radio.
    VoiceNearby,
    VoiceRadio,
    /// Into the free camera (F4) from the cab panel, and back to the driver's seat.
    FreeCam,
    /// The free camera rises and sinks while held (E, Q).
    FlyUp,
    FlyDown,
    /// The free camera's speed halved or doubled.
    FlySlower,
    FlyFaster,
    /// The photo mode's: leave, the settings sheet, the panel hidden, the shutter.
    PhotoExit,
    PhotoPanel,
    PhotoHide,
    PhotoShutter,
}

impl Btn {
    /// Held down while the finger stays (a push button), rather than a tap.
    fn held(self) -> bool {
        matches!(self, Btn::Door(_) | Btn::DirectDoor(_) | Btn::Horn | Btn::Engine | Btn::Announce | Btn::VoiceNearby | Btn::VoiceRadio | Btn::FlyUp | Btn::FlyDown)
    }
}

/// The physical layout requested for direct mobile door controls. Button 3 operates
/// both 2 and 3; 4 is not an independent control and must not appear.
const DIRECT_DOOR_GROUPS: [&[&str]; 3] = [
    &["bus_doorfront0"],
    &["bus_doorfront1"],
    &["bus_doorfront2", "bus_doorfront3"],
];

/// Display a group only when the bus actually exposes one of its triggers.
fn direct_door_buttons<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let names: Vec<&str> = names.into_iter().collect();
    DIRECT_DOOR_GROUPS
        .iter()
        .enumerate()
        .filter(|(_, group)| group.iter().any(|cmd| names.iter().any(|n| n.eq_ignore_ascii_case(cmd))))
        .map(|(index, _)| index)
        .collect()
}

/// A control on the screen this frame.
#[derive(Clone)]
struct Button {
    btn: Btn,
    rect: Rect,
    icon: &'static str,
    label: String,
    /// Lit (a switch that is on, the chosen gear).
    on: bool,
    round: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    /// A button: where it was in the list when the finger came down, and which.
    Button(usize, Btn),
    /// The wheel: the finger's angle round the wheel's centre when it last moved (rad,
    /// screen y down: clockwise positive), and its distance from the centre then.
    Wheel(f32, f32),
    Throttle,
    Brake,
    Clutch,
    /// Walking or flying the free camera (W A S D from the stick).
    Stick,
    /// A switch of the cockpit held by the finger.
    Cockpit,
    /// Looking round (or, not moved far, a tap on the picture).
    Look,
    /// The mouse, while the city map has the screen.
    Mouse,
    /// The game menu's (or the chooser's) list: a drag scrolls it, a tap picks a line.
    Menu,
    MenuSlider,
    /// The photo mode's picture: a drag turns the camera, a tap focuses, two fingers zoom.
    Photo,
    /// The small navigator: a tap opens the city map, a drag moves it (as the mouse does,
    /// #940), where it then stays - out of the middle of the screen, which it covered on a
    /// phone (#1138).
    Navigator,
    Panel,
}

struct Touched {
    id: u64,
    start: Vec2,
    pos: Vec2,
    role: Role,
    moved: bool,
    since: Instant,
}

use glam::Vec2;

/// The touch controls' state (a field of `App`).
pub struct Touch {
    pub enabled: bool,
    fingers: Vec<Touched>,
    /// The wheel's turn (-1 .. 1), the pedals (0 .. 1).
    steer: f32,
    throttle: f32,
    brake: f32,
    /// A finger has the wheel (it stays where it is put; let go, it comes back).
    steering: bool,
    tilt: bool,
    /// The voice chat's talk buttons held down on the screen: nearby, radio.
    pub(crate) voice_held: [bool; 2],
    panel: bool,
    hidden: bool,
    /// The two fingers of a pinch: their last distance.
    pinch: Option<f32>,
    /// Keys the stick holds (free camera, on foot).
    stick_keys: Vec<KeyCode>,
    stick_at: Option<(Vec2, Vec2)>,
    buttons: Vec<Button>,
    wheel_c: Vec2,
    wheel_r: f32,
    throttle_r: Rect,
    brake_r: Rect,
    /// A manual gearbox's clutch pedal (zero-sized when the settings work the clutch).
    clutch_r: Rect,
    clutch: f32,
    stick_c: Vec2,
    stick_r: f32,
    /// Physical pixels per point of the layout.
    u: f32,
    size: (f32, f32),
    gpu: Option<(Gpu, wgpu::TextureFormat)>,
    shot_gpu: Option<Gpu>,
    fonts: Option<Fonts>,
    atlas: Atlas,
    painter: Painter,
    /// The gear button pressed last (the gearbox's variables differ from bus to bus).
    gear: Option<&'static str>,
    /// A word about what a button did (bottom middle), and how long it stays.
    note: Option<(String, f32)>,
    /// The room the information bar has (`ui::Frame::info_room`): between the buttons along
    /// the top, which it lay under in the middle of the screen (#1164).
    pub info_room: Option<[f32; 3]>,
    /// The gap between the buttons along the top, left and right, and their bottom: the
    /// navigator keeps out of them.
    pub top_gap: Option<[f32; 3]>,
}

impl Touch {
    pub(crate) fn pointing(&self) -> bool {
        self.fingers.iter().any(|f| matches!(f.role, Role::Cockpit | Role::Mouse))
    }

    pub fn new() -> Touch {
        Touch {
            enabled: crate::platform::touch_controls(),
            fingers: Vec::new(),
            steer: 0.0,
            throttle: 0.0,
            brake: 0.0,
            steering: false,
            tilt: false,
            voice_held: [false; 2],
            panel: false,
            hidden: false,
            pinch: None,
            stick_keys: Vec::new(),
            stick_at: None,
            buttons: Vec::new(),
            wheel_c: Vec2::ZERO,
            wheel_r: 1.0,
            throttle_r: Rect::new(0.0, 0.0, 0.0, 0.0),
            brake_r: Rect::new(0.0, 0.0, 0.0, 0.0),
            clutch_r: Rect::new(0.0, 0.0, 0.0, 0.0),
            clutch: 0.0,
            stick_c: Vec2::ZERO,
            stick_r: 1.0,
            u: 1.0,
            size: (1.0, 1.0),
            gpu: None,
            shot_gpu: None,
            fonts: None,
            atlas: Atlas::new(1024),
            painter: Painter::new(),
            note: None,
            gear: None,
            info_room: None,
            top_gap: None,
        }
    }

    fn button_at(&self, p: Vec2) -> Option<usize> {
        // (a round button takes a finger a little outside its circle)
        self.buttons.iter().position(|b| {
            if b.round {
                p.distance(b.rect.center()) <= b.rect.w * 0.5 + 6.0 * self.u
            } else {
                b.rect.pad(4.0 * self.u, 4.0 * self.u).contains(p)
            }
        })
    }

    /// How far the stick on the screen is pushed (x right, y down, 0 .. 1 at the edge); nothing
    /// when no finger is on it or it is not shown.
    pub fn stick_analog(&self) -> Vec2 {
        match self.stick_at {
            Some((_, d)) if self.enabled && self.stick_r > 0.0 => (d / self.stick_r).clamp_length_max(1.0),
            _ => Vec2::ZERO,
        }
    }

    /// A finger holds a switch of the cockpit.
    pub fn on_switch(&self) -> bool {
        self.fingers.iter().any(|f| f.role == Role::Cockpit)
    }

    /// The window's graphics went away (a phone put the app in the background).
    pub fn drop_gpu(&mut self) {
        self.gpu = None;
        self.shot_gpu = None;
        self.atlas = Atlas::new(1024);
    }
}

const PANEL_BG: Color = Color::rgba(14, 16, 20, 0.62);
const PANEL_ON: Color = Color::rgba(232, 160, 48, 0.92);
const TEXT: Color = Color::rgba(240, 240, 240, 1.0);
const DIM: Color = Color::rgba(240, 240, 240, 0.55);

/// Where the information bar goes on a screen `w` wide whose buttons along the top leave the
/// gap from `left` to `right` (`pad` from the edges, the buttons' bottom at `bottom`): in the
/// gap, or under the buttons across the screen where the gap is too narrow for it.
fn info_room(w: f32, pad: f32, left: f32, right: f32, bottom: f32, u: f32) -> [f32; 3] {
    let (l, r) = (left + 8.0 * u, right - 8.0 * u);
    if r - l >= w * 0.35 {
        [l, r, pad]
    } else {
        [pad, w - pad, bottom + 8.0 * u]
    }
}

/// The cab panel's `n` buttons in a grid over the middle of a screen `w` x `h`: four across
/// where they fit (a tablet, a phone on its side), fewer and wider on a narrow screen. `r` is
/// the radius of the buttons along the top.
fn panel_rects(n: usize, w: f32, h: f32, pad: f32, r: f32, u: f32) -> Vec<Rect> {
    let gap = 8.0 * u;
    let mut cw = 150.0 * u;
    let ch = 64.0 * u;
    let room = (w - 2.0 * pad).max(cw.min(w));
    let cols = (((room + gap) / (cw + gap)).floor() as usize).clamp(1, 4);
    // (the columns share the width the screen leaves, a little wider than their own size at most)
    cw = ((room - (cols - 1) as f32 * gap) / cols as f32).min(cw * 1.3);
    let rows = n.div_ceil(cols).max(1);
    let pw = cols as f32 * cw + (cols - 1) as f32 * gap;
    let ph = rows as f32 * ch + (rows - 1) as f32 * gap;
    let x0 = (w - pw) * 0.5;
    let y0 = (pad + r * 2.0 + 16.0 * u).max((h - ph) * 0.42).min((h - ph - pad).max(pad));
    (0..n).map(|k| Rect::new(x0 + (k % cols) as f32 * (cw + gap), y0 + (k / cols) as f32 * (ch + gap), cw, ch)).collect()
}

impl App {
    /// Where the controls are, for a picture of `w` x `h` physical pixels, and what they
    /// show now.
    /// The controls inside the part of the screen nothing covers (an iPhone's island and
    /// rounded corners, see `platform::safe_insets`): laid out in it, then moved into place.
    fn touch_layout(&mut self, w: f32, h: f32) {
        let (left, top, right, bottom) = crate::platform::safe_insets();
        self.touch_layout_in((w - left - right).max(1.0), (h - top - bottom).max(1.0));
        let t = &mut self.input.touch;
        t.size = (w, h);
        if left == 0.0 && top == 0.0 {
            return;
        }
        let d = Vec2::new(left, top);
        for b in &mut t.buttons {
            b.rect.x += d.x;
            b.rect.y += d.y;
        }
        for r in [&mut t.throttle_r, &mut t.brake_r, &mut t.clutch_r] {
            r.x += d.x;
            r.y += d.y;
        }
        t.wheel_c += d;
        t.stick_c += d;
        if let Some(room) = t.info_room.as_mut() {
            room[0] += d.x;
            room[1] += d.x;
            room[2] += d.y;
        }
        if let Some(gap) = t.top_gap.as_mut() {
            gap[0] += d.x;
            gap[1] += d.x;
            gap[2] += d.y;
        }
    }

    fn touch_layout_in(&mut self, w: f32, h: f32) {
        let dpi = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0).max(0.5);
        // (a point is the screen's density, a little larger on a big tablet and smaller
        // on a small phone)
        let short = w.min(h) / dpi;
        let mut u = dpi * (short / 400.0).clamp(0.85, 1.35);
        // ... and no larger than the bottom row fits across: the wheel and the horn on the
        // left, the speed in the middle, the brakes, the pedals (a manual's clutch too) on
        // the right - in points of the layout below. On a tablet held upright (an iPad mini's
        // 744 points) the clutch pedal ran under the speed and the horn.
        let clutch = self.player.as_ref().is_some_and(|p| p.vehicle.ty.program.manual_gearbox()) && !self.settings.auto_clutch;
        let side = if clutch { 300.0 } else { 238.0 };
        // (only where that row is: the free camera and walking have a stick and a few buttons)
        if self.player.is_some() && self.photo.is_none() && matches!(self.view.as_str(), "driver" | "outside" | "pax") {
            u = u.min(w / (2.0 * (side + 67.0) + 28.0)).max(dpi * 0.7);
        }
        let pad = 14.0 * u;
        if self.photo.is_some() {
            self.touch_photo_layout(w, h, pad, u);
            return;
        }
        let t = &mut self.input.touch;
        t.u = u;
        t.size = (w, h);
        let mut b: Vec<Button> = Vec::new();
        let rb = |x: f32, y: f32, r: f32| Rect::new(x - r, y - r, r * 2.0, r * 2.0);
        let push = |b: &mut Vec<Button>, btn: Btn, rect: Rect, icon: &'static str, label: &str, on: bool, round: bool| {
            b.push(Button { btn, rect, icon, label: label.to_string(), on, round });
        };
        let menu_mode = self.menus.game_menu.is_some() || self.menus.chooser.is_some() || self.menus.navigator.as_ref().is_some_and(|n| n.map_open());
        t.top_gap = None;
        if menu_mode {
            // only the way out: the menu is worked with the fingers as a mouse
            let r = 24.0 * u;
            if self.menus.game_menu.is_some() || self.menus.chooser.is_some() {
                push(&mut b, Btn::CloseMenu, rb(w - pad - r, pad + r, r), "close", "", false, true);
            } else {
                push(&mut b, Btn::Map, rb(w - pad - r, pad + r, r), "close", "", false, true);
            }
            t.info_room = Some(info_room(w, pad, pad, w - pad - r * 2.0, pad + r * 2.0, u));
            t.buttons = b;
            return;
        }
        // --- the top bar
        // (the buttons shrink to fit the screen's width: on a narrow phone the two groups ran
        // into each other where the free camera and walking, with a larger `u`, had them)
        let count = 4.0 + if t.hidden { 1.0 } else if self.session.duty.is_some() { 5.0 } else { 4.0 };
        let fit = ((w - 2.0 * pad - 8.0 * u) / (count * 52.0 * u)).clamp(0.72, 1.0);
        let r = 21.0 * u * fit;
        let y = pad + r;
        let step = r * 2.0 + 10.0 * u * fit;
        let mut x = pad + r;
        for (btn, icon) in [(Btn::Menu, "menu"), (Btn::Pause, if self.paused { "play_arrow" } else { "pause" }), (Btn::Camera, "videocam"), (Btn::LookReset, "360")] {
            push(&mut b, btn, rb(x, y, r), icon, "", btn == Btn::Pause && self.paused, true);
            x += step;
        }
        let left_end = x - step + r;
        let mut x = w - pad - r;
        let hidden = t.hidden;
        push(&mut b, Btn::Hide, rb(x, y, r), if hidden { "visibility" } else { "visibility" }, "", hidden, true);
        // (the free camera's way back stays when the controls are hidden: nothing else of the
        // screen is a button then)
        let free = self.view == "free";
        if hidden {
            t.stick_r = 0.0;
            if free {
                push(&mut b, Btn::FreeCam, Rect::new(w - pad - 178.0 * u, h * 0.5 - 22.0 * u, 178.0 * u, 44.0 * u), "close", "Exit free camera", false, false);
            }
            t.info_room = Some(info_room(w, pad, left_end, x - r, pad + r * 2.0, u));
            t.buttons = b;
            return;
        }
        let driving = self.player.is_some() && matches!(self.view.as_str(), "driver" | "outside" | "pax");
        x -= step;
        push(&mut b, Btn::Panel, rb(x, y, r), "tune", "", t.panel, true);
        x -= step;
        push(&mut b, Btn::Map, rb(x, y, r), "map", "", false, true);
        if self.session.duty.is_some() {
            x -= step;
            push(&mut b, Btn::Timetable, rb(x, y, r), "departure_board", "", self.menus.timetable, true);
        }
        x -= step;
        push(&mut b, Btn::Screenshot, rb(x, y, r), "photo_camera", "", false, true);
        t.info_room = Some(info_room(w, pad, left_end, x - r, pad + r * 2.0, u));
        t.top_gap = Some([left_end + 6.0 * u, x - r - 6.0 * u, pad + r * 2.0]);
        if !driving {
            // on foot or the free camera: a stick to walk or fly
            t.stick_r = 62.0 * u;
            t.stick_c = Vec2::new(pad + t.stick_r + 10.0 * u, h - pad - t.stick_r - 10.0 * u);
            // on foot, out of the eyes: kneel for a picture from low down (#1148)
            if let Some(f) = self.session.on_foot.as_ref().filter(|f| f.cam == crate::on_foot::FootCam::First && f.seat.is_none()) {
                let kr = 26.0 * u;
                push(&mut b, Btn::Kneel, rb(w - pad - kr, h - pad - kr, kr), "keyboard_arrow_down", "", f.kneel, true);
            }
            if free {
                // the free camera: the stick moves it, a drag looks, a pinch zooms; up and
                // down and the speed beside them in the corner, the way back at the side
                // (what the walk of `cam.ego` has no use for: it keeps to the ground)
                if !t.panel {
                    push(&mut b, Btn::FreeCam, Rect::new(w - pad - 178.0 * u, h * 0.5 - 22.0 * u, 178.0 * u, 44.0 * u), "close", "Exit free camera", false, false);
                }
                if !self.cam.ego {
                    let fr = 26.0 * u;
                    let (x1, y0) = (w - pad - fr, h - pad - fr);
                    let (x0, y1) = (x1 - fr * 2.0 - 12.0 * u, y0 - fr * 2.0 - 12.0 * u);
                    push(&mut b, Btn::FlyDown, rb(x1, y0, fr), "keyboard_arrow_down", "", false, true);
                    push(&mut b, Btn::FlyUp, rb(x1, y1, fr), "keyboard_arrow_up", "", false, true);
                    push(&mut b, Btn::FlySlower, rb(x0, y0, fr), "remove", "", false, true);
                    push(&mut b, Btn::FlyFaster, rb(x0, y1, fr), "add", "", false, true);
                }
            }
        } else {
            t.stick_r = 0.0;
        }
        if driving {
            let p = self.player.as_ref().unwrap();
            let has = |name: &str| p.vehicle.ty.program.trigger(name).is_some() || p.bound_actions().iter().any(|a| a.eq_ignore_ascii_case(name));
            // --- the wheel, bottom left
            t.wheel_r = 78.0 * u;
            t.wheel_c = Vec2::new(pad + t.wheel_r + 6.0 * u, h - pad - t.wheel_r);
            // --- the pedals, bottom right
            // (as long as the settings make them, within what the screen's height leaves)
            let long = self.settings.touch_pedal_length.clamp(0.8, 1.6).min((h * 0.45 / (150.0 * u)).max(0.8));
            let th = 150.0 * u * long;
            t.throttle_r = Rect::new(w - pad - 64.0 * u, h - pad - th, 64.0 * u, th);
            let bh = 112.0 * u * long;
            t.brake_r = Rect::new(t.throttle_r.x - 14.0 * u - 80.0 * u, h - pad - bh, 80.0 * u, bh);
            // the gearbox above the pedals: an automatic's R N D, a sequential lever's - N +,
            // else a manual's whole gate - R, N and every gear its script has (`kw_s_1` ...),
            // in two rows as the H of the lever (R 1 3 5 over N 2 4 6)
            const MANUAL: [(&str, &str); 10] = [("kw_s_R", "R"), ("kw_s_N", "N"), ("kw_s_1", "1"), ("kw_s_2", "2"), ("kw_s_3", "3"), ("kw_s_4", "4"), ("kw_s_5", "5"), ("kw_s_6", "6"), ("kw_s_7", "7"), ("kw_s_8", "8")];
            // (the kind of gearbox by what the bus's own scripts answer to: every key of
            // the keyboard layout is bound whatever the bus, the automatic's D included)
            let scripted = |name: &str| p.vehicle.ty.program.trigger(name).is_some();
            // (a manual's scripts answer to the gear keys; the LiAZ's KPP has - and + too, and
            // triggers up to 10 whatever its box has: `antrieb_number_gears` says how many)
            // (a dashboard script answers to the automatic's keys for its own display on a
            // manual bus as well - the Sprinter W906 MT showed R N D, #279: a gearbox
            // script that reads the clutch pedal is a manual one)
            let program = &p.vehicle.ty.program;
            let manual = program.manual_gearbox();
            let count = p.vehicle.ty.program.constant("antrieb_number_gears").map(|n| n.round() as usize).filter(|n| (1..=8).contains(n));
            let gears: Vec<(&'static str, &'static str)> = if manual {
                MANUAL.iter().copied().enumerate().filter(|(k, (a, _))| {
                    matches!(*a, "kw_s_N") || (scripted(a) && count.is_none_or(|n| *k < n + 2))
                }).map(|(_, g)| g).collect()
            } else if has("automatic_D") {
                vec![("automatic_R", "R"), ("automatic_N", "N"), ("automatic_D", "D")]
            } else if has("kw_s_plus") {
                vec![("kw_s_minus", "-"), ("kw_s_N", "N"), ("kw_s_plus", "+")]
            } else {
                vec![("kw_s_R", "R"), ("kw_s_N", "N"), ("kw_s_1", "1")]
            };
            // (the top of the gearbox: the doors go above it)
            let gy;
            if manual && gears.len() > 3 {
                // the gear engaged, as the lever's script has it
                let engaged = p.vehicle.var("antrieb_getr_aktugang").or_else(|| p.vehicle.var("antrieb_getr_gang")).map(|g| g.round() as i32);
                let label_of = |g: i32| match g {
                    -1 => "R",
                    0 => "N",
                    g => MANUAL.get(g as usize + 1).map(|x| x.1).unwrap_or(""),
                };
                let cols = gears.len().div_ceil(2);
                let gw = (t.throttle_r.right() - t.brake_r.x) / cols as f32;
                let gh = 36.0 * u;
                let top = t.throttle_r.y - 10.0 * u - 2.0 * gh - 6.0 * u;
                gy = top;
                for (k, (action, letter)) in gears.iter().enumerate() {
                    let (col, row) = (k / 2, k % 2);
                    let on = match engaged {
                        Some(g) => label_of(g) == *letter,
                        None => t.gear == Some(*letter),
                    };
                    push(&mut b, Btn::Gear(action, letter), Rect::new(t.brake_r.x + gw * col as f32 + 3.0 * u, top + row as f32 * (gh + 6.0 * u), gw - 6.0 * u, gh), "", letter, on, false);
                }
            } else {
                let gw = (t.throttle_r.right() - t.brake_r.x) / 3.0;
                gy = t.throttle_r.y - 10.0 * u - 40.0 * u;
                for (k, (action, letter)) in gears.iter().enumerate() {
                    let on = t.gear == Some(*letter) && matches!(*letter, "R" | "N" | "D");
                    push(&mut b, Btn::Gear(action, letter), Rect::new(t.brake_r.x + gw * k as f32 + 3.0 * u, gy, gw - 6.0 * u, 40.0 * u), "", letter, on, false);
                }
            }
            // a manual without the automatic clutch of the settings: its clutch pedal, left of
            // the brake's buttons
            t.clutch_r = if manual && !self.settings.auto_clutch {
                let ch = 112.0 * u;
                Rect::new(t.brake_r.x - 14.0 * u - 50.0 * u - 14.0 * u - 64.0 * u, h - pad - ch, 64.0 * u, ch)
            } else {
                Rect::new(0.0, 0.0, 0.0, 0.0)
            };
            // the brakes left of the brake pedal
            let br = 25.0 * u;
            let bx = t.brake_r.x - 14.0 * u - br;
            let pb_on = p.vehicle.var("parkingbrake").or_else(|| p.vehicle.var("Handbremse")).or_else(|| p.vehicle.var("parking_brake")).is_some_and(|v| v > 0.5);
            push(&mut b, Btn::ParkingBrake, rb(bx, h - pad - br, br), "local_parking", "", pb_on, true);
            let sb_on = p.vehicle.var("bremse_halte_sw").or_else(|| p.vehicle.var("haltestellenbremse")).is_some_and(|v| v > 0.5);
            push(&mut b, Btn::StopBrake, rb(bx, h - pad - br * 3.0 - 10.0 * u, br), "back_hand", "", sb_on, true);
            // Preserve the current door detection in Standard mode. Direct mode
            // uses 0, 1 and combined 2+3; the fourth trigger button is not shown.
            let door_buttons: Vec<Btn> = if self.settings.touch_door_mode == "direct" {
                direct_door_buttons(p.vehicle.ty.program.triggers.keys().map(String::as_str))
                    .into_iter()
                    .map(Btn::DirectDoor)
                    .collect()
            } else {
                (1..=crate::player::door_keys(&p.vehicle.ty).len().clamp(1, 4))
                    .map(Btn::Door)
                    .collect()
            };
            let doors = door_buttons.len();
            let dr = 23.0 * u;
            let dy = gy - 12.0 * u - dr;
            for (k, action) in door_buttons.into_iter().enumerate() {
                let dx = t.throttle_r.right() - dr - (doors - 1 - k) as f32 * (dr * 2.0 + 10.0 * u);
                push(&mut b, action, rb(dx, dy, dr), "door_sliding", &(k + 1).to_string(), false, true);
            }
            // the indicators and the horn beside the wheel
            let ir = 23.0 * u;
            let iy = t.wheel_c.y - t.wheel_r - 14.0 * u - ir;
            let blink = p.vehicle.var("lights_sw_blinker").map(|v| v.round() as i32).unwrap_or(0);
            let warn = p.vehicle.var("lights_sw_warnblinker").is_some_and(|v| v > 0.5);
            push(&mut b, Btn::BlinkLeft, rb(t.wheel_c.x - t.wheel_r + ir, iy, ir), "turn_left", "", blink == 1, true);
            push(&mut b, Btn::Hazard, rb(t.wheel_c.x, iy, ir), "warning", "", warn, true);
            push(&mut b, Btn::BlinkRight, rb(t.wheel_c.x + t.wheel_r - ir, iy, ir), "turn_right", "", blink == 2, true);
            let hr = 26.0 * u;
            push(&mut b, Btn::Horn, rb(t.wheel_c.x + t.wheel_r + 18.0 * u + hr, h - pad - hr, hr), "campaign", "", false, true);
            // --- the cab panel: the rest of the switches, over the middle
            if t.panel {
                let items: Vec<(Btn, &'static str, &str, bool)> = vec![
                    (Btn::Battery, "power_settings_new", "Battery", p.vehicle.var("elec_busbar_main").or_else(|| p.vehicle.var("bat_switch")).is_some_and(|v| v > 0.5)),
                    (Btn::Engine, "key", "Engine start (hold)", p.vehicle.var("engine_on").is_some_and(|v| v > 0.5)),
                    (Btn::AutoStart, "autorenew", "Start the bus by itself", false),
                    (Btn::Headlights, "light", "Headlights", p.vehicle.var("lights_fern").or_else(|| p.vehicle.var("lights_sw_licht")).is_some_and(|v| v > 0.5)),
                    (Btn::HighBeam, "flashlight_on", "High beam", false),
                    (Btn::Wipers, "water_drop", "Wipers", p.vehicle.var("wiper_sw").is_some_and(|v| v > 0.5)),
                    (Btn::SaloonLights, "highlight", "Saloon lights", false),
                    (Btn::Ticket, "confirmation_number", "Sell ticket", false),
                    (Btn::Announce, "campaign", "Announcement (Q)", false),
                    (Btn::VoiceNearby, "record_voice_over", "Voice: nearby (hold)", t.voice_held[0]),
                    (Btn::VoiceRadio, "settings_voice", "Voice: radio (hold)", t.voice_held[1]),
                    (Btn::InteriorCam, "airline_seat_recline_normal", "Next seat view", false),
                    (Btn::FreeCam, "videocam", "Free camera", false),
                    (Btn::Navigator, "navigation", "Navigator", self.menus.navigator.as_ref().is_some_and(|n| n.enabled)),
                    (Btn::Info, "info", "Information bar", self.menus.info_bar),
                    (Btn::Tilt, "screen_rotation", "Tilt steering", t.tilt),
                ];
                // (the talk buttons only where voice chat is on, in a session)
                let voice_on = self.settings.voice_enabled && self.net.lan.is_some();
                let items: Vec<_> = items.into_iter().filter(|i| voice_on || !matches!(i.0, Btn::VoiceNearby | Btn::VoiceRadio)).collect();
                for (rect, (btn, icon, label, on)) in panel_rects(items.len(), w, h, pad, r, u).into_iter().zip(items) {
                    push(&mut b, btn, rect, icon, label, on, false);
                }
            }
        }
        // the free camera's own panel: the way back, the speed, what the cab panel has of the
        // screen's furniture
        if free && t.panel && !driving {
            let items: Vec<(Btn, &'static str, &str, bool)> = vec![
                (Btn::FreeCam, "close", "Exit free camera", false),
                (Btn::FlySlower, "remove", "Slower", false),
                (Btn::FlyFaster, "add", "Faster", false),
                (Btn::Navigator, "navigation", "Navigator", self.menus.navigator.as_ref().is_some_and(|n| n.enabled)),
                (Btn::Info, "info", "Information bar", self.menus.info_bar),
            ];
            for (rect, (btn, icon, label, on)) in panel_rects(items.len(), w, h, pad, r, u).into_iter().zip(items) {
                push(&mut b, btn, rect, icon, label, on, false);
            }
        }
        t.buttons = b;
    }

    /// A finger on the screen.
    pub(crate) fn on_touch(&mut self, event_loop: &ActiveEventLoop, f: Finger) {
        if !self.input.touch.enabled {
            return;
        }
        let p = Vec2::new(f.location.x as f32, f.location.y as f32);
        match f.phase {
            TouchPhase::Started => self.finger_down(event_loop, f.id, p),
            TouchPhase::Moved => self.finger_move(f.id, p),
            TouchPhase::Ended => self.finger_up(event_loop, f.id, p, false),
            TouchPhase::Cancelled => self.finger_up(event_loop, f.id, p, true),
        }
    }

    pub(crate) fn finger_down(&mut self, event_loop: &ActiveEventLoop, id: u64, p: Vec2) {
        self.input.touch.fingers.retain(|f| f.id != id);
        let menu_mode = self.menus.game_menu.is_some() || self.menus.chooser.is_some() || self.menus.navigator.as_ref().is_some_and(|n| n.map_open());
        let t = &self.input.touch;
        let role = if let Some(k) = t.button_at(p) {
            Role::Button(k, t.buttons[k].btn)
        } else if let Some(ph) = self.photo.as_ref() {
            let in_sheet = ph.touch_panel && !ph.hide_ui && ph.panel_rect.is_some_and(|r| p.x >= r[0] && p.x <= r[0] + r[2] && p.y >= r[1] && p.y <= r[1] + r[3]);
            if in_sheet {
                Role::Menu
            } else if t.stick_r > 0.0 && p.distance(t.stick_c) <= t.stick_r * 1.3 {
                Role::Stick
            } else {
                Role::Photo
            }
        } else if self.menus.game_menu.is_some() || self.menus.chooser.is_some() {
            Role::Menu
        } else if menu_mode {
            Role::Mouse
        } else if t.stick_r > 0.0 && p.distance(t.stick_c) <= t.stick_r * 1.3 {
            Role::Stick
        } else if self.player.is_some() && t.stick_r == 0.0 && !t.hidden && t.throttle_r.pad(6.0 * t.u, 6.0 * t.u).contains(p) {
            Role::Throttle
        } else if self.player.is_some() && t.stick_r == 0.0 && !t.hidden && t.brake_r.pad(6.0 * t.u, 6.0 * t.u).contains(p) {
            Role::Brake
        } else if self.player.is_some() && t.stick_r == 0.0 && !t.hidden && t.clutch_r.w > 0.0 && t.clutch_r.pad(6.0 * t.u, 6.0 * t.u).contains(p) {
            Role::Clutch
        } else if self.player.is_some() && t.stick_r == 0.0 && !t.hidden && !t.tilt && p.distance(t.wheel_c) <= t.wheel_r * 1.15 {
            {
                let d = p - t.wheel_c;
                Role::Wheel(d.y.atan2(d.x), d.length())
            }
        } else if self.game_panel_touch(p.x, p.y) {
            Role::Panel
        } else if self.menus.navigator.as_ref().is_some_and(|n| n.over_panel(p.x, p.y)) {
            Role::Navigator
        } else {
            // the cockpit's switch under the finger, else the camera's
            self.on_cursor(p.x, p.y);
            if self.menus.hover.is_some() && self.view == "driver" && self.input.touch.fingers.iter().all(|f| f.role != Role::Cockpit) {
                self.left_button(event_loop, true);
                Role::Cockpit
            } else {
                Role::Look
            }
        };
        match role {
            Role::Button(_, b) => {
                if b.held() {
                    self.touch_button(event_loop, b, true);
                }
            }
            Role::Mouse | Role::Navigator => {
                self.on_cursor(p.x, p.y);
                self.left_button(event_loop, true);
            }
            // (the line under the finger lights up; it is picked when the finger comes up
            // without having moved - a finger that came down to scroll used to pick the
            // line it landed on)
            Role::Menu if !self.input.touch.fingers.iter().any(|f| f.role == Role::MenuSlider) => self.on_cursor(p.x, p.y),
            Role::Wheel(..) => self.input.touch.steering = true,
            _ => {}
        }
        let moved = role == Role::Menu && self.input.touch.fingers.iter().any(|f| f.role == Role::MenuSlider);
        self.input.touch.fingers.push(Touched { id, start: p, pos: p, role, moved, since: Instant::now() });
        self.input.touch_pedals(p, self.settings.touch_pedal_deadzone);
        // two fingers on the picture: a pinch
        let looks: Vec<Vec2> = self.input.touch.fingers.iter().filter(|f| matches!(f.role, Role::Look | Role::Mouse | Role::Photo)).map(|f| f.pos).collect();
        if looks.len() == 2 {
            self.input.touch.pinch = Some(looks[0].distance(looks[1]).max(1.0));
        }
    }

    pub(crate) fn finger_move(&mut self, id: u64, p: Vec2) {
        let u = self.input.touch.u;
        let Some(k) = self.input.touch.fingers.iter().position(|f| f.id == id) else { return };
        let (last, start, was_moved) = (self.input.touch.fingers[k].pos, self.input.touch.fingers[k].start, self.input.touch.fingers[k].moved);
        {
            let f = &mut self.input.touch.fingers[k];
            f.pos = p;
            if !f.moved && p.distance(f.start) > 10.0 * u {
                f.moved = true;
            }
        }
        let role = self.input.touch.fingers[k].role;
        if role == Role::Menu && self.input.touch.fingers.iter().any(|f| f.role == Role::MenuSlider) { return; }
        let sideways = (p.x - start.x).abs() >= (p.y - start.y).abs();
        let grip = if role == Role::Menu && !was_moved && self.input.touch.fingers[k].moved && sideways && self.shell_takes_mouse() { self.shell.onto_slider(start, p) } else { None };
        if let Some(q) = grip {
            self.input.touch.fingers.iter_mut().filter(|f| f.role == Role::Menu).for_each(|f| f.moved = true);
            self.input.touch.fingers[k].role = Role::MenuSlider;
            self.on_cursor(q.x, q.y);
            self.shell.button(true);
            return;
        }
        match role {
            Role::Wheel(a0, _) => {
                // The wheel turns as far round as the finger goes round its centre - the
                // drawn rim stays under the finger, and the full lock is 120 degrees of it
                // (`touch_paint`). It used to follow the finger's way across instead, a drag
                // of one and a half diameters for the full lock: from its place in the corner
                // the finger ran off the screen at about half a lock to the left, the system
                // took the touch away and the wheel sprang back to the middle.
                // (close to the centre the angle means nothing: only followed)
                let d = p - self.input.touch.wheel_c;
                let a = d.y.atan2(d.x);
                if d.length() > self.input.touch.wheel_r * 0.2 {
                    let mut da = a - a0;
                    if da > std::f32::consts::PI {
                        da -= std::f32::consts::TAU;
                    } else if da < -std::f32::consts::PI {
                        da += std::f32::consts::TAU;
                    }
                    self.input.touch.steer = (self.input.touch.steer + da / touch_lock_angle(&self.settings)).clamp(-1.0, 1.0);
                }
                self.input.touch.fingers[k].role = Role::Wheel(a, d.length());
            }
            Role::Throttle | Role::Brake => self.input.touch_pedals(p, self.settings.touch_pedal_deadzone),
            Role::Stick => {
                let d = p - self.input.touch.stick_c;
                self.input.touch.stick_at = Some((self.input.touch.stick_c, d.clamp_length_max(self.input.touch.stick_r)));
                // (the free camera flies by how far the stick is pushed, see `Touch::stick_analog`;
                // walking is by the keys' steps)
                let flying = self.view == "free" && !self.cam.ego;
                self.stick_keys(if flying { Vec2::ZERO } else { d / self.input.touch.stick_r });
            }
            // (only a finger that has gone a way drags it: one wavering on a tap still opens
            // the city map)
            Role::Navigator => {
                if self.input.touch.fingers[k].moved {
                    self.on_cursor(p.x, p.y);
                }
            }
            Role::Panel => {
                self.input.cursor = (p.x, p.y);
                self.game_panel_drag(p.x);
            }
            Role::MenuSlider => {
                let q = self.shell.onto_slider(start, p).unwrap_or(p);
                self.on_cursor(q.x, q.y);
            }
            Role::Cockpit | Role::Mouse => {
                if let (Role::Mouse, Some(_)) = (role, self.input.touch.pinch) {
                    return self.touch_pinch();
                }
                self.on_cursor(p.x, p.y);
            }
            // The shell's pause menu owns its scroll areas (rail, pages and lists).
            // The legacy menu_top belongs to the old renderer, so moving it here no
            // longer scrolls anything on Android (#2108).
            Role::Menu => {
                if self.input.touch.fingers[k].moved {
                    if self.shell_takes_mouse() {
                        // Keep the pointer under the finger: the shell sends the
                        // wheel to whichever scroll area was touched, including
                        // nested lists. Dragging upwards must scroll down.
                        self.shell.pointer(p.x, p.y);
                        self.shell.wheel(touch_menu_scroll_lines(p.y - last.y, self.shell.scale));
                    } else {
                        // Legacy UI / VR still uses menu_top, in rows rather than pixels.
                        self.on_cursor(-1e4, -1e4);
                        let (start, row_h) = self.ui.as_ref().map(|u| (u.menu_start as f32, u.menu_row_h.max(1.0))).unwrap_or((0.0, 1.0));
                        let top = self.menus.menu_top.unwrap_or(start) - (p.y - last.y) / row_h;
                        let (n, rows) = (self.menu_len() as f32, self.ui.as_ref().map(|u| u.menu_rows).unwrap_or(0) as f32);
                        self.menus.menu_top = Some(top.clamp(0.0, (n - rows).max(0.0)));
                    }
                } else {
                    self.on_cursor(p.x, p.y);
                }
            }
            Role::Photo => {
                if self.input.touch.pinch.is_some() {
                    return self.touch_pinch();
                }
                if self.input.touch.fingers[k].moved {
                    self.photo_look_touch(p.x - last.x, p.y - last.y);
                }
            }
            Role::Look => {
                if self.input.touch.pinch.is_some() {
                    return self.touch_pinch();
                }
                if self.input.touch.fingers[k].moved {
                    // (degrees for a point dragged: a full turn is a few swipes)
                    let k = 0.28 / u * self.settings.look_sens;
                    // (the view turns the way the finger moves: taken the other way round,
                    // as grabbing the world, every direction felt inverted)
                    self.look_by((p.x - last.x) * k, (p.y - last.y) * k);
                }
            }
            _ => {}
        }
    }

    fn touch_pinch(&mut self) {
        let looks: Vec<Vec2> = self.input.touch.fingers.iter().filter(|f| matches!(f.role, Role::Look | Role::Mouse | Role::Photo)).map(|f| f.pos).collect();
        if looks.len() < 2 {
            return;
        }
        let d = looks[0].distance(looks[1]).max(1.0);
        if let Some(d0) = self.input.touch.pinch {
            let amount = (d - d0) / (28.0 * self.input.touch.u);
            if amount.abs() > 0.05 {
                if let Some(ph) = self.photo.as_mut() {
                    ph.zoom(amount);
                } else {
                    self.wheel(amount);
                }
                self.input.touch.pinch = Some(d);
            }
        }
    }

    pub(crate) fn finger_up(&mut self, event_loop: &ActiveEventLoop, id: u64, p: Vec2, cancelled: bool) {
        let Some(k) = self.input.touch.fingers.iter().position(|f| f.id == id) else { return };
        let f = self.input.touch.fingers.remove(k);
        if self.input.touch.fingers.iter().filter(|f| matches!(f.role, Role::Look | Role::Mouse | Role::Photo)).count() < 2 {
            self.input.touch.pinch = None;
        }
        match f.role {
            Role::Button(_, b) => {
                if b.held() {
                    self.touch_button(event_loop, b, false);
                } else if !cancelled && self.input.touch.button_at(p).and_then(|k| self.input.touch.buttons.get(k)).map(|x| x.btn) == Some(b) {
                    self.touch_button(event_loop, b, true);
                }
            }
            Role::Wheel(..) => self.input.touch.steering = self.input.touch.fingers.iter().any(|f| matches!(f.role, Role::Wheel(..))),
            Role::Throttle => self.input.touch.throttle = 0.0,
            Role::Brake => self.input.touch.brake = 0.0,
            Role::Clutch => self.input.touch.clutch = 0.0,
            Role::Stick => {
                self.input.touch.stick_at = None;
                self.stick_keys(Vec2::ZERO);
            }
            Role::Cockpit | Role::Mouse => {
                self.on_cursor(p.x, p.y);
                self.left_button(event_loop, false);
            }
            Role::Panel => {
                self.game_panel_click(false);
            }
            Role::Navigator => {
                if f.moved {
                    self.on_cursor(p.x, p.y);
                }
                match (cancelled, f.moved, self.menus.navigator.as_mut()) {
                    // (a tap taken away by the system opens nothing)
                    (true, false, Some(n)) => {
                        n.panel_release();
                    }
                    _ => self.left_button(event_loop, false),
                }
            }
            Role::MenuSlider => {
                let at = if cancelled { Vec2::splat(-1e4) } else { self.shell.onto_slider(f.start, p).unwrap_or(p) };
                self.on_cursor(at.x, at.y);
                self.shell.button(false);
            }
            Role::Menu => {
                if !f.moved && !cancelled {
                    self.on_cursor(p.x, p.y);
                    self.left_button(event_loop, true);
                    self.left_button(event_loop, false);
                } else if !self.shell_takes_mouse() {
                    // Only the legacy menu scrolls by whole rows; the shell's
                    // scroll areas retain their own smooth pixel positions.
                    self.menus.menu_top = self.menus.menu_top.map(f32::round);
                }
            }
            Role::Photo => {
                if !f.moved && !cancelled && f.since.elapsed().as_secs_f32() < 0.45 && self.input.touch.fingers.is_empty() {
                    let (w, h) = self.input.touch.size;
                    self.photo_request(crate::photo::Request::FocusAt(p.x / w.max(1.0), p.y / h.max(1.0)));
                }
            }
            Role::Look => {
                // a tap on the picture: a click there (a switch the finger missed by a hair
                // is still found: the cursor's pick is generous)
                if !f.moved && !cancelled && f.since.elapsed().as_secs_f32() < 0.45 && self.input.touch.fingers.is_empty() {
                    self.on_cursor(p.x, p.y);
                    self.left_button(event_loop, true);
                    self.left_button(event_loop, false);
                }
            }
        }
    }

    /// The stick as W A S D (free camera, on foot); pushed far, Shift runs.
    fn stick_keys(&mut self, d: Vec2) {
        let mut want = Vec::new();
        if d.y < -0.35 {
            want.push(KeyCode::KeyW);
        }
        if d.y > 0.35 {
            want.push(KeyCode::KeyS);
        }
        if d.x < -0.35 {
            want.push(KeyCode::KeyA);
        }
        if d.x > 0.35 {
            want.push(KeyCode::KeyD);
        }
        if d.length() > 0.95 {
            want.push(KeyCode::ShiftLeft);
        }
        for k in self.input.touch.stick_keys.clone() {
            if !want.contains(&k) {
                self.input.keys.remove(&k);
            }
        }
        for k in &want {
            self.input.keys.insert(*k);
        }
        self.input.touch.stick_keys = want;
    }

    /// A key tapped (with Shift held when `shift`), through the keyboard's own handler.
    fn tap_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode, shift: bool, pressed: bool) {
        if shift {
            self.input.keys.insert(KeyCode::ShiftLeft);
        }
        self.on_key(event_loop, code, pressed, false);
        if shift {
            self.input.keys.remove(&KeyCode::ShiftLeft);
        }
    }

    /// A vehicle action of `Inputs/keyboard.cfg` (the axes' own, or the scripts').
    fn vehicle_action(&mut self, name: &str, down: bool) {
        if let Some(p) = self.player.as_mut() {
            if let Some(a) = omsi_sim::engine_action(name) {
                p.axes.set(a, down);
            } else {
                p.action(name, down);
            }
        }
    }

    /// Back from the free camera to the driver's seat (F1): the fingers' keys let go.
    fn leave_free_camera(&mut self) {
        self.game_action("view_set_driver");
        self.stick_keys(Vec2::ZERO);
        self.input.touch.stick_at = None;
        for key in [KeyCode::KeyE, KeyCode::KeyQ] {
            self.input.keys.remove(&key);
        }
        self.touch_note("Driver's view");
    }

    fn touch_note(&mut self, text: &str) {
        self.input.touch.note = Some((text.to_string(), 1.6));
    }

    fn touch_button(&mut self, event_loop: &ActiveEventLoop, b: Btn, down: bool) {
        if down {
            crate::platform::buzz(12);
        }
        match b {
            Btn::Menu => {
                self.input.touch.panel = false;
                self.open_game_menu();
            }
            Btn::CloseMenu => {
                if self.menus.chooser.is_some() {
                    self.tap_key(event_loop, KeyCode::Escape, false, true);
                } else {
                    self.close_game_menu();
                }
            }
            Btn::Pause => self.toggle_pause(),
            Btn::Kneel => self.kneel(),
            Btn::FreeCam => {
                self.input.touch.panel = false;
                if self.view == "free" {
                    self.leave_free_camera();
                } else if self.player.is_some() {
                    self.game_action("view_set_map");
                    self.touch_note("Free camera: the stick moves, a drag looks, two fingers zoom");
                }
            }
            Btn::PhotoExit => self.photo_request(crate::photo::Request::Exit),
            Btn::PhotoHide => self.photo_request(crate::photo::Request::HideUi),
            Btn::PhotoShutter => {
                if self.photo.as_ref().is_some_and(|p| !p.render.capture) {
                    self.photo_request(crate::photo::Request::Take);
                }
            }
            Btn::PhotoPanel => {
                if let Some(ph) = self.photo.as_mut() {
                    ph.touch_panel = !ph.touch_panel;
                }
            }
            Btn::FlyUp | Btn::FlyDown if self.photo.is_some() => {
                let z = if b == Btn::FlyUp { 1.0 } else { -1.0 };
                if let Some(ph) = self.photo.as_mut() {
                    ph.touch_z = if down { z } else { 0.0 };
                }
            }
            Btn::FlyFaster if self.photo.is_some() => {
                if let Some(ph) = self.photo.as_mut() {
                    ph.touch_fast = !ph.touch_fast;
                }
            }
            Btn::FlyUp | Btn::FlyDown => {
                let key = if b == Btn::FlyUp { KeyCode::KeyE } else { KeyCode::KeyQ };
                if down {
                    self.input.keys.insert(key);
                } else {
                    self.input.keys.remove(&key);
                }
            }
            Btn::FlySlower | Btn::FlyFaster => {
                let k = if b == Btn::FlyFaster { 2.0 } else { 0.5 };
                self.cam.speed = (self.cam.speed * k).clamp(2.0, 480.0);
                let note = format!("Free camera speed {:.0} m/s", self.cam.speed);
                self.touch_note(&note);
            }
            Btn::Camera if self.view == "free" => self.leave_free_camera(),
            Btn::Camera => {
                // driver → outside → passenger → driver (on foot: back to the bus)
                self.view = match self.view.as_str() {
                    "driver" => "outside",
                    "outside" if self.player.as_ref().is_some_and(|p| p.pax_camera_count() > 0) => "pax",
                    _ => "driver",
                }
                .into();
                let v = match self.view.as_str() {
                    "driver" => "Driver's view",
                    "outside" => "Outside view",
                    _ => "Passenger's view",
                };
                self.touch_note(v);
            }
            Btn::LookReset => {
                self.game_action("view_reset_direction");
                self.cam.orbit = ORBIT_DEFAULT;
            }
            Btn::Map => {
                if let Some(n) = self.menus.navigator.as_mut() {
                    n.enabled = true;
                    n.toggle_map();
                }
            }
            Btn::Timetable => self.menus.timetable = !self.menus.timetable,
            Btn::Panel => self.input.touch.panel = !self.input.touch.panel,
            Btn::Hide => {
                self.input.touch.hidden = !self.input.touch.hidden;
                self.input.touch.panel = false;
            }
            Btn::Screenshot => self.take_screenshot(),
            Btn::Door(n) => {
                let code = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4][(n - 1).min(3)];
                self.tap_key(event_loop, code, down, down);
            }
            Btn::DirectDoor(group) => {
                if let Some(names) = DIRECT_DOOR_GROUPS.get(group) {
                    for name in *names {
                        // A partial group is valid for buses exposing only one trigger.
                        if self.player.as_ref().is_some_and(|p| p.vehicle.ty.program.trigger(name).is_some()) {
                            self.vehicle_action(name, down);
                        }
                    }
                }
            }
            Btn::StopBrake => {
                self.vehicle_action("bus_dooraft", true);
                self.vehicle_action("bus_dooraft", false);
            }
            Btn::ParkingBrake => {
                self.vehicle_action("parking_brake_toggle", true);
                self.vehicle_action("parking_brake_toggle", false);
            }
            Btn::Gear(action, letter) => {
                self.input.touch.gear = Some(letter);
                self.vehicle_action(action, true);
                self.vehicle_action(action, false);
            }
            // (the lever itself, not the Z / C / X keys: those are the simple layouts' only,
            // and with OMSI's keys chosen the phone's indicator buttons did nothing)
            Btn::BlinkLeft => self.blinker(1),
            Btn::BlinkRight => self.blinker(2),
            Btn::Hazard => self.blinker(3),
            Btn::Horn => self.vehicle_action("horn", down),
            Btn::Engine => self.vehicle_action("kw_m_enginestart", down),
            Btn::Battery => {
                self.vehicle_action("cp_batterietrennschalter_toggle", true);
                self.vehicle_action("cp_batterietrennschalter_toggle", false);
            }
            Btn::AutoStart => {
                self.tap_key(event_loop, KeyCode::KeyU, true, true);
                self.tap_key(event_loop, KeyCode::KeyU, true, false);
                self.input.touch.panel = false;
            }
            Btn::Headlights => {
                self.vehicle_action("kw_scheinwerfer_toggle", true);
                self.vehicle_action("kw_scheinwerfer_toggle", false);
            }
            Btn::HighBeam => {
                self.vehicle_action("kw_fernlicht_toggle", true);
                self.vehicle_action("kw_fernlicht_toggle", false);
            }
            Btn::Wipers => {
                self.vehicle_action("kw_wipermode_up", true);
                self.vehicle_action("kw_wipermode_up", false);
            }
            Btn::SaloonLights => {
                self.tap_key(event_loop, KeyCode::KeyI, false, true);
                self.tap_key(event_loop, KeyCode::KeyI, false, false);
            }
            Btn::Ticket => {
                self.vehicle_action("ticket_give", true);
                self.vehicle_action("ticket_give", false);
            }
            Btn::Announce => self.tap_key(event_loop, KeyCode::KeyQ, false, down),
            Btn::VoiceNearby => self.input.touch.voice_held[0] = down,
            Btn::VoiceRadio => self.input.touch.voice_held[1] = down,
            Btn::InteriorCam => {
                self.game_action("view_interiorcam_plus");
            }
            Btn::Navigator => {
                if let Some(n) = self.menus.navigator.as_mut() {
                    n.enabled = !n.enabled;
                }
            }
            Btn::Info => self.set_info_bar(!self.menus.info_bar),
            Btn::Tilt => {
                self.input.touch.tilt = !self.input.touch.tilt;
                crate::platform::set_tilt(self.input.touch.tilt);
                let t = if self.input.touch.tilt { "Tilt steering on: hold the phone like a wheel" } else { "Tilt steering off" };
                self.touch_note(t);
            }
        }
    }

    /// Once a frame before the bus moves: the wheel and the pedals as the controller's axes
    /// (the stronger of the two for the pedals), and the wheel coming back when let go.
    pub(crate) fn touch_frame(&mut self, dt: f32) {
        if !self.input.touch.enabled {
            return;
        }
        let t = &mut self.input.touch;
        if t.tilt {
            if let Some(s) = crate::platform::tilt_steering() {
                t.steer = s;
            }
        } else if !t.steering {
            // let go: back as the bus's wheel comes back by itself with the keys - the castor's
            // pull, slow standing and brisker rolling (OMSI's steady pace with Steering
            // linearity, less at speed with Dynamic steering), not at all with Old Steering;
            // it sprang back to the middle in a third of a second whatever the speed (#1090)
            t.steer = match self.player.as_ref() {
                Some(p) => p.axes.let_go(t.steer, dt),
                None => 0.0,
            };
        }
        if let Some((_, left)) = t.note.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                t.note = None;
            }
        }
        let (steer, thr, brk, active) = (steer_curve(t.steer), t.throttle, t.brake, t.steering || t.tilt || t.steer != 0.0);
        let clu = t.clutch;
        if let Some(p) = self.player.as_mut() {
            let a = &mut p.analog;
            if active {
                a.steering = Some(steer);
            }
            if thr > 0.0 {
                a.throttle = Some(a.throttle.unwrap_or(0.0).max(thr));
            }
            if brk > 0.0 {
                a.brake = Some(a.brake.unwrap_or(0.0).max(brk));
            }
            if clu > 0.0 {
                a.clutch = Some(a.clutch.unwrap_or(0.0).max(clu));
            }
        }
    }

    /// Lay the controls out for a frame of `w` x `h` and paint them (drawn by
    /// `Touch::render` once the game's picture is in the frame).
    pub(crate) fn touch_prepare(&mut self, w: u32, h: u32) {
        if !self.input.touch.enabled {
            return;
        }
        self.touch_layout(w as f32, h as f32);
        self.touch_paint();
    }

    /// `touch down|move|up x,y [id]` of an input script: a finger, through the same path.
    pub(crate) fn script_touch(&mut self, event_loop: &ActiveEventLoop, verb: &str, x: f32, y: f32, id: u64) {
        let p = Vec2::new(x, y);
        match verb {
            "down" => self.finger_down(event_loop, id, p),
            "move" => self.finger_move(id, p),
            _ => self.finger_up(event_loop, id, p, false),
        }
    }
}

impl Touch {
    /// Draw the controls painted last over the frame (`view`, `w` x `h`).
    pub(crate) fn render(&mut self, r: &Renderer, view: &wgpu::TextureView, w: u32, h: u32) {
        if !self.enabled || self.painter.is_empty() {
            return;
        }
        let format = r.format();
        if self.gpu.as_ref().map(|g| g.1 != format).unwrap_or(true) {
            self.gpu = Some((Gpu::new(&r.device, format, 1, self.atlas.size), format));
            self.atlas.mark_all_dirty();
        }
        let Some((gpu, _)) = self.gpu.as_mut() else { return };
        gpu.upload(&r.device, &r.queue, 0, &self.painter.verts);
        gpu.upload_atlas(&r.queue, &mut self.atlas);
        let layers = [Layer::flat([0.0, 0.0, w as f32, h as f32], 0.0, 1.0)];
        let draws = [Draw { buffer: 0, range: 0..self.painter.len(), layer: 0, texture: 0 }];
        let mut enc = r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("touch controls") });
        gpu.render(&r.device, &r.queue, &mut enc, view, (w, h), None, &layers, &draws);
        r.queue.submit([enc.finish()]);
    }

    /// The controls painted last as an RGBA picture (premultiplied) of `w` x `h`, for the
    /// `shot` pictures of an input script.
    pub(crate) fn picture(&mut self, r: &Renderer, w: u32, h: u32) -> Option<Vec<u8>> {
        if !self.enabled || self.painter.is_empty() {
            return None;
        }
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let size = self.atlas.size;
        let gpu = self.shot_gpu.get_or_insert_with(|| Gpu::new(&r.device, format, 1, size));
        gpu.upload(&r.device, &r.queue, 0, &self.painter.verts);
        self.atlas.mark_all_dirty();
        gpu.upload_atlas(&r.queue, &mut self.atlas);
        // (the window's pipeline has the atlas sent again next frame)
        self.atlas.mark_all_dirty();
        let tex = r.device.create_texture(&wgpu::TextureDescriptor { label: Some("touch shot"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let view = tex.create_view(&Default::default());
        let layers = [Layer::flat([0.0, 0.0, w as f32, h as f32], 0.0, 1.0)];
        let draws = [Draw { buffer: 0, range: 0..self.painter.len(), layer: 0, texture: 0 }];
        let mut enc = r.device.create_command_encoder(&Default::default());
        gpu.render(&r.device, &r.queue, &mut enc, &view, (w, h), Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        let stride = (w * 4).div_ceil(256) * 256;
        let buf = r.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        enc.copy_texture_to_buffer(tex.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        r.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        omsi_render::wait_gpu(&r.device, None).ok();
        let data = buf.slice(..).get_mapped_range();
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            let row = &data[y * stride as usize..y * stride as usize + w as usize * 4];
            out[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(row);
        }
        Some(out)
    }
}

/// `over` (premultiplied RGBA) laid over `base` (RGBA), for a picture with the controls.
pub(crate) fn composite(base: &mut [u8], over: &[u8]) {
    for (b, o) in base.chunks_exact_mut(4).zip(over.chunks_exact(4)) {
        let a = o[3] as f32 / 255.0;
        for c in 0..3 {
            b[c] = (o[c] as f32 + b[c] as f32 * (1.0 - a)).round().min(255.0) as u8;
        }
    }
}

/// How far the wheel on the screen turns from the middle to the full lock (rad): half the
/// settings' wheel turn lock to lock - "Full lock at" where it is set, else the whole
/// "Wheel rotation" (900 degrees unless changed: one and a quarter turns each way, as a bus's
/// wheel and the phone bus games have it; the finger goes round and round to the lock, and
/// the wheel comes back by itself when let go). The settings did nothing on a phone, so the
/// drawn wheel could not be made to turn as the bus's own (#856). (120 degrees was a lock in
/// a flick.)
fn touch_lock_angle(s: &crate::settings::Settings) -> f32 {
    let lock_to_lock = if s.wheel_lock >= 45.0 { s.wheel_lock } else { s.wheel_range };
    (lock_to_lock.clamp(90.0, 2880.0) * 0.5).to_radians()
}

/// Convert a vertical finger drag (physical pixels, down positive) to the
/// shell toolkit's wheel units. The toolkit scrolls 42 logical pixels per unit;
/// preserving the entire displacement also works with high-DPI screens.
fn touch_menu_scroll_lines(delta_y: f32, scale: f32) -> f32 {
    delta_y / (42.0 * scale.max(0.25))
}

/// The wheel's turn as the bus gets it: one to one, the drawn wheel and the bus's wheel
/// turn alike (a curve that was gentle round the middle made the bus turn faster and
/// faster as the finger went on round).
fn steer_curve(s: f32) -> f32 {
    s
}

/// How far a screen pedal is pressed with a finger at `at` of its height (0 its foot, 1 its
/// top): nothing in the rest zone, then a curve that gives the first part of the travel to
/// gentle presses (a car's pedal does the same with its progressive spring) - a soft brake
/// is a third of the way up, full braking the top.
fn pedal_press(at: f32, rest: f32) -> f32 {
    let u = ((at - rest) / (1.0 - rest).max(0.05)).clamp(0.0, 1.0);
    u.powf(1.6)
}

impl InputState {
    /// The pedals from where the fingers on them are: the higher up the pedal, the harder
    /// it is pressed - nothing at all in the rest zone at its foot (#1458: a finger put on the
    /// brake pressed a fifth of it at once, a hard stop where a gentle one was meant).
    /// `rest`: the rest zone's share of the pedal (Settings, `touch_pedal_deadzone`).
    fn touch_pedals(&mut self, _p: Vec2, rest: f32) {
        let rest = rest.clamp(0.0, 0.4);
        let t = &mut self.touch;
        let depth = |r: Rect, y: f32| pedal_press(((r.bottom() - y) / r.h.max(1.0)).clamp(0.0, 1.0), rest);
        for f in &t.fingers {
            match f.role {
                Role::Throttle => t.throttle = depth(t.throttle_r, f.pos.y),
                Role::Brake => t.brake = depth(t.brake_r, f.pos.y),
                // (the clutch pushed in most of the way is in: the gearboxes want it at 1
                // to take a gear, see `pedal_ends`, and a thumb seldom sits at the top edge)
                // (the clutch as it was: no rest zone, straight up the pedal)
                Role::Clutch => {
                    let c = (0.2 + 0.8 * ((t.clutch_r.bottom() - f.pos.y) / t.clutch_r.h.max(1.0))).clamp(0.2, 1.0);
                    t.clutch = if c >= 0.75 { 1.0 } else { c };
                }
                _ => {}
            }
        }
    }
}
