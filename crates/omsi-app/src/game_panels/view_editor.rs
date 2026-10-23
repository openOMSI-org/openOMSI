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

#[derive(Default)]
pub(crate) struct ViewEditor {
    said: Option<String>,
}

const SEAT: (f32, f32) = (-0.8, 0.8);
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
    pub(crate) fn panel(&self, app: &crate::App) -> Panel {
        let mirrors = app.player.as_ref().map(|p| (0..p.vehicle.ty.def.cameras_reflexion.len()).map(|i| p.mirror_offsets.get(i).copied().unwrap_or([0.0; 2])).collect()).unwrap_or_default();
        self.panel_of(&ViewValues { seat: app.settings.seat, pitch: app.settings.seat_pitch_deg, yaw: app.cam.view_yaw, fov: app.cam.view_fov, mirrors })
    }

    pub(crate) fn panel_of(&self, v: &ViewValues) -> Panel {
        let cm = |v: f32| format!("{:+.0} cm", v * 100.0);
        let deg = |v: f32| format!("{v:+.1}°");
        let mut c = vec![
            title("View editor"),
            note("The driver's view of this vehicle; the cab behind shows each change."),
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
            c.push(note(said));
        }
        // (at the side, see-through: the view it changes is the picture behind it)
        card(Anchor::Right, -12.0, 0.0, 440.0, 170, c)
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
    app.menus.game_panels.view = None;
}

pub(crate) fn change(app: &mut crate::App, id: &str, value: &Value) {
    let Value::Num(v) = value else { return };
    let v = *v as f32;
    match id {
        "seat0" | "seat1" | "seat2" => {
            let k = (id.as_bytes()[4] - b'0') as usize;
            app.settings.seat[k] = v;
        }
        "pitch" => app.settings.seat_pitch_deg = v,
        "yaw" => app.cam.view_yaw = v,
        "fov" => app.cam.view_fov = (v >= 20.0).then_some(v),
        m => {
            // a mirror's turn: `m<i>x` across, `m<i>y` up
            let Some((i, axis)) = m.strip_prefix('m').and_then(|r| Some((r[..r.len() - 1].parse::<usize>().ok()?, r.as_bytes()[r.len() - 1]))) else { return };
            let Some(p) = app.player.as_mut() else { return };
            let n = p.vehicle.ty.def.cameras_reflexion.len();
            if i >= n {
                return;
            }
            if p.mirror_offsets.len() < n {
                p.mirror_offsets.resize(n, [0.0; 2]);
            }
            p.mirror_offsets[i][if axis == b'x' { 0 } else { 1 }] = v;
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
