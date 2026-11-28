use super::*;
use omsi_content::{
    KeyBinding,
    input::{KEY_ALT, KEY_CTRL, KEY_SHIFT},
};

#[test]
fn remapped_get_up_keys_reach_the_action_before_walking_or_driving() {
    let bound = |code, modifier| {
        vec![KeyBinding {
            action: "get_up".into(),
            scan_code: keys::dik_code(code).unwrap(),
            modifier,
        }]
    };
    for code in [KeyCode::KeyG, KeyCode::KeyW, KeyCode::Space, KeyCode::F1] {
        assert!(get_up_key(&bound(code, 0), code, false, false, false));
    }
    assert!(get_up_key(
        &bound(KeyCode::KeyG, KEY_ALT),
        KeyCode::KeyG,
        false,
        false,
        true
    ));
    assert!(!get_up_key(
        &bound(KeyCode::KeyG, KEY_ALT),
        KeyCode::KeyG,
        false,
        false,
        false
    ));
    let default = bound(KeyCode::KeyG, KEY_CTRL | KEY_SHIFT);
    assert!(get_up_key(&default, KeyCode::KeyG, true, true, false));
    assert!(
        !get_up_key(&default, KeyCode::KeyG, true, false, false),
        "Shift+G stays the seat key"
    );
    assert!(
        !get_up_key(&[], KeyCode::KeyG, false, false, false),
        "unbound G stays the seat key"
    );
    assert!(!get_up_key(
        &bound(KeyCode::KeyW, 0),
        KeyCode::KeyG,
        false,
        false,
        false
    ));
    assert!(!get_up_key(
        &[KeyBinding {
            action: "get_up".into(),
            ..Default::default()
        }],
        KeyCode::KeyG,
        false,
        false,
        false
    ));
}
