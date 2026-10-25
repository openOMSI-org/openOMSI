use super::*;

/// The fog composite's gather (`gathered_air_light`) skips its weighted sum only when none of
/// the sixteen half-size rays of the tent holds light: an outer ray lit with the four
/// nearest dark still lights the pixel (the reviewer's case: f = 0.25, the ray one to the
/// left lit, every depth alike - the tent gives about 0.082).
#[test]
fn the_fog_gather_skips_only_when_all_sixteen_rays_are_dark() {
    // the tent of the shader, on the rays' light: (offset, light) -> the weighted mean
    let tent = |f: (f32, f32), light: &dyn Fn(i32, i32) -> f32| -> f32 {
        let (mut sum, mut wsum) = (0.0f32, 0.0f32);
        for k in 0..16 {
            let (ox, oy) = ((k & 3) - 1, (k >> 2) - 1);
            let bil = (1.0 - (ox as f32 - f.0).abs() * 0.5).max(0.0) * (1.0 - (oy as f32 - f.1).abs() * 0.5).max(0.0);
            let w = bil + 1e-3;
            sum += light(ox, oy) * w;
            wsum += w;
        }
        sum / wsum
    };
    let edge = tent((0.25, 0.25), &|x, y| if (x, y) == (-1, 0) { 1.0 } else { 0.0 });
    assert!((edge - 0.082).abs() < 0.01, "{edge}");
    assert_eq!(tent((0.25, 0.25), &|_, _| 0.0), 0.0);
    // the shader scans every one of the sixteen taps before it returns zero
    let src = include_str!("../fog_lamps.wgsl");
    let gather = &src[src.find("fn gathered_air_light").unwrap()..];
    let scan = &gather[..gather.find("if (!lit)").unwrap()];
    assert_eq!(scan.matches("for (var k = 0; k < 16;").count(), 1);
    assert!(scan.contains("lit = lit || any(textureLoad(t_fog, clamp(base + o,"), "{scan}");
    assert!(!scan.contains("2e-4"), "no threshold: any light at all keeps the sum");
}
