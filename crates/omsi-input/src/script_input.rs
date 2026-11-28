//! Scripted input playback and key-binding helpers, independent of the application.

use crate::keys;
use winit::keyboard::KeyCode;

/// `OMSI_INPUT="t=3 move 1045,826; t=3.2 press; ..."` → (time, command) pairs.
pub fn parse_input_script() -> Vec<(f32, String)> {
    let Some(v) = omsi_cfg::flags::OMSI_INPUT.var() else {
        return Vec::new();
    };
    let mut out: Vec<(f32, String)> = v
        .split(';')
        .filter_map(|item| {
            let item = item.trim();
            let rest = item.strip_prefix("t=")?;
            let (t, cmd) = rest.split_once(' ')?;
            Some((t.trim().parse::<f32>().ok()?, cmd.trim().to_string()))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// The keys that fly the free camera (and, with no bus of one's own, the view).
pub fn flies_free_camera(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::KeyW
            | KeyCode::KeyA
            | KeyCode::KeyS
            | KeyCode::KeyD
            | KeyCode::KeyQ
            | KeyCode::KeyE
            | KeyCode::Space
            | KeyCode::ShiftLeft
            | KeyCode::ArrowLeft
            | KeyCode::ArrowRight
            | KeyCode::ArrowUp
            | KeyCode::ArrowDown
    )
}

/// Whether the binding `b` is the key `code` (scan code `scan`) pressed with the other
/// modifiers `m`: its own scan code or, for a Shift, Ctrl or Alt key, OMSI's code for either
/// side ("Alt (both)"), and a modifier key's binding written with its own bit counts too
/// (OMSI's files have "Shift (both)" with the Shift flag; #2003, #2392).
pub fn binding_is_key(b: &omsi_content::KeyBinding, code: KeyCode, scan: i32, m: i32) -> bool {
    let ob = keys::own_modifier_bit(code);
    (b.scan_code == scan || keys::both_code(code) == Some(b.scan_code))
        && (b.matches(m) || ob != 0 && b.matches(m | ob))
}

pub fn get_up_key(
    game: &[omsi_content::KeyBinding],
    code: KeyCode,
    shift: bool,
    ctrl: bool,
    alt: bool,
) -> bool {
    let modifier = omsi_content::input::chord(shift, ctrl, alt) & !keys::own_modifier_bit(code);
    keys::dik_code(code).is_some_and(|scan| {
        game.iter().any(|b| {
            b.action.eq_ignore_ascii_case("get_up") && binding_is_key(b, code, scan, modifier)
        })
    })
}

/// `App::key_left_free` for a key's scan code.
pub fn key_left_free(
    scan: Option<i32>,
    action: &str,
    own: &std::collections::HashSet<i32>,
    game: &[omsi_content::KeyBinding],
) -> bool {
    !scan.is_some_and(|s| own.contains(&s))
        && !game
            .iter()
            .any(|b| b.scan_code != 0 && b.action.eq_ignore_ascii_case(action))
}

/// OS shortcuts can consume modifier key-ups without taking window focus. Winit then
/// reports the released modifiers through `ModifiersChanged` (on macOS, also before
/// the next ordinary key event). Remove those stale keys and return them so their
/// normal release handlers run, including vehicle keys and Shift+number door buttons.
pub fn release_inactive_modifiers(
    mut remove: impl FnMut(KeyCode) -> bool,
    modifiers: winit::keyboard::ModifiersState,
) -> Vec<KeyCode> {
    use winit::keyboard::ModifiersState as M;
    let mut released = Vec::new();
    for (flag, left, right) in [
        (M::SHIFT, KeyCode::ShiftLeft, KeyCode::ShiftRight),
        (M::CONTROL, KeyCode::ControlLeft, KeyCode::ControlRight),
        (M::ALT, KeyCode::AltLeft, KeyCode::AltRight),
        (M::SUPER, KeyCode::SuperLeft, KeyCode::SuperRight),
    ] {
        if !modifiers.contains(flag) {
            for code in [left, right] {
                if remove(code) {
                    released.push(code);
                }
            }
        }
    }
    released
}
