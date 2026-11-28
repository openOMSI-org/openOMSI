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

mod layout;
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
    Sit,
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
        matches!(
            self,
            Btn::Door(_)
                | Btn::DirectDoor(_)
                | Btn::Horn
                | Btn::Engine
                | Btn::Announce
                | Btn::VoiceNearby
                | Btn::VoiceRadio
                | Btn::FlyUp
                | Btn::FlyDown
        )
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
        .filter(|(_, group)| {
            group
                .iter()
                .any(|cmd| names.iter().any(|n| n.eq_ignore_ascii_case(cmd)))
        })
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
    /// The cursor the game draws itself (#1247): its kind, its place in physical pixels and
    /// the interface's scale; `None` when the player uses the system's cursor.
    pub(crate) cursor: Option<(u8, Vec2, f32)>,
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
        self.fingers
            .iter()
            .any(|f| matches!(f.role, Role::Cockpit | Role::Mouse))
    }

    pub fn new() -> Touch {
        Touch {
            enabled: crate::platform::touch_controls(),
            cursor: None,
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
            Some((_, d)) if self.enabled && self.stick_r > 0.0 => {
                (d / self.stick_r).clamp_length_max(1.0)
            }
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
    let y0 = (pad + r * 2.0 + 16.0 * u)
        .max((h - ph) * 0.42)
        .min((h - ph - pad).max(pad));
    (0..n)
        .map(|k| {
            Rect::new(
                x0 + (k % cols) as f32 * (cw + gap),
                y0 + (k / cols) as f32 * (ch + gap),
                cw,
                ch,
            )
        })
        .collect()
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
        let menu_mode = self.menus.game_menu.is_some()
            || self.menus.chooser.is_some()
            || self.menus.navigator.as_ref().is_some_and(|n| n.map_open());
        let t = &self.input.touch;
        let role = if let Some(k) = t.button_at(p) {
            Role::Button(k, t.buttons[k].btn)
        } else if let Some(ph) = self.photo.as_ref() {
            let in_sheet = ph.touch_panel
                && !ph.hide_ui
                && ph.panel_rect.is_some_and(|r| {
                    p.x >= r[0] && p.x <= r[0] + r[2] && p.y >= r[1] && p.y <= r[1] + r[3]
                });
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
        } else if self.player.is_some()
            && t.stick_r == 0.0
            && !t.hidden
            && t.throttle_r.pad(6.0 * t.u, 6.0 * t.u).contains(p)
        {
            Role::Throttle
        } else if self.player.is_some()
            && t.stick_r == 0.0
            && !t.hidden
            && t.brake_r.pad(6.0 * t.u, 6.0 * t.u).contains(p)
        {
            Role::Brake
        } else if self.player.is_some()
            && t.stick_r == 0.0
            && !t.hidden
            && t.clutch_r.w > 0.0
            && t.clutch_r.pad(6.0 * t.u, 6.0 * t.u).contains(p)
        {
            Role::Clutch
        } else if self.player.is_some()
            && t.stick_r == 0.0
            && !t.hidden
            && !t.tilt
            && p.distance(t.wheel_c) <= t.wheel_r * 1.15
        {
            {
                let d = p - t.wheel_c;
                Role::Wheel(d.y.atan2(d.x), d.length())
            }
        } else if self.game_panel_touch(p.x, p.y) {
            Role::Panel
        } else if self
            .menus
            .navigator
            .as_ref()
            .is_some_and(|n| n.over_panel(p.x, p.y))
        {
            Role::Navigator
        } else {
            // the cockpit's switch under the finger, else the camera's
            self.on_cursor(p.x, p.y);
            if self.menus.hover.is_some()
                && self.view == "driver"
                && self
                    .input
                    .touch
                    .fingers
                    .iter()
                    .all(|f| f.role != Role::Cockpit)
            {
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
            Role::Menu
                if !self
                    .input
                    .touch
                    .fingers
                    .iter()
                    .any(|f| f.role == Role::MenuSlider) =>
            {
                self.on_cursor(p.x, p.y)
            }
            Role::Wheel(..) => self.input.touch.steering = true,
            _ => {}
        }
        let moved = role == Role::Menu
            && self
                .input
                .touch
                .fingers
                .iter()
                .any(|f| f.role == Role::MenuSlider);
        self.input.touch.fingers.push(Touched {
            id,
            start: p,
            pos: p,
            role,
            moved,
            since: Instant::now(),
        });
        self.input
            .touch_pedals(p, self.settings.touch_pedal_deadzone);
        // two fingers on the picture: a pinch
        let looks: Vec<Vec2> = self
            .input
            .touch
            .fingers
            .iter()
            .filter(|f| matches!(f.role, Role::Look | Role::Mouse | Role::Photo))
            .map(|f| f.pos)
            .collect();
        if looks.len() == 2 {
            self.input.touch.pinch = Some(looks[0].distance(looks[1]).max(1.0));
        }
    }

    pub(crate) fn finger_move(&mut self, id: u64, p: Vec2) {
        let u = self.input.touch.u;
        let Some(k) = self.input.touch.fingers.iter().position(|f| f.id == id) else {
            return;
        };
        let (last, start, was_moved) = (
            self.input.touch.fingers[k].pos,
            self.input.touch.fingers[k].start,
            self.input.touch.fingers[k].moved,
        );
        {
            let f = &mut self.input.touch.fingers[k];
            f.pos = p;
            if !f.moved && p.distance(f.start) > 10.0 * u {
                f.moved = true;
            }
        }
        let role = self.input.touch.fingers[k].role;
        if role == Role::Menu
            && self
                .input
                .touch
                .fingers
                .iter()
                .any(|f| f.role == Role::MenuSlider)
        {
            return;
        }
        let sideways = (p.x - start.x).abs() >= (p.y - start.y).abs();
        let grip = if role == Role::Menu
            && !was_moved
            && self.input.touch.fingers[k].moved
            && sideways
            && self.shell_takes_mouse()
        {
            self.shell.onto_slider(start, p)
        } else {
            None
        };
        if let Some(q) = grip {
            self.input
                .touch
                .fingers
                .iter_mut()
                .filter(|f| f.role == Role::Menu)
                .for_each(|f| f.moved = true);
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
                    self.input.touch.steer = (self.input.touch.steer
                        + da / touch_lock_angle(&self.settings))
                    .clamp(-1.0, 1.0);
                }
                self.input.touch.fingers[k].role = Role::Wheel(a, d.length());
            }
            Role::Throttle | Role::Brake => self
                .input
                .touch_pedals(p, self.settings.touch_pedal_deadzone),
            Role::Stick => {
                let d = p - self.input.touch.stick_c;
                self.input.touch.stick_at = Some((
                    self.input.touch.stick_c,
                    d.clamp_length_max(self.input.touch.stick_r),
                ));
                // (the free camera flies by how far the stick is pushed, see `Touch::stick_analog`;
                // walking is by the keys' steps)
                let flying = self.view == "free" && !self.cam.ego;
                self.stick_keys(if flying {
                    Vec2::ZERO
                } else {
                    d / self.input.touch.stick_r
                });
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
                        self.shell
                            .wheel(touch_menu_scroll_lines(p.y - last.y, self.shell.scale));
                    } else {
                        // Legacy UI / VR still uses menu_top, in rows rather than pixels.
                        self.on_cursor(-1e4, -1e4);
                        let (start, row_h) = self
                            .ui
                            .as_ref()
                            .map(|u| (u.menu_start as f32, u.menu_row_h.max(1.0)))
                            .unwrap_or((0.0, 1.0));
                        let top = self.menus.menu_top.unwrap_or(start) - (p.y - last.y) / row_h;
                        let (n, rows) = (
                            self.menu_len() as f32,
                            self.ui.as_ref().map(|u| u.menu_rows).unwrap_or(0) as f32,
                        );
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
        let looks: Vec<Vec2> = self
            .input
            .touch
            .fingers
            .iter()
            .filter(|f| matches!(f.role, Role::Look | Role::Mouse | Role::Photo))
            .map(|f| f.pos)
            .collect();
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

    pub(crate) fn finger_up(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: u64,
        p: Vec2,
        cancelled: bool,
    ) {
        let Some(k) = self.input.touch.fingers.iter().position(|f| f.id == id) else {
            return;
        };
        let f = self.input.touch.fingers.remove(k);
        if self
            .input
            .touch
            .fingers
            .iter()
            .filter(|f| matches!(f.role, Role::Look | Role::Mouse | Role::Photo))
            .count()
            < 2
        {
            self.input.touch.pinch = None;
        }
        match f.role {
            Role::Button(_, b) => {
                if b.held() {
                    self.touch_button(event_loop, b, false);
                } else if !cancelled
                    && self
                        .input
                        .touch
                        .button_at(p)
                        .and_then(|k| self.input.touch.buttons.get(k))
                        .map(|x| x.btn)
                        == Some(b)
                {
                    self.touch_button(event_loop, b, true);
                }
            }
            Role::Wheel(..) => {
                self.input.touch.steering = self
                    .input
                    .touch
                    .fingers
                    .iter()
                    .any(|f| matches!(f.role, Role::Wheel(..)))
            }
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
                let at = if cancelled {
                    Vec2::splat(-1e4)
                } else {
                    self.shell.onto_slider(f.start, p).unwrap_or(p)
                };
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
                if !f.moved
                    && !cancelled
                    && f.since.elapsed().as_secs_f32() < 0.45
                    && self.input.touch.fingers.is_empty()
                {
                    let (w, h) = self.input.touch.size;
                    self.photo_request(crate::photo::Request::FocusAt(
                        p.x / w.max(1.0),
                        p.y / h.max(1.0),
                    ));
                }
            }
            Role::Look => {
                // a tap on the picture: a click there (a switch the finger missed by a hair
                // is still found: the cursor's pick is generous)
                if !f.moved
                    && !cancelled
                    && f.since.elapsed().as_secs_f32() < 0.45
                    && self.input.touch.fingers.is_empty()
                {
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
            Btn::Sit => {
                if down
                    && self
                        .session
                        .on_foot
                        .as_ref()
                        .is_some_and(|f| f.can_touch_sit())
                {
                    self.use_seat();
                }
            }
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
                let key = if b == Btn::FlyUp {
                    KeyCode::KeyE
                } else {
                    KeyCode::KeyQ
                };
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
                    "outside"
                        if self
                            .player
                            .as_ref()
                            .is_some_and(|p| p.pax_camera_count() > 0) =>
                    {
                        "pax"
                    }
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
                let code = [
                    KeyCode::Digit1,
                    KeyCode::Digit2,
                    KeyCode::Digit3,
                    KeyCode::Digit4,
                ][(n - 1).min(3)];
                self.tap_key(event_loop, code, down, down);
            }
            Btn::DirectDoor(group) => {
                if let Some(names) = DIRECT_DOOR_GROUPS.get(group) {
                    for name in *names {
                        // A partial group is valid for buses exposing only one trigger.
                        if self
                            .player
                            .as_ref()
                            .is_some_and(|p| p.vehicle.ty.program.trigger(name).is_some())
                        {
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
                let t = if self.input.touch.tilt {
                    "Tilt steering on: hold the phone like a wheel"
                } else {
                    "Tilt steering off"
                };
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
        let (steer, thr, brk, active) = (
            steer_curve(t.steer),
            t.throttle,
            t.brake,
            t.steering || t.tilt || t.steer != 0.0,
        );
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
        if self.input.touch.enabled {
            self.touch_layout(w as f32, h as f32);
            self.touch_paint();
        } else if self.input.touch.cursor.is_some() {
            // (no controls to paint, but the cursor of this frame still goes into the painter,
            // which is cleared once a frame either way)
            self.input.touch.painter.clear();
        }
        if let Some((kind, at, scale)) = self.input.touch.cursor {
            omsi_ui::cursor::paint(&mut self.input.touch.painter, kind, at, scale);
        }
    }

    /// Put the software cursor `kind` at `at` (physical pixels) at the interface's `scale`, or
    /// take it away (`None`). Nothing is drawn while the system's own cursor is used.
    pub(crate) fn software_cursor(&mut self, cursor: Option<(u8, Vec2, f32)>) {
        self.input.touch.cursor = cursor;
    }

    /// `touch down|move|up x,y [id]` of an input script: a finger, through the same path.
    pub(crate) fn script_touch(
        &mut self,
        event_loop: &ActiveEventLoop,
        verb: &str,
        x: f32,
        y: f32,
        id: u64,
    ) {
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
        if (!self.enabled && self.cursor.is_none()) || self.painter.is_empty() {
            return;
        }
        let format = r.format();
        if self.gpu.as_ref().map(|g| g.1 != format).unwrap_or(true) {
            self.gpu = Some((Gpu::new(&r.device, format, 1, self.atlas.size), format));
            self.atlas.mark_all_dirty();
        }
        let Some((gpu, _)) = self.gpu.as_mut() else {
            return;
        };
        gpu.upload(&r.device, &r.queue, 0, &self.painter.verts);
        gpu.upload_atlas(&r.queue, &mut self.atlas);
        let layers = [Layer::flat([0.0, 0.0, w as f32, h as f32], 0.0, 1.0)];
        let draws = [Draw {
            buffer: 0,
            range: 0..self.painter.len(),
            layer: 0,
            texture: 0,
        }];
        let mut enc = r
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("touch controls"),
            });
        gpu.render(
            &r.device,
            &r.queue,
            &mut enc,
            view,
            (w, h),
            None,
            &layers,
            &draws,
        );
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
        let gpu = self
            .shot_gpu
            .get_or_insert_with(|| Gpu::new(&r.device, format, 1, size));
        gpu.upload(&r.device, &r.queue, 0, &self.painter.verts);
        self.atlas.mark_all_dirty();
        gpu.upload_atlas(&r.queue, &mut self.atlas);
        // (the window's pipeline has the atlas sent again next frame)
        self.atlas.mark_all_dirty();
        let tex = r.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("touch shot"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let layers = [Layer::flat([0.0, 0.0, w as f32, h as f32], 0.0, 1.0)];
        let draws = [Draw {
            buffer: 0,
            range: 0..self.painter.len(),
            layer: 0,
            texture: 0,
        }];
        let mut enc = r.device.create_command_encoder(&Default::default());
        gpu.render(
            &r.device,
            &r.queue,
            &mut enc,
            &view,
            (w, h),
            Some(wgpu::Color::TRANSPARENT),
            &layers,
            &draws,
        );
        let stride = (w * 4).div_ceil(256) * 256;
        let buf = r.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (stride * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
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
    let lock_to_lock = if s.wheel_lock >= 45.0 {
        s.wheel_lock
    } else {
        s.wheel_range
    };
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
        let depth =
            |r: Rect, y: f32| pedal_press(((r.bottom() - y) / r.h.max(1.0)).clamp(0.0, 1.0), rest);
        for f in &t.fingers {
            match f.role {
                Role::Throttle => t.throttle = depth(t.throttle_r, f.pos.y),
                Role::Brake => t.brake = depth(t.brake_r, f.pos.y),
                // (the clutch pushed in most of the way is in: the gearboxes want it at 1
                // to take a gear, see `pedal_ends`, and a thumb seldom sits at the top edge)
                // (the clutch as it was: no rest zone, straight up the pedal)
                Role::Clutch => {
                    let c = (0.2 + 0.8 * ((t.clutch_r.bottom() - f.pos.y) / t.clutch_r.h.max(1.0)))
                        .clamp(0.2, 1.0);
                    t.clutch = if c >= 0.75 { 1.0 } else { c };
                }
                _ => {}
            }
        }
    }
}
