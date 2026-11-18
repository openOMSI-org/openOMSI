use super::{hud, wrap_rows, Frame, Label, MenuKind, Renderer, Scene, TextureId, Ui, BOLD};

/// A translated label and its live value on the information bar.
#[derive(Clone, Debug, PartialEq)]
pub struct InfoItem {
    pub label: &'static str,
    pub value: String,
    pub badge: Option<String>,
    pub progress: Option<f32>,
    pub style: InfoItemStyle,
}

impl InfoItem {
    pub(crate) fn new(label: &'static str, value: String, style: InfoItemStyle) -> Self {
        Self {
            label,
            value,
            badge: None,
            progress: None,
            style,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InfoItemStyle {
    #[default]
    Normal,
    Clock,
    Speed,
    SpeedLimit,
    NextStop,
    Route,
    Delay,
    Status,
    Detail,
    Fuel,
}

/// Driving details stay above the less time-critical vehicle data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfoBar {
    pub primary: Vec<InfoItem>,
    pub secondary: Vec<InfoItem>,
}

struct InfoItemRows {
    labels: Vec<String>,
    values: Vec<String>,
}

struct InfoItemLabels {
    labels: Vec<Label>,
    values: Vec<Label>,
    badge: Option<Label>,
    icon: Option<(TextureId, f32)>,
    progress: Option<f32>,
    style: InfoItemStyle,
    tint: Option<[u8; 4]>,
    width: f32,
    height: f32,
}

fn info_item_rows(
    item: &InfoItem,
    room: f32,
    prefix_room: f32,
    trailing_room: f32,
    inline_label: bool,
    gap: f32,
    label_px: u32,
    value_px: u32,
    measure: impl Fn(&str, u32) -> f32,
) -> InfoItemRows {
    let label = omsi_ui::tr(item.label);
    let available = (room - prefix_room - trailing_room).max(1.0);
    let has_value = !item.value.is_empty();
    let label_room = if inline_label && has_value {
        (available - gap - measure("0", value_px)).max(1.0)
    } else {
        available
    };
    let labels = wrap_rows(&label, label_room, 0.0, &|text| measure(text, label_px));
    let label_width = if inline_label {
        labels
            .iter()
            .map(|line| measure(line, label_px))
            .fold(0.0, f32::max)
    } else {
        0.0
    };
    let value_room =
        (available - label_width - if inline_label && has_value { gap } else { 0.0 }).max(1.0);
    let values = if has_value {
        wrap_rows(&item.value, value_room, 0.0, &|text| {
            measure(text, value_px)
        })
    } else {
        Vec::new()
    };
    InfoItemRows { labels, values }
}

/// The information bar's items in rows that fit the available room, preserving their order.
pub(crate) fn info_rows(
    items: &[InfoItem],
    room: f32,
    gap: f32,
    width: impl Fn(usize, &InfoItem) -> f32,
) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row_width = 0.0;
    for (index, item) in items.iter().enumerate() {
        let item_width = width(index, item);
        let next_width = if rows.last().is_some_and(|row| !row.is_empty()) {
            row_width + gap + item_width
        } else {
            item_width
        };
        if next_width > room && rows.last().is_some_and(|row| !row.is_empty()) {
            rows.push(vec![index]);
            row_width = item_width;
        } else {
            if rows.is_empty() {
                rows.push(Vec::new());
            }
            rows.last_mut().unwrap().push(index);
            row_width = next_width;
        }
    }
    rows
}

fn info_row_offsets(widths: &[f32], gap: f32, room: f32) -> Vec<f32> {
    if widths.len() <= 1 {
        return vec![0.0; widths.len()];
    }
    let gap = ((room - widths.iter().sum::<f32>()) / (widths.len() - 1) as f32).max(gap);
    let mut x = 0.0;
    widths
        .iter()
        .map(|width| {
            let offset = x;
            x += width + gap;
            offset
        })
        .collect()
}

fn delay_seconds(value: &str) -> f32 {
    let (sign, rest) = match value.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, value.trim_start_matches('+')),
    };
    let mut parts = rest.split(':').map(|part| part.trim().parse::<f32>().unwrap_or(0.0));
    let minutes = parts.next().unwrap_or(0.0);
    let seconds = parts.next().unwrap_or(0.0);
    sign * (minutes * 60.0 + seconds)
}

fn info_bar_width(available_width: f32, content_width: f32, padding: f32, max_width: f32) -> f32 {
    (content_width + padding * 2.0).min(available_width.min(max_width))
}

fn quantize_info_bar_dimensions(
    width: f32,
    height: f32,
    available_width: f32,
    step: f32,
) -> (f32, f32) {
    let step = step.max(1.0);
    let round_up = |value: f32| (value / step).ceil() * step;
    (round_up(width).min(available_width), round_up(height))
}

impl Ui {
    fn draw_info_progress(
        &mut self,
        r: &Renderer,
        scene: &mut Scene,
        x: f32,
        y: f32,
        progress: f32,
        s: f32,
    ) {
        self.text.rounded(
            r,
            scene,
            [x, y, x + 44.0 * s, y + 5.0 * s],
            2.5 * s,
            hud::tint(self.hud.ink, 40),
        );
        self.text.rounded(
            r,
            scene,
            [x, y, x + 44.0 * s * progress.clamp(0.0, 1.0), y + 5.0 * s],
            2.5 * s,
            hud::ON_TIME,
        );
    }

    fn render_info_items(
        &mut self,
        r: &Renderer,
        scene: &mut Scene,
        items: &[InfoItem],
        secondary: bool,
        s: f32,
        room: f32,
        label_px: u32,
        stack_gap: f32,
        badge_pad_x: f32,
        badge_pad_y: f32,
    ) -> Vec<InfoItemLabels> {
        let look = self.hud;
        let ui = self;
        items
            .iter()
            .map(|item| {
                let value_px = match item.style {
                    InfoItemStyle::Clock => 15.0,
                    InfoItemStyle::Speed => 17.0,
                    InfoItemStyle::SpeedLimit => 13.0,
                    InfoItemStyle::NextStop => 14.0,
                    InfoItemStyle::Route => 12.0,
                    InfoItemStyle::Delay => 12.0,
                    InfoItemStyle::Status => 9.0,
                    InfoItemStyle::Detail => 11.0,
                    InfoItemStyle::Fuel => 11.0,
                    InfoItemStyle::Normal if secondary => 11.0,
                    InfoItemStyle::Normal => 13.0,
                };
                let value_px = (value_px * s).round().max(1.0) as u32;
                let value_weight = if !secondary && matches!(item.style, InfoItemStyle::Clock | InfoItemStyle::Speed | InfoItemStyle::SpeedLimit) { BOLD } else { 0 };
                let tint = match item.style {
                    InfoItemStyle::Status => Some(hud::WARN),
                    InfoItemStyle::Delay => Some(hud::delay_color(delay_seconds(&item.value))),
                    _ => None,
                };
                let label_color = hud::text(tint.unwrap_or(look.dim));
                let value_color = match item.style {
                    InfoItemStyle::SpeedLimit => [15, 15, 15, 0],
                    _ => hud::text(tint.unwrap_or(look.ink)),
                };
                let badge_room = item.badge.as_deref().map(|badge| {
                    ui.text.width_raw(badge, value_px as f32) + badge_pad_x * 2.0 + stack_gap
                });
                let icon_name = if secondary {
                    None
                } else {
                    match item.style {
                        InfoItemStyle::Clock => Some("schedule"),
                        InfoItemStyle::NextStop => Some("location_on"),
                        InfoItemStyle::Delay => Some("timer"),
                        InfoItemStyle::Status => Some("directions_bus"),
                        _ => None,
                    }
                };
                let icon_size = if item.style == InfoItemStyle::Status { 18.0 } else { 16.0 } * s;
                let icon_room = icon_name.map_or(0.0, |_| icon_size + stack_gap);
                let prefix_room = if icon_room > 0.0 {
                    icon_room
                } else {
                    badge_room.unwrap_or(0.0)
                };
                let trailing_room = if item.progress.is_some() {
                    44.0 * s + stack_gap
                } else {
                    0.0
                };
                let text = info_item_rows(
                    item,
                    room,
                    prefix_room,
                    trailing_room,
                    secondary,
                    stack_gap,
                    label_px,
                    value_px,
                    |text, px| ui.text.width_raw(text, px as f32),
                );
                let hide_label = !secondary
                    && matches!(
                        item.style,
                        InfoItemStyle::Route
                            | InfoItemStyle::Clock
                            | InfoItemStyle::Speed
                            | InfoItemStyle::SpeedLimit
                            | InfoItemStyle::NextStop
                            | InfoItemStyle::Delay
                    );
                let labels: Vec<Label> = if hide_label {
                    Vec::new()
                } else {
                    text.labels
                        .iter()
                        .map(|line| ui.text.label_op(r, scene, line, label_px, label_color, 1.0))
                        .collect()
                };
                let values: Vec<Label> = text
                    .values
                    .iter()
                    .map(|line| ui.text.label_op(r, scene, line, value_px | value_weight, value_color, 1.0))
                    .collect();
                let badge = item.badge.as_deref().map(|text| {
                    ui.text
                        .label_op(r, scene, text, value_px | BOLD, [255, 255, 255, 0], 1.0)
                });
                let sign_size = if item.style == InfoItemStyle::SpeedLimit {
                    28.0 * s
                } else {
                    0.0
                };
                let icon_color = tint.unwrap_or(look.accent);
                let icon = if item.style == InfoItemStyle::SpeedLimit && !secondary {
                    ui.text
                        .color_icon(
                            r,
                            scene,
                            "speed_limit_sign",
                            sign_size.round().max(8.0) as u32,
                        )
                        .map(|icon| (icon, sign_size))
                } else {
                    icon_name.and_then(|name| {
                        ui.text
                            .icon(r, scene, name, icon_size.round() as u32, icon_color)
                            .map(|icon| (icon, icon_size))
                    })
                };
                let label_width = labels
                    .iter()
                    .map(|label| label.w as f32)
                    .fold(0.0, f32::max);
                let value_width = values
                    .iter()
                    .map(|label| label.w as f32)
                    .fold(0.0, f32::max);
                let badge_width = badge
                    .as_ref()
                    .map_or(0.0, |badge| badge.w as f32 + badge_pad_x * 2.0);
                let progress_width = item.progress.map_or(0.0, |_| 44.0 * s + stack_gap);
                let status = tint.is_some();
                let labels_height = labels.iter().map(|label| label.h as f32).sum::<f32>()
                    + stack_gap * labels.len().saturating_sub(1) as f32;
                let value_height = values.iter().map(|value| value.h as f32).sum::<f32>()
                    + stack_gap * values.len().saturating_sub(1) as f32;
                let icon_width = icon.map_or(0.0, |(_, size)| size);
                let content_height = value_height
                    .max(
                        badge
                            .as_ref()
                            .map_or(0.0, |badge| badge.h as f32 + badge_pad_y * 2.0),
                    )
                    .max(icon_width)
                    .max(sign_size);
                let height = if secondary {
                    labels_height.max(content_height).max(7.0 * s)
                } else {
                    labels_height.max(content_height).max(7.0 * s)
                        + if status { badge_pad_y * 2.0 } else { 0.0 }
                };
                let width = if secondary {
                    label_width
                        + if label_width > 0.0 && content_height > 0.0 {
                            stack_gap
                        } else {
                            0.0
                        }
                        + value_width
                        + progress_width
                } else if sign_size > 0.0 {
                    sign_size
                } else {
                    let prefix = if icon_width > 0.0 {
                        icon_width + stack_gap
                    } else {
                        badge_width + if badge_width > 0.0 { stack_gap } else { 0.0 }
                    };
                    prefix
                        + if values.is_empty() {
                            label_width
                        } else {
                            value_width + progress_width
                        }
                        + if status { badge_pad_x * 2.0 } else { 0.0 }
                };
                InfoItemLabels {
                    labels,
                    values,
                    badge,
                    icon,
                    progress: item.progress,
                    style: item.style,
                    tint,
                    width,
                    height,
                }
            })
            .collect()
    }

    fn draw_info_item(
        &mut self,
        r: &Renderer,
        scene: &mut Scene,
        item: &InfoItemLabels,
        primary: bool,
        item_x: f32,
        item_y: f32,
        stack_gap: f32,
        badge_pad_x: f32,
        badge_pad_y: f32,
        s: f32,
    ) {
        let status = item.tint.is_some();
        let labels_height = item.labels.iter().map(|label| label.h as f32).sum::<f32>()
            + stack_gap * item.labels.len().saturating_sub(1) as f32;
        let values_height = item.values.iter().map(|value| value.h as f32).sum::<f32>()
            + stack_gap * item.values.len().saturating_sub(1) as f32;
        let values_width = item
            .values
            .iter()
            .map(|value| value.w as f32)
            .fold(0.0, f32::max);
        if let Some(tint) = item.tint {
            self.text.rounded(
                r,
                scene,
                [item_x, item_y, item_x + item.width, item_y + item.height],
                hud::RADIUS * s,
                hud::tint(tint, 41),
            );
        }
        if primary && item.style == InfoItemStyle::SpeedLimit {
            let diameter = item.width.min(28.0 * s);
            let sign_x = item_x + (item.width - diameter) * 0.5;
            let sign_y = item_y + (item.height - diameter) * 0.5;
            if let Some((icon, _)) = item.icon {
                scene
                    .overlays
                    .push((icon, [sign_x, sign_y, sign_x + diameter, sign_y + diameter]));
            }
            if let Some(value) = item.values.first() {
                let value_x = sign_x + (diameter - value.w as f32) * 0.5;
                let value_y = sign_y + (diameter - value.h as f32) * 0.5;
                scene.overlays.push((
                    value.tex,
                    [
                        value_x,
                        value_y,
                        value_x + value.w as f32,
                        value_y + value.h as f32,
                    ],
                ));
            }
        } else if primary {
            let content_height = labels_height
                .max(values_height)
                .max(item.icon.map_or(0.0, |(_, size)| size))
                .max(
                    item.badge
                        .map_or(0.0, |badge| badge.h as f32 + badge_pad_y * 2.0),
                );
            let line_y = item_y + (item.height - content_height) * 0.5;
            let mut cursor_x = item_x + if status { badge_pad_x } else { 0.0 };
            if let Some((icon, size)) = item.icon {
                let icon_y = line_y + (content_height - size) * 0.5;
                scene
                    .overlays
                    .push((icon, [cursor_x, icon_y, cursor_x + size, icon_y + size]));
                cursor_x += size + stack_gap;
            }
            if let Some(badge) = item.badge {
                let badge_width = badge.w as f32 + badge_pad_x * 2.0;
                let badge_height = badge.h as f32 + badge_pad_y * 2.0;
                let badge_y = line_y + (content_height - badge_height) * 0.5;
                self.text.rounded(
                    r,
                    scene,
                    [
                        cursor_x,
                        badge_y,
                        cursor_x + badge_width,
                        badge_y + badge_height,
                    ],
                    4.0 * s,
                    hud::LINE,
                );
                scene.overlays.push((
                    badge.tex,
                    [
                        cursor_x + badge_pad_x,
                        badge_y + badge_pad_y,
                        cursor_x + badge_pad_x + badge.w as f32,
                        badge_y + badge_pad_y + badge.h as f32,
                    ],
                ));
                cursor_x += badge_width + stack_gap;
            }
            if !item.labels.is_empty() {
                let label_y = item_y + (item.height - labels_height) * 0.5;
                let mut y = label_y;
                for label in &item.labels {
                    scene.overlays.push((
                        label.tex,
                        [cursor_x, y, cursor_x + label.w as f32, y + label.h as f32],
                    ));
                    y += label.h as f32 + stack_gap;
                }
                if !item.values.is_empty() {
                    cursor_x += item
                        .labels
                        .iter()
                        .map(|label| label.w as f32)
                        .fold(0.0, f32::max)
                        + stack_gap;
                }
            }
            let value_y = item_y + (item.height - values_height) * 0.5;
            let mut y = value_y;
            for value in &item.values {
                scene.overlays.push((
                    value.tex,
                    [cursor_x, y, cursor_x + value.w as f32, y + value.h as f32],
                ));
                y += value.h as f32 + stack_gap;
            }
            if let Some(progress) = item.progress {
                let bar_x = cursor_x + values_width + stack_gap;
                let bar_y = item_y + (item.height - 5.0 * s) * 0.5;
                self.draw_info_progress(r, scene, bar_x, bar_y, progress, s);
            }
        } else {
            let label_y = item_y + (item.height - labels_height) * 0.5;
            let mut y = label_y;
            for label in &item.labels {
                scene.overlays.push((
                    label.tex,
                    [item_x, y, item_x + label.w as f32, y + label.h as f32],
                ));
                y += label.h as f32 + stack_gap;
            }
            let label_width = item
                .labels
                .iter()
                .map(|label| label.w as f32)
                .fold(0.0, f32::max);
            let value_x = item_x + label_width + if label_width > 0.0 { stack_gap } else { 0.0 };
            let value_y = item_y + (item.height - values_height) * 0.5;
            let mut y = value_y;
            for value in &item.values {
                scene.overlays.push((
                    value.tex,
                    [value_x, y, value_x + value.w as f32, y + value.h as f32],
                ));
                y += value.h as f32 + stack_gap;
            }
            if let Some(progress) = item.progress {
                let bar_x = value_x + values_width + stack_gap;
                let bar_y = item_y + (item.height - 5.0 * s) * 0.5;
                self.draw_info_progress(r, scene, bar_x, bar_y, progress, s);
            }
        }
    }

    pub(super) fn draw_info_bar(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, s: f32) {
        let size = self.shown_size(0, f.sizes[0]);
        let s = s * size;
        // --- the information bar: driving details first, vehicle data beneath
        self.info_rect = None;
        // (not under the pause menu's rail, from under which it stood out)
        let rail =
            f.menu.is_some() && f.menu_kind == MenuKind::Game && f.menu_head.is_none() && !f.vr;
        if let Some(info) = f
            .info
            .as_ref()
            .filter(|info| !rail && (!info.primary.is_empty() || !info.secondary.is_empty()))
        {
            let label_px = (11.0 * s).round().max(1.0) as u32;
            let pad_x = 9.0 * s;
            let pad_y = 6.0 * s;
            let gap = 9.0 * s;
            let stack_gap = 2.0 * s;
            let row_gap = 4.0 * s;
            let section_gap = 9.0 * s;
            let badge_pad_x = 6.0 * s;
            let badge_pad_y = 3.0 * s;
            let [x0, x1, y] = f
                .info_room
                .unwrap_or([12.0 * s, f.width - 12.0 * s, 8.0 * s]);
            let available_width = (x1 - x0).max(1.0);
            let preferred_width = (available_width * 0.94).min(1120.0 * s).max(1.0);
            let room = (preferred_width - pad_x * 2.0).max(1.0);
            let primary_labels = self.render_info_items(
                r,
                scene,
                &info.primary,
                false,
                s,
                room,
                label_px,
                stack_gap,
                badge_pad_x,
                badge_pad_y,
            );
            let secondary_labels = self.render_info_items(
                r,
                scene,
                &info.secondary,
                true,
                s,
                room,
                label_px,
                stack_gap,
                badge_pad_x,
                badge_pad_y,
            );
            let primary_rows = info_rows(&info.primary, room, gap, |index, _| {
                primary_labels[index].width
            });
            let secondary_rows = info_rows(&info.secondary, room, gap, |index, _| {
                secondary_labels[index].width
            });
            let mut rows = Vec::new();
            for (primary, item_rows, labels) in [
                (true, &primary_rows, &primary_labels),
                (false, &secondary_rows, &secondary_labels),
            ] {
                for indexes in item_rows {
                    let width: f32 = indexes
                        .iter()
                        .map(|&index| labels[index].width)
                        .sum::<f32>()
                        + gap * indexes.len().saturating_sub(1) as f32;
                    let height = indexes
                        .iter()
                        .map(|&index| labels[index].height)
                        .fold(0.0, f32::max);
                    rows.push((primary, indexes.clone(), width, height));
                }
            }
            if !rows.is_empty() {
                let content_width = rows.iter().map(|row| row.2).fold(0.0, f32::max);
                let w = info_bar_width(available_width, content_width, pad_x, preferred_width);
                let row_spacing = rows
                    .windows(2)
                    .map(|pair| {
                        if pair[0].0 && !pair[1].0 {
                            section_gap
                        } else {
                            row_gap
                        }
                    })
                    .sum::<f32>();
                let h = rows.iter().map(|row| row.3).sum::<f32>() + row_spacing + pad_y * 2.0;
                let (w, h) = quantize_info_bar_dimensions(w, h, available_width, 8.0 * s);
                let x = (x0 + x1 - w) * 0.5;
                let opacity = self.text.backdrop.clamp(0.0, 1.0);
                self.hud_card(r, scene, [x, y, x + w, y + h], hud::RADIUS * s, opacity, s);
                let divider = self
                    .text
                    .solid(r, scene, hud::tint(self.hud.ink, (30.0 * opacity).round() as u8));
                let mut row_y = y + pad_y;
                for (row_at, (primary, indexes, row_width, row_height)) in rows.iter().enumerate() {
                    let labels = if *primary {
                        &primary_labels
                    } else {
                        &secondary_labels
                    };
                    let item_widths: Vec<f32> =
                        indexes.iter().map(|&index| labels[index].width).collect();
                    let row_x = if indexes.len() == 1 {
                        x + (w - row_width) * 0.5
                    } else {
                        x + pad_x
                    };
                    let item_offsets = info_row_offsets(&item_widths, gap, w - pad_x * 2.0);
                    for (item_at, &index) in indexes.iter().enumerate() {
                        let item = &labels[index];
                        let item_x = row_x + item_offsets[item_at];
                        if item_at > 0 {
                            let divider_x = item_x - gap * 0.5;
                            scene.overlays.push((
                                divider,
                                [
                                    divider_x,
                                    row_y + 3.0 * s,
                                    divider_x + s,
                                    row_y + row_height - 3.0 * s,
                                ],
                            ));
                        }
                        let item_y = row_y + (row_height - item.height) * 0.5;
                        self.draw_info_item(
                            r,
                            scene,
                            item,
                            *primary,
                            item_x,
                            item_y,
                            stack_gap,
                            badge_pad_x,
                            badge_pad_y,
                            s,
                        );
                    }
                    if let Some((next_primary, _, _, _)) = rows.get(row_at + 1) {
                        let spacing = if *primary && !next_primary {
                            section_gap
                        } else {
                            row_gap
                        };
                        if *primary && !next_primary {
                            let divider_y = row_y + row_height + (spacing - s) * 0.5;
                            scene.overlays.push((
                                divider,
                                [x + pad_x, divider_y, x + w - pad_x, divider_y + s],
                            ));
                        }
                        row_y += row_height + spacing;
                    }
                }
                self.info_rect = Some([x, y, x + w, y + h]);
                self.hud_grip(r, scene, 0, [x, y, x + w, y + h], size, f, s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_delay_reads_back_from_its_text() {
        assert_eq!(delay_seconds("+6:52"), 412.0);
        assert_eq!(delay_seconds("-1:05"), -65.0);
        assert_eq!(delay_seconds("+0:30"), 30.0);
    }

    #[test]
    fn the_information_bar_wraps_labelled_items_without_reordering_or_dropping_them() {
        let info = InfoBar {
            primary: vec![
                InfoItem::new("Time", "09:00".into(), InfoItemStyle::Clock),
                InfoItem::new("Speed", "0 km/h".into(), InfoItemStyle::Speed),
            ],
            secondary: vec![
                InfoItem::new("Next stop", "Bauernhof".into(), InfoItemStyle::NextStop),
                InfoItem::new("Delay", "-6:52".into(), InfoItemStyle::Delay),
            ],
        };
        let width = |_: usize, item: &InfoItem| {
            (item.label.chars().count() + item.value.chars().count()) as f32
        };
        assert_eq!(
            info_rows(&info.primary, 100.0, 2.0, width),
            vec![vec![0, 1]]
        );
        assert_eq!(info_rows(&info.primary, 22.0, 2.0, width), vec![vec![0, 1]]);
        assert_eq!(
            info_rows(&info.primary, 21.0, 2.0, width),
            vec![vec![0], vec![1]]
        );
        assert_eq!(
            info_rows(&info.secondary, 100.0, 2.0, width),
            vec![vec![0, 1]]
        );
        assert_eq!(
            info_rows(&info.secondary, 21.0, 2.0, width),
            vec![vec![0], vec![1]]
        );
        let narrow = info_rows(&info.secondary, 5.0, 2.0, width);
        assert_eq!(narrow, vec![vec![0], vec![1]]);
        assert_eq!(narrow.iter().flatten().copied().collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn information_bar_item_positions_advance_past_each_full_item() {
        assert_eq!(info_row_offsets(&[32.0, 48.0], 9.0, 89.0), [0.0, 41.0]);
        assert_eq!(info_row_offsets(&[32.0], 9.0, 89.0), [0.0]);
    }

    #[test]
    fn information_bar_width_stays_compact_but_respects_long_content() {
        assert_eq!(info_bar_width(1600.0, 400.0, 10.0, 920.0), 420.0);
        assert_eq!(info_bar_width(640.0, 400.0, 10.0, 576.0), 420.0);
        assert_eq!(info_bar_width(1600.0, 700.0, 20.0, 920.0), 740.0);
        assert_eq!(info_bar_width(640.0, 700.0, 10.0, 576.0), 576.0);
    }

    #[test]
    fn information_bar_dimensions_round_up_without_exceeding_available_width() {
        assert_eq!(
            quantize_info_bar_dimensions(421.0, 59.0, 900.0, 8.0),
            (424.0, 64.0)
        );
        assert_eq!(
            quantize_info_bar_dimensions(421.0, 59.0, 422.0, 8.0),
            (422.0, 64.0)
        );
        assert_eq!(
            quantize_info_bar_dimensions(421.0, 59.0, 900.0, 12.0),
            (432.0, 60.0)
        );
    }

    #[test]
    fn an_information_item_wraps_long_labels_and_values_without_clipping() {
        let item = InfoItem::new(
            "Next stop",
            "Bauernhof-Suedseite".into(),
            InfoItemStyle::NextStop,
        );
        let lines = crate::with_test_language("ENG", || {
            info_item_rows(&item, 8.0, 0.0, 0.0, false, 1.0, 1, 1, |text, _| {
                text.chars().count() as f32
            })
        });
        assert_eq!(lines.labels.join(" "), "Next stop");
        assert_eq!(lines.values.concat(), item.value);
        assert!(lines
            .labels
            .iter()
            .chain(&lines.values)
            .all(|line| line.chars().count() <= 8));
    }

    #[test]
    fn an_information_item_wraps_its_value_after_reserving_icon_space() {
        let item = InfoItem::new(
            "Next stop",
            "Bauernhof-Suedseite".into(),
            InfoItemStyle::NextStop,
        );
        let icon_and_gap = 18.0;
        let room = 25.0;
        let lines = info_item_rows(
            &item,
            room,
            icon_and_gap,
            0.0,
            false,
            2.0,
            1,
            1,
            |text, _| text.chars().count() as f32,
        );
        assert_eq!(lines.values.concat(), item.value);
        assert!(lines
            .values
            .iter()
            .all(|line| line.chars().count() as f32 + icon_and_gap <= room));
    }

    #[test]
    fn inline_information_labels_reserve_room_for_their_values() {
        let item = InfoItem::new(
            "Outside / cabin",
            "19 C / 22 C".into(),
            InfoItemStyle::Detail,
        );
        let lines = crate::with_test_language("ENG", || {
            info_item_rows(&item, 17.0, 0.0, 0.0, true, 2.0, 1, 1, |text, _| {
                text.chars().count() as f32
            })
        });
        let label_width = lines
            .labels
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap() as f32;
        let value_width = lines
            .values
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap() as f32;
        assert!(label_width + 2.0 + value_width <= 17.0);
        assert_eq!(lines.labels.concat().replace(' ', ""), "Outside/cabin");
        assert_eq!(lines.values.concat().replace(' ', ""), "19C/22C");
    }
}
