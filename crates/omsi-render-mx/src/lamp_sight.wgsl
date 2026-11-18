struct Sight {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    lamps: array<vec4<f32>, 256>,
}

@group(0) @binding(0) var<uniform> u: Sight;
@group(0) @binding(1) var t_depth: texture_depth_2d;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn covered(px: vec2<i32>, size: vec2<i32>, lamp_dist: f32) -> f32 {
    let q = clamp(px, vec2<i32>(0), size - vec2<i32>(1));
    let d = textureLoad(t_depth, q, 0);
    if (d <= 0.0) {
        return 0.0;
    }
    let uv = (vec2<f32>(q) + 0.5) / vec2<f32>(size);
    let w = u.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let dist = length(w.xyz / w.w - u.eye.xyz);
    return select(0.0, 1.0, dist < lamp_dist - (0.75 + 0.03 * lamp_dist));
}

@fragment
fn fs_sight(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let lamp = u.lamps[min(u32(pos.x), 255u)];
    let clip = u.view_proj * vec4<f32>(lamp.xyz, 1.0);
    if (lamp.w < 0.5 || clip.w <= 0.0) {
        return vec4<f32>(1.0);
    }
    let ndc = clip.xy / clip.w;
    if (abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0) {
        return vec4<f32>(1.0);
    }
    let size = vec2<i32>(textureDimensions(t_depth));
    let px = vec2<i32>(vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * vec2<f32>(size));
    let lamp_dist = length(lamp.xyz - u.eye.xyz);
    var taps = array<vec2<i32>, 5>(vec2<i32>(0, 0), vec2<i32>(3, 0), vec2<i32>(-3, 0), vec2<i32>(0, 3), vec2<i32>(0, -3));
    var hidden = 0.0;
    for (var k = 0; k < 5; k++) {
        hidden += covered(px + taps[k], size, lamp_dist);
    }
    return vec4<f32>(1.0 - hidden / 5.0, 0.0, 0.0, 1.0);
}
