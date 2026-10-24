use super::*;

fn road_spline() -> omsi_scenery::sli::Spline {
    use omsi_scenery::sli::{SplineProfile, SplineProfilePoint};
    omsi_scenery::sli::Spline {
        profiles: vec![SplineProfile { points: vec![SplineProfilePoint { x: -4.0, ..Default::default() }, SplineProfilePoint { x: 4.0, ..Default::default() }], ..Default::default() }],
        // This editor selection width is deliberately much wider than the mesh.
        height_profiles: vec![omsi_scenery::sli::HeightProfile { x0: -40.0, x1: 40.0, ..Default::default() }],
        ..Default::default()
    }
}

#[test]
fn road_candidates_use_drawn_width_and_ignore_nonroad_assets() {
    let def = road_spline();
    assert!(road_sections("Splines/StreetLight.sli", &def).is_empty());
    assert!(road_surface_name("Splines/Fahrbahn.sli"));
    assert!(!road_surface_name("Splines/Straßenbahn.sli"));
    assert!(road_surface_name("amelys  route 6m.bmp"));
    assert!(!road_surface_name("Splines/amelys trottoir 1.sli"));
    assert_eq!(road_sections("Splines/Str_2spur_6m.sli", &def), vec![(-4.0, 4.0, 0.0)]);
    let mut only_editor = def.clone();
    only_editor.only_editor = true;
    assert!(road_sections("Splines/Str_2spur_6m.sli", &only_editor).is_empty());
    let mut asphalt = def;
    asphalt.textures.push(omsi_scenery::sli::SplineTexture { file: "Texture/asphalt_rough.bmp".into(), ..Default::default() });
    assert_eq!(road_sections("Splines/CustomCurve.sli", &asphalt), vec![(-4.0, 4.0, 0.0)]);
    assert!(road_sections("Splines/BS_Gehweg_Asphalt01_MD_04m.sli", &asphalt).is_empty());
}

#[test]
fn road_surface_bounds_keep_offsets_medians_and_unused_textures() {
    use omsi_scenery::sli::{Spline, SplineProfile, SplineProfilePoint, SplineTexture};
    let profile = |texture, lo, hi| SplineProfile { texture, points: vec![SplineProfilePoint { x: lo, ..Default::default() }, SplineProfilePoint { x: hi, ..Default::default() }] };
    let mut def = Spline {
        textures: vec![SplineTexture { file: "str_asphdrk.bmp".into(), ..Default::default() }, SplineTexture { file: "str_side1.bmp".into(), ..Default::default() }, SplineTexture { file: "gras1.bmp".into(), ..Default::default() }],
        profiles: vec![profile(1, -25.0, -18.0), profile(0, -18.0, -15.0), profile(0, -15.0, -12.0), profile(2, -12.0, 12.0), profile(0, 12.0, 18.0), profile(1, 18.0, 25.0)],
        ..Default::default()
    };
    assert_eq!(road_sections("Custom/divided.sli", &def), vec![(-18.0, -12.0, 0.0), (12.0, 18.0, 0.0)]);
    def.profiles = vec![profile(1, -25.0, 25.0)];
    assert!(road_sections("Custom/surface.sli", &def).is_empty(), "unused asphalt texture must not turn pavement into a road");
    def.profiles = vec![profile(0, -4.0, 4.0)];
    def.paths.push(omsi_scenery::PathDef { kind: 2, ..Default::default() });
    assert!(road_sections("Custom/track.sli", &def).is_empty());
}
