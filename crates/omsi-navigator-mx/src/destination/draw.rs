//! Destination and entry markers use the same projection as the map beneath them.
use super::*;

const ENTRY: Color = Color::rgba(239, 145, 35, 1.0);

impl Navigator {
    pub(crate) fn draw_destination_markers(
        &mut self,
        ui: &mut Painter,
        screen: &dyn Fn(DVec2) -> Option<Vec2>,
        area: Rect,
        s: f32,
        labels: bool,
        pal: &style::Palette,
    ) {
        let mut used = Vec::new();
        if labels {
            self.city.entry_hits.clear();
        }
        for (index, e) in self.entries.iter().enumerate() {
            let Some(p) = screen(e.at).filter(|p| area.contains(*p)) else {
                continue;
            };
            ui.circle(p, 7.5 * s, pal.card);
            ui.circle(p, 5.5 * s, ENTRY);
            if !labels || e.name.is_empty() {
                continue;
            }
            let name = self.fonts.fit(&e.name, 11.5 * s, Weight::Medium, 190.0 * s);
            let width = self.fonts.width(&name, 11.5 * s, Weight::Medium) + 12.0 * s;
            let label = Rect::new(p.x + 11.0 * s, p.y - 10.0 * s, width, 20.0 * s);
            if !area.contains(Vec2::new(label.right(), label.bottom()))
                || used.iter().any(|r| rects_overlap(r, &label))
            {
                continue;
            }
            used.push(label);
            self.city.entry_hits.push((label, index));
            ui.rounded(label, 5.0 * s, pal.card);
            ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &name,
                11.5 * s,
                Weight::Medium,
                label,
                Align::Center,
                pal.card_text,
            );
        }
        if labels {
            self.city.waypoint_hits.clear();
            self.city.waypoint_remove.clear();
        }
        for (index, d) in self.destination.iter().chain(&self.waypoints).enumerate() {
            let Some(p) = screen(d.at).filter(|p| area.contains(*p)) else {
                continue;
            };
            let head = p - Vec2::new(0.0, 22.0 * s);
            let pin = Rect::new(p.x - 15.0 * s, p.y - 37.0 * s, 30.0 * s, 39.0 * s);
            let number = Rect::new(head.x - 14.0 * s, head.y - 14.0 * s, 28.0 * s, 28.0 * s);
            let color = if d.status == DestinationStatus::Unreachable {
                WARN
            } else {
                pal.route
            };
            pin_body(ui, head, p, 14.0 * s, pal.card);
            pin_body(ui, head, p - Vec2::new(0.0, 4.0 * s), 11.5 * s, color);
            ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &(index + 1).to_string(),
                13.0 * s,
                Weight::Bold,
                number,
                Align::Center,
                Color::WHITE,
            );
            if !labels {
                continue;
            }
            self.city.waypoint_hits.push((pin, index));
            let size = 22.0 * s;
            let cross = Rect::new(
                (p.x + 6.0 * s).clamp(area.x, (area.right() - size).max(area.x)),
                (head.y - 28.0 * s).clamp(area.y, (area.bottom() - size).max(area.y)),
                size,
                size,
            );
            // Include the bridge to the cross so it stays visible while moving onto it.
            let hover = Rect::new(
                pin.x.min(cross.x),
                pin.y.min(cross.y),
                pin.right().max(cross.right()) - pin.x.min(cross.x),
                pin.bottom().max(cross.bottom()) - pin.y.min(cross.y),
            );
            if self.city.pointer.is_some_and(|q| hover.contains(q)) {
                self.city.waypoint_remove.push((cross, index));
            }
        }
        if labels {
            // Match hit testing: all delete controls are above all waypoint markers.
            for (cross, _) in &self.city.waypoint_remove {
                ui.circle(cross.center(), 11.0 * s, pal.card);
                ui.icon(
                    &mut self.atlas,
                    "close",
                    cross.center(),
                    17.0 * s,
                    pal.card_text,
                );
            }
        }
    }

    pub(crate) fn destination_info(&self, net: Option<&Network>) -> String {
        let Some(d) = &self.destination else {
            return String::new();
        };
        let status = match d.status {
            DestinationStatus::Pending => omsi_ui::tr("Calculating route").into_owned(),
            DestinationStatus::Unreachable => {
                omsi_ui::tr("No drivable route to this point").into_owned()
            }
            DestinationStatus::Arrived => {
                let off_road = d
                    .path
                    .as_ref()
                    .is_some_and(|p| (p.end.truncate() - d.at).length() > 12.0);
                omsi_ui::tr(if off_road {
                    "Road access reached"
                } else {
                    "Destination reached"
                })
                .into_owned()
            }
            DestinationStatus::Navigating => {
                if self.route.off_for > OFF_ROUTE_AFTER {
                    return omsi_ui::tr("Recalculating route").into_owned();
                }
                let distance = net
                    .and_then(|n| self.destination_distance(n))
                    .map(|d| omsi_ui::units::Units::now().distance(d as f64))
                    .unwrap_or_default();
                match d
                    .path
                    .as_ref()
                    .map(|p| (p.end.truncate() - d.at).length())
                    .filter(|d| *d > 12.0)
                {
                    Some(last) => format!(
                        "{} · {}: {}",
                        distance,
                        omsi_ui::tr("From road to pin"),
                        omsi_ui::units::Units::now().distance(last)
                    ),
                    None => distance,
                }
            }
        };
        status
    }

    pub(crate) fn draw_destination_card(
        &mut self,
        ui: &mut Painter,
        net: Option<&Network>,
        map: Rect,
        s: f32,
        pal: &style::Palette,
    ) {
        let Some(d) = self.destination.as_ref() else {
            return;
        };
        let title = format!("1/{} · {}", self.waypoints.len() + 1, d.name);
        let name = self
            .fonts
            .fit(&title, 14.5 * s, Weight::Bold, map.w - 44.0 * s);
        let status = self.destination_info(net);
        let status = self
            .fonts
            .fit(&status, 11.5 * s, Weight::Medium, map.w - 44.0 * s);
        let card = Rect::new(
            map.x + 8.0 * s,
            map.bottom() - 60.0 * s,
            map.w - 16.0 * s,
            52.0 * s,
        );
        ui.shadow(card, 14.0 * s, 8.0 * s, Color::rgba(0, 0, 0, 0.16));
        ui.rounded(card, 14.0 * s, pal.card);
        let text = card.pad(12.0 * s, 0.0);
        ui.text_in(
            &mut self.atlas,
            &self.fonts,
            &name,
            14.5 * s,
            Weight::Bold,
            Rect::new(text.x, card.y + 6.0 * s, text.w, 22.0 * s),
            Align::Left,
            pal.card_text,
        );
        let color = if d.status == DestinationStatus::Unreachable {
            WARN
        } else {
            pal.card_dim
        };
        ui.text_in(
            &mut self.atlas,
            &self.fonts,
            &status,
            11.5 * s,
            Weight::Medium,
            Rect::new(text.x, card.y + 29.0 * s, text.w, 18.0 * s),
            Align::Left,
            color,
        );
    }

    pub(crate) fn draw_destination_controls(
        &mut self,
        ui: &mut Painter,
        head: Rect,
        s: f32,
        pal: &style::Palette,
    ) {
        self.city.buttons.push((head, 9));
        if self.destination.is_none() {
            return;
        }
        let caption = omsi_ui::tr("Clear all waypoints");
        let width = self.fonts.width(&caption, 12.0 * s, Weight::Medium) + 42.0 * s;
        let button = Rect::new(head.x, head.bottom() + 8.0 * s, width, 32.0 * s);
        ui.shadow(button, 10.0 * s, 6.0 * s, Color::rgba(0, 0, 0, 0.14));
        ui.rounded(button, 10.0 * s, pal.card);
        ui.icon(
            &mut self.atlas,
            "close",
            Vec2::new(button.x + 16.0 * s, button.center().y),
            18.0 * s,
            pal.card_text,
        );
        ui.text_in(
            &mut self.atlas,
            &self.fonts,
            &caption,
            12.0 * s,
            Weight::Medium,
            Rect::new(button.x + 30.0 * s, button.y, button.w - 36.0 * s, button.h),
            Align::Left,
            pal.card_text,
        );
        self.city.buttons.push((button, 8));
    }
}

/// Tangent sides join the circular head to a tip anchored at the selected map point.
fn pin_body(ui: &mut Painter, head: Vec2, tip: Vec2, radius: f32, color: Color) {
    let height = tip.y - head.y;
    let y = radius * radius / height;
    let x = (radius * radius - y * y).sqrt();
    ui.tri(
        head + Vec2::new(-x, y),
        head + Vec2::new(x, y),
        tip,
        color,
        color,
        color,
    );
    ui.circle(head, radius, color);
}
