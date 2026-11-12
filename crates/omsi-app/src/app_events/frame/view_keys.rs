//! Looking round, zooming, the mirrors' keys, the clock's keys and the free camera in the
//! window's frame.

use super::*;
use crate::app_impl::{JumpCx, ShiftClockCx};

crate::app::stage_ctx!(
    /// The view keys' part of the game.
    ViewKeysCx { reads: [net, menus, people, world], writes: [input, view, player, clock, traffic, gfx, ui] }
);

/// The keys held this frame that turn the head, aim the mirrors, run the clock, zoom
/// and fly the free camera.
pub(super) fn view_keys(mut cx: ViewKeysCx, dt: f32) {
    let size = cx.gfx.gfx.surface.as_ref().map(|s| (s.config.width, s.config.height));
    aim_flashlight(&mut cx, size);
    // looking around and zooming work in every view, not only the free camera
    crate::app_impl::sync_view_look(cx.view.cam, cx.view.view, cx.player.player.as_ref());
    zoom_hold_keys(&mut cx, dt);
    if cx.player.player.is_some() && *cx.view.view != "free" {
        // looking around with the keyboard: Alt + I/J/K/L (the plain letters
        // belong to the bus - L is the headlights in Inputs/keyboard.cfg)
        let step = 60.0 * dt;
        let ctrl_alt = mirror_keys(&mut cx, dt);
        // a controller's look buttons (Settings → Controllers: view_look_*)
        cx.view.cam.look.0 += step * 1.5 * (cx.input.pad_look[1] as i32 - cx.input.pad_look[0] as i32) as f32;
        cx.view.cam.look.1 = (cx.view.cam.look.1 + step * 0.7 * (cx.input.pad_look[2] as i32 - cx.input.pad_look[3] as i32) as f32).clamp(-85.0, 85.0);
        // with a wheel steering and the setting off, the arrow keys look around (a glance;
        // in OMSI they switch the cameras with a wheel as without one, the default, #2164)
        if !ctrl_alt && !cx.common.settings.arrows_switch_cams && cx.input.controllers.as_ref().is_some_and(|c| c.wheel_steering()) && !cx.input.keys.contains(&KeyCode::ControlLeft) && !cx.input.keys.contains(&KeyCode::ControlRight) {
            // a glance: held, the head turns (in the driver's seat to 140 degrees
            // at most, or no further than the mouse had it); let go, it comes back
            // to the road - held, it went round and round, and the other key never
            // brought it back straight
            let uses = |p: &crate::player::Player, a: &str| omsi_sim::input::engine_action(a).is_some() || omsi_input::actions::bus_uses_action(&p.vehicle.ty.program, a, crate::player::ACTION_ALIASES);
            let bus_key = |k: KeyCode| crate::keys::dik_code(k).is_some_and(|sc| cx.player.player.as_ref().is_some_and(|p| p.bindings.iter().any(|b| b.scan_code == sc && b.chord() == 0 && uses(p, &b.action))));
            let held = |k: KeyCode| cx.input.keys.contains(&k) && !bus_key(k);
            let (l, r) = (held(KeyCode::ArrowLeft), held(KeyCode::ArrowRight));
            if l || r {
                let y = cx.view.cam.look.0 + step * 1.5 * (r as i32 - l as i32) as f32;
                cx.view.cam.look.0 = if *cx.view.view == "pax" { y } else { y.clamp(cx.view.cam.look.0.min(-140.0), cx.view.cam.look.0.max(140.0)) };
                cx.input.arrow_glance = true;
            } else if cx.input.arrow_glance {
                cx.view.cam.look.0 *= (-6.0 * dt).exp();
                // (down to a hundredth of a degree before it is set to 0: at half a
                // degree the last step was a visible snap of several pixels)
                if cx.view.cam.look.0.abs() < 0.02 {
                    cx.view.cam.look.0 = 0.0;
                    cx.input.arrow_glance = false;
                }
            }
            if held(KeyCode::ArrowUp) {
                cx.view.cam.look.1 = (cx.view.cam.look.1 + step * 0.7).min(85.0);
            }
            if held(KeyCode::ArrowDown) {
                cx.view.cam.look.1 = (cx.view.cam.look.1 - step * 0.7).max(-85.0);
            }
        }
        let alt = cx.input.keys.contains(&KeyCode::AltLeft)
            || cx.input.keys.contains(&KeyCode::AltRight);
        if alt && cx.input.keys.contains(&KeyCode::KeyJ) {
            cx.view.cam.look.0 -= step;
        }
        if alt && cx.input.keys.contains(&KeyCode::KeyL) {
            cx.view.cam.look.0 += step;
        }
        if alt && cx.input.keys.contains(&KeyCode::KeyI) {
            cx.view.cam.look.1 = (cx.view.cam.look.1 + step * 0.7).min(85.0);
        }
        if alt && cx.input.keys.contains(&KeyCode::KeyK) {
            cx.view.cam.look.1 = (cx.view.cam.look.1 - step * 0.7).max(-85.0);
        }
        if *cx.view.view != "outside" {
            cx.view.cam.look.0 = crate::input_script::cab_look_yaw(cx.view.cam.look.0);
        }
        // Ctrl+Shift+Page Up / Page Down held: the clock runs forwards / backwards,
        // a quarter of an hour per second at first, faster the longer it is held
        // (the menu's whole hours were the only way)
        {
            let ctrl = cx.input.keys.contains(&KeyCode::ControlLeft) || cx.input.keys.contains(&KeyCode::ControlRight);
            let shift = cx.input.keys.contains(&KeyCode::ShiftLeft) || cx.input.keys.contains(&KeyCode::ShiftRight);
            let dir = match (cx.input.keys.contains(&KeyCode::PageUp), cx.input.keys.contains(&KeyCode::PageDown)) {
                (true, false) => 1.0,
                (false, true) => -1.0,
                _ => 0.0,
            };
            let client = cx.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client);
            if ctrl && shift && dir != 0.0 && !client {
                if cx.input.clock_hold == 0.0 && crate::app_impl::real_time_locked(cx.common.settings, cx.net) {
                    // (says once why the clock stays)
                    crate::app_impl::shift_clock(crate::app::narrow!(cx => ShiftClockCx { reads: [net, input, world], writes: [clock, traffic, player, ui, gfx] }), dir as f64);
                }
                cx.input.clock_hold += dt;
                if !crate::app_impl::real_time_locked(cx.common.settings, cx.net) {
                    let rate = 900.0 * (1.0 + cx.input.clock_hold * 1.5).min(8.0);
                    crate::app_impl::shift_clock(crate::app::narrow!(cx => ShiftClockCx { reads: [net, input, world], writes: [clock, traffic, player, ui, gfx] }), dir * rate as f64 * dt as f64);
                }
            } else {
                if cx.input.clock_hold > 0.0 {
                    cx.input.clock_hold = 0.0;
                    crate::app_impl::timetable_after_clock_jump(crate::app::narrow!(cx => JumpCx { reads: [world], writes: [clock, traffic, gfx] }));
                }
                cx.input.clock_hold = 0.0;
            }
        }
        // zooming with the keys: the [game] keys `view_zoom_in` / `view_zoom_out` where the
        // player gave them keys; otherwise = and - (unless the player bound them to something
        // of their own, #701) and, outside, the numpad's + and - while no key binding uses
        // them (OMSI's own file has the kickdown and a door on them: the outside camera
        // zoomed whenever they were pressed, #2469, #2322)
        let pressed: Vec<i32> = cx.input.keys.iter().filter_map(|k| crate::keys::dik_code(*k)).collect();
        let chord = omsi_content::input::chord(
            cx.input.keys.contains(&KeyCode::ShiftLeft) || cx.input.keys.contains(&KeyCode::ShiftRight),
            cx.input.keys.contains(&KeyCode::ControlLeft) || cx.input.keys.contains(&KeyCode::ControlRight),
            cx.input.keys.contains(&KeyCode::AltLeft) || cx.input.keys.contains(&KeyCode::AltRight),
        );
        let bus_keys = cx.player.player.as_ref().map(|p| p.bindings.as_slice()).unwrap_or_default();
        let (zoom_in, pad_in) = zoom_keys("view_zoom_in", &pressed, chord, &cx.input.game_keys, bus_keys, &cx.input.own_keys, 13, 78);
        let (zoom_out, pad_out) = zoom_keys("view_zoom_out", &pressed, chord, &cx.input.game_keys, bus_keys, &cx.input.own_keys, 12, 74);
        if matches!(cx.view.view.as_str(), "driver" | "pax") {
            if zoom_in {
                crate::app_impl::zoom_view(cx.view.cam, cx.view.view, 3.0 * dt);
            }
            if zoom_out {
                crate::app_impl::zoom_view(cx.view.cam, cx.view.view, -3.0 * dt);
            }
        }
        // W/S and the wheel pull the outside camera in and out
        if *cx.view.view == "outside" {
            if zoom_in || pad_in {
                cx.view.cam.orbit = (cx.view.cam.orbit - 12.0 * dt).max(ORBIT_MIN);
            }
            if zoom_out || pad_out {
                cx.view.cam.orbit = (cx.view.cam.orbit + 12.0 * dt).min(ORBIT_MAX);
            }
        }
        // Home held recentres the view - unless keyboard.cfg gives it a job (the
        // stock file makes it the ticket desk camera, which this then turned
        // straight ahead again whenever it was switched to, #733)
        if cx.input.keys.contains(&KeyCode::Home) && !cx.input.game_keys.iter().any(|b| Some(b.scan_code) == crate::keys::dik_code(KeyCode::Home)) {
            cx.view.cam.look = (0.0, 0.0);
            cx.view.cam.orbit = ORBIT_DEFAULT;
            cx.view.cam.view_zoom.remove(cx.view.view.as_str());
            cx.view.cam.f1_reset = None;
        }
    }
    if *cx.view.view != "free" {
        cx.view.cam.ego = false;
    }
    free_camera(&mut cx, dt);
}

fn aim_flashlight(cx: &mut ViewKeysCx, size: Option<(u32, u32)>) {
    let live = if cx.input.touch.enabled { cx.input.touch.pointing() } else { !cx.input.cursor_stale };
    let free = live && cx.input.look_lock.is_none() && cx.input.mouse_grab.mode.is_none() && !cx.input.mouse_look && cx.input.cursor_hidden.is_none();
    let wanted = cx.common.settings.flashlight_cursor && cx.view.cam.flashlight && free && !cx.common.settings.vr_requested();
    cx.view.cam.flashlight_aim = match (wanted, cx.view.camera.as_ref(), size) {
        (true, Some(cam), Some(size)) => {
            let rig = crate::app_impl::triple_rig(cx.common.settings, cx.view.cam, cx.view.view, size);
            Some(cursor_ray(cam, rig.as_ref(), cx.input.cursor, size))
        }
        _ => None,
    };
}

fn zoom_hold_keys(cx: &mut ViewKeysCx, dt: f32) {
    let pressed: Vec<i32> = cx.input.keys.iter().filter_map(|k| crate::keys::dik_code(*k)).collect();
    let chord = omsi_content::input::chord(
        cx.input.keys.contains(&KeyCode::ShiftLeft) || cx.input.keys.contains(&KeyCode::ShiftRight),
        cx.input.keys.contains(&KeyCode::ControlLeft) || cx.input.keys.contains(&KeyCode::ControlRight),
        cx.input.keys.contains(&KeyCode::AltLeft) || cx.input.keys.contains(&KeyCode::AltRight),
    );
    let bus_keys = cx.player.player.as_ref().map(|p| p.bindings.as_slice()).unwrap_or_default();
    let held = cx.input.pad_zoom_hold || zoom_keys("view_zoom_hold", &pressed, chord, &cx.input.game_keys, bus_keys, &cx.input.own_keys, 0, 0).0;
    cx.view.cam.zoom_hold.step(held, cx.common.settings.zoom_hold_amount, cx.common.settings.zoom_hold_latch, dt);
}

/// Whether the keys held (`held`: their scan codes, `chord`: the modifiers) zoom: `action`'s
/// [game] keys where keyboard.cfg gives it any, else the key `plain` (= or -) unless it is the
/// player's own; and, second, the numpad key `pad` (outside only) where nothing is bound to it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn zoom_keys(
    action: &str,
    held: &[i32],
    chord: i32,
    game: &[omsi_content::KeyBinding],
    bus: &[omsi_content::KeyBinding],
    own: &std::collections::HashSet<i32>,
    plain: i32,
    pad: i32,
) -> (bool, bool) {
    let mut own_keys = game.iter().filter(|b| b.scan_code != 0 && b.action.eq_ignore_ascii_case(action)).peekable();
    if own_keys.peek().is_some() {
        return (own_keys.any(|b| held.contains(&b.scan_code) && b.matches(chord)), false);
    }
    let bound = |k: i32| own.contains(&k) || game.iter().chain(bus).any(|b| b.scan_code == k);
    (held.contains(&plain) && !own.contains(&plain), held.contains(&pad) && !bound(pad))
}

/// The mirror editor's keys and Ctrl+Alt+arrows: the mirrors aimed, shifted and zoomed
/// (and kept per bus when the keys are let go). Whether Ctrl+Alt are held.
fn mirror_keys(cx: &mut ViewKeysCx, dt: f32) -> bool {
    // Ctrl+Alt+arrows in the cab: the mirror nearest to where the driver looks
    // turns (kept per bus in mirrors.cfg when the keys are let go)
    // the mirror editor: an arrow held over a panel aims that panel's mirror
    // (kept per bus like Ctrl+Alt+arrows below)
    if let (Some(size), Some(a)) =
        (crate::app_events::mirror_hud_size(cx.common.settings, cx.view.cam, cx.menus.menus, cx.gfx.gfx), cx.gfx.gfx.mirror_hud.turning())
    {
        if let (Some(i), Some(p)) = (
            cx.gfx.gfx.mirror_hud.cam_under(crate::app_impl::hud_cursor(cx.common.settings, cx.gfx.gfx, cx.input), size),
            cx.player.player.as_mut(),
        ) {
            let n = p.vehicle.ty.def.cameras_reflexion.len();
            if p.mirror_offsets.len() < n {
                p.mirror_offsets.resize(n, [0.0; 2]);
            }
            if p.mirror_shifts.len() < n {
                p.mirror_shifts.resize(n, [0.0; 3]);
            }
            // Alt+arrows and Page Up/Down shift the mirror (0.2 m a second, at most
            // 0.6 m across and up, a metre along), the plain arrows aim it
            if p.mirror_fovs.len() < n {
                p.mirror_fovs.resize(n, 0.0);
            }
            let alt = cx.input.keys.contains(&KeyCode::AltLeft) || cx.input.keys.contains(&KeyCode::AltRight);
            let along = (a[4] as i32 - a[5] as i32) as f32;
            let zoom = (a[7] as i32 - a[6] as i32) as f32;
            if zoom != 0.0 {
                // - and + narrow and widen the mirror's field of view (20° a second)
                if let Some(f) = p.mirror_fovs.get_mut(i) {
                    *f = (*f + 20.0 * dt * zoom).clamp(-60.0, 60.0);
                    p.mirrors_dirty = true;
                    let base = p.vehicle.ty.def.cameras_reflexion.get(i).map(|c| if c.fov > 1.0 { c.fov } else { 50.0 }).unwrap_or(50.0);
                    *cx.ui.service_msg = Some((format!("Mirror {}: field of view {:.0}° (the bus's {:.0}°)", i + 1, (base + *f).clamp(8.0, 110.0), base), 2.0));
                }
            } else if alt || along != 0.0 {
                let metres = 0.2 * dt;
                if let Some(s) = p.mirror_shifts.get_mut(i) {
                    if alt {
                        s[0] = (s[0] + metres * (a[1] as i32 - a[0] as i32) as f32).clamp(-0.6, 0.6);
                        s[2] = (s[2] + metres * (a[2] as i32 - a[3] as i32) as f32).clamp(-0.6, 0.6);
                    }
                    s[1] = (s[1] + metres * along).clamp(-1.0, 1.0);
                    p.mirrors_dirty = true;
                    *cx.ui.service_msg = Some((format!("Mirror {} shifted {:+.2} m across, {:+.2} m forward, {:+.2} m up", i + 1, s[0], s[1], s[2]), 2.0));
                }
            } else if let Some(o) = p.mirror_offsets.get_mut(i) {
                let rate = 12.0 * dt;
                o[0] = (o[0] + rate * (a[1] as i32 - a[0] as i32) as f32).clamp(-45.0, 45.0);
                o[1] = (o[1] + rate * (a[2] as i32 - a[3] as i32) as f32).clamp(-30.0, 30.0);
                p.mirrors_dirty = true;
                *cx.ui.service_msg = Some((format!("Mirror {}: {:+.1}° across, {:+.1}° up", i + 1, o[0], o[1]), 2.0));
            }
        }
    }
    let ctrl_alt = (cx.input.keys.contains(&KeyCode::ControlLeft) || cx.input.keys.contains(&KeyCode::ControlRight)) && (cx.input.keys.contains(&KeyCode::AltLeft) || cx.input.keys.contains(&KeyCode::AltRight));
    let arrows = [KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown].map(|k| cx.input.keys.contains(&k));
    if let (true, Some(p), Some(cam)) = (ctrl_alt && *cx.view.view == "driver" && arrows.iter().any(|a| *a), cx.player.player.as_mut(), cx.view.camera.as_ref()) {
        let cams = &p.vehicle.ty.def.cameras_reflexion;
        let f = cam.forward();
        let best = (0..cams.len())
            .map(|i| (i, (p.vehicle.camera_world_full(&cams[i]).0 - cam.position).as_vec3().normalize_or_zero().dot(f)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        if let Some(i) = best {
            if p.mirror_offsets.len() <= i {
                p.mirror_offsets.resize(cams.len(), [0.0; 2]);
            }
            let o = &mut p.mirror_offsets[i];
            let rate = 12.0 * dt;
            o[0] = (o[0] + rate * (arrows[1] as i32 - arrows[0] as i32) as f32).clamp(-45.0, 45.0);
            o[1] = (o[1] + rate * (arrows[2] as i32 - arrows[3] as i32) as f32).clamp(-30.0, 30.0);
            p.mirrors_dirty = true;
            *cx.ui.service_msg = Some((format!("Mirror {}: {:+.1}° across, {:+.1}° up (Ctrl+Alt+arrows)", i + 1, o[0], o[1]), 2.0));
        }
    } else if let Some(p) = cx.player.player.as_mut().filter(|p| p.mirrors_dirty) {
        p.mirrors_dirty = false;
        crate::settings::save_mirror_state(&p.vehicle.ty.def.path, &p.mirror_offsets, &p.mirror_shifts, &p.mirror_fovs);
    }
    ctrl_alt
}

/// The free camera's flight (and the walk with `ego`).
fn free_camera(cx: &mut ViewKeysCx, dt: f32) {
    // (the free camera flies; with no bus it is the view too - but not out of the
    // walker's eyes: on foot without a bus of one's own (started on foot, the bus
    // removed) the keys flew the camera on from where the walk had put it every
    // frame, and walking jumped about, the more so the lower the frame rate, #807)
    if let (Some(cam), true) = (
        cx.view.camera.as_mut(),
        *cx.view.view == "free" || (cx.player.player.is_none() && cx.people.on_foot.is_none()),
    ) {
        // the free camera keeps the horizon level: taken over from the driver's view
        // of a bus leaning on a slope, it went on with the bus's roll (#2220)
        if cx.ui.photo.is_none() {
            cam.roll = 0.0;
        }
        // (the keys handed to the bus, #1779: the camera keeps still but for the mouse)
        let keys_on = !cx.input.camera_keys_off || cx.player.player.is_none() || cx.view.cam.ego || cx.input.touch.enabled;
        let keys = &cx.input.keys;
        let held = |k: KeyCode| keys_on && keys.contains(&k);
        let mut v = Vec3::ZERO;
        let f = cam.forward();
        let r = cam.right();
        if held(KeyCode::KeyW) {
            v += f;
        }
        if held(KeyCode::KeyS) {
            v -= f;
        }
        if held(KeyCode::KeyD) {
            v += r;
        }
        if held(KeyCode::KeyA) {
            v -= r;
        }
        if held(KeyCode::KeyE) || held(KeyCode::Space) {
            v += Vec3::Z;
        }
        if held(KeyCode::KeyQ) {
            v -= Vec3::Z;
        }
        let boost = if held(KeyCode::ShiftLeft) {
            5.0
        } else {
            1.0
        };
        if cx.view.cam.ego {
            // walking: along the ground at eye height, 1.4 m/s (running 4.5)
            let flat = Vec3::new(v.x, v.y, 0.0).normalize_or_zero();
            let pace = if boost > 1.0 { 4.5 } else { 1.4 };
            cam.position += (flat * pace * dt).as_dvec3();
            if let Some(g) = cx.world.world.as_ref().and_then(|w| w.walk_height(cam.position.x, cam.position.y)) {
                cam.position.z = g + 1.7;
            }
        } else {
            // (a finger on the screen's stick: as far as it is pushed, the harder at the edge,
            // so that a touch can fly slowly too)
            let pushed = cx.input.touch.stick_analog();
            let by_finger = (f * -pushed.y + r * pushed.x).normalize_or_zero() * pushed.length().min(1.0).powi(2);
            let speed = cx.view.cam.speed;
            cam.position += ((v.normalize_or_zero() * boost + by_finger) * speed * dt).as_dvec3();
        }
        if held(KeyCode::ArrowLeft) {
            cam.yaw -= 60.0 * dt;
        }
        if held(KeyCode::ArrowRight) {
            cam.yaw += 60.0 * dt;
        }
        if held(KeyCode::ArrowUp) {
            cam.pitch = (cam.pitch + 40.0 * dt).min(89.0);
        }
        if held(KeyCode::ArrowDown) {
            cam.pitch = (cam.pitch - 40.0 * dt).max(-89.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::zoom_keys;
    use omsi_content::KeyBinding;

    fn kb(a: &str, k: i32, m: i32) -> KeyBinding {
        KeyBinding { action: a.into(), scan_code: k, modifier: m }
    }

    /// Num+ and Num- zoomed the outside camera on top of the kickdown and the rear door
    /// OMSI's file puts on them (#2469, #2322); `view_zoom_in/out` move the zoom.
    #[test]
    fn the_numpad_zooms_only_where_nothing_is_bound_to_it() {
        let none = std::collections::HashSet::new();
        // nothing bound: the numpad zooms outside, = everywhere
        assert_eq!(zoom_keys("view_zoom_in", &[78], 0, &[], &[], &none, 13, 78), (false, true));
        assert_eq!(zoom_keys("view_zoom_in", &[13], 0, &[], &[], &none, 13, 78), (true, false));
        // the rear door on Num- (stock [vehicles], the bus's keys): no zoom
        let bus = [kb("bus_dooraft", 74, 0)];
        assert_eq!(zoom_keys("view_zoom_out", &[74], 0, &[], &bus, &none, 12, 74), (false, false));
        // = the player's own: no zoom on it
        let own: std::collections::HashSet<i32> = [13].into();
        assert_eq!(zoom_keys("view_zoom_in", &[13], 0, &[], &[], &own, 13, 78), (false, false));
        // the zoom moved to Ctrl+PageUp: only that zooms, = and the numpad do not
        let game = [kb("view_zoom_in", 201, 4)];
        assert_eq!(zoom_keys("view_zoom_in", &[201], 4, &game, &[], &none, 13, 78), (true, false));
        assert_eq!(zoom_keys("view_zoom_in", &[201], 0, &game, &[], &none, 13, 78), (false, false));
        assert_eq!(zoom_keys("view_zoom_in", &[13, 78], 0, &game, &[], &none, 13, 78), (false, false));
        // an entry with no key (cleared) leaves the defaults
        let cleared = [kb("view_zoom_in", 0, 0)];
        assert_eq!(zoom_keys("view_zoom_in", &[13], 0, &cleared, &[], &none, 13, 78), (true, false));
    }

    fn app() -> crate::App {
        use clap::Parser;
        let mut app = crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default());
        app.settings.zoom_hold_amount = 0.5;
        app
    }

    fn frames(app: &mut crate::App, n: usize) {
        for _ in 0..n {
            super::view_keys(super::ViewKeysCx::of(&mut app.parts()), 0.05);
        }
    }

    #[test]
    fn the_zoom_key_works_in_the_free_view_without_a_bus() {
        let mut app = app();
        app.view = "free".into();
        app.input.game_keys = vec![kb("view_zoom_hold", 44, 0)];
        app.input.keys.insert(winit::keyboard::KeyCode::KeyZ);
        frames(&mut app, 40);
        assert_eq!(app.cam.zoom_hold.factor, 0.5);
        app.input.keys.clear();
        frames(&mut app, 40);
        assert_eq!(app.cam.zoom_hold.factor, 1.0);
    }

    #[test]
    fn letting_go_after_a_switch_to_the_free_view_ends_the_zoom() {
        let mut app = app();
        app.view = "driver".into();
        app.input.pad_zoom_hold = true;
        frames(&mut app, 40);
        assert_eq!(app.cam.zoom_hold.factor, 0.5);
        app.view = "free".into();
        app.input.pad_zoom_hold = false;
        frames(&mut app, 40);
        assert_eq!(app.cam.zoom_hold.factor, 1.0);
    }

    #[test]
    fn a_latched_zoom_turns_off_with_a_press_in_the_free_view() {
        let mut app = app();
        app.settings.zoom_hold_latch = true;
        app.view = "driver".into();
        app.input.pad_zoom_hold = true;
        frames(&mut app, 1);
        app.input.pad_zoom_hold = false;
        frames(&mut app, 40);
        assert_eq!(app.cam.zoom_hold.factor, 0.5);
        app.view = "free".into();
        app.input.pad_zoom_hold = true;
        frames(&mut app, 1);
        app.input.pad_zoom_hold = false;
        frames(&mut app, 40);
        assert_eq!(app.cam.zoom_hold.factor, 1.0);
    }

    fn aim(app: &mut crate::App) -> Option<glam::Vec3> {
        super::aim_flashlight(&mut super::ViewKeysCx::of(&mut app.parts()), Some((1920, 1080)));
        app.cam.flashlight_aim
    }

    #[test]
    fn the_flashlight_follows_only_a_shown_cursor() {
        let mut app = app();
        app.camera = Some(omsi_render::Camera { position: glam::DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 });
        app.settings.flashlight_cursor = true;
        app.cam.flashlight = true;
        app.input.cursor = (100.0, 80.0);
        let shown = aim(&mut app).expect("the beam follows the shown cursor");
        let ahead = app.camera.as_ref().map(|c| c.forward()).unwrap_or_default();
        assert!((shown - ahead).length() > 0.1);
        app.input.cursor_hidden = Some(app.input.cursor);
        assert_eq!(aim(&mut app), None);
        app.input.cursor_hidden = None;
        app.input.cursor_stale = true;
        assert_eq!(aim(&mut app), None, "a controller was used since the mouse last moved");
        app.input.cursor_stale = false;
        app.input.touch.enabled = true;
        assert_eq!(aim(&mut app), None, "no finger on the screen");
        app.input.touch.enabled = false;
        app.settings.flashlight_cursor = false;
        assert_eq!(aim(&mut app), None);
    }
}

fn cursor_ray(cam: &omsi_render::Camera, rig: Option<&omsi_render::TripleScreen>, cursor: (f32, f32), size: (u32, u32)) -> glam::Vec3 {
    match rig {
        Some(rig) => rig.cursor_ray(cam, cursor, size).1,
        None => omsi_scene::camera_util::cursor_ray(cam, cursor.0, cursor.1, size.0 as f32, size.1 as f32).1,
    }
}

#[cfg(test)]
mod cursor_ray_tests {
    use super::cursor_ray;

    fn cam(fov_deg: f32) -> omsi_render::Camera {
        omsi_render::Camera { position: glam::DVec3::ZERO, yaw: 0.3, pitch: 0.1, roll: 0.0, fov_deg, near: 0.1, far: 100.0 }
    }

    fn on_screen(cam: &omsi_render::Camera, d: glam::Vec3, size: (u32, u32)) -> (f32, f32) {
        let ndc = cam.view_proj(size.0 as f32 / size.1 as f32, cam.position).project_point3(d * 10.0);
        ((ndc.x + 1.0) * 0.5 * size.0 as f32, (1.0 - ndc.y) * 0.5 * size.1 as f32)
    }

    #[test]
    fn the_beam_goes_through_the_cursor_at_any_zoom() {
        for fov in [30.0, 60.0, 95.0] {
            let c = cam(fov);
            for p in [(960.0, 540.0), (100.0, 80.0), (1800.0, 1000.0)] {
                let at = on_screen(&c, cursor_ray(&c, None, p, (1920, 1080)), (1920, 1080));
                assert!((at.0 - p.0).abs() < 1.0 && (at.1 - p.1).abs() < 1.0, "fov {fov}: {p:?} -> {at:?}");
            }
        }
    }

    #[test]
    fn on_a_triple_screen_the_beam_follows_the_panel_under_the_cursor() {
        let size = (5760, 1080);
        let rig = omsi_render::TripleScreen { enabled: true, ..Default::default() }.zoomed(size.0, size.1, 1.0);
        let c = cam(60.0);
        let views = rig.views(&c, size.0, size.1);
        for v in &views {
            let [x, y, w, h] = v.viewport;
            for f in [(0.5, 0.5), (0.1, 0.2), (0.9, 0.8)] {
                let p = (x as f32 + f.0 * w as f32, y as f32 + f.1 * h as f32);
                let d = cursor_ray(&c, Some(&rig), p, size);
                let vp = v.projection * glam::Mat4::look_to_rh(glam::Vec3::ZERO, v.camera.forward(), v.camera.up());
                let ndc = vp.project_point3(d);
                let at = ((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5);
                assert!((at.0 - f.0).abs() < 1e-3 && (at.1 - f.1).abs() < 1e-3, "panel at {x}: {f:?} -> {at:?}");
            }
        }
        let flat = cursor_ray(&c, None, (0.1 * 5760.0, 540.0), size);
        assert!((cursor_ray(&c, Some(&rig), (0.1 * 5760.0, 540.0), size) - flat).length() > 0.05, "a side panel is not the flat picture");
    }
}
