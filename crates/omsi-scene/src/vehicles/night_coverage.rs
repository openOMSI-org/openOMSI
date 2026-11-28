use super::*;

// Resolve the same repaint-substituted name for both the GPU texture and alpha metadata.
pub(super) fn load_night(
    ov: &[&MaterialDef],
    subst: &dyn Fn(&str) -> String,
    mut load: impl FnMut(&str) -> Option<(TextureId, bool)>,
) -> Option<(TextureId, bool)> {
    ov.iter()
        .find_map(|o| o.nightmap.as_deref())
        .and_then(|name| load(&subst(name)))
}

// Generated pictures, mirrors, rain films and freetex own their changing coverage.
// A missing diffuse picture is not a fully clear placeholder.
pub(super) fn static_diffuse(
    tex: Option<TextureId>,
    freetex: bool,
    rain: bool,
    name: &str,
) -> bool {
    tex.is_some() && !freetex && !rain && mirror_index(name).is_none()
}

// `night_has_alpha` is true only for a successfully loaded map whose decoded metadata
// reports alpha. RGB maps sample alpha 1 and would turn a clear quad into a solid rectangle.
pub(super) fn night_coverage(
    alpha: AlphaMode,
    clear: bool,
    static_diffuse: bool,
    night_has_alpha: bool,
    mask: bool,
    (text, script): (Option<usize>, Option<usize>),
) -> bool {
    alpha == AlphaMode::Blend
        && clear
        && static_diffuse
        && night_has_alpha
        && !mask
        && text.is_none()
        && script.is_none()
}

// A plain vehicle night map follows the night unless something switches the surface itself.
// `[visible]` meshes (dashboard warning lamps, the stop-requested sign) and slots fed by a
// script's text or texture are drawn only while their own switch is on: their night map is
// their lit picture and glows by day as well (#497). A static instrument panel fades with the
// night instead (#3195).
pub(super) fn night_map_switched(
    def: &MeshDef,
    text_slot: Option<usize>,
    script_slot: Option<usize>,
) -> bool {
    def.visible.is_some()
        || def.visible_groups().next().is_some()
        || text_slot.is_some()
        || script_slot.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_requires_a_loaded_alpha_night_map_and_clear_blended_diffuse() {
        for (label, mode, clear, loaded, night_alpha, mask, expected) in [
            (
                "alpha night",
                AlphaMode::Blend,
                true,
                true,
                true,
                false,
                true,
            ),
            (
                "RGB night",
                AlphaMode::Blend,
                true,
                true,
                false,
                false,
                false,
            ),
            (
                "ordinary diffuse",
                AlphaMode::Blend,
                false,
                true,
                true,
                false,
                false,
            ),
            (
                "missing diffuse",
                AlphaMode::Blend,
                true,
                false,
                true,
                false,
                false,
            ),
            (
                "explicit transmap",
                AlphaMode::Blend,
                true,
                true,
                true,
                true,
                false,
            ),
            (
                "alpha test",
                AlphaMode::Test,
                true,
                true,
                true,
                false,
                false,
            ),
            ("opaque", AlphaMode::Opaque, true, true, true, false, false),
        ] {
            assert_eq!(
                night_coverage(mode, clear, loaded, night_alpha, mask, (None, None)),
                expected,
                "{label}"
            );
        }
    }

    #[test]
    fn changing_diffuse_slots_keep_their_own_coverage() {
        for (label, freetex, rain, name, text, script, expected) in [
            ("static", false, false, "lamp.tga", None, None, true),
            ("freetex", true, false, "lamp.tga", None, None, false),
            ("rain layer", false, true, "lamp.tga", None, None, false),
            ("mirror", false, false, "reflexion1.bmp", None, None, false),
            (
                "text texture",
                false,
                false,
                "lamp.tga",
                Some(0),
                None,
                false,
            ),
            (
                "script texture",
                false,
                false,
                "lamp.tga",
                None,
                Some(0),
                false,
            ),
        ] {
            let eligible = static_diffuse(Some(0), freetex, rain, name);
            assert_eq!(
                night_coverage(
                    AlphaMode::Blend,
                    true,
                    eligible,
                    true,
                    false,
                    (text, script)
                ),
                expected,
                "{label}"
            );
        }
    }

    #[test]
    fn material_items_use_the_selected_maps_alpha_metadata() {
        // An RGB base must not suppress an alpha item's artwork, nor may an RGB item
        // inherit an alpha base's coverage. A missing item map inherits the entire pair.
        let base = Some((0, false));
        let alpha_item = Some((1, true));
        let rgb_item = Some((2, false));
        for (base, item, expected) in [
            (base, alpha_item, true),
            (alpha_item, rgb_item, false),
            (alpha_item, None, true),
            (None, None, false),
        ] {
            let selected = item.or(base);
            assert_eq!(
                night_coverage(
                    AlphaMode::Blend,
                    true,
                    true,
                    selected.is_some_and(|(_, a)| a),
                    false,
                    (None, None)
                ),
                expected
            );
        }
    }

    #[test]
    fn night_map_metadata_follows_repaint_substitution_and_missing_items() {
        let base = MaterialDef {
            nightmap: Some("lamp.tga".into()),
            ..Default::default()
        };
        let item = MaterialDef {
            nightmap: Some("rgb.bmp".into()),
            ..Default::default()
        };
        let subst = |s: &str| {
            if s == "lamp.tga" {
                "repaint.tga".into()
            } else {
                s.into()
            }
        };
        let load = |s: &str| match s {
            "repaint.tga" => Some((0, true)),
            "rgb.bmp" => Some((1, false)),
            _ => None,
        };
        let base = load_night(&[&base], &subst, load);
        assert_eq!(base, Some((0, true)));
        assert_eq!(
            load_night(&[&item], &subst, load).or(base),
            Some((1, false))
        );
        assert_eq!(load_night(&[], &subst, load).or(base), base);
        assert_eq!(load_night(&[&item], &subst, |_| None).or(base), base);
    }

    #[test]
    fn only_switched_meshes_glow_by_day() {
        let plain = MeshDef::default();
        let empty_visible_group = MeshDef {
            visible_any: vec![Vec::new()],
            ..Default::default()
        };
        let visible_mesh = MeshDef {
            visible: Some(("cockpit_light".into(), 1.0)),
            ..Default::default()
        };
        let visible_any_mesh = MeshDef {
            visible_any: vec![vec![omsi_model::VisibleCond {
                variable: "stop_requested".into(),
                lo: 1.0,
                ..Default::default()
            }]],
            ..Default::default()
        };
        assert!(!night_map_switched(&plain, None, None));
        assert!(!night_map_switched(&empty_visible_group, None, None));
        assert!(night_map_switched(&visible_mesh, None, None));
        assert!(night_map_switched(&visible_any_mesh, None, None));
        assert!(night_map_switched(&plain, Some(0), None));
        assert!(night_map_switched(&plain, None, Some(0)));
    }
}
