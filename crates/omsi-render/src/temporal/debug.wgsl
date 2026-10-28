struct Params { size_mode: vec4<f32> };
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var vectors: texture_2d<f32>;
@group(0) @binding(2) var validity: texture_2d<f32>;
@group(0) @binding(3) var depth: texture_depth_2d;

@vertex
fn vs_debug(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32(i32(i & 1u) * 4 - 1), f32(i32(i >> 1u) * 4 - 1), 0.0, 1.0);
}

@fragment
fn fs_debug(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(vectors));
    let px = clamp(vec2<i32>(pos.xy / p.size_mode.xy * vec2<f32>(dims)), vec2<i32>(0), dims - vec2<i32>(1));
    let z = textureLoad(depth, px, 0);
    // The sky has no geometry history yet, and is explicitly invalid.
    if (p.size_mode.z > 1.5 && p.size_mode.z < 2.5) {
        return vec4<f32>(vec3<f32>(pow(z, 0.25)), 1.0);
    }
    let valid = textureLoad(validity, px, 0).r;
    if (p.size_mode.z > 2.5) {
        return vec4<f32>(mix(vec3<f32>(1.0, 0.0, 1.0), vec3<f32>(0.0, 0.5, 0.0), valid), 1.0);
    }
    if (valid < 0.5) {
        return vec4<f32>(0.25, 0.0, 0.25, 1.0);
    }
    let v = textureLoad(vectors, px, 0).rg;
    // Neutral grey = stationary; red/green encode signed screen X/Y, 16px = full range.
    return vec4<f32>(clamp(vec2<f32>(0.5) + v / 32.0, vec2<f32>(0.0), vec2<f32>(1.0)), 0.5, 1.0);
}
