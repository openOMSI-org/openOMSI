//! DirectInput scan codes (as used by `Inputs/keyboard.cfg`) ↔ winit key codes.

use winit::keyboard::KeyCode;

/// Shift, Ctrl, Alt and the system key, either side: the keys `ModifiersChanged` keeps.
pub fn is_modifier(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
    )
}

/// DIK_* scan code for a winit key, if known.
pub fn dik_code(key: KeyCode) -> Option<i32> {
    use KeyCode::*;
    Some(match key {
        Escape => 1,
        Digit1 => 2,
        Digit2 => 3,
        Digit3 => 4,
        Digit4 => 5,
        Digit5 => 6,
        Digit6 => 7,
        Digit7 => 8,
        Digit8 => 9,
        Digit9 => 10,
        Digit0 => 11,
        Minus => 12,
        Equal => 13,
        Backspace => 14,
        Tab => 15,
        KeyQ => 16,
        KeyW => 17,
        KeyE => 18,
        KeyR => 19,
        KeyT => 20,
        KeyY => 21,
        KeyU => 22,
        KeyI => 23,
        KeyO => 24,
        KeyP => 25,
        BracketLeft => 26,
        BracketRight => 27,
        Enter => 28,
        ControlLeft => 29,
        KeyA => 30,
        KeyS => 31,
        KeyD => 32,
        KeyF => 33,
        KeyG => 34,
        KeyH => 35,
        KeyJ => 36,
        KeyK => 37,
        KeyL => 38,
        Semicolon => 39,
        Quote => 40,
        Backquote => 41,
        ShiftLeft => 42,
        Backslash => 43,
        KeyZ => 44,
        KeyX => 45,
        KeyC => 46,
        KeyV => 47,
        KeyB => 48,
        KeyN => 49,
        KeyM => 50,
        Comma => 51,
        Period => 52,
        Slash => 53,
        ShiftRight => 54,
        NumpadMultiply => 55,
        AltLeft => 56,
        Space => 57,
        CapsLock => 58,
        F1 => 59,
        F2 => 60,
        F3 => 61,
        F4 => 62,
        F5 => 63,
        F6 => 64,
        F7 => 65,
        F8 => 66,
        F9 => 67,
        F10 => 68,
        NumLock => 69,
        ScrollLock => 70,
        Numpad7 => 71,
        Numpad8 => 72,
        Numpad9 => 73,
        NumpadSubtract => 74,
        Numpad4 => 75,
        Numpad5 => 76,
        Numpad6 => 77,
        NumpadAdd => 78,
        Numpad1 => 79,
        Numpad2 => 80,
        Numpad3 => 81,
        Numpad0 => 82,
        NumpadDecimal => 83,
        IntlBackslash => 86,
        F11 => 87,
        F12 => 88,
        F13 => 100,
        F14 => 101,
        F15 => 102,
        NumpadEqual => 141,
        NumpadEnter => 156,
        ControlRight => 157,
        NumpadDivide => 181,
        PrintScreen => 183,
        AltRight => 184,
        Pause => 197,
        Home => 199,
        ArrowUp => 200,
        PageUp => 201,
        ArrowLeft => 203,
        ArrowRight => 205,
        End => 207,
        ArrowDown => 208,
        PageDown => 209,
        Insert => 210,
        Delete => 211,
        SuperLeft => 219,
        SuperRight => 220,
        ContextMenu => 221,
        _ => return None,
    })
}

/// OMSI's own codes for a modifier key of either side (`Inputs/ENG.kyb`: 90 "Shift (both)",
/// 91 "Ctrl (both)", 92 "Alt (both)"; the stock keyboard.cfg puts `open_menue`, the quick
/// menu, on 92): the one a Shift, Ctrl or Alt key also is.
pub fn both_code(key: KeyCode) -> Option<i32> {
    use KeyCode::*;
    match key {
        ShiftLeft | ShiftRight => Some(SHIFT_BOTH),
        ControlLeft | ControlRight => Some(CTRL_BOTH),
        AltLeft | AltRight => Some(ALT_BOTH),
        _ => None,
    }
}

/// The other key Windows takes for the same virtual key: with Num Lock off (or Shift held
/// with it on) a numpad key is its grey twin (Num4 is VK_LEFT, the Left arrow's key), and
/// both Enters are VK_RETURN. Windows keeps one up/down state per virtual key.
pub fn vk_twin(key: KeyCode) -> Option<KeyCode> {
    use KeyCode::*;
    const PAIRS: &[(KeyCode, KeyCode)] = &[
        (Numpad0, Insert),
        (NumpadDecimal, Delete),
        (Numpad1, End),
        (Numpad2, ArrowDown),
        (Numpad3, PageDown),
        (Numpad4, ArrowLeft),
        (Numpad6, ArrowRight),
        (Numpad7, Home),
        (Numpad8, ArrowUp),
        (Numpad9, PageUp),
        (NumpadEnter, Enter),
    ];
    PAIRS.iter().find_map(|&(a, b)| {
        if a == key {
            Some(b)
        } else if b == key {
            Some(a)
        } else {
            None
        }
    })
}

/// A press of `key` that came flagged as a repeat while `key` itself is not held (`held`)
/// but its twin (see [`vk_twin`]) is: its first press. Windows flags it so because the
/// virtual key the two share is down already; taken for a repeat, the Left arrow pressed
/// with Num4 held did nothing (#1830).
pub fn twin_first_press(key: KeyCode, held: impl Fn(KeyCode) -> bool) -> bool {
    !held(key) && vk_twin(key).is_some_and(held)
}

/// Whether a key event of the window (`repeat` as the system flags it) is a repeat. A release
/// never is: Windows flags the second release of two keys sharing one virtual key as one
/// (its key was up already), and a release taken for a repeat went nowhere - with Num4 held
/// and Left tapped, Num4's release was lost and the wheel stayed at full lock until Num4 was
/// pressed again (#1830). On Windows (`windows`) a twin's first press is no repeat either.
pub fn key_repeat(
    key: KeyCode,
    pressed: bool,
    repeat: bool,
    windows: bool,
    held: impl Fn(KeyCode) -> bool,
) -> bool {
    pressed && repeat && !(windows && twin_first_press(key, held))
}

#[cfg(test)]
mod vk_twin_tests {
    use super::*;
    use winit::keyboard::KeyCode::*;

    /// The modifier key-ups the window drops once `ModifiersChanged` has let them go (Windows
    /// reports the state first): the eight modifier keys, nothing else.
    #[test]
    fn only_the_modifier_keys_count_as_modifiers() {
        for code in [
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ] {
            assert!(is_modifier(code), "{code:?}");
        }
        for code in [
            KeyCode::KeyW,
            KeyCode::CapsLock,
            KeyCode::Escape,
            KeyCode::Digit1,
        ] {
            assert!(!is_modifier(code), "{code:?}");
        }
    }

    #[test]
    fn twins() {
        assert_eq!(vk_twin(Numpad4), Some(ArrowLeft));
        assert_eq!(vk_twin(ArrowLeft), Some(Numpad4));
        assert_eq!(vk_twin(Enter), Some(NumpadEnter));
        assert_eq!(vk_twin(Numpad5), None);
        assert_eq!(vk_twin(KeyA), None);
    }

    /// Num4 held, Left pressed (flagged as a repeat): Left's first press (#1830).
    #[test]
    fn numpad_and_arrow_held_together() {
        assert!(twin_first_press(ArrowLeft, |k| k == Numpad4));
        assert!(twin_first_press(Numpad4, |k| k == ArrowLeft));
        // a real repeat of a key held, or a repeat with no twin held, stays a repeat
        assert!(!twin_first_press(ArrowLeft, |k| k == ArrowLeft || k == Numpad4));
        assert!(!twin_first_press(ArrowRight, |k| k == Numpad4));
        assert!(!twin_first_press(KeyA, |_| false));
    }

    /// Num4 held, Left tapped, Left and then Num4 let go, as Windows flags the events: the
    /// Left press is a press, and both releases are releases (#1830).
    #[test]
    fn a_release_is_never_a_repeat() {
        assert!(!key_repeat(Numpad4, true, false, true, |_| false));
        assert!(!key_repeat(ArrowLeft, true, true, true, |k| k == Numpad4));
        assert!(key_repeat(ArrowLeft, true, true, true, |k| k == Numpad4
            || k == ArrowLeft));
        assert!(!key_repeat(ArrowLeft, false, false, true, |k| k == Numpad4
            || k == ArrowLeft));
        assert!(!key_repeat(Numpad4, false, true, true, |k| k == Numpad4));
        // elsewhere keys are told apart by themselves: a repeat stays one
        assert!(key_repeat(ArrowLeft, true, true, false, |k| k == Numpad4));
        assert!(key_repeat(KeyW, true, true, true, |k| k == KeyW));
    }
}

/// The mouse's side buttons, Mouse 4 (back) and Mouse 5 (forward), as keyboard.cfg keys: codes
/// past DirectInput's 0..255, which OMSI never writes (#2281).
pub const MOUSE_4: i32 = 1004;
pub const MOUSE_5: i32 = 1005;

/// "Shift (both)", "Ctrl (both)", "Alt (both)": see [`both_code`].
pub const SHIFT_BOTH: i32 = 90;
pub const CTRL_BOTH: i32 = 91;
pub const ALT_BOTH: i32 = 92;

/// The chord bit (`KEY_SHIFT`, `KEY_CTRL`, `KEY_ALT`) of a modifier key, 0 for any other key.
pub fn own_modifier_bit(key: KeyCode) -> i32 {
    use omsi_content::input::{KEY_ALT, KEY_CTRL, KEY_SHIFT};
    match both_code(key) {
        Some(SHIFT_BOTH) => KEY_SHIFT,
        Some(CTRL_BOTH) => KEY_CTRL,
        Some(ALT_BOTH) => KEY_ALT,
        _ => 0,
    }
}

/// A readable name for a DIK scan code ("W", "Num8", "Up", "F5"), from the same table.
pub fn scan_name(scan: i32) -> Option<String> {
    use KeyCode::*;
    match scan {
        SHIFT_BOTH => return Some("Shift".into()),
        CTRL_BOTH => return Some("Ctrl".into()),
        ALT_BOTH => return Some("Alt".into()),
        MOUSE_4 => return Some("Mouse 4".into()),
        MOUSE_5 => return Some("Mouse 5".into()),
        _ => {}
    }
    const ALL: &[KeyCode] = &[
        Escape,
        Digit1,
        Digit2,
        Digit3,
        Digit4,
        Digit5,
        Digit6,
        Digit7,
        Digit8,
        Digit9,
        Digit0,
        Minus,
        Equal,
        Backspace,
        Tab,
        KeyQ,
        KeyW,
        KeyE,
        KeyR,
        KeyT,
        KeyY,
        KeyU,
        KeyI,
        KeyO,
        KeyP,
        BracketLeft,
        BracketRight,
        Enter,
        ControlLeft,
        KeyA,
        KeyS,
        KeyD,
        KeyF,
        KeyG,
        KeyH,
        KeyJ,
        KeyK,
        KeyL,
        Semicolon,
        Quote,
        Backquote,
        ShiftLeft,
        Backslash,
        KeyZ,
        KeyX,
        KeyC,
        KeyV,
        KeyB,
        KeyN,
        KeyM,
        Comma,
        Period,
        Slash,
        ShiftRight,
        NumpadMultiply,
        AltLeft,
        Space,
        CapsLock,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        NumLock,
        ScrollLock,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadSubtract,
        Numpad4,
        Numpad5,
        Numpad6,
        NumpadAdd,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad0,
        NumpadDecimal,
        IntlBackslash,
        F11,
        F12,
        F13,
        F14,
        F15,
        NumpadEqual,
        NumpadEnter,
        ControlRight,
        NumpadDivide,
        PrintScreen,
        AltRight,
        Pause,
        Home,
        ArrowUp,
        PageUp,
        ArrowLeft,
        ArrowRight,
        End,
        ArrowDown,
        PageDown,
        Insert,
        Delete,
        SuperLeft,
        SuperRight,
        ContextMenu,
    ];
    let k = ALL.iter().find(|k| dik_code(**k) == Some(scan))?;
    let n = format!("{k:?}");
    let n = n
        .strip_prefix("Key")
        .or_else(|| n.strip_prefix("Digit"))
        .map(String::from)
        .unwrap_or(n.clone());
    let n = n.replace("Numpad", "Num").replace("Arrow", "");
    Some(match n.as_str() {
        "Period" => ".".into(),
        "Comma" => ",".into(),
        "Minus" => "-".into(),
        "Equal" => "=".into(),
        "Slash" => "/".into(),
        "Semicolon" => ";".into(),
        _ => n,
    })
}

#[cfg(test)]
mod scan_name_tests {
    #[test]
    fn names() {
        assert_eq!(super::scan_name(17).as_deref(), Some("W"));
        assert_eq!(super::scan_name(72).as_deref(), Some("Num8"));
        assert_eq!(super::scan_name(200).as_deref(), Some("Up"));
        assert_eq!(super::scan_name(52).as_deref(), Some("."));
        assert_eq!(super::scan_name(92).as_deref(), Some("Alt"));
    }

    /// OMSI's "Alt (both)" and the rest: either side's key is it (#2392, #2003).
    #[test]
    fn both_sides_of_a_modifier() {
        use winit::keyboard::KeyCode::*;
        assert_eq!(super::both_code(AltLeft), Some(92));
        assert_eq!(super::both_code(AltRight), Some(92));
        assert_eq!(super::both_code(ShiftRight), Some(90));
        assert_eq!(super::both_code(ControlLeft), Some(91));
        assert_eq!(super::both_code(KeyA), None);
        assert_eq!(
            super::own_modifier_bit(ShiftLeft),
            omsi_content::input::KEY_SHIFT
        );
        assert_eq!(super::own_modifier_bit(KeyA), 0);
    }
}

/// A key and its modifiers as the player reads them ("Ctrl+D"); `(unbound)` for scan 0.
pub fn key_name(scan: i64, modifier: i64) -> String {
    if scan == 0 {
        return "(unbound)".into();
    }
    let k = scan_name(scan as i32).unwrap_or_else(|| format!("scan {scan}"));
    let mut mods = Vec::new();
    if modifier & omsi_content::input::KEY_SHIFT as i64 != 0 {
        mods.push("Shift");
    }
    if modifier & omsi_content::input::KEY_CTRL as i64 != 0 {
        mods.push("Ctrl");
    }
    if modifier & omsi_content::input::KEY_ALT as i64 != 0 {
        mods.push("Alt");
    }
    if mods.is_empty() {
        k
    } else {
        format!("{}+{k}", mods.join("+"))
    }
}
