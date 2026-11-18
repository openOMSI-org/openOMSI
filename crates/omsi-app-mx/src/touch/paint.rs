//! The touch controls painted into the frame, and the photo mode's button layout.

use super::*;

impl App {
    /// The photo mode's buttons: the camera's own, and its settings in a sheet the button
    /// opens (the driving controls are not shown).
    pub(super) fn touch_photo_layout(&mut self, w: f32, h: f32, pad: f32, u: f32) {
        let Some(ph) = self.photo.as_ref() else { return };
        let t = &mut self.input.touch;
        t.u = u;
        t.size = (w, h);
        t.top_gap = None;
        let mut b: Vec<Button> = Vec::new();
        let rb = |x: f32, y: f32, r: f32| Rect::new(x - r, y - r, r * 2.0, r * 2.0);
        let push = |b: &mut Vec<Button>, btn: Btn, rect: Rect, icon: &'static str, label: &str, on: bool, round: bool| {
            b.push(Button { btn, rect, icon, label: label.to_string(), on, round });
        };
        // the photo mode: the camera's own buttons, and its settings in a sheet the
        // button opens (the driving controls are not shown)
        let (open, hide, busy, fast) = (ph.touch_panel, ph.hide_ui, ph.render.capture, ph.touch_fast);
        let r = 24.0 * u;
        let step = r * 2.0 + 10.0 * u;
        t.stick_r = 0.0;
        t.info_room = None;
        let (lt, lb) = (pad + r, pad + r);
        if hide {
            push(&mut b, Btn::PhotoHide, rb(lt, lb, r), "visibility", "", true, true);
        } else {
            push(&mut b, Btn::PhotoExit, rb(lt, lb, r), "close", "", false, true);
            push(&mut b, Btn::PhotoPanel, rb(lt + step, lb, r), "tune", "", open, true);
            push(&mut b, Btn::PhotoHide, rb(lt + step * 2.0, lb, r), "visibility", "", false, true);
        }
        if !open {
            let sr = 38.0 * u;
            let fr = 26.0 * u;
            let x1 = w - pad - fr;
            if !hide {
                let y0 = h - pad - fr;
                push(&mut b, Btn::FlyDown, rb(x1, y0, fr), "keyboard_arrow_down", "", false, true);
                push(&mut b, Btn::FlyUp, rb(x1, y0 - fr * 2.0 - 12.0 * u, fr), "keyboard_arrow_up", "", false, true);
                push(&mut b, Btn::FlyFaster, rb(x1, y0 - (fr * 2.0 + 12.0 * u) * 2.0, fr), "speed", "", fast, true);
                t.stick_r = 62.0 * u;
                t.stick_c = Vec2::new(pad + t.stick_r + 10.0 * u, h - pad - t.stick_r - 10.0 * u);
            }
            let sx = if hide { w - pad - sr } else { x1 - fr - 14.0 * u - sr };
            push(&mut b, Btn::PhotoShutter, rb(sx, h - pad - sr, sr), "photo_camera", "", busy, true);
        }
        t.buttons = b;
    }

    /// Paint the controls into `painter` (physical pixels).
    pub(super) fn touch_paint(&mut self) {
        let speed = self.player.as_ref().map(|p| p.vehicle.physics.velocity_kmh().abs());
        let lock_angle = touch_lock_angle(&self.settings);
        // (the buttons' backgrounds follow the interface's opacity; their icons stay solid)
        let panel_bg = PANEL_BG.alpha(crate::ui::backdrop(self.settings.ui_opacity));
        let (w, h) = self.input.touch.size;
        let t = &mut self.input.touch;
        let u = t.u;
        let fonts = t.fonts.get_or_insert_with(Fonts::new);
        let pt = &mut t.painter;
        pt.clear();
        t.atlas.begin_frame();
        let atlas = &mut t.atlas;
        // the wheel, the stick and the pedals
        let driving = !t.buttons.is_empty() && t.stick_r == 0.0 && !t.hidden && t.throttle_r.w > 0.0 && t.buttons.iter().any(|b| matches!(b.btn, Btn::Gear(..)));
        if driving {
            if !t.tilt {
                let c = t.wheel_c;
                let r = t.wheel_r;
                // every part is drawn once, side by side, never one see-through shape over
                // another (that showed as darker patches where they crossed)
                use std::f32::consts::{FRAC_PI_2, PI, TAU};
                let (rim_in, hub) = (r - 12.0 * u, 16.0 * u);
                let part = Color::rgba(230, 230, 230, if t.steering { 0.95 } else { 0.8 });
                pt.circle(c, rim_in, panel_bg);
                // the spokes turn with the wheel (to the settings' lock, see `touch_lock_angle`)
                let a0 = t.steer * lock_angle;
                let half = 3.5 * u;
                for k in 0..3 {
                    let a = a0 + FRAC_PI_2 + k as f32 * TAU / 3.0 + PI;
                    let d = Vec2::new(a.cos(), a.sin());
                    let n = Vec2::new(-d.y, d.x) * half;
                    // from the hub's edge to the rim's inner edge
                    let (p0, p1) = (c + d * hub, c + d * (rim_in * rim_in - half * half).max(0.0).sqrt());
                    pt.convex(&[p0 + n, p1 + n, p1 - n, p0 - n], part);
                }
                pt.circle(c, hub, part);
                // the rim, with the top mark in its gap
                let top = a0 - FRAC_PI_2;
                pt.arc(c, rim_in, r, top + 0.12, top + TAU - 0.12, part);
                pt.arc(c, rim_in, r, top - 0.12, top + 0.12, PANEL_ON);
            } else {
                pt.text(atlas, fonts, &omsi_ui::tr("TILT"), 14.0 * u, Weight::Bold, t.wheel_c, Align::Center, DIM);
            }
            for (r, v, name) in [(t.clutch_r, t.clutch, "CLUTCH"), (t.brake_r, t.brake, "BRAKE"), (t.throttle_r, t.throttle, "GAS")] {
                if r.w <= 0.0 {
                    continue;
                }
                pt.rounded(r, 12.0 * u, panel_bg);
                if v > 0.0 {
                    let fill = Rect::new(r.x, r.bottom() - r.h * v, r.w, r.h * v);
                    pt.rounded(fill, 12.0 * u, match name { "GAS" => Color::rgba(104, 190, 118, 0.75), "CLUTCH" => Color::rgba(90, 150, 230, 0.75), _ => Color::rgba(222, 78, 68, 0.75) });
                }
                pt.rounded_border(r, 12.0 * u, 2.0 * u, Color::rgba(230, 230, 230, 0.35));
                // the pedal's ribs
                for k in 1..5 {
                    let y = r.y + r.h * k as f32 / 5.0;
                    pt.line(Vec2::new(r.x + 12.0 * u, y), Vec2::new(r.right() - 12.0 * u, y), 2.0 * u, Color::rgba(230, 230, 230, 0.18));
                }
                pt.text(atlas, fonts, &omsi_ui::tr(name), 11.0 * u, Weight::Bold, Vec2::new(r.center().x, r.bottom() - 10.0 * u), Align::Center, DIM);
            }
            if let Some(v) = speed {
                let at = Vec2::new(w * 0.5, h - 22.0 * u);
                let s = crate::units::Units::now().speed_text(v);
                let tw = fonts.width(&s, 20.0 * u, Weight::Bold);
                pt.rounded(Rect::new(at.x - tw * 0.5 - 12.0 * u, at.y - 24.0 * u, tw + 24.0 * u, 34.0 * u), 10.0 * u, panel_bg);
                pt.text(atlas, fonts, &s, 20.0 * u, Weight::Bold, at, Align::Center, TEXT);
            }
        }
        if t.stick_r > 0.0 && !t.hidden {
            pt.circle(t.stick_c, t.stick_r, panel_bg);
            pt.arc(t.stick_c, t.stick_r - 3.0 * u, t.stick_r, 0.0, std::f32::consts::TAU, Color::rgba(230, 230, 230, 0.35));
            let knob = t.stick_at.map(|(c, d)| c + d).unwrap_or(t.stick_c);
            pt.circle(knob, 24.0 * u, Color::rgba(230, 230, 230, 0.8));
            pt.icon(atlas, "open_with", knob, 26.0 * u, Color::rgba(20, 20, 20, 0.9));
        }
        if t.panel && !t.hidden {
            let rs: Vec<Rect> = t.buttons.iter().filter(|b| !b.round && !matches!(b.btn, Btn::Gear(..))).map(|b| b.rect).collect();
            if let Some(first) = rs.first() {
                // (the grid is wider than one row: every button is inside)
                let (x0, y0, x1, y1) = rs.iter().fold((first.x, first.y, first.right(), first.bottom()), |a, r| (a.0.min(r.x), a.1.min(r.y), a.2.max(r.right()), a.3.max(r.bottom())));
                let all = Rect::new(x0 - 10.0 * u, y0 - 10.0 * u, x1 - x0 + 20.0 * u, y1 - y0 + 20.0 * u);
                pt.rounded(all, 16.0 * u, Color::rgba(10, 12, 16, 0.78));
            }
        }
        // the buttons
        let held: Vec<Btn> = t.fingers.iter().filter_map(|f| if let Role::Button(_, b) = f.role { Some(b) } else { None }).collect();
        for b in t.buttons.iter() {
            let pressed = held.contains(&b.btn);
            let bg = if b.on { PANEL_ON } else if pressed { Color::rgba(90, 94, 100, 0.85) } else { panel_bg };
            let fg = if b.on { Color::rgba(20, 20, 20, 1.0) } else { TEXT };
            if b.round {
                let c = b.rect.center();
                let r = b.rect.w * 0.5 * if pressed { 0.94 } else { 1.0 };
                pt.circle(c, r, bg);
                pt.arc(c, r - 1.5 * u, r, 0.0, std::f32::consts::TAU, Color::rgba(255, 255, 255, 0.16));
                pt.icon(atlas, b.icon, c, r * 1.15, fg);
                if !b.label.is_empty() {
                    // (the door's number)
                    let at = c + Vec2::new(r * 0.62, r * 0.62);
                    pt.circle(at, 9.0 * u, Color::rgba(20, 20, 20, 0.9));
                    pt.text(atlas, fonts, &b.label, 11.0 * u, Weight::Bold, at + Vec2::new(0.0, 4.0 * u), Align::Center, TEXT);
                }
            } else {
                pt.rounded(b.rect, 10.0 * u, bg);
                if b.icon.is_empty() {
                    pt.text_in(atlas, fonts, &b.label, 17.0 * u, Weight::Bold, b.rect, Align::Center, fg);
                } else {
                    pt.icon(atlas, b.icon, Vec2::new(b.rect.x + 24.0 * u, b.rect.center().y), 24.0 * u, fg);
                    let tr = Rect::new(b.rect.x + 44.0 * u, b.rect.y, b.rect.w - 50.0 * u, b.rect.h);
                    let text = fonts.fit(&omsi_ui::tr(&b.label), 12.5 * u, Weight::Medium, tr.w);
                    pt.text_in(atlas, fonts, &text, 12.5 * u, Weight::Medium, tr, Align::Left, fg);
                }
            }
        }
        if let Some((note, left)) = t.note.clone() {
            let a = left.clamp(0.0, 0.4) / 0.4;
            let s = omsi_ui::tr(&note).to_string();
            let tw = fonts.width(&s, 15.0 * u, Weight::Medium);
            let at = Vec2::new(w * 0.5, h * 0.42);
            pt.rounded(Rect::new(at.x - tw * 0.5 - 16.0 * u, at.y - 24.0 * u, tw + 32.0 * u, 36.0 * u), 12.0 * u, Color::rgba(10, 12, 16, 0.8 * a));
            pt.text(atlas, fonts, &s, 15.0 * u, Weight::Medium, at, Align::Center, TEXT.alpha(a));
        }
    }
}
