//! The view editor (Esc → Camera → "Edit the view and mirrors"): a see-through panel at the
//! side of the driver's view with sliders for the eye's place (the seat), the direction it
//! looks in, the field of view and each mirror's turn, all shown live in the cab behind it.
//! "Save for this vehicle" keeps them for the bus: the seat and the mirrors where the game
//! keeps them (`settings::bus_seats`, `settings::save_mirror_state`), and the driver's
//! camera as it now is, with its mirrors, in `views.cfg` beside the seats - written as
//! OMSI's own `[add_camera_driver]` and `[add_camera_reflexion]` entries (x, y, z, distance,
//! field of view, turn, tilt: `omsi_vehicle::vehicle::read_camera`), ready to be put into
//! the bus's file; the turn and the field of view are read back from it.

use super::{button, card, divider, grow_row, note, slider_row, title};
use omsi_plugin::api::Value;
use omsi_plugin::ui::{Anchor, Panel};
use omsi_ui::theme::Palette;

#[derive(Default)]
pub(crate) struct ViewEditor {
    said: Option<String>,
    aimed: Option<(usize, (f32, f32))>,
    look_before: Option<(String, String, (f32, f32))>,
}

const SEAT: (f32, f32) = (-0.8, 0.8);
const WIDTH: f32 = 440.0;
const TURN: (f32, f32) = (-45.0, 45.0);
const TILT: (f32, f32) = (-30.0, 30.0);
/// The field of view: below 20 the bus's own camera's.
const FOV: (f32, f32) = (0.0, 110.0);
const MIRROR_ACROSS: (f32, f32) = (-45.0, 45.0);
const MIRROR_UP: (f32, f32) = (-30.0, 30.0);

/// What the editor shows: the seat, the look's tilt and turn, the field of view (None: the
/// vehicle's) and each mirror's turn across and up.
pub(crate) struct ViewValues {
    pub seat: [f32; 3],
    pub pitch: f32,
    pub yaw: f32,
    pub fov: Option<f32>,
    pub mirrors: Vec<[f32; 2]>,
}

impl ViewEditor {
    pub(crate) fn panel(&self, app: &crate::App, pal: Palette) -> Panel {
        let mirrors = app.player.as_ref().map(|p| (0..p.vehicle.ty.def.cameras_reflexion.len()).map(|i| p.mirror_offsets.get(i).copied().unwrap_or([0.0; 2])).collect()).unwrap_or_default();
        self.panel_of(&ViewValues { seat: app.settings.seat, pitch: app.settings.seat_pitch_deg, yaw: app.cam.view_yaw, fov: app.cam.view_fov, mirrors }, pal)
    }

    pub(crate) fn panel_of(&self, v: &ViewValues, pal: Palette) -> Panel {
        let cm = |v: f32| format!("{:+.0} cm", v * 100.0);
        let deg = |v: f32| format!("{v:+.1}°");
        let mut c = vec![
            title("View editor"),
            note("The driver's view of this vehicle; the cab behind shows each change.", pal),
            slider_row("seat0", "Eye left / right", &cm(v.seat[0]), v.seat[0], SEAT, 0.01),
            slider_row("seat1", "Eye forward / back", &cm(v.seat[1]), v.seat[1], SEAT, 0.01),
            slider_row("seat2", "Eye up / down", &cm(v.seat[2]), v.seat[2], SEAT, 0.01),
            slider_row("yaw", "Look left / right", &deg(v.yaw), v.yaw, TURN, 0.5),
            slider_row("pitch", "Look up / down", &deg(v.pitch), v.pitch, TILT, 0.5),
        ];
        let fov = v.fov.unwrap_or(0.0);
        let fov_text = if fov < 20.0 { omsi_ui::tr("The vehicle's").into_owned() } else { format!("{fov:.0}°") };
        c.push(slider_row("fov", "Field of view", &fov_text, fov, FOV, 1.0));
        if !v.mirrors.is_empty() {
            c.push(divider());
        }
        for (i, o) in v.mirrors.iter().enumerate() {
            c.push(slider_row(&format!("m{i}x"), &format!("{} {} ↔", omsi_ui::tr("Mirror"), i + 1), &deg(o[0]), o[0], MIRROR_ACROSS, 0.5));
            c.push(slider_row(&format!("m{i}y"), &format!("{} {} ↕", omsi_ui::tr("Mirror"), i + 1), &deg(o[1]), o[1], MIRROR_UP, 0.5));
        }
        c.push(grow_row(vec![button("save", "Save for this vehicle", Some("save")), button("reset", "Reset", Some("restart_alt")), button("close", "Close", Some("close"))]));
        if let Some(said) = &self.said {
            c.push(note(said, pal));
        }
        // (at the side, see-through: the view it changes is the picture behind it)
        card(Anchor::Right, -12.0, 0.0, WIDTH, 170, pal, c)
    }
}

/// The editor opened: the menu closes and the driver's own camera is taken.
pub(crate) fn open(app: &mut crate::App) {
    if app.player.is_none() {
        app.service_msg = Some(("No vehicle to set the view of".into(), 3.0));
        return;
    }
    app.close_game_menu();
    app.view = "driver".into();
    if let Some(p) = app.player.as_mut() {
        p.cam_choice.0 = 0;
    }
    app.menus.game_panels.view = Some(ViewEditor::default());
}

/// The editor closed (`save`: kept for the vehicle first).
pub(crate) fn close(app: &mut crate::App, save: bool) {
    if save {
        save_view(app);
    }
    look_back(app);
    app.menus.game_panels.view = None;
}

fn look_back(app: &mut crate::App) {
    if let Some(e) = app.menus.game_panels.view.as_mut() {
        e.aimed = None;
        if let Some((_, key, l)) = e.look_before.take().filter(|b| b.0 == app.session.seat_bus) {
            restore_look(&mut app.cam.look, &mut app.cam.view_looks, &app.cam.look_view, (key, l));
        }
    }
}

fn restore_look(look: &mut (f32, f32), looks: &mut std::collections::HashMap<String, (f32, f32)>, look_view: &str, (key, before): (String, (f32, f32))) {
    if key == look_view {
        *look = before;
    } else {
        looks.insert(key, before);
    }
}

fn look_at_mirror(app: &mut crate::App, i: usize) {
    if app.menus.game_panels.view.as_ref().and_then(|e| e.look_before.as_ref()).is_some_and(|(b, k, _)| *b != app.session.seat_bus || *k != app.cam.look_view) {
        look_back(app);
    }
    let tracked = app.settings.head_tracking && app.input.headtrack.is_some();
    if app.menus.game_panels.view.as_ref().is_some_and(|e| stays_aimed(e.aimed, i, app.cam.look)) || app.view != "driver" || tracked {
        return;
    }
    if let Some((.., key)) = app.cam.f1_reset.take() {
        if key == app.cam.look_view {
            app.cam.look = (0.0, 0.0);
        } else {
            app.cam.view_looks.insert(key, (0.0, 0.0));
        }
        app.cam.view_zoom.remove("driver");
    }
    let vr = app.vr_active();
    let own_view = app.player.as_ref().is_some_and(|p| p.cam_choice.0 == 0) && !vr;
    let offset = (if own_view { app.cam.view_yaw } else { 0.0 }, if vr { 0.0 } else { app.settings.seat_pitch_deg });
    let zoom = crate::app_impl::with_zoom_hold(app.cam.view_zoom.get(&app.view).copied(), app.cam.zoom_hold.factor);
    let (view_fov, fov_setting) = (app.cam.view_fov.filter(|_| own_view), app.settings.fov);
    let window = app.gfx.surface.as_ref().map(|s| (s.config.width as f32, s.config.height as f32));
    let (Some(p), Some(fallback)) = (app.player.as_ref(), app.camera.map(|c| omsi_render::Camera { fov_deg: 60.0, ..c })) else { return };
    let Some(c) = p.vehicle.ty.def.cameras_reflexion.get(i) else { return };
    let target = p.vehicle.camera_world_full(c).0;
    let (screen_x, aspect) = match (app.menus.game_panels.laid_out, window) {
        (Some((hud, scale)), Some((w, h))) if w > 0.0 && h > 0.0 => ((((hud[0] + hud[2] - (WIDTH + 12.0) * scale) * 2.0 / w - 2.0) * 0.5).clamp(-0.8, 0.0), w / h),
        _ => (0.0, 16.0 / 9.0),
    };
    let camera = |l: (f32, f32)| {
        let mut c = p.camera_look("driver", &fallback, l, app.cam.orbit);
        c.fov_deg = crate::app_impl::drawn_fov(c.fov_deg, view_fov, fov_setting, zoom);
        c
    };
    let Some(look) = aim_look(camera, offset, target, app.cam.look, screen_x, aspect) else {
        look_back(app);
        return;
    };
    let before = (app.session.seat_bus.clone(), app.cam.look_view.clone(), app.cam.look);
    if let Some(e) = app.menus.game_panels.view.as_mut() {
        e.look_before.get_or_insert(before);
        e.aimed = Some((i, look));
    }
    app.cam.look = look;
}

fn stays_aimed(aimed: Option<(usize, (f32, f32))>, i: usize, look: (f32, f32)) -> bool {
    aimed == Some((i, look))
}

fn aim_look(camera: impl Fn((f32, f32)) -> omsi_render::Camera, offset: (f32, f32), target: glam::DVec3, start: (f32, f32), screen_x: f32, aspect: f32) -> Option<(f32, f32)> {
    let camera = |l: (f32, f32)| camera((l.0 + offset.0, l.1 + offset.1));
    let angles = |v: glam::Vec3| (v.x.atan2(v.y).to_degrees(), v.z.clamp(-1.0, 1.0).asin().to_degrees());
    let wrap = |a: f32| (a + 540.0).rem_euclid(360.0) - 180.0;
    let mut look = start;
    for _ in 0..6 {
        let cam = camera(look);
        let (mut want, have) = (angles((target - cam.position).as_vec3().normalize_or_zero()), angles(cam.forward()));
        want.0 -= (screen_x * (cam.fov_deg.to_radians() * 0.5).tan() * aspect).atan().to_degrees();
        let dp = if look.1 > 0.0 { -1.0 } else { 1.0 };
        let probe = angles(camera((look.0 + 1.0, look.1 + dp)).forward());
        let (sy, sp) = (wrap(probe.0 - have.0).signum(), ((probe.1 - have.1) * dp).signum());
        if sy == 0.0 || sp == 0.0 {
            return None;
        }
        look = (crate::input_script::cab_look_yaw(look.0 + sy * wrap(want.0 - have.0)), (look.1 + sp * (want.1 - have.1)).clamp(-85.0, 85.0));
    }
    (look.0.abs() <= 120.0).then_some(look)
}

/// Parse only slider IDs made by the mirror editor: m<index>x or m<index>y.
fn mirror_axis_id(id: &str) -> Option<(usize, usize)> {
    let rest = id.strip_prefix('m')?;
    if let Some(index) = rest.strip_suffix('x').and_then(|s| s.parse().ok()) {
        return Some((index, 0));
    }
    rest.strip_suffix('y').and_then(|s| s.parse().ok()).map(|index| (index, 1))
}

pub(crate) fn change(app: &mut crate::App, id: &str, value: &Value) {
    let Value::Num(v) = value else { return };
    let v = *v as f32;
    match id {
        "seat0" | "seat1" | "seat2" | "pitch" | "yaw" | "fov" if app.menus.game_panels.view.as_ref().is_some_and(|e| e.aimed.is_some()) => {
            look_back(app);
            change(app, id, value);
        }
        "seat0" | "seat1" | "seat2" => {
            let k = (id.as_bytes()[4] - b'0') as usize;
            app.settings.seat[k] = v;
        }
        "pitch" => app.settings.seat_pitch_deg = v,
        "yaw" => app.cam.view_yaw = v,
        "fov" => app.cam.view_fov = (v >= 20.0).then_some(v),
        m => {
            // a mirror's turn: `m<i>x` across, `m<i>y` up
            let Some((i, axis)) = mirror_axis_id(m) else { return };
            let Some(p) = app.player.as_mut() else { return };
            let n = p.vehicle.ty.def.cameras_reflexion.len();
            if i >= n {
                return;
            }
            if p.mirror_offsets.len() < n {
                p.mirror_offsets.resize(n, [0.0; 2]);
            }
            p.mirror_offsets[i][axis] = v;
            look_at_mirror(app, i);
        }
    }
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    match id {
        "save" => {
            let said = save_view(app);
            if let Some(e) = app.menus.game_panels.view.as_mut() {
                e.said = Some(said);
            }
        }
        "reset" => {
            look_back(app);
            app.settings.seat = [0.0; 3];
            app.settings.seat_pitch_deg = 0.0;
            app.cam.view_yaw = 0.0;
            app.cam.view_fov = None;
            if let Some(p) = app.player.as_mut() {
                p.mirror_offsets.iter_mut().for_each(|o| *o = [0.0; 2]);
            }
        }
        "close" => close(app, false),
        _ => {}
    }
}

/// Keep the view for the vehicle driven; what was done, in words.
fn save_view(app: &mut crate::App) -> String {
    let Some(p) = app.player.as_ref() else { return String::new() };
    let bus = crate::game_lists::seat_key(p);
    crate::settings::bus_seats::remember(&bus, app.settings.seat, app.settings.seat_pitch_deg);
    crate::settings::save_mirror_state(&p.vehicle.ty.def.path, &p.mirror_offsets, &p.mirror_shifts, &p.mirror_fovs);
    let def = &p.vehicle.ty.def;
    let n = def.cameras_driver.len().max(1);
    let Some(std) = def.cameras_driver.get(def.camera_std % n) else {
        return omsi_ui::tr("Saved:").into_owned();
    };
    let mut cam = std.clone();
    for k in 0..3 {
        cam.pos[k] += app.settings.seat[k];
    }
    cam.yaw += app.cam.view_yaw;
    cam.pitch += app.settings.seat_pitch_deg;
    if let Some(f) = app.cam.view_fov {
        cam.fov = f;
    }
    let mirrors: Vec<omsi_vehicle::Camera> = def
        .cameras_reflexion
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let o = p.mirror_offsets.get(i).copied().unwrap_or([0.0; 2]);
            omsi_vehicle::Camera { yaw: m.yaw + o[0], pitch: m.pitch + o[1], ..m.clone() }
        })
        .collect();
    match store(&bus, &cam, &mirrors) {
        Ok(path) => format!("{} {}", omsi_ui::tr("Saved for this vehicle:"), path.display()),
        Err(e) => format!("{} {}", omsi_ui::tr("Not saved:"), omsi_ui::tr(&e.to_string())),
    }
}

/// `views.cfg`: beside the seats (`lan::data_dir`).
fn views_path() -> Option<std::path::PathBuf> {
    crate::lan::data_dir().map(|d| d.join("views.cfg"))
}

/// One camera as OMSI writes it in a `.bus` file under `keyword`.
pub(crate) fn camera_entry(keyword: &str, c: &omsi_vehicle::Camera) -> String {
    format!("[{keyword}]\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n", c.pos[0], c.pos[1], c.pos[2], c.dist, c.fov, c.yaw, c.pitch)
}

/// The views of every vehicle in a `views.cfg` text: each under `[vehicle]` and its key,
/// its entries as OMSI's (`camera_entry`).
fn blocks(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        if l.trim() == "[vehicle]" {
            let key = lines.next().unwrap_or("").trim().to_string();
            let mut body = String::new();
            while let Some(n) = lines.peek() {
                if n.trim() == "[vehicle]" {
                    break;
                }
                body.push_str(n);
                body.push('\n');
                lines.next();
            }
            out.push((key, body));
        }
    }
    out
}

/// Write the view of `bus`; the file it is in.
fn store(bus: &str, driver: &omsi_vehicle::Camera, mirrors: &[omsi_vehicle::Camera]) -> std::io::Result<std::path::PathBuf> {
    let path = views_path().ok_or_else(|| std::io::Error::other("no folder for the game's data"))?;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let key = bus.trim().replace('\\', "/").to_ascii_lowercase();
    let mut out = String::from("# openOMSI: the driver's view and the mirrors of each vehicle as set in the view editor,\n# in the .bus file's own camera entries\n");
    for (k, body) in blocks(&text).into_iter().filter(|(k, _)| *k != key) {
        out.push_str(&format!("[vehicle]\n{k}\n{body}"));
    }
    out.push_str(&format!("[vehicle]\n{key}\n{}", camera_entry("add_camera_driver", driver)));
    for m in mirrors {
        out.push_str(&camera_entry("add_camera_reflexion", m));
    }
    out.push('\n');
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&path, out)?;
    Ok(path)
}

/// The turn and the field of view kept for `bus` against its standard camera `std` (none
/// kept: none).
pub(crate) fn stored(bus: &str, std: &omsi_vehicle::Camera) -> (f32, Option<f32>) {
    let text = views_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    stored_in(&text, bus, std)
}

fn stored_in(text: &str, bus: &str, std: &omsi_vehicle::Camera) -> (f32, Option<f32>) {
    let key = bus.trim().replace('\\', "/").to_ascii_lowercase();
    let Some((_, body)) = blocks(text).into_iter().find(|(k, _)| *k == key) else { return (0.0, None) };
    let mut lines = body.lines().map(str::trim).skip_while(|l| *l != "[add_camera_driver]").skip(1);
    let nums: Vec<f32> = (0..7).filter_map(|_| lines.next().map(omsi_cfg::parse_f32)).collect();
    if nums.len() < 7 {
        return (0.0, None);
    }
    let fov = ((nums[4] - std.fov).abs() > 0.05).then_some(nums[4]);
    (nums[5] - std.yaw, fov)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_is_kept_in_omsis_camera_entries() {
        let std = omsi_vehicle::Camera { pos: [-0.6, 5.1, 1.9], dist: 0.0, fov: 50.0, yaw: 0.0, pitch: -5.0, extra: None, fixed: false };
        let mine = omsi_vehicle::Camera { yaw: 4.5, fov: 62.0, ..std.clone() };
        let entry = camera_entry("add_camera_driver", &mine);
        assert_eq!(entry, "[add_camera_driver]\n-0.6\n5.1\n1.9\n0\n62\n4.5\n-5\n");
        // as the .bus reader reads it
        let bus = omsi_vehicle::Vehicle::parse(&omsi_cfg::CfgFile::from_str("view.bus", &format!("{entry}\n{}", camera_entry("add_camera_reflexion", &std))));
        assert_eq!(bus.cameras_driver, [mine.clone()]);
        assert_eq!(bus.cameras_reflexion, [std.clone()]);
        let text = format!("[vehicle]\nvehicles/a.bus\n{}[vehicle]\nvehicles/b.bus\n{entry}", camera_entry("add_camera_driver", &std));
        assert_eq!(stored_in(&text, "Vehicles\\B.bus", &std), (4.5, Some(62.0)));
        assert_eq!(stored_in(&text, "vehicles/a.bus", &std), (0.0, None));
        assert_eq!(stored_in(&text, "vehicles/c.bus", &std), (0.0, None));
        assert_eq!(blocks(&text).len(), 2);
    }
}

#[cfg(test)]
mod mirror_axis_tests {
    use super::mirror_axis_id;

    #[test]
    fn accepts_only_valid_ascii_mirror_axes() {
        assert_eq!(mirror_axis_id("m0x"), Some((0, 0)));
        assert_eq!(mirror_axis_id("m12y"), Some((12, 1)));
        for invalid in ["", "m", "m0", "m0z", "m🦊", "m0é", "mxy"] {
            assert_eq!(mirror_axis_id(invalid), None, "{invalid}");
        }
    }
}

#[cfg(test)]
mod aim_tests {
    use super::{aim_look, restore_look, stays_aimed};

    fn cam(yaw: f32, pitch: f32) -> omsi_render::Camera {
        omsi_render::Camera { position: glam::DVec3::ZERO, yaw, pitch, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 }
    }

    #[test]
    fn the_head_turns_to_the_mirror_whichever_way_the_bus_counts_its_angles() {
        let (sy, cy) = 60f32.to_radians().sin_cos();
        let (sp, cp) = (-8f32).to_radians().sin_cos();
        let target = glam::DVec3::new((sy * cp) as f64, (cy * cp) as f64, sp as f64) * 3.0;
        let along = aim_look(|l| cam(10.0 + l.0, l.1), (0.0, 0.0), target, (0.0, 0.0), 0.0, 1.0).unwrap();
        assert!((along.0 - 50.0).abs() < 0.1 && (along.1 + 8.0).abs() < 0.1, "{along:?}");
        let against = aim_look(|l| cam(10.0 - l.0, -l.1), (0.0, 0.0), target, (0.0, 0.0), 0.0, 1.0).unwrap();
        assert!((against.0 + 50.0).abs() < 0.1 && (against.1 - 8.0).abs() < 0.1, "{against:?}");
        let beside = aim_look(|l| cam(10.0 + l.0, l.1), (0.0, 0.0), target, (0.0, 0.0), -0.5, 1.0).unwrap();
        assert!((beside.0 - 50.0 - (0.5 * 30f32.to_radians().tan()).atan().to_degrees()).abs() < 0.1, "left of the panel: the head turns further, {beside:?}");
    }

    #[test]
    fn closing_the_editor_in_another_view_gives_the_driver_his_head_back_and_leaves_that_view_alone() {
        let mut looks = std::collections::HashMap::new();
        let mut view = "driver#0".to_string();
        let before = (view.clone(), (20.0, 2.0));
        let mut look = (60.0, -8.0);
        crate::input_script::swap_view_look(&mut look, &mut looks, &mut view, "chase");
        look = (12.0, 3.0);
        restore_look(&mut look, &mut looks, &view, before);
        assert_eq!(look, (12.0, 3.0));
        crate::input_script::swap_view_look(&mut look, &mut looks, &mut view, "driver#0");
        assert_eq!(look, (20.0, 2.0));
    }

    #[test]
    fn a_mirror_behind_the_driver_is_not_aimed_at() {
        let (sy, cy) = 170f32.to_radians().sin_cos();
        let target = glam::DVec3::new(sy as f64, cy as f64, 0.0) * 3.0;
        assert_eq!(aim_look(|l| cam(10.0 + l.0, l.1), (0.0, 0.0), target, (0.0, 0.0), 0.0, 1.0), None);
    }

    #[test]
    fn the_aim_keeps_the_yaw_in_range_and_works_from_the_pitch_stop() {
        let (sy, cy) = 60f32.to_radians().sin_cos();
        let target = glam::DVec3::new(sy as f64, cy as f64, 0.0) * 3.0;
        let wrapped = aim_look(|l| cam(10.0 + l.0, l.1), (0.0, 0.0), target, (350.0, 0.0), 0.0, 1.0).unwrap();
        assert!((wrapped.0 - 50.0).abs() < 0.1, "{wrapped:?}");
        let stopped = aim_look(|l| cam(10.0 + l.0, l.1.clamp(-30.0, 30.0)), (0.0, 0.0), target, (0.0, 30.0), 0.0, 1.0).unwrap();
        assert!((stopped.0 - 50.0).abs() < 0.1 && stopped.1.abs() < 0.1, "{stopped:?}");
    }

    #[test]
    fn the_aim_counts_the_editors_own_turn_and_tilt() {
        let (sy, cy) = 60f32.to_radians().sin_cos();
        let (sp, cp) = (-8f32).to_radians().sin_cos();
        let target = glam::DVec3::new((sy * cp) as f64, (cy * cp) as f64, sp as f64) * 3.0;
        let look = aim_look(|l| cam(10.0 + l.0, l.1), (20.0, -5.0), target, (0.0, 0.0), 0.0, 1.0).unwrap();
        assert!((look.0 - 30.0).abs() < 0.1 && (look.1 + 3.0).abs() < 0.1, "{look:?}");
    }

    #[test]
    fn a_mirror_aimed_at_is_aimed_at_again_once_the_head_has_moved() {
        let aimed = Some((0, (50.0, -8.0)));
        assert!(stays_aimed(aimed, 0, (50.0, -8.0)));
        assert!(!stays_aimed(aimed, 0, (0.0, 0.0)));
        assert!(!stays_aimed(aimed, 1, (50.0, -8.0)));
    }
}
