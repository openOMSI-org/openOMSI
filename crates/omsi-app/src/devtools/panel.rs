//! The developer tools' panel at the right of the picture, in the launcher's toolkit: the
//! variables, the overlays, the mirrors and the files, one page each.

use glam::Vec2;
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

use super::{overlay, set_value, DevTools, Kind, Request};
use omsi_ui::theme::*;
use omsi_ui::kit::{ButtonKind, Ui};
use crate::player::Player;

pub(crate) const TABS: [&str; 4] = ["Variables", "Overlays", "Mirrors", "Files"];

const WIDTH: f32 = 400.0;
const LINE: f32 = 26.0;
/// At most this many variables are listed for a search (a bus has thousands).
const LISTED: usize = 200;

/// The panel; what it asks of the game.
pub(crate) fn draw(ui: &mut Ui, d: &mut DevTools, mut p: Option<&mut Player>, size: Vec2) -> Vec<Request> {
    let theme = ui.theme;
    let mut out = Vec::new();
    let r = Rect::new((size.x - WIDTH - 12.0).max(0.0), 12.0, WIDTH.min(size.x), (size.y - 24.0).max(200.0));
    ui.panel(r);
    let inner = r.inset(14.0);
    ui.text(&omsi_ui::tr("Developer tools"), Vec2::new(inner.x, inner.y + 15.0), 16.0, Weight::Bold, theme.TEXT, Align::Left);
    if ui.icon_button("dt-close", Vec2::new(inner.right() - 10.0, inner.y + 9.0), 12.0, "close", "Close (Ctrl+Shift+D)") {
        out.push(Request::Close);
    }
    let bus = p.as_ref().map(|p| p.vehicle.ty.def.path.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_else(|| omsi_ui::tr("no bus of one's own").into_owned());
    ui.text_in(&bus, Rect::new(inner.x, inner.y + 24.0, inner.w, 18.0), 12.0, Weight::Regular, theme.TEXT_DIM, Align::Left);
    let tabs = TABS.map(|tab| omsi_ui::tr(tab).into_owned());
    let tab_labels = tabs.iter().map(|tab| tab.as_str()).collect::<Vec<_>>();
    ui.segmented("dt-tabs", Rect::new(inner.x, inner.y + 48.0, inner.w, 30.0), &mut d.tab, &tab_labels);
    let body = Rect::new(inner.x, inner.y + 90.0, inner.w, inner.h - 90.0);
    match d.tab {
        0 => variables(ui, d, p.as_deref_mut(), body),
        1 => overlays(ui, d, body),
        2 => mirrors(ui, d, p.as_deref_mut(), body, &mut out),
        _ => files(ui, d, body, &mut out),
    }
    out
}

/// A variable's value as the list shows it.
fn shown(v: f32) -> String {
    if v.fract() == 0.0 && v.abs() < 1e7 {
        format!("{v:.0}")
    } else {
        format!("{v:.4}")
    }
}

fn value_of(p: Option<&Player>, name: &str, string: bool) -> String {
    let Some(p) = p else { return "-".into() };
    let v = &p.vehicle;
    if string {
        match v.ty.program.str_var(name).and_then(|i| v.state.str_vars.get(i as usize)) {
            Some(s) => format!("\"{s}\""),
            None => omsi_ui::tr("(none)").into_owned(),
        }
    } else {
        v.var(name).map(shown).unwrap_or_else(|| omsi_ui::tr("(none)").into_owned())
    }
}

/// The watch list (click a line to edit it), the chosen variable's value, and the search
/// through all of them.
fn variables(ui: &mut Ui, d: &mut DevTools, mut p: Option<&mut Player>, r: Rect) {
    let theme = ui.theme;
    let mut y = r.y;
    ui.heading(Rect::new(r.x, y, r.w, 26.0), "Watched", None);
    y += 26.0;
    let watch_h = (d.watch.len().max(1) as f32 * LINE).min(r.h * 0.4);
    let mut remove = None;
    let mut choose = None;
    let pv = p.as_deref();
    let (watch, selected) = (&d.watch, d.selected);
    ui.scroll_area("dt-watch", Rect::new(r.x, y, r.w, watch_h), &mut |ui, area| {
        if watch.is_empty() {
            ui.text_in(&omsi_ui::tr("Nothing watched: pick variables from the list below"), Rect::new(area.x, area.y, area.w, LINE), 12.0, Weight::Regular, theme.TEXT_FAINT, Align::Left);
            return LINE;
        }
        for (k, w) in watch.iter().enumerate() {
            let row = Rect::new(area.x, area.y + k as f32 * LINE, area.w - 8.0, LINE - 2.0);
            if !ui.rect_visible(row) {
                continue;
            }
            if ui.row(&format!("dt-w{k}"), Rect::new(row.x, row.y, row.w - 28.0, row.h), selected == Some(k)) {
                choose = Some(k);
            }
            let name = if w.string { format!("${}", w.name) } else { w.name.clone() };
            ui.text_in(&name, Rect::new(row.x + 8.0, row.y, row.w * 0.55, row.h), 12.5, Weight::Medium, theme.TEXT, Align::Left);
            let held = if w.hold.is_some() { format!("  [{}]", omsi_ui::tr("held")) } else { String::new() };
            ui.text_in(&format!("{}{held}", value_of(pv, &w.name, w.string)), Rect::new(row.x + row.w * 0.55, row.y, row.w * 0.45 - 34.0, row.h), 12.5, Weight::Regular, if w.hold.is_some() { theme.ACCENT } else { theme.TEXT_SOFT }, Align::Right);
            if ui.icon_button(&format!("dt-wx{k}"), Vec2::new(row.right() - 12.0, row.center().y), 10.0, "close", "Stop watching") {
                remove = Some(k);
            }
        }
        watch.len() as f32 * LINE
    });
    y += watch_h + 8.0;
    if let Some(k) = choose {
        d.selected = Some(k);
        d.edit = value_of(p.as_deref(), &d.watch[k].name, d.watch[k].string).trim_matches('"').to_string();
    }
    if let Some(k) = remove {
        d.remove_watch(k);
    }
    // the chosen one: a value to set it to, once or held
    if let Some(k) = d.selected.filter(|k| *k < d.watch.len()) {
        let w = d.watch[k].clone();
        let field = Rect::new(r.x, y, r.w - 150.0, 30.0);
        let placeholder = format!("{}：{}", omsi_ui::tr("Variable value"), w.name);
        ui.text_input("dt-edit", field, &mut d.edit, &placeholder, None);
        let set = omsi_ui::tr("Set");
        if ui.button("dt-set", Rect::new(field.right() + 6.0, y, 66.0, 30.0), &set, None, ButtonKind::Normal) {
            if let Some(p) = p.as_deref_mut() {
                set_value(&mut p.vehicle, &w.name, w.string, d.edit.trim());
            }
        }
        let (label, kind) = if w.hold.is_some() { ("Let go", ButtonKind::Primary) } else { ("Hold", ButtonKind::Normal) };
        let label = omsi_ui::tr(label);
        if ui.button("dt-hold", Rect::new(field.right() + 78.0, y, 72.0, 30.0), &label, None, kind) {
            d.watch[k].hold = match w.hold {
                Some(_) => None,
                None => Some(d.edit.trim().to_string()),
            };
        }
        y += 40.0;
    }
    ui.heading(Rect::new(r.x, y, r.w, 26.0), "All variables", None);
    y += 26.0;
    let placeholder = omsi_ui::tr("Search variables ($ for string variables)");
    ui.text_input("dt-filter", Rect::new(r.x, y, r.w, 30.0), &mut d.filter, &placeholder, Some("search"));
    y += 38.0;
    let Some(pl) = p.as_deref() else { return };
    let prog = &pl.vehicle.ty.program;
    let q = d.filter.trim().to_ascii_lowercase();
    let (strings, q) = match q.strip_prefix('$') {
        Some(rest) => (true, rest.to_string()),
        None => (false, q),
    };
    let names: &[String] = if strings { &prog.str_var_names } else { &prog.var_names };
    let found: Vec<&String> = names.iter().filter(|n| q.is_empty() || n.to_ascii_lowercase().contains(&q)).take(LISTED).collect();
    let mut add = None;
    ui.scroll_area("dt-all", Rect::new(r.x, y, r.w, (r.bottom() - y).max(LINE)), &mut |ui, area| {
        for (k, n) in found.iter().enumerate() {
            let row = Rect::new(area.x, area.y + k as f32 * LINE, area.w - 8.0, LINE - 2.0);
            if !ui.rect_visible(row) {
                continue;
            }
            if ui.row(&format!("dt-a{k}"), row, false) {
                add = Some(n.to_string());
            }
            ui.text_in(n, Rect::new(row.x + 8.0, row.y, row.w * 0.6, row.h), 12.5, Weight::Regular, theme.TEXT_SOFT, Align::Left);
            ui.text_in(&value_of(Some(pl), n, strings), Rect::new(row.x + row.w * 0.6, row.y, row.w * 0.4 - 8.0, row.h), 12.5, Weight::Regular, theme.TEXT_DIM, Align::Right);
        }
        found.len() as f32 * LINE
    });
    if let Some(n) = add {
        d.add_watch(&n, strings);
        d.edit = value_of(Some(pl), &n, strings).trim_matches('"').to_string();
    }
}

fn overlays(ui: &mut Ui, d: &mut DevTools, r: Rect) {
    let mut y = r.y;
    ui.heading(Rect::new(r.x, y, r.w, 26.0), "Over the picture", None);
    y += 28.0;
    for (k, name) in overlay::NAMES.iter().enumerate() {
        let name = omsi_ui::tr(name);
        ui.toggle(&format!("dt-o{k}"), Rect::new(r.x, y, r.w, ROW - 6.0), &mut d.overlays.on[k], &name);
        y += ROW - 4.0;
    }
    y += 8.0;
    let half = (r.w - 8.0) * 0.5;
    let all = omsi_ui::tr("All");
    if ui.button("dt-oall", Rect::new(r.x, y, half, 30.0), &all, None, ButtonKind::Normal) {
        d.overlays.on = [true; overlay::NAMES.len()];
    }
    let none = omsi_ui::tr("None");
    if ui.button("dt-onone", Rect::new(r.x + half + 8.0, y, half, 30.0), &none, None, ButtonKind::Normal) {
        d.overlays.on = [false; overlay::NAMES.len()];
    }
}

/// A mirror camera as the player has turned and moved it, as the `.bus` would give it:
/// the `[add_camera_reflexion_2]` block (position, distance, field of view, yaw, pitch,
/// radius).
pub(crate) fn mirror_block(p: &Player, i: usize) -> Option<String> {
    let c = p.vehicle.ty.def.cameras_reflexion.get(i)?;
    let c = crate::camera_util::adjusted(c, p.mirror_shifts.get(i).copied().unwrap_or([0.0; 3]), p.mirror_fovs.get(i).copied().unwrap_or(0.0));
    let off = p.mirror_offsets.get(i).copied().unwrap_or([0.0; 2]);
    let n = |v: f32| {
        let s = format!("{v:.3}");
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s == "-0" { "0".to_string() } else { s }
    };
    Some(format!(
        "[add_camera_reflexion_2]\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
        n(c.pos[0]),
        n(c.pos[1]),
        n(c.pos[2]),
        n(c.dist),
        n(c.fov),
        n(c.yaw + off[0]),
        n(c.pitch + off[1]),
        n(c.extra.unwrap_or(0.0)),
    ))
}

/// Mirror `i` moved by `shift` (m, bus frame), turned by `turn` (yaw, pitch degrees) and
/// widened by `dfov` degrees, from what the `.bus` gives.
pub(crate) fn adjust_mirror(p: &mut Player, i: usize, shift: [f32; 3], turn: [f32; 2], dfov: f32) {
    let n = p.vehicle.ty.def.cameras_reflexion.len();
    if i >= n {
        return;
    }
    p.mirror_shifts.resize(n.max(p.mirror_shifts.len()), [0.0; 3]);
    p.mirror_offsets.resize(n.max(p.mirror_offsets.len()), [0.0; 2]);
    p.mirror_fovs.resize(n.max(p.mirror_fovs.len()), 0.0);
    p.mirror_shifts[i] = shift;
    p.mirror_offsets[i] = turn;
    p.mirror_fovs[i] = dfov;
    p.mirrors_dirty = true;
}

fn mirrors(ui: &mut Ui, d: &mut DevTools, p: Option<&mut Player>, r: Rect, out: &mut Vec<Request>) {
    let theme = ui.theme;
    let Some(p) = p else { return };
    let n = p.vehicle.ty.def.cameras_reflexion.len();
    let mut y = r.y;
    if n == 0 {
        ui.text_in(&omsi_ui::tr("This bus has no mirrors"), Rect::new(r.x, y, r.w, LINE), 13.0, Weight::Regular, theme.TEXT_DIM, Align::Left);
        return;
    }
    d.mirror = d.mirror.min(n - 1);
    ui.heading(Rect::new(r.x, y, r.w, 26.0), "Mirrors", None);
    y += 26.0;
    for (k, c) in p.vehicle.ty.def.cameras_reflexion.iter().enumerate().take(8) {
        let row = Rect::new(r.x, y, r.w, LINE - 2.0);
        if ui.row(&format!("dt-m{k}"), row, d.mirror == k) {
            d.mirror = k;
        }
        let yaw = omsi_ui::tr("yaw");
        let pitch = omsi_ui::tr("pitch");
        let fov = omsi_ui::tr("fov");
        ui.text_in(&format!("{k}   {:.2}, {:.2}, {:.2}   {yaw} {:.0}  {pitch} {:.0}  {fov} {:.0}", c.pos[0], c.pos[1], c.pos[2], c.yaw, c.pitch, c.fov), row.pad(8.0, 0.0), 12.0, Weight::Regular, theme.TEXT_SOFT, Align::Left);
        y += LINE;
    }
    y += 8.0;
    let i = d.mirror;
    let mut shift = p.mirror_shifts.get(i).copied().unwrap_or([0.0; 3]);
    let mut turn = p.mirror_offsets.get(i).copied().unwrap_or([0.0; 2]);
    let mut dfov = p.mirror_fovs.get(i).copied().unwrap_or(0.0);
    let mut changed = false;
    let m = |v: f32| format!("{v:+.2} m");
    let deg = |v: f32| format!("{v:+.1}°");
    for (k, (label, v)) in ["Across", "Along", "Up"].into_iter().zip(shift.iter_mut()).enumerate() {
        let label = omsi_ui::tr(label);
        changed |= ui.slider(&format!("dt-ms{k}"), Rect::new(r.x, y, r.w, ROW - 4.0), v, -0.5, 0.5, 0.01, &label, &m);
        y += ROW;
    }
    for (k, (label, v)) in ["Turn", "Tilt"].into_iter().zip(turn.iter_mut()).enumerate() {
        let label = omsi_ui::tr(label);
        changed |= ui.slider(&format!("dt-mt{k}"), Rect::new(r.x, y, r.w, ROW - 4.0), v, -30.0, 30.0, 0.25, &label, &deg);
        y += ROW;
    }
    let field_of_view = omsi_ui::tr("Field of view");
    changed |= ui.slider("dt-mf", Rect::new(r.x, y, r.w, ROW - 4.0), &mut dfov, -30.0, 30.0, 0.5, &field_of_view, &deg);
    y += ROW + 4.0;
    if changed {
        adjust_mirror(p, i, shift, turn, dfov);
    }
    let half = (r.w - 8.0) * 0.5;
    let copy = omsi_ui::tr("Copy the block");
    if ui.button("dt-mcopy", Rect::new(r.x, y, half, 32.0), &copy, Some("content_copy"), ButtonKind::Primary) {
        out.push(Request::CopyMirror(i));
    }
    let reset = omsi_ui::tr("As the bus has it");
    if ui.button("dt-mreset", Rect::new(r.x + half + 8.0, y, half, 32.0), &reset, None, ButtonKind::Normal) {
        adjust_mirror(p, i, [0.0; 3], [0.0; 2], 0.0);
    }
    y += 42.0;
    if let Some(block) = mirror_block(p, i) {
        for line in block.lines() {
            ui.text_in(line, Rect::new(r.x + 4.0, y, r.w, 18.0), 12.0, Weight::Regular, theme.TEXT_SOFT, Align::Left);
            y += 17.0;
        }
    }
}

fn files(ui: &mut Ui, d: &mut DevTools, r: Rect, out: &mut Vec<Request>) {
    let theme = ui.theme;
    let mut y = r.y;
    let auto_reload = omsi_ui::tr("Read a file again when it changes");
    ui.toggle("dt-auto", Rect::new(r.x, y, r.w, ROW - 6.0), &mut d.auto_reload, &auto_reload);
    y += ROW + 4.0;
    ui.heading(Rect::new(r.x, y, r.w, 26.0), "Read again now", None);
    y += 28.0;
    for (kind, label) in Kind::BUTTONS {
        let label = omsi_ui::tr(label);
        if ui.button(&format!("dt-r{kind:?}"), Rect::new(r.x, y, r.w, 30.0), &label, Some("refresh"), ButtonKind::Normal) {
            out.push(Request::Reload(kind));
        }
        y += 36.0;
    }
    let open_log = omsi_ui::tr("Open the log");
    if ui.button("dt-log", Rect::new(r.x, y, r.w, 30.0), &open_log, Some("description"), ButtonKind::Ghost) {
        out.push(Request::OpenLog);
    }
    y += 42.0;
    ui.heading(Rect::new(r.x, y, r.w, 26.0), "Last done", None);
    y += 26.0;
    for m in d.messages.iter().rev() {
        let h = ui.paragraph_height(m, r.w, 12.0, Weight::Regular);
        if y + h > r.bottom() {
            break;
        }
        ui.paragraph(m, Vec2::new(r.x, y), r.w, 12.0, Weight::Regular, theme.TEXT_SOFT);
        y += h + 6.0;
    }
    if y + 26.0 < r.bottom() {
        ui.heading(Rect::new(r.x, y, r.w, 26.0), "Files looked at", None);
        y += 26.0;
        for (kind, path, _) in &d.files {
            if y + 16.0 > r.bottom() {
                break;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let kind = match kind {
                Kind::Paths => "Paths",
                Kind::Sound => "Sound",
                Kind::Bus => "Bus",
                Kind::Model => "Model",
                Kind::Constfiles => "Constant files",
                Kind::Scripts => "Scripts",
            };
            ui.text_in(&format!("{name}  ({})", omsi_ui::tr(kind)), Rect::new(r.x, y, r.w, 16.0), 11.5, Weight::Regular, theme.TEXT_DIM, Align::Left);
            y += 16.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every page is drawn without a bus of one's own, and asks nothing of the game.
    #[test]
    fn every_page_draws_without_a_bus() {
        let mut ui = Ui::new();
        let mut d = DevTools::new(None);
        let size = Vec2::new(1280.0, 800.0);
        for tab in 0..TABS.len() {
            d.tab = tab;
            ui.begin(size, 1.0, 0.016);
            assert!(draw(&mut ui, &mut d, None, size).is_empty());
            ui.finish();
        }
    }

    #[test]
    fn values_are_shown_whole_or_to_four_places() {
        assert_eq!(super::shown(3.0), "3");
        assert_eq!(super::shown(-0.25), "-0.2500");
        assert_eq!(super::shown(1.0e9), "1000000000.0000");
    }

    #[test]
    fn traditional_chinese_localizes_developer_tool_labels() {
        crate::with_test_language("ZHT", || {
            for (source, expected) in [
            ("Variables", "變數"),
            ("Overlays", "疊加項目"),
            ("Files", "檔案"),
            ("Watched", "監看的變數"),
            ("All variables", "所有變數"),
            ("Search variables ($ for string variables)", "搜尋變數（字串變數請加上 $）"),
            ("no bus of one's own", "尚未選擇巴士"),
            ("held", "固定中"),
            ("Set", "設定"),
            ("Hold", "固定"),
            ("Let go", "解除固定"),
            ("Across", "橫向"),
            ("Along", "縱向"),
            ("Turn", "水平旋轉"),
            ("Tilt", "傾斜"),
            ("Field of view", "視野角度 (FOV)"),
            ("yaw", "偏航"),
            ("pitch", "俯仰"),
            ("fov", "視野角"),
            ("(none)", "（無）"),
            ("Last done", "最近執行的操作"),
            ("Files looked at", "已檢查的檔案"),
            ("Entry point", "入口"),
            ("Variable value", "變數值"),
            ("mirror 2  fov 60", "後視鏡 2　視野角 60"),
            ("Paths", "路徑"),
            ("Path points", "路徑節點"),
            ("stamper", "打票機"),
            ("driver view", "駕駛員視角"),
            ("passenger view", "乘客視角"),
            ("(path point 4)", "（路徑節點 4）"),
            ("Cabin reloaded: 3 path points, 4 places, 2 entries, 2 exits", "乘客艙已重新載入：3 個路徑點、4 個位置、2 個入口、2 個出口"),
            ] {
                assert_eq!(omsi_ui::tr(source), expected, "missing Traditional Chinese translation for {source}");
            }
        });
    }
}
