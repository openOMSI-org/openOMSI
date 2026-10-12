use glam::Vec2;
use omsi_launcher_ui::ui::Ui;
use omsi_ui::Rect;

fn frame(ui: &mut Ui, sliders: &[(&str, Rect)], v: &mut f32) -> bool {
    ui.begin(Vec2::new(800.0, 600.0), 1.0, 1.0 / 60.0);
    ui.push_clip(Rect::new(0.0, 0.0, 800.0, 200.0), 0.0);
    let mut changed = false;
    for (name, r) in sliders {
        changed |= ui.slider(name, *r, v, 0.0, 1.0, 0.0, "", &|x| format!("{x}"));
    }
    ui.pop_clip();
    ui.finish();
    changed
}

#[test]
fn a_tap_within_one_frame_sets_a_slider_where_it_lands() {
    let mut ui = Ui::new();
    let r = Rect::new(100.0, 100.0, 258.0, 34.0);
    let mut v = 0.0;
    frame(&mut ui, &[("s", r)], &mut v);
    ui.input.mouse = Vec2::new(r.x + 100.0, r.center().y);
    ui.input.pressed = true;
    ui.input.released = true;
    assert!(frame(&mut ui, &[("s", r)], &mut v));
    assert!((v - 0.5).abs() < 1e-4, "{v}");
}

#[test]
fn onto_slider_keeps_a_finger_on_the_visible_sliders_of_the_last_frame() {
    let mut ui = Ui::new();
    let mut v = 0.5;
    let shown = Rect::new(100.0, 100.0, 258.0, 34.0);
    let clipped = Rect::new(100.0, 300.0, 258.0, 34.0);
    frame(&mut ui, &[("a", shown), ("b", clipped)], &mut v);
    let start = Vec2::new(110.0, 117.0);
    assert_eq!(ui.onto_slider(start, Vec2::new(40.0, 150.0)), Some(Vec2::new(shown.x - 8.0, 117.0)));
    assert_eq!(ui.onto_slider(start, Vec2::new(200.0, 90.0)), Some(Vec2::new(200.0, 117.0)));
    assert_eq!(ui.onto_slider(Vec2::new(150.0, 60.0), start), None);
    assert_eq!(ui.onto_slider(Vec2::new(150.0, 317.0), start), None);
    frame(&mut ui, &[], &mut v);
    assert_eq!(ui.onto_slider(start, start), None);
}
