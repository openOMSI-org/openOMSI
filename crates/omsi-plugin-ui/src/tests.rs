use super::*;
use omsi_plugin::ui::{panel_from_source, Anchor};

fn panel(src: &str) -> Panel {
    panel_from_source(src).unwrap()
}

#[test]
fn a_row_gives_the_rest_to_what_grows() {
    let fonts = Fonts::new();
    let p = panel(
        r#"{ width = 300, padding = 10, children = { { type = "row", gap = 8, children = {
        { type = "icon", name = "schedule", size = 20 },
        { type = "bar", value = 0.5, grow = true },
        { type = "badge", text = "+2" },
    } } } }"#,
    );
    let l = layout(&p, &fonts, 1.0);
    let rounded: Vec<Rect> = l
        .items
        .iter()
        .filter_map(|i| {
            if let Item::Rounded { r, .. } = i {
                Some(*r)
            } else {
                None
            }
        })
        .collect();
    // the track from after the icon to before the badge, the badge at the right edge
    let (track, badge) = (rounded[0], rounded[2]);
    assert_eq!(track.x, 10.0 + 20.0 + 8.0);
    assert!(
        (badge.right() - 290.0).abs() < 0.01 && (track.right() + 8.0 - badge.x).abs() < 0.01,
        "{track:?} {badge:?}"
    );
    assert_eq!(l.h, (10.0 + BADGE_H + 10.0f32).ceil());
}

#[test]
fn panels_are_placed_by_their_anchor_and_stay_on_the_screen() {
    let screen = Vec2::new(1920.0, 1080.0);
    let mut p = panel("{ x = 16, y = 20 }");
    assert_eq!(place(&p, 300.0, 100.0, screen), Vec2::new(16.0, 20.0));
    p.anchor = Anchor::BottomRight;
    assert_eq!(
        place(&p, 300.0, 100.0, screen),
        Vec2::new(1920.0 - 316.0, 1080.0 - 120.0)
    );
    p.anchor = Anchor::Center;
    assert_eq!(
        place(&p, 300.0, 100.0, screen),
        Vec2::new(810.0 + 16.0, 490.0 + 20.0)
    );
    p.anchor = Anchor::TopLeft;
    p.x = -50.0;
    assert_eq!(place(&p, 300.0, 100.0, screen).x, 0.0);
}

#[test]
fn clicks_find_the_innermost_part() {
    let fonts = Fonts::new();
    let p = panel(
        r#"{ width = 300, clickable = true, children = {
        { type = "row", id = "line", clickable = true, children = {
            { type = "text", text = "Linie 5", grow = true },
            { type = "button", id = "take", text = "Nehmen" },
        } },
        { type = "text", text = "nicht klickbar" },
    } }"#,
    );
    let l = layout(&p, &fonts, 1.0);
    let at = |i: usize| l.hits[i].r.center();
    let element = |p: Vec2| hit_at(&l, p).and_then(|i| l.hits[i].element.clone());
    let button = l
        .hits
        .iter()
        .position(|h| h.element.as_deref() == Some("take"))
        .unwrap();
    assert_eq!(element(at(button)).as_deref(), Some("take"));
    assert_eq!(
        element(Vec2::new(20.0, at(button).y)).as_deref(),
        Some("line")
    );
    // below the row: the panel itself
    assert_eq!(hit_at(&l, Vec2::new(20.0, l.h - 14.0)), Some(0));
    assert_eq!(element(Vec2::new(20.0, l.h - 14.0)), None);
}

#[test]
fn a_toast_is_a_card_with_its_colour() {
    let fonts = Fonts::new();
    let t = Toast {
        owner: 1,
        serial: 1,
        text: "Fahrt beendet".into(),
        title: Some("Karriere".into()),
        icon: Some("payments".into()),
        color: None,
        seconds: 5.0,
        age: 0.0,
    };
    let l = layout(&toast_panel(&t), &fonts, 1.0);
    assert_eq!(l.w, TOAST_W);
    assert!(l.accent.is_some());
    assert_eq!(
        l.items
            .iter()
            .filter(|i| matches!(i, Item::Icon { .. }))
            .count(),
        1
    );
    assert_eq!(
        l.items
            .iter()
            .filter(|i| matches!(i, Item::Text { .. }))
            .count(),
        2
    );
}
