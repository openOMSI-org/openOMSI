//! The navigator's cards over the map: the next turn, the time and temperatures, the speed and
//! its limit, the next stop and the stop list, and the timetable board. Moved out of
//! `panel.rs` as they were.

use super::*;

impl Navigator {
    /// The cards over the map.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_cards(
        &mut self,
        ui: &mut Painter,
        f: &NavFrame,
        wd: &Words,
        net: Option<&Network>,
        map: Rect,
        s: f32,
        pal: &Palette,
    ) {
        let pad = 8.0 * s;
        let units = omsi_ui::units::Units::now();
        // --- top left: the next turn, or the street the bus is on
        let mut left_w = 0.0;
        if let Some((dir, angle, dist, street)) = self.next_turn.clone() {
            let icon = match dir {
                2 => "u_turn_left",
                -1 if angle < 60.0 => "turn_slight_left",
                1 if angle < 60.0 => "turn_slight_right",
                -1 => "turn_left",
                _ => "turn_right",
            };
            let t = units.distance(dist);
            let max_w = map.w * 0.64;
            let tw = self.fonts.width(&t, 19.0 * s, Weight::Bold);
            let street = street.as_deref().map(|n| {
                self.fonts
                    .fit(n, 12.0 * s, Weight::Medium, max_w - 58.0 * s)
            });
            let sw_ = street
                .as_deref()
                .map(|n| self.fonts.width(n, 12.0 * s, Weight::Medium))
                .unwrap_or(0.0);
            let w = (52.0 * s + tw.max(sw_) + 14.0 * s).min(max_w);
            let h = if street.is_some() { 54.0 } else { 42.0 } * s;
            let b = Rect::new(map.x + pad, map.y + pad, w, h);
            ui.shadow(b, 12.0 * s, 8.0 * s, Color::rgba(0, 0, 0, 0.18));
            ui.rounded(b, 12.0 * s, BANNER);
            ui.icon(
                &mut self.atlas,
                icon,
                Vec2::new(b.x + 26.0 * s, b.center().y),
                32.0 * s,
                BANNER_TEXT,
            );
            let tx = b.x + 50.0 * s;
            let top = if street.is_some() { b.y + 6.0 * s } else { b.y };
            ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &t,
                19.0 * s,
                Weight::Bold,
                Rect::new(
                    tx,
                    top,
                    w - 54.0 * s,
                    if street.is_some() { 26.0 * s } else { b.h },
                ),
                Align::Left,
                BANNER_TEXT,
            );
            if let Some(n) = street.as_deref() {
                ui.text_in(
                    &mut self.atlas,
                    &self.fonts,
                    n,
                    12.0 * s,
                    Weight::Medium,
                    Rect::new(tx, b.y + 30.0 * s, w - 54.0 * s, 18.0 * s),
                    Align::Left,
                    BANNER_DIM,
                );
            }
            left_w = w;
        } else if let Some(n) = self.street_here.clone() {
            let px = 12.5 * s;
            let n = self.fonts.fit(&n, px, Weight::Bold, map.w * 0.55);
            let w = self.fonts.width(&n, px, Weight::Bold) + 34.0 * s;
            let b = Rect::new(map.x + pad, map.y + pad, w, 28.0 * s);
            ui.shadow(b, 14.0 * s, 6.0 * s, Color::rgba(0, 0, 0, 0.14));
            ui.rounded(b, 14.0 * s, BANNER);
            ui.icon(
                &mut self.atlas,
                "straight",
                Vec2::new(b.x + 14.0 * s, b.center().y),
                16.0 * s,
                BANNER_TEXT,
            );
            ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &n,
                px,
                Weight::Bold,
                Rect::new(b.x + 26.0 * s, b.y, w - 28.0 * s, b.h),
                Align::Left,
                BANNER_TEXT,
            );
            left_w = w;
        }
        // --- top right: the time, the day and the temperatures (and the line)
        let hh = (f.time / 3600.0) as i32 % 24;
        let mm = ((f.time % 3600.0) / 60.0) as i32;
        let time_text = format!("{hh:02}:{mm:02}");
        let day = wd.days[f.weekday.clamp(0, 6) as usize];
        let temp = format!("{:.0}° · {:.0}°", f.outside_temp, f.inside_temp);
        let tw = self.fonts.width(&time_text, 14.0 * s, Weight::Bold)
            + 5.0 * s
            + self.fonts.width(day, 11.0 * s, Weight::Medium);
        let w = tw.max(self.fonts.width(&temp, 11.0 * s, Weight::Medium)) + 20.0 * s;
        if map.w - left_w - 3.0 * pad > w {
            let b = Rect::new(map.right() - pad - w, map.y + pad, w, 40.0 * s);
            ui.shadow(b, 12.0 * s, 6.0 * s, Color::rgba(0, 0, 0, 0.12));
            ui.rounded(b, 12.0 * s, pal.card);
            let base = b.y + 17.0 * s;
            ui.text(
                &mut self.atlas,
                &self.fonts,
                &time_text,
                14.0 * s,
                Weight::Bold,
                Vec2::new(b.right() - 10.0 * s, base),
                Align::Right,
                pal.card_text,
            );
            ui.text(
                &mut self.atlas,
                &self.fonts,
                day,
                11.0 * s,
                Weight::Medium,
                Vec2::new(
                    b.right()
                        - 10.0 * s
                        - self.fonts.width(&time_text, 14.0 * s, Weight::Bold)
                        - 5.0 * s,
                    base,
                ),
                Align::Right,
                pal.card_dim,
            );
            ui.text(
                &mut self.atlas,
                &self.fonts,
                &temp,
                11.0 * s,
                Weight::Medium,
                Vec2::new(b.right() - 10.0 * s, b.y + 33.0 * s),
                Align::Right,
                pal.card_dim,
            );
        }
        draw_signal_countdown(self, ui, f, map, s);
        let bottom_h = 52.0 * s;
        let card = Rect::new(
            map.x + pad,
            map.bottom() - pad - bottom_h,
            map.w - 2.0 * pad,
            bottom_h,
        );
        if self.destination.is_some() {
            self.draw_destination_card(ui, net, map, s, pal);
        } else {
            // --- bottom: the next stop
            ui.shadow(card, 14.0 * s, 8.0 * s, Color::rgba(0, 0, 0, 0.16));
            ui.rounded(card, 14.0 * s, pal.card);
            let inner = card.pad(12.0 * s, 0.0);
            let row1 = Rect::new(inner.x, card.y + 7.0 * s, inner.w, 20.0 * s);
            let row2 = Rect::new(inner.x, card.y + 28.0 * s, inner.w, 18.0 * s);
            let row1 = stop_request_icon(ui, &mut self.atlas, f.stop_requested, row1, s * 0.8);
            let note = if self.route.note > 0.0 {
                Some((wd.recalculated.to_string(), ON_TIME))
            } else if self.route.joined
                && !self.route.lanes.is_empty()
                && !self.route.on_route
                && self.route.off_for > OFF_ROUTE_AFTER
            {
                Some((
                    (if self.route.off_for < OFF_ROUTE_AFTER + 20.0 {
                        wd.rerouting
                    } else {
                        wd.off_route
                    })
                    .to_string(),
                    WARN,
                ))
            } else {
                None
            };
            let jam_note = (self.jam_cost >= 30.0).then(|| {
                let n = (self.jam_cost / 60.0).max(1.0).round();
                (
                    if self.jam_cost >= 60.0 {
                        format!("Traffic jam +{n:.0} min")
                    } else {
                        format!("Slow traffic +{n:.0} min")
                    },
                    if self.jam_cost >= 60.0 {
                        style::TRAFFIC[2]
                    } else {
                        style::TRAFFIC[1]
                    },
                )
            });
            match f.stops.first() {
                Some(st) => {
                    let name = if f.stops.len() == 1 {
                        format!("{} · {}", st.name.trim(), omsi_ui::tr(wd.last_stop))
                    } else {
                        st.name.trim().to_string()
                    };
                    let line_w = match f.line.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
                        Some(l) => {
                            let px = 11.0 * s;
                            let l = self.fonts.fit(l, px, Weight::Bold, 48.0 * s);
                            let w = self.fonts.width(&l, px, Weight::Bold) + 10.0 * s;
                            let r = Rect::new(row1.x, row1.y + 2.0 * s, w, 16.0 * s);
                            ui.rounded(r, 4.0 * s, BUS);
                            ui.text_in(
                                &mut self.atlas,
                                &self.fonts,
                                &l,
                                px,
                                Weight::Bold,
                                r,
                                Align::Center,
                                Color::WHITE,
                            );
                            w + 7.0 * s
                        }
                        None => 0.0,
                    };
                    let name = self
                        .fonts
                        .fit(&name, 14.5 * s, Weight::Bold, row1.w - line_w);
                    let name_r = Rect::new(row1.x + line_w, row1.y, row1.w - line_w, row1.h);
                    let w = ui.text_in(
                        &mut self.atlas,
                        &self.fonts,
                        &name,
                        14.5 * s,
                        Weight::Bold,
                        name_r,
                        Align::Left,
                        if st.closed.is_some() {
                            pal.card_dim
                        } else {
                            pal.card_text
                        },
                    );
                    if st.closed.is_some() {
                        strike(ui, name_r.x, w, name_r, s, pal.card_dim);
                    }
                    let mut parts = Vec::new();
                    if let Some((_, d)) = self.next_dist {
                        parts.push(units.distance(d));
                        let secs = d / (self.speed_avg.max(5.0) as f64)
                            + if self.jam_cost > 0.0 {
                                self.jam_cost as f64
                            } else {
                                0.0
                            };
                        parts.push(if secs < 60.0 {
                            omsi_ui::tr("<1 min").into_owned()
                        } else {
                            format!("{:.0} {}", (secs / 60.0).round(), omsi_ui::tr("min"))
                        });
                    }
                    parts.push(format!(
                        "{:02}:{:02}",
                        (st.arrival / 3600.0) as i32 % 24,
                        ((st.arrival % 3600.0) / 60.0) as i32
                    ));
                    let line2 = parts.join(" · ");
                    // (closed: the way round in place of the distance)
                    let closed = st.closed.as_deref().map(|via| {
                        if via.is_empty() {
                            wd.closed.to_string()
                        } else {
                            format!("{} · {via}", omsi_ui::tr(wd.closed))
                        }
                    });
                    let (text, col) = match (&closed, &note, &jam_note) {
                        (Some(t), _, _) => (t.clone(), WARN),
                        (None, Some((t, c)), _) => (t.clone(), *c),
                        (None, None, Some((t, c))) => (format!("{line2} · {}", omsi_ui::tr(t)), *c),
                        _ => (line2, pal.card_dim),
                    };
                    let mut right = 0.0;
                    if let Some(d) = f.delay {
                        let (txt, c) = if d > 59.0 {
                            (
                                format!("+{}:{:02}", (d / 60.0) as i32, (d % 60.0) as i32),
                                style::TRAFFIC[2],
                            )
                        } else if d < -59.0 {
                            (
                                format!("-{}:{:02}", (-d / 60.0) as i32, (-d % 60.0) as i32),
                                style::TRAFFIC[0],
                            )
                        } else {
                            (wd.on_time.to_string(), Color::hex(0x2fa84f))
                        };
                        let px = 11.5 * s;
                        let w = self.fonts.width(&txt, px, Weight::Bold) + 12.0 * s;
                        let r = Rect::new(row2.right() - w, row2.y, w, 17.0 * s);
                        ui.rounded(r, 8.5 * s, c.alpha(0.16));
                        ui.text_in(
                            &mut self.atlas,
                            &self.fonts,
                            &txt,
                            px,
                            Weight::Bold,
                            r,
                            Align::Center,
                            c,
                        );
                        right = w + 6.0 * s;
                    }
                    let text = self
                        .fonts
                        .fit(&text, 12.0 * s, Weight::Medium, row2.w - right);
                    ui.text_in(
                        &mut self.atlas,
                        &self.fonts,
                        &text,
                        12.0 * s,
                        Weight::Medium,
                        Rect::new(row2.x, row2.y, row2.w - right, row2.h),
                        Align::Left,
                        col,
                    );
                }
                None => {
                    let t = f
                        .terminus
                        .clone()
                        .filter(|t| !t.trim().is_empty())
                        .unwrap_or_else(|| wd.no_duty.to_string());
                    ui.text_in(
                        &mut self.atlas,
                        &self.fonts,
                        &t,
                        14.5 * s,
                        Weight::Bold,
                        row1,
                        Align::Left,
                        pal.card_text,
                    );
                    let here = self.street_here.clone().unwrap_or_default();
                    ui.text_in(
                        &mut self.atlas,
                        &self.fonts,
                        &here,
                        12.0 * s,
                        Weight::Medium,
                        row2,
                        Align::Left,
                        pal.card_dim,
                    );
                }
            }
        }
        // --- the speed and its limit, bottom left over the card
        let (speed, unit) = units.speed(f.speed_kmh.abs());
        let limit = net.and_then(|n| {
            let lane = if self.route.on_route {
                self.route.lanes.get(self.route.progress).copied()
            } else {
                None
            };
            let lane = lane.or_else(|| {
                n.nearest_lane_near(f.bus, LaneKind::Street)
                    .filter(|l| l.2 < 8.0)
                    .map(|l| l.0)
            })?;
            let v = n.lanes.get(lane)?.speed_limit_kmh;
            (v > 1.0 && v < 200.0).then_some(v)
        });
        self.speed_limit = limit;
        let r = 20.0 * s;
        let c = Vec2::new(card.x + r + 2.0 * s, card.y - r - 7.0 * s);
        let over = limit.is_some_and(|l| f.speed_kmh.abs() > l + 3.0);
        ui.circle(
            c + Vec2::new(0.0, 1.5 * s),
            r + 1.0 * s,
            Color::rgba(0, 0, 0, 0.14),
        );
        ui.circle(c, r, pal.card);
        let sp = format!("{speed:.0}");
        ui.text(
            &mut self.atlas,
            &self.fonts,
            &sp,
            17.0 * s,
            Weight::Bold,
            Vec2::new(c.x, c.y + 3.0 * s),
            Align::Center,
            if over {
                style::TRAFFIC[2]
            } else {
                pal.card_text
            },
        );
        ui.text(
            &mut self.atlas,
            &self.fonts,
            unit,
            7.5 * s,
            Weight::Medium,
            Vec2::new(c.x, c.y + 12.0 * s),
            Align::Center,
            pal.card_dim,
        );
        if let Some(v) = limit {
            let lc = Vec2::new(c.x + r * 2.0 + 2.0 * s, c.y);
            let lr = 15.0 * s;
            if let Some(sign) = self
                .atlas
                .color_icon("speed_limit_sign", (lr * 2.0).round() as u32)
            {
                ui.sprite(
                    sign,
                    lc - Vec2::splat(lr),
                    Vec2::splat(lr * 2.0),
                    Color::WHITE,
                );
            }
            let t = format!("{:.0}", units.speed_limit(v));
            let px = if t.len() > 2 { 9.5 } else { 12.0 } * s;
            ui.text(
                &mut self.atlas,
                &self.fonts,
                &t,
                px,
                Weight::Black,
                Vec2::new(lc.x, lc.y + self.fonts.cap_height(px, Weight::Black) * 0.5),
                Align::Center,
                Color::rgba(15, 15, 15, 1.0),
            );
        }
    }

    /// The next stops with their planned and expected times, under the map.
    pub(super) fn draw_schedule(
        &mut self,
        ui: &mut Painter,
        f: &NavFrame,
        r: Rect,
        s: f32,
        pal: &Palette,
    ) {
        ui.rect(r, pal.card.alpha(1.0));
        let pad = 12.0 * s;
        let mut y = r.y + 6.0 * s;
        let late = f.delay.unwrap_or(0.0);
        for (k, st) in f.stops.iter().take(5).enumerate() {
            let row = Rect::new(r.x + pad, y, r.w - 2.0 * pad, 22.0 * s);
            // a line of dots down the left, as a route's stops
            let dot = Vec2::new(row.x + 5.0 * s, row.center().y);
            if k + 1 < f.stops.len().min(5) {
                ui.rect(
                    Rect::new(dot.x - 1.0 * s, dot.y, 2.0 * s, 22.0 * s),
                    style::TRAFFIC[0].alpha(0.35),
                );
            }
            ui.circle(dot, 4.5 * s, style::TRAFFIC[0]);
            ui.circle(dot, 2.3 * s, pal.card);
            let planned = format!(
                "{:02}:{:02}",
                (st.arrival / 3600.0) as i32 % 24,
                ((st.arrival % 3600.0) / 60.0) as i32
            );
            ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &planned,
                12.0 * s,
                Weight::Bold,
                Rect::new(row.x + 16.0 * s, row.y, 40.0 * s, row.h),
                Align::Left,
                pal.card_dim,
            );
            let name = self
                .fonts
                .fit(st.name.trim(), 13.0 * s, Weight::Medium, row.w - 110.0 * s);
            let name_r = Rect::new(row.x + 58.0 * s, row.y, row.w - 110.0 * s, row.h);
            let w = ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &name,
                13.0 * s,
                Weight::Medium,
                name_r,
                Align::Left,
                if st.closed.is_some() {
                    pal.card_dim
                } else {
                    pal.card_text
                },
            );
            // (closed today: struck through, and the bus does not stop there)
            if st.closed.is_some() {
                strike(ui, name_r.x, w, name_r, s, pal.card_dim);
            }
            let exp = st.arrival + late;
            let e = format!(
                "{:02}:{:02}",
                (exp / 3600.0).rem_euclid(24.0) as i32,
                ((exp.rem_euclid(3600.0)) / 60.0) as i32
            );
            ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &e,
                12.0 * s,
                Weight::Medium,
                row,
                Align::Right,
                if late > 59.0 {
                    style::TRAFFIC[2]
                } else if late < -59.0 {
                    style::TRAFFIC[0]
                } else {
                    pal.card_dim
                },
            );
            y += 22.0 * s;
        }
    }
}

/// A line through a stop's name `w` wide from `x`, at the middle of `r` (a stop closed today).
fn strike(ui: &mut Painter, x: f32, w: f32, r: Rect, s: f32, c: Color) {
    ui.rect(
        Rect::new(
            x,
            (r.center().y - 0.75 * s).round(),
            w.max(0.0),
            (1.5 * s).max(1.0),
        ),
        c,
    );
}

fn draw_signal_countdown(
    nav: &mut Navigator,
    ui: &mut Painter,
    f: &NavFrame<'_>,
    map: Rect,
    s: f32,
) {
    let Some(secs) = f
        .traffic
        .and_then(|t| crate::signal_countdown::green_in(t, f.bus, f.heading))
    else {
        return;
    };
    let label = format!("Green in {secs}s");
    let px = 13.0 * s;
    let w = nav.fonts.width(&label, px, Weight::Bold) + 24.0 * s;
    let badge = Rect::new(
        map.x + (map.w - w) * 0.5,
        map.bottom() - 86.0 * s,
        w,
        26.0 * s,
    );
    ui.rounded(badge, 12.0 * s, BANNER);
    ui.text_in(
        &mut nav.atlas,
        &nav.fonts,
        &label,
        px,
        Weight::Bold,
        badge,
        Align::Center,
        BANNER_TEXT,
    );
}
