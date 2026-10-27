use super::*;
use omsi_plugin::ui::panel_from_source;

fn panel(src: &str) -> Panel {
    panel_from_source(src).unwrap()
}

#[test]
fn light_theme_uses_shared_surface_ink_and_control_colors() {
    let fonts = Fonts::new();
    let p = panel(r#"{ width = 240, children = {
        { type = "text", text = "Summary" },
        { type = "input", id = "name", placeholder = "Name" },
        { type = "button", id = "ok", text = "OK" },
    } }"#);
    let light = layout_typing_with_theme(&p, &fonts, 1.0, None, true);
    assert!(light.background.0[0] > 0.95 && light.background.0[1] > 0.95 && light.background.0[2] > 0.95);
    assert!(light.items.iter().any(|item| matches!(item, Item::Text { color, .. } if color.0[0] < 0.2)));
    assert!(light.items.iter().any(|item| matches!(item, Item::Rounded { color, .. } if color.0[0] > 0.9)));

    let dark = layout_typing_with_theme(&p, &fonts, 1.0, None, false);
    assert_eq!(dark.background.0[..3], [14.0 / 255.0, 16.0 / 255.0, 20.0 / 255.0]);
    assert!(dark.items.iter().any(|item| matches!(item, Item::Text { color, .. } if color.0[0] > 0.9)));
    assert!(dark.items.iter().any(|item| matches!(item, Item::Rounded { color, .. } if color.0[0] < 0.3)));
}

/// A panel too tall for the navigator's board is cut off: drawn and clicked only down to
/// the room there (review of #2880).
#[test]
fn a_card_cut_off_takes_no_click_below() {
    let fonts = Fonts::new();
    let p = panel(r#"{ width = 200, padding = 0, gap = 0, children = {
        { type = "button", id = "a", text = "A" },
        { type = "button", id = "b", text = "B" },
        { type = "button", id = "c", text = "C" },
    } }"#);
    let mut l = layout(&p, &fonts, 1.0);
    let full = l.h;
    let (n, hot) = (l.hits.len(), l.items.clone());
    cut(&mut l, full * 0.5);
    assert_eq!((l.h, l.hits.len(), &l.items), (full * 0.5, n, &hot), "the height only, and the hits keep their places");
    assert_eq!(l.hits.iter().find(|h| h.element.as_deref() == Some("c")).unwrap().r.h, 0.0);
    assert!(hit_at(&l, Vec2::new(10.0, full * 0.9)).is_none());
    assert!(hit_at(&l, Vec2::new(10.0, 5.0)).is_some());
    // (one that fits is left as it is)
    let mut m = layout(&p, &fonts, 1.0);
    cut(&mut m, full + 10.0);
    assert_eq!(m.h, full);
}

#[test]
fn a_stack_wraps_its_texts_and_grows_down() {
    let fonts = Fonts::new();
    let p = panel(
        r#"{ width = 200, padding = 10, gap = 5, children = {
        { type = "text", text = "Ein ziemlich langer Satz, der umbrochen werden muss", size = 14 },
        { type = "divider" },
        { type = "button", id = "go", text = "Los" },
    } }"#,
    );
    let l = layout(&p, &fonts, 1.0);
    let texts = l
        .items
        .iter()
        .filter(|i| matches!(i, Item::Text { .. }))
        .count();
    // several lines and the button's label
    assert!(texts >= 3, "{:?}", l.items);
    let lines = texts - 1;
    let want = 10.0 + lines as f32 * 14.0 * LINE + 5.0 + 1.0 + 5.0 + BUTTON_H + 10.0;
    assert!((l.h - want.ceil()).abs() < 1.0, "{} {want}", l.h);
    for i in &l.items {
        if let Item::Text {
            text,
            px,
            weight,
            at,
            ..
        } = i
        {
            assert!(
                at.x >= 10.0 && at.x + fonts.width_as_is(text, *px, *weight) <= 190.5,
                "{text} at {at}"
            );
        }
    }
    // (the card's height is whole pixels, rounded up)
    let [b] = &l.hits[..] else {
        panic!("{:?}", l.hits)
    };
    assert_eq!(
        (b.r.x, b.r.w, b.r.h, b.element.as_deref()),
        (10.0, 180.0, BUTTON_H, Some("go"))
    );
    assert!(
        (b.r.bottom() + 10.0 - l.h).abs() < 1.0,
        "{:?} in {}",
        b.r,
        l.h
    );
}

/// The icon-only tiles added on main must use the selected palette too, while
/// keeping the white icon on an explicitly lit tile and disabling its hit target.
#[test]
fn square_tiles_follow_the_palette_without_losing_lit_or_disabled_states() {
    let fonts = Fonts::new();
    let mut p = panel(r#"{ width = 280, children = {} }"#);
    let tile = |id: &str, icon: &str, focus, off, color| omsi_plugin::ui::Element {
        id: Some(id.into()), color, grow: false, clickable: false,
        kind: Kind::Tile { text: String::new(), icon: icon.into(), focus, off },
    };
    p.children = vec![
        tile("plain", "add", true, false, None),
        tile("lit", "navigation", false, false, Some(Rgba([138, 72, 26, 255]))),
        tile("off", "map", false, true, None),
    ];
    for light in [false, true] {
        let laid = layout_with_theme(&p, &fonts, 1.0, light);
        assert!(laid.hits.iter().any(|h| h.element.as_deref() == Some("plain")));
        assert!(!laid.hits.iter().any(|h| h.element.as_deref() == Some("off")));
        let icons: Vec<_> = laid.items.iter().filter_map(|i| match i {
            Item::Icon { color, .. } => Some(*color), _ => None,
        }).collect();
        assert_eq!(icons.len(), 3);
        assert_eq!(icons[1], INK, "a lit tile keeps its white icon");
        assert_eq!(icons[0].0[0] < 0.2, light, "the ordinary icon follows the theme");
        let accent = ThemeColors::new(light).accent;
        assert!(laid.items.iter().any(|i| matches!(i, Item::Rounded { color, .. } if *color == accent)));
    }
}
