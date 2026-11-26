use super::*;

const MANUAL: [(&str, &str); 10] = [
    ("kw_s_R", "R"),
    ("kw_s_N", "N"),
    ("kw_s_1", "1"),
    ("kw_s_2", "2"),
    ("kw_s_3", "3"),
    ("kw_s_4", "4"),
    ("kw_s_5", "5"),
    ("kw_s_6", "6"),
    ("kw_s_7", "7"),
    ("kw_s_8", "8"),
];

fn rb(x: f32, y: f32, r: f32) -> Rect {
    Rect::new(x - r, y - r, r * 2.0, r * 2.0)
}

fn push(
    b: &mut Vec<Button>,
    btn: Btn,
    rect: Rect,
    icon: &'static str,
    label: &str,
    on: bool,
    round: bool,
) {
    b.push(Button {
        btn,
        rect,
        icon,
        label: label.to_string(),
        on,
        round,
    });
}

fn touch_gears(p: &crate::player::Player) -> (bool, Vec<(&'static str, &'static str)>) {
    let has = |name: &str| {
        p.vehicle.ty.program.trigger(name).is_some()
            || p.bound_actions()
                .iter()
                .any(|a| a.eq_ignore_ascii_case(name))
    };
    let scripted = |name: &str| p.vehicle.ty.program.trigger(name).is_some();
    let program = &p.vehicle.ty.program;
    let manual = program.manual_gearbox();
    let count = p
        .vehicle
        .ty
        .program
        .constant("antrieb_number_gears")
        .map(|n| n.round() as usize)
        .filter(|n| (1..=8).contains(n));
    let gears: Vec<(&'static str, &'static str)> = if manual {
        MANUAL
            .iter()
            .copied()
            .enumerate()
            .filter(|(k, (a, _))| {
                matches!(*a, "kw_s_N") || (scripted(a) && count.is_none_or(|n| *k < n + 2))
            })
            .map(|(_, g)| g)
            .collect()
    } else if has("automatic_D") {
        vec![
            ("automatic_R", "R"),
            ("automatic_N", "N"),
            ("automatic_D", "D"),
        ]
    } else if has("kw_s_plus") {
        vec![("kw_s_minus", "-"), ("kw_s_N", "N"), ("kw_s_plus", "+")]
    } else {
        vec![("kw_s_R", "R"), ("kw_s_N", "N"), ("kw_s_1", "1")]
    };
    (manual, gears)
}

impl App {
    pub(super) fn touch_layout_in(&mut self, w: f32, h: f32) {
        let dpi = self
            .window
            .as_ref()
            .map(|w| w.scale_factor() as f32)
            .unwrap_or(1.0)
            .max(0.5);
        // (a point is the screen's density, a little larger on a big tablet and smaller
        // on a small phone)
        let short = w.min(h) / dpi;
        let mut u = dpi * (short / 400.0).clamp(0.85, 1.35);
        // ... and no larger than the bottom row fits across: the wheel and the horn on the
        // left, the speed in the middle, the brakes, the pedals (a manual's clutch too) on
        // the right - in points of the layout below. On a tablet held upright (an iPad mini's
        // 744 points) the clutch pedal ran under the speed and the horn.
        let clutch = self
            .player
            .as_ref()
            .is_some_and(|p| p.vehicle.ty.program.manual_gearbox())
            && !self.settings.auto_clutch;
        let side = if clutch { 300.0 } else { 238.0 };
        // (only where that row is: the free camera and walking have a stick and a few buttons)
        if self.player.is_some()
            && self.photo.is_none()
            && matches!(self.view.as_str(), "driver" | "outside" | "pax")
        {
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
        t.top_gap = None;
        let mut b: Vec<Button> = Vec::new();
        let menu_mode = self.menus.game_menu.is_some()
            || self.menus.chooser.is_some()
            || self.menus.navigator.as_ref().is_some_and(|n| n.map_open());
        if menu_mode {
            self.touch_layout_menu(&mut b, w, pad, u);
            self.input.touch.buttons = b;
            return;
        }
        let (r, left_end, right, hidden, driving, free) =
            self.touch_layout_top_bar(&mut b, w, h, pad, u);
        if hidden {
            self.input.touch.info_room = Some(info_room(w, pad, left_end, right, pad + r * 2.0, u));
            self.input.touch.buttons = b;
            return;
        }
        if driving {
            self.input.touch.stick_r = 0.0;
            self.touch_layout_driving(&mut b, w, h, pad, r, u);
        } else {
            self.touch_layout_walking(&mut b, w, h, pad, u, free);
        }
        if free && self.input.touch.panel && !driving {
            self.touch_layout_free_panel(&mut b, w, h, pad, r, u);
        }
        self.input.touch.buttons = b;
    }

    fn touch_layout_menu(&mut self, b: &mut Vec<Button>, w: f32, pad: f32, u: f32) {
        let r = 24.0 * u;
        let close = self.menus.game_menu.is_some() || self.menus.chooser.is_some();
        push(
            b,
            if close { Btn::CloseMenu } else { Btn::Map },
            rb(w - pad - r, pad + r, r),
            "close",
            "",
            false,
            true,
        );
        self.input.touch.info_room =
            Some(info_room(w, pad, pad, w - pad - r * 2.0, pad + r * 2.0, u));
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_layout_top_bar(
        &mut self,
        b: &mut Vec<Button>,
        w: f32,
        h: f32,
        pad: f32,
        u: f32,
    ) -> (f32, f32, f32, bool, bool, bool) {
        // (the buttons shrink to fit the screen's width: on a narrow phone the two groups ran
        // into each other where the free camera and walking, with a larger `u`, had them)
        let count = 4.0
            + if self.input.touch.hidden {
                1.0
            } else if self.session.duty.is_some() {
                5.0
            } else {
                4.0
            };
        let fit = ((w - 2.0 * pad - 8.0 * u) / (count * 52.0 * u)).clamp(0.72, 1.0);
        let r = 21.0 * u * fit;
        let y = pad + r;
        let step = r * 2.0 + 10.0 * u * fit;
        let mut x = pad + r;
        for (btn, icon) in [
            (Btn::Menu, "menu"),
            (Btn::Pause, if self.paused { "play_arrow" } else { "pause" }),
            (Btn::Camera, "videocam"),
            (Btn::LookReset, "360"),
        ] {
            push(
                b,
                btn,
                rb(x, y, r),
                icon,
                "",
                btn == Btn::Pause && self.paused,
                true,
            );
            x += step;
        }
        let left_end = x - step + r;
        let mut x = w - pad - r;
        let hidden = self.input.touch.hidden;
        push(
            b,
            Btn::Hide,
            rb(x, y, r),
            if hidden { "visibility" } else { "visibility" },
            "",
            hidden,
            true,
        );
        let free = self.view == "free";
        if hidden {
            self.input.touch.stick_r = 0.0;
            // (the free camera's way back stays when the controls are hidden: nothing else of the
            // screen is a button then)
            if free {
                push(
                    b,
                    Btn::FreeCam,
                    Rect::new(w - pad - 178.0 * u, h * 0.5 - 22.0 * u, 178.0 * u, 44.0 * u),
                    "close",
                    "Exit free camera",
                    false,
                    false,
                );
            }
            return (r, left_end, x - r, hidden, false, free);
        }
        let driving =
            self.player.is_some() && matches!(self.view.as_str(), "driver" | "outside" | "pax");
        x -= step;
        push(
            b,
            Btn::Panel,
            rb(x, y, r),
            "tune",
            "",
            self.input.touch.panel,
            true,
        );
        x -= step;
        push(b, Btn::Map, rb(x, y, r), "map", "", false, true);
        if self.session.duty.is_some() {
            x -= step;
            push(
                b,
                Btn::Timetable,
                rb(x, y, r),
                "departure_board",
                "",
                self.menus.timetable,
                true,
            );
        }
        x -= step;
        push(
            b,
            Btn::Screenshot,
            rb(x, y, r),
            "photo_camera",
            "",
            false,
            true,
        );
        self.input.touch.info_room = Some(info_room(w, pad, left_end, x - r, pad + r * 2.0, u));
        self.input.touch.top_gap = Some([left_end + 6.0 * u, x - r - 6.0 * u, pad + r * 2.0]);
        (r, left_end, x - r, hidden, driving, free)
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_layout_walking(
        &mut self,
        b: &mut Vec<Button>,
        w: f32,
        h: f32,
        pad: f32,
        u: f32,
        free: bool,
    ) {
        self.input.touch.stick_r = 62.0 * u;
        self.input.touch.stick_c = Vec2::new(
            pad + self.input.touch.stick_r + 10.0 * u,
            h - pad - self.input.touch.stick_r - 10.0 * u,
        );
        if let Some(f) = self
            .session
            .on_foot
            .as_ref()
            .filter(|f| f.cam == crate::on_foot::FootCam::First && f.seat.is_none())
        {
            let kr = 26.0 * u;
            push(
                b,
                Btn::Kneel,
                rb(w - pad - kr, h - pad - kr, kr),
                "keyboard_arrow_down",
                "",
                f.kneel,
                true,
            );
        }
        if free {
            // the free camera: the stick moves it, a drag looks, a pinch zooms; up and
            // down and the speed beside them in the corner, the way back at the side
            // (what the walk of `cam.ego` has no use for: it keeps to the ground)
            if !self.input.touch.panel {
                push(
                    b,
                    Btn::FreeCam,
                    Rect::new(w - pad - 178.0 * u, h * 0.5 - 22.0 * u, 178.0 * u, 44.0 * u),
                    "close",
                    "Exit free camera",
                    false,
                    false,
                );
            }
            if !self.cam.ego {
                let fr = 26.0 * u;
                let (x1, y0) = (w - pad - fr, h - pad - fr);
                let (x0, y1) = (x1 - fr * 2.0 - 12.0 * u, y0 - fr * 2.0 - 12.0 * u);
                push(
                    b,
                    Btn::FlyDown,
                    rb(x1, y0, fr),
                    "keyboard_arrow_down",
                    "",
                    false,
                    true,
                );
                push(
                    b,
                    Btn::FlyUp,
                    rb(x1, y1, fr),
                    "keyboard_arrow_up",
                    "",
                    false,
                    true,
                );
                push(b, Btn::FlySlower, rb(x0, y0, fr), "remove", "", false, true);
                push(b, Btn::FlyFaster, rb(x0, y1, fr), "add", "", false, true);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_layout_driving(
        &mut self,
        b: &mut Vec<Button>,
        w: f32,
        h: f32,
        pad: f32,
        r: f32,
        u: f32,
    ) {
        let (manual, gears) = {
            let p = self.player.as_ref().unwrap();
            touch_gears(p)
        };
        // --- the wheel, bottom left
        self.input.touch.wheel_r = 78.0 * u;
        self.input.touch.wheel_c = Vec2::new(
            pad + self.input.touch.wheel_r + 6.0 * u,
            h - pad - self.input.touch.wheel_r,
        );
        // --- the pedals, bottom right
        // (as long as the settings make them, within what the screen's height leaves)
        let long = self
            .settings
            .touch_pedal_length
            .clamp(0.8, 1.6)
            .min((h * 0.45 / (150.0 * u)).max(0.8));
        let th = 150.0 * u * long;
        self.input.touch.throttle_r = Rect::new(w - pad - 64.0 * u, h - pad - th, 64.0 * u, th);
        let bh = 112.0 * u * long;
        self.input.touch.brake_r = Rect::new(
            self.input.touch.throttle_r.x - 14.0 * u - 80.0 * u,
            h - pad - bh,
            80.0 * u,
            bh,
        );
        let gy = self.touch_layout_gears(b, h, pad, u, manual, gears);
        self.touch_layout_doors_and_signals(b, h, pad, u, gy);
        if self.input.touch.panel {
            self.touch_layout_cab_panel(b, w, h, pad, r, u);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_layout_gears(
        &mut self,
        b: &mut Vec<Button>,
        h: f32,
        pad: f32,
        u: f32,
        manual: bool,
        gears: Vec<(&'static str, &'static str)>,
    ) -> f32 {
        // the gearbox above the pedals: an automatic's R N D, a sequential lever's - N +,
        // else a manual's whole gate - R, N and every gear its script has (`kw_s_1` ...),
        // in two rows as the H of the lever (R 1 3 5 over N 2 4 6)
        let gy;
        if manual && gears.len() > 3 {
            // the gear engaged, as the lever's script has it
            let p = self.player.as_ref().unwrap();
            let engaged = p
                .vehicle
                .var("antrieb_getr_aktugang")
                .or_else(|| p.vehicle.var("antrieb_getr_gang"))
                .map(|g| g.round() as i32);
            let label_of = |g: i32| match g {
                -1 => "R",
                0 => "N",
                g => MANUAL.get(g as usize + 1).map(|x| x.1).unwrap_or(""),
            };
            let cols = gears.len().div_ceil(2);
            let gw =
                (self.input.touch.throttle_r.right() - self.input.touch.brake_r.x) / cols as f32;
            let gh = 36.0 * u;
            let top = self.input.touch.throttle_r.y - 10.0 * u - 2.0 * gh - 6.0 * u;
            gy = top;
            for (k, (action, letter)) in gears.iter().enumerate() {
                let (col, row) = (k / 2, k % 2);
                let on = match engaged {
                    Some(g) => label_of(g) == *letter,
                    None => self.input.touch.gear == Some(*letter),
                };
                push(
                    b,
                    Btn::Gear(action, letter),
                    Rect::new(
                        self.input.touch.brake_r.x + gw * col as f32 + 3.0 * u,
                        top + row as f32 * (gh + 6.0 * u),
                        gw - 6.0 * u,
                        gh,
                    ),
                    "",
                    letter,
                    on,
                    false,
                );
            }
        } else {
            let gw = (self.input.touch.throttle_r.right() - self.input.touch.brake_r.x) / 3.0;
            gy = self.input.touch.throttle_r.y - 10.0 * u - 40.0 * u;
            for (k, (action, letter)) in gears.iter().enumerate() {
                let on =
                    self.input.touch.gear == Some(*letter) && matches!(*letter, "R" | "N" | "D");
                push(
                    b,
                    Btn::Gear(action, letter),
                    Rect::new(
                        self.input.touch.brake_r.x + gw * k as f32 + 3.0 * u,
                        gy,
                        gw - 6.0 * u,
                        40.0 * u,
                    ),
                    "",
                    letter,
                    on,
                    false,
                );
            }
        }
        // a manual without the automatic clutch of the settings: its clutch pedal, left of
        // the brake's buttons
        self.input.touch.clutch_r = if manual && !self.settings.auto_clutch {
            let ch = 112.0 * u;
            Rect::new(
                self.input.touch.brake_r.x - 14.0 * u - 50.0 * u - 14.0 * u - 64.0 * u,
                h - pad - ch,
                64.0 * u,
                ch,
            )
        } else {
            Rect::new(0.0, 0.0, 0.0, 0.0)
        };
        gy
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_layout_doors_and_signals(
        &mut self,
        b: &mut Vec<Button>,
        h: f32,
        pad: f32,
        u: f32,
        gy: f32,
    ) {
        // the brakes left of the brake pedal
        let p = self.player.as_ref().unwrap();
        let br = 25.0 * u;
        let bx = self.input.touch.brake_r.x - 14.0 * u - br;
        let pb_on = p
            .vehicle
            .var("parkingbrake")
            .or_else(|| p.vehicle.var("Handbremse"))
            .or_else(|| p.vehicle.var("parking_brake"))
            .is_some_and(|v| v > 0.5);
        push(
            b,
            Btn::ParkingBrake,
            rb(bx, h - pad - br, br),
            "local_parking",
            "",
            pb_on,
            true,
        );
        let sb_on = p
            .vehicle
            .var("bremse_halte_sw")
            .or_else(|| p.vehicle.var("haltestellenbremse"))
            .is_some_and(|v| v > 0.5);
        push(
            b,
            Btn::StopBrake,
            rb(bx, h - pad - br * 3.0 - 10.0 * u, br),
            "back_hand",
            "",
            sb_on,
            true,
        );
        // Preserve the current door detection in Standard mode. Direct mode
        // uses 0, 1 and combined 2+3; the fourth trigger button is not shown.
        let door_buttons: Vec<Btn> = if self.settings.touch_door_mode == "direct" {
            super::direct_door_buttons(p.vehicle.ty.program.triggers.keys().map(String::as_str))
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
            let dx = self.input.touch.throttle_r.right()
                - dr
                - (doors - 1 - k) as f32 * (dr * 2.0 + 10.0 * u);
            push(
                b,
                action,
                rb(dx, dy, dr),
                "door_sliding",
                &(k + 1).to_string(),
                false,
                true,
            );
        }
        // the indicators and the horn beside the wheel
        let ir = 23.0 * u;
        let iy = self.input.touch.wheel_c.y - self.input.touch.wheel_r - 14.0 * u - ir;
        let blink = p
            .vehicle
            .var("lights_sw_blinker")
            .map(|v| v.round() as i32)
            .unwrap_or(0);
        let warn = p
            .vehicle
            .var("lights_sw_warnblinker")
            .is_some_and(|v| v > 0.5);
        push(
            b,
            Btn::BlinkLeft,
            rb(
                self.input.touch.wheel_c.x - self.input.touch.wheel_r + ir,
                iy,
                ir,
            ),
            "turn_left",
            "",
            blink == 1,
            true,
        );
        push(
            b,
            Btn::Hazard,
            rb(self.input.touch.wheel_c.x, iy, ir),
            "warning",
            "",
            warn,
            true,
        );
        push(
            b,
            Btn::BlinkRight,
            rb(
                self.input.touch.wheel_c.x + self.input.touch.wheel_r - ir,
                iy,
                ir,
            ),
            "turn_right",
            "",
            blink == 2,
            true,
        );
        let hr = 26.0 * u;
        push(
            b,
            Btn::Horn,
            rb(
                self.input.touch.wheel_c.x + self.input.touch.wheel_r + 18.0 * u + hr,
                h - pad - hr,
                hr,
            ),
            "campaign",
            "",
            false,
            true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_layout_cab_panel(
        &mut self,
        b: &mut Vec<Button>,
        w: f32,
        h: f32,
        pad: f32,
        r: f32,
        u: f32,
    ) {
        // --- the cab panel: the rest of the switches, over the middle
        let p = self.player.as_ref().unwrap();
        let t = &self.input.touch;
        let items: Vec<(Btn, &'static str, &str, bool)> = vec![
            (
                Btn::Battery,
                "power_settings_new",
                "Battery",
                p.vehicle
                    .var("elec_busbar_main")
                    .or_else(|| p.vehicle.var("bat_switch"))
                    .is_some_and(|v| v > 0.5),
            ),
            (
                Btn::Engine,
                "key",
                "Engine start (hold)",
                p.vehicle.var("engine_on").is_some_and(|v| v > 0.5),
            ),
            (
                Btn::AutoStart,
                "autorenew",
                "Start the bus by itself",
                false,
            ),
            (
                Btn::Headlights,
                "light",
                "Headlights",
                p.vehicle
                    .var("lights_fern")
                    .or_else(|| p.vehicle.var("lights_sw_licht"))
                    .is_some_and(|v| v > 0.5),
            ),
            (Btn::HighBeam, "flashlight_on", "High beam", false),
            (
                Btn::Wipers,
                "water_drop",
                "Wipers",
                p.vehicle.var("wiper_sw").is_some_and(|v| v > 0.5),
            ),
            (Btn::SaloonLights, "highlight", "Saloon lights", false),
            (Btn::Ticket, "confirmation_number", "Sell ticket", false),
            (Btn::Announce, "campaign", "Announcement (Q)", false),
            (
                Btn::VoiceNearby,
                "record_voice_over",
                "Voice: nearby (hold)",
                t.voice_held[0],
            ),
            (
                Btn::VoiceRadio,
                "settings_voice",
                "Voice: radio (hold)",
                t.voice_held[1],
            ),
            (
                Btn::InteriorCam,
                "airline_seat_recline_normal",
                "Next seat view",
                false,
            ),
            (Btn::FreeCam, "videocam", "Free camera", false),
            (
                Btn::Navigator,
                "navigation",
                "Navigator",
                self.menus.navigator.as_ref().is_some_and(|n| n.enabled),
            ),
            (Btn::Info, "info", "Information bar", self.menus.info_bar),
            (Btn::Tilt, "screen_rotation", "Tilt steering", t.tilt),
        ];
        // (the talk buttons only where voice chat is on, in a session)
        let voice_on = self.settings.voice_enabled && self.net.lan.is_some();
        let items: Vec<_> = items
            .into_iter()
            .filter(|i| voice_on || !matches!(i.0, Btn::VoiceNearby | Btn::VoiceRadio))
            .collect();
        for (rect, (btn, icon, label, on)) in panel_rects(items.len(), w, h, pad, r, u)
            .into_iter()
            .zip(items)
        {
            push(b, btn, rect, icon, label, on, false);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_layout_free_panel(
        &mut self,
        b: &mut Vec<Button>,
        w: f32,
        h: f32,
        pad: f32,
        r: f32,
        u: f32,
    ) {
        // the free camera's own panel: the way back, the speed, what the cab panel has of the
        // screen's furniture
        let items: Vec<(Btn, &'static str, &str, bool)> = vec![
            (Btn::FreeCam, "close", "Exit free camera", false),
            (Btn::FlySlower, "remove", "Slower", false),
            (Btn::FlyFaster, "add", "Faster", false),
            (
                Btn::Navigator,
                "navigation",
                "Navigator",
                self.menus.navigator.as_ref().is_some_and(|n| n.enabled),
            ),
            (Btn::Info, "info", "Information bar", self.menus.info_bar),
        ];
        for (rect, (btn, icon, label, on)) in panel_rects(items.len(), w, h, pad, r, u)
            .into_iter()
            .zip(items)
        {
            push(b, btn, rect, icon, label, on, false);
        }
    }
}
