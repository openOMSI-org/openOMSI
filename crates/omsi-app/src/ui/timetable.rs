//! The timetable HUD and its clock columns, kept in the legacy renderer’s compact style.

use super::*;

impl Ui {
    #[rustfmt::skip]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_timetable(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, t: &TimetableView, top: f32, beside: f32, s: f32) {
        let size = self.shown_size(1, f.sizes[1]);
        let s = s * size;
        let ink_1 = hud::text(self.hud.ink);
        let ink_2 = hud::text(self.hud.dim);
        let muted = hud::text(self.hud.muted);
        let hpx = (11.5 * s).round() as u32 | BOLD;
        let time_w = self.text.width_bold(&crate::units::ClockFormat::now().time(46740.0, false), (13.0 * s).round()).max(44.0 * s);
        let exp_w = self.text.width_bold("Expected", (11.5 * s).round()).max(time_w) + 16.0 * s;
        let wide = (470.0 * s).min(f.width * 0.42).round();
        let roomy = wide - 54.0 * s - 2.0 * time_w - exp_w >= 150.0 * s;
        let w = if roomy { wide } else { (400.0 * s).min(f.width * 0.42).round() };
        let x = (f.width - w - 16.0 * s - beside).max(16.0 * s).round();
        let top = self.below_info(x, x + w, top, s).round();
        let pad = (16.0 * s).round();
        let px = (13.0 * s).round() as u32;
        let lh = (30.0 * s).round();
        let head_h = 64.0 * s;
        let th_h = 26.0 * s;
        let max_rows = (((f.height * 0.72 - head_h - th_h) / lh).floor() as usize).max(3);
        let n = t.rows.len() + t.next.is_some() as usize;
        let shown = n.min(max_rows);
        // (from the stop before the next one on)
        let next = t.rows.iter().position(|r| r.state == 1).unwrap_or(0);
        let first = next.saturating_sub(1).min(n.saturating_sub(shown));
        let h = (head_h + th_h + lh * shown as f32 + pad * 0.6).round();
        self.hud_card(r, scene, [x, top, x + w, top + h], hud::RADIUS * s, self.text.backdrop, s);
        // the header: the line's sign, the terminus, how late
        let cy = top + pad + 11.0 * s;
        let sign = self.text.label_op(r, scene, &t.line, (13.0 * s).round() as u32 | BOLD, [255, 255, 255, 0], 1.0);
        let sw = (sign.w as f32 + 10.0 * s).max(30.0 * s).round();
        let sh = (22.0 * s).round();
        self.text.rounded(r, scene, [x + pad, cy - sh * 0.5, x + pad + sw, cy + sh * 0.5], 4.0 * s, hud::LINE);
        let (sx, sy) = ((x + pad + (sw - sign.w as f32) * 0.5).round(), (cy - sign.h as f32 * 0.5).round());
        scene.overlays.push((sign.tex, [sx, sy, sx + sign.w as f32, sy + sign.h as f32]));
        let d = t.delay;
        let (dtext, dcol) = {
            let m = (d.abs() / 60.0) as i64;
            let sec = (d.abs() % 60.0) as i64;
            let sign = if d < 0.0 { "-" } else { "+" };
            (format!("{sign}{m}:{sec:02}"), hud::delay_color(d as f32))
        };
        let dl = self.text.label_op(r, scene, &dtext, (13.0 * s).round() as u32 | BOLD, hud::text(dcol), 1.0);
        let dx = (x + w - pad - 7.0 * s - dl.w as f32).round();
        self.text.rounded(r, scene, [dx - 7.0 * s, cy - dl.h as f32 * 0.5 - 2.0 * s, dx + dl.w as f32 + 7.0 * s, cy + dl.h as f32 * 0.5 + 2.0 * s], hud::RADIUS * s, hud::tint(dcol, 41));
        scene.overlays.push((dl.tex, [dx, (cy - dl.h as f32 * 0.5).round(), dx + dl.w as f32, (cy - dl.h as f32 * 0.5).round() + dl.h as f32]));
        let tx = x + pad + sw + 10.0 * s;
        let term = clip_to(&self.text, &t.terminus, 15.0 * s, dx - 10.0 * s - tx);
        let tl = self.text.label_op(r, scene, &term, (15.0 * s).round() as u32 | BOLD, ink_1, 1.0);
        scene.overlays.push((tl.tex, [tx.round(), (cy - tl.h as f32 * 0.5).round(), tx.round() + tl.w as f32, (cy - tl.h as f32 * 0.5).round() + tl.h as f32]));
        let my = top + pad + 34.0 * s;
        let icon_px = (14.0 * s).round() as u32;
        if let Some(ic) = self.text.icon(r, scene, "departure_board", icon_px, self.hud.muted) {
            let iy = (my - icon_px as f32 * 0.5).round();
            scene.overlays.push((ic, [x + pad, iy, x + pad + icon_px as f32, iy + icon_px as f32]));
        }
        let ml = self.text.label_op(r, scene, &t.meta, (12.0 * s).round() as u32, muted, 1.0);
        let mx = (x + pad + icon_px as f32 + 6.0 * s).round();
        scene.overlays.push((ml.tex, [mx, (my - ml.h as f32 * 0.5).round(), mx + ml.w as f32, (my - ml.h as f32 * 0.5).round() + ml.h as f32]));
        // the table's head
        let col_arr = x + pad;
        let col_stop = x + pad + time_w + 14.0 * s;
        let hy = top + head_h + th_h * 0.5;
        let col_exp = x + w - pad;
        let col_dep = if roomy { col_exp - exp_w } else { col_exp };
        let heads = [(omsi_ui::tr("Arrives"), col_arr, false), (omsi_ui::tr("Bus stop"), col_stop, false), (omsi_ui::tr("Departs"), col_dep, true), (omsi_ui::tr("Expected"), col_exp, true)];
        for (txt, at, right) in heads.into_iter().take(if roomy { 4 } else { 3 }) {
            let l = self.text.label_op(r, scene, &txt, hpx, muted, 1.0);
            let lx = if right { at - l.w as f32 } else { at };
            let ly = (hy - l.h as f32 * 0.5).round();
            scene.overlays.push((l.tex, [lx.round(), ly, lx.round() + l.w as f32, ly + l.h as f32]));
        }
        // the rows
        for k in first..first + shown {
            let ry = top + head_h + th_h + lh * (k - first) as f32;
            let band = [x + 6.0 * s, ry, x + w - 6.0 * s, ry + lh - 2.0 * s];
            let (name, arr, dep, expected, state) = match t.rows.get(k) {
                Some(row) => (row.name.clone(), row.arr.clone(), row.dep.clone(), if roomy { row.expected.clone() } else { String::new() }, row.state),
                None => match t.next.as_ref() {
                    Some((what, when)) => (format!("› {what}"), String::new(), when.clone(), String::new(), 3),
                    None => continue,
                },
            };
            if state == 1 {
                self.text.rounded(r, scene, band, 4.0 * s, self.hud.shade(self.hud.accent, 0.16, self.text.backdrop));
                self.text.rounded(r, scene, [band[0], band[1], band[0] + 3.0 * s, band[3]], 1.5 * s, self.hud.accent);
            } else if (k - first) % 2 == 1 {
                self.text.rounded(r, scene, band, 4.0 * s, self.hud.shade(self.hud.ink, 0.04, self.text.backdrop));
            }
            let (ink, time_ink) = match state {
                0 | 3 => (muted, muted),
                1 => (ink_1, hud::text(self.hud.accent)),
                _ => (ink_2, ink_1),
            };
            let size = if state == 1 { px | BOLD } else { px };
            let cy = ry + (lh - 2.0 * s) * 0.5;
            for (txt, at, right, col) in [(&arr, col_arr, false, time_ink), (&dep, col_dep, true, time_ink), (&expected, col_exp, true, hud::text(dcol))] {
                if txt.is_empty() {
                    continue;
                }
                let l = self.text.label_op(r, scene, txt, size, col, 1.0);
                let lx = if right { at - l.w as f32 } else { at };
                let ly = (cy - l.h as f32 * 0.5).round();
                scene.overlays.push((l.tex, [lx.round(), ly, lx.round() + l.w as f32, ly + l.h as f32]));
            }
            let name = clip_to(&self.text, &name, px as f32, col_dep - time_w - 8.0 * s - col_stop);
            let l = self.text.label_op(r, scene, &name, size, ink, 1.0);
            let ly = (cy - l.h as f32 * 0.5).round();
            scene.overlays.push((l.tex, [col_stop.round(), ly, col_stop.round() + l.w as f32, ly + l.h as f32]));
        }
        self.hud_grip(r, scene, 1, [x, top, x + w, top + h], size, f, s);
    }
}
