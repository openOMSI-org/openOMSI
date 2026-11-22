//! The bus chooser, with the same showroom and cameras as the launcher.

use super::*;
use crate::launcher::showroom::PreviewView;

fn panes(body: Rect) -> (Rect, Rect) {
    if body.w >= 760.0 || (body.w >= 620.0 && body.h < 480.0) {
        let width = (body.w * 0.36).clamp(240.0, 400.0);
        (
            Rect::new(body.x, body.y, width, body.h),
            Rect::new(body.x + width + GAP, body.y, body.w - width - GAP, body.h),
        )
    } else {
        let height = (body.h * 0.34).clamp(80.0, 220.0);
        (
            Rect::new(body.x, body.y, body.w, height),
            Rect::new(
                body.x,
                body.y + height + GAP,
                body.w,
                (body.h - height - GAP).max(0.0),
            ),
        )
    }
}

pub(super) fn draw(sh: &mut Shell, v: &PauseView, list: &ListView, body: Rect) {
    let (left, right) = panes(body);
    sh.ui.panel(left);
    super::list_rows(sh, v, list, left.inset(10.0), true);
    sh.ui.panel(right);
    let inner = right.inset(12.0);
    let footer = Rect::new(inner.x, (inner.bottom() - 40.0).max(inner.y), inner.w, 40.0);
    let picture = Rect::new(
        inner.x,
        inner.y,
        inner.w,
        (inner.h - 140.0).max(0.0).min(inner.w * 0.64),
    );
    if picture.h >= 40.0 {
        sh.ui.p().rounded(picture, 8.0, th().surface_2);
        if let Some(texture) = sh.bus_preview.texture.filter(|_| sh.bus_preview.ready) {
            // The showroom always sends 16:9; fit it without stretching the bus.
            let height = (picture.w * 9.0 / 16.0).min(picture.h);
            let width = height * 16.0 / 9.0;
            let fitted = Rect::new(
                picture.center().x - width * 0.5,
                picture.center().y - height * 0.5,
                width,
                height,
            );
            sh.ui.image(fitted, texture, 8.0);
        } else {
            let text = if sh.bus_preview.busy {
                "Loading vehicle..."
            } else if sh.bus_preview.error.is_some() {
                "No preview of this bus"
            } else {
                "No preview"
            };
            sh.ui.text_in(
                &omsi_ui::tr(text),
                picture,
                14.0,
                Weight::Regular,
                th().text_dim,
                Align::Center,
            );
        }
        camera_controls(sh, picture);
    }
    let text_y = if picture.h >= 40.0 {
        picture.bottom() + 8.0
    } else {
        inner.y
    };
    if text_y + 44.0 < footer.y {
        sh.ui.text_in(
            &sh.bus_preview.title,
            Rect::new(inner.x, text_y, inner.w, 24.0),
            16.0,
            Weight::Bold,
            th().text,
            Align::Left,
        );
        sh.ui.text_in(
            &sh.bus_preview.meta,
            Rect::new(inner.x, text_y + 27.0, inner.w, 20.0),
            12.0,
            Weight::Regular,
            th().text_dim,
            Align::Left,
        );
    }
    if picture.bottom() + 72.0 < footer.y {
        sh.ui.text_in(
            &omsi_ui::tr("Drag to turn, scroll to zoom"),
            Rect::new(inner.x, picture.bottom() + 62.0, inner.w, 20.0),
            11.0,
            Weight::Regular,
            th().text_dim,
            Align::Left,
        );
    }
    let selected = list
        .items
        .get(list.sel)
        .is_some_and(|row| crate::game_lists::bus_preview::selectable(row.0));
    if selected
        && sh.ui.button(
            "pause-bus-continue",
            footer,
            &omsi_ui::tr("Continue"),
            Some("arrow_forward"),
            ButtonKind::Primary,
        )
    {
        sh.actions.push(Action::Choose(list.sel));
    }
}

fn camera_controls(sh: &mut Shell, picture: Rect) {
    let bar_h = if picture.w < 420.0 { 28.0 } else { 32.0 };
    let labels = [
        (PreviewView::Outside, "Outside view", "directions_bus"),
        (PreviewView::Passenger, "Passenger view", "groups"),
        (PreviewView::Driver, "Driver's view", "person"),
    ];
    let width = ((picture.w - 16.0) / 3.0).min(150.0);
    let bar = Rect::new(
        picture.right() - width * 3.0 - 8.0,
        picture.y + 8.0,
        width * 3.0,
        bar_h,
    );
    for (i, (view, label, icon)) in labels.into_iter().enumerate() {
        if view != PreviewView::Outside && !sh.bus_preview.views.contains(&view.name()) {
            continue;
        }
        let rect = Rect::new(bar.x + i as f32 * width, bar.y, width - 3.0, bar.h);
        let text = if width < 112.0 {
            "".into()
        } else {
            omsi_ui::tr(label)
        };
        if sh.ui.button(
            &format!("pause-bus-view-{}", view.name()),
            rect,
            &text,
            Some(icon),
            if sh.bus_preview.view == view {
                ButtonKind::Primary
            } else {
                ButtonKind::Normal
            },
        ) {
            sh.bus_preview.view = view;
        }
        sh.ui.tooltip(rect, &omsi_ui::tr(label));
    }
    let drag_area = Rect::new(
        picture.x,
        bar.bottom() + 4.0,
        picture.w,
        (picture.bottom() - bar.bottom() - 4.0).max(0.0),
    );
    let (_, down, _) = sh.ui.interact(id_of("pause-bus-orbit"), drag_area);
    let mouse = sh.ui.input.mouse;
    if down {
        if let Some(last) = sh.bus_preview.drag {
            let delta = mouse - last;
            sh.bus_preview.orbit.0 += delta.x;
            sh.bus_preview.orbit.1 += delta.y;
        }
        sh.bus_preview.drag = Some(mouse);
    } else {
        sh.bus_preview.drag = None;
    }
    if sh.ui.hover(picture) && sh.ui.input.wheel.y != 0.0 {
        sh.bus_preview.zoom =
            (sh.bus_preview.zoom * (-sh.ui.input.wheel.y * 0.12).exp()).clamp(0.3, 4.0);
        sh.ui.input.wheel.y = 0.0;
    }
}

#[cfg(test)]
#[path = "vehicles_tests.rs"]
mod tests;
