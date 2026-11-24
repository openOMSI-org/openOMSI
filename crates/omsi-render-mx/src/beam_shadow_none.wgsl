// `beam_shadow.wgsl` for the shaders without the lamp shadow tiles: nothing stands in a beam.
fn beam_tile(li: u32, l: PointLight) -> u32 {
    return 0xffffffffu;
}

fn beam_reaches(k: u32, x: vec3<f32>) -> f32 {
    return 1.0;
}
