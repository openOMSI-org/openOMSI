//! Tests of openOMSI's model additions and of the material order Omsi.exe's reader keeps.

use super::*;

/// Commands between a `[matl_change]` and its first `[matl_item]` change the slot's own
/// material, as Omsi.exe's reader does: an LED matrix that opens its change first keeps
/// its `[matl_alpha]` (unlit dots see-through) whether the variable picks the item or not.
#[test]
fn commands_after_a_matl_change_go_to_the_slot() {
    let text = "[mesh]\nmatrix.o3d\n[matl_change]\nled.bmp\n0\nmatrix_on\n[matl_alpha]\n2\n[useScriptTexture]\n0\n[matl_item]\n[matl_lightmap]\nled_lit.bmp\nmatrix_on\n[matl_item]\n";
    let m = Model::parse(&CfgFile::from_str("model.cfg", text));
    let mats = &m.meshes[0].materials;
    let base = mats
        .iter()
        .find(|x| !x.item && x.change.is_none())
        .expect("the slot's own material");
    assert_eq!(
        (base.texture.as_str(), base.alpha, base.alpha_set),
        ("led.bmp", 2, true)
    );
    assert_eq!(base.use_script_texture, Some(0));
    let change = mats.iter().find(|x| !x.item && x.change.is_some()).unwrap();
    assert!(
        !change.alpha_set && change.use_script_texture.is_none(),
        "{change:?}"
    );
    let items: Vec<&MaterialDef> = mats.iter().filter(|x| x.item).collect();
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|i| i.alpha == 2 && i.use_script_texture == Some(0))
    );
    assert!(items[0].lightmap.is_some());
    assert!(
        items[1].lightmap.is_none(),
        "a second item starts from the slot, not from the item before"
    );
    // a plain [matl] of the slot written before the change is the one changed
    let text = "[mesh]\nm.o3d\n[matl]\nled.bmp\n0\n[matl_envmap]\ne.bmp\n0.2\n[matl_change]\nled.bmp\n0\nv\n[matl_alpha]\n1\n[matl_item]\n";
    let m = Model::parse(&CfgFile::from_str("model.cfg", text));
    let plain: Vec<&MaterialDef> = m.meshes[0]
        .materials
        .iter()
        .filter(|x| !x.item && x.change.is_none())
        .collect();
    assert_eq!(plain.len(), 1);
    assert!(plain[0].envmap.is_some() && plain[0].alpha == 1);
}

/// `[illumination_interior]` keeps the lamps that exist, each once; `-1` means none.
#[test]
fn interior_lamp_lists_keep_existing_lamps_once() {
    let lamps = "[interiorlight]\na\n1\n255\n255\n255\n0\n0\n2\n".repeat(3);
    let text = format!(
        "{lamps}[mesh]\na.o3d\n[illumination_interior]\n0\n-1\n-1\n-1\n[mesh]\nb.o3d\n[illumination_interior]\n2\n7\n2\n1\n[mesh]\nc.o3d\n"
    );
    let m = Model::parse(&CfgFile::from_str("model.cfg", &text));
    let lists: Vec<&[i32]> = m
        .meshes
        .iter()
        .map(|x| x.illumination_interior.as_slice())
        .collect();
    assert_eq!(lists, vec![&[0][..], &[2, 1][..], &[2, 1][..]]);
    // the list a mesh has without a line: what exists of 0..3
    let m = Model::parse(&CfgFile::from_str(
        "model.cfg",
        &format!("{lamps}[mesh]\na.o3d\n"),
    ));
    assert_eq!(m.meshes[0].illumination_interior, vec![0, 1, 2]);
}

/// `[led_glow_effect]` is the slot's, kept in 0..1, and an item takes it from the slot.
#[test]
fn led_glow_effect_belongs_to_the_slot() {
    let text = "[mesh]\nm.o3d\n[matl]\nled.bmp\n0\n[led_glow_effect]\n0.4\n[matl]\nflip.bmp\n0\n[led_glow_effect]\n3\n[matl_change]\nled.bmp\n0\nv\n[matl_item]\n";
    let m = Model::parse(&CfgFile::from_str("model.cfg", text));
    let of = |t: &str, item: bool| {
        m.meshes[0]
            .materials
            .iter()
            .find(|x| x.texture == t && x.item == item && x.change.is_none() == !item)
            .and_then(|x| x.led_glow)
    };
    assert_eq!(of("led.bmp", false), Some(0.4));
    assert_eq!(of("flip.bmp", false), Some(1.0));
    assert_eq!(of("led.bmp", true), Some(0.4));
}

/// `[spotlight_cookie]`: fourteen lines, then the fade time and the yaw and pitch
/// variables where they are given.
#[test]
fn a_spotlight_cookie_reads_its_lines() {
    let full = "[spotlight_cookie]\n0.9\n5.9\n0.7\n0\n1\n-0.05\n255\n250\n240\n80\n60\n20\nbeam.png\nlights_main\n0.25\ncookie_yaw\ncookie_pitch\n\n[mesh]\na.o3d\n";
    let m = Model::parse(&CfgFile::from_str("model.cfg", full));
    let c = &m.spotlight_cookies[0];
    assert_eq!(
        (c.pos, c.dir, c.color),
        ([0.9, 5.9, 0.7], [0.0, 1.0, -0.05], [255.0, 250.0, 240.0])
    );
    assert_eq!((c.range, c.width, c.height), (80.0, 60.0, 20.0));
    assert_eq!(
        (c.picture.as_str(), c.variable.as_str(), c.fade),
        ("beam.png", "lights_main", 0.25)
    );
    assert_eq!(
        (c.yaw.as_deref(), c.pitch.as_deref()),
        (Some("cookie_yaw"), Some("cookie_pitch"))
    );
    assert!(!c.mirrored);
    assert_eq!(m.meshes.len(), 1);
    let short = "[spotlight_cookie]\n0.9\n5.9\n0.7\n0\n1\n0\n255\n255\n255\n40\n60\n20\nbeam.png\n1\n[mesh]\na.o3d\n";
    let m = Model::parse(&CfgFile::from_str("model.cfg", short));
    let c = &m.spotlight_cookies[0];
    assert_eq!(
        (c.variable.as_str(), c.fade, c.yaw.clone()),
        ("1", 0.0, None)
    );
    assert_eq!(
        m.meshes.len(),
        1,
        "the keyword after it is not taken as its line"
    );
}

/// Sobol's buses use a shorter `[spotlight_cookie]`: no colour numbers or cone angles, the
/// picture carries the colour. The flag asks for a twin, and the offsets come pitch first.
#[test]
fn a_sobol_spotlight_cookie_reads_its_range_variable_flag_and_picture() {
    let text = "[spotlight_cookie]\n0.71507\n5.82032\n0.67736\n0\n1\n0\n200\nlights_fern_front_1\n0\ncookie_fern_H4_lens.png\n0.05\nlamp_pitch\nlamp_yaw\n\n[mesh]\na.o3d\n";
    let m = Model::parse(&CfgFile::from_str("model.cfg", text));
    let c = &m.spotlight_cookies[0];
    assert_eq!(
        (c.pos, c.dir),
        ([0.71507, 5.82032, 0.67736], [0.0, 1.0, 0.0])
    );
    assert_eq!(c.color, [255.0; 3]);
    assert_eq!((c.range, c.width, c.height), (200.0, 120.0, 96.0));
    assert_eq!(
        (c.variable.as_str(), c.picture.as_str(), c.fade),
        ("lights_fern_front_1", "cookie_fern_H4_lens.png", 0.05)
    );
    assert_eq!(
        (c.pitch.as_deref(), c.yaw.as_deref()),
        (Some("lamp_pitch"), Some("lamp_yaw"))
    );
    assert!(c.mirrored);
    assert_eq!(m.meshes.len(), 1);
    let one = text.replace(
        "lights_fern_front_1\n0\ncookie",
        "lights_fern_front_1\n1\ncookie",
    );
    let m = Model::parse(&CfgFile::from_str("model.cfg", &one));
    assert!(!m.spotlight_cookies[0].mirrored);
    let short = "[spotlight_cookie]\n0.71507\n5.82032\n0.67736\n0\n1\n0\n200\nlights_fern\n0\nbeam.png\n\n[mesh]\na.o3d\n";
    let m = Model::parse(&CfgFile::from_str("model.cfg", short));
    let c = &m.spotlight_cookies[0];
    assert_eq!(
        (c.fade, c.pitch.as_deref(), c.yaw.as_deref()),
        (0.0, None, None)
    );
    assert_eq!(m.meshes.len(), 1, "the next keyword is not read as a fade");
}

/// `[light_emitter_interior]`'s lines are all optional: a blank line or the next keyword ends
/// it, a comma is a decimal point, and a free text line is a comment, not a variable.
#[test]
fn light_emitter_interior_reads_its_optional_lines() {
    let parse = |text: &str| {
        Model::parse(&CfgFile::from_str("model.cfg", text)).meshes[0]
            .light_emitter_interior
            .clone()
            .expect("the tag")
    };
    let full = parse("[mesh]\nlamps.o3d\n[light_emitter_interior]\n1,5\n6\nlights_innen\n");
    assert_eq!(
        (full.strength, full.max_lamps, full.variable.as_deref()),
        (1.5, 6, Some("lights_innen"))
    );
    let bare = parse("[mesh]\nlamps.o3d\n[light_emitter_interior]\n\n[viewpoint]\n1\n");
    assert_eq!(
        (bare.strength, bare.max_lamps, bare.variable),
        (1.0, 4, None)
    );
    let commented = parse("[mesh]\nlamps.o3d\n[light_emitter_interior]\n2\nthe ceiling strips\n");
    assert_eq!((commented.strength, commented.variable), (2.0, None));
    // (the keyword after it is read as one)
    let m = Model::parse(&CfgFile::from_str(
        "model.cfg",
        "[mesh]\nlamps.o3d\n[light_emitter_interior]\n[viewpoint]\n2\n",
    ));
    assert_eq!(m.meshes[0].viewpoint, 2);
}
